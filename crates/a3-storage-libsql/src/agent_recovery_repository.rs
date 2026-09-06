use crate::{catalog::is_corruption, run_journal_repository, task_ledger_repository};
use a3_application::{
    AgentMutationResultRecord, AgentRecoveryChoice, AgentRecoveryStoreFailure,
    TaskLedgerStoreFailure, TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind, AgentRun, AgentRunId,
    AgentRunTimestamp, AgentToolAttempt, AgentToolAttemptNumber, AgentToolAttemptStatus,
    AgentToolEvidence, ContentHash, EvidenceRef, FileRevision, MutationActionFingerprint,
    MutationReconciliation, RepositoryPath, RunEvent, RunEventCode, RunEventKind, RunEventOutcome,
    RunEventSequence, RunEventSubject, SnapshotId, SourcePosition, SourceRange, TaskEvidenceId,
    TaskLedger, ToolRunId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;
const EVIDENCE_QUERY_BATCH: usize = 512;
const MAX_RECOVERY_EVIDENCE: usize = 16_384;
const MAX_MUTATION_ATTEMPTS: usize = 4_096;

pub(crate) async fn begin_tool_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    tool_run_id: ToolRunId,
    started_at: AgentRunTimestamp,
) -> Result<AgentToolAttempt, AgentRecoveryRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        validate_active_run_snapshot(&transaction, worktree_id, run_id, snapshot_id).await?;
        let next_attempt = next_attempt_number(&transaction, tool_run_id).await?;
        transaction
            .execute(
                "INSERT INTO tool_run_attempts (
                 tool_run_id, attempt_sequence, run_id, snapshot_id, status,
                 started_at_unix_millis, updated_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, 'in_flight', ?5, ?5)",
                params![
                    id_bytes(tool_run_id),
                    i64::from(next_attempt.get()),
                    id_bytes(run_id),
                    id_bytes(snapshot_id),
                    timestamp_to_i64(started_at)
                ],
            )
            .await
            .map_err(classify_attempt_constraint)?;
        AgentToolAttempt::new(
            tool_run_id,
            next_attempt,
            run_id,
            snapshot_id,
            AgentToolAttemptStatus::InFlight,
            started_at,
            started_at,
        )
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
    }
    .await;
    close_write_transaction(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn begin_mutation_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    tool_run_id: ToolRunId,
    fingerprint: MutationActionFingerprint,
    kind: AgentMutationKind,
    started_at: AgentRunTimestamp,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    if kind == AgentMutationKind::UnclassifiedLegacy {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        validate_active_run_snapshot(&transaction, worktree_id, run_id, snapshot_id).await?;
        if has_unreconciled_mutation(&transaction, worktree_id).await? {
            return Err(AgentRecoveryRepositoryError::MutationReconciliationRequired);
        }
        let next_attempt = next_attempt_number(&transaction, tool_run_id).await?;
        transaction
            .execute(
                "INSERT INTO tool_run_attempts (
                 tool_run_id, attempt_sequence, run_id, snapshot_id, status,
                 started_at_unix_millis, updated_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, 'in_flight', ?5, ?5)",
                params![
                    id_bytes(tool_run_id),
                    i64::from(next_attempt.get()),
                    id_bytes(run_id),
                    id_bytes(snapshot_id),
                    timestamp_to_i64(started_at)
                ],
            )
            .await
            .map_err(classify_attempt_constraint)?;
        transaction
            .execute(
                "INSERT INTO mutation_attempts (
                 tool_run_id, attempt_sequence, action_fingerprint, action_kind,
                 application_state, reconciliation_state, reconciled_snapshot_id,
                 reconciled_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, 'unknown', 'required', NULL, NULL)",
                params![
                    id_bytes(tool_run_id),
                    i64::from(next_attempt.get()),
                    fingerprint.as_bytes().to_vec(),
                    mutation_kind_text(kind)
                ],
            )
            .await
            .map_err(classify_attempt_constraint)?;
        mutation_attempt(
            tool_run_id,
            next_attempt,
            run_id,
            snapshot_id,
            AgentToolAttemptStatus::InFlight,
            started_at,
            started_at,
            fingerprint,
            kind,
            AgentMutationDisposition::Unknown(MutationReconciliation::Required),
        )
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn finish_tool_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
    status: AgentToolAttemptStatus,
    finished_at: AgentRunTimestamp,
) -> Result<AgentToolAttempt, AgentRecoveryRepositoryError> {
    if !matches!(
        status,
        AgentToolAttemptStatus::Failed
            | AgentToolAttemptStatus::Cancelled
            | AgentToolAttemptStatus::Denied
    ) {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let existing = read_tool_attempt(&transaction, worktree_id, tool_run_id, attempt)
            .await?
            .ok_or(AgentRecoveryRepositoryError::ToolAttemptConflict)?;
        if existing.status() != AgentToolAttemptStatus::InFlight
            || finished_at < existing.started_at()
            || mutation_attempt_exists(&transaction, tool_run_id, attempt).await?
        {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        let changed = transaction
            .execute(
                "UPDATE tool_run_attempts SET status = ?1, updated_at_unix_millis = ?2
                 WHERE tool_run_id = ?3 AND attempt_sequence = ?4 AND status = 'in_flight'",
                params![
                    attempt_status_text(status),
                    timestamp_to_i64(finished_at),
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        AgentToolAttempt::new(
            tool_run_id,
            attempt,
            existing.run_id(),
            existing.snapshot_id(),
            status,
            existing.started_at(),
            finished_at,
        )
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn finish_mutation_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
    status: AgentToolAttemptStatus,
    disposition: AgentMutationDisposition,
    finished_at: AgentRunTimestamp,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    if !matches!(
        status,
        AgentToolAttemptStatus::Failed
            | AgentToolAttemptStatus::Cancelled
            | AgentToolAttemptStatus::Denied
    ) || matches!(
        disposition,
        AgentMutationDisposition::Unknown(
            MutationReconciliation::Reconciled { .. } | MutationReconciliation::Replanned { .. }
        )
    ) {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let existing = read_mutation_attempt(&transaction, worktree_id, tool_run_id, attempt)
            .await?
            .ok_or(AgentRecoveryRepositoryError::ToolAttemptConflict)?;
        let tool_attempt = existing.tool_attempt();
        if tool_attempt.status() != AgentToolAttemptStatus::InFlight
            || existing.disposition()
                != AgentMutationDisposition::Unknown(MutationReconciliation::Required)
            || finished_at < tool_attempt.started_at()
        {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        let changed = transaction
            .execute(
                "UPDATE tool_run_attempts SET status = ?1, updated_at_unix_millis = ?2
                 WHERE tool_run_id = ?3 AND attempt_sequence = ?4 AND status = 'in_flight'",
                params![
                    attempt_status_text(status),
                    timestamp_to_i64(finished_at),
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        let (application_state, reconciliation_state) = disposition_text(disposition)?;
        let changed = transaction
            .execute(
                "UPDATE mutation_attempts
                 SET application_state = ?1, reconciliation_state = ?2
                 WHERE tool_run_id = ?3 AND attempt_sequence = ?4
                   AND application_state = 'unknown' AND reconciliation_state = 'required'",
                params![
                    application_state,
                    reconciliation_state,
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        mutation_attempt(
            tool_run_id,
            attempt,
            tool_attempt.run_id(),
            tool_attempt.snapshot_id(),
            status,
            tool_attempt.started_at(),
            finished_at,
            existing.fingerprint(),
            existing.kind(),
            disposition,
        )
    }
    .await;
    close_write_transaction(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn complete_tool_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<AgentToolAttempt, AgentRecoveryRepositoryError> {
    if event.kind() != RunEventKind::ToolAction
        || event.subject() != Some(RunEventSubject::Tool(tool_run_id))
        || event.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || event.run_id() != run.id()
    {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let existing = read_tool_attempt(&transaction, worktree_id, tool_run_id, attempt)
            .await?
            .ok_or(AgentRecoveryRepositoryError::ToolAttemptConflict)?;
        if existing.status() != AgentToolAttemptStatus::InFlight
            || existing.run_id() != run.id()
            || event.occurred_at() < existing.started_at()
            || mutation_attempt_exists(&transaction, tool_run_id, attempt).await?
        {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(AgentRecoveryRepositoryError::RunJournal)?;
        let changed = transaction
            .execute(
                "UPDATE tool_run_attempts SET status = 'succeeded', updated_at_unix_millis = ?1
                 WHERE tool_run_id = ?2 AND attempt_sequence = ?3 AND status = 'in_flight'",
                params![
                    timestamp_to_i64(event.occurred_at()),
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        AgentToolAttempt::new(
            tool_run_id,
            attempt,
            existing.run_id(),
            existing.snapshot_id(),
            AgentToolAttemptStatus::Succeeded,
            existing.started_at(),
            event.occurred_at(),
        )
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
    }
    .await;
    close_write_transaction(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn complete_mutation_attempt(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
    result: AgentMutationResultRecord,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    if event.kind() != RunEventKind::ToolAction
        || event.subject() != Some(RunEventSubject::Tool(tool_run_id))
        || event.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || event.run_id() != run.id()
    {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let existing = read_mutation_attempt(&transaction, worktree_id, tool_run_id, attempt)
            .await?
            .ok_or(AgentRecoveryRepositoryError::ToolAttemptConflict)?;
        let tool_attempt = existing.tool_attempt();
        if tool_attempt.status() != AgentToolAttemptStatus::InFlight
            || tool_attempt.run_id() != run.id()
            || event.occurred_at() < tool_attempt.started_at()
            || existing.disposition()
                != AgentMutationDisposition::Unknown(MutationReconciliation::Required)
        {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(AgentRecoveryRepositoryError::RunJournal)?;
        transaction
            .execute(
                "INSERT INTO tool_runs (
                 tool_run_id, run_id, event_sequence, status, result_digest, result_truncated,
                 snapshot_before_id, snapshot_after_id, observed_output_bytes
                 ) VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5, ?6, ?6, ?7)",
                params![
                    id_bytes(tool_run_id),
                    id_bytes(run.id()),
                    i64::try_from(event.sequence().get())
                        .map_err(|_| AgentRecoveryRepositoryError::ResourceLimit)?,
                    result.digest().as_bytes().to_vec(),
                    i64::from(result.truncated()),
                    id_bytes(event.snapshot_id()),
                    i64::try_from(result.observed_output_bytes())
                        .map_err(|_| AgentRecoveryRepositoryError::ResourceLimit)?
                ],
            )
            .await
            .map_err(classify_attempt_constraint)?;
        let changed = transaction
            .execute(
                "UPDATE tool_run_attempts SET status = 'succeeded', updated_at_unix_millis = ?1
                 WHERE tool_run_id = ?2 AND attempt_sequence = ?3 AND status = 'in_flight'",
                params![
                    timestamp_to_i64(event.occurred_at()),
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        let changed = transaction
            .execute(
                "UPDATE mutation_attempts
                 SET application_state = 'applied', reconciliation_state = 'not_required'
                 WHERE tool_run_id = ?1 AND attempt_sequence = ?2
                   AND application_state = 'unknown' AND reconciliation_state = 'required'",
                params![id_bytes(tool_run_id), i64::from(attempt.get())],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        mutation_attempt(
            tool_run_id,
            attempt,
            tool_attempt.run_id(),
            tool_attempt.snapshot_id(),
            AgentToolAttemptStatus::Succeeded,
            tool_attempt.started_at(),
            event.occurred_at(),
            existing.fingerprint(),
            existing.kind(),
            AgentMutationDisposition::Applied,
        )
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn interrupt_tool_attempts(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    interrupted_at: AgentRunTimestamp,
) -> Result<u32, AgentRecoveryRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        ensure_run_exists(&transaction, worktree_id, run_id).await?;
        let changed = transaction
            .execute(
                "UPDATE tool_run_attempts
                 SET status = 'interrupted', updated_at_unix_millis = ?1
                 WHERE run_id = ?2 AND status = 'in_flight'
                   AND started_at_unix_millis <= ?1",
                params![timestamp_to_i64(interrupted_at), id_bytes(run_id)],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        u32::try_from(changed).map_err(|_| AgentRecoveryRepositoryError::ResourceLimit)
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn load_mutation_attempts(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<Vec<AgentMutationAttempt>, AgentRecoveryRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        ensure_run_exists(&transaction, worktree_id, run_id).await?;
        let mut rows = transaction
            .query(
                "SELECT tool_run_attempts.tool_run_id, tool_run_attempts.attempt_sequence,
                 tool_run_attempts.snapshot_id, tool_run_attempts.status,
                 tool_run_attempts.started_at_unix_millis,
                 tool_run_attempts.updated_at_unix_millis,
                 mutation_attempts.action_fingerprint, mutation_attempts.action_kind,
                 mutation_attempts.application_state, mutation_attempts.reconciliation_state,
                 mutation_attempts.reconciled_snapshot_id
                 FROM mutation_attempts
                 JOIN tool_run_attempts USING (tool_run_id, attempt_sequence)
                 JOIN agent_runs USING (run_id) JOIN tasks USING (task_id)
                 WHERE tool_run_attempts.run_id = ?1 AND tasks.worktree_id = ?2
                 ORDER BY tool_run_attempts.tool_run_id, tool_run_attempts.attempt_sequence
                 LIMIT ?3",
                params![
                    id_bytes(run_id),
                    id_bytes(worktree_id),
                    i64::try_from(MAX_MUTATION_ATTEMPTS + 1)
                        .map_err(|_| AgentRecoveryRepositoryError::ResourceLimit)?
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Read)?;
        let mut attempts = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(AgentRecoveryRepositoryError::Read)?
        {
            attempts.push(read_mutation_row(&row, run_id)?);
            if attempts.len() > MAX_MUTATION_ATTEMPTS {
                return Err(AgentRecoveryRepositoryError::ResourceLimit);
            }
        }
        Ok(attempts)
    }
    .await;
    close_read_transaction(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn reconcile_mutation(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    if event.kind() != RunEventKind::Diagnostic
        || event.subject() != Some(RunEventSubject::Tool(tool_run_id))
        || event.payload().code() != RunEventCode::StateRecovered
        || event.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || event.run_id() != run.id()
        || event.snapshot_id() != run.current_snapshot_id()
    {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let existing = read_mutation_attempt(&transaction, worktree_id, tool_run_id, attempt)
            .await?
            .ok_or(AgentRecoveryRepositoryError::ToolAttemptConflict)?;
        let tool_attempt = existing.tool_attempt();
        if !tool_attempt.status().is_terminal()
            || tool_attempt.run_id() != run.id()
            || event.occurred_at() < tool_attempt.updated_at()
            || existing.disposition()
                != AgentMutationDisposition::Unknown(MutationReconciliation::Required)
        {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        if latest_published_snapshot(&transaction, worktree_id).await? != Some(event.snapshot_id())
        {
            return Err(AgentRecoveryRepositoryError::PublishedSnapshotConflict);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(AgentRecoveryRepositoryError::RunJournal)?;
        let changed = transaction
            .execute(
                "UPDATE mutation_attempts
                 SET reconciliation_state = 'reconciled', reconciled_snapshot_id = ?1,
                   reconciled_at_unix_millis = ?2
                 WHERE tool_run_id = ?3 AND attempt_sequence = ?4
                   AND application_state = 'unknown' AND reconciliation_state = 'required'",
                params![
                    id_bytes(event.snapshot_id()),
                    timestamp_to_i64(event.occurred_at()),
                    id_bytes(tool_run_id),
                    i64::from(attempt.get())
                ],
            )
            .await
            .map_err(AgentRecoveryRepositoryError::Write)?;
        if changed != 1 {
            return Err(AgentRecoveryRepositoryError::ToolAttemptConflict);
        }
        mutation_attempt(
            tool_run_id,
            attempt,
            tool_attempt.run_id(),
            tool_attempt.snapshot_id(),
            tool_attempt.status(),
            tool_attempt.started_at(),
            tool_attempt.updated_at(),
            existing.fingerprint(),
            existing.kind(),
            AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled {
                snapshot_id: event.snapshot_id(),
            }),
        )
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn load_tool_evidence(
    connection: &Connection,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    evidence_ids: &[TaskEvidenceId],
) -> Result<Vec<AgentToolEvidence>, AgentRecoveryRepositoryError> {
    if evidence_ids.is_empty() || evidence_ids.len() > MAX_RECOVERY_EVIDENCE {
        return Err(AgentRecoveryRepositoryError::ResourceLimit);
    }
    let requested = evidence_ids.iter().copied().collect::<BTreeSet<_>>();
    if requested.len() != evidence_ids.len() {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        ensure_run_exists(&transaction, worktree_id, run_id).await?;
        let mut evidence = Vec::new();
        for batch in evidence_ids.chunks(EVIDENCE_QUERY_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let sql = format!(
                "SELECT tool_evidence.evidence_id, tool_evidence.location_kind,
                 tool_evidence.repository_path, tool_evidence.content_hash,
                 tool_evidence.start_byte, tool_evidence.end_byte, tool_evidence.start_row,
                 tool_evidence.start_column, tool_evidence.end_row, tool_evidence.end_column
                 FROM tool_evidence JOIN tool_runs USING (tool_run_id)
                 WHERE tool_runs.run_id = ? AND tool_evidence.evidence_id IN ({placeholders})
                 ORDER BY tool_evidence.evidence_id"
            );
            let mut parameters = vec![Value::Blob(run_id.as_bytes().to_vec())];
            parameters.extend(batch.iter().map(|id| Value::Blob(id.as_bytes().to_vec())));
            let mut rows = transaction
                .query(&sql, params_from_iter(parameters))
                .await
                .map_err(AgentRecoveryRepositoryError::Read)?;
            while let Some(row) = rows
                .next()
                .await
                .map_err(AgentRecoveryRepositoryError::Read)?
            {
                evidence.push(read_evidence(&row)?);
            }
        }
        evidence.sort_by_key(AgentToolEvidence::id);
        if evidence.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(AgentRecoveryRepositoryError::InvalidStoredData);
        }
        Ok(evidence)
    }
    .await;
    close_read_transaction(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn commit_recovery(
    connection: &Connection,
    worktree_id: WorktreeId,
    choice: AgentRecoveryChoice,
    expected_published_snapshot: SnapshotId,
    expected_ledger_version: TaskLedgerStoreVersion,
    expected_last_sequence: RunEventSequence,
    ledger: &TaskLedger,
    run: &AgentRun,
    event: &RunEvent,
) -> Result<TaskLedgerStoreVersion, AgentRecoveryRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentRecoveryRepositoryError::Begin)?;
    let result = async {
        let current = latest_published_snapshot(&transaction, worktree_id)
            .await?
            .ok_or(AgentRecoveryRepositoryError::PublishedSnapshotConflict)?;
        if current != expected_published_snapshot || run.current_snapshot_id() != current {
            return Err(AgentRecoveryRepositoryError::PublishedSnapshotConflict);
        }
        let mutation_gate = mutation_recovery_gate(&transaction, worktree_id, run.id()).await?;
        match choice {
            AgentRecoveryChoice::Resume if mutation_gate != MutationRecoveryGate::Clear => {
                return Err(AgentRecoveryRepositoryError::MutationReconciliationRequired);
            }
            AgentRecoveryChoice::Replan
                if mutation_gate == MutationRecoveryGate::ReconciliationRequired =>
            {
                return Err(AgentRecoveryRepositoryError::MutationReconciliationRequired);
            }
            AgentRecoveryChoice::Resume
            | AgentRecoveryChoice::Replan
            | AgentRecoveryChoice::Cancel => {}
        }
        let next_version = task_ledger_repository::replace_in_transaction(
            &transaction,
            worktree_id,
            expected_ledger_version,
            ledger,
        )
        .await
        .map_err(AgentRecoveryRepositoryError::TaskLedger)?;
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(AgentRecoveryRepositoryError::RunJournal)?;
        if choice == AgentRecoveryChoice::Replan {
            transaction
                .execute(
                    "UPDATE mutation_attempts SET reconciliation_state = 'replanned'
                     WHERE application_state = 'unknown' AND reconciliation_state = 'reconciled'
                       AND EXISTS (SELECT 1 FROM tool_run_attempts
                         WHERE tool_run_attempts.tool_run_id = mutation_attempts.tool_run_id
                           AND tool_run_attempts.attempt_sequence = mutation_attempts.attempt_sequence
                           AND tool_run_attempts.run_id = ?1)",
                    [id_bytes(run.id())],
                )
                .await
                .map_err(AgentRecoveryRepositoryError::Write)?;
        }
        Ok(next_version)
    }
    .await;
    close_write_transaction(transaction, result).await
}

async fn validate_active_run_snapshot(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
) -> Result<(), AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT agent_runs.current_snapshot_id, agent_runs.controller_state
             FROM agent_runs JOIN tasks USING (task_id)
             WHERE agent_runs.run_id = ?1 AND tasks.worktree_id = ?2",
            params![id_bytes(run_id), id_bytes(worktree_id)],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
    else {
        return Err(AgentRecoveryRepositoryError::RunNotFound);
    };
    let stored_snapshot = SnapshotId::from_bytes(read_id(&row, 0)?);
    let state: String = row
        .get(1)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    if stored_snapshot != snapshot_id || matches!(state.as_str(), "done" | "failed" | "cancelled") {
        return Err(AgentRecoveryRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn ensure_run_exists(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<(), AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT 1 FROM agent_runs JOIN tasks USING (task_id)
             WHERE agent_runs.run_id = ?1 AND tasks.worktree_id = ?2 LIMIT 1",
            params![id_bytes(run_id), id_bytes(worktree_id)],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
        .is_none()
    {
        return Err(AgentRecoveryRepositoryError::RunNotFound);
    }
    Ok(())
}

async fn next_attempt_number(
    transaction: &Transaction,
    tool_run_id: ToolRunId,
) -> Result<AgentToolAttemptNumber, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(attempt_sequence), 0) FROM tool_run_attempts
             WHERE tool_run_id = ?1",
            [tool_run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
        .ok_or(AgentRecoveryRepositoryError::InvalidStoredData)?;
    let previous: i64 = row
        .get(0)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let next = u32::try_from(previous)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(AgentRecoveryRepositoryError::ResourceLimit)?;
    AgentToolAttemptNumber::new(next).map_err(|_| AgentRecoveryRepositoryError::ResourceLimit)
}

async fn read_tool_attempt(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<Option<AgentToolAttempt>, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT tool_run_attempts.run_id, tool_run_attempts.snapshot_id,
             tool_run_attempts.status, tool_run_attempts.started_at_unix_millis,
             tool_run_attempts.updated_at_unix_millis
             FROM tool_run_attempts JOIN agent_runs USING (run_id) JOIN tasks USING (task_id)
             WHERE tool_run_attempts.tool_run_id = ?1
               AND tool_run_attempts.attempt_sequence = ?2 AND tasks.worktree_id = ?3",
            params![
                id_bytes(tool_run_id),
                i64::from(attempt.get()),
                id_bytes(worktree_id)
            ],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let run_id = AgentRunId::from_bytes(read_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_id(&row, 1)?);
    let status: String = row
        .get(2)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let started_at = read_timestamp(&row, 3)?;
    let updated_at = read_timestamp(&row, 4)?;
    AgentToolAttempt::new(
        tool_run_id,
        attempt,
        run_id,
        snapshot_id,
        parse_attempt_status(&status)?,
        started_at,
        updated_at,
    )
    .map(Some)
    .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

async fn read_mutation_attempt(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<Option<AgentMutationAttempt>, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT tool_run_attempts.run_id, tool_run_attempts.snapshot_id,
             tool_run_attempts.status, tool_run_attempts.started_at_unix_millis,
             tool_run_attempts.updated_at_unix_millis,
             mutation_attempts.action_fingerprint, mutation_attempts.action_kind,
             mutation_attempts.application_state, mutation_attempts.reconciliation_state,
             mutation_attempts.reconciled_snapshot_id
             FROM mutation_attempts
             JOIN tool_run_attempts USING (tool_run_id, attempt_sequence)
             JOIN agent_runs USING (run_id) JOIN tasks USING (task_id)
             WHERE tool_run_attempts.tool_run_id = ?1
               AND tool_run_attempts.attempt_sequence = ?2 AND tasks.worktree_id = ?3",
            params![
                id_bytes(tool_run_id),
                i64::from(attempt.get()),
                id_bytes(worktree_id)
            ],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let run_id = AgentRunId::from_bytes(read_id(&row, 0)?);
    let snapshot_id = SnapshotId::from_bytes(read_id(&row, 1)?);
    let status: String = row
        .get(2)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let started_at = read_timestamp(&row, 3)?;
    let updated_at = read_timestamp(&row, 4)?;
    let fingerprint = MutationActionFingerprint::from_bytes(read_id(&row, 5)?);
    let kind: String = row
        .get(6)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let application_state: String = row
        .get(7)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let reconciliation_state: String = row
        .get(8)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let reconciled_snapshot_id = read_optional_id(&row, 9)?.map(SnapshotId::from_bytes);
    mutation_attempt(
        tool_run_id,
        attempt,
        run_id,
        snapshot_id,
        parse_attempt_status(&status)?,
        started_at,
        updated_at,
        fingerprint,
        parse_mutation_kind(&kind)?,
        parse_mutation_disposition(
            &application_state,
            &reconciliation_state,
            reconciled_snapshot_id,
        )?,
    )
    .map(Some)
}

fn read_mutation_row(
    row: &libsql::Row,
    run_id: AgentRunId,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    let tool_run_id = ToolRunId::from_bytes(read_id(row, 0)?);
    let attempt = AgentToolAttemptNumber::new(read_u32(row, 1)?)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let snapshot_id = SnapshotId::from_bytes(read_id(row, 2)?);
    let status: String = row
        .get(3)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let fingerprint = MutationActionFingerprint::from_bytes(read_id(row, 6)?);
    let kind: String = row
        .get(7)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let application_state: String = row
        .get(8)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let reconciliation_state: String = row
        .get(9)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let reconciled_snapshot_id = read_optional_id(row, 10)?.map(SnapshotId::from_bytes);
    mutation_attempt(
        tool_run_id,
        attempt,
        run_id,
        snapshot_id,
        parse_attempt_status(&status)?,
        read_timestamp(row, 4)?,
        read_timestamp(row, 5)?,
        fingerprint,
        parse_mutation_kind(&kind)?,
        parse_mutation_disposition(
            &application_state,
            &reconciliation_state,
            reconciled_snapshot_id,
        )?,
    )
}

#[allow(clippy::too_many_arguments)]
fn mutation_attempt(
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    status: AgentToolAttemptStatus,
    started_at: AgentRunTimestamp,
    updated_at: AgentRunTimestamp,
    fingerprint: MutationActionFingerprint,
    kind: AgentMutationKind,
    disposition: AgentMutationDisposition,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    let tool_attempt = AgentToolAttempt::new(
        tool_run_id,
        attempt,
        run_id,
        snapshot_id,
        status,
        started_at,
        updated_at,
    )
    .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    AgentMutationAttempt::new(tool_attempt, fingerprint, kind, disposition)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

async fn mutation_attempt_exists(
    transaction: &Transaction,
    tool_run_id: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<bool, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT EXISTS(SELECT 1 FROM mutation_attempts
             WHERE tool_run_id = ?1 AND attempt_sequence = ?2)",
            params![id_bytes(tool_run_id), i64::from(attempt.get())],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
        .ok_or(AgentRecoveryRepositoryError::InvalidStoredData)?;
    let exists: i64 = row
        .get(0)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    Ok(exists == 1)
}

async fn has_unreconciled_mutation(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<bool, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT EXISTS(
               SELECT 1 FROM mutation_attempts
               JOIN tool_run_attempts USING (tool_run_id, attempt_sequence)
               JOIN agent_runs USING (run_id) JOIN tasks USING (task_id)
               WHERE tasks.worktree_id = ?1 AND mutation_attempts.application_state = 'unknown'
                 AND mutation_attempts.reconciliation_state IN ('required', 'reconciled')
             )",
            [id_bytes(worktree_id)],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
        .ok_or(AgentRecoveryRepositoryError::InvalidStoredData)?;
    let exists: i64 = row
        .get(0)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    Ok(exists == 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationRecoveryGate {
    Clear,
    ReconciliationRequired,
    ReplanRequired,
}

async fn mutation_recovery_gate(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
) -> Result<MutationRecoveryGate, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT mutation_attempts.reconciliation_state
             FROM mutation_attempts
             JOIN tool_run_attempts USING (tool_run_id, attempt_sequence)
             JOIN agent_runs USING (run_id) JOIN tasks USING (task_id)
             WHERE tasks.worktree_id = ?1 AND tool_run_attempts.run_id = ?2
               AND mutation_attempts.application_state = 'unknown'
               AND mutation_attempts.reconciliation_state IN ('required', 'reconciled')
             ORDER BY CASE mutation_attempts.reconciliation_state
               WHEN 'required' THEN 0 ELSE 1 END LIMIT 1",
            params![id_bytes(worktree_id), id_bytes(run_id)],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
    else {
        return Ok(MutationRecoveryGate::Clear);
    };
    let state: String = row
        .get(0)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    match state.as_str() {
        "required" => Ok(MutationRecoveryGate::ReconciliationRequired),
        "reconciled" => Ok(MutationRecoveryGate::ReplanRequired),
        _ => Err(AgentRecoveryRepositoryError::InvalidStoredData),
    }
}

async fn latest_published_snapshot(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<SnapshotId>, AgentRecoveryRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT snapshot_id FROM index_runs
             WHERE worktree_id = ?1 AND status = 'published'
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?;
    rows.next()
        .await
        .map_err(AgentRecoveryRepositoryError::Read)?
        .map(|row| read_id(&row, 0).map(SnapshotId::from_bytes))
        .transpose()
}

pub(crate) fn read_evidence(
    row: &libsql::Row,
) -> Result<AgentToolEvidence, AgentRecoveryRepositoryError> {
    let expected_id = TaskEvidenceId::from_bytes(read_id(row, 0)?);
    let location_kind: String = row
        .get(1)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let path: Vec<u8> = row
        .get(2)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let hash = ContentHash::from_bytes(read_id(row, 3)?);
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(path)
            .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?,
        hash,
    );
    let evidence = match location_kind.as_str() {
        "file" => AgentToolEvidence::for_file(revision),
        "span" => {
            let start_byte = read_usize(row, 4)?;
            let end_byte = read_usize(row, 5)?;
            let start_row = read_u32(row, 6)?;
            let start_column = read_u32(row, 7)?;
            let end_row = read_u32(row, 8)?;
            let end_column = read_u32(row, 9)?;
            AgentToolEvidence::for_span(EvidenceRef::new(
                revision,
                SourceRange::new(
                    start_byte,
                    end_byte,
                    SourcePosition::new(start_row, start_column),
                    SourcePosition::new(end_row, end_column),
                )
                .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?,
            ))
        }
        _ => return Err(AgentRecoveryRepositoryError::InvalidStoredData),
    };
    if evidence.id() != expected_id {
        return Err(AgentRecoveryRepositoryError::InvalidStoredData);
    }
    Ok(evidence)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], AgentRecoveryRepositoryError> {
    let value: Vec<u8> = row
        .get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    value
        .try_into()
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, AgentRecoveryRepositoryError> {
    let value: Option<Vec<u8>> = row
        .get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    value
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, AgentRecoveryRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    u32::try_from(value).map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

fn read_usize(row: &libsql::Row, index: i32) -> Result<usize, AgentRecoveryRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    usize::try_from(value).map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<AgentRunTimestamp, AgentRecoveryRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    let value =
        u64::try_from(value).map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(value)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}

fn id_bytes<T>(id: T) -> Vec<u8>
where
    T: RecoveryId,
{
    id.recovery_bytes().to_vec()
}

trait RecoveryId {
    fn recovery_bytes(&self) -> &[u8; 32];
}

macro_rules! recovery_id {
    ($($type:ty),+ $(,)?) => {
        $(impl RecoveryId for $type {
            fn recovery_bytes(&self) -> &[u8; 32] {
                self.as_bytes()
            }
        })+
    };
}

recovery_id!(AgentRunId, SnapshotId, ToolRunId, WorktreeId);

fn timestamp_to_i64(timestamp: AgentRunTimestamp) -> i64 {
    timestamp.unix_millis() as i64
}

const fn attempt_status_text(status: AgentToolAttemptStatus) -> &'static str {
    match status {
        AgentToolAttemptStatus::InFlight => "in_flight",
        AgentToolAttemptStatus::Succeeded => "succeeded",
        AgentToolAttemptStatus::Failed => "failed",
        AgentToolAttemptStatus::Cancelled => "cancelled",
        AgentToolAttemptStatus::Denied => "denied",
        AgentToolAttemptStatus::Interrupted => "interrupted",
    }
}

fn parse_attempt_status(
    value: &str,
) -> Result<AgentToolAttemptStatus, AgentRecoveryRepositoryError> {
    match value {
        "in_flight" => Ok(AgentToolAttemptStatus::InFlight),
        "succeeded" => Ok(AgentToolAttemptStatus::Succeeded),
        "failed" => Ok(AgentToolAttemptStatus::Failed),
        "cancelled" => Ok(AgentToolAttemptStatus::Cancelled),
        "denied" => Ok(AgentToolAttemptStatus::Denied),
        "interrupted" => Ok(AgentToolAttemptStatus::Interrupted),
        _ => Err(AgentRecoveryRepositoryError::InvalidStoredData),
    }
}

const fn mutation_kind_text(kind: AgentMutationKind) -> &'static str {
    match kind {
        AgentMutationKind::Patch => "patch",
        AgentMutationKind::Process => "process",
        AgentMutationKind::UnclassifiedLegacy => "unclassified_legacy",
    }
}

fn parse_mutation_kind(value: &str) -> Result<AgentMutationKind, AgentRecoveryRepositoryError> {
    match value {
        "patch" => Ok(AgentMutationKind::Patch),
        "process" => Ok(AgentMutationKind::Process),
        "unclassified_legacy" => Ok(AgentMutationKind::UnclassifiedLegacy),
        _ => Err(AgentRecoveryRepositoryError::InvalidStoredData),
    }
}

fn disposition_text(
    disposition: AgentMutationDisposition,
) -> Result<(&'static str, &'static str), AgentRecoveryRepositoryError> {
    match disposition {
        AgentMutationDisposition::Applied => Ok(("applied", "not_required")),
        AgentMutationDisposition::NotApplied => Ok(("not_applied", "not_required")),
        AgentMutationDisposition::Unknown(MutationReconciliation::Required) => {
            Ok(("unknown", "required"))
        }
        AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled { .. }) => {
            Err(AgentRecoveryRepositoryError::InvalidInput)
        }
        AgentMutationDisposition::Unknown(MutationReconciliation::Replanned { .. }) => {
            Err(AgentRecoveryRepositoryError::InvalidInput)
        }
    }
}

fn parse_mutation_disposition(
    application_state: &str,
    reconciliation_state: &str,
    reconciled_snapshot_id: Option<SnapshotId>,
) -> Result<AgentMutationDisposition, AgentRecoveryRepositoryError> {
    match (
        application_state,
        reconciliation_state,
        reconciled_snapshot_id,
    ) {
        ("applied", "not_required", None) => Ok(AgentMutationDisposition::Applied),
        ("not_applied", "not_required", None) => Ok(AgentMutationDisposition::NotApplied),
        ("unknown", "required", None) => Ok(AgentMutationDisposition::Unknown(
            MutationReconciliation::Required,
        )),
        ("unknown", "reconciled", Some(snapshot_id)) => Ok(AgentMutationDisposition::Unknown(
            MutationReconciliation::Reconciled { snapshot_id },
        )),
        ("unknown", "replanned", Some(snapshot_id)) => Ok(AgentMutationDisposition::Unknown(
            MutationReconciliation::Replanned { snapshot_id },
        )),
        _ => Err(AgentRecoveryRepositoryError::InvalidStoredData),
    }
}

async fn close_write_transaction<T>(
    transaction: Transaction,
    result: Result<T, AgentRecoveryRepositoryError>,
) -> Result<T, AgentRecoveryRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(AgentRecoveryRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(AgentRecoveryRepositoryError::Rollback(source)),
        },
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, AgentRecoveryRepositoryError>,
) -> Result<T, AgentRecoveryRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(AgentRecoveryRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(AgentRecoveryRepositoryError::Rollback(source)),
        },
    }
}

fn classify_attempt_constraint(source: libsql::Error) -> AgentRecoveryRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        AgentRecoveryRepositoryError::ToolAttemptConflict
    } else {
        AgentRecoveryRepositoryError::Write(source)
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

#[derive(Debug)]
pub(crate) enum AgentRecoveryRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    TaskLedger(task_ledger_repository::TaskLedgerRepositoryError),
    RunJournal(run_journal_repository::RunJournalRepositoryError),
    InvalidInput,
    InvalidStoredData,
    RunNotFound,
    ToolAttemptConflict,
    MutationReconciliationRequired,
    PublishedSnapshotConflict,
    ResourceLimit,
}

impl AgentRecoveryRepositoryError {
    pub(crate) fn classify(&self) -> AgentRecoveryStoreFailure {
        match self {
            Self::InvalidInput | Self::InvalidStoredData => {
                AgentRecoveryStoreFailure::InvalidStoredData
            }
            Self::RunNotFound => AgentRecoveryStoreFailure::RunNotFound,
            Self::ToolAttemptConflict => AgentRecoveryStoreFailure::ToolAttemptConflict,
            Self::MutationReconciliationRequired => {
                AgentRecoveryStoreFailure::MutationReconciliationRequired
            }
            Self::PublishedSnapshotConflict => AgentRecoveryStoreFailure::PublishedSnapshotConflict,
            Self::ResourceLimit => AgentRecoveryStoreFailure::ResourceLimit,
            Self::TaskLedger(error) => match error.classify() {
                TaskLedgerStoreFailure::Unavailable => AgentRecoveryStoreFailure::Unavailable,
                TaskLedgerStoreFailure::Corrupt => AgentRecoveryStoreFailure::Corrupt,
                TaskLedgerStoreFailure::UnsupportedSchema => {
                    AgentRecoveryStoreFailure::UnsupportedSchema
                }
                TaskLedgerStoreFailure::VersionConflict => {
                    AgentRecoveryStoreFailure::LedgerVersionConflict
                }
                TaskLedgerStoreFailure::InvalidStoredData
                | TaskLedgerStoreFailure::LedgerAlreadyExists
                | TaskLedgerStoreFailure::TaskNotFound => {
                    AgentRecoveryStoreFailure::InvalidStoredData
                }
            },
            Self::RunJournal(error) => match error.classify() {
                a3_application::RunJournalStoreFailure::Unavailable => {
                    AgentRecoveryStoreFailure::Unavailable
                }
                a3_application::RunJournalStoreFailure::Corrupt => {
                    AgentRecoveryStoreFailure::Corrupt
                }
                a3_application::RunJournalStoreFailure::UnsupportedSchema => {
                    AgentRecoveryStoreFailure::UnsupportedSchema
                }
                a3_application::RunJournalStoreFailure::RunNotFound => {
                    AgentRecoveryStoreFailure::RunNotFound
                }
                a3_application::RunJournalStoreFailure::SequenceConflict => {
                    AgentRecoveryStoreFailure::RunSequenceConflict
                }
                a3_application::RunJournalStoreFailure::InvalidStoredData
                | a3_application::RunJournalStoreFailure::RunAlreadyExists => {
                    AgentRecoveryStoreFailure::InvalidStoredData
                }
            },
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    AgentRecoveryStoreFailure::Corrupt
                } else {
                    AgentRecoveryStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for AgentRecoveryRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "agent-recovery transaction could not begin",
            Self::Read(_) => "agent-recovery data could not be read",
            Self::Write(_) => "agent-recovery data could not be written",
            Self::Commit(_) => "agent-recovery transaction could not commit",
            Self::Rollback(_) => "agent-recovery transaction could not roll back",
            Self::TaskLedger(_) => "agent-recovery Task Ledger could not be persisted",
            Self::RunJournal(_) => "agent-recovery run journal could not be persisted",
            Self::InvalidInput => "agent-recovery input was invalid",
            Self::InvalidStoredData => "agent-recovery data was invalid",
            Self::RunNotFound => "agent-recovery run was not found",
            Self::ToolAttemptConflict => "agent-recovery tool attempt conflicted",
            Self::MutationReconciliationRequired => {
                "agent-recovery mutation requires reconciliation"
            }
            Self::PublishedSnapshotConflict => "agent-recovery published snapshot conflicted",
            Self::ResourceLimit => "agent-recovery data exceeded a resource limit",
        })
    }
}

impl Error for AgentRecoveryRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            Self::TaskLedger(error) => Some(error),
            Self::RunJournal(error) => Some(error),
            Self::InvalidInput
            | Self::InvalidStoredData
            | Self::RunNotFound
            | Self::ToolAttemptConflict
            | Self::MutationReconciliationRequired
            | Self::PublishedSnapshotConflict
            | Self::ResourceLimit => None,
        }
    }
}
