//! Durable provider accounting for the first Plan before a Run exists.

use crate::{
    AdmittedInitialPlan, EditPhaseCallKind, EditPhaseReservationFailure, EditPhaseReservedUsage,
    EditPhaseWireCost, PlanPhaseAnchor, PlanPhaseHalt, PreparedPlanPhase, StoredTaskLedger,
};
use a3_domain::{AgentRunTimestamp, ContentHash, ProjectIdentity, TaskId};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// One process-local authorization for an already charged initial Plan request.
#[derive(Debug)]
pub struct InitialPlanReservation {
    anchor: PlanPhaseAnchor,
    job_digest: ContentHash,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved_at: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
}

impl InitialPlanReservation {
    /// Adapter constructor. Issue a permit only after the charge commits.
    #[must_use]
    pub const fn issue(
        phase: &PreparedPlanPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        reserved_at: AgentRunTimestamp,
        deadline: AgentRunTimestamp,
    ) -> Self {
        Self {
            anchor: phase.anchor(),
            job_digest: phase.job_digest(),
            kind,
            wire,
            reserved_at,
            deadline,
        }
    }

    /// Exact promoted Goal, publication and model profile binding.
    #[must_use]
    pub const fn anchor(&self) -> PlanPhaseAnchor {
        self.anchor
    }

    /// Digest of the same scope across the original request and its one repair.
    #[must_use]
    pub const fn job_digest(&self) -> ContentHash {
        self.job_digest
    }

    /// Initial request or the single structural repair.
    #[must_use]
    pub const fn kind(&self) -> EditPhaseCallKind {
        self.kind
    }

    /// Complete encoded request digest and charged wire limits.
    #[must_use]
    pub const fn wire(&self) -> EditPhaseWireCost {
        self.wire
    }

    /// Time used for the durable debit.
    #[must_use]
    pub const fn reserved_at(&self) -> AgentRunTimestamp {
        self.reserved_at
    }

    /// The unchanged absolute deadline pinned by Intake.
    #[must_use]
    pub const fn deadline(&self) -> AgentRunTimestamp {
        self.deadline
    }
}

/// Closed provider dispositions that cannot create an initial Ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialPlanRejection {
    /// The complete response was structurally invalid.
    InvalidStructure,
    /// The reply was structurally valid and its proposal could not be used.
    RejectedReply,
    /// A known provider or transport failure.
    ProviderFailure,
    /// The owner cancelled the request.
    Cancelled,
    /// The outcome of the provider request could not be established.
    Unknown,
}

/// One observed provider result, including its actual reported token counts.
#[derive(Debug)]
pub enum InitialPlanFinish {
    /// A valid explicit Add decision stopped without creating a Ledger or a Run.
    Halted {
        /// Closed reason retained for cold-reopen presentation.
        reason: PlanPhaseHalt,
        /// Actual positive prompt usage reported by the provider.
        prompt_tokens: u32,
        /// Actual positive output usage reported by the provider.
        output_tokens: u32,
    },
    /// A checked Plan may commit its Ready Ledger together with this settlement.
    Accepted {
        /// An opaque result minted by the bound Plan decoder and Ledger validation.
        plan: AdmittedInitialPlan,
        /// Actual reported prompt tokens.
        prompt_tokens: u32,
        /// Actual reported output tokens.
        output_tokens: u32,
    },
    /// The model asked for another bound source before making a proposal.
    NeedEvidence {
        /// Actual reported prompt tokens.
        prompt_tokens: u32,
        /// Actual reported output tokens.
        output_tokens: u32,
    },
    /// The model reported that the current scope could not be planned safely.
    TooLarge {
        /// Actual reported prompt tokens.
        prompt_tokens: u32,
        /// Actual reported output tokens.
        output_tokens: u32,
    },
    /// A closed failure that cannot materialize any Ledger.
    ObservedFailure {
        /// Provider or decoder disposition.
        kind: InitialPlanRejection,
        /// Actual prompt count if the provider supplied it.
        reported_prompt_tokens: Option<u32>,
        /// Actual output count if the provider supplied it.
        reported_output_tokens: Option<u32>,
    },
}

/// Closed failures at the durable initial Plan boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialPlanReservationFailure {
    /// Shared wire, time, usage or durable observation failure.
    Accounting(EditPhaseReservationFailure),
    /// There is no pinned Intake allowance for this Goal.
    ScopeMissing,
    /// Current project, Goal, publication or scope differs from preparation.
    StalePlan,
    /// An initial Ledger has already been materialized.
    AlreadyMaterialized,
    /// A decoded Plan does not match its reservation or durable Goal.
    InvalidPlan,
}

impl From<EditPhaseReservationFailure> for InitialPlanReservationFailure {
    fn from(value: EditPhaseReservationFailure) -> Self {
        Self::Accounting(value)
    }
}

impl fmt::Display for InitialPlanReservationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accounting(error) => error.fmt(f),
            Self::ScopeMissing => f.write_str("Initial Plan has no pinned Intake allowance"),
            Self::StalePlan => f.write_str("Initial Plan binding is stale"),
            Self::AlreadyMaterialized => f.write_str("Initial Plan Ledger already exists"),
            Self::InvalidPlan => f.write_str("Initial Plan does not match its durable binding"),
        }
    }
}

impl Error for InitialPlanReservationFailure {}

/// Future returned by the initial Plan reservation port.
pub type InitialPlanReservationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, InitialPlanReservationFailure>> + Send + 'a>>;

/// Candidate-only provider accounting in the current Goal and Ledger authority.
pub trait InitialPlanReservationStore: Send + Sync {
    /// Charges one exact request before provider dispatch and returns its one-use permit.
    fn reserve_initial_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedPlanPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, InitialPlanReservation>;

    /// Settles a call once. A successful Plan commits its Ledger atomically with this observation.
    fn finish_initial_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: InitialPlanReservation,
        outcome: InitialPlanFinish,
        now: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, Option<StoredTaskLedger>>;

    /// Returns only initial Plan charges, with no ability to re-dispatch a settled or lost call.
    fn load_initial_plan_usage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> InitialPlanReservationFuture<'a, EditPhaseReservedUsage>;

    /// Returns the latest current-Goal call's explicit halt, without granting another call.
    fn load_initial_plan_halt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> InitialPlanReservationFuture<'a, Option<PlanPhaseHalt>>;
}
