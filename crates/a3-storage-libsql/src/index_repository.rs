use crate::catalog::is_corruption;
use a3_application::{KnowledgeIndexFailure, KnowledgeStoreFailure};
use a3_domain::{
    ContentHash, FileRevision, GitHead, GitObjectId, GitReferenceName, IndexLanguage, IndexRunId,
    IndexRunRecord, IndexRunSequence, IndexRunStart, IndexRunStatus, IndexRunTerminalOutcome,
    IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion, RankingPolicyVersion,
    RepositoryFileState, RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotId,
    WorktreeGeneration, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn append_snapshot(
    connection: &Connection,
    expected_worktree_id: WorktreeId,
    snapshot: &Snapshot,
) -> Result<(), IndexRepositoryError> {
    if snapshot.worktree_id() != expected_worktree_id {
        return Err(IndexRepositoryError::IdentityConflict);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(IndexRepositoryError::Begin)?;
    let result = append_snapshot_in_transaction(&transaction, expected_worktree_id, snapshot).await;
    if let Err(error) = result {
        return rollback(transaction, error).await;
    }
    transaction
        .commit()
        .await
        .map_err(IndexRepositoryError::Commit)
}

async fn append_snapshot_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    snapshot: &Snapshot,
) -> Result<(), IndexRepositoryError> {
    validate_snapshot_chain_in_transaction(transaction, worktree_id).await?;
    let latest = latest_snapshot_position(transaction, worktree_id).await?;
    match latest {
        None => {
            if snapshot.generation().get() != 1 || snapshot.parent_id().is_some() {
                return Err(IndexRepositoryError::SnapshotConflict);
            }
        }
        Some((latest_id, latest_generation)) => {
            let next = latest_generation
                .next()
                .map_err(|_| IndexRepositoryError::SequenceExhausted)?;
            if snapshot.generation() != next || snapshot.parent_id() != Some(latest_id) {
                return Err(IndexRepositoryError::SnapshotConflict);
            }
        }
    }
    if snapshot_exists(transaction, snapshot.id()).await? {
        return Err(IndexRepositoryError::SnapshotConflict);
    }

    let head = HeadFields::from(snapshot.head());
    transaction
        .execute(
            "INSERT INTO snapshots (\n\
             snapshot_id, worktree_id, parent_snapshot_id, generation,\n\
             head_kind, head_object_id, head_reference, index_schema_version\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot.id().as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec(),
                snapshot.parent_id().map(|id| id.as_bytes().to_vec()),
                generation_to_i64(snapshot.generation())?,
                head.kind,
                head.object_id,
                head.reference,
                i64::from(snapshot.index_schema_version().get())
            ],
        )
        .await
        .map_err(classify_snapshot_write)?;

    for revision in snapshot.adapter_revisions() {
        transaction
            .execute(
                "INSERT INTO snapshot_adapter_revisions\n\
                 (snapshot_id, language, adapter_version) VALUES (?1, ?2, ?3)",
                params![
                    snapshot.id().as_bytes().to_vec(),
                    revision.language().as_str(),
                    revision.version().as_str()
                ],
            )
            .await
            .map_err(classify_snapshot_write)?;
    }
    for change in snapshot.changes() {
        transaction
            .execute(
                "INSERT INTO snapshot_changes\n\
                 (snapshot_id, repository_path, change_kind, content_hash)\n\
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    snapshot.id().as_bytes().to_vec(),
                    change.path().as_bytes().to_vec(),
                    change.kind().as_str(),
                    change.content_hash().as_bytes().to_vec()
                ],
            )
            .await
            .map_err(classify_snapshot_write)?;
    }
    Ok(())
}

