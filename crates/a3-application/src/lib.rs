//! Application use cases and ports for A^3.

mod embedding_provider;
mod exact_search;
mod graph_traversal;
mod health_query;
mod jobs;
mod knowledge_index_store;
mod knowledge_search_store;
mod knowledge_store;
mod language_adapter;
mod lexical_search;
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

pub use embedding_provider::{
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
    EmbeddingProviderFuture, EmbeddingRequestTimeout, EmbeddingRequestTimeoutError,
    RawEmbeddingBatch, RawEmbeddingBatchError,
};
pub use exact_search::SearchExactIndex;
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
