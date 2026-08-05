use crate::catalog::{
    configure_connection, connection_policy_is_valid, integrity_is_valid, is_corruption,
};
use crate::migration::{
    KnowledgeSchemaVersion, MigrationError, migrate_knowledge, read_user_version,
    verify_knowledge_migration_history,
};
use crate::{ProjectStorageLayout, ProjectStorageLayoutError};
use a3_domain::{ProjectIdentity, RepositoryId, WorktreeId};
use libsql::{Connection, Database, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// An opened, identity-bound, policy-checked database for one worktree.
///
/// Handles and rows remain private to the adapter boundary.
pub struct KnowledgeDatabase {
    _database: Database,
    connection: Connection,
    path: PathBuf,
    schema_version: KnowledgeSchemaVersion,
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
}

impl KnowledgeDatabase {
    /// Opens, migrates, binds, and verifies the database for exactly one project observation.
    pub async fn open(
        layout: &ProjectStorageLayout,
        project: &ProjectIdentity,
    ) -> Result<Self, KnowledgeOpenError> {
        if layout.worktree_id() != project.worktree().id() {
            return Err(KnowledgeOpenError::IdentityConflict);
        }
        layout
            .validate_knowledge_target()
            .map_err(KnowledgeOpenError::Layout)?;
        if layout.knowledge_path().exists() {
            preflight_existing_knowledge(layout.knowledge_path(), project).await?;
        }

        let database = libsql::Builder::new_local(layout.knowledge_path())
            .build()
            .await
            .map_err(classify_open_error)?;
        let connection = database.connect().map_err(classify_connect_error)?;
        reject_newer_schema(&connection).await?;
        verify_integrity(&connection).await?;
        configure_connection(&connection)
            .await
            .map_err(classify_configuration_error)?;
        verify_connection_policy(&connection).await?;
        let schema_version = migrate_knowledge(
            &connection,
            project.repository().id().as_bytes(),
            project.worktree().id().as_bytes(),
        )
        .await
        .map_err(classify_migration_error)?;
        verify_identity(&connection, project).await?;
        layout
            .validate_knowledge_target()
            .map_err(KnowledgeOpenError::Layout)?;

        let knowledge = Self {
            _database: database,
            connection,
            path: layout.knowledge_path().to_path_buf(),
            schema_version,
            repository_id: project.repository().id(),
            worktree_id: project.worktree().id(),
        };
        knowledge.verify().await?;
        Ok(knowledge)
    }

    pub(crate) async fn reconcile_identity(
        layout: &ProjectStorageLayout,
        source_repository_id: RepositoryId,
        source_worktree_id: WorktreeId,
        target: &ProjectIdentity,
    ) -> Result<Self, KnowledgeOpenError> {
        if layout.worktree_id() != target.worktree().id()
            || source_worktree_id == target.worktree().id()
        {
            return Err(KnowledgeOpenError::IdentityConflict);
        }
        let stored = preflight_reconciliation_identity(
            layout,
            source_repository_id,
            source_worktree_id,
            target,
        )
        .await?;
        let source = (source_repository_id, source_worktree_id);

        let database = libsql::Builder::new_local(layout.knowledge_path())
            .build()
            .await
            .map_err(classify_open_error)?;
        let connection = database.connect().map_err(classify_connect_error)?;
        reject_newer_schema(&connection).await?;
        verify_integrity(&connection).await?;
        configure_connection(&connection)
            .await
            .map_err(classify_configuration_error)?;
        verify_connection_policy(&connection).await?;
        let schema_version =
            migrate_knowledge(&connection, stored.0.as_bytes(), stored.1.as_bytes())
                .await
                .map_err(classify_migration_error)?;
        if stored == source {
            rewrite_identity(
                &connection,
                source_repository_id,
                source_worktree_id,
                target.repository().id(),
                target.worktree().id(),
            )
            .await?;
        }
        verify_identity(&connection, target).await?;
        layout
            .validate_knowledge_target()
            .map_err(KnowledgeOpenError::Layout)?;

        let knowledge = Self {
            _database: database,
            connection,
            path: layout.knowledge_path().to_path_buf(),
            schema_version,
            repository_id: target.repository().id(),
            worktree_id: target.worktree().id(),
        };
        knowledge.verify().await?;
        Ok(knowledge)
    }

    pub(crate) async fn preflight_reconciliation(
        layout: &ProjectStorageLayout,
        source_repository_id: RepositoryId,
        source_worktree_id: WorktreeId,
        target: &ProjectIdentity,
    ) -> Result<(), KnowledgeOpenError> {
        preflight_reconciliation_identity(layout, source_repository_id, source_worktree_id, target)
            .await
            .map(|_| ())
    }

    /// Returns the validated database path in this worktree's private storage directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the schema version verified when the database was opened.
    #[must_use]
    pub const fn schema_version(&self) -> KnowledgeSchemaVersion {
        self.schema_version
    }

    pub(crate) const fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) const fn repository_id(&self) -> RepositoryId {
        self.repository_id
    }

    pub(crate) const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Re-runs connection, integrity, migration-history, schema, and identity checks.
    pub async fn verify(&self) -> Result<KnowledgeVerification, KnowledgeOpenError> {
        verify_connection_policy(&self.connection).await?;
        verify_integrity(&self.connection).await?;
        let found = read_user_version(&self.connection)
            .await
            .map(KnowledgeSchemaVersion::new)
            .map_err(classify_schema_inspection_error)?;
        if found != self.schema_version {
            return Err(KnowledgeOpenError::UnexpectedSchemaVersion {
                expected: self.schema_version,
                found,
            });
        }
        let verified = migrate_knowledge(
            &self.connection,
            self.repository_id.as_bytes(),
            self.worktree_id.as_bytes(),
        )
        .await
        .map_err(classify_migration_error)?;
        if verified != self.schema_version {
            return Err(KnowledgeOpenError::UnexpectedSchemaVersion {
                expected: self.schema_version,
                found: verified,
            });
        }
        verify_legacy_identity(&self.connection, self.repository_id, self.worktree_id).await?;
        verify_project_repository_identity(&self.connection, self.repository_id, self.worktree_id)
            .await?;
        Ok(KnowledgeVerification {
            schema_version: found,
            repository_id: self.repository_id,
            worktree_id: self.worktree_id,
        })
    }
}

