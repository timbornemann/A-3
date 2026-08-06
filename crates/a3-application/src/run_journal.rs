use crate::JobContext;
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, Progress, ProjectIdentity, RunEvent, RunEventCode,
    RunEventKind, RunEventOutcome, RunEventRedactionSource, RunEventSequence, RunEventSubject,
};
use serde_json::{Map, Value};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_RUN_EVENT_PAGE_SIZE: u16 = 256;
const MAX_RUN_JOURNAL_EXPORT_EVENTS: u64 = 10_000;
const MAX_RUN_JOURNAL_EXPORT_BYTES: usize = 8 * 1024 * 1024;

/// Owned future returned by the object-safe run-journal persistence port.
pub type RunJournalStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RunJournalStoreFailure>> + Send + 'a>>;

/// Non-zero bounded number of journal events returned by one storage read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunEventPageLimit(u16);

impl RunEventPageLimit {
    /// Creates a page limit from one through 256.
    pub const fn new(value: u16) -> Result<Self, RunEventPageLimitError> {
        if value == 0 || value > MAX_RUN_EVENT_PAGE_SIZE {
            return Err(RunEventPageLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the validated page size.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Run-event page limit was zero or exceeded the fixed maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEventPageLimitError {
    value: u16,
}

impl RunEventPageLimitError {
    /// Returns the rejected limit.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.value
    }
}

impl fmt::Display for RunEventPageLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "run-event page limit is {}; expected 1 through {MAX_RUN_EVENT_PAGE_SIZE}",
            self.value
        )
    }
}

impl Error for RunEventPageLimitError {}

/// One contiguous bounded journal page and whether a later page exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEventPage {
    events: Vec<RunEvent>,
    has_more: bool,
}

impl RunEventPage {
    /// Validates a page returned after `after_sequence`.
    pub fn new(
        after_sequence: Option<RunEventSequence>,
        limit: RunEventPageLimit,
        events: Vec<RunEvent>,
        has_more: bool,
    ) -> Result<Self, RunEventPageError> {
        if events.len() > usize::from(limit.get()) {
            return Err(RunEventPageError::LimitExceeded {
                actual: events.len(),
                limit,
            });
        }
        let mut expected = match after_sequence {
            Some(sequence) => sequence
                .get()
                .checked_add(1)
                .ok_or(RunEventPageError::SequenceOverflow)?,
            None => RunEventSequence::FIRST.get(),
        };
        for event in &events {
            if event.sequence().get() != expected {
                return Err(RunEventPageError::NonContiguous);
            }
            expected = expected
                .checked_add(1)
                .ok_or(RunEventPageError::SequenceOverflow)?;
        }
        if has_more && events.len() != usize::from(limit.get()) {
            return Err(RunEventPageError::InvalidContinuation);
        }
        Ok(Self { events, has_more })
    }

    /// Returns the events in ascending contiguous sequence order.
    #[must_use]
    pub fn events(&self) -> &[RunEvent] {
        &self.events
    }

    /// Returns whether at least one later event exists.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// Adapter returned an invalid bounded run-event page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEventPageError {
    /// Page contained more events than requested.
    LimitExceeded {
        /// Returned event count.
        actual: usize,
        /// Validated requested maximum.
        limit: RunEventPageLimit,
    },
    /// First or later event did not immediately follow its predecessor.
    NonContiguous,
    /// A continuation was claimed without a full current page.
    InvalidContinuation,
    /// Sequence arithmetic exceeded the persisted range.
    SequenceOverflow,
}

impl fmt::Display for RunEventPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded { actual, limit } => write!(
                formatter,
                "run-event page returned {actual} events; limit is {}",
                limit.get()
            ),
            Self::NonContiguous => formatter.write_str("run-event page is not contiguous"),
            Self::InvalidContinuation => {
                formatter.write_str("run-event page continuation metadata is invalid")
            }
            Self::SequenceOverflow => formatter.write_str("run-event page sequence overflowed"),
        }
    }
}

impl Error for RunEventPageError {}

/// Persistence boundary for materialized runs and their append-only audit events.
pub trait RunJournalStore: fmt::Debug + Send + Sync {
    /// Creates one run together with its mandatory sequence-one start event atomically.
    fn create_agent_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run: &'a AgentRun,
        start_event: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()>;

