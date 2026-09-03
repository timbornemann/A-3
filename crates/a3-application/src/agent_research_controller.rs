use crate::{AskResearchAction, AskResearchDecisionNote};
use a3_domain::{AgentResearchDepth, AskResearchSourceId, FileRevision, IndexRunId, SnapshotId};
use blake3::Hasher;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

/// Immutable resource profile selected for one research section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchLimits {
    model_decisions: u8,
    read_actions: u8,
    duration_millis: u64,
    source_references: u16,
    repairs: u8,
}

impl ResearchLimits {
    /// Returns the fixed ADR-0038 profile for a user-selected depth.
    #[must_use]
    pub const fn for_depth(depth: AgentResearchDepth) -> Self {
        match depth {
            AgentResearchDepth::Standard => Self {
                model_decisions: 6,
                read_actions: 12,
                duration_millis: 5 * 60 * 1_000,
                source_references: 200,
                repairs: 1,
            },
            AgentResearchDepth::Thorough => Self {
                model_decisions: 12,
                read_actions: 24,
                duration_millis: 15 * 60 * 1_000,
                source_references: 200,
                repairs: 1,
            },
        }
    }

    #[must_use]
    /// Returns the maximum number of model decisions.
    pub const fn model_decisions(self) -> u8 {
        self.model_decisions
    }
    #[must_use]
    /// Returns the maximum number of requested read actions.
    pub const fn read_actions(self) -> u8 {
        self.read_actions
    }
    #[must_use]
    /// Returns the total monotone wall-clock limit.
    pub const fn duration_millis(self) -> u64 {
        self.duration_millis
    }
    #[must_use]
    /// Returns the maximum number of retained source references.
    pub const fn source_references(self) -> u16 {
        self.source_references
    }
    #[must_use]
    /// Returns the total structured-output repair allowance.
    pub const fn repairs(self) -> u8 {
        self.repairs
    }
}

/// Permission for the next bounded model decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginResearchDecision {
    /// The model may answer or request another bounded action batch.
    SearchAllowed,
    /// This is the final available decision and must not request more reads.
    FinalOnly,
}

/// One deduplicated sequential action batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchActionBatch {
    requested: u8,
    duplicate_count: u8,
    actions: Vec<AskResearchAction>,
}

impl ResearchActionBatch {
    #[must_use]
    /// Returns how many actions were charged for this batch.
    pub const fn requested(&self) -> u8 {
        self.requested
    }
    #[must_use]
    /// Returns how many requested actions were already executed.
    pub const fn duplicate_count(&self) -> u8 {
        self.duplicate_count
    }
    #[must_use]
    /// Returns the new actions that may be executed sequentially.
    pub fn actions(&self) -> &[AskResearchAction] {
        &self.actions
    }
}

/// Deterministic finite-state budget and stagnation guard for research execution.
#[derive(Debug, Clone)]
pub struct BoundedResearchController {
    limits: ResearchLimits,
    decisions_used: u8,
    actions_used: u8,
    repairs_used: u8,
    stagnant_rounds: u8,
    seen_actions: BTreeSet<AskResearchAction>,
}

impl BoundedResearchController {
    #[must_use]
    /// Creates a fresh controller for the selected fixed profile.
    pub fn new(depth: AgentResearchDepth) -> Self {
        Self {
            limits: ResearchLimits::for_depth(depth),
            decisions_used: 0,
            actions_used: 0,
            repairs_used: 0,
            stagnant_rounds: 0,
            seen_actions: BTreeSet::new(),
        }
    }

    /// Reserves the next decision using caller-supplied monotone elapsed time.
    pub fn begin_decision(
        &mut self,
        elapsed_millis: u64,
    ) -> Result<BeginResearchDecision, ResearchControllerError> {
        if elapsed_millis >= self.limits.duration_millis {
            return Err(ResearchControllerError::TimedOut);
        }
        if self.decisions_used >= self.limits.model_decisions {
            return Err(ResearchControllerError::DecisionBudgetExhausted);
        }
        self.decisions_used = self.decisions_used.saturating_add(1);
        let search_allowed = self.decisions_used < self.limits.model_decisions
            && self.actions_used < self.limits.read_actions
            && self.stagnant_rounds < 2;
        Ok(if search_allowed {
            BeginResearchDecision::SearchAllowed
        } else {
            BeginResearchDecision::FinalOnly
        })
    }

