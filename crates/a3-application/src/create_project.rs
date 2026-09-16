use crate::open_project::complete_project_catalog;
use crate::{
    KnowledgeStore, KnowledgeStoreFailure, OpenProjectError, OpenProjectOutcome,
    ProjectDirectoryPicker, ProjectDirectorySelectionError, ProjectInspectionFailure,
    ProjectInspector, ProjectReconciliationConfirmationError, ProjectReconciliationConfirmer,
};
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// Outbound port that initializes one empty selected directory as a local Git worktree.
pub trait EmptyWorktreeInitializer: fmt::Debug + Send + Sync {
    /// Creates repository metadata in `selected_root` without expanding its authority boundary.
    fn initialize_empty_worktree(
        &self,
        selected_root: &Path,
    ) -> Result<(), EmptyWorktreeInitializationFailure>;
}

/// Stable application classification of empty-worktree initialization failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyWorktreeInitializationFailure {
    /// The selected filesystem entry disappeared or could not be safely canonicalized.
    SelectionUnavailable,
    /// The selected directory already contains files or directories.
    DirectoryNotEmpty,
    /// The selected directory already contains Git metadata.
    AlreadyRepository,
    /// Git resolved a different worktree root than the directory explicitly selected.
    NotWorktreeRoot,
    /// Isolated Git initialization failed after the safety checks.
    Failed,
}

impl fmt::Display for EmptyWorktreeInitializationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelectionUnavailable => {
                formatter.write_str("selected project directory is unavailable")
            }
            Self::DirectoryNotEmpty => formatter.write_str("selected directory is not empty"),
            Self::AlreadyRepository => {
                formatter.write_str("selected directory is already a Git repository")
            }
            Self::NotWorktreeRoot => {
                formatter.write_str("selected directory is nested in another Git worktree")
            }
            Self::Failed => formatter.write_str("empty Git worktree initialization failed"),
        }
    }
}

impl Error for EmptyWorktreeInitializationFailure {}

/// Application use case for creating one empty local Git worktree and cataloging it.
#[derive(Debug)]
pub struct CreateProject {
    picker: Arc<dyn ProjectDirectoryPicker>,
    inspector: Arc<dyn ProjectInspector>,
    initializer: Arc<dyn EmptyWorktreeInitializer>,
    reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
    store: Arc<dyn KnowledgeStore>,
}

impl CreateProject {
    /// Wires native selection, empty-worktree initialization, and catalog registration.
    #[must_use]
    pub fn new(
        picker: Arc<dyn ProjectDirectoryPicker>,
        inspector: Arc<dyn ProjectInspector>,
        initializer: Arc<dyn EmptyWorktreeInitializer>,
        reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            picker,
            inspector,
            initializer,
            reconciliation_confirmer,
            store,
        }
    }

    /// Creates an empty Git worktree in the selected directory or reports cancellation.
    pub async fn execute(&self) -> Result<OpenProjectOutcome, CreateProjectError> {
        let Some(selected_root) = self
            .picker
            .pick_project_directory()
            .map_err(CreateProjectError::DirectorySelection)?
        else {
            return Ok(OpenProjectOutcome::Cancelled);
        };

        match self.inspector.inspect_project(&selected_root) {
            Ok(_) => return Err(CreateProjectError::AlreadyGitRepository),
            Err(ProjectInspectionFailure::NotRepository) => {}
            Err(failure) => return Err(CreateProjectError::Inspection(failure)),
        }

        self.initializer
            .initialize_empty_worktree(&selected_root)
            .map_err(CreateProjectError::Initialization)?;

        let project = self
            .inspector
            .inspect_project(&selected_root)
            .map_err(CreateProjectError::Inspection)?;
        complete_project_catalog(
            project,
            self.reconciliation_confirmer.as_ref(),
            self.store.as_ref(),
        )
        .await
        .map_err(CreateProjectError::from)
    }
}

/// Failure while executing the empty-project create use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateProjectError {
    /// The native directory picker did not yield a usable local path.
    DirectorySelection(ProjectDirectorySelectionError),
    /// The selected path failed safe repository inspection.
    Inspection(ProjectInspectionFailure),
    /// The selected directory is already a Git worktree.
    AlreadyGitRepository,
    /// Isolated Git initialization refused or failed the selected directory.
    Initialization(EmptyWorktreeInitializationFailure),
    /// The native move-confirmation dialog failed safely.
    ReconciliationConfirmation(ProjectReconciliationConfirmationError),
    /// The inspected project could not be recorded durably.
    Storage(KnowledgeStoreFailure),
}

