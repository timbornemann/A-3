//! Desktop composition root and explicit boundary mappings for A^3.

mod agent_approval_mapping;
mod agent_approval_metadata;
mod agent_conversation_runtime;
mod agent_goal_metadata;
mod agent_inspection_mapping;
mod agent_recovery_metadata;
mod agent_run_manager;
mod agent_runtime_recovery;
mod agent_session_manager;
mod clock;
/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod deep_map_manager;
mod deep_map_runtime;
mod job_ids;
mod model_settings_manager;
mod platform;
mod production_agent_run_executor;
mod project_map_atlas_mapping;
mod project_picker;
mod project_reconciliation_dialog;
mod project_settings_manager;
mod repository_index_manager;

use a3_application::ModuleCardLifecycle;
use a3_application::{
    ActivateCatalogProject, ActivateCatalogProjectError, AgentActionStore, AgentActivity,
    AgentActivityLoadResult, AgentApprovalBuffer, AgentApprovalControlAction,
    AgentApprovalControlOutcome, AgentApprovalControlResult, AgentApprovalLoadResult,
    AgentApprovalRevision, AgentControllerControl, AgentGoalCriterionDraft, AgentGoalDraft,
    AgentGoalMetadataSource, AgentInspectionBuffer, AgentInspectionContext, AgentInspectionId,
    AgentInspectionOverview, AgentInspectionQueryError, AgentInspectionRevision, AgentLogPageLimit,
    AgentLogPageOffset, AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryOutcomeKind,
    AgentRecoveryStore, AgentRunExecutionRequest, AgentRunExecutor, AgentSessionDetail,
    AgentSessionListQuery, AgentSessionStore, AgentTaskControlFailure, AgentTaskControlResult,
    AgentTaskRecovery, AgentTaskRecoveryLoadResult, AgentWorkspaceLayout, CompileWorkspaceTaskLens,
    CompileWorkspaceTaskLensFailure, CompileWorkspaceTaskLensResult, ControlAgentApproval,
    ControlAgentTaskRun, CreateAgentGoal, CreateAgentGoalFailure, DeepMapExecutionFailure,
    DeepMapExecutor, DeepMapJournalEvent, DeepMapPhase, DeepMapPublicationState,
    DeepMapPublicationStateStore, DeepMapRunCursor, DeepMapRunJournalStore, DeepMapRunSummary,
    DeepMapSafeAction, DeepMapTargetKind, GetAgentActivity, GetAgentActivityFailure,
    GetAgentApprovalCenter, GetAgentGoal, GetHealth, GetModuleCardDetail, GetModuleCardEvidence,
    GetModuleCardFreshness, GetModuleDependencyGraph, GetModuleRuntimeMap, GetModuleTreePage,
    GetProjectIndexStatus, GetProjectIndexStatusError, GetProjectMapScene,
    GetProjectMapSourcePreview, GetProjectStorageUsage, GetProjectStorageUsageError,
    GetPublishedIndexOverview, GetPublishedIndexOverviewError, GetRepositoryTreePage,
    GetTaskLensTask, GetTaskVerificationInspection, GoalContractStore, HealthQuery,
    IndexPersistenceControl, IndexPersistenceControlError, InspectAgentTaskRecovery,
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
    ModuleTreeStore, OpenProject, OpenProjectError, OpenProjectOutcome, PolicyStore,
    ProjectCatalogAdmin, ProjectCatalogAdminFailure, ProjectCatalogPage, ProjectCatalogQuery,
    ProjectDirectoryPicker, ProjectIndexStatus, ProjectInspectionFailure, ProjectMapMappingStatus,
    ProjectMapScene, ProjectMapSceneControl, ProjectMapSceneControlError, ProjectMapSceneFailure,
    ProjectMapSceneLoadResult, ProjectMapSceneModule, ProjectMapSceneModuleKind,
    ProjectMapSceneQuery, ProjectMapSceneRelation, ProjectMapSceneStore, ProjectMapSearchQuery,
    ProjectMapSearchResult, ProjectMapSourcePreview, ProjectMapSourcePreviewControl,
    ProjectMapSourcePreviewControlError, ProjectMapSourcePreviewFailure,
    ProjectMapSourcePreviewQuery, ProjectMapSourcePreviewResult, ProjectReconciliationConfirmer,
    ProjectStorageControl, ProjectStorageControlError, ProjectStorageFailure, ProjectStorageStore,
    PublishedIndexOverview, RecentProject, RemoveProjectFromList, RemoveProjectFromListError,
    RepositoryTreeChildName, RepositoryTreeControl, RepositoryTreeControlError,
    RepositoryTreeEntryKind, RepositoryTreeFailure, RepositoryTreePage, RepositoryTreePageSize,
    RepositoryTreeQuery, RepositoryTreeStore, ReviseAgentGoal, ReviseAgentGoalFailure,
    RunJournalStore, RunJournalStoreFailure, SearchProjectMap, SearchProjectMapFailure,
    TaskLedgerStore, TaskLedgerStoreFailure, TaskLedgerStoreVersion, TaskLensClaimStore,
    TaskLensCompilation, TaskLensControl, TaskLensControlError, TaskLensIndexStore,
    TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
    TaskLensWorkspaceStore, TaskVerificationInspection, TaskVerificationInspectionLoadResult,
    TraceModuleRuntimeFlow, UiPreferencesError, UiPreferencesStore, UiPreferencesStoreVersion,
    VerificationEvidenceStore,
};
use a3_application::{
    ExploreProjectMapAtlas, ProjectMapAtlasControl, ProjectMapAtlasControlError,
    ProjectMapAtlasFailure, ProjectMapAtlasLoadResult, ProjectMapAtlasSceneQuery,
    ProjectMapAtlasStore, ProjectMapEntitySelection, ProjectMapFlowSceneQuery,
    ProjectMapInventoryPageQuery,
};
use a3_credentials::NativeProviderCredentialStore;
use a3_domain::{
    AcceptanceCriterionId, AcceptanceCriterionRequirement, AcceptanceCriterionStatement,
    AgentControllerState, AgentSession, AgentSessionEntry, AgentSessionEntryKind, AgentSessionId,
    AgentSessionMode, AgentSessionRevision, AgentSessionState, AgentTurnActionClass,
    AgentTurnRepairUsage, ApplicationVersion, ApplicationVersionError, DeepMapDiagnosticCode,
    DeepMapEventSequence, DeepMapMode, DeepMapRunId, ExactSearchExplanation, ExactSearchTarget,
    ExploreBudget, FileRevision, FusionPriority, GitHead, GoalConstraint, GoalContract,
    GoalContractRevision, GoalObjective, GoalRevisionReason, GraphEdge, GraphEndpoint, GraphSymbol,
    GraphTraversalResult, Health, IndexLanguage, IndexRunId, IndexRunStatus, InvalidationReason,
    LexicalSearchExplanation, LinkResolution, ModuleCardEvidenceId, ModuleCardField, ModuleCardId,
    ModuleClaimPolarity, ModuleClaimPredicate, ModuleId, ModuleKind, ModuleRoot, NonGoal,
    ParseDiagnosticCode, ParseDiagnosticSeverity, Platform, Progress, ProjectId, ProjectIdentity,
    RepositoryPath, ResolvedModuleCardEvidence, ResultSourceExplanation, RetrievalCandidateReason,
    RunEvent, RunEventCode, RunEventKind, RunEventOutcome, SnapshotId, SourceChannel,
    SuccessVerification, SymbolId, SymbolKind, SyntaxProvider, SyntaxRelationKind, TaskId,
    TaskLedgerRevision, TaskLensEntryReason, TaskLensTarget, TaskStepId, TaskStepStatus,
    TraversalResultLimit, UserDecision, VerifiedClaimKind, VerifiedClaimStatus, WorktreeId,
};
use a3_protocol::{
    AgentActivityBlockerStatusV1, AgentActivityBlockerV1, AgentActivityBudgetV1,
    AgentActivityCodeV1, AgentActivityEventKindV1, AgentActivityEventV1, AgentActivityOutcomeV1,
    AgentActivityResponseV1, AgentActivityRunV1, AgentActivityTurnV1, AgentActivityUsageV1,
    AgentActivityV1, AgentApprovalControlActionV1, AgentApprovalControlOutcomeV1,
    AgentApprovalControlResponseV1, AgentApprovalControlResultV1, AgentApprovalResponseV1,
    AgentApprovalResultV1, AgentApprovalRuntimeStartV1, AgentControllerStateV1,
    AgentGoalContractV1, AgentGoalCriterionInputV1, AgentGoalCriterionRequirementV1,
    AgentGoalCriterionV1, AgentGoalDraftInputV1, AgentGoalMutationResponseV1, AgentGoalResponseV1,
    AgentInspectionLogResponseV1, AgentInspectionResponseV1, AgentSelectedActionV1,
    AgentSessionEntryKindV1, AgentSessionEntryV1, AgentSessionModeV1, AgentSessionResponseV1,
    AgentSessionStateV1, AgentSessionSummaryV1, AgentSessionV1, AgentSessionsResponseV1,
    AgentTaskControlAcceptedOutcomeV1, AgentTaskControlActionV1, AgentTaskControlOutcomeV1,
    AgentTaskControlResponseV1, AgentTaskControlResultV1, AgentTaskRecoveryResponseV1,
    AgentTaskRecoveryResultV1, AgentTaskRecoveryV1, AgentTaskRuntimeStartV1,
    AgentTaskRuntimeStateV1, AgentTaskRuntimeV1, CommandErrorV1, CompileTaskLensRequestV1,
    DeepMapActivityStateV1, DeepMapActivityV2, DeepMapBudgetV1, DeepMapCompactProgressV3,
    DeepMapConfigurationV1, DeepMapControlResponseV1, DeepMapEntryDetailResponseV1,
    DeepMapEntryPageResponseV1, DeepMapEntryV1, DeepMapEventV2, DeepMapFailureV1, DeepMapFailureV3,
    DeepMapLifecycleV3, DeepMapModeV2, DeepMapModelV1, DeepMapPhaseV2, DeepMapProgressV1,
    DeepMapPublicationSummaryV2, DeepMapRunPageResponseV1, DeepMapRunV1, DeepMapSafeActionV2,
    DeepMapStartResponseV2, DeepMapStatusResponseV2, DeepMapStatusResponseV3, DeepMapTargetKindV2,
    ErrorCodeV1, GitHeadV1, HealthResponseV1, IndexActivityResponseV1, IndexActivityStateV1,
    IndexActivityV1, IndexDiagnosticCodeV1, IndexDiagnosticSeverityV1, IndexDiagnosticV1,
    IndexFileDiagnosticsV1, IndexLanguageV1, IndexOverviewCountsV1, IndexOverviewResponseV1,
    IndexOverviewV1, IndexPhaseV1, IndexStateV1, ModuleCardClaimKindV1, ModuleCardClaimStateV1,
    ModuleCardClaimV1, ModuleCardCoverageBandV1, ModuleCardCoverageV1, ModuleCardDetailFieldV1,
    ModuleCardDetailResponseV1, ModuleCardDetailV1, ModuleCardEvidenceFreshnessV1,
    ModuleCardEvidencePayloadV1, ModuleCardEvidenceRelationV1, ModuleCardEvidenceResponseV1,
    ModuleCardEvidenceRevisionV1, ModuleCardEvidenceV1, ModuleCardFieldKindV1,
    ModuleCardFreshnessCountsV1, ModuleCardFreshnessReasonCountV1, ModuleCardFreshnessReasonV1,
    ModuleCardFreshnessResponseV1, ModuleCardFreshnessStatusV1, ModuleCardFreshnessV1,
    ModuleCardLifecycleV1, ModuleCardValueV1, ModuleDependencyEdgeEvidenceV1,
    ModuleDependencyEdgeV1, ModuleDependencyEndpointV1, ModuleDependencyGraphResponseV1,
    ModuleDependencyGraphV1, ModuleDependencyNodeEvidenceV1, ModuleDependencyNodeV1,
    ModuleDependencyProviderV1, ModuleDependencyRelationV1, ModuleDependencyResolutionV1,
    ModuleDependencySourcePositionV1, ModuleDependencySourceRangeV1, ModuleRuntimeFlowEdgeV1,
    ModuleRuntimeFlowHitV1, ModuleRuntimeFlowKindV1, ModuleRuntimeFlowRelationV1,
    ModuleRuntimeFlowResponseV1, ModuleRuntimeFlowTargetV1, ModuleRuntimeFlowV1,
    ModuleRuntimeMapResponseV1, ModuleRuntimeMapV1, ModuleRuntimeRootKindV1,
    ModuleRuntimeRootSetV1, ModuleRuntimeRootV1, ModuleRuntimeSymbolKindV1, ModuleRuntimeSymbolV1,
    ModuleTreeBoundaryEvidenceV1, ModuleTreeChildStateV1, ModuleTreeEntryKindV1, ModuleTreeEntryV1,
    ModuleTreeFeatureCountV1, ModuleTreePageV1, ModuleTreeResponseV1, ModuleTreeRevisionV1,
    OpenProjectResponseV1, PlatformV1, ProjectActivationResponseV1, ProjectCatalogResponseV1,
    ProjectIndexStatusV1, ProjectMapExactExplanationV1, ProjectMapLexicalExplanationV1,
    ProjectMapMappingStatusV1, ProjectMapSceneCardBindingV1, ProjectMapSceneModuleKindV1,
    ProjectMapSceneModuleV1, ProjectMapSceneRelationV1, ProjectMapSceneResponseV1,
    ProjectMapSceneV1, ProjectMapSearchChannelV1, ProjectMapSearchEvidenceSelectionV2,
    ProjectMapSearchEvidenceV1, ProjectMapSearchHitV1, ProjectMapSearchPriorityV1,
    ProjectMapSearchResponseV1, ProjectMapSearchSourceV1, ProjectMapSearchSymbolKindV1,
    ProjectMapSearchTargetV1, ProjectMapSearchV1, ProjectMapSourceHighlightV1,
    ProjectMapSourcePreviewResponseV1, ProjectMapSourcePreviewSelectionV1,
    ProjectMapSourcePreviewV1, ProjectSnapshotV1, ProjectStatusResponseV1, ProjectSummaryV1,
    QueryModuleCardDetailRequestV1, QueryModuleCardEvidenceRequestV1,
    QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
    QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectMapSceneRequestV1,
    QueryProjectMapSearchRequestV1, QueryProjectMapSourcePreviewRequestV1,
    QueryRepositoryTreeRequestV1, QueryTaskLensTaskRequestV1, RebuildProjectIndexResponseV1,
    RebuildStateV1, RecentProjectSummaryV1, RecentProjectsResponseV1, RemoveProjectResponseV1,
    RepositoryTreeEntryKindV1, RepositoryTreeEntryV1, RepositoryTreePageV1,
    RepositoryTreeResponseV1, TaskLensClaimEvidenceV1, TaskLensClaimKindV1,
    TaskLensClaimPolarityV1, TaskLensClaimPredicateV1, TaskLensClaimV1, TaskLensCompileResponseV1,
    TaskLensEntryReasonV1, TaskLensEntryTargetV1, TaskLensEntryV1, TaskLensModuleKindV1,
    TaskLensPathV1, TaskLensPriorityV1, TaskLensRetrievalChannelV1, TaskLensRetrievalSourceV1,
    TaskLensStepStatusV1, TaskLensStepV1, TaskLensTaskResponseV1, TaskLensTaskSummaryV1,
    TaskLensTasksResponseV1, TaskLensV1, UiPreferencesResponseV1,
};
use a3_protocol::{
    DeepMapAtlasImpactItemV1, DeepMapAtlasImpactKindV1, DeepMapAtlasImpactResponseV1,
    DeepMapAtlasImpactResultV1, DeepMapAtlasImpactSummaryV1, DeepMapCardFieldV1,
    DeepMapCurrentActivityV1, DeepMapDashboardFailureV1, DeepMapDashboardFreshnessV1,
    DeepMapDashboardPhaseProgressV1, DeepMapDashboardPhaseStateV1, DeepMapDashboardPhaseV1,
    DeepMapDashboardStateV1, DeepMapModuleStateV1, DeepMapModuleStepV1,
    DeepMapModuleStepsResponseV1, DeepMapPlanStepStateV1, DeepMapRunDashboardResponseV1,
    DeepMapRunModuleV1, DeepMapRunModulesResponseV1, DeepMapSelectionReasonV1, ProtocolVersion,
};
use a3_protocol::{
    ProjectMapAtlasSceneResponseV1, ProjectMapEntityContextResponseV1,
    ProjectMapFlowSceneResponseV1, ProjectMapInventoryPageResponseV1,
};
use a3_storage_libsql::{
    CatalogOpenError, LibsqlKnowledgeStore, StorageLayout, StorageLayoutError,
};
use a3_workspace::{RepositoryInspector, WorkspaceAgentSourceReader};
use agent_approval_mapping::map_agent_approval_to_v1;
use agent_approval_metadata::SystemAgentApprovalMetadata;
use agent_goal_metadata::SystemAgentGoalMetadata;
use agent_inspection_mapping::{
    map_agent_inspection_to_v1, map_agent_log_page_to_v1, map_inspection_stream_from_v1,
};
use agent_recovery_metadata::SystemAgentRecoveryMetadata;
use agent_run_manager::{AgentRunActivityState, AgentRunManager, AgentRunManagerControlError};
use agent_runtime_recovery::CoreAgentRuntimeRecovery;
use agent_session_manager::{
    AgentAskResearcher, AgentSessionManager, AgentSessionManagerDependencies,
    AgentSessionManagerFailure, AgentSessionRunReporter, PresentationMutation,
};
use clock::SystemJobClock;
use deep_map_manager::{
    DeepMapActivity, DeepMapActivityState, DeepMapManager, DeepMapManagerControlError,
};
use deep_map_runtime::DeepMapRuntime;
use job_ids::DesktopJobIds;
use model_settings_manager::ModelSettingsManager;
use platform::SystemPlatform;
use production_agent_run_executor::{ProductionAgentRunExecutor, ProductionAgentRunPorts};
use project_map_atlas_mapping::{
    map_context_to_v1, map_flow_to_v1, map_index_evidence_from_v1, map_inventory_to_v1,
    map_scene_to_v1,
};
use project_picker::NativeProjectDirectoryPicker;
use project_reconciliation_dialog::NativeProjectReconciliationConfirmer;
use project_settings_manager::ProjectSettingsManager;
use repository_index_manager::{
    RepositoryIndexActivity, RepositoryIndexActivityState, RepositoryIndexDeactivationError,
    RepositoryIndexManager, RepositoryIndexRebuildRequestError, RepositoryIndexRebuildState,
};
use std::collections::{BTreeMap, BTreeSet};
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
    model_settings: Option<ModelSettingsManager>,
    project_settings: Option<ProjectSettingsManager>,
    open_project: OpenProject,
    activate_catalog_project: ActivateCatalogProject,
    project_catalog_store: Arc<dyn KnowledgeStore>,
    recent_projects: ListRecentProjects,
    project_status: Option<GetProjectIndexStatus>,
    index_overview: Option<GetPublishedIndexOverview>,
    module_card_freshness: Option<GetModuleCardFreshness>,
    module_card_detail: Option<GetModuleCardDetail>,
    module_card_evidence: Option<GetModuleCardEvidence>,
    project_map_scene: Option<GetProjectMapScene>,
    project_map_atlas: Option<ExploreProjectMapAtlas>,
    project_map_source_preview: Option<GetProjectMapSourcePreview>,
    module_dependency_graph: Option<GetModuleDependencyGraph>,
    module_runtime_map: Option<GetModuleRuntimeMap>,
    module_runtime_flow: Option<TraceModuleRuntimeFlow>,
    project_map_search: Option<SearchProjectMap>,
    task_lens_tasks: Option<ListTaskLensTasks>,
    task_lens_task: Option<GetTaskLensTask>,
    task_lens_compile: Option<CompileWorkspaceTaskLens>,
    agent_activity: Option<GetAgentActivity>,
    agent_verification: Option<GetTaskVerificationInspection>,
    agent_inspection: Arc<AgentInspectionBuffer>,
    agent_approval: Arc<AgentApprovalBuffer>,
    agent_approval_query: Option<GetAgentApprovalCenter>,
    agent_approval_control: Option<ControlAgentApproval>,
    agent_approval_metadata: SystemAgentApprovalMetadata,
    agent_task_recovery: Option<InspectAgentTaskRecovery>,
    agent_task_control: Option<ControlAgentTaskRun>,
    agent_recovery_metadata: SystemAgentRecoveryMetadata,
    agent_goal_query: Option<GetAgentGoal>,
    agent_goal_create: Option<CreateAgentGoal>,
    agent_goal_revise: Option<ReviseAgentGoal>,
    module_tree: Option<GetModuleTreePage>,
    repository_tree: Option<GetRepositoryTreePage>,
    project_storage: Option<GetProjectStorageUsage>,
    remove_project: Option<RemoveProjectFromList>,
    active_project: Mutex<Option<ActiveProject>>,
    project_operation_active: AtomicBool,
    agent_task_operation_active: AtomicBool,
    index_manager: Option<RepositoryIndexManager>,
    deep_map_manager: Option<DeepMapManager>,
    deep_map_runtime: Option<DeepMapRuntime>,
    deep_map_publication_state: Option<Arc<dyn DeepMapPublicationStateStore>>,
    deep_map_journal: Option<Arc<dyn DeepMapRunJournalStore>>,
    deep_map_dashboard_index: Option<Arc<dyn KnowledgeIndexStore>>,
    agent_run_manager: Option<Arc<AgentRunManager>>,
    agent_sessions: Option<AgentSessionManager>,
    ui_preferences: Option<Arc<dyn UiPreferencesStore>>,
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

    /// Lists bounded project-local Agent conversations for the session rail.
    pub async fn query_agent_sessions(
        &self,
        query: AgentSessionListQuery,
    ) -> Result<AgentSessionsResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionsResponseV1::no_project());
        };
        let manager = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        let page = manager
            .list(&active.project, &query)
            .await
            .map_err(map_agent_session_failure)?;
        let next_cursor = page
            .has_more()
            .then(|| {
                page.sessions()
                    .last()
                    .map(|session| session.updated_at().unix_millis().to_string())
            })
            .flatten();
        Ok(AgentSessionsResponseV1::available(
            page.sessions()
                .iter()
                .map(map_agent_session_summary_to_v1)
                .collect(),
            next_cursor,
        ))
    }

    /// Loads one bounded project-local conversation page.
    pub async fn query_agent_session(
        &self,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        limit: u16,
    ) -> Result<AgentSessionResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionResponseV1::no_project());
        };
        let manager = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        let mut detail = manager
            .load(&active.project, session_id, before_sequence, limit)
            .await
            .map_err(map_agent_session_failure)?;
        if let Some(current) = detail.as_ref()
            && let Some(work_item) = current.session().active_work_item()
            && let Some(runtime) = self
                .agent_run_manager
                .as_ref()
                .map(|value| value.activity())
            && runtime.task_id() == Some(work_item.task_id())
        {
            let projected = match runtime.state() {
                AgentRunActivityState::Paused => Some(AgentSessionState::Paused),
                AgentRunActivityState::Queued
                | AgentRunActivityState::Running
                | AgentRunActivityState::Pausing => Some(AgentSessionState::Running),
                AgentRunActivityState::Idle
                | AgentRunActivityState::Cancelling
                | AgentRunActivityState::Succeeded
                | AgentRunActivityState::Failed
                | AgentRunActivityState::Cancelled => None,
            };
            if projected.is_some_and(|state| state != current.session().state()) {
                manager
                    .project_runtime_state(
                        &active.project,
                        work_item.task_id(),
                        projected.unwrap_or(AgentSessionState::Running),
                    )
                    .await
                    .map_err(map_agent_session_failure)?;
                detail = manager
                    .load(&active.project, session_id, before_sequence, limit)
                    .await
                    .map_err(map_agent_session_failure)?;
            }
        }
        match detail {
            Some(detail) => Ok(AgentSessionResponseV1::available(
                map_agent_session_detail_to_v1(&detail),
            )),
            None => Ok(AgentSessionResponseV1::not_found()),
        }
    }

    /// Persists one message and completes its bounded read-only conversation turn.
    pub async fn submit_agent_message(
        &self,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        message: String,
    ) -> Result<AgentSessionResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionResponseV1::no_project());
        };
        let manager = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        let detail = manager
            .submit(
                &active.project,
                session_id,
                expected_revision,
                start_mode,
                message,
            )
            .await
            .map_err(map_agent_session_failure)?;
        Ok(AgentSessionResponseV1::available(
            map_agent_session_detail_to_v1(&detail),
        ))
    }

    /// Applies one non-privileged session-presentation control.
    pub(crate) async fn control_agent_session_presentation(
        &self,
        session_id: AgentSessionId,
        expected_revision: AgentSessionRevision,
        mutation: PresentationMutation,
    ) -> Result<AgentSessionResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionResponseV1::no_project());
        };
        let manager = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        match manager
            .update_presentation(&active.project, session_id, expected_revision, mutation)
            .await
            .map_err(map_agent_session_failure)?
        {
            Some(detail) => Ok(AgentSessionResponseV1::available(
                map_agent_session_detail_to_v1(&detail),
            )),
            None => Ok(AgentSessionResponseV1::not_found()),
        }
    }

    /// Materializes and queues exactly the reviewed immutable plan revision.
    pub(crate) async fn implement_agent_session_plan(
        &self,
        session_id: AgentSessionId,
        expected_revision: AgentSessionRevision,
        plan_revision: u32,
    ) -> Result<AgentSessionResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionResponseV1::no_project());
        };
        let manager = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        let detail = manager
            .implement_plan(
                &active.project,
                session_id,
                expected_revision,
                plan_revision,
            )
            .await
            .map_err(map_agent_session_failure)?;
        Ok(AgentSessionResponseV1::available(
            map_agent_session_detail_to_v1(&detail),
        ))
    }

    /// Resolves Pause, Resume, or Cancel from the session's Core-owned task anchor.
    pub(crate) async fn control_agent_session_runtime(
        &self,
        session_id: AgentSessionId,
        expected_revision: AgentSessionRevision,
        action: AgentTaskControlActionV1,
    ) -> Result<AgentSessionResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(AgentSessionResponseV1::no_project());
        };
        let sessions = self
            .agent_sessions
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        let detail = sessions
            .load(&active.project, session_id, None, 128)
            .await
            .map_err(map_agent_session_failure)?
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        if detail.session().revision() != expected_revision {
            return Err(CommandErrorV1::agent_session(
                ErrorCodeV1::AgentSessionRevisionConflict,
            ));
        }
        let Some(task_id) = detail
            .session()
            .active_work_item()
            .map(|item| item.task_id())
        else {
            if action != AgentTaskControlActionV1::Cancel
                || detail.session().state() != AgentSessionState::Running
            {
                return Err(CommandErrorV1::agent_session(
                    ErrorCodeV1::AgentSessionUnavailable,
                ));
            }
            sessions
                .cancel_conversation(&active.project, session_id)
                .await
                .map_err(map_agent_session_failure)?;
            return self.query_agent_session(session_id, None, 128).await;
        };
        let target =
            load_agent_runtime_target(self.agent_activity.as_ref(), &active.project, task_id)
                .await?;
        let AgentRuntimeTargetLoad::Available(target) = target else {
            return Err(CommandErrorV1::agent_session(
                ErrorCodeV1::AgentSessionRevisionConflict,
            ));
        };
        let _result = self
            .control_agent_task_run(
                task_id,
                target.ledger_revision.get(),
                target.ledger_store_version,
                action,
            )
            .await?;
        self.query_agent_session(session_id, None, 128).await
    }

    /// Loads global content-free Agent workspace layout preferences.
    pub async fn query_ui_preferences(&self) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
        let store = self
            .ui_preferences
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        store
            .load()
            .await
            .map(map_ui_preferences_to_v1)
            .map_err(map_ui_preferences_failure)
    }

    /// Compare-and-appends global content-free Agent workspace layout preferences.
    pub async fn update_agent_workspace_layout(
        &self,
        expected: UiPreferencesStoreVersion,
        layout: AgentWorkspaceLayout,
    ) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
        let store = self
            .ui_preferences
            .as_ref()
            .ok_or_else(|| CommandErrorV1::agent_session(ErrorCodeV1::AgentSessionUnavailable))?;
        store
            .append(expected, layout)
            .await
            .map(map_ui_preferences_to_v1)
            .map_err(map_ui_preferences_failure)
    }

    /// Reads the current global model Settings without provider or network access.
    pub async fn query_settings(&self) -> Result<a3_protocol::SettingsResponseV1, CommandErrorV1> {
        self.model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .query()
            .await
    }

    /// Validates and stores one closed active provider without performing a request.
    pub async fn configure_model_provider(
        &self,
        expected: a3_application::DesktopSettingsStoreVersion,
        provider_kind: a3_protocol::ModelProviderKindV1,
        endpoint: Option<&str>,
    ) -> Result<a3_protocol::SettingsResponseV1, CommandErrorV1> {
        let response = self
            .model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .configure_provider(expected, provider_kind, endpoint)
            .await?;
        self.synchronize_deep_map_runtime().await?;
        Ok(response)
    }

    /// Stores one bounded credential for the Core-owned provider without network access.
    pub async fn set_model_provider_credential(
        &self,
        expected: a3_application::DesktopSettingsStoreVersion,
        secret: a3_application::ProviderApiKey,
    ) -> Result<a3_protocol::SettingsResponseV1, CommandErrorV1> {
        let response = self
            .model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .set_credential(expected, secret)
            .await?;
        self.synchronize_deep_map_runtime().await?;
        Ok(response)
    }

    /// Deletes the current provider credential without contacting the provider.
    pub async fn delete_model_provider_credential(
        &self,
        expected: a3_application::DesktopSettingsStoreVersion,
    ) -> Result<a3_protocol::SettingsResponseV1, CommandErrorV1> {
        let response = self
            .model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .delete_credential(expected)
            .await?;
        self.synchronize_deep_map_runtime().await?;
        Ok(response)
    }

    /// Explicitly discovers a bounded model list from the current local provider.
    pub async fn discover_provider_models(
        &self,
        expected: a3_application::DesktopSettingsStoreVersion,
    ) -> Result<a3_protocol::ProviderModelsResponseV1, CommandErrorV1> {
        self.model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .discover_models(expected)
            .await
    }

    /// Performs one explicit bounded local capability probe.
    pub async fn probe_model_role(
        &self,
        expected: a3_application::DesktopSettingsStoreVersion,
        request: &a3_protocol::ProbeModelRoleRequestV1,
    ) -> Result<a3_protocol::SettingsResponseV1, CommandErrorV1> {
        let response = self
            .model_settings
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .probe(expected, request)
            .await?;
        self.synchronize_deep_map_runtime().await?;
        Ok(response)
    }

    /// Requests cooperative cancellation of the single explicit model operation.
    pub fn cancel_model_probe(&self) -> a3_protocol::CancelModelProbeResponseV1 {
        self.model_settings.as_ref().map_or_else(
            || a3_protocol::CancelModelProbeResponseV1::new(false),
            ModelSettingsManager::cancel_probe,
        )
    }

    async fn synchronize_deep_map_runtime(&self) -> Result<(), CommandErrorV1> {
        let Some(runtime) = &self.deep_map_runtime else {
            return Ok(());
        };
        let executor = runtime.resolve().await;
        self.deep_map_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?
            .configure_executor(executor)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))
    }

    async fn ensure_deep_map_runtime_available(&self) -> Result<(), CommandErrorV1> {
        let manager = self
            .deep_map_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        if manager.model().is_some() {
            return Ok(());
        }
        self.synchronize_deep_map_runtime()
            .await
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))
    }

    /// Reads active-project ignore and manifest-evidenced command Settings.
    pub async fn query_project_settings(
        &self,
    ) -> Result<a3_protocol::ProjectSettingsResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(a3_protocol::ProjectSettingsResponseV1::no_project());
        };
        self.project_settings
            .as_ref()
            .ok_or_else(|| {
                CommandErrorV1::project_settings(ErrorCodeV1::ProjectSettingsUnavailable)
            })?
            .query(&active.project)
            .await
    }

    /// Confirms a non-empty subset of the exact current safe-command catalog.
    pub async fn confirm_project_command_allowlist(
        &self,
        expected_catalog_id: a3_domain::CommandCatalogId,
        expected_revision: Option<a3_application::CommandAllowlistStoreVersion>,
        command_ids: Vec<a3_domain::DiscoveredCommandId>,
    ) -> Result<a3_protocol::ProjectSettingsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_settings)?;
        let active = lock_recovering_poison(&self.active_project)
            .clone()
            .ok_or_else(|| CommandErrorV1::project_settings(ErrorCodeV1::NoActiveProject))?;
        self.project_settings
            .as_ref()
            .ok_or_else(|| {
                CommandErrorV1::project_settings(ErrorCodeV1::ProjectSettingsUnavailable)
            })?
            .confirm(
                &active.project,
                expected_catalog_id,
                expected_revision,
                command_ids,
            )
            .await
    }

    /// Executes one user-controlled native project selection and maps it to IPC V1.
    pub async fn open_project(&self) -> Result<OpenProjectResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_open)?;
        let _agent_operation = self
            .try_acquire_agent_task_operation()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::ProjectOperationBusy))?;
        let outcome = self
            .open_project
            .execute()
            .await
            .map_err(map_open_project_error_to_v1)?;
        if let OpenProjectOutcome::Opened {
            project,
            project_id,
        } = &outcome
        {
            self.activate_project_runtime(
                project.as_ref().clone(),
                *project_id,
                CommandErrorV1::project_open,
            )?;
        }
        Ok(map_open_project_to_v1(outcome))
    }

    /// Reads one fixed 25-entry catalog page from safe display projections.
    pub async fn query_project_catalog(
        &self,
        query: &ProjectCatalogQuery,
    ) -> Result<ProjectCatalogResponseV1, CommandErrorV1> {
        self.project_catalog_store
            .list_project_catalog(query)
            .await
            .map(map_project_catalog_to_v1)
            .map_err(|error| CommandErrorV1::project_open(map_storage_error_to_v1(error)))
    }

    /// Revalidates and activates one previously listed worktree ID.
    pub async fn activate_catalog_project(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_open)?;
        let _agent_operation = self
            .try_acquire_agent_task_operation()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::ProjectOperationBusy))?;
        let (project, project_id) = self
            .activate_catalog_project
            .execute(worktree_id)
            .await
            .map_err(map_catalog_activation_error_to_v1)?;
        self.activate_validated_catalog_project(&project, project_id)
            .await?;
        Ok(ProjectActivationResponseV1::activated(
            project_id.to_string(),
            map_project_summary_to_v1(&project),
        ))
    }

    /// Restores only the most recently activated catalog entry, without fallback.
    pub async fn restore_last_project(
        &self,
    ) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_open)?;
        let _agent_operation = self
            .try_acquire_agent_task_operation()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::ProjectOperationBusy))?;
        let Some((project, project_id)) = self
            .activate_catalog_project
            .restore_last()
            .await
            .map_err(map_catalog_activation_error_to_v1)?
        else {
            return Ok(ProjectActivationResponseV1::no_saved_project());
        };
        self.activate_validated_catalog_project(&project, project_id)
            .await?;
        Ok(ProjectActivationResponseV1::activated(
            project_id.to_string(),
            map_project_summary_to_v1(&project),
        ))
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

    /// Returns a current Card selected only through project-bound Deep-Map tokens.
    pub async fn query_deep_map_module_card_detail(
        &self,
        run_selection: &str,
        module_selection: &str,
    ) -> Result<ModuleCardDetailResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Ok(ModuleCardDetailResponseV1::no_project());
        };
        let worktree_id = active.project.worktree().id();
        let run_id = decode_deep_map_run_selection(worktree_id, run_selection)
            .map_err(|_| invalid_module_card_detail_query())?;
        let module_id = decode_deep_map_module_selection(worktree_id, run_id, module_selection)
            .map_err(|_| invalid_module_card_detail_query())?;
        let run = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(invalid_module_card_detail_query)?
            .load_run(&active.project, run_id)
            .await
            .map_err(|_| invalid_module_card_detail_query())?
            .ok_or_else(invalid_module_card_detail_query)?;
        let index = self.load_deep_map_dashboard_index(&active.project).await?;
        if !index.is_some_and(|value| {
            value.run().id() == run.start().anchor().index_run_id()
                && value.run().snapshot_id() == run.start().anchor().snapshot_id()
        }) {
            return Ok(ModuleCardDetailResponseV1::card_unavailable());
        }
        self.query_module_card_detail(&ModuleCardDetailQuery::new(module_id))
            .await
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

    /// Reads source only for one still-current Evidence hook selected from the Inspector.
    pub async fn query_project_map_source_preview(
        &self,
        query: &ProjectMapSourcePreviewQuery,
    ) -> Result<ProjectMapSourcePreviewResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapSourcePreviewResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_source_preview else {
            return Ok(ProjectMapSourcePreviewResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_project_map_source_preview_error_to_v1)
            .map(|result| match result {
                ProjectMapSourcePreviewResult::NoPublishedIndex => {
                    ProjectMapSourcePreviewResponseV1::no_published_index()
                }
                ProjectMapSourcePreviewResult::ProjectionUnavailable => {
                    ProjectMapSourcePreviewResponseV1::projection_unavailable()
                }
                ProjectMapSourcePreviewResult::ModuleUnavailable => {
                    ProjectMapSourcePreviewResponseV1::module_unavailable()
                }
                ProjectMapSourcePreviewResult::CardUnavailable => {
                    ProjectMapSourcePreviewResponseV1::card_unavailable()
                }
                ProjectMapSourcePreviewResult::SelectionChanged => {
                    ProjectMapSourcePreviewResponseV1::selection_changed()
                }
                ProjectMapSourcePreviewResult::EvidenceUnavailable => {
                    ProjectMapSourcePreviewResponseV1::evidence_unavailable()
                }
                ProjectMapSourcePreviewResult::StaleEvidence => {
                    ProjectMapSourcePreviewResponseV1::stale_evidence()
                }
                ProjectMapSourcePreviewResult::Available(preview) => {
                    ProjectMapSourcePreviewResponseV1::available(
                        map_project_map_source_preview_to_v1(&preview),
                    )
                }
            })
    }

    /// Returns the bounded deterministic architecture-atlas scene for the active project.
    pub async fn query_project_map_scene(
        &self,
        query: &ProjectMapSceneQuery,
    ) -> Result<ProjectMapSceneResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapSceneResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_scene else {
            return Ok(ProjectMapSceneResponseV1::no_published_index());
        };
        reader
            .execute(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_project_map_scene_error_to_v1)
            .map(|result| match result {
                ProjectMapSceneLoadResult::NoPublishedIndex => {
                    ProjectMapSceneResponseV1::no_published_index()
                }
                ProjectMapSceneLoadResult::ProjectionUnavailable => {
                    ProjectMapSceneResponseV1::projection_unavailable()
                }
                ProjectMapSceneLoadResult::FocusUnavailable => {
                    ProjectMapSceneResponseV1::focus_unavailable()
                }
                ProjectMapSceneLoadResult::Scene(scene) => {
                    ProjectMapSceneResponseV1::available(map_project_map_scene_to_v1(&scene))
                }
            })
    }

    /// Returns one bounded semantic-zoom scene for the active project's latest publication.
    pub async fn query_project_map_atlas_scene(
        &self,
        query: &ProjectMapAtlasSceneQuery,
    ) -> Result<ProjectMapAtlasSceneResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapAtlasSceneResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_atlas else {
            return Ok(ProjectMapAtlasSceneResponseV1::no_published_index());
        };
        reader
            .scene(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_project_map_atlas_error_to_v1)
            .map(|result| match result {
                ProjectMapAtlasLoadResult::NoPublishedIndex => {
                    ProjectMapAtlasSceneResponseV1::no_published_index()
                }
                ProjectMapAtlasLoadResult::ProjectionUnavailable => {
                    ProjectMapAtlasSceneResponseV1::projection_unavailable()
                }
                ProjectMapAtlasLoadResult::SelectionChanged => {
                    ProjectMapAtlasSceneResponseV1::selection_changed()
                }
                ProjectMapAtlasLoadResult::Available(scene) => {
                    ProjectMapAtlasSceneResponseV1::available(map_scene_to_v1(&scene))
                }
            })
    }

    /// Returns bounded Inspector metadata for one Core-issued current Atlas selection.
    pub async fn query_project_map_entity_context(
        &self,
        selection: ProjectMapEntitySelection,
    ) -> Result<ProjectMapEntityContextResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapEntityContextResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_atlas else {
            return Ok(ProjectMapEntityContextResponseV1::no_published_index());
        };
        reader
            .context(
                &active.project,
                selection,
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(map_project_map_atlas_error_to_v1)
            .map(|result| match result {
                ProjectMapAtlasLoadResult::NoPublishedIndex => {
                    ProjectMapEntityContextResponseV1::no_published_index()
                }
                ProjectMapAtlasLoadResult::ProjectionUnavailable => {
                    ProjectMapEntityContextResponseV1::projection_unavailable()
                }
                ProjectMapAtlasLoadResult::SelectionChanged => {
                    ProjectMapEntityContextResponseV1::selection_changed()
                }
                ProjectMapAtlasLoadResult::Available(context) => {
                    ProjectMapEntityContextResponseV1::available(map_context_to_v1(&context))
                }
            })
    }

    /// Returns exactly one fixed fifty-entry inventory page for the active publication.
    pub async fn query_project_map_inventory_page(
        &self,
        query: &ProjectMapInventoryPageQuery,
    ) -> Result<ProjectMapInventoryPageResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapInventoryPageResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_atlas else {
            return Ok(ProjectMapInventoryPageResponseV1::no_published_index());
        };
        reader
            .inventory(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_project_map_atlas_error_to_v1)
            .map(|result| match result {
                ProjectMapAtlasLoadResult::NoPublishedIndex => {
                    ProjectMapInventoryPageResponseV1::no_published_index()
                }
                ProjectMapAtlasLoadResult::ProjectionUnavailable => {
                    ProjectMapInventoryPageResponseV1::projection_unavailable()
                }
                ProjectMapAtlasLoadResult::SelectionChanged => {
                    ProjectMapInventoryPageResponseV1::selection_changed()
                }
                ProjectMapAtlasLoadResult::Available(page) => {
                    ProjectMapInventoryPageResponseV1::available(map_inventory_to_v1(&page))
                }
            })
    }

    /// Returns one fixed-preset callers, callees, tests, or data-access flow scene.
    pub async fn query_project_map_flow_scene(
        &self,
        query: &ProjectMapFlowSceneQuery,
    ) -> Result<ProjectMapFlowSceneResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(ProjectMapFlowSceneResponseV1::no_project());
        };
        let Some(reader) = &self.project_map_atlas else {
            return Ok(ProjectMapFlowSceneResponseV1::no_published_index());
        };
        reader
            .flow(&active.project, query, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_project_map_atlas_error_to_v1)
            .map(|result| match result {
                ProjectMapAtlasLoadResult::NoPublishedIndex => {
                    ProjectMapFlowSceneResponseV1::no_published_index()
                }
                ProjectMapAtlasLoadResult::ProjectionUnavailable => {
                    ProjectMapFlowSceneResponseV1::projection_unavailable()
                }
                ProjectMapAtlasLoadResult::SelectionChanged => {
                    ProjectMapFlowSceneResponseV1::selection_changed()
                }
                ProjectMapAtlasLoadResult::Available(flow) => {
                    ProjectMapFlowSceneResponseV1::available(map_flow_to_v1(&flow))
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

    /// Loads one complete current Goal Contract for the Agent workspace.
    pub async fn query_agent_goal(
        &self,
        task_id: TaskId,
    ) -> Result<AgentGoalResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentGoalResponseV1::no_project());
        };
        let query = self
            .agent_goal_query
            .as_ref()
            .ok_or_else(agent_goal_unavailable)?;
        let goal = query
            .execute(&active.project, task_id)
            .await
            .map_err(map_agent_goal_store_error_to_v1)?;
        Ok(
            goal.map_or_else(AgentGoalResponseV1::task_not_found, |goal| {
                AgentGoalResponseV1::available(map_agent_goal_to_v1(&goal))
            }),
        )
    }

    /// Creates one task together with its initial immutable Goal Contract revision.
    pub async fn create_agent_goal(
        &self,
        draft: AgentGoalDraft,
    ) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Err(CommandErrorV1::project_open(ErrorCodeV1::NoActiveProject));
        };
        let create = self
            .agent_goal_create
            .as_ref()
            .ok_or_else(agent_goal_unavailable)?;
        let goal = create
            .execute(&active.project, draft)
            .await
            .map_err(map_create_agent_goal_error_to_v1)?;
        Ok(AgentGoalMutationResponseV1::new(map_agent_goal_to_v1(
            &goal,
        )))
    }

    /// Compare-and-appends one material successor Goal Contract revision.
    pub async fn revise_agent_goal(
        &self,
        task_id: TaskId,
        expected_revision: GoalContractRevision,
        draft: AgentGoalDraft,
        reason: GoalRevisionReason,
    ) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Err(CommandErrorV1::project_open(ErrorCodeV1::NoActiveProject));
        };
        let revise = self
            .agent_goal_revise
            .as_ref()
            .ok_or_else(agent_goal_unavailable)?;
        let goal = revise
            .execute(&active.project, task_id, expected_revision, draft, reason)
            .await
            .map_err(map_revise_agent_goal_error_to_v1)?;
        Ok(AgentGoalMutationResponseV1::new(map_agent_goal_to_v1(
            &goal,
        )))
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

    /// Loads bounded current execution activity without accepting a run identity from the WebView.
    pub async fn query_agent_activity(
        &self,
        task_id: TaskId,
    ) -> Result<AgentActivityResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentActivityResponseV1::no_project());
        };
        let reader = self
            .agent_activity
            .as_ref()
            .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::TaskLensUnavailable))?;
        match reader
            .execute(&active.project, task_id, &DesktopBoundedReadControl::new())
            .await
            .map_err(map_agent_activity_error_to_v1)?
        {
            AgentActivityLoadResult::TaskNotFound => Ok(AgentActivityResponseV1::task_not_found()),
            AgentActivityLoadResult::LedgerUnavailable => {
                Ok(AgentActivityResponseV1::ledger_unavailable())
            }
            AgentActivityLoadResult::GoalRevisionMismatch {
                current_revision,
                ledger_revision,
            } => Ok(AgentActivityResponseV1::goal_revision_mismatch(
                current_revision,
                ledger_revision,
            )),
            AgentActivityLoadResult::ActivityChanged => {
                Ok(AgentActivityResponseV1::activity_changed())
            }
            AgentActivityLoadResult::Available(activity) => map_agent_activity_to_v1(&activity)
                .map(AgentActivityResponseV1::available)
                .ok_or_else(|| CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)),
        }
    }

    /// Loads exact volatile diff/process data plus freshly derived durable verification truth.
    pub async fn query_agent_inspection(
        &self,
        task_id: TaskId,
    ) -> Result<AgentInspectionResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentInspectionResponseV1::no_project());
        };
        let reader = self
            .agent_verification
            .as_ref()
            .ok_or_else(agent_inspection_unavailable)?;
        match reader
            .execute(&active.project, task_id, &DesktopBoundedReadControl::new())
            .await
            .map_err(|_| agent_inspection_unavailable())?
        {
            TaskVerificationInspectionLoadResult::TaskNotFound => {
                Ok(AgentInspectionResponseV1::task_not_found())
            }
            TaskVerificationInspectionLoadResult::LedgerUnavailable => {
                Ok(AgentInspectionResponseV1::ledger_unavailable())
            }
            TaskVerificationInspectionLoadResult::GoalRevisionMismatch => {
                Ok(AgentInspectionResponseV1::goal_revision_mismatch())
            }
            TaskVerificationInspectionLoadResult::InspectionChanged => {
                Ok(AgentInspectionResponseV1::inspection_changed())
            }
            TaskVerificationInspectionLoadResult::Available(verification) => {
                let volatile = self
                    .agent_inspection
                    .overview(&active.project, task_id)
                    .map_err(|_| agent_inspection_unavailable())?;
                let volatile = volatile
                    .as_ref()
                    .filter(|overview| inspection_contexts_are_current(overview, &verification));
                Ok(AgentInspectionResponseV1::available(
                    map_agent_inspection_to_v1(volatile, &verification),
                ))
            }
        }
    }

    /// Loads one explicit safe log page after revalidating durable task and volatile anchors.
    #[allow(clippy::too_many_arguments)]
    pub async fn query_agent_inspection_log(
        &self,
        task_id: TaskId,
        revision: AgentInspectionRevision,
        inspection_id: AgentInspectionId,
        stream: a3_protocol::AgentInspectionStreamV1,
        offset: AgentLogPageOffset,
        limit: AgentLogPageLimit,
    ) -> Result<AgentInspectionLogResponseV1, CommandErrorV1> {
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentInspectionLogResponseV1::no_project());
        };
        let reader = self
            .agent_verification
            .as_ref()
            .ok_or_else(agent_inspection_unavailable)?;
        let verification = match reader
            .execute(&active.project, task_id, &DesktopBoundedReadControl::new())
            .await
            .map_err(|_| agent_inspection_unavailable())?
        {
            TaskVerificationInspectionLoadResult::Available(verification) => verification,
            TaskVerificationInspectionLoadResult::InspectionChanged => {
                return Ok(AgentInspectionLogResponseV1::inspection_changed());
            }
            TaskVerificationInspectionLoadResult::TaskNotFound
            | TaskVerificationInspectionLoadResult::LedgerUnavailable
            | TaskVerificationInspectionLoadResult::GoalRevisionMismatch => {
                return Ok(AgentInspectionLogResponseV1::unavailable());
            }
        };
        let overview = match self.agent_inspection.overview(&active.project, task_id) {
            Ok(Some(overview)) if overview.revision() == revision => overview,
            Ok(Some(_)) => return Ok(AgentInspectionLogResponseV1::inspection_changed()),
            Ok(None) | Err(AgentInspectionQueryError::Unavailable) => {
                return Ok(AgentInspectionLogResponseV1::unavailable());
            }
            Err(AgentInspectionQueryError::RevisionChanged) => {
                return Ok(AgentInspectionLogResponseV1::inspection_changed());
            }
            Err(
                AgentInspectionQueryError::RecordUnavailable
                | AgentInspectionQueryError::InvalidCursor,
            ) => return Ok(AgentInspectionLogResponseV1::unavailable()),
        };
        if !inspection_contexts_are_current(&overview, &verification)
            || !overview
                .processes()
                .iter()
                .any(|process| process.id() == inspection_id)
        {
            return Ok(AgentInspectionLogResponseV1::unavailable());
        }
        match self.agent_inspection.load_process_log_page(
            &active.project,
            task_id,
            revision,
            inspection_id,
            map_inspection_stream_from_v1(stream),
            offset,
            limit,
        ) {
            Ok(page) => Ok(AgentInspectionLogResponseV1::available(
                map_agent_log_page_to_v1(&page),
            )),
            Err(AgentInspectionQueryError::RevisionChanged) => {
                Ok(AgentInspectionLogResponseV1::inspection_changed())
            }
            Err(
                AgentInspectionQueryError::Unavailable
                | AgentInspectionQueryError::RecordUnavailable
                | AgentInspectionQueryError::InvalidCursor,
            ) => Ok(AgentInspectionLogResponseV1::unavailable()),
        }
    }

    /// Loads the exact task-bound approval action and its current durable lifecycle.
    pub async fn query_agent_approval(
        &self,
        task_id: TaskId,
    ) -> Result<AgentApprovalResponseV1, CommandErrorV1> {
        let Some(_operation) = self.try_acquire_agent_task_operation() else {
            return Ok(AgentApprovalResponseV1::new(
                AgentApprovalResultV1::ActivityChanged,
            ));
        };
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentApprovalResponseV1::new(
                AgentApprovalResultV1::NoProject,
            ));
        };
        let reader = self
            .agent_approval_query
            .as_ref()
            .ok_or_else(agent_approval_unavailable)?;
        let observed_at = self
            .agent_approval_metadata
            .now()
            .map_err(|_| agent_approval_unavailable())?;
        let result = reader
            .execute(
                &active.project,
                task_id,
                observed_at,
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(|_| agent_approval_unavailable())?;
        Ok(AgentApprovalResponseV1::new(match result {
            AgentApprovalLoadResult::TaskNotFound => AgentApprovalResultV1::TaskNotFound,
            AgentApprovalLoadResult::LedgerUnavailable => AgentApprovalResultV1::LedgerUnavailable,
            AgentApprovalLoadResult::GoalRevisionMismatch {
                current_revision,
                ledger_revision,
            } => AgentApprovalResultV1::GoalRevisionMismatch {
                current_revision,
                ledger_revision,
            },
            AgentApprovalLoadResult::ActivityChanged => AgentApprovalResultV1::ActivityChanged,
            AgentApprovalLoadResult::ApprovalUnavailable => AgentApprovalResultV1::Unavailable,
            AgentApprovalLoadResult::Available(approval) => AgentApprovalResultV1::Available {
                approval: Box::new(map_agent_approval_to_v1(&approval)),
            },
        }))
    }

    /// Applies one explicit approval choice using only the exact visible optimistic anchors.
    #[allow(clippy::too_many_arguments)]
    pub async fn control_agent_approval(
        &self,
        task_id: TaskId,
        expected_approval_revision: AgentApprovalRevision,
        expected_ledger_revision: u32,
        expected_ledger_store_version: TaskLedgerStoreVersion,
        action: AgentApprovalControlActionV1,
    ) -> Result<AgentApprovalControlResponseV1, CommandErrorV1> {
        let Some(_operation) = self.try_acquire_agent_task_operation() else {
            return Ok(AgentApprovalControlResponseV1::new(
                AgentApprovalControlResultV1::ActivityChanged,
            ));
        };
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentApprovalControlResponseV1::new(
                AgentApprovalControlResultV1::NoProject,
            ));
        };
        let controller = self
            .agent_approval_control
            .as_ref()
            .ok_or_else(agent_approval_unavailable)?;
        let metadata = self
            .agent_approval_metadata
            .next()
            .map_err(|_| agent_approval_unavailable())?;
        let action = match action {
            AgentApprovalControlActionV1::AllowOnce => AgentApprovalControlAction::AllowOnce,
            AgentApprovalControlActionV1::Deny => AgentApprovalControlAction::Deny,
            AgentApprovalControlActionV1::Continue => AgentApprovalControlAction::Continue,
            AgentApprovalControlActionV1::Revoke => AgentApprovalControlAction::Revoke,
        };
        let result = controller
            .execute(
                &active.project,
                task_id,
                expected_approval_revision,
                expected_ledger_revision,
                expected_ledger_store_version,
                action,
                metadata,
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(|_| agent_approval_unavailable())?;
        let result = match result {
            AgentApprovalControlResult::TaskNotFound => AgentApprovalControlResultV1::TaskNotFound,
            AgentApprovalControlResult::LedgerUnavailable => {
                AgentApprovalControlResultV1::LedgerUnavailable
            }
            AgentApprovalControlResult::GoalRevisionMismatch => {
                AgentApprovalControlResultV1::GoalRevisionMismatch
            }
            AgentApprovalControlResult::ActivityChanged => {
                AgentApprovalControlResultV1::ActivityChanged
            }
            AgentApprovalControlResult::ApprovalUnavailable
            | AgentApprovalControlResult::ActionUnavailable => {
                AgentApprovalControlResultV1::Unavailable
            }
            AgentApprovalControlResult::Applied(outcome) => {
                let (outcome, approval_revision, ledger_store_version, runtime_start) =
                    match outcome {
                        AgentApprovalControlOutcome::GrantStored { approval_revision } => (
                            AgentApprovalControlOutcomeV1::GrantStored,
                            approval_revision,
                            expected_ledger_store_version,
                            None,
                        ),
                        AgentApprovalControlOutcome::Denied {
                            ledger_store_version,
                            approval_revision,
                        } => (
                            AgentApprovalControlOutcomeV1::Denied,
                            approval_revision,
                            ledger_store_version,
                            None,
                        ),
                        AgentApprovalControlOutcome::Revoked { approval_revision } => (
                            AgentApprovalControlOutcomeV1::Revoked,
                            approval_revision,
                            expected_ledger_store_version,
                            None,
                        ),
                        AgentApprovalControlOutcome::ContinueReady { approval_id } => {
                            let revision = TaskLedgerRevision::new(expected_ledger_revision)
                                .map_err(|_| agent_approval_unavailable())?;
                            let runtime = match &self.agent_run_manager {
                                Some(manager) => match manager.start_attempt(
                                    AgentRunExecutionRequest::after_approval(
                                        task_id,
                                        revision,
                                        expected_ledger_store_version,
                                        approval_id,
                                    ),
                                ) {
                                    Ok(()) => AgentApprovalRuntimeStartV1::Queued,
                                    Err(_) => AgentApprovalRuntimeStartV1::Failed,
                                },
                                None => AgentApprovalRuntimeStartV1::Unavailable,
                            };
                            (
                                AgentApprovalControlOutcomeV1::ContinueRequested,
                                expected_approval_revision,
                                expected_ledger_store_version,
                                Some(runtime),
                            )
                        }
                    };
                AgentApprovalControlResultV1::Applied {
                    outcome,
                    approval_revision: approval_revision.get().to_string(),
                    ledger_store_version: ledger_store_version.get().to_string(),
                    runtime_start,
                }
            }
        };
        Ok(AgentApprovalControlResponseV1::new(result))
    }

    /// Inspects current restart-safe controls for the active task-derived Agent run.
    pub async fn query_agent_task_recovery(
        &self,
        task_id: TaskId,
    ) -> Result<AgentTaskRecoveryResponseV1, CommandErrorV1> {
        let Some(_operation) = self.try_acquire_agent_task_operation() else {
            return Ok(AgentTaskRecoveryResponseV1::new(
                AgentTaskRecoveryResultV1::ActivityChanged,
            ));
        };
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentTaskRecoveryResponseV1::new(
                AgentTaskRecoveryResultV1::NoProject,
            ));
        };
        if let Some(manager) = &self.agent_run_manager {
            let runtime = manager.activity();
            if runtime.state().owns_live_worker() && runtime.task_id() == Some(task_id) {
                let target = load_agent_runtime_target(
                    self.agent_activity.as_ref(),
                    &active.project,
                    task_id,
                )
                .await?;
                let result = match target {
                    AgentRuntimeTargetLoad::Expected(result) => result,
                    AgentRuntimeTargetLoad::Available(target) => {
                        AgentTaskRecoveryResultV1::RuntimeOwned {
                            runtime: AgentTaskRuntimeV1::new(
                                target.ledger_revision.get(),
                                target.ledger_store_version.get().to_string(),
                                map_agent_controller_state_to_v1(target.controller_state),
                                map_agent_runtime_state_to_v1(runtime.state())
                                    .ok_or_else(agent_task_control_unavailable)?,
                            ),
                        }
                    }
                };
                return Ok(AgentTaskRecoveryResponseV1::new(result));
            }
        }
        let inspector = self
            .agent_task_recovery
            .as_ref()
            .ok_or_else(agent_task_control_unavailable)?;
        let observed_at = self
            .agent_recovery_metadata
            .now()
            .map_err(|_| agent_task_control_unavailable())?;
        let result = inspector
            .execute(
                &active.project,
                task_id,
                observed_at,
                &DesktopBoundedReadControl::new(),
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(map_agent_task_control_error_to_v1)?;
        let result = map_agent_task_recovery_result_to_v1(result);
        let result = if self.agent_run_manager.as_ref().is_some_and(|manager| {
            let runtime = manager.activity();
            runtime.state() == AgentRunActivityState::Paused && runtime.task_id() == Some(task_id)
        }) {
            match result {
                AgentTaskRecoveryResultV1::Available { recovery } => {
                    AgentTaskRecoveryResultV1::Paused { recovery }
                }
                result => result,
            }
        } else {
            result
        };
        Ok(AgentTaskRecoveryResponseV1::new(result))
    }

    /// Atomically applies one explicit recovery decision against exact visible Ledger anchors.
    pub async fn control_agent_task_run(
        &self,
        task_id: TaskId,
        expected_ledger_revision: u32,
        expected_ledger_store_version: TaskLedgerStoreVersion,
        action: AgentTaskControlActionV1,
    ) -> Result<AgentTaskControlResponseV1, CommandErrorV1> {
        let Some(_operation) = self.try_acquire_agent_task_operation() else {
            return Ok(AgentTaskControlResponseV1::new(
                AgentTaskControlResultV1::ActivityChanged,
            ));
        };
        let active = lock_recovering_poison(&self.active_project).clone();
        let Some(active) = active else {
            return Ok(AgentTaskControlResponseV1::new(
                AgentTaskControlResultV1::NoProject,
            ));
        };
        let runtime = self
            .agent_run_manager
            .as_ref()
            .map(|manager| manager.activity());
        if let (Some(manager), Some(runtime)) = (&self.agent_run_manager, runtime.as_ref())
            && runtime.task_id() == Some(task_id)
            && (runtime.state().owns_live_worker()
                || runtime.state() == AgentRunActivityState::Paused)
        {
            let target =
                load_agent_runtime_target(self.agent_activity.as_ref(), &active.project, task_id)
                    .await?;
            let target = match target {
                AgentRuntimeTargetLoad::Expected(result) => {
                    return Ok(AgentTaskControlResponseV1::new(
                        map_runtime_expected_to_control(result),
                    ));
                }
                AgentRuntimeTargetLoad::Available(target) => target,
            };
            if target.ledger_revision.get() != expected_ledger_revision
                || target.ledger_store_version != expected_ledger_store_version
            {
                return Ok(AgentTaskControlResponseV1::new(
                    AgentTaskControlResultV1::ActivityChanged,
                ));
            }
            match action {
                AgentTaskControlActionV1::Pause if runtime.state().owns_live_worker() => {
                    manager
                        .pause(task_id)
                        .map_err(map_agent_run_manager_error_to_v1)?;
                    return Ok(AgentTaskControlResponseV1::new(
                        AgentTaskControlResultV1::Accepted {
                            outcome: AgentTaskControlAcceptedOutcomeV1::PauseRequested,
                        },
                    ));
                }
                AgentTaskControlActionV1::Cancel if runtime.state().owns_live_worker() => {
                    let revision = TaskLedgerRevision::new(expected_ledger_revision)
                        .map_err(|_| agent_task_control_unavailable())?;
                    manager
                        .cancel_owned_worker(AgentRunExecutionRequest::new(
                            task_id,
                            revision,
                            expected_ledger_store_version,
                        ))
                        .map_err(map_agent_run_manager_error_to_v1)?;
                    return Ok(AgentTaskControlResponseV1::new(
                        AgentTaskControlResultV1::Accepted {
                            outcome: AgentTaskControlAcceptedOutcomeV1::CancelRequested,
                        },
                    ));
                }
                AgentTaskControlActionV1::Resume | AgentTaskControlActionV1::Replan
                    if runtime.state().owns_live_worker() =>
                {
                    return Ok(AgentTaskControlResponseV1::new(
                        AgentTaskControlResultV1::ActivityChanged,
                    ));
                }
                AgentTaskControlActionV1::Pause => {
                    return Ok(AgentTaskControlResponseV1::new(
                        AgentTaskControlResultV1::ActivityChanged,
                    ));
                }
                AgentTaskControlActionV1::Resume
                | AgentTaskControlActionV1::Replan
                | AgentTaskControlActionV1::Cancel => {}
            }
        } else if action == AgentTaskControlActionV1::Pause {
            return Ok(AgentTaskControlResponseV1::new(
                AgentTaskControlResultV1::ActivityChanged,
            ));
        }
        let controller = self
            .agent_task_control
            .as_ref()
            .ok_or_else(agent_task_control_unavailable)?;
        let event_id = self
            .agent_recovery_metadata
            .next_event_id()
            .map_err(|_| agent_task_control_unavailable())?;
        let observed_at = self
            .agent_recovery_metadata
            .now()
            .map_err(|_| agent_task_control_unavailable())?;
        let choice = match action {
            AgentTaskControlActionV1::Pause => {
                return Ok(AgentTaskControlResponseV1::new(
                    AgentTaskControlResultV1::ActivityChanged,
                ));
            }
            AgentTaskControlActionV1::Resume => AgentRecoveryChoice::Resume,
            AgentTaskControlActionV1::Replan => AgentRecoveryChoice::Replan,
            AgentTaskControlActionV1::Cancel => AgentRecoveryChoice::Cancel,
        };
        let result = controller
            .execute(
                &active.project,
                task_id,
                expected_ledger_revision,
                expected_ledger_store_version,
                choice,
                event_id,
                observed_at,
                &DesktopBoundedReadControl::new(),
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(map_agent_task_control_error_to_v1)?;
        let runtime_start = match &result {
            AgentTaskControlResult::Applied {
                outcome:
                    AgentRecoveryOutcomeKind::Resumed | AgentRecoveryOutcomeKind::ReplanRequired,
                ledger_store_version,
                ..
            } => Some(match &self.agent_run_manager {
                Some(manager) => {
                    let revision = TaskLedgerRevision::new(expected_ledger_revision)
                        .map_err(|_| agent_task_control_unavailable())?;
                    match manager.start_attempt(AgentRunExecutionRequest::new(
                        task_id,
                        revision,
                        *ledger_store_version,
                    )) {
                        Ok(()) => AgentTaskRuntimeStartV1::Queued,
                        Err(_) => AgentTaskRuntimeStartV1::Failed,
                    }
                }
                None => AgentTaskRuntimeStartV1::Unavailable,
            }),
            _ => None,
        };
        if matches!(
            result,
            AgentTaskControlResult::Applied {
                outcome: AgentRecoveryOutcomeKind::Cancelled,
                ..
            }
        ) && runtime.as_ref().is_some_and(|runtime| {
            runtime.task_id() == Some(task_id) && runtime.state() == AgentRunActivityState::Paused
        }) && let Some(manager) = &self.agent_run_manager
        {
            let _cleared = manager.complete_external_cancel(task_id);
        }
        Ok(AgentTaskControlResponseV1::new(
            map_agent_task_control_result_to_v1(result, runtime_start),
        ))
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
    pub fn query_deep_map_status(&self) -> DeepMapStatusResponseV2 {
        if lock_recovering_poison(&self.active_project).is_none() {
            return DeepMapStatusResponseV2::no_project();
        }
        let Some(manager) = &self.deep_map_manager else {
            return DeepMapStatusResponseV2::unavailable();
        };
        let Some(model) = manager.model() else {
            return DeepMapStatusResponseV2::unavailable();
        };
        DeepMapStatusResponseV2::available(
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
            map_deep_map_activity_to_v2(manager.activity()),
        )
    }

    /// Returns the compact V3 lifecycle after a Core-owned current-index preflight.
    pub async fn query_deep_map_status_v3(&self) -> DeepMapStatusResponseV3 {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return DeepMapStatusResponseV3::no_project();
        };
        if self.ensure_deep_map_runtime_available().await.is_err() {
            return DeepMapStatusResponseV3::unavailable();
        }
        let (Some(manager), Some(publication_store)) = (
            self.deep_map_manager.as_ref(),
            self.deep_map_publication_state.as_ref(),
        ) else {
            return DeepMapStatusResponseV3::unavailable();
        };
        let Some(model) = manager.model() else {
            return DeepMapStatusResponseV3::unavailable();
        };
        let publication = match publication_store
            .load_deep_map_publication_state(&active.project)
            .await
        {
            Ok(publication) => publication,
            Err(_) => {
                let lifecycle = publication_read_failure_lifecycle(map_deep_map_lifecycle_to_v3(
                    &manager.activity(),
                ));
                return DeepMapStatusResponseV3::available(
                    map_deep_map_model_to_v1(&model),
                    lifecycle,
                );
            }
        };
        let activity = manager.activity();
        let lifecycle = match publication {
            DeepMapPublicationState::Current { anchor, card_count } => {
                let details_available = if let Some(journal) = self.deep_map_journal.as_ref() {
                    journal
                        .list_runs(&active.project, None)
                        .await
                        .ok()
                        .is_some_and(|page| {
                            page.runs().iter().any(|run| run.start().anchor() == anchor)
                        })
                } else {
                    false
                };
                DeepMapLifecycleV3::Current {
                    card_count: card_count.to_string(),
                    details_available,
                }
            }
            DeepMapPublicationState::Ready(_) => map_deep_map_lifecycle_to_v3(&activity),
            DeepMapPublicationState::NoPublishedIndex => {
                if activity.state() == DeepMapActivityState::Failed {
                    map_deep_map_lifecycle_to_v3(&activity)
                } else {
                    DeepMapLifecycleV3::Ready
                }
            }
        };
        DeepMapStatusResponseV3::available(map_deep_map_model_to_v1(&model), lifecycle)
    }

    /// Starts only one of the three Core-owned modes and returns before model work when current.
    pub async fn start_deep_map_v2(
        &self,
        mode: DeepMapMode,
    ) -> Result<DeepMapStartResponseV2, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        self.ensure_deep_map_runtime_available().await?;
        let manager = self
            .deep_map_manager
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let publication = self
            .deep_map_publication_state
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?
            .load_deep_map_publication_state(&active.project)
            .await
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        match publication {
            DeepMapPublicationState::Current { .. } => {
                Ok(DeepMapStartResponseV2::already_current())
            }
            DeepMapPublicationState::Ready(anchor) => {
                manager
                    .start_current_mapping(mode, anchor)
                    .map_err(map_deep_map_control_error)?;
                Ok(DeepMapStartResponseV2::queued())
            }
            DeepMapPublicationState::NoPublishedIndex => {
                Err(CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))
            }
        }
    }

    /// Reads the newest twenty durable Deep-Map runs for the active project.
    pub async fn query_deep_map_runs(
        &self,
        cursor: Option<&str>,
    ) -> Result<DeepMapRunPageResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let cursor = cursor
            .map(|value| decode_deep_map_run_cursor(active.project.worktree().id(), value))
            .transpose()
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        if let Some(cursor) = cursor {
            let sequence = DeepMapEventSequence::new(1)
                .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
            let valid = journal
                .load_entry(&active.project, cursor.run_id(), sequence)
                .await
                .ok()
                .flatten()
                .is_some_and(|detail| detail.run().updated_at() == cursor.updated_at());
            if !valid {
                return Err(CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable));
            }
        }
        let page = journal
            .list_runs(&active.project, cursor)
            .await
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        Ok(map_deep_map_run_page_to_v1(
            active.project.worktree().id(),
            &page,
        ))
    }

    /// Reads one bounded chronological event page after validating its run and cursor.
    pub async fn query_deep_map_entries(
        &self,
        run_selection: &str,
        cursor: Option<&str>,
    ) -> Result<DeepMapEntryPageResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let run_id = decode_deep_map_run_selection(active.project.worktree().id(), run_selection)
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let before = cursor
            .map(|value| {
                decode_deep_map_entry_selection(active.project.worktree().id(), run_id, value)
            })
            .transpose()
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        if let Some(sequence) = before
            && journal
                .load_entry(&active.project, run_id, sequence)
                .await
                .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?
                .is_none()
        {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable));
        }
        let page = journal
            .list_entries(&active.project, run_id, before)
            .await
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        Ok(map_deep_map_entry_page_to_v1(
            active.project.worktree().id(),
            run_id,
            &page,
        ))
    }

    /// Reads exactly one safe event detail for a project-bound Core-issued selection.
    pub async fn query_deep_map_entry_detail(
        &self,
        run_selection: &str,
        entry_selection: &str,
    ) -> Result<DeepMapEntryDetailResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let run_id = decode_deep_map_run_selection(active.project.worktree().id(), run_selection)
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let sequence = decode_deep_map_entry_selection(
            active.project.worktree().id(),
            run_id,
            entry_selection,
        )
        .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        let detail = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?
            .load_entry(&active.project, run_id, sequence)
            .await
            .map_err(|_| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?
            .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
        map_deep_map_entry_detail_to_v1(active.project.worktree().id(), &detail)
    }

    /// Reads the user-facing five-phase dashboard without provider or budget metadata.
    pub async fn query_deep_map_run_dashboard(
        &self,
        run_selection: &str,
    ) -> Result<DeepMapRunDashboardResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let worktree_id = active.project.worktree().id();
        let run_id = decode_deep_map_run_selection(worktree_id, run_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let run = journal
            .load_run(&active.project, run_id)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let latest_event = journal
            .list_entries(&active.project, run_id, None)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .entries()
            .last()
            .copied();
        let index = self.load_deep_map_dashboard_index(&active.project).await?;
        let current_anchor = index.as_ref().map(|value| {
            a3_application::DeepMapPublicationAnchor::new(
                value.run().id(),
                value.run().snapshot_id(),
            )
        });
        let dashboard =
            a3_application::DeepMapRunDashboard::derive(&run, latest_event, current_anchor);
        let current_activity =
            if dashboard.freshness() == a3_application::DeepMapDashboardFreshness::Current {
                self.map_deep_map_current_activity(
                    &active.project,
                    journal.as_ref(),
                    run_id,
                    dashboard.activity(),
                    index.as_ref(),
                )
                .await?
            } else {
                dashboard
                    .activity()
                    .map(|activity| DeepMapCurrentActivityV1 {
                        phase: map_dashboard_phase(activity.phase()),
                        action: activity.action().map(map_deep_map_safe_action_to_v2),
                        target_kind: activity.target_kind().map(map_deep_map_target_kind_to_v2),
                        module_name: None,
                        target_label: None,
                        selection_reason: None,
                        card_fields: Vec::new(),
                    })
            };
        let historical_plan_limited = first_plan_step(journal.as_ref(), &active.project, run_id)
            .await?
            .is_some_and(|step| step.target_reference().is_none());
        Ok(DeepMapRunDashboardResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            run_selection: run_selection.to_owned(),
            state: map_dashboard_state(dashboard.state()),
            freshness: map_dashboard_freshness(dashboard.freshness()),
            phases: dashboard
                .phases()
                .iter()
                .copied()
                .map(|phase| DeepMapDashboardPhaseProgressV1 {
                    phase: map_dashboard_phase(phase.phase()),
                    state: map_dashboard_phase_state(phase.state()),
                })
                .collect(),
            confirmed_steps: dashboard.confirmed_steps().to_string(),
            total_steps: dashboard.total_steps().to_string(),
            started_at_unix_millis: dashboard.started_at().unix_millis().to_string(),
            updated_at_unix_millis: dashboard.updated_at().unix_millis().to_string(),
            current_activity,
            failure: dashboard
                .diagnostic()
                .map(|diagnostic| DeepMapDashboardFailureV1 {
                    cause: map_deep_map_diagnostic_to_v3(diagnostic),
                    confirmed_work_retained: dashboard.confirmed_steps() > 0,
                    diagnostic_code: Some(deep_map_diagnostic_code(diagnostic).to_owned()),
                }),
            details_incomplete: dashboard.details_incomplete(),
            historical_plan_limited,
        })
    }

    /// Reads at most twenty module summaries with Core-derived product states.
    pub async fn query_deep_map_run_modules(
        &self,
        run_selection: &str,
        cursor: Option<&str>,
    ) -> Result<DeepMapRunModulesResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let worktree_id = active.project.worktree().id();
        let run_id = decode_deep_map_run_selection(worktree_id, run_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let run = journal
            .load_run(&active.project, run_id)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let cursor = cursor
            .map(|value| decode_deep_map_module_cursor(worktree_id, run_id, value))
            .transpose()
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let page = journal
            .list_run_modules(&active.project, run_id, cursor)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let latest_event = journal
            .list_entries(&active.project, run_id, None)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .entries()
            .last()
            .copied();
        let index = self.load_deep_map_dashboard_index(&active.project).await?;
        let is_current = index.as_ref().is_some_and(|value| {
            value.run().id() == run.start().anchor().index_run_id()
                && value.run().snapshot_id() == run.start().anchor().snapshot_id()
        });
        let mut modules = Vec::with_capacity(page.modules().len());
        for (page_index, module) in page.modules().iter().copied().enumerate() {
            let card_available = if is_current {
                self.current_card_for_run(&active.project, &run, module.module_id())
                    .await?
                    .is_some()
            } else {
                false
            };
            let display_name = index
                .as_ref()
                .filter(|_| is_current)
                .and_then(|value| deep_map_module_display_name(value, module.module_id()))
                .unwrap_or_else(|| format!("Historisches Modul {}", page_index + 1));
            modules.push(DeepMapRunModuleV1 {
                selection: encode_deep_map_module_selection(
                    worktree_id,
                    run_id,
                    module.module_id(),
                ),
                display_name,
                state: map_dashboard_module_state(a3_application::derive_deep_map_module_state(
                    &run,
                    latest_event,
                    module,
                    card_available,
                )),
                planned_steps: module.planned_steps().to_string(),
                confirmed_steps: module.confirmed_steps().to_string(),
                card_available,
            });
        }
        Ok(DeepMapRunModulesResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            modules,
            next_cursor: page
                .next_cursor()
                .map(|value| encode_deep_map_module_cursor(worktree_id, run_id, value)),
        })
    }

    /// Reads at most fifty safe, understandable exploration targets for one module.
    pub async fn query_deep_map_module_steps(
        &self,
        run_selection: &str,
        module_selection: &str,
        cursor: Option<&str>,
    ) -> Result<DeepMapModuleStepsResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let worktree_id = active.project.worktree().id();
        let run_id = decode_deep_map_run_selection(worktree_id, run_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let module_id = decode_deep_map_module_selection(worktree_id, run_id, module_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let after_position = cursor
            .map(|value| decode_deep_map_step_cursor(worktree_id, run_id, module_id, value))
            .transpose()
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let run = journal
            .load_run(&active.project, run_id)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let page = journal
            .list_module_steps(&active.project, run_id, module_id, after_position)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let latest_event = journal
            .list_entries(&active.project, run_id, None)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .entries()
            .last()
            .copied();
        let index = self.load_deep_map_dashboard_index(&active.project).await?;
        let is_current = index.as_ref().is_some_and(|value| {
            value.run().id() == run.start().anchor().index_run_id()
                && value.run().snapshot_id() == run.start().anchor().snapshot_id()
        });
        let historical_details_limited = page
            .steps()
            .iter()
            .any(|step| step.target_reference().is_none());
        Ok(DeepMapModuleStepsResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            steps: page
                .steps()
                .iter()
                .map(|step| DeepMapModuleStepV1 {
                    position: step.position().to_string(),
                    target_kind: map_deep_map_target_kind_to_v2(step.target_kind()),
                    target_label: index
                        .as_ref()
                        .filter(|_| is_current)
                        .and_then(|value| resolve_deep_map_target_label(value, step)),
                    selection_reason: map_deep_map_selection_reason(step.seed_reason()),
                    card_fields: step.coverage_fields().map(|fields| {
                        fields
                            .iter()
                            .copied()
                            .map(map_deep_map_card_field)
                            .collect()
                    }),
                    state: if step.confirmed() {
                        DeepMapPlanStepStateV1::Confirmed
                    } else if latest_event.is_some_and(|event| {
                        event.module_id() == Some(module_id)
                            && event.step_position() == Some(step.position())
                            && matches!(
                                run.state(),
                                a3_domain::DeepMapRunState::Running
                                    | a3_domain::DeepMapRunState::Pausing
                                    | a3_domain::DeepMapRunState::Paused
                            )
                    }) {
                        DeepMapPlanStepStateV1::Exploring
                    } else {
                        DeepMapPlanStepStateV1::Planned
                    },
                })
                .collect(),
            next_cursor: page.next_after_position().map(|position| {
                encode_deep_map_step_cursor(worktree_id, run_id, module_id, position)
            }),
            historical_details_limited,
        })
    }

    /// Reads exact current verified Card evidence projected onto Atlas entities.
    pub async fn query_deep_map_atlas_impact(
        &self,
        run_selection: &str,
        module_selection: &str,
        cursor: Option<&str>,
    ) -> Result<DeepMapAtlasImpactResponseV1, CommandErrorV1> {
        let Some(active) = lock_recovering_poison(&self.active_project).clone() else {
            return Err(CommandErrorV1::deep_map(ErrorCodeV1::NoActiveProject));
        };
        let worktree_id = active.project.worktree().id();
        let run_id = decode_deep_map_run_selection(worktree_id, run_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let module_id = decode_deep_map_module_selection(worktree_id, run_id, module_selection)
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let offset = cursor
            .map(|value| decode_deep_map_impact_cursor(worktree_id, run_id, module_id, value))
            .transpose()
            .map_err(|_| deep_map_dashboard_unavailable())?
            .unwrap_or(0);
        let journal = self
            .deep_map_journal
            .as_ref()
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let run = journal
            .load_run(&active.project, run_id)
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?
            .ok_or_else(deep_map_dashboard_unavailable)?;
        let index = self.load_deep_map_dashboard_index(&active.project).await?;
        let Some(index) = index.filter(|value| {
            value.run().id() == run.start().anchor().index_run_id()
                && value.run().snapshot_id() == run.start().anchor().snapshot_id()
        }) else {
            return Ok(DeepMapAtlasImpactResponseV1 {
                protocol_version: ProtocolVersion::CURRENT,
                result: DeepMapAtlasImpactResultV1::Historical,
            });
        };
        let Some(card) = self
            .current_card_for_run(&active.project, &run, module_id)
            .await?
        else {
            return Ok(DeepMapAtlasImpactResponseV1 {
                protocol_version: ProtocolVersion::CURRENT,
                result: DeepMapAtlasImpactResultV1::CardUnavailable,
            });
        };
        let projection = build_deep_map_atlas_impact(&index, &card)?;
        let start = usize::try_from(offset).map_err(|_| deep_map_dashboard_unavailable())?;
        if start > projection.items.len() {
            return Err(deep_map_dashboard_unavailable());
        }
        let end = start.saturating_add(50).min(projection.items.len());
        let next_cursor = (end < projection.items.len()).then(|| {
            encode_deep_map_impact_cursor(
                worktree_id,
                run_id,
                module_id,
                u64::try_from(end).unwrap_or(u64::MAX),
            )
        });
        Ok(DeepMapAtlasImpactResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapAtlasImpactResultV1::Available {
                summary: projection.summary,
                items: projection.items[start..end].to_vec(),
                next_cursor,
            },
        })
    }

    async fn load_deep_map_dashboard_index(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Option<a3_domain::PublishedIndex>, CommandErrorV1> {
        let Some(store) = self.deep_map_dashboard_index.as_ref() else {
            return Ok(None);
        };
        store
            .latest_published_index(project, &DesktopBoundedReadControl::new())
            .await
            .map_err(|_| deep_map_dashboard_unavailable())
    }

    async fn current_card_for_run(
        &self,
        project: &ProjectIdentity,
        run: &DeepMapRunSummary,
        module_id: ModuleId,
    ) -> Result<Option<Box<ModuleCardDetail>>, CommandErrorV1> {
        let Some(reader) = self.module_card_detail.as_ref() else {
            return Ok(None);
        };
        let result = reader
            .execute(
                project,
                &ModuleCardDetailQuery::new(module_id),
                &DesktopBoundedReadControl::new(),
            )
            .await
            .map_err(|_| deep_map_dashboard_unavailable())?;
        let ModuleCardDetailLoadResult::Detail(detail) = result else {
            return Ok(None);
        };
        Ok((matches!(detail.lifecycle(), ModuleCardLifecycle::Current)
            && detail.source_index_run_id() == run.start().anchor().index_run_id()
            && detail.source_snapshot_id() == run.start().anchor().snapshot_id())
        .then_some(detail))
    }

    async fn map_deep_map_current_activity(
        &self,
        project: &ProjectIdentity,
        journal: &dyn DeepMapRunJournalStore,
        run_id: DeepMapRunId,
        activity: Option<a3_application::DeepMapDashboardActivity>,
        index: Option<&a3_domain::PublishedIndex>,
    ) -> Result<Option<DeepMapCurrentActivityV1>, CommandErrorV1> {
        let Some(activity) = activity else {
            return Ok(None);
        };
        let step = if let (Some(module_id), Some(position)) =
            (activity.module_id(), activity.step_position())
        {
            journal
                .list_module_steps(project, run_id, module_id, position.checked_sub(1))
                .await
                .map_err(|_| deep_map_dashboard_unavailable())?
                .steps()
                .first()
                .filter(|step| step.position() == position)
                .cloned()
        } else {
            None
        };
        Ok(Some(DeepMapCurrentActivityV1 {
            phase: map_dashboard_phase(activity.phase()),
            action: activity.action().map(map_deep_map_safe_action_to_v2),
            target_kind: activity.target_kind().map(map_deep_map_target_kind_to_v2),
            module_name: activity.module_id().and_then(|module_id| {
                index.and_then(|value| deep_map_module_display_name(value, module_id))
            }),
            target_label: step.as_ref().and_then(|step| {
                index.and_then(|value| resolve_deep_map_target_label(value, step))
            }),
            selection_reason: step
                .as_ref()
                .map(|step| map_deep_map_selection_reason(step.seed_reason())),
            card_fields: step
                .as_ref()
                .and_then(|step| step.coverage_fields())
                .unwrap_or_default()
                .iter()
                .copied()
                .map(map_deep_map_card_field)
                .collect(),
        }))
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
        let _agent_operation = self
            .try_acquire_agent_task_operation()
            .ok_or_else(|| CommandErrorV1::project_removal(ErrorCodeV1::ProjectOperationBusy))?;
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
        if let Some(agent_run_manager) = &self.agent_run_manager
            && agent_run_manager.deactivate_project().is_err()
        {
            if let Some(deep_map_manager) = &self.deep_map_manager {
                let _restored = deep_map_manager.activate_project(active.project.clone());
            }
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
            if let Some(agent_run_manager) = &self.agent_run_manager
                && agent_run_manager
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
        self.agent_inspection.deactivate_project();
        self.agent_approval.deactivate_project();
        Ok(RemoveProjectResponseV1::removed())
    }

    /// Removes one exact catalog row; active removal first performs the owned shutdown path.
    pub async fn remove_catalog_project(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
        let _operation = self.acquire_project_operation(CommandErrorV1::project_removal)?;
        let _agent_operation = self
            .try_acquire_agent_task_operation()
            .ok_or_else(|| CommandErrorV1::project_removal(ErrorCodeV1::ProjectOperationBusy))?;
        let remove = self.remove_project.as_ref().ok_or_else(|| {
            CommandErrorV1::project_removal(ErrorCodeV1::ProjectRemovalUnavailable)
        })?;
        let active = lock_recovering_poison(&self.active_project).clone();
        let removing_active = active
            .as_ref()
            .is_some_and(|active| active.project.worktree().id() == worktree_id);
        if removing_active {
            let active = active.as_ref().ok_or_else(|| {
                CommandErrorV1::project_removal(ErrorCodeV1::ProjectRemovalUnavailable)
            })?;
            self.deactivate_active_runtime(active)?;
        }
        if let Err(error) = remove.execute_catalog(worktree_id).await {
            if removing_active && let Some(active) = &active {
                self.restore_active_runtime(active);
            }
            return Err(map_project_removal_error_to_v1(error));
        }
        if removing_active {
            *lock_recovering_poison(&self.active_project) = None;
            self.agent_inspection.deactivate_project();
            self.agent_approval.deactivate_project();
        }
        Ok(RemoveProjectResponseV1::removed())
    }

    fn activate_project_runtime(
        &self,
        project: ProjectIdentity,
        project_id: ProjectId,
        error: fn(ErrorCodeV1) -> CommandErrorV1,
    ) -> Result<(), CommandErrorV1> {
        if self
            .agent_sessions
            .as_ref()
            .is_some_and(|manager| manager.quiesce().is_err())
        {
            return Err(error(ErrorCodeV1::ProjectOperationBusy));
        }
        let previous = lock_recovering_poison(&self.active_project).clone();
        if let Some(manager) = &self.index_manager
            && manager.activate_project(project.clone()).is_err()
        {
            self.restore_runtime_after_failed_activation(previous.as_ref());
            return Err(error(ErrorCodeV1::LocalStorageUnavailable));
        }
        if let Some(manager) = &self.deep_map_manager
            && manager.activate_project(project.clone()).is_err()
        {
            self.restore_runtime_after_failed_activation(previous.as_ref());
            return Err(error(ErrorCodeV1::DeepMapUnavailable));
        }
        if let Some(manager) = &self.agent_run_manager
            && manager.activate_project(project.clone()).is_err()
        {
            self.restore_runtime_after_failed_activation(previous.as_ref());
            return Err(error(ErrorCodeV1::AgentTaskControlUnavailable));
        }
        self.agent_inspection.activate_project(&project);
        self.agent_approval.activate_project(&project);
        *lock_recovering_poison(&self.active_project) = Some(ActiveProject {
            project_id,
            project,
        });
        Ok(())
    }

    async fn activate_validated_catalog_project(
        &self,
        project: &ProjectIdentity,
        project_id: ProjectId,
    ) -> Result<(), CommandErrorV1> {
        let previous = lock_recovering_poison(&self.active_project).clone();
        self.activate_project_runtime(project.clone(), project_id, CommandErrorV1::project_open)?;
        let recorded_project_id = match self
            .project_catalog_store
            .record_opened_project(project)
            .await
        {
            Ok(recorded_project_id) => recorded_project_id,
            Err(error) => {
                self.rollback_catalog_activation(previous.as_ref());
                return Err(CommandErrorV1::project_open(map_storage_error_to_v1(error)));
            }
        };
        if recorded_project_id != project_id {
            self.rollback_catalog_activation(previous.as_ref());
            return Err(CommandErrorV1::project_open(
                ErrorCodeV1::ProjectIdentityConflict,
            ));
        }
        Ok(())
    }

    fn rollback_catalog_activation(&self, previous: Option<&ActiveProject>) {
        self.restore_runtime_after_failed_activation(previous);
        match previous {
            Some(previous) => {
                self.agent_inspection.activate_project(&previous.project);
                self.agent_approval.activate_project(&previous.project);
                *lock_recovering_poison(&self.active_project) = Some(previous.clone());
            }
            None => {
                self.agent_inspection.deactivate_project();
                self.agent_approval.deactivate_project();
                *lock_recovering_poison(&self.active_project) = None;
            }
        }
    }

    fn restore_runtime_after_failed_activation(&self, previous: Option<&ActiveProject>) {
        match previous {
            Some(previous) => self.restore_active_runtime(previous),
            None => {
                if let Some(manager) = &self.agent_run_manager {
                    let _deactivated = manager.deactivate_project();
                }
                if let Some(manager) = &self.deep_map_manager {
                    let _deactivated = manager.deactivate_project();
                }
                if let Some(manager) = &self.index_manager {
                    let _deactivated = manager.deactivate_project();
                }
            }
        }
    }

    fn restore_active_runtime(&self, active: &ActiveProject) {
        if let Some(manager) = &self.index_manager {
            let _restored = manager.activate_project(active.project.clone());
        }
        if let Some(manager) = &self.deep_map_manager {
            let _restored = manager.activate_project(active.project.clone());
        }
        if let Some(manager) = &self.agent_run_manager {
            let _restored = manager.activate_project(active.project.clone());
        }
    }

    fn deactivate_active_runtime(&self, active: &ActiveProject) -> Result<(), CommandErrorV1> {
        if self
            .agent_sessions
            .as_ref()
            .is_some_and(|manager| manager.quiesce().is_err())
        {
            return Err(CommandErrorV1::project_removal(
                ErrorCodeV1::ProjectOperationBusy,
            ));
        }
        if let Some(manager) = &self.index_manager
            && let Err(error) = manager.deactivate_project()
        {
            self.restore_active_runtime(active);
            return Err(map_deactivation_error_to_v1(error));
        }
        if let Some(manager) = &self.deep_map_manager
            && manager.deactivate_project().is_err()
        {
            self.restore_active_runtime(active);
            return Err(CommandErrorV1::project_removal(
                ErrorCodeV1::ProjectRemovalUnavailable,
            ));
        }
        if let Some(manager) = &self.agent_run_manager
            && manager.deactivate_project().is_err()
        {
            self.restore_active_runtime(active);
            return Err(CommandErrorV1::project_removal(
                ErrorCodeV1::ProjectRemovalUnavailable,
            ));
        }
        Ok(())
    }

    fn acquire_project_operation(
        &self,
        error: fn(ErrorCodeV1) -> CommandErrorV1,
    ) -> Result<ExclusiveOperationPermit<'_>, CommandErrorV1> {
        self.project_operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ExclusiveOperationPermit {
                active: &self.project_operation_active,
            })
            .map_err(|_| error(ErrorCodeV1::ProjectOperationBusy))
    }

    fn try_acquire_agent_task_operation(&self) -> Option<ExclusiveOperationPermit<'_>> {
        self.agent_task_operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| ExclusiveOperationPermit {
                active: &self.agent_task_operation_active,
            })
    }
}

