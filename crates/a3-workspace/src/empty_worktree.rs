use crate::path_policy::{PathPolicy, PathPolicyError};
use a3_application::{EmptyWorktreeInitializationFailure, EmptyWorktreeInitializer};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// Isolated Git initializer for one explicitly selected empty directory.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorkspaceEmptyWorktreeInitializer;

impl WorkspaceEmptyWorktreeInitializer {
    /// Creates an initializer that never reads user Git configuration or the environment.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Initializes exactly the selected empty directory as a Git worktree with unborn `main`.
    pub fn initialize(
        &self,
        selected_root: impl AsRef<Path>,
    ) -> Result<(), EmptyWorktreeInitError> {
        let policy = PathPolicy::from_selected_root(selected_root)?;
        let root = policy.root().as_path();
        if git_metadata_exists(root) {
            return Err(EmptyWorktreeInitError::AlreadyRepository);
        }
        if !directory_is_empty(root)? {
            return Err(EmptyWorktreeInitError::DirectoryNotEmpty);
        }
        if enclosing_git_worktree(root) {
            return Err(EmptyWorktreeInitError::NotWorktreeRoot);
        }

        let options = gix::create::Options {
            destination_must_be_empty: Some(true),
            ..gix::create::Options::default()
        };
        gix::ThreadSafeRepository::init_opts(
            root,
            gix::create::Kind::WithWorktree,
            options,
            gix::open::Options::isolated(),
        )
        .map_err(EmptyWorktreeInitError::from)?;
        Ok(())
    }
}

impl EmptyWorktreeInitializer for WorkspaceEmptyWorktreeInitializer {
    fn initialize_empty_worktree(
        &self,
        selected_root: &Path,
    ) -> Result<(), EmptyWorktreeInitializationFailure> {
        self.initialize(selected_root).map_err(classify_init_error)
    }
}

fn git_metadata_exists(directory: &Path) -> bool {
    directory.join(".git").exists()
}

fn directory_is_empty(directory: &Path) -> Result<bool, EmptyWorktreeInitError> {
    let mut entries =
        fs::read_dir(directory).map_err(|source| EmptyWorktreeInitError::ReadDir {
            path: directory.to_path_buf(),
            source,
        })?;
    Ok(entries.next().is_none())
}

fn enclosing_git_worktree(selected_root: &Path) -> bool {
    let mut current = selected_root.parent();
    while let Some(directory) = current {
        if git_metadata_exists(directory) {
            return true;
        }
        current = directory.parent();
    }
    false
}

fn classify_init_error(error: EmptyWorktreeInitError) -> EmptyWorktreeInitializationFailure {
    match error {
        EmptyWorktreeInitError::PathPolicy(
            PathPolicyError::Canonicalize { .. }
            | PathPolicyError::Metadata { .. }
            | PathPolicyError::NotDirectory(_)
            | PathPolicyError::UnsupportedFileType(_)
            | PathPolicyError::InvalidCanonicalPath(_),
        )
        | EmptyWorktreeInitError::ReadDir { .. } => {
            EmptyWorktreeInitializationFailure::SelectionUnavailable
        }
        EmptyWorktreeInitError::PathPolicy(PathPolicyError::OutsideRoot { .. })
        | EmptyWorktreeInitError::NotWorktreeRoot => {
            EmptyWorktreeInitializationFailure::NotWorktreeRoot
        }
        EmptyWorktreeInitError::DirectoryNotEmpty => {
            EmptyWorktreeInitializationFailure::DirectoryNotEmpty
        }
        EmptyWorktreeInitError::AlreadyRepository => {
            EmptyWorktreeInitializationFailure::AlreadyRepository
        }
        EmptyWorktreeInitError::Git(GitInitFailure::DirectoryNotEmpty) => {
            EmptyWorktreeInitializationFailure::DirectoryNotEmpty
        }
        EmptyWorktreeInitError::Git(GitInitFailure::AlreadyRepository) => {
            EmptyWorktreeInitializationFailure::AlreadyRepository
        }
        EmptyWorktreeInitError::Git(GitInitFailure::Failed) => {
            EmptyWorktreeInitializationFailure::Failed
        }
    }
}

