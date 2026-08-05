use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{
    IndexPublication, IndexRunId, IndexRunRecord, IndexRunStart, IndexRunTerminalOutcome, Progress,
    ProjectIdentity, PublishedIndex, RepositoryFileState, Snapshot,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe deterministic-index persistence port.
pub type KnowledgeIndexFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KnowledgeIndexFailure>> + Send + 'a>>;

/// Cooperative cancellation and bounded progress boundary for durable index mutation.
pub trait IndexPersistenceControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports monotone deterministic persistence progress.
    fn report_progress(&self, progress: Progress) -> Result<(), IndexPersistenceControlError>;
}

impl IndexPersistenceControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), IndexPersistenceControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| IndexPersistenceControlError::Unavailable)
    }
}

/// Stable progress-delivery failure at the durable index boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPersistenceControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for IndexPersistenceControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("index persistence progress is unavailable")
    }
}

impl Error for IndexPersistenceControlError {}

/// Snapshot and index-run capability of the local KnowledgeStore boundary.
///
/// The separate capability keeps project-open consumers narrow while ensuring
/// SQL, database handles, rows, and engine-specific errors remain in adapters.
pub trait KnowledgeIndexStore: fmt::Debug + Send + Sync {
    /// Appends exactly the next immutable snapshot for the observed worktree.
    fn append_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot: &'a Snapshot,
    ) -> KnowledgeIndexFuture<'a, ()>;

    /// Returns the latest complete snapshot for this worktree, if one exists.
    fn latest_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<Snapshot>>;

    /// Reconstructs the effective relevant file revisions at the latest snapshot.
    fn current_file_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, RepositoryFileState>;

    /// Starts one index attempt for an existing immutable snapshot.
    fn start_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: IndexRunStart,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord>;

    /// Records a failed or cancelled terminal outcome without publishing index data.
    fn finish_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord>;

    /// Atomically commits one complete deterministic index and publishes its building run.
    fn publish_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        publication: &'a IndexPublication,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord>;

    /// Returns the latest attempted run in durable worktree-local order.
    fn latest_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>>;

    /// Returns the last atomically published index-run record, if one exists.
    fn latest_published_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>>;

    /// Returns the latest complete index through the same published-run visibility boundary.
    fn latest_published_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>>;

    /// Removes only regenerable deterministic index state for this worktree.
    fn rebuild_regenerable_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, ()>;
}

/// Stable application classification of snapshot and index-run persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeIndexFailure {
    /// The shared local storage boundary failed.
    Storage(KnowledgeStoreFailure),
    /// The snapshot was not the exact next generation and parent for the worktree.
    SnapshotConflict,
    /// The requested snapshot does not exist for the observed worktree.
    SnapshotNotFound,
    /// A building index run already owns the worktree mutation slot.
    IndexRunAlreadyActive,
    /// The requested index run does not exist for the observed worktree.
    IndexRunNotFound,
    /// The requested run transition is not legal from its durable state.
    InvalidIndexRunTransition,
    /// Graph, ranking, snapshot, policy, or durable file state did not match the active run.
    IndexPublicationMismatch,
    /// The complete publication exceeded a fixed deterministic storage boundary.
    IndexPublicationTooLarge,
    /// The owning job cancelled before the mutation committed.
    Cancelled,
    /// The bounded mutation exceeded its fixed wall-clock deadline.
    TimedOut,
    /// Progress could not be delivered to the owning job.
    ProgressUnavailable,
}

impl fmt::Display for KnowledgeIndexFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "knowledge index storage failed: {error}"),
            Self::SnapshotConflict => {
                formatter.write_str("snapshot conflicts with the durable worktree generation")
            }
            Self::SnapshotNotFound => formatter.write_str("snapshot was not found"),
            Self::IndexRunAlreadyActive => {
                formatter.write_str("an index run is already active for this worktree")
            }
            Self::IndexRunNotFound => formatter.write_str("index run was not found"),
            Self::InvalidIndexRunTransition => {
                formatter.write_str("index run transition is invalid")
            }
            Self::IndexPublicationMismatch => {
                formatter.write_str("index publication does not match the active run")
            }
            Self::IndexPublicationTooLarge => {
                formatter.write_str("index publication exceeds a fixed storage limit")
            }
            Self::Cancelled => formatter.write_str("index persistence was cancelled"),
            Self::TimedOut => formatter.write_str("index persistence timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("index persistence progress is unavailable")
            }
        }
    }
}

impl Error for KnowledgeIndexFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::SnapshotConflict
            | Self::SnapshotNotFound
            | Self::IndexRunAlreadyActive
            | Self::IndexRunNotFound
            | Self::InvalidIndexRunTransition
            | Self::IndexPublicationMismatch
            | Self::IndexPublicationTooLarge
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}
