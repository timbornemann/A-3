use super::{
    ActionClass, AgentRunId, AgentRunTimestamp, ApprovalId, ApprovalRequestId, PolicyAction,
    PolicyActionFingerprint, PolicyDecisionId, PolicyScopeDigest, RiskLevel,
};
use std::error::Error;
use std::fmt;

const MAX_APPROVAL_LIFETIME_MILLIS: u64 = 24 * 60 * 60 * 1_000;

/// Immutable, exact, time-bounded request for one privileged action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    id: ApprovalRequestId,
    run_id: AgentRunId,
    action_fingerprint: PolicyActionFingerprint,
    scope_digest: PolicyScopeDigest,
    action_class: ActionClass,
    risk_level: RiskLevel,
    requested_at: AgentRunTimestamp,
    expires_at: AgentRunTimestamp,
}

impl ApprovalRequest {
    /// Requests approval for exactly one action and scope for at most 24 hours.
    pub fn new(
        id: ApprovalRequestId,
        run_id: AgentRunId,
        action: &PolicyAction,
        requested_at: AgentRunTimestamp,
        expires_at: AgentRunTimestamp,
    ) -> Result<Self, ApprovalRequestError> {
        validate_window(requested_at, expires_at)?;
        Ok(Self {
            id,
            run_id,
            action_fingerprint: action.fingerprint(),
            scope_digest: action.scope_digest(),
            action_class: action.class(),
            risk_level: action.risk(),
            requested_at,
            expires_at,
        })
    }

    /// Reconstructs one persisted request while reapplying its time-window invariants.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstruct(
        id: ApprovalRequestId,
        run_id: AgentRunId,
        action_fingerprint: PolicyActionFingerprint,
        scope_digest: PolicyScopeDigest,
        action_class: ActionClass,
        risk_level: RiskLevel,
        requested_at: AgentRunTimestamp,
        expires_at: AgentRunTimestamp,
    ) -> Result<Self, ApprovalRequestError> {
        validate_window(requested_at, expires_at)?;
        if !action_class.permits_risk(risk_level) {
            return Err(ApprovalRequestError::ClassRiskMismatch);
        }
        Ok(Self {
            id,
            run_id,
            action_fingerprint,
            scope_digest,
            action_class,
            risk_level,
            requested_at,
            expires_at,
        })
    }

    /// Returns the request identity.
    #[must_use]
    pub const fn id(&self) -> ApprovalRequestId {
        self.id
    }

    /// Returns the exact owning run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact typed-action fingerprint.
    #[must_use]
    pub const fn action_fingerprint(&self) -> PolicyActionFingerprint {
        self.action_fingerprint
    }

    /// Returns the content-free scope digest.
    #[must_use]
    pub const fn scope_digest(&self) -> PolicyScopeDigest {
        self.scope_digest
    }

    /// Returns the derived action class.
    #[must_use]
    pub const fn action_class(&self) -> ActionClass {
        self.action_class
    }

    /// Returns the derived risk level.
    #[must_use]
    pub const fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    /// Returns when the request was created.
    #[must_use]
    pub const fn requested_at(&self) -> AgentRunTimestamp {
        self.requested_at
    }

    /// Returns the exclusive expiration boundary.
    #[must_use]
    pub const fn expires_at(&self) -> AgentRunTimestamp {
        self.expires_at
    }
}

/// Request window was invalid or exceeded the fixed lifetime bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRequestError {
    /// Expiration was not strictly later than creation.
    NonFutureExpiration,
    /// Requested lifetime exceeded 24 hours.
    LifetimeExceeded,
    /// Persisted risk was not a valid projection for the action class.
    ClassRiskMismatch,
}

impl fmt::Display for ApprovalRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NonFutureExpiration => "approval expiration must follow its request",
            Self::LifetimeExceeded => "approval request exceeds the maximum lifetime",
            Self::ClassRiskMismatch => "approval request class and risk do not match",
        })
    }
}

impl Error for ApprovalRequestError {}

