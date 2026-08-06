use a3_domain::{
    AgentRun, AgentRunError, AgentRunTimestamp, ApprovalGrant, ApprovalRequest,
    ApprovalRequestError, ApprovalRequestId, ApprovalUseError, PolicyAction, PolicyDecision,
    PolicyDecisionError, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyDisposition, PolicyEvaluationTiming, RunEvent, RunEventCode, RunEventId, RunEventKind,
    RunEventOutcome, RunEventPayload, SnapshotId, SystemPolicyV1, WorkspacePolicy,
};
use std::error::Error;
use std::fmt;

/// Caller-owned identities and timing for one deterministic, auditable policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyEvaluationContext {
    decision_id: PolicyDecisionId,
    approval_request_id: ApprovalRequestId,
    event_id: RunEventId,
    snapshot_id: SnapshotId,
    timing: PolicyEvaluationTiming,
    approval_expires_at: AgentRunTimestamp,
}

impl PolicyEvaluationContext {
    /// Creates explicit context. Approval expiration is validated only if the result needs a new
    /// request, so automatic and hard-denied actions do not depend on unused approval metadata.
    #[must_use]
    pub const fn new(
        decision_id: PolicyDecisionId,
        approval_request_id: ApprovalRequestId,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        timing: PolicyEvaluationTiming,
        approval_expires_at: AgentRunTimestamp,
    ) -> Self {
        Self {
            decision_id,
            approval_request_id,
            event_id,
            snapshot_id,
            timing,
            approval_expires_at,
        }
    }
}

/// One central decision paired with exactly one append-only run event and optional new request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedPolicyAction {
    decision: PolicyDecision,
    approval_request: Option<ApprovalRequest>,
    event: RunEvent,
}

impl EvaluatedPolicyAction {
    /// Returns the exact central decision.
    #[must_use]
    pub const fn decision(&self) -> &PolicyDecision {
        &self.decision
    }

    /// Returns the new exact approval request when execution remains blocked.
    #[must_use]
    pub const fn approval_request(&self) -> Option<&ApprovalRequest> {
        self.approval_request.as_ref()
    }

    /// Returns the one content-free append-only audit event.
    #[must_use]
    pub const fn event(&self) -> &RunEvent {
        &self.event
    }

    /// Consumes the evaluation for atomic persistence.
    #[must_use]
    pub fn into_parts(self) -> (PolicyDecision, Option<ApprovalRequest>, RunEvent) {
        (self.decision, self.approval_request, self.event)
    }
}

/// Central ADR-0012 policy engine. It is deliberately stateless; durable state is carried by the
/// run, exact approval aggregate, decision, request, and append-only event returned to the caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvaluateActionPolicy {
    system: SystemPolicyV1,
}

