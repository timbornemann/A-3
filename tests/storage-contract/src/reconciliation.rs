use crate::fixture::{
    ContractWorkspace, ProjectEvidence, change, project_with_evidence, snapshot, unborn_head,
};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    KnowledgeIndexStore, KnowledgeStore, KnowledgeStoreFailure, ProjectOpenPreparation,
    ProjectReconciliationEvidence, RecentProjectLimit,
};
use a3_domain::{RemoteIdentity, RepositoryId, SnapshotChangeKind, WorktreeAnchorId, WorktreeId};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    verify_confirmed_same_repository_move(factory, workspace).await?;
    verify_confirmed_repository_move(factory, workspace).await?;
    verify_remote_match_can_be_opened_separately(factory, workspace).await
}

pub(crate) async fn verify_confirmed_same_repository_move<F>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("reconciliation-move");
    let common = workspace.create_directory("reconciliation-common")?;
    let previous_root = workspace.create_directory("reconciliation-previous")?;
    let moved_root = workspace.create_directory("reconciliation-moved")?;
    let repository_id = RepositoryId::from_bytes([70; 32]);
    let previous_worktree_id = WorktreeId::from_bytes([71; 32]);
    let moved_worktree_id = WorktreeId::from_bytes([72; 32]);
    let anchor_id = WorktreeAnchorId::from_bytes([73; 32]);
    let previous = project_with_evidence(
        repository_id,
        previous_worktree_id,
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: None,
        },
        &common,
        &previous_root,
        unborn_head()?,
    )?;
    let moved = project_with_evidence(
        repository_id,
        moved_worktree_id,
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: None,
        },
        &common,
        &moved_root,
        unborn_head()?,
    )?;

    let store = factory.open(&app_data).await?;
    let project_id = store.record_opened_project(&previous).await?;
    let previous_snapshot = snapshot(
        [74; 32],
        previous_worktree_id,
        None,
        1,
        vec![change(b"src/lib.rs", [75; 32], SnapshotChangeKind::Upsert)?],
    )?;
    store.append_snapshot(&previous, &previous_snapshot).await?;

    let proposal = match store.prepare_project_open(&moved).await? {
        ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
        other => return Err(format!("expected move confirmation, received {other:?}").into()),
    };
    assert_eq!(proposal.project_id(), project_id);
    assert_eq!(proposal.previous_worktree_id(), previous_worktree_id);
    assert_eq!(
        proposal.evidence(),
        ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor
    );
    assert_eq!(
        store.reconcile_project(&moved, &proposal).await?,
        project_id
    );

    let recent = store
        .list_recent_projects(RecentProjectLimit::DEFAULT)
        .await?;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].project_id(), project_id);
    assert_eq!(recent[0].worktree_id(), moved_worktree_id);
    let moved_snapshot = store
        .latest_snapshot(&moved)
        .await?
        .ok_or("reconciled snapshot is missing")?;
    assert_eq!(moved_snapshot.id(), previous_snapshot.id());
    assert_eq!(moved_snapshot.worktree_id(), moved_worktree_id);
    assert_eq!(moved_snapshot.changes(), previous_snapshot.changes());
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data).await?;
    assert_eq!(
        reopened.prepare_project_open(&moved).await?,
        ProjectOpenPreparation::Ready
    );
    assert_eq!(
        reopened
            .latest_snapshot(&moved)
            .await?
            .map(|snapshot| snapshot.worktree_id()),
        Some(moved_worktree_id)
    );
    crate::complete_contract_phase()
}

