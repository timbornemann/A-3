use crate::EvaluatedPolicyAction;
use a3_domain::{
    AgentRun, AgentRunError, AgentRunTimestamp, ApprovalGrant, ApprovalGrantError,
    ApprovalGrantState, ApprovalId, ApprovalRequest, ApprovalRequestId, ApprovalRevokeError,
    PolicyDecision, PolicyDecisionId, ProjectIdentity, RunEvent, RunEventCode, RunEventId,
    RunEventKind, RunEventOutcome, RunEventPayload, RunEventSequence, SnapshotId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe policy and approval persistence port.
pub type PolicyStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PolicyStoreFailure>> + Send + 'a>>;

/// Persistence boundary for decisions, approval lifecycle, and their append-only run events.
pub trait PolicyStore: fmt::Debug + Send + Sync {
    /// Atomically stores one decision, optional request, event, run projection, and optional grant
    /// consumption using the supplied previous run sequence as compare-and-swap anchor.
    fn record_policy_evaluation<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        evaluation: &'a EvaluatedPolicyAction,
    ) -> PolicyStoreFuture<'a, ()>;

    /// Loads one immutable request by exact identity.
    fn load_approval_request<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request_id: ApprovalRequestId,
    ) -> PolicyStoreFuture<'a, Option<ApprovalRequest>>;

    /// Loads one grant including its terminal or active state.
    fn load_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        approval_id: ApprovalId,
    ) -> PolicyStoreFuture<'a, Option<ApprovalGrant>>;

    /// Loads the at-most-one grant created from an exact immutable request.
    fn load_approval_for_request<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request_id: ApprovalRequestId,
    ) -> PolicyStoreFuture<'a, Option<ApprovalGrant>>;

    /// Loads one immutable central decision for recovery and audit verification.
    fn load_policy_decision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        decision_id: PolicyDecisionId,
    ) -> PolicyStoreFuture<'a, Option<PolicyDecision>>;

    /// Atomically inserts one grant and appends its user audit event.
    fn grant_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        approval: &'a ApprovalGrant,
        event: &'a RunEvent,
    ) -> PolicyStoreFuture<'a, ()>;

    /// Atomically compare-and-swaps one grant lifecycle state and appends its user audit event.
    fn revoke_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        expected_state: ApprovalGrantState,
        approval: &'a ApprovalGrant,
        event: &'a RunEvent,
    ) -> PolicyStoreFuture<'a, ()>;
}

/// Stable application classification of local policy storage failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStoreFailure {
    /// Local worktree storage could not be reached or written.
    Unavailable,
    /// Local worktree storage failed integrity checks.
    Corrupt,
    /// Schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated relational or domain invariants.
    InvalidStoredData,
    /// Owning run or approval request was not found.
    NotFound,
    /// A decision, request, or grant identity already exists.
    AlreadyExists,
    /// Another writer advanced the run journal.
    RunSequenceConflict,
    /// Another writer changed the approval lifecycle.
    ApprovalConflict,
}

impl fmt::Display for PolicyStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "policy storage is unavailable",
            Self::Corrupt => "policy storage is corrupt",
            Self::UnsupportedSchema => "policy storage uses an unsupported schema",
            Self::InvalidStoredData => "policy storage contains invalid data",
            Self::NotFound => "policy run or approval was not found",
            Self::AlreadyExists => "policy decision or approval already exists",
            Self::RunSequenceConflict => "policy run sequence changed concurrently",
            Self::ApprovalConflict => "approval lifecycle changed concurrently",
        })
    }
}

impl Error for PolicyStoreFailure {}

/// Persists the already evaluated central decision and audit event atomically.
#[derive(Debug, Clone, Copy)]
pub struct PersistPolicyEvaluation<'a> {
    store: &'a dyn PolicyStore,
}

impl<'a> PersistPolicyEvaluation<'a> {
    /// Creates the use case from its narrow policy-store capability.
    #[must_use]
    pub const fn new(store: &'a dyn PolicyStore) -> Self {
        Self { store }
    }

    /// Commits one and only one central decision for the run sequence observed before evaluation.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &AgentRun,
        evaluation: &EvaluatedPolicyAction,
    ) -> Result<(), PolicyStoreFailure> {
        self.store
            .record_policy_evaluation(project, expected_last_sequence, run, evaluation)
            .await
    }
}

/// Inbound use case for explicitly granting one immutable request.
#[derive(Debug, Clone, Copy)]
pub struct GrantPolicyApproval<'a> {
    store: &'a dyn PolicyStore,
}

impl<'a> GrantPolicyApproval<'a> {
    /// Creates the use case from its narrow policy-store capability.
    #[must_use]
    pub const fn new(store: &'a dyn PolicyStore) -> Self {
        Self { store }
    }

    /// Grants an exact live request and journals the explicit user action atomically.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        request_id: ApprovalRequestId,
        approval_id: ApprovalId,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        granted_at: AgentRunTimestamp,
    ) -> Result<ApprovalGrant, GrantPolicyApprovalError> {
        if run.current_snapshot_id() != snapshot_id {
            return Err(GrantPolicyApprovalError::SnapshotMismatch);
        }
        if run.state().is_terminal() {
            return Err(GrantPolicyApprovalError::Run(AgentRunError::TerminalRun));
        }
        let request = self
            .store
            .load_approval_request(project, request_id)
            .await?
            .ok_or(GrantPolicyApprovalError::RequestNotFound)?;
        if request.run_id() != run.id() {
            return Err(GrantPolicyApprovalError::RequestRunMismatch);
        }
        let approval = ApprovalGrant::grant(approval_id, &request, granted_at)?;
        let expected_last_sequence = run.last_event_sequence();
        let event = run.record(
            event_id,
            RunEventKind::ApprovalRecorded,
            RunEventPayload::new(
                RunEventCode::UserRequest,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            snapshot_id,
            None,
            granted_at,
        )?;
        self.store
            .grant_approval(project, expected_last_sequence, run, &approval, &event)
            .await?;
        Ok(approval)
    }
}

