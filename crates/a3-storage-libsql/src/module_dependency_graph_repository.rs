use crate::catalog::is_corruption;
use crate::index_codec;
use crate::index_publication::{IndexPublicationRepositoryError, read_stable_id};
use a3_application::{
    KnowledgeStoreFailure, ModuleDependencyEdge, ModuleDependencyGraph,
    ModuleDependencyGraphControl, ModuleDependencyGraphFailure, ModuleDependencyGraphLoadResult,
    ModuleDependencyGraphQuery, ModuleDependencyNode,
};
use a3_domain::{
    ContentHash, FileRevision, GraphEdge, GraphEndpoint, IndexRunId, ModuleId, ModuleKind,
    ModuleRoot, RepositoryPath, SnapshotId, SymbolId, SyntaxRelationKind, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_READ_DURATION: Duration = Duration::from_secs(2);
const MAX_INSPECTED_EDGES: usize = 4_096;
const MAX_VISIBLE_EDGE_GROUPS: usize = 256;

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleDependencyGraphQuery,
    control: &dyn ModuleDependencyGraphControl,
) -> Result<ModuleDependencyGraphLoadResult, ModuleDependencyGraphRepositoryError> {
    let guard = ReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ModuleDependencyGraphLoadResult::NoPublishedIndex);
        };
        let (Some(expected_modules), Some(expected_symbols)) = (
            publication.expected_module_count,
            publication.expected_symbol_count,
        ) else {
            return Ok(ModuleDependencyGraphLoadResult::ProjectionUnavailable);
        };
        validate_projection(
            &transaction,
            publication.index_run_id,
            expected_modules,
            expected_symbols,
        )
        .await?;
        if !center_exists(
            &transaction,
            publication.index_run_id,
            query.center_module_id(),
        )
        .await?
        {
            return Ok(ModuleDependencyGraphLoadResult::CenterUnavailable);
        }

        let edge_batch = read_incident_edges(
            &transaction,
            publication.index_run_id,
            publication.snapshot_id,
            query.center_module_id(),
            &guard,
        )
        .await?;
        let endpoint_modules = map_endpoints_to_modules(
            &transaction,
            publication.index_run_id,
            &edge_batch.edges,
            &guard,
        )
        .await?;
        let aggregation = aggregate_edges(
            query.center_module_id(),
            edge_batch.edges,
            &endpoint_modules,
        )?;
        let selected_ids = select_nodes(
            query.center_module_id(),
            query.node_limit().get(),
            &aggregation.neighbor_weights,
        );
        let nodes = load_nodes(
            &transaction,
            publication.index_run_id,
            &selected_ids,
            &guard,
        )
        .await?;
        let selected = selected_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut selected_groups = aggregation
            .groups
            .into_iter()
            .filter(|((source, target, _), _)| {
                selected.contains(source) && selected.contains(target)
            })
            .collect::<Vec<_>>();
        let observed_edge_group_count = u64::try_from(selected_groups.len())
            .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
        selected_groups.sort_by(|left, right| {
            right
                .1
                .count
                .cmp(&left.1.count)
                .then_with(|| left.0.cmp(&right.0))
        });
        selected_groups.truncate(MAX_VISIBLE_EDGE_GROUPS);
        selected_groups.sort_by_key(|(key, _)| *key);
        let edges = selected_groups
            .into_iter()
            .map(|((source, target, relation), group)| {
                ModuleDependencyEdge::new(
                    source,
                    target,
                    relation,
                    group.count,
                    group.representative,
                )
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)
            })
            .collect::<Result<Vec<_>, _>>()?;
        guard.checkpoint()?;
        ModuleDependencyGraph::new(
            publication.index_run_id,
            publication.snapshot_id,
            query.center_module_id(),
            nodes,
            u64::try_from(aggregation.neighbor_weights.len())
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?,
            aggregation.neighbor_weights.len() > selected_ids.len().saturating_sub(1),
            edges,
            observed_edge_group_count,
            observed_edge_group_count
                > u64::try_from(MAX_VISIBLE_EDGE_GROUPS)
                    .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?,
            u64::try_from(edge_batch.inspected_count)
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?,
            edge_batch.truncated,
            aggregation.unmapped_edge_count,
        )
        .map(ModuleDependencyGraphLoadResult::Graph)
        .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)
    }
    .await;

    match result {
        Ok(result) => {
            transaction
                .commit()
                .await
                .map_err(ModuleDependencyGraphRepositoryError::Commit)?;
            Ok(result)
        }
        Err(error) => rollback(transaction, error).await,
    }
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
) -> Result<Option<Publication>, ModuleDependencyGraphRepositoryError> {
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
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
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
) -> Result<(), ModuleDependencyGraphRepositoryError> {
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
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
        .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
    if read_count(&row, 0)? != expected_modules
        || read_count(&row, 1)? != expected_symbols
        || read_count(&row, 2)? != expected_symbols
        || read_count(&row, 3)? != 0
        || read_count(&row, 4)? != 0
        || read_count(&row, 5)? != 0
    {
        return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn center_exists(
    transaction: &Transaction,
    run_id: IndexRunId,
    center: ModuleId,
) -> Result<bool, ModuleDependencyGraphRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM modules
             WHERE index_run_id = ?1 AND module_id = ?2 AND kind IN ('manifest', 'path')",
            params![run_id.as_bytes().to_vec(), center.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
        .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
    match read_count(&row, 0)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection),
    }
}

