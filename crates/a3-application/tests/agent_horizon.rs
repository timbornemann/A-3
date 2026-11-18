//! Controlled state/CAS fixtures. No provider call or product functional proof is simulated.
use a3_application::*;
use a3_domain::*;
use futures::executor::block_on;
use std::{error::Error, sync::Mutex};
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
#[derive(Debug)]
struct Active;
impl AgentControllerControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }
}
#[derive(Debug)]
struct Cancelled;
impl AgentControllerControl for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}
#[derive(Debug, Default)]
struct Store {
    commits: Mutex<Vec<(TaskLedger, AgentRun, RunEvent)>>,
    failure: Option<AgentActionStoreFailure>,
}
impl AgentActionStore for Store {
    fn commit_ledger_action<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        v: TaskLedgerStoreVersion,
        _: RunEventSequence,
        l: &'a TaskLedger,
        r: &'a AgentRun,
        e: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async move {
            if let Some(e) = self.failure {
                return Err(e);
            };
            self.commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((l.clone(), r.clone(), e.clone()));
            TaskLedgerStoreVersion::new(v.get() + 1)
                .map_err(|_| AgentActionStoreFailure::Unavailable)
        })
    }
}
fn ts(n: u64) -> Result<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(n)?)
}
fn lt(n: u64) -> Result<TaskLedgerTimestamp> {
    Ok(TaskLedgerTimestamp::from_unix_millis(n)?)
}
fn eid(n: u8) -> RunEventId {
    RunEventId::from_bytes([n; 32])
}
fn sid(n: u8) -> TaskStepId {
    TaskStepId::from_bytes([n; 32])
}
fn snap() -> SnapshotId {
    SnapshotId::from_bytes([90; 32])
}
fn rid() -> AgentRunId {
    AgentRunId::from_bytes([91; 32])
}
fn cid() -> AcceptanceCriterionId {
    AcceptanceCriterionId::from_bytes([92; 32])
}
fn version() -> Result<TaskLedgerStoreVersion> {
    Ok(TaskLedgerStoreVersion::new(1)?)
}
fn requirement() -> Result<VerificationRequirement> {
    Ok(VerificationRequirement::try_from_string(
        "declared local check".into(),
    )?)
}
fn diff(n: u8, path: &str) -> Result<VerificationSpec> {
    Ok(VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([n; 32]),
        requirement()?,
        DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?],
        )?,
    ))
}
fn proof_spec(n: u8) -> Result<VerificationSpec> {
    Ok(VerificationSpec::test(
        VerificationSpecId::from_bytes([n; 32]),
        requirement()?,
        DiscoveredCommandId::from_bytes([93; 32]),
        TestCaseSelector::All,
        MinimumTestCaseCount::new(1)?,
        VerificationScope::Targeted,
    ))
}
fn step(n: u8, spec: VerificationSpec, proves: bool) -> Result<TaskStepDefinition> {
    let d = TaskStepDefinition::new(
        sid(n),
        None,
        TaskStepOutcome::try_from_string(format!("outcome {n}"))?,
        TaskStepRationale::try_from_string(format!("rationale {n}"))?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(format!(
            "evidence {n}"
        ))?],
        spec,
    )?;
    Ok(if proves {
        d.with_criterion_coverage(Vec::new(), vec![cid()])?
    } else {
        d.with_criterion_coverage(vec![cid()], Vec::new())?
    })
}
fn goal() -> Result<GoalContract> {
    let task = TaskId::from_bytes([94; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"Implement the verified behavior.".to_vec(),
    )?;
    let c = AcceptanceCriterion::new(
        cid(),
        AcceptanceCriterionStatement::try_from_string("verified behavior".into())?,
    )
    .with_origin(CriterionOrigin::new(
        original.range(0, original.as_bytes().len())?,
        CriterionEvidenceRequirement::ExistingTest {
            command: DiscoveredCommandId::from_bytes([93; 32]),
            selector: TestCaseSelector::All,
            minimum_cases: MinimumTestCaseCount::new(1)?,
            scope: VerificationScope::Targeted,
            python_integer_translation: None,
        },
    ));
    Ok(GoalContract::initial(
        task,
        GoalContractDraft::new(
            GoalObjective::try_from_string("verified behavior".into())?,
            vec![c],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("run relevant test".into())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    )
    .with_original(original)?)
}
fn fixture() -> Result<(GoalContract, TaskLedger, AgentRun)> {
    fixture_with_proof(false)
}
fn fixture_with_proof(proves: bool) -> Result<(GoalContract, TaskLedger, AgentRun)> {
    let goal = goal()?;
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![step(
            1,
            if proves {
                proof_spec(1)?
            } else {
                diff(1, "first.py")?
            },
            proves,
        )?],
        lt(1)?,
    )?;
    ledger.start_step(sid(1), rid(), lt(2)?)?;
    let evidence = TaskEvidenceId::from_bytes([95; 32]);
    ledger.begin_step_verification(sid(1), rid(), None, vec![evidence], lt(3)?)?;
    ledger.finish_step_verification(
        sid(1),
        StepVerification::new(
            StepVerificationId::from_bytes([96; 32]),
            VerificationSpecId::from_bytes([1; 32]),
            rid(),
            StepVerificationOutcome::Passed,
            vec![evidence],
            lt(4)?,
        )?,
    )?;
    let (mut run, _) = AgentRun::start(
        rid(),
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([97; 32]),
            ModelProfileVersion::V1,
        ),
        snap(),
        eid(1),
        ts(1)?,
    )?;
    run.record_turn(
        eid(2),
        RunEventPayload::empty(),
        snap(),
        ts(2)?,
        AgentTurnCharge::new(
            ModelTokenCount::new(400),
            ModelTokenCount::new(40),
            Some(AgentTurnActionClass::Inspect),
            AgentTurnRepairUsage::One,
        ),
    )?;
    for (id, state) in [
        (3, AgentControllerState::Localize),
        (4, AgentControllerState::Plan),
        (5, AgentControllerState::Execute),
        (6, AgentControllerState::Verify),
    ] {
        run.transition(
            eid(id),
            state,
            RunEventPayload::empty(),
            snap(),
            ts(u64::from(id))?,
        )?;
    }
    Ok((goal, ledger, run))
}
fn project() -> Result<ProjectIdentity> {
    let root = std::env::current_dir()?.canonicalize()?;
    let id = RepositoryId::from_bytes([98; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([99; 32]),
            WorktreeAnchorId::from_bytes([100; 32]),
            id,
            CanonicalDirectory::from_canonicalized(root)?,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}
fn request(
    store: &Store,
    goal: &GoalContract,
    ledger: &mut TaskLedger,
    run: &mut AgentRun,
) -> Result<RunEvent> {
    match block_on(ContinueSourceAgentPlan::new(store).execute(
        &project()?,
        version()?,
        goal,
        run,
        ledger,
        snap(),
        eid(7),
        ts(10)?,
        &Active,
    ))? {
        SourcePlanContinuationOutcome::PlanningRequested { request_event, .. } => Ok(request_event),
        other => Err(format!("unexpected {other:?}").into()),
    }
}
#[test]
fn request_append_activate_preserves_usage_history_and_separate_cause() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let initial = (r.usage(), r.budget(), r.created_at(), r.model_profile());
    let event = request(&store, &g, &mut l, &mut r)?;
    assert_eq!(r.state(), AgentControllerState::Plan);
    assert_eq!(l.revision(), TaskLedgerRevision::INITIAL);
    assert_eq!(
        l.step(sid(1)).ok_or("old step")?.status(),
        TaskStepStatus::Completed
    );
    assert_eq!(
        validate_pending_planning_pair(&l, &r, &event)?,
        PendingPlanningPairDisposition::Live
    );
    let v = block_on(AppendAgentHorizonPlan::new(&store).execute(
        &project()?,
        TaskLedgerStoreVersion::new(2)?,
        &g,
        &mut r,
        &mut l,
        snap(),
        &event,
        vec![step(2, diff(2, "second.py")?, false)?],
        eid(8),
        ts(11)?,
        &Active,
    ))?;
    assert!(matches!(
        l.pending_planning().ok_or("pending")?.stage(),
        PendingPlanningStage::PlanAppended { .. }
    ));
    assert_eq!(
        l.replans()[0].cause(),
        TaskLedgerReplanCause::HorizonExtension
    );
    assert!(l.replans()[0].retired_step_ids().is_empty());
    assert_eq!(l.steps().count(), 2);
    block_on(ActivateAgentHorizonPlan::new(&store).execute(
        &project()?,
        v,
        &g,
        &mut r,
        &mut l,
        snap(),
        &event,
        sid(2),
        eid(9),
        ts(12)?,
        &Active,
    ))?;
    assert!(l.pending_planning().is_none());
    assert_eq!(r.state(), AgentControllerState::Execute);
    assert_eq!(
        l.step(sid(2)).ok_or("new step")?.status(),
        TaskStepStatus::InProgress
    );
    assert_eq!(
        (r.usage(), r.budget(), r.created_at(), r.model_profile()),
        initial
    );
    assert_eq!(r.usage().repair_count(), 1);
    assert_eq!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len(),
        3
    );
    Ok(())
}
#[test]
fn repeated_obligation_with_only_new_ids_and_words_is_not_progress() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    let old = (l.clone(), r.clone());
    let result = block_on(AppendAgentHorizonPlan::new(&store).execute(
        &project()?,
        TaskLedgerStoreVersion::new(2)?,
        &g,
        &mut r,
        &mut l,
        snap(),
        &e,
        vec![step(2, diff(2, "first.py")?, false)?],
        eid(8),
        ts(11)?,
        &Active,
    ));
    assert!(matches!(
        result,
        Err(AgentHorizonFailure::NoMaterialProgress)
    ));
    assert_eq!((l, r), old);
    assert_eq!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len(),
        1
    );
    Ok(())
}
#[test]
fn transition_from_addresses_to_matching_proves_is_material() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    block_on(AppendAgentHorizonPlan::new(&store).execute(
        &project()?,
        TaskLedgerStoreVersion::new(2)?,
        &g,
        &mut r,
        &mut l,
        snap(),
        &e,
        vec![step(2, proof_spec(2)?, true)?],
        eid(8),
        ts(11)?,
        &Active,
    ))?;
    assert!(
        l.step(sid(2))
            .ok_or("proof")?
            .definition()
            .addressed_criteria()
            .is_empty()
    );
    Ok(())
}
#[test]
fn duplicate_normalized_obligations_in_one_preview_are_rejected() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    assert!(matches!(
        block_on(AppendAgentHorizonPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            &e,
            vec![
                step(2, diff(2, "second.py")?, false)?,
                step(3, diff(3, "second.py")?, false)?
            ],
            eid(8),
            ts(11)?,
            &Active
        )),
        Err(AgentHorizonFailure::NoMaterialProgress)
    ));
    Ok(())
}
#[test]
fn all_three_atomic_failures_leave_both_in_memory_aggregates_unchanged() -> Result {
    let (g, base_l, base_r) = fixture()?;
    let failing = Store {
        failure: Some(AgentActionStoreFailure::RunSequenceConflict),
        ..Store::default()
    };
    let (mut l, mut r) = (base_l.clone(), base_r.clone());
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&failing).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(10)?,
            &Active
        )),
        Err(AgentHorizonFailure::Storage(_))
    ));
    assert_eq!((l.clone(), r.clone()), (base_l, base_r));
    let good = Store::default();
    let e = request(&good, &g, &mut l, &mut r)?;
    let old = (l.clone(), r.clone());
    assert!(matches!(
        block_on(AppendAgentHorizonPlan::new(&failing).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            &e,
            vec![step(2, diff(2, "second.py")?, false)?],
            eid(8),
            ts(11)?,
            &Active
        )),
        Err(AgentHorizonFailure::Storage(_))
    ));
    assert_eq!((l.clone(), r.clone()), old);
    block_on(AppendAgentHorizonPlan::new(&good).execute(
        &project()?,
        version()?,
        &g,
        &mut r,
        &mut l,
        snap(),
        &e,
        vec![step(2, diff(2, "second.py")?, false)?],
        eid(8),
        ts(11)?,
        &Active,
    ))?;
    let old = (l.clone(), r.clone());
    assert!(matches!(
        block_on(ActivateAgentHorizonPlan::new(&failing).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            &e,
            sid(2),
            eid(9),
            ts(12)?,
            &Active
        )),
        Err(AgentHorizonFailure::Storage(_))
    ));
    assert_eq!((l, r), old);
    Ok(())
}
#[test]
fn stale_source_and_cancelled_owner_are_inert_but_auditable() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    r.record(
        eid(8),
        RunEventKind::Diagnostic,
        RunEventPayload::empty(),
        SnapshotId::from_bytes([101; 32]),
        None,
        ts(11)?,
    )?;
    assert_eq!(
        validate_pending_planning_pair(&l, &r, &e)?,
        PendingPlanningPairDisposition::StaleInert
    );
    assert!(
        block_on(AppendAgentHorizonPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            SnapshotId::from_bytes([101; 32]),
            &e,
            vec![step(2, diff(2, "second.py")?, false)?],
            eid(9),
            ts(12)?,
            &Active
        ))
        .is_err()
    );
    let old = r.clone();
    let old_l = l.clone();
    let cancel = r.transition(
        eid(9),
        AgentControllerState::Cancelled,
        RunEventPayload::empty(),
        r.current_snapshot_id(),
        ts(12)?,
    )?;
    validate_pending_planning_transition(&old_l, &old, &l, &r, &cancel)?;
    assert_eq!(
        validate_pending_planning_pair(&l, &r, &e)?,
        PendingPlanningPairDisposition::TerminalInert
    );
    assert!(
        block_on(ActivateAgentHorizonPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            &e,
            sid(2),
            eid(10),
            ts(13)?,
            &Active
        ))
        .is_err()
    );
    Ok(())
}
#[test]
fn stale_evidence_can_be_marked_without_corrupting_audit_marker() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    let old_l = l.clone();
    let old_r = r.clone();
    l.invalidate_verification_evidence(vec![TaskEvidenceId::from_bytes([95; 32])], lt(11)?)?;
    let d = r.record(
        eid(8),
        RunEventKind::Diagnostic,
        RunEventPayload::empty(),
        snap(),
        None,
        ts(11)?,
    )?;
    validate_pending_planning_transition(&old_l, &old_r, &l, &r, &d)?;
    assert_eq!(
        validate_pending_planning_pair(&l, &r, &e)?,
        PendingPlanningPairDisposition::StaleInert
    );
    Ok(())
}
#[test]
fn ordinary_step_start_and_replan_cannot_bypass_pending() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let e = request(&Store::default(), &g, &mut l, &mut r)?;
    assert!(matches!(
        l.start_step(sid(1), rid(), lt(11)?),
        Err(TaskLedgerError::PlanningAlreadyPending)
    ));
    assert!(matches!(
        l.replan(
            Vec::new(),
            vec![step(2, diff(2, "second.py")?, false)?],
            TaskReplanReason::try_from_string("different words".into())?,
            lt(11)?
        ),
        Err(TaskLedgerError::PlanningAlreadyPending)
    ));
    assert_eq!(
        validate_pending_planning_pair(&l, &r, &e)?,
        PendingPlanningPairDisposition::Live
    );
    Ok(())
}
#[test]
fn no_marker_horizon_record_is_rejected_by_atomic_gate() -> Result {
    let (_, l, r) = fixture()?;
    let mut next = l.clone();
    let rev = next.replan(
        Vec::new(),
        vec![step(2, diff(2, "second.py")?, false)?],
        TaskReplanReason::try_from_string("injected".into())?,
        lt(10)?,
    )?;
    let mut records = next.replans().to_vec();
    records[0] = records[0]
        .clone()
        .with_cause(TaskLedgerReplanCause::HorizonExtension)?;
    let forged = TaskLedger::reconstruct(
        next.goal_contract(),
        next.revision(),
        next.steps().cloned().collect(),
        records,
        next.created_at(),
        next.updated_at(),
    )?;
    let mut nr = r.clone();
    let e = nr.record_ledger_update(eid(7), rev, RunEventPayload::empty(), snap(), ts(10)?)?;
    assert!(matches!(
        validate_pending_planning_transition(&l, &r, &forged, &nr, &e),
        Err(AgentHorizonFailure::InvalidPair)
    ));
    Ok(())
}
#[test]
fn foreign_job_id_and_request_position_are_rejected() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let e = request(&store, &g, &mut l, &mut r)?;
    for (id, seq) in [
        (eid(77), e.sequence()),
        (e.id(), RunEventSequence::new(e.sequence().get() + 1)?),
    ] {
        let p = PendingPlanning::new(r.id(), g.reference(), l.revision(), snap(), id, seq)?;
        let mut bare = l.clone(); // Reconstruct preserves actual steps but does not invent any run.
        bare = TaskLedger::reconstruct(
            bare.goal_contract(),
            bare.revision(),
            bare.steps().cloned().collect(),
            bare.replans().to_vec(),
            bare.created_at(),
            bare.updated_at(),
        )?
        .with_pending_planning(p)?;
        assert!(validate_pending_planning_pair(&bare, &r, &e).is_err());
    }
    Ok(())
}
#[test]
fn cancellation_before_request_makes_zero_writes() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(10)?,
            &Cancelled
        )),
        Err(AgentHorizonFailure::Cancelled)
    ));
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}
#[derive(Debug)]
struct Journal {
    run: AgentRun,
    event: RunEvent,
    cursors: Mutex<Vec<Option<RunEventSequence>>>,
}
impl RunJournalStore for Journal {
    fn create_agent_run<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a AgentRun,
        _: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
    }
    fn append_run_event<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: RunEventSequence,
        _: &'a AgentRun,
        _: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
    }
    fn append_agent_read<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: RunEventSequence,
        _: &'a AgentRun,
        _: &'a RecordedAgentRead,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
    }
    fn load_agent_run<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        id: AgentRunId,
    ) -> RunJournalStoreFuture<'a, Option<AgentRun>> {
        Box::pin(async move { Ok((id == self.run.id()).then(|| self.run.clone())) })
    }
    fn load_run_events<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: AgentRunId,
        after: Option<RunEventSequence>,
        limit: RunEventPageLimit,
    ) -> RunJournalStoreFuture<'a, RunEventPage> {
        Box::pin(async move {
            self.cursors
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(after);
            RunEventPage::new(after, limit, vec![self.event.clone()], true)
                .map_err(|_| RunJournalStoreFailure::InvalidStoredData)
        })
    }
}
#[test]
fn resume_loads_exact_original_event_after_more_than_sixty_four_later_events() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let e = request(&Store::default(), &g, &mut l, &mut r)?;
    for id in 8..80 {
        r.record(
            eid(id),
            RunEventKind::Diagnostic,
            RunEventPayload::empty(),
            snap(),
            None,
            ts(u64::from(id) + 10)?,
        )?;
    }
    let journal = Journal {
        run: r.clone(),
        event: e.clone(),
        cursors: Mutex::new(Vec::new()),
    };
    let (owner, original, disposition) =
        block_on(load_pending_planning_pair(&project()?, &l, &journal))?;
    assert_eq!(owner, r);
    assert_eq!(original, e);
    assert_eq!(disposition, PendingPlanningPairDisposition::Live);
    assert_eq!(
        journal
            .cursors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)[0]
            .ok_or("cursor")?
            .get(),
        e.sequence().get() - 1
    );
    Ok(())
}
#[test]
fn intrinsic_checkpoint_rejects_bad_revision_duplicate_ids_and_oversized_preview() -> Result {
    let (g, l, r) = fixture()?;
    let rev = TaskLedgerRevision::new(2)?;
    for ids in [
        Vec::new(),
        vec![sid(2), sid(2)],
        vec![sid(2), sid(3), sid(4), sid(5)],
    ] {
        assert!(
            PendingPlanning::reconstruct(
                r.id(),
                g.reference(),
                l.revision(),
                snap(),
                eid(7),
                RunEventSequence::new(7)?,
                PendingPlanningStage::PlanAppended {
                    revision: rev,
                    added_step_ids: ids
                }
            )
            .is_err()
        );
    }
    assert!(
        PendingPlanning::new(
            r.id(),
            g.reference(),
            l.revision(),
            snap(),
            eid(7),
            RunEventSequence::FIRST
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn complete_proves_mapping_returns_acceptance_request_without_claiming_done() -> Result {
    let (g, mut l, mut r) = fixture_with_proof(true)?;
    let store = Store::default();
    let outcome = block_on(ContinueSourceAgentPlan::new(&store).execute(
        &project()?,
        version()?,
        &g,
        &mut r,
        &mut l,
        snap(),
        eid(7),
        ts(10)?,
        &Active,
    ))?;
    assert!(matches!(
        outcome,
        SourcePlanContinuationOutcome::Continued(
            ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance
        )
    ));
    assert_eq!(r.state(), AgentControllerState::Verify);
    assert!(l.pending_planning().is_none());
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}
#[test]
fn consumed_elapsed_budget_does_not_get_a_new_horizon_or_write() -> Result {
    let (g, mut l, mut r) = fixture()?;
    let store = Store::default();
    let old = (l.clone(), r.clone());
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(7_200_002)?,
            &Active
        )),
        Err(AgentHorizonFailure::BudgetExhausted)
    ));
    assert_eq!((l, r), old);
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}

