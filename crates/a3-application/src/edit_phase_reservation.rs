//! Durable, content-free accounting for a single candidate Edit provider request.

use crate::{EditNoChangeReceipt, EditNoChangeWindow, EditPhaseAnchor, PreparedEditPhase};
use a3_domain::{
    AgentBudgetDimension, AgentRunId, AgentRunTimestamp, ContentHash, ModelProfile,
    ProjectIdentity, PublishedIndex,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// Selects the first call or the one permitted structural repair of its exact job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseCallKind {
    /// First call for the bound job.
    Initial,
    /// Repair after a completed, structurally invalid first reply.
    StructuralRepair,
}

/// Complete wire projection fingerprint and conservative token reservations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditPhaseWireCost {
    digest: ContentHash,
    request_bytes: u32,
    prompt_estimate: u32,
    output_reserve: u32,
}

impl EditPhaseWireCost {
    /// Requires nonempty wire data and positive prompt and output reservations.
    pub const fn new(
        digest: ContentHash,
        request_bytes: u32,
        prompt_estimate: u32,
        output_reserve: u32,
    ) -> Result<Self, EditPhaseReservationFailure> {
        if request_bytes == 0 || prompt_estimate == 0 || output_reserve == 0 {
            return Err(EditPhaseReservationFailure::InvalidWireCost);
        }
        Ok(Self {
            digest,
            request_bytes,
            prompt_estimate,
            output_reserve,
        })
    }

    /// Digest of the actual fully accounted wire request.
    #[must_use]
    pub const fn digest(self) -> ContentHash {
        self.digest
    }

    /// Size of the serialized wire request in bytes.
    #[must_use]
    pub const fn request_bytes(self) -> u32 {
        self.request_bytes
    }

    /// Conservatively accounted input tokens, including protocol costs.
    #[must_use]
    pub const fn prompt_estimate(self) -> u32 {
        self.prompt_estimate
    }

    /// Maximum output tokens reserved for this call.
    #[must_use]
    pub const fn output_reserve(self) -> u32 {
        self.output_reserve
    }
}

/// One process-local capability to use a committed reservation and finish it once.
#[derive(Debug)]
pub struct EditPhaseReservation {
    anchor: EditPhaseAnchor,
    job_digest: ContentHash,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved_at: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    source_read_key: Option<ContentHash>,
    request_digest: Option<ContentHash>,
}

impl EditPhaseReservation {
    /// Store adapter constructor. Return this capability only after the reservation commits.
    #[must_use]
    pub fn issue(
        phase: &PreparedEditPhase,
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
            source_read_key: None,
            request_digest: None,
        }
    }

    /// Current execution anchors used to create the reservation.
    #[must_use]
    pub const fn anchor(&self) -> EditPhaseAnchor {
        self.anchor
    }

    /// Full binding digest, shared by the original call and its structural repair.
    #[must_use]
    pub const fn job_digest(&self) -> ContentHash {
        self.job_digest
    }

    /// Original or one-shot repair.
    #[must_use]
    pub const fn kind(&self) -> EditPhaseCallKind {
        self.kind
    }

    /// Full wire identity and charge.
    #[must_use]
    pub const fn wire(&self) -> EditPhaseWireCost {
        self.wire
    }

    /// Time at which the durable consumption began.
    #[must_use]
    pub const fn reserved_at(&self) -> AgentRunTimestamp {
        self.reserved_at
    }

    /// Absolute deadline inherited from the original run start.
    #[must_use]
    pub const fn deadline(&self) -> AgentRunTimestamp {
        self.deadline
    }

    /// Store adapter binding to the exact committed Edit3 request, including any Core frame.
    /// A digest is descriptive and never reconstructs a parent-pass or patch proposal.
    #[must_use]
    pub const fn with_request_digest(mut self, digest: ContentHash) -> Self {
        self.request_digest = Some(digest);
        self
    }
    /// Returns the request identity authenticated by the issuing Store; None is legacy-only.
    #[must_use]
    pub const fn request_digest(&self) -> Option<ContentHash> {
        self.request_digest
    }

    /// Binds the permit to the durable original read linked in the same transaction.
    #[must_use]
    pub const fn with_source_read_key(mut self, key: ContentHash) -> Self {
        self.source_read_key = Some(key);
        self
    }

    /// Returns the identity of the tracked acquisition sent by this permit.
    #[must_use]
    pub const fn source_read_key(&self) -> Option<ContentHash> {
        self.source_read_key
    }
}

/// Closed outcomes observed by the caller after provider dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseObservationOutcome {
    /// One complete reply passed the structural decoder.
    AcceptedReply,
    /// A structurally valid reply could not be used and grants no structural repair.
    RejectedReply,
    /// The owning original deadline elapsed; settles stale with known consumption, never repair.
    DeadlineReached,
    /// Core lowered a complete current Existing/Replace reply to the unchanged
    /// full file. This grants neither structural repair nor source delivery.
    NoChange,
    /// One complete reply failed structural decoding and may admit the sole repair.
    InvalidStructure,
    /// A definite provider or transport failure.
    ProviderFailure,
    /// Caller cancellation with a known end to the owned request.
    Cancelled,
    /// The outcome of the dispatched request could not be established.
    Unknown,
}

