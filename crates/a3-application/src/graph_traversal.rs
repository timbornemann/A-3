use crate::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{GraphTraversalResult, ProjectIdentity, TraversalQuery};
use std::sync::Arc;

/// Inbound use case for bounded evidence-graph traversal.
#[derive(Debug)]
pub struct TraverseKnowledgeGraph {
    store: Arc<dyn KnowledgeSearchStore>,
}

impl TraverseKnowledgeGraph {
    /// Wires the read-only retrieval port.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeSearchStore>) -> Self {
        Self { store }
    }

    /// Returns deterministic shortest evidence paths without exposing storage details.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &TraversalQuery,
        control: &dyn KnowledgeSearchControl,
    ) -> Result<GraphTraversalResult, KnowledgeSearchFailure> {
        if control.is_cancelled() {
            return Err(KnowledgeSearchFailure::Cancelled);
        }
        self.store.traverse_graph(project, query, control).await
    }
}

#[cfg(test)]
mod tests {
    use super::TraverseKnowledgeGraph;
    use crate::{
        KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    };
    use a3_domain::{
        ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, GitHead,
        GitReferenceName, GraphTraversalResult, LexicalSearchCursor, LexicalSearchPage,
        LexicalSearchPageSize, LexicalSearchQuery, ProjectIdentity, RepositoryId,
        RepositoryIdentity, SymbolId, TraversalQuery, TraversalResultLimit, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Arc;

    #[derive(Debug)]
    struct CancelledControl;

    impl KnowledgeSearchControl for CancelledControl {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[derive(Debug)]
    struct MustNotRunStore;

    impl KnowledgeSearchStore for MustNotRunStore {
        fn search_exact<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ExactSearchQuery,
            _page_size: ExactSearchPageSize,
            _cursor: Option<&'a ExactSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }

        fn search_lexical<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a LexicalSearchQuery,
            _page_size: LexicalSearchPageSize,
            _cursor: Option<&'a LexicalSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }

        fn traverse_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a TraversalQuery,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }
    }

    #[test]
    fn cancelled_query_never_crosses_the_storage_boundary() -> Result<(), Box<dyn std::error::Error>>
    {
        let project = project()?;
        let query =
            TraversalQuery::callees(SymbolId::from_bytes([4; 32]), TraversalResultLimit::DEFAULT);
        let traversal = TraverseKnowledgeGraph::new(Arc::new(MustNotRunStore));

        assert_eq!(
            block_on(traversal.execute(&project, &query, &CancelledControl)),
            Err(KnowledgeSearchFailure::Cancelled)
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }
}
