use super::{
    ActionClass, AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalGrantState, ApprovalId,
    ApprovalRequest, ApprovalRequestId, PolicyAction, PolicyActionFingerprint, PolicyDecisionId,
    PolicyScopeDigest, RiskLevel,
};
use std::error::Error;
use std::fmt;

/// Deterministic timing metadata retained for one central policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyEvaluationTiming {
    started_at: AgentRunTimestamp,
    decided_at: AgentRunTimestamp,
    duration_millis: u64,
}

impl PolicyEvaluationTiming {
    /// Creates timing only when the decision does not precede evaluation start.
    pub const fn new(
        started_at: AgentRunTimestamp,
        decided_at: AgentRunTimestamp,
    ) -> Result<Self, PolicyEvaluationTimingError> {
        if decided_at.unix_millis() < started_at.unix_millis() {
            return Err(PolicyEvaluationTimingError);
        }
        Ok(Self {
            started_at,
            decided_at,
            duration_millis: decided_at.unix_millis() - started_at.unix_millis(),
        })
    }

    /// Returns evaluation start.
    #[must_use]
    pub const fn started_at(self) -> AgentRunTimestamp {
        self.started_at
    }

    /// Returns decision time.
    #[must_use]
    pub const fn decided_at(self) -> AgentRunTimestamp {
        self.decided_at
    }

    /// Returns measured deterministic evaluation duration.
    #[must_use]
    pub const fn duration_millis(self) -> u64 {
        self.duration_millis
    }
}

/// Policy decision time preceded evaluation start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyEvaluationTimingError;

impl fmt::Display for PolicyEvaluationTimingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("policy evaluation timestamp regressed")
    }
}

impl Error for PolicyEvaluationTimingError {}

/// Executability result of exactly one central policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyDecisionOutcome {
    /// Action may cross its tool boundary now.
    Allowed,
    /// Action remains blocked pending the attached exact request.
    ApprovalRequired,
    /// Trusted workspace policy denied the action.
    Denied,
}

/// Closed, content-free explanation for a policy outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyDecisionReason {
    /// Fixed system baseline permits the action automatically.
    SystemAutomatic,
    /// Fixed system baseline requires explicit approval.
    SystemApprovalRequired,
    /// Trusted workspace policy upgraded automatic execution to approval.
    WorkspaceApprovalRequired,
    /// Trusted workspace policy denied the class.
    WorkspaceDenied,
    /// A live exact grant was atomically consumed.
    ApprovalGranted,
    /// Presented approval belonged to another run.
    ApprovalRunMismatch,
    /// Presented approval covered a different scope.
    ApprovalScopeMismatch,
    /// Presented approval covered a different action.
    ApprovalActionMismatch,
    /// Presented approval expired.
    ApprovalExpired,
    /// Presented approval was revoked.
    ApprovalRevoked,
    /// Presented approval had already been consumed.
    ApprovalAlreadyConsumed,
    /// Presented approval carried a regressing use timestamp.
    ApprovalTimestampRegressed,
}

/// Content-free durable central policy decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    id: PolicyDecisionId,
    run_id: AgentRunId,
    action_fingerprint: PolicyActionFingerprint,
    scope_digest: PolicyScopeDigest,
    action_class: ActionClass,
    risk_level: RiskLevel,
    outcome: PolicyDecisionOutcome,
    reason: PolicyDecisionReason,
    approval_request_id: Option<ApprovalRequestId>,
    approval_id: Option<ApprovalId>,
    timing: PolicyEvaluationTiming,
}

impl PolicyDecision {
    /// Records an automatically allowed action under the immutable system baseline.
    #[must_use]
    pub fn automatic(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        timing: PolicyEvaluationTiming,
    ) -> Self {
        Self::from_action(
            id,
            run_id,
            action,
            PolicyDecisionOutcome::Allowed,
            PolicyDecisionReason::SystemAutomatic,
            None,
            None,
            timing,
        )
    }

    /// Records a decision allowed by the exact grant consumed for this decision ID.
    pub fn approved(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        approval: &ApprovalGrant,
        timing: PolicyEvaluationTiming,
    ) -> Result<Self, PolicyDecisionError> {
        if approval.run_id() != run_id
            || approval.action_fingerprint() != action.fingerprint()
            || approval.scope_digest() != action.scope_digest()
            || approval.action_class() != action.class()
            || approval.risk_level() != action.risk()
            || !matches!(
                approval.state(),
                ApprovalGrantState::Consumed { decision_id, consumed_at }
                    if decision_id == id && consumed_at == timing.decided_at()
            )
        {
            return Err(PolicyDecisionError::ApprovalNotConsumedForDecision);
        }
        Ok(Self::from_action(
            id,
            run_id,
            action,
            PolicyDecisionOutcome::Allowed,
            PolicyDecisionReason::ApprovalGranted,
            None,
            Some(approval.id()),
            timing,
        ))
    }

