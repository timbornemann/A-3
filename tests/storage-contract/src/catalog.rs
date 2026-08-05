use crate::fixture::{ContractWorkspace, born_head, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{KnowledgeStore, RecentProjectLimit};
use a3_domain::{RepositoryId, WorktreeId};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    verify_recency_and_reopen(factory, workspace).await?;
    verify_linked_worktrees(factory, workspace).await
}

pub(crate) async fn verify_recency_and_reopen<F>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("catalog-recency");
    let common_a = workspace.create_directory("catalog-common-a")?;
    let common_b = workspace.create_directory("catalog-common-b")?;
    let root_a = workspace.create_directory("catalog-worktree-a")?;
    let root_b = workspace.create_directory("catalog-worktree-b")?;
    let repository_a = RepositoryId::from_bytes([1; 32]);
    let repository_b = RepositoryId::from_bytes([2; 32]);
    let worktree_a = WorktreeId::from_bytes([11; 32]);
    let worktree_b = WorktreeId::from_bytes([22; 32]);
    let first = project(repository_a, worktree_a, &common_a, &root_a, unborn_head()?)?;
    let second = project(
        repository_b,
        worktree_b,
        &common_b,
        &root_b,
        born_head("1111111111111111111111111111111111111111")?,
    )?;

    let store = factory.open(&app_data).await?;
    assert_eq!(
        store
            .list_recent_projects(RecentProjectLimit::DEFAULT)
            .await?,
        Vec::new(),
        "a fresh store must have no recent projects"
    );
    let first_project_id = store.record_opened_project(&first).await?;
    let second_project_id = store.record_opened_project(&second).await?;
    let one = store
        .list_recent_projects(RecentProjectLimit::new(1)?)
        .await?;
    assert_eq!(one.len(), 1, "the requested recent-project limit is exact");
    assert_eq!(one[0].project_id(), second_project_id);

    let updated_head = born_head("2222222222222222222222222222222222222222")?;
    let updated_first = project(
        repository_a,
        worktree_a,
        &common_a,
        &root_a,
        updated_head.clone(),
    )?;
    assert_eq!(
        store.record_opened_project(&updated_first).await?,
        first_project_id,
        "re-observing one repository must retain its catalog identity"
    );
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data).await?;
    let recent = reopened
        .list_recent_projects(RecentProjectLimit::DEFAULT)
        .await?;
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].project_id(), first_project_id);
    assert_eq!(recent[0].repository_id(), repository_a);
    assert_eq!(recent[0].worktree_id(), worktree_a);
    assert_eq!(recent[0].head(), &updated_head);
    assert_eq!(recent[1].project_id(), second_project_id);
    crate::complete_contract_phase()
}

pub(crate) async fn verify_linked_worktrees<F>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("catalog-linked");
    let common = workspace.create_directory("linked-common")?;
    let primary_root = workspace.create_directory("linked-primary")?;
    let linked_root = workspace.create_directory("linked-secondary")?;
    let repository_id = RepositoryId::from_bytes([3; 32]);
    let primary_id = WorktreeId::from_bytes([31; 32]);
    let linked_id = WorktreeId::from_bytes([32; 32]);
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
    let project_id = store.record_opened_project(&primary).await?;
    assert_eq!(store.record_opened_project(&linked).await?, project_id);
    let recent = store
        .list_recent_projects(RecentProjectLimit::DEFAULT)
        .await?;
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].project_id(), project_id);
    assert_eq!(recent[0].worktree_id(), linked_id);
    assert_eq!(recent[1].project_id(), project_id);
    assert_eq!(recent[1].worktree_id(), primary_id);
    crate::complete_contract_phase()
}
