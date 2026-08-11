//! Desktop composition root and explicit boundary mappings for A^3.

mod clock;
/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod platform;
mod project_picker;
mod project_reconciliation_dialog;
mod repository_index_manager;

use a3_application::{
    GetHealth, GetProjectIndexStatus, GetProjectIndexStatusError, GetProjectStorageUsage,
    GetProjectStorageUsageError, HealthQuery, JobEventStream, JobScheduler, JobSchedulerConfig,
    JobSchedulerConfigError, JobSchedulerCreateError, KnowledgeIndexFailure, KnowledgeIndexStore,
    KnowledgeStore, KnowledgeStoreFailure, ListRecentProjects, ListRecentProjectsError,
    OpenProject, OpenProjectError, OpenProjectOutcome, ProjectDirectoryPicker, ProjectIndexStatus,
    ProjectInspectionFailure, ProjectReconciliationConfirmer, ProjectStorageControl,
    ProjectStorageControlError, ProjectStorageFailure, ProjectStorageStore, RecentProject,
};
use a3_domain::{
    ApplicationVersion, ApplicationVersionError, GitHead, Health, IndexRunStatus, Platform,
    ProjectId, ProjectIdentity,
};
use a3_protocol::{
    CommandErrorV1, ErrorCodeV1, GitHeadV1, HealthResponseV1, IndexStateV1, OpenProjectResponseV1,
    PlatformV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1, ProjectSummaryV1,
    RebuildProjectIndexResponseV1, RebuildStateV1, RecentProjectSummaryV1,
    RecentProjectsResponseV1,
};
use a3_storage_libsql::{
    CatalogOpenError, LibsqlKnowledgeStore, StorageLayout, StorageLayoutError,
};
use a3_workspace::RepositoryInspector;
use clock::SystemJobClock;
use platform::SystemPlatform;
use project_picker::NativeProjectDirectoryPicker;
use project_reconciliation_dialog::NativeProjectReconciliationConfirmer;
use repository_index_manager::{
    RepositoryIndexManager, RepositoryIndexRebuildRequestError, RepositoryIndexRebuildState,
};
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::Manager;

const MAX_PROJECT_PATH_DISPLAY_CHARS: usize = 32_768;

/// Owns the concrete application use cases used by the desktop process.
#[derive(Debug)]
pub struct CompositionRoot {
    health_query: GetHealth,
    open_project: OpenProject,
    recent_projects: ListRecentProjects,
    project_status: Option<GetProjectIndexStatus>,
    project_storage: Option<GetProjectStorageUsage>,
    active_project: Mutex<Option<ActiveProject>>,
    index_manager: Option<RepositoryIndexManager>,
    _job_scheduler: JobScheduler,
    _job_events: JobEventStream,
}

impl CompositionRoot {
    /// Wires process metadata, project selection, and a bounded owned background-job runtime.
    pub fn new(
        application_version: ApplicationVersion,
        platform: Platform,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
    ) -> Result<Self, CompositionRootError> {
        CompositionBase::new(application_version, platform).and_then(|base| {
            base.finish(
                project_directory_picker,
                project_reconciliation_confirmer,
                store,
            )
        })
    }

    /// Wires the desktop application using package, platform, and native-picker adapters.
    pub fn from_environment(
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
    ) -> Result<Self, CompositionRootError> {
        CompositionBase::from_environment().and_then(|base| {
            base.finish(
                project_directory_picker,
                project_reconciliation_confirmer,
                store,
            )
        })
    }

    /// Executes the health use case and maps its domain result to IPC V1.
    #[must_use]
    pub fn query_health(&self) -> HealthResponseV1 {
        map_health_to_v1(self.health_query.execute())
    }

    /// Executes one user-controlled native project selection and maps it to IPC V1.
    pub async fn open_project(&self) -> Result<OpenProjectResponseV1, CommandErrorV1> {
        let outcome = self
            .open_project
            .execute()
            .await
            .map_err(map_open_project_error_to_v1)?;
        if let OpenProjectOutcome::Opened { project, .. } = &outcome
            && let Some(manager) = &self.index_manager
        {
            manager
                .activate_project(project.as_ref().clone())
                .map_err(|_| CommandErrorV1::project_open(ErrorCodeV1::LocalStorageUnavailable))?;
        }
        if let OpenProjectOutcome::Opened {
            project,
            project_id,
        } = &outcome
        {
            *lock_recovering_poison(&self.active_project) = Some(ActiveProject {
                project_id: *project_id,
                project: project.as_ref().clone(),
            });
        }
        Ok(map_open_project_to_v1(outcome))
    }

