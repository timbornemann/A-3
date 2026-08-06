use super::{
    AgentBudgetEvaluationError, AgentBudgetExhaustion, AgentRunBudget, AgentRunId, AgentRunUsage,
    AgentRunUsageError, AgentTurnCharge, GoalContractReference, ModelProfileReference, RunEventId,
    SnapshotId, TaskEvidenceId, TaskLedgerRevision, ToolRunId,
};
use std::error::Error;
use std::fmt;

const MAX_PERSISTED_TIMESTAMP_MILLIS: u64 = i64::MAX as u64;

/// Finite controller state mandated for one A^3 agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentControllerState {
    /// Validate and anchor the requested task.
    Intake,
    /// Retrieve deterministic repository context.
    Localize,
    /// Construct or inspect a typed task plan.
    Plan,
    /// Execute one allowed action for the current step.
    Execute,
    /// Verify the produced outcome against explicit evidence.
    Verify,
    /// Replace future plan steps after a material finding.
    Replan,
    /// Wait for one scoped user approval.
    AwaitApproval,
    /// Terminal successful state after current acceptance verification.
    Done,
    /// Terminal unsuccessful state.
    Failed,
    /// Terminal user- or policy-requested cancellation.
    Cancelled,
}

impl AgentControllerState {
    /// Returns whether no later event may change this run's controller state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    const fn may_transition_to(self, next: Self) -> bool {
        match self {
            Self::Intake => matches!(next, Self::Localize | Self::Failed | Self::Cancelled),
            Self::Localize => matches!(next, Self::Plan | Self::Failed | Self::Cancelled),
            Self::Plan => matches!(next, Self::Execute | Self::Failed | Self::Cancelled),
            Self::Execute => matches!(
                next,
                Self::Verify | Self::AwaitApproval | Self::Failed | Self::Cancelled
            ),
            Self::Verify => matches!(
                next,
                Self::Execute | Self::Replan | Self::Done | Self::Failed | Self::Cancelled
            ),
            Self::Replan => matches!(next, Self::Localize | Self::Failed | Self::Cancelled),
            Self::AwaitApproval => {
                matches!(next, Self::Execute | Self::Failed | Self::Cancelled)
            }
            Self::Done | Self::Failed | Self::Cancelled => false,
        }
    }
}

/// Persistable wall-clock timestamp used only as run audit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentRunTimestamp(u64);

impl AgentRunTimestamp {
    /// Creates a timestamp exactly representable by signed SQLite storage.
    pub const fn from_unix_millis(value: u64) -> Result<Self, AgentRunTimestampError> {
        if value > MAX_PERSISTED_TIMESTAMP_MILLIS {
            return Err(AgentRunTimestampError);
        }
        Ok(Self(value))
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

/// Agent-run timestamp exceeded the exact persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunTimestampError;

impl fmt::Display for AgentRunTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent-run timestamp exceeds the persisted range")
    }
}

impl Error for AgentRunTimestampError {}

/// One-based, strictly increasing event position local to one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunEventSequence(u64);

impl RunEventSequence {
    /// Sequence of the mandatory run-start event.
    pub const FIRST: Self = Self(1);

    /// Creates a non-zero run-event sequence.
    pub const fn new(value: u64) -> Result<Self, RunEventSequenceError> {
        if value == 0 {
            return Err(RunEventSequenceError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    fn next(self) -> Result<Self, RunEventSequenceError> {
        match self.0.checked_add(1) {
            Some(value) => Ok(Self(value)),
            None => Err(RunEventSequenceError),
        }
    }
}

/// Run-event sequence was zero or overflowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEventSequenceError;

impl fmt::Display for RunEventSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("run-event sequence must be non-zero and cannot overflow")
    }
}

impl Error for RunEventSequenceError {}

/// Stable, non-textual reason safe to retain in a journal payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunEventCode {
    /// No additional reason is needed.
    None,
    /// The user explicitly requested the action.
    UserRequest,
    /// The deterministic controller selected the transition.
    ControllerDecision,
    /// A central policy decision allowed or denied an action.
    PolicyDecision,
    /// A bounded operation reached its deadline.
    Timeout,
    /// Cancellation was observed.
    Cancellation,
    /// Structured model output was rejected.
    InvalidModelOutput,
    /// A bounded tool operation failed.
    ToolFailure,
    /// Deterministic verification did not pass.
    VerificationFailure,
    /// Durable state was recovered after restart.
    StateRecovered,
}

/// Coarse event result safe for audit and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunEventOutcome {
    /// The described operation completed successfully.
    Succeeded,
    /// The described operation failed.
    Failed,
    /// The described operation was cancelled.
    Cancelled,
    /// Policy or the user denied the operation.
    Denied,
}

/// Untrusted source whose raw content was deliberately omitted before persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunEventRedactionSource {
    /// Free user- or repository-derived text.
    UntrustedText,
    /// Raw model provider output.
    ModelOutput,
    /// Raw process or tool output.
    ToolOutput,
    /// Raw error-chain text from an external boundary.
    ExternalError,
}

