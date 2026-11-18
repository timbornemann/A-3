//! Bounded source acquisition and observed wire delivery for one active edit attempt.

use crate::{ContextOriginalSource, EditEvidenceNeed, EditPhaseAnchor};
use a3_domain::{
    AgentRunId, ContentHash, EvidenceRef, FileRevision, GoalContractReference, GraphEndpoint,
    SnapshotId, SourceRange, TaskLedgerRevision, TaskStepAttemptNumber, TaskStepId,
    VerificationSpecId, WorktreeId,
};
use std::{error::Error, fmt};

const MAX_ACQUISITIONS: usize = 8;
const MAX_WITHOUT_NOVELTY: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceDeliveryScopeKind {
    Edit,
    HttpContext(ContentHash),
}

/// Stable owner of the source budget across journal events within one step attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDeliveryScope {
    kind: SourceDeliveryScopeKind,
    worktree: WorktreeId,
    goal: GoalContractReference,
    ledger: TaskLedgerRevision,
    run: AgentRunId,
    step: TaskStepId,
    attempt: TaskStepAttemptNumber,
    verification: VerificationSpecId,
    snapshot: SnapshotId,
}

impl SourceDeliveryScope {
    /// Preserves the current task and source authority when a read advances the journal sequence.
    #[must_use]
    pub const fn from_edit_anchor(anchor: EditPhaseAnchor) -> Self {
        Self {
            kind: SourceDeliveryScopeKind::Edit,
            worktree: anchor.worktree(),
            goal: anchor.goal(),
            ledger: anchor.ledger(),
            run: anchor.run(),
            step: anchor.step(),
            attempt: anchor.attempt(),
            verification: anchor.verification(),
            snapshot: anchor.snapshot(),
        }
    }

    /// Describes one HTTP-plan S1 scope without manufacturing an active Edit anchor.
    /// The durable adapter must authenticate the accepted parent job before admission.
    #[must_use]
    pub const fn from_http_context(
        anchor: &crate::HttpCorrectionPlanAnchor,
        parent_job: ContentHash,
    ) -> Self {
        Self {
            kind: SourceDeliveryScopeKind::HttpContext(parent_job),
            worktree: anchor.worktree,
            goal: anchor.goal,
            ledger: anchor.ledger,
            run: anchor.run,
            step: anchor.step,
            attempt: anchor.failed_attempt,
            verification: anchor.verification,
            snapshot: anchor.snapshot,
        }
    }
}

/// One Core-selected attempt to acquire relevant original source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAcquisition {
    /// Read from the exact current file revision selected by the Core.
    Read(FileRevision),
    /// One bounded new localization for a specific missing relationship or endpoint.
    Relocalize {
        /// The closed kind of missing evidence.
        need: EditEvidenceNeed,
        /// Exact endpoint scoped to this attempt. Use a symbol for a symbol-specific gap.
        target: GraphEndpoint,
    },
}

/// Position of one started acquisition in the bounded attempt history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAcquisitionOrdinal(u8);

impl SourceAcquisitionOrdinal {
    /// Returns its one-based position.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Content-free ownership of one charged acquisition, before any source I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAcquisitionAdmission {
    scope: SourceDeliveryScope,
    ordinal: SourceAcquisitionOrdinal,
    acquisition: SourceAcquisition,
}

impl SourceAcquisitionAdmission {
    /// Returns the stable step-attempt owner.
    #[must_use]
    pub const fn scope(&self) -> SourceDeliveryScope {
        self.scope
    }

    /// Returns the acquisition's ordered position.
    #[must_use]
    pub const fn ordinal(&self) -> SourceAcquisitionOrdinal {
        self.ordinal
    }

    /// Returns the exact authorized acquisition descriptor.
    #[must_use]
    pub const fn acquisition(&self) -> &SourceAcquisition {
        &self.acquisition
    }
}

