//! Real Python evidence through existing ports; no model, canned process result or Passed marker.
#[path = "plan22_a02_index_tests.rs"]
mod a02_index;
#[path = "plan22_changed_existing_test_tests.rs"]
mod changed_tests;
#[path = "plan22_loopback_storage_tests.rs"]
mod http_store;

use super::*;
use a3_application::{
    AcceptanceVerificationRequest, AgentActionStore, AgentApprovalBuffer, AgentContextCompileInput,
    AgentContextCompiler, AgentContextCompilerFuture, AgentControllerControl,
    AgentControllerSignal, AgentInspectionBuffer, AgentPermissionStore, AgentRecoveryStore,
    AppendRunEvent, BuiltinProcessVerificationEvidenceFactory, CommandAllowlistStore,
    ConfirmProjectCommandAllowlist, ContextCompileControl, ContextCompileFailure,
    ContextCompilePhase, CreateAgentRun, DeterministicAcceptanceVerifier, DiscoverProjectCommands,
    ExecuteMutatingAgentAction, GrantPolicyApproval, KnowledgeIndexStore, KnowledgeStore,
    MutationCommandSelection, MutationContextSeed, MutationControllerOutcome, MutationExecutionIds,
    PolicyStore, ProcessEventSink, ProcessEventSinkError, ProcessRunControl,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryIndexPhase, RepositoryRescanReason, RunJournalStore,
    TaskLensControlError, VerificationEvidenceStore, VerifyAgentAcceptance, WorkspacePatchControl,
    WorkspacePatchProgressError, WorktreeMutationCoordinator,
};
use a3_application::{AgentActionStoreFailure, StoredTaskLedger, TaskLedgerStoreVersion};
use a3_domain::{
    AgentAction, AgentControllerState, AgentMutationKind, AgentPermissionMode, AgentRun,
    AgentRunAction, AgentRunId, AgentRunTimestamp, AgentToolAttemptStatus, ApprovalId,
    ApprovalRequestId, DiscoveredCommandKind, ModelCapabilities, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    ProcessEnvironmentVariable, ProcessEvent, ProcessEventKind, ProcessTermination, Progress,
    RunEventId, RunMemoryCheckpoint, SnapshotId, StepVerificationId, TaskEvidenceId,
    TaskLedgerTimestamp, TaskStepStatus, ToolRunId, VerificationEvidence, VerificationRunId,
    WorkspacePolicy,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_workspace::{
    ProcessHostEnvironment, RepositoryInspector, WorkspacePatchAdapter, WorkspaceProcessRunner,
};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SUBJECT: &[u8] = b"def average(values):\n    if not values:\n        raise ValueError('empty')\n    return sum(values) / len(values)\n";
const TESTS: &[u8] = b"import unittest\nfrom average import average\n\nclass Average(unittest.TestCase):\n    def test_fractional(self):\n        self.assertEqual(average([1, 2]), 1.5)\n    def test_empty(self):\n        with self.assertRaises(ValueError):\n            average([])\n";
const GOAL_BYTES: &[u8] = b"average([1, 2]) returns 1.5; average([]) raises ValueError.\nDo not create or change tests.\n";
const RUN: u8 = 211;
const STEP: u8 = 212;
const POLICY: u8 = 232;

#[test]
fn already_satisfied_uses_actual_unittest_and_survives_full_candidate_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        let mut fixture = Fixture::new()?;
        let worktree = fixture.root.join("worktree");
        initialize_git(&worktree)?;
        fs::create_dir(worktree.join("tests"))?;
        fs::write(worktree.join("average.py"), SUBJECT)?;
        fs::write(worktree.join("tests/__init__.py"), b"")?;
        fs::write(worktree.join("tests/test_average.py"), TESTS)?;
        // No database is opened before deriving the real repository/worktree identity.
        fixture.project = RepositoryInspector::new().inspect(&worktree)?;
        let initial_inventory = inventory(&worktree)?;
        let (goal, evidence_id, snapshot) = run_existing_test(&fixture).await?;
        assert_eq!(inventory(&worktree)?, initial_inventory);
        assert_eq!(fs::read(worktree.join("average.py"))?, SUBJECT);
        assert_eq!(fs::read(worktree.join("tests/test_average.py"))?, TESTS);

        // Each operation starts with a completely new store, before Goal/Ledger can
        // warm its mutation cache. Search and mutation caches cannot hide V42 opens.
        assert_cold_candidate_ports(&fixture, &goal, evidence_id, snapshot).await?;
        assert_cold_action_cas(&fixture, &goal).await?;

        finish_reopened_source(&fixture, goal, evidence_id, snapshot, 30).await?;
        assert_eq!(inventory(&worktree)?, initial_inventory);
        fixture.cleanup()?;
        Ok(())
    })
}

