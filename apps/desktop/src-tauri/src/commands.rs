use crate::agent_session_manager::PresentationMutation;
use crate::model_settings_manager::settings_version_from_v1;
use crate::project_map_atlas_mapping::{
    map_flow_query_from_v1, map_inventory_query_from_v1, map_selection_from_v1,
};
use crate::project_settings_manager::{
    allowlist_version_from_v1, catalog_id_from_v1, command_ids_from_v1,
};
use crate::{
    CompositionRoot, decode_stable_id, map_agent_activity_task_id_from_v1,
    map_agent_approval_control_from_v1, map_agent_approval_task_id_from_v1,
    map_agent_goal_task_id_from_v1, map_agent_inspection_log_query_from_v1,
    map_agent_inspection_task_id_from_v1, map_agent_task_control_from_v1,
    map_agent_task_recovery_task_id_from_v1, map_create_agent_goal_from_v1,
    map_module_card_detail_query_from_v1, map_module_card_evidence_query_from_v1,
    map_module_dependency_graph_query_from_v1, map_module_runtime_flow_query_from_v1,
    map_module_runtime_map_query_from_v1, map_module_tree_query_from_v1,
    map_project_catalog_query_from_v1, map_project_map_scene_query_from_v1,
    map_project_map_search_query_from_v1, map_project_map_source_preview_query_from_v1,
    map_repository_tree_query_from_v1, map_revise_agent_goal_from_v1,
    map_task_lens_selection_from_v1, map_task_lens_task_id_from_v1, map_worktree_id_from_v1,
    parse_canonical_positive_u64,
};
use a3_application::{
    AgentSessionListQuery, AgentWorkspaceLayout, SLASH_COMMAND_LENSES, SLASH_COMMANDS,
    UiPreferencesStoreVersion,
};
use a3_domain::{
    AgentResearchDepth, AgentSessionId, AgentSessionMode, AgentSessionRevision, DeepMapMode,
    SlashCommandEmptyInput,
};
use a3_protocol::{
    ActivateCatalogProjectRequestV1, AgentActivityResponseV1, AgentApprovalControlResponseV1,
    AgentApprovalResponseV1, AgentAskResearchDetailResponseV1,
    AgentAskResearchSourcePreviewResponseV1, AgentAskResearchSourcesResponseV1,
    AgentAskResearchTurnsResponseV1, AgentDiagramArtifactResponseV1,
    AgentDiagramArtifactsResponseV1, AgentDiagramExportFormatV1, AgentDiagramExportResponseV1,
    AgentDiagramExportResultV1, AgentGoalMutationResponseV1, AgentGoalResponseV1,
    AgentInspectionLogResponseV1, AgentInspectionResponseV1, AgentResearchDepthSelectionV1,
    AgentResearchDepthV1, AgentSessionControlActionV1, AgentSessionModeV1, AgentSessionResponseV1,
    AgentSessionResponseV2, AgentSessionsResponseV1, AgentSlashCommandRoleV1, AgentSlashCommandV1,
    AgentSlashCommandsResponseV1, AgentTaskControlActionV1, AgentTaskControlResponseV1,
    AgentTaskRecoveryResponseV1, CancelModelProbeRequestV1, CancelModelProbeResponseV1,
    CommandErrorV1, CompileTaskLensRequestV1, ConfigureModelProviderRequestV1,
    ConfirmProjectCommandAllowlistRequestV1, ContinueAgentResearchRequestV1,
    ControlAgentApprovalRequestV1, ControlAgentSessionRequestV1, ControlAgentTaskRunRequestV1,
    ControlDeepMapRequestV1, CreateAgentGoalRequestV1, DeepMapAtlasImpactResponseV1,
    DeepMapControlResponseV1, DeepMapEntryDetailResponseV1, DeepMapEntryPageResponseV1,
    DeepMapModeV2, DeepMapModuleStepsResponseV1, DeepMapRunDashboardResponseV1,
    DeepMapRunModulesResponseV1, DeepMapRunPageResponseV1, DeepMapStartResponseV2,
    DeepMapStatusResponseV3, DeleteModelProviderCredentialRequestV1,
    DiscoverProviderModelsRequestV1, ExportAgentDiagramRequestV1, HealthRequestV1,
    HealthResponseV1, IndexActivityResponseV1, IndexOverviewResponseV1,
    ListRecentProjectsRequestV1, ModuleCardDetailResponseV1, ModuleCardEvidenceResponseV1,
    ModuleCardFreshnessResponseV1, ModuleDependencyGraphResponseV1, ModuleRuntimeFlowResponseV1,
    ModuleRuntimeMapResponseV1, ModuleTreeResponseV1, OpenProjectRequestV1, OpenProjectResponseV1,
    ProbeModelRoleRequestV1, ProjectActivationResponseV1, ProjectCatalogResponseV1,
    ProjectMapSceneResponseV1, ProjectMapSearchResponseV1, ProjectMapSourcePreviewResponseV1,
    ProjectSettingsResponseV1, ProjectStatusResponseV1, ProtocolVersion, ProviderModelsResponseV1,
    QueryAgentActivityRequestV1, QueryAgentApprovalRequestV1, QueryAgentAskResearchDetailRequestV1,
    QueryAgentAskResearchSourcePreviewRequestV1, QueryAgentAskResearchSourcesRequestV1,
    QueryAgentAskResearchTurnsRequestV1, QueryAgentDiagramArtifactRequestV1,
    QueryAgentDiagramArtifactsRequestV1, QueryAgentGoalRequestV1, QueryAgentInspectionLogRequestV1,
    QueryAgentInspectionRequestV1, QueryAgentSessionRequestV1, QueryAgentSessionsRequestV1,
    QueryAgentSlashCommandsRequestV1, QueryAgentTaskRecoveryRequestV1,
    QueryDeepMapAtlasImpactRequestV1, QueryDeepMapEntriesRequestV1,
    QueryDeepMapEntryDetailRequestV1, QueryDeepMapModuleStepsRequestV1, QueryDeepMapRequestV1,
    QueryDeepMapRunDashboardRequestV1, QueryDeepMapRunModulesRequestV1, QueryDeepMapRunsRequestV1,
    QueryIndexActivityRequestV1, QueryIndexOverviewRequestV1, QueryModuleCardDetailRequestV1,
    QueryModuleCardEvidenceRequestV1, QueryModuleCardFreshnessRequestV1,
    QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
    QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectCatalogRequestV1,
    QueryProjectMapSceneRequestV1, QueryProjectMapSearchRequestV1,
    QueryProjectMapSourcePreviewRequestV1, QueryProjectSettingsRequestV1,
    QueryProjectStatusRequestV1, QueryRepositoryTreeRequestV1, QuerySettingsRequestV1,
    QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1, QueryUiPreferencesRequestV1,
    RebuildProjectIndexRequestV1, RebuildProjectIndexResponseV1, RecentProjectsResponseV1,
    RemoveCatalogProjectRequestV1, RemoveProjectRequestV1, RemoveProjectResponseV1,
    RepositoryTreeResponseV1, RestoreLastProjectRequestV1, ReviseAgentGoalRequestV1,
    SetModelProviderCredentialRequestV1, SettingsResponseV1, StartDeepMapRequestV2,
    SubmitAgentMessageRequestV1, SubmitAgentMessageRequestV2, SubmitAgentMessageRequestV3,
    TaskLensCompileResponseV1, TaskLensTaskResponseV1, TaskLensTasksResponseV1,
    UiPreferencesResponseV1, UpdateAgentWorkspaceLayoutRequestV1,
};
use a3_protocol::{
    ProjectMapAtlasSceneResponseV1, ProjectMapEntityContextResponseV1,
    ProjectMapFlowSceneResponseV1, ProjectMapInventoryPageResponseV1,
    QueryProjectMapAtlasSceneRequestV1, QueryProjectMapEntityContextRequestV1,
    QueryProjectMapFlowSceneRequestV1, QueryProjectMapInventoryPageRequestV1,
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
/// Opens one native directory picker and returns only a validated project identity projection.
pub async fn open_project(
    request: OpenProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<OpenProjectResponseV1, CommandErrorV1> {
    execute_open_project(request, root.inner()).await
}

#[tauri::command]
/// Returns a bounded most-recent-first list without exposing authoritative paths.
pub async fn list_recent_projects(
    request: ListRecentProjectsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<RecentProjectsResponseV1, CommandErrorV1> {
    execute_list_recent_projects(request, root.inner()).await
}

#[tauri::command]
/// Returns one fixed-size, searchable catalog page without accepting a path.
pub async fn query_project_catalog(
    request: QueryProjectCatalogRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectCatalogResponseV1, CommandErrorV1> {
    execute_query_project_catalog(request, root.inner()).await
}

#[tauri::command]
/// Revalidates and activates one worktree ID from the durable catalog.
pub async fn activate_catalog_project(
    request: ActivateCatalogProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
    execute_activate_catalog_project(request, root.inner()).await
}

#[tauri::command]
/// Restores only the most recently activated catalog entry.
pub async fn restore_last_project(
    request: RestoreLastProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
    execute_restore_last_project(request, root.inner()).await
}

#[tauri::command]
/// Returns bounded status for the Core-owned active project without accepting an identity or path.
pub async fn query_project_status(
    request: QueryProjectStatusRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectStatusResponseV1, CommandErrorV1> {
    execute_query_project_status(request, root.inner()).await
}

#[tauri::command]
/// Returns only the in-memory Fast-Index activity for responsive polling.
pub fn query_index_activity(
    request: QueryIndexActivityRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<IndexActivityResponseV1, CommandErrorV1> {
    execute_query_index_activity(request, root.inner())
}

#[tauri::command]
/// Returns a bounded projection of the active project's latest complete published index.
pub async fn query_index_overview(
    request: QueryIndexOverviewRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<IndexOverviewResponseV1, CommandErrorV1> {
    execute_query_index_overview(request, root.inner()).await
}

#[tauri::command]
/// Returns exact current Module Card lifecycle counts without accepting an identity or path.
pub async fn query_module_card_freshness(
    request: QueryModuleCardFreshnessRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleCardFreshnessResponseV1, CommandErrorV1> {
    execute_query_module_card_freshness(request, root.inner()).await
}

#[tauri::command]
/// Returns the latest durable classified Card for one explicit current primary module.
pub async fn query_module_card_detail(
    request: QueryModuleCardDetailRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleCardDetailResponseV1, CommandErrorV1> {
    execute_query_module_card_detail(request, root.inner()).await
}

#[tauri::command]
/// Resolves one Evidence hook only while all visible Module Card anchors still match.
pub async fn query_module_card_evidence(
    request: QueryModuleCardEvidenceRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleCardEvidenceResponseV1, CommandErrorV1> {
    execute_query_module_card_evidence(request, root.inner()).await
}

#[tauri::command]
/// Revalidates one Core-issued Evidence selection before returning bounded plain source text.
pub async fn query_project_map_source_preview(
    request: QueryProjectMapSourcePreviewRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapSourcePreviewResponseV1, CommandErrorV1> {
    execute_query_project_map_source_preview(request, root.inner()).await
}

#[tauri::command]
/// Returns the policy-bounded deterministic atlas scene for the active project.
pub async fn query_project_map_scene(
    request: QueryProjectMapSceneRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapSceneResponseV1, CommandErrorV1> {
    execute_query_project_map_scene(request, root.inner()).await
}

#[tauri::command]
/// Returns one bounded Project, Module, File, or Symbol Atlas scene.
pub async fn query_project_map_atlas_scene(
    request: QueryProjectMapAtlasSceneRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapAtlasSceneResponseV1, CommandErrorV1> {
    execute_query_project_map_atlas_scene(request, root.inner()).await
}

#[tauri::command]
/// Returns progressive Inspector metadata for one Core-issued current selection.
pub async fn query_project_map_entity_context(
    request: QueryProjectMapEntityContextRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapEntityContextResponseV1, CommandErrorV1> {
    execute_query_project_map_entity_context(request, root.inner()).await
}

#[tauri::command]
/// Returns exactly one fixed fifty-entry Atlas inventory page.
pub async fn query_project_map_inventory_page(
    request: QueryProjectMapInventoryPageRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapInventoryPageResponseV1, CommandErrorV1> {
    execute_query_project_map_inventory_page(request, root.inner()).await
}

#[tauri::command]
/// Returns one bounded fixed-preset callers, callees, tests, or data-access flow.
pub async fn query_project_map_flow_scene(
    request: QueryProjectMapFlowSceneRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapFlowSceneResponseV1, CommandErrorV1> {
    execute_query_project_map_flow_scene(request, root.inner()).await
}

#[tauri::command]
/// Returns one bounded page of deterministic primary modules from the current publication.
pub async fn query_module_tree(
    request: QueryModuleTreeRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleTreeResponseV1, CommandErrorV1> {
    execute_query_module_tree(request, root.inner()).await
}

#[tauri::command]
/// Returns an evidence-bound direct neighborhood around one current deterministic module.
pub async fn query_module_dependency_graph(
    request: QueryModuleDependencyGraphRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleDependencyGraphResponseV1, CommandErrorV1> {
    execute_query_module_dependency_graph(request, root.inner()).await
}

#[tauri::command]
/// Returns bounded current entrypoint and test roots for one deterministic primary module.
pub async fn query_module_runtime_map(
    request: QueryModuleRuntimeMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleRuntimeMapResponseV1, CommandErrorV1> {
    execute_query_module_runtime_map(request, root.inner()).await
}

#[tauri::command]
/// Traverses one fixed role-specific preset bound to a visible atomic publication.
pub async fn query_module_runtime_flow(
    request: QueryModuleRuntimeFlowRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ModuleRuntimeFlowResponseV1, CommandErrorV1> {
    execute_query_module_runtime_flow(request, root.inner()).await
}

#[tauri::command]
/// Searches exact and lexical current-index projections after an explicit user request.
pub async fn query_project_map_search(
    request: QueryProjectMapSearchRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectMapSearchResponseV1, CommandErrorV1> {
    execute_query_project_map_search(request, root.inner()).await
}

#[tauri::command]
/// Lists bounded durable tasks without accepting a project identity or path.
pub async fn query_task_lens_tasks(
    request: QueryTaskLensTasksRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<TaskLensTasksResponseV1, CommandErrorV1> {
    execute_query_task_lens_tasks(request, root.inner()).await
}

#[tauri::command]
/// Loads current active-plan steps for one opaque durable task identity.
pub async fn query_task_lens_task(
    request: QueryTaskLensTaskRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<TaskLensTaskResponseV1, CommandErrorV1> {
    execute_query_task_lens_task(request, root.inner()).await
}

#[tauri::command]
/// Compiles one selected current task/step through the bounded deterministic R10 pipeline.
pub async fn compile_task_lens(
    request: CompileTaskLensRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<TaskLensCompileResponseV1, CommandErrorV1> {
    execute_compile_task_lens(request, root.inner()).await
}

#[tauri::command]
/// Lists bounded project-local Agent conversations newest first.
pub async fn query_agent_sessions(
    request: QueryAgentSessionsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionsResponseV1, CommandErrorV1> {
    execute_query_agent_sessions(request, root.inner()).await
}

#[tauri::command]
/// Loads one bounded Agent conversation page without exposing storage capabilities.
pub async fn query_agent_session(
    request: QueryAgentSessionRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_query_agent_session(request, root.inner()).await
}

#[tauri::command]
/// Loads one session page with persisted command chips and diagram summaries.
pub async fn query_agent_session_v2(
    request: QueryAgentSessionRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV2, CommandErrorV1> {
    execute_query_agent_session_v2(request, root.inner()).await
}

#[tauri::command]
/// Returns the immutable built-in slash-command catalog filtered for one mode.
pub async fn query_agent_slash_commands(
    request: QueryAgentSlashCommandsRequestV1,
) -> Result<AgentSlashCommandsResponseV1, CommandErrorV1> {
    execute_query_agent_slash_commands(request)
}

#[tauri::command]
/// Lists persistent research turns without exposing index or provider identities.
pub async fn query_agent_ask_research_turns(
    request: QueryAgentAskResearchTurnsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentAskResearchTurnsResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_turns(request, root.inner()).await
}

#[tauri::command]
/// Loads one safe chronological Ask research projection.
pub async fn query_agent_ask_research_detail(
    request: QueryAgentAskResearchDetailRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentAskResearchDetailResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_detail(request, root.inner()).await
}

#[tauri::command]
/// Lists one bound page of sources found or used by Ask.
pub async fn query_agent_ask_research_sources(
    request: QueryAgentAskResearchSourcesRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentAskResearchSourcesResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_sources(request, root.inner()).await
}

#[tauri::command]
/// Loads a safe source preview through an opaque research source reference.
pub async fn query_agent_ask_research_source_preview(
    request: QueryAgentAskResearchSourcePreviewRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentAskResearchSourcePreviewResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_source_preview(request, root.inner()).await
}

#[tauri::command]
/// Lists at most three diagram artifacts completed with one session turn.
pub async fn query_agent_diagram_artifacts(
    request: QueryAgentDiagramArtifactsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentDiagramArtifactsResponseV1, CommandErrorV1> {
    execute_query_agent_diagram_artifacts(request, root.inner()).await
}

#[tauri::command]
/// Loads one Core-compiled diagram through an opaque session-bound reference.
pub async fn query_agent_diagram_artifact(
    request: QueryAgentDiagramArtifactRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentDiagramArtifactResponseV1, CommandErrorV1> {
    execute_query_agent_diagram_artifact(request, root.inner()).await
}

#[tauri::command]
/// Opens a native save dialog and atomically exports one validated rendered diagram.
pub async fn export_agent_diagram(
    request: ExportAgentDiagramRequestV1,
    app: AppHandle,
    root: State<'_, CompositionRoot>,
) -> Result<AgentDiagramExportResponseV1, CommandErrorV1> {
    execute_export_agent_diagram(request, &app, root.inner()).await
}

#[tauri::command]
/// Lists generic Ask, Plan, and Agent-preparation work traces.
pub async fn query_agent_work_trace_turns(
    request: a3_protocol::QueryAgentWorkTraceTurnsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceTurnsResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.query_agent_work_trace_turns(decode_agent_session_id(request.session_id())?)
        .await
}

#[tauri::command]
/// Loads public notes and bounded events for one work-trace turn.
pub async fn query_agent_work_trace_detail(
    request: a3_protocol::QueryAgentWorkTraceDetailRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceDetailResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.query_agent_work_trace_detail(
        decode_agent_session_id(request.session_id())?,
        a3_domain::AgentSessionSequence::new(
            parse_canonical_positive_u64(request.user_sequence())
                .map_err(|_| invalid_agent_session())?,
        )
        .map_err(|_| invalid_agent_session())?,
    )
    .await
}

#[tauri::command]
/// Loads the V2 work trace for the expanded closed analysis-action catalog.
pub async fn query_agent_work_trace_detail_v2(
    request: a3_protocol::QueryAgentWorkTraceDetailRequestV2,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceDetailResponseV2, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.query_agent_work_trace_detail(
        decode_agent_session_id(request.session_id())?,
        a3_domain::AgentSessionSequence::new(
            parse_canonical_positive_u64(request.user_sequence())
                .map_err(|_| invalid_agent_session())?,
        )
        .map_err(|_| invalid_agent_session())?,
    )
    .await
}

#[tauri::command]
/// Loads detail, counts, and the first source page as one coherent presentation projection.
pub async fn query_agent_work_trace_projection(
    request: a3_protocol::QueryAgentWorkTraceProjectionRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceProjectionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.query_agent_work_trace_projection(
        decode_agent_session_id(request.session_id())?,
        decode_agent_user_sequence(request.user_sequence())?,
    )
    .await
}

#[tauri::command]
/// Lists one cursor-bound generic work-trace source page.
pub async fn query_agent_work_trace_sources(
    request: a3_protocol::QueryAgentWorkTraceSourcesRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceSourcesResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_sources(request, root.inner()).await
}

#[tauri::command]
/// Loads a continuation source page bound to an unchanged coherent projection.
pub async fn query_agent_work_trace_sources_v2(
    request: a3_protocol::QueryAgentWorkTraceSourcesRequestV2,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceSourcesResponseV2, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    decode_stable_id(request.projection_ref()).map_err(|_| invalid_agent_session())?;
    if let Some(cursor) = request.cursor() {
        decode_stable_id(cursor).map_err(|_| invalid_agent_session())?;
    }
    root.query_agent_work_trace_sources_v2(
        decode_agent_session_id(request.session_id())?,
        decode_agent_user_sequence(request.user_sequence())?,
        request.projection_ref(),
        request.cursor(),
    )
    .await
}

#[tauri::command]
/// Loads one safe preview through an opaque generic work-trace source reference.
pub async fn query_agent_work_trace_source_preview(
    request: a3_protocol::QueryAgentWorkTraceSourcePreviewRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<a3_protocol::AgentWorkTraceSourcePreviewResponseV1, CommandErrorV1> {
    execute_query_agent_ask_research_source_preview(request, root.inner()).await
}

#[tauri::command]
/// Submits one bounded message to a new or existing project-local Agent conversation.
pub async fn submit_agent_message(
    request: SubmitAgentMessageRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_submit_agent_message(request, root.inner()).await
}

#[tauri::command]
/// Submits one message with an explicit finite research depth.
pub async fn submit_agent_message_v2(
    request: SubmitAgentMessageRequestV2,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_submit_agent_message_v2(request, root.inner()).await
}

#[tauri::command]
/// Submits an ordinary-depth message or a Core-resolved built-in slash command.
pub async fn submit_agent_message_v3(
    request: SubmitAgentMessageRequestV3,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_submit_agent_message_v3(request, root.inner()).await
}

#[tauri::command]
/// Continues only the newest continuation-ready research section.
pub async fn continue_agent_research(
    request: ContinueAgentResearchRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_continue_agent_research(request, root.inner()).await
}

#[tauri::command]
/// Applies one closed optimistic Agent-session control.
pub async fn control_agent_session(
    request: ControlAgentSessionRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    execute_control_agent_session(request, root.inner()).await
}

#[tauri::command]
/// Loads global content-free Agent workspace layout preferences.
pub async fn query_ui_preferences(
    request: QueryUiPreferencesRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
    execute_query_ui_preferences(request, root.inner()).await
}

#[tauri::command]
/// Persists bounded content-free Agent workspace layout preferences.
pub async fn update_agent_workspace_layout(
    request: UpdateAgentWorkspaceLayoutRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
    execute_update_agent_workspace_layout(request, root.inner()).await
}

#[tauri::command]
/// Loads the complete current Goal Contract for one opaque durable task identity.
pub async fn query_agent_goal(
    request: QueryAgentGoalRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentGoalResponseV1, CommandErrorV1> {
    execute_query_agent_goal(request, root.inner()).await
}

#[tauri::command]
/// Loads bounded current run activity derived from the selected task's durable ledger.
pub async fn query_agent_activity(
    request: QueryAgentActivityRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentActivityResponseV1, CommandErrorV1> {
    execute_query_agent_activity(request, root.inner()).await
}

#[tauri::command]
/// Loads exact task-bound patch/process data and freshly evaluated durable verification.
pub async fn query_agent_inspection(
    request: QueryAgentInspectionRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentInspectionResponseV1, CommandErrorV1> {
    execute_query_agent_inspection(request, root.inner()).await
}

#[tauri::command]
/// Loads one explicitly selected retained safe process-log page.
pub async fn query_agent_inspection_log(
    request: QueryAgentInspectionLogRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentInspectionLogResponseV1, CommandErrorV1> {
    execute_query_agent_inspection_log(request, root.inner()).await
}

#[tauri::command]
/// Loads the exact task-bound action and lifecycle shown before a privileged mutation.
pub async fn query_agent_approval(
    request: QueryAgentApprovalRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentApprovalResponseV1, CommandErrorV1> {
    execute_query_agent_approval(request, root.inner()).await
}

#[tauri::command]
/// Applies one closed approval decision against the exact visible optimistic anchors.
pub async fn control_agent_approval(
    request: ControlAgentApprovalRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentApprovalControlResponseV1, CommandErrorV1> {
    execute_control_agent_approval(request, root.inner()).await
}

#[tauri::command]
/// Inspects restart-safe Resume, Replan, and Cancel controls for a task-derived active run.
pub async fn query_agent_task_recovery(
    request: QueryAgentTaskRecoveryRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentTaskRecoveryResponseV1, CommandErrorV1> {
    execute_query_agent_task_recovery(request, root.inner()).await
}

#[tauri::command]
/// Atomically applies one explicit task-bound recovery choice against exact visible anchors.
pub async fn control_agent_task_run(
    request: ControlAgentTaskRunRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentTaskControlResponseV1, CommandErrorV1> {
    execute_control_agent_task_run(request, root.inner()).await
}

#[tauri::command]
/// Creates one task atomically with a Core-identified initial Goal Contract revision.
pub async fn create_agent_goal(
    request: CreateAgentGoalRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
    execute_create_agent_goal(request, root.inner()).await
}

#[tauri::command]
/// Compare-and-appends a material Goal Contract successor without silent mutation.
pub async fn revise_agent_goal(
    request: ReviseAgentGoalRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
    execute_revise_agent_goal(request, root.inner()).await
}

#[tauri::command]
/// Returns one bounded indexed directory page without accepting a filesystem path capability.
pub async fn query_repository_tree(
    request: QueryRepositoryTreeRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<RepositoryTreeResponseV1, CommandErrorV1> {
    execute_query_repository_tree(request, root.inner()).await
}

#[tauri::command]
/// Returns verified pre-start configuration and the Core-owned Deep-Map lifecycle.
pub async fn query_deep_map(
    request: QueryDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapStatusResponseV3, CommandErrorV1> {
    execute_query_deep_map(request, root.inner()).await
}

#[tauri::command]
/// Explicitly starts Deep Map with the supplied bounded budget and no WebView-selected identity.
pub async fn start_deep_map(
    request: StartDeepMapRequestV2,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapStartResponseV2, CommandErrorV1> {
    execute_start_deep_map(request, root.inner()).await
}

#[tauri::command]
/// Returns at most twenty durable project-bound Deep-Map runs.
pub async fn query_deep_map_runs(
    request: QueryDeepMapRunsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapRunPageResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_runs(request.cursor()).await
}

#[tauri::command]
/// Returns at most fifty chronological safe entries for one Core-issued run selection.
pub async fn query_deep_map_entries(
    request: QueryDeepMapEntriesRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapEntryPageResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_entries(request.run_selection(), request.cursor())
        .await
}

#[tauri::command]
/// Returns one selected safe entry with its bounded technical metadata.
pub async fn query_deep_map_entry_detail(
    request: QueryDeepMapEntryDetailRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapEntryDetailResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_entry_detail(request.run_selection(), request.entry_selection())
        .await
}

#[tauri::command]
/// Returns the understandable five-phase dashboard for one Core-issued run.
pub async fn query_deep_map_run_dashboard(
    request: QueryDeepMapRunDashboardRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapRunDashboardResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_run_dashboard(request.run_selection())
        .await
}

#[tauri::command]
/// Returns at most twenty understandable module summaries for one run.
pub async fn query_deep_map_run_modules(
    request: QueryDeepMapRunModulesRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapRunModulesResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_run_modules(request.run_selection(), request.cursor())
        .await
}

#[tauri::command]
/// Returns at most fifty safe resolved exploration targets for one module.
pub async fn query_deep_map_module_steps(
    request: QueryDeepMapModuleStepsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapModuleStepsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_module_steps(
        request.run_selection(),
        request.module_selection(),
        request.cursor(),
    )
    .await
}

#[tauri::command]
/// Returns at most fifty exact current Atlas effects for one published Card.
pub async fn query_deep_map_atlas_impact(
    request: QueryDeepMapAtlasImpactRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapAtlasImpactResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_deep_map_atlas_impact(
        request.run_selection(),
        request.module_selection(),
        request.cursor(),
    )
    .await
}

#[tauri::command]
/// Requests a checkpoint-producing cooperative pause of the active Deep Map.
pub fn pause_deep_map(
    request: ControlDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    execute_control_deep_map(request, root.inner(), CompositionRoot::pause_deep_map)
}

#[tauri::command]
/// Resumes the exact plan prefix retained by a completed pause.
pub fn resume_deep_map(
    request: ControlDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    execute_control_deep_map(request, root.inner(), CompositionRoot::resume_deep_map)
}

#[tauri::command]
/// Cancels active work or discards a retained paused checkpoint.
pub fn cancel_deep_map(
    request: ControlDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    execute_control_deep_map(request, root.inner(), CompositionRoot::cancel_deep_map)
}

#[tauri::command]
/// Queues a bounded rebuild for the Core-owned active project without accepting an identity.
pub fn rebuild_project_index(
    request: RebuildProjectIndexRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<RebuildProjectIndexResponseV1, CommandErrorV1> {
    execute_rebuild_project_index(request, root.inner())
}

#[tauri::command]
/// Removes only the Core-owned active worktree's recent-list projection.
pub async fn remove_project(
    request: RemoveProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
    execute_remove_project(request, root.inner()).await
}

#[tauri::command]
/// Non-destructively removes one exact worktree ID from the durable catalog.
pub async fn remove_catalog_project(
    request: RemoveCatalogProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
    execute_remove_catalog_project(request, root.inner()).await
}

#[tauri::command]
/// Returns process health metadata when the request uses the current protocol version.
pub fn query_health(
    request: HealthRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<HealthResponseV1, CommandErrorV1> {
    execute_query_health(request, root.inner())
}

#[tauri::command]
/// Reads the global model Settings without contacting a provider.
pub async fn query_settings(
    request: QuerySettingsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    execute_query_settings(request, root.inner()).await
}

#[tauri::command]
/// Validates and stores one closed active provider configuration or clears it.
pub async fn configure_model_provider(
    request: ConfigureModelProviderRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    execute_configure_model_provider(request, root.inner()).await
}

#[tauri::command]
/// Stores a one-way API key for the current Core-owned provider without network access.
pub async fn set_model_provider_credential(
    request: SetModelProviderCredentialRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    execute_set_model_provider_credential(request, root.inner()).await
}

#[tauri::command]
/// Deletes the credential belonging to the current Core-owned provider.
pub async fn delete_model_provider_credential(
    request: DeleteModelProviderCredentialRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    execute_delete_model_provider_credential(request, root.inner()).await
}

#[tauri::command]
/// Explicitly lists bounded local models from the current Core-owned provider.
pub async fn discover_provider_models(
    request: DiscoverProviderModelsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProviderModelsResponseV1, CommandErrorV1> {
    execute_discover_provider_models(request, root.inner()).await
}

#[tauri::command]
/// Runs one explicit bounded Core-owned capability probe for a closed role.
pub async fn probe_model_role(
    request: ProbeModelRoleRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    execute_probe_model_role(request, root.inner()).await
}

#[tauri::command]
/// Requests cooperative cancellation of the one active Core-owned model operation.
pub fn cancel_model_probe(
    request: CancelModelProbeRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<CancelModelProbeResponseV1, CommandErrorV1> {
    execute_cancel_model_probe(request, root.inner())
}

#[tauri::command]
/// Reads active-project ignore and safe-command Settings without accepting a project identity.
pub async fn query_project_settings(
    request: QueryProjectSettingsRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
    execute_query_project_settings(request, root.inner()).await
}

#[tauri::command]
/// Confirms only command IDs from the exact current Core-reconstructed catalog.
pub async fn confirm_project_command_allowlist(
    request: ConfirmProjectCommandAllowlistRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
    execute_confirm_project_command_allowlist(request, root.inner()).await
}

fn execute_query_health(
    request: HealthRequestV1,
    root: &CompositionRoot,
) -> Result<HealthResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    Ok(root.query_health())
}

async fn execute_query_settings(
    request: QuerySettingsRequestV1,
    root: &CompositionRoot,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_settings().await
}

async fn execute_configure_model_provider(
    request: ConfigureModelProviderRequestV1,
    root: &CompositionRoot,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let expected = settings_version_from_v1(request.expected_settings_revision())?;
    root.configure_model_provider(expected, request.provider_kind(), request.endpoint_origin())
        .await
}

async fn execute_set_model_provider_credential(
    request: SetModelProviderCredentialRequestV1,
    root: &CompositionRoot,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    let (protocol_version, expected_revision, secret_bytes) = request.into_parts();
    let parsed_secret = a3_application::ProviderApiKey::from_bytes(secret_bytes);
    if protocol_version != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let expected = settings_version_from_v1(&expected_revision)?;
    let secret = parsed_secret.map_err(|_| {
        CommandErrorV1::settings(a3_protocol::ErrorCodeV1::ProviderCredentialInvalid)
    })?;
    root.set_model_provider_credential(expected, secret).await
}

async fn execute_delete_model_provider_credential(
    request: DeleteModelProviderCredentialRequestV1,
    root: &CompositionRoot,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let expected = settings_version_from_v1(request.expected_settings_revision())?;
    root.delete_model_provider_credential(expected).await
}

async fn execute_discover_provider_models(
    request: DiscoverProviderModelsRequestV1,
    root: &CompositionRoot,
) -> Result<ProviderModelsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let expected = settings_version_from_v1(request.expected_settings_revision())?;
    root.discover_provider_models(expected).await
}

async fn execute_probe_model_role(
    request: ProbeModelRoleRequestV1,
    root: &CompositionRoot,
) -> Result<SettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let expected = settings_version_from_v1(request.expected_settings_revision())?;
    root.probe_model_role(expected, &request).await
}

fn execute_cancel_model_probe(
    request: CancelModelProbeRequestV1,
    root: &CompositionRoot,
) -> Result<CancelModelProbeResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    Ok(root.cancel_model_probe())
}

async fn execute_query_project_settings(
    request: QueryProjectSettingsRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_project_settings().await
}

async fn execute_confirm_project_command_allowlist(
    request: ConfirmProjectCommandAllowlistRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let catalog_id = catalog_id_from_v1(request.expected_catalog_id())?;
    let revision = allowlist_version_from_v1(request.expected_allowlist_revision())?;
    let command_ids = command_ids_from_v1(request.command_ids())?;
    root.confirm_project_command_allowlist(catalog_id, revision, command_ids)
        .await
}

async fn execute_open_project(
    request: OpenProjectRequestV1,
    root: &CompositionRoot,
) -> Result<OpenProjectResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.open_project().await
}

async fn execute_list_recent_projects(
    request: ListRecentProjectsRequestV1,
    root: &CompositionRoot,
) -> Result<RecentProjectsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.list_recent_projects().await
}

async fn execute_query_project_catalog(
    request: QueryProjectCatalogRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectCatalogResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_project_catalog_query_from_v1(&request)?;
    root.query_project_catalog(&query).await
}

async fn execute_activate_catalog_project(
    request: ActivateCatalogProjectRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let worktree_id = map_worktree_id_from_v1(request.worktree_id())?;
    root.activate_catalog_project(worktree_id).await
}

async fn execute_restore_last_project(
    request: RestoreLastProjectRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectActivationResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.restore_last_project().await
}

async fn execute_query_project_status(
    request: QueryProjectStatusRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectStatusResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.query_project_status().await
}

fn execute_query_index_activity(
    request: QueryIndexActivityRequestV1,
    root: &CompositionRoot,
) -> Result<IndexActivityResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    Ok(root.query_index_activity())
}

async fn execute_query_index_overview(
    request: QueryIndexOverviewRequestV1,
    root: &CompositionRoot,
) -> Result<IndexOverviewResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.query_index_overview().await
}

async fn execute_query_module_card_freshness(
    request: QueryModuleCardFreshnessRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleCardFreshnessResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.query_module_card_freshness().await
}

async fn execute_query_module_card_detail(
    request: QueryModuleCardDetailRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleCardDetailResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    match (
        request.module_id(),
        request.run_selection(),
        request.module_selection(),
    ) {
        (Some(_), None, None) => {
            let query = map_module_card_detail_query_from_v1(&request)?;
            root.query_module_card_detail(&query).await
        }
        (None, Some(run), Some(module)) => {
            root.query_deep_map_module_card_detail(run, module).await
        }
        _ => Err(crate::invalid_module_card_detail_query()),
    }
}

async fn execute_query_module_card_evidence(
    request: QueryModuleCardEvidenceRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleCardEvidenceResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_module_card_evidence_query_from_v1(&request)?;
    root.query_module_card_evidence(&query).await
}

async fn execute_query_project_map_source_preview(
    request: QueryProjectMapSourcePreviewRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapSourcePreviewResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_project_map_source_preview_query_from_v1(&request)?;
    root.query_project_map_source_preview(&query).await
}

async fn execute_query_project_map_scene(
    request: QueryProjectMapSceneRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapSceneResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_project_map_scene_query_from_v1(&request)?;
    root.query_project_map_scene(&query).await
}

async fn execute_query_project_map_atlas_scene(
    request: QueryProjectMapAtlasSceneRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapAtlasSceneResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let selection = request
        .selection()
        .map(map_selection_from_v1)
        .transpose()
        .map_err(|_| invalid_project_map_atlas_query())?;
    root.query_project_map_atlas_scene(&a3_application::ProjectMapAtlasSceneQuery::new(selection))
        .await
}

async fn execute_query_project_map_entity_context(
    request: QueryProjectMapEntityContextRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapEntityContextResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let selection = map_selection_from_v1(request.selection())
        .map_err(|_| invalid_project_map_atlas_query())?;
    root.query_project_map_entity_context(selection).await
}

async fn execute_query_project_map_inventory_page(
    request: QueryProjectMapInventoryPageRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapInventoryPageResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query =
        map_inventory_query_from_v1(&request).map_err(|_| invalid_project_map_atlas_query())?;
    root.query_project_map_inventory_page(&query).await
}

async fn execute_query_project_map_flow_scene(
    request: QueryProjectMapFlowSceneRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapFlowSceneResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_flow_query_from_v1(&request).map_err(|_| invalid_project_map_atlas_query())?;
    root.query_project_map_flow_scene(&query).await
}

fn invalid_project_map_atlas_query() -> CommandErrorV1 {
    CommandErrorV1::project_open(a3_protocol::ErrorCodeV1::InvalidProjectMapSceneQuery)
}

async fn execute_query_module_tree(
    request: QueryModuleTreeRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleTreeResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_module_tree_query_from_v1(&request)?;
    root.query_module_tree(&query).await
}

async fn execute_query_module_dependency_graph(
    request: QueryModuleDependencyGraphRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleDependencyGraphResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_module_dependency_graph_query_from_v1(&request)?;
    root.query_module_dependency_graph(&query).await
}

async fn execute_query_module_runtime_map(
    request: QueryModuleRuntimeMapRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleRuntimeMapResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_module_runtime_map_query_from_v1(&request)?;
    root.query_module_runtime_map(&query).await
}

async fn execute_query_module_runtime_flow(
    request: QueryModuleRuntimeFlowRequestV1,
    root: &CompositionRoot,
) -> Result<ModuleRuntimeFlowResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_module_runtime_flow_query_from_v1(&request)?;
    root.query_module_runtime_flow(&query).await
}

async fn execute_query_project_map_search(
    request: QueryProjectMapSearchRequestV1,
    root: &CompositionRoot,
) -> Result<ProjectMapSearchResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_project_map_search_query_from_v1(&request)?;
    root.query_project_map_search(&query).await
}

async fn execute_query_task_lens_tasks(
    request: QueryTaskLensTasksRequestV1,
    root: &CompositionRoot,
) -> Result<TaskLensTasksResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.query_task_lens_tasks().await
}

async fn execute_query_task_lens_task(
    request: QueryTaskLensTaskRequestV1,
    root: &CompositionRoot,
) -> Result<TaskLensTaskResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_task_lens_task_id_from_v1(&request)?;
    root.query_task_lens_task(task_id).await
}

async fn execute_compile_task_lens(
    request: CompileTaskLensRequestV1,
    root: &CompositionRoot,
) -> Result<TaskLensCompileResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let (task_id, step_id) = map_task_lens_selection_from_v1(&request)?;
    root.compile_task_lens(task_id, step_id).await
}

async fn execute_query_agent_sessions(
    request: QueryAgentSessionsRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionsResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let cursor = request
        .before_updated_at_unix_millis()
        .map(parse_canonical_positive_u64)
        .transpose()
        .map_err(|()| invalid_agent_session())?;
    let query = AgentSessionListQuery::new(
        request.search().map(str::to_owned),
        request.include_archived(),
        cursor,
        request.limit(),
    )
    .map_err(|_| invalid_agent_session())?;
    root.query_agent_sessions(query).await
}

fn execute_query_agent_slash_commands(
    request: QueryAgentSlashCommandsRequestV1,
) -> Result<AgentSlashCommandsResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let mode = map_session_mode(request.mode());
    let mut entries = Vec::with_capacity(SLASH_COMMANDS.len() + SLASH_COMMAND_LENSES.len());
    entries.extend(SLASH_COMMANDS.into_iter().map(|descriptor| {
        let command = descriptor.command();
        AgentSlashCommandV1::new(
            format!("/{}", command.name()),
            descriptor.title().to_owned(),
            descriptor.description().to_owned(),
            AgentSlashCommandRoleV1::Primary,
            command.available_in(mode),
            map_research_depth_to_v1(command.depth()),
            command.empty_input_behavior() == SlashCommandEmptyInput::Reject,
            None,
        )
    }));
    entries.extend(SLASH_COMMAND_LENSES.into_iter().map(|descriptor| {
        AgentSlashCommandV1::new(
            format!("/{}", descriptor.lens().name()),
            descriptor.title().to_owned(),
            descriptor.description().to_owned(),
            AgentSlashCommandRoleV1::Lens,
            true,
            AgentResearchDepthV1::Thorough,
            false,
            Some("/review".to_owned()),
        )
    }));
    Ok(AgentSlashCommandsResponseV1::new(entries))
}

async fn execute_query_agent_session(
    request: QueryAgentSessionRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let session_id = decode_agent_session_id(request.session_id())?;
    let before = request
        .before_sequence()
        .map(parse_canonical_positive_u64)
        .transpose()
        .map_err(|()| invalid_agent_session())?;
    if request.limit() == 0 || request.limit() > 128 {
        return Err(invalid_agent_session());
    }
    root.query_agent_session(session_id, before, request.limit())
        .await
}

async fn execute_query_agent_session_v2(
    request: QueryAgentSessionRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV2, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let session_id = decode_agent_session_id(request.session_id())?;
    let before = request
        .before_sequence()
        .map(parse_canonical_positive_u64)
        .transpose()
        .map_err(|()| invalid_agent_session())?;
    if request.limit() == 0 || request.limit() > 128 {
        return Err(invalid_agent_session());
    }
    root.query_agent_session_v2(session_id, before, request.limit())
        .await
}

async fn execute_query_agent_ask_research_turns(
    request: QueryAgentAskResearchTurnsRequestV1,
    root: &CompositionRoot,
) -> Result<AgentAskResearchTurnsResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let session_id = AgentSessionId::from_bytes(
        decode_stable_id(request.session_id()).map_err(|_| invalid_agent_session())?,
    );
    root.query_agent_ask_research_turns(session_id).await
}

async fn execute_query_agent_ask_research_detail(
    request: QueryAgentAskResearchDetailRequestV1,
    root: &CompositionRoot,
) -> Result<AgentAskResearchDetailResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let session_id = AgentSessionId::from_bytes(
        decode_stable_id(request.session_id()).map_err(|_| invalid_agent_session())?,
    );
    let sequence = a3_domain::AgentSessionSequence::new(
        parse_canonical_positive_u64(request.user_sequence())
            .map_err(|_| invalid_agent_session())?,
    )
    .map_err(|_| invalid_agent_session())?;
    root.query_agent_ask_research_detail(session_id, sequence)
        .await
}

async fn execute_query_agent_ask_research_sources(
    request: QueryAgentAskResearchSourcesRequestV1,
    root: &CompositionRoot,
) -> Result<AgentAskResearchSourcesResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let session_id = AgentSessionId::from_bytes(
        decode_stable_id(request.session_id()).map_err(|_| invalid_agent_session())?,
    );
    let sequence = a3_domain::AgentSessionSequence::new(
        parse_canonical_positive_u64(request.user_sequence())
            .map_err(|_| invalid_agent_session())?,
    )
    .map_err(|_| invalid_agent_session())?;
    root.query_agent_ask_research_sources(session_id, sequence, request.cursor())
        .await
}

async fn execute_query_agent_ask_research_source_preview(
    request: QueryAgentAskResearchSourcePreviewRequestV1,
    root: &CompositionRoot,
) -> Result<AgentAskResearchSourcePreviewResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let session_id = AgentSessionId::from_bytes(
        decode_stable_id(request.session_id()).map_err(|_| invalid_agent_session())?,
    );
    let sequence = a3_domain::AgentSessionSequence::new(
        parse_canonical_positive_u64(request.user_sequence())
            .map_err(|_| invalid_agent_session())?,
    )
    .map_err(|_| invalid_agent_session())?;
    decode_stable_id(request.source_ref()).map_err(|_| invalid_agent_session())?;
    root.query_agent_ask_research_source_preview(session_id, sequence, request.source_ref())
        .await
}

async fn execute_query_agent_diagram_artifacts(
    request: QueryAgentDiagramArtifactsRequestV1,
    root: &CompositionRoot,
) -> Result<AgentDiagramArtifactsResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let session_id = decode_agent_session_id(request.session_id())?;
    let user_sequence = a3_domain::AgentSessionSequence::new(
        parse_canonical_positive_u64(request.user_sequence())
            .map_err(|_| invalid_agent_session())?,
    )
    .map_err(|_| invalid_agent_session())?;
    root.query_agent_diagram_artifacts(session_id, user_sequence)
        .await
}

async fn execute_query_agent_diagram_artifact(
    request: QueryAgentDiagramArtifactRequestV1,
    root: &CompositionRoot,
) -> Result<AgentDiagramArtifactResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    require_diagram_artifact_ref(request.artifact_ref())?;
    root.query_agent_diagram_artifact(
        decode_agent_session_id(request.session_id())?,
        request.artifact_ref(),
    )
    .await
}

async fn execute_export_agent_diagram(
    request: ExportAgentDiagramRequestV1,
    app: &AppHandle,
    root: &CompositionRoot,
) -> Result<AgentDiagramExportResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    require_diagram_artifact_ref(request.artifact_ref())?;
    let response = root
        .query_agent_diagram_artifact(
            decode_agent_session_id(request.session_id())?,
            request.artifact_ref(),
        )
        .await?;
    let artifact = match response.result {
        a3_protocol::AgentDiagramArtifactResultV1::Available { artifact } => artifact,
        a3_protocol::AgentDiagramArtifactResultV1::NoProject
        | a3_protocol::AgentDiagramArtifactResultV1::NotFound => {
            return Ok(AgentDiagramExportResponseV1 {
                protocol_version: ProtocolVersion::CURRENT,
                result: AgentDiagramExportResultV1::NotFound,
            });
        }
    };
    let bytes = match crate::diagram_export::validate_rendered_payload(
        request.format(),
        request.rendered_payload(),
    ) {
        Ok(bytes) => bytes,
        Err(crate::diagram_export::DiagramExportFailure::InvalidPayload) => {
            return Ok(AgentDiagramExportResponseV1 {
                protocol_version: ProtocolVersion::CURRENT,
                result: AgentDiagramExportResultV1::InvalidPayload,
            });
        }
        Err(crate::diagram_export::DiagramExportFailure::Unavailable) => {
            return Ok(AgentDiagramExportResponseV1 {
                protocol_version: ProtocolVersion::CURRENT,
                result: AgentDiagramExportResultV1::Failed,
            });
        }
    };
    let (extension, filter_name) = match request.format() {
        AgentDiagramExportFormatV1::Svg => ("svg", "SVG-Diagramm"),
        AgentDiagramExportFormatV1::Png => ("png", "PNG-Bild"),
    };
    let _theme = request.theme();
    let selection = app
        .dialog()
        .file()
        .set_title("A^3 Diagramm exportieren")
        .set_file_name(crate::diagram_export::safe_file_name(
            &artifact.summary.title,
            extension,
        ))
        .add_filter(filter_name, &[extension])
        .blocking_save_file();
    let Some(selection) = selection else {
        return Ok(AgentDiagramExportResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentDiagramExportResultV1::Cancelled,
        });
    };
    let Ok(path) = selection.into_path() else {
        return Ok(AgentDiagramExportResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentDiagramExportResultV1::Failed,
        });
    };
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        return Ok(AgentDiagramExportResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentDiagramExportResultV1::Failed,
        });
    }
    let result = match crate::diagram_export::write_atomically(&path, &bytes) {
        Ok(()) => AgentDiagramExportResultV1::Exported,
        Err(_) => AgentDiagramExportResultV1::Failed,
    };
    Ok(AgentDiagramExportResponseV1 {
        protocol_version: ProtocolVersion::CURRENT,
        result,
    })
}

fn require_diagram_artifact_ref(value: &str) -> Result<(), CommandErrorV1> {
    if value.len() == 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid_agent_session())
    }
}

async fn execute_submit_agent_message(
    request: SubmitAgentMessageRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    if !request.context_references().is_empty() {
        return Err(invalid_agent_session());
    }
    let session_id = request
        .session_id()
        .map(decode_agent_session_id)
        .transpose()?;
    let expected = request
        .expected_session_revision()
        .map(parse_agent_session_revision)
        .transpose()?;
    let mode = request.start_mode().map(|mode| match mode {
        AgentSessionModeV1::Ask => AgentSessionMode::Ask,
        AgentSessionModeV1::Plan => AgentSessionMode::Plan,
        AgentSessionModeV1::Agent => AgentSessionMode::Agent,
    });
    if session_id.is_some() != expected.is_some() || session_id.is_some() == mode.is_some() {
        return Err(invalid_agent_session());
    }
    root.submit_agent_message(session_id, expected, mode, request.message().to_owned())
        .await
}

async fn execute_submit_agent_message_v2(
    request: SubmitAgentMessageRequestV2,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    if !request.context_references().is_empty() {
        return Err(invalid_agent_session());
    }
    let session_id = request
        .session_id()
        .map(decode_agent_session_id)
        .transpose()?;
    let expected = request
        .expected_session_revision()
        .map(parse_agent_session_revision)
        .transpose()?;
    let mode = request.start_mode().map(map_session_mode);
    if session_id.is_some() != expected.is_some() || session_id.is_some() == mode.is_some() {
        return Err(invalid_agent_session());
    }
    root.submit_agent_message_v2(
        session_id,
        expected,
        mode,
        map_research_depth(request.research_depth()),
        request.message().to_owned(),
    )
    .await
}

async fn execute_submit_agent_message_v3(
    request: SubmitAgentMessageRequestV3,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    if !request.context_references().is_empty() {
        return Err(invalid_agent_session());
    }
    let session_id = request
        .session_id()
        .map(decode_agent_session_id)
        .transpose()?;
    let expected = request
        .expected_session_revision()
        .map(parse_agent_session_revision)
        .transpose()?;
    let mode = request.start_mode().map(map_session_mode);
    if session_id.is_some() != expected.is_some() || session_id.is_some() == mode.is_some() {
        return Err(invalid_agent_session());
    }
    let explicit_depth = match request.research_depth() {
        AgentResearchDepthSelectionV1::Standard => Some(AgentResearchDepth::Standard),
        AgentResearchDepthSelectionV1::Thorough => Some(AgentResearchDepth::Thorough),
        AgentResearchDepthSelectionV1::Command => None,
    };
    root.submit_agent_message_v3(
        session_id,
        expected,
        mode,
        explicit_depth,
        request.message().to_owned(),
    )
    .await
}

async fn execute_continue_agent_research(
    request: ContinueAgentResearchRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.continue_agent_research(
        decode_agent_session_id(request.session_id())?,
        parse_agent_session_revision(request.expected_session_revision())?,
        map_research_depth(request.research_depth()),
    )
    .await
}

const fn map_session_mode(mode: AgentSessionModeV1) -> AgentSessionMode {
    match mode {
        AgentSessionModeV1::Ask => AgentSessionMode::Ask,
        AgentSessionModeV1::Plan => AgentSessionMode::Plan,
        AgentSessionModeV1::Agent => AgentSessionMode::Agent,
    }
}

const fn map_research_depth(depth: AgentResearchDepthV1) -> AgentResearchDepth {
    match depth {
        AgentResearchDepthV1::Standard => AgentResearchDepth::Standard,
        AgentResearchDepthV1::Thorough => AgentResearchDepth::Thorough,
    }
}

const fn map_research_depth_to_v1(depth: AgentResearchDepth) -> AgentResearchDepthV1 {
    match depth {
        AgentResearchDepth::Standard => AgentResearchDepthV1::Standard,
        AgentResearchDepth::Thorough => AgentResearchDepthV1::Thorough,
    }
}

async fn execute_control_agent_session(
    request: ControlAgentSessionRequestV1,
    root: &CompositionRoot,
) -> Result<AgentSessionResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let session_id = decode_agent_session_id(request.session_id())?;
    let expected = parse_agent_session_revision(request.expected_session_revision())?;
    let mutation = match request.action() {
        AgentSessionControlActionV1::Rename { title } => {
            PresentationMutation::Rename(title.clone())
        }
        AgentSessionControlActionV1::Archive => PresentationMutation::Archive,
        AgentSessionControlActionV1::Unarchive => PresentationMutation::Unarchive,
        AgentSessionControlActionV1::DeletePresentation => PresentationMutation::Delete,
        AgentSessionControlActionV1::SwitchToPlan => PresentationMutation::SwitchToPlan,
        AgentSessionControlActionV1::ImplementPlan { plan_revision } => {
            return root
                .implement_agent_session_plan(session_id, expected, *plan_revision)
                .await;
        }
        AgentSessionControlActionV1::Pause => {
            return root
                .control_agent_session_runtime(
                    session_id,
                    expected,
                    AgentTaskControlActionV1::Pause,
                )
                .await;
        }
        AgentSessionControlActionV1::Resume => {
            return root
                .control_agent_session_runtime(
                    session_id,
                    expected,
                    AgentTaskControlActionV1::Resume,
                )
                .await;
        }
        AgentSessionControlActionV1::Cancel => {
            return root
                .control_agent_session_runtime(
                    session_id,
                    expected,
                    AgentTaskControlActionV1::Cancel,
                )
                .await;
        }
    };
    root.control_agent_session_presentation(session_id, expected, mutation)
        .await
}

async fn execute_query_ui_preferences(
    request: QueryUiPreferencesRequestV1,
    root: &CompositionRoot,
) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    root.query_ui_preferences().await
}

async fn execute_update_agent_workspace_layout(
    request: UpdateAgentWorkspaceLayoutRequestV1,
    root: &CompositionRoot,
) -> Result<UiPreferencesResponseV1, CommandErrorV1> {
    require_agent_session_protocol(request.protocol_version())?;
    let expected = parse_ui_preferences_revision(request.expected_revision())?;
    let layout = AgentWorkspaceLayout::new(
        request.session_rail_width(),
        request.inspector_width(),
        request.session_rail_collapsed(),
        request.inspector_collapsed(),
    )
    .map_err(|_| invalid_agent_session())?;
    root.update_agent_workspace_layout(expected, layout).await
}

fn require_agent_session_protocol(version: ProtocolVersion) -> Result<(), CommandErrorV1> {
    if version == ProtocolVersion::CURRENT {
        Ok(())
    } else {
        Err(CommandErrorV1::unsupported_protocol_version())
    }
}

fn decode_agent_session_id(value: &str) -> Result<AgentSessionId, CommandErrorV1> {
    decode_stable_id(value)
        .map(AgentSessionId::from_bytes)
        .map_err(|()| invalid_agent_session())
}

fn decode_agent_user_sequence(
    value: &str,
) -> Result<a3_domain::AgentSessionSequence, CommandErrorV1> {
    a3_domain::AgentSessionSequence::new(
        parse_canonical_positive_u64(value).map_err(|_| invalid_agent_session())?,
    )
    .map_err(|_| invalid_agent_session())
}

fn parse_agent_session_revision(value: &str) -> Result<AgentSessionRevision, CommandErrorV1> {
    parse_canonical_positive_u64(value)
        .and_then(|revision| AgentSessionRevision::new(revision).map_err(|_| ()))
        .map_err(|()| invalid_agent_session())
}

fn parse_ui_preferences_revision(value: &str) -> Result<UiPreferencesStoreVersion, CommandErrorV1> {
    if value == "0" {
        return Ok(UiPreferencesStoreVersion::EMPTY);
    }
    parse_canonical_positive_u64(value)
        .and_then(|revision| UiPreferencesStoreVersion::new(revision).map_err(|_| ()))
        .map_err(|()| invalid_agent_session())
}

fn invalid_agent_session() -> CommandErrorV1 {
    CommandErrorV1::agent_session(a3_protocol::ErrorCodeV1::InvalidAgentSessionRequest)
}

async fn execute_query_agent_goal(
    request: QueryAgentGoalRequestV1,
    root: &CompositionRoot,
) -> Result<AgentGoalResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_agent_goal_task_id_from_v1(&request)?;
    root.query_agent_goal(task_id).await
}

async fn execute_query_agent_activity(
    request: QueryAgentActivityRequestV1,
    root: &CompositionRoot,
) -> Result<AgentActivityResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_agent_activity_task_id_from_v1(&request)?;
    root.query_agent_activity(task_id).await
}

async fn execute_query_agent_inspection(
    request: QueryAgentInspectionRequestV1,
    root: &CompositionRoot,
) -> Result<AgentInspectionResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_agent_inspection_task_id_from_v1(&request)?;
    root.query_agent_inspection(task_id).await
}

async fn execute_query_agent_inspection_log(
    request: QueryAgentInspectionLogRequestV1,
    root: &CompositionRoot,
) -> Result<AgentInspectionLogResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let (task_id, revision, inspection_id, stream, offset, limit) =
        map_agent_inspection_log_query_from_v1(&request)?;
    root.query_agent_inspection_log(task_id, revision, inspection_id, stream, offset, limit)
        .await
}

async fn execute_query_agent_approval(
    request: QueryAgentApprovalRequestV1,
    root: &CompositionRoot,
) -> Result<AgentApprovalResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_agent_approval_task_id_from_v1(&request)?;
    root.query_agent_approval(task_id).await
}

async fn execute_control_agent_approval(
    request: ControlAgentApprovalRequestV1,
    root: &CompositionRoot,
) -> Result<AgentApprovalControlResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let (task_id, approval_revision, ledger_revision, ledger_store_version, action) =
        map_agent_approval_control_from_v1(&request)?;
    root.control_agent_approval(
        task_id,
        approval_revision,
        ledger_revision,
        ledger_store_version,
        action,
    )
    .await
}

async fn execute_query_agent_task_recovery(
    request: QueryAgentTaskRecoveryRequestV1,
    root: &CompositionRoot,
) -> Result<AgentTaskRecoveryResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task_id = map_agent_task_recovery_task_id_from_v1(&request)?;
    root.query_agent_task_recovery(task_id).await
}

async fn execute_control_agent_task_run(
    request: ControlAgentTaskRunRequestV1,
    root: &CompositionRoot,
) -> Result<AgentTaskControlResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let (task_id, ledger_revision, ledger_store_version, action) =
        map_agent_task_control_from_v1(&request)?;
    root.control_agent_task_run(task_id, ledger_revision, ledger_store_version, action)
        .await
}

async fn execute_create_agent_goal(
    request: CreateAgentGoalRequestV1,
    root: &CompositionRoot,
) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let draft = map_create_agent_goal_from_v1(&request)?;
    root.create_agent_goal(draft).await
}

async fn execute_revise_agent_goal(
    request: ReviseAgentGoalRequestV1,
    root: &CompositionRoot,
) -> Result<AgentGoalMutationResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let (task_id, expected_revision, draft, reason) = map_revise_agent_goal_from_v1(&request)?;
    root.revise_agent_goal(task_id, expected_revision, draft, reason)
        .await
}

async fn execute_query_repository_tree(
    request: QueryRepositoryTreeRequestV1,
    root: &CompositionRoot,
) -> Result<RepositoryTreeResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let query = map_repository_tree_query_from_v1(&request)?;
    root.query_repository_tree(&query).await
}

async fn execute_query_deep_map(
    request: QueryDeepMapRequestV1,
    root: &CompositionRoot,
) -> Result<DeepMapStatusResponseV3, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    Ok(root.query_deep_map_status_v3().await)
}

async fn execute_start_deep_map(
    request: StartDeepMapRequestV2,
    root: &CompositionRoot,
) -> Result<DeepMapStartResponseV2, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let mode = match request.mode() {
        DeepMapModeV2::Fast => DeepMapMode::Fast,
        DeepMapModeV2::Standard => DeepMapMode::Standard,
        DeepMapModeV2::Thorough => DeepMapMode::Thorough,
    };
    root.start_deep_map_v2(mode).await
}

fn execute_control_deep_map(
    request: ControlDeepMapRequestV1,
    root: &CompositionRoot,
    operation: fn(&CompositionRoot) -> Result<DeepMapControlResponseV1, CommandErrorV1>,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    operation(root)
}

fn execute_rebuild_project_index(
    request: RebuildProjectIndexRequestV1,
    root: &CompositionRoot,
) -> Result<RebuildProjectIndexResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.rebuild_project_index()
}

async fn execute_remove_project(
    request: RemoveProjectRequestV1,
    root: &CompositionRoot,
) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.remove_project().await
}

