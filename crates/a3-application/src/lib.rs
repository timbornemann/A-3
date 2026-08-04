//! Application use cases and ports for A^3.

mod health_query;
mod jobs;
mod knowledge_store;
mod open_project;
mod recent_projects;

pub use health_query::{GetHealth, HealthQuery};
pub use jobs::{
    CancellationToken, JobCancelResult, JobCancellationError, JobClock, JobCompletion, JobContext,
    JobEvent, JobEventKind, JobEventSequence, JobEventStream, JobEventStreamClosed, JobScheduler,
    JobSchedulerConfig, JobSchedulerConfigError, JobSchedulerCreateError,
    JobSchedulerShutdownError, JobSchedulerSubmitError, JobSnapshot, JobTask, JobTimestamp,
    ProgressReportError, ShutdownMode, ShutdownReport,
};
pub use knowledge_store::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectPathDisplay,
    ProjectPathDisplayError, RecentProject, RecentProjectLimit, RecentProjectLimitError,
};
pub use open_project::{
    OpenProject, OpenProjectError, OpenProjectOutcome, ProjectDirectoryPicker,
    ProjectDirectorySelectionError, ProjectInspectionFailure, ProjectInspector,
};
pub use recent_projects::{ListRecentProjects, ListRecentProjectsError};