struct EdgeBatch {
    edges: Vec<GraphEdge>,
    inspected_count: usize,
    truncated: bool,
}

async fn read_incident_edges(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    center: ModuleId,
    guard: &ReadGuard<'_>,
) -> Result<EdgeBatch, ModuleDependencyGraphRepositoryError> {
    let sql = format!(
        "WITH center_members AS (
           SELECT symbol_id, member_path FROM module_members
           WHERE index_run_id = ?1 AND module_id = ?2
             AND membership_kind IN ('manifest', 'path')
         ), center_files AS (
           SELECT DISTINCT member_path FROM center_members
         )
         SELECT edge_sequence, {} FROM symbol_edges edge
         WHERE index_run_id = ?1 AND relation_kind NOT IN ('contains', 'defines') AND (
           (source_kind = 'symbol' AND source_value IN (SELECT symbol_id FROM center_members)) OR
           (source_kind = 'file' AND source_value IN (SELECT member_path FROM center_files)) OR
           (target_kind = 'symbol' AND target_value IN (SELECT symbol_id FROM center_members)) OR
           (target_kind = 'file' AND target_value IN (SELECT member_path FROM center_files))
         ) ORDER BY edge_sequence LIMIT ?3",
        index_codec::EDGE_COLUMNS
    );
    let sql_limit = i64::try_from(MAX_INSPECTED_EDGES.saturating_add(1))
        .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
    let mut rows = transaction
        .query(
            &sql,
            params![
                run_id.as_bytes().to_vec(),
                center.as_bytes().to_vec(),
                sql_limit
            ],
        )
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let mut edges = Vec::new();
    let mut previous_sequence = 0_i64;
    let mut truncated = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if edges.len() == MAX_INSPECTED_EDGES {
            truncated = true;
            break;
        }
        let sequence: i64 = row
            .get(0)
            .map_err(ModuleDependencyGraphRepositoryError::Read)?;
        if sequence <= previous_sequence {
            return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
        }
        previous_sequence = sequence;
        edges.push(
            index_codec::graph_edge_from_row(&row, 1, snapshot_id).map_err(map_decode_error)?,
        );
    }
    let inspected_count = edges.len();
    Ok(EdgeBatch {
        edges,
        inspected_count,
        truncated,
    })
}

