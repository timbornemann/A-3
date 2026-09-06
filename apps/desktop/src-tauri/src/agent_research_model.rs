//! Narrow model seam for exercising the real research loop without a provider or credentials.
use super::*;
use std::future::Future;

/// Revalidation is not an adaptive read: it adds no evidence and cannot advance a cursor.
/// The existing reader verifies canonical path, full revision, encoding and secret policy.
pub(super) struct EvidenceGuard<'a> {
    pub work: Option<super::research_work::WorkGuard>,
    pub project: &'a ProjectIdentity,
    pub revisions: Vec<(a3_domain::FileRevision, u32)>,
}
impl EvidenceGuard<'_> {
    pub(super) fn admit_work(
        &self,
        mut decision: a3_application::AskResearchDecision,
        required: bool,
        mode: AgentSessionMode,
    ) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
        let note = match &decision {
            a3_application::AskResearchDecision::Answer { note, .. }
            | a3_application::AskResearchDecision::Research { note, .. } => note,
        };
        match (&note.work, &self.work) {
            (Some(update), Some(guard)) => {
                if matches!(
                    decision,
                    a3_application::AskResearchDecision::Research { .. }
                ) {
                    return Err(DecisionIssue::ReadsClosed);
                }
                let state = guard.admit(update)?;
                if let a3_application::AskResearchDecision::Answer {
                    evidence_status,
                    markdown,
                    ..
                } = &mut decision
                {
                    let design_choice = mode != AgentSessionMode::Ask
                        && state.can_request_design_choice()
                        && markdown
                            .trim()
                            .strip_prefix("QUESTION:")
                            .is_some_and(|question| !question.trim().is_empty());
                    if markdown.trim().starts_with("QUESTION:") && !design_choice {
                        return Err(DecisionIssue::WorkEvidence);
                    }
                    if guard.output_phase() != a3_application::ResearchOutputPhase::Finalize
                        && markdown != "Recherchezwischenstand."
                        && !design_choice
                    {
                        return Err(DecisionIssue::WorkEvidence);
                    }
                    *evidence_status = if state.ready_to_finish() || design_choice {
                        AskResearchEvidenceStatus::Sufficient
                    } else {
                        AskResearchEvidenceStatus::Incomplete
                    };
                }
            }
            (None, _) if required => return Err(DecisionIssue::WorkEvidence),
            (Some(_), None) => return Err(DecisionIssue::WorkEvidence),
            _ => {}
        }
        Ok(decision)
    }
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
    WorkEvidence,
    WorkCoverage,
    WorkAdmission(a3_application::ResearchWorkAdmissionError),
    Json,
    Shape,
    Object,
    Array,
    String,
    Stream,
    Fields,
    Version,
    Value,
    Markers,
    Truncated,
    UnknownSource,
    ReadsClosed,
    PlanShape,
}

