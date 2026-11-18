//! Bounded failed observations; these never constitute verification evidence.

use super::*;
use crate::AgentToolAttemptNumber;

/// One complete response that actually differs from an original startup check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackHttpMismatch {
    recipe: PolicyResourceId,
    phase: LoopbackStartupPhase,
    index: u8,
    observation: LoopbackHttpObservation,
}

impl LoopbackHttpMismatch {
    /// Rejects non-HTTP phases, foreign default positions and matching observations.
    pub fn new(
        recipe: &LoopbackHttpRecipe,
        phase: LoopbackStartupPhase,
        index: u8,
        observation: LoopbackHttpObservation,
    ) -> Result<Self, LoopbackHttpError> {
        let default = recipe
            .default_start_check_index()
            .ok_or(LoopbackHttpError)?;
        if !matches!(
            phase,
            LoopbackStartupPhase::FreeExplicit | LoopbackStartupPhase::Default
        ) || (phase == LoopbackStartupPhase::Default && index != default)
            || recipe
                .checks()
                .get(usize::from(index))
                .ok_or(LoopbackHttpError)?
                .matches(&observation)
        {
            return Err(LoopbackHttpError);
        }
        Ok(Self {
            recipe: recipe.id(),
            phase,
            index,
            observation,
        })
    }
    /// Original immutable recipe.
    pub const fn recipe_id(self) -> PolicyResourceId {
        self.recipe
    }
    /// Start under which the response was observed.
    pub const fn phase(self) -> LoopbackStartupPhase {
        self.phase
    }
    /// Position in the original recipe, including under default startup.
    pub const fn check_index(self) -> u8 {
        self.index
    }
    /// Complete bounded content-free response.
    pub const fn observation(self) -> LoopbackHttpObservation {
        self.observation
    }
}

/// Exact failed-attempt coordinates and observations, with no acceptance/evidence status.
///
/// The owning application must additionally establish cleanup and durable current bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackHttpMismatchDiagnostic {
    context: LoopbackHttpEvidenceContext,
    attempt: AgentToolAttemptNumber,
    worktree: WorktreeId,
    script: FileRevision,
    process: LoopbackHttpProcess,
    mismatch: LoopbackHttpMismatch,
    decisions: Vec<PolicyDecisionId>,
}

impl LoopbackHttpMismatchDiagnostic {
    /// Binds a real mismatch and all ordered authorizations to the exact original process.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: LoopbackHttpEvidenceContext,
        attempt: AgentToolAttemptNumber,
        worktree: WorktreeId,
        recipe: &LoopbackHttpRecipe,
        script: FileRevision,
        process: LoopbackHttpProcess,
        mismatch: LoopbackHttpMismatch,
        decisions: Vec<PolicyDecisionId>,
    ) -> Result<Self, LoopbackHttpError> {
        let checked = LoopbackHttpMismatch::new(
            recipe,
            mismatch.phase(),
            mismatch.check_index(),
            mismatch.observation(),
        )?;
        let position = match mismatch.phase() {
            LoopbackStartupPhase::FreeExplicit => 3 + usize::from(mismatch.check_index()),
            LoopbackStartupPhase::Default => 4 + recipe.checks().len(),
            _ => return Err(LoopbackHttpError),
        };
        if checked != mismatch
            || script.path() != recipe.script()
            || decisions.len() != recipe.checks().len() + 6
            || decisions
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != decisions.len()
            || decisions.get(position) != Some(&mismatch.observation().decision())
            || recipe
                .process_spec(
                    context.run_id,
                    worktree,
                    context.step_id,
                    process.settings(),
                )?
                .specification_id()
                != process.specification()
        {
            return Err(LoopbackHttpError);
        }
        Ok(Self {
            context,
            attempt,
            worktree,
            script,
            process,
            mismatch,
            decisions,
        })
    }
    /// Shared coordinate type; its presence does not imply verification success.
    pub const fn context(&self) -> LoopbackHttpEvidenceContext {
        self.context
    }
    /// Exact one-based attempt.
    pub const fn attempt(&self) -> AgentToolAttemptNumber {
        self.attempt
    }
    /// Bound worktree.
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree
    }
    /// Bound original recipe.
    pub const fn recipe_id(&self) -> PolicyResourceId {
        self.mismatch.recipe_id()
    }
    /// Complete script revision at the failed attempt.
    pub const fn script(&self) -> &FileRevision {
        &self.script
    }
    /// Canonical explicit process of the complete startup recipe.
    pub const fn process(&self) -> LoopbackHttpProcess {
        self.process
    }
    /// Actual failed response.
    pub const fn mismatch(&self) -> LoopbackHttpMismatch {
        self.mismatch
    }
    /// All consumed approvals in the recipe's original order.
    pub fn decisions(&self) -> &[PolicyDecisionId] {
        &self.decisions
    }
    /// Content-free normalized result digest; never a TaskEvidenceId.
    pub fn digest(&self) -> ContentHash {
        let mut hash = blake3::Hasher::new_derive_key("a3.loopback-http-mismatch.v1");
        let c = self.context;
        for id in [
            c.verification_run_id.as_bytes(),
            c.spec_id.as_bytes(),
            c.run_id.as_bytes(),
            c.tool_run_id.as_bytes(),
            c.step_id.as_bytes(),
            c.snapshot_id.as_bytes(),
            self.worktree.as_bytes(),
            self.recipe_id().as_bytes(),
            self.script.content_hash().as_bytes(),
            self.process.specification().as_bytes(),
        ] {
            hash.update(id);
        }
        hash_field(&mut hash, self.script.path().as_bytes());
        hash.update(&self.attempt.get().to_le_bytes());
        let settings = self.process.settings();
        hash.update(&settings.timeout.as_millis().to_le_bytes());
        hash.update(&settings.stdout_limit.get().to_le_bytes());
        hash.update(&settings.stderr_limit.get().to_le_bytes());
        hash.update(&[
            u8::from(settings.system_root),
            self.mismatch.phase().tag(),
            self.mismatch.check_index(),
        ]);
        let actual = self.mismatch.observation();
        hash.update(&actual.status().to_le_bytes());
        hash.update(actual.body_hash().as_bytes());
        hash.update(&actual.body_bytes().to_le_bytes());
        hash_headers(&mut hash, actual.headers());
        for decision in &self.decisions {
            hash.update(decision.as_bytes());
        }
        ContentHash::from_bytes(*hash.finalize().as_bytes())
    }
}