pub(crate) async fn verify_confirmed_repository_move<F>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("reconciliation-repository-move");
    let previous_common = workspace.create_directory("confirmed-remote-previous-common")?;
    let target_common = workspace.create_directory("confirmed-remote-target-common")?;
    let previous_root = workspace.create_directory("confirmed-remote-previous-root")?;
    let target_root = workspace.create_directory("confirmed-remote-target-root")?;
    let previous_repository_id = RepositoryId::from_bytes([90; 32]);
    let previous_worktree_id = WorktreeId::from_bytes([91; 32]);
    let target_repository_id = RepositoryId::from_bytes([92; 32]);
    let target_worktree_id = WorktreeId::from_bytes([93; 32]);
    let anchor_id = WorktreeAnchorId::from_bytes([94; 32]);
    let remote = RemoteIdentity::from_bytes([95; 32]);
    let previous = project_with_evidence(
        previous_repository_id,
        previous_worktree_id,
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: Some(remote),
        },
        &previous_common,
        &previous_root,
        unborn_head()?,
    )?;
    let target = project_with_evidence(
        target_repository_id,
        target_worktree_id,
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: Some(remote),
        },
        &target_common,
        &target_root,
        unborn_head()?,
    )?;

    let store = factory.open(&app_data).await?;
    let project_id = store.record_opened_project(&previous).await?;
    let previous_snapshot = snapshot(
        [96; 32],
        previous_worktree_id,
        None,
        1,
        vec![change(
            b"src/main.rs",
            [97; 32],
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    store.append_snapshot(&previous, &previous_snapshot).await?;

    let proposal = match store.prepare_project_open(&target).await? {
        ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
        other => {
            return Err(
                format!("expected repository move confirmation, received {other:?}").into(),
            );
        }
    };
    assert_eq!(
        proposal.evidence(),
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor
    );
    assert_eq!(
        store.reconcile_project(&target, &proposal).await?,
        project_id
    );
    let reconciled_snapshot = store
        .latest_snapshot(&target)
        .await?
        .ok_or("repository-move snapshot is missing")?;
    assert_eq!(reconciled_snapshot.id(), previous_snapshot.id());
    assert_eq!(reconciled_snapshot.worktree_id(), target_worktree_id);
    assert_eq!(
        store
            .list_recent_projects(RecentProjectLimit::DEFAULT)
            .await?
            .as_slice(),
        [a3_application::RecentProject::new(
            project_id,
            target_repository_id,
            target_worktree_id,
            a3_application::ProjectPathDisplay::from_path(target_root.as_path()),
            unborn_head()?,
        )]
    );

    crate::release_contract_store(store);
    let reopened = factory.open(&app_data).await?;
    assert_eq!(
        reopened.prepare_project_open(&target).await?,
        ProjectOpenPreparation::Ready
    );
    assert!(reopened.latest_snapshot(&target).await?.is_some());
    crate::complete_contract_phase()
}

pub(crate) async fn verify_remote_match_can_be_opened_separately<F>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("reconciliation-remote");
    let previous_common = workspace.create_directory("remote-previous-common")?;
    let target_common = workspace.create_directory("remote-target-common")?;
    let ambiguous_common = workspace.create_directory("remote-ambiguous-common")?;
    let previous_root = workspace.create_directory("remote-previous-root")?;
    let target_root = workspace.create_directory("remote-target-root")?;
    let ambiguous_root = workspace.create_directory("remote-ambiguous-root")?;
    let anchor_id = WorktreeAnchorId::from_bytes([80; 32]);
    let remote = RemoteIdentity::from_bytes([81; 32]);
    let previous = project_with_evidence(
        RepositoryId::from_bytes([82; 32]),
        WorktreeId::from_bytes([83; 32]),
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: Some(remote),
        },
        &previous_common,
        &previous_root,
        unborn_head()?,
    )?;
    let target = project_with_evidence(
        RepositoryId::from_bytes([84; 32]),
        WorktreeId::from_bytes([85; 32]),
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: Some(remote),
        },
        &target_common,
        &target_root,
        unborn_head()?,
    )?;
    let ambiguous = project_with_evidence(
        RepositoryId::from_bytes([86; 32]),
        WorktreeId::from_bytes([87; 32]),
        ProjectEvidence {
            worktree_anchor_id: anchor_id,
            main_remote: Some(remote),
        },
        &ambiguous_common,
        &ambiguous_root,
        unborn_head()?,
    )?;

    let store = factory.open(&app_data).await?;
    let previous_project_id = store.record_opened_project(&previous).await?;
    let proposal = match store.prepare_project_open(&target).await? {
        ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
        other => return Err(format!("expected remote confirmation, received {other:?}").into()),
    };
    assert_eq!(
        proposal.evidence(),
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor
    );

    let separate_project_id = store.record_opened_project(&target).await?;
    assert_ne!(separate_project_id, previous_project_id);
    assert!(matches!(
        store.reconcile_project(&target, &proposal).await,
        Err(KnowledgeStoreFailure::IdentityConflict)
    ));
    assert_eq!(
        store.prepare_project_open(&target).await?,
        ProjectOpenPreparation::Ready
    );
    let recent = store
        .list_recent_projects(RecentProjectLimit::DEFAULT)
        .await?;
    assert_eq!(recent.len(), 2);
    assert!(
        recent
            .iter()
            .any(|project| project.project_id() == previous_project_id)
    );
    assert!(
        recent
            .iter()
            .any(|project| project.project_id() == separate_project_id)
    );
    assert_eq!(
        store.prepare_project_open(&ambiguous).await?,
        ProjectOpenPreparation::Ready,
        "multiple remote-and-anchor matches must never select a candidate"
    );
    crate::complete_contract_phase()
}
