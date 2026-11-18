use super::{
    AcceptanceCriterionId, GoalContract, GoalOriginalEntry, GoalOriginalEntryKind,
    GoalOriginalError, GoalOriginalRange, GoalOriginalRequest,
};
use std::error::Error;
use std::fmt;

pub(super) fn validate_original_binding(
    goal: &GoalContract,
) -> Result<(), GoalOriginalBindingError> {
    let Some(original) = goal.original() else {
        return if goal
            .draft()
            .acceptance_criteria()
            .iter()
            .any(|criterion| criterion.origin().is_some())
            || goal
                .draft()
                .constraints()
                .iter()
                .any(|entry| entry.origin().is_some())
            || goal
                .draft()
                .non_goals()
                .iter()
                .any(|entry| entry.origin().is_some())
            || goal
                .draft()
                .user_decisions()
                .iter()
                .any(|entry| entry.origin().is_some())
        {
            Err(GoalOriginalBindingError::MissingOriginal)
        } else {
            Ok(())
        };
    };
    if original.goal() != goal.reference() {
        return Err(GoalOriginalBindingError::OriginalGoalMismatch);
    }
    for criterion in goal.draft().acceptance_criteria() {
        let origin =
            criterion
                .origin()
                .ok_or(GoalOriginalBindingError::MissingCriterionOrigin {
                    criterion_id: criterion.id(),
                })?;
        original.resolve(origin.original_range()).map_err(|error| {
            GoalOriginalBindingError::InvalidCriterionOrigin {
                criterion_id: criterion.id(),
                error,
            }
        })?;
    }
    validate_entries(
        original,
        GoalOriginalEntryKind::Constraint,
        goal.draft()
            .constraints()
            .iter()
            .map(|entry| entry.origin()),
    )?;
    validate_entries(
        original,
        GoalOriginalEntryKind::NonGoal,
        goal.draft().non_goals().iter().map(|entry| entry.origin()),
    )?;
    validate_entries(
        original,
        GoalOriginalEntryKind::UserDecision,
        goal.draft()
            .user_decisions()
            .iter()
            .map(|entry| entry.origin()),
    )?;
    Ok(())
}

fn validate_entries(
    original: &GoalOriginalRequest,
    kind: GoalOriginalEntryKind,
    origins: impl Iterator<Item = Option<GoalOriginalRange>>,
) -> Result<(), GoalOriginalBindingError> {
    for (index, origin) in origins.enumerate() {
        let entry = GoalOriginalEntry::from_index(kind, index)
            .ok_or(GoalOriginalBindingError::EntryLimitExceeded)?;
        if let Some(range) = origin {
            original
                .resolve(range)
                .map_err(|error| GoalOriginalBindingError::InvalidEntryOrigin { entry, error })?;
        }
    }
    Ok(())
}

// Callers validate both complete bindings first. Only the necessary owner-revision
// rebind is ignored; original bytes, span coordinates and all proof content stay material.
pub(super) fn same_original_revision_content(first: &GoalContract, next: &GoalContract) -> bool {
    let (Some(first_original), Some(next_original)) = (first.original(), next.original()) else {
        return false;
    };
    let first = first.draft();
    let next = next.draft();
    first_original.as_bytes() == next_original.as_bytes()
        && first_original.digest() == next_original.digest()
        && first.objective() == next.objective()
        && first.success_verification() == next.success_verification()
        && first.acceptance_criteria().len() == next.acceptance_criteria().len()
        && first
            .acceptance_criteria()
            .iter()
            .zip(next.acceptance_criteria())
            .all(|(a, b)| {
                a.id() == b.id()
                    && a.statement() == b.statement()
                    && a.requirement() == b.requirement()
                    && match (a.origin(), b.origin()) {
                        (Some(a), Some(b)) => {
                            same_range_content(Some(a.original_range()), Some(b.original_range()))
                                && a.evidence_requirement() == b.evidence_requirement()
                        }
                        (None, None) => true,
                        _ => false,
                    }
            })
        && first.constraints().len() == next.constraints().len()
        && first
            .constraints()
            .iter()
            .zip(next.constraints())
            .all(|(a, b)| a.as_str() == b.as_str() && same_range_content(a.origin(), b.origin()))
        && first.non_goals().len() == next.non_goals().len()
        && first
            .non_goals()
            .iter()
            .zip(next.non_goals())
            .all(|(a, b)| a.as_str() == b.as_str() && same_range_content(a.origin(), b.origin()))
        && first.user_decisions().len() == next.user_decisions().len()
        && first
            .user_decisions()
            .iter()
            .zip(next.user_decisions())
            .all(|(a, b)| a.as_str() == b.as_str() && same_range_content(a.origin(), b.origin()))
}

fn same_range_content(first: Option<GoalOriginalRange>, next: Option<GoalOriginalRange>) -> bool {
    match (first, next) {
        (Some(a), Some(b)) => {
            a.digest() == b.digest()
                && a.start_byte() == b.start_byte()
                && a.end_byte() == b.end_byte()
        }
        (None, None) => true,
        _ => false,
    }
}

/// Exact original and criterion provenance do not belong to one immutable goal revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOriginalBindingError {
    /// An entry's range belongs to different original bytes or another goal revision.
    InvalidEntryOrigin {
        /// Checked collection and one-based ordinal, without entry text.
        entry: GoalOriginalEntry,
        /// Closed content-free reason for rejecting the range.
        error: GoalOriginalError,
    },
    /// A non-criterion collection exceeded the supported ordinal boundary.
    EntryLimitExceeded,
    /// An original-bound criterion cannot be admitted as a source-less legacy goal.
    MissingOriginal,
    /// Every criterion in an original-backed goal requires explicit original provenance.
    MissingCriterionOrigin {
        /// The criterion whose provenance is absent; no statement text is exposed.
        criterion_id: AcceptanceCriterionId,
    },
    /// The original belongs to another task or immutable goal revision.
    OriginalGoalMismatch,
    /// A criterion's range does not resolve against this goal's exact original.
    InvalidCriterionOrigin {
        /// The criterion whose range could not be resolved.
        criterion_id: AcceptanceCriterionId,
        /// Closed content-free reason for the range rejection.
        error: GoalOriginalError,
    },
}

impl fmt::Display for GoalOriginalBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOriginal => formatter.write_str("goal origin requires an original"),
            Self::InvalidEntryOrigin { error, .. } => {
                write!(formatter, "goal entry original range is invalid: {error}")
            }
            Self::EntryLimitExceeded => formatter.write_str("goal original entry bound exceeded"),
            Self::MissingCriterionOrigin { .. } => {
                formatter.write_str("original-backed goal criterion has no origin")
            }
            Self::OriginalGoalMismatch => {
                formatter.write_str("goal original belongs to another revision")
            }
            Self::InvalidCriterionOrigin { error, .. } => write!(
                formatter,
                "goal criterion original range is invalid: {error}"
            ),
        }
    }
}

impl Error for GoalOriginalBindingError {}
