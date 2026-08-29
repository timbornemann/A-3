use a3_domain::{IndexRunId, ProjectIdentity, SnapshotId};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Exact immutable Fast-Index anchor evaluated by the Deep-Map lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapPublicationAnchor {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
}

impl DeepMapPublicationAnchor {
    /// Creates an anchor from one atomically published index.
    #[must_use]
    pub const fn new(index_run_id: IndexRunId, snapshot_id: SnapshotId) -> Self {
        Self {
            index_run_id,
            snapshot_id,
        }
    }

    /// Returns the exact latest index run.
    #[must_use]
    pub const fn index_run_id(self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable indexed snapshot.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }
}

/// Consistent read-only publication projection for one active project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapPublicationState {
    /// No complete published Fast Index exists.
    NoPublishedIndex,
    /// Latest index exists and has not received a Module-Card publication.
    Ready(DeepMapPublicationAnchor),
    /// Latest index has one complete immutable Module-Card publication.
    Current {
        /// Exact publication anchor.
        anchor: DeepMapPublicationAnchor,
        /// Number of published Module Cards and matching FTS documents.
        card_count: u64,
    },
}

impl DeepMapPublicationState {
    /// Returns the latest index anchor when one exists.
    #[must_use]
    pub const fn anchor(self) -> Option<DeepMapPublicationAnchor> {
        match self {
            Self::NoPublishedIndex => None,
            Self::Ready(anchor) | Self::Current { anchor, .. } => Some(anchor),
        }
    }

    /// Returns whether the latest index is already completely mapped.
    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current { .. })
    }
}

/// Owned asynchronous publication-state read.
pub type DeepMapPublicationStateFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<DeepMapPublicationState, DeepMapPublicationStateFailure>>
            + Send
            + 'a,
    >,
>;

/// Read-only boundary that classifies the latest index before any model work starts.
pub trait DeepMapPublicationStateStore: fmt::Debug + Send + Sync {
    /// Reads index, card, FTS and projection counts from one consistent storage snapshot.
    fn load_deep_map_publication_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> DeepMapPublicationStateFuture<'a>;
}

/// Safe publication-state read failure without SQL or row details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapPublicationStateFailure {
    /// Local knowledge storage was unavailable.
    Storage,
    /// Stored card, FTS, or projection markers contradicted each other.
    InvalidStoredData,
}

impl fmt::Display for DeepMapPublicationStateFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Storage => "Deep-Map publication state is unavailable",
            Self::InvalidStoredData => "Deep-Map publication state is inconsistent",
        })
    }
}

impl Error for DeepMapPublicationStateFailure {}
