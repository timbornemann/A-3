use super::{AgentSessionId, AgentWorkItemId, TaskId};
use std::error::Error;
use std::fmt;

const MAX_SESSION_TITLE_BYTES: usize = 120;
const MAX_SESSION_ENTRY_BYTES: usize = 256 * 1024;

/// User-selected capability envelope for one Agent work item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentSessionMode {
    /// Evidence-grounded read-only investigation and answer.
    Ask,
    /// Collaborative read-only planning with explicit review.
    Plan,
    /// Deterministic tool execution under central policy.
    Agent,
}

impl AgentSessionMode {
    /// Returns whether a transition preserves the forward-only mode contract.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Ask, Self::Plan) | (Self::Plan, Self::Agent)
        )
    }
}

/// User-facing lifecycle projection for one conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentSessionState {
    /// A new session has no submitted work yet.
    Draft,
    /// One owned job is active.
    Running,
    /// The current work item needs a user answer.
    AwaitingUser,
    /// The latest immutable plan awaits review.
    AwaitingPlanReview,
    /// A policy-controlled action awaits exact approval.
    AwaitingApproval,
    /// The owning runtime is durably paused.
    Paused,
    /// The latest work item completed.
    Completed,
    /// The latest work item failed.
    Failed,
    /// The latest work item was cancelled.
    Cancelled,
    /// The session is hidden from the ordinary recent list.
    Archived,
}

impl AgentSessionState {
    /// Returns whether a new work item may start without reopening the prior task.
    #[must_use]
    pub const fn accepts_follow_up(self) -> bool {
        matches!(
            self,
            Self::Draft | Self::Completed | Self::Failed | Self::Cancelled
        )
    }
}

/// Durable, bounded title shown in the project-local session rail.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AgentSessionTitle(String);

impl AgentSessionTitle {
    /// Validates a concise single-line title.
    pub fn try_from_string(value: String) -> Result<Self, AgentSessionTextError> {
        validate_text(&value, MAX_SESSION_TITLE_BYTES, false)?;
        Ok(Self(value))
    }

    /// Returns the validated title.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AgentSessionTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSessionTitle")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Durable user-visible conversation content.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentSessionText(String);

impl AgentSessionText {
    /// Validates one bounded multiline conversation entry.
    pub fn try_from_string(value: String) -> Result<Self, AgentSessionTextError> {
        validate_text(&value, MAX_SESSION_ENTRY_BYTES, true)?;
        Ok(Self(value))
    }

    /// Returns content only to bounded persistence and presentation boundaries.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AgentSessionText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSessionText")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Invalid durable conversation text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSessionTextError {
    /// Content was empty or exceeded its allocation boundary.
    InvalidLength,
    /// Content contained an unsafe control character or a newline in a single-line field.
    UnsafeCharacter,
}

impl fmt::Display for AgentSessionTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength => "Agent session text has an invalid length",
            Self::UnsafeCharacter => "Agent session text contains an unsafe character",
        })
    }
}

impl Error for AgentSessionTextError {}

fn validate_text(
    value: &str,
    max_bytes: usize,
    allow_newlines: bool,
) -> Result<(), AgentSessionTextError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(AgentSessionTextError::InvalidLength);
    }
    if value.chars().any(|character| {
        (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || (!allow_newlines && matches!(character, '\n' | '\r'))
    }) {
        return Err(AgentSessionTextError::UnsafeCharacter);
    }
    Ok(())
}

/// Monotone optimistic revision of one session projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentSessionRevision(u64);

impl AgentSessionRevision {
    /// Initial durable session revision.
    pub const INITIAL: Self = Self(1);

    /// Reconstructs a positive stored revision.
    pub const fn new(value: u64) -> Result<Self, AgentSessionRevisionError> {
        if value == 0 || value > i64::MAX as u64 {
            Err(AgentSessionRevisionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the stored numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the immediate successor.
    pub const fn next(self) -> Result<Self, AgentSessionRevisionError> {
        match self.0.checked_add(1) {
            Some(value) => Self::new(value),
            None => Err(AgentSessionRevisionError),
        }
    }
}

/// Invalid or exhausted session revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSessionRevisionError;

impl fmt::Display for AgentSessionRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent session revision must be positive and locally representable")
    }
}

impl Error for AgentSessionRevisionError {}

/// Monotone entry sequence inside one conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentSessionSequence(u64);

impl AgentSessionSequence {
    /// First user-visible entry sequence.
    pub const FIRST: Self = Self(1);

