use crate::{KnowledgeStore, KnowledgeStoreFailure, RecentProject, RecentProjectLimit};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Query use case for a bounded, most-recent-first project list.
#[derive(Debug)]
pub struct ListRecentProjects {
    store: Arc<dyn KnowledgeStore>,
    limit: RecentProjectLimit,
}

impl ListRecentProjects {
    /// Wires the persistence port with the default desktop list bound.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self {
            store,
            limit: RecentProjectLimit::DEFAULT,
        }
    }

    /// Returns recent projects without exposing raw persisted paths or database rows.
    pub async fn execute(&self) -> Result<Vec<RecentProject>, ListRecentProjectsError> {
        self.store
            .list_recent_projects(self.limit)
            .await
            .map_err(ListRecentProjectsError::Storage)
    }
}

/// Failure while querying recent projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListRecentProjectsError {
    /// The persistence port could not produce a valid bounded projection.
    Storage(KnowledgeStoreFailure),
}

impl fmt::Display for ListRecentProjectsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "recent project query failed: {error}"),
        }
    }
}

impl Error for ListRecentProjectsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ListRecentProjects;
    use crate::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectOpenPreparation,
        ProjectPathDisplay, ProjectReconciliationProposal, RecentProject, RecentProjectLimit,
    };
    use a3_domain::{
        GitHead, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId, WorktreeId,
    };
    use futures::executor::block_on;
    use std::sync::Arc;

    #[derive(Debug)]
    struct FixedStore(Vec<RecentProject>);

    impl KnowledgeStore for FixedStore {
        fn prepare_project_open<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectOpenPreparation> {
            Box::pin(async { Ok(ProjectOpenPreparation::Ready) })
        }

        fn record_opened_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            Box::pin(async { Err(KnowledgeStoreFailure::Unavailable) })
        }

        fn reconcile_project<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _proposal: &'a ProjectReconciliationProposal,
        ) -> KnowledgeStoreFuture<'a, ProjectId> {
            Box::pin(async { Err(KnowledgeStoreFailure::Unavailable) })
        }

        fn list_recent_projects(
            &self,
            limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            let projects = self
                .0
                .iter()
                .take(usize::from(limit.get()))
                .cloned()
                .collect();
            Box::pin(async move { Ok(projects) })
        }
    }

    #[test]
    fn recent_project_query_returns_the_bounded_port_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let expected = RecentProject::new(
            ProjectId::from_bytes([1; 32]),
            RepositoryId::from_bytes([2; 32]),
            WorktreeId::from_bytes([3; 32]),
            ProjectPathDisplay::try_from_stored("/worktree".to_owned())?,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        );
        let query = ListRecentProjects::new(Arc::new(FixedStore(vec![expected.clone()])));

        assert_eq!(block_on(query.execute())?, vec![expected]);
        Ok(())
    }
}
