use crate::{ExactSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{
    ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ProjectIdentity,
};
use std::sync::Arc;

/// Inbound use case for deterministic retrieval from one published repository snapshot.
#[derive(Debug)]
pub struct SearchExactIndex {
    store: Arc<dyn KnowledgeSearchStore>,
}

impl SearchExactIndex {
    /// Wires the read-only retrieval port.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeSearchStore>) -> Self {
        Self { store }
    }

    /// Returns one stable page without exposing persistence rows or engine capabilities.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ExactSearchQuery,
        page_size: ExactSearchPageSize,
        cursor: Option<&ExactSearchCursor>,
        control: &dyn ExactSearchControl,
    ) -> Result<ExactSearchPage, KnowledgeSearchFailure> {
        if control.is_cancelled() {
            return Err(KnowledgeSearchFailure::Cancelled);
        }
        self.store
            .search_exact(project, query, page_size, cursor, control)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::SearchExactIndex;
    use crate::{
        ExactSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    };
    use a3_domain::{
        ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ExactSearchTerm,
        GitHead, GitReferenceName, ProjectIdentity, RepositoryId, RepositoryIdentity,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Arc;

    #[derive(Debug)]
    struct CancelledControl;

    impl ExactSearchControl for CancelledControl {
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
            _control: &'a dyn ExactSearchControl,
        ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }
    }

    #[test]
    fn cancelled_query_never_crosses_the_storage_boundary() -> Result<(), Box<dyn std::error::Error>>
    {
        let project = project()?;
        let query = ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("main".to_owned())?);
        let search = SearchExactIndex::new(Arc::new(MustNotRunStore));

        assert_eq!(
            block_on(search.execute(
                &project,
                &query,
                ExactSearchPageSize::DEFAULT,
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
