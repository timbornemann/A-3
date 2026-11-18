//! Real four-step component fixture. No model call, automatic plan generation or A10 claim.
use super::*;
use a3_application::{
    AcceptanceRejection, AcceptanceVerifier, AcceptanceVerifierOutcome, AcceptanceVerifierTimeout,
    ActivateAgentHorizonPlan, AgentHorizonFailure, AppendAgentHorizonPlan, ContinueSourceAgentPlan,
    ContinueVerifiedAgentPlanOutcome, PendingPlanningPairDisposition,
    SourcePlanContinuationOutcome, load_pending_planning_pair,
};
use a3_domain::{
    AgentRunBudget, AgentRunUsage, AgentTurnActionClass, AgentTurnCharge, AgentTurnRepairUsage,
    GoalContractReference, ModelTokenCount, PatchAction, PatchActionSchemaVersion, PatchAdd,
    PatchFileContent, PatchOperation, PatchRationale, PatchUpdate, PendingPlanningStage, RunEvent,
    RunEventPayload, StepDependency, TaskLedger, TaskLedgerReplanCause, TaskStep,
};

const COUNT_STEP: u8 = 180;
const WIRE_STEP: u8 = 181;
const CHECK_STEP: u8 = 182;
const ORIGINAL: &[u8] = b"Add total.py: total(values) returns the exact sum, including an empty list.\nAdd count.py: count(values) returns the number of values, including an empty list.\nFix average.py: average([1, 2]) returns 1.5; average([]) raises ValueError.\nIntegrate average through both helpers and prove these four obligations with the two existing tests.\nDo not create or change tests. Deliver total.py, count.py and average.py as three separate verified file results in that order; then run the existing behavioral check as a fourth step.\n";
const TOTAL: &[u8] = b"def total(values):\n    return sum(values)\n";
const COUNT: &[u8] = b"def count(values):\n    return len(values)\n";
const WIRED: &[u8] = b"from total import total\nfrom count import count\n\ndef average(values):\n    if not values:\n        raise ValueError('empty')\n    return total(values) / count(values)\n";
const EXISTING_TESTS: &[u8] = b"import unittest\nfrom unittest.mock import patch\nimport average as subject\nfrom total import total\nfrom count import count\n\nclass Behavior(unittest.TestCase):\n    def test_fractional_helpers_and_integration(self):\n        self.assertEqual(total([1, 2]), 3)\n        self.assertEqual(count([1, 2]), 2)\n        self.assertEqual(subject.average([1, 2]), 1.5)\n        with patch('average.total', return_value=10), patch('average.count', return_value=4):\n            self.assertEqual(subject.average([1, 2]), 2.5)\n    def test_empty_helpers_and_error(self):\n        self.assertEqual(total([]), 0)\n        self.assertEqual(count([]), 0)\n        with self.assertRaises(ValueError):\n            subject.average([])\n";

#[test]
fn actual_four_step_horizon_reopens_pending_and_appended_then_proves_all_original_results()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = horizon_fixture()?;
            let root = fixture.root.join("worktree");
            let mut expected = inventory(&root)?;
            expected
                .iter_mut()
                .find(|(path, _)| path == Path::new("average.py"))
                .ok_or("initial subject missing")?
                .1 = WIRED.to_vec();
            expected.push((PathBuf::from("total.py"), TOTAL.to_vec()));
            expected.push((PathBuf::from("count.py"), COUNT.to_vec()));
            expected.sort_by(|a, b| a.0.cmp(&b.0));

            // Each phase returns only immutable expectations, never an Arc/store/runner owner.
            let checkpoint = execute_three_real_patches(&fixture).await?;
            assert_eq!(inventory(&root)?, expected);
            append_after_full_reopen(&fixture, &checkpoint).await?;
            assert_eq!(inventory(&root)?, expected);
            let evidence = activate_and_execute_after_full_reopen(&fixture, &checkpoint).await?;
            assert_eq!(inventory(&root)?, expected);
            finish_reopened_source(
                &fixture,
                checkpoint.goal,
                evidence,
                checkpoint.snapshot,
                110,
            )
            .await?;
            assert_eq!(inventory(&root)?, expected);
            fixture.cleanup()?;
            Ok(())
        })
    })
}

struct Checkpoint {
    goal: GoalContract,
    request: RunEvent,
    snapshot: SnapshotId,
    run_id: AgentRunId,
    created_at: AgentRunTimestamp,
    budget: AgentRunBudget,
    usage: AgentRunUsage,
    history: Vec<TaskStep>,
    diff_ids: Vec<TaskEvidenceId>,
}

