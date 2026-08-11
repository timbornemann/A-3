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
    CompileWorkspaceTaskLens, CompileWorkspaceTaskLensFailure, CompileWorkspaceTaskLensResult,
    DeepMapExecutor, GetHealth, GetModuleCardDetail, GetModuleCardEvidence, GetModuleCardFreshness,
    GetModuleDependencyGraph, GetModuleRuntimeMap, GetModuleTreePage, GetProjectIndexStatus,
    GetProjectIndexStatusError, GetProjectStorageUsage, GetProjectStorageUsageError,
    GetPublishedIndexOverview, GetPublishedIndexOverviewError, GetRepositoryTreePage,
    GetTaskLensTask, HealthQuery, IndexPersistenceControl, IndexPersistenceControlError,
    JobEventStream, JobScheduler, JobSchedulerConfig, JobSchedulerConfigError,
    JobSchedulerCreateError, KnowledgeIndexFailure, KnowledgeIndexStore, KnowledgeSearchControl,
    KnowledgeSearchStore, KnowledgeStore, KnowledgeStoreFailure, ListRecentProjects,
    ListRecentProjectsError, ListTaskLensTasks, ModuleCardClaimState, ModuleCardCoverageBand,
    ModuleCardDetail, ModuleCardDetailControl, ModuleCardDetailControlError,
    ModuleCardDetailFailure, ModuleCardDetailLoadResult, ModuleCardDetailQuery,
    ModuleCardDetailStore, ModuleCardEvidenceControl, ModuleCardEvidenceControlError,
    ModuleCardEvidenceDetail, ModuleCardEvidenceFailure, ModuleCardEvidenceFreshness,
    ModuleCardEvidenceLoadResult, ModuleCardEvidencePayload, ModuleCardEvidenceQuery,
    ModuleCardEvidenceStore, ModuleCardFreshness, ModuleCardFreshnessControl,
    ModuleCardFreshnessControlError, ModuleCardFreshnessFailure, ModuleCardFreshnessStatus,
    ModuleCardFreshnessStore, ModuleDependencyEdge, ModuleDependencyGraph,
    ModuleDependencyGraphControl, ModuleDependencyGraphControlError, ModuleDependencyGraphFailure,
    ModuleDependencyGraphLoadResult, ModuleDependencyGraphQuery, ModuleDependencyGraphStore,
    ModuleDependencyNode, ModuleDependencyNodeLimit, ModuleDependencyRelation,
    ModuleRuntimeControl, ModuleRuntimeControlError, ModuleRuntimeFailure, ModuleRuntimeFlowKind,
    ModuleRuntimeFlowLoadResult, ModuleRuntimeFlowQuery, ModuleRuntimeMap,
    ModuleRuntimeMapLoadResult, ModuleRuntimeMapQuery, ModuleRuntimeRoot, ModuleRuntimeRootKind,
    ModuleRuntimeRootLimit, ModuleRuntimeRootSet, ModuleRuntimeStore, ModuleTreeChildState,
    ModuleTreeControl, ModuleTreeControlError, ModuleTreeEntry, ModuleTreeEntryKind,
    ModuleTreeFailure, ModuleTreeLoadResult, ModuleTreePage, ModuleTreePageSize, ModuleTreeQuery,
    ModuleTreeStore, OpenProject, OpenProjectError, OpenProjectOutcome, ProjectCatalogAdmin,
    ProjectCatalogAdminFailure, ProjectDirectoryPicker, ProjectIndexStatus,
    ProjectInspectionFailure, ProjectMapSearchQuery, ProjectReconciliationConfirmer,
    ProjectStorageControl, ProjectStorageControlError, ProjectStorageFailure, ProjectStorageStore,
    PublishedIndexOverview, RecentProject, RemoveProjectFromList, RemoveProjectFromListError,
    RepositoryTreeChildName, RepositoryTreeControl, RepositoryTreeControlError,
    RepositoryTreeEntryKind, RepositoryTreeFailure, RepositoryTreePage, RepositoryTreePageSize,
    RepositoryTreeQuery, RepositoryTreeStore, SearchProjectMap, SearchProjectMapFailure,
    TaskLensClaimStore, TaskLensCompilation, TaskLensControl, TaskLensControlError,
    TaskLensIndexStore, TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
    TaskLensWorkspaceStore, TraceModuleRuntimeFlow,
};
use a3_domain::{
    ApplicationVersion, ApplicationVersionError, ExactSearchExplanation, ExactSearchTarget,
    ExploreBudget, FileRevision, FusedRetrievalResult, FusionPriority, GitHead, GraphEdge,
    GraphEndpoint, GraphSymbol, GraphTraversalResult, Health, IndexLanguage, IndexRunId,
    IndexRunStatus, InvalidationReason, LexicalSearchExplanation, LinkResolution,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleId, ModuleKind, ModuleRoot, ParseDiagnosticCode, ParseDiagnosticSeverity, Platform,
    Progress, ProjectId, ProjectIdentity, RepositoryPath, ResolvedModuleCardEvidence,
    ResultSourceExplanation, RetrievalCandidateReason, SnapshotId, SourceChannel, SymbolId,
    SymbolKind, SyntaxProvider, SyntaxRelationKind, TaskId, TaskLensEntryReason, TaskLensTarget,
    TaskStepId, TaskStepStatus, TraversalResultLimit, VerifiedClaimKind, VerifiedClaimStatus,
};
use a3_protocol::{
    CommandErrorV1, CompileTaskLensRequestV1, DeepMapActivityStateV1, DeepMapActivityV1,
    DeepMapBudgetV1, DeepMapConfigurationV1, DeepMapControlResponseV1, DeepMapModelV1,
    DeepMapProgressV1, DeepMapStatusResponseV1, ErrorCodeV1, GitHeadV1, HealthResponseV1,
    IndexActivityResponseV1, IndexActivityStateV1, IndexActivityV1, IndexDiagnosticCodeV1,
    IndexDiagnosticSeverityV1, IndexDiagnosticV1, IndexFileDiagnosticsV1, IndexLanguageV1,
    IndexOverviewCountsV1, IndexOverviewResponseV1, IndexOverviewV1, IndexPhaseV1, IndexStateV1,
    ModuleCardClaimKindV1, ModuleCardClaimStateV1, ModuleCardClaimV1, ModuleCardCoverageBandV1,
    ModuleCardCoverageV1, ModuleCardDetailFieldV1, ModuleCardDetailResponseV1, ModuleCardDetailV1,
    ModuleCardEvidenceFreshnessV1, ModuleCardEvidencePayloadV1, ModuleCardEvidenceRelationV1,
    ModuleCardEvidenceResponseV1, ModuleCardEvidenceRevisionV1, ModuleCardEvidenceV1,
    ModuleCardFieldKindV1, ModuleCardFreshnessCountsV1, ModuleCardFreshnessReasonCountV1,
    ModuleCardFreshnessReasonV1, ModuleCardFreshnessResponseV1, ModuleCardFreshnessStatusV1,
    ModuleCardFreshnessV1, ModuleCardLifecycleV1, ModuleCardValueV1,
    ModuleDependencyEdgeEvidenceV1, ModuleDependencyEdgeV1, ModuleDependencyEndpointV1,
    ModuleDependencyGraphResponseV1, ModuleDependencyGraphV1, ModuleDependencyNodeEvidenceV1,
    ModuleDependencyNodeV1, ModuleDependencyProviderV1, ModuleDependencyRelationV1,
    ModuleDependencyResolutionV1, ModuleDependencySourcePositionV1, ModuleDependencySourceRangeV1,
    ModuleRuntimeFlowEdgeV1, ModuleRuntimeFlowHitV1, ModuleRuntimeFlowKindV1,
    ModuleRuntimeFlowRelationV1, ModuleRuntimeFlowResponseV1, ModuleRuntimeFlowTargetV1,
    ModuleRuntimeFlowV1, ModuleRuntimeMapResponseV1, ModuleRuntimeMapV1, ModuleRuntimeRootKindV1,
    ModuleRuntimeRootSetV1, ModuleRuntimeRootV1, ModuleRuntimeSymbolKindV1, ModuleRuntimeSymbolV1,
    ModuleTreeBoundaryEvidenceV1, ModuleTreeChildStateV1, ModuleTreeEntryKindV1, ModuleTreeEntryV1,
    ModuleTreeFeatureCountV1, ModuleTreePageV1, ModuleTreeResponseV1, ModuleTreeRevisionV1,
    OpenProjectResponseV1, PlatformV1, ProjectIndexStatusV1, ProjectMapExactExplanationV1,
    ProjectMapLexicalExplanationV1, ProjectMapSearchChannelV1, ProjectMapSearchEvidenceV1,
    ProjectMapSearchHitV1, ProjectMapSearchPriorityV1, ProjectMapSearchResponseV1,
    ProjectMapSearchSourceV1, ProjectMapSearchSymbolKindV1, ProjectMapSearchTargetV1,
    ProjectMapSearchV1, ProjectSnapshotV1, ProjectStatusResponseV1, ProjectSummaryV1,
    QueryModuleCardDetailRequestV1, QueryModuleCardEvidenceRequestV1,
    QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
    QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectMapSearchRequestV1,
    QueryRepositoryTreeRequestV1, QueryTaskLensTaskRequestV1, RebuildProjectIndexResponseV1,
    RebuildStateV1, RecentProjectSummaryV1, RecentProjectsResponseV1, RemoveProjectResponseV1,
    RepositoryTreeEntryKindV1, RepositoryTreeEntryV1, RepositoryTreePageV1,
    RepositoryTreeResponseV1, TaskLensClaimEvidenceV1, TaskLensClaimKindV1,
    TaskLensClaimPolarityV1, TaskLensClaimPredicateV1, TaskLensClaimV1, TaskLensCompileResponseV1,
    TaskLensEntryReasonV1, TaskLensEntryTargetV1, TaskLensEntryV1, TaskLensModuleKindV1,
    TaskLensPathV1, TaskLensPriorityV1, TaskLensRetrievalChannelV1, TaskLensRetrievalSourceV1,
    TaskLensStepStatusV1, TaskLensStepV1, TaskLensTaskResponseV1, TaskLensTaskSummaryV1,
    TaskLensTasksResponseV1, TaskLensV1,
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
    module_card_freshness: Option<GetModuleCardFreshness>,
    module_card_detail: Option<GetModuleCardDetail>,
    module_card_evidence: Option<GetModuleCardEvidence>,
    module_dependency_graph: Option<GetModuleDependencyGraph>,
    module_runtime_map: Option<GetModuleRuntimeMap>,
    module_runtime_flow: Option<TraceModuleRuntimeFlow>,
    project_map_search: Option<SearchProjectMap>,
    task_lens_tasks: Option<ListTaskLensTasks>,
    task_lens_task: Option<GetTaskLensTask>,
    task_lens_compile: Option<CompileWorkspaceTaskLens>,
    module_tree: Option<GetModuleTreePage>,
    repository_tree: Option<GetRepositoryTreePage>,
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
            .execute(&active.project, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_index_overview_error_to_v1)
            .map(|overview| match overview {
                Some(overview) => {
                    IndexOverviewResponseV1::published(map_index_overview_to_v1(&overview))
                }
                None => IndexOverviewResponseV1::no_published_index(),
            })
    }

    /// Returns authoritative latest-card lifecycle counts without card contents or paths.
    pub async fn query_module_card_freshness(
        &self,
    ) -> Result<ModuleCardFreshnessResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleCardFreshnessResponseV1::no_project());
        };
        let Some(query) = &self.module_card_freshness else {
            return Ok(ModuleCardFreshnessResponseV1::no_published_index());
        };
        query
            .execute(&active.project, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_card_freshness_error_to_v1)
            .map(|freshness| match freshness {
                Some(freshness) => ModuleCardFreshnessResponseV1::available(
                    map_module_card_freshness_to_v1(&freshness),
                ),
                None => ModuleCardFreshnessResponseV1::no_published_index(),
            })
    }

    /// Returns one bounded progressive page from the current published repository tree.
    pub async fn query_repository_tree(
        &self,
        query: &RepositoryTreeQuery,
    ) -> Result<RepositoryTreeResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(RepositoryTreeResponseV1::no_project());
        };
        let Some(reader) = &self.repository_tree else {
            return Ok(RepositoryTreeResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_repository_tree_error_to_v1)
            .map(|page| match page {
                Some(page) => {
                    RepositoryTreeResponseV1::available(map_repository_tree_page_to_v1(&page))
                }
                None => RepositoryTreeResponseV1::no_published_index(),
            })
    }

    /// Returns one bounded progressive page from the current deterministic module projection.
    pub async fn query_module_tree(
        &self,
        query: &ModuleTreeQuery,
    ) -> Result<ModuleTreeResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleTreeResponseV1::no_project());
        };
        let Some(reader) = &self.module_tree else {
            return Ok(ModuleTreeResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_tree_error_to_v1)
            .map(|result| match result {
                ModuleTreeLoadResult::NoPublishedIndex => {
                    ModuleTreeResponseV1::no_published_index()
                }
                ModuleTreeLoadResult::ProjectionUnavailable => {
                    ModuleTreeResponseV1::projection_unavailable()
                }
                ModuleTreeLoadResult::Page(page) => {
                    ModuleTreeResponseV1::available(map_module_tree_page_to_v1(&page))
                }
            })
    }

    /// Returns the latest durable verified Card for one explicit current primary module.
    pub async fn query_module_card_detail(
        &self,
        query: &ModuleCardDetailQuery,
    ) -> Result<ModuleCardDetailResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleCardDetailResponseV1::no_project());
        };
        let Some(reader) = &self.module_card_detail else {
            return Ok(ModuleCardDetailResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_card_detail_error_to_v1)
            .map(|result| match result {
                ModuleCardDetailLoadResult::NoPublishedIndex => {
                    ModuleCardDetailResponseV1::no_published_index()
                }
                ModuleCardDetailLoadResult::ProjectionUnavailable => {
                    ModuleCardDetailResponseV1::projection_unavailable()
                }
                ModuleCardDetailLoadResult::ModuleUnavailable => {
                    ModuleCardDetailResponseV1::module_unavailable()
                }
                ModuleCardDetailLoadResult::CardUnavailable => {
                    ModuleCardDetailResponseV1::card_unavailable()
                }
                ModuleCardDetailLoadResult::Detail(detail) => {
                    ModuleCardDetailResponseV1::available(map_module_card_detail_to_v1(&detail))
                }
            })
    }

    /// Resolves one exact Evidence hook of the still-selected latest durable Card.
    pub async fn query_module_card_evidence(
        &self,
        query: &ModuleCardEvidenceQuery,
    ) -> Result<ModuleCardEvidenceResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleCardEvidenceResponseV1::no_project());
        };
        let Some(reader) = &self.module_card_evidence else {
            return Ok(ModuleCardEvidenceResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_card_evidence_error_to_v1)
            .map(|result| match result {
                ModuleCardEvidenceLoadResult::NoPublishedIndex => {
                    ModuleCardEvidenceResponseV1::no_published_index()
                }
                ModuleCardEvidenceLoadResult::ProjectionUnavailable => {
                    ModuleCardEvidenceResponseV1::projection_unavailable()
                }
                ModuleCardEvidenceLoadResult::ModuleUnavailable => {
                    ModuleCardEvidenceResponseV1::module_unavailable()
                }
                ModuleCardEvidenceLoadResult::CardUnavailable => {
                    ModuleCardEvidenceResponseV1::card_unavailable()
                }
                ModuleCardEvidenceLoadResult::SelectionChanged => {
                    ModuleCardEvidenceResponseV1::selection_changed()
                }
                ModuleCardEvidenceLoadResult::EvidenceUnavailable => {
                    ModuleCardEvidenceResponseV1::evidence_unavailable()
                }
                ModuleCardEvidenceLoadResult::Detail(detail) => {
                    ModuleCardEvidenceResponseV1::available(map_module_card_evidence_to_v1(&detail))
                }
            })
    }

    /// Returns an evidence-bound bounded neighborhood around one current primary module.
    pub async fn query_module_dependency_graph(
        &self,
        query: &ModuleDependencyGraphQuery,
    ) -> Result<ModuleDependencyGraphResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleDependencyGraphResponseV1::no_project());
        };
        let Some(reader) = &self.module_dependency_graph else {
            return Ok(ModuleDependencyGraphResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_dependency_graph_error_to_v1)
            .map(|result| match result {
                ModuleDependencyGraphLoadResult::NoPublishedIndex => {
                    ModuleDependencyGraphResponseV1::no_published_index()
                }
                ModuleDependencyGraphLoadResult::ProjectionUnavailable => {
                    ModuleDependencyGraphResponseV1::projection_unavailable()
                }
                ModuleDependencyGraphLoadResult::CenterUnavailable => {
                    ModuleDependencyGraphResponseV1::center_unavailable()
                }
                ModuleDependencyGraphLoadResult::Graph(graph) => {
                    ModuleDependencyGraphResponseV1::available(map_module_dependency_graph_to_v1(
                        &graph,
                    ))
                }
            })
    }

    /// Returns bounded current entrypoint and test roots for one primary module.
    pub async fn query_module_runtime_map(
        &self,
        query: &ModuleRuntimeMapQuery,
    ) -> Result<ModuleRuntimeMapResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleRuntimeMapResponseV1::no_project());
        };
        let Some(reader) = &self.module_runtime_map else {
            return Ok(ModuleRuntimeMapResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_runtime_error_to_v1)
            .map(|result| match result {
                ModuleRuntimeMapLoadResult::NoPublishedIndex => {
                    ModuleRuntimeMapResponseV1::no_published_index()
                }
                ModuleRuntimeMapLoadResult::ProjectionUnavailable => {
                    ModuleRuntimeMapResponseV1::projection_unavailable()
                }
                ModuleRuntimeMapLoadResult::ModuleUnavailable => {
                    ModuleRuntimeMapResponseV1::module_unavailable()
                }
                ModuleRuntimeMapLoadResult::Map(map) => {
                    ModuleRuntimeMapResponseV1::available(map_module_runtime_map_to_v1(&map))
                }
            })
    }

    /// Traverses one fixed role-specific preset after revalidating the visible publication seed.
    pub async fn query_module_runtime_flow(
        &self,
        query: &ModuleRuntimeFlowQuery,
    ) -> Result<ModuleRuntimeFlowResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ModuleRuntimeFlowResponseV1::no_project());
        };
        let Some(reader) = &self.module_runtime_flow else {
            return Ok(ModuleRuntimeFlowResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_module_runtime_error_to_v1)
            .map(|result| match result {
                ModuleRuntimeFlowLoadResult::NoPublishedIndex => {
                    ModuleRuntimeFlowResponseV1::no_published_index()
                }
                ModuleRuntimeFlowLoadResult::ProjectionUnavailable => {
                    ModuleRuntimeFlowResponseV1::projection_unavailable()
                }
                ModuleRuntimeFlowLoadResult::PublicationChanged => {
                    ModuleRuntimeFlowResponseV1::publication_changed()
                }
                ModuleRuntimeFlowLoadResult::ModuleUnavailable => {
                    ModuleRuntimeFlowResponseV1::module_unavailable()
                }
                ModuleRuntimeFlowLoadResult::RootUnavailable => {
                    ModuleRuntimeFlowResponseV1::root_unavailable()
                }
                ModuleRuntimeFlowLoadResult::Flow(flow) => ModuleRuntimeFlowResponseV1::available(
                    map_module_runtime_flow_to_v1(query, &flow),
                ),
            })
    }

    /// Searches current deterministic projections without exposing source or storage capabilities.
    pub async fn query_project_map_search(
        &self,
        query: &ProjectMapSearchQuery,
    ) -> Result<ProjectMapSearchResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapSearchResponseV1::no_project());
        };
        let Some(search) = &self.project_map_search else {
            return Ok(ProjectMapSearchResponseV1::no_published_index());
        };
        match search
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
        {
            Ok(result) => map_project_map_search_to_v1(query, &result)
                .map(ProjectMapSearchResponseV1::available)
                .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)),
            Err(SearchProjectMapFailure::Search(
                a3_application::KnowledgeSearchFailure::IndexUnavailable,
            )) => Ok(ProjectMapSearchResponseV1::no_published_index()),
            Err(SearchProjectMapFailure::Search(
                a3_application::KnowledgeSearchFailure::ProjectionUnavailable(channel),
            )) => map_project_map_search_channel_to_v1(channel)
                .map(ProjectMapSearchResponseV1::projection_unavailable)
                .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)),
            Err(error) => Err(map_project_map_search_error_to_v1(error)),
        }
    }

    /// Lists bounded durable Goal Contracts available for Task Lens selection.
    pub async fn query_task_lens_tasks(&self) -> Result<TaskLensTasksResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(TaskLensTasksResponseV1::no_project());
        };
        let reader = self
            .task_lens_tasks
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::TaskLensUnavailable))?;
        let page = reader
            .execute(&active.project, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_task_lens_workspace_error_to_v1)?;
        Ok(TaskLensTasksResponseV1::available(
            page.goals()
                .iter()
                .map(map_task_lens_summary_to_v1)
                .collect(),
            page.truncated(),
        ))
    }

    /// Loads current active-plan steps for one opaque durable task identity.
    pub async fn query_task_lens_task(
        &self,
        task_id: TaskId,
    ) -> Result<TaskLensTaskResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(TaskLensTaskResponseV1::no_project());
        };
        let reader = self
            .task_lens_task
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::TaskLensUnavailable))?;
        match reader
            .execute(&active.project, task_id, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_task_lens_workspace_error_to_v1)?
        {
            TaskLensTaskLoadResult::NotFound => Ok(TaskLensTaskResponseV1::task_not_found()),
            TaskLensTaskLoadResult::LedgerUnavailable { goal_contract } => {
                Ok(TaskLensTaskResponseV1::ledger_unavailable(
                    map_task_lens_summary_to_v1(&goal_contract),
                ))
            }
            TaskLensTaskLoadResult::GoalRevisionMismatch {
                current_goal,
                ledger_goal,
            } => Ok(TaskLensTaskResponseV1::goal_revision_mismatch(
                current_goal.task_id().to_string(),
                current_goal.revision().get(),
                ledger_goal.revision().get(),
            )),
            TaskLensTaskLoadResult::Available(anchor) => {
                let stored = anchor.task_ledger();
                let steps = stored
                    .ledger()
                    .steps()
                    .filter(|step| step.is_active_plan_step())
                    .map(|step| {
                        TaskLensStepV1::new(
                            step.definition().id().to_string(),
                            step.definition().intended_outcome().as_str().to_owned(),
                            map_task_lens_step_status_to_v1(step.status()),
                        )
                    })
                    .collect();
                Ok(TaskLensTaskResponseV1::available(
                    map_task_lens_summary_to_v1(anchor.goal_contract()),
                    stored.ledger().revision().get(),
                    stored.version().get().to_string(),
                    steps,
                ))
            }
        }
    }

    /// Recompiles the selected current durable task/step through the existing R10 pipeline.
    pub async fn compile_task_lens(
        &self,
        task_id: TaskId,
        step_id: TaskStepId,
    ) -> Result<TaskLensCompileResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(TaskLensCompileResponseV1::no_project());
        };
        let compiler = self
            .task_lens_compile
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::TaskLensUnavailable))?;
        let control = DesktopBoundedReadControl::new();
        let result = compiler
            .execute(&active.project, task_id, step_id, &control, &control)
            .await
            .map_err(map_task_lens_compile_error_to_v1)?;
        match result {
            CompileWorkspaceTaskLensResult::TaskNotFound => {
                Ok(TaskLensCompileResponseV1::task_not_found())
            }
            CompileWorkspaceTaskLensResult::LedgerUnavailable => {
                Ok(TaskLensCompileResponseV1::ledger_unavailable())
            }
            CompileWorkspaceTaskLensResult::GoalRevisionMismatch {
                current_goal,
                ledger_goal,
            } => Ok(TaskLensCompileResponseV1::goal_revision_mismatch(
                current_goal.task_id().to_string(),
                current_goal.revision().get(),
                ledger_goal.revision().get(),
            )),
            CompileWorkspaceTaskLensResult::StepUnavailable => {
                Ok(TaskLensCompileResponseV1::step_unavailable())
            }
            CompileWorkspaceTaskLensResult::IndexUnavailable => {
                Ok(TaskLensCompileResponseV1::no_published_index())
            }
            CompileWorkspaceTaskLensResult::Available(compilation) => {
                map_task_lens_to_v1(&compilation)
                    .map(TaskLensCompileResponseV1::available)
                    .ok_or_else(|| {
                        CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)
                    })
            }
        }
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
struct DesktopBoundedReadControl {
    completed: AtomicU64,
    total: AtomicU64,
}