    /// Accounts for the single permitted structured-output repair.
    pub fn use_repair(&mut self) -> Result<(), ResearchControllerError> {
        if self.repairs_used >= self.limits.repairs {
            return Err(ResearchControllerError::RepairBudgetExhausted);
        }
        self.repairs_used = self.repairs_used.saturating_add(1);
        Ok(())
    }

    /// Consumes requested actions and returns only actions not executed before.
    pub fn prepare_actions(
        &mut self,
        actions: Vec<AskResearchAction>,
    ) -> Result<ResearchActionBatch, ResearchControllerError> {
        let requested = u8::try_from(actions.len())
            .map_err(|_| ResearchControllerError::ActionBudgetExhausted)?;
        if requested == 0
            || requested > 4
            || self.actions_used.saturating_add(requested) > self.limits.read_actions
        {
            return Err(ResearchControllerError::ActionBudgetExhausted);
        }
        self.actions_used = self.actions_used.saturating_add(requested);
        let mut unique = Vec::with_capacity(actions.len());
        for action in actions {
            if self.seen_actions.insert(action.clone()) {
                unique.push(action);
            }
        }
        let unique_count = u8::try_from(unique.len()).unwrap_or(u8::MAX);
        Ok(ResearchActionBatch {
            requested,
            duplicate_count: requested.saturating_sub(unique_count),
            actions: unique,
        })
    }

    /// Records whether one full action round produced new source Evidence.
    pub fn finish_round(&mut self, evidence_before: usize, evidence_after: usize) {
        if evidence_after > evidence_before {
            self.stagnant_rounds = 0;
        } else {
            self.stagnant_rounds = self.stagnant_rounds.saturating_add(1);
        }
    }

    #[must_use]
    /// Returns whether two consecutive rounds produced no new Evidence.
    pub const fn is_stagnant(&self) -> bool {
        self.stagnant_rounds >= 2
    }
    #[must_use]
    /// Returns the immutable selected limits.
    pub const fn limits(&self) -> ResearchLimits {
        self.limits
    }
    #[must_use]
    /// Returns the number of charged model decisions.
    pub const fn decisions_used(&self) -> u8 {
        self.decisions_used
    }
    #[must_use]
    /// Returns the number of charged read actions.
    pub const fn actions_used(&self) -> u8 {
        self.actions_used
    }
}

/// Epistemic kind stored in a deterministic memory checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchMemoryFindingKind {
    /// Directly observed current Evidence.
    Observation,
    /// Explicitly unproven search lead.
    Hypothesis,
    /// Conclusion supported by current Evidence.
    Conclusion,
}

/// One bounded public finding with its original source chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchMemoryFinding {
    /// Epistemic classification.
    pub kind: ResearchMemoryFindingKind,
    /// Bounded public finding text.
    pub text: String,
    /// Original current source chain.
    pub sources: Vec<AskResearchSourceId>,
}

/// Deterministic, content-bounded memory supplied before a research model turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchMemoryCheckpoint {
    digest: [u8; 32],
    question: String,
    findings: Vec<ResearchMemoryFinding>,
    gaps: Vec<String>,
}

