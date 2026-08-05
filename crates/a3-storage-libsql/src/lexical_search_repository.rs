use crate::catalog::is_corruption;
use crate::index_codec;
use crate::index_publication::{IndexPublicationRepositoryError, read_stable_id};
use crate::lexical_search_projection::searchable_path;
use a3_application::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeStoreFailure};
use a3_domain::{
    ContentHash, ExactSearchSymbol, FileRevision, IndexRunId, LexicalScore, LexicalSearchCursor,
    LexicalSearchExplanation, LexicalSearchHit, LexicalSearchPage, LexicalSearchPageSize,
    LexicalSearchPosition, LexicalSearchQuery, LexicalSearchTarget, QualifiedSymbolName,
    RepositoryPath, SnapshotId, SourceChannel, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_SEARCH_DURATION: Duration = Duration::from_secs(2);
const MAX_FTS_CANDIDATES_PER_CLASS: i64 = 512;
const PROJECTION_VERSION: i64 = 1;
const MINIMUM_SCORE: u32 = 10_000;
const SYMBOL_COLUMNS: &str = "s.symbol_id, s.repository_path, s.content_hash, s.local_symbol_id,\n\
 s.kind, s.name, s.signature, s.declaration_start_byte, s.declaration_end_byte,\n\
 s.declaration_start_row, s.declaration_start_column, s.declaration_end_row,\n\
 s.declaration_end_column, s.selection_start_byte, s.selection_end_byte,\n\
 s.selection_start_row, s.selection_start_column, s.selection_end_row,\n\
 s.selection_end_column, s.documentation_start_byte, s.documentation_end_byte,\n\
 s.documentation_start_row, s.documentation_start_column, s.documentation_end_row,\n\
 s.documentation_end_column, s.visibility, s.roles";

pub(crate) async fn search_lexical(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &LexicalSearchQuery,
    page_size: LexicalSearchPageSize,
    cursor: Option<&LexicalSearchCursor>,
    control: &dyn KnowledgeSearchControl,
) -> Result<LexicalSearchPage, LexicalSearchRepositoryError> {
    let guard = SearchGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(LexicalSearchRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let (run_id, snapshot_id) = read_latest_publication(&transaction, worktree_id).await?;
        validate_cursor(cursor, query, run_id, snapshot_id)?;
        validate_projection(&transaction, run_id).await?;
        let terms = query_terms(query.term().as_str());
        let expression =
            fts_expression(&terms).ok_or(LexicalSearchRepositoryError::InvalidStoredProjection)?;
        let mut candidates =
            read_symbol_candidates(&transaction, run_id, &expression, &terms, &guard).await?;
        candidates
            .extend(read_path_candidates(&transaction, run_id, &expression, &terms, &guard).await?);
        candidates.sort_by(compare_candidates);
        let start = cursor_start(&candidates, cursor)?;
        let requested = usize::from(page_size.get());
        let end = start.saturating_add(requested).min(candidates.len());
        let has_more = end < candidates.len();
        let page_candidates = &candidates[start..end];
        let hits = page_candidates
            .iter()
            .map(|candidate| candidate.hit.clone())
            .collect::<Vec<_>>();
        let next_cursor = if has_more {
            page_candidates.last().map(|candidate| {
                LexicalSearchCursor::new(
                    run_id,
                    snapshot_id,
                    query.clone(),
                    candidate.position.clone(),
                )
            })
        } else {
            None
        };
        guard.checkpoint()?;
        LexicalSearchPage::new(run_id, snapshot_id, hits, next_cursor, page_size)
            .map_err(|_| LexicalSearchRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(page) => {
            transaction
                .commit()
                .await
                .map_err(LexicalSearchRepositoryError::Commit)?;
            Ok(page)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn read_latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(IndexRunId, SnapshotId), LexicalSearchRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(LexicalSearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(LexicalSearchRepositoryError::Read)?
    else {
        return Err(LexicalSearchRepositoryError::IndexUnavailable);
    };
    let run_id = IndexRunId::from_bytes(read_search_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_search_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(LexicalSearchRepositoryError::Read)?
        .is_some()
    {
        return Err(LexicalSearchRepositoryError::InvalidStoredProjection);
    }
    Ok((run_id, snapshot_id))
}

fn validate_cursor(
    cursor: Option<&LexicalSearchCursor>,
    query: &LexicalSearchQuery,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
) -> Result<(), LexicalSearchRepositoryError> {
    if cursor.is_some_and(|cursor| {
        cursor.query() != query
            || cursor.index_run_id() != run_id
            || cursor.snapshot_id() != snapshot_id
    }) {
        return Err(LexicalSearchRepositoryError::InvalidCursor);
    }
    Ok(())
}

async fn validate_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<(), LexicalSearchRepositoryError> {
    let parameter = run_id.as_bytes().to_vec();
    let mut rows = transaction
        .query(
            "SELECT projection_version, symbol_count, path_count, card_count,\n\
             (SELECT COUNT(*) FROM symbols WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM file_revisions WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM symbol_fts WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM path_fts WHERE index_run_id = ?1),\n\
             (SELECT COUNT(*) FROM card_fts WHERE index_run_id = ?1)\n\
             FROM lexical_search_projections WHERE index_run_id = ?1",
            [parameter],
        )
        .await
        .map_err(LexicalSearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(LexicalSearchRepositoryError::Read)?
    else {
        return Err(LexicalSearchRepositoryError::ProjectionUnavailable);
    };
    let values = (0..9)
        .map(|index| read_i64(&row, index))
        .collect::<Result<Vec<_>, _>>()?;
    if values[0] != PROJECTION_VERSION
        || values[1..].iter().any(|value| *value < 0)
        || values[1] != values[4]
        || values[2] != values[5]
        || values[1] != values[6]
        || values[2] != values[7]
        || values[3] != values[8]
        || values[3] != 0
        || rows
            .next()
            .await
            .map_err(LexicalSearchRepositoryError::Read)?
            .is_some()
    {
        return Err(LexicalSearchRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn read_symbol_candidates(
    transaction: &Transaction,
    run_id: IndexRunId,
    expression: &str,
    query: &[String],
    guard: &SearchGuard<'_>,
) -> Result<Vec<Candidate>, LexicalSearchRepositoryError> {
    let sql = format!(
        "SELECT f.repository_path, f.qualified_name, f.name, f.signature, q.qualified_name,\n\
         {SYMBOL_COLUMNS}\n\
         FROM symbol_fts AS f\n\
         JOIN symbols AS s ON s.index_run_id = f.index_run_id AND s.symbol_id = f.symbol_id\n\
         JOIN exact_search_symbols AS q\n\
           ON q.index_run_id = f.index_run_id AND q.symbol_id = f.symbol_id\n\
         WHERE f.index_run_id = ?1 AND symbol_fts MATCH ?2\n\
         ORDER BY bm25(symbol_fts, 0.0, 0.0, 4.0, 8.0, 10.0, 6.0), f.rowid\n\
         LIMIT {MAX_FTS_CANDIDATES_PER_CLASS}"
    );
    let mut rows = transaction
        .query(
            &sql,
            params![run_id.as_bytes().to_vec(), expression.to_owned()],
        )
        .await
        .map_err(LexicalSearchRepositoryError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(LexicalSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let projected_path: String = row.get(0).map_err(LexicalSearchRepositoryError::Read)?;
        let projected_qualified: String = row.get(1).map_err(LexicalSearchRepositoryError::Read)?;
        let projected_name: String = row.get(2).map_err(LexicalSearchRepositoryError::Read)?;
        let projected_signature: String = row.get(3).map_err(LexicalSearchRepositoryError::Read)?;
        let qualified_name = read_qualified_name(&row, 4)?;
        let symbol = index_codec::graph_symbol_from_row(&row, 5).map_err(map_decode_error)?;
        let stored_signature = symbol
            .parsed()
            .signature()
            .map_or("", |signature| signature.as_str());
        if projected_path != searchable_path(symbol.revision().path())
            || projected_qualified != qualified_name.as_str()
            || projected_name != symbol.parsed().name().as_str()
            || projected_signature != stored_signature
        {
            return Err(LexicalSearchRepositoryError::InvalidStoredProjection);
        }
        let fields = [
            (
                LexicalSearchExplanation::Path,
                projected_path.as_str(),
                4_u32,
            ),
            (
                LexicalSearchExplanation::Signature,
                projected_signature.as_str(),
                6,
            ),
            (
                LexicalSearchExplanation::QualifiedName,
                projected_qualified.as_str(),
                8,
            ),
            (
                LexicalSearchExplanation::SymbolName,
                projected_name.as_str(),
                10,
            ),
        ];
        let Some((score, explanation)) = best_weighted_score(query, &fields) else {
            continue;
        };
        let score = LexicalScore::new(score)
            .map_err(|_| LexicalSearchRepositoryError::InvalidStoredProjection)?;
        let symbol_id = symbol.id();
        let path = symbol.revision().path().clone();
        let hit = LexicalSearchHit::symbol(
            ExactSearchSymbol::new(symbol, qualified_name.clone()),
            explanation,
            score,
        );
        candidates.push(Candidate {
            hit,
            position: LexicalSearchPosition::Symbol {
                score,
                path,
                qualified_name,
                symbol_id,
            },
        });
    }
    Ok(candidates)
}

async fn read_path_candidates(
    transaction: &Transaction,
    run_id: IndexRunId,
    expression: &str,
    query: &[String],
    guard: &SearchGuard<'_>,
) -> Result<Vec<Candidate>, LexicalSearchRepositoryError> {
    let sql = format!(
        "SELECT f.repository_path, f.path, revisions.content_hash\n\
         FROM path_fts AS f\n\
         JOIN file_revisions AS revisions\n\
           ON revisions.index_run_id = f.index_run_id\n\
          AND revisions.repository_path = f.repository_path\n\
         WHERE f.index_run_id = ?1 AND path_fts MATCH ?2\n\
         ORDER BY bm25(path_fts, 0.0, 0.0, 10.0), f.rowid\n\
         LIMIT {MAX_FTS_CANDIDATES_PER_CLASS}"
    );
    let mut rows = transaction
        .query(
            &sql,
            params![run_id.as_bytes().to_vec(), expression.to_owned()],
        )
        .await
        .map_err(LexicalSearchRepositoryError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(LexicalSearchRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let raw_path: Vec<u8> = row.get(0).map_err(LexicalSearchRepositoryError::Read)?;
        let path_text: String = row.get(1).map_err(LexicalSearchRepositoryError::Read)?;
        let path = RepositoryPath::try_from_bytes(raw_path)
            .map_err(|_| LexicalSearchRepositoryError::InvalidStoredProjection)?;
        if searchable_path(&path) != path_text {
            return Err(LexicalSearchRepositoryError::InvalidStoredProjection);
        }
        let Some(score) = field_score(query, &path_text)
            .and_then(|base| base.checked_mul(4))
            .filter(|score| *score >= MINIMUM_SCORE)
        else {
            continue;
        };
        let score = LexicalScore::new(score)
            .map_err(|_| LexicalSearchRepositoryError::InvalidStoredProjection)?;
        let revision = FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(read_search_id(&row, 2)?),
        );
        candidates.push(Candidate {
            hit: LexicalSearchHit::file(revision, score),
            position: LexicalSearchPosition::File { score, path },
        });
    }
    Ok(candidates)
}

fn query_terms(query: &str) -> Vec<String> {
    normalized_tokens(query)
        .into_iter()
        .filter(|term| term.chars().count() >= 3)
        .collect()
}

fn normalized_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' {
            current.push(character);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn fts_expression(terms: &[String]) -> Option<String> {
    let expressions = terms
        .iter()
        .filter_map(|term| term_expression(term))
        .collect::<BTreeSet<_>>();
    if expressions.is_empty() {
        return None;
    }
    Some(expressions.into_iter().collect::<Vec<_>>().join(" OR "))
}

fn term_expression(term: &str) -> Option<String> {
    const MAX_SELECTED_TRIGRAMS: usize = 8;
    let characters = term.chars().collect::<Vec<_>>();
    let mut selected = characters
        .chunks_exact(3)
        .take(MAX_SELECTED_TRIGRAMS.saturating_sub(1))
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>();
    if characters.len() > 3 && selected.len() < MAX_SELECTED_TRIGRAMS {
        let final_trigram = characters[characters.len() - 3..]
            .iter()
            .collect::<String>();
        if selected.last() != Some(&final_trigram) {
            selected.push(final_trigram);
        }
    }
    let selected = selected
        .into_iter()
        .map(|trigram| format!("\"{trigram}\""))
        .collect::<Vec<_>>();
    match selected.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        [first, second] => Some(format!("({first} OR {second})")),
        _ => Some(
            (0..selected.len())
                .map(|omitted| {
                    format!(
                        "({})",
                        selected
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| *index != omitted)
                            .map(|(_, trigram)| trigram.as_str())
                            .collect::<Vec<_>>()
                            .join(" AND ")
                    )
                })
                .collect::<Vec<_>>()
                .join(" OR "),
        ),
    }
}

fn trigrams(value: &str) -> BTreeSet<String> {
    let characters = value.chars().collect::<Vec<_>>();
    characters
        .windows(3)
        .map(|window| window.iter().collect())
        .collect()
}

fn best_weighted_score(
    query: &[String],
    fields: &[(LexicalSearchExplanation, &str, u32)],
) -> Option<(u32, LexicalSearchExplanation)> {
    fields
        .iter()
        .filter_map(|(explanation, field, weight)| {
            field_score(query, field)
                .and_then(|score| score.checked_mul(*weight))
                .filter(|score| *score >= MINIMUM_SCORE)
                .map(|score| (score, *explanation))
        })
        .max_by_key(|(score, _)| *score)
}

fn field_score(query: &[String], field: &str) -> Option<u32> {
    let field_tokens = normalized_tokens(field)
        .into_iter()
        .filter(|token| token.chars().count() >= 3)
        .collect::<Vec<_>>();
    query
        .iter()
        .flat_map(|query| {
            field_tokens
                .iter()
                .map(move |field| token_score(query, field))
        })
        .max()
}

fn token_score(query: &str, field: &str) -> u32 {
    if query == field {
        return 10_000;
    }
    if field.starts_with(query) {
        return 9_500;
    }
    if field.contains(query) {
        return 9_000;
    }
    let query_trigrams = trigrams(query);
    let field_trigrams = trigrams(field);
    let denominator = query_trigrams.len().saturating_add(field_trigrams.len());
    if denominator == 0 {
        return 0;
    }
    let intersection = query_trigrams.intersection(&field_trigrams).count();
    u32::try_from(intersection.saturating_mul(20_000) / denominator).map_or(0, |score| score)
}

#[derive(Clone)]
struct Candidate {
    hit: LexicalSearchHit,
    position: LexicalSearchPosition,
}

fn compare_candidates(left: &Candidate, right: &Candidate) -> Ordering {
    right
        .hit
        .score()
        .cmp(&left.hit.score())
        .then_with(|| target_kind(&left.hit).cmp(&target_kind(&right.hit)))
        .then_with(|| {
            left.hit
                .target()
                .revision()
                .path()
                .cmp(right.hit.target().revision().path())
        })
        .then_with(|| symbol_tie_breaker(&left.hit).cmp(&symbol_tie_breaker(&right.hit)))
}

fn target_kind(hit: &LexicalSearchHit) -> u8 {
    match hit.target() {
        LexicalSearchTarget::File(_) => 0,
        LexicalSearchTarget::Symbol(_) => 1,
    }
}

fn symbol_tie_breaker(
    hit: &LexicalSearchHit,
) -> Option<(&QualifiedSymbolName, a3_domain::SymbolId)> {
    match hit.target() {
        LexicalSearchTarget::File(_) => None,
        LexicalSearchTarget::Symbol(symbol) => {
            Some((symbol.qualified_name(), symbol.symbol().id()))
        }
    }
}

fn cursor_start(
    candidates: &[Candidate],
    cursor: Option<&LexicalSearchCursor>,
) -> Result<usize, LexicalSearchRepositoryError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    candidates
        .iter()
        .position(|candidate| &candidate.position == cursor.position())
        .and_then(|position| position.checked_add(1))
        .ok_or(LexicalSearchRepositoryError::InvalidCursor)
}

fn read_qualified_name(
    row: &libsql::Row,
    index: i32,
) -> Result<QualifiedSymbolName, LexicalSearchRepositoryError> {
    let value: String = row.get(index).map_err(LexicalSearchRepositoryError::Read)?;
    QualifiedSymbolName::try_from_string(value)
        .map_err(|_| LexicalSearchRepositoryError::InvalidStoredProjection)
}

fn read_search_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], LexicalSearchRepositoryError> {
    read_stable_id(row, index).map_err(map_decode_error)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, LexicalSearchRepositoryError> {
    row.get(index).map_err(LexicalSearchRepositoryError::Read)
}

fn map_decode_error(error: IndexPublicationRepositoryError) -> LexicalSearchRepositoryError {
    match error {
        IndexPublicationRepositoryError::Read(source) => LexicalSearchRepositoryError::Read(source),
        _ => LexicalSearchRepositoryError::InvalidStoredProjection,
    }
}

struct SearchGuard<'a> {
    control: &'a dyn KnowledgeSearchControl,
    started: Instant,
}

impl<'a> SearchGuard<'a> {
    fn new(control: &'a dyn KnowledgeSearchControl) -> Result<Self, LexicalSearchRepositoryError> {
        let guard = Self {
            control,
            started: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), LexicalSearchRepositoryError> {
        if self.control.is_cancelled() {
            return Err(LexicalSearchRepositoryError::Cancelled);
        }
        if self.started.elapsed() > MAX_SEARCH_DURATION {
            return Err(LexicalSearchRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: LexicalSearchRepositoryError,
) -> Result<T, LexicalSearchRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(LexicalSearchRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum LexicalSearchRepositoryError {
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

impl LexicalSearchRepositoryError {
    pub(crate) fn classify(&self) -> KnowledgeSearchFailure {
        match self {
            Self::IndexUnavailable => KnowledgeSearchFailure::IndexUnavailable,
            Self::InvalidCursor => KnowledgeSearchFailure::InvalidCursor,
            Self::ProjectionUnavailable => {
                KnowledgeSearchFailure::ProjectionUnavailable(SourceChannel::Lexical)
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

impl fmt::Display for LexicalSearchRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Begin(_) => "could not begin lexical-search transaction",
            Self::Read(_) => "could not read lexical-search projection",
            Self::Commit(_) => "could not close lexical-search transaction",
            Self::Rollback(_) => "could not roll back lexical-search transaction",
            Self::IndexUnavailable => "no published index is available",
            Self::InvalidCursor => "lexical-search cursor is invalid",
            Self::ProjectionUnavailable => "lexical-search projection is unavailable",
            Self::InvalidStoredProjection => "stored lexical-search projection is invalid",
            Self::Cancelled => "lexical search was cancelled",
            Self::TimedOut => "lexical search timed out",
        };
        formatter.write_str(message)
    }
}

impl Error for LexicalSearchRepositoryError {
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
    use super::{fts_expression, query_terms, token_score};

    #[test]
    fn fts_expression_contains_only_derived_quoted_trigrams() {
        let expression = fts_expression(&query_terms("launch' OR 1=1 --"));
        assert_eq!(expression.as_deref(), Some("(\"lau\" OR \"nch\")"));
    }

    #[test]
    fn long_term_allows_one_disjoint_trigram_to_differ() {
        let expression = fts_expression(&query_terms("function_4999x"));
        assert_eq!(
            expression.as_deref(),
            Some(
                "(\"cti\" AND \"on_\" AND \"499\" AND \"99x\") OR (\"fun\" AND \"on_\" AND \"499\" AND \"99x\") OR (\"fun\" AND \"cti\" AND \"499\" AND \"99x\") OR (\"fun\" AND \"cti\" AND \"on_\" AND \"99x\") OR (\"fun\" AND \"cti\" AND \"on_\" AND \"499\")"
            )
        );
    }

    #[test]
    fn typo_score_is_deterministic() {
        assert_eq!(token_score("launcj", "launch"), 7_500);
        assert_eq!(token_score("launch", "launch"), 10_000);
    }
}
