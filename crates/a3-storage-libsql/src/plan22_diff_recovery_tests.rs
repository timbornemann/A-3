//! A real approved Add produces typed DiffEvidence, never a read-tool evidence row.

use super::*;
use a3_application::{
    AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryOutcomeKind, AgentRecoveryStoreFailure,
    CompletedDiffRecoveryEvidence, InspectAgentRunRecovery, RecoverAgentRun,
};
use a3_domain::{
    DiffEvidence, DiffEvidenceSource, PatchAction, PatchActionSchemaVersion, PatchAdd,
    PatchFileContent, PatchOperation, PatchRationale, PlannedFileAddition,
};

const SOURCE: &[u8] = b"def answer():\n    return 42\n";
const USER_EDIT: &[u8] = b"def answer():\n    return 43\n";

struct Checkpoint {
    goal: GoalContract,
    run: AgentRun,
    stored: StoredTaskLedger,
    evidence: TaskEvidenceId,
    base: PublishedIndex,
    current: PublishedIndex,
}

#[test]
fn cold_recovery_resolves_current_approved_add_addresses_diff_without_tool_row() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = empty_fixture()?;
            let checkpoint = apply_add(&fixture).await?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let project = &fixture.project;
            let inspection = InspectAgentRunRecovery::new(&store, &store, &store, &store)
                .execute(project, checkpoint.run.id(), at(30)?, &ActiveControl)
                .await?;
            assert!(inspection.stale_evidence_ids().is_empty());
            assert!(inspection.can_resume());
            let resumed = RecoverAgentRun::new(&store, &store, &store, &store)
                .execute(
                    project,
                    checkpoint.run.id(),
                    AgentRecoveryChoice::Resume,
                    RunEventId::from_bytes([140; 32]),
                    at(31)?,
                    &ActiveControl,
                )
                .await?;
            assert_eq!(resumed.kind(), AgentRecoveryOutcomeKind::Resumed);
            assert!(resumed.reopened_step_ids().is_empty());
            assert_eq!(resumed.ledger().ledger(), checkpoint.stored.ledger());
            assert_eq!(fs::read(fixture.root.join("worktree/answer.py"))?, SOURCE);
            drop(store);
            fixture.cleanup()
        })
    })
}