impl DecisionIssue {
    pub(super) fn repair_hint_for_phase(
        self,
        phase: Option<a3_application::ResearchOutputPhase>,
        source_count: usize,
    ) -> String {
        use a3_application::ResearchOutputPhase;
        let rule = match phase {
            Some(ResearchOutputPhase::Initialize) => {
                "Initialize: decision contains only kind=progress and note. Return schema_version=5, work.questions with the complete required investigation contract and work.results=[]. Classify existing-code questions as repository; future proposals as design. Do not answer yet."
            }
            Some(ResearchOutputPhase::Analyze(id)) => {
                return format!(
                    "Analyze Q{}: return schema_version=5; decision contains only kind=progress and note; work.questions=[]. work.results contains at most one result, question_id={}, kind=interpretation, using only actually delivered current E-window anchor_ref evidence. Cover all explicitly named originals relevant to this question. No copied quotes, citation markers, boundedUnknown, designDecision, markdown or new questions. If original evidence cannot answer this question, return results=[] and identify the exact gap. Failure category: {}.",
                    id.get(),
                    id.get(),
                    self.code()
                );
            }
            Some(ResearchOutputPhase::Design(id)) => {
                return format!(
                    "Design Q{}: return schema_version=5; decision contains only kind=progress and note; work.questions=[]. work.results must contain exactly one concrete result, question_id={}, kind=designDecision, evidence=[]. Answer the original request, not merely a heading or status. Preserve admitted prerequisite design decisions; specify the requested future outcome. A proposed implementation need not already exist; further repository reads cannot repair an empty design. Only a consequential missing user choice permits kind=question with message and empty results. Do not add source anchors, quotes, citation markers, interpretation, boundedUnknown, markdown or new obligations. Failure category: {}.",
                    id.get(),
                    id.get(),
                    self.code()
                );
            }
            Some(ResearchOutputPhase::SummarizeOriginals(id)) => {
                return format!(
                    "SummarizeOriginals Q{}: schema_version=5; decision contains only kind=progress and note; work.questions=[]. Return exactly one result, question_id={}, kind=interpretation, with current E-window anchor_ref evidence. Describe the delivered entry points, APIs and visible integration constraints; all named originals are present in full. State unseen external implementation details as limits, not new prerequisites. No new design, tools, question or empty result. Failure category: {}.",
                    id.get(),
                    id.get(),
                    self.code()
                );
            }
            Some(ResearchOutputPhase::Finalize) => {
                "Finalize: return kind=plan with note, summary, changes, interfaces, tests and assumptions. Changes/tests are nonempty arrays of single-line concrete verifiable outcomes. No markdown field, markers, headings, citations, source_refs, evidence_status, new research or question. Use work.questions=[] and work.results=[]. The Core formats the plan and attaches admitted original evidence."
            }
            None => return self.repair_hint(source_count),
        };
        format!("{rule} Failure category: {}.", self.code())
    }

    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::WorkCoverage => "research-v2/required-source-coverage",
            Self::WorkAdmission(reason) => match reason {
                a3_application::ResearchWorkAdmissionError::AmbiguousQuote => {
                    "research-v2/quote-ambiguous"
                }
                a3_application::ResearchWorkAdmissionError::UndeliveredQuote => {
                    "research-v2/quote-undelivered"
                }
                a3_application::ResearchWorkAdmissionError::ContractChanged => {
                    "research-v2/contract-changed"
                }
                a3_application::ResearchWorkAdmissionError::UnknownQuestion => {
                    "research-v2/question-unknown"
                }
                a3_application::ResearchWorkAdmissionError::UnexaminedUnknown => {
                    "research-v2/boundary-unexamined"
                }
                a3_application::ResearchWorkAdmissionError::UnsupportedResult => {
                    "research-v2/result-unsupported"
                }
            },
            Self::WorkEvidence => "research-v2/work-evidence",
            Self::Json => "research-v1/json",
            Self::Shape => "research-v1/shape",
            Self::Object => "research-v1/object-type",
            Self::Array => "research-v1/array-type",
            Self::String => "research-v1/string-type",
            Self::Stream => "research-v1/stream-invalid",
            Self::Fields => "research-v1/fields",
            Self::Version => "research-v1/version",
            Self::Value => "research-v1/value",
            Self::Markers => "research-v1/markers",
            Self::Truncated => "research-v1/output-truncated",
            Self::UnknownSource => "research-v1/source",
            Self::ReadsClosed => "research-v1/reads-closed",
            Self::PlanShape => "research-v1/plan-shape",
        }
    }
    pub(super) fn repair_hint(self, source_count: usize) -> String {
        let detail = match self {
            Self::WorkCoverage => {
                "Cover EVERY explicitly named indexed file in NAMED TARGETS with actual E-window references in required results. Leave unsupported questions unresolved. Source captions or markdown citations do not replace work.results evidence."
            }
            Self::WorkAdmission(reason) => match reason {
                a3_application::ResearchWorkAdmissionError::AmbiguousQuote => {
                    "The quote occurs in multiple original locations. Include enough adjacent original text to identify one unique occurrence in CURRENT EVIDENCE. Do not calculate line numbers."
                }
                a3_application::ResearchWorkAdmissionError::UndeliveredQuote => {
                    "Select an E-window anchor_ref actually shown in CURRENT EVIDENCE. Cached text, source captions and old conversation are not delivered original evidence. Omit the result if the needed original text is absent."
                }
                a3_application::ResearchWorkAdmissionError::ContractChanged => {
                    "Preserve the frozen Core question contract. After initialization return work.questions=[]; never rewrite an answered result. The Core binds initial questions to the original goal."
                }
                a3_application::ResearchWorkAdmissionError::UnknownQuestion => {
                    "Return results only for question IDs listed in the Core contract, or sequential IDs 1..N of the initial question definitions."
                }
                a3_application::ResearchWorkAdmissionError::UnexaminedUnknown => {
                    "No exhausted access boundary has been established by the Core. Do not claim boundedUnknown; omit this result and request exact relevant original evidence."
                }
                a3_application::ResearchWorkAdmissionError::UnsupportedResult => {
                    "An interpretation needs original evidence; a designDecision requires a design question. Resolve prerequisites first. Result text must not contain citation markers; the Core attaches original source references."
                }
            },
            Self::WorkEvidence => {
                "Use schema_version 5 with work.questions and work.results. Initially define all required questions; afterwards keep questions empty. Do not change answered questions. Return results only for ACTIVE Q using issued E-window anchor_ref values from CURRENT EVIDENCE. An unknown needs a Core-issued exhausted boundary. Omit unsupported results and describe the exact missing evidence in the public note."
            }
            Self::Object => {
                "An object has the wrong JSON type. Return the root, decision, note and each action as JSON objects, not strings or arrays."
            }
            Self::Array => {
                "An array has the wrong JSON type. actions, source_refs, finding_source_refs and literals must be arrays, including [] when allowed; never null, a string or an object."
            }
            Self::String => {
                "A text field has the wrong JSON type. Use strings for the public note fields, markdown, kind, path and source_ref; use an empty string rather than null where the schema permits it."
            }
            Self::Stream => {
                "The response stream did not produce exactly one nonempty completed document. Return one concise, complete JSON document under the supplied schema."
            }
            Self::PlanShape => {
                "A sufficient planning answer must begin PLAN: with Markdown headings Summary, Implementation Changes, Interfaces, Test Plan, Assumptions. Include nonempty ordered change and test steps (at most 64 total) and current source citations. Use QUESTION: only for a genuinely blocking user choice. Proposed new interfaces and formats belong in the plan as explicit design assumptions, not missing evidence. Do not ask the user to restart for an output-format error."
            }
            Self::Json => {
                "Return a complete JSON object, without fences or prose. Close all strings, arrays and objects."
            }
            Self::Fields => {
                "Use exactly the required fields of the supplied schema; omit unknown fields. Do not omit the public note or evidence_status."
            }
            Self::Version => "Use the schema_version required by the exact supplied phase schema.",
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

#[cfg(test)]
pub(super) fn validate_decision(
    raw: &str,
    permission: BeginResearchDecision,
    source_count: usize,
) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
    validate_phase_decision(raw, permission, source_count, None)
}

pub(super) fn validate_phase_decision(
    raw: &str,
    permission: BeginResearchDecision,
    source_count: usize,
    phase: Option<a3_application::ResearchOutputPhase>,
) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
    let decision = phase
        .map_or_else(
            || DecodeAskResearchDecision.decode(raw),
            |phase| DecodeAskResearchDecision.decode_phase(raw, phase),
        )
        .map_err(|error| match error {
            a3_application::AskResearchDecisionDecodeError::MalformedJson => DecisionIssue::Json,
            a3_application::AskResearchDecisionDecodeError::ExpectedObject => DecisionIssue::Object,
            a3_application::AskResearchDecisionDecodeError::ExpectedArray => DecisionIssue::Array,
            a3_application::AskResearchDecisionDecodeError::ExpectedString => DecisionIssue::String,
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

/// Validate the planning outcome at the SAME document boundary as JSON and references. A
/// structurally invalid plan must use its single repair, never masquerade as a user question.
#[cfg(test)]
pub(super) fn validate_outcome(
    decision: a3_application::AskResearchDecision,
    mode: AgentSessionMode,
) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
    validate_outcome_with_attribution(decision, mode, true)
}

pub(super) fn validate_outcome_with_attribution(
    decision: a3_application::AskResearchDecision,
    mode: AgentSessionMode,
    model_attribution: bool,
) -> Result<a3_application::AskResearchDecision, DecisionIssue> {
    if mode != AgentSessionMode::Ask
        && let a3_application::AskResearchDecision::Answer {
            markdown,
            source_ordinals,
            evidence_status: AskResearchEvidenceStatus::Sufficient,
            ..
        } = &decision
    {
        let text = markdown.trim();
        let question = text
            .strip_prefix("QUESTION:")
            .is_some_and(|text| !text.trim().is_empty());
        let plan = text.strip_prefix("PLAN:").is_some_and(|plan| {
            has_required_plan_sections(plan)
                && a3_domain::AgentWorkPlan::from_reviewed_markdown(plan).is_ok()
                && (!model_attribution || !source_ordinals.is_empty())
        });
        if !question && !plan {
            return Err(DecisionIssue::PlanShape);
        }
    }
    Ok(decision)
}

pub(super) trait ResearchModel: Send + Sync {
    /// Historical replay models retain V3/V4; the production provider always requires V5.
    fn requires_work_contract(&self) -> bool {
        false
    }
    fn research_evidence_budget(
        &self,
        mode: AgentSessionMode,
        command: Option<&str>,
    ) -> impl Future<Output = Result<usize, AgentConversationFailure>> + Send;
    fn complete_research_decision(
        &self,
        mode: AgentSessionMode,
        search_allowed: bool,
        phase: a3_application::ResearchOutputPhase,
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
    fn requires_work_contract(&self) -> bool {
        true
    }
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
        phase: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        command: Option<String>,
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.complete_research_decision(mode, search_allowed, phase, transcript, command, control)
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
