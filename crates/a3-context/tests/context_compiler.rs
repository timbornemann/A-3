//! H7 end-to-end Context Compiler contract tests over the real ordered Task Lens.

use a3_application::{
    AgentContextCompileInput, AgentContextCompiler, CompileTaskLens, ContextCompileControl,
    ContextCompileFailure, ContextCompilePhase, KnowledgeSearchControl, KnowledgeSearchFailure,
    KnowledgeSearchFuture, KnowledgeSearchStore, ModelMessageRole, TaskLensClaimLimit,
    TaskLensClaimResult, TaskLensClaimStore, TaskLensClaimStoreFailure, TaskLensClaimStoreFuture,
    TaskLensControl, TaskLensControlError, TaskLensIndexStore, TaskLensIndexStoreFuture,
};
use a3_context::DeterministicAgentContextCompiler;
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentControllerState,
    AgentRun, AgentRunId, AgentRunIdentity, AgentRunMaterializedState, AgentRunTimestamp,
    AgentRunTiming, CanonicalDirectory, Centrality, Confidence, ContentHash, ExactSearchCursor,
    ExactSearchExplanation, ExactSearchHit, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery,
    ExactSearchSymbol, ExactSearchTarget, ExpectedTaskEvidence, FileRevision, GitHead,
    GitReferenceName, GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective,
    GraphSymbol, GraphTraversalResult, IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord,
    IndexRunSequence, IndexRunStatus, LexicalScore, LexicalSearchCursor, LexicalSearchExplanation,
    LexicalSearchHit, LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery, LinkedGraph,
    LocalSymbolId, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding,
    ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
    ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimStatement,
    ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion,
    ModuleProjection, ModuleRoot, ModuleSymbolSet, ParsedSymbol, ProjectIdentity, PublishedIndex,
    QualifiedSymbolName, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
    RepositoryId, RepositoryIdentity, RepositoryModule, RepositoryPath, ResolvedModuleCardEvidence,
    RunEventSequence, RunMemoryCheckpoint, SnapshotId, SourcePosition, SourceRange, StepDependency,
    StepVerification, StepVerificationId, StepVerificationOutcome, SymbolId, SymbolKind,
    SymbolName, SymbolRank, SymbolRankSignals, TaskEvidenceId, TaskId, TaskLedger,
    TaskLedgerTimestamp, TaskLensClaim, TaskStepDefinition, TaskStepId, TaskStepOutcome,
    TaskStepRationale, TaskStepResultSummary, TraversalQuery, VerificationFailureSummary,
    VerificationMethod, VerificationRequirement, VerificationSpec, VerificationSpecId,
    VerifiedClaimKind, VerifiedClaimStatus, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use futures::executor::block_on;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};

#[test]
fn context_pack_is_fresh_bounded_and_deterministic() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published.clone(),
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let input = input(fixture.snapshot_id)?;
    let control = RecordingControl::default();
    let compiler =
        DeterministicAgentContextCompiler::new(CompileTaskLens::new(&store, &store, &store));

    let first = block_on(compiler.compile(&input, &control))?;
    let second = block_on(compiler.compile(&input, &RecordingControl::default()))?;

    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.request(), second.request());
    assert_eq!(first.snapshot_id(), fixture.snapshot_id);
    assert_eq!(first.excluded_stale_claims(), 1);
    assert_eq!(first.budget_plan().context_limit(), 16_384);
    assert_eq!(first.budget_plan().output_reserve(), 3_605);
    assert!(first.budget_usage().prompt_total() <= 11_879);

    let messages = first.request().messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), ModelMessageRole::System);
    assert_eq!(messages[1].role(), ModelMessageRole::User);
    let pack = messages[1].content();
    assert!(pack.starts_with("A3_CONTEXT_PACK_V1\n[ANCHOR]\n"));
    assert!(pack.contains("objective=implement H7"));
    assert!(pack.contains("current_step="));
    assert!(pack.contains("outcome=compile context"));
    assert!(pack.contains("verification=test run tests"));
    assert!(pack.contains("[PROJECT_MAP]"));
    assert!(pack.contains("L0 repository"));
    assert!(pack.contains("L1 module"));
    assert!(pack.contains("[CODE_AND_EVIDENCE]"));
    assert!(pack.contains("L2 symbol"));
    assert!(pack.contains("claim id="));
    assert!(pack.contains("kind=hypothesis"));
    assert!(!pack.contains("kind=fact"));
    assert!(pack.contains("excluded_stale_claims=1"));
    assert!(pack.contains("[PACK_STATE]"));

    let counted_prompt = messages.iter().try_fold(0_u32, |total, message| {
        let count = input
            .model_profile()
            .settings()
            .token_counting()
            .count_text(message.content())
            .map_err(|_| TestError("token count failed"))?
            .get();
        total
            .checked_add(count)
            .ok_or(TestError("token sum overflow"))
    })?;
    assert_eq!(first.budget_usage().prompt_total(), counted_prompt);

    let phases = control
        .phases
        .lock()
        .map_err(|_| TestError("phase lock was poisoned"))?;
    assert_eq!(phases.first(), Some(&ContextCompilePhase::Anchor));
    assert!(phases.contains(&ContextCompilePhase::Retrieve));
    assert!(phases.contains(&ContextCompilePhase::Rank));
    assert_eq!(phases.last(), Some(&ContextCompilePhase::Complete));
    drop(phases);
    assert!(
        !calls
            .lock()
            .map_err(|_| TestError("call lock was poisoned"))?
            .is_empty()
    );
    Ok(())
}

