//! Contract tests for immutable snapshots and atomic index publication persistence.

mod support;

use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexFailure,
    KnowledgeIndexStore, KnowledgeStore, KnowledgeStoreFailure,
};
use a3_domain::{
    CanonicalDirectory, ContentHash, GitHead, GitReferenceName, IndexLanguage, IndexPublication,
    IndexRunId, IndexRunStart, IndexRunStatus, IndexRunTerminalOutcome, IndexSchemaVersion,
    LanguageAdapterRevision, LanguageAdapterVersion, LinkedGraph, ProjectIdentity, RankProjection,
    RankingPolicyVersion, RepositoryId, RepositoryIdentity, RepositoryPath, Snapshot,
    SnapshotChange, SnapshotChangeKind, SnapshotId, WorktreeAnchorId, WorktreeGeneration,
    WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use futures::executor::block_on;
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use support::TempDirectory;

// libSQL's Windows native test runtime is not safe when separate local database
// fixtures are opened and torn down concurrently inside one process.
static INDEX_REPOSITORY_TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Default)]
struct TestIndexControl {
    progress: Mutex<Vec<a3_domain::Progress>>,
}

impl IndexPersistenceControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        self.progress
            .lock()
            .map_err(|_| IndexPersistenceControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct CancelledIndexControl;

impl IndexPersistenceControl for CancelledIndexControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct UnavailableProgressControl;

impl IndexPersistenceControl for UnavailableProgressControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Err(IndexPersistenceControlError::Unavailable)
    }
}