#[test]
fn changed_add_dependency_stays_stale_and_only_explicit_recovery_reopens_it() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for choice in [AgentRecoveryChoice::Replan, AgentRecoveryChoice::Cancel] {
                let fixture = empty_fixture()?;
                let checkpoint = apply_add(&fixture).await?;
                fs::write(fixture.root.join("worktree/answer.py"), USER_EDIT)?;
                let (publisher, _, _, newer) = index_fixture(&fixture).await?;
                assert_ne!(
                    newer.run().snapshot_id(),
                    checkpoint.run.current_snapshot_id()
                );
                drop(publisher);
                let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                let project = &fixture.project;
                let inspection = InspectAgentRunRecovery::new(&store, &store, &store, &store)
                    .execute(project, checkpoint.run.id(), at(30)?, &ActiveControl)
                    .await?;
                assert_eq!(inspection.stale_evidence_ids(), &[checkpoint.evidence]);
                assert!(!inspection.can_resume());
                let recovery = RecoverAgentRun::new(&store, &store, &store, &store);
                assert!(matches!(
                    recovery
                        .execute(
                            project,
                            checkpoint.run.id(),
                            AgentRecoveryChoice::Resume,
                            RunEventId::from_bytes([141; 32]),
                            at(31)?,
                            &ActiveControl,
                        )
                        .await,
                    Err(AgentRecoveryError::ResumeRequiresReplan)
                ));
                assert_eq!(
                    store
                        .load_task_ledger(project, checkpoint.goal.task_id())
                        .await?,
                    Some(checkpoint.stored.clone())
                );
                let result = recovery
                    .execute(
                        project,
                        checkpoint.run.id(),
                        choice,
                        RunEventId::from_bytes([142; 32]),
                        at(32)?,
                        &ActiveControl,
                    )
                    .await?;
                assert_eq!(
                    result.reopened_step_ids(),
                    &[TaskStepId::from_bytes([STEP; 32])]
                );
                assert_eq!(
                    result.run().state(),
                    match choice {
                        // Recovery requests a replan; its controller transition remains separate.
                        AgentRecoveryChoice::Replan => checkpoint.run.state(),
                        AgentRecoveryChoice::Cancel => AgentControllerState::Cancelled,
                        AgentRecoveryChoice::Resume =>
                            return Err("unexpected fixture choice".into()),
                    }
                );
                assert_eq!(
                    result.kind(),
                    match choice {
                        AgentRecoveryChoice::Replan => AgentRecoveryOutcomeKind::ReplanRequired,
                        AgentRecoveryChoice::Cancel => AgentRecoveryOutcomeKind::Cancelled,
                        AgentRecoveryChoice::Resume =>
                            return Err("unexpected fixture choice".into()),
                    }
                );
                let step = result
                    .ledger()
                    .ledger()
                    .step(TaskStepId::from_bytes([STEP; 32]))
                    .ok_or("reopened Add missing")?;
                assert_ne!(step.status(), TaskStepStatus::Completed);
                assert!(step.definition().planned_file_addition().is_some());
                assert_eq!(
                    step.attempts(),
                    checkpoint
                        .stored
                        .ledger()
                        .step(step.definition().id())
                        .ok_or("original Add missing")?
                        .attempts()
                );
                // Recovery never executes another Add or overwrites the user's new bytes.
                assert_eq!(
                    fs::read(fixture.root.join("worktree/answer.py"))?,
                    USER_EDIT
                );
                assert_eq!(
                    store
                        .load_agent_mutation_attempts(project, checkpoint.run.id())
                        .await?
                        .len(),
                    1
                );
                drop(store);
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn typed_diff_recovery_distinguishes_missing_foreign_task_and_frozen_baseline() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = empty_fixture()?;
            let checkpoint = apply_add(&fixture).await?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let project = &fixture.project;
            for (task, id) in [
                (
                    checkpoint.goal.task_id(),
                    TaskEvidenceId::from_bytes([199; 32]),
                ),
                (TaskId::from_bytes([199; 32]), checkpoint.evidence),
            ] {
                assert_eq!(
                    store
                        .load_completed_diff_evidence(
                            project,
                            task,
                            &[id],
                            Duration::from_secs(5),
                            &ActiveControl,
                        )
                        .await?,
                    CompletedDiffRecoveryEvidence::Typed(Vec::new())
                );
            }
            // An externally damaged reference cannot fall back to a read-tool locator.
            let missing = TaskEvidenceId::from_bytes([199; 32]);
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute(
                    "UPDATE task_step_verification_evidence SET evidence_id=?1 WHERE evidence_id=?2",
                    params![missing.as_bytes().to_vec(), checkpoint.evidence.as_bytes().to_vec()],
                )
                .await?;
            drop(connection);
            drop(database);
            drop(store);
            let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            assert_eq!(
                cold.load_completed_diff_evidence(
                    project,
                    checkpoint.goal.task_id(),
                    &[missing],
                    Duration::from_secs(5),
                    &ActiveControl
                )
                .await?,
                CompletedDiffRecoveryEvidence::Typed(Vec::new())
            );
            let inspection = InspectAgentRunRecovery::new(&cold, &cold, &cold, &cold)
                .execute(project, checkpoint.run.id(), at(30)?, &ActiveControl)
                .await?;
            assert_eq!(inspection.stale_evidence_ids(), &[missing]);
            assert!(!inspection.can_resume());
            drop(cold);
            fixture.cleanup()?;

            let baseline = empty_fixture()?;
            let legacy = LibsqlKnowledgeStore::open(&baseline.layout).await?;
            let layout = baseline
                .layout
                .prepare_project(baseline.project.worktree())?;
            let database = KnowledgeDatabase::open(&layout, &baseline.project).await?;
            assert_eq!(database.schema_version().get(), 42);
            assert_eq!(
                legacy
                    .load_completed_diff_evidence(
                        &baseline.project,
                        TaskId::from_bytes([1; 32]),
                        &[checkpoint.evidence],
                        Duration::from_secs(5),
                        &ActiveControl,
                    )
                    .await?,
                CompletedDiffRecoveryEvidence::LegacyToolLocators
            );
            assert_eq!(database.schema_version().get(), 42);
            drop(database);
            drop(legacy);
            baseline.cleanup()
        })
    })
}

