use crate::layout::{StorageLayout, StorageLayoutError};
use crate::migration::{
    CatalogSchemaVersion, MigrationError, migrate_catalog, query_i64, query_string,
    read_user_version,
};
use libsql::{Connection, Database};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

const SQLITE_CORRUPT: i32 = 11;
const SQLITE_NOT_A_DATABASE: i32 = 26;
const BUSY_TIMEOUT_MILLISECONDS: i64 = 5_000;

/// An opened and policy-checked global A^3 catalog database.
///
/// The adapter deliberately keeps its libSQL handles private so persistence
/// rows and unrestricted SQL cannot escape the infrastructure boundary.
pub struct CatalogDatabase {
    database: Database,
    connection: Connection,
    path: PathBuf,
    schema_version: CatalogSchemaVersion,
}

impl CatalogDatabase {
    /// Opens the local catalog, applies pending migrations atomically, and verifies it.
    pub async fn open(layout: &StorageLayout) -> Result<Self, CatalogOpenError> {
        layout
            .validate_catalog_target()
            .map_err(CatalogOpenError::Layout)?;
        if layout.catalog_path().exists() {
            preflight_existing_catalog(layout.catalog_path()).await?;
        }

        let database = libsql::Builder::new_local(layout.catalog_path())
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
        let schema_version = migrate_catalog(&connection)
            .await
            .map_err(classify_migration_error)?;

        layout
            .validate_catalog_target()
            .map_err(CatalogOpenError::Layout)?;

        let catalog = Self {
            database,
            connection,
            path: layout.catalog_path().to_path_buf(),
            schema_version,
        };
        catalog.verify().await?;
        Ok(catalog)
    }

    /// Returns the validated catalog path inside the application-data root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the schema version verified when the catalog was opened.
    #[must_use]
    pub const fn schema_version(&self) -> CatalogSchemaVersion {
        self.schema_version
    }

    /// Re-runs the connection-policy, integrity, and schema-version checks.
    pub async fn verify(&self) -> Result<CatalogVerification, CatalogOpenError> {
        verify_connection_policy(&self.connection).await?;
        verify_integrity(&self.connection).await?;
        let found = read_user_version(&self.connection)
            .await
            .map_err(classify_schema_inspection_error)?;
        if found != self.schema_version {
            return Err(CatalogOpenError::UnexpectedSchemaVersion {
                expected: self.schema_version,
                found,
            });
        }
        Ok(CatalogVerification {
            schema_version: found,
        })
    }

    pub(crate) async fn connection_for_operation(&self) -> Result<Connection, CatalogOpenError> {
        let connection = self.database.connect().map_err(classify_connect_error)?;
        configure_connection(&connection)
            .await
            .map_err(classify_configuration_error)?;
        verify_connection_policy(&connection).await?;
        verify_integrity(&connection).await?;
        Ok(connection)
    }
}

async fn preflight_existing_catalog(path: &Path) -> Result<(), CatalogOpenError> {
    let database = libsql::Builder::new_local(path)
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .map_err(classify_open_error)?;
    let connection = database.connect().map_err(classify_connect_error)?;
    reject_newer_schema(&connection).await?;
    verify_integrity(&connection).await
}

async fn reject_newer_schema(connection: &Connection) -> Result<(), CatalogOpenError> {
    let found = read_user_version(connection)
        .await
        .map_err(classify_schema_inspection_error)?;
    if found > CatalogSchemaVersion::CURRENT {
        return Err(CatalogOpenError::NewerSchema {
            found,
            supported: CatalogSchemaVersion::CURRENT,
        });
    }
    Ok(())
}

impl fmt::Debug for CatalogDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogDatabase")
            .field("path", &self.path)
            .field("schema_version", &self.schema_version)
            .finish_non_exhaustive()
    }
}

/// Result of explicitly verifying an open catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogVerification {
    schema_version: CatalogSchemaVersion,
}

impl CatalogVerification {
    /// Returns the schema version observed during verification.
    #[must_use]
    pub const fn schema_version(self) -> CatalogSchemaVersion {
        self.schema_version
    }
}

