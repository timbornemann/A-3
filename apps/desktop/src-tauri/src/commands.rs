use crate::CompositionRoot;
use a3_protocol::{
    CommandErrorV1, HealthRequestV1, HealthResponseV1, IndexActivityResponseV1,
    IndexOverviewResponseV1, ListRecentProjectsRequestV1, OpenProjectRequestV1,
    OpenProjectResponseV1, ProjectStatusResponseV1, ProtocolVersion, QueryIndexActivityRequestV1,
    QueryIndexOverviewRequestV1, QueryProjectStatusRequestV1, RebuildProjectIndexRequestV1,
    RebuildProjectIndexResponseV1, RecentProjectsResponseV1, RemoveProjectRequestV1,
    RemoveProjectResponseV1,
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
        execute_list_recent_projects, execute_open_project, execute_query_health,
        execute_query_index_activity, execute_query_index_overview, execute_query_project_status,
        execute_rebuild_project_index, execute_remove_project,
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
        ErrorCodeV1, HealthRequestV1, IndexActivityResultV1, IndexOverviewResultV1,
        ListRecentProjectsRequestV1, OpenProjectRequestV1, ProjectStatusResultV1, ProtocolVersion,
        QueryIndexActivityRequestV1, QueryIndexOverviewRequestV1, QueryProjectStatusRequestV1,
        RebuildProjectIndexRequestV1, RemoveProjectRequestV1,
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
