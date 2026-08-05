use super::{IndexRunId, SnapshotId};
use std::error::Error;
use std::fmt;

const MAX_PERSISTED_SEQUENCE: u64 = i64::MAX as u64;

/// Version of the deterministic ranking policy applied by an index run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RankingPolicyVersion(u32);

impl RankingPolicyVersion {
    /// Creates a non-zero ranking policy version.
    pub fn new(value: u32) -> Result<Self, RankingPolicyVersionError> {
        if value == 0 {
            return Err(RankingPolicyVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the initial deterministic graph-ranking policy.
    #[must_use]
    pub const fn v1() -> Self {
        Self(1)
    }

    /// Returns the durable integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Ranking policy version zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankingPolicyVersionError;

impl fmt::Display for RankingPolicyVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ranking policy version must be positive")
    }
}

impl Error for RankingPolicyVersionError {}

/// Request to begin indexing one immutable snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRunStart {
    id: IndexRunId,
    snapshot_id: SnapshotId,
    ranking_policy_version: RankingPolicyVersion,
}

impl IndexRunStart {
    /// Creates a deterministic index-run request.
    #[must_use]
    pub const fn new(
        id: IndexRunId,
        snapshot_id: SnapshotId,
        ranking_policy_version: RankingPolicyVersion,
    ) -> Self {
        Self {
            id,
            snapshot_id,
            ranking_policy_version,
        }
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn id(self) -> IndexRunId {
        self.id
    }

    /// Returns the immutable input snapshot.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the deterministic ranking policy version.
    #[must_use]
    pub const fn ranking_policy_version(self) -> RankingPolicyVersion {
        self.ranking_policy_version
    }
}

/// Monotone worktree-local order assigned durably to index attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexRunSequence(u64);

impl IndexRunSequence {
    /// Creates a positive sequence representable by durable storage.
    pub fn new(value: u64) -> Result<Self, IndexRunSequenceError> {
        if value == 0 {
            return Err(IndexRunSequenceError::Zero);
        }
        if value > MAX_PERSISTED_SEQUENCE {
            return Err(IndexRunSequenceError::TooLarge(value));
        }
        Ok(Self(value))
    }

    /// Returns the durable integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Invalid worktree-local index-run sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexRunSequenceError {
    /// Sequence zero is not a persisted run.
    Zero,
    /// The value cannot be represented durably.
    TooLarge(u64),
}

impl fmt::Display for IndexRunSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("index run sequence must be positive"),
            Self::TooLarge(value) => write!(formatter, "index run sequence {value} is too large"),
        }
    }
}

impl Error for IndexRunSequenceError {}

/// Durable lifecycle state of one index attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexRunStatus {
    /// The run owns the worktree's current index mutation slot.
    Building,
    /// All index data was atomically committed and made visible.
    Published,
    /// The run terminated with a failure and published nothing.
    Failed,
    /// The run was cancelled and published nothing.
    Cancelled,
}

impl IndexRunStatus {
    /// Returns the stable persisted identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Reconstructs a status from durable storage.
    pub fn try_from_stored(value: &str) -> Result<Self, IndexRunStatusError> {
        match value {
            "building" => Ok(Self::Building),
            "published" => Ok(Self::Published),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(IndexRunStatusError),
        }
    }
}

/// Durable storage contained an unknown index-run lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRunStatusError;

impl fmt::Display for IndexRunStatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("stored index run status is invalid")
    }
}

impl Error for IndexRunStatusError {}

/// Non-publishing terminal outcome available before atomic index publication exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexRunTerminalOutcome {
    /// Indexing failed and no new index became visible.
    Failed,
    /// Indexing was cancelled and no new index became visible.
    Cancelled,
}

impl IndexRunTerminalOutcome {
    /// Returns the durable terminal status.
    #[must_use]
    pub const fn status(self) -> IndexRunStatus {
        match self {
            Self::Failed => IndexRunStatus::Failed,
            Self::Cancelled => IndexRunStatus::Cancelled,
        }
    }
}

/// Storage-independent projection of one durable index attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRunRecord {
    id: IndexRunId,
    snapshot_id: SnapshotId,
    ranking_policy_version: RankingPolicyVersion,
    sequence: IndexRunSequence,
    status: IndexRunStatus,
}

impl IndexRunRecord {
    /// Creates a record after an adapter has validated durable fields.
    #[must_use]
    pub const fn new(
        id: IndexRunId,
        snapshot_id: SnapshotId,
        ranking_policy_version: RankingPolicyVersion,
        sequence: IndexRunSequence,
        status: IndexRunStatus,
    ) -> Self {
        Self {
            id,
            snapshot_id,
            ranking_policy_version,
            sequence,
            status,
        }
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn id(self) -> IndexRunId {
        self.id
    }

    /// Returns the immutable input snapshot.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the ranking policy version used by this attempt.
    #[must_use]
    pub const fn ranking_policy_version(self) -> RankingPolicyVersion {
        self.ranking_policy_version
    }

    /// Returns the worktree-local durable order.
    #[must_use]
    pub const fn sequence(self) -> IndexRunSequence {
        self.sequence
    }

    /// Returns the current durable lifecycle state.
    #[must_use]
    pub const fn status(self) -> IndexRunStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IndexRunId, IndexRunSequence, IndexRunStart, IndexRunStatus, IndexRunTerminalOutcome,
        RankingPolicyVersion, SnapshotId,
    };

    #[test]
    fn run_start_and_terminal_outcomes_remain_typed() -> Result<(), Box<dyn std::error::Error>> {
        let start = IndexRunStart::new(
            IndexRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            RankingPolicyVersion::new(1)?,
        );
        assert_eq!(start.snapshot_id(), SnapshotId::from_bytes([2; 32]));
        assert_eq!(IndexRunSequence::new(1)?.get(), 1);
        assert_eq!(
            IndexRunTerminalOutcome::Failed.status(),
            IndexRunStatus::Failed
        );
        assert_eq!(
            IndexRunTerminalOutcome::Cancelled.status(),
            IndexRunStatus::Cancelled
        );
        Ok(())
    }
}
