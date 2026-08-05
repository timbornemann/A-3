use crate::catalog::is_corruption;
use crate::index_codec;
use crate::index_publication::{IndexPublicationRepositoryError, read_stable_id};
use a3_application::{ExactSearchControl, KnowledgeSearchFailure, KnowledgeStoreFailure};
use a3_domain::{
    ContentHash, ExactSearchCursor, ExactSearchExplanation, ExactSearchHit, ExactSearchPage,
    ExactSearchPageSize, ExactSearchPosition, ExactSearchQuery, ExactSearchRole, ExactSearchSymbol,
    FileRevision, IndexRunId, QualifiedSymbolName, RepositoryPath, SnapshotId, SymbolId,
    WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_SEARCH_DURATION: Duration = Duration::from_secs(2);
const PROJECTION_VERSION: i64 = 1;
const SYMBOL_COLUMNS: &str = "s.symbol_id, s.repository_path, s.content_hash, s.local_symbol_id,\n\
 s.kind, s.name, s.signature, s.declaration_start_byte, s.declaration_end_byte,\n\
 s.declaration_start_row, s.declaration_start_column, s.declaration_end_row,\n\
 s.declaration_end_column, s.selection_start_byte, s.selection_end_byte,\n\
 s.selection_start_row, s.selection_start_column, s.selection_end_row,\n\
 s.selection_end_column, s.documentation_start_byte, s.documentation_end_byte,\n\
 s.documentation_start_row, s.documentation_start_column, s.documentation_end_row,\n\
 s.documentation_end_column, s.visibility, s.roles";

pub(crate) async fn search_exact(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ExactSearchQuery,
    page_size: ExactSearchPageSize,
    cursor: Option<&ExactSearchCursor>,
    control: &dyn ExactSearchControl,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let guard = SearchGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ExactSearchRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let (run_id, snapshot_id) = read_latest_publication(&transaction, worktree_id).await?;
        validate_cursor(cursor, query, run_id, snapshot_id)?;
        validate_projection(&transaction, run_id).await?;
        let page = match query {
            ExactSearchQuery::Path(path) => {
                search_path(&transaction, run_id, snapshot_id, path, page_size, &guard).await?
            }
            ExactSearchQuery::Symbol(term) => {
                search_symbols(
                    &transaction,
                    run_id,
                    snapshot_id,
                    query,
                    term.as_str(),
                    page_size,
                    cursor,
                    &guard,
                )
                .await?
            }
            ExactSearchQuery::Role(ExactSearchRole::Manifest) => {
                search_manifests(
                    &transaction,
                    run_id,
                    snapshot_id,
                    query,
                    page_size,
                    cursor,
                    &guard,
                )
                .await?
            }
            ExactSearchQuery::Role(
                role @ (ExactSearchRole::Entrypoint | ExactSearchRole::Test),
            ) => {
                search_symbol_role(
                    &transaction,
                    run_id,
                    snapshot_id,
                    query,
                    *role,
                    page_size,
                    cursor,
                    &guard,
                )
                .await?
            }
        };
        guard.checkpoint()?;
        Ok(page)
    }
    .await;
    match result {
        Ok(page) => {
            transaction
                .commit()
                .await
                .map_err(ExactSearchRepositoryError::Commit)?;
            Ok(page)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn read_latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(IndexRunId, SnapshotId), ExactSearchRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    else {
        return Err(ExactSearchRepositoryError::IndexUnavailable);
    };
    let run_id = IndexRunId::from_bytes(read_search_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_search_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
        .is_some()
    {
        return Err(ExactSearchRepositoryError::InvalidStoredProjection);
    }
    Ok((run_id, snapshot_id))
}

fn validate_cursor(
    cursor: Option<&ExactSearchCursor>,
    query: &ExactSearchQuery,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
) -> Result<(), ExactSearchRepositoryError> {
    if cursor.is_some_and(|cursor| {
        cursor.query() != query
            || cursor.index_run_id() != run_id
            || cursor.snapshot_id() != snapshot_id
    }) {
        return Err(ExactSearchRepositoryError::InvalidCursor);
    }
    if matches!(query, ExactSearchQuery::Path(_)) && cursor.is_some() {
        return Err(ExactSearchRepositoryError::InvalidCursor);
    }
    Ok(())
}

async fn validate_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<(), ExactSearchRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT projection_version, symbol_count, manifest_count,\n\
             (SELECT COUNT(*) FROM symbols WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM exact_search_symbols WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM exact_search_manifests WHERE index_run_id = ?1)\n\
             FROM exact_search_projections WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    else {
        return Err(ExactSearchRepositoryError::ProjectionUnavailable);
    };
    let version = read_i64(&row, 0)?;
    let expected_symbols = read_i64(&row, 1)?;
    let expected_manifests = read_i64(&row, 2)?;
    let stored_symbols = read_i64(&row, 3)?;
    let projected_symbols = read_i64(&row, 4)?;
    let projected_manifests = read_i64(&row, 5)?;
    if version != PROJECTION_VERSION
        || expected_symbols < 0
        || expected_manifests < 0
        || expected_symbols != stored_symbols
        || expected_symbols != projected_symbols
        || expected_manifests != projected_manifests
        || rows
            .next()
            .await
            .map_err(ExactSearchRepositoryError::Read)?
            .is_some()
    {
        return Err(ExactSearchRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn search_path(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    path: &RepositoryPath,
    page_size: ExactSearchPageSize,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT repository_path, content_hash FROM file_revisions\n\
             WHERE index_run_id = ?1 AND repository_path = ?2 LIMIT 2",
            params![run_id.as_bytes().to_vec(), path.as_bytes().to_vec()],
        )
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    let mut hits = Vec::new();
    if let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        hits.push(
            ExactSearchHit::file(
                file_revision_from_row(&row, 0, 1)?,
                ExactSearchExplanation::NormalizedPathExact,
            )
            .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        );
    }
    if rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
        .is_some()
    {
        return Err(ExactSearchRepositoryError::InvalidStoredProjection);
    }
    ExactSearchPage::new(run_id, snapshot_id, hits, None, page_size)
        .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)
}

async fn search_manifests(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    page_size: ExactSearchPageSize,
    cursor: Option<&ExactSearchCursor>,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let after = match cursor.map(ExactSearchCursor::position) {
        None => None,
        Some(ExactSearchPosition::File(path)) => Some(path),
        Some(ExactSearchPosition::Symbol { .. }) => {
            return Err(ExactSearchRepositoryError::InvalidCursor);
        }
    };
    let mut sql = String::from(
        "SELECT repository_path, content_hash FROM exact_search_manifests\n\
         WHERE index_run_id = ?",
    );
    let mut parameters = vec![Value::Blob(run_id.as_bytes().to_vec())];
    if let Some(path) = after {
        sql.push_str(" AND repository_path > ?");
        parameters.push(Value::Blob(path.as_bytes().to_vec()));
    }
    sql.push_str(" ORDER BY repository_path LIMIT ?");
    parameters.push(Value::Integer(page_limit(page_size)?));
    let mut rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    let mut hits = Vec::with_capacity(usize::from(page_size.get()));
    let mut last_path = None;
    let mut has_more = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if hits.len() == usize::from(page_size.get()) {
            has_more = true;
            break;
        }
        let revision = file_revision_from_row(&row, 0, 1)?;
        last_path = Some(revision.path().clone());
        hits.push(
            ExactSearchHit::file(revision, ExactSearchExplanation::ManifestRole)
                .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        );
    }
    let next_cursor = if has_more {
        Some(
            ExactSearchCursor::new(
                run_id,
                snapshot_id,
                query.clone(),
                ExactSearchPosition::File(
                    last_path.ok_or(ExactSearchRepositoryError::InvalidStoredProjection)?,
                ),
            )
            .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        )
    } else {
        None
    };
    ExactSearchPage::new(run_id, snapshot_id, hits, next_cursor, page_size)
        .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)
}