/// Content-free proof that an untrusted payload was redacted centrally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunEventRedaction {
    source: RunEventRedactionSource,
    observed_bytes: u64,
    source_was_truncated: bool,
}

impl RunEventRedaction {
    /// Records only safe metadata about omitted raw content.
    #[must_use]
    pub const fn new(
        source: RunEventRedactionSource,
        observed_bytes: u64,
        source_was_truncated: bool,
    ) -> Self {
        Self {
            source,
            observed_bytes,
            source_was_truncated,
        }
    }

    /// Returns the omitted content category.
    #[must_use]
    pub const fn source(self) -> RunEventRedactionSource {
        self.source
    }

    /// Returns the observed byte count without retaining the bytes.
    #[must_use]
    pub const fn observed_bytes(self) -> u64 {
        self.observed_bytes
    }

    /// Returns whether the upstream observation was already truncated.
    #[must_use]
    pub const fn source_was_truncated(self) -> bool {
        self.source_was_truncated
    }
}

/// Domain-separated digest of the canonical, secret-free structured payload.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunPayloadDigest([u8; 32]);

impl RunPayloadDigest {
    /// Reconstructs a persisted payload digest before payload validation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for RunPayloadDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RunPayloadDigest([REDACTED])")
    }
}

/// Version-one structured journal payload containing no free-form content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEventPayload {
    code: RunEventCode,
    outcome: Option<RunEventOutcome>,
    redaction: Option<RunEventRedaction>,
    digest: RunPayloadDigest,
}

impl RunEventPayload {
    /// Creates a structured payload from safe enums and optional content-free redaction metadata.
    #[must_use]
    pub fn new(
        code: RunEventCode,
        outcome: Option<RunEventOutcome>,
        redaction: Option<RunEventRedaction>,
    ) -> Self {
        let digest = derive_payload_digest(code, outcome, redaction);
        Self {
            code,
            outcome,
            redaction,
            digest,
        }
    }

    /// Reconstructs and authenticates the canonical structured payload.
    pub fn reconstruct(
        code: RunEventCode,
        outcome: Option<RunEventOutcome>,
        redaction: Option<RunEventRedaction>,
        digest: RunPayloadDigest,
    ) -> Result<Self, AgentRunError> {
        let payload = Self::new(code, outcome, redaction);
        if payload.digest != digest {
            return Err(AgentRunError::PayloadDigestMismatch);
        }
        Ok(payload)
    }

    /// Returns an empty safe payload.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(RunEventCode::None, None, None)
    }

    /// Returns its safe reason code.
    #[must_use]
    pub const fn code(&self) -> RunEventCode {
        self.code
    }

    /// Returns its optional coarse result.
    #[must_use]
    pub const fn outcome(&self) -> Option<RunEventOutcome> {
        self.outcome
    }

    /// Returns metadata proving raw content was omitted.
    #[must_use]
    pub const fn redaction(&self) -> Option<RunEventRedaction> {
        self.redaction
    }

    /// Returns the digest of this canonical secret-free payload.
    #[must_use]
    pub const fn digest(&self) -> RunPayloadDigest {
        self.digest
    }
}

fn derive_payload_digest(
    code: RunEventCode,
    outcome: Option<RunEventOutcome>,
    redaction: Option<RunEventRedaction>,
) -> RunPayloadDigest {
    let mut hasher = blake3::Hasher::new_derive_key("a3.run-event-payload.v1");
    hasher.update(&[run_event_code_tag(code)]);
    hasher.update(&[outcome.map_or(0, |value| run_event_outcome_tag(value) + 1)]);
    match redaction {
        Some(value) => {
            hasher.update(&[run_event_redaction_source_tag(value.source()) + 1]);
            hasher.update(&value.observed_bytes().to_le_bytes());
            hasher.update(&[u8::from(value.source_was_truncated())]);
        }
        None => {
            hasher.update(&[0]);
            hasher.update(&0_u64.to_le_bytes());
            hasher.update(&[0]);
        }
    }
    RunPayloadDigest(*hasher.finalize().as_bytes())
}

const fn run_event_code_tag(code: RunEventCode) -> u8 {
    match code {
        RunEventCode::None => 0,
        RunEventCode::UserRequest => 1,
        RunEventCode::ControllerDecision => 2,
        RunEventCode::PolicyDecision => 3,
        RunEventCode::Timeout => 4,
        RunEventCode::Cancellation => 5,
        RunEventCode::InvalidModelOutput => 6,
        RunEventCode::ToolFailure => 7,
        RunEventCode::VerificationFailure => 8,
        RunEventCode::StateRecovered => 9,
    }
}

const fn run_event_outcome_tag(outcome: RunEventOutcome) -> u8 {
    match outcome {
        RunEventOutcome::Succeeded => 0,
        RunEventOutcome::Failed => 1,
        RunEventOutcome::Cancelled => 2,
        RunEventOutcome::Denied => 3,
    }
}

