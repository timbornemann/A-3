use super::source::{
    StaticString, diagnostic, node_text, normalize_layout, normalized_node_text, static_string,
    warning,
};
use super::{is_setup_py, manifest::dependency_name};
use crate::{TreeSitterParserPool, normalize_parse_diagnostics, source_range_for_node};
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, DiscoveredFileRole, LanguageAdapterRevision, LanguageParseArtifacts,
    LanguageParseResult, LocalSymbolId, ParseDiagnostic, ParseDiagnosticCode, ParsedSymbol,
    RepositoryPath, SourceRange, SymbolKind, SymbolName, SymbolReference, SymbolRole,
    SymbolSignature, SymbolVisibility, SyntaxProvider, SyntaxRelation, SyntaxRelationKind,
    SyntaxSource, SyntaxTarget,
};
use std::time::Instant;
use tree_sitter::{Node, Tree};

const DIRECT_CALL_CONFIDENCE: u16 = 7_000;
const ATTRIBUTE_CALL_CONFIDENCE: u16 = 6_000;
const DYNAMIC_CALL_CONFIDENCE: u16 = 4_000;
const CONVENTIONAL_EXPORT_CONFIDENCE: u16 = 8_500;
const EXTRACTION_POLL_INTERVAL: usize = 256;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
    parser_pool: &TreeSitterParserPool,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    let parsed = parser_pool.parse(input.source(), policy, control)?;
    let (tree, _parser_coverage, diagnostics) = parsed.into_parts();
    let artifacts = PythonExtractor::new(input, policy, control, diagnostics).extract(&tree)?;
    let (coverage, diagnostics) = normalize_parse_diagnostics(
        input.source().len(),
        policy.max_diagnostics(),
        artifacts.diagnostics,
    )?;
    let result = LanguageParseResult::new(
        input.revision().clone(),
        revision.clone(),
        policy.contract_version(),
        coverage,
        LanguageParseArtifacts {
            diagnostics,
            ..artifacts
        },
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)?;
    crate::function_flow::attach(&tree, input, result, control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonScope {
    Module,
    Class,
    Function,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonTestScope {
    Pytest,
    Unittest,
}

impl PythonTestScope {
    const fn reference(self) -> &'static str {
        match self {
            Self::Pytest => "pytest",
            Self::Unittest => "unittest",
        }
    }

    fn confidence(self) -> Result<Confidence, LanguageParseFailure> {
        Confidence::from_basis_points(match self {
            Self::Pytest => 9_000,
            Self::Unittest => 9_500,
        })
        .map_err(|_| LanguageParseFailure::InvalidResult)
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame<'tree> {
    node: Node<'tree>,
    container: LocalSymbolId,
    callable: Option<LocalSymbolId>,
    scope: PythonScope,
    test_scope: Option<PythonTestScope>,
}

#[derive(Debug, Clone, Copy)]
struct SymbolVisit {
    id: LocalSymbolId,
    callable: Option<LocalSymbolId>,
    scope: PythonScope,
    test_scope: Option<PythonTestScope>,
}

struct SymbolDescriptor<'tree> {
    kind: SymbolKind,
    name: SymbolName,
    name_text: String,
    selection: Node<'tree>,
    scope: PythonScope,
}

struct PythonExtractor<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    artifacts: LanguageParseArtifacts,
    next_symbol_id: u32,
    started: Instant,
    file_is_test: bool,
    work_units: usize,
}

impl<'a> PythonExtractor<'a> {
    fn new(
        input: LanguageParseInput<'a>,
        policy: LanguageParsePolicy,
        control: &'a dyn LanguageParseControl,
        diagnostics: Vec<ParseDiagnostic>,
    ) -> Self {
        let file_is_test = input.discovery_roles().contains(DiscoveredFileRole::Test)
            || is_python_test_path(input.revision().path());
        Self {
            input,
            policy,
            control,
            artifacts: LanguageParseArtifacts {
                diagnostics,
                ..LanguageParseArtifacts::default()
            },
            next_symbol_id: 1,
            started: Instant::now(),
            file_is_test,
            work_units: 0,
        }
    }

