use crate::fixture::{ContractWorkspace, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CommandAllowlistStoreFailure, CommandAllowlistStoreVersion, ConfirmProjectCommandAllowlist,
    ConfirmProjectCommandAllowlistError, LoadProjectCommandAllowlist,
};
use a3_domain::{
    AgentRunTimestamp, CommandDiscoveryEvidence, CommandDiscoverySchemaVersion, ContentHash,
    DiscoveredCommand, DiscoveredCommandKind, FileRevision, ProjectCommandCatalog, RepositoryId,
    RepositoryPath, WorkspaceDirectory, WorktreeId,
};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("command-allowlist");
    let common = workspace.create_directory("command-allowlist-common")?;
    let root = workspace.create_directory("command-allowlist-root")?;
    let worktree_id = WorktreeId::from_bytes([221; 32]);
    let primary_project = project(
        RepositoryId::from_bytes([220; 32]),
        worktree_id,
        &common,
        &root,
        unborn_head()?,
    )?;
    let first_catalog = catalog(worktree_id, [222; 32])?;
    let first_command = first_catalog
        .commands()
        .first()
        .ok_or_else(|| std::io::Error::other("fixture command catalog is empty"))?
        .id();

    let store = factory.open(&app_data_root).await?;
    assert_eq!(
        LoadProjectCommandAllowlist::new(&store)
            .execute(&primary_project)
            .await?,
        None
    );
    let first = ConfirmProjectCommandAllowlist::new(&store)
        .execute(
            &primary_project,
            &first_catalog,
            vec![first_command],
            AgentRunTimestamp::from_unix_millis(7_000)?,
            None,
        )
        .await?;
    assert_eq!(first.version(), CommandAllowlistStoreVersion::new(1)?);

    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        LoadProjectCommandAllowlist::new(&reopened)
            .execute(&primary_project)
            .await?,
        Some(first.clone())
    );
    assert!(matches!(
        ConfirmProjectCommandAllowlist::new(&reopened)
            .execute(
                &primary_project,
                &first_catalog,
                vec![first_command],
                AgentRunTimestamp::from_unix_millis(7_001)?,
                None,
            )
            .await,
        Err(ConfirmProjectCommandAllowlistError::Store(
            CommandAllowlistStoreFailure::VersionConflict
        ))
    ));
    assert_eq!(
        LoadProjectCommandAllowlist::new(&reopened)
            .execute(&primary_project)
            .await?,
        Some(first.clone())
    );

    let changed_catalog = catalog(worktree_id, [223; 32])?;
    let changed_command = changed_catalog
        .commands()
        .first()
        .ok_or_else(|| std::io::Error::other("changed command catalog is empty"))?
        .id();
    let second = ConfirmProjectCommandAllowlist::new(&reopened)
        .execute(
            &primary_project,
            &changed_catalog,
            vec![changed_command],
            AgentRunTimestamp::from_unix_millis(7_002)?,
            Some(first.version()),
        )
        .await?;
    assert_eq!(second.version(), CommandAllowlistStoreVersion::new(2)?);
    assert_eq!(
        LoadProjectCommandAllowlist::new(&reopened)
            .execute(&primary_project)
            .await?,
        Some(second.clone())
    );

    let other_root = workspace.create_directory("command-allowlist-other-root")?;
    let other = project(
        RepositoryId::from_bytes([224; 32]),
        WorktreeId::from_bytes([225; 32]),
        &common,
        &other_root,
        unborn_head()?,
    )?;
    assert_eq!(
        LoadProjectCommandAllowlist::new(&reopened)
            .execute(&other)
            .await?,
        None
    );

    crate::release_contract_store(reopened);
    crate::release_contract_store(store);
    crate::complete_contract_phase()
}

fn catalog(worktree_id: WorktreeId, hash: [u8; 32]) -> ContractResult<ProjectCommandCatalog> {
    let command = DiscoveredCommand::try_new(
        DiscoveredCommandKind::Test,
        WorkspaceDirectory::Root,
        "cargo".to_owned(),
        vec![
            "test".to_owned(),
            "--offline".to_owned(),
            "--locked".to_owned(),
        ],
        vec![CommandDiscoveryEvidence::File(FileRevision::new(
            RepositoryPath::try_from_bytes(b"Cargo.toml".to_vec())?,
            ContentHash::from_bytes(hash),
        ))],
    )?;
    Ok(ProjectCommandCatalog::new(
        CommandDiscoverySchemaVersion::V1,
        worktree_id,
        vec![command],
    )?)
}
