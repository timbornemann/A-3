use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, ModuleTreeBoundaryEvidence, ModuleTreeChildState, ModuleTreeControl,
    ModuleTreeEntry, ModuleTreeFailure, ModuleTreeLoadResult, ModuleTreePage, ModuleTreeQuery,
};
use a3_domain::{
    ContentHash, FileRevision, IndexRunId, ModuleId, ModuleKind, ModuleRoot, RepositoryPath,
    SnapshotId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_MODULE_TREE_READ_DURATION: Duration = Duration::from_secs(2);

const MODULE_TREE_SELECT: &str = "WITH primary_modules AS (\n\
  SELECT module_id, kind, root_kind, root_path, central_symbols_truncated,\n\
    entrypoints_truncated, tests_truncated\n\
  FROM modules WHERE index_run_id = ?1 AND kind IN ('manifest', 'path')\n\
)\n\
SELECT c.module_id, c.kind, c.root_kind, c.root_path,\n\
  c.central_symbols_truncated, c.entrypoints_truncated, c.tests_truncated,\n\
  (SELECT COUNT(*) FROM module_manifests manifest\n\
    WHERE manifest.index_run_id = ?1 AND manifest.module_id = c.module_id),\n\
  (SELECT repository_path FROM module_manifests manifest\n\
    WHERE manifest.index_run_id = ?1 AND manifest.module_id = c.module_id\n\
    ORDER BY manifest_order LIMIT 1),\n\
  (SELECT content_hash FROM module_manifests manifest\n\
    WHERE manifest.index_run_id = ?1 AND manifest.module_id = c.module_id\n\
    ORDER BY manifest_order LIMIT 1),\n\
  (SELECT COUNT(*) FROM module_members member\n\
    WHERE member.index_run_id = ?1 AND member.module_id = c.module_id),\n\
  (SELECT COUNT(DISTINCT member_path) FROM module_members member\n\
    WHERE member.index_run_id = ?1 AND member.module_id = c.module_id),\n\
  (SELECT member_path FROM module_members member\n\
    WHERE member.index_run_id = ?1 AND member.module_id = c.module_id\n\
    ORDER BY symbol_id LIMIT 1),\n\
  (SELECT member_hash FROM module_members member\n\
    WHERE member.index_run_id = ?1 AND member.module_id = c.module_id\n\
    ORDER BY symbol_id LIMIT 1),\n\
  (SELECT COUNT(*) FROM module_central_symbols feature\n\
    WHERE feature.index_run_id = ?1 AND feature.module_id = c.module_id),\n\
  (SELECT COUNT(*) FROM module_entrypoints feature\n\
    WHERE feature.index_run_id = ?1 AND feature.module_id = c.module_id),\n\
  (SELECT COUNT(*) FROM module_tests feature\n\
    WHERE feature.index_run_id = ?1 AND feature.module_id = c.module_id),\n\
  CASE WHEN EXISTS (SELECT 1 FROM primary_modules child WHERE\n\
    (c.root_kind = 'repository' AND child.root_kind = 'directory') OR\n\
    (c.root_kind = 'directory' AND child.root_kind = 'directory'\n\
      AND length(child.root_path) > length(c.root_path)\n\
      AND substr(child.root_path, 1, length(c.root_path)) = c.root_path\n\
      AND substr(child.root_path, length(c.root_path) + 1, 1) = X'2F'))\n\
    THEN 1 ELSE 0 END,\n\
  (SELECT COUNT(*) FROM module_members member\n\
    WHERE member.index_run_id = ?1 AND member.module_id = c.module_id\n\
      AND member.membership_kind <> c.kind)\n\
FROM primary_modules c\n\
WHERE ";

const ROOT_DIRECT_PREDICATE: &str = "NOT EXISTS (SELECT 1 FROM primary_modules ancestor WHERE\n\
  (ancestor.root_kind = 'repository' AND c.root_kind = 'directory') OR\n\
  (ancestor.root_kind = 'directory' AND c.root_kind = 'directory'\n\
    AND length(c.root_path) > length(ancestor.root_path)\n\
    AND substr(c.root_path, 1, length(ancestor.root_path)) = ancestor.root_path\n\
    AND substr(c.root_path, length(ancestor.root_path) + 1, 1) = X'2F'))";

const REPOSITORY_PARENT_DIRECT_PREDICATE: &str = "c.root_kind = 'directory' AND\n\
  NOT EXISTS (SELECT 1 FROM primary_modules intermediate WHERE\n\
    intermediate.root_kind = 'directory'\n\
    AND length(c.root_path) > length(intermediate.root_path)\n\
    AND substr(c.root_path, 1, length(intermediate.root_path)) = intermediate.root_path\n\
    AND substr(c.root_path, length(intermediate.root_path) + 1, 1) = X'2F')";

const DIRECTORY_PARENT_DIRECT_PREDICATE: &str = "c.root_kind = 'directory'\n\
  AND length(c.root_path) > length(?4)\n\
  AND substr(c.root_path, 1, length(?4)) = ?4\n\
  AND substr(c.root_path, length(?4) + 1, 1) = X'2F'\n\
  AND NOT EXISTS (SELECT 1 FROM primary_modules intermediate WHERE\n\
    intermediate.root_kind = 'directory'\n\
    AND length(intermediate.root_path) > length(?4)\n\
    AND substr(intermediate.root_path, 1, length(?4)) = ?4\n\
    AND substr(intermediate.root_path, length(?4) + 1, 1) = X'2F'\n\
    AND length(c.root_path) > length(intermediate.root_path)\n\
    AND substr(c.root_path, 1, length(intermediate.root_path)) = intermediate.root_path\n\
    AND substr(c.root_path, length(intermediate.root_path) + 1, 1) = X'2F')";

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleTreeQuery,
    control: &dyn ModuleTreeControl,
) -> Result<ModuleTreeLoadResult, ModuleTreeRepositoryError> {
    let guard = ModuleTreeReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleTreeRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ModuleTreeLoadResult::NoPublishedIndex);
        };
        let Some(expected_module_count) = publication.expected_module_count else {
            return Ok(ModuleTreeLoadResult::ProjectionUnavailable);
        };
        let (primary_module_count, graph_community_count) = module_counts(
            &transaction,
            publication.index_run_id,
            expected_module_count,
        )
        .await?;
        let parent_root = load_parent_root(
            &transaction,
            publication.index_run_id,
            query.parent_module_id(),
        )
        .await?;
        let raw_entries = load_entries(
            &transaction,
            publication.index_run_id,
            parent_root.as_ref(),
            query,
            &guard,
        )
        .await?;
        let page_size = usize::from(query.page_size().get());
        let has_more = raw_entries.len() > page_size;
        let entries = raw_entries.into_iter().take(page_size).collect::<Vec<_>>();
        let next_cursor = if has_more {
            entries.last().map(ModuleTreeEntry::module_id)
        } else {
            None
        };
        guard.checkpoint()?;
        ModuleTreePage::new(
            publication.index_run_id,
            publication.snapshot_id,
            query.parent_module_id(),
            primary_module_count,
            graph_community_count,
            entries,
            next_cursor,
        )
        .map(ModuleTreeLoadResult::Page)
        .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(page) => {
            transaction
                .commit()
                .await
                .map_err(ModuleTreeRepositoryError::Commit)?;
            Ok(page)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

struct Publication {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    expected_module_count: Option<u64>,
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<Publication>, ModuleTreeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT run.index_run_id, run.snapshot_id, projection.module_count\n\
             FROM index_runs run LEFT JOIN module_projections projection\n\
               ON projection.index_run_id = run.index_run_id\n\
             WHERE run.worktree_id = ?1 AND run.status = 'published'\n\
             ORDER BY run.run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleTreeRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(ModuleTreeRepositoryError::Read)? else {
        return Ok(None);
    };
    Ok(Some(Publication {
        index_run_id: IndexRunId::from_bytes(read_id(&row, 0)?),
        snapshot_id: SnapshotId::from_bytes(read_id(&row, 1)?),
        expected_module_count: read_optional_count(&row, 2)?,
    }))
}

async fn module_counts(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    expected_module_count: u64,
) -> Result<(u64, u64), ModuleTreeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT\n\
               COALESCE(SUM(CASE WHEN kind IN ('manifest', 'path') THEN 1 ELSE 0 END), 0),\n\
               COALESCE(SUM(CASE WHEN kind = 'graph-community' THEN 1 ELSE 0 END), 0),\n\
               COUNT(*),\n\
               (SELECT COUNT(*) FROM (\n\
                 SELECT root_kind, root_path FROM modules\n\
                 WHERE index_run_id = ?1 AND kind IN ('manifest', 'path')\n\
                 GROUP BY root_kind, root_path HAVING COUNT(*) > 1))\n\
             FROM modules WHERE index_run_id = ?1",
            [index_run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleTreeRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleTreeRepositoryError::Read)?
        .ok_or(ModuleTreeRepositoryError::InvalidStoredProjection)?;
    let primary = read_count(&row, 0)?;
    let communities = read_count(&row, 1)?;
    let total = read_count(&row, 2)?;
    let duplicate_roots = read_count(&row, 3)?;
    if total != expected_module_count
        || primary.checked_add(communities) != Some(total)
        || duplicate_roots != 0
    {
        return Err(ModuleTreeRepositoryError::InvalidStoredProjection);
    }
    Ok((primary, communities))
}

