use super::linker::GraphLinkFailure;
use a3_domain::{
    Confidence, GraphEndpoint, GraphSymbol, IndexLanguage, LinkResolution, RepositoryFileState,
    RepositoryPath, SymbolId, SymbolKind, SymbolReference, SyntaxRelationKind,
    UnresolvedGraphTarget, UnresolvedReason,
};
use std::collections::{BTreeMap, BTreeSet};

const MODULE_CONFIDENCE_CAP: u16 = 9_500;
const FILE_LOCAL_CONFIDENCE_CAP: u16 = 9_000;
const QUALIFIED_CONFIDENCE_CAP: u16 = 8_500;
const JAVASCRIPT_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

pub(super) enum ResolutionOutcome {
    Resolved {
        endpoint: GraphEndpoint,
        resolution: LinkResolution,
        confidence_cap: Confidence,
    },
    Unresolved {
        target: UnresolvedGraphTarget,
        reason: UnresolvedReason,
    },
}

type ScopedCallable = (SymbolId, a3_domain::SourceRange, a3_domain::SourceRange);

pub(super) struct ResolutionIndexes {
    callables: BTreeMap<(RepositoryPath, String), Vec<ScopedCallable>>,
    shadowing:
        BTreeMap<(RepositoryPath, String), Vec<(a3_domain::SourceRange, a3_domain::SourceRange)>>,
    callable_visibility: BTreeMap<SymbolId, a3_domain::SymbolVisibility>,
    files: BTreeSet<RepositoryPath>,
    symbols_by_file_name: BTreeMap<(RepositoryPath, String), Vec<SymbolId>>,
    symbols_by_name: BTreeMap<String, Vec<(RepositoryPath, SymbolId)>>,
    python_modules: BTreeMap<String, Vec<RepositoryPath>>,
}

impl ResolutionIndexes {
    pub(super) fn new(
        files: &RepositoryFileState,
        symbols: &[GraphSymbol],
        parses: &[a3_domain::LanguageParseResult],
    ) -> Result<Self, GraphLinkFailure> {
        let files = files
            .revisions()
            .iter()
            .map(|revision| revision.path().clone())
            .collect::<BTreeSet<_>>();
        let mut symbols_by_file_name = BTreeMap::<_, Vec<_>>::new();
        let mut symbols_by_name = BTreeMap::<_, Vec<_>>::new();
        for symbol in symbols {
            if symbol.parsed().kind() == SymbolKind::Implementation {
                continue;
            }
            let path = symbol.revision().path().clone();
            let name = symbol.parsed().name().as_str().to_owned();
            symbols_by_file_name
                .entry((path.clone(), name.clone()))
                .or_default()
                .push(symbol.id());
            symbols_by_name
                .entry(name)
                .or_default()
                .push((path, symbol.id()));
        }
        for values in symbols_by_file_name.values_mut() {
            values.sort();
            values.dedup();
        }
        for values in symbols_by_name.values_mut() {
            values.sort();
            values.dedup();
        }

        let mut python_modules = BTreeMap::<_, Vec<_>>::new();
        for path in &files {
            for alias in python_module_aliases(path, &files) {
                python_modules.entry(alias).or_default().push(path.clone());
            }
        }
        for values in python_modules.values_mut() {
            values.sort();
            values.dedup();
        }

        let by_local = symbols
            .iter()
            .map(|s| ((s.revision().path(), s.parsed().id()), s))
            .collect::<BTreeMap<_, _>>();
        let mut callables = BTreeMap::<_, Vec<_>>::new();
        let mut shadowing = BTreeMap::<_, Vec<_>>::new();
        let mut callable_visibility = BTreeMap::new();
        for parse in parses {
            for flow in parse.function_flows() {
                let owner = by_local
                    .get(&(parse.revision().path(), flow.owner()))
                    .ok_or(GraphLinkFailure::InvalidInput)?;
                if matches!(
                    owner.parsed().kind(),
                    SymbolKind::Function | SymbolKind::Method
                ) {
                    callables
                        .entry((
                            parse.revision().path().clone(),
                            owner.parsed().name().as_str().to_owned(),
                        ))
                        .or_default()
                        .push((
                            owner.id(),
                            flow.lexical_scope(),
                            owner.parsed().selection_range(),
                        ));
                    callable_visibility.insert(owner.id(), owner.parsed().visibility());
                }
                for value in flow.values().iter().filter(|v| {
                    matches!(
                        v.kind,
                        a3_domain::FlowValueKind::Parameter | a3_domain::FlowValueKind::Local
                    )
                }) {
                    shadowing
                        .entry((
                            parse.revision().path().clone(),
                            value.name.as_str().to_owned(),
                        ))
                        .or_default()
                        .push((value.scope, value.range));
                }
            }
        }
        Ok(Self {
            callables,
            shadowing,
            callable_visibility,
            files,
            symbols_by_file_name,
            symbols_by_name,
            python_modules,
        })
    }