struct Effects {
    coordinator: WorktreeMutationCoordinator,
    inspection: AgentInspectionBuffer,
    approvals: AgentApprovalBuffer,
    patch: WorkspacePatchAdapter,
    runner: WorkspaceProcessRunner,
    context: UnexpectedContext,
    events: BoundedEvents,
    factory: BuiltinProcessVerificationEvidenceFactory,
}
impl Effects {
    fn new(fixture: &Fixture) -> TestResult<Self> {
        let inspection = AgentInspectionBuffer::new();
        inspection.activate_project(&fixture.project);
        let approvals = AgentApprovalBuffer::new();
        approvals.activate_project(&fixture.project);
        Ok(Self {
            coordinator: WorktreeMutationCoordinator::new(),
            inspection,
            approvals,
            patch: WorkspacePatchAdapter::new(),
            runner: native_runner()?,
            context: UnexpectedContext::default(),
            events: BoundedEvents::default(),
            factory: BuiltinProcessVerificationEvidenceFactory,
        })
    }
    fn controller<'a>(
        &'a self,
        store: &'a LibsqlKnowledgeStore,
        refresh: &'a RefreshRepositoryIndex,
    ) -> ExecuteMutatingAgentAction<'a> {
        ExecuteMutatingAgentAction::new(
            &self.coordinator,
            store,
            store,
            store,
            store,
            store,
            &self.inspection,
            &self.approvals,
            &self.patch,
            &self.runner,
            &self.factory,
            &self.context,
            refresh,
        )
        .with_permissions(store)
    }
}

