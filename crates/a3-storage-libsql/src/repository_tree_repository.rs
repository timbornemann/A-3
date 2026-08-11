use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, RepositoryTreeChildName, RepositoryTreeControl, RepositoryTreeEntry,
    RepositoryTreeEntryKind, RepositoryTreeFailure, RepositoryTreePage, RepositoryTreeQuery,
};
use a3_domain::{ContentHash, IndexRunId, RepositoryPath, SnapshotId, WorktreeId};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_REPOSITORY_TREE_READ_DURATION: Duration = Duration::from_secs(2);

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &RepositoryTreeQuery,
    control: &dyn RepositoryTreeControl,
) -> Result<Option<RepositoryTreePage>, RepositoryTreeRepositoryError> {
    let guard = RepositoryTreeReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(RepositoryTreeRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some((index_run_id, snapshot_id)) =
            latest_publication(&transaction, worktree_id).await?
        else {
            return Ok(None);
        };
        let raw_entries = match query.directory() {
            Some(directory) => {
                load_directory_entries(&transaction, index_run_id, directory, query, &guard).await?
            }
            None => load_root_entries(&transaction, index_run_id, query, &guard).await?,
        };
        let page_size = usize::from(query.page_size().get());
        let has_more = raw_entries.len() > page_size;
        let entries = raw_entries.into_iter().take(page_size).collect::<Vec<_>>();
        let next_cursor = if has_more {
            entries.last().map(|entry| entry.child_name().clone())
        } else {
            None
        };
        guard.checkpoint()?;
        RepositoryTreePage::new(
            index_run_id,
            snapshot_id,
            query.directory().cloned(),
            entries,
            next_cursor,
        )
        .map(Some)
        .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(page) => {
            transaction
                .commit()
                .await
                .map_err(RepositoryTreeRepositoryError::Commit)?;
            Ok(page)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<(IndexRunId, SnapshotId)>, RepositoryTreeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let publication = (
        IndexRunId::from_bytes(read_id(&row, 0)?),
        SnapshotId::from_bytes(read_id(&row, 1)?),
    );
    if rows
        .next()
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?
        .is_some()
    {
        return Err(RepositoryTreeRepositoryError::InvalidStoredProjection);
    }
    Ok(Some(publication))
}

async fn load_root_entries(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    query: &RepositoryTreeQuery,
    guard: &RepositoryTreeReadGuard<'_>,
) -> Result<Vec<RepositoryTreeEntry>, RepositoryTreeRepositoryError> {
    let limit = query_limit(query)?;
    let after = query.after().map(|name| name.as_bytes().to_vec());
    let rows = transaction
        .query(
            "WITH raw_children AS (\n\
               SELECT CASE WHEN instr(repository_path, X'2F') = 0\n\
                   THEN repository_path\n\
                   ELSE substr(repository_path, 1, instr(repository_path, X'2F') - 1)\n\
                 END AS child_name,\n\
                 CASE WHEN instr(repository_path, X'2F') = 0 THEN 0 ELSE 1 END AS is_directory,\n\
                 content_hash\n\
               FROM file_revisions WHERE index_run_id = ?1\n\
             ), children AS (\n\
               SELECT child_name, MAX(is_directory) AS is_directory, COUNT(*) AS file_count,\n\
                 CASE WHEN MAX(is_directory) = 0 THEN MIN(content_hash) END AS content_hash,\n\
                 SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END) AS direct_file_count\n\
               FROM raw_children GROUP BY child_name\n\
             )\n\
             SELECT child_name, is_directory, file_count, content_hash, direct_file_count\n\
             FROM children WHERE (?2 IS NULL OR child_name > ?2)\n\
             ORDER BY child_name LIMIT ?3",
            params![index_run_id.as_bytes().to_vec(), after, limit],
        )
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?;
    read_entries(rows, None, guard).await
}

async fn load_directory_entries(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    directory: &RepositoryPath,
    query: &RepositoryTreeQuery,
    guard: &RepositoryTreeReadGuard<'_>,
) -> Result<Vec<RepositoryTreeEntry>, RepositoryTreeRepositoryError> {
    let mut prefix = directory.as_bytes().to_vec();
    prefix
        .try_reserve(1)
        .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)?;
    prefix.push(b'/');
    let limit = query_limit(query)?;
    let after = query.after().map(|name| name.as_bytes().to_vec());
    let rows = transaction
        .query(
            "WITH descendants AS (\n\
               SELECT substr(repository_path, length(?2) + 1) AS relative_path, content_hash\n\
               FROM file_revisions\n\
               WHERE index_run_id = ?1\n\
                 AND length(repository_path) > length(?2)\n\
                 AND substr(repository_path, 1, length(?2)) = ?2\n\
             ), raw_children AS (\n\
               SELECT CASE WHEN instr(relative_path, X'2F') = 0\n\
                   THEN relative_path\n\
                   ELSE substr(relative_path, 1, instr(relative_path, X'2F') - 1)\n\
                 END AS child_name,\n\
                 CASE WHEN instr(relative_path, X'2F') = 0 THEN 0 ELSE 1 END AS is_directory,\n\
                 content_hash\n\
               FROM descendants\n\
             ), children AS (\n\
               SELECT child_name, MAX(is_directory) AS is_directory, COUNT(*) AS file_count,\n\
                 CASE WHEN MAX(is_directory) = 0 THEN MIN(content_hash) END AS content_hash,\n\
                 SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END) AS direct_file_count\n\
               FROM raw_children GROUP BY child_name\n\
             )\n\
             SELECT child_name, is_directory, file_count, content_hash, direct_file_count\n\
             FROM children WHERE (?3 IS NULL OR child_name > ?3)\n\
             ORDER BY child_name LIMIT ?4",
            params![index_run_id.as_bytes().to_vec(), prefix, after, limit],
        )
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?;
    let entries = read_entries(rows, Some(directory), guard).await?;
    if entries.is_empty() && query.after().is_none() {
        return Err(RepositoryTreeRepositoryError::DirectoryUnavailable);
    }
    Ok(entries)
}

fn query_limit(query: &RepositoryTreeQuery) -> Result<i64, RepositoryTreeRepositoryError> {
    i64::from(query.page_size().get())
        .checked_add(1)
        .ok_or(RepositoryTreeRepositoryError::InvalidStoredProjection)
}

async fn read_entries(
    mut rows: libsql::Rows,
    directory: Option<&RepositoryPath>,
    guard: &RepositoryTreeReadGuard<'_>,
) -> Result<Vec<RepositoryTreeEntry>, RepositoryTreeRepositoryError> {
    let mut entries = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(RepositoryTreeRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let child_bytes: Vec<u8> = row.get(0).map_err(RepositoryTreeRepositoryError::Read)?;
        let child_name = RepositoryTreeChildName::try_from_bytes(child_bytes)
            .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)?;
        let is_directory = read_flag(&row, 1)?;
        let file_count = read_count(&row, 2)?;
        let content_hash = read_optional_hash(&row, 3)?;
        let direct_file_count = read_count(&row, 4)?;
        if (is_directory && direct_file_count != 0) || (!is_directory && direct_file_count != 1) {
            return Err(RepositoryTreeRepositoryError::InvalidStoredProjection);
        }
        let path = direct_child_path(directory, &child_name)?;
        let kind = if is_directory {
            RepositoryTreeEntryKind::Directory
        } else {
            RepositoryTreeEntryKind::File
        };
        entries.push(
            RepositoryTreeEntry::new(path, child_name, kind, file_count, content_hash)
                .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)?,
        );
    }
    Ok(entries)
}

