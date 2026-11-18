//! Append-only interpretation proposals; neither a Goal nor an execution receipt.
use super::{
    AcceptanceCriterionRequirement, AcceptanceCriterionStatement, CriterionEvidenceRequirement,
    GoalConstraint, GoalContract, GoalContractReference, GoalContractTimestamp, GoalOriginalDigest,
    GoalOriginalRange, NonGoal, PendingGoalIntake, UserDecision,
};
use std::{error::Error, fmt};

/// Maximum admitted source windows in one pending interpretation history.
pub const MAX_GOAL_INTAKE_REVIEW_BATCHES: usize = 128;
/// Maximum proposals in one small source-window result.
pub const MAX_GOAL_INTAKE_BATCH_FINDINGS: usize = 4;
/// Maximum questions added by one small result.
pub const MAX_GOAL_INTAKE_BATCH_QUESTIONS: usize = 1;
/// Maximum unresolved user choices in this slice, which has no resolution operation.
pub const MAX_GOAL_INTAKE_QUESTIONS: usize = 32;
/// Maximum total stored ChangeOnly path bytes across the complete interpretation.
pub const MAX_GOAL_INTAKE_REVIEW_PATH_BYTES: usize = 65_536;
const MAX_FINDINGS_PER_KIND: usize = 64;
const MAX_REVIEW_TEXT_BYTES: usize = 65_536;
const MAX_FINDING_TEXT_BYTES: usize = 2_048;
const MAX_QUESTION_TEXT_BYTES: usize = 512;
const MAX_QUESTION_RANGES: usize = 8;

