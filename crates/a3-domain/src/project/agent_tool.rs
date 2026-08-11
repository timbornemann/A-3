use super::{
    AgentAction, AgentRunId, AgentRunTimestamp, EvidenceRef, FileRevision, SnapshotId, SourceRange,
    TaskEvidenceId, ToolRunId,
};
use std::error::Error;
use std::fmt;

const MAX_AGENT_TOOL_EVIDENCE: usize = 100;

/// Content-free identity of one exact model-selected mutating action.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MutationActionFingerprint([u8; 32]);

impl MutationActionFingerprint {
    /// Derives a stable identity from a structured patch or discovered-command selection only.
    pub fn from_action(action: &AgentAction) -> Result<Self, MutationActionFingerprintError> {
        let mut hasher = blake3::Hasher::new_derive_key("a3.agent-mutation-action.v1");
        match action {
            AgentAction::ApplyPatch(patch) => {
                hasher.update(&[0]);
                hasher.update(&patch.digest().as_bytes());
            }
            AgentAction::Run(run) => {
                hasher.update(&[1]);
                hasher.update(run.step_id().as_bytes());
                hasher.update(run.command_id().as_bytes());
            }
            AgentAction::Search(_)
            | AgentAction::Inspect(_)
            | AgentAction::UpdateLedger(_)
            | AgentAction::Finish(_) => return Err(MutationActionFingerprintError),
        }
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    /// Reconstructs a persisted content-free fingerprint.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for MutationActionFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MutationActionFingerprint([REDACTED])")
    }
}

/// A non-mutating AgentAction cannot have a mutation fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationActionFingerprintError;

impl fmt::Display for MutationActionFingerprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent action is not a worktree mutation")
    }
}

impl Error for MutationActionFingerprintError {}

impl ToolRunId {
    /// Derives a unique run-local identity from a one-based selected-action ordinal.
    pub fn for_agent_action_v1(
        run_id: AgentRunId,
        action_ordinal: u32,
    ) -> Result<Self, ToolRunIdDerivationError> {
        if action_ordinal == 0 {
            return Err(ToolRunIdDerivationError);
        }
        let mut hasher = blake3::Hasher::new_derive_key("a3.agent-tool-run.v1");
        hasher.update(run_id.as_bytes());
        hasher.update(&action_ordinal.to_le_bytes());
        Ok(Self::from_bytes(*hasher.finalize().as_bytes()))
    }
}

/// A tool-run identity was requested without a valid selected-action ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolRunIdDerivationError;

impl fmt::Display for ToolRunIdDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("tool-run action ordinal must be non-zero")
    }
}

impl Error for ToolRunIdDerivationError {}

/// One-based durable attempt number for retries of the same logical tool action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentToolAttemptNumber(u32);

impl AgentToolAttemptNumber {
    /// First attempt of one logical tool action.
    pub const FIRST: Self = Self(1);

    /// Reconstructs a non-zero persisted attempt number.
    pub const fn new(value: u32) -> Result<Self, AgentToolAttemptNumberError> {
        if value == 0 {
            return Err(AgentToolAttemptNumberError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A tool-attempt number was zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentToolAttemptNumberError;

impl fmt::Display for AgentToolAttemptNumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent tool-attempt number must be non-zero")
    }
}

impl Error for AgentToolAttemptNumberError {}

/// Durable lifecycle state of one bounded attempt of a logical agent tool action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentToolAttemptStatus {
    /// The application persisted the attempt before invoking the tool boundary.
    InFlight,
    /// A normalized successful result was atomically journaled.
    Succeeded,
    /// The tool boundary failed before producing an admissible result.
    Failed,
    /// Cooperative cancellation stopped the tool boundary.
    Cancelled,
    /// Central policy denied the tool boundary.
    Denied,
    /// The application restarted while this attempt was still in flight.
    Interrupted,
}

impl AgentToolAttemptStatus {
    /// Returns whether this attempt can no longer change lifecycle state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::InFlight)
    }
}

/// Content-free durable projection of one tool attempt across application restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentToolAttempt {
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    status: AgentToolAttemptStatus,
    started_at: AgentRunTimestamp,
    updated_at: AgentRunTimestamp,
}

impl AgentToolAttempt {
    /// Creates or reconstructs a lifecycle projection after validating its chronology.
    pub fn new(
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        status: AgentToolAttemptStatus,
        started_at: AgentRunTimestamp,
        updated_at: AgentRunTimestamp,
    ) -> Result<Self, AgentToolAttemptError> {
        if updated_at < started_at {
            return Err(AgentToolAttemptError::TimestampRegressed);
        }
        Ok(Self {
            tool_run_id,
            attempt,
            run_id,
            snapshot_id,
            status,
            started_at,
            updated_at,
        })
    }