async fn preflight_reconciliation_identity(
    layout: &ProjectStorageLayout,
    source_repository_id: RepositoryId,
    source_worktree_id: WorktreeId,
    target: &ProjectIdentity,
) -> Result<(RepositoryId, WorktreeId), KnowledgeOpenError> {
    layout
        .validate_knowledge_target()
        .map_err(KnowledgeOpenError::Layout)?;
    if !layout.knowledge_path().exists() {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }

    let database = libsql::Builder::new_local(layout.knowledge_path())
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .map_err(classify_open_error)?;
    let connection = database.connect().map_err(classify_connect_error)?;
    let version = reject_newer_schema(&connection).await?;
    verify_integrity(&connection).await?;
    if version == 0 {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    verify_knowledge_migration_history(&connection, version)
        .await
        .map_err(classify_migration_error)?;
    let stored = read_legacy_identity(&connection).await?;
    if version >= 2 && read_project_repository_identity(&connection).await? != stored {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    let source = (source_repository_id, source_worktree_id);
    let target_identity = (target.repository().id(), target.worktree().id());
    if stored != source && stored != target_identity {
        return Err(KnowledgeOpenError::IdentityConflict);
    }
    Ok(stored)
}

impl fmt::Debug for KnowledgeDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KnowledgeDatabase")
            .field("path", &self.path)
            .field("schema_version", &self.schema_version)
            .field("repository_id", &self.repository_id)
            .field("worktree_id", &self.worktree_id)
            .finish_non_exhaustive()
    }
}

/// Result of explicitly verifying one worktree knowledge database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeVerification {
    schema_version: KnowledgeSchemaVersion,
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
}

impl KnowledgeVerification {
    /// Returns the verified worktree schema version.
    #[must_use]
    pub const fn schema_version(self) -> KnowledgeSchemaVersion {
        self.schema_version
    }

    /// Returns the verified repository binding.
    #[must_use]
    pub const fn repository_id(self) -> RepositoryId {
        self.repository_id
    }

    /// Returns the verified worktree binding.
    #[must_use]
    pub const fn worktree_id(self) -> WorktreeId {
        self.worktree_id
    }
}