#[test]
fn run_memory_reinjects_original_sources_without_duplicate_claims() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published.clone(),
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let (input, memory_digest, run_sequence) =
        input_with_run_memory(&fixture, "completed H7 groundwork")?;
    let compiler =
        DeterministicAgentContextCompiler::new(CompileTaskLens::new(&store, &store, &store));

    let first = block_on(compiler.compile(&input, &RecordingControl::default()))?;
    let repeated = block_on(compiler.compile(&input, &RecordingControl::default()))?;

    assert_eq!(first.digest(), repeated.digest());
    assert_eq!(first.run_memory_digest(), Some(memory_digest));
    assert_eq!(run_sequence, RunEventSequence::new(7)?);
    let pack = first.request().messages()[1].content();
    assert!(pack.contains("[RUN_MEMORY]"));
    assert!(pack.contains("through_event=7"));
    assert!(pack.contains("kind=verification_failed"));
    assert!(pack.contains("outcome=verification_failed"));
    assert!(pack.contains("summary=completed H7 groundwork"));
    assert!(pack.contains("current_status=completed outcome=completed"));
    assert!(pack.contains("evidence=1010101010101010101010101010101010101010101010101010101010101010,1111111111111111111111111111111111111111111111111111111111111111"));
    assert!(pack.contains(
        "memory_claim id=4747474747474747474747474747474747474747474747474747474747474747"
    ));
    assert_eq!(pack.matches("id=4747474747474747").count(), 1);
    let counted_prompt = first
        .request()
        .messages()
        .iter()
        .try_fold(0_u32, |total, message| {
            let count = input
                .model_profile()
                .settings()
                .token_counting()
                .count_text(message.content())
                .map_err(|_| TestError("token count failed"))?
                .get();
            total
                .checked_add(count)
                .ok_or(TestError("token sum overflow"))
        })?;
    assert_eq!(first.budget_usage().prompt_total(), counted_prompt);
    Ok(())
}

#[test]
fn run_memory_secret_candidate_never_reaches_provider_request() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published.clone(),
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let (input, _, _) = input_with_run_memory(&fixture, "AKIAIOSFODNN7EXAMPLE")?;
    let compiler =
        DeterministicAgentContextCompiler::new(CompileTaskLens::new(&store, &store, &store));

    let result = block_on(compiler.compile(&input, &RecordingControl::default()));

    assert!(matches!(
        result,
        Err(ContextCompileFailure::SecretCandidate)
    ));
    Ok(())
}

#[test]
fn cancellation_stops_before_retrieval() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published,
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let control = RecordingControl {
        cancelled: true,
        phases: Mutex::new(Vec::new()),
    };
    let compiler =
        DeterministicAgentContextCompiler::new(CompileTaskLens::new(&store, &store, &store));
    let result = block_on(compiler.compile(&input(fixture.snapshot_id)?, &control));

    assert!(matches!(result, Err(ContextCompileFailure::Cancelled)));
    assert!(
        calls
            .lock()
            .map_err(|_| TestError("call lock was poisoned"))?
            .is_empty()
    );
    Ok(())
}