    /// Reconstructs a positive stored sequence.
    pub fn new(value: u64) -> Result<Self, AgentSessionRevisionError> {
        AgentSessionRevision::new(value).map(|revision| Self(revision.get()))
    }

    /// Returns the stored numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the immediate successor.
    pub fn next(self) -> Result<Self, AgentSessionRevisionError> {
        AgentSessionRevision::new(self.0)?
            .next()
            .map(|value| Self(value.get()))
    }
}

/// Core-owned timestamp attached to conversation projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentSessionTimestamp(u64);

impl AgentSessionTimestamp {
    /// Reconstructs a locally representable Unix millisecond timestamp.
    pub const fn from_unix_millis(value: u64) -> Result<Self, AgentSessionRevisionError> {
        if value > i64::MAX as u64 {
            Err(AgentSessionRevisionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

/// Persisted user-facing entry classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentSessionEntryKind {
    /// Verbatim accepted user input.
    UserMessage,
    /// Bounded, evidence-linked assistant checkpoint.
    AssistantSummary,
    /// Full immutable plan revision.
    Plan,
    /// Full terminal answer or implementation report.
    FinalReport,
    /// Typed activity label without raw provider or tool payloads.
    Activity,
}

/// One durable work item grouped by a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentWorkItem {
    id: AgentWorkItemId,
    task_id: TaskId,
    mode: AgentSessionMode,
}

impl AgentWorkItem {
    /// Creates an immutable link to one Core task.
    #[must_use]
    pub const fn new(id: AgentWorkItemId, task_id: TaskId, mode: AgentSessionMode) -> Self {
        Self { id, task_id, mode }
    }

    /// Returns the work-item identity.
    #[must_use]
    pub const fn id(self) -> AgentWorkItemId {
        self.id
    }

    /// Returns the authoritative Core task.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    /// Returns the work-item capability envelope.
    #[must_use]
    pub const fn mode(self) -> AgentSessionMode {
        self.mode
    }
}

/// Latest durable projection of one project-bound conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    id: AgentSessionId,
    revision: AgentSessionRevision,
    title: AgentSessionTitle,
    mode: AgentSessionMode,
    state: AgentSessionState,
    created_at: AgentSessionTimestamp,
    updated_at: AgentSessionTimestamp,
    latest_sequence: Option<AgentSessionSequence>,
    active_work_item: Option<AgentWorkItem>,
    current_plan_revision: Option<u32>,
    presentation_deleted: bool,
}

impl AgentSession {
    /// Reconstructs a validated durable projection.
    #[allow(clippy::too_many_arguments)]
    pub const fn from_parts(
        id: AgentSessionId,
        revision: AgentSessionRevision,
        title: AgentSessionTitle,
        mode: AgentSessionMode,
        state: AgentSessionState,
        created_at: AgentSessionTimestamp,
        updated_at: AgentSessionTimestamp,
        latest_sequence: Option<AgentSessionSequence>,
        active_work_item: Option<AgentWorkItem>,
        current_plan_revision: Option<u32>,
        presentation_deleted: bool,
    ) -> Self {
        Self {
            id,
            revision,
            title,
            mode,
            state,
            created_at,
            updated_at,
            latest_sequence,
            active_work_item,
            current_plan_revision,
            presentation_deleted,
        }
    }