    fn extract(mut self, tree: &Tree) -> Result<LanguageParseArtifacts, LanguageParseFailure> {
        self.ensure_active()?;
        let root = tree.root_node();
        let root_id = self.add_root_module(root)?;
        let mut stack = Vec::new();
        self.push_children(root, root_id, None, PythonScope::Module, None, &mut stack)?;
        let mut visited = 0usize;
        while let Some(frame) = stack.pop() {
            visited = visited
                .checked_add(1)
                .filter(|value| *value <= self.policy.max_tree_nodes())
                .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
            if visited.is_multiple_of(EXTRACTION_POLL_INTERVAL) {
                self.ensure_active()?;
            }
            if frame.node.kind() == "comment" {
                continue;
            }

            let visit = self.add_symbol(frame)?;
            let (container, callable, scope, test_scope) = visit.map_or(
                (
                    frame.container,
                    frame.callable,
                    frame.scope,
                    frame.test_scope,
                ),
                |visit| (visit.id, visit.callable, visit.scope, visit.test_scope),
            );

            match frame.node.kind() {
                "import_statement" => self.add_import_statement(frame)?,
                "import_from_statement" => self.add_from_import_statement(frame)?,
                "future_import_statement" => self.add_future_import_statement(frame)?,
                "call" => {
                    self.add_call_relation(frame)?;
                    if is_setup_py(self.input.revision().path()) {
                        self.add_setup_metadata(frame)?;
                    }
                }
                "assignment" => self.add_explicit_exports(frame)?,
                _ => {}
            }
            self.push_children(
                frame.node, container, callable, scope, test_scope, &mut stack,
            )?;
        }
        self.ensure_active()?;
        Ok(self.artifacts)
    }

    fn add_root_module(&mut self, root: Node<'_>) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let declaration = source_range_for_node(root)?;
        let selection = SourceRange::new(
            0,
            0,
            a3_domain::SourcePosition::new(0, 0),
            a3_domain::SourcePosition::new(0, 0),
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
        let mut symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            module_name(self.input.revision().path())?,
            declaration,
            selection,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Internal);
        if self.file_is_test {
            symbol = symbol.with_role(SymbolRole::Test);
        }
        if is_python_entry_path(self.input.revision().path()) || self.module_has_main_guard(root)? {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        if let Some(documentation) = documentation_range(root)? {
            symbol = symbol.with_documentation_range(documentation);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::File,
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Defines,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            declaration,
        ))?;
        Ok(id)
    }