    /// Queries the bounded recent-project list and maps it to IPC V1.
    pub async fn list_recent_projects(&self) -> Result<RecentProjectsResponseV1, CommandErrorV1> {
        self.recent_projects
            .execute()
            .await
            .map(map_recent_projects_to_v1)
            .map_err(map_recent_projects_error_to_v1)
    }

    /// Returns bounded metadata for the active Core-owned project identity.
    pub async fn query_project_status(&self) -> Result<ProjectStatusResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectStatusResponseV1::no_project());
        };
        let index = match &self.project_status {
            Some(query) => query
                .execute(&active.project)
                .await
                .map_err(map_project_status_error_to_v1)?,
            None => ProjectIndexStatus::new(None, None, None),
        };
        let storage_bytes = match &self.project_storage {
            Some(query) => Some(
                query
                    .execute(&active.project, &DesktopProjectStorageControl::new())
                    .await
                    .map_err(map_project_storage_error_to_v1)?
                    .bytes()
                    .to_string(),
            ),
            None => None,
        };
        Ok(ProjectStatusResponseV1::active(
            active.project_id.to_string(),
            map_project_summary_to_v1(&active.project),
            map_project_index_status_to_v1(index),
            storage_bytes,
            self.index_manager
                .as_ref()
                .map_or(RebuildStateV1::Idle, |manager| {
                    map_rebuild_state_to_v1(manager.rebuild_state())
                }),
        ))
    }

    /// Queues a bounded rebuild for the Core-owned active project.
    pub fn rebuild_project_index(&self) -> Result<RebuildProjectIndexResponseV1, CommandErrorV1> {
        let manager = self
            .index_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_rebuild(ErrorCodeV1::IndexRebuildUnavailable))?;
        manager
            .request_rebuild()
            .map_err(map_rebuild_request_error_to_v1)?;
        Ok(RebuildProjectIndexResponseV1::queued())
    }
}

#[derive(Debug, Clone)]
struct ActiveProject {
    project_id: ProjectId,
    project: ProjectIdentity,
}

#[derive(Debug)]
struct DesktopProjectStorageControl {
    entries: AtomicU32,
}

impl DesktopProjectStorageControl {
    fn new() -> Self {
        Self {
            entries: AtomicU32::new(0),
        }
    }
}

impl ProjectStorageControl for DesktopProjectStorageControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_entries(&self, entries: u32) -> Result<(), ProjectStorageControlError> {
        let previous = self.entries.load(Ordering::Acquire);
        if entries < previous {
            return Err(ProjectStorageControlError);
        }
        self.entries.store(entries, Ordering::Release);
        Ok(())
    }
}

#[derive(Debug)]
struct CompositionBase {
    health_query: GetHealth,
    job_scheduler: JobScheduler,
    job_events: JobEventStream,
}

impl CompositionBase {
    fn new(
        application_version: ApplicationVersion,
        platform: Platform,
    ) -> Result<Self, CompositionRootError> {
        let config = JobSchedulerConfig::new(2, 32, 256)
            .map_err(CompositionRootError::InvalidJobSchedulerConfig)?;
        let (job_scheduler, job_events) =
            JobScheduler::new(config, Arc::new(SystemJobClock::new()))
                .map_err(CompositionRootError::JobScheduler)?;

        Ok(Self {
            health_query: GetHealth::new(application_version, platform),
            job_scheduler,
            job_events,
        })
    }

    fn from_environment() -> Result<Self, CompositionRootError> {
        let version = ApplicationVersion::try_from(env!("CARGO_PKG_VERSION"))
            .map_err(CompositionRootError::InvalidVersion)?;
        Self::new(version, SystemPlatform::current())
    }

    fn finish(
        self,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
    ) -> Result<CompositionRoot, CompositionRootError> {
        self.finish_internal(
            project_directory_picker,
            project_reconciliation_confirmer,
            store,
            None,
            None,
        )
    }

    fn finish_with_indexing(
        self,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
        index_store: Arc<dyn KnowledgeIndexStore>,
        project_storage: Arc<dyn ProjectStorageStore>,
    ) -> Result<CompositionRoot, CompositionRootError> {
        self.finish_internal(
            project_directory_picker,
            project_reconciliation_confirmer,
            store,
            Some(index_store),
            Some(project_storage),
        )
    }

