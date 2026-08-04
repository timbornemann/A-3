use super::{
    LocalSymbolId, ParseDiagnostic, ParsedSymbol, SourceRange, SyntaxSource, SyntaxTarget,
};
use crate::{FileRevision, IndexLanguage, LanguageAdapterRevision};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

/// Version of the language-adapter input, output, and invariant contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageAdapterContractVersion(u32);

impl LanguageAdapterContractVersion {
    /// Creates a positive contract version.
    pub fn new(value: u32) -> Result<Self, LanguageAdapterContractVersionError> {
        if value == 0 {
            return Err(LanguageAdapterContractVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the V1 contract used by the initial structural adapters.
    #[must_use]
    pub const fn v1() -> Self {
        Self(1)
    }

    /// Returns the stable primitive representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Language-adapter contract version zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageAdapterContractVersionError;

impl fmt::Display for LanguageAdapterContractVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("language-adapter contract version must be positive")
    }
}

impl Error for LanguageAdapterContractVersionError {}

/// Visible structural coverage of one parsed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseCoverage {
    total_bytes: u32,
    covered_bytes: u32,
    incomplete_regions: u32,
}

impl ParseCoverage {
    /// Creates coverage while rejecting overflow and contradictory totals.
    pub fn new(
        total_bytes: usize,
        covered_bytes: usize,
        incomplete_regions: usize,
    ) -> Result<Self, ParseCoverageError> {
        let total_bytes =
            u32::try_from(total_bytes).map_err(|_| ParseCoverageError::ValueTooLarge)?;
        let covered_bytes =
            u32::try_from(covered_bytes).map_err(|_| ParseCoverageError::ValueTooLarge)?;
        let incomplete_regions =
            u32::try_from(incomplete_regions).map_err(|_| ParseCoverageError::ValueTooLarge)?;
        if covered_bytes > total_bytes {
            return Err(ParseCoverageError::CoveredExceedsTotal);
        }
        Ok(Self {
            total_bytes,
            covered_bytes,
            incomplete_regions,
        })
    }

    /// Returns complete coverage for a bounded source length.
    pub fn complete(total_bytes: usize) -> Result<Self, ParseCoverageError> {
        Self::new(total_bytes, total_bytes, 0)
    }

    /// Returns the bounded source length.
    #[must_use]
    pub const fn total_bytes(self) -> u32 {
        self.total_bytes
    }

    /// Returns bytes not covered by incomplete syntax regions.
    #[must_use]
    pub const fn covered_bytes(self) -> u32 {
        self.covered_bytes
    }

    /// Returns the number of disjoint or missing incomplete regions.
    #[must_use]
    pub const fn incomplete_regions(self) -> u32 {
        self.incomplete_regions
    }

    /// Returns whether the adapter reports complete structural coverage.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.covered_bytes == self.total_bytes && self.incomplete_regions == 0
    }

    /// Returns deterministic coverage in basis points, treating an empty file as complete.
    #[must_use]
    pub fn basis_points(self) -> u16 {
        if self.total_bytes == 0 {
            return 10_000;
        }
        ((u64::from(self.covered_bytes) * 10_000) / u64::from(self.total_bytes)) as u16
    }
}

/// Invalid parse coverage supplied by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseCoverageError {
    /// A count exceeded the durable 32-bit representation.
    ValueTooLarge,
    /// Covered bytes exceeded source bytes.
    CoveredExceedsTotal,
}

impl fmt::Display for ParseCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueTooLarge => formatter.write_str("parse coverage value is too large"),
            Self::CoveredExceedsTotal => {
                formatter.write_str("covered bytes exceed total source bytes")
            }
        }
    }
}

impl Error for ParseCoverageError {}

/// Bounded structural artifacts assembled by one adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageParseArtifacts {
    /// Symbols before aggregate canonicalization.
    pub symbols: Vec<ParsedSymbol>,
    /// Relations before aggregate canonicalization.
    pub relations: Vec<super::SyntaxRelation>,
    /// Diagnostics before aggregate canonicalization.
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Canonical language-adapter output for one exact file revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageParseResult {
    revision: FileRevision,
    adapter_revision: LanguageAdapterRevision,
    contract_version: LanguageAdapterContractVersion,
    coverage: ParseCoverage,
    symbols: Vec<ParsedSymbol>,
    relations: Vec<super::SyntaxRelation>,
    diagnostics: Vec<ParseDiagnostic>,
}

