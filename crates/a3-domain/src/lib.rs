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
    BodyHash, CandidateFreshness, CandidateSetCompleteness, CandidateTokenCost,
    CandidateTokenCostError, CanonicalDirectory, CanonicalDirectoryError, Centrality,
    CentralityError, Confidence, ConfidenceError, ContentHash, DiagnosticMessage,
    DiagnosticMessageError, DiscoveredFile, DiscoveredFileRole, DiscoveredFileRoles,
    DiscoveredFileRolesError, DiscoveryExclusionCounts, DiscoveryExclusionReason, DiscoveryOrigin,
    DiscoveryPolicy, DiscoveryPolicyVersion, DiscoveryPolicyVersionError, DiscoveryResult,
    DiscoveryResultError, EmbeddingBatchSize, EmbeddingBatchSizeError, EmbeddingCacheKey,
    EmbeddingDataType, EmbeddingDimension, EmbeddingDimensionError, EmbeddingModelId,
    EmbeddingModelProfile, EmbeddingProfileTextError, EmbeddingProfileTextKind,
    EmbeddingProviderId, EmbeddingQuantization, EmbeddingTimestamp, EmbeddingTimestampError,
    EmbeddingVector, EmbeddingVectorError, EmbeddingVectorNormalization, EvidenceRef,
    ExactSearchCursor, ExactSearchCursorError, ExactSearchExplanation, ExactSearchHit,
    ExactSearchHitError, ExactSearchPage, ExactSearchPageError, ExactSearchPageSize,
    ExactSearchPageSizeError, ExactSearchPosition, ExactSearchQuery, ExactSearchRole,
    ExactSearchSymbol, ExactSearchTarget, ExactSearchTerm, ExactSearchTextError, FileDelta,
    FileRevision, FusedRetrievalHit, FusedRetrievalResult, FusionContribution, FusionError,
    FusionPolicy, FusionPolicyVersion, FusionPriority, FusionResultLimit, FusionResultLimitError,
    FusionScore, FusionSignalExplanation, FusionTokenExplanation, GitHead, GitObjectId,
    GitObjectIdError, GitReferenceName, GitReferenceNameError, GraphEdge, GraphEndpoint,
    GraphSymbol, GraphTraversalHit, GraphTraversalHitError, GraphTraversalResult,
    GraphTraversalResultError, GraphTraversalTarget, IndexLanguage, IndexPublication,
    IndexPublicationError, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunSequenceError,
    IndexRunStart, IndexRunStatus, IndexRunStatusError, IndexRunTerminalOutcome,
    IndexSchemaVersion, IndexSchemaVersionError, LanguageAdapterContractVersion,
    LanguageAdapterContractVersionError, LanguageAdapterRevision, LanguageAdapterVersion,
    LanguageAdapterVersionError, LanguageParseArtifacts, LanguageParseResult,
    LanguageParseResultError, LexicalScore, LexicalScoreError, LexicalSearchCursor,
    LexicalSearchExplanation, LexicalSearchHit, LexicalSearchPage, LexicalSearchPageError,
    LexicalSearchPageSize, LexicalSearchPageSizeError, LexicalSearchPosition, LexicalSearchQuery,
    LexicalSearchSymbol, LexicalSearchTarget, LexicalSearchTerm, LexicalSearchTermError,
    LinkResolution, LinkedGraph, LinkedGraphError, LocalSymbolId, LocalSymbolIdError,
    MemoryCandidateExplanation, MemoryCandidateExplanationError, ModelProfileId, ModuleId,
    ModuleKind, ModuleMapError, ModuleMembership, ModuleMembershipEvidence, ModuleMembershipKind,
    ModulePolicyVersion, ModulePolicyVersionError, ModuleProjection, ModuleRoot, ModuleSymbolSet,
    NormalizedRetrievalSignal, NormalizedRetrievalSignalError, NormalizedSemanticCard,
    ParseCoverage, ParseCoverageError, ParseDiagnostic, ParseDiagnosticCode,
    ParseDiagnosticSeverity, ParsedSymbol, ParsedSymbolError, ProjectId, ProjectIdentity,
    ProjectIdentityError, PublishedIndex, QualifiedSymbolName, RankProjection, RankProjectionError,
    RankScore, RankScoreError, RankingPolicyVersion, RankingPolicyVersionError,
    RelationshipCandidateExplanation, RemoteIdentity, RenameCandidate, RepositoryCard,
    RepositoryFileState, RepositoryFileStateError, RepositoryId, RepositoryIdentity,
    RepositoryModule, RepositoryPath, RepositoryPathError, ResultExplanation,
    ResultSourceExplanation, RetrievalCandidate, RetrievalCandidateReason, RetrievalCandidateSet,
    RetrievalCandidateSetError, RetrievalCandidateSets, RetrievalCandidateSetsError,
    RetrievalCandidateSignals, RetrievalTargetId, SemanticCardBatch, SemanticCardBatchError,
    SemanticCardId, SemanticCardNormalizationError, SemanticCardNormalizationVersion,
    SemanticEmbedding, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotDelta, SnapshotError,
    SnapshotId, SourceChannel, SourcePosition, SourceRange, SourceRangeError, SymbolId, SymbolKind,
    SymbolName, SymbolRank, SymbolRankSignals, SymbolReference, SymbolReferenceError, SymbolRole,
    SymbolRoles, SymbolSignature, SymbolTextError, SymbolVisibility, SyntaxProvider,
    SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget, TraversalDepth,
    TraversalDepthError, TraversalDirection, TraversalQuery, TraversalResultLimit,
    TraversalResultLimitError, UnresolvedEdgeCandidate, UnresolvedGraphTarget, UnresolvedReason,
    VectorHit, VectorSearchCapability, VectorSearchLimit, VectorSearchLimitError,
    VectorSearchResult, VectorSearchResultError, WorktreeAnchorId, WorktreeGeneration,
    WorktreeGenerationError, WorktreeId, WorktreeIdentity,
};
pub use version::{ApplicationVersion, ApplicationVersionError};