    pub(super) fn contains_file(&self, path: &RepositoryPath) -> bool {
        self.files.contains(path)
    }

    pub(super) fn resolve_call(
        &self,
        parse: &a3_domain::LanguageParseResult,
        relation: &a3_domain::SyntaxRelation,
        reference: &SymbolReference,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let name = reference.as_str();
        let path = parse.revision().path();
        let at = relation.evidence_range();
        let (base, suffix) = name
            .split_once('.')
            .or_else(|| name.split_once("::"))
            .map_or((name, None), |(b, s)| (b, Some(s)));
        let candidates = self.callables.get(&(path.clone(), base.to_owned()));
        let shadowed = self
            .shadowing
            .get(&(path.clone(), base.to_owned()))
            .is_some_and(|bindings| {
                bindings.iter().any(|(scope, definition)| {
                    scope.contains(at)
                        && (parse.language() != IndexLanguage::Rust
                            || definition.start_byte() < at.start_byte())
                        && !candidates.is_some_and(|c| {
                            c.iter()
                                .any(|(_, s, r)| s.contains(at) && r.contains(*definition))
                        })
                })
            });
        if shadowed {
            return unresolved_reference(name, UnresolvedReason::DynamicReference);
        }
        if suffix.is_none() {
            let mut eligible = candidates
                .into_iter()
                .flatten()
                .filter(|(_, scope, _)| scope.contains(at))
                .collect::<Vec<_>>();
            eligible.sort_by_key(|(_, scope, _)| scope.len());
            if let Some((id, scope, _)) = eligible.first() {
                if eligible
                    .get(1)
                    .is_some_and(|(_, next, _)| next.len() == scope.len())
                {
                    return ambiguous_reference(name);
                }
                return resolved_symbol(
                    *id,
                    LinkResolution::UniqueFileLocalName,
                    FILE_LOCAL_CONFIDENCE_CAP,
                );
            }
        }
        let mut imports = parse
            .static_imports()
            .iter()
            .filter(|i| i.local.as_str() == base && i.scope.contains(at))
            .collect::<Vec<_>>();
        imports.sort_by_key(|i| i.scope.len());
        if let Some(import) = imports.first() {
            if imports
                .get(1)
                .is_some_and(|i| i.scope.len() == import.scope.len())
            {
                return ambiguous_reference(name);
            }
            if parse.language() == IndexLanguage::Rust {
                let qualified = suffix.map_or_else(
                    || import.module.as_str().to_owned(),
                    |s| format!("{}::{s}", import.module.as_str()),
                );
                if let Some(outcome) =
                    self.resolve_rust(path, &qualified, SyntaxRelationKind::Calls)?
                {
                    return Ok(outcome);
                }
            } else {
                let export = match (&import.export, suffix) {
                    (Some(e), None) => Some(e.as_str()),
                    (None, Some(s)) if is_simple_identifier(s) => Some(s),
                    _ => None,
                };
                if let Some(export) = export {
                    let module = match parse.language() {
                        IndexLanguage::TypeScriptJavaScript => self.resolve_javascript(
                            path,
                            import.module.as_str(),
                            SyntaxRelationKind::Imports,
                        )?,
                        IndexLanguage::Python => self.resolve_python(
                            path,
                            import.module.as_str(),
                            SyntaxRelationKind::Imports,
                        )?,
                        _ => None,
                    };
                    if let Some(ResolutionOutcome::Resolved {
                        endpoint: GraphEndpoint::File(file),
                        ..
                    }) = module
                    {
                        let ids = self
                            .callables
                            .get(&(file, export.to_owned()))
                            .into_iter()
                            .flatten()
                            .filter(|(id, scope, _)| {
                                scope.start_byte() == 0
                                    && (parse.language() != IndexLanguage::TypeScriptJavaScript
                                        || self.callable_visibility.get(id)
                                            == Some(&a3_domain::SymbolVisibility::Public))
                            })
                            .map(|(id, _, _)| GraphEndpoint::Symbol(*id))
                            .collect();
                        return canonical_resolution(ids, name);
                    }
                }
            }
            return unresolved_reference(name, UnresolvedReason::NoDeterministicMatch);
        }
        // Never fall back to a file-wide simple name when lexical metadata says otherwise.
        if suffix.is_none() && !parse.function_flows().is_empty() {
            return unresolved_reference(name, UnresolvedReason::NoDeterministicMatch);
        }
        self.resolve(parse.language(), path, reference, SyntaxRelationKind::Calls)
    }

