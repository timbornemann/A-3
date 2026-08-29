use crate::{
    exact_search_projection, index_codec, index_invalidation_repository, lexical_search_projection,
    module_projection_codec,
};
use a3_application::{IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeStoreFailure};
use a3_domain::{
    FileRevision, IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus,
    IndexSchemaVersion, Progress, PublishedIndex, RankingPolicyVersion, RepositoryPath, SnapshotId,
    WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_FILES: usize = 250_000;
const MAX_DIAGNOSTICS: usize = 2_000_000;
const MAX_SYMBOLS: usize = 1_000_000;
const MAX_EDGES: usize = 2_000_000;
const MAX_UNRESOLVED: usize = 2_000_000;
const MAX_MODULES: usize = 250_000;
const MAX_MEMBERSHIPS: usize = 2_000_000;
const MAX_MEMBERSHIP_EVIDENCE: usize = 4_000_000;
const MAX_MODULE_FEATURES: usize = 4_000_000;
const MAX_MUTATION_DURATION: Duration = Duration::from_secs(300);
const MAX_PROGRESS_EVENTS: u64 = 64;
const CANCELLATION_POLL_INTERVAL: u64 = 1_024;
const REBUILD_DELETE_BATCH: i64 = 4_096;
const SUPERSEDED_DELETE_BATCH: i64 = 1_024;

const RESTORE_CARD_SEARCH_FOR_RUN_SQL: &str = "INSERT INTO card_fts (index_run_id, card_id, title, purpose, body)\n\
     SELECT cards.source_index_run_id, cards.card_id,\n\
       COALESCE((\n\
         SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
           SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
           WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
             AND values_row.card_id = cards.card_id AND values_row.field_kind = 'title'\n\
           ORDER BY values_row.value_index\n\
         ) AS ordered\n\
       ), ''),\n\
       COALESCE((\n\
         SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
           SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
           WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
             AND values_row.card_id = cards.card_id AND values_row.field_kind = 'purpose'\n\
           ORDER BY values_row.value_index\n\
         ) AS ordered\n\
       ), ''),\n\
       COALESCE((\n\
         SELECT group_concat(ordered.field_kind || ': ' || ordered.joined_values, char(10))\n\
         FROM (\n\
           SELECT fields.field_kind, COALESCE((\n\
             SELECT group_concat(ordered_values.field_value, char(10)) FROM (\n\
               SELECT values_row.field_value\n\
               FROM module_card_field_values AS values_row\n\
               WHERE values_row.source_index_run_id = fields.source_index_run_id\n\
                 AND values_row.card_id = fields.card_id\n\
                 AND values_row.field_kind = fields.field_kind\n\
               ORDER BY values_row.value_index\n\
             ) AS ordered_values\n\
           ), '') AS joined_values\n\
           FROM module_card_fields AS fields\n\
           WHERE fields.source_index_run_id = cards.source_index_run_id\n\
             AND fields.card_id = cards.card_id\n\
           ORDER BY CASE fields.field_kind\n\
             WHEN 'title' THEN 1 WHEN 'paths' THEN 2 WHEN 'purpose' THEN 3\n\
             WHEN 'responsibilities' THEN 4 WHEN 'public-surface' THEN 5\n\
             WHEN 'entrypoints' THEN 6 WHEN 'dependencies' THEN 7\n\
             WHEN 'data-flows' THEN 8 WHEN 'invariants' THEN 9 WHEN 'tests' THEN 10\n\
             WHEN 'risks' THEN 11 WHEN 'open-questions' THEN 12 ELSE 13 END\n\
         ) AS ordered\n\
       ), '')\n\
     FROM module_cards AS cards\n\
     WHERE cards.source_index_run_id = ?1 AND cards.status = 'published'\n\
       AND EXISTS (\n\
         SELECT 1 FROM module_card_lifecycle AS lifecycle\n\
         WHERE lifecycle.source_index_run_id = cards.source_index_run_id\n\
           AND lifecycle.card_id = cards.card_id AND lifecycle.status = 'published'\n\
       )\n\
       AND EXISTS (\n\
         SELECT 1 FROM modules\n\
         WHERE modules.index_run_id = cards.source_index_run_id\n\
           AND modules.module_id = cards.module_id\n\
       )\n\
       AND EXISTS (\n\
         SELECT 1 FROM module_card_fields AS fields\n\
         WHERE fields.source_index_run_id = cards.source_index_run_id\n\
           AND fields.card_id = cards.card_id\n\
       )\n\
       AND NOT EXISTS (\n\
         SELECT 1 FROM module_card_fields AS fields\n\
         WHERE fields.source_index_run_id = cards.source_index_run_id\n\
           AND fields.card_id = cards.card_id\n\
           AND NOT EXISTS (\n\
             SELECT 1 FROM module_card_field_values AS values_row\n\
             WHERE values_row.source_index_run_id = fields.source_index_run_id\n\
               AND values_row.card_id = fields.card_id\n\
               AND values_row.field_kind = fields.field_kind\n\
           )\n\
       )";

