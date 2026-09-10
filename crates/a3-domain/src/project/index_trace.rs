use std::error::Error;
use std::fmt;

/// Closed lifecycle of one durable Fast-Index diagnostic trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceState {
    /// The scheduler accepted the run but its worker has not started.
    Queued,
    /// The owned worker is executing one of the six phases.
    Running,
    /// Cooperative cancellation has been requested.
    Cancelling,
    /// A complete index was published, or the existing publication was already current.
    Succeeded,
    /// The run ended without replacing the previous publication.
    Failed,
    /// The run stopped after cooperative cancellation.
    Cancelled,
    /// Process termination left the run non-terminal and startup reconciliation closed it.
    Interrupted,
}

impl IndexTraceState {
    /// Returns whether no later worker transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Validates the monotone lifecycle transitions accepted by the trace journal.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        if self.is_terminal() {
            return self as u8 == next as u8;
        }
        matches!(
            (self, next),
            (
                Self::Queued,
                Self::Queued | Self::Running | Self::Cancelling
            ) | (
                Self::Queued,
                Self::Failed | Self::Cancelled | Self::Interrupted
            ) | (Self::Running, Self::Running | Self::Cancelling)
                | (
                    Self::Running,
                    Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
                )
                | (
                    Self::Cancelling,
                    Self::Cancelling | Self::Cancelled | Self::Failed | Self::Interrupted
                )
        )
    }
}

/// Core-owned cause of a Fast-Index run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceTrigger {
    /// Initial observation after activating a project.
    InitialObservation,
    /// A bounded watcher batch reported repository changes.
    FileChanges,
    /// Watcher uncertainty required a complete rescan.
    RecoveryRescan,
    /// The user explicitly requested a full retry without deleting the published index.
    ManualRetry,
}

/// Fixed ADR-0006 Fast-Index phases in deterministic execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexTracePhase {
    /// Discover the bounded repository candidate set.
    Discover,
    /// Hash exact file contents.
    Hash,
    /// Parse supported source and manifest files.
    Parse,
    /// Link structural graph relationships.
    Link,
    /// Rank symbols and form deterministic projections.
    Rank,
    /// Atomically publish the completed read model.
    Publish,
}

impl IndexTracePhase {
    /// All phases in their only legal order.
    pub const ALL: [Self; 6] = [
        Self::Discover,
        Self::Hash,
        Self::Parse,
        Self::Link,
        Self::Rank,
        Self::Publish,
    ];

    /// Returns the zero-based phase position.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Discover => 0,
            Self::Hash => 1,
            Self::Parse => 2,
            Self::Link => 3,
            Self::Rank => 4,
            Self::Publish => 5,
        }
    }
}

/// Per-phase lifecycle retained in the diagnostic journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTracePhaseState {
    /// The phase has not started.
    Pending,
    /// The phase currently owns execution.
    Running,
    /// The next phase was entered or the run succeeded.
    Succeeded,
    /// The run failed in this phase.
    Failed,
    /// The run was cancelled or interrupted in this phase.
    Cancelled,
}

/// Repository change classification shown for every trace file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceFileChange {
    /// Discovery accepted the file but comparison has not completed yet.
    Pending,
    /// The file was absent from the previous snapshot.
    New,
    /// The file revision changed.
    Changed,
    /// The previous file revision remained current.
    Unchanged,
    /// The previous snapshot contained the file but discovery no longer did.
    Deleted,
}

/// Hash work performed for one trace file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceHashOutcome {
    /// Discovery accepted the file but hashing has not completed yet.
    Pending,
    /// Exact source bytes were hashed in this run.
    Hashed,
    /// The previous immutable revision was reused.
    Reused,
    /// Deleted files have no current bytes to hash.
    NotApplicable,
}

/// Structural analysis work performed for one trace file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceParseOutcome {
    /// A supported language adapter analyzed the file in this run.
    Structural,
    /// A prior supported-language parse artifact was reused.
    Reused,
    /// No structural adapter supports the file and generic indexing was used.
    Generic,
    /// Structural analysis failed safely for this file.
    Failed,
    /// Deleted files are not analyzed.
    NotApplicable,
}

