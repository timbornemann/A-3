use crate::{
    KnowledgeStore, KnowledgeStoreFailure, ProjectInspectionFailure, ProjectInspector,
    RecentProject, StoredProjectTarget,
};
use a3_domain::{ProjectId, ProjectIdentity, WorktreeId};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Fixed number of safe project summaries returned per catalog page.
pub const PROJECT_CATALOG_PAGE_SIZE: usize = 25;
const MAX_PROJECT_CATALOG_SEARCH_CHARS: usize = 128;

/// Opaque application cursor. Its numeric position is never authoritative outside the Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectCatalogCursor(u64);

impl ProjectCatalogCursor {
    /// Creates a non-zero opaque cursor position.
    pub fn new(value: u64) -> Result<Self, ProjectCatalogQueryError> {
        if value == 0 {
            return Err(ProjectCatalogQueryError::InvalidCursor);
        }
        Ok(Self(value))
    }

    #[must_use]
    /// Returns the Core-owned position for an adapter to encode.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Direction in which a cursor-bound page is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCatalogDirection {
    /// Reads the first page without a cursor.
    Initial,
    /// Reads the page after the supplied cursor.
    Next,
    /// Reads the page before the supplied cursor.
    Previous,
}

/// Strict, bounded project-catalog query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCatalogQuery {
    search: Option<String>,
    cursor: Option<ProjectCatalogCursor>,
    direction: ProjectCatalogDirection,
}

impl ProjectCatalogQuery {
    /// Validates a bounded search and the cursor/direction relationship.
    pub fn new(
        search: Option<String>,
        cursor: Option<ProjectCatalogCursor>,
        direction: ProjectCatalogDirection,
    ) -> Result<Self, ProjectCatalogQueryError> {
        let search = search
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if search.as_ref().is_some_and(|value| {
            value.chars().count() > MAX_PROJECT_CATALOG_SEARCH_CHARS
                || value.chars().any(char::is_control)
        }) {
            return Err(ProjectCatalogQueryError::InvalidSearch);
        }
        match (direction, cursor) {
            (ProjectCatalogDirection::Initial, None)
            | (ProjectCatalogDirection::Next | ProjectCatalogDirection::Previous, Some(_)) => {}
            _ => return Err(ProjectCatalogQueryError::InvalidCursor),
        }
        Ok(Self {
            search,
            cursor,
            direction,
        })
    }

    #[must_use]
    /// Returns the normalized optional search text.
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    #[must_use]
    /// Returns the optional opaque cursor.
    pub const fn cursor(&self) -> Option<ProjectCatalogCursor> {
        self.cursor
    }

    #[must_use]
    /// Returns the requested page direction.
    pub const fn direction(&self) -> ProjectCatalogDirection {
        self.direction
    }
}

/// One bounded page plus opaque navigation cursors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCatalogPage {
    projects: Vec<RecentProject>,
    previous_cursor: Option<ProjectCatalogCursor>,
    next_cursor: Option<ProjectCatalogCursor>,
}

impl ProjectCatalogPage {
    #[must_use]
    /// Creates a bounded catalog page after adapter validation.
    pub const fn new(
        projects: Vec<RecentProject>,
        previous_cursor: Option<ProjectCatalogCursor>,
        next_cursor: Option<ProjectCatalogCursor>,
    ) -> Self {
        Self {
            projects,
            previous_cursor,
            next_cursor,
        }
    }

    #[must_use]
    /// Returns the safe project summaries in activation order.
    pub fn projects(&self) -> &[RecentProject] {
        &self.projects
    }

    #[must_use]
    /// Returns the cursor for the preceding page when one exists.
    pub const fn previous_cursor(&self) -> Option<ProjectCatalogCursor> {
        self.previous_cursor
    }

    #[must_use]
    /// Returns the cursor for the following page when one exists.
    pub const fn next_cursor(&self) -> Option<ProjectCatalogCursor> {
        self.next_cursor
    }
}

/// Invalid search or cursor input at the application boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCatalogQueryError {
    /// The search exceeded its bound or contained control characters.
    InvalidSearch,
    /// The cursor was zero or did not match the requested direction.
    InvalidCursor,
}

impl fmt::Display for ProjectCatalogQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSearch => formatter.write_str("project catalog search is invalid"),
            Self::InvalidCursor => formatter.write_str("project catalog cursor is invalid"),
        }
    }
}