    pub(super) fn resolve(
        &self,
        language: IndexLanguage,
        source_path: &RepositoryPath,
        reference: &SymbolReference,
        kind: SyntaxRelationKind,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let value = reference.as_str();
        if allows_file_local_name(kind)
            && is_simple_identifier(value)
            && let Some(outcome) = self.unique_symbol_in_file(source_path, value)?
        {
            return Ok(outcome);
        }

        let language_outcome = match language {
            IndexLanguage::TypeScriptJavaScript => {
                self.resolve_javascript(source_path, value, kind)?
            }
            IndexLanguage::Python => self.resolve_python(source_path, value, kind)?,
            IndexLanguage::Rust => self.resolve_rust(source_path, value, kind)?,
            IndexLanguage::Generic => None,
        };
        if let Some(outcome) = language_outcome {
            return Ok(outcome);
        }

        if matches!(
            kind,
            SyntaxRelationKind::Extends | SyntaxRelationKind::Implements
        ) && is_simple_identifier(value)
            && let Some(outcome) = self.unique_global_symbol(value)?
        {
            return Ok(outcome);
        }

        Ok(ResolutionOutcome::Unresolved {
            target: UnresolvedGraphTarget::Reference(reference.clone()),
            reason: unresolved_reason(value, kind),
        })
    }

    fn unique_symbol_in_file(
        &self,
        path: &RepositoryPath,
        name: &str,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        let Some(symbols) = self
            .symbols_by_file_name
            .get(&(path.clone(), name.to_owned()))
        else {
            return Ok(None);
        };
        match symbols.as_slice() {
            [id] => Ok(Some(resolved_symbol(
                *id,
                LinkResolution::UniqueFileLocalName,
                FILE_LOCAL_CONFIDENCE_CAP,
            )?)),
            [] => Ok(None),
            _ => Ok(Some(ambiguous_reference(name)?)),
        }
    }

    fn unique_global_symbol(
        &self,
        name: &str,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        let Some(symbols) = self.symbols_by_name.get(name) else {
            return Ok(None);
        };
        match symbols.as_slice() {
            [(_, id)] => Ok(Some(resolved_symbol(
                *id,
                LinkResolution::UniqueQualifiedName,
                QUALIFIED_CONFIDENCE_CAP,
            )?)),
            [] => Ok(None),
            _ => Ok(Some(ambiguous_reference(name)?)),
        }
    }

    fn resolve_javascript(
        &self,
        source_path: &RepositoryPath,
        value: &str,
        kind: SyntaxRelationKind,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        if !matches!(
            kind,
            SyntaxRelationKind::Imports
                | SyntaxRelationKind::Exports
                | SyntaxRelationKind::Builds
                | SyntaxRelationKind::Tests
                | SyntaxRelationKind::Configures
        ) || !(value.starts_with("./") || value.starts_with("../"))
        {
            return Ok(None);
        }
        let Some(base) = normalized_relative_path(source_path, value) else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        if has_file_extension(value) {
            candidates.push(base);
        } else {
            candidates.push(base.clone());
            let base_text = repository_path_text(&base);
            if let Some(base_text) = base_text {
                for extension in JAVASCRIPT_EXTENSIONS {
                    add_path_candidate(&mut candidates, &format!("{base_text}.{extension}"));
                    add_path_candidate(&mut candidates, &format!("{base_text}/index.{extension}"));
                }
            }
        }
        Ok(Some(self.resolve_file_candidates(candidates, value)?))
    }

