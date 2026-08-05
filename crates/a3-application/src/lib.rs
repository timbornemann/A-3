//! Application use cases and ports for A^3.

mod exact_search;
mod health_query;
mod jobs;
mod knowledge_index_store;
mod knowledge_search_store;
mod knowledge_store;
mod language_adapter;
mod open_project;
mod project_reconciliation;
mod recent_projects;
mod repository_discovery;
mod repository_index;
mod repository_snapshot;
mod repository_watcher;

pub use exact_search::SearchExactIndex;
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
    ExactSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
};
pub use knowledge_store::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectPathDisplay,
    ProjectPathDisplayError, RecentProject, RecentProjectLimit, RecentProjectLimitError,
};
pub use language_adapter::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
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