/// A model request or bounded size halt, never a project fact, source credit or tool authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseHalt {
    /// The model requested evidence; unsupported/unoffered requests remain terminal.
    NeedEvidence(crate::EditEvidenceNeed),
    /// The model could not express this bounded edit.
    TooLarge,
}

/// Cold descriptive observation of one exact v3 request. It cannot authorize dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditPhaseHaltReceipt {
    disposition: EditPhaseHalt,
    request: ContentHash,
    wire: EditPhaseWireCost,
    finished_at: AgentRunTimestamp,
}
impl EditPhaseHaltReceipt {
    /// Adapter constructor after all request, reservation, usage and live-anchor checks.
    #[must_use]
    pub const fn from_recorded(
        disposition: EditPhaseHalt,
        request: ContentHash,
        wire: EditPhaseWireCost,
        finished_at: AgentRunTimestamp,
    ) -> Self {
        Self {
            disposition,
            request,
            wire,
            finished_at,
        }
    }
    /// Exact nonmutating model disposition; no evidence sufficiency is implied.
    #[must_use]
    pub const fn disposition(self) -> EditPhaseHalt {
        self.disposition
    }
    /// Ordered neutral request identity, including the v3 schema and projection.
    #[must_use]
    pub const fn request_digest(self) -> ContentHash {
        self.request
    }
    /// Fully accounted provider wire bound before I/O.
    #[must_use]
    pub const fn wire(self) -> EditPhaseWireCost {
        self.wire
    }
    /// Durable terminal observation including owned Store I/O, without a refreshed deadline.
    #[must_use]
    pub const fn finished_at(self) -> AgentRunTimestamp {
        self.finished_at
    }
}

/// Optional reported token counts stay distinct from the pre-dispatch estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditPhaseObservation {
    outcome: EditPhaseObservationOutcome,
    reported_prompt_tokens: Option<u32>,
    reported_output_tokens: Option<u32>,
    finished_at: AgentRunTimestamp,
}

impl EditPhaseObservation {
    /// Captures only closed outcome, optional usage, and the finish time.
    ///
    /// A usable or structurally invalid reply requires both reported token counts.
    #[must_use]
    pub const fn new(
        outcome: EditPhaseObservationOutcome,
        reported_prompt_tokens: Option<u32>,
        reported_output_tokens: Option<u32>,
        finished_at: AgentRunTimestamp,
    ) -> Self {
        Self {
            outcome,
            reported_prompt_tokens,
            reported_output_tokens,
            finished_at,
        }
    }

    /// Provider/decoder disposition.
    #[must_use]
    pub const fn outcome(self) -> EditPhaseObservationOutcome {
        self.outcome
    }

    /// Provider-reported prompt tokens, if available.
    #[must_use]
    pub const fn reported_prompt_tokens(self) -> Option<u32> {
        self.reported_prompt_tokens
    }

    /// Provider-reported output tokens, if available.
    #[must_use]
    pub const fn reported_output_tokens(self) -> Option<u32> {
        self.reported_output_tokens
    }

    /// Observation time from the same clock as the reservation.
    #[must_use]
    pub const fn finished_at(self) -> AgentRunTimestamp {
        self.finished_at
    }
}

/// Cumulative candidate charges, including calls whose provider outcome is unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditPhaseReservedUsage {
    /// Number of committed reservations.
    pub calls: u32,
    /// Number of committed one-shot repair reservations.
    pub repairs: u32,
    /// Sum of the greater of reserved or reported prompt tokens for each call.
    pub prompt_tokens: u64,
    /// Sum of the greater of reserved or reported output tokens for each call.
    pub output_tokens: u64,
    /// Reservations with no durable completion observation.
    pub unobserved_calls: u32,
    /// Calls explicitly observed as unknown.
    pub unknown_calls: u32,
}

/// Closed failures for the reservation and finish CAS boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseReservationFailure {
    /// The selected storage contract has no Edit reservation support.
    UnsupportedSchema,
    /// The wire request has missing or invalid accounting.
    InvalidWireCost,
    /// A claimed usable or repairable reply lacked complete provider token reports.
    IncompleteProviderUsage,
    /// Current run, ledger, source snapshot or attempt no longer matches.
    StaleAnchor,
    /// Provider dispatch or a usable reply is past the original run deadline.
    DeadlineReached,
    /// Clock input predates the associated durable state.
    TimestampRegressed,
    /// The call or repair for this exact job was consumed already.
    AlreadyReserved,
    /// Another reservation is unobserved or has an uncertain provider outcome.
    UncertainProviderCall,
    /// Mutation recovery must establish fresh state before model work.
    MutationRecoveryRequired,
    /// A durable first structural failure is required before this repair.
    RepairNotAvailable,
    /// The repair would send the identical wire request again.
    UnchangedRepairWire,
    /// The active attempt has an unresolved structural failure on a different job.
    StructuralFailurePending,
    /// Reserving or accepting this call would exceed an accounted budget.
    BudgetExhausted(AgentBudgetDimension),
    /// The finish capability does not match an outstanding durable reservation.
    ObservationConflict,
    /// Stored accounting cannot be reconstructed.
    InvalidStoredData,
    /// An infrastructure error prevented a durable decision.
    Unavailable,
}

