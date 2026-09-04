use crate::{AgentDiagramSummaryV1, ProtocolVersion};
use serde::{Deserialize, Serialize};
use std::fmt;

/// User-selected capability envelope for one session work item.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSessionModeV1 {
    /// Read-only investigation and answer.
    Ask,
    /// Collaborative read-only planning.
    Plan,
    /// Policy-controlled repository execution.
    Agent,
}

/// User-selected finite research depth for one message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentResearchDepthV1 {
    /// Up to six model decisions and twelve reads.
    Standard,
    /// Up to twelve model decisions and twenty-four reads.
    Thorough,
}

/// V3 selection: ordinary messages choose a depth, commands use the Core-owned profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentResearchDepthSelectionV1 {
    /// Explicit ordinary Standard research.
    Standard,
    /// Explicit ordinary Thorough research.
    Thorough,
    /// Depth is resolved exclusively from a validated slash command.
    Command,
}

/// User-facing lifecycle projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSessionStateV1 {
    /// No task has been submitted.
    Draft,
    /// One job is running.
    Running,
    /// The user must answer a blocking question.
    AwaitingUser,
    /// A plan awaits review.
    AwaitingPlanReview,
    /// An exact policy approval is required.
    AwaitingApproval,
    /// The current job is paused.
    Paused,
    /// The latest work item completed.
    Completed,
    /// The latest work item failed.
    Failed,
    /// The latest work item was cancelled.
    Cancelled,
    /// The session is archived.
    Archived,
}

/// Durable conversation-entry classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSessionEntryKindV1 {
    /// Verbatim accepted user input.
    UserMessage,
    /// Evidence-grounded assistant checkpoint.
    AssistantSummary,
    /// Full immutable plan revision.
    Plan,
    /// Full final answer or implementation report.
    FinalReport,
    /// Typed activity without raw payload content.
    Activity,
}

/// Strict project-local session-list request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentSessionsRequestV1 {
    protocol_version: ProtocolVersion,
    search: Option<String>,
    include_archived: bool,
    before_updated_at_unix_millis: Option<String>,
    limit: u16,
}

impl QueryAgentSessionsRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns optional title search text.
    #[must_use]
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }
    /// Returns whether archived sessions are requested.
    #[must_use]
    pub const fn include_archived(&self) -> bool {
        self.include_archived
    }
    /// Returns the optional decimal time cursor.
    #[must_use]
    pub fn before_updated_at_unix_millis(&self) -> Option<&str> {
        self.before_updated_at_unix_millis.as_deref()
    }
    /// Returns the requested page size.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

/// Strict request for one bounded conversation page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentSessionRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    before_sequence: Option<String>,
    limit: u16,
}

impl QueryAgentSessionRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque Core-issued session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the optional exclusive sequence cursor.
    #[must_use]
    pub fn before_sequence(&self) -> Option<&str> {
        self.before_sequence.as_deref()
    }
    /// Returns the requested entry count.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

/// Reserved Core-issued indexed context reference.
///
/// V1 requires the surrounding list to be empty until a dedicated Core resolver exists.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentContextReferenceV1 {
    reference_id: String,
}

impl AgentContextReferenceV1 {
    /// Returns the opaque current-index reference identity.
    #[must_use]
    pub fn reference_id(&self) -> &str {
        &self.reference_id
    }
}

/// Creates a session or appends a message to one current revision.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitAgentMessageRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: Option<String>,
    expected_session_revision: Option<String>,
    start_mode: Option<AgentSessionModeV1>,
    message: String,
    context_references: Vec<AgentContextReferenceV1>,
}

impl fmt::Debug for SubmitAgentMessageRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmitAgentMessageRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("has_session_id", &self.session_id.is_some())
            .field("message_bytes", &self.message.len())
            .field("context_references", &self.context_references.len())
            .finish_non_exhaustive()
    }
}

