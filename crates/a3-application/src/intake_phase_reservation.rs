//! Durable accounting for a bounded pending Intake call and its one-time Run carryover.

use crate::{
    EditPhaseCallKind, EditPhaseReservationFailure, EditPhaseReservedUsage, EditPhaseWireCost,
    IntakePhaseAnchor,
};
use a3_domain::{
    AgentRunBudget, AgentRunDurationLimit, AgentRunId, AgentRunTimestamp, GoalContractTimestamp,
    GoalIntakeReview, ModelProfileReference, ProjectIdentity, TaskId,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// The durable allowance and charged calls of one captured original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntakePhaseAccounting {
    budget: AgentRunBudget,
    captured_at: GoalContractTimestamp,
    deadline: AgentRunTimestamp,
    profile: ModelProfileReference,
    linked_run: Option<AgentRunId>,
    usage: EditPhaseReservedUsage,
}

impl IntakePhaseAccounting {
    /// Reconstructs an allowance using the original immutable capture timestamp and budget.
    pub fn reconstruct(
        budget: AgentRunBudget,
        captured_at: GoalContractTimestamp,
        profile: ModelProfileReference,
        linked_run: Option<AgentRunId>,
        usage: EditPhaseReservedUsage,
    ) -> Result<Self, IntakePhaseReservationFailure> {
        let deadline = captured_at
            .unix_millis()
            .checked_add(budget.duration_limit().millis())
            .and_then(|value| AgentRunTimestamp::from_unix_millis(value).ok())
            .ok_or(IntakePhaseReservationFailure::InvalidBudget)?;
        Ok(Self {
            budget,
            captured_at,
            deadline,
            profile,
            linked_run,
            usage,
        })
    }

    /// Immutable full allowance selected before the first Intake call.
    #[must_use]
    pub const fn budget(self) -> AgentRunBudget {
        self.budget
    }

    /// Capture time from which the original deadline is derived.
    #[must_use]
    pub const fn captured_at(self) -> GoalContractTimestamp {
        self.captured_at
    }

    /// Original absolute deadline shared by Intake and the eventual Run.
    #[must_use]
    pub const fn deadline(self) -> AgentRunTimestamp {
        self.deadline
    }

    /// Exact model profile pinned by the first reservation.
    #[must_use]
    pub const fn profile(self) -> ModelProfileReference {
        self.profile
    }

    /// The one Run bound to this scope, when it exists.
    #[must_use]
    pub const fn linked_run(self) -> Option<AgentRunId> {
        self.linked_run
    }

    /// Aggregate charged Intake calls, including unsettled and unknown calls.
    #[must_use]
    pub const fn usage(self) -> EditPhaseReservedUsage {
        self.usage
    }

    /// Derives the effective immutable Run budget at the actual Run start.
    pub fn run_budget(
        self,
        now: AgentRunTimestamp,
    ) -> Result<AgentRunBudget, IntakePhaseReservationFailure> {
        if now.unix_millis() < self.captured_at.unix_millis() {
            return Err(EditPhaseReservationFailure::TimestampRegressed.into());
        }
        if now >= self.deadline {
            return Err(EditPhaseReservationFailure::DeadlineReached.into());
        }
        let duration =
            AgentRunDurationLimit::from_millis(self.deadline.unix_millis() - now.unix_millis())
                .map_err(|_| IntakePhaseReservationFailure::InvalidBudget)?;
        Ok(AgentRunBudget::new(
            self.budget.turn_limit(),
            self.budget.prompt_token_limit(),
            self.budget.output_token_limit(),
            self.budget.action_limit(),
            duration,
            self.budget.repair_limit(),
        ))
    }
}

/// One process-local permission to dispatch the already charged Intake call.
#[derive(Debug)]
pub struct IntakePhaseReservation {
    anchor: IntakePhaseAnchor,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved_at: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
}

impl IntakePhaseReservation {
    /// Store adapter constructor. Issue only after committing the charge.
    #[must_use]
    pub const fn issue(
        anchor: IntakePhaseAnchor,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        reserved_at: AgentRunTimestamp,
        deadline: AgentRunTimestamp,
    ) -> Self {
        Self {
            anchor,
            kind,
            wire,
            reserved_at,
            deadline,
        }
    }

    /// Exact captured original and Review/window anchors.
    #[must_use]
    pub const fn anchor(&self) -> IntakePhaseAnchor {
        self.anchor
    }

    /// Initial or structural repair call already charged by the store.
    #[must_use]
    pub const fn kind(&self) -> EditPhaseCallKind {
        self.kind
    }

