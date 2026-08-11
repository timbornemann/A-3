use crate::support::{TempDirectory, run_libsql_test};
use a3_application::{
    AcceptanceVerificationRequest, AdvanceAgentController, AgentControllerControl,
    AgentControllerSignal, AppendRunEvent, CompileTaskLens, ConfirmProjectCommandAllowlist,
    ConservativeProcessVerificationEvidenceFactory, ContextCompileControl, ContextCompilePhase,
    CreateAgentRun, CreateGoalContract, CreateTaskLedger, DeterministicAcceptanceVerifier,
    DiscoverProjectCommands, ExecuteMutatingAgentAction, GoalContractStore, GrantPolicyApproval,
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeStore,
    MutationCommandSelection, MutationContextSeed, MutationControllerOutcome, MutationExecutionIds,
    ProcessEventSink, ProcessEventSinkError, ProcessRunControl, RefreshRepositoryIndex,
    RepositoryChangeBatch, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryRescanReason, RunEventPageLimit, RunJournalStore, TaskLedgerStoreVersion,
    TaskLensControlError, VerifyAgentAcceptance, WorkspacePatchControl,
    WorkspacePatchProgressError, WorktreeMutationCoordinator,
};
use a3_context::DeterministicAgentContextCompiler;
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentAction,
    AgentControllerState, AgentRun, AgentRunAction, AgentRunId, AgentRunTimestamp, ApprovalId,
    ApprovalRequestId, ContentHash, DiscoveredCommandKind, ExpectedTaskEvidence, FileRevision,
    GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelCapabilities,
    ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit, ModelProfile,
    ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile,
    ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, PatchAction,
    PatchActionSchemaVersion, PatchFileContent, PatchOperation, PatchRationale, PatchUpdate,
    PolicyDecisionId, ProcessEnvironmentVariable, ProcessEvent, ProcessEventKind, Progress,
    ProjectIdentity, PublishedIndex, RepositoryPath, RunEventId, RunEventKind, RunMemoryCheckpoint,
    SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, TaskStepStatus, ToolRunId, VerificationRequirement,
    VerificationRunId, VerificationScope, VerificationSpec, VerificationSpecId, WorkspacePolicy,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::{
    ProcessHostEnvironment, RepositoryInspector, WorkspacePatchAdapter, WorkspaceProcessRunner,
};
use std::error::Error;
use std::ffi::OsString;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const FIXTURE_PYPROJECT: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/small-local-bugfix/pyproject.toml");
const FIXTURE_TEST_RUNNER: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/small-local-bugfix/pytest.py");
const FIXTURE_TEST: &[u8] = include_bytes!(
    "../../../../fixtures/agent-coding-eval-v1/small-local-bugfix/tests/test_increment.py"
);
const BUGGY_SOURCE: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/small-local-bugfix/increment.py");
const FIXED_SOURCE: &[u8] = b"def increment(value: int) -> int:\n    return value + 1\n";
const EXPECTED_RESULTS: &str =
    include_str!("../../../../fixtures/agent-coding-eval-v1/expected-results.json");

#[test]
fn coding_eval_v1_matches_reviewed_results() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let result = evaluate_small_local_bugfix().await?;
        let actual = render_results(&[result]);
        if actual.trim() != EXPECTED_RESULTS.trim() {
            return Err(std::io::Error::other(format!(
                "coding eval result changed\nexpected:\n{}\nactual:\n{}",
                EXPECTED_RESULTS.trim(),
                actual.trim()
            ))
            .into());
        }
        Ok(())
    })
}