    fn add_symbol(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<Option<SymbolVisit>, LanguageParseFailure> {
        let Some(descriptor) = self.symbol_descriptor(frame)? else {
            return Ok(None);
        };
        let id = self.take_symbol_id()?;
        let declaration_node = decoration_anchor(frame.node);
        let declaration = source_range_for_node(declaration_node)?;
        let selection = source_range_for_node(descriptor.selection)?;
        let visibility = python_visibility(frame.scope, &descriptor.name_text);
        let mut symbol =
            ParsedSymbol::new(id, descriptor.kind, descriptor.name, declaration, selection)
                .map_err(|_| LanguageParseFailure::InvalidResult)?
                .with_visibility(visibility);
        if let Some(signature) = self.signature(frame.node)? {
            symbol = symbol.with_signature(signature);
        }
        if let Some(documentation) = documentation_range(frame.node)? {
            symbol = symbol.with_documentation_range(documentation);
        }
        let test_scope = self.test_scope(frame, descriptor.kind, &descriptor.name_text)?;
        if test_scope.is_some() {
            symbol = symbol.with_role(SymbolRole::Test);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(frame.container),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            declaration,
        ))?;
        if frame.scope == PythonScope::Module && visibility == SymbolVisibility::Public {
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(frame.container),
                SyntaxTarget::Symbol(id),
                SyntaxRelationKind::Exports,
                SyntaxProvider::LanguageHeuristic,
                Confidence::from_basis_points(CONVENTIONAL_EXPORT_CONFIDENCE)
                    .map_err(|_| LanguageParseFailure::InvalidResult)?,
                selection,
            ))?;
        }
        if let Some(test_scope) = test_scope {
            self.push_unresolved_relation(
                SyntaxSource::Symbol(id),
                test_scope.reference().to_owned(),
                SyntaxRelationKind::Tests,
                declaration,
                SyntaxProvider::LanguageHeuristic,
                test_scope.confidence()?,
            )?;
        }
        if descriptor.kind == SymbolKind::Class {
            self.add_base_relations(frame.node, id)?;
        }
        let callable = if matches!(descriptor.kind, SymbolKind::Function | SymbolKind::Method) {
            Some(id)
        } else {
            frame.callable
        };
        Ok(Some(SymbolVisit {
            id,
            callable,
            scope: descriptor.scope,
            test_scope: if descriptor.kind == SymbolKind::Class {
                test_scope
            } else {
                None
            },
        }))
    }

    fn symbol_descriptor<'tree>(
        &mut self,
        frame: Frame<'tree>,
    ) -> Result<Option<SymbolDescriptor<'tree>>, LanguageParseFailure> {
        let (kind, scope, selection) = match frame.node.kind() {
            "function_definition" => (
                if frame.scope == PythonScope::Class {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                PythonScope::Function,
                frame.node.child_by_field_name("name"),
            ),
            "class_definition" => (
                SymbolKind::Class,
                PythonScope::Class,
                frame.node.child_by_field_name("name"),
            ),
            "type_alias_statement" => (
                SymbolKind::TypeAlias,
                PythonScope::Other,
                frame.node.child_by_field_name("left"),
            ),
            _ => return Ok(None),
        };
        let Some(mut selection) = selection else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(frame.node)?,
                "Python declaration has no stable supported name",
            )?)?;
            return Ok(None);
        };
        if selection.kind() == "generic_type"
            && let Some(identifier) = first_named_child_of_kind(selection, "identifier")
        {
            selection = identifier;
        }
        if selection.kind() != "identifier" {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(selection)?,
                "Python declaration name is not a single identifier",
            )?)?;
            return Ok(None);
        }
        let Some(name_text) = node_text(self.input.source(), selection).map(str::to_owned) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(selection)?,
                "Python symbol name is not valid UTF-8",
            )?)?;
            return Ok(None);
        };
        let name = match SymbolName::try_from_string(name_text.clone()) {
            Ok(name) => name,
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(selection)?,
                    "Python symbol name exceeds the adapter contract",
                )?)?;
                return Ok(None);
            }
        };
        Ok(Some(SymbolDescriptor {
            kind,
            name,
            name_text,
            selection,
            scope,
        }))
    }

    fn signature(
        &mut self,
        node: Node<'_>,
    ) -> Result<Option<SymbolSignature>, LanguageParseFailure> {
        let end = match node.kind() {
            "function_definition" | "class_definition" => node
                .child_by_field_name("body")
                .map(|body| body.start_byte())
                .unwrap_or(node.end_byte()),
            "type_alias_statement" => node.end_byte(),
            _ => return Ok(None),
        };
        let Some(bytes) = self.input.source().get(node.start_byte()..end) else {
            return Err(LanguageParseFailure::InvalidResult);
        };
        let Some(text) = std::str::from_utf8(bytes).ok() else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(node)?,
                "Python signature is not valid UTF-8",
            )?)?;
            return Ok(None);
        };
        let normalized = normalize_layout(text);
        let normalized = normalized
            .trim_end()
            .strip_suffix(':')
            .unwrap_or(normalized.trim_end())
            .to_owned();
        match SymbolSignature::try_from_string(normalized) {
            Ok(signature) => Ok(Some(signature)),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    source_range_for_node(node)?,
                    "Python signature exceeds the adapter contract",
                )?)?;
                Ok(None)
            }
        }
    }

    fn test_scope(
        &mut self,
        frame: Frame<'_>,
        kind: SymbolKind,
        name: &str,
    ) -> Result<Option<PythonTestScope>, LanguageParseFailure> {
        let decorators = self.decorator_references(frame.node)?;
        if decorators
            .iter()
            .any(|decorator| decorator == "pytest.fixture" || decorator.starts_with("pytest.mark."))
        {
            return Ok(Some(PythonTestScope::Pytest));
        }
        if kind == SymbolKind::Class {
            if self
                .class_base_references(frame.node)?
                .iter()
                .any(|base| base == "TestCase" || base.ends_with(".TestCase"))
            {
                return Ok(Some(PythonTestScope::Unittest));
            }
            if self.file_is_test && name.starts_with("Test") {
                return Ok(Some(PythonTestScope::Pytest));
            }
            return Ok(None);
        }
        if matches!(kind, SymbolKind::Function | SymbolKind::Method) && name.starts_with("test_") {
            if frame.scope == PythonScope::Class
                && frame.test_scope == Some(PythonTestScope::Unittest)
            {
                return Ok(Some(PythonTestScope::Unittest));
            }
            if self.file_is_test || frame.test_scope == Some(PythonTestScope::Pytest) {
                return Ok(Some(PythonTestScope::Pytest));
            }
        }
        Ok(None)
    }

    fn add_import_statement(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        for index in 0..frame.node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = frame
                .node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let target = import_name_node(child).unwrap_or(child);
            self.push_import(frame, target, None)?;
        }
        Ok(())
    }

    fn add_from_import_statement(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let module = frame
            .node
            .child_by_field_name("module_name")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(module_name) = normalized_node_text(self.input.source(), module) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(module)?,
                "Python import module is not valid UTF-8",
            )?)?;
            return Ok(());
        };
        let mut imported = 0usize;
        for index in 0..frame.node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = frame
                .node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if child.id() == module.id() {
                continue;
            }
            if child.kind() == "wildcard_import" {
                let reference = join_import_reference(&module_name, "*");
                self.push_unresolved_relation(
                    import_source(frame),
                    reference,
                    SyntaxRelationKind::Imports,
                    source_range_for_node(child)?,
                    SyntaxProvider::TreeSitter,
                    Confidence::certain(),
                )?;
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(child)?,
                    "Python wildcard import has unknown bound names",
                )?)?;
                imported = imported.saturating_add(1);
                continue;
            }
            if matches!(child.kind(), "dotted_name" | "aliased_import") {
                let target = import_name_node(child).unwrap_or(child);
                self.push_import(frame, target, Some(&module_name))?;
                imported = imported.saturating_add(1);
            }
        }
        if imported == 0 {
            self.push_unresolved_relation(
                import_source(frame),
                module_name,
                SyntaxRelationKind::Imports,
                source_range_for_node(module)?,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
        }
        Ok(())
    }

    fn add_future_import_statement(
        &mut self,
        frame: Frame<'_>,
    ) -> Result<(), LanguageParseFailure> {
        for index in 0..frame.node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = frame
                .node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let target = import_name_node(child).unwrap_or(child);
            self.push_import(frame, target, Some("__future__"))?;
        }
        Ok(())
    }

    fn push_import(
        &mut self,
        frame: Frame<'_>,
        target: Node<'_>,
        module: Option<&str>,
    ) -> Result<(), LanguageParseFailure> {
        let Some(name) = normalized_node_text(self.input.source(), target) else {
            self.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                source_range_for_node(target)?,
                "Python import target is not valid UTF-8",
            )?)?;
            return Ok(());
        };
        let reference = module.map_or(name.clone(), |module| join_import_reference(module, &name));
        self.push_unresolved_relation(
            import_source(frame),
            reference,
            SyntaxRelationKind::Imports,
            source_range_for_node(target)?,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
        )
    }

    fn add_call_relation(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let function = frame
            .node
            .child_by_field_name("function")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(target) = call_target(self.input.source(), function)? else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(function)?,
                "Dynamic Python call target is not structurally stable",
            )?)?;
            return Ok(());
        };
        self.push_unresolved_relation(
            SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container)),
            target.reference,
            SyntaxRelationKind::Calls,
            source_range_for_node(function)?,
            SyntaxProvider::TreeSitter,
            target.confidence,
        )
    }

    fn add_explicit_exports(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        if frame.scope != PythonScope::Module {
            return Ok(());
        }
        let Some(left) = frame.node.child_by_field_name("left") else {
            return Ok(());
        };
        if node_text(self.input.source(), left) != Some("__all__") {
            return Ok(());
        }
        let Some(right) = frame.node.child_by_field_name("right") else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(frame.node)?,
                "Python __all__ declaration has no static value",
            )?)?;
            return Ok(());
        };
        if !matches!(right.kind(), "list" | "tuple" | "set") {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(right)?,
                "Python __all__ must be a static string collection",
            )?)?;
            return Ok(());
        }
        for index in 0..right.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let item = right
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(value) = static_string(self.input.source(), item)? else {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(item)?,
                    "Python __all__ item is not a static text string",
                )?)?;
                continue;
            };
            self.push_unresolved_relation(
                SyntaxSource::Symbol(frame.container),
                value.value,
                SyntaxRelationKind::Exports,
                value.range,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
        }
        Ok(())
    }

    fn add_base_relations(
        &mut self,
        node: Node<'_>,
        symbol: LocalSymbolId,
    ) -> Result<(), LanguageParseFailure> {
        let Some(superclasses) = node.child_by_field_name("superclasses") else {
            return Ok(());
        };
        for index in 0..superclasses.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let base = superclasses
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if matches!(
                base.kind(),
                "keyword_argument" | "list_splat" | "dictionary_splat"
            ) {
                if base.kind() != "keyword_argument"
                    || base
                        .child_by_field_name("name")
                        .and_then(|name| node_text(self.input.source(), name))
                        != Some("metaclass")
                {
                    self.push_diagnostic(warning(
                        ParseDiagnosticCode::UnsupportedSyntax,
                        source_range_for_node(base)?,
                        "Dynamic Python base class is not structurally stable",
                    )?)?;
                }
                continue;
            }
            let Some(reference) = normalized_node_text(self.input.source(), base) else {
                continue;
            };
            self.push_unresolved_relation(
                SyntaxSource::Symbol(symbol),
                reference,
                SyntaxRelationKind::Extends,
                source_range_for_node(base)?,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
            )?;
        }
        Ok(())
    }

    fn add_setup_metadata(&mut self, frame: Frame<'_>) -> Result<(), LanguageParseFailure> {
        let function = frame
            .node
            .child_by_field_name("function")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let Some(function) = normalized_node_text(self.input.source(), function) else {
            return Ok(());
        };
        if !matches!(
            function.as_str(),
            "setup" | "setuptools.setup" | "distutils.core.setup"
        ) {
            return Ok(());
        }
        let arguments = frame
            .node
            .child_by_field_name("arguments")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let mut package_name = None;
        let mut dependencies = Vec::new();
        let mut packages = Vec::new();
        let mut entrypoints = Vec::new();
        for index in 0..arguments.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let argument = arguments
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if argument.kind() != "keyword_argument" {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(argument)?,
                    "Dynamic setup.py argument is not interpreted as package metadata",
                )?)?;
                continue;
            }
            let name_node = argument
                .child_by_field_name("name")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let value = argument
                .child_by_field_name("value")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(name) = node_text(self.input.source(), name_node) else {
                continue;
            };
            match name {
                "name" => {
                    package_name = static_string(self.input.source(), value)?;
                    if package_name.is_none() {
                        self.push_diagnostic(warning(
                            ParseDiagnosticCode::UnsupportedSyntax,
                            source_range_for_node(value)?,
                            "setup.py package name is not a static string",
                        )?)?;
                    }
                }
                "install_requires" => dependencies.extend(
                    self.setup_string_collection(value)?
                        .into_iter()
                        .map(|value| (value, SyntaxRelationKind::Imports)),
                ),
                "tests_require" => dependencies.extend(
                    self.setup_string_collection(value)?
                        .into_iter()
                        .map(|value| (value, SyntaxRelationKind::Tests)),
                ),
                "setup_requires" => dependencies.extend(
                    self.setup_string_collection(value)?
                        .into_iter()
                        .map(|value| (value, SyntaxRelationKind::Builds)),
                ),
                "extras_require" => {
                    dependencies.extend(self.setup_extra_dependencies(value)?);
                }
                "packages" | "py_modules" => {
                    packages.extend(self.setup_string_collection(value)?);
                }
                "entry_points" => {
                    entrypoints.extend(self.setup_entrypoints(value)?);
                }
                _ => {}
            }
        }
        let source = if let Some(name) = package_name {
            SyntaxSource::Symbol(self.add_setup_package(
                frame.container,
                &name,
                !entrypoints.is_empty(),
                frame.node,
            )?)
        } else {
            SyntaxSource::Symbol(frame.container)
        };
        let confidence = Confidence::from_basis_points(8_500)
            .map_err(|_| LanguageParseFailure::InvalidResult)?;
        for (dependency, kind) in dependencies {
            let Some(name) = dependency_name(&dependency.value) else {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    dependency.range,
                    "setup.py dependency has no static package name",
                )?)?;
                continue;
            };
            self.push_unresolved_relation(
                source,
                name.to_owned(),
                kind,
                dependency.range,
                SyntaxProvider::LanguageHeuristic,
                confidence,
            )?;
        }
        for package in packages {
            self.push_unresolved_relation(
                source,
                format!("package:{}", package.value),
                SyntaxRelationKind::Builds,
                package.range,
                SyntaxProvider::LanguageHeuristic,
                confidence,
            )?;
        }
        for entrypoint in entrypoints {
            self.add_setup_entrypoint(source, entrypoint, confidence)?;
        }
        Ok(())
    }

    fn setup_string_collection(
        &mut self,
        node: Node<'_>,
    ) -> Result<Vec<StaticString>, LanguageParseFailure> {
        if !matches!(node.kind(), "list" | "tuple" | "set") {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(node)?,
                "setup.py metadata collection is not a static list, tuple, or set",
            )?)?;
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        for index in 0..node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let item = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(value) = static_string(self.input.source(), item)? else {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(item)?,
                    "setup.py metadata item is not a static string",
                )?)?;
                continue;
            };
            values.push(value);
        }
        Ok(values)
    }

    fn setup_extra_dependencies(
        &mut self,
        node: Node<'_>,
    ) -> Result<Vec<(StaticString, SyntaxRelationKind)>, LanguageParseFailure> {
        if node.kind() != "dictionary" {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(node)?,
                "setup.py extras_require is not a static dictionary",
            )?)?;
            return Ok(Vec::new());
        }
        let mut dependencies = Vec::new();
        for index in 0..node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let pair = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if pair.kind() != "pair" {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(pair)?,
                    "setup.py extras dictionary expansion is dynamic",
                )?)?;
                continue;
            }
            let key = pair
                .child_by_field_name("key")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let value = pair
                .child_by_field_name("value")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(group) = static_string(self.input.source(), key)? else {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(key)?,
                    "setup.py extras group is not a static string",
                )?)?;
                continue;
            };
            let group_lower = group.value.to_ascii_lowercase();
            let kind = if group_lower.contains("test") || group_lower.contains("dev") {
                SyntaxRelationKind::Tests
            } else {
                SyntaxRelationKind::Imports
            };
            dependencies.extend(
                self.setup_string_collection(value)?
                    .into_iter()
                    .map(|dependency| (dependency, kind)),
            );
        }
        Ok(dependencies)
    }

    fn setup_entrypoints(
        &mut self,
        node: Node<'_>,
    ) -> Result<Vec<SetupEntrypoint>, LanguageParseFailure> {
        if node.kind() != "dictionary" {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                source_range_for_node(node)?,
                "setup.py entry_points is not a static dictionary",
            )?)?;
            return Ok(Vec::new());
        }
        let mut entrypoints = Vec::new();
        for index in 0..node.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let pair = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if pair.kind() != "pair" {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    source_range_for_node(pair)?,
                    "setup.py entry point dictionary expansion is dynamic",
                )?)?;
                continue;
            }
            let value = pair
                .child_by_field_name("value")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            for specification in self.setup_string_collection(value)? {
                let Some((name, target)) = specification.value.split_once('=') else {
                    self.push_diagnostic(warning(
                        ParseDiagnosticCode::UnsupportedSyntax,
                        specification.range,
                        "setup.py entry point has no static name/target separator",
                    )?)?;
                    continue;
                };
                let name = name.trim();
                let target = target.trim();
                if name.is_empty() || target.is_empty() {
                    self.push_diagnostic(warning(
                        ParseDiagnosticCode::UnsupportedSyntax,
                        specification.range,
                        "setup.py entry point name or target is empty",
                    )?)?;
                    continue;
                }
                entrypoints.push(SetupEntrypoint {
                    name: name.to_owned(),
                    target: target.to_owned(),
                    range: specification.range,
                });
            }
        }
        Ok(entrypoints)
    }

    fn add_setup_package(
        &mut self,
        parent: LocalSymbolId,
        name: &StaticString,
        entrypoint: bool,
        call: Node<'_>,
    ) -> Result<LocalSymbolId, LanguageParseFailure> {
        let symbol_name = match SymbolName::try_from_string(name.value.clone()) {
            Ok(name) => name,
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    name.range,
                    "setup.py package name exceeds the adapter contract",
                )?)?;
                return Ok(parent);
            }
        };
        let id = self.take_symbol_id()?;
        let declaration = source_range_for_node(call)?;
        let mut symbol =
            ParsedSymbol::new(id, SymbolKind::Module, symbol_name, declaration, name.range)
                .map_err(|_| LanguageParseFailure::InvalidResult)?
                .with_visibility(SymbolVisibility::Internal)
                .with_signature(
                    SymbolSignature::try_from_string("setup.py package".to_owned())
                        .map_err(|_| LanguageParseFailure::InvalidResult)?,
                );
        if entrypoint {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(parent),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::LanguageHeuristic,
            Confidence::from_basis_points(8_500)
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            declaration,
        ))?;
        Ok(id)
    }

    fn add_setup_entrypoint(
        &mut self,
        parent: SyntaxSource,
        entrypoint: SetupEntrypoint,
        confidence: Confidence,
    ) -> Result<(), LanguageParseFailure> {
        let SyntaxSource::Symbol(parent) = parent else {
            return Err(LanguageParseFailure::InvalidResult);
        };
        let name = match SymbolName::try_from_string(entrypoint.name) {
            Ok(name) => name,
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    entrypoint.range,
                    "setup.py entry point name exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        let id = self.take_symbol_id()?;
        let symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            name,
            entrypoint.range,
            entrypoint.range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Public)
        .with_role(SymbolRole::Entrypoint);
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(parent),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::LanguageHeuristic,
            confidence,
            entrypoint.range,
        ))?;
        self.push_unresolved_relation(
            SyntaxSource::Symbol(id),
            entrypoint.target,
            SyntaxRelationKind::Configures,
            entrypoint.range,
            SyntaxProvider::LanguageHeuristic,
            confidence,
        )
    }

    fn push_unresolved_relation(
        &mut self,
        source: SyntaxSource,
        reference: String,
        kind: SyntaxRelationKind,
        range: SourceRange,
        provider: SyntaxProvider,
        confidence: Confidence,
    ) -> Result<(), LanguageParseFailure> {
        let target = match SymbolReference::try_from_string(reference) {
            Ok(reference) => SyntaxTarget::Unresolved(reference),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    range,
                    "Python relation target exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        self.push_relation(SyntaxRelation::new(
            source, target, kind, provider, confidence, range,
        ))
    }

    fn push_children<'tree>(
        &mut self,
        node: Node<'tree>,
        container: LocalSymbolId,
        callable: Option<LocalSymbolId>,
        scope: PythonScope,
        test_scope: Option<PythonTestScope>,
        stack: &mut Vec<Frame<'tree>>,
    ) -> Result<(), LanguageParseFailure> {
        for index in (0..node.named_child_count()).rev() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            stack.push(Frame {
                node: child,
                container,
                callable,
                scope,
                test_scope,
            });
        }
        Ok(())
    }

    fn take_symbol_id(&mut self) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = LocalSymbolId::new(self.next_symbol_id)
            .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
        self.next_symbol_id = self
            .next_symbol_id
            .checked_add(1)
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        Ok(id)
    }

    fn push_symbol(&mut self, symbol: ParsedSymbol) -> Result<(), LanguageParseFailure> {
        if self.artifacts.symbols.len() >= self.policy.max_symbols() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.symbols.push(symbol);
        Ok(())
    }

    fn push_relation(&mut self, relation: SyntaxRelation) -> Result<(), LanguageParseFailure> {
        if self.artifacts.relations.len() >= self.policy.max_relations() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.relations.push(relation);
        Ok(())
    }

    fn push_diagnostic(&mut self, diagnostic: ParseDiagnostic) -> Result<(), LanguageParseFailure> {
        if self.artifacts.diagnostics.len() >= self.policy.max_diagnostics() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.diagnostics.push(diagnostic);
        Ok(())
    }

    fn decorator_references(
        &mut self,
        node: Node<'_>,
    ) -> Result<Vec<String>, LanguageParseFailure> {
        let Some(parent) = node
            .parent()
            .filter(|parent| parent.kind() == "decorated_definition")
        else {
            return Ok(Vec::new());
        };
        let mut decorators = Vec::new();
        for index in 0..parent.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = parent
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if child.kind() != "decorator" {
                continue;
            }
            let Some(expression) = child.named_child(0) else {
                continue;
            };
            let target = if expression.kind() == "call" {
                expression.child_by_field_name("function")
            } else {
                Some(expression)
            };
            if let Some(target) =
                target.and_then(|target| normalized_node_text(self.input.source(), target))
            {
                decorators.push(target);
            }
        }
        Ok(decorators)
    }

    fn class_base_references(
        &mut self,
        node: Node<'_>,
    ) -> Result<Vec<String>, LanguageParseFailure> {
        let Some(superclasses) = node.child_by_field_name("superclasses") else {
            return Ok(Vec::new());
        };
        let mut bases = Vec::new();
        for index in 0..superclasses.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = superclasses
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if child.kind() != "keyword_argument"
                && let Some(reference) = normalized_node_text(self.input.source(), child)
            {
                bases.push(reference);
            }
        }
        Ok(bases)
    }

    fn module_has_main_guard(&mut self, root: Node<'_>) -> Result<bool, LanguageParseFailure> {
        for index in 0..root.named_child_count() {
            self.poll_work()?;
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let Some(statement) = root.named_child(index) else {
                return Err(LanguageParseFailure::InvalidResult);
            };
            if statement.kind() != "if_statement" {
                continue;
            }
            let Some(condition) = statement.child_by_field_name("condition") else {
                continue;
            };
            let Some(condition) = normalized_node_text(self.input.source(), condition) else {
                continue;
            };
            if matches!(
                condition.as_str(),
                "__name__ == \"__main__\""
                    | "__name__ == '__main__'"
                    | "\"__main__\" == __name__"
                    | "'__main__' == __name__"
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn poll_work(&mut self) -> Result<(), LanguageParseFailure> {
        self.work_units = self
            .work_units
            .checked_add(1)
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        if self.work_units.is_multiple_of(EXTRACTION_POLL_INTERVAL) {
            self.ensure_active()?;
        }
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), LanguageParseFailure> {
        if self.control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        if self.started.elapsed() >= self.policy.parse_timeout() {
            return Err(LanguageParseFailure::TimedOut);
        }
        Ok(())
    }
}

struct CallTarget {
    reference: String,
    confidence: Confidence,
}

struct SetupEntrypoint {
    name: String,
    target: String,
    range: SourceRange,
}

fn call_target(source: &[u8], node: Node<'_>) -> Result<Option<CallTarget>, LanguageParseFailure> {
    let basis_points = match node.kind() {
        "identifier" => DIRECT_CALL_CONFIDENCE,
        "attribute" => {
            let object = node
                .child_by_field_name("object")
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if matches!(object.kind(), "identifier" | "attribute") {
                ATTRIBUTE_CALL_CONFIDENCE
            } else {
                DYNAMIC_CALL_CONFIDENCE
            }
        }
        "subscript" => DYNAMIC_CALL_CONFIDENCE,
        "parenthesized_expression" if node.named_child_count() == 1 => {
            let child = node
                .named_child(0)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            return call_target(source, child);
        }
        _ => return Ok(None),
    };
    let Some(reference) = normalized_node_text(source, node) else {
        return Ok(None);
    };
    Ok(Some(CallTarget {
        reference,
        confidence: Confidence::from_basis_points(basis_points)
            .map_err(|_| LanguageParseFailure::InvalidResult)?,
    }))
}

fn import_name_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "aliased_import" {
        return node.child_by_field_name("name");
    }
    Some(node)
}

fn import_source(frame: Frame<'_>) -> SyntaxSource {
    SyntaxSource::Symbol(frame.callable.unwrap_or(frame.container))
}

fn join_import_reference(module: &str, name: &str) -> String {
    if module.ends_with('.') {
        format!("{module}{name}")
    } else {
        format!("{module}.{name}")
    }
}

fn python_visibility(scope: PythonScope, name: &str) -> SymbolVisibility {
    match scope {
        PythonScope::Function | PythonScope::Other => SymbolVisibility::Local,
        PythonScope::Module => {
            if name.starts_with('_') {
                SymbolVisibility::Internal
            } else {
                SymbolVisibility::Public
            }
        }
        PythonScope::Class => {
            if name.starts_with("__") && !name.ends_with("__") {
                SymbolVisibility::Private
            } else if name.starts_with('_') && !(name.starts_with("__") && name.ends_with("__")) {
                SymbolVisibility::Protected
            } else {
                SymbolVisibility::Public
            }
        }
    }
}

fn documentation_range(node: Node<'_>) -> Result<Option<SourceRange>, LanguageParseFailure> {
    let body = if node.kind() == "module" {
        node
    } else if matches!(node.kind(), "function_definition" | "class_definition") {
        let Some(body) = node.child_by_field_name("body") else {
            return Ok(None);
        };
        body
    } else {
        return Ok(None);
    };
    let Some(statement) = body.named_child(0) else {
        return Ok(None);
    };
    if statement.kind() != "expression_statement" {
        return Ok(None);
    }
    let Some(value) = statement.named_child(0) else {
        return Ok(None);
    };
    if !matches!(value.kind(), "string" | "concatenated_string") {
        return Ok(None);
    }
    Ok(Some(source_range_for_node(value)?))
}

fn first_named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    (0..node.named_child_count()).find_map(|index| {
        u32::try_from(index)
            .ok()
            .and_then(|index| node.named_child(index))
            .filter(|child| child.kind() == kind)
    })
}

