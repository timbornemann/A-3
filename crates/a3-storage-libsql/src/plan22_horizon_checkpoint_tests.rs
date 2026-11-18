//! Actual Ledger/Journal transactions around deliberately prepared state fixtures.
//! Passing step metadata and seeded usage below are not runtime/model-execution claims.
use super::*;
use a3_application::{
    ActivateAgentHorizonPlan, AdvanceAgentController, AgentActionStore, AgentActionStoreFailure,
    AgentControllerControl, AgentControllerSignal, AgentRecoveryChoice, AgentRecoveryStore,
    AppendAgentHorizonPlan, ContinueSourceAgentPlan, CreateAgentRun, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexStore, PendingPlanningPairDisposition,
    RunJournalStore, SourcePlanContinuationOutcome, TaskLedgerStoreFailure, TaskLedgerStoreVersion,
    load_pending_planning_pair,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, AgentTurnCharge,
    AgentTurnRepairUsage, GoalContractReference, IndexLanguage, IndexPublication, IndexRunId,
    IndexRunStart, IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion,
    LinkedGraph, ModelProfileId, ModelProfileReference, ModelProfileVersion, ModelTokenCount,
    ModulePolicyVersion, ModuleProjection, ModuleSymbolSet, PendingPlanning, PendingPlanningStage,
    Progress, RankProjection, RankingPolicyVersion, RepositoryCard, RunEvent, RunEventCode,
    RunEventId, RunEventKind, RunEventOutcome, RunEventPayload, RunEventSequence, Snapshot,
    SnapshotId, StepVerification, StepVerificationId, StepVerificationOutcome, TaskEvidenceId,
    TaskLedger, TaskLedgerReplanCause, TaskLedgerTimestamp, TaskReplanReason, TaskStepStatus,
    WorktreeGeneration,
};

#[derive(Debug)]
struct Live;
impl AgentControllerControl for Live {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl IndexPersistenceControl for Live {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

struct Prepared {
    store: LibsqlKnowledgeStore,
    goal: GoalContract,
    ledger: TaskLedger,
    version: TaskLedgerStoreVersion,
    run: AgentRun,
    snapshot: SnapshotId,
}

fn event_id(value: u8) -> RunEventId {
    RunEventId::from_bytes([value; 32])
}
fn at(value: u64) -> TestResult<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(value)?)
}
fn ledger_at(value: u64) -> TestResult<TaskLedgerTimestamp> {
    Ok(TaskLedgerTimestamp::from_unix_millis(value)?)
}

fn source_goal_for_planning() -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"Implement the fractional average result".to_vec(),
    )?;
    let criterion =
        criterion(2, "The average preserves fractions")?.with_origin(CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            test_requirement()?,
        ));
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Complete the original average request".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Matching current tests pass".to_owned())?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(10)?)
            .with_original(original)?,
    )
}

fn scaffold() -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([43; 32]),
        None,
        TaskStepOutcome::try_from_string("Prepare only the implementation scaffold".to_owned())?,
        TaskStepRationale::try_from_string(
            "Operational progress is not the final behavior proof".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "A bounded scaffold diff".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([42; 32]),
            VerificationRequirement::try_from_string("Only the intended path changes".to_owned())?,
            DiffInvariantVerification::new(
                DiffInvariantMode::OnlyPaths,
                vec![RepositoryPath::try_from_bytes(b"average.py".to_vec())?],
            )?,
        ),
    )?
    .with_criterion_coverage(vec![AcceptanceCriterionId::from_bytes([2; 32])], Vec::new())?)
}

fn next_step() -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([90; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Finish behavior and provide its actual targeted test result".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "The original runtime obligation remains open".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "The matching current test report".to_owned(),
        )?],
        test_spec()?.reidentified(VerificationSpecId::from_bytes([91; 32])),
    )?
    .with_criterion_coverage(Vec::new(), vec![AcceptanceCriterionId::from_bytes([2; 32])])?)
}