async fn map_endpoints_to_modules(
    transaction: &Transaction,
    run_id: IndexRunId,
    edges: &[GraphEdge],
    guard: &ReadGuard<'_>,
) -> Result<BTreeMap<GraphEndpoint, Option<ModuleId>>, ModuleDependencyGraphRepositoryError> {
    let endpoints = edges
        .iter()
        .flat_map(|edge| [edge.source().clone(), edge.target().clone()])
        .collect::<BTreeSet<_>>();
    let symbols = endpoints
        .iter()
        .filter_map(|endpoint| match endpoint {
            GraphEndpoint::Symbol(id) => Some(*id),
            GraphEndpoint::File(_) => None,
        })
        .collect::<Vec<_>>();
    let files = endpoints
        .iter()
        .filter_map(|endpoint| match endpoint {
            GraphEndpoint::File(path) => Some(path),
            GraphEndpoint::Symbol(_) => None,
        })
        .collect::<Vec<_>>();
    let mut mappings = BTreeMap::new();
    map_symbols(transaction, run_id, &symbols, guard, &mut mappings).await?;
    map_files(transaction, run_id, &files, guard, &mut mappings).await?;
    if mappings.len() != endpoints.len() {
        return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
    }
    Ok(mappings)
}

async fn map_symbols(
    transaction: &Transaction,
    run_id: IndexRunId,
    symbols: &[SymbolId],
    guard: &ReadGuard<'_>,
    mappings: &mut BTreeMap<GraphEndpoint, Option<ModuleId>>,
) -> Result<(), ModuleDependencyGraphRepositoryError> {
    if symbols.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; symbols.len()].join(", ");
    let sql = format!(
        "SELECT symbol_id, module_id FROM module_members
         WHERE index_run_id = ? AND membership_kind IN ('manifest', 'path')
           AND symbol_id IN ({placeholders}) ORDER BY symbol_id"
    );
    let mut values = vec![Value::Blob(run_id.as_bytes().to_vec())];
    values.extend(
        symbols
            .iter()
            .map(|symbol| Value::Blob(symbol.as_bytes().to_vec())),
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(values))
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let endpoint = GraphEndpoint::Symbol(SymbolId::from_bytes(read_id(&row, 0)?));
        let module = ModuleId::from_bytes(read_id(&row, 1)?);
        if mappings.insert(endpoint, Some(module)).is_some() {
            return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
        }
    }
    if mappings.len() != symbols.len() {
        return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn map_files(
    transaction: &Transaction,
    run_id: IndexRunId,
    files: &[&RepositoryPath],
    guard: &ReadGuard<'_>,
    mappings: &mut BTreeMap<GraphEndpoint, Option<ModuleId>>,
) -> Result<(), ModuleDependencyGraphRepositoryError> {
    if files.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; files.len()].join(", ");
    let sql = format!(
        "SELECT member_path, MIN(module_id), COUNT(DISTINCT module_id)
         FROM module_members WHERE index_run_id = ?
           AND membership_kind IN ('manifest', 'path')
           AND member_path IN ({placeholders}) GROUP BY member_path ORDER BY member_path"
    );
    let mut values = vec![Value::Blob(run_id.as_bytes().to_vec())];
    values.extend(
        files
            .iter()
            .map(|path| Value::Blob(path.as_bytes().to_vec())),
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(values))
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let path = RepositoryPath::try_from_bytes(
            row.get(0)
                .map_err(ModuleDependencyGraphRepositoryError::Read)?,
        )
        .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
        if read_count(&row, 2)? != 1 {
            return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
        }
        let module = ModuleId::from_bytes(read_id(&row, 1)?);
        if mappings
            .insert(GraphEndpoint::File(path), Some(module))
            .is_some()
        {
            return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
        }
    }
    for path in files {
        mappings
            .entry(GraphEndpoint::File((*path).clone()))
            .or_insert(None);
    }
    Ok(())
}

struct EdgeAggregation {
    groups: BTreeMap<(ModuleId, ModuleId, SyntaxRelationKind), EdgeGroup>,
    neighbor_weights: BTreeMap<ModuleId, u64>,
    unmapped_edge_count: u64,
}

