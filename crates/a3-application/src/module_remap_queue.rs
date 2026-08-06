use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{IndexRunId, Progress, ProjectIdentity, RemapRequest, SnapshotId};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_PENDING_REMAPS: u16 = 256;

/// Positive bounded page size for the durable remap queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemapQueueLimit(u16);

impl RemapQueueLimit {
    /// Default interactive queue page.
    pub const DEFAULT: Self = Self(64);

    /// Creates a limit between one and 256 entries.
    pub fn new(value: u16) -> Result<Self, RemapQueueLimitError> {
        if value == 0 || value > MAX_PENDING_REMAPS {
            return Err(RemapQueueLimitError(value));
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Remap queue limit was zero or exceeded the fixed page bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemapQueueLimitError(u16);

impl fmt::Display for RemapQueueLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "remap queue limit {} is outside 1..=256", self.0)
    }
}

impl Error for RemapQueueLimitError {}

/// Bounded direct-before-dependent queue snapshot for one current index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRemapQueue {
    target_index_run_id: IndexRunId,
    target_snapshot_id: SnapshotId,
    entries: Vec<RemapRequest>,
    truncated: bool,
}

impl PendingRemapQueue {
    /// Validates target consistency, stable ordering, uniqueness, and cardinality.
    pub fn new(
        target_index_run_id: IndexRunId,
        target_snapshot_id: SnapshotId,
        entries: Vec<RemapRequest>,
        truncated: bool,
    ) -> Result<Self, PendingRemapQueueError> {
        if entries.len() > usize::from(MAX_PENDING_REMAPS) {
            return Err(PendingRemapQueueError::TooManyEntries);
        }
        if entries.iter().any(|entry| {
            entry.target_index_run_id() != target_index_run_id
                || entry.target_snapshot_id() != target_snapshot_id
        }) {
            return Err(PendingRemapQueueError::TargetMismatch);
        }
        let mut modules = BTreeSet::new();
        if entries
            .iter()
            .any(|entry| !modules.insert(entry.module_id()))
        {
            return Err(PendingRemapQueueError::UnstableOrder);
        }
        if entries.windows(2).any(|pair| {
            (pair[0].priority(), pair[0].module_id()) >= (pair[1].priority(), pair[1].module_id())
        }) {
            return Err(PendingRemapQueueError::UnstableOrder);
        }
        Ok(Self {
            target_index_run_id,
            target_snapshot_id,
            entries,
            truncated,
        })
    }

    /// Returns the current index run every request must remap against.
    #[must_use]
    pub const fn target_index_run_id(&self) -> IndexRunId {
        self.target_index_run_id
    }

    /// Returns the current immutable snapshot every request must remap against.
    #[must_use]
    pub const fn target_snapshot_id(&self) -> SnapshotId {
        self.target_snapshot_id
    }

    /// Returns requests in direct-before-dependent, stable module order.
    #[must_use]
    pub fn entries(&self) -> &[RemapRequest] {
        &self.entries
    }

    /// Returns whether additional requests were omitted by the requested page limit.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Persisted queue rows violated the application boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRemapQueueError {
    /// More than 256 entries crossed the boundary.
    TooManyEntries,
    /// An entry targeted a different run or snapshot.
    TargetMismatch,
    /// Entries were duplicated or not stably ordered.
    UnstableOrder,
}

impl fmt::Display for PendingRemapQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooManyEntries => "pending remap queue exceeds 256 entries",
            Self::TargetMismatch => "pending remap queue mixes target publications",
            Self::UnstableOrder => "pending remap queue ordering is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for PendingRemapQueueError {}

/// Cooperative cancellation and two-phase progress for one bounded queue read.
pub trait RemapQueueControl: fmt::Debug + Send + Sync {
    /// Returns whether the owner cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports start and completed phases.
    fn report_progress(&self, progress: Progress) -> Result<(), RemapQueueControlError>;
}

impl RemapQueueControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RemapQueueControlError> {
        JobContext::report_progress(self, progress).map_err(|_| RemapQueueControlError::Unavailable)
    }
}

/// Queue progress could not be delivered to the owning job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemapQueueControlError {
    /// The scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for RemapQueueControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("remap queue progress is unavailable")
    }
}

impl Error for RemapQueueControlError {}

/// Owned future returned by the object-safe remap queue port.
pub type ModuleRemapQueueFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PendingRemapQueue, ModuleRemapQueueFailure>> + Send + 'a>>;

/// Read-only durable queue capability; mapping execution remains a separate use case.
pub trait ModuleRemapQueueStore: fmt::Debug + Send + Sync {
    /// Loads one bounded page against the latest atomically published index.
    fn load_pending<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        limit: RemapQueueLimit,
        control: &'a dyn RemapQueueControl,
    ) -> ModuleRemapQueueFuture<'a>;
}

/// Stable queue-read failure classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRemapQueueFailure {
    /// Shared local storage failed.
    Storage(KnowledgeStoreFailure),
    /// No current published index exists.
    IndexUnavailable,
    /// Persisted lifecycle or queue rows were contradictory.
    InvalidStoredProjection,
    /// The owner cancelled before a result was delivered.
    Cancelled,
    /// The bounded adapter read exceeded its deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleRemapQueueFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "remap queue storage failed: {error}"),
            Self::IndexUnavailable => formatter.write_str("published index is unavailable"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored remap queue projection is invalid")
            }
            Self::Cancelled => formatter.write_str("remap queue read was cancelled"),
            Self::TimedOut => formatter.write_str("remap queue read timed out"),
            Self::ProgressUnavailable => formatter.write_str("remap queue progress is unavailable"),
        }
    }
}

impl Error for ModuleRemapQueueFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::IndexUnavailable
            | Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

/// Small application orchestrator retaining progress and cancellation outside adapters.
#[derive(Debug)]
pub struct LoadPendingModuleRemaps<'a> {
    store: &'a dyn ModuleRemapQueueStore,
}

impl<'a> LoadPendingModuleRemaps<'a> {
    /// Creates the read-only queue use case.
    #[must_use]
    pub const fn new(store: &'a dyn ModuleRemapQueueStore) -> Self {
        Self { store }
    }

    /// Loads a current bounded queue page.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        limit: RemapQueueLimit,
        control: &dyn RemapQueueControl,
    ) -> Result<PendingRemapQueue, ModuleRemapQueueFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleRemapQueueFailure::Cancelled);
        }
        let queue = self.store.load_pending(project, limit, control).await?;
        if control.is_cancelled() {
            return Err(ModuleRemapQueueFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(queue)
    }
}

fn report(control: &dyn RemapQueueControl, completed: u64) -> Result<(), ModuleRemapQueueFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleRemapQueueFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleRemapQueueFailure::ProgressUnavailable)
}
