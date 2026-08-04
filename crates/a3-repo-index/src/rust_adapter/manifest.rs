use super::source::{diagnostic, range_for_offsets};
use crate::normalize_parse_diagnostics;
use a3_application::{
    LanguageParseControl, LanguageParseControlError, LanguageParseFailure, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseArtifacts, LanguageParseResult,
    LocalSymbolId, ParseDiagnosticCode, ParsedSymbol, Progress, RepositoryPath, SourceRange,
    SymbolKind, SymbolName, SymbolReference, SymbolRole, SymbolSignature, SymbolVisibility,
    SyntaxProvider, SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Instant;
use toml::Spanned;

const MAX_CARGO_MANIFEST_BYTES: usize = 256 * 1024;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_CARGO_MANIFEST_BYTES {
        return Err(LanguageParseFailure::InputTooLarge);
    }
    ensure_active(control)?;
    let total = u64::try_from(input.source().len())
        .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?
        .max(1);
    report_progress(control, 0, total)?;
    let started = Instant::now();
    let mut artifacts = LanguageParseArtifacts::default();

    let source_text = match std::str::from_utf8(input.source()) {
        Ok(value) => value,
        Err(error) => {
            let start = error.valid_up_to();
            let end = start.saturating_add(error.error_len().map_or(1, |length| length));
            artifacts.diagnostics.push(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                range_for_offsets(input.source(), start, end.min(input.source().len()))?,
                "Cargo manifest is not valid UTF-8",
            )?);
            return finish(input, policy, control, revision, total, artifacts);
        }
    };
    let manifest = match toml::from_str::<CargoManifest>(source_text) {
        Ok(value) => value,
        Err(error) => {
            let range = error.span().map_or(0..input.source().len(), |span| span);
            artifacts.diagnostics.push(diagnostic(
                ParseDiagnosticCode::SyntaxError,
                range_for_offsets(input.source(), range.start, range.end)?,
                "Cargo manifest syntax or supported schema is invalid",
            )?);
            return finish(input, policy, control, revision, total, artifacts);
        }
    };
    if started.elapsed() >= policy.parse_timeout() {
        return Err(LanguageParseFailure::TimedOut);
    }
    ensure_active(control)?;
    let mut builder = ManifestArtifacts::new(input, policy, control, artifacts)?;
    builder.extract(manifest)?;
    if started.elapsed() >= policy.parse_timeout() {
        return Err(LanguageParseFailure::TimedOut);
    }
    finish(input, policy, control, revision, total, builder.artifacts)
}

fn finish(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
    progress_total: u64,
    artifacts: LanguageParseArtifacts,
) -> Result<LanguageParseResult, LanguageParseFailure> {
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
    ensure_active(control)?;
    report_progress(control, progress_total, progress_total)?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: Option<CargoPackage>,
    workspace: Option<CargoWorkspace>,
    lib: Option<CargoTarget>,
    #[serde(default)]
    bin: Vec<CargoTarget>,
    #[serde(default)]
    test: Vec<CargoTarget>,
    #[serde(default)]
    bench: Vec<CargoTarget>,
    #[serde(default)]
    example: Vec<CargoTarget>,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: Option<Spanned<String>>,
}

#[derive(Debug, Deserialize)]
struct CargoWorkspace {
    #[serde(default)]
    members: Vec<Spanned<String>>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: Option<Spanned<String>>,
    path: Option<Spanned<String>>,
}

struct ManifestArtifacts<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    artifacts: LanguageParseArtifacts,
    next_symbol_id: u32,
    full_range: SourceRange,
}

impl<'a> ManifestArtifacts<'a> {
    fn new(
        input: LanguageParseInput<'a>,
        policy: LanguageParsePolicy,
        control: &'a dyn LanguageParseControl,
        artifacts: LanguageParseArtifacts,
    ) -> Result<Self, LanguageParseFailure> {
        Ok(Self {
            input,
            policy,
            control,
            artifacts,
            next_symbol_id: 1,
            full_range: range_for_offsets(input.source(), 0, input.source().len())?,
        })
    }