fn direct_child_path(
    directory: Option<&RepositoryPath>,
    child_name: &RepositoryTreeChildName,
) -> Result<RepositoryPath, RepositoryTreeRepositoryError> {
    let mut path = directory
        .map(|directory| directory.as_bytes().to_vec())
        .unwrap_or_default();
    let extra = child_name
        .as_bytes()
        .len()
        .checked_add(usize::from(directory.is_some()))
        .ok_or(RepositoryTreeRepositoryError::InvalidStoredProjection)?;
    path.try_reserve(extra)
        .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)?;
    if directory.is_some() {
        path.push(b'/');
    }
    path.extend_from_slice(child_name.as_bytes());
    RepositoryPath::try_from_bytes(path)
        .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)
}

struct RepositoryTreeReadGuard<'a> {
    control: &'a dyn RepositoryTreeControl,
    started_at: Instant,
}

impl<'a> RepositoryTreeReadGuard<'a> {
    fn new(control: &'a dyn RepositoryTreeControl) -> Result<Self, RepositoryTreeRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), RepositoryTreeRepositoryError> {
        if self.control.is_cancelled() {
            return Err(RepositoryTreeRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_REPOSITORY_TREE_READ_DURATION {
            return Err(RepositoryTreeRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: RepositoryTreeRepositoryError,
) -> Result<T, RepositoryTreeRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(RepositoryTreeRepositoryError::Rollback(source)),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], RepositoryTreeRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(RepositoryTreeRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)
}

fn read_flag(row: &libsql::Row, index: i32) -> Result<bool, RepositoryTreeRepositoryError> {
    match row
        .get::<i64>(index)
        .map_err(RepositoryTreeRepositoryError::Read)?
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RepositoryTreeRepositoryError::InvalidStoredProjection),
    }
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, RepositoryTreeRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(RepositoryTreeRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)
}

fn read_optional_hash(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<ContentHash>, RepositoryTreeRepositoryError> {
    let value: Option<Vec<u8>> = row
        .get(index)
        .map_err(RepositoryTreeRepositoryError::Read)?;
    value
        .map(|bytes| {
            bytes
                .try_into()
                .map(ContentHash::from_bytes)
                .map_err(|_| RepositoryTreeRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

#[derive(Debug)]
pub(crate) enum RepositoryTreeRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    DirectoryUnavailable,
    Cancelled,
    TimedOut,
}

impl RepositoryTreeRepositoryError {
    pub(crate) fn classify(&self) -> RepositoryTreeFailure {
        match self {
            Self::InvalidStoredProjection => RepositoryTreeFailure::InvalidStoredProjection,
            Self::DirectoryUnavailable => RepositoryTreeFailure::DirectoryUnavailable,
            Self::Cancelled => RepositoryTreeFailure::Cancelled,
            Self::TimedOut => RepositoryTreeFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    RepositoryTreeFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    RepositoryTreeFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for RepositoryTreeRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin repository-tree read",
            Self::Read(_) => "could not read repository tree",
            Self::Commit(_) => "could not commit repository-tree read",
            Self::Rollback(_) => "could not roll back repository-tree read",
            Self::InvalidStoredProjection => "stored repository-tree projection is invalid",
            Self::DirectoryUnavailable => "repository-tree directory is unavailable",
            Self::Cancelled => "repository-tree read was cancelled",
            Self::TimedOut => "repository-tree read timed out",
        })
    }
}

impl Error for RepositoryTreeRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::DirectoryUnavailable
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
