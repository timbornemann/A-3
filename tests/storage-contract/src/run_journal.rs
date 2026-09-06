use crate::fixture::{ContractWorkspace, project, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    AgentActionStore, AgentActionStoreFailure, AgentReadResult, AgentRecoveryStore,
    ContextToolResultDigest, ContextToolResultPreview, ContextToolResultStatus, CreateAgentRun,
    CreateGoalContract, CreateTaskLedger, ExportRunJournal, KnowledgeIndexStore, RunEventPageLimit,
    RunJournalExportControl, RunJournalExportControlError, RunJournalExportSchemaVersion,
    RunJournalRetentionPolicy, RunJournalStore, RunJournalStoreFailure, TaskLedgerStore,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentActionLimit,
    AgentControllerState, AgentRepairLimit, AgentRun, AgentRunBudget, AgentRunDurationLimit,
    AgentRunId, AgentRunTimestamp, AgentTokenLimit, AgentToolEvidence, AgentToolEvidenceSet,
    AgentTurnActionClass, AgentTurnCharge, AgentTurnLimit, AgentTurnRepairUsage, ContentHash,
    EvidenceRef, ExpectedTaskEvidence, FileRevision, GoalConstraint, GoalContract,
    GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelProfileId, ModelProfileReference,
    ModelProfileVersion, ModelTokenCount, NonGoal, Progress, RepositoryId, RepositoryPath,
    RunEventCode, RunEventId, RunEventOutcome, RunEventPayload, RunEventRedaction,
    RunEventRedactionSource, SnapshotId, SourcePosition, SourceRange, SuccessVerification, TaskId,
    TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome,
    TaskStepRationale, TaskStepStatus, ToolRunId, UserDecision, VerificationMethod,
    VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeId,
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
    let budget = AgentRunBudget::new(
        AgentTurnLimit::new(17)?,
        AgentTokenLimit::new(500_000)?,
        AgentTokenLimit::new(50_000)?,
        AgentActionLimit::new(13)?,
        AgentRunDurationLimit::from_millis(123_456)?,
        AgentRepairLimit::new(2)?,
    );
    let (initial_run, start_event) = AgentRun::start_with_budget(
        run_id,
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([165; 32]),
            ModelProfileVersion::V1,
        ),
        budget,
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
    assert_eq!(current.budget(), budget);
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
    let turn_charge = AgentTurnCharge::new(
        ModelTokenCount::new(1_024),
        ModelTokenCount::new(64),
        Some(AgentTurnActionClass::ApplyPatch),
        AgentTurnRepairUsage::One,
    );
    let redacted_event = current.record_turn(
        RunEventId::from_bytes([159; 32]),
        safe_payload,
        snapshot_id,
        AgentRunTimestamp::from_unix_millis(2_005)?,
        turn_charge,
    )?;
    first_writer
        .append_run_event(&first, expected_sequence, &current, &redacted_event)
        .await?;
    let expected_sequence = current.last_event_sequence();
    let tool_run_id = ToolRunId::from_bytes([166; 32]);
    first_writer
        .begin_agent_tool_attempt(
            &first,
            run_id,
            snapshot_id,
            tool_run_id,
            AgentRunTimestamp::from_unix_millis(2_005)?,
        )
        .await?;
    let evidence = AgentToolEvidence::for_span(EvidenceRef::new(
        FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([167; 32]),
        ),
        SourceRange::new(0, 8, SourcePosition::new(0, 0), SourcePosition::new(0, 8))?,
    ));
    let unmarked_evidence = AgentToolEvidence::for_span(EvidenceRef::new(
        FileRevision::new(
            evidence.location().revision().path().clone(),
            ContentHash::from_bytes([185; 32]),
        ),
        evidence.location().range().ok_or("span")?,
    ));
    let read = AgentReadResult::new(
        tool_run_id,
        ContextToolResultStatus::Succeeded,
        ContextToolResultPreview::try_from_string(SECRET_FIXTURE.to_owned())?,
        ContextToolResultDigest::from_bytes([169; 32]),
        false,
        snapshot_id,
        AgentToolEvidenceSet::new(
            snapshot_id,
            vec![evidence.clone(), unmarked_evidence.clone()],
        )?,
        u64::try_from(SECRET_FIXTURE.len())?,
    )?
    .with_original_page(a3_application::AgentSourcePage::new(
        evidence.location().revision().clone(),
        evidence.location().range().ok_or("span")?,
        a3_domain::AgentFileStartLine::new(1)?,
        "original".to_owned(),
        None,
        false,
    )?)?
    .record(
        &mut current,
        RunEventId::from_bytes([168; 32]),
        AgentRunTimestamp::from_unix_millis(2_006)?,
    )?;
    let mut checkpoint = a3_application::ReplanResearchCheckpoint::new(
        TaskStepId::from_bytes([161; 32]),
        snapshot_id,
        &a3_domain::TaskReplanReason::try_from_string("Locate the serializer".to_owned())?,
        "preserve serialized value",
    )?;
    checkpoint.record_read(
        &a3_domain::AgentAction::Inspect(a3_domain::AgentInspectAction::new(
            a3_domain::AgentInspectTarget::File(a3_domain::AgentFileInspection::new(
                evidence.location().revision().path().clone(),
                a3_domain::AgentFileStartLine::new(1)?,
                a3_domain::AgentFileLineCount::new(1)?,
            )),
        )),
        true,
    )?;
    let read = read.with_replan(checkpoint.clone());
    first_writer
        .append_agent_read(&first, expected_sequence, &current, &read)
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
    assert_eq!(redacted_event.turn_charge(), Some(turn_charge));
    assert_eq!(
        second_page.events(),
        &[redacted_event, read.event().clone()]
    );
    assert_eq!(current.usage().turn_count(), 1);
    assert_eq!(current.usage().action_count(), 1);
    assert_eq!(current.usage().repair_count(), 1);
    assert!(!second_page.has_more());

    let control = RecordingExportControl::default();
    let first_export = ExportRunJournal::new(&first_writer)
        .execute(&first, run_id, &control)
        .await?;
    let second_export = ExportRunJournal::new(&first_writer)
        .execute(&first, run_id, &control)
        .await?;
    assert_eq!(first_export, second_export);
    assert_eq!(first_export.event_count(), 4);
    assert_eq!(
        first_export.schema_version(),
        RunJournalExportSchemaVersion::V2
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
    assert!(text.contains("\"turn_count\":1"));
    assert!(text.contains("\"turn_limit\":17"));
    assert!(text.contains("\"turn_action_kind\":\"apply_patch\""));
    assert!(text.contains("\"turn_repair_used\":true"));
    assert_eq!(control.last_completed()?, Some(4));

    let mut committed_ledger = ledger.clone();
    committed_ledger.start_step(
        TaskStepId::from_bytes([161; 32]),
        run_id,
        TaskLedgerTimestamp::from_unix_millis(2_007)?,
    )?;
    let expected_sequence = current.last_event_sequence();
    let committed_event = current.transition(
        RunEventId::from_bytes([170; 32]),
        AgentControllerState::Plan,
        RunEventPayload::empty(),
        snapshot_id,
        AgentRunTimestamp::from_unix_millis(2_008)?,
    )?;
    let next_ledger_version = first_writer
        .commit_ledger_action(
            &first,
            TaskLedgerStoreVersion::INITIAL,
            expected_sequence,
            &committed_ledger,
            &current,
            &committed_event,
        )
        .await?;
    assert_eq!(next_ledger_version.get(), 2);

    let mut losing_run = current.clone();
    let losing_event = losing_run.transition(
        RunEventId::from_bytes([171; 32]),
        AgentControllerState::Execute,
        RunEventPayload::empty(),
        snapshot_id,
        AgentRunTimestamp::from_unix_millis(2_009)?,
    )?;
    assert_eq!(
        first_writer
            .commit_ledger_action(
                &first,
                TaskLedgerStoreVersion::INITIAL,
                current.last_event_sequence(),
                &committed_ledger,
                &losing_run,
                &losing_event,
            )
            .await,
        Err(AgentActionStoreFailure::LedgerVersionConflict)
    );
    assert_eq!(
        first_writer.load_agent_run(&first, run_id).await?,
        Some(current.clone())
    );

    crate::release_contract_store(second_writer);
    crate::release_contract_store(first_writer);
    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened.load_agent_run(&first, run_id).await?,
        Some(current.clone())
    );
    let reopened_ledger = reopened
        .load_task_ledger(&first, task_id)
        .await?
        .ok_or_else(|| std::io::Error::other("atomically committed Ledger disappeared"))?;
    assert_eq!(reopened_ledger.version(), next_ledger_version);
    assert_eq!(
        reopened_ledger
            .ledger()
            .step(TaskStepId::from_bytes([161; 32]))
            .map(|step| step.status()),
        Some(TaskStepStatus::InProgress)
    );
    let all_events = reopened
        .load_run_events(&first, run_id, None, RunEventPageLimit::new(8)?)
        .await?;
    assert_eq!(all_events.events().len(), 5);
    assert_eq!(
        reopened
            .load_replan_originals(&first, run_id, checkpoint.step_id, snapshot_id)
            .await?,
        vec![evidence.clone()]
    );
    assert!(
        reopened
            .load_replan_originals(
                &first,
                run_id,
                TaskStepId::from_bytes([99; 32]),
                snapshot_id
            )
            .await?
            .is_empty()
    );
    assert!(
        reopened
            .load_replan_originals(
                &first,
                run_id,
                checkpoint.step_id,
                SnapshotId::from_bytes([99; 32])
            )
            .await?
            .is_empty()
    );
    assert!(
        reopened
            .load_replan_originals(&second, run_id, checkpoint.step_id, snapshot_id)
            .await
            .is_err()
    );
    assert_eq!(
        reopened
            .load_replan_research(&first, run_id, checkpoint.step_id)
            .await?,
        Some(checkpoint.clone())
    );
    assert!(
        reopened
            .load_replan_research(&second, run_id, checkpoint.step_id)
            .await
            .is_err()
    );
    // Unsupported evidence must roll back both the charged event and checkpoint. A valid
    // original span then commits both; none of this verifies the implementation step.
    for case in 0..3 {
        let valid = case == 2;
        let mut next = checkpoint.clone();
        next.work.begin_analysis(
            a3_domain::ResearchQuestionId::FIRST,
            ContentHash::from_bytes([180; 32]),
        )?;
        let revision = if valid {
            evidence.location().revision().clone()
        } else if case == 1 {
            // A persisted search/inspection span without an actual original-page
            // marker is still not admissible Replan research evidence.
            unmarked_evidence.location().revision().clone()
        } else {
            FileRevision::new(
                evidence.location().revision().path().clone(),
                ContentHash::from_bytes([181; 32]),
            )
        };
        next.work.resolve(a3_domain::ResearchQuestionId::FIRST, a3_domain::ResearchResult::new(a3_domain::ResearchResultKind::Interpretation,
            "The current serializer needs the missing field preserved and a round-trip regression.".to_owned(),
            vec![a3_domain::ResearchResultSource { source_id:a3_domain::AskResearchSourceId::from_bytes([182;32]),revision,range:evidence.location().range().ok_or("span")? }],None)?)?;
        let mut candidate = current.clone();
        let event = candidate.record_turn(
            RunEventId::from_bytes([183; 32]),
            RunEventPayload::empty(),
            snapshot_id,
            AgentRunTimestamp::from_unix_millis(2_010)?,
            AgentTurnCharge::new(
                ModelTokenCount::new(100),
                ModelTokenCount::new(30),
                None,
                AgentTurnRepairUsage::None,
            ),
        )?;
        let result = reopened
            .append_replan_research(
                &first,
                current.last_event_sequence(),
                &candidate,
                &event,
                &next,
            )
            .await;
        if valid {
            result?;
            assert_eq!(
                reopened
                    .load_replan_research(&first, run_id, next.step_id)
                    .await?,
                Some(next)
            );
            assert_eq!(
                reopened.load_agent_run(&first, run_id).await?,
                Some(candidate)
            );
        } else {
            assert!(result.is_err());
            assert_eq!(
                reopened.load_agent_run(&first, run_id).await?,
                Some(current.clone())
            );
            assert_eq!(
                reopened
                    .load_replan_research(&first, run_id, next.step_id)
                    .await?,
                Some(checkpoint.clone())
            );
        }
    }
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
