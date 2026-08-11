use a3_domain::{ProjectId, ProjectIdentity};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Owned future returned by the object-safe project-catalog administration boundary.
pub type ProjectCatalogAdminFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProjectCatalogAdminFailure>> + Send + 'a>>;

/// Narrow persistence boundary for removing one validated worktree from the project list.
pub trait ProjectCatalogAdmin: fmt::Debug + Send + Sync {
    /// Removes only the recent-list projection and pending reconciliation intents.
    ///
    /// Repository files, stable catalog identity anchors, and private project storage must remain.
    fn remove_recent_worktree<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        project_id: ProjectId,
    ) -> ProjectCatalogAdminFuture<'a, ()>;
}

/// Application use case for safely hiding one worktree from the A^3 project list.
#[derive(Debug)]
pub struct RemoveProjectFromList {
    store: Arc<dyn ProjectCatalogAdmin>,
}

impl RemoveProjectFromList {
    /// Wires the narrow project-catalog administration port.
    #[must_use]
    pub fn new(store: Arc<dyn ProjectCatalogAdmin>) -> Self {
        Self { store }
    }

    /// Removes exactly the Core-owned project identity while retaining all private knowledge.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        project_id: ProjectId,
    ) -> Result<RemovedProject, RemoveProjectFromListError> {
        self.store
            .remove_recent_worktree(project, project_id)
            .await
            .map_err(RemoveProjectFromListError::Storage)?;
        Ok(RemovedProject)
    }
}

/// Successful removal whose retention guarantee is fixed by the use-case contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedProject;

impl RemovedProject {
    /// Private A^3 data is always retained by this operation.
    #[must_use]
    pub const fn retained_private_storage(self) -> bool {
        true
    }
}

/// Stable project-catalog administration failure classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCatalogAdminFailure {
    /// The catalog could not be opened or written.
    Unavailable,
    /// Catalog integrity checks or SQLite corruption detection failed.
    Corrupt,
    /// The catalog was created by a newer A^3 build.
    UnsupportedSchema,
    /// Stored catalog rows violated the versioned logical schema.
    InvalidStoredData,
    /// The supplied identity does not exactly match the stored recent entry.
    IdentityConflict,
    /// The exact worktree is no longer present in the recent-project list.
    NotFound,
}

impl fmt::Display for ProjectCatalogAdminFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("project catalog is unavailable"),
            Self::Corrupt => formatter.write_str("project catalog is corrupt"),
            Self::UnsupportedSchema => formatter.write_str("project catalog schema is newer"),
            Self::InvalidStoredData => formatter.write_str("project catalog data is invalid"),
            Self::IdentityConflict => formatter.write_str("project catalog identity conflicts"),
            Self::NotFound => formatter.write_str("project is not in the recent-project list"),
        }
    }
}

impl Error for ProjectCatalogAdminFailure {}

/// Failure while removing a project from the A^3 project list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveProjectFromListError {
    /// The persistence boundary could not complete the exact removal.
    Storage(ProjectCatalogAdminFailure),
}

impl fmt::Display for RemoveProjectFromListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project-list removal failed: {error}"),
        }
    }
}

impl Error for RemoveProjectFromListError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectCatalogAdmin, ProjectCatalogAdminFailure, ProjectCatalogAdminFuture,
        RemoveProjectFromList,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId,
        RepositoryIdentity, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingStore {
        request: Mutex<Option<(ProjectId, ProjectIdentity)>>,
        result: Result<(), ProjectCatalogAdminFailure>,
    }

    impl ProjectCatalogAdmin for RecordingStore {
        fn remove_recent_worktree<'a>(
            &'a self,
            project: &'a ProjectIdentity,
            project_id: ProjectId,
        ) -> ProjectCatalogAdminFuture<'a, ()> {
            let recorded = self.request.lock().map(|mut request| {
                *request = Some((project_id, project.clone()));
            });
            let result = if recorded.is_ok() {
                self.result
            } else {
                Err(ProjectCatalogAdminFailure::Unavailable)
            };
            Box::pin(async move { result })
        }
    }

    #[test]
    fn removal_forwards_exact_core_owned_identity_and_guarantees_retention()
    -> Result<(), Box<dyn std::error::Error>> {
        let project = project()?;
        let project_id = ProjectId::from_bytes([7; 32]);
        let store = Arc::new(RecordingStore {
            request: Mutex::new(None),
            result: Ok(()),
        });
        let use_case = RemoveProjectFromList::new(store.clone());

        let removed = block_on(use_case.execute(&project, project_id))?;

        assert!(removed.retained_private_storage());
        assert_eq!(
            store
                .request
                .lock()
                .map_err(|_| "request lock poisoned")?
                .as_ref(),
            Some(&(project_id, project))
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
                CanonicalDirectory::from_canonicalized(root.clone())?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
