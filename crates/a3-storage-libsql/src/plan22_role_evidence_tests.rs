//! Actual evidence-role consumer; explicit test-driven replan is not automatic horizon execution.
use super::*;
use a3_application::{
    AcceptanceRejection, AcceptanceVerifier, AcceptanceVerifierOutcome, AcceptanceVerifierTimeout,
    ApplyAgentPlanRevision,
};
use a3_domain::{StepDependency, TaskReplanReason};

const PROOF_STEP: u8 = 162;

#[test]
fn actual_addresses_diff_keeps_runtime_open_until_a_later_actual_test_proves_it() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = wrong_fixture()?;
            let worktree = fixture.root.join("worktree");
            let mut expected = inventory(&worktree)?;
            expected
                .iter_mut()
                .find(|(path, _)| path == Path::new("average.py"))
                .ok_or("subject missing")?
                .1 = SUBJECT.to_vec();
            let (goal, evidence, snapshot) = run_role_consumer(&fixture).await?;
            assert_eq!(inventory(&worktree)?, expected);
            finish_reopened_source(&fixture, goal, evidence, snapshot, 40).await?;
            assert_eq!(inventory(&worktree)?, expected);
            fixture.cleanup()?;
            Ok(())
        })
    })
}

async fn run_role_consumer(
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
        unplanned_criteria,
    } = prepare_existing_run_with_step(fixture, profile()?, repair_goal, addresses_step).await?;
    let criterion = goal.draft().acceptance_criteria()[0].id();
    assert_eq!(unplanned_criteria, [criterion]);
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::new();
    let runner = native_runner()?;
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
        &approvals,
        &patch,
        &runner,
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let step_id = TaskStepId::from_bytes([STEP; 32]);
    let before_step = ledger.step(step_id).ok_or("addresses step missing")?;
    assert_eq!(before_step.definition().addressed_criteria(), [criterion]);
    assert!(before_step.definition().acceptance_criteria().is_empty());
    assert!(
        goal.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("origin missing")?
            .evidence_requirement()
            .validate_spec(before_step.definition().verification_spec())
            .is_err()
    );
    let original_revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path().as_bytes() == b"average.py")
        .ok_or("subject revision missing")?
        .clone();
    assert_eq!(
        original_revision.content_hash().as_bytes(),
        blake3::hash(WRONG_SUBJECT).as_bytes()
    );
    let action = a3_domain::PatchAction::new(
        a3_domain::PatchActionSchemaVersion::V1,
        run.id(),
        fixture.project.worktree().id(),
        run.current_snapshot_id(),
        step_id,
        before_step.definition().verification_spec().id(),
        a3_domain::PatchRationale::try_from_string(
            "Make the requested scoped source change; runtime remains unproven".to_owned(),
        )?,
        vec![a3_domain::PatchOperation::Update(
            a3_domain::PatchUpdate::new(
                original_revision,
                a3_domain::PatchFileContent::try_from_bytes(SUBJECT.to_vec())?,
            )?,
        )],
    )?;
    let outcome = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::ApplyPatch(Box::new(action)),
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id: diff_id,
        snapshot_id: changed_snapshot,
    } = outcome
    else {
        return Err(
            format!("actual addresses diff did not verify its own spec: {outcome:?}").into(),
        );
    };
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_ne!(changed_snapshot, published.run().snapshot_id());
    let state = store
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[diff_id],
            changed_snapshot,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    assert!(matches!(state.evidence(), [VerificationEvidence::Diff(_)]));
    let addresses = ledger.step(step_id).ok_or("addresses result missing")?;
    assert_eq!(addresses.status(), TaskStepStatus::Completed);
    assert!(
        addresses
            .attempts()
            .last()
            .and_then(|attempt| attempt.verification())
            .is_some_and(|result| result.passed())
    );
    assert!(
        ledger
            .steps()
            .all(|step| step.definition().acceptance_criteria().is_empty())
    );
    let addresses_history = addresses.attempts().to_vec();
    let memory =
        RunMemoryCheckpoint::compile(&goal, &ledger, &run, state.published_index(), Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &run,
        goal.clone(),
        ledger.clone(),
        memory,
    )?;
    let verifier = DeterministicAcceptanceVerifier::new(store.as_ref());
    assert_eq!(
        verifier
            .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
            .await?,
        AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
    );
    {
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let restored = cold
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("roles missing after reopen")?;
        let restored_step = restored
            .ledger()
            .step(step_id)
            .ok_or("restored addresses missing")?;
        assert_eq!(restored_step.definition().addressed_criteria(), [criterion]);
        assert!(restored_step.definition().acceptance_criteria().is_empty());
        assert_eq!(restored_step.status(), TaskStepStatus::Completed);
        assert_eq!(
            cold.load_agent_run(&fixture.project, run.id())
                .await?
                .ok_or("open run missing")?
                .state(),
            AgentControllerState::Verify
        );
    }

    // This concrete test explicitly asks for a replan and appends the proof.
    // It does not qualify an automatic horizon selector, restart or budget loop.
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let decision = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::VerificationNeedsReplan,
        RunEventId::from_bytes([170; 32]),
        snapshot,
        at(17)?,
        false,
    )?;
    AppendRunEvent::new(store.as_ref())
        .execute(&fixture.project, expected, &run, decision.event())
        .await?;
    let proof = proof_step(&goal, check_command(&goal)?)?;
    version = ApplyAgentPlanRevision::new(store.as_ref())
        .execute(
            &fixture.project,
            version,
            &mut run,
            &mut ledger,
            Vec::new(),
            vec![proof],
            TaskReplanReason::try_from_string(
                "Append the missing runtime proof while retaining verified source progress"
                    .to_owned(),
            )?,
            RunEventId::from_bytes([171; 32]),
            at(18)?,
            &ActiveControl,
        )
        .await?;
    assert_eq!(
        ledger
            .step(step_id)
            .ok_or("retained addresses missing")?
            .attempts(),
        addresses_history
    );
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let advance = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::ReplanApplied,
        RunEventId::from_bytes([172; 32]),
        snapshot,
        at(19)?,
        false,
    )?;
    AppendRunEvent::new(store.as_ref())
        .execute(&fixture.project, expected, &run, advance.event())
        .await?;
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let advance = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::LocalizationComplete,
        RunEventId::from_bytes([173; 32]),
        snapshot,
        at(20)?,
        false,
    )?;
    AppendRunEvent::new(store.as_ref())
        .execute(&fixture.project, expected, &run, advance.event())
        .await?;
    let expected = run.last_event_sequence();
    ledger.start_step(
        TaskStepId::from_bytes([PROOF_STEP; 32]),
        run.id(),
        TaskLedgerTimestamp::from_unix_millis(21)?,
    )?;
    let snapshot = run.current_snapshot_id();
    let advance = a3_application::AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::PlanReady,
        RunEventId::from_bytes([174; 32]),
        snapshot,
        at(21)?,
        false,
    )?;
    version = store
        .commit_ledger_action(
            &fixture.project,
            version,
            expected,
            &ledger,
            &run,
            advance.event(),
        )
        .await?;
    let current = store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or("current index missing")?;
    let current_catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &current)?;
    assert_ne!(catalog.id(), current_catalog.id());
    let check = RequestAgentFinish
        .verification_command(
            ledger
                .step(TaskStepId::from_bytes([PROOF_STEP; 32]))
                .ok_or("proof step missing")?,
        )
        .ok_or("bound runtime test missing")?;
    let current_confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(
            &fixture.project,
            &current_catalog,
            vec![check.command_id()],
            at(22)?,
            Some(confirmation.version()),
        )
        .await?;
    let outcome = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(check),
        Some((&current_catalog, &current_confirmation)),
        100,
        30,
        &seed,
        &mut compiler,
        &events,
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("later actual runtime proof did not verify: {outcome:?}").into());
    };
    assert_eq!(snapshot_id, changed_snapshot);
    assert_ne!(evidence_id, diff_id);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
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
        return Err("actual TestEvidence missing".into());
    };
    assert_eq!(test.cases().len(), 2);
    let memory =
        RunMemoryCheckpoint::compile(&goal, &ledger, &run, state.published_index(), Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &run,
        goal.clone(),
        ledger.clone(),
        memory,
    )?;
    let AcceptanceVerifierOutcome::Accepted(receipt) = verifier
        .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
        .await?
    else {
        return Err("actual later runtime proof did not close global acceptance".into());
    };
    assert_eq!(receipt.criteria().len(), 1);
    assert_eq!(receipt.criteria()[0].criterion_id(), criterion);
    assert_eq!(receipt.criteria()[0].evidence_ids(), [evidence_id]);
    assert_eq!(
        ledger
            .step(step_id)
            .ok_or("progress history missing")?
            .attempts(),
        addresses_history
    );
    Ok((goal, evidence_id, snapshot_id))
}