async fn load_parent_root(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    parent_module_id: Option<ModuleId>,
) -> Result<Option<ModuleRoot>, ModuleTreeRepositoryError> {
    let Some(parent_module_id) = parent_module_id else {
        return Ok(None);
    };
    let mut rows = transaction
        .query(
            "SELECT root_kind, root_path FROM modules\n\
             WHERE index_run_id = ?1 AND module_id = ?2 AND kind IN ('manifest', 'path')",
            params![
                index_run_id.as_bytes().to_vec(),
                parent_module_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleTreeRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(ModuleTreeRepositoryError::Read)? else {
        return Err(ModuleTreeRepositoryError::ParentUnavailable);
    };
    read_root(&row, 0, 1).map(Some)
}

async fn load_entries(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    parent_root: Option<&ModuleRoot>,
    query: &ModuleTreeQuery,
    guard: &ModuleTreeReadGuard<'_>,
) -> Result<Vec<ModuleTreeEntry>, ModuleTreeRepositoryError> {
    let after = query
        .after_module_id()
        .map(|module_id| module_id.as_bytes().to_vec());
    let limit = i64::from(query.page_size().get())
        .checked_add(1)
        .ok_or(ModuleTreeRepositoryError::InvalidStoredProjection)?;
    let (predicate, parent_path) = match parent_root {
        None => (ROOT_DIRECT_PREDICATE, None),
        Some(ModuleRoot::Repository) => (REPOSITORY_PARENT_DIRECT_PREDICATE, None),
        Some(ModuleRoot::Directory(path)) => (
            DIRECTORY_PARENT_DIRECT_PREDICATE,
            Some(path.as_bytes().to_vec()),
        ),
    };
    let sql = format!(
        "{MODULE_TREE_SELECT}{predicate}\n\
         AND (?2 IS NULL OR c.module_id > ?2)\n\
         ORDER BY c.module_id LIMIT ?3"
    );
    let rows = match parent_path {
        Some(parent_path) => {
            transaction
                .query(
                    &sql,
                    params![index_run_id.as_bytes().to_vec(), after, limit, parent_path],
                )
                .await
        }
        None => {
            transaction
                .query(
                    &sql,
                    params![index_run_id.as_bytes().to_vec(), after, limit],
                )
                .await
        }
    }
    .map_err(ModuleTreeRepositoryError::Read)?;
    read_entries(rows, guard).await
}

async fn read_entries(
    mut rows: libsql::Rows,
    guard: &ModuleTreeReadGuard<'_>,
) -> Result<Vec<ModuleTreeEntry>, ModuleTreeRepositoryError> {
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(ModuleTreeRepositoryError::Read)? {
        guard.checkpoint()?;
        let module_id = ModuleId::from_bytes(read_id(&row, 0)?);
        let kind = read_primary_kind(&row, 1)?;
        let root = read_root(&row, 2, 3)?;
        let central_truncated = read_bool(&row, 4)?;
        let entrypoints_truncated = read_bool(&row, 5)?;
        let tests_truncated = read_bool(&row, 6)?;
        let manifest_count = read_count(&row, 7)?;
        let manifest_revision = read_optional_revision(&row, 8, 9)?;
        let symbol_count = read_count(&row, 10)?;
        let file_count = read_count(&row, 11)?;
        let representative_revision = read_optional_revision(&row, 12, 13)?;
        let central_count = read_count(&row, 14)?;
        let entrypoint_count = read_count(&row, 15)?;
        let test_count = read_count(&row, 16)?;
        let child_state = if read_bool(&row, 17)? {
            ModuleTreeChildState::HasChildren
        } else {
            ModuleTreeChildState::Leaf
        };
        if read_count(&row, 18)? != 0 {
            return Err(ModuleTreeRepositoryError::InvalidStoredProjection);
        }
        let evidence = ModuleTreeBoundaryEvidence::new(
            kind,
            symbol_count,
            representative_revision,
            manifest_revision,
        )
        .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)?;
        entries.push(
            ModuleTreeEntry::new(
                module_id,
                kind,
                root,
                evidence,
                manifest_count,
                file_count,
                symbol_count,
                central_count,
                central_truncated,
                entrypoint_count,
                entrypoints_truncated,
                test_count,
                tests_truncated,
                child_state,
            )
            .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)?,
        );
    }
    Ok(entries)
}

