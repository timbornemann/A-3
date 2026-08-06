use crate::{JobContext, ProgressReportError};
use a3_domain::{
    PatchAction, PatchChangeSet, PatchPreview, PolicyDecision, PolicyDecisionId,
    PolicyDecisionOutcome, PolicyDecisionReason, Progress, ProjectIdentity, PublishedIndex,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Future returned by the object-safe read-only patch-preview port.
pub type PatchPreviewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PatchPreview, PatchPreviewFailure>> + Send + 'a>>;

/// Future returned by the object-safe mutating patch-apply port.
pub type PatchApplyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PatchChangeSet, PatchApplyFailure>> + Send + 'a>>;

/// Cooperative cancellation and bounded progress for preview and application.
pub trait WorkspacePatchControl: fmt::Debug + Send + Sync {
    /// Returns whether the owner cancelled the operation.
    fn is_cancelled(&self) -> bool;

    /// Reports bounded progress before returning a terminal result.
    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError>;
}

impl WorkspacePatchControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        JobContext::report_progress(self, progress).map_err(Into::into)
    }
}

/// The owning scheduler no longer accepts patch progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspacePatchProgressError;

impl fmt::Display for WorkspacePatchProgressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace patch progress is unavailable")
    }
}

impl Error for WorkspacePatchProgressError {}

impl From<ProgressReportError> for WorkspacePatchProgressError {
    fn from(_value: ProgressReportError) -> Self {
        Self
    }
}

/// Non-cloneable capability proving one exact patch received one exact allowed central decision.
pub struct AuthorizedPatchAction {
    action: PatchAction,
    policy_decision_id: PolicyDecisionId,
}

impl AuthorizedPatchAction {
    /// Binds an approval-granted decision to the exact patch fingerprint and scope.
    pub fn new(
        action: PatchAction,
        decision: &PolicyDecision,
    ) -> Result<Self, PatchAuthorizationError> {
        let policy_action = action.policy_action();
        if decision.run_id() != action.run_id() {
            return Err(PatchAuthorizationError::RunMismatch);
        }
        if decision.outcome() != PolicyDecisionOutcome::Allowed
            || decision.reason() != PolicyDecisionReason::ApprovalGranted
        {
            return Err(PatchAuthorizationError::NotExplicitlyApproved);
        }
        if decision.action_fingerprint() != policy_action.fingerprint()
            || decision.scope_digest() != policy_action.scope_digest()
            || decision.action_class() != policy_action.class()
            || decision.risk_level() != policy_action.risk()
        {
            return Err(PatchAuthorizationError::ActionMismatch);
        }
        Ok(Self {
            action,
            policy_decision_id: decision.id(),
        })
    }

    /// Returns the exact approved action for adapter preflight.
    #[must_use]
    pub const fn action(&self) -> &PatchAction {
        &self.action
    }

    /// Consumes the one-shot capability before mutation begins.
    #[must_use]
    pub fn into_parts(self) -> (PatchAction, PolicyDecisionId) {
        (self.action, self.policy_decision_id)
    }
}

impl fmt::Debug for AuthorizedPatchAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizedPatchAction")
            .field("action", &self.action)
            .field("policy_decision_id", &self.policy_decision_id)
            .finish()
    }
}

/// A central policy decision did not authorize this exact patch once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchAuthorizationError {
    /// Decision belonged to another run.
    RunMismatch,
    /// Decision was not allowed by a consumed explicit approval.
    NotExplicitlyApproved,
    /// Fingerprint, scope, class, or risk differed from the patch.
    ActionMismatch,
}

impl fmt::Display for PatchAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RunMismatch => "patch authorization belongs to another run",
            Self::NotExplicitlyApproved => "patch authorization lacks explicit approval",
            Self::ActionMismatch => "patch authorization does not match the action",
        })
    }
}

impl Error for PatchAuthorizationError {}

/// Narrow workspace capability for safe preview and one-shot patch application.
pub trait WorkspacePatchTool: fmt::Debug + Send + Sync {
    /// Revalidates the published snapshot and live filesystem, then returns a bounded preview.
    fn preview<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        action: &'a PatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchPreviewFuture<'a>;

    /// Consumes exact authorization, revalidates again, and returns actual post-write evidence.
    fn apply<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        authorized: AuthorizedPatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchApplyFuture<'a>;
}

/// Stable preview failure without paths, file content, or OS diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchPreviewFailure {
    /// Worktree, path class, or canonical root policy denied the action.
    Denied,
    /// Action no longer targets the current published snapshot.
    StaleSnapshot,
    /// Live absence or expected hash disagreed with the action.
    Conflict,
    /// Owner cancelled before a complete preview existed.
    Cancelled,
    /// Progress could not be delivered.
    ProgressUnavailable,
    /// Filesystem data was unavailable.
    Unavailable,
    /// Adapter could not construct the canonical bounded preview.
    InvalidResult,
}

