//! Contract tests for immutable snapshots and atomic index publication persistence.

mod support;

use a3_application::{
    CompileTaskLens, GetModuleCardFreshness, GetRepositoryTreePage, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexFailure, KnowledgeIndexStore, KnowledgeStore,
    KnowledgeStoreFailure, LoadPendingModuleRemaps, ModuleCardFreshnessControl,
    ModuleCardFreshnessControlError, ModuleCardFreshnessFailure, ModuleCardFreshnessStatus,
    ModuleCardVerificationControl, ModuleCardVerificationControlError, PublishVerifiedModuleCards,
    PublishVerifiedModuleCardsFailure, RemapQueueControl, RemapQueueControlError, RemapQueueLimit,
    RepositoryTreeControl, RepositoryTreeControlError, RepositoryTreeEntryKind,
    RepositoryTreeFailure, RepositoryTreePageSize, RepositoryTreeQuery, TaskLensControl,
    TaskLensControlError, VerifiedModuleCardPublisherFailure,
};
use a3_domain::{
    CanonicalDirectory, Centrality, Confidence, ContentHash, DiagnosticMessage, EvidenceRef,
    GitHead, GitReferenceName, GraphEdge, GraphEndpoint, GraphSymbol, IndexLanguage,
    IndexPublication, IndexRunId, IndexRunStart, IndexRunStatus, IndexRunTerminalOutcome,
    IndexSchemaVersion, IndexedFileAnalysis, InvalidationReason, LanguageAdapterRevision,
    LanguageAdapterVersion, LinkResolution, LinkedGraph, LocalSymbolId, MapperProfileVersion,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal,
    ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleCardVerificationCandidate,
    ModuleCardVerifier, ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleClaimProposal, ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
    ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, ParseCoverage,
    ParseDiagnostic, ParseDiagnosticCode, ParseDiagnosticSeverity, ParsedSymbol, ProjectIdentity,
    ProposedModuleCardField, PublishedIndex, RankProjection, RankScore, RankingPolicyVersion,
    RemapPriority, RepositoryCard, RepositoryId, RepositoryIdentity, RepositoryModule,
    RepositoryPath, ResolvedModuleCardEvidence, ResolvedModuleCardEvidenceSet, Snapshot,
    SnapshotChange, SnapshotChangeKind, SnapshotId, SourcePosition, SourceRange, SymbolId,
    SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SyntaxProvider, SyntaxRelationKind,
    TaskLensSeed, TaskLensSeedSet, TaskLensSeedText, TaskLensTokenBudget, VerifiedClaimKind,
    VerifiedModuleCardBatch, WorktreeAnchorId, WorktreeGeneration, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use futures::executor::block_on;
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
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

impl ModuleCardVerificationControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: a3_domain::Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        self.progress
            .lock()
            .map_err(|_| ModuleCardVerificationControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

impl TaskLensControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: a3_domain::Progress) -> Result<(), TaskLensControlError> {
        self.progress
            .lock()
            .map_err(|_| TaskLensControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

impl RemapQueueControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: a3_domain::Progress) -> Result<(), RemapQueueControlError> {
        self.progress
            .lock()
            .map_err(|_| RemapQueueControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

impl ModuleCardFreshnessControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: a3_domain::Progress,
    ) -> Result<(), ModuleCardFreshnessControlError> {
        self.progress
            .lock()
            .map_err(|_| ModuleCardFreshnessControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

impl RepositoryTreeControl for TestIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: a3_domain::Progress,
    ) -> Result<(), RepositoryTreeControlError> {
        self.progress
            .lock()
            .map_err(|_| RepositoryTreeControlError::Unavailable)?
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

impl ModuleCardVerificationControl for CancelledIndexControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        Ok(())
    }
}

impl RepositoryTreeControl for CancelledIndexControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), RepositoryTreeControlError> {
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

impl ModuleCardVerificationControl for UnavailableProgressControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        Err(ModuleCardVerificationControlError::Unavailable)
    }
}

#[test]
fn snapshot_roundtrip_retains_canonical_reproducibility_state()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([1; 32], [11; 32])?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&fixture.layout).await?);
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
            GetModuleCardFreshness::new(store.clone())
                .execute(&fixture.project, &TestIndexControl::default())
                .await?,
            None
        );
        assert_eq!(
            GetRepositoryTreePage::new(store.clone())
                .execute(
                    &fixture.project,
                    &RepositoryTreeQuery::new(None, None, RepositoryTreePageSize::DEFAULT),
                    &TestIndexControl::default(),
                )
                .await?,
            None
        );
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
fn published_file_analysis_roundtrip_retains_coverage_and_safe_diagnostics()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([35; 32], [36; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [37; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [38; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let run = store
            .start_index_run(&fixture.project, run([39; 32], snapshot.id(), 1)?)
            .await?;
        let publication = analyzed_file_publication(snapshot.id(), b"src/lib.rs", [38; 32])?;

        let published = store
            .publish_index(
                &fixture.project,
                run.id(),
                &publication,
                &TestIndexControl::default(),
            )
            .await?;
        let reloaded = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("published file-analysis fixture is missing")?;

        assert_eq!(reloaded.run(), published);
        assert_eq!(
            reloaded.publication().file_analyses(),
            publication.file_analyses()
        );
        assert_eq!(
            reloaded.publication().file_analyses()[0]
                .coverage()
                .map(ParseCoverage::basis_points),
            Some(8_000)
        );
        assert_eq!(
            reloaded.publication().file_analyses()[0]
                .diagnostics()
                .len(),
            1
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn published_v4_index_without_analysis_rows_remains_readable()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([40; 32], [41; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let snapshot = snapshot(
            [42; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [43; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store.append_snapshot(&fixture.project, &snapshot).await?;
        let run = store
            .start_index_run(&fixture.project, run([44; 32], snapshot.id(), 1)?)
            .await?;
        let publication = file_only_publication(snapshot.id(), b"src/lib.rs", [43; 32])?;
        store
            .publish_index(
                &fixture.project,
                run.id(),
                &publication,
                &TestIndexControl::default(),
            )
            .await?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "DELETE FROM index_parse_diagnostics;
             DELETE FROM index_file_analyses;
             UPDATE snapshots SET index_schema_version = 4",
        )
        .await?;

        let reloaded = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("legacy V4 fixture index is missing")?;

        assert_eq!(reloaded.publication().file_analyses().len(), 1);
        assert_eq!(
            reloaded.publication().file_analyses()[0].language(),
            IndexLanguage::Generic
        );
        assert_eq!(reloaded.publication().file_analyses()[0].coverage(), None);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn verified_module_cards_publish_atomically_with_evidence_and_search_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([71; 32], [72; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let initial_snapshot = snapshot(
            [73; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [74; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &initial_snapshot)
            .await?;
        let initial_run = store
            .start_index_run(&fixture.project, run([75; 32], initial_snapshot.id(), 1)?)
            .await?;
        let publication = symbol_publication(initial_snapshot.id(), b"src/lib.rs", [74; 32])?;
        store
            .publish_index(
                &fixture.project,
                initial_run.id(),
                &publication,
                &TestIndexControl::default(),
            )
            .await?;
        let published = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("published fixture index is missing")?;
        let batch = verified_card_batch(&published)?;
        let publisher = PublishVerifiedModuleCards::new(&store);
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();

        assert_eq!(
            publisher
                .execute(&fixture.project, &batch, &CancelledIndexControl)
                .await,
            Err(PublishVerifiedModuleCardsFailure::Cancelled)
        );
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 0);
        assert_eq!(
            publisher
                .execute(&fixture.project, &batch, &UnavailableProgressControl)
                .await,
            Err(PublishVerifiedModuleCardsFailure::Publisher(
                VerifiedModuleCardPublisherFailure::ProgressUnavailable
            ))
        );
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 0);

        mutate_knowledge(
            &knowledge_path,
            "CREATE TRIGGER reject_verified_claim BEFORE INSERT ON claims\n\
             BEGIN SELECT RAISE(ABORT, 'simulated card publication crash'); END",
        )
        .await?;
        assert_eq!(
            publisher
                .execute(&fixture.project, &batch, &TestIndexControl::default())
                .await,
            Err(PublishVerifiedModuleCardsFailure::Publisher(
                VerifiedModuleCardPublisherFailure::Storage
            ))
        );
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 0);
        assert_eq!(read_count(&knowledge_path, "evidence_refs").await?, 0);
        mutate_knowledge(&knowledge_path, "DROP TRIGGER reject_verified_claim").await?;

        let control = TestIndexControl::default();
        let receipt = publisher
            .execute(&fixture.project, &batch, &control)
            .await?;
        assert_eq!(receipt.snapshot_id(), initial_snapshot.id());
        assert_eq!(receipt.card_count(), 1);
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 1);
        assert_eq!(read_count(&knowledge_path, "module_card_fields").await?, 2);
        assert_eq!(
            read_count(&knowledge_path, "module_card_field_values").await?,
            2
        );
        assert_eq!(
            read_count(&knowledge_path, "module_card_field_evidence").await?,
            2
        );
        assert_eq!(read_count(&knowledge_path, "evidence_refs").await?, 1);
        assert_eq!(read_count(&knowledge_path, "claims").await?, 2);
        assert_eq!(read_count(&knowledge_path, "claim_evidence").await?, 1);
        assert_eq!(read_count(&knowledge_path, "claim_relations").await?, 1);
        assert_eq!(read_count(&knowledge_path, "card_fts").await?, 1);
        assert_eq!(
            read_lexical_card_count(&knowledge_path, initial_run.id()).await?,
            1
        );
        assert_eq!(
            read_claim_classification(&knowledge_path, initial_run.id()).await?,
            vec![
                (
                    "architectural-intent".to_owned(),
                    "hypothesis".to_owned(),
                    5_000
                ),
                ("relation".to_owned(), "fact".to_owned(), 7_000),
            ]
        );
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
            publisher
                .execute(&fixture.project, &batch, &TestIndexControl::default())
                .await,
            Err(PublishVerifiedModuleCardsFailure::Publisher(
                VerifiedModuleCardPublisherFailure::Rejected
            ))
        );
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 1);

        let lens = CompileTaskLens::new(&store, &store, &store)
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(
                    TaskLensSeedText::try_from_string(
                        "repair the public call relation".to_owned(),
                    )?,
                    TaskLensSeedText::try_from_string(
                        "inspect implementation and tests".to_owned(),
                    )?,
                    vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                        b"src/lib.rs".to_vec(),
                    )?)],
                )?,
                TaskLensTokenBudget::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(lens.index_run_id(), initial_run.id());
        assert_eq!(lens.snapshot_id(), initial_snapshot.id());
        assert_eq!(lens.claims().len(), 2);
        assert!(
            lens.claims()
                .iter()
                .any(|claim| claim.kind() == VerifiedClaimKind::Fact)
        );

        let changed_snapshot = snapshot(
            [83; 32],
            fixture.project.worktree().id(),
            Some(initial_snapshot.id()),
            2,
            vec![change(b"src/lib.rs", [84; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &changed_snapshot)
            .await?;
        let changed_run = store
            .start_index_run(&fixture.project, run([85; 32], changed_snapshot.id(), 1)?)
            .await?;
        let changed_publication =
            symbol_publication(changed_snapshot.id(), b"src/lib.rs", [84; 32])?;
        store
            .publish_index(
                &fixture.project,
                changed_run.id(),
                &changed_publication,
                &TestIndexControl::default(),
            )
            .await?;

        assert_eq!(
            read_single_text(&knowledge_path, "SELECT status FROM module_card_lifecycle").await?,
            "stale"
        );
        assert_eq!(
            read_count(&knowledge_path, "evidence_invalidations").await?,
            1
        );
        assert_eq!(read_count(&knowledge_path, "module_remap_queue").await?, 1);
        let remaps = LoadPendingModuleRemaps::new(&store)
            .execute(
                &fixture.project,
                RemapQueueLimit::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(remaps.target_index_run_id(), changed_run.id());
        assert_eq!(remaps.target_snapshot_id(), changed_snapshot.id());
        assert_eq!(remaps.entries().len(), 1);
        assert_eq!(remaps.entries()[0].priority(), RemapPriority::Direct);
        assert_eq!(
            read_single_text(
                &knowledge_path,
                "SELECT lifecycle.status FROM claim_lifecycle lifecycle\n\
                 JOIN claims ON claims.source_index_run_id = lifecycle.source_index_run_id\n\
                  AND claims.claim_id = lifecycle.claim_id\n\
                 WHERE claims.claim_kind = 'fact'"
            )
            .await?,
            "stale"
        );
        let rebuilt_lens = CompileTaskLens::new(&store, &store, &store)
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(
                    TaskLensSeedText::try_from_string(
                        "repair the public call relation".to_owned(),
                    )?,
                    TaskLensSeedText::try_from_string(
                        "inspect implementation and tests".to_owned(),
                    )?,
                    vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                        b"src/lib.rs".to_vec(),
                    )?)],
                )?,
                TaskLensTokenBudget::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(rebuilt_lens.index_run_id(), changed_run.id());
        assert_eq!(rebuilt_lens.snapshot_id(), changed_snapshot.id());
        assert!(rebuilt_lens.claims().is_empty());
        assert_ne!(rebuilt_lens.digest(), lens.digest());

        store
            .rebuild_regenerable_index(&fixture.project, &TestIndexControl::default())
            .await?;
        assert_eq!(read_count(&knowledge_path, "index_runs").await?, 0);
        assert_eq!(read_count(&knowledge_path, "card_fts").await?, 0);
        assert_eq!(read_count(&knowledge_path, "module_cards").await?, 1);
        assert_eq!(read_count(&knowledge_path, "claims").await?, 2);
        assert_eq!(read_count(&knowledge_path, "evidence_refs").await?, 1);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn unchanged_file_claim_survives_an_unrelated_delta_without_remap()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([86; 32], [87; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let initial_snapshot = snapshot(
            [88; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(b"src/lib.rs", [89; 32], SnapshotChangeKind::Upsert)?],
        )?;
        store
            .append_snapshot(&fixture.project, &initial_snapshot)
            .await?;
        let initial_run = store
            .start_index_run(&fixture.project, run([90; 32], initial_snapshot.id(), 1)?)
            .await?;
        let initial_publication =
            symbol_publication(initial_snapshot.id(), b"src/lib.rs", [89; 32])?;
        store
            .publish_index(
                &fixture.project,
                initial_run.id(),
                &initial_publication,
                &TestIndexControl::default(),
            )
            .await?;
        let published = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("published fixture index is missing")?;
        PublishVerifiedModuleCards::new(&store)
            .execute(
                &fixture.project,
                &verified_file_card_batch(&published)?,
                &TestIndexControl::default(),
            )
            .await?;

        let unrelated_snapshot = snapshot(
            [91; 32],
            fixture.project.worktree().id(),
            Some(initial_snapshot.id()),
            2,
            vec![change(
                b"docs/note.md",
                [92; 32],
                SnapshotChangeKind::Upsert,
            )?],
        )?;
        store
            .append_snapshot(&fixture.project, &unrelated_snapshot)
            .await?;
        let unrelated_run = store
            .start_index_run(&fixture.project, run([93; 32], unrelated_snapshot.id(), 1)?)
            .await?;
        let unrelated_publication = symbol_publication_with_extra_files(
            unrelated_snapshot.id(),
            b"src/lib.rs",
            [89; 32],
            &[(b"docs/note.md", [92; 32])],
        )?;
        store
            .publish_index(
                &fixture.project,
                unrelated_run.id(),
                &unrelated_publication,
                &TestIndexControl::default(),
            )
            .await?;

        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        assert_eq!(
            read_single_text(&knowledge_path, "SELECT status FROM module_card_lifecycle").await?,
            "published"
        );
        assert_eq!(
            read_count(&knowledge_path, "evidence_invalidations").await?,
            0
        );
        assert_eq!(read_count(&knowledge_path, "module_remap_queue").await?, 0);

        let lens = CompileTaskLens::new(&store, &store, &store)
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(
                    TaskLensSeedText::try_from_string("inspect src/lib.rs".to_owned())?,
                    TaskLensSeedText::try_from_string("retain current path facts".to_owned())?,
                    vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                        b"src/lib.rs".to_vec(),
                    )?)],
                )?,
                TaskLensTokenBudget::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(lens.index_run_id(), unrelated_run.id());
        assert_eq!(lens.claims().len(), 1);
        assert_eq!(lens.claims()[0].source_index_run_id(), initial_run.id());
        assert_eq!(lens.claims()[0].kind(), VerifiedClaimKind::Fact);

        let parser_snapshot = snapshot_with_rust_version(
            [96; 32],
            fixture.project.worktree().id(),
            Some(unrelated_snapshot.id()),
            3,
            "tree-sitter-rust-2",
            Vec::new(),
        )?;
        store
            .append_snapshot(&fixture.project, &parser_snapshot)
            .await?;
        let parser_run = store
            .start_index_run(&fixture.project, run([97; 32], parser_snapshot.id(), 1)?)
            .await?;
        let parser_publication = symbol_publication_with_extra_files(
            parser_snapshot.id(),
            b"src/lib.rs",
            [89; 32],
            &[(b"docs/note.md", [92; 32])],
        )?;
        store
            .publish_index(
                &fixture.project,
                parser_run.id(),
                &parser_publication,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(
            read_single_text(&knowledge_path, "SELECT reason FROM module_card_lifecycle").await?,
            "parser-version-changed"
        );
        assert_eq!(read_count(&knowledge_path, "module_remap_queue").await?, 1);
        let parser_lens = CompileTaskLens::new(&store, &store, &store)
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(
                    TaskLensSeedText::try_from_string("inspect src/lib.rs".to_owned())?,
                    TaskLensSeedText::try_from_string("revalidate parser facts".to_owned())?,
                    vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                        b"src/lib.rs".to_vec(),
                    )?)],
                )?,
                TaskLensTokenBudget::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(parser_lens.index_run_id(), parser_run.id());
        assert!(parser_lens.claims().is_empty());
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn direct_module_change_marks_only_one_hop_dependents_for_review()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([101; 32], [102; 32])?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&fixture.layout).await?);
        let initial_snapshot = snapshot(
            [103; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![
                change(b"packages/a/lib.rs", [111; 32], SnapshotChangeKind::Upsert)?,
                change(b"packages/b/lib.rs", [112; 32], SnapshotChangeKind::Upsert)?,
                change(b"packages/c/lib.rs", [113; 32], SnapshotChangeKind::Upsert)?,
            ],
        )?;
        store
            .append_snapshot(&fixture.project, &initial_snapshot)
            .await?;
        let initial_run = store
            .start_index_run(&fixture.project, run([104; 32], initial_snapshot.id(), 1)?)
            .await?;
        let initial_publication =
            multi_module_publication(initial_snapshot.id(), [[111; 32], [112; 32], [113; 32]])?;
        store
            .publish_index(
                &fixture.project,
                initial_run.id(),
                &initial_publication,
                &TestIndexControl::default(),
            )
            .await?;
        let initial = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("multi-module fixture index is missing")?;
        PublishVerifiedModuleCards::new(store.as_ref())
            .execute(
                &fixture.project,
                &multi_module_card_batch(&initial)?,
                &TestIndexControl::default(),
            )
            .await?;

        let changed_snapshot = snapshot(
            [105; 32],
            fixture.project.worktree().id(),
            Some(initial_snapshot.id()),
            2,
            vec![change(
                b"packages/a/lib.rs",
                [121; 32],
                SnapshotChangeKind::Upsert,
            )?],
        )?;
        store
            .append_snapshot(&fixture.project, &changed_snapshot)
            .await?;
        let changed_run = store
            .start_index_run(&fixture.project, run([106; 32], changed_snapshot.id(), 1)?)
            .await?;
        let changed_publication =
            multi_module_publication(changed_snapshot.id(), [[121; 32], [112; 32], [113; 32]])?;
        store
            .publish_index(
                &fixture.project,
                changed_run.id(),
                &changed_publication,
                &TestIndexControl::default(),
            )
            .await?;

        let remaps = LoadPendingModuleRemaps::new(store.as_ref())
            .execute(
                &fixture.project,
                RemapQueueLimit::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(remaps.entries().len(), 2);
        assert_eq!(
            remaps.entries()[0].module_id(),
            ModuleId::from_bytes([201; 32])
        );
        assert_eq!(remaps.entries()[0].priority(), RemapPriority::Direct);
        assert_eq!(
            remaps.entries()[1].module_id(),
            ModuleId::from_bytes([202; 32])
        );
        assert_eq!(remaps.entries()[1].priority(), RemapPriority::Dependent);
        assert!(
            remaps
                .entries()
                .iter()
                .all(|entry| entry.module_id() != ModuleId::from_bytes([203; 32]))
        );
        let freshness_query = GetModuleCardFreshness::new(store.clone());
        let freshness = freshness_query
            .execute(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("freshness projection is missing")?;
        assert_eq!(freshness.index_run_id(), changed_run.id());
        assert_eq!(freshness.snapshot_id(), changed_snapshot.id());
        assert_eq!(freshness.published_count(), 1);
        assert_eq!(freshness.stale_count(), 1);
        assert_eq!(freshness.needs_review_count(), 1);
        assert_eq!(freshness.total_count(), 3);
        assert_eq!(freshness.reason_counts().len(), 2);
        assert_eq!(
            freshness.reason_counts()[0].status(),
            ModuleCardFreshnessStatus::Stale
        );
        assert_eq!(
            freshness.reason_counts()[0].reason(),
            InvalidationReason::EvidenceChanged
        );
        assert_eq!(
            freshness.reason_counts()[1].status(),
            ModuleCardFreshnessStatus::NeedsReview
        );
        assert_eq!(
            freshness.reason_counts()[1].reason(),
            InvalidationReason::DirectDependencyChanged
        );

        let lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref())
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(
                    TaskLensSeedText::try_from_string("inspect independent module c".to_owned())?,
                    TaskLensSeedText::try_from_string("retain unrelated evidence".to_owned())?,
                    vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                        b"packages/c/lib.rs".to_vec(),
                    )?)],
                )?,
                TaskLensTokenBudget::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert_eq!(lens.index_run_id(), changed_run.id());
        assert_eq!(lens.claims().len(), 1);
        assert_eq!(
            lens.claims()[0].module_id(),
            ModuleId::from_bytes([203; 32])
        );
        assert_eq!(lens.claims()[0].kind(), VerifiedClaimKind::Fact);

        let changed = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("changed multi-module fixture index is missing")?;
        PublishVerifiedModuleCards::new(store.as_ref())
            .execute(
                &fixture.project,
                &multi_module_card_batch(&changed)?,
                &TestIndexControl::default(),
            )
            .await?;
        let remapped = freshness_query
            .execute(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("remapped freshness projection is missing")?;
        assert_eq!(remapped.published_count(), 3);
        assert_eq!(remapped.stale_count(), 0);
        assert_eq!(remapped.needs_review_count(), 0);
        assert_eq!(remapped.total_count(), 3);
        assert!(remapped.reason_counts().is_empty());

        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "DELETE FROM module_card_lifecycle WHERE card_id = x'9797979797979797979797979797979797979797979797979797979797979797'",
        )
        .await?;
        assert_eq!(
            freshness_query
                .execute(&fixture.project, &TestIndexControl::default())
                .await,
            Err(ModuleCardFreshnessFailure::InvalidStoredProjection)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn removed_module_remains_visible_as_stale_without_a_remap_request()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let fixture = ProjectFixture::new([131; 32], [132; 32])?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&fixture.layout).await?);
        let initial_snapshot = snapshot(
            [133; 32],
            fixture.project.worktree().id(),
            None,
            1,
            vec![change(
                b"src/lib.rs",
                [134; 32],
                SnapshotChangeKind::Upsert,
            )?],
        )?;
        store
            .append_snapshot(&fixture.project, &initial_snapshot)
            .await?;
        let initial_run = store
            .start_index_run(&fixture.project, run([135; 32], initial_snapshot.id(), 1)?)
            .await?;
        store
            .publish_index(
                &fixture.project,
                initial_run.id(),
                &symbol_publication(initial_snapshot.id(), b"src/lib.rs", [134; 32])?,
                &TestIndexControl::default(),
            )
            .await?;
        let initial = store
            .latest_published_index(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("initial index is missing")?;
        PublishVerifiedModuleCards::new(store.as_ref())
            .execute(
                &fixture.project,
                &verified_file_card_batch(&initial)?,
                &TestIndexControl::default(),
            )
            .await?;

        let removed_snapshot = snapshot(
            [136; 32],
            fixture.project.worktree().id(),
            Some(initial_snapshot.id()),
            2,
            vec![change(
                b"src/lib.rs",
                [134; 32],
                SnapshotChangeKind::Delete,
            )?],
        )?;
        store
            .append_snapshot(&fixture.project, &removed_snapshot)
            .await?;
        let removed_run = store
            .start_index_run(&fixture.project, run([137; 32], removed_snapshot.id(), 1)?)
            .await?;
        store
            .publish_index(
                &fixture.project,
                removed_run.id(),
                &empty_publication(removed_snapshot.id())?,
                &TestIndexControl::default(),
            )
            .await?;

        let remaps = LoadPendingModuleRemaps::new(store.as_ref())
            .execute(
                &fixture.project,
                RemapQueueLimit::DEFAULT,
                &TestIndexControl::default(),
            )
            .await?;
        assert!(remaps.entries().is_empty());
        let freshness = GetModuleCardFreshness::new(store)
            .execute(&fixture.project, &TestIndexControl::default())
            .await?
            .ok_or("removed-module freshness projection is missing")?;
        assert_eq!(freshness.index_run_id(), removed_run.id());
        assert_eq!(freshness.published_count(), 0);
        assert_eq!(freshness.stale_count(), 1);
        assert_eq!(freshness.needs_review_count(), 0);
        assert_eq!(freshness.total_count(), 1);
        assert_eq!(freshness.reason_counts().len(), 1);
        assert_eq!(
            freshness.reason_counts()[0].reason(),
            InvalidationReason::ModuleRemoved
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

#[test]
fn repository_tree_pages_root_and_directories_losslessly_against_latest_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_index_repository_test()?;
    run_index_test(async {
        let control = TestIndexControl::default();
        let fixture = ProjectFixture::new([71; 32], [72; 32])?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&fixture.layout).await?);
        let non_utf8_path = vec![0xff, b'.', b'r', b's'];
        let files = vec![
            (b"README.md".to_vec(), [11; 32]),
            (b"docs/guide.md".to_vec(), [12; 32]),
            (b"src/lib.rs".to_vec(), [13; 32]),
            (b"src/nested/mod.rs".to_vec(), [14; 32]),
            (non_utf8_path.clone(), [15; 32]),
        ];
        let first_snapshot = snapshot(
            [73; 32],
            fixture.project.worktree().id(),
            None,
            1,
            files
                .iter()
                .map(|(path, hash)| change(path, *hash, SnapshotChangeKind::Upsert))
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        store
            .append_snapshot(&fixture.project, &first_snapshot)
            .await?;
        let first_run = store
            .start_index_run(&fixture.project, run([74; 32], first_snapshot.id(), 1)?)
            .await?;
        store
            .publish_index(
                &fixture.project,
                first_run.id(),
                &files_publication(first_snapshot.id(), &files)?,
                &control,
            )
            .await?;
        let query = GetRepositoryTreePage::new(store.clone());

        let first_page = query
            .execute(
                &fixture.project,
                &RepositoryTreeQuery::new(None, None, RepositoryTreePageSize::new(2)?),
                &control,
            )
            .await?
            .ok_or("repository-tree root page is missing")?;
        assert_eq!(first_page.index_run_id(), first_run.id());
        assert_eq!(first_page.snapshot_id(), first_snapshot.id());
        assert_eq!(first_page.entries().len(), 2);
        assert_eq!(
            first_page.entries()[0].child_name().as_bytes(),
            b"README.md"
        );
        assert_eq!(
            first_page.entries()[0].kind(),
            RepositoryTreeEntryKind::File
        );
        assert_eq!(
            first_page.entries()[0].content_hash(),
            Some(ContentHash::from_bytes([11; 32]))
        );
        assert_eq!(first_page.entries()[1].child_name().as_bytes(), b"docs");
        assert_eq!(
            first_page.entries()[1].kind(),
            RepositoryTreeEntryKind::Directory
        );
        assert_eq!(first_page.entries()[1].descendant_file_count(), 1);
        let first_cursor = first_page
            .next_cursor()
            .cloned()
            .ok_or("repository-tree first page should be truncated")?;

        let second_page = query
            .execute(
                &fixture.project,
                &RepositoryTreeQuery::new(
                    None,
                    Some(first_cursor),
                    RepositoryTreePageSize::new(2)?,
                ),
                &control,
            )
            .await?
            .ok_or("repository-tree second root page is missing")?;
        assert_eq!(second_page.entries().len(), 2);
        assert_eq!(second_page.entries()[0].child_name().as_bytes(), b"src");
        assert_eq!(second_page.entries()[0].descendant_file_count(), 2);
        assert_eq!(
            second_page.entries()[1].child_name().as_bytes(),
            non_utf8_path
        );
        assert!(
            !second_page.entries()[1]
                .display_name()
                .as_str()
                .chars()
                .any(char::is_control)
        );
        assert!(second_page.next_cursor().is_none());

        let src = RepositoryPath::try_from_bytes(b"src".to_vec())?;
        let src_page = query
            .execute(
                &fixture.project,
                &RepositoryTreeQuery::new(Some(src.clone()), None, RepositoryTreePageSize::DEFAULT),
                &control,
            )
            .await?
            .ok_or("repository-tree src page is missing")?;
        assert_eq!(src_page.entries().len(), 2);
        assert_eq!(src_page.entries()[0].path().as_bytes(), b"src/lib.rs");
        assert_eq!(src_page.entries()[0].kind(), RepositoryTreeEntryKind::File);
        assert_eq!(src_page.entries()[1].path().as_bytes(), b"src/nested");
        assert_eq!(
            src_page.entries()[1].kind(),
            RepositoryTreeEntryKind::Directory
        );

        assert_eq!(
            query
                .execute(
                    &fixture.project,
                    &RepositoryTreeQuery::new(
                        Some(RepositoryPath::try_from_bytes(b"missing".to_vec())?),
                        None,
                        RepositoryTreePageSize::DEFAULT,
                    ),
                    &control,
                )
                .await,
            Err(RepositoryTreeFailure::DirectoryUnavailable)
        );
        assert_eq!(
            query
                .execute(
                    &fixture.project,
                    &RepositoryTreeQuery::new(None, None, RepositoryTreePageSize::DEFAULT),
                    &CancelledIndexControl,
                )
                .await,
            Err(RepositoryTreeFailure::Cancelled)
        );

        let mut next_changes = files
            .iter()
            .map(|(path, hash)| change(path, *hash, SnapshotChangeKind::Delete))
            .collect::<Result<Vec<_>, _>>()?;
        next_changes.push(change(b"next.rs", [16; 32], SnapshotChangeKind::Upsert)?);
        let next_snapshot = snapshot(
            [75; 32],
            fixture.project.worktree().id(),
            Some(first_snapshot.id()),
            2,
            next_changes,
        )?;
        store
            .append_snapshot(&fixture.project, &next_snapshot)
            .await?;
        let next_run = store
            .start_index_run(&fixture.project, run([76; 32], next_snapshot.id(), 1)?)
            .await?;
        store
            .publish_index(
                &fixture.project,
                next_run.id(),
                &file_only_publication(next_snapshot.id(), b"next.rs", [16; 32])?,
                &control,
            )
            .await?;

        let latest_page = query
            .execute(
                &fixture.project,
                &RepositoryTreeQuery::new(None, None, RepositoryTreePageSize::DEFAULT),
                &control,
            )
            .await?
            .ok_or("latest repository-tree root page is missing")?;
        assert_eq!(latest_page.index_run_id(), next_run.id());
        assert_eq!(latest_page.snapshot_id(), next_snapshot.id());
        assert_eq!(latest_page.entries().len(), 1);
        assert_eq!(latest_page.entries()[0].path().as_bytes(), b"next.rs");
        assert_eq!(
            query
                .execute(
                    &fixture.project,
                    &RepositoryTreeQuery::new(Some(src), None, RepositoryTreePageSize::DEFAULT,),
                    &control,
                )
                .await,
            Err(RepositoryTreeFailure::DirectoryUnavailable)
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
    snapshot_with_rust_version(
        id,
        worktree_id,
        parent_id,
        generation,
        "tree-sitter-rust-1",
        changes,
    )
}

fn snapshot_with_rust_version(
    id: [u8; 32],
    worktree_id: WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: u64,
    rust_version: &str,
    changes: Vec<SnapshotChange>,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    Ok(Snapshot::new(
        SnapshotId::from_bytes(id),
        worktree_id,
        parent_id,
        WorktreeGeneration::new(generation)?,
        unborn_head()?,
        IndexSchemaVersion::v5(),
        vec![
            LanguageAdapterRevision::new(
                IndexLanguage::Rust,
                LanguageAdapterVersion::try_from_string(rust_version.to_owned())?,
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

fn files_publication(
    snapshot_id: SnapshotId,
    files: &[(Vec<u8>, [u8; 32])],
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    let revisions = files
        .iter()
        .map(|(path, hash)| {
            RepositoryPath::try_from_bytes(path.clone())
                .map(|path| a3_domain::FileRevision::new(path, ContentHash::from_bytes(*hash)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let graph = LinkedGraph::new(snapshot_id, revisions, Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let modules = support::module_projection(&graph, &ranking, &[])?;
    Ok(IndexPublication::new(graph, ranking, Vec::new(), modules)?)
}

fn empty_publication(
    snapshot_id: SnapshotId,
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        Vec::new(),
        Vec::new(),
        RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            Vec::new(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            0,
            0,
        )?,
    )?;
    Ok(IndexPublication::new(graph, ranking, Vec::new(), modules)?)
}

fn analyzed_file_publication(
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
        vec![revision.clone()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let modules = support::module_projection(&graph, &ranking, &[])?;
    let diagnostic = ParseDiagnostic::new(
        ParseDiagnosticCode::SyntaxError,
        ParseDiagnosticSeverity::Error,
        SourceRange::new(8, 10, SourcePosition::new(0, 8), SourcePosition::new(0, 10))?,
        DiagnosticMessage::try_from_string("syntax error".to_owned())?,
    );
    let analysis = IndexedFileAnalysis::parsed(
        revision,
        LanguageAdapterRevision::new(
            IndexLanguage::Rust,
            LanguageAdapterVersion::try_from_string("tree-sitter-rust-1".to_owned())?,
        ),
        ParseCoverage::new(10, 8, 1)?,
        vec![diagnostic],
    )?;
    Ok(IndexPublication::new_with_file_analyses(
        graph,
        ranking,
        Vec::new(),
        modules,
        vec![analysis],
    )?)
}

fn symbol_publication(
    snapshot_id: SnapshotId,
    path: &[u8],
    hash: [u8; 32],
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    symbol_publication_with_extra_files(snapshot_id, path, hash, &[])
}

fn symbol_publication_with_extra_files(
    snapshot_id: SnapshotId,
    path: &[u8],
    hash: [u8; 32],
    extra_files: &[(&[u8], [u8; 32])],
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    let revision = a3_domain::FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
    );
    let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
    let symbol_id = SymbolId::from_bytes([76; 32]);
    let symbol = GraphSymbol::new(
        symbol_id,
        revision.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("main".to_owned())?,
            range,
            range,
        )?,
    );
    let edge = GraphEdge::new(
        GraphEndpoint::File(revision.path().clone()),
        GraphEndpoint::Symbol(symbol_id),
        SyntaxRelationKind::Exports,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        EvidenceRef::new(revision.clone(), range),
    );
    let mut files = vec![revision];
    for (extra_path, extra_hash) in extra_files {
        files.push(a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(extra_path.to_vec())?,
            ContentHash::from_bytes(*extra_hash),
        ));
    }
    let graph = LinkedGraph::new(snapshot_id, files, vec![symbol], vec![edge], Vec::new())?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(1_000)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(1_000)?,
                degree_contribution: 0,
                centrality_contribution: 1_000,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        )],
    )?;
    let modules = support::module_projection(&graph, &ranking, &[])?;
    Ok(IndexPublication::new(graph, ranking, Vec::new(), modules)?)
}

fn multi_module_publication(
    snapshot_id: SnapshotId,
    hashes: [[u8; 32]; 3],
) -> Result<IndexPublication, Box<dyn std::error::Error>> {
    let paths = [
        b"packages/a/lib.rs".as_slice(),
        b"packages/b/lib.rs".as_slice(),
        b"packages/c/lib.rs".as_slice(),
    ];
    let names = ["module_a", "module_b", "module_c"];
    let mut revisions = Vec::new();
    let mut symbols = Vec::new();
    let mut ranks = Vec::new();
    let mut modules = Vec::new();
    let mut memberships = Vec::new();
    for index in 0..3 {
        let revision = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(paths[index].to_vec())?,
            ContentHash::from_bytes(hashes[index]),
        );
        let symbol_id = SymbolId::from_bytes([hashes[index][0]; 32]);
        let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
        symbols.push(GraphSymbol::new(
            symbol_id,
            revision.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string(names[index].to_owned())?,
                range,
                range,
            )?,
        ));
        ranks.push(SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(u64::try_from(3 - index)? * 1_000)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(u16::try_from(3 - index)? * 1_000)?,
                degree_contribution: 0,
                centrality_contribution: u32::try_from(3 - index)? * 1_000,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        ));
        let module_id = ModuleId::from_bytes([u8::try_from(201 + index)?; 32]);
        modules.push(RepositoryModule::new(
            module_id,
            ModuleKind::PathBoundary,
            Some(ModuleRoot::Directory(RepositoryPath::try_from_bytes(
                format!("packages/{}", ['a', 'b', 'c'][index]).into_bytes(),
            )?)),
            Vec::new(),
            ModuleSymbolSet::new(vec![symbol_id], false)?,
            ModuleSymbolSet::empty(),
            ModuleSymbolSet::empty(),
        )?);
        memberships.push(ModuleMembership::new(
            module_id,
            symbol_id,
            ModuleMembershipEvidence::path(revision.clone()),
        ));
        revisions.push(revision);
    }
    let dependency_edge = GraphEdge::new(
        GraphEndpoint::Symbol(SymbolId::from_bytes([hashes[1][0]; 32])),
        GraphEndpoint::Symbol(SymbolId::from_bytes([hashes[0][0]; 32])),
        SyntaxRelationKind::Calls,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        EvidenceRef::new(
            revisions[1].clone(),
            SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?,
        ),
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        revisions,
        symbols,
        vec![dependency_edge],
        Vec::new(),
    )?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
    let repository_card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![
            ModuleId::from_bytes([201; 32]),
            ModuleId::from_bytes([202; 32]),
            ModuleId::from_bytes([203; 32]),
        ],
        vec![IndexLanguage::Rust],
        ModuleSymbolSet::empty(),
        3,
        3,
    )?;
    let projection = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        modules,
        memberships,
        repository_card,
    )?;
    Ok(IndexPublication::new(
        graph,
        ranking,
        Vec::new(),
        projection,
    )?)
}

fn multi_module_card_batch(
    published: &PublishedIndex,
) -> Result<VerifiedModuleCardBatch, Box<dyn std::error::Error>> {
    let mut candidates = Vec::new();
    let mut resolved = Vec::new();
    for (index, module) in published
        .publication()
        .modules()
        .modules()
        .iter()
        .enumerate()
    {
        let membership = published
            .publication()
            .modules()
            .memberships()
            .iter()
            .find(|membership| module.id() == membership.module_id())
            .ok_or("module membership is missing")?;
        let symbol = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == membership.symbol_id())
            .ok_or("module symbol is missing")?;
        let revision = symbol.revision().clone();
        let evidence_id = ModuleCardEvidenceId::for_file_revision_v1(&revision);
        let card_id = ModuleCardId::from_bytes([u8::try_from(151 + index)?; 32]);
        let claim_id = ModuleCardClaimId::from_bytes([u8::try_from(161 + index)?; 32]);
        let proposal = ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                card_id,
                module.id(),
                published.run().snapshot_id(),
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::from_basis_points(8_000)?,
            ),
            vec![ProposedModuleCardField::new(
                ModuleCardField::Paths,
                vec![String::from_utf8(revision.path().as_bytes().to_vec())?],
                vec![evidence_id],
            )?],
            512,
        )?;
        let claim = ModuleClaimProposal::new(
            ModuleClaimEnvelope::new(
                claim_id,
                card_id,
                module.id(),
                published.run().snapshot_id(),
                ModuleCardField::Paths,
                0,
                Confidence::from_basis_points(8_000)?,
            ),
            ModuleClaimPolarity::Affirms,
            ModuleClaimPredicate::Path(revision.path().clone()),
            vec![evidence_id],
        )?;
        candidates.push(ModuleCardVerificationCandidate::new(proposal, vec![claim])?);
        resolved.push(ResolvedModuleCardEvidence::File {
            id: evidence_id,
            revision,
        });
    }
    let evidence = ResolvedModuleCardEvidenceSet::new(published.run().snapshot_id(), resolved)?;
    Ok(ModuleCardVerifier::verify(
        published, candidates, &evidence,
    )?)
}

