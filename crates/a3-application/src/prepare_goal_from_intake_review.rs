//! Deterministic compilation of reviewed original ranges into an initial Goal proposal.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, CriterionOrigin, GoalConstraint, GoalContract, GoalContractDraft,
    GoalContractDraftError, GoalContractTextError, GoalIntakeError, GoalIntakeFinding,
    GoalIntakeFindingKind, GoalIntakeReview, GoalIntakeReviewError, GoalObjective,
    GoalOriginalBindingError, GoalOriginalRange, NonGoal, PendingGoalIntake, SuccessVerification,
    UserDecision,
};
use std::{error::Error, fmt};

/// Compiles a complete, question-free intake review for the exact supplied capture.
///
/// The normalized value of each finding must come from its literal original range.
/// The proposal retains original order and all typed evidence obligations. Reviewed
/// bytes with no attributed finding, including whitespace, still block promotion.
/// This structural compilation makes no claim about semantic classification or
/// passing verification. Use the existing promotion use case for current state and CAS.
pub fn prepare_goal_from_intake_review(
    pending: &PendingGoalIntake,
    review: &GoalIntakeReview,
) -> Result<GoalContract, PrepareGoalFromIntakeReviewFailure> {
    use PrepareGoalFromIntakeReviewFailure as Failure;

    if !review.is_fully_reviewed(pending).map_err(Failure::Review)? {
        return Err(Failure::Review(GoalIntakeReviewError::IncompleteReview));
    }
    if review.questions().next().is_some() {
        return Err(Failure::Review(GoalIntakeReviewError::OpenQuestions));
    }

    let original = pending.original();
    let mut findings = review.findings().collect::<Vec<_>>();
    findings.sort_by_key(|finding| {
        let range = finding.range();
        (range.start_byte(), range.end_byte())
    });
    let mut criteria = Vec::new();
    let mut constraints = Vec::new();
    let mut non_goals = Vec::new();
    let mut decisions = Vec::new();
    for finding in findings {
        let range = finding.range();
        let literal = original
            .resolve(range)
            .map_err(|_| Failure::Review(GoalIntakeReviewError::InvalidSource))?;
        let grounded = GoalIntakeFinding::new(finding.kind().clone(), range, literal.to_owned())
            .map_err(Failure::Review)?;
        if &grounded != finding {
            return Err(Failure::NonLiteralFinding);
        }

        let statement = finding.statement().to_owned();
        match finding.kind() {
            GoalIntakeFindingKind::Outcome {
                requirement,
                evidence_requirement,
            } => {
                let criterion = AcceptanceCriterion::with_requirement(
                    criterion_id(range),
                    AcceptanceCriterionStatement::try_from_string(statement)
                        .map_err(Failure::Text)?,
                    *requirement,
                )
                .with_origin(CriterionOrigin::new(range, evidence_requirement.clone()));
                criteria.push(criterion);
            }
            GoalIntakeFindingKind::Constraint => constraints.push(
                GoalConstraint::try_from_string(statement)
                    .map_err(Failure::Text)?
                    .with_origin(range),
            ),
            GoalIntakeFindingKind::NonGoal => non_goals.push(
                NonGoal::try_from_string(statement)
                    .map_err(Failure::Text)?
                    .with_origin(range),
            ),
            GoalIntakeFindingKind::ExistingUserDecision => decisions.push(
                UserDecision::try_from_string(statement)
                    .map_err(Failure::Text)?
                    .with_origin(range),
            ),
        }
    }

    let display = criteria
        .iter()
        .find(|criterion| criterion.requirement() == AcceptanceCriterionRequirement::Must)
        .or(criteria.first())
        .ok_or(Failure::MissingOutcome)?;
    let objective = GoalObjective::try_from_string(display.statement().as_str().to_owned())
        .map_err(Failure::Text)?;
    let success_verification = SuccessVerification::try_from_string(
        "Verify the acceptance criteria using their declared evidence requirements.".to_owned(),
    )
    .map_err(Failure::Text)?;
    let draft = GoalContractDraft::new(
        objective,
        criteria,
        constraints,
        non_goals,
        decisions,
        success_verification,
    )
    .map_err(Failure::Draft)?;
    let goal = GoalContract::initial(pending.task_id(), draft, pending.created_at())
        .with_original(original.clone())
        .map_err(Failure::OriginalBinding)?;
    pending
        .validate_promotion(&goal)
        .map_err(Failure::Promotion)?;
    Ok(goal)
}

fn criterion_id(range: GoalOriginalRange) -> AcceptanceCriterionId {
    let mut hasher = blake3::Hasher::new_derive_key("a3 intake criterion original range v1");
    hasher.update(range.goal().task_id().as_bytes());
    hasher.update(&range.goal().revision().get().to_le_bytes());
    hasher.update(range.digest().as_bytes());
    hasher.update(&range.start_byte().to_le_bytes());
    hasher.update(&range.end_byte().to_le_bytes());
    AcceptanceCriterionId::from_bytes(*hasher.finalize().as_bytes())
}

/// Closed preparation failures without original, finding or question text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareGoalFromIntakeReviewFailure {
    /// Review belongs to a different capture, remains incomplete or has open questions.
    Review(GoalIntakeReviewError),
    /// The finding's normalized text differs from its literal source.
    NonLiteralFinding,
    /// At least one outcome finding is required to build the initial Goal.
    MissingOutcome,
    /// A Goal display or entry value violates the existing text grammar.
    Text(GoalContractTextError),
    /// Goal collections violate their existing uniqueness or size limits.
    Draft(GoalContractDraftError),
    /// Proposed original and entry origins do not bind to the initial Goal.
    OriginalBinding(GoalOriginalBindingError),
    /// Existing promotion rules fail, including unassigned original bytes.
    Promotion(GoalIntakeError),
}

impl fmt::Display for PrepareGoalFromIntakeReviewFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Review(error) => write!(f, "cannot prepare goal from intake review: {error}"),
            Self::NonLiteralFinding => {
                f.write_str("intake finding differs from its literal original")
            }
            Self::MissingOutcome => f.write_str("intake review has no outcome"),
            Self::Text(error) => write!(f, "intake goal text is invalid: {error}"),
            Self::Draft(error) => write!(f, "intake goal draft is invalid: {error}"),
            Self::OriginalBinding(error) => {
                write!(f, "intake goal source binding is invalid: {error}")
            }
            Self::Promotion(error) => write!(f, "intake goal is incomplete or invalid: {error}"),
        }
    }
}

impl Error for PrepareGoalFromIntakeReviewFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Review(error) => Some(error),
            Self::Text(error) => Some(error),
            Self::Draft(error) => Some(error),
            Self::OriginalBinding(error) => Some(error),
            Self::Promotion(error) => Some(error),
            Self::NonLiteralFinding | Self::MissingOutcome => None,
        }
    }
}

#[cfg(test)]
mod tests;
