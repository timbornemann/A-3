use crate::catalog::is_corruption;
use crate::index_codec;
use crate::index_publication::{IndexPublicationRepositoryError, read_stable_id};
use a3_application::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeStoreFailure};
use a3_domain::{
    ContentHash, ExactSearchSymbol, ExactSearchTarget, FileRevision, GraphEdge, GraphEndpoint,
    GraphTraversalHit, GraphTraversalResult, IndexRunId, QualifiedSymbolName, RepositoryPath,
    SnapshotId, TraversalDirection, TraversalQuery, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_SEARCH_DURATION: Duration = Duration::from_secs(2);
const MAX_INSPECTED_EDGES: usize = 4_096;
const EXACT_PROJECTION_VERSION: i64 = 1;
const SYMBOL_COLUMNS: &str = "s.symbol_id, s.repository_path, s.content_hash, s.local_symbol_id,\n\
 s.kind, s.name, s.signature, s.declaration_start_byte, s.declaration_end_byte,\n\
 s.declaration_start_row, s.declaration_start_column, s.declaration_end_row,\n\
 s.declaration_end_column, s.selection_start_byte, s.selection_end_byte,\n\
 s.selection_start_row, s.selection_start_column, s.selection_end_row,\n\
 s.selection_end_column, s.documentation_start_byte, s.documentation_end_byte,\n\
 s.documentation_start_row, s.documentation_start_column, s.documentation_end_row,\n\
 s.documentation_end_column, s.visibility, s.roles";

pub(crate) async fn traverse_graph(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &TraversalQuery,
    control: &dyn KnowledgeSearchControl,
) -> Result<GraphTraversalResult, GraphTraversalRepositoryError> {
    let guard = SearchGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(GraphTraversalRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let (run_id, snapshot_id) = read_latest_publication(&transaction, worktree_id).await?;
        validate_projection(&transaction, run_id).await?;
        validate_seed(&transaction, run_id, query.start()).await?;
        let (mut discoveries, mut truncated) =
            breadth_first_paths(&transaction, run_id, snapshot_id, query, &guard).await?;
        let limit = usize::from(query.result_limit().get());
        if discoveries.len() > limit {
            discoveries.truncate(limit);
            truncated = true;
        }
        let targets = read_targets(&transaction, run_id, &discoveries, &guard).await?;
        let mut hits = Vec::with_capacity(discoveries.len());
        for discovery in discoveries {
            guard.checkpoint()?;
            let target = targets
                .get(&discovery.endpoint)
                .cloned()
                .ok_or(GraphTraversalRepositoryError::InvalidStoredProjection)?;
            hits.push(
                GraphTraversalHit::new(target, discovery.path, query, snapshot_id)
                    .map_err(|_| GraphTraversalRepositoryError::InvalidStoredProjection)?,
            );
        }
        guard.checkpoint()?;
        GraphTraversalResult::new(run_id, snapshot_id, query.clone(), hits, truncated)
            .map_err(|_| GraphTraversalRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(result) => {
            transaction
                .commit()
                .await
                .map_err(GraphTraversalRepositoryError::Commit)?;
            Ok(result)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn read_latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(IndexRunId, SnapshotId), GraphTraversalRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
    else {
        return Err(GraphTraversalRepositoryError::IndexUnavailable);
    };
    let run_id = IndexRunId::from_bytes(read_search_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_search_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
        .is_some()
    {
        return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
    }
    Ok((run_id, snapshot_id))
}

async fn validate_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<(), GraphTraversalRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT projection_version, symbol_count,\n\
             (SELECT COUNT(*) FROM symbols WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM exact_search_symbols WHERE index_run_id = ?1)\n\
             FROM exact_search_projections WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
    else {
        return Err(GraphTraversalRepositoryError::ProjectionUnavailable);
    };
    let version = read_i64(&row, 0)?;
    let expected = read_i64(&row, 1)?;
    let symbols = read_i64(&row, 2)?;
    let projected = read_i64(&row, 3)?;
    if version != EXACT_PROJECTION_VERSION
        || expected < 0
        || expected != symbols
        || expected != projected
        || rows
            .next()
            .await
            .map_err(GraphTraversalRepositoryError::Read)?
            .is_some()
    {
        return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn validate_seed(
    transaction: &Transaction,
    run_id: IndexRunId,
    seed: &GraphEndpoint,
) -> Result<(), GraphTraversalRepositoryError> {
    let (table, column, value): (&str, &str, &[u8]) = match seed {
        GraphEndpoint::File(path) => ("file_revisions", "repository_path", path.as_bytes()),
        GraphEndpoint::Symbol(id) => ("symbols", "symbol_id", id.as_bytes().as_slice()),
    };
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE index_run_id = ?1 AND {column} = ?2");
    let mut rows = transaction
        .query(&sql, params![run_id.as_bytes().to_vec(), value.to_vec()])
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
        .ok_or(GraphTraversalRepositoryError::InvalidStoredProjection)?;
    let count = read_i64(&row, 0)?;
    if rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
        .is_some()
    {
        return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
    }
    match count {
        0 => Err(GraphTraversalRepositoryError::SeedUnavailable),
        1 => Ok(()),
        _ => Err(GraphTraversalRepositoryError::InvalidStoredProjection),
    }
}

#[derive(Clone)]
struct FrontierEntry {
    endpoint: GraphEndpoint,
    path: Vec<GraphEdge>,
}

type Discovery = FrontierEntry;

async fn breadth_first_paths(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &TraversalQuery,
    guard: &SearchGuard<'_>,
) -> Result<(Vec<Discovery>, bool), GraphTraversalRepositoryError> {
    let mut visited = BTreeSet::from([query.start().clone()]);
    let mut frontier = vec![FrontierEntry {
        endpoint: query.start().clone(),
        path: Vec::new(),
    }];
    let mut discoveries = Vec::new();
    let mut inspected = 0_usize;
    let mut truncated = false;
    let discovery_boundary = usize::from(query.result_limit().get()).saturating_add(1);

    for _ in 0..query.max_depth().get() {
        if frontier.is_empty() || discoveries.len() >= discovery_boundary {
            break;
        }
        guard.checkpoint()?;
        let remaining = MAX_INSPECTED_EDGES.saturating_sub(inspected);
        if remaining == 0 {
            truncated = true;
            break;
        }
        let batch = read_frontier_edges(
            transaction,
            run_id,
            snapshot_id,
            query,
            &frontier,
            remaining,
            guard,
        )
        .await?;
        inspected = inspected.saturating_add(batch.edges.len());
        truncated |= batch.truncated;
        let parents = frontier
            .into_iter()
            .map(|entry| (entry.endpoint, entry.path))
            .collect::<BTreeMap<_, _>>();
        let mut next = Vec::new();
        for edge in batch.edges {
            let (from, to) = traversal_endpoints(&edge, query.direction());
            let parent = parents
                .get(from)
                .ok_or(GraphTraversalRepositoryError::InvalidStoredProjection)?;
            let endpoint = to.clone();
            if !visited.insert(endpoint.clone()) {
                continue;
            }
            let mut path = parent.clone();
            path.push(edge);
            let discovery = Discovery { endpoint, path };
            next.push(discovery.clone());
            discoveries.push(discovery);
            if discoveries.len() >= discovery_boundary {
                truncated = true;
                break;
            }
        }
        frontier = next;
        if truncated && batch.truncated {
            break;
        }
    }
    Ok((discoveries, truncated))
}

struct EdgeBatch {
    edges: Vec<GraphEdge>,
    truncated: bool,
}

async fn read_frontier_edges(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &TraversalQuery,
    frontier: &[FrontierEntry],
    limit: usize,
    guard: &SearchGuard<'_>,
) -> Result<EdgeBatch, GraphTraversalRepositoryError> {
    let (kind_column, value_column) = match query.direction() {
        TraversalDirection::Outgoing => ("source_kind", "source_value"),
        TraversalDirection::Incoming => ("target_kind", "target_value"),
    };
    let mut sql = format!(
        "SELECT edge_sequence, {} FROM symbol_edges\n\
         WHERE index_run_id = ? AND relation_kind = ? AND (",
        index_codec::EDGE_COLUMNS
    );
    let mut parameters = vec![
        Value::Blob(run_id.as_bytes().to_vec()),
        Value::Text(index_codec::relation_kind_to_stored(query.relation()).to_owned()),
    ];
    for (index, entry) in frontier.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str(&format!("({kind_column} = ? AND {value_column} = ?)"));
        let (kind, value) = index_codec::endpoint_to_stored(&entry.endpoint);
        parameters.push(Value::Text(kind.to_owned()));
        parameters.push(Value::Blob(value));
    }
    sql.push_str(") ORDER BY edge_sequence LIMIT ?");
    let sql_limit = i64::try_from(limit.saturating_add(1))
        .map_err(|_| GraphTraversalRepositoryError::InvalidStoredProjection)?;
    parameters.push(Value::Integer(sql_limit));
    let mut rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    let mut edges = Vec::new();
    let mut truncated = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if edges.len() == limit {
            truncated = true;
            break;
        }
        let sequence = read_i64(&row, 0)?;
        if sequence <= 0 {
            return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
        }
        edges.push(
            index_codec::graph_edge_from_row(&row, 1, snapshot_id).map_err(map_decode_error)?,
        );
    }
    Ok(EdgeBatch { edges, truncated })
}

fn traversal_endpoints(
    edge: &GraphEdge,
    direction: TraversalDirection,
) -> (&GraphEndpoint, &GraphEndpoint) {
    match direction {
        TraversalDirection::Outgoing => (edge.source(), edge.target()),
        TraversalDirection::Incoming => (edge.target(), edge.source()),
    }
}

async fn read_targets(
    transaction: &Transaction,
    run_id: IndexRunId,
    discoveries: &[Discovery],
    guard: &SearchGuard<'_>,
) -> Result<BTreeMap<GraphEndpoint, ExactSearchTarget>, GraphTraversalRepositoryError> {
    let files = discoveries
        .iter()
        .filter_map(|discovery| match &discovery.endpoint {
            GraphEndpoint::File(path) => Some(path),
            GraphEndpoint::Symbol(_) => None,
        })
        .collect::<Vec<_>>();
    let symbols = discoveries
        .iter()
        .filter_map(|discovery| match &discovery.endpoint {
            GraphEndpoint::File(_) => None,
            GraphEndpoint::Symbol(id) => Some(*id),
        })
        .collect::<Vec<_>>();
    let mut targets = BTreeMap::new();
    read_file_targets(transaction, run_id, &files, guard, &mut targets).await?;
    read_symbol_targets(transaction, run_id, &symbols, guard, &mut targets).await?;
    if targets.len() != discoveries.len() {
        return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
    }
    Ok(targets)
}

async fn read_file_targets(
    transaction: &Transaction,
    run_id: IndexRunId,
    paths: &[&RepositoryPath],
    guard: &SearchGuard<'_>,
    targets: &mut BTreeMap<GraphEndpoint, ExactSearchTarget>,
) -> Result<(), GraphTraversalRepositoryError> {
    if paths.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; paths.len()].join(", ");
    let sql = format!(
        "SELECT repository_path, content_hash FROM file_revisions\n\
         WHERE index_run_id = ? AND repository_path IN ({placeholders})\n\
         ORDER BY repository_path"
    );
    let mut parameters = vec![Value::Blob(run_id.as_bytes().to_vec())];
    parameters.extend(
        paths
            .iter()
            .map(|path| Value::Blob(path.as_bytes().to_vec())),
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let path: Vec<u8> = row.get(0).map_err(GraphTraversalRepositoryError::Read)?;
        let path = RepositoryPath::try_from_bytes(path)
            .map_err(|_| GraphTraversalRepositoryError::InvalidStoredProjection)?;
        let endpoint = GraphEndpoint::File(path.clone());
        let target = ExactSearchTarget::File(FileRevision::new(
            path,
            ContentHash::from_bytes(read_search_id(&row, 1)?),
        ));
        if targets.insert(endpoint, target).is_some() {
            return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
        }
    }
    Ok(())
}

async fn read_symbol_targets(
    transaction: &Transaction,
    run_id: IndexRunId,
    ids: &[a3_domain::SymbolId],
    guard: &SearchGuard<'_>,
    targets: &mut BTreeMap<GraphEndpoint, ExactSearchTarget>,
) -> Result<(), GraphTraversalRepositoryError> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT q.qualified_name, {SYMBOL_COLUMNS}\n\
         FROM symbols AS s JOIN exact_search_symbols AS q\n\
           ON q.index_run_id = s.index_run_id AND q.symbol_id = s.symbol_id\n\
         WHERE s.index_run_id = ? AND s.symbol_id IN ({placeholders})\n\
         ORDER BY s.symbol_id"
    );
    let mut parameters = vec![Value::Blob(run_id.as_bytes().to_vec())];
    parameters.extend(ids.iter().map(|id| Value::Blob(id.as_bytes().to_vec())));
    let mut rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(GraphTraversalRepositoryError::Read)?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(GraphTraversalRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let qualified_name = read_qualified_name(&row, 0)?;
        let symbol = index_codec::graph_symbol_from_row(&row, 1).map_err(map_decode_error)?;
        let endpoint = GraphEndpoint::Symbol(symbol.id());
        let target = ExactSearchTarget::Symbol(ExactSearchSymbol::new(symbol, qualified_name));
        if targets.insert(endpoint, target).is_some() {
            return Err(GraphTraversalRepositoryError::InvalidStoredProjection);
        }
    }
    Ok(())
}

fn read_qualified_name(
    row: &libsql::Row,
    index: i32,
) -> Result<QualifiedSymbolName, GraphTraversalRepositoryError> {
    let value: String = row
        .get(index)
        .map_err(GraphTraversalRepositoryError::Read)?;
    QualifiedSymbolName::try_from_string(value)
        .map_err(|_| GraphTraversalRepositoryError::InvalidStoredProjection)
}

fn read_search_id(
    row: &libsql::Row,
    index: i32,
) -> Result<[u8; 32], GraphTraversalRepositoryError> {
    read_stable_id(row, index).map_err(map_decode_error)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, GraphTraversalRepositoryError> {
    row.get(index).map_err(GraphTraversalRepositoryError::Read)
}

fn map_decode_error(error: IndexPublicationRepositoryError) -> GraphTraversalRepositoryError {
    match error {
        IndexPublicationRepositoryError::Read(source) => {
            GraphTraversalRepositoryError::Read(source)
        }
        _ => GraphTraversalRepositoryError::InvalidStoredProjection,
    }
}

struct SearchGuard<'a> {
    control: &'a dyn KnowledgeSearchControl,
    started: Instant,
}

impl<'a> SearchGuard<'a> {
    fn new(control: &'a dyn KnowledgeSearchControl) -> Result<Self, GraphTraversalRepositoryError> {
        let guard = Self {
            control,
            started: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), GraphTraversalRepositoryError> {
        if self.control.is_cancelled() {
            return Err(GraphTraversalRepositoryError::Cancelled);
        }
        if self.started.elapsed() > MAX_SEARCH_DURATION {
            return Err(GraphTraversalRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: GraphTraversalRepositoryError,
) -> Result<T, GraphTraversalRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(GraphTraversalRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum GraphTraversalRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    IndexUnavailable,
    SeedUnavailable,
    ProjectionUnavailable,
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl GraphTraversalRepositoryError {
    pub(crate) fn classify(&self, query: &TraversalQuery) -> KnowledgeSearchFailure {
        match self {
            Self::IndexUnavailable => KnowledgeSearchFailure::IndexUnavailable,
            Self::SeedUnavailable => KnowledgeSearchFailure::SeedUnavailable,
            Self::ProjectionUnavailable => {
                KnowledgeSearchFailure::ProjectionUnavailable(query.source_channel())
            }
            Self::InvalidStoredProjection => KnowledgeSearchFailure::InvalidStoredProjection,
            Self::Cancelled => KnowledgeSearchFailure::Cancelled,
            Self::TimedOut => KnowledgeSearchFailure::TimedOut,
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => KnowledgeSearchFailure::Storage(if is_corruption(source) {
                KnowledgeStoreFailure::Corrupt
            } else {
                KnowledgeStoreFailure::Unavailable
            }),
        }
    }
}

impl fmt::Display for GraphTraversalRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Begin(_) => "could not begin graph-traversal transaction",
            Self::Read(_) => "could not read graph-traversal projection",
            Self::Commit(_) => "could not close graph-traversal transaction",
            Self::Rollback(_) => "could not roll back graph-traversal transaction",
            Self::IndexUnavailable => "no published index is available",
            Self::SeedUnavailable => "graph-traversal seed is unavailable",
            Self::ProjectionUnavailable => "graph-traversal projection is unavailable",
            Self::InvalidStoredProjection => "stored graph-traversal projection is invalid",
            Self::Cancelled => "graph traversal was cancelled",
            Self::TimedOut => "graph traversal timed out",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphTraversalRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::IndexUnavailable
            | Self::SeedUnavailable
            | Self::ProjectionUnavailable
            | Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