struct ModuleTreeReadGuard<'a> {
    control: &'a dyn ModuleTreeControl,
    started_at: Instant,
}

impl<'a> ModuleTreeReadGuard<'a> {
    fn new(control: &'a dyn ModuleTreeControl) -> Result<Self, ModuleTreeRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleTreeRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleTreeRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_MODULE_TREE_READ_DURATION {
            return Err(ModuleTreeRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleTreeRepositoryError,
) -> Result<T, ModuleTreeRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleTreeRepositoryError::Rollback(source)),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleTreeRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(ModuleTreeRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ModuleTreeRepositoryError> {
    let value: i64 = row.get(index).map_err(ModuleTreeRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)
}

fn read_optional_count(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, ModuleTreeRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(ModuleTreeRepositoryError::Read)?;
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, ModuleTreeRepositoryError> {
    match row
        .get::<i64>(index)
        .map_err(ModuleTreeRepositoryError::Read)?
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ModuleTreeRepositoryError::InvalidStoredProjection),
    }
}

fn read_primary_kind(
    row: &libsql::Row,
    index: i32,
) -> Result<ModuleKind, ModuleTreeRepositoryError> {
    match row
        .get::<String>(index)
        .map_err(ModuleTreeRepositoryError::Read)?
        .as_str()
    {
        "manifest" => Ok(ModuleKind::ManifestBoundary),
        "path" => Ok(ModuleKind::PathBoundary),
        _ => Err(ModuleTreeRepositoryError::InvalidStoredProjection),
    }
}

fn read_root(
    row: &libsql::Row,
    kind_index: i32,
    path_index: i32,
) -> Result<ModuleRoot, ModuleTreeRepositoryError> {
    let kind: String = row
        .get(kind_index)
        .map_err(ModuleTreeRepositoryError::Read)?;
    let path: Option<Vec<u8>> = row
        .get(path_index)
        .map_err(ModuleTreeRepositoryError::Read)?;
    match (kind.as_str(), path) {
        ("repository", None) => Ok(ModuleRoot::Repository),
        ("directory", Some(path)) => RepositoryPath::try_from_bytes(path)
            .map(ModuleRoot::Directory)
            .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection),
        _ => Err(ModuleTreeRepositoryError::InvalidStoredProjection),
    }
}

