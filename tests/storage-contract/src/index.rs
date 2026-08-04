use crate::fixture::{ContractWorkspace, change, project, run, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{KnowledgeIndexFailure, KnowledgeIndexStore, KnowledgeStoreFailure};
use a3_domain::{
    IndexRunId, IndexRunStatus, IndexRunTerminalOutcome, RepositoryId, SnapshotChangeKind,
    SnapshotId, WorktreeId,
};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("index");
    let common = workspace.create_directory("index-common")?;
    let primary_root = workspace.create_directory("index-primary")?;
    let linked_root = workspace.create_directory("index-linked")?;
    let repository_id = RepositoryId::from_bytes([4; 32]);
    let primary_id = WorktreeId::from_bytes([41; 32]);
    let linked_id = WorktreeId::from_bytes([42; 32]);
    let primary = project(
        repository_id,
        primary_id,
        &common,
        &primary_root,
        unborn_head()?,
    )?;
    let linked = project(
        repository_id,
        linked_id,
        &common,
        &linked_root,
        unborn_head()?,
    )?;
    let store = factory.open(&app_data).await?;

    assert_eq!(store.latest_snapshot(&primary).await?, None);
    assert_eq!(store.latest_snapshot(&linked).await?, None);
    let first = snapshot(
        [51; 32],
        primary_id,
        None,
        1,
        vec![
            change(b"src/z.rs", [2; 32], SnapshotChangeKind::Delete)?,
            change(b"src/a.rs", [1; 32], SnapshotChangeKind::Upsert)?,
        ],
    )?;
    store.append_snapshot(&primary, &first).await?;
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));
    assert_eq!(store.latest_snapshot(&linked).await?, None);

    let foreign = snapshot(
        [52; 32],
        WorktreeId::from_bytes([99; 32]),
        None,
        1,
        Vec::new(),
    )?;
    assert_eq!(
        store.append_snapshot(&primary, &foreign).await,
        Err(KnowledgeIndexFailure::Storage(
            KnowledgeStoreFailure::IdentityConflict
        ))
    );
    let stale = snapshot(
        [53; 32],
        primary_id,
        Some(SnapshotId::from_bytes([100; 32])),
        2,
        Vec::new(),
    )?;
    assert_eq!(
        store.append_snapshot(&primary, &stale).await,
        Err(KnowledgeIndexFailure::SnapshotConflict)
    );
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));

    let linked_snapshot = snapshot([54; 32], linked_id, None, 1, Vec::new())?;
    store.append_snapshot(&linked, &linked_snapshot).await?;
    assert_eq!(
        store.latest_snapshot(&linked).await?,
        Some(linked_snapshot.clone())
    );
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));

    let second = snapshot([55; 32], primary_id, Some(first.id()), 2, Vec::new())?;
    store.append_snapshot(&primary, &second).await?;
    drop(store);

    let reopened = factory.open(&app_data).await?;
    assert_eq!(
        reopened.latest_snapshot(&primary).await?,
        Some(second.clone())
    );
    assert_eq!(
        reopened.latest_snapshot(&linked).await?,
        Some(linked_snapshot)
    );
    assert_eq!(
        reopened
            .start_index_run(
                &primary,
                run([61; 32], SnapshotId::from_bytes([101; 32]), 1)?,
            )
            .await,
        Err(KnowledgeIndexFailure::SnapshotNotFound)
    );

    let first_run = reopened
        .start_index_run(&primary, run([62; 32], first.id(), 1)?)
        .await?;
    assert_eq!(first_run.sequence().get(), 1);
    assert_eq!(first_run.status(), IndexRunStatus::Building);
    assert_eq!(reopened.latest_index_run(&primary).await?, Some(first_run));
    assert_eq!(
        reopened
            .start_index_run(&primary, run([63; 32], second.id(), 1)?)
            .await,
        Err(KnowledgeIndexFailure::IndexRunAlreadyActive)
    );
    assert_eq!(
        reopened
            .finish_index_run(
                &primary,
                IndexRunId::from_bytes([110; 32]),
                IndexRunTerminalOutcome::Failed,
            )
            .await,
        Err(KnowledgeIndexFailure::IndexRunNotFound)
    );
    assert_eq!(reopened.latest_published_index_run(&primary).await?, None);

    let failed = reopened
        .finish_index_run(&primary, first_run.id(), IndexRunTerminalOutcome::Failed)
        .await?;
    assert_eq!(failed.status(), IndexRunStatus::Failed);
    assert_eq!(
        reopened
            .finish_index_run(&primary, first_run.id(), IndexRunTerminalOutcome::Cancelled,)
            .await,
        Err(KnowledgeIndexFailure::InvalidIndexRunTransition)
    );

    let second_run = reopened
        .start_index_run(&primary, run([63; 32], second.id(), 2)?)
        .await?;
    assert_eq!(second_run.sequence().get(), 2);
    let cancelled = reopened
        .finish_index_run(
            &primary,
            second_run.id(),
            IndexRunTerminalOutcome::Cancelled,
        )
        .await?;
    assert_eq!(cancelled.status(), IndexRunStatus::Cancelled);
    drop(reopened);

    let reopened_again = factory.open(&app_data).await?;
    assert_eq!(
        reopened_again.latest_index_run(&primary).await?,
        Some(cancelled)
    );
    assert_eq!(
        reopened_again.latest_published_index_run(&primary).await?,
        None
    );
    Ok(())
}