    /// Returns the logical tool-run identity shared by its retries and final result.
    #[must_use]
    pub const fn tool_run_id(self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the one-based retry position of this attempt.
    #[must_use]
    pub const fn attempt(self) -> AgentToolAttemptNumber {
        self.attempt
    }

    /// Returns the owning agent run.
    #[must_use]
    pub const fn run_id(self) -> AgentRunId {
        self.run_id
    }

    /// Returns the immutable repository snapshot observed by this attempt.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the current durable lifecycle state.
    #[must_use]
    pub const fn status(self) -> AgentToolAttemptStatus {
        self.status
    }

    /// Returns when the attempt became durable, before tool invocation.
    #[must_use]
    pub const fn started_at(self) -> AgentRunTimestamp {
        self.started_at
    }

    /// Returns the latest lifecycle-transition timestamp.
    #[must_use]
    pub const fn updated_at(self) -> AgentRunTimestamp {
        self.updated_at
    }
}

/// A persisted tool-attempt lifecycle projection violated a domain invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolAttemptError {
    /// The lifecycle update preceded the durable start.
    TimestampRegressed,
}

impl fmt::Display for AgentToolAttemptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent tool-attempt timestamp regressed")
    }
}

impl Error for AgentToolAttemptError {}

/// Closed kind of adapter boundary represented by one mutating tool attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentMutationKind {
    /// Structured hash-protected workspace patch.
    Patch,
    /// Direct argv process selected from the confirmed command catalog.
    Process,
    /// Pre-V22 in-flight attempt whose original boundary kind was not persisted.
    UnclassifiedLegacy,
}

/// Public three-state classification of a mutating action's observable application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MutationApplicationState {
    /// The adapter reported a complete or partial visible application.
    Applied,
    /// The adapter contract proves that no mutation boundary effect was applied.
    NotApplied,
    /// A boundary effect could have occurred but cannot be proven either way.
    Unknown,
}

/// Durable safety state attached to an unknown mutation application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MutationReconciliation {
    /// No authoritative post-failure repository snapshot has been adopted yet.
    Required,
    /// A full authoritative snapshot was adopted without claiming the original action succeeded.
    Reconciled {
        /// Published snapshot that became the new safe mutation baseline.
        snapshot_id: SnapshotId,
    },
    /// The reconciled baseline was acknowledged by a durable recovery Replan.
    Replanned {
        /// Published snapshot retained as the safe post-recovery baseline.
        snapshot_id: SnapshotId,
    },
}

/// Durable disposition retaining the public three-state outcome and reconciliation gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentMutationDisposition {
    /// The action produced a known complete or partial boundary effect.
    Applied,
    /// The action did not cross the mutation boundary.
    NotApplied,
    /// The action's boundary effect remains unknowable, with an explicit safety gate.
    Unknown(MutationReconciliation),
}

impl AgentMutationDisposition {
    /// Returns the user-visible three-state classification.
    #[must_use]
    pub const fn application_state(self) -> MutationApplicationState {
        match self {
            Self::Applied => MutationApplicationState::Applied,
            Self::NotApplied => MutationApplicationState::NotApplied,
            Self::Unknown(_) => MutationApplicationState::Unknown,
        }
    }

    /// Returns whether another mutation must remain blocked.
    #[must_use]
    pub const fn requires_reconciliation(self) -> bool {
        matches!(self, Self::Unknown(MutationReconciliation::Required))
    }

    /// Returns whether a full baseline exists but recovery still must force Replan.
    #[must_use]
    pub const fn requires_replan(self) -> bool {
        matches!(
            self,
            Self::Unknown(MutationReconciliation::Reconciled { .. })
        )
    }

    /// Returns whether the Unknown safety workflow is fully acknowledged.
    #[must_use]
    pub const fn permits_future_mutation(self) -> bool {
        !matches!(
            self,
            Self::Unknown(
                MutationReconciliation::Required | MutationReconciliation::Reconciled { .. }
            )
        )
    }

    /// Returns the adopted post-recovery snapshot, if reconciliation completed.
    #[must_use]
    pub const fn reconciled_snapshot_id(self) -> Option<SnapshotId> {
        match self {
            Self::Unknown(
                MutationReconciliation::Reconciled { snapshot_id }
                | MutationReconciliation::Replanned { snapshot_id },
            ) => Some(snapshot_id),
            Self::Applied | Self::NotApplied | Self::Unknown(MutationReconciliation::Required) => {
                None
            }
        }
    }
}

