use crate::JobContext;
use a3_domain::{
    PolicyDecision, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyDisposition, ProcessEvent, ProcessRunResult, ProcessSpec, SystemPolicyV1,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Future returned by the object-safe local process boundary.
pub type ProcessRunFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProcessRunResult, ProcessRunFailure>> + Send + 'a>>;

/// Wakeable cancellation observed while A^3 owns one complete process group.
pub trait ProcessRunControl: fmt::Debug + Send + Sync {
    /// Returns whether the owner requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Waits at most the supplied poll interval and reports whether cancellation arrived.
    fn wait_cancelled_timeout(&self, timeout: Duration) -> bool;
}

impl ProcessRunControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn wait_cancelled_timeout(&self, timeout: Duration) -> bool {
        self.cancellation_token().wait_cancelled_timeout(timeout)
    }
}

/// Bounded event consumer; backpressure failure is terminal and never silently drops an event.
pub trait ProcessEventSink: fmt::Debug + Send + Sync {
    /// Accepts exactly one already secret-classified, strictly sequenced event.
    fn emit(&self, event: ProcessEvent) -> Result<(), ProcessEventSinkError>;
}

/// The event consumer no longer accepts process progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessEventSinkError;

impl fmt::Display for ProcessEventSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process event sink is unavailable")
    }
}

impl Error for ProcessEventSinkError {}

/// Non-cloneable capability proving the central engine allowed one exact `ProcessSpec`.
pub struct AuthorizedProcessSpec {
    specification: ProcessSpec,
    policy_decision_id: PolicyDecisionId,
}

impl AuthorizedProcessSpec {
    /// Accepts either a valid system-automatic safe process or an exactly consumed approval.
    pub fn new(
        specification: ProcessSpec,
        decision: &PolicyDecision,
    ) -> Result<Self, ProcessAuthorizationError> {
        let action = specification.policy_action();
        if decision.run_id() != specification.run_id() {
            return Err(ProcessAuthorizationError::RunMismatch);
        }
        if decision.outcome() != PolicyDecisionOutcome::Allowed {
            return Err(ProcessAuthorizationError::NotAllowed);
        }
        let valid_reason = match decision.reason() {
            PolicyDecisionReason::SystemAutomatic => {
                SystemPolicyV1.disposition(&action) == PolicyDisposition::Automatic
            }
            PolicyDecisionReason::ApprovalGranted => true,
            _ => false,
        };
        if !valid_reason {
            return Err(ProcessAuthorizationError::InvalidDecisionReason);
        }
        if decision.action_fingerprint() != action.fingerprint()
            || decision.scope_digest() != action.scope_digest()
            || decision.action_class() != action.class()
            || decision.risk_level() != action.risk()
        {
            return Err(ProcessAuthorizationError::ActionMismatch);
        }
        Ok(Self {
            specification,
            policy_decision_id: decision.id(),
        })
    }

    /// Returns the exact authorized specification for read-only adapter preflight.
    #[must_use]
    pub const fn specification(&self) -> &ProcessSpec {
        &self.specification
    }

    /// Consumes the capability immediately before the process boundary opens.
    #[must_use]
    pub fn into_parts(self) -> (ProcessSpec, PolicyDecisionId) {
        (self.specification, self.policy_decision_id)
    }
}

impl fmt::Debug for AuthorizedProcessSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizedProcessSpec")
            .field("specification", &self.specification)
            .field("policy_decision_id", &self.policy_decision_id)
            .finish()
    }
}

/// A central policy decision did not authorize this exact process boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAuthorizationError {
    /// Decision belonged to another run.
    RunMismatch,
    /// Decision did not permit tool execution.
    NotAllowed,
    /// Allowed reason was neither a valid system automatic nor a consumed approval.
    InvalidDecisionReason,
    /// Fingerprint, scope, class, or risk differed from the specification.
    ActionMismatch,
}

impl fmt::Display for ProcessAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RunMismatch => "process authorization belongs to another run",
            Self::NotAllowed => "process authorization does not allow execution",
            Self::InvalidDecisionReason => "process authorization reason is invalid",
            Self::ActionMismatch => "process authorization does not match the specification",
        })
    }
}

impl Error for ProcessAuthorizationError {}

/// Narrow adapter capability for one direct argv process and its owned process group.
pub trait ProcessRunner: fmt::Debug + Send + Sync {
    /// Revalidates project, CWD, executable, and environment before consuming authorization.
    fn run<'a>(
        &'a self,
        project: &'a a3_domain::ProjectIdentity,
        authorized: AuthorizedProcessSpec,
        control: &'a dyn ProcessRunControl,
        events: &'a dyn ProcessEventSink,
    ) -> ProcessRunFuture<'a>;
}

