use crate::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{
    LexicalSearchCursor, LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery,
    ProjectIdentity,
};
use std::sync::Arc;

/// Inbound use case for bounded typo-tolerant retrieval from one published snapshot.
#[derive(Debug)]
pub struct SearchLexicalIndex {
    store: Arc<dyn KnowledgeSearchStore>,
}

impl SearchLexicalIndex {
    /// Wires the read-only retrieval port.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeSearchStore>) -> Self {
        Self { store }
    }

    /// Returns one deterministic weighted page without exposing FTS or storage details.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &LexicalSearchQuery,
        page_size: LexicalSearchPageSize,
        cursor: Option<&LexicalSearchCursor>,
        control: &dyn KnowledgeSearchControl,
    ) -> Result<LexicalSearchPage, KnowledgeSearchFailure> {
        if control.is_cancelled() {
            return Err(KnowledgeSearchFailure::Cancelled);
        }
        self.store
            .search_lexical(project, query, page_size, cursor, control)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::SearchLexicalIndex;
    use crate::{
        KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    };
    use a3_domain::{
        ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, GitHead,
        GitReferenceName, LexicalSearchCursor, LexicalSearchPage, LexicalSearchPageSize,
        LexicalSearchQuery, LexicalSearchTerm, ProjectIdentity, RepositoryId, RepositoryIdentity,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
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
    }

    #[test]
    fn cancelled_query_never_crosses_the_storage_boundary() -> Result<(), Box<dyn std::error::Error>>
    {
        let project = project()?;
        let query =
            LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("launcj".to_owned())?);
        let search = SearchLexicalIndex::new(Arc::new(MustNotRunStore));

        assert_eq!(
            block_on(search.execute(
                &project,
                &query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &CancelledControl,
            )),
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
