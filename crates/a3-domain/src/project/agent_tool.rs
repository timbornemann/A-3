use super::{
    AgentRunId, AgentRunTimestamp, EvidenceRef, FileRevision, SnapshotId, SourceRange,
    TaskEvidenceId, ToolRunId,
};
use std::error::Error;
use std::fmt;

const MAX_AGENT_TOOL_EVIDENCE: usize = 100;

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
}
