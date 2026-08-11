use super::*;
use a3_application::{AcceptanceVerificationRequestError, AgentActionStore};
use a3_domain::{RunEventCode, RunEventOutcome, RunEventPayload, TaskReplanReason};

const PYPROJECT: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/replan-user-edit/pyproject.toml");
const RUNNER: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/replan-user-edit/pytest.py");
const TEST: &[u8] = include_bytes!(
    "../../../../fixtures/agent-coding-eval-v1/replan-user-edit/tests/test_average.py"
);
const ORIGINAL: &[u8] =
    include_bytes!("../../../../fixtures/agent-coding-eval-v1/replan-user-edit/average.py");
const WRONG_FIX: &[u8] =
    b"def average(values: list[int]) -> float:\n    return round(sum(values) / len(values))\n";
const CORRECT_FIX: &[u8] =
    b"def average(values: list[int]) -> float:\n    return sum(values) / len(values)\n";
const USER_EDIT: &[u8] = b"Keep fractional averages for reporting.\n";

pub(super) async fn evaluate() -> Result<CodingEvalResult, Box<dyn Error>> {
    let case = initial_case();
    let fixture = CodingFixture::new(case.files).await?;
    let catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &fixture.published)?;
    let command = catalog
        .commands()
        .iter()
        .find(|command| command.kind() == DiscoveredCommandKind::Test)
        .ok_or_else(|| test_error("replan fixture test command was not discovered"))?;
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
    let failed_step_id = TaskStepId::from_bytes(id(21));
    let failed_spec_id = VerificationSpecId::from_bytes(id(22));
    let failed_spec = VerificationSpec::command(
        failed_spec_id,
        requirement("fractional average test passes")?,
        command.id(),
        VerificationScope::Workspace,
    );
    let mut durable =
        DurableCodingTask::new(&fixture, case, criterion_id, failed_step_id, failed_spec).await?;
    let wrong_patch = PatchAction::new(
        PatchActionSchemaVersion::V1,
        durable.run.id(),
        fixture.project.worktree().id(),
        fixture.published.run().snapshot_id(),
        failed_step_id,
        failed_spec_id,
        PatchRationale::try_from_string(case.patch_rationale.to_owned())?,
        case.patches
            .iter()
            .copied()
            .map(EvalPatch::operation)
            .collect::<Result<Vec<_>, _>>()?,
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
    let mut compiler = compiler()?;
    let seed = durable.context_seed();
    let first = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &fixture.published,
            AgentAction::ApplyPatch(Box::new(wrong_patch.clone())),
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
        return Err(test_error("failed-plan patch bypassed explicit approval"));
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
            AgentAction::ApplyPatch(Box::new(wrong_patch)),
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
            "failed-plan patch did not request its Must-test",
        ));
    }

    let wrong_index = latest_index(&fixture).await?;
    let selection = MutationCommandSelection::new(&catalog, &confirmation);
    let first_failure = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &wrong_index,
            AgentAction::Run(AgentRunAction::new(failed_step_id, command.id())),
            Some(selection),
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(100),
            timestamp(30)?,
            timestamp(100)?,
            &seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    if !matches!(first_failure, MutationControllerOutcome::NextAction(_))
        || durable.run.state() != AgentControllerState::Execute
    {
        return Err(test_error(
            "first failed Must-test did not take one bounded retry",
        ));
    }
    reject_red_completion(&fixture, &mut durable, &wrong_index).await?;
    let second_failure = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &wrong_index,
            AgentAction::Run(AgentRunAction::new(failed_step_id, command.id())),
            Some(selection),
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(120),
            timestamp(31)?,
            timestamp(100)?,
            &seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    if !matches!(
        second_failure,
        MutationControllerOutcome::ReplanRequired { .. }
    ) || durable.run.state() != AgentControllerState::Replan
        || !durable
            .ledger
            .step(failed_step_id)
            .is_some_and(has_failed_verification)
    {
        return Err(std::io::Error::other(format!(
            "repeated failed Must-test did not require Replan: outcome={second_failure:?}, state={:?}, step={:?}",
            durable.run.state(),
            durable.ledger.step(failed_step_id).map(|step| step.status())
        ))
        .into());
    }

    fixture.repository.write("USER_NOTES.md", USER_EDIT)?;
    let user_index = refresh
        .execute(
            &fixture.project,
            &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
            &mut compiler,
            &ActiveControl,
        )
        .await?
        .published_index()
        .clone();
    let user_revision = FileRevision::new(path("USER_NOTES.md")?, hash(USER_EDIT));
    if user_index.run().snapshot_id() == wrong_index.run().snapshot_id()
        || !user_index
            .publication()
            .graph()
            .files()
            .contains(&user_revision)
    {
        return Err(test_error(
            "intermediate user edit was not observed before Replan",
        ));
    }
    let replacement_step_id = TaskStepId::from_bytes(id(140));
    let replacement_spec_id = VerificationSpecId::from_bytes(id(141));
    let replacement = TaskStepDefinition::new(
        replacement_step_id,
        None,
        TaskStepOutcome::try_from_string("preserve fractional averages".to_owned())?,
        TaskStepRationale::try_from_string("replace the disproven rounding plan".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "fractional average test result".to_owned(),
        )?],
        VerificationSpec::command(
            replacement_spec_id,
            requirement("fractional average test passes")?,
            command.id(),
            VerificationScope::Workspace,
        ),
    )?
    .with_acceptance_criteria(vec![criterion_id])?;
    apply_replan(
        &fixture,
        &mut durable,
        failed_step_id,
        replacement,
        replacement_step_id,
        user_index.run().snapshot_id(),
    )
    .await?;

    let fresh_catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &user_index)?;
    let fresh_command = fresh_catalog
        .commands()
        .iter()
        .find(|candidate| candidate.kind() == DiscoveredCommandKind::Test)
        .ok_or_else(|| test_error("replanned test command disappeared"))?;
    let correct_patch = PatchAction::new(
        PatchActionSchemaVersion::V1,
        durable.run.id(),
        fixture.project.worktree().id(),
        user_index.run().snapshot_id(),
        replacement_step_id,
        replacement_spec_id,
        PatchRationale::try_from_string(
            "replace rounding with exact fractional division".to_owned(),
        )?,
        vec![PatchOperation::Update(PatchUpdate::new(
            FileRevision::new(path("average.py")?, hash(WRONG_FIX)),
            PatchFileContent::try_from_bytes(CORRECT_FIX.to_vec())?,
        )?)],
    )?;
    let replanned_seed = durable.context_seed();
    let approval_request = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &user_index,
            AgentAction::ApplyPatch(Box::new(correct_patch.clone())),
            None,
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(160),
            timestamp(40)?,
            timestamp(100)?,
            &replanned_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request_id) = approval_request else {
        return Err(test_error("replanned patch bypassed explicit approval"));
    };
    let mut approval = GrantPolicyApproval::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            &mut durable.run,
            request_id,
            ApprovalId::from_bytes(id(190)),
            RunEventId::from_bytes(id(191)),
            user_index.run().snapshot_id(),
            timestamp(41)?,
        )
        .await?;
    let patched = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &user_index,
            AgentAction::ApplyPatch(Box::new(correct_patch)),
            None,
            &WorkspacePolicy::unrestricted(),
            Some(&mut approval),
            mutation_ids(200),
            timestamp(42)?,
            timestamp(100)?,
            &replanned_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    if !matches!(patched, MutationControllerOutcome::NextAction(_)) {
        return Err(test_error("replanned patch did not request verification"));
    }
    let corrected_index = latest_index(&fixture).await?;
    let verified = controller
        .execute(
            &fixture.project,
            &mut durable.run,
            &mut durable.ledger,
            &mut durable.ledger_version,
            &corrected_index,
            AgentAction::Run(AgentRunAction::new(replacement_step_id, fresh_command.id())),
            Some(MutationCommandSelection::new(&fresh_catalog, &confirmation)),
            &WorkspacePolicy::unrestricted(),
            None,
            mutation_ids(220),
            timestamp(43)?,
            timestamp(100)?,
            &replanned_seed,
            &mut compiler,
            &NoopProcessEvents,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified { evidence_id, .. } = verified else {
        return Err(test_error("replanned Must-test did not verify"));
    };

    accept(&fixture, &mut durable, &corrected_index).await?;
    evaluate_result(
        &fixture,
        &durable,
        failed_step_id,
        replacement_step_id,
        evidence_id,
    )
    .await
}

