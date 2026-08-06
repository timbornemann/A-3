use crate::fixture::{ContractWorkspace, project, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateAgentRun, CreateGoalContract, CreateTaskLedger, ExportRunJournal, KnowledgeIndexStore,
    RunEventPageLimit, RunJournalExportControl, RunJournalExportControlError,
    RunJournalExportSchemaVersion, RunJournalRetentionPolicy, RunJournalStore,
    RunJournalStoreFailure,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentControllerState,
    AgentRun, AgentRunId, AgentRunTimestamp, ExpectedTaskEvidence, GoalConstraint, GoalContract,
    GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelProfileId, ModelProfileReference,
    ModelProfileVersion, NonGoal, Progress, RepositoryId, RunEventCode, RunEventId, RunEventKind,
    RunEventOutcome, RunEventPayload, RunEventRedaction, RunEventRedactionSource, SnapshotId,
    SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, UserDecision, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId, WorktreeId,
};
use futures::join;
use std::sync::Mutex;

const SECRET_FIXTURE: &str = "a3-contract-secret-do-not-persist-7ed89a";

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("run-journals");
    let common = workspace.create_directory("run-journal-common")?;
    let first_root = workspace.create_directory("run-journal-first")?;
    let second_root = workspace.create_directory("run-journal-second")?;
    let repository_id = RepositoryId::from_bytes([150; 32]);
    let first = project(
        repository_id,
        WorktreeId::from_bytes([151; 32]),
        &common,
        &first_root,
        unborn_head()?,
    )?;
    let second = project(
        repository_id,
        WorktreeId::from_bytes([152; 32]),
        &common,
        &second_root,
        unborn_head()?,
    )?;
    let snapshot_id = SnapshotId::from_bytes([153; 32]);
    let current_snapshot = snapshot(
        *snapshot_id.as_bytes(),
        first.worktree().id(),
        None,
        1,
        Vec::new(),
    )?;
    let task_id = TaskId::from_bytes([154; 32]);
    let goal = GoalContract::initial(
        task_id,
        goal_draft()?,
        GoalContractTimestamp::from_unix_millis(2_000)?,
    );
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![task_step()?],
        TaskLedgerTimestamp::from_unix_millis(2_001)?,
    )?;
    let run_id = AgentRunId::from_bytes([155; 32]);
    let (initial_run, start_event) = AgentRun::start(
        run_id,
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([165; 32]),
            ModelProfileVersion::V1,
        ),
        snapshot_id,
        RunEventId::from_bytes([156; 32]),
        AgentRunTimestamp::from_unix_millis(2_002)?,
    )?;

    let first_writer = factory.open(&app_data_root).await?;
    first_writer
        .append_snapshot(&first, &current_snapshot)
        .await?;
    CreateGoalContract::new(&first_writer)
        .execute(&first, &goal)
        .await?;
    CreateTaskLedger::new(&first_writer)
        .execute(&first, &ledger)
        .await?;
    CreateAgentRun::new(&first_writer)
        .execute(&first, &initial_run, &start_event)
        .await?;
    assert_eq!(
        first_writer
            .create_agent_run(&first, &initial_run, &start_event)
            .await,
        Err(RunJournalStoreFailure::RunAlreadyExists)
    );
    assert_eq!(first_writer.load_agent_run(&second, run_id).await?, None);

    let second_writer = factory.open(&app_data_root).await?;
    let (first_candidate, first_event) = transition_candidate(
        &initial_run,
        [157; 32],
        AgentRunTimestamp::from_unix_millis(2_003)?,
    )?;
    let (second_candidate, second_event) = transition_candidate(
        &initial_run,
        [158; 32],
        AgentRunTimestamp::from_unix_millis(2_004)?,
    )?;
    let (first_result, second_result) = join!(
        first_writer.append_run_event(
            &first,
            initial_run.last_event_sequence(),
            &first_candidate,
            &first_event,
        ),
        second_writer.append_run_event(
            &first,
            initial_run.last_event_sequence(),
            &second_candidate,
            &second_event,
        )
    );
    assert_one_writer_won(first_result, second_result)?;

    let mut current = first_writer
        .load_agent_run(&first, run_id)
        .await?
        .ok_or_else(|| std::io::Error::other("materialized run disappeared"))?;
    assert_eq!(current.model_profile(), initial_run.model_profile());
    assert_eq!(current.state(), AgentControllerState::Localize);
    assert_eq!(current.last_event_sequence().get(), 2);
    let expected_sequence = current.last_event_sequence();
    let safe_payload = RunEventPayload::new(
        RunEventCode::InvalidModelOutput,
        Some(RunEventOutcome::Failed),
        Some(RunEventRedaction::new(
            RunEventRedactionSource::ModelOutput,
            u64::try_from(SECRET_FIXTURE.len())?,
            false,
        )),
    );
    let redacted_event = current.record(
        RunEventId::from_bytes([159; 32]),
        RunEventKind::ModelInteraction,
        safe_payload,
        snapshot_id,
        None,
        AgentRunTimestamp::from_unix_millis(2_005)?,
    )?;
    first_writer
        .append_run_event(&first, expected_sequence, &current, &redacted_event)
        .await?;

    let first_page = first_writer
        .load_run_events(&first, run_id, None, RunEventPageLimit::new(2)?)
        .await?;
    assert_eq!(first_page.events().len(), 2);
    assert!(first_page.has_more());
    let second_page = first_writer
        .load_run_events(
            &first,
            run_id,
            Some(first_page.events()[1].sequence()),
            RunEventPageLimit::new(2)?,
        )
        .await?;
    assert_eq!(second_page.events(), &[redacted_event]);
    assert!(!second_page.has_more());

    let control = RecordingExportControl::default();
    let first_export = ExportRunJournal::new(&first_writer)
        .execute(&first, run_id, &control)
        .await?;
    let second_export = ExportRunJournal::new(&first_writer)
        .execute(&first, run_id, &control)
        .await?;
    assert_eq!(first_export, second_export);
    assert_eq!(first_export.event_count(), 3);
    assert_eq!(
        first_export.schema_version(),
        RunJournalExportSchemaVersion::V1
    );
    assert_eq!(
        first_export.retention_policy(),
        RunJournalRetentionPolicy::PreserveAuditEvents
    );
    assert!(
        !first_export
            .bytes()
            .windows(SECRET_FIXTURE.len())
            .any(|window| window == SECRET_FIXTURE.as_bytes())
    );
    let text = std::str::from_utf8(first_export.bytes())?;
    assert!(text.contains("\"schema\":\"a3.run-journal.jsonl\""));
    assert!(text.contains("\"retention\":\"audit_events_preserved\""));
    assert!(text.contains("\"redaction_source\":\"model_output\""));
    assert_eq!(control.last_completed()?, Some(3));

    crate::release_contract_store(second_writer);
    crate::release_contract_store(first_writer);
    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened.load_agent_run(&first, run_id).await?,
        Some(current)
    );
    let all_events = reopened
        .load_run_events(&first, run_id, None, RunEventPageLimit::new(8)?)
        .await?;
    assert_eq!(all_events.events().len(), 3);
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

