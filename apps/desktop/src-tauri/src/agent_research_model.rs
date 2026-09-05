//! Narrow model seam for exercising the real research loop without a provider or credentials.
use super::*;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecisionIssue {
    Shape,
    UnknownSource,
    ReadsClosed,
}

impl DecisionIssue {
    pub(super) fn repair_hint(self, source_count: usize) -> String {
        let detail = match self {
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
        .map_err(|_| DecisionIssue::Shape)?;
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
