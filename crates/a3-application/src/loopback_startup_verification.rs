//! One exact authorization envelope for complete local startup verification.

use crate::{LoopbackVerificationFailure as Failure, loopback_verification::exact_approval};
use a3_domain::{
    AgentRunId, FileRevision, LoopbackFailureExitCode, LoopbackHttpObservation, LoopbackHttpRecipe,
    LoopbackProcessSettings, LoopbackStartupPhase, MutationActionFingerprint, PolicyAction,
    PolicyDecision, PolicyDecisionId, PolicyResourceId, ProcessSpec, SnapshotId, TaskStepId,
    WorktreeId,
};
use std::{future::Future, pin::Pin};

/// Complete immutable source, command and probe bounds prepared before asking for approval.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedLoopbackStartupCoverage {
    explicit: ProcessSpec,
    default_start: ProcessSpec,
    invalid: ProcessSpec,
    script: FileRevision,
    snapshot: SnapshotId,
    recipe: LoopbackHttpRecipe,
    settings: LoopbackProcessSettings,
    step: TaskStepId,
    default_check: usize,
}

impl PreparedLoopbackStartupCoverage {
    /// Compiles all commands from the one original-bound recipe and shared limits.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run: AgentRunId,
        worktree: WorktreeId,
        step: TaskStepId,
        snapshot: SnapshotId,
        script: FileRevision,
        recipe: LoopbackHttpRecipe,
        settings: LoopbackProcessSettings,
    ) -> Result<Self, Failure> {
        let default_check = usize::from(recipe.default_start_check_index().ok_or(Failure::Denied)?);
        if script.path() != recipe.script() {
            return Err(Failure::Denied);
        }
        let explicit = recipe
            .process_spec(run, worktree, step, settings)
            .map_err(|_| Failure::Denied)?;
        let default_start = recipe
            .default_process_spec(run, worktree, step, settings)
            .map_err(|_| Failure::Denied)?;
        let invalid = recipe
            .invalid_port_process_spec(run, worktree, step, settings)
            .map_err(|_| Failure::Denied)?;
        Ok(Self {
            explicit,
            default_start,
            invalid,
            script,
            snapshot,
            recipe,
            settings,
            step,
            default_check,
        })
    }

    /// Exact process for this phase. Endpoint reservation has no process.
    #[must_use]
    pub const fn process(&self, phase: LoopbackStartupPhase) -> Option<&ProcessSpec> {
        match phase {
            LoopbackStartupPhase::ReserveEndpoint => None,
            LoopbackStartupPhase::HeldExplicit | LoopbackStartupPhase::FreeExplicit => {
                Some(&self.explicit)
            }
            LoopbackStartupPhase::Default => Some(&self.default_start),
            LoopbackStartupPhase::InvalidPort => Some(&self.invalid),
        }
    }

    /// The exact current script shared by every start.
    #[must_use]
    pub const fn script(&self) -> &FileRevision {
        &self.script
    }
    /// The one immutable complete recipe.
    #[must_use]
    pub const fn recipe(&self) -> &LoopbackHttpRecipe {
        &self.recipe
    }
    /// The publication used by all the exact scopes.
    #[must_use]
    pub const fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }
    /// Limits shared by the complete sealed call.
    #[must_use]
    pub const fn settings(&self) -> LoopbackProcessSettings {
        self.settings
    }

    /// Exact explicit process shared by held and free startup.
    #[must_use]
    pub const fn start(&self) -> &ProcessSpec {
        &self.explicit
    }

    /// Complete bounded effect recorded before any bind or process.
    #[must_use]
    pub fn effect_resource(&self) -> PolicyResourceId {
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-startup.effect.v1");
        hash.update(self.recipe.id().as_bytes());
        hash.update(self.snapshot.as_bytes());
        hash.update(self.script.content_hash().as_bytes());
        for process in [&self.explicit, &self.default_start, &self.invalid] {
            hash.update(process.specification_id().as_bytes());
        }
        PolicyResourceId::from_bytes(*hash.finalize().as_bytes())
    }

    /// Mutation identity over the entire sealed sequence, independent of any single start.
    #[must_use]
    pub fn effect_fingerprint(&self) -> MutationActionFingerprint {
        MutationActionFingerprint::from_bytes(blake3::derive_key(
            "a3.loopback-startup.mutation.v1",
            self.effect_resource().as_bytes(),
        ))
    }

    /// Ordered approval actions matching the bounded runtime sequence.
    #[must_use]
    pub fn policy_actions(&self) -> Vec<PolicyAction> {
        let mut actions = vec![
            self.start_policy_action(LoopbackStartupPhase::ReserveEndpoint),
            self.start_policy_action(LoopbackStartupPhase::HeldExplicit),
            self.start_policy_action(LoopbackStartupPhase::FreeExplicit),
        ];
        for index in 0..self.recipe.checks().len() {
            actions.push(self.probe_policy_action(index, false));
        }
        actions.push(self.start_policy_action(LoopbackStartupPhase::Default));
        actions.push(self.probe_policy_action(self.default_check, true));
        actions.push(self.start_policy_action(LoopbackStartupPhase::InvalidPort));
        actions
    }

    fn start_policy_action(&self, phase: LoopbackStartupPhase) -> PolicyAction {
        PolicyAction::LoopbackStartup {
            worktree_id: self.explicit.worktree_id(),
            step_id: self.step,
            target_id: self.effect_resource(),
            phase,
        }
    }

    fn probe_policy_action(&self, index: usize, default_start: bool) -> PolicyAction {
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-startup.probe.v1");
        hash.update(self.effect_resource().as_bytes());
        hash.update(&[u8::from(default_start)]);
        hash.update(&(index as u64).to_le_bytes());
        let method = if default_start {
            a3_domain::LoopbackMethod::Get
        } else {
            self.recipe.checks()[index].probe().method()
        };
        PolicyAction::LoopbackHttpProbe {
            worktree_id: self.explicit.worktree_id(),
            step_id: self.step,
            target_id: PolicyResourceId::from_bytes(*hash.finalize().as_bytes()),
            method,
        }
    }
}