/// Content-free durable projection of one mutating tool attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMutationAttempt {
    tool_attempt: AgentToolAttempt,
    fingerprint: MutationActionFingerprint,
    kind: AgentMutationKind,
    disposition: AgentMutationDisposition,
}

impl AgentMutationAttempt {
    /// Creates or reconstructs one mutation projection after cross-state validation.
    pub fn new(
        tool_attempt: AgentToolAttempt,
        fingerprint: MutationActionFingerprint,
        kind: AgentMutationKind,
        disposition: AgentMutationDisposition,
    ) -> Result<Self, AgentMutationAttemptError> {
        if tool_attempt.status() == AgentToolAttemptStatus::InFlight
            && disposition != AgentMutationDisposition::Unknown(MutationReconciliation::Required)
        {
            return Err(AgentMutationAttemptError::InFlightDisposition);
        }
        if tool_attempt.status() == AgentToolAttemptStatus::Succeeded
            && disposition != AgentMutationDisposition::Applied
        {
            return Err(AgentMutationAttemptError::SucceededDisposition);
        }
        Ok(Self {
            tool_attempt,
            fingerprint,
            kind,
            disposition,
        })
    }

    /// Returns the shared lifecycle projection.
    #[must_use]
    pub const fn tool_attempt(self) -> AgentToolAttempt {
        self.tool_attempt
    }

    /// Returns the exact content-free action identity.
    #[must_use]
    pub const fn fingerprint(self) -> MutationActionFingerprint {
        self.fingerprint
    }

    /// Returns the closed adapter-boundary kind.
    #[must_use]
    pub const fn kind(self) -> AgentMutationKind {
        self.kind
    }

    /// Returns application and reconciliation state.
    #[must_use]
    pub const fn disposition(self) -> AgentMutationDisposition {
        self.disposition
    }
}

/// A reconstructed mutating attempt combined incompatible lifecycle and disposition states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMutationAttemptError {
    /// An in-flight boundary was classified before a terminal observation existed.
    InFlightDisposition,
    /// A successfully journaled tool result did not carry an applied mutation disposition.
    SucceededDisposition,
}

impl fmt::Display for AgentMutationAttemptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InFlightDisposition => "in-flight mutation must remain unknown and unreconciled",
            Self::SucceededDisposition => "successful mutation must be classified as applied",
        })
    }
}

impl Error for AgentMutationAttemptError {}

/// Exact current source location retained by one read-only agent tool result.
#[derive(Clone, PartialEq, Eq)]
pub enum AgentToolEvidenceLocation {
    /// A complete content-addressed repository file.
    File(FileRevision),
    /// A half-open source range within one content-addressed repository file.
    Span(EvidenceRef),
}

impl AgentToolEvidenceLocation {
    /// Returns the exact source revision used to detect stale evidence.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        match self {
            Self::File(revision) => revision,
            Self::Span(evidence) => evidence.revision(),
        }
    }

    /// Returns the optional source range used for a line-level link.
    #[must_use]
    pub const fn range(&self) -> Option<SourceRange> {
        match self {
            Self::File(_) => None,
            Self::Span(evidence) => Some(evidence.range()),
        }
    }
}

impl fmt::Debug for AgentToolEvidenceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::File(_) => "AgentToolEvidenceLocation::File([REDACTED])",
            Self::Span(_) => "AgentToolEvidenceLocation::Span([REDACTED])",
        })
    }
}

/// Deterministically identified source evidence admitted from one agent read.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentToolEvidence {
    id: TaskEvidenceId,
    location: AgentToolEvidenceLocation,
}

impl AgentToolEvidence {
    /// Derives one evidence ID from an exact current file revision.
    #[must_use]
    pub fn for_file(revision: FileRevision) -> Self {
        let id = derive_evidence_id(0, &revision, None);
        Self {
            id,
            location: AgentToolEvidenceLocation::File(revision),
        }
    }

    /// Derives one evidence ID from an exact current source span.
    #[must_use]
    pub fn for_span(evidence: EvidenceRef) -> Self {
        let id = derive_evidence_id(1, evidence.revision(), Some(evidence.range()));
        Self {
            id,
            location: AgentToolEvidenceLocation::Span(evidence),
        }
    }

    /// Returns the stable evidence identity used by the Task Ledger.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the exact source location for freshness checks and navigation.
    #[must_use]
    pub const fn location(&self) -> &AgentToolEvidenceLocation {
        &self.location
    }
}

impl fmt::Debug for AgentToolEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentToolEvidence")
            .field("id", &self.id)
            .field("location", &self.location)
            .finish()
    }
}

