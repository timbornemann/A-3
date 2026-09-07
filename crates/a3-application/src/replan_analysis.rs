//! Pure replan admission and content-free feedback for the existing single repair.
use crate::{
    AskResearchDecision, AskResearchDecisionDecodeError, DecodeAskResearchDecision,
    ReplanResearchContext, ResearchOutputPhase, ResearchWorkAdmissionError,
};
use a3_domain::{ResearchQuestionId, ResearchResultKind, ResearchWorkState};

/// Closed rejection classes; never carries source bytes, model text or provider details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanAnalysisFailure {
    /// The independently strict research decoder rejected structure or a bounded value.
    Decode(AskResearchDecisionDecodeError),
    /// Replan analysis only accepts the supplied progress decision, not an action or question.
    WrongDecision,
    /// A non-interpretation cannot stand in for repository investigation.
    WrongResultKind,
    /// Current original pages could not form valid admission windows.
    OriginalWindows,
    /// The packet cannot start a new analysis of the current obligation.
    PacketState,
    /// Original evidence or the durable work contract rejected the complete update.
    Admission(ResearchWorkAdmissionError),
}

impl std::fmt::Display for ReplanAnalysisFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("replan analysis was not admitted")
    }
}

impl std::error::Error for ReplanAnalysisFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Admission(error) => Some(error),
            _ => None,
        }
    }
}

impl ReplanAnalysisFailure {
    /// Only fixed Core text and closed enums enter the already-budgeted repair request.
    pub(crate) fn repair_instruction(self) -> String {
        let hint = match self {
            Self::Decode(AskResearchDecisionDecodeError::MalformedJson) => {
                "Return a complete JSON object without prose or fences."
            }
            Self::Decode(AskResearchDecisionDecodeError::UnknownOrMissingField) => {
                "Include every required field, including the V5 note; remove extra fields."
            }
            Self::Decode(AskResearchDecisionDecodeError::MissingSources) => {
                "Observations require supplied source references; hypotheses are not facts."
            }
            Self::Decode(_) => "Check the supplied field types, version, enums and bounds.",
            Self::WrongDecision => "No action, question, plan or final answer is admitted here.",
            Self::WrongResultKind => "Only an interpretation can explain the current code.",
            Self::OriginalWindows | Self::PacketState => {
                "Do not invent originals or claim that an already analyzed packet is new."
            }
            Self::Admission(ResearchWorkAdmissionError::UndeliveredQuote) => {
                "Cite only E anchors present in this request, not remembered or invented sources."
            }
            Self::Admission(ResearchWorkAdmissionError::AmbiguousQuote) => {
                "Select an unambiguous delivered original anchor."
            }
            Self::Admission(ResearchWorkAdmissionError::ContractChanged)
            | Self::Admission(ResearchWorkAdmissionError::UnknownQuestion) => {
                "Preserve the issued obligation; do not add or redefine questions."
            }
            Self::Admission(_) => "An unsupported result cannot resolve the obligation.",
        };
        format!(
            "Replan analysis rejected: {self:?}. {hint} Return the supplied V5 Analyze schema, progress decision, Q1 only, no new questions or actions. Use current E anchors for an interpretation, or empty results for missing evidence. This is the single repair."
        )
    }
}