    fn resolve_python(
        &self,
        source_path: &RepositoryPath,
        value: &str,
        kind: SyntaxRelationKind,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        if kind == SyntaxRelationKind::Calls {
            return Ok(None);
        }
        if value.starts_with('.') {
            return self.resolve_relative_python(source_path, value).map(Some);
        }
        if let Some((module, symbol)) = value.split_once(':') {
            return self
                .resolve_python_module_symbol(module, Some(symbol), value)
                .map(Some);
        }
        if matches!(
            kind,
            SyntaxRelationKind::Imports
                | SyntaxRelationKind::Exports
                | SyntaxRelationKind::Configures
                | SyntaxRelationKind::Extends
                | SyntaxRelationKind::Implements
        ) && let Some(outcome) = self.resolve_python_dotted(value)?
        {
            return Ok(Some(outcome));
        }
        Ok(None)
    }

    fn resolve_relative_python(
        &self,
        source_path: &RepositoryPath,
        value: &str,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let leading = value
            .chars()
            .take_while(|character| *character == '.')
            .count();
        let suffix = &value[leading..];
        if suffix.is_empty() {
            return unresolved_reference(value, UnresolvedReason::DynamicReference);
        }
        let Some(mut base) = source_parent_components(source_path) else {
            return unresolved_reference(value, UnresolvedReason::NoDeterministicMatch);
        };
        for _ in 1..leading {
            if base.pop().is_none() {
                return unresolved_reference(value, UnresolvedReason::NoDeterministicMatch);
            }
        }
        let parts = suffix.split('.').collect::<Vec<_>>();
        self.resolve_module_parts(&base, &parts, value, "py", Some("__init__.py"))
    }

    fn resolve_python_dotted(
        &self,
        value: &str,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        if let Some(paths) = self.python_modules.get(value) {
            return self.resolve_module_paths(paths, None, value).map(Some);
        }
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() < 2 {
            return Ok(None);
        }
        for split in (1..parts.len()).rev() {
            let module = parts[..split].join(".");
            let symbol = parts[split..].join(".");
            if !is_simple_identifier(&symbol) {
                continue;
            }
            if let Some(paths) = self.python_modules.get(&module) {
                return self
                    .resolve_module_paths(paths, Some(&symbol), value)
                    .map(Some);
            }
        }
        Ok(None)
    }

    fn resolve_python_module_symbol(
        &self,
        module: &str,
        symbol: Option<&str>,
        original: &str,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let Some(paths) = self.python_modules.get(module) else {
            return unresolved_reference(original, UnresolvedReason::NoDeterministicMatch);
        };
        self.resolve_module_paths(paths, symbol, original)
    }

    fn resolve_rust(
        &self,
        source_path: &RepositoryPath,
        value: &str,
        kind: SyntaxRelationKind,
    ) -> Result<Option<ResolutionOutcome>, GraphLinkFailure> {
        if !matches!(
            kind,
            SyntaxRelationKind::Imports
                | SyntaxRelationKind::Exports
                | SyntaxRelationKind::Calls
                | SyntaxRelationKind::Extends
                | SyntaxRelationKind::Implements
        ) {
            return Ok(None);
        }
        let reference = value.split_once(" as ").map_or(value, |(target, _)| target);
        if reference.contains('*') || reference.ends_with('!') {
            return Ok(None);
        }
        let mut parts = reference
            .split("::")
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 2 {
            if kind == SyntaxRelationKind::Imports
                && is_simple_identifier(reference)
                && let Some(base) = rust_crate_source_root(source_path)
            {
                return self
                    .resolve_module_parts(&base, &[reference], value, "rs", Some("mod.rs"))
                    .map(Some);
            }
            return Ok(None);
        }
        let Some(mut base) = rust_resolution_base(source_path, &mut parts) else {
            return Ok(None);
        };
        if parts.first() == Some(&"super") {
            while parts.first() == Some(&"super") {
                parts.remove(0);
                if base.pop().is_none() {
                    return Ok(None);
                }
            }
        }
        if parts.is_empty() {
            return Ok(None);
        }
        self.resolve_module_parts(&base, &parts, value, "rs", Some("mod.rs"))
            .map(Some)
    }

