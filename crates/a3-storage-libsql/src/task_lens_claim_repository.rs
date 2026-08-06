use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, TaskLensClaimResult, TaskLensClaimStoreFailure, TaskLensControl,
};
use a3_domain::{
    Confidence, GraphEndpoint, IndexRunId, ModuleCardClaimId, ModuleCardEvidenceId,
    ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimStatement, ModuleId, PublishedIndex,
    RepositoryPath, ResolvedModuleCardEvidence, SnapshotId, SymbolId, SyntaxRelationKind,
    TaskLensClaim, VerifiedClaimKind, VerifiedClaimStatus, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_CLAIM_READ_DURATION: Duration = Duration::from_secs(2);
const MAX_EVIDENCE_PER_CLAIM: usize = 16;

pub(crate) async fn load_claims(
    connection: &Connection,
    worktree_id: WorktreeId,
    published: &PublishedIndex,
    limit: u16,
    control: &dyn TaskLensControl,
) -> Result<TaskLensClaimResult, TaskLensClaimRepositoryError> {
    let guard = ClaimReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(TaskLensClaimRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        validate_latest_publication(&transaction, worktree_id, published).await?;
        read_claim_projection(&transaction, published, None, limit, &guard).await
    }
    .await;
    match result {
        Ok(claims) => {
            transaction
                .commit()
                .await
                .map_err(TaskLensClaimRepositoryError::Commit)?;
            Ok(claims)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

pub(crate) async fn load_claim(
    connection: &Connection,
    worktree_id: WorktreeId,
    published: &PublishedIndex,
    claim_id: ModuleCardClaimId,
    control: &dyn TaskLensControl,
) -> Result<Option<TaskLensClaim>, TaskLensClaimRepositoryError> {
    let guard = ClaimReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(TaskLensClaimRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        validate_latest_publication(&transaction, worktree_id, published).await?;
        let claims =
            read_claim_projection(&transaction, published, Some(claim_id), 1, &guard).await?;
        if claims.truncated() {
            return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
        }
        Ok(claims.claims().first().cloned())
    }
    .await;
    match result {
        Ok(claim) => {
            transaction
                .commit()
                .await
                .map_err(TaskLensClaimRepositoryError::Commit)?;
            Ok(claim)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn read_claim_projection(
    transaction: &Transaction,
    published: &PublishedIndex,
    claim_id: Option<ModuleCardClaimId>,
    limit: u16,
    guard: &ClaimReadGuard<'_>,
) -> Result<TaskLensClaimResult, TaskLensClaimRepositoryError> {
    let (rows, truncated) = read_claim_rows(transaction, published, claim_id, limit, guard).await?;
    if rows.is_empty() {
        guard.checkpoint()?;
        return TaskLensClaimResult::new(Vec::new(), truncated)
            .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection);
    }
    let evidence_by_claim = read_claim_evidence(transaction, &rows, guard).await?;
    let evidence = evidence_projection(published, &rows, &evidence_by_claim)?;
    let mut claims = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        if index.is_multiple_of(16) {
            guard.checkpoint()?;
        }
        claims.push(row.resolve(published, &evidence_by_claim, &evidence)?);
    }
    guard.checkpoint()?;
    TaskLensClaimResult::new(claims, truncated)
        .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)
}

async fn validate_latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    published: &PublishedIndex,
) -> Result<(), TaskLensClaimRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?
    else {
        return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
    };
    let run_id = IndexRunId::from_bytes(read_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_id(&row, 1)?);
    if run_id != published.run().id() || snapshot_id != published.run().snapshot_id() {
        return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn read_claim_rows(
    transaction: &Transaction,
    published: &PublishedIndex,
    claim_id: Option<ModuleCardClaimId>,
    limit: u16,
    guard: &ClaimReadGuard<'_>,
) -> Result<(Vec<StoredClaimRow>, bool), TaskLensClaimRepositoryError> {
    let query_limit = i64::from(limit)
        .checked_add(1)
        .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
    let mut rows = transaction
        .query(
            "WITH ranked_cards AS (\n\
               SELECT m.source_index_run_id, m.card_id, m.module_id, lifecycle.status,\n\
                 ROW_NUMBER() OVER (\n\
                   PARTITION BY m.module_id ORDER BY\n\
                     snapshots.generation DESC,\n\
                     CASE WHEN m.source_index_run_id = ?1 THEN 1 ELSE 0 END DESC,\n\
                     COALESCE(runs.run_sequence, 0) DESC, m.card_id DESC\n\
                 ) AS card_rank\n\
               FROM module_cards m\n\
               JOIN module_card_lifecycle lifecycle\n\
                 ON lifecycle.source_index_run_id = m.source_index_run_id\n\
                AND lifecycle.card_id = m.card_id\n\
               JOIN snapshots ON snapshots.snapshot_id = m.snapshot_id\n\
               LEFT JOIN index_runs runs ON runs.index_run_id = m.source_index_run_id\n\
               WHERE snapshots.worktree_id = (\n\
                 SELECT worktree_id FROM index_runs WHERE index_run_id = ?1\n\
               )\n\
             )\n\
             SELECT c.source_index_run_id, c.snapshot_id, c.claim_id, m.module_id,\n\
             c.polarity, c.predicate_kind, c.statement, c.claim_kind, c.status, c.confidence,\n\
             r.predicate_kind, r.predicate_path, r.predicate_symbol_id, r.source_kind,\n\
             r.source_value, r.target_kind, r.target_value, r.relation_kind\n\
             FROM claims c\n\
             JOIN ranked_cards m ON m.source_index_run_id = c.source_index_run_id\n\
               AND m.card_id = c.card_id AND m.card_rank = 1 AND m.status = 'published'\n\
             JOIN claim_lifecycle claim_state\n\
               ON claim_state.source_index_run_id = c.source_index_run_id\n\
              AND claim_state.claim_id = c.claim_id AND claim_state.status = 'active'\n\
             JOIN modules current_module ON current_module.index_run_id = ?1\n\
               AND current_module.module_id = m.module_id\n\
             LEFT JOIN claim_relations r ON r.source_index_run_id = c.source_index_run_id\n\
               AND r.claim_id = c.claim_id\n\
             WHERE (?2 IS NULL OR c.claim_id = ?2)\n\
             ORDER BY c.claim_id, c.source_index_run_id LIMIT ?3",
            params![
                published.run().id().as_bytes().to_vec(),
                claim_id.map(|id| id.as_bytes().to_vec()),
                query_limit
            ],
        )
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?;
    let mut stored = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?
    {
        if stored.len().is_multiple_of(16) {
            guard.checkpoint()?;
        }
        stored.push(StoredClaimRow::read(&row)?);
    }
    let truncated = stored.len() > usize::from(limit);
    if truncated {
        stored.pop();
    }
    if stored
        .windows(2)
        .any(|pair| pair[0].claim_id == pair[1].claim_id)
    {
        return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
    }
    Ok((stored, truncated))
}

async fn read_claim_evidence(
    transaction: &Transaction,
    selected: &[StoredClaimRow],
    guard: &ClaimReadGuard<'_>,
) -> Result<BTreeMap<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>, TaskLensClaimRepositoryError> {
    let maximum_rows = selected
        .len()
        .checked_mul(MAX_EVIDENCE_PER_CLAIM)
        .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
    let query_limit = i64::try_from(
        maximum_rows
            .checked_add(1)
            .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?,
    )
    .map_err(|_| TaskLensClaimRepositoryError::ResourceLimit)?;
    if selected.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut filters = Vec::with_capacity(selected.len());
    let mut parameters = Vec::with_capacity(selected.len().saturating_mul(2).saturating_add(1));
    for (index, claim) in selected.iter().enumerate() {
        let first = index
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
        let second = first
            .checked_add(1)
            .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
        filters.push(format!(
            "(e.source_index_run_id = ?{first} AND e.claim_id = ?{second})"
        ));
        parameters.push(Value::Blob(claim.run_id.as_bytes().to_vec()));
        parameters.push(Value::Blob(claim.claim_id.as_bytes().to_vec()));
    }
    let limit_parameter = parameters
        .len()
        .checked_add(1)
        .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
    parameters.push(Value::Integer(query_limit));
    let sql = format!(
        "SELECT e.claim_id, e.evidence_id FROM claim_evidence e\n\
         WHERE ({}) ORDER BY e.claim_id, e.evidence_id LIMIT ?{limit_parameter}",
        filters.join(" OR ")
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(parameters))
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?;
    let mut by_claim = BTreeMap::<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>::new();
    let mut row_count = 0_usize;
    while let Some(row) = rows
        .next()
        .await
        .map_err(TaskLensClaimRepositoryError::Read)?
    {
        if row_count.is_multiple_of(32) {
            guard.checkpoint()?;
        }
        row_count = row_count
            .checked_add(1)
            .ok_or(TaskLensClaimRepositoryError::ResourceLimit)?;
        if row_count > maximum_rows {
            return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
        }
        let claim_id = ModuleCardClaimId::from_bytes(read_id(&row, 0)?);
        let evidence_id = ModuleCardEvidenceId::from_bytes(read_id(&row, 1)?);
        let claims = by_claim.entry(claim_id).or_default();
        if claims.len() >= MAX_EVIDENCE_PER_CLAIM {
            return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
        }
        claims.push(evidence_id);
    }
    Ok(by_claim)
}

fn evidence_projection(
    published: &PublishedIndex,
    rows: &[StoredClaimRow],
    evidence_by_claim: &BTreeMap<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>,
) -> Result<BTreeMap<ModuleCardEvidenceId, ResolvedModuleCardEvidence>, TaskLensClaimRepositoryError>
{
    let graph = published.publication().graph();
    let mut result = BTreeMap::new();
    let mut requested = evidence_by_claim
        .values()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();

    for row in rows {
        match &row.predicate {
            ModuleClaimPredicate::Path(path) => {
                if let Ok(position) = graph
                    .files()
                    .binary_search_by(|revision| revision.path().cmp(path))
                {
                    let revision = &graph.files()[position];
                    let id = ModuleCardEvidenceId::for_file_revision_v1(revision);
                    insert_requested_evidence(
                        &mut requested,
                        &mut result,
                        id,
                        ResolvedModuleCardEvidence::File {
                            id,
                            revision: revision.clone(),
                        },
                    )?;
                }
            }
            ModuleClaimPredicate::Symbol(symbol_id) => {
                if let Ok(position) = graph
                    .symbols()
                    .binary_search_by_key(symbol_id, |symbol| symbol.id())
                {
                    let symbol = &graph.symbols()[position];
                    let id = ModuleCardEvidenceId::for_symbol_v1(symbol);
                    insert_requested_evidence(
                        &mut requested,
                        &mut result,
                        id,
                        ResolvedModuleCardEvidence::Symbol {
                            id,
                            symbol: symbol.clone(),
                        },
                    )?;
                }
            }
            ModuleClaimPredicate::Relation {
                source,
                target,
                kind,
            } => {
                for edge in graph.edges().iter().filter(|edge| {
                    edge.source() == source && edge.target() == target && edge.kind() == *kind
                }) {
                    let id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
                    insert_requested_evidence(
                        &mut requested,
                        &mut result,
                        id,
                        ResolvedModuleCardEvidence::GraphEdge {
                            id,
                            edge: edge.clone(),
                        },
                    )?;
                }
            }
            ModuleClaimPredicate::Observed(_) | ModuleClaimPredicate::ArchitecturalIntent(_) => {}
        }
    }
    if requested.is_empty() {
        return Ok(result);
    }

    for revision in graph.files() {
        let id = ModuleCardEvidenceId::for_file_revision_v1(revision);
        insert_requested_evidence(
            &mut requested,
            &mut result,
            id,
            ResolvedModuleCardEvidence::File {
                id,
                revision: revision.clone(),
            },
        )?;
    }
    for symbol in graph.symbols() {
        if requested.is_empty() {
            break;
        }
        let id = ModuleCardEvidenceId::for_symbol_v1(symbol);
        insert_requested_evidence(
            &mut requested,
            &mut result,
            id,
            ResolvedModuleCardEvidence::Symbol {
                id,
                symbol: symbol.clone(),
            },
        )?;
    }
    for edge in graph.edges() {
        if requested.is_empty() {
            break;
        }
        let id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
        insert_requested_evidence(
            &mut requested,
            &mut result,
            id,
            ResolvedModuleCardEvidence::GraphEdge {
                id,
                edge: edge.clone(),
            },
        )?;
    }
    if requested.is_empty() {
        Ok(result)
    } else {
        Err(TaskLensClaimRepositoryError::InvalidStoredProjection)
    }
}

fn insert_requested_evidence(
    requested: &mut BTreeSet<ModuleCardEvidenceId>,
    evidence: &mut BTreeMap<ModuleCardEvidenceId, ResolvedModuleCardEvidence>,
    id: ModuleCardEvidenceId,
    value: ResolvedModuleCardEvidence,
) -> Result<(), TaskLensClaimRepositoryError> {
    if !requested.remove(&id) {
        return Ok(());
    }
    if evidence.insert(id, value).is_some() {
        Err(TaskLensClaimRepositoryError::InvalidStoredProjection)
    } else {
        Ok(())
    }
}

struct StoredClaimRow {
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    claim_id: ModuleCardClaimId,
    module_id: ModuleId,
    polarity: ModuleClaimPolarity,
    predicate: ModuleClaimPredicate,
    kind: VerifiedClaimKind,
    status: VerifiedClaimStatus,
    confidence: Confidence,
}

impl StoredClaimRow {
    fn read(row: &libsql::Row) -> Result<Self, TaskLensClaimRepositoryError> {
        let polarity = match read_text(row, 4)?.as_str() {
            "affirms" => ModuleClaimPolarity::Affirms,
            "denies" => ModuleClaimPolarity::Denies,
            _ => return Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
        };
        let predicate = read_predicate(row)?;
        let kind = match read_text(row, 7)?.as_str() {
            "fact" => VerifiedClaimKind::Fact,
            "observation" => VerifiedClaimKind::Observation,
            "hypothesis" => VerifiedClaimKind::Hypothesis,
            _ => return Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
        };
        let status = match read_text(row, 8)?.as_str() {
            "active" => VerifiedClaimStatus::Active,
            _ => return Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
        };
        let confidence = u16::try_from(read_i64(row, 9)?)
            .ok()
            .and_then(|value| Confidence::from_basis_points(value).ok())
            .ok_or(TaskLensClaimRepositoryError::InvalidStoredProjection)?;
        Ok(Self {
            run_id: IndexRunId::from_bytes(read_id(row, 0)?),
            snapshot_id: SnapshotId::from_bytes(read_id(row, 1)?),
            claim_id: ModuleCardClaimId::from_bytes(read_id(row, 2)?),
            module_id: ModuleId::from_bytes(read_id(row, 3)?),
            polarity,
            predicate,
            kind,
            status,
            confidence,
        })
    }

    fn resolve(
        self,
        published: &PublishedIndex,
        evidence_by_claim: &BTreeMap<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>,
        evidence: &BTreeMap<ModuleCardEvidenceId, ResolvedModuleCardEvidence>,
    ) -> Result<TaskLensClaim, TaskLensClaimRepositoryError> {
        if !published
            .publication()
            .modules()
            .modules()
            .iter()
            .any(|module| module.id() == self.module_id)
        {
            return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
        }
        let mut resolved = Vec::new();
        if let Some(ids) = evidence_by_claim.get(&self.claim_id) {
            for id in ids {
                resolved.push(
                    evidence
                        .get(id)
                        .cloned()
                        .ok_or(TaskLensClaimRepositoryError::InvalidStoredProjection)?,
                );
            }
        }
        TaskLensClaim::new(
            self.run_id,
            self.snapshot_id,
            self.claim_id,
            self.module_id,
            self.polarity,
            self.predicate,
            self.kind,
            self.status,
            self.confidence,
            resolved,
        )
        .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)
    }
}

fn read_predicate(row: &libsql::Row) -> Result<ModuleClaimPredicate, TaskLensClaimRepositoryError> {
    let kind = read_text(row, 5)?;
    let statement = read_optional_text(row, 6)?;
    let structured_kind = read_optional_text(row, 10)?;
    let path = read_optional_blob(row, 11)?;
    let symbol = read_optional_blob(row, 12)?;
    let source_kind = read_optional_text(row, 13)?;
    let source_value = read_optional_blob(row, 14)?;
    let target_kind = read_optional_text(row, 15)?;
    let target_value = read_optional_blob(row, 16)?;
    let relation = read_optional_text(row, 17)?;
    match kind.as_str() {
        "path" => {
            ensure_no_prose_or_relation(
                statement.as_ref(),
                symbol.as_ref(),
                source_kind.as_ref(),
                source_value.as_ref(),
                target_kind.as_ref(),
                target_value.as_ref(),
                relation.as_ref(),
            )?;
            if structured_kind.as_deref() != Some("path") {
                return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
            }
            Ok(ModuleClaimPredicate::Path(
                RepositoryPath::try_from_bytes(
                    path.ok_or(TaskLensClaimRepositoryError::InvalidStoredProjection)?,
                )
                .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)?,
            ))
        }
        "symbol" => {
            ensure_no_prose_or_relation(
                statement.as_ref(),
                path.as_ref(),
                source_kind.as_ref(),
                source_value.as_ref(),
                target_kind.as_ref(),
                target_value.as_ref(),
                relation.as_ref(),
            )?;
            if structured_kind.as_deref() != Some("symbol") {
                return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
            }
            Ok(ModuleClaimPredicate::Symbol(SymbolId::from_bytes(
                exact_id(symbol)?,
            )))
        }
        "relation" => {
            if statement.is_some()
                || path.is_some()
                || symbol.is_some()
                || structured_kind.as_deref() != Some("relation")
            {
                return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
            }
            Ok(ModuleClaimPredicate::Relation {
                source: decode_endpoint(source_kind, source_value)?,
                target: decode_endpoint(target_kind, target_value)?,
                kind: decode_relation(relation)?,
            })
        }
        "observed" | "architectural-intent" => {
            if structured_kind.is_some()
                || path.is_some()
                || symbol.is_some()
                || source_kind.is_some()
                || source_value.is_some()
                || target_kind.is_some()
                || target_value.is_some()
                || relation.is_some()
            {
                return Err(TaskLensClaimRepositoryError::InvalidStoredProjection);
            }
            let statement = ModuleClaimStatement::try_from_string(
                statement.ok_or(TaskLensClaimRepositoryError::InvalidStoredProjection)?,
            )
            .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)?;
            if kind == "observed" {
                Ok(ModuleClaimPredicate::Observed(statement))
            } else {
                Ok(ModuleClaimPredicate::ArchitecturalIntent(statement))
            }
        }
        _ => Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
    }
}

#[allow(clippy::too_many_arguments)]
fn ensure_no_prose_or_relation(
    statement: Option<&String>,
    other: Option<&Vec<u8>>,
    source_kind: Option<&String>,
    source_value: Option<&Vec<u8>>,
    target_kind: Option<&String>,
    target_value: Option<&Vec<u8>>,
    relation: Option<&String>,
) -> Result<(), TaskLensClaimRepositoryError> {
    if statement.is_some()
        || other.is_some()
        || source_kind.is_some()
        || source_value.is_some()
        || target_kind.is_some()
        || target_value.is_some()
        || relation.is_some()
    {
        Err(TaskLensClaimRepositoryError::InvalidStoredProjection)
    } else {
        Ok(())
    }
}

fn decode_endpoint(
    kind: Option<String>,
    value: Option<Vec<u8>>,
) -> Result<GraphEndpoint, TaskLensClaimRepositoryError> {
    match (kind.as_deref(), value) {
        (Some("file"), Some(path)) => Ok(GraphEndpoint::File(
            RepositoryPath::try_from_bytes(path)
                .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)?,
        )),
        (Some("symbol"), Some(id)) => Ok(GraphEndpoint::Symbol(SymbolId::from_bytes(exact_id(
            Some(id),
        )?))),
        _ => Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
    }
}

