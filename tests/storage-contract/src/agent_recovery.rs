use crate::fixture::{ContractWorkspace, change, project, run_at, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory, index};
use a3_application::{
    AgentMutationResultRecord, AgentReadResult, AgentRecoveryChoice, AgentRecoveryError,
    AgentRecoveryOutcomeKind, AgentRecoveryStore, AgentRecoveryStoreFailure,
    ContextToolResultDigest, ContextToolResultPreview, ContextToolResultStatus, CreateAgentRun,
    CreateGoalContract, CreateTaskLedger, IndexPersistenceControl, IndexPersistenceControlError,
    InspectAgentRunRecovery, KnowledgeIndexStore, RecoverAgentRun, RunJournalStore, SaveTaskLedger,
    TaskLedgerStore, TaskLedgerStoreVersion,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentControllerState,
    AgentMutationDisposition, AgentMutationKind, AgentRun, AgentRunId, AgentRunTimestamp,
    AgentToolAttemptNumber, AgentToolAttemptStatus, AgentToolEvidence, AgentToolEvidenceSet,
    ContentHash, ExpectedTaskEvidence, FileRevision, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, ModelProfileId, ModelProfileReference,
    ModelProfileVersion, MutationActionFingerprint, MutationReconciliation, RepositoryId,
    RepositoryPath, RunEventCode, RunEventId, RunEventKind, RunEventOutcome, RunEventPayload,
    RunEventSequence, RunEventSubject, SnapshotChangeKind, SnapshotId, StepVerification,
    StepVerificationId, StepVerificationOutcome, SuccessVerification, TaskId, TaskLedger,
    TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale,
    TaskStepResultSummary, TaskStepStatus, ToolRunId, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId, WorktreeId,
};

#[derive(Debug)]
struct RecoveryIndexControl;

