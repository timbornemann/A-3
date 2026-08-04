use crate::project_catalog::ProjectCatalogError;
use crate::{
    CatalogDatabase, CatalogOpenError, KnowledgeDatabase, KnowledgeOpenError,
    ProjectStorageLayoutError, StorageLayout,
};
use crate::{index_repository, index_repository::IndexRepositoryError};
use a3_application::{
    KnowledgeIndexFailure, KnowledgeIndexFuture, KnowledgeIndexStore, KnowledgeStore,
    KnowledgeStoreFailure, KnowledgeStoreFuture, RecentProject, RecentProjectLimit,
};
use a3_domain::{
    IndexRunId, IndexRunRecord, IndexRunStart, IndexRunTerminalOutcome, ProjectId, ProjectIdentity,
    Snapshot,
};

/// Local libSQL implementation of the application knowledge-store boundary.
///
/// A project is recorded in the global catalog only after its identity-bound
/// per-worktree database has been opened and verified successfully.
pub struct LibsqlKnowledgeStore {
    layout: StorageLayout,
    catalog: CatalogDatabase,
}

impl LibsqlKnowledgeStore {
    /// Opens the global catalog and retains the validated app-data layout used
    /// to derive private per-worktree database paths.
    pub async fn open(layout: &StorageLayout) -> Result<Self, CatalogOpenError> {
        let catalog = CatalogDatabase::open(layout).await?;
        Ok(Self {
            layout: layout.clone(),
            catalog,
        })
    }
}

impl std::fmt::Debug for LibsqlKnowledgeStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LibsqlKnowledgeStore")
            .field("layout", &self.layout)
            .field("catalog", &self.catalog)
            .finish()
    }
}

impl KnowledgeStore for LibsqlKnowledgeStore {
    fn record_opened_project<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeStoreFuture<'a, ProjectId> {
        Box::pin(async move {
            let project_layout = self
                .layout
                .prepare_project(project.worktree())
                .map_err(classify_project_layout_error)?;
            let _knowledge = KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)?;
            self.catalog
                .record_project(project)
                .await
                .map_err(ProjectCatalogError::classify)
        })
    }

    fn list_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
        Box::pin(async move {
            self.catalog
                .read_recent_projects(limit)
                .await
                .map_err(ProjectCatalogError::classify)
        })
    }
}

impl KnowledgeIndexStore for LibsqlKnowledgeStore {
    fn append_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot: &'a Snapshot,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::append_snapshot(
                knowledge.connection(),
                project.worktree().id(),
                snapshot,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_snapshot(knowledge.connection(), project.worktree().id())
                .await
                .map_err(IndexRepositoryError::classify)
        })
    }

    fn start_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: IndexRunStart,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::start_index_run(
                knowledge.connection(),
                project.worktree().id(),
                request,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn finish_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::finish_index_run(
                knowledge.connection(),
                project.worktree().id(),
                run_id,
                outcome,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_index_run(
                knowledge.connection(),
                project.worktree().id(),
                false,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_published_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_index_run(
                knowledge.connection(),
                project.worktree().id(),
                true,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }
}

impl LibsqlKnowledgeStore {
    async fn open_project_knowledge(
        &self,
        project: &ProjectIdentity,
    ) -> Result<KnowledgeDatabase, KnowledgeIndexFailure> {
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(KnowledgeIndexFailure::Storage)?;
        KnowledgeDatabase::open(&project_layout, project)
            .await
            .map_err(classify_knowledge_open_error)
            .map_err(KnowledgeIndexFailure::Storage)
    }
}

fn classify_project_layout_error(error: ProjectStorageLayoutError) -> KnowledgeStoreFailure {
    match error {
        ProjectStorageLayoutError::SymbolicLink { .. }
        | ProjectStorageLayoutError::NotDirectory { .. }
        | ProjectStorageLayoutError::NotRegularFile { .. }
        | ProjectStorageLayoutError::OutsideParent { .. } => {
            KnowledgeStoreFailure::InvalidStoredData
        }
        ProjectStorageLayoutError::StorageInsideWorktree { .. }
        | ProjectStorageLayoutError::Create { .. }
        | ProjectStorageLayoutError::Inspect { .. }
        | ProjectStorageLayoutError::Canonicalize { .. } => KnowledgeStoreFailure::Unavailable,
    }
}

fn classify_knowledge_open_error(error: KnowledgeOpenError) -> KnowledgeStoreFailure {
    match error {
        KnowledgeOpenError::Layout(error) => classify_project_layout_error(error),
        KnowledgeOpenError::IntegrityCheckFailed | KnowledgeOpenError::CorruptDatabase => {
            KnowledgeStoreFailure::Corrupt
        }
        KnowledgeOpenError::NewerSchema { .. } => KnowledgeStoreFailure::UnsupportedSchema,
        KnowledgeOpenError::ConnectionPolicyMismatch
        | KnowledgeOpenError::MigrationHistoryMismatch { .. }
        | KnowledgeOpenError::InspectIdentity(_)
        | KnowledgeOpenError::InvalidStoredData
        | KnowledgeOpenError::UnexpectedSchemaVersion { .. } => {
            KnowledgeStoreFailure::InvalidStoredData
        }
        KnowledgeOpenError::IdentityConflict => KnowledgeStoreFailure::IdentityConflict,
        KnowledgeOpenError::Open(_)
        | KnowledgeOpenError::Connect(_)
        | KnowledgeOpenError::Configure(_)
        | KnowledgeOpenError::InspectConnectionPolicy(_)
        | KnowledgeOpenError::InspectIntegrity(_)
        | KnowledgeOpenError::InspectSchema(_)
        | KnowledgeOpenError::InspectMigrationHistory(_)
        | KnowledgeOpenError::BeginMigration { .. }
        | KnowledgeOpenError::ApplyMigration { .. }
        | KnowledgeOpenError::RollbackMigration { .. }
        | KnowledgeOpenError::CommitMigration { .. } => KnowledgeStoreFailure::Unavailable,
    }
}
