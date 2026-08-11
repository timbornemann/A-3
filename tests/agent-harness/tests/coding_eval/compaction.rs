use super::*;
use a3_application::{AgentActionStore, AgentContextCompileInput, AgentContextCompiler};
use a3_domain::{RunEventCode, RunEventOutcome, RunEventPayload, StepDependency};

const PYPROJECT: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/context-compaction/pyproject.toml");
const RUNNER: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/context-compaction/pytest.py");
const ORIGINAL: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/context-compaction/arithmetic.py");
const EXISTING_TEST: &[u8] = include_bytes!(
    "../../../../fixtures/agent-coding-eval-v1/context-compaction/tests/test_double.py"
);
const UPDATED: &[u8] = b"def double(value: int) -> int:\n    return value * 2\n\n\ndef triple(value: int) -> int:\n    return value * 3\n";
const ADDED_TEST: &[u8] =
    b"from arithmetic import triple\n\n\ndef test_triple() -> None:\n    assert triple(4) == 12\n";

pub(super) async fn evaluate() -> Result<CodingEvalResult, Box<dyn Error>> {
    let fixture = CodingFixture::new(&[
        FixtureFile {
            path: "pyproject.toml",
            content: PYPROJECT,
        },
        FixtureFile {
            path: "arithmetic.py",
            content: ORIGINAL,
        },
        FixtureFile {
            path: "pytest.py",
            content: RUNNER,
        },
        FixtureFile {
            path: "tests/test_double.py",
            content: EXISTING_TEST,
        },
    ])
    .await?;
    let catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &fixture.published)?;
    let command = catalog
        .commands()
        .iter()
        .find(|candidate| candidate.kind() == DiscoveredCommandKind::Test)
        .ok_or_else(|| test_error("compaction fixture test command was not discovered"))?;
    let confirmation = ConfirmProjectCommandAllowlist::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            &catalog,
            vec![command.id()],
            timestamp(10)?,
            None,
        )
        .await?;
    let first_criterion = AcceptanceCriterionId::from_bytes(id(20));
    let first_step_id = TaskStepId::from_bytes(id(21));
    let first_spec_id = VerificationSpecId::from_bytes(id(22));
    let second_criterion = AcceptanceCriterionId::from_bytes(id(23));
    let second_step_id = TaskStepId::from_bytes(id(24));
    let second_spec_id = VerificationSpecId::from_bytes(id(25));
    let mut durable = create_durable_task(
        &fixture,
        command.id(),
        first_criterion,
        first_step_id,
        first_spec_id,
        second_criterion,
        second_step_id,
        second_spec_id,
    )
    .await?;

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
    let mut compiler = compiler()?;
    let seed = durable.context_seed();
    let first_verified = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &fixture.published,
            AgentAction::Run(AgentRunAction::new(first_step_id, command.id())),
            Some(MutationCommandSelection::new(&catalog, &confirmation)),
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
    let MutationControllerOutcome::StepVerified {
        evidence_id: first_evidence_id,
        ..
    } = first_verified
    else {
        return Err(test_error("pre-compaction step did not verify"));
    };
    if durable
        .ledger
        .step(second_step_id)
        .map(|step| step.status())
        != Some(TaskStepStatus::Ready)
    {
        return Err(test_error(
            "second step was not ready for compacted continuation",
        ));
    }

    let memory = RunMemoryCheckpoint::compile(
        &durable.goal,
        &durable.ledger,
        &durable.run,
        &fixture.published,
        Vec::new(),
    )?;
    let repeated_memory = RunMemoryCheckpoint::compile(
        &durable.goal,
        &durable.ledger,
        &durable.run,
        &fixture.published,
        Vec::new(),
    )?;
    if memory.digest() != repeated_memory.digest()
        || memory.step_results().len() != 1
        || !memory.open_issues().is_empty()
    {
        return Err(test_error(
            "authoritative compaction was not deterministic or complete",
        ));
    }
    start_second_step(&fixture, &mut durable, second_step_id).await?;
    let compile_input = AgentContextCompileInput::new(
        fixture.project.clone(),
        durable.goal.clone(),
        durable.ledger.clone(),
        second_step_id,
        durable.profile.clone(),
        Some(memory.clone()),
        Vec::new(),
        Vec::new(),
    )?;
    let compacted = context.compile(&compile_input, &ActiveControl).await?;
    let repeated = context.compile(&compile_input, &ActiveControl).await?;
    let context_has_memory = compacted.request().messages().iter().any(|message| {
        message.content().contains("[RUN_MEMORY]")
            && message.content().contains("protect arithmetic behavior")
            && message.content().contains("current_status=completed")
    });
    if compacted.digest() != repeated.digest()
        || compacted.run_memory_digest() != Some(memory.digest())
        || compacted.current_step_id() != second_step_id
        || !context_has_memory
    {
        return Err(test_error(
            "compacted context did not retain the durable task anchors",
        ));
    }
    record_compacted_context(&fixture, &mut durable, &compacted).await?;

    let patch = PatchAction::new(
        PatchActionSchemaVersion::V1,
        durable.run.id(),
        fixture.project.worktree().id(),
        fixture.published.run().snapshot_id(),
        second_step_id,
        second_spec_id,
        PatchRationale::try_from_string(
            "add triple behavior and its focused regression test".to_owned(),
        )?,
        vec![
            PatchOperation::Update(PatchUpdate::new(
                FileRevision::new(path("arithmetic.py")?, hash(ORIGINAL)),
                PatchFileContent::try_from_bytes(UPDATED.to_vec())?,
            )?),
            PatchOperation::Add(PatchAdd::new(
                path("tests/test_triple.py")?,
                PatchFileContent::try_from_bytes(ADDED_TEST.to_vec())?,
            )),
        ],
    )?;
    let second_seed = durable.context_seed();
    let approval_request = controller
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
            mutation_ids(80),
            timestamp(32)?,
            timestamp(100)?,
            &second_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request_id) = approval_request else {
        return Err(test_error("post-compaction patch bypassed approval"));
    };
    let mut approval = GrantPolicyApproval::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            &mut durable.run,
            request_id,
            ApprovalId::from_bytes(id(100)),
            RunEventId::from_bytes(id(101)),
            fixture.published.run().snapshot_id(),
            timestamp(33)?,
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
            mutation_ids(110),
            timestamp(34)?,
            timestamp(100)?,
            &second_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    if !matches!(patched, MutationControllerOutcome::NextAction(_)) {
        return Err(test_error(
            "post-compaction patch did not request verification",
        ));
    }
    let patched_index = latest_index(&fixture).await?;
    let second_verified = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &patched_index,
            AgentAction::Run(AgentRunAction::new(second_step_id, command.id())),
            Some(MutationCommandSelection::new(&catalog, &confirmation)),
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(130),
            timestamp(35)?,
            timestamp(100)?,
            &second_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id: second_evidence_id,
        ..
    } = second_verified
    else {
        return Err(test_error("post-compaction step did not verify"));
    };
    accept(&fixture, &mut durable, &patched_index).await?;
    evaluate_result(
        &fixture,
        &durable,
        first_step_id,
        second_step_id,
        first_evidence_id,
        second_evidence_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn create_durable_task(
    fixture: &CodingFixture,
    command_id: a3_domain::DiscoveredCommandId,
    first_criterion: AcceptanceCriterionId,
    first_step_id: TaskStepId,
    first_spec_id: VerificationSpecId,
    second_criterion: AcceptanceCriterionId,
    second_step_id: TaskStepId,
    second_spec_id: VerificationSpecId,
) -> Result<DurableCodingTask, Box<dyn Error>> {
    let goal = GoalContract::initial(
        TaskId::from_bytes(*first_criterion.as_bytes()),
        GoalContractDraft::new(
            GoalObjective::try_from_string("protect arithmetic behavior".to_owned())?,
            vec![
                AcceptanceCriterion::new(
                    first_criterion,
                    AcceptanceCriterionStatement::try_from_string(
                        "existing arithmetic test passes".to_owned(),
                    )?,
                ),
                AcceptanceCriterion::new(
                    second_criterion,
                    AcceptanceCriterionStatement::try_from_string(
                        "triple behavior is tested".to_owned(),
                    )?,
                ),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("run offline arithmetic tests".to_owned())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let first_step = step_definition(
        first_step_id,
        Vec::new(),
        "verify existing arithmetic behavior",
        first_spec_id,
        command_id,
        first_criterion,
    )?;
    let second_step = step_definition(
        second_step_id,
        vec![StepDependency::new(first_step_id)],
        "add and test triple behavior",
        second_spec_id,
        command_id,
        second_criterion,
    )?;
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![first_step, second_step],
        TaskLedgerTimestamp::from_unix_millis(2)?,
    )?;
    let run_id = AgentRunId::from_bytes(*first_step_id.as_bytes());
    ledger.start_step(
        first_step_id,
        run_id,
        TaskLedgerTimestamp::from_unix_millis(3)?,
    )?;
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
    Ok(DurableCodingTask {
        goal,
        ledger,
        ledger_version,
        run,
        profile,
    })
}

fn step_definition(
    step_id: TaskStepId,
    prerequisites: Vec<StepDependency>,
    outcome: &str,
    spec_id: VerificationSpecId,
    command_id: a3_domain::DiscoveredCommandId,
    criterion_id: AcceptanceCriterionId,
) -> Result<TaskStepDefinition, Box<dyn Error>> {
    Ok(TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string(outcome.to_owned())?,
        TaskStepRationale::try_from_string("retain evidence before continuing".to_owned())?,
        prerequisites,
        vec![ExpectedTaskEvidence::try_from_string(
            "offline arithmetic test result".to_owned(),
        )?],
        VerificationSpec::command(
            spec_id,
            requirement("offline arithmetic tests pass")?,
            command_id,
            VerificationScope::Workspace,
        ),
    )?
    .with_acceptance_criteria(vec![criterion_id])?)
}

async fn start_second_step(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    step_id: TaskStepId,
) -> Result<(), Box<dyn Error>> {
    let expected_sequence = durable.run.last_event_sequence();
    let mut next_run = durable.run.clone();
    let transition = AdvanceAgentController.execute(
        &mut next_run,
        AgentControllerSignal::VerificationNeedsExecution,
        RunEventId::from_bytes(id(70)),
        durable.run.current_snapshot_id(),
        timestamp(30)?,
        false,
    )?;
    let mut next_ledger = durable.ledger.clone();
    next_ledger.start_step(
        step_id,
        durable.run.id(),
        TaskLedgerTimestamp::from_unix_millis(30)?,
    )?;
    let next_version = fixture
        .store
        .commit_ledger_action(
            &fixture.project,
            durable.ledger_version,
            expected_sequence,
            &next_ledger,
            &next_run,
            transition.event(),
        )
        .await?;
    durable.ledger = next_ledger;
    durable.run = next_run;
    durable.ledger_version = next_version;
    Ok(())
}

async fn record_compacted_context(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    compiled: &a3_application::CompiledAgentContext,
) -> Result<(), Box<dyn Error>> {
    let expected_sequence = durable.run.last_event_sequence();
    let mut next_run = durable.run.clone();
    let event = next_run.record(
        RunEventId::from_bytes(id(71)),
        RunEventKind::ContextCompiled,
        RunEventPayload::new(
            RunEventCode::ControllerDecision,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        compiled.snapshot_id(),
        None,
        timestamp(31)?,
    )?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(&fixture.project, expected_sequence, &next_run, &event)
        .await?;
    durable.run = next_run;
    Ok(())
}

async fn accept(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    index: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    let memory = RunMemoryCheckpoint::compile(
        &durable.goal,
        &durable.ledger,
        &durable.run,
        index,
        Vec::new(),
    )?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &durable.run,
        durable.goal.clone(),
        durable.ledger.clone(),
        memory,
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
    Ok(())
}

async fn evaluate_result(
    fixture: &CodingFixture,
    durable: &DurableCodingTask,
    first_step_id: TaskStepId,
    second_step_id: TaskStepId,
    first_evidence_id: a3_domain::TaskEvidenceId,
    second_evidence_id: a3_domain::TaskEvidenceId,
) -> Result<CodingEvalResult, Box<dyn Error>> {
    let stored_goal = fixture
        .store
        .load_current_goal_contract(&fixture.project, durable.goal.task_id())
        .await?;
    let stored_ledger = fixture
        .store
        .load_task_ledger(&fixture.project, durable.goal.task_id())
        .await?;
    let stored_run = fixture
        .store
        .load_agent_run(&fixture.project, durable.run.id())
        .await?;
    let first = durable
        .ledger
        .step(first_step_id)
        .ok_or_else(|| test_error("pre-compaction step disappeared"))?;
    let second = durable
        .ledger
        .step(second_step_id)
        .ok_or_else(|| test_error("post-compaction step disappeared"))?;
    let first_verification = first
        .attempts()
        .last()
        .and_then(|attempt| attempt.verification());
    let second_verification = second
        .attempts()
        .last()
        .and_then(|attempt| attempt.verification());
    let events = fixture
        .store
        .load_run_events(
            &fixture.project,
            durable.run.id(),
            None,
            RunEventPageLimit::new(128)?,
        )
        .await?;
    let compacted_context_persisted = events
        .events()
        .iter()
        .any(|event| event.id() == RunEventId::from_bytes(id(71)));
    Ok(CodingEvalResult {
        id: "context-compaction",
        final_state: if durable.run.state() == AgentControllerState::Done {
            "done"
        } else {
            "not_done"
        },
        goal: stored_goal.as_ref() == Some(&durable.goal)
            && stored_ledger.as_ref().is_some_and(|stored| {
                stored.ledger() == &durable.ledger && stored.version() == durable.ledger_version
            })
            && stored_run.as_ref() == Some(&durable.run),
        step: first.status() == TaskStepStatus::Completed
            && second.status() == TaskStepStatus::Completed,
        patch: std::fs::read(fixture.repository.path().join("arithmetic.py"))? == UPDATED
            && std::fs::read(fixture.repository.path().join("tests/test_triple.py"))? == ADDED_TEST,
        evidence: first_verification
            .is_some_and(|value| value.evidence_ids() == [first_evidence_id])
            && second_verification
                .is_some_and(|value| value.evidence_ids() == [second_evidence_id]),
        verification: first_verification.is_some_and(|value| value.passed())
            && second_verification.is_some_and(|value| value.passed()),
        foreign_change_preserved: std::fs::read(
            fixture.repository.path().join("tests/test_double.py"),
        )? == EXISTING_TEST,
        replan_count: durable.ledger.replans().len(),
        compaction_count: usize::from(compacted_context_persisted),
    })
}