async fn add_snapshot(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    id: u8,
    parent: Option<SnapshotId>,
    generation: u64,
) -> TestResult<SnapshotId> {
    let id = SnapshotId::from_bytes([id; 32]);
    let snapshot = Snapshot::new(
        id,
        fixture.project.worktree().id(),
        parent,
        WorktreeGeneration::new(generation)?,
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
        IndexSchemaVersion::new(1)?,
        vec![LanguageAdapterRevision::new(
            IndexLanguage::Generic,
            LanguageAdapterVersion::try_from_string("planning-fixture-1".to_owned())?,
        )],
        Vec::new(),
    )?;
    store.append_snapshot(&fixture.project, &snapshot).await?;
    Ok(id)
}

async fn publish_empty(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    snapshot: SnapshotId,
) -> TestResult {
    let policy = ModulePolicyVersion::v1();
    let card = RepositoryCard::new(
        snapshot,
        policy,
        Vec::new(),
        Vec::new(),
        ModuleSymbolSet::empty(),
        0,
        0,
    )?;
    let publication = IndexPublication::new(
        LinkedGraph::new(snapshot, Vec::new(), Vec::new(), Vec::new(), Vec::new())?,
        RankProjection::new(snapshot, RankingPolicyVersion::v1(), Vec::new())?,
        Vec::new(),
        ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?,
    )?;
    let sequence = store.next_index_run_sequence(&fixture.project).await?;
    let run = store
        .start_index_run(
            &fixture.project,
            IndexRunStart::new(
                IndexRunId::from_bytes([199; 32]),
                snapshot,
                RankingPolicyVersion::v1(),
                sequence,
            ),
        )
        .await?;
    store
        .publish_index(&fixture.project, run.id(), &publication, &Live)
        .await?;
    Ok(())
}

async fn advance(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    run: &mut AgentRun,
    signal: AgentControllerSignal,
    id: u8,
    time: u64,
) -> TestResult {
    let sequence = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let changed =
        AdvanceAgentController.execute(run, signal, event_id(id), snapshot, at(time)?, false)?;
    store
        .append_run_event(&fixture.project, sequence, run, changed.event())
        .await?;
    Ok(())
}

async fn prepare(fixture: &Fixture, source: bool) -> TestResult<Prepared> {
    let store = if source {
        LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?
    } else {
        LibsqlKnowledgeStore::open(&fixture.layout).await?
    };
    let goal = if source {
        source_goal_for_planning()?
    } else {
        GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            plain_draft("Legacy capability rejection fixture")?,
            GoalContractTimestamp::from_unix_millis(10)?,
        )
    };
    let definition = if source {
        scaffold()?
    } else {
        step(test_spec()?)?
    };
    let mut ledger = TaskLedger::new(goal.reference(), vec![definition], ledger_at(10)?)?;
    store.create_goal_contract(&fixture.project, &goal).await?;
    let version = store.create_task_ledger(&fixture.project, &ledger).await?;
    let snapshot = add_snapshot(&store, fixture, 64, None, 1).await?;
    let (mut run, start) = AgentRun::start(
        AgentRunId::from_bytes([61; 32]),
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([62; 32]),
            ModelProfileVersion::V1,
        ),
        snapshot,
        event_id(70),
        at(11)?,
    )?;
    CreateAgentRun::new(&store)
        .execute(&fixture.project, &run, &start)
        .await?;
    for (signal, id, time) in [
        (AgentControllerSignal::AnchorsAccepted, 71, 12),
        (AgentControllerSignal::LocalizationComplete, 72, 13),
        (AgentControllerSignal::PlanReady, 73, 14),
    ] {
        advance(&store, fixture, &mut run, signal, id, time).await?;
    }
    ledger.start_step(TaskStepId::from_bytes([43; 32]), run.id(), ledger_at(20)?)?;
    ledger.begin_step_verification(
        TaskStepId::from_bytes([43; 32]),
        run.id(),
        None,
        vec![TaskEvidenceId::from_bytes([63; 32])],
        ledger_at(21)?,
    )?;
    // Strongly typed completed-state input only; no tool/test evidence is invented by this test.
    ledger.finish_step_verification(
        TaskStepId::from_bytes([43; 32]),
        StepVerification::new(
            StepVerificationId::from_bytes([65; 32]),
            VerificationSpecId::from_bytes([42; 32]),
            run.id(),
            StepVerificationOutcome::Passed,
            vec![TaskEvidenceId::from_bytes([63; 32])],
            ledger_at(22)?,
        )?,
    )?;
    let version = store
        .replace_task_ledger(&fixture.project, version, &ledger)
        .await?;
    advance(
        &store,
        fixture,
        &mut run,
        AgentControllerSignal::TurnNeedsVerification,
        74,
        23,
    )
    .await?;
    // Seeded accounting fixture, not a provider measurement. It must survive every CAS/reopen unchanged.
    let sequence = run.last_event_sequence();
    let consumed = run.record_turn(
        event_id(75),
        RunEventPayload::empty(),
        snapshot,
        at(24)?,
        AgentTurnCharge::new(
            ModelTokenCount::new(17),
            ModelTokenCount::new(5),
            None,
            AgentTurnRepairUsage::One,
        ),
    )?;
    store
        .append_run_event(&fixture.project, sequence, &run, &consumed)
        .await?;
    Ok(Prepared {
        store,
        goal,
        ledger,
        version,
        run,
        snapshot,
    })
}

