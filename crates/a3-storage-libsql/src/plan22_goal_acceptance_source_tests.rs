//! Native rejection controls: passing step metadata must not bypass original-source readiness.
//! No model, process or operational-success claim is produced by these fixtures.
use super::*;
use a3_application::{
    AcceptanceVerificationRequest, AcceptanceVerifier, AcceptanceVerifierOutcome,
    AcceptanceVerifierTimeout, AdvanceAgentController, AgentControllerAdvanceKind,
    AgentControllerControl, AgentControllerSignal, DeterministicAcceptanceVerifier,
    StoredVerificationState, VerificationEvidenceStore, VerificationEvidenceStoreFailure,
    VerificationEvidenceStoreFuture, VerifyAgentAcceptance,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, GoalOriginalCoverage,
    GoalOriginalEntryKind, IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence,
    IndexRunStatus, LinkedGraph, ModelProfileId, ModelProfileReference, ModelProfileVersion,
    ModulePolicyVersion, ModuleProjection, ModuleSymbolSet, PublishedIndex, RankProjection,
    RankingPolicyVersion, RepositoryCard, RunEventId, RunMemoryCheckpoint, SnapshotId,
    StepVerification, StepVerificationId, StepVerificationOutcome, TaskEvidenceId, TaskLedger,
    TaskLedgerTimestamp, TaskStepStatus, VerificationEvidence,
};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[derive(Clone, Copy)]
enum SourceCase {
    UncoveredOriginal,
    UnboundConstraint,
}

fn isolated_goal(case: SourceCase) -> TestResult<GoalContract> {
    let template = source_goal(small_original(), test_requirement()?)?;
    let (bytes, end, constraints) = match case {
        SourceCase::UncoveredOriginal => {
            let bytes = small_original();
            let end = std::str::from_utf8(&bytes)?
                .find('\n')
                .ok_or("first line missing")?
                + 1;
            (bytes, end, Vec::new())
        }
        SourceCase::UnboundConstraint => {
            let bytes = b"R01: repair average\nDo not modify foreign files".to_vec();
            let end = bytes.len();
            (
                bytes,
                end,
                vec![GoalConstraint::try_from_string(
                    "Do not modify foreign files".to_owned(),
                )?],
            )
        }
    };
    let original = GoalOriginalRequest::try_from_bytes(template.reference(), bytes)?;
    let criterion = template.draft().acceptance_criteria()[0]
        .clone()
        .with_origin(CriterionOrigin::new(
            original.range(0, end)?,
            test_requirement()?,
        ));
    let draft = GoalContractDraft::new(
        template.draft().objective().clone(),
        vec![criterion],
        constraints,
        Vec::new(),
        Vec::new(),
        template.draft().success_verification().clone(),
    )?;
    Ok(
        GoalContract::initial(template.task_id(), draft, template.created_at())
            .with_original(original)?,
    )
}

#[derive(Debug, Default)]
struct UnreachableEvidenceStore {
    reads: AtomicUsize,
}
impl VerificationEvidenceStore for UnreachableEvidenceStore {
    fn append_verification_evidence<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _evidence: &'a VerificationEvidence,
        _timeout: Duration,
        _control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, ()> {
        Box::pin(async { Err(VerificationEvidenceStoreFailure::Unavailable) })
    }
    fn load_verification_state<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task: TaskId,
        _ids: &'a [TaskEvidenceId],
        _snapshot: SnapshotId,
        _timeout: Duration,
        _control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(VerificationEvidenceStoreFailure::Unavailable) })
    }
    fn load_verification_inspection_state<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _task: TaskId,
        _ids: &'a [TaskEvidenceId],
        _timeout: Duration,
        _control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(VerificationEvidenceStoreFailure::Unavailable) })
    }
}
#[derive(Debug)]
struct ActiveControl;
impl AgentControllerControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

fn current_index(snapshot: SnapshotId) -> TestResult<PublishedIndex> {
    let graph = LinkedGraph::new(snapshot, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot, RankingPolicyVersion::v1(), Vec::new())?;
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
    let modules = ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
    Ok(PublishedIndex::new(
        IndexRunRecord::new(
            IndexRunId::from_bytes([68; 32]),
            snapshot,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        ),
        publication,
    )?)
}