async fn finish_reopened_source(
    fixture: &Fixture,
    goal: GoalContract,
    evidence_id: TaskEvidenceId,
    snapshot: SnapshotId,
    finished_at: u64,
) -> TestResult {
    finish_reopened_source_for_step(
        fixture,
        goal,
        evidence_id,
        snapshot,
        finished_at,
        AgentRunId::from_bytes([RUN; 32]),
        TaskStepId::from_bytes([STEP; 32]),
        RunEventId::from_bytes([251; 32]),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish_reopened_source_for_step(
    fixture: &Fixture,
    goal: GoalContract,
    evidence_id: TaskEvidenceId,
    snapshot: SnapshotId,
    finished_at: u64,
    run_id: AgentRunId,
    step_id: TaskStepId,
    event_id: RunEventId,
) -> TestResult {
    let started = std::time::Instant::now();
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        // Evidence is deliberately the first per-project operation after full reopen.
        let state = store
            .load_verification_state(
                &fixture.project,
                goal.task_id(),
                &[evidence_id],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let loaded_goal = store
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("original goal missing after reopen")?;
        assert_eq!(loaded_goal, goal);
        assert_eq!(
            loaded_goal.original().ok_or("source missing")?.as_bytes(),
            goal.original()
                .ok_or("expected original missing")?
                .as_bytes()
        );
        assert!(a3_domain::GoalOriginalCoverage::derive_from(&loaded_goal)?.is_complete());
        let ledger = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing after reopen")?;
        assert_eq!(
            ledger
                .ledger()
                .step(step_id)
                .ok_or("step missing")?
                .status(),
            TaskStepStatus::Completed
        );
        let mut run = store
            .load_agent_run(&fixture.project, run_id)
            .await?
            .ok_or("run missing after reopen")?;
        assert_eq!(run.state(), AgentControllerState::Verify);
        let memory = RunMemoryCheckpoint::compile(
            &loaded_goal,
            ledger.ledger(),
            &run,
            state.published_index(),
            Vec::new(),
        )?;
        let request = AcceptanceVerificationRequest::new(
            fixture.project.clone(),
            &run,
            loaded_goal,
            ledger.ledger().clone(),
            memory,
        )?;
        let expected = run.last_event_sequence();
        let verifier = DeterministicAcceptanceVerifier::new(&store);
        let observed_at = source_acceptance_observed_at(started, finished_at, &run)?;
        let advance = VerifyAgentAcceptance::new(&verifier)
            .execute(&mut run, &request, event_id, observed_at, &ActiveControl)
            .await?;
        assert_eq!(run.state(), AgentControllerState::Done);
        // Recheck real elapsed time after verifier I/O and before durable Done.
        source_acceptance_observed_at(started, finished_at, &run)?;
        AppendRunEvent::new(&store)
            .execute(&fixture.project, expected, &run, advance.event())
            .await?;
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        // RunJournal is first on this fresh store too; no materializer preheat.
        assert_eq!(
            store
                .load_agent_run(&fixture.project, run_id)
                .await?
                .ok_or("done run missing")?
                .state(),
            AgentControllerState::Done
        );
        assert_eq!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .ok_or("final source missing")?,
            goal
        );
    }
    Ok(())
}

fn source_acceptance_observed_at(
    started: std::time::Instant,
    base: u64,
    run: &AgentRun,
) -> TestResult<AgentRunTimestamp> {
    let observed = at(base
        .checked_add(u64::try_from(started.elapsed().as_millis())?)
        .ok_or("acceptance clock overflow")?
        .max(run.updated_at().unix_millis()))?;
    let deadline = run
        .created_at()
        .unix_millis()
        .checked_add(run.budget().duration_limit().millis())
        .ok_or("original deadline overflow")?;
    if observed.unix_millis() >= deadline {
        return Err("original Run deadline elapsed before durable acceptance".into());
    }
    Ok(observed)
}

struct PreparedExistingRun {
    store: Arc<LibsqlKnowledgeStore>,
    refresh: RefreshRepositoryIndex,
    compiler: BuiltinIncrementalIndexCompiler,
    published: a3_domain::PublishedIndex,
    catalog: a3_domain::ProjectCommandCatalog,
    confirmation: a3_application::StoredProjectCommandAllowlist,
    goal: GoalContract,
    ledger: a3_domain::TaskLedger,
    version: TaskLedgerStoreVersion,
    profile: ModelProfile,
    run: AgentRun,
    unplanned_criteria: Vec<AcceptanceCriterionId>,
}

async fn prepare_existing_run(
    fixture: &Fixture,
    profile: ModelProfile,
    goal_builder: fn(DiscoveredCommandId) -> TestResult<GoalContract>,
) -> TestResult<PreparedExistingRun> {
    let prepared =
        prepare_existing_run_with_step(fixture, profile, goal_builder, existing_test_step).await?;
    assert!(prepared.unplanned_criteria.is_empty());
    Ok(prepared)
}

async fn prepare_existing_run_with_step(
    fixture: &Fixture,
    profile: ModelProfile,
    goal_builder: fn(DiscoveredCommandId) -> TestResult<GoalContract>,
    step_builder: fn(&GoalContract, DiscoveredCommandId) -> TestResult<TaskStepDefinition>,
) -> TestResult<PreparedExistingRun> {
    prepare_existing_run_with_steps(fixture, profile, goal_builder, &[step_builder]).await
}

async fn prepare_existing_run_with_steps(
    fixture: &Fixture,
    profile: ModelProfile,
    goal_builder: fn(DiscoveredCommandId) -> TestResult<GoalContract>,
    step_builders: &[fn(&GoalContract, DiscoveredCommandId) -> TestResult<TaskStepDefinition>],
) -> TestResult<PreparedExistingRun> {
    let (store, refresh, compiler, published) = index_fixture(fixture).await?;
    let catalog = DiscoverProjectCommands.execute(fixture.project.worktree().id(), &published)?;
    let matches = catalog
        .commands()
        .iter()
        .filter(|command| command.kind() == DiscoveredCommandKind::Test)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let command = matches[0];
    assert_eq!(command.executable().as_str(), "python");
    assert_eq!(
        command
            .arguments()
            .iter()
            .map(|arg| arg.as_str())
            .collect::<Vec<_>>(),
        ["-B", "-m", "unittest", "discover", "-s", "tests"]
    );
    let confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(&fixture.project, &catalog, vec![command.id()], at(2)?, None)
        .await?;
    let goal = goal_builder(command.id())?;
    let definitions = step_builders
        .iter()
        .map(|builder| builder(&goal, command.id()))
        .collect::<TestResult<Vec<_>>>()?;
    let (run, stored, unplanned_criteria) = start_fixture_step(
        fixture,
        store.as_ref(),
        &goal,
        &profile,
        published.run().snapshot_id(),
        definitions,
    )
    .await?;
    let (ledger, version) = stored.into_parts();

    Ok(PreparedExistingRun {
        store,
        refresh,
        compiler,
        published,
        catalog,
        confirmation,
        goal,
        ledger,
        version,
        profile,
        run,
        unplanned_criteria,
    })
}

async fn index_fixture(
    fixture: &Fixture,
) -> TestResult<(
    Arc<LibsqlKnowledgeStore>,
    RefreshRepositoryIndex,
    BuiltinIncrementalIndexCompiler,
    a3_domain::PublishedIndex,
)> {
    let (store, refresh, compiler, indexed) =
        index_fixture_refresh(fixture, &ActiveControl).await?;
    let published = indexed.published_index().clone();
    Ok((store, refresh, compiler, published))
}

async fn index_fixture_refresh(
    fixture: &Fixture,
    control: &dyn RepositoryIndexControl,
) -> TestResult<(
    Arc<LibsqlKnowledgeStore>,
    RefreshRepositoryIndex,
    BuiltinIncrementalIndexCompiler,
    a3_application::RepositoryIndexRefresh,
)> {
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    assert_eq!(
        store.load_agent_permissions().await?.mode(),
        AgentPermissionMode::AskPermissions
    );
    store.record_opened_project(&fixture.project).await?;
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let indexed = refresh
        .execute(
            &fixture.project,
            &RepositoryChangeBatch::full_rescan(
                Vec::new(),
                RepositoryRescanReason::InitialObservation,
            )?,
            &mut compiler,
            control,
        )
        .await?;
    Ok((store, refresh, compiler, indexed))
}

async fn start_fixture_step(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    goal: &GoalContract,
    profile: &ModelProfile,
    snapshot: SnapshotId,
    definitions: Vec<TaskStepDefinition>,
) -> TestResult<(AgentRun, StoredTaskLedger, Vec<AcceptanceCriterionId>)> {
    start_fixture_step_with_budget(
        fixture,
        store,
        goal,
        profile,
        snapshot,
        definitions,
        (a3_domain::AgentRunBudget::DEFAULT, at(3)?),
    )
    .await
}

async fn start_fixture_step_with_budget(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    goal: &GoalContract,
    profile: &ModelProfile,
    snapshot: SnapshotId,
    definitions: Vec<TaskStepDefinition>,
    start: (a3_domain::AgentRunBudget, AgentRunTimestamp),
) -> TestResult<(AgentRun, StoredTaskLedger, Vec<AcceptanceCriterionId>)> {
    let materialized = MaterializeOriginalTask::new(store, store)
        .execute(&fixture.project, goal, definitions)
        .await?;
    let preview = materialized.preview();
    let next_step = preview.next_step.ok_or("initial plan has no Ready step")?;
    let unplanned_criteria = preview.unplanned_criteria.to_vec();
    let (run, stored) = start_materialized_fixture_step(
        fixture,
        store,
        goal,
        profile,
        snapshot,
        materialized.stored_ledger().clone(),
        next_step,
        start,
    )
    .await?;
    Ok((run, stored, unplanned_criteria))
}

#[allow(clippy::too_many_arguments)]
async fn start_materialized_fixture_step(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    goal: &GoalContract,
    profile: &ModelProfile,
    snapshot: SnapshotId,
    stored: StoredTaskLedger,
    next_step: TaskStepId,
    start: (a3_domain::AgentRunBudget, AgentRunTimestamp),
) -> TestResult<(AgentRun, StoredTaskLedger)> {
    let (budget, started_at) = start;
    let started_at_millis = started_at.unix_millis();
    let (mut ledger, version) = stored.into_parts();
    let (mut run, start) = AgentRun::start_with_budget(
        AgentRunId::from_bytes([RUN; 32]),
        goal.reference(),
        ledger.revision(),
        profile.reference(),
        budget,
        snapshot,
        RunEventId::from_bytes([214; 32]),
        started_at,
    )?;
    CreateAgentRun::new(store)
        .execute(&fixture.project, &run, &start)
        .await?;
    for (event, signal) in [
        (215, AgentControllerSignal::AnchorsAccepted),
        (216, AgentControllerSignal::LocalizationComplete),
    ] {
        let expected = run.last_event_sequence();
        let snapshot = run.current_snapshot_id();
        let advance = a3_application::AdvanceAgentController.execute(
            &mut run,
            signal,
            RunEventId::from_bytes([event; 32]),
            snapshot,
            at(started_at_millis + 1 + u64::from(event - 215))?,
            false,
        )?;
        AppendRunEvent::new(store)
            .execute(&fixture.project, expected, &run, advance.event())
            .await?;
    }
    let expected = run.last_event_sequence();
    ledger.start_step(
        next_step,
        run.id(),
        TaskLedgerTimestamp::from_unix_millis(started_at_millis + 3)?,
    )?;
    let snapshot = run.current_snapshot_id();
    let advance = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::PlanReady,
        RunEventId::from_bytes([217; 32]),
        snapshot,
        at(started_at_millis + 3)?,
        false,
    )?;
    let version = store
        .commit_ledger_action(
            &fixture.project,
            version,
            expected,
            &ledger,
            &run,
            advance.event(),
        )
        .await?;

    Ok((run, StoredTaskLedger::new(ledger, version)))
}