async fn evaluate_small_local_bugfix() -> Result<CodingEvalResult, Box<dyn Error>> {
    let fixture = CodingFixture::new().await?;
    let catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &fixture.published)?;
    let command = catalog
        .commands()
        .iter()
        .find(|command| command.kind() == DiscoveredCommandKind::Test)
        .ok_or_else(|| test_error("coding fixture test command was not discovered"))?;
    let confirmation = ConfirmProjectCommandAllowlist::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            &catalog,
            vec![command.id()],
            timestamp(10)?,
            None,
        )
        .await?;
    let criterion_id = AcceptanceCriterionId::from_bytes(id(20));
    let step_id = TaskStepId::from_bytes(id(21));
    let spec_id = VerificationSpecId::from_bytes(id(22));
    let spec = VerificationSpec::command(
        spec_id,
        requirement("locked tests pass")?,
        command.id(),
        VerificationScope::Workspace,
    );
    let mut durable = DurableCodingTask::new(&fixture, criterion_id, step_id, spec).await?;
    let patch = PatchAction::new(
        PatchActionSchemaVersion::V1,
        durable.run.id(),
        fixture.project.worktree().id(),
        fixture.published.run().snapshot_id(),
        step_id,
        spec_id,
        PatchRationale::try_from_string(
            "fix the local increment regression proven by the existing test".to_owned(),
        )?,
        vec![PatchOperation::Update(PatchUpdate::new(
            FileRevision::new(path("increment.py")?, hash(BUGGY_SOURCE)),
            PatchFileContent::try_from_bytes(FIXED_SOURCE.to_vec())?,
        )?)],
    )?;

    let refresh = refresh(fixture.store.clone());
    let context = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
    ));
    let coordinator = WorktreeMutationCoordinator::new();
    let patch_tool = WorkspacePatchAdapter::new();
    let process_runner = WorkspaceProcessRunner::new(process_environment()?);
    let evidence_factory = ConservativeProcessVerificationEvidenceFactory;
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        &patch_tool,
        &process_runner,
        &evidence_factory,
        &context,
        &refresh,
    );
    let seed = durable.context_seed();
    let mut compiler = compiler()?;
    let first = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &fixture.published,
            AgentAction::ApplyPatch(Box::new(patch.clone())),
            None,
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(40),
            timestamp(20)?,
            timestamp(100)?,
            &seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request_id) = first else {
        return Err(test_error("coding bugfix patch bypassed explicit approval"));
    };
    let mut approval = GrantPolicyApproval::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            &mut durable.run,
            request_id,
            ApprovalId::from_bytes(id(70)),
            RunEventId::from_bytes(id(71)),
            fixture.published.run().snapshot_id(),
            timestamp(21)?,
        )
        .await?;
    let patched = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &fixture.published,
            AgentAction::ApplyPatch(Box::new(patch)),
            None,
            &WorkspacePolicy::unrestricted(),
            Some(&mut approval),
            mutation_ids(80),
            timestamp(22)?,
            timestamp(100)?,
            &seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    if !matches!(patched, MutationControllerOutcome::NextAction(_)) {
        return Err(test_error(
            "coding bugfix patch did not request verification",
        ));
    }
    let patched_index = latest_index(&fixture).await?;
    let selection = MutationCommandSelection::new(&catalog, &confirmation);
    let process_events = RecordingProcessEvents::default();
    let verified = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &patched_index,
            AgentAction::Run(AgentRunAction::new(step_id, command.id())),
            Some(selection),
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(120),
            timestamp(30)?,
            timestamp(100)?,
            &seed,
            &mut compiler,
            &process_events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = verified
    else {
        return Err(std::io::Error::other(format!(
            "offline locked test command did not verify the coding step: {verified:?}; events: {:?}",
            process_events.summary()
        ))
        .into());
    };
    if snapshot_id != durable.run.current_snapshot_id() {
        return Err(test_error("coding verification used a stale snapshot"));
    }

    let accepted_index = latest_index(&fixture).await?;
    let run_memory = RunMemoryCheckpoint::compile(
        &durable.goal,
        &durable.ledger,
        &durable.run,
        &accepted_index,
        Vec::new(),
    )?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &durable.run,
        durable.goal.clone(),
        durable.ledger.clone(),
        run_memory,
    )?;
    let verifier = DeterministicAcceptanceVerifier::new(fixture.store.as_ref());
    let expected_sequence = durable.run.last_event_sequence();
    let accepted = VerifyAgentAcceptance::new(&verifier)
        .execute(
            &mut durable.run,
            &request,
            RunEventId::from_bytes(id(160)),
            timestamp(40)?,
            &ActiveControl,
        )
        .await?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            expected_sequence,
            &durable.run,
            accepted.event(),
        )
        .await?;

    let stored_goal = fixture
        .store
        .load_current_goal_contract(&fixture.project, durable.goal.task_id())
        .await?;
    let step = durable
        .ledger
        .step(step_id)
        .ok_or_else(|| test_error("coding task step disappeared"))?;
    let verification = step
        .attempts()
        .last()
        .and_then(|attempt| attempt.verification());
    let events = fixture
        .store
        .load_run_events(
            &fixture.project,
            durable.run.id(),
            None,
            RunEventPageLimit::new(64)?,
        )
        .await?;
    let patch_present =
        std::fs::read(fixture.repository.path().join("increment.py"))? == FIXED_SOURCE;
    let unrelated_manifest_preserved =
        std::fs::read(fixture.repository.path().join("pyproject.toml"))? == FIXTURE_PYPROJECT;
    let tool_event_count = events
        .events()
        .iter()
        .filter(|event| event.kind() == RunEventKind::ToolAction)
        .count();
    Ok(CodingEvalResult {
        id: "small-local-bugfix",
        final_state: if durable.run.state() == AgentControllerState::Done {
            "done"
        } else {
            "not_done"
        },
        goal: stored_goal.as_ref() == Some(&durable.goal),
        step: step.status() == TaskStepStatus::Completed,
        patch: patch_present && tool_event_count >= 2,
        evidence: verification.is_some_and(|value| value.evidence_ids() == [evidence_id]),
        verification: verification.is_some_and(|value| value.passed()),
        foreign_change_preserved: unrelated_manifest_preserved,
        replan_count: durable.ledger.replans().len(),
        compaction_count: 0,
    })
}

