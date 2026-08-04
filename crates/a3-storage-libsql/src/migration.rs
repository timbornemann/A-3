use blake3::Hasher;
use libsql::{Connection, TransactionBehavior, params};

const CATALOG_MIGRATIONS: &[Migration] = &[Migration {
    version: CatalogSchemaVersion::new(1),
    name: "bootstrap_catalog",
    sql: "CREATE TABLE schema_migrations (\n\
          version INTEGER PRIMARY KEY CHECK (version > 0),\n\
          name TEXT NOT NULL UNIQUE,\n\
          checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
          ) STRICT;",
}];

/// Monotone version of the global catalog schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogSchemaVersion(u32);

impl CatalogSchemaVersion {
    /// Current schema version understood by this build.
    pub const CURRENT: Self = Self::new(1);

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
