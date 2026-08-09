use crate::catalog::is_corruption;
use a3_application::{
    CommandAllowlistStoreFailure, CommandAllowlistStoreVersion, StoredProjectCommandAllowlist,
};
use a3_domain::{
    AgentRunTimestamp, CommandCatalogId, DiscoveredCommandId, ProjectCommandAllowlist, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;
const MAX_COMMANDS: usize = 256;

pub(crate) async fn load_current(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<Option<StoredProjectCommandAllowlist>, CommandAllowlistRepositoryError> {
    let Some((version, catalog_id, confirmed_at)) =
        load_latest_header(connection, worktree_id).await?
    else {
        return Ok(None);
    };
    let command_ids = load_entries(connection, worktree_id, version).await?;
    let allowlist =
        ProjectCommandAllowlist::from_stored(worktree_id, catalog_id, command_ids, confirmed_at)
            .map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)?;
    Ok(Some(StoredProjectCommandAllowlist::new(version, allowlist)))
}

pub(crate) async fn append(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected: Option<CommandAllowlistStoreVersion>,
    confirmation: &ProjectCommandAllowlist,
) -> Result<StoredProjectCommandAllowlist, CommandAllowlistRepositoryError> {
    if confirmation.worktree_id() != worktree_id {
        return Err(CommandAllowlistRepositoryError::ProjectMismatch);
    }
    let transaction = begin(connection).await?;
    let result = async {
        let current = load_latest_version(&transaction, worktree_id).await?;
        if current != expected {
            return Err(CommandAllowlistRepositoryError::VersionConflict);
        }
        let next_value = match current {
            None => 1,
            Some(version) => version
                .get()
                .checked_add(1)
                .ok_or(CommandAllowlistRepositoryError::ResourceLimit)?,
        };
        let next = CommandAllowlistStoreVersion::new(next_value)
            .map_err(|_| CommandAllowlistRepositoryError::ResourceLimit)?;
        transaction
            .execute(
                "INSERT INTO command_allowlist_revisions (
                 worktree_id, revision, catalog_id, confirmed_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id_bytes(worktree_id.as_bytes()),
                    u64_to_i64(next.get())?,
                    id_bytes(confirmation.catalog_id().as_bytes()),
                    u64_to_i64(confirmation.confirmed_at().unix_millis())?
                ],
            )
            .await
            .map_err(classify_write)?;
        for (ordinal, command_id) in confirmation.command_ids().iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO command_allowlist_entries (
                     worktree_id, revision, ordinal, command_id
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        id_bytes(worktree_id.as_bytes()),
                        u64_to_i64(next.get())?,
                        usize_to_i64(ordinal)?,
                        id_bytes(command_id.as_bytes())
                    ],
                )
                .await
                .map_err(classify_write)?;
        }
        Ok(StoredProjectCommandAllowlist::new(
            next,
            confirmation.clone(),
        ))
    }
    .await;
    close(transaction, result).await
}