struct ExclusiveOperationPermit<'a> {
    active: &'a AtomicBool,
}

fn map_agent_session_detail_to_v1(detail: &AgentSessionDetail) -> AgentSessionV1 {
    AgentSessionV1::new(
        map_agent_session_summary_to_v1(detail.session()),
        detail
            .session()
            .active_work_item()
            .map(|work_item| work_item.task_id().to_string()),
        detail
            .entries()
            .iter()
            .map(map_agent_session_entry_to_v1)
            .collect(),
        detail.has_older_entries(),
    )
}

fn map_agent_session_summary_to_v1(session: &AgentSession) -> AgentSessionSummaryV1 {
    AgentSessionSummaryV1::new(
        session.id().to_string(),
        session.revision().get().to_string(),
        session.title().as_str().to_owned(),
        map_agent_session_mode_to_v1(session.mode()),
        map_agent_session_state_to_v1(session.state()),
        session.updated_at().unix_millis().to_string(),
        session.current_plan_revision(),
    )
}

fn map_agent_session_entry_to_v1(entry: &AgentSessionEntry) -> AgentSessionEntryV1 {
    AgentSessionEntryV1::new(
        entry.sequence().get().to_string(),
        match entry.kind() {
            AgentSessionEntryKind::UserMessage => AgentSessionEntryKindV1::UserMessage,
            AgentSessionEntryKind::AssistantSummary => AgentSessionEntryKindV1::AssistantSummary,
            AgentSessionEntryKind::Plan => AgentSessionEntryKindV1::Plan,
            AgentSessionEntryKind::FinalReport => AgentSessionEntryKindV1::FinalReport,
            AgentSessionEntryKind::Activity => AgentSessionEntryKindV1::Activity,
        },
        entry.text().as_str().to_owned(),
        entry.created_at().unix_millis().to_string(),
        entry.plan_revision(),
    )
}