pub(crate) async fn publish_index(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
    publication: &IndexPublication,
    control: &dyn IndexPersistenceControl,
) -> Result<PublishedIndex, IndexPublicationRepositoryError> {
    validate_resource_limits(publication)?;
    let search_projection = exact_search_projection::build_projection(publication)?;
    let lexical_projection =
        lexical_search_projection::build_projection(publication, &search_projection)?;
    let total = publication_work_units(publication, &search_projection, &lexical_projection)?;
    let mut progress = MutationProgress::new(control, total)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(IndexPublicationRepositoryError::Begin)?;
    let result = publish_index_in_transaction(
        &transaction,
        worktree_id,
        run_id,
        publication,
        &search_projection,
        &lexical_projection,
        &mut progress,
    )
    .await;
    let published = match result {
        Ok(published) => published,
        Err(error) => return rollback(transaction, error).await,
    };
    transaction
        .commit()
        .await
        .map_err(IndexPublicationRepositoryError::Commit)?;
    Ok(published)
}

async fn publish_index_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
    publication: &IndexPublication,
    search_projection: &exact_search_projection::ExactSearchProjection,
    lexical_projection: &lexical_search_projection::LexicalSearchProjection,
    progress: &mut MutationProgress<'_>,
) -> Result<PublishedIndex, IndexPublicationRepositoryError> {
    progress.checkpoint()?;
    let Some(run) = read_index_run(transaction, worktree_id, run_id).await? else {
        return Err(IndexPublicationRepositoryError::IndexRunNotFound);
    };
    if run.status() != IndexRunStatus::Building {
        return Err(IndexPublicationRepositoryError::InvalidIndexRunTransition);
    }
    if run.snapshot_id() != publication.graph().snapshot_id()
        || run.snapshot_id() != publication.ranking().snapshot_id()
        || run.ranking_policy_version() != publication.ranking().policy_version()
    {
        return Err(IndexPublicationRepositoryError::PublicationMismatch);
    }
    if published_snapshot_policy_exists(transaction, run).await? {
        return Err(IndexPublicationRepositoryError::InvalidIndexRunTransition);
    }
    if publication_rows_exist(transaction, run_id).await? {
        return Err(IndexPublicationRepositoryError::InvalidIndexRunTransition);
    }

    let durable_files =
        effective_files_at_snapshot(transaction, worktree_id, run.snapshot_id()).await?;
    if durable_files != publication.graph().files() {
        return Err(IndexPublicationRepositoryError::PublicationMismatch);
    }

    write_file_delta_projection(transaction, worktree_id, run).await?;
    progress.advance(1)?;
    index_codec::write_publication_rows(transaction, run_id, publication, progress).await?;
    exact_search_projection::write_projection(transaction, run_id, search_projection, progress)
        .await?;
    lexical_search_projection::write_projection(transaction, run_id, lexical_projection, progress)
        .await?;
    restore_durable_card_search_projection(transaction, run_id, progress).await?;
    let published_record = IndexRunRecord::new(
        run.id(),
        run.snapshot_id(),
        run.ranking_policy_version(),
        run.sequence(),
        IndexRunStatus::Published,
    );
    let published = PublishedIndex::new(published_record, publication.clone())
        .map_err(|_| IndexPublicationRepositoryError::PublicationMismatch)?;
    index_invalidation_repository::invalidate_for_publication(transaction, &published, progress)
        .await?;
    progress.advance(1)?;
    delete_superseded_publication_rows(transaction, worktree_id, run_id, progress).await?;

    let affected = transaction
        .execute(
            "UPDATE index_runs SET status = 'published'\n\
             WHERE index_run_id = ?1 AND worktree_id = ?2 AND status = 'building'",
            params![run_id.as_bytes().to_vec(), worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    if affected != 1 {
        return Err(IndexPublicationRepositoryError::InvalidIndexRunTransition);
    }
    progress.advance(1)?;

    Ok(published)
}

async fn delete_superseded_publication_rows(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    current_run_id: IndexRunId,
    progress: &MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    for table in [
        "repository_card_entrypoints",
        "module_tests",
        "module_entrypoints",
        "module_central_symbols",
        "module_membership_evidence",
        "module_members",
        "module_manifests",
        "modules",
        "module_projections",
        "ranking_projections",
        "unresolved_edges",
        "symbol_edges",
        "symbol_fts",
        "path_fts",
        "card_fts",
        "lexical_search_projections",
        "exact_search_symbols",
        "exact_search_manifests",
        "exact_search_projections",
        "index_parse_diagnostics",
        "index_file_analyses",
        "symbols",
        "file_revisions",
    ] {
        loop {
            progress.checkpoint()?;
            let sql = format!(
                "DELETE FROM {table} WHERE rowid IN (\n\
                 SELECT rows.rowid FROM {table} AS rows\n\
                 JOIN index_runs AS runs ON runs.index_run_id = rows.index_run_id\n\
                 WHERE runs.worktree_id = ?1 AND rows.index_run_id <> ?2\n\
                 LIMIT {SUPERSEDED_DELETE_BATCH}\n\
                 )"
            );
            let affected = transaction
                .execute(
                    &sql,
                    params![
                        worktree_id.as_bytes().to_vec(),
                        current_run_id.as_bytes().to_vec()
                    ],
                )
                .await
                .map_err(IndexPublicationRepositoryError::Write)?;
            if affected == 0 {
                break;
            }
        }
    }
    Ok(())
}

async fn published_snapshot_policy_exists(
    transaction: &Transaction,
    run: IndexRunRecord,
) -> Result<bool, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM index_runs\n\
             WHERE snapshot_id = ?1 AND ranking_policy_version = ?2 AND status = 'published'",
            params![
                run.snapshot_id().as_bytes().to_vec(),
                i64::from(run.ranking_policy_version().get())
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    match count {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

pub(crate) async fn latest_published_index(
    connection: &Connection,
    worktree_id: WorktreeId,
    control: &dyn IndexPersistenceControl,
) -> Result<Option<a3_domain::PublishedIndex>, IndexPublicationRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(IndexPublicationRepositoryError::Begin)?;
    let result = async {
        let Some(run) = read_latest_published_run(&transaction, worktree_id).await? else {
            return Ok(None);
        };
        if !module_projection_exists(&transaction, run.id()).await? {
            if snapshot_predates_module_projection(&transaction, run.snapshot_id()).await? {
                return Ok(None);
            }
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
        let total = publication_row_count(&transaction, run.id()).await?.max(1);
        let mut progress = MutationProgress::new(control, total)?;
        let manifests =
            exact_search_projection::read_manifest_files(&transaction, run.id(), &mut progress)
                .await?;
        let publication =
            index_codec::read_publication_rows(&transaction, run, manifests, &mut progress).await?;
        progress.complete_if_empty()?;
        a3_domain::PublishedIndex::new(run, publication)
            .map(Some)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
    }
    .await;
    match result {
        Ok(index) => {
            transaction
                .commit()
                .await
                .map_err(IndexPublicationRepositoryError::Commit)?;
            Ok(index)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn module_projection_exists(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<bool, IndexPublicationRepositoryError> {
    match count_rows(
        transaction,
        "SELECT COUNT(*) FROM module_projections WHERE index_run_id = ?1",
        run_id.as_bytes().to_vec(),
    )
    .await?
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

async fn snapshot_predates_module_projection(
    transaction: &Transaction,
    snapshot_id: SnapshotId,
) -> Result<bool, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_schema_version FROM snapshots WHERE snapshot_id = ?1",
            [snapshot_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let version: i64 = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    let version =
        u32::try_from(version).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    Ok(version < IndexSchemaVersion::v4().get())
}

pub(crate) async fn rebuild_regenerable_index(
    connection: &Connection,
    worktree_id: WorktreeId,
    control: &dyn IndexPersistenceControl,
) -> Result<(), IndexPublicationRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(IndexPublicationRepositoryError::Begin)?;
    let result = async {
        if count_rows(
            &transaction,
            "SELECT COUNT(*) FROM index_runs WHERE worktree_id = ?1 AND status = 'building'",
            worktree_id.as_bytes().to_vec(),
        )
        .await?
            != 0
        {
            return Err(IndexPublicationRepositoryError::IndexRunAlreadyActive);
        }
        let total = rebuild_row_count(&transaction, worktree_id)
            .await?
            .checked_add(1)
            .ok_or(IndexPublicationRepositoryError::ResourceLimit)?;
        let mut progress = MutationProgress::new(control, total)?;
        for table in [
            "repository_card_entrypoints",
            "module_tests",
            "module_entrypoints",
            "module_central_symbols",
            "module_membership_evidence",
            "module_members",
            "module_manifests",
            "modules",
            "module_projections",
            "ranking_projections",
            "unresolved_edges",
            "symbol_edges",
            "symbol_fts",
            "path_fts",
            "card_fts",
            "lexical_search_projections",
            "exact_search_symbols",
            "exact_search_manifests",
            "exact_search_projections",
            "index_parse_diagnostics",
            "index_file_analyses",
            "symbols",
            "file_revisions",
        ] {
            delete_rebuild_rows(&transaction, worktree_id, table, &mut progress).await?;
        }
        delete_index_runs(&transaction, worktree_id, &mut progress).await?;
        progress.advance(1)?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        return rollback(transaction, error).await;
    }
    transaction
        .commit()
        .await
        .map_err(IndexPublicationRepositoryError::Commit)
}

fn publication_work_units(
    publication: &IndexPublication,
    search_projection: &exact_search_projection::ExactSearchProjection,
    lexical_projection: &lexical_search_projection::LexicalSearchProjection,
) -> Result<u64, IndexPublicationRepositoryError> {
    let publication_units = [
        publication.file_analyses().len(),
        publication
            .file_analyses()
            .iter()
            .map(|analysis| analysis.diagnostics().len())
            .try_fold(0_usize, usize::checked_add)
            .ok_or(IndexPublicationRepositoryError::ResourceLimit)?,
        publication.graph().symbols().len(),
        publication.graph().edges().len(),
        publication.graph().unresolved().len(),
        publication.ranking().symbols().len(),
    ]
    .into_iter()
    .try_fold(4_u64, |total, length| {
        u64::try_from(length)
            .ok()
            .and_then(|length| total.checked_add(length))
            .ok_or(IndexPublicationRepositoryError::ResourceLimit)
    })?;
    let exact_units = search_projection.work_units()?;
    let lexical_units = lexical_projection.work_units()?;
    let module_units = module_projection_codec::work_units(publication.modules())?;
    publication_units
        .checked_add(exact_units)
        .and_then(|total| total.checked_add(lexical_units))
        .and_then(|total| total.checked_add(module_units))
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)
}

async fn restore_durable_card_search_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    progress.checkpoint()?;
    let run = run_id.as_bytes().to_vec();
    let card_count = count_rows(
        transaction,
        "SELECT COUNT(*) FROM module_cards\n\
         WHERE source_index_run_id = ?1 AND status = 'published'",
        run.clone(),
    )
    .await?;
    if card_count
        > i64::try_from(MAX_MODULES).map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    if card_count == 0 {
        progress.advance(1)?;
        return Ok(());
    }

    let restorable_count = count_rows(
        transaction,
        "SELECT COUNT(*) FROM module_cards AS cards\n\
         WHERE cards.source_index_run_id = ?1 AND cards.status = 'published'\n\
           AND EXISTS (\n\
             SELECT 1 FROM module_card_lifecycle AS lifecycle\n\
             WHERE lifecycle.source_index_run_id = cards.source_index_run_id\n\
               AND lifecycle.card_id = cards.card_id AND lifecycle.status = 'published'\n\
           )\n\
           AND EXISTS (\n\
             SELECT 1 FROM modules\n\
             WHERE modules.index_run_id = cards.source_index_run_id\n\
               AND modules.module_id = cards.module_id\n\
           )\n\
           AND EXISTS (\n\
             SELECT 1 FROM module_card_fields AS fields\n\
             WHERE fields.source_index_run_id = cards.source_index_run_id\n\
               AND fields.card_id = cards.card_id\n\
           )\n\
           AND NOT EXISTS (\n\
             SELECT 1 FROM module_card_fields AS fields\n\
             WHERE fields.source_index_run_id = cards.source_index_run_id\n\
               AND fields.card_id = cards.card_id\n\
               AND NOT EXISTS (\n\
                 SELECT 1 FROM module_card_field_values AS values_row\n\
                 WHERE values_row.source_index_run_id = fields.source_index_run_id\n\
                   AND values_row.card_id = fields.card_id\n\
                   AND values_row.field_kind = fields.field_kind\n\
               )\n\
           )",
        run.clone(),
    )
    .await?;
    if restorable_count != card_count {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }

    let affected = transaction
        .execute(RESTORE_CARD_SEARCH_FOR_RUN_SQL, [run.clone()])
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    if i64::try_from(affected).ok() != Some(card_count)
        || count_rows(
            transaction,
            "SELECT COUNT(*) FROM card_fts WHERE index_run_id = ?1",
            run.clone(),
        )
        .await?
            != card_count
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    let affected = transaction
        .execute(
            "UPDATE lexical_search_projections SET card_count = ?2\n\
             WHERE index_run_id = ?1 AND card_count = 0",
            params![run, card_count],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    if affected != 1 {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    progress.advance(1)
}

async fn publication_row_count(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<u64, IndexPublicationRepositoryError> {
    let mut total = 0_u64;
    for (table, limit) in [
        ("file_revisions", MAX_FILES),
        ("index_file_analyses", MAX_FILES),
        ("index_parse_diagnostics", MAX_DIAGNOSTICS),
        ("symbols", MAX_SYMBOLS),
        ("symbol_edges", MAX_EDGES),
        ("unresolved_edges", MAX_UNRESOLVED),
        ("ranking_projections", MAX_SYMBOLS),
        ("exact_search_projections", 1),
        ("exact_search_manifests", MAX_FILES),
        ("module_projections", 1),
        ("modules", MAX_MODULES),
        ("module_manifests", MAX_FILES),
        ("module_members", MAX_MEMBERSHIPS),
        ("module_membership_evidence", MAX_MEMBERSHIP_EVIDENCE),
        ("module_central_symbols", MAX_MODULE_FEATURES),
        ("module_entrypoints", MAX_MEMBERSHIPS),
        ("module_tests", MAX_MEMBERSHIPS),
        ("repository_card_entrypoints", 256),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE index_run_id = ?1");
        let count = count_rows(transaction, &sql, run_id.as_bytes().to_vec()).await?;
        if count
            > i64::try_from(limit).map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?
        {
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
        total = total
            .checked_add(
                u64::try_from(count)
                    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            )
            .ok_or(IndexPublicationRepositoryError::ResourceLimit)?;
    }
    Ok(total)
}

async fn rebuild_row_count(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<u64, IndexPublicationRepositoryError> {
    let mut total = 0_u64;
    for table in [
        "repository_card_entrypoints",
        "module_tests",
        "module_entrypoints",
        "module_central_symbols",
        "module_membership_evidence",
        "module_members",
        "module_manifests",
        "modules",
        "module_projections",
        "ranking_projections",
        "unresolved_edges",
        "symbol_edges",
        "symbol_fts",
        "path_fts",
        "card_fts",
        "lexical_search_projections",
        "exact_search_symbols",
        "exact_search_manifests",
        "exact_search_projections",
        "index_parse_diagnostics",
        "index_file_analyses",
        "symbols",
        "file_revisions",
    ] {
        let sql = format!(
            "SELECT COUNT(*) FROM {table} WHERE index_run_id IN (\n\
             SELECT index_run_id FROM index_runs WHERE worktree_id = ?1\n\
             )"
        );
        let count = count_rows(transaction, &sql, worktree_id.as_bytes().to_vec()).await?;
        total = total
            .checked_add(
                u64::try_from(count)
                    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            )
            .ok_or(IndexPublicationRepositoryError::ResourceLimit)?;
    }
    let runs = count_rows(
        transaction,
        "SELECT COUNT(*) FROM index_runs WHERE worktree_id = ?1",
        worktree_id.as_bytes().to_vec(),
    )
    .await?;
    total
        .checked_add(
            u64::try_from(runs).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        )
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)
}

async fn delete_rebuild_rows(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    table: &str,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    loop {
        progress.checkpoint()?;
        let sql = format!(
            "DELETE FROM {table} WHERE rowid IN (\n\
             SELECT rowid FROM {table} WHERE index_run_id IN (\n\
             SELECT index_run_id FROM index_runs WHERE worktree_id = ?1\n\
             ) LIMIT {REBUILD_DELETE_BATCH}\n\
             )"
        );
        let affected = transaction
            .execute(&sql, [worktree_id.as_bytes().to_vec()])
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
        if affected == 0 {
            return Ok(());
        }
        progress.advance(affected)?;
    }
}

async fn delete_index_runs(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    loop {
        progress.checkpoint()?;
        let affected = transaction
            .execute(
                "DELETE FROM index_runs WHERE rowid IN (\n\
                 SELECT rowid FROM index_runs WHERE worktree_id = ?1\n\
                 LIMIT 4096\n\
                 )",
                [worktree_id.as_bytes().to_vec()],
            )
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
        if affected == 0 {
            return Ok(());
        }
        progress.advance(affected)?;
    }
}

pub(crate) struct MutationProgress<'a> {
    control: &'a dyn IndexPersistenceControl,
    started: Instant,
    total: u64,
    completed: u64,
    next_report: u64,
    report_interval: u64,
    work_since_checkpoint: u64,
}

impl<'a> MutationProgress<'a> {
    fn new(
        control: &'a dyn IndexPersistenceControl,
        total: u64,
    ) -> Result<Self, IndexPublicationRepositoryError> {
        if total == 0 {
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
        let report_interval = total.div_ceil(MAX_PROGRESS_EVENTS.saturating_sub(1)).max(1);
        let progress = Self {
            control,
            started: Instant::now(),
            total,
            completed: 0,
            next_report: report_interval,
            report_interval,
            work_since_checkpoint: 0,
        };
        progress.checkpoint()?;
        progress.report(0)?;
        Ok(progress)
    }

    pub(crate) fn advance(&mut self, units: u64) -> Result<(), IndexPublicationRepositoryError> {
        self.completed = self
            .completed
            .checked_add(units)
            .filter(|completed| *completed <= self.total)
            .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
        self.work_since_checkpoint = self.work_since_checkpoint.saturating_add(units);
        if self.work_since_checkpoint >= CANCELLATION_POLL_INTERVAL {
            self.checkpoint()?;
            self.work_since_checkpoint = 0;
        }
        if self.completed >= self.next_report || self.completed == self.total {
            self.checkpoint()?;
            self.report(self.completed)?;
            while self.next_report <= self.completed {
                self.next_report = self.next_report.saturating_add(self.report_interval);
            }
        }
        Ok(())
    }

    pub(crate) fn checkpoint(&self) -> Result<(), IndexPublicationRepositoryError> {
        if self.control.is_cancelled() {
            return Err(IndexPublicationRepositoryError::Cancelled);
        }
        if self.started.elapsed() > MAX_MUTATION_DURATION {
            return Err(IndexPublicationRepositoryError::TimedOut);
        }
        Ok(())
    }

    fn complete_if_empty(&mut self) -> Result<(), IndexPublicationRepositoryError> {
        if self.completed == 0 && self.total == 1 {
            self.advance(1)?;
        }
        Ok(())
    }

    fn report(&self, completed: u64) -> Result<(), IndexPublicationRepositoryError> {
        let progress = Progress::determinate(completed, self.total)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        self.control
            .report_progress(progress)
            .map_err(|_| IndexPublicationRepositoryError::ProgressUnavailable)
    }
}

fn validate_resource_limits(
    publication: &IndexPublication,
) -> Result<(), IndexPublicationRepositoryError> {
    let graph = publication.graph();
    if graph.files().len() > MAX_FILES
        || publication.file_analyses().len() > MAX_FILES
        || publication
            .file_analyses()
            .iter()
            .map(|analysis| analysis.diagnostics().len())
            .try_fold(0_usize, usize::checked_add)
            .is_none_or(|total| total > MAX_DIAGNOSTICS)
        || graph.symbols().len() > MAX_SYMBOLS
        || graph.edges().len() > MAX_EDGES
        || graph.unresolved().len() > MAX_UNRESOLVED
        || publication.modules().modules().len() > MAX_MODULES
        || publication.modules().memberships().len() > MAX_MEMBERSHIPS
        || publication
            .modules()
            .memberships()
            .iter()
            .try_fold(0usize, |total, membership| {
                total.checked_add(membership.evidence().relationships().len())
            })
            .is_none_or(|total| total > MAX_MEMBERSHIP_EVIDENCE)
    {
        return Err(IndexPublicationRepositoryError::ResourceLimit);
    }
    Ok(())
}

async fn publication_rows_exist(
    transaction: &Transaction,
    run_id: IndexRunId,
) -> Result<bool, IndexPublicationRepositoryError> {
    for table in [
        "file_revisions",
        "index_file_analyses",
        "index_parse_diagnostics",
        "symbols",
        "symbol_edges",
        "unresolved_edges",
        "ranking_projections",
        "module_projections",
        "modules",
        "module_manifests",
        "module_members",
        "module_membership_evidence",
        "module_central_symbols",
        "module_entrypoints",
        "module_tests",
        "repository_card_entrypoints",
        "lexical_search_projections",
        "symbol_fts",
        "path_fts",
        "card_fts",
        "exact_search_projections",
        "exact_search_symbols",
        "exact_search_manifests",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE index_run_id = ?1");
        if count_rows(transaction, &sql, run_id.as_bytes().to_vec()).await? != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn effective_files_at_snapshot(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
) -> Result<Vec<FileRevision>, IndexPublicationRepositoryError> {
    let generation = snapshot_generation(transaction, worktree_id, snapshot_id).await?;
    let mut rows = transaction
        .query(
            "WITH ranked_changes AS (\n\
             SELECT changes.repository_path, changes.change_kind, changes.content_hash,\n\
                    ROW_NUMBER() OVER (\n\
                      PARTITION BY changes.repository_path\n\
                      ORDER BY snapshots.generation DESC\n\
                    ) AS path_rank\n\
             FROM snapshot_changes AS changes\n\
             JOIN snapshots ON snapshots.snapshot_id = changes.snapshot_id\n\
             WHERE snapshots.worktree_id = ?1 AND snapshots.generation <= ?2\n\
             )\n\
             SELECT repository_path, content_hash FROM ranked_changes\n\
             WHERE path_rank = 1 AND change_kind = 'upsert'\n\
             ORDER BY repository_path",
            params![worktree_id.as_bytes().to_vec(), generation],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut files = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let path: Vec<u8> = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
        let hash = read_stable_id(&row, 1)?;
        files.push(FileRevision::new(
            RepositoryPath::try_from_bytes(path)
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            a3_domain::ContentHash::from_bytes(hash),
        ));
    }
    Ok(files)
}

async fn write_file_delta_projection(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run: IndexRunRecord,
) -> Result<(), IndexPublicationRepositoryError> {
    let generation = snapshot_generation(transaction, worktree_id, run.snapshot_id()).await?;
    transaction
        .execute(
            "INSERT INTO file_revisions (index_run_id, repository_path, content_hash)\n\
             WITH ranked_changes AS (\n\
             SELECT changes.repository_path, changes.change_kind, changes.content_hash,\n\
                    ROW_NUMBER() OVER (\n\
                      PARTITION BY changes.repository_path\n\
                      ORDER BY snapshots.generation DESC\n\
                    ) AS path_rank\n\
             FROM snapshot_changes AS changes\n\
             JOIN snapshots ON snapshots.snapshot_id = changes.snapshot_id\n\
             WHERE snapshots.worktree_id = ?1 AND snapshots.generation <= ?2\n\
             )\n\
             SELECT ?3, repository_path, content_hash FROM ranked_changes\n\
             WHERE path_rank = 1 AND change_kind = 'upsert'\n\
             ORDER BY repository_path",
            params![
                worktree_id.as_bytes().to_vec(),
                generation,
                run.id().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn snapshot_generation(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
) -> Result<i64, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT generation FROM snapshots WHERE snapshot_id = ?1 AND worktree_id = ?2",
            params![
                snapshot_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    else {
        return Err(IndexPublicationRepositoryError::PublicationMismatch);
    };
    let generation: i64 = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
    if generation <= 0
        || rows
            .next()
            .await
            .map_err(IndexPublicationRepositoryError::Read)?
            .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(generation)
}

async fn read_index_run(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
) -> Result<Option<IndexRunRecord>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id, ranking_policy_version, run_sequence, status\n\
             FROM index_runs WHERE index_run_id = ?1 AND worktree_id = ?2",
            params![run_id.as_bytes().to_vec(), worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let record = index_run_from_row(&row)?;
    if rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(Some(record))
}

async fn read_latest_published_run(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<IndexRunRecord>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id, ranking_policy_version, run_sequence, status\n\
             FROM index_runs WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let record = index_run_from_row(&row)?;
    if rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(Some(record))
}

fn index_run_from_row(
    row: &libsql::Row,
) -> Result<IndexRunRecord, IndexPublicationRepositoryError> {
    let id = IndexRunId::from_bytes(read_stable_id(row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_stable_id(row, 1)?);
    let policy: i64 = row.get(2).map_err(IndexPublicationRepositoryError::Read)?;
    let sequence: i64 = row.get(3).map_err(IndexPublicationRepositoryError::Read)?;
    let status: String = row.get(4).map_err(IndexPublicationRepositoryError::Read)?;
    let policy = u32::try_from(policy)
        .ok()
        .and_then(|value| RankingPolicyVersion::new(value).ok())
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let sequence = u64::try_from(sequence)
        .ok()
        .and_then(|value| IndexRunSequence::new(value).ok())
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let status = IndexRunStatus::try_from_stored(&status)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    Ok(IndexRunRecord::new(
        id,
        snapshot_id,
        policy,
        sequence,
        status,
    ))
}

async fn count_rows(
    transaction: &Transaction,
    sql: &str,
    parameter: Vec<u8>,
) -> Result<i64, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(sql, [parameter])
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
    if count < 0
        || rows
            .next()
            .await
            .map_err(IndexPublicationRepositoryError::Read)?
            .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(count)
}

pub(crate) fn read_stable_id(
    row: &libsql::Row,
    index: i32,
) -> Result<[u8; 32], IndexPublicationRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

async fn rollback<T>(
    transaction: Transaction,
    error: IndexPublicationRepositoryError,
) -> Result<T, IndexPublicationRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(IndexPublicationRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum IndexPublicationRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    IndexRunAlreadyActive,
    IndexRunNotFound,
    InvalidIndexRunTransition,
    PublicationMismatch,
    ResourceLimit,
    Cancelled,
    TimedOut,
    ProgressUnavailable,
    InvalidStoredData,
}

impl IndexPublicationRepositoryError {
    pub(crate) const fn classify(&self) -> KnowledgeIndexFailure {
        match self {
            Self::IndexRunAlreadyActive => KnowledgeIndexFailure::IndexRunAlreadyActive,
            Self::IndexRunNotFound => KnowledgeIndexFailure::IndexRunNotFound,
            Self::InvalidIndexRunTransition => KnowledgeIndexFailure::InvalidIndexRunTransition,
            Self::PublicationMismatch => KnowledgeIndexFailure::IndexPublicationMismatch,
            Self::ResourceLimit => KnowledgeIndexFailure::IndexPublicationTooLarge,
            Self::Cancelled => KnowledgeIndexFailure::Cancelled,
            Self::TimedOut => KnowledgeIndexFailure::TimedOut,
            Self::ProgressUnavailable => KnowledgeIndexFailure::ProgressUnavailable,
            Self::InvalidStoredData => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::InvalidStoredData)
            }
            Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Commit(_)
            | Self::Rollback(_) => {
                KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::Unavailable)
            }
        }
    }
}

impl fmt::Display for IndexPublicationRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Begin(_) => "could not begin index publication transaction",
            Self::Read(_) => "could not read index publication state",
            Self::Write(_) => "could not write index publication state",
            Self::Commit(_) => "could not commit index publication transaction",
            Self::Rollback(_) => "could not roll back index publication transaction",
            Self::IndexRunAlreadyActive => "an index run is active during rebuild",
            Self::IndexRunNotFound => "index run was not found",
            Self::InvalidIndexRunTransition => "index run transition is invalid",
            Self::PublicationMismatch => "index publication does not match its run",
            Self::ResourceLimit => "index publication exceeds a fixed storage limit",
            Self::Cancelled => "index persistence was cancelled",
            Self::TimedOut => "index persistence timed out",
            Self::ProgressUnavailable => "index persistence progress is unavailable",
            Self::InvalidStoredData => "stored index publication is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for IndexPublicationRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::IndexRunAlreadyActive
            | Self::IndexRunNotFound
            | Self::InvalidIndexRunTransition
            | Self::PublicationMismatch
            | Self::ResourceLimit
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable
            | Self::InvalidStoredData => None,
        }
    }
}
