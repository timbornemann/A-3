//! Bounded navigation memory, never an admitted result or an executable request.
use crate::{AgentSourcePage, AskResearchDecisionDecodeError, ResearchEvidenceNeed};
use a3_domain::{ContentHash, ResearchQuestionId, ResearchResultSource, ResearchWorkState};

/// A proposed lexical frontier bound to the exact original packet that requested it.
#[derive(Clone, PartialEq, Eq)]
pub struct ReplanEvidenceNeed {
    need: ResearchEvidenceNeed,
    originals: Vec<ResearchResultSource>,
}

impl std::fmt::Debug for ReplanEvidenceNeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplanEvidenceNeed")
            .field("targets", &self.need.targets().len())
            .field("originals", &self.originals.len())
            .finish_non_exhaustive()
    }
}

impl ReplanEvidenceNeed {
    /// Validates shape only. Admission and rehydration must also validate original bytes.
    pub fn new(
        need: ResearchEvidenceNeed,
        originals: Vec<ResearchResultSource>,
    ) -> Result<Self, AskResearchDecisionDecodeError> {
        if need.question() != ResearchQuestionId::FIRST
            || originals.is_empty()
            || originals.len() > 8
            || originals.iter().enumerate().any(|(i, source)| {
                source.range.is_empty()
                    || originals[..i].iter().any(|prior| {
                        prior.revision == source.revision && prior.range == source.range
                    })
            })
        {
            return Err(AskResearchDecisionDecodeError::InvalidValue);
        }
        Ok(Self { need, originals })
    }

    /// Lexical hints, not proven existence, policy or tool arguments.
    #[must_use]
    pub const fn need(&self) -> &ResearchEvidenceNeed {
        &self.need
    }

    /// The actual analyzed packet, retained without source text.
    #[must_use]
    pub fn originals(&self) -> &[ResearchResultSource] {
        &self.originals
    }

    /// Same canonical packet identity used by the analysis duplicate guard.
    #[must_use]
    pub fn packet(&self) -> ContentHash {
        crate::replan_research::original_packet(
            self.originals.iter().map(|s| (&s.revision, s.range)),
        )
    }

    /// A pending need must retain its unresolved question and actual packet attempt.
    #[must_use]
    pub fn validates_work(&self, work: &ResearchWorkState) -> bool {
        !work.ready_to_finish()
            && work
                .question(self.need.question())
                .is_some_and(|q| q.result().is_none() && q.attempts().contains(&self.packet()))
    }

    /// Rechecks literal origin against the exact current Safe Reader windows, also on restart.
    #[must_use]
    pub fn validates_originals(&self, objective: &str, pages: &[AgentSourcePage]) -> bool {
        self.originals.iter().all(|s| {
            pages
                .iter()
                .any(|p| p.revision() == &s.revision && p.range() == s.range)
        }) && self.need.targets().iter().all(|target| {
            objective.contains(target)
                || pages.iter().any(|p| {
                    self.originals
                        .iter()
                        .any(|s| p.revision() == &s.revision && p.range() == s.range)
                        && p.text().contains(target)
                })
        })
    }
}
