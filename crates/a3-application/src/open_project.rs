use crate::{KnowledgeStore, KnowledgeStoreFailure};
use a3_domain::{ProjectId, ProjectIdentity};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Outbound port for one explicit, user-controlled native directory selection.
pub trait ProjectDirectoryPicker: fmt::Debug + Send + Sync {
    /// Returns the selected directory, `None` on user cancellation, or a typed adapter failure.
    fn pick_project_directory(&self) -> Result<Option<PathBuf>, ProjectDirectorySelectionError>;
}

/// Outbound port for validating and identifying one selected local Git worktree.
pub trait ProjectInspector: fmt::Debug + Send + Sync {
    /// Inspects exactly the selected path without expanding its authority boundary.
    fn inspect_project(
        &self,
        selected_root: &Path,
    ) -> Result<ProjectIdentity, ProjectInspectionFailure>;
}

/// Application use case for explicitly selecting and opening one project worktree.
#[derive(Debug)]
pub struct OpenProject {
    picker: Arc<dyn ProjectDirectoryPicker>,
    inspector: Arc<dyn ProjectInspector>,
    store: Arc<dyn KnowledgeStore>,
}

impl OpenProject {
    /// Wires the native selection and repository-inspection ports.
    #[must_use]
    pub fn new(
        picker: Arc<dyn ProjectDirectoryPicker>,
        inspector: Arc<dyn ProjectInspector>,
        store: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            picker,
            inspector,
            store,
        }
    }

    /// Opens the explicitly selected project or reports user cancellation without inspection.
    pub async fn execute(&self) -> Result<OpenProjectOutcome, OpenProjectError> {
        let Some(selected_root) = self
            .picker
            .pick_project_directory()
            .map_err(OpenProjectError::DirectorySelection)?
        else {
            return Ok(OpenProjectOutcome::Cancelled);
        };

        let project = self
            .inspector
            .inspect_project(&selected_root)
            .map_err(OpenProjectError::Inspection)?;
        let project_id = self
            .store
            .record_opened_project(&project)
            .await
            .map_err(OpenProjectError::Storage)?;
        Ok(OpenProjectOutcome::Opened {
            project: Box::new(project),
            project_id,
        })
    }
}

/// Successful result of one project-open request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenProjectOutcome {
    /// The user dismissed the native directory picker.
    Cancelled,
    /// The selected directory was safely identified as a Git worktree.
    Opened {
        /// Safely inspected repository and worktree identity.
        project: Box<ProjectIdentity>,
        /// Stable catalog identity assigned by the persistence port.
        project_id: ProjectId,
    },
}

/// Failure to convert a native picker result into a local operating-system path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectDirectorySelectionError {
    /// The native picker returned a non-local or otherwise unusable selection.
    InvalidNativeSelection,
}

impl fmt::Display for ProjectDirectorySelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNativeSelection => {
                formatter.write_str("native project selection was not a local directory path")
            }
        }
    }
}

impl Error for ProjectDirectorySelectionError {}

/// Stable application-level classification of repository-inspection failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectInspectionFailure {
    /// The selected filesystem entry disappeared or could not be safely canonicalized.
    SelectionUnavailable,
    /// The selected directory is not a Git repository root.
    NotRepository,
    /// Git resolved a different worktree root than the directory explicitly selected.
    NotWorktreeRoot,
    /// The selected repository shape is intentionally unsupported.
    UnsupportedRepository,
    /// Required local Git identity metadata was malformed or inconsistent.
    InvalidRepositoryMetadata,
}

impl fmt::Display for ProjectInspectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelectionUnavailable => {
                formatter.write_str("selected project directory is unavailable")
            }
            Self::NotRepository => {
                formatter.write_str("selected directory is not a Git repository")
            }
            Self::NotWorktreeRoot => {
                formatter.write_str("selected directory is not the Git worktree root")
            }
            Self::UnsupportedRepository => {
                formatter.write_str("selected Git repository shape is unsupported")
            }
            Self::InvalidRepositoryMetadata => {
                formatter.write_str("selected repository metadata is invalid")
            }
        }
    }
}

impl Error for ProjectInspectionFailure {}

/// Failure while executing the project-open use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenProjectError {
    /// The native directory picker did not yield a usable local path.
    DirectorySelection(ProjectDirectorySelectionError),
    /// The selected path failed safe repository inspection.
    Inspection(ProjectInspectionFailure),
    /// The inspected project could not be recorded durably.
    Storage(KnowledgeStoreFailure),
}