impl fmt::Display for CreateProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectorySelection(error) => {
                write!(formatter, "project selection failed: {error}")
            }
            Self::Inspection(error) => write!(formatter, "project inspection failed: {error}"),
            Self::AlreadyGitRepository => {
                formatter.write_str("selected directory is already a Git repository")
            }
            Self::Initialization(error) => {
                write!(formatter, "empty worktree initialization failed: {error}")
            }
            Self::ReconciliationConfirmation(error) => {
                write!(
                    formatter,
                    "project reconciliation confirmation failed: {error}"
                )
            }
            Self::Storage(error) => write!(formatter, "project storage failed: {error}"),
        }
    }
}

impl Error for CreateProjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DirectorySelection(error) => Some(error),
            Self::Inspection(error) => Some(error),
            Self::Initialization(error) => Some(error),
            Self::ReconciliationConfirmation(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::AlreadyGitRepository => None,
        }
    }
}

impl From<OpenProjectError> for CreateProjectError {
    fn from(error: OpenProjectError) -> Self {
        match error {
            OpenProjectError::DirectorySelection(error) => Self::DirectorySelection(error),
            OpenProjectError::Inspection(error) => Self::Inspection(error),
            OpenProjectError::ReconciliationConfirmation(error) => {
                Self::ReconciliationConfirmation(error)
            }
            OpenProjectError::Storage(error) => Self::Storage(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CreateProject, CreateProjectError, EmptyWorktreeInitializationFailure,
        EmptyWorktreeInitializer,
    };
    use crate::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, OpenProjectOutcome,
        ProjectDirectoryPicker, ProjectDirectorySelectionError, ProjectInspectionFailure,
        ProjectInspector, ProjectOpenPreparation, ProjectPathDisplay, ProjectReconciliationChoice,
        ProjectReconciliationConfirmationError, ProjectReconciliationConfirmer,
        ProjectReconciliationProposal, RecentProject, RecentProjectLimit,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId,
        RepositoryIdentity, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
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
    struct SequenceInspector {
        calls: Arc<AtomicUsize>,
        first: Result<ProjectIdentity, ProjectInspectionFailure>,
        rest: Result<ProjectIdentity, ProjectInspectionFailure>,
    }

    impl ProjectInspector for SequenceInspector {
        fn inspect_project(
            &self,
            _selected_root: &Path,
        ) -> Result<ProjectIdentity, ProjectInspectionFailure> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                self.first.clone()
            } else {
                self.rest.clone()
            }
        }
    }

    #[derive(Debug)]
    struct RecordingInitializer {
        calls: Arc<AtomicUsize>,
        result: Result<(), EmptyWorktreeInitializationFailure>,
    }