async fn preflight_existing_knowledge(
    path: &Path,
    project: &ProjectIdentity,
) -> Result<(), KnowledgeOpenError> {
    let database = libsql::Builder::new_local(path)
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .map_err(classify_open_error)?;
    let connection = database.connect().map_err(classify_connect_error)?;
    let version = reject_newer_schema(&connection).await?;
    verify_integrity(&connection).await?;
    if version >= 1 {
        verify_legacy_identity(
            &connection,
            project.repository().id(),
            project.worktree().id(),
        )
        .await?;
    }
    if version >= 2 {
        verify_project_repository_identity(
            &connection,
            project.repository().id(),
            project.worktree().id(),
        )
        .await?;
    }
    Ok(())
}

async fn reject_newer_schema(connection: &Connection) -> Result<u32, KnowledgeOpenError> {
    let found = read_user_version(connection)
        .await
        .map_err(classify_schema_inspection_error)?;
    if found > KnowledgeSchemaVersion::CURRENT.get() {
        return Err(KnowledgeOpenError::NewerSchema {
            found: KnowledgeSchemaVersion::new(found),
            supported: KnowledgeSchemaVersion::CURRENT,
        });
    }
    Ok(found)
}

async fn verify_connection_policy(connection: &Connection) -> Result<(), KnowledgeOpenError> {
    if !connection_policy_is_valid(connection)
        .await
        .map_err(classify_policy_inspection_error)?
    {
        return Err(KnowledgeOpenError::ConnectionPolicyMismatch);
    }
    Ok(())
}

async fn verify_integrity(connection: &Connection) -> Result<(), KnowledgeOpenError> {
    if !integrity_is_valid(connection)
        .await
        .map_err(classify_integrity_error)?
    {
        return Err(KnowledgeOpenError::IntegrityCheckFailed);
    }
    Ok(())
}

async fn verify_identity(
    connection: &Connection,
    project: &ProjectIdentity,
) -> Result<(), KnowledgeOpenError> {
    verify_legacy_identity(
        connection,
        project.repository().id(),
        project.worktree().id(),
    )
    .await?;
    verify_project_repository_identity(
        connection,
        project.repository().id(),
        project.worktree().id(),
    )
    .await
}

async fn verify_legacy_identity(
    connection: &Connection,
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
) -> Result<(), KnowledgeOpenError> {
    let (stored_repository_id, stored_worktree_id) = read_legacy_identity(connection).await?;
    if stored_repository_id != repository_id || stored_worktree_id != worktree_id {
        return Err(KnowledgeOpenError::IdentityConflict);
    }
    Ok(())
}

async fn read_legacy_identity(
    connection: &Connection,
) -> Result<(RepositoryId, WorktreeId), KnowledgeOpenError> {
    let mut rows = connection
        .query(
            "SELECT repository_id, worktree_id FROM worktree_storage_identity\n\
             WHERE singleton = 1",
            (),
        )
        .await
        .map_err(classify_identity_read_error)?;
    let row = rows
        .next()
        .await
        .map_err(classify_identity_read_error)?
        .ok_or(KnowledgeOpenError::InvalidStoredData)?;
    let stored_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 0)?);
    let stored_worktree_id = WorktreeId::from_bytes(read_stable_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(classify_identity_read_error)?
        .is_some()
    {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    Ok((stored_repository_id, stored_worktree_id))
}

async fn verify_project_repository_identity(
    connection: &Connection,
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
) -> Result<(), KnowledgeOpenError> {
    let (stored_repository_id, stored_worktree_id) =
        read_project_repository_identity(connection).await?;
    if stored_repository_id != repository_id || stored_worktree_id != worktree_id {
        return Err(KnowledgeOpenError::IdentityConflict);
    }
    Ok(())
}

async fn read_project_repository_identity(
    connection: &Connection,
) -> Result<(RepositoryId, WorktreeId), KnowledgeOpenError> {
    let mut rows = connection
        .query(
            "SELECT\n\
             (SELECT COUNT(*) FROM repositories),\n\
             (SELECT COUNT(*) FROM worktrees),\n\
             repositories.repository_id, worktrees.worktree_id, worktrees.repository_id\n\
             FROM repositories\n\
             JOIN worktrees ON worktrees.repository_id = repositories.repository_id",
            (),
        )
        .await
        .map_err(classify_identity_read_error)?;
    let row = rows
        .next()
        .await
        .map_err(classify_identity_read_error)?
        .ok_or(KnowledgeOpenError::InvalidStoredData)?;
    let repository_count: i64 = row.get(0).map_err(classify_identity_read_error)?;
    let worktree_count: i64 = row.get(1).map_err(classify_identity_read_error)?;
    let stored_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 2)?);
    let stored_worktree_id = WorktreeId::from_bytes(read_stable_id(&row, 3)?);
    let worktree_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 4)?);
    if repository_count != 1 || worktree_count != 1 {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    if worktree_repository_id != stored_repository_id {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    if rows
        .next()
        .await
        .map_err(classify_identity_read_error)?
        .is_some()
    {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    Ok((stored_repository_id, stored_worktree_id))
}

async fn rewrite_identity(
    connection: &Connection,
    source_repository_id: RepositoryId,
    source_worktree_id: WorktreeId,
    target_repository_id: RepositoryId,
    target_worktree_id: WorktreeId,
) -> Result<(), KnowledgeOpenError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(KnowledgeOpenError::BeginReconciliation)?;
    transaction
        .execute("PRAGMA defer_foreign_keys = ON", ())
        .await
        .map_err(KnowledgeOpenError::WriteReconciliation)?;

    let result = rewrite_identity_in_transaction(
        &transaction,
        source_repository_id,
        source_worktree_id,
        target_repository_id,
        target_worktree_id,
    )
    .await;
    if let Err(error) = result {
        return match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(KnowledgeOpenError::RollbackReconciliation(source)),
        };
    }
    transaction
        .commit()
        .await
        .map_err(KnowledgeOpenError::CommitReconciliation)
}