const fn map_agent_session_mode_to_v1(mode: AgentSessionMode) -> AgentSessionModeV1 {
    match mode {
        AgentSessionMode::Ask => AgentSessionModeV1::Ask,
        AgentSessionMode::Plan => AgentSessionModeV1::Plan,
        AgentSessionMode::Agent => AgentSessionModeV1::Agent,
    }
}

const fn map_agent_session_state_to_v1(state: AgentSessionState) -> AgentSessionStateV1 {
    match state {
        AgentSessionState::Draft => AgentSessionStateV1::Draft,
        AgentSessionState::Running => AgentSessionStateV1::Running,
        AgentSessionState::AwaitingUser => AgentSessionStateV1::AwaitingUser,
        AgentSessionState::AwaitingPlanReview => AgentSessionStateV1::AwaitingPlanReview,
        AgentSessionState::AwaitingApproval => AgentSessionStateV1::AwaitingApproval,
        AgentSessionState::Paused => AgentSessionStateV1::Paused,
        AgentSessionState::Completed => AgentSessionStateV1::Completed,
        AgentSessionState::Failed => AgentSessionStateV1::Failed,
        AgentSessionState::Cancelled => AgentSessionStateV1::Cancelled,
        AgentSessionState::Archived => AgentSessionStateV1::Archived,
    }
}