pub(crate) async fn latest_snapshot(
    connection: &Connection,
    expected_worktree_id: WorktreeId,
) -> Result<Option<Snapshot>, IndexRepositoryError> {
    validate_snapshot_chain(connection, expected_worktree_id).await?;
    let mut rows = connection
        .query(
            "SELECT snapshot_id, worktree_id, parent_snapshot_id, generation,\n\
             head_kind, head_object_id, head_reference, index_schema_version\n\
             FROM snapshots WHERE worktree_id = ?1\n\
             ORDER BY generation DESC LIMIT 1",
            [expected_worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? else {
        return Ok(None);
    };
    let header = snapshot_header_from_row(&row)?;
    if header.worktree_id != expected_worktree_id {
        return Err(IndexRepositoryError::IdentityConflict);
    }
    if rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    let adapter_revisions = read_adapter_revisions(connection, header.id).await?;
    let changes = read_snapshot_changes(connection, header.id).await?;
    Snapshot::new(
        header.id,
        header.worktree_id,
        header.parent_id,
        header.generation,
        header.head,
        header.index_schema_version,
        adapter_revisions,
        changes,
    )
    .map(Some)
    .map_err(|_| IndexRepositoryError::InvalidStoredData)
}

pub(crate) async fn current_file_state(
    connection: &Connection,
    expected_worktree_id: WorktreeId,
) -> Result<RepositoryFileState, IndexRepositoryError> {
    validate_snapshot_chain(connection, expected_worktree_id).await?;
    let mut rows = connection
        .query(
            "WITH ranked_changes AS (\n\
             SELECT changes.repository_path, changes.change_kind, changes.content_hash,\n\
                    ROW_NUMBER() OVER (\n\
                      PARTITION BY changes.repository_path\n\
                      ORDER BY snapshots.generation DESC\n\
                    ) AS path_rank\n\
             FROM snapshot_changes AS changes\n\
             JOIN snapshots ON snapshots.snapshot_id = changes.snapshot_id\n\
             WHERE snapshots.worktree_id = ?1\n\
             )\n\
             SELECT repository_path, change_kind, content_hash\n\
             FROM ranked_changes WHERE path_rank = 1\n\
             ORDER BY repository_path",
            [expected_worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let mut revisions = Vec::new();
    while let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? {
        let path: Vec<u8> = row.get(0).map_err(IndexRepositoryError::Read)?;
        let kind: String = row.get(1).map_err(IndexRepositoryError::Read)?;
        let content_hash = ContentHash::from_bytes(read_stable_id(&row, 2)?);
        match SnapshotChangeKind::try_from_stored(&kind)
            .map_err(|_| IndexRepositoryError::InvalidStoredData)?
        {
            SnapshotChangeKind::Upsert => revisions.push(FileRevision::new(
                RepositoryPath::try_from_bytes(path)
                    .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
                content_hash,
            )),
            SnapshotChangeKind::Delete => {
                RepositoryPath::try_from_bytes(path)
                    .map_err(|_| IndexRepositoryError::InvalidStoredData)?;
            }
        }
    }
    RepositoryFileState::new(revisions).map_err(|_| IndexRepositoryError::InvalidStoredData)
}

pub(crate) async fn start_index_run(
    connection: &Connection,
    worktree_id: WorktreeId,
    request: IndexRunStart,
) -> Result<IndexRunRecord, IndexRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(IndexRepositoryError::Begin)?;
    let result = start_index_run_in_transaction(&transaction, worktree_id, request).await;
    let record = match result {
        Ok(record) => record,
        Err(error) => return rollback(transaction, error).await,
    };
    transaction
        .commit()
        .await
        .map_err(IndexRepositoryError::Commit)?;
    Ok(record)
}

async fn start_index_run_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    request: IndexRunStart,
) -> Result<IndexRunRecord, IndexRepositoryError> {
    validate_index_run_sequence_in_transaction(transaction, worktree_id).await?;
    if !snapshot_belongs_to_worktree(transaction, request.snapshot_id(), worktree_id).await? {
        return Err(IndexRepositoryError::SnapshotNotFound);
    }
    if active_index_run_exists(transaction, worktree_id).await? {
        return Err(IndexRepositoryError::IndexRunAlreadyActive);
    }
    if index_run_exists(transaction, request.id()).await? {
        return Err(IndexRepositoryError::InvalidIndexRunTransition);
    }
    let sequence = next_index_run_sequence(transaction, worktree_id).await?;
    transaction
        .execute(
            "INSERT INTO index_runs (\n\
             index_run_id, worktree_id, snapshot_id, run_sequence,\n\
             ranking_policy_version, status\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'building')",
            params![
                request.id().as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec(),
                request.snapshot_id().as_bytes().to_vec(),
                sequence_to_i64(sequence)?,
                i64::from(request.ranking_policy_version().get())
            ],
        )
        .await
        .map_err(classify_index_run_write)?;
    Ok(IndexRunRecord::new(
        request.id(),
        request.snapshot_id(),
        request.ranking_policy_version(),
        sequence,
        IndexRunStatus::Building,
    ))
}

pub(crate) async fn finish_index_run(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
    outcome: IndexRunTerminalOutcome,
) -> Result<IndexRunRecord, IndexRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(IndexRepositoryError::Begin)?;
    let result = finish_index_run_in_transaction(&transaction, worktree_id, run_id, outcome).await;
    let record = match result {
        Ok(record) => record,
        Err(error) => return rollback(transaction, error).await,
    };
    transaction
        .commit()
        .await
        .map_err(IndexRepositoryError::Commit)?;
    Ok(record)
}

async fn finish_index_run_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
    outcome: IndexRunTerminalOutcome,
) -> Result<IndexRunRecord, IndexRepositoryError> {
    validate_index_run_sequence_in_transaction(transaction, worktree_id).await?;
    let Some(record) = read_index_run_from_transaction(transaction, worktree_id, run_id).await?
    else {
        return Err(IndexRepositoryError::IndexRunNotFound);
    };
    if record.status() != IndexRunStatus::Building {
        return Err(IndexRepositoryError::InvalidIndexRunTransition);
    }
    let status = outcome.status();
    let affected = transaction
        .execute(
            "UPDATE index_runs SET status = ?1\n\
             WHERE index_run_id = ?2 AND worktree_id = ?3 AND status = 'building'",
            params![
                status.as_str(),
                run_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(IndexRepositoryError::Write)?;
    if affected != 1 {
        return Err(IndexRepositoryError::InvalidIndexRunTransition);
    }
    Ok(IndexRunRecord::new(
        record.id(),
        record.snapshot_id(),
        record.ranking_policy_version(),
        record.sequence(),
        status,
    ))
}

pub(crate) async fn latest_index_run(
    connection: &Connection,
    worktree_id: WorktreeId,
    published_only: bool,
) -> Result<Option<IndexRunRecord>, IndexRepositoryError> {
    validate_index_run_sequence(connection, worktree_id).await?;
    let sql = if published_only {
        "SELECT index_run_id, snapshot_id, ranking_policy_version, run_sequence, status\n\
         FROM index_runs WHERE worktree_id = ?1 AND status = 'published'\n\
         ORDER BY run_sequence DESC LIMIT 1"
    } else {
        "SELECT index_run_id, snapshot_id, ranking_policy_version, run_sequence, status\n\
         FROM index_runs WHERE worktree_id = ?1\n\
         ORDER BY run_sequence DESC LIMIT 1"
    };
    let mut rows = connection
        .query(sql, [worktree_id.as_bytes().to_vec()])
        .await
        .map_err(IndexRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? else {
        return Ok(None);
    };
    let record = index_run_from_row(&row)?;
    if !snapshot_exists_for_connection(connection, record.snapshot_id(), worktree_id).await? {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    if rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    Ok(Some(record))
}

async fn latest_snapshot_position(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<(SnapshotId, WorktreeGeneration)>, IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT snapshot_id, generation FROM snapshots\n\
             WHERE worktree_id = ?1 ORDER BY generation DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? else {
        return Ok(None);
    };
    let id = SnapshotId::from_bytes(read_stable_id(&row, 0)?);
    let generation = read_generation(&row, 1)?;
    Ok(Some((id, generation)))
}

async fn snapshot_exists(
    transaction: &Transaction,
    snapshot_id: SnapshotId,
) -> Result<bool, IndexRepositoryError> {
    query_transaction_count(
        transaction,
        "SELECT COUNT(*) FROM snapshots WHERE snapshot_id = ?1",
        snapshot_id.as_bytes().to_vec(),
    )
    .await
    .map(|count| count != 0)
}

async fn snapshot_belongs_to_worktree(
    transaction: &Transaction,
    snapshot_id: SnapshotId,
    worktree_id: WorktreeId,
) -> Result<bool, IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM snapshots WHERE snapshot_id = ?1 AND worktree_id = ?2",
            params![
                snapshot_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    Ok(count == 1)
}

async fn active_index_run_exists(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<bool, IndexRepositoryError> {
    query_transaction_count(
        transaction,
        "SELECT COUNT(*) FROM index_runs WHERE worktree_id = ?1 AND status = 'building'",
        worktree_id.as_bytes().to_vec(),
    )
    .await
    .and_then(|count| match count {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexRepositoryError::InvalidStoredData),
    })
}

async fn index_run_exists(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<bool, IndexRepositoryError> {
    query_transaction_count(
        transaction,
        "SELECT COUNT(*) FROM index_runs WHERE index_run_id = ?1",
        run_id.as_bytes().to_vec(),
    )
    .await
    .map(|count| count != 0)
}

async fn next_index_run_sequence(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<IndexRunSequence, IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(run_sequence), 0) FROM index_runs WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let current: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    let next = current
        .checked_add(1)
        .ok_or(IndexRepositoryError::SequenceExhausted)?;
    u64::try_from(next)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
        .and_then(|value| {
            IndexRunSequence::new(value).map_err(|_| IndexRepositoryError::SequenceExhausted)
        })
}

async fn read_index_run_from_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
) -> Result<Option<IndexRunRecord>, IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id, ranking_policy_version, run_sequence, status\n\
             FROM index_runs WHERE index_run_id = ?1 AND worktree_id = ?2",
            params![run_id.as_bytes().to_vec(), worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? else {
        return Ok(None);
    };
    Ok(Some(index_run_from_row(&row)?))
}

async fn validate_snapshot_chain(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<(), IndexRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT COUNT(*), COALESCE(MAX(generation), 0)\n\
             FROM snapshots WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    let maximum: i64 = row.get(1).map_err(IndexRepositoryError::Read)?;
    if count != maximum {
        return Err(IndexRepositoryError::InvalidStoredData);
    }

    let mut invalid_rows = connection
        .query(
            "SELECT COUNT(*) FROM snapshots AS child\n\
             LEFT JOIN snapshots AS parent\n\
               ON parent.snapshot_id = child.parent_snapshot_id\n\
              AND parent.worktree_id = child.worktree_id\n\
             WHERE child.worktree_id = ?1 AND (\n\
               (child.generation = 1 AND child.parent_snapshot_id IS NOT NULL) OR\n\
               (child.generation > 1 AND (\n\
                 child.parent_snapshot_id IS NULL OR parent.snapshot_id IS NULL OR\n\
                 parent.generation <> child.generation - 1\n\
               ))\n\
             )",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = invalid_rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let invalid: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    if invalid != 0 {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn validate_snapshot_chain_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(), IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*), COALESCE(MAX(generation), 0)\n\
             FROM snapshots WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    let maximum: i64 = row.get(1).map_err(IndexRepositoryError::Read)?;
    if count != maximum {
        return Err(IndexRepositoryError::InvalidStoredData);
    }

    let mut invalid_rows = transaction
        .query(
            "SELECT COUNT(*) FROM snapshots AS child\n\
             LEFT JOIN snapshots AS parent\n\
               ON parent.snapshot_id = child.parent_snapshot_id\n\
              AND parent.worktree_id = child.worktree_id\n\
             WHERE child.worktree_id = ?1 AND (\n\
               (child.generation = 1 AND child.parent_snapshot_id IS NOT NULL) OR\n\
               (child.generation > 1 AND (\n\
                 child.parent_snapshot_id IS NULL OR parent.snapshot_id IS NULL OR\n\
                 parent.generation <> child.generation - 1\n\
               ))\n\
             )",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = invalid_rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let invalid: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    if invalid != 0 {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn validate_index_run_sequence(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<(), IndexRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT COUNT(*), COALESCE(MAX(run_sequence), 0)\n\
             FROM index_runs WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    let maximum: i64 = row.get(1).map_err(IndexRepositoryError::Read)?;
    if count != maximum {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn validate_index_run_sequence_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(), IndexRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*), COALESCE(MAX(run_sequence), 0)\n\
             FROM index_runs WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    let maximum: i64 = row.get(1).map_err(IndexRepositoryError::Read)?;
    if count != maximum {
        return Err(IndexRepositoryError::InvalidStoredData);
    }
    Ok(())
}

fn snapshot_header_from_row(row: &libsql::Row) -> Result<SnapshotHeader, IndexRepositoryError> {
    let id = SnapshotId::from_bytes(read_stable_id(row, 0)?);
    let worktree_id = WorktreeId::from_bytes(read_stable_id(row, 1)?);
    let parent_id = read_optional_stable_id(row, 2)?.map(SnapshotId::from_bytes);
    let generation = read_generation(row, 3)?;
    let kind: String = row.get(4).map_err(IndexRepositoryError::Read)?;
    let object_id: Option<String> = row.get(5).map_err(IndexRepositoryError::Read)?;
    let reference: Option<String> = row.get(6).map_err(IndexRepositoryError::Read)?;
    let head = parse_head(&kind, object_id, reference)?;
    let schema: i64 = row.get(7).map_err(IndexRepositoryError::Read)?;
    let schema = u32::try_from(schema)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
        .and_then(|value| {
            IndexSchemaVersion::new(value).map_err(|_| IndexRepositoryError::InvalidStoredData)
        })?;
    Ok(SnapshotHeader {
        id,
        worktree_id,
        parent_id,
        generation,
        head,
        index_schema_version: schema,
    })
}

async fn read_adapter_revisions(
    connection: &Connection,
    snapshot_id: SnapshotId,
) -> Result<Vec<LanguageAdapterRevision>, IndexRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT language, adapter_version FROM snapshot_adapter_revisions\n\
             WHERE snapshot_id = ?1 ORDER BY language",
            [snapshot_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let mut revisions = Vec::new();
    while let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? {
        let language: String = row.get(0).map_err(IndexRepositoryError::Read)?;
        let version: String = row.get(1).map_err(IndexRepositoryError::Read)?;
        revisions.push(LanguageAdapterRevision::new(
            IndexLanguage::try_from_stored(&language)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
            LanguageAdapterVersion::try_from_string(version)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
        ));
    }
    Ok(revisions)
}

async fn read_snapshot_changes(
    connection: &Connection,
    snapshot_id: SnapshotId,
) -> Result<Vec<SnapshotChange>, IndexRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT repository_path, change_kind, content_hash FROM snapshot_changes\n\
             WHERE snapshot_id = ?1 ORDER BY repository_path",
            [snapshot_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let mut changes = Vec::new();
    while let Some(row) = rows.next().await.map_err(IndexRepositoryError::Read)? {
        let path: Vec<u8> = row.get(0).map_err(IndexRepositoryError::Read)?;
        let kind: String = row.get(1).map_err(IndexRepositoryError::Read)?;
        changes.push(SnapshotChange::new(
            RepositoryPath::try_from_bytes(path)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
            ContentHash::from_bytes(read_stable_id(&row, 2)?),
            SnapshotChangeKind::try_from_stored(&kind)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
        ));
    }
    Ok(changes)
}

fn index_run_from_row(row: &libsql::Row) -> Result<IndexRunRecord, IndexRepositoryError> {
    let id = IndexRunId::from_bytes(read_stable_id(row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_stable_id(row, 1)?);
    let policy: i64 = row.get(2).map_err(IndexRepositoryError::Read)?;
    let policy = u32::try_from(policy)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
        .and_then(|value| {
            RankingPolicyVersion::new(value).map_err(|_| IndexRepositoryError::InvalidStoredData)
        })?;
    let raw_sequence: i64 = row.get(3).map_err(IndexRepositoryError::Read)?;
    let sequence = u64::try_from(raw_sequence)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
        .and_then(|value| {
            IndexRunSequence::new(value).map_err(|_| IndexRepositoryError::InvalidStoredData)
        })?;
    let status: String = row.get(4).map_err(IndexRepositoryError::Read)?;
    let status = IndexRunStatus::try_from_stored(&status)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)?;
    Ok(IndexRunRecord::new(
        id,
        snapshot_id,
        policy,
        sequence,
        status,
    ))
}

async fn snapshot_exists_for_connection(
    connection: &Connection,
    snapshot_id: SnapshotId,
    worktree_id: WorktreeId,
) -> Result<bool, IndexRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT COUNT(*) FROM snapshots WHERE snapshot_id = ?1 AND worktree_id = ?2",
            params![
                snapshot_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexRepositoryError::Read)?;
    Ok(count == 1)
}

async fn query_transaction_count(
    transaction: &Transaction,
    sql: &str,
    parameter: Vec<u8>,
) -> Result<i64, IndexRepositoryError> {
    let mut rows = transaction
        .query(sql, [parameter])
        .await
        .map_err(IndexRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexRepositoryError::Read)?
        .ok_or(IndexRepositoryError::InvalidStoredData)?;
    row.get(0).map_err(IndexRepositoryError::Read)
}

fn read_stable_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], IndexRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(IndexRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
}

fn read_optional_stable_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, IndexRepositoryError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(IndexRepositoryError::Read)?;
    bytes
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| IndexRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_generation(
    row: &libsql::Row,
    index: i32,
) -> Result<WorktreeGeneration, IndexRepositoryError> {
    let raw: i64 = row.get(index).map_err(IndexRepositoryError::Read)?;
    u64::try_from(raw)
        .map_err(|_| IndexRepositoryError::InvalidStoredData)
        .and_then(|value| {
            WorktreeGeneration::new(value).map_err(|_| IndexRepositoryError::InvalidStoredData)
        })
}

fn parse_head(
    kind: &str,
    object_id: Option<String>,
    reference: Option<String>,
) -> Result<GitHead, IndexRepositoryError> {
    match (kind, object_id, reference) {
        ("born", Some(object_id), reference) => Ok(GitHead::Born {
            object_id: GitObjectId::try_from_hex(object_id)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
            reference: reference
                .map(GitReferenceName::try_from_full_name)
                .transpose()
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
        }),
        ("unborn", None, Some(reference)) => Ok(GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name(reference)
                .map_err(|_| IndexRepositoryError::InvalidStoredData)?,
        }),
        _ => Err(IndexRepositoryError::InvalidStoredData),
    }
}

fn generation_to_i64(generation: WorktreeGeneration) -> Result<i64, IndexRepositoryError> {
    i64::try_from(generation.get()).map_err(|_| IndexRepositoryError::SequenceExhausted)
}

fn sequence_to_i64(sequence: IndexRunSequence) -> Result<i64, IndexRepositoryError> {
    i64::try_from(sequence.get()).map_err(|_| IndexRepositoryError::SequenceExhausted)
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_snapshot_write(source: libsql::Error) -> IndexRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        IndexRepositoryError::SnapshotConflict
    } else {
        IndexRepositoryError::Write(source)
    }
}

fn classify_index_run_write(source: libsql::Error) -> IndexRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        IndexRepositoryError::InvalidIndexRunTransition
    } else {
        IndexRepositoryError::Write(source)
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: IndexRepositoryError,
) -> Result<T, IndexRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(IndexRepositoryError::Rollback(source)),
    }
}

