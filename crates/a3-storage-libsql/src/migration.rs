use blake3::Hasher;
use libsql::{Connection, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const CATALOG_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "bootstrap_catalog",
        sql: "CREATE TABLE schema_migrations (\n\
          version INTEGER PRIMARY KEY CHECK (version > 0),\n\
          name TEXT NOT NULL UNIQUE,\n\
          checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
          ) STRICT;",
    },
    Migration {
        version: 2,
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

const KNOWLEDGE_BOOTSTRAP_MIGRATION: Migration = Migration {
    version: 1,
    name: "bootstrap_worktree_knowledge",
    sql: "CREATE TABLE schema_migrations (\n\
      version INTEGER PRIMARY KEY CHECK (version > 0),\n\
      name TEXT NOT NULL UNIQUE,\n\
      checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
      ) STRICT;\n\
      CREATE TABLE worktree_storage_identity (\n\
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),\n\
      repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
      worktree_id BLOB NOT NULL UNIQUE CHECK (length(worktree_id) = 32)\n\
      ) STRICT;",
};

const KNOWLEDGE_PROJECT_INDEX_MIGRATION: Migration = Migration {
    version: 2,
    name: "project_snapshot_index_runs",
    sql: "CREATE TABLE repositories (\n\
      repository_id BLOB PRIMARY KEY NOT NULL CHECK (length(repository_id) = 32)\n\
      ) STRICT;\n\
      CREATE TABLE worktrees (\n\
      worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
      repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
      UNIQUE (worktree_id, repository_id),\n\
      FOREIGN KEY (repository_id) REFERENCES repositories(repository_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO repositories (repository_id)\n\
        SELECT repository_id FROM worktree_storage_identity WHERE singleton = 1;\n\
      INSERT INTO worktrees (worktree_id, repository_id)\n\
        SELECT worktree_id, repository_id FROM worktree_storage_identity WHERE singleton = 1;\n\
      CREATE TABLE snapshots (\n\
      snapshot_id BLOB PRIMARY KEY NOT NULL CHECK (length(snapshot_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      parent_snapshot_id BLOB CHECK (parent_snapshot_id IS NULL OR length(parent_snapshot_id) = 32),\n\
      generation INTEGER NOT NULL CHECK (generation > 0),\n\
      head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
      head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
      head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
      index_schema_version INTEGER NOT NULL\n\
        CHECK (index_schema_version BETWEEN 1 AND 4294967295),\n\
      CHECK (parent_snapshot_id IS NULL OR parent_snapshot_id <> snapshot_id),\n\
      CHECK (\n\
        (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
        (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
      ),\n\
      UNIQUE (worktree_id, generation),\n\
      UNIQUE (snapshot_id, worktree_id),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (parent_snapshot_id, worktree_id)\n\
        REFERENCES snapshots(snapshot_id, worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_adapter_revisions (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      language TEXT NOT NULL\n\
        CHECK (language IN ('generic', 'rust', 'typescript-javascript', 'python')),\n\
      adapter_version TEXT NOT NULL CHECK (length(adapter_version) BETWEEN 1 AND 128),\n\
      PRIMARY KEY (snapshot_id, language),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_changes (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      change_kind TEXT NOT NULL CHECK (change_kind IN ('upsert', 'delete')),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (snapshot_id, repository_path),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE index_runs (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      run_sequence INTEGER NOT NULL CHECK (run_sequence > 0),\n\
      ranking_policy_version INTEGER NOT NULL\n\
        CHECK (ranking_policy_version BETWEEN 1 AND 4294967295),\n\
      status TEXT NOT NULL CHECK (status IN ('building', 'published', 'failed', 'cancelled')),\n\
      UNIQUE (worktree_id, run_sequence),\n\
      FOREIGN KEY (snapshot_id, worktree_id)\n\
        REFERENCES snapshots(snapshot_id, worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE UNIQUE INDEX index_runs_one_building_per_worktree_idx\n\
        ON index_runs (worktree_id) WHERE status = 'building';\n\
      CREATE UNIQUE INDEX index_runs_one_publish_per_snapshot_policy_idx\n\
        ON index_runs (snapshot_id, ranking_policy_version) WHERE status = 'published';\n\
      CREATE INDEX index_runs_worktree_sequence_idx\n\
        ON index_runs (worktree_id, run_sequence DESC);",
};

const KNOWLEDGE_MIGRATIONS: &[Migration] = &[
    KNOWLEDGE_BOOTSTRAP_MIGRATION,
    KNOWLEDGE_PROJECT_INDEX_MIGRATION,
];

const CATALOG_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.catalog-migration.v1";
const KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.knowledge-migration.v1";

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

/// Monotone version of one worktree knowledge database schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeSchemaVersion(u32);

impl KnowledgeSchemaVersion {
    /// Current worktree schema version understood by this build.
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
    version: u32,
    name: &'static str,
    sql: &'static str,
}

pub(crate) async fn migrate_catalog(
    connection: &Connection,
) -> Result<CatalogSchemaVersion, MigrationError> {
    migrate(
        connection,
        CATALOG_MIGRATIONS,
        CatalogSchemaVersion::CURRENT.get(),
        CATALOG_MIGRATION_CHECKSUM_DOMAIN,
    )
    .await
    .map(CatalogSchemaVersion::new)
}

pub(crate) async fn migrate_knowledge(
    connection: &Connection,
    repository_id: &[u8; 32],
    worktree_id: &[u8; 32],
) -> Result<KnowledgeSchemaVersion, MigrationError> {
    let current = read_user_version(connection)
        .await
        .map_err(MigrationError::ReadVersion)?;
    if current == 0 {
        verify_history(
            connection,
            KNOWLEDGE_MIGRATIONS,
            current,
            KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
        )
        .await?;
        apply_knowledge_bootstrap(connection, repository_id, worktree_id).await?;
    }
    migrate(
        connection,
        KNOWLEDGE_MIGRATIONS,
        KnowledgeSchemaVersion::CURRENT.get(),
        KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
    )
    .await
    .map(KnowledgeSchemaVersion::new)
}

async fn apply_knowledge_bootstrap(
    connection: &Connection,
    repository_id: &[u8; 32],
    worktree_id: &[u8; 32],
) -> Result<(), MigrationError> {
    let migration = &KNOWLEDGE_BOOTSTRAP_MIGRATION;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|source| MigrationError::Begin {
            version: migration.version,
            source,
        })?;
    let result = async {
        apply_migration_body(&transaction, migration, KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN).await?;
        transaction
            .execute(
                "INSERT INTO worktree_storage_identity (singleton, repository_id, worktree_id)\n\
                 VALUES (1, ?1, ?2)",
                params![repository_id.to_vec(), worktree_id.to_vec()],
            )
            .await?;
        Ok::<(), libsql::Error>(())
    }
    .await;

    if let Err(source) = result {
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

async fn migrate(
    connection: &Connection,
    migrations: &[Migration],
    supported: u32,
    checksum_domain: &[u8],
) -> Result<u32, MigrationError> {
    let current = read_user_version(connection)
        .await
        .map_err(MigrationError::ReadVersion)?;
    if current > supported {
        return Err(MigrationError::NewerSchema { current, supported });
    }
    verify_history(connection, migrations, current, checksum_domain).await?;

    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current)
    {
        apply_migration(connection, migration, checksum_domain).await?;
    }

    verify_history(connection, migrations, supported, checksum_domain).await?;
    Ok(supported)
}

async fn apply_migration(
    connection: &Connection,
    migration: &Migration,
    checksum_domain: &[u8],
) -> Result<(), MigrationError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|source| MigrationError::Begin {
            version: migration.version,
            source,
        })?;

    if let Err(source) = apply_migration_body(&transaction, migration, checksum_domain).await {
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
    checksum_domain: &[u8],
) -> libsql::Result<()> {
    transaction.execute_batch(migration.sql).await?;
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![
                i64::from(migration.version),
                migration.name,
                migration_checksum(migration, checksum_domain).to_vec()
            ],
        )
        .await?;
    transaction
        .execute_batch(&format!("PRAGMA user_version = {}", migration.version))
        .await?;
    Ok(())
}

async fn verify_history(
    connection: &Connection,
    migrations: &[Migration],
    current: u32,
    checksum_domain: &[u8],
) -> Result<(), MigrationError> {
    if current == 0 {
        return Ok(());
    }

    let maximum = query_i64(
        connection,
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
    )
    .await
    .map_err(MigrationError::ReadHistory)?;
    if maximum != i64::from(current) {
        return Err(MigrationError::HistoryMismatch { version: current });
    }

    for migration in migrations
        .iter()
        .filter(|migration| migration.version <= current)
    {
        let mut rows = connection
            .query(
                "SELECT name, checksum FROM schema_migrations WHERE version = ?1",
                [i64::from(migration.version)],
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
        if name != migration.name
            || checksum.as_slice() != migration_checksum(migration, checksum_domain)
        {
            return Err(MigrationError::HistoryMismatch {
                version: migration.version,
            });
        }
    }
    Ok(())
}

pub(crate) async fn read_user_version(connection: &Connection) -> libsql::Result<u32> {
    let raw = query_i64(connection, "PRAGMA user_version").await?;
    let value = u32::try_from(raw).map_err(|_| libsql::Error::InvalidColumnType)?;
    Ok(value)
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

fn migration_checksum(migration: &Migration, checksum_domain: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    update_checksum_field(&mut hasher, checksum_domain);
    update_checksum_field(&mut hasher, &migration.version.to_le_bytes());
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
    NewerSchema { current: u32, supported: u32 },
    ReadHistory(libsql::Error),
    HistoryMismatch { version: u32 },
    Begin { version: u32, source: libsql::Error },
    Apply { version: u32, source: libsql::Error },
    Rollback { version: u32, source: libsql::Error },
    Commit { version: u32, source: libsql::Error },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadVersion(_) => formatter.write_str("could not read catalog schema version"),
            Self::NewerSchema { current, supported } => write!(
                formatter,
                "catalog schema {} is newer than supported schema {}",
                current, supported
            ),
            Self::ReadHistory(_) => formatter.write_str("could not read migration history"),
            Self::HistoryMismatch { version } => {
                write!(formatter, "migration history differs at schema {}", version)
            }
            Self::Begin { version, .. } => {
                write!(formatter, "could not begin migration {version}")
            }
            Self::Apply { version, .. } => {
                write!(formatter, "could not apply migration {version}")
            }
            Self::Rollback { version, .. } => {
                write!(formatter, "could not roll back migration {version}")
            }
            Self::Commit { version, .. } => {
                write!(formatter, "could not commit migration {version}")
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
        CATALOG_MIGRATION_CHECKSUM_DOMAIN, CATALOG_MIGRATIONS, CatalogSchemaVersion,
        KNOWLEDGE_MIGRATIONS, KnowledgeSchemaVersion, Migration, MigrationError, migrate,
        query_i64,
    };
    use futures::executor::block_on;
    use std::collections::HashSet;

    #[test]
    fn catalog_migration_definitions_are_contiguous_and_uniquely_named() {
        assert_migration_definitions(CATALOG_MIGRATIONS, CatalogSchemaVersion::CURRENT.get());
    }

    #[test]
    fn knowledge_migration_definitions_are_contiguous_and_uniquely_named() {
        assert_migration_definitions(KNOWLEDGE_MIGRATIONS, KnowledgeSchemaVersion::CURRENT.get());
    }

    fn assert_migration_definitions(migrations: &[Migration], current: u32) {
        assert_eq!(migrations.len(), current as usize);
        let mut names = HashSet::new();
        for (index, migration) in migrations.iter().enumerate() {
            assert_eq!(migration.version as usize, index + 1);
            assert!(names.insert(migration.name));
            assert!(!migration.sql.trim().is_empty());
        }
    }

    #[test]
    fn empty_knowledge_schema_migrates_to_current() -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;

            let version = super::migrate_knowledge(&connection, &[1; 32], &[2; 32]).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (\n\
                     'schema_migrations', 'worktree_storage_identity', 'repositories', 'worktrees',\n\
                     'snapshots', 'snapshot_adapter_revisions', 'snapshot_changes', 'index_runs'\n\
                     )",
                )
                .await?,
                8
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_upgrades_v1_identity_into_project_repositories()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [7; 32];
            let worktree_id = [8; 32];

            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 1);

            let version =
                super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM repositories").await?,
                1
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM worktrees").await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v2_upgrade_preserves_the_v1_database()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [9; 32];
            let worktree_id = [10; 32];

            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            connection
                .execute("CREATE TABLE repositories (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 2, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 1);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'worktrees'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('repositories') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_bootstrap_rolls_back_schema_history_and_version()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            connection
                .execute(
                    "CREATE TABLE worktree_storage_identity (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &[1; 32], &[2; 32]).await;

            assert!(matches!(result, Err(MigrationError::Apply { .. })));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 0);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('worktree_storage_identity') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
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
                1,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
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
                version: 1,
                name: "broken",
                sql: "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT, checksum BLOB);\n\
                      CREATE TABLE must_rollback (id INTEGER);\n\
                      THIS IS NOT SQL;",
            }];

            let result = migrate(
                &connection,
                &migrations,
                1,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await;

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
