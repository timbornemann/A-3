use crate::{
    AgentAskResearchCompletenessV1, AgentAskResearchSelectionReasonV1,
    AgentAskResearchSourceKindV1, AgentAskResearchSourcePreviewResponseV1,
    AgentAskResearchSourcesResponseV1, AgentSessionModeV1, ProtocolVersion,
    QueryAgentAskResearchDetailRequestV1, QueryAgentAskResearchSourcePreviewRequestV1,
    QueryAgentAskResearchSourcesRequestV1, QueryAgentAskResearchTurnsRequestV1,
};
use serde::{Deserialize, Serialize};

/// Existing strict requests reused by the generic work-trace reads.
pub type QueryAgentWorkTraceTurnsRequestV1 = QueryAgentAskResearchTurnsRequestV1;
/// Existing strict exact-turn request reused by the generic work trace.
pub type QueryAgentWorkTraceDetailRequestV1 = QueryAgentAskResearchDetailRequestV1;
/// V2 keeps the exact-turn request pathless while recognizing the expanded closed action schema.
pub type QueryAgentWorkTraceDetailRequestV2 = QueryAgentAskResearchDetailRequestV1;
/// Existing strict cursor request reused by the generic work trace.
pub type QueryAgentWorkTraceSourcesRequestV1 = QueryAgentAskResearchSourcesRequestV1;
/// Existing strict opaque source request reused by the generic work trace.
pub type QueryAgentWorkTraceSourcePreviewRequestV1 = QueryAgentAskResearchSourcePreviewRequestV1;
/// Source pages use the unchanged metadata-only projection.
pub type AgentWorkTraceSourcesResponseV1 = AgentAskResearchSourcesResponseV1;
/// Source previews use the unchanged secure bounded projection.
pub type AgentWorkTraceSourcePreviewResponseV1 = AgentAskResearchSourcePreviewResponseV1;

/// Requests one coherent work-trace projection for an exact visible turn.
pub type QueryAgentWorkTraceProjectionRequestV1 = QueryAgentAskResearchDetailRequestV1;

/// Requests another page belonging to one exact coherent projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentWorkTraceSourcesRequestV2 {
    protocol_version: ProtocolVersion,
    session_id: String,
    user_sequence: String,
    projection_ref: String,
    cursor: Option<String>,
}

impl QueryAgentWorkTraceSourcesRequestV2 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the visible session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the owning user-message sequence.
    #[must_use]
    pub fn user_sequence(&self) -> &str {
        &self.user_sequence
    }
    /// Returns the opaque projection binding.
    #[must_use]
    pub fn projection_ref(&self) -> &str {
        &self.projection_ref
    }
    /// Returns the opaque page cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

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
    /// V36 Core checklist; absent for historical turns. Never synthesized from timeline notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research_work: Option<crate::ResearchWorkV1>,
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

/// V2 keeps the safe bounded projection; expanded actions are rendered as Core-owned sentences.
pub type AgentWorkTraceDetailResponseV2 = AgentWorkTraceDetailResponseV1;

/// One source in a coherent projection. `reference_label` is the public turn-local S label.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceSourceV2 {
    /// Opaque capability accepted by the safe preview endpoint.
    pub source_ref: String,
    /// Stable public turn-local label such as `S1`.
    pub reference_label: String,
    /// Repository-relative display path.
    pub path: String,
    /// Optional one-based first line.
    pub start_line: Option<u32>,
    /// Optional one-based last line.
    pub end_line: Option<u32>,
    /// Optional indexed symbol name.
    pub symbol: Option<String>,
    /// Closed source classification.
    pub kind: AgentAskResearchSourceKindV1,
    /// Core-derived selection reason.
    pub reason: AgentAskResearchSelectionReasonV1,
    /// Whether the persisted answer cited this source.
    pub used_for_answer: bool,
}

/// Availability of an atomically presented detail and first source page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum AgentWorkTraceProjectionResultV1 {
    /// No project is active.
    NoProject,
    /// The session or turn does not exist.
    NotFound,
    /// The historical turn predates research tracing.
    NotRecorded,
    /// The trace or index changed while the projection was assembled; callers retain the last
    /// complete projection and retry.
    Updating,
    /// One complete coherent projection is available.
    Available {
        /// Bounded event and count detail.
        detail: AgentWorkTraceDetailV1,
        /// Opaque binding shared by continuation pages.
        projection_ref: String,
        /// First source page with at most fifty entries.
        sources: Vec<AgentWorkTraceSourceV2>,
        /// Opaque continuation cursor.
        next_cursor: Option<String>,
    },
}

