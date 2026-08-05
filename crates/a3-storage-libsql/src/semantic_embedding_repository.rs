use crate::catalog::is_corruption;
use a3_application::{
    EmbeddingOperationControl, KnowledgeStoreFailure, SemanticCacheRebuildControl,
    SemanticEmbeddingStoreFailure,
};
use a3_domain::{
    BodyHash, EmbeddingCacheKey, EmbeddingDataType, EmbeddingDimension, EmbeddingModelProfile,
    EmbeddingQuantization, EmbeddingVector, EmbeddingVectorNormalization,
    NormalizedRetrievalSignal, Progress, SemanticCardId, SemanticEmbedding, SnapshotId, VectorHit,
    VectorSearchCapability, VectorSearchLimit, VectorSearchResult, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_CACHE_KEYS: usize = 512;
const MAX_STORE_BATCH: usize = 64;
const MAX_LINEAR_CANDIDATES: usize = 4_096;
const DELETE_BATCH_SIZE: i64 = 4_096;
const MAX_SEARCH_DURATION: Duration = Duration::from_secs(2);
const MAX_MUTATION_DURATION: Duration = Duration::from_secs(300);
const UNIT_NORM_TOLERANCE: f64 = 0.001;

pub(crate) async fn find_cached(
    connection: &Connection,
    profile: &EmbeddingModelProfile,
    keys: &[EmbeddingCacheKey],
    control: &dyn EmbeddingOperationControl,
) -> Result<Vec<EmbeddingCacheKey>, SemanticEmbeddingRepositoryError> {
    validate_profile(profile)?;
    if keys.len() > MAX_CACHE_KEYS || keys.iter().any(|key| key.profile_id() != profile.id()) {
        return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
    }
    let guard = OperationGuard::new(control, MAX_SEARCH_DURATION)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(SemanticEmbeddingRepositoryError::Begin)?;
    let result = async {
        if !read_profile(&transaction, profile).await? {
            return Ok(Vec::new());
        }
        let mut cached = BTreeSet::new();
        for key in keys {
            guard.checkpoint()?;
            let mut rows = transaction
                .query(
                    "SELECT vector_bytes FROM embeddings\n\
                     WHERE card_id = ?1 AND profile_id = ?2 AND body_hash = ?3",
                    params![
                        key.card_id().as_bytes().to_vec(),
                        key.profile_id().as_bytes().to_vec(),
                        key.body_hash().as_bytes().to_vec()
                    ],
                )
                .await
                .map_err(SemanticEmbeddingRepositoryError::Read)?;
            if let Some(row) = rows
                .next()
                .await
                .map_err(SemanticEmbeddingRepositoryError::Read)?
            {
                let bytes: Vec<u8> = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
                decode_vector(&bytes, usize::from(profile.dimension().get()))?;
                cached.insert(*key);
            }
            if rows
                .next()
                .await
                .map_err(SemanticEmbeddingRepositoryError::Read)?
                .is_some()
            {
                return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
            }
        }
        Ok(cached.into_iter().collect())
    }
    .await;
    close_read_transaction(transaction, result).await
}

pub(crate) async fn store_batch(
    connection: &Connection,
    worktree_id: WorktreeId,
    profile: &EmbeddingModelProfile,
    embeddings: &[SemanticEmbedding],
    control: &dyn EmbeddingOperationControl,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    validate_profile(profile)?;
    validate_batch(profile, embeddings)?;
    let guard = OperationGuard::new(control, MAX_MUTATION_DURATION)?;
    if embeddings.is_empty() {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(SemanticEmbeddingRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let snapshot_id = embeddings[0].card().snapshot_id();
        validate_snapshot(&transaction, worktree_id, snapshot_id).await?;
        write_profile(&transaction, profile).await?;
        for embedding in embeddings {
            guard.checkpoint()?;
            write_card(&transaction, embedding).await?;
            write_snapshot_mapping(&transaction, embedding).await?;
            write_embedding(&transaction, profile, embedding).await?;
        }
        guard.checkpoint()
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn probe_vector_capability(
    dimension: EmbeddingDimension,
    control: &dyn EmbeddingOperationControl,
) -> Result<VectorSearchCapability, SemanticEmbeddingRepositoryError> {
    let guard = OperationGuard::new(control, MAX_SEARCH_DURATION)?;
    let Some(connection) = open_vector_memory_database().await else {
        return Ok(VectorSearchCapability::LinearFallback);
    };
    guard.checkpoint()?;
    let create_table = format!(
        "CREATE TABLE vector_capability_probe (id INTEGER PRIMARY KEY, vector FLOAT32({}))",
        dimension.get()
    );
    let unit_vector = capability_probe_vector(dimension);
    if connection.execute(&create_table, ()).await.is_err()
        || connection
            .execute(
                "CREATE INDEX vector_capability_probe_idx ON vector_capability_probe\n\
                 (libsql_vector_idx(vector, 'metric=cosine'))",
                (),
            )
            .await
            .is_err()
        || connection
            .execute(
                "INSERT INTO vector_capability_probe VALUES (1, vector(?1))",
                [unit_vector.clone()],
            )
            .await
            .is_err()
    {
        return Ok(VectorSearchCapability::LinearFallback);
    }
    let Ok(mut rows) = connection
        .query(
            "SELECT id FROM vector_top_k('vector_capability_probe_idx', ?1, 1)",
            [unit_vector],
        )
        .await
    else {
        return Ok(VectorSearchCapability::LinearFallback);
    };
    let Ok(Some(row)) = rows.next().await else {
        return Ok(VectorSearchCapability::LinearFallback);
    };
    let Ok(id) = row.get::<i64>(0) else {
        return Ok(VectorSearchCapability::LinearFallback);
    };
    if id != 1 || !matches!(rows.next().await, Ok(None)) {
        return Ok(VectorSearchCapability::LinearFallback);
    }
    guard.checkpoint()?;
    Ok(VectorSearchCapability::Indexed)
}

pub(crate) async fn search_similar(
    connection: &Connection,
    snapshot_id: SnapshotId,
    profile: &EmbeddingModelProfile,
    query: &EmbeddingVector,
    limit: VectorSearchLimit,
    preferred_capability: VectorSearchCapability,
    control: &dyn EmbeddingOperationControl,
) -> Result<VectorSearchResult, SemanticEmbeddingRepositoryError> {
    validate_profile(profile)?;
    if query.dimension() != usize::from(profile.dimension().get()) {
        return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
    }
    let guard = OperationGuard::new(control, MAX_SEARCH_DURATION)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(SemanticEmbeddingRepositoryError::Begin)?;
    let result = async {
        if !read_profile(&transaction, profile).await? {
            return empty_search_result(snapshot_id, profile, limit, preferred_capability);
        }
        let (candidates, corridor_truncated) =
            read_candidates(&transaction, snapshot_id, profile, &guard).await?;
        guard.checkpoint()?;
        let capability = if preferred_capability == VectorSearchCapability::Indexed
            && native_candidate_projection(&candidates, query, &guard).await?
        {
            VectorSearchCapability::Indexed
        } else {
            VectorSearchCapability::LinearFallback
        };
        rank_candidates(
            snapshot_id,
            profile,
            query,
            limit,
            capability,
            candidates,
            corridor_truncated,
            &guard,
        )
    }
    .await;
    close_read_transaction(transaction, result).await
}

pub(crate) async fn rebuild_semantic_cache(
    connection: &Connection,
    control: &dyn SemanticCacheRebuildControl,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let guard = OperationGuard::new(control, MAX_MUTATION_DURATION)?;
    let total = semantic_cache_row_count(connection).await?;
    let progress_total = total.max(1);
    report_rebuild_progress(control, 0, progress_total)?;
    let mut completed = 0_u64;
    for table in [
        "embeddings",
        "semantic_card_snapshots",
        "semantic_cards",
        "embedding_profiles",
    ] {
        loop {
            guard.checkpoint()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(SemanticEmbeddingRepositoryError::Begin)?;
            let sql = format!(
                "DELETE FROM {table} WHERE rowid IN\n\
                 (SELECT rowid FROM {table} LIMIT {DELETE_BATCH_SIZE})"
            );
            let affected = match transaction.execute(&sql, ()).await {
                Ok(affected) => affected,
                Err(source) => {
                    return rollback(transaction, SemanticEmbeddingRepositoryError::Write(source))
                        .await;
                }
            };
            transaction
                .commit()
                .await
                .map_err(SemanticEmbeddingRepositoryError::Commit)?;
            if affected == 0 {
                break;
            }
            completed = completed
                .checked_add(affected)
                .ok_or(SemanticEmbeddingRepositoryError::InvalidStoredData)?;
        }
        report_rebuild_progress(control, completed, progress_total)?;
    }
    if total == 0 {
        report_rebuild_progress(control, 1, progress_total)?;
    } else if completed != total {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    guard.checkpoint()
}

async fn semantic_cache_row_count(
    connection: &Connection,
) -> Result<u64, SemanticEmbeddingRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT\n\
             (SELECT COUNT(*) FROM embeddings) +\n\
             (SELECT COUNT(*) FROM semantic_card_snapshots) +\n\
             (SELECT COUNT(*) FROM semantic_cards) +\n\
             (SELECT COUNT(*) FROM embedding_profiles)",
            (),
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let count: i64 = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
    if count < 0
        || rows
            .next()
            .await
            .map_err(SemanticEmbeddingRepositoryError::Read)?
            .is_some()
    {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    u64::try_from(count).map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

fn report_rebuild_progress(
    control: &dyn SemanticCacheRebuildControl,
    completed: u64,
    total: u64,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let progress = Progress::determinate(completed, total)
        .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)?;
    control
        .report_progress(progress)
        .map_err(|_| SemanticEmbeddingRepositoryError::ProgressUnavailable)
}

fn validate_profile(
    profile: &EmbeddingModelProfile,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    if !profile.has_compatible_identity() {
        return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
    }
    Ok(())
}

fn validate_batch(
    profile: &EmbeddingModelProfile,
    embeddings: &[SemanticEmbedding],
) -> Result<(), SemanticEmbeddingRepositoryError> {
    if embeddings.len() > MAX_STORE_BATCH {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    let snapshot_id = embeddings
        .first()
        .map(|embedding| embedding.card().snapshot_id());
    let mut keys = BTreeSet::new();
    for embedding in embeddings {
        if embedding.profile_id() != profile.id()
            || embedding.vector().dimension() != usize::from(profile.dimension().get())
        {
            return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
        }
        if Some(embedding.card().snapshot_id()) != snapshot_id
            || !keys.insert(embedding.cache_key())
        {
            return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
        }
    }
    Ok(())
}

async fn validate_snapshot(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM snapshots WHERE snapshot_id = ?1 AND worktree_id = ?2",
            params![
                snapshot_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let count: i64 = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
    if count != 1
        || rows
            .next()
            .await
            .map_err(SemanticEmbeddingRepositoryError::Read)?
            .is_some()
    {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn write_profile(
    transaction: &Transaction,
    profile: &EmbeddingModelProfile,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO embedding_profiles\n\
             (profile_id, provider_id, model_id, dimensions, data_type, quantization, normalization)\n\
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                profile.id().as_bytes().to_vec(),
                profile.provider_id().as_str().to_owned(),
                profile.model_id().as_str().to_owned(),
                i64::from(profile.dimension().get()),
                data_type_name(profile),
                quantization_name(profile),
                normalization_name(profile)
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Write)?;
    if !read_profile(transaction, profile).await? {
        return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
    }
    Ok(())
}

async fn read_profile(
    transaction: &Transaction,
    profile: &EmbeddingModelProfile,
) -> Result<bool, SemanticEmbeddingRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT provider_id, model_id, dimensions, data_type, quantization, normalization\n\
             FROM embedding_profiles WHERE profile_id = ?1",
            [profile.id().as_bytes().to_vec()],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Ok(false);
    };
    let provider_id: String = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let model_id: String = row.get(1).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let dimensions: i64 = row.get(2).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let data_type: String = row.get(3).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let quantization: String = row.get(4).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let normalization: String = row.get(5).map_err(SemanticEmbeddingRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
        .is_some()
        || provider_id != profile.provider_id().as_str()
        || model_id != profile.model_id().as_str()
        || dimensions != i64::from(profile.dimension().get())
        || data_type != data_type_name(profile)
        || quantization != quantization_name(profile)
        || normalization != normalization_name(profile)
    {
        return Err(SemanticEmbeddingRepositoryError::ProfileConflict);
    }
    Ok(true)
}

async fn write_card(
    transaction: &Transaction,
    embedding: &SemanticEmbedding,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let card = embedding.card();
    transaction
        .execute(
            "INSERT OR IGNORE INTO semantic_cards\n\
             (card_id, body_hash, normalization_version, normalized_body)\n\
             VALUES (?1, ?2, ?3, ?4)",
            params![
                card.id().as_bytes().to_vec(),
                card.body_hash().as_bytes().to_vec(),
                i64::from(card.normalization_version().get()),
                card.body().to_owned()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Write)?;
    let mut rows = transaction
        .query(
            "SELECT normalization_version, normalized_body FROM semantic_cards\n\
             WHERE card_id = ?1 AND body_hash = ?2",
            params![
                card.id().as_bytes().to_vec(),
                card.body_hash().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let version: i64 = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
    let body: String = row.get(1).map_err(SemanticEmbeddingRepositoryError::Read)?;
    if version != i64::from(card.normalization_version().get())
        || body != card.body()
        || rows
            .next()
            .await
            .map_err(SemanticEmbeddingRepositoryError::Read)?
            .is_some()
    {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn write_snapshot_mapping(
    transaction: &Transaction,
    embedding: &SemanticEmbedding,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let card = embedding.card();
    transaction
        .execute(
            "INSERT OR IGNORE INTO semantic_card_snapshots (snapshot_id, card_id, body_hash)\n\
             VALUES (?1, ?2, ?3)",
            params![
                card.snapshot_id().as_bytes().to_vec(),
                card.id().as_bytes().to_vec(),
                card.body_hash().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Write)?;
    let mut rows = transaction
        .query(
            "SELECT body_hash FROM semantic_card_snapshots\n\
             WHERE snapshot_id = ?1 AND card_id = ?2",
            params![
                card.snapshot_id().as_bytes().to_vec(),
                card.id().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let body_hash = read_id(&row, 0)?;
    if body_hash != *card.body_hash().as_bytes()
        || rows
            .next()
            .await
            .map_err(SemanticEmbeddingRepositoryError::Read)?
            .is_some()
    {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn write_embedding(
    transaction: &Transaction,
    profile: &EmbeddingModelProfile,
    embedding: &SemanticEmbedding,
) -> Result<(), SemanticEmbeddingRepositoryError> {
    let key = embedding.cache_key();
    let bytes = encode_vector(embedding.vector());
    transaction
        .execute(
            "INSERT OR IGNORE INTO embeddings\n\
             (card_id, body_hash, profile_id, vector_bytes, created_at_ms)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                key.card_id().as_bytes().to_vec(),
                key.body_hash().as_bytes().to_vec(),
                key.profile_id().as_bytes().to_vec(),
                bytes,
                i64::try_from(embedding.created_at().as_unix_millis())
                    .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)?
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Write)?;
    let mut rows = transaction
        .query(
            "SELECT vector_bytes FROM embeddings\n\
             WHERE card_id = ?1 AND profile_id = ?2 AND body_hash = ?3",
            params![
                key.card_id().as_bytes().to_vec(),
                key.profile_id().as_bytes().to_vec(),
                key.body_hash().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let persisted: Vec<u8> = row.get(0).map_err(SemanticEmbeddingRepositoryError::Read)?;
    decode_vector(&persisted, usize::from(profile.dimension().get()))?;
    if rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
        .is_some()
    {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn read_candidates(
    transaction: &Transaction,
    snapshot_id: SnapshotId,
    profile: &EmbeddingModelProfile,
    guard: &OperationGuard<'_>,
) -> Result<(Vec<StoredCandidate>, bool), SemanticEmbeddingRepositoryError> {
    let sql = format!(
        "SELECT embeddings.card_id, embeddings.body_hash, embeddings.vector_bytes\n\
         FROM semantic_card_snapshots AS cards\n\
         JOIN embeddings ON embeddings.card_id = cards.card_id\n\
           AND embeddings.body_hash = cards.body_hash\n\
         WHERE cards.snapshot_id = ?1 AND embeddings.profile_id = ?2\n\
         ORDER BY embeddings.card_id, embeddings.body_hash\n\
         LIMIT {}",
        MAX_LINEAR_CANDIDATES + 1
    );
    let mut rows = transaction
        .query(
            &sql,
            params![
                snapshot_id.as_bytes().to_vec(),
                profile.id().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(SemanticEmbeddingRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let card_id = SemanticCardId::from_bytes(read_id(&row, 0)?);
        let body_hash = BodyHash::from_bytes(read_id(&row, 1)?);
        let bytes: Vec<u8> = row.get(2).map_err(SemanticEmbeddingRepositoryError::Read)?;
        let vector = decode_vector(&bytes, usize::from(profile.dimension().get()))?;
        candidates.push(StoredCandidate {
            card_id,
            body_hash,
            vector,
        });
    }
    let truncated = candidates.len() > MAX_LINEAR_CANDIDATES;
    candidates.truncate(MAX_LINEAR_CANDIDATES);
    Ok((candidates, truncated))
}

async fn native_candidate_projection(
    candidates: &[StoredCandidate],
    query: &EmbeddingVector,
    guard: &OperationGuard<'_>,
) -> Result<bool, SemanticEmbeddingRepositoryError> {
    if candidates.is_empty() {
        return Ok(true);
    }
    let Some(connection) = open_vector_memory_database().await else {
        return Ok(false);
    };
    let create_table = format!(
        "CREATE TABLE semantic_vector_candidates\n\
         (id INTEGER PRIMARY KEY, vector FLOAT32({}))",
        query.dimension()
    );
    if connection.execute(&create_table, ()).await.is_err()
        || connection
            .execute(
                "CREATE INDEX semantic_vector_candidates_idx ON semantic_vector_candidates\n\
                 (libsql_vector_idx(vector, 'metric=cosine'))",
                (),
            )
            .await
            .is_err()
    {
        return Ok(false);
    }
    for (index, candidate) in candidates.iter().enumerate() {
        guard.checkpoint()?;
        let id = i64::try_from(index.saturating_add(1))
            .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)?;
        if connection
            .execute(
                "INSERT INTO semantic_vector_candidates VALUES (?1, vector(?2))",
                params![id, vector_json(&candidate.vector)],
            )
            .await
            .is_err()
        {
            return Ok(false);
        }
    }
    guard.checkpoint()?;
    let Ok(candidate_count) = i64::try_from(candidates.len()) else {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    };
    let Ok(mut rows) = connection
        .query(
            "SELECT id FROM vector_top_k('semantic_vector_candidates_idx', ?1, ?2)",
            params![vector_json(query), candidate_count],
        )
        .await
    else {
        return Ok(false);
    };
    let mut ids = BTreeSet::new();
    loop {
        let row = match rows.next().await {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(_) => return Ok(false),
        };
        guard.checkpoint()?;
        let Ok(id) = row.get::<i64>(0) else {
            return Ok(false);
        };
        if id <= 0
            || usize::try_from(id).map_or(true, |value| value > candidates.len())
            || !ids.insert(id)
        {
            return Ok(false);
        }
    }
    Ok(ids.len() == candidates.len())
}

#[allow(clippy::too_many_arguments)]
fn rank_candidates(
    snapshot_id: SnapshotId,
    profile: &EmbeddingModelProfile,
    query: &EmbeddingVector,
    limit: VectorSearchLimit,
    capability: VectorSearchCapability,
    candidates: Vec<StoredCandidate>,
    corridor_truncated: bool,
    guard: &OperationGuard<'_>,
) -> Result<VectorSearchResult, SemanticEmbeddingRepositoryError> {
    let mut hits = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        guard.checkpoint()?;
        let similarity = cosine_signal(query, &candidate.vector)?;
        hits.push(VectorHit::new(
            candidate.card_id,
            candidate.body_hash,
            profile.id(),
            similarity,
        ));
    }
    hits.sort_by(|left, right| {
        right
            .similarity()
            .cmp(&left.similarity())
            .then_with(|| left.card_id().cmp(&right.card_id()))
            .then_with(|| left.body_hash().cmp(&right.body_hash()))
    });
    let result_truncated = corridor_truncated || hits.len() > usize::from(limit.get());
    hits.truncate(usize::from(limit.get()));
    VectorSearchResult::new(
        snapshot_id,
        profile.id(),
        capability,
        limit,
        hits,
        result_truncated,
    )
    .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

fn empty_search_result(
    snapshot_id: SnapshotId,
    profile: &EmbeddingModelProfile,
    limit: VectorSearchLimit,
    capability: VectorSearchCapability,
) -> Result<VectorSearchResult, SemanticEmbeddingRepositoryError> {
    VectorSearchResult::new(
        snapshot_id,
        profile.id(),
        capability,
        limit,
        Vec::new(),
        false,
    )
    .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

fn cosine_signal(
    left: &EmbeddingVector,
    right: &EmbeddingVector,
) -> Result<NormalizedRetrievalSignal, SemanticEmbeddingRepositoryError> {
    if left.dimension() != right.dimension() {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    let dot = left
        .components()
        .iter()
        .zip(right.components())
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum::<f64>()
        .clamp(-1.0, 1.0);
    let basis_points = ((dot + 1.0) * 5_000.0).round() as u16;
    NormalizedRetrievalSignal::new(basis_points)
        .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

fn encode_vector(vector: &EmbeddingVector) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.dimension().saturating_mul(4));
    for component in vector.components() {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
    bytes
}

fn decode_vector(
    bytes: &[u8],
    expected_dimension: usize,
) -> Result<EmbeddingVector, SemanticEmbeddingRepositoryError> {
    if bytes.len() != expected_dimension.saturating_mul(4) {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    let components = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    let squared_norm = components
        .iter()
        .map(|component| f64::from(*component).powi(2))
        .sum::<f64>();
    if !squared_norm.is_finite() || (squared_norm.sqrt() - 1.0).abs() > UNIT_NORM_TOLERANCE {
        return Err(SemanticEmbeddingRepositoryError::InvalidStoredData);
    }
    let dimension = u16::try_from(expected_dimension)
        .ok()
        .and_then(|value| a3_domain::EmbeddingDimension::new(value).ok())
        .ok_or(SemanticEmbeddingRepositoryError::InvalidStoredData)?;
    EmbeddingVector::normalize_l2(components, dimension)
        .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

fn vector_json(vector: &EmbeddingVector) -> String {
    let mut json = String::with_capacity(vector.dimension().saturating_mul(16));
    json.push('[');
    for (index, component) in vector.components().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&component.to_string());
    }
    json.push(']');
    json
}

fn capability_probe_vector(dimension: EmbeddingDimension) -> String {
    let mut json = String::with_capacity(usize::from(dimension.get()).saturating_mul(2));
    json.push_str("[1");
    for _ in 1..dimension.get() {
        json.push_str(",0");
    }
    json.push(']');
    json
}

async fn open_vector_memory_database() -> Option<Connection> {
    let database = libsql::Builder::new_local(":memory:").build().await.ok()?;
    database.connect().ok()
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], SemanticEmbeddingRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(SemanticEmbeddingRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| SemanticEmbeddingRepositoryError::InvalidStoredData)
}

const fn data_type_name(profile: &EmbeddingModelProfile) -> &'static str {
    match profile.data_type() {
        EmbeddingDataType::Float32 => "float32",
    }
}

const fn quantization_name(profile: &EmbeddingModelProfile) -> &'static str {
    match profile.quantization() {
        EmbeddingQuantization::None => "none",
    }
}

const fn normalization_name(profile: &EmbeddingModelProfile) -> &'static str {
    match profile.normalization() {
        EmbeddingVectorNormalization::L2Unit => "l2_unit",
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, SemanticEmbeddingRepositoryError>,
) -> Result<T, SemanticEmbeddingRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(SemanticEmbeddingRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn close_write_transaction<T>(
    transaction: Transaction,
    result: Result<T, SemanticEmbeddingRepositoryError>,
) -> Result<T, SemanticEmbeddingRepositoryError> {
    close_read_transaction(transaction, result).await
}

async fn rollback<T>(
    transaction: Transaction,
    error: SemanticEmbeddingRepositoryError,
) -> Result<T, SemanticEmbeddingRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(SemanticEmbeddingRepositoryError::Rollback(source)),
    }
}

struct OperationGuard<'a> {
    control: &'a dyn EmbeddingOperationControl,
    started: Instant,
    maximum: Duration,
}

impl<'a> OperationGuard<'a> {
    fn new(
        control: &'a dyn EmbeddingOperationControl,
        maximum: Duration,
    ) -> Result<Self, SemanticEmbeddingRepositoryError> {
        let guard = Self {
            control,
            started: Instant::now(),
            maximum,
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), SemanticEmbeddingRepositoryError> {
        if self.control.is_cancelled() {
            return Err(SemanticEmbeddingRepositoryError::Cancelled);
        }
        if self.started.elapsed() > self.maximum {
            return Err(SemanticEmbeddingRepositoryError::TimedOut);
        }
        Ok(())
    }
}

struct StoredCandidate {
    card_id: SemanticCardId,
    body_hash: BodyHash,
    vector: EmbeddingVector,
}

#[derive(Debug)]
pub(crate) enum SemanticEmbeddingRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    ProfileConflict,
    InvalidStoredData,
    TimedOut,
    Cancelled,
    ProgressUnavailable,
}

impl SemanticEmbeddingRepositoryError {
    pub(crate) fn classify(&self) -> SemanticEmbeddingStoreFailure {
        match self {
            Self::ProfileConflict => SemanticEmbeddingStoreFailure::ProfileConflict,
            Self::InvalidStoredData => SemanticEmbeddingStoreFailure::InvalidStoredData,
            Self::TimedOut => SemanticEmbeddingStoreFailure::TimedOut,
            Self::Cancelled => SemanticEmbeddingStoreFailure::Cancelled,
            Self::ProgressUnavailable => SemanticEmbeddingStoreFailure::ProgressUnavailable,
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Commit(source)
            | Self::Rollback(source) => {
                SemanticEmbeddingStoreFailure::Storage(if is_corruption(source) {
                    KnowledgeStoreFailure::Corrupt
                } else {
                    KnowledgeStoreFailure::Unavailable
                })
            }
        }
    }
}

impl fmt::Display for SemanticEmbeddingRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin semantic-cache transaction",
            Self::Read(_) => "could not read semantic-cache state",
            Self::Write(_) => "could not write semantic-cache state",
            Self::Commit(_) => "could not commit semantic-cache transaction",
            Self::Rollback(_) => "could not roll back semantic-cache transaction",
            Self::ProfileConflict => "semantic-cache profile metadata conflicts",
            Self::InvalidStoredData => "semantic-cache state is invalid",
            Self::TimedOut => "semantic-cache operation timed out",
            Self::Cancelled => "semantic-cache operation was cancelled",
            Self::ProgressUnavailable => "semantic-cache rebuild progress is unavailable",
        })
    }
}

impl Error for SemanticEmbeddingRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::ProfileConflict
            | Self::InvalidStoredData
            | Self::TimedOut
            | Self::Cancelled
            | Self::ProgressUnavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OperationGuard, StoredCandidate, cosine_signal, decode_vector, encode_vector,
        native_candidate_projection, probe_vector_capability, rank_candidates, vector_json,
    };
    use a3_application::EmbeddingOperationControl;
    use a3_domain::{
        BodyHash, EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
        EmbeddingProviderId, EmbeddingVector, NormalizedRetrievalSignal, SemanticCardId,
        SnapshotId, VectorSearchCapability, VectorSearchLimit,
    };
    use futures::executor::block_on;
    use std::time::Duration;

    #[test]
    fn float32_codec_and_similarity_are_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let dimension = EmbeddingDimension::new(2)?;
        let left = EmbeddingVector::normalize_l2(vec![1.0, 0.0], dimension)?;
        let same = decode_vector(&encode_vector(&left), 2)?;
        let opposite = EmbeddingVector::normalize_l2(vec![-1.0, 0.0], dimension)?;
        assert_eq!(same.components(), left.components());
        assert_eq!(
            cosine_signal(&left, &same)?,
            NormalizedRetrievalSignal::FULL
        );
        assert_eq!(
            cosine_signal(&left, &opposite)?,
            NormalizedRetrievalSignal::ZERO
        );
        assert_eq!(vector_json(&left), "[1,0]");
        Ok(())
    }

    #[test]
    fn bundled_libsql_exposes_the_bounded_native_vector_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            let control = TestControl;
            let dimension = EmbeddingDimension::new(2)?;
            assert_eq!(
                probe_vector_capability(dimension, &control).await?,
                VectorSearchCapability::Indexed
            );
            let query = EmbeddingVector::normalize_l2(vec![1.0, 0.0], dimension)?;
            let candidates = vec![
                StoredCandidate {
                    card_id: SemanticCardId::from_bytes([1; 32]),
                    body_hash: BodyHash::from_bytes([2; 32]),
                    vector: query.clone(),
                },
                StoredCandidate {
                    card_id: SemanticCardId::from_bytes([3; 32]),
                    body_hash: BodyHash::from_bytes([4; 32]),
                    vector: EmbeddingVector::normalize_l2(vec![0.0, 1.0], dimension)?,
                },
            ];
            let guard = OperationGuard::new(&control, Duration::from_secs(2))?;
            assert!(native_candidate_projection(&candidates, &query, &guard).await?);
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn bounded_linear_fallback_has_stable_order_and_visible_truncation()
    -> Result<(), Box<dyn std::error::Error>> {
        let control = TestControl;
        let guard = OperationGuard::new(&control, Duration::from_secs(2))?;
        let dimension = EmbeddingDimension::new(2)?;
        let profile = EmbeddingModelProfile::v1(
            EmbeddingProviderId::new("test-local".to_owned())?,
            EmbeddingModelId::new("fallback".to_owned())?,
            dimension,
            EmbeddingBatchSize::new(8)?,
        );
        let query = EmbeddingVector::normalize_l2(vec![1.0, 0.0], dimension)?;
        let candidates = vec![
            StoredCandidate {
                card_id: SemanticCardId::from_bytes([2; 32]),
                body_hash: BodyHash::from_bytes([2; 32]),
                vector: EmbeddingVector::normalize_l2(vec![0.0, 1.0], dimension)?,
            },
            StoredCandidate {
                card_id: SemanticCardId::from_bytes([1; 32]),
                body_hash: BodyHash::from_bytes([1; 32]),
                vector: query.clone(),
            },
            StoredCandidate {
                card_id: SemanticCardId::from_bytes([3; 32]),
                body_hash: BodyHash::from_bytes([3; 32]),
                vector: EmbeddingVector::normalize_l2(vec![-1.0, 0.0], dimension)?,
            },
        ];
        let result = rank_candidates(
            SnapshotId::from_bytes([9; 32]),
            &profile,
            &query,
            VectorSearchLimit::new(2)?,
            VectorSearchCapability::LinearFallback,
            candidates,
            false,
            &guard,
        )?;
        assert_eq!(result.capability(), VectorSearchCapability::LinearFallback);
        assert_eq!(
            result.hits()[0].card_id(),
            SemanticCardId::from_bytes([1; 32])
        );
        assert!(result.truncated());
        Ok(())
    }

    #[derive(Debug)]
    struct TestControl;

    impl EmbeddingOperationControl for TestControl {
        fn is_cancelled(&self) -> bool {
            false
        }
    }
}