    /// Appends exactly one event and compare-and-swaps materialized run state atomically.
    fn append_run_event<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()>;

    /// Loads materialized state without replaying or requiring the journal body.
    fn load_agent_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
    ) -> RunJournalStoreFuture<'a, Option<AgentRun>>;

    /// Loads one bounded contiguous event page after the optional cursor.
    fn load_run_events<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        after_sequence: Option<RunEventSequence>,
        limit: RunEventPageLimit,
    ) -> RunJournalStoreFuture<'a, RunEventPage>;
}

/// Stable application classification of run-journal persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunJournalStoreFailure {
    /// Local worktree storage could not be reached or written.
    Unavailable,
    /// Local worktree storage failed integrity checks.
    Corrupt,
    /// Database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated relational, page, or domain invariants.
    InvalidStoredData,
    /// A run with this identity already exists.
    RunAlreadyExists,
    /// The run or its exact Goal Contract, Ledger, or Snapshot anchor does not exist.
    RunNotFound,
    /// Another writer already appended the next event.
    SequenceConflict,
}

impl fmt::Display for RunJournalStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "run-journal storage is unavailable",
            Self::Corrupt => "run-journal storage is corrupt",
            Self::UnsupportedSchema => "run-journal storage uses an unsupported schema",
            Self::InvalidStoredData => "run-journal storage contains invalid data",
            Self::RunAlreadyExists => "agent run already exists",
            Self::RunNotFound => "agent run was not found",
            Self::SequenceConflict => "run-event sequence conflicts with the current journal",
        })
    }
}

impl Error for RunJournalStoreFailure {}

/// Cooperative cancellation and progress boundary for a bounded journal export.
pub trait RunJournalExportControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports exported events against the fixed materialized event count.
    fn report_progress(&self, progress: Progress) -> Result<(), RunJournalExportControlError>;
}

impl RunJournalExportControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RunJournalExportControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| RunJournalExportControlError::Unavailable)
    }
}

/// Journal-export progress could not be delivered to its owning operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunJournalExportControlError {
    /// The owner no longer accepts progress updates.
    Unavailable,
}

impl fmt::Display for RunJournalExportControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("run-journal export progress is unavailable")
    }
}

impl Error for RunJournalExportControlError {}

/// Stable, content-free JSON Lines export of one complete run audit journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunJournalExport {
    bytes: Vec<u8>,
    event_count: u64,
    schema_version: RunJournalExportSchemaVersion,
    retention_policy: RunJournalRetentionPolicy,
}

impl RunJournalExport {
    /// Returns schema-versioned UTF-8 JSON Lines bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the number of exported immutable events, excluding the header line.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    /// Returns the strict export schema used for every line.
    #[must_use]
    pub const fn schema_version(&self) -> RunJournalExportSchemaVersion {
        self.schema_version
    }

    /// Returns the non-destructive V1 audit retention policy declared by the export.
    #[must_use]
    pub const fn retention_policy(&self) -> RunJournalRetentionPolicy {
        self.retention_policy
    }
}

/// Version of the strict, deterministic run-journal JSON Lines schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunJournalExportSchemaVersion {
    /// Header plus one content-free event record per immutable journal row.
    V1,
}

impl RunJournalExportSchemaVersion {
    /// Returns the persisted numeric version.
    #[must_use]
    pub const fn get(self) -> u16 {
        match self {
            Self::V1 => 1,
        }
    }
}

/// Retention rule for V1 journals under ADR-0013 and the H8 audit invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunJournalRetentionPolicy {
    /// Preserve every structured audit event; raw untrusted content is never journaled.
    PreserveAuditEvents,
}

/// Failure from producing a bounded, deterministic run-journal export.
#[derive(Debug)]
pub enum RunJournalExportError {
    /// Durable state or journal paging failed.
    Store(RunJournalStoreFailure),
    /// No materialized run exists for the requested identity.
    RunNotFound,
    /// The fixed V1 event-count bound was exceeded.
    EventLimitExceeded,
    /// The fixed V1 byte bound was exceeded.
    ByteLimitExceeded,
    /// The owning operation cancelled export between bounded pages.
    Cancelled,
    /// Progress could not be delivered to the owner.
    ProgressUnavailable,
    /// The stable JSON representation could not be serialized.
    Serialization(serde_json::Error),
}