fn map_agent_session_failure(error: AgentSessionManagerFailure) -> CommandErrorV1 {
    let code = match error {
        AgentSessionManagerFailure::InvalidInput | AgentSessionManagerFailure::InvalidOutput => {
            ErrorCodeV1::InvalidAgentSessionRequest
        }
        AgentSessionManagerFailure::Conflict => ErrorCodeV1::AgentSessionRevisionConflict,
        AgentSessionManagerFailure::Busy => ErrorCodeV1::AgentSessionBusy,
        AgentSessionManagerFailure::NotFound | AgentSessionManagerFailure::Unavailable => {
            ErrorCodeV1::AgentSessionUnavailable
        }
    };
    CommandErrorV1::agent_session(code)
}

fn map_ui_preferences_to_v1(
    stored: a3_application::StoredUiPreferences,
) -> UiPreferencesResponseV1 {
    let layout = stored.agent_workspace();
    UiPreferencesResponseV1::new(
        stored.version().get().to_string(),
        layout.session_rail_width(),
        layout.inspector_width(),
        layout.session_rail_collapsed(),
        layout.inspector_collapsed(),
    )
}

fn map_ui_preferences_failure(error: UiPreferencesError) -> CommandErrorV1 {
    let code = match error {
        UiPreferencesError::InvalidLayout | UiPreferencesError::InvalidVersion => {
            ErrorCodeV1::InvalidAgentSessionRequest
        }
        UiPreferencesError::Conflict => ErrorCodeV1::AgentSessionRevisionConflict,
        UiPreferencesError::Unavailable => ErrorCodeV1::AgentSessionUnavailable,
    };
    CommandErrorV1::agent_session(code)
}

impl Drop for ExclusiveOperationPermit<'_> {
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

impl AgentControllerControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
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

impl ProjectMapSourcePreviewControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: Progress,
    ) -> Result<(), ProjectMapSourcePreviewControlError> {
        let completed = progress
            .completed()
            .ok_or(ProjectMapSourcePreviewControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ProjectMapSourcePreviewControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ProjectMapSourcePreviewControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ProjectMapSceneControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapSceneControlError> {
        let completed = progress
            .completed()
            .ok_or(ProjectMapSceneControlError::Unavailable)?;
        let total = progress
            .total()
            .ok_or(ProjectMapSceneControlError::Unavailable)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ProjectMapSceneControlError::Unavailable);
        }
        self.total.store(total, Ordering::Release);
        self.completed.store(completed, Ordering::Release);
        Ok(())
    }
}