impl fmt::Display for PatchPreviewFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "workspace patch preview was denied",
            Self::StaleSnapshot => "workspace patch snapshot is stale",
            Self::Conflict => "workspace patch preview found a concurrent change",
            Self::Cancelled => "workspace patch preview was cancelled",
            Self::ProgressUnavailable => "workspace patch preview progress is unavailable",
            Self::Unavailable => "workspace patch preview source is unavailable",
            Self::InvalidResult => "workspace patch preview is invalid",
        })
    }
}

impl Error for PatchPreviewFailure {}

/// Stable apply failure; any actual change is explicit and must be invalidated before reasoning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchApplyFailure {
    /// Worktree, path class, or canonical root policy denied the action.
    Denied,
    /// Action no longer targets the current published snapshot.
    StaleSnapshot,
    /// Live absence or expected hash disagreed before mutation.
    Conflict,
    /// Another mutation already owns this worktree.
    Busy,
    /// Owner cancelled before mutation began.
    Cancelled,
    /// Progress could not be delivered.
    ProgressUnavailable,
    /// Filesystem mutation was unavailable before any change.
    Unavailable,
    /// Adapter could not construct the canonical result.
    InvalidResult,
    /// The worktree changed before a terminal success could be reported; exact evidence is retained.
    Changed(Box<PatchChangeSet>),
}

impl fmt::Display for PatchApplyFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "workspace patch application was denied",
            Self::StaleSnapshot => "workspace patch snapshot is stale",
            Self::Conflict => "workspace patch application found a concurrent change",
            Self::Busy => "another worktree mutation is active",
            Self::Cancelled => "workspace patch application was cancelled",
            Self::ProgressUnavailable => "workspace patch progress is unavailable",
            Self::Unavailable => "workspace patch application is unavailable",
            Self::InvalidResult => "workspace patch result is invalid",
            Self::Changed(_) => "workspace patch changed the worktree before reporting success",
        })
    }
}

impl Error for PatchApplyFailure {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest,
        ApprovalRequestId, PatchActionSchemaVersion, PatchAdd, PatchFileContent, PatchRationale,
        PolicyDecision, PolicyEvaluationTiming, RepositoryPath, SnapshotId, TaskStepId,
        VerificationSpecId, WorktreeId,
    };

    #[test]
    fn authorization_is_exact_and_requires_consumed_approval() -> Result<(), Box<dyn Error>> {
        let first_action = action(b"src/one.rs")?;
        let policy_action = first_action.policy_action();
        let requested_at = AgentRunTimestamp::from_unix_millis(10)?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([7; 32]),
            first_action.run_id(),
            &policy_action,
            requested_at,
            AgentRunTimestamp::from_unix_millis(100)?,
        )?;
        let mut grant = ApprovalGrant::grant(
            ApprovalId::from_bytes([8; 32]),
            &request,
            AgentRunTimestamp::from_unix_millis(11)?,
        )?;
        let decision_id = PolicyDecisionId::from_bytes([9; 32]);
        let decided_at = AgentRunTimestamp::from_unix_millis(12)?;
        grant.consume(
            decision_id,
            first_action.run_id(),
            &policy_action,
            decided_at,
        )?;
        let decision = PolicyDecision::approved(
            decision_id,
            first_action.run_id(),
            &policy_action,
            &grant,
            PolicyEvaluationTiming::new(decided_at, decided_at)?,
        )?;
        assert!(AuthorizedPatchAction::new(first_action, &decision).is_ok());
        assert!(matches!(
            AuthorizedPatchAction::new(action(b"src/two.rs")?, &decision),
            Err(PatchAuthorizationError::ActionMismatch)
        ));
        Ok(())
    }

    fn action(path: &[u8]) -> Result<PatchAction, Box<dyn Error>> {
        Ok(PatchAction::new(
            PatchActionSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            SnapshotId::from_bytes([3; 32]),
            TaskStepId::from_bytes([4; 32]),
            VerificationSpecId::from_bytes([5; 32]),
            PatchRationale::try_from_string("apply approved patch".to_owned())?,
            vec![a3_domain::PatchOperation::Add(PatchAdd::new(
                RepositoryPath::try_from_bytes(path.to_vec())?,
                PatchFileContent::try_from_bytes(b"pub fn added() {}\n".to_vec())?,
            ))],
        )?)
    }
}
