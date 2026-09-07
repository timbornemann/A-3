//! Pure replan admission and content-free feedback for the existing single repair.
use crate::{
    AskResearchDecision, AskResearchDecisionDecodeError, DecodeAskResearchDecision,
    ReplanResearchContext, ResearchOutputPhase, ResearchWorkAdmissionError,
};
use a3_domain::{ResearchQuestionId, ResearchResultKind, ResearchResultSource};

/// Closed rejection classes; never carries source bytes, model text or provider details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanAnalysisFailure {
    /// The independently strict research decoder rejected structure or a bounded value.
    Decode(AskResearchDecisionDecodeError),
    /// Replan analysis only accepts the supplied interpretation or need, not an action or question.
    WrongDecision,
    /// A non-interpretation cannot stand in for repository investigation.
    WrongResultKind,
    /// Current original pages could not form valid admission windows.
    OriginalWindows,
    /// The packet cannot start a new analysis of the current obligation.
    PacketState,
    /// Navigation targets are not present in the supplied objective or original packet.
    UnboundEvidenceNeed,
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
                "Include every required V7 response field; remove extra fields."
            }
            Self::Decode(AskResearchDecisionDecodeError::MissingSources) => {
                "Observations require supplied source references; hypotheses are not facts."
            }
            Self::Decode(_) => "Check the supplied field types, version, enums and bounds.",
            Self::WrongDecision => "No action, question, plan or final answer is admitted here.",
            Self::WrongResultKind => "Only an interpretation can explain the current code.",
            Self::UnboundEvidenceNeed => {
                "Use only literal targets present in the objective or delivered originals."
            }
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
            "Replan analysis rejected: {self:?}. {hint} Return the supplied V7 Analyze schema, Q1 only, no new questions or actions. Use current E anchors for an interpretation, or evidenceNeed with bounded literal targets. This is the single repair."
        )
    }
}

