//! Desktop composition root and explicit boundary mappings for A^3.

mod clock;
/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod deep_map_manager;
mod job_ids;
mod platform;
mod project_picker;
mod project_reconciliation_dialog;
mod repository_index_manager;

use a3_application::{
    DeepMapExecutor, GetHealth, GetProjectIndexStatus, GetProjectIndexStatusError,
    GetProjectStorageUsage, GetProjectStorageUsageError, GetPublishedIndexOverview,
    GetPublishedIndexOverviewError, HealthQuery, IndexPersistenceControl,
    IndexPersistenceControlError, JobEventStream, JobScheduler, JobSchedulerConfig,
    JobSchedulerConfigError, JobSchedulerCreateError, KnowledgeIndexFailure, KnowledgeIndexStore,
    KnowledgeStore, KnowledgeStoreFailure, ListRecentProjects, ListRecentProjectsError,
    OpenProject, OpenProjectError, OpenProjectOutcome, ProjectCatalogAdmin,
    ProjectCatalogAdminFailure, ProjectDirectoryPicker, ProjectIndexStatus,
    ProjectInspectionFailure, ProjectReconciliationConfirmer, ProjectStorageControl,
    ProjectStorageControlError, ProjectStorageFailure, ProjectStorageStore, PublishedIndexOverview,
    RecentProject, RemoveProjectFromList, RemoveProjectFromListError,
};
use a3_domain::{
    ApplicationVersion, ApplicationVersionError, ExploreBudget, GitHead, Health, IndexLanguage,
    IndexRunStatus, ParseDiagnosticCode, ParseDiagnosticSeverity, Platform, Progress, ProjectId,
    ProjectIdentity,
};
use a3_protocol::{
    CommandErrorV1, DeepMapActivityStateV1, DeepMapActivityV1, DeepMapBudgetV1,
    DeepMapConfigurationV1, DeepMapControlResponseV1, DeepMapModelV1, DeepMapProgressV1,
    DeepMapStatusResponseV1, ErrorCodeV1, GitHeadV1, HealthResponseV1, IndexActivityResponseV1,
    IndexActivityStateV1, IndexActivityV1, IndexDiagnosticCodeV1, IndexDiagnosticSeverityV1,
    IndexDiagnosticV1, IndexFileDiagnosticsV1, IndexLanguageV1, IndexOverviewCountsV1,
    IndexOverviewResponseV1, IndexOverviewV1, IndexPhaseV1, IndexStateV1, OpenProjectResponseV1,
    PlatformV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1, ProjectSummaryV1,
    RebuildProjectIndexResponseV1, RebuildStateV1, RecentProjectSummaryV1,
    RecentProjectsResponseV1, RemoveProjectResponseV1,
};
use a3_storage_libsql::{
    CatalogOpenError, LibsqlKnowledgeStore, StorageLayout, StorageLayoutError,
};
use a3_workspace::RepositoryInspector;
use clock::SystemJobClock;
use deep_map_manager::{
    DeepMapActivity, DeepMapActivityState, DeepMapManager, DeepMapManagerControlError,
};
use job_ids::DesktopJobIds;
use platform::SystemPlatform;
use project_picker::NativeProjectDirectoryPicker;
use project_reconciliation_dialog::NativeProjectReconciliationConfirmer;
use repository_index_manager::{
    RepositoryIndexActivity, RepositoryIndexActivityState, RepositoryIndexDeactivationError,
    RepositoryIndexManager, RepositoryIndexRebuildRequestError, RepositoryIndexRebuildState,
};
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
    index_overview: Option<GetPublishedIndexOverview>,
    project_storage: Option<GetProjectStorageUsage>,
    remove_project: Option<RemoveProjectFromList>,
    active_project: Mutex<Option<ActiveProject>>,
    project_operation_active: AtomicBool,
    index_manager: Option<RepositoryIndexManager>,
    deep_map_manager: Option<DeepMapManager>,
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
        let _operation = self.acquire_project_operation(CommandErrorV1::project_open)?;
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
        if let OpenProjectOutcome::Opened { project, .. } = &outcome
            && let Some(manager) = &self.deep_map_manager
            && manager.activate_project(project.as_ref().clone()).is_err()
        {
            if let Some(index_manager) = &self.index_manager {
                let _deactivated = index_manager.deactivate_project();
            }
            return Err(CommandErrorV1::project_open(
                ErrorCodeV1::DeepMapUnavailable,
            ));
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

    /// Returns a non-blocking in-memory Fast-Index activity snapshot for the active project.
    #[must_use]
    pub fn query_index_activity(&self) -> IndexActivityResponseV1 {
        if lock_recovering_poison(&self.active_project).is_none() {
            return IndexActivityResponseV1::no_project();
        }
        let activity = self.index_manager.as_ref().map_or_else(
            RepositoryIndexActivity::idle,
            RepositoryIndexManager::activity,
        );
        IndexActivityResponseV1::active(map_index_activity_to_v1(activity))
    }

    /// Returns a bounded read-only projection of the last atomically published index.
    pub async fn query_index_overview(&self) -> Result<IndexOverviewResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(IndexOverviewResponseV1::no_project());
        };
        let Some(query) = &self.index_overview else {
            return Ok(IndexOverviewResponseV1::no_published_index());
        };
        query
            .execute(&active.project, &DesktopIndexOverviewControl::new())
            .await
            .map_err(map_index_overview_error_to_v1)
            .map(|overview| match overview {
                Some(overview) => {
                    IndexOverviewResponseV1::published(map_index_overview_to_v1(&overview))
                }
                None => IndexOverviewResponseV1::no_published_index(),
            })
    }

    /// Returns only Core-owned Deep-Map configuration and in-memory lifecycle state.
    #[must_use]
    pub fn query_deep_map_status(&self) -> DeepMapStatusResponseV1 {
        if lock_recovering_poison(&self.active_project).is_none() {
            return DeepMapStatusResponseV1::no_project();
        }
        let Some(manager) = &self.deep_map_manager else {
            return DeepMapStatusResponseV1::unavailable();
        };
        let model = manager.model();
        DeepMapStatusResponseV1::available(
            DeepMapConfigurationV1::new(
                DeepMapModelV1::new(
                    model.profile().id().to_string(),
                    model.profile().version().get(),
                    model.provider_id().to_owned(),
                    model.model_id().to_owned(),
                    model.context_tokens(),
                    model.output_tokens(),
                ),
                map_deep_map_budget_to_v1(ExploreBudget::MINIMUM),
                map_deep_map_budget_to_v1(ExploreBudget::DEFAULT),
                map_deep_map_budget_to_v1(ExploreBudget::MAXIMUM),
            ),
            map_deep_map_activity_to_v1(manager.activity()),
        )
    }

    /// Starts model work only after the explicit bounded WebView request was validated.
    pub fn start_deep_map(
        &self,
        budget: DeepMapBudgetV1,
    ) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
        if lock_recovering_poison(&self.active_project).is_none() {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        }
        let budget = ExploreBudget::new(
            budget.token_limit(),
            budget.time_limit_millis(),
            budget.tool_call_limit(),
        )
        .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::InvalidDeepMapBudget))?;
        self.deep_map_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?
            .start_mapping(budget)
            .map_err(map_deep_map_control_error)?;
        Ok(DeepMapControlResponseV1::accepted())
    }

    /// Requests checkpoint-producing cooperative cancellation of the active mapping attempt.
    pub fn pause_deep_map(&self) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
        self.control_deep_map(DeepMapManager::pause)
    }

    /// Starts a new owned scheduler attempt from the exact paused plan prefix.
    pub fn resume_deep_map(&self) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
        self.control_deep_map(DeepMapManager::resume)
    }

    /// Cancels and discards either the active attempt or retained paused checkpoint.
    pub fn cancel_deep_map(&self) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
        self.control_deep_map(DeepMapManager::cancel)
    }

    fn control_deep_map(
        &self,
        operation: fn(&DeepMapManager) -> Result<(), DeepMapManagerControlError>,
    ) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
        if lock_recovering_poison(&self.active_project).is_none() {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        }
        let manager = self
            .deep_map_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        operation(manager).map_err(map_deep_map_control_error)?;
        Ok(DeepMapControlResponseV1::accepted())
    }

    /// Queues a bounded rebuild for the Core-owned active project.
    pub fn rebuild_project_index(&self) -> Result<RebuildProjectIndexResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_rebuild)?;
        if lock_recovering_poison(&self.active_project).is_none() {
            return Err(CommandErrorV1::project_rebuild(
                ErrorCodeV1::NoActiveProject,
            ));
        }
        let manager = self
            .index_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_rebuild(ErrorCodeV1::IndexRebuildUnavailable))?;
        manager
            .request_rebuild()
            .map_err(map_rebuild_request_error_to_v1)?;
        Ok(RebuildProjectIndexResponseV1::queued())
    }

    /// Removes the Core-owned active worktree from A^3 while retaining source and private data.
    pub async fn remove_project(&self) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_removal)?;
        let active = lock_recovering_poison(&self.active_project)
            .clone()
            .ok_or_else(|| CommandErrorV1::project_removal(ErrorCodeV1::NoActiveProject))?;
        let manager = self.index_manager.as_ref().ok_or_else(|| {
            CommandErrorV1::project_removal(ErrorCodeV1::ProjectRemovalUnavailable)
        })?;
        let remove = self.remove_project.as_ref().ok_or_else(|| {
            CommandErrorV1::project_removal(ErrorCodeV1::ProjectRemovalUnavailable)
        })?;

        if let Err(error) = manager.deactivate_project() {
            let _restored = manager.activate_project(active.project.clone());
            return Err(map_deactivation_error_to_v1(error));
        }
        if let Some(deep_map_manager) = &self.deep_map_manager
            && deep_map_manager.deactivate_project().is_err()
        {
            let _restored = manager.activate_project(active.project.clone());
            return Err(CommandErrorV1::project_removal(
                ErrorCodeV1::ProjectRemovalUnavailable,
            ));
        }
        let outcome = remove.execute(&active.project, active.project_id).await;
        if let Err(error) = outcome {
            if manager.activate_project(active.project.clone()).is_err() {
                return Err(CommandErrorV1::project_removal(
                    ErrorCodeV1::ProjectRemovalUnavailable,
                ));
            }
            if let Some(deep_map_manager) = &self.deep_map_manager
                && deep_map_manager
                    .activate_project(active.project.clone())
                    .is_err()
            {
                return Err(CommandErrorV1::project_removal(
                    ErrorCodeV1::ProjectRemovalUnavailable,
                ));
            }
            return Err(map_project_removal_error_to_v1(error));
        }

        *lock_recovering_poison(&self.active_project) = None;
        Ok(RemoveProjectResponseV1::removed())
    }

    fn acquire_project_operation(
        &self,
        error: fn(ErrorCodeV1) -> CommandErrorV1,
    ) -> Result<ProjectOperationPermit<'_>, CommandErrorV1> {
        self.project_operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ProjectOperationPermit {
                active: &self.project_operation_active,
            })
            .map_err(|_| error(ErrorCodeV1::ProjectOperationBusy))
    }
}