const fn run_event_redaction_source_tag(source: RunEventRedactionSource) -> u8 {
    match source {
        RunEventRedactionSource::UntrustedText => 0,
        RunEventRedactionSource::ModelOutput => 1,
        RunEventRedactionSource::ToolOutput => 2,
        RunEventRedactionSource::ExternalError => 3,
    }
}

/// Optional typed subject attached to a journal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunEventSubject {
    /// One bounded tool execution.
    Tool(ToolRunId),
    /// One evidence artifact.
    Evidence(TaskEvidenceId),
}

/// Stable category of one append-only run event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunEventKind {
    /// Mandatory first event of every run.
    RunStarted,
    /// Validated controller-state transition.
    StateTransition {
        /// Materialized state before the transition.
        from: AgentControllerState,
        /// Materialized state after the transition.
        to: AgentControllerState,
    },
    /// A deterministic context pack was compiled.
    ContextCompiled,
    /// A bounded model interaction completed or failed.
    ModelInteraction,
    /// A bounded tool action was requested or completed.
    ToolAction,
    /// The durable Task Ledger advanced through one validated replan.
    LedgerUpdated {
        /// Materialized plan revision before the replan.
        from: TaskLedgerRevision,
        /// Immediate next materialized plan revision.
        to: TaskLedgerRevision,
    },
    /// A deterministic verification was recorded.
    VerificationRecorded,
    /// A scoped approval was requested or resolved.
    ApprovalRecorded,
    /// A safe diagnostic marker was recorded.
    Diagnostic,
}

/// One immutable, snapshot-bound event in a run journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEvent {
    id: RunEventId,
    run_id: AgentRunId,
    sequence: RunEventSequence,
    occurred_at: AgentRunTimestamp,
    kind: RunEventKind,
    payload: RunEventPayload,
    snapshot_id: SnapshotId,
    subject: Option<RunEventSubject>,
    turn_charge: Option<AgentTurnCharge>,
}

impl RunEvent {
    /// Reconstructs an event after validating start and transition shape.
    pub fn reconstruct(
        identity: RunEventIdentity,
        occurrence: RunEventOccurrence,
        kind: RunEventKind,
        payload: RunEventPayload,
    ) -> Result<Self, AgentRunError> {
        if occurrence.turn_charge.is_some() && kind != RunEventKind::ModelInteraction {
            return Err(AgentRunError::InvalidTurnCharge);
        }
        match kind {
            RunEventKind::RunStarted if identity.sequence != RunEventSequence::FIRST => {
                return Err(AgentRunError::InvalidStartEvent);
            }
            RunEventKind::StateTransition { from, to }
                if identity.sequence == RunEventSequence::FIRST || !from.may_transition_to(to) =>
            {
                return Err(AgentRunError::InvalidStateTransition { from, to });
            }
            RunEventKind::LedgerUpdated { from, to }
                if identity.sequence == RunEventSequence::FIRST
                    || from.get().checked_add(1) != Some(to.get()) =>
            {
                return Err(AgentRunError::InvalidLedgerRevision { from, to });
            }
            RunEventKind::RunStarted
            | RunEventKind::StateTransition { .. }
            | RunEventKind::LedgerUpdated { .. } => {}
            _ if identity.sequence == RunEventSequence::FIRST => {
                return Err(AgentRunError::InvalidStartEvent);
            }
            _ => {}
        }
        Ok(Self {
            id: identity.id,
            run_id: identity.run_id,
            sequence: identity.sequence,
            occurred_at: occurrence.occurred_at,
            kind,
            payload,
            snapshot_id: occurrence.snapshot_id,
            subject: occurrence.subject,
            turn_charge: occurrence.turn_charge,
        })
    }

    /// Returns the globally stable event identity.
    #[must_use]
    pub const fn id(&self) -> RunEventId {
        self.id
    }

    /// Returns the owning run identity.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the contiguous run-local position.
    #[must_use]
    pub const fn sequence(&self) -> RunEventSequence {
        self.sequence
    }

    /// Returns the audit timestamp.
    #[must_use]
    pub const fn occurred_at(&self) -> AgentRunTimestamp {
        self.occurred_at
    }

    /// Returns the stable event category.
    #[must_use]
    pub const fn kind(&self) -> RunEventKind {
        self.kind
    }

    /// Returns the content-free structured payload.
    #[must_use]
    pub const fn payload(&self) -> &RunEventPayload {
        &self.payload
    }

    /// Returns the exact repository snapshot observed by this event.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the optional typed tool or evidence subject.
    #[must_use]
    pub const fn subject(&self) -> Option<RunEventSubject> {
        self.subject
    }

    /// Returns resource usage for one controller turn, absent only on non-turn or legacy events.
    #[must_use]
    pub const fn turn_charge(&self) -> Option<AgentTurnCharge> {
        self.turn_charge
    }
}

/// Stable identity and position used to reconstruct one journal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEventIdentity {
    id: RunEventId,
    run_id: AgentRunId,
    sequence: RunEventSequence,
}

