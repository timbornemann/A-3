//! Contract tests for the desktop health-query boundary.

use a3_application::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectDirectoryPicker,
    ProjectDirectorySelectionError, ProjectOpenPreparation, ProjectPathDisplay,
    ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
    ProjectReconciliationConfirmer, ProjectReconciliationProposal, RecentProject,
    RecentProjectLimit,
};
use a3_desktop::CompositionRoot;
use a3_domain::{ApplicationVersion, Platform, ProjectId, ProjectIdentity};
use a3_protocol::{HealthStatusV1, OpenProjectResultV1, PlatformV1, ProtocolVersion};
use futures::executor::block_on;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
struct FixedPicker(Option<PathBuf>);

impl ProjectDirectoryPicker for FixedPicker {
    fn pick_project_directory(&self) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
        Ok(self.0.clone())
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
fn composition_root_maps_domain_health_to_protocol_v1() -> Result<(), Box<dyn Error>> {
    let root = CompositionRoot::new(
        ApplicationVersion::try_from("1.2.3")?,
        Platform::Windows,
        Arc::new(FixedPicker(None)),
        Arc::new(CancelledConfirmer),
        Arc::new(EmptyStore),
    )?;

    let response = root.query_health();

    assert_eq!(response.protocol_version(), ProtocolVersion::V1);
    assert_eq!(response.application_version(), "1.2.3");
    assert_eq!(response.platform(), PlatformV1::Windows);
    assert_eq!(response.status(), HealthStatusV1::Ready);
    Ok(())
}

#[test]
fn environment_builds_a_valid_composition_root() -> Result<(), Box<dyn Error>> {
    let root = CompositionRoot::from_environment(
        Arc::new(FixedPicker(None)),
        Arc::new(CancelledConfirmer),
        Arc::new(EmptyStore),
    )?;

    assert_eq!(root.query_health().application_version(), "0.1.0");
    Ok(())
}

#[test]
fn composition_root_opens_the_explicit_checkout_root() -> Result<(), Box<dyn Error>> {
    let checkout_root =
        std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))?;
    let root = CompositionRoot::new(
        ApplicationVersion::try_from("1.2.3")?,
        Platform::Windows,
        Arc::new(FixedPicker(Some(checkout_root.clone()))),
        Arc::new(CancelledConfirmer),
        Arc::new(EmptyStore),
    )?;

    let response = block_on(root.open_project())
        .map_err(|error| io::Error::other(error.message().to_owned()))?;

    let OpenProjectResultV1::Opened { project } = response.result() else {
        return Err("explicit checkout selection was cancelled".into());
    };
    assert_eq!(project.repository_id().len(), 64);
    assert_eq!(project.worktree_id().len(), 64);
    assert_eq!(
        project.worktree_root_display(),
        checkout_root.to_string_lossy()
    );
    Ok(())
}
