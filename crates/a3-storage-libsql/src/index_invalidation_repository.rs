use crate::index_publication::{IndexPublicationRepositoryError, MutationProgress};
use a3_domain::{
    IndexInvalidationPlan, IndexRunId, InvalidationReason, MapperProfileVersion, ModuleCardId,
    ModuleCardInvalidationCandidate, ModuleCardStatus, ModuleId, PublishedIndex, RemapPriority,
    SnapshotId,
};
use libsql::{Transaction, params};

pub(crate) async fn invalidate_for_publication(
    transaction: &Transaction,
    published: &PublishedIndex,
    progress: &MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    progress.checkpoint()?;
    insert_direct_evidence_invalidations(transaction, published).await?;
    progress.checkpoint()?;
    let candidates = read_latest_published_cards(transaction, published, progress).await?;
    let plan = IndexInvalidationPlan::compile(published, MapperProfileVersion::V1, candidates)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    progress.checkpoint()?;
    refresh_existing_queue_targets(transaction, published).await?;
    progress.checkpoint()?;
    apply_plan(transaction, &plan, progress).await
}

async fn insert_direct_evidence_invalidations(
    transaction: &Transaction,
    published: &PublishedIndex,
) -> Result<(), IndexPublicationRepositoryError> {
    transaction
        .execute(
            "INSERT INTO evidence_invalidations (\n\
             target_index_run_id, source_index_run_id, evidence_id, reason\n\
             )\n\
             WITH ranked_cards AS (\n\
               SELECT mc.source_index_run_id, mc.card_id, mc.module_id, ml.status,\n\
                 ROW_NUMBER() OVER (\n\
                   PARTITION BY mc.module_id ORDER BY\n\
                     s.generation DESC,\n\
                     CASE WHEN mc.source_index_run_id = ?1 THEN 1 ELSE 0 END DESC,\n\
                     COALESCE(r.run_sequence, 0) DESC, mc.card_id DESC\n\
                 ) AS card_rank\n\
               FROM module_cards mc\n\
               JOIN module_card_lifecycle ml\n\
                 ON ml.source_index_run_id = mc.source_index_run_id\n\
                AND ml.card_id = mc.card_id\n\
               JOIN snapshots s ON s.snapshot_id = mc.snapshot_id\n\
               LEFT JOIN index_runs r ON r.index_run_id = mc.source_index_run_id\n\
               WHERE s.worktree_id = (\n\
                 SELECT worktree_id FROM index_runs WHERE index_run_id = ?1\n\
               )\n\
             ), latest_cards AS (\n\
               SELECT source_index_run_id, card_id FROM ranked_cards\n\
               WHERE card_rank = 1 AND status = 'published'\n\
             ), card_evidence AS (\n\
               SELECT latest.source_index_run_id, latest.card_id, field.evidence_id\n\
               FROM latest_cards latest\n\
               JOIN module_card_field_evidence field\n\
                 ON field.source_index_run_id = latest.source_index_run_id\n\
                AND field.card_id = latest.card_id\n\
               UNION\n\
               SELECT latest.source_index_run_id, latest.card_id, claim.evidence_id\n\
               FROM latest_cards latest\n\
               JOIN claims c ON c.source_index_run_id = latest.source_index_run_id\n\
                AND c.card_id = latest.card_id\n\
               JOIN claim_evidence claim ON claim.source_index_run_id = c.source_index_run_id\n\
                AND claim.claim_id = c.claim_id\n\
             )\n\
             SELECT ?1, evidence.source_index_run_id, evidence.evidence_id, 'evidence-changed'\n\
             FROM card_evidence evidence\n\
             JOIN evidence_refs stored\n\
               ON stored.source_index_run_id = evidence.source_index_run_id\n\
              AND stored.evidence_id = evidence.evidence_id\n\
             WHERE\n\
               (stored.evidence_kind = 'file' AND NOT EXISTS (\n\
                 SELECT 1 FROM file_revisions current\n\
                 WHERE current.index_run_id = ?1\n\
                   AND current.repository_path = stored.repository_path\n\
                   AND current.content_hash = stored.content_hash\n\
               )) OR\n\
               (stored.evidence_kind = 'symbol' AND NOT EXISTS (\n\
                 SELECT 1 FROM symbols current\n\
                 WHERE current.index_run_id = ?1 AND current.symbol_id = stored.symbol_id\n\
               )) OR\n\
               (stored.evidence_kind = 'graph-edge' AND (\n\
                 stored.snapshot_id <> ?2 OR NOT EXISTS (\n\
                   SELECT 1 FROM symbol_edges current\n\
                   WHERE current.index_run_id = ?1\n\
                     AND current.source_kind = stored.source_kind\n\
                     AND current.source_value = stored.source_value\n\
                     AND current.target_kind = stored.target_kind\n\
                     AND current.target_value = stored.target_value\n\
                     AND current.relation_kind = stored.relation_kind\n\
                     AND current.provider = stored.provider\n\
                     AND current.confidence = stored.edge_confidence\n\
                     AND current.resolution = stored.resolution\n\
                     AND current.evidence_path = stored.repository_path\n\
                     AND current.evidence_hash = stored.content_hash\n\
                     AND current.evidence_start_byte = stored.start_byte\n\
                     AND current.evidence_end_byte = stored.end_byte\n\
                     AND current.evidence_start_row = stored.start_row\n\
                     AND current.evidence_start_column = stored.start_column\n\
                     AND current.evidence_end_row = stored.end_row\n\
                     AND current.evidence_end_column = stored.end_column\n\
                 )\n\
               ))",
            params![
                published.run().id().as_bytes().to_vec(),
                published.run().snapshot_id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn read_latest_published_cards(
    transaction: &Transaction,
    published: &PublishedIndex,
    progress: &MutationProgress<'_>,
) -> Result<Vec<ModuleCardInvalidationCandidate>, IndexPublicationRepositoryError> {
    progress.checkpoint()?;
    let mut rows = transaction
        .query(
            "WITH ranked_cards AS (\n\
               SELECT mc.source_index_run_id, mc.snapshot_id, mc.card_id, mc.module_id,\n\
                 mc.mapper_profile_version, ml.status,\n\
                 ROW_NUMBER() OVER (\n\
                   PARTITION BY mc.module_id ORDER BY\n\
                     s.generation DESC,\n\
                     CASE WHEN mc.source_index_run_id = ?1 THEN 1 ELSE 0 END DESC,\n\
                     COALESCE(r.run_sequence, 0) DESC, mc.card_id DESC\n\
                 ) AS card_rank\n\
               FROM module_cards mc\n\
               JOIN module_card_lifecycle ml\n\
                 ON ml.source_index_run_id = mc.source_index_run_id\n\
                AND ml.card_id = mc.card_id\n\
               JOIN snapshots s ON s.snapshot_id = mc.snapshot_id\n\
               LEFT JOIN index_runs r ON r.index_run_id = mc.source_index_run_id\n\
               WHERE s.worktree_id = (\n\
                 SELECT worktree_id FROM index_runs WHERE index_run_id = ?1\n\
               )\n\
             )\n\
             SELECT card.source_index_run_id, card.snapshot_id, card.card_id, card.module_id,\n\
               card.mapper_profile_version,\n\
               NOT EXISTS (\n\
                 SELECT 1 FROM snapshot_adapter_revisions old\n\
                 LEFT JOIN snapshot_adapter_revisions current\n\
                   ON current.snapshot_id = ?2 AND current.language = old.language\n\
                  AND current.adapter_version = old.adapter_version\n\
                 WHERE old.snapshot_id = card.snapshot_id AND current.language IS NULL\n\
               ) AND NOT EXISTS (\n\
                 SELECT 1 FROM snapshot_adapter_revisions current\n\
                 LEFT JOIN snapshot_adapter_revisions old\n\
                   ON old.snapshot_id = card.snapshot_id AND old.language = current.language\n\
                  AND old.adapter_version = current.adapter_version\n\
                 WHERE current.snapshot_id = ?2 AND old.language IS NULL\n\
               ) AS parser_compatible,\n\
               NOT EXISTS (\n\
                 SELECT 1 FROM (\n\
                   SELECT evidence_id FROM module_card_field_evidence\n\
                   WHERE source_index_run_id = card.source_index_run_id\n\
                     AND card_id = card.card_id\n\
                   UNION\n\
                   SELECT ce.evidence_id FROM claims c\n\
                   JOIN claim_evidence ce ON ce.source_index_run_id = c.source_index_run_id\n\
                    AND ce.claim_id = c.claim_id\n\
                   WHERE c.source_index_run_id = card.source_index_run_id\n\
                     AND c.card_id = card.card_id\n\
                 ) evidence\n\
                 JOIN evidence_invalidations invalid\n\
                   ON invalid.target_index_run_id = ?1\n\
                  AND invalid.source_index_run_id = card.source_index_run_id\n\
                  AND invalid.evidence_id = evidence.evidence_id\n\
               ) AS evidence_current\n\
             FROM ranked_cards card\n\
             WHERE card.card_rank = 1 AND card.status = 'published'\n\
             ORDER BY card.module_id",
            params![
                published.run().id().as_bytes().to_vec(),
                published.run().snapshot_id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        if candidates.len().is_multiple_of(256) {
            progress.checkpoint()?;
        }
        let mapper_version = u16::try_from(read_i64(&row, 4)?)
            .ok()
            .and_then(|value| MapperProfileVersion::new(value).ok())
            .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
        candidates.push(ModuleCardInvalidationCandidate::new(
            IndexRunId::from_bytes(read_id(&row, 0)?),
            SnapshotId::from_bytes(read_id(&row, 1)?),
            ModuleCardId::from_bytes(read_id(&row, 2)?),
            ModuleId::from_bytes(read_id(&row, 3)?),
            mapper_version,
            read_bool(&row, 5)?,
            read_bool(&row, 6)?,
        ));
    }
    progress.checkpoint()?;
    Ok(candidates)
}

async fn refresh_existing_queue_targets(
    transaction: &Transaction,
    published: &PublishedIndex,
) -> Result<(), IndexPublicationRepositoryError> {
    transaction
        .execute(
            "DELETE FROM module_remap_queue WHERE module_id NOT IN (\n\
             SELECT module_id FROM modules WHERE index_run_id = ?1\n\
             )",
            [published.run().id().as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    transaction
        .execute(
            "UPDATE module_remap_queue\n\
             SET target_index_run_id = ?1, target_snapshot_id = ?2",
            params![
                published.run().id().as_bytes().to_vec(),
                published.run().snapshot_id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn apply_plan(
    transaction: &Transaction,
    plan: &IndexInvalidationPlan,
    progress: &MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    for (index, invalidation) in plan.invalidations().iter().enumerate() {
        if index.is_multiple_of(256) {
            progress.checkpoint()?;
        }
        let status = card_status(invalidation.status())?;
        let reason = invalidation_reason(invalidation.reason());
        let affected = transaction
            .execute(
                "UPDATE module_card_lifecycle\n\
                 SET status = ?1, invalidated_by_index_run_id = ?2, reason = ?3\n\
                 WHERE source_index_run_id = ?4 AND card_id = ?5 AND status = 'published'",
                params![
                    status,
                    plan.target_index_run_id().as_bytes().to_vec(),
                    reason,
                    invalidation.source_index_run_id().as_bytes().to_vec(),
                    invalidation.card_id().as_bytes().to_vec(),
                ],
            )
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
        if affected != 1 {
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
        if invalidation.status() == ModuleCardStatus::Stale {
            invalidate_claims(transaction, plan, *invalidation, reason).await?;
        }
    }
    for (index, request) in plan.remaps().iter().enumerate() {
        if index.is_multiple_of(256) {
            progress.checkpoint()?;
        }
        let affected = transaction
            .execute(
                "INSERT INTO module_remap_queue (\n\
                 module_id, source_index_run_id, card_id, target_index_run_id,\n\
                 target_snapshot_id, priority, reason\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n\
                 ON CONFLICT(module_id) DO UPDATE SET\n\
                   source_index_run_id = excluded.source_index_run_id,\n\
                   card_id = excluded.card_id,\n\
                   target_index_run_id = excluded.target_index_run_id,\n\
                   target_snapshot_id = excluded.target_snapshot_id,\n\
                   priority = excluded.priority, reason = excluded.reason",
                params![
                    request.module_id().as_bytes().to_vec(),
                    request.source_index_run_id().as_bytes().to_vec(),
                    request.card_id().as_bytes().to_vec(),
                    request.target_index_run_id().as_bytes().to_vec(),
                    request.target_snapshot_id().as_bytes().to_vec(),
                    remap_priority(request.priority()),
                    invalidation_reason(request.reason()),
                ],
            )
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
        if affected != 1 {
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
    }
    progress.checkpoint()?;
    Ok(())
}

async fn invalidate_claims(
    transaction: &Transaction,
    plan: &IndexInvalidationPlan,
    invalidation: a3_domain::ModuleCardInvalidation,
    reason: &'static str,
) -> Result<(), IndexPublicationRepositoryError> {
    let (predicate, parameters) = if invalidation.reason() == InvalidationReason::EvidenceChanged {
        (
            " AND claim_id IN (\n\
               SELECT c.claim_id FROM claims c\n\
               JOIN claim_evidence ce ON ce.source_index_run_id = c.source_index_run_id\n\
                AND ce.claim_id = c.claim_id\n\
               JOIN evidence_invalidations invalid\n\
                 ON invalid.target_index_run_id = ?5\n\
                AND invalid.source_index_run_id = ce.source_index_run_id\n\
                AND invalid.evidence_id = ce.evidence_id\n\
               WHERE c.source_index_run_id = ?3 AND c.card_id = ?4\n\
             )",
            true,
        )
    } else {
        (
            " AND claim_id IN (\n\
               SELECT claim_id FROM claims WHERE source_index_run_id = ?3 AND card_id = ?4\n\
             )",
            false,
        )
    };
    let sql = format!(
        "UPDATE claim_lifecycle\n\
         SET status = 'stale', invalidated_by_index_run_id = ?1, reason = ?2\n\
         WHERE source_index_run_id = ?3 AND status = 'active'{predicate}"
    );
    let run = plan.target_index_run_id().as_bytes().to_vec();
    let source = invalidation.source_index_run_id().as_bytes().to_vec();
    let card = invalidation.card_id().as_bytes().to_vec();
    if parameters {
        transaction
            .execute(&sql, params![run.clone(), reason, source, card, run])
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
    } else {
        transaction
            .execute(&sql, params![run, reason, source, card])
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
    }
    Ok(())
}

fn card_status(status: ModuleCardStatus) -> Result<&'static str, IndexPublicationRepositoryError> {
    match status {
        ModuleCardStatus::Stale => Ok("stale"),
        ModuleCardStatus::NeedsReview => Ok("needs-review"),
        ModuleCardStatus::Proposed | ModuleCardStatus::Verified | ModuleCardStatus::Published => {
            Err(IndexPublicationRepositoryError::InvalidStoredData)
        }
    }
}

const fn invalidation_reason(reason: InvalidationReason) -> &'static str {
    match reason {
        InvalidationReason::EvidenceChanged => "evidence-changed",
        InvalidationReason::ModuleRemoved => "module-removed",
        InvalidationReason::ParserVersionChanged => "parser-version-changed",
        InvalidationReason::MapperVersionChanged => "mapper-version-changed",
        InvalidationReason::DirectDependencyChanged => "direct-dependency-changed",
    }
}

const fn remap_priority(priority: RemapPriority) -> i64 {
    match priority {
        RemapPriority::Direct => 0,
        RemapPriority::Dependent => 1,
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], IndexPublicationRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, IndexPublicationRepositoryError> {
    row.get(index)
        .map_err(IndexPublicationRepositoryError::Read)
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, IndexPublicationRepositoryError> {
    match read_i64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}