impl RunEventIdentity {
    /// Creates an event identity projection from typed fields.
    #[must_use]
    pub const fn new(id: RunEventId, run_id: AgentRunId, sequence: RunEventSequence) -> Self {
        Self {
            id,
            run_id,
            sequence,
        }
    }
}

/// Snapshot, time, and optional subject of one reconstructed event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEventOccurrence {
    occurred_at: AgentRunTimestamp,
    snapshot_id: SnapshotId,
    subject: Option<RunEventSubject>,
    turn_charge: Option<AgentTurnCharge>,
}

impl RunEventOccurrence {
    /// Creates event occurrence metadata from typed fields.
    #[must_use]
    pub const fn new(
        occurred_at: AgentRunTimestamp,
        snapshot_id: SnapshotId,
        subject: Option<RunEventSubject>,
    ) -> Self {
        Self {
            occurred_at,
            snapshot_id,
            subject,
            turn_charge: None,
        }
    }

    /// Creates occurrence metadata for one bounded controller turn.
    #[must_use]
    pub const fn for_turn(
        occurred_at: AgentRunTimestamp,
        snapshot_id: SnapshotId,
        turn_charge: AgentTurnCharge,
    ) -> Self {
        Self {
            occurred_at,
            snapshot_id,
            subject: None,
            turn_charge: Some(turn_charge),
        }
    }
}

/// Materialized current run state; the journal remains an audit source, not event sourcing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    id: AgentRunId,
    goal_contract: GoalContractReference,
    task_ledger_revision: TaskLedgerRevision,
    model_profile: Option<ModelProfileReference>,
    budget: AgentRunBudget,
    usage: AgentRunUsage,
    state: AgentControllerState,
    last_event_sequence: RunEventSequence,
    current_snapshot_id: SnapshotId,
    created_at: AgentRunTimestamp,
    updated_at: AgentRunTimestamp,
}

impl AgentRun {
    /// Starts one run in Intake and creates its mandatory sequence-one audit event.
    pub fn start(
        id: AgentRunId,
        goal_contract: GoalContractReference,
        task_ledger_revision: TaskLedgerRevision,
        model_profile: ModelProfileReference,
        snapshot_id: SnapshotId,
        event_id: RunEventId,
        created_at: AgentRunTimestamp,
    ) -> Result<(Self, RunEvent), AgentRunError> {
        Self::start_with_budget(
            id,
            goal_contract,
            task_ledger_revision,
            model_profile,
            AgentRunBudget::DEFAULT,
            snapshot_id,
            event_id,
            created_at,
        )
    }

    /// Starts one run with explicit immutable hard budgets.
    #[allow(clippy::too_many_arguments)]
    pub fn start_with_budget(
        id: AgentRunId,
        goal_contract: GoalContractReference,
        task_ledger_revision: TaskLedgerRevision,
        model_profile: ModelProfileReference,
        budget: AgentRunBudget,
        snapshot_id: SnapshotId,
        event_id: RunEventId,
        created_at: AgentRunTimestamp,
    ) -> Result<(Self, RunEvent), AgentRunError> {
        let event = RunEvent::reconstruct(
            RunEventIdentity::new(event_id, id, RunEventSequence::FIRST),
            RunEventOccurrence::new(created_at, snapshot_id, None),
            RunEventKind::RunStarted,
            RunEventPayload::empty(),
        )?;
        Ok((
            Self {
                id,
                goal_contract,
                task_ledger_revision,
                model_profile: Some(model_profile),
                budget,
                usage: AgentRunUsage::ZERO,
                state: AgentControllerState::Intake,
                last_event_sequence: RunEventSequence::FIRST,
                current_snapshot_id: snapshot_id,
                created_at,
                updated_at: created_at,
            },
            event,
        ))
    }

    /// Reconstructs independently materialized state without replaying the audit journal.
    pub fn reconstruct(
        identity: AgentRunIdentity,
        materialized: AgentRunMaterializedState,
        timing: AgentRunTiming,
    ) -> Result<Self, AgentRunError> {
        if timing.updated_at < timing.created_at {
            return Err(AgentRunError::TimestampRegressed);
        }
        Ok(Self {
            id: identity.id,
            goal_contract: identity.goal_contract,
            task_ledger_revision: identity.task_ledger_revision,
            model_profile: identity.model_profile,
            budget: identity.budget,
            usage: materialized.usage,
            state: materialized.state,
            last_event_sequence: materialized.last_event_sequence,
            current_snapshot_id: materialized.current_snapshot_id,
            created_at: timing.created_at,
            updated_at: timing.updated_at,
        })
    }