async fn execute_three_real_patches(fixture: &Fixture) -> TestResult<Checkpoint> {
    let PreparedExistingRun {
        store,
        refresh,
        mut compiler,
        published,
        goal,
        mut ledger,
        mut version,
        profile,
        mut run,
        unplanned_criteria,
        ..
    } = prepare_existing_run_with_steps(
        fixture,
        profile()?,
        horizon_goal,
        &[total_step, count_step, wire_step],
    )
    .await?;
    let criteria = criterion_ids(&goal);
    assert_eq!(criteria.len(), 4);
    assert_eq!(unplanned_criteria, criteria);
    assert!(a3_domain::GoalOriginalCoverage::derive_from(&goal)?.is_complete());

    // Controlled prior history, not a provider invocation or a measured token-cost claim.
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let event = run.record_turn(
        RunEventId::from_bytes([170; 32]),
        RunEventPayload::empty(),
        snapshot,
        at(8)?,
        AgentTurnCharge::new(
            ModelTokenCount::new(400),
            ModelTokenCount::new(40),
            Some(AgentTurnActionClass::Inspect),
            AgentTurnRepairUsage::One,
        ),
    )?;
    AppendRunEvent::new(store.as_ref())
        .execute(&fixture.project, expected, &run, &event)
        .await?;
    let usage = run.usage();
    assert_eq!(usage.turn_count(), 1);
    assert_eq!(usage.prompt_tokens(), 400);
    assert_eq!(usage.output_tokens(), 40);
    assert_eq!(usage.action_count(), 1);
    assert_eq!(usage.repair_count(), 1);

    let effects = Effects::new(fixture)?;
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let mut current = published;
    let mut diff_ids = Vec::new();
    for (index, (step, path, bytes, ids, time)) in [
        (STEP, "total.py", TOTAL, 20, 10),
        (COUNT_STEP, "count.py", COUNT, 60, 30),
        (WIRE_STEP, "average.py", WIRED, 100, 50),
    ]
    .into_iter()
    .enumerate()
    {
        let step_id = TaskStepId::from_bytes([step; 32]);
        assert_eq!(
            ledger
                .step(step_id)
                .ok_or("active implementation missing")?
                .status(),
            TaskStepStatus::InProgress
        );
        let operation = if index < 2 {
            assert!(!fixture.root.join("worktree").join(path).exists());
            PatchOperation::Add(PatchAdd::new(
                RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
                PatchFileContent::try_from_bytes(bytes.to_vec())?,
            ))
        } else {
            let revision = current
                .publication()
                .graph()
                .files()
                .iter()
                .find(|r| r.path().as_bytes() == path.as_bytes())
                .ok_or("current subject missing")?
                .clone();
            assert_eq!(
                revision.content_hash().as_bytes(),
                blake3::hash(WRONG_SUBJECT).as_bytes()
            );
            PatchOperation::Update(PatchUpdate::new(
                revision,
                PatchFileContent::try_from_bytes(bytes.to_vec())?,
            )?)
        };
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            run.id(),
            fixture.project.worktree().id(),
            run.current_snapshot_id(),
            step_id,
            ledger
                .step(step_id)
                .ok_or("step missing")?
                .definition()
                .verification_spec()
                .id(),
            PatchRationale::try_from_string(
                "Apply this original-bound file result; runtime remains open".to_owned(),
            )?,
            vec![operation],
        )?;
        let before = run.current_snapshot_id();
        let outcome = execute_approved(
            effects.controller(store.as_ref(), &refresh),
            fixture,
            store.as_ref(),
            &mut run,
            &mut ledger,
            &mut version,
            &current,
            AgentAction::ApplyPatch(Box::new(action)),
            None,
            ids,
            time,
            &seed,
            &mut compiler,
            &effects.events,
        )
        .await?;
        let MutationControllerOutcome::StepVerified {
            evidence_id,
            snapshot_id,
        } = outcome
        else {
            return Err(format!(
                "real scoped patch did not produce verified progress: {outcome:?}"
            )
            .into());
        };
        assert_ne!(snapshot_id, before);
        assert_eq!(run.state(), AgentControllerState::Verify);
        assert_eq!(run.usage(), usage);
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
        assert!(matches!(state.evidence(), [VerificationEvidence::Diff(_)]));
        current = state.published_index().clone();
        diff_ids.push(evidence_id);
        if index < 2 {
            let next = [COUNT_STEP, WIRE_STEP][index];
            let outcome = ContinueSourceAgentPlan::new(store.as_ref())
                .execute(
                    &fixture.project,
                    version,
                    &goal,
                    &mut run,
                    &mut ledger,
                    snapshot_id,
                    RunEventId::from_bytes([172 + index as u8; 32]),
                    at(time + 10)?,
                    &ActiveControl,
                )
                .await?;
            let SourcePlanContinuationOutcome::Continued(
                ContinueVerifiedAgentPlanOutcome::StepStarted {
                    step_id,
                    ledger_version,
                },
            ) = outcome
            else {
                return Err("existing horizon did not start its next dependent step".into());
            };
            assert_eq!(step_id, TaskStepId::from_bytes([next; 32]));
            version = ledger_version;
        }
    }
    assert_eq!(effects.context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(ledger.steps().len(), 3);
    assert!(
        ledger
            .steps()
            .all(|s| s.status() == TaskStepStatus::Completed
                && s.definition().acceptance_criteria().is_empty())
    );
    let history = ledger.steps().cloned().collect::<Vec<_>>();
    let snapshot = current.run().snapshot_id();
    assert_no_runtime_acceptance(fixture, store.as_ref(), &goal, &ledger, &run, &current).await?;
    let frozen = (run.id(), run.created_at(), run.budget(), run.usage());
    let outcome = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            &fixture.project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot,
            RunEventId::from_bytes([174; 32]),
            at(60)?,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::PlanningRequested {
        ledger_version: _,
        request_event,
    } = outcome
    else {
        return Err("completed Addresses horizon did not request durable planning".into());
    };
    assert_eq!(run.state(), AgentControllerState::Plan);
    assert!(matches!(
        ledger
            .pending_planning()
            .ok_or("planning marker missing")?
            .stage(),
        PendingPlanningStage::NeedsPlan
    ));
    assert_eq!(
        (run.id(), run.created_at(), run.budget(), run.usage()),
        frozen
    );
    assert_eq!(ledger.steps().cloned().collect::<Vec<_>>(), history);
    Ok(Checkpoint {
        goal,
        request: request_event,
        snapshot,
        run_id: frozen.0,
        created_at: frozen.1,
        budget: frozen.2,
        usage: frozen.3,
        history,
        diff_ids,
    })
}

async fn assert_no_runtime_acceptance(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    goal: &GoalContract,
    ledger: &TaskLedger,
    run: &AgentRun,
    current: &PublishedIndex,
) -> TestResult {
    let memory = RunMemoryCheckpoint::compile(goal, ledger, run, current, Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        run,
        goal.clone(),
        ledger.clone(),
        memory,
    )?;
    assert_eq!(
        DeterministicAcceptanceVerifier::new(store)
            .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
            .await?,
        AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
    );
    Ok(())
}