impl fmt::Display for RunJournalExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Store(_) => "run-journal export could not read durable state",
            Self::RunNotFound => "run-journal export run was not found",
            Self::EventLimitExceeded => "run-journal export exceeds the event limit",
            Self::ByteLimitExceeded => "run-journal export exceeds the byte limit",
            Self::Cancelled => "run-journal export was cancelled",
            Self::ProgressUnavailable => "run-journal export progress is unavailable",
            Self::Serialization(_) => "run-journal export could not serialize stable JSON Lines",
        })
    }
}

impl Error for RunJournalExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::RunNotFound
            | Self::EventLimitExceeded
            | Self::ByteLimitExceeded
            | Self::Cancelled
            | Self::ProgressUnavailable => None,
        }
    }
}

impl From<RunJournalStoreFailure> for RunJournalExportError {
    fn from(error: RunJournalStoreFailure) -> Self {
        Self::Store(error)
    }
}

/// Inbound use case exporting immutable audit events without raw text or provider payloads.
#[derive(Debug, Clone, Copy)]
pub struct ExportRunJournal<'a> {
    store: &'a dyn RunJournalStore,
}

impl<'a> ExportRunJournal<'a> {
    /// Creates the use case from its narrow read-only persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn RunJournalStore) -> Self {
        Self { store }
    }

    /// Exports a complete, bounded journal using the stable `a3.run-journal.jsonl` V1 schema.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run_id: AgentRunId,
        control: &dyn RunJournalExportControl,
    ) -> Result<RunJournalExport, RunJournalExportError> {
        if control.is_cancelled() {
            return Err(RunJournalExportError::Cancelled);
        }
        let run = self
            .store
            .load_agent_run(project, run_id)
            .await?
            .ok_or(RunJournalExportError::RunNotFound)?;
        let total = run.last_event_sequence().get();
        if total > MAX_RUN_JOURNAL_EXPORT_EVENTS {
            return Err(RunJournalExportError::EventLimitExceeded);
        }
        report_export_progress(control, 0, total)?;

        let mut bytes = serialize_export_line(&export_header(&run))?;
        let mut after_sequence = None;
        let page_limit = RunEventPageLimit::new(MAX_RUN_EVENT_PAGE_SIZE)
            .map_err(|_| RunJournalExportError::EventLimitExceeded)?;
        let mut event_count = 0_u64;
        loop {
            if control.is_cancelled() {
                return Err(RunJournalExportError::Cancelled);
            }
            let page = self
                .store
                .load_run_events(project, run_id, after_sequence, page_limit)
                .await?;
            for event in page.events() {
                append_export_line(&mut bytes, &export_event(event))?;
                event_count = event_count
                    .checked_add(1)
                    .ok_or(RunJournalExportError::EventLimitExceeded)?;
                after_sequence = Some(event.sequence());
            }
            report_export_progress(control, event_count, total)?;
            if !page.has_more() {
                break;
            }
        }
        if event_count != total {
            return Err(RunJournalExportError::Store(
                RunJournalStoreFailure::InvalidStoredData,
            ));
        }
        Ok(RunJournalExport {
            bytes,
            event_count,
            schema_version: RunJournalExportSchemaVersion::V1,
            retention_policy: RunJournalRetentionPolicy::PreserveAuditEvents,
        })
    }
}

fn report_export_progress(
    control: &dyn RunJournalExportControl,
    completed: u64,
    total: u64,
) -> Result<(), RunJournalExportError> {
    let progress = Progress::determinate(completed, total)
        .map_err(|_| RunJournalExportError::EventLimitExceeded)?;
    control
        .report_progress(progress)
        .map_err(|_| RunJournalExportError::ProgressUnavailable)
}

fn serialize_export_line(value: &Value) -> Result<Vec<u8>, RunJournalExportError> {
    let mut bytes = serde_json::to_vec(value).map_err(RunJournalExportError::Serialization)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RUN_JOURNAL_EXPORT_BYTES {
        return Err(RunJournalExportError::ByteLimitExceeded);
    }
    Ok(bytes)
}

fn append_export_line(bytes: &mut Vec<u8>, value: &Value) -> Result<(), RunJournalExportError> {
    let line = serialize_export_line(value)?;
    if bytes.len().saturating_add(line.len()) > MAX_RUN_JOURNAL_EXPORT_BYTES {
        return Err(RunJournalExportError::ByteLimitExceeded);
    }
    bytes.extend_from_slice(&line);
    Ok(())
}

