mod claim_verification;
mod deep_map;
mod deep_map_explorer;
mod discovery;
mod embedding;
mod git;
mod graph;
mod graph_retrieval;
mod id;
mod index_run;
mod language;
mod lexical_retrieval;
mod module_map;
mod path;
mod retrieval;
mod retrieval_fusion;
mod revision;
mod semantic_card;
mod snapshot;
mod task_lens;

pub use claim_verification::{
    ModuleCardClaimId, ModuleCardContradiction, ModuleCardContradictionReport,
    ModuleCardVerificationCandidate, ModuleCardVerificationError, ModuleCardVerifier,
    ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimProposal,
    ModuleClaimProposalError, ModuleClaimSchemaVersion, ModuleClaimStatement,
    ModuleClaimStatementError, ResolvedModuleCardEvidence, ResolvedModuleCardEvidenceSet,
    VerifiedClaimKind, VerifiedClaimStatus, VerifiedModuleCard, VerifiedModuleCardBatch,
    VerifiedModuleClaim,
};
pub use deep_map::{
    CoverageRequirement, DeepMapPlanError, DeepMapPlanner, ExpectedInformationGain,
    ExpectedInformationGainError, ExplorationStopPolicy, ExplorationStopReason,
    ExplorationStopState, ExploreBudget, ExploreBudgetError, ExploreCost, ExploreCostError,
    ExploreEvidenceRequirement, ExplorePlan, ExplorePlanStopReason, ExplorePolicyVersion,
    ExploreSeedReason, ExploreStep, ExploreStepStatus, ExploreTarget, ExploreVerificationMethod,
    MapperProfileVersion, ModuleCardEvidenceId, ModuleCardField, ModuleCardFieldSpec, ModuleCardId,
    ModuleCardMetadataField, ModuleCardSchema, ModuleCardSchemaVersion, ModuleCardStatus,
    ModuleCoverage, ModuleCoverageSnapshot,
};
pub use deep_map_explorer::{
    ExplorerAction, ExplorerActionSchemaVersion, ExplorerCheckpoint, ExplorerCheckpointError,
    ExplorerInspectAction, ExplorerSearchAction, ExplorerSearchActionError, ExplorerSearchKind,
    ExplorerSearchLimit, ExplorerSearchQuery, InformationGainRationale,
    InformationGainRationaleError, ModuleCardProposal, ModuleCardProposalEnvelope,
    ModuleCardProposalError, ProposedModuleCardField,
};
pub use discovery::{
    DiscoveredFile, DiscoveredFileRole, DiscoveredFileRoles, DiscoveredFileRolesError,
    DiscoveryExclusionCounts, DiscoveryExclusionReason, DiscoveryOrigin, DiscoveryPolicy,
    DiscoveryPolicyVersion, DiscoveryPolicyVersionError, DiscoveryResult, DiscoveryResultError,
};
pub use embedding::{
    EmbeddingBatchSize, EmbeddingBatchSizeError, EmbeddingCacheKey, EmbeddingDataType,
    EmbeddingDimension, EmbeddingDimensionError, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProfileTextError, EmbeddingProfileTextKind, EmbeddingProviderId,
    EmbeddingQuantization, EmbeddingTimestamp, EmbeddingTimestampError, EmbeddingVector,
    EmbeddingVectorError, EmbeddingVectorNormalization, ModelProfileId, SemanticEmbedding,
    VectorHit, VectorSearchCapability, VectorSearchLimit, VectorSearchLimitError,
    VectorSearchResult, VectorSearchResultError,
};
pub use git::{GitHead, GitObjectId, GitObjectIdError, GitReferenceName, GitReferenceNameError};
pub use graph::{
    Centrality, CentralityError, EvidenceRef, GraphEdge, GraphEndpoint, GraphSymbol,
    IndexPublication, IndexPublicationError, LinkResolution, LinkedGraph, LinkedGraphError,
    PublishedIndex, RankProjection, RankProjectionError, RankScore, RankScoreError, SymbolId,
    SymbolRank, SymbolRankSignals, UnresolvedEdgeCandidate, UnresolvedGraphTarget,
    UnresolvedReason,
};
pub use graph_retrieval::{
    GraphTraversalHit, GraphTraversalHitError, GraphTraversalResult, GraphTraversalResultError,
    GraphTraversalTarget, TraversalDepth, TraversalDepthError, TraversalDirection, TraversalQuery,
    TraversalResultLimit, TraversalResultLimitError,
};
pub use id::{
    IndexRunId, ProjectId, RemoteIdentity, RepositoryId, SnapshotId, WorktreeAnchorId, WorktreeId,
};
pub use index_run::{
    IndexRunRecord, IndexRunSequence, IndexRunSequenceError, IndexRunStart, IndexRunStatus,
    IndexRunStatusError, IndexRunTerminalOutcome, RankingPolicyVersion, RankingPolicyVersionError,
};
pub use language::{
    Confidence, ConfidenceError, DiagnosticMessage, DiagnosticMessageError,
    LanguageAdapterContractVersion, LanguageAdapterContractVersionError, LanguageParseArtifacts,
    LanguageParseResult, LanguageParseResultError, LocalSymbolId, LocalSymbolIdError,
    ParseCoverage, ParseCoverageError, ParseDiagnostic, ParseDiagnosticCode,
    ParseDiagnosticSeverity, ParsedSymbol, ParsedSymbolError, SourcePosition, SourceRange,
    SourceRangeError, SymbolKind, SymbolName, SymbolReference, SymbolReferenceError, SymbolRole,
    SymbolRoles, SymbolSignature, SymbolTextError, SymbolVisibility, SyntaxProvider,
    SyntaxRelation, SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
pub use lexical_retrieval::{
    LexicalScore, LexicalScoreError, LexicalSearchCursor, LexicalSearchExplanation,
    LexicalSearchHit, LexicalSearchPage, LexicalSearchPageError, LexicalSearchPageSize,
    LexicalSearchPageSizeError, LexicalSearchPosition, LexicalSearchQuery, LexicalSearchSymbol,
    LexicalSearchTarget, LexicalSearchTerm, LexicalSearchTermError,
};
pub use module_map::{
    ModuleId, ModuleKind, ModuleMapError, ModuleMembership, ModuleMembershipEvidence,
    ModuleMembershipKind, ModulePolicyVersion, ModulePolicyVersionError, ModuleProjection,
    ModuleRoot, ModuleSymbolSet, RepositoryCard, RepositoryModule,
};
pub use path::{CanonicalDirectory, CanonicalDirectoryError};
pub use retrieval::{
    ExactSearchCursor, ExactSearchCursorError, ExactSearchExplanation, ExactSearchHit,
    ExactSearchHitError, ExactSearchPage, ExactSearchPageError, ExactSearchPageSize,
    ExactSearchPageSizeError, ExactSearchPosition, ExactSearchQuery, ExactSearchRole,
    ExactSearchSymbol, ExactSearchTarget, ExactSearchTerm, ExactSearchTextError,
    QualifiedSymbolName, SourceChannel,
};
pub use retrieval_fusion::{
    CandidateFreshness, CandidateSetCompleteness, CandidateTokenCost, CandidateTokenCostError,
    FusedRetrievalHit, FusedRetrievalResult, FusionContribution, FusionError, FusionPolicy,
    FusionPolicyVersion, FusionPriority, FusionResultLimit, FusionResultLimitError, FusionScore,
    FusionSignalExplanation, FusionTokenExplanation, MemoryCandidateExplanation,
    MemoryCandidateExplanationError, NormalizedRetrievalSignal, NormalizedRetrievalSignalError,
    RelationshipCandidateExplanation, ResultExplanation, ResultSourceExplanation,
    RetrievalCandidate, RetrievalCandidateReason, RetrievalCandidateSet,
    RetrievalCandidateSetError, RetrievalCandidateSets, RetrievalCandidateSetsError,
    RetrievalCandidateSignals, RetrievalTargetId,
};
pub use revision::{
    FileDelta, FileRevision, RenameCandidate, RepositoryFileState, RepositoryFileStateError,
    SnapshotDelta,
};
pub use semantic_card::{
    BodyHash, NormalizedSemanticCard, SemanticCardBatch, SemanticCardBatchError, SemanticCardId,
    SemanticCardNormalizationError, SemanticCardNormalizationVersion,
};
pub use snapshot::{
    ContentHash, IndexLanguage, IndexSchemaVersion, IndexSchemaVersionError,
    LanguageAdapterRevision, LanguageAdapterVersion, LanguageAdapterVersionError, RepositoryPath,
    RepositoryPathError, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotError,
    WorktreeGeneration, WorktreeGenerationError,
};
pub use task_lens::{
    TaskLens, TaskLensClaim, TaskLensClaimError, TaskLensCompileError, TaskLensDiagnosticKind,
    TaskLensDigest, TaskLensEntry, TaskLensEntryReason, TaskLensPolicy, TaskLensPolicyVersion,
    TaskLensSeed, TaskLensSeedSet, TaskLensSeedSetError, TaskLensSeedText, TaskLensSeedTextError,
    TaskLensTarget, TaskLensTokenBudget, TaskLensTokenBudgetError, TaskLensZoomLevel,
};

use std::error::Error;
use std::fmt;

/// Stable identity and observable location of a logical Git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryIdentity {
    id: RepositoryId,
    common_directory: CanonicalDirectory,
    main_remote: Option<RemoteIdentity>,
}