fn decoration_anchor(node: Node<'_>) -> Node<'_> {
    node.parent()
        .filter(|parent| parent.kind() == "decorated_definition")
        .unwrap_or(node)
}

fn module_name(path: &RepositoryPath) -> Result<SymbolName, LanguageParseFailure> {
    let components = path
        .as_bytes()
        .split(|byte| *byte == b'/')
        .collect::<Vec<_>>();
    let file = components
        .last()
        .copied()
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let stem = file
        .strip_suffix(b".pyi")
        .or_else(|| file.strip_suffix(b".py"))
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let selected = if stem == b"__init__" {
        components
            .get(components.len().saturating_sub(2))
            .copied()
            .unwrap_or(stem)
    } else {
        stem
    };
    if let Some(name) = std::str::from_utf8(selected)
        .ok()
        .and_then(|name| SymbolName::try_from_string(name.to_owned()).ok())
    {
        return Ok(name);
    }
    SymbolName::try_from_string(format!("module-{}", blake3::hash(path.as_bytes()).to_hex()))
        .map_err(|_| LanguageParseFailure::InvalidResult)
}

fn is_python_entry_path(path: &RepositoryPath) -> bool {
    matches!(
        path.as_bytes()
            .rsplit(|byte| *byte == b'/')
            .next()
            .unwrap_or_default(),
        b"__init__.py" | b"__init__.pyi" | b"__main__.py" | b"main.py" | b"cli.py" | b"setup.py"
    )
}

fn is_python_test_path(path: &RepositoryPath) -> bool {
    let name = path
        .as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    name == b"conftest.py"
        || name.starts_with(b"test_")
        || name
            .strip_suffix(b".py")
            .is_some_and(|stem| stem.ends_with(b"_test"))
}
