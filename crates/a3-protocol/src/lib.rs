//! Versioned, infrastructure-independent IPC boundary types for A^3.

mod error;
mod goal_contract;
mod health;
mod index_activity;
mod index_overview;
mod project;
mod project_rebuild;
mod project_removal;
mod project_status;
mod recent_projects;
mod version;

pub use error::{CommandErrorV1, ErrorCodeV1};
pub use goal_contract::{AcceptanceCriterionV1, GoalContractDraftV1, GoalContractV1};
pub use health::{HealthRequestV1, HealthResponseV1, HealthStatusV1, PlatformV1};
pub use index_activity::{
    IndexActivityResponseV1, IndexActivityResultV1, IndexActivityStateV1, IndexActivityV1,
    IndexPhaseV1, QueryIndexActivityRequestV1,
};
pub use index_overview::{
    IndexDiagnosticCodeV1, IndexDiagnosticSeverityV1, IndexDiagnosticV1, IndexFileDiagnosticsV1,
    IndexLanguageV1, IndexOverviewCountsV1, IndexOverviewResponseV1, IndexOverviewResultV1,
    IndexOverviewV1, QueryIndexOverviewRequestV1,
};
pub use project::{
    GitHeadV1, OpenProjectRequestV1, OpenProjectResponseV1, OpenProjectResultV1, ProjectSummaryV1,
};
pub use project_rebuild::{RebuildProjectIndexRequestV1, RebuildProjectIndexResponseV1};
pub use project_removal::{RemoveProjectRequestV1, RemoveProjectResponseV1, RemoveProjectResultV1};
pub use project_status::{
    IndexStateV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1,
    ProjectStatusResultV1, QueryProjectStatusRequestV1, RebuildStateV1,
};
pub use recent_projects::{
    ListRecentProjectsRequestV1, RecentProjectSummaryV1, RecentProjectsResponseV1,
};
pub use version::ProtocolVersion;