fn existing_test_step(
    goal: &GoalContract,
    command: DiscoveredCommandId,
) -> TestResult<TaskStepDefinition> {
    let spec = VerificationSpec::test(
        VerificationSpecId::from_bytes([213; 32]),
        VerificationRequirement::try_from_string(
            "Both existing behavioral cases execute and pass".to_owned(),
        )?,
        command,
        TestCaseSelector::All,
        MinimumTestCaseCount::new(2)?,
        VerificationScope::Workspace,
    );
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(goal.draft().objective().as_str().to_owned())?,
        TaskStepRationale::try_from_string(
            "Run the source-bound existing check after any required change".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Two actual passing existing unittest cases".to_owned(),
        )?],
        spec,
    )?
    .with_acceptance_criteria(vec![AcceptanceCriterionId::from_bytes([210; 32])])?)
}

async fn run_existing_test(
    fixture: &Fixture,
) -> TestResult<(GoalContract, TaskEvidenceId, SnapshotId)> {
    let PreparedExistingRun {
        store,
        refresh,
        mut compiler,
        published,
        catalog,
        confirmation,
        goal,
        mut ledger,
        mut version,
        profile,
        mut run,
        ..
    } = prepare_existing_run(fixture, profile()?, actual_goal).await?;
    let command = catalog
        .commands()
        .iter()
        .find(|command| command.kind() == DiscoveredCommandKind::Test)
        .ok_or("fixture test command missing")?;
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approval = AgentApprovalBuffer::new();
    approval.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::new();
    let env_names = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"]
        .into_iter()
        .map(|name| ProcessEnvironmentVariable::try_from_string(name.to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let runner = WorkspaceProcessRunner::new(ProcessHostEnvironment::capture(env_names)?);
    let factory = BuiltinProcessVerificationEvidenceFactory;
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approval,
        &patch,
        &runner,
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let context_seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let outcome = controller
        .execute(
            &fixture.project,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            AgentAction::Run(AgentRunAction::new(
                TaskStepId::from_bytes([STEP; 32]),
                command.id(),
            )),
            Some(MutationCommandSelection::new(&catalog, &confirmation)),
            &WorkspacePolicy::unrestricted(),
            None,
            execution_ids(220),
            at(10)?,
            at(1000)?,
            &context_seed,
            &mut compiler,
            &events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request_id) = outcome else {
        return Err(format!(
            "Ask permissions did not request exact execution approval: {outcome:?}"
        )
        .into());
    };
    assert_eq!(request_id, ApprovalRequestId::from_bytes([221; 32]));
    assert_eq!(run.state(), AgentControllerState::AwaitApproval);
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "event capture poisoned")?
            .is_empty()
    );
    assert!(
        store
            .load_agent_mutation_attempts(&fixture.project, run.id())
            .await?
            .is_empty()
    );
    let snapshot = run.current_snapshot_id();
    let mut grant = GrantPolicyApproval::new(store.as_ref())
        .execute(
            &fixture.project,
            &mut run,
            request_id,
            ApprovalId::from_bytes([250; 32]),
            RunEventId::from_bytes([231; 32]),
            snapshot,
            at(11)?,
        )
        .await?;
    let outcome = controller
        .execute(
            &fixture.project,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            AgentAction::Run(AgentRunAction::new(
                TaskStepId::from_bytes([STEP; 32]),
                command.id(),
            )),
            Some(MutationCommandSelection::new(&catalog, &confirmation)),
            &WorkspacePolicy::unrestricted(),
            Some(&mut grant),
            execution_ids(POLICY),
            at(12)?,
            at(1000)?,
            &context_seed,
            &mut compiler,
            &events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(
            format!("real existing check did not produce StepVerified: {outcome:?}").into(),
        );
    };
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(snapshot_id, run.current_snapshot_id());
    let state = store
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[evidence_id],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::Test(evidence)] = state.evidence() else {
        return Err("actual TestEvidence missing".into());
    };
    assert_eq!(evidence.cases().len(), 2);
    assert_eq!(evidence.command().command_id(), command.id());
    assert_eq!(evidence.command().snapshot_id(), snapshot_id);
    assert!(
        matches!(evidence.command().termination(), ProcessTermination::Exited(exit) if exit.success())
    );
    assert_eq!(
        evidence.command().policy_decision_id(),
        PolicyDecisionId::from_bytes([POLICY; 32])
    );
    let decision = store
        .load_policy_decision(&fixture.project, PolicyDecisionId::from_bytes([POLICY; 32]))
        .await?
        .ok_or("actual policy receipt missing")?;
    assert_eq!(decision.outcome(), PolicyDecisionOutcome::Allowed);
    assert_eq!(decision.reason(), PolicyDecisionReason::ApprovalGranted);
    assert_eq!(
        decision
            .permission_settings()
            .ok_or("policy revision missing")?
            .mode(),
        AgentPermissionMode::AskPermissions
    );
    let attempts = store
        .load_agent_mutation_attempts(&fixture.project, run.id())
        .await?;
    assert_eq!(
        attempts.len(),
        1,
        "no patch or fabricated earlier mutation is allowed"
    );
    assert_eq!(attempts[0].kind(), AgentMutationKind::Process);
    assert_eq!(
        attempts[0].tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    let captured = events.values.lock().map_err(|_| "event capture poisoned")?;
    assert!(
        captured
            .iter()
            .any(|event| matches!(event.kind(), ProcessEventKind::Started))
    );
    assert!(captured.iter().any(|event| matches!(event.kind(), ProcessEventKind::Terminated(ProcessTermination::Exited(exit)) if exit.success())));
    Ok((goal, evidence_id, snapshot_id))
}

