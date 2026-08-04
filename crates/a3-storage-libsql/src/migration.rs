use blake3::Hasher;
use libsql::{Connection, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const CATALOG_MIGRATIONS: &[Migration] = &[
    Migration {
        version: CatalogSchemaVersion::new(1),
        name: "bootstrap_catalog",
        sql: "CREATE TABLE schema_migrations (\n\
          version INTEGER PRIMARY KEY CHECK (version > 0),\n\
          name TEXT NOT NULL UNIQUE,\n\
          checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
          ) STRICT;",
    },
    Migration {
        version: CatalogSchemaVersion::new(2),
        name: "project_catalog",
        sql: "CREATE TABLE projects (\n\
          project_id BLOB PRIMARY KEY NOT NULL CHECK (length(project_id) = 32),\n\
          repository_id BLOB NOT NULL UNIQUE CHECK (length(repository_id) = 32),\n\
          repository_common_directory BLOB NOT NULL\n\
            CHECK (length(repository_common_directory) BETWEEN 1 AND 131072),\n\
          repository_path_encoding TEXT NOT NULL\n\
            CHECK (repository_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          main_remote_id BLOB CHECK (main_remote_id IS NULL OR length(main_remote_id) = 32),\n\
          created_open_sequence INTEGER NOT NULL UNIQUE CHECK (created_open_sequence > 0),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE\n\
            CHECK (last_open_sequence >= created_open_sequence),\n\
          UNIQUE (project_id, repository_id)\n\
          ) STRICT;\n\
          CREATE INDEX projects_main_remote_id_idx\n\
            ON projects (main_remote_id) WHERE main_remote_id IS NOT NULL;\n\
          CREATE TABLE recent_worktrees (\n\
          worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
          project_id BLOB NOT NULL CHECK (length(project_id) = 32),\n\
          repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
          worktree_root BLOB NOT NULL CHECK (length(worktree_root) BETWEEN 1 AND 131072),\n\
          worktree_path_encoding TEXT NOT NULL\n\
            CHECK (worktree_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          worktree_root_display TEXT NOT NULL\n\
            CHECK (length(worktree_root_display) BETWEEN 1 AND 32768),\n\
          head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
          head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
          head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE CHECK (last_open_sequence > 0),\n\
          CHECK (\n\
            (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
            (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
          ),\n\
          FOREIGN KEY (project_id, repository_id)\n\
            REFERENCES projects(project_id, repository_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          CREATE INDEX recent_worktrees_project_id_idx\n\
            ON recent_worktrees (project_id);",
    },
];

/// Monotone version of the global catalog schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogSchemaVersion(u32);

impl CatalogSchemaVersion {
    /// Current schema version understood by this build.
    pub const CURRENT: Self = Self::new(2);

    /// Creates a schema version from a migration number.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Migration {
    version: CatalogSchemaVersion,
    name: &'static str,
    sql: &'static str,
}

pub(crate) async fn migrate_catalog(
    connection: &Connection,
) -> Result<CatalogSchemaVersion, MigrationError> {
    migrate(
        connection,
        CATALOG_MIGRATIONS,
        CatalogSchemaVersion::CURRENT,
    )
    .await
}

async fn migrate(
    connection: &Connection,
    migrations: &[Migration],
    supported: CatalogSchemaVersion,
) -> Result<CatalogSchemaVersion, MigrationError> {
    let current = read_user_version(connection)
        .await
        .map_err(MigrationError::ReadVersion)?;
    if current > supported {
        return Err(MigrationError::NewerSchema { current, supported });
    }
    verify_history(connection, migrations, current).await?;

    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current)
    {
        apply_migration(connection, migration).await?;
    }

    verify_history(connection, migrations, supported).await?;
    Ok(supported)
}

async fn apply_migration(
    connection: &Connection,
    migration: &Migration,
) -> Result<(), MigrationError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|source| MigrationError::Begin {
            version: migration.version,
            source,
        })?;

    if let Err(source) = apply_migration_body(&transaction, migration).await {
        return match transaction.rollback().await {
            Ok(()) => Err(MigrationError::Apply {
                version: migration.version,
                source,
            }),
            Err(source) => Err(MigrationError::Rollback {
                version: migration.version,
                source,
            }),
        };
    }

    transaction
        .commit()
        .await
        .map_err(|source| MigrationError::Commit {
            version: migration.version,
            source,
        })
}

async fn apply_migration_body(
    transaction: &libsql::Transaction,
    migration: &Migration,
) -> libsql::Result<()> {
    transaction.execute_batch(migration.sql).await?;
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![
                i64::from(migration.version.get()),
                migration.name,
                migration_checksum(migration).to_vec()
            ],
        )
        .await?;
    transaction
        .execute_batch(&format!(
            "PRAGMA user_version = {}",
            migration.version.get()
        ))
        .await?;
    Ok(())
}