    fn finish_internal(
        self,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
        index_store: Option<Arc<dyn KnowledgeIndexStore>>,
        project_storage: Option<Arc<dyn ProjectStorageStore>>,
    ) -> Result<CompositionRoot, CompositionRootError> {
        let project_status = index_store.clone().map(GetProjectIndexStatus::new);
        let index_manager = index_store
            .map(|store| {
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::IndexManagerUnavailable)?;
                RepositoryIndexManager::start(submitter, self.job_events.clone(), store)
                    .map_err(|_| CompositionRootError::IndexManager)
            })
            .transpose()?;
        Ok(CompositionRoot {
            health_query: self.health_query,
            open_project: OpenProject::new(
                project_directory_picker,
                Arc::new(RepositoryInspector::new()),
                project_reconciliation_confirmer,
                Arc::clone(&store),
            ),
            recent_projects: ListRecentProjects::new(store),
            project_status,
            project_storage: project_storage.map(GetProjectStorageUsage::new),
            active_project: Mutex::new(None),
            index_manager,
            _job_scheduler: self.job_scheduler,
            _job_events: self.job_events,
        })
    }
}

/// Starts the Tauri desktop process with its narrow command surface.
pub fn run() -> Result<(), DesktopRunError> {
    let base = CompositionBase::from_environment().map_err(DesktopRunError::Composition)?;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_data_root = app
                .path()
                .app_data_dir()
                .map_err(CompositionRootError::AppDataPath)?;
            let layout = StorageLayout::prepare(app_data_root)
                .map_err(CompositionRootError::StorageLayout)?;
            let store = Arc::new(
                tauri::async_runtime::block_on(LibsqlKnowledgeStore::open(&layout))
                    .map_err(CompositionRootError::Catalog)?,
            );
            let project_storage: Arc<dyn ProjectStorageStore> = store.clone();
            let catalog_store: Arc<dyn KnowledgeStore> = store.clone();
            let index_store: Arc<dyn KnowledgeIndexStore> = store;
            app.manage(base.finish_with_indexing(
                Arc::new(NativeProjectDirectoryPicker::new(app.handle().clone())),
                Arc::new(NativeProjectReconciliationConfirmer::new(
                    app.handle().clone(),
                )),
                catalog_store,
                index_store,
                project_storage,
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_recent_projects,
            commands::open_project,
            commands::query_project_status,
            commands::query_health,
            commands::rebuild_project_index
        ])
        .run(tauri::generate_context!())
        .map_err(DesktopRunError::Tauri)
}

fn map_health_to_v1(health: Health) -> HealthResponseV1 {
    HealthResponseV1::ready(
        health.application_version().as_str().to_owned(),
        map_platform_to_v1(health.platform()),
    )
}

const fn map_platform_to_v1(platform: Platform) -> PlatformV1 {
    match platform {
        Platform::Windows => PlatformV1::Windows,
        Platform::Linux => PlatformV1::Linux,
        Platform::MacOs => PlatformV1::MacOs,
        Platform::Unsupported => PlatformV1::Unsupported,
    }
}

fn map_open_project_to_v1(outcome: OpenProjectOutcome) -> OpenProjectResponseV1 {
    match outcome {
        OpenProjectOutcome::Cancelled => OpenProjectResponseV1::cancelled(),
        OpenProjectOutcome::Opened { project, .. } => {
            OpenProjectResponseV1::opened(map_project_summary_to_v1(&project))
        }
    }
}

fn map_recent_projects_to_v1(projects: Vec<RecentProject>) -> RecentProjectsResponseV1 {
    RecentProjectsResponseV1::new(
        projects
            .into_iter()
            .map(|recent| {
                RecentProjectSummaryV1::new(
                    recent.project_id().to_string(),
                    ProjectSummaryV1::new(
                        recent.repository_id().to_string(),
                        recent.worktree_id().to_string(),
                        recent.worktree_root_display().as_str().to_owned(),
                        map_git_head_to_v1(recent.head()),
                    ),
                )
            })
            .collect(),
    )
}

fn map_project_summary_to_v1(project: &ProjectIdentity) -> ProjectSummaryV1 {
    ProjectSummaryV1::new(
        project.repository().id().to_string(),
        project.worktree().id().to_string(),
        project_path_display(project.worktree().root().as_path()),
        map_git_head_to_v1(project.head()),
    )
}

