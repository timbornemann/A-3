//! Exact process and HTTP admission for one Core-owned local verification run.

use crate::{AuthorizedProcessSpec, ProcessRunControl};
use a3_domain::{
    AgentRunId, FileRevision, LoopbackHttpObservation, LoopbackHttpProcess, LoopbackHttpRecipe,
    PolicyAction, PolicyDecision, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyResourceId, ProcessPlanBinding, ProcessSpec, ProjectIdentity, SnapshotId, TaskStepId,
    WorktreeId,
};
pub use a3_domain::{LoopbackMethod, LoopbackProbe};
use std::{error::Error, fmt, future::Future, num::NonZeroU16, pin::Pin};

/// A complete, non-executable scope prepared by the Core for central policy.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedLoopbackVerification {
    start: ProcessSpec,
    script: FileRevision,
    snapshot: SnapshotId,
    recipe: LoopbackHttpRecipe,
}

impl PreparedLoopbackVerification {
    /// Admits only the exact bounded Python script invocation and ordered loopback probes.
    pub fn new(
        start: ProcessSpec,
        script: FileRevision,
        snapshot: SnapshotId,
        recipe: LoopbackHttpRecipe,
    ) -> Result<Self, LoopbackVerificationFailure> {
        if recipe.default_start_check_index().is_some() {
            return Err(LoopbackVerificationFailure::Unsupported);
        }
        let ProcessPlanBinding::Validated(step) = start.plan_binding() else {
            return Err(LoopbackVerificationFailure::Denied);
        };
        let process = LoopbackHttpProcess::from_spec(&start)
            .map_err(|_| LoopbackVerificationFailure::Denied)?;
        let exact = recipe
            .process_spec(
                start.run_id(),
                start.worktree_id(),
                step,
                process.settings(),
            )
            .map_err(|_| LoopbackVerificationFailure::Denied)?;
        if start != exact || script.path() != recipe.script() {
            return Err(LoopbackVerificationFailure::Denied);
        }
        Ok(Self {
            start,
            script,
            snapshot,
            recipe,
        })
    }

    /// Returns the exact requested network resource for the Python listener specification.
    #[must_use]
    pub fn network_target(port: NonZeroU16) -> PolicyResourceId {
        LoopbackHttpRecipe::network_target(port)
    }

    /// Returns the closed, approval-bound process to start.
    #[must_use]
    pub const fn start(&self) -> &ProcessSpec {
        &self.start
    }

    /// Returns the exact script revision to validate before starting and after stopping.
    #[must_use]
    pub const fn script(&self) -> &FileRevision {
        &self.script
    }

    /// Returns the sole port on literal IPv4 loopback.
    #[must_use]
    pub const fn port(&self) -> NonZeroU16 {
        self.recipe.port()
    }

    /// Returns the immutable recipe and its exact ordered response conditions.
    #[must_use]
    pub const fn recipe(&self) -> &LoopbackHttpRecipe {
        &self.recipe
    }

    /// Returns the current snapshot used to prepare and authorize this recipe.
    #[must_use]
    pub const fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// Creates one explicit policy action bound to this revision and exact ordered recipe.
    #[must_use]
    pub fn probe_policy_action(&self, index: usize) -> Option<PolicyAction> {
        let ProcessPlanBinding::Validated(step) = self.start.plan_binding() else {
            return None;
        };
        Self::bound_probe_policy_action(
            self.start.worktree_id(),
            step,
            self.start.specification_id(),
            self.snapshot,
            &self.recipe,
            &self.script,
            index,
        )
    }

    /// Reconstructs the exact policy action for one persisted recipe observation.
    #[must_use]
    pub fn bound_probe_policy_action(
        worktree: WorktreeId,
        step: TaskStepId,
        process_specification: PolicyResourceId,
        snapshot: SnapshotId,
        recipe: &LoopbackHttpRecipe,
        script: &FileRevision,
        index: usize,
    ) -> Option<PolicyAction> {
        if script.path() != recipe.script() {
            return None;
        }
        let method = recipe.checks().get(index)?.probe().method();
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-verification.probe.v2");
        hash.update(process_specification.as_bytes());
        hash.update(snapshot.as_bytes());
        hash.update(recipe.id().as_bytes());
        hash.update(script.content_hash().as_bytes());
        hash.update(&(index as u64).to_le_bytes());
        Some(PolicyAction::LoopbackHttpProbe {
            worktree_id: worktree,
            step_id: step,
            target_id: PolicyResourceId::from_bytes(*hash.finalize().as_bytes()),
            method,
        })
    }
}

