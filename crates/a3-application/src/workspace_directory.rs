use crate::{JobContext, ProgressReportError};
use a3_domain::{
    Progress, ProjectIdentity, PublishedIndex, WorkspaceDirectoryListRequest,
    WorkspaceDirectoryListing,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Future returned by the object-safe secure directory-listing port.
pub type WorkspaceDirectoryListerFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WorkspaceDirectoryListing, WorkspaceDirectoryReadFailure>>
            + Send
            + 'a,
    >,
>;

/// Cooperative cancellation and bounded progress for one indexed directory read.
pub trait WorkspaceDirectoryReadControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports bounded progress while filtering the published file set.
    fn report_progress(&self, progress: Progress) -> Result<(), WorkspaceDirectoryProgressError>;
}

impl WorkspaceDirectoryReadControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspaceDirectoryProgressError> {
        JobContext::report_progress(self, progress).map_err(Into::into)
    }
}

/// The owning scheduler no longer accepts progress for a directory read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceDirectoryProgressError;

impl fmt::Display for WorkspaceDirectoryProgressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace directory progress is unavailable")
    }
}

impl Error for WorkspaceDirectoryProgressError {}

impl From<ProgressReportError> for WorkspaceDirectoryProgressError {
    fn from(_value: ProgressReportError) -> Self {
        Self
    }
}

/// Read-only capability for a bounded page projected from one published index snapshot.
pub trait WorkspaceDirectoryLister: fmt::Debug + Send + Sync {
    /// Validates the live directory boundary, then lists only current indexed direct children.
    fn list<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        request: &'a WorkspaceDirectoryListRequest,
        control: &'a dyn WorkspaceDirectoryReadControl,
    ) -> WorkspaceDirectoryListerFuture<'a>;
}

/// Stable directory-read failure without paths, source content, or OS details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDirectoryReadFailure {
    /// Worktree, snapshot, cursor, or canonical root policy denied the request.
    Denied,
    /// The requested directory is unavailable or no longer a regular directory.
    Unavailable,
    /// The owning job cancelled the read.
    Cancelled,
    /// Progress could not be delivered to the owning job.
    ProgressUnavailable,
    /// The adapter could not construct a canonical bounded listing.
    InvalidResult,
}

impl fmt::Display for WorkspaceDirectoryReadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "workspace directory read was denied",
            Self::Unavailable => "workspace directory is unavailable",
            Self::Cancelled => "workspace directory read was cancelled",
            Self::ProgressUnavailable => "workspace directory progress is unavailable",
            Self::InvalidResult => "workspace directory listing is invalid",
        })
    }
}

impl Error for WorkspaceDirectoryReadFailure {}
