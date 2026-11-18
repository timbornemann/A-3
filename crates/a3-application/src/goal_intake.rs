//! Capture and atomic promotion through the existing Goal store, with no model work.

use crate::{GoalContractStore, GoalContractStoreFailure};
use a3_domain::{
    GoalContract, GoalContractReference, GoalContractTimestamp, GoalIntakeError, GoalOriginalError,
    GoalOriginalRequest, PendingGoalIntake, ProjectIdentity, TaskId,
};
use std::error::Error;
use std::fmt;

/// Fixed bounded discovery page. Listing never loads original request payloads.
pub const PENDING_GOAL_INTAKE_PAGE_SIZE: usize = 32;

/// A keyset page of task identities awaiting interpretation in one opened project.
///
/// Identities are discovery hints only. A later task-bound load must revalidate
/// Pending before dispatch; a promotion may remove an item between page reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingGoalIntakePage {
    task_ids: Vec<TaskId>,
    next_after: Option<TaskId>,
}

impl PendingGoalIntakePage {
    /// Validates at most one bounded lookahead row, then discards that lookahead.
    /// The supplied rows must be strictly ordered after the requested cursor.
    pub fn from_lookahead(
        after: Option<TaskId>,
        mut task_ids: Vec<TaskId>,
    ) -> Result<Self, PendingGoalIntakePageError> {
        if task_ids.len() > PENDING_GOAL_INTAKE_PAGE_SIZE + 1
            || task_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || after.is_some_and(|cursor| task_ids.first().is_some_and(|id| *id <= cursor))
        {
            return Err(PendingGoalIntakePageError);
        }
        let has_more = task_ids.len() > PENDING_GOAL_INTAKE_PAGE_SIZE;
        if has_more {
            task_ids.pop();
        }
        let next_after = if has_more {
            task_ids.last().copied()
        } else {
            None
        };
        Ok(Self {
            task_ids,
            next_after,
        })
    }

    /// Returns at most 32 task identities and no source bytes or model state.
    #[must_use]
    pub fn task_ids(&self) -> &[TaskId] {
        &self.task_ids
    }

    /// Returns the last delivered identity only if a later row was observed.
    #[must_use]
    pub const fn next_after(&self) -> Option<TaskId> {
        self.next_after
    }
}

/// A discovery page exceeded its bounds or violated keyset order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingGoalIntakePageError;

impl fmt::Display for PendingGoalIntakePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("pending goal intake page is unordered or outside its bounds")
    }
}

impl Error for PendingGoalIntakePageError {}

/// Nonzero SQLite-representable CAS version of immutable pending original capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalIntakeStoreVersion(u64);

impl GoalIntakeStoreVersion {
    /// Version assigned only to newly captured pending state.
    pub const INITIAL: Self = Self(1);

    /// Reconstructs a checked persisted CAS version; it is not a model budget or progress count.
    pub const fn new(value: u64) -> Result<Self, GoalIntakeStoreVersionError> {
        if value == 0 || value > i64::MAX as u64 {
            Err(GoalIntakeStoreVersionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Pending store version was zero or exceeded signed persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalIntakeStoreVersionError;

impl fmt::Display for GoalIntakeStoreVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("goal intake store version is outside its persistence range")
    }
}

impl Error for GoalIntakeStoreVersionError {}

/// One validated immutable original paired with its consumed-on-promotion CAS version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPendingGoalIntake {
    pending: PendingGoalIntake,
    version: GoalIntakeStoreVersion,
}

impl StoredPendingGoalIntake {
    /// Wraps domain-validated pending state after persistence revalidated its stored fields.
    #[must_use]
    pub const fn new(pending: PendingGoalIntake, version: GoalIntakeStoreVersion) -> Self {
        Self { pending, version }
    }

    /// Returns the retained original capture; it contains no executable draft.
    #[must_use]
    pub const fn pending(&self) -> &PendingGoalIntake {
        &self.pending
    }

    /// Returns the exact optimistic version consumed only by successful promotion.
    #[must_use]
    pub const fn version(&self) -> GoalIntakeStoreVersion {
        self.version
    }
}

/// One task-bound consistent read, not a second committed original authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalIntakeLoadResult {
    /// Original capture exists, but no initial Goal has been materialized.
    Pending(StoredPendingGoalIntake),
    /// The actual initial Goal exists. This proves neither prior capture history nor
    /// source completeness; existing Goal reads and execution admission remain required.
    Materialized(GoalContractReference),
}

/// Captures exact original bytes once before interpretation or model dispatch.
///
/// No model, Ledger, Run, retry, pending listing, or intake progress is created here.
#[derive(Debug, Clone, Copy)]
pub struct CaptureGoalIntake<'a> {
    store: &'a dyn GoalContractStore,
}