fn decode_relation(
    value: Option<String>,
) -> Result<SyntaxRelationKind, TaskLensClaimRepositoryError> {
    match value.as_deref() {
        Some("imports") => Ok(SyntaxRelationKind::Imports),
        Some("exports") => Ok(SyntaxRelationKind::Exports),
        Some("calls") => Ok(SyntaxRelationKind::Calls),
        Some("tests") => Ok(SyntaxRelationKind::Tests),
        _ => Err(TaskLensClaimRepositoryError::InvalidStoredProjection),
    }
}

fn exact_id(value: Option<Vec<u8>>) -> Result<[u8; 32], TaskLensClaimRepositoryError> {
    value
        .ok_or(TaskLensClaimRepositoryError::InvalidStoredProjection)?
        .try_into()
        .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], TaskLensClaimRepositoryError> {
    let value: Vec<u8> = row.get(index).map_err(TaskLensClaimRepositoryError::Read)?;
    value
        .try_into()
        .map_err(|_| TaskLensClaimRepositoryError::InvalidStoredProjection)
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, TaskLensClaimRepositoryError> {
    row.get(index).map_err(TaskLensClaimRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, TaskLensClaimRepositoryError> {
    row.get(index).map_err(TaskLensClaimRepositoryError::Read)
}

fn read_optional_blob(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<Vec<u8>>, TaskLensClaimRepositoryError> {
    row.get(index).map_err(TaskLensClaimRepositoryError::Read)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, TaskLensClaimRepositoryError> {
    row.get(index).map_err(TaskLensClaimRepositoryError::Read)
}

struct ClaimReadGuard<'a> {
    control: &'a dyn TaskLensControl,
    started: Instant,
}

impl<'a> ClaimReadGuard<'a> {
    fn new(control: &'a dyn TaskLensControl) -> Result<Self, TaskLensClaimRepositoryError> {
        let guard = Self {
            control,
            started: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), TaskLensClaimRepositoryError> {
        if self.control.is_cancelled() {
            Err(TaskLensClaimRepositoryError::Cancelled)
        } else if self.started.elapsed() >= MAX_CLAIM_READ_DURATION {
            Err(TaskLensClaimRepositoryError::TimedOut)
        } else {
            Ok(())
        }
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: TaskLensClaimRepositoryError,
) -> Result<T, TaskLensClaimRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(TaskLensClaimRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum TaskLensClaimRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    ResourceLimit,
    Cancelled,
    TimedOut,
}

impl TaskLensClaimRepositoryError {
    pub(crate) fn classify(&self) -> TaskLensClaimStoreFailure {
        match self {
            Self::InvalidStoredProjection | Self::ResourceLimit => {
                TaskLensClaimStoreFailure::InvalidStoredProjection
            }
            Self::Cancelled => TaskLensClaimStoreFailure::Cancelled,
            Self::TimedOut => TaskLensClaimStoreFailure::TimedOut,
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => {
                TaskLensClaimStoreFailure::Storage(if is_corruption(source) {
                    KnowledgeStoreFailure::Corrupt
                } else {
                    KnowledgeStoreFailure::Unavailable
                })
            }
        }
    }
}

impl fmt::Display for TaskLensClaimRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin Task Lens claim transaction",
            Self::Read(_) => "could not read Task Lens claim projection",
            Self::Commit(_) => "could not close Task Lens claim transaction",
            Self::Rollback(_) => "could not roll back Task Lens claim transaction",
            Self::InvalidStoredProjection => "stored Task Lens claim projection is invalid",
            Self::ResourceLimit => "Task Lens claim read exceeded a resource boundary",
            Self::Cancelled => "Task Lens claim read was cancelled",
            Self::TimedOut => "Task Lens claim read timed out",
        })
    }
}

impl Error for TaskLensClaimRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::ResourceLimit
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}