fn verified_file_card_batch(
    published: &PublishedIndex,
) -> Result<VerifiedModuleCardBatch, Box<dyn std::error::Error>> {
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path().as_bytes() == b"src/lib.rs")
        .ok_or("published fixture file is missing")?
        .clone();
    let evidence_id = ModuleCardEvidenceId::for_file_revision_v1(&revision);
    let module = published
        .publication()
        .modules()
        .modules()
        .first()
        .ok_or("published fixture module is missing")?;
    let card_id = ModuleCardId::from_bytes([94; 32]);
    let proposal = ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::from_basis_points(8_000)?,
        ),
        vec![ProposedModuleCardField::new(
            ModuleCardField::Paths,
            vec!["src/lib.rs".to_owned()],
            vec![evidence_id],
        )?],
        512,
    )?;
    let claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([95; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::Paths,
            0,
            Confidence::from_basis_points(8_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Path(revision.path().clone()),
        vec![evidence_id],
    )?;
    let candidate = ModuleCardVerificationCandidate::new(proposal, vec![claim])?;
    let evidence = ResolvedModuleCardEvidenceSet::new(
        published.run().snapshot_id(),
        vec![ResolvedModuleCardEvidence::File {
            id: evidence_id,
            revision,
        }],
    )?;
    Ok(ModuleCardVerifier::verify(
        published,
        vec![candidate],
        &evidence,
    )?)
}

fn verified_card_batch(
    published: &PublishedIndex,
) -> Result<VerifiedModuleCardBatch, Box<dyn std::error::Error>> {
    let edge = published
        .publication()
        .graph()
        .edges()
        .first()
        .ok_or("published fixture edge is missing")?
        .clone();
    let evidence_id = ModuleCardEvidenceId::for_graph_edge_v1(&edge);
    let module = published
        .publication()
        .modules()
        .modules()
        .first()
        .ok_or("published fixture module is missing")?;
    let card_id = ModuleCardId::from_bytes([77; 32]);
    let proposal = ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::from_basis_points(8_000)?,
        ),
        vec![
            ProposedModuleCardField::new(
                ModuleCardField::Purpose,
                vec!["keeps policy centralized".to_owned()],
                vec![evidence_id],
            )?,
            ProposedModuleCardField::new(
                ModuleCardField::PublicSurface,
                vec!["exports main".to_owned()],
                vec![evidence_id],
            )?,
        ],
        512,
    )?;
    let relation_claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([78; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::PublicSurface,
            0,
            Confidence::from_basis_points(7_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Relation {
            source: edge.source().clone(),
            target: edge.target().clone(),
            kind: edge.kind(),
        },
        vec![evidence_id],
    )?;
    let intent_claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([79; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::Purpose,
            0,
            Confidence::from_basis_points(5_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::ArchitecturalIntent(
            a3_domain::ModuleClaimStatement::try_from_string(
                "keeps policy centralized".to_owned(),
            )?,
        ),
        Vec::new(),
    )?;
    let candidate =
        ModuleCardVerificationCandidate::new(proposal, vec![relation_claim, intent_claim])?;
    let evidence = ResolvedModuleCardEvidenceSet::new(
        published.run().snapshot_id(),
        vec![ResolvedModuleCardEvidence::GraphEdge {
            id: evidence_id,
            edge,
        }],
    )?;
    Ok(ModuleCardVerifier::verify(
        published,
        vec![candidate],
        &evidence,
    )?)
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
        "module_cards" => "SELECT COUNT(*) FROM module_cards",
        "module_card_fields" => "SELECT COUNT(*) FROM module_card_fields",
        "module_card_field_values" => "SELECT COUNT(*) FROM module_card_field_values",
        "module_card_field_evidence" => "SELECT COUNT(*) FROM module_card_field_evidence",
        "evidence_refs" => "SELECT COUNT(*) FROM evidence_refs",
        "claims" => "SELECT COUNT(*) FROM claims",
        "claim_evidence" => "SELECT COUNT(*) FROM claim_evidence",
        "claim_relations" => "SELECT COUNT(*) FROM claim_relations",
        "evidence_invalidations" => "SELECT COUNT(*) FROM evidence_invalidations",
        "module_remap_queue" => "SELECT COUNT(*) FROM module_remap_queue",
        "card_fts" => "SELECT COUNT(*) FROM card_fts",
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

async fn read_single_text(path: &Path, sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    let value = row.get(0)?;
    if rows.next().await?.is_some() {
        return Err(io::Error::other("expected exactly one text row").into());
    }
    Ok(value)
}

async fn read_lexical_card_count(
    path: &Path,
    run_id: IndexRunId,
) -> Result<i64, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query(
            "SELECT card_count FROM lexical_search_projections WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    let count = row.get(0)?;
    if rows.next().await?.is_some() {
        return Err(io::Error::other("duplicate lexical projection row").into());
    }
    Ok(count)
}

async fn read_claim_classification(
    path: &Path,
    run_id: IndexRunId,
) -> Result<Vec<(String, String, i64)>, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query(
            "SELECT predicate_kind, claim_kind, confidence FROM claims\n\
             WHERE source_index_run_id = ?1 AND status = 'active'\n\
             ORDER BY predicate_kind",
            [run_id.as_bytes().to_vec()],
        )
        .await?;
    let mut claims = Vec::new();
    while let Some(row) = rows.next().await? {
        claims.push((row.get(0)?, row.get(1)?, row.get(2)?));
    }
    Ok(claims)
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