    fn extract(&mut self, manifest: CargoManifest) -> Result<(), LanguageParseFailure> {
        let root = match manifest.package.and_then(|package| package.name) {
            Some(name) => self.add_named_module(&name, None, false)?,
            None => self.add_workspace_module()?,
        };
        self.add_dependencies(root, manifest.dependencies, false)?;
        self.add_dependencies(root, manifest.dev_dependencies, true)?;
        self.add_dependencies(root, manifest.build_dependencies, false)?;
        if let Some(workspace) = manifest.workspace {
            self.add_workspace_members(root, workspace.members)?;
        }
        if let Some(target) = manifest.lib {
            self.add_target(root, target, "lib", TargetRole::Library)?;
        }
        for target in manifest.bin {
            self.add_target(root, target, "bin", TargetRole::Binary)?;
        }
        for target in manifest.test {
            self.add_target(root, target, "test", TargetRole::Test)?;
        }
        for target in manifest.bench {
            self.add_target(root, target, "bench", TargetRole::Test)?;
        }
        for target in manifest.example {
            self.add_target(root, target, "example", TargetRole::Binary)?;
        }
        Ok(())
    }

    fn add_workspace_module(&mut self) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let selection = range_for_offsets(self.input.source(), 0, 0)?;
        let symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            SymbolName::try_from_string("workspace".to_owned())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            self.full_range,
            selection,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_signature(
            SymbolSignature::try_from_string("Cargo workspace".to_owned())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
        )
        .with_visibility(SymbolVisibility::Internal);
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::File,
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Defines,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            self.full_range,
        ))?;
        Ok(id)
    }

    fn add_named_module(
        &mut self,
        name: &Spanned<String>,
        parent: Option<LocalSymbolId>,
        entrypoint: bool,
    ) -> Result<LocalSymbolId, LanguageParseFailure> {
        let id = self.take_symbol_id()?;
        let range = range_for_offsets(self.input.source(), name.span().start, name.span().end)?;
        let mut symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            SymbolName::try_from_string(name.get_ref().clone())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            range,
            range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Internal);
        if entrypoint {
            symbol = symbol.with_role(SymbolRole::Entrypoint);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            parent.map_or(SyntaxSource::File, SyntaxSource::Symbol),
            SyntaxTarget::Symbol(id),
            parent.map_or(SyntaxRelationKind::Defines, |_| {
                SyntaxRelationKind::Contains
            }),
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        ))?;
        Ok(id)
    }

    fn add_target(
        &mut self,
        root: LocalSymbolId,
        target: CargoTarget,
        fallback_name: &str,
        role: TargetRole,
    ) -> Result<(), LanguageParseFailure> {
        ensure_active(self.control)?;
        let (name, range) = match target.name.as_ref() {
            Some(name) => (
                name.get_ref().clone(),
                range_for_offsets(self.input.source(), name.span().start, name.span().end)?,
            ),
            None => match target.path.as_ref() {
                Some(path) => (
                    target_name_or_fallback(path.get_ref(), fallback_name),
                    range_for_offsets(self.input.source(), path.span().start, path.span().end)?,
                ),
                None => (fallback_name.to_owned(), self.full_range),
            },
        };
        let id = self.take_symbol_id()?;
        let mut symbol = ParsedSymbol::new(
            id,
            SymbolKind::Module,
            SymbolName::try_from_string(name).map_err(|_| LanguageParseFailure::InvalidResult)?,
            range,
            range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?
        .with_visibility(SymbolVisibility::Internal)
        .with_role(SymbolRole::Entrypoint);
        if role == TargetRole::Test {
            symbol = symbol.with_role(SymbolRole::Test);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            SyntaxSource::Symbol(root),
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Contains,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        ))?;
        if let Some(path) = target.path {
            let path_range =
                range_for_offsets(self.input.source(), path.span().start, path.span().end)?;
            if let Some(path) =
                resolve_manifest_path(self.input.revision().path(), path.get_ref().as_bytes())
            {
                self.push_relation(SyntaxRelation::new(
                    SyntaxSource::Symbol(id),
                    SyntaxTarget::File(path),
                    SyntaxRelationKind::Builds,
                    SyntaxProvider::Manifest,
                    Confidence::certain(),
                    path_range,
                ))?;
            }
        }
        Ok(())
    }

    fn add_dependencies(
        &mut self,
        root: LocalSymbolId,
        dependencies: BTreeMap<String, toml::Value>,
        tests: bool,
    ) -> Result<(), LanguageParseFailure> {
        for name in dependencies.into_keys() {
            ensure_active(self.control)?;
            let reference = SymbolReference::try_from_string(name)
                .map_err(|_| LanguageParseFailure::InvalidResult)?;
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(root),
                SyntaxTarget::Unresolved(reference),
                if tests {
                    SyntaxRelationKind::Tests
                } else {
                    SyntaxRelationKind::Imports
                },
                SyntaxProvider::Manifest,
                Confidence::certain(),
                self.full_range,
            ))?;
        }
        Ok(())
    }

    fn add_workspace_members(
        &mut self,
        root: LocalSymbolId,
        members: Vec<Spanned<String>>,
    ) -> Result<(), LanguageParseFailure> {
        for member in members {
            ensure_active(self.control)?;
            let range =
                range_for_offsets(self.input.source(), member.span().start, member.span().end)?;
            let reference = SymbolReference::try_from_string(member.get_ref().clone())
                .map_err(|_| LanguageParseFailure::InvalidResult)?;
            self.push_relation(SyntaxRelation::new(
                SyntaxSource::Symbol(root),
                SyntaxTarget::Unresolved(reference),
                SyntaxRelationKind::Builds,
                SyntaxProvider::Manifest,
                Confidence::certain(),
                range,
            ))?;
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
        if self.artifacts.relations.contains(&relation) {
            return Ok(());
        }
        if self.artifacts.relations.len() >= self.policy.max_relations() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.relations.push(relation);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetRole {
    Library,
    Binary,
    Test,
}

fn target_name_from_path(path: &str) -> Option<String> {
    let file = path.rsplit(['/', '\\']).next()?;
    file.strip_suffix(".rs").map(str::to_owned)
}

fn target_name_or_fallback(path: &str, fallback: &str) -> String {
    if let Some(name) = target_name_from_path(path) {
        return name;
    }
    fallback.to_owned()
}

fn resolve_manifest_path(manifest: &RepositoryPath, relative: &[u8]) -> Option<RepositoryPath> {
    if relative.is_empty() || relative.first() == Some(&b'/') || relative.contains(&b'\\') {
        return None;
    }
    let mut components = manifest
        .as_bytes()
        .split(|byte| *byte == b'/')
        .collect::<Vec<_>>();
    components.pop()?;
    for component in relative.split(|byte| *byte == b'/') {
        match component {
            b"" | b"." => {}
            b".." => {
                components.pop()?;
            }
            value => components.push(value),
        }
    }
    let mut bytes = Vec::new();
    for component in components {
        if !bytes.is_empty() {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(component);
    }
    RepositoryPath::try_from_bytes(bytes).ok()
}

fn ensure_active(control: &dyn LanguageParseControl) -> Result<(), LanguageParseFailure> {
    if control.is_cancelled() {
        return Err(LanguageParseFailure::Cancelled);
    }
    Ok(())
}

fn report_progress(
    control: &dyn LanguageParseControl,
    completed: u64,
    total: u64,
) -> Result<(), LanguageParseFailure> {
    let progress =
        Progress::determinate(completed, total).map_err(|_| LanguageParseFailure::InvalidResult)?;
    control
        .report_progress(progress)
        .map_err(|LanguageParseControlError::Unavailable| LanguageParseFailure::ProgressUnavailable)
}