impl RepositoryIdentity {
    /// Creates a repository identity from adapter-validated facts.
    #[must_use]
    pub const fn new(
        id: RepositoryId,
        common_directory: CanonicalDirectory,
        main_remote: Option<RemoteIdentity>,
    ) -> Self {
        Self {
            id,
            common_directory,
            main_remote,
        }
    }

    /// Returns the stable local repository ID.
    #[must_use]
    pub const fn id(&self) -> RepositoryId {
        self.id
    }

    /// Returns the canonical Git common directory.
    #[must_use]
    pub const fn common_directory(&self) -> &CanonicalDirectory {
        &self.common_directory
    }

    /// Returns the credential-free normalized remote fingerprint when configured.
    #[must_use]
    pub const fn main_remote(&self) -> Option<RemoteIdentity> {
        self.main_remote
    }
}

/// Stable identity and canonical root of one concrete Git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeIdentity {
    id: WorktreeId,
    anchor_id: WorktreeAnchorId,
    repository_id: RepositoryId,
    root: CanonicalDirectory,
}

impl WorktreeIdentity {
    /// Creates a worktree identity from adapter-validated facts.
    #[must_use]
    pub const fn new(
        id: WorktreeId,
        anchor_id: WorktreeAnchorId,
        repository_id: RepositoryId,
        root: CanonicalDirectory,
    ) -> Self {
        Self {
            id,
            anchor_id,
            repository_id,
            root,
        }
    }