impl ProjectMapAtlasControl for DesktopBoundedReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapAtlasControlError> {
        let completed = progress.completed().ok_or(ProjectMapAtlasControlError)?;
        let total = progress.total().ok_or(ProjectMapAtlasControlError)?;
        let previous_completed = self.completed.load(Ordering::Acquire);
        let previous_total = self.total.load(Ordering::Acquire);
        if completed < previous_completed || (previous_total != 0 && total != previous_total) {
            return Err(ProjectMapAtlasControlError);
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
    settings_store: Option<Arc<dyn a3_application::DesktopSettingsStore>>,
    credential_store: Option<Arc<dyn a3_application::ProviderCredentialStore>>,
    agent_session_store: Option<Arc<dyn AgentSessionStore>>,
    ui_preferences_store: Option<Arc<dyn UiPreferencesStore>>,
    command_allowlist_store: Option<Arc<dyn a3_application::CommandAllowlistStore>>,
    project_ignore_settings_source: Option<Arc<dyn a3_application::ProjectIgnoreSettingsSource>>,
    index_store: Option<Arc<dyn KnowledgeIndexStore>>,
    module_card_freshness_store: Option<Arc<dyn ModuleCardFreshnessStore>>,
    module_card_detail_store: Option<Arc<dyn ModuleCardDetailStore>>,
    module_card_evidence_store: Option<Arc<dyn ModuleCardEvidenceStore>>,
    project_map_scene_store: Option<Arc<dyn ProjectMapSceneStore>>,
    project_map_atlas_store: Option<Arc<dyn ProjectMapAtlasStore>>,
    module_dependency_graph_store: Option<Arc<dyn ModuleDependencyGraphStore>>,
    module_runtime_store: Option<Arc<dyn ModuleRuntimeStore>>,
    knowledge_search_store: Option<Arc<dyn KnowledgeSearchStore>>,
    task_lens_index_store: Option<Arc<dyn TaskLensIndexStore>>,
    task_lens_claim_store: Option<Arc<dyn TaskLensClaimStore>>,
    task_lens_workspace_store: Option<Arc<dyn TaskLensWorkspaceStore>>,
    verification_evidence_store: Option<Arc<dyn VerificationEvidenceStore>>,
    run_journal_store: Option<Arc<dyn RunJournalStore>>,
    policy_store: Option<Arc<dyn PolicyStore>>,
    agent_action_store: Option<Arc<dyn AgentActionStore>>,
    task_ledger_store: Option<Arc<dyn TaskLedgerStore>>,
    agent_recovery_store: Option<Arc<dyn AgentRecoveryStore>>,
    goal_contract_store: Option<Arc<dyn GoalContractStore>>,
    module_tree_store: Option<Arc<dyn ModuleTreeStore>>,
    repository_tree_store: Option<Arc<dyn RepositoryTreeStore>>,
    project_storage: Option<Arc<dyn ProjectStorageStore>>,
    project_catalog_admin: Option<Arc<dyn ProjectCatalogAdmin>>,
    deep_map_executor: Option<Arc<dyn DeepMapExecutor>>,
    deep_map_runtime: Option<DeepMapRuntime>,
    deep_map_publication_state: Option<Arc<dyn DeepMapPublicationStateStore>>,
    deep_map_journal: Option<Arc<dyn DeepMapRunJournalStore>>,
    agent_run_executor: Option<Arc<dyn AgentRunExecutor>>,
}

struct IndexingCompositionPorts {
    settings_store: Arc<dyn a3_application::DesktopSettingsStore>,
    credential_store: Arc<dyn a3_application::ProviderCredentialStore>,
    agent_session_store: Arc<dyn AgentSessionStore>,
    ui_preferences_store: Arc<dyn UiPreferencesStore>,
    command_allowlist_store: Arc<dyn a3_application::CommandAllowlistStore>,
    project_ignore_settings_source: Arc<dyn a3_application::ProjectIgnoreSettingsSource>,
    index_store: Arc<dyn KnowledgeIndexStore>,
    module_card_freshness_store: Arc<dyn ModuleCardFreshnessStore>,
    module_card_detail_store: Arc<dyn ModuleCardDetailStore>,
    module_card_evidence_store: Arc<dyn ModuleCardEvidenceStore>,
    project_map_scene_store: Arc<dyn ProjectMapSceneStore>,
    project_map_atlas_store: Arc<dyn ProjectMapAtlasStore>,
    module_dependency_graph_store: Arc<dyn ModuleDependencyGraphStore>,
    module_runtime_store: Arc<dyn ModuleRuntimeStore>,
    knowledge_search_store: Arc<dyn KnowledgeSearchStore>,
    task_lens_index_store: Arc<dyn TaskLensIndexStore>,
    task_lens_claim_store: Arc<dyn TaskLensClaimStore>,
    task_lens_workspace_store: Arc<dyn TaskLensWorkspaceStore>,
    verification_evidence_store: Arc<dyn VerificationEvidenceStore>,
    run_journal_store: Arc<dyn RunJournalStore>,
    policy_store: Arc<dyn PolicyStore>,
    agent_action_store: Arc<dyn AgentActionStore>,
    task_ledger_store: Arc<dyn TaskLedgerStore>,
    agent_recovery_store: Arc<dyn AgentRecoveryStore>,
    goal_contract_store: Arc<dyn GoalContractStore>,
    module_tree_store: Arc<dyn ModuleTreeStore>,
    repository_tree_store: Arc<dyn RepositoryTreeStore>,
    project_storage: Arc<dyn ProjectStorageStore>,
    project_catalog_admin: Arc<dyn ProjectCatalogAdmin>,
    deep_map_executor: Option<Arc<dyn DeepMapExecutor>>,
    deep_map_runtime: DeepMapRuntime,
    deep_map_publication_state: Arc<dyn DeepMapPublicationStateStore>,
    deep_map_journal: Arc<dyn DeepMapRunJournalStore>,
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
                settings_store: Some(ports.settings_store),
                credential_store: Some(ports.credential_store),
                agent_session_store: Some(ports.agent_session_store),
                ui_preferences_store: Some(ports.ui_preferences_store),
                command_allowlist_store: Some(ports.command_allowlist_store),
                project_ignore_settings_source: Some(ports.project_ignore_settings_source),
                index_store: Some(ports.index_store),
                module_card_freshness_store: Some(ports.module_card_freshness_store),
                module_card_detail_store: Some(ports.module_card_detail_store),
                module_card_evidence_store: Some(ports.module_card_evidence_store),
                project_map_scene_store: Some(ports.project_map_scene_store),
                project_map_atlas_store: Some(ports.project_map_atlas_store),
                module_dependency_graph_store: Some(ports.module_dependency_graph_store),
                module_runtime_store: Some(ports.module_runtime_store),
                knowledge_search_store: Some(ports.knowledge_search_store),
                task_lens_index_store: Some(ports.task_lens_index_store),
                task_lens_claim_store: Some(ports.task_lens_claim_store),
                task_lens_workspace_store: Some(ports.task_lens_workspace_store),
                verification_evidence_store: Some(ports.verification_evidence_store),
                run_journal_store: Some(ports.run_journal_store),
                policy_store: Some(ports.policy_store),
                agent_action_store: Some(ports.agent_action_store),
                task_ledger_store: Some(ports.task_ledger_store),
                agent_recovery_store: Some(ports.agent_recovery_store),
                goal_contract_store: Some(ports.goal_contract_store),
                module_tree_store: Some(ports.module_tree_store),
                repository_tree_store: Some(ports.repository_tree_store),
                project_storage: Some(ports.project_storage),
                project_catalog_admin: Some(ports.project_catalog_admin),
                deep_map_executor: ports.deep_map_executor,
                deep_map_runtime: Some(ports.deep_map_runtime),
                deep_map_publication_state: Some(ports.deep_map_publication_state),
                deep_map_journal: Some(ports.deep_map_journal),
                agent_run_executor: None,
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
        let project_inspector: Arc<dyn a3_application::ProjectInspector> =
            Arc::new(RepositoryInspector::new());
        let model_settings = match (
            ports.settings_store.as_ref(),
            ports.credential_store.as_ref(),
        ) {
            (Some(settings), Some(credentials)) => Some(ModelSettingsManager::new(
                Arc::clone(settings),
                Arc::clone(credentials),
            )),
            _ => None,
        };
        let ui_preferences = ports.ui_preferences_store.clone();
        let project_settings = match (
            ports.project_ignore_settings_source.as_ref(),
            ports.index_store.as_ref(),
            ports.command_allowlist_store.as_ref(),
        ) {
            (Some(ignore), Some(index), Some(allowlist)) => Some(ProjectSettingsManager::new(
                a3_application::GetProjectSettings::new(
                    Arc::clone(ignore),
                    Arc::clone(index),
                    Arc::clone(allowlist),
                ),
                Arc::clone(allowlist),
            )),
            _ => None,
        };
        let project_status = ports.index_store.clone().map(GetProjectIndexStatus::new);
        let index_overview = ports
            .index_store
            .clone()
            .map(GetPublishedIndexOverview::new);
        let module_card_freshness = ports
            .module_card_freshness_store
            .map(GetModuleCardFreshness::new);
        let module_card_detail = ports.module_card_detail_store.map(GetModuleCardDetail::new);
        let project_map_source_preview = ports
            .module_card_evidence_store
            .clone()
            .zip(ports.project_map_atlas_store.clone())
            .map(|(evidence, atlas)| {
                GetProjectMapSourcePreview::new(
                    evidence,
                    atlas,
                    Arc::new(WorkspaceAgentSourceReader),
                )
            });
        let project_map_scene = ports.project_map_scene_store.map(GetProjectMapScene::new);
        let project_map_atlas = ports
            .project_map_atlas_store
            .map(ExploreProjectMapAtlas::new);
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
        let agent_activity = ports
            .task_lens_workspace_store
            .clone()
            .zip(ports.run_journal_store.clone())
            .map(|(workspace, journal)| GetAgentActivity::new(workspace, journal));
        let agent_verification = ports
            .task_lens_workspace_store
            .clone()
            .zip(ports.verification_evidence_store.clone())
            .map(|(workspace, evidence)| GetTaskVerificationInspection::new(workspace, evidence));
        let agent_inspection = Arc::new(AgentInspectionBuffer::new());
        let agent_approval = Arc::new(AgentApprovalBuffer::new());
        let agent_session_reporter = ports
            .agent_session_store
            .as_ref()
            .map(|store| Arc::new(AgentSessionRunReporter::new(Arc::clone(store))));
        let agent_ask_researcher = match (
            ports.task_lens_index_store.as_ref(),
            ports.knowledge_search_store.as_ref(),
            ports.task_lens_claim_store.as_ref(),
        ) {
            (Some(index), Some(search), Some(claims)) => Some(AgentAskResearcher::new(
                Arc::clone(index),
                Arc::clone(search),
                Arc::clone(claims),
            )),
            _ => None,
        };
        let production_agent_run_executor = match (
            ports.settings_store.as_ref(),
            ports.credential_store.as_ref(),
            ports.task_lens_workspace_store.as_ref(),
            ports.run_journal_store.as_ref(),
            ports.agent_action_store.as_ref(),
            ports.agent_recovery_store.as_ref(),
            ports.policy_store.as_ref(),
            ports.verification_evidence_store.as_ref(),
            ports.index_store.as_ref(),
            ports.task_lens_index_store.as_ref(),
            ports.knowledge_search_store.as_ref(),
            ports.task_lens_claim_store.as_ref(),
            ports.command_allowlist_store.as_ref(),
        ) {
            (
                Some(settings),
                Some(credentials),
                Some(workspace),
                Some(journal),
                Some(actions),
                Some(recovery),
                Some(policy),
                Some(evidence),
                Some(index),
                Some(lens_index),
                Some(search),
                Some(claims),
                Some(allowlist),
            ) => Some(Arc::new(
                ProductionAgentRunExecutor::new(
                    ProductionAgentRunPorts {
                        workspace: Arc::clone(workspace),
                        journal: Arc::clone(journal),
                        actions: Arc::clone(actions),
                        recovery: Arc::clone(recovery),
                        policy: Arc::clone(policy),
                        evidence: Arc::clone(evidence),
                        index: Arc::clone(index),
                        lens_index: Arc::clone(lens_index),
                        search: Arc::clone(search),
                        claims: Arc::clone(claims),
                        allowlist: Arc::clone(allowlist),
                    },
                    agent_conversation_runtime::AgentConversationRuntime::new(
                        Arc::clone(settings),
                        Arc::clone(credentials),
                    ),
                    Arc::clone(&agent_inspection),
                    Arc::clone(&agent_approval),
                    agent_session_reporter.clone(),
                )
                .map_err(|_| CompositionRootError::AgentRunManagerUnavailable)?,
            ) as Arc<dyn AgentRunExecutor>),
            _ => None,
        };
        let approval_read_ports = ports
            .task_lens_workspace_store
            .clone()
            .zip(ports.run_journal_store.clone())
            .zip(ports.policy_store.clone());
        let agent_approval_query =
            approval_read_ports
                .clone()
                .map(|((workspace, journal), policy)| {
                    GetAgentApprovalCenter::new(
                        workspace,
                        journal,
                        policy,
                        Arc::clone(&agent_approval),
                    )
                });
        let agent_approval_control = approval_read_ports
            .zip(ports.agent_action_store.clone())
            .map(|(((workspace, journal), policy), actions)| {
                ControlAgentApproval::new(
                    workspace,
                    journal,
                    policy,
                    actions,
                    Arc::clone(&agent_approval),
                )
            });
        let recovery_ports = (
            ports.task_lens_workspace_store.clone(),
            ports.agent_recovery_store.clone(),
            ports.run_journal_store.clone(),
            ports.task_ledger_store.clone(),
            ports.index_store.clone(),
        );
        let agent_task_recovery = match &recovery_ports {
            (Some(workspace), Some(recovery), Some(journal), Some(ledgers), Some(index)) => {
                Some(InspectAgentTaskRecovery::new(
                    Arc::clone(workspace),
                    Arc::clone(recovery),
                    Arc::clone(journal),
                    Arc::clone(ledgers),
                    Arc::clone(index),
                ))
            }
            _ => None,
        };
        let agent_task_control = match recovery_ports {
            (Some(workspace), Some(recovery), Some(journal), Some(ledgers), Some(index)) => Some(
                ControlAgentTaskRun::new(workspace, recovery, journal, ledgers, index),
            ),
            _ => None,
        };
        let agent_goal_metadata: Arc<dyn AgentGoalMetadataSource> =
            Arc::new(SystemAgentGoalMetadata);
        let agent_goal_query = ports.goal_contract_store.clone().map(GetAgentGoal::new);
        let agent_goal_create = ports
            .goal_contract_store
            .clone()
            .map(|store| CreateAgentGoal::new(store, Arc::clone(&agent_goal_metadata)));
        let agent_goal_revise = ports
            .goal_contract_store
            .clone()
            .map(|store| ReviseAgentGoal::new(store, agent_goal_metadata));
        let module_tree = ports.module_tree_store.map(GetModuleTreePage::new);
        let repository_tree = ports.repository_tree_store.map(GetRepositoryTreePage::new);
        let index_manager = ports
            .index_store
            .clone()
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
        let deep_map_runtime = ports.deep_map_runtime;
        let deep_map_enabled = deep_map_runtime.is_some() || ports.deep_map_executor.is_some();
        let deep_map_manager = if deep_map_enabled {
            Some({
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::DeepMapManagerUnavailable)?;
                DeepMapManager::start_with_journal(
                    submitter,
                    self.job_events.clone(),
                    ports.deep_map_executor,
                    Arc::clone(&job_ids),
                    ports.deep_map_journal.clone(),
                )
                .map_err(|_| CompositionRootError::DeepMapManager)
            }?)
        } else {
            None
        };
        let agent_run_executor = ports.agent_run_executor.or(production_agent_run_executor);
        let agent_run_manager = match (
            agent_run_executor,
            agent_task_recovery.clone(),
            agent_task_control.clone(),
        ) {
            (Some(executor), Some(inspector), Some(controller)) => {
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::AgentRunManagerUnavailable)?;
                Some(Arc::new(
                    AgentRunManager::start(
                        submitter,
                        self.job_events.clone(),
                        executor,
                        Arc::new(CoreAgentRuntimeRecovery::new(inspector, controller)),
                        Arc::clone(&job_ids),
                    )
                    .map_err(|_| CompositionRootError::AgentRunManager)?,
                ))
            }
            _ => None,
        };
        let agent_task_materializer = match (
            ports.goal_contract_store.as_ref(),
            ports.task_ledger_store.as_ref(),
            ports.run_journal_store.as_ref(),
            ports.index_store.as_ref(),
        ) {
            (Some(goals), Some(ledgers), Some(journal), Some(index)) => {
                Some(agent_session_manager::AgentTaskMaterializer::new(
                    Arc::clone(goals),
                    Arc::clone(ledgers),
                    Arc::clone(journal),
                    Arc::clone(index),
                ))
            }
            _ => None,
        };
        let agent_sessions = match (
            ports.agent_session_store.as_ref(),
            ports.settings_store.as_ref(),
            ports.credential_store.as_ref(),
        ) {
            (Some(sessions), Some(settings), Some(credentials)) => {
                let submitter = self
                    .job_scheduler
                    .submitter()
                    .map_err(|_| CompositionRootError::AgentRunManagerUnavailable)?;
                Some(AgentSessionManager::new(AgentSessionManagerDependencies {
                    store: Arc::clone(sessions),
                    runtime: agent_conversation_runtime::AgentConversationRuntime::new(
                        Arc::clone(settings),
                        Arc::clone(credentials),
                    ),
                    submitter,
                    job_ids: Arc::clone(&job_ids),
                    materializer: agent_task_materializer,
                    run_manager: agent_run_manager.clone(),
                    reporter: agent_session_reporter,
                    researcher: agent_ask_researcher,
                }))
            }
            _ => None,
        };
        Ok(CompositionRoot {
            health_query: self.health_query,
            model_settings,
            project_settings,
            open_project: OpenProject::new(
                project_directory_picker,
                Arc::clone(&project_inspector),
                project_reconciliation_confirmer,
                Arc::clone(&store),
            ),
            activate_catalog_project: ActivateCatalogProject::new(
                project_inspector,
                Arc::clone(&store),
            ),
            project_catalog_store: Arc::clone(&store),
            recent_projects: ListRecentProjects::new(store),
            project_status,
            index_overview,
            module_card_freshness,
            module_card_detail,
            module_card_evidence,
            project_map_scene,
            project_map_atlas,
            project_map_source_preview,
            module_dependency_graph,
            module_runtime_map,
            module_runtime_flow,
            project_map_search,
            task_lens_tasks,
            task_lens_task,
            task_lens_compile,
            agent_activity,
            agent_verification,
            agent_inspection,
            agent_approval,
            agent_approval_query,
            agent_approval_control,
            agent_approval_metadata: SystemAgentApprovalMetadata,
            agent_task_recovery,
            agent_task_control,
            agent_recovery_metadata: SystemAgentRecoveryMetadata,
            agent_goal_query,
            agent_goal_create,
            agent_goal_revise,
            module_tree,
            repository_tree,
            project_storage: ports.project_storage.map(GetProjectStorageUsage::new),
            remove_project: ports.project_catalog_admin.map(RemoveProjectFromList::new),
            active_project: Mutex::new(None),
            project_operation_active: AtomicBool::new(false),
            agent_task_operation_active: AtomicBool::new(false),
            index_manager,
            deep_map_manager,
            deep_map_runtime,
            deep_map_publication_state: ports.deep_map_publication_state,
            deep_map_journal: ports.deep_map_journal,
            deep_map_dashboard_index: ports.index_store,
            agent_run_manager,
            agent_sessions,
            ui_preferences,
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
            let settings_store: Arc<dyn a3_application::DesktopSettingsStore> = store.clone();
            let credential_store: Arc<dyn a3_application::ProviderCredentialStore> =
                Arc::new(NativeProviderCredentialStore::new());
            let agent_session_store: Arc<dyn AgentSessionStore> = store.clone();
            let ui_preferences_store: Arc<dyn UiPreferencesStore> = store.clone();
            let command_allowlist_store: Arc<dyn a3_application::CommandAllowlistStore> =
                store.clone();
            let project_ignore_settings_source: Arc<
                dyn a3_application::ProjectIgnoreSettingsSource,
            > = Arc::new(a3_repo_index::RepositoryProjectIgnoreSettingsSource);
            let project_storage: Arc<dyn ProjectStorageStore> = store.clone();
            let project_catalog_admin: Arc<dyn ProjectCatalogAdmin> = store.clone();
            let module_card_freshness_store: Arc<dyn ModuleCardFreshnessStore> = store.clone();
            let module_card_detail_store: Arc<dyn ModuleCardDetailStore> = store.clone();
            let module_card_evidence_store: Arc<dyn ModuleCardEvidenceStore> = store.clone();
            let project_map_scene_store: Arc<dyn ProjectMapSceneStore> = store.clone();
            let project_map_atlas_store: Arc<dyn ProjectMapAtlasStore> = store.clone();
            let module_dependency_graph_store: Arc<dyn ModuleDependencyGraphStore> = store.clone();
            let module_runtime_store: Arc<dyn ModuleRuntimeStore> = store.clone();
            let knowledge_search_store: Arc<dyn KnowledgeSearchStore> = store.clone();
            let task_lens_index_store: Arc<dyn TaskLensIndexStore> = store.clone();
            let task_lens_claim_store: Arc<dyn TaskLensClaimStore> = store.clone();
            let task_lens_workspace_store: Arc<dyn TaskLensWorkspaceStore> = store.clone();
            let verification_evidence_store: Arc<dyn VerificationEvidenceStore> = store.clone();
            let run_journal_store: Arc<dyn RunJournalStore> = store.clone();
            let policy_store: Arc<dyn PolicyStore> = store.clone();
            let agent_action_store: Arc<dyn AgentActionStore> = store.clone();
            let task_ledger_store: Arc<dyn TaskLedgerStore> = store.clone();
            let agent_recovery_store: Arc<dyn AgentRecoveryStore> = store.clone();
            let goal_contract_store: Arc<dyn GoalContractStore> = store.clone();
            let module_tree_store: Arc<dyn ModuleTreeStore> = store.clone();
            let repository_tree_store: Arc<dyn RepositoryTreeStore> = store.clone();
            let catalog_store: Arc<dyn KnowledgeStore> = store.clone();
            let index_store: Arc<dyn KnowledgeIndexStore> = store.clone();
            let deep_map_publication_state: Arc<dyn a3_application::DeepMapPublicationStateStore> =
                store.clone();
            let deep_map_journal: Arc<dyn a3_application::DeepMapRunJournalStore> = store.clone();
            let module_card_publisher: Arc<dyn a3_application::VerifiedModuleCardPublisher> = store;
            let deep_map_runtime = DeepMapRuntime::new(
                Arc::clone(&settings_store),
                Arc::clone(&credential_store),
                Arc::clone(&index_store),
                module_card_publisher,
                Arc::clone(&deep_map_publication_state),
            );
            let deep_map_executor = tauri::async_runtime::block_on(deep_map_runtime.resolve());
            app.manage(base.finish_with_indexing(
                Arc::new(NativeProjectDirectoryPicker::new(app.handle().clone())),
                Arc::new(NativeProjectReconciliationConfirmer::new(
                    app.handle().clone(),
                )),
                catalog_store,
                IndexingCompositionPorts {
                    settings_store,
                    credential_store,
                    agent_session_store,
                    ui_preferences_store,
                    command_allowlist_store,
                    project_ignore_settings_source,
                    index_store,
                    module_card_freshness_store,
                    module_card_detail_store,
                    module_card_evidence_store,
                    project_map_scene_store,
                    project_map_atlas_store,
                    module_dependency_graph_store,
                    module_runtime_store,
                    knowledge_search_store,
                    task_lens_index_store,
                    task_lens_claim_store,
                    task_lens_workspace_store,
                    verification_evidence_store,
                    run_journal_store,
                    policy_store,
                    agent_action_store,
                    task_ledger_store,
                    agent_recovery_store,
                    goal_contract_store,
                    module_tree_store,
                    repository_tree_store,
                    project_storage,
                    project_catalog_admin,
                    deep_map_executor,
                    deep_map_runtime,
                    deep_map_publication_state,
                    deep_map_journal,
                },
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::activate_catalog_project,
            commands::cancel_model_probe,
            commands::cancel_deep_map,
            commands::compile_task_lens,
            commands::configure_model_provider,
            commands::delete_model_provider_credential,
            commands::confirm_project_command_allowlist,
            commands::control_agent_approval,
            commands::control_agent_session,
            commands::control_agent_task_run,
            commands::create_agent_goal,
            commands::discover_provider_models,
            commands::list_recent_projects,
            commands::open_project,
            commands::pause_deep_map,
            commands::query_deep_map,
            commands::query_deep_map_runs,
            commands::query_deep_map_entries,
            commands::query_deep_map_entry_detail,
            commands::query_deep_map_run_dashboard,
            commands::query_deep_map_run_modules,
            commands::query_deep_map_module_steps,
            commands::query_deep_map_atlas_impact,
            commands::query_project_catalog,
            commands::query_project_status,
            commands::query_project_settings,
            commands::query_index_activity,
            commands::query_index_overview,
            commands::query_module_card_freshness,
            commands::query_module_card_detail,
            commands::query_module_card_evidence,
            commands::query_project_map_source_preview,
            commands::query_project_map_scene,
            commands::query_project_map_atlas_scene,
            commands::query_project_map_entity_context,
            commands::query_project_map_inventory_page,
            commands::query_project_map_flow_scene,
            commands::query_module_dependency_graph,
            commands::query_module_runtime_flow,
            commands::query_module_runtime_map,
            commands::query_module_tree,
            commands::query_agent_activity,
            commands::query_agent_approval,
            commands::query_agent_session,
            commands::query_agent_sessions,
            commands::query_agent_inspection,
            commands::query_agent_inspection_log,
            commands::query_agent_goal,
            commands::query_agent_task_recovery,
            commands::query_project_map_search,
            commands::query_task_lens_task,
            commands::query_task_lens_tasks,
            commands::query_repository_tree,
            commands::query_health,
            commands::query_settings,
            commands::query_ui_preferences,
            commands::rebuild_project_index,
            commands::resume_deep_map,
            commands::restore_last_project,
            commands::revise_agent_goal,
            commands::remove_project,
            commands::remove_catalog_project,
            commands::probe_model_role,
            commands::set_model_provider_credential,
            commands::start_deep_map,
            commands::submit_agent_message,
            commands::update_agent_workspace_layout
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

fn inspection_contexts_are_current(
    overview: &AgentInspectionOverview,
    verification: &TaskVerificationInspection,
) -> bool {
    overview
        .patch()
        .is_none_or(|patch| inspection_context_is_current(patch.context(), verification))
        && overview
            .processes()
            .iter()
            .all(|process| inspection_context_is_current(process.context(), verification))
}

fn inspection_context_is_current(
    context: AgentInspectionContext,
    verification: &TaskVerificationInspection,
) -> bool {
    context.task_id() == verification.goal_contract().task_id()
        && context.snapshot_id() == verification.published_snapshot_id()
        && verification
            .task_ledger()
            .ledger()
            .steps()
            .filter(|step| step.is_active_plan_step())
            .find(|step| step.definition().id() == context.step_id())
            .is_some_and(|step| {
                step.definition().verification_spec().id() == context.verification_spec_id()
                    && step
                        .attempts()
                        .iter()
                        .any(|attempt| attempt.run_id() == context.run_id())
            })
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

fn map_project_catalog_to_v1(page: ProjectCatalogPage) -> ProjectCatalogResponseV1 {
    ProjectCatalogResponseV1::new(
        page.projects()
            .iter()
            .map(|project| {
                RecentProjectSummaryV1::new(
                    project.project_id().to_string(),
                    ProjectSummaryV1::new(
                        project.repository_id().to_string(),
                        project.worktree_id().to_string(),
                        project.worktree_root_display().as_str().to_owned(),
                        map_git_head_to_v1(project.head()),
                    ),
                )
            })
            .collect(),
        page.previous_cursor()
            .map(|cursor| format!("{:016x}", cursor.get())),
        page.next_cursor()
            .map(|cursor| format!("{:016x}", cursor.get())),
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
    decode_module_id(
        request
            .module_id()
            .ok_or_else(invalid_module_card_detail_query)?,
    )
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

pub(crate) fn map_project_map_source_preview_query_from_v1(
    request: &QueryProjectMapSourcePreviewRequestV1,
) -> Result<ProjectMapSourcePreviewQuery, CommandErrorV1> {
    match request.selection() {
        ProjectMapSourcePreviewSelectionV1::ModuleCard {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            evidence_id,
        } => {
            let current_index_run_id = decode_index_run_id(current_index_run_id)
                .map_err(|()| invalid_project_map_source_preview_query())?;
            let current_snapshot_id = decode_snapshot_id(current_snapshot_id)
                .map_err(|()| invalid_project_map_source_preview_query())?;
            let source_index_run_id = decode_index_run_id(source_index_run_id)
                .map_err(|()| invalid_project_map_source_preview_query())?;
            let source_snapshot_id = decode_snapshot_id(source_snapshot_id)
                .map_err(|()| invalid_project_map_source_preview_query())?;
            if source_index_run_id == current_index_run_id
                && source_snapshot_id != current_snapshot_id
            {
                return Err(invalid_project_map_source_preview_query());
            }
            Ok(ProjectMapSourcePreviewQuery::ModuleCard(
                ModuleCardEvidenceQuery::new(
                    current_index_run_id,
                    current_snapshot_id,
                    source_index_run_id,
                    source_snapshot_id,
                    decode_stable_id(card_id)
                        .map(ModuleCardId::from_bytes)
                        .map_err(|()| invalid_project_map_source_preview_query())?,
                    decode_module_id(module_id)
                        .map_err(|()| invalid_project_map_source_preview_query())?,
                    decode_stable_id(evidence_id)
                        .map(ModuleCardEvidenceId::from_bytes)
                        .map_err(|()| invalid_project_map_source_preview_query())?,
                ),
            ))
        }
        ProjectMapSourcePreviewSelectionV1::Index { evidence } => {
            map_index_evidence_from_v1(evidence)
                .map(ProjectMapSourcePreviewQuery::Index)
                .map_err(|()| invalid_project_map_source_preview_query())
        }
    }
}

pub(crate) fn map_project_map_scene_query_from_v1(
    request: &QueryProjectMapSceneRequestV1,
) -> Result<ProjectMapSceneQuery, CommandErrorV1> {
    request
        .focus_module_id()
        .map(decode_module_id)
        .transpose()
        .map(ProjectMapSceneQuery::new)
        .map_err(|()| invalid_project_map_scene_query())
}

fn map_project_map_scene_to_v1(scene: &ProjectMapScene) -> ProjectMapSceneV1 {
    ProjectMapSceneV1::new(
        scene.index_run_id().to_string(),
        scene.snapshot_id().to_string(),
        a3_protocol::ScenePolicyVersionV1::V1,
        scene.focus_module_id().map(|id| id.to_string()),
        scene.primary_module_count().to_string(),
        scene
            .modules()
            .iter()
            .map(map_project_map_scene_module_to_v1)
            .collect(),
        scene.modules_truncated(),
        scene.observed_relation_group_count().to_string(),
        scene
            .relations()
            .iter()
            .map(map_project_map_scene_relation_to_v1)
            .collect(),
        scene.relations_truncated(),
        scene.inspected_edge_count().to_string(),
        scene.unmapped_edge_count().to_string(),
        scene.source_edges_truncated(),
    )
}

fn map_project_map_scene_module_to_v1(module: &ProjectMapSceneModule) -> ProjectMapSceneModuleV1 {
    ProjectMapSceneModuleV1::new(
        module.module_id().to_string(),
        module.parent_module_id().map(|id| id.to_string()),
        match module.kind() {
            ProjectMapSceneModuleKind::ManifestBoundary => {
                ProjectMapSceneModuleKindV1::ManifestBoundary
            }
            ProjectMapSceneModuleKind::PathBoundary => ProjectMapSceneModuleKindV1::PathBoundary,
        },
        module.display_name().to_owned(),
        module.rank(),
        module.manifest_count().to_string(),
        module.file_count().to_string(),
        module.symbol_count().to_string(),
        module.central_symbol_count().to_string(),
        module.entrypoint_count().to_string(),
        module.test_count().to_string(),
        match module.mapping_status() {
            ProjectMapMappingStatus::Current => ProjectMapMappingStatusV1::Current,
            ProjectMapMappingStatus::Stale => ProjectMapMappingStatusV1::Stale,
            ProjectMapMappingStatus::NeedsReview => ProjectMapMappingStatusV1::NeedsReview,
            ProjectMapMappingStatus::Unmapped => ProjectMapMappingStatusV1::Unmapped,
        },
        module.card_coverage_basis_points(),
        module.card_binding().map(|binding| {
            ProjectMapSceneCardBindingV1::new(
                encode_hex(binding.card_id().as_bytes()),
                binding.source_index_run_id().to_string(),
                binding.source_snapshot_id().to_string(),
            )
        }),
        module
            .representative_evidence_id()
            .map(|id| encode_hex(id.as_bytes())),
    )
}

fn map_project_map_scene_relation_to_v1(
    relation: &ProjectMapSceneRelation,
) -> ProjectMapSceneRelationV1 {
    ProjectMapSceneRelationV1::new(
        relation.source_module_id().to_string(),
        relation.target_module_id().to_string(),
        match relation.relation() {
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
        relation.observed_evidence_count().to_string(),
        relation.evidence_id().map(|id| encode_hex(id.as_bytes())),
    )
}

fn map_project_map_source_preview_to_v1(
    preview: &ProjectMapSourcePreview,
) -> ProjectMapSourcePreviewV1 {
    ProjectMapSourcePreviewV1::new(
        match preview.language() {
            IndexLanguage::Generic => IndexLanguageV1::Generic,
            IndexLanguage::Rust => IndexLanguageV1::Rust,
            IndexLanguage::TypeScriptJavaScript => IndexLanguageV1::TypeScriptJavaScript,
            IndexLanguage::Python => IndexLanguageV1::Python,
        },
        preview.path_display().to_owned(),
        preview.start_line(),
        preview.line_count(),
        preview.highlight().map(|highlight| {
            ProjectMapSourceHighlightV1::new(
                highlight.start_line(),
                highlight.start_column(),
                highlight.end_line(),
                highlight.end_column(),
            )
        }),
        preview.text().to_owned(),
        preview.truncated_before(),
        preview.truncated_after(),
    )
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
    result: &ProjectMapSearchResult,
) -> Option<ProjectMapSearchV1> {
    let mut rank = 0_u16;
    let mut hits = Vec::with_capacity(result.hits().len());
    for (index, hit) in result.hits().iter().enumerate() {
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
            result.module_binding(index).map(|id| id.to_string()),
            map_project_map_search_selection_to_v2(hit.target()),
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

fn map_project_map_search_selection_to_v2(
    target: &ExactSearchTarget,
) -> ProjectMapSearchEvidenceSelectionV2 {
    match target {
        ExactSearchTarget::File(revision) => ProjectMapSearchEvidenceSelectionV2::File {
            evidence_id: encode_hex(
                ModuleCardEvidenceId::for_file_revision_v1(revision).as_bytes(),
            ),
        },
        ExactSearchTarget::Symbol(symbol) => ProjectMapSearchEvidenceSelectionV2::Symbol {
            evidence_id: encode_hex(
                ModuleCardEvidenceId::for_symbol_v1(symbol.symbol()).as_bytes(),
            ),
            symbol_id: symbol.symbol().id().to_string(),
        },
    }
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

pub(crate) fn map_agent_goal_task_id_from_v1(
    request: &a3_protocol::QueryAgentGoalRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid_agent_goal())
}

pub(crate) fn map_agent_activity_task_id_from_v1(
    request: &a3_protocol::QueryAgentActivityRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidTaskLensSelection))
}

pub(crate) fn map_agent_inspection_task_id_from_v1(
    request: &a3_protocol::QueryAgentInspectionRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid_agent_inspection_query())
}

pub(crate) fn map_agent_approval_task_id_from_v1(
    request: &a3_protocol::QueryAgentApprovalRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentApprovalRequest))
}

#[allow(clippy::type_complexity)]
pub(crate) fn map_agent_approval_control_from_v1(
    request: &a3_protocol::ControlAgentApprovalRequestV1,
) -> Result<
    (
        TaskId,
        AgentApprovalRevision,
        u32,
        TaskLedgerStoreVersion,
        AgentApprovalControlActionV1,
    ),
    CommandErrorV1,
> {
    let invalid = || CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentApprovalRequest);
    let task_id = decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid())?;
    let revision = parse_canonical_positive_u64(request.expected_approval_revision())
        .and_then(|value| AgentApprovalRevision::new(value).map_err(|_| ()))
        .map_err(|()| invalid())?;
    if request.expected_ledger_revision() == 0 {
        return Err(invalid());
    }
    let version = parse_canonical_positive_u64(request.expected_ledger_store_version())
        .and_then(|value| TaskLedgerStoreVersion::new(value).map_err(|_| ()))
        .map_err(|()| invalid())?;
    Ok((
        task_id,
        revision,
        request.expected_ledger_revision(),
        version,
        request.action(),
    ))
}

#[allow(clippy::type_complexity)]
pub(crate) fn map_agent_inspection_log_query_from_v1(
    request: &a3_protocol::QueryAgentInspectionLogRequestV1,
) -> Result<
    (
        TaskId,
        AgentInspectionRevision,
        AgentInspectionId,
        a3_protocol::AgentInspectionStreamV1,
        AgentLogPageOffset,
        AgentLogPageLimit,
    ),
    CommandErrorV1,
> {
    let invalid = invalid_agent_inspection_query;
    let task_id = decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid())?;
    let revision = parse_canonical_positive_u64(request.inspection_revision())
        .and_then(|value| AgentInspectionRevision::new(value).map_err(|_| ()))
        .map_err(|()| invalid())?;
    let inspection_id = decode_stable_id(request.inspection_id())
        .map(AgentInspectionId::from_bytes)
        .map_err(|()| invalid())?;
    let limit = AgentLogPageLimit::new(request.limit()).map_err(|_| invalid())?;
    Ok((
        task_id,
        revision,
        inspection_id,
        request.stream(),
        AgentLogPageOffset::new(request.offset()),
        limit,
    ))
}

pub(crate) fn map_agent_task_recovery_task_id_from_v1(
    request: &a3_protocol::QueryAgentTaskRecoveryRequestV1,
) -> Result<TaskId, CommandErrorV1> {
    decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentTaskControl))
}

pub(crate) fn map_agent_task_control_from_v1(
    request: &a3_protocol::ControlAgentTaskRunRequestV1,
) -> Result<
    (
        TaskId,
        u32,
        TaskLedgerStoreVersion,
        AgentTaskControlActionV1,
    ),
    CommandErrorV1,
> {
    let invalid = || CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentTaskControl);
    let task_id = decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid())?;
    if request.expected_ledger_revision() == 0 {
        return Err(invalid());
    }
    let version = parse_canonical_positive_u64(request.expected_ledger_store_version())
        .and_then(|value| TaskLedgerStoreVersion::new(value).map_err(|_| ()))
        .map_err(|()| invalid())?;
    Ok((
        task_id,
        request.expected_ledger_revision(),
        version,
        request.action(),
    ))
}

pub(crate) fn map_create_agent_goal_from_v1(
    request: &a3_protocol::CreateAgentGoalRequestV1,
) -> Result<AgentGoalDraft, CommandErrorV1> {
    map_agent_goal_draft_from_v1(request.draft())
}

pub(crate) fn map_revise_agent_goal_from_v1(
    request: &a3_protocol::ReviseAgentGoalRequestV1,
) -> Result<
    (
        TaskId,
        GoalContractRevision,
        AgentGoalDraft,
        GoalRevisionReason,
    ),
    CommandErrorV1,
> {
    let task_id = decode_stable_id(request.task_id())
        .map(TaskId::from_bytes)
        .map_err(|()| invalid_agent_goal())?;
    let revision =
        GoalContractRevision::new(request.expected_revision()).map_err(|_| invalid_agent_goal())?;
    let reason = bounded_goal_text(request.revision_reason(), 4 * 1_024)
        .and_then(|value| GoalRevisionReason::try_from_string(value).map_err(|_| ()))
        .map_err(|_| invalid_agent_goal())?;
    let draft = map_agent_goal_draft_from_v1(request.draft())?;
    Ok((task_id, revision, draft, reason))
}