pub(crate) fn admit(
    raw: &str,
    research: &ReplanResearchContext,
) -> Result<ResearchWorkState, ReplanAnalysisFailure> {
    let shape: serde_json::Value = serde_json::from_str(raw).map_err(|_| {
        ReplanAnalysisFailure::Decode(AskResearchDecisionDecodeError::MalformedJson)
    })?;
    if shape["decision"]["kind"] != "progress" {
        return Err(ReplanAnalysisFailure::WrongDecision);
    }
    let decision = DecodeAskResearchDecision
        .decode_phase(raw, ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST))
        .map_err(ReplanAnalysisFailure::Decode)?;
    let AskResearchDecision::Answer { note, .. } = decision else {
        return Err(ReplanAnalysisFailure::WrongDecision);
    };
    let update = note.work.as_ref().ok_or(ReplanAnalysisFailure::Decode(
        AskResearchDecisionDecodeError::InvalidShape,
    ))?;
    if update
        .results
        .iter()
        .any(|r| r.kind != ResearchResultKind::Interpretation)
    {
        return Err(ReplanAnalysisFailure::WrongResultKind);
    }
    let windows = research
        .windows()
        .map_err(|_| ReplanAnalysisFailure::OriginalWindows)?;
    let mut previous = research.checkpoint.work.clone();
    previous
        .begin_analysis(ResearchQuestionId::FIRST, research.packet())
        .map_err(|_| ReplanAnalysisFailure::PacketState)?;
    crate::admit_research_work(previous.objective(), Some(&previous), update, &windows)
        .map_err(ReplanAnalysisFailure::Admission)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentFileStartLine, ContentHash, FileRevision, RepositoryPath, SnapshotId, SourcePosition,
        SourceRange, TaskReplanReason, TaskStepId,
    };
    use serde_json::json;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn context() -> Result<ReplanResearchContext, Box<dyn std::error::Error>> {
        Ok(ReplanResearchContext {
            checkpoint: crate::ReplanResearchCheckpoint::new(
                TaskStepId::from_bytes([1; 32]),
                SnapshotId::from_bytes([2; 32]),
                &TaskReplanReason::try_from_string("incorrect return".to_owned())?,
                "preserve the value",
            )?,
            pages: vec![crate::AgentSourcePage::new(
                FileRevision::new(
                    RepositoryPath::try_from_bytes(b"private.py".to_vec())?,
                    ContentHash::from_bytes([3; 32]),
                ),
                SourceRange::new(0, 9, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
                AgentFileStartLine::new(1)?,
                "return 0\n".to_owned(),
                None,
                false,
            )?],
        })
    }

    fn document() -> serde_json::Value {
        json!({"schema_version":5,
            "decision":{"kind":"progress","note":{"goal":"Check return","finding_kind":"hypothesis","finding":"Current evidence","finding_source_refs":[],"gap":"Verify behavior","next_step":"Resolve cause"}},
            "work":{"questions":[],"results":[{"question_id":1,"kind":"interpretation","text":"The function returns zero; verify the required return.","evidence":[{"anchor_ref":"E1"}]}]}})
    }

    #[test]
    fn replan_admission_preserves_failure_stage_without_source_or_model_content() -> TestResult {
        let research = context()?;
        let before = research.clone();
        let mut missing = document();
        missing["decision"]
            .as_object_mut()
            .ok_or("decision")?
            .remove("note");
        let mut invented = document();
        invented["work"]["results"][0]["evidence"][0]["anchor_ref"] = json!("E8");
        let mut extra = document();
        extra["private_user_data"] = json!("never echo this");
        let cases = [
            (
                "{private model bytes".to_owned(),
                ReplanAnalysisFailure::Decode(AskResearchDecisionDecodeError::MalformedJson),
            ),
            (
                json!({"schema_version":5,"action":{"kind":"run","command":"secret"}}).to_string(),
                ReplanAnalysisFailure::WrongDecision,
            ),
            (
                missing.to_string(),
                ReplanAnalysisFailure::Decode(
                    AskResearchDecisionDecodeError::UnknownOrMissingField,
                ),
            ),
            (
                extra.to_string(),
                ReplanAnalysisFailure::Decode(
                    AskResearchDecisionDecodeError::UnknownOrMissingField,
                ),
            ),
            (
                invented.to_string(),
                ReplanAnalysisFailure::Admission(ResearchWorkAdmissionError::UndeliveredQuote),
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(admit(&raw, &research), Err(expected));
            let feedback = expected.repair_instruction();
            assert!(feedback.len() <= 512);
            for forbidden in [
                "private.py",
                "private model bytes",
                "never echo this",
                "return 0",
                "secret",
                "E8",
            ] {
                assert!(!feedback.contains(forbidden));
                assert!(!format!("{expected:?}").contains(forbidden));
            }
            assert_eq!(
                research, before,
                "rejected proposals never alter durable work"
            );
        }
        let accepted = admit(&document().to_string(), &research)?;
        assert!(accepted.ready_to_finish());
        assert_eq!(
            accepted.questions()[0].result().ok_or("result")?.kind(),
            ResearchResultKind::Interpretation
        );
        assert_eq!(research, before);
        let mut no_evidence = document();
        no_evidence["work"]["results"] = json!([]);
        assert!(!admit(&no_evidence.to_string(), &research)?.ready_to_finish());
        Ok(())
    }

    #[test]
    fn replan_admission_all_repair_messages_have_a_fixed_small_bound() {
        use AskResearchDecisionDecodeError as D;
        use ResearchWorkAdmissionError as A;
        let mut failures = vec![
            ReplanAnalysisFailure::WrongDecision,
            ReplanAnalysisFailure::WrongResultKind,
            ReplanAnalysisFailure::OriginalWindows,
            ReplanAnalysisFailure::PacketState,
        ];
        failures.extend(
            [
                D::InvalidSchema,
                D::OutputTooLarge,
                D::MalformedJson,
                D::InvalidShape,
                D::ExpectedObject,
                D::ExpectedArray,
                D::ExpectedString,
                D::UnknownOrMissingField,
                D::UnsupportedVersion,
                D::InvalidValue,
                D::CitationMismatch,
                D::MissingSources,
            ]
            .map(ReplanAnalysisFailure::Decode),
        );
        failures.extend(
            [
                A::AmbiguousQuote,
                A::ContractChanged,
                A::UnknownQuestion,
                A::UndeliveredQuote,
                A::UnexaminedUnknown,
                A::UnsupportedResult,
            ]
            .map(ReplanAnalysisFailure::Admission),
        );
        for failure in failures {
            let feedback = failure.repair_instruction();
            assert!(feedback.len() <= 512, "{failure:?}: {}", feedback.len());
            assert!(feedback.contains("This is the single repair."));
        }
    }
}