struct EdgeGroup {
    count: u64,
    representative: GraphEdge,
}

fn aggregate_edges(
    center: ModuleId,
    edges: Vec<GraphEdge>,
    mappings: &BTreeMap<GraphEndpoint, Option<ModuleId>>,
) -> Result<EdgeAggregation, ModuleDependencyGraphRepositoryError> {
    let mut groups = BTreeMap::<_, EdgeGroup>::new();
    let mut neighbor_weights = BTreeMap::<ModuleId, u64>::new();
    let mut unmapped_edge_count = 0_u64;
    for edge in edges {
        let source = mappings
            .get(edge.source())
            .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
        let target = mappings
            .get(edge.target())
            .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
        let (Some(source), Some(target)) = (*source, *target) else {
            if *source != Some(center) && *target != Some(center) {
                return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
            }
            unmapped_edge_count = unmapped_edge_count
                .checked_add(1)
                .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
            continue;
        };
        if source != center && target != center {
            return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
        }
        if source == target {
            continue;
        }
        let neighbor = if source == center { target } else { source };
        let weight = neighbor_weights.entry(neighbor).or_default();
        *weight = weight
            .checked_add(1)
            .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
        let key = (source, target, edge.kind());
        match groups.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(EdgeGroup {
                    count: 1,
                    representative: edge,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().count = entry
                    .get()
                    .count
                    .checked_add(1)
                    .ok_or(ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
            }
        }
    }
    Ok(EdgeAggregation {
        groups,
        neighbor_weights,
        unmapped_edge_count,
    })
}

fn select_nodes(center: ModuleId, limit: u16, weights: &BTreeMap<ModuleId, u64>) -> Vec<ModuleId> {
    let mut ranked = weights.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let neighbor_limit = usize::from(limit).saturating_sub(1);
    let mut selected = ranked
        .into_iter()
        .take(neighbor_limit)
        .map(|(module, _)| *module)
        .collect::<Vec<_>>();
    selected.push(center);
    selected.sort_unstable();
    selected
}

async fn load_nodes(
    transaction: &Transaction,
    run_id: IndexRunId,
    selected: &[ModuleId],
    guard: &ReadGuard<'_>,
) -> Result<Vec<ModuleDependencyNode>, ModuleDependencyGraphRepositoryError> {
    let placeholders = vec!["?"; selected.len()].join(", ");
    let sql = format!(
        "SELECT module.module_id, module.kind, module.root_kind, module.root_path,
           (SELECT member_path FROM module_members member
             WHERE member.index_run_id = module.index_run_id
               AND member.module_id = module.module_id
               AND member.membership_kind = module.kind ORDER BY symbol_id LIMIT 1),
           (SELECT member_hash FROM module_members member
             WHERE member.index_run_id = module.index_run_id
               AND member.module_id = module.module_id
               AND member.membership_kind = module.kind ORDER BY symbol_id LIMIT 1)
         FROM modules module WHERE module.index_run_id = ?
           AND module.kind IN ('manifest', 'path')
           AND module.module_id IN ({placeholders}) ORDER BY module.module_id"
    );
    let mut values = vec![Value::Blob(run_id.as_bytes().to_vec())];
    values.extend(
        selected
            .iter()
            .map(|module| Value::Blob(module.as_bytes().to_vec())),
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(values))
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let mut nodes = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
    {
        guard.checkpoint()?;
        nodes.push(
            ModuleDependencyNode::new(
                ModuleId::from_bytes(read_id(&row, 0)?),
                read_primary_kind(&row, 1)?,
                read_root(&row, 2, 3)?,
                read_optional_revision(&row, 4, 5)?,
            )
            .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?,
        );
    }
    if nodes.len() != selected.len() {
        return Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection);
    }
    Ok(nodes)
}

struct ReadGuard<'a> {
    control: &'a dyn ModuleDependencyGraphControl,
    started_at: Instant,
}