fn export_header(run: &AgentRun) -> Value {
    let mut header = Map::new();
    header.insert("record".to_owned(), Value::String("header".to_owned()));
    header.insert(
        "schema".to_owned(),
        Value::String("a3.run-journal.jsonl".to_owned()),
    );
    header.insert(
        "version".to_owned(),
        Value::from(RunJournalExportSchemaVersion::V1.get()),
    );
    header.insert(
        "run_id".to_owned(),
        Value::String(hex_id(run.id().as_bytes())),
    );
    header.insert(
        "event_count".to_owned(),
        Value::from(run.last_event_sequence().get()),
    );
    header.insert(
        "retention".to_owned(),
        Value::String("audit_events_preserved".to_owned()),
    );
    Value::Object(header)
}

fn export_event(event: &RunEvent) -> Value {
    let mut value = Map::new();
    value.insert("record".to_owned(), Value::String("event".to_owned()));
    value.insert(
        "event_id".to_owned(),
        Value::String(hex_id(event.id().as_bytes())),
    );
    value.insert(
        "run_id".to_owned(),
        Value::String(hex_id(event.run_id().as_bytes())),
    );
    value.insert("sequence".to_owned(), Value::from(event.sequence().get()));
    value.insert(
        "occurred_at_unix_millis".to_owned(),
        Value::from(event.occurred_at().unix_millis()),
    );
    let (kind, state_from, state_to, ledger_from, ledger_to) = export_event_kind(event.kind());
    value.insert("kind".to_owned(), Value::String(kind.to_owned()));
    value.insert("state_from".to_owned(), optional_string(state_from));
    value.insert("state_to".to_owned(), optional_string(state_to));
    value.insert("ledger_revision_from".to_owned(), optional_u32(ledger_from));
    value.insert("ledger_revision_to".to_owned(), optional_u32(ledger_to));
    value.insert(
        "payload_code".to_owned(),
        Value::String(export_event_code(event.payload().code()).to_owned()),
    );
    value.insert(
        "payload_outcome".to_owned(),
        optional_string(event.payload().outcome().map(export_event_outcome)),
    );
    let redaction = event.payload().redaction();
    value.insert(
        "redaction_source".to_owned(),
        optional_string(redaction.map(|item| export_redaction_source(item.source()))),
    );
    value.insert(
        "redaction_observed_bytes".to_owned(),
        redaction.map_or(Value::Null, |item| Value::from(item.observed_bytes())),
    );
    value.insert(
        "redaction_source_truncated".to_owned(),
        redaction.map_or(Value::Null, |item| Value::from(item.source_was_truncated())),
    );
    value.insert(
        "payload_digest".to_owned(),
        Value::String(hex_id(event.payload().digest().as_bytes())),
    );
    value.insert(
        "snapshot_id".to_owned(),
        Value::String(hex_id(event.snapshot_id().as_bytes())),
    );
    let (subject_kind, subject_id) = match event.subject() {
        Some(RunEventSubject::Tool(id)) => (Some("tool"), Some(hex_id(id.as_bytes()))),
        Some(RunEventSubject::Evidence(id)) => (Some("evidence"), Some(hex_id(id.as_bytes()))),
        None => (None, None),
    };
    value.insert("subject_kind".to_owned(), optional_string(subject_kind));
    value.insert(
        "subject_id".to_owned(),
        subject_id.map_or(Value::Null, Value::String),
    );
    Value::Object(value)
}

type ExportEventKind = (
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    Option<u32>,
    Option<u32>,
);

fn export_event_kind(kind: RunEventKind) -> ExportEventKind {
    match kind {
        RunEventKind::RunStarted => ("run_started", None, None, None, None),
        RunEventKind::StateTransition { from, to } => (
            "state_transition",
            Some(export_controller_state(from)),
            Some(export_controller_state(to)),
            None,
            None,
        ),
        RunEventKind::ContextCompiled => ("context_compiled", None, None, None, None),
        RunEventKind::ModelInteraction => ("model_interaction", None, None, None, None),
        RunEventKind::ToolAction => ("tool_action", None, None, None, None),
        RunEventKind::LedgerUpdated { from, to } => (
            "ledger_updated",
            None,
            None,
            Some(from.get()),
            Some(to.get()),
        ),
        RunEventKind::VerificationRecorded => ("verification_recorded", None, None, None, None),
        RunEventKind::ApprovalRecorded => ("approval_recorded", None, None, None, None),
        RunEventKind::Diagnostic => ("diagnostic", None, None, None, None),
    }
}

