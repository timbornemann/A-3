use crate::{ProjectMapSourcePreviewV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// User-facing Ask research phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentAskResearchPhaseV1 {
    /// Binds the turn to one published project index.
    Preparing,
    /// Selects relevant indexed evidence through Task Lens.
    SelectingEvidence,
    /// Searches safely readable current repository text.
    SearchingSource,
    /// Reads and validates an exact current source location.
    InspectingSource,
    /// Produces the evidence-grounded answer.
    Answering,
    /// Marks the research path as terminal.
    Completed,
}

/// Active or terminal Ask research state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentAskResearchStateV1 {
    /// Research is still progressing.
    Running,
    /// Research and answer persistence completed.
    Completed,
    /// Research stopped after a safe failure.
    Failed,
    /// Research stopped after cancellation.
    Cancelled,
}

/// Honest completeness of a repository search.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentAskResearchCompletenessV1 {
    /// The requested bounded search covered all eligible indexed files.
    Complete,
    /// A documented safety or resource bound stopped the search.
    Limited,
    /// Completeness is not meaningful for this step.
    NotApplicable,
}

/// Safe reason a source was selected.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentAskResearchSelectionReasonV1 {
    /// The question named the source path or symbol exactly.
    ExactNameOrPath,
    /// Indexed lexical text selected the source.
    IndexedText,
    /// An indexed code relationship selected the source.
    Relationship,
    /// Test proximity selected the source.
    Test,
    /// A current verified module card selected the source.
    VerifiedModuleKnowledge,
    /// Semantic ranking proposed the source before current-source validation.
    SemanticCandidate,
    /// A bounded current-source literal search found the source.
    SourceText,
}

/// Safe source classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentAskResearchSourceKindV1 {
    /// Whole-file evidence.
    File,
    /// Symbol or source-span evidence.
    Symbol,
    /// Evidence for an indexed relationship.
    Relationship,
    /// Evidence backing current verified module knowledge.
    VerifiedClaim,
}

/// One content-free chronological research step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentAskResearchStepV1 {
    phase: AgentAskResearchPhaseV1,
    state: AgentAskResearchStateV1,
    action: String,
    query: Option<String>,
    completeness: AgentAskResearchCompletenessV1,
    occurred_at_unix_millis: String,
}

impl AgentAskResearchStepV1 {
    /// Creates one Core-validated presentation step.
    #[must_use]
    pub const fn new(
        phase: AgentAskResearchPhaseV1,
        state: AgentAskResearchStateV1,
        action: String,
        query: Option<String>,
        completeness: AgentAskResearchCompletenessV1,
        occurred_at_unix_millis: String,
    ) -> Self {
        Self {
            phase,
            state,
            action,
            query,
            completeness,
            occurred_at_unix_millis,
        }
    }
}

/// Compact summary for one recorded Ask turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentAskResearchTurnV1 {
    user_sequence: String,
    phase: AgentAskResearchPhaseV1,
    state: AgentAskResearchStateV1,
    action: String,
    started_at_unix_millis: String,
    source_count: u16,
    cited_source_count: u16,
    stale: bool,
}

impl AgentAskResearchTurnV1 {
    /// Creates one bounded current or historical turn summary.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        user_sequence: String,
        phase: AgentAskResearchPhaseV1,
        state: AgentAskResearchStateV1,
        action: String,
        started_at_unix_millis: String,
        source_count: u16,
        cited_source_count: u16,
        stale: bool,
    ) -> Self {
        Self {
            user_sequence,
            phase,
            state,
            action,
            started_at_unix_millis,
            source_count,
            cited_source_count,
            stale,
        }
    }
}

/// Full safe research detail for one Ask answer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentAskResearchDetailV1 {
    user_sequence: String,
    steps: Vec<AgentAskResearchStepV1>,
    source_count: u16,
    cited_source_count: u16,
    stale: bool,
}

impl AgentAskResearchDetailV1 {
    /// Creates one bounded detail projection.
    #[must_use]
    pub const fn new(
        user_sequence: String,
        steps: Vec<AgentAskResearchStepV1>,
        source_count: u16,
        cited_source_count: u16,
        stale: bool,
    ) -> Self {
        Self {
            user_sequence,
            steps,
            source_count,
            cited_source_count,
            stale,
        }
    }
}

/// One source reference safe for the untrusted WebView.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentAskResearchSourceV1 {
    source_ref: String,
    path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    symbol: Option<String>,
    kind: AgentAskResearchSourceKindV1,
    reason: AgentAskResearchSelectionReasonV1,
    used_for_answer: bool,
}

impl AgentAskResearchSourceV1 {
    /// Creates one metadata-only source item.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source_ref: String,
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
        symbol: Option<String>,
        kind: AgentAskResearchSourceKindV1,
        reason: AgentAskResearchSelectionReasonV1,
        used_for_answer: bool,
    ) -> Self {
        Self {
            source_ref,
            path,
            start_line,
            end_line,
            symbol,
            kind,
            reason,
            used_for_answer,
        }
    }
}

/// Requests recorded Ask turns for one project-local session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentAskResearchTurnsRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
}
impl QueryAgentAskResearchTurnsRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Requests one exact Ask turn detail.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentAskResearchDetailRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    user_sequence: String,
}
impl QueryAgentAskResearchDetailRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the visible user-message sequence.
    #[must_use]
    pub fn user_sequence(&self) -> &str {
        &self.user_sequence
    }
}