    /// Appends a non-transition event and advances the materialized audit cursor.
    pub fn record(
        &mut self,
        event_id: RunEventId,
        kind: RunEventKind,
        payload: RunEventPayload,
        snapshot_id: SnapshotId,
        subject: Option<RunEventSubject>,
        occurred_at: AgentRunTimestamp,
    ) -> Result<RunEvent, AgentRunError> {
        if matches!(
            kind,
            RunEventKind::RunStarted
                | RunEventKind::StateTransition { .. }
                | RunEventKind::LedgerUpdated { .. }
                | RunEventKind::ModelInteraction
        ) {
            return Err(AgentRunError::InvalidEventKind);
        }
        self.append(event_id, kind, payload, snapshot_id, subject, occurred_at)
    }

    /// Records one bounded model turn containing zero or one selected action.
    pub fn record_turn(
        &mut self,
        event_id: RunEventId,
        payload: RunEventPayload,
        snapshot_id: SnapshotId,
        occurred_at: AgentRunTimestamp,
        charge: AgentTurnCharge,
    ) -> Result<RunEvent, AgentRunError> {
        if self.state.is_terminal() {
            return Err(AgentRunError::TerminalRun);
        }
        if occurred_at < self.updated_at {
            return Err(AgentRunError::TimestampRegressed);
        }
        let sequence = self
            .last_event_sequence
            .next()
            .map_err(|_| AgentRunError::SequenceOverflow)?;
        let event = RunEvent::reconstruct(
            RunEventIdentity::new(event_id, self.id, sequence),
            RunEventOccurrence::for_turn(occurred_at, snapshot_id, charge),
            RunEventKind::ModelInteraction,
            payload,
        )?;
        self.apply_event(&event)?;
        Ok(event)
    }

    /// Records one immediate Task Ledger replan and advances the run's durable plan anchor.
    pub fn record_ledger_update(
        &mut self,
        event_id: RunEventId,
        next_revision: TaskLedgerRevision,
        payload: RunEventPayload,
        snapshot_id: SnapshotId,
        occurred_at: AgentRunTimestamp,
    ) -> Result<RunEvent, AgentRunError> {
        if self.task_ledger_revision.get().checked_add(1) != Some(next_revision.get()) {
            return Err(AgentRunError::InvalidLedgerRevision {
                from: self.task_ledger_revision,
                to: next_revision,
            });
        }
        self.append(
            event_id,
            RunEventKind::LedgerUpdated {
                from: self.task_ledger_revision,
                to: next_revision,
            },
            payload,
            snapshot_id,
            None,
            occurred_at,
        )
    }

    /// Applies one allowed controller transition and returns its next append-only event.
    pub fn transition(
        &mut self,
        event_id: RunEventId,
        next: AgentControllerState,
        payload: RunEventPayload,
        snapshot_id: SnapshotId,
        occurred_at: AgentRunTimestamp,
    ) -> Result<RunEvent, AgentRunError> {
        if !self.state.may_transition_to(next) {
            return Err(AgentRunError::InvalidStateTransition {
                from: self.state,
                to: next,
            });
        }
        self.append(
            event_id,
            RunEventKind::StateTransition {
                from: self.state,
                to: next,
            },
            payload,
            snapshot_id,
            None,
            occurred_at,
        )
    }

    /// Applies one reconstructed next event to validate storage state and sequence.
    pub fn apply_event(&mut self, event: &RunEvent) -> Result<(), AgentRunError> {
        if event.run_id != self.id {
            return Err(AgentRunError::RunMismatch);
        }
        let expected = self
            .last_event_sequence
            .next()
            .map_err(|_| AgentRunError::SequenceOverflow)?;
        if event.sequence != expected {
            return Err(AgentRunError::SequenceMismatch {
                expected,
                actual: event.sequence,
            });
        }
        if event.occurred_at < self.updated_at {
            return Err(AgentRunError::TimestampRegressed);
        }
        if self.state.is_terminal() {
            return Err(AgentRunError::TerminalRun);
        }
        match event.kind {
            RunEventKind::RunStarted => return Err(AgentRunError::InvalidEventKind),
            RunEventKind::StateTransition { from, to } => {
                if from != self.state || !from.may_transition_to(to) {
                    return Err(AgentRunError::InvalidStateTransition {
                        from: self.state,
                        to,
                    });
                }
                self.state = to;
            }
            RunEventKind::LedgerUpdated { from, to } => {
                if from != self.task_ledger_revision || from.get().checked_add(1) != Some(to.get())
                {
                    return Err(AgentRunError::InvalidLedgerRevision {
                        from: self.task_ledger_revision,
                        to,
                    });
                }
                self.task_ledger_revision = to;
            }
            _ => {}
        }
        if let Some(charge) = event.turn_charge {
            self.usage = self
                .usage
                .record_turn(charge)
                .map_err(AgentRunError::Usage)?;
        }
        self.last_event_sequence = event.sequence;
        self.current_snapshot_id = event.snapshot_id;
        self.updated_at = event.occurred_at;
        Ok(())
    }

