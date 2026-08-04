use crate::JobContext;
use a3_domain::{DiscoveryPolicy, DiscoveryResult, Progress, ProjectIdentity};
use std::error::Error;
use std::fmt;

/// Cooperative cancellation and progress boundary for one discovery run.
pub trait RepositoryDiscoveryControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cooperative cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports an indeterminate enumeration or determinate classification observation.
    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryDiscoveryControlError>;
}

impl RepositoryDiscoveryControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryDiscoveryControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| RepositoryDiscoveryControlError::Unavailable)
    }
}

/// Stable application classification of progress-delivery failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryDiscoveryControlError {
    /// The scheduler no longer accepts progress for the owning job.
    Unavailable,
}

impl fmt::Display for RepositoryDiscoveryControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository discovery progress is unavailable")
    }
}

impl Error for RepositoryDiscoveryControlError {}

/// Outbound port for bounded deterministic discovery inside one inspected worktree.
pub trait RepositoryDiscoverer: fmt::Debug + Send + Sync {
    /// Discovers relevant files without hashing, parsing, persistence, or publication.
    fn discover(
        &self,
        project: &ProjectIdentity,
        policy: DiscoveryPolicy,
        control: &dyn RepositoryDiscoveryControl,
    ) -> Result<DiscoveryResult, RepositoryDiscoveryFailure>;
}

/// Stable application classification of repository discovery failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryDiscoveryFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// The previously inspected worktree root disappeared or changed identity.
    RootUnavailable,
    /// Git metadata is missing, inconsistent, or unsupported.
    InvalidRepository,
    /// `.a3/project.toml` is malformed or exceeds its strict schema and limits.
    InvalidConfiguration,
    /// A Git path cannot be represented safely on the current platform.
    InvalidPath,
    /// A fixed candidate, configuration, or path resource limit was exceeded.
    ResourceLimitExceeded,
    /// A file changed or became unreadable during the bounded discovery observation.
    Filesystem,
    /// The owning scheduler rejected progress reporting.
    ProgressUnavailable,
    /// The adapter assembled a result that violates a domain invariant.
    InvalidResult,
}

impl fmt::Display for RepositoryDiscoveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("repository discovery was cancelled"),
            Self::RootUnavailable => {
                formatter.write_str("repository discovery root is unavailable")
            }
            Self::InvalidRepository => formatter.write_str("repository metadata is invalid"),
            Self::InvalidConfiguration => {
                formatter.write_str("repository discovery configuration is invalid")
            }
            Self::InvalidPath => formatter.write_str("repository path is invalid on this platform"),
            Self::ResourceLimitExceeded => {
                formatter.write_str("repository discovery resource limit was exceeded")
            }
            Self::Filesystem => {
                formatter.write_str("repository discovery filesystem access failed")
            }
            Self::ProgressUnavailable => {
                formatter.write_str("repository discovery progress could not be reported")
            }
            Self::InvalidResult => formatter.write_str("repository discovery result is invalid"),
        }
    }
}

impl Error for RepositoryDiscoveryFailure {}

#[cfg(test)]
mod tests {
    use super::{RepositoryDiscoveryControl, RepositoryDiscoveryControlError};
    use a3_domain::Progress;

    #[derive(Debug)]
    struct TestControl;

    impl RepositoryDiscoveryControl for TestControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(
            &self,
            _progress: Progress,
        ) -> Result<(), RepositoryDiscoveryControlError> {
            Ok(())
        }
    }

    #[test]
    fn control_contract_accepts_indeterminate_progress() {
        let control = TestControl;
        assert!(!control.is_cancelled());
        assert_eq!(control.report_progress(Progress::Indeterminate), Ok(()));
    }
}