impl Error for ProjectCatalogQueryError {}

/// Revalidates one durable catalog target before it can become active.
#[derive(Debug)]
pub struct ActivateCatalogProject {
    inspector: Arc<dyn ProjectInspector>,
    store: Arc<dyn KnowledgeStore>,
}

impl ActivateCatalogProject {
    #[must_use]
    /// Creates the revalidation use case from the inspection and storage ports.
    pub fn new(inspector: Arc<dyn ProjectInspector>, store: Arc<dyn KnowledgeStore>) -> Self {
        Self { inspector, store }
    }

    /// Resolves and revalidates exactly one WebView-selected catalog ID.
    pub async fn execute(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<(ProjectIdentity, ProjectId), ActivateCatalogProjectError> {
        let target = self
            .store
            .resolve_project_catalog_entry(worktree_id)
            .await
            .map_err(ActivateCatalogProjectError::Storage)?
            .ok_or(ActivateCatalogProjectError::NotFound)?;
        self.activate(target).await
    }

    /// Resolves and revalidates only the most recently activated catalog entry.
    pub async fn restore_last(
        &self,
    ) -> Result<Option<(ProjectIdentity, ProjectId)>, ActivateCatalogProjectError> {
        let Some(target) = self
            .store
            .resolve_last_project_catalog_entry()
            .await
            .map_err(ActivateCatalogProjectError::Storage)?
        else {
            return Ok(None);
        };
        self.activate(target).await.map(Some)
    }

    async fn activate(
        &self,
        target: StoredProjectTarget,
    ) -> Result<(ProjectIdentity, ProjectId), ActivateCatalogProjectError> {
        let observed = self
            .inspector
            .inspect_project(target.worktree_root())
            .map_err(ActivateCatalogProjectError::Inspection)?;
        if observed.worktree().id() != target.worktree_id()
            || observed.repository().id() != target.repository_id()
        {
            return Err(ActivateCatalogProjectError::IdentityConflict);
        }
        Ok((observed, target.project_id()))
    }
}

/// Stable failure classes for stored-project activation and restoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivateCatalogProjectError {
    /// Durable catalog data could not be read safely.
    Storage(KnowledgeStoreFailure),
    /// The saved native root could not be inspected as a project.
    Inspection(ProjectInspectionFailure),
    /// The repository or worktree no longer has its stored identity.
    IdentityConflict,
    /// The requested worktree is not in the catalog.
    NotFound,
}

impl fmt::Display for ActivateCatalogProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project catalog read failed: {error}"),
            Self::Inspection(error) => {
                write!(formatter, "stored project validation failed: {error}")
            }
            Self::IdentityConflict => formatter.write_str("stored project identity conflicts"),
            Self::NotFound => formatter.write_str("project is not in the catalog"),
        }
    }
}

impl Error for ActivateCatalogProjectError {}

