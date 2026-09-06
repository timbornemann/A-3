//! Atomic admission of V5 proposals against the original bytes actually delivered.
use crate::ResearchWorkUpdate;
use a3_domain::{
    AskResearchSourceId, FileRevision, ResearchResult, ResearchResultKind, ResearchResultSource,
    ResearchWorkState, SourcePosition, SourceRange,
};
use std::error::Error;
use std::fmt;

/// One original current source window delivered to the model, not merely cached.
pub struct ResearchEvidenceWindow<'a> {
    /// Issued only when this exact window appears in the current packet with an E label.
    pub anchor: Option<crate::ResearchEvidenceAnchorId>,
    /// Issued turn-local source number.
    pub ordinal: u16,
    /// Stable source identity.
    pub source_id: AskResearchSourceId,
    /// Hash-bound original file revision.
    pub revision: &'a FileRevision,
    /// Exact original range of the window.
    pub range: SourceRange,
    /// Original bytes; never persisted by admission.
    pub text: &'a str,
}

/// Applies a whole proposed update or nothing. The returned aggregate remains non-executable.
pub fn admit_research_work(
    objective: &str,
    previous: Option<&ResearchWorkState>,
    update: &ResearchWorkUpdate,
    windows: &[ResearchEvidenceWindow<'_>],
) -> Result<ResearchWorkState, ResearchWorkAdmissionError> {
    if update
        .questions
        .iter()
        .any(|q| q.outcome.contains(['【', '】']))
    {
        return Err(ResearchWorkAdmissionError::ContractChanged);
    }
    let mut state = match previous {
        Some(state) => {
            if state.objective() != objective
                || (!update.questions.is_empty()
                    && !state
                        .questions()
                        .iter()
                        .map(|q| q.definition())
                        .eq(update.questions.iter()))
            {
                return Err(ResearchWorkAdmissionError::ContractChanged);
            }
            state.clone()
        }
        None => {
            let mut questions = update.questions.clone();
            let mut end = objective.len().min(2048);
            while !objective.is_char_boundary(end) {
                end -= 1;
            }
            for question in &mut questions {
                // New phase documents do not ask the model to copy the user request.
                // Bind the question to the original goal; historical literal fragments stay exact.
                if question.request_fragment.is_empty() {
                    question.request_fragment = objective[..end].to_owned();
                }
            }
            ResearchWorkState::new(objective.to_owned(), questions)
                .map_err(|_| ResearchWorkAdmissionError::ContractChanged)?
        }
    };
    let mut results = update.results.iter().collect::<Vec<_>>();
    results.sort_by_key(|r| r.question_id);
    for proposal in results {
        if proposal.text.contains("【") || proposal.text.contains("】") {
            return Err(ResearchWorkAdmissionError::UnsupportedResult);
        }
        let question = state
            .question(proposal.question_id)
            .ok_or(ResearchWorkAdmissionError::UnknownQuestion)?;
        // Idempotent result repetition preserves its original evidence; it cannot rewrite history.
        if question.resolved() {
            if question
                .result()
                .is_some_and(|r| r.kind() == proposal.kind && r.text() == proposal.text)
            {
                continue;
            }
            return Err(ResearchWorkAdmissionError::ContractChanged);
        }
        let mut sources = Vec::with_capacity(proposal.evidence.len());
        if proposal
            .evidence
            .len()
            .saturating_add(proposal.anchors.len())
            > 32
        {
            return Err(ResearchWorkAdmissionError::UnsupportedResult);
        }
        for anchor in &proposal.anchors {
            let mut selected = None;
            for window in windows.iter().filter(|w| w.anchor == Some(*anchor)) {
                if window.text.is_empty()
                    || usize::try_from(
                        window
                            .range
                            .end_byte()
                            .saturating_sub(window.range.start_byte()),
                    )
                    .ok()
                        != Some(window.text.len())
                    || position_after(window.range.start_position(), window.text)
                        != Some(window.range.end_position())
                {
                    return Err(ResearchWorkAdmissionError::UndeliveredQuote);
                }
                let source = a3_domain::ResearchResultSource {
                    source_id: window.source_id,
                    revision: window.revision.clone(),
                    range: window.range,
                };
                if selected
                    .as_ref()
                    .is_some_and(|old: &a3_domain::ResearchResultSource| {
                        old.revision != source.revision || old.range != source.range
                    })
                {
                    return Err(ResearchWorkAdmissionError::AmbiguousQuote);
                }
                selected = Some(source);
            }
            let source = selected.ok_or(ResearchWorkAdmissionError::UndeliveredQuote)?;
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        for quote in &proposal.evidence {
            if quote.quote.trim().is_empty() || quote.quote.len() > 512 {
                return Err(ResearchWorkAdmissionError::UndeliveredQuote);
            }
            let mut admitted = None;
            for window in windows
                .iter()
                .filter(|window| window.ordinal == quote.source_ordinal)
            {
                for (offset, _) in window.text.match_indices(&quote.quote) {
                    let source = admit_quote(window, offset, &quote.quote)
                        .ok_or(ResearchWorkAdmissionError::UndeliveredQuote)?;
                    if admitted.as_ref().is_some_and(
                        |previous: &a3_domain::ResearchResultSource| {
                            previous.revision != source.revision || previous.range != source.range
                        },
                    ) {
                        return Err(ResearchWorkAdmissionError::AmbiguousQuote);
                    }
                    if admitted.is_none() {
                        admitted = Some(source);
                    }
                }
            }
            let source = admitted.ok_or(ResearchWorkAdmissionError::UndeliveredQuote)?;
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        let boundary = if proposal.kind == ResearchResultKind::BoundedUnknown {
            Some(
                *question
                    .exclusions()
                    .last()
                    .ok_or(ResearchWorkAdmissionError::UnexaminedUnknown)?,
            )
        } else {
            None
        };
        let result = ResearchResult::new(proposal.kind, proposal.text.clone(), sources, boundary)
            .map_err(|_| ResearchWorkAdmissionError::UnsupportedResult)?;
        state
            .resolve(proposal.question_id, result)
            .map_err(|_| ResearchWorkAdmissionError::UnsupportedResult)?;
    }
    Ok(state)
}

fn admit_quote(
    window: &ResearchEvidenceWindow<'_>,
    offset: usize,
    quote: &str,
) -> Option<ResearchResultSource> {
    let before = &window.text[..offset];
    let start = position_after(window.range.start_position(), before)?;
    let end = position_after(start, quote)?;
    let start_byte = window
        .range
        .start_byte()
        .checked_add(u32::try_from(offset).ok()?)?;
    let end_byte = start_byte.checked_add(u32::try_from(quote.len()).ok()?)?;
    if end_byte > window.range.end_byte() {
        return None;
    }
    Some(ResearchResultSource {
        source_id: window.source_id,
        revision: window.revision.clone(),
        range: SourceRange::new(
            usize::try_from(start_byte).ok()?,
            usize::try_from(end_byte).ok()?,
            start,
            end,
        )
        .ok()?,
    })
}

fn position_after(start: SourcePosition, text: &str) -> Option<SourcePosition> {
    let lines = u32::try_from(text.bytes().filter(|b| *b == b'\n').count()).ok()?;
    let column = match text.rsplit_once('\n') {
        Some((_, last)) => u32::try_from(last.len()).ok()?,
        None => start
            .column()
            .checked_add(u32::try_from(text.len()).ok()?)?,
    };
    Some(SourcePosition::new(start.row().checked_add(lines)?, column))
}

/// Safe repair reason. Neither quotes nor raw model content enter diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchWorkAdmissionError {
    /// Quote matches multiple distinct original locations; use a more specific quote.
    AmbiguousQuote,
    /// The original contract was missing, invalid, or replaced.
    ContractChanged,
    /// A result refers to a question the Core has not issued.
    UnknownQuestion,
    /// The cited original bytes were not present in the current model packet.
    UndeliveredQuote,
    /// No actual exhausted investigation supports the claimed unknown.
    UnexaminedUnknown,
    /// The result does not satisfy its kind or dependency requirements.
    UnsupportedResult,
}
impl fmt::Display for ResearchWorkAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("research result was not admitted")
    }
}
impl Error for ResearchWorkAdmissionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResearchQuoteProposal, ResearchResultProposal};
    use a3_domain::{
        ContentHash, RepositoryPath, ResearchQuestionDraft, ResearchQuestionId,
        ResearchQuestionKind, ResearchQuestionPriority,
    };
    #[test]
    fn source_label_without_delivered_original_bytes_is_not_evidence() -> Result<(), Box<dyn Error>>
    {
        let query = "Where is the log?";
        let update = ResearchWorkUpdate {
            questions: vec![ResearchQuestionDraft {
                request_fragment: query.to_owned(),
                outcome: "log destination".to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Repository,
                dependencies: vec![],
            }],
            results: vec![ResearchResultProposal {
                anchors: vec![],
                question_id: ResearchQuestionId::new(1)?,
                kind: ResearchResultKind::Interpretation,
                text: "Default: audit.txt".to_owned(),
                evidence: vec![ResearchQuoteProposal {
                    source_ordinal: 1,
                    quote: "audit.txt".to_owned(),
                }],
            }],
        };
        assert_eq!(
            admit_research_work(query, None, &update, &[]),
            Err(ResearchWorkAdmissionError::UndeliveredQuote)
        );
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"audit.py".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let window = ResearchEvidenceWindow {
            anchor: None,
            ordinal: 1,
            source_id: AskResearchSourceId::from_bytes([1; 32]),
            revision: &revision,
            range: SourceRange::new(0, 22, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
            text: "log_path = 'audit.txt'\n",
        };
        let state = admit_research_work(query, None, &update, &[window])?;
        assert!(state.ready_to_finish());
        assert_eq!(
            state.questions()[0]
                .result()
                .ok_or("missing result")?
                .sources()[0]
                .range
                .start_byte(),
            12
        );
        Ok(())
    }

    #[test]
    fn delivered_window_anchor_is_core_bound_and_cannot_select_a_caption_or_forged_range()
    -> Result<(), Box<dyn Error>> {
        let query = "Where is the log?";
        let anchor = crate::ResearchEvidenceAnchorId::new(1)?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"audit.py".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let mut window = ResearchEvidenceWindow {
            anchor: Some(anchor),
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([4; 32]),
            revision: &revision,
            range: SourceRange::new(
                100,
                113,
                SourcePosition::new(8, 0),
                SourcePosition::new(8, 13),
            )?,
            text: "audit_log.txt",
        };
        let update = ResearchWorkUpdate {
            questions: vec![ResearchQuestionDraft {
                request_fragment: String::new(),
                outcome: "Log target".to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Repository,
                dependencies: vec![],
            }],
            results: vec![ResearchResultProposal {
                question_id: ResearchQuestionId::FIRST,
                kind: ResearchResultKind::Interpretation,
                text: "Writes to audit_log.txt".to_owned(),
                evidence: vec![],
                anchors: vec![anchor],
            }],
        };
        assert!(admit_research_work(query, None, &update, &[]).is_err());
        let admitted = admit_research_work(query, None, &update, std::slice::from_ref(&window))?;
        assert_eq!(admitted.questions()[0].definition().request_fragment, query);
        assert_eq!(
            admitted.questions()[0].result().ok_or("result")?.sources()[0].range,
            window.range
        );
        window.text = "caption only";
        assert!(admit_research_work(query, None, &update, std::slice::from_ref(&window)).is_err());
        assert!(crate::ResearchEvidenceAnchorId::new(0).is_err());
        assert!(crate::ResearchEvidenceAnchorId::new(9).is_err());
        Ok(())
    }

    #[test]
    fn unique_original_quote_uses_exact_utf8_bytes_and_rejects_ambiguous_or_wrong_alias()
    -> Result<(), Box<dyn Error>> {
        let query = "log destination";
        let mut update = ResearchWorkUpdate {
            questions: vec![ResearchQuestionDraft {
                request_fragment: query.to_owned(),
                outcome: query.to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Repository,
                dependencies: vec![],
            }],
            results: vec![ResearchResultProposal {
                anchors: vec![],
                question_id: ResearchQuestionId::new(1)?,
                kind: ResearchResultKind::Interpretation,
                text: "The configured destination is used".to_owned(),
                evidence: vec![ResearchQuoteProposal {
                    source_ordinal: 2,
                    quote: "audit.txt".to_owned(),
                }],
            }],
        };
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"audit.py".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let text = "# ä\npath = 'audit.txt'\nother = 'audit.txt'\n";
        let windows = [ResearchEvidenceWindow {
            anchor: None,
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([4; 32]),
            revision: &revision,
            range: SourceRange::new(
                100,
                100 + text.len(),
                SourcePosition::new(8, 0),
                SourcePosition::new(11, 0),
            )?,
            text,
        }];
        assert_eq!(
            admit_research_work(query, None, &update, &windows),
            Err(ResearchWorkAdmissionError::AmbiguousQuote)
        );
        update.results[0].evidence[0].quote = "path = 'audit.txt'".to_owned();
        update.results[0].evidence[0].source_ordinal = 1;
        assert_eq!(
            admit_research_work(query, None, &update, &windows),
            Err(ResearchWorkAdmissionError::UndeliveredQuote)
        );
        update.results[0].evidence[0].source_ordinal = 2;
        let admitted = admit_research_work(query, None, &update, &windows)?;
        let source = &admitted.questions()[0].result().ok_or("result")?.sources()[0];
        assert_eq!(source.range.start_byte(), 105);
        assert_eq!(source.range.start_position(), SourcePosition::new(9, 0));
        assert_eq!(source.range.end_byte(), 123);
        assert_eq!(source.source_id, windows[0].source_id);
        let overlap = ResearchEvidenceWindow {
            anchor: None,
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([5; 32]),
            revision: windows[0].revision,
            range: windows[0].range,
            text: windows[0].text,
        };
        let overlap_windows = [windows.into_iter().next().ok_or("window")?, overlap];
        let overlap_result = admit_research_work(query, None, &update, &overlap_windows)?;
        assert_eq!(
            overlap_result.questions()[0]
                .result()
                .ok_or("overlap result")?
                .sources()
                .len(),
            1
        );
        Ok(())
    }
}