impl IndexPersistenceControl for RecoveryIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("agent-recovery");
    let common = workspace.create_directory("agent-recovery-common")?;
    let root = workspace.create_directory("agent-recovery-root")?;
    let worktree_id = WorktreeId::from_bytes([181; 32]);
    let project = project(
        RepositoryId::from_bytes([180; 32]),
        worktree_id,
        &common,
        &root,
        unborn_head()?,
    )?;
    let snapshot_one_id = SnapshotId::from_bytes([182; 32]);
    let snapshot_one = snapshot(
        *snapshot_one_id.as_bytes(),
        worktree_id,
        None,
        1,
        vec![change(
            b"src/lib.rs",
            [183; 32],
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    let task_id = TaskId::from_bytes([184; 32]);
    let goal = GoalContract::initial(
        task_id,
        goal_draft()?,
        GoalContractTimestamp::from_unix_millis(10)?,
    );
    let step_id = TaskStepId::from_bytes([185; 32]);
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![task_step(step_id)?],
        TaskLedgerTimestamp::from_unix_millis(11)?,
    )?;
    let run_id = AgentRunId::from_bytes([186; 32]);
    let (mut agent_run, start_event) = AgentRun::start(
        run_id,
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([187; 32]),
            ModelProfileVersion::V1,
        ),
        snapshot_one_id,
        RunEventId::from_bytes([188; 32]),
        AgentRunTimestamp::from_unix_millis(12)?,
    )?;

    let store = factory.open(&app_data_root).await?;
    store.append_snapshot(&project, &snapshot_one).await?;
    publish(
        &store,
        &project,
        snapshot_one_id,
        [189; 32],
        b"src/lib.rs",
        [183; 32],
        190,
    )
    .await?;
    CreateGoalContract::new(&store)
        .execute(&project, &goal)
        .await?;
    CreateTaskLedger::new(&store)
        .execute(&project, &ledger)
        .await?;
    CreateAgentRun::new(&store)
        .execute(&project, &agent_run, &start_event)
        .await?;
    advance_to_execute(&store, &project, &mut agent_run).await?;

    let mutation_tool = ToolRunId::from_bytes([210; 32]);
    let mutation_attempt = store
        .begin_agent_mutation_attempt(
            &project,
            run_id,
            snapshot_one_id,
            mutation_tool,
            MutationActionFingerprint::from_bytes([211; 32]),
            AgentMutationKind::Patch,
            AgentRunTimestamp::from_unix_millis(16)?,
        )
        .await
        .map_err(|error| std::io::Error::other(format!("mutation attempt failed: {error:?}")))?;
    let expected_sequence = agent_run.last_event_sequence();
    let mut mutation_run = agent_run.clone();
    let mutation_event = mutation_run.record(
        RunEventId::from_bytes([221; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
        snapshot_one_id,
        Some(RunEventSubject::Tool(mutation_tool)),
        AgentRunTimestamp::from_unix_millis(17)?,
    )?;
    let mutation_result =
        AgentMutationResultRecord::new(ContextToolResultDigest::from_bytes([212; 32]), false, 0);
    assert_eq!(
        store
            .complete_agent_mutation_attempt(
                &project,
                RunEventSequence::new(expected_sequence.get() - 1)?,
                &mutation_run,
                &mutation_event,
                mutation_tool,
                mutation_attempt.tool_attempt().attempt(),
                mutation_result,
            )
            .await,
        Err(AgentRecoveryStoreFailure::RunSequenceConflict),
        "a stale journal CAS must roll back the tool-attempt update"
    );
    let completed_mutation = store
        .complete_agent_mutation_attempt(
            &project,
            expected_sequence,
            &mutation_run,
            &mutation_event,
            mutation_tool,
            mutation_attempt.tool_attempt().attempt(),
            mutation_result,
        )
        .await
        .map_err(|error| std::io::Error::other(format!("mutation completion failed: {error:?}")))?;
    assert_eq!(
        completed_mutation.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_eq!(
        completed_mutation.disposition(),
        AgentMutationDisposition::Applied
    );
    agent_run = mutation_run;

    let not_applied_tool = ToolRunId::from_bytes([230; 32]);
    let not_applied = store
        .begin_agent_mutation_attempt(
            &project,
            run_id,
            snapshot_one_id,
            not_applied_tool,
            MutationActionFingerprint::from_bytes([231; 32]),
            AgentMutationKind::Process,
            AgentRunTimestamp::from_unix_millis(18)?,
        )
        .await?;
    let not_applied = store
        .finish_agent_mutation_attempt(
            &project,
            not_applied_tool,
            not_applied.tool_attempt().attempt(),
            AgentToolAttemptStatus::Failed,
            AgentMutationDisposition::NotApplied,
            AgentRunTimestamp::from_unix_millis(19)?,
        )
        .await?;
    assert_eq!(
        not_applied.disposition(),
        AgentMutationDisposition::NotApplied
    );

    let unknown_tool = ToolRunId::from_bytes([232; 32]);
    let unknown = store
        .begin_agent_mutation_attempt(
            &project,
            run_id,
            snapshot_one_id,
            unknown_tool,
            MutationActionFingerprint::from_bytes([233; 32]),
            AgentMutationKind::Patch,
            AgentRunTimestamp::from_unix_millis(20)?,
        )
        .await?;
    assert_eq!(
        store
            .begin_agent_mutation_attempt(
                &project,
                run_id,
                snapshot_one_id,
                ToolRunId::from_bytes([234; 32]),
                MutationActionFingerprint::from_bytes([235; 32]),
                AgentMutationKind::Process,
                AgentRunTimestamp::from_unix_millis(21)?,
            )
            .await,
        Err(AgentRecoveryStoreFailure::MutationReconciliationRequired)
    );
    assert_eq!(
        store
            .interrupt_agent_tool_attempts(
                &project,
                run_id,
                AgentRunTimestamp::from_unix_millis(21)?,
            )
            .await?,
        1
    );
    let loaded = store.load_agent_mutation_attempts(&project, run_id).await?;
    let loaded_unknown = loaded
        .iter()
        .find(|candidate| candidate.tool_attempt().tool_run_id() == unknown_tool)
        .ok_or_else(|| std::io::Error::other("unknown mutation was not persisted"))?;
    assert_eq!(
        loaded_unknown.disposition(),
        AgentMutationDisposition::Unknown(MutationReconciliation::Required)
    );
    assert_eq!(
        loaded_unknown.tool_attempt().status(),
        AgentToolAttemptStatus::Interrupted
    );

    let expected_sequence = agent_run.last_event_sequence();
    let mut reconciled_run = agent_run.clone();
    let reconciliation_event = reconciled_run.record(
        RunEventId::from_bytes([236; 32]),
        RunEventKind::Diagnostic,
        RunEventPayload::new(
            RunEventCode::StateRecovered,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        snapshot_one_id,
        Some(RunEventSubject::Tool(unknown_tool)),
        AgentRunTimestamp::from_unix_millis(22)?,
    )?;
    let reconciled = store
        .reconcile_agent_mutation(
            &project,
            expected_sequence,
            &reconciled_run,
            &reconciliation_event,
            unknown_tool,
            unknown.tool_attempt().attempt(),
        )
        .await?;
    assert_eq!(
        reconciled.disposition(),
        AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled {
            snapshot_id: snapshot_one_id,
        })
    );
    agent_run = reconciled_run;

    let post_reconciliation_tool = ToolRunId::from_bytes([237; 32]);
    assert_eq!(
        store
            .begin_agent_mutation_attempt(
                &project,
                run_id,
                snapshot_one_id,
                post_reconciliation_tool,
                MutationActionFingerprint::from_bytes([238; 32]),
                AgentMutationKind::Process,
                AgentRunTimestamp::from_unix_millis(23)?,
            )
            .await,
        Err(AgentRecoveryStoreFailure::MutationReconciliationRequired),
        "reconciliation alone must not bypass the mandatory recovery Replan"
    );

    let evidence = AgentToolEvidence::for_file(FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
        ContentHash::from_bytes([183; 32]),
    ));
    let evidence_id = evidence.id();
    let completed_tool = ToolRunId::from_bytes([191; 32]);
    let completed_attempt = store
        .begin_agent_tool_attempt(
            &project,
            run_id,
            snapshot_one_id,
            completed_tool,
            AgentRunTimestamp::from_unix_millis(25)?,
        )
        .await?;
    assert_eq!(completed_attempt.attempt(), AgentToolAttemptNumber::FIRST);
    let expected_sequence = agent_run.last_event_sequence();
    let recorded = AgentReadResult::new(
        completed_tool,
        ContextToolResultStatus::Succeeded,
        ContextToolResultPreview::try_from_string("current source".to_owned())?,
        ContextToolResultDigest::from_bytes([192; 32]),
        false,
        snapshot_one_id,
        AgentToolEvidenceSet::new(snapshot_one_id, vec![evidence])?,
        14,
    )?
    .record(
        &mut agent_run,
        RunEventId::from_bytes([193; 32]),
        AgentRunTimestamp::from_unix_millis(26)?,
    )?;
    store
        .append_agent_read(&project, expected_sequence, &agent_run, &recorded)
        .await?;

    ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(27)?)?;
    ledger.begin_step_verification(
        step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            "verified against current source".to_owned(),
        )?),
        vec![evidence_id],
        TaskLedgerTimestamp::from_unix_millis(28)?,
    )?;
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([194; 32]),
            VerificationSpecId::from_bytes([195; 32]),
            run_id,
            StepVerificationOutcome::Passed,
            vec![evidence_id],
            TaskLedgerTimestamp::from_unix_millis(29)?,
        )?,
    )?;
    let saved = SaveTaskLedger::new(&store)
        .execute(&project, TaskLedgerStoreVersion::INITIAL, &ledger)
        .await?;
    assert_eq!(
        saved.ledger().step(step_id).map(|step| step.status()),
        Some(TaskStepStatus::Completed)
    );

    let interrupted_tool = ToolRunId::from_bytes([196; 32]);
    store
        .begin_agent_tool_attempt(
            &project,
            run_id,
            snapshot_one_id,
            interrupted_tool,
            AgentRunTimestamp::from_unix_millis(30)?,
        )
        .await?;
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data_root).await?;
    let inspection = InspectAgentRunRecovery::new(&reopened, &reopened, &reopened, &reopened)
        .execute(
            &project,
            run_id,
            AgentRunTimestamp::from_unix_millis(31)?,
            &RecoveryIndexControl,
        )
        .await?;
    assert_eq!(inspection.interrupted_tool_attempts(), 1);
    assert!(!inspection.snapshot_changed());
    assert!(!inspection.can_resume());
    assert!(!inspection.mutation_reconciliation_required());
    assert!(inspection.mutation_replan_required());
    assert!(inspection.stale_evidence_ids().is_empty());
    let retry = reopened
        .begin_agent_tool_attempt(
            &project,
            run_id,
            snapshot_one_id,
            interrupted_tool,
            AgentRunTimestamp::from_unix_millis(32)?,
        )
        .await?;
    assert_eq!(retry.attempt(), AgentToolAttemptNumber::new(2)?);
    assert_eq!(
        reopened
            .finish_agent_tool_attempt(
                &project,
                interrupted_tool,
                retry.attempt(),
                AgentToolAttemptStatus::Succeeded,
                AgentRunTimestamp::from_unix_millis(33)?,
            )
            .await,
        Err(AgentRecoveryStoreFailure::InvalidStoredData),
        "only the atomic result-journal transaction may mark an attempt succeeded"
    );
    let failed_retry = reopened
        .finish_agent_tool_attempt(
            &project,
            interrupted_tool,
            retry.attempt(),
            AgentToolAttemptStatus::Failed,
            AgentRunTimestamp::from_unix_millis(33)?,
        )
        .await?;
    assert_eq!(failed_retry.status(), AgentToolAttemptStatus::Failed);

    let mutation_replanned = RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
        .execute(
            &project,
            run_id,
            AgentRecoveryChoice::Replan,
            RunEventId::from_bytes([197; 32]),
            AgentRunTimestamp::from_unix_millis(34)?,
            &RecoveryIndexControl,
        )
        .await?;
    assert_eq!(
        mutation_replanned.kind(),
        AgentRecoveryOutcomeKind::ReplanRequired
    );
    assert_eq!(
        mutation_replanned.run().current_snapshot_id(),
        snapshot_one_id
    );
    assert_eq!(
        mutation_replanned
            .ledger()
            .ledger()
            .step(step_id)
            .map(|step| step.status()),
        Some(TaskStepStatus::Completed)
    );
    let post_reconciliation = reopened
        .begin_agent_mutation_attempt(
            &project,
            run_id,
            snapshot_one_id,
            post_reconciliation_tool,
            MutationActionFingerprint::from_bytes([238; 32]),
            AgentMutationKind::Process,
            AgentRunTimestamp::from_unix_millis(35)?,
        )
        .await?;
    reopened
        .finish_agent_mutation_attempt(
            &project,
            post_reconciliation_tool,
            post_reconciliation.tool_attempt().attempt(),
            AgentToolAttemptStatus::Denied,
            AgentMutationDisposition::NotApplied,
            AgentRunTimestamp::from_unix_millis(36)?,
        )
        .await?;

    let snapshot_two_id = SnapshotId::from_bytes([198; 32]);
    let snapshot_two = snapshot(
        *snapshot_two_id.as_bytes(),
        worktree_id,
        Some(snapshot_one_id),
        2,
        vec![change(
            b"src/lib.rs",
            [199; 32],
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    reopened.append_snapshot(&project, &snapshot_two).await?;
    publish(
        &reopened,
        &project,
        snapshot_two_id,
        [200; 32],
        b"src/lib.rs",
        [199; 32],
        201,
    )
    .await?;
    let stale_inspection = InspectAgentRunRecovery::new(&reopened, &reopened, &reopened, &reopened)
        .execute(
            &project,
            run_id,
            AgentRunTimestamp::from_unix_millis(37)?,
            &RecoveryIndexControl,
        )
        .await?;
    assert!(stale_inspection.snapshot_changed());
    assert!(!stale_inspection.can_resume());
    assert_eq!(stale_inspection.stale_evidence_ids(), &[evidence_id]);
    let before_rejected_resume = reopened
        .load_agent_run(&project, run_id)
        .await?
        .ok_or_else(|| std::io::Error::other("recovered run disappeared"))?;
    assert!(matches!(
        RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
            .execute(
                &project,
                run_id,
                AgentRecoveryChoice::Resume,
                RunEventId::from_bytes([202; 32]),
                AgentRunTimestamp::from_unix_millis(38)?,
                &RecoveryIndexControl,
            )
            .await,
        Err(AgentRecoveryError::ResumeRequiresReplan)
    ));
    assert_eq!(
        reopened.load_agent_run(&project, run_id).await?,
        Some(before_rejected_resume)
    );

    let replanned = RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
        .execute(
            &project,
            run_id,
            AgentRecoveryChoice::Replan,
            RunEventId::from_bytes([203; 32]),
            AgentRunTimestamp::from_unix_millis(39)?,
            &RecoveryIndexControl,
        )
        .await?;
    assert_eq!(replanned.kind(), AgentRecoveryOutcomeKind::ReplanRequired);
    assert_eq!(replanned.reopened_step_ids(), &[step_id]);
    assert_eq!(replanned.run().current_snapshot_id(), snapshot_two_id);
    assert_eq!(
        replanned
            .ledger()
            .ledger()
            .step(step_id)
            .map(|step| step.status()),
        Some(TaskStepStatus::Ready)
    );

    let snapshot_three_id = SnapshotId::from_bytes([204; 32]);
    let snapshot_three = snapshot(
        *snapshot_three_id.as_bytes(),
        worktree_id,
        Some(snapshot_two_id),
        3,
        Vec::new(),
    )?;
    reopened.append_snapshot(&project, &snapshot_three).await?;
    publish(
        &reopened,
        &project,
        snapshot_three_id,
        [205; 32],
        b"src/lib.rs",
        [199; 32],
        206,
    )
    .await?;
    let mut stale_commit_run = replanned.run().clone();
    let stale_event = stale_commit_run.record(
        RunEventId::from_bytes([207; 32]),
        RunEventKind::Diagnostic,
        RunEventPayload::new(
            a3_domain::RunEventCode::StateRecovered,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        snapshot_two_id,
        None,
        AgentRunTimestamp::from_unix_millis(40)?,
    )?;
    assert_eq!(
        reopened
            .commit_agent_recovery(
                &project,
                AgentRecoveryChoice::Resume,
                snapshot_two_id,
                replanned.ledger().version(),
                replanned.run().last_event_sequence(),
                replanned.ledger().ledger(),
                &stale_commit_run,
                &stale_event,
            )
            .await,
        Err(AgentRecoveryStoreFailure::PublishedSnapshotConflict)
    );
    assert_eq!(
        reopened.load_agent_run(&project, run_id).await?,
        Some(replanned.run().clone())
    );
    assert_eq!(
        reopened
            .load_task_ledger(&project, task_id)
            .await?
            .map(|stored| stored.version()),
        Some(replanned.ledger().version())
    );

    let mut conflicting_run = replanned.run().clone();
    let conflicting_event = conflicting_run.record(
        RunEventId::from_bytes([220; 32]),
        RunEventKind::Diagnostic,
        RunEventPayload::new(
            a3_domain::RunEventCode::StateRecovered,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        snapshot_three_id,
        None,
        AgentRunTimestamp::from_unix_millis(40)?,
    )?;
    assert_eq!(
        reopened
            .commit_agent_recovery(
                &project,
                AgentRecoveryChoice::Resume,
                snapshot_three_id,
                replanned.ledger().version(),
                RunEventSequence::FIRST,
                replanned.ledger().ledger(),
                &conflicting_run,
                &conflicting_event,
            )
            .await,
        Err(AgentRecoveryStoreFailure::RunSequenceConflict)
    );
    assert_eq!(
        reopened.load_agent_run(&project, run_id).await?,
        Some(replanned.run().clone())
    );
    assert_eq!(
        reopened
            .load_task_ledger(&project, task_id)
            .await?
            .map(|stored| stored.version()),
        Some(replanned.ledger().version()),
        "a run CAS failure must roll back the preceding Ledger replacement"
    );

    let cancelled = RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
        .execute(
            &project,
            run_id,
            AgentRecoveryChoice::Cancel,
            RunEventId::from_bytes([208; 32]),
            AgentRunTimestamp::from_unix_millis(41)?,
            &RecoveryIndexControl,
        )
        .await?;
    assert_eq!(cancelled.kind(), AgentRecoveryOutcomeKind::Cancelled);
    assert_eq!(cancelled.run().state(), AgentControllerState::Cancelled);
    assert_eq!(cancelled.run().current_snapshot_id(), snapshot_three_id);
    assert_eq!(
        reopened
            .latest_published_index(&project, &RecoveryIndexControl)
            .await?
            .map(|published| published.run().snapshot_id()),
        Some(snapshot_three_id),
        "recovery must not mutate or corrupt the published index"
    );
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

async fn advance_to_execute<S>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    run: &mut AgentRun,
) -> ContractResult<()>
where
    S: RunJournalStore,
{
    for (index, next) in [
        AgentControllerState::Localize,
        AgentControllerState::Plan,
        AgentControllerState::Execute,
    ]
    .into_iter()
    .enumerate()
    {
        let expected = run.last_event_sequence();
        let event = run.transition(
            RunEventId::from_bytes([u8::try_from(210 + index)?; 32]),
            next,
            RunEventPayload::empty(),
            run.current_snapshot_id(),
            AgentRunTimestamp::from_unix_millis(13 + u64::try_from(index)?)?,
        )?;
        store
            .append_run_event(project, expected, run, &event)
            .await?;
    }
    Ok(())
}

async fn publish<S>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    snapshot_id: SnapshotId,
    run_id: [u8; 32],
    path: &[u8],
    hash: [u8; 32],
    symbol_byte: u8,
) -> ContractResult<()>
where
    S: KnowledgeIndexStore,
{
    let sequence = store.next_index_run_sequence(project).await?;
    let start = run_at(run_id, snapshot_id, 1, sequence.get())?;
    store.start_index_run(project, start).await?;
    store
        .publish_index(
            project,
            a3_domain::IndexRunId::from_bytes(run_id),
            &index::publication(snapshot_id, path, hash, symbol_byte)?,
            &RecoveryIndexControl,
        )
        .await?;
    Ok(())
}

fn goal_draft() -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("recover a durable agent run".to_owned())?,
        vec![AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([209; 32]),
            AcceptanceCriterionStatement::try_from_string(
                "resume only with fresh verification evidence".to_owned(),
            )?,
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("run the recovery contract".to_owned())?,
    )?)
}

fn task_step(step_id: TaskStepId) -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string("retain a verified source observation".to_owned())?,
        TaskStepRationale::try_from_string("exercise recovery freshness".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "current source revision".to_owned(),
        )?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([195; 32]),
            VerificationMethod::Diagnostic,
            VerificationRequirement::try_from_string(
                "the source revision remains current".to_owned(),
            )?,
        ),
    )?)
}