/// Durable lifecycle of a granted approval; expiration is derived from time, not a writer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalGrantState {
    /// Available for exactly one matching central policy decision.
    Active,
    /// Atomically consumed by the named policy decision.
    Consumed {
        /// Decision that spent the grant.
        decision_id: PolicyDecisionId,
        /// Consumption timestamp.
        consumed_at: AgentRunTimestamp,
    },
    /// Explicitly withdrawn by the user before use.
    Revoked {
        /// Revocation timestamp.
        revoked_at: AgentRunTimestamp,
    },
}

/// Effective status including expiration of an otherwise active grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// Live and unused.
    Active,
    /// Used once by a decision.
    Consumed,
    /// Explicitly withdrawn.
    Revoked,
    /// Its exclusive expiration boundary has passed.
    Expired,
}

/// One user-granted, exact, non-reusable approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalGrant {
    id: ApprovalId,
    request_id: ApprovalRequestId,
    run_id: AgentRunId,
    action_fingerprint: PolicyActionFingerprint,
    scope_digest: PolicyScopeDigest,
    action_class: ActionClass,
    risk_level: RiskLevel,
    granted_at: AgentRunTimestamp,
    expires_at: AgentRunTimestamp,
    state: ApprovalGrantState,
}

impl ApprovalGrant {
    /// Grants the exact request without extending its original expiration.
    pub fn grant(
        id: ApprovalId,
        request: &ApprovalRequest,
        granted_at: AgentRunTimestamp,
    ) -> Result<Self, ApprovalGrantError> {
        if granted_at < request.requested_at() || granted_at >= request.expires_at() {
            return Err(ApprovalGrantError::GrantOutsideRequestWindow);
        }
        Ok(Self {
            id,
            request_id: request.id(),
            run_id: request.run_id(),
            action_fingerprint: request.action_fingerprint(),
            scope_digest: request.scope_digest(),
            action_class: request.action_class(),
            risk_level: request.risk_level(),
            granted_at,
            expires_at: request.expires_at(),
            state: ApprovalGrantState::Active,
        })
    }