/// Grant path failed before it could authorize any privileged action.
#[derive(Debug)]
pub enum GrantPolicyApprovalError {
    /// Policy storage failed.
    Store(PolicyStoreFailure),
    /// Request identity does not exist.
    RequestNotFound,
    /// Request belongs to another run.
    RequestRunMismatch,
    /// Grant time was outside the request window.
    Grant(ApprovalGrantError),
    /// Run could not append the user audit event.
    Run(AgentRunError),
    /// Snapshot differed from the run's current authoritative snapshot.
    SnapshotMismatch,
}

impl fmt::Display for GrantPolicyApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Store(_) => "approval could not be stored",
            Self::RequestNotFound => "approval request was not found",
            Self::RequestRunMismatch => "approval request belongs to another run",
            Self::Grant(_) => "approval request is no longer grantable",
            Self::Run(_) => "approval grant audit could not be recorded",
            Self::SnapshotMismatch => "approval snapshot differs from the run",
        })
    }
}

impl Error for GrantPolicyApprovalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Grant(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::RequestNotFound | Self::RequestRunMismatch | Self::SnapshotMismatch => None,
        }
    }
}

impl From<PolicyStoreFailure> for GrantPolicyApprovalError {
    fn from(error: PolicyStoreFailure) -> Self {
        Self::Store(error)
    }
}

impl From<ApprovalGrantError> for GrantPolicyApprovalError {
    fn from(error: ApprovalGrantError) -> Self {
        Self::Grant(error)
    }
}

impl From<AgentRunError> for GrantPolicyApprovalError {
    fn from(error: AgentRunError) -> Self {
        Self::Run(error)
    }
}

/// Inbound use case for explicit revocation before one-time use.
#[derive(Debug, Clone, Copy)]
pub struct RevokePolicyApproval<'a> {
    store: &'a dyn PolicyStore,
}

impl<'a> RevokePolicyApproval<'a> {
    /// Creates the use case from its narrow policy-store capability.
    #[must_use]
    pub const fn new(store: &'a dyn PolicyStore) -> Self {
        Self { store }
    }

    /// Revokes a live grant and journals the user action atomically.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        approval_id: ApprovalId,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        revoked_at: AgentRunTimestamp,
    ) -> Result<ApprovalGrant, RevokePolicyApprovalError> {
        if run.current_snapshot_id() != snapshot_id {
            return Err(RevokePolicyApprovalError::SnapshotMismatch);
        }
        if run.state().is_terminal() {
            return Err(RevokePolicyApprovalError::Run(AgentRunError::TerminalRun));
        }
        let mut approval = self
            .store
            .load_approval(project, approval_id)
            .await?
            .ok_or(RevokePolicyApprovalError::ApprovalNotFound)?;
        if approval.run_id() != run.id() {
            return Err(RevokePolicyApprovalError::ApprovalRunMismatch);
        }
        let expected_state = approval.state();
        approval.revoke(revoked_at)?;
        let expected_last_sequence = run.last_event_sequence();
        let event = run.record(
            event_id,
            RunEventKind::ApprovalRecorded,
            RunEventPayload::new(
                RunEventCode::UserRequest,
                Some(RunEventOutcome::Cancelled),
                None,
            ),
            snapshot_id,
            None,
            revoked_at,
        )?;
        self.store
            .revoke_approval(
                project,
                expected_last_sequence,
                run,
                expected_state,
                &approval,
                &event,
            )
            .await?;
        Ok(approval)
    }
}

/// Revocation failed before any privileged action could use the grant.
#[derive(Debug)]
pub enum RevokePolicyApprovalError {
    /// Policy storage failed.
    Store(PolicyStoreFailure),
    /// Approval identity does not exist.
    ApprovalNotFound,
    /// Approval belongs to another run.
    ApprovalRunMismatch,
    /// Approval was terminal, expired, or carried an invalid timestamp.
    Revoke(ApprovalRevokeError),
    /// Run could not append the user audit event.
    Run(AgentRunError),
    /// Snapshot differed from the run's current authoritative snapshot.
    SnapshotMismatch,
}

impl fmt::Display for RevokePolicyApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Store(_) => "approval revocation could not be stored",
            Self::ApprovalNotFound => "approval was not found",
            Self::ApprovalRunMismatch => "approval belongs to another run",
            Self::Revoke(_) => "approval cannot be revoked",
            Self::Run(_) => "approval revocation audit could not be recorded",
            Self::SnapshotMismatch => "approval snapshot differs from the run",
        })
    }
}

impl Error for RevokePolicyApprovalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Revoke(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::ApprovalNotFound | Self::ApprovalRunMismatch | Self::SnapshotMismatch => None,
        }
    }
}

impl From<PolicyStoreFailure> for RevokePolicyApprovalError {
    fn from(error: PolicyStoreFailure) -> Self {
        Self::Store(error)
    }
}

impl From<ApprovalRevokeError> for RevokePolicyApprovalError {
    fn from(error: ApprovalRevokeError) -> Self {
        Self::Revoke(error)
    }
}

impl From<AgentRunError> for RevokePolicyApprovalError {
    fn from(error: AgentRunError) -> Self {
        Self::Run(error)
    }
}