fn export_controller_state(state: AgentControllerState) -> &'static str {
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

fn export_event_code(code: RunEventCode) -> &'static str {
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

fn export_event_outcome(outcome: RunEventOutcome) -> &'static str {
    match outcome {
        RunEventOutcome::Succeeded => "succeeded",
        RunEventOutcome::Failed => "failed",
        RunEventOutcome::Cancelled => "cancelled",
        RunEventOutcome::Denied => "denied",
    }
}

fn export_redaction_source(source: RunEventRedactionSource) -> &'static str {
    match source {
        RunEventRedactionSource::UntrustedText => "untrusted_text",
        RunEventRedactionSource::ModelOutput => "model_output",
        RunEventRedactionSource::ToolOutput => "tool_output",
        RunEventRedactionSource::ExternalError => "external_error",
    }
}

fn optional_string(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |item| Value::String(item.to_owned()))
}

fn optional_u32(value: Option<u32>) -> Value {
    value.map_or(Value::Null, Value::from)
}

fn hex_id(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

/// Inbound use case atomically creating a materialized run and its start event.
#[derive(Debug, Clone, Copy)]
pub struct CreateAgentRun<'a> {
    store: &'a dyn RunJournalStore,
}

impl<'a> CreateAgentRun<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn RunJournalStore) -> Self {
        Self { store }
    }

    /// Persists only a correctly paired sequence-one run and start event.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        start_event: &RunEvent,
    ) -> Result<(), RunJournalStoreFailure> {
        self.store.create_agent_run(project, run, start_event).await
    }
}

/// Inbound use case atomically appending an event and its resulting current run state.
#[derive(Debug, Clone, Copy)]
pub struct AppendRunEvent<'a> {
    store: &'a dyn RunJournalStore,
}

impl<'a> AppendRunEvent<'a> {
    /// Creates the use case from its narrow persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn RunJournalStore) -> Self {
        Self { store }
    }

    /// Compare-and-swaps the exact expected event tail in one storage transaction.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &AgentRun,
        event: &RunEvent,
    ) -> Result<(), RunJournalStoreFailure> {
        self.store
            .append_run_event(project, expected_last_sequence, run, event)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{RunEventPage, RunEventPageError, RunEventPageLimit};
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRun,
        AgentRunId, AgentRunTimestamp, GoalContract, GoalContractDraft, GoalContractTimestamp,
        GoalObjective, RunEventId, RunEventIdentity, RunEventKind, RunEventOccurrence,
        RunEventPayload, RunEventSequence, SnapshotId, SuccessVerification, TaskId,
        TaskLedgerRevision,
    };
    use std::error::Error;

    #[test]
    fn event_page_rejects_gaps_and_false_continuations() -> Result<(), Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("test run-event paging".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "pages remain contiguous".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run application tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let (run, event) = AgentRun::start(
            AgentRunId::from_bytes([3; 32]),
            goal.reference(),
            TaskLedgerRevision::INITIAL,
            SnapshotId::from_bytes([4; 32]),
            RunEventId::from_bytes([5; 32]),
            AgentRunTimestamp::from_unix_millis(1)?,
        )?;

        assert!(matches!(
            RunEventPage::new(None, RunEventPageLimit::new(2)?, vec![event.clone()], true),
            Err(RunEventPageError::InvalidContinuation)
        ));
        let third = a3_domain::RunEvent::reconstruct(
            RunEventIdentity::new(
                RunEventId::from_bytes([6; 32]),
                run.id(),
                RunEventSequence::new(3)?,
            ),
            RunEventOccurrence::new(
                AgentRunTimestamp::from_unix_millis(2)?,
                run.current_snapshot_id(),
                None,
            ),
            RunEventKind::Diagnostic,
            RunEventPayload::empty(),
        )?;
        assert!(matches!(
            RunEventPage::new(None, RunEventPageLimit::new(2)?, vec![event, third], false),
            Err(RunEventPageError::NonContiguous)
        ));
        Ok(())
    }
}