impl DesktopBoundedReadControl {
    fn new() -> Self {
        Self {
            completed: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }
}

impl IndexPersistenceControl for DesktopBoundedReadControl {
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

impl ModuleCardFreshnessControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardFreshnessControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleCardFreshnessControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleCardFreshnessControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleCardFreshnessControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ModuleCardDetailControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardDetailControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleCardDetailControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleCardDetailControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleCardDetailControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ModuleCardEvidenceControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardEvidenceControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleCardEvidenceControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleCardEvidenceControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleCardEvidenceControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl RepositoryTreeControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryTreeControlError> {
        let completed = progress
            .completed()
            .ok_or(RepositoryTreeControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(RepositoryTreeControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(RepositoryTreeControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ModuleTreeControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleTreeControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleTreeControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleTreeControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleTreeControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ModuleDependencyGraphControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleDependencyGraphControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleDependencyGraphControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleDependencyGraphControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleDependencyGraphControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ModuleRuntimeControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleRuntimeControlError> {
        let completed = progress
            .completed()
            .ok_or(ModuleRuntimeControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ModuleRuntimeControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ModuleRuntimeControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl KnowledgeSearchControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl TaskLensWorkspaceControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl TaskLensControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), TaskLensControlError> {
        let completed = progress
            .completed()
            .ok_or(TaskLensControlError::Unavailable)?;
        let total = progress.total().ok_or(TaskLensControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(TaskLensControlError::Unavailable);
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
    module_card_freshness_store: Option<Arc<dyn ModuleCardFreshnessStore>>,
    module_card_detail_store: Option<Arc<dyn ModuleCardDetailStore>>,
    module_card_evidence_store: Option<Arc<dyn ModuleCardEvidenceStore>>,
    module_dependency_graph_store: Option<Arc<dyn ModuleDependencyGraphStore>>,
    module_runtime_store: Option<Arc<dyn ModuleRuntimeStore>>,
    knowledge_search_store: Option<Arc<dyn KnowledgeSearchStore>>,
    task_lens_index_store: Option<Arc<dyn TaskLensIndexStore>>,
    task_lens_claim_store: Option<Arc<dyn TaskLensClaimStore>>,
    task_lens_workspace_store: Option<Arc<dyn TaskLensWorkspaceStore>>,
    module_tree_store: Option<Arc<dyn ModuleTreeStore>>,
    repository_tree_store: Option<Arc<dyn RepositoryTreeStore>>,
    project_storage: Option<Arc<dyn ProjectStorageStore>>,
    project_catalog_admin: Option<Arc<dyn ProjectCatalogAdmin>>,
    deep_map_executor: Option<Arc<dyn DeepMapExecutor>>,
}

struct IndexingCompositionPorts {
    index_store: Arc<dyn KnowledgeIndexStore>,
    module_card_freshness_store: Arc<dyn ModuleCardFreshnessStore>,
    module_card_detail_store: Arc<dyn ModuleCardDetailStore>,
    module_card_evidence_store: Arc<dyn ModuleCardEvidenceStore>,
    module_dependency_graph_store: Arc<dyn ModuleDependencyGraphStore>,
    module_runtime_store: Arc<dyn ModuleRuntimeStore>,
    knowledge_search_store: Arc<dyn KnowledgeSearchStore>,
    task_lens_index_store: Arc<dyn TaskLensIndexStore>,
    task_lens_claim_store: Arc<dyn TaskLensClaimStore>,
    task_lens_workspace_store: Arc<dyn TaskLensWorkspaceStore>,
    module_tree_store: Arc<dyn ModuleTreeStore>,
    repository_tree_store: Arc<dyn RepositoryTreeStore>,
    project_storage: Arc<dyn ProjectStorageStore>,
    project_catalog_admin: Arc<dyn ProjectCatalogAdmin>,
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
        ports: IndexingCompositionPorts,
    ) -> Result<CompositionRoot, CompositionRootError> {
        self.finish_internal(
            project_directory_picker,
            project_reconciliation_confirmer,
            store,
            OptionalCompositionPorts {
                index_store: Some(ports.index_store),
                module_card_freshness_store: Some(ports.module_card_freshness_store),
                module_card_detail_store: Some(ports.module_card_detail_store),
                module_card_evidence_store: Some(ports.module_card_evidence_store),
                module_dependency_graph_store: Some(ports.module_dependency_graph_store),
                module_runtime_store: Some(ports.module_runtime_store),
                knowledge_search_store: Some(ports.knowledge_search_store),
                task_lens_index_store: Some(ports.task_lens_index_store),
                task_lens_claim_store: Some(ports.task_lens_claim_store),
                task_lens_workspace_store: Some(ports.task_lens_workspace_store),
                module_tree_store: Some(ports.module_tree_store),
                repository_tree_store: Some(ports.repository_tree_store),
                project_storage: Some(ports.project_storage),
                project_catalog_admin: Some(ports.project_catalog_admin),
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
        let module_card_freshness = ports
            .module_card_freshness_store
            .map(GetModuleCardFreshness::new);
        let module_card_detail = ports.module_card_detail_store.map(GetModuleCardDetail::new);
        let module_card_evidence = ports
            .module_card_evidence_store
            .map(GetModuleCardEvidence::new);
        let module_dependency_graph = ports
            .module_dependency_graph_store
            .map(GetModuleDependencyGraph::new);
        let module_runtime_map = ports
            .module_runtime_store
            .clone()
            .map(GetModuleRuntimeMap::new);
        let module_runtime_flow = ports
            .module_runtime_store
            .zip(ports.knowledge_search_store.clone())
            .map(|(runtime, search)| TraceModuleRuntimeFlow::new(runtime, search));
        let project_map_search = ports
            .knowledge_search_store
            .clone()
            .map(SearchProjectMap::new);
        let task_lens_tasks = ports
            .task_lens_workspace_store
            .clone()
            .map(ListTaskLensTasks::new);
        let task_lens_task = ports
            .task_lens_workspace_store
            .clone()
            .map(GetTaskLensTask::new);
        let task_lens_compile = match (
            ports.task_lens_workspace_store.as_ref(),
            ports.task_lens_index_store.as_ref(),
            ports.knowledge_search_store.as_ref(),
            ports.task_lens_claim_store.as_ref(),
        ) {
            (Some(workspace), Some(index), Some(search), Some(claims)) => {
                Some(CompileWorkspaceTaskLens::new(
                    Arc::clone(workspace),
                    Arc::clone(index),
                    Arc::clone(search),
                    Arc::clone(claims),
                ))
            }
            _ => None,
        };
        let module_tree = ports.module_tree_store.map(GetModuleTreePage::new);
        let repository_tree = ports.repository_tree_store.map(GetRepositoryTreePage::new);
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
            module_card_freshness,
            module_card_detail,
            module_card_evidence,
            module_dependency_graph,
            module_runtime_map,
            module_runtime_flow,
            project_map_search,
            task_lens_tasks,
            task_lens_task,
            task_lens_compile,
            module_tree,
            repository_tree,
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
            let module_card_freshness_store: Arc<dyn ModuleCardFreshnessStore> = store.clone();
            let module_card_detail_store: Arc<dyn ModuleCardDetailStore> = store.clone();
            let module_card_evidence_store: Arc<dyn ModuleCardEvidenceStore> = store.clone();
            let module_dependency_graph_store: Arc<dyn ModuleDependencyGraphStore> = store.clone();
            let module_runtime_store: Arc<dyn ModuleRuntimeStore> = store.clone();
            let knowledge_search_store: Arc<dyn KnowledgeSearchStore> = store.clone();
            let task_lens_index_store: Arc<dyn TaskLensIndexStore> = store.clone();
            let task_lens_claim_store: Arc<dyn TaskLensClaimStore> = store.clone();
            let task_lens_workspace_store: Arc<dyn TaskLensWorkspaceStore> = store.clone();
            let module_tree_store: Arc<dyn ModuleTreeStore> = store.clone();
            let repository_tree_store: Arc<dyn RepositoryTreeStore> = store.clone();
            let catalog_store: Arc<dyn KnowledgeStore> = store.clone();
            let index_store: Arc<dyn KnowledgeIndexStore> = store;
            app.manage(base.finish_with_indexing(
                Arc::new(NativeProjectDirectoryPicker::new(app.handle().clone())),
                Arc::new(NativeProjectReconciliationConfirmer::new(
                    app.handle().clone(),
                )),
                catalog_store,
                IndexingCompositionPorts {
                    index_store,
                    module_card_freshness_store,
                    module_card_detail_store,
                    module_card_evidence_store,
                    module_dependency_graph_store,
                    module_runtime_store,
                    knowledge_search_store,
                    task_lens_index_store,
                    task_lens_claim_store,
                    task_lens_workspace_store,
                    module_tree_store,
                    repository_tree_store,
                    project_storage,
                    project_catalog_admin,
                },
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cancel_deep_map,
            commands::compile_task_lens,
            commands::list_recent_projects,
            commands::open_project,
            commands::pause_deep_map,
            commands::query_deep_map,
            commands::query_project_status,
            commands::query_index_activity,
            commands::query_index_overview,
            commands::query_module_card_freshness,
            commands::query_module_card_detail,
            commands::query_module_card_evidence,
            commands::query_module_dependency_graph,
            commands::query_module_runtime_flow,
            commands::query_module_runtime_map,
            commands::query_module_tree,
            commands::query_project_map_search,
            commands::query_task_lens_task,
            commands::query_task_lens_tasks,
            commands::query_repository_tree,
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

fn map_module_card_freshness_to_v1(freshness: &ModuleCardFreshness) -> ModuleCardFreshnessV1 {
    ModuleCardFreshnessV1::new(
        freshness.index_run_id().to_string(),
        freshness.snapshot_id().to_string(),
        ModuleCardFreshnessCountsV1::new(
            freshness.published_count().to_string(),
            freshness.stale_count().to_string(),
            freshness.needs_review_count().to_string(),
            freshness.total_count().to_string(),
        ),
        freshness
            .reason_counts()
            .iter()
            .map(|reason| {
                ModuleCardFreshnessReasonCountV1::new(
                    match reason.status() {
                        ModuleCardFreshnessStatus::Stale => ModuleCardFreshnessStatusV1::Stale,
                        ModuleCardFreshnessStatus::NeedsReview => {
                            ModuleCardFreshnessStatusV1::NeedsReview
                        }
                    },
                    match reason.reason() {
                        InvalidationReason::EvidenceChanged => {
                            ModuleCardFreshnessReasonV1::EvidenceChanged
                        }
                        InvalidationReason::ModuleRemoved => {
                            ModuleCardFreshnessReasonV1::ModuleRemoved
                        }
                        InvalidationReason::ParserVersionChanged => {
                            ModuleCardFreshnessReasonV1::ParserVersionChanged
                        }
                        InvalidationReason::MapperVersionChanged => {
                            ModuleCardFreshnessReasonV1::MapperVersionChanged
                        }
                        InvalidationReason::DirectDependencyChanged => {
                            ModuleCardFreshnessReasonV1::DirectDependencyChanged
                        }
                    },
                    reason.count().to_string(),
                )
            })
            .collect(),
    )
}

pub(crate) fn map_module_card_detail_query_from_v1(
    request: &QueryModuleCardDetailRequestV1,
) -> Result<ModuleCardDetailQuery, CommandErrorV1> {
    decode_module_id(request.module_id())
        .map(ModuleCardDetailQuery::new)
        .map_err(|()| invalid_module_card_detail_query())
}

pub(crate) fn map_module_card_evidence_query_from_v1(
    request: &QueryModuleCardEvidenceRequestV1,
) -> Result<ModuleCardEvidenceQuery, CommandErrorV1> {
    let current_index_run_id = decode_index_run_id(request.current_index_run_id())
        .map_err(|()| invalid_module_card_evidence_query())?;
    let current_snapshot_id = decode_snapshot_id(request.current_snapshot_id())
        .map_err(|()| invalid_module_card_evidence_query())?;
    let source_index_run_id = decode_index_run_id(request.source_index_run_id())
        .map_err(|()| invalid_module_card_evidence_query())?;
    let source_snapshot_id = decode_snapshot_id(request.source_snapshot_id())
        .map_err(|()| invalid_module_card_evidence_query())?;
    if source_index_run_id == current_index_run_id && source_snapshot_id != current_snapshot_id {
        return Err(invalid_module_card_evidence_query());
    }
    Ok(ModuleCardEvidenceQuery::new(
        current_index_run_id,
        current_snapshot_id,
        source_index_run_id,
        source_snapshot_id,
        decode_stable_id(request.card_id())
            .map(ModuleCardId::from_bytes)
            .map_err(|()| invalid_module_card_evidence_query())?,
        decode_module_id(request.module_id()).map_err(|()| invalid_module_card_evidence_query())?,
        decode_stable_id(request.evidence_id())
            .map(ModuleCardEvidenceId::from_bytes)
            .map_err(|()| invalid_module_card_evidence_query())?,
    ))
}

fn map_module_card_evidence_to_v1(detail: &ModuleCardEvidenceDetail) -> ModuleCardEvidenceV1 {
    let revision = detail.payload().revision();
    let mapped_revision = || {
        ModuleCardEvidenceRevisionV1::new(
            encode_hex(revision.path().as_bytes()),
            encode_hex(revision.content_hash().as_bytes()),
        )
    };
    let payload = match detail.payload() {
        ModuleCardEvidencePayload::File { .. } => ModuleCardEvidencePayloadV1::File {
            revision: mapped_revision(),
        },
        ModuleCardEvidencePayload::Symbol { symbol_id, .. } => {
            ModuleCardEvidencePayloadV1::Symbol {
                symbol_id: symbol_id.to_string(),
                revision: mapped_revision(),
            }
        }
        ModuleCardEvidencePayload::GraphEdge { edge } => ModuleCardEvidencePayloadV1::GraphEdge {
            relation: map_module_card_evidence_relation_to_v1(edge.kind()),
            edge: Box::new(map_graph_edge_evidence_to_v1(edge)),
        },
    };
    ModuleCardEvidenceV1::new(
        detail.current_index_run_id().to_string(),
        detail.current_snapshot_id().to_string(),
        detail.source_index_run_id().to_string(),
        detail.source_snapshot_id().to_string(),
        encode_hex(detail.card_id().as_bytes()),
        detail.module_id().to_string(),
        encode_hex(detail.evidence_id().as_bytes()),
        map_module_card_lifecycle_to_v1(detail.card_lifecycle()),
        match detail.freshness() {
            ModuleCardEvidenceFreshness::Current => ModuleCardEvidenceFreshnessV1::Current,
            ModuleCardEvidenceFreshness::Stale => ModuleCardEvidenceFreshnessV1::Stale,
        },
        payload,
    )
}

fn map_module_card_evidence_relation_to_v1(
    relation: SyntaxRelationKind,
) -> ModuleCardEvidenceRelationV1 {
    match relation {
        SyntaxRelationKind::Contains => ModuleCardEvidenceRelationV1::Contains,
        SyntaxRelationKind::Defines => ModuleCardEvidenceRelationV1::Defines,
        SyntaxRelationKind::Imports => ModuleCardEvidenceRelationV1::Imports,
        SyntaxRelationKind::Exports => ModuleCardEvidenceRelationV1::Exports,
        SyntaxRelationKind::Calls => ModuleCardEvidenceRelationV1::Calls,
        SyntaxRelationKind::Implements => ModuleCardEvidenceRelationV1::Implements,
        SyntaxRelationKind::Extends => ModuleCardEvidenceRelationV1::Extends,
        SyntaxRelationKind::Reads => ModuleCardEvidenceRelationV1::Reads,
        SyntaxRelationKind::Writes => ModuleCardEvidenceRelationV1::Writes,
        SyntaxRelationKind::Configures => ModuleCardEvidenceRelationV1::Configures,
        SyntaxRelationKind::Tests => ModuleCardEvidenceRelationV1::Tests,
        SyntaxRelationKind::Builds => ModuleCardEvidenceRelationV1::Builds,
        SyntaxRelationKind::Documents => ModuleCardEvidenceRelationV1::Documents,
    }
}

fn map_module_card_detail_to_v1(detail: &ModuleCardDetail) -> ModuleCardDetailV1 {
    ModuleCardDetailV1::new(
        detail.current_index_run_id().to_string(),
        detail.current_snapshot_id().to_string(),
        detail.source_index_run_id().to_string(),
        detail.source_snapshot_id().to_string(),
        encode_hex(detail.id().as_bytes()),
        detail.module_id().to_string(),
        detail.schema_version().get(),
        detail.mapper_profile_version().get(),
        detail.confidence().basis_points(),
        ModuleCardCoverageV1::new(
            detail.coverage().basis_points(),
            detail.coverage().covered_field_count(),
            detail.coverage().total_field_count(),
            map_module_card_coverage_band_to_v1(detail.coverage().must()),
            map_module_card_coverage_band_to_v1(detail.coverage().should()),
        ),
        map_module_card_lifecycle_to_v1(detail.lifecycle()),
        detail
            .fields()
            .iter()
            .map(|field| {
                ModuleCardDetailFieldV1::new(
                    map_module_card_field_to_v1(field.field()),
                    field
                        .evidence_ids()
                        .iter()
                        .map(|id| encode_hex(id.as_bytes()))
                        .collect(),
                    field
                        .values()
                        .iter()
                        .map(|value| {
                            let claim = value.claim();
                            ModuleCardValueV1::new(
                                value.value().to_owned(),
                                ModuleCardClaimV1::new(
                                    encode_hex(claim.id().as_bytes()),
                                    match claim.kind() {
                                        VerifiedClaimKind::Fact => ModuleCardClaimKindV1::Fact,
                                        VerifiedClaimKind::Observation => {
                                            ModuleCardClaimKindV1::Observation
                                        }
                                        VerifiedClaimKind::Hypothesis => {
                                            ModuleCardClaimKindV1::Hypothesis
                                        }
                                    },
                                    match claim.state() {
                                        ModuleCardClaimState::Current => {
                                            ModuleCardClaimStateV1::Current
                                        }
                                        ModuleCardClaimState::Stale => {
                                            ModuleCardClaimStateV1::Stale
                                        }
                                        ModuleCardClaimState::NeedsReview => {
                                            ModuleCardClaimStateV1::NeedsReview
                                        }
                                    },
                                    claim.confidence().basis_points(),
                                    claim
                                        .evidence_ids()
                                        .iter()
                                        .map(|id| encode_hex(id.as_bytes()))
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn map_module_card_coverage_band_to_v1(band: &ModuleCardCoverageBand) -> ModuleCardCoverageBandV1 {
    ModuleCardCoverageBandV1::new(
        band.basis_points(),
        band.covered_field_count(),
        band.total_field_count(),
        band.missing_fields()
            .iter()
            .copied()
            .map(map_module_card_field_to_v1)
            .collect(),
    )
}

fn map_module_card_lifecycle_to_v1(
    lifecycle: a3_application::ModuleCardLifecycle,
) -> ModuleCardLifecycleV1 {
    match lifecycle {
        a3_application::ModuleCardLifecycle::Current => ModuleCardLifecycleV1::Current,
        a3_application::ModuleCardLifecycle::Stale {
            invalidated_by_index_run_id,
            reason,
        } => ModuleCardLifecycleV1::Stale {
            invalidated_by_index_run_id: invalidated_by_index_run_id.to_string(),
            reason: map_invalidation_reason_to_v1(reason),
        },
        a3_application::ModuleCardLifecycle::NeedsReview {
            invalidated_by_index_run_id,
            reason,
        } => ModuleCardLifecycleV1::NeedsReview {
            invalidated_by_index_run_id: invalidated_by_index_run_id.to_string(),
            reason: map_invalidation_reason_to_v1(reason),
        },
    }
}

const fn map_module_card_field_to_v1(field: ModuleCardField) -> ModuleCardFieldKindV1 {
    match field {
        ModuleCardField::Title => ModuleCardFieldKindV1::Title,
        ModuleCardField::Paths => ModuleCardFieldKindV1::Paths,
        ModuleCardField::Purpose => ModuleCardFieldKindV1::Purpose,
        ModuleCardField::Responsibilities => ModuleCardFieldKindV1::Responsibilities,
        ModuleCardField::PublicSurface => ModuleCardFieldKindV1::PublicSurface,
        ModuleCardField::Entrypoints => ModuleCardFieldKindV1::Entrypoints,
        ModuleCardField::Dependencies => ModuleCardFieldKindV1::Dependencies,
        ModuleCardField::DataFlows => ModuleCardFieldKindV1::DataFlows,
        ModuleCardField::Invariants => ModuleCardFieldKindV1::Invariants,
        ModuleCardField::Tests => ModuleCardFieldKindV1::Tests,
        ModuleCardField::Risks => ModuleCardFieldKindV1::Risks,
        ModuleCardField::OpenQuestions => ModuleCardFieldKindV1::OpenQuestions,
    }
}

const fn map_invalidation_reason_to_v1(reason: InvalidationReason) -> ModuleCardFreshnessReasonV1 {
    match reason {
        InvalidationReason::EvidenceChanged => ModuleCardFreshnessReasonV1::EvidenceChanged,
        InvalidationReason::ModuleRemoved => ModuleCardFreshnessReasonV1::ModuleRemoved,
        InvalidationReason::ParserVersionChanged => {
            ModuleCardFreshnessReasonV1::ParserVersionChanged
        }
        InvalidationReason::MapperVersionChanged => {
            ModuleCardFreshnessReasonV1::MapperVersionChanged
        }
        InvalidationReason::DirectDependencyChanged => {
            ModuleCardFreshnessReasonV1::DirectDependencyChanged
        }
    }
}

pub(crate) fn map_module_tree_query_from_v1(
    request: &QueryModuleTreeRequestV1,
) -> Result<ModuleTreeQuery, CommandErrorV1> {
    let parent_module_id = request
        .parent_module_id()
        .map(decode_module_id)
        .transpose()
        .map_err(|()| invalid_module_tree_query())?;
    let after_module_id = request
        .after_module_id()
        .map(decode_module_id)
        .transpose()
        .map_err(|()| invalid_module_tree_query())?;
    let page_size =
        ModuleTreePageSize::new(request.limit()).map_err(|_| invalid_module_tree_query())?;
    Ok(ModuleTreeQuery::new(
        parent_module_id,
        after_module_id,
        page_size,
    ))
}

fn map_module_tree_page_to_v1(page: &ModuleTreePage) -> ModuleTreePageV1 {
    ModuleTreePageV1::new(
        page.index_run_id().to_string(),
        page.snapshot_id().to_string(),
        page.parent_module_id().map(|id| id.to_string()),
        page.primary_module_count().to_string(),
        page.graph_community_count().to_string(),
        page.entries()
            .iter()
            .map(map_module_tree_entry_to_v1)
            .collect(),
        page.next_cursor().map(|id| id.to_string()),
    )
}

fn map_module_tree_entry_to_v1(entry: &ModuleTreeEntry) -> ModuleTreeEntryV1 {
    ModuleTreeEntryV1::new(
        entry.module_id().to_string(),
        match entry.kind() {
            ModuleTreeEntryKind::ManifestBoundary => ModuleTreeEntryKindV1::ManifestBoundary,
            ModuleTreeEntryKind::PathBoundary => ModuleTreeEntryKindV1::PathBoundary,
        },
        match entry.root() {
            ModuleRoot::Repository => None,
            ModuleRoot::Directory(path) => Some(encode_hex(path.as_bytes())),
        },
        entry.display_name().as_str().to_owned(),
        entry.display_name().is_truncated(),
        ModuleTreeBoundaryEvidenceV1::new(
            entry
                .boundary_evidence()
                .representative_revision()
                .map(map_module_tree_revision_to_v1),
            entry
                .boundary_evidence()
                .manifest_revision()
                .map(map_module_tree_revision_to_v1),
        ),
        entry.manifest_count().to_string(),
        entry.file_count().to_string(),
        entry.symbol_count().to_string(),
        ModuleTreeFeatureCountV1::new(
            entry.central_symbol_count().to_string(),
            entry.central_symbols_truncated(),
        ),
        ModuleTreeFeatureCountV1::new(
            entry.entrypoint_count().to_string(),
            entry.entrypoints_truncated(),
        ),
        ModuleTreeFeatureCountV1::new(entry.test_count().to_string(), entry.tests_truncated()),
        match entry.child_state() {
            ModuleTreeChildState::Leaf => ModuleTreeChildStateV1::Leaf,
            ModuleTreeChildState::HasChildren => ModuleTreeChildStateV1::HasChildren,
        },
    )
}

fn map_module_tree_revision_to_v1(revision: &FileRevision) -> ModuleTreeRevisionV1 {
    ModuleTreeRevisionV1::new(
        encode_hex(revision.path().as_bytes()),
        encode_hex(revision.content_hash().as_bytes()),
    )
}

pub(crate) fn map_module_dependency_graph_query_from_v1(
    request: &QueryModuleDependencyGraphRequestV1,
) -> Result<ModuleDependencyGraphQuery, CommandErrorV1> {
    let center_module_id = decode_module_id(request.center_module_id())
        .map_err(|()| invalid_module_dependency_graph_query())?;
    let node_limit = ModuleDependencyNodeLimit::new(request.node_limit())
        .map_err(|_| invalid_module_dependency_graph_query())?;
    Ok(ModuleDependencyGraphQuery::new(
        center_module_id,
        node_limit,
    ))
}

fn map_module_dependency_graph_to_v1(graph: &ModuleDependencyGraph) -> ModuleDependencyGraphV1 {
    ModuleDependencyGraphV1::new(
        graph.index_run_id().to_string(),
        graph.snapshot_id().to_string(),
        graph.center_module_id().to_string(),
        graph
            .nodes()
            .iter()
            .map(map_module_dependency_node_to_v1)
            .collect(),
        graph.observed_neighbor_count().to_string(),
        graph.nodes_truncated(),
        graph
            .edges()
            .iter()
            .map(map_module_dependency_edge_to_v1)
            .collect(),
        graph.observed_edge_group_count().to_string(),
        graph.edges_truncated(),
        graph.inspected_edge_count().to_string(),
        graph.source_edges_truncated(),
        graph.unmapped_edge_count().to_string(),
    )
}

fn map_module_dependency_node_to_v1(node: &ModuleDependencyNode) -> ModuleDependencyNodeV1 {
    let evidence = node.representative_revision().and_then(|revision| {
        node.representative_evidence_id().map(|evidence_id| {
            ModuleDependencyNodeEvidenceV1::new(
                encode_hex(evidence_id.as_bytes()),
                encode_hex(revision.path().as_bytes()),
                encode_hex(revision.content_hash().as_bytes()),
            )
        })
    });
    ModuleDependencyNodeV1::new(
        node.module_id().to_string(),
        match node.kind() {
            ModuleTreeEntryKind::ManifestBoundary => ModuleTreeEntryKindV1::ManifestBoundary,
            ModuleTreeEntryKind::PathBoundary => ModuleTreeEntryKindV1::PathBoundary,
        },
        match node.root() {
            ModuleRoot::Repository => None,
            ModuleRoot::Directory(path) => Some(encode_hex(path.as_bytes())),
        },
        node.display_name().as_str().to_owned(),
        node.display_name().is_truncated(),
        evidence,
    )
}

fn map_module_dependency_edge_to_v1(edge: &ModuleDependencyEdge) -> ModuleDependencyEdgeV1 {
    let representative = edge.representative_edge();
    let evidence = representative.evidence();
    let range = evidence.range();
    let start = range.start_position();
    let end = range.end_position();
    ModuleDependencyEdgeV1::new(
        edge.source_module_id().to_string(),
        edge.target_module_id().to_string(),
        match edge.relation() {
            ModuleDependencyRelation::Imports => ModuleDependencyRelationV1::Imports,
            ModuleDependencyRelation::Exports => ModuleDependencyRelationV1::Exports,
            ModuleDependencyRelation::Calls => ModuleDependencyRelationV1::Calls,
            ModuleDependencyRelation::Implements => ModuleDependencyRelationV1::Implements,
            ModuleDependencyRelation::Extends => ModuleDependencyRelationV1::Extends,
            ModuleDependencyRelation::Reads => ModuleDependencyRelationV1::Reads,
            ModuleDependencyRelation::Writes => ModuleDependencyRelationV1::Writes,
            ModuleDependencyRelation::Configures => ModuleDependencyRelationV1::Configures,
            ModuleDependencyRelation::Tests => ModuleDependencyRelationV1::Tests,
            ModuleDependencyRelation::Builds => ModuleDependencyRelationV1::Builds,
            ModuleDependencyRelation::Documents => ModuleDependencyRelationV1::Documents,
        },
        edge.observed_evidence_count().to_string(),
        ModuleDependencyEdgeEvidenceV1::new(
            encode_hex(edge.evidence_id().as_bytes()),
            map_module_dependency_endpoint_to_v1(representative.source()),
            map_module_dependency_endpoint_to_v1(representative.target()),
            encode_hex(evidence.revision().path().as_bytes()),
            encode_hex(evidence.revision().content_hash().as_bytes()),
            ModuleDependencySourceRangeV1::new(
                range.start_byte(),
                range.end_byte(),
                ModuleDependencySourcePositionV1::new(start.row(), start.column()),
                ModuleDependencySourcePositionV1::new(end.row(), end.column()),
            ),
            match representative.provider() {
                SyntaxProvider::TreeSitter => ModuleDependencyProviderV1::TreeSitter,
                SyntaxProvider::Manifest => ModuleDependencyProviderV1::Manifest,
                SyntaxProvider::LanguageHeuristic => ModuleDependencyProviderV1::LanguageHeuristic,
            },
            representative.confidence().basis_points(),
            match representative.resolution() {
                LinkResolution::AdapterLocalSymbol => {
                    ModuleDependencyResolutionV1::AdapterLocalSymbol
                }
                LinkResolution::AdapterFile => ModuleDependencyResolutionV1::AdapterFile,
                LinkResolution::ExactModuleReference => {
                    ModuleDependencyResolutionV1::ExactModuleReference
                }
                LinkResolution::UniqueFileLocalName => {
                    ModuleDependencyResolutionV1::UniqueFileLocalName
                }
                LinkResolution::UniqueQualifiedName => {
                    ModuleDependencyResolutionV1::UniqueQualifiedName
                }
            },
        ),
    )
}

fn map_module_dependency_endpoint_to_v1(endpoint: &GraphEndpoint) -> ModuleDependencyEndpointV1 {
    match endpoint {
        GraphEndpoint::File(path) => ModuleDependencyEndpointV1::File {
            path_hex: encode_hex(path.as_bytes()),
        },
        GraphEndpoint::Symbol(id) => ModuleDependencyEndpointV1::Symbol {
            symbol_id: id.to_string(),
        },
    }
}

fn map_project_map_search_to_v1(
    query: &ProjectMapSearchQuery,
    result: &FusedRetrievalResult,
) -> Option<ProjectMapSearchV1> {
    let mut rank = 0_u16;
    let mut hits = Vec::with_capacity(result.hits().len());
    for hit in result.hits() {
        rank = rank.checked_add(1)?;
        let priority = match hit.explanation().priority() {
            FusionPriority::Exact => ProjectMapSearchPriorityV1::Exact,
            FusionPriority::Evidence => ProjectMapSearchPriorityV1::Evidence,
            FusionPriority::Semantic => return None,
        };
        let sources = hit
            .explanation()
            .sources()
            .iter()
            .map(map_project_map_search_source_to_v1)
            .collect::<Option<Vec<_>>>()?;
        hits.push(ProjectMapSearchHitV1::new(
            rank,
            priority,
            hit.explanation().final_score().get(),
            sources,
            map_project_map_search_target_to_v1(hit.target()),
        ));
    }
    Some(ProjectMapSearchV1::new(
        query.lexical().term().as_str().to_owned(),
        result.index_run_id().to_string(),
        result.snapshot_id().to_string(),
        result.policy_version().get(),
        hits,
        result.truncated(),
    ))
}

fn map_project_map_search_source_to_v1(
    source: &ResultSourceExplanation,
) -> Option<ProjectMapSearchSourceV1> {
    match source.reason() {
        RetrievalCandidateReason::Exact(explanation) => Some(ProjectMapSearchSourceV1::Exact {
            explanation: map_project_map_exact_explanation_to_v1(*explanation),
            normalized_score_basis_points: source.normalized_score().get(),
        }),
        RetrievalCandidateReason::Lexical { explanation, score } => {
            Some(ProjectMapSearchSourceV1::Lexical {
                explanation: map_project_map_lexical_explanation_to_v1(*explanation),
                native_score: score.get(),
                normalized_score_basis_points: source.normalized_score().get(),
            })
        }
        RetrievalCandidateReason::Graph(_)
        | RetrievalCandidateReason::Test(_)
        | RetrievalCandidateReason::Memory(_)
        | RetrievalCandidateReason::Semantic(_) => None,
    }
}

fn map_project_map_search_target_to_v1(target: &ExactSearchTarget) -> ProjectMapSearchTargetV1 {
    match target {
        ExactSearchTarget::File(revision) => ProjectMapSearchTargetV1::File {
            evidence: map_project_map_search_evidence_to_v1(revision, None),
        },
        ExactSearchTarget::Symbol(symbol) => {
            let parsed = symbol.symbol().parsed();
            ProjectMapSearchTargetV1::Symbol {
                symbol_id: symbol.symbol().id().to_string(),
                symbol_kind: map_project_map_search_symbol_kind_to_v1(parsed.kind()),
                name: parsed.name().as_str().to_owned(),
                qualified_name: symbol.qualified_name().as_str().to_owned(),
                signature: parsed.signature().map(|value| value.as_str().to_owned()),
                evidence: map_project_map_search_evidence_to_v1(
                    symbol.symbol().revision(),
                    Some(parsed.declaration_range()),
                ),
            }
        }
    }
}

fn map_project_map_search_evidence_to_v1(
    revision: &FileRevision,
    declaration_range: Option<a3_domain::SourceRange>,
) -> ProjectMapSearchEvidenceV1 {
    ProjectMapSearchEvidenceV1::new(
        repository_path_display(revision.path()),
        encode_hex(revision.path().as_bytes()),
        encode_hex(revision.content_hash().as_bytes()),
        declaration_range.map(|range| {
            let start = range.start_position();
            let end = range.end_position();
            ModuleDependencySourceRangeV1::new(
                range.start_byte(),
                range.end_byte(),
                ModuleDependencySourcePositionV1::new(start.row(), start.column()),
                ModuleDependencySourcePositionV1::new(end.row(), end.column()),
            )
        }),
    )
}

fn repository_path_display(path: &RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

const fn map_project_map_search_channel_to_v1(
    channel: a3_domain::SourceChannel,
) -> Option<ProjectMapSearchChannelV1> {
    match channel {
        a3_domain::SourceChannel::Exact => Some(ProjectMapSearchChannelV1::Exact),
        a3_domain::SourceChannel::Lexical => Some(ProjectMapSearchChannelV1::Lexical),
        a3_domain::SourceChannel::Graph
        | a3_domain::SourceChannel::Test
        | a3_domain::SourceChannel::Memory
        | a3_domain::SourceChannel::Semantic => None,
    }
}

const fn map_project_map_exact_explanation_to_v1(
    explanation: ExactSearchExplanation,
) -> ProjectMapExactExplanationV1 {
    match explanation {
        ExactSearchExplanation::NormalizedPathExact => {
            ProjectMapExactExplanationV1::NormalizedPathExact
        }
        ExactSearchExplanation::QualifiedNameExact => {
            ProjectMapExactExplanationV1::QualifiedNameExact
        }
        ExactSearchExplanation::SymbolNameExact => ProjectMapExactExplanationV1::SymbolNameExact,
        ExactSearchExplanation::SignatureExact => ProjectMapExactExplanationV1::SignatureExact,
        ExactSearchExplanation::QualifiedNamePrefix => {
            ProjectMapExactExplanationV1::QualifiedNamePrefix
        }
        ExactSearchExplanation::SymbolNamePrefix => ProjectMapExactExplanationV1::SymbolNamePrefix,
        ExactSearchExplanation::SignaturePrefix => ProjectMapExactExplanationV1::SignaturePrefix,
        ExactSearchExplanation::ManifestRole => ProjectMapExactExplanationV1::ManifestRole,
        ExactSearchExplanation::EntrypointRole => ProjectMapExactExplanationV1::EntrypointRole,
        ExactSearchExplanation::TestRole => ProjectMapExactExplanationV1::TestRole,
    }
}

const fn map_project_map_lexical_explanation_to_v1(
    explanation: LexicalSearchExplanation,
) -> ProjectMapLexicalExplanationV1 {
    match explanation {
        LexicalSearchExplanation::Path => ProjectMapLexicalExplanationV1::Path,
        LexicalSearchExplanation::QualifiedName => ProjectMapLexicalExplanationV1::QualifiedName,
        LexicalSearchExplanation::SymbolName => ProjectMapLexicalExplanationV1::SymbolName,
        LexicalSearchExplanation::Signature => ProjectMapLexicalExplanationV1::Signature,
    }
}

const fn map_project_map_search_symbol_kind_to_v1(
    kind: SymbolKind,
) -> ProjectMapSearchSymbolKindV1 {
    match kind {
        SymbolKind::Module => ProjectMapSearchSymbolKindV1::Module,
        SymbolKind::Namespace => ProjectMapSearchSymbolKindV1::Namespace,
        SymbolKind::Function => ProjectMapSearchSymbolKindV1::Function,
        SymbolKind::Method => ProjectMapSearchSymbolKindV1::Method,
        SymbolKind::Struct => ProjectMapSearchSymbolKindV1::Struct,
        SymbolKind::Enum => ProjectMapSearchSymbolKindV1::Enum,
        SymbolKind::Trait => ProjectMapSearchSymbolKindV1::Trait,
        SymbolKind::Interface => ProjectMapSearchSymbolKindV1::Interface,
        SymbolKind::Class => ProjectMapSearchSymbolKindV1::Class,
        SymbolKind::Implementation => ProjectMapSearchSymbolKindV1::Implementation,
        SymbolKind::TypeAlias => ProjectMapSearchSymbolKindV1::TypeAlias,
        SymbolKind::Constant => ProjectMapSearchSymbolKindV1::Constant,
        SymbolKind::Static => ProjectMapSearchSymbolKindV1::Static,
        SymbolKind::Variable => ProjectMapSearchSymbolKindV1::Variable,
        SymbolKind::Field => ProjectMapSearchSymbolKindV1::Field,
        SymbolKind::Variant => ProjectMapSearchSymbolKindV1::Variant,
        SymbolKind::Parameter => ProjectMapSearchSymbolKindV1::Parameter,
    }
}

fn map_task_lens_summary_to_v1(goal: &a3_domain::GoalContract) -> TaskLensTaskSummaryV1 {
    TaskLensTaskSummaryV1::new(
        goal.task_id().to_string(),
        goal.revision().get(),
        goal.draft().objective().as_str().to_owned(),
    )
}

const fn map_task_lens_step_status_to_v1(status: TaskStepStatus) -> TaskLensStepStatusV1 {
    match status {
        TaskStepStatus::Pending => TaskLensStepStatusV1::Pending,
        TaskStepStatus::Ready => TaskLensStepStatusV1::Ready,
        TaskStepStatus::InProgress => TaskLensStepStatusV1::InProgress,
        TaskStepStatus::Blocked => TaskLensStepStatusV1::Blocked,
        TaskStepStatus::AwaitingApproval => TaskLensStepStatusV1::AwaitingApproval,
        TaskStepStatus::Verifying => TaskLensStepStatusV1::Verifying,
        TaskStepStatus::Completed => TaskLensStepStatusV1::Completed,
        TaskStepStatus::Failed => TaskLensStepStatusV1::Failed,
        TaskStepStatus::Cancelled => TaskLensStepStatusV1::Cancelled,
        TaskStepStatus::Stale => TaskLensStepStatusV1::Stale,
    }
}

fn map_task_lens_to_v1(compilation: &TaskLensCompilation) -> Option<TaskLensV1> {
    let lens = compilation.lens();
    let mut position = 0_u16;
    let mut boundary_truncated = false;
    let mut entries = Vec::with_capacity(lens.entries().len());
    for entry in lens.entries() {
        position = position.checked_add(1)?;
        let (target, target_truncated) = map_task_lens_target_to_v1(entry.target())?;
        boundary_truncated |= target_truncated;
        entries.push(TaskLensEntryV1::new(
            position,
            entry.estimated_tokens().get(),
            map_task_lens_entry_reason_to_v1(entry.reason())?,
            target,
        ));
    }
    let claims = lens
        .claims()
        .iter()
        .map(map_task_lens_claim_to_v1)
        .collect::<Option<Vec<_>>>()?;
    Some(TaskLensV1::new(
        compilation.goal_contract().task_id().to_string(),
        compilation.goal_contract().revision().get(),
        compilation.ledger_revision().get(),
        compilation.ledger_store_version().get().to_string(),
        compilation.step_id().to_string(),
        lens.index_run_id().to_string(),
        lens.snapshot_id().to_string(),
        lens.policy_version().get(),
        lens.fusion_policy_version().get(),
        lens.token_budget().get(),
        lens.estimated_tokens(),
        lens.seeds().goal().as_str().to_owned(),
        lens.seeds().step().as_str().to_owned(),
        encode_hex(&lens.digest().as_bytes()),
        lens.excluded_stale_claims(),
        entries,
        claims,
        lens.truncated() || boundary_truncated,
    ))
}

fn map_task_lens_entry_reason_to_v1(reason: &TaskLensEntryReason) -> Option<TaskLensEntryReasonV1> {
    match reason {
        TaskLensEntryReason::RepositoryAnchor => Some(TaskLensEntryReasonV1::RepositoryAnchor),
        TaskLensEntryReason::Retrieval { rank, explanation } => {
            let sources = explanation
                .sources()
                .iter()
                .map(|source| {
                    Some(TaskLensRetrievalSourceV1::new(
                        map_task_lens_channel_to_v1(source.reason().source_channel()),
                        source.normalized_score().get(),
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(TaskLensEntryReasonV1::Retrieval {
                rank: *rank,
                priority: match explanation.priority() {
                    FusionPriority::Exact => TaskLensPriorityV1::Exact,
                    FusionPriority::Evidence => TaskLensPriorityV1::Evidence,
                    FusionPriority::Semantic => TaskLensPriorityV1::Semantic,
                },
                final_score: explanation.final_score().get(),
                sources,
            })
        }
        TaskLensEntryReason::Claim(claim_id) => Some(TaskLensEntryReasonV1::Claim {
            claim_id: encode_hex(claim_id.as_bytes()),
        }),
    }
}

const fn map_task_lens_channel_to_v1(channel: SourceChannel) -> TaskLensRetrievalChannelV1 {
    match channel {
        SourceChannel::Exact => TaskLensRetrievalChannelV1::Exact,
        SourceChannel::Lexical => TaskLensRetrievalChannelV1::Lexical,
        SourceChannel::Graph => TaskLensRetrievalChannelV1::Graph,
        SourceChannel::Test => TaskLensRetrievalChannelV1::Test,
        SourceChannel::Memory => TaskLensRetrievalChannelV1::Memory,
        SourceChannel::Semantic => TaskLensRetrievalChannelV1::Semantic,
    }
}

fn map_task_lens_target_to_v1(target: &TaskLensTarget) -> Option<(TaskLensEntryTargetV1, bool)> {
    const MAX_VISIBLE_MANIFESTS: usize = 16;
    match target {
        TaskLensTarget::Repository(card) => Some((
            TaskLensEntryTargetV1::Repository {
                module_policy_version: card.policy_version().get(),
                package_count: u32::try_from(card.packages().len()).ok()?,
                language_count: u32::try_from(card.languages().len()).ok()?,
                entrypoint_count: u32::try_from(card.entrypoints().symbols().len()).ok()?,
                file_count: card.file_count(),
                symbol_count: card.symbol_count(),
            },
            false,
        )),
        TaskLensTarget::Module(module) => {
            let manifests_truncated = module.manifests().len() > MAX_VISIBLE_MANIFESTS;
            let manifests = module
                .manifests()
                .iter()
                .take(MAX_VISIBLE_MANIFESTS)
                .map(|revision| map_project_map_search_evidence_to_v1(revision, None))
                .collect();
            Some((
                TaskLensEntryTargetV1::Module {
                    module_id: module.id().to_string(),
                    module_kind: match module.kind() {
                        ModuleKind::ManifestBoundary => TaskLensModuleKindV1::ManifestBoundary,
                        ModuleKind::PathBoundary => TaskLensModuleKindV1::PathBoundary,
                        ModuleKind::GraphCommunity => TaskLensModuleKindV1::GraphCommunity,
                    },
                    root: module.root().and_then(|root| match root {
                        ModuleRoot::Repository => None,
                        ModuleRoot::Directory(path) => Some(map_task_lens_path_to_v1(path)),
                    }),
                    manifests,
                    manifests_truncated,
                },
                manifests_truncated,
            ))
        }
        TaskLensTarget::File(revision) => Some((
            TaskLensEntryTargetV1::File {
                evidence: map_project_map_search_evidence_to_v1(revision, None),
            },
            false,
        )),
        TaskLensTarget::Symbol(symbol) => {
            let parsed = symbol.parsed();
            Some((
                TaskLensEntryTargetV1::Symbol {
                    symbol_id: symbol.id().to_string(),
                    symbol_kind: map_project_map_search_symbol_kind_to_v1(parsed.kind()),
                    name: parsed.name().as_str().to_owned(),
                    signature: parsed.signature().map(|value| value.as_str().to_owned()),
                    evidence: map_project_map_search_evidence_to_v1(
                        symbol.revision(),
                        Some(parsed.declaration_range()),
                    ),
                },
                false,
            ))
        }
        TaskLensTarget::SourceSpan {
            symbol_id,
            evidence,
        } => Some((
            TaskLensEntryTargetV1::SourceSpan {
                symbol_id: symbol_id.to_string(),
                evidence: map_project_map_search_evidence_to_v1(
                    evidence.revision(),
                    Some(evidence.range()),
                ),
            },
            false,
        )),
    }
}

fn map_task_lens_path_to_v1(path: &RepositoryPath) -> TaskLensPathV1 {
    TaskLensPathV1::new(repository_path_display(path), encode_hex(path.as_bytes()))
}

fn map_task_lens_claim_to_v1(claim: &a3_domain::TaskLensClaim) -> Option<TaskLensClaimV1> {
    if claim.status() != VerifiedClaimStatus::Active {
        return None;
    }
    let evidence = claim
        .evidence()
        .iter()
        .map(map_task_lens_claim_evidence_to_v1)
        .collect::<Option<Vec<_>>>()?;
    Some(TaskLensClaimV1::new(
        encode_hex(claim.id().as_bytes()),
        claim.module_id().to_string(),
        match claim.kind() {
            VerifiedClaimKind::Fact => TaskLensClaimKindV1::Fact,
            VerifiedClaimKind::Observation => TaskLensClaimKindV1::Observation,
            VerifiedClaimKind::Hypothesis => TaskLensClaimKindV1::Hypothesis,
        },
        match claim.polarity() {
            ModuleClaimPolarity::Affirms => TaskLensClaimPolarityV1::Affirms,
            ModuleClaimPolarity::Denies => TaskLensClaimPolarityV1::Denies,
        },
        claim.confidence().basis_points(),
        map_task_lens_claim_predicate_to_v1(claim.predicate())?,
        evidence,
    ))
}

fn map_task_lens_claim_predicate_to_v1(
    predicate: &ModuleClaimPredicate,
) -> Option<TaskLensClaimPredicateV1> {
    match predicate {
        ModuleClaimPredicate::Path(path) => Some(TaskLensClaimPredicateV1::Path {
            path: map_task_lens_path_to_v1(path),
        }),
        ModuleClaimPredicate::Symbol(symbol_id) => Some(TaskLensClaimPredicateV1::Symbol {
            symbol_id: symbol_id.to_string(),
        }),
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => Some(TaskLensClaimPredicateV1::Relation {
            source: map_module_dependency_endpoint_to_v1(source),
            target: map_module_dependency_endpoint_to_v1(target),
            relation: map_syntax_relation_to_v1(*kind)?,
        }),
        ModuleClaimPredicate::Observed(statement) => Some(TaskLensClaimPredicateV1::Observed {
            statement: statement.as_str().to_owned(),
        }),
        ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            Some(TaskLensClaimPredicateV1::ArchitecturalIntent {
                statement: statement.as_str().to_owned(),
            })
        }
    }
}

fn map_task_lens_claim_evidence_to_v1(
    evidence: &ResolvedModuleCardEvidence,
) -> Option<TaskLensClaimEvidenceV1> {
    match evidence {
        ResolvedModuleCardEvidence::File { id, revision } => Some(TaskLensClaimEvidenceV1::File {
            evidence_id: encode_hex(id.as_bytes()),
            revision: map_project_map_search_evidence_to_v1(revision, None),
        }),
        ResolvedModuleCardEvidence::Symbol { id, symbol } => {
            let parsed = symbol.parsed();
            Some(TaskLensClaimEvidenceV1::Symbol {
                evidence_id: encode_hex(id.as_bytes()),
                symbol_id: symbol.id().to_string(),
                symbol_kind: map_project_map_search_symbol_kind_to_v1(parsed.kind()),
                name: parsed.name().as_str().to_owned(),
                signature: parsed.signature().map(|value| value.as_str().to_owned()),
                revision: map_project_map_search_evidence_to_v1(
                    symbol.revision(),
                    Some(parsed.declaration_range()),
                ),
            })
        }
        ResolvedModuleCardEvidence::GraphEdge { id, edge } => {
            Some(TaskLensClaimEvidenceV1::GraphEdge {
                relation: map_syntax_relation_to_v1(edge.kind())?,
                edge: map_task_lens_graph_edge_evidence_to_v1(*id, edge),
            })
        }
    }
}

fn map_task_lens_graph_edge_evidence_to_v1(
    evidence_id: ModuleCardEvidenceId,
    edge: &GraphEdge,
) -> ModuleDependencyEdgeEvidenceV1 {
    let evidence = edge.evidence();
    let range = evidence.range();
    let start = range.start_position();
    let end = range.end_position();
    ModuleDependencyEdgeEvidenceV1::new(
        encode_hex(evidence_id.as_bytes()),
        map_module_dependency_endpoint_to_v1(edge.source()),
        map_module_dependency_endpoint_to_v1(edge.target()),
        encode_hex(evidence.revision().path().as_bytes()),
        encode_hex(evidence.revision().content_hash().as_bytes()),
        ModuleDependencySourceRangeV1::new(
            range.start_byte(),
            range.end_byte(),
            ModuleDependencySourcePositionV1::new(start.row(), start.column()),
            ModuleDependencySourcePositionV1::new(end.row(), end.column()),
        ),
        match edge.provider() {
            SyntaxProvider::TreeSitter => ModuleDependencyProviderV1::TreeSitter,
            SyntaxProvider::Manifest => ModuleDependencyProviderV1::Manifest,
            SyntaxProvider::LanguageHeuristic => ModuleDependencyProviderV1::LanguageHeuristic,
        },
        edge.confidence().basis_points(),
        match edge.resolution() {
            LinkResolution::AdapterLocalSymbol => ModuleDependencyResolutionV1::AdapterLocalSymbol,
            LinkResolution::AdapterFile => ModuleDependencyResolutionV1::AdapterFile,
            LinkResolution::ExactModuleReference => {
                ModuleDependencyResolutionV1::ExactModuleReference
            }
            LinkResolution::UniqueFileLocalName => {
                ModuleDependencyResolutionV1::UniqueFileLocalName
            }
            LinkResolution::UniqueQualifiedName => {
                ModuleDependencyResolutionV1::UniqueQualifiedName
            }
        },
    )
}

const fn map_syntax_relation_to_v1(
    relation: SyntaxRelationKind,
) -> Option<ModuleDependencyRelationV1> {
    match relation {
        SyntaxRelationKind::Imports => Some(ModuleDependencyRelationV1::Imports),
        SyntaxRelationKind::Exports => Some(ModuleDependencyRelationV1::Exports),
        SyntaxRelationKind::Calls => Some(ModuleDependencyRelationV1::Calls),
        SyntaxRelationKind::Implements => Some(ModuleDependencyRelationV1::Implements),
        SyntaxRelationKind::Extends => Some(ModuleDependencyRelationV1::Extends),
        SyntaxRelationKind::Reads => Some(ModuleDependencyRelationV1::Reads),
        SyntaxRelationKind::Writes => Some(ModuleDependencyRelationV1::Writes),
        SyntaxRelationKind::Configures => Some(ModuleDependencyRelationV1::Configures),
        SyntaxRelationKind::Tests => Some(ModuleDependencyRelationV1::Tests),
        SyntaxRelationKind::Builds => Some(ModuleDependencyRelationV1::Builds),
        SyntaxRelationKind::Documents => Some(ModuleDependencyRelationV1::Documents),
        SyntaxRelationKind::Contains | SyntaxRelationKind::Defines => None,
    }
}

pub(crate) fn map_module_runtime_map_query_from_v1(
    request: &QueryModuleRuntimeMapRequestV1,
) -> Result<ModuleRuntimeMapQuery, CommandErrorV1> {
    let module_id =
        decode_module_id(request.module_id()).map_err(|()| invalid_module_runtime_map_query())?;
    let entrypoint_limit = ModuleRuntimeRootLimit::new(request.entrypoint_limit())
        .map_err(|_| invalid_module_runtime_map_query())?;
    let test_limit = ModuleRuntimeRootLimit::new(request.test_limit())
        .map_err(|_| invalid_module_runtime_map_query())?;
    Ok(ModuleRuntimeMapQuery::new(
        module_id,
        entrypoint_limit,
        test_limit,
    ))
}

pub(crate) fn map_module_runtime_flow_query_from_v1(
    request: &QueryModuleRuntimeFlowRequestV1,
) -> Result<ModuleRuntimeFlowQuery, CommandErrorV1> {
    let expected_index_run_id = decode_index_run_id(request.expected_index_run_id())
        .map_err(|()| invalid_module_runtime_flow_query())?;
    let expected_snapshot_id = decode_snapshot_id(request.expected_snapshot_id())
        .map_err(|()| invalid_module_runtime_flow_query())?;
    let module_id =
        decode_module_id(request.module_id()).map_err(|()| invalid_module_runtime_flow_query())?;
    let root_symbol_id = decode_symbol_id(request.root_symbol_id())
        .map_err(|()| invalid_module_runtime_flow_query())?;
    let kind = match request.kind() {
        ModuleRuntimeFlowKindV1::EntrypointCalls => ModuleRuntimeFlowKind::EntrypointCalls,
        ModuleRuntimeFlowKindV1::TestTargets => ModuleRuntimeFlowKind::TestTargets,
    };
    let result_limit = TraversalResultLimit::new(request.result_limit())
        .map_err(|_| invalid_module_runtime_flow_query())?;
    Ok(ModuleRuntimeFlowQuery::new(
        expected_index_run_id,
        expected_snapshot_id,
        module_id,
        root_symbol_id,
        kind,
        result_limit,
    ))
}

pub(crate) fn map_project_map_search_query_from_v1(
    request: &QueryProjectMapSearchRequestV1,
) -> Result<ProjectMapSearchQuery, CommandErrorV1> {
    ProjectMapSearchQuery::try_from_string(request.query().to_owned())
        .map_err(|_| CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectMapSearchQuery))
}

pub(crate) fn map_task_lens_task_id_from_v1(
    request: &QueryTaskLensTaskRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidTaskLensSelection))
}

pub(crate) fn map_task_lens_selection_from_v1(
    request: &CompileTaskLensRequestV1,
) -> Result<(TaskId, TaskStepId), CommandErrorV1> {
    let task_id = decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidTaskLensSelection))?;
    let step_id = decode_stable_id(request.step_id())
        .map(TaskStepId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidTaskLensSelection))?;
    Ok((task_id, step_id))
}

fn map_module_runtime_map_to_v1(map: &ModuleRuntimeMap) -> ModuleRuntimeMapV1 {
    ModuleRuntimeMapV1::new(
        map.index_run_id().to_string(),
        map.snapshot_id().to_string(),
        map.module_id().to_string(),
        map_module_runtime_root_set_to_v1(map.entrypoints()),
        map_module_runtime_root_set_to_v1(map.tests()),
    )
}

fn map_module_runtime_root_set_to_v1(set: &ModuleRuntimeRootSet) -> ModuleRuntimeRootSetV1 {
    ModuleRuntimeRootSetV1::new(
        set.roots()
            .iter()
            .map(map_module_runtime_root_to_v1)
            .collect(),
        set.stored_count().to_string(),
        set.projection_truncated(),
        set.visible_truncated(),
    )
}

fn map_module_runtime_root_to_v1(root: &ModuleRuntimeRoot) -> ModuleRuntimeRootV1 {
    ModuleRuntimeRootV1::new(
        match root.kind() {
            ModuleRuntimeRootKind::Entrypoint => ModuleRuntimeRootKindV1::Entrypoint,
            ModuleRuntimeRootKind::Test => ModuleRuntimeRootKindV1::Test,
        },
        root.rank(),
        map_module_runtime_symbol_to_v1(root.symbol()),
    )
}

fn map_module_runtime_symbol_to_v1(symbol: &GraphSymbol) -> ModuleRuntimeSymbolV1 {
    let range = symbol.parsed().selection_range();
    let start = range.start_position();
    let end = range.end_position();
    ModuleRuntimeSymbolV1::new(
        symbol.id().to_string(),
        map_module_runtime_symbol_kind_to_v1(symbol.parsed().kind()),
        symbol.parsed().name().as_str().to_owned(),
        encode_hex(ModuleCardEvidenceId::for_symbol_v1(symbol).as_bytes()),
        encode_hex(symbol.revision().path().as_bytes()),
        encode_hex(symbol.revision().content_hash().as_bytes()),
        ModuleDependencySourceRangeV1::new(
            range.start_byte(),
            range.end_byte(),
            ModuleDependencySourcePositionV1::new(start.row(), start.column()),
            ModuleDependencySourcePositionV1::new(end.row(), end.column()),
        ),
    )
}

const fn map_module_runtime_symbol_kind_to_v1(kind: SymbolKind) -> ModuleRuntimeSymbolKindV1 {
    match kind {
        SymbolKind::Module => ModuleRuntimeSymbolKindV1::Module,
        SymbolKind::Namespace => ModuleRuntimeSymbolKindV1::Namespace,
        SymbolKind::Function => ModuleRuntimeSymbolKindV1::Function,
        SymbolKind::Method => ModuleRuntimeSymbolKindV1::Method,
        SymbolKind::Struct => ModuleRuntimeSymbolKindV1::Struct,
        SymbolKind::Enum => ModuleRuntimeSymbolKindV1::Enum,
        SymbolKind::Trait => ModuleRuntimeSymbolKindV1::Trait,
        SymbolKind::Interface => ModuleRuntimeSymbolKindV1::Interface,
        SymbolKind::Class => ModuleRuntimeSymbolKindV1::Class,
        SymbolKind::Implementation => ModuleRuntimeSymbolKindV1::Implementation,
        SymbolKind::TypeAlias => ModuleRuntimeSymbolKindV1::TypeAlias,
        SymbolKind::Constant => ModuleRuntimeSymbolKindV1::Constant,
        SymbolKind::Static => ModuleRuntimeSymbolKindV1::Static,
        SymbolKind::Variable => ModuleRuntimeSymbolKindV1::Variable,
        SymbolKind::Field => ModuleRuntimeSymbolKindV1::Field,
        SymbolKind::Variant => ModuleRuntimeSymbolKindV1::Variant,
        SymbolKind::Parameter => ModuleRuntimeSymbolKindV1::Parameter,
    }
}

fn map_module_runtime_flow_to_v1(
    query: &ModuleRuntimeFlowQuery,
    flow: &GraphTraversalResult,
) -> ModuleRuntimeFlowV1 {
    let kind = match query.kind() {
        ModuleRuntimeFlowKind::EntrypointCalls => ModuleRuntimeFlowKindV1::EntrypointCalls,
        ModuleRuntimeFlowKind::TestTargets => ModuleRuntimeFlowKindV1::TestTargets,
    };
    let relation = match query.kind() {
        ModuleRuntimeFlowKind::EntrypointCalls => ModuleRuntimeFlowRelationV1::Calls,
        ModuleRuntimeFlowKind::TestTargets => ModuleRuntimeFlowRelationV1::Tests,
    };
    ModuleRuntimeFlowV1::new(
        flow.index_run_id().to_string(),
        flow.snapshot_id().to_string(),
        query.module_id().to_string(),
        query.root_symbol_id().to_string(),
        kind,
        flow.hits()
            .iter()
            .map(|hit| {
                let target = match hit.target() {
                    ExactSearchTarget::File(revision) => ModuleRuntimeFlowTargetV1::File {
                        evidence_id: encode_hex(
                            ModuleCardEvidenceId::for_file_revision_v1(revision).as_bytes(),
                        ),
                        path_hex: encode_hex(revision.path().as_bytes()),
                        content_hash: encode_hex(revision.content_hash().as_bytes()),
                    },
                    ExactSearchTarget::Symbol(symbol) => ModuleRuntimeFlowTargetV1::Symbol {
                        symbol: map_module_runtime_symbol_to_v1(symbol.symbol()),
                    },
                };
                ModuleRuntimeFlowHitV1::new(
                    target,
                    hit.path()
                        .iter()
                        .map(|edge| {
                            ModuleRuntimeFlowEdgeV1::new(
                                relation,
                                map_graph_edge_evidence_to_v1(edge),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
        flow.truncated(),
    )
}

fn map_graph_edge_evidence_to_v1(edge: &GraphEdge) -> ModuleDependencyEdgeEvidenceV1 {
    let evidence = edge.evidence();
    let range = evidence.range();
    let start = range.start_position();
    let end = range.end_position();
    ModuleDependencyEdgeEvidenceV1::new(
        encode_hex(ModuleCardEvidenceId::for_graph_edge_v1(edge).as_bytes()),
        map_module_dependency_endpoint_to_v1(edge.source()),
        map_module_dependency_endpoint_to_v1(edge.target()),
        encode_hex(evidence.revision().path().as_bytes()),
        encode_hex(evidence.revision().content_hash().as_bytes()),
        ModuleDependencySourceRangeV1::new(
            range.start_byte(),
            range.end_byte(),
            ModuleDependencySourcePositionV1::new(start.row(), start.column()),
            ModuleDependencySourcePositionV1::new(end.row(), end.column()),
        ),
        match edge.provider() {
            SyntaxProvider::TreeSitter => ModuleDependencyProviderV1::TreeSitter,
            SyntaxProvider::Manifest => ModuleDependencyProviderV1::Manifest,
            SyntaxProvider::LanguageHeuristic => ModuleDependencyProviderV1::LanguageHeuristic,
        },
        edge.confidence().basis_points(),
        match edge.resolution() {
            LinkResolution::AdapterLocalSymbol => ModuleDependencyResolutionV1::AdapterLocalSymbol,
            LinkResolution::AdapterFile => ModuleDependencyResolutionV1::AdapterFile,
            LinkResolution::ExactModuleReference => {
                ModuleDependencyResolutionV1::ExactModuleReference
            }
            LinkResolution::UniqueFileLocalName => {
                ModuleDependencyResolutionV1::UniqueFileLocalName
            }
            LinkResolution::UniqueQualifiedName => {
                ModuleDependencyResolutionV1::UniqueQualifiedName
            }
        },
    )
}

fn decode_module_id(value: &str) -> Result<ModuleId, ()> {
    let bytes = decode_hex(value, 32)?;
    let bytes = <[u8; 32]>::try_from(bytes).map_err(|_| ())?;
    Ok(ModuleId::from_bytes(bytes))
}

fn decode_index_run_id(value: &str) -> Result<IndexRunId, ()> {
    decode_stable_id(value).map(IndexRunId::from_bytes)
}

fn decode_snapshot_id(value: &str) -> Result<SnapshotId, ()> {
    decode_stable_id(value).map(SnapshotId::from_bytes)
}

fn decode_symbol_id(value: &str) -> Result<SymbolId, ()> {
    decode_stable_id(value).map(SymbolId::from_bytes)
}

fn decode_stable_id(value: &str) -> Result<[u8; 32], ()> {
    decode_hex(value, 32)?.try_into().map_err(|_| ())
}

pub(crate) fn map_repository_tree_query_from_v1(
    request: &QueryRepositoryTreeRequestV1,
) -> Result<RepositoryTreeQuery, CommandErrorV1> {
    let directory = request
        .directory_path_hex()
        .map(|value| decode_hex(value, 131_072))
        .transpose()
        .and_then(|bytes| {
            bytes
                .map(RepositoryPath::try_from_bytes)
                .transpose()
                .map_err(|_| ())
        })
        .map_err(|()| invalid_repository_tree_query())?;
    let after = request
        .after_name_hex()
        .map(|value| decode_hex(value, 4_096))
        .transpose()
        .and_then(|bytes| {
            bytes
                .map(RepositoryTreeChildName::try_from_bytes)
                .transpose()
                .map_err(|_| ())
        })
        .map_err(|()| invalid_repository_tree_query())?;
    let page_size = RepositoryTreePageSize::new(request.limit())
        .map_err(|_| invalid_repository_tree_query())?;
    Ok(RepositoryTreeQuery::new(directory, after, page_size))
}

fn map_repository_tree_page_to_v1(page: &RepositoryTreePage) -> RepositoryTreePageV1 {
    RepositoryTreePageV1::new(
        page.index_run_id().to_string(),
        page.snapshot_id().to_string(),
        page.directory().map(|path| encode_hex(path.as_bytes())),
        page.entries()
            .iter()
            .map(|entry| {
                RepositoryTreeEntryV1::new(
                    match entry.kind() {
                        RepositoryTreeEntryKind::Directory => RepositoryTreeEntryKindV1::Directory,
                        RepositoryTreeEntryKind::File => RepositoryTreeEntryKindV1::File,
                    },
                    encode_hex(entry.path().as_bytes()),
                    entry.display_name().as_str().to_owned(),
                    entry.display_name().is_truncated(),
                    entry.descendant_file_count().to_string(),
                    entry.content_hash().map(|hash| encode_hex(hash.as_bytes())),
                )
            })
            .collect(),
        page.next_cursor()
            .map(|cursor| encode_hex(cursor.as_bytes())),
    )
}

fn decode_hex(value: &str, max_bytes: usize) -> Result<Vec<u8>, ()> {
    if value.is_empty()
        || !value.len().is_multiple_of(2)
        || value.len() > max_bytes.checked_mul(2).ok_or(())?
    {
        return Err(());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = decode_hex_nibble(pair[0]).ok_or(())?;
            let low = decode_hex_nibble(pair[1]).ok_or(())?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn invalid_repository_tree_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidRepositoryTreeQuery)
}

fn invalid_module_tree_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleTreeQuery)
}

fn invalid_module_dependency_graph_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleDependencyGraphQuery)
}

fn invalid_module_runtime_map_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleRuntimeMapQuery)
}

fn invalid_module_runtime_flow_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleRuntimeFlowQuery)
}

fn invalid_module_card_detail_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleCardDetailQuery)
}

fn invalid_module_card_evidence_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidModuleCardEvidenceQuery)
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

fn map_module_card_freshness_error_to_v1(error: ModuleCardFreshnessFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleCardFreshnessFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleCardFreshnessFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ModuleCardFreshnessFailure::Cancelled
        | ModuleCardFreshnessFailure::TimedOut
        | ModuleCardFreshnessFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_module_card_detail_error_to_v1(error: ModuleCardDetailFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleCardDetailFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleCardDetailFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ModuleCardDetailFailure::Cancelled
        | ModuleCardDetailFailure::TimedOut
        | ModuleCardDetailFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_module_card_evidence_error_to_v1(error: ModuleCardEvidenceFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleCardEvidenceFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleCardEvidenceFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ModuleCardEvidenceFailure::Cancelled
        | ModuleCardEvidenceFailure::TimedOut
        | ModuleCardEvidenceFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_module_tree_error_to_v1(error: ModuleTreeFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleTreeFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleTreeFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ModuleTreeFailure::ParentUnavailable => ErrorCodeV1::ModuleTreeParentUnavailable,
        ModuleTreeFailure::Cancelled
        | ModuleTreeFailure::TimedOut
        | ModuleTreeFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_module_dependency_graph_error_to_v1(error: ModuleDependencyGraphFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleDependencyGraphFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleDependencyGraphFailure::InvalidStoredProjection => {
            ErrorCodeV1::LocalStorageInvalidData
        }
        ModuleDependencyGraphFailure::Cancelled
        | ModuleDependencyGraphFailure::TimedOut
        | ModuleDependencyGraphFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_module_runtime_error_to_v1(error: ModuleRuntimeFailure) -> CommandErrorV1 {
    let code = match error {
        ModuleRuntimeFailure::Storage(error) => map_storage_error_to_v1(error),
        ModuleRuntimeFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ModuleRuntimeFailure::Cancelled
        | ModuleRuntimeFailure::TimedOut
        | ModuleRuntimeFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_project_map_search_error_to_v1(error: SearchProjectMapFailure) -> CommandErrorV1 {
    let code = match error {
        SearchProjectMapFailure::Search(a3_application::KnowledgeSearchFailure::Storage(error)) => {
            map_storage_error_to_v1(error)
        }
        SearchProjectMapFailure::Search(
            a3_application::KnowledgeSearchFailure::Cancelled
            | a3_application::KnowledgeSearchFailure::TimedOut,
        )
        | SearchProjectMapFailure::Cancelled => ErrorCodeV1::LocalStorageUnavailable,
        SearchProjectMapFailure::Search(
            a3_application::KnowledgeSearchFailure::IndexUnavailable
            | a3_application::KnowledgeSearchFailure::ProjectionUnavailable(_)
            | a3_application::KnowledgeSearchFailure::SeedUnavailable
            | a3_application::KnowledgeSearchFailure::InvalidCursor
            | a3_application::KnowledgeSearchFailure::InvalidStoredProjection,
        )
        | SearchProjectMapFailure::InvalidCandidateSet(_)
        | SearchProjectMapFailure::InvalidPublication(_)
        | SearchProjectMapFailure::Fusion(_)
        | SearchProjectMapFailure::ResourceLimit => ErrorCodeV1::LocalStorageInvalidData,
    };
    CommandErrorV1::project_open(code)
}

fn map_task_lens_workspace_error_to_v1(error: TaskLensWorkspaceFailure) -> CommandErrorV1 {
    let code = match error {
        TaskLensWorkspaceFailure::Unavailable | TaskLensWorkspaceFailure::Cancelled => {
            ErrorCodeV1::LocalStorageUnavailable
        }
        TaskLensWorkspaceFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        TaskLensWorkspaceFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        TaskLensWorkspaceFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
    };
    CommandErrorV1::project_open(code)
}

fn map_task_lens_compile_error_to_v1(error: CompileWorkspaceTaskLensFailure) -> CommandErrorV1 {
    use a3_application::{
        CompileTaskLensFailure, KnowledgeSearchFailure, TaskLensClaimStoreFailure,
    };
    let code = match error {
        CompileWorkspaceTaskLensFailure::Workspace(error) => {
            return map_task_lens_workspace_error_to_v1(error);
        }
        CompileWorkspaceTaskLensFailure::InvalidDurableAnchor => {
            ErrorCodeV1::LocalStorageInvalidData
        }
        CompileWorkspaceTaskLensFailure::Compile(CompileTaskLensFailure::Index(
            KnowledgeIndexFailure::Storage(error),
        ))
        | CompileWorkspaceTaskLensFailure::Compile(CompileTaskLensFailure::Search(
            KnowledgeSearchFailure::Storage(error),
        ))
        | CompileWorkspaceTaskLensFailure::Compile(CompileTaskLensFailure::Claims(
            TaskLensClaimStoreFailure::Storage(error),
        )) => map_storage_error_to_v1(error),
        CompileWorkspaceTaskLensFailure::Compile(
            CompileTaskLensFailure::InvalidSeedQuery
            | CompileTaskLensFailure::InvalidChannelProjection
            | CompileTaskLensFailure::CandidateSet(_)
            | CompileTaskLensFailure::CandidateSets(_)
            | CompileTaskLensFailure::Fusion(_)
            | CompileTaskLensFailure::Compile(_)
            | CompileTaskLensFailure::ResourceLimit,
        ) => ErrorCodeV1::LocalStorageInvalidData,
        CompileWorkspaceTaskLensFailure::Compile(
            CompileTaskLensFailure::IndexUnavailable
            | CompileTaskLensFailure::Index(_)
            | CompileTaskLensFailure::Search(_)
            | CompileTaskLensFailure::Claims(_)
            | CompileTaskLensFailure::Semantic(_)
            | CompileTaskLensFailure::Cancelled
            | CompileTaskLensFailure::TimedOut
            | CompileTaskLensFailure::ProgressUnavailable,
        ) => ErrorCodeV1::TaskLensUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_repository_tree_error_to_v1(error: RepositoryTreeFailure) -> CommandErrorV1 {
    let code = match error {
        RepositoryTreeFailure::Storage(error) => map_storage_error_to_v1(error),
        RepositoryTreeFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        RepositoryTreeFailure::DirectoryUnavailable => {
            ErrorCodeV1::RepositoryTreeDirectoryUnavailable
        }
        RepositoryTreeFailure::Cancelled
        | RepositoryTreeFailure::TimedOut
        | RepositoryTreeFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
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