#[derive(Debug, Default)]
struct RecordingControl {
    cancelled: bool,
    phases: Mutex<Vec<ContextCompilePhase>>,
}

impl ContextCompileControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn report_phase(&self, phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        self.phases
            .lock()
            .map_err(|_| TaskLensControlError::Unavailable)?
            .push(phase);
        Ok(())
    }
}

#[derive(Debug)]
struct StubStore<'a> {
    published: PublishedIndex,
    symbol_id: SymbolId,
    module_id: ModuleId,
    calls: &'a Mutex<Vec<&'static str>>,
}

impl StubStore<'_> {
    fn record(&self, call: &'static str) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
    }

    fn exact_target(&self) -> Result<ExactSearchTarget, KnowledgeSearchFailure> {
        let symbol = self
            .published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == self.symbol_id)
            .cloned()
            .ok_or(KnowledgeSearchFailure::InvalidStoredProjection)?;
        let qualified =
            QualifiedSymbolName::try_from_string(symbol.parsed().name().as_str().to_owned())
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
        Ok(ExactSearchTarget::Symbol(ExactSearchSymbol::new(
            symbol, qualified,
        )))
    }
}

impl TaskLensIndexStore for StubStore<'_> {
    fn load_current_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a> {
        self.record("index");
        let published = Arc::new(self.published.clone());
        Box::pin(async move { Ok(Some(published)) })
    }
}

impl KnowledgeSearchStore for StubStore<'_> {
    fn search_exact<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _query: &'a ExactSearchQuery,
        page_size: ExactSearchPageSize,
        _cursor: Option<&'a ExactSearchCursor>,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
        self.record("exact");
        let result = self.exact_target().and_then(|target| {
            let ExactSearchTarget::Symbol(symbol) = target else {
                return Err(KnowledgeSearchFailure::InvalidStoredProjection);
            };
            let hit = ExactSearchHit::symbol(symbol, ExactSearchExplanation::SymbolNameExact)
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
            ExactSearchPage::new(
                self.published.run().id(),
                self.published.run().snapshot_id(),
                vec![hit],
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
        });
        Box::pin(async move { result })
    }

    fn search_lexical<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _query: &'a LexicalSearchQuery,
        page_size: LexicalSearchPageSize,
        _cursor: Option<&'a LexicalSearchCursor>,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
        self.record("lexical");
        let result = self.exact_target().and_then(|target| {
            let ExactSearchTarget::Symbol(symbol) = target else {
                return Err(KnowledgeSearchFailure::InvalidStoredProjection);
            };
            let score = LexicalScore::new(90_000)
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
            LexicalSearchPage::new(
                self.published.run().id(),
                self.published.run().snapshot_id(),
                vec![LexicalSearchHit::symbol(
                    symbol,
                    LexicalSearchExplanation::SymbolName,
                    score,
                )],
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
        });
        Box::pin(async move { result })
    }

    fn traverse_graph<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        query: &'a TraversalQuery,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
        self.record("graph");
        let result = GraphTraversalResult::new(
            self.published.run().id(),
            self.published.run().snapshot_id(),
            query.clone(),
            Vec::new(),
            false,
        )
        .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection);
        Box::pin(async move { result })
    }
}

impl TaskLensClaimStore for StubStore<'_> {
    fn load_claims<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        _limit: TaskLensClaimLimit,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimStoreFuture<'a> {
        self.record("claims");
        let symbol = published.publication().graph().symbols()[0].clone();
        let result = (|| {
            let stale_fact = TaskLensClaim::new(
                published.run().id(),
                published.run().snapshot_id(),
                ModuleCardClaimId::from_bytes([70; 32]),
                self.module_id,
                ModuleClaimPolarity::Affirms,
                ModuleClaimPredicate::Symbol(symbol.id()),
                VerifiedClaimKind::Fact,
                VerifiedClaimStatus::Stale,
                Confidence::certain(),
                vec![ResolvedModuleCardEvidence::Symbol {
                    id: ModuleCardEvidenceId::for_symbol_v1(&symbol),
                    symbol,
                }],
            )
            .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)?;
            let current_hypothesis = current_hypothesis(published, self.module_id)?;
            TaskLensClaimResult::new(vec![stale_fact, current_hypothesis], false)
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)
        })();
        Box::pin(async move { result })
    }
}