    /// Records a blocked action and its exact new request.
    pub fn approval_required(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        timing: PolicyEvaluationTiming,
    ) -> Result<Self, PolicyDecisionError> {
        if !approval_required_reason(reason) {
            return Err(PolicyDecisionError::InvalidShape);
        }
        if request.run_id() != run_id
            || request.action_fingerprint() != action.fingerprint()
            || request.scope_digest() != action.scope_digest()
            || request.action_class() != action.class()
            || request.risk_level() != action.risk()
            || request.requested_at() != timing.decided_at()
        {
            return Err(PolicyDecisionError::RequestMismatch);
        }
        Ok(Self::from_action(
            id,
            run_id,
            action,
            PolicyDecisionOutcome::ApprovalRequired,
            reason,
            Some(request.id()),
            None,
            timing,
        ))
    }

    /// Records a hard workspace restriction; an approval can never override this result.
    #[must_use]
    pub fn denied(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        timing: PolicyEvaluationTiming,
    ) -> Self {
        Self::from_action(
            id,
            run_id,
            action,
            PolicyDecisionOutcome::Denied,
            PolicyDecisionReason::WorkspaceDenied,
            None,
            None,
            timing,
        )
    }

    /// Reconstructs persisted content-free audit data and rejects impossible field combinations.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstruct(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action_fingerprint: PolicyActionFingerprint,
        scope_digest: PolicyScopeDigest,
        action_class: ActionClass,
        risk_level: RiskLevel,
        outcome: PolicyDecisionOutcome,
        reason: PolicyDecisionReason,
        approval_request_id: Option<ApprovalRequestId>,
        approval_id: Option<ApprovalId>,
        timing: PolicyEvaluationTiming,
    ) -> Result<Self, PolicyDecisionError> {
        if !action_class.permits_risk(risk_level) {
            return Err(PolicyDecisionError::ClassRiskMismatch);
        }
        if !valid_shape(outcome, reason, approval_request_id, approval_id) {
            return Err(PolicyDecisionError::InvalidShape);
        }
        Ok(Self {
            id,
            run_id,
            action_fingerprint,
            scope_digest,
            action_class,
            risk_level,
            outcome,
            reason,
            approval_request_id,
            approval_id,
            timing,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn from_action(
        id: PolicyDecisionId,
        run_id: AgentRunId,
        action: &PolicyAction,
        outcome: PolicyDecisionOutcome,
        reason: PolicyDecisionReason,
        approval_request_id: Option<ApprovalRequestId>,
        approval_id: Option<ApprovalId>,
        timing: PolicyEvaluationTiming,
    ) -> Self {
        Self {
            id,
            run_id,
            action_fingerprint: action.fingerprint(),
            scope_digest: action.scope_digest(),
            action_class: action.class(),
            risk_level: action.risk(),
            outcome,
            reason,
            approval_request_id,
            approval_id,
            timing,
        }
    }

    /// Returns decision identity.
    #[must_use]
    pub const fn id(&self) -> PolicyDecisionId {
        self.id
    }

    /// Returns owning run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns exact action fingerprint.
    #[must_use]
    pub const fn action_fingerprint(&self) -> PolicyActionFingerprint {
        self.action_fingerprint
    }

    /// Returns exact scope digest.
    #[must_use]
    pub const fn scope_digest(&self) -> PolicyScopeDigest {
        self.scope_digest
    }

    /// Returns derived action class.
    #[must_use]
    pub const fn action_class(&self) -> ActionClass {
        self.action_class
    }

    /// Returns derived risk.
    #[must_use]
    pub const fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    /// Returns executability outcome.
    #[must_use]
    pub const fn outcome(&self) -> PolicyDecisionOutcome {
        self.outcome
    }

    /// Returns closed explanation.
    #[must_use]
    pub const fn reason(&self) -> PolicyDecisionReason {
        self.reason
    }

    /// Returns new request identity when blocked for approval.
    #[must_use]
    pub const fn approval_request_id(&self) -> Option<ApprovalRequestId> {
        self.approval_request_id
    }

    /// Returns the consumed approval only for an approved decision.
    #[must_use]
    pub const fn approval_id(&self) -> Option<ApprovalId> {
        self.approval_id
    }

    /// Returns deterministic timing.
    #[must_use]
    pub const fn timing(&self) -> PolicyEvaluationTiming {
        self.timing
    }
}

/// Decision constructor detected request, approval, or field-shape inconsistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecisionError {
    /// Approval was not consumed by this decision at this exact time.
    ApprovalNotConsumedForDecision,
    /// Approval request did not match run, action, scope, class, risk, or decision time.
    RequestMismatch,
    /// Persisted outcome, reason, and optional identities cannot coexist.
    InvalidShape,
    /// Persisted risk was not a valid projection for the action class.
    ClassRiskMismatch,
}

impl fmt::Display for PolicyDecisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ApprovalNotConsumedForDecision => {
                "approval was not consumed for this policy decision"
            }
            Self::RequestMismatch => "approval request does not match the policy decision",
            Self::InvalidShape => "policy decision fields form an invalid outcome",
            Self::ClassRiskMismatch => "policy decision class and risk do not match",
        })
    }
}

