use crate::catalog::is_corruption;
use crate::index_codec;
use crate::index_publication::{IndexPublicationRepositoryError, read_stable_id};
use a3_application::{
    ModuleRuntimeControl, ModuleRuntimeFailure, ModuleRuntimeFlowQuery,
    ModuleRuntimeFlowRootValidation, ModuleRuntimeMap, ModuleRuntimeMapLoadResult,
    ModuleRuntimeMapQuery, ModuleRuntimeRoot, ModuleRuntimeRootKind, ModuleRuntimeRootSet,
};
use a3_domain::{IndexRunId, ModuleId, SnapshotId, WorktreeId};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_READ_DURATION: Duration = Duration::from_secs(2);
const MAX_FEATURE_ROOTS: u16 = 256;
const ENTRYPOINT_ROLE_MASK: i64 = 1 << 1;
const TEST_ROLE_MASK: i64 = 1;
const SYMBOL_COLUMNS: &str = "symbol.symbol_id, symbol.repository_path, symbol.content_hash,
 symbol.local_symbol_id, symbol.kind, symbol.name, symbol.signature,
 symbol.declaration_start_byte, symbol.declaration_end_byte, symbol.declaration_start_row,
 symbol.declaration_start_column, symbol.declaration_end_row, symbol.declaration_end_column,
 symbol.selection_start_byte, symbol.selection_end_byte, symbol.selection_start_row,
 symbol.selection_start_column, symbol.selection_end_row, symbol.selection_end_column,
 symbol.documentation_start_byte, symbol.documentation_end_byte, symbol.documentation_start_row,
 symbol.documentation_start_column, symbol.documentation_end_row, symbol.documentation_end_column,
 symbol.visibility, symbol.roles";

pub(crate) async fn load_map(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleRuntimeMapQuery,
    control: &dyn ModuleRuntimeControl,
) -> Result<ModuleRuntimeMapLoadResult, ModuleRuntimeRepositoryError> {
    let guard = ReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleRuntimeRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ModuleRuntimeMapLoadResult::NoPublishedIndex);
        };
        let (Some(expected_modules), Some(expected_symbols)) = (
            publication.expected_module_count,
            publication.expected_symbol_count,
        ) else {
            return Ok(ModuleRuntimeMapLoadResult::ProjectionUnavailable);
        };
        validate_projection(
            &transaction,
            publication.index_run_id,
            expected_modules,
            expected_symbols,
        )
        .await?;
        let Some(features) =
            read_feature_metadata(&transaction, publication.index_run_id, query.module_id())
                .await?
        else {
            return Ok(ModuleRuntimeMapLoadResult::ModuleUnavailable);
        };
        let entrypoints = read_roots(
            &transaction,
            publication.index_run_id,
            query.module_id(),
            ModuleRuntimeRootKind::Entrypoint,
            query.entrypoint_limit().get(),
            &guard,
        )
        .await?;
        let tests = read_roots(
            &transaction,
            publication.index_run_id,
            query.module_id(),
            ModuleRuntimeRootKind::Test,
            query.test_limit().get(),
            &guard,
        )
        .await?;
        guard.checkpoint()?;
        let entrypoints = ModuleRuntimeRootSet::new(
            ModuleRuntimeRootKind::Entrypoint,
            entrypoints,
            features.entrypoint_count,
            features.entrypoints_truncated,
        )
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
        let tests = ModuleRuntimeRootSet::new(
            ModuleRuntimeRootKind::Test,
            tests,
            features.test_count,
            features.tests_truncated,
        )
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
        ModuleRuntimeMap::new(
            publication.index_run_id,
            publication.snapshot_id,
            query.module_id(),
            entrypoints,
            tests,
        )
        .map(ModuleRuntimeMapLoadResult::Map)
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)
    }
    .await;
    finish(transaction, result).await
}

