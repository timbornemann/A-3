use crate::identity;
use crate::path_policy::{PathPolicy, PathPolicyError, canonicalize_directory};
use a3_application::{ProjectInspectionFailure, ProjectInspector};
use a3_domain::{
    GitHead, GitObjectId, GitReferenceName, ProjectIdentity, ProjectIdentityError,
    RepositoryIdentity, WorktreeIdentity,
};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// Read-only inspector for one explicitly selected local Git worktree root.
#[derive(Debug, Default, Clone, Copy)]
pub struct RepositoryInspector;

impl RepositoryInspector {
    /// Creates an inspector with no ambient configuration or network capability.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Opens exactly the selected root and derives its repository, worktree, and HEAD identities.
    pub fn inspect(
        &self,
        selected_root: impl AsRef<Path>,
    ) -> Result<ProjectIdentity, RepositoryInspectionError> {
        let policy = PathPolicy::from_selected_root(selected_root)?;
        let repository = gix::open_opts(
            policy.root().as_path().to_path_buf(),
            gix::open::Options::isolated(),
        )
        .map_err(|_| RepositoryInspectionError::GitRepositoryOpen)?;

        let worktree_root = repository
            .workdir()
            .ok_or(RepositoryInspectionError::BareRepository)
            .and_then(|path| canonicalize_directory(path).map_err(Into::into))?;
        if worktree_root != *policy.root() {
            return Err(RepositoryInspectionError::SelectedPathIsNotWorktreeRoot {
                selected: policy.root().as_path().to_path_buf(),
                actual: worktree_root.into_path_buf(),
            });
        }

        let common_directory = canonicalize_directory(repository.common_dir())?;
        let repository_id = identity::repository_id(&common_directory);
        let main_remote = inspect_main_remote(&repository)?;
        let repository_identity =
            RepositoryIdentity::new(repository_id, common_directory, main_remote);
        let worktree_identity = WorktreeIdentity::new(
            identity::worktree_id(repository_id, &worktree_root),
            repository_id,
            worktree_root,
        );
        let head = inspect_head(&repository)?;

        ProjectIdentity::new(repository_identity, worktree_identity, head).map_err(Into::into)
    }
}

impl ProjectInspector for RepositoryInspector {
    fn inspect_project(
        &self,
        selected_root: &Path,
    ) -> Result<ProjectIdentity, ProjectInspectionFailure> {
        self.inspect(selected_root)
            .map_err(classify_inspection_error)
    }
}

fn classify_inspection_error(error: RepositoryInspectionError) -> ProjectInspectionFailure {
    match error {
        RepositoryInspectionError::PathPolicy(
            PathPolicyError::Canonicalize { .. }
            | PathPolicyError::Metadata { .. }
            | PathPolicyError::NotDirectory(_)
            | PathPolicyError::UnsupportedFileType(_)
            | PathPolicyError::InvalidCanonicalPath(_),
        ) => ProjectInspectionFailure::SelectionUnavailable,
        RepositoryInspectionError::PathPolicy(PathPolicyError::OutsideRoot { .. })
        | RepositoryInspectionError::SelectedPathIsNotWorktreeRoot { .. } => {
            ProjectInspectionFailure::NotWorktreeRoot
        }
        RepositoryInspectionError::GitRepositoryOpen => ProjectInspectionFailure::NotRepository,
        RepositoryInspectionError::BareRepository => {
            ProjectInspectionFailure::UnsupportedRepository
        }
        RepositoryInspectionError::InvalidHead
        | RepositoryInspectionError::InvalidRemoteConfiguration
        | RepositoryInspectionError::InvalidIdentity(_) => {
            ProjectInspectionFailure::InvalidRepositoryMetadata
        }
    }
}

fn inspect_head(repository: &gix::Repository) -> Result<GitHead, RepositoryInspectionError> {
    let head = repository
        .head()
        .map_err(|_| RepositoryInspectionError::InvalidHead)?;
    let reference = head
        .referent_name()
        .map(|name| GitReferenceName::try_from_full_name(name.to_string()))
        .transpose()
        .map_err(|_| RepositoryInspectionError::InvalidHead)?;

    if head.is_unborn() {
        return reference
            .map(|reference| GitHead::Unborn { reference })
            .ok_or(RepositoryInspectionError::InvalidHead);
    }

    let object_id = head
        .id()
        .map(|id| GitObjectId::try_from_hex(id.detach().to_string()))
        .transpose()
        .map_err(|_| RepositoryInspectionError::InvalidHead)?
        .ok_or(RepositoryInspectionError::InvalidHead)?;
    Ok(GitHead::Born {
        object_id,
        reference,
    })
}

fn inspect_main_remote(
    repository: &gix::Repository,
) -> Result<Option<a3_domain::RemoteIdentity>, RepositoryInspectionError> {
    let Some(remote) = repository.find_default_remote(gix::remote::Direction::Fetch) else {
        return Ok(None);
    };
    let remote = remote.map_err(|_| RepositoryInspectionError::InvalidRemoteConfiguration)?;
    Ok(remote
        .url(gix::remote::Direction::Fetch)
        .map(identity::remote_identity))
}

/// Safe, typed failure while inspecting selected repository metadata.
#[derive(Debug)]
pub enum RepositoryInspectionError {
    /// Selected-root canonicalization or containment failed.
    PathPolicy(PathPolicyError),
    /// The selected directory could not be opened as a Git repository.
    GitRepositoryOpen,
    /// A bare repository has no worktree and cannot be opened as a project.
    BareRepository,
    /// Git resolved a parent worktree, so accepting it would expand the selected root implicitly.
    SelectedPathIsNotWorktreeRoot {
        /// Canonical directory selected by the user.
        selected: PathBuf,
        /// Canonical worktree root reported by Git.
        actual: PathBuf,
    },
    /// HEAD metadata was invalid or inconsistent.
    InvalidHead,
    /// The local repository's primary remote configuration was invalid.
    InvalidRemoteConfiguration,
    /// Derived domain identities were internally inconsistent.
    InvalidIdentity(ProjectIdentityError),
}

impl fmt::Display for RepositoryInspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathPolicy(error) => write!(formatter, "selected root is invalid: {error}"),
            Self::GitRepositoryOpen => {
                formatter.write_str("selected root is not an accessible Git repository")
            }
            Self::BareRepository => formatter.write_str("bare repositories are not worktrees"),
            Self::SelectedPathIsNotWorktreeRoot { selected, actual } => write!(
                formatter,
                "selected directory {} is not the worktree root {}",
                selected.display(),
                actual.display()
            ),
            Self::InvalidHead => formatter.write_str("repository HEAD metadata is invalid"),
            Self::InvalidRemoteConfiguration => {
                formatter.write_str("repository remote configuration is invalid")
            }
            Self::InvalidIdentity(error) => {
                write!(formatter, "project identity is invalid: {error}")
            }
        }
    }
}

impl Error for RepositoryInspectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PathPolicy(error) => Some(error),
            Self::InvalidIdentity(error) => Some(error),
            Self::GitRepositoryOpen
            | Self::BareRepository
            | Self::SelectedPathIsNotWorktreeRoot { .. }
            | Self::InvalidHead
            | Self::InvalidRemoteConfiguration => None,
        }
    }
}

impl From<PathPolicyError> for RepositoryInspectionError {
    fn from(error: PathPolicyError) -> Self {
        Self::PathPolicy(error)
    }
}

impl From<ProjectIdentityError> for RepositoryInspectionError {
    fn from(error: ProjectIdentityError) -> Self {
        Self::InvalidIdentity(error)
    }
}