fn addresses_step(goal: &GoalContract, _: DiscoveredCommandId) -> TestResult<TaskStepDefinition> {
    let criterion = goal.draft().acceptance_criteria()[0].id();
    let spec = VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([160; 32]),
        VerificationRequirement::try_from_string(
            "Only the requested implementation file changes".to_owned(),
        )?,
        DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(b"average.py".to_vec())?],
        )?,
    );
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Apply the scoped source change; runtime behavior remains to verify".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "This implementation progress only addresses the runtime requirement".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Current exact-path diff evidence".to_owned(),
        )?],
        spec,
    )?
    .with_criterion_coverage(vec![criterion], Vec::new())?)
}

fn check_command(goal: &GoalContract) -> TestResult<DiscoveredCommandId> {
    match goal.draft().acceptance_criteria()[0]
        .origin()
        .ok_or("criterion origin missing")?
        .evidence_requirement()
    {
        CriterionEvidenceRequirement::ExistingTest { command, .. } => Ok(*command),
        _ => Err("fixture requires actual existing-test proof".into()),
    }
}

fn proof_step(goal: &GoalContract, command: DiscoveredCommandId) -> TestResult<TaskStepDefinition> {
    let spec = VerificationSpec::test(
        VerificationSpecId::from_bytes([161; 32]),
        VerificationRequirement::try_from_string(
            "Both current existing tests execute and pass".to_owned(),
        )?,
        command,
        TestCaseSelector::All,
        MinimumTestCaseCount::new(2)?,
        VerificationScope::Workspace,
    );
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([PROOF_STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Prove the original runtime criterion using existing tests".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "A previous source diff is not a runtime proof".to_owned(),
        )?,
        vec![StepDependency::new(TaskStepId::from_bytes([STEP; 32]))],
        vec![ExpectedTaskEvidence::try_from_string(
            "Matching actual current TestEvidence".to_owned(),
        )?],
        spec,
    )?
    .with_criterion_coverage(Vec::new(), vec![goal.draft().acceptance_criteria()[0].id()])?)
}
