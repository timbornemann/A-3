use a3_application::{
    INDEX_TRACE_EVENT_LIMIT, INDEX_TRACE_FILE_PAGE_LIMIT, IndexTraceCheckpoint, IndexTraceCounts,
    IndexTraceEvent, IndexTraceEventKind, IndexTraceFileFilter, IndexTraceFilePage,
    IndexTraceFileQuery, IndexTraceFileRecord, IndexTracePhaseProgress, IndexTraceRunSummary,
    IndexTraceSnapshot, IndexTraceStoreFailure, RetainedIndexTraces,
};
use a3_domain::{
    IndexTraceDiagnosticCode, IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId,
    IndexTraceParseOutcome, IndexTracePhase, IndexTracePhaseState, IndexTraceRevision,
    IndexTraceState, IndexTraceTimestamp, IndexTraceTrigger, RepositoryPath, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, params};

pub(crate) async fn reconcile_interrupted(
    connection: &Connection,
    worktree_id: WorktreeId,
    at: IndexTraceTimestamp,
) -> Result<(), IndexTraceStoreFailure> {
    let transaction = begin(connection).await?;
    transaction
        .execute(
            "INSERT INTO index_trace_events (
             worktree_id, trace_id, revision, occurred_at_unix_millis, event_kind,
             phase, repository_path, diagnostic_code)
             SELECT worktree_id, trace_id, revision + 1, ?2, 'interrupted', current_phase,
                    current_path, 'interrupted'
             FROM index_trace_runs WHERE worktree_id = ?1
               AND state IN ('queued', 'running', 'cancelling')",
            params![bytes(worktree_id.as_bytes()), at.unix_millis()],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    transaction
        .execute(
            "UPDATE index_trace_phases SET state = 'cancelled', ended_at_unix_millis = ?2
             WHERE worktree_id = ?1 AND state = 'running'",
            params![bytes(worktree_id.as_bytes()), at.unix_millis()],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    transaction
        .execute(
            "UPDATE index_trace_runs SET revision = revision + 1, state = 'interrupted',
             ended_at_unix_millis = ?2, last_activity_at_unix_millis = ?2,
             diagnostic_code = 'interrupted', details_incomplete = 1
             WHERE worktree_id = ?1 AND state IN ('queued', 'running', 'cancelling')",
            params![bytes(worktree_id.as_bytes()), at.unix_millis()],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    transaction
        .execute(
            "UPDATE index_runs SET status = 'failed'
             WHERE worktree_id = ?1 AND status = 'building'",
            [bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    retain_latest_terminal(&transaction, worktree_id, None).await?;
    commit(transaction).await
}

pub(crate) async fn create_trace(
    connection: &Connection,
    worktree_id: WorktreeId,
    snapshot: &IndexTraceSnapshot,
) -> Result<(), IndexTraceStoreFailure> {
    if snapshot.summary().state() != IndexTraceState::Queued
        || snapshot.summary().revision() != IndexTraceRevision::FIRST
        || snapshot.phases().len() != 6
    {
        return Err(IndexTraceStoreFailure::Conflict);
    }
    let transaction = begin(connection).await?;
    let active = scalar_count(
        &transaction,
        "SELECT COUNT(*) FROM index_trace_runs WHERE worktree_id = ?1
         AND state IN ('queued', 'running', 'cancelling')",
        worktree_id,
    )
    .await?;
    if active != 0 {
        return rollback(transaction, IndexTraceStoreFailure::Conflict).await;
    }
    retain_latest_terminal(&transaction, worktree_id, None).await?;
    insert_run(&transaction, worktree_id, snapshot.summary()).await?;
    replace_phases(
        &transaction,
        worktree_id,
        snapshot.summary().id(),
        snapshot.phases(),
    )
    .await?;
    for event in snapshot.events() {
        insert_event(&transaction, worktree_id, snapshot.summary().id(), event).await?;
    }
    for file in snapshot.files() {
        upsert_file(&transaction, worktree_id, snapshot.summary().id(), file).await?;
    }
    commit(transaction).await
}

pub(crate) async fn checkpoint_trace(
    connection: &Connection,
    worktree_id: WorktreeId,
    checkpoint: &IndexTraceCheckpoint,
) -> Result<(), IndexTraceStoreFailure> {
    let transaction = begin(connection).await?;
    let summary = checkpoint.snapshot.summary();
    let affected = update_run(
        &transaction,
        worktree_id,
        checkpoint.expected_revision,
        summary,
    )
    .await?;
    if affected != 1 {
        return rollback(transaction, IndexTraceStoreFailure::StaleRevision).await;
    }
    replace_phases(
        &transaction,
        worktree_id,
        summary.id(),
        checkpoint.snapshot.phases(),
    )
    .await?;
    for file in &checkpoint.file_updates {
        upsert_file(&transaction, worktree_id, summary.id(), file).await?;
    }
    for event in &checkpoint.new_events {
        insert_event(&transaction, worktree_id, summary.id(), event).await?;
    }
    transaction
        .execute(
            "DELETE FROM index_trace_events WHERE worktree_id = ?1 AND trace_id = ?2
             AND revision NOT IN (
               SELECT revision FROM index_trace_events WHERE worktree_id = ?1 AND trace_id = ?2
               ORDER BY revision DESC LIMIT ?3)",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(summary.id().as_bytes()),
                i64::try_from(INDEX_TRACE_EVENT_LIMIT)
                    .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?,
            ],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    if summary.state().is_terminal() {
        retain_latest_terminal(&transaction, worktree_id, Some(summary.id())).await?;
    }
    commit(transaction).await
}

pub(crate) async fn load_retained(
    connection: &Connection,
    worktree_id: WorktreeId,
) -> Result<RetainedIndexTraces, IndexTraceStoreFailure> {
    let mut rows = connection
        .query(
            "SELECT trace_id FROM index_trace_runs WHERE worktree_id = ?1
             ORDER BY CASE WHEN state IN ('queued', 'running', 'cancelling') THEN 0 ELSE 1 END,
                      started_at_unix_millis DESC LIMIT 2",
            [bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let mut ids = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
    {
        ids.push(id_from_blob(
            row.get::<Vec<u8>>(0)
                .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?,
        )?);
    }
    let first = match ids.first().copied() {
        Some(id) => Some(load_trace(connection, worktree_id, id).await?),
        None => None,
    };
    let first_active = first
        .as_ref()
        .is_some_and(|trace| !trace.summary().state().is_terminal());
    let previous = if first_active {
        match ids.get(1).copied() {
            Some(id) => Some(load_trace(connection, worktree_id, id).await?),
            None => None,
        }
    } else {
        None
    };
    Ok(RetainedIndexTraces {
        current: first,
        previous,
    })
}

pub(crate) async fn query_files(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &IndexTraceFileQuery,
) -> Result<IndexTraceFilePage, IndexTraceStoreFailure> {
    let stored_revision = load_revision(connection, worktree_id, query.trace_id).await?;
    if stored_revision != query.revision {
        return Err(IndexTraceStoreFailure::StaleRevision);
    }
    let search = match query.search.as_deref() {
        Some(value) => value.to_lowercase(),
        None => String::new(),
    };
    let filter = encode_filter(query.filter);
    let total = query_file_count(connection, worktree_id, query, &search, filter).await?;
    let mut rows = connection
        .query(
            "SELECT repository_path, change_kind, hash_outcome, parse_outcome,
                    diagnostic_codes, diagnostics_truncated
             FROM index_trace_files WHERE worktree_id = ?1 AND trace_id = ?2
               AND (?3 = '' OR instr(path_search, ?3) > 0)
               AND (?4 = 'all'
                 OR (?4 = 'new' AND change_kind = 'new')
                 OR (?4 = 'changed' AND change_kind = 'changed')
                 OR (?4 = 'unchanged' AND change_kind = 'unchanged')
                 OR (?4 = 'deleted' AND change_kind = 'deleted')
                 OR (?4 = 'hashed' AND hash_outcome = 'hashed')
                 OR (?4 = 'hash_reused' AND hash_outcome = 'reused')
                 OR (?4 = 'structural' AND parse_outcome = 'structural')
                 OR (?4 = 'parse_reused' AND parse_outcome = 'reused')
                 OR (?4 = 'generic' AND parse_outcome = 'generic')
                 OR (?4 = 'failed' AND parse_outcome = 'failed'))
             ORDER BY repository_path LIMIT ?5 OFFSET ?6",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(query.trace_id.as_bytes()),
                search,
                filter,
                i64::from(INDEX_TRACE_FILE_PAGE_LIMIT),
                to_i64(query.offset)?,
            ],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let mut files = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
    {
        files.push(decode_file(&row)?);
    }
    let returned =
        u64::try_from(files.len()).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?;
    let has_more = query
        .offset
        .checked_add(returned)
        .is_some_and(|end| end < total);
    Ok(IndexTraceFilePage {
        files,
        total,
        offset: query.offset,
        has_more,
    })
}

async fn load_trace(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<IndexTraceSnapshot, IndexTraceStoreFailure> {
    let summary = load_summary(connection, worktree_id, trace_id).await?;
    let phases = load_phases(connection, worktree_id, trace_id).await?;
    let events = load_events(connection, worktree_id, trace_id).await?;
    let files = load_files(connection, worktree_id, trace_id).await?;
    IndexTraceSnapshot::new(summary, phases, events, files)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}

async fn insert_run(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    summary: &IndexTraceRunSummary,
) -> Result<(), IndexTraceStoreFailure> {
    let counts = summary.counts();
    transaction.execute(
        "INSERT INTO index_trace_runs (
         worktree_id, trace_id, revision, state, trigger_kind, started_at_unix_millis,
         ended_at_unix_millis, last_activity_at_unix_millis, current_phase, current_path,
         diagnostic_code, details_incomplete, previous_publication_available,
         discovered_count, pending_count, new_count, changed_count, unchanged_count, deleted_count,
         hashed_count, hash_reused_count, structural_count, parse_reused_count, generic_count, failed_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                 ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        params![
            bytes(worktree_id.as_bytes()), bytes(summary.id().as_bytes()), to_i64(summary.revision().get())?,
            encode_state(summary.state()), encode_trigger(summary.trigger()), summary.started_at().unix_millis(),
            summary.ended_at().map(IndexTraceTimestamp::unix_millis), summary.last_activity_at().unix_millis(),
            summary.current_phase().map(encode_phase), summary.current_file().map(|path| path.as_bytes().to_vec()),
            summary.diagnostic().map(encode_diagnostic), i64::from(summary.details_incomplete()),
            i64::from(summary.previous_publication_available()), to_i64(counts.discovered)?, to_i64(counts.pending)?,
            to_i64(counts.new_files)?, to_i64(counts.changed)?, to_i64(counts.unchanged)?, to_i64(counts.deleted)?, to_i64(counts.hashed)?,
            to_i64(counts.hash_reused)?, to_i64(counts.structural)?, to_i64(counts.parse_reused)?,
            to_i64(counts.generic)?, to_i64(counts.failed)?,
        ],
    ).await.map_err(|_| IndexTraceStoreFailure::Conflict)?;
    Ok(())
}

async fn update_run(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    expected: IndexTraceRevision,
    summary: &IndexTraceRunSummary,
) -> Result<u64, IndexTraceStoreFailure> {
    let counts = summary.counts();
    transaction
        .execute(
            "UPDATE index_trace_runs SET revision = ?3, state = ?4, trigger_kind = ?5,
         started_at_unix_millis = ?6, ended_at_unix_millis = ?7,
         last_activity_at_unix_millis = ?8, current_phase = ?9, current_path = ?10,
         diagnostic_code = ?11, details_incomplete = ?12, previous_publication_available = ?13,
         discovered_count = ?14, pending_count = ?15, new_count = ?16, changed_count = ?17,
         unchanged_count = ?18, deleted_count = ?19, hashed_count = ?20,
         hash_reused_count = ?21, structural_count = ?22, parse_reused_count = ?23,
         generic_count = ?24, failed_count = ?25
         WHERE worktree_id = ?1 AND trace_id = ?2 AND revision = ?26",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(summary.id().as_bytes()),
                to_i64(summary.revision().get())?,
                encode_state(summary.state()),
                encode_trigger(summary.trigger()),
                summary.started_at().unix_millis(),
                summary.ended_at().map(IndexTraceTimestamp::unix_millis),
                summary.last_activity_at().unix_millis(),
                summary.current_phase().map(encode_phase),
                summary.current_file().map(|path| path.as_bytes().to_vec()),
                summary.diagnostic().map(encode_diagnostic),
                i64::from(summary.details_incomplete()),
                i64::from(summary.previous_publication_available()),
                to_i64(counts.discovered)?,
                to_i64(counts.pending)?,
                to_i64(counts.new_files)?,
                to_i64(counts.changed)?,
                to_i64(counts.unchanged)?,
                to_i64(counts.deleted)?,
                to_i64(counts.hashed)?,
                to_i64(counts.hash_reused)?,
                to_i64(counts.structural)?,
                to_i64(counts.parse_reused)?,
                to_i64(counts.generic)?,
                to_i64(counts.failed)?,
                to_i64(expected.get())?,
            ],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)
}

async fn replace_phases(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    phases: &[IndexTracePhaseProgress],
) -> Result<(), IndexTraceStoreFailure> {
    transaction
        .execute(
            "DELETE FROM index_trace_phases WHERE worktree_id = ?1 AND trace_id = ?2",
            params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    for phase in phases {
        transaction.execute(
            "INSERT INTO index_trace_phases (worktree_id, trace_id, phase_ordinal, phase, state,
             started_at_unix_millis, ended_at_unix_millis, completed_count, total_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes()), i64::from(phase.phase().ordinal()),
                encode_phase(phase.phase()), encode_phase_state(phase.state()),
                phase.started_at().map(IndexTraceTimestamp::unix_millis), phase.ended_at().map(IndexTraceTimestamp::unix_millis),
                phase.completed().map(to_i64).transpose()?, phase.total().map(to_i64).transpose()?],
        ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    }
    Ok(())
}

async fn insert_event(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    event: &IndexTraceEvent,
) -> Result<(), IndexTraceStoreFailure> {
    transaction.execute(
        "INSERT INTO index_trace_events (worktree_id, trace_id, revision, occurred_at_unix_millis,
         event_kind, phase, repository_path, diagnostic_code) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes()), to_i64(event.revision().get())?,
            event.occurred_at().unix_millis(), encode_event(event.kind()), event.phase().map(encode_phase),
            event.path().map(|path| path.as_bytes().to_vec()), event.diagnostic().map(encode_diagnostic)],
    ).await.map_err(|_| IndexTraceStoreFailure::Conflict)?;
    Ok(())
}

async fn upsert_file(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
    file: &IndexTraceFileRecord,
) -> Result<(), IndexTraceStoreFailure> {
    transaction
        .execute(
            "INSERT INTO index_trace_files (worktree_id, trace_id, repository_path, path_search,
         change_kind, hash_outcome, parse_outcome, diagnostic_codes, diagnostics_truncated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(worktree_id, trace_id, repository_path) DO UPDATE SET
           path_search = excluded.path_search, change_kind = excluded.change_kind,
           hash_outcome = excluded.hash_outcome, parse_outcome = excluded.parse_outcome,
           diagnostic_codes = excluded.diagnostic_codes,
           diagnostics_truncated = excluded.diagnostics_truncated",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(trace_id.as_bytes()),
                file.path().as_bytes().to_vec(),
                path_search(file.path()),
                encode_change(file.change()),
                encode_hash(file.hash()),
                file.parse().map(encode_parse),
                encode_diagnostics(file.diagnostics()),
                i64::from(file.diagnostics_truncated())
            ],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    Ok(())
}

async fn load_summary(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<IndexTraceRunSummary, IndexTraceStoreFailure> {
    let mut rows = connection
        .query(
            "SELECT revision, state, trigger_kind, started_at_unix_millis, ended_at_unix_millis,
         last_activity_at_unix_millis, current_phase, current_path, diagnostic_code,
         details_incomplete, previous_publication_available, discovered_count, pending_count,
         new_count, changed_count, unchanged_count, deleted_count, hashed_count, hash_reused_count,
         structural_count, parse_reused_count, generic_count, failed_count
         FROM index_trace_runs WHERE worktree_id = ?1 AND trace_id = ?2",
            params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
        .ok_or(IndexTraceStoreFailure::NotFound)?;
    let counts = IndexTraceCounts {
        discovered: nonnegative(&row, 11)?,
        pending: nonnegative(&row, 12)?,
        new_files: nonnegative(&row, 13)?,
        changed: nonnegative(&row, 14)?,
        unchanged: nonnegative(&row, 15)?,
        deleted: nonnegative(&row, 16)?,
        hashed: nonnegative(&row, 17)?,
        hash_reused: nonnegative(&row, 18)?,
        structural: nonnegative(&row, 19)?,
        parse_reused: nonnegative(&row, 20)?,
        generic: nonnegative(&row, 21)?,
        failed: nonnegative(&row, 22)?,
    };
    IndexTraceRunSummary::restored(
        trace_id,
        revision(&row, 0)?,
        decode_state(text(&row, 1)?)?,
        decode_trigger(text(&row, 2)?)?,
        timestamp(required_i64(&row, 3)?)?,
        optional_timestamp(&row, 4)?,
        timestamp(required_i64(&row, 5)?)?,
        optional_text(&row, 6)?.map(decode_phase).transpose()?,
        optional_path(&row, 7)?,
        optional_text(&row, 8)?.map(decode_diagnostic).transpose()?,
        boolean(&row, 9)?,
        boolean(&row, 10)?,
        counts,
    )
    .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}

async fn load_phases(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<Vec<IndexTracePhaseProgress>, IndexTraceStoreFailure> {
    let mut rows = connection.query(
        "SELECT phase, state, started_at_unix_millis, ended_at_unix_millis, completed_count, total_count
         FROM index_trace_phases WHERE worktree_id = ?1 AND trace_id = ?2 ORDER BY phase_ordinal",
        params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
    ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
    {
        result.push(
            IndexTracePhaseProgress::new(
                decode_phase(text(&row, 0)?)?,
                decode_phase_state(text(&row, 1)?)?,
                optional_timestamp(&row, 2)?,
                optional_timestamp(&row, 3)?,
                optional_u64(&row, 4)?,
                optional_u64(&row, 5)?,
            )
            .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?,
        );
    }
    Ok(result)
}

async fn load_events(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<Vec<IndexTraceEvent>, IndexTraceStoreFailure> {
    let mut rows = connection.query(
        "SELECT revision, occurred_at_unix_millis, event_kind, phase, repository_path, diagnostic_code
         FROM index_trace_events WHERE worktree_id = ?1 AND trace_id = ?2 ORDER BY revision",
        params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
    ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
    {
        result.push(IndexTraceEvent::new(
            revision(&row, 0)?,
            timestamp(required_i64(&row, 1)?)?,
            decode_event(text(&row, 2)?)?,
            optional_text(&row, 3)?.map(decode_phase).transpose()?,
            optional_path(&row, 4)?,
            optional_text(&row, 5)?.map(decode_diagnostic).transpose()?,
        ));
    }
    Ok(result)
}

async fn load_files(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<Vec<IndexTraceFileRecord>, IndexTraceStoreFailure> {
    let mut rows = connection.query(
        "SELECT repository_path, change_kind, hash_outcome, parse_outcome, diagnostic_codes, diagnostics_truncated
         FROM index_trace_files WHERE worktree_id = ?1 AND trace_id = ?2 ORDER BY repository_path",
        params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
    ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
    {
        result.push(decode_file(&row)?);
    }
    Ok(result)
}

fn decode_file(row: &Row) -> Result<IndexTraceFileRecord, IndexTraceStoreFailure> {
    IndexTraceFileRecord::new(
        path(
            row.get::<Vec<u8>>(0)
                .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?,
        )?,
        decode_change(text(row, 1)?)?,
        decode_hash(text(row, 2)?)?,
        optional_text(row, 3)?.map(decode_parse).transpose()?,
        decode_diagnostics(text(row, 4)?)?,
        boolean(row, 5)?,
    )
    .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}

async fn retain_latest_terminal(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    keep: Option<IndexTraceId>,
) -> Result<(), IndexTraceStoreFailure> {
    transaction.execute(
        "DELETE FROM index_trace_runs WHERE worktree_id = ?1
         AND state IN ('succeeded', 'failed', 'cancelled', 'interrupted')
         AND trace_id NOT IN (
           SELECT trace_id FROM index_trace_runs WHERE worktree_id = ?1
             AND state IN ('succeeded', 'failed', 'cancelled', 'interrupted')
           ORDER BY CASE WHEN trace_id = ?2 THEN 0 ELSE 1 END, started_at_unix_millis DESC LIMIT 1)",
        params![bytes(worktree_id.as_bytes()), keep.map(|id| bytes(id.as_bytes()))],
    ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    Ok(())
}

async fn load_revision(
    connection: &Connection,
    worktree_id: WorktreeId,
    trace_id: IndexTraceId,
) -> Result<IndexTraceRevision, IndexTraceStoreFailure> {
    let mut rows = connection
        .query(
            "SELECT revision FROM index_trace_runs WHERE worktree_id = ?1 AND trace_id = ?2",
            params![bytes(worktree_id.as_bytes()), bytes(trace_id.as_bytes())],
        )
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
        .ok_or(IndexTraceStoreFailure::NotFound)?;
    revision(&row, 0)
}

async fn query_file_count(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &IndexTraceFileQuery,
    search: &str,
    filter: &str,
) -> Result<u64, IndexTraceStoreFailure> {
    let mut rows = connection.query(
        "SELECT COUNT(*) FROM index_trace_files WHERE worktree_id = ?1 AND trace_id = ?2
         AND (?3 = '' OR instr(path_search, ?3) > 0)
         AND (?4 = 'all' OR (?4 = 'new' AND change_kind = 'new') OR (?4 = 'changed' AND change_kind = 'changed')
           OR (?4 = 'unchanged' AND change_kind = 'unchanged') OR (?4 = 'deleted' AND change_kind = 'deleted')
           OR (?4 = 'hashed' AND hash_outcome = 'hashed') OR (?4 = 'hash_reused' AND hash_outcome = 'reused')
           OR (?4 = 'structural' AND parse_outcome = 'structural') OR (?4 = 'parse_reused' AND parse_outcome = 'reused')
           OR (?4 = 'generic' AND parse_outcome = 'generic') OR (?4 = 'failed' AND parse_outcome = 'failed'))",
        params![bytes(worktree_id.as_bytes()), bytes(query.trace_id.as_bytes()), search, filter],
    ).await.map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
        .ok_or(IndexTraceStoreFailure::InvalidStoredData)?;
    nonnegative(&row, 0)
}

async fn scalar_count(
    transaction: &Transaction,
    sql: &str,
    worktree_id: WorktreeId,
) -> Result<u64, IndexTraceStoreFailure> {
    let mut rows = transaction
        .query(sql, [bytes(worktree_id.as_bytes())])
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)?
        .ok_or(IndexTraceStoreFailure::InvalidStoredData)?;
    nonnegative(&row, 0)
}

async fn begin(connection: &Connection) -> Result<Transaction, IndexTraceStoreFailure> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)
}
async fn commit(transaction: Transaction) -> Result<(), IndexTraceStoreFailure> {
    transaction
        .commit()
        .await
        .map_err(|_| IndexTraceStoreFailure::Unavailable)
}
async fn rollback<T>(transaction: Transaction, error: T) -> Result<(), T> {
    let _ = transaction.rollback().await;
    Err(error)
}

fn bytes(value: &[u8; 32]) -> Vec<u8> {
    value.to_vec()
}
fn to_i64(value: u64) -> Result<i64, IndexTraceStoreFailure> {
    i64::try_from(value).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn required_i64(row: &Row, index: i32) -> Result<i64, IndexTraceStoreFailure> {
    row.get(index)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn nonnegative(row: &Row, index: i32) -> Result<u64, IndexTraceStoreFailure> {
    u64::try_from(required_i64(row, index)?).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn optional_u64(row: &Row, index: i32) -> Result<Option<u64>, IndexTraceStoreFailure> {
    row.get::<Option<i64>>(index)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?
        .map(|value| u64::try_from(value).map_err(|_| IndexTraceStoreFailure::InvalidStoredData))
        .transpose()
}
fn text(row: &Row, index: i32) -> Result<&str, IndexTraceStoreFailure> {
    row.get_str(index)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn optional_text(row: &Row, index: i32) -> Result<Option<&str>, IndexTraceStoreFailure> {
    match row.get_str(index) {
        Ok(value) => Ok(Some(value)),
        Err(_) if row.get::<Option<String>>(index).ok().flatten().is_none() => Ok(None),
        Err(_) => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn boolean(row: &Row, index: i32) -> Result<bool, IndexTraceStoreFailure> {
    match required_i64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn timestamp(value: i64) -> Result<IndexTraceTimestamp, IndexTraceStoreFailure> {
    IndexTraceTimestamp::new(value).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn optional_timestamp(
    row: &Row,
    index: i32,
) -> Result<Option<IndexTraceTimestamp>, IndexTraceStoreFailure> {
    row.get::<Option<i64>>(index)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?
        .map(timestamp)
        .transpose()
}
fn revision(row: &Row, index: i32) -> Result<IndexTraceRevision, IndexTraceStoreFailure> {
    IndexTraceRevision::new(nonnegative(row, index)?)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn path(value: Vec<u8>) -> Result<RepositoryPath, IndexTraceStoreFailure> {
    RepositoryPath::try_from_bytes(value).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)
}
fn optional_path(row: &Row, index: i32) -> Result<Option<RepositoryPath>, IndexTraceStoreFailure> {
    row.get::<Option<Vec<u8>>>(index)
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?
        .map(path)
        .transpose()
}
fn id_from_blob(value: Vec<u8>) -> Result<IndexTraceId, IndexTraceStoreFailure> {
    let bytes: [u8; 32] = value
        .try_into()
        .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?;
    Ok(IndexTraceId::from_bytes(bytes))
}

fn path_search(path: &RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .take(512)
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect::<String>()
        .to_lowercase()
}
fn encode_state(value: IndexTraceState) -> &'static str {
    match value {
        IndexTraceState::Queued => "queued",
        IndexTraceState::Running => "running",
        IndexTraceState::Cancelling => "cancelling",
        IndexTraceState::Succeeded => "succeeded",
        IndexTraceState::Failed => "failed",
        IndexTraceState::Cancelled => "cancelled",
        IndexTraceState::Interrupted => "interrupted",
    }
}
fn decode_state(value: &str) -> Result<IndexTraceState, IndexTraceStoreFailure> {
    match value {
        "queued" => Ok(IndexTraceState::Queued),
        "running" => Ok(IndexTraceState::Running),
        "cancelling" => Ok(IndexTraceState::Cancelling),
        "succeeded" => Ok(IndexTraceState::Succeeded),
        "failed" => Ok(IndexTraceState::Failed),
        "cancelled" => Ok(IndexTraceState::Cancelled),
        "interrupted" => Ok(IndexTraceState::Interrupted),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_trigger(value: IndexTraceTrigger) -> &'static str {
    match value {
        IndexTraceTrigger::InitialObservation => "initial_observation",
        IndexTraceTrigger::FileChanges => "file_changes",
        IndexTraceTrigger::RecoveryRescan => "recovery_rescan",
        IndexTraceTrigger::ManualRetry => "manual_retry",
    }
}
fn decode_trigger(value: &str) -> Result<IndexTraceTrigger, IndexTraceStoreFailure> {
    match value {
        "initial_observation" => Ok(IndexTraceTrigger::InitialObservation),
        "file_changes" => Ok(IndexTraceTrigger::FileChanges),
        "recovery_rescan" => Ok(IndexTraceTrigger::RecoveryRescan),
        "manual_retry" => Ok(IndexTraceTrigger::ManualRetry),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_phase(value: IndexTracePhase) -> &'static str {
    match value {
        IndexTracePhase::Discover => "discover",
        IndexTracePhase::Hash => "hash",
        IndexTracePhase::Parse => "parse",
        IndexTracePhase::Link => "link",
        IndexTracePhase::Rank => "rank",
        IndexTracePhase::Publish => "publish",
    }
}
fn decode_phase(value: &str) -> Result<IndexTracePhase, IndexTraceStoreFailure> {
    match value {
        "discover" => Ok(IndexTracePhase::Discover),
        "hash" => Ok(IndexTracePhase::Hash),
        "parse" => Ok(IndexTracePhase::Parse),
        "link" => Ok(IndexTracePhase::Link),
        "rank" => Ok(IndexTracePhase::Rank),
        "publish" => Ok(IndexTracePhase::Publish),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_phase_state(value: IndexTracePhaseState) -> &'static str {
    match value {
        IndexTracePhaseState::Pending => "pending",
        IndexTracePhaseState::Running => "running",
        IndexTracePhaseState::Succeeded => "succeeded",
        IndexTracePhaseState::Failed => "failed",
        IndexTracePhaseState::Cancelled => "cancelled",
    }
}
fn decode_phase_state(value: &str) -> Result<IndexTracePhaseState, IndexTraceStoreFailure> {
    match value {
        "pending" => Ok(IndexTracePhaseState::Pending),
        "running" => Ok(IndexTracePhaseState::Running),
        "succeeded" => Ok(IndexTracePhaseState::Succeeded),
        "failed" => Ok(IndexTracePhaseState::Failed),
        "cancelled" => Ok(IndexTracePhaseState::Cancelled),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_change(value: IndexTraceFileChange) -> &'static str {
    match value {
        IndexTraceFileChange::Pending => "pending",
        IndexTraceFileChange::New => "new",
        IndexTraceFileChange::Changed => "changed",
        IndexTraceFileChange::Unchanged => "unchanged",
        IndexTraceFileChange::Deleted => "deleted",
    }
}
fn decode_change(value: &str) -> Result<IndexTraceFileChange, IndexTraceStoreFailure> {
    match value {
        "pending" => Ok(IndexTraceFileChange::Pending),
        "new" => Ok(IndexTraceFileChange::New),
        "changed" => Ok(IndexTraceFileChange::Changed),
        "unchanged" => Ok(IndexTraceFileChange::Unchanged),
        "deleted" => Ok(IndexTraceFileChange::Deleted),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_hash(value: IndexTraceHashOutcome) -> &'static str {
    match value {
        IndexTraceHashOutcome::Pending => "pending",
        IndexTraceHashOutcome::Hashed => "hashed",
        IndexTraceHashOutcome::Reused => "reused",
        IndexTraceHashOutcome::NotApplicable => "not_applicable",
    }
}
fn decode_hash(value: &str) -> Result<IndexTraceHashOutcome, IndexTraceStoreFailure> {
    match value {
        "pending" => Ok(IndexTraceHashOutcome::Pending),
        "hashed" => Ok(IndexTraceHashOutcome::Hashed),
        "reused" => Ok(IndexTraceHashOutcome::Reused),
        "not_applicable" => Ok(IndexTraceHashOutcome::NotApplicable),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_parse(value: IndexTraceParseOutcome) -> &'static str {
    match value {
        IndexTraceParseOutcome::Structural => "structural",
        IndexTraceParseOutcome::Reused => "reused",
        IndexTraceParseOutcome::Generic => "generic",
        IndexTraceParseOutcome::Failed => "failed",
        IndexTraceParseOutcome::NotApplicable => "not_applicable",
    }
}
fn decode_parse(value: &str) -> Result<IndexTraceParseOutcome, IndexTraceStoreFailure> {
    match value {
        "structural" => Ok(IndexTraceParseOutcome::Structural),
        "reused" => Ok(IndexTraceParseOutcome::Reused),
        "generic" => Ok(IndexTraceParseOutcome::Generic),
        "failed" => Ok(IndexTraceParseOutcome::Failed),
        "not_applicable" => Ok(IndexTraceParseOutcome::NotApplicable),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_diagnostic(value: IndexTraceDiagnosticCode) -> &'static str {
    match value {
        IndexTraceDiagnosticCode::Discovery => "discovery",
        IndexTraceDiagnosticCode::SourceUnavailable => "source_unavailable",
        IndexTraceDiagnosticCode::RevisionChanged => "revision_changed",
        IndexTraceDiagnosticCode::Parse => "parse",
        IndexTraceDiagnosticCode::Link => "link",
        IndexTraceDiagnosticCode::Rank => "rank",
        IndexTraceDiagnosticCode::Publish => "publish",
        IndexTraceDiagnosticCode::ResourceLimit => "resource_limit",
        IndexTraceDiagnosticCode::Timeout => "timeout",
        IndexTraceDiagnosticCode::ProgressUnavailable => "progress_unavailable",
        IndexTraceDiagnosticCode::JournalIncomplete => "journal_incomplete",
        IndexTraceDiagnosticCode::WorkerUnavailable => "worker_unavailable",
        IndexTraceDiagnosticCode::Interrupted => "interrupted",
    }
}
fn decode_diagnostic(value: &str) -> Result<IndexTraceDiagnosticCode, IndexTraceStoreFailure> {
    match value {
        "discovery" => Ok(IndexTraceDiagnosticCode::Discovery),
        "source_unavailable" => Ok(IndexTraceDiagnosticCode::SourceUnavailable),
        "revision_changed" => Ok(IndexTraceDiagnosticCode::RevisionChanged),
        "parse" => Ok(IndexTraceDiagnosticCode::Parse),
        "link" => Ok(IndexTraceDiagnosticCode::Link),
        "rank" => Ok(IndexTraceDiagnosticCode::Rank),
        "publish" => Ok(IndexTraceDiagnosticCode::Publish),
        "resource_limit" => Ok(IndexTraceDiagnosticCode::ResourceLimit),
        "timeout" => Ok(IndexTraceDiagnosticCode::Timeout),
        "progress_unavailable" => Ok(IndexTraceDiagnosticCode::ProgressUnavailable),
        "journal_incomplete" => Ok(IndexTraceDiagnosticCode::JournalIncomplete),
        "worker_unavailable" => Ok(IndexTraceDiagnosticCode::WorkerUnavailable),
        "interrupted" => Ok(IndexTraceDiagnosticCode::Interrupted),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_event(value: IndexTraceEventKind) -> &'static str {
    match value {
        IndexTraceEventKind::Queued => "queued",
        IndexTraceEventKind::Started => "started",
        IndexTraceEventKind::PhaseStarted => "phase_started",
        IndexTraceEventKind::Progress => "progress",
        IndexTraceEventKind::FileObserved => "file_observed",
        IndexTraceEventKind::Diagnostic => "diagnostic",
        IndexTraceEventKind::CancellationRequested => "cancellation_requested",
        IndexTraceEventKind::Succeeded => "succeeded",
        IndexTraceEventKind::Failed => "failed",
        IndexTraceEventKind::Cancelled => "cancelled",
        IndexTraceEventKind::Interrupted => "interrupted",
    }
}
fn decode_event(value: &str) -> Result<IndexTraceEventKind, IndexTraceStoreFailure> {
    match value {
        "queued" => Ok(IndexTraceEventKind::Queued),
        "started" => Ok(IndexTraceEventKind::Started),
        "phase_started" => Ok(IndexTraceEventKind::PhaseStarted),
        "progress" => Ok(IndexTraceEventKind::Progress),
        "file_observed" => Ok(IndexTraceEventKind::FileObserved),
        "diagnostic" => Ok(IndexTraceEventKind::Diagnostic),
        "cancellation_requested" => Ok(IndexTraceEventKind::CancellationRequested),
        "succeeded" => Ok(IndexTraceEventKind::Succeeded),
        "failed" => Ok(IndexTraceEventKind::Failed),
        "cancelled" => Ok(IndexTraceEventKind::Cancelled),
        "interrupted" => Ok(IndexTraceEventKind::Interrupted),
        _ => Err(IndexTraceStoreFailure::InvalidStoredData),
    }
}
fn encode_diagnostics(values: &[IndexTraceDiagnosticCode]) -> String {
    values
        .iter()
        .copied()
        .map(encode_diagnostic)
        .collect::<Vec<_>>()
        .join(",")
}
fn decode_diagnostics(
    value: &str,
) -> Result<Vec<IndexTraceDiagnosticCode>, IndexTraceStoreFailure> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(decode_diagnostic).collect()
}
fn encode_filter(value: IndexTraceFileFilter) -> &'static str {
    match value {
        IndexTraceFileFilter::All => "all",
        IndexTraceFileFilter::New => "new",
        IndexTraceFileFilter::Changed => "changed",
        IndexTraceFileFilter::Unchanged => "unchanged",
        IndexTraceFileFilter::Deleted => "deleted",
        IndexTraceFileFilter::Hashed => "hashed",
        IndexTraceFileFilter::HashReused => "hash_reused",
        IndexTraceFileFilter::Structural => "structural",
        IndexTraceFileFilter::ParseReused => "parse_reused",
        IndexTraceFileFilter::Generic => "generic",
        IndexTraceFileFilter::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        checkpoint_trace, create_trace, load_retained, query_files, reconcile_interrupted,
    };
    use a3_application::{
        IndexTraceCheckpoint, IndexTraceCounts, IndexTraceEvent, IndexTraceEventKind,
        IndexTraceFileFilter, IndexTraceFileQuery, IndexTraceFileRecord, IndexTracePhaseProgress,
        IndexTraceRunSummary, IndexTraceSnapshot,
    };
    use a3_domain::{
        IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId, IndexTraceParseOutcome,
        IndexTracePhase, IndexTracePhaseState, IndexTraceRevision, IndexTraceState,
        IndexTraceTimestamp, IndexTraceTrigger, RepositoryPath, WorktreeId,
    };

    #[test]
    fn trace_roundtrip_pages_retains_and_reconciles_per_worktree()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            connection
                .execute_batch(
                    "PRAGMA foreign_keys=ON;
                     CREATE TABLE index_runs (
                         worktree_id BLOB NOT NULL,
                         status TEXT NOT NULL
                     );",
                )
                .await?;
            connection
                .execute_batch(include_str!("migrations/knowledge_v39.sql"))
                .await?;
            let worktree = WorktreeId::from_bytes([1; 32]);
            connection
                .execute(
                    "INSERT INTO index_runs (worktree_id, status) VALUES (?1, 'building')",
                    [super::bytes(worktree.as_bytes())],
                )
                .await?;
            let first_id = IndexTraceId::from_bytes([2; 32]);
            let at1 = IndexTraceTimestamp::new(1)?;
            let queued = IndexTraceRunSummary::queued(
                first_id,
                IndexTraceTrigger::InitialObservation,
                at1,
                false,
            );
            let queued_snapshot = IndexTraceSnapshot::new(
                queued,
                pending_phases()?,
                vec![IndexTraceEvent::new(
                    IndexTraceRevision::FIRST,
                    at1,
                    IndexTraceEventKind::Queued,
                    Some(IndexTracePhase::Discover),
                    None,
                    None,
                )],
                Vec::new(),
            )?;
            create_trace(&connection, worktree, &queued_snapshot)
                .await
                .map_err(|_| std::io::Error::other("create queued trace"))?;

            let files = (0..101)
                .map(|index| {
                    Ok::<_, Box<dyn std::error::Error>>(IndexTraceFileRecord::new(
                        RepositoryPath::try_from_bytes(
                            format!("src/file-{index:03}.rs").into_bytes(),
                        )?,
                        IndexTraceFileChange::New,
                        IndexTraceHashOutcome::Hashed,
                        Some(IndexTraceParseOutcome::Structural),
                        Vec::new(),
                        false,
                    )?)
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            let at2 = IndexTraceTimestamp::new(2)?;
            let running = IndexTraceRunSummary::restored(
                first_id,
                IndexTraceRevision::new(2)?,
                IndexTraceState::Running,
                IndexTraceTrigger::InitialObservation,
                at1,
                None,
                at2,
                Some(IndexTracePhase::Parse),
                Some(RepositoryPath::try_from_bytes(b"src/file-100.rs".to_vec())?),
                None,
                false,
                false,
                IndexTraceCounts {
                    discovered: 101,
                    pending: 0,
                    new_files: 101,
                    changed: 0,
                    unchanged: 0,
                    deleted: 0,
                    hashed: 101,
                    hash_reused: 0,
                    structural: 101,
                    parse_reused: 0,
                    generic: 0,
                    failed: 0,
                },
            )?;
            let running_snapshot = IndexTraceSnapshot::new(
                running,
                running_phases(at1, at2)?,
                vec![
                    IndexTraceEvent::new(
                        IndexTraceRevision::FIRST,
                        at1,
                        IndexTraceEventKind::Queued,
                        Some(IndexTracePhase::Discover),
                        None,
                        None,
                    ),
                    IndexTraceEvent::new(
                        IndexTraceRevision::new(2)?,
                        at2,
                        IndexTraceEventKind::Progress,
                        Some(IndexTracePhase::Parse),
                        None,
                        None,
                    ),
                ],
                files.clone(),
            )?;
            checkpoint_trace(
                &connection,
                worktree,
                &IndexTraceCheckpoint {
                    expected_revision: IndexTraceRevision::FIRST,
                    snapshot: running_snapshot,
                    file_updates: files,
                    new_events: vec![IndexTraceEvent::new(
                        IndexTraceRevision::new(2)?,
                        at2,
                        IndexTraceEventKind::Progress,
                        Some(IndexTracePhase::Parse),
                        None,
                        None,
                    )],
                },
            )
            .await
            .map_err(|_| std::io::Error::other("checkpoint running trace"))?;

            let first_page = query_files(
                &connection,
                worktree,
                &IndexTraceFileQuery::new(
                    first_id,
                    IndexTraceRevision::new(2)?,
                    Some("src/file".to_owned()),
                    IndexTraceFileFilter::All,
                    0,
                )?,
            )
            .await
            .map_err(|_| std::io::Error::other("query first page"))?;
            assert_eq!(first_page.files.len(), 100);
            assert_eq!(first_page.total, 101);
            assert!(first_page.has_more);
            let second_page = query_files(
                &connection,
                worktree,
                &IndexTraceFileQuery::new(
                    first_id,
                    IndexTraceRevision::new(2)?,
                    None,
                    IndexTraceFileFilter::All,
                    100,
                )?,
            )
            .await
            .map_err(|_| std::io::Error::other("query second page"))?;
            assert_eq!(second_page.files.len(), 1);

            let at3 = IndexTraceTimestamp::new(3)?;
            let terminal = IndexTraceRunSummary::restored(
                first_id,
                IndexTraceRevision::new(3)?,
                IndexTraceState::Succeeded,
                IndexTraceTrigger::InitialObservation,
                at1,
                Some(at3),
                at3,
                Some(IndexTracePhase::Publish),
                None,
                None,
                false,
                false,
                IndexTraceCounts {
                    discovered: 101,
                    pending: 0,
                    new_files: 101,
                    changed: 0,
                    unchanged: 0,
                    deleted: 0,
                    hashed: 101,
                    hash_reused: 0,
                    structural: 101,
                    parse_reused: 0,
                    generic: 0,
                    failed: 0,
                },
            )?;
            let terminal_snapshot = IndexTraceSnapshot::new(
                terminal,
                succeeded_phases(at1, at3)?,
                vec![IndexTraceEvent::new(
                    IndexTraceRevision::new(3)?,
                    at3,
                    IndexTraceEventKind::Succeeded,
                    Some(IndexTracePhase::Publish),
                    None,
                    None,
                )],
                second_page
                    .files
                    .into_iter()
                    .chain(first_page.files)
                    .collect(),
            )?;
            checkpoint_trace(
                &connection,
                worktree,
                &IndexTraceCheckpoint {
                    expected_revision: IndexTraceRevision::new(2)?,
                    snapshot: terminal_snapshot,
                    file_updates: Vec::new(),
                    new_events: vec![IndexTraceEvent::new(
                        IndexTraceRevision::new(3)?,
                        at3,
                        IndexTraceEventKind::Succeeded,
                        Some(IndexTracePhase::Publish),
                        None,
                        None,
                    )],
                },
            )
            .await
            .map_err(|_| std::io::Error::other("checkpoint terminal trace"))?;

            let second_id = IndexTraceId::from_bytes([3; 32]);
            let at4 = IndexTraceTimestamp::new(4)?;
            let second_snapshot = IndexTraceSnapshot::new(
                IndexTraceRunSummary::queued(second_id, IndexTraceTrigger::ManualRetry, at4, true),
                pending_phases()?,
                vec![IndexTraceEvent::new(
                    IndexTraceRevision::FIRST,
                    at4,
                    IndexTraceEventKind::Queued,
                    Some(IndexTracePhase::Discover),
                    None,
                    None,
                )],
                Vec::new(),
            )?;
            create_trace(&connection, worktree, &second_snapshot)
                .await
                .map_err(|_| std::io::Error::other("create second trace"))?;
            let retained = load_retained(&connection, worktree)
                .await
                .map_err(|_| std::io::Error::other("load two retained traces"))?;
            assert_eq!(
                retained.current.as_ref().map(|trace| trace.summary().id()),
                Some(second_id)
            );
            assert_eq!(
                retained.previous.as_ref().map(|trace| trace.summary().id()),
                Some(first_id)
            );
            assert!(
                load_retained(&connection, WorktreeId::from_bytes([9; 32]))
                    .await?
                    .current
                    .is_none()
            );

            reconcile_interrupted(&connection, worktree, IndexTraceTimestamp::new(5)?)
                .await
                .map_err(|_| std::io::Error::other("reconcile interrupted trace"))?;
            let retained = load_retained(&connection, worktree)
                .await
                .map_err(|_| std::io::Error::other("load reconciled trace"))?;
            assert_eq!(
                retained
                    .current
                    .as_ref()
                    .map(|trace| trace.summary().state()),
                Some(IndexTraceState::Interrupted)
            );
            assert!(retained.previous.is_none());
            let mut orphan_rows = connection
                .query(
                    "SELECT COUNT(*) FROM index_runs WHERE worktree_id = ?1 AND status = 'failed'",
                    [super::bytes(worktree.as_bytes())],
                )
                .await?;
            let orphan_row = orphan_rows
                .next()
                .await?
                .ok_or_else(|| std::io::Error::other("missing orphan reconciliation count"))?;
            assert_eq!(orphan_row.get::<i64>(0)?, 1);
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    fn pending_phases() -> Result<Vec<IndexTracePhaseProgress>, a3_application::IndexTraceDataError>
    {
        IndexTracePhase::ALL
            .into_iter()
            .map(|phase| {
                IndexTracePhaseProgress::new(
                    phase,
                    IndexTracePhaseState::Pending,
                    None,
                    None,
                    None,
                    None,
                )
            })
            .collect()
    }

    fn running_phases(
        start: IndexTraceTimestamp,
        end: IndexTraceTimestamp,
    ) -> Result<Vec<IndexTracePhaseProgress>, a3_application::IndexTraceDataError> {
        IndexTracePhase::ALL
            .into_iter()
            .map(|phase| match phase {
                IndexTracePhase::Discover | IndexTracePhase::Hash => IndexTracePhaseProgress::new(
                    phase,
                    IndexTracePhaseState::Succeeded,
                    Some(start),
                    Some(end),
                    None,
                    None,
                ),
                IndexTracePhase::Parse => IndexTracePhaseProgress::new(
                    phase,
                    IndexTracePhaseState::Running,
                    Some(end),
                    None,
                    Some(101),
                    Some(101),
                ),
                _ => IndexTracePhaseProgress::new(
                    phase,
                    IndexTracePhaseState::Pending,
                    None,
                    None,
                    None,
                    None,
                ),
            })
            .collect()
    }

    fn succeeded_phases(
        start: IndexTraceTimestamp,
        end: IndexTraceTimestamp,
    ) -> Result<Vec<IndexTracePhaseProgress>, a3_application::IndexTraceDataError> {
        IndexTracePhase::ALL
            .into_iter()
            .map(|phase| {
                IndexTracePhaseProgress::new(
                    phase,
                    IndexTracePhaseState::Succeeded,
                    Some(start),
                    Some(end),
                    None,
                    None,
                )
            })
            .collect()
    }
}
