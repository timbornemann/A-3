use a3_application::{
    DEEP_MAP_ENTRY_PAGE_LIMIT, DEEP_MAP_RUN_PAGE_LIMIT, DeepMapEntryDetail, DeepMapEntryPage,
    DeepMapEventResult, DeepMapJournalEvent, DeepMapModelDescriptor, DeepMapPhase,
    DeepMapPublicationAnchor, DeepMapPublicationResult, DeepMapRunCursor, DeepMapRunJournalFailure,
    DeepMapRunPage, DeepMapRunStart, DeepMapRunSummary, DeepMapSafeAction, DeepMapStepDetail,
    DeepMapTargetKind,
};
use a3_domain::{
    DeepMapDiagnosticCode, DeepMapEventSequence, DeepMapMode, DeepMapRunId, DeepMapRunState,
    DeepMapRunTimestamp, ExploreCost, ExploreEvidenceRequirement, ExplorePlan,
    ExplorePlanStopReason, ExploreSeedReason, ExploreTarget, ExploreVerificationMethod, IndexRunId,
    ModelProfileId, ModelProfileReference, ModelProfileVersion, ModuleId, SnapshotId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};

pub(crate) async fn create_run(
    connection: &Connection,
    worktree_id: WorktreeId,
    run: &DeepMapRunStart,
) -> Result<(), DeepMapRunJournalFailure> {
    let transaction = begin(connection).await?;
    let budget = run.mode().budget();
    let model = run.model();
    let affected = transaction
        .execute(
            "INSERT INTO deep_map_runs (\n\
             worktree_id, run_id, index_run_id, snapshot_id, mode, token_budget,\n\
             time_budget_millis, tool_budget, provider_id, model_id, profile_id,\n\
             profile_version, context_tokens, output_tokens, state, created_at_unix_millis,\n\
             updated_at_unix_millis, confirmed_steps, total_steps, latest_event_sequence,\n\
             diagnostic_code, details_incomplete, plan_stop_reason, publication_result\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,\n\
             'queued', ?15, ?15, 0, 0, 1, NULL, 0, NULL, NULL)",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run.id().as_bytes()),
                bytes(run.anchor().index_run_id().as_bytes()),
                bytes(run.anchor().snapshot_id().as_bytes()),
                encode_mode(run.mode()),
                i64::from(budget.tokens()),
                as_i64(budget.milliseconds())?,
                i64::from(budget.tool_calls()),
                model.provider_id(),
                model.model_id(),
                bytes(model.profile().id().as_bytes()),
                i64::from(model.profile().version().get()),
                i64::from(model.context_tokens()),
                i64::from(model.output_tokens()),
                run.created_at().unix_millis(),
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Conflict)?;
    if affected != 1 {
        return rollback(transaction, DeepMapRunJournalFailure::Conflict).await;
    }
    transaction
        .execute(
            "INSERT INTO deep_map_events (\n\
             worktree_id, run_id, sequence, occurred_at_unix_millis, state, phase, target_kind,\n\
             safe_action, module_id, step_position, total_steps, confirmed, result, diagnostic_code\n\
             ) VALUES (?1, ?2, 1, ?3, 'queued', NULL, NULL, NULL, NULL, NULL, NULL, 0,\n\
             'pending', NULL)",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run.id().as_bytes()),
                run.created_at().unix_millis(),
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    commit(transaction).await
}

