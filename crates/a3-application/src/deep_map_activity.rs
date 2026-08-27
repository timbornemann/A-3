use a3_domain::ModuleId;
use std::fmt;

/// Stable product phase emitted without prompts, source, model output, or reasoning text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeepMapPhase {
    /// Deterministic plan construction.
    Planning,
    /// Bounded evidence exploration.
    Exploring,
    /// Evidence-bound claim generation.
    Claiming,
    /// Evidence and claim verification.
    Verifying,
    /// Atomic verified-card publication.
    Publishing,
}

/// Coarse target class safe to expose in the untrusted WebView.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeepMapTargetKind {
    /// Complete project publication.
    Project,
    /// Stable module identity.
    Module,
    /// Current manifest revision.
    Manifest,
    /// Current symbol identity.
    Symbol,
}

/// Content-free action class; this deliberately excludes prompts, queries, and rationale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeepMapSafeAction {
    /// Constructs the immutable plan.
    BuildPlan,
    /// Inspects one exact target.
    Inspect,
    /// Searches the bounded published index.
    Search,
    /// Confirms one evidence-bound proposal.
    Propose,
    /// Generates structured claims.
    GenerateClaims,
    /// Revalidates evidence and claims.
    VerifyEvidence,
    /// Publishes verified cards atomically.
    PublishCards,
}

/// One ephemeral activity update owned by the current Deep-Map start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeepMapActivityUpdate {
    phase: DeepMapPhase,
    module_id: Option<ModuleId>,
    target_kind: DeepMapTargetKind,
    action: DeepMapSafeAction,
    step_position: Option<u64>,
    total_steps: Option<u64>,
    confirmed: bool,
}

impl DeepMapActivityUpdate {
    /// Creates a safe activity update with optional deterministic step progress.
    #[must_use]
    pub const fn new(
        phase: DeepMapPhase,
        module_id: Option<ModuleId>,
        target_kind: DeepMapTargetKind,
        action: DeepMapSafeAction,
        step_position: Option<u64>,
        total_steps: Option<u64>,
        confirmed: bool,
    ) -> Self {
        Self {
            phase,
            module_id,
            target_kind,
            action,
            step_position,
            total_steps,
            confirmed,
        }
    }

    #[must_use]
    /// Returns the safe pipeline phase.
    pub const fn phase(self) -> DeepMapPhase {
        self.phase
    }

    #[must_use]
    /// Returns the current module when the update is module-bound.
    pub const fn module_id(self) -> Option<ModuleId> {
        self.module_id
    }

    #[must_use]
    /// Returns the coarse target category.
    pub const fn target_kind(self) -> DeepMapTargetKind {
        self.target_kind
    }

    #[must_use]
    /// Returns the content-free action category.
    pub const fn action(self) -> DeepMapSafeAction {
        self.action
    }

    #[must_use]
    /// Returns the one-based deterministic step position when applicable.
    pub const fn step_position(self) -> Option<u64> {
        self.step_position
    }

    #[must_use]
    /// Returns the immutable plan length when applicable.
    pub const fn total_steps(self) -> Option<u64> {
        self.total_steps
    }

    #[must_use]
    /// Returns whether this update confirmed its deterministic step or publication.
    pub const fn confirmed(self) -> bool {
        self.confirmed
    }
}

/// Observer for bounded, non-persistent, user-visible Deep-Map activity.
pub trait DeepMapActivityObserver: fmt::Debug + Send + Sync {
    /// Records one content-free update. Observability must never control execution.
    fn observe(&self, update: DeepMapActivityUpdate);
}

/// Default observer used by callers that do not need a live activity projection.
#[derive(Debug)]
pub struct IgnoreDeepMapActivity;

impl DeepMapActivityObserver for IgnoreDeepMapActivity {
    fn observe(&self, _update: DeepMapActivityUpdate) {}
}