fn current_hypothesis(
    published: &PublishedIndex,
    module_id: ModuleId,
) -> Result<TaskLensClaim, TaskLensClaimStoreFailure> {
    TaskLensClaim::new(
        published.run().id(),
        published.run().snapshot_id(),
        ModuleCardClaimId::from_bytes([71; 32]),
        module_id,
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::ArchitecturalIntent(
            ModuleClaimStatement::try_from_string("context stays deterministic".to_owned())
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)?,
        ),
        VerifiedClaimKind::Hypothesis,
        VerifiedClaimStatus::Active,
        Confidence::from_basis_points(7_500)
            .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)?,
        Vec::new(),
    )
    .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)
}

struct Fixture {
    published: PublishedIndex,
    snapshot_id: SnapshotId,
    symbol_id: SymbolId,
    module_id: ModuleId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let revision =
            FileRevision::new(path("src/context.rs")?, ContentHash::from_bytes([10; 32]));
        let symbol_id = SymbolId::from_bytes([11; 32]);
        let range = SourceRange::new(0, 48, SourcePosition::new(0, 0), SourcePosition::new(2, 0))?;
        let symbol = GraphSymbol::new(
            symbol_id,
            revision.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string("compile_context".to_owned())?,
                range,
                range,
            )?,
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![revision.clone()],
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
        let module_id = ModuleId::from_bytes([31; 32]);
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![RepositoryModule::new(
                module_id,
                ModuleKind::PathBoundary,
                Some(ModuleRoot::Repository),
                Vec::new(),
                ModuleSymbolSet::new(vec![symbol_id], false)?,
                ModuleSymbolSet::empty(),
                ModuleSymbolSet::empty(),
            )?],
            vec![ModuleMembership::new(
                module_id,
                symbol_id,
                ModuleMembershipEvidence::path(revision),
            )],
            RepositoryCard::new(
                snapshot_id,
                ModulePolicyVersion::v1(),
                vec![module_id],
                vec![IndexLanguage::Rust],
                ModuleSymbolSet::empty(),
                1,
                1,
            )?,
        )?;
        let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([100; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(Self {
            published: PublishedIndex::new(run, publication)?,
            snapshot_id,
            symbol_id,
            module_id,
        })
    }
}

fn input(_snapshot_id: SnapshotId) -> Result<AgentContextCompileInput, Box<dyn Error>> {
    let goal = GoalContract::initial(
        TaskId::from_bytes([90; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string("implement H7".to_owned())?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([91; 32]),
                AcceptanceCriterionStatement::try_from_string("pack is deterministic".to_owned())?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            a3_domain::SuccessVerification::try_from_string("run context tests".to_owned())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let step_id = TaskStepId::from_bytes([92; 32]);
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("compile context".to_owned())?,
            TaskStepRationale::try_from_string("prepare next turn".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "context digest".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([93; 32]),
                VerificationMethod::Test,
                VerificationRequirement::try_from_string("run tests".to_owned())?,
            ),
        )?],
        TaskLedgerTimestamp::from_unix_millis(1)?,
    )?;
    AgentContextCompileInput::new(
        project()?,
        goal,
        ledger,
        step_id,
        profile()?,
        None,
        Vec::new(),
        Vec::new(),
    )
    .map_err(Into::into)
}

fn input_with_run_memory(
    fixture: &Fixture,
    completed_summary: &str,
) -> Result<
    (
        AgentContextCompileInput,
        a3_domain::RunMemoryDigest,
        RunEventSequence,
    ),
    Box<dyn Error>,
