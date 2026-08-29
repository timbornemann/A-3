use a3_application::{
    DeepMapPublicationAnchor, DeepMapPublicationState, DeepMapPublicationStateFailure,
};
use a3_domain::{IndexRunId, SnapshotId, WorktreeId};
use libsql::{Connection, TransactionBehavior};

pub(crate) async fn load_publication_state(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<DeepMapPublicationState, DeepMapPublicationStateFailure> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(|_| DeepMapPublicationStateFailure::Storage)?;
    let result = async {
        let mut rows = transaction
            .query(
                "SELECT index_runs.index_run_id, index_runs.snapshot_id,\n\
                 (SELECT COUNT(*) FROM module_cards\n\
                   WHERE source_index_run_id = index_runs.index_run_id),\n\
                 (SELECT COUNT(*) FROM card_fts\n\
                   WHERE index_run_id = index_runs.index_run_id),\n\
                 (SELECT COUNT(*) FROM lexical_search_projections\n\
                   WHERE index_run_id = index_runs.index_run_id),\n\
                 (SELECT card_count FROM lexical_search_projections\n\
                   WHERE index_run_id = index_runs.index_run_id)\n\
                 FROM index_runs\n\
                 WHERE worktree_id = ?1 AND status = 'published'\n\
                 ORDER BY run_sequence DESC LIMIT 1",
                [worktree_id.as_bytes().to_vec()],
            )
            .await
            .map_err(|_| DeepMapPublicationStateFailure::Storage)?;
        let Some(row) = rows
            .next()
            .await
            .map_err(|_| DeepMapPublicationStateFailure::Storage)?
        else {
            return Ok(DeepMapPublicationState::NoPublishedIndex);
        };
        let run = parse_id::<IndexRunId>(
            row.get::<Vec<u8>>(0)
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?,
            IndexRunId::from_bytes,
        )?;
        let snapshot = parse_id::<SnapshotId>(
            row.get::<Vec<u8>>(1)
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?,
            SnapshotId::from_bytes,
        )?;
        let card_count = non_negative(
            row.get::<i64>(2)
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?,
        )?;
        let fts_count = non_negative(
            row.get::<i64>(3)
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?,
        )?;
        let projection_count = non_negative(
            row.get::<i64>(4)
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?,
        )?;
        let projected_cards = row
            .get::<Option<i64>>(5)
            .map_err(|_| DeepMapPublicationStateFailure::Storage)?
            .map(non_negative)
            .transpose()?;
        if rows
            .next()
            .await
            .map_err(|_| DeepMapPublicationStateFailure::Storage)?
            .is_some()
        {
            return Err(DeepMapPublicationStateFailure::InvalidStoredData);
        }
        let anchor = DeepMapPublicationAnchor::new(run, snapshot);
        match (card_count, fts_count, projection_count, projected_cards) {
            (0, 0, 1, Some(0)) => Ok(DeepMapPublicationState::Ready(anchor)),
            (cards, fts, 1, Some(projected)) if cards > 0 && cards == fts && cards == projected => {
                Ok(DeepMapPublicationState::Current {
                    anchor,
                    card_count: cards,
                })
            }
            _ => Err(DeepMapPublicationStateFailure::InvalidStoredData),
        }
    }
    .await;
    match result {
        Ok(state) => {
            transaction
                .commit()
                .await
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?;
            Ok(state)
        }
        Err(failure) => {
            transaction
                .rollback()
                .await
                .map_err(|_| DeepMapPublicationStateFailure::Storage)?;
            Err(failure)
        }
    }
}

fn non_negative(value: i64) -> Result<u64, DeepMapPublicationStateFailure> {
    u64::try_from(value).map_err(|_| DeepMapPublicationStateFailure::InvalidStoredData)
}

fn parse_id<T>(
    bytes: Vec<u8>,
    build: impl FnOnce([u8; 32]) -> T,
) -> Result<T, DeepMapPublicationStateFailure> {
    let bytes = bytes
        .try_into()
        .map_err(|_| DeepMapPublicationStateFailure::InvalidStoredData)?;
    Ok(build(bytes))
}