    /// Reconstructs persisted grant state and validates all lifecycle timestamps.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstruct(
        id: ApprovalId,
        request_id: ApprovalRequestId,
        run_id: AgentRunId,
        action_fingerprint: PolicyActionFingerprint,
        scope_digest: PolicyScopeDigest,
        action_class: ActionClass,
        risk_level: RiskLevel,
        granted_at: AgentRunTimestamp,
        expires_at: AgentRunTimestamp,
        state: ApprovalGrantState,
    ) -> Result<Self, ApprovalGrantError> {
        if granted_at >= expires_at {
            return Err(ApprovalGrantError::GrantOutsideRequestWindow);
        }
        if !action_class.permits_risk(risk_level) {
            return Err(ApprovalGrantError::ClassRiskMismatch);
        }
        match state {
            ApprovalGrantState::Active => {}
            ApprovalGrantState::Consumed { consumed_at, .. } if consumed_at < granted_at => {
                return Err(ApprovalGrantError::LifecycleTimestampRegressed);
            }
            ApprovalGrantState::Revoked { revoked_at } if revoked_at < granted_at => {
                return Err(ApprovalGrantError::LifecycleTimestampRegressed);
            }
            ApprovalGrantState::Consumed { consumed_at, .. } if consumed_at >= expires_at => {
                return Err(ApprovalGrantError::LifecycleOutsideApprovalWindow);
            }
            ApprovalGrantState::Revoked { revoked_at } if revoked_at >= expires_at => {
                return Err(ApprovalGrantError::LifecycleOutsideApprovalWindow);
            }
            ApprovalGrantState::Consumed { .. } | ApprovalGrantState::Revoked { .. } => {}
        }
        Ok(Self {
            id,
            request_id,
            run_id,
            action_fingerprint,
            scope_digest,
            action_class,
            risk_level,
            granted_at,
            expires_at,
            state,
        })
    }

    /// Returns the approval identity.
    #[must_use]
    pub const fn id(&self) -> ApprovalId {
        self.id
    }

    /// Returns the immutable originating request.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }

    /// Returns the exact owning run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact action fingerprint.
    #[must_use]
    pub const fn action_fingerprint(&self) -> PolicyActionFingerprint {
        self.action_fingerprint
    }

    /// Returns the exact scope digest.
    #[must_use]
    pub const fn scope_digest(&self) -> PolicyScopeDigest {
        self.scope_digest
    }

    /// Returns the immutable derived class.
    #[must_use]
    pub const fn action_class(&self) -> ActionClass {
        self.action_class
    }

    /// Returns the immutable risk level.
    #[must_use]
    pub const fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    /// Returns the grant time.
    #[must_use]
    pub const fn granted_at(&self) -> AgentRunTimestamp {
        self.granted_at
    }

    /// Returns the exclusive expiration boundary inherited from the request.
    #[must_use]
    pub const fn expires_at(&self) -> AgentRunTimestamp {
        self.expires_at
    }

    /// Returns the persisted state without deriving expiration.
    #[must_use]
    pub const fn state(&self) -> ApprovalGrantState {
        self.state
    }

    /// Returns effective status at one deterministic time.
    #[must_use]
    pub const fn status_at(&self, observed_at: AgentRunTimestamp) -> ApprovalStatus {
        match self.state {
            ApprovalGrantState::Consumed { .. } => ApprovalStatus::Consumed,
            ApprovalGrantState::Revoked { .. } => ApprovalStatus::Revoked,
            ApprovalGrantState::Active
                if observed_at.unix_millis() >= self.expires_at.unix_millis() =>
            {
                ApprovalStatus::Expired
            }
            ApprovalGrantState::Active => ApprovalStatus::Active,
        }
    }

    /// Consumes the grant exactly once for the same run, action, and scope.
    pub fn consume(
        &mut self,
        decision_id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        observed_at: AgentRunTimestamp,
    ) -> Result<(), ApprovalUseError> {
        if observed_at < self.granted_at {
            return Err(ApprovalUseError::TimestampRegressed);
        }
        match self.status_at(observed_at) {
            ApprovalStatus::Active => {}
            ApprovalStatus::Consumed => return Err(ApprovalUseError::AlreadyConsumed),
            ApprovalStatus::Revoked => return Err(ApprovalUseError::Revoked),
            ApprovalStatus::Expired => return Err(ApprovalUseError::Expired),
        }
        if run_id != self.run_id {
            return Err(ApprovalUseError::RunMismatch);
        }
        if action.scope_digest() != self.scope_digest {
            return Err(ApprovalUseError::ScopeMismatch);
        }
        if action.fingerprint() != self.action_fingerprint
            || action.class() != self.action_class
            || action.risk() != self.risk_level
        {
            return Err(ApprovalUseError::ActionMismatch);
        }
        self.state = ApprovalGrantState::Consumed {
            decision_id,
            consumed_at: observed_at,
        };
        Ok(())
    }

    /// Revokes one unused live approval; expired or already terminal grants cannot be rewritten.
    pub fn revoke(&mut self, revoked_at: AgentRunTimestamp) -> Result<(), ApprovalRevokeError> {
        if revoked_at < self.granted_at {
            return Err(ApprovalRevokeError::TimestampRegressed);
        }
        match self.status_at(revoked_at) {
            ApprovalStatus::Active => {
                self.state = ApprovalGrantState::Revoked { revoked_at };
                Ok(())
            }
            ApprovalStatus::Consumed => Err(ApprovalRevokeError::AlreadyConsumed),
            ApprovalStatus::Revoked => Err(ApprovalRevokeError::AlreadyRevoked),
            ApprovalStatus::Expired => Err(ApprovalRevokeError::Expired),
        }
    }
}

/// Persisted grant shape violated its immutable lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalGrantError {
    /// Grant time did not lie inside the originating request window.
    GrantOutsideRequestWindow,
    /// Consumed or revoked timestamp preceded the grant.
    LifecycleTimestampRegressed,
    /// Consumed or revoked timestamp reached or passed the exclusive expiry.
    LifecycleOutsideApprovalWindow,
    /// Persisted risk was not a valid projection for the action class.
    ClassRiskMismatch,
}