fn transition_candidate(
    initial: &AgentRun,
    event_id: [u8; 32],
    timestamp: AgentRunTimestamp,
) -> ContractResult<(AgentRun, a3_domain::RunEvent)> {
    let mut candidate = initial.clone();
    let event = candidate.transition(
        RunEventId::from_bytes(event_id),
        AgentControllerState::Localize,
        RunEventPayload::new(
            RunEventCode::ControllerDecision,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        initial.current_snapshot_id(),
        timestamp,
    )?;
    Ok((candidate, event))
}

fn assert_one_writer_won(
    first: Result<(), RunJournalStoreFailure>,
    second: Result<(), RunJournalStoreFailure>,
) -> ContractResult<()> {
    assert!(matches!(
        (first, second),
        (Ok(()), Err(RunJournalStoreFailure::SequenceConflict))
            | (Err(RunJournalStoreFailure::SequenceConflict), Ok(()))
    ));
    Ok(())
}

fn goal_draft() -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("persist a safe append-only run journal".to_owned())?,
        vec![AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([160; 32]),
            AcceptanceCriterionStatement::try_from_string(
                "restart restores exact current run state".to_owned(),
            )?,
        )],
        vec![GoalConstraint::try_from_string(
            "never persist raw model output".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not execute journaled tool actions".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "retain immutable audit events".to_owned(),
        )?],
        SuccessVerification::try_from_string("run the shared journal contract".to_owned())?,
    )?)
}

fn task_step() -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([161; 32]),
        None,
        TaskStepOutcome::try_from_string("persist one run".to_owned())?,
        TaskStepRationale::try_from_string("exercise the journal boundary".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "the shared contract passes".to_owned(),
        )?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([162; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string("the shared contract passes".to_owned())?,
        ),
    )?)
}

#[derive(Debug, Default)]
struct RecordingExportControl {
    progress: Mutex<Vec<Progress>>,
}

impl RecordingExportControl {
    fn last_completed(&self) -> ContractResult<Option<u64>> {
        let progress = self
            .progress
            .lock()
            .map_err(|_| std::io::Error::other("export progress lock was poisoned"))?;
        Ok(progress.last().and_then(|value| value.completed()))
    }
}

impl RunJournalExportControl for RecordingExportControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RunJournalExportControlError> {
        self.progress
            .lock()
            .map_err(|_| RunJournalExportControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}