struct Restored {
    store: Arc<LibsqlKnowledgeStore>,
    goal: GoalContract,
    ledger: TaskLedger,
    version: TaskLedgerStoreVersion,
    run: AgentRun,
    request: RunEvent,
    published: PublishedIndex,
}
async fn restore_pending(fixture: &Fixture, expected: &Checkpoint) -> TestResult<Restored> {
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let stored = store
        .load_task_ledger(&fixture.project, expected.goal.task_id())
        .await?
        .ok_or("durable horizon ledger missing")?;
    let goal = store
        .load_current_goal_contract(&fixture.project, expected.goal.task_id())
        .await?
        .ok_or("original goal missing after crash")?;
    assert_eq!(goal, expected.goal);
    let ledger = stored.ledger().clone();
    let (run, request, disposition) =
        load_pending_planning_pair(&fixture.project, &ledger, store.as_ref()).await?;
    assert_eq!(disposition, PendingPlanningPairDisposition::Live);
    assert_eq!(request, expected.request);
    assert_eq!(
        (run.id(), run.created_at(), run.budget(), run.usage()),
        (
            expected.run_id,
            expected.created_at,
            expected.budget,
            expected.usage
        )
    );
    assert_eq!(run.state(), AgentControllerState::Plan);
    assert!(!ledger.steps().any(|s| matches!(
        s.status(),
        TaskStepStatus::InProgress | TaskStepStatus::AwaitingApproval | TaskStepStatus::Verifying
    )));
    for prior in &expected.history {
        assert_eq!(ledger.step(prior.definition().id()), Some(prior));
    }
    let published = store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or("published source missing")?;
    assert_eq!(published.run().snapshot_id(), expected.snapshot);
    Ok(Restored {
        store,
        goal,
        ledger,
        version: stored.version(),
        run,
        request,
        published,
    })
}

async fn append_after_full_reopen(fixture: &Fixture, expected: &Checkpoint) -> TestResult {
    let Restored {
        store,
        goal,
        mut ledger,
        mut version,
        mut run,
        request,
        published,
    } = restore_pending(fixture, expected).await?;
    assert_eq!(ledger.steps().len(), 3);
    assert!(matches!(
        ledger.pending_planning().ok_or("pending missing")?.stage(),
        PendingPlanningStage::NeedsPlan
    ));
    let proof = proof_step(&goal)?;
    version = AppendAgentHorizonPlan::new(store.as_ref())
        .execute(
            &fixture.project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            published.run().snapshot_id(),
            &request,
            vec![proof.clone()],
            RunEventId::from_bytes([175; 32]),
            at(70)?,
            &ActiveControl,
        )
        .await?;
    assert_eq!(ledger.steps().len(), 4);
    assert_eq!(ledger.replans().len(), 1);
    assert_eq!(
        ledger.replans()[0].cause(),
        TaskLedgerReplanCause::HorizonExtension
    );
    assert!(
        matches!(ledger.pending_planning().ok_or("appended marker missing")?.stage(), PendingPlanningStage::PlanAppended {added_step_ids,..} if added_step_ids == &[TaskStepId::from_bytes([CHECK_STEP;32])])
    );
    let frozen_run = run.clone();
    let frozen_ledger = ledger.clone();
    assert!(matches!(
        AppendAgentHorizonPlan::new(store.as_ref())
            .execute(
                &fixture.project,
                version,
                &goal,
                &mut run,
                &mut ledger,
                published.run().snapshot_id(),
                &request,
                vec![proof],
                RunEventId::from_bytes([178; 32]),
                at(71)?,
                &ActiveControl
            )
            .await,
        Err(AgentHorizonFailure::InvalidState)
    ));
    assert_eq!(run, frozen_run);
    assert_eq!(ledger, frozen_ledger);
    assert_eq!(
        store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("appended ledger missing")?
            .version(),
        version
    );
    assert_eq!(
        store.load_agent_run(&fixture.project, run.id()).await?,
        Some(run)
    );
    Ok(())
}