#[allow(clippy::too_many_arguments)]
async fn search_symbol_role(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    role: ExactSearchRole,
    page_size: ExactSearchPageSize,
    cursor: Option<&ExactSearchCursor>,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let (role_bit, explanation) = match role {
        ExactSearchRole::Entrypoint => (2_i64, ExactSearchExplanation::EntrypointRole),
        ExactSearchRole::Test => (1_i64, ExactSearchExplanation::TestRole),
        ExactSearchRole::Manifest => return Err(ExactSearchRepositoryError::InvalidCursor),
    };
    let after = symbol_cursor_position(cursor, explanation)?;
    let mut sql = format!(
        "SELECT q.qualified_name, {SYMBOL_COLUMNS}\n\
         FROM exact_search_symbols AS q\n\
         JOIN symbols AS s ON s.index_run_id = q.index_run_id AND s.symbol_id = q.symbol_id\n\
         WHERE q.index_run_id = ? AND (s.roles & ?) <> 0"
    );
    let mut parameters = vec![
        Value::Blob(run_id.as_bytes().to_vec()),
        Value::Integer(role_bit),
    ];
    if let Some((_, path, qualified_name, symbol_id)) = after {
        sql.push_str(" AND (s.repository_path, q.qualified_name, s.symbol_id) > (?, ?, ?)");
        parameters.extend([
            Value::Blob(path.as_bytes().to_vec()),
            Value::Text(qualified_name.as_str().to_owned()),
            Value::Blob(symbol_id.as_bytes().to_vec()),
        ]);
    }
    sql.push_str(" ORDER BY s.repository_path, q.qualified_name, s.symbol_id LIMIT ?");
    parameters.push(Value::Integer(page_limit(page_size)?));
    let rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    read_symbol_rows(
        rows,
        run_id,
        snapshot_id,
        query,
        page_size,
        explanation,
        0,
        guard,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn search_symbols(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    term: &str,
    page_size: ExactSearchPageSize,
    cursor: Option<&ExactSearchCursor>,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let after = symbol_query_cursor_position(cursor)?;
    let upper = prefix_successor(term);
    let mut sql = String::from("WITH matches(match_order, symbol_id) AS (");
    let mut parameters = Vec::new();
    push_match_query(
        &mut sql,
        &mut parameters,
        0,
        "exact_search_symbols",
        "qualified_name",
        "=",
        run_id,
        term,
        None,
        false,
    );
    push_match_query(
        &mut sql,
        &mut parameters,
        1,
        "symbols",
        "name",
        "=",
        run_id,
        term,
        None,
        true,
    );
    push_match_query(
        &mut sql,
        &mut parameters,
        2,
        "symbols",
        "signature",
        "=",
        run_id,
        term,
        None,
        true,
    );
    push_match_query(
        &mut sql,
        &mut parameters,
        3,
        "exact_search_symbols",
        "qualified_name",
        ">=",
        run_id,
        term,
        upper.as_deref(),
        true,
    );
    push_match_query(
        &mut sql,
        &mut parameters,
        4,
        "symbols",
        "name",
        ">=",
        run_id,
        term,
        upper.as_deref(),
        true,
    );
    push_match_query(
        &mut sql,
        &mut parameters,
        5,
        "symbols",
        "signature",
        ">=",
        run_id,
        term,
        upper.as_deref(),
        true,
    );
    sql.push_str(
        "), best(match_order, symbol_id) AS (\n\
         SELECT MIN(match_order), symbol_id FROM matches GROUP BY symbol_id)\n\
         SELECT best.match_order, q.qualified_name, ",
    );
    sql.push_str(SYMBOL_COLUMNS);
    sql.push_str(
        " FROM best\n\
         JOIN symbols AS s ON s.index_run_id = ? AND s.symbol_id = best.symbol_id\n\
         JOIN exact_search_symbols AS q\n\
           ON q.index_run_id = s.index_run_id AND q.symbol_id = s.symbol_id",
    );
    parameters.push(Value::Blob(run_id.as_bytes().to_vec()));
    if let Some((explanation, path, qualified_name, symbol_id)) = after {
        sql.push_str(
            " WHERE (best.match_order, s.repository_path, q.qualified_name, s.symbol_id)\n\
             > (?, ?, ?, ?)",
        );
        parameters.extend([
            Value::Integer(i64::from(explanation.sort_order())),
            Value::Blob(path.as_bytes().to_vec()),
            Value::Text(qualified_name.as_str().to_owned()),
            Value::Blob(symbol_id.as_bytes().to_vec()),
        ]);
    }
    sql.push_str(
        " ORDER BY best.match_order, s.repository_path, q.qualified_name, s.symbol_id LIMIT ?",
    );
    parameters.push(Value::Integer(page_limit(page_size)?));
    let rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(ExactSearchRepositoryError::Read)?;
    read_ranked_symbol_rows(rows, run_id, snapshot_id, query, page_size, guard).await
}

#[allow(clippy::too_many_arguments)]
fn push_match_query(
    sql: &mut String,
    parameters: &mut Vec<Value>,
    rank: i64,
    table: &str,
    field: &str,
    comparison: &str,
    run_id: IndexRunId,
    lower: &str,
    upper: Option<&str>,
    union: bool,
) {
    if union {
        sql.push_str(" UNION ALL ");
    }
    sql.push_str(&format!(
        "SELECT {rank}, symbol_id FROM {table} WHERE index_run_id = ? AND {field} {comparison} ?"
    ));
    parameters.extend([
        Value::Blob(run_id.as_bytes().to_vec()),
        Value::Text(lower.to_owned()),
    ]);
    if comparison == ">="
        && let Some(upper) = upper
    {
        sql.push_str(&format!(" AND {field} < ?"));
        parameters.push(Value::Text(upper.to_owned()));
    }
}

async fn read_ranked_symbol_rows(
    mut rows: libsql::Rows,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    page_size: ExactSearchPageSize,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let mut hits = Vec::with_capacity(usize::from(page_size.get()));
    let mut last_position = None;
    let mut has_more = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if hits.len() == usize::from(page_size.get()) {
            has_more = true;
            break;
        }
        let rank = read_i64(&row, 0)?;
        let explanation = explanation_from_rank(rank)?;
        let qualified_name = read_qualified_name(&row, 1)?;
        let symbol = index_codec::graph_symbol_from_row(&row, 2).map_err(map_decode_error)?;
        last_position = Some(symbol_position(&symbol, &qualified_name, explanation));
        hits.push(
            ExactSearchHit::symbol(ExactSearchSymbol::new(symbol, qualified_name), explanation)
                .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        );
    }
    finish_symbol_page(
        run_id,
        snapshot_id,
        query,
        page_size,
        hits,
        last_position,
        has_more,
    )
}

#[allow(clippy::too_many_arguments)]
async fn read_symbol_rows(
    mut rows: libsql::Rows,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    page_size: ExactSearchPageSize,
    explanation: ExactSearchExplanation,
    qualified_name_offset: i32,
    guard: &SearchGuard<'_>,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let mut hits = Vec::with_capacity(usize::from(page_size.get()));
    let mut last_position = None;
    let mut has_more = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ExactSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if hits.len() == usize::from(page_size.get()) {
            has_more = true;
            break;
        }
        let qualified_name = read_qualified_name(&row, qualified_name_offset)?;
        let symbol = index_codec::graph_symbol_from_row(&row, qualified_name_offset + 1)
            .map_err(map_decode_error)?;
        last_position = Some(symbol_position(&symbol, &qualified_name, explanation));
        hits.push(
            ExactSearchHit::symbol(ExactSearchSymbol::new(symbol, qualified_name), explanation)
                .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        );
    }
    finish_symbol_page(
        run_id,
        snapshot_id,
        query,
        page_size,
        hits,
        last_position,
        has_more,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_symbol_page(
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: &ExactSearchQuery,
    page_size: ExactSearchPageSize,
    hits: Vec<ExactSearchHit>,
    last_position: Option<ExactSearchPosition>,
    has_more: bool,
) -> Result<ExactSearchPage, ExactSearchRepositoryError> {
    let next_cursor = if has_more {
        Some(
            ExactSearchCursor::new(
                run_id,
                snapshot_id,
                query.clone(),
                last_position.ok_or(ExactSearchRepositoryError::InvalidStoredProjection)?,
            )
            .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        )
    } else {
        None
    };
    ExactSearchPage::new(run_id, snapshot_id, hits, next_cursor, page_size)
        .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)
}

fn symbol_position(
    symbol: &a3_domain::GraphSymbol,
    qualified_name: &QualifiedSymbolName,
    explanation: ExactSearchExplanation,
) -> ExactSearchPosition {
    ExactSearchPosition::Symbol {
        explanation,
        path: symbol.revision().path().clone(),
        qualified_name: qualified_name.clone(),
        symbol_id: symbol.id(),
    }
}

fn symbol_cursor_position(
    cursor: Option<&ExactSearchCursor>,
    expected: ExactSearchExplanation,
) -> Result<Option<SymbolCursorPosition<'_>>, ExactSearchRepositoryError> {
    match cursor.map(ExactSearchCursor::position) {
        None => Ok(None),
        Some(ExactSearchPosition::Symbol {
            explanation,
            path,
            qualified_name,
            symbol_id,
        }) if *explanation == expected => {
            Ok(Some((*explanation, path, qualified_name, *symbol_id)))
        }
        Some(_) => Err(ExactSearchRepositoryError::InvalidCursor),
    }
}

