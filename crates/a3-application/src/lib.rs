//! Application use cases and ports for A^3.

mod deep_map_action_codec;
mod deep_map_explorer;
mod deep_map_planner;
mod deep_map_read_tools;
mod embedding_provider;
mod exact_search;
mod explorer_model_provider;
mod graph_traversal;
mod health_query;
mod jobs;
mod knowledge_index_store;
mod knowledge_search_store;
mod knowledge_store;
mod language_adapter;
mod lexical_search;
mod module_card_claim_codec;
mod module_card_verification;
mod open_project;
mod project_reconciliation;
mod recent_projects;
mod repository_discovery;
mod repository_index;
mod repository_snapshot;
mod repository_watcher;
mod retrieval_fusion;
mod semantic_embedding_store;
mod semantic_embeddings;
mod task_lens;

pub use deep_map_action_codec::{
    DecodeExplorerAction, ExplorerActionDecodeError, ExplorerActionJsonSchema,
};
pub use deep_map_explorer::{
    DeepMapExplorerFailure, DeepMapExplorerFuture, DeepMapExplorerOutcome, DeepMapExplorerStatus,
    ExploreDeepMap,
};
pub use deep_map_planner::PlanDeepMap;
pub use deep_map_read_tools::{
    DeepMapReadControl, DeepMapReadFailure, DeepMapReadFuture, DeepMapReadTimeout,
    DeepMapReadTools, ExplorerObservation, ExplorerObservationError, ExplorerObservationStatus,
};
pub use embedding_provider::{
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
    EmbeddingProviderFuture, EmbeddingRequestTimeout, EmbeddingRequestTimeoutError,
    RawEmbeddingBatch, RawEmbeddingBatchError,
};
pub use exact_search::SearchExactIndex;
pub use explorer_model_provider::{
    ExplorerModelControl, ExplorerModelFailure, ExplorerModelFuture, ExplorerModelProvider,
    ExplorerModelRequest, ExplorerModelRequestPhase, ExplorerModelTimeout,
    ExplorerModelTimeoutError, ExplorerRepairReason, RawExplorerModelOutput,
    RawExplorerModelOutputError,
};
pub use graph_traversal::TraverseKnowledgeGraph;
pub use health_query::{GetHealth, HealthQuery};
pub use jobs::{
    CancellationToken, JobCancelResult, JobCancellationError, JobClock, JobCompletion, JobContext,
    JobEvent, JobEventKind, JobEventSequence, JobEventStream, JobEventStreamClosed, JobScheduler,
    JobSchedulerConfig, JobSchedulerConfigError, JobSchedulerCreateError,
    JobSchedulerShutdownError, JobSchedulerSubmitError, JobSnapshot, JobSubmitter, JobTask,
    JobTimestamp, ProgressReportError, ShutdownMode, ShutdownReport,
};
pub use knowledge_index_store::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexFailure,
    KnowledgeIndexFuture, KnowledgeIndexStore,
};
pub use knowledge_search_store::{
    KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
};
pub use knowledge_store::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectPathDisplay,
    ProjectPathDisplayError, RecentProject, RecentProjectLimit, RecentProjectLimitError,
};
pub use language_adapter::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
};
pub use lexical_search::SearchLexicalIndex;
pub use module_card_claim_codec::{
    DecodeModuleCardClaims, ModuleCardClaimDecodeError, ModuleCardClaimJsonSchema,
};
pub use module_card_verification::{
    ModuleCardEvidenceResolutionTimeout, ModuleCardEvidenceResolutionTimeoutError,
    ModuleCardEvidenceResolver, ModuleCardEvidenceResolverFailure,
    ModuleCardEvidenceResolverFuture, ModuleCardPublicationTimeout,
    ModuleCardPublicationTimeoutError, ModuleCardVerificationControl,
    ModuleCardVerificationControlError, PublishVerifiedModuleCards,
    PublishVerifiedModuleCardsFailure, PublishedIndexEvidenceResolver, PublishedModuleCardReceipt,
    VerifiedModuleCardPublisher, VerifiedModuleCardPublisherFailure,
    VerifiedModuleCardPublisherFuture, VerifyModuleCards, VerifyModuleCardsFailure,
};
pub use open_project::{
    OpenProject, OpenProjectError, OpenProjectOutcome, ProjectDirectoryPicker,
    ProjectDirectorySelectionError, ProjectInspectionFailure, ProjectInspector,
    ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
    ProjectReconciliationConfirmer,
};
pub use project_reconciliation::{
    ProjectCatalogRevision, ProjectCatalogRevisionError, ProjectOpenPreparation,
    ProjectReconciliationEvidence, ProjectReconciliationProposal,
};
pub use recent_projects::{ListRecentProjects, ListRecentProjectsError};
pub use repository_discovery::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositoryDiscoveryFailure,
};
pub use repository_index::{
    IndexRunIdFactory, IndexRunIdFactoryFailure, RefreshRepositoryIndex,
    RefreshRepositoryIndexError, RepositoryIndexCompilation, RepositoryIndexCompiler,
    RepositoryIndexCompilerFailure, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryIndexMode, RepositoryIndexRefresh,
};
pub use repository_snapshot::{
    IncrementalRepositorySnapshotBuild, IncrementalRepositorySnapshotBuilder,
    RepositorySnapshotBuild, RepositorySnapshotBuilder, RepositorySnapshotControl,
    RepositorySnapshotControlError, RepositorySnapshotFailure, RepositorySnapshotPolicy,
    SnapshotBaseline, SnapshotBaselineError, SnapshotCompatibility, SnapshotCompatibilityError,
};
pub use repository_watcher::{
    RepositoryChangeBatch, RepositoryChangeBatchError, RepositoryRescanReason,
};
pub use retrieval_fusion::FuseRetrievalCandidates;
pub use semantic_embedding_store::{
    SemanticCacheRebuildControl, SemanticCacheRebuildProgressError, SemanticEmbeddingStore,
    SemanticEmbeddingStoreFailure, SemanticEmbeddingStoreFuture,
};
pub use semantic_embeddings::{
    EmbeddingClock, EmbeddingClockFailure, EmbeddingExecutionMode, EmbeddingProgressError,
    GenerateSemanticEmbeddings, GenerateSemanticEmbeddingsError, GenerateSemanticEmbeddingsOutcome,
    SemanticEmbeddingBatchJob, SemanticEmbeddingJobControl,
};
pub use task_lens::{
    CompileTaskLens, CompileTaskLensFailure, TaskLensClaimLimit, TaskLensClaimResult,
    TaskLensClaimResultError, TaskLensClaimStore, TaskLensClaimStoreFailure,
    TaskLensClaimStoreFuture, TaskLensControl, TaskLensControlError, TaskLensSemanticHit,
    TaskLensSemanticLimit, TaskLensSemanticResult, TaskLensSemanticResultError,
    TaskLensSemanticSearch, TaskLensSemanticSearchFailure, TaskLensSemanticSearchFuture,
    TaskLensTimeout, TaskLensTimeoutError,
};