fn derive_evidence_id(
    kind: u8,
    revision: &FileRevision,
    range: Option<SourceRange>,
) -> TaskEvidenceId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.agent-tool-evidence.v1");
    hasher.update(&[kind]);
    hasher.update(&(revision.path().as_bytes().len() as u64).to_le_bytes());
    hasher.update(revision.path().as_bytes());
    hasher.update(revision.content_hash().as_bytes());
    if let Some(range) = range {
        hasher.update(&range.start_byte().to_le_bytes());
        hasher.update(&range.end_byte().to_le_bytes());
    }
    TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes())
}

/// Canonical bounded evidence admitted by one tool result for one immutable snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentToolEvidenceSet {
    snapshot_id: SnapshotId,
    evidence: Vec<AgentToolEvidence>,
}

impl AgentToolEvidenceSet {
    /// Sorts evidence by ID and rejects duplicates or an oversized result.
    pub fn new(
        snapshot_id: SnapshotId,
        mut evidence: Vec<AgentToolEvidence>,
    ) -> Result<Self, AgentToolEvidenceSetError> {
        if evidence.len() > MAX_AGENT_TOOL_EVIDENCE {
            return Err(AgentToolEvidenceSetError::TooMuchEvidence {
                actual: evidence.len(),
            });
        }
        evidence.sort_by_key(AgentToolEvidence::id);
        if evidence.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(AgentToolEvidenceSetError::DuplicateEvidence);
        }
        Ok(Self {
            snapshot_id,
            evidence,
        })
    }

    /// Returns the immutable snapshot in which every source was proven current.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns evidence in stable TaskEvidenceId order.
    #[must_use]
    pub fn evidence(&self) -> &[AgentToolEvidence] {
        &self.evidence
    }

    /// Returns whether no clickable source was admitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.evidence.is_empty()
    }
}

/// A read tool attempted to cross the fixed evidence boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolEvidenceSetError {
    /// More than one hundred source locations were returned.
    TooMuchEvidence {
        /// Observed evidence count.
        actual: usize,
    },
    /// The same exact source location appeared more than once.
    DuplicateEvidence,
}

impl fmt::Display for AgentToolEvidenceSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMuchEvidence { actual } => write!(
                formatter,
                "agent tool result has {actual} evidence items; maximum is {MAX_AGENT_TOOL_EVIDENCE}"
            ),
            Self::DuplicateEvidence => {
                formatter.write_str("agent tool result repeats an exact evidence location")
            }
        }
    }
}

