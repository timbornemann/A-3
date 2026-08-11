use crate::{
    CompositionRoot, map_module_card_detail_query_from_v1,
    map_module_dependency_graph_query_from_v1, map_module_runtime_flow_query_from_v1,
    map_module_runtime_map_query_from_v1, map_module_tree_query_from_v1,
    map_repository_tree_query_from_v1,
};
use a3_protocol::{
    CommandErrorV1, ControlDeepMapRequestV1, DeepMapControlResponseV1, DeepMapStatusResponseV1,
    HealthRequestV1, HealthResponseV1, IndexActivityResponseV1, IndexOverviewResponseV1,
    ListRecentProjectsRequestV1, ModuleCardDetailResponseV1, ModuleCardFreshnessResponseV1,
    ModuleDependencyGraphResponseV1, ModuleRuntimeFlowResponseV1, ModuleRuntimeMapResponseV1,
    ModuleTreeResponseV1, OpenProjectRequestV1, OpenProjectResponseV1, ProjectStatusResponseV1,
    ProtocolVersion, QueryDeepMapRequestV1, QueryIndexActivityRequestV1,
    QueryIndexOverviewRequestV1, QueryModuleCardDetailRequestV1, QueryModuleCardFreshnessRequestV1,
    QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
    QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectStatusRequestV1,
    QueryRepositoryTreeRequestV1, RebuildProjectIndexRequestV1, RebuildProjectIndexResponseV1,
    RecentProjectsResponseV1, RemoveProjectRequestV1, RemoveProjectResponseV1,
    RepositoryTreeResponseV1, StartDeepMapRequestV1,
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
        execute_control_deep_map, execute_list_recent_projects, execute_open_project,
        execute_query_deep_map, execute_query_health, execute_query_index_activity,
        execute_query_index_overview, execute_query_module_card_detail,
        execute_query_module_card_freshness, execute_query_module_dependency_graph,
        execute_query_module_runtime_flow, execute_query_module_runtime_map,
        execute_query_module_tree, execute_query_project_status, execute_query_repository_tree,
        execute_rebuild_project_index, execute_remove_project, execute_start_deep_map,
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
        ControlDeepMapRequestV1, DeepMapBudgetV1, DeepMapStatusResultV1, ErrorCodeV1,
        HealthRequestV1, IndexActivityResultV1, IndexOverviewResultV1, ListRecentProjectsRequestV1,
        ModuleCardDetailResultV1, ModuleCardFreshnessResultV1, ModuleDependencyGraphResultV1,
        ModuleRuntimeFlowKindV1, ModuleRuntimeFlowResultV1, ModuleRuntimeMapResultV1,
        ModuleTreeResultV1, OpenProjectRequestV1, ProjectStatusResultV1, ProtocolVersion,
        QueryDeepMapRequestV1, QueryIndexActivityRequestV1, QueryIndexOverviewRequestV1,
        QueryModuleCardDetailRequestV1, QueryModuleCardFreshnessRequestV1,
        QueryModuleDependencyGraphRequestV1, QueryModuleRuntimeFlowRequestV1,
        QueryModuleRuntimeMapRequestV1, QueryModuleTreeRequestV1, QueryProjectStatusRequestV1,
        QueryRepositoryTreeRequestV1, RebuildProjectIndexRequestV1, RemoveProjectRequestV1,
        RepositoryTreeResultV1, StartDeepMapRequestV1,
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