pub(crate) async fn record_plan(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    plan: &ExplorePlan,
) -> Result<(), DeepMapRunJournalFailure> {
    let transaction = begin(connection).await?;
    let total = as_i64(
        u64::try_from(plan.steps().len()).map_err(|_| DeepMapRunJournalFailure::InvalidInput)?,
    )?;
    let affected = transaction
        .execute(
            "UPDATE deep_map_runs SET total_steps = ?3, plan_stop_reason = ?4\n\
             WHERE worktree_id = ?1 AND run_id = ?2 AND total_steps = 0\n\
               AND state IN ('queued', 'running')",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run_id.as_bytes()),
                total,
                encode_plan_stop(plan.stop_reason()),
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    if affected != 1 {
        return rollback(transaction, DeepMapRunJournalFailure::Conflict).await;
    }
    for step in plan.steps() {
        let cost = step.reserved_cost();
        transaction
            .execute(
                "INSERT INTO deep_map_steps (\n\
                 worktree_id, run_id, step_position, module_id, target_kind, seed_reason,\n\
                 reserved_tokens, reserved_time_millis, reserved_tool_calls,\n\
                 information_gain_basis_points, coverage_field_count, evidence_requirement,\n\
                 verification_method, confirmed\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)",
                params![
                    bytes(worktree_id.as_bytes()),
                    bytes(run_id.as_bytes()),
                    i64::from(step.sequence()),
                    bytes(step.module_id().as_bytes()),
                    encode_explore_target(step.target()),
                    encode_seed_reason(step.reason()),
                    i64::from(cost.tokens()),
                    as_i64(cost.milliseconds())?,
                    i64::from(cost.tool_calls()),
                    i64::from(step.expected_information_gain().basis_points()),
                    as_i64(
                        u64::try_from(step.coverage_fields().len())
                            .map_err(|_| DeepMapRunJournalFailure::InvalidInput)?
                    )?,
                    encode_evidence_requirement(step.evidence_requirement()),
                    encode_verification(step.verification_method()),
                ],
            )
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    }
    commit(transaction).await
}

