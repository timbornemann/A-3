//! Contract tests for durable project identity and recent-worktree ordering.

mod support;

use a3_application::{
    KnowledgeStore, KnowledgeStoreFailure, ProjectOpenPreparation, ProjectReconciliationEvidence,
    ProjectReconciliationProposal, RecentProjectLimit,
};
use a3_domain::{
    CanonicalDirectory, GitHead, GitObjectId, GitReferenceName, ProjectIdentity, RemoteIdentity,
    RepositoryId, RepositoryIdentity, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
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

        let catalog = LibsqlKnowledgeStore::open(&layout).await?;
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

        let reopened = LibsqlKnowledgeStore::open(&layout).await?;
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

        let catalog = LibsqlKnowledgeStore::open(&layout).await?;
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
        let catalog = LibsqlKnowledgeStore::open(&layout).await?;
        catalog.record_opened_project(&project).await?;
        drop(catalog);

        mutate_catalog(
            &layout,
            "UPDATE recent_worktrees SET worktree_root_display = char(10)",
        )
        .await?;
        let catalog = LibsqlKnowledgeStore::open(&layout).await?;

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
        let catalog = LibsqlKnowledgeStore::open(&layout).await?;
        catalog.record_opened_project(&project).await?;
        drop(catalog);

        mutate_catalog(
            &layout,
            "UPDATE recent_worktrees SET repository_id = zeroblob(32)",
        )
        .await?;
        let catalog = LibsqlKnowledgeStore::open(&layout).await?;

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

#[test]
fn prepared_reconciliation_resumes_after_the_storage_directory_moved()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("resume-common"))?;
        let previous_root = create_directory(temporary.path().join("resume-previous"))?;
        let target_root = create_directory(temporary.path().join("resume-target"))?;
        let anchor = [91; 32];
        let previous = project_fixture_with_anchor(
            [90; 32],
            [92; 32],
            anchor,
            &common,
            &previous_root,
            None,
            unborn_head()?,
        )?;
        let target = project_fixture_with_anchor(
            [90; 32],
            [93; 32],
            anchor,
            &common,
            &target_root,
            None,
            unborn_head()?,
        )?;

        let store = LibsqlKnowledgeStore::open(&layout).await?;
        let project_id = store.record_opened_project(&previous).await?;
        let proposal = match store.prepare_project_open(&target).await? {
            ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
            other => {
                return Err(format!("expected confirmation proposal, received {other:?}").into());
            }
        };
        drop(store);

        insert_prepared_reconciliation(&layout, &target, &proposal).await?;
        let previous_layout = layout.prepare_project(previous.worktree())?;
        let target_storage = layout
            .root()
            .join("projects")
            .join(target.worktree().id().to_string());
        fs::rename(previous_layout.root(), &target_storage)?;

        let reopened = LibsqlKnowledgeStore::open(&layout).await?;
        let resumed = match reopened.prepare_project_open(&target).await? {
            ProjectOpenPreparation::ResumeConfirmed(proposal) => proposal,
            other => return Err(format!("expected confirmed resume, received {other:?}").into()),
        };
        assert_eq!(resumed, proposal);
        assert_eq!(
            reopened.reconcile_project(&target, &resumed).await?,
            project_id
        );
        let recent = reopened
            .list_recent_projects(RecentProjectLimit::DEFAULT)
            .await?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].project_id(), project_id);
        assert_eq!(recent[0].worktree_id(), target.worktree().id());
        assert_eq!(
            reopened.prepare_project_open(&target).await?,
            ProjectOpenPreparation::Ready
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn invalid_reconciliation_source_is_rejected_before_intent_or_move()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("preflight-common"))?;
        let previous_root = create_directory(temporary.path().join("preflight-previous"))?;
        let target_root = create_directory(temporary.path().join("preflight-target"))?;
        let anchor = [101; 32];
        let previous = project_fixture_with_anchor(
            [100; 32],
            [102; 32],
            anchor,
            &common,
            &previous_root,
            None,
            unborn_head()?,
        )?;
        let target = project_fixture_with_anchor(
            [100; 32],
            [103; 32],
            anchor,
            &common,
            &target_root,
            None,
            unborn_head()?,
        )?;

        let store = LibsqlKnowledgeStore::open(&layout).await?;
        store.record_opened_project(&previous).await?;
        let proposal = match store.prepare_project_open(&target).await? {
            ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
            other => {
                return Err(format!("expected confirmation proposal, received {other:?}").into());
            }
        };
        drop(store);

        let previous_layout = layout.prepare_project(previous.worktree())?;
        mutate_knowledge(
            previous_layout.knowledge_path(),
            "UPDATE schema_migrations SET checksum = zeroblob(32) WHERE version = 1",
        )
        .await?;
        let target_storage = layout
            .root()
            .join("projects")
            .join(target.worktree().id().to_string());

        let reopened = LibsqlKnowledgeStore::open(&layout).await?;
        assert_eq!(
            reopened.reconcile_project(&target, &proposal).await,
            Err(KnowledgeStoreFailure::InvalidStoredData)
        );
        assert!(previous_layout.root().is_dir());
        assert!(!target_storage.exists());
        assert_eq!(reconciliation_intent_count(&layout).await?, 0);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn contradictory_prepared_intent_is_never_resumed() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_project_catalog_test()?;
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("intent-common"))?;
        let previous_root = create_directory(temporary.path().join("intent-previous"))?;
        let target_root = create_directory(temporary.path().join("intent-target"))?;
        let anchor = [111; 32];
        let previous = project_fixture_with_anchor(
            [110; 32],
            [112; 32],
            anchor,
            &common,
            &previous_root,
            None,
            unborn_head()?,
        )?;
        let target = project_fixture_with_anchor(
            [110; 32],
            [113; 32],
            anchor,
            &common,
            &target_root,
            None,
            unborn_head()?,
        )?;

        let store = LibsqlKnowledgeStore::open(&layout).await?;
        store.record_opened_project(&previous).await?;
        let proposal = match store.prepare_project_open(&target).await? {
            ProjectOpenPreparation::ConfirmationRequired(proposal) => proposal,
            other => {
                return Err(format!("expected confirmation proposal, received {other:?}").into());
            }
        };
        drop(store);

        insert_prepared_reconciliation(&layout, &target, &proposal).await?;
        mutate_catalog(
            &layout,
            "UPDATE worktree_reconciliations SET target_repository_id = zeroblob(32)",
        )
        .await?;
        let reopened = LibsqlKnowledgeStore::open(&layout).await?;

        assert_eq!(
            reopened.prepare_project_open(&target).await,
            Err(KnowledgeStoreFailure::IdentityConflict)
        );
        assert!(layout.prepare_project(previous.worktree())?.root().is_dir());
        assert!(
            !layout
                .root()
                .join("projects")
                .join(target.worktree().id().to_string())
                .exists()
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
    project_fixture_with_anchor(
        repository_bytes,
        worktree_bytes,
        worktree_bytes,
        common_directory,
        worktree_root,
        remote_bytes,
        head,
    )
}