    fn append(
        &mut self,
        event_id: RunEventId,
        kind: RunEventKind,
        payload: RunEventPayload,
        snapshot_id: SnapshotId,
        subject: Option<RunEventSubject>,
        occurred_at: AgentRunTimestamp,
    ) -> Result<RunEvent, AgentRunError> {
        if self.state.is_terminal() {
            return Err(AgentRunError::TerminalRun);
        }
        if occurred_at < self.updated_at {
            return Err(AgentRunError::TimestampRegressed);
        }
        let sequence = self
            .last_event_sequence
            .next()
            .map_err(|_| AgentRunError::SequenceOverflow)?;
        let event = RunEvent::reconstruct(
            RunEventIdentity::new(event_id, self.id, sequence),
            RunEventOccurrence::new(occurred_at, snapshot_id, subject),
            kind,
            payload,
        )?;
        self.apply_event(&event)?;
        Ok(event)
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn id(&self) -> AgentRunId {
        self.id
    }

    /// Returns the exact immutable Goal Contract revision used by this run.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the Task Ledger plan revision selected at run start.
    #[must_use]
    pub const fn task_ledger_revision(&self) -> TaskLedgerRevision {
        self.task_ledger_revision
    }

    /// Returns the exact model profile, or `None` only for a pre-H5 legacy run.
    #[must_use]
    pub const fn model_profile(&self) -> Option<ModelProfileReference> {
        self.model_profile
    }

    /// Returns immutable hard resource ceilings selected at run start.
    #[must_use]
    pub const fn budget(&self) -> AgentRunBudget {
        self.budget
    }

    /// Returns durable cumulative turn usage.
    #[must_use]
    pub const fn usage(&self) -> AgentRunUsage {
        self.usage
    }

    /// Evaluates the first exhausted hard limit at one observed wall-clock timestamp.
    pub fn budget_exhaustion(
        &self,
        observed_at: AgentRunTimestamp,
    ) -> Result<Option<AgentBudgetExhaustion>, AgentBudgetEvaluationError> {
        self.budget
            .exhaustion(self.usage, self.created_at, observed_at)
    }

    /// Returns the current finite controller state.
    #[must_use]
    pub const fn state(&self) -> AgentControllerState {
        self.state
    }

    /// Returns the latest durable event position.
    #[must_use]
    pub const fn last_event_sequence(&self) -> RunEventSequence {
        self.last_event_sequence
    }

    /// Returns the snapshot observed by the latest event.
    #[must_use]
    pub const fn current_snapshot_id(&self) -> SnapshotId {
        self.current_snapshot_id
    }

    /// Returns run creation time.
    #[must_use]
    pub const fn created_at(&self) -> AgentRunTimestamp {
        self.created_at
    }

    /// Returns latest event time.
    #[must_use]
    pub const fn updated_at(&self) -> AgentRunTimestamp {
        self.updated_at
    }
}

/// Immutable identity projection used to reconstruct one materialized run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunIdentity {
    id: AgentRunId,
    goal_contract: GoalContractReference,
    task_ledger_revision: TaskLedgerRevision,
    model_profile: Option<ModelProfileReference>,
    budget: AgentRunBudget,
}

impl AgentRunIdentity {
    /// Creates an identity projection from already validated typed fields.
    #[must_use]
    pub const fn new(
        id: AgentRunId,
        goal_contract: GoalContractReference,
        task_ledger_revision: TaskLedgerRevision,
        model_profile: Option<ModelProfileReference>,
    ) -> Self {
        Self {
            id,
            goal_contract,
            task_ledger_revision,
            model_profile,
            budget: AgentRunBudget::DEFAULT,
        }
    }

    /// Creates an identity projection with explicit immutable controller budgets.
    #[must_use]
    pub const fn with_budget(
        id: AgentRunId,
        goal_contract: GoalContractReference,
        task_ledger_revision: TaskLedgerRevision,
        model_profile: Option<ModelProfileReference>,
        budget: AgentRunBudget,
    ) -> Self {
        Self {
            id,
            goal_contract,
            task_ledger_revision,
            model_profile,
            budget,
        }
    }
}

/// Current-state projection used during run reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunMaterializedState {
    state: AgentControllerState,
    last_event_sequence: RunEventSequence,
    current_snapshot_id: SnapshotId,
    usage: AgentRunUsage,
}

impl AgentRunMaterializedState {
    /// Creates an untrusted materialized projection validated with its event tail by storage.
    #[must_use]
    pub const fn new(
        state: AgentControllerState,
        last_event_sequence: RunEventSequence,
        current_snapshot_id: SnapshotId,
    ) -> Self {
        Self {
            state,
            last_event_sequence,
            current_snapshot_id,
            usage: AgentRunUsage::ZERO,
        }
    }

    /// Creates a materialized projection with persisted controller usage.
    #[must_use]
    pub const fn with_usage(
        state: AgentControllerState,
        last_event_sequence: RunEventSequence,
        current_snapshot_id: SnapshotId,
        usage: AgentRunUsage,
    ) -> Self {
        Self {
            state,
            last_event_sequence,
            current_snapshot_id,
            usage,
        }
    }
}

/// Timestamp projection used during run reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunTiming {
    created_at: AgentRunTimestamp,
    updated_at: AgentRunTimestamp,
}

