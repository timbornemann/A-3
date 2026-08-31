//! Contract tests for per-worktree paths, migrations, identity binding, and safe rejection.

mod support;

use a3_application::{
    DeepMapEventResult, DeepMapJournalEvent, DeepMapModelDescriptor, DeepMapPhase,
    DeepMapPlanTargetReference, DeepMapPublicationAnchor, DeepMapPublicationResult,
    DeepMapRunJournalFailure, DeepMapRunJournalStore, DeepMapRunStart, DeepMapSafeAction,
    DeepMapTargetKind, KnowledgeStore, KnowledgeStoreFailure, RecentProjectLimit,
};
use a3_domain::{
    CanonicalDirectory, Centrality, ContentHash, DeepMapEventSequence, DeepMapMode, DeepMapPlanner,
    DeepMapRunId, DeepMapRunState, DeepMapRunTimestamp, ExploreBudget, ExplorePlan, FileRevision,
    GitHead, GitReferenceName, GraphSymbol, IndexLanguage, IndexPublication, IndexRunId,
    IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkedGraph, LocalSymbolId, ModelProfileId,
    ModelProfileReference, ModelProfileVersion, ModuleCoverageSnapshot, ModuleId, ModuleKind,
    ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot,
    ModuleSymbolSet, ParsedSymbol, ProjectIdentity, RankProjection, RankScore,
    RankingPolicyVersion, RepositoryCard, RepositoryId, RepositoryIdentity, RepositoryModule,
    RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName,
    SymbolRank, SymbolRankSignals, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{
    KnowledgeDatabase, KnowledgeOpenError, KnowledgeSchemaVersion, LibsqlKnowledgeStore,
    ProjectStorageLayoutError, StorageLayout,
};
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use support::TempDirectory;

// libSQL's Windows native test runtime is not safe when separate local database
// fixtures are opened and torn down concurrently inside one process.
static KNOWLEDGE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn empty_knowledge_database_migrates_binds_and_reopens() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([1; 32], [11; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;

        let first = KnowledgeDatabase::open(&project_layout, &fixture.project).await?;
        assert_eq!(first.path(), project_layout.knowledge_path());
        assert_eq!(first.schema_version(), KnowledgeSchemaVersion::CURRENT);
        let verification = first.verify().await?;
        assert_eq!(
            verification.schema_version(),
            KnowledgeSchemaVersion::CURRENT
        );
        assert_eq!(
            verification.repository_id(),
            fixture.project.repository().id()
        );
        assert_eq!(verification.worktree_id(), fixture.project.worktree().id());
        drop(first);

        let reopened = KnowledgeDatabase::open(&project_layout, &fixture.project).await?;
        assert_eq!(reopened.path(), project_layout.knowledge_path());
        assert_eq!(read_identity_count(reopened.path()).await?, 1);
        assert_eq!(
            read_user_version(reopened.path()).await?,
            KnowledgeSchemaVersion::CURRENT.get()
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn deep_map_journal_roundtrips_reconciles_and_rejects_false_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([91; 32], [92; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let anchor = DeepMapPublicationAnchor::new(
            IndexRunId::from_bytes([93; 32]),
            SnapshotId::from_bytes([94; 32]),
        );
        let model = DeepMapModelDescriptor::from_stored_parts(
            ModelProfileReference::new(
                ModelProfileId::from_bytes([95; 32]),
                ModelProfileVersion::V1,
            ),
            "openai".to_owned(),
            "gpt-5.4".to_owned(),
            128_000,
            16_384,
        )?;
        let completed_id = DeepMapRunId::from_bytes([96; 32]);
        store
            .create_run(
                &fixture.project,
                &DeepMapRunStart::new(
                    completed_id,
                    anchor,
                    DeepMapMode::Standard,
                    model.clone(),
                    DeepMapRunTimestamp::new(1_000)?,
                ),
            )
            .await?;
        store
            .append_event(
                &fixture.project,
                completed_id,
                DeepMapJournalEvent::new(
                    DeepMapEventSequence::new(2)?,
                    DeepMapRunTimestamp::new(1_100)?,
                    DeepMapRunState::Running,
                    Some(DeepMapPhase::Planning),
                    Some(DeepMapTargetKind::Project),
                    Some(DeepMapSafeAction::BuildPlan),
                    None,
                    None,
                    None,
                    false,
                    DeepMapEventResult::Pending,
                    None,
                )?,
            )
            .await?;
        store
            .append_event(
                &fixture.project,
                completed_id,
                DeepMapJournalEvent::new(
                    DeepMapEventSequence::new(3)?,
                    DeepMapRunTimestamp::new(1_200)?,
                    DeepMapRunState::Succeeded,
                    Some(DeepMapPhase::Publishing),
                    Some(DeepMapTargetKind::Project),
                    Some(DeepMapSafeAction::PublishCards),
                    None,
                    None,
                    None,
                    true,
                    DeepMapEventResult::AlreadyCurrent,
                    None,
                )?,
            )
            .await?;
        assert_eq!(
            store
                .append_event(
                    &fixture.project,
                    completed_id,
                    DeepMapJournalEvent::new(
                        DeepMapEventSequence::new(4)?,
                        DeepMapRunTimestamp::new(1_300)?,
                        DeepMapRunState::Succeeded,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        false,
                        DeepMapEventResult::Pending,
                        None,
                    )?,
                )
                .await,
            Err(DeepMapRunJournalFailure::Conflict)
        );

        let interrupted_id = DeepMapRunId::from_bytes([97; 32]);
        store
            .create_run(
                &fixture.project,
                &DeepMapRunStart::new(
                    interrupted_id,
                    anchor,
                    DeepMapMode::Fast,
                    model,
                    DeepMapRunTimestamp::new(2_000)?,
                ),
            )
            .await?;
        store
            .mark_details_incomplete(&fixture.project, interrupted_id)
            .await?;
        drop(store);

        let reopened = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        assert_eq!(
            reopened
                .reconcile_interrupted(&fixture.project, DeepMapRunTimestamp::new(2_100)?,)
                .await?,
            1
        );
        assert_eq!(
            reopened
                .reconcile_interrupted(&fixture.project, DeepMapRunTimestamp::new(2_200)?,)
                .await?,
            0
        );
        let runs = reopened.list_runs(&fixture.project, None).await?;
        assert_eq!(runs.runs().len(), 2);
        assert_eq!(runs.runs()[0].state(), DeepMapRunState::Interrupted);
        assert!(runs.runs()[0].details_incomplete());
        assert_eq!(runs.runs()[1].state(), DeepMapRunState::Succeeded);
        assert_eq!(
            runs.runs()[1].publication_result(),
            Some(DeepMapPublicationResult::AlreadyCurrent)
        );

        let entries = reopened
            .list_entries(&fixture.project, completed_id, None)
            .await?;
        assert_eq!(
            entries
                .entries()
                .iter()
                .map(|event| event.sequence().get())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let detail = reopened
            .load_entry(
                &fixture.project,
                completed_id,
                DeepMapEventSequence::new(3)?,
            )
            .await?
            .ok_or("completed journal detail is missing")?;
        assert_eq!(detail.event().result(), DeepMapEventResult::AlreadyCurrent);
        assert_eq!(detail.run().start().model().provider_id(), "openai");
        assert!(detail.step().is_none());
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn deep_map_v29_plan_details_roundtrip_as_safe_references_and_canonical_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([105; 32], [106; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let plan = deep_map_plan_fixture()?;
        let run_id = DeepMapRunId::from_bytes([107; 32]);
        store
            .create_run(&fixture.project, &deep_map_run_start(run_id, &plan, 1_000)?)
            .await?;

        store.record_plan(&fixture.project, run_id, &plan).await?;

        let page = store
            .list_run_modules(&fixture.project, run_id, None)
            .await?;
        assert_eq!(page.modules().len(), 1);
        assert_eq!(
            page.modules()[0].planned_steps(),
            u64::try_from(plan.steps().len())?
        );
        let steps = store
            .list_module_steps(
                &fixture.project,
                run_id,
                page.modules()[0].module_id(),
                None,
            )
            .await?;
        assert_eq!(steps.steps().len(), plan.steps().len());
        for step in steps.steps() {
            assert!(matches!(
                step.target_reference(),
                Some(
                    DeepMapPlanTargetReference::Module(_)
                        | DeepMapPlanTargetReference::FileEvidence(_)
                        | DeepMapPlanTargetReference::Symbol(_)
                )
            ));
            let fields = step.coverage_fields().ok_or("V29 Card fields missing")?;
            assert!(!fields.is_empty());
            assert!(fields.windows(2).all(|pair| pair[0] < pair[1]));
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn deep_map_v29_plan_persistence_rolls_back_all_rows_on_a_mid_plan_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([108; 32], [109; 32])?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let plan = deep_map_plan_fixture()?;
        assert!(plan.steps().len() >= 2);
        let run_id = DeepMapRunId::from_bytes([110; 32]);
        store
            .create_run(&fixture.project, &deep_map_run_start(run_id, &plan, 2_000)?)
            .await?;
        let knowledge_path = fixture
            .layout
            .prepare_project(fixture.project.worktree())?
            .knowledge_path()
            .to_path_buf();
        mutate_knowledge(
            &knowledge_path,
            "CREATE TRIGGER reject_second_safe_plan_target\n\
             BEFORE INSERT ON deep_map_step_targets\n\
             WHEN NEW.step_position = 2\n\
             BEGIN SELECT RAISE(ABORT, 'test rollback'); END",
        )
        .await?;

        assert_eq!(
            store.record_plan(&fixture.project, run_id, &plan).await,
            Err(DeepMapRunJournalFailure::Unavailable)
        );
        let run = store
            .load_run(&fixture.project, run_id)
            .await?
            .ok_or("run missing after rollback")?;
        assert_eq!(run.total_steps(), 0);
        assert!(
            store
                .list_run_modules(&fixture.project, run_id, None)
                .await?
                .modules()
                .is_empty()
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn linked_worktrees_receive_distinct_identity_bound_databases()
-> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let common = create_directory(temporary.path().join("common-git"))?;
        let repository_id = RepositoryId::from_bytes([2; 32]);
        let first = project(
            repository_id,
            WorktreeId::from_bytes([21; 32]),
            &common,
            &create_directory(temporary.path().join("primary"))?,
        )?;
        let second = project(
            repository_id,
            WorktreeId::from_bytes([22; 32]),
            &common,
            &create_directory(temporary.path().join("linked"))?,
        )?;
        let first_layout = layout.prepare_project(first.worktree())?;
        let second_layout = layout.prepare_project(second.worktree())?;

        assert_ne!(first_layout.root(), second_layout.root());
        assert_eq!(
            first_layout.knowledge_path(),
            layout
                .root()
                .join("projects")
                .join(first.worktree().id().to_string())
                .join("knowledge.db")
        );
        let first_database = KnowledgeDatabase::open(&first_layout, &first).await?;
        let second_database = KnowledgeDatabase::open(&second_layout, &second).await?;
        assert_ne!(first_database.path(), second_database.path());
        assert_eq!(
            first_database.verify().await?.worktree_id(),
            first.worktree().id()
        );
        assert_eq!(
            second_database.verify().await?.worktree_id(),
            second.worktree().id()
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn knowledge_rejects_a_newer_schema_without_modifying_it() -> Result<(), Box<dyn std::error::Error>>
{
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([3; 32], [31; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        set_user_version(
            project_layout.knowledge_path(),
            KnowledgeSchemaVersion::CURRENT.get() + 1,
        )
        .await?;
        let content_before = fs::read(project_layout.knowledge_path())?;

        assert!(matches!(
            KnowledgeDatabase::open(&project_layout, &fixture.project).await,
            Err(KnowledgeOpenError::NewerSchema { found, supported })
                if found.get() == KnowledgeSchemaVersion::CURRENT.get() + 1
                    && supported == KnowledgeSchemaVersion::CURRENT
        ));
        assert_eq!(
            read_user_version(project_layout.knowledge_path()).await?,
            KnowledgeSchemaVersion::CURRENT.get() + 1
        );
        assert_eq!(fs::read(project_layout.knowledge_path())?, content_before);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn knowledge_rejects_tampered_migration_history() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([4; 32], [41; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        drop(KnowledgeDatabase::open(&project_layout, &fixture.project).await?);
        mutate_knowledge(
            project_layout.knowledge_path(),
            "UPDATE schema_migrations SET checksum = zeroblob(32) WHERE version = 1",
        )
        .await?;

        assert!(matches!(
            KnowledgeDatabase::open(&project_layout, &fixture.project).await,
            Err(KnowledgeOpenError::MigrationHistoryMismatch { version })
                if version == KnowledgeSchemaVersion::new(1)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn knowledge_rejects_a_persisted_identity_conflict() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([5; 32], [51; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        drop(KnowledgeDatabase::open(&project_layout, &fixture.project).await?);
        mutate_knowledge(
            project_layout.knowledge_path(),
            "UPDATE worktree_storage_identity SET repository_id = zeroblob(32)",
        )
        .await?;

        assert!(matches!(
            KnowledgeDatabase::open(&project_layout, &fixture.project).await,
            Err(KnowledgeOpenError::IdentityConflict)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn knowledge_rejects_non_database_content_as_corrupt() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([6; 32], [61; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        fs::write(project_layout.knowledge_path(), b"this is not a database")?;

        assert!(matches!(
            KnowledgeDatabase::open(&project_layout, &fixture.project).await,
            Err(KnowledgeOpenError::CorruptDatabase)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn invalid_knowledge_target_prevents_catalog_recency() -> Result<(), Box<dyn std::error::Error>> {
    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([7; 32], [71; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        fs::create_dir(project_layout.knowledge_path())?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;

        assert_eq!(
            store.record_opened_project(&fixture.project).await,
            Err(KnowledgeStoreFailure::InvalidStoredData)
        );
        assert!(
            store
                .list_recent_projects(RecentProjectLimit::DEFAULT)
                .await?
                .is_empty()
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn project_storage_rejects_app_data_inside_the_worktree() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = TempDirectory::new()?;
    let worktree_root = create_directory(temporary.path().join("worktree"))?;
    let layout = StorageLayout::prepare(worktree_root.as_path().join(".a3-data"))?;
    let common = create_directory(temporary.path().join("common-git"))?;
    let project = project(
        RepositoryId::from_bytes([8; 32]),
        WorktreeId::from_bytes([81; 32]),
        &common,
        &worktree_root,
    )?;

    assert!(matches!(
        layout.prepare_project(project.worktree()),
        Err(ProjectStorageLayoutError::StorageInsideWorktree { .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn knowledge_rejects_a_symlink_outside_private_storage() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let _test_lock = lock_knowledge_test()?;
    run_knowledge_test(async {
        let fixture = ProjectFixture::new([9; 32], [91; 32])?;
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let outside = fixture._temporary.path().join("outside.db");
        fs::write(&outside, b"outside")?;
        symlink(outside, project_layout.knowledge_path())?;

        assert!(matches!(
            KnowledgeDatabase::open(&project_layout, &fixture.project).await,
            Err(KnowledgeOpenError::Layout(
                ProjectStorageLayoutError::SymbolicLink { .. }
            ))
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

fn deep_map_run_start(
    run_id: DeepMapRunId,
    plan: &ExplorePlan,
    created_at: i64,
) -> Result<DeepMapRunStart, Box<dyn std::error::Error>> {
    Ok(DeepMapRunStart::new(
        run_id,
        DeepMapPublicationAnchor::new(plan.index_run_id(), plan.snapshot_id()),
        DeepMapMode::Standard,
        DeepMapModelDescriptor::from_stored_parts(
            ModelProfileReference::new(
                ModelProfileId::from_bytes([111; 32]),
                ModelProfileVersion::V1,
            ),
            "local".to_owned(),
            "mapper".to_owned(),
            32_000,
            4_096,
        )?,
        DeepMapRunTimestamp::new(created_at)?,
    ))
}

fn deep_map_plan_fixture() -> Result<ExplorePlan, Box<dyn std::error::Error>> {
    let published = deep_map_published_fixture()?;
    let coverage = ModuleCoverageSnapshot::empty(
        published.run().snapshot_id(),
        a3_domain::ModuleCardSchemaVersion::V1,
    );
    Ok(DeepMapPlanner::v1().plan(&published, &coverage, ExploreBudget::DEFAULT)?)
}

fn deep_map_published_fixture() -> Result<a3_domain::PublishedIndex, Box<dyn std::error::Error>> {
    let snapshot_id = SnapshotId::from_bytes([112; 32]);
    let manifest = deep_map_revision("Cargo.toml", 113)?;
    let source = deep_map_revision("src/lib.rs", 114)?;
    let symbol_id = SymbolId::from_bytes([115; 32]);
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let symbol = GraphSymbol::new(
        symbol_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("main".to_owned())?,
            range,
            range,
        )?,
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![manifest.clone(), source.clone()],
        vec![symbol],
        Vec::new(),
        Vec::new(),
    )?;
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
    let module_id = ModuleId::from_bytes([116; 32]);
    let featured = ModuleSymbolSet::new(vec![symbol_id], false)?;
    let module = RepositoryModule::new(
        module_id,
        ModuleKind::ManifestBoundary,
        Some(ModuleRoot::Repository),
        vec![manifest.clone()],
        featured.clone(),
        featured.clone(),
        ModuleSymbolSet::empty(),
    )?;
    let membership = ModuleMembership::new(
        module_id,
        symbol_id,
        ModuleMembershipEvidence::manifest(source, manifest.clone()),
    );
    let card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module_id],
        vec![IndexLanguage::Rust],
        featured,
        2,
        1,
    )?;
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module],
        vec![membership],
        card,
    )?;
    let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([117; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(a3_domain::PublishedIndex::new(run, publication)?)
}

fn deep_map_revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
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
        let project = project(
            RepositoryId::from_bytes(repository_bytes),
            WorktreeId::from_bytes(worktree_bytes),
            &common,
            &root,
        )?;
        Ok(Self {
            _temporary: temporary,
            layout,
            project,
        })
    }
}

fn project(
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    common_directory: &CanonicalDirectory,
    worktree_root: &CanonicalDirectory,
) -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository_id, common_directory.clone(), None),
        WorktreeIdentity::new(
            worktree_id,
            WorktreeAnchorId::from_bytes(*worktree_id.as_bytes()),
            repository_id,
            worktree_root.clone(),
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn create_directory(
    path: impl AsRef<Path>,
) -> Result<CanonicalDirectory, Box<dyn std::error::Error>> {
    fs::create_dir(path.as_ref())?;
    Ok(CanonicalDirectory::from_canonicalized(fs::canonicalize(
        path.as_ref(),
    )?)?)
}

fn lock_knowledge_test() -> Result<MutexGuard<'static, ()>, Box<dyn std::error::Error>> {
    KNOWLEDGE_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error>::from(io::Error::other("knowledge test lock was poisoned"))
    })
}

fn run_knowledge_test<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread
        .name()
        .ok_or_else(|| io::Error::other("knowledge contract test has no harness thread name"))?;
    #[cfg(windows)]
    if std::env::var_os("A3_KNOWLEDGE_CONTRACT_ISOLATED_TEST").as_deref()
        != Some(std::ffi::OsStr::new(test_name))
    {
        const MAX_NATIVE_ATTEMPTS: u8 = 3;
        const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;
        let success_marker = knowledge_contract_success_marker(test_name);
        for attempt in 1..=MAX_NATIVE_ATTEMPTS {
            remove_knowledge_contract_success_marker(&success_marker)?;
            let mut child = std::process::Command::new(std::env::current_exe()?)
                .arg(test_name)
                .arg("--exact")
                .arg("--test-threads=1")
                .env("A3_KNOWLEDGE_CONTRACT_ISOLATED_TEST", test_name)
                .env("A3_LIBSQL_RETAIN_TEMP_DIRECTORY", "1")
                .env("A3_KNOWLEDGE_CONTRACT_SUCCESS_MARKER", &success_marker)
                .spawn()?;
            let child_id = child.id();
            let status = child.wait()?;
            cleanup_knowledge_contract_workspaces(child_id)?;
            let completed = success_marker.is_file();
            remove_knowledge_contract_success_marker(&success_marker)?;
            if completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < MAX_NATIVE_ATTEMPTS {
                continue;
            }
            return Err(io::Error::other(format!(
                "isolated knowledge contract test {test_name} failed on attempt {attempt} with {status} before completion evidence"
            ))
            .into());
        }
        return Err(io::Error::other(format!(
            "isolated knowledge contract test {test_name} exhausted its native retry bound"
        ))
        .into());
    }

    let result = futures::executor::block_on(future);
    #[cfg(windows)]
    match result {
        Ok(()) => {
            let marker = std::env::var_os("A3_KNOWLEDGE_CONTRACT_SUCCESS_MARKER")
                .ok_or_else(|| io::Error::other("knowledge contract success marker is missing"))?;
            fs::write(marker, b"complete")?;
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("knowledge contract failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    result
}

#[cfg(windows)]
fn knowledge_contract_success_marker(test_name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "a3-storage-knowledge-parent-{}-{test_name}.complete",
        std::process::id()
    ))
}

#[cfg(windows)]
fn remove_knowledge_contract_success_marker(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn cleanup_knowledge_contract_workspaces(child_id: u32) -> io::Result<()> {
    let temporary_root = std::env::temp_dir();
    let expected_prefix = format!("a3-storage-test-{child_id}-");
    for entry in fs::read_dir(&temporary_root)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(&expected_prefix)
        {
            continue;
        }
        let target = entry.path();
        if target.parent() != Some(temporary_root.as_path()) {
            return Err(io::Error::other(
                "knowledge contract workspace escaped the temporary root",
            ));
        }
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

async fn mutate_knowledge(path: &Path, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    connection.execute(sql, ()).await?;
    Ok(())
}

async fn set_user_version(path: &Path, version: u32) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .await?;
    Ok(())
}

async fn read_user_version(path: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection.query("PRAGMA user_version", ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    let version: i64 = row.get(0)?;
    Ok(u32::try_from(version)?)
}

async fn read_identity_count(path: &Path) -> Result<i64, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(path).build().await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query("SELECT COUNT(*) FROM worktree_storage_identity", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    Ok(row.get(0)?)
}