pub(crate) fn admit(
    raw: &str,
    research: &ReplanResearchContext,
) -> Result<crate::ReplanResearchCheckpoint, ReplanAnalysisFailure> {
    let shape: serde_json::Value = serde_json::from_str(raw).map_err(|_| {
        ReplanAnalysisFailure::Decode(AskResearchDecisionDecodeError::MalformedJson)
    })?;
    if shape["schema_version"] != 7
        || !matches!(
            shape["response"]["kind"].as_str(),
            Some("interpretation" | "evidenceNeed")
        )
    {
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
    if !research.should_analyze() {
        return Err(ReplanAnalysisFailure::PacketState);
    }
    let mut checkpoint = research.checkpoint.clone();
    let mut previous = checkpoint.work.clone();
    previous
        .begin_analysis(ResearchQuestionId::FIRST, research.packet())
        .map_err(|_| ReplanAnalysisFailure::PacketState)?;
    if let Some(need) = note.evidence_need {
        if !update.questions.is_empty() || !update.results.is_empty() {
            return Err(ReplanAnalysisFailure::WrongDecision);
        }
        let need = crate::ReplanEvidenceNeed::new(
            *need,
            windows
                .iter()
                .map(|w| ResearchResultSource {
                    source_id: w.source_id,
                    revision: w.revision.clone(),
                    range: w.range,
                })
                .collect(),
        )
        .map_err(ReplanAnalysisFailure::Decode)?;
        if !need.validates_originals(previous.objective(), &research.pages) {
            return Err(ReplanAnalysisFailure::UnboundEvidenceNeed);
        }
        checkpoint.work = previous;
        checkpoint.pending_need = Some(need);
    } else {
        checkpoint.work =
            crate::admit_research_work(previous.objective(), Some(&previous), update, &windows)
                .map_err(ReplanAnalysisFailure::Admission)?;
        checkpoint.pending_need = None;
    }
    Ok(checkpoint)
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
        json!({"schema_version":7, "response":{"kind":"interpretation","result":{
            "question_id":1,"text":"The function returns zero; verify the required return.","evidence":[{"anchor_ref":"E1"}]}}})
    }

    #[test]
    fn replan_admission_preserves_failure_stage_without_source_or_model_content() -> TestResult {
        let research = context()?;
        let before = research.clone();
        let mut missing = document();
        missing["response"]["result"]
            .as_object_mut()
            .ok_or("decision")?
            .remove("text");
        let mut invented = document();
        invented["response"]["result"]["evidence"][0]["anchor_ref"] = json!("E8");
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
        assert!(accepted.work.ready_to_finish());
        assert_eq!(
            accepted.work.questions()[0]
                .result()
                .ok_or("result")?
                .kind(),
            ResearchResultKind::Interpretation
        );
        assert_eq!(research, before);
        let no_evidence = json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["return"]}});
        let needed = admit(&no_evidence.to_string(), &research)?;
        assert!(!needed.work.ready_to_finish());
        assert!(needed.pending_need.is_some());
        Ok(())
    }

    #[test]
    fn replan_need_survives_reads_and_only_a_new_original_packet_can_resolve_it() -> TestResult {
        let mut research = context()?;
        let body = "return helper_b()\n";
        research.pages[0] = crate::AgentSourcePage::new(
            research.pages[0].revision().clone(),
            SourceRange::new(
                0,
                body.len(),
                SourcePosition::new(0, 0),
                SourcePosition::new(1, 0),
            )?,
            AgentFileStartLine::new(1)?,
            body.to_owned(),
            None,
            false,
        )?;
        let need = json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["helper_b"]}});
        for invalid in [
            json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["invented_target"]}}),
            json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":2,"targets":["helper_b"]}}),
            json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["helper_b","helper_b"]}}),
            json!({"schema_version":7,"response":{"kind":"evidenceNeed","question_id":1,"targets":["helper_b"],"result":{}}}),
            json!({"schema_version":5,"decision":{"kind":"progress"},"work":{"questions":[],"results":[]}}),
        ] {
            assert!(admit(&invalid.to_string(), &research).is_err());
        }
        research.checkpoint = admit(&need.to_string(), &research)?;
        let saved = research.checkpoint.clone();
        assert!(!research.should_analyze());
        assert!(!research.checkpoint.work.ready_to_finish());
        assert!(research.render().contains("helper_b"));
        assert!(!research.render().contains("return helper_b()"));
        assert_eq!(
            admit(&need.to_string(), &research),
            Err(ReplanAnalysisFailure::PacketState)
        );
        for query in ["helper_b", "caller", "definition", "tests"] {
            let action = a3_domain::AgentAction::Search(a3_domain::AgentSearchAction::new(
                a3_domain::AgentSearchQuery::try_from_string(query.to_owned())?,
                a3_domain::AgentSearchLimit::new(5)?,
            ));
            assert!(research.checkpoint.permits(&action));
            research.checkpoint.record_read(&action, true)?;
            assert_eq!(research.checkpoint.pending_need, saved.pending_need);
            assert!(!research.should_analyze());
        }
        assert_eq!(research.checkpoint.reads(), 4);
        let mut missing = research.clone();
        missing.pages.clear();
        assert!(!missing.validates_pending_need());
        assert!(!missing.render().contains("helper_b"));
        let mut falsified = research.clone();
        falsified.checkpoint.pending_need = Some(crate::ReplanEvidenceNeed::new(
            crate::ResearchEvidenceNeed::new(
                ResearchQuestionId::FIRST,
                vec!["invented_target".to_owned()],
            )?,
            saved
                .pending_need
                .as_ref()
                .ok_or("need")?
                .originals()
                .to_vec(),
        )?);
        assert!(!falsified.validates_pending_need());
        research.pages.push(crate::AgentSourcePage::new(
            FileRevision::new(
                RepositoryPath::try_from_bytes(b"helper.py".to_vec())?,
                ContentHash::from_bytes([9; 32]),
            ),
            SourceRange::new(0, 9, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
            AgentFileStartLine::new(1)?,
            "return 0\n".to_owned(),
            None,
            false,
        )?);
        assert!(research.should_analyze());
        let mut answer = document();
        answer["response"]["result"]["evidence"][0]["anchor_ref"] = json!("E2");
        research.checkpoint = admit(&answer.to_string(), &research)?;
        assert!(research.checkpoint.work.ready_to_finish());
        assert!(research.checkpoint.pending_need.is_none());
        assert_eq!(research.checkpoint.reads(), 4);
        assert_eq!(research.checkpoint.work.questions()[0].attempts().len(), 2);
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
            ReplanAnalysisFailure::UnboundEvidenceNeed,
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
