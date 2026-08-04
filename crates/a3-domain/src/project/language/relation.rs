use super::{LocalSymbolId, SourceRange};
use crate::RepositoryPath;
use std::error::Error;
use std::fmt;

const MAX_REFERENCE_BYTES: usize = 4 * 1_024;
const CERTAIN_BASIS_POINTS: u16 = 10_000;

/// Deterministic confidence represented as basis points from zero through 10,000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Confidence(u16);

impl Confidence {
    /// Creates a bounded confidence value.
    pub fn from_basis_points(value: u16) -> Result<Self, ConfidenceError> {
        if value > CERTAIN_BASIS_POINTS {
            return Err(ConfidenceError(value));
        }
        Ok(Self(value))
    }

    /// Returns certainty for directly proven syntax relationships.
    #[must_use]
    pub const fn certain() -> Self {
        Self(CERTAIN_BASIS_POINTS)
    }

    /// Returns the stable integer representation.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

/// Confidence exceeded 100 percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceError(u16);

impl fmt::Display for ConfidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "confidence {} exceeds 10,000 basis points",
            self.0
        )
    }
}

impl Error for ConfidenceError {}

/// Bounded unresolved name or module specifier emitted by an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolReference(String);

impl SymbolReference {
    /// Validates a non-empty single-line reference.
    pub fn try_from_string(value: String) -> Result<Self, SymbolReferenceError> {
        if value.is_empty() || value.len() > MAX_REFERENCE_BYTES {
            return Err(SymbolReferenceError::InvalidLength(value.len()));
        }
        if value.chars().any(char::is_control) {
            return Err(SymbolReferenceError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the unresolved source text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid unresolved symbol or module reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolReferenceError {
    /// Reference text was empty or too large.
    InvalidLength(usize),
    /// Reference text contained a control character.
    InvalidCharacter,
}

impl fmt::Display for SymbolReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(formatter, "symbol reference has invalid length {length}")
            }
            Self::InvalidCharacter => {
                formatter.write_str("symbol reference contains an invalid character")
            }
        }
    }
}

impl Error for SymbolReferenceError {}

/// Source endpoint of one file-local syntactic relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxSource {
    /// The current file is the source.
    File,
    /// A parsed symbol in the current file is the source.
    Symbol(LocalSymbolId),
}

/// Target endpoint before the graph linker resolves cross-file identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxTarget {
    /// A parsed symbol in the current file.
    Symbol(LocalSymbolId),
    /// An already normalized repository-relative file target.
    File(RepositoryPath),
    /// A source-level name or module specifier requiring later resolution.
    Unresolved(SymbolReference),
}

/// Language-neutral syntactic relationship category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxRelationKind {
    /// Lexical containment.
    Contains,
    /// Definition by a file or containing symbol.
    Defines,
    /// Import relationship.
    Imports,
    /// Export relationship.
    Exports,
    /// Syntactically visible call candidate.
    Calls,
    /// Trait or interface implementation.
    Implements,
    /// Type extension or inheritance.
    Extends,
    /// Read access candidate.
    Reads,
    /// Write access candidate.
    Writes,
    /// Configuration relationship.
    Configures,
    /// Test-to-subject relationship.
    Tests,
    /// Build relationship.
    Builds,
    /// Documentation relationship.
    Documents,
}

/// Deterministic provider that observed a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxProvider {
    /// Direct Tree-sitter syntax observation.
    TreeSitter,
    /// Deterministic manifest interpretation.
    Manifest,
    /// Bounded language-specific syntax heuristic.
    LanguageHeuristic,
}

/// One evidence-ranged relation emitted before graph linking.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SyntaxRelation {
    source: SyntaxSource,
    target: SyntaxTarget,
    kind: SyntaxRelationKind,
    provider: SyntaxProvider,
    confidence: Confidence,
    evidence_range: SourceRange,
}

impl SyntaxRelation {
    /// Creates one relation from typed endpoints and evidence.
    #[must_use]
    pub const fn new(
        source: SyntaxSource,
        target: SyntaxTarget,
        kind: SyntaxRelationKind,
        provider: SyntaxProvider,
        confidence: Confidence,
        evidence_range: SourceRange,
    ) -> Self {
        Self {
            source,
            target,
            kind,
            provider,
            confidence,
            evidence_range,
        }
    }

    /// Returns the source endpoint.
    #[must_use]
    pub const fn source(&self) -> SyntaxSource {
        self.source
    }

    /// Returns the target endpoint.
    #[must_use]
    pub const fn target(&self) -> &SyntaxTarget {
        &self.target
    }

    /// Returns the relation kind.
    #[must_use]
    pub const fn kind(&self) -> SyntaxRelationKind {
        self.kind
    }

    /// Returns the deterministic provider.
    #[must_use]
    pub const fn provider(&self) -> SyntaxProvider {
        self.provider
    }

    /// Returns confidence in the unresolved relation.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns the source evidence range.
    #[must_use]
    pub const fn evidence_range(&self) -> SourceRange {
        self.evidence_range
    }
}