impl SubmitAgentMessageRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns an existing session identity, if this is a follow-up.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    /// Returns the visible optimistic revision for a follow-up.
    #[must_use]
    pub fn expected_session_revision(&self) -> Option<&str> {
        self.expected_session_revision.as_deref()
    }
    /// Returns the required starting mode for a new session.
    #[must_use]
    pub const fn start_mode(&self) -> Option<AgentSessionModeV1> {
        self.start_mode
    }
    /// Returns the user-authored message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Returns reserved Core-issued context references; V1 accepts only an empty list.
    #[must_use]
    pub fn context_references(&self) -> &[AgentContextReferenceV1] {
        &self.context_references
    }
}

/// V2 message submission with an explicit per-message research depth.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitAgentMessageRequestV2 {
    protocol_version: ProtocolVersion,
    session_id: Option<String>,
    expected_session_revision: Option<String>,
    start_mode: Option<AgentSessionModeV1>,
    research_depth: AgentResearchDepthV1,
    message: String,
    context_references: Vec<AgentContextReferenceV1>,
}

impl fmt::Debug for SubmitAgentMessageRequestV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmitAgentMessageRequestV2")
            .field("protocol_version", &self.protocol_version)
            .field("has_session_id", &self.session_id.is_some())
            .field("research_depth", &self.research_depth)
            .field("message_bytes", &self.message.len())
            .field("context_references", &self.context_references.len())
            .finish_non_exhaustive()
    }
}

impl SubmitAgentMessageRequestV2 {
    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    #[must_use]
    /// Returns the optional existing session capability.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    #[must_use]
    /// Returns the optional optimistic session revision.
    pub fn expected_session_revision(&self) -> Option<&str> {
        self.expected_session_revision.as_deref()
    }
    #[must_use]
    /// Returns the mode required only when creating a session.
    pub const fn start_mode(&self) -> Option<AgentSessionModeV1> {
        self.start_mode
    }
    #[must_use]
    /// Returns the user-selected research depth.
    pub const fn research_depth(&self) -> AgentResearchDepthV1 {
        self.research_depth
    }
    #[must_use]
    /// Returns the bounded user-authored message.
    pub fn message(&self) -> &str {
        &self.message
    }
    #[must_use]
    /// Returns reserved opaque context capabilities.
    pub fn context_references(&self) -> &[AgentContextReferenceV1] {
        &self.context_references
    }
}

/// V3 message submission supporting Core-resolved built-in slash commands.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitAgentMessageRequestV3 {
    protocol_version: ProtocolVersion,
    session_id: Option<String>,
    expected_session_revision: Option<String>,
    start_mode: Option<AgentSessionModeV1>,
    research_depth: AgentResearchDepthSelectionV1,
    message: String,
    context_references: Vec<AgentContextReferenceV1>,
}

/// Creates or queues one message with an explicit next-work-item capability envelope.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitAgentMessageRequestV4 {
    protocol_version: ProtocolVersion,
    session_id: Option<String>,
    expected_session_revision: Option<String>,
    target_mode: AgentSessionModeV1,
    research_depth: AgentResearchDepthSelectionV1,
    message: String,
    context_references: Vec<AgentContextReferenceV1>,
}

impl fmt::Debug for SubmitAgentMessageRequestV4 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmitAgentMessageRequestV4")
            .field("protocol_version", &self.protocol_version)
            .field("has_session_id", &self.session_id.is_some())
            .field("target_mode", &self.target_mode)
            .field("research_depth", &self.research_depth)
            .field("message_bytes", &self.message.len())
            .field("context_references", &self.context_references.len())
            .finish_non_exhaustive()
    }
}

