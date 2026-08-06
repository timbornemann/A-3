//! Reproducible manual R1/R2/R6 retrieval and module-load baseline; excluded from default tests.

mod support;

use a3_application::{
    AgentContextCompileInput, AgentContextCompiler, CompileTaskLens, ContextCompileControl,
    ContextCompilePhase, IndexPersistenceControl, IndexPersistenceControlError,
    KnowledgeIndexStore, KnowledgeSearchControl, KnowledgeSearchStore,
    ModuleCardVerificationControl, ModuleCardVerificationControlError, PublishVerifiedModuleCards,
    TaskLensControl, TaskLensControlError,
};
use a3_context::DeterministicAgentContextCompiler;
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, CanonicalDirectory,
    Centrality, Confidence, ContentHash, ExactSearchPageSize, ExactSearchQuery, ExactSearchTerm,
    ExpectedTaskEvidence, FileRevision, GitHead, GitReferenceName, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, GraphSymbol, IndexPublication, IndexRunId, IndexRunStart,
    IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion, LexicalSearchPageSize,
    LexicalSearchQuery, LexicalSearchTarget, LexicalSearchTerm, LinkedGraph, LocalSymbolId,
    MapperProfileVersion, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding,
    ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
    ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal,
    ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleCardVerificationCandidate,
    ModuleCardVerifier, ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleClaimProposal, ParsedSymbol, ProjectIdentity, ProposedModuleCardField, PublishedIndex,
    RankProjection, RankScore, RankingPolicyVersion, RepositoryId, RepositoryIdentity,
    RepositoryPath, ResolvedModuleCardEvidence, ResolvedModuleCardEvidenceSet, Snapshot,
    SnapshotChange, SnapshotChangeKind, SnapshotId, SourcePosition, SourceRange, SymbolId,
    SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, TaskId, TaskLedger, TaskLedgerTimestamp,
    TaskLensSeedSet, TaskLensSeedText, TaskLensTarget, TaskLensTokenBudget, TaskStepDefinition,
    TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId, VerifiedModuleCardBatch, WorktreeAnchorId,
    WorktreeGeneration, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use futures::executor::block_on;
use std::error::Error;
use std::fs;
use std::time::{Duration, Instant};
use support::TempDirectory;

const STRUCTURAL_LINES: usize = 100_000;
const SYMBOL_COUNT: usize = STRUCTURAL_LINES / 2;
const BASELINE_SAMPLES: usize = 5;
const EXACT_SAMPLES: usize = 30;
const LEXICAL_SAMPLES: usize = 30;
const TASK_LENS_SAMPLES: usize = 30;
const CONTEXT_COMPILE_SAMPLES: usize = 30;
const EXACT_P95_TARGET: Duration = Duration::from_millis(100);
const LEXICAL_P95_TARGET: Duration = Duration::from_millis(100);
const TASK_LENS_P95_TARGET: Duration = Duration::from_millis(300);
const CONTEXT_COMPILE_P95_TARGET: Duration = Duration::from_millis(300);

#[derive(Debug)]
struct SilentControl;

impl KnowledgeSearchControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl IndexPersistenceControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

impl TaskLensControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: a3_domain::Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl ContextCompileControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_phase(&self, _phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl ModuleCardVerificationControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        Ok(())
    }
}