struct ProjectOperationPermit<'a> {
    active: &'a AtomicBool,
}

impl Drop for ProjectOperationPermit<'_> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
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

#[derive(Debug)]
struct DesktopIndexOverviewControl {
    completed: AtomicU64,
    total: AtomicU64,
}

impl DesktopIndexOverviewControl {
    fn new() -> Self {
        Self {
            completed: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }
}

impl IndexPersistenceControl for DesktopIndexOverviewControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), IndexPersistenceControlError> {
        let completed = progress
            .completed()
            .ok_or(IndexPersistenceControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(IndexPersistenceControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(IndexPersistenceControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
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

#[derive(Default)]
struct OptionalCompositionPorts {
    index_store: Option<Arc<dyn KnowledgeIndexStore>>,
    project_storage: Option<Arc<dyn ProjectStorageStore>>,
    project_catalog_admin: Option<Arc<dyn ProjectCatalogAdmin>>,
    deep_map_executor: Option<Arc<dyn DeepMapExecutor>>,
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
            OptionalCompositionPorts::default(),
        )
    }

    fn finish_with_indexing(
        self,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
        index_store: Arc<dyn KnowledgeIndexStore>,
        project_storage: Arc<dyn ProjectStorageStore>,
        project_catalog_admin: Arc<dyn ProjectCatalogAdmin>,
    ) -> Result<CompositionRoot, CompositionRootError> {
        self.finish_internal(
            project_directory_picker,
            project_reconciliation_confirmer,
            store,
            OptionalCompositionPorts {
                index_store: Some(index_store),
                project_storage: Some(project_storage),
                project_catalog_admin: Some(project_catalog_admin),
                deep_map_executor: None,
            },
        )
    }

    fn finish_internal(
        self,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
        project_reconciliation_confirmer: Arc<dyn ProjectReconciliationConfirmer>,
        store: Arc<dyn KnowledgeStore>,
        ports: OptionalCompositionPorts,
    ) -> Result<CompositionRoot, CompositionRootError> {
        let job_ids = Arc::new(DesktopJobIds::new());
        let project_status = ports.index_store.clone().map(GetProjectIndexStatus::new);
        let index_overview = ports
            .index_store
            .clone()
            .map(GetPublishedIndexOverview::new);
        let index_manager = ports
            .index_store
            .map(|store| {
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::IndexManagerUnavailable)?;
                RepositoryIndexManager::start(
                    submitter,
                    self.job_events.clone(),
                    store,
                    Arc::clone(&job_ids),
                )
                .map_err(|_| CompositionRootError::IndexManager)
            })
            .transpose()?;
        let deep_map_manager = ports
            .deep_map_executor
            .map(|executor| {
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::DeepMapManagerUnavailable)?;
                DeepMapManager::start(
                    submitter,
                    self.job_events.clone(),
                    executor,
                    Arc::clone(&job_ids),
                )
                .map_err(|_| CompositionRootError::DeepMapManager)
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
            index_overview,
            project_storage: ports.project_storage.map(GetProjectStorageUsage::new),
            remove_project: ports.project_catalog_admin.map(RemoveProjectFromList::new),
            active_project: Mutex::new(None),
            project_operation_active: AtomicBool::new(false),
            index_manager,
            deep_map_manager,
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
            let project_catalog_admin: Arc<dyn ProjectCatalogAdmin> = store.clone();
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
                project_catalog_admin,
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cancel_deep_map,
            commands::list_recent_projects,
            commands::open_project,
            commands::pause_deep_map,
            commands::query_deep_map,
            commands::query_project_status,
            commands::query_index_activity,
            commands::query_index_overview,
            commands::query_health,
            commands::rebuild_project_index,
            commands::resume_deep_map,
            commands::remove_project,
            commands::start_deep_map
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

fn map_index_activity_to_v1(activity: RepositoryIndexActivity) -> IndexActivityV1 {
    IndexActivityV1::new(
        match activity.state() {
            RepositoryIndexActivityState::Idle => IndexActivityStateV1::Idle,
            RepositoryIndexActivityState::Queued => IndexActivityStateV1::Queued,
            RepositoryIndexActivityState::Running => IndexActivityStateV1::Running,
            RepositoryIndexActivityState::Cancelling => IndexActivityStateV1::Cancelling,
            RepositoryIndexActivityState::Succeeded => IndexActivityStateV1::Succeeded,
            RepositoryIndexActivityState::Failed => IndexActivityStateV1::Failed,
            RepositoryIndexActivityState::Cancelled => IndexActivityStateV1::Cancelled,
        },
        activity.phase().map(|phase| match phase {
            a3_application::RepositoryIndexPhase::Discover => IndexPhaseV1::Discover,
            a3_application::RepositoryIndexPhase::Hash => IndexPhaseV1::Hash,
            a3_application::RepositoryIndexPhase::Parse => IndexPhaseV1::Parse,
            a3_application::RepositoryIndexPhase::Link => IndexPhaseV1::Link,
            a3_application::RepositoryIndexPhase::Rank => IndexPhaseV1::Rank,
            a3_application::RepositoryIndexPhase::Publish => IndexPhaseV1::Publish,
        }),
        activity.completed(),
        RepositoryIndexActivity::TOTAL_PHASES,
    )
}

fn map_index_overview_to_v1(overview: &PublishedIndexOverview) -> IndexOverviewV1 {
    IndexOverviewV1::new(
        overview.snapshot_id().to_string(),
        IndexOverviewCountsV1::new(
            overview.file_count().to_string(),
            overview.symbol_count().to_string(),
            overview.diagnostic_count().to_string(),
            overview.parsed_file_count().to_string(),
            overview.diagnostic_file_count().to_string(),
        ),
        overview.coverage_basis_points(),
        overview
            .diagnostic_files()
            .iter()
            .map(|file| {
                IndexFileDiagnosticsV1::new(
                    file.path_display().as_str().to_owned(),
                    file.path_display().is_truncated(),
                    match file.language() {
                        IndexLanguage::Generic => IndexLanguageV1::Generic,
                        IndexLanguage::Rust => IndexLanguageV1::Rust,
                        IndexLanguage::TypeScriptJavaScript => {
                            IndexLanguageV1::TypeScriptJavaScript
                        }
                        IndexLanguage::Python => IndexLanguageV1::Python,
                    },
                    file.coverage_basis_points(),
                    file.diagnostic_count().to_string(),
                    file.diagnostics()
                        .iter()
                        .map(|diagnostic| {
                            IndexDiagnosticV1::new(
                                match diagnostic.code() {
                                    ParseDiagnosticCode::SyntaxError => {
                                        IndexDiagnosticCodeV1::SyntaxError
                                    }
                                    ParseDiagnosticCode::MissingSyntax => {
                                        IndexDiagnosticCodeV1::MissingSyntax
                                    }
                                    ParseDiagnosticCode::InvalidEncoding => {
                                        IndexDiagnosticCodeV1::InvalidEncoding
                                    }
                                    ParseDiagnosticCode::UnsupportedSyntax => {
                                        IndexDiagnosticCodeV1::UnsupportedSyntax
                                    }
                                    ParseDiagnosticCode::OutputTruncated => {
                                        IndexDiagnosticCodeV1::OutputTruncated
                                    }
                                },
                                match diagnostic.severity() {
                                    ParseDiagnosticSeverity::Error => {
                                        IndexDiagnosticSeverityV1::Error
                                    }
                                    ParseDiagnosticSeverity::Warning => {
                                        IndexDiagnosticSeverityV1::Warning
                                    }
                                    ParseDiagnosticSeverity::Information => {
                                        IndexDiagnosticSeverityV1::Information
                                    }
                                },
                                diagnostic.message().to_owned(),
                                diagnostic.start_byte(),
                                diagnostic.end_byte(),
                            )
                        })
                        .collect(),
                    file.diagnostics_truncated(),
                )
            })
            .collect(),
        overview.diagnostic_files_truncated(),
    )
}

const fn map_deep_map_budget_to_v1(budget: ExploreBudget) -> DeepMapBudgetV1 {
    DeepMapBudgetV1::new(budget.tokens(), budget.milliseconds(), budget.tool_calls())
}

fn map_deep_map_activity_to_v1(activity: DeepMapActivity) -> DeepMapActivityV1 {
    let progress = activity.progress().and_then(|progress| {
        progress
            .completed()
            .zip(progress.total())
            .map(|(completed, total)| {
                DeepMapProgressV1::new(completed.to_string(), total.to_string())
            })
    });
    DeepMapActivityV1::new(
        match activity.state() {
            DeepMapActivityState::Idle => DeepMapActivityStateV1::Idle,
            DeepMapActivityState::Queued => DeepMapActivityStateV1::Queued,
            DeepMapActivityState::Running => DeepMapActivityStateV1::Running,
            DeepMapActivityState::Pausing => DeepMapActivityStateV1::Pausing,
            DeepMapActivityState::Paused => DeepMapActivityStateV1::Paused,
            DeepMapActivityState::Cancelling => DeepMapActivityStateV1::Cancelling,
            DeepMapActivityState::Succeeded => DeepMapActivityStateV1::Succeeded,
            DeepMapActivityState::Failed => DeepMapActivityStateV1::Failed,
            DeepMapActivityState::Cancelled => DeepMapActivityStateV1::Cancelled,
        },
        activity.budget().map(map_deep_map_budget_to_v1),
        progress,
        activity.completed_steps().to_string(),
        activity.total_steps().to_string(),
    )
}

fn map_deep_map_control_error(error: DeepMapManagerControlError) -> CommandErrorV1 {
    let code = match error {
        DeepMapManagerControlError::NoActiveProject => ErrorCodeV1::NoActiveProject,
        DeepMapManagerControlError::NotRunning => ErrorCodeV1::DeepMapNotRunning,
        DeepMapManagerControlError::NotPaused => ErrorCodeV1::DeepMapNotPaused,
        DeepMapManagerControlError::AlreadyPending => ErrorCodeV1::DeepMapAlreadyPending,
        DeepMapManagerControlError::QueueFull
        | DeepMapManagerControlError::JobIdsExhausted
        | DeepMapManagerControlError::CoordinatorStopped => ErrorCodeV1::DeepMapUnavailable,
    };
    CommandErrorV1::deep_map(code)
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

fn map_index_overview_error_to_v1(error: GetPublishedIndexOverviewError) -> CommandErrorV1 {
    match error {
        GetPublishedIndexOverviewError::Storage(KnowledgeIndexFailure::Storage(error)) => {
            CommandErrorV1::project_open(map_storage_error_to_v1(error))
        }
        GetPublishedIndexOverviewError::Storage(_) => {
            CommandErrorV1::project_open(ErrorCodeV1::LocalStorageUnavailable)
        }
        GetPublishedIndexOverviewError::ProjectionTooLarge => {
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

fn map_deactivation_error_to_v1(error: RepositoryIndexDeactivationError) -> CommandErrorV1 {
    let code = match error {
        RepositoryIndexDeactivationError::NoActiveProject => ErrorCodeV1::NoActiveProject,
        RepositoryIndexDeactivationError::AlreadyPending => ErrorCodeV1::ProjectOperationBusy,
        RepositoryIndexDeactivationError::WatcherShutdown
        | RepositoryIndexDeactivationError::QueueFull
        | RepositoryIndexDeactivationError::CoordinatorStopped => {
            ErrorCodeV1::ProjectRemovalUnavailable
        }
    };
    CommandErrorV1::project_removal(code)
}

fn map_project_removal_error_to_v1(error: RemoveProjectFromListError) -> CommandErrorV1 {
    let RemoveProjectFromListError::Storage(error) = error;
    let code = match error {
        ProjectCatalogAdminFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
        ProjectCatalogAdminFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        ProjectCatalogAdminFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        ProjectCatalogAdminFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
        ProjectCatalogAdminFailure::IdentityConflict => ErrorCodeV1::ProjectIdentityConflict,
        ProjectCatalogAdminFailure::NotFound => ErrorCodeV1::ProjectNotInList,
    };
    CommandErrorV1::project_removal(code)
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
    /// The scheduler stopped before the Deep-Map coordinator could acquire a submit capability.
    DeepMapManagerUnavailable,
    /// The owned Deep-Map coordinator could not be started.
    DeepMapManager,
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
            Self::DeepMapManagerUnavailable => {
                formatter.write_str("Deep Map manager is unavailable")
            }
            Self::DeepMapManager => formatter.write_str("Deep Map manager failed"),
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
            Self::DeepMapManagerUnavailable | Self::DeepMapManager => None,
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