    /// Accounted actual wire projection.
    #[must_use]
    pub const fn wire(&self) -> EditPhaseWireCost {
        self.wire
    }

    /// Time used to reserve this provider call.
    #[must_use]
    pub const fn reserved_at(&self) -> AgentRunTimestamp {
        self.reserved_at
    }

    /// Original absolute Intake deadline.
    #[must_use]
    pub const fn deadline(&self) -> AgentRunTimestamp {
        self.deadline
    }
}

/// Closed dispositions for an Intake call that produced no usable Review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakePhaseRejection {
    /// The returned structure was invalid.
    InvalidStructure,
    /// The structure was valid but its proposal could not be used.
    RejectedReply,
    /// A known provider failure ended the call.
    ProviderFailure,
    /// The owned provider request stopped after cancellation.
    Cancelled,
    /// No definitive provider disposition could be established.
    Unknown,
}

/// A useful Intake result is one typed Review append committed with its observation.
#[derive(Debug, Clone, Copy)]
pub enum IntakePhaseFinish<'a> {
    /// Accepts one complete successor Review, with both provider token counts.
    Review {
        /// Exact Review including the expected previous prefix.
        review: &'a GoalIntakeReview,
        /// Actual reported prompt tokens.
        prompt_tokens: u32,
        /// Actual reported output tokens.
        output_tokens: u32,
    },
    /// Observes a completed failure without editing the Review.
    ObservedFailure {
        /// Closed call failure.
        kind: IntakePhaseRejection,
        /// Actual reported prompt tokens when present.
        reported_prompt_tokens: Option<u32>,
        /// Actual reported output tokens when present.
        reported_output_tokens: Option<u32>,
    },
}

/// Closed failures for capture-bound Intake reservations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakePhaseReservationFailure {
    /// Shared wire, deadline, usage, dispatch or persistence failure.
    Accounting(EditPhaseReservationFailure),
    /// Capture, source window, Goal revision or Worktree is invalid.
    InvalidAnchor,
    /// Original allowance or time cannot be represented.
    InvalidBudget,
    /// Another call pinned a different budget, profile or capture.
    ScopeConflict,
    /// The expected Review version is no longer current.
    ReviewConflict,
    /// The proposed Review is not one valid append for the bound window.
    InvalidReview,
}

impl From<EditPhaseReservationFailure> for IntakePhaseReservationFailure {
    fn from(error: EditPhaseReservationFailure) -> Self {
        Self::Accounting(error)
    }
}

impl fmt::Display for IntakePhaseReservationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accounting(error) => error.fmt(f),
            Self::InvalidAnchor => f.write_str("Intake capture binding is invalid"),
            Self::InvalidBudget => f.write_str("Intake budget is invalid"),
            Self::ScopeConflict => f.write_str("Intake scope conflicts with the pinned allowance"),
            Self::ReviewConflict => f.write_str("Intake Review version is stale"),
            Self::InvalidReview => f.write_str("Intake reply is not the bound Review append"),
        }
    }
}

impl Error for IntakePhaseReservationFailure {}

/// Future returned by the capture-bound reservation port.
pub type IntakePhaseReservationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, IntakePhaseReservationFailure>> + Send + 'a>>;

/// Durable provider accounting on the existing Goal and Run storage authority.
pub trait IntakePhaseReservationStore: Send + Sync {
    /// Commits an initial, complete literal Review resolved by trusted Core input.
    /// Pins the original allowance without recording a provider call. This is
    /// create-only. After an uncertain acknowledgement, read the durable Review
    /// and accounting rather than replacing or replaying the commit.
    fn commit_bound_intake<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: IntakePhaseAnchor,
        review: &'a GoalIntakeReview,
        budget: AgentRunBudget,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, IntakePhaseAccounting>;

    /// Pins the budget on first use. Charges one initial call and at most one
    /// structural repair for the same window and Review.
    fn reserve_intake_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: IntakePhaseAnchor,
        kind: EditPhaseCallKind,
        budget: AgentRunBudget,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, IntakePhaseReservation>;

    /// Settles the call once, atomically appending any accepted Review.
    fn finish_intake_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: IntakePhaseReservation,
        outcome: IntakePhaseFinish<'a>,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, ()>;

    /// Reads the pinned allowance and charged calls after capture, promotion or Run creation.
    fn load_intake_phase_accounting<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> IntakePhaseReservationFuture<'a, Option<IntakePhaseAccounting>>;
}