fn symbol_query_cursor_position(
    cursor: Option<&ExactSearchCursor>,
) -> Result<Option<SymbolCursorPosition<'_>>, ExactSearchRepositoryError> {
    let position = symbol_cursor_position_from_any(cursor)?;
    if position.as_ref().is_some_and(|(explanation, ..)| {
        !matches!(
            explanation,
            ExactSearchExplanation::QualifiedNameExact
                | ExactSearchExplanation::SymbolNameExact
                | ExactSearchExplanation::SignatureExact
                | ExactSearchExplanation::QualifiedNamePrefix
                | ExactSearchExplanation::SymbolNamePrefix
                | ExactSearchExplanation::SignaturePrefix
        )
    }) {
        return Err(ExactSearchRepositoryError::InvalidCursor);
    }
    Ok(position)
}

fn symbol_cursor_position_from_any(
    cursor: Option<&ExactSearchCursor>,
) -> Result<Option<SymbolCursorPosition<'_>>, ExactSearchRepositoryError> {
    match cursor.map(ExactSearchCursor::position) {
        None => Ok(None),
        Some(ExactSearchPosition::Symbol {
            explanation,
            path,
            qualified_name,
            symbol_id,
        }) => Ok(Some((*explanation, path, qualified_name, *symbol_id))),
        Some(ExactSearchPosition::File(_)) => Err(ExactSearchRepositoryError::InvalidCursor),
    }
}