async fn execute_remove_catalog_project(
    request: RemoveCatalogProjectRequestV1,
    root: &CompositionRoot,
) -> Result<RemoveProjectResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let worktree_id = map_worktree_id_from_v1(request.worktree_id())?;
    root.remove_catalog_project(worktree_id).await
}

#[cfg(test)]
mod tests {
    use super::{
        execute_activate_catalog_project, execute_compile_task_lens,
        execute_control_agent_approval, execute_control_agent_task_run, execute_control_deep_map,
        execute_create_agent_goal, execute_list_recent_projects, execute_open_project,
        execute_query_agent_activity, execute_query_agent_approval, execute_query_agent_goal,
        execute_query_agent_inspection, execute_query_agent_inspection_log,
        execute_query_agent_task_recovery, execute_query_deep_map, execute_query_health,
        execute_query_index_activity, execute_query_index_overview,
        execute_query_module_card_detail, execute_query_module_card_evidence,
        execute_query_module_card_freshness, execute_query_module_dependency_graph,
        execute_query_module_runtime_flow, execute_query_module_runtime_map,
        execute_query_module_tree, execute_query_project_catalog, execute_query_project_map_search,
        execute_query_project_settings, execute_query_project_status,
        execute_query_repository_tree, execute_query_task_lens_task, execute_query_task_lens_tasks,
        execute_rebuild_project_index, execute_remove_catalog_project, execute_remove_project,
        execute_restore_last_project, execute_revise_agent_goal, execute_start_deep_map,
    };
    use crate::CompositionRoot;
    use a3_application::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectDirectoryPicker,
        ProjectDirectorySelectionError, ProjectOpenPreparation, ProjectPathDisplay,
        ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
        ProjectReconciliationConfirmer, ProjectReconciliationProposal, RecentProject,
        RecentProjectLimit, StoredProjectTarget,
    };
    use a3_domain::{ApplicationVersion, Platform, ProjectId, ProjectIdentity, WorktreeId};
    use a3_protocol::{
        ActivateCatalogProjectRequestV1, AgentActivityResultV1, AgentApprovalControlResultV1,
        AgentApprovalResultV1, AgentGoalResultV1, AgentInspectionLogResultV1,
        AgentInspectionResultV1, AgentInspectionStreamV1, AgentTaskControlResultV1,
        AgentTaskRecoveryResultV1, CompileTaskLensRequestV1, ControlAgentApprovalRequestV1,
        ControlAgentTaskRunRequestV1, ControlDeepMapRequestV1, CreateAgentGoalRequestV1,
        DeepMapModeV2, DeepMapStatusResultV3, ErrorCodeV1, HealthRequestV1, IndexActivityResultV1,
        IndexOverviewResultV1, ListRecentProjectsRequestV1, ModuleCardDetailResultV1,
        ModuleCardEvidenceResultV1, ModuleCardFreshnessResultV1, ModuleDependencyGraphResultV1,
        ModuleRuntimeFlowKindV1, ModuleRuntimeFlowResultV1, ModuleRuntimeMapResultV1,
        ModuleTreeResultV1, OpenProjectRequestV1, ProjectCatalogDirectionV1,
        ProjectMapSearchResultV1, ProjectStatusResultV1, ProtocolVersion,
        QueryAgentActivityRequestV1, QueryAgentApprovalRequestV1, QueryAgentGoalRequestV1,
        QueryAgentInspectionLogRequestV1, QueryAgentInspectionRequestV1,
        QueryAgentTaskRecoveryRequestV1, QueryDeepMapRequestV1, QueryIndexActivityRequestV1,
        QueryIndexOverviewRequestV1, QueryModuleCardDetailRequestV1,
        QueryModuleCardEvidenceRequestV1, QueryModuleCardFreshnessRequestV1,
        QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
        QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectCatalogRequestV1,
        QueryProjectMapSearchRequestV1, QueryProjectSettingsRequestV1, QueryProjectStatusRequestV1,
        QueryRepositoryTreeRequestV1, QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1,
        RebuildProjectIndexRequestV1, RemoveCatalogProjectRequestV1, RemoveProjectRequestV1,
        RepositoryTreeResultV1, RestoreLastProjectRequestV1, ReviseAgentGoalRequestV1,
        StartDeepMapRequestV2, TaskLensCompileResultV1, TaskLensTaskResultV1,
        TaskLensTasksResultV1,
    };
    use futures::executor::block_on;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CancelledPicker;

    impl ProjectDirectoryPicker for CancelledPicker {
        fn pick_project_directory(
            &self,
        ) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
            Ok(None)
        }
    }

    #[derive(Debug)]
    struct FixedPicker(PathBuf);

    impl ProjectDirectoryPicker for FixedPicker {
        fn pick_project_directory(
            &self,
        ) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
            Ok(Some(self.0.clone()))
        }
    }

    #[derive(Debug)]
    struct EmptyStore;

    impl KnowledgeStore for EmptyStore {
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
            Box::pin(async { Ok(ProjectId::from_bytes([1; 32])) })
        }

        fn reconcile_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _proposal: &'a ProjectReconciliationProposal,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            Box::pin(async { Err(KnowledgeStoreFailure::IdentityConflict) })
        }

        fn list_recent_projects(
            &self,
            _limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[derive(Debug)]
    struct FailingSwitchStore {
        initial_worktree_id: WorktreeId,
        target: StoredProjectTarget,
        record_calls: Arc<AtomicUsize>,
    }

    impl KnowledgeStore for FailingSwitchStore {
        fn prepare_project_open<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectOpenPreparation> {
            Box::pin(async { Ok(ProjectOpenPreparation::Ready) })
        }

        fn record_opened_project<'a>(
            &'a self,
            project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            self.record_calls.fetch_add(1, Ordering::SeqCst);
            let result = if project.worktree().id() == self.initial_worktree_id {
                Ok(ProjectId::from_bytes([1; 32]))
            } else {
                Err(KnowledgeStoreFailure::Unavailable)
            };
            Box::pin(async move { result })
        }

        fn reconcile_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _proposal: &'a ProjectReconciliationProposal,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            Box::pin(async { Err(KnowledgeStoreFailure::IdentityConflict) })
        }

        fn list_recent_projects(
            &self,
            _limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn resolve_project_catalog_entry(
            &self,
            worktree_id: WorktreeId,
        ) -> KnowledgeStoreFuture<'_, Option<StoredProjectTarget>> {
            let target = (self.target.worktree_id() == worktree_id).then(|| self.target.clone());
            Box::pin(async move { Ok(target) })
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

    fn root() -> Result<CompositionRoot, Box<dyn std::error::Error>> {
        Ok(CompositionRoot::new(
            ApplicationVersion::try_from("0.1.0")?,
            Platform::Windows,
            Arc::new(CancelledPicker),
            Arc::new(CancelledConfirmer),
            Arc::new(EmptyStore),
        )?)
    }

    #[test]
    fn rejects_unsupported_protocol_version_without_executing_payload()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = execute_query_health(HealthRequestV1::new(ProtocolVersion::new(999)), &root);

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn project_settings_query_selects_no_project_without_webview_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let request: QueryProjectSettingsRequestV1 =
            serde_json::from_value(serde_json::json!({"protocolVersion": 1}))?;

        let response = block_on(execute_query_project_settings(request, &root))
            .map_err(|error| format!("project Settings query failed: {:?}", error.code()))?;

        assert_eq!(
            serde_json::to_value(response)?["result"]["status"],
            "noProject"
        );
        Ok(())
    }

    #[test]
    fn project_command_rejects_unsupported_version_before_opening_picker()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_open_project(
            OpenProjectRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn recent_project_command_rejects_unsupported_version_before_storage()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_list_recent_projects(
            ListRecentProjectsRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn project_status_command_rejects_unsupported_version_before_reading_core_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_query_project_status(
            QueryProjectStatusRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn project_status_reports_no_project_before_a_successful_native_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_query_project_status(
            QueryProjectStatusRequestV1::current(),
            &root,
        ));
        let response = match result {
            Ok(response) => response,
            Err(error) => return Err(std::io::Error::other(error.message()).into()),
        };

        assert!(matches!(
            response.result(),
            ProjectStatusResultV1::NoProject
        ));
        Ok(())
    }

    #[test]
    fn index_activity_is_pathless_and_reports_no_project_before_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let response = execute_query_index_activity(QueryIndexActivityRequestV1::current(), &root)
            .map_err(|error| std::io::Error::other(error.message()))?;

        assert!(matches!(
            response.result(),
            IndexActivityResultV1::NoProject
        ));
        Ok(())
    }

    #[test]
    fn index_overview_is_pathless_and_reports_no_project_before_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let response = block_on(execute_query_index_overview(
            QueryIndexOverviewRequestV1::current(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;

        assert!(matches!(
            response.result(),
            IndexOverviewResultV1::NoProject
        ));
        Ok(())
    }

    #[test]
    fn index_overview_rejects_unsupported_version_before_reading_core_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_query_index_overview(
            QueryIndexOverviewRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));
        let error = match result {
            Ok(_) => {
                return Err(std::io::Error::other(
                    "unsupported version unexpectedly queried the active project",
                )
                .into());
            }
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCodeV1::UnsupportedProtocolVersion);
        Ok(())
    }

    #[test]
    fn module_card_freshness_is_pathless_and_reports_no_project_before_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let response = block_on(execute_query_module_card_freshness(
            QueryModuleCardFreshnessRequestV1::current(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;

        assert!(matches!(
            response.result(),
            ModuleCardFreshnessResultV1::NoProject
        ));
        Ok(())
    }

    #[test]
    fn module_card_freshness_rejects_unsupported_version_before_reading_core_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_query_module_card_freshness(
            QueryModuleCardFreshnessRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn module_card_detail_reports_no_project_and_rejects_untrusted_module_tokens()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_module_card_detail(
            QueryModuleCardDetailRequestV1::new(ProtocolVersion::CURRENT, "11".repeat(32)),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ModuleCardDetailResultV1::NoProject
        ));

        for module_id in [
            "aa".repeat(31),
            "GG".repeat(32),
            format!("{}A", "a".repeat(63)),
        ] {
            let result = block_on(execute_query_module_card_detail(
                QueryModuleCardDetailRequestV1::new(ProtocolVersion::CURRENT, module_id),
                &root,
            ));
            assert_eq!(
                result.map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidModuleCardDetailQuery)
            );
        }
        Ok(())
    }

    #[test]
    fn module_card_detail_rejects_version_before_module_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = block_on(execute_query_module_card_detail(
            QueryModuleCardDetailRequestV1::new(
                ProtocolVersion::new(999),
                "not-a-module-id".to_owned(),
            ),
            &root()?,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn module_card_evidence_reports_no_project_and_rejects_untrusted_anchors()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let request = module_card_evidence_request(ProtocolVersion::CURRENT, "77".repeat(32));
        let response = block_on(execute_query_module_card_evidence(request, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ModuleCardEvidenceResultV1::NoProject
        ));

        let invalid = module_card_evidence_request(ProtocolVersion::CURRENT, "GG".repeat(32));
        assert_eq!(
            block_on(execute_query_module_card_evidence(invalid, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidModuleCardEvidenceQuery)
        );
        let inconsistent = QueryModuleCardEvidenceRequestV1::new(
            ProtocolVersion::CURRENT,
            "11".repeat(32),
            "22".repeat(32),
            "11".repeat(32),
            "44".repeat(32),
            "55".repeat(32),
            "66".repeat(32),
            "77".repeat(32),
        );
        assert_eq!(
            block_on(execute_query_module_card_evidence(inconsistent, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidModuleCardEvidenceQuery)
        );
        Ok(())
    }

    #[test]
    fn module_card_evidence_rejects_version_before_anchor_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = block_on(execute_query_module_card_evidence(
            module_card_evidence_request(ProtocolVersion::new(999), "not-an-id".to_owned()),
            &root()?,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    fn module_card_evidence_request(
        version: ProtocolVersion,
        evidence_id: String,
    ) -> QueryModuleCardEvidenceRequestV1 {
        QueryModuleCardEvidenceRequestV1::new(
            version,
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32),
            "44".repeat(32),
            "55".repeat(32),
            "66".repeat(32),
            evidence_id,
        )
    }

    #[test]
    fn module_tree_reports_no_project_and_rejects_untrusted_query_tokens()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_module_tree(
            QueryModuleTreeRequestV1::root(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(response.result(), ModuleTreeResultV1::NoProject));

        for invalid_id in [
            "aa".repeat(31),
            "GG".repeat(32),
            format!("{}A", "a".repeat(63)),
        ] {
            let result = block_on(execute_query_module_tree(
                QueryModuleTreeRequestV1::new(ProtocolVersion::CURRENT, Some(invalid_id), None, 50),
                &root,
            ));
            assert_eq!(
                result.map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidModuleTreeQuery)
            );
        }

        let invalid_limit = block_on(execute_query_module_tree(
            QueryModuleTreeRequestV1::new(ProtocolVersion::CURRENT, None, None, 101),
            &root,
        ));
        assert_eq!(
            invalid_limit.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidModuleTreeQuery)
        );
        Ok(())
    }

    #[test]
    fn module_tree_rejects_unsupported_version_before_query_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let result = block_on(execute_query_module_tree(
            QueryModuleTreeRequestV1::new(
                ProtocolVersion::new(999),
                Some("not-a-module-id".to_owned()),
                None,
                0,
            ),
            &root,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn module_dependency_graph_reports_no_project_and_rejects_untrusted_bounds()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_module_dependency_graph(
            QueryModuleDependencyGraphRequestV1::new(ProtocolVersion::CURRENT, "11".repeat(32), 50),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ModuleDependencyGraphResultV1::NoProject
        ));

        for (module_id, node_limit) in [
            ("aa".repeat(31), 50),
            ("GG".repeat(32), 50),
            ("11".repeat(32), 0),
            ("11".repeat(32), 101),
        ] {
            let result = block_on(execute_query_module_dependency_graph(
                QueryModuleDependencyGraphRequestV1::new(
                    ProtocolVersion::CURRENT,
                    module_id,
                    node_limit,
                ),
                &root,
            ));
            assert_eq!(
                result.map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidModuleDependencyGraphQuery)
            );
        }
        Ok(())
    }

    #[test]
    fn module_dependency_graph_rejects_version_before_payload_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = block_on(execute_query_module_dependency_graph(
            QueryModuleDependencyGraphRequestV1::new(
                ProtocolVersion::new(999),
                "not-a-module-id".to_owned(),
                0,
            ),
            &root()?,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn module_runtime_map_reports_no_project_and_rejects_untrusted_bounds()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_module_runtime_map(
            QueryModuleRuntimeMapRequestV1::new(ProtocolVersion::CURRENT, "11".repeat(32), 20, 20),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ModuleRuntimeMapResultV1::NoProject
        ));

        for (module_id, entrypoint_limit, test_limit) in [
            ("aa".repeat(31), 20, 20),
            ("GG".repeat(32), 20, 20),
            ("11".repeat(32), 0, 20),
            ("11".repeat(32), 20, 257),
        ] {
            let result = block_on(execute_query_module_runtime_map(
                QueryModuleRuntimeMapRequestV1::new(
                    ProtocolVersion::CURRENT,
                    module_id,
                    entrypoint_limit,
                    test_limit,
                ),
                &root,
            ));
            assert_eq!(
                result.map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidModuleRuntimeMapQuery)
            );
        }
        Ok(())
    }

    #[test]
    fn module_runtime_flow_reports_no_project_and_validates_all_seed_tokens()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let request = |run: String, snapshot: String, module: String, symbol: String, limit| {
            QueryModuleRuntimeFlowRequestV1::new(
                ProtocolVersion::CURRENT,
                run,
                snapshot,
                module,
                symbol,
                ModuleRuntimeFlowKindV1::EntrypointCalls,
                limit,
            )
        };
        let valid = "11".repeat(32);
        let response = block_on(execute_query_module_runtime_flow(
            request(
                valid.clone(),
                valid.clone(),
                valid.clone(),
                valid.clone(),
                100,
            ),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ModuleRuntimeFlowResultV1::NoProject
        ));

        for (run, snapshot, module, symbol, limit) in [
            (
                "aa".repeat(31),
                valid.clone(),
                valid.clone(),
                valid.clone(),
                20,
            ),
            (
                valid.clone(),
                "GG".repeat(32),
                valid.clone(),
                valid.clone(),
                20,
            ),
            (
                valid.clone(),
                valid.clone(),
                "aa".repeat(31),
                valid.clone(),
                20,
            ),
            (
                valid.clone(),
                valid.clone(),
                valid.clone(),
                "GG".repeat(32),
                20,
            ),
            (
                valid.clone(),
                valid.clone(),
                valid.clone(),
                valid.clone(),
                101,
            ),
        ] {
            let result = block_on(execute_query_module_runtime_flow(
                request(run, snapshot, module, symbol, limit),
                &root,
            ));
            assert_eq!(
                result.map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidModuleRuntimeFlowQuery)
            );
        }
        Ok(())
    }

    #[test]
    fn repository_tree_reports_no_project_and_rejects_untrusted_query_tokens()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_repository_tree(
            QueryRepositoryTreeRequestV1::root(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            RepositoryTreeResultV1::NoProject
        ));

        let invalid_hex = block_on(execute_query_repository_tree(
            QueryRepositoryTreeRequestV1::new(
                ProtocolVersion::CURRENT,
                Some("C:/untrusted".to_owned()),
                None,
                50,
            ),
            &root,
        ));
        assert_eq!(
            invalid_hex.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidRepositoryTreeQuery)
        );

        let invalid_limit = block_on(execute_query_repository_tree(
            QueryRepositoryTreeRequestV1::new(ProtocolVersion::CURRENT, None, None, 101),
            &root,
        ));
        assert_eq!(
            invalid_limit.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidRepositoryTreeQuery)
        );
        Ok(())
    }

    #[test]
    fn repository_tree_rejects_unsupported_version_before_query_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let result = block_on(execute_query_repository_tree(
            QueryRepositoryTreeRequestV1::new(
                ProtocolVersion::new(999),
                Some("not-hex".to_owned()),
                None,
                0,
            ),
            &root,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn project_map_search_is_pathless_and_validates_version_before_query()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let response = block_on(execute_query_project_map_search(
            QueryProjectMapSearchRequestV1::new(
                ProtocolVersion::CURRENT,
                "launch parser".to_owned(),
            ),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            ProjectMapSearchResultV1::NoProject
        ));

        let invalid = block_on(execute_query_project_map_search(
            QueryProjectMapSearchRequestV1::new(ProtocolVersion::CURRENT, "ab".to_owned()),
            &root,
        ));
        assert_eq!(
            invalid.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidProjectMapSearchQuery)
        );

        let unsupported = block_on(execute_query_project_map_search(
            QueryProjectMapSearchRequestV1::new(ProtocolVersion::new(999), "ab".to_owned()),
            &root,
        ));
        assert_eq!(
            unsupported.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn task_lens_commands_are_pathless_and_validate_versions_before_opaque_selections()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let stable_task_id = "11".repeat(32);
        let stable_step_id = "22".repeat(32);

        let tasks = block_on(execute_query_task_lens_tasks(
            QueryTaskLensTasksRequestV1::new(ProtocolVersion::CURRENT),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(tasks.result(), TaskLensTasksResultV1::NoProject));

        let task = block_on(execute_query_task_lens_task(
            QueryTaskLensTaskRequestV1::new(ProtocolVersion::CURRENT, stable_task_id.clone()),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(task.result(), TaskLensTaskResultV1::NoProject));

        let lens = block_on(execute_compile_task_lens(
            CompileTaskLensRequestV1::new(
                ProtocolVersion::CURRENT,
                stable_task_id.clone(),
                stable_step_id,
            ),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(lens.result(), TaskLensCompileResultV1::NoProject));

        let invalid_task = block_on(execute_query_task_lens_task(
            QueryTaskLensTaskRequestV1::new(ProtocolVersion::CURRENT, "not-an-id".to_owned()),
            &root,
        ));
        assert_eq!(
            invalid_task.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidTaskLensSelection)
        );

        let invalid_step = block_on(execute_compile_task_lens(
            CompileTaskLensRequestV1::new(
                ProtocolVersion::CURRENT,
                stable_task_id,
                "not-an-id".to_owned(),
            ),
            &root,
        ));
        assert_eq!(
            invalid_step.map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidTaskLensSelection)
        );

        let unsupported = block_on(execute_compile_task_lens(
            CompileTaskLensRequestV1::new(
                ProtocolVersion::new(999),
                "not-a-task".to_owned(),
                "not-a-step".to_owned(),
            ),
            &root,
        ));
        assert_eq!(
            unsupported.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn agent_goal_commands_are_pathless_strict_and_validate_version_before_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let task_id = "11".repeat(32);
        let query: QueryAgentGoalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": task_id
        }))?;
        let response = block_on(execute_query_agent_goal(query, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(response.result(), AgentGoalResultV1::NoProject));

        let create: CreateAgentGoalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "draft": {
                "objective": "build the Agent workspace",
                "acceptanceCriteria": [{
                    "criterionId": null,
                    "statement": "the goal remains visible",
                    "requirement": "must"
                }],
                "constraints": ["remain local-only"],
                "nonGoals": ["do not start a run"],
                "userDecisions": ["retain revisions"],
                "successVerification": "reopen and compare the goal"
            }
        }))?;
        assert_eq!(
            block_on(execute_create_agent_goal(create, &root)).map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );

        let revise: ReviseAgentGoalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedRevision": 1,
            "revisionReason": "the user clarified the outcome",
            "draft": {
                "objective": "build the complete Agent workspace",
                "acceptanceCriteria": [{
                    "criterionId": "22".repeat(32),
                    "statement": "the goal remains visible",
                    "requirement": "must"
                }],
                "constraints": ["remain local-only"],
                "nonGoals": ["do not start a run"],
                "userDecisions": ["retain revisions"],
                "successVerification": "reopen and compare the goal"
            }
        }))?;
        assert_eq!(
            block_on(execute_revise_agent_goal(revise, &root)).map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );

        let unsupported: CreateAgentGoalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 999,
            "draft": {
                "objective": "",
                "acceptanceCriteria": [],
                "constraints": [],
                "nonGoals": [],
                "userDecisions": [],
                "successVerification": ""
            }
        }))?;
        assert_eq!(
            block_on(execute_create_agent_goal(unsupported, &root)).map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn agent_activity_command_accepts_only_a_valid_task_identity_after_version_check()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let query: QueryAgentActivityRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32)
        }))?;
        let response = block_on(execute_query_agent_activity(query, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentActivityResultV1::NoProject
        ));

        let invalid: QueryAgentActivityRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "not-an-id"
        }))?;
        assert_eq!(
            block_on(execute_query_agent_activity(invalid, &root)).map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidTaskLensSelection)
        );

        let unsupported: QueryAgentActivityRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 999,
            "taskId": "not-an-id"
        }))?;
        assert_eq!(
            block_on(execute_query_agent_activity(unsupported, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn agent_inspection_commands_reject_free_or_oversized_selections()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let overview =
            QueryAgentInspectionRequestV1::new(ProtocolVersion::CURRENT, "11".repeat(32));
        let response = block_on(execute_query_agent_inspection(overview, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentInspectionResultV1::NoProject
        ));

        let log = QueryAgentInspectionLogRequestV1::new(
            ProtocolVersion::CURRENT,
            "11".repeat(32),
            "1".to_owned(),
            "22".repeat(32),
            AgentInspectionStreamV1::Stdout,
            0,
            8_192,
        );
        let response = block_on(execute_query_agent_inspection_log(log, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentInspectionLogResultV1::NoProject
        ));

        for invalid in [
            QueryAgentInspectionLogRequestV1::new(
                ProtocolVersion::CURRENT,
                "11".repeat(32),
                "01".to_owned(),
                "22".repeat(32),
                AgentInspectionStreamV1::Stdout,
                0,
                8_192,
            ),
            QueryAgentInspectionLogRequestV1::new(
                ProtocolVersion::CURRENT,
                "11".repeat(32),
                "1".to_owned(),
                "22".repeat(32),
                AgentInspectionStreamV1::Stderr,
                0,
                16 * 1_024 + 1,
            ),
            QueryAgentInspectionLogRequestV1::new(
                ProtocolVersion::CURRENT,
                "11".repeat(32),
                "1".to_owned(),
                "22".repeat(32),
                AgentInspectionStreamV1::Stdout,
                0,
                3,
            ),
        ] {
            assert_eq!(
                block_on(execute_query_agent_inspection_log(invalid, &root))
                    .map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidAgentInspectionQuery)
            );
        }
        Ok(())
    }

    #[test]
    fn agent_approval_commands_are_task_bound_anchor_bound_and_version_first()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let query: QueryAgentApprovalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32)
        }))?;
        let response = block_on(execute_query_agent_approval(query, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentApprovalResultV1::NoProject
        ));

        let control: ControlAgentApprovalRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedApprovalRevision": "4",
            "expectedLedgerRevision": 3,
            "expectedLedgerStoreVersion": "8",
            "action": "deny"
        }))?;
        let response = block_on(execute_control_agent_approval(control, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentApprovalControlResultV1::NoProject
        ));

        let invalid_query: QueryAgentApprovalRequestV1 =
            serde_json::from_value(serde_json::json!({
                "protocolVersion": 1,
                "taskId": "not-an-id"
            }))?;
        assert_eq!(
            block_on(execute_query_agent_approval(invalid_query, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidAgentApprovalRequest)
        );

        let invalid_control: ControlAgentApprovalRequestV1 =
            serde_json::from_value(serde_json::json!({
                "protocolVersion": 1,
                "taskId": "11".repeat(32),
                "expectedApprovalRevision": "04",
                "expectedLedgerRevision": 0,
                "expectedLedgerStoreVersion": "08",
                "action": "continue"
            }))?;
        assert_eq!(
            block_on(execute_control_agent_approval(invalid_control, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidAgentApprovalRequest)
        );

        let unsupported: ControlAgentApprovalRequestV1 =
            serde_json::from_value(serde_json::json!({
                "protocolVersion": 999,
                "taskId": "not-an-id",
                "expectedApprovalRevision": "bad",
                "expectedLedgerRevision": 0,
                "expectedLedgerStoreVersion": "bad",
                "action": "revoke"
            }))?;
        assert_eq!(
            block_on(execute_control_agent_approval(unsupported, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn agent_task_controls_are_pathless_anchor_bound_and_version_first()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let query: QueryAgentTaskRecoveryRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32)
        }))?;
        let response = block_on(execute_query_agent_task_recovery(query, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentTaskRecoveryResultV1::NoProject
        ));

        let control: ControlAgentTaskRunRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedLedgerRevision": 2,
            "expectedLedgerStoreVersion": "7",
            "action": "cancel"
        }))?;
        let response = block_on(execute_control_agent_task_run(control, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(
            response.result(),
            AgentTaskControlResultV1::NoProject
        ));

        let invalid_anchor: ControlAgentTaskRunRequestV1 =
            serde_json::from_value(serde_json::json!({
                "protocolVersion": 1,
                "taskId": "not-an-id",
                "expectedLedgerRevision": 0,
                "expectedLedgerStoreVersion": "07",
                "action": "resume"
            }))?;
        assert_eq!(
            block_on(execute_control_agent_task_run(invalid_anchor, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidAgentTaskControl)
        );

        let unsupported: ControlAgentTaskRunRequestV1 =
            serde_json::from_value(serde_json::json!({
                "protocolVersion": 999,
                "taskId": "not-an-id",
                "expectedLedgerRevision": 0,
                "expectedLedgerStoreVersion": "bad",
                "action": "replan"
            }))?;
        assert_eq!(
            block_on(execute_control_agent_task_run(unsupported, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn deep_map_commands_are_pathless_and_require_core_owned_project_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let status = block_on(execute_query_deep_map(
            QueryDeepMapRequestV1::current(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(status.result(), DeepMapStatusResultV3::NoProject));

        let start = block_on(execute_start_deep_map(
            StartDeepMapRequestV2::new(ProtocolVersion::CURRENT, DeepMapModeV2::Standard),
            &root,
        ));
        assert_eq!(
            start.map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );

        let pause = execute_control_deep_map(
            ControlDeepMapRequestV1::current(),
            &root,
            CompositionRoot::pause_deep_map,
        );
        assert_eq!(
            pause.map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );
        Ok(())
    }

    #[test]
    fn deep_map_rejects_unsupported_protocol_before_lifecycle_access()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let result = block_on(execute_query_deep_map(
            QueryDeepMapRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));
        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn rebuild_command_rejects_unsupported_version_before_coordinator_access()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = execute_rebuild_project_index(
            RebuildProjectIndexRequestV1::new(ProtocolVersion::new(999)),
            &root,
        );

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn rebuild_command_requires_a_core_owned_active_project()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = execute_rebuild_project_index(RebuildProjectIndexRequestV1::current(), &root);

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );
        Ok(())
    }

    #[test]
    fn removal_command_rejects_unsupported_version_before_core_state_access()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_remove_project(
            RemoveProjectRequestV1::new(ProtocolVersion::new(999)),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn removal_command_requires_a_core_owned_active_project()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = block_on(execute_remove_project(
            RemoveProjectRequestV1::current(),
            &root,
        ));

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::NoActiveProject)
        );
        Ok(())
    }

    #[test]
    fn project_catalog_commands_are_pathless_bounded_and_version_first()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let initial = QueryProjectCatalogRequestV1::new(
            ProtocolVersion::CURRENT,
            None,
            None,
            ProjectCatalogDirectionV1::Initial,
        );
        let page = block_on(execute_query_project_catalog(initial, &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(serde_json::to_value(page)?.get("projects").is_some());

        let invalid_search = QueryProjectCatalogRequestV1::new(
            ProtocolVersion::CURRENT,
            Some("bad\nsearch".to_owned()),
            None,
            ProjectCatalogDirectionV1::Initial,
        );
        assert_eq!(
            block_on(execute_query_project_catalog(invalid_search, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidProjectCatalogRequest)
        );

        let invalid_cursor = QueryProjectCatalogRequestV1::new(
            ProtocolVersion::CURRENT,
            None,
            Some("0000000000000000".to_owned()),
            ProjectCatalogDirectionV1::Next,
        );
        assert_eq!(
            block_on(execute_query_project_catalog(invalid_cursor, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidProjectCatalogRequest)
        );

        let invalid_id =
            ActivateCatalogProjectRequestV1::new(ProtocolVersion::CURRENT, "not-an-id".to_owned());
        assert_eq!(
            block_on(execute_activate_catalog_project(invalid_id, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidProjectCatalogRequest)
        );

        let invalid_remove =
            RemoveCatalogProjectRequestV1::new(ProtocolVersion::CURRENT, "AA".repeat(32));
        assert_eq!(
            block_on(execute_remove_catalog_project(invalid_remove, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::InvalidProjectCatalogRequest)
        );

        let unsupported =
            ActivateCatalogProjectRequestV1::new(ProtocolVersion::new(999), "not-an-id".to_owned());
        assert_eq!(
            block_on(execute_activate_catalog_project(unsupported, &root))
                .map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );

        let restore = block_on(execute_restore_last_project(
            RestoreLastProjectRequestV1::current(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert_eq!(
            serde_json::to_value(restore)?["result"]["status"],
            "noSavedProject"
        );
        Ok(())
    }

    #[test]
    fn failed_catalog_commit_restores_the_previously_active_project()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = TestDirectory::new()?;
        let first_root = fixture.path().join("first");
        let second_root = fixture.path().join("second");
        initialize_repository(&first_root)?;
        initialize_repository(&second_root)?;
        let inspector = a3_workspace::RepositoryInspector::new();
        let first = inspector.inspect(&first_root)?;
        let second = inspector.inspect(&second_root)?;
        let record_calls = Arc::new(AtomicUsize::new(0));
        let store = Arc::new(FailingSwitchStore {
            initial_worktree_id: first.worktree().id(),
            target: StoredProjectTarget::new(
                ProjectId::from_bytes([2; 32]),
                second.repository().id(),
                second.worktree().id(),
                second_root,
            ),
            record_calls: Arc::clone(&record_calls),
        });
        let root = CompositionRoot::new(
            ApplicationVersion::try_from("0.1.0")?,
            Platform::Windows,
            Arc::new(FixedPicker(first_root)),
            Arc::new(CancelledConfirmer),
            store,
        )?;

        block_on(execute_open_project(OpenProjectRequestV1::current(), &root))
            .map_err(|error| std::io::Error::other(error.message()))?;
        let switch = block_on(execute_activate_catalog_project(
            ActivateCatalogProjectRequestV1::new(
                ProtocolVersion::CURRENT,
                second.worktree().id().to_string(),
            ),
            &root,
        ));
        assert_eq!(
            switch.map_err(|error| error.code()),
            Err(ErrorCodeV1::LocalStorageUnavailable)
        );

        let status = block_on(execute_query_project_status(
            QueryProjectStatusRequestV1::current(),
            &root,
        ))
        .map_err(|error| std::io::Error::other(error.message()))?;
        assert_eq!(
            serde_json::to_value(status)?["result"]["project"]["worktreeId"],
            first.worktree().id().to_string()
        );
        assert_eq!(record_calls.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[derive(Debug)]
    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> std::io::Result<Self> {
            static NEXT: AtomicUsize = AtomicUsize::new(1);
            let path = std::env::temp_dir().join(format!(
                "a3-project-catalog-command-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ignored = fs::remove_dir_all(&self.0);
        }
    }

    fn initialize_repository(path: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir(path)?;
        let output = Command::new("git")
            .current_dir(path)
            .args(["-c", "init.defaultBranch=main", "init"])
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("Git fixture initialization failed"))
        }
    }
}