async fn configure_connection(connection: &Connection) -> libsql::Result<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;\n\
             PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;\n\
             PRAGMA busy_timeout = 5000;\n\
             PRAGMA trusted_schema = OFF;",
        )
        .await
        .map(|_| ())
}

async fn verify_connection_policy(connection: &Connection) -> Result<(), CatalogOpenError> {
    let foreign_keys = query_i64(connection, "PRAGMA foreign_keys")
        .await
        .map_err(classify_policy_inspection_error)?;
    let journal_mode = query_string(connection, "PRAGMA journal_mode")
        .await
        .map_err(classify_policy_inspection_error)?;
    let synchronous = query_i64(connection, "PRAGMA synchronous")
        .await
        .map_err(classify_policy_inspection_error)?;
    let busy_timeout = query_i64(connection, "PRAGMA busy_timeout")
        .await
        .map_err(classify_policy_inspection_error)?;
    let trusted_schema = query_i64(connection, "PRAGMA trusted_schema")
        .await
        .map_err(classify_policy_inspection_error)?;

    if foreign_keys != 1
        || !journal_mode.eq_ignore_ascii_case("wal")
        || synchronous != 1
        || busy_timeout != BUSY_TIMEOUT_MILLISECONDS
        || trusted_schema != 0
    {
        return Err(CatalogOpenError::ConnectionPolicyMismatch);
    }
    Ok(())
}

async fn verify_integrity(connection: &Connection) -> Result<(), CatalogOpenError> {
    let result = query_string(connection, "PRAGMA quick_check(1)")
        .await
        .map_err(classify_integrity_error)?;
    if result != "ok" {
        return Err(CatalogOpenError::IntegrityCheckFailed);
    }
    Ok(())
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn is_corruption(error: &libsql::Error) -> bool {
    matches!(
        sqlite_primary_code(error),
        Some(SQLITE_CORRUPT | SQLITE_NOT_A_DATABASE)
    )
}

fn classify_open_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::Open(source)
    }
}

fn classify_connect_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::Connect(source)
    }
}

fn classify_configuration_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::Configure(source)
    }
}

fn classify_policy_inspection_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::InspectConnectionPolicy(source)
    }
}

fn classify_integrity_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::InspectIntegrity(source)
    }
}

fn classify_schema_inspection_error(source: libsql::Error) -> CatalogOpenError {
    if is_corruption(&source) {
        CatalogOpenError::CorruptDatabase
    } else {
        CatalogOpenError::InspectSchema(source)
    }
}

fn classify_migration_error(error: MigrationError) -> CatalogOpenError {
    match error {
        MigrationError::ReadVersion(source) => classify_schema_inspection_error(source),
        MigrationError::NewerSchema { current, supported } => CatalogOpenError::NewerSchema {
            found: current,
            supported,
        },
        MigrationError::ReadHistory(source) if is_corruption(&source) => {
            CatalogOpenError::CorruptDatabase
        }
        MigrationError::ReadHistory(source) => CatalogOpenError::InspectMigrationHistory(source),
        MigrationError::HistoryMismatch { version } => {
            CatalogOpenError::MigrationHistoryMismatch { version }
        }
        MigrationError::Begin { version, source } => {
            CatalogOpenError::BeginMigration { version, source }
        }
        MigrationError::Apply { version, source } => {
            CatalogOpenError::ApplyMigration { version, source }
        }
        MigrationError::Rollback { version, source } => {
            CatalogOpenError::RollbackMigration { version, source }
        }
        MigrationError::Commit { version, source } => {
            CatalogOpenError::CommitMigration { version, source }
        }
    }
}

