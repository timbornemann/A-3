use a3_application::JobContext;
use a3_domain::Progress;
use std::error::Error;
use std::fmt;

/// Cooperative cancellation and bounded progress boundary shared by link and rank.
pub trait GraphComputationControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports monotone deterministic work progress.
    fn report_progress(&self, progress: Progress) -> Result<(), GraphComputationControlError>;
}

impl GraphComputationControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), GraphComputationControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| GraphComputationControlError::Unavailable)
    }
}

/// Stable progress-delivery failure at the index feature boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphComputationControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for GraphComputationControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("graph computation progress is unavailable")
    }
}

impl Error for GraphComputationControlError {}