async fn rewrite_identity_in_transaction(
    transaction: &libsql::Transaction,
    source_repository_id: RepositoryId,
    source_worktree_id: WorktreeId,
    target_repository_id: RepositoryId,
    target_worktree_id: WorktreeId,
) -> Result<(), KnowledgeOpenError> {
    if source_repository_id != target_repository_id {
        let affected = transaction
            .execute(
                "UPDATE repositories SET repository_id = ?1 WHERE repository_id = ?2",
                params![
                    target_repository_id.as_bytes().to_vec(),
                    source_repository_id.as_bytes().to_vec()
                ],
            )
            .await
            .map_err(KnowledgeOpenError::WriteReconciliation)?;
        if affected != 1 {
            return Err(KnowledgeOpenError::InvalidStoredData);
        }
    }

    let affected = transaction
        .execute(
            "UPDATE worktrees SET worktree_id = ?1\n\
             WHERE worktree_id = ?2 AND repository_id = ?3",
            params![
                target_worktree_id.as_bytes().to_vec(),
                source_worktree_id.as_bytes().to_vec(),
                target_repository_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(KnowledgeOpenError::WriteReconciliation)?;
    if affected != 1 {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }

    let affected = transaction
        .execute(
            "UPDATE worktree_storage_identity SET repository_id = ?1, worktree_id = ?2\n\
             WHERE singleton = 1 AND repository_id = ?3 AND worktree_id = ?4",
            params![
                target_repository_id.as_bytes().to_vec(),
                target_worktree_id.as_bytes().to_vec(),
                source_repository_id.as_bytes().to_vec(),
                source_worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(KnowledgeOpenError::WriteReconciliation)?;
    if affected != 1 {
        return Err(KnowledgeOpenError::InvalidStoredData);
    }
    Ok(())
}

fn read_stable_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], KnowledgeOpenError> {
    let bytes: Vec<u8> = row.get(index).map_err(classify_identity_read_error)?;
    bytes
        .try_into()
        .map_err(|_| KnowledgeOpenError::InvalidStoredData)
}

fn classify_open_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::Open(source)
    }
}

fn classify_connect_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::Connect(source)
    }
}

fn classify_configuration_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::Configure(source)
    }
}

fn classify_policy_inspection_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::InspectConnectionPolicy(source)
    }
}

fn classify_integrity_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::InspectIntegrity(source)
    }
}

fn classify_schema_inspection_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::InspectSchema(source)
    }
}

fn classify_identity_read_error(source: libsql::Error) -> KnowledgeOpenError {
    if is_corruption(&source) {
        KnowledgeOpenError::CorruptDatabase
    } else {
        KnowledgeOpenError::InspectIdentity(source)
    }
}