async fn activate_and_execute_after_full_reopen(
    fixture: &Fixture,
    expected: &Checkpoint,
) -> TestResult<TaskEvidenceId> {
    let Restored {
        store,
        goal,
        mut ledger,
        mut version,
        mut run,
        request,
        published,
    } = restore_pending(fixture, expected).await?;
    assert_eq!(ledger.steps().len(), 4);
    assert!(matches!(
        ledger
            .pending_planning()
            .ok_or("appended checkpoint missing")?
            .stage(),
        PendingPlanningStage::PlanAppended { .. }
    ));
    version = ActivateAgentHorizonPlan::new(store.as_ref())
        .execute(
            &fixture.project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            expected.snapshot,
            &request,
            TaskStepId::from_bytes([CHECK_STEP; 32]),
            RunEventId::from_bytes([176; 32]),
            at(80)?,
            &ActiveControl,
        )
        .await?;
    assert!(ledger.pending_planning().is_none());
    let frozen_run = run.clone();
    let frozen_ledger = ledger.clone();
    assert!(matches!(
        ActivateAgentHorizonPlan::new(store.as_ref())
            .execute(
                &fixture.project,
                version,
                &goal,
                &mut run,
                &mut ledger,
                expected.snapshot,
                &request,
                TaskStepId::from_bytes([CHECK_STEP; 32]),
                RunEventId::from_bytes([179; 32]),
                at(81)?,
                &ActiveControl
            )
            .await,
        Err(AgentHorizonFailure::NoPending)
    ));
    assert_eq!(run, frozen_run);
    assert_eq!(ledger, frozen_ledger);
    let proof = ledger
        .step(TaskStepId::from_bytes([CHECK_STEP; 32]))
        .ok_or("actual proof step missing")?;
    assert_eq!(proof.attempts().len(), 1);
    assert_eq!(
        proof.definition().acceptance_criteria(),
        criterion_ids(&goal)
    );
    let check = RequestAgentFinish
        .verification_command(proof)
        .ok_or("bound test check missing")?;
    let catalog = DiscoverProjectCommands.execute(fixture.project.worktree().id(), &published)?;
    let old = CommandAllowlistStore::load_current(store.as_ref(), &fixture.project)
        .await?
        .ok_or("prior allowlist missing")?;
    let confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(
            &fixture.project,
            &catalog,
            vec![check.command_id()],
            at(85)?,
            Some(old.version()),
        )
        .await?;
    assert_ne!(
        old.allowlist().catalog_id(),
        confirmation.allowlist().catalog_id()
    );
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let effects = Effects::new(fixture)?;
    let seed = MutationContextSeed::new(goal.clone(), profile()?, Vec::new(), Vec::new());
    let outcome = execute_approved(
        effects.controller(store.as_ref(), &refresh),
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::Run(check),
        Some((&catalog, &confirmation)),
        140,
        90,
        &seed,
        &mut compiler,
        &effects.events,
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("actual fourth-step test did not verify: {outcome:?}").into());
    };
    assert_eq!(snapshot_id, expected.snapshot);
    assert!(!expected.diff_ids.contains(&evidence_id));
    assert_eq!(effects.context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.usage(), expected.usage);
    for prior in &expected.history {
        assert_eq!(ledger.step(prior.definition().id()), Some(prior));
    }
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
    let [VerificationEvidence::Test(test)] = state.evidence() else {
        return Err("real fourth-step TestEvidence missing".into());
    };
    assert_eq!(test.cases().len(), 2);
    let outcome = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            &fixture.project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            RunEventId::from_bytes([177; 32]),
            at(100)?,
            &ActiveControl,
        )
        .await?;
    assert_eq!(
        outcome,
        SourcePlanContinuationOutcome::Continued(
            ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance
        )
    );
    let memory =
        RunMemoryCheckpoint::compile(&goal, &ledger, &run, state.published_index(), Vec::new())?;
    let request =
        AcceptanceVerificationRequest::new(fixture.project.clone(), &run, goal, ledger, memory)?;
    let AcceptanceVerifierOutcome::Accepted(receipt) =
        DeterministicAcceptanceVerifier::new(store.as_ref())
            .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
            .await?
    else {
        return Err("all four original results did not receive current functional proof".into());
    };
    assert_eq!(receipt.criteria().len(), 4);
    for criterion in receipt.criteria() {
        assert_eq!(criterion.evidence_ids(), [evidence_id]);
    }
    Ok(evidence_id)
}

fn horizon_fixture() -> TestResult<Fixture> {
    let mut fixture = Fixture::new()?;
    let root = fixture.root.join("worktree");
    initialize_git(&root)?;
    fs::create_dir(root.join("tests"))?;
    fs::write(root.join("average.py"), WRONG_SUBJECT)?;
    fs::write(root.join("tests/__init__.py"), b"")?;
    fs::write(root.join("tests/test_average.py"), EXISTING_TESTS)?;
    fixture.project = RepositoryInspector::new().inspect(&root)?;
    Ok(fixture)
}