impl LanguageParseResult {
    /// Validates source bounds and references, then canonicalizes all artifact sets.
    pub fn new(
        revision: FileRevision,
        adapter_revision: LanguageAdapterRevision,
        contract_version: LanguageAdapterContractVersion,
        coverage: ParseCoverage,
        mut artifacts: LanguageParseArtifacts,
    ) -> Result<Self, LanguageParseResultError> {
        if coverage.is_complete() && !artifacts.diagnostics.is_empty() {
            return Err(LanguageParseResultError::DiagnosticsWithCompleteCoverage);
        }
        let total_bytes = coverage.total_bytes();
        artifacts.symbols.sort_by_key(ParsedSymbol::id);
        if artifacts
            .symbols
            .windows(2)
            .any(|pair| pair[0].id() == pair[1].id())
        {
            return Err(LanguageParseResultError::DuplicateSymbol);
        }
        let symbol_ids = artifacts
            .symbols
            .iter()
            .map(ParsedSymbol::id)
            .collect::<BTreeSet<_>>();
        for symbol in &artifacts.symbols {
            validate_range(symbol.declaration_range(), total_bytes)?;
            validate_range(symbol.selection_range(), total_bytes)?;
            if let Some(range) = symbol.documentation_range() {
                validate_range(range, total_bytes)?;
            }
        }
        for relation in &artifacts.relations {
            validate_range(relation.evidence_range(), total_bytes)?;
            if let SyntaxSource::Symbol(id) = relation.source() {
                validate_symbol_reference(id, &symbol_ids)?;
            }
            if let SyntaxTarget::Symbol(id) = relation.target() {
                validate_symbol_reference(*id, &symbol_ids)?;
            }
        }
        for diagnostic in &artifacts.diagnostics {
            validate_range(diagnostic.range(), total_bytes)?;
        }

        artifacts.relations.sort();
        if artifacts
            .relations
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(LanguageParseResultError::DuplicateRelation);
        }
        artifacts.diagnostics.sort();
        if artifacts
            .diagnostics
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(LanguageParseResultError::DuplicateDiagnostic);
        }
        Ok(Self {
            revision,
            adapter_revision,
            contract_version,
            coverage,
            symbols: artifacts.symbols,
            relations: artifacts.relations,
            diagnostics: artifacts.diagnostics,
        })
    }

    /// Returns the exact file revision that supplied source bytes.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the language family of the adapter.
    #[must_use]
    pub const fn language(&self) -> IndexLanguage {
        self.adapter_revision.language()
    }

    /// Returns the exact adapter and grammar revision.
    #[must_use]
    pub const fn adapter_revision(&self) -> &LanguageAdapterRevision {
        &self.adapter_revision
    }

    /// Returns the input/output contract revision.
    #[must_use]
    pub const fn contract_version(&self) -> LanguageAdapterContractVersion {
        self.contract_version
    }

    /// Returns visible structural coverage.
    #[must_use]
    pub const fn coverage(&self) -> ParseCoverage {
        self.coverage
    }

    /// Returns symbols in file-local ID order.
    #[must_use]
    pub fn symbols(&self) -> &[ParsedSymbol] {
        &self.symbols
    }

    /// Returns relations in canonical endpoint/evidence order.
    #[must_use]
    pub fn relations(&self) -> &[super::SyntaxRelation] {
        &self.relations
    }

    /// Returns diagnostics in canonical range/code order.
    #[must_use]
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }
}

fn validate_symbol_reference(
    id: LocalSymbolId,
    known: &BTreeSet<LocalSymbolId>,
) -> Result<(), LanguageParseResultError> {
    if !known.contains(&id) {
        return Err(LanguageParseResultError::UnknownSymbolReference);
    }
    Ok(())
}

fn validate_range(range: SourceRange, total_bytes: u32) -> Result<(), LanguageParseResultError> {
    if range.end_byte() > total_bytes {
        return Err(LanguageParseResultError::RangeOutsideSource);
    }
    Ok(())
}

/// Invalid or non-canonical language-adapter output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageParseResultError {
    /// A source range exceeded the input length.
    RangeOutsideSource,
    /// More than one symbol used the same file-local identity.
    DuplicateSymbol,
    /// A relation referred to an absent file-local symbol.
    UnknownSymbolReference,
    /// An exact duplicate relation was emitted.
    DuplicateRelation,
    /// An exact duplicate diagnostic was emitted.
    DuplicateDiagnostic,
    /// Complete coverage contradicted emitted diagnostics.
    DiagnosticsWithCompleteCoverage,
}

impl fmt::Display for LanguageParseResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RangeOutsideSource => formatter.write_str("parse range is outside the source"),
            Self::DuplicateSymbol => formatter.write_str("parse result contains duplicate symbols"),
            Self::UnknownSymbolReference => {
                formatter.write_str("parse relation refers to an unknown symbol")
            }
            Self::DuplicateRelation => {
                formatter.write_str("parse result contains duplicate relations")
            }
            Self::DuplicateDiagnostic => {
                formatter.write_str("parse result contains duplicate diagnostics")
            }
            Self::DiagnosticsWithCompleteCoverage => {
                formatter.write_str("complete parse coverage cannot contain diagnostics")
            }
        }
    }
}

impl Error for LanguageParseResultError {}
