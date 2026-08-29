use super::ExploreBudget;
use std::error::Error;
use std::fmt;

/// Closed product modes with fixed, Core-owned Deep-Map resource limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeepMapMode {
    /// Short mapping pass for small or already well-covered repositories.
    Fast,
    /// Balanced default mapping pass.
    Standard,
    /// Largest bounded mapping pass for complex repositories.
    Thorough,
}

impl DeepMapMode {
    /// Returns the immutable budget associated with this mode.
    #[must_use]
    pub const fn budget(self) -> ExploreBudget {
        match self {
            Self::Fast => ExploreBudget::FAST,
            Self::Standard => ExploreBudget::DEFAULT,
            Self::Thorough => ExploreBudget::THOROUGH,
        }
    }
}

/// Closed durable lifecycle of one Deep-Map run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeepMapRunState {
    /// Accepted by the Core but not yet executing.
    Queued,
    /// Owned worker is executing.
    Running,
    /// Cooperative pause has been requested.
    Pausing,
    /// Exact resumable checkpoint is retained.
    Paused,
    /// Cooperative cancellation has been requested.
    Cancelling,
    /// Verified cards are current for the run's index.
    Succeeded,
    /// Execution ended with a safe classified failure.
    Failed,
    /// Execution ended through deliberate cancellation.
    Cancelled,
    /// A non-terminal process-owned run was reconciled after restart.
    Interrupted,
}

impl DeepMapRunState {
    /// Returns whether no further worker transition is expected.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

/// Stable safe failure categories retained without raw boundary errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeepMapDiagnosticCode {
    /// No complete Fast-Index publication exists.
    NoPublishedIndex,
    /// The planned publication is no longer the newest index.
    StaleIndex,
    /// Deterministic planning failed.
    Planning,
    /// Provider could not complete the bounded request.
    ModelUnavailable,
    /// Provider rejected the bounded request.
    ModelRejected,
    /// Provider response exceeded the deadline.
    ModelTimeout,
    /// Structured provider output was invalid after the one allowed repair.
    InvalidModelResponse,
    /// A bounded read-only exploration operation failed.
    Read,
    /// Evidence or claims could not be verified.
    Verification,
    /// The publication batch was stale or otherwise rejected.
    PublicationRejected,
    /// Local storage could not complete publication.
    PublicationStorage,
    /// Publication exceeded its deadline.
    PublicationTimeout,
    /// Publication progress could not reach its owner.
    PublicationProgress,
    /// Resume data contradicted its immutable plan.
    InvalidCheckpoint,
    /// Scheduler progress could not reach its owner.
    ProgressUnavailable,
    /// Process termination interrupted a non-terminal run.
    Interrupted,
}

/// Non-negative Unix timestamp retained in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeepMapRunTimestamp(i64);

impl DeepMapRunTimestamp {
    /// Validates an adapter-produced Unix timestamp.
    pub const fn new(value: i64) -> Result<Self, DeepMapRunTimestampError> {
        if value < 0 {
            Err(DeepMapRunTimestampError)
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
pub struct DeepMapRunTimestampError;

impl fmt::Display for DeepMapRunTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Deep-Map run timestamp must be non-negative")
    }
}

impl Error for DeepMapRunTimestampError {}

/// Strictly positive sequence in one append-only Deep-Map journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeepMapEventSequence(u64);

impl DeepMapEventSequence {
    /// Validates a persisted or newly assigned sequence.
    pub const fn new(value: u64) -> Result<Self, DeepMapEventSequenceError> {
        if value == 0 {
            Err(DeepMapEventSequenceError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the one-based sequence.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Deep-Map journal sequences must start at one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapEventSequenceError;

impl fmt::Display for DeepMapEventSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Deep-Map event sequence must be positive")
    }
}

impl Error for DeepMapEventSequenceError {}

#[cfg(test)]
mod tests {
    use super::{DeepMapEventSequence, DeepMapMode, DeepMapRunState, DeepMapRunTimestamp};

    #[test]
    fn modes_own_the_three_fixed_budgets() {
        assert_eq!(DeepMapMode::Fast.budget().tokens(), 8_000);
        assert_eq!(DeepMapMode::Standard.budget().tokens(), 32_000);
        assert_eq!(DeepMapMode::Thorough.budget().tokens(), 128_000);
        assert_eq!(DeepMapMode::Thorough.budget().tool_calls(), 256);
    }

    #[test]
    fn persisted_sequences_and_timestamps_reject_invalid_values() {
        assert!(DeepMapEventSequence::new(0).is_err());
        assert!(DeepMapEventSequence::new(1).is_ok());
        assert!(DeepMapRunTimestamp::new(-1).is_err());
        assert!(DeepMapRunTimestamp::new(0).is_ok());
        assert!(DeepMapRunState::Interrupted.is_terminal());
        assert!(!DeepMapRunState::Running.is_terminal());
    }
}
