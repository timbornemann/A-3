//! Narrow model seam for exercising the real research loop without a provider or credentials.
use super::*;
use std::future::Future;

/// Revalidation is not an adaptive read: it adds no evidence and cannot advance a cursor.
/// The existing reader verifies canonical path, full revision, encoding and secret policy.
pub(super) struct EvidenceGuard<'a> {
    pub project: &'a ProjectIdentity,
    pub revisions: Vec<(a3_domain::FileRevision, u32)>,
}
impl EvidenceGuard<'_> {
    pub(super) async fn validate(
        &self,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        if control.cancellation_token().is_cancelled() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        for (revision, start_line) in &self.revisions {
            let request = AgentFileInspection::new(
                revision.path().clone(),
                AgentFileStartLine::new(*start_line)
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                AgentFileLineCount::new(1).map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
            );
            WorkspaceAgentSourceReader
                .read_page(self.project, revision, &request, control)
                .await
                .map_err(|error| {
                    if error == AgentSourceReadFailure::Stale {
                        AgentSessionManagerFailure::IndexChanged
                    } else {
                        AgentSessionManagerFailure::Unavailable
                    }
                })?;
        }
        Ok(())
    }
}

pub(super) fn diagnostic(
    code: &str,
    permission: BeginResearchDecision,
    controller: &BoundedResearchController,
) -> String {
    format!(
        "{code}; phase={}; model={}/{}; reads={}/{}; repairs={}/{}; retries={}/{}",
        if permission == BeginResearchDecision::FinalOnly {
            "answer"
        } else {
            "research"
        },
        controller.decisions_used(),
        controller.limits().model_decisions(),
        controller.actions_used(),
        controller.limits().read_actions(),
        controller.repairs_used(),
        controller.limits().repairs(),
        controller.model_retries_used(),
        controller.limits().model_retries()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecisionIssue {
    Json,
    Shape,
    Fields,
    Version,
    Value,
    Markers,
    Truncated,
    UnknownSource,
    ReadsClosed,
}

impl DecisionIssue {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Json => "research-v1/json",
            Self::Shape => "research-v1/shape",
            Self::Fields => "research-v1/fields",
            Self::Version => "research-v1/version",
            Self::Value => "research-v1/value",
            Self::Markers => "research-v1/markers",
            Self::Truncated => "research-v1/output-truncated",
            Self::UnknownSource => "research-v1/source",
            Self::ReadsClosed => "research-v1/reads-closed",
        }
    }
    pub(super) fn repair_hint(self, source_count: usize) -> String {
        let detail = match self {
            Self::Json => {
                "Return a complete JSON object, without fences or prose. Close all strings, arrays and objects."
            }
            Self::Fields => {
                "Use exactly the required fields of the supplied schema; omit unknown fields. Do not omit the public note or evidence_status."
            }
            Self::Version => "Use schema_version 4 and the exact supplied phase schema.",
            Self::Value => {
                "Use only the schema's closed enums and bounded values. Source labels are S1..S200 and start_line is positive."
            }
            Self::Markers => {
                "Markdown markers and source_refs must name exactly the same sources. Do not place markers in code."
            }
            Self::Truncated => {
                "The output was cut off. Return a substantially SHORTER complete schema-conforming object. Use a concise note and answer; do not repeat source excerpts."
            }
            Self::Shape => {
                "The document violates the supplied schema. Use only its exact field and action names. A research decision requires evidence_status=incomplete and 1-4 actions; inspectPath requires path and a positive start_line. An answer requires markdown and exactly matching source_refs. Keep the public note concise."
            }
            Self::UnknownSource => {
                "The document cites a source that has not been issued. Use only references actually present in the CURRENT EVIDENCE packet, not labels from old conversation. Do not invent evidence."
            }
            Self::ReadsClosed => {
                "No further reads are available. Return kind answer with an honest sufficient or incomplete evidence_status; do not include actions."
            }
        };
        format!(
            "REPAIR: {detail} There are {source_count} issued sources in this section. Return one complete JSON document without surrounding prose. No actions from the invalid output were executed."
        )
    }
}

pub(super) fn validate_decision(
    raw: &str,
    permission: BeginResearchDecision,
    source_count: usize,
) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
    let decision = DecodeAskResearchDecision
        .decode(raw)
        .map_err(|error| match error {
            a3_application::AskResearchDecisionDecodeError::MalformedJson => DecisionIssue::Json,
            a3_application::AskResearchDecisionDecodeError::UnknownOrMissingField => {
                DecisionIssue::Fields
            }
            a3_application::AskResearchDecisionDecodeError::UnsupportedVersion => {
                DecisionIssue::Version
            }
            a3_application::AskResearchDecisionDecodeError::InvalidValue => DecisionIssue::Value,
            a3_application::AskResearchDecisionDecodeError::CitationMismatch => {
                DecisionIssue::Markers
            }
            a3_application::AskResearchDecisionDecodeError::MissingSources => {
                DecisionIssue::UnknownSource
            }
            a3_application::AskResearchDecisionDecodeError::OutputTooLarge => {
                DecisionIssue::Truncated
            }
            _ => DecisionIssue::Shape,
        })?;
    let valid = match &decision {
        a3_application::AskResearchDecision::Answer {
            source_ordinals,
            note,
            ..
        } => source_ordinals
            .iter()
            .chain(note.source_ordinals.iter())
            .all(|ordinal| usize::from(*ordinal) <= source_count),
        a3_application::AskResearchDecision::Research { note, actions } => {
            if permission == BeginResearchDecision::FinalOnly {
                return Err(DecisionIssue::ReadsClosed);
            }
            note.source_ordinals
                .iter()
                .all(|ordinal| usize::from(*ordinal) <= source_count)
                && actions.iter().all(|action| match action {
                    AskResearchAction::InspectSource(ordinal) => {
                        usize::from(*ordinal) <= source_count
                    }
                    AskResearchAction::InspectRelations { source_ordinal, .. }
                    | AskResearchAction::InspectFunctionFlow { source_ordinal, .. } => {
                        usize::from(*source_ordinal) <= source_count
                    }
                    _ => true,
                })
        }
    };
    if valid {
        Ok(decision)
    } else {
        Err(DecisionIssue::UnknownSource)
    }
}

pub(super) trait ResearchModel: Send + Sync {
    fn research_evidence_budget(
        &self,
        mode: AgentSessionMode,
        command: Option<&str>,
    ) -> impl Future<Output = Result<usize, AgentConversationFailure>> + Send;
    fn complete_research_decision(
        &self,
        mode: AgentSessionMode,
        search_allowed: bool,
        transcript: &[(ModelMessageRole, String)],
        command: Option<String>,
        control: &JobContext,
    ) -> impl Future<Output = Result<String, AgentConversationFailure>> + Send;
    fn complete_evidence_diagrams(
        &self,
        transcript: &[(ModelMessageRole, String)],
        control: &JobContext,
    ) -> impl Future<Output = Result<String, AgentConversationFailure>> + Send;
}

impl ResearchModel for AgentConversationRuntime {
    async fn research_evidence_budget(
        &self,
        mode: AgentSessionMode,
        command: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        self.research_evidence_budget(mode, command).await
    }
    async fn complete_research_decision(
        &self,
        mode: AgentSessionMode,
        search_allowed: bool,
        transcript: &[(ModelMessageRole, String)],
        command: Option<String>,
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.complete_research_decision(mode, search_allowed, transcript, command, control)
            .await
    }
    async fn complete_evidence_diagrams(
        &self,
        transcript: &[(ModelMessageRole, String)],
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.complete_evidence_diagrams(transcript, control).await
    }
}