/// Separate append-only Review CAS; capture CAS and execution budgets are not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalIntakeReviewRevision(u16);
impl GoalIntakeReviewRevision {
    /// Restores a nonempty history version, at most 128.
    pub fn new(value: u16) -> Result<Self, GoalIntakeReviewError> {
        if value == 0 || usize::from(value) > MAX_GOAL_INTAKE_REVIEW_BATCHES {
            return Err(GoalIntakeReviewError::HistoryLimit);
        }
        Ok(Self(value))
    }
    /// Returns the exact number of committed batches.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}
/// Core-assigned immutable question position; models never supply this identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalIntakeQuestionId(u16);
impl GoalIntakeQuestionId {
    /// Restores a previously Core-issued bounded position.
    pub fn new(value: u16) -> Result<Self, GoalIntakeReviewError> {
        if value == 0 || usize::from(value) > MAX_GOAL_INTAKE_QUESTIONS {
            return Err(GoalIntakeReviewError::QuestionLimit);
        }
        Ok(Self(value))
    }
    /// Returns its one-based position.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}
/// Proposed classification, never a model-selected fact or completion status.
#[derive(Clone, PartialEq, Eq)]
pub enum GoalIntakeFindingKind {
    /// Proposed result and proof obligation; no placeholder criterion is created.
    Outcome {
        /// Requiredness to preserve if this proposal is promoted unchanged.
        requirement: AcceptanceCriterionRequirement,
        /// Concrete original proof obligation to preserve, possibly explicitly unavailable.
        evidence_requirement: CriterionEvidenceRequirement,
    },
    /// Proposed mandatory task boundary.
    Constraint,
    /// Proposed exclusion.
    NonGoal,
    /// Proposal that this literal input already contains an explicit user choice.
    /// It is not a response to an unresolved question.
    ExistingUserDecision,
}
impl fmt::Debug for GoalIntakeFindingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Outcome { .. } => "Outcome([PROPOSED_OBLIGATION])",
            Self::Constraint => "Constraint",
            Self::NonGoal => "NonGoal",
            Self::ExistingUserDecision => "ExistingUserDecision",
        })
    }
}
impl GoalIntakeFindingKind {
    fn family(&self) -> usize {
        match self {
            Self::Outcome { .. } => 0,
            Self::Constraint => 1,
            Self::NonGoal => 2,
            Self::ExistingUserDecision => 3,
        }
    }
}
/// One bounded proposal, with exact original attribution but no semantic truth claim.
#[derive(Clone, PartialEq, Eq)]
pub struct GoalIntakeFinding {
    kind: GoalIntakeFindingKind,
    range: GoalOriginalRange,
    statement: String,
}
impl GoalIntakeFinding {
    /// Normalizes only the proposed value using its actual Goal value grammar.
    /// Original bytes and the source range are never normalized.
    pub fn new(
        kind: GoalIntakeFindingKind,
        range: GoalOriginalRange,
        statement: String,
    ) -> Result<Self, GoalIntakeReviewError> {
        if let GoalIntakeFindingKind::Outcome {
            evidence_requirement: CriterionEvidenceRequirement::ChangeOnly(diff),
            ..
        } = &kind
        {
            let total = diff
                .paths()
                .iter()
                .try_fold(0_usize, |sum, path| sum.checked_add(path.as_bytes().len()))
                .ok_or(GoalIntakeReviewError::RequirementPathLimit)?;
            if total > MAX_GOAL_INTAKE_REVIEW_PATH_BYTES {
                return Err(GoalIntakeReviewError::RequirementPathLimit);
            }
        }
        let statement = match &kind {
            GoalIntakeFindingKind::Outcome { .. } => {
                AcceptanceCriterionStatement::try_from_string(statement)
                    .map(|v| v.as_str().to_owned())
            }
            GoalIntakeFindingKind::Constraint => {
                GoalConstraint::try_from_string(statement).map(|v| v.as_str().to_owned())
            }
            GoalIntakeFindingKind::NonGoal => {
                NonGoal::try_from_string(statement).map(|v| v.as_str().to_owned())
            }
            GoalIntakeFindingKind::ExistingUserDecision => {
                UserDecision::try_from_string(statement).map(|v| v.as_str().to_owned())
            }
        }
        .map_err(|_| GoalIntakeReviewError::InvalidText)?;
        if statement.len() > MAX_FINDING_TEXT_BYTES {
            return Err(GoalIntakeReviewError::InvalidText);
        }
        Ok(Self {
            kind,
            range,
            statement,
        })
    }
    /// Returns the proposed classification and any proof obligation.
    #[must_use]
    pub const fn kind(&self) -> &GoalIntakeFindingKind {
        &self.kind
    }
    /// Returns the unchanged source binding.
    #[must_use]
    pub const fn range(&self) -> GoalOriginalRange {
        self.range
    }
    /// Returns the bounded proposed value, never an original-source substitute.
    #[must_use]
    pub fn statement(&self) -> &str {
        &self.statement
    }
}
impl fmt::Debug for GoalIntakeFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoalIntakeFinding")
            .field("kind", &self.kind)
            .field("range", &self.range)
            .field("statement_bytes", &self.statement.len())
            .finish()
    }
}
/// Only missing user intent or actual conflicting requirements justify an intake question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalIntakeQuestionKind {
    /// A user choice is needed; repository lookup is not a substitute for it.
    MissingUserChoice,
    /// Literal requirements conflict; no model-selected priority resolves them.
    ConflictingRequirements,
}
/// Bounded question proposal without an identity or model-selected status.
#[derive(Clone, PartialEq, Eq)]
pub struct GoalIntakeQuestionDraft {
    kind: GoalIntakeQuestionKind,
    text: String,
    ranges: Vec<GoalOriginalRange>,
}
impl GoalIntakeQuestionDraft {
    /// Preserves actual original references; aggregate admission validates their owner.
    pub fn new(
        kind: GoalIntakeQuestionKind,
        text: String,
        mut ranges: Vec<GoalOriginalRange>,
    ) -> Result<Self, GoalIntakeReviewError> {
        let text = text.trim().to_owned();
        if text.is_empty()
            || text.len() > MAX_QUESTION_TEXT_BYTES
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(GoalIntakeReviewError::InvalidText);
        }
        if ranges.is_empty() || ranges.len() > MAX_QUESTION_RANGES {
            return Err(GoalIntakeReviewError::InvalidQuestionRanges);
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|p| p[0] == p[1]) {
            return Err(GoalIntakeReviewError::InvalidQuestionRanges);
        }
        Ok(Self { kind, text, ranges })
    }
    /// Returns the proposed user-intent gap.
    #[must_use]
    pub const fn kind(&self) -> GoalIntakeQuestionKind {
        self.kind
    }
    /// Returns the bounded question for presentation.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Returns its canonical original references.
    #[must_use]
    pub fn ranges(&self) -> &[GoalOriginalRange] {
        &self.ranges
    }
}
impl fmt::Debug for GoalIntakeQuestionDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoalIntakeQuestionDraft")
            .field("kind", &self.kind)
            .field("text_bytes", &self.text.len())
            .field("ranges", &self.ranges)
            .finish()
    }
}
/// An unresolved immutable question. This slice deliberately has no resolve/delete operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalIntakeOpenQuestion {
    id: GoalIntakeQuestionId,
    draft: GoalIntakeQuestionDraft,
}
impl GoalIntakeOpenQuestion {
    /// Returns the Core-issued identity.
    #[must_use]
    pub const fn id(&self) -> GoalIntakeQuestionId {
        self.id
    }
    /// Returns the preserved original question proposal.
    #[must_use]
    pub const fn draft(&self) -> &GoalIntakeQuestionDraft {
        &self.draft
    }
    /// Restores a stored Core position; history restore checks the actual ordinal again.
    #[must_use]
    pub const fn reconstruct(id: GoalIntakeQuestionId, draft: GoalIntakeQuestionDraft) -> Self {
        Self { id, draft }
    }
}
/// One source window and all retained results, admitted or rejected together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalIntakeReviewBatchProposal {
    window: GoalOriginalRange,
    findings: Vec<GoalIntakeFinding>,
    questions: Vec<GoalIntakeQuestionDraft>,
}
impl GoalIntakeReviewBatchProposal {
    /// Checks local batch bounds only; source/admission checks require the pending aggregate.
    pub fn new(
        window: GoalOriginalRange,
        findings: Vec<GoalIntakeFinding>,
        questions: Vec<GoalIntakeQuestionDraft>,
    ) -> Result<Self, GoalIntakeReviewError> {
        if window.end_byte() - window.start_byte() > 2048 {
            return Err(GoalIntakeReviewError::WindowTooLarge);
        }
        if findings.len() > MAX_GOAL_INTAKE_BATCH_FINDINGS {
            return Err(GoalIntakeReviewError::FindingLimit);
        }
        if questions.len() > MAX_GOAL_INTAKE_BATCH_QUESTIONS {
            return Err(GoalIntakeReviewError::QuestionLimit);
        }
        Ok(Self {
            window,
            findings,
            questions,
        })
    }
    /// Returns the actual proposed review window.
    #[must_use]
    pub const fn window(&self) -> GoalOriginalRange {
        self.window
    }
    /// Returns proposed classifications, possibly empty for a considered source-only window.
    #[must_use]
    pub fn findings(&self) -> &[GoalIntakeFinding] {
        &self.findings
    }
    /// Returns only genuine unresolved user questions.
    #[must_use]
    pub fn questions(&self) -> &[GoalIntakeQuestionDraft] {
        &self.questions
    }
}
/// One immutable committed batch. Sources/results and question IDs cannot be replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalIntakeReviewBatch {
    window: GoalOriginalRange,
    findings: Vec<GoalIntakeFinding>,
    questions: Vec<GoalIntakeOpenQuestion>,
}
impl GoalIntakeReviewBatch {
    /// Restores bounded row shape. Full history restore revalidates source, ordinals and prefix.
    pub fn reconstruct(
        window: GoalOriginalRange,
        findings: Vec<GoalIntakeFinding>,
        questions: Vec<GoalIntakeOpenQuestion>,
    ) -> Result<Self, GoalIntakeReviewError> {
        GoalIntakeReviewBatchProposal::new(
            window,
            findings.clone(),
            questions.iter().map(|q| q.draft.clone()).collect(),
        )?;
        Ok(Self {
            window,
            findings,
            questions,
        })
    }
    /// Returns the exact considered window.
    #[must_use]
    pub const fn window(&self) -> GoalOriginalRange {
        self.window
    }
    /// Returns preserved proposals.
    #[must_use]
    pub fn findings(&self) -> &[GoalIntakeFinding] {
        &self.findings
    }
    /// Returns the permanent unresolved question records.
    #[must_use]
    pub fn questions(&self) -> &[GoalIntakeOpenQuestion] {
        &self.questions
    }
}
/// Append-only proposed interpretation, not a Goal, Ledger, Run or provider reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalIntakeReview {
    goal: GoalContractReference,
    digest: GoalOriginalDigest,
    created_at: GoalContractTimestamp,
    batches: Vec<GoalIntakeReviewBatch>,
}
impl GoalIntakeReview {
    /// Admits the first actual considered window and its results without a fake Goal draft.
    pub fn begin(
        pending: &PendingGoalIntake,
        proposal: GoalIntakeReviewBatchProposal,
    ) -> Result<Self, GoalIntakeReviewError> {
        let mut review = Self {
            goal: pending.reference(),
            digest: pending.original().digest(),
            created_at: pending.created_at(),
            batches: Vec::new(),
        };
        review.append(pending, proposal)?;
        Ok(review)
    }
    /// Revalidates every immutable batch, including Core ordinals; safe for post-promotion audit.
    pub fn restore(
        pending: &PendingGoalIntake,
        batches: Vec<GoalIntakeReviewBatch>,
    ) -> Result<Self, GoalIntakeReviewError> {
        if batches.is_empty() || batches.len() > MAX_GOAL_INTAKE_REVIEW_BATCHES {
            return Err(GoalIntakeReviewError::HistoryLimit);
        }
        check_path_budget(batches.iter().flat_map(|b| b.findings.iter()))?;
        let mut result = None;
        for batch in &batches {
            let proposal = GoalIntakeReviewBatchProposal::new(
                batch.window,
                batch.findings.clone(),
                batch.questions.iter().map(|q| q.draft.clone()).collect(),
            )?;
            match &mut result {
                None => result = Some(Self::begin(pending, proposal)?),
                Some(review) => review.append(pending, proposal)?,
            }
        }
        let result = result.ok_or(GoalIntakeReviewError::InvalidHistory)?;
        if result.batches != batches {
            return Err(GoalIntakeReviewError::InvalidHistory);
        }
        Ok(result)
    }
    /// Appends atomically in memory; no read count or model-selected status is accepted.
    pub fn append(
        &mut self,
        pending: &PendingGoalIntake,
        proposal: GoalIntakeReviewBatchProposal,
    ) -> Result<(), GoalIntakeReviewError> {
        self.validate_capture(pending)?;
        if self.batches.len() >= MAX_GOAL_INTAKE_REVIEW_BATCHES {
            return Err(GoalIntakeReviewError::HistoryLimit);
        }
        let original = pending.original();
        original
            .resolve(proposal.window)
            .map_err(|_| GoalIntakeReviewError::InvalidSource)?;
        if self
            .batches
            .iter()
            .any(|b| overlaps(b.window, proposal.window))
        {
            return Err(GoalIntakeReviewError::WindowAlreadyReviewed);
        }
        let mut counts = [0_usize; 4];
        let mut text_bytes = 0_usize;
        let mut previous_questions = Vec::new();
        let previous_findings = self
            .batches
            .iter()
            .flat_map(|b| b.findings.iter())
            .collect::<Vec<_>>();
        check_path_budget(
            previous_findings
                .iter()
                .copied()
                .chain(proposal.findings.iter()),
        )?;
        for f in &previous_findings {
            counts[f.kind.family()] += 1;
            text_bytes += f.statement.len()
        }
        for q in self.batches.iter().flat_map(|b| b.questions.iter()) {
            text_bytes += q.draft.text.len();
            previous_questions.push(q.draft.clone());
        }
        for (index, f) in proposal.findings.iter().enumerate() {
            original
                .resolve(f.range)
                .map_err(|_| GoalIntakeReviewError::InvalidSource)?;
            if !contains(proposal.window, f.range) {
                return Err(GoalIntakeReviewError::FindingOutsideWindow);
            }
            if previous_findings.contains(&f) || proposal.findings[..index].contains(f) {
                return Err(GoalIntakeReviewError::DuplicateFinding);
            }
            counts[f.kind.family()] += 1;
            text_bytes += f.statement.len();
        }
        if counts.iter().any(|c| *c > MAX_FINDINGS_PER_KIND) {
            return Err(GoalIntakeReviewError::FindingLimit);
        }
        if previous_questions.len() + proposal.questions.len() > MAX_GOAL_INTAKE_QUESTIONS {
            return Err(GoalIntakeReviewError::QuestionLimit);
        }
        let mut questions = Vec::new();
        for (index, q) in proposal.questions.iter().enumerate() {
            if previous_questions.contains(q) || proposal.questions[..index].contains(q) {
                return Err(GoalIntakeReviewError::DuplicateQuestion);
            }
            let mut current_anchor = false;
            for range in &q.ranges {
                original
                    .resolve(*range)
                    .map_err(|_| GoalIntakeReviewError::InvalidSource)?;
                if contains(proposal.window, *range) {
                    current_anchor = true
                } else if !self.batches.iter().any(|b| contains(b.window, *range)) {
                    return Err(GoalIntakeReviewError::InvalidQuestionRanges);
                }
            }
            if !current_anchor {
                return Err(GoalIntakeReviewError::InvalidQuestionRanges);
            }
            text_bytes += q.text.len();
            let ordinal = u16::try_from(previous_questions.len() + index + 1)
                .map_err(|_| GoalIntakeReviewError::QuestionLimit)?;
            questions.push(GoalIntakeOpenQuestion {
                id: GoalIntakeQuestionId::new(ordinal)?,
                draft: q.clone(),
            });
        }
        if text_bytes > MAX_REVIEW_TEXT_BYTES {
            return Err(GoalIntakeReviewError::TextLimit);
        }
        self.batches.push(GoalIntakeReviewBatch {
            window: proposal.window,
            findings: proposal.findings,
            questions,
        });
        Ok(())
    }
    /// Validates capture identity/digest/time without retaining another raw-source copy.
    pub fn validate_capture(
        &self,
        pending: &PendingGoalIntake,
    ) -> Result<(), GoalIntakeReviewError> {
        if self.goal != pending.reference()
            || self.digest != pending.original().digest()
            || self.created_at != pending.created_at()
        {
            return Err(GoalIntakeReviewError::CaptureMismatch);
        }
        Ok(())
    }
    /// Requires exactly one successor batch and unchanged audit history for the same capture.
    pub fn validate_successor_of(&self, previous: &Self) -> Result<(), GoalIntakeReviewError> {
        if self.goal != previous.goal
            || self.digest != previous.digest
            || self.created_at != previous.created_at
            || self.batches.len() != previous.batches.len() + 1
            || !self.batches.starts_with(&previous.batches)
        {
            return Err(GoalIntakeReviewError::InvalidHistory);
        }
        Ok(())
    }
    /// Enforces review preservation in addition to the existing actual raw/Goal coverage gate.
    pub fn validate_promotion(
        &self,
        pending: &PendingGoalIntake,
        goal: &GoalContract,
    ) -> Result<(), GoalIntakeReviewError> {
        self.validate_capture(pending)?;
        pending
            .validate_promotion(goal)
            .map_err(|_| GoalIntakeReviewError::InvalidPromotion)?;
        if self.questions().next().is_some() {
            return Err(GoalIntakeReviewError::OpenQuestions);
        }
        if !self.is_fully_reviewed(pending)? {
            return Err(GoalIntakeReviewError::IncompleteReview);
        }
        for finding in self.findings() {
            let present =
                match &finding.kind {
                    GoalIntakeFindingKind::Outcome {
                        requirement,
                        evidence_requirement,
                    } => goal.draft().acceptance_criteria().iter().any(|c| {
                        c.statement().as_str() == finding.statement
                            && c.requirement() == *requirement
                            && c.origin().is_some_and(|o| {
                                o.original_range() == finding.range
                                    && o.evidence_requirement() == evidence_requirement
                            })
                    }),
                    GoalIntakeFindingKind::Constraint => {
                        goal.draft().constraints().iter().any(|v| {
                            v.as_str() == finding.statement && v.origin() == Some(finding.range)
                        })
                    }
                    GoalIntakeFindingKind::NonGoal => goal.draft().non_goals().iter().any(|v| {
                        v.as_str() == finding.statement && v.origin() == Some(finding.range)
                    }),
                    GoalIntakeFindingKind::ExistingUserDecision => {
                        goal.draft().user_decisions().iter().any(|v| {
                            v.as_str() == finding.statement && v.origin() == Some(finding.range)
                        })
                    }
                };
            if !present {
                return Err(GoalIntakeReviewError::FindingOmitted);
            }
        }
        Ok(())
    }
    /// Derives considered-byte completeness only; never semantic truth or ready execution.
    pub fn is_fully_reviewed(
        &self,
        pending: &PendingGoalIntake,
    ) -> Result<bool, GoalIntakeReviewError> {
        self.validate_capture(pending)?;
        let mut ranges = self.batches.iter().map(|b| b.window).collect::<Vec<_>>();
        ranges.sort_unstable_by_key(|r| r.start_byte());
        let mut cursor = 0;
        for range in ranges {
            if range.start_byte() != cursor {
                return Ok(false);
            }
            cursor = range.end_byte();
        }
        Ok(cursor as usize == pending.original().as_bytes().len())
    }
    /// Returns the future initial Goal reference, not proof it exists.
    #[must_use]
    pub const fn goal(&self) -> GoalContractReference {
        self.goal
    }
    /// Returns the actual capture digest.
    #[must_use]
    pub const fn digest(&self) -> GoalOriginalDigest {
        self.digest
    }
    /// Returns the immutable capture timestamp.
    #[must_use]
    pub const fn created_at(&self) -> GoalContractTimestamp {
        self.created_at
    }
    /// Returns a checked nonempty-history CAS value.
    #[must_use]
    pub fn revision(&self) -> GoalIntakeReviewRevision {
        GoalIntakeReviewRevision(self.batches.len() as u16)
    }
    /// Returns all append-only batches in original commit order.
    #[must_use]
    pub fn batches(&self) -> &[GoalIntakeReviewBatch] {
        &self.batches
    }
    /// Returns all proposals in stable batch/position order.
    pub fn findings(&self) -> impl Iterator<Item = &GoalIntakeFinding> {
        self.batches.iter().flat_map(|b| b.findings.iter())
    }
    /// Returns every unresolved question, never discarding earlier questions.
    pub fn questions(&self) -> impl Iterator<Item = &GoalIntakeOpenQuestion> {
        self.batches.iter().flat_map(|b| b.questions.iter())
    }
}
fn check_path_budget<'a>(
    findings: impl Iterator<Item = &'a GoalIntakeFinding>,
) -> Result<(), GoalIntakeReviewError> {
    let mut bytes = 0_usize;
    for finding in findings {
        if let GoalIntakeFindingKind::Outcome {
            evidence_requirement: CriterionEvidenceRequirement::ChangeOnly(diff),
            ..
        } = &finding.kind
        {
            for path in diff.paths() {
                bytes = bytes
                    .checked_add(path.as_bytes().len())
                    .ok_or(GoalIntakeReviewError::RequirementPathLimit)?;
                if bytes > MAX_GOAL_INTAKE_REVIEW_PATH_BYTES {
                    return Err(GoalIntakeReviewError::RequirementPathLimit);
                }
            }
        }
    }
    Ok(())
}

