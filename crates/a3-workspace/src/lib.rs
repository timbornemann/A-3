//! Safe filesystem and local Git inspection adapters for A^3 worktrees.

mod agent_source_reader;
mod identity;
mod path_policy;
mod platform_path;
mod process_environment;
mod process_launch;
mod process_output;
mod process_runner;
mod repository;
mod secure_file;
mod workspace_directory_lister;
mod workspace_patch;

pub use agent_source_reader::WorkspaceAgentSourceReader;
pub use path_policy::{CanonicalWorkspacePath, PathEntryKind, PathPolicy, PathPolicyError};
pub use process_environment::{ProcessHostEnvironment, ProcessHostEnvironmentError};
pub use process_runner::WorkspaceProcessRunner;
pub use repository::{RepositoryInspectionError, RepositoryInspector};
pub use workspace_directory_lister::IndexedWorkspaceDirectoryLister;
pub use workspace_patch::WorkspacePatchAdapter;