#[test]
#[ignore = "manual 100,000-structural-line exact/FTS/Task-Lens/Context-Compile P95 baseline"]
fn exact_symbol_search_meets_the_100_millisecond_p95_target() -> Result<(), Box<dyn Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let app_data = temporary.path().join("app-data");
        let worktree = temporary.path().join("worktree");
        let common = temporary.path().join("common-git");
        fs::create_dir(&worktree)?;
        fs::create_dir(&common)?;
        let layout = StorageLayout::prepare(app_data)?;
        let project = project(&worktree, &common)?;
        let store = LibsqlKnowledgeStore::open(&layout).await?;
        let (snapshot, publication) = fixture(project.worktree().id())?;
        store.append_snapshot(&project, &snapshot).await?;
        let run = store
            .start_index_run(
                &project,
                IndexRunStart::new(
                    IndexRunId::from_bytes([9; 32]),
                    snapshot.id(),
                    RankingPolicyVersion::v1(),
                ),
            )
            .await?;
        store
            .publish_index(&project, run.id(), &publication, &SilentControl)
            .await?;
        let published_for_card = store
            .latest_published_index(&project, &SilentControl)
            .await?
            .ok_or("published benchmark index is missing before card verification")?;
        let card_batch = verified_card_batch(&published_for_card)?;
        PublishVerifiedModuleCards::new(&store)
            .execute(&project, &card_batch, &SilentControl)
            .await?;

        let target = format!("function_{:05}", SYMBOL_COUNT - 1);
        let query = ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(target.clone())?);
        let lexical_query = LexicalSearchQuery::new(LexicalSearchTerm::try_from_string(format!(
            "function_{:04}x",
            (SYMBOL_COUNT - 1) / 10
        ))?);
        let lens_seeds = TaskLensSeedSet::new(
            TaskLensSeedText::try_from_string(target.clone())?,
            TaskLensSeedText::try_from_string(target.clone())?,
            Vec::new(),
        )?;
        let context_input = context_input(&project, &target)?;
        let _warm_exact = store
            .search_exact(
                &project,
                &query,
                ExactSearchPageSize::DEFAULT,
                None,
                &SilentControl,
            )
            .await?;
        let _warm_lexical = store
            .search_lexical(
                &project,
                &lexical_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &SilentControl,
            )
            .await?;
        let _warm_baseline = store
            .latest_published_index(&project, &SilentControl)
            .await?
            .ok_or("published benchmark index is missing")?;
        let _warm_task_lens = CompileTaskLens::new(&store, &store, &store)
            .execute(
                &project,
                lens_seeds.clone(),
                TaskLensTokenBudget::DEFAULT,
                &SilentControl,
            )
            .await?;
        let _warm_context =
            DeterministicAgentContextCompiler::new(CompileTaskLens::new(&store, &store, &store))
                .compile(&context_input, &SilentControl)
                .await?;

        let mut baseline_samples = Vec::with_capacity(BASELINE_SAMPLES);
        for _ in 0..BASELINE_SAMPLES {
            let started = Instant::now();
            let published = store
                .latest_published_index(&project, &SilentControl)
                .await?
                .ok_or("published benchmark index is missing")?;
            let found = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .any(|symbol| symbol.parsed().name().as_str() == target);
            baseline_samples.push(started.elapsed());
            assert!(found);
        }

        let mut exact_samples = Vec::with_capacity(EXACT_SAMPLES);
        for _ in 0..EXACT_SAMPLES {
            let started = Instant::now();
            let page = store
                .search_exact(
                    &project,
                    &query,
                    ExactSearchPageSize::DEFAULT,
                    None,
                    &SilentControl,
                )
                .await?;
            exact_samples.push(started.elapsed());
            assert_eq!(page.hits().len(), 1);
        }
        let mut lexical_samples = Vec::with_capacity(LEXICAL_SAMPLES);
        for _ in 0..LEXICAL_SAMPLES {
            let started = Instant::now();
            let page = store
                .search_lexical(
                    &project,
                    &lexical_query,
                    LexicalSearchPageSize::DEFAULT,
                    None,
                    &SilentControl,
                )
                .await?;
            lexical_samples.push(started.elapsed());
            assert!(page.hits().iter().any(|hit| matches!(
                hit.target(),
                LexicalSearchTarget::Symbol(symbol)
                    if symbol.symbol().parsed().name().as_str() == target
            )));
        }
        let mut task_lens_samples = Vec::with_capacity(TASK_LENS_SAMPLES);
        for _ in 0..TASK_LENS_SAMPLES {
            let started = Instant::now();
            let lens = CompileTaskLens::new(&store, &store, &store)
                .execute(
                    &project,
                    lens_seeds.clone(),
                    TaskLensTokenBudget::DEFAULT,
                    &SilentControl,
                )
                .await?;
            task_lens_samples.push(started.elapsed());
            assert!(lens.entries().iter().any(|entry| matches!(
                entry.target(),
                TaskLensTarget::Symbol(symbol) if symbol.parsed().name().as_str() == target
            )));
        }
        let mut context_compile_samples = Vec::with_capacity(CONTEXT_COMPILE_SAMPLES);
        for _ in 0..CONTEXT_COMPILE_SAMPLES {
            let started = Instant::now();
            let context = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
                &store, &store, &store,
            ))
            .compile(&context_input, &SilentControl)
            .await?;
            context_compile_samples.push(started.elapsed());
            assert_eq!(context.snapshot_id(), snapshot.id());
            assert!(context.budget_usage().prompt_total() <= 11_879);
        }
        baseline_samples.sort_unstable();
        exact_samples.sort_unstable();
        lexical_samples.sort_unstable();
        task_lens_samples.sort_unstable();
        context_compile_samples.sort_unstable();
        let baseline_p50 = baseline_samples[BASELINE_SAMPLES / 2];
        let baseline_p95 = baseline_samples[percentile_index(BASELINE_SAMPLES)];
        let exact_p50 = exact_samples[EXACT_SAMPLES / 2];
        let exact_p95 = exact_samples[percentile_index(EXACT_SAMPLES)];
        let lexical_p50 = lexical_samples[LEXICAL_SAMPLES / 2];
        let lexical_p95 = lexical_samples[percentile_index(LEXICAL_SAMPLES)];
        let task_lens_p50 = task_lens_samples[TASK_LENS_SAMPLES / 2];
        let task_lens_p95 = task_lens_samples[percentile_index(TASK_LENS_SAMPLES)];
        let context_compile_p50 = context_compile_samples[CONTEXT_COMPILE_SAMPLES / 2];
        let context_compile_p95 =
            context_compile_samples[percentile_index(CONTEXT_COMPILE_SAMPLES)];
        println!(
            "A^3 fast-search baseline: {STRUCTURAL_LINES} structural lines, {SYMBOL_COUNT} symbols and primary module memberships; pre-retrieval full-index-load scan {BASELINE_SAMPLES} samples P50={baseline_p50:?}, P95={baseline_p95:?}; indexed exact retrieval {EXACT_SAMPLES} samples P50={exact_p50:?}, P95={exact_p95:?}; typo-tolerant FTS retrieval {LEXICAL_SAMPLES} samples P50={lexical_p50:?}, P95={lexical_p95:?}; deterministic Task Lens compile {TASK_LENS_SAMPLES} samples P50={task_lens_p50:?}, P95={task_lens_p95:?}; complete Context Compile {CONTEXT_COMPILE_SAMPLES} samples P50={context_compile_p50:?}, P95={context_compile_p95:?}"
        );
        assert!(
            lexical_p95 <= LEXICAL_P95_TARGET,
            "lexical-search P95 {lexical_p95:?} exceeded {LEXICAL_P95_TARGET:?}"
        );
        assert!(
            exact_p95 <= EXACT_P95_TARGET,
            "exact-search P95 {exact_p95:?} exceeded {EXACT_P95_TARGET:?}"
        );
        assert!(
            task_lens_p95 <= TASK_LENS_P95_TARGET,
            "Task Lens P95 {task_lens_p95:?} exceeded {TASK_LENS_P95_TARGET:?}"
        );
        assert!(
            context_compile_p95 <= CONTEXT_COMPILE_P95_TARGET,
            "Context Compile P95 {context_compile_p95:?} exceeded {CONTEXT_COMPILE_P95_TARGET:?}"
        );
        Ok::<(), Box<dyn Error>>(())
    })
}

