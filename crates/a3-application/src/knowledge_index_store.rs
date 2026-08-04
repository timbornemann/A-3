use crate::KnowledgeStoreFailure;
use a3_domain::{
    IndexRunId, IndexRunRecord, IndexRunStart, IndexRunTerminalOutcome, ProjectIdentity,
    RepositoryFileState, Snapshot,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe deterministic-index persistence port.
pub type KnowledgeIndexFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KnowledgeIndexFailure>> + Send + 'a>>;

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

    /// Records a non-publishing terminal outcome for the active index attempt.
    ///
    /// Publishing is intentionally unavailable until the S10 adapter transaction
    /// can commit index data and the visible run in one atomic operation.
    fn finish_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord>;

    /// Returns the latest attempted run in durable worktree-local order.
    fn latest_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>>;

    /// Returns the last atomically published index, if S10 has published one.
    fn latest_published_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>>;
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
            | Self::InvalidIndexRunTransition => None,
        }
    }
}