impl fmt::Display for EditPhaseReservationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedSchema => "Edit reservations are unsupported by this schema",
            Self::InvalidWireCost => "Edit wire accounting is invalid",
            Self::IncompleteProviderUsage => "Edit reply lacks complete provider usage",
            Self::StaleAnchor => "Edit reservation anchors are stale",
            Self::DeadlineReached => "Edit run deadline has been reached",
            Self::TimestampRegressed => "Edit accounting time regressed",
            Self::AlreadyReserved => "Edit call was already reserved",
            Self::UncertainProviderCall => "Edit run has an uncertain provider call",
            Self::MutationRecoveryRequired => "Edit run requires mutation recovery",
            Self::RepairNotAvailable => "Edit structural repair is unavailable",
            Self::UnchangedRepairWire => "Edit structural repair request did not change",
            Self::StructuralFailurePending => "Edit attempt has an unresolved structural failure",
            Self::BudgetExhausted(_) => "Edit call exceeds an accounted run budget",
            Self::ObservationConflict => "Edit observation does not match its reservation",
            Self::InvalidStoredData => "Stored Edit accounting is invalid",
            Self::Unavailable => "Edit reservation store is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for EditPhaseReservationFailure {}

/// Future returned by the durable Edit reservation boundary.
pub type EditPhaseReservationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, EditPhaseReservationFailure>> + Send + 'a>>;

/// Candidate Edit calls debit their owning run before any provider request.
pub trait EditPhaseReservationStore: Send + Sync {
    /// Reserves the v3 request identity and immutable actual generation/context caps before I/O.
    fn reserve_edit_job<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _phase: &'a PreparedEditPhase,
        _compiled: &'a crate::CompiledEditPhaseRequest,
        _wire: EditPhaseWireCost,
        _now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }
    /// Atomically settles v3 accounting with an optional regular halt; never grants repair for a Need.
    fn finish_edit_job<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _reservation: EditPhaseReservation,
        _compiled: &'a crate::CompiledEditPhaseRequest,
        _observation: EditPhaseObservation,
        _halt: Option<EditPhaseHalt>,
    ) -> EditPhaseReservationFuture<'a, ()> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }
    /// Reopens only the exact recorded model request disposition, without tool or resume authority.
    fn load_edit_halt<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _compiled: &'a crate::CompiledEditPhaseRequest,
    ) -> EditPhaseReservationFuture<'a, Option<EditPhaseHaltReceipt>> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }

    /// Consumes the budget and one exact dispatch capability in a single durable CAS.
    fn reserve_edit_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedEditPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation>;

    /// Reserves the exact wire with the current, journaled original read.
    fn reserve_edit_phase_with_source<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _phase: &'a PreparedEditPhase,
        _kind: EditPhaseCallKind,
        _wire: EditPhaseWireCost,
        _now: AgentRunTimestamp,
        _source: &'a crate::AcquiredSourceRead,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }

    /// Records the outcome once without refunding any charge or extending the deadline.
    fn finish_edit_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: EditPhaseReservation,
        observation: EditPhaseObservation,
    ) -> EditPhaseReservationFuture<'a, ()>;

    /// Records only Core-proven unchanged current Existing/Replace bytes with
    /// their exact source window, live publication and immutable full profile.
    /// Generic finish must not create this narrower cold-replay authority.
    fn finish_edit_no_change<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _reservation: EditPhaseReservation,
        _observation: EditPhaseObservation,
        _window: &'a EditNoChangeWindow,
        _current: &'a PublishedIndex,
        _profile: &'a ModelProfile,
    ) -> EditPhaseReservationFuture<'a, ()> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }

    /// Authenticates an actual terminal NoChange for this same still-active
    /// Existing/Replace job. Rejected/unknown/stale rows never become this fact.
    fn load_edit_no_change<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _phase: &'a PreparedEditPhase,
        _profile: &'a ModelProfile,
        _current: &'a PublishedIndex,
        _now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<EditNoChangeReceipt>> {
        Box::pin(async { Err(EditPhaseReservationFailure::UnsupportedSchema) })
    }

    /// Loads Run accounting, including linked Intake charges, without granting dispatch.
    fn load_edit_phase_usage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run: AgentRunId,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservedUsage>;
}