fn classify_migration_error(error: MigrationError) -> KnowledgeOpenError {
    match error {
        MigrationError::ReadVersion(source) => classify_schema_inspection_error(source),
        MigrationError::NewerSchema { current, supported } => KnowledgeOpenError::NewerSchema {
            found: KnowledgeSchemaVersion::new(current),
            supported: KnowledgeSchemaVersion::new(supported),
        },
        MigrationError::ReadHistory(source) if is_corruption(&source) => {
            KnowledgeOpenError::CorruptDatabase
        }
        MigrationError::ReadHistory(source) => KnowledgeOpenError::InspectMigrationHistory(source),
        MigrationError::HistoryMismatch { version } => {
            KnowledgeOpenError::MigrationHistoryMismatch {
                version: KnowledgeSchemaVersion::new(version),
            }
        }
        MigrationError::Begin { version, source } => KnowledgeOpenError::BeginMigration {
            version: KnowledgeSchemaVersion::new(version),
            source,
        },
        MigrationError::Apply { version, source } => KnowledgeOpenError::ApplyMigration {
            version: KnowledgeSchemaVersion::new(version),
            source,
        },
        MigrationError::Rollback { version, source } => KnowledgeOpenError::RollbackMigration {
            version: KnowledgeSchemaVersion::new(version),
            source,
        },
        MigrationError::Commit { version, source } => KnowledgeOpenError::CommitMigration {
            version: KnowledgeSchemaVersion::new(version),
            source,
        },
    }
}

/// Failure to establish or verify a worktree knowledge database boundary.
#[derive(Debug)]
pub enum KnowledgeOpenError {
    /// The per-worktree storage path failed boundary validation.
    Layout(ProjectStorageLayoutError),
    /// libSQL could not construct the local-only database.
    Open(libsql::Error),
    /// libSQL could not create a connection.
    Connect(libsql::Error),
    /// Required safety pragmas could not be applied.
    Configure(libsql::Error),
    /// Required safety pragmas could not be inspected.
    InspectConnectionPolicy(libsql::Error),
    /// The connection did not retain every required safety setting.
    ConnectionPolicyMismatch,
    /// The integrity check could not be executed.
    InspectIntegrity(libsql::Error),
    /// The integrity check reported damaged content.
    IntegrityCheckFailed,
    /// SQLite identified the file as corrupt or not a database.
    CorruptDatabase,
    /// The persisted schema version could not be inspected.
    InspectSchema(libsql::Error),
    /// The database was written by a newer unsupported A^3 version.
    NewerSchema {
        /// Version stored by the database.
        found: KnowledgeSchemaVersion,
        /// Highest version understood by this build.
        supported: KnowledgeSchemaVersion,
    },
    /// The migration journal could not be inspected.
    InspectMigrationHistory(libsql::Error),
    /// A persisted migration name or checksum does not match the binary.
    MigrationHistoryMismatch {
        /// First inconsistent migration version.
        version: KnowledgeSchemaVersion,
    },
    /// An immediate migration transaction could not begin.
    BeginMigration {
        /// Migration version that was starting.
        version: KnowledgeSchemaVersion,
        /// libSQL failure.
        source: libsql::Error,
    },
    /// A migration statement failed and its transaction was rolled back.
    ApplyMigration {
        /// Failed migration version.
        version: KnowledgeSchemaVersion,
        /// libSQL failure.
        source: libsql::Error,
    },
    /// A failed migration transaction could not be rolled back.
    RollbackMigration {
        /// Failed migration version.
        version: KnowledgeSchemaVersion,
        /// libSQL rollback failure.
        source: libsql::Error,
    },
    /// A completed migration transaction could not be committed.
    CommitMigration {
        /// Failed migration version.
        version: KnowledgeSchemaVersion,
        /// libSQL commit failure.
        source: libsql::Error,
    },
    /// The stored repository/worktree binding could not be inspected.
    InspectIdentity(libsql::Error),
    /// Durable rows violated the versioned logical schema.
    InvalidStoredData,
    /// The requested or stored worktree identity conflicts with this database path.
    IdentityConflict,
    /// An immediate identity-reconciliation transaction could not begin.
    BeginReconciliation(libsql::Error),
    /// A confirmed identity rewrite failed before commit.
    WriteReconciliation(libsql::Error),
    /// A failed identity rewrite could not be rolled back.
    RollbackReconciliation(libsql::Error),
    /// A completed identity rewrite could not be committed.
    CommitReconciliation(libsql::Error),
    /// The schema version changed after the database was opened.
    UnexpectedSchemaVersion {
        /// Version verified during open.
        expected: KnowledgeSchemaVersion,
        /// Version found during explicit verification.
        found: KnowledgeSchemaVersion,
    },
}