/// The exact original interval carried by a provider body. Paging cursors are not evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredSourceRange {
    snapshot: SnapshotId,
    evidence: EvidenceRef,
}

impl DeliveredSourceRange {
    /// Returns the current publication that authorized the source.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot
    }

    /// Returns the full observed file revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        self.evidence.revision()
    }

    /// Returns the half-open interval that was sent.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.evidence.range()
    }
}

/// Content-free source receipt tied to the exact dispatched provider body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWireDelivery {
    source: DeliveredSourceRange,
    wire_digest: ContentHash,
}

impl SourceWireDelivery {
    /// The caller must observe dispatch of the checked body containing this exact source.
    /// A read, context pack, reserved request or rejected send alone is insufficient.
    #[must_use]
    pub fn from_observed_dispatch(source: ContextOriginalSource, wire_digest: ContentHash) -> Self {
        Self::from_recorded_dispatch(
            source.snapshot_id(),
            EvidenceRef::new(source.revision().clone(), source.range()),
            wire_digest,
        )
    }

    /// Reconstructs metadata after storage proved an accepted reservation for this exact original.
    #[must_use]
    pub const fn from_recorded_dispatch(
        snapshot: SnapshotId,
        evidence: EvidenceRef,
        wire_digest: ContentHash,
    ) -> Self {
        Self {
            source: DeliveredSourceRange { snapshot, evidence },
            wire_digest,
        }
    }

    /// Returns the exact byte and file revision metadata included in the observed dispatch.
    #[must_use]
    pub const fn source(&self) -> &DeliveredSourceRange {
        &self.source
    }

    /// Returns the digest of the checked body actually dispatched.
    #[must_use]
    pub const fn wire_digest(&self) -> ContentHash {
        self.wire_digest
    }
}

/// Result of one acquisition. Only new delivered bytes reset the novelty streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDeliveryOutcome {
    /// The acquisition produced no observed delivery, including capacity or dispatch rejection.
    NotDelivered,
    /// The dispatched source had an empty byte range.
    EmptyRange,
    /// The union of earlier delivered intervals already covered this range.
    AlreadyDelivered,
    /// A checked dispatch delivered additional original bytes in this exact revision.
    NewBytes(u32),
}

/// Replayable metadata for one settled acquisition, without source or request text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDeliveryReceipt {
    admission: SourceAcquisitionAdmission,
    delivery: Option<SourceWireDelivery>,
    outcome: SourceDeliveryOutcome,
}

impl SourceDeliveryReceipt {
    /// Returns the exact admission that was settled.
    #[must_use]
    pub const fn admission(&self) -> &SourceAcquisitionAdmission {
        &self.admission
    }

    /// Returns checked-wire metadata only for an observed dispatch.
    #[must_use]
    pub const fn delivery(&self) -> Option<&SourceWireDelivery> {
        self.delivery.as_ref()
    }

    /// Returns the derived change in delivered source coverage.
    #[must_use]
    pub const fn outcome(&self) -> SourceDeliveryOutcome {
        self.outcome
    }
}

/// Pure progress state. The owning controller persists admission and settlement separately.
#[derive(Debug)]
pub struct SourceDeliveryProgress {
    scope: SourceDeliveryScope,
    settled: Vec<SourceDeliveryReceipt>,
    pending: Option<SourceAcquisitionAdmission>,
}

impl SourceDeliveryProgress {
    /// Version of the fixed acquisition, novelty and relocalization policy.
    pub const POLICY_VERSION: u8 = 1;

    /// Starts accounting for one current step attempt.
    #[must_use]
    pub const fn new(scope: SourceDeliveryScope) -> Self {
        Self {
            scope,
            settled: Vec::new(),
            pending: None,
        }
    }