impl EvaluateActionPolicy {
    /// Uses the immutable V1 system baseline.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            system: SystemPolicyV1,
        }
    }

    /// Evaluates one typed action and emits exactly one decision plus one run audit event.
    pub fn execute(
        self,
        run: &mut AgentRun,
        action: &PolicyAction,
        workspace_policy: &WorkspacePolicy,
        approval: Option<&mut ApprovalGrant>,
        context: PolicyEvaluationContext,
    ) -> Result<EvaluatedPolicyAction, EvaluateActionPolicyError> {
        preflight_run(run, context)?;
        let baseline = self.system.disposition(action);
        let effective = workspace_policy.apply(action.class(), baseline);

        let (decision, request, consumed_approval) = match effective {
            PolicyDisposition::Automatic => (
                PolicyDecision::automatic(context.decision_id, run.id(), action, context.timing),
                None,
                None,
            ),
            PolicyDisposition::Denied => (
                PolicyDecision::denied(context.decision_id, run.id(), action, context.timing),
                None,
                None,
            ),
            PolicyDisposition::ApprovalRequired => match approval {
                Some(grant) => {
                    let mut candidate = grant.clone();
                    match candidate.consume(
                        context.decision_id,
                        run.id(),
                        action,
                        context.timing.decided_at(),
                    ) {
                        Ok(()) => (
                            PolicyDecision::approved(
                                context.decision_id,
                                run.id(),
                                action,
                                &candidate,
                                context.timing,
                            )?,
                            None,
                            Some((grant, candidate)),
                        ),
                        Err(error) => {
                            let request = approval_request(run, action, context)?;
                            let decision = PolicyDecision::approval_required(
                                context.decision_id,
                                run.id(),
                                action,
                                &request,
                                reason_for_approval_error(error),
                                context.timing,
                            )?;
                            (decision, Some(request), None)
                        }
                    }
                }
                None => {
                    let request = approval_request(run, action, context)?;
                    let reason = if baseline == PolicyDisposition::Automatic {
                        PolicyDecisionReason::WorkspaceApprovalRequired
                    } else {
                        PolicyDecisionReason::SystemApprovalRequired
                    };
                    let decision = PolicyDecision::approval_required(
                        context.decision_id,
                        run.id(),
                        action,
                        &request,
                        reason,
                        context.timing,
                    )?;
                    (decision, Some(request), None)
                }
            },
        };

        let outcome = match decision.outcome() {
            PolicyDecisionOutcome::Allowed => RunEventOutcome::Succeeded,
            PolicyDecisionOutcome::ApprovalRequired | PolicyDecisionOutcome::Denied => {
                RunEventOutcome::Denied
            }
        };
        let event = run.record(
            context.event_id,
            RunEventKind::ApprovalRecorded,
            RunEventPayload::new(RunEventCode::PolicyDecision, Some(outcome), None),
            context.snapshot_id,
            None,
            context.timing.decided_at(),
        )?;
        if let Some((target, consumed)) = consumed_approval {
            *target = consumed;
        }
        Ok(EvaluatedPolicyAction {
            decision,
            approval_request: request,
            event,
        })
    }
}

fn preflight_run(
    run: &AgentRun,
    context: PolicyEvaluationContext,
) -> Result<(), EvaluateActionPolicyError> {
    if run.current_snapshot_id() != context.snapshot_id {
        return Err(EvaluateActionPolicyError::SnapshotMismatch);
    }
    if run.state().is_terminal() {
        return Err(EvaluateActionPolicyError::Run(AgentRunError::TerminalRun));
    }
    if context.timing.decided_at() < run.updated_at() {
        return Err(EvaluateActionPolicyError::Run(
            AgentRunError::TimestampRegressed,
        ));
    }
    Ok(())
}

fn approval_request(
    run: &AgentRun,
    action: &PolicyAction,
    context: PolicyEvaluationContext,
) -> Result<ApprovalRequest, ApprovalRequestError> {
    ApprovalRequest::new(
        context.approval_request_id,
        run.id(),
        action,
        context.timing.decided_at(),
        context.approval_expires_at,
    )
}

const fn reason_for_approval_error(error: ApprovalUseError) -> PolicyDecisionReason {
    match error {
        ApprovalUseError::TimestampRegressed => PolicyDecisionReason::ApprovalTimestampRegressed,
        ApprovalUseError::RunMismatch => PolicyDecisionReason::ApprovalRunMismatch,
        ApprovalUseError::ScopeMismatch => PolicyDecisionReason::ApprovalScopeMismatch,
        ApprovalUseError::ActionMismatch => PolicyDecisionReason::ApprovalActionMismatch,
        ApprovalUseError::Expired => PolicyDecisionReason::ApprovalExpired,
        ApprovalUseError::Revoked => PolicyDecisionReason::ApprovalRevoked,
        ApprovalUseError::AlreadyConsumed => PolicyDecisionReason::ApprovalAlreadyConsumed,
    }
}