type SymbolCursorPosition<'a> = (
    ExactSearchExplanation,
    &'a RepositoryPath,
    &'a QualifiedSymbolName,
    SymbolId,
);

fn explanation_from_rank(rank: i64) -> Result<ExactSearchExplanation, ExactSearchRepositoryError> {
    match rank {
        0 => Ok(ExactSearchExplanation::QualifiedNameExact),
        1 => Ok(ExactSearchExplanation::SymbolNameExact),
        2 => Ok(ExactSearchExplanation::SignatureExact),
        3 => Ok(ExactSearchExplanation::QualifiedNamePrefix),
        4 => Ok(ExactSearchExplanation::SymbolNamePrefix),
        5 => Ok(ExactSearchExplanation::SignaturePrefix),
        _ => Err(ExactSearchRepositoryError::InvalidStoredProjection),
    }
}

fn prefix_successor(value: &str) -> Option<String> {
    let mut characters = value.chars().collect::<Vec<_>>();
    while let Some(last) = characters.pop() {
        let code = u32::from(last);
        let next = if code == 0xD7FF { 0xE000 } else { code + 1 };
        if let Some(next) = char::from_u32(next) {
            characters.push(next);
            return Some(characters.into_iter().collect());
        }
    }
    None
}

fn file_revision_from_row(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
) -> Result<FileRevision, ExactSearchRepositoryError> {
    let path: Vec<u8> = row
        .get(path_index)
        .map_err(ExactSearchRepositoryError::Read)?;
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path)
            .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)?,
        ContentHash::from_bytes(read_search_id(row, hash_index)?),
    ))
}

