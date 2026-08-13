use crate::{
    CompositionRoot, map_agent_activity_task_id_from_v1, map_agent_goal_task_id_from_v1,
    map_agent_task_control_from_v1, map_agent_task_recovery_task_id_from_v1,
    map_create_agent_goal_from_v1, map_module_card_detail_query_from_v1,
    map_module_card_evidence_query_from_v1, map_module_dependency_graph_query_from_v1,
    map_module_runtime_flow_query_from_v1, map_module_runtime_map_query_from_v1,
    map_module_tree_query_from_v1, map_project_map_search_query_from_v1,
    map_repository_tree_query_from_v1, map_revise_agent_goal_from_v1,
    map_task_lens_selection_from_v1, map_task_lens_task_id_from_v1,
};
use a3_protocol::{
    AgentActivityResponseV1, AgentGoalMutationResponseV1, AgentGoalResponseV1,
    AgentTaskControlResponseV1, AgentTaskRecoveryResponseV1, CommandErrorV1,
    CompileTaskLensRequestV1, ControlAgentTaskRunRequestV1, ControlDeepMapRequestV1,
    CreateAgentGoalRequestV1, DeepMapControlResponseV1, DeepMapStatusResponseV1, HealthRequestV1,
    HealthResponseV1, IndexActivityResponseV1, IndexOverviewResponseV1,
    ListRecentProjectsRequestV1, ModuleCardDetailResponseV1, ModuleCardEvidenceResponseV1,
    ModuleCardFreshnessResponseV1, ModuleDependencyGraphResponseV1, ModuleRuntimeFlowResponseV1,
    ModuleRuntimeMapResponseV1, ModuleTreeResponseV1, OpenProjectRequestV1, OpenProjectResponseV1,
    ProjectMapSearchResponseV1, ProjectStatusResponseV1, ProtocolVersion,
    QueryAgentActivityRequestV1, QueryAgentGoalRequestV1, QueryAgentTaskRecoveryRequestV1,
    QueryDeepMapRequestV1, QueryIndexActivityRequestV1, QueryIndexOverviewRequestV1,
    QueryModuleCardDetailRequestV1, QueryModuleCardEvidenceRequestV1,
    QueryModuleCardFreshnessRequestV1, QueryModuleDependencyGraphRequestV1,
    QueryModuleRuntimeFlowRequestV1, QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1,
    QueryProjectMapSearchRequestV1, QueryProjectStatusRequestV1, QueryRepositoryTreeRequestV1,
    QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1, RebuildProjectIndexRequestV1,
    RebuildProjectIndexResponseV1, RecentProjectsResponseV1, RemoveProjectRequestV1,
    RemoveProjectResponseV1, RepositoryTreeResponseV1, ReviseAgentGoalRequestV1,
    StartDeepMapRequestV1, TaskLensCompileResponseV1, TaskLensTaskResponseV1,
    TaskLensTasksResponseV1,
};
use tauri::State;

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
pub fn query_deep_map(
    request: QueryDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapStatusResponseV1, CommandErrorV1> {
    execute_query_deep_map(request, root.inner())
}

#[tauri::command]
/// Explicitly starts Deep Map with the supplied bounded budget and no WebView-selected identity.
pub fn start_deep_map(
    request: StartDeepMapRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    execute_start_deep_map(request, root.inner())
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
/// Returns process health metadata when the request uses the current protocol version.
pub fn query_health(
    request: HealthRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<HealthResponseV1, CommandErrorV1> {
    execute_query_health(request, root.inner())
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
    let query = map_module_card_detail_query_from_v1(&request)?;
    root.query_module_card_detail(&query).await
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

fn execute_query_deep_map(
    request: QueryDeepMapRequestV1,
    root: &CompositionRoot,
) -> Result<DeepMapStatusResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    Ok(root.query_deep_map_status())
}

fn execute_start_deep_map(
    request: StartDeepMapRequestV1,
    root: &CompositionRoot,
) -> Result<DeepMapControlResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    root.start_deep_map(request.budget())
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

#[cfg(test)]
mod tests {
    use super::{
        execute_compile_task_lens, execute_control_agent_task_run, execute_control_deep_map,
        execute_create_agent_goal, execute_list_recent_projects, execute_open_project,
        execute_query_agent_activity, execute_query_agent_goal, execute_query_agent_task_recovery,
        execute_query_deep_map, execute_query_health, execute_query_index_activity,
        execute_query_index_overview, execute_query_module_card_detail,
        execute_query_module_card_evidence, execute_query_module_card_freshness,
        execute_query_module_dependency_graph, execute_query_module_runtime_flow,
        execute_query_module_runtime_map, execute_query_module_tree,
        execute_query_project_map_search, execute_query_project_status,
        execute_query_repository_tree, execute_query_task_lens_task, execute_query_task_lens_tasks,
        execute_rebuild_project_index, execute_remove_project, execute_revise_agent_goal,
        execute_start_deep_map,
    };
    use crate::CompositionRoot;
    use a3_application::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectDirectoryPicker,
        ProjectDirectorySelectionError, ProjectOpenPreparation, ProjectPathDisplay,
        ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
        ProjectReconciliationConfirmer, ProjectReconciliationProposal, RecentProject,
        RecentProjectLimit,
    };
    use a3_domain::{ApplicationVersion, Platform, ProjectId, ProjectIdentity};
    use a3_protocol::{
        AgentActivityResultV1, AgentGoalResultV1, AgentTaskControlResultV1,
        AgentTaskRecoveryResultV1, CompileTaskLensRequestV1, ControlAgentTaskRunRequestV1,
        ControlDeepMapRequestV1, CreateAgentGoalRequestV1, DeepMapBudgetV1, DeepMapStatusResultV1,
        ErrorCodeV1, HealthRequestV1, IndexActivityResultV1, IndexOverviewResultV1,
        ListRecentProjectsRequestV1, ModuleCardDetailResultV1, ModuleCardEvidenceResultV1,
        ModuleCardFreshnessResultV1, ModuleDependencyGraphResultV1, ModuleRuntimeFlowKindV1,
        ModuleRuntimeFlowResultV1, ModuleRuntimeMapResultV1, ModuleTreeResultV1,
        OpenProjectRequestV1, ProjectMapSearchResultV1, ProjectStatusResultV1, ProtocolVersion,
        QueryAgentActivityRequestV1, QueryAgentGoalRequestV1, QueryAgentTaskRecoveryRequestV1,
        QueryDeepMapRequestV1, QueryIndexActivityRequestV1, QueryIndexOverviewRequestV1,
        QueryModuleCardDetailRequestV1, QueryModuleCardEvidenceRequestV1,
        QueryModuleCardFreshnessRequestV1, QueryModuleDependencyGraphRequestV1,
        QueryModuleRuntimeFlowRequestV1, QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1,
        QueryProjectMapSearchRequestV1, QueryProjectStatusRequestV1, QueryRepositoryTreeRequestV1,
        QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1, RebuildProjectIndexRequestV1,
        RemoveProjectRequestV1, RepositoryTreeResultV1, ReviseAgentGoalRequestV1,
        StartDeepMapRequestV1, TaskLensCompileResultV1, TaskLensTaskResultV1,
        TaskLensTasksResultV1,
    };
    use futures::executor::block_on;
    use std::path::PathBuf;
    use std::sync::Arc;

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
        let status = execute_query_deep_map(QueryDeepMapRequestV1::current(), &root)
            .map_err(|error| std::io::Error::other(error.message()))?;
        assert!(matches!(status.result(), DeepMapStatusResultV1::NoProject));

        let start = execute_start_deep_map(
            StartDeepMapRequestV1::new(
                ProtocolVersion::CURRENT,
                DeepMapBudgetV1::new(32_000, 120_000, 64),
            ),
            &root,
        );
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
        let result =
            execute_query_deep_map(QueryDeepMapRequestV1::new(ProtocolVersion::new(999)), &root);
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
}
