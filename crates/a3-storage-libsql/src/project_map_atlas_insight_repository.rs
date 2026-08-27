use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, ProjectMapAtlasControl, ProjectMapAtlasFailure,
    ProjectMapAtlasModuleInsight, ProjectMapMappingStatus,
};
use a3_domain::{IndexRunId, ModuleId, WorktreeId};
use libsql::{Connection, TransactionBehavior, Value, params_from_iter};
use std::collections::BTreeMap;

const MAX_SUMMARY_MODULES: usize = 64;

pub(crate) async fn load_summaries(
    connection: &Connection,
    worktree_id: WorktreeId,
    current_run_id: IndexRunId,
    module_ids: &[ModuleId],
    control: &dyn ProjectMapAtlasControl,
) -> Result<Vec<ProjectMapAtlasModuleInsight>, ProjectMapAtlasFailure> {
    if module_ids.is_empty() {
        return Ok(Vec::new());
    }
    if module_ids.len() > MAX_SUMMARY_MODULES
        || module_ids.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(ProjectMapAtlasFailure::InvalidStoredProjection);
    }
    checkpoint(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(classify)?;
    let result = async {
        let placeholders = (4..module_ids.len() + 4)
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "WITH ranked_cards AS (\n\
               SELECT card.module_id, card.source_index_run_id, card.card_id, lifecycle.status,\n\
                 ROW_NUMBER() OVER (PARTITION BY card.module_id ORDER BY snapshot.generation DESC,\n\
                   CASE WHEN card.source_index_run_id = ?2 THEN 1 ELSE 0 END DESC,\n\
                   source_run.run_sequence DESC, card.card_id DESC) card_rank\n\
               FROM module_cards card\n\
               JOIN module_card_lifecycle lifecycle\n\
                 ON lifecycle.source_index_run_id = card.source_index_run_id\n\
                AND lifecycle.card_id = card.card_id\n\
               JOIN snapshots snapshot ON snapshot.snapshot_id = card.snapshot_id\n\
               JOIN index_runs source_run ON source_run.index_run_id = card.source_index_run_id\n\
                 AND source_run.snapshot_id = card.snapshot_id\n\
                 AND source_run.worktree_id = ?1 AND source_run.status = 'published'\n\
               WHERE snapshot.worktree_id = ?1 AND card.status = 'published'\n\
                 AND card.module_id IN ({placeholders})\n\
             )\n\
             SELECT module.module_id, card.status,\n\
               CASE WHEN card.status IN ('published', 'needs-review') THEN\n\
                 (SELECT value.field_value FROM module_card_field_values value\n\
                   WHERE value.source_index_run_id = card.source_index_run_id\n\
                     AND value.card_id = card.card_id AND value.field_kind = 'purpose'\n\
                   ORDER BY value.value_index LIMIT 1) ELSE NULL END,\n\
               CASE WHEN card.status = 'published' THEN\n\
                 (SELECT COUNT(*) FROM module_card_field_values value\n\
                   WHERE value.source_index_run_id = card.source_index_run_id\n\
                     AND value.card_id = card.card_id AND value.field_kind = 'risks') ELSE 0 END\n\
             FROM modules module LEFT JOIN ranked_cards card\n\
               ON card.module_id = module.module_id AND card.card_rank = 1\n\
             WHERE module.index_run_id = ?3 AND module.kind IN ('manifest', 'path')\n\
               AND module.module_id IN ({placeholders}) ORDER BY module.module_id"
        );
        let mut values = vec![
            Value::Blob(worktree_id.as_bytes().to_vec()),
            Value::Blob(current_run_id.as_bytes().to_vec()),
            Value::Blob(current_run_id.as_bytes().to_vec()),
        ];
        values.extend(
            module_ids
                .iter()
                .map(|id| Value::Blob(id.as_bytes().to_vec())),
        );
        let mut rows = transaction
            .query(&sql, params_from_iter(values))
            .await
            .map_err(classify)?;
        let mut summaries = BTreeMap::new();
        while let Some(row) = rows.next().await.map_err(classify)? {
            checkpoint(control)?;
            let module_id = ModuleId::from_bytes(read_id(&row, 0)?);
            let status = match row.get::<Option<String>>(1).map_err(classify)?.as_deref() {
                None => ProjectMapMappingStatus::Unmapped,
                Some("published") => ProjectMapMappingStatus::Current,
                Some("stale") => ProjectMapMappingStatus::Stale,
                Some("needs-review") => ProjectMapMappingStatus::NeedsReview,
                Some(_) => return Err(ProjectMapAtlasFailure::InvalidStoredProjection),
            };
            let purpose = row.get::<Option<String>>(2).map_err(classify)?;
            let risks = read_count(&row, 3)?;
            let insight = ProjectMapAtlasModuleInsight::summary(
                module_id,
                status,
                purpose,
                risks,
            )
            .map_err(|_| ProjectMapAtlasFailure::InvalidStoredProjection)?;
            if summaries.insert(module_id, insight).is_some() {
                return Err(ProjectMapAtlasFailure::InvalidStoredProjection);
            }
        }
        if summaries.len() != module_ids.len() {
            return Err(ProjectMapAtlasFailure::InvalidStoredProjection);
        }
        module_ids
            .iter()
            .map(|module_id| {
                summaries
                    .remove(module_id)
                    .ok_or(ProjectMapAtlasFailure::InvalidStoredProjection)
            })
            .collect()
    }
    .await;
    match result {
        Ok(summaries) => {
            transaction.commit().await.map_err(classify)?;
            Ok(summaries)
        }
        Err(error) => {
            transaction.rollback().await.map_err(classify)?;
            Err(error)
        }
    }
}

fn checkpoint(control: &dyn ProjectMapAtlasControl) -> Result<(), ProjectMapAtlasFailure> {
    if control.is_cancelled() {
        Err(ProjectMapAtlasFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ProjectMapAtlasFailure> {
    row.get::<Vec<u8>>(index)
        .map_err(classify)?
        .try_into()
        .map_err(|_| ProjectMapAtlasFailure::InvalidStoredProjection)
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ProjectMapAtlasFailure> {
    let value = row.get::<i64>(index).map_err(classify)?;
    u64::try_from(value).map_err(|_| ProjectMapAtlasFailure::InvalidStoredProjection)
}

fn classify(error: libsql::Error) -> ProjectMapAtlasFailure {
    ProjectMapAtlasFailure::Storage(if is_corruption(&error) {
        KnowledgeStoreFailure::Corrupt
    } else {
        KnowledgeStoreFailure::Unavailable
    })
}