impl SubmitAgentMessageRequestV4 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the optional existing session identity.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    /// Returns the required revision for an existing session.
    #[must_use]
    pub fn expected_session_revision(&self) -> Option<&str> {
        self.expected_session_revision.as_deref()
    }
    /// Returns the capability envelope for the next independent work item.
    #[must_use]
    pub const fn target_mode(&self) -> AgentSessionModeV1 {
        self.target_mode
    }
    /// Returns the requested or command-owned research depth.
    #[must_use]
    pub const fn research_depth(&self) -> AgentResearchDepthSelectionV1 {
        self.research_depth
    }
    /// Returns the bounded user message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Returns the explicitly selected bounded context references.
    #[must_use]
    pub fn context_references(&self) -> &[AgentContextReferenceV1] {
        &self.context_references
    }
}

impl fmt::Debug for SubmitAgentMessageRequestV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmitAgentMessageRequestV3")
            .field("protocol_version", &self.protocol_version)
            .field("has_session_id", &self.session_id.is_some())
            .field("research_depth", &self.research_depth)
            .field("message_bytes", &self.message.len())
            .field("context_references", &self.context_references.len())
            .finish_non_exhaustive()
    }
}

impl SubmitAgentMessageRequestV3 {
    #[must_use]
    /// Returns the protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    #[must_use]
    /// Returns the optional existing session identity.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    #[must_use]
    /// Returns the optimistic session revision for an existing session.
    pub fn expected_session_revision(&self) -> Option<&str> {
        self.expected_session_revision.as_deref()
    }
    #[must_use]
    /// Returns the required mode only when creating a session.
    pub const fn start_mode(&self) -> Option<AgentSessionModeV1> {
        self.start_mode
    }
    #[must_use]
    /// Returns the ordinary or command-owned depth selection.
    pub const fn research_depth(&self) -> AgentResearchDepthSelectionV1 {
        self.research_depth
    }
    #[must_use]
    /// Returns the bounded visible message.
    pub fn message(&self) -> &str {
        &self.message
    }
    #[must_use]
    /// Returns reserved opaque context capabilities.
    pub fn context_references(&self) -> &[AgentContextReferenceV1] {
        &self.context_references
    }
}

/// Requests continuation of the newest continuation-ready research section.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContinueAgentResearchRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    expected_session_revision: String,
    research_depth: AgentResearchDepthV1,
}

impl ContinueAgentResearchRequestV1 {
    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    #[must_use]
    /// Returns the visible session capability.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    #[must_use]
    /// Returns the optimistic visible session revision.
    pub fn expected_session_revision(&self) -> &str {
        &self.expected_session_revision
    }
    #[must_use]
    /// Returns the new fixed research depth.
    pub const fn research_depth(&self) -> AgentResearchDepthV1 {
        self.research_depth
    }
}

/// Closed session controls; no action embeds a path, command, or policy decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentSessionControlActionV1 {
    /// Pause the active work item.
    Pause,
    /// Resume the active work item.
    Resume,
    /// Cancel the active work item.
    Cancel,
    /// Move an Ask work item into collaborative planning.
    SwitchToPlan,
    /// Approve and execute the exact current plan revision.
    ImplementPlan {
        /// Exact immutable plan revision visible during approval.
        plan_revision: u32,
    },
    /// Replace the visible session title.
    Rename {
        /// New bounded visible title.
        title: String,
    },
    /// Hide the session from the ordinary recent list.
    Archive,
    /// Restore an archived session.
    Unarchive,
    /// Remove only user-facing conversation content.
    DeletePresentation,
}

/// Strict optimistic session-control request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlAgentSessionRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    expected_session_revision: String,
    action: AgentSessionControlActionV1,
}

impl ControlAgentSessionRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the Core-issued session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the visible optimistic revision.
    #[must_use]
    pub fn expected_session_revision(&self) -> &str {
        &self.expected_session_revision
    }
    /// Returns the selected closed action.
    #[must_use]
    pub const fn action(&self) -> &AgentSessionControlActionV1 {
        &self.action
    }
}

/// Bounded session summary shown in the rail.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionSummaryV1 {
    session_id: String,
    revision: String,
    title: String,
    mode: AgentSessionModeV1,
    state: AgentSessionStateV1,
    updated_at_unix_millis: String,
    current_plan_revision: Option<u32>,
}