async fn assert_cold_candidate_ports(
    fixture: &Fixture,
    goal: &GoalContract,
    evidence: TaskEvidenceId,
    snapshot: SnapshotId,
) -> TestResult {
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(store.latest_snapshot(&fixture.project).await?.is_some());
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(store.load_current(&fixture.project).await?.is_some());
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(
            store
                .load_agent_run(&fixture.project, AgentRunId::from_bytes([RUN; 32]))
                .await?
                .is_some()
        );
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(
            store
                .load_policy_decision(&fixture.project, PolicyDecisionId::from_bytes([POLICY; 32]))
                .await?
                .is_some()
        );
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store
                .load_agent_mutation_attempts(&fixture.project, AgentRunId::from_bytes([RUN; 32]))
                .await?
                .len(),
            1
        );
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store
                .load_verification_state(
                    &fixture.project,
                    goal.task_id(),
                    &[evidence],
                    snapshot,
                    Duration::from_secs(10),
                    &ActiveControl
                )
                .await?
                .evidence()
                .len(),
            1
        );
    }
    Ok(())
}

async fn assert_cold_action_cas(fixture: &Fixture, goal: &GoalContract) -> TestResult {
    let (stored_ledger, mut run) = {
        let read = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        (
            read.load_task_ledger(&fixture.project, goal.task_id())
                .await?
                .ok_or("cold ledger missing")?,
            read.load_agent_run(&fixture.project, AgentRunId::from_bytes([RUN; 32]))
                .await?
                .ok_or("cold run missing")?,
        )
    };
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let advance = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::VerificationNeedsExecution,
        RunEventId::from_bytes([245; 32]),
        snapshot,
        at(20)?,
        false,
    )?;
    let wrong_version = TaskLedgerStoreVersion::new(
        stored_ledger
            .version()
            .get()
            .checked_add(1)
            .ok_or("fixture version overflow")?,
    )?;
    // This is the FIRST public operation on the fresh store. It must reach the
    // real CAS transaction instead of failing at a hidden CURRENT42 opener.
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        cold.commit_ledger_action(
            &fixture.project,
            wrong_version,
            expected,
            stored_ledger.ledger(),
            &run,
            advance.event(),
        )
        .await,
        Err(AgentActionStoreFailure::LedgerVersionConflict)
    );
    Ok(())
}