pub(crate) async fn validate_flow_root(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleRuntimeFlowQuery,
    control: &dyn ModuleRuntimeControl,
) -> Result<ModuleRuntimeFlowRootValidation, ModuleRuntimeRepositoryError> {
    let guard = ReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleRuntimeRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ModuleRuntimeFlowRootValidation::NoPublishedIndex);
        };
        let (Some(expected_modules), Some(expected_symbols)) = (
            publication.expected_module_count,
            publication.expected_symbol_count,
        ) else {
            return Ok(ModuleRuntimeFlowRootValidation::ProjectionUnavailable);
        };
        if publication.index_run_id != query.expected_index_run_id()
            || publication.snapshot_id != query.expected_snapshot_id()
        {
            return Ok(ModuleRuntimeFlowRootValidation::PublicationChanged);
        }
        validate_projection(
            &transaction,
            publication.index_run_id,
            expected_modules,
            expected_symbols,
        )
        .await?;
        if read_feature_metadata(&transaction, publication.index_run_id, query.module_id())
            .await?
            .is_none()
        {
            return Ok(ModuleRuntimeFlowRootValidation::ModuleUnavailable);
        }
        let root = read_root(
            &transaction,
            publication.index_run_id,
            query.module_id(),
            query.kind().root_kind(),
            query.root_symbol_id(),
        )
        .await?;
        guard.checkpoint()?;
        if root.is_some() {
            Ok(ModuleRuntimeFlowRootValidation::Current)
        } else {
            Ok(ModuleRuntimeFlowRootValidation::RootUnavailable)
        }
    }
    .await;
    finish(transaction, result).await
}

struct Publication {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    expected_module_count: Option<u64>,
    expected_symbol_count: Option<u64>,
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<Publication>, ModuleRuntimeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT run.index_run_id, run.snapshot_id, projection.module_count,
               projection.symbol_count
             FROM index_runs run LEFT JOIN module_projections projection
               ON projection.index_run_id = run.index_run_id
             WHERE run.worktree_id = ?1 AND run.status = 'published'
             ORDER BY run.run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?
    else {
        return Ok(None);
    };
    Ok(Some(Publication {
        index_run_id: IndexRunId::from_bytes(read_id(&row, 0)?),
        snapshot_id: SnapshotId::from_bytes(read_id(&row, 1)?),
        expected_module_count: read_optional_count(&row, 2)?,
        expected_symbol_count: read_optional_count(&row, 3)?,
    }))
}