/// Versioned coherent work-trace projection response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceProjectionResponseV1 {
    /// Exact desktop protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed projection availability.
    pub result: AgentWorkTraceProjectionResultV1,
}

/// Availability of a projection-bound source continuation page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum AgentWorkTraceSourcesResultV2 {
    /// No project is active.
    NoProject,
    /// The session or turn does not exist.
    NotFound,
    /// The requested projection no longer describes the current trace revision.
    ProjectionChanged,
    /// One projection-bound page is available.
    Available {
        /// At most fifty source entries.
        sources: Vec<AgentWorkTraceSourceV2>,
        /// Opaque continuation cursor.
        next_cursor: Option<String>,
    },
}

/// Versioned projection-bound source continuation response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentWorkTraceSourcesResponseV2 {
    /// Exact desktop protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed page availability.
    pub result: AgentWorkTraceSourcesResultV2,
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn projection_page_request_rejects_paths_and_internal_anchors() {
        let invalid = r#"{"protocolVersion":1,"sessionId":"00","userSequence":"1","projectionRef":"11","cursor":null,"path":"src/lib.rs"}"#;
        assert!(serde_json::from_str::<QueryAgentWorkTraceSourcesRequestV2>(invalid).is_err());
    }

    #[test]
    fn public_source_label_serializes_without_an_ordinal_or_internal_id()
    -> Result<(), serde_json::Error> {
        let source = AgentWorkTraceSourceV2 {
            source_ref: "a".repeat(64),
            reference_label: "S12".to_owned(),
            path: "src/lib.rs".to_owned(),
            start_line: Some(18),
            end_line: Some(20),
            symbol: None,
            kind: AgentAskResearchSourceKindV1::File,
            reason: AgentAskResearchSelectionReasonV1::SourceText,
            used_for_answer: true,
        };
        let encoded = serde_json::to_string(&source)?;
        assert!(encoded.contains("\"referenceLabel\":\"S12\""));
        assert!(!encoded.contains("ordinal"));
        assert!(!encoded.contains("sourceId"));
        Ok(())
    }

    #[test]
    fn projection_and_source_page_bindings_use_camel_case_wire_names()
    -> Result<(), serde_json::Error> {
        let detail = AgentWorkTraceDetailV1 {
            research_work: None,
            user_sequence: "1".to_owned(),
            mode: AgentSessionModeV1::Ask,
            depth: AgentWorkTraceDepthV1::Standard,
            steps: Vec::new(),
            source_count: 0,
            cited_source_count: 0,
            stale: false,
            legacy: false,
        };
        let projection = serde_json::to_value(AgentWorkTraceProjectionResultV1::Available {
            detail,
            projection_ref: "a".repeat(64),
            sources: Vec::new(),
            next_cursor: Some("b".repeat(64)),
        })?;
        let page = serde_json::to_value(AgentWorkTraceSourcesResultV2::Available {
            sources: Vec::new(),
            next_cursor: Some("c".repeat(64)),
        })?;
        let projection = projection.as_object().ok_or_else(|| {
            serde_json::Error::io(std::io::Error::other("projection is not an object"))
        })?;
        let page = page.as_object().ok_or_else(|| {
            serde_json::Error::io(std::io::Error::other("source page is not an object"))
        })?;

        assert_eq!(
            projection.get("projectionRef"),
            Some(&serde_json::json!("a".repeat(64)))
        );
        assert_eq!(
            projection.get("nextCursor"),
            Some(&serde_json::json!("b".repeat(64)))
        );
        assert!(!projection.contains_key("projection_ref"));
        assert!(!projection.contains_key("next_cursor"));
        assert_eq!(
            page.get("nextCursor"),
            Some(&serde_json::json!("c".repeat(64)))
        );
        assert!(!page.contains_key("next_cursor"));
        Ok(())
    }
}