/// Non-cloneable authorization to dispatch one prepared recipe.
///
/// The owning controller persists the policy decisions and a new attempt before dispatch.
pub struct AuthorizedLoopbackVerification {
    prepared: PreparedLoopbackVerification,
    start: AuthorizedProcessSpec,
    probe_decisions: Vec<PolicyDecisionId>,
}

impl AuthorizedLoopbackVerification {
    /// Requires a matching, distinct explicit decision for every effect in recipe order.
    pub fn new(
        prepared: PreparedLoopbackVerification,
        start_decision: &PolicyDecision,
        probe_decisions: &[PolicyDecision],
    ) -> Result<Self, LoopbackVerificationFailure> {
        if start_decision.reason() != PolicyDecisionReason::ApprovalGranted
            || probe_decisions.len() != prepared.recipe.checks().len()
        {
            return Err(LoopbackVerificationFailure::Denied);
        }
        let start = AuthorizedProcessSpec::new(prepared.start.clone(), start_decision)
            .map_err(|_| LoopbackVerificationFailure::Denied)?;
        let mut admitted = Vec::with_capacity(probe_decisions.len());
        for (index, decision) in probe_decisions.iter().enumerate() {
            let action = prepared
                .probe_policy_action(index)
                .ok_or(LoopbackVerificationFailure::Denied)?;
            if !exact_approval(decision, prepared.start.run_id(), &action)
                || decision.id() == start_decision.id()
                || admitted.contains(&decision.id())
            {
                return Err(LoopbackVerificationFailure::Denied);
            }
            admitted.push(decision.id());
        }
        Ok(Self {
            prepared,
            start,
            probe_decisions: admitted,
        })
    }

    /// Returns the admitted recipe for adapter preflight.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedLoopbackVerification {
        &self.prepared
    }

    /// Consumes the exact capabilities at the owned process and HTTP boundary.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        PreparedLoopbackVerification,
        AuthorizedProcessSpec,
        Vec<PolicyDecisionId>,
    ) {
        (self.prepared, self.start, self.probe_decisions)
    }
}

pub(crate) fn exact_approval(
    decision: &PolicyDecision,
    run: AgentRunId,
    action: &PolicyAction,
) -> bool {
    decision.reason() == PolicyDecisionReason::ApprovalGranted
        && decision.outcome() == PolicyDecisionOutcome::Allowed
        && decision.run_id() == run
        && decision.action_fingerprint() == action.fingerprint()
        && decision.scope_digest() == action.scope_digest()
        && decision.action_class() == action.class()
        && decision.risk_level() == action.risk()
}

/// Adapter observations. The controller must still prove durable completion and freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackVerificationReport {
    /// Exact decision used to start the service.
    pub start_decision: PolicyDecisionId,
    /// Complete ordered probe observations.
    pub probes: Vec<LoopbackHttpObservation>,
}

/// Scope of the cleanup that the adapter actually confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackCleanupProof {
    /// The direct child was joined, its owned process group was empty and the listener was gone.
    ProcessGroupAndListener,
    /// The direct child was signaled, its private Windows job was empty and the listener was gone.
    WindowsJobAndListener,
}

/// Distinguishes the service's lifecycle from the result of checking its responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopbackVerificationOutcome {
    /// The approved process was definitely not started.
    NotStarted(LoopbackVerificationFailure),
    /// The adapter observed cleanup at the given scope, even if the check failed.
    Stopped {
        /// Scope actually confirmed during cleanup.
        cleanup: LoopbackCleanupProof,
        /// Complete observations or the bounded reason they could not be obtained.
        result: Result<LoopbackVerificationReport, LoopbackVerificationFailure>,
    },
    /// A process may have started and the adapter could not confirm cleanup.
    UnknownCleanup,
}

/// Closed runtime failure classes for the local verification boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackVerificationFailure {
    /// The prepared effect or its approval binding was invalid.
    Denied,
    /// The adapter cannot establish required process or socket ownership.
    Unsupported,
    /// The operation was cancelled.
    Cancelled,
    /// The operation exhausted its deadline.
    TimedOut,
    /// An unrelated listener occupied the approved port.
    Occupied,
    /// The owned service exited before completing the probes.
    Exited,
    /// The approved listener or response could not be reached.
    Unavailable,
    /// The response could not be represented by the closed observation contract.
    InvalidResponse,
    /// Output or another bounded resource exceeded its limit.
    ResourceLimit,
    /// The script was not the bound revision when checked.
    StaleScript,
}

