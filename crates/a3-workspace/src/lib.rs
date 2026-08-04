//! Safe filesystem and local Git inspection adapters for A^3 worktrees.

mod identity;
mod path_policy;
mod platform_path;
mod repository;

pub use path_policy::{CanonicalWorkspacePath, PathEntryKind, PathPolicy, PathPolicyError};
pub use repository::{RepositoryInspectionError, RepositoryInspector};