#[test]
fn corrupt_typed_diff_and_cancellation_never_fall_back_to_tool_evidence() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = empty_fixture()?;
            let checkpoint = apply_add(&fixture).await?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let project = &fixture.project;
            assert_eq!(
                store
                    .load_completed_diff_evidence(
                        project,
                        checkpoint.goal.task_id(),
                        &[checkpoint.evidence],
                        Duration::from_secs(5),
                        &Cancelled,
                    )
                    .await,
                Err(AgentRecoveryStoreFailure::Unavailable)
            );
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute(
                    "UPDATE verification_diff_evidence SET action_digest=zeroblob(32) WHERE evidence_id=?1",
                    params![checkpoint.evidence.as_bytes().to_vec()],
                )
                .await?;
            drop(connection);
            drop(database);
            drop(store);
            let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            assert!(matches!(
                InspectAgentRunRecovery::new(&cold, &cold, &cold, &cold)
                    .execute(project, checkpoint.run.id(), at(30)?, &ActiveControl)
                    .await,
                Err(AgentRecoveryError::Store(
                    AgentRecoveryStoreFailure::InvalidStoredData
                ))
            ));
            drop(cold);
            fixture.cleanup()
        })
    })
}

#[test]
fn valid_typed_diff_from_another_run_cannot_verify_the_completed_step() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = empty_fixture()?;
            let checkpoint = apply_add(&fixture).await?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let project = &fixture.project;
            let (foreign, event) = AgentRun::start(
                AgentRunId::from_bytes([199; 32]),
                checkpoint.goal.reference(),
                checkpoint.stored.ledger().revision(),
                profile()?.reference(),
                checkpoint.run.current_snapshot_id(),
                RunEventId::from_bytes([198; 32]),
                at(25)?,
            )?;
            CreateAgentRun::new(&store)
                .execute(project, &foreign, &event)
                .await?;
            let diff = DiffEvidence::from_published_indexes(
                VerificationRunId::from_bytes([199; 32]),
                VerificationSpecId::from_bytes([213; 32]),
                foreign.id(),
                &checkpoint.base,
                &checkpoint.current,
            )?;
            store
                .append_verification_evidence(
                    project,
                    &VerificationEvidence::Diff(diff.clone()),
                    Duration::from_secs(5),
                    &ActiveControl,
                )
                .await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection.execute("UPDATE task_step_verification_evidence SET evidence_id=?1 WHERE evidence_id=?2", params![diff.id().as_bytes().to_vec(), checkpoint.evidence.as_bytes().to_vec()]).await?;
            drop(connection);
            drop(database);
            drop(store);
            let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let inspection = InspectAgentRunRecovery::new(&cold, &cold, &cold, &cold)
                .execute(project, checkpoint.run.id(), at(30)?, &ActiveControl)
                .await?;
            assert_eq!(inspection.stale_evidence_ids(), &[diff.id()]);
            assert!(!inspection.can_resume());
            assert!(matches!(
                RecoverAgentRun::new(&cold, &cold, &cold, &cold)
                    .execute(
                        project,
                        checkpoint.run.id(),
                        AgentRecoveryChoice::Resume,
                        RunEventId::from_bytes([140; 32]),
                        at(31)?,
                        &ActiveControl,
                    )
                    .await,
                Err(AgentRecoveryError::ResumeRequiresReplan)
            ));
            assert_eq!(fs::read(fixture.root.join("worktree/answer.py"))?, SOURCE);
            drop(cold);
            fixture.cleanup()
        })
    })
}