#[cfg(test)]
mod tests {
    use super::{
        ActivateCatalogProject, ActivateCatalogProjectError, ProjectCatalogCursor,
        ProjectCatalogDirection, ProjectCatalogQuery,
    };
    use crate::{
        KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectOpenPreparation,
        ProjectReconciliationProposal, RecentProject, RecentProjectLimit, StoredProjectTarget,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId,
        RepositoryIdentity, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct FixedInspector {
        calls: Arc<AtomicUsize>,
        result: Result<ProjectIdentity, crate::ProjectInspectionFailure>,
    }

    impl crate::ProjectInspector for FixedInspector {
        fn inspect_project(
            &self,
            _selected_root: &Path,
        ) -> Result<ProjectIdentity, crate::ProjectInspectionFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    #[derive(Debug)]
    struct FixedStore {
        target: Option<StoredProjectTarget>,
        record_calls: Arc<AtomicUsize>,
    }

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
            self.record_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(ProjectId::from_bytes([3; 32])) })
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
            _limit: RecentProjectLimit,
        ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn resolve_project_catalog_entry(
            &self,
            worktree_id: WorktreeId,
        ) -> KnowledgeStoreFuture<'_, Option<StoredProjectTarget>> {
            let target = self
                .target
                .clone()
                .filter(|target| target.worktree_id() == worktree_id);
            Box::pin(async move { Ok(target) })
        }

        fn resolve_last_project_catalog_entry(
            &self,
        ) -> KnowledgeStoreFuture<'_, Option<StoredProjectTarget>> {
            let target = self.target.clone();
            Box::pin(async move { Ok(target) })
        }
    }

    #[test]
    fn query_requires_cursor_only_for_navigation() {
        assert!(ProjectCatalogQuery::new(None, None, ProjectCatalogDirection::Initial).is_ok());
        assert!(ProjectCatalogQuery::new(None, None, ProjectCatalogDirection::Next).is_err());
        assert!(
            ProjectCatalogQuery::new(
                None,
                ProjectCatalogCursor::new(1).ok(),
                ProjectCatalogDirection::Initial,
            )
            .is_err()
        );
    }

    #[test]
    fn search_is_trimmed_bounded_and_control_free() {
        let query = ProjectCatalogQuery::new(
            Some("  workspace  ".to_owned()),
            None,
            ProjectCatalogDirection::Initial,
        );
        assert_eq!(
            query
                .ok()
                .and_then(|query| query.search().map(str::to_owned)),
            Some("workspace".to_owned())
        );
        assert!(
            ProjectCatalogQuery::new(
                Some("bad\nquery".to_owned()),
                None,
                ProjectCatalogDirection::Initial,
            )
            .is_err()
        );
    }

    #[test]
    fn activation_revalidates_the_exact_saved_identity_without_reordering_first()
    -> Result<(), Box<dyn std::error::Error>> {
        let project = fixture_project([1; 32], [2; 32])?;
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let record_calls = Arc::new(AtomicUsize::new(0));
        let use_case = ActivateCatalogProject::new(
            Arc::new(FixedInspector {
                calls: Arc::clone(&inspection_calls),
                result: Ok(project.clone()),
            }),
            Arc::new(FixedStore {
                target: Some(fixture_target([1; 32], [2; 32])?),
                record_calls: Arc::clone(&record_calls),
            }),
        );

        let (observed, project_id) = block_on(use_case.execute(WorktreeId::from_bytes([2; 32])))?;

        assert_eq!(observed, project);
        assert_eq!(project_id, ProjectId::from_bytes([3; 32]));
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 1);
        assert_eq!(record_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn identity_conflict_is_rejected_before_activation_order_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let record_calls = Arc::new(AtomicUsize::new(0));
        let use_case = ActivateCatalogProject::new(
            Arc::new(FixedInspector {
                calls: Arc::new(AtomicUsize::new(0)),
                result: Ok(fixture_project([8; 32], [9; 32])?),
            }),
            Arc::new(FixedStore {
                target: Some(fixture_target([1; 32], [2; 32])?),
                record_calls: Arc::clone(&record_calls),
            }),
        );

        assert_eq!(
            block_on(use_case.execute(WorktreeId::from_bytes([2; 32]))),
            Err(ActivateCatalogProjectError::IdentityConflict)
        );
        assert_eq!(record_calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn restore_failure_does_not_fall_back_to_another_catalog_entry()
    -> Result<(), Box<dyn std::error::Error>> {
        let inspection_calls = Arc::new(AtomicUsize::new(0));
        let use_case = ActivateCatalogProject::new(
            Arc::new(FixedInspector {
                calls: Arc::clone(&inspection_calls),
                result: Err(crate::ProjectInspectionFailure::SelectionUnavailable),
            }),
            Arc::new(FixedStore {
                target: Some(fixture_target([1; 32], [2; 32])?),
                record_calls: Arc::new(AtomicUsize::new(0)),
            }),
        );

        assert_eq!(
            block_on(use_case.restore_last()),
            Err(ActivateCatalogProjectError::Inspection(
                crate::ProjectInspectionFailure::SelectionUnavailable
            ))
        );
        assert_eq!(inspection_calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    fn fixture_target(
        repository: [u8; 32],
        worktree: [u8; 32],
    ) -> Result<StoredProjectTarget, Box<dyn std::error::Error>> {
        Ok(StoredProjectTarget::new(
            ProjectId::from_bytes([3; 32]),
            RepositoryId::from_bytes(repository),
            WorktreeId::from_bytes(worktree),
            std::env::current_dir()?,
        ))
    }

    fn fixture_project(
        repository: [u8; 32],
        worktree: [u8; 32],
    ) -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = CanonicalDirectory::from_canonicalized(std::env::current_dir()?)?;
        let repository_id = RepositoryId::from_bytes(repository);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes(worktree),
                WorktreeAnchorId::from_bytes([4; 32]),
                repository_id,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