impl fmt::Display for LoopbackVerificationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "local verification scope or approval is invalid",
            Self::Unsupported => "local verification ownership is unsupported",
            Self::Cancelled => "local verification was cancelled",
            Self::TimedOut => "local verification exceeded its deadline",
            Self::Occupied => "local verification port is occupied",
            Self::Exited => "local verification service exited before all probes",
            Self::Unavailable => "local verification service or response is unavailable",
            Self::InvalidResponse => "local verification response is invalid",
            Self::ResourceLimit => "local verification exceeded its resource limit",
            Self::StaleScript => "local verification script is stale",
        })
    }
}

impl Error for LoopbackVerificationFailure {}

/// Future owned by the controller for the complete bounded process lifetime.
pub type LoopbackVerificationFuture<'a> =
    Pin<Box<dyn Future<Output = LoopbackVerificationOutcome> + Send + 'a>>;

/// Adapter boundary for one authorized local service start, HTTP probes and owned stop.
pub trait LoopbackVerificationRunner: fmt::Debug + Send + Sync {
    /// Indicates whether this adapter can prove that the entire owned group has stopped.
    /// The controller must still check the actual outcome of each invocation.
    fn supports_owned_cleanup(&self) -> bool;

    /// Indicates support for the entire closed startup coverage under one owner and deadline.
    fn supports_startup_coverage(&self) -> bool {
        false
    }