impl fmt::Display for KnowledgeOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(_) => formatter.write_str("worktree storage path validation failed"),
            Self::Open(_) => formatter.write_str("could not open the worktree knowledge database"),
            Self::Connect(_) => {
                formatter.write_str("could not connect to the worktree knowledge database")
            }
            Self::Configure(_) => {
                formatter.write_str("could not configure the knowledge connection")
            }
            Self::InspectConnectionPolicy(_) => {
                formatter.write_str("could not inspect the knowledge connection policy")
            }
            Self::ConnectionPolicyMismatch => {
                formatter.write_str("knowledge connection policy verification failed")
            }
            Self::InspectIntegrity(_) => {
                formatter.write_str("could not inspect knowledge integrity")
            }
            Self::IntegrityCheckFailed | Self::CorruptDatabase => {
                formatter.write_str("worktree knowledge database is corrupt")
            }
            Self::InspectSchema(_) => {
                formatter.write_str("could not inspect knowledge schema version")
            }
            Self::NewerSchema { found, supported } => write!(
                formatter,
                "knowledge schema version {} is newer than supported version {}",
                found.get(),
                supported.get()
            ),
            Self::InspectMigrationHistory(_) => {
                formatter.write_str("could not inspect knowledge migration history")
            }
            Self::MigrationHistoryMismatch { version } => write!(
                formatter,
                "knowledge migration history differs at version {}",
                version.get()
            ),
            Self::BeginMigration { version, .. } => {
                write!(
                    formatter,
                    "could not begin knowledge migration {}",
                    version.get()
                )
            }
            Self::ApplyMigration { version, .. } => {
                write!(formatter, "knowledge migration {} failed", version.get())
            }
            Self::RollbackMigration { version, .. } => write!(
                formatter,
                "could not roll back knowledge migration {}",
                version.get()
            ),
            Self::CommitMigration { version, .. } => {
                write!(
                    formatter,
                    "could not commit knowledge migration {}",
                    version.get()
                )
            }
            Self::InspectIdentity(_) => formatter.write_str("could not inspect worktree identity"),
            Self::InvalidStoredData => formatter.write_str("knowledge data is invalid"),
            Self::IdentityConflict => formatter.write_str("knowledge identity conflicts"),
            Self::BeginReconciliation(_) => {
                formatter.write_str("could not begin knowledge identity reconciliation")
            }
            Self::WriteReconciliation(_) => {
                formatter.write_str("could not rewrite knowledge identity")
            }
            Self::RollbackReconciliation(_) => {
                formatter.write_str("could not roll back knowledge identity reconciliation")
            }
            Self::CommitReconciliation(_) => {
                formatter.write_str("could not commit knowledge identity reconciliation")
            }
            Self::UnexpectedSchemaVersion { expected, found } => write!(
                formatter,
                "knowledge schema changed from version {} to {}",
                expected.get(),
                found.get()
            ),
        }
    }
}

impl Error for KnowledgeOpenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Layout(source) => Some(source),
            Self::Open(source)
            | Self::Connect(source)
            | Self::Configure(source)
            | Self::InspectConnectionPolicy(source)
            | Self::InspectIntegrity(source)
            | Self::InspectSchema(source)
            | Self::InspectMigrationHistory(source)
            | Self::BeginMigration { source, .. }
            | Self::ApplyMigration { source, .. }
            | Self::RollbackMigration { source, .. }
            | Self::CommitMigration { source, .. }
            | Self::InspectIdentity(source)
            | Self::BeginReconciliation(source)
            | Self::WriteReconciliation(source)
            | Self::RollbackReconciliation(source)
            | Self::CommitReconciliation(source) => Some(source),
            Self::ConnectionPolicyMismatch
            | Self::IntegrityCheckFailed
            | Self::CorruptDatabase
            | Self::NewerSchema { .. }
            | Self::MigrationHistoryMismatch { .. }
            | Self::InvalidStoredData
            | Self::IdentityConflict
            | Self::UnexpectedSchemaVersion { .. } => None,
        }
    }
}