/// Failure to establish or verify the local catalog boundary.
#[derive(Debug)]
pub enum CatalogOpenError {
    /// The storage path failed boundary validation.
    Layout(StorageLayoutError),
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
    /// The catalog was written by a newer, unsupported A^3 version.
    NewerSchema {
        /// Version stored by the database.
        found: CatalogSchemaVersion,
        /// Highest version understood by this build.
        supported: CatalogSchemaVersion,
    },
    /// The migration journal could not be inspected.
    InspectMigrationHistory(libsql::Error),
    /// A persisted migration name or checksum does not match the binary.
    MigrationHistoryMismatch {
        /// First inconsistent migration version.
        version: CatalogSchemaVersion,
    },
    /// An immediate migration transaction could not begin.
    BeginMigration {
        /// Migration version that was starting.
        version: CatalogSchemaVersion,
        /// libSQL failure.
        source: libsql::Error,
    },
    /// A migration statement failed and its transaction was rolled back.
    ApplyMigration {
        /// Failed migration version.
        version: CatalogSchemaVersion,
        /// libSQL failure.
        source: libsql::Error,
    },
    /// A failed migration transaction could not be rolled back.
    RollbackMigration {
        /// Failed migration version.
        version: CatalogSchemaVersion,
        /// libSQL failure while rolling back.
        source: libsql::Error,
    },
    /// A completed migration transaction could not be committed.
    CommitMigration {
        /// Failed migration version.
        version: CatalogSchemaVersion,
        /// libSQL failure while committing.
        source: libsql::Error,
    },
    /// The schema version changed after the catalog was opened.
    UnexpectedSchemaVersion {
        /// Version verified during open.
        expected: CatalogSchemaVersion,
        /// Version found during explicit verification.
        found: CatalogSchemaVersion,
    },
}

impl fmt::Display for CatalogOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(_) => formatter.write_str("catalog path validation failed"),
            Self::Open(_) => formatter.write_str("could not open the local catalog database"),
            Self::Connect(_) => {
                formatter.write_str("could not connect to the local catalog database")
            }
            Self::Configure(_) => formatter.write_str("could not configure the catalog connection"),
            Self::InspectConnectionPolicy(_) => {
                formatter.write_str("could not inspect the catalog connection policy")
            }
            Self::ConnectionPolicyMismatch => {
                formatter.write_str("catalog connection policy verification failed")
            }
            Self::InspectIntegrity(_) => formatter.write_str("could not inspect catalog integrity"),
            Self::IntegrityCheckFailed | Self::CorruptDatabase => {
                formatter.write_str("catalog database is corrupt")
            }
            Self::InspectSchema(_) => {
                formatter.write_str("could not inspect catalog schema version")
            }
            Self::NewerSchema { found, supported } => write!(
                formatter,
                "catalog schema version {} is newer than supported version {}",
                found.get(),
                supported.get()
            ),
            Self::InspectMigrationHistory(_) => {
                formatter.write_str("could not inspect catalog migration history")
            }
            Self::MigrationHistoryMismatch { version } => write!(
                formatter,
                "catalog migration history differs at version {}",
                version.get()
            ),
            Self::BeginMigration { version, .. } => {
                write!(
                    formatter,
                    "could not begin catalog migration {}",
                    version.get()
                )
            }
            Self::ApplyMigration { version, .. } => {
                write!(formatter, "catalog migration {} failed", version.get())
            }
            Self::RollbackMigration { version, .. } => write!(
                formatter,
                "could not roll back catalog migration {}",
                version.get()
            ),
            Self::CommitMigration { version, .. } => {
                write!(
                    formatter,
                    "could not commit catalog migration {}",
                    version.get()
                )
            }
            Self::UnexpectedSchemaVersion { expected, found } => write!(
                formatter,
                "catalog schema changed from version {} to {}",
                expected.get(),
                found.get()
            ),
        }
    }
}

impl Error for CatalogOpenError {
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
            | Self::CommitMigration { source, .. } => Some(source),
            Self::ConnectionPolicyMismatch
            | Self::IntegrityCheckFailed
            | Self::CorruptDatabase
            | Self::NewerSchema { .. }
            | Self::MigrationHistoryMismatch { .. }
            | Self::UnexpectedSchemaVersion { .. } => None,
        }
    }
}