    /// Returns the stable worktree ID.
    #[must_use]
    pub const fn id(&self) -> WorktreeId {
        self.id
    }

    /// Returns the repository-local Git metadata anchor used only as move evidence.
    #[must_use]
    pub const fn anchor_id(&self) -> WorktreeAnchorId {
        self.anchor_id
    }

    /// Returns the owning repository ID.
    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        self.repository_id
    }

    /// Returns the canonical worktree root.
    #[must_use]
    pub const fn root(&self) -> &CanonicalDirectory {
        &self.root
    }
}

/// One coherent observation of repository, worktree, and HEAD identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIdentity {
    repository: RepositoryIdentity,
    worktree: WorktreeIdentity,
    head: GitHead,
}

impl ProjectIdentity {
    /// Creates an observation only when repository and worktree ownership agree.
    pub fn new(
        repository: RepositoryIdentity,
        worktree: WorktreeIdentity,
        head: GitHead,
    ) -> Result<Self, ProjectIdentityError> {
        if repository.id() != worktree.repository_id() {
            return Err(ProjectIdentityError::RepositoryMismatch);
        }
        Ok(Self {
            repository,
            worktree,
            head,
        })
    }

    /// Returns the logical repository identity.
    #[must_use]
    pub const fn repository(&self) -> &RepositoryIdentity {
        &self.repository
    }

    /// Returns the concrete worktree identity.
    #[must_use]
    pub const fn worktree(&self) -> &WorktreeIdentity {
        &self.worktree
    }

    /// Returns the observed Git HEAD state.
    #[must_use]
    pub const fn head(&self) -> &GitHead {
        &self.head
    }
}

/// Inconsistent project identity assembled at an adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIdentityError {
    /// The worktree referenced a different repository ID.
    RepositoryMismatch,
}

impl fmt::Display for ProjectIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepositoryMismatch => {
                formatter.write_str("worktree identity belongs to a different repository")
            }
        }
    }
}

impl Error for ProjectIdentityError {}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalDirectory, GitHead, GitReferenceName, ProjectIdentity, ProjectIdentityError,
        RepositoryId, RepositoryIdentity, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };

    #[test]
    fn project_identity_rejects_cross_repository_worktree() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = std::env::current_dir()?;
        let repository = RepositoryIdentity::new(
            RepositoryId::from_bytes([1; 32]),
            CanonicalDirectory::from_canonicalized(path.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([4; 32]),
            RepositoryId::from_bytes([3; 32]),
            CanonicalDirectory::from_canonicalized(path)?,
        );
        let head = GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        };

        assert_eq!(
            ProjectIdentity::new(repository, worktree, head),
            Err(ProjectIdentityError::RepositoryMismatch)
        );
        Ok(())
    }
}