impl fmt::Display for ApprovalGrantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::GrantOutsideRequestWindow => "approval grant is outside its request window",
            Self::LifecycleTimestampRegressed => "approval lifecycle timestamp regressed",
            Self::LifecycleOutsideApprovalWindow => {
                "approval lifecycle timestamp is outside the approval window"
            }
            Self::ClassRiskMismatch => "approval class and risk do not match",
        })
    }
}

impl Error for ApprovalGrantError {}

/// A grant could not authorize the exact action at the observed time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalUseError {
    /// Evaluation time preceded grant creation.
    TimestampRegressed,
    /// Grant belongs to another run.
    RunMismatch,
    /// Exact path, process, network, Git, or root scope differs.
    ScopeMismatch,
    /// Action semantics or derived class/risk differ.
    ActionMismatch,
    /// The approval reached its exclusive expiry.
    Expired,
    /// The user revoked the approval.
    Revoked,
    /// A prior policy decision already consumed the approval.
    AlreadyConsumed,
}

impl fmt::Display for ApprovalUseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TimestampRegressed => "approval use timestamp regressed",
            Self::RunMismatch => "approval belongs to another run",
            Self::ScopeMismatch => "approval scope does not match the action",
            Self::ActionMismatch => "approval action does not match",
            Self::Expired => "approval has expired",
            Self::Revoked => "approval was revoked",
            Self::AlreadyConsumed => "approval was already consumed",
        })
    }
}

impl Error for ApprovalUseError {}

/// An approval could not be revoked without rewriting terminal history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRevokeError {
    /// Revocation time preceded grant creation.
    TimestampRegressed,
    /// Approval was already spent.
    AlreadyConsumed,
    /// Approval was already revoked.
    AlreadyRevoked,
    /// Approval had already expired.
    Expired,
}

impl fmt::Display for ApprovalRevokeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TimestampRegressed => "approval revocation timestamp regressed",
            Self::AlreadyConsumed => "consumed approval cannot be revoked",
            Self::AlreadyRevoked => "approval is already revoked",
            Self::Expired => "expired approval cannot be revoked",
        })
    }
}

impl Error for ApprovalRevokeError {}