fn percentile_index(sample_count: usize) -> usize {
    sample_count
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1)
}

fn project(
    root: &std::path::Path,
    common: &std::path::Path,
) -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(root.canonicalize()?)?;
    let common = CanonicalDirectory::from_canonicalized(common.canonicalize()?)?;
    let repository_id = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository_id, common, None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository_id,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn context_input(
    project: &ProjectIdentity,
    target: &str,
) -> Result<AgentContextCompileInput, Box<dyn Error>> {
    let goal = GoalContract::initial(
        TaskId::from_bytes([40; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string(format!("inspect {target}"))?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([41; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "current evidence is packed deterministically".to_owned(),
                )?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            a3_domain::SuccessVerification::try_from_string(
                "verify the bounded Context Pack".to_owned(),
            )?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let step_id = TaskStepId::from_bytes([42; 32]);
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string(format!("inspect {target}"))?,
            TaskStepRationale::try_from_string("ground the next action".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "current symbol evidence".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([43; 32]),
                VerificationMethod::Test,
                VerificationRequirement::try_from_string(
                    "run the context performance fixture".to_owned(),
                )?,
            ),
        )?],
        TaskLedgerTimestamp::from_unix_millis(1)?,
    )?;
    AgentContextCompileInput::new(
        project.clone(),
        goal,
        ledger,
        step_id,
        performance_profile()?,
        Vec::new(),
        Vec::new(),
    )
    .map_err(Into::into)
}

fn performance_profile() -> Result<ModelProfile, Box<dyn Error>> {
    let settings = ModelProfileSettings::new(
        ModelContextLimit::new(16_384)?,
        ModelOutputLimit::new(4_096)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(0)?,
            ModelTopP::from_milli(1_000)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::FormatFieldOnly,
    )?;
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("performance".to_owned())?,
        ModelId::try_from_string("performance-model".to_owned())?,
        settings,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

fn fixture(worktree_id: WorktreeId) -> Result<(Snapshot, IndexPublication), Box<dyn Error>> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/benchmark.rs".to_vec())?,
        ContentHash::from_bytes([4; 32]),
    );
    let snapshot_id = SnapshotId::from_bytes([5; 32]);
    let snapshot = Snapshot::new(
        snapshot_id,
        worktree_id,
        None,
        WorktreeGeneration::new(1)?,
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
        IndexSchemaVersion::v4(),
        vec![LanguageAdapterRevision::new(
            a3_domain::IndexLanguage::Rust,
            LanguageAdapterVersion::try_from_string("performance-rust-1".to_owned())?,
        )],
        vec![SnapshotChange::new(
            revision.path().clone(),
            revision.content_hash(),
            SnapshotChangeKind::Upsert,
        )],
    )?;
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let mut symbols = Vec::with_capacity(SYMBOL_COUNT);
    let mut ranks = Vec::with_capacity(SYMBOL_COUNT);
    for index in 0..SYMBOL_COUNT {
        let symbol_id = symbol_id(index)?;
        symbols.push(GraphSymbol::new(
            symbol_id,
            revision.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(u32::try_from(index)?.saturating_add(1))?,
                SymbolKind::Function,
                SymbolName::try_from_string(format!("function_{index:05}"))?,
                range,
                range,
            )?,
        ));
        ranks.push(SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(0)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(0)?,
                degree_contribution: 0,
                centrality_contribution: 0,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        ));
    }
    let graph = LinkedGraph::new(snapshot_id, vec![revision], symbols, Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
    let modules = support::module_projection(&graph, &ranking, &[])?;
    Ok((
        snapshot,
        IndexPublication::new(graph, ranking, Vec::new(), modules)?,
    ))
}