/// Stable process-boundary failure without argv, environment values, output, paths, or OS text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRunFailure {
    /// Worktree, CWD, executable, or environment policy denied the request.
    Denied,
    /// Cancellation won before a process group was created.
    Cancelled,
    /// The OS could not create the configured process group.
    SpawnUnavailable,
    /// A bounded stdout or stderr reader could not be owned.
    OutputUnavailable,
    /// The complete process group could not be terminated or reaped.
    TerminationUnavailable,
    /// The bounded event consumer rejected progress; no later events were silently dropped.
    EventUnavailable,
    /// Adapter observations could not form a valid domain result.
    InvalidResult,
}

impl fmt::Display for ProcessRunFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "process execution was denied",
            Self::Cancelled => "process execution was cancelled before start",
            Self::SpawnUnavailable => "process group could not be started",
            Self::OutputUnavailable => "process output could not be drained",
            Self::TerminationUnavailable => "process group could not be terminated",
            Self::EventUnavailable => "process event delivery is unavailable",
            Self::InvalidResult => "process result is invalid",
        })
    }
}

impl Error for ProcessRunFailure {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentRunId, AgentRunTimestamp, PolicyEvaluationTiming, ProcessArgument,
        ProcessEnvironmentVariable, ProcessExecutable, ProcessExecutionMode, ProcessNetworkScope,
        ProcessOutputLimit, ProcessPlanBinding, ProcessSpecSchemaVersion, ProcessTimeout,
        TaskStepId, WorkspaceDirectory, WorktreeId,
    };

    #[test]
    fn authorization_accepts_exact_automatic_safe_process_only() -> Result<(), Box<dyn Error>> {
        let safe_specification = specification(ProcessExecutionMode::KnownSafe)?;
        let action = safe_specification.policy_action();
        let timestamp = AgentRunTimestamp::from_unix_millis(1)?;
        let decision = PolicyDecision::automatic(
            PolicyDecisionId::from_bytes([4; 32]),
            safe_specification.run_id(),
            &action,
            PolicyEvaluationTiming::new(timestamp, timestamp)?,
        );
        assert!(AuthorizedProcessSpec::new(safe_specification, &decision).is_ok());

        let open = specification(ProcessExecutionMode::Open)?;
        let open_action = open.policy_action();
        let invalid_automatic = PolicyDecision::automatic(
            PolicyDecisionId::from_bytes([5; 32]),
            open.run_id(),
            &open_action,
            PolicyEvaluationTiming::new(timestamp, timestamp)?,
        );
        assert!(matches!(
            AuthorizedProcessSpec::new(open, &invalid_automatic),
            Err(ProcessAuthorizationError::InvalidDecisionReason)
        ));
        Ok(())
    }

    #[test]
    fn authorization_is_bound_to_every_specification_field() -> Result<(), Box<dyn Error>> {
        let first = specification(ProcessExecutionMode::KnownSafe)?;
        let action = first.policy_action();
        let timestamp = AgentRunTimestamp::from_unix_millis(1)?;
        let decision = PolicyDecision::automatic(
            PolicyDecisionId::from_bytes([4; 32]),
            first.run_id(),
            &action,
            PolicyEvaluationTiming::new(timestamp, timestamp)?,
        );
        let changed = ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            first.run_id(),
            first.worktree_id(),
            ProcessExecutable::try_from_string("fixture".to_owned())?,
            vec![ProcessArgument::try_from_string("changed".to_owned())?],
            WorkspaceDirectory::Root,
            vec![ProcessEnvironmentVariable::try_from_string(
                "PATH".to_owned(),
            )?],
            ProcessTimeout::from_millis(1_000)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Denied,
        )?;
        assert!(matches!(
            AuthorizedProcessSpec::new(changed, &decision),
            Err(ProcessAuthorizationError::ActionMismatch)
        ));
        Ok(())
    }

    fn specification(mode: ProcessExecutionMode) -> Result<ProcessSpec, Box<dyn Error>> {
        Ok(ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            ProcessExecutable::try_from_string("fixture".to_owned())?,
            vec![ProcessArgument::try_from_string(
                "literal;$(value)".to_owned(),
            )?],
            WorkspaceDirectory::Root,
            vec![ProcessEnvironmentVariable::try_from_string(
                "PATH".to_owned(),
            )?],
            ProcessTimeout::from_millis(1_000)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessOutputLimit::new(1_024)?,
            mode,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Denied,
        )?)
    }
}