async fn load_latest_header(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<
    Option<(
        CommandAllowlistStoreVersion,
        CommandCatalogId,
        AgentRunTimestamp,
    )>,
    CommandAllowlistRepositoryError,
> {
    let mut rows = connection
        .query(
            "SELECT revision, catalog_id, confirmed_at_unix_millis
             FROM command_allowlist_revisions WHERE worktree_id = ?1
             ORDER BY revision DESC LIMIT 1",
            params![id_bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let version = read_version(&row, 0)?;
    let catalog_id = CommandCatalogId::from_bytes(read_id(&row, 1)?);
    let confirmed_at = read_timestamp(&row, 2)?;
    if rows
        .next()
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?
        .is_some()
    {
        return Err(CommandAllowlistRepositoryError::InvalidStoredData);
    }
    Ok(Some((version, catalog_id, confirmed_at)))
}

async fn load_entries(
    connection: &Connection,
    worktree_id: WorktreeId,
    version: CommandAllowlistStoreVersion,
) -> Result<Vec<DiscoveredCommandId>, CommandAllowlistRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT ordinal, command_id FROM command_allowlist_entries
             WHERE worktree_id = ?1 AND revision = ?2 ORDER BY ordinal",
            params![id_bytes(worktree_id.as_bytes()), u64_to_i64(version.get())?],
        )
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?;
    let mut command_ids = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?
    {
        if command_ids.len() == MAX_COMMANDS {
            return Err(CommandAllowlistRepositoryError::ResourceLimit);
        }
        let ordinal: i64 = row.get(0).map_err(CommandAllowlistRepositoryError::Read)?;
        let expected = usize_to_i64(command_ids.len())?;
        if ordinal != expected {
            return Err(CommandAllowlistRepositoryError::InvalidStoredData);
        }
        command_ids.push(DiscoveredCommandId::from_bytes(read_id(&row, 1)?));
    }
    Ok(command_ids)
}

async fn load_latest_version(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<CommandAllowlistStoreVersion>, CommandAllowlistRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT revision FROM command_allowlist_revisions WHERE worktree_id = ?1
             ORDER BY revision DESC LIMIT 1",
            params![id_bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?;
    let version = rows
        .next()
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?
        .as_ref()
        .map(|row| read_version(row, 0))
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(CommandAllowlistRepositoryError::Read)?
        .is_some()
    {
        return Err(CommandAllowlistRepositoryError::InvalidStoredData);
    }
    Ok(version)
}

async fn begin(connection: &Connection) -> Result<Transaction, CommandAllowlistRepositoryError> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(CommandAllowlistRepositoryError::Begin)
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, CommandAllowlistRepositoryError>,
) -> Result<T, CommandAllowlistRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(CommandAllowlistRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(CommandAllowlistRepositoryError::Rollback(source)),
        },
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], CommandAllowlistRepositoryError> {
    let value: Vec<u8> = row
        .get(index)
        .map_err(CommandAllowlistRepositoryError::Read)?;
    value
        .try_into()
        .map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)
}

fn read_version(
    row: &libsql::Row,
    index: i32,
) -> Result<CommandAllowlistStoreVersion, CommandAllowlistRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(CommandAllowlistRepositoryError::Read)?;
    let value =
        u64::try_from(value).map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)?;
    CommandAllowlistStoreVersion::new(value)
        .map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<AgentRunTimestamp, CommandAllowlistRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(CommandAllowlistRepositoryError::Read)?;
    let value =
        u64::try_from(value).map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(value)
        .map_err(|_| CommandAllowlistRepositoryError::InvalidStoredData)
}

fn id_bytes(bytes: &[u8; 32]) -> Vec<u8> {
    bytes.to_vec()
}

fn u64_to_i64(value: u64) -> Result<i64, CommandAllowlistRepositoryError> {
    i64::try_from(value).map_err(|_| CommandAllowlistRepositoryError::ResourceLimit)
}

fn usize_to_i64(value: usize) -> Result<i64, CommandAllowlistRepositoryError> {
    i64::try_from(value).map_err(|_| CommandAllowlistRepositoryError::ResourceLimit)
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_write(source: libsql::Error) -> CommandAllowlistRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        CommandAllowlistRepositoryError::InvalidStoredData
    } else {
        CommandAllowlistRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum CommandAllowlistRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredData,
    ResourceLimit,
    ProjectMismatch,
    VersionConflict,
}

impl CommandAllowlistRepositoryError {
    pub(crate) fn classify(&self) -> CommandAllowlistStoreFailure {
        match self {
            Self::InvalidStoredData | Self::ResourceLimit => {
                CommandAllowlistStoreFailure::InvalidStoredData
            }
            Self::ProjectMismatch => CommandAllowlistStoreFailure::ProjectMismatch,
            Self::VersionConflict => CommandAllowlistStoreFailure::VersionConflict,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    CommandAllowlistStoreFailure::Corrupt
                } else {
                    CommandAllowlistStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for CommandAllowlistRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "command allowlist transaction could not begin",
            Self::Read(_) => "command allowlist data could not be read",
            Self::Write(_) => "command allowlist data could not be written",
            Self::Commit(_) => "command allowlist transaction could not commit",
            Self::Rollback(_) => "command allowlist transaction could not roll back",
            Self::InvalidStoredData => "command allowlist data is invalid",
            Self::ResourceLimit => "command allowlist data exceeds a fixed bound",
            Self::ProjectMismatch => "command allowlist belongs to another project",
            Self::VersionConflict => "command allowlist version changed concurrently",
        })
    }
}