/// Requests one cursor-bound source page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentAskResearchSourcesRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    user_sequence: String,
    cursor: Option<String>,
}
impl QueryAgentAskResearchSourcesRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the visible user-message sequence.
    #[must_use]
    pub fn user_sequence(&self) -> &str {
        &self.user_sequence
    }
    /// Returns the opaque project-bound cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Requests a safe preview only through a previously issued opaque source reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentAskResearchSourcePreviewRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    user_sequence: String,
    source_ref: String,
}
impl QueryAgentAskResearchSourcePreviewRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the visible user-message sequence.
    #[must_use]
    pub fn user_sequence(&self) -> &str {
        &self.user_sequence
    }
    /// Returns the opaque source capability.
    #[must_use]
    pub fn source_ref(&self) -> &str {
        &self.source_ref
    }
}

macro_rules! response {
    ($name:ident, $result:ident) => {
        #[doc = concat!("Versioned response for `", stringify!($result), "`.")]
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $name {
            /// Protocol version.
            pub protocol_version: ProtocolVersion,
            /// Closed result.
            pub result: $result,
        }
    };
}

/// Turn-list availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentAskResearchTurnsResultV1 {
    /// No project is active.
    NoProject,
    /// The requested session is absent from the active project.
    NotFound,
    /// Recorded turns are available.
    Available {
        /// At most 32 newest research turns.
        turns: Vec<AgentAskResearchTurnV1>,
    },
}
response!(
    AgentAskResearchTurnsResponseV1,
    AgentAskResearchTurnsResultV1
);

/// Detail availability including the pre-V30 historical state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentAskResearchDetailResultV1 {
    /// No project is active.
    NoProject,
    /// The requested session or user entry is absent.
    NotFound,
    /// The Ask answer predates durable research traces.
    NotRecorded,
    /// The trace detail is available.
    Available {
        /// Metadata-only research path.
        detail: AgentAskResearchDetailV1,
    },
}
response!(
    AgentAskResearchDetailResponseV1,
    AgentAskResearchDetailResultV1
);

/// Source-list availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentAskResearchSourcesResultV1 {
    /// No project is active.
    NoProject,
    /// The requested trace or cursor is absent or no longer valid.
    NotFound,
    /// A project-bound source page is available.
    Available {
        /// At most 50 safe source metadata items.
        sources: Vec<AgentAskResearchSourceV1>,
        /// Opaque cursor for the next trace-bound page.
        next_cursor: Option<String>,
    },
}
response!(
    AgentAskResearchSourcesResponseV1,
    AgentAskResearchSourcesResultV1
);

/// Safe source-preview availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentAskResearchSourcePreviewResultV1 {
    /// No project is active.
    NoProject,
    /// The source capability is absent or belongs to another trace.
    NotFound,
    /// The trace belongs to an older project index, so source text is withheld.
    Stale,
    /// A bounded current-source preview is available.
    Available {
        /// Existing safe source-preview projection.
        preview: ProjectMapSourcePreviewV1,
    },
}
response!(
    AgentAskResearchSourcePreviewResponseV1,
    AgentAskResearchSourcePreviewResultV1
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_reject_paths_and_internal_ids() {
        let turns = r#"{"protocolVersion":1,"sessionId":"00","path":"src/lib.rs"}"#;
        let detail =
            r#"{"protocolVersion":1,"sessionId":"00","userSequence":"1","snapshotId":"11"}"#;
        let sources = r#"{"protocolVersion":1,"sessionId":"00","userSequence":"1","cursor":null,"evidenceId":"11"}"#;
        let preview = r#"{"protocolVersion":1,"sessionId":"00","userSequence":"1","sourceRef":"11","path":"src/lib.rs"}"#;

        assert!(serde_json::from_str::<QueryAgentAskResearchTurnsRequestV1>(turns).is_err());
        assert!(serde_json::from_str::<QueryAgentAskResearchDetailRequestV1>(detail).is_err());
        assert!(serde_json::from_str::<QueryAgentAskResearchSourcesRequestV1>(sources).is_err());
        assert!(
            serde_json::from_str::<QueryAgentAskResearchSourcePreviewRequestV1>(preview).is_err()
        );
    }

    #[test]
    fn detail_response_contains_no_private_runtime_fields() -> Result<(), serde_json::Error> {
        let response = AgentAskResearchDetailResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: AgentAskResearchDetailResultV1::Available {
                detail: AgentAskResearchDetailV1::new(
                    "1".to_owned(),
                    vec![AgentAskResearchStepV1::new(
                        AgentAskResearchPhaseV1::SelectingEvidence,
                        AgentAskResearchStateV1::Running,
                        "Task Lens wählt aktuelle Evidence".to_owned(),
                        Some("TODO".to_owned()),
                        AgentAskResearchCompletenessV1::NotApplicable,
                        "1".to_owned(),
                    )],
                    2,
                    1,
                    false,
                ),
            },
        };

        let encoded = serde_json::to_string(&response)?;
        for forbidden in [
            "prompt",
            "rawResponse",
            "provider",
            "model",
            "token",
            "budget",
            "snapshotId",
            "indexRunId",
            "evidenceId",
            "confidence",
            "score",
        ] {
            assert!(!encoded.contains(forbidden), "unexpected field {forbidden}");
        }
        Ok(())
    }
}
