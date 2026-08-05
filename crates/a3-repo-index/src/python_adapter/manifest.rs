use super::source::{SourceLocator, warning};
use crate::normalize_parse_diagnostics;
use a3_application::{
    LanguageParseControl, LanguageParseControlError, LanguageParseFailure, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseArtifacts, LanguageParseResult,
    LocalSymbolId, ParseDiagnostic, ParseDiagnosticCode, ParsedSymbol, Progress, RepositoryPath,
    SourceRange, SymbolKind, SymbolName, SymbolReference, SymbolRole, SymbolSignature,
    SymbolVisibility, SyntaxProvider, SyntaxRelation, SyntaxRelationKind, SyntaxSource,
    SyntaxTarget,
};
use std::time::Instant;

pub(super) struct ManifestBuilder<'a> {
    input: LanguageParseInput<'a>,
    policy: LanguageParsePolicy,
    control: &'a dyn LanguageParseControl,
    locator: SourceLocator,
    artifacts: LanguageParseArtifacts,
    next_symbol_id: u32,
    full_range: SourceRange,
    progress_total: u64,
    started: Instant,
    visited: usize,
}

impl<'a> ManifestBuilder<'a> {
    pub(super) fn new(
        input: LanguageParseInput<'a>,
        policy: LanguageParsePolicy,
        control: &'a dyn LanguageParseControl,
    ) -> Result<Self, LanguageParseFailure> {
        ensure_active(control)?;
        let locator = SourceLocator::new(input.source());
        let full_range = locator.range(0, input.source().len())?;
        let progress_total = u64::try_from(input.source().len())
            .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?
            .max(1);
        report_progress(control, 0, progress_total)?;
        Ok(Self {
            input,
            policy,
            control,
            locator,
            artifacts: LanguageParseArtifacts::default(),
            next_symbol_id: 1,
            full_range,
            progress_total,
            started: Instant::now(),
            visited: 0,
        })
    }

    pub(super) fn source(&self) -> &[u8] {
        self.input.source()
    }

    pub(super) fn path(&self) -> &RepositoryPath {
        self.input.revision().path()
    }

    pub(super) fn range(
        &self,
        start: usize,
        end: usize,
    ) -> Result<SourceRange, LanguageParseFailure> {
        self.locator.range(start, end)
    }

    pub(super) fn add_root_module(
        &mut self,
        name: &str,
        selection: SourceRange,
        signature: &'static str,
        roles: &[SymbolRole],
    ) -> Result<LocalSymbolId, LanguageParseFailure> {
        self.add_module(
            None,
            name,
            self.full_range,
            selection,
            signature,
            SymbolVisibility::Internal,
            roles,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )
    }

