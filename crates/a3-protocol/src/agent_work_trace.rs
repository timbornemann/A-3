use crate::{
    AgentAskResearchCompletenessV1, AgentAskResearchSourcePreviewResponseV1,
    AgentAskResearchSourcesResponseV1, AgentSessionModeV1, ProtocolVersion,
    QueryAgentAskResearchDetailRequestV1, QueryAgentAskResearchSourcePreviewRequestV1,
    QueryAgentAskResearchSourcesRequestV1, QueryAgentAskResearchTurnsRequestV1,
};
use serde::{Deserialize, Serialize};

/// Existing strict requests reused by the generic work-trace reads.
pub type QueryAgentWorkTraceTurnsRequestV1 = QueryAgentAskResearchTurnsRequestV1;
/// Existing strict exact-turn request reused by the generic work trace.
pub type QueryAgentWorkTraceDetailRequestV1 = QueryAgentAskResearchDetailRequestV1;
/// Existing strict cursor request reused by the generic work trace.
pub type QueryAgentWorkTraceSourcesRequestV1 = QueryAgentAskResearchSourcesRequestV1;
/// Existing strict opaque source request reused by the generic work trace.
pub type QueryAgentWorkTraceSourcePreviewRequestV1 = QueryAgentAskResearchSourcePreviewRequestV1;
/// Source pages use the unchanged metadata-only projection.
pub type AgentWorkTraceSourcesResponseV1 = AgentAskResearchSourcesResponseV1;
/// Source previews use the unchanged secure bounded projection.
pub type AgentWorkTraceSourcePreviewResponseV1 = AgentAskResearchSourcePreviewResponseV1;

/// User-selected depth retained with one work-trace turn.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorkTraceDepthV1 {
    /// Six decisions, twelve reads, and five minutes.
    Standard,
    /// Twelve decisions, twenty-four reads, and fifteen minutes.
    Thorough,
}

/// Public finite research phase shared by Ask, Plan, and Agent preparation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorkTracePhaseV1 {
    /// Bind the request to current durable state.
    Preparing,
    /// Locate relevant indexed areas.
    Locating,
    /// Choose the next bounded step.
    Deciding,
    /// Execute safe read-only actions.
    Reading,
    /// Evaluate new Evidence and remaining gaps.
    Evaluating,
    /// Form the user-facing result.
    AnsweringOrPlanning,
    /// Publish the terminal result and citations.
    Completed,
}

/// Public active or terminal work-trace state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorkTraceStateV1 {
    /// Work is still progressing.
    Running,
    /// A complete bounded result was published.
    Completed,
    /// More budget is required for a reliable result.
    AwaitingContinuation,
    /// The section failed safely.
    Failed,
    /// The user cancelled the section.
    Cancelled,
}

/// Epistemic label for a bounded public work note.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorkTraceFindingKindV1 {
    /// Directly observed current Evidence.
    Observation,
    /// Explicitly unsupported search lead.
    Hypothesis,
    /// Conclusion supported by cited current Evidence.
    Conclusion,
}

/// Public work note; source references are opaque and carry no source content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceNoteV1 {
    /// Current bounded subgoal.
    pub goal: String,
    /// Epistemic label for the finding.
    pub finding_kind: AgentWorkTraceFindingKindV1,
    /// Public compact finding, never hidden reasoning.
    pub finding: String,
    /// Opaque source capabilities supporting the finding.
    pub source_refs: Vec<String>,
    /// Evidence still needed.
    pub gap: String,
    /// Purpose of the next bounded action.
    pub next_step: String,
}

/// One content-free chronological step with an optional public note.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceStepV1 {
    /// Product-facing phase.
    pub phase: AgentWorkTracePhaseV1,
    /// Active or terminal state.
    pub state: AgentWorkTraceStateV1,
    /// Safe human-readable action sentence.
    pub action: String,
    /// Optional bounded search term.
    pub query: Option<String>,
    /// Whether this individual search was complete or limited.
    pub completeness: AgentAskResearchCompletenessV1,
    /// Core-owned decimal timestamp.
    pub occurred_at_unix_millis: String,
    /// Optional public structured work note.
    pub note: Option<AgentWorkTraceNoteV1>,
}

/// Compact newest-first turn summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceTurnV1 {
    /// User-message sequence owning this trace.
    pub user_sequence: String,
    /// Session mode using the shared controller.
    pub mode: AgentSessionModeV1,
    /// Selected fixed research profile.
    pub depth: AgentWorkTraceDepthV1,
    /// Latest product-facing phase.
    pub phase: AgentWorkTracePhaseV1,
    /// Latest active or terminal state.
    pub state: AgentWorkTraceStateV1,
    /// Latest safe action sentence.
    pub action: String,
    /// Core-owned start timestamp.
    pub started_at_unix_millis: String,
    /// Number of current supplied sources.
    pub source_count: u16,
    /// Number of sources used by the result.
    pub cited_source_count: u16,
    /// Whether the trace describes an older index.
    pub stale: bool,
    /// Whether this is a V30 trace projected without new notes.
    pub legacy: bool,
}

/// Full bounded work-trace detail.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceDetailV1 {
    /// User-message sequence owning this trace.
    pub user_sequence: String,
    /// Session mode using the shared controller.
    pub mode: AgentSessionModeV1,
    /// Selected fixed research profile.
    pub depth: AgentWorkTraceDepthV1,
    /// At most sixty-four chronological steps.
    pub steps: Vec<AgentWorkTraceStepV1>,
    /// Number of current supplied sources.
    pub source_count: u16,
    /// Number of sources used by the result.
    pub cited_source_count: u16,
    /// Whether the trace describes an older index.
    pub stale: bool,
    /// Whether the trace predates V31.
    pub legacy: bool,
}

/// Turn-list availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentWorkTraceTurnsResultV1 {
    /// No project is active.
    NoProject,
    /// The visible session does not exist in this project.
    NotFound,
    /// A bounded newest-first turn page is available.
    Available {
        /// At most thirty-two summaries.
        turns: Vec<AgentWorkTraceTurnV1>,
    },
}

/// Versioned generic turn-list response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceTurnsResponseV1 {
    /// Exact desktop protocol version.
    pub protocol_version: ProtocolVersion,
    /// Bounded read result.
    pub result: AgentWorkTraceTurnsResultV1,
}

/// Detail availability including V30 legacy turns.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentWorkTraceDetailResultV1 {
    /// No project is active.
    NoProject,
    /// The visible session or requested turn is absent.
    NotFound,
    /// This historical turn predates trace capture.
    NotRecorded,
    /// The exact bounded trace is available.
    Available {
        /// Current or historical trace detail.
        detail: AgentWorkTraceDetailV1,
    },
}

/// Versioned generic detail response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceDetailResponseV1 {
    /// Exact desktop protocol version.
    pub protocol_version: ProtocolVersion,
    /// Bounded read result.
    pub result: AgentWorkTraceDetailResultV1,
}