async fn validate_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    expected_modules: u64,
    expected_symbols: u64,
) -> Result<(), ModuleRuntimeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT
               (SELECT COUNT(*) FROM modules WHERE index_run_id = ?1),
               (SELECT COUNT(*) FROM symbols WHERE index_run_id = ?1),
               (SELECT COUNT(*) FROM module_members
                 WHERE index_run_id = ?1 AND membership_kind IN ('manifest', 'path')),
               (SELECT COUNT(*) FROM module_members member JOIN modules module
                 ON module.index_run_id = member.index_run_id
                   AND module.module_id = member.module_id
                 WHERE member.index_run_id = ?1
                   AND member.membership_kind IN ('manifest', 'path')
                   AND member.membership_kind <> module.kind),
               (SELECT COUNT(*) FROM (
                 SELECT root_kind, root_path FROM modules
                 WHERE index_run_id = ?1 AND kind IN ('manifest', 'path')
                 GROUP BY root_kind, root_path HAVING COUNT(*) > 1)),
               (SELECT COUNT(*) FROM (
                 SELECT symbol.symbol_id FROM symbols symbol
                 LEFT JOIN module_members member
                   ON member.index_run_id = symbol.index_run_id
                     AND member.symbol_id = symbol.symbol_id
                     AND member.membership_kind IN ('manifest', 'path')
                 WHERE symbol.index_run_id = ?1
                 GROUP BY symbol.symbol_id HAVING COUNT(member.symbol_id) <> 1))",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?
        .ok_or(ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
    if read_count(&row, 0)? != expected_modules
        || read_count(&row, 1)? != expected_symbols
        || read_count(&row, 2)? != expected_symbols
        || read_count(&row, 3)? != 0
        || read_count(&row, 4)? != 0
        || read_count(&row, 5)? != 0
    {
        return Err(ModuleRuntimeRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

struct FeatureMetadata {
    entrypoints_truncated: bool,
    tests_truncated: bool,
    entrypoint_count: u16,
    test_count: u16,
}

async fn read_feature_metadata(
    transaction: &Transaction,
    run_id: IndexRunId,
    module_id: ModuleId,
) -> Result<Option<FeatureMetadata>, ModuleRuntimeRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT module.entrypoints_truncated, module.tests_truncated,
               (SELECT COUNT(*) FROM module_entrypoints feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT MIN(rank_order) FROM module_entrypoints feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT MAX(rank_order) FROM module_entrypoints feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT COUNT(*) FROM module_tests feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT MIN(rank_order) FROM module_tests feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT MAX(rank_order) FROM module_tests feature
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id),
               (SELECT COUNT(*) FROM module_entrypoints feature
                 LEFT JOIN symbols symbol
                   ON symbol.index_run_id = feature.index_run_id
                     AND symbol.symbol_id = feature.symbol_id
                 LEFT JOIN module_members member
                   ON member.index_run_id = feature.index_run_id
                     AND member.module_id = feature.module_id
                     AND member.symbol_id = feature.symbol_id
                     AND member.membership_kind = module.kind
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id
                   AND (symbol.symbol_id IS NULL OR member.symbol_id IS NULL
                     OR (symbol.roles & ?3) = 0)),
               (SELECT COUNT(*) FROM module_tests feature
                 LEFT JOIN symbols symbol
                   ON symbol.index_run_id = feature.index_run_id
                     AND symbol.symbol_id = feature.symbol_id
                 LEFT JOIN module_members member
                   ON member.index_run_id = feature.index_run_id
                     AND member.module_id = feature.module_id
                     AND member.symbol_id = feature.symbol_id
                     AND member.membership_kind = module.kind
                 WHERE feature.index_run_id = module.index_run_id
                   AND feature.module_id = module.module_id
                   AND (symbol.symbol_id IS NULL OR member.symbol_id IS NULL
                     OR (symbol.roles & ?4) = 0))
             FROM modules module
             WHERE module.index_run_id = ?1 AND module.module_id = ?2
               AND module.kind IN ('manifest', 'path')",
            params![
                run_id.as_bytes().to_vec(),
                module_id.as_bytes().to_vec(),
                ENTRYPOINT_ROLE_MASK,
                TEST_ROLE_MASK
            ],
        )
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let entrypoint_count = read_feature_count(&row, 2, 3, 4)?;
    let test_count = read_feature_count(&row, 5, 6, 7)?;
    if read_count(&row, 8)? != 0 || read_count(&row, 9)? != 0 {
        return Err(ModuleRuntimeRepositoryError::InvalidStoredProjection);
    }
    Ok(Some(FeatureMetadata {
        entrypoints_truncated: read_bool(&row, 0)?,
        tests_truncated: read_bool(&row, 1)?,
        entrypoint_count,
        test_count,
    }))
}

fn read_feature_count(
    row: &libsql::Row,
    count_index: i32,
    min_index: i32,
    max_index: i32,
) -> Result<u16, ModuleRuntimeRepositoryError> {
    let count = u16::try_from(read_count(row, count_index)?)
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
    let minimum = read_optional_i64(row, min_index)?;
    let maximum = read_optional_i64(row, max_index)?;
    let valid = if count == 0 {
        minimum.is_none() && maximum.is_none()
    } else {
        minimum == Some(1) && maximum == Some(i64::from(count))
    };
    if !valid || count > MAX_FEATURE_ROOTS {
        return Err(ModuleRuntimeRepositoryError::InvalidStoredProjection);
    }
    Ok(count)
}

async fn read_roots(
    transaction: &Transaction,
    run_id: IndexRunId,
    module_id: ModuleId,
    kind: ModuleRuntimeRootKind,
    limit: u16,
    guard: &ReadGuard<'_>,
) -> Result<Vec<ModuleRuntimeRoot>, ModuleRuntimeRepositoryError> {
    let table = feature_table(kind);
    let sql = format!(
        "SELECT feature.rank_order, {SYMBOL_COLUMNS}
         FROM {table} feature JOIN symbols symbol
           ON symbol.index_run_id = feature.index_run_id
             AND symbol.symbol_id = feature.symbol_id
         WHERE feature.index_run_id = ?1 AND feature.module_id = ?2
         ORDER BY feature.rank_order LIMIT ?3"
    );
    let mut rows = transaction
        .query(
            &sql,
            params![
                run_id.as_bytes().to_vec(),
                module_id.as_bytes().to_vec(),
                i64::from(limit)
            ],
        )
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?;
    let mut roots = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let rank = u16::try_from(read_i64(&row, 0)?)
            .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
        let symbol = index_codec::graph_symbol_from_row(&row, 1).map_err(map_decode_error)?;
        roots.push(
            ModuleRuntimeRoot::new(kind, rank, symbol)
                .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?,
        );
    }
    Ok(roots)
}