impl AgentRunTiming {
    /// Creates timing metadata; chronological order is validated by reconstruction.
    #[must_use]
    pub const fn new(created_at: AgentRunTimestamp, updated_at: AgentRunTimestamp) -> Self {
        Self {
            created_at,
            updated_at,
        }
    }
}

/// Rejected run transition, event append, or persistence reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunError {
    /// The transition is not part of ADR-0010's finite state machine.
    InvalidStateTransition {
        /// Current materialized state.
        from: AgentControllerState,
        /// Requested next state.
        to: AgentControllerState,
    },
    /// A terminal run cannot accept another event.
    TerminalRun,
    /// Event timestamp preceded the latest durable event.
    TimestampRegressed,
    /// Event sequence reached its persisted maximum.
    SequenceOverflow,
    /// Reconstructed event did not immediately follow the materialized cursor.
    SequenceMismatch {
        /// Required next position.
        expected: RunEventSequence,
        /// Supplied position.
        actual: RunEventSequence,
    },
    /// Event belonged to another run.
    RunMismatch,
    /// Start or transition-only kind was used through the generic append path.
    InvalidEventKind,
    /// Turn usage appeared on another event kind or bypassed the turn-specific append path.
    InvalidTurnCharge,
    /// Durable cumulative usage violated cardinality or integer bounds.
    Usage(AgentRunUsageError),
    /// Sequence one was not the mandatory run-start event.
    InvalidStartEvent,
    /// Persisted payload fields did not match their canonical safe digest.
    PayloadDigestMismatch,
    /// Ledger update did not advance the exact current plan revision by one.
    InvalidLedgerRevision {
        /// Current materialized revision.
        from: TaskLedgerRevision,
        /// Requested next revision.
        to: TaskLedgerRevision,
    },
}

impl fmt::Display for AgentRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStateTransition { .. } => {
                formatter.write_str("agent controller state transition is not allowed")
            }
            Self::TerminalRun => formatter.write_str("terminal agent run cannot append events"),
            Self::TimestampRegressed => formatter.write_str("agent-run timestamp regressed"),
            Self::SequenceOverflow => formatter.write_str("run-event sequence overflowed"),
            Self::SequenceMismatch { .. } => {
                formatter.write_str("run-event sequence is not contiguous")
            }
            Self::RunMismatch => formatter.write_str("run event belongs to another run"),
            Self::InvalidEventKind => formatter.write_str("run event kind is invalid here"),
            Self::InvalidTurnCharge => formatter.write_str("run event turn charge is invalid"),
            Self::Usage(_) => formatter.write_str("agent run usage is invalid"),
            Self::InvalidStartEvent => formatter.write_str("agent run has an invalid start event"),
            Self::PayloadDigestMismatch => {
                formatter.write_str("run-event payload digest does not match its safe fields")
            }
            Self::InvalidLedgerRevision { .. } => {
                formatter.write_str("agent run Task Ledger revision is not contiguous")
            }
        }
    }
}

impl Error for AgentRunError {}