fn contains(outer: GoalOriginalRange, inner: GoalOriginalRange) -> bool {
    outer.goal() == inner.goal()
        && outer.digest() == inner.digest()
        && outer.start_byte() <= inner.start_byte()
        && inner.end_byte() <= outer.end_byte()
}
fn overlaps(a: GoalOriginalRange, b: GoalOriginalRange) -> bool {
    a.start_byte() < b.end_byte() && b.start_byte() < a.end_byte()
}
/// Closed content-free classification. No question, user text or proposal enters errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalIntakeReviewError {
    /// Capture identity, digest or timestamp changed.
    CaptureMismatch,
    /// A source reference is foreign, stale or outside the original.
    InvalidSource,
    /// The actual review window exceeds 2048 bytes.
    WindowTooLarge,
    /// An already considered byte cannot be counted as new progress.
    WindowAlreadyReviewed,
    /// A proposal lies outside its newly considered window.
    FindingOutsideWindow,
    /// Exact repeated proposals do not count twice.
    DuplicateFinding,
    /// Exact repeated questions do not create new identities.
    DuplicateQuestion,
    /// Question anchors are missing, repeated, foreign or not considered.
    InvalidQuestionRanges,
    /// A proposed value or question violates its bounded grammar.
    InvalidText,
    /// A batch or total family exceeds its fixed finding bound.
    FindingLimit,
    /// A batch or complete history exceeds its fixed question bound.
    QuestionLimit,
    /// History is empty or exceeds 128 committed batches.
    HistoryLimit,
    /// Public proposed text exceeds 64 KiB across the history.
    TextLimit,
    /// ChangeOnly path payload exceeds 64 KiB across this review.
    RequirementPathLimit,
    /// Immutable prefix or Core question ordinals changed.
    InvalidHistory,
    /// Actual initial Goal does not pass existing exact capture/coverage admission.
    InvalidPromotion,
    /// Unresolved user intent cannot be silently promoted as confirmed decisions.
    OpenQuestions,
    /// Some original bytes were never considered, including whitespace.
    IncompleteReview,
    /// A recorded proposal disappeared or weakened without an explicit correction API.
    FindingOmitted,
}
impl fmt::Display for GoalIntakeReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CaptureMismatch => "intake review capture differs",
            Self::InvalidSource => "intake review source is invalid",
            Self::WindowTooLarge => "intake review window exceeds its bound",
            Self::WindowAlreadyReviewed => "intake review repeats considered bytes",
            Self::FindingOutsideWindow => "intake finding is outside its window",
            Self::DuplicateFinding => "intake review repeats a finding",
            Self::DuplicateQuestion => "intake review repeats a question",
            Self::InvalidQuestionRanges => "intake question ranges are invalid",
            Self::InvalidText => "intake review text is invalid",
            Self::FindingLimit => "intake review finding limit exceeded",
            Self::QuestionLimit => "intake review question limit exceeded",
            Self::HistoryLimit => "intake review history limit exceeded",
            Self::TextLimit => "intake review text limit exceeded",
            Self::RequirementPathLimit => "intake review requirement path limit exceeded",
            Self::InvalidHistory => "intake review history changed",
            Self::InvalidPromotion => "intake review initial promotion is invalid",
            Self::OpenQuestions => "intake review has open user questions",
            Self::IncompleteReview => "intake review original is not fully considered",
            Self::FindingOmitted => "intake review finding is missing from goal",
        })
    }
}
impl Error for GoalIntakeReviewError {}