    fn resolve_module_parts(
        &self,
        base: &[String],
        parts: &[&str],
        original: &str,
        extension: &str,
        module_file: Option<&str>,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let mut outcomes = Vec::new();
        for split in (1..=parts.len()).rev() {
            let mut module = base.to_vec();
            module.extend(parts[..split].iter().map(|part| (*part).to_owned()));
            let mut file_text = module.join("/");
            file_text.push('.');
            file_text.push_str(extension);
            let mut file_candidates = Vec::new();
            add_path_candidate(&mut file_candidates, &file_text);
            if let Some(module_file) = module_file {
                add_path_candidate(
                    &mut file_candidates,
                    &format!("{}/{module_file}", module.join("/")),
                );
            }
            for path in file_candidates {
                if !self.files.contains(&path) {
                    continue;
                }
                let remaining = &parts[split..];
                if remaining.is_empty() {
                    outcomes.push(GraphEndpoint::File(path));
                } else if remaining.len() == 1
                    && is_simple_identifier(remaining[0])
                    && let Some(ids) = self
                        .symbols_by_file_name
                        .get(&(path.clone(), remaining[0].to_owned()))
                {
                    outcomes.extend(ids.iter().copied().map(GraphEndpoint::Symbol));
                }
            }
        }
        canonical_resolution(outcomes, original)
    }

    fn resolve_module_paths(
        &self,
        paths: &[RepositoryPath],
        symbol: Option<&str>,
        original: &str,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let mut outcomes = Vec::new();
        for path in paths {
            if let Some(symbol) = symbol {
                if let Some(ids) = self
                    .symbols_by_file_name
                    .get(&(path.clone(), symbol.to_owned()))
                {
                    outcomes.extend(ids.iter().copied().map(GraphEndpoint::Symbol));
                }
            } else {
                outcomes.push(GraphEndpoint::File(path.clone()));
            }
        }
        canonical_resolution(outcomes, original)
    }

    fn resolve_file_candidates(
        &self,
        candidates: Vec<RepositoryPath>,
        original: &str,
    ) -> Result<ResolutionOutcome, GraphLinkFailure> {
        let outcomes = candidates
            .into_iter()
            .filter(|path| self.files.contains(path))
            .map(GraphEndpoint::File)
            .collect::<Vec<_>>();
        canonical_resolution(outcomes, original)
    }
}

fn canonical_resolution(
    mut outcomes: Vec<GraphEndpoint>,
    original: &str,
) -> Result<ResolutionOutcome, GraphLinkFailure> {
    outcomes.sort();
    outcomes.dedup();
    match outcomes.as_slice() {
        [endpoint] => Ok(ResolutionOutcome::Resolved {
            endpoint: endpoint.clone(),
            resolution: LinkResolution::ExactModuleReference,
            confidence_cap: confidence(MODULE_CONFIDENCE_CAP)?,
        }),
        [] => unresolved_reference(original, UnresolvedReason::NoDeterministicMatch),
        _ => unresolved_reference(original, UnresolvedReason::AmbiguousMatch),
    }
}

fn resolved_symbol(
    id: SymbolId,
    resolution: LinkResolution,
    cap: u16,
) -> Result<ResolutionOutcome, GraphLinkFailure> {
    Ok(ResolutionOutcome::Resolved {
        endpoint: GraphEndpoint::Symbol(id),
        resolution,
        confidence_cap: confidence(cap)?,
    })
}

fn ambiguous_reference(value: &str) -> Result<ResolutionOutcome, GraphLinkFailure> {
    unresolved_reference(value, UnresolvedReason::AmbiguousMatch)
}