async fn read_root(
    transaction: &Transaction,
    run_id: IndexRunId,
    module_id: ModuleId,
    kind: ModuleRuntimeRootKind,
    symbol_id: a3_domain::SymbolId,
) -> Result<Option<ModuleRuntimeRoot>, ModuleRuntimeRepositoryError> {
    let table = feature_table(kind);
    let sql = format!(
        "SELECT feature.rank_order, {SYMBOL_COLUMNS}
         FROM {table} feature JOIN symbols symbol
           ON symbol.index_run_id = feature.index_run_id
             AND symbol.symbol_id = feature.symbol_id
         JOIN module_members member
           ON member.index_run_id = feature.index_run_id
             AND member.module_id = feature.module_id
             AND member.symbol_id = feature.symbol_id
             AND member.membership_kind IN ('manifest', 'path')
         WHERE feature.index_run_id = ?1 AND feature.module_id = ?2
           AND feature.symbol_id = ?3"
    );
    let mut rows = transaction
        .query(
            &sql,
            params![
                run_id.as_bytes().to_vec(),
                module_id.as_bytes().to_vec(),
                symbol_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleRuntimeRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let rank = u16::try_from(read_i64(&row, 0)?)
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)?;
    let symbol = index_codec::graph_symbol_from_row(&row, 1).map_err(map_decode_error)?;
    ModuleRuntimeRoot::new(kind, rank, symbol)
        .map(Some)
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)
}

const fn feature_table(kind: ModuleRuntimeRootKind) -> &'static str {
    match kind {
        ModuleRuntimeRootKind::Entrypoint => "module_entrypoints",
        ModuleRuntimeRootKind::Test => "module_tests",
    }
}

struct ReadGuard<'a> {
    control: &'a dyn ModuleRuntimeControl,
    started_at: Instant,
}

impl<'a> ReadGuard<'a> {
    fn new(control: &'a dyn ModuleRuntimeControl) -> Result<Self, ModuleRuntimeRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleRuntimeRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleRuntimeRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_READ_DURATION {
            return Err(ModuleRuntimeRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn finish<T>(
    transaction: Transaction,
    result: Result<T, ModuleRuntimeRepositoryError>,
) -> Result<T, ModuleRuntimeRepositoryError> {
    match result {
        Ok(result) => {
            transaction
                .commit()
                .await
                .map_err(ModuleRuntimeRepositoryError::Commit)?;
            Ok(result)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(ModuleRuntimeRepositoryError::Rollback(source)),
        },
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleRuntimeRepositoryError> {
    read_stable_id(row, index).map_err(map_decode_error)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, ModuleRuntimeRepositoryError> {
    row.get(index).map_err(ModuleRuntimeRepositoryError::Read)
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ModuleRuntimeRepositoryError> {
    u64::try_from(read_i64(row, index)?)
        .map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)
}

fn read_optional_count(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, ModuleRuntimeRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(ModuleRuntimeRepositoryError::Read)?;
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| ModuleRuntimeRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_optional_i64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<i64>, ModuleRuntimeRepositoryError> {
    row.get(index).map_err(ModuleRuntimeRepositoryError::Read)
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, ModuleRuntimeRepositoryError> {
    match read_i64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ModuleRuntimeRepositoryError::InvalidStoredProjection),
    }
}

fn map_decode_error(error: IndexPublicationRepositoryError) -> ModuleRuntimeRepositoryError {
    match error {
        IndexPublicationRepositoryError::Read(source) => ModuleRuntimeRepositoryError::Read(source),
        _ => ModuleRuntimeRepositoryError::InvalidStoredProjection,
    }
}

#[derive(Debug)]
pub(crate) enum ModuleRuntimeRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl ModuleRuntimeRepositoryError {
    pub(crate) fn classify(&self) -> ModuleRuntimeFailure {
        match self {
            Self::InvalidStoredProjection => ModuleRuntimeFailure::InvalidStoredProjection,
            Self::Cancelled => ModuleRuntimeFailure::Cancelled,
            Self::TimedOut => ModuleRuntimeFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleRuntimeFailure::Storage(a3_application::KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleRuntimeFailure::Storage(
                        a3_application::KnowledgeStoreFailure::Unavailable,
                    )
                }
            }
        }
    }
}

impl fmt::Display for ModuleRuntimeRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin module runtime read",
            Self::Read(_) => "could not read module runtime projection",
            Self::Commit(_) => "could not commit module runtime read",
            Self::Rollback(_) => "could not roll back module runtime read",
            Self::InvalidStoredProjection => "stored module runtime projection is invalid",
            Self::Cancelled => "module runtime read was cancelled",
            Self::TimedOut => "module runtime read timed out",
        })
    }
}

impl Error for ModuleRuntimeRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::InvalidStoredProjection | Self::Cancelled | Self::TimedOut => None,
        }
    }
}
