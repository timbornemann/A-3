use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{
    ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, LexicalSearchCursor,
    LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery, ProjectIdentity, SourceChannel,
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
}

/// Stable application classification of deterministic retrieval failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeSearchFailure {
    /// The shared local storage boundary failed.
    Storage(KnowledgeStoreFailure),
    /// The worktree has no atomically published index yet.
    IndexUnavailable,
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
            | Self::InvalidCursor
            | Self::ProjectionUnavailable(_)
            | Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