fn unresolved_reference(
    value: &str,
    reason: UnresolvedReason,
) -> Result<ResolutionOutcome, GraphLinkFailure> {
    let reference = SymbolReference::try_from_string(value.to_owned())
        .map_err(|_| GraphLinkFailure::InvalidInput)?;
    Ok(ResolutionOutcome::Unresolved {
        target: UnresolvedGraphTarget::Reference(reference),
        reason,
    })
}

fn confidence(value: u16) -> Result<Confidence, GraphLinkFailure> {
    Confidence::from_basis_points(value).map_err(|_| GraphLinkFailure::InvalidGraph)
}

fn allows_file_local_name(kind: SyntaxRelationKind) -> bool {
    matches!(
        kind,
        SyntaxRelationKind::Exports
            | SyntaxRelationKind::Calls
            | SyntaxRelationKind::Implements
            | SyntaxRelationKind::Extends
            | SyntaxRelationKind::Reads
            | SyntaxRelationKind::Writes
            | SyntaxRelationKind::Documents
    )
}

fn is_simple_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn unresolved_reason(value: &str, kind: SyntaxRelationKind) -> UnresolvedReason {
    if value.contains(['*', '?', '[', ']', '{', '}'])
        || (kind == SyntaxRelationKind::Calls && !is_simple_identifier(value))
    {
        UnresolvedReason::DynamicReference
    } else {
        UnresolvedReason::NoDeterministicMatch
    }
}

fn normalized_relative_path(source: &RepositoryPath, reference: &str) -> Option<RepositoryPath> {
    if reference.contains('\\') || reference.contains('\0') {
        return None;
    }
    let mut components = source_parent_components(source)?;
    for component in reference.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            value if value.contains(':') => return None,
            value => components.push(value.to_owned()),
        }
    }
    repository_path_from_components(&components)
}

fn source_parent_components(path: &RepositoryPath) -> Option<Vec<String>> {
    let text = repository_path_text(path)?;
    let mut components = text.split('/').map(str::to_owned).collect::<Vec<_>>();
    components.pop()?;
    Some(components)
}

fn repository_path_text(path: &RepositoryPath) -> Option<&str> {
    std::str::from_utf8(path.as_bytes()).ok()
}

fn repository_path_from_components(components: &[String]) -> Option<RepositoryPath> {
    if components.is_empty() {
        return None;
    }
    RepositoryPath::try_from_bytes(components.join("/").into_bytes()).ok()
}

fn add_path_candidate(candidates: &mut Vec<RepositoryPath>, value: &str) {
    if let Ok(path) = RepositoryPath::try_from_bytes(value.as_bytes().to_vec()) {
        candidates.push(path);
    }
}

fn has_file_extension(value: &str) -> bool {
    value
        .rsplit('/')
        .next()
        .is_some_and(|name| name.rsplit_once('.').is_some())
}

fn python_module_aliases(path: &RepositoryPath, files: &BTreeSet<RepositoryPath>) -> Vec<String> {
    let Some(text) = repository_path_text(path) else {
        return Vec::new();
    };
    let Some((without_extension, extension)) = text.rsplit_once('.') else {
        return Vec::new();
    };
    if !matches!(extension, "py" | "pyi") {
        return Vec::new();
    }
    let mut components = without_extension.split('/').collect::<Vec<_>>();
    if components.last() == Some(&"__init__") {
        components.pop();
    }
    if components.is_empty() {
        return Vec::new();
    }
    let mut aliases = BTreeSet::new();
    aliases.insert(components.join("."));

    if let Some(index) = components
        .iter()
        .rposition(|component| matches!(*component, "src" | "python" | "lib"))
        && index + 1 < components.len()
    {
        aliases.insert(components[index + 1..].join("."));
    }

    let parent_count = if text.ends_with("/__init__.py") || text.ends_with("/__init__.pyi") {
        components.len()
    } else {
        components.len().saturating_sub(1)
    };
    let mut package_start = parent_count;
    while package_start > 0 {
        let directory = components[..package_start].join("/");
        let init_py =
            RepositoryPath::try_from_bytes(format!("{directory}/__init__.py").into_bytes());
        let init_pyi =
            RepositoryPath::try_from_bytes(format!("{directory}/__init__.pyi").into_bytes());
        let is_package = init_py.as_ref().is_ok_and(|value| files.contains(value))
            || init_pyi.as_ref().is_ok_and(|value| files.contains(value));
        if !is_package {
            break;
        }
        package_start = package_start.saturating_sub(1);
    }
    let package_start = package_start.min(parent_count);
    if package_start < components.len() {
        aliases.insert(components[package_start..].join("."));
    }
    aliases
        .into_iter()
        .filter(|alias| !alias.is_empty())
        .collect()
}