fn criterion_ids(goal: &GoalContract) -> Vec<AcceptanceCriterionId> {
    goal.draft()
        .acceptance_criteria()
        .iter()
        .map(|c| c.id())
        .collect()
}
fn horizon_goal(command: DiscoveredCommandId) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([209; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        ORIGINAL.to_vec(),
    )?;
    let mut position = 0;
    let mut criteria = Vec::new();
    for (index, line) in original.as_str().split_inclusive('\n').take(4).enumerate() {
        let end = position + line.len();
        criteria.push(
            AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([210 + index as u8; 32]),
                AcceptanceCriterionStatement::try_from_string(line.to_owned())?,
            )
            .with_origin(CriterionOrigin::new(
                original.range(position, end)?,
                CriterionEvidenceRequirement::ExistingTest {
                    command,
                    selector: TestCaseSelector::All,
                    minimum_cases: MinimumTestCaseCount::new(2)?,
                    scope: VerificationScope::Workspace,
                    python_integer_translation: None,
                },
            )),
        );
        position = end;
    }
    let constraint = GoalConstraint::try_from_string(
        "Preserve tests and deliver three separately verified file results before runtime proof"
            .to_owned(),
    )?
    .with_origin(original.range(position, ORIGINAL.len())?);
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string(
            "Implement three required modules and prove all four original results".to_owned(),
        )?,
        criteria,
        vec![constraint],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Both protected existing tests prove all helpers and integration".to_owned(),
        )?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
            .with_original(original)?,
    )
}

fn total_step(goal: &GoalContract, _: DiscoveredCommandId) -> TestResult<TaskStepDefinition> {
    file_step(goal, STEP, 190, "total.py", &[], &[0])
}
fn count_step(goal: &GoalContract, _: DiscoveredCommandId) -> TestResult<TaskStepDefinition> {
    file_step(goal, COUNT_STEP, 191, "count.py", &[STEP], &[1])
}
fn wire_step(goal: &GoalContract, _: DiscoveredCommandId) -> TestResult<TaskStepDefinition> {
    file_step(goal, WIRE_STEP, 192, "average.py", &[COUNT_STEP], &[2, 3])
}
fn file_step(
    goal: &GoalContract,
    id: u8,
    spec: u8,
    path: &str,
    dependencies: &[u8],
    addressed: &[usize],
) -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([id; 32]),
        None,
        TaskStepOutcome::try_from_string(format!("Deliver the required {path} implementation"))?,
        TaskStepRationale::try_from_string(
            "One separate original file obligation; final behavior remains unproven".to_owned(),
        )?,
        dependencies
            .iter()
            .map(|id| StepDependency::new(TaskStepId::from_bytes([*id; 32])))
            .collect(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Actual current exact-path patch evidence".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([spec; 32]),
            VerificationRequirement::try_from_string(format!(
                "Exactly {path} changes in this delivery"
            ))?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?],
            )?,
        ),
    )?
    .with_criterion_coverage(
        addressed
            .iter()
            .map(|i| goal.draft().acceptance_criteria()[*i].id())
            .collect(),
        Vec::new(),
    )?)
}
fn proof_step(goal: &GoalContract) -> TestResult<TaskStepDefinition> {
    let CriterionEvidenceRequirement::ExistingTest { command, .. } =
        goal.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("original test obligation missing")?
            .evidence_requirement()
    else {
        return Err("actual existing-test fixture required".into());
    };
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([CHECK_STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Prove all four original helper and behavior results".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Current runtime evidence follows the three verified source results".to_owned(),
        )?,
        vec![StepDependency::new(TaskStepId::from_bytes([WIRE_STEP; 32]))],
        vec![ExpectedTaskEvidence::try_from_string(
            "Two actual protected tests on the current snapshot".to_owned(),
        )?],
        VerificationSpec::test(
            VerificationSpecId::from_bytes([193; 32]),
            VerificationRequirement::try_from_string(
                "Both helper, integration and empty-input behavioral cases pass".to_owned(),
            )?,
            *command,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(2)?,
            VerificationScope::Workspace,
        ),
    )?
    .with_criterion_coverage(Vec::new(), criterion_ids(goal))?)
}