    /// Executes a complete startup recipe, or rejects it before any effect.
    fn verify_startup<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _authorized: crate::AuthorizedLoopbackStartupCoverage,
        _control: &'a dyn ProcessRunControl,
    ) -> crate::LoopbackStartupVerificationFuture<'a> {
        Box::pin(async {
            crate::LoopbackStartupVerificationOutcome::NotStarted(
                LoopbackVerificationFailure::Unsupported,
            )
        })
    }

    /// Runs the exact authorized recipe within the supplied process control.
    fn verify<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackVerification,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest, ApprovalRequestId,
        ContentHash, LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackStart,
        PolicyEvaluationTiming, ProcessArgument, ProcessExecutable, ProcessExecutionMode,
        ProcessNetworkScope, ProcessOutputLimit, ProcessSpecSchemaVersion, ProcessTimeout,
        RepositoryPath, WorkspaceDirectory,
    };

    fn process(port: NonZeroU16, arguments: &[&str]) -> Result<ProcessSpec, Box<dyn Error>> {
        let arguments = arguments
            .iter()
            .map(|argument| ProcessArgument::try_from_string((*argument).to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        let limit = ProcessOutputLimit::new(1024)?;
        Ok(ProcessSpec::new(
            ProcessSpecSchemaVersion::V2,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            ProcessExecutable::try_from_string("python".to_owned())?,
            arguments,
            WorkspaceDirectory::Root,
            Vec::new(),
            ProcessTimeout::from_millis(4000)?,
            limit,
            limit,
            ProcessExecutionMode::Open,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Requested(PreparedLoopbackVerification::network_target(port)),
        )?)
    }

    fn prepare(
        method: LoopbackMethod,
        source: u8,
        snapshot: u8,
        status: u16,
    ) -> Result<PreparedLoopbackVerification, Box<dyn Error>> {
        let port = NonZeroU16::try_from(8765)?;
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let checks = [(LoopbackMethod::Get, 200), (method, status)]
            .into_iter()
            .map(|(method, status)| {
                LoopbackHttpCheck::new(
                    LoopbackProbe::new(method, "/".to_owned())?,
                    status,
                    LoopbackExpectedBody::AnyBounded,
                    LoopbackHeaderChecks::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PreparedLoopbackVerification::new(
            process(port, &["-B", "server.py"])?,
            FileRevision::new(path.clone(), ContentHash::from_bytes([source; 32])),
            SnapshotId::from_bytes([snapshot; 32]),
            LoopbackHttpRecipe::new(path, LoopbackStart::Default, port, checks)?,
        )?)
    }

    fn approve(
        seed: u8,
        run: AgentRunId,
        action: &PolicyAction,
    ) -> Result<PolicyDecision, Box<dyn Error>> {
        let when = AgentRunTimestamp::from_unix_millis(10)?;
        let expires = AgentRunTimestamp::from_unix_millis(20)?;
        let request = ApprovalRequest::new(
            ApprovalRequestId::from_bytes([seed; 32]),
            run,
            action,
            when,
            expires,
        )?;
        let id = PolicyDecisionId::from_bytes([seed; 32]);
        let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([seed; 32]), &request, when)?;
        grant.consume(id, run, action, when)?;
        Ok(PolicyDecision::approved(
            id,
            run,
            action,
            &grant,
            PolicyEvaluationTiming::new(when, when)?,
        )?)
    }

    #[test]
    fn approvals_bind_order_method_revision_snapshot_and_expectations() -> Result<(), Box<dyn Error>>
    {
        let prepared = prepare(LoopbackMethod::Post, 8, 4, 405)?;
        let run = prepared.start().run_id();
        let start = approve(1, run, &prepared.start().policy_action())?;
        let get = approve(2, run, &prepared.probe_policy_action(0).ok_or("GET scope")?)?;
        let post = approve(
            3,
            run,
            &prepared.probe_policy_action(1).ok_or("POST scope")?,
        )?;
        assert!(matches!(
            AuthorizedLoopbackVerification::new(
                prepare(LoopbackMethod::Post, 8, 4, 405)?,
                &start,
                &[post.clone(), get.clone()]
            ),
            Err(LoopbackVerificationFailure::Denied)
        ));
        for (method, revision, snapshot, status) in [
            (LoopbackMethod::Put, 8, 4, 405),
            (LoopbackMethod::Post, 9, 4, 405),
            (LoopbackMethod::Post, 8, 5, 405),
            (LoopbackMethod::Post, 8, 4, 400),
        ] {
            assert!(matches!(
                AuthorizedLoopbackVerification::new(
                    prepare(method, revision, snapshot, status)?,
                    &start,
                    &[get.clone(), post.clone()]
                ),
                Err(LoopbackVerificationFailure::Denied)
            ));
        }
        let decisions = [get.id(), post.id()];
        let authorized = AuthorizedLoopbackVerification::new(prepared, &start, &[get, post])?;
        let (prepared, authorized_start, probes) = authorized.into_parts();
        assert_eq!(authorized_start.specification(), prepared.start());
        assert_eq!(probes, decisions);
        Ok(())
    }

    #[test]
    fn loopback_preparation_rejects_a_different_approved_script() -> Result<(), Box<dyn Error>> {
        let prepared = prepare(LoopbackMethod::Post, 8, 4, 405)?;
        let unrelated = process(prepared.port(), &["-B", "other.py"])?;
        assert!(matches!(
            PreparedLoopbackVerification::new(
                unrelated,
                prepared.script,
                prepared.snapshot,
                prepared.recipe,
            ),
            Err(LoopbackVerificationFailure::Denied)
        ));
        Ok(())
    }

    #[test]
    fn single_start_dispatch_cannot_prepare_the_startup_coverage_recipe()
    -> Result<(), Box<dyn Error>> {
        let port = NonZeroU16::try_from(18_765)?;
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let script = FileRevision::new(path.clone(), ContentHash::from_bytes([8; 32]));
        let snapshot = SnapshotId::from_bytes([4; 32]);
        let recipe = LoopbackHttpRecipe::new(
            path,
            LoopbackStart::Explicit,
            port,
            vec![LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: ContentHash::from_bytes([9; 32]),
                    bytes: 5,
                },
                LoopbackHeaderChecks::default(),
            )?],
        )?
        .with_startup_coverage(0)?;
        let start = process(
            port,
            &["-B", "server.py", "--host", "127.0.0.1", "--port", "18765"],
        )?;
        assert!(matches!(
            PreparedLoopbackVerification::new(start, script, snapshot, recipe),
            Err(LoopbackVerificationFailure::Unsupported)
        ));
        Ok(())
    }

    #[test]
    fn probe_rejects_url_syntax_and_noncanonical_segments() {
        for path in [
            "//evil", "/%2e", "/./ok", "/../ok", "/ok?q", "/ok#f", "/a\\b",
        ] {
            assert!(LoopbackProbe::new(LoopbackMethod::Get, path.to_owned()).is_err());
        }
        assert!(LoopbackProbe::new(LoopbackMethod::Get, "/health/v1".to_owned()).is_ok());
    }
}
