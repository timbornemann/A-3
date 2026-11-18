//! Durable small interpretation results; no provider, execution owner or fake Goal draft.
use crate::{
    GoalContractStore, GoalContractStoreFailure, GoalIntakeLoadResult, GoalIntakeStoreVersion,
};
use a3_domain::{
    GoalIntakeReview, GoalIntakeReviewBatchProposal, GoalIntakeReviewError,
    GoalIntakeReviewRevision, ProjectIdentity, TaskId,
};
use std::{error::Error, fmt};
/// Appends one source-bound result through the existing Goal persistence capability.
#[derive(Debug, Clone, Copy)]
pub struct RecordGoalIntakeReview<'a> {
    store: &'a dyn GoalContractStore,
}
impl<'a> RecordGoalIntakeReview<'a> {
    /// Uses the same store as immutable capture and eventual atomic promotion.
    #[must_use]
    pub const fn new(store: &'a dyn GoalContractStore) -> Self {
        Self { store }
    }
    /// Loads once and proposes exactly one successor. Adapter repeats actual CAS/prefix
    /// and source admission in its transaction. A lost acknowledgement is not retried.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_capture_version: GoalIntakeStoreVersion,
        expected_review_revision: Option<GoalIntakeReviewRevision>,
        proposal: GoalIntakeReviewBatchProposal,
    ) -> Result<GoalIntakeReview, RecordGoalIntakeReviewFailure> {
        let loaded = self.store.load_goal_intake(project, task_id).await?;
        let Some(GoalIntakeLoadResult::Pending(stored)) = loaded else {
            return Err(RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::IntakeNotPending,
            ));
        };
        if stored.pending().task_id() != task_id {
            return Err(RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::InvalidStoredData,
            ));
        }
        if stored.version() != expected_capture_version {
            return Err(RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::IntakeVersionConflict,
            ));
        }
        let previous = self.store.load_goal_intake_review(project, task_id).await?;
        if previous.as_ref().map(GoalIntakeReview::revision) != expected_review_revision {
            return Err(RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::IntakeReviewVersionConflict,
            ));
        }
        let review = match previous {
            None => GoalIntakeReview::begin(stored.pending(), proposal)?,
            Some(mut previous) => {
                previous.append(stored.pending(), proposal)?;
                previous
            }
        };
        let actual = self
            .store
            .append_goal_intake_review(
                project,
                task_id,
                expected_capture_version,
                expected_review_revision,
                &review,
            )
            .await?;
        if actual != review.revision() {
            return Err(RecordGoalIntakeReviewFailure::Store(
                GoalContractStoreFailure::InvalidStoredData,
            ));
        }
        Ok(review)
    }
}
/// Closed admission/read/write failure. No input, finding or question text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordGoalIntakeReviewFailure {
    /// Source, bounds, novelty or prefix rejected a proposed interpretation update.
    InvalidReview(GoalIntakeReviewError),
    /// Actual capture/review CAS or storage rejected the operation.
    Store(GoalContractStoreFailure),
}
impl fmt::Display for RecordGoalIntakeReviewFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidReview(_) => "goal intake review proposal is invalid",
            Self::Store(_) => "goal intake review could not be persisted",
        })
    }
}
impl Error for RecordGoalIntakeReviewFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidReview(e) => Some(e),
            Self::Store(e) => Some(e),
        }
    }
}
impl From<GoalIntakeReviewError> for RecordGoalIntakeReviewFailure {
    fn from(value: GoalIntakeReviewError) -> Self {
        Self::InvalidReview(value)
    }
}
impl From<GoalContractStoreFailure> for RecordGoalIntakeReviewFailure {
    fn from(value: GoalContractStoreFailure) -> Self {
        Self::Store(value)
    }
}