fn validate_window(
    requested_at: AgentRunTimestamp,
    expires_at: AgentRunTimestamp,
) -> Result<(), ApprovalRequestError> {
    if expires_at <= requested_at {
        return Err(ApprovalRequestError::NonFutureExpiration);
    }
    if expires_at
        .unix_millis()
        .saturating_sub(requested_at.unix_millis())
        > MAX_APPROVAL_LIFETIME_MILLIS
    {
        return Err(ApprovalRequestError::LifetimeExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalGrant, ApprovalGrantError, ApprovalGrantState, ApprovalRequest,
        ApprovalRequestError, ApprovalRevokeError, ApprovalStatus, ApprovalUseError,
    };
    use crate::{
        ActionClass, AgentRunId, AgentRunTimestamp, ApprovalId, ApprovalRequestId,
        PathPolicyOperation, PathScopeCoverage, PolicyAction, PolicyActionFingerprint,
        PolicyDecisionId, PolicyPathScope, PolicyScopeDigest, RepositoryPath, RiskLevel,
        WorktreeId,
    };

    #[test]
    fn approval_is_exact_path_scoped_and_consumed_once() -> Result<(), Box<dyn std::error::Error>> {
        let run_id = AgentRunId::from_bytes([1; 32]);
        let first = path_action("src/first.rs")?;
        let second = path_action("src/second.rs")?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([2; 32]),
            run_id,
            &first,
            timestamp(10)?,
            timestamp(1_000)?,
        )?;
        let mut approval =
            ApprovalGrant::grant(ApprovalId::from_bytes([3; 32]), &request, timestamp(20)?)?;

        assert_eq!(
            approval.consume(
                PolicyDecisionId::from_bytes([4; 32]),
                run_id,
                &second,
                timestamp(30)?
            ),
            Err(ApprovalUseError::ScopeMismatch)
        );
        approval.consume(
            PolicyDecisionId::from_bytes([5; 32]),
            run_id,
            &first,
            timestamp(31)?,
        )?;
        assert_eq!(approval.status_at(timestamp(31)?), ApprovalStatus::Consumed);
        assert_eq!(
            approval.consume(
                PolicyDecisionId::from_bytes([6; 32]),
                run_id,
                &first,
                timestamp(32)?
            ),
            Err(ApprovalUseError::AlreadyConsumed)
        );
        assert_eq!(
            approval.revoke(timestamp(33)?),
            Err(ApprovalRevokeError::AlreadyConsumed)
        );
        Ok(())
    }

    #[test]
    fn approval_expiration_and_revocation_are_terminal() -> Result<(), Box<dyn std::error::Error>> {
        let run_id = AgentRunId::from_bytes([1; 32]);
        let action = path_action("src/lib.rs")?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([2; 32]),
            run_id,
            &action,
            timestamp(10)?,
            timestamp(100)?,
        )?;
        let mut expired =
            ApprovalGrant::grant(ApprovalId::from_bytes([3; 32]), &request, timestamp(20)?)?;
        assert_eq!(expired.status_at(timestamp(100)?), ApprovalStatus::Expired);
        assert_eq!(
            expired.consume(
                PolicyDecisionId::from_bytes([4; 32]),
                run_id,
                &action,
                timestamp(100)?
            ),
            Err(ApprovalUseError::Expired)
        );

        let mut revoked =
            ApprovalGrant::grant(ApprovalId::from_bytes([5; 32]), &request, timestamp(20)?)?;
        revoked.revoke(timestamp(30)?)?;
        assert_eq!(revoked.status_at(timestamp(31)?), ApprovalStatus::Revoked);
        assert_eq!(
            revoked.consume(
                PolicyDecisionId::from_bytes([6; 32]),
                run_id,
                &action,
                timestamp(31)?
            ),
            Err(ApprovalUseError::Revoked)
        );
        Ok(())
    }

    #[test]
    fn approval_request_window_is_positive_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let action = path_action("src/lib.rs")?;
        assert_eq!(
            ApprovalRequest::new(
                ApprovalRequestId::from_bytes([1; 32]),
                AgentRunId::from_bytes([2; 32]),
                &action,
                timestamp(10)?,
                timestamp(10)?
            ),
            Err(ApprovalRequestError::NonFutureExpiration)
        );
        assert_eq!(
            ApprovalRequest::new(
                ApprovalRequestId::from_bytes([1; 32]),
                AgentRunId::from_bytes([2; 32]),
                &action,
                timestamp(0)?,
                timestamp(86_400_001)?
            ),
            Err(ApprovalRequestError::LifetimeExceeded)
        );
        assert_eq!(
            ApprovalRequest::reconstruct(
                ApprovalRequestId::from_bytes([3; 32]),
                AgentRunId::from_bytes([4; 32]),
                PolicyActionFingerprint::from_bytes([5; 32]),
                PolicyScopeDigest::from_bytes([6; 32]),
                ActionClass::Read,
                RiskLevel::Critical,
                timestamp(10)?,
                timestamp(20)?,
            ),
            Err(ApprovalRequestError::ClassRiskMismatch)
        );
        Ok(())
    }

    #[test]
    fn persisted_terminal_state_must_precede_expiration() -> Result<(), Box<dyn std::error::Error>>
    {
        let run_id = AgentRunId::from_bytes([1; 32]);
        let action = path_action("src/lib.rs")?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([2; 32]),
            run_id,
            &action,
            timestamp(10)?,
            timestamp(100)?,
        )?;

        let result = ApprovalGrant::reconstruct(
            ApprovalId::from_bytes([3; 32]),
            request.id(),
            run_id,
            request.action_fingerprint(),
            request.scope_digest(),
            request.action_class(),
            request.risk_level(),
            timestamp(20)?,
            timestamp(100)?,
            ApprovalGrantState::Consumed {
                decision_id: PolicyDecisionId::from_bytes([4; 32]),
                consumed_at: timestamp(100)?,
            },
        );

        assert_eq!(
            result,
            Err(ApprovalGrantError::LifecycleOutsideApprovalWindow)
        );
        Ok(())
    }

    fn path_action(path: &str) -> Result<PolicyAction, Box<dyn std::error::Error>> {
        Ok(PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: WorktreeId::from_bytes([9; 32]),
                path: RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Write,
        })
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn std::error::Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