/// Stable safe diagnostics retained without raw adapter errors or source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTraceDiagnosticCode {
    /// Repository discovery or configuration failed.
    Discovery,
    /// Source bytes could not be read safely.
    SourceUnavailable,
    /// The worktree changed during a coherent observation.
    RevisionChanged,
    /// Structural parsing failed or produced invalid bounded output.
    Parse,
    /// Graph linking failed.
    Link,
    /// Ranking or module formation failed.
    Rank,
    /// Atomic publication failed while the prior index remained visible.
    Publish,
    /// A fixed resource limit was exceeded.
    ResourceLimit,
    /// A bounded operation exceeded its deadline.
    Timeout,
    /// Scheduler progress could not reach the owner.
    ProgressUnavailable,
    /// Durable diagnostics could not be fully recorded; indexing continued.
    JournalIncomplete,
    /// The owned worker ended unexpectedly without a typed failure.
    WorkerUnavailable,
    /// Startup reconciliation found a process-interrupted run.
    Interrupted,
}

/// Non-negative Unix timestamp retained in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexTraceTimestamp(i64);

impl IndexTraceTimestamp {
    /// Unix epoch used as the safe fallback when a platform clock precedes it.
    pub const UNIX_EPOCH: Self = Self(0);

    /// Validates an adapter-produced Unix timestamp.
    pub const fn new(value: i64) -> Result<Self, IndexTraceTimestampError> {
        if value < 0 {
            Err(IndexTraceTimestampError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns milliseconds since the Unix epoch.
    #[must_use]
    pub const fn unix_millis(self) -> i64 {
        self.0
    }
}

/// Adapter returned a timestamp before the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexTraceTimestampError;

impl fmt::Display for IndexTraceTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Fast-Index trace timestamp must be non-negative")
    }
}

impl Error for IndexTraceTimestampError {}

/// Positive optimistic-concurrency revision of a trace projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexTraceRevision(u64);

impl IndexTraceRevision {
    /// First revision assigned when a trace is queued.
    pub const FIRST: Self = Self(1);

    /// Validates a persisted or newly assigned revision.
    pub const fn new(value: u64) -> Result<Self, IndexTraceRevisionError> {
        if value == 0 || value > i64::MAX as u64 {
            Err(IndexTraceRevisionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the positive revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next representable revision.
    pub const fn next(self) -> Result<Self, IndexTraceRevisionError> {
        match self.0.checked_add(1) {
            Some(value) => Self::new(value),
            None => Err(IndexTraceRevisionError),
        }
    }
}

/// Trace revisions must be positive and durably representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexTraceRevisionError;

impl fmt::Display for IndexTraceRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Fast-Index trace revision is invalid")
    }
}

impl Error for IndexTraceRevisionError {}

#[cfg(test)]
mod tests {
    use super::{IndexTracePhase, IndexTraceRevision, IndexTraceState, IndexTraceTimestamp};

    #[test]
    fn lifecycle_is_monotone_and_terminal_states_cannot_reopen() {
        assert!(IndexTraceState::Queued.can_transition_to(IndexTraceState::Running));
        assert!(IndexTraceState::Running.can_transition_to(IndexTraceState::Cancelling));
        assert!(IndexTraceState::Cancelling.can_transition_to(IndexTraceState::Cancelled));
        assert!(!IndexTraceState::Succeeded.can_transition_to(IndexTraceState::Running));
        assert!(IndexTraceState::Interrupted.is_terminal());
    }

    #[test]
    fn phase_order_and_persisted_numbers_are_fixed() {
        assert_eq!(
            IndexTracePhase::ALL.map(IndexTracePhase::ordinal),
            [0, 1, 2, 3, 4, 5]
        );
        assert!(IndexTraceRevision::new(0).is_err());
        assert_eq!(
            IndexTraceRevision::new(1).and_then(IndexTraceRevision::next),
            IndexTraceRevision::new(2)
        );
        assert!(IndexTraceTimestamp::new(-1).is_err());
    }
}