#[test]
fn snapshot_roundtrip_retains_canonical_reproducibility_state()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([1; 32], [11; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        store.record_opened_project(&fixture.project).await?;
        let snapshot = snapshot(
            [21; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![
                change(b"src/z.rs", [3; 32], SnapshotChangeKind::Delete)?,
                change(b"src/a.rs", [2; 32], SnapshotChangeKind::Upsert)?,
            ],
        )?;

        store.append_snapshot(&fixture.project, &snapshot).await?;
        assert_eq!(
            store.latest_snapshot(&fixture.project).await?,
            Some(snapshot.clone())
        );
        drop(store);

        let reopened = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        assert_eq!(
            reopened.latest_snapshot(&fixture.project).await?,
            Some(snapshot)
        );
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        assert_eq!(read_count(&knowledge_path, "repositories").await?, 1);
        assert_eq!(read_count(&knowledge_path, "worktrees").await?, 1);
        assert_eq!(read_count(&knowledge_path, "snapshot_changes").await?, 2);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn stale_snapshot_generation_rolls_back_without_changing_latest()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([2; 32], [12; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let first = snapshot(
            [22; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [4; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &first).await?;
        let stale = snapshot(
            [23; 32],
            fixture.project.worktree().id(),
            Some(first.id()),
            3,
            vec![change(b"src/lib.rs", [5; 32], SnapshotChangeKind::Upsert)?],
        )?;

        assert_eq!(
            store.append_snapshot(&fixture.project, &stale).await,
            Err(KnowledgeIndexFailure::SnapshotConflict)
        );
        assert_eq!(
            store.latest_snapshot(&fixture.project).await?,
            Some(first.clone())
        );

        let second = snapshot(
            [24; 32],
            fixture.project.worktree().id(),
            Some(first.id()),
            2,
            vec![change(b"src/lib.rs", [5; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &second).await?;
        assert_eq!(store.latest_snapshot(&fixture.project).await?, Some(second));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn snapshot_for_another_worktree_is_rejected_before_writing()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([3; 32], [13; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let foreign = snapshot(
            [25; 32],
            WorktreeId::from_bytes([99; 32]),
            None,
            1,
            Vec::new(),
        )?;

        assert_eq!(
            store.append_snapshot(&fixture.project, &foreign).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::IdentityConflict
            ))
        );
        assert_eq!(store.latest_snapshot(&fixture.project).await?, None);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn index_run_lifecycle_serializes_mutation_and_never_false_publishes()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([4; 32], [14; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [26; 32],
            fixture.project.worktree().id(),
            None,
            1,
            Vec::new(),
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let first_request = run([31; 32], snapshot.id(), 1)?;
        let first = store
            .start_index_run(&fixture.project, first_request)
            .await?;
        assert_eq!(first.sequence().get(), 1);
        assert_eq!(first.status(), IndexRunStatus::Building);

        assert_eq!(
            store
                .start_index_run(&fixture.project, run([32; 32], snapshot.id(), 1)?)
                .await,
            Err(KnowledgeIndexFailure::IndexRunAlreadyActive)
        );
        let failed = store
            .finish_index_run(
                &fixture.project,
                first.id(),
                IndexRunTerminalOutcome::Failed,
            )
            .await?;
        assert_eq!(failed.status(), IndexRunStatus::Failed);
        assert_eq!(
            store
                .finish_index_run(
                    &fixture.project,
                    first.id(),
                    IndexRunTerminalOutcome::Cancelled,
                )
                .await,
            Err(KnowledgeIndexFailure::InvalidIndexRunTransition)
        );
        assert_eq!(
            store.latest_published_index_run(&fixture.project).await?,
            None
        );

        let second = store
            .start_index_run(&fixture.project, run([32; 32], snapshot.id(), 2)?)
            .await?;
        assert_eq!(second.sequence().get(), 2);
        let cancelled = store
            .finish_index_run(
                &fixture.project,
                second.id(),
                IndexRunTerminalOutcome::Cancelled,
            )
            .await?;
        assert_eq!(cancelled.status(), IndexRunStatus::Cancelled);
        drop(store);

        let reopened = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        assert_eq!(
            reopened.latest_index_run(&fixture.project).await?,
            Some(cancelled)
        );
        assert_eq!(
            reopened
                .latest_published_index_run(&fixture.project)
                .await?,
            None
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn index_run_requires_a_snapshot_from_the_same_worktree() -> Result<(), Box<dyn std::error::Error>>
{
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([5; 32], [15; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;

        assert_eq!(
            store
                .start_index_run(
                    &fixture.project,
                    run([33; 32], SnapshotId::from_bytes([88; 32]), 1)?,
                )
                .await,
            Err(KnowledgeIndexFailure::SnapshotNotFound)
        );
        assert_eq!(store.latest_index_run(&fixture.project).await?, None);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn broken_index_run_sequence_blocks_reads_and_further_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([8; 32], [18; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [34; 32],
            fixture.project.worktree().id(),
            None,
            1,
            Vec::new(),
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let first = store
            .start_index_run(&fixture.project, run([35; 32], snapshot.id(), 1)?)
            .await?;
        store
            .finish_index_run(
                &fixture.project,
                first.id(),
                IndexRunTerminalOutcome::Failed,
            )
            .await?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "UPDATE index_runs SET run_sequence = 2 WHERE run_sequence = 1",
        )
        .await?;

        assert_eq!(
            store.latest_index_run(&fixture.project).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        assert_eq!(
            store
                .start_index_run(&fixture.project, run([36; 32], snapshot.id(), 1)?)
                .await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn malformed_persisted_repository_path_is_rejected_at_the_adapter_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([6; 32], [16; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [27; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"safe.rs", [6; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        drop(store);
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "UPDATE snapshot_changes SET repository_path = x'2f657363617065'",
        )
        .await?;
        let reopened = LibsqlKnowledgeStore::open(&fixture.layout).await?;

        assert_eq!(
            reopened.latest_snapshot(&fixture.project).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        assert_eq!(
            reopened.current_file_state(&fixture.project).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn broken_snapshot_parent_chain_is_rejected_after_reopen() -> Result<(), Box<dyn std::error::Error>>
{
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([7; 32], [17; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let first = snapshot(
            [28; 32],
            fixture.project.worktree().id(),
            None,
            1,
            Vec::new(),
        )?;
        let second = snapshot(
            [29; 32],
            fixture.project.worktree().id(),
            Some(first.id()),
            2,
            Vec::new(),
        )?;
        store.append_snapshot(&fixture.project, &first).await?;
        store.append_snapshot(&fixture.project, &second).await?;
        drop(store);
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "UPDATE snapshots SET parent_snapshot_id = NULL WHERE generation = 2",
        )
        .await?;
        let reopened = LibsqlKnowledgeStore::open(&fixture.layout).await?;

        assert_eq!(
            reopened.latest_snapshot(&fixture.project).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        let third = snapshot(
            [30; 32],
            fixture.project.worktree().id(),
            Some(second.id()),
            3,
            Vec::new(),
        )?;
        assert_eq!(
            reopened.append_snapshot(&fixture.project, &third).await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::InvalidStoredData
            ))
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn persistence_control_cancels_before_mutation_and_bounds_progress()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([22; 32], [32; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [43; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [53; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let run = store
            .start_index_run(&fixture.project, run([63; 32], snapshot.id(), 1)?)
            .await?;
        let publication = file_only_publication(snapshot.id(), b"src/lib.rs", [53; 32])?;

        assert_eq!(
            store
                .publish_index(
                    &fixture.project,
                    run.id(),
                    &publication,
                    &CancelledIndexControl,
                )
                .await,
            Err(KnowledgeIndexFailure::Cancelled)
        );
        assert_eq!(
            store
                .publish_index(
                    &fixture.project,
                    run.id(),
                    &publication,
                    &UnavailableProgressControl,
                )
                .await,
            Err(KnowledgeIndexFailure::ProgressUnavailable)
        );
        assert_eq!(store.latest_index_run(&fixture.project).await?, Some(run));

        let control = TestIndexControl::default();
        let published = store
            .publish_index(&fixture.project, run.id(), &publication, &control)
            .await?;
        {
            let progress = control
                .progress
                .lock()
                .map_err(|_| io::Error::other("progress lock was poisoned"))?;
            assert!(progress.len() <= 64);
            assert_eq!(
                progress.first().and_then(|value| value.completed()),
                Some(0)
            );
            assert_eq!(progress.last().map(|value| value.is_complete()), Some(true));
        }

        assert_eq!(
            store
                .latest_published_index(&fixture.project, &CancelledIndexControl)
                .await,
            Err(KnowledgeIndexFailure::Cancelled)
        );
        assert_eq!(
            store
                .rebuild_regenerable_index(&fixture.project, &CancelledIndexControl)
                .await,
            Err(KnowledgeIndexFailure::Cancelled)
        );
        assert_eq!(
            store.latest_published_index_run(&fixture.project).await?,
            Some(published)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn superseded_projection_rows_are_retired_without_deleting_run_history()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let control = TestIndexControl::default();
        let fixture = ProjectFixture::new([23; 32], [33; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let first_snapshot = snapshot(
            [44; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [54; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &first_snapshot)
            .await?;
        let first_run = store
            .start_index_run(&fixture.project, run([64; 32], first_snapshot.id(), 1)?)
            .await?;
        let first_publication =
            file_only_publication(first_snapshot.id(), b"src/lib.rs", [54; 32])?;
        let first_run = store
            .publish_index(
                &fixture.project,
                first_run.id(),
                &first_publication,
                &control,
            )
            .await?;

        let second_snapshot = snapshot(
            [45; 32],
            fixture.project.worktree().id(),
            Some(first_snapshot.id()),
            2,
            vec![change(b"src/lib.rs", [55; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &second_snapshot)
            .await?;
        let second_run = store
            .start_index_run(&fixture.project, run([65; 32], second_snapshot.id(), 1)?)
            .await?;
        let second_publication =
            file_only_publication(second_snapshot.id(), b"src/lib.rs", [55; 32])?;
        let second_run = store
            .publish_index(
                &fixture.project,
                second_run.id(),
                &second_publication,
                &control,
            )
            .await?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();

        assert_eq!(
            read_run_count(&knowledge_path, "file_revisions", first_run.id()).await?,
            0
        );
        assert_eq!(
            read_run_count(&knowledge_path, "file_revisions", second_run.id()).await?,
            1
        );
        assert_eq!(read_count(&knowledge_path, "index_runs").await?, 2);
        assert_eq!(read_count(&knowledge_path, "snapshots").await?, 2);
        let visible = store
            .latest_published_index(&fixture.project, &control)
            .await?
            .ok_or("latest published index is missing")?;
        assert_eq!(visible.run(), second_run);
        assert_eq!(visible.publication(), &second_publication);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn zz_crash_before_visible_publish_rolls_back_new_rows_and_keeps_previous_index()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let control = TestIndexControl::default();
        let fixture = ProjectFixture::new([20; 32], [30; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let first_snapshot = snapshot(
            [40; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [50; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &first_snapshot)
            .await?;
        let first_run = store
            .start_index_run(&fixture.project, run([60; 32], first_snapshot.id(), 1)?)
            .await?;
        let first_publication =
            file_only_publication(first_snapshot.id(), b"src/lib.rs", [50; 32])?;
        let first_run = store
            .publish_index(
                &fixture.project,
                first_run.id(),
                &first_publication,
                &control,
            )
            .await?;

        let second_snapshot = snapshot(
            [41; 32],
            fixture.project.worktree().id(),
            Some(first_snapshot.id()),
            2,
            vec![change(b"src/lib.rs", [51; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &second_snapshot)
            .await?;
        let second_run = store
            .start_index_run(&fixture.project, run([61; 32], second_snapshot.id(), 1)?)
            .await?;
        let second_publication =
            file_only_publication(second_snapshot.id(), b"src/lib.rs", [51; 32])?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "CREATE TRIGGER simulate_crash_before_publish\n\
             BEFORE UPDATE OF status ON index_runs\n\
             WHEN NEW.status = 'published'\n\
             BEGIN SELECT RAISE(ABORT, 'simulated crash'); END",
        )
        .await?;

        assert!(matches!(
            store
                .publish_index(
                    &fixture.project,
                    second_run.id(),
                    &second_publication,
                    &control,
                )
                .await,
            Err(KnowledgeIndexFailure::Storage(
                KnowledgeStoreFailure::Unavailable
            ))
        ));
        mutate_knowledge(
            &knowledge_path,
            "DROP TRIGGER simulate_crash_before_publish",
        )
        .await?;

        let visible = store
            .latest_published_index(&fixture.project, &control)
            .await?
            .ok_or("previous published index is missing")?;
        assert_eq!(visible.run(), first_run);
        assert_eq!(visible.publication(), &first_publication);
        assert_eq!(
            read_run_count(&knowledge_path, "file_revisions", second_run.id()).await?,
            0
        );
        assert_eq!(
            store.latest_index_run(&fixture.project).await?,
            Some(second_run)
        );
        store
            .finish_index_run(
                &fixture.project,
                second_run.id(),
                IndexRunTerminalOutcome::Failed,
            )
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn rebuild_removes_only_regenerable_index_state() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let control = TestIndexControl::default();
        let fixture = ProjectFixture::new([21; 32], [31; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [42; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(
                b"src/main.rs",
                [52; 32],
                SnapshotChangeKind::Upsert,
            )?],
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let run = store
            .start_index_run(&fixture.project, run([62; 32], snapshot.id(), 1)?)
            .await?;
        let publication = file_only_publication(snapshot.id(), b"src/main.rs", [52; 32])?;
        store
            .publish_index(&fixture.project, run.id(), &publication, &control)
            .await?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "CREATE TABLE task_state_probe (\n\
             id INTEGER PRIMARY KEY, body TEXT NOT NULL\n\
             ) STRICT",
        )
        .await?;
        mutate_knowledge(
            &knowledge_path,
            "INSERT INTO task_state_probe (id, body) VALUES (1, 'durable task')",
        )
        .await?;

        store
            .rebuild_regenerable_index(&fixture.project, &control)
            .await?;

        assert_eq!(read_count(&knowledge_path, "index_runs").await?, 0);
        assert_eq!(read_count(&knowledge_path, "file_revisions").await?, 0);
        assert_eq!(read_count(&knowledge_path, "snapshots").await?, 1);
        assert_eq!(read_count(&knowledge_path, "task_state_probe").await?, 1);
        assert_eq!(
            store
                .latest_published_index(&fixture.project, &control)
                .await?,
            None
        );
        assert_eq!(
            store.latest_snapshot(&fixture.project).await?,
            Some(snapshot)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

struct ProjectFixture {
    _temporary: TempDirectory,
    layout: StorageLayout,
    project: ProjectIdentity,
}

impl ProjectFixture {
    fn new(
        repository_bytes: [u8; 32],
        worktree_bytes: [u8; 32],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("common-git"))?;
        let root = create_directory(temporary.path().join("worktree"))?;
        let repository_id = RepositoryId::from_bytes(repository_bytes);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, common, None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes(worktree_bytes),
                WorktreeAnchorId::from_bytes(worktree_bytes),
                repository_id,
                root,
            ),
            unborn_head()?,
        )?;
        Ok(Self {
            _temporary: temporary,
            layout,
            project,
        })
    }
}

fn snapshot(
    id: [u8; 32],
    worktree_id: WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: u64,
    changes: Vec<SnapshotChange>,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    Ok(Snapshot::new(
        SnapshotId::from_bytes(id),
        worktree_id,
        parent_id,
        WorktreeGeneration::new(generation)?,
        unborn_head()?,
        IndexSchemaVersion::new(1)?,
        vec![
            LanguageAdapterRevision::new(
                IndexLanguage::Rust,
                LanguageAdapterVersion::try_from_string("tree-sitter-rust-1".to_owned())?,
            ),
            LanguageAdapterRevision::new(
                IndexLanguage::Generic,
                LanguageAdapterVersion::try_from_string("generic-1".to_owned())?,
            ),
        ],
        changes,
    )?)
}

fn change(
    path: &[u8],
    hash: [u8; 32],
    kind: SnapshotChangeKind,
) -> Result<SnapshotChange, Box<dyn std::error::Error>> {
    Ok(SnapshotChange::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
        kind,
    ))
}

fn run(
    id: [u8; 32],
    snapshot_id: SnapshotId,
    policy: u32,
) -> Result<IndexRunStart, Box<dyn std::error::Error>> {
    Ok(IndexRunStart::new(
        IndexRunId::from_bytes(id),
        snapshot_id,
        RankingPolicyVersion::new(policy)?,
    ))
}

fn file_only_publication(
    snapshot_id: SnapshotId,
    path: &[u8],
    hash: [u8; 32],
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    let revision = a3_domain::FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![revision],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let modules = support::module_projection(&graph, &ranking, &[])?;
    Ok(IndexPublication::new(graph, ranking, Vec::new(), modules)?)
}

fn unborn_head() -> Result<GitHead, Box<dyn std::error::Error>> {
    Ok(GitHead::Unborn {
        reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
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

fn lock_index_repository_test() -> Result<MutexGuard<'static, ()>, Box<dyn std::error::Error>> {
    INDEX_REPOSITORY_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error>::from(io::Error::other(
            "index repository test lock was poisoned",
        ))
    })
}

fn run_index_test<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread
        .name()
        .ok_or_else(|| io::Error::other("libSQL contract test has no harness thread name"))?;
    #[cfg(windows)]
    if std::env::var_os("A3_LIBSQL_ISOLATED_TEST").as_deref()
        != Some(std::ffi::OsStr::new(test_name))
    {
        let status = std::process::Command::new(std::env::current_exe()?)
            .arg(test_name)
            .arg("--exact")
            .arg("--test-threads=1")
            .env("A3_LIBSQL_ISOLATED_TEST", test_name)
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "isolated libSQL contract {test_name} failed with {status}"
            ))
            .into());
        }
        return Ok(());
    }
    let result = block_on(future);
    // The Windows native backend finishes worker teardown just after a store drops.
    // Keep fixture lifetimes serial and give each owned teardown a bounded grace period.
    #[cfg(windows)]
    std::thread::sleep(std::time::Duration::from_millis(500));
    result
}

async fn mutate_knowledge(path: &Path, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = OFF", ()).await?;
    connection.execute(sql, ()).await?;
    Ok(())
}

async fn read_count(path: &Path, table: &str) -> Result<i64, Box<dyn std::error::Error>> {
    let sql = match table {
        "repositories" => "SELECT COUNT(*) FROM repositories",
        "worktrees" => "SELECT COUNT(*) FROM worktrees",
        "snapshot_changes" => "SELECT COUNT(*) FROM snapshot_changes",
        "snapshots" => "SELECT COUNT(*) FROM snapshots",
        "index_runs" => "SELECT COUNT(*) FROM index_runs",
        "file_revisions" => "SELECT COUNT(*) FROM file_revisions",
        "task_state_probe" => "SELECT COUNT(*) FROM task_state_probe",
        _ => return Err(Box::from(io::Error::other("unsupported test table"))),
    };
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    Ok(row.get(0)?)
}

async fn read_run_count(
    path: &Path,
    table: &str,
    run_id: IndexRunId,
) -> Result<i64, Box<dyn std::error::Error>> {
    let sql = match table {
        "file_revisions" => "SELECT COUNT(*) FROM file_revisions WHERE index_run_id = ?1",
        _ => return Err(Box::from(io::Error::other("unsupported run-scoped table"))),
    };
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection.query(sql, [run_id.as_bytes().to_vec()]).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    Ok(row.get(0)?)
}