impl<'a> CaptureGoalIntake<'a> {
    /// Uses the existing identity-bound Goal persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn GoalContractStore) -> Self {
        Self { store }
    }

    /// Validates raw UTF-8 and size before the first store call, then creates pending once.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        task_id: TaskId,
        bytes: Vec<u8>,
        created_at: GoalContractTimestamp,
    ) -> Result<StoredPendingGoalIntake, CaptureGoalIntakeFailure> {
        let original =
            GoalOriginalRequest::try_from_bytes(GoalContractReference::initial(task_id), bytes)
                .map_err(CaptureGoalIntakeFailure::InvalidOriginal)?;
        let pending = PendingGoalIntake::new(original, created_at)
            .map_err(CaptureGoalIntakeFailure::InvalidPending)?;
        let version = self
            .store
            .capture_goal_intake(project, &pending)
            .await
            .map_err(CaptureGoalIntakeFailure::Store)?;
        if version != GoalIntakeStoreVersion::INITIAL {
            return Err(CaptureGoalIntakeFailure::Store(
                GoalContractStoreFailure::InvalidStoredData,
            ));
        }
        Ok(StoredPendingGoalIntake::new(pending, version))
    }
}

/// Capture failed without any model work or automatic create retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureGoalIntakeFailure {
    /// Raw bytes violate the exact UTF-8/length contract before any persistence call.
    InvalidOriginal(GoalOriginalError),
    /// Pending state did not bind the future initial Goal identity.
    InvalidPending(GoalIntakeError),
    /// The adapter rejected create-only capture or could not persist it.
    Store(GoalContractStoreFailure),
}

impl fmt::Display for CaptureGoalIntakeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidOriginal(_) => "goal intake raw original is invalid",
            Self::InvalidPending(_) => "goal intake pending state is invalid",
            Self::Store(_) => "goal intake capture could not be persisted",
        })
    }
}

impl Error for CaptureGoalIntakeFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOriginal(error) => Some(error),
            Self::InvalidPending(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}

/// Validates a real initial proposal, then requests its atomic pending-to-Goal promotion.
#[derive(Debug, Clone, Copy)]
pub struct PromoteGoalIntake<'a> {
    store: &'a dyn GoalContractStore,
}

impl<'a> PromoteGoalIntake<'a> {
    /// Uses the same existing Goal authority as capture and later Goal revision reads.
    #[must_use]
    pub const fn new(store: &'a dyn GoalContractStore) -> Self {
        Self { store }
    }

    /// Reads and validates once; the adapter revalidates/CAS-writes in one transaction.
    ///
    /// A lost commit acknowledgement is handled by an explicit later load. This method
    /// neither retries nor pretends a materialized reference proves prior promotion.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_version: GoalIntakeStoreVersion,
        goal: &GoalContract,
    ) -> Result<GoalContractReference, PromoteGoalIntakeFailure> {
        let state = self
            .store
            .load_goal_intake(project, task_id)
            .await
            .map_err(PromoteGoalIntakeFailure::Store)?;
        let stored = match state {
            Some(GoalIntakeLoadResult::Pending(stored)) => stored,
            Some(GoalIntakeLoadResult::Materialized(reference)) => {
                if reference != GoalContractReference::initial(task_id) {
                    return Err(PromoteGoalIntakeFailure::Store(
                        GoalContractStoreFailure::InvalidStoredData,
                    ));
                }
                return Err(PromoteGoalIntakeFailure::Store(
                    GoalContractStoreFailure::IntakeNotPending,
                ));
            }
            None => {
                return Err(PromoteGoalIntakeFailure::Store(
                    GoalContractStoreFailure::IntakeNotPending,
                ));
            }
        };
        if stored.pending().reference() != GoalContractReference::initial(task_id) {
            return Err(PromoteGoalIntakeFailure::Store(
                GoalContractStoreFailure::InvalidStoredData,
            ));
        }
        if stored.version() != expected_version {
            return Err(PromoteGoalIntakeFailure::Store(
                GoalContractStoreFailure::IntakeVersionConflict,
            ));
        }
        stored
            .pending()
            .validate_promotion(goal)
            .map_err(PromoteGoalIntakeFailure::InvalidPromotion)?;
        if let Some(review) = self
            .store
            .load_goal_intake_review(project, task_id)
            .await
            .map_err(PromoteGoalIntakeFailure::Store)?
        {
            review
                .validate_promotion(stored.pending(), goal)
                .map_err(PromoteGoalIntakeFailure::InvalidReview)?;
        }
        self.store
            .promote_goal_intake(project, task_id, expected_version, goal)
            .await
            .map_err(PromoteGoalIntakeFailure::Store)?;
        Ok(goal.reference())
    }
}

/// Promotion failed before write admission or at the adapter's atomic CAS boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromoteGoalIntakeFailure {
    /// Present interpretation history is incomplete or its recorded findings were lost.
    InvalidReview(a3_domain::GoalIntakeReviewError),
    /// The proposed Goal changed pending identity/content or has incomplete source assignment.
    InvalidPromotion(GoalIntakeError),
    /// Read, expected-version check, already-materialized state or atomic write failed.
    Store(GoalContractStoreFailure),
}

impl fmt::Display for PromoteGoalIntakeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPromotion(_) => "goal intake initial proposal is invalid",
            Self::InvalidReview(_) => "goal intake interpretation is incomplete or changed",
            Self::Store(_) => "goal intake promotion could not be committed",
        })
    }
}

impl Error for PromoteGoalIntakeFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPromotion(error) => Some(error),
            Self::InvalidReview(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}