#[cfg(test)]
mod tests {
    use super::{
        AgentControllerState, AgentRun, AgentRunError, AgentRunTimestamp, RunEventCode,
        RunEventKind, RunEventOutcome, RunEventPayload, RunEventRedaction, RunEventRedactionSource,
        RunEventSequence, RunEventSubject,
    };
    use crate::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
        AgentTurnActionClass, AgentTurnCharge, AgentTurnRepairUsage, GoalContract,
        GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelProfileId,
        ModelProfileReference, ModelProfileVersion, ModelTokenCount, RunEventId, SnapshotId,
        SuccessVerification, TaskEvidenceId, TaskId, TaskLedgerRevision,
    };
    use std::error::Error;

    #[test]
    fn run_starts_with_one_snapshot_bound_event() -> Result<(), Box<dyn Error>> {
        let (run, event) = started_run()?;

        assert_eq!(run.state(), AgentControllerState::Intake);
        assert_eq!(run.last_event_sequence(), RunEventSequence::FIRST);
        assert_eq!(event.sequence(), RunEventSequence::FIRST);
        assert_eq!(event.kind(), RunEventKind::RunStarted);
        assert_eq!(event.snapshot_id(), snapshot(1));
        Ok(())
    }

    #[test]
    fn controller_accepts_only_documented_transitions_and_terminal_is_final()
    -> Result<(), Box<dyn Error>> {
        let (mut run, _) = started_run()?;
        assert!(matches!(
            run.transition(
                event_id(2),
                AgentControllerState::Done,
                RunEventPayload::empty(),
                snapshot(1),
                timestamp(2)?,
            ),
            Err(AgentRunError::InvalidStateTransition { .. })
        ));
        for (index, state) in [
            AgentControllerState::Localize,
            AgentControllerState::Plan,
            AgentControllerState::Execute,
            AgentControllerState::Verify,
            AgentControllerState::Done,
        ]
        .into_iter()
        .enumerate()
        {
            run.transition(
                event_id(u8::try_from(index + 2)?),
                state,
                RunEventPayload::new(
                    RunEventCode::ControllerDecision,
                    Some(RunEventOutcome::Succeeded),
                    None,
                ),
                snapshot(1),
                timestamp(u64::try_from(index + 2)?)?,
            )?;
        }
        assert_eq!(run.state(), AgentControllerState::Done);
        assert_eq!(
            run.record(
                event_id(9),
                RunEventKind::Diagnostic,
                RunEventPayload::empty(),
                snapshot(1),
                None,
                timestamp(9)?,
            ),
            Err(AgentRunError::TerminalRun)
        );
        Ok(())
    }

    #[test]
    fn redaction_payload_retains_no_secret_fixture() {
        let secret = "Authorization: Bearer secret-fixture-value";
        let payload = RunEventPayload::new(
            RunEventCode::ToolFailure,
            Some(RunEventOutcome::Failed),
            Some(RunEventRedaction::new(
                RunEventRedactionSource::ToolOutput,
                secret.len() as u64,
                false,
            )),
        );

        assert!(!format!("{payload:?}").contains(secret));
        assert_eq!(
            payload.redaction().map(RunEventRedaction::observed_bytes),
            Some(secret.len() as u64)
        );
        assert!(!format!("{:?}", payload.digest()).contains(secret));
    }

    #[test]
    fn observations_are_contiguous_and_snapshot_bound() -> Result<(), Box<dyn Error>> {
        let (mut run, _) = started_run()?;
        let event = run.record(
            event_id(2),
            RunEventKind::VerificationRecorded,
            RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
            snapshot(2),
            Some(RunEventSubject::Evidence(TaskEvidenceId::from_bytes(
                [7; 32],
            ))),
            timestamp(2)?,
        )?;

        assert_eq!(event.sequence().get(), 2);
        assert_eq!(run.current_snapshot_id(), snapshot(2));
        assert_eq!(run.updated_at(), timestamp(2)?);
        Ok(())
    }

    #[test]
    fn model_turn_charges_are_journal_bound_and_cumulative() -> Result<(), Box<dyn Error>> {
        let (mut run, _) = started_run()?;
        let charge = AgentTurnCharge::new(
            ModelTokenCount::new(400),
            ModelTokenCount::new(25),
            Some(AgentTurnActionClass::Inspect),
            AgentTurnRepairUsage::One,
        );

        assert_eq!(
            run.record(
                event_id(2),
                RunEventKind::ModelInteraction,
                RunEventPayload::empty(),
                snapshot(1),
                None,
                timestamp(2)?,
            ),
            Err(AgentRunError::InvalidEventKind)
        );
        let event = run.record_turn(
            event_id(2),
            RunEventPayload::empty(),
            snapshot(1),
            timestamp(2)?,
            charge,
        )?;

        assert_eq!(event.turn_charge(), Some(charge));
        assert_eq!(run.usage().turn_count(), 1);
        assert_eq!(run.usage().prompt_tokens(), 400);
        assert_eq!(run.usage().output_tokens(), 25);
        assert_eq!(run.usage().action_count(), 1);
        assert_eq!(run.usage().repair_count(), 1);
        Ok(())
    }

    #[test]
    fn ledger_update_advances_only_the_immediate_revision() -> Result<(), Box<dyn Error>> {
        let (mut run, _) = started_run()?;
        assert!(matches!(
            run.record_ledger_update(
                event_id(2),
                TaskLedgerRevision::new(3)?,
                RunEventPayload::empty(),
                snapshot(1),
                timestamp(2)?,
            ),
            Err(AgentRunError::InvalidLedgerRevision { .. })
        ));

        let event = run.record_ledger_update(
            event_id(2),
            TaskLedgerRevision::new(2)?,
            RunEventPayload::empty(),
            snapshot(1),
            timestamp(2)?,
        )?;

        assert_eq!(run.task_ledger_revision().get(), 2);
        assert!(matches!(event.kind(), RunEventKind::LedgerUpdated { .. }));
        Ok(())
    }

    fn started_run() -> Result<(AgentRun, super::RunEvent), Box<dyn Error>> {
        Ok(AgentRun::start(
            AgentRunId::from_bytes([1; 32]),
            goal_contract()?.reference(),
            TaskLedgerRevision::INITIAL,
            ModelProfileReference::new(
                ModelProfileId::from_bytes([8; 32]),
                ModelProfileVersion::V1,
            ),
            snapshot(1),
            event_id(1),
            timestamp(1)?,
        )?)
    }

    fn goal_contract() -> Result<GoalContract, Box<dyn Error>> {
        Ok(GoalContract::initial(
            TaskId::from_bytes([2; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("implement the run journal".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([3; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "events remain contiguous".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run domain tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        ))
    }

    const fn event_id(value: u8) -> RunEventId {
        RunEventId::from_bytes([value; 32])
    }

    const fn snapshot(value: u8) -> SnapshotId {
        SnapshotId::from_bytes([value; 32])
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
