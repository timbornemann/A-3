//! Contract tests for durable project identity and recent-worktree ordering.

mod support;

use a3_application::{KnowledgeStore, KnowledgeStoreFailure, RecentProjectLimit};
use a3_domain::{
    CanonicalDirectory, GitHead, GitObjectId, GitReferenceName, ProjectIdentity, RemoteIdentity,
    RepositoryId, RepositoryIdentity, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{CatalogDatabase, StorageLayout};
use futures::executor::block_on;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use support::TempDirectory;

// libSQL's Windows native test runtime is not safe when separate local database
// fixtures are opened and torn down concurrently inside one process.
static PROJECT_CATALOG_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn project_records_survive_reopen_and_follow_open_order() -> Result<(), Box<dyn std::error::Error>>
{
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common_a = create_directory(temporary.path().join("repository-a-git"))?;
        let common_b = create_directory(temporary.path().join("repository-b-git"))?;
        let root_a = create_directory(temporary.path().join("worktree-a"))?;
        let root_b = create_directory(temporary.path().join("worktree-b"))?;
        let first = project_fixture([1; 32], [11; 32], &common_a, &root_a, None, unborn_head()?)?;
        let second = project_fixture(
            [2; 32],
            [22; 32],
            &common_b,
            &root_b,
            Some([9; 32]),
            born_head("1111111111111111111111111111111111111111")?,
        )?;

        let catalog = CatalogDatabase::open(&layout).await?;
        let first_project_id = catalog.record_opened_project(&first).await?;
        let second_project_id = catalog.record_opened_project(&second).await?;
        let recent = catalog
            .list_recent_projects(RecentProjectLimit::DEFAULT)
            .await?;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].project_id(), second_project_id);
        assert_eq!(recent[1].project_id(), first_project_id);

        let updated_first = project_fixture(
            [1; 32],
            [11; 32],
            &common_a,
            &root_a,
            None,
            born_head("2222222222222222222222222222222222222222")?,
        )?;
        assert_eq!(
            catalog.record_opened_project(&updated_first).await?,
            first_project_id
        );
        drop(catalog);

        let reopened = CatalogDatabase::open(&layout).await?;
        let recent = reopened
            .list_recent_projects(RecentProjectLimit::DEFAULT)
            .await?;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].project_id(), first_project_id);
        assert!(matches!(
            recent[0].head(),
            GitHead::Born { object_id, .. }
                if object_id.as_str() == "2222222222222222222222222222222222222222"
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn linked_worktrees_share_one_catalog_project() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("common-git"))?;
        let root_a = create_directory(temporary.path().join("primary"))?;
        let root_b = create_directory(temporary.path().join("linked"))?;
        let primary = project_fixture(
            [3; 32],
            [31; 32],
            &common,
            &root_a,
            Some([7; 32]),
            unborn_head()?,
        )?;
        let linked = project_fixture(
            [3; 32],
            [32; 32],
            &common,
            &root_b,
            Some([7; 32]),
            unborn_head()?,
        )?;

        let catalog = CatalogDatabase::open(&layout).await?;
        let primary_id = catalog.record_opened_project(&primary).await?;
        let linked_id = catalog.record_opened_project(&linked).await?;

        assert_eq!(linked_id, primary_id);
        let recent = catalog
            .list_recent_projects(RecentProjectLimit::new(1)?)
            .await?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].worktree_id(), linked.worktree().id());
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn invalid_persisted_projection_is_rejected_at_the_adapter_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("common-git"))?;
        let root = create_directory(temporary.path().join("worktree"))?;
        let project = project_fixture([4; 32], [41; 32], &common, &root, None, unborn_head()?)?;
        let catalog = CatalogDatabase::open(&layout).await?;
        catalog.record_opened_project(&project).await?;
        drop(catalog);

        mutate_catalog(
            &layout,
            "UPDATE recent_worktrees SET worktree_root_display = char(10)",
        )
        .await?;
        let catalog = CatalogDatabase::open(&layout).await?;

        assert_eq!(
            catalog
                .list_recent_projects(RecentProjectLimit::DEFAULT)
                .await,
            Err(KnowledgeStoreFailure::InvalidStoredData)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn conflicting_worktree_ownership_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("common-git"))?;
        let root = create_directory(temporary.path().join("worktree"))?;
        let project = project_fixture([5; 32], [51; 32], &common, &root, None, unborn_head()?)?;
        let catalog = CatalogDatabase::open(&layout).await?;
        catalog.record_opened_project(&project).await?;
        drop(catalog);

        mutate_catalog(
            &layout,
            "UPDATE recent_worktrees SET repository_id = zeroblob(32)",
        )
        .await?;
        let catalog = CatalogDatabase::open(&layout).await?;

        assert_eq!(
            catalog
                .list_recent_projects(RecentProjectLimit::DEFAULT)
                .await,
            Err(KnowledgeStoreFailure::InvalidStoredData)
        );
        assert_eq!(
            catalog.record_opened_project(&project).await,
            Err(KnowledgeStoreFailure::IdentityConflict)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

fn lock_project_catalog_test() -> Result<MutexGuard<'static, ()>, Box<dyn std::error::Error>> {
    PROJECT_CATALOG_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error>::from(io::Error::other(
            "project catalog test lock was poisoned",
        ))
    })
}

fn create_directory(
    path: impl AsRef<Path>,
) -> Result<CanonicalDirectory, Box<dyn std::error::Error>> {
    fs::create_dir(path.as_ref())?;
    Ok(CanonicalDirectory::from_canonicalized(fs::canonicalize(
        path.as_ref(),
    )?)?)
}

fn project_fixture(
    repository_bytes: [u8; 32],
    worktree_bytes: [u8; 32],
    common_directory: &CanonicalDirectory,
    worktree_root: &CanonicalDirectory,
    remote_bytes: Option<[u8; 32]>,
    head: GitHead,
) -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
    let repository_id = RepositoryId::from_bytes(repository_bytes);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository_id,
            common_directory.clone(),
            remote_bytes.map(RemoteIdentity::from_bytes),
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes(worktree_bytes),
            repository_id,
            worktree_root.clone(),
        ),
        head,
    )?)
}

fn unborn_head() -> Result<GitHead, Box<dyn std::error::Error>> {
    Ok(GitHead::Unborn {
        reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
    })
}

fn born_head(object_id: &str) -> Result<GitHead, Box<dyn std::error::Error>> {
    Ok(GitHead::Born {
        object_id: GitObjectId::try_from_hex(object_id)?,
        reference: Some(GitReferenceName::try_from_full_name("refs/heads/main")?),
    })
}

async fn mutate_catalog(
    layout: &StorageLayout,
    sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(layout.catalog_path())
        .build()
        .await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = OFF", ()).await?;
    connection.execute(sql, ()).await?;
    Ok(())
}