    impl EmptyWorktreeInitializer for RecordingInitializer {
        fn initialize_empty_worktree(
            &self,
            _selected_root: &Path,
        ) -> Result<(), EmptyWorktreeInitializationFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result
        }
    }

    #[derive(Debug)]
    struct RecordingStore {
        record_calls: Arc<AtomicUsize>,
        result: Result<ProjectId, KnowledgeStoreFailure>,
    }

    impl KnowledgeStore for RecordingStore {
        fn prepare_project_open<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectOpenPreparation> {
            Box::pin(async { Ok(ProjectOpenPreparation::Ready) })
        }

        fn record_opened_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            self.record_calls.fetch_add(1, Ordering::SeqCst);
            let result = self.result;
            Box::pin(async move { result })
        }

        fn reconcile_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _proposal: &'a ProjectReconciliationProposal,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            Box::pin(async { Err(KnowledgeStoreFailure::Unavailable) })
        }

        fn list_recent_projects(
            &self,
            _limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[derive(Debug)]
    struct CancelledConfirmer;

    impl ProjectReconciliationConfirmer for CancelledConfirmer {
        fn choose_reconciliation(
            &self,
            _proposal: &ProjectReconciliationProposal,
            _new_root_display: &ProjectPathDisplay,
        ) -> Result<ProjectReconciliationChoice, ProjectReconciliationConfirmationError> {
            Ok(ProjectReconciliationChoice::Cancel)
        }
    }

    fn cancelled_confirmer() -> Arc<dyn ProjectReconciliationConfirmer> {
        Arc::new(CancelledConfirmer)
    }

    #[test]
    fn cancellation_never_inspects_or_initializes() -> Result<(), Box<dyn std::error::Error>> {
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let init_calls = Arc::new(AtomicUsize::new(0));
        let storage_calls = Arc::new(AtomicUsize::new(0));
        let use_case = CreateProject::new(
            Arc::new(FixedPicker(None)),
            Arc::new(SequenceInspector {
                calls: Arc::clone(&inspection_calls),
                first: Ok(fixture_project()?),
                rest: Ok(fixture_project()?),
            }),
            Arc::new(RecordingInitializer {
                calls: Arc::clone(&init_calls),
                result: Ok(()),
            }),
            cancelled_confirmer(),
            Arc::new(RecordingStore {
                record_calls: Arc::clone(&storage_calls),
                result: Ok(ProjectId::from_bytes([3; 32])),
            }),
        );

        assert_eq!(block_on(use_case.execute())?, OpenProjectOutcome::Cancelled);
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 0);
        assert_eq!(init_calls.load(Ordering::SeqCst), 0);
        assert_eq!(storage_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn already_git_repository_never_initializes() -> Result<(), Box<dyn std::error::Error>> {
        let init_calls = Arc::new(AtomicUsize::new(0));
        let use_case = CreateProject::new(
            Arc::new(FixedPicker(Some(std::env::current_dir()?))),
            Arc::new(SequenceInspector {
                calls: Arc::new(AtomicUsize::new(0)),
                first: Ok(fixture_project()?),
                rest: Ok(fixture_project()?),
            }),
            Arc::new(RecordingInitializer {
                calls: Arc::clone(&init_calls),
                result: Ok(()),
            }),
            cancelled_confirmer(),
            Arc::new(RecordingStore {
                record_calls: Arc::new(AtomicUsize::new(0)),
                result: Ok(ProjectId::from_bytes([3; 32])),
            }),
        );

        assert_eq!(
            block_on(use_case.execute()),
            Err(CreateProjectError::AlreadyGitRepository)
        );
        assert_eq!(init_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn nested_worktree_never_initializes() -> Result<(), Box<dyn std::error::Error>> {
        let init_calls = Arc::new(AtomicUsize::new(0));
        let use_case = CreateProject::new(
            Arc::new(FixedPicker(Some(std::env::current_dir()?))),
            Arc::new(SequenceInspector {
                calls: Arc::new(AtomicUsize::new(0)),
                first: Err(ProjectInspectionFailure::NotWorktreeRoot),
                rest: Ok(fixture_project()?),
            }),
            Arc::new(RecordingInitializer {
                calls: Arc::clone(&init_calls),
                result: Ok(()),
            }),
            cancelled_confirmer(),
            Arc::new(RecordingStore {
                record_calls: Arc::new(AtomicUsize::new(0)),
                result: Ok(ProjectId::from_bytes([3; 32])),
            }),
        );

        assert_eq!(
            block_on(use_case.execute()),
            Err(CreateProjectError::Inspection(
                ProjectInspectionFailure::NotWorktreeRoot
            ))
        );
        assert_eq!(init_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn directory_not_empty_never_catalogs() -> Result<(), Box<dyn std::error::Error>> {
        let storage_calls = Arc::new(AtomicUsize::new(0));
        let use_case = CreateProject::new(
            Arc::new(FixedPicker(Some(std::env::current_dir()?))),
            Arc::new(SequenceInspector {
                calls: Arc::new(AtomicUsize::new(0)),
                first: Err(ProjectInspectionFailure::NotRepository),
                rest: Ok(fixture_project()?),
            }),
            Arc::new(RecordingInitializer {
                calls: Arc::new(AtomicUsize::new(0)),
                result: Err(EmptyWorktreeInitializationFailure::DirectoryNotEmpty),
            }),
            cancelled_confirmer(),
            Arc::new(RecordingStore {
                record_calls: Arc::clone(&storage_calls),
                result: Ok(ProjectId::from_bytes([3; 32])),
            }),
        );

        assert_eq!(
            block_on(use_case.execute()),
            Err(CreateProjectError::Initialization(
                EmptyWorktreeInitializationFailure::DirectoryNotEmpty
            ))
        );
        assert_eq!(storage_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn empty_directory_initializes_once_and_catalogs_unborn_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let init_calls = Arc::new(AtomicUsize::new(0));
        let storage_calls = Arc::new(AtomicUsize::new(0));
        let project = fixture_project()?;
        let project_id = ProjectId::from_bytes([3; 32]);
        let use_case = CreateProject::new(
            Arc::new(FixedPicker(Some(std::env::current_dir()?))),
            Arc::new(SequenceInspector {
                calls: Arc::clone(&inspection_calls),
                first: Err(ProjectInspectionFailure::NotRepository),
                rest: Ok(project.clone()),
            }),
            Arc::new(RecordingInitializer {
                calls: Arc::clone(&init_calls),
                result: Ok(()),
            }),
            cancelled_confirmer(),
            Arc::new(RecordingStore {
                record_calls: Arc::clone(&storage_calls),
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
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 2);
        assert_eq!(init_calls.load(Ordering::SeqCst), 1);
        assert_eq!(storage_calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    fn fixture_project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = CanonicalDirectory::from_canonicalized(std::env::current_dir()?)?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([4; 32]),
                repository_id,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }
}