/// Exact consumed decision for every effect in the sealed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackStartupDecisions {
    /// Bound-only endpoint reservation.
    pub reserve: PolicyDecisionId,
    /// Explicit start under the held endpoint.
    pub held_explicit: PolicyDecisionId,
    /// Same explicit command after endpoint release.
    pub free_explicit: PolicyDecisionId,
    /// No-flags start on the bound default port.
    pub default_start: PolicyDecisionId,
    /// Exact invalid argument start.
    pub invalid_port: PolicyDecisionId,
    /// Exact primary HTTP requests in original order.
    pub explicit_probes: Vec<PolicyDecisionId>,
    /// Designated default GET.
    pub default_probe: PolicyDecisionId,
}

impl LoopbackStartupDecisions {
    /// All decisions in the same order as prepared.policy_actions().
    #[must_use]
    pub fn ordered(&self) -> Vec<PolicyDecisionId> {
        let mut decisions = vec![self.reserve, self.held_explicit, self.free_explicit];
        decisions.extend_from_slice(&self.explicit_probes);
        decisions.extend([self.default_start, self.default_probe, self.invalid_port]);
        decisions
    }
}

/// Non-cloneable authority for precisely one complete startup call.
#[derive(Debug)]
pub struct AuthorizedLoopbackStartupCoverage {
    prepared: PreparedLoopbackStartupCoverage,
    decisions: LoopbackStartupDecisions,
}

impl AuthorizedLoopbackStartupCoverage {
    /// Requires distinct current explicit approvals for all effects.
    pub fn new(
        prepared: PreparedLoopbackStartupCoverage,
        decisions: &[PolicyDecision],
    ) -> Result<Self, Failure> {
        let actions = prepared.policy_actions();
        let mut seen = std::collections::BTreeSet::new();
        if actions.len() != decisions.len()
            || decisions.iter().zip(&actions).any(|(decision, action)| {
                !exact_approval(decision, prepared.explicit.run_id(), action)
                    || !seen.insert(decision.id())
            })
        {
            return Err(Failure::Denied);
        }
        let count = prepared.recipe.checks().len();
        let identities = LoopbackStartupDecisions {
            reserve: decisions[0].id(),
            held_explicit: decisions[1].id(),
            free_explicit: decisions[2].id(),
            explicit_probes: decisions[3..3 + count]
                .iter()
                .map(PolicyDecision::id)
                .collect(),
            default_start: decisions[3 + count].id(),
            default_probe: decisions[4 + count].id(),
            invalid_port: decisions[5 + count].id(),
        };
        Ok(Self {
            prepared,
            decisions: identities,
        })
    }

    /// Exact prepared scope for adapter preflight.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedLoopbackStartupCoverage {
        &self.prepared
    }
    /// Consumes the authority at the one owned runtime boundary.
    #[must_use]
    pub fn into_parts(self) -> (PreparedLoopbackStartupCoverage, LoopbackStartupDecisions) {
        (self.prepared, self.decisions)
    }
}

/// Complete observations after every owned process and endpoint has been released.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackStartupVerificationReport {
    /// Exact effect authorizations.
    pub decisions: LoopbackStartupDecisions,
    /// Observed nonzero code while holding the explicit endpoint.
    pub held_exit: LoopbackFailureExitCode,
    /// Observed nonzero code for the invalid argument.
    pub invalid_exit: LoopbackFailureExitCode,
    /// Complete primary responses.
    pub explicit_probes: Vec<LoopbackHttpObservation>,
    /// Designated response from the no-flags start.
    pub default_probe: LoopbackHttpObservation,
}

/// Scope observed for all started phases and the owned endpoints they used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackStartupCleanupProof {
    /// Direct children joined, owned groups empty, and their endpoint effects ended.
    ProcessGroupsAndEndpoints,
    /// Direct children signaled, private jobs empty, and their endpoint effects ended.
    WindowsJobsAndEndpoints,
}

/// Complete lifecycle disposition of one sealed startup call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopbackStartupVerificationOutcome {
    /// No bind or process effect began.
    NotStarted(Failure),
    /// A complete current response differed, after every started effect was proven ended.
    Mismatched {
        /// Actual lifecycle cleanup scope.
        cleanup: LoopbackStartupCleanupProof,
        /// Bounded response, never verification evidence.
        mismatch: a3_domain::LoopbackHttpMismatch,
    },
    /// All started effects ended, with either all observations or a known partial failure.
    Stopped {
        /// Proof covering every effect actually started by this call.
        cleanup: LoopbackStartupCleanupProof,
        /// Complete observations or the first bounded failure after proven cleanup.
        result: Result<Box<LoopbackStartupVerificationReport>, Failure>,
    },
    /// At least one owned effect could not be proven ended.
    UnknownCleanup,
}

/// Future owned by the controller for every phase of the complete bounded call.
pub type LoopbackStartupVerificationFuture<'a> =
    Pin<Box<dyn Future<Output = LoopbackStartupVerificationOutcome> + Send + 'a>>;