impl AgentSessionSummaryV1 {
    /// Creates one Core-validated session summary.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        session_id: String,
        revision: String,
        title: String,
        mode: AgentSessionModeV1,
        state: AgentSessionStateV1,
        updated_at_unix_millis: String,
        current_plan_revision: Option<u32>,
    ) -> Self {
        Self {
            session_id,
            revision,
            title,
            mode,
            state,
            updated_at_unix_millis,
            current_plan_revision,
        }
    }

    /// Returns the capability mode of the currently displayed work item.
    #[must_use]
    pub const fn mode(&self) -> AgentSessionModeV1 {
        self.mode
    }
}

/// One bounded durable conversation record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionEntryV1 {
    sequence: String,
    kind: AgentSessionEntryKindV1,
    text: String,
    created_at_unix_millis: String,
    plan_revision: Option<u32>,
}

impl AgentSessionEntryV1 {
    /// Creates a bounded presentation entry.
    #[must_use]
    pub const fn new(
        sequence: String,
        kind: AgentSessionEntryKindV1,
        text: String,
        created_at_unix_millis: String,
        plan_revision: Option<u32>,
    ) -> Self {
        Self {
            sequence,
            kind,
            text,
            created_at_unix_millis,
            plan_revision,
        }
    }
}

/// Session-list result for the active Core-owned project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum AgentSessionsResultV1 {
    /// No project is active.
    NoProject,
    /// A bounded project-local page is available.
    Available {
        /// Session summaries newest first.
        sessions: Vec<AgentSessionSummaryV1>,
        /// Exclusive updated-time cursor for the next page.
        next_cursor: Option<String>,
    },
}

/// Versioned session-list response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionsResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentSessionsResultV1,
}

impl AgentSessionsResponseV1 {
    /// Reports that no project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentSessionsResultV1::NoProject,
        }
    }
    /// Returns one bounded available page.
    #[must_use]
    pub const fn available(
        sessions: Vec<AgentSessionSummaryV1>,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentSessionsResultV1::Available {
                sessions,
                next_cursor,
            },
        }
    }
}

/// Complete bounded session presentation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionV1 {
    summary: AgentSessionSummaryV1,
    active_task_id: Option<String>,
    entries: Vec<AgentSessionEntryV1>,
    has_older_entries: bool,
}

impl AgentSessionV1 {
    /// Creates one Core-bounded conversation view.
    #[must_use]
    pub const fn new(
        summary: AgentSessionSummaryV1,
        active_task_id: Option<String>,
        entries: Vec<AgentSessionEntryV1>,
        has_older_entries: bool,
    ) -> Self {
        Self {
            summary,
            active_task_id,
            entries,
            has_older_entries,
        }
    }

    /// Returns the bounded session summary.
    #[must_use]
    pub const fn summary(&self) -> &AgentSessionSummaryV1 {
        &self.summary
    }
}

/// User-facing chips for one persisted built-in command invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionCommandChipsV1 {
    /// Immutable built-in catalog version.
    pub catalog_version: u16,
    /// Primary command including its leading slash.
    pub primary: String,
    /// Ordered specialist lenses including their leading slashes.
    pub lenses: Vec<String>,
    /// Core-owned effective research depth.
    pub depth: AgentResearchDepthV1,
}

/// V32 presentation metadata attached to one visible user entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionEntryAugmentationV1 {
    /// Sequence of the owning user message.
    pub user_sequence: String,
    /// Persisted command chips, when this was a validated slash invocation.
    pub command: Option<AgentSessionCommandChipsV1>,
    /// At most three diagrams atomically completed with this turn.
    pub diagrams: Vec<AgentDiagramSummaryV1>,
}

/// V2 session projection combining the compatible page with V32 presentation metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionV2 {
    /// Complete bounded V1 session page.
    pub session: AgentSessionV1,
    /// Metadata for only the user entries contained in this page.
    pub entry_augmentations: Vec<AgentSessionEntryAugmentationV1>,
}

