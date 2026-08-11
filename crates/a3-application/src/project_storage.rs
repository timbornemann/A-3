use a3_domain::ProjectIdentity;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Owned future returned by the object-safe private project-storage boundary.
pub type ProjectStorageFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProjectStorageFailure>> + Send + 'a>>;

/// Cooperative cancellation and bounded progress for private storage inspection.
pub trait ProjectStorageControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning request no longer needs the operation.
    fn is_cancelled(&self) -> bool;

    /// Reports a monotone number of inspected private filesystem entries.
    fn report_entries(&self, entries: u32) -> Result<(), ProjectStorageControlError>;
}

/// The owner could no longer accept mandatory storage-inspection progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectStorageControlError;

impl fmt::Display for ProjectStorageControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project storage progress is unavailable")
    }
}

impl Error for ProjectStorageControlError {}

/// Narrow infrastructure boundary for application-owned data of one validated worktree.
pub trait ProjectStorageStore: fmt::Debug + Send + Sync {
    /// Measures private A^3 storage without reading or traversing the repository root.
    fn measure_project_storage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn ProjectStorageControl,
    ) -> ProjectStorageFuture<'a, ProjectStorageUsage>;
}

/// Lossless byte count of all validated private entries owned by one worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectStorageUsage(u64);

impl ProjectStorageUsage {
    /// Creates a lossless count reported by the bounded adapter traversal.
    #[must_use]
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Returns the exact byte count.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.0
    }
}

/// Read-only use case for private A^3 storage consumption.
#[derive(Debug)]
pub struct GetProjectStorageUsage {
    store: Arc<dyn ProjectStorageStore>,
}

impl GetProjectStorageUsage {
    /// Wires the narrow private-storage adapter.
    #[must_use]
    pub fn new(store: Arc<dyn ProjectStorageStore>) -> Self {
        Self { store }
    }

    /// Measures only the private directory selected by the validated worktree identity.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        control: &dyn ProjectStorageControl,
    ) -> Result<ProjectStorageUsage, GetProjectStorageUsageError> {
        self.store
            .measure_project_storage(project, control)
            .await
            .map_err(GetProjectStorageUsageError::Storage)
    }
}

/// Stable application classification of private project-storage failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStorageFailure {
    /// The private application-data root or one of its entries could not be read.
    Unavailable,
    /// A symlink, special file, missing worktree directory, or escaped path was rejected.
    InvalidLayout,
    /// The fixed traversal entry budget was exhausted.
    TooManyEntries,
    /// File sizes could not be represented by the lossless aggregate.
    SizeOverflow,
    /// The owning request cancelled before a complete result was available.
    Cancelled,
    /// The bounded traversal exceeded its fixed deadline.
    TimedOut,
    /// Mandatory progress could not be delivered to the owner.
    ProgressUnavailable,
}

impl fmt::Display for ProjectStorageFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("private project storage is unavailable"),
            Self::InvalidLayout => formatter.write_str("private project storage layout is invalid"),
            Self::TooManyEntries => {
                formatter.write_str("private project storage exceeds the entry limit")
            }
            Self::SizeOverflow => {
                formatter.write_str("private project storage size cannot be represented")
            }
            Self::Cancelled => formatter.write_str("project storage inspection was cancelled"),
            Self::TimedOut => formatter.write_str("project storage inspection timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("project storage inspection progress is unavailable")
            }
        }
    }
}

impl Error for ProjectStorageFailure {}

/// Failure while executing the private-storage usage query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetProjectStorageUsageError {
    /// The private storage adapter could not produce a complete bounded result.
    Storage(ProjectStorageFailure),
}

impl fmt::Display for GetProjectStorageUsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project storage usage failed: {error}"),
        }
    }
}

impl Error for GetProjectStorageUsageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
        }
    }
}
