//! Native A^3 desktop executable entry point.

fn main() -> Result<(), a3_desktop::DesktopRunError> {
    a3_desktop::run()
}

#[cfg(test)]
mod tests {
    use a3_application::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectDirectoryPicker,
        ProjectDirectorySelectionError, ProjectOpenPreparation, ProjectPathDisplay,
        ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
        ProjectReconciliationConfirmer, ProjectReconciliationProposal, RecentProject,
        RecentProjectLimit,
    };
    use a3_desktop::CompositionRoot;
    use a3_domain::{ApplicationVersion, Platform, ProjectId, ProjectIdentity};
    use a3_protocol::{
        CommandErrorV1, DeepMapStatusResponseV1, DeepMapStatusResultV1, ErrorCodeV1,
        HealthResponseV1, HealthStatusV1, ModuleCardFreshnessResponseV1,
        ModuleCardFreshnessResultV1, ModuleDependencyGraphResponseV1,
        ModuleDependencyGraphResultV1, ModuleRuntimeFlowResponseV1, ModuleRuntimeFlowResultV1,
        ModuleRuntimeMapResponseV1, ModuleRuntimeMapResultV1, ModuleTreeResponseV1,
        ModuleTreeResultV1, OpenProjectResponseV1, OpenProjectResultV1, PlatformV1,
        ProtocolVersion, RecentProjectsResponseV1, RepositoryTreeResponseV1,
        RepositoryTreeResultV1,
    };
    use serde_json::json;
    use std::error::Error;
    use std::io;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;

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

    #[test]
    fn tauri_ipc_enforces_the_versioned_health_contract() -> Result<(), Box<dyn Error>> {
        let root = CompositionRoot::new(
            ApplicationVersion::try_from("1.2.3")?,
            Platform::Windows,
            Arc::new(CancelledPicker),
            Arc::new(CancelledConfirmer),
            Arc::new(EmptyStore),
        )?;
        let app = mock_builder()
            .manage(root)
            .invoke_handler(tauri::generate_handler![
                a3_desktop::commands::cancel_deep_map,
                a3_desktop::commands::list_recent_projects,
                a3_desktop::commands::open_project,
                a3_desktop::commands::pause_deep_map,
                a3_desktop::commands::query_deep_map,
                a3_desktop::commands::query_project_status,
                a3_desktop::commands::query_index_activity,
                a3_desktop::commands::query_index_overview,
                a3_desktop::commands::query_module_card_freshness,
                a3_desktop::commands::query_module_dependency_graph,
                a3_desktop::commands::query_module_runtime_flow,
                a3_desktop::commands::query_module_runtime_map,
                a3_desktop::commands::query_module_tree,
                a3_desktop::commands::query_repository_tree,
                a3_desktop::commands::query_health,
                a3_desktop::commands::rebuild_project_index,
                a3_desktop::commands::remove_project,
                a3_desktop::commands::resume_deep_map,
                a3_desktop::commands::start_deep_map
            ])
            .build(tauri::generate_context!())?;
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
        let local_app_url = webview.url()?;
        let response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<HealthResponseV1>()?;

        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        assert_eq!(response.application_version(), "1.2.3");
        assert_eq!(response.platform(), PlatformV1::Windows);
        assert_eq!(response.status(), HealthStatusV1::Ready);

        let freshness_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_module_card_freshness".into(),
                callback: CallbackFn(16),
                error: CallbackFn(17),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<ModuleCardFreshnessResponseV1>()?;
        assert!(matches!(
            freshness_response.result(),
            ModuleCardFreshnessResultV1::NoProject
        ));

        let module_tree_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_module_tree".into(),
                callback: CallbackFn(20),
                error: CallbackFn(21),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "parentModuleId": null,
                        "afterModuleId": null,
                        "limit": 50
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<ModuleTreeResponseV1>()?;
        assert!(matches!(
            module_tree_response.result(),
            ModuleTreeResultV1::NoProject
        ));

        let module_dependency_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_module_dependency_graph".into(),
                callback: CallbackFn(22),
                error: CallbackFn(23),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "centerModuleId": "11".repeat(32),
                        "nodeLimit": 50
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<ModuleDependencyGraphResponseV1>()?;
        assert!(matches!(
            module_dependency_response.result(),
            ModuleDependencyGraphResultV1::NoProject
        ));

        let module_runtime_map_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_module_runtime_map".into(),
                callback: CallbackFn(24),
                error: CallbackFn(25),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "moduleId": "11".repeat(32),
                        "entrypointLimit": 20,
                        "testLimit": 20
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<ModuleRuntimeMapResponseV1>()?;
        assert!(matches!(
            module_runtime_map_response.result(),
            ModuleRuntimeMapResultV1::NoProject
        ));

        let module_runtime_flow_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_module_runtime_flow".into(),
                callback: CallbackFn(26),
                error: CallbackFn(27),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "expectedIndexRunId": "22".repeat(32),
                        "expectedSnapshotId": "33".repeat(32),
                        "moduleId": "11".repeat(32),
                        "rootSymbolId": "44".repeat(32),
                        "kind": "entrypointCalls",
                        "resultLimit": 20
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<ModuleRuntimeFlowResponseV1>()?;
        assert!(matches!(
            module_runtime_flow_response.result(),
            ModuleRuntimeFlowResultV1::NoProject
        ));

        let repository_tree_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_repository_tree".into(),
                callback: CallbackFn(18),
                error: CallbackFn(19),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "directoryPathHex": null,
                        "afterNameHex": null,
                        "limit": 50
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<RepositoryTreeResponseV1>()?;
        assert!(matches!(
            repository_tree_response.result(),
            RepositoryTreeResultV1::NoProject
        ));

        let project_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "open_project".into(),
                callback: CallbackFn(6),
                error: CallbackFn(7),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<OpenProjectResponseV1>()?;
        assert_eq!(project_response.protocol_version(), ProtocolVersion::V1);
        assert!(matches!(
            project_response.result(),
            OpenProjectResultV1::Cancelled
        ));

        let recent_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "list_recent_projects".into(),
                callback: CallbackFn(10),
                error: CallbackFn(11),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<RecentProjectsResponseV1>()?;
        assert!(recent_response.projects().is_empty());

        let deep_map_response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_deep_map".into(),
                callback: CallbackFn(12),
                error: CallbackFn(13),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<DeepMapStatusResponseV1>()?;
        assert!(matches!(
            deep_map_response.result(),
            DeepMapStatusResultV1::NoProject
        ));

        let untrusted_deep_map_scope = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "start_deep_map".into(),
                callback: CallbackFn(14),
                error: CallbackFn(15),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "budget": {
                            "tokenLimit": 32_000,
                            "timeLimitMillis": 120_000,
                            "toolCallLimit": 64
                        },
                        "profileId": "11".repeat(32)
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );
        assert!(untrusted_deep_map_scope.is_err());

        let untrusted_project_path = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "open_project".into(),
                callback: CallbackFn(8),
                error: CallbackFn(9),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "selectedPath": "C:\\untrusted"
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );
        assert!(untrusted_project_path.is_err());

        let invalid_payload = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(2),
                error: CallbackFn(3),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "unexpected": true
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );

        assert!(invalid_payload.is_err());

        let unsupported_version = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(4),
                error: CallbackFn(5),
                url: local_app_url,
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 999 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );

        let error = match unsupported_version {
            Ok(_) => {
                return Err(io::Error::other("unsupported protocol version was accepted").into());
            }
            Err(error) => serde_json::from_value::<CommandErrorV1>(error)?,
        };
        assert_eq!(error.protocol_version(), ProtocolVersion::V1);
        assert_eq!(error.code(), ErrorCodeV1::UnsupportedProtocolVersion);

        Ok(())
    }

    #[test]
    fn main_capability_exposes_no_direct_dialog_or_filesystem_permission()
    -> Result<(), Box<dyn Error>> {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main.json"))?;

        assert_eq!(
            capability.get("permissions"),
            Some(&json!([
                "allow-list-recent-projects",
                "allow-cancel-deep-map",
                "allow-open-project",
                "allow-pause-deep-map",
                "allow-query-deep-map",
                "allow-query-index-activity",
                "allow-query-index-overview",
                "allow-query-module-card-freshness",
                "allow-query-module-card-detail",
                "allow-query-module-card-evidence",
                "allow-query-module-dependency-graph",
                "allow-query-module-runtime-flow",
                "allow-query-module-runtime-map",
                "allow-query-module-tree",
                "allow-query-repository-tree",
                "allow-query-project-status",
                "allow-query-health",
                "allow-rebuild-project-index",
                "allow-remove-project",
                "allow-resume-deep-map",
                "allow-start-deep-map"
            ]))
        );
        Ok(())
    }
}
