//! Proven original delivery before omitting prerequisite interpretation prose.
use super::AskResearchWorkingSet;
use a3_application::ResearchDesignBasis;
use a3_domain::{ResearchQuestionKind, ResearchResultKind};

impl AskResearchWorkingSet {
    pub(super) fn design_original_sources(&self) -> Vec<&a3_domain::ResearchResultSource> {
        let Some(work) = &self.work else {
            return vec![];
        };
        let Some(active) = work.next_question().and_then(|id| work.question(id)) else {
            return vec![];
        };
        active
            .definition()
            .dependencies
            .iter()
            .filter_map(|id| work.question(*id).and_then(|q| q.result()))
            .filter(|result| result.kind() != ResearchResultKind::DesignDecision)
            .flat_map(|result| result.sources())
            .collect()
    }
    pub(super) fn uses_original_design_basis(&self) -> bool {
        self.design_basis == ResearchDesignBasis::Originals
            && self.work.as_ref().is_some_and(|work| {
                super::research_work::core_plan_contract(work)
                    && work
                        .next_question()
                        .and_then(|id| work.question(id))
                        .is_some_and(|q| q.definition().kind == ResearchQuestionKind::Design)
            })
    }

    /// Cache coverage is insufficient: check exactly the packet about to reach the model.
    pub(super) fn design_originals_delivered(&self) -> bool {
        if !self.uses_original_design_basis() {
            return true;
        }
        let Some(work) = &self.work else { return false };
        let Some(active) = work.next_question().and_then(|id| work.question(id)) else {
            return false;
        };
        let windows = self.work_evidence_windows();
        active.definition().dependencies.iter().all(|id| {
            work.question(*id)
                .and_then(|q| q.result())
                .is_some_and(|result| {
                    result.kind() == ResearchResultKind::DesignDecision
                        || (!result.sources().is_empty()
                            && result.sources().iter().all(|source| {
                                windows.iter().any(|window| {
                                    window.revision == &source.revision
                                        && window.range.contains(source.range)
                                })
                            }))
                })
        })
    }
}