fn project_fixture_with_anchor(
    repository_bytes: [u8; 32],
    worktree_bytes: [u8; 32],
    anchor_bytes: [u8; 32],
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
            WorktreeAnchorId::from_bytes(anchor_bytes),
            repository_id,
            worktree_root.clone(),
        ),
        head,
    )?)
}

async fn insert_prepared_reconciliation(
    layout: &StorageLayout,
    target: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(layout.catalog_path())
        .build()
        .await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    let evidence = match proposal.evidence() {
        ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor => "repository-anchor",
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor => "remote-anchor",
    };
    connection
        .execute(
            "INSERT INTO worktree_reconciliations (\n\
             target_worktree_id, source_worktree_id, project_id, source_repository_id,\n\
             target_repository_id, worktree_anchor_id, evidence_kind,\n\
             source_last_open_sequence, status, completed_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'prepared', NULL)",
            libsql::params![
                target.worktree().id().as_bytes().to_vec(),
                proposal.previous_worktree_id().as_bytes().to_vec(),
                proposal.project_id().as_bytes().to_vec(),
                proposal.previous_repository_id().as_bytes().to_vec(),
                target.repository().id().as_bytes().to_vec(),
                proposal.previous_worktree_anchor_id().as_bytes().to_vec(),
                evidence,
                i64::try_from(proposal.expected_revision().get())?
            ],
        )
        .await?;
    Ok(())
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

async fn mutate_knowledge(path: &Path, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    connection.execute(sql, ()).await?;
    Ok(())
}

async fn reconciliation_intent_count(
    layout: &StorageLayout,
) -> Result<i64, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(layout.catalog_path())
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query("SELECT COUNT(*) FROM worktree_reconciliations", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    Ok(row.get(0)?)
}
