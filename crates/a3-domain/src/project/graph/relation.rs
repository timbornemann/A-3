use super::SymbolId;
use crate::{
    Confidence, FileRevision, RepositoryPath, SnapshotId, SourceRange, SymbolReference,
    SyntaxProvider, SyntaxRelationKind,
};

/// Exact source revision and range supporting one graph relation or unresolved candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    revision: FileRevision,
    range: SourceRange,
}

impl EvidenceRef {
    /// Creates snapshot-local evidence from already validated parser output.
    #[must_use]
    pub const fn new(revision: FileRevision, range: SourceRange) -> Self {
        Self { revision, range }
    }

    /// Returns the exact source file revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the half-open source range supporting the relation.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }
}

/// Resolved graph endpoint inside one immutable snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphEndpoint {
    /// One repository-relative file in the effective snapshot state.
    File(RepositoryPath),
    /// One content- and adapter-bound structural symbol.
    Symbol(SymbolId),
}

/// Deterministic evidence class used to resolve one syntax target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinkResolution {
    /// The adapter directly referred to a file-local symbol.
    AdapterLocalSymbol,
    /// The adapter directly emitted a validated repository-relative file target.
    AdapterFile,
    /// A language-aware module reference selected exactly one repository file or symbol.
    ExactModuleReference,
    /// A simple source name selected exactly one symbol within the same file.
    UniqueFileLocalName,
    /// A qualified source name selected exactly one symbol in the snapshot.
    UniqueQualifiedName,
}

/// One resolved, evidence-carrying graph relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    source: GraphEndpoint,
    target: GraphEndpoint,
    kind: SyntaxRelationKind,
    provider: SyntaxProvider,
    confidence: Confidence,
    resolution: LinkResolution,
    snapshot_id: SnapshotId,
    evidence: EvidenceRef,
}

impl GraphEdge {
    /// Creates one resolved edge while retaining all adapter evidence.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source: GraphEndpoint,
        target: GraphEndpoint,
        kind: SyntaxRelationKind,
        provider: SyntaxProvider,
        confidence: Confidence,
        resolution: LinkResolution,
        snapshot_id: SnapshotId,
        evidence: EvidenceRef,
    ) -> Self {
        Self {
            source,
            target,
            kind,
            provider,
            confidence,
            resolution,
            snapshot_id,
            evidence,
        }
    }

    /// Returns the resolved source endpoint.
    #[must_use]
    pub const fn source(&self) -> &GraphEndpoint {
        &self.source
    }

    /// Returns the resolved target endpoint.
    #[must_use]
    pub const fn target(&self) -> &GraphEndpoint {
        &self.target
    }

    /// Returns the language-neutral relation kind.
    #[must_use]
    pub const fn kind(&self) -> SyntaxRelationKind {
        self.kind
    }

    /// Returns the adapter provider that observed the relation.
    #[must_use]
    pub const fn provider(&self) -> SyntaxProvider {
        self.provider
    }

    /// Returns confidence after conservative linker capping.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns how the target was resolved.
    #[must_use]
    pub const fn resolution(&self) -> LinkResolution {
        self.resolution
    }

    /// Returns the immutable snapshot containing both endpoints.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact source evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRef {
        &self.evidence
    }
}

/// Unresolved target retained without promoting it to a graph fact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnresolvedGraphTarget {
    /// A normalized file target absent from the current effective state.
    File(RepositoryPath),
    /// A bounded source-level name, module specifier, or dynamic expression.
    Reference(SymbolReference),
}

/// Why the deterministic linker refused to promote a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnresolvedReason {
    /// No exact repository-local target was proven.
    NoDeterministicMatch,
    /// More than one target satisfied the same deterministic key.
    AmbiguousMatch,
    /// The source expression requires runtime semantics.
    DynamicReference,
    /// An adapter-emitted file target is not present in the effective snapshot.
    MissingFile,
}

/// One evidence-carrying relation candidate that is explicitly not a resolved graph edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedEdgeCandidate {
    source: GraphEndpoint,
    target: UnresolvedGraphTarget,
    kind: SyntaxRelationKind,
    provider: SyntaxProvider,
    confidence: Confidence,
    reason: UnresolvedReason,
    snapshot_id: SnapshotId,
    evidence: EvidenceRef,
}

impl UnresolvedEdgeCandidate {
    /// Creates a candidate that cannot be consumed as a resolved edge.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source: GraphEndpoint,
        target: UnresolvedGraphTarget,
        kind: SyntaxRelationKind,
        provider: SyntaxProvider,
        confidence: Confidence,
        reason: UnresolvedReason,
        snapshot_id: SnapshotId,
        evidence: EvidenceRef,
    ) -> Self {
        Self {
            source,
            target,
            kind,
            provider,
            confidence,
            reason,
            snapshot_id,
            evidence,
        }
    }

    /// Returns the resolved source endpoint.
    #[must_use]
    pub const fn source(&self) -> &GraphEndpoint {
        &self.source
    }

    /// Returns the deliberately unresolved target.
    #[must_use]
    pub const fn target(&self) -> &UnresolvedGraphTarget {
        &self.target
    }

    /// Returns the requested relation kind.
    #[must_use]
    pub const fn kind(&self) -> SyntaxRelationKind {
        self.kind
    }

    /// Returns the original deterministic provider.
    #[must_use]
    pub const fn provider(&self) -> SyntaxProvider {
        self.provider
    }

    /// Returns the adapter confidence without implying successful resolution.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns why no resolved edge was emitted.
    #[must_use]
    pub const fn reason(&self) -> UnresolvedReason {
        self.reason
    }

    /// Returns the immutable snapshot in which resolution was attempted.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact source evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRef {
        &self.evidence
    }
}