fn read_optional_revision(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
) -> Result<Option<FileRevision>, ModuleTreeRepositoryError> {
    let path: Option<Vec<u8>> = row
        .get(path_index)
        .map_err(ModuleTreeRepositoryError::Read)?;
    let hash: Option<Vec<u8>> = row
        .get(hash_index)
        .map_err(ModuleTreeRepositoryError::Read)?;
    match (path, hash) {
        (None, None) => Ok(None),
        (Some(path), Some(hash)) => {
            let path = RepositoryPath::try_from_bytes(path)
                .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)?;
            let hash = hash
                .try_into()
                .map(ContentHash::from_bytes)
                .map_err(|_| ModuleTreeRepositoryError::InvalidStoredProjection)?;
            Ok(Some(FileRevision::new(path, hash)))
        }
        _ => Err(ModuleTreeRepositoryError::InvalidStoredProjection),
    }
}

#[derive(Debug)]
pub(crate) enum ModuleTreeRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    ParentUnavailable,
    Cancelled,
    TimedOut,
}

impl ModuleTreeRepositoryError {
    pub(crate) fn classify(&self) -> ModuleTreeFailure {
        match self {
            Self::InvalidStoredProjection => ModuleTreeFailure::InvalidStoredProjection,
            Self::ParentUnavailable => ModuleTreeFailure::ParentUnavailable,
            Self::Cancelled => ModuleTreeFailure::Cancelled,
            Self::TimedOut => ModuleTreeFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleTreeFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleTreeFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleTreeRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin module-tree read",
            Self::Read(_) => "could not read module tree",
            Self::Commit(_) => "could not commit module-tree read",
            Self::Rollback(_) => "could not roll back module-tree read",
            Self::InvalidStoredProjection => "stored module-tree projection is invalid",
            Self::ParentUnavailable => "module-tree parent is unavailable",
            Self::Cancelled => "module-tree read was cancelled",
            Self::TimedOut => "module-tree read timed out",
        })
    }
}

impl Error for ModuleTreeRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::ParentUnavailable
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