    /// Re-derives each settled observation. A persisted unfinished admission remains pending.
    pub fn replay(
        scope: SourceDeliveryScope,
        receipts: &[SourceDeliveryReceipt],
        pending: Option<SourceAcquisitionAdmission>,
    ) -> Result<Self, SourceDeliveryFailure> {
        let mut progress = Self::new(scope);
        for expected in receipts {
            if expected.admission.scope != scope {
                return Err(SourceDeliveryFailure::StaleScope);
            }
            let ordinal = progress.begin(scope, expected.admission.acquisition.clone())?;
            let actual = progress.finish(scope, ordinal, expected.delivery.clone())?;
            if actual != *expected {
                return Err(SourceDeliveryFailure::ReceiptMismatch);
            }
        }
        if let Some(expected) = pending {
            if expected.scope != scope {
                return Err(SourceDeliveryFailure::StaleScope);
            }
            if progress.begin(scope, expected.acquisition)? != expected.ordinal {
                return Err(SourceDeliveryFailure::ReceiptMismatch);
            }
        }
        Ok(progress)
    }

    /// Returns the settled acquisition history, at most eight observations.
    #[must_use]
    pub fn receipts(&self) -> &[SourceDeliveryReceipt] {
        &self.settled
    }

    /// Returns the charged acquisition whose delivery is still unobserved.
    #[must_use]
    pub const fn pending(&self) -> Option<&SourceAcquisitionAdmission> {
        self.pending.as_ref()
    }

    /// Admits one source acquisition before it runs. Metadata-only navigation is separate.
    pub fn begin(
        &mut self,
        current: SourceDeliveryScope,
        acquisition: SourceAcquisition,
    ) -> Result<SourceAcquisitionOrdinal, SourceDeliveryFailure> {
        if self.scope != current {
            return Err(SourceDeliveryFailure::StaleScope);
        }
        if self.pending.is_some() {
            return Err(SourceDeliveryFailure::AcquisitionPending);
        }
        if self.settled.len() >= MAX_ACQUISITIONS {
            return Err(SourceDeliveryFailure::AcquisitionLimit);
        }
        if self
            .settled
            .iter()
            .rev()
            .take(MAX_WITHOUT_NOVELTY)
            .filter(|receipt| !matches!(receipt.outcome, SourceDeliveryOutcome::NewBytes(_)))
            .count()
            == MAX_WITHOUT_NOVELTY
        {
            return Err(SourceDeliveryFailure::NoveltyLimit);
        }
        match &acquisition {
            SourceAcquisition::Read(revision) => self.require_same_revision(revision)?,
            SourceAcquisition::Relocalize { .. } => {
                if self
                    .settled
                    .iter()
                    .any(|receipt| receipt.admission.acquisition == acquisition)
                {
                    return Err(SourceDeliveryFailure::GapAlreadyRelocalized);
                }
            }
        }
        let ordinal = u8::try_from(self.settled.len() + 1)
            .map(SourceAcquisitionOrdinal)
            .map_err(|_| SourceDeliveryFailure::AcquisitionLimit)?;
        self.pending = Some(SourceAcquisitionAdmission {
            scope: current,
            ordinal,
            acquisition,
        });
        Ok(ordinal)
    }

    /// Settles one acquisition after observed dispatch, or with no delivery credit.
    /// An invalid observation leaves the charged acquisition pending.
    pub fn finish(
        &mut self,
        current: SourceDeliveryScope,
        ordinal: SourceAcquisitionOrdinal,
        delivery: Option<SourceWireDelivery>,
    ) -> Result<SourceDeliveryReceipt, SourceDeliveryFailure> {
        if self.scope != current {
            return Err(SourceDeliveryFailure::StaleScope);
        }
        let Some(admission) = self.pending.as_ref() else {
            return Err(SourceDeliveryFailure::ReceiptMismatch);
        };
        if ordinal != admission.ordinal {
            return Err(SourceDeliveryFailure::ReceiptMismatch);
        }
        let outcome = match delivery.as_ref() {
            None => SourceDeliveryOutcome::NotDelivered,
            Some(delivery) => self.measure_delivery(&admission.acquisition, delivery.source())?,
        };
        let admission = self
            .pending
            .take()
            .ok_or(SourceDeliveryFailure::ReceiptMismatch)?;
        let receipt = SourceDeliveryReceipt {
            admission,
            delivery,
            outcome,
        };
        self.settled.push(receipt.clone());
        Ok(receipt)
    }