/// Central policy evaluation failed before any privileged tool boundary was crossed.
#[derive(Debug)]
pub enum EvaluateActionPolicyError {
    /// Requested snapshot differed from the run's current authoritative snapshot.
    SnapshotMismatch,
    /// A needed request had an invalid expiration window.
    ApprovalRequest(ApprovalRequestError),
    /// Domain decision invariants rejected an inconsistent request or grant.
    Decision(PolicyDecisionError),
    /// Run audit could not append exactly one event.
    Run(AgentRunError),
}

impl fmt::Display for EvaluateActionPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SnapshotMismatch => "policy evaluation snapshot differs from the run",
            Self::ApprovalRequest(_) => "policy approval request is invalid",
            Self::Decision(_) => "policy decision is inconsistent",
            Self::Run(_) => "policy decision audit could not be recorded",
        })
    }
}

impl Error for EvaluateActionPolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ApprovalRequest(error) => Some(error),
            Self::Decision(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::SnapshotMismatch => None,
        }
    }
}

impl From<ApprovalRequestError> for EvaluateActionPolicyError {
    fn from(error: ApprovalRequestError) -> Self {
        Self::ApprovalRequest(error)
    }
}

impl From<PolicyDecisionError> for EvaluateActionPolicyError {
    fn from(error: PolicyDecisionError) -> Self {
        Self::Decision(error)
    }
}

impl From<AgentRunError> for EvaluateActionPolicyError {
    fn from(error: AgentRunError) -> Self {
        Self::Run(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluateActionPolicy, PolicyEvaluationContext};
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, ActionClass,
        AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId,
        ApprovalRequest, ApprovalRequestId, ApprovalStatus, GitPolicyOperation, GoalContract,
        GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelProfileId,
        ModelProfileReference, ModelProfileVersion, PathPolicyOperation, PathScopeCoverage,
        PolicyAction, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
        PolicyEvaluationTiming, PolicyPathScope, RepositoryPath, RiskLevel, RunEventCode,
        RunEventId, RunEventKind, RunEventOutcome, SnapshotId, SuccessVerification, TaskId,
        TaskLedgerRevision, WorkspacePolicy, WorkspacePolicyRestriction, WorkspacePolicyRule,
        WorktreeId,
    };

    #[test]
    fn each_evaluation_appends_exactly_one_content_free_policy_event()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut run = run()?;
        let before = run.last_event_sequence();
        let evaluated = EvaluateActionPolicy::new().execute(
            &mut run,
            &PolicyAction::Git {
                worktree_id: WorktreeId::from_bytes([8; 32]),
                operation: GitPolicyOperation::Status,
            },
            &WorkspacePolicy::unrestricted(),
            None,
            context(20, 100)?,
        )?;

