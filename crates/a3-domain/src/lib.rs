//! Framework-independent domain model and invariants for A^3.

mod health;
mod job;
mod platform;
mod progress;
mod project;
mod version;

pub use health::Health;
pub use job::{JobId, JobOwner, JobStatus};
pub use platform::Platform;
pub use progress::{Progress, ProgressTransitionError, ProgressValueError};
pub use project::{
    CanonicalDirectory, CanonicalDirectoryError, Centrality, CentralityError, Confidence,
    ConfidenceError, ContentHash, DiagnosticMessage, DiagnosticMessageError, DiscoveredFile,
    DiscoveredFileRole, DiscoveredFileRoles, DiscoveredFileRolesError, DiscoveryExclusionCounts,
    DiscoveryExclusionReason, DiscoveryOrigin, DiscoveryPolicy, DiscoveryPolicyVersion,
    DiscoveryPolicyVersionError, DiscoveryResult, DiscoveryResultError, EvidenceRef, FileDelta,
    FileRevision, GitHead, GitObjectId, GitObjectIdError, GitReferenceName, GitReferenceNameError,
    GraphEdge, GraphEndpoint, GraphSymbol, IndexLanguage, IndexRunId, IndexRunRecord,
    IndexRunSequence, IndexRunSequenceError, IndexRunStart, IndexRunStatus, IndexRunStatusError,
    IndexRunTerminalOutcome, IndexSchemaVersion, IndexSchemaVersionError,
    LanguageAdapterContractVersion, LanguageAdapterContractVersionError, LanguageAdapterRevision,
    LanguageAdapterVersion, LanguageAdapterVersionError, LanguageParseArtifacts,
    LanguageParseResult, LanguageParseResultError, LinkResolution, LinkedGraph, LinkedGraphError,
    LocalSymbolId, LocalSymbolIdError, ParseCoverage, ParseCoverageError, ParseDiagnostic,
    ParseDiagnosticCode, ParseDiagnosticSeverity, ParsedSymbol, ParsedSymbolError, ProjectId,
    ProjectIdentity, ProjectIdentityError, RankProjection, RankProjectionError, RankScore,
    RankScoreError, RankingPolicyVersion, RankingPolicyVersionError, RemoteIdentity,
    RenameCandidate, RepositoryFileState, RepositoryFileStateError, RepositoryId,
    RepositoryIdentity, RepositoryPath, RepositoryPathError, Snapshot, SnapshotChange,
    SnapshotChangeKind, SnapshotDelta, SnapshotError, SnapshotId, SourcePosition, SourceRange,
    SourceRangeError, SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals,
    SymbolReference, SymbolReferenceError, SymbolRole, SymbolRoles, SymbolSignature,
    SymbolTextError, SymbolVisibility, SyntaxProvider, SyntaxRelation, SyntaxRelationKind,
    SyntaxSource, SyntaxTarget, UnresolvedEdgeCandidate, UnresolvedGraphTarget, UnresolvedReason,
    WorktreeAnchorId, WorktreeGeneration, WorktreeGenerationError, WorktreeId, WorktreeIdentity,
};
pub use version::{ApplicationVersion, ApplicationVersionError};