impl ResearchMemoryCheckpoint {
    /// Builds and hashes one deterministic bounded checkpoint.
    pub fn build(
        question: String,
        findings: Vec<ResearchMemoryFinding>,
        gaps: Vec<String>,
    ) -> Result<Self, ResearchControllerError> {
        if !safe_text(&question, 4 * 1024)
            || findings.len() > 64
            || gaps.len() > 32
            || findings.iter().any(|finding| {
                !safe_text(&finding.text, 4 * 1024)
                    || finding.sources.len() > 32
                    || (finding.kind != ResearchMemoryFindingKind::Hypothesis
                        && finding.sources.is_empty())
            })
            || gaps.iter().any(|gap| !safe_text(gap, 1024))
        {
            return Err(ResearchControllerError::InvalidMemory);
        }
        let mut hasher = Hasher::new();
        hasher.update(b"a3.research-memory-checkpoint.v1\0");
        hash_text(&mut hasher, &question);
        for finding in &findings {
            hasher.update(&[match finding.kind {
                ResearchMemoryFindingKind::Observation => 1,
                ResearchMemoryFindingKind::Hypothesis => 2,
                ResearchMemoryFindingKind::Conclusion => 3,
            }]);
            hash_text(&mut hasher, &finding.text);
            for source in &finding.sources {
                hasher.update(source.as_bytes());
            }
        }
        for gap in &gaps {
            hash_text(&mut hasher, gap);
        }
        Ok(Self {
            digest: *hasher.finalize().as_bytes(),
            question,
            findings,
            gaps,
        })
    }

    #[must_use]
    /// Returns the deterministic checkpoint identity.
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    #[must_use]
    /// Returns the original current question.
    pub fn question(&self) -> &str {
        &self.question
    }
    #[must_use]
    /// Returns public findings with their original sources.
    pub fn findings(&self) -> &[ResearchMemoryFinding] {
        &self.findings
    }
    #[must_use]
    /// Returns unresolved Evidence gaps.
    pub fn gaps(&self) -> &[String] {
        &self.gaps
    }
}

/// Revalidated current research handed to Agent task materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchHandoff {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    revisions: Vec<FileRevision>,
}

impl ResearchHandoff {
    /// Creates a bounded handoff anchored to one published index.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        revisions: Vec<FileRevision>,
    ) -> Result<Self, ResearchControllerError> {
        if revisions.len() > 200 {
            return Err(ResearchControllerError::InvalidMemory);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            revisions,
        })
    }
    #[must_use]
    /// Returns the published index run anchor.
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    #[must_use]
    /// Returns the immutable repository snapshot anchor.
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    #[must_use]
    /// Returns the exact source revisions revalidated for handoff.
    pub fn revisions(&self) -> &[FileRevision] {
        &self.revisions
    }
}

/// Stable finite research-controller rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchControllerError {
    /// The selected monotone time budget elapsed.
    TimedOut,
    /// No further model decision is available.
    DecisionBudgetExhausted,
    /// The read-action budget or batch bound was exceeded.
    ActionBudgetExhausted,
    /// The sole structured-output repair was already consumed.
    RepairBudgetExhausted,
    /// A memory or handoff invariant was violated.
    InvalidMemory,
}

impl fmt::Display for ResearchControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded research controller rejected the transition")
    }
}
impl Error for ResearchControllerError {}

fn safe_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
}