        assert_eq!(
            evaluated.decision().outcome(),
            PolicyDecisionOutcome::Allowed
        );
        assert_eq!(evaluated.decision().action_class(), ActionClass::Read);
        assert_eq!(evaluated.decision().risk_level(), RiskLevel::Low);
        assert_eq!(evaluated.approval_request(), None);
        assert_eq!(
            evaluated.event().sequence().get(),
            before.get().checked_add(1).ok_or("sequence overflow")?
        );
        assert_eq!(evaluated.event().kind(), RunEventKind::ApprovalRecorded);
        assert_eq!(
            evaluated.event().payload().code(),
            RunEventCode::PolicyDecision
        );
        assert_eq!(
            evaluated.event().payload().outcome(),
            Some(RunEventOutcome::Succeeded)
        );
        Ok(())
    }

    #[test]
    fn path_approval_cannot_authorize_another_path_and_valid_grant_is_one_time()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = path_action("src/first.rs")?;
        let second = path_action("src/second.rs")?;
        let mut run = run()?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([30; 32]),
            run.id(),
            &first,
            timestamp(10)?,
            timestamp(1_000)?,
        )?;
        let mut approval =
            ApprovalGrant::grant(ApprovalId::from_bytes([31; 32]), &request, timestamp(11)?)?;

        let mismatch = EvaluateActionPolicy::new().execute(
            &mut run,
            &second,
            &WorkspacePolicy::unrestricted(),
            Some(&mut approval),
            context(20, 1_000)?,
        )?;
        assert_eq!(
            mismatch.decision().reason(),
            PolicyDecisionReason::ApprovalScopeMismatch
        );
        assert_eq!(approval.status_at(timestamp(20)?), ApprovalStatus::Active);

        let allowed = EvaluateActionPolicy::new().execute(
            &mut run,
            &first,
            &WorkspacePolicy::unrestricted(),
            Some(&mut approval),
            context(21, 1_000)?,
        )?;
        assert_eq!(allowed.decision().outcome(), PolicyDecisionOutcome::Allowed);
        assert_eq!(allowed.decision().approval_id(), Some(approval.id()));
        assert_eq!(approval.status_at(timestamp(21)?), ApprovalStatus::Consumed);
        Ok(())
    }

    #[test]
    fn workspace_denial_cannot_be_relaxed_by_a_valid_approval()
    -> Result<(), Box<dyn std::error::Error>> {
        let action = path_action("src/lib.rs")?;
        let mut run = run()?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([30; 32]),
            run.id(),
            &action,
            timestamp(10)?,
            timestamp(1_000)?,
        )?;
        let mut approval =
            ApprovalGrant::grant(ApprovalId::from_bytes([31; 32]), &request, timestamp(11)?)?;
        let policy = WorkspacePolicy::new(vec![WorkspacePolicyRule::new(
            ActionClass::Write,
            WorkspacePolicyRestriction::Deny,
        )])?;

        let evaluated = EvaluateActionPolicy::new().execute(
            &mut run,
            &action,
            &policy,
            Some(&mut approval),
            context(20, 1_000)?,
        )?;

        assert_eq!(
            evaluated.decision().outcome(),
            PolicyDecisionOutcome::Denied
        );
        assert_eq!(
            evaluated.decision().reason(),
            PolicyDecisionReason::WorkspaceDenied
        );
        assert_eq!(approval.status_at(timestamp(20)?), ApprovalStatus::Active);
        Ok(())
    }

    fn run() -> Result<AgentRun, Box<dyn std::error::Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("evaluate central policy".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "one decision is audited".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("inspect policy audit".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let (mut run, _) = AgentRun::start(
            AgentRunId::from_bytes([3; 32]),
            goal.reference(),
            TaskLedgerRevision::INITIAL,
            ModelProfileReference::new(
                ModelProfileId::from_bytes([4; 32]),
                ModelProfileVersion::V1,
            ),
            SnapshotId::from_bytes([5; 32]),
            RunEventId::from_bytes([6; 32]),
            timestamp(1)?,
        )?;
        for (id, state, at) in [
            (7, AgentControllerState::Localize, 2),
            (8, AgentControllerState::Plan, 3),
            (9, AgentControllerState::Execute, 4),
        ] {
            run.transition(
                RunEventId::from_bytes([id; 32]),
                state,
                a3_domain::RunEventPayload::empty(),
                SnapshotId::from_bytes([5; 32]),
                timestamp(at)?,
            )?;
        }
        Ok(run)
    }

    fn path_action(path: &str) -> Result<PolicyAction, Box<dyn std::error::Error>> {
        Ok(PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: WorktreeId::from_bytes([8; 32]),
                path: RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Write,
        })
    }

    fn context(
        decided_at: u64,
        expires_at: u64,
    ) -> Result<PolicyEvaluationContext, Box<dyn std::error::Error>> {
        Ok(PolicyEvaluationContext::new(
            PolicyDecisionId::from_bytes([decided_at as u8; 32]),
            ApprovalRequestId::from_bytes([(decided_at as u8).wrapping_add(1); 32]),
            RunEventId::from_bytes([(decided_at as u8).wrapping_add(2); 32]),
            SnapshotId::from_bytes([5; 32]),
            PolicyEvaluationTiming::new(
                timestamp(decided_at.saturating_sub(1))?,
                timestamp(decided_at)?,
            )?,
            timestamp(expires_at)?,
        ))
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn std::error::Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