    pub(super) fn add_entrypoint(
        &mut self,
        parent: LocalSymbolId,
        name: &str,
        range: SourceRange,
        target: &str,
        target_range: SourceRange,
    ) -> Result<(), LanguageParseFailure> {
        let id = self.add_module(
            Some(parent),
            name,
            range,
            range,
            "Python entry point",
            SymbolVisibility::Public,
            &[SymbolRole::Entrypoint],
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )?;
        self.push_reference(
            SyntaxSource::Symbol(id),
            target,
            SyntaxRelationKind::Configures,
            target_range,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn add_module(
        &mut self,
        parent: Option<LocalSymbolId>,
        name: &str,
        declaration: SourceRange,
        selection: SourceRange,
        signature: &'static str,
        visibility: SymbolVisibility,
        roles: &[SymbolRole],
        provider: SyntaxProvider,
        confidence: Confidence,
    ) -> Result<LocalSymbolId, LanguageParseFailure> {
        let name = match SymbolName::try_from_string(name.to_owned()) {
            Ok(name) => name,
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    selection,
                    "Python package symbol name exceeds the adapter contract",
                )?)?;
                SymbolName::try_from_string("python-package".to_owned())
                    .map_err(|_| LanguageParseFailure::InvalidResult)?
            }
        };
        let id = self.take_symbol_id()?;
        let mut symbol = ParsedSymbol::new(id, SymbolKind::Module, name, declaration, selection)
            .map_err(|_| LanguageParseFailure::InvalidResult)?
            .with_visibility(visibility)
            .with_signature(
                SymbolSignature::try_from_string(signature.to_owned())
                    .map_err(|_| LanguageParseFailure::InvalidResult)?,
            );
        for role in roles {
            symbol = symbol.with_role(*role);
        }
        self.push_symbol(symbol)?;
        self.push_relation(SyntaxRelation::new(
            parent.map_or(SyntaxSource::File, SyntaxSource::Symbol),
            SyntaxTarget::Symbol(id),
            parent.map_or(SyntaxRelationKind::Defines, |_| {
                SyntaxRelationKind::Contains
            }),
            provider,
            confidence,
            declaration,
        ))?;
        Ok(id)
    }

    pub(super) fn push_reference(
        &mut self,
        source: SyntaxSource,
        reference: &str,
        kind: SyntaxRelationKind,
        range: SourceRange,
        provider: SyntaxProvider,
        confidence: Confidence,
    ) -> Result<(), LanguageParseFailure> {
        let target = match SymbolReference::try_from_string(reference.to_owned()) {
            Ok(reference) => SyntaxTarget::Unresolved(reference),
            Err(_) => {
                self.push_diagnostic(warning(
                    ParseDiagnosticCode::OutputTruncated,
                    range,
                    "Python package relation target exceeds the adapter contract",
                )?)?;
                return Ok(());
            }
        };
        self.push_relation(SyntaxRelation::new(
            source, target, kind, provider, confidence, range,
        ))
    }

    pub(super) fn push_file(
        &mut self,
        source: SyntaxSource,
        relative: &str,
        kind: SyntaxRelationKind,
        range: SourceRange,
    ) -> Result<(), LanguageParseFailure> {
        let Some(path) = resolve_manifest_path(self.path(), relative.as_bytes()) else {
            self.push_diagnostic(warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                range,
                "Python package path is not a safe repository-relative target",
            )?)?;
            return Ok(());
        };
        self.push_relation(SyntaxRelation::new(
            source,
            SyntaxTarget::File(path),
            kind,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        ))
    }

    pub(super) fn unsupported(
        &mut self,
        range: SourceRange,
        message: &'static str,
    ) -> Result<(), LanguageParseFailure> {
        self.push_diagnostic(warning(
            ParseDiagnosticCode::UnsupportedSyntax,
            range,
            message,
        )?)
    }

    pub(super) fn push_diagnostic(
        &mut self,
        diagnostic: ParseDiagnostic,
    ) -> Result<(), LanguageParseFailure> {
        if self.artifacts.diagnostics.len() >= self.policy.max_diagnostics() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        self.artifacts.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn poll(&mut self) -> Result<(), LanguageParseFailure> {
        self.visited = self
            .visited
            .checked_add(1)
            .filter(|value| *value <= self.policy.max_tree_nodes())
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        if self.visited.is_multiple_of(64) {
            self.ensure_active()?;
        }
        Ok(())
    }

    pub(super) fn finish(
        self,
        revision: &LanguageAdapterRevision,
    ) -> Result<LanguageParseResult, LanguageParseFailure> {
        self.ensure_active()?;
        let (coverage, diagnostics) = normalize_parse_diagnostics(
            self.input.source().len(),
            self.policy.max_diagnostics(),
            self.artifacts.diagnostics,
        )?;
        let result = LanguageParseResult::new(
            self.input.revision().clone(),
            revision.clone(),
            self.policy.contract_version(),
            coverage,
            LanguageParseArtifacts {
                diagnostics,
                symbols: self.artifacts.symbols,
                relations: self.artifacts.relations,
            },
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
        ensure_active(self.control)?;
        report_progress(self.control, self.progress_total, self.progress_total)?;
        Ok(result)
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

    fn ensure_active(&self) -> Result<(), LanguageParseFailure> {
        ensure_active(self.control)?;
        if self.started.elapsed() >= self.policy.parse_timeout() {
            return Err(LanguageParseFailure::TimedOut);
        }
        Ok(())
    }
}

pub(super) fn dependency_name(specification: &str) -> Option<&str> {
    let specification = specification.trim();
    let end = specification
        .char_indices()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .map(|(index, character)| index.saturating_add(character.len_utf8()))
        .last()?;
    specification.get(..end).filter(|name| !name.is_empty())
}

fn resolve_manifest_path(manifest: &RepositoryPath, relative: &[u8]) -> Option<RepositoryPath> {
    let relative = relative.strip_prefix(b"./").unwrap_or(relative);
    if relative.is_empty()
        || relative.first() == Some(&b'/')
        || relative.contains(&b'\\')
        || relative.contains(&b'\0')
        || relative.windows(2).any(|window| window == b":/")
    {
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