    fn require_same_revision(&self, revision: &FileRevision) -> Result<(), SourceDeliveryFailure> {
        if self
            .settled
            .iter()
            .filter_map(SourceDeliveryReceipt::delivery)
            .any(|receipt| {
                let prior = receipt.source().revision();
                prior.path() == revision.path() && prior != revision
            })
        {
            return Err(SourceDeliveryFailure::RevisionMismatch);
        }
        Ok(())
    }

    fn measure_delivery(
        &self,
        acquisition: &SourceAcquisition,
        source: &DeliveredSourceRange,
    ) -> Result<SourceDeliveryOutcome, SourceDeliveryFailure> {
        if source.snapshot_id() != self.scope.snapshot {
            return Err(SourceDeliveryFailure::StaleDelivery);
        }
        if let SourceAcquisition::Read(expected) = acquisition
            && source.revision() != expected
        {
            return Err(SourceDeliveryFailure::RevisionMismatch);
        }
        self.require_same_revision(source.revision())?;
        let range = source.range();
        if range.is_empty() {
            return Ok(SourceDeliveryOutcome::EmptyRange);
        }
        let mut prior = self
            .settled
            .iter()
            .filter_map(SourceDeliveryReceipt::delivery)
            .map(SourceWireDelivery::source)
            .filter(|prior| prior.revision() == source.revision())
            .map(DeliveredSourceRange::range)
            .filter(|prior| !prior.is_empty())
            .collect::<Vec<_>>();
        prior.sort_unstable_by_key(|range| range.start_byte());

        // The cursor advances through the union of half-open intervals.
        let end = range.end_byte();
        let mut cursor = range.start_byte();
        let mut additional = 0;
        for covered in prior {
            if covered.end_byte() <= cursor {
                continue;
            }
            if covered.start_byte() >= end || cursor == end {
                break;
            }
            additional += covered.start_byte().saturating_sub(cursor);
            cursor = covered.end_byte().min(end);
        }
        additional += end - cursor;
        Ok(if additional == 0 {
            SourceDeliveryOutcome::AlreadyDelivered
        } else {
            SourceDeliveryOutcome::NewBytes(additional)
        })
    }
}

/// Content-free reason the current owner cannot acquire or credit source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDeliveryFailure {
    /// The run, goal, step attempt or snapshot differs.
    StaleScope,
    /// The previous acquisition is still pending.
    AcquisitionPending,
    /// All eight acquisitions are spent.
    AcquisitionLimit,
    /// Two consecutive acquisitions delivered no new bytes.
    NoveltyLimit,
    /// This exact gap already spent its relocalization.
    GapAlreadyRelocalized,
    /// A receipt was out of order, duplicated or inconsistent on replay.
    ReceiptMismatch,
    /// The supplied source belongs to another published snapshot.
    StaleDelivery,
    /// The supplied path or full-content digest differs from the required revision.
    RevisionMismatch,
}

impl fmt::Display for SourceDeliveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StaleScope => "source acquisition owner changed",
            Self::AcquisitionPending => "source acquisition is already pending",
            Self::AcquisitionLimit => "source acquisition limit reached",
            Self::NoveltyLimit => "consecutive source acquisitions delivered no new bytes",
            Self::GapAlreadyRelocalized => "source gap already used its relocalization",
            Self::ReceiptMismatch => "source acquisition receipt does not match",
            Self::StaleDelivery => "source delivery belongs to a different snapshot",
            Self::RevisionMismatch => "source delivery does not match the file revision",
        })
    }
}

impl Error for SourceDeliveryFailure {}

#[cfg(test)]
mod tests;