fn verified_card_batch(
    published: &PublishedIndex,
) -> Result<VerifiedModuleCardBatch, Box<dyn Error>> {
    let module = published
        .publication()
        .modules()
        .modules()
        .first()
        .ok_or("benchmark module is missing")?;
    let symbol = published
        .publication()
        .graph()
        .symbols()
        .last()
        .ok_or("benchmark symbol is missing")?;
    let evidence_id = ModuleCardEvidenceId::for_symbol_v1(symbol);
    let card_id = ModuleCardId::from_bytes([250; 32]);
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
            ModuleCardField::PublicSurface,
            vec!["benchmark public surface".to_owned()],
            vec![evidence_id],
        )?],
        512,
    )?;
    let claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([251; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::PublicSurface,
            0,
            Confidence::from_basis_points(7_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Symbol(symbol.id()),
        vec![evidence_id],
    )?;
    let candidate = ModuleCardVerificationCandidate::new(proposal, vec![claim])?;
    let evidence = ResolvedModuleCardEvidenceSet::new(
        published.run().snapshot_id(),
        vec![ResolvedModuleCardEvidence::Symbol {
            id: evidence_id,
            symbol: symbol.clone(),
        }],
    )?;
    Ok(ModuleCardVerifier::verify(
        published,
        vec![candidate],
        &evidence,
    )?)
}

fn symbol_id(index: usize) -> Result<SymbolId, Box<dyn Error>> {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&u64::try_from(index)?.to_be_bytes());
    Ok(SymbolId::from_bytes(bytes))
}