fn map_project_index_status_to_v1(status: ProjectIndexStatus) -> ProjectIndexStatusV1 {
    let latest_attempt = status.latest_attempt();
    let state = latest_attempt.map_or(IndexStateV1::NotStarted, |attempt| match attempt.status() {
        IndexRunStatus::Building => IndexStateV1::Building,
        IndexRunStatus::Published => IndexStateV1::Published,
        IndexRunStatus::Failed => IndexStateV1::Failed,
        IndexRunStatus::Cancelled => IndexStateV1::Cancelled,
    });
    ProjectIndexStatusV1::new(
        state,
        status.latest_snapshot().map(|snapshot| {
            ProjectSnapshotV1::new(
                snapshot.id().to_string(),
                snapshot.generation().get().to_string(),
            )
        }),
        latest_attempt.map(|attempt| attempt.snapshot_id().to_string()),
        status
            .published_snapshot_id()
            .map(|snapshot_id| snapshot_id.to_string()),
    )
}

const fn map_rebuild_state_to_v1(state: RepositoryIndexRebuildState) -> RebuildStateV1 {
    match state {
        RepositoryIndexRebuildState::Idle => RebuildStateV1::Idle,
        RepositoryIndexRebuildState::Queued => RebuildStateV1::Queued,
        RepositoryIndexRebuildState::Running => RebuildStateV1::Running,
        RepositoryIndexRebuildState::Succeeded => RebuildStateV1::Succeeded,
        RepositoryIndexRebuildState::Failed => RebuildStateV1::Failed,
        RepositoryIndexRebuildState::Cancelled => RebuildStateV1::Cancelled,
    }
}

fn map_git_head_to_v1(head: &GitHead) -> GitHeadV1 {
    match head {
        GitHead::Born {
            object_id,
            reference,
        } => GitHeadV1::Born {
            object_id: object_id.as_str().to_owned(),
            reference: reference
                .as_ref()
                .map(|reference| reference.as_str().to_owned()),
        },
        GitHead::Unborn { reference } => GitHeadV1::Unborn {
            reference: reference.as_str().to_owned(),
        },
    }
}

fn project_path_display(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .take(MAX_PROJECT_PATH_DISPLAY_CHARS)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn map_open_project_error_to_v1(error: OpenProjectError) -> CommandErrorV1 {
    let code = match error {
        OpenProjectError::DirectorySelection(_) => ErrorCodeV1::ProjectSelectionFailed,
        OpenProjectError::Inspection(ProjectInspectionFailure::SelectionUnavailable) => {
            ErrorCodeV1::ProjectSelectionUnavailable
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::NotRepository) => {
            ErrorCodeV1::NotGitRepository
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::NotWorktreeRoot) => {
            ErrorCodeV1::ProjectRootRequired
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::UnsupportedRepository) => {
            ErrorCodeV1::UnsupportedRepository
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::InvalidRepositoryMetadata) => {
            ErrorCodeV1::InvalidRepositoryMetadata
        }
        OpenProjectError::ReconciliationConfirmation(_) => ErrorCodeV1::ProjectSelectionFailed,
        OpenProjectError::Storage(error) => map_storage_error_to_v1(error),
    };
    CommandErrorV1::project_open(code)
}

fn map_recent_projects_error_to_v1(error: ListRecentProjectsError) -> CommandErrorV1 {
    match error {
        ListRecentProjectsError::Storage(error) => {
            CommandErrorV1::project_open(map_storage_error_to_v1(error))
        }
    }
}

fn map_project_status_error_to_v1(error: GetProjectIndexStatusError) -> CommandErrorV1 {
    match error {
        GetProjectIndexStatusError::Storage(KnowledgeIndexFailure::Storage(error)) => {
            CommandErrorV1::project_open(map_storage_error_to_v1(error))
        }
        GetProjectIndexStatusError::Storage(_) => {
            CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)
        }
    }
}

