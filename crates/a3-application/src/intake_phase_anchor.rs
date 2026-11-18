//! Content-free identity of one bounded interpretation of a pending original.

use crate::{GoalIntakeStoreVersion, StoredPendingGoalIntake};
use a3_domain::{
    ContentHash, GoalContractReference, GoalContractTimestamp, GoalIntakeReviewRevision,
    GoalOriginalError, GoalOriginalRange, MAX_GOAL_ORIGINAL_PART_BYTES, ModelProfileReference,
    ProjectIdentity, WorktreeId,
};
use std::{error::Error, fmt};

/// Exact original, review position and profile for one non-executable intake job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntakePhaseAnchor {
    worktree: WorktreeId,
    capture_version: GoalIntakeStoreVersion,
    review_revision: Option<GoalIntakeReviewRevision>,
    window: GoalOriginalRange,
    captured_at: GoalContractTimestamp,
    model_profile: ModelProfileReference,
    job_digest: ContentHash,
}

impl IntakePhaseAnchor {
    /// Resolves the window against the complete immutable Pending original.
    pub fn new(
        project: &ProjectIdentity,
        capture: &StoredPendingGoalIntake,
        review_revision: Option<GoalIntakeReviewRevision>,
        window: GoalOriginalRange,
        model_profile: ModelProfileReference,
    ) -> Result<Self, IntakePhaseAnchorFailure> {
        let original = capture.pending().original();
        let source = original
            .resolve(window)
            .map_err(IntakePhaseAnchorFailure::Original)?;
        if source.len() > MAX_GOAL_ORIGINAL_PART_BYTES {
            return Err(IntakePhaseAnchorFailure::WindowTooLarge);
        }
        let worktree = project.worktree().id();
        let capture_version = capture.version();
        let captured_at = capture.pending().created_at();
        let mut digest = blake3::Hasher::new_derive_key("a3 intake phase window v1");
        digest.update(worktree.as_bytes());
        digest.update(window.goal().task_id().as_bytes());
        digest.update(&window.goal().revision().get().to_le_bytes());
        digest.update(window.digest().as_bytes());
        digest.update(&capture_version.get().to_le_bytes());
        digest.update(
            &review_revision
                .map_or(0, GoalIntakeReviewRevision::get)
                .to_le_bytes(),
        );
        digest.update(&window.start_byte().to_le_bytes());
        digest.update(&window.end_byte().to_le_bytes());
        digest.update(&captured_at.unix_millis().to_le_bytes());
        digest.update(model_profile.id().as_bytes());
        digest.update(&model_profile.version().get().to_le_bytes());
        digest.update(source.as_bytes());
        Ok(Self {
            worktree,
            capture_version,
            review_revision,
            window,
            captured_at,
            model_profile,
            job_digest: ContentHash::from_bytes(*digest.finalize().as_bytes()),
        })
    }

    /// Returns the scoped worktree.
    #[must_use]
    pub const fn worktree(self) -> WorktreeId {
        self.worktree
    }

    /// Returns the future initial Goal reference.
    #[must_use]
    pub const fn goal(self) -> GoalContractReference {
        self.window.goal()
    }

    /// Returns the exact pending capture CAS.
    #[must_use]
    pub const fn capture_version(self) -> GoalIntakeStoreVersion {
        self.capture_version
    }

    /// Returns the expected Review predecessor.
    #[must_use]
    pub const fn review_revision(self) -> Option<GoalIntakeReviewRevision> {
        self.review_revision
    }

    /// Returns the selected exact source range without raw source text.
    #[must_use]
    pub const fn window(self) -> GoalOriginalRange {
        self.window
    }

    /// Returns the original time from which the whole task deadline is measured.
    #[must_use]
    pub const fn captured_at(self) -> GoalContractTimestamp {
        self.captured_at
    }

    /// Returns the profile to latch before the first provider call.
    #[must_use]
    pub const fn model_profile(self) -> ModelProfileReference {
        self.model_profile
    }

    /// Returns the logical job digest, independent of the encoded provider body.
    #[must_use]
    pub const fn job_digest(self) -> ContentHash {
        self.job_digest
    }
}

/// The proposed source range cannot identify one bounded pending interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakePhaseAnchorFailure {
    /// Range owner, digest or UTF-8 coordinates do not match the exact Pending source.
    Original(GoalOriginalError),
    /// The original range exceeds the fixed interpretation window.
    WindowTooLarge,
}

impl fmt::Display for IntakePhaseAnchorFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Original(_) => "intake window does not match the pending original",
            Self::WindowTooLarge => "intake window crossed the source bound",
        })
    }
}

impl Error for IntakePhaseAnchorFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Original(error) => Some(error),
            Self::WindowTooLarge => None,
        }
    }
}
