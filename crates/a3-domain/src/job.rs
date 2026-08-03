/// Stable identifier assigned to one scheduled job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JobId(u64);

impl JobId {
    /// Creates a job identifier from an application-owned value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the primitive value at serialization boundaries.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Stable identifier for the lifecycle owner responsible for a job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JobOwner(u64);

impl JobOwner {
    /// Creates an owner identifier from an application-owned value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the primitive value at serialization boundaries.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Lifecycle state of a bounded background job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JobStatus {
    /// Accepted and waiting for a worker slot.
    Queued,
    /// Currently executing on an owned scheduler worker.
    Running,
    /// Cancellation was requested and cooperative termination is pending.
    Cancelling,
    /// Finished successfully.
    Succeeded,
    /// Finished with a controlled task failure.
    Failed,
    /// Finished after cancellation.
    Cancelled,
}

impl JobStatus {
    /// Returns whether this state is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    /// Returns whether the lifecycle permits the requested next state.
    #[must_use]
    pub const fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running | Self::Cancelling)
                | (
                    Self::Running,
                    Self::Cancelling | Self::Succeeded | Self::Failed
                )
                | (Self::Cancelling, Self::Cancelled)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::JobStatus;

    #[test]
    fn terminal_states_cannot_transition() {
        for status in [
            JobStatus::Succeeded,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            assert!(status.is_terminal());
            assert!(!status.allows(JobStatus::Running));
        }
    }

    #[test]
    fn active_states_only_allow_documented_transitions() {
        assert!(JobStatus::Queued.allows(JobStatus::Running));
        assert!(JobStatus::Queued.allows(JobStatus::Cancelling));
        assert!(JobStatus::Running.allows(JobStatus::Succeeded));
        assert!(JobStatus::Running.allows(JobStatus::Failed));
        assert!(JobStatus::Running.allows(JobStatus::Cancelling));
        assert!(JobStatus::Cancelling.allows(JobStatus::Cancelled));
        assert!(!JobStatus::Queued.allows(JobStatus::Succeeded));
        assert!(!JobStatus::Cancelling.allows(JobStatus::Succeeded));
    }
}