fn requested(
    ledger: &TaskLedger,
    run: &AgentRun,
    id: u8,
    time: u64,
) -> TestResult<(TaskLedger, AgentRun, RunEvent)> {
    let mut candidate = run.clone();
    let snapshot = run.current_snapshot_id();
    let transition = AdvanceAgentController.execute(
        &mut candidate,
        AgentControllerSignal::VerificationNeedsPlanning,
        event_id(id),
        snapshot,
        at(time)?,
        false,
    )?;
    let event = transition.event().clone();
    let mut ledger = ledger.clone();
    ledger.request_horizon_planning(
        PendingPlanning::new(
            run.id(),
            run.goal_contract(),
            ledger.revision(),
            snapshot,
            event.id(),
            event.sequence(),
        )?,
        ledger_at(time)?,
    )?;
    Ok((ledger, candidate, event))
}

async fn request(fixture: &Fixture, p: &mut Prepared) -> TestResult<RunEvent> {
    match ContinueSourceAgentPlan::new(&p.store)
        .execute(
            &fixture.project,
            p.version,
            &p.goal,
            &mut p.run,
            &mut p.ledger,
            p.snapshot,
            event_id(80),
            at(25)?,
            &Live,
        )
        .await?
    {
        SourcePlanContinuationOutcome::PlanningRequested {
            ledger_version,
            request_event,
        } => {
            p.version = ledger_version;
            Ok(request_event)
        }
        _ => Err("expected durable planning request".into()),
    }
}