struct SnapshotHeader {
    id: SnapshotId,
    worktree_id: WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: WorktreeGeneration,
    head: GitHead,
    index_schema_version: IndexSchemaVersion,
}

struct HeadFields {
    kind: &'static str,
    object_id: Option<String>,
    reference: Option<String>,
}

impl From<&GitHead> for HeadFields {
    fn from(head: &GitHead) -> Self {
        match head {
            GitHead::Born {
                object_id,
                reference,
            } => Self {
                kind: "born",
                object_id: Some(object_id.as_str().to_owned()),
                reference: reference
                    .as_ref()
                    .map(|reference| reference.as_str().to_owned()),
            },
            GitHead::Unborn { reference } => Self {
                kind: "unborn",
                object_id: None,
                reference: Some(reference.as_str().to_owned()),
            },
        }
    }
}

#[derive(Debug)]
pub(crate) enum IndexRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Rollback(libsql::Error),
    Commit(libsql::Error),
    InvalidStoredData,
    IdentityConflict,
    SnapshotConflict,
    SnapshotNotFound,
    IndexRunAlreadyActive,
    IndexRunNotFound,
    InvalidIndexRunTransition,
    SequenceExhausted,
}

impl IndexRepositoryError {
    pub(crate) fn classify(self) -> KnowledgeIndexFailure {
        match self {
            Self::Read(ref source) | Self::Write(ref source) if is_corruption(source) => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::Corrupt)
            }
            Self::InvalidStoredData => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::InvalidStoredData)
            }
            Self::IdentityConflict => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::IdentityConflict)
            }
            Self::SnapshotConflict => KnowledgeIndexFailure::SnapshotConflict,
            Self::SnapshotNotFound => KnowledgeIndexFailure::SnapshotNotFound,
            Self::IndexRunAlreadyActive => KnowledgeIndexFailure::IndexRunAlreadyActive,
            Self::IndexRunNotFound => KnowledgeIndexFailure::IndexRunNotFound,
            Self::InvalidIndexRunTransition => KnowledgeIndexFailure::InvalidIndexRunTransition,
            Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Rollback(_)
            | Self::Commit(_)
            | Self::SequenceExhausted => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::Unavailable)
            }
        }
    }
}