fn map_agent_goal_draft_from_v1(
    draft: &AgentGoalDraftInputV1,
) -> Result<AgentGoalDraft, CommandErrorV1> {
    if draft.acceptance_criteria().is_empty()
        || draft.acceptance_criteria().len() > 64
        || draft.constraints().len() > 64
        || draft.non_goals().len() > 64
        || draft.user_decisions().len() > 64
    {
        return Err(invalid_agent_goal());
    }
    let objective = bounded_goal_text(draft.objective(), 16 * 1_024)
        .and_then(|value| GoalObjective::try_from_string(value).map_err(|_| ()))
        .map_err(|_| invalid_agent_goal())?;
    let acceptance_criteria = draft
        .acceptance_criteria()
        .iter()
        .map(map_agent_goal_criterion_from_v1)
        .collect::<Result<Vec<_>, _>>()?;
    let constraints =
        map_agent_goal_text_collection(draft.constraints(), GoalConstraint::try_from_string)?;
    let non_goals = map_agent_goal_text_collection(draft.non_goals(), NonGoal::try_from_string)?;
    let user_decisions =
        map_agent_goal_text_collection(draft.user_decisions(), UserDecision::try_from_string)?;
    let success_verification = bounded_goal_text(draft.success_verification(), 8 * 1_024)
        .and_then(|value| SuccessVerification::try_from_string(value).map_err(|_| ()))
        .map_err(|_| invalid_agent_goal())?;
    Ok(AgentGoalDraft::new(
        objective,
        acceptance_criteria,
        constraints,
        non_goals,
        user_decisions,
        success_verification,
    ))
}

fn map_agent_goal_criterion_from_v1(
    criterion: &AgentGoalCriterionInputV1,
) -> Result<AgentGoalCriterionDraft, CommandErrorV1> {
    let criterion_id = criterion
        .criterion_id()
        .map(decode_stable_id)
        .transpose()
        .map_err(|()| invalid_agent_goal())?
        .map(AcceptanceCriterionId::from_bytes);
    let statement = bounded_goal_text(criterion.statement(), 4 * 1_024)
        .and_then(|value| AcceptanceCriterionStatement::try_from_string(value).map_err(|_| ()))
        .map_err(|_| invalid_agent_goal())?;
    let requirement = match criterion.requirement() {
        AgentGoalCriterionRequirementV1::Must => AcceptanceCriterionRequirement::Must,
        AgentGoalCriterionRequirementV1::Should => AcceptanceCriterionRequirement::Should,
    };
    Ok(AgentGoalCriterionDraft::new(
        criterion_id,
        statement,
        requirement,
    ))
}

fn map_agent_goal_text_collection<T>(
    values: &[String],
    map: fn(String) -> Result<T, a3_domain::GoalContractTextError>,
) -> Result<Vec<T>, CommandErrorV1> {
    values
        .iter()
        .map(|value| {
            bounded_goal_text(value, 4 * 1_024)
                .and_then(|value| map(value).map_err(|_| ()))
                .map_err(|_| invalid_agent_goal())
        })
        .collect()
}

fn bounded_goal_text(value: &str, maximum_bytes: usize) -> Result<String, ()> {
    if value.len() > maximum_bytes {
        return Err(());
    }
    Ok(value.to_owned())
}

fn map_agent_goal_to_v1(goal: &GoalContract) -> AgentGoalContractV1 {
    AgentGoalContractV1::new(
        goal.task_id().to_string(),
        goal.revision().get(),
        goal.previous_revision().map(GoalContractRevision::get),
        goal.revision_reason()
            .map(|reason| reason.as_str().to_owned()),
        goal.draft().objective().as_str().to_owned(),
        goal.draft()
            .acceptance_criteria()
            .iter()
            .map(|criterion| {
                AgentGoalCriterionV1::new(
                    criterion.id().to_string(),
                    criterion.statement().as_str().to_owned(),
                    match criterion.requirement() {
                        AcceptanceCriterionRequirement::Must => {
                            AgentGoalCriterionRequirementV1::Must
                        }
                        AcceptanceCriterionRequirement::Should => {
                            AgentGoalCriterionRequirementV1::Should
                        }
                    },
                )
            })
            .collect(),
        goal.draft()
            .constraints()
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect(),
        goal.draft()
            .non_goals()
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect(),
        goal.draft()
            .user_decisions()
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect(),
        goal.draft().success_verification().as_str().to_owned(),
        goal.created_at().unix_millis().to_string(),
    )
}

fn map_agent_activity_to_v1(activity: &AgentActivity) -> Option<AgentActivityV1> {
    let stored = activity.anchor().task_ledger();
    let ledger = stored.ledger();
    let blockers = ledger
        .steps()
        .filter(|step| step.is_active_plan_step())
        .filter_map(|step| {
            let status = match step.status() {
                TaskStepStatus::Blocked => AgentActivityBlockerStatusV1::Blocked,
                TaskStepStatus::AwaitingApproval => AgentActivityBlockerStatusV1::AwaitingApproval,
                _ => return None,
            };
            step.blocking_reason().map(|reason| {
                AgentActivityBlockerV1::new(
                    step.definition().id().to_string(),
                    status,
                    reason.as_str().to_owned(),
                )
            })
        })
        .collect();
    let run = match activity.run() {
        Some(activity_run) => Some(map_agent_activity_run_to_v1(
            activity_run,
            ledger.revision().get(),
        )?),
        None => None,
    };
    Some(AgentActivityV1::new(
        ledger.revision().get(),
        stored.version().get().to_string(),
        blockers,
        run,
    ))
}

#[derive(Debug, Clone, Copy)]
struct AgentRuntimeTarget {
    ledger_revision: TaskLedgerRevision,
    ledger_store_version: TaskLedgerStoreVersion,
    controller_state: AgentControllerState,
}

enum AgentRuntimeTargetLoad {
    Expected(AgentTaskRecoveryResultV1),
    Available(AgentRuntimeTarget),
}

async fn load_agent_runtime_target(
    reader: Option<&GetAgentActivity>,
    project: &ProjectIdentity,
    task_id: TaskId,
) -> Result<AgentRuntimeTargetLoad, CommandErrorV1> {
    let reader = reader.ok_or_else(agent_task_control_unavailable)?;
    let result = reader
        .execute(project, task_id, &DesktopBoundedReadControl::new())
        .await
        .map_err(map_agent_activity_error_to_v1)?;
    let activity = match result {
        AgentActivityLoadResult::TaskNotFound => {
            return Ok(AgentRuntimeTargetLoad::Expected(
                AgentTaskRecoveryResultV1::TaskNotFound,
            ));
        }
        AgentActivityLoadResult::LedgerUnavailable => {
            return Ok(AgentRuntimeTargetLoad::Expected(
                AgentTaskRecoveryResultV1::LedgerUnavailable,
            ));
        }
        AgentActivityLoadResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => {
            return Ok(AgentRuntimeTargetLoad::Expected(
                AgentTaskRecoveryResultV1::GoalRevisionMismatch {
                    current_revision,
                    ledger_revision,
                },
            ));
        }
        AgentActivityLoadResult::ActivityChanged => {
            return Ok(AgentRuntimeTargetLoad::Expected(
                AgentTaskRecoveryResultV1::ActivityChanged,
            ));
        }
        AgentActivityLoadResult::Available(activity) => activity,
    };
    let Some(run) = activity.run() else {
        return Ok(AgentRuntimeTargetLoad::Expected(
            AgentTaskRecoveryResultV1::RunUnavailable,
        ));
    };
    if !run.is_active_attempt() || run.run().state().is_terminal() {
        return Ok(AgentRuntimeTargetLoad::Expected(
            AgentTaskRecoveryResultV1::RunNotControllable {
                state: map_agent_controller_state_to_v1(run.run().state()),
            },
        ));
    }
    let ledger = activity.anchor().task_ledger();
    if run.run().task_ledger_revision() != ledger.ledger().revision() {
        return Ok(AgentRuntimeTargetLoad::Expected(
            AgentTaskRecoveryResultV1::ActivityChanged,
        ));
    }
    Ok(AgentRuntimeTargetLoad::Available(AgentRuntimeTarget {
        ledger_revision: ledger.ledger().revision(),
        ledger_store_version: ledger.version(),
        controller_state: run.run().state(),
    }))
}

fn map_runtime_expected_to_control(result: AgentTaskRecoveryResultV1) -> AgentTaskControlResultV1 {
    match result {
        AgentTaskRecoveryResultV1::NoProject => AgentTaskControlResultV1::NoProject,
        AgentTaskRecoveryResultV1::TaskNotFound => AgentTaskControlResultV1::TaskNotFound,
        AgentTaskRecoveryResultV1::LedgerUnavailable => AgentTaskControlResultV1::LedgerUnavailable,
        AgentTaskRecoveryResultV1::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => AgentTaskControlResultV1::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        },
        AgentTaskRecoveryResultV1::ActivityChanged => AgentTaskControlResultV1::ActivityChanged,
        AgentTaskRecoveryResultV1::RunUnavailable => AgentTaskControlResultV1::RunUnavailable,
        AgentTaskRecoveryResultV1::RunNotControllable { state } => {
            AgentTaskControlResultV1::RunNotControllable { state }
        }
        AgentTaskRecoveryResultV1::RuntimeOwned { .. }
        | AgentTaskRecoveryResultV1::Paused { .. }
        | AgentTaskRecoveryResultV1::Available { .. } => AgentTaskControlResultV1::ActivityChanged,
    }
}

const fn map_agent_runtime_state_to_v1(
    state: AgentRunActivityState,
) -> Option<AgentTaskRuntimeStateV1> {
    match state {
        AgentRunActivityState::Queued => Some(AgentTaskRuntimeStateV1::Queued),
        AgentRunActivityState::Running => Some(AgentTaskRuntimeStateV1::Running),
        AgentRunActivityState::Pausing => Some(AgentTaskRuntimeStateV1::Pausing),
        AgentRunActivityState::Cancelling => Some(AgentTaskRuntimeStateV1::Cancelling),
        AgentRunActivityState::Idle
        | AgentRunActivityState::Paused
        | AgentRunActivityState::Succeeded
        | AgentRunActivityState::Failed
        | AgentRunActivityState::Cancelled => None,
    }
}

fn map_agent_run_manager_error_to_v1(_error: AgentRunManagerControlError) -> CommandErrorV1 {
    agent_task_control_unavailable()
}

fn map_agent_task_recovery_result_to_v1(
    result: AgentTaskRecoveryLoadResult,
) -> AgentTaskRecoveryResultV1 {
    match result {
        AgentTaskRecoveryLoadResult::TaskNotFound => AgentTaskRecoveryResultV1::TaskNotFound,
        AgentTaskRecoveryLoadResult::LedgerUnavailable => {
            AgentTaskRecoveryResultV1::LedgerUnavailable
        }
        AgentTaskRecoveryLoadResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => AgentTaskRecoveryResultV1::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        },
        AgentTaskRecoveryLoadResult::ActivityChanged => AgentTaskRecoveryResultV1::ActivityChanged,
        AgentTaskRecoveryLoadResult::RunUnavailable => AgentTaskRecoveryResultV1::RunUnavailable,
        AgentTaskRecoveryLoadResult::RunNotControllable { state } => {
            AgentTaskRecoveryResultV1::RunNotControllable {
                state: map_agent_controller_state_to_v1(state),
            }
        }
        AgentTaskRecoveryLoadResult::Available(recovery) => AgentTaskRecoveryResultV1::Available {
            recovery: map_agent_task_recovery_to_v1(&recovery),
        },
    }
}

fn map_agent_task_recovery_to_v1(recovery: &AgentTaskRecovery) -> AgentTaskRecoveryV1 {
    AgentTaskRecoveryV1::new(
        recovery.ledger_revision(),
        recovery.ledger_store_version().get().to_string(),
        map_agent_controller_state_to_v1(recovery.state()),
        recovery.run_snapshot_id().to_string(),
        recovery.published_snapshot_id().to_string(),
        recovery.snapshot_changed(),
        recovery.interrupted_tool_attempts(),
        recovery.stale_evidence_count(),
        recovery.mutation_reconciliation_required(),
        recovery.mutation_replan_required(),
        recovery.can_resume(),
    )
}

fn map_agent_task_control_result_to_v1(
    result: AgentTaskControlResult,
    runtime_start: Option<AgentTaskRuntimeStartV1>,
) -> AgentTaskControlResultV1 {
    match result {
        AgentTaskControlResult::TaskNotFound => AgentTaskControlResultV1::TaskNotFound,
        AgentTaskControlResult::LedgerUnavailable => AgentTaskControlResultV1::LedgerUnavailable,
        AgentTaskControlResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => AgentTaskControlResultV1::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        },
        AgentTaskControlResult::ActivityChanged => AgentTaskControlResultV1::ActivityChanged,
        AgentTaskControlResult::RunUnavailable => AgentTaskControlResultV1::RunUnavailable,
        AgentTaskControlResult::RunNotControllable { state } => {
            AgentTaskControlResultV1::RunNotControllable {
                state: map_agent_controller_state_to_v1(state),
            }
        }
        AgentTaskControlResult::MutationReconciliationRequired => {
            AgentTaskControlResultV1::MutationReconciliationRequired
        }
        AgentTaskControlResult::ResumeRequiresReplan => {
            AgentTaskControlResultV1::ResumeRequiresReplan
        }
        AgentTaskControlResult::Applied {
            outcome,
            ledger_store_version,
            state,
            reopened_step_count,
            interrupted_tool_attempts,
        } => AgentTaskControlResultV1::Applied {
            outcome: match outcome {
                a3_application::AgentRecoveryOutcomeKind::Resumed => {
                    AgentTaskControlOutcomeV1::Resumed
                }
                a3_application::AgentRecoveryOutcomeKind::ReplanRequired => {
                    AgentTaskControlOutcomeV1::ReplanRequired
                }
                a3_application::AgentRecoveryOutcomeKind::Cancelled => {
                    AgentTaskControlOutcomeV1::Cancelled
                }
            },
            ledger_store_version: ledger_store_version.get().to_string(),
            state: map_agent_controller_state_to_v1(state),
            reopened_step_count,
            interrupted_tool_attempts,
            runtime_start,
        },
    }
}

fn map_agent_activity_run_to_v1(
    activity: &a3_application::AgentActivityRun,
    current_ledger_revision: u32,
) -> Option<AgentActivityRunV1> {
    let run = activity.run();
    let budget = run.budget();
    let usage = run.usage();
    let elapsed_at_last_event = run
        .updated_at()
        .unix_millis()
        .checked_sub(run.created_at().unix_millis())?;
    Some(AgentActivityRunV1::new(
        run.id().to_string(),
        activity.step_id().to_string(),
        activity.attempt_number(),
        run.task_ledger_revision().get(),
        run.task_ledger_revision().get() == current_ledger_revision,
        map_agent_controller_state_to_v1(run.state()),
        run.state().is_terminal(),
        run.current_snapshot_id().to_string(),
        run.created_at().unix_millis().to_string(),
        run.updated_at().unix_millis().to_string(),
        AgentActivityBudgetV1::new(
            budget.turn_limit().get(),
            budget.prompt_token_limit().get().to_string(),
            budget.output_token_limit().get().to_string(),
            budget.action_limit().get(),
            budget.duration_limit().millis().to_string(),
            budget.repair_limit().get(),
        ),
        AgentActivityUsageV1::new(
            usage.turn_count(),
            usage.prompt_tokens().to_string(),
            usage.output_tokens().to_string(),
            usage.action_count(),
            elapsed_at_last_event.to_string(),
            usage.repair_count(),
        ),
        activity.earlier_events_omitted(),
        activity
            .events()
            .iter()
            .map(map_agent_event_to_v1)
            .collect(),
    ))
}

fn map_agent_event_to_v1(event: &RunEvent) -> AgentActivityEventV1 {
    AgentActivityEventV1::new(
        event.sequence().get().to_string(),
        event.occurred_at().unix_millis().to_string(),
        event.snapshot_id().to_string(),
        match event.kind() {
            RunEventKind::RunStarted => AgentActivityEventKindV1::RunStarted,
            RunEventKind::StateTransition { from, to } => {
                AgentActivityEventKindV1::StateTransition {
                    from: map_agent_controller_state_to_v1(from),
                    to: map_agent_controller_state_to_v1(to),
                }
            }
            RunEventKind::ContextCompiled => AgentActivityEventKindV1::ContextCompiled,
            RunEventKind::ModelInteraction => AgentActivityEventKindV1::ModelInteraction {
                turn: event.turn_charge().map(|charge| {
                    AgentActivityTurnV1::new(
                        charge.action().map(map_agent_selected_action_to_v1),
                        charge.prompt_tokens().get(),
                        charge.output_tokens().get(),
                        matches!(charge.repair(), AgentTurnRepairUsage::One),
                    )
                }),
            },
            RunEventKind::ToolAction => AgentActivityEventKindV1::ToolAction,
            RunEventKind::LedgerUpdated { from, to } => AgentActivityEventKindV1::LedgerUpdated {
                from_revision: from.get(),
                to_revision: to.get(),
            },
            RunEventKind::VerificationRecorded => AgentActivityEventKindV1::VerificationRecorded,
            RunEventKind::ApprovalRecorded => AgentActivityEventKindV1::ApprovalRecorded,
            RunEventKind::Diagnostic => AgentActivityEventKindV1::Diagnostic,
        },
        map_agent_event_code_to_v1(event.payload().code()),
        event.payload().outcome().map(map_agent_event_outcome_to_v1),
    )
}

const fn map_agent_controller_state_to_v1(state: AgentControllerState) -> AgentControllerStateV1 {
    match state {
        AgentControllerState::Intake => AgentControllerStateV1::Intake,
        AgentControllerState::Localize => AgentControllerStateV1::Localize,
        AgentControllerState::Plan => AgentControllerStateV1::Plan,
        AgentControllerState::Execute => AgentControllerStateV1::Execute,
        AgentControllerState::Verify => AgentControllerStateV1::Verify,
        AgentControllerState::Replan => AgentControllerStateV1::Replan,
        AgentControllerState::AwaitApproval => AgentControllerStateV1::AwaitApproval,
        AgentControllerState::Done => AgentControllerStateV1::Done,
        AgentControllerState::Failed => AgentControllerStateV1::Failed,
        AgentControllerState::Cancelled => AgentControllerStateV1::Cancelled,
    }
}

const fn map_agent_selected_action_to_v1(action: AgentTurnActionClass) -> AgentSelectedActionV1 {
    match action {
        AgentTurnActionClass::Search => AgentSelectedActionV1::Search,
        AgentTurnActionClass::Inspect => AgentSelectedActionV1::Inspect,
        AgentTurnActionClass::UpdateLedger => AgentSelectedActionV1::UpdateLedger,
        AgentTurnActionClass::Finish => AgentSelectedActionV1::Finish,
        AgentTurnActionClass::ApplyPatch => AgentSelectedActionV1::ApplyPatch,
        AgentTurnActionClass::Run => AgentSelectedActionV1::Run,
    }
}

const fn map_agent_event_code_to_v1(code: RunEventCode) -> AgentActivityCodeV1 {
    match code {
        RunEventCode::None => AgentActivityCodeV1::None,
        RunEventCode::UserRequest => AgentActivityCodeV1::UserRequest,
        RunEventCode::ControllerDecision => AgentActivityCodeV1::ControllerDecision,
        RunEventCode::PolicyDecision => AgentActivityCodeV1::PolicyDecision,
        RunEventCode::Timeout => AgentActivityCodeV1::Timeout,
        RunEventCode::Cancellation => AgentActivityCodeV1::Cancellation,
        RunEventCode::InvalidModelOutput => AgentActivityCodeV1::InvalidModelOutput,
        RunEventCode::ToolFailure => AgentActivityCodeV1::ToolFailure,
        RunEventCode::VerificationFailure => AgentActivityCodeV1::VerificationFailure,
        RunEventCode::StateRecovered => AgentActivityCodeV1::StateRecovered,
    }
}

const fn map_agent_event_outcome_to_v1(outcome: RunEventOutcome) -> AgentActivityOutcomeV1 {
    match outcome {
        RunEventOutcome::Succeeded => AgentActivityOutcomeV1::Succeeded,
        RunEventOutcome::Failed => AgentActivityOutcomeV1::Failed,
        RunEventOutcome::Cancelled => AgentActivityOutcomeV1::Cancelled,
        RunEventOutcome::Denied => AgentActivityOutcomeV1::Denied,
    }
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

fn parse_canonical_positive_u64(value: &str) -> Result<u64, ()> {
    if value.is_empty()
        || value == "0"
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(());
    }
    value.parse::<u64>().map_err(|_| ())
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

fn map_worktree_id_from_v1(value: &str) -> Result<WorktreeId, CommandErrorV1> {
    let bytes: [u8; 32] = decode_hex(value, 32)
        .and_then(|bytes| bytes.try_into().map_err(|_| ()))
        .map_err(|_| CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectCatalogRequest))?;
    Ok(WorktreeId::from_bytes(bytes))
}

fn map_project_catalog_query_from_v1(
    request: &a3_protocol::QueryProjectCatalogRequestV1,
) -> Result<ProjectCatalogQuery, CommandErrorV1> {
    let cursor = request
        .cursor()
        .map(|value| {
            if value.len() != 16
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(CommandErrorV1::project_open(
                    ErrorCodeV1::InvalidProjectCatalogRequest,
                ));
            }
            u64::from_str_radix(value, 16)
                .map_err(|_| {
                    CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectCatalogRequest)
                })
                .and_then(|value| {
                    a3_application::ProjectCatalogCursor::new(value).map_err(|_| {
                        CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectCatalogRequest)
                    })
                })
        })
        .transpose()?;
    let direction = match request.direction() {
        a3_protocol::ProjectCatalogDirectionV1::Initial => {
            a3_application::ProjectCatalogDirection::Initial
        }
        a3_protocol::ProjectCatalogDirectionV1::Next => {
            a3_application::ProjectCatalogDirection::Next
        }
        a3_protocol::ProjectCatalogDirectionV1::Previous => {
            a3_application::ProjectCatalogDirection::Previous
        }
    };
    ProjectCatalogQuery::new(request.search().map(str::to_owned), cursor, direction)
        .map_err(|_| CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectCatalogRequest))
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

fn invalid_project_map_source_preview_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectMapSourcePreviewQuery)
}

fn invalid_project_map_scene_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidProjectMapSceneQuery)
}

const fn map_deep_map_budget_to_v1(budget: ExploreBudget) -> DeepMapBudgetV1 {
    DeepMapBudgetV1::new(budget.tokens(), budget.milliseconds(), budget.tool_calls())
}

fn map_deep_map_model_to_v1(model: &a3_application::DeepMapModelDescriptor) -> DeepMapModelV1 {
    DeepMapModelV1::new(
        model.profile().id().to_string(),
        model.profile().version().get(),
        model.provider_id().to_owned(),
        model.model_id().to_owned(),
        model.context_tokens(),
        model.output_tokens(),
    )
}