async fn verify_history(
    connection: &Connection,
    migrations: &[Migration],
    current: CatalogSchemaVersion,
) -> Result<(), MigrationError> {
    if current.get() == 0 {
        return Ok(());
    }

    let maximum = query_i64(
        connection,
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
    )
    .await
    .map_err(MigrationError::ReadHistory)?;
    if maximum != i64::from(current.get()) {
        return Err(MigrationError::HistoryMismatch { version: current });
    }

    for migration in migrations
        .iter()
        .filter(|migration| migration.version <= current)
    {
        let mut rows = connection
            .query(
                "SELECT name, checksum FROM schema_migrations WHERE version = ?1",
                [i64::from(migration.version.get())],
            )
            .await
            .map_err(MigrationError::ReadHistory)?;
        let row = rows
            .next()
            .await
            .map_err(MigrationError::ReadHistory)?
            .ok_or(MigrationError::HistoryMismatch {
                version: migration.version,
            })?;
        let name: String = row.get(0).map_err(MigrationError::ReadHistory)?;
        let checksum: Vec<u8> = row.get(1).map_err(MigrationError::ReadHistory)?;
        if name != migration.name || checksum.as_slice() != migration_checksum(migration) {
            return Err(MigrationError::HistoryMismatch {
                version: migration.version,
            });
        }
    }
    Ok(())
}

pub(crate) async fn read_user_version(
    connection: &Connection,
) -> libsql::Result<CatalogSchemaVersion> {
    let raw = query_i64(connection, "PRAGMA user_version").await?;
    let value = u32::try_from(raw).map_err(|_| libsql::Error::InvalidColumnType)?;
    Ok(CatalogSchemaVersion::new(value))
}

pub(crate) async fn query_i64(connection: &Connection, sql: &str) -> libsql::Result<i64> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    row.get(0)
}

pub(crate) async fn query_string(connection: &Connection, sql: &str) -> libsql::Result<String> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    row.get(0)
}

fn migration_checksum(migration: &Migration) -> [u8; 32] {
    let mut hasher = Hasher::new();
    update_checksum_field(&mut hasher, b"a3.catalog-migration.v1");
    update_checksum_field(&mut hasher, &migration.version.get().to_le_bytes());
    update_checksum_field(&mut hasher, migration.name.as_bytes());
    update_checksum_field(&mut hasher, migration.sql.as_bytes());
    *hasher.finalize().as_bytes()
}

fn update_checksum_field(hasher: &mut Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u128).to_le_bytes());
    hasher.update(value);
}

#[derive(Debug)]
pub(crate) enum MigrationError {
    ReadVersion(libsql::Error),
    NewerSchema {
        current: CatalogSchemaVersion,
        supported: CatalogSchemaVersion,
    },
    ReadHistory(libsql::Error),
    HistoryMismatch {
        version: CatalogSchemaVersion,
    },
    Begin {
        version: CatalogSchemaVersion,
        source: libsql::Error,
    },
    Apply {
        version: CatalogSchemaVersion,
        source: libsql::Error,
    },
    Rollback {
        version: CatalogSchemaVersion,
        source: libsql::Error,
    },
    Commit {
        version: CatalogSchemaVersion,
        source: libsql::Error,
    },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadVersion(_) => formatter.write_str("could not read catalog schema version"),
            Self::NewerSchema { current, supported } => write!(
                formatter,
                "catalog schema {} is newer than supported schema {}",
                current.get(),
                supported.get()
            ),
            Self::ReadHistory(_) => formatter.write_str("could not read migration history"),
            Self::HistoryMismatch { version } => write!(
                formatter,
                "migration history differs at schema {}",
                version.get()
            ),
            Self::Begin { version, .. } => {
                write!(formatter, "could not begin migration {}", version.get())
            }
            Self::Apply { version, .. } => {
                write!(formatter, "could not apply migration {}", version.get())
            }
            Self::Rollback { version, .. } => {
                write!(formatter, "could not roll back migration {}", version.get())
            }
            Self::Commit { version, .. } => {
                write!(formatter, "could not commit migration {}", version.get())
            }
        }
    }
}

impl Error for MigrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadVersion(source)
            | Self::ReadHistory(source)
            | Self::Begin { source, .. }
            | Self::Apply { source, .. }
            | Self::Rollback { source, .. }
            | Self::Commit { source, .. } => Some(source),
            Self::NewerSchema { .. } | Self::HistoryMismatch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CATALOG_MIGRATIONS, CatalogSchemaVersion, Migration, MigrationError, migrate, query_i64,
    };
    use futures::executor::block_on;
    use std::collections::HashSet;

    #[test]
    fn catalog_migration_definitions_are_contiguous_and_uniquely_named() {
        assert_eq!(
            CATALOG_MIGRATIONS.len(),
            CatalogSchemaVersion::CURRENT.get() as usize
        );
        let mut names = HashSet::new();
        for (index, migration) in CATALOG_MIGRATIONS.iter().enumerate() {
            assert_eq!(migration.version.get() as usize, index + 1);
            assert!(names.insert(migration.name));
            assert!(!migration.sql.trim().is_empty());
        }
    }

    #[test]
    fn catalog_upgrades_from_every_supported_predecessor() -> Result<(), Box<dyn std::error::Error>>
    {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;

            migrate(
                &connection,
                &CATALOG_MIGRATIONS[..1],
                CatalogSchemaVersion::new(1),
            )
            .await?;
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 1);

            let version = super::migrate_catalog(&connection).await?;
            assert_eq!(version, CatalogSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('projects', 'recent_worktrees')",
                )
                .await?,
                2
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let migrations = [Migration {
                version: CatalogSchemaVersion::new(1),
                name: "broken",
                sql: "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT, checksum BLOB);\n\
                      CREATE TABLE must_rollback (id INTEGER);\n\
                      THIS IS NOT SQL;",
            }];

            let result = migrate(&connection, &migrations, CatalogSchemaVersion::new(1)).await;

            assert!(matches!(result, Err(MigrationError::Apply { .. })));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 0);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = 'must_rollback'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