#[test]
fn planning_request_append_and_start_reopen_the_same_run_without_duplicate_attempts() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let mut p = prepare(&fixture, true).await?;
        let original_run = p.run.clone();
        let original_step = p.ledger.steps().next().ok_or("step missing")?.clone();
        let request_event = request(&fixture, &mut p).await?;
        assert_eq!(p.run.state(), AgentControllerState::Plan);
        assert_eq!(p.ledger.revision().get(), 1);
        let goal = p.goal.clone();
        let expected = p.ledger.clone();
        let version = p.version;
        drop(p);
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let stored = cold
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(stored.ledger(), &expected);
        assert_eq!(stored.version(), version);
        let (mut run, event, disposition) =
            load_pending_planning_pair(&fixture.project, stored.ledger(), &cold).await?;
        assert_eq!(event, request_event);
        assert_eq!(disposition, PendingPlanningPairDisposition::Live);
        assert_eq!(run.usage(), original_run.usage());
        assert_eq!(run.budget(), original_run.budget());
        assert_eq!(run.created_at(), original_run.created_at());
        let mut ledger = stored.ledger().clone();
        let snapshot = run.current_snapshot_id();
        let appended = AppendAgentHorizonPlan::new(&cold)
            .execute(
                &fixture.project,
                version,
                &goal,
                &mut run,
                &mut ledger,
                snapshot,
                &event,
                vec![next_step()?],
                event_id(81),
                at(26)?,
                &Live,
            )
            .await?;
        assert!(matches!(
            ledger.pending_planning().ok_or("marker missing")?.stage(),
            PendingPlanningStage::PlanAppended { .. }
        ));
        assert_eq!(
            ledger.replans().last().ok_or("revision missing")?.cause(),
            TaskLedgerReplanCause::HorizonExtension
        );
        assert_eq!(
            ledger.step(original_step.definition().id()),
            Some(&original_step)
        );
        drop(cold);
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let stored = cold
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        let (mut run, event, _) =
            load_pending_planning_pair(&fixture.project, stored.ledger(), &cold).await?;
        let mut ledger = stored.ledger().clone();
        let before = ledger.clone();
        assert!(
            AppendAgentHorizonPlan::new(&cold)
                .execute(
                    &fixture.project,
                    appended,
                    &goal,
                    &mut run,
                    &mut ledger,
                    snapshot,
                    &event,
                    vec![next_step()?],
                    event_id(82),
                    at(27)?,
                    &Live
                )
                .await
                .is_err()
        );
        assert_eq!(ledger, before);
        let started = ActivateAgentHorizonPlan::new(&cold)
            .execute(
                &fixture.project,
                appended,
                &goal,
                &mut run,
                &mut ledger,
                snapshot,
                &event,
                TaskStepId::from_bytes([90; 32]),
                event_id(83),
                at(28)?,
                &Live,
            )
            .await?;
        assert!(ledger.pending_planning().is_none());
        assert_eq!(run.state(), AgentControllerState::Execute);
        assert_eq!(run.id(), original_run.id());
        assert_eq!(run.usage(), original_run.usage());
        assert_eq!(run.created_at(), original_run.created_at());
        drop(cold);
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let stored = cold
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(stored.version(), started);
        assert!(stored.ledger().pending_planning().is_none());
        let active = stored
            .ledger()
            .step(TaskStepId::from_bytes([90; 32]))
            .ok_or("new step missing")?;
        assert_eq!(active.status(), TaskStepStatus::InProgress);
        assert_eq!(active.attempts().len(), 1);
        assert_eq!(active.attempts()[0].run_id(), original_run.id());
        assert_eq!(
            cold.load_agent_run(&fixture.project, original_run.id())
                .await?,
            Some(run)
        );
        drop(cold);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn either_stale_cas_and_each_partial_write_failure_leave_both_projections_unchanged() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let p = prepare(&fixture, true).await?;
        let (ledger, run, event) = requested(&p.ledger, &p.run, 80, 25)?;
        let stale_ledger = TaskLedgerStoreVersion::INITIAL;
        let stale_run = RunEventSequence::new(p.run.last_event_sequence().get() - 1)?;
        assert_eq!(
            p.store
                .commit_ledger_action(
                    &fixture.project,
                    stale_ledger,
                    p.run.last_event_sequence(),
                    &ledger,
                    &run,
                    &event
                )
                .await,
            Err(AgentActionStoreFailure::LedgerVersionConflict)
        );
        assert_eq!(
            p.store
                .commit_ledger_action(
                    &fixture.project,
                    p.version,
                    stale_run,
                    &ledger,
                    &run,
                    &event
                )
                .await,
            Err(AgentActionStoreFailure::RunSequenceConflict)
        );
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        for trigger in [
            "CREATE TRIGGER abort_checkpoint BEFORE INSERT ON task_ledger_pending_planning BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            "CREATE TRIGGER abort_checkpoint BEFORE INSERT ON run_events WHEN NEW.state_from='verify' AND NEW.state_to='plan' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            "CREATE TRIGGER abort_checkpoint BEFORE UPDATE ON agent_runs WHEN NEW.controller_state='plan' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            "CREATE TRIGGER abort_checkpoint AFTER INSERT ON task_ledger_pending_planning BEGIN UPDATE task_ledger_pending_planning SET requested_event_id=(SELECT event_id FROM run_events WHERE run_id=NEW.run_id AND event_sequence=1) WHERE task_id=NEW.task_id; END;",
        ] {
            connection.execute_batch(trigger).await?;
            assert!(
                p.store
                    .commit_ledger_action(
                        &fixture.project,
                        p.version,
                        p.run.last_event_sequence(),
                        &ledger,
                        &run,
                        &event
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                p.store
                    .load_task_ledger(&fixture.project, p.goal.task_id())
                    .await?
                    .ok_or("ledger missing")?
                    .ledger(),
                &p.ledger
            );
            assert_eq!(
                p.store.load_agent_run(&fixture.project, p.run.id()).await?,
                Some(p.run.clone())
            );
            assert_eq!(
                count(
                    &connection,
                    "SELECT count(*) FROM task_ledger_pending_planning"
                )
                .await?,
                0
            );
            connection
                .execute_batch("DROP TRIGGER abort_checkpoint")
                .await?;
        }
        drop(connection);
        drop(database);
        drop(p);
        fixture.cleanup()?;
        Ok(())
    })
}