fn map_deep_map_lifecycle_to_v3(activity: &DeepMapActivity) -> DeepMapLifecycleV3 {
    if activity.state() == DeepMapActivityState::Idle {
        return DeepMapLifecycleV3::Ready;
    }
    let progress = DeepMapCompactProgressV3::new(
        activity.completed_steps().to_string(),
        activity.total_steps().to_string(),
        activity.phase().map(map_deep_map_phase_to_v2),
        activity.safe_action().map(map_deep_map_safe_action_to_v2),
    );
    let details_incomplete = activity.details_incomplete();
    match activity.state() {
        DeepMapActivityState::Idle => DeepMapLifecycleV3::Ready,
        DeepMapActivityState::Queued => DeepMapLifecycleV3::Queued {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Running => DeepMapLifecycleV3::Running {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Pausing => DeepMapLifecycleV3::Pausing {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Paused => DeepMapLifecycleV3::Paused {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Cancelling => DeepMapLifecycleV3::Cancelling {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Succeeded => DeepMapLifecycleV3::Succeeded {
            progress,
            details_incomplete,
        },
        DeepMapActivityState::Failed => DeepMapLifecycleV3::Failed {
            progress,
            failure: activity
                .failure()
                .map(map_deep_map_failure_to_v3)
                .unwrap_or(DeepMapFailureV3::ProgressUnavailable),
            details_incomplete,
        },
        DeepMapActivityState::Cancelled => DeepMapLifecycleV3::Cancelled {
            progress,
            details_incomplete,
        },
    }
}

fn publication_read_failure_lifecycle(lifecycle: DeepMapLifecycleV3) -> DeepMapLifecycleV3 {
    // Publication-state availability and execution success are independent signals. Keep this
    // policy explicit so a read failure cannot silently become a terminal Deep-Map failure again.
    lifecycle
}

fn map_deep_map_run_page_to_v1(
    worktree_id: WorktreeId,
    page: &a3_application::DeepMapRunPage,
) -> DeepMapRunPageResponseV1 {
    DeepMapRunPageResponseV1 {
        protocol_version: a3_protocol::ProtocolVersion::CURRENT,
        runs: page
            .runs()
            .iter()
            .map(|run| map_deep_map_run_to_v1(worktree_id, run))
            .collect(),
        next_cursor: page
            .next_cursor()
            .map(|cursor| encode_deep_map_run_cursor(worktree_id, cursor)),
    }
}

fn map_deep_map_entry_page_to_v1(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    page: &a3_application::DeepMapEntryPage,
) -> DeepMapEntryPageResponseV1 {
    DeepMapEntryPageResponseV1 {
        protocol_version: a3_protocol::ProtocolVersion::CURRENT,
        entries: page
            .entries()
            .iter()
            .copied()
            .map(|entry| map_deep_map_entry_to_v1(worktree_id, run_id, entry))
            .collect(),
        next_cursor: page
            .next_before_sequence()
            .map(|sequence| encode_deep_map_entry_selection(worktree_id, run_id, sequence)),
    }
}

fn map_deep_map_run_to_v1(worktree_id: WorktreeId, run: &DeepMapRunSummary) -> DeepMapRunV1 {
    DeepMapRunV1 {
        selection: encode_deep_map_run_selection(worktree_id, run.start().id()),
        mode: map_deep_map_mode_to_v2(run.start().mode()),
        state: deep_map_run_state_label(run.state()).to_owned(),
        started_at_unix_millis: run.start().created_at().unix_millis().to_string(),
        updated_at_unix_millis: run.updated_at().unix_millis().to_string(),
        confirmed_steps: run.confirmed_steps().to_string(),
        total_steps: run.total_steps().to_string(),
        failure: run.diagnostic().map(map_deep_map_diagnostic_to_v3),
        details_incomplete: run.details_incomplete(),
    }
}

fn map_deep_map_entry_to_v1(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    entry: DeepMapJournalEvent,
) -> DeepMapEntryV1 {
    DeepMapEntryV1 {
        selection: encode_deep_map_entry_selection(worktree_id, run_id, entry.sequence()),
        sequence: entry.sequence().get().to_string(),
        state: deep_map_run_state_label(entry.state()).to_owned(),
        occurred_at_unix_millis: entry.occurred_at().unix_millis().to_string(),
        phase: entry.phase().map(map_deep_map_phase_to_v2),
        action: entry.action().map(map_deep_map_safe_action_to_v2),
        target_kind: entry.target_kind().map(map_deep_map_target_kind_to_v2),
        step_position: entry.step_position().map(|value| value.to_string()),
        total_steps: entry.total_steps().map(|value| value.to_string()),
        confirmed: entry.confirmed(),
        result: deep_map_event_result_label(entry.result()).to_owned(),
        failure: entry.diagnostic().map(map_deep_map_diagnostic_to_v3),
    }
}

fn map_deep_map_entry_detail_to_v1(
    worktree_id: WorktreeId,
    detail: &a3_application::DeepMapEntryDetail,
) -> Result<DeepMapEntryDetailResponseV1, CommandErrorV1> {
    let run = detail.run();
    let entry = detail.event();
    let duration = entry
        .occurred_at()
        .unix_millis()
        .checked_sub(run.start().created_at().unix_millis())
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable))?;
    let model = run.start().model();
    let budget = run.start().mode().budget();
    Ok(DeepMapEntryDetailResponseV1 {
        protocol_version: a3_protocol::ProtocolVersion::CURRENT,
        run: map_deep_map_run_to_v1(worktree_id, run),
        entry: map_deep_map_entry_to_v1(worktree_id, run.start().id(), entry),
        duration_millis: duration.to_string(),
        provider_id: model.provider_id().to_owned(),
        model_id: model.model_id().to_owned(),
        profile_id: model.profile().id().to_string(),
        profile_version: model.profile().version().get(),
        token_budget: budget.tokens(),
        time_budget_millis: budget.milliseconds().to_string(),
        tool_call_budget: budget.tool_calls(),
        index_reference: short_reference(run.start().anchor().index_run_id().as_bytes()),
        snapshot_reference: short_reference(run.start().anchor().snapshot_id().as_bytes()),
        next_action: entry
            .diagnostic()
            .or(run.diagnostic())
            .map(deep_map_next_action)
            .map(str::to_owned),
        plan_stop_reason: run
            .plan_stop_reason()
            .map(deep_map_plan_stop_label)
            .map(str::to_owned),
        publication_result: run
            .publication_result()
            .map(deep_map_publication_result_label)
            .map(str::to_owned),
        step: detail.step().map(map_deep_map_step_detail_to_v1),
    })
}

fn map_deep_map_step_detail_to_v1(
    step: a3_application::DeepMapStepDetail,
) -> a3_protocol::DeepMapStepDetailV1 {
    let cost = step.reserved_cost();
    a3_protocol::DeepMapStepDetailV1 {
        target_kind: map_deep_map_target_kind_to_v2(step.target_kind()),
        seed_reason: deep_map_seed_reason_label(step.seed_reason()).to_owned(),
        reserved_tokens: cost.tokens(),
        reserved_time_millis: cost.milliseconds().to_string(),
        reserved_tool_calls: cost.tool_calls(),
        information_gain_basis_points: step.information_gain_basis_points(),
        coverage_field_count: step.coverage_field_count(),
        evidence_requirement: "fieldEvidence".to_owned(),
        verification_method: "publishedIndexEvidence".to_owned(),
        confirmed: step.confirmed(),
    }
}

fn deep_map_dashboard_unavailable() -> CommandErrorV1 {
    CommandErrorV1::deep_map(ErrorCodeV1::DeepMapUnavailable)
}

async fn first_plan_step(
    journal: &dyn DeepMapRunJournalStore,
    project: &ProjectIdentity,
    run_id: DeepMapRunId,
) -> Result<Option<a3_application::DeepMapPlanStep>, CommandErrorV1> {
    let modules = journal
        .list_run_modules(project, run_id, None)
        .await
        .map_err(|_| deep_map_dashboard_unavailable())?;
    let Some(module) = modules.modules().first() else {
        return Ok(None);
    };
    journal
        .list_module_steps(project, run_id, module.module_id(), None)
        .await
        .map_err(|_| deep_map_dashboard_unavailable())
        .map(|page| page.steps().first().cloned())
}

const fn map_dashboard_state(
    value: a3_application::DeepMapDashboardState,
) -> DeepMapDashboardStateV1 {
    match value {
        a3_application::DeepMapDashboardState::Queued => DeepMapDashboardStateV1::Queued,
        a3_application::DeepMapDashboardState::Running => DeepMapDashboardStateV1::Running,
        a3_application::DeepMapDashboardState::Pausing => DeepMapDashboardStateV1::Pausing,
        a3_application::DeepMapDashboardState::Paused => DeepMapDashboardStateV1::Paused,
        a3_application::DeepMapDashboardState::Cancelling => DeepMapDashboardStateV1::Cancelling,
        a3_application::DeepMapDashboardState::Completed => DeepMapDashboardStateV1::Completed,
        a3_application::DeepMapDashboardState::AlreadyCurrent => {
            DeepMapDashboardStateV1::AlreadyCurrent
        }
        a3_application::DeepMapDashboardState::Cancelled => DeepMapDashboardStateV1::Cancelled,
        a3_application::DeepMapDashboardState::Failed => DeepMapDashboardStateV1::Failed,
        a3_application::DeepMapDashboardState::Interrupted => DeepMapDashboardStateV1::Interrupted,
    }
}

const fn map_dashboard_freshness(
    value: a3_application::DeepMapDashboardFreshness,
) -> DeepMapDashboardFreshnessV1 {
    match value {
        a3_application::DeepMapDashboardFreshness::Current => DeepMapDashboardFreshnessV1::Current,
        a3_application::DeepMapDashboardFreshness::Historical => {
            DeepMapDashboardFreshnessV1::Historical
        }
    }
}

const fn map_dashboard_phase(
    value: a3_application::DeepMapDashboardPhase,
) -> DeepMapDashboardPhaseV1 {
    match value {
        a3_application::DeepMapDashboardPhase::Planning => DeepMapDashboardPhaseV1::Planning,
        a3_application::DeepMapDashboardPhase::Exploring => DeepMapDashboardPhaseV1::Exploring,
        a3_application::DeepMapDashboardPhase::CreatingCards => {
            DeepMapDashboardPhaseV1::CreatingCards
        }
        a3_application::DeepMapDashboardPhase::Verifying => DeepMapDashboardPhaseV1::Verifying,
        a3_application::DeepMapDashboardPhase::UpdatingAtlas => {
            DeepMapDashboardPhaseV1::UpdatingAtlas
        }
    }
}

const fn map_dashboard_phase_state(
    value: a3_application::DeepMapDashboardPhaseState,
) -> DeepMapDashboardPhaseStateV1 {
    match value {
        a3_application::DeepMapDashboardPhaseState::Pending => {
            DeepMapDashboardPhaseStateV1::Pending
        }
        a3_application::DeepMapDashboardPhaseState::Active => DeepMapDashboardPhaseStateV1::Active,
        a3_application::DeepMapDashboardPhaseState::Completed => {
            DeepMapDashboardPhaseStateV1::Completed
        }
        a3_application::DeepMapDashboardPhaseState::Stopped => {
            DeepMapDashboardPhaseStateV1::Stopped
        }
    }
}

const fn map_dashboard_module_state(
    value: a3_application::DeepMapDashboardModuleState,
) -> DeepMapModuleStateV1 {
    match value {
        a3_application::DeepMapDashboardModuleState::Planned => DeepMapModuleStateV1::Planned,
        a3_application::DeepMapDashboardModuleState::Exploring => DeepMapModuleStateV1::Exploring,
        a3_application::DeepMapDashboardModuleState::Verifying => DeepMapModuleStateV1::Verifying,
        a3_application::DeepMapDashboardModuleState::Published => DeepMapModuleStateV1::Published,
        a3_application::DeepMapDashboardModuleState::Incomplete => DeepMapModuleStateV1::Incomplete,
    }
}

const fn map_deep_map_selection_reason(
    value: a3_domain::ExploreSeedReason,
) -> DeepMapSelectionReasonV1 {
    match value {
        a3_domain::ExploreSeedReason::Manifest => DeepMapSelectionReasonV1::Manifest,
        a3_domain::ExploreSeedReason::Entrypoint => DeepMapSelectionReasonV1::Entrypoint,
        a3_domain::ExploreSeedReason::CentralSymbol => DeepMapSelectionReasonV1::CentralSymbol,
        a3_domain::ExploreSeedReason::TestRoot => DeepMapSelectionReasonV1::TestRoot,
        a3_domain::ExploreSeedReason::GraphCommunity => DeepMapSelectionReasonV1::GraphCommunity,
        a3_domain::ExploreSeedReason::UncoveredModule => DeepMapSelectionReasonV1::UncoveredModule,
    }
}

const fn map_deep_map_card_field(value: ModuleCardField) -> DeepMapCardFieldV1 {
    match value {
        ModuleCardField::Title => DeepMapCardFieldV1::Title,
        ModuleCardField::Paths => DeepMapCardFieldV1::Paths,
        ModuleCardField::Purpose => DeepMapCardFieldV1::Purpose,
        ModuleCardField::Responsibilities => DeepMapCardFieldV1::Responsibilities,
        ModuleCardField::PublicSurface => DeepMapCardFieldV1::PublicSurface,
        ModuleCardField::Entrypoints => DeepMapCardFieldV1::Entrypoints,
        ModuleCardField::Dependencies => DeepMapCardFieldV1::Dependencies,
        ModuleCardField::DataFlows => DeepMapCardFieldV1::DataFlows,
        ModuleCardField::Invariants => DeepMapCardFieldV1::Invariants,
        ModuleCardField::Tests => DeepMapCardFieldV1::Tests,
        ModuleCardField::Risks => DeepMapCardFieldV1::Risks,
        ModuleCardField::OpenQuestions => DeepMapCardFieldV1::OpenQuestions,
    }
}

fn deep_map_module_display_name(
    index: &a3_domain::PublishedIndex,
    module_id: ModuleId,
) -> Option<String> {
    index
        .publication()
        .modules()
        .modules()
        .iter()
        .find(|module| module.id() == module_id && module.kind().is_primary())
        .map(|module| match module.root() {
            Some(ModuleRoot::Repository) => "Repository".to_owned(),
            Some(ModuleRoot::Directory(path)) => path
                .as_bytes()
                .rsplit(|byte| *byte == b'/')
                .next()
                .map(safe_path_display)
                .unwrap_or_else(|| "Modul".to_owned()),
            None => "Modul".to_owned(),
        })
}

fn resolve_deep_map_target_label(
    index: &a3_domain::PublishedIndex,
    step: &a3_application::DeepMapPlanStep,
) -> Option<String> {
    match step.target_reference()? {
        a3_application::DeepMapPlanTargetReference::Module(module_id) => {
            deep_map_module_display_name(index, module_id)
        }
        a3_application::DeepMapPlanTargetReference::FileEvidence(evidence_id) => index
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| ModuleCardEvidenceId::for_file_revision_v1(revision) == evidence_id)
            .map(|revision| safe_path_display(revision.path().as_bytes())),
        a3_application::DeepMapPlanTargetReference::Symbol(symbol_id) => index
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == symbol_id)
            .map(|symbol| {
                format!(
                    "{} · {}",
                    symbol.parsed().name().as_str(),
                    safe_path_display(symbol.revision().path().as_bytes())
                )
            }),
    }
}

fn safe_path_display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn deep_map_diagnostic_code(value: DeepMapDiagnosticCode) -> &'static str {
    match value {
        DeepMapDiagnosticCode::NoPublishedIndex => "DM-NO-INDEX",
        DeepMapDiagnosticCode::StaleIndex => "DM-STALE-INDEX",
        DeepMapDiagnosticCode::Planning => "DM-PLAN",
        DeepMapDiagnosticCode::ModelUnavailable => "DM-MODEL-OFFLINE",
        DeepMapDiagnosticCode::ModelRejected => "DM-MODEL-REJECTED",
        DeepMapDiagnosticCode::ModelTimeout => "DM-MODEL-TIMEOUT",
        DeepMapDiagnosticCode::InvalidModelResponse => "DM-INVALID-RESPONSE",
        DeepMapDiagnosticCode::Read => "DM-READ",
        DeepMapDiagnosticCode::Verification => "DM-VERIFY",
        DeepMapDiagnosticCode::PublicationRejected => "DM-PUBLISH-REJECTED",
        DeepMapDiagnosticCode::PublicationStorage => "DM-PUBLISH-STORAGE",
        DeepMapDiagnosticCode::PublicationTimeout => "DM-PUBLISH-TIMEOUT",
        DeepMapDiagnosticCode::PublicationProgress => "DM-PUBLISH-PROGRESS",
        DeepMapDiagnosticCode::InvalidCheckpoint => "DM-CHECKPOINT",
        DeepMapDiagnosticCode::ProgressUnavailable => "DM-PROGRESS",
        DeepMapDiagnosticCode::Interrupted => "DM-INTERRUPTED",
    }
}

struct DeepMapAtlasImpactProjection {
    summary: DeepMapAtlasImpactSummaryV1,
    items: Vec<DeepMapAtlasImpactItemV1>,
}

fn build_deep_map_atlas_impact(
    index: &a3_domain::PublishedIndex,
    card: &ModuleCardDetail,
) -> Result<DeepMapAtlasImpactProjection, CommandErrorV1> {
    let mut claims_by_evidence =
        BTreeMap::<ModuleCardEvidenceId, BTreeSet<a3_domain::ModuleCardClaimId>>::new();
    for value in card
        .fields()
        .iter()
        .flat_map(a3_application::ModuleCardDetailField::values)
    {
        if value.claim().state() != ModuleCardClaimState::Current {
            continue;
        }
        for evidence_id in value.claim().evidence_ids() {
            claims_by_evidence
                .entry(*evidence_id)
                .or_default()
                .insert(value.claim().id());
        }
    }
    let purpose = card
        .fields()
        .iter()
        .find(|field| field.field() == ModuleCardField::Purpose)
        .and_then(|field| field.values().first())
        .map(|value| value.value().to_owned());
    let risk_count = card
        .fields()
        .iter()
        .find(|field| field.field() == ModuleCardField::Risks)
        .map_or(0_usize, |field| field.values().len());
    let mut items = Vec::new();
    let mut file_count = 0_u64;
    let mut symbol_count = 0_u64;
    let mut relation_count = 0_u64;
    for revision in index.publication().graph().files() {
        let evidence_id = ModuleCardEvidenceId::for_file_revision_v1(revision);
        if let Some(claims) = claims_by_evidence.get(&evidence_id) {
            file_count = file_count.saturating_add(1);
            items.push(DeepMapAtlasImpactItemV1 {
                kind: DeepMapAtlasImpactKindV1::File,
                label: safe_path_display(revision.path().as_bytes()),
                confirmed_claim_count: claims.len().to_string(),
            });
        }
    }
    for symbol in index.publication().graph().symbols() {
        let evidence_id = ModuleCardEvidenceId::for_symbol_v1(symbol);
        if let Some(claims) = claims_by_evidence.get(&evidence_id) {
            symbol_count = symbol_count.saturating_add(1);
            items.push(DeepMapAtlasImpactItemV1 {
                kind: DeepMapAtlasImpactKindV1::Symbol,
                label: format!(
                    "{} · {}",
                    symbol.parsed().name().as_str(),
                    safe_path_display(symbol.revision().path().as_bytes())
                ),
                confirmed_claim_count: claims.len().to_string(),
            });
        }
    }
    for edge in index.publication().graph().edges() {
        let evidence_id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
        if let Some(claims) = claims_by_evidence.get(&evidence_id) {
            relation_count = relation_count.saturating_add(1);
            items.push(DeepMapAtlasImpactItemV1 {
                kind: DeepMapAtlasImpactKindV1::Relation,
                label: format!(
                    "{} → {} · {}",
                    graph_endpoint_display(index, edge.source()),
                    graph_endpoint_display(index, edge.target()),
                    syntax_relation_display(edge.kind())
                ),
                confirmed_claim_count: claims.len().to_string(),
            });
        }
    }
    Ok(DeepMapAtlasImpactProjection {
        summary: DeepMapAtlasImpactSummaryV1 {
            purpose,
            risk_count: risk_count.to_string(),
            file_count: file_count.to_string(),
            symbol_count: symbol_count.to_string(),
            relation_count: relation_count.to_string(),
        },
        items,
    })
}

fn graph_endpoint_display(index: &a3_domain::PublishedIndex, endpoint: &GraphEndpoint) -> String {
    match endpoint {
        GraphEndpoint::File(path) => safe_path_display(path.as_bytes()),
        GraphEndpoint::Symbol(symbol_id) => index
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == *symbol_id)
            .map(|symbol| symbol.parsed().name().as_str().to_owned())
            .unwrap_or_else(|| "Symbol".to_owned()),
    }
}

const fn syntax_relation_display(value: SyntaxRelationKind) -> &'static str {
    match value {
        SyntaxRelationKind::Contains => "enthält",
        SyntaxRelationKind::Defines => "definiert",
        SyntaxRelationKind::Imports => "importiert",
        SyntaxRelationKind::Exports => "exportiert",
        SyntaxRelationKind::Calls => "ruft auf",
        SyntaxRelationKind::Implements => "implementiert",
        SyntaxRelationKind::Extends => "erweitert",
        SyntaxRelationKind::Reads => "liest",
        SyntaxRelationKind::Writes => "schreibt",
        SyntaxRelationKind::Configures => "konfiguriert",
        SyntaxRelationKind::Tests => "testet",
        SyntaxRelationKind::Builds => "baut",
        SyntaxRelationKind::Documents => "dokumentiert",
    }
}

const fn deep_map_plan_stop_label(value: a3_domain::ExplorePlanStopReason) -> &'static str {
    match value {
        a3_domain::ExplorePlanStopReason::CoveragePlanned => "coveragePlanned",
        a3_domain::ExplorePlanStopReason::BudgetExhausted => "budgetExhausted",
        a3_domain::ExplorePlanStopReason::BelowInformationGainThreshold => "belowGainThreshold",
        a3_domain::ExplorePlanStopReason::NoEligibleSeed => "noEligibleSeed",
    }
}

const fn deep_map_publication_result_label(
    value: a3_application::DeepMapPublicationResult,
) -> &'static str {
    match value {
        a3_application::DeepMapPublicationResult::Published => "published",
        a3_application::DeepMapPublicationResult::AlreadyCurrent => "alreadyCurrent",
    }
}

const fn deep_map_seed_reason_label(value: a3_domain::ExploreSeedReason) -> &'static str {
    match value {
        a3_domain::ExploreSeedReason::Manifest => "manifest",
        a3_domain::ExploreSeedReason::Entrypoint => "entrypoint",
        a3_domain::ExploreSeedReason::CentralSymbol => "centralSymbol",
        a3_domain::ExploreSeedReason::TestRoot => "testRoot",
        a3_domain::ExploreSeedReason::GraphCommunity => "graphCommunity",
        a3_domain::ExploreSeedReason::UncoveredModule => "uncoveredModule",
    }
}

const fn map_deep_map_mode_to_v2(mode: DeepMapMode) -> DeepMapModeV2 {
    match mode {
        DeepMapMode::Fast => DeepMapModeV2::Fast,
        DeepMapMode::Standard => DeepMapModeV2::Standard,
        DeepMapMode::Thorough => DeepMapModeV2::Thorough,
    }
}

const fn deep_map_run_state_label(state: a3_domain::DeepMapRunState) -> &'static str {
    match state {
        a3_domain::DeepMapRunState::Queued => "queued",
        a3_domain::DeepMapRunState::Running => "running",
        a3_domain::DeepMapRunState::Pausing => "pausing",
        a3_domain::DeepMapRunState::Paused => "paused",
        a3_domain::DeepMapRunState::Cancelling => "cancelling",
        a3_domain::DeepMapRunState::Succeeded => "succeeded",
        a3_domain::DeepMapRunState::Failed => "failed",
        a3_domain::DeepMapRunState::Cancelled => "cancelled",
        a3_domain::DeepMapRunState::Interrupted => "interrupted",
    }
}

const fn deep_map_event_result_label(result: a3_application::DeepMapEventResult) -> &'static str {
    match result {
        a3_application::DeepMapEventResult::Pending => "pending",
        a3_application::DeepMapEventResult::Confirmed => "confirmed",
        a3_application::DeepMapEventResult::AlreadyCurrent => "alreadyCurrent",
        a3_application::DeepMapEventResult::Published => "published",
        a3_application::DeepMapEventResult::Paused => "paused",
        a3_application::DeepMapEventResult::Resumed => "resumed",
        a3_application::DeepMapEventResult::Cancelled => "cancelled",
        a3_application::DeepMapEventResult::Failed => "failed",
        a3_application::DeepMapEventResult::Interrupted => "interrupted",
    }
}

const fn map_deep_map_failure_to_v3(failure: DeepMapExecutionFailure) -> DeepMapFailureV3 {
    match failure {
        DeepMapExecutionFailure::NoPublishedIndex => DeepMapFailureV3::NoPublishedIndex,
        DeepMapExecutionFailure::StaleSnapshot => DeepMapFailureV3::StaleIndex,
        DeepMapExecutionFailure::Planning => DeepMapFailureV3::Planning,
        DeepMapExecutionFailure::ModelUnavailable => DeepMapFailureV3::ModelUnavailable,
        DeepMapExecutionFailure::ModelRejected => DeepMapFailureV3::ModelRejected,
        DeepMapExecutionFailure::ModelTimedOut => DeepMapFailureV3::ModelTimeout,
        DeepMapExecutionFailure::InvalidModelResponse => DeepMapFailureV3::InvalidModelResponse,
        DeepMapExecutionFailure::Read => DeepMapFailureV3::Read,
        DeepMapExecutionFailure::Verification => DeepMapFailureV3::Verification,
        DeepMapExecutionFailure::PublicationRejected => DeepMapFailureV3::PublicationRejected,
        DeepMapExecutionFailure::PublicationStorage => DeepMapFailureV3::PublicationStorage,
        DeepMapExecutionFailure::PublicationTimedOut => DeepMapFailureV3::PublicationTimeout,
        DeepMapExecutionFailure::PublicationProgressUnavailable => {
            DeepMapFailureV3::PublicationProgress
        }
        DeepMapExecutionFailure::InvalidCheckpoint => DeepMapFailureV3::InvalidCheckpoint,
        DeepMapExecutionFailure::ProgressUnavailable => DeepMapFailureV3::ProgressUnavailable,
    }
}

const fn map_deep_map_diagnostic_to_v3(code: DeepMapDiagnosticCode) -> DeepMapFailureV3 {
    match code {
        DeepMapDiagnosticCode::NoPublishedIndex => DeepMapFailureV3::NoPublishedIndex,
        DeepMapDiagnosticCode::StaleIndex => DeepMapFailureV3::StaleIndex,
        DeepMapDiagnosticCode::Planning => DeepMapFailureV3::Planning,
        DeepMapDiagnosticCode::ModelUnavailable => DeepMapFailureV3::ModelUnavailable,
        DeepMapDiagnosticCode::ModelRejected => DeepMapFailureV3::ModelRejected,
        DeepMapDiagnosticCode::ModelTimeout => DeepMapFailureV3::ModelTimeout,
        DeepMapDiagnosticCode::InvalidModelResponse => DeepMapFailureV3::InvalidModelResponse,
        DeepMapDiagnosticCode::Read => DeepMapFailureV3::Read,
        DeepMapDiagnosticCode::Verification => DeepMapFailureV3::Verification,
        DeepMapDiagnosticCode::PublicationRejected => DeepMapFailureV3::PublicationRejected,
        DeepMapDiagnosticCode::PublicationStorage => DeepMapFailureV3::PublicationStorage,
        DeepMapDiagnosticCode::PublicationTimeout => DeepMapFailureV3::PublicationTimeout,
        DeepMapDiagnosticCode::PublicationProgress => DeepMapFailureV3::PublicationProgress,
        DeepMapDiagnosticCode::InvalidCheckpoint => DeepMapFailureV3::InvalidCheckpoint,
        DeepMapDiagnosticCode::ProgressUnavailable => DeepMapFailureV3::ProgressUnavailable,
        DeepMapDiagnosticCode::Interrupted => DeepMapFailureV3::Interrupted,
    }
}

const fn deep_map_next_action(code: DeepMapDiagnosticCode) -> &'static str {
    match code {
        DeepMapDiagnosticCode::NoPublishedIndex => "Fast Index vollständig erstellen.",
        DeepMapDiagnosticCode::StaleIndex => "Status aktualisieren und Deep Map erneut starten.",
        DeepMapDiagnosticCode::ModelUnavailable => "Provider-Verbindung und Zugang prüfen.",
        DeepMapDiagnosticCode::ModelRejected => "Modellprofil und strukturierte Ausgabe prüfen.",
        DeepMapDiagnosticCode::ModelTimeout => "Provider prüfen oder einen kleineren Modus wählen.",
        DeepMapDiagnosticCode::PublicationStorage => "Lokalen Speicher prüfen und erneut starten.",
        DeepMapDiagnosticCode::PublicationTimeout => "Speicherlast prüfen und erneut starten.",
        DeepMapDiagnosticCode::PublicationProgress => {
            "Deep Map nach Statusaktualisierung erneut starten."
        }
        DeepMapDiagnosticCode::Interrupted => "Deep Map bei Bedarf erneut starten.",
        DeepMapDiagnosticCode::Planning
        | DeepMapDiagnosticCode::InvalidModelResponse
        | DeepMapDiagnosticCode::Read
        | DeepMapDiagnosticCode::Verification
        | DeepMapDiagnosticCode::PublicationRejected
        | DeepMapDiagnosticCode::InvalidCheckpoint
        | DeepMapDiagnosticCode::ProgressUnavailable => {
            "Details prüfen und Deep Map erneut starten."
        }
    }
}

fn short_reference(bytes: &[u8; 32]) -> String {
    encode_hex(&bytes[..6])
}

