use a3_domain::{GitHead, ProjectId, ProjectIdentity, RepositoryId, WorktreeId};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

const MAX_PROJECT_PATH_DISPLAY_CHARS: usize = 32_768;
const MAX_RECENT_PROJECTS: u8 = 50;

/// Owned future returned by the object-safe persistence port.
pub type KnowledgeStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KnowledgeStoreFailure>> + Send + 'a>>;

/// Persistence port derived from project-open and recent-project use cases.
///
/// Concrete database handles, SQL, rows, and engine-specific errors remain in adapters.
pub trait KnowledgeStore: fmt::Debug + Send + Sync {
    /// Atomically records one safely inspected project as the most recently opened worktree.
    fn record_opened_project<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeStoreFuture<'a, ProjectId>;

    /// Returns a bounded, most-recent-first projection without persistence rows or raw paths.
    fn list_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>>;
}

/// Maximum number of recent project projections requested from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentProjectLimit(u8);

impl RecentProjectLimit {
    /// Default number of recent projects displayed by the desktop shell.
    pub const DEFAULT: Self = Self(10);

    /// Creates a non-zero limit capped at the product boundary.
    pub fn new(value: u8) -> Result<Self, RecentProjectLimitError> {
        if value == 0 || value > MAX_RECENT_PROJECTS {
            return Err(RecentProjectLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded integer representation.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Invalid recent-project query limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentProjectLimitError {
    value: u8,
}

impl fmt::Display for RecentProjectLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recent project limit {} must be between 1 and {MAX_RECENT_PROJECTS}",
            self.value
        )
    }
}

impl Error for RecentProjectLimitError {}

/// Bounded, non-authoritative project path intended only for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPathDisplay(String);

impl ProjectPathDisplay {
    /// Produces a bounded display from an adapter-validated operating-system path.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        let value = path
            .to_string_lossy()
            .chars()
            .take(MAX_PROJECT_PATH_DISPLAY_CHARS)
            .map(|character| {
                if character.is_control() {
                    '\u{fffd}'
                } else {
                    character
                }
            })
            .collect();
        Self(value)
    }

    /// Validates a display value reconstructed from durable storage.
    pub fn try_from_stored(value: String) -> Result<Self, ProjectPathDisplayError> {
        let length = value.chars().count();
        if length == 0 || length > MAX_PROJECT_PATH_DISPLAY_CHARS {
            return Err(ProjectPathDisplayError::InvalidLength(length));
        }
        if value.chars().any(char::is_control) {
            return Err(ProjectPathDisplayError::ControlCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the safe display text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid display text reconstructed from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectPathDisplayError {
    /// The stored display was empty or exceeded its fixed character budget.
    InvalidLength(usize),
    /// The stored display contained an unsafe control character.
    ControlCharacter,
}

impl fmt::Display for ProjectPathDisplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "project path display has invalid length {length}"
                )
            }
            Self::ControlCharacter => {
                formatter.write_str("project path display contains a control character")
            }
        }
    }
}

impl Error for ProjectPathDisplayError {}

/// Storage-independent projection of one recently opened worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProject {
    project_id: ProjectId,
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    worktree_root_display: ProjectPathDisplay,
    head: GitHead,
}

impl RecentProject {
    /// Creates a projection after an adapter has validated every persisted field.
    #[must_use]
    pub const fn new(
        project_id: ProjectId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
        worktree_root_display: ProjectPathDisplay,
        head: GitHead,
    ) -> Self {
        Self {
            project_id,
            repository_id,
            worktree_id,
            worktree_root_display,
            head,
        }
    }

    /// Returns the stable catalog project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the repository identity observed for this worktree.
    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        self.repository_id
    }

    /// Returns the concrete worktree identity.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the non-authoritative, bounded path display.
    #[must_use]
    pub const fn worktree_root_display(&self) -> &ProjectPathDisplay {
        &self.worktree_root_display
    }

    /// Returns the HEAD state observed at the most recent successful open.
    #[must_use]
    pub const fn head(&self) -> &GitHead {
        &self.head
    }
}

/// Stable application classification of persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeStoreFailure {
    /// Local application storage could not be reached or written.
    Unavailable,
    /// The database failed its integrity checks.
    Corrupt,
    /// The database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated the versioned logical schema.
    InvalidStoredData,
    /// Stored and observed identities conflict.
    IdentityConflict,
}

impl fmt::Display for KnowledgeStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("local knowledge storage is unavailable"),
            Self::Corrupt => formatter.write_str("local knowledge storage is corrupt"),
            Self::UnsupportedSchema => {
                formatter.write_str("local knowledge storage uses an unsupported schema")
            }
            Self::InvalidStoredData => {
                formatter.write_str("local knowledge storage contains invalid data")
            }
            Self::IdentityConflict => {
                formatter.write_str("stored project identity conflicts with the observation")
            }
        }
    }
}

impl Error for KnowledgeStoreFailure {}

#[cfg(test)]
mod tests {
    use super::{ProjectPathDisplay, ProjectPathDisplayError, RecentProjectLimit};
    use std::path::Path;

    #[test]
    fn recent_project_limit_is_non_zero_and_bounded() {
        assert!(RecentProjectLimit::new(0).is_err());
        assert_eq!(
            RecentProjectLimit::new(50).map(RecentProjectLimit::get),
            Ok(50)
        );
        assert!(RecentProjectLimit::new(51).is_err());
    }

    #[test]
    fn path_display_is_bounded_and_sanitized() {
        let value = format!("/root/\n{}", "a".repeat(40_000));
        let display = ProjectPathDisplay::from_path(Path::new(&value));

        assert_eq!(display.as_str().chars().count(), 32_768);
        assert!(!display.as_str().chars().any(char::is_control));
    }

    #[test]
    fn stored_path_display_rejects_control_characters() {
        assert_eq!(
            ProjectPathDisplay::try_from_stored("unsafe\npath".to_owned()),
            Err(ProjectPathDisplayError::ControlCharacter)
        );
    }
}