fn actual_goal(command: DiscoveredCommandId) -> TestResult<GoalContract> {
    goal_for_case(
        command,
        GOAL_BYTES,
        "Verify the already implemented average behavior",
    )
}

fn goal_for_case(
    command: DiscoveredCommandId,
    original_bytes: &[u8],
    objective: &str,
) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([209; 32]);
    let criterion = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([210; 32]),
        AcceptanceCriterionStatement::try_from_string(
            "Fractional mean and empty-input error match the existing tests".to_owned(),
        )?,
    );
    let draft = |criterion, constraints| -> TestResult<GoalContractDraft> {
        Ok(GoalContractDraft::new(
            GoalObjective::try_from_string(objective.to_owned())?,
            vec![criterion],
            constraints,
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string(
                "Run both existing unittest cases with current evidence".to_owned(),
            )?,
        )?)
    };
    let provisional = GoalContract::initial(
        task,
        draft(criterion.clone(), Vec::new())?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let original =
        GoalOriginalRequest::try_from_bytes(provisional.reference(), original_bytes.to_vec())?;
    let boundary = original
        .as_str()
        .find("Do not")
        .ok_or("rule boundary missing")?;
    let criterion = criterion.with_origin(CriterionOrigin::new(
        original.range(0, boundary)?,
        CriterionEvidenceRequirement::ExistingTest {
            command,
            selector: TestCaseSelector::All,
            minimum_cases: MinimumTestCaseCount::new(2)?,
            scope: VerificationScope::Workspace,
            python_integer_translation: None,
        },
    ));
    let constraint = GoalConstraint::try_from_string("Do not create or change tests".to_owned())?
        .with_origin(original.range(boundary, original.as_bytes().len())?);
    Ok(GoalContract::initial(
        task,
        draft(criterion, vec![constraint])?,
        provisional.created_at(),
    )
    .with_original(original)?)
}