/// One bounded durable FIFO entry visible above the composer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentQueuedMessageSummaryV1 {
    /// Opaque session-bound reference accepted by the remove action.
    pub queue_reference: String,
    /// Current one-based FIFO position.
    pub position: u16,
    /// Capability envelope selected for the queued work item.
    pub target_mode: AgentSessionModeV1,
    /// Bounded single-line message preview.
    pub preview: String,
    /// Decimal enqueue timestamp.
    pub enqueued_at_unix_millis: String,
}

/// Core-derived next-message choices. Choosing a mode never interrupts current work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionModeOptionV1 {
    /// Candidate mode.
    pub mode: AgentSessionModeV1,
    /// Whether the mode can be selected for the next message.
    pub selectable: bool,
    /// Whether this choice must stop at a fresh plan review.
    pub requires_plan_review: bool,
}

/// V33 session projection with the bounded durable queue and mode choices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionV3 {
    /// Compatible V2 conversation and artifact projection.
    pub projection: AgentSessionV2,
    /// Core-derived choices for the next message.
    pub mode_options: Vec<AgentSessionModeOptionV1>,
    /// Decimal append-only queue revision.
    pub queue_revision: String,
    /// Whether explicit continuation is required.
    pub queue_paused: bool,
    /// At most sixteen waiting messages in FIFO order.
    pub queued_messages: Vec<AgentQueuedMessageSummaryV1>,
}

/// Result of loading the V33 session projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentSessionResultV3 {
    /// No project is active.
    NoProject,
    /// The requested session is unavailable in this project.
    NotFound,
    /// The bounded current projection is available.
    Available {
        /// Current V33 projection.
        projection: Box<AgentSessionV3>,
    },
}

/// Versioned V33 session response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionResponseV3 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed query result.
    pub result: AgentSessionResultV3,
}

/// Closed outcome of accepting one V4 message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentMessageSubmissionOutcomeV1 {
    /// The message started immediately.
    Started,
    /// The message was durably queued.
    Queued,
    /// Agent intent is being prepared as a fresh Plan requiring review.
    RequiresPlanReview,
}

/// V4 submit response includes the same projection used by subsequent polling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum SubmitAgentMessageResultV4 {
    /// No project is active.
    NoProject,
    /// The message was accepted and the current projection is available.
    Available {
        /// Closed acceptance outcome.
        outcome: AgentMessageSubmissionOutcomeV1,
        /// Current V33 projection.
        projection: Box<AgentSessionV3>,
    },
}

/// Versioned V4 message-submission response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubmitAgentMessageResponseV4 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed submission result.
    pub result: SubmitAgentMessageResultV4,
}

/// Queue controls never contain message text, paths, task identities, or process data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentSessionQueueControlActionV1 {
    /// Removes one not-yet-started message.
    Remove {
        /// Opaque session-bound queue reference.
        queue_reference: String,
    },
    /// Explicitly permits automatic queue dispatch again.
    Resume,
}

/// Versioned request for one narrow queue presentation action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlAgentSessionQueueRequestV1 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Opaque session identity.
    pub session_id: String,
    /// Decimal optimistic queue revision.
    pub expected_queue_revision: String,
    /// Narrow requested action.
    pub action: AgentSessionQueueControlActionV1,
}

/// Closed plan-start outcomes distinguish normal races from stale anchors.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentPlanStartOutcomeV1 {
    /// Materialization and execution started.
    Started,
    /// The exact reviewed plan waits for the prior conversation release.
    Queued,
    /// A different plan revision is now current.
    PlanChanged,
    /// The reviewed plan describes an older project index.
    IndexChanged,
    /// Required local execution capability is unavailable.
    Unavailable,
}