pub(crate) async fn append_event(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    event: DeepMapJournalEvent,
) -> Result<(), DeepMapRunJournalFailure> {
    let transaction = begin(connection).await?;
    let sequence = as_i64(event.sequence().get())?;
    let previous = sequence
        .checked_sub(1)
        .ok_or(DeepMapRunJournalFailure::InvalidInput)?;
    let step = event.step_position().map(as_i64).transpose()?;
    let total = event.total_steps().map(as_i64).transpose()?;
    let publication = match event.result() {
        DeepMapEventResult::Published => Some("published"),
        DeepMapEventResult::AlreadyCurrent => Some("already-current"),
        _ => None,
    };
    let affected = transaction
        .execute(
            "UPDATE deep_map_runs SET state = ?4, updated_at_unix_millis = ?5,\n\
             confirmed_steps = CASE WHEN ?6 = 1 AND ?7 IS NOT NULL\n\
               THEN MAX(confirmed_steps, ?7) ELSE confirmed_steps END,\n\
             total_steps = CASE WHEN ?8 IS NULL THEN total_steps ELSE MAX(total_steps, ?8) END,\n\
             latest_event_sequence = ?3, diagnostic_code = ?9,\n\
             publication_result = COALESCE(?10, publication_result)\n\
             WHERE worktree_id = ?1 AND run_id = ?2 AND latest_event_sequence = ?11\n\
               AND updated_at_unix_millis <= ?5\n\
               AND state NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run_id.as_bytes()),
                sequence,
                encode_state(event.state()),
                event.occurred_at().unix_millis(),
                i64::from(event.confirmed()),
                step,
                total,
                event.diagnostic().map(encode_diagnostic),
                publication,
                previous,
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    if affected != 1 {
        return rollback(transaction, DeepMapRunJournalFailure::Conflict).await;
    }
    transaction
        .execute(
            "INSERT INTO deep_map_events (\n\
             worktree_id, run_id, sequence, occurred_at_unix_millis, state, phase, target_kind,\n\
             safe_action, module_id, step_position, total_steps, confirmed, result, diagnostic_code\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run_id.as_bytes()),
                sequence,
                event.occurred_at().unix_millis(),
                encode_state(event.state()),
                event.phase().map(encode_phase),
                event.target_kind().map(encode_target),
                event.action().map(encode_action),
                event.module_id().map(|id| bytes(id.as_bytes())),
                step,
                total,
                i64::from(event.confirmed()),
                encode_result(event.result()),
                event.diagnostic().map(encode_diagnostic),
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    if event.confirmed()
        && let Some(step_position) = step
    {
        transaction
            .execute(
                "UPDATE deep_map_steps SET confirmed = 1\n\
                 WHERE worktree_id = ?1 AND run_id = ?2 AND step_position = ?3",
                params![
                    bytes(worktree_id.as_bytes()),
                    bytes(run_id.as_bytes()),
                    step_position,
                ],
            )
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    }
    commit(transaction).await
}

pub(crate) async fn mark_details_incomplete(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
) -> Result<(), DeepMapRunJournalFailure> {
    connection
        .execute(
            "UPDATE deep_map_runs SET details_incomplete = 1 WHERE worktree_id = ?1 AND run_id = ?2",
            params![bytes(worktree_id.as_bytes()), bytes(run_id.as_bytes())],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    Ok(())
}

pub(crate) async fn reconcile_interrupted(
    connection: &Connection,
    worktree_id: WorktreeId,
    occurred_at: DeepMapRunTimestamp,
) -> Result<u64, DeepMapRunJournalFailure> {
    let transaction = begin(connection).await?;
    let mut rows = transaction
        .query(
            "SELECT run_id, latest_event_sequence FROM deep_map_runs\n\
             WHERE worktree_id = ?1 AND state IN\n\
               ('queued', 'running', 'pausing', 'paused', 'cancelling')\n\
             ORDER BY run_id",
            [bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let mut runs = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
    {
        runs.push((
            parse_id(read_bytes(&row, 0)?, DeepMapRunId::from_bytes)?,
            positive_sequence(read_i64(&row, 1)?)?,
        ));
    }
    drop(rows);
    for (run_id, latest) in &runs {
        let next = latest
            .get()
            .checked_add(1)
            .ok_or(DeepMapRunJournalFailure::InvalidStoredData)?;
        transaction
            .execute(
                "UPDATE deep_map_runs SET state = 'interrupted', updated_at_unix_millis = ?3,\n\
                 latest_event_sequence = ?4 WHERE worktree_id = ?1 AND run_id = ?2",
                params![
                    bytes(worktree_id.as_bytes()),
                    bytes(run_id.as_bytes()),
                    occurred_at.unix_millis(),
                    as_i64(next)?,
                ],
            )
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
        transaction
            .execute(
                "INSERT INTO deep_map_events (worktree_id, run_id, sequence,\n\
                 occurred_at_unix_millis, state, confirmed, result)\n\
                 VALUES (?1, ?2, ?3, ?4, 'interrupted', 0, 'interrupted')",
                params![
                    bytes(worktree_id.as_bytes()),
                    bytes(run_id.as_bytes()),
                    as_i64(next)?,
                    occurred_at.unix_millis(),
                ],
            )
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    }
    commit(transaction).await?;
    u64::try_from(runs.len()).map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}

const RUN_COLUMNS: &str = "run_id, index_run_id, snapshot_id, mode, provider_id, model_id,\n\
 profile_id, profile_version, context_tokens, output_tokens, state, created_at_unix_millis,\n\
 updated_at_unix_millis, confirmed_steps, total_steps, diagnostic_code, details_incomplete,\n\
 latest_event_sequence, plan_stop_reason, publication_result";

pub(crate) async fn list_runs(
    connection: &Connection,
    worktree_id: WorktreeId,
    cursor: Option<DeepMapRunCursor>,
) -> Result<DeepMapRunPage, DeepMapRunJournalFailure> {
    let limit = i64::from(DEEP_MAP_RUN_PAGE_LIMIT) + 1;
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM deep_map_runs WHERE worktree_id = ?1\n\
         AND (?2 IS NULL OR updated_at_unix_millis < ?2\n\
           OR (updated_at_unix_millis = ?2 AND run_id < ?3))\n\
         ORDER BY updated_at_unix_millis DESC, run_id DESC LIMIT ?4"
    );
    let mut rows = connection
        .query(
            &sql,
            params![
                bytes(worktree_id.as_bytes()),
                cursor.map(|value| value.updated_at().unix_millis()),
                cursor.map(|value| bytes(value.run_id().as_bytes())),
                limit,
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let mut runs = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
    {
        runs.push(decode_run(&row)?);
    }
    let has_more = runs.len() > usize::from(DEEP_MAP_RUN_PAGE_LIMIT);
    if has_more {
        runs.pop();
    }
    let next = if has_more {
        runs.last()
            .map(|run| DeepMapRunCursor::new(run.updated_at(), run.start().id()))
    } else {
        None
    };
    DeepMapRunPage::new(runs, next)
}

const EVENT_COLUMNS: &str = "sequence, occurred_at_unix_millis, state, phase, target_kind,\n\
 safe_action, module_id, step_position, total_steps, confirmed, result, diagnostic_code";

pub(crate) async fn list_entries(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    before_sequence: Option<DeepMapEventSequence>,
) -> Result<DeepMapEntryPage, DeepMapRunJournalFailure> {
    let sql = format!(
        "SELECT {EVENT_COLUMNS} FROM deep_map_events\n\
         WHERE worktree_id = ?1 AND run_id = ?2 AND (?3 IS NULL OR sequence < ?3)\n\
         ORDER BY sequence DESC LIMIT ?4"
    );
    let mut rows = connection
        .query(
            &sql,
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run_id.as_bytes()),
                before_sequence
                    .map(|value| as_i64(value.get()))
                    .transpose()?,
                i64::from(DEEP_MAP_ENTRY_PAGE_LIMIT) + 1,
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let mut entries = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
    {
        entries.push(decode_event(&row)?);
    }
    let has_more = entries.len() > usize::from(DEEP_MAP_ENTRY_PAGE_LIMIT);
    if has_more {
        entries.pop();
    }
    entries.reverse();
    let next = if has_more {
        entries.first().map(|entry| entry.sequence())
    } else {
        None
    };
    DeepMapEntryPage::new(entries, next)
}

pub(crate) async fn load_entry(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: DeepMapRunId,
    sequence: DeepMapEventSequence,
) -> Result<Option<DeepMapEntryDetail>, DeepMapRunJournalFailure> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let run_sql =
        format!("SELECT {RUN_COLUMNS} FROM deep_map_runs WHERE worktree_id = ?1 AND run_id = ?2");
    let mut run_rows = transaction
        .query(
            &run_sql,
            params![bytes(worktree_id.as_bytes()), bytes(run_id.as_bytes())],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let Some(run_row) = run_rows
        .next()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
    else {
        drop(run_rows);
        commit(transaction).await?;
        return Ok(None);
    };
    let run = decode_run(&run_row)?;
    drop(run_rows);
    let event_sql = format!(
        "SELECT {EVENT_COLUMNS} FROM deep_map_events\n\
         WHERE worktree_id = ?1 AND run_id = ?2 AND sequence = ?3"
    );
    let mut event_rows = transaction
        .query(
            &event_sql,
            params![
                bytes(worktree_id.as_bytes()),
                bytes(run_id.as_bytes()),
                as_i64(sequence.get())?,
            ],
        )
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    let event = event_rows
        .next()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
        .map(|row| decode_event(&row))
        .transpose()?;
    drop(event_rows);
    let Some(event) = event else {
        commit(transaction).await?;
        return Ok(None);
    };
    let step = if let Some(step_position) = event.step_position() {
        let mut rows = transaction
            .query(
                "SELECT target_kind, seed_reason, reserved_tokens, reserved_time_millis,\n\
                 reserved_tool_calls, information_gain_basis_points, coverage_field_count,\n\
                 confirmed FROM deep_map_steps\n\
                 WHERE worktree_id = ?1 AND run_id = ?2 AND step_position = ?3",
                params![
                    bytes(worktree_id.as_bytes()),
                    bytes(run_id.as_bytes()),
                    as_i64(step_position)?,
                ],
            )
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
        let value = rows
            .next()
            .await
            .map_err(|_| DeepMapRunJournalFailure::Unavailable)?
            .map(|row| decode_step_detail(&row))
            .transpose()?;
        drop(rows);
        value
    } else {
        None
    };
    commit(transaction).await?;
    Ok(Some(DeepMapEntryDetail::new(run, event, step)))
}

async fn begin(connection: &Connection) -> Result<Transaction, DeepMapRunJournalFailure> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}

async fn commit(transaction: Transaction) -> Result<(), DeepMapRunJournalFailure> {
    transaction
        .commit()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}

async fn rollback(
    transaction: Transaction,
    failure: DeepMapRunJournalFailure,
) -> Result<(), DeepMapRunJournalFailure> {
    transaction
        .rollback()
        .await
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)?;
    Err(failure)
}

fn decode_run(row: &libsql::Row) -> Result<DeepMapRunSummary, DeepMapRunJournalFailure> {
    let run_id = parse_id(read_bytes(row, 0)?, DeepMapRunId::from_bytes)?;
    let anchor = DeepMapPublicationAnchor::new(
        parse_id(read_bytes(row, 1)?, IndexRunId::from_bytes)?,
        parse_id(read_bytes(row, 2)?, SnapshotId::from_bytes)?,
    );
    let mode = decode_mode(&read_string(row, 3)?)?;
    let profile = ModelProfileReference::new(
        parse_id(read_bytes(row, 6)?, ModelProfileId::from_bytes)?,
        ModelProfileVersion::from_u16(as_u16(read_i64(row, 7)?)?)
            .map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)?,
    );
    let model = DeepMapModelDescriptor::from_stored_parts(
        profile,
        read_string(row, 4)?,
        read_string(row, 5)?,
        as_u32(read_i64(row, 8)?)?,
        as_u32(read_i64(row, 9)?)?,
    )
    .map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)?;
    let start = DeepMapRunStart::new(run_id, anchor, mode, model, timestamp(read_i64(row, 11)?)?);
    DeepMapRunSummary::new(
        start,
        decode_state(&read_string(row, 10)?)?,
        timestamp(read_i64(row, 12)?)?,
        as_u64(read_i64(row, 13)?)?,
        as_u64(read_i64(row, 14)?)?,
        read_optional_string(row, 15)?
            .map(|value| decode_diagnostic(&value))
            .transpose()?,
        read_i64(row, 16)? == 1,
        positive_sequence(read_i64(row, 17)?)?,
        read_optional_string(row, 18)?
            .map(|value| decode_plan_stop(&value))
            .transpose()?,
        read_optional_string(row, 19)?
            .map(|value| decode_publication_result(&value))
            .transpose()?,
    )
}

fn decode_step_detail(row: &libsql::Row) -> Result<DeepMapStepDetail, DeepMapRunJournalFailure> {
    DeepMapStepDetail::new(
        decode_target(&read_string(row, 0)?)?,
        decode_seed_reason(&read_string(row, 1)?)?,
        ExploreCost::new(
            as_u32(read_i64(row, 2)?)?,
            as_u64(read_i64(row, 3)?)?,
            as_u16(read_i64(row, 4)?)?,
        )
        .map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)?,
        as_u16(read_i64(row, 5)?)?,
        as_u16(read_i64(row, 6)?)?,
        read_i64(row, 7)? == 1,
    )
}

fn decode_event(row: &libsql::Row) -> Result<DeepMapJournalEvent, DeepMapRunJournalFailure> {
    DeepMapJournalEvent::new(
        positive_sequence(read_i64(row, 0)?)?,
        timestamp(read_i64(row, 1)?)?,
        decode_state(&read_string(row, 2)?)?,
        read_optional_string(row, 3)?
            .map(|value| decode_phase(&value))
            .transpose()?,
        read_optional_string(row, 4)?
            .map(|value| decode_target(&value))
            .transpose()?,
        read_optional_string(row, 5)?
            .map(|value| decode_action(&value))
            .transpose()?,
        read_optional_bytes(row, 6)?
            .map(|value| parse_id(value, ModuleId::from_bytes))
            .transpose()?,
        read_optional_i64(row, 7)?.map(as_u64).transpose()?,
        read_optional_i64(row, 8)?.map(as_u64).transpose()?,
        read_i64(row, 9)? == 1,
        decode_result(&read_string(row, 10)?)?,
        read_optional_string(row, 11)?
            .map(|value| decode_diagnostic(&value))
            .transpose()?,
    )
}

const fn encode_mode(value: DeepMapMode) -> &'static str {
    match value {
        DeepMapMode::Fast => "fast",
        DeepMapMode::Standard => "standard",
        DeepMapMode::Thorough => "thorough",
    }
}
fn decode_mode(value: &str) -> Result<DeepMapMode, DeepMapRunJournalFailure> {
    match value {
        "fast" => Ok(DeepMapMode::Fast),
        "standard" => Ok(DeepMapMode::Standard),
        "thorough" => Ok(DeepMapMode::Thorough),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_state(value: DeepMapRunState) -> &'static str {
    match value {
        DeepMapRunState::Queued => "queued",
        DeepMapRunState::Running => "running",
        DeepMapRunState::Pausing => "pausing",
        DeepMapRunState::Paused => "paused",
        DeepMapRunState::Cancelling => "cancelling",
        DeepMapRunState::Succeeded => "succeeded",
        DeepMapRunState::Failed => "failed",
        DeepMapRunState::Cancelled => "cancelled",
        DeepMapRunState::Interrupted => "interrupted",
    }
}
fn decode_state(value: &str) -> Result<DeepMapRunState, DeepMapRunJournalFailure> {
    match value {
        "queued" => Ok(DeepMapRunState::Queued),
        "running" => Ok(DeepMapRunState::Running),
        "pausing" => Ok(DeepMapRunState::Pausing),
        "paused" => Ok(DeepMapRunState::Paused),
        "cancelling" => Ok(DeepMapRunState::Cancelling),
        "succeeded" => Ok(DeepMapRunState::Succeeded),
        "failed" => Ok(DeepMapRunState::Failed),
        "cancelled" => Ok(DeepMapRunState::Cancelled),
        "interrupted" => Ok(DeepMapRunState::Interrupted),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_phase(value: DeepMapPhase) -> &'static str {
    match value {
        DeepMapPhase::Planning => "planning",
        DeepMapPhase::Exploring => "exploring",
        DeepMapPhase::Claiming => "claiming",
        DeepMapPhase::Verifying => "verifying",
        DeepMapPhase::Publishing => "publishing",
    }
}
fn decode_phase(value: &str) -> Result<DeepMapPhase, DeepMapRunJournalFailure> {
    match value {
        "planning" => Ok(DeepMapPhase::Planning),
        "exploring" => Ok(DeepMapPhase::Exploring),
        "claiming" => Ok(DeepMapPhase::Claiming),
        "verifying" => Ok(DeepMapPhase::Verifying),
        "publishing" => Ok(DeepMapPhase::Publishing),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_target(value: DeepMapTargetKind) -> &'static str {
    match value {
        DeepMapTargetKind::Project => "project",
        DeepMapTargetKind::Module => "module",
        DeepMapTargetKind::Manifest => "manifest",
        DeepMapTargetKind::Symbol => "symbol",
    }
}
fn decode_target(value: &str) -> Result<DeepMapTargetKind, DeepMapRunJournalFailure> {
    match value {
        "project" => Ok(DeepMapTargetKind::Project),
        "module" => Ok(DeepMapTargetKind::Module),
        "manifest" => Ok(DeepMapTargetKind::Manifest),
        "symbol" => Ok(DeepMapTargetKind::Symbol),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_action(value: DeepMapSafeAction) -> &'static str {
    match value {
        DeepMapSafeAction::BuildPlan => "build-plan",
        DeepMapSafeAction::Inspect => "inspect",
        DeepMapSafeAction::Search => "search",
        DeepMapSafeAction::Propose => "propose",
        DeepMapSafeAction::GenerateClaims => "generate-claims",
        DeepMapSafeAction::VerifyEvidence => "verify-evidence",
        DeepMapSafeAction::PublishCards => "publish-cards",
    }
}
fn decode_action(value: &str) -> Result<DeepMapSafeAction, DeepMapRunJournalFailure> {
    match value {
        "build-plan" => Ok(DeepMapSafeAction::BuildPlan),
        "inspect" => Ok(DeepMapSafeAction::Inspect),
        "search" => Ok(DeepMapSafeAction::Search),
        "propose" => Ok(DeepMapSafeAction::Propose),
        "generate-claims" => Ok(DeepMapSafeAction::GenerateClaims),
        "verify-evidence" => Ok(DeepMapSafeAction::VerifyEvidence),
        "publish-cards" => Ok(DeepMapSafeAction::PublishCards),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_result(value: DeepMapEventResult) -> &'static str {
    match value {
        DeepMapEventResult::Pending => "pending",
        DeepMapEventResult::Confirmed => "confirmed",
        DeepMapEventResult::AlreadyCurrent => "already-current",
        DeepMapEventResult::Published => "published",
        DeepMapEventResult::Paused => "paused",
        DeepMapEventResult::Resumed => "resumed",
        DeepMapEventResult::Cancelled => "cancelled",
        DeepMapEventResult::Failed => "failed",
        DeepMapEventResult::Interrupted => "interrupted",
    }
}
fn decode_result(value: &str) -> Result<DeepMapEventResult, DeepMapRunJournalFailure> {
    match value {
        "pending" => Ok(DeepMapEventResult::Pending),
        "confirmed" => Ok(DeepMapEventResult::Confirmed),
        "already-current" => Ok(DeepMapEventResult::AlreadyCurrent),
        "published" => Ok(DeepMapEventResult::Published),
        "paused" => Ok(DeepMapEventResult::Paused),
        "resumed" => Ok(DeepMapEventResult::Resumed),
        "cancelled" => Ok(DeepMapEventResult::Cancelled),
        "failed" => Ok(DeepMapEventResult::Failed),
        "interrupted" => Ok(DeepMapEventResult::Interrupted),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_diagnostic(value: DeepMapDiagnosticCode) -> &'static str {
    match value {
        DeepMapDiagnosticCode::NoPublishedIndex => "no-published-index",
        DeepMapDiagnosticCode::StaleIndex => "stale-index",
        DeepMapDiagnosticCode::Planning => "planning",
        DeepMapDiagnosticCode::ModelUnavailable => "model-unavailable",
        DeepMapDiagnosticCode::ModelRejected => "model-rejected",
        DeepMapDiagnosticCode::ModelTimeout => "model-timeout",
        DeepMapDiagnosticCode::InvalidModelResponse => "invalid-model-response",
        DeepMapDiagnosticCode::Read => "read",
        DeepMapDiagnosticCode::Verification => "verification",
        DeepMapDiagnosticCode::PublicationRejected => "publication-rejected",
        DeepMapDiagnosticCode::PublicationStorage => "publication-storage",
        DeepMapDiagnosticCode::PublicationTimeout => "publication-timeout",
        DeepMapDiagnosticCode::PublicationProgress => "publication-progress",
        DeepMapDiagnosticCode::InvalidCheckpoint => "invalid-checkpoint",
        DeepMapDiagnosticCode::ProgressUnavailable => "progress-unavailable",
        DeepMapDiagnosticCode::Interrupted => "interrupted",
    }
}
fn decode_diagnostic(value: &str) -> Result<DeepMapDiagnosticCode, DeepMapRunJournalFailure> {
    match value {
        "no-published-index" => Ok(DeepMapDiagnosticCode::NoPublishedIndex),
        "stale-index" => Ok(DeepMapDiagnosticCode::StaleIndex),
        "planning" => Ok(DeepMapDiagnosticCode::Planning),
        "model-unavailable" => Ok(DeepMapDiagnosticCode::ModelUnavailable),
        "model-rejected" => Ok(DeepMapDiagnosticCode::ModelRejected),
        "model-timeout" => Ok(DeepMapDiagnosticCode::ModelTimeout),
        "invalid-model-response" => Ok(DeepMapDiagnosticCode::InvalidModelResponse),
        "read" => Ok(DeepMapDiagnosticCode::Read),
        "verification" => Ok(DeepMapDiagnosticCode::Verification),
        "publication-rejected" => Ok(DeepMapDiagnosticCode::PublicationRejected),
        "publication-storage" => Ok(DeepMapDiagnosticCode::PublicationStorage),
        "publication-timeout" => Ok(DeepMapDiagnosticCode::PublicationTimeout),
        "publication-progress" => Ok(DeepMapDiagnosticCode::PublicationProgress),
        "invalid-checkpoint" => Ok(DeepMapDiagnosticCode::InvalidCheckpoint),
        "progress-unavailable" => Ok(DeepMapDiagnosticCode::ProgressUnavailable),
        "interrupted" => Ok(DeepMapDiagnosticCode::Interrupted),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_plan_stop(value: ExplorePlanStopReason) -> &'static str {
    match value {
        ExplorePlanStopReason::CoveragePlanned => "coverage-planned",
        ExplorePlanStopReason::BudgetExhausted => "budget-exhausted",
        ExplorePlanStopReason::BelowInformationGainThreshold => "below-gain-threshold",
        ExplorePlanStopReason::NoEligibleSeed => "no-eligible-seed",
    }
}
fn decode_plan_stop(value: &str) -> Result<ExplorePlanStopReason, DeepMapRunJournalFailure> {
    match value {
        "coverage-planned" => Ok(ExplorePlanStopReason::CoveragePlanned),
        "budget-exhausted" => Ok(ExplorePlanStopReason::BudgetExhausted),
        "below-gain-threshold" => Ok(ExplorePlanStopReason::BelowInformationGainThreshold),
        "no-eligible-seed" => Ok(ExplorePlanStopReason::NoEligibleSeed),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
fn decode_publication_result(
    value: &str,
) -> Result<DeepMapPublicationResult, DeepMapRunJournalFailure> {
    match value {
        "published" => Ok(DeepMapPublicationResult::Published),
        "already-current" => Ok(DeepMapPublicationResult::AlreadyCurrent),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_explore_target(value: &ExploreTarget) -> &'static str {
    match value {
        ExploreTarget::Module(_) => "module",
        ExploreTarget::Manifest { .. } => "manifest",
        ExploreTarget::Symbol(_) => "symbol",
    }
}
const fn encode_seed_reason(value: ExploreSeedReason) -> &'static str {
    match value {
        ExploreSeedReason::Manifest => "manifest",
        ExploreSeedReason::Entrypoint => "entrypoint",
        ExploreSeedReason::CentralSymbol => "central-symbol",
        ExploreSeedReason::TestRoot => "test-root",
        ExploreSeedReason::GraphCommunity => "graph-community",
        ExploreSeedReason::UncoveredModule => "uncovered-module",
    }
}
fn decode_seed_reason(value: &str) -> Result<ExploreSeedReason, DeepMapRunJournalFailure> {
    match value {
        "manifest" => Ok(ExploreSeedReason::Manifest),
        "entrypoint" => Ok(ExploreSeedReason::Entrypoint),
        "central-symbol" => Ok(ExploreSeedReason::CentralSymbol),
        "test-root" => Ok(ExploreSeedReason::TestRoot),
        "graph-community" => Ok(ExploreSeedReason::GraphCommunity),
        "uncovered-module" => Ok(ExploreSeedReason::UncoveredModule),
        _ => Err(DeepMapRunJournalFailure::InvalidStoredData),
    }
}
const fn encode_evidence_requirement(value: ExploreEvidenceRequirement) -> &'static str {
    match value {
        ExploreEvidenceRequirement::CurrentModuleProjection
        | ExploreEvidenceRequirement::CurrentManifestRevision
        | ExploreEvidenceRequirement::CurrentSymbolRevision => "field-evidence",
    }
}
const fn encode_verification(value: ExploreVerificationMethod) -> &'static str {
    match value {
        ExploreVerificationMethod::ResolveFieldEvidenceAgainstPublishedIndex => {
            "published-index-evidence"
        }
    }
}

fn bytes(value: &[u8; 32]) -> Vec<u8> {
    value.to_vec()
}
fn as_i64(value: u64) -> Result<i64, DeepMapRunJournalFailure> {
    i64::try_from(value).map_err(|_| DeepMapRunJournalFailure::InvalidInput)
}
fn as_u64(value: i64) -> Result<u64, DeepMapRunJournalFailure> {
    u64::try_from(value).map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}
fn as_u32(value: i64) -> Result<u32, DeepMapRunJournalFailure> {
    u32::try_from(value).map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}
fn as_u16(value: i64) -> Result<u16, DeepMapRunJournalFailure> {
    u16::try_from(value).map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}
fn timestamp(value: i64) -> Result<DeepMapRunTimestamp, DeepMapRunJournalFailure> {
    DeepMapRunTimestamp::new(value).map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}
fn positive_sequence(value: i64) -> Result<DeepMapEventSequence, DeepMapRunJournalFailure> {
    DeepMapEventSequence::new(as_u64(value)?)
        .map_err(|_| DeepMapRunJournalFailure::InvalidStoredData)
}
fn parse_id<T>(
    value: Vec<u8>,
    build: impl FnOnce([u8; 32]) -> T,
) -> Result<T, DeepMapRunJournalFailure> {
    Ok(build(value.try_into().map_err(|_| {
        DeepMapRunJournalFailure::InvalidStoredData
    })?))
}
fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
fn read_optional_i64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<i64>, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
fn read_string(row: &libsql::Row, index: i32) -> Result<String, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
fn read_optional_string(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
fn read_bytes(row: &libsql::Row, index: i32) -> Result<Vec<u8>, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
fn read_optional_bytes(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<Vec<u8>>, DeepMapRunJournalFailure> {
    row.get(index)
        .map_err(|_| DeepMapRunJournalFailure::Unavailable)
}