impl Error for AgentToolEvidenceSetError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentHash, RepositoryPath, SourcePosition, SourceRange};

    #[test]
    fn tool_attempt_requires_monotone_time_and_a_non_zero_number() -> Result<(), Box<dyn Error>> {
        let started_at = AgentRunTimestamp::from_unix_millis(20)?;
        let attempt = AgentToolAttempt::new(
            ToolRunId::from_bytes([3; 32]),
            AgentToolAttemptNumber::FIRST,
            AgentRunId::from_bytes([4; 32]),
            SnapshotId::from_bytes([5; 32]),
            AgentToolAttemptStatus::InFlight,
            started_at,
            AgentRunTimestamp::from_unix_millis(21)?,
        )?;

        assert_eq!(attempt.attempt(), AgentToolAttemptNumber::FIRST);
        assert!(!attempt.status().is_terminal());
        assert_eq!(
            AgentToolAttemptNumber::new(0),
            Err(AgentToolAttemptNumberError)
        );
        assert!(matches!(
            AgentToolAttempt::new(
                attempt.tool_run_id(),
                attempt.attempt(),
                attempt.run_id(),
                attempt.snapshot_id(),
                AgentToolAttemptStatus::Interrupted,
                started_at,
                AgentRunTimestamp::from_unix_millis(19)?,
            ),
            Err(AgentToolAttemptError::TimestampRegressed)
        ));
        assert!(AgentToolAttemptStatus::Interrupted.is_terminal());
        Ok(())
    }

    #[test]
    fn evidence_is_content_bound_canonical_and_bounded() -> Result<(), Box<dyn Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let span = EvidenceRef::new(
            revision.clone(),
            SourceRange::new(5, 10, SourcePosition::new(1, 0), SourcePosition::new(1, 5))?,
        );
        let file = AgentToolEvidence::for_file(revision);
        let source = AgentToolEvidence::for_span(span.clone());
        let repeated = AgentToolEvidence::for_span(span);
        let set = AgentToolEvidenceSet::new(
            SnapshotId::from_bytes([2; 32]),
            vec![source.clone(), file.clone()],
        )?;

        assert_eq!(set.evidence().len(), 2);
        assert!(set.evidence()[0].id() < set.evidence()[1].id());
        assert_ne!(file.id(), source.id());
        assert_eq!(
            AgentToolEvidenceSet::new(SnapshotId::from_bytes([2; 32]), vec![source, repeated]),
            Err(AgentToolEvidenceSetError::DuplicateEvidence)
        );
        assert!(
            AgentToolEvidenceSet::new(
                SnapshotId::from_bytes([2; 32]),
                vec![file; MAX_AGENT_TOOL_EVIDENCE + 1]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn tool_run_identity_is_run_and_action_bound() -> Result<(), Box<dyn Error>> {
        let run = AgentRunId::from_bytes([7; 32]);

        assert_eq!(
            ToolRunId::for_agent_action_v1(run, 1)?,
            ToolRunId::for_agent_action_v1(run, 1)?
        );
        assert_ne!(
            ToolRunId::for_agent_action_v1(run, 1)?,
            ToolRunId::for_agent_action_v1(run, 2)?
        );
        assert!(ToolRunId::for_agent_action_v1(run, 0).is_err());
        Ok(())
    }

    #[test]
    fn mutation_attempts_preserve_unknown_until_explicit_reconciliation()
    -> Result<(), Box<dyn Error>> {
        let started_at = AgentRunTimestamp::from_unix_millis(20)?;
        let tool_attempt = AgentToolAttempt::new(
            ToolRunId::from_bytes([8; 32]),
            AgentToolAttemptNumber::FIRST,
            AgentRunId::from_bytes([9; 32]),
            SnapshotId::from_bytes([10; 32]),
            AgentToolAttemptStatus::InFlight,
            started_at,
            started_at,
        )?;
        let fingerprint = MutationActionFingerprint::from_bytes([11; 32]);
        let unknown = AgentMutationDisposition::Unknown(MutationReconciliation::Required);
        let attempt = AgentMutationAttempt::new(
            tool_attempt,
            fingerprint,
            AgentMutationKind::Patch,
            unknown,
        )?;

        assert_eq!(
            attempt.disposition().application_state(),
            MutationApplicationState::Unknown
        );
        assert!(attempt.disposition().requires_reconciliation());
        assert!(matches!(
            AgentMutationAttempt::new(
                tool_attempt,
                fingerprint,
                AgentMutationKind::Patch,
                AgentMutationDisposition::NotApplied,
            ),
            Err(AgentMutationAttemptError::InFlightDisposition)
        ));

        let interrupted = AgentToolAttempt::new(
            tool_attempt.tool_run_id(),
            tool_attempt.attempt(),
            tool_attempt.run_id(),
            tool_attempt.snapshot_id(),
            AgentToolAttemptStatus::Interrupted,
            started_at,
            AgentRunTimestamp::from_unix_millis(21)?,
        )?;
        let reconciled_snapshot = SnapshotId::from_bytes([12; 32]);
        let reconciled = AgentMutationAttempt::new(
            interrupted,
            fingerprint,
            AgentMutationKind::Patch,
            AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled {
                snapshot_id: reconciled_snapshot,
            }),
        )?;
        assert_eq!(
            reconciled.disposition().application_state(),
            MutationApplicationState::Unknown
        );
        assert!(!reconciled.disposition().requires_reconciliation());
        assert!(reconciled.disposition().requires_replan());
        assert!(!reconciled.disposition().permits_future_mutation());
        assert_eq!(
            reconciled.disposition().reconciled_snapshot_id(),
            Some(reconciled_snapshot)
        );
        let replanned = AgentMutationAttempt::new(
            interrupted,
            fingerprint,
            AgentMutationKind::Patch,
            AgentMutationDisposition::Unknown(MutationReconciliation::Replanned {
                snapshot_id: reconciled_snapshot,
            }),
        )?;
        assert!(replanned.disposition().permits_future_mutation());

        let succeeded = AgentToolAttempt::new(
            tool_attempt.tool_run_id(),
            tool_attempt.attempt(),
            tool_attempt.run_id(),
            tool_attempt.snapshot_id(),
            AgentToolAttemptStatus::Succeeded,
            started_at,
            AgentRunTimestamp::from_unix_millis(22)?,
        )?;
        assert!(matches!(
            AgentMutationAttempt::new(
                succeeded,
                fingerprint,
                AgentMutationKind::Process,
                AgentMutationDisposition::NotApplied,
            ),
            Err(AgentMutationAttemptError::SucceededDisposition)
        ));
        Ok(())
    }
}