impl fmt::Display for IndexRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Begin(_) => formatter.write_str("could not begin index repository transaction"),
            Self::Read(_) => formatter.write_str("could not read index repository data"),
            Self::Write(_) => formatter.write_str("could not write index repository data"),
            Self::Rollback(_) => {
                formatter.write_str("could not roll back index repository transaction")
            }
            Self::Commit(_) => formatter.write_str("could not commit index repository transaction"),
            Self::InvalidStoredData => formatter.write_str("index repository data is invalid"),
            Self::IdentityConflict => formatter.write_str("index repository identity conflicts"),
            Self::SnapshotConflict => formatter.write_str("snapshot chain conflicts"),
            Self::SnapshotNotFound => formatter.write_str("snapshot was not found"),
            Self::IndexRunAlreadyActive => formatter.write_str("index run is already active"),
            Self::IndexRunNotFound => formatter.write_str("index run was not found"),
            Self::InvalidIndexRunTransition => {
                formatter.write_str("index run transition is invalid")
            }
            Self::SequenceExhausted => {
                formatter.write_str("index repository sequence is exhausted")
            }
        }
    }
}

impl Error for IndexRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Rollback(source)
            | Self::Commit(source) => Some(source),
            Self::InvalidStoredData
            | Self::IdentityConflict
            | Self::SnapshotConflict
            | Self::SnapshotNotFound
            | Self::IndexRunAlreadyActive
            | Self::IndexRunNotFound
            | Self::InvalidIndexRunTransition
            | Self::SequenceExhausted => None,
        }
    }
}
