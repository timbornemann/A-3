use crate::ProcessRunControl;
use a3_domain::{
    AgentRunId, ContentHash, MachineHttpAction, PatchFileContent, PolicyDecision, PolicyDecisionId,
    PolicyDecisionOutcome, PolicyDecisionReason, PolicyDisposition, PolicyResourceId,
    PreparedMachineHttpAction, ProjectIdentity,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// Bounded, cancellable future of the closed network read boundary.
pub type MachineNetworkFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MachineNetworkFailure>> + Send + 'a>>;

/// Non-cloneable exact GET capability, minted from an actual central policy decision.
#[derive(Debug)]
pub struct AuthorizedMachineHttpGet {
    prepared: PreparedMachineHttpAction,
    decision: PolicyDecision,
}
impl AuthorizedMachineHttpGet {
    /// Requires the exact active run, canonical target, step, method and current policy authority.
    pub fn new(
        prepared: PreparedMachineHttpAction,
        run: AgentRunId,
        decision: &PolicyDecision,
    ) -> Result<Self, MachineNetworkFailure> {
        let action = prepared.policy_action();
        let allowed = decision.reason() == PolicyDecisionReason::ApprovalGranted
            || (decision.reason() == PolicyDecisionReason::SystemAutomatic
                && decision
                    .permission_settings()
                    .is_some_and(|p| p.disposition(&action) == PolicyDisposition::Automatic));
        if !allowed
            || decision.run_id() != run
            || decision.outcome() != PolicyDecisionOutcome::Allowed
            || decision.action_fingerprint() != action.fingerprint()
            || decision.scope_digest() != action.scope_digest()
            || decision.action_class() != action.class()
            || decision.risk_level() != action.risk()
        {
            return Err(MachineNetworkFailure::Denied);
        }
        Ok(Self {
            prepared,
            decision: decision.clone(),
        })
    }
    /// Immutable exact transport proposal after privileged validation.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedMachineHttpAction {
        &self.prepared
    }
    /// Actual central authorization, including its permission settings revision.
    #[must_use]
    pub const fn decision(&self) -> &PolicyDecision {
        &self.decision
    }
}

/// Actual credential-free HTTP response. It proves one observation, not a remote mutation.
#[derive(Debug, Clone)]
pub struct MachineHttpReceipt {
    /// Canonical GET target identity.
    pub resource: PolicyResourceId,
    /// Decision that actually authorized this request.
    pub decision: PolicyDecisionId,
    /// Observed HTTP status, including unsuccessful and redirect responses.
    pub status: u16,
    /// Complete bounded UTF-8 response after secret classification.
    pub body: PatchFileContent,
    /// Exact digest of the observed response bytes.
    pub hash: ContentHash,
}

/// Closed V1 network read capability. No generic HTTP client enters the WebView.
pub trait MachineNetworkTool: fmt::Debug + Send + Sync {
    /// Canonicalizes one HTTPS or literal-loopback HTTP URL without any network request.
    fn prepare(
        &self,
        project: &ProjectIdentity,
        action: &MachineHttpAction,
    ) -> Result<PreparedMachineHttpAction, MachineNetworkFailure>;
    /// Revalidates mode immediately before the bounded, cancellable GET.
    fn get<'a>(
        &'a self,
        authorized: AuthorizedMachineHttpGet,
        control: &'a dyn ProcessRunControl,
    ) -> MachineNetworkFuture<'a, MachineHttpReceipt>;
}

/// Content-free terminal result of the closed network boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineNetworkFailure {
    /// Invalid target, secret, response, authorization or transport boundary.
    Denied,
    /// Settings changed before network admission.
    PermissionsChanged,
    /// Owner cancelled this request; dropping the owned transport aborts it.
    Cancelled,
    /// The complete bounded request exceeded its deadline.
    TimedOut,
    /// No validated complete response was observed.
    Unavailable,
    /// The fixed response boundary was exceeded; no partial response is proof.
    ResourceLimit,
}
impl fmt::Display for MachineNetworkFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Denied => "machine network read denied",
            Self::PermissionsChanged => "machine network permissions changed",
            Self::Cancelled => "machine network read cancelled",
            Self::TimedOut => "machine network read timed out",
            Self::Unavailable => "machine network response unavailable",
            Self::ResourceLimit => "machine network response exceeded its limit",
        })
    }
}
impl Error for MachineNetworkFailure {}
