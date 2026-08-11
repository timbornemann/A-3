use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, ModuleCardClaimPresentation, ModuleCardClaimState, ModuleCardDetail,
    ModuleCardDetailControl, ModuleCardDetailFailure, ModuleCardDetailField,
    ModuleCardDetailLoadResult, ModuleCardDetailQuery, ModuleCardLifecycle,
    ModuleCardValuePresentation,
};
use a3_domain::{
    Confidence, IndexRunId, InvalidationReason, MapperProfileVersion, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardSchemaVersion, ModuleId,
    SnapshotId, VerifiedClaimKind, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_MODULE_CARD_DETAIL_READ_DURATION: Duration = Duration::from_secs(2);
const MAX_FIELD_EVIDENCE_ROWS: usize = 6_144;
// Sum of the item bounds for all twelve accepted V1 Module Card fields.
const MAX_VALUE_ROWS: usize = 585;
const MAX_CLAIM_EVIDENCE_ROWS: usize = MAX_VALUE_ROWS * 16;

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleCardDetailQuery,
    control: &dyn ModuleCardDetailControl,
) -> Result<ModuleCardDetailLoadResult, ModuleCardDetailRepositoryError> {
    let guard = ModuleCardDetailReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleCardDetailRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ModuleCardDetailLoadResult::NoPublishedIndex);
        };
        let Some(expected_module_count) = publication.expected_module_count else {
            return Ok(ModuleCardDetailLoadResult::ProjectionUnavailable);
        };
        match validate_selected_module(
            &transaction,
            publication.index_run_id,
            expected_module_count,
            query.module_id(),
        )
        .await?
        {
            SelectedModule::Unavailable => {
                return Ok(ModuleCardDetailLoadResult::ModuleUnavailable);
            }
            SelectedModule::Primary => {}
        }
        let Some(card) = latest_card(
            &transaction,
            worktree_id,
            publication.index_run_id,
            query.module_id(),
        )
        .await?
        else {
            return Ok(ModuleCardDetailLoadResult::CardUnavailable);
        };
        validate_invalidation_run(
            &transaction,
            worktree_id,
            publication.index_run_id,
            card.lifecycle,
        )
        .await?;
        let field_evidence =
            load_field_evidence(&transaction, card.source_index_run_id, card.id, &guard).await?;
        let stored_values = load_values(
            &transaction,
            card.source_index_run_id,
            card.id,
            card.lifecycle,
            &guard,
        )
        .await?;
        let claim_evidence = load_claim_evidence(
            &transaction,
            card.source_index_run_id,
            card.id,
            &stored_values,
            &guard,
        )
        .await?;
        let fields = build_fields(stored_values, field_evidence, claim_evidence)?;
        guard.checkpoint()?;
        ModuleCardDetail::new(
            publication.index_run_id,
            publication.snapshot_id,
            card.source_index_run_id,
            card.source_snapshot_id,
            card.id,
            query.module_id(),
            card.schema_version,
            card.mapper_profile_version,
            card.confidence,
            card.lifecycle,
            fields,
        )
        .map(Box::new)
        .map(ModuleCardDetailLoadResult::Detail)
        .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(detail) => {
            transaction
                .commit()
                .await
                .map_err(ModuleCardDetailRepositoryError::Commit)?;
            Ok(detail)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

pub(crate) struct Publication {
    pub(crate) index_run_id: IndexRunId,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) expected_module_count: Option<u64>,
}

pub(crate) async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<Publication>, ModuleCardDetailRepositoryError> {
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
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    else {
        return Ok(None);
    };
    Ok(Some(Publication {
        index_run_id: IndexRunId::from_bytes(read_id(&row, 0)?),
        snapshot_id: SnapshotId::from_bytes(read_id(&row, 1)?),
        expected_module_count: read_optional_u64(&row, 2)?,
    }))
}

pub(crate) enum SelectedModule {
    Primary,
    Unavailable,
}

