//! Safe filesystem and local Git inspection adapters for A^3 worktrees.

mod agent_source_reader;
mod identity;
mod path_policy;
mod platform_path;
mod repository;

pub use agent_source_reader::WorkspaceAgentSourceReader;
pub use path_policy::{CanonicalWorkspacePath, PathEntryKind, PathPolicy, PathPolicyError};
pub use repository::{RepositoryInspectionError, RepositoryInspector};