fn execution_ids(base: u8) -> MutationExecutionIds {
    MutationExecutionIds::new(
        PolicyDecisionId::from_bytes([base; 32]),
        ApprovalRequestId::from_bytes([base + 1; 32]),
        RunEventId::from_bytes([base + 2; 32]),
        RunEventId::from_bytes([base + 3; 32]),
        ToolRunId::from_bytes([base + 4; 32]),
        RunEventId::from_bytes([base + 5; 32]),
        RunEventId::from_bytes([base + 6; 32]),
        RunEventId::from_bytes([base + 7; 32]),
        RunEventId::from_bytes([base + 8; 32]),
        VerificationRunId::from_bytes([base + 9; 32]),
        StepVerificationId::from_bytes([base + 10; 32]),
    )
}
fn at(value: u64) -> TestResult<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(value)?)
}

fn profile() -> TestResult<ModelProfile> {
    // A valid inert seed is required by the existing controller constructor;
    // no capability probe, provider or model call is executed by this fixture.
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("native-existing-test-fixture".to_owned())?,
        ModelId::try_from_string("not-invoked".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(8192)?,
            ModelOutputLimit::new(2048)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

#[derive(Debug, Default)]
struct UnexpectedContext {
    calls: std::sync::atomic::AtomicUsize,
}
impl AgentContextCompiler for UnexpectedContext {
    fn compile<'a>(
        &'a self,
        _: &'a AgentContextCompileInput,
        _: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ContextCompileFailure::InvalidPack) })
    }
}
#[derive(Debug, Default)]
struct BoundedEvents {
    values: Mutex<Vec<ProcessEvent>>,
}
impl ProcessEventSink for BoundedEvents {
    fn emit(&self, event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        let mut values = self.values.lock().map_err(|_| ProcessEventSinkError)?;
        if values.len() >= 128 {
            return Err(ProcessEventSinkError);
        }
        values.push(event);
        Ok(())
    }
}
#[derive(Debug)]
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
    fn report_phase(&self, _: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}
impl ProcessRunControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn wait_cancelled_timeout(&self, wait: Duration) -> bool {
        std::thread::sleep(wait);
        false
    }
}
impl RepositoryIndexControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
    fn report_phase(&self, _: RepositoryIndexPhase) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}
impl WorkspacePatchControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}

struct FixtureGitChild(std::process::Child);
impl Drop for FixtureGitChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn initialize_git(root: &Path) -> TestResult {
    let mut child = FixtureGitChild(
        Command::new("git")
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "init.templateDir=",
                "init",
                "--quiet",
            ])
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err("native Git fixture initialization failed".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("native Git fixture initialization timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn inventory(root: &Path) -> TestResult<Vec<(PathBuf, Vec<u8>)>> {
    fn visit(root: &Path, dir: &Path, values: &mut Vec<(PathBuf, Vec<u8>)>) -> TestResult {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(root, &path, values)?;
            } else {
                values.push((path.strip_prefix(root)?.to_path_buf(), fs::read(path)?));
            }
        }
        Ok(())
    }
    let mut values = Vec::new();
    visit(root, root, &mut values)?;
    values.sort();
    Ok(values)
}
