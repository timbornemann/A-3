//! Immutable original capture before a usable initial Goal Contract exists.

use super::{
    GoalContract, GoalContractReference, GoalContractRevision, GoalContractTimestamp,
    GoalOriginalCoverage, GoalOriginalRequest, TaskId,
};
use std::error::Error;
use std::fmt;

/// Captured original with no criteria, ledger, run, or permission to execute.
///
/// This is the pending state of the existing Goal lifecycle, not presentation history.
/// The reference identifies the future initial Goal; it does not prove that Goal exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingGoalIntake {
    original: GoalOriginalRequest,
    created_at: GoalContractTimestamp,
}

impl PendingGoalIntake {
    /// Binds an already validated exact original to its future initial Goal revision.
    pub fn new(
        original: GoalOriginalRequest,
        created_at: GoalContractTimestamp,
    ) -> Result<Self, GoalIntakeError> {
        if original.goal().revision() != GoalContractRevision::INITIAL {
            return Err(GoalIntakeError::InitialReferenceRequired);
        }
        Ok(Self {
            original,
            created_at,
        })
    }

    /// Returns the immutable exact original and its deterministic ordered parts.
    #[must_use]
    pub const fn original(&self) -> &GoalOriginalRequest {
        &self.original
    }

    /// Returns only the future initial identity, never proof of executable state.
    #[must_use]
    pub const fn reference(&self) -> GoalContractReference {
        self.original.goal()
    }

    /// Returns the task identity that must survive promotion unchanged.
    #[must_use]
    pub const fn task_id(&self) -> TaskId {
        self.reference().task_id()
    }

    /// Returns when the exact original was captured, before any intake model work.
    #[must_use]
    pub const fn created_at(&self) -> GoalContractTimestamp {
        self.created_at
    }

    /// Validates a complete initial proposal without changing pending or proposed state.
    ///
    /// Full byte coverage is necessary structure, never semantic truth or verification.
    /// Storage must repeat this check against its actual pending row in the CAS transaction.
    pub fn validate_promotion(&self, goal: &GoalContract) -> Result<(), GoalIntakeError> {
        if goal.revision() != GoalContractRevision::INITIAL
            || goal.previous_revision().is_some()
            || goal.revision_reason().is_some()
        {
            return Err(GoalIntakeError::InitialReferenceRequired);
        }
        if goal.task_id() != self.task_id() {
            return Err(GoalIntakeError::TaskMismatch);
        }
        if goal.created_at() != self.created_at {
            return Err(GoalIntakeError::CreatedAtMismatch);
        }
        goal.validate_original_binding()
            .map_err(|_| GoalIntakeError::InvalidOriginalBinding)?;
        let original = goal.original().ok_or(GoalIntakeError::MissingOriginal)?;
        if original.goal() != self.reference()
            || original.digest() != self.original.digest()
            || original.as_bytes() != self.original.as_bytes()
        {
            return Err(GoalIntakeError::OriginalMismatch);
        }
        let coverage = GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| GoalIntakeError::InvalidOriginalBinding)?;
        if !coverage.is_complete() {
            return Err(GoalIntakeError::IntakeIncomplete);
        }
        Ok(())
    }
}

/// Closed reasons why pending capture or initial promotion is invalid; no text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalIntakeError {
    /// Pending capture and promotion require exactly the initial Goal identity.
    InitialReferenceRequired,
    /// The proposed Goal belongs to a different task.
    TaskMismatch,
    /// Promotion changed the immutable capture timestamp.
    CreatedAtMismatch,
    /// A proposed original or entry range has an invalid aggregate binding.
    InvalidOriginalBinding,
    /// A clean source-less Goal cannot replace the captured exact original.
    MissingOriginal,
    /// The proposed Goal does not retain exactly the captured original bytes and owner.
    OriginalMismatch,
    /// Some original bytes or existing text entries remain unassigned.
    IntakeIncomplete,
}

impl fmt::Display for GoalIntakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InitialReferenceRequired => "goal intake requires an initial reference",
            Self::TaskMismatch => "goal intake task does not match",
            Self::CreatedAtMismatch => "goal intake capture time does not match",
            Self::InvalidOriginalBinding => "goal intake original binding is invalid",
            Self::MissingOriginal => "goal intake promotion requires the original",
            Self::OriginalMismatch => "goal intake original does not match",
            Self::IntakeIncomplete => "goal intake original assignment is incomplete",
        })
    }
}

impl Error for GoalIntakeError {}