fn encode_deep_map_run_selection(worktree_id: WorktreeId, run_id: DeepMapRunId) -> String {
    let mut bytes = Vec::with_capacity(48);
    bytes.extend_from_slice(run_id.as_bytes());
    bytes.extend_from_slice(&deep_map_selection_tag(
        b"a3.deep-map.run-selection.v1\0",
        worktree_id,
        run_id,
        None,
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_run_selection(worktree_id: WorktreeId, value: &str) -> Result<DeepMapRunId, ()> {
    let bytes = decode_hex(value, 48)?;
    if bytes.len() != 48 {
        return Err(());
    }
    let run_id = DeepMapRunId::from_bytes(bytes[..32].try_into().map_err(|_| ())?);
    let expected =
        deep_map_selection_tag(b"a3.deep-map.run-selection.v1\0", worktree_id, run_id, None);
    if bytes[32..] != expected {
        return Err(());
    }
    Ok(run_id)
}

fn encode_deep_map_module_selection(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
) -> String {
    let mut bytes = Vec::with_capacity(48);
    bytes.extend_from_slice(module_id.as_bytes());
    bytes.extend_from_slice(&deep_map_module_tag(
        b"a3.deep-map.module-selection.v1\0",
        worktree_id,
        run_id,
        module_id,
        None,
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_module_selection(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    value: &str,
) -> Result<ModuleId, ()> {
    let bytes = decode_hex(value, 48)?;
    if bytes.len() != 48 {
        return Err(());
    }
    let module_id = ModuleId::from_bytes(bytes[..32].try_into().map_err(|_| ())?);
    let expected = deep_map_module_tag(
        b"a3.deep-map.module-selection.v1\0",
        worktree_id,
        run_id,
        module_id,
        None,
    );
    if bytes[32..] != expected {
        return Err(());
    }
    Ok(module_id)
}

fn encode_deep_map_module_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    cursor: a3_application::DeepMapModuleCursor,
) -> String {
    let module_id = cursor.module_id();
    let mut bytes = Vec::with_capacity(48);
    bytes.extend_from_slice(module_id.as_bytes());
    bytes.extend_from_slice(&deep_map_module_tag(
        b"a3.deep-map.module-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        None,
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_module_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    value: &str,
) -> Result<a3_application::DeepMapModuleCursor, ()> {
    let bytes = decode_hex(value, 48)?;
    if bytes.len() != 48 {
        return Err(());
    }
    let module_id = ModuleId::from_bytes(bytes[..32].try_into().map_err(|_| ())?);
    let expected = deep_map_module_tag(
        b"a3.deep-map.module-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        None,
    );
    if bytes[32..] != expected {
        return Err(());
    }
    Ok(a3_application::DeepMapModuleCursor::new(module_id))
}

fn encode_deep_map_step_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    position: u64,
) -> String {
    encode_deep_map_number_cursor(
        b"a3.deep-map.step-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        position,
    )
}

fn decode_deep_map_step_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    value: &str,
) -> Result<u64, ()> {
    decode_deep_map_number_cursor(
        b"a3.deep-map.step-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        value,
    )
    .and_then(|position| (position > 0).then_some(position).ok_or(()))
}

fn encode_deep_map_impact_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    offset: u64,
) -> String {
    encode_deep_map_number_cursor(
        b"a3.deep-map.impact-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        offset,
    )
}

fn decode_deep_map_impact_cursor(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    value: &str,
) -> Result<u64, ()> {
    decode_deep_map_number_cursor(
        b"a3.deep-map.impact-cursor.v1\0",
        worktree_id,
        run_id,
        module_id,
        value,
    )
}

fn encode_deep_map_number_cursor(
    domain: &[u8],
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    number: u64,
) -> String {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.extend_from_slice(&deep_map_module_tag(
        domain,
        worktree_id,
        run_id,
        module_id,
        Some(number),
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_number_cursor(
    domain: &[u8],
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    value: &str,
) -> Result<u64, ()> {
    let bytes = decode_hex(value, 24)?;
    if bytes.len() != 24 {
        return Err(());
    }
    let number = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| ())?);
    let expected = deep_map_module_tag(domain, worktree_id, run_id, module_id, Some(number));
    if bytes[8..] != expected {
        return Err(());
    }
    Ok(number)
}

fn deep_map_module_tag(
    domain: &[u8],
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    module_id: ModuleId,
    number: Option<u64>,
) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(worktree_id.as_bytes());
    hasher.update(run_id.as_bytes());
    hasher.update(module_id.as_bytes());
    if let Some(number) = number {
        hasher.update(&number.to_be_bytes());
    }
    let mut tag = [0_u8; 16];
    tag.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    tag
}

fn encode_deep_map_entry_selection(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    sequence: DeepMapEventSequence,
) -> String {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(&sequence.get().to_be_bytes());
    bytes.extend_from_slice(&deep_map_selection_tag(
        b"a3.deep-map.entry-selection.v1\0",
        worktree_id,
        run_id,
        Some(sequence.get()),
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_entry_selection(
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    value: &str,
) -> Result<DeepMapEventSequence, ()> {
    let bytes = decode_hex(value, 24)?;
    if bytes.len() != 24 {
        return Err(());
    }
    let sequence = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| ())?);
    let expected = deep_map_selection_tag(
        b"a3.deep-map.entry-selection.v1\0",
        worktree_id,
        run_id,
        Some(sequence),
    );
    if bytes[8..] != expected {
        return Err(());
    }
    DeepMapEventSequence::new(sequence).map_err(|_| ())
}

fn encode_deep_map_run_cursor(worktree_id: WorktreeId, cursor: DeepMapRunCursor) -> String {
    let mut bytes = Vec::with_capacity(56);
    bytes.extend_from_slice(&cursor.updated_at().unix_millis().to_be_bytes());
    bytes.extend_from_slice(cursor.run_id().as_bytes());
    bytes.extend_from_slice(&deep_map_selection_tag(
        b"a3.deep-map.run-cursor.v1\0",
        worktree_id,
        cursor.run_id(),
        Some(cursor.updated_at().unix_millis().unsigned_abs()),
    ));
    encode_hex(&bytes)
}

fn decode_deep_map_run_cursor(
    worktree_id: WorktreeId,
    value: &str,
) -> Result<DeepMapRunCursor, ()> {
    let bytes = decode_hex(value, 56)?;
    if bytes.len() != 56 {
        return Err(());
    }
    let timestamp = i64::from_be_bytes(bytes[..8].try_into().map_err(|_| ())?);
    let timestamp = a3_domain::DeepMapRunTimestamp::new(timestamp).map_err(|_| ())?;
    let run_id = DeepMapRunId::from_bytes(bytes[8..40].try_into().map_err(|_| ())?);
    let expected = deep_map_selection_tag(
        b"a3.deep-map.run-cursor.v1\0",
        worktree_id,
        run_id,
        Some(timestamp.unix_millis().unsigned_abs()),
    );
    if bytes[40..] != expected {
        return Err(());
    }
    Ok(DeepMapRunCursor::new(timestamp, run_id))
}

fn deep_map_selection_tag(
    domain: &[u8],
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    number: Option<u64>,
) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(worktree_id.as_bytes());
    hasher.update(run_id.as_bytes());
    if let Some(number) = number {
        hasher.update(&number.to_be_bytes());
    }
    let mut tag = [0_u8; 16];
    tag.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    tag
}

fn map_deep_map_activity_to_v2(activity: DeepMapActivity) -> DeepMapActivityV2 {
    let progress = activity.progress().and_then(|progress| {
        progress
            .completed()
            .zip(progress.total())
            .map(|(completed, total)| {
                DeepMapProgressV1::new(completed.to_string(), total.to_string())
            })
    });
    let events = activity
        .events()
        .copied()
        .map(|event| {
            let update = event.update();
            DeepMapEventV2::new(
                event.sequence().to_string(),
                map_deep_map_phase_to_v2(update.phase()),
                update.module_id().map(|module_id| module_id.to_string()),
                map_deep_map_target_kind_to_v2(update.target_kind()),
                map_deep_map_safe_action_to_v2(update.action()),
                update.step_position().map(|position| position.to_string()),
                update.total_steps().map(|total| total.to_string()),
                update.confirmed(),
            )
        })
        .collect();
    DeepMapActivityV2::new(
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
        activity.failure().map(map_deep_map_failure_to_v1),
        activity.completed_steps().to_string(),
        activity.total_steps().to_string(),
        activity.phase().map(map_deep_map_phase_to_v2),
        activity
            .current_module_id()
            .map(|module_id| module_id.to_string()),
        activity.target_kind().map(map_deep_map_target_kind_to_v2),
        activity.safe_action().map(map_deep_map_safe_action_to_v2),
        activity
            .step_position()
            .map(|position| position.to_string()),
        events,
        activity
            .publication_succeeded()
            .then_some(DeepMapPublicationSummaryV2::succeeded()),
    )
}

const fn map_deep_map_phase_to_v2(phase: DeepMapPhase) -> DeepMapPhaseV2 {
    match phase {
        DeepMapPhase::Planning => DeepMapPhaseV2::Planning,
        DeepMapPhase::Exploring => DeepMapPhaseV2::Exploring,
        DeepMapPhase::Claiming => DeepMapPhaseV2::Claiming,
        DeepMapPhase::Verifying => DeepMapPhaseV2::Verifying,
        DeepMapPhase::Publishing => DeepMapPhaseV2::Publishing,
    }
}

const fn map_deep_map_target_kind_to_v2(kind: DeepMapTargetKind) -> DeepMapTargetKindV2 {
    match kind {
        DeepMapTargetKind::Project => DeepMapTargetKindV2::Project,
        DeepMapTargetKind::Module => DeepMapTargetKindV2::Module,
        DeepMapTargetKind::Manifest => DeepMapTargetKindV2::Manifest,
        DeepMapTargetKind::Symbol => DeepMapTargetKindV2::Symbol,
    }
}

const fn map_deep_map_safe_action_to_v2(action: DeepMapSafeAction) -> DeepMapSafeActionV2 {
    match action {
        DeepMapSafeAction::BuildPlan => DeepMapSafeActionV2::BuildPlan,
        DeepMapSafeAction::Inspect => DeepMapSafeActionV2::Inspect,
        DeepMapSafeAction::Search => DeepMapSafeActionV2::Search,
        DeepMapSafeAction::Propose => DeepMapSafeActionV2::Propose,
        DeepMapSafeAction::GenerateClaims => DeepMapSafeActionV2::GenerateClaims,
        DeepMapSafeAction::VerifyEvidence => DeepMapSafeActionV2::VerifyEvidence,
        DeepMapSafeAction::PublishCards => DeepMapSafeActionV2::PublishCards,
    }
}

const fn map_deep_map_failure_to_v1(failure: DeepMapExecutionFailure) -> DeepMapFailureV1 {
    match failure {
        DeepMapExecutionFailure::NoPublishedIndex => DeepMapFailureV1::NoPublishedIndex,
        DeepMapExecutionFailure::StaleSnapshot => DeepMapFailureV1::StaleSnapshot,
        DeepMapExecutionFailure::Planning => DeepMapFailureV1::Planning,
        DeepMapExecutionFailure::ModelUnavailable => DeepMapFailureV1::ModelUnavailable,
        DeepMapExecutionFailure::ModelRejected => DeepMapFailureV1::ModelRejected,
        DeepMapExecutionFailure::ModelTimedOut => DeepMapFailureV1::ModelTimedOut,
        DeepMapExecutionFailure::InvalidModelResponse => DeepMapFailureV1::InvalidModelResponse,
        DeepMapExecutionFailure::Read => DeepMapFailureV1::Read,
        DeepMapExecutionFailure::Verification => DeepMapFailureV1::Verification,
        DeepMapExecutionFailure::PublicationRejected
        | DeepMapExecutionFailure::PublicationStorage
        | DeepMapExecutionFailure::PublicationTimedOut
        | DeepMapExecutionFailure::PublicationProgressUnavailable => DeepMapFailureV1::Publication,
        DeepMapExecutionFailure::InvalidCheckpoint => DeepMapFailureV1::InvalidCheckpoint,
        DeepMapExecutionFailure::ProgressUnavailable => DeepMapFailureV1::ProgressUnavailable,
    }
}

fn map_deep_map_control_error(error: DeepMapManagerControlError) -> CommandErrorV1 {
    let code = match error {
        DeepMapManagerControlError::NoActiveProject => ErrorCodeV1::NoActiveProject,
        DeepMapManagerControlError::NotRunning => ErrorCodeV1::DeepMapNotRunning,
        DeepMapManagerControlError::NotPaused => ErrorCodeV1::DeepMapNotPaused,
        DeepMapManagerControlError::AlreadyPending => ErrorCodeV1::DeepMapAlreadyPending,
        DeepMapManagerControlError::Unavailable
        | DeepMapManagerControlError::QueueFull
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

fn map_catalog_activation_error_to_v1(error: ActivateCatalogProjectError) -> CommandErrorV1 {
    let code = match error {
        ActivateCatalogProjectError::Storage(error) => map_storage_error_to_v1(error),
        ActivateCatalogProjectError::Inspection(ProjectInspectionFailure::SelectionUnavailable) => {
            ErrorCodeV1::ProjectSelectionUnavailable
        }
        ActivateCatalogProjectError::Inspection(ProjectInspectionFailure::NotRepository) => {
            ErrorCodeV1::NotGitRepository
        }
        ActivateCatalogProjectError::Inspection(ProjectInspectionFailure::NotWorktreeRoot) => {
            ErrorCodeV1::ProjectRootRequired
        }
        ActivateCatalogProjectError::Inspection(
            ProjectInspectionFailure::UnsupportedRepository,
        ) => ErrorCodeV1::UnsupportedRepository,
        ActivateCatalogProjectError::Inspection(
            ProjectInspectionFailure::InvalidRepositoryMetadata,
        )
        | ActivateCatalogProjectError::IdentityConflict => ErrorCodeV1::ProjectIdentityConflict,
        ActivateCatalogProjectError::NotFound => ErrorCodeV1::ProjectNotInList,
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

fn map_project_map_source_preview_error_to_v1(
    error: ProjectMapSourcePreviewFailure,
) -> CommandErrorV1 {
    match error {
        ProjectMapSourcePreviewFailure::Evidence(error) => {
            map_module_card_evidence_error_to_v1(error)
        }
        ProjectMapSourcePreviewFailure::IndexEvidence(error) => {
            map_project_map_atlas_error_to_v1(error)
        }
        ProjectMapSourcePreviewFailure::InvalidProjection => {
            CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)
        }
        ProjectMapSourcePreviewFailure::Source(_)
        | ProjectMapSourcePreviewFailure::Cancelled
        | ProjectMapSourcePreviewFailure::ProgressUnavailable => {
            CommandErrorV1::project_open(ErrorCodeV1::ProjectMapSourcePreviewUnavailable)
        }
    }
}

fn map_project_map_scene_error_to_v1(error: ProjectMapSceneFailure) -> CommandErrorV1 {
    let code = match error {
        ProjectMapSceneFailure::Storage(error) => map_storage_error_to_v1(error),
        ProjectMapSceneFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ProjectMapSceneFailure::Cancelled
        | ProjectMapSceneFailure::TimedOut
        | ProjectMapSceneFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_project_map_atlas_error_to_v1(error: ProjectMapAtlasFailure) -> CommandErrorV1 {
    let code = match error {
        ProjectMapAtlasFailure::Storage(error) => map_storage_error_to_v1(error),
        ProjectMapAtlasFailure::InvalidStoredProjection => ErrorCodeV1::LocalStorageInvalidData,
        ProjectMapAtlasFailure::Cancelled
        | ProjectMapAtlasFailure::TimedOut
        | ProjectMapAtlasFailure::ProgressUnavailable => ErrorCodeV1::LocalStorageUnavailable,
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
        | SearchProjectMapFailure::ResourceLimit
        | SearchProjectMapFailure::InvalidModuleBindings => ErrorCodeV1::LocalStorageInvalidData,
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

fn map_agent_activity_error_to_v1(error: GetAgentActivityFailure) -> CommandErrorV1 {
    match error {
        GetAgentActivityFailure::Workspace(error) => map_task_lens_workspace_error_to_v1(error),
        GetAgentActivityFailure::Journal(error) => {
            let code = match error {
                RunJournalStoreFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
                RunJournalStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
                RunJournalStoreFailure::UnsupportedSchema => {
                    ErrorCodeV1::LocalStorageUpgradeRequired
                }
                RunJournalStoreFailure::InvalidStoredData
                | RunJournalStoreFailure::RunAlreadyExists
                | RunJournalStoreFailure::RunNotFound
                | RunJournalStoreFailure::SequenceConflict => ErrorCodeV1::LocalStorageInvalidData,
            };
            CommandErrorV1::project_open(code)
        }
        GetAgentActivityFailure::InvalidRunAnchor
        | GetAgentActivityFailure::InvalidConfiguration => {
            CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)
        }
    }
}

fn map_agent_task_control_error_to_v1(error: AgentTaskControlFailure) -> CommandErrorV1 {
    match error {
        AgentTaskControlFailure::Activity(error) => map_agent_activity_error_to_v1(error),
        AgentTaskControlFailure::Recovery(error) => map_agent_recovery_error_to_v1(error),
        AgentTaskControlFailure::ResourceLimit => agent_task_control_unavailable(),
    }
}

fn map_agent_recovery_error_to_v1(error: AgentRecoveryError) -> CommandErrorV1 {
    let code = match error {
        AgentRecoveryError::Journal(error) => match error {
            RunJournalStoreFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
            RunJournalStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
            RunJournalStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
            RunJournalStoreFailure::InvalidStoredData
            | RunJournalStoreFailure::RunAlreadyExists
            | RunJournalStoreFailure::RunNotFound
            | RunJournalStoreFailure::SequenceConflict => ErrorCodeV1::LocalStorageInvalidData,
        },
        AgentRecoveryError::Ledger(error) => match error {
            TaskLedgerStoreFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
            TaskLedgerStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
            TaskLedgerStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
            TaskLedgerStoreFailure::InvalidStoredData
            | TaskLedgerStoreFailure::LedgerAlreadyExists
            | TaskLedgerStoreFailure::TaskNotFound
            | TaskLedgerStoreFailure::VersionConflict => ErrorCodeV1::LocalStorageInvalidData,
        },
        AgentRecoveryError::Index(KnowledgeIndexFailure::Storage(error)) => {
            map_storage_error_to_v1(error)
        }
        AgentRecoveryError::Store(error) => match error {
            a3_application::AgentRecoveryStoreFailure::Unavailable => {
                ErrorCodeV1::LocalStorageUnavailable
            }
            a3_application::AgentRecoveryStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
            a3_application::AgentRecoveryStoreFailure::UnsupportedSchema => {
                ErrorCodeV1::LocalStorageUpgradeRequired
            }
            a3_application::AgentRecoveryStoreFailure::InvalidStoredData
            | a3_application::AgentRecoveryStoreFailure::RunNotFound
            | a3_application::AgentRecoveryStoreFailure::ToolAttemptConflict
            | a3_application::AgentRecoveryStoreFailure::MutationReconciliationRequired
            | a3_application::AgentRecoveryStoreFailure::RunSequenceConflict
            | a3_application::AgentRecoveryStoreFailure::LedgerVersionConflict
            | a3_application::AgentRecoveryStoreFailure::PublishedSnapshotConflict
            | a3_application::AgentRecoveryStoreFailure::ResourceLimit => {
                ErrorCodeV1::LocalStorageInvalidData
            }
        },
        AgentRecoveryError::RunNotFound
        | AgentRecoveryError::TerminalRun
        | AgentRecoveryError::LedgerNotFound
        | AgentRecoveryError::AnchorMismatch
        | AgentRecoveryError::PublishedIndexUnavailable
        | AgentRecoveryError::ResumeRequiresReplan
        | AgentRecoveryError::MutationReconciliationRequired
        | AgentRecoveryError::InvalidTimestamp
        | AgentRecoveryError::InvalidRecoveryReason
        | AgentRecoveryError::ResourceLimit
        | AgentRecoveryError::Index(_)
        | AgentRecoveryError::RunDomain(_)
        | AgentRecoveryError::LedgerDomain(_) => ErrorCodeV1::AgentTaskControlUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn agent_task_control_unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::AgentTaskControlUnavailable)
}

fn agent_inspection_unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::AgentInspectionUnavailable)
}

fn agent_approval_unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::AgentApprovalUnavailable)
}

fn invalid_agent_inspection_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentInspectionQuery)
}

fn invalid_agent_goal() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::InvalidAgentGoal)
}

fn agent_goal_unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_open(ErrorCodeV1::AgentGoalUnavailable)
}

fn map_agent_goal_store_error_to_v1(
    error: a3_application::GoalContractStoreFailure,
) -> CommandErrorV1 {
    use a3_application::GoalContractStoreFailure;
    let code = match error {
        GoalContractStoreFailure::Unavailable => ErrorCodeV1::LocalStorageUnavailable,
        GoalContractStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        GoalContractStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        GoalContractStoreFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
        GoalContractStoreFailure::TaskNotFound => ErrorCodeV1::AgentGoalTaskNotFound,
        GoalContractStoreFailure::RevisionConflict => ErrorCodeV1::AgentGoalRevisionConflict,
        GoalContractStoreFailure::TaskAlreadyExists => ErrorCodeV1::AgentGoalUnavailable,
    };
    CommandErrorV1::project_open(code)
}

fn map_create_agent_goal_error_to_v1(error: CreateAgentGoalFailure) -> CommandErrorV1 {
    use a3_application::CreateGoalContractFailure;
    match error {
        CreateAgentGoalFailure::ExistingCriterionIdentity | CreateAgentGoalFailure::Draft(_) => {
            invalid_agent_goal()
        }
        CreateAgentGoalFailure::Metadata(_) => agent_goal_unavailable(),
        CreateAgentGoalFailure::Create(CreateGoalContractFailure::InvalidInitialRevision) => {
            CommandErrorV1::project_open(ErrorCodeV1::LocalStorageInvalidData)
        }
        CreateAgentGoalFailure::Create(CreateGoalContractFailure::Store(error)) => {
            map_agent_goal_store_error_to_v1(error)
        }
    }
}

fn map_revise_agent_goal_error_to_v1(error: ReviseAgentGoalFailure) -> CommandErrorV1 {
    match error {
        ReviseAgentGoalFailure::TaskNotFound => {
            CommandErrorV1::project_open(ErrorCodeV1::AgentGoalTaskNotFound)
        }
        ReviseAgentGoalFailure::RevisionConflict => {
            CommandErrorV1::project_open(ErrorCodeV1::AgentGoalRevisionConflict)
        }
        ReviseAgentGoalFailure::Metadata(_) => agent_goal_unavailable(),
        ReviseAgentGoalFailure::Draft(_)
        | ReviseAgentGoalFailure::InvalidRevision(
            a3_domain::GoalContractRevisionFailure::NoMaterialChange,
        ) => invalid_agent_goal(),
        ReviseAgentGoalFailure::InvalidRevision(
            a3_domain::GoalContractRevisionFailure::RevisionExhausted
            | a3_domain::GoalContractRevisionFailure::TimestampRegressed,
        ) => agent_goal_unavailable(),
        ReviseAgentGoalFailure::Store(error) => map_agent_goal_store_error_to_v1(error),
    }
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
    /// The scheduler stopped before the Agent Run coordinator could acquire a submit capability.
    AgentRunManagerUnavailable,
    /// The owned Agent Run coordinator could not be started.
    AgentRunManager,
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
            Self::AgentRunManagerUnavailable => {
                formatter.write_str("Agent Run manager is unavailable")
            }
            Self::AgentRunManager => formatter.write_str("Agent Run manager failed"),
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
            Self::DeepMapManagerUnavailable
            | Self::DeepMapManager
            | Self::AgentRunManagerUnavailable
            | Self::AgentRunManager => None,
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
    use super::{
        MAX_PROJECT_PATH_DISPLAY_CHARS, decode_deep_map_entry_selection,
        decode_deep_map_impact_cursor, decode_deep_map_module_selection,
        decode_deep_map_run_cursor, decode_deep_map_run_selection, decode_deep_map_step_cursor,
        encode_deep_map_entry_selection, encode_deep_map_impact_cursor,
        encode_deep_map_module_selection, encode_deep_map_run_cursor,
        encode_deep_map_run_selection, encode_deep_map_step_cursor, map_agent_goal_to_v1,
        map_agent_task_control_result_to_v1, map_create_agent_goal_from_v1, project_path_display,
        publication_read_failure_lifecycle,
    };
    use a3_application::DeepMapRunCursor;
    use a3_application::{
        AgentRecoveryOutcomeKind, AgentTaskControlResult, TaskLedgerStoreVersion,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
        AcceptanceCriterionStatement, DeepMapEventSequence, DeepMapRunId, DeepMapRunTimestamp,
        GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, ModuleId,
        SuccessVerification, TaskId, WorktreeId,
    };
    use a3_protocol::{
        AgentTaskRuntimeStartV1, CreateAgentGoalRequestV1, DeepMapCompactProgressV3,
        DeepMapLifecycleV3,
    };
    use serde_json::json;
    use std::error::Error;
    use std::path::Path;

    #[test]
    fn deep_map_status_preserves_live_work_during_a_transient_publication_read_failure() {
        let running = DeepMapLifecycleV3::Running {
            progress: DeepMapCompactProgressV3::new("2".to_owned(), "5".to_owned(), None, None),
            details_incomplete: false,
        };

        assert_eq!(publication_read_failure_lifecycle(running.clone()), running);
        assert_eq!(
            publication_read_failure_lifecycle(DeepMapLifecycleV3::Ready),
            DeepMapLifecycleV3::Ready
        );
    }

    #[test]
    fn project_path_display_is_bounded_and_contains_no_control_characters() {
        let path = format!("C:\\\n{}", "a".repeat(MAX_PROJECT_PATH_DISPLAY_CHARS + 8));

        let display = project_path_display(Path::new(&path));

        assert_eq!(display.chars().count(), MAX_PROJECT_PATH_DISPLAY_CHARS);
        assert!(!display.chars().any(char::is_control));
        assert!(display.contains('\u{fffd}'));
    }

    #[test]
    fn deep_map_selections_are_project_bound_and_tamper_evident() -> Result<(), Box<dyn Error>> {
        let first_worktree = WorktreeId::from_bytes([1; 32]);
        let second_worktree = WorktreeId::from_bytes([2; 32]);
        let run_id = DeepMapRunId::from_bytes([3; 32]);
        let sequence = DeepMapEventSequence::new(7)?;

        let run_selection = encode_deep_map_run_selection(first_worktree, run_id);
        assert_eq!(
            decode_deep_map_run_selection(first_worktree, &run_selection),
            Ok(run_id)
        );
        assert!(decode_deep_map_run_selection(second_worktree, &run_selection).is_err());

        let entry_selection = encode_deep_map_entry_selection(first_worktree, run_id, sequence);
        assert_eq!(
            decode_deep_map_entry_selection(first_worktree, run_id, &entry_selection),
            Ok(sequence)
        );
        assert!(
            decode_deep_map_entry_selection(
                first_worktree,
                DeepMapRunId::from_bytes([4; 32]),
                &entry_selection,
            )
            .is_err()
        );

        let mut tampered = entry_selection.into_bytes();
        tampered[0] = if tampered[0] == b'0' { b'1' } else { b'0' };
        let tampered = String::from_utf8(tampered)?;
        assert!(decode_deep_map_entry_selection(first_worktree, run_id, &tampered).is_err());
        Ok(())
    }

    #[test]
    fn deep_map_run_cursors_are_project_bound_and_reject_stale_or_invented_values()
    -> Result<(), Box<dyn Error>> {
        let first_worktree = WorktreeId::from_bytes([5; 32]);
        let second_worktree = WorktreeId::from_bytes([6; 32]);
        let cursor = DeepMapRunCursor::new(
            DeepMapRunTimestamp::new(1_725_000_000_000)?,
            DeepMapRunId::from_bytes([7; 32]),
        );

        let encoded = encode_deep_map_run_cursor(first_worktree, cursor);
        assert_eq!(
            decode_deep_map_run_cursor(first_worktree, &encoded),
            Ok(cursor)
        );
        assert!(decode_deep_map_run_cursor(second_worktree, &encoded).is_err());
        assert!(decode_deep_map_run_cursor(first_worktree, "00").is_err());
        Ok(())
    }

    #[test]
    fn deep_map_dashboard_module_and_number_cursors_are_bound_to_the_exact_scope() {
        let worktree = WorktreeId::from_bytes([8; 32]);
        let other_worktree = WorktreeId::from_bytes([9; 32]);
        let run = DeepMapRunId::from_bytes([10; 32]);
        let other_run = DeepMapRunId::from_bytes([11; 32]);
        let module = ModuleId::from_bytes([12; 32]);
        let other_module = ModuleId::from_bytes([13; 32]);

        let selection = encode_deep_map_module_selection(worktree, run, module);
        assert_eq!(
            decode_deep_map_module_selection(worktree, run, &selection),
            Ok(module)
        );
        assert!(decode_deep_map_module_selection(other_worktree, run, &selection).is_err());
        assert!(decode_deep_map_module_selection(worktree, other_run, &selection).is_err());

        let step = encode_deep_map_step_cursor(worktree, run, module, 50);
        assert_eq!(
            decode_deep_map_step_cursor(worktree, run, module, &step),
            Ok(50)
        );
        assert!(decode_deep_map_step_cursor(worktree, run, other_module, &step).is_err());

        let impact = encode_deep_map_impact_cursor(worktree, run, module, 50);
        assert_eq!(
            decode_deep_map_impact_cursor(worktree, run, module, &impact),
            Ok(50)
        );
        assert!(decode_deep_map_step_cursor(worktree, run, module, &impact).is_err());
    }

    #[test]
    fn agent_goal_boundary_preserves_must_and_should_without_exposing_storage_rows()
    -> Result<(), Box<dyn Error>> {
        let request = serde_json::from_value::<CreateAgentGoalRequestV1>(json!({
            "protocolVersion": 1,
            "draft": {
                "objective": "build the Agent workspace",
                "acceptanceCriteria": [
                    {"criterionId": null, "statement": "must pass", "requirement": "must"},
                    {"criterionId": null, "statement": "should remain visible", "requirement": "should"}
                ],
                "constraints": [],
                "nonGoals": [],
                "userDecisions": [],
                "successVerification": "reopen the durable contract"
            }
        }))?;
        let mapped = map_create_agent_goal_from_v1(&request)
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert_eq!(
            mapped
                .acceptance_criteria()
                .iter()
                .map(a3_application::AgentGoalCriterionDraft::requirement)
                .collect::<Vec<_>>(),
            vec![
                AcceptanceCriterionRequirement::Must,
                AcceptanceCriterionRequirement::Should
            ]
        );

        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("build the Agent workspace".to_owned())?,
                vec![AcceptanceCriterion::with_requirement(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "should remain visible".to_owned(),
                    )?,
                    AcceptanceCriterionRequirement::Should,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("reopen the durable contract".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        assert_eq!(
            serde_json::to_value(map_agent_goal_to_v1(&goal))?["acceptanceCriteria"][0]["requirement"],
            "should"
        );
        Ok(())
    }

    #[test]
    fn recovery_boundary_reports_new_job_only_after_a_nonterminal_commit()
    -> Result<(), Box<dyn Error>> {
        let response = map_agent_task_control_result_to_v1(
            AgentTaskControlResult::Applied {
                outcome: AgentRecoveryOutcomeKind::Resumed,
                ledger_store_version: TaskLedgerStoreVersion::new(8)?,
                state: a3_domain::AgentControllerState::Execute,
                reopened_step_count: 0,
                interrupted_tool_attempts: 1,
            },
            Some(AgentTaskRuntimeStartV1::Queued),
        );
        let json = serde_json::to_value(response)?;
        assert_eq!(json["status"], "applied");
        assert_eq!(json["runtimeStart"], "queued");
        assert_eq!(json["state"], "execute");
        Ok(())
    }
}