/// Exact reviewed-plan start request. It carries no task, run, path, or process authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlAgentSessionRequestV2 {
    protocol_version: ProtocolVersion,
    session_id: String,
    expected_session_revision: String,
    plan_revision: u32,
}

impl ControlAgentSessionRequestV2 {
    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    #[must_use]
    /// Returns the opaque session capability.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    /// Returns the exact visible optimistic session revision.
    pub fn expected_session_revision(&self) -> &str {
        &self.expected_session_revision
    }

    #[must_use]
    /// Returns the exact immutable plan revision approved by the user.
    pub const fn plan_revision(&self) -> u32 {
        self.plan_revision
    }
}

/// Closed outcome of one exact reviewed-plan start request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentPlanStartResultV1 {
    /// No project is active.
    NoProject,
    /// The selected session is unavailable in this project.
    NotFound,
    /// The current projection and semantic start outcome are available.
    Available {
        /// Closed semantic result of the start attempt.
        outcome: AgentPlanStartOutcomeV1,
        /// Current V33 session projection after the attempt.
        projection: Box<AgentSessionV3>,
    },
}

/// Versioned response for `control_agent_session_v2`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentPlanStartResponseV1 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed project/session/start outcome.
    pub result: AgentPlanStartResultV1,
}

/// Result of reading one V2 session projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentSessionResultV2 {
    /// No project is active.
    NoProject,
    /// The selected session no longer exists in this project.
    NotFound,
    /// The current enriched projection is available.
    Available {
        /// Current bounded session and V32 presentation metadata.
        projection: AgentSessionV2,
    },
}

/// Versioned enriched session response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionResponseV2 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed read result.
    pub result: AgentSessionResultV2,
}

/// Result of reading or mutating one session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentSessionResultV1 {
    /// No project is active.
    NoProject,
    /// The selected session no longer exists in this project.
    NotFound,
    /// The current session projection is available.
    Available {
        /// Current bounded session projection.
        session: AgentSessionV1,
    },
}

/// Versioned single-session response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSessionResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentSessionResultV1,
}

impl AgentSessionResponseV1 {
    /// Reports that no project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentSessionResultV1::NoProject,
        }
    }
    /// Reports a missing session.
    #[must_use]
    pub const fn not_found() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentSessionResultV1::NotFound,
        }
    }
    /// Returns the current bounded session.
    #[must_use]
    pub const fn available(session: AgentSessionV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentSessionResultV1::Available { session },
        }
    }
}

/// Strict request for the global Agent workspace layout snapshot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryUiPreferencesRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryUiPreferencesRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Strict optimistic update of non-sensitive Agent layout preferences.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateAgentWorkspaceLayoutRequestV1 {
    protocol_version: ProtocolVersion,
    expected_revision: String,
    session_rail_width: u16,
    inspector_width: u16,
    session_rail_collapsed: bool,
    inspector_collapsed: bool,
}

impl UpdateAgentWorkspaceLayoutRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the expected decimal revision.
    #[must_use]
    pub fn expected_revision(&self) -> &str {
        &self.expected_revision
    }
    /// Returns the preferred rail width.
    #[must_use]
    pub const fn session_rail_width(&self) -> u16 {
        self.session_rail_width
    }
    /// Returns the preferred inspector width.
    #[must_use]
    pub const fn inspector_width(&self) -> u16 {
        self.inspector_width
    }
    /// Returns the wide-layout rail state.
    #[must_use]
    pub const fn session_rail_collapsed(&self) -> bool {
        self.session_rail_collapsed
    }
    /// Returns the wide-layout inspector state.
    #[must_use]
    pub const fn inspector_collapsed(&self) -> bool {
        self.inspector_collapsed
    }
}

/// Versioned content-free Agent layout preference projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UiPreferencesResponseV1 {
    protocol_version: ProtocolVersion,
    revision: String,
    session_rail_width: u16,
    inspector_width: u16,
    session_rail_collapsed: bool,
    inspector_collapsed: bool,
}