fn initial_case() -> CodingCase {
    CodingCase {
        id: "replan-user-edit",
        objective: "preserve fractional averages",
        step_outcome: "repair average calculation",
        patch_rationale: "round the computed average to avoid integer truncation",
        files: &[
            FixtureFile {
                path: "pyproject.toml",
                content: PYPROJECT,
            },
            FixtureFile {
                path: "average.py",
                content: ORIGINAL,
            },
            FixtureFile {
                path: "pytest.py",
                content: RUNNER,
            },
            FixtureFile {
                path: "tests/test_average.py",
                content: TEST,
            },
        ],
        patches: &[EvalPatch::Update {
            before: FixtureFile {
                path: "average.py",
                content: ORIGINAL,
            },
            after: FixtureFile {
                path: "average.py",
                content: WRONG_FIX,
            },
        }],
        preserved: &[],
    }
}

async fn apply_replan(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    failed_step_id: TaskStepId,
    replacement: TaskStepDefinition,
    replacement_step_id: TaskStepId,
    snapshot_id: a3_domain::SnapshotId,
) -> Result<(), Box<dyn Error>> {
    let mut next_ledger = durable.ledger.clone();
    let revision = next_ledger.replan(
        vec![failed_step_id],
        vec![replacement],
        TaskReplanReason::try_from_string(
            "the rounding plan failed the required fractional test".to_owned(),
        )?,
        TaskLedgerTimestamp::from_unix_millis(32)?,
    )?;
    let expected_sequence = durable.run.last_event_sequence();
    let mut next_run = durable.run.clone();
    let event = next_run.record_ledger_update(
        RunEventId::from_bytes(id(150)),
        revision,
        RunEventPayload::new(
            RunEventCode::ControllerDecision,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        snapshot_id,
        timestamp(32)?,
    )?;
    let next_version = fixture
        .store
        .commit_ledger_action(
            &fixture.project,
            durable.ledger_version,
            expected_sequence,
            &next_ledger,
            &next_run,
            &event,
        )
        .await?;
    durable.ledger = next_ledger;
    durable.run = next_run;
    durable.ledger_version = next_version;
    advance(
        fixture.store.as_ref(),
        &fixture.project,
        &mut durable.run,
        AgentControllerSignal::ReplanApplied,
        RunEventId::from_bytes(id(151)),
        timestamp(33)?,
    )
    .await?;
    advance(
        fixture.store.as_ref(),
        &fixture.project,
        &mut durable.run,
        AgentControllerSignal::LocalizationComplete,
        RunEventId::from_bytes(id(152)),
        timestamp(34)?,
    )
    .await?;

    let expected_sequence = durable.run.last_event_sequence();
    let mut executing_run = durable.run.clone();
    let transition = AdvanceAgentController.execute(
        &mut executing_run,
        AgentControllerSignal::PlanReady,
        RunEventId::from_bytes(id(153)),
        snapshot_id,
        timestamp(35)?,
        false,
    )?;
    let mut executing_ledger = durable.ledger.clone();
    executing_ledger.start_step(
        replacement_step_id,
        durable.run.id(),
        TaskLedgerTimestamp::from_unix_millis(35)?,
    )?;
    let next_version = fixture
        .store
        .commit_ledger_action(
            &fixture.project,
            durable.ledger_version,
            expected_sequence,
            &executing_ledger,
            &executing_run,
            transition.event(),
        )
        .await?;
    durable.ledger = executing_ledger;
    durable.run = executing_run;
    durable.ledger_version = next_version;
    Ok(())
}

async fn reject_red_completion(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    index: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    advance(
        fixture.store.as_ref(),
        &fixture.project,
        &mut durable.run,
        AgentControllerSignal::TurnNeedsVerification,
        RunEventId::from_bytes(id(111)),
        timestamp(30)?,
    )
    .await?;
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
    );
    if !matches!(
        request,
        Err(AcceptanceVerificationRequestError::IncompleteLedger)
    ) || durable.run.state() == AgentControllerState::Done
    {
        return Err(test_error("red Must-test was allowed to complete the task"));
    }
    advance(
        fixture.store.as_ref(),
        &fixture.project,
        &mut durable.run,
        AgentControllerSignal::VerificationNeedsExecution,
        RunEventId::from_bytes(id(112)),
        timestamp(30)?,
    )
    .await?;
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
            RunEventId::from_bytes(id(240)),
            timestamp(50)?,
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
    failed_step_id: TaskStepId,
    replacement_step_id: TaskStepId,
    evidence_id: a3_domain::TaskEvidenceId,
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
    let failed = durable
        .ledger
        .step(failed_step_id)
        .ok_or_else(|| test_error("failed plan step disappeared"))?;
    let replacement = durable
        .ledger
        .step(replacement_step_id)
        .ok_or_else(|| test_error("replacement plan step disappeared"))?;
    let verification = replacement
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
    let replan_event = events.events().iter().any(|event| {
        matches!(
            event.kind(),
            RunEventKind::LedgerUpdated { from, to }
                if from.get() == 1 && to.get() == 2
        )
    });
    Ok(CodingEvalResult {
        id: "replan-user-edit",
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
        step: has_failed_verification(failed)
            && failed.retired_in_revision().is_some()
            && replacement.status() == TaskStepStatus::Completed,
        patch: std::fs::read(fixture.repository.path().join("average.py"))? == CORRECT_FIX
            && replan_event,
        evidence: verification.is_some_and(|value| value.evidence_ids() == [evidence_id]),
        verification: verification.is_some_and(|value| value.passed()),
        foreign_change_preserved: std::fs::read(fixture.repository.path().join("USER_NOTES.md"))?
            == USER_EDIT,
        replan_count: durable.ledger.replans().len(),
        compaction_count: 0,
    })
}

fn has_failed_verification(step: &a3_domain::TaskStep) -> bool {
    step.attempts()
        .iter()
        .filter_map(|attempt| attempt.verification())
        .any(|verification| !verification.passed())
}