fn rust_crate_source_root(path: &RepositoryPath) -> Option<Vec<String>> {
    let text = repository_path_text(path)?;
    let components = text.split('/').collect::<Vec<_>>();
    let src = components
        .iter()
        .rposition(|component| *component == "src")?;
    Some(
        components[..=src]
            .iter()
            .map(|component| (*component).to_owned())
            .collect(),
    )
}

fn rust_resolution_base(
    source_path: &RepositoryPath,
    parts: &mut Vec<&str>,
) -> Option<Vec<String>> {
    let root = rust_crate_source_root(source_path)?;
    match parts.first().copied() {
        Some("crate") => {
            parts.remove(0);
            Some(root)
        }
        Some("self") => {
            parts.remove(0);
            rust_current_module_directory(source_path)
        }
        Some("super") => rust_current_module_directory(source_path),
        _ => Some(root),
    }
}

fn rust_current_module_directory(path: &RepositoryPath) -> Option<Vec<String>> {
    let text = repository_path_text(path)?;
    let mut components = text.split('/').map(str::to_owned).collect::<Vec<_>>();
    let file = components.pop()?;
    if !matches!(file.as_str(), "lib.rs" | "main.rs" | "mod.rs") {
        let stem = file.strip_suffix(".rs")?;
        components.push(stem.to_owned());
    }
    Some(components)
}

#[cfg(test)]
mod tests {
    use super::{ResolutionIndexes, ResolutionOutcome};
    use crate::GraphLinkFailure;
    use a3_domain::{
        ContentHash, FileRevision, GraphSymbol, IndexLanguage, LocalSymbolId, ParsedSymbol,
        RepositoryFileState, RepositoryPath, SourcePosition, SourceRange, SymbolId, SymbolKind,
        SymbolName, SymbolReference, SyntaxRelationKind, UnresolvedReason,
    };

    #[test]
    fn ambiguous_global_names_remain_candidates() -> Result<(), Box<dyn std::error::Error>> {
        let first = revision("src/first.rs", 1)?;
        let second = revision("src/second.rs", 2)?;
        let source = revision("src/source.rs", 5)?;
        let files = RepositoryFileState::new(vec![first.clone(), second.clone(), source.clone()])?;
        let indexes = ResolutionIndexes::new(
            &files,
            &[
                symbol(SymbolId::from_bytes([3; 32]), first.clone())?,
                symbol(SymbolId::from_bytes([4; 32]), second)?,
            ],
            &[],
        )?;
        let reference = SymbolReference::try_from_string("Base".to_owned())?;
        assert!(matches!(
            indexes.resolve(
                IndexLanguage::Generic,
                source.path(),
                &reference,
                SyntaxRelationKind::Extends,
            )?,
            ResolutionOutcome::Unresolved {
                reason: UnresolvedReason::AmbiguousMatch,
                ..
            }
        ));
        Ok(())
    }

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    fn symbol(id: SymbolId, revision: FileRevision) -> Result<GraphSymbol, GraphLinkFailure> {
        let range = SourceRange::new(0, 0, SourcePosition::new(0, 0), SourcePosition::new(0, 0))
            .map_err(|_| GraphLinkFailure::InvalidInput)?;
        let parsed = ParsedSymbol::new(
            LocalSymbolId::new(1).map_err(|_| GraphLinkFailure::InvalidInput)?,
            SymbolKind::Class,
            SymbolName::try_from_string("Base".to_owned())
                .map_err(|_| GraphLinkFailure::InvalidInput)?,
            range,
            range,
        )
        .map_err(|_| GraphLinkFailure::InvalidInput)?;
        Ok(GraphSymbol::new(id, revision, parsed))
    }
}
