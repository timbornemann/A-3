//! Closed process roles and observations for complete local startup coverage.

use super::*;
use std::num::NonZeroU8;

/// Exact role of one approved effect inside the bounded startup recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoopbackStartupPhase {
    /// Own a bound-only socket at the explicit literal loopback endpoint.
    ReserveEndpoint,
    /// Start the explicit command while the owned endpoint is held.
    HeldExplicit,
    /// Start the same explicit command after releasing the endpoint.
    FreeExplicit,
    /// Start without CLI flags on the original-bound default endpoint.
    Default,
    /// Observe a regular failure for the exact invalid port argument.
    InvalidPort,
}

impl LoopbackStartupPhase {
    /// Stable tag used only in canonical action and effect digests.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::ReserveEndpoint => 0,
            Self::HeldExplicit => 1,
            Self::FreeExplicit => 2,
            Self::Default => 3,
            Self::InvalidPort => 4,
        }
    }
}

/// A numeric application error observed before any owner-requested termination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackFailureExitCode(NonZeroU8);

impl LoopbackFailureExitCode {
    /// Accepts the portable nonzero application-code range established by ADR-0123.
    pub fn new(code: u32) -> Result<Self, LoopbackHttpError> {
        u8::try_from(code)
            .ok()
            .and_then(NonZeroU8::new)
            .map(Self)
            .ok_or(LoopbackHttpError)
    }

    /// Returns the observed application code.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// Extra evidence required in addition to the free explicit start and its responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackStartupEvidence {
    reserve_decision: PolicyDecisionId,
    held_explicit_decision: PolicyDecisionId,
    default_start_decision: PolicyDecisionId,
    invalid_port_decision: PolicyDecisionId,
    held_exit: LoopbackFailureExitCode,
    invalid_exit: LoopbackFailureExitCode,
    default_observation: LoopbackHttpObservation,
}

impl LoopbackStartupEvidence {
    /// Admits only distinct decisions for every additional effect.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reserve_decision: PolicyDecisionId,
        held_explicit_decision: PolicyDecisionId,
        default_start_decision: PolicyDecisionId,
        invalid_port_decision: PolicyDecisionId,
        held_exit: LoopbackFailureExitCode,
        invalid_exit: LoopbackFailureExitCode,
        default_observation: LoopbackHttpObservation,
    ) -> Result<Self, LoopbackHttpError> {
        let evidence = Self {
            reserve_decision,
            held_explicit_decision,
            default_start_decision,
            invalid_port_decision,
            held_exit,
            invalid_exit,
            default_observation,
        };
        if evidence
            .decisions()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != 5
        {
            return Err(LoopbackHttpError);
        }
        Ok(evidence)
    }

    /// Exact decision permitting the held endpoint.
    #[must_use]
    pub const fn reserve_decision(&self) -> PolicyDecisionId {
        self.reserve_decision
    }
    /// Exact decision for the start while the endpoint was held.
    #[must_use]
    pub const fn held_explicit_decision(&self) -> PolicyDecisionId {
        self.held_explicit_decision
    }
    /// Exact decision for the no-flags start.
    #[must_use]
    pub const fn default_start_decision(&self) -> PolicyDecisionId {
        self.default_start_decision
    }
    /// Exact decision for the invalid argument start.
    #[must_use]
    pub const fn invalid_port_decision(&self) -> PolicyDecisionId {
        self.invalid_port_decision
    }
    /// Actual regular failure of the held-port start.
    #[must_use]
    pub const fn held_exit(&self) -> LoopbackFailureExitCode {
        self.held_exit
    }
    /// Actual regular failure of the invalid argument start.
    #[must_use]
    pub const fn invalid_exit(&self) -> LoopbackFailureExitCode {
        self.invalid_exit
    }
    /// Actual designated GET under the default start.
    #[must_use]
    pub const fn default_observation(&self) -> LoopbackHttpObservation {
        self.default_observation
    }

    pub(super) fn decisions(&self) -> impl Iterator<Item = PolicyDecisionId> {
        [
            self.reserve_decision,
            self.held_explicit_decision,
            self.default_start_decision,
            self.invalid_port_decision,
            self.default_observation.decision(),
        ]
        .into_iter()
    }

    pub(super) fn hash_into(&self, hash: &mut blake3::Hasher) {
        for decision in self.decisions() {
            hash.update(decision.as_bytes());
        }
        hash.update(&[self.held_exit.get(), self.invalid_exit.get()]);
        let observation = self.default_observation;
        hash.update(&observation.status().to_le_bytes());
        hash.update(observation.body_hash().as_bytes());
        hash.update(&observation.body_bytes().to_le_bytes());
        hash_headers(hash, observation.headers());
    }
}

impl LoopbackHttpRecipe {
    /// Compiles the exact no-flags invocation required by complete startup coverage.
    pub fn default_process_spec(
        &self,
        run: AgentRunId,
        worktree: WorktreeId,
        step: TaskStepId,
        settings: LoopbackProcessSettings,
    ) -> Result<ProcessSpec, LoopbackHttpError> {
        let index = self.default_start_check_index.ok_or(LoopbackHttpError)?;
        let check = self
            .checks
            .get(usize::from(index))
            .ok_or(LoopbackHttpError)?
            .clone();
        Self::new(
            self.script.clone(),
            LoopbackStart::Default,
            NonZeroU16::new(LOOPBACK_DEFAULT_PORT).ok_or(LoopbackHttpError)?,
            vec![check],
        )?
        .process_spec(run, worktree, step, settings)
    }

    /// Compiles the exact rejected CLI invocation without network authority.
    pub fn invalid_port_process_spec(
        &self,
        run: AgentRunId,
        worktree: WorktreeId,
        step: TaskStepId,
        settings: LoopbackProcessSettings,
    ) -> Result<ProcessSpec, LoopbackHttpError> {
        self.default_start_check_index.ok_or(LoopbackHttpError)?;
        let explicit = self.process_spec(run, worktree, step, settings)?;
        let script = loopback_script_path(&self.script).ok_or(LoopbackHttpError)?;
        let arguments = ["-B", script, "--port", "invalid"]
            .into_iter()
            .map(|value| ProcessArgument::try_from_string(value.to_owned()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| LoopbackHttpError)?;
        ProcessSpec::new(
            ProcessSpecSchemaVersion::V2,
            run,
            worktree,
            explicit.executable().clone(),
            arguments,
            WorkspaceDirectory::Root,
            explicit.environment_allowlist().to_vec(),
            settings.timeout,
            settings.stdout_limit,
            settings.stderr_limit,
            ProcessExecutionMode::Open,
            ProcessPlanBinding::Validated(step),
            ProcessNetworkScope::Denied,
        )
        .map_err(|_| LoopbackHttpError)
    }
}
