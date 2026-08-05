use crate::{EmbeddingOperationControl, JobContext, KnowledgeStoreFailure};
use a3_domain::{
    EmbeddingCacheKey, EmbeddingModelProfile, EmbeddingVector, Progress, ProjectIdentity,
    SemanticEmbedding, SnapshotId, VectorSearchCapability, VectorSearchLimit, VectorSearchResult,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe semantic-cache storage port.
pub type SemanticEmbeddingStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SemanticEmbeddingStoreFailure>> + Send + 'a>>;

/// Cancellation and monotone progress required by the long-running semantic-cache rebuild.
pub trait SemanticCacheRebuildControl: EmbeddingOperationControl {
    /// Reports determinate rows removed from the regenerable cache.
    fn report_progress(&self, progress: Progress) -> Result<(), SemanticCacheRebuildProgressError>;
}

impl SemanticCacheRebuildControl for JobContext {
    fn report_progress(&self, progress: Progress) -> Result<(), SemanticCacheRebuildProgressError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| SemanticCacheRebuildProgressError::Unavailable)
    }
}

/// Owning job can no longer accept semantic-cache rebuild progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCacheRebuildProgressError {
    /// Scheduler event or lifecycle boundary is unavailable.
    Unavailable,
}

impl fmt::Display for SemanticCacheRebuildProgressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("semantic-cache rebuild progress is unavailable")
    }
}

impl Error for SemanticCacheRebuildProgressError {}

/// Optional semantic-cache capability kept separate from deterministic index storage.
///
/// SQL, vector functions, database rows, and engine errors remain inside adapters.
pub trait SemanticEmbeddingStore: fmt::Debug + Send + Sync {
    /// Returns the subset of exact card/body/profile keys already cached locally.
    fn find_cached<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        keys: &'a [EmbeddingCacheKey],
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, Vec<EmbeddingCacheKey>>;

    /// Atomically stores one provider-bounded batch after full domain validation.
    fn store_batch<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        embeddings: &'a [SemanticEmbedding],
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, ()>;

    /// Reports whether native indexing or the deterministic fallback is active.
    fn vector_search_capability<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchCapability>;

    /// Produces non-evidentiary semantic candidates for exactly one snapshot and profile.
    fn search_similar<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        profile: &'a EmbeddingModelProfile,
        query: &'a EmbeddingVector,
        limit: VectorSearchLimit,
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchResult>;

    /// Removes only regenerable semantic cards, embeddings, and vector projections.
    fn rebuild_semantic_cache<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn SemanticCacheRebuildControl,
    ) -> SemanticEmbeddingStoreFuture<'a, ()>;
}

/// Stable semantic-cache failure classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticEmbeddingStoreFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// A persisted profile ID was paired with different vector-shaping metadata.
    ProfileConflict,
    /// Persisted card, body, vector, or capability state violated the logical schema.
    InvalidStoredData,
    /// A bounded semantic query exceeded its adapter deadline.
    TimedOut,
    /// The owning job cancelled before the operation completed.
    Cancelled,
    /// The owning job could not accept mandatory rebuild progress.
    ProgressUnavailable,
}

impl fmt::Display for SemanticEmbeddingStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(source) => write!(formatter, "semantic cache storage failed: {source}"),
            Self::ProfileConflict => {
                formatter.write_str("semantic cache profile metadata conflicts with its identity")
            }
            Self::InvalidStoredData => {
                formatter.write_str("semantic cache contains invalid stored data")
            }
            Self::TimedOut => formatter.write_str("semantic cache operation timed out"),
            Self::Cancelled => formatter.write_str("semantic cache operation was cancelled"),
            Self::ProgressUnavailable => {
                formatter.write_str("semantic cache rebuild progress is unavailable")
            }
        }
    }
}

impl Error for SemanticEmbeddingStoreFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::ProfileConflict
            | Self::InvalidStoredData
            | Self::TimedOut
            | Self::Cancelled
            | Self::ProgressUnavailable => None,
        }
    }
}