> {
    let goal = GoalContract::initial(
        TaskId::from_bytes([110; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string("implement H8 compaction".to_owned())?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([111; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "retain original result sources".to_owned(),
                )?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            a3_domain::SuccessVerification::try_from_string(
                "run memory contract tests".to_owned(),
            )?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let completed_step_id = TaskStepId::from_bytes([112; 32]);
    let current_step_id = TaskStepId::from_bytes([113; 32]);
    let completed_spec_id = VerificationSpecId::from_bytes([114; 32]);
    let current_spec_id = VerificationSpecId::from_bytes([115; 32]);
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![
            TaskStepDefinition::new(
                completed_step_id,
                None,
                TaskStepOutcome::try_from_string("materialize prior step".to_owned())?,
                TaskStepRationale::try_from_string("retain factual progress".to_owned())?,
                Vec::new(),
                vec![ExpectedTaskEvidence::try_from_string(
                    "verified H7 result".to_owned(),
                )?],
                VerificationSpec::new(
                    completed_spec_id,
                    VerificationMethod::Test,
                    VerificationRequirement::try_from_string("run H7 tests".to_owned())?,
                ),
            )?,
            TaskStepDefinition::new(
                current_step_id,
                None,
                TaskStepOutcome::try_from_string("compile next context".to_owned())?,
                TaskStepRationale::try_from_string("continue without full run text".to_owned())?,
                vec![StepDependency::new(completed_step_id)],
                vec![ExpectedTaskEvidence::try_from_string(
                    "source-bound memory".to_owned(),
                )?],
                VerificationSpec::new(
                    current_spec_id,
                    VerificationMethod::Test,
                    VerificationRequirement::try_from_string("run H8 tests".to_owned())?,
                ),
            )?,
        ],
        TaskLedgerTimestamp::from_unix_millis(1)?,
    )?;
    let run_id = AgentRunId::from_bytes([120; 32]);
    ledger.start_step(
        completed_step_id,
        run_id,
        TaskLedgerTimestamp::from_unix_millis(2)?,
    )?;
    ledger.begin_step_verification(
        completed_step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            completed_summary.to_owned(),
        )?),
        vec![TaskEvidenceId::from_bytes([16; 32])],
        TaskLedgerTimestamp::from_unix_millis(3)?,
    )?;
    ledger.finish_step_verification(
        completed_step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([116; 32]),
            completed_spec_id,
            run_id,
            StepVerificationOutcome::Passed,
            vec![TaskEvidenceId::from_bytes([17; 32])],
            TaskLedgerTimestamp::from_unix_millis(4)?,
        )?,
    )?;
    ledger.start_step(
        current_step_id,
        run_id,
        TaskLedgerTimestamp::from_unix_millis(5)?,
    )?;
    ledger.begin_step_verification(
        current_step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            "H8 verification remains open".to_owned(),
        )?),
        vec![TaskEvidenceId::from_bytes([18; 32])],
        TaskLedgerTimestamp::from_unix_millis(6)?,
    )?;
    ledger.finish_step_verification(
        current_step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([117; 32]),
            current_spec_id,
            run_id,
            StepVerificationOutcome::Failed {
                summary: VerificationFailureSummary::try_from_string(
                    "retry the H8 verification".to_owned(),
                )?,
            },
            vec![TaskEvidenceId::from_bytes([19; 32])],
            TaskLedgerTimestamp::from_unix_millis(7)?,
        )?,
    )?;

    let model_profile = profile()?;
    let run = AgentRun::reconstruct(
        AgentRunIdentity::new(
            run_id,
            goal.reference(),
            ledger.revision(),
            Some(model_profile.reference()),
        ),
        AgentRunMaterializedState::new(
            AgentControllerState::Verify,
            RunEventSequence::new(7)?,
            fixture.snapshot_id,
        ),
        AgentRunTiming::new(
            AgentRunTimestamp::from_unix_millis(1)?,
            AgentRunTimestamp::from_unix_millis(7)?,
        ),
    )?;
    let memory = RunMemoryCheckpoint::compile(
        &goal,
        &ledger,
        &run,
        &fixture.published,
        vec![current_hypothesis(&fixture.published, fixture.module_id)?],
    )?;
    let memory_digest = memory.digest();
    let run_sequence = run.last_event_sequence();
    let input = AgentContextCompileInput::new(
        project()?,
        goal,
        ledger,
        current_step_id,
        model_profile,
        Some(memory),
        Vec::new(),
        Vec::new(),
    )?;
    Ok((input, memory_digest, run_sequence))
}

fn profile() -> Result<ModelProfile, Box<dyn Error>> {
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
        ModelProviderId::try_from_string("fixture".to_owned())?,
        ModelId::try_from_string("fixture-model".to_owned())?,
        settings,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = std::env::current_dir()?.canonicalize()?;
    let repository_id = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn path(value: &str) -> Result<RepositoryPath, Box<dyn Error>> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

#[derive(Debug)]
struct TestError(&'static str);

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for TestError {}