struct CodingFixture {
    repository: TempDirectory,
    _app_data: TempDirectory,
    project: ProjectIdentity,
    store: Arc<LibsqlKnowledgeStore>,
    published: PublishedIndex,
}

impl CodingFixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("pyproject.toml", FIXTURE_PYPROJECT)?;
        repository.write("increment.py", BUGGY_SOURCE)?;
        repository.write("pytest.py", FIXTURE_TEST_RUNNER)?;
        repository.write("tests/test_increment.py", FIXTURE_TEST)?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = refresh(store.clone());
        let mut compiler = compiler()?;
        let indexed = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &ActiveControl,
            )
            .await?;
        Ok(Self {
            repository,
            _app_data: app_data,
            project,
            store,
            published: indexed.published_index().clone(),
        })
    }
}

struct DurableCodingTask {
    goal: GoalContract,
    ledger: TaskLedger,
    ledger_version: TaskLedgerStoreVersion,
    run: AgentRun,
    profile: ModelProfile,
}

impl DurableCodingTask {
    async fn new(
        fixture: &CodingFixture,
        criterion_id: AcceptanceCriterionId,
        step_id: TaskStepId,
        spec: VerificationSpec,
    ) -> Result<Self, Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes(*criterion_id.as_bytes()),
            GoalContractDraft::new(
                GoalObjective::try_from_string("fix increment regression".to_owned())?,
                vec![AcceptanceCriterion::new(
                    criterion_id,
                    AcceptanceCriterionStatement::try_from_string("locked tests pass".to_owned())?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run locked tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let definition = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("fix increment and test".to_owned())?,
            TaskStepRationale::try_from_string("prove the fix with tests".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "locked test result".to_owned(),
            )?],
            spec,
        )?
        .with_acceptance_criteria(vec![criterion_id])?;
        let run_id = AgentRunId::from_bytes(*step_id.as_bytes());
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![definition],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(3)?)?;
        CreateGoalContract::new(fixture.store.as_ref())
            .execute(&fixture.project, &goal)
            .await?;
        let ledger_version = CreateTaskLedger::new(fixture.store.as_ref())
            .execute(&fixture.project, &ledger)
            .await?
            .version();
        let profile = model_profile()?;
        let (mut run, start_event) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            profile.reference(),
            fixture.published.run().snapshot_id(),
            RunEventId::from_bytes(id(4)),
            timestamp(4)?,
        )?;
        CreateAgentRun::new(fixture.store.as_ref())
            .execute(&fixture.project, &run, &start_event)
            .await?;
        for (signal, event, at) in [
            (AgentControllerSignal::AnchorsAccepted, 5, 5),
            (AgentControllerSignal::LocalizationComplete, 6, 6),
            (AgentControllerSignal::PlanReady, 7, 7),
        ] {
            advance(
                fixture.store.as_ref(),
                &fixture.project,
                &mut run,
                signal,
                RunEventId::from_bytes(id(event)),
                timestamp(at)?,
            )
            .await?;
        }
        Ok(Self {
            goal,
            ledger,
            ledger_version,
            run,
            profile,
        })
    }

    fn context_seed(&self) -> MutationContextSeed {
        MutationContextSeed::new(
            self.goal.clone(),
            self.profile.clone(),
            Vec::new(),
            Vec::new(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodingEvalResult {
    id: &'static str,
    final_state: &'static str,
    goal: bool,
    step: bool,
    patch: bool,
    evidence: bool,
    verification: bool,
    foreign_change_preserved: bool,
    replan_count: usize,
    compaction_count: usize,
}

fn render_results(results: &[CodingEvalResult]) -> String {
    let mut output =
        String::from("{\n  \"schema\": \"a3.agent-coding-eval.v1\",\n  \"cases\": [\n");
    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str("    {\n");
        output.push_str(&format!("      \"id\": \"{}\",\n", result.id));
        output.push_str(&format!(
            "      \"final_state\": \"{}\",\n",
            result.final_state
        ));
        output.push_str(&format!("      \"goal\": {},\n", result.goal));
        output.push_str(&format!("      \"step\": {},\n", result.step));
        output.push_str(&format!("      \"patch\": {},\n", result.patch));
        output.push_str(&format!("      \"evidence\": {},\n", result.evidence));
        output.push_str(&format!(
            "      \"verification\": {},\n",
            result.verification
        ));
        output.push_str(&format!(
            "      \"foreign_change_preserved\": {},\n",
            result.foreign_change_preserved
        ));
        output.push_str(&format!(
            "      \"replan_count\": {},\n",
            result.replan_count
        ));
        output.push_str(&format!(
            "      \"compaction_count\": {}\n",
            result.compaction_count
        ));
        output.push_str("    }");
    }
    output.push_str("\n  ]\n}\n");
    output
}

async fn latest_index(fixture: &CodingFixture) -> Result<PublishedIndex, Box<dyn Error>> {
    fixture
        .store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or_else(|| test_error("coding fixture published index is missing"))
}

async fn advance(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: &mut AgentRun,
    signal: AgentControllerSignal,
    event_id: RunEventId,
    observed_at: AgentRunTimestamp,
) -> Result<(), Box<dyn Error>> {
    let expected = run.last_event_sequence();
    let outcome = AdvanceAgentController.execute(
        run,
        signal,
        event_id,
        run.current_snapshot_id(),
        observed_at,
        false,
    )?;
    AppendRunEvent::new(store)
        .execute(project, expected, run, outcome.event())
        .await?;
    Ok(())
}

fn process_environment() -> Result<ProcessHostEnvironment, Box<dyn Error>> {
    let ambient_path = std::env::var_os("PATH").ok_or_else(|| test_error("PATH is unavailable"))?;
    let temporary = OsString::from(std::env::temp_dir());
    let values = [
        ("PATH", ambient_path),
        ("TEMP", temporary.clone()),
        ("TMP", temporary.clone()),
        ("TMPDIR", temporary),
    ]
    .into_iter()
    .map(|(name, value)| {
        Ok((
            ProcessEnvironmentVariable::try_from_string(name.to_owned())?,
            value,
        ))
    })
    .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(ProcessHostEnvironment::new(values)?)
}

fn refresh(store: Arc<LibsqlKnowledgeStore>) -> RefreshRepositoryIndex {
    let index_store: Arc<dyn KnowledgeIndexStore> = store;
    RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        index_store,
        Arc::new(Blake3IndexRunIdFactory),
    )
}

fn compiler() -> Result<BuiltinIncrementalIndexCompiler, Box<dyn Error>> {
    Ok(BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(
        2,
    )?)?)
}

