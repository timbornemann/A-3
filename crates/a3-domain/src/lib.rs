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
    DiscoveryPolicyVersionError, DiscoveryResult, DiscoveryResultError, EvidenceRef,
    ExactSearchCursor, ExactSearchCursorError, ExactSearchExplanation, ExactSearchHit,
    ExactSearchHitError, ExactSearchPage, ExactSearchPageError, ExactSearchPageSize,
    ExactSearchPageSizeError, ExactSearchPosition, ExactSearchQuery, ExactSearchRole,
    ExactSearchSymbol, ExactSearchTarget, ExactSearchTerm, ExactSearchTextError, FileDelta,
    FileRevision, GitHead, GitObjectId, GitObjectIdError, GitReferenceName, GitReferenceNameError,
    GraphEdge, GraphEndpoint, GraphSymbol, IndexLanguage, IndexPublication, IndexPublicationError,
    IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunSequenceError, IndexRunStart,
    IndexRunStatus, IndexRunStatusError, IndexRunTerminalOutcome, IndexSchemaVersion,
    IndexSchemaVersionError, LanguageAdapterContractVersion, LanguageAdapterContractVersionError,
    LanguageAdapterRevision, LanguageAdapterVersion, LanguageAdapterVersionError,
    LanguageParseArtifacts, LanguageParseResult, LanguageParseResultError, LexicalScore,
    LexicalScoreError, LexicalSearchCursor, LexicalSearchExplanation, LexicalSearchHit,
    LexicalSearchPage, LexicalSearchPageError, LexicalSearchPageSize, LexicalSearchPageSizeError,
    LexicalSearchPosition, LexicalSearchQuery, LexicalSearchSymbol, LexicalSearchTarget,
    LexicalSearchTerm, LexicalSearchTermError, LinkResolution, LinkedGraph, LinkedGraphError,
    LocalSymbolId, LocalSymbolIdError, ParseCoverage, ParseCoverageError, ParseDiagnostic,
    ParseDiagnosticCode, ParseDiagnosticSeverity, ParsedSymbol, ParsedSymbolError, ProjectId,
    ProjectIdentity, ProjectIdentityError, PublishedIndex, QualifiedSymbolName, RankProjection,
    RankProjectionError, RankScore, RankScoreError, RankingPolicyVersion,
    RankingPolicyVersionError, RemoteIdentity, RenameCandidate, RepositoryFileState,
    RepositoryFileStateError, RepositoryId, RepositoryIdentity, RepositoryPath,
    RepositoryPathError, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotDelta,
    SnapshotError, SnapshotId, SourceChannel, SourcePosition, SourceRange, SourceRangeError,
    SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SymbolReference,
    SymbolReferenceError, SymbolRole, SymbolRoles, SymbolSignature, SymbolTextError,
    SymbolVisibility, SyntaxProvider, SyntaxRelation, SyntaxRelationKind, SyntaxSource,
    SyntaxTarget, UnresolvedEdgeCandidate, UnresolvedGraphTarget, UnresolvedReason,
    WorktreeAnchorId, WorktreeGeneration, WorktreeGenerationError, WorktreeId, WorktreeIdentity,
};
pub use version::{ApplicationVersion, ApplicationVersionError};