// Each case represents the last previously admissible model charge exactly
// reaching one ceiling. Reconstruct is a controlled persisted-state fixture.
fn at_model_ceiling(run: &AgentRun, dimension: AgentBudgetDimension) -> Result<AgentRun> {
    let d = AgentRunBudget::DEFAULT;
    let b = AgentRunBudget::new(
        AgentTurnLimit::new(if dimension == AgentBudgetDimension::Turns {
            1
        } else {
            d.turn_limit().get()
        })?,
        AgentTokenLimit::new(if dimension == AgentBudgetDimension::PromptTokens {
            400
        } else {
            d.prompt_token_limit().get()
        })?,
        AgentTokenLimit::new(if dimension == AgentBudgetDimension::OutputTokens {
            40
        } else {
            d.output_token_limit().get()
        })?,
        AgentActionLimit::new(if dimension == AgentBudgetDimension::Actions {
            1
        } else {
            d.action_limit().get()
        })?,
        d.duration_limit(),
        AgentRepairLimit::new(if dimension == AgentBudgetDimension::Repairs {
            1
        } else {
            d.repair_limit().get()
        })?,
    );
    Ok(AgentRun::reconstruct(
        AgentRunIdentity::with_budget(
            run.id(),
            run.goal_contract(),
            run.task_ledger_revision(),
            run.model_profile(),
            b,
        ),
        AgentRunMaterializedState::with_usage(
            run.state(),
            run.last_event_sequence(),
            run.current_snapshot_id(),
            run.usage(),
        ),
        AgentRunTiming::new(run.created_at(), run.updated_at()),
    )?)
}
#[test]
fn exact_model_ceilings_allow_only_existing_deterministic_acceptance_without_new_charge() -> Result
{
    for dimension in [
        AgentBudgetDimension::Turns,
        AgentBudgetDimension::PromptTokens,
        AgentBudgetDimension::OutputTokens,
        AgentBudgetDimension::Actions,
        AgentBudgetDimension::Repairs,
    ] {
        let (g, mut l, r) = fixture_with_proof(true)?;
        let mut r = at_model_ceiling(&r, dimension)?;
        assert_eq!(
            r.budget_exhaustion(ts(10)?)?
                .ok_or("exhaustion")?
                .dimension(),
            dimension
        );
        let store = Store::default();
        let old = (l.clone(), r.clone());
        let mut legacy_l = l.clone();
        let mut legacy_r = r.clone();
        assert!(matches!(
            block_on(ContinueVerifiedAgentPlan::new(&store).execute(
                &project()?,
                version()?,
                &mut legacy_r,
                &mut legacy_l,
                eid(7),
                ts(10)?,
                &Active
            ))?,
            ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance
        ));
        assert!(matches!(
            block_on(ContinueSourceAgentPlan::new(&store).execute(
                &project()?,
                version()?,
                &g,
                &mut r,
                &mut l,
                snap(),
                eid(7),
                ts(10)?,
                &Active
            ))?,
            SourcePlanContinuationOutcome::Continued(
                ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance
            )
        ));
        assert_eq!((l, r), old);
        assert!(
            store
                .commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
        // Keeping a global proof open needs another horizon and remains forbidden.
        let (g, mut l, r) = fixture()?;
        let mut r = at_model_ceiling(&r, dimension)?;
        let old = (l.clone(), r.clone());
        assert!(matches!(
            block_on(ContinueSourceAgentPlan::new(&store).execute(
                &project()?,
                version()?,
                &g,
                &mut r,
                &mut l,
                snap(),
                eid(7),
                ts(10)?,
                &Active
            )),
            Err(AgentHorizonFailure::BudgetExhausted)
        ));
        assert_eq!((l, r), old);
        assert!(
            store
                .commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }
    Ok(())
}
#[test]
fn deterministic_acceptance_keeps_time_and_cancellation_hard_at_model_ceiling() -> Result {
    let (g, mut l, r) = fixture_with_proof(true)?;
    let mut r = at_model_ceiling(&r, AgentBudgetDimension::Turns)?;
    let store = Store::default();
    let old = (l.clone(), r.clone());
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(7_200_001)?,
            &Active
        )),
        Err(AgentHorizonFailure::BudgetExhausted)
    ));
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(10)?,
            &Cancelled
        )),
        Err(AgentHorizonFailure::Cancelled)
    ));
    assert_eq!((l, r), old);
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}
#[test]
fn model_ceiling_never_bypasses_original_constraints_or_snapshot_admission() -> Result {
    let (g, mut l, r) = fixture_with_proof(true)?;
    let mut r = at_model_ceiling(&r, AgentBudgetDimension::Turns)?;
    let store = Store::default();
    let draft = g.draft();
    let incomplete = GoalContract::initial(
        g.task_id(),
        GoalContractDraft::new(
            draft.objective().clone(),
            draft.acceptance_criteria().to_vec(),
            vec![GoalConstraint::try_from_string(
                "An unbound original constraint".into(),
            )?],
            draft.non_goals().to_vec(),
            draft.user_decisions().to_vec(),
            draft.success_verification().clone(),
        )?,
        g.created_at(),
    )
    .with_original(g.original().ok_or("original")?.clone())?;
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &incomplete,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(10)?,
            &Active
        )),
        Err(AgentHorizonFailure::InvalidState)
    ));
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            SnapshotId::from_bytes([123; 32]),
            eid(7),
            ts(10)?,
            &Active
        )),
        Err(AgentHorizonFailure::SnapshotChanged)
    ));
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}