fn without_pending(ledger: &TaskLedger) -> TestResult<TaskLedger> {
    Ok(TaskLedger::reconstruct(
        ledger.goal_contract(),
        ledger.revision(),
        ledger.steps().cloned().collect(),
        ledger.replans().to_vec(),
        ledger.created_at(),
        ledger.updated_at(),
    )?)
}

#[test]
fn ordinary_ledger_writes_cannot_establish_advance_or_remove_planning_authority() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let mut p = prepare(&fixture, true).await?;
        let (pending, _, _) = requested(&p.ledger, &p.run, 80, 25)?;
        assert_eq!(
            p.store.create_task_ledger(&fixture.project, &pending).await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &pending)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        // Reconstruct is a decoder capability, not permission to forge a normal horizon append.
        let mut forged = p.ledger.clone();
        let revision = forged.replan(
            Vec::new(),
            vec![next_step()?],
            TaskReplanReason::try_from_string("forged horizon label".to_owned())?,
            ledger_at(25)?,
        )?;
        let mut records = forged.replans().to_vec();
        let record = records
            .pop()
            .ok_or("revision missing")?
            .with_cause(TaskLedgerReplanCause::HorizonExtension)?;
        records.push(record);
        let forged = TaskLedger::reconstruct(
            forged.goal_contract(),
            revision,
            forged.steps().cloned().collect(),
            records,
            forged.created_at(),
            forged.updated_at(),
        )?;
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &forged)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        let mut forged_run = p.run.clone();
        let forged_event = forged_run.record_ledger_update(
            event_id(79),
            revision,
            RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            p.snapshot,
            at(25)?,
        )?;
        assert!(
            p.store
                .commit_ledger_action(
                    &fixture.project,
                    p.version,
                    p.run.last_event_sequence(),
                    &forged,
                    &forged_run,
                    &forged_event
                )
                .await
                .is_err()
        );
        assert_eq!(
            p.store.load_agent_run(&fixture.project, p.run.id()).await?,
            Some(p.run.clone())
        );
        assert_eq!(
            p.store
                .load_task_ledger(&fixture.project, p.goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .version(),
            p.version
        );
        let event = request(&fixture, &mut p).await?;
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &without_pending(&p.ledger)?)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        let mut appended = p.ledger.clone();
        appended.append_horizon(
            vec![next_step()?],
            TaskReplanReason::try_from_string("next bounded proof".to_owned())?,
            ledger_at(26)?,
        )?;
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &appended)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        p.version = AppendAgentHorizonPlan::new(&p.store)
            .execute(
                &fixture.project,
                p.version,
                &p.goal,
                &mut p.run,
                &mut p.ledger,
                p.snapshot,
                &event,
                vec![next_step()?],
                event_id(81),
                at(26)?,
                &Live,
            )
            .await?;
        let mut started = p.ledger.clone();
        started.start_appended_horizon(
            TaskStepId::from_bytes([90; 32]),
            p.run.id(),
            ledger_at(27)?,
        )?;
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &started)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        assert_eq!(
            p.store
                .load_task_ledger(&fixture.project, p.goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .ledger(),
            &p.ledger
        );
        assert_eq!(
            p.store.load_agent_run(&fixture.project, p.run.id()).await?,
            Some(p.run.clone())
        );
        drop(p);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn exact_request_survives_presentation_tail_and_cancel_keeps_only_inert_audit() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let mut p = prepare(&fixture, true).await?;
        let event = request(&fixture, &mut p).await?;
        let usage = p.run.usage();
        let created = p.run.created_at();
        for index in 0..70_u8 {
            let sequence = p.run.last_event_sequence();
            let diagnostic = p.run.record(
                event_id(100 + index),
                RunEventKind::Diagnostic,
                RunEventPayload::new(
                    RunEventCode::StateRecovered,
                    Some(RunEventOutcome::Succeeded),
                    None,
                ),
                p.snapshot,
                None,
                at(30 + u64::from(index))?,
            )?;
            p.store
                .append_run_event(&fixture.project, sequence, &p.run, &diagnostic)
                .await?;
        }
        let ledger = p.ledger.clone();
        let id = p.run.id();
        drop(p);
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let (mut run, loaded, disposition) =
            load_pending_planning_pair(&fixture.project, &ledger, &cold).await?;
        assert_eq!(loaded, event);
        assert_eq!(disposition, PendingPlanningPairDisposition::Live);
        assert_eq!(run.usage(), usage);
        assert_eq!(run.created_at(), created);
        advance(
            &cold,
            &fixture,
            &mut run,
            AgentControllerSignal::CancelRequested,
            190,
            200,
        )
        .await?;
        assert_eq!(run.id(), id);
        assert_eq!(run.state(), AgentControllerState::Cancelled);
        let (_, loaded, disposition) =
            load_pending_planning_pair(&fixture.project, &ledger, &cold).await?;
        assert_eq!(loaded, event);
        assert_eq!(disposition, PendingPlanningPairDisposition::TerminalInert);
        drop(cold);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn changed_snapshot_stays_auditable_without_rebinding_or_activating_the_job() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let mut p = prepare(&fixture, true).await?;
        let request_event = request(&fixture, &mut p).await?;
        let original = p.ledger.pending_planning().ok_or("marker missing")?.clone();
        let snapshot = add_snapshot(&p.store, &fixture, 66, Some(p.snapshot), 2).await?;
        let sequence = p.run.last_event_sequence();
        let diagnostic = p.run.record(
            event_id(82),
            RunEventKind::Diagnostic,
            RunEventPayload::new(
                RunEventCode::StateRecovered,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            snapshot,
            None,
            at(26)?,
        )?;
        p.store
            .append_run_event(&fixture.project, sequence, &p.run, &diagnostic)
            .await?;
        let (run, _, disposition) =
            load_pending_planning_pair(&fixture.project, &p.ledger, &p.store).await?;
        assert_eq!(disposition, PendingPlanningPairDisposition::StaleInert);
        assert_eq!(run, p.run);
        assert!(
            AppendAgentHorizonPlan::new(&p.store)
                .execute(
                    &fixture.project,
                    p.version,
                    &p.goal,
                    &mut p.run,
                    &mut p.ledger,
                    snapshot,
                    &request_event,
                    vec![next_step()?],
                    event_id(83),
                    at(27)?,
                    &Live
                )
                .await
                .is_err()
        );
        assert_eq!(p.ledger.pending_planning(), Some(&original));
        assert_eq!(
            p.store
                .load_task_ledger(&fixture.project, p.goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .version(),
            p.version
        );
        assert_eq!(
            p.store.load_agent_run(&fixture.project, p.run.id()).await?,
            Some(p.run.clone())
        );
        drop(p);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn actual_recovery_transaction_preserves_stale_checkpoint_as_terminal_audit() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let mut p = prepare(&fixture, true).await?;
        request(&fixture, &mut p).await?;
        let marker = p.ledger.pending_planning().ok_or("marker missing")?.clone();
        let snapshot = add_snapshot(&p.store, &fixture, 66, Some(p.snapshot), 2).await?;
        publish_empty(&p.store, &fixture, snapshot).await?;
        let mut ledger = p.ledger.clone();
        ledger.invalidate_verification_evidence(
            vec![TaskEvidenceId::from_bytes([63; 32])],
            ledger_at(26)?,
        )?;
        let mut run = p.run.clone();
        let event = run.transition(
            event_id(84),
            AgentControllerState::Cancelled,
            RunEventPayload::new(
                RunEventCode::UserRequest,
                Some(RunEventOutcome::Cancelled),
                None,
            ),
            snapshot,
            at(26)?,
        )?;
        let version = p
            .store
            .commit_agent_recovery(
                &fixture.project,
                AgentRecoveryChoice::Cancel,
                snapshot,
                p.version,
                p.run.last_event_sequence(),
                &ledger,
                &run,
                &event,
            )
            .await?;
        assert!(version.get() > p.version.get());
        assert_eq!(
            p.store
                .load_task_ledger(&fixture.project, p.goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .ledger(),
            &ledger
        );
        let (loaded, _, disposition) =
            load_pending_planning_pair(&fixture.project, &ledger, &p.store).await?;
        assert_eq!(loaded, run);
        assert_eq!(disposition, PendingPlanningPairDisposition::TerminalInert);
        assert_eq!(ledger.pending_planning(), Some(&marker));
        assert_eq!(run.usage(), p.run.usage());
        assert_eq!(run.created_at(), p.run.created_at());
        drop(p);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn planning_marker_rejects_new_run_identity_and_bounded_decoder_rejects_corruption() -> TestResult {
    crate::run_native_libsql_test(async {
        for corruption in [
            "large_id",
            "large_stage",
            "wrong_request",
            "too_many_steps",
            "orphan_child",
        ] {
            let fixture = Fixture::new()?;
            let mut p = prepare(&fixture, true).await?;
            request(&fixture, &mut p).await?;
            let (new_run, start) = AgentRun::start(
                AgentRunId::from_bytes([201; 32]),
                p.goal.reference(),
                p.ledger.revision(),
                ModelProfileReference::new(
                    ModelProfileId::from_bytes([202; 32]),
                    ModelProfileVersion::V1,
                ),
                p.snapshot,
                event_id(203),
                at(30)?,
            )?;
            assert!(
                p.store
                    .create_agent_run(&fixture.project, &new_run, &start)
                    .await
                    .is_err()
            );
            assert!(
                p.store
                    .load_agent_run(&fixture.project, new_run.id())
                    .await?
                    .is_none()
            );
            assert_eq!(
                p.store.load_agent_run(&fixture.project, p.run.id()).await?,
                Some(p.run.clone())
            );
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            // Isolated corrupt fixture only; normal writes retain all SQL constraints.
            connection
                .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
                .await?;
            let sql = match corruption {
                "large_id" => {
                    "UPDATE task_ledger_pending_planning SET requested_event_id=zeroblob(8388608)"
                }
                "large_stage" => {
                    "UPDATE task_ledger_pending_planning SET stage=CAST(zeroblob(8388608) AS TEXT)"
                }
                "wrong_request" => {
                    "UPDATE task_ledger_pending_planning SET requested_event_id=(SELECT event_id FROM run_events WHERE event_sequence=1)"
                }
                "orphan_child" => {
                    "INSERT INTO task_ledger_pending_planning_steps(task_id,step_sequence,step_id)
                    SELECT task_id,1,step_id FROM task_steps LIMIT 1;
                    DELETE FROM task_ledger_pending_planning;"
                }
                _ => "UPDATE task_ledger_pending_planning SET added_step_count=4",
            };
            connection.execute_batch(sql).await?;
            if corruption == "wrong_request" {
                assert!(
                    crate::run_journal_repository::load_run(
                        &connection,
                        fixture.project.worktree().id(),
                        p.run.id()
                    )
                    .await
                    .is_err()
                );
            } else {
                assert!(
                    crate::task_ledger_repository::load(
                        &connection,
                        fixture.project.worktree().id(),
                        p.goal.task_id()
                    )
                    .await
                    .is_err()
                );
            }
            drop(connection);
            drop(database);
            drop(p);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn ordinary_schema_cannot_silently_drop_pending_before_any_ledger_write() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let p = prepare(&fixture, false).await?;
        let (ledger, run, event) = requested(&p.ledger, &p.run, 80, 25)?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let database = KnowledgeDatabase::open(&layout, &fixture.project).await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "CREATE TRIGGER forbid_planning_write BEFORE UPDATE ON task_ledgers
            BEGIN SELECT RAISE(ABORT,'unexpected partial write'); END;",
            )
            .await?;
        assert_eq!(
            p.store
                .replace_task_ledger(&fixture.project, p.version, &ledger)
                .await,
            Err(TaskLedgerStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            p.store
                .commit_ledger_action(
                    &fixture.project,
                    p.version,
                    p.run.last_event_sequence(),
                    &ledger,
                    &run,
                    &event
                )
                .await,
            Err(AgentActionStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            p.store
                .load_task_ledger(&fixture.project, p.goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .ledger(),
            &p.ledger
        );
        assert_eq!(
            p.store.load_agent_run(&fixture.project, p.run.id()).await?,
            Some(p.run.clone())
        );
        drop(connection);
        drop(database);
        drop(p);
        fixture.cleanup()?;
        Ok(())
    })
}
