use crate::CompositionRoot;
use a3_protocol::{
    CommandErrorV1, HealthRequestV1, HealthResponseV1, ListRecentProjectsRequestV1,
    OpenProjectRequestV1, OpenProjectResponseV1, ProtocolVersion, RecentProjectsResponseV1,
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

#[cfg(test)]
mod tests {
    use super::{execute_list_recent_projects, execute_open_project, execute_query_health};
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
        ErrorCodeV1, HealthRequestV1, ListRecentProjectsRequestV1, OpenProjectRequestV1,
        ProtocolVersion,
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
}
