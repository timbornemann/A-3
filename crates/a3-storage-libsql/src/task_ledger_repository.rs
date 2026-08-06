use crate::{catalog::is_corruption, goal_contract_repository};
use a3_application::{StoredTaskLedger, TaskLedgerStoreFailure, TaskLedgerStoreVersion};
use a3_domain::{
    AgentRunId, ExpectedTaskEvidence, GoalContractReference, GoalContractRevision, StepDependency,
    StepVerification, StepVerificationId, StepVerificationOutcome, TaskEvidenceId, TaskId,
    TaskLedger, TaskLedgerReplan, TaskLedgerRevision, TaskLedgerTimestamp, TaskReplanReason,
    TaskStep, TaskStepAttempt, TaskStepAttemptDetails, TaskStepAttemptNumber,
    TaskStepAttemptOutcome, TaskStepAttemptTiming, TaskStepBlockingReason,
    TaskStepCancellationReason, TaskStepDefinition, TaskStepFailureReason, TaskStepId,
    TaskStepMaterializedState, TaskStepOutcome, TaskStepRationale, TaskStepResultSummary,
    TaskStepStaleCause, TaskStepStatus, VerificationFailureSummary, VerificationMethod,
    VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn create(
    connection: &Connection,
    worktree_id: WorktreeId,
    ledger: &TaskLedger,
) -> Result<TaskLedgerStoreVersion, TaskLedgerRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(TaskLedgerRepositoryError::Begin)?;
    let result = async {
        ensure_goal_exists(&transaction, worktree_id, ledger.goal_contract()).await?;
        if read_header(&transaction, worktree_id, ledger.task_id())
            .await?
            .is_some()
        {
            return Err(TaskLedgerRepositoryError::LedgerAlreadyExists);
        }
        let version = TaskLedgerStoreVersion::INITIAL;
        transaction
            .execute(
                "INSERT INTO task_ledgers (
                 task_id, goal_revision, plan_revision, store_version,
                 created_at_unix_millis, updated_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id_bytes(ledger.task_id()),
                    i64::from(ledger.goal_contract().revision().get()),
                    i64::from(ledger.revision().get()),
                    version_to_i64(version)?,
                    timestamp_to_i64(ledger.created_at()),
                    timestamp_to_i64(ledger.updated_at())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        write_projection(&transaction, ledger).await?;
        Ok(version)
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn replace(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_version: TaskLedgerStoreVersion,
    ledger: &TaskLedger,
) -> Result<TaskLedgerStoreVersion, TaskLedgerRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(TaskLedgerRepositoryError::Begin)?;
    let result = replace_in_transaction(&transaction, worktree_id, expected_version, ledger).await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn replace_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    expected_version: TaskLedgerStoreVersion,
    ledger: &TaskLedger,
) -> Result<TaskLedgerStoreVersion, TaskLedgerRepositoryError> {
    let existing = load_from_transaction(transaction, worktree_id, ledger.task_id())
        .await?
        .ok_or(TaskLedgerRepositoryError::TaskNotFound)?;
    if existing.version() != expected_version {
        return Err(TaskLedgerRepositoryError::VersionConflict);
    }
    validate_successor(existing.ledger(), ledger)?;
    ensure_goal_exists(transaction, worktree_id, ledger.goal_contract()).await?;
    let next_version = next_store_version(expected_version)?;
    let changed = transaction
        .execute(
            "UPDATE task_ledgers SET goal_revision = ?1, plan_revision = ?2,
             store_version = ?3, updated_at_unix_millis = ?4
             WHERE task_id = ?5 AND store_version = ?6",
            params![
                i64::from(ledger.goal_contract().revision().get()),
                i64::from(ledger.revision().get()),
                version_to_i64(next_version)?,
                timestamp_to_i64(ledger.updated_at()),
                id_bytes(ledger.task_id()),
                version_to_i64(expected_version)?
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    if changed != 1 {
        return Err(TaskLedgerRepositoryError::VersionConflict);
    }
    transaction
        .execute(
            "DELETE FROM task_ledger_replans WHERE task_id = ?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Write)?;
    transaction
        .execute(
            "DELETE FROM task_step_dependencies WHERE task_id = ?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Write)?;
    transaction
        .execute(
            "DELETE FROM task_step_attempts WHERE task_id = ?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Write)?;
    transaction
        .execute(
            "UPDATE task_steps SET parent_step_id = NULL WHERE task_id = ?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Write)?;
    transaction
        .execute(
            "DELETE FROM task_steps WHERE task_id = ?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Write)?;
    write_projection(transaction, ledger).await?;
    Ok(next_version)
}

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<StoredTaskLedger>, TaskLedgerRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(TaskLedgerRepositoryError::Begin)?;
    let result = load_from_transaction(&transaction, worktree_id, task_id).await;
    close_read_transaction(transaction, result).await
}

async fn write_projection(
    transaction: &Transaction,
    ledger: &TaskLedger,
) -> Result<(), TaskLedgerRepositoryError> {
    for step in ledger.steps() {
        write_step(transaction, ledger.task_id(), step).await?;
    }
    for replan in ledger.replans() {
        write_replan(transaction, ledger.task_id(), replan).await?;
    }
    Ok(())
}

async fn write_step(
    transaction: &Transaction,
    task_id: TaskId,
    step: &TaskStep,
) -> Result<(), TaskLedgerRepositoryError> {
    let definition = step.definition();
    let (stale_kind, stale_dependency) = match step.stale_cause() {
        Some(TaskStepStaleCause::VerificationEvidence(_)) => (Some("verification_evidence"), None),
        Some(TaskStepStaleCause::Dependency(step_id)) => {
            (Some("dependency"), Some(id_bytes(*step_id)))
        }
        None => (None, None),
    };
    transaction
        .execute(
            "INSERT INTO task_steps (
             task_id, step_id, parent_step_id, intended_outcome, rationale,
             verification_spec_id, verification_method, verification_requirement,
             introduced_plan_revision, retired_plan_revision, status, blocking_reason,
             stale_kind, stale_dependency_step_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id_bytes(task_id),
                id_bytes(definition.id()),
                definition.parent_step_id().map(id_bytes),
                definition.intended_outcome().as_str(),
                definition.rationale().as_str(),
                id_bytes(definition.verification_spec().id()),
                verification_method_text(definition.verification_spec().method()),
                definition.verification_spec().requirement().as_str(),
                i64::from(step.introduced_in_revision().get()),
                step.retired_in_revision()
                    .map(|value| i64::from(value.get())),
                step_status_text(step.status()),
                step.blocking_reason().map(TaskStepBlockingReason::as_str),
                stale_kind,
                stale_dependency
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;

    for (index, dependency) in definition.dependencies().iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO task_step_dependencies (
                 task_id, step_id, item_sequence, prerequisite_step_id
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id_bytes(task_id),
                    id_bytes(definition.id()),
                    sequence_to_i64(index)?,
                    id_bytes(dependency.prerequisite())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    for (index, evidence) in definition.expected_evidence().iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO task_step_expected_evidence (
                 task_id, step_id, item_sequence, description
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id_bytes(task_id),
                    id_bytes(definition.id()),
                    sequence_to_i64(index)?,
                    evidence.as_str()
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    for attempt in step.attempts() {
        write_attempt(transaction, task_id, definition.id(), attempt).await?;
    }
    if let Some(TaskStepStaleCause::VerificationEvidence(evidence_ids)) = step.stale_cause() {
        write_evidence_rows(
            transaction,
            "task_step_stale_evidence",
            task_id,
            definition.id(),
            None,
            evidence_ids,
        )
        .await?;
    }
    Ok(())
}

async fn write_attempt(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
    attempt: &TaskStepAttempt,
) -> Result<(), TaskLedgerRepositoryError> {
    let (outcome, reason) = attempt_outcome_text(attempt.outcome());
    transaction
        .execute(
            "INSERT INTO task_step_attempts (
             task_id, step_id, attempt_number, run_id, started_at_unix_millis,
             finished_at_unix_millis, outcome, outcome_reason, result_summary
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id_bytes(task_id),
                id_bytes(step_id),
                i64::from(attempt.number().get()),
                id_bytes(attempt.run_id()),
                timestamp_to_i64(attempt.started_at()),
                attempt.finished_at().map(timestamp_to_i64),
                outcome,
                reason,
                attempt.result_summary().map(TaskStepResultSummary::as_str)
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    write_evidence_rows(
        transaction,
        "task_step_attempt_evidence",
        task_id,
        step_id,
        Some(attempt.number()),
        attempt.evidence_ids(),
    )
    .await?;
    if let Some(verification) = attempt.verification() {
        let (outcome, failure_summary) = match verification.outcome() {
            StepVerificationOutcome::Passed => ("passed", None),
            StepVerificationOutcome::Failed { summary } => ("failed", Some(summary.as_str())),
        };
        transaction
            .execute(
                "INSERT INTO task_step_verifications (
                 task_id, step_id, attempt_number, verification_id, verification_spec_id,
                 run_id, outcome, failure_summary, verified_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id_bytes(task_id),
                    id_bytes(step_id),
                    i64::from(attempt.number().get()),
                    id_bytes(verification.id()),
                    id_bytes(verification.spec_id()),
                    id_bytes(verification.run_id()),
                    outcome,
                    failure_summary,
                    timestamp_to_i64(verification.verified_at())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        write_evidence_rows(
            transaction,
            "task_step_verification_evidence",
            task_id,
            step_id,
            Some(attempt.number()),
            verification.evidence_ids(),
        )
        .await?;
    }
    Ok(())
}

async fn write_evidence_rows(
    transaction: &Transaction,
    table: &str,
    task_id: TaskId,
    step_id: TaskStepId,
    attempt_number: Option<TaskStepAttemptNumber>,
    evidence_ids: &[TaskEvidenceId],
) -> Result<(), TaskLedgerRepositoryError> {
    let sql = if attempt_number.is_some() {
        format!(
            "INSERT INTO {table} (task_id, step_id, attempt_number, item_sequence, evidence_id)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)"
        )
    } else {
        format!(
            "INSERT INTO {table} (task_id, step_id, item_sequence, evidence_id)\n\
             VALUES (?1, ?2, ?3, ?4)"
        )
    };
    for (index, evidence_id) in evidence_ids.iter().enumerate() {
        let result = if let Some(number) = attempt_number {
            transaction
                .execute(
                    &sql,
                    params![
                        id_bytes(task_id),
                        id_bytes(step_id),
                        i64::from(number.get()),
                        sequence_to_i64(index)?,
                        id_bytes(*evidence_id)
                    ],
                )
                .await
        } else {
            transaction
                .execute(
                    &sql,
                    params![
                        id_bytes(task_id),
                        id_bytes(step_id),
                        sequence_to_i64(index)?,
                        id_bytes(*evidence_id)
                    ],
                )
                .await
        };
        result.map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}

async fn write_replan(
    transaction: &Transaction,
    task_id: TaskId,
    replan: &TaskLedgerReplan,
) -> Result<(), TaskLedgerRepositoryError> {
    transaction
        .execute(
            "INSERT INTO task_ledger_replans (
             task_id, plan_revision, previous_plan_revision, reason, created_at_unix_millis
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id_bytes(task_id),
                i64::from(replan.revision().get()),
                i64::from(replan.previous_revision().get()),
                replan.reason().as_str(),
                timestamp_to_i64(replan.created_at())
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    write_replan_step_ids(
        transaction,
        "task_ledger_replan_retirements",
        task_id,
        replan,
        replan.retired_step_ids(),
    )
    .await?;
    write_replan_step_ids(
        transaction,
        "task_ledger_replan_additions",
        task_id,
        replan,
        replan.added_step_ids(),
    )
    .await
}

async fn write_replan_step_ids(
    transaction: &Transaction,
    table: &str,
    task_id: TaskId,
    replan: &TaskLedgerReplan,
    step_ids: &[TaskStepId],
) -> Result<(), TaskLedgerRepositoryError> {
    let sql = format!("INSERT INTO {table} (task_id, plan_revision, step_id) VALUES (?1, ?2, ?3)");
    for step_id in step_ids {
        transaction
            .execute(
                &sql,
                params![
                    id_bytes(task_id),
                    i64::from(replan.revision().get()),
                    id_bytes(*step_id)
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct LedgerHeader {
    goal_revision: GoalContractRevision,
    plan_revision: TaskLedgerRevision,
    store_version: TaskLedgerStoreVersion,
    created_at: TaskLedgerTimestamp,
    updated_at: TaskLedgerTimestamp,
}

pub(crate) async fn load_from_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<StoredTaskLedger>, TaskLedgerRepositoryError> {
    let Some(header) = read_header(transaction, worktree_id, task_id).await? else {
        return Ok(None);
    };
    let steps = read_steps(transaction, task_id).await?;
    let replans = read_replans(transaction, task_id).await?;
    let goal_contract = goal_contract_repository::load_revision_from_transaction(
        transaction,
        worktree_id,
        task_id,
        header.goal_revision,
    )
    .await
    .map_err(TaskLedgerRepositoryError::GoalContract)?
    .ok_or(TaskLedgerRepositoryError::InvalidStoredData)?
    .reference();
    let ledger = TaskLedger::reconstruct(
        goal_contract,
        header.plan_revision,
        steps,
        replans,
        header.created_at,
        header.updated_at,
    )
    .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    Ok(Some(StoredTaskLedger::new(ledger, header.store_version)))
}

async fn read_header(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<LedgerHeader>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT l.goal_revision, l.plan_revision, l.store_version,
             l.created_at_unix_millis, l.updated_at_unix_millis
             FROM task_ledgers l JOIN tasks t ON t.task_id = l.task_id
             WHERE l.task_id = ?1 AND t.worktree_id = ?2",
            params![id_bytes(task_id), id_bytes(worktree_id)],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let header = rows
        .next()
        .await
        .map_err(TaskLedgerRepositoryError::Read)?
        .map(|row| {
            Ok(LedgerHeader {
                goal_revision: read_goal_revision(&row, 0)?,
                plan_revision: read_ledger_revision(&row, 1)?,
                store_version: read_store_version(&row, 2)?,
                created_at: read_timestamp(&row, 3)?,
                updated_at: read_timestamp(&row, 4)?,
            })
        })
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(TaskLedgerRepositoryError::Read)?
        .is_some()
    {
        return Err(TaskLedgerRepositoryError::InvalidStoredData);
    }
    Ok(header)
}

async fn read_steps(
    transaction: &Transaction,
    task_id: TaskId,
) -> Result<Vec<TaskStep>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT step_id, parent_step_id, intended_outcome, rationale,
             verification_spec_id, verification_method, verification_requirement,
             introduced_plan_revision, retired_plan_revision, status, blocking_reason,
             stale_kind, stale_dependency_step_id
             FROM task_steps WHERE task_id = ?1 ORDER BY step_id",
            params![id_bytes(task_id)],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        records.push(StepRecord {
            step_id: TaskStepId::from_bytes(read_id(&row, 0)?),
            parent_step_id: read_optional_id(&row, 1)?.map(TaskStepId::from_bytes),
            intended_outcome: read_text(&row, 2)?,
            rationale: read_text(&row, 3)?,
            verification_spec_id: VerificationSpecId::from_bytes(read_id(&row, 4)?),
            verification_method: read_text(&row, 5)?,
            verification_requirement: read_text(&row, 6)?,
            introduced_revision: read_ledger_revision(&row, 7)?,
            retired_revision: read_optional_ledger_revision(&row, 8)?,
            status: read_text(&row, 9)?,
            blocking_reason: read_optional_text(&row, 10)?,
            stale_kind: read_optional_text(&row, 11)?,
            stale_dependency: read_optional_id(&row, 12)?.map(TaskStepId::from_bytes),
        });
    }
    let mut steps = Vec::with_capacity(records.len());
    for record in records {
        steps.push(read_step(transaction, task_id, record).await?);
    }
    Ok(steps)
}

struct StepRecord {
    step_id: TaskStepId,
    parent_step_id: Option<TaskStepId>,
    intended_outcome: String,
    rationale: String,
    verification_spec_id: VerificationSpecId,
    verification_method: String,
    verification_requirement: String,
    introduced_revision: TaskLedgerRevision,
    retired_revision: Option<TaskLedgerRevision>,
    status: String,
    blocking_reason: Option<String>,
    stale_kind: Option<String>,
    stale_dependency: Option<TaskStepId>,
}

async fn read_step(
    transaction: &Transaction,
    task_id: TaskId,
    record: StepRecord,
) -> Result<TaskStep, TaskLedgerRepositoryError> {
    let dependencies = read_dependencies(transaction, task_id, record.step_id).await?;
    let expected_evidence = read_expected_evidence(transaction, task_id, record.step_id).await?;
    let attempts = read_attempts(transaction, task_id, record.step_id).await?;
    let verification_spec = VerificationSpec::new(
        record.verification_spec_id,
        parse_verification_method(&record.verification_method)?,
        VerificationRequirement::try_from_string(record.verification_requirement)
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
    );
    let definition = TaskStepDefinition::new(
        record.step_id,
        record.parent_step_id,
        TaskStepOutcome::try_from_string(record.intended_outcome)
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
        TaskStepRationale::try_from_string(record.rationale)
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
        dependencies,
        expected_evidence,
        verification_spec,
    )
    .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    let status = parse_step_status(&record.status)?;
    let blocking_reason = record
        .blocking_reason
        .map(TaskStepBlockingReason::try_from_string)
        .transpose()
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    let stale_cause = read_stale_cause(
        transaction,
        task_id,
        record.step_id,
        record.stale_kind.as_deref(),
        record.stale_dependency,
    )
    .await?;
    TaskStep::reconstruct(
        definition,
        record.introduced_revision,
        TaskStepMaterializedState::new(
            status,
            blocking_reason,
            stale_cause,
            record.retired_revision,
        ),
        attempts,
    )
    .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

async fn read_dependencies(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
) -> Result<Vec<StepDependency>, TaskLedgerRepositoryError> {
    let ids = read_ordered_ids(
        transaction,
        "task_step_dependencies",
        "prerequisite_step_id",
        task_id,
        step_id,
        None,
    )
    .await?;
    Ok(ids
        .into_iter()
        .map(TaskStepId::from_bytes)
        .map(StepDependency::new)
        .collect())
}

async fn read_expected_evidence(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
) -> Result<Vec<ExpectedTaskEvidence>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT item_sequence, description FROM task_step_expected_evidence
             WHERE task_id = ?1 AND step_id = ?2 ORDER BY item_sequence",
            params![id_bytes(task_id), id_bytes(step_id)],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let mut values = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        validate_sequence(&row, values.len())?;
        values.push(
            ExpectedTaskEvidence::try_from_string(read_text(&row, 1)?)
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
        );
    }
    Ok(values)
}

async fn read_attempts(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
) -> Result<Vec<TaskStepAttempt>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT attempt_number, run_id, started_at_unix_millis,
             finished_at_unix_millis, outcome, outcome_reason, result_summary
             FROM task_step_attempts WHERE task_id = ?1 AND step_id = ?2
             ORDER BY attempt_number",
            params![id_bytes(task_id), id_bytes(step_id)],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        records.push(AttemptRecord {
            number: read_attempt_number(&row, 0)?,
            run_id: AgentRunId::from_bytes(read_id(&row, 1)?),
            started_at: read_timestamp(&row, 2)?,
            finished_at: read_optional_timestamp(&row, 3)?,
            outcome: read_text(&row, 4)?,
            reason: read_optional_text(&row, 5)?,
            result_summary: read_optional_text(&row, 6)?,
        });
    }
    let mut attempts = Vec::with_capacity(records.len());
    for record in records {
        attempts.push(read_attempt(transaction, task_id, step_id, record).await?);
    }
    Ok(attempts)
}

struct AttemptRecord {
    number: TaskStepAttemptNumber,
    run_id: AgentRunId,
    started_at: TaskLedgerTimestamp,
    finished_at: Option<TaskLedgerTimestamp>,
    outcome: String,
    reason: Option<String>,
    result_summary: Option<String>,
}

async fn read_attempt(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
    record: AttemptRecord,
) -> Result<TaskStepAttempt, TaskLedgerRepositoryError> {
    let outcome = parse_attempt_outcome(&record.outcome, record.reason)?;
    let result_summary = record
        .result_summary
        .map(TaskStepResultSummary::try_from_string)
        .transpose()
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    let evidence_ids = read_evidence_ids(
        transaction,
        "task_step_attempt_evidence",
        task_id,
        step_id,
        Some(record.number),
    )
    .await?;
    let verification = read_verification(transaction, task_id, step_id, record.number).await?;
    TaskStepAttempt::reconstruct(
        record.number,
        record.run_id,
        TaskStepAttemptTiming::new(record.started_at, record.finished_at),
        TaskStepAttemptDetails::new(outcome, result_summary, evidence_ids, verification),
    )
    .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

async fn read_verification(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
    attempt_number: TaskStepAttemptNumber,
) -> Result<Option<StepVerification>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT verification_id, verification_spec_id, run_id, outcome,
             failure_summary, verified_at_unix_millis
             FROM task_step_verifications
             WHERE task_id = ?1 AND step_id = ?2 AND attempt_number = ?3",
            params![
                id_bytes(task_id),
                id_bytes(step_id),
                i64::from(attempt_number.get())
            ],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? else {
        return Ok(None);
    };
    let verification_id = StepVerificationId::from_bytes(read_id(&row, 0)?);
    let spec_id = VerificationSpecId::from_bytes(read_id(&row, 1)?);
    let run_id = AgentRunId::from_bytes(read_id(&row, 2)?);
    let outcome_text = read_text(&row, 3)?;
    let failure_summary = read_optional_text(&row, 4)?;
    let verified_at = read_timestamp(&row, 5)?;
    if rows
        .next()
        .await
        .map_err(TaskLedgerRepositoryError::Read)?
        .is_some()
    {
        return Err(TaskLedgerRepositoryError::InvalidStoredData);
    }
    let outcome = parse_verification_outcome(&outcome_text, failure_summary)?;
    let evidence_ids = read_evidence_ids(
        transaction,
        "task_step_verification_evidence",
        task_id,
        step_id,
        Some(attempt_number),
    )
    .await?;
    StepVerification::new(
        verification_id,
        spec_id,
        run_id,
        outcome,
        evidence_ids,
        verified_at,
    )
    .map(Some)
    .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

async fn read_stale_cause(
    transaction: &Transaction,
    task_id: TaskId,
    step_id: TaskStepId,
    kind: Option<&str>,
    dependency: Option<TaskStepId>,
) -> Result<Option<TaskStepStaleCause>, TaskLedgerRepositoryError> {
    match (kind, dependency) {
        (None, None) => Ok(None),
        (Some("dependency"), Some(step_id)) => Ok(Some(TaskStepStaleCause::Dependency(step_id))),
        (Some("verification_evidence"), None) => read_evidence_ids(
            transaction,
            "task_step_stale_evidence",
            task_id,
            step_id,
            None,
        )
        .await
        .map(TaskStepStaleCause::VerificationEvidence)
        .map(Some),
        _ => Err(TaskLedgerRepositoryError::InvalidStoredData),
    }
}

async fn read_evidence_ids(
    transaction: &Transaction,
    table: &str,
    task_id: TaskId,
    step_id: TaskStepId,
    attempt_number: Option<TaskStepAttemptNumber>,
) -> Result<Vec<TaskEvidenceId>, TaskLedgerRepositoryError> {
    read_ordered_ids(
        transaction,
        table,
        "evidence_id",
        task_id,
        step_id,
        attempt_number,
    )
    .await
    .map(|ids| ids.into_iter().map(TaskEvidenceId::from_bytes).collect())
}

async fn read_ordered_ids(
    transaction: &Transaction,
    table: &str,
    column: &str,
    task_id: TaskId,
    step_id: TaskStepId,
    attempt_number: Option<TaskStepAttemptNumber>,
) -> Result<Vec<[u8; 32]>, TaskLedgerRepositoryError> {
    let (sql, attempt) = if let Some(number) = attempt_number {
        (
            format!(
                "SELECT item_sequence, {column} FROM {table}\n\
                 WHERE task_id = ?1 AND step_id = ?2 AND attempt_number = ?3\n\
                 ORDER BY item_sequence"
            ),
            Some(i64::from(number.get())),
        )
    } else {
        (
            format!(
                "SELECT item_sequence, {column} FROM {table}\n\
                 WHERE task_id = ?1 AND step_id = ?2 ORDER BY item_sequence"
            ),
            None,
        )
    };
    let mut rows = if let Some(number) = attempt {
        transaction
            .query(&sql, params![id_bytes(task_id), id_bytes(step_id), number])
            .await
    } else {
        transaction
            .query(&sql, params![id_bytes(task_id), id_bytes(step_id)])
            .await
    }
    .map_err(TaskLedgerRepositoryError::Read)?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        validate_sequence(&row, ids.len())?;
        ids.push(read_id(&row, 1)?);
    }
    Ok(ids)
}

async fn read_replans(
    transaction: &Transaction,
    task_id: TaskId,
) -> Result<Vec<TaskLedgerReplan>, TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT plan_revision, previous_plan_revision, reason, created_at_unix_millis
             FROM task_ledger_replans WHERE task_id = ?1 ORDER BY plan_revision",
            params![id_bytes(task_id)],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        records.push((
            read_ledger_revision(&row, 0)?,
            read_ledger_revision(&row, 1)?,
            TaskReplanReason::try_from_string(read_text(&row, 2)?)
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
            read_timestamp(&row, 3)?,
        ));
    }
    let mut replans = Vec::with_capacity(records.len());
    for (revision, previous, reason, created_at) in records {
        let retired = read_replan_step_ids(
            transaction,
            "task_ledger_replan_retirements",
            task_id,
            revision,
        )
        .await?;
        let added = read_replan_step_ids(
            transaction,
            "task_ledger_replan_additions",
            task_id,
            revision,
        )
        .await?;
        replans.push(
            TaskLedgerReplan::reconstruct(revision, previous, reason, retired, added, created_at)
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?,
        );
    }
    Ok(replans)
}

async fn read_replan_step_ids(
    transaction: &Transaction,
    table: &str,
    task_id: TaskId,
    revision: TaskLedgerRevision,
) -> Result<Vec<TaskStepId>, TaskLedgerRepositoryError> {
    let sql = format!(
        "SELECT step_id FROM {table} WHERE task_id = ?1 AND plan_revision = ?2 ORDER BY step_id"
    );
    let mut rows = transaction
        .query(&sql, params![id_bytes(task_id), i64::from(revision.get())])
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await.map_err(TaskLedgerRepositoryError::Read)? {
        ids.push(TaskStepId::from_bytes(read_id(&row, 0)?));
    }
    Ok(ids)
}

async fn ensure_goal_exists(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    goal: GoalContractReference,
) -> Result<(), TaskLedgerRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT 1 FROM tasks t JOIN goal_contract_revisions g ON g.task_id = t.task_id
             WHERE t.task_id = ?1 AND t.worktree_id = ?2 AND g.revision = ?3",
            params![
                id_bytes(goal.task_id()),
                id_bytes(worktree_id),
                i64::from(goal.revision().get())
            ],
        )
        .await
        .map_err(TaskLedgerRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(TaskLedgerRepositoryError::Read)?
        .is_none()
    {
        return Err(TaskLedgerRepositoryError::TaskNotFound);
    }
    if rows
        .next()
        .await
        .map_err(TaskLedgerRepositoryError::Read)?
        .is_some()
    {
        return Err(TaskLedgerRepositoryError::InvalidStoredData);
    }
    Ok(())
}

fn validate_successor(
    existing: &TaskLedger,
    candidate: &TaskLedger,
) -> Result<(), TaskLedgerRepositoryError> {
    if existing.task_id() != candidate.task_id()
        || existing.goal_contract() != candidate.goal_contract()
        || existing.created_at() != candidate.created_at()
        || candidate.updated_at() < existing.updated_at()
        || candidate.revision().get() < existing.revision().get()
        || !candidate.replans().starts_with(existing.replans())
    {
        return Err(TaskLedgerRepositoryError::InvalidInput);
    }
    for existing_step in existing.steps() {
        let Some(candidate_step) = candidate.step(existing_step.definition().id()) else {
            return Err(TaskLedgerRepositoryError::InvalidInput);
        };
        if existing_step.definition() != candidate_step.definition()
            || existing_step.introduced_in_revision() != candidate_step.introduced_in_revision()
            || existing_step.retired_in_revision().is_some()
                && existing_step.retired_in_revision() != candidate_step.retired_in_revision()
            || !attempt_history_is_prefix(existing_step.attempts(), candidate_step.attempts())
        {
            return Err(TaskLedgerRepositoryError::InvalidInput);
        }
    }
    Ok(())
}

fn attempt_history_is_prefix(existing: &[TaskStepAttempt], candidate: &[TaskStepAttempt]) -> bool {
    if existing.len() > candidate.len() {
        return false;
    }
    existing
        .iter()
        .zip(candidate)
        .enumerate()
        .all(|(index, (old, new))| {
            if old == new {
                return true;
            }
            index + 1 == existing.len()
                && matches!(old.outcome(), TaskStepAttemptOutcome::Active)
                && old.number() == new.number()
                && old.run_id() == new.run_id()
                && old.started_at() == new.started_at()
                && old
                    .result_summary()
                    .is_none_or(|summary| new.result_summary() == Some(summary))
                && (old.evidence_ids().is_empty() || old.evidence_ids() == new.evidence_ids())
                && old.verification().is_none()
        })
}

fn verification_method_text(method: VerificationMethod) -> &'static str {
    match method {
        VerificationMethod::Command => "command",
        VerificationMethod::Test => "test",
        VerificationMethod::DiffInvariant => "diff_invariant",
        VerificationMethod::Diagnostic => "diagnostic",
        VerificationMethod::UserConfirm => "user_confirm",
    }
}

fn parse_verification_method(value: &str) -> Result<VerificationMethod, TaskLedgerRepositoryError> {
    match value {
        "command" => Ok(VerificationMethod::Command),
        "test" => Ok(VerificationMethod::Test),
        "diff_invariant" => Ok(VerificationMethod::DiffInvariant),
        "diagnostic" => Ok(VerificationMethod::Diagnostic),
        "user_confirm" => Ok(VerificationMethod::UserConfirm),
        _ => Err(TaskLedgerRepositoryError::InvalidStoredData),
    }
}

fn step_status_text(status: TaskStepStatus) -> &'static str {
    match status {
        TaskStepStatus::Pending => "pending",
        TaskStepStatus::Ready => "ready",
        TaskStepStatus::InProgress => "in_progress",
        TaskStepStatus::Blocked => "blocked",
        TaskStepStatus::AwaitingApproval => "awaiting_approval",
        TaskStepStatus::Verifying => "verifying",
        TaskStepStatus::Completed => "completed",
        TaskStepStatus::Failed => "failed",
        TaskStepStatus::Cancelled => "cancelled",
        TaskStepStatus::Stale => "stale",
    }
}

fn parse_step_status(value: &str) -> Result<TaskStepStatus, TaskLedgerRepositoryError> {
    match value {
        "pending" => Ok(TaskStepStatus::Pending),
        "ready" => Ok(TaskStepStatus::Ready),
        "in_progress" => Ok(TaskStepStatus::InProgress),
        "blocked" => Ok(TaskStepStatus::Blocked),
        "awaiting_approval" => Ok(TaskStepStatus::AwaitingApproval),
        "verifying" => Ok(TaskStepStatus::Verifying),
        "completed" => Ok(TaskStepStatus::Completed),
        "failed" => Ok(TaskStepStatus::Failed),
        "cancelled" => Ok(TaskStepStatus::Cancelled),
        "stale" => Ok(TaskStepStatus::Stale),
        _ => Err(TaskLedgerRepositoryError::InvalidStoredData),
    }
}

fn attempt_outcome_text(outcome: &TaskStepAttemptOutcome) -> (&'static str, Option<&str>) {
    match outcome {
        TaskStepAttemptOutcome::Active => ("active", None),
        TaskStepAttemptOutcome::Blocked { reason } => ("blocked", Some(reason.as_str())),
        TaskStepAttemptOutcome::VerificationFailed => ("verification_failed", None),
        TaskStepAttemptOutcome::Completed => ("completed", None),
        TaskStepAttemptOutcome::Failed { reason } => ("failed", Some(reason.as_str())),
        TaskStepAttemptOutcome::Cancelled { reason } => ("cancelled", Some(reason.as_str())),
    }
}

fn parse_attempt_outcome(
    outcome: &str,
    reason: Option<String>,
) -> Result<TaskStepAttemptOutcome, TaskLedgerRepositoryError> {
    match (outcome, reason) {
        ("active", None) => Ok(TaskStepAttemptOutcome::Active),
        ("blocked", Some(reason)) => TaskStepBlockingReason::try_from_string(reason)
            .map(|reason| TaskStepAttemptOutcome::Blocked { reason })
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData),
        ("verification_failed", None) => Ok(TaskStepAttemptOutcome::VerificationFailed),
        ("completed", None) => Ok(TaskStepAttemptOutcome::Completed),
        ("failed", Some(reason)) => TaskStepFailureReason::try_from_string(reason)
            .map(|reason| TaskStepAttemptOutcome::Failed { reason })
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData),
        ("cancelled", Some(reason)) => TaskStepCancellationReason::try_from_string(reason)
            .map(|reason| TaskStepAttemptOutcome::Cancelled { reason })
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData),
        _ => Err(TaskLedgerRepositoryError::InvalidStoredData),
    }
}

fn parse_verification_outcome(
    outcome: &str,
    summary: Option<String>,
) -> Result<StepVerificationOutcome, TaskLedgerRepositoryError> {
    match (outcome, summary) {
        ("passed", None) => Ok(StepVerificationOutcome::Passed),
        ("failed", Some(summary)) => VerificationFailureSummary::try_from_string(summary)
            .map(|summary| StepVerificationOutcome::Failed { summary })
            .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData),
        _ => Err(TaskLedgerRepositoryError::InvalidStoredData),
    }
}

fn validate_sequence(row: &libsql::Row, index: usize) -> Result<(), TaskLedgerRepositoryError> {
    if read_i64(row, 0)? == sequence_to_i64(index)? {
        Ok(())
    } else {
        Err(TaskLedgerRepositoryError::InvalidStoredData)
    }
}

fn sequence_to_i64(index: usize) -> Result<i64, TaskLedgerRepositoryError> {
    index
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(TaskLedgerRepositoryError::ResourceLimit)
}

fn next_store_version(
    version: TaskLedgerStoreVersion,
) -> Result<TaskLedgerStoreVersion, TaskLedgerRepositoryError> {
    version
        .get()
        .checked_add(1)
        .ok_or(TaskLedgerRepositoryError::ResourceLimit)
        .and_then(|value| {
            TaskLedgerStoreVersion::new(value).map_err(|_| TaskLedgerRepositoryError::ResourceLimit)
        })
}

fn timestamp_to_i64(timestamp: TaskLedgerTimestamp) -> i64 {
    timestamp.unix_millis() as i64
}

fn version_to_i64(version: TaskLedgerStoreVersion) -> Result<i64, TaskLedgerRepositoryError> {
    i64::try_from(version.get()).map_err(|_| TaskLedgerRepositoryError::ResourceLimit)
}

fn id_bytes<T: StableIdBytes>(id: T) -> Vec<u8> {
    id.stable_bytes().to_vec()
}

trait StableIdBytes {
    fn stable_bytes(&self) -> &[u8; 32];
}

macro_rules! stable_id_bytes {
    ($($type:ty),+ $(,)?) => {
        $(impl StableIdBytes for $type {
            fn stable_bytes(&self) -> &[u8; 32] { self.as_bytes() }
        })+
    };
}

stable_id_bytes!(
    TaskId,
    TaskStepId,
    TaskEvidenceId,
    VerificationSpecId,
    StepVerificationId,
    AgentRunId,
    WorktreeId
);

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], TaskLedgerRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(TaskLedgerRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, TaskLedgerRepositoryError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(TaskLedgerRepositoryError::Read)?;
    bytes
        .map(|value| {
            value
                .try_into()
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, TaskLedgerRepositoryError> {
    row.get(index).map_err(TaskLedgerRepositoryError::Read)
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, TaskLedgerRepositoryError> {
    row.get(index).map_err(TaskLedgerRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, TaskLedgerRepositoryError> {
    row.get(index).map_err(TaskLedgerRepositoryError::Read)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<TaskLedgerTimestamp, TaskLedgerRepositoryError> {
    let value = u64::try_from(read_i64(row, index)?)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    TaskLedgerTimestamp::from_unix_millis(value)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

fn read_optional_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<TaskLedgerTimestamp>, TaskLedgerRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(TaskLedgerRepositoryError::Read)?;
    value
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
                .and_then(|value| {
                    TaskLedgerTimestamp::from_unix_millis(value)
                        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
                })
        })
        .transpose()
}

fn read_goal_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<GoalContractRevision, TaskLedgerRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    GoalContractRevision::new(value).map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

fn read_ledger_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<TaskLedgerRevision, TaskLedgerRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    TaskLedgerRevision::new(value).map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

fn read_optional_ledger_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<TaskLedgerRevision>, TaskLedgerRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(TaskLedgerRepositoryError::Read)?;
    value
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
                .and_then(|value| {
                    TaskLedgerRevision::new(value)
                        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
                })
        })
        .transpose()
}

fn read_attempt_number(
    row: &libsql::Row,
    index: i32,
) -> Result<TaskStepAttemptNumber, TaskLedgerRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    TaskStepAttemptNumber::new(value).map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

fn read_store_version(
    row: &libsql::Row,
    index: i32,
) -> Result<TaskLedgerStoreVersion, TaskLedgerRepositoryError> {
    let value = u64::try_from(read_i64(row, index)?)
        .map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)?;
    TaskLedgerStoreVersion::new(value).map_err(|_| TaskLedgerRepositoryError::InvalidStoredData)
}

async fn close_write_transaction<T>(
    transaction: Transaction,
    result: Result<T, TaskLedgerRepositoryError>,
) -> Result<T, TaskLedgerRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(TaskLedgerRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, TaskLedgerRepositoryError>,
) -> Result<T, TaskLedgerRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(TaskLedgerRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: TaskLedgerRepositoryError,
) -> Result<T, TaskLedgerRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(TaskLedgerRepositoryError::Rollback(source)),
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_unexpected_constraint(source: libsql::Error) -> TaskLedgerRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        TaskLedgerRepositoryError::InvalidStoredData
    } else {
        TaskLedgerRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum TaskLedgerRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    GoalContract(goal_contract_repository::GoalContractRepositoryError),
    InvalidInput,
    InvalidStoredData,
    ResourceLimit,
    LedgerAlreadyExists,
    TaskNotFound,
    VersionConflict,
}

impl TaskLedgerRepositoryError {
    pub(crate) fn classify(&self) -> TaskLedgerStoreFailure {
        match self {
            Self::InvalidInput | Self::InvalidStoredData | Self::ResourceLimit => {
                TaskLedgerStoreFailure::InvalidStoredData
            }
            Self::LedgerAlreadyExists => TaskLedgerStoreFailure::LedgerAlreadyExists,
            Self::TaskNotFound => TaskLedgerStoreFailure::TaskNotFound,
            Self::VersionConflict => TaskLedgerStoreFailure::VersionConflict,
            Self::GoalContract(error) => match error.classify() {
                a3_application::GoalContractStoreFailure::Unavailable => {
                    TaskLedgerStoreFailure::Unavailable
                }
                a3_application::GoalContractStoreFailure::Corrupt => {
                    TaskLedgerStoreFailure::Corrupt
                }
                a3_application::GoalContractStoreFailure::UnsupportedSchema => {
                    TaskLedgerStoreFailure::UnsupportedSchema
                }
                a3_application::GoalContractStoreFailure::InvalidStoredData
                | a3_application::GoalContractStoreFailure::TaskAlreadyExists
                | a3_application::GoalContractStoreFailure::TaskNotFound
                | a3_application::GoalContractStoreFailure::RevisionConflict => {
                    TaskLedgerStoreFailure::InvalidStoredData
                }
            },
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    TaskLedgerStoreFailure::Corrupt
                } else {
                    TaskLedgerStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for TaskLedgerRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "Task Ledger transaction could not begin",
            Self::Read(_) => "Task Ledger data could not be read",
            Self::Write(_) => "Task Ledger data could not be written",
            Self::Commit(_) => "Task Ledger transaction could not commit",
            Self::Rollback(_) => "Task Ledger transaction could not roll back",
            Self::GoalContract(_) => "Task Ledger Goal Contract could not be reconstructed",
            Self::InvalidInput => "Task Ledger successor was invalid",
            Self::InvalidStoredData => "Task Ledger data was invalid",
            Self::ResourceLimit => "Task Ledger data exceeded a resource limit",
            Self::LedgerAlreadyExists => "Task Ledger already exists",
            Self::TaskNotFound => "Task Ledger task was not found",
            Self::VersionConflict => "Task Ledger store version conflicted",
        })
    }
}

impl Error for TaskLedgerRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            Self::GoalContract(error) => Some(error),
            Self::InvalidInput
            | Self::InvalidStoredData
            | Self::ResourceLimit
            | Self::LedgerAlreadyExists
            | Self::TaskNotFound
            | Self::VersionConflict => None,
        }
    }
}