fn verifying_run(goal: &GoalContract, ledger: &mut TaskLedger) -> TestResult<AgentRun> {
    let run_id = AgentRunId::from_bytes([61; 32]);
    let snapshot = SnapshotId::from_bytes([64; 32]);
    let profile = ModelProfileReference::new(
        ModelProfileId::from_bytes([69; 32]),
        ModelProfileVersion::V1,
    );
    let (mut run, _start) = AgentRun::start(
        run_id,
        goal.reference(),
        ledger.revision(),
        profile,
        snapshot,
        RunEventId::from_bytes([70; 32]),
        AgentRunTimestamp::from_unix_millis(11)?,
    )?;
    for (ordinal, signal) in [
        AgentControllerSignal::AnchorsAccepted,
        AgentControllerSignal::LocalizationComplete,
        AgentControllerSignal::PlanReady,
    ]
    .into_iter()
    .enumerate()
    {
        AdvanceAgentController.execute(
            &mut run,
            signal,
            RunEventId::from_bytes([71 + u8::try_from(ordinal)?; 32]),
            snapshot,
            AgentRunTimestamp::from_unix_millis(12 + u64::try_from(ordinal)?)?,
            false,
        )?;
    }
    let step_id = TaskStepId::from_bytes([43; 32]);
    let evidence = TaskEvidenceId::from_bytes([62; 32]);
    ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(15)?)?;
    ledger.begin_step_verification(
        step_id,
        run_id,
        None,
        vec![evidence],
        TaskLedgerTimestamp::from_unix_millis(16)?,
    )?;
    // Deliberately strong rejection fixture: valid immutable passing verification metadata.
    // It is not claimed to be executed test evidence; the gate must reject before loading it.
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([63; 32]),
            test_spec()?.id(),
            run_id,
            StepVerificationOutcome::Passed,
            vec![evidence],
            TaskLedgerTimestamp::from_unix_millis(17)?,
        )?,
    )?;
    AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::TurnNeedsVerification,
        RunEventId::from_bytes([74; 32]),
        snapshot,
        AgentRunTimestamp::from_unix_millis(18)?,
        false,
    )?;
    assert_eq!(run.state(), AgentControllerState::Verify);
    // Attempt status transitions preserve the plan revision; only a replan advances it.
    assert_eq!(run.task_ledger_revision(), ledger.revision());
    Ok(run)
}

async fn source_rejection_after_reopen(case: SourceCase) -> TestResult {
    let fixture = Fixture::new()?;
    let goal = isolated_goal(case)?;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let prepared = MaterializeOriginalTask::new(&store, &store)
        .execute(&fixture.project, &goal, vec![step(test_spec()?)?])
        .await?;
    assert!(prepared.unplanned_criteria().is_empty());
    let mut ledger = prepared.stored_ledger().ledger().clone();
    let mut run = verifying_run(&goal, &mut ledger)?;
    let version = store
        .replace_task_ledger(
            &fixture.project,
            prepared.stored_ledger().version(),
            &ledger,
        )
        .await?;
    drop(store);
    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let loaded = reopened
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("loaded goal missing")?;
    let stored = reopened
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("loaded ledger missing")?;
    assert_eq!(loaded, goal);
    assert_eq!(stored.ledger(), &ledger);
    assert_eq!(stored.version(), version);
    assert_eq!(run.task_ledger_revision(), stored.ledger().revision());
    assert!(stored.ledger().steps().all(|step| {
        step.status() == TaskStepStatus::Completed
            && step
                .attempts()
                .last()
                .and_then(a3_domain::TaskStepAttempt::verification)
                .is_some_and(StepVerification::passed)
    }));
    let coverage = GoalOriginalCoverage::derive_from(&loaded)?;
    assert!(!coverage.is_complete());
    match case {
        SourceCase::UncoveredOriginal => {
            assert!(!coverage.gaps().is_empty());
            assert!(coverage.unbound_entries().is_empty());
            assert!(
                loaded
                    .original()
                    .ok_or("original disappeared")?
                    .resolve(coverage.gaps()[0])?
                    .contains("R30:")
            );
        }
        SourceCase::UnboundConstraint => {
            assert!(coverage.gaps().is_empty());
            assert_eq!(coverage.unbound_entries().len(), 1);
            assert_eq!(
                coverage.unbound_entries()[0].kind(),
                GoalOriginalEntryKind::Constraint
            );
        }
    }
    let memory = RunMemoryCheckpoint::compile(
        &loaded,
        stored.ledger(),
        &run,
        &current_index(run.current_snapshot_id())?,
        Vec::new(),
    )?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &run,
        loaded.clone(),
        stored.ledger().clone(),
        memory,
    )?;
    let evidence = UnreachableEvidenceStore::default();
    let verifier = DeterministicAcceptanceVerifier::new(&evidence);
    let result = verifier
        .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
        .await?;
    assert!(matches!(result, AcceptanceVerifierOutcome::Rejected(_)));
    assert_eq!(evidence.reads.load(Ordering::SeqCst), 0);
    let advance = VerifyAgentAcceptance::new(&verifier)
        .execute(
            &mut run,
            &request,
            RunEventId::from_bytes([75; 32]),
            AgentRunTimestamp::from_unix_millis(19)?,
            &ActiveControl,
        )
        .await?;
    assert!(matches!(
        advance.kind(),
        AgentControllerAdvanceKind::AcceptanceRejected(_)
    ));
    assert_ne!(run.state(), AgentControllerState::Done);
    assert_eq!(evidence.reads.load(Ordering::SeqCst), 0);
    assert_eq!(
        reopened
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?,
        Some(loaded)
    );
    let unchanged = reopened
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("ledger disappeared")?;
    assert_eq!(unchanged, stored);
    drop(reopened);
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn reopened_completed_ledger_with_source_gap_never_reaches_evidence_or_done() -> TestResult {
    crate::run_native_libsql_test(source_rejection_after_reopen(SourceCase::UncoveredOriginal))
}

#[test]
fn reopened_completed_ledger_with_only_unbound_constraint_never_reaches_evidence_or_done()
-> TestResult {
    crate::run_native_libsql_test(source_rejection_after_reopen(SourceCase::UnboundConstraint))
}