/// Typed failure while initializing one empty selected worktree.
#[derive(Debug)]
pub enum EmptyWorktreeInitError {
    /// Selected-root canonicalization or containment failed.
    PathPolicy(PathPolicyError),
    /// The selected directory could not be listed safely.
    ReadDir {
        /// Directory that could not be listed.
        path: std::path::PathBuf,
        /// Operating-system listing failure.
        source: io::Error,
    },
    /// The selected directory already contains files or directories.
    DirectoryNotEmpty,
    /// The selected directory already contains Git metadata.
    AlreadyRepository,
    /// An ancestor directory already contains Git metadata.
    NotWorktreeRoot,
    /// Isolated Git initialization failed after the safety checks.
    Git(GitInitFailure),
}

/// Isolated classification of a gix initialization failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitInitFailure {
    /// The destination was not empty when Git tried to initialize it.
    DirectoryNotEmpty,
    /// Git refused because `.git` already existed.
    AlreadyRepository,
    /// Git initialization failed for another isolated reason.
    Failed,
}

impl From<PathPolicyError> for EmptyWorktreeInitError {
    fn from(error: PathPolicyError) -> Self {
        Self::PathPolicy(error)
    }
}

impl From<gix::init::Error> for EmptyWorktreeInitError {
    fn from(error: gix::init::Error) -> Self {
        Self::Git(classify_gix_init_error(&error))
    }
}

fn classify_gix_init_error(error: &gix::init::Error) -> GitInitFailure {
    match error {
        gix::init::Error::Init(gix::create::Error::DirectoryNotEmpty { .. }) => {
            GitInitFailure::DirectoryNotEmpty
        }
        gix::init::Error::Init(gix::create::Error::DirectoryExists { .. }) => {
            GitInitFailure::AlreadyRepository
        }
        gix::init::Error::CurrentDir(_)
        | gix::init::Error::Init(_)
        | gix::init::Error::Open(_)
        | gix::init::Error::InvalidBranchName { .. }
        | gix::init::Error::EditHeadForDefaultBranch(_) => GitInitFailure::Failed,
    }
}

impl fmt::Display for EmptyWorktreeInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathPolicy(error) => write!(formatter, "selected root is invalid: {error}"),
            Self::ReadDir { path, source } => {
                write!(
                    formatter,
                    "could not list selected directory {}: {source}",
                    path.display()
                )
            }
            Self::DirectoryNotEmpty => formatter.write_str("selected directory is not empty"),
            Self::AlreadyRepository => {
                formatter.write_str("selected directory is already a Git repository")
            }
            Self::NotWorktreeRoot => {
                formatter.write_str("selected directory is nested in another Git worktree")
            }
            Self::Git(GitInitFailure::DirectoryNotEmpty) => {
                formatter.write_str("Git refused to initialize a non-empty directory")
            }
            Self::Git(GitInitFailure::AlreadyRepository) => {
                formatter.write_str("Git refused to overwrite an existing repository")
            }
            Self::Git(GitInitFailure::Failed) => {
                formatter.write_str("isolated Git initialization failed")
            }
        }
    }
}

impl Error for EmptyWorktreeInitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PathPolicy(error) => Some(error),
            Self::ReadDir { source, .. } => Some(source),
            Self::DirectoryNotEmpty
            | Self::AlreadyRepository
            | Self::NotWorktreeRoot
            | Self::Git(_) => None,
        }
    }
}

impl From<EmptyWorktreeInitError> for EmptyWorktreeInitializationFailure {
    fn from(error: EmptyWorktreeInitError) -> Self {
        classify_init_error(error)
    }
}