impl<'a> ReadGuard<'a> {
    fn new(
        control: &'a dyn ModuleDependencyGraphControl,
    ) -> Result<Self, ModuleDependencyGraphRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleDependencyGraphRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleDependencyGraphRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_READ_DURATION {
            return Err(ModuleDependencyGraphRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleDependencyGraphRepositoryError,
) -> Result<T, ModuleDependencyGraphRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleDependencyGraphRepositoryError::Rollback(source)),
    }
}

fn read_id(
    row: &libsql::Row,
    index: i32,
) -> Result<[u8; 32], ModuleDependencyGraphRepositoryError> {
    read_stable_id(row, index).map_err(map_decode_error)
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ModuleDependencyGraphRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)
}

fn read_optional_count(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, ModuleDependencyGraphRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    value
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_primary_kind(
    row: &libsql::Row,
    index: i32,
) -> Result<ModuleKind, ModuleDependencyGraphRepositoryError> {
    match row
        .get::<String>(index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?
        .as_str()
    {
        "manifest" => Ok(ModuleKind::ManifestBoundary),
        "path" => Ok(ModuleKind::PathBoundary),
        _ => Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection),
    }
}

fn read_root(
    row: &libsql::Row,
    kind_index: i32,
    path_index: i32,
) -> Result<ModuleRoot, ModuleDependencyGraphRepositoryError> {
    let kind: String = row
        .get(kind_index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let path: Option<Vec<u8>> = row
        .get(path_index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    match (kind.as_str(), path) {
        ("repository", None) => Ok(ModuleRoot::Repository),
        ("directory", Some(path)) => RepositoryPath::try_from_bytes(path)
            .map(ModuleRoot::Directory)
            .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection),
        _ => Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection),
    }
}

fn read_optional_revision(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
) -> Result<Option<FileRevision>, ModuleDependencyGraphRepositoryError> {
    let path: Option<Vec<u8>> = row
        .get(path_index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    let hash: Option<Vec<u8>> = row
        .get(hash_index)
        .map_err(ModuleDependencyGraphRepositoryError::Read)?;
    match (path, hash) {
        (None, None) => Ok(None),
        (Some(path), Some(hash)) => {
            let path = RepositoryPath::try_from_bytes(path)
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
            let hash = hash
                .try_into()
                .map(ContentHash::from_bytes)
                .map_err(|_| ModuleDependencyGraphRepositoryError::InvalidStoredProjection)?;
            Ok(Some(FileRevision::new(path, hash)))
        }
        _ => Err(ModuleDependencyGraphRepositoryError::InvalidStoredProjection),
    }
}

fn map_decode_error(
    error: IndexPublicationRepositoryError,
) -> ModuleDependencyGraphRepositoryError {
    match error {
        IndexPublicationRepositoryError::Read(source) => {
            ModuleDependencyGraphRepositoryError::Read(source)
        }
        _ => ModuleDependencyGraphRepositoryError::InvalidStoredProjection,
    }
}

#[derive(Debug)]
pub(crate) enum ModuleDependencyGraphRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl ModuleDependencyGraphRepositoryError {
    pub(crate) fn classify(&self) -> ModuleDependencyGraphFailure {
        match self {
            Self::InvalidStoredProjection => ModuleDependencyGraphFailure::InvalidStoredProjection,
            Self::Cancelled => ModuleDependencyGraphFailure::Cancelled,
            Self::TimedOut => ModuleDependencyGraphFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleDependencyGraphFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleDependencyGraphFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleDependencyGraphRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin module dependency read",
            Self::Read(_) => "could not read module dependencies",
            Self::Commit(_) => "could not commit module dependency read",
            Self::Rollback(_) => "could not roll back module dependency read",
            Self::InvalidStoredProjection => "stored module dependency projection is invalid",
            Self::Cancelled => "module dependency read was cancelled",
            Self::TimedOut => "module dependency read timed out",
        })
    }
}

impl Error for ModuleDependencyGraphRepositoryError {
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