    /// Returns the session identity.
    #[must_use]
    pub const fn id(&self) -> AgentSessionId {
        self.id
    }
    /// Returns the optimistic revision.
    #[must_use]
    pub const fn revision(&self) -> AgentSessionRevision {
        self.revision
    }
    /// Returns the visible title.
    #[must_use]
    pub const fn title(&self) -> &AgentSessionTitle {
        &self.title
    }
    /// Returns the current mode.
    #[must_use]
    pub const fn mode(&self) -> AgentSessionMode {
        self.mode
    }
    /// Returns the current projected state.
    #[must_use]
    pub const fn state(&self) -> AgentSessionState {
        self.state
    }
    /// Returns the creation time.
    #[must_use]
    pub const fn created_at(&self) -> AgentSessionTimestamp {
        self.created_at
    }
    /// Returns the latest activity time.
    #[must_use]
    pub const fn updated_at(&self) -> AgentSessionTimestamp {
        self.updated_at
    }
    /// Returns the current conversation tail.
    #[must_use]
    pub const fn latest_sequence(&self) -> Option<AgentSessionSequence> {
        self.latest_sequence
    }
    /// Returns the current work item.
    #[must_use]
    pub const fn active_work_item(&self) -> Option<AgentWorkItem> {
        self.active_work_item
    }
    /// Returns the current immutable plan revision.
    #[must_use]
    pub const fn current_plan_revision(&self) -> Option<u32> {
        self.current_plan_revision
    }
    /// Returns whether user-facing content was removed.
    #[must_use]
    pub const fn presentation_deleted(&self) -> bool {
        self.presentation_deleted
    }
}

/// One immutable user-visible conversation record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionEntry {
    session_id: AgentSessionId,
    sequence: AgentSessionSequence,
    kind: AgentSessionEntryKind,
    text: AgentSessionText,
    created_at: AgentSessionTimestamp,
    work_item_id: Option<AgentWorkItemId>,
    task_id: Option<TaskId>,
    plan_revision: Option<u32>,
}

impl AgentSessionEntry {
    /// Creates one already bounded immutable entry.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        session_id: AgentSessionId,
        sequence: AgentSessionSequence,
        kind: AgentSessionEntryKind,
        text: AgentSessionText,
        created_at: AgentSessionTimestamp,
        work_item_id: Option<AgentWorkItemId>,
        task_id: Option<TaskId>,
        plan_revision: Option<u32>,
    ) -> Self {
        Self {
            session_id,
            sequence,
            kind,
            text,
            created_at,
            work_item_id,
            task_id,
            plan_revision,
        }
    }

    /// Returns the owning session.
    #[must_use]
    pub const fn session_id(&self) -> AgentSessionId {
        self.session_id
    }
    /// Returns the monotone session sequence.
    #[must_use]
    pub const fn sequence(&self) -> AgentSessionSequence {
        self.sequence
    }
    /// Returns the presentation classification.
    #[must_use]
    pub const fn kind(&self) -> AgentSessionEntryKind {
        self.kind
    }
    /// Returns the bounded content.
    #[must_use]
    pub const fn text(&self) -> &AgentSessionText {
        &self.text
    }
    /// Returns the Core-owned timestamp.
    #[must_use]
    pub const fn created_at(&self) -> AgentSessionTimestamp {
        self.created_at
    }
    /// Returns the optional work-item identity.
    #[must_use]
    pub const fn work_item_id(&self) -> Option<AgentWorkItemId> {
        self.work_item_id
    }
    /// Returns the optional authoritative task identity.
    #[must_use]
    pub const fn task_id(&self) -> Option<TaskId> {
        self.task_id
    }
    /// Returns the immutable plan revision when this is a plan artifact.
    #[must_use]
    pub const fn plan_revision(&self) -> Option<u32> {
        self.plan_revision
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentSessionMode, AgentSessionText, AgentSessionTitle};

    #[test]
    fn modes_only_move_forward_inside_one_work_item() {
        assert!(AgentSessionMode::Ask.can_transition_to(AgentSessionMode::Plan));
        assert!(AgentSessionMode::Plan.can_transition_to(AgentSessionMode::Agent));
        assert!(!AgentSessionMode::Agent.can_transition_to(AgentSessionMode::Ask));
    }

    #[test]
    fn title_is_single_line_and_entries_are_bounded_multiline() {
        assert!(AgentSessionTitle::try_from_string("Refactor index".to_owned()).is_ok());
        assert!(AgentSessionTitle::try_from_string("Refactor\nindex".to_owned()).is_err());
        assert!(AgentSessionText::try_from_string("First\nSecond".to_owned()).is_ok());
    }
}