#[test]
fn unrelated_snapshot_keeps_granular_patch_diff_current_after_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = empty_fixture()?;
            let checkpoint = apply_add(&fixture).await?;
            fs::write(
                fixture.root.join("worktree/foreign.txt"),
                b"User's independent note\n",
            )?;
            let (publisher, _, _, newer) = index_fixture(&fixture).await?;
            assert_ne!(
                newer.run().snapshot_id(),
                checkpoint.run.current_snapshot_id()
            );
            drop(publisher);
            let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let inspection = InspectAgentRunRecovery::new(&cold, &cold, &cold, &cold)
                .execute(
                    &fixture.project,
                    checkpoint.run.id(),
                    at(30)?,
                    &ActiveControl,
                )
                .await?;
            assert!(inspection.stale_evidence_ids().is_empty());
            assert!(inspection.can_resume());
            assert_eq!(fs::read(fixture.root.join("worktree/answer.py"))?, SOURCE);
            drop(cold);
            fixture.cleanup()
        })
    })
}

#[derive(Debug)]
struct Cancelled;
impl AgentControllerControl for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

fn empty_fixture() -> TestResult<Fixture> {
    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    Ok(fixture)
}

async fn apply_add(fixture: &Fixture) -> TestResult<Checkpoint> {
    let path = RepositoryPath::try_from_bytes(b"answer.py".to_vec())?;
    let invariant =
        DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?;
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"Create only answer.py.\n".to_vec(),
    )?;
    let criterion =
        criterion(2, "answer.py is the sole changed file")?.with_origin(CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            CriterionEvidenceRequirement::ChangeOnly(invariant.clone()),
        ));
    let criterion_id = criterion.id();
    let goal = GoalContract::initial(
        task,
        GoalContractDraft::new(
            GoalObjective::try_from_string("Create answer.py".to_owned())?,
            vec![criterion],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string(
                "The exact path diff; a later proof remains open".to_owned(),
            )?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    )
    .with_original(original)?;
    let step_id = TaskStepId::from_bytes([STEP; 32]);
    let spec_id = VerificationSpecId::from_bytes([213; 32]);
    let definition = TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string("Create answer.py".to_owned())?,
        TaskStepRationale::try_from_string("Apply the requested bounded initial Add".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "A current exact-path diff".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            spec_id,
            VerificationRequirement::try_from_string("Change only answer.py".to_owned())?,
            invariant,
        ),
    )?
    .with_criterion_coverage(vec![criterion_id], Vec::new())?
    .with_planned_file_addition(PlannedFileAddition::new(path.clone()))?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    assert!(published.publication().graph().files().is_empty());
    let model = profile()?;
    let (mut run, stored, unplanned) = start_fixture_step(
        fixture,
        &store,
        &goal,
        &model,
        published.run().snapshot_id(),
        vec![definition],
    )
    .await?;
    assert_eq!(unplanned, vec![criterion_id]);
    let (mut ledger, mut version) = stored.into_parts();
    let action = PatchAction::new(
        PatchActionSchemaVersion::V1,
        run.id(),
        fixture.project.worktree().id(),
        published.run().snapshot_id(),
        step_id,
        spec_id,
        PatchRationale::try_from_string("Create the only requested source file".to_owned())?,
        vec![PatchOperation::Add(PatchAdd::new(
            path,
            PatchFileContent::try_from_bytes(SOURCE.to_vec())?,
        ))],
    )?;
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
    let seed = MutationContextSeed::new(goal.clone(), model, Vec::new(), Vec::new());
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
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err("the actual Add did not produce a verified diff".into());
    };
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.current_snapshot_id(), snapshot_id);
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
    assert!(
        matches!(state.evidence(), [VerificationEvidence::Diff(diff)] if matches!(diff.source(), DiffEvidenceSource::Patch { .. }))
    );
    assert!(
        store
            .load_agent_tool_evidence(&fixture.project, run.id(), &[evidence_id])
            .await?
            .is_empty()
    );
    assert_eq!(fs::read(fixture.root.join("worktree/answer.py"))?, SOURCE);
    Ok(Checkpoint {
        goal,
        run,
        stored: StoredTaskLedger::new(ledger, version),
        evidence: evidence_id,
        base: published,
        current: state.published_index().clone(),
    })
}