fn model_profile() -> Result<ModelProfile, Box<dyn Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("e9-contract".to_owned())?,
        ModelId::try_from_string("e9-local-model".to_owned())?,
        ModelProfileSettings::new(
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
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

fn mutation_ids(base: u8) -> MutationExecutionIds {
    MutationExecutionIds::new(
        PolicyDecisionId::from_bytes(id(base)),
        ApprovalRequestId::from_bytes(id(base.wrapping_add(1))),
        RunEventId::from_bytes(id(base.wrapping_add(2))),
        RunEventId::from_bytes(id(base.wrapping_add(3))),
        ToolRunId::from_bytes(id(base.wrapping_add(4))),
        RunEventId::from_bytes(id(base.wrapping_add(5))),
        RunEventId::from_bytes(id(base.wrapping_add(6))),
        RunEventId::from_bytes(id(base.wrapping_add(7))),
        RunEventId::from_bytes(id(base.wrapping_add(8))),
        VerificationRunId::from_bytes(id(base.wrapping_add(9))),
        a3_domain::StepVerificationId::from_bytes(id(base.wrapping_add(10))),
    )
}

fn requirement(value: &str) -> Result<VerificationRequirement, Box<dyn Error>> {
    Ok(VerificationRequirement::try_from_string(value.to_owned())?)
}

fn path(value: &str) -> Result<RepositoryPath, Box<dyn Error>> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

fn hash(value: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(value).as_bytes())
}