impl UiPreferencesResponseV1 {
    /// Creates one current preference response.
    #[must_use]
    pub const fn new(
        revision: String,
        session_rail_width: u16,
        inspector_width: u16,
        session_rail_collapsed: bool,
        inspector_collapsed: bool,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            revision,
            session_rail_width,
            inspector_width,
            session_rail_collapsed,
            inspector_collapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentResearchDepthSelectionV1, AgentSessionModeV1, AgentSessionsResultV1,
        ControlAgentSessionRequestV2, SubmitAgentMessageRequestV1, SubmitAgentMessageRequestV4,
    };
    use crate::ProtocolVersion;

    #[test]
    fn submit_message_rejects_unknown_authority_fields() {
        let value = serde_json::json!({
            "protocolVersion": ProtocolVersion::CURRENT.get(),
            "sessionId": null,
            "expectedSessionRevision": null,
            "startMode": AgentSessionModeV1::Agent,
            "message": "Implement the task",
            "contextReferences": [],
            "worktreePath": "C:/escape"
        });
        assert!(serde_json::from_value::<SubmitAgentMessageRequestV1>(value).is_err());
    }

    #[test]
    fn session_page_cursor_uses_the_camel_case_wire_name() -> Result<(), serde_json::Error> {
        let encoded = serde_json::to_value(AgentSessionsResultV1::Available {
            sessions: Vec::new(),
            next_cursor: Some("a".repeat(64)),
        })?;
        let object = encoded.as_object().ok_or_else(|| {
            serde_json::Error::io(std::io::Error::other("session result is not an object"))
        })?;

        assert_eq!(
            object.get("nextCursor"),
            Some(&serde_json::json!("a".repeat(64)))
        );
        assert!(!object.contains_key("next_cursor"));
        Ok(())
    }

    #[test]
    fn v4_submit_requires_the_explicit_target_mode_and_rejects_legacy_authority_fields()
    -> Result<(), serde_json::Error> {
        let valid = serde_json::json!({
            "protocolVersion": ProtocolVersion::CURRENT.get(),
            "sessionId": null,
            "expectedSessionRevision": null,
            "targetMode": AgentSessionModeV1::Plan,
            "researchDepth": AgentResearchDepthSelectionV1::Thorough,
            "message": "Untersuche den Plan",
            "contextReferences": []
        });
        let request = serde_json::from_value::<SubmitAgentMessageRequestV4>(valid)?;
        assert_eq!(request.target_mode(), AgentSessionModeV1::Plan);

        let invalid = serde_json::json!({
            "protocolVersion": ProtocolVersion::CURRENT.get(),
            "sessionId": null,
            "expectedSessionRevision": null,
            "targetMode": AgentSessionModeV1::Agent,
            "startMode": AgentSessionModeV1::Ask,
            "researchDepth": AgentResearchDepthSelectionV1::Standard,
            "message": "Ändere das Projekt",
            "contextReferences": []
        });
        assert!(serde_json::from_value::<SubmitAgentMessageRequestV4>(invalid).is_err());
        Ok(())
    }

    #[test]
    fn v2_plan_start_accepts_only_an_exact_visible_plan_reference() -> Result<(), serde_json::Error>
    {
        let valid = serde_json::json!({
            "protocolVersion": ProtocolVersion::CURRENT.get(),
            "sessionId": "a".repeat(64),
            "expectedSessionRevision": "4",
            "planRevision": 2
        });
        let request = serde_json::from_value::<ControlAgentSessionRequestV2>(valid)?;
        assert_eq!(request.plan_revision(), 2);

        let invalid = serde_json::json!({
            "protocolVersion": ProtocolVersion::CURRENT.get(),
            "sessionId": "a".repeat(64),
            "expectedSessionRevision": "4",
            "planRevision": 2,
            "taskId": "b".repeat(64)
        });
        assert!(serde_json::from_value::<ControlAgentSessionRequestV2>(invalid).is_err());
        Ok(())
    }
}
