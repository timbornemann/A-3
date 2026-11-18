//! Durable admission for a read-only source acquisition and its observed wire.

use crate::{
    BoundEditSource, EditPhaseAnchor, EditPhaseReservationFailure, SourceDeliveryFailure,
    SourceDeliveryProgress, SourceDeliveryScope,
};
use a3_domain::{
    AgentRunTimestamp, AgentToolAttempt, ContentHash, EvidenceRef, ProjectIdentity, ToolRunId,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// The one tool attempt durably charged before reading source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceReadAdmission {
    key: ContentHash,
    attempt: AgentToolAttempt,
}

impl SourceReadAdmission {
    /// Reconstructs the admission from the exact stored attempt.
    #[must_use]
    pub const fn new(key: ContentHash, attempt: AgentToolAttempt) -> Self {
        Self { key, attempt }
    }

    /// Returns the identity of the charged acquisition.
    #[must_use]
    pub const fn key(self) -> ContentHash {
        self.key
    }

    /// Returns the existing tool-attempt receipt for this read.
    #[must_use]
    pub const fn attempt(self) -> AgentToolAttempt {
        self.attempt
    }
}

/// Content-free stamp for a real original page journaled by the current acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquiredSourceRead {
    key: ContentHash,
    scope: SourceDeliveryScope,
    required: EvidenceRef,
    original: EvidenceRef,
}

impl AcquiredSourceRead {
    /// Storage reconstructs this only from the matching terminal ToolAction and original evidence.
    /// An Edit reservation independently checks this stamp inside its write transaction.
    pub fn from_recorded(
        key: ContentHash,
        scope: SourceDeliveryScope,
        required: EvidenceRef,
        original: EvidenceRef,
    ) -> Result<Self, SourceReadFailure> {
        if original.revision() != required.revision()
            || original.range().is_empty()
            || !original.range().contains(required.range())
        {
            return Err(SourceReadFailure::InvalidStoredData);
        }
        Ok(Self {
            key,
            scope,
            required,
            original,
        })
    }

    /// Returns the charged acquisition identity.
    #[must_use]
    pub const fn key(&self) -> ContentHash {
        self.key
    }

    /// Returns the exact current owner of the acquisition.
    #[must_use]
    pub const fn scope(&self) -> SourceDeliveryScope {
        self.scope
    }

    /// Returns the Core-selected prerequisite that was read.
    #[must_use]
    pub const fn required(&self) -> &EvidenceRef {
        &self.required
    }

    /// Returns the exact original evidence that was journaled.
    #[must_use]
    pub const fn original(&self) -> &EvidenceRef {
        &self.original
    }
}

/// Current source acquisition history and optional original stamp. No raw bytes are retained.
#[derive(Debug)]
pub struct SourceReadState {
    progress: SourceDeliveryProgress,
    acquired: Option<AcquiredSourceRead>,
    caller_relocalizations: Vec<crate::CallerRelocalizationReceipt>,
}

impl SourceReadState {
    /// Combines the replayed progress and its one current acquired original.
    #[must_use]
    pub const fn new(
        progress: SourceDeliveryProgress,
        acquired: Option<AcquiredSourceRead>,
        caller_relocalizations: Vec<crate::CallerRelocalizationReceipt>,
    ) -> Self {
        Self {
            progress,
            acquired,
            caller_relocalizations,
        }
    }

    /// Returns derived novelty and the charged pending acquisition, if any.
    #[must_use]
    pub const fn progress(&self) -> &SourceDeliveryProgress {
        &self.progress
    }

    /// Returns the bounded closed results of completed caller lookups.
    #[must_use]
    pub fn caller_relocalizations(&self) -> &[crate::CallerRelocalizationReceipt] {
        &self.caller_relocalizations
    }

    /// Returns the exact original stamp only when its acquisition may still be delivered.
    #[must_use]
    pub const fn acquired(&self) -> Option<&AcquiredSourceRead> {
        self.acquired.as_ref()
    }
}

/// Typed failures for durable read admission and replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceReadFailure {
    /// The selected knowledge schema does not support this contract.
    UnsupportedSchema,
    /// The binding cannot be proved against the live durable owner.
    Reservation(EditPhaseReservationFailure),
    /// The charged acquisition exceeds the bounded source policy.
    Progress(SourceDeliveryFailure),
    /// The exact tool attempt could not be durably charged.
    ToolAttempt(crate::AgentRecoveryStoreFailure),
    /// The acquisition has no eligible terminal read or is already settled.
    NotPending,
    /// The stored source relation cannot be verified.
    InvalidStoredData,
    /// A storage operation could not complete.
    Unavailable,
}

impl fmt::Display for SourceReadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema => formatter.write_str("source read tracking is unsupported"),
            Self::Reservation(error) => fmt::Display::fmt(error, formatter),
            Self::Progress(error) => fmt::Display::fmt(error, formatter),
            Self::ToolAttempt(error) => fmt::Display::fmt(error, formatter),
            Self::NotPending => formatter.write_str("source acquisition cannot be settled"),
            Self::InvalidStoredData => formatter.write_str("stored source acquisition is invalid"),
            Self::Unavailable => formatter.write_str("source acquisition store is unavailable"),
        }
    }
}

impl Error for SourceReadFailure {}

/// Future returned by the durable source read boundary.
pub type SourceReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SourceReadFailure>> + Send + 'a>>;

/// Candidate-only admission and replay. The existing journal still owns the tool result.
pub trait SourceReadStore: Send + Sync {
    /// Atomically charges both source admission and the existing tool attempt.
    fn begin_source_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
        source: &'a BoundEditSource,
        tool: ToolRunId,
        now: AgentRunTimestamp,
    ) -> SourceReadFuture<'a, SourceReadAdmission>;

    /// Atomically charges the one scoped direct-caller lookup and its existing tool attempt.
    fn begin_caller_relocalization<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        job: &'a crate::CallerRelocalization,
        tool: ToolRunId,
        now: AgentRunTimestamp,
    ) -> SourceReadFuture<'a, crate::CallerRelocalizationAdmission>;

    /// Replays this step's acquisition history against the exact live publication.
    fn load_source_read_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
    ) -> SourceReadFuture<'a, SourceReadState>;

    /// Settles an unsent terminal acquisition without credit. An uncertain call remains charged and pending.
    fn abandon_source_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
        key: ContentHash,
    ) -> SourceReadFuture<'a, ()>;
}
