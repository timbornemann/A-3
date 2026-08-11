use crate::{KnowledgeIndexFailure, KnowledgeIndexStore};
use a3_domain::ProjectIdentity;
use a3_domain::{IndexRunRecord, SnapshotId, WorktreeGeneration};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Bounded read model for the durable index state of one validated worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectIndexStatus {
    latest_snapshot: Option<ProjectSnapshotStatus>,
    latest_attempt: Option<IndexRunRecord>,
    published_snapshot_id: Option<SnapshotId>,
}

impl ProjectIndexStatus {
    /// Creates a status projection from independently validated storage results.
    #[must_use]
    pub const fn new(
        latest_snapshot: Option<ProjectSnapshotStatus>,
        latest_attempt: Option<IndexRunRecord>,
        published_snapshot_id: Option<SnapshotId>,
    ) -> Self {
        Self {
            latest_snapshot,
            latest_attempt,
            published_snapshot_id,
        }
    }

    /// Returns the latest immutable repository observation, if indexing observed one.
    #[must_use]
    pub const fn latest_snapshot(self) -> Option<ProjectSnapshotStatus> {
        self.latest_snapshot
    }

    /// Returns the most recent durable index attempt, whether terminal or active.
    #[must_use]
    pub const fn latest_attempt(self) -> Option<IndexRunRecord> {
        self.latest_attempt
    }

    /// Returns the snapshot still visible through the atomic publish boundary.
    #[must_use]
    pub const fn published_snapshot_id(self) -> Option<SnapshotId> {
        self.published_snapshot_id
    }
}

/// Identity and monotone generation of the latest durable snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectSnapshotStatus {
    id: SnapshotId,
    generation: WorktreeGeneration,
}

impl ProjectSnapshotStatus {
    /// Creates a summary from a storage-validated snapshot.
    #[must_use]
    pub const fn new(id: SnapshotId, generation: WorktreeGeneration) -> Self {
        Self { id, generation }
    }

    /// Returns the snapshot identity.
    #[must_use]
    pub const fn id(self) -> SnapshotId {
        self.id
    }

    /// Returns the worktree-local monotone generation.
    #[must_use]
    pub const fn generation(self) -> WorktreeGeneration {
        self.generation
    }
}

/// Read-only use case for the active project's durable index state.
#[derive(Debug)]
pub struct GetProjectIndexStatus {
    store: Arc<dyn KnowledgeIndexStore>,
}

impl GetProjectIndexStatus {
    /// Wires the existing deterministic-index persistence capability.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeIndexStore>) -> Self {
        Self { store }
    }

    /// Loads only bounded snapshot and run metadata for a validated project identity.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
    ) -> Result<ProjectIndexStatus, GetProjectIndexStatusError> {
        let latest_snapshot = self
            .store
            .latest_snapshot(project)
            .await
            .map_err(GetProjectIndexStatusError::Storage)?
            .map(|snapshot| ProjectSnapshotStatus::new(snapshot.id(), snapshot.generation()));
        let latest_attempt = self
            .store
            .latest_index_run(project)
            .await
            .map_err(GetProjectIndexStatusError::Storage)?;
        let published_snapshot_id = self
            .store
            .latest_published_index_run(project)
            .await
            .map_err(GetProjectIndexStatusError::Storage)?
            .map(IndexRunRecord::snapshot_id);

        Ok(ProjectIndexStatus::new(
            latest_snapshot,
            latest_attempt,
            published_snapshot_id,
        ))
    }
}

/// Failure while querying the bounded project index read model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetProjectIndexStatusError {
    /// The deterministic-index persistence boundary could not be read safely.
    Storage(KnowledgeIndexFailure),
}

impl fmt::Display for GetProjectIndexStatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project index status failed: {error}"),
        }
    }
}

impl Error for GetProjectIndexStatusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GetProjectIndexStatus, ProjectIndexStatus};
    use crate::{
        IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeIndexFuture, KnowledgeIndexStore,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, IndexPublication, IndexRunId,
        IndexRunRecord, IndexRunStart, IndexRunTerminalOutcome, ProjectIdentity, PublishedIndex,
        RepositoryFileState, RepositoryId, RepositoryIdentity, Snapshot, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Arc;

    #[derive(Debug)]
    struct EmptyIndexStore;

    impl KnowledgeIndexStore for EmptyIndexStore {
        fn append_snapshot<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _snapshot: &'a Snapshot,
        ) -> KnowledgeIndexFuture<'a, ()> {
            unavailable()
        }

        fn latest_snapshot<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
            Box::pin(async { Ok(None) })
        }

        fn current_file_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, RepositoryFileState> {
            unavailable()
        }

        fn start_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _request: IndexRunStart,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            unavailable()
        }

        fn finish_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: IndexRunId,
            _outcome: IndexRunTerminalOutcome,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            unavailable()
        }

        fn publish_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: IndexRunId,
            _publication: &'a IndexPublication,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            unavailable()
        }

        fn latest_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
            Box::pin(async { Ok(None) })
        }

        fn latest_published_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
            Box::pin(async { Ok(None) })
        }

        fn latest_published_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>> {
            unavailable()
        }

        fn rebuild_regenerable_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, ()> {
            unavailable()
        }
    }

    fn unavailable<'a, T>() -> KnowledgeIndexFuture<'a, T> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
    }

    #[test]
    fn query_returns_empty_metadata_before_the_first_index_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        let query = GetProjectIndexStatus::new(Arc::new(EmptyIndexStore));

        assert_eq!(
            block_on(query.execute(&project()?))?,
            ProjectIndexStatus::new(None, None, None)
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