const fn id(value: u8) -> [u8; 32] {
    [value; 32]
}

fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
    Ok(AgentRunTimestamp::from_unix_millis(value)?)
}

fn test_error(message: &'static str) -> Box<dyn Error> {
    std::io::Error::other(message).into()
}

#[derive(Debug, Clone, Copy)]
struct NoopProcessEvents;

impl ProcessEventSink for NoopProcessEvents {
    fn emit(&self, _event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RecordingProcessEvents {
    events: Mutex<Vec<ProcessEvent>>,
}

impl RecordingProcessEvents {
    fn summary(&self) -> Vec<String> {
        lock_recovering_poison(&self.events)
            .iter()
            .map(|event| match event.kind() {
                ProcessEventKind::Output { stream, chunk } => {
                    format!("{stream:?}: {}", chunk.as_str())
                }
                kind => format!("{kind:?}"),
            })
            .collect()
    }
}

impl ProcessEventSink for RecordingProcessEvents {
    fn emit(&self, event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        lock_recovering_poison(&self.events).push(event);
        Ok(())
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveControl;

impl AgentControllerControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl ContextCompileControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_phase(&self, _phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl RepositoryIndexControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl IndexPersistenceControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

impl WorkspacePatchControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}

impl ProcessRunControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn wait_cancelled_timeout(&self, _timeout: Duration) -> bool {
        false
    }
}