impl fmt::Display for OpenProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectorySelection(error) => {
                write!(formatter, "project selection failed: {error}")
            }
            Self::Inspection(error) => write!(formatter, "project inspection failed: {error}"),
            Self::Storage(error) => write!(formatter, "project storage failed: {error}"),
        }
    }
}

impl Error for OpenProjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DirectorySelection(error) => Some(error),
            Self::Inspection(error) => Some(error),
            Self::Storage(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OpenProject, OpenProjectError, OpenProjectOutcome, ProjectDirectoryPicker,
        ProjectDirectorySelectionError, ProjectInspectionFailure, ProjectInspector,
    };
    use crate::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, RecentProject,
        RecentProjectLimit,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId,
        RepositoryIdentity, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct FixedPicker(Option<PathBuf>);

    impl ProjectDirectoryPicker for FixedPicker {
        fn pick_project_directory(
            &self,
        ) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
            Ok(self.0.clone())
        }
    }

    #[derive(Debug)]
    struct FixedInspector {
        calls: Arc<AtomicUsize>,
        project: ProjectIdentity,
    }

    impl ProjectInspector for FixedInspector {
        fn inspect_project(
            &self,
            _selected_root: &Path,
        ) -> Result<ProjectIdentity, ProjectInspectionFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.project.clone())
        }
    }

    #[derive(Debug)]
    struct RecordingStore {
        calls: Arc<AtomicUsize>,
        result: Result<ProjectId, KnowledgeStoreFailure>,
    }

    impl KnowledgeStore for RecordingStore {
        fn record_opened_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self.result;
            Box::pin(async move { result })
        }

        fn list_recent_projects(
            &self,
            _limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[test]
    fn cancellation_never_inspects_an_ambient_directory() -> Result<(), Box<dyn std::error::Error>>
    {
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let storage_calls = Arc::new(AtomicUsize::new(0));
        let use_case = OpenProject::new(
            Arc::new(FixedPicker(None)),
            Arc::new(FixedInspector {
                calls: Arc::clone(&inspection_calls),
                project: fixture_project()?,
            }),
            Arc::new(RecordingStore {
                calls: Arc::clone(&storage_calls),
                result: Ok(ProjectId::from_bytes([3; 32])),
            }),
        );

        assert_eq!(block_on(use_case.execute())?, OpenProjectOutcome::Cancelled);
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 0);
        assert_eq!(storage_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn selected_directory_is_inspected_exactly_once() -> Result<(), Box<dyn std::error::Error>> {
        let selected_root = std::env::current_dir()?;
        let project = fixture_project()?;
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let storage_calls = Arc::new(AtomicUsize::new(0));
        let project_id = ProjectId::from_bytes([3; 32]);
        let use_case = OpenProject::new(
            Arc::new(FixedPicker(Some(selected_root))),
            Arc::new(FixedInspector {
                calls: Arc::clone(&inspection_calls),
                project: project.clone(),
            }),
            Arc::new(RecordingStore {
                calls: Arc::clone(&storage_calls),
                result: Ok(project_id),
            }),
        );

        assert_eq!(
            block_on(use_case.execute())?,
            OpenProjectOutcome::Opened {
                project: Box::new(project),
                project_id,
            }
        );
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 1);
        assert_eq!(storage_calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn storage_failure_prevents_a_false_opened_outcome() -> Result<(), Box<dyn std::error::Error>> {
        let use_case = OpenProject::new(
            Arc::new(FixedPicker(Some(std::env::current_dir()?))),
            Arc::new(FixedInspector {
                calls: Arc::new(AtomicUsize::new(0)),
                project: fixture_project()?,
            }),
            Arc::new(RecordingStore {
                calls: Arc::new(AtomicUsize::new(0)),
                result: Err(KnowledgeStoreFailure::Unavailable),
            }),
        );

        assert_eq!(
            block_on(use_case.execute()),
            Err(OpenProjectError::Storage(
                KnowledgeStoreFailure::Unavailable
            ))
        );
        Ok(())
    }

    fn fixture_project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = CanonicalDirectory::from_canonicalized(std::env::current_dir()?)?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, root.clone(), None),
            WorktreeIdentity::new(WorktreeId::from_bytes([2; 32]), repository_id, root),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }
}