fn read_qualified_name(
    row: &libsql::Row,
    index: i32,
) -> Result<QualifiedSymbolName, ExactSearchRepositoryError> {
    let value: String = row.get(index).map_err(ExactSearchRepositoryError::Read)?;
    QualifiedSymbolName::try_from_string(value)
        .map_err(|_| ExactSearchRepositoryError::InvalidStoredProjection)
}

fn read_search_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ExactSearchRepositoryError> {
    read_stable_id(row, index).map_err(map_decode_error)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, ExactSearchRepositoryError> {
    row.get(index).map_err(ExactSearchRepositoryError::Read)
}

fn page_limit(page_size: ExactSearchPageSize) -> Result<i64, ExactSearchRepositoryError> {
    i64::from(page_size.get())
        .checked_add(1)
        .ok_or(ExactSearchRepositoryError::InvalidStoredProjection)
}

fn map_decode_error(error: IndexPublicationRepositoryError) -> ExactSearchRepositoryError {
    match error {
        IndexPublicationRepositoryError::Read(source) => ExactSearchRepositoryError::Read(source),
        _ => ExactSearchRepositoryError::InvalidStoredProjection,
    }
}

struct SearchGuard<'a> {
    control: &'a dyn ExactSearchControl,
    started: Instant,
}

impl<'a> SearchGuard<'a> {
    fn new(control: &'a dyn ExactSearchControl) -> Result<Self, ExactSearchRepositoryError> {
        let guard = Self {
            control,
            started: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ExactSearchRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ExactSearchRepositoryError::Cancelled);
        }
        if self.started.elapsed() > MAX_SEARCH_DURATION {
            return Err(ExactSearchRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ExactSearchRepositoryError,
) -> Result<T, ExactSearchRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ExactSearchRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum ExactSearchRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    IndexUnavailable,
    InvalidCursor,
    ProjectionUnavailable,
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl ExactSearchRepositoryError {
    pub(crate) fn classify(&self) -> KnowledgeSearchFailure {
        match self {
            Self::IndexUnavailable => KnowledgeSearchFailure::IndexUnavailable,
            Self::InvalidCursor => KnowledgeSearchFailure::InvalidCursor,
            Self::ProjectionUnavailable => KnowledgeSearchFailure::ProjectionUnavailable,
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

impl fmt::Display for ExactSearchRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Begin(_) => "could not begin exact-search transaction",
            Self::Read(_) => "could not read exact-search projection",
            Self::Commit(_) => "could not close exact-search transaction",
            Self::Rollback(_) => "could not roll back exact-search transaction",
            Self::IndexUnavailable => "no published index is available",
            Self::InvalidCursor => "exact-search cursor is invalid",
            Self::ProjectionUnavailable => "exact-search projection is unavailable",
            Self::InvalidStoredProjection => "stored exact-search projection is invalid",
            Self::Cancelled => "exact search was cancelled",
            Self::TimedOut => "exact search timed out",
        };
        formatter.write_str(message)
    }
}

impl Error for ExactSearchRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::IndexUnavailable
            | Self::InvalidCursor
            | Self::ProjectionUnavailable
            | Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prefix_successor;

    #[test]
    fn prefix_successor_handles_unicode_scalar_boundaries() {
        assert_eq!(prefix_successor("abc").as_deref(), Some("abd"));
        assert_eq!(prefix_successor("a\u{d7ff}").as_deref(), Some("a\u{e000}"));
        assert_eq!(prefix_successor("\u{10ffff}"), None);
    }
}