fn map_project_storage_error_to_v1(error: GetProjectStorageUsageError) -> CommandErrorV1 {
    let GetProjectStorageUsageError::Storage(error) = error;
    let code = match error {
        ProjectStorageFailure::InvalidLayout
        | ProjectStorageFailure::TooManyEntries
        | ProjectStorageFailure::SizeOverflow => ErrorCodeV1::LocalStorageInvalidData,
        ProjectStorageFailure::Unavailable
        | ProjectStorageFailure::Cancelled
        | ProjectStorageFailure::TimedOut
        | ProjectStorageFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_rebuild_request_error_to_v1(error: RepositoryIndexRebuildRequestError) -> CommandErrorV1 {
    let code = match error {
        RepositoryIndexRebuildRequestError::NoActiveProject => ErrorCodeV1::NoActiveProject,
        RepositoryIndexRebuildRequestError::AlreadyPending => {
            ErrorCodeV1::IndexRebuildAlreadyPending
        }
        RepositoryIndexRebuildRequestError::QueueFull
        | RepositoryIndexRebuildRequestError::CoordinatorStopped => {
            ErrorCodeV1::IndexRebuildUnavailable
        }
    };
    CommandErrorV1::project_rebuild(code)
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

const fn map_storage_error_to_v1(error: KnowledgeStoreFailure) -> ErrorCodeV1 {
    match error {
        KnowledgeStoreFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
        KnowledgeStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        KnowledgeStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        KnowledgeStoreFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
        KnowledgeStoreFailure::IdentityConflict => ErrorCodeV1::ProjectIdentityConflict,
    }
}

/// Failure while constructing the desktop composition root.
#[derive(Debug)]
pub enum CompositionRootError {
    /// Build metadata contained an invalid application version.
    InvalidVersion(ApplicationVersionError),
    /// The compile-time desktop scheduler limits were invalid.
    InvalidJobSchedulerConfig(JobSchedulerConfigError),
    /// The operating system rejected an owned scheduler worker.
    JobScheduler(JobSchedulerCreateError),
    /// The scheduler stopped before the index coordinator could acquire a submit capability.
    IndexManagerUnavailable,
    /// The owned repository-index coordinator could not be started.
    IndexManager,
    /// Tauri could not resolve the private application-data directory.
    AppDataPath(tauri::Error),
    /// The private application-data storage boundary could not be established.
    StorageLayout(StorageLayoutError),
    /// The global project catalog could not be opened safely.
    Catalog(CatalogOpenError),
}

impl fmt::Display for CompositionRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersion(error) => {
                write!(formatter, "invalid application version: {error}")
            }
            Self::InvalidJobSchedulerConfig(error) => {
                write!(formatter, "invalid job scheduler configuration: {error}")
            }
            Self::JobScheduler(error) => write!(formatter, "job scheduler failed: {error}"),
            Self::IndexManagerUnavailable => {
                formatter.write_str("repository index manager is unavailable")
            }
            Self::IndexManager => formatter.write_str("repository index manager failed"),
            Self::AppDataPath(_) => formatter.write_str("application data path is unavailable"),
            Self::StorageLayout(error) => write!(formatter, "storage layout failed: {error}"),
            Self::Catalog(error) => write!(formatter, "catalog open failed: {error}"),
        }
    }
}

impl Error for CompositionRootError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidVersion(error) => Some(error),
            Self::InvalidJobSchedulerConfig(error) => Some(error),
            Self::JobScheduler(error) => Some(error),
            Self::IndexManagerUnavailable => None,
            Self::IndexManager => None,
            Self::AppDataPath(error) => Some(error),
            Self::StorageLayout(error) => Some(error),
            Self::Catalog(error) => Some(error),
        }
    }
}

/// Failure while constructing or running the desktop process.
#[derive(Debug)]
pub enum DesktopRunError {
    /// The process composition root could not be constructed.
    Composition(CompositionRootError),
    /// Tauri failed to construct or run the desktop application.
    Tauri(tauri::Error),
}

impl fmt::Display for DesktopRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(error) => write!(formatter, "composition failed: {error}"),
            Self::Tauri(error) => write!(formatter, "desktop runtime failed: {error}"),
        }
    }
}

impl Error for DesktopRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Composition(error) => Some(error),
            Self::Tauri(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_PROJECT_PATH_DISPLAY_CHARS, project_path_display};
    use std::path::Path;

    #[test]
    fn project_path_display_is_bounded_and_contains_no_control_characters() {
        let path = format!("C:\\\n{}", "a".repeat(MAX_PROJECT_PATH_DISPLAY_CHARS + 8));

        let display = project_path_display(Path::new(&path));

        assert_eq!(display.chars().count(), MAX_PROJECT_PATH_DISPLAY_CHARS);
        assert!(!display.chars().any(char::is_control));
        assert!(display.contains('\u{fffd}'));
    }
}