pub(crate) async fn validate_selected_module(
    transaction: &Transaction,
    index_run_id: IndexRunId,
    expected_module_count: u64,
    module_id: ModuleId,
) -> Result<SelectedModule, ModuleCardDetailRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*), COALESCE(MAX(CASE WHEN module_id = ?2 THEN kind END), '')\n\
             FROM modules WHERE index_run_id = ?1",
            params![
                index_run_id.as_bytes().to_vec(),
                module_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
        .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
    if read_u64(&row, 0)? != expected_module_count {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    match read_text(&row, 1)?.as_str() {
        "manifest" | "path" => Ok(SelectedModule::Primary),
        "" | "graph-community" => Ok(SelectedModule::Unavailable),
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct StoredCard {
    pub(crate) source_index_run_id: IndexRunId,
    pub(crate) source_snapshot_id: SnapshotId,
    pub(crate) id: ModuleCardId,
    schema_version: ModuleCardSchemaVersion,
    mapper_profile_version: MapperProfileVersion,
    confidence: Confidence,
    pub(crate) lifecycle: ModuleCardLifecycle,
}

pub(crate) async fn latest_card(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    current_index_run_id: IndexRunId,
    module_id: ModuleId,
) -> Result<Option<StoredCard>, ModuleCardDetailRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT card.source_index_run_id, card.snapshot_id, card.card_id,\n\
               card.card_schema_version, card.mapper_profile_version, card.confidence,\n\
               card.status, lifecycle.status, lifecycle.invalidated_by_index_run_id,\n\
               lifecycle.reason\n\
             FROM module_cards card\n\
             JOIN module_card_lifecycle lifecycle\n\
               ON lifecycle.source_index_run_id = card.source_index_run_id\n\
              AND lifecycle.card_id = card.card_id\n\
             JOIN snapshots snapshot ON snapshot.snapshot_id = card.snapshot_id\n\
             JOIN index_runs source_run\n\
               ON source_run.index_run_id = card.source_index_run_id\n\
              AND source_run.snapshot_id = card.snapshot_id\n\
              AND source_run.worktree_id = ?1 AND source_run.status = 'published'\n\
             WHERE snapshot.worktree_id = ?1 AND card.module_id = ?2\n\
             ORDER BY snapshot.generation DESC,\n\
               CASE WHEN card.source_index_run_id = ?3 THEN 1 ELSE 0 END DESC,\n\
               source_run.run_sequence DESC, card.card_id DESC LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                module_id.as_bytes().to_vec(),
                current_index_run_id.as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    else {
        return Ok(None);
    };
    if read_text(&row, 6)? != "published" {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    let schema_version = match read_u16(&row, 3)? {
        1 => ModuleCardSchemaVersion::V1,
        _ => return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    };
    let mapper_profile_version = match read_u16(&row, 4)? {
        1 => MapperProfileVersion::V1,
        _ => return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    };
    let lifecycle = read_card_lifecycle(&row, 7, 8, 9)?;
    Ok(Some(StoredCard {
        source_index_run_id: IndexRunId::from_bytes(read_id(&row, 0)?),
        source_snapshot_id: SnapshotId::from_bytes(read_id(&row, 1)?),
        id: ModuleCardId::from_bytes(read_id(&row, 2)?),
        schema_version,
        mapper_profile_version,
        confidence: read_confidence(&row, 5)?,
        lifecycle,
    }))
}

pub(crate) async fn validate_invalidation_run(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    current_index_run_id: IndexRunId,
    lifecycle: ModuleCardLifecycle,
) -> Result<(), ModuleCardDetailRepositoryError> {
    let invalidated_by = match lifecycle {
        ModuleCardLifecycle::Current => return Ok(()),
        ModuleCardLifecycle::Stale {
            invalidated_by_index_run_id,
            ..
        }
        | ModuleCardLifecycle::NeedsReview {
            invalidated_by_index_run_id,
            ..
        } => invalidated_by_index_run_id,
    };
    let mut rows = transaction
        .query(
            "SELECT CASE WHEN invalidating.run_sequence <= current_run.run_sequence THEN 1 ELSE 0 END\n\
             FROM index_runs invalidating JOIN index_runs current_run\n\
               ON current_run.index_run_id = ?3\n\
             WHERE invalidating.index_run_id = ?1 AND invalidating.worktree_id = ?2\n\
               AND invalidating.status = 'published' AND current_run.worktree_id = ?2\n\
               AND current_run.status = 'published'",
            params![
                invalidated_by.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec(),
                current_index_run_id.as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    else {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    };
    if read_u64(&row, 0)? != 1 {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    Ok(())
}

async fn load_field_evidence(
    transaction: &Transaction,
    source_index_run_id: IndexRunId,
    card_id: ModuleCardId,
    guard: &ModuleCardDetailReadGuard<'_>,
) -> Result<BTreeMap<ModuleCardField, Vec<ModuleCardEvidenceId>>, ModuleCardDetailRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT field.field_kind, evidence.evidence_id\n\
             FROM module_card_fields field\n\
             LEFT JOIN module_card_field_evidence evidence\n\
               ON evidence.source_index_run_id = field.source_index_run_id\n\
              AND evidence.card_id = field.card_id AND evidence.field_kind = field.field_kind\n\
             WHERE field.source_index_run_id = ?1 AND field.card_id = ?2\n\
             ORDER BY CASE field.field_kind\n\
               WHEN 'title' THEN 0 WHEN 'paths' THEN 1 WHEN 'purpose' THEN 2\n\
               WHEN 'responsibilities' THEN 3 WHEN 'public-surface' THEN 4\n\
               WHEN 'entrypoints' THEN 5 WHEN 'dependencies' THEN 6\n\
               WHEN 'data-flows' THEN 7 WHEN 'invariants' THEN 8 WHEN 'tests' THEN 9\n\
               WHEN 'risks' THEN 10 WHEN 'open-questions' THEN 11 ELSE 12 END,\n\
               evidence.evidence_id",
            params![
                source_index_run_id.as_bytes().to_vec(),
                card_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let mut result = BTreeMap::<ModuleCardField, Vec<ModuleCardEvidenceId>>::new();
    let mut row_count = 0_usize;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    {
        guard.checkpoint()?;
        row_count = row_count
            .checked_add(1)
            .ok_or(ModuleCardDetailRepositoryError::ResourceLimit)?;
        if row_count > MAX_FIELD_EVIDENCE_ROWS {
            return Err(ModuleCardDetailRepositoryError::ResourceLimit);
        }
        let field = read_field(&row, 0)?;
        let evidence_id = read_optional_id(&row, 1)?
            .map(ModuleCardEvidenceId::from_bytes)
            .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
        let evidence = result.entry(field).or_default();
        if evidence
            .last()
            .is_some_and(|previous| *previous >= evidence_id)
        {
            return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
        }
        evidence.push(evidence_id);
    }
    Ok(result)
}

struct StoredValue {
    field: ModuleCardField,
    value_index: u16,
    value: String,
    claim_id: ModuleCardClaimId,
    kind: VerifiedClaimKind,
    confidence: Confidence,
    state: ModuleCardClaimState,
}

async fn load_values(
    transaction: &Transaction,
    source_index_run_id: IndexRunId,
    card_id: ModuleCardId,
    lifecycle: ModuleCardLifecycle,
    guard: &ModuleCardDetailReadGuard<'_>,
) -> Result<Vec<StoredValue>, ModuleCardDetailRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT value.field_kind, value.value_index, value.field_value,\n\
               claim.claim_id, claim.claim_kind, claim.confidence, claim.status,\n\
               claim_state.status, claim_state.invalidated_by_index_run_id, claim_state.reason\n\
             FROM module_card_field_values value\n\
             LEFT JOIN claims claim\n\
               ON claim.source_index_run_id = value.source_index_run_id\n\
              AND claim.card_id = value.card_id AND claim.field_kind = value.field_kind\n\
              AND claim.value_index = value.value_index\n\
             LEFT JOIN claim_lifecycle claim_state\n\
               ON claim_state.source_index_run_id = claim.source_index_run_id\n\
              AND claim_state.claim_id = claim.claim_id\n\
             WHERE value.source_index_run_id = ?1 AND value.card_id = ?2\n\
             ORDER BY CASE value.field_kind\n\
               WHEN 'title' THEN 0 WHEN 'paths' THEN 1 WHEN 'purpose' THEN 2\n\
               WHEN 'responsibilities' THEN 3 WHEN 'public-surface' THEN 4\n\
               WHEN 'entrypoints' THEN 5 WHEN 'dependencies' THEN 6\n\
               WHEN 'data-flows' THEN 7 WHEN 'invariants' THEN 8 WHEN 'tests' THEN 9\n\
               WHEN 'risks' THEN 10 WHEN 'open-questions' THEN 11 ELSE 12 END,\n\
               value.value_index",
            params![
                source_index_run_id.as_bytes().to_vec(),
                card_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let effective_state = lifecycle_state(lifecycle);
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    {
        guard.checkpoint()?;
        if result.len() >= MAX_VALUE_ROWS {
            return Err(ModuleCardDetailRepositoryError::ResourceLimit);
        }
        let claim_id = read_optional_id(&row, 3)?
            .map(ModuleCardClaimId::from_bytes)
            .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
        if read_optional_text(&row, 6)?.as_deref() != Some("active") {
            return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
        }
        validate_claim_lifecycle(&row, lifecycle)?;
        result.push(StoredValue {
            field: read_field(&row, 0)?,
            value_index: read_u16(&row, 1)?,
            value: read_text(&row, 2)?,
            claim_id,
            kind: read_claim_kind_optional(&row, 4)?,
            confidence: read_confidence_optional(&row, 5)?,
            state: effective_state,
        });
    }
    Ok(result)
}

fn validate_claim_lifecycle(
    row: &libsql::Row,
    card_lifecycle: ModuleCardLifecycle,
) -> Result<(), ModuleCardDetailRepositoryError> {
    let status = read_optional_text(row, 7)?
        .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
    let invalidated_by = read_optional_id(row, 8)?.map(IndexRunId::from_bytes);
    let reason = read_optional_text(row, 9)?
        .map(|value| parse_reason(&value))
        .transpose()?;
    match (status.as_str(), invalidated_by, reason, card_lifecycle) {
        (
            "active",
            None,
            None,
            ModuleCardLifecycle::Current | ModuleCardLifecycle::Stale { .. },
        )
        | ("active", None, None, ModuleCardLifecycle::NeedsReview { .. }) => Ok(()),
        (
            "stale",
            Some(claim_run),
            Some(claim_reason),
            ModuleCardLifecycle::Stale {
                invalidated_by_index_run_id,
                reason,
            },
        ) if claim_run == invalidated_by_index_run_id && claim_reason == reason => Ok(()),
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

async fn load_claim_evidence(
    transaction: &Transaction,
    source_index_run_id: IndexRunId,
    card_id: ModuleCardId,
    stored_values: &[StoredValue],
    guard: &ModuleCardDetailReadGuard<'_>,
) -> Result<BTreeMap<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>, ModuleCardDetailRepositoryError>
{
    let mut rows = transaction
        .query(
            "SELECT claim.claim_id, evidence.evidence_id\n\
             FROM claims claim LEFT JOIN claim_evidence evidence\n\
               ON evidence.source_index_run_id = claim.source_index_run_id\n\
              AND evidence.claim_id = claim.claim_id\n\
             WHERE claim.source_index_run_id = ?1 AND claim.card_id = ?2\n\
             ORDER BY claim.claim_id, evidence.evidence_id",
            params![
                source_index_run_id.as_bytes().to_vec(),
                card_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let expected_claims = stored_values
        .iter()
        .map(|value| value.claim_id)
        .collect::<BTreeSet<_>>();
    if expected_claims.len() != stored_values.len() {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    let mut result = BTreeMap::<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>::new();
    let mut row_count = 0_usize;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardDetailRepositoryError::Read)?
    {
        guard.checkpoint()?;
        row_count = row_count
            .checked_add(1)
            .ok_or(ModuleCardDetailRepositoryError::ResourceLimit)?;
        if row_count > MAX_CLAIM_EVIDENCE_ROWS {
            return Err(ModuleCardDetailRepositoryError::ResourceLimit);
        }
        let claim_id = ModuleCardClaimId::from_bytes(read_id(&row, 0)?);
        if !expected_claims.contains(&claim_id) {
            return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
        }
        let evidence = result.entry(claim_id).or_default();
        if let Some(evidence_id) = read_optional_id(&row, 1)?.map(ModuleCardEvidenceId::from_bytes)
        {
            if evidence.len() >= 16
                || evidence
                    .last()
                    .is_some_and(|previous| *previous >= evidence_id)
            {
                return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
            }
            evidence.push(evidence_id);
        }
    }
    if result.len() != expected_claims.len() {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    Ok(result)
}

fn build_fields(
    stored_values: Vec<StoredValue>,
    mut field_evidence: BTreeMap<ModuleCardField, Vec<ModuleCardEvidenceId>>,
    mut claim_evidence: BTreeMap<ModuleCardClaimId, Vec<ModuleCardEvidenceId>>,
) -> Result<Vec<ModuleCardDetailField>, ModuleCardDetailRepositoryError> {
    let mut grouped = BTreeMap::<ModuleCardField, Vec<StoredValue>>::new();
    for value in stored_values {
        grouped.entry(value.field).or_default().push(value);
    }
    if grouped.len() != field_evidence.len()
        || grouped
            .keys()
            .any(|field| !field_evidence.contains_key(field))
    {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    let mut fields = Vec::with_capacity(grouped.len());
    for (field, values) in grouped {
        if values
            .iter()
            .enumerate()
            .any(|(index, value)| usize::from(value.value_index) != index)
        {
            return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
        }
        let mut presented = Vec::with_capacity(values.len());
        for value in values {
            let evidence_ids = claim_evidence
                .remove(&value.claim_id)
                .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
            let claim = ModuleCardClaimPresentation::new(
                value.claim_id,
                value.kind,
                value.confidence,
                value.state,
                evidence_ids,
            )
            .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
            presented.push(ModuleCardValuePresentation::new(value.value, claim));
        }
        fields.push(
            ModuleCardDetailField::new(
                field,
                presented,
                field_evidence
                    .remove(&field)
                    .ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?,
            )
            .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)?,
        );
    }
    if !claim_evidence.is_empty() || !field_evidence.is_empty() {
        return Err(ModuleCardDetailRepositoryError::InvalidStoredProjection);
    }
    Ok(fields)
}

const fn lifecycle_state(lifecycle: ModuleCardLifecycle) -> ModuleCardClaimState {
    match lifecycle {
        ModuleCardLifecycle::Current => ModuleCardClaimState::Current,
        ModuleCardLifecycle::Stale { .. } => ModuleCardClaimState::Stale,
        ModuleCardLifecycle::NeedsReview { .. } => ModuleCardClaimState::NeedsReview,
    }
}

fn read_card_lifecycle(
    row: &libsql::Row,
    status_index: i32,
    run_index: i32,
    reason_index: i32,
) -> Result<ModuleCardLifecycle, ModuleCardDetailRepositoryError> {
    let status = read_text(row, status_index)?;
    let invalidated_by = read_optional_id(row, run_index)?.map(IndexRunId::from_bytes);
    let reason = read_optional_text(row, reason_index)?
        .map(|value| parse_reason(&value))
        .transpose()?;
    match (status.as_str(), invalidated_by, reason) {
        ("published", None, None) => Ok(ModuleCardLifecycle::Current),
        ("stale", Some(invalidated_by_index_run_id), Some(reason)) => {
            Ok(ModuleCardLifecycle::Stale {
                invalidated_by_index_run_id,
                reason,
            })
        }
        ("needs-review", Some(invalidated_by_index_run_id), Some(reason)) => {
            Ok(ModuleCardLifecycle::NeedsReview {
                invalidated_by_index_run_id,
                reason,
            })
        }
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

fn parse_reason(value: &str) -> Result<InvalidationReason, ModuleCardDetailRepositoryError> {
    match value {
        "evidence-changed" => Ok(InvalidationReason::EvidenceChanged),
        "module-removed" => Ok(InvalidationReason::ModuleRemoved),
        "parser-version-changed" => Ok(InvalidationReason::ParserVersionChanged),
        "mapper-version-changed" => Ok(InvalidationReason::MapperVersionChanged),
        "direct-dependency-changed" => Ok(InvalidationReason::DirectDependencyChanged),
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

fn read_field(
    row: &libsql::Row,
    index: i32,
) -> Result<ModuleCardField, ModuleCardDetailRepositoryError> {
    match read_text(row, index)?.as_str() {
        "title" => Ok(ModuleCardField::Title),
        "paths" => Ok(ModuleCardField::Paths),
        "purpose" => Ok(ModuleCardField::Purpose),
        "responsibilities" => Ok(ModuleCardField::Responsibilities),
        "public-surface" => Ok(ModuleCardField::PublicSurface),
        "entrypoints" => Ok(ModuleCardField::Entrypoints),
        "dependencies" => Ok(ModuleCardField::Dependencies),
        "data-flows" => Ok(ModuleCardField::DataFlows),
        "invariants" => Ok(ModuleCardField::Invariants),
        "tests" => Ok(ModuleCardField::Tests),
        "risks" => Ok(ModuleCardField::Risks),
        "open-questions" => Ok(ModuleCardField::OpenQuestions),
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

fn read_claim_kind_optional(
    row: &libsql::Row,
    index: i32,
) -> Result<VerifiedClaimKind, ModuleCardDetailRepositoryError> {
    match read_optional_text(row, index)?.as_deref() {
        Some("fact") => Ok(VerifiedClaimKind::Fact),
        Some("observation") => Ok(VerifiedClaimKind::Observation),
        Some("hypothesis") => Ok(VerifiedClaimKind::Hypothesis),
        _ => Err(ModuleCardDetailRepositoryError::InvalidStoredProjection),
    }
}

fn read_confidence(
    row: &libsql::Row,
    index: i32,
) -> Result<Confidence, ModuleCardDetailRepositoryError> {
    Confidence::from_basis_points(read_u16(row, index)?)
        .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
}

fn read_confidence_optional(
    row: &libsql::Row,
    index: i32,
) -> Result<Confidence, ModuleCardDetailRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    let value = value.ok_or(ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
    let value = u16::try_from(value)
        .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)?;
    Confidence::from_basis_points(value)
        .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
}

struct ModuleCardDetailReadGuard<'a> {
    control: &'a dyn ModuleCardDetailControl,
    started_at: Instant,
}

impl<'a> ModuleCardDetailReadGuard<'a> {
    fn new(
        control: &'a dyn ModuleCardDetailControl,
    ) -> Result<Self, ModuleCardDetailRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleCardDetailRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleCardDetailRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_MODULE_CARD_DETAIL_READ_DURATION {
            return Err(ModuleCardDetailRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleCardDetailRepositoryError,
) -> Result<T, ModuleCardDetailRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleCardDetailRepositoryError::Rollback(source)),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleCardDetailRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, ModuleCardDetailRepositoryError> {
    let bytes: Option<Vec<u8>> = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    bytes
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, ModuleCardDetailRepositoryError> {
    row.get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, ModuleCardDetailRepositoryError> {
    row.get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)
}

fn read_u16(row: &libsql::Row, index: i32) -> Result<u16, ModuleCardDetailRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    u16::try_from(value).map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
}

fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, ModuleCardDetailRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
}

fn read_optional_u64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, ModuleCardDetailRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(ModuleCardDetailRepositoryError::Read)?;
    value
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| ModuleCardDetailRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

#[derive(Debug)]
pub(crate) enum ModuleCardDetailRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    ResourceLimit,
    Cancelled,
    TimedOut,
}

impl ModuleCardDetailRepositoryError {
    pub(crate) fn classify(&self) -> ModuleCardDetailFailure {
        match self {
            Self::InvalidStoredProjection | Self::ResourceLimit => {
                ModuleCardDetailFailure::InvalidStoredProjection
            }
            Self::Cancelled => ModuleCardDetailFailure::Cancelled,
            Self::TimedOut => ModuleCardDetailFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleCardDetailFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleCardDetailFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleCardDetailRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin Module Card detail read",
            Self::Read(_) => "could not read Module Card detail",
            Self::Commit(_) => "could not commit Module Card detail read",
            Self::Rollback(_) => "could not roll back Module Card detail read",
            Self::InvalidStoredProjection => "stored Module Card detail projection is invalid",
            Self::ResourceLimit => "Module Card detail exceeded its fixed resource bound",
            Self::Cancelled => "Module Card detail read was cancelled",
            Self::TimedOut => "Module Card detail read timed out",
        })
    }
}

impl Error for ModuleCardDetailRepositoryError {
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