impl Error for PolicyDecisionError {}

const fn valid_shape(
    outcome: PolicyDecisionOutcome,
    reason: PolicyDecisionReason,
    request_id: Option<ApprovalRequestId>,
    approval_id: Option<ApprovalId>,
) -> bool {
    match (outcome, reason, request_id, approval_id) {
        (PolicyDecisionOutcome::Allowed, PolicyDecisionReason::SystemAutomatic, None, None)
        | (PolicyDecisionOutcome::Allowed, PolicyDecisionReason::ApprovalGranted, None, Some(_))
        | (PolicyDecisionOutcome::Denied, PolicyDecisionReason::WorkspaceDenied, None, None) => {
            true
        }
        (PolicyDecisionOutcome::ApprovalRequired, reason, Some(_), None) => {
            approval_required_reason(reason)
        }
        _ => false,
    }
}

const fn approval_required_reason(reason: PolicyDecisionReason) -> bool {
    matches!(
        reason,
        PolicyDecisionReason::SystemApprovalRequired
            | PolicyDecisionReason::WorkspaceApprovalRequired
            | PolicyDecisionReason::ApprovalRunMismatch
            | PolicyDecisionReason::ApprovalScopeMismatch
            | PolicyDecisionReason::ApprovalActionMismatch
            | PolicyDecisionReason::ApprovalExpired
            | PolicyDecisionReason::ApprovalRevoked
            | PolicyDecisionReason::ApprovalAlreadyConsumed
            | PolicyDecisionReason::ApprovalTimestampRegressed
    )
}

#[cfg(test)]
mod tests {
    use super::{
        PolicyDecision, PolicyDecisionError, PolicyDecisionOutcome, PolicyDecisionReason,
        PolicyEvaluationTiming,
    };
    use crate::{
        ActionClass, AgentRunId, AgentRunTimestamp, ApprovalId, ApprovalRequestId,
        PathPolicyOperation, PathScopeCoverage, PolicyAction, PolicyActionFingerprint,
        PolicyDecisionId, PolicyPathScope, PolicyScopeDigest, RepositoryPath, RiskLevel,
        WorktreeId,
    };

    #[test]
    fn persisted_decision_shape_cannot_claim_approval_without_an_id()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            PolicyDecision::reconstruct(
                PolicyDecisionId::from_bytes([1; 32]),
                AgentRunId::from_bytes([2; 32]),
                PolicyActionFingerprint::from_bytes([3; 32]),
                PolicyScopeDigest::from_bytes([4; 32]),
                ActionClass::Write,
                RiskLevel::Moderate,
                PolicyDecisionOutcome::Allowed,
                PolicyDecisionReason::ApprovalGranted,
                None,
                None,
                PolicyEvaluationTiming::new(timestamp(1)?, timestamp(2)?)?,
            ),
            Err(PolicyDecisionError::InvalidShape)
        );
        Ok(())
    }

    #[test]
    fn automatic_decision_derives_action_and_scope_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let action = PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: WorktreeId::from_bytes([7; 32]),
                path: RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Read,
        };
        let decision = PolicyDecision::automatic(
            PolicyDecisionId::from_bytes([1; 32]),
            AgentRunId::from_bytes([2; 32]),
            &action,
            PolicyEvaluationTiming::new(timestamp(10)?, timestamp(12)?)?,
        );
        assert_eq!(decision.action_fingerprint(), action.fingerprint());
        assert_eq!(decision.scope_digest(), action.scope_digest());
        assert_eq!(decision.outcome(), PolicyDecisionOutcome::Allowed);
        assert_eq!(decision.timing().duration_millis(), 2);
        assert_eq!(decision.approval_id(), None::<ApprovalId>);
        assert_eq!(decision.approval_request_id(), None::<ApprovalRequestId>);
        Ok(())
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn std::error::Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
