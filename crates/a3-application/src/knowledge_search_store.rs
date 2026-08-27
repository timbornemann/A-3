use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{
    ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ExactSearchTarget,
    GraphTraversalResult, IndexRunId, LexicalSearchCursor, LexicalSearchPage,
    LexicalSearchPageSize, LexicalSearchQuery, ModuleId, ProjectIdentity, SourceChannel,
    TraversalQuery,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the object-safe deterministic retrieval port.
pub type KnowledgeSearchFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KnowledgeSearchFailure>> + Send + 'a>>;

/// Cooperative cancellation boundary for bounded local retrieval.
pub trait KnowledgeSearchControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl KnowledgeSearchControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Read-only deterministic search capability of the local KnowledgeStore boundary.
///
/// SQL, database handles, persisted rows, and engine errors remain inside adapters.
pub trait KnowledgeSearchStore: fmt::Debug + Send + Sync {
    /// Searches exactly one atomically published index using stable keyset pagination.
    fn search_exact<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ExactSearchQuery,
        page_size: ExactSearchPageSize,
        cursor: Option<&'a ExactSearchCursor>,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, ExactSearchPage>;

    /// Searches weighted lexical candidates from exactly one atomically published index.
    fn search_lexical<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a LexicalSearchQuery,
        page_size: LexicalSearchPageSize,
        cursor: Option<&'a LexicalSearchCursor>,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, LexicalSearchPage>;

    /// Resolves each current search target to exactly one primary module when the published
    /// membership projection proves that association. Ambiguous and unassigned targets remain
    /// deliberately unbound.
    fn bind_modules<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _index_run_id: IndexRunId,
        targets: &'a [ExactSearchTarget],
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, Vec<Option<ModuleId>>> {
        Box::pin(async move { Ok(vec![None; targets.len()]) })
    }

    /// Traverses typed relationships in exactly one atomically published evidence graph.
    fn traverse_graph<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a TraversalQuery,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, GraphTraversalResult>;
}

/// Stable application classification of deterministic retrieval failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeSearchFailure {
    /// The shared local storage boundary failed.
    Storage(KnowledgeStoreFailure),
    /// The worktree has no atomically published index yet.
    IndexUnavailable,
    /// The requested file or symbol seed is not part of the current published graph.
    SeedUnavailable,
    /// The supplied cursor belongs to another query, snapshot, or published run.
    InvalidCursor,
    /// The published run predates the required channel-specific projection.
    ProjectionUnavailable(SourceChannel),
    /// Durable retrieval rows violated a domain or publication invariant.
    InvalidStoredProjection,
    /// The owning operation cancelled retrieval.
    Cancelled,
    /// The bounded read exceeded its fixed wall-clock deadline.
    TimedOut,
}

impl fmt::Display for KnowledgeSearchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "knowledge search storage failed: {error}"),
            Self::IndexUnavailable => formatter.write_str("no published index is available"),
            Self::SeedUnavailable => {
                formatter.write_str("graph traversal seed is unavailable in the published index")
            }
            Self::InvalidCursor => {
                formatter.write_str("search cursor is stale or does not match the query")
            }
            Self::ProjectionUnavailable(channel) => write!(
                formatter,
                "{channel:?} search projection is not available for the published run"
            ),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored search projection is invalid")
            }
            Self::Cancelled => formatter.write_str("knowledge search was cancelled"),
            Self::TimedOut => formatter.write_str("knowledge search timed out"),
        }
    }
}

impl Error for KnowledgeSearchFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::IndexUnavailable
            | Self::SeedUnavailable
            | Self::InvalidCursor
            | Self::ProjectionUnavailable(_)
            | Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