fn with_persisted_usage(run: &AgentRun, usage: AgentRunUsage) -> Result<AgentRun> {
    Ok(AgentRun::reconstruct(
        AgentRunIdentity::with_budget(
            run.id(),
            run.goal_contract(),
            run.task_ledger_revision(),
            run.model_profile(),
            run.budget(),
        ),
        AgentRunMaterializedState::with_usage(
            run.state(),
            run.last_event_sequence(),
            run.current_snapshot_id(),
            usage,
        ),
        AgentRunTiming::new(run.created_at(), run.updated_at()),
    )?)
}
#[test]
fn pure_acceptance_never_excuses_an_overrun_in_any_model_dimension() -> Result {
    for (dimension, counts) in [
        (AgentBudgetDimension::Turns, (2, 400, 40, 1, 1)),
        (AgentBudgetDimension::PromptTokens, (1, 401, 40, 1, 1)),
        (AgentBudgetDimension::OutputTokens, (1, 400, 41, 1, 1)),
        (AgentBudgetDimension::Actions, (2, 400, 40, 2, 1)),
        (AgentBudgetDimension::Repairs, (2, 400, 40, 1, 2)),
    ] {
        let (g, mut l, r) = fixture_with_proof(true)?;
        let r = at_model_ceiling(&r, dimension)?;
        let mut r = with_persisted_usage(
            &r,
            AgentRunUsage::reconstruct(counts.0, counts.1, counts.2, counts.3, counts.4)?,
        )?;
        let store = Store::default();
        let old = (l.clone(), r.clone());
        assert!(matches!(
            block_on(ContinueSourceAgentPlan::new(&store).execute(
                &project()?,
                version()?,
                &g,
                &mut r,
                &mut l,
                snap(),
                eid(7),
                ts(10)?,
                &Active
            )),
            Err(AgentHorizonFailure::BudgetExhausted)
        ));
        assert_eq!((l, r), old);
        assert!(
            store
                .commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }
    Ok(())
}
#[test]
fn exact_earlier_exhaustion_does_not_hide_a_later_actual_overrun() -> Result {
    let (g, mut l, r) = fixture_with_proof(true)?;
    // Turns is first and exactly exhausted; PromptTokens exceeds its hard ceiling.
    let mut r = at_model_ceiling(&r, AgentBudgetDimension::Turns)?;
    r = with_persisted_usage(
        &r,
        AgentRunUsage::reconstruct(1, r.budget().prompt_token_limit().get() + 1, 40, 1, 1)?,
    )?;
    let first = r.budget_exhaustion(ts(10)?)?.ok_or("exhaustion")?;
    assert_eq!(first.dimension(), AgentBudgetDimension::Turns);
    assert_eq!(first.observed(), first.limit());
    let store = Store::default();
    let old = (l.clone(), r.clone());
    assert!(matches!(
        block_on(ContinueSourceAgentPlan::new(&store).execute(
            &project()?,
            version()?,
            &g,
            &mut r,
            &mut l,
            snap(),
            eid(7),
            ts(10)?,
            &Active
        )),
        Err(AgentHorizonFailure::BudgetExhausted)
    ));
    assert_eq!((l, r), old);
    assert!(
        store
            .commits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}