fn hash_text(hasher: &mut Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

/// Converts a validated model note into checkpoint input without granting it authority.
#[must_use]
pub fn memory_finding_from_note(
    note: &AskResearchDecisionNote,
    sources: Vec<AskResearchSourceId>,
) -> ResearchMemoryFinding {
    let kind = match note.finding_kind {
        crate::AskResearchFindingKind::Observation => ResearchMemoryFindingKind::Observation,
        crate::AskResearchFindingKind::Hypothesis => ResearchMemoryFindingKind::Hypothesis,
        crate::AskResearchFindingKind::Conclusion => ResearchMemoryFindingKind::Conclusion,
    };
    ResearchMemoryFinding {
        kind,
        text: note.finding.clone(),
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(count: usize) -> Vec<AskResearchAction> {
        (0..count)
            .map(|index| AskResearchAction::SearchIndex(format!("query-{index}")))
            .collect()
    }

    #[test]
    fn profiles_match_adr_0038_limits() {
        let standard = ResearchLimits::for_depth(AgentResearchDepth::Standard);
        assert_eq!(
            (standard.model_decisions(), standard.read_actions()),
            (6, 12)
        );
        assert_eq!(standard.duration_millis(), 300_000);
        let thorough = ResearchLimits::for_depth(AgentResearchDepth::Thorough);
        assert_eq!(
            (thorough.model_decisions(), thorough.read_actions()),
            (12, 24)
        );
        assert_eq!(thorough.duration_millis(), 900_000);
    }

    #[test]
    fn duplicates_count_but_do_not_execute_twice() -> Result<(), Box<dyn Error>> {
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        assert_eq!(controller.prepare_actions(actions(2))?.actions().len(), 2);
        let duplicate = controller.prepare_actions(actions(2))?;
        assert_eq!(duplicate.duplicate_count(), 2);
        assert!(duplicate.actions().is_empty());
        assert_eq!(controller.actions_used(), 4);
        Ok(())
    }

    #[test]
    fn final_decision_and_stagnation_are_finite() -> Result<(), Box<dyn Error>> {
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        for turn in 0..5 {
            assert_eq!(
                controller.begin_decision(turn * 100)?,
                BeginResearchDecision::SearchAllowed
            );
        }
        assert_eq!(
            controller.begin_decision(500)?,
            BeginResearchDecision::FinalOnly
        );
        controller.finish_round(2, 2);
        controller.finish_round(2, 2);
        assert!(controller.is_stagnant());
        Ok(())
    }

    #[test]
    fn multi_round_symbol_caller_and_source_chain_stays_within_standard_budget()
    -> Result<(), Box<dyn Error>> {
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        assert_eq!(
            controller.begin_decision(0)?,
            BeginResearchDecision::SearchAllowed
        );
        let locate = controller.prepare_actions(vec![AskResearchAction::SearchIndex(
            "createTask".to_owned(),
        )])?;
        assert_eq!(locate.actions().len(), 1);
        controller.finish_round(0, 1);

        assert_eq!(
            controller.begin_decision(100)?,
            BeginResearchDecision::SearchAllowed
        );
        let callers = controller.prepare_actions(vec![AskResearchAction::InspectRelations {
            source_ordinal: 1,
            relation: crate::AskResearchRelation::Callers,
        }])?;
        assert_eq!(callers.actions().len(), 1);
        controller.finish_round(1, 3);

        assert_eq!(
            controller.begin_decision(200)?,
            BeginResearchDecision::SearchAllowed
        );
        let sources = controller.prepare_actions(vec![
            AskResearchAction::InspectSource(1),
            AskResearchAction::InspectSource(2),
            AskResearchAction::InspectSource(3),
        ])?;
        assert_eq!(sources.actions().len(), 3);
        controller.finish_round(3, 6);
        assert_eq!(controller.decisions_used(), 3);
        assert_eq!(controller.actions_used(), 5);
        Ok(())
    }

    #[test]
    fn standard_action_timeout_and_repair_limits_are_hard() -> Result<(), Box<dyn Error>> {
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        for round in 0..3 {
            let offset = round * 4;
            controller.prepare_actions(
                actions(4)
                    .into_iter()
                    .map(|action| match action {
                        AskResearchAction::SearchIndex(query) => {
                            AskResearchAction::SearchIndex(format!("{query}-{offset}"))
                        }
                        other => other,
                    })
                    .collect(),
            )?;
        }
        assert_eq!(controller.actions_used(), 12);
        assert!(matches!(
            controller.prepare_actions(actions(1)),
            Err(ResearchControllerError::ActionBudgetExhausted)
        ));
        controller.use_repair()?;
        assert!(matches!(
            controller.use_repair(),
            Err(ResearchControllerError::RepairBudgetExhausted)
        ));
        assert!(matches!(
            controller.begin_decision(300_000),
            Err(ResearchControllerError::TimedOut)
        ));
        Ok(())
    }

    #[test]
    fn checkpoint_rejects_unbacked_observation() {
        let finding = ResearchMemoryFinding {
            kind: ResearchMemoryFindingKind::Observation,
            text: "gesehen".to_owned(),
            sources: Vec::new(),
        };
        assert!(
            ResearchMemoryCheckpoint::build("frage".to_owned(), vec![finding], vec![]).is_err()
        );
    }
}
