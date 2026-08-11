//! Versioned, infrastructure-independent IPC boundary types for A^3.

mod error;
mod goal_contract;
mod health;
mod project;
mod project_status;
mod recent_projects;
mod version;

pub use error::{CommandErrorV1, ErrorCodeV1};
pub use goal_contract::{AcceptanceCriterionV1, GoalContractDraftV1, GoalContractV1};
pub use health::{HealthRequestV1, HealthResponseV1, HealthStatusV1, PlatformV1};
pub use project::{
    GitHeadV1, OpenProjectRequestV1, OpenProjectResponseV1, OpenProjectResultV1, ProjectSummaryV1,
};
pub use project_status::{
    IndexStateV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1,
    ProjectStatusResultV1, QueryProjectStatusRequestV1,
};
pub use recent_projects::{
    ListRecentProjectsRequestV1, RecentProjectSummaryV1, RecentProjectsResponseV1,
};
pub use version::ProtocolVersion;
