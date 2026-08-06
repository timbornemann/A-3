use crate::{catalog::is_corruption, goal_contract_repository, task_ledger_repository};
use a3_application::{RunEventPage, RunEventPageLimit, RunJournalStoreFailure};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunIdentity, AgentRunMaterializedState,
    AgentRunTimestamp, AgentRunTiming, GoalContractRevision, RunEvent, RunEventCode, RunEventId,
    RunEventIdentity, RunEventKind, RunEventOccurrence, RunEventOutcome, RunEventPayload,
    RunEventRedaction, RunEventRedactionSource, RunEventSequence, RunEventSubject,
    RunPayloadDigest, SnapshotId, TaskEvidenceId, TaskLedgerRevision, ToolRunId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn create(
    connection: &Connection,
    worktree_id: WorktreeId,
    run: &AgentRun,
    start_event: &RunEvent,
) -> Result<(), RunJournalRepositoryError> {
    validate_start_pair(run, start_event)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(RunJournalRepositoryError::Begin)?;
    let result = async {
        if read_run_header(&transaction, worktree_id, run.id())
            .await?
            .is_some()
        {
            return Err(RunJournalRepositoryError::RunAlreadyExists);
        }
        validate_run_anchors(&transaction, worktree_id, run, true).await?;
        transaction
            .execute(
                "INSERT INTO agent_runs (
                 run_id, task_id, goal_revision, task_ledger_revision, controller_state,
                 last_event_sequence, current_snapshot_id, created_at_unix_millis,
                 updated_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id_bytes(run.id()),
                    id_bytes(run.goal_contract().task_id()),
                    i64::from(run.goal_contract().revision().get()),
                    i64::from(run.task_ledger_revision().get()),
                    controller_state_text(run.state()),
                    sequence_to_i64(run.last_event_sequence())?,
                    id_bytes(run.current_snapshot_id()),
                    timestamp_to_i64(run.created_at()),
                    timestamp_to_i64(run.updated_at())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        write_event(&transaction, start_event).await
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn append(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
) -> Result<(), RunJournalRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(RunJournalRepositoryError::Begin)?;
    let result = async {
        let existing = load_run_from_transaction(&transaction, worktree_id, run.id())
            .await?
            .ok_or(RunJournalRepositoryError::RunNotFound)?;
        if existing.last_event_sequence() != expected_last_sequence {
            return Err(RunJournalRepositoryError::SequenceConflict);
        }
        let tail = read_event(&transaction, run.id(), expected_last_sequence)
            .await?
            .ok_or(RunJournalRepositoryError::InvalidStoredData)?;
        validate_materialized_tail(&existing, &tail)?;
        let mut expected_run = existing;
        expected_run
            .apply_event(event)
            .map_err(|_| RunJournalRepositoryError::InvalidInput)?;
        if &expected_run != run {
            return Err(RunJournalRepositoryError::InvalidInput);
        }
        validate_run_anchors(&transaction, worktree_id, run, true).await?;
        write_event(&transaction, event).await?;
        let changed = transaction
            .execute(
                "UPDATE agent_runs SET task_ledger_revision = ?1, controller_state = ?2,
                 last_event_sequence = ?3, current_snapshot_id = ?4,
                 updated_at_unix_millis = ?5
                 WHERE run_id = ?6 AND last_event_sequence = ?7",
                params![
                    i64::from(run.task_ledger_revision().get()),
                    controller_state_text(run.state()),
                    sequence_to_i64(run.last_event_sequence())?,
                    id_bytes(run.current_snapshot_id()),
                    timestamp_to_i64(run.updated_at()),
                    id_bytes(run.id()),
                    sequence_to_i64(expected_last_sequence)?
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        if changed != 1 {
            return Err(RunJournalRepositoryError::SequenceConflict);
        }
        Ok(())
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn load_run(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<Option<AgentRun>, RunJournalRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(RunJournalRepositoryError::Begin)?;
    let result = load_run_from_transaction(&transaction, worktree_id, run_id).await;
    close_read_transaction(transaction, result).await
}

pub(crate) async fn load_events(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    after_sequence: Option<RunEventSequence>,
    limit: RunEventPageLimit,
) -> Result<RunEventPage, RunJournalRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(RunJournalRepositoryError::Begin)?;
    let result = async {
        if read_run_header(&transaction, worktree_id, run_id)
            .await?
            .is_none()
        {
            return Err(RunJournalRepositoryError::RunNotFound);
        }
        let after = after_sequence.map_or(0, RunEventSequence::get);
        let query_limit = u64::from(limit.get()) + 1;
        let mut rows = transaction
            .query(
                "SELECT event_sequence, event_id, occurred_at_unix_millis, event_kind,
                 state_from, state_to, ledger_revision_from, ledger_revision_to,
                 payload_schema_version, payload_code, payload_outcome, redaction_source,
                 redaction_observed_bytes, redaction_source_truncated, payload_digest,
                 snapshot_id, subject_kind, subject_id
                 FROM run_events WHERE run_id = ?1 AND event_sequence > ?2
                 ORDER BY event_sequence LIMIT ?3",
                params![
                    id_bytes(run_id),
                    u64_to_i64(after)?,
                    u64_to_i64(query_limit)?
                ],
            )
            .await
            .map_err(RunJournalRepositoryError::Read)?;
        let mut events = Vec::new();
        while let Some(row) = rows.next().await.map_err(RunJournalRepositoryError::Read)? {
            events.push(read_event_row(&row, run_id)?);
        }
        let has_more = events.len() > usize::from(limit.get());
        if has_more {
            events.pop();
        }
        RunEventPage::new(after_sequence, limit, events, has_more)
            .map_err(RunJournalRepositoryError::InvalidPage)
    }
    .await;
    close_read_transaction(transaction, result).await
}

async fn load_run_from_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<Option<AgentRun>, RunJournalRepositoryError> {
    let Some(header) = read_run_header(transaction, worktree_id, run_id).await? else {
        return Ok(None);
    };
    let goal = goal_contract_repository::load_revision_from_transaction(
        transaction,
        worktree_id,
        header.task_id,
        header.goal_revision,
    )
    .await
    .map_err(RunJournalRepositoryError::GoalContract)?
    .ok_or(RunJournalRepositoryError::InvalidStoredData)?;
    let run = AgentRun::reconstruct(
        AgentRunIdentity::new(run_id, goal.reference(), header.task_ledger_revision),
        AgentRunMaterializedState::new(
            header.state,
            header.last_event_sequence,
            header.current_snapshot_id,
        ),
        AgentRunTiming::new(header.created_at, header.updated_at),
    )
    .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    validate_run_anchors(transaction, worktree_id, &run, false).await?;
    Ok(Some(run))
}

#[derive(Debug, Clone, Copy)]
struct RunHeader {
    task_id: a3_domain::TaskId,
    goal_revision: GoalContractRevision,
    task_ledger_revision: TaskLedgerRevision,
    state: AgentControllerState,
    last_event_sequence: RunEventSequence,
    current_snapshot_id: SnapshotId,
    created_at: AgentRunTimestamp,
    updated_at: AgentRunTimestamp,
}

async fn read_run_header(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<Option<RunHeader>, RunJournalRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT r.task_id, r.goal_revision, r.task_ledger_revision, r.controller_state,
             r.last_event_sequence, r.current_snapshot_id, r.created_at_unix_millis,
             r.updated_at_unix_millis
             FROM agent_runs r JOIN tasks t ON t.task_id = r.task_id
             WHERE r.run_id = ?1 AND t.worktree_id = ?2",
            params![id_bytes(run_id), id_bytes(worktree_id)],
        )
        .await
        .map_err(RunJournalRepositoryError::Read)?;
    let header = rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .map(|row| {
            Ok(RunHeader {
                task_id: a3_domain::TaskId::from_bytes(read_id(&row, 0)?),
                goal_revision: read_goal_revision(&row, 1)?,
                task_ledger_revision: read_ledger_revision(&row, 2)?,
                state: parse_controller_state(&read_text(&row, 3)?)?,
                last_event_sequence: read_sequence(&row, 4)?,
                current_snapshot_id: SnapshotId::from_bytes(read_id(&row, 5)?),
                created_at: read_timestamp(&row, 6)?,
                updated_at: read_timestamp(&row, 7)?,
            })
        })
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .is_some()
    {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    Ok(header)
}

async fn validate_run_anchors(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run: &AgentRun,
    require_current_ledger_revision: bool,
) -> Result<(), RunJournalRepositoryError> {
    let stored_ledger = task_ledger_repository::load_from_transaction(
        transaction,
        worktree_id,
        run.goal_contract().task_id(),
    )
    .await
    .map_err(RunJournalRepositoryError::TaskLedger)?
    .ok_or(RunJournalRepositoryError::RunNotFound)?;
    let ledger = stored_ledger.ledger();
    if ledger.goal_contract() != run.goal_contract()
        || run.task_ledger_revision().get() > ledger.revision().get()
        || require_current_ledger_revision && run.task_ledger_revision() != ledger.revision()
        || run.task_ledger_revision() != TaskLedgerRevision::INITIAL
            && !ledger
                .replans()
                .iter()
                .any(|replan| replan.revision() == run.task_ledger_revision())
    {
        return Err(RunJournalRepositoryError::InvalidInput);
    }
    ensure_snapshot_exists(transaction, worktree_id, run.current_snapshot_id()).await
}

async fn ensure_snapshot_exists(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
) -> Result<(), RunJournalRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT 1 FROM snapshots WHERE snapshot_id = ?1 AND worktree_id = ?2",
            params![id_bytes(snapshot_id), id_bytes(worktree_id)],
        )
        .await
        .map_err(RunJournalRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .is_none()
    {
        return Err(RunJournalRepositoryError::RunNotFound);
    }
    if rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .is_some()
    {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    Ok(())
}

fn validate_start_pair(run: &AgentRun, event: &RunEvent) -> Result<(), RunJournalRepositoryError> {
    if run.state() != AgentControllerState::Intake
        || run.last_event_sequence() != RunEventSequence::FIRST
        || run.created_at() != run.updated_at()
        || event.run_id() != run.id()
        || event.sequence() != RunEventSequence::FIRST
        || event.kind() != RunEventKind::RunStarted
        || event.occurred_at() != run.created_at()
        || event.snapshot_id() != run.current_snapshot_id()
        || event.subject().is_some()
        || event.payload() != &RunEventPayload::empty()
    {
        return Err(RunJournalRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_materialized_tail(
    run: &AgentRun,
    event: &RunEvent,
) -> Result<(), RunJournalRepositoryError> {
    if event.run_id() != run.id()
        || event.sequence() != run.last_event_sequence()
        || event.occurred_at() != run.updated_at()
        || event.snapshot_id() != run.current_snapshot_id()
    {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    match event.kind() {
        RunEventKind::RunStarted
            if run.state() != AgentControllerState::Intake
                || run.last_event_sequence() != RunEventSequence::FIRST =>
        {
            Err(RunJournalRepositoryError::InvalidStoredData)
        }
        RunEventKind::StateTransition { to, .. } if to != run.state() => {
            Err(RunJournalRepositoryError::InvalidStoredData)
        }
        RunEventKind::LedgerUpdated { to, .. } if to != run.task_ledger_revision() => {
            Err(RunJournalRepositoryError::InvalidStoredData)
        }
        _ => Ok(()),
    }
}

async fn write_event(
    transaction: &Transaction,
    event: &RunEvent,
) -> Result<(), RunJournalRepositoryError> {
    let shape = EventShape::from_kind(event.kind());
    let (redaction_source, redaction_bytes, redaction_truncated) = match event.payload().redaction()
    {
        Some(redaction) => (
            Some(redaction_source_text(redaction.source())),
            Some(u64_to_i64(redaction.observed_bytes())),
            Some(i64::from(redaction.source_was_truncated())),
        ),
        None => (None, None, None),
    };
    let redaction_bytes = redaction_bytes.transpose()?;
    let (subject_kind, subject_id) = match event.subject() {
        Some(RunEventSubject::Tool(id)) => (Some("tool"), Some(id_bytes(id))),
        Some(RunEventSubject::Evidence(id)) => (Some("evidence"), Some(id_bytes(id))),
        None => (None, None),
    };
    transaction
        .execute(
            "INSERT INTO run_events (
             run_id, event_sequence, event_id, occurred_at_unix_millis, event_kind,
             state_from, state_to, ledger_revision_from, ledger_revision_to,
             payload_schema_version, payload_code, payload_outcome, redaction_source,
             redaction_observed_bytes, redaction_source_truncated, payload_digest,
             snapshot_id, subject_kind, subject_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12,
             ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                id_bytes(event.run_id()),
                sequence_to_i64(event.sequence())?,
                id_bytes(event.id()),
                timestamp_to_i64(event.occurred_at()),
                shape.kind,
                shape.state_from.map(controller_state_text),
                shape.state_to.map(controller_state_text),
                shape.ledger_from.map(|value| i64::from(value.get())),
                shape.ledger_to.map(|value| i64::from(value.get())),
                event_code_text(event.payload().code()),
                event.payload().outcome().map(event_outcome_text),
                redaction_source,
                redaction_bytes,
                redaction_truncated,
                event.payload().digest().as_bytes().to_vec(),
                id_bytes(event.snapshot_id()),
                subject_kind,
                subject_id
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    Ok(())
}

async fn read_event(
    transaction: &Transaction,
    run_id: AgentRunId,
    sequence: RunEventSequence,
) -> Result<Option<RunEvent>, RunJournalRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT event_sequence, event_id, occurred_at_unix_millis, event_kind,
             state_from, state_to, ledger_revision_from, ledger_revision_to,
             payload_schema_version, payload_code, payload_outcome, redaction_source,
             redaction_observed_bytes, redaction_source_truncated, payload_digest,
             snapshot_id, subject_kind, subject_id
             FROM run_events WHERE run_id = ?1 AND event_sequence = ?2",
            params![id_bytes(run_id), sequence_to_i64(sequence)?],
        )
        .await
        .map_err(RunJournalRepositoryError::Read)?;
    let event = rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .map(|row| read_event_row(&row, run_id))
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .is_some()
    {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    Ok(event)
}

fn read_event_row(
    row: &libsql::Row,
    run_id: AgentRunId,
) -> Result<RunEvent, RunJournalRepositoryError> {
    let sequence = read_sequence(row, 0)?;
    let event_id = RunEventId::from_bytes(read_id(row, 1)?);
    let occurred_at = read_timestamp(row, 2)?;
    let kind_text = read_text(row, 3)?;
    let state_from = read_optional_text(row, 4)?
        .map(|value| parse_controller_state(&value))
        .transpose()?;
    let state_to = read_optional_text(row, 5)?
        .map(|value| parse_controller_state(&value))
        .transpose()?;
    let ledger_from = read_optional_ledger_revision(row, 6)?;
    let ledger_to = read_optional_ledger_revision(row, 7)?;
    if read_i64(row, 8)? != 1 {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    let code = parse_event_code(&read_text(row, 9)?)?;
    let outcome = read_optional_text(row, 10)?
        .map(|value| parse_event_outcome(&value))
        .transpose()?;
    let redaction_source = read_optional_text(row, 11)?
        .map(|value| parse_redaction_source(&value))
        .transpose()?;
    let redaction_bytes = read_optional_u64(row, 12)?;
    let redaction_truncated = read_optional_bool(row, 13)?;
    let redaction = match (redaction_source, redaction_bytes, redaction_truncated) {
        (Some(source), Some(bytes), Some(truncated)) => {
            Some(RunEventRedaction::new(source, bytes, truncated))
        }
        (None, None, None) => None,
        _ => return Err(RunJournalRepositoryError::InvalidStoredData),
    };
    let digest = RunPayloadDigest::from_bytes(read_id(row, 14)?);
    let payload = RunEventPayload::reconstruct(code, outcome, redaction, digest)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    let snapshot_id = SnapshotId::from_bytes(read_id(row, 15)?);
    let subject_kind = read_optional_text(row, 16)?;
    let subject_id = read_optional_id(row, 17)?;
    let subject = match (subject_kind.as_deref(), subject_id) {
        (Some("tool"), Some(bytes)) => Some(RunEventSubject::Tool(ToolRunId::from_bytes(bytes))),
        (Some("evidence"), Some(bytes)) => {
            Some(RunEventSubject::Evidence(TaskEvidenceId::from_bytes(bytes)))
        }
        (None, None) => None,
        _ => return Err(RunJournalRepositoryError::InvalidStoredData),
    };
    let kind = parse_event_kind(&kind_text, state_from, state_to, ledger_from, ledger_to)?;
    RunEvent::reconstruct(
        RunEventIdentity::new(event_id, run_id, sequence),
        RunEventOccurrence::new(occurred_at, snapshot_id, subject),
        kind,
        payload,
    )
    .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

struct EventShape {
    kind: &'static str,
    state_from: Option<AgentControllerState>,
    state_to: Option<AgentControllerState>,
    ledger_from: Option<TaskLedgerRevision>,
    ledger_to: Option<TaskLedgerRevision>,
}

impl EventShape {
    const fn from_kind(kind: RunEventKind) -> Self {
        match kind {
            RunEventKind::RunStarted => Self::plain("run_started"),
            RunEventKind::StateTransition { from, to } => Self {
                kind: "state_transition",
                state_from: Some(from),
                state_to: Some(to),
                ledger_from: None,
                ledger_to: None,
            },
            RunEventKind::ContextCompiled => Self::plain("context_compiled"),
            RunEventKind::ModelInteraction => Self::plain("model_interaction"),
            RunEventKind::ToolAction => Self::plain("tool_action"),
            RunEventKind::LedgerUpdated { from, to } => Self {
                kind: "ledger_updated",
                state_from: None,
                state_to: None,
                ledger_from: Some(from),
                ledger_to: Some(to),
            },
            RunEventKind::VerificationRecorded => Self::plain("verification_recorded"),
            RunEventKind::ApprovalRecorded => Self::plain("approval_recorded"),
            RunEventKind::Diagnostic => Self::plain("diagnostic"),
        }
    }

    const fn plain(kind: &'static str) -> Self {
        Self {
            kind,
            state_from: None,
            state_to: None,
            ledger_from: None,
            ledger_to: None,
        }
    }
}

fn parse_event_kind(
    kind: &str,
    state_from: Option<AgentControllerState>,
    state_to: Option<AgentControllerState>,
    ledger_from: Option<TaskLedgerRevision>,
    ledger_to: Option<TaskLedgerRevision>,
) -> Result<RunEventKind, RunJournalRepositoryError> {
    match (kind, state_from, state_to, ledger_from, ledger_to) {
        ("run_started", None, None, None, None) => Ok(RunEventKind::RunStarted),
        ("state_transition", Some(from), Some(to), None, None) => {
            Ok(RunEventKind::StateTransition { from, to })
        }
        ("context_compiled", None, None, None, None) => Ok(RunEventKind::ContextCompiled),
        ("model_interaction", None, None, None, None) => Ok(RunEventKind::ModelInteraction),
        ("tool_action", None, None, None, None) => Ok(RunEventKind::ToolAction),
        ("ledger_updated", None, None, Some(from), Some(to)) => {
            Ok(RunEventKind::LedgerUpdated { from, to })
        }
        ("verification_recorded", None, None, None, None) => Ok(RunEventKind::VerificationRecorded),
        ("approval_recorded", None, None, None, None) => Ok(RunEventKind::ApprovalRecorded),
        ("diagnostic", None, None, None, None) => Ok(RunEventKind::Diagnostic),
        _ => Err(RunJournalRepositoryError::InvalidStoredData),
    }
}

fn controller_state_text(state: AgentControllerState) -> &'static str {
    match state {
        AgentControllerState::Intake => "intake",
        AgentControllerState::Localize => "localize",
        AgentControllerState::Plan => "plan",
        AgentControllerState::Execute => "execute",
        AgentControllerState::Verify => "verify",
        AgentControllerState::Replan => "replan",
        AgentControllerState::AwaitApproval => "await_approval",
        AgentControllerState::Done => "done",
        AgentControllerState::Failed => "failed",
        AgentControllerState::Cancelled => "cancelled",
    }
}

fn parse_controller_state(value: &str) -> Result<AgentControllerState, RunJournalRepositoryError> {
    match value {
        "intake" => Ok(AgentControllerState::Intake),
        "localize" => Ok(AgentControllerState::Localize),
        "plan" => Ok(AgentControllerState::Plan),
        "execute" => Ok(AgentControllerState::Execute),
        "verify" => Ok(AgentControllerState::Verify),
        "replan" => Ok(AgentControllerState::Replan),
        "await_approval" => Ok(AgentControllerState::AwaitApproval),
        "done" => Ok(AgentControllerState::Done),
        "failed" => Ok(AgentControllerState::Failed),
        "cancelled" => Ok(AgentControllerState::Cancelled),
        _ => Err(RunJournalRepositoryError::InvalidStoredData),
    }
}

fn event_code_text(code: RunEventCode) -> &'static str {
    match code {
        RunEventCode::None => "none",
        RunEventCode::UserRequest => "user_request",
        RunEventCode::ControllerDecision => "controller_decision",
        RunEventCode::PolicyDecision => "policy_decision",
        RunEventCode::Timeout => "timeout",
        RunEventCode::Cancellation => "cancellation",
        RunEventCode::InvalidModelOutput => "invalid_model_output",
        RunEventCode::ToolFailure => "tool_failure",
        RunEventCode::VerificationFailure => "verification_failure",
        RunEventCode::StateRecovered => "state_recovered",
    }
}

fn parse_event_code(value: &str) -> Result<RunEventCode, RunJournalRepositoryError> {
    match value {
        "none" => Ok(RunEventCode::None),
        "user_request" => Ok(RunEventCode::UserRequest),
        "controller_decision" => Ok(RunEventCode::ControllerDecision),
        "policy_decision" => Ok(RunEventCode::PolicyDecision),
        "timeout" => Ok(RunEventCode::Timeout),
        "cancellation" => Ok(RunEventCode::Cancellation),
        "invalid_model_output" => Ok(RunEventCode::InvalidModelOutput),
        "tool_failure" => Ok(RunEventCode::ToolFailure),
        "verification_failure" => Ok(RunEventCode::VerificationFailure),
        "state_recovered" => Ok(RunEventCode::StateRecovered),
        _ => Err(RunJournalRepositoryError::InvalidStoredData),
    }
}

fn event_outcome_text(outcome: RunEventOutcome) -> &'static str {
    match outcome {
        RunEventOutcome::Succeeded => "succeeded",
        RunEventOutcome::Failed => "failed",
        RunEventOutcome::Cancelled => "cancelled",
        RunEventOutcome::Denied => "denied",
    }
}

fn parse_event_outcome(value: &str) -> Result<RunEventOutcome, RunJournalRepositoryError> {
    match value {
        "succeeded" => Ok(RunEventOutcome::Succeeded),
        "failed" => Ok(RunEventOutcome::Failed),
        "cancelled" => Ok(RunEventOutcome::Cancelled),
        "denied" => Ok(RunEventOutcome::Denied),
        _ => Err(RunJournalRepositoryError::InvalidStoredData),
    }
}

fn redaction_source_text(source: RunEventRedactionSource) -> &'static str {
    match source {
        RunEventRedactionSource::UntrustedText => "untrusted_text",
        RunEventRedactionSource::ModelOutput => "model_output",
        RunEventRedactionSource::ToolOutput => "tool_output",
        RunEventRedactionSource::ExternalError => "external_error",
    }
}

fn parse_redaction_source(
    value: &str,
) -> Result<RunEventRedactionSource, RunJournalRepositoryError> {
    match value {
        "untrusted_text" => Ok(RunEventRedactionSource::UntrustedText),
        "model_output" => Ok(RunEventRedactionSource::ModelOutput),
        "tool_output" => Ok(RunEventRedactionSource::ToolOutput),
        "external_error" => Ok(RunEventRedactionSource::ExternalError),
        _ => Err(RunJournalRepositoryError::InvalidStoredData),
    }
}

fn timestamp_to_i64(timestamp: AgentRunTimestamp) -> i64 {
    timestamp.unix_millis() as i64
}

fn sequence_to_i64(sequence: RunEventSequence) -> Result<i64, RunJournalRepositoryError> {
    u64_to_i64(sequence.get())
}

fn u64_to_i64(value: u64) -> Result<i64, RunJournalRepositoryError> {
    i64::try_from(value).map_err(|_| RunJournalRepositoryError::ResourceLimit)
}

fn id_bytes<T: StableIdBytes>(id: T) -> Vec<u8> {
    id.stable_bytes().to_vec()
}

trait StableIdBytes {
    fn stable_bytes(&self) -> &[u8; 32];
}

macro_rules! stable_id_bytes {
    ($($type:ty),+ $(,)?) => {
        $(impl StableIdBytes for $type {
            fn stable_bytes(&self) -> &[u8; 32] { self.as_bytes() }
        })+
    };
}

stable_id_bytes!(
    AgentRunId,
    RunEventId,
    SnapshotId,
    TaskEvidenceId,
    ToolRunId,
    WorktreeId,
    a3_domain::TaskId
);

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], RunJournalRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(RunJournalRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, RunJournalRepositoryError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(RunJournalRepositoryError::Read)?;
    bytes
        .map(|value| {
            value
                .try_into()
                .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, RunJournalRepositoryError> {
    row.get(index).map_err(RunJournalRepositoryError::Read)
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, RunJournalRepositoryError> {
    row.get(index).map_err(RunJournalRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, RunJournalRepositoryError> {
    row.get(index).map_err(RunJournalRepositoryError::Read)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<AgentRunTimestamp, RunJournalRepositoryError> {
    let value = u64::try_from(read_i64(row, index)?)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(value)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

fn read_sequence(
    row: &libsql::Row,
    index: i32,
) -> Result<RunEventSequence, RunJournalRepositoryError> {
    let value = u64::try_from(read_i64(row, index)?)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    RunEventSequence::new(value).map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

fn read_goal_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<GoalContractRevision, RunJournalRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    GoalContractRevision::new(value).map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

fn read_ledger_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<TaskLedgerRevision, RunJournalRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?;
    TaskLedgerRevision::new(value).map_err(|_| RunJournalRepositoryError::InvalidStoredData)
}

fn read_optional_ledger_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<TaskLedgerRevision>, RunJournalRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(RunJournalRepositoryError::Read)?;
    value
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
                .and_then(|value| {
                    TaskLedgerRevision::new(value)
                        .map_err(|_| RunJournalRepositoryError::InvalidStoredData)
                })
        })
        .transpose()
}

fn read_optional_u64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, RunJournalRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(RunJournalRepositoryError::Read)?;
    value
        .map(|value| u64::try_from(value).map_err(|_| RunJournalRepositoryError::InvalidStoredData))
        .transpose()
}

fn read_optional_bool(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<bool>, RunJournalRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(RunJournalRepositoryError::Read)?;
    value
        .map(|value| match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(RunJournalRepositoryError::InvalidStoredData),
        })
        .transpose()
}

async fn close_write_transaction<T>(
    transaction: Transaction,
    result: Result<T, RunJournalRepositoryError>,
) -> Result<T, RunJournalRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(RunJournalRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, RunJournalRepositoryError>,
) -> Result<T, RunJournalRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(RunJournalRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: RunJournalRepositoryError,
) -> Result<T, RunJournalRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(RunJournalRepositoryError::Rollback(source)),
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_unexpected_constraint(source: libsql::Error) -> RunJournalRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        RunJournalRepositoryError::InvalidStoredData
    } else {
        RunJournalRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum RunJournalRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    GoalContract(goal_contract_repository::GoalContractRepositoryError),
    TaskLedger(task_ledger_repository::TaskLedgerRepositoryError),
    InvalidPage(a3_application::RunEventPageError),
    InvalidInput,
    InvalidStoredData,
    ResourceLimit,
    RunAlreadyExists,
    RunNotFound,
    SequenceConflict,
}

impl RunJournalRepositoryError {
    pub(crate) fn classify(&self) -> RunJournalStoreFailure {
        match self {
            Self::InvalidInput
            | Self::InvalidStoredData
            | Self::ResourceLimit
            | Self::InvalidPage(_) => RunJournalStoreFailure::InvalidStoredData,
            Self::RunAlreadyExists => RunJournalStoreFailure::RunAlreadyExists,
            Self::RunNotFound => RunJournalStoreFailure::RunNotFound,
            Self::SequenceConflict => RunJournalStoreFailure::SequenceConflict,
            Self::GoalContract(error) => classify_goal_contract_failure(error.classify()),
            Self::TaskLedger(error) => classify_task_ledger_failure(error.classify()),
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    RunJournalStoreFailure::Corrupt
                } else {
                    RunJournalStoreFailure::Unavailable
                }
            }
        }
    }
}

fn classify_goal_contract_failure(
    failure: a3_application::GoalContractStoreFailure,
) -> RunJournalStoreFailure {
    match failure {
        a3_application::GoalContractStoreFailure::Unavailable => {
            RunJournalStoreFailure::Unavailable
        }
        a3_application::GoalContractStoreFailure::Corrupt => RunJournalStoreFailure::Corrupt,
        a3_application::GoalContractStoreFailure::UnsupportedSchema => {
            RunJournalStoreFailure::UnsupportedSchema
        }
        a3_application::GoalContractStoreFailure::InvalidStoredData
        | a3_application::GoalContractStoreFailure::TaskAlreadyExists
        | a3_application::GoalContractStoreFailure::TaskNotFound
        | a3_application::GoalContractStoreFailure::RevisionConflict => {
            RunJournalStoreFailure::InvalidStoredData
        }
    }
}

fn classify_task_ledger_failure(
    failure: a3_application::TaskLedgerStoreFailure,
) -> RunJournalStoreFailure {
    match failure {
        a3_application::TaskLedgerStoreFailure::Unavailable => RunJournalStoreFailure::Unavailable,
        a3_application::TaskLedgerStoreFailure::Corrupt => RunJournalStoreFailure::Corrupt,
        a3_application::TaskLedgerStoreFailure::UnsupportedSchema => {
            RunJournalStoreFailure::UnsupportedSchema
        }
        a3_application::TaskLedgerStoreFailure::InvalidStoredData
        | a3_application::TaskLedgerStoreFailure::LedgerAlreadyExists
        | a3_application::TaskLedgerStoreFailure::TaskNotFound
        | a3_application::TaskLedgerStoreFailure::VersionConflict => {
            RunJournalStoreFailure::InvalidStoredData
        }
    }
}

impl fmt::Display for RunJournalRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "run-journal transaction could not begin",
            Self::Read(_) => "run-journal data could not be read",
            Self::Write(_) => "run-journal data could not be written",
            Self::Commit(_) => "run-journal transaction could not commit",
            Self::Rollback(_) => "run-journal transaction could not roll back",
            Self::GoalContract(_) => "run-journal Goal Contract could not be reconstructed",
            Self::TaskLedger(_) => "run-journal Task Ledger could not be reconstructed",
            Self::InvalidPage(_) => "run-journal event page was invalid",
            Self::InvalidInput => "run-journal successor was invalid",
            Self::InvalidStoredData => "run-journal data was invalid",
            Self::ResourceLimit => "run-journal data exceeded a resource limit",
            Self::RunAlreadyExists => "agent run already exists",
            Self::RunNotFound => "agent run was not found",
            Self::SequenceConflict => "run-event sequence conflicted",
        })
    }
}

impl Error for RunJournalRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            Self::GoalContract(error) => Some(error),
            Self::TaskLedger(error) => Some(error),
            Self::InvalidPage(error) => Some(error),
            Self::InvalidInput
            | Self::InvalidStoredData
            | Self::ResourceLimit
            | Self::RunAlreadyExists
            | Self::RunNotFound
            | Self::SequenceConflict => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{create, load_run};
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRun,
        AgentRunId, AgentRunTimestamp, ExpectedTaskEvidence, GitHead, GitReferenceName,
        GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, IndexLanguage,
        IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion, NonGoal, RunEventId,
        Snapshot, SnapshotId, SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp,
        TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod,
        VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeGeneration,
        WorktreeId,
    };
    use std::error::Error;

    #[test]
    fn materialized_run_load_does_not_require_journal_replay() -> Result<(), Box<dyn Error>> {
        crate::run_native_libsql_test(async {
            let repository_id = [171; 32];
            let worktree_id = WorktreeId::from_bytes([172; 32]);
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            crate::migration::migrate_knowledge(
                &connection,
                &repository_id,
                worktree_id.as_bytes(),
            )
            .await?;

            let snapshot_id = SnapshotId::from_bytes([173; 32]);
            let snapshot = Snapshot::new(
                snapshot_id,
                worktree_id,
                None,
                WorktreeGeneration::new(1)?,
                GitHead::Unborn {
                    reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
                },
                IndexSchemaVersion::new(1)?,
                vec![LanguageAdapterRevision::new(
                    IndexLanguage::Rust,
                    LanguageAdapterVersion::try_from_string("journal-test-rust-1".to_owned())?,
                )],
                Vec::new(),
            )?;
            crate::index_repository::append_snapshot(&connection, worktree_id, &snapshot).await?;

            let goal = GoalContract::initial(
                TaskId::from_bytes([174; 32]),
                GoalContractDraft::new(
                    GoalObjective::try_from_string("read materialized run state".to_owned())?,
                    vec![AcceptanceCriterion::new(
                        AcceptanceCriterionId::from_bytes([175; 32]),
                        AcceptanceCriterionStatement::try_from_string(
                            "state does not require journal replay".to_owned(),
                        )?,
                    )],
                    Vec::new(),
                    vec![NonGoal::try_from_string(
                        "do not treat the journal as event sourcing".to_owned(),
                    )?],
                    Vec::new(),
                    SuccessVerification::try_from_string(
                        "load after simulated journal loss".to_owned(),
                    )?,
                )?,
                GoalContractTimestamp::from_unix_millis(3_000)?,
            );
            crate::goal_contract_repository::create(&connection, worktree_id, &goal).await?;
            let ledger = TaskLedger::new(
                goal.reference(),
                vec![TaskStepDefinition::new(
                    TaskStepId::from_bytes([176; 32]),
                    None,
                    TaskStepOutcome::try_from_string("load the run".to_owned())?,
                    TaskStepRationale::try_from_string(
                        "materialized state is authoritative".to_owned(),
                    )?,
                    Vec::new(),
                    vec![ExpectedTaskEvidence::try_from_string(
                        "the repository test passes".to_owned(),
                    )?],
                    VerificationSpec::new(
                        VerificationSpecId::from_bytes([177; 32]),
                        VerificationMethod::Test,
                        VerificationRequirement::try_from_string(
                            "the repository test passes".to_owned(),
                        )?,
                    ),
                )?],
                TaskLedgerTimestamp::from_unix_millis(3_001)?,
            )?;
            crate::task_ledger_repository::create(&connection, worktree_id, &ledger).await?;
            let (run, start_event) = AgentRun::start(
                AgentRunId::from_bytes([178; 32]),
                goal.reference(),
                ledger.revision(),
                snapshot_id,
                RunEventId::from_bytes([179; 32]),
                AgentRunTimestamp::from_unix_millis(3_002)?,
            )?;
            create(&connection, worktree_id, &run, &start_event).await?;
            connection.execute("DELETE FROM run_events", ()).await?;

            assert_eq!(
                load_run(&connection, worktree_id, run.id()).await?,
                Some(run)
            );
            Ok::<(), Box<dyn Error>>(())
        })
    }
}
