use crate::{
    catalog::is_corruption, loopback_http_codec as http, python_translation_codec as python,
};
use a3_application::{
    AgentControllerControl, PreparedLoopbackStartupCoverage, PreparedLoopbackVerification,
    StoredVerificationState, VerificationEvidenceStoreFailure,
};
use a3_domain::{
    AgentRunId, CommandEvidence, CommandEvidenceContext, ContentHash, DiagnosticCount,
    DiagnosticEvidence, DiffEvidence, DiffEvidenceSource, EvidenceDependency, FileRevision,
    IndexRunId, LoopbackHttpEvidence, LoopbackHttpRecipe, MutationActionFingerprint,
    PatchActionDigest, PolicyAction, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyResourceId, ProcessDuration, ProcessExit, ProcessOutputDigest, ProcessOutputRedaction,
    ProcessStreamEvidence, ProcessTermination, PublishedIndex, RepositoryPath, SnapshotId,
    StoredDiffEvidenceContext, StoredProcessEvidence, TaskEvidenceId, TaskId, TaskLedgerTimestamp,
    TestCaseEvidence, TestCaseName, TestCaseOutcome, TestEvidence, ToolRunId,
    UserConfirmationEvidence, VerificationDependencies, VerificationEvidence, VerificationRunId,
    VerificationSpecId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::fmt;
use std::time::{Duration, Instant};

const SQLITE_CONSTRAINT: i32 = 19;
// Each completed active step can provide one typed HTTP receipt.
const MAX_LOOPBACK_RECOVERY_EVIDENCE: usize = 256;
const MAX_DIFF_RECOVERY_EVIDENCE: usize = 16_384;

pub(crate) async fn append(
    connection: &Connection,
    worktree_id: WorktreeId,
    evidence: &VerificationEvidence,
    schema: u32,
    timeout: Duration,
    control: &dyn AgentControllerControl,
) -> Result<(), VerificationEvidenceRepositoryError> {
    let guard = OperationGuard::new(timeout, control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(VerificationEvidenceRepositoryError::Begin)?;
    let result = append_in_transaction(&transaction, worktree_id, evidence, schema, &guard).await;
    close_write(transaction, result).await
}

pub(crate) async fn append_in_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    evidence: &VerificationEvidence,
    schema: u32,
    guard: &OperationGuard<'_>,
) -> Result<(), VerificationEvidenceRepositoryError> {
    guard.checkpoint()?;
    let task_id = evidence_task_id(
        transaction,
        worktree_id,
        evidence.spec_id(),
        evidence.run_id(),
    )
    .await?;
    require_test_guard(
        transaction,
        task_id,
        evidence,
        schema >= python::SCHEMA_VERSION,
    )
    .await?;
    if matches!(evidence, VerificationEvidence::LoopbackHttp(_)) && schema < http::SCHEMA_VERSION {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    if let VerificationEvidence::LoopbackHttp(receipt) = evidence
        && receipt.startup().is_some()
        && schema < http::STARTUP_EVIDENCE_SCHEMA_VERSION
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    if let Some(existing) =
        read_evidence(transaction, worktree_id, task_id, evidence.id(), schema).await?
    {
        return if &existing == evidence {
            guard.checkpoint()?;
            Ok(())
        } else {
            Err(VerificationEvidenceRepositoryError::EvidenceConflict)
        };
    }
    if let VerificationEvidence::LoopbackHttp(receipt) = evidence {
        require_loopback_binding(transaction, worktree_id, task_id, receipt).await?;
        require_current_loopback_source(transaction, worktree_id, receipt).await?;
    }
    let mut values = Vec::from(params![
        id_bytes(evidence.id().as_bytes()),
        id_bytes(task_id.as_bytes()),
        id_bytes(worktree_id.as_bytes()),
        evidence_kind(evidence),
        id_bytes(evidence.verification_run_id().as_bytes()),
        id_bytes(evidence.spec_id().as_bytes()),
        id_bytes(evidence.run_id().as_bytes()),
        id_bytes(evidence.snapshot_id().as_bytes()),
    ]);
    let (column, parameter) = if let VerificationEvidence::LoopbackHttp(receipt) = evidence {
        values.push(Ok(Value::Blob(http::encode_evidence(receipt))));
        (", loopback_http_evidence", ", ?9")
    } else if let VerificationEvidence::Test(test) = evidence
        && let Some(witness) = test.python_integer_translation()
    {
        values.push(Ok(Value::Blob(python::write_witness(witness))));
        (", python_integer_translation_witness", ", ?9")
    } else {
        ("", "")
    };
    transaction
        .execute(
            &format!(
                "INSERT INTO verification_evidence (
                 evidence_id, task_id, worktree_id, evidence_kind, schema_version,
                 verification_run_id, verification_spec_id, run_id, snapshot_id{column}
                 ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8{parameter})"
            ),
            params_from_iter(values),
        )
        .await
        .map_err(classify_write)?;
    write_variant(transaction, evidence, guard).await?;
    guard.checkpoint()
}

pub(crate) struct VerificationStateQuery<'a> {
    worktree_id: WorktreeId,
    task_id: TaskId,
    evidence_ids: &'a [TaskEvidenceId],
    expected_snapshot_id: Option<SnapshotId>,
    published: PublishedIndex,
    timeout: Duration,
}

impl<'a> VerificationStateQuery<'a> {
    pub(crate) const fn new(
        worktree_id: WorktreeId,
        task_id: TaskId,
        evidence_ids: &'a [TaskEvidenceId],
        expected_snapshot_id: SnapshotId,
        published: PublishedIndex,
        timeout: Duration,
    ) -> Self {
        Self {
            worktree_id,
            task_id,
            evidence_ids,
            expected_snapshot_id: Some(expected_snapshot_id),
            published,
            timeout,
        }
    }

    pub(crate) const fn for_inspection(
        worktree_id: WorktreeId,
        task_id: TaskId,
        evidence_ids: &'a [TaskEvidenceId],
        published: PublishedIndex,
        timeout: Duration,
    ) -> Self {
        Self {
            worktree_id,
            task_id,
            evidence_ids,
            expected_snapshot_id: None,
            published,
            timeout,
        }
    }
}

pub(crate) async fn load_state(
    connection: &Connection,
    query: VerificationStateQuery<'_>,
    schema: u32,
    control: &dyn AgentControllerControl,
) -> Result<StoredVerificationState, VerificationEvidenceRepositoryError> {
    let guard = OperationGuard::new(query.timeout, control)?;
    if query
        .expected_snapshot_id
        .is_some_and(|expected| query.published.publication().graph().snapshot_id() != expected)
    {
        return Err(VerificationEvidenceRepositoryError::SnapshotMismatch);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(VerificationEvidenceRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        validate_latest_publication(&transaction, query.worktree_id, &query.published).await?;
        let mut evidence = Vec::with_capacity(query.evidence_ids.len());
        for (index, evidence_id) in query.evidence_ids.iter().enumerate() {
            if index.is_multiple_of(16) {
                guard.checkpoint()?;
            }
            evidence.push(
                read_evidence(
                    &transaction,
                    query.worktree_id,
                    query.task_id,
                    *evidence_id,
                    schema,
                )
                .await?
                .ok_or(VerificationEvidenceRepositoryError::InvalidStoredData)?,
            );
        }
        validate_latest_publication(&transaction, query.worktree_id, &query.published).await?;
        guard.checkpoint()?;
        StoredVerificationState::new(query.published, evidence)
            .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
    }
    .await;
    close_read(transaction, result).await
}

pub(crate) async fn load_loopback_recovery_evidence(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
    evidence_ids: &[TaskEvidenceId],
    schema: u32,
    timeout: Duration,
    control: &dyn AgentControllerControl,
) -> Result<Vec<VerificationEvidence>, VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    if evidence_ids.len() > MAX_LOOPBACK_RECOVERY_EVIDENCE {
        return Err(Error::ResourceLimit);
    }
    let guard = OperationGuard::new(timeout, control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        let mut evidence = Vec::with_capacity(evidence_ids.len());
        for id in evidence_ids {
            guard.checkpoint()?;
            if let Some(receipt @ VerificationEvidence::LoopbackHttp(_)) =
                read_evidence(&transaction, worktree_id, task_id, *id, schema).await?
            {
                evidence.push(receipt);
            }
        }
        guard.checkpoint()?;
        Ok(evidence)
    }
    .await;
    close_read(transaction, result).await
}

/// Reads existing typed Diff rows without requiring their historical snapshot to be current.
/// The application independently evaluates their exact step binding and granular freshness.
pub(crate) async fn load_diff_recovery_evidence(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
    evidence_ids: &[TaskEvidenceId],
    schema: u32,
    timeout: Duration,
    control: &dyn AgentControllerControl,
) -> Result<Vec<DiffEvidence>, VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    if evidence_ids.len() > MAX_DIFF_RECOVERY_EVIDENCE {
        return Err(Error::ResourceLimit);
    }
    let guard = OperationGuard::new(timeout, control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        let mut evidence = Vec::with_capacity(evidence_ids.len());
        for id in evidence_ids {
            guard.checkpoint()?;
            if let Some(VerificationEvidence::Diff(diff)) =
                read_evidence(&transaction, worktree_id, task_id, *id, schema).await?
            {
                evidence.push(diff);
            }
        }
        evidence.sort_by_key(DiffEvidence::id);
        guard.checkpoint()?;
        Ok(evidence)
    }
    .await;
    close_read(transaction, result).await
}

async fn evidence_task_id(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
) -> Result<TaskId, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT s.task_id FROM task_steps s
             JOIN verification_specs_v1 v ON v.task_id = s.task_id
               AND v.verification_spec_id = s.verification_spec_id
             JOIN tasks t ON t.task_id = s.task_id
             JOIN agent_runs r ON r.task_id = s.task_id
             WHERE s.verification_spec_id = ?1 AND t.worktree_id = ?2 AND r.run_id = ?3",
            params![
                id_bytes(spec_id.as_bytes()),
                id_bytes(worktree_id.as_bytes()),
                id_bytes(run_id.as_bytes())
            ],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::ProjectMismatch);
    };
    let task_id = TaskId::from_bytes(read_id(&row, 0)?);
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    Ok(task_id)
}

async fn write_variant(
    transaction: &Transaction,
    evidence: &VerificationEvidence,
    guard: &OperationGuard<'_>,
) -> Result<(), VerificationEvidenceRepositoryError> {
    guard.checkpoint()?;
    match evidence {
        VerificationEvidence::Command(command) => {
            write_process(transaction, evidence.id(), command.id(), command).await?;
            write_dependencies(transaction, evidence.id(), command.dependencies(), guard).await?;
        }
        VerificationEvidence::Test(test) => {
            write_process(
                transaction,
                evidence.id(),
                test.command().id(),
                test.command(),
            )
            .await?;
            write_dependencies(
                transaction,
                evidence.id(),
                test.command().dependencies(),
                guard,
            )
            .await?;
            for (index, case) in test.cases().iter().enumerate() {
                if index.is_multiple_of(16) {
                    guard.checkpoint()?;
                }
                transaction
                    .execute(
                        "INSERT INTO verification_test_cases (
                         evidence_id, item_sequence, case_name, outcome
                         ) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            id_bytes(evidence.id().as_bytes()),
                            sequence_to_i64(index)?,
                            case.name().as_str(),
                            test_outcome_text(case.outcome())
                        ],
                    )
                    .await
                    .map_err(classify_write)?;
            }
        }
        VerificationEvidence::Diagnostic(diagnostic) => {
            write_process(
                transaction,
                evidence.id(),
                diagnostic.command().id(),
                diagnostic.command(),
            )
            .await?;
            write_dependencies(
                transaction,
                evidence.id(),
                diagnostic.command().dependencies(),
                guard,
            )
            .await?;
            transaction
                .execute(
                    "INSERT INTO verification_diagnostic_reports (
                     evidence_id, error_count, warning_count
                     ) VALUES (?1, ?2, ?3)",
                    params![
                        id_bytes(evidence.id().as_bytes()),
                        i64::from(diagnostic.errors().get()),
                        i64::from(diagnostic.warnings().get())
                    ],
                )
                .await
                .map_err(classify_write)?;
        }
        VerificationEvidence::Diff(diff) => {
            let source = diff_source_fields(diff.source());
            transaction
                .execute(
                    "INSERT INTO verification_diff_evidence (
                     evidence_id, source_kind, action_digest, policy_decision_id,
                     base_index_run_id, current_index_run_id, base_snapshot_id, complete
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id_bytes(evidence.id().as_bytes()),
                        source.kind,
                        source.action_digest,
                        source.policy_decision_id,
                        source.base_index_run_id,
                        source.current_index_run_id,
                        id_bytes(diff.base_snapshot_id().as_bytes()),
                        bool_to_i64(diff.complete())
                    ],
                )
                .await
                .map_err(classify_write)?;
            for (index, path) in diff.changed_paths().iter().enumerate() {
                if index.is_multiple_of(16) {
                    guard.checkpoint()?;
                }
                transaction
                    .execute(
                        "INSERT INTO verification_diff_paths (
                         evidence_id, item_sequence, repository_path
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            id_bytes(evidence.id().as_bytes()),
                            sequence_to_i64(index)?,
                            path.as_bytes().to_vec()
                        ],
                    )
                    .await
                    .map_err(classify_write)?;
            }
            write_dependencies(transaction, evidence.id(), diff.dependencies(), guard).await?;
        }
        VerificationEvidence::UserConfirmation(confirmation) => {
            transaction
                .execute(
                    "INSERT INTO verification_user_confirmations (
                     evidence_id, scope_id, confirmed_at_unix_millis
                     ) VALUES (?1, ?2, ?3)",
                    params![
                        id_bytes(evidence.id().as_bytes()),
                        id_bytes(confirmation.scope_id().as_bytes()),
                        u64_to_i64(confirmation.confirmed_at().unix_millis())?
                    ],
                )
                .await
                .map_err(classify_write)?;
        }
        VerificationEvidence::LoopbackHttp(_) => {
            // The V56 payload owns the revision and observations. No legacy child
            // can stand in for it if this content-free payload is missing.
        }
    }
    Ok(())
}

async fn write_process(
    transaction: &Transaction,
    evidence_id: TaskEvidenceId,
    command_evidence_id: TaskEvidenceId,
    command: &CommandEvidence,
) -> Result<(), VerificationEvidenceRepositoryError> {
    let (termination_kind, exit_code) = termination_fields(command.termination());
    let stdout = command.stdout();
    let stderr = command.stderr();
    transaction
        .execute(
            "INSERT INTO verification_process_evidence (
             evidence_id, command_evidence_id, tool_run_id, command_id,
             process_specification_id, policy_decision_id, termination_kind, exit_code,
             duration_millis, stdout_digest, stdout_observed_bytes, stdout_retained_limit,
             stdout_truncated, stdout_redaction, stderr_digest, stderr_observed_bytes,
             stderr_retained_limit, stderr_truncated, stderr_redaction
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
               ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                id_bytes(evidence_id.as_bytes()),
                id_bytes(command_evidence_id.as_bytes()),
                id_bytes(command.tool_run_id().as_bytes()),
                id_bytes(command.command_id().as_bytes()),
                id_bytes(command.process_specification_id().as_bytes()),
                id_bytes(command.policy_decision_id().as_bytes()),
                termination_kind,
                exit_code,
                u64_to_i64(command.duration().as_millis())?,
                stdout.digest().as_bytes().to_vec(),
                u64_to_i64(stdout.observed_bytes())?,
                i64::from(stdout.retained_limit()),
                bool_to_i64(stdout.truncated()),
                redaction_text(stdout.redaction()),
                stderr.digest().as_bytes().to_vec(),
                u64_to_i64(stderr.observed_bytes())?,
                i64::from(stderr.retained_limit()),
                bool_to_i64(stderr.truncated()),
                redaction_text(stderr.redaction())
            ],
        )
        .await
        .map_err(classify_write)?;
    Ok(())
}

async fn write_dependencies(
    transaction: &Transaction,
    evidence_id: TaskEvidenceId,
    dependencies: &VerificationDependencies,
    guard: &OperationGuard<'_>,
) -> Result<(), VerificationEvidenceRepositoryError> {
    for (index, dependency) in dependencies.as_slice().iter().enumerate() {
        if index.is_multiple_of(16) {
            guard.checkpoint()?;
        }
        let (state, hash) = match dependency {
            EvidenceDependency::Present(revision) => {
                ("present", Some(revision.content_hash().as_bytes().to_vec()))
            }
            EvidenceDependency::Absent(_) => ("absent", None),
        };
        transaction
            .execute(
                "INSERT INTO verification_evidence_dependencies (
                 evidence_id, item_sequence, repository_path, dependency_state, content_hash
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id_bytes(evidence_id.as_bytes()),
                    sequence_to_i64(index)?,
                    dependency.path().as_bytes().to_vec(),
                    state,
                    hash
                ],
            )
            .await
            .map_err(classify_write)?;
    }
    Ok(())
}

async fn read_evidence(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
    evidence_id: TaskEvidenceId,
    schema: u32,
) -> Result<Option<VerificationEvidence>, VerificationEvidenceRepositoryError> {
    let guarded = schema >= python::SCHEMA_VERSION;
    let loopback = schema >= http::SCHEMA_VERSION;
    let guard_column = if guarded { python::WITNESS_SELECT } else { "" };
    let http_column = if loopback { http::EVIDENCE_SELECT } else { "" };
    let mut rows = transaction
        .query(
            &format!("SELECT evidence_kind, verification_run_id, verification_spec_id, run_id, snapshot_id{guard_column}{http_column}
             FROM verification_evidence
             WHERE evidence_id = ?1 AND task_id = ?2 AND worktree_id = ?3"),
            params![
                id_bytes(evidence_id.as_bytes()),
                id_bytes(task_id.as_bytes()),
                id_bytes(worktree_id.as_bytes())
            ],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let header = EvidenceHeader {
        id: evidence_id,
        kind: read_text(&row, 0)?,
        verification_run_id: VerificationRunId::from_bytes(read_id(&row, 1)?),
        spec_id: VerificationSpecId::from_bytes(read_id(&row, 2)?),
        run_id: AgentRunId::from_bytes(read_id(&row, 3)?),
        snapshot_id: SnapshotId::from_bytes(read_id(&row, 4)?),
    };
    let witness = if guarded {
        python::read_witness(&row, 5)
            .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?
    } else {
        None
    };
    let http_receipt = if loopback {
        let column = 5 + i32::from(guarded);
        let startup_step = http::read_startup_step(&row, column)
            .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?;
        let recipe = if let Some(step) = startup_step {
            if schema < http::STARTUP_EVIDENCE_SCHEMA_VERSION {
                return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
            }
            Some(
                crate::task_ledger_repository::read_bound_loopback_recipe(
                    transaction,
                    task_id,
                    step,
                    header.spec_id,
                )
                .await
                .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?,
            )
        } else {
            None
        };
        http::read_evidence(
            &row,
            column,
            header.verification_run_id,
            header.spec_id,
            header.run_id,
            header.snapshot_id,
            recipe.as_ref(),
        )
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?
    } else {
        None
    };
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    if header.kind != "test" && witness.is_some() {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    if let Some(receipt) = http_receipt {
        if header.kind != "command" || receipt.id() != header.id {
            return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
        }
        require_no_legacy_evidence_children(transaction, header.id).await?;
        require_loopback_binding(transaction, worktree_id, task_id, &receipt).await?;
        return Ok(Some(VerificationEvidence::LoopbackHttp(receipt)));
    }
    let evidence = match header.kind.as_str() {
        "command" => VerificationEvidence::Command(
            read_process(transaction, &header)
                .await?
                .into_command(header.id)?,
        ),
        "test" => {
            let process = read_process(transaction, &header).await?;
            let command_evidence_id = process.command_evidence_id;
            let command = process.into_command(command_evidence_id)?;
            let cases = read_test_cases(transaction, header.id).await?;
            let test = TestEvidence::new(command, cases)
                .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?;
            let test = match witness {
                Some(witness) => test.with_python_integer_translation(witness),
                None => test,
            };
            if test.id() != header.id {
                return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
            }
            VerificationEvidence::Test(test)
        }
        "diagnostic" => {
            let process = read_process(transaction, &header).await?;
            let command_evidence_id = process.command_evidence_id;
            let command = process.into_command(command_evidence_id)?;
            let (errors, warnings) = read_diagnostic(transaction, header.id).await?;
            let diagnostic = DiagnosticEvidence::new(command, errors, warnings);
            if diagnostic.id() != header.id {
                return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
            }
            VerificationEvidence::Diagnostic(diagnostic)
        }
        "diff_invariant" => {
            VerificationEvidence::Diff(read_diff(transaction, worktree_id, &header).await?)
        }
        "user_confirm" => {
            VerificationEvidence::UserConfirmation(read_confirmation(transaction, &header).await?)
        }
        _ => return Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    };
    require_test_guard(transaction, task_id, &evidence, guarded).await?;
    Ok(Some(evidence))
}

async fn require_no_legacy_evidence_children(
    tx: &Transaction,
    id: TaskEvidenceId,
) -> Result<(), VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    let mut rows = tx
        .query(
            "SELECT 1 FROM verification_process_evidence WHERE evidence_id=?1
             UNION ALL SELECT 1 FROM verification_diff_evidence WHERE evidence_id=?1
             UNION ALL SELECT 1 FROM verification_user_confirmations WHERE evidence_id=?1
             UNION ALL SELECT 1 FROM verification_evidence_dependencies WHERE evidence_id=?1
             LIMIT 1",
            params![id_bytes(id.as_bytes())],
        )
        .await
        .map_err(Error::Read)?;
    if rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    Ok(())
}

async fn require_loopback_binding(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    receipt: &LoopbackHttpEvidence,
) -> Result<(), VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    let context = receipt.context();
    let recipe = crate::task_ledger_repository::read_bound_loopback_recipe(
        tx,
        task,
        context.step_id,
        context.spec_id,
    )
    .await
    .map_err(|_| Error::InvalidStoredData)?;
    let binding = bind_loopback_receipt(worktree, receipt, recipe)?;
    let mut attempts = tx
        .query(
            "SELECT a.started_at_unix_millis, t.event_sequence
             FROM tool_runs t
             JOIN agent_runs r ON r.run_id=t.run_id
             JOIN tool_run_attempts a ON a.tool_run_id=t.tool_run_id AND a.run_id=t.run_id
             JOIN mutation_attempts m ON m.tool_run_id=a.tool_run_id AND m.attempt_sequence=a.attempt_sequence
             JOIN machine_effect_scopes s ON s.tool_run_id=a.tool_run_id AND s.attempt_sequence=a.attempt_sequence
             WHERE t.tool_run_id=?1 AND t.run_id=?2 AND r.task_id=?3
               AND t.snapshot_before_id=?4 AND t.snapshot_after_id=?4 AND a.snapshot_id=?4
               AND t.result_digest=?5 AND t.result_truncated=0
               AND t.status='succeeded' AND a.status='succeeded' AND m.action_kind='machine_process'
               AND m.application_state='applied' AND m.reconciliation_state='not_required'
               AND m.action_fingerprint=?6 AND s.step_id=?7
               AND s.resource_kind='process' AND s.resource_id=?8 AND s.target='python'",
            params![
                id_bytes(context.tool_run_id.as_bytes()), id_bytes(context.run_id.as_bytes()),
                id_bytes(task.as_bytes()), id_bytes(context.snapshot_id.as_bytes()),
                id_bytes(receipt.id().as_bytes()), id_bytes(&binding.fingerprint.as_bytes()),
                id_bytes(context.step_id.as_bytes()), id_bytes(binding.resource.as_bytes()),
            ],
        )
        .await
        .map_err(Error::Read)?;
    let attempt = attempts
        .next()
        .await
        .map_err(Error::Read)?
        .ok_or(Error::InvalidStoredData)?;
    let started_at = read_u64(&attempt, 0)?;
    let completion = read_i64(&attempt, 1)?;
    if attempts.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    for (decision, action) in binding.authorizations {
        require_loopback_decision(tx, receipt, decision, &action, started_at, completion).await?;
    }
    Ok(())
}

struct LoopbackBinding {
    resource: PolicyResourceId,
    fingerprint: MutationActionFingerprint,
    authorizations: Vec<(PolicyDecisionId, PolicyAction)>,
}

fn bind_loopback_receipt(
    worktree: WorktreeId,
    receipt: &LoopbackHttpEvidence,
    recipe: LoopbackHttpRecipe,
) -> Result<LoopbackBinding, VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError::InvalidStoredData;
    let context = receipt.context();
    if recipe.id() != receipt.recipe_id()
        || recipe.script() != receipt.script().path()
        || recipe.checks().len() != receipt.observations().len()
    {
        return Err(InvalidStoredData);
    }
    let process = recipe
        .process_spec(
            context.run_id,
            worktree,
            context.step_id,
            receipt.process().settings(),
        )
        .map_err(|_| InvalidStoredData)?;
    if process.specification_id() != receipt.process_specification() {
        return Err(InvalidStoredData);
    }
    match (recipe.default_start_check_index(), receipt.startup()) {
        (Some(_), Some(startup)) => {
            let prepared = PreparedLoopbackStartupCoverage::new(
                context.run_id,
                worktree,
                context.step_id,
                context.snapshot_id,
                receipt.script().clone(),
                recipe,
                receipt.process().settings(),
            )
            .map_err(|_| InvalidStoredData)?;
            let actions = prepared.policy_actions();
            let mut decisions = vec![
                startup.reserve_decision(),
                startup.held_explicit_decision(),
                receipt.start_decision(),
            ];
            decisions.extend(
                receipt
                    .observations()
                    .iter()
                    .map(|observation| observation.decision()),
            );
            decisions.extend([
                startup.default_start_decision(),
                startup.default_observation().decision(),
                startup.invalid_port_decision(),
            ]);
            if actions.len() != decisions.len() {
                return Err(InvalidStoredData);
            }
            Ok(LoopbackBinding {
                resource: prepared.effect_resource(),
                fingerprint: prepared.effect_fingerprint(),
                authorizations: decisions.into_iter().zip(actions).collect(),
            })
        }
        (None, None) => {
            let start = process.policy_action();
            let mut authorizations = vec![(receipt.start_decision(), start.clone())];
            for (index, observation) in receipt.observations().iter().enumerate() {
                let action = PreparedLoopbackVerification::bound_probe_policy_action(
                    worktree,
                    context.step_id,
                    receipt.process_specification(),
                    context.snapshot_id,
                    &recipe,
                    receipt.script(),
                    index,
                )
                .ok_or(InvalidStoredData)?;
                authorizations.push((observation.decision(), action));
            }
            Ok(LoopbackBinding {
                resource: receipt.process_specification(),
                fingerprint: MutationActionFingerprint::from_bytes(*start.fingerprint().as_bytes()),
                authorizations,
            })
        }
        _ => Err(InvalidStoredData),
    }
}

pub(crate) async fn expected_loopback_effect_resource(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    receipt: &LoopbackHttpEvidence,
) -> Result<PolicyResourceId, VerificationEvidenceRepositoryError> {
    let context = receipt.context();
    let recipe = crate::task_ledger_repository::read_bound_loopback_recipe(
        tx,
        task,
        context.step_id,
        context.spec_id,
    )
    .await
    .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?;
    bind_loopback_receipt(worktree, receipt, recipe).map(|binding| binding.resource)
}

async fn require_current_loopback_source(
    tx: &Transaction,
    worktree: WorktreeId,
    receipt: &LoopbackHttpEvidence,
) -> Result<(), VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    let mut rows = tx
        .query(
            "WITH latest AS (
                SELECT index_run_id, snapshot_id FROM index_runs
                WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1
             )
             SELECT 1 FROM latest JOIN file_revisions f USING(index_run_id)
             WHERE latest.snapshot_id=?2 AND f.repository_path=?3 AND f.content_hash=?4",
            params![
                id_bytes(worktree.as_bytes()),
                id_bytes(receipt.context().snapshot_id.as_bytes()),
                receipt.script().path().as_bytes().to_vec(),
                id_bytes(receipt.script().content_hash().as_bytes()),
            ],
        )
        .await
        .map_err(Error::Read)?;
    if rows.next().await.map_err(Error::Read)?.is_none() {
        return Err(Error::SnapshotMismatch);
    }
    Ok(())
}

async fn require_loopback_decision(
    tx: &Transaction,
    receipt: &LoopbackHttpEvidence,
    id: PolicyDecisionId,
    action: &PolicyAction,
    started_at: u64,
    completion: i64,
) -> Result<(), VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    let context = receipt.context();
    let decision = crate::policy_repository::load_decision(tx, id)
        .await
        .map_err(|_| Error::InvalidStoredData)?
        .ok_or(Error::InvalidStoredData)?;
    if decision.run_id() != context.run_id
        || decision.outcome() != PolicyDecisionOutcome::Allowed
        || decision.reason() != PolicyDecisionReason::ApprovalGranted
        || decision.action_fingerprint() != action.fingerprint()
        || decision.scope_digest() != action.scope_digest()
        || decision.action_class() != action.class()
        || decision.risk_level() != action.risk()
        || decision.timing().decided_at().unix_millis() > started_at
    {
        return Err(Error::InvalidStoredData);
    }
    let mut events = tx.query(
        "SELECT 1 FROM policy_decisions p JOIN run_events e ON e.event_id=p.event_id AND e.run_id=p.run_id
         WHERE p.policy_decision_id=?1 AND p.run_id=?2 AND e.snapshot_id=?3 AND e.event_sequence<?4",
        params![id_bytes(id.as_bytes()), id_bytes(context.run_id.as_bytes()),
            id_bytes(context.snapshot_id.as_bytes()), completion],
    ).await.map_err(Error::Read)?;
    if events.next().await.map_err(Error::Read)?.is_none() {
        return Err(Error::InvalidStoredData);
    }
    Ok(())
}

async fn require_test_guard(
    tx: &Transaction,
    task: TaskId,
    evidence: &VerificationEvidence,
    guarded: bool,
) -> Result<(), VerificationEvidenceRepositoryError> {
    use VerificationEvidenceRepositoryError as Error;
    let VerificationEvidence::Test(test) = evidence else {
        return Ok(());
    };
    // An actual process observation remains durable even when the separate source proof is open.
    let Some(witness) = test.python_integer_translation() else {
        return Ok(());
    };
    if !guarded {
        return Err(Error::InvalidStoredData);
    }
    let projection = python::REQUIREMENT_SELECT;
    let mut rows = tx
        .query(
            &format!(
                "SELECT target_kind{projection} FROM verification_specs_v1
         WHERE task_id=?1 AND verification_spec_id=?2"
            ),
            params![
                id_bytes(task.as_bytes()),
                id_bytes(evidence.spec_id().as_bytes())
            ],
        )
        .await
        .map_err(Error::Read)?;
    let requirement = if let Some(row) = rows.next().await.map_err(Error::Read)? {
        let requirement =
            python::read_requirement(&row, 1).map_err(|_| Error::InvalidStoredData)?;
        if requirement.is_some() && read_text(&row, 0)? != "test" {
            return Err(Error::InvalidStoredData);
        }
        requirement
    } else {
        None
    };
    if requirement.as_ref() != Some(witness.requirement()) {
        return Err(Error::InvalidStoredData);
    }
    if !test.command().dependencies().as_slice().iter().any(
            |dependency| matches!(dependency, EvidenceDependency::Present(revision) if revision == witness.revision())
        )
    {
        return Err(Error::InvalidStoredData);
    }
    Ok(())
}

struct EvidenceHeader {
    id: TaskEvidenceId,
    kind: String,
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
}

struct ProcessRecord {
    header_context: CommandEvidenceContext,
    command_evidence_id: TaskEvidenceId,
    process: StoredProcessEvidence,
    dependencies: VerificationDependencies,
}

impl ProcessRecord {
    fn into_command(
        self,
        expected_id: TaskEvidenceId,
    ) -> Result<CommandEvidence, VerificationEvidenceRepositoryError> {
        CommandEvidence::from_stored(
            expected_id,
            self.header_context,
            self.process,
            self.dependencies,
        )
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
    }
}

async fn read_process(
    transaction: &Transaction,
    header: &EvidenceHeader,
) -> Result<ProcessRecord, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT command_evidence_id, tool_run_id, command_id, process_specification_id,
             policy_decision_id, termination_kind, exit_code, duration_millis,
             stdout_digest, stdout_observed_bytes, stdout_retained_limit, stdout_truncated,
             stdout_redaction, stderr_digest, stderr_observed_bytes, stderr_retained_limit,
             stderr_truncated, stderr_redaction
             FROM verification_process_evidence WHERE evidence_id = ?1",
            params![id_bytes(header.id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    };
    let command_evidence_id = TaskEvidenceId::from_bytes(read_id(&row, 0)?);
    let context = CommandEvidenceContext::new(
        header.verification_run_id,
        header.spec_id,
        header.run_id,
        ToolRunId::from_bytes(read_id(&row, 1)?),
        a3_domain::DiscoveredCommandId::from_bytes(read_id(&row, 2)?),
        header.snapshot_id,
    );
    let process = StoredProcessEvidence::new(
        PolicyResourceId::from_bytes(read_id(&row, 3)?),
        PolicyDecisionId::from_bytes(read_id(&row, 4)?),
        parse_termination(&read_text(&row, 5)?, read_optional_i32(&row, 6)?)?,
        ProcessDuration::from_millis(read_u64(&row, 7)?),
        read_stream(&row, 8)?,
        read_stream(&row, 13)?,
    );
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    Ok(ProcessRecord {
        header_context: context,
        command_evidence_id,
        process,
        dependencies: read_dependencies(transaction, header.id).await?,
    })
}

fn read_stream(
    row: &libsql::Row,
    start: i32,
) -> Result<ProcessStreamEvidence, VerificationEvidenceRepositoryError> {
    ProcessStreamEvidence::from_stored(
        ProcessOutputDigest::from_bytes(read_id(row, start)?),
        read_u64(row, start + 1)?,
        read_u32(row, start + 2)?,
        read_bool(row, start + 3)?,
        parse_redaction(read_optional_text(row, start + 4)?.as_deref())?,
    )
    .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

async fn read_test_cases(
    transaction: &Transaction,
    evidence_id: TaskEvidenceId,
) -> Result<Vec<TestCaseEvidence>, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT item_sequence, case_name, outcome FROM verification_test_cases
             WHERE evidence_id = ?1 ORDER BY item_sequence",
            params![id_bytes(evidence_id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let mut cases = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    {
        validate_sequence(&row, cases.len())?;
        cases.push(TestCaseEvidence::new(
            TestCaseName::try_from_string(read_text(&row, 1)?)
                .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?,
            parse_test_outcome(&read_text(&row, 2)?)?,
        ));
    }
    Ok(cases)
}

async fn read_diagnostic(
    transaction: &Transaction,
    evidence_id: TaskEvidenceId,
) -> Result<(DiagnosticCount, DiagnosticCount), VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT error_count, warning_count FROM verification_diagnostic_reports
             WHERE evidence_id = ?1",
            params![id_bytes(evidence_id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    };
    let result = (
        DiagnosticCount::new(read_u32(&row, 0)?),
        DiagnosticCount::new(read_u32(&row, 1)?),
    );
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    Ok(result)
}

async fn read_diff(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    header: &EvidenceHeader,
) -> Result<DiffEvidence, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT source_kind, action_digest, policy_decision_id, base_index_run_id,
             current_index_run_id, base_snapshot_id, complete
             FROM verification_diff_evidence WHERE evidence_id = ?1",
            params![id_bytes(header.id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    };
    let source = parse_diff_source(
        &read_text(&row, 0)?,
        read_optional_id(&row, 1)?,
        read_optional_id(&row, 2)?,
        read_optional_id(&row, 3)?,
        read_optional_id(&row, 4)?,
    )?;
    let base_snapshot_id = SnapshotId::from_bytes(read_id(&row, 5)?);
    validate_diff_source(
        transaction,
        source,
        worktree_id,
        base_snapshot_id,
        header.snapshot_id,
    )
    .await?;
    let context = StoredDiffEvidenceContext::new(
        header.verification_run_id,
        header.spec_id,
        header.run_id,
        source,
        base_snapshot_id,
        header.snapshot_id,
        read_bool(&row, 6)?,
    );
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    DiffEvidence::from_stored(
        header.id,
        context,
        read_paths(transaction, "verification_diff_paths", header.id).await?,
        read_dependencies(transaction, header.id).await?,
    )
    .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

async fn validate_diff_source(
    transaction: &Transaction,
    source: DiffEvidenceSource,
    worktree_id: WorktreeId,
    base_snapshot_id: SnapshotId,
    current_snapshot_id: SnapshotId,
) -> Result<(), VerificationEvidenceRepositoryError> {
    let DiffEvidenceSource::PublishedIndexes {
        base_index_run_id,
        current_index_run_id,
    } = source
    else {
        return Ok(());
    };
    let mut rows = transaction
        .query(
            "SELECT b.worktree_id, b.snapshot_id, b.run_sequence, b.status,
             c.worktree_id, c.snapshot_id, c.run_sequence, c.status
             FROM index_runs b JOIN index_runs c
             WHERE b.index_run_id = ?1 AND c.index_run_id = ?2",
            params![
                id_bytes(base_index_run_id.as_bytes()),
                id_bytes(current_index_run_id.as_bytes())
            ],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    };
    let valid = WorktreeId::from_bytes(read_id(&row, 0)?) == worktree_id
        && SnapshotId::from_bytes(read_id(&row, 1)?) == base_snapshot_id
        && read_u64(&row, 2)? < read_u64(&row, 6)?
        && read_text(&row, 3)? == "published"
        && WorktreeId::from_bytes(read_id(&row, 4)?) == worktree_id
        && SnapshotId::from_bytes(read_id(&row, 5)?) == current_snapshot_id
        && read_text(&row, 7)? == "published";
    if !valid
        || rows
            .next()
            .await
            .map_err(VerificationEvidenceRepositoryError::Read)?
            .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    Ok(())
}

async fn read_confirmation(
    transaction: &Transaction,
    header: &EvidenceHeader,
) -> Result<UserConfirmationEvidence, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT scope_id, confirmed_at_unix_millis FROM verification_user_confirmations
             WHERE evidence_id = ?1",
            params![id_bytes(header.id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    };
    let confirmation = UserConfirmationEvidence::new(
        header.verification_run_id,
        header.spec_id,
        header.run_id,
        header.snapshot_id,
        PolicyResourceId::from_bytes(read_id(&row, 0)?),
        TaskLedgerTimestamp::from_unix_millis(read_u64(&row, 1)?)
            .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)?,
    );
    if confirmation.id() != header.id
        || rows
            .next()
            .await
            .map_err(VerificationEvidenceRepositoryError::Read)?
            .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
    }
    Ok(confirmation)
}

async fn read_dependencies(
    transaction: &Transaction,
    evidence_id: TaskEvidenceId,
) -> Result<VerificationDependencies, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT item_sequence, repository_path, dependency_state, content_hash
             FROM verification_evidence_dependencies
             WHERE evidence_id = ?1 ORDER BY item_sequence",
            params![id_bytes(evidence_id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let mut dependencies = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    {
        validate_sequence(&row, dependencies.len())?;
        let path = read_path(&row, 1)?;
        dependencies.push(match read_text(&row, 2)?.as_str() {
            "present" => EvidenceDependency::Present(FileRevision::new(
                path,
                ContentHash::from_bytes(read_id(&row, 3)?),
            )),
            "absent" if read_optional_id(&row, 3)?.is_none() => EvidenceDependency::Absent(path),
            _ => return Err(VerificationEvidenceRepositoryError::InvalidStoredData),
        });
    }
    VerificationDependencies::new(dependencies)
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

async fn read_paths(
    transaction: &Transaction,
    table: &str,
    evidence_id: TaskEvidenceId,
) -> Result<Vec<RepositoryPath>, VerificationEvidenceRepositoryError> {
    let sql = format!(
        "SELECT item_sequence, repository_path FROM {table}
         WHERE evidence_id = ?1 ORDER BY item_sequence"
    );
    let mut rows = transaction
        .query(&sql, params![id_bytes(evidence_id.as_bytes())])
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let mut paths = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    {
        validate_sequence(&row, paths.len())?;
        paths.push(read_path(&row, 1)?);
    }
    Ok(paths)
}

async fn validate_latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    published: &PublishedIndex,
) -> Result<(), VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs
             WHERE worktree_id = ?1 AND status = 'published'
             ORDER BY run_sequence DESC LIMIT 1",
            params![id_bytes(worktree_id.as_bytes())],
        )
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
    else {
        return Err(VerificationEvidenceRepositoryError::SnapshotMismatch);
    };
    if IndexRunId::from_bytes(read_id(&row, 0)?) != published.run().id()
        || SnapshotId::from_bytes(read_id(&row, 1)?) != published.run().snapshot_id()
    {
        return Err(VerificationEvidenceRepositoryError::SnapshotMismatch);
    }
    Ok(())
}

pub(crate) struct OperationGuard<'a> {
    started: Instant,
    timeout: Duration,
    control: &'a dyn AgentControllerControl,
}

impl<'a> OperationGuard<'a> {
    pub(crate) fn new(
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> Result<Self, VerificationEvidenceRepositoryError> {
        if timeout.is_zero() {
            return Err(VerificationEvidenceRepositoryError::TimedOut);
        }
        let guard = Self {
            started: Instant::now(),
            timeout,
            control,
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    pub(crate) fn checkpoint(&self) -> Result<(), VerificationEvidenceRepositoryError> {
        if self.control.is_cancelled() {
            return Err(VerificationEvidenceRepositoryError::Cancelled);
        }
        if self.started.elapsed() >= self.timeout {
            return Err(VerificationEvidenceRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn close_write<T>(
    transaction: Transaction,
    result: Result<T, VerificationEvidenceRepositoryError>,
) -> Result<T, VerificationEvidenceRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(VerificationEvidenceRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(VerificationEvidenceRepositoryError::Rollback(source)),
        },
    }
}

async fn close_read<T>(
    transaction: Transaction,
    result: Result<T, VerificationEvidenceRepositoryError>,
) -> Result<T, VerificationEvidenceRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(VerificationEvidenceRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(VerificationEvidenceRepositoryError::Rollback(source)),
        },
    }
}

fn evidence_kind(evidence: &VerificationEvidence) -> &'static str {
    match evidence {
        VerificationEvidence::Command(_) => "command",
        VerificationEvidence::Test(_) => "test",
        VerificationEvidence::Diff(_) => "diff_invariant",
        VerificationEvidence::Diagnostic(_) => "diagnostic",
        VerificationEvidence::UserConfirmation(_) => "user_confirm",
        // This constrained V28 projection has no operational command child.
        VerificationEvidence::LoopbackHttp(_) => "command",
    }
}

struct DiffSourceFields {
    kind: &'static str,
    action_digest: Option<Vec<u8>>,
    policy_decision_id: Option<Vec<u8>>,
    base_index_run_id: Option<Vec<u8>>,
    current_index_run_id: Option<Vec<u8>>,
}

fn diff_source_fields(source: DiffEvidenceSource) -> DiffSourceFields {
    match source {
        DiffEvidenceSource::Patch {
            action_digest,
            policy_decision_id,
        } => DiffSourceFields {
            kind: "patch",
            action_digest: Some(action_digest.as_bytes().to_vec()),
            policy_decision_id: Some(id_bytes(policy_decision_id.as_bytes())),
            base_index_run_id: None,
            current_index_run_id: None,
        },
        DiffEvidenceSource::PublishedIndexes {
            base_index_run_id,
            current_index_run_id,
        } => DiffSourceFields {
            kind: "published_indexes",
            action_digest: None,
            policy_decision_id: None,
            base_index_run_id: Some(id_bytes(base_index_run_id.as_bytes())),
            current_index_run_id: Some(id_bytes(current_index_run_id.as_bytes())),
        },
    }
}

fn parse_diff_source(
    kind: &str,
    action_digest: Option<[u8; 32]>,
    policy_decision_id: Option<[u8; 32]>,
    base_index_run_id: Option<[u8; 32]>,
    current_index_run_id: Option<[u8; 32]>,
) -> Result<DiffEvidenceSource, VerificationEvidenceRepositoryError> {
    match (
        kind,
        action_digest,
        policy_decision_id,
        base_index_run_id,
        current_index_run_id,
    ) {
        ("patch", Some(action), Some(policy), None, None) => Ok(DiffEvidenceSource::Patch {
            action_digest: PatchActionDigest::from_bytes(action),
            policy_decision_id: PolicyDecisionId::from_bytes(policy),
        }),
        ("published_indexes", None, None, Some(base), Some(current)) if base != current => {
            Ok(DiffEvidenceSource::PublishedIndexes {
                base_index_run_id: IndexRunId::from_bytes(base),
                current_index_run_id: IndexRunId::from_bytes(current),
            })
        }
        _ => Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    }
}

fn termination_fields(termination: ProcessTermination) -> (&'static str, Option<i64>) {
    match termination {
        ProcessTermination::Exited(exit) => ("exited", exit.code().map(i64::from)),
        ProcessTermination::TimedOut => ("timed_out", None),
        ProcessTermination::Cancelled => ("cancelled", None),
    }
}

fn parse_termination(
    kind: &str,
    code: Option<i32>,
) -> Result<ProcessTermination, VerificationEvidenceRepositoryError> {
    match (kind, code) {
        ("exited", code) => ProcessExit::new(code, code == Some(0))
            .map(ProcessTermination::Exited)
            .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData),
        ("timed_out", None) => Ok(ProcessTermination::TimedOut),
        ("cancelled", None) => Ok(ProcessTermination::Cancelled),
        _ => Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    }
}

const fn redaction_text(redaction: Option<ProcessOutputRedaction>) -> Option<&'static str> {
    match redaction {
        None => None,
        Some(ProcessOutputRedaction::InvalidUtf8) => Some("invalid_utf8"),
        Some(ProcessOutputRedaction::SecretCandidate) => Some("secret_candidate"),
        Some(ProcessOutputRedaction::UnsafeControl) => Some("unsafe_control"),
    }
}

fn parse_redaction(
    value: Option<&str>,
) -> Result<Option<ProcessOutputRedaction>, VerificationEvidenceRepositoryError> {
    match value {
        None => Ok(None),
        Some("invalid_utf8") => Ok(Some(ProcessOutputRedaction::InvalidUtf8)),
        Some("secret_candidate") => Ok(Some(ProcessOutputRedaction::SecretCandidate)),
        Some("unsafe_control") => Ok(Some(ProcessOutputRedaction::UnsafeControl)),
        _ => Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    }
}

const fn test_outcome_text(outcome: TestCaseOutcome) -> &'static str {
    match outcome {
        TestCaseOutcome::Passed => "passed",
        TestCaseOutcome::Failed => "failed",
        TestCaseOutcome::Ignored => "ignored",
    }
}

fn parse_test_outcome(value: &str) -> Result<TestCaseOutcome, VerificationEvidenceRepositoryError> {
    match value {
        "passed" => Ok(TestCaseOutcome::Passed),
        "failed" => Ok(TestCaseOutcome::Failed),
        "ignored" => Ok(TestCaseOutcome::Ignored),
        _ => Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    }
}

fn validate_sequence(
    row: &libsql::Row,
    index: usize,
) -> Result<(), VerificationEvidenceRepositoryError> {
    if read_i64(row, 0)? == sequence_to_i64(index)? {
        Ok(())
    } else {
        Err(VerificationEvidenceRepositoryError::InvalidStoredData)
    }
}

fn read_path(
    row: &libsql::Row,
    index: i32,
) -> Result<RepositoryPath, VerificationEvidenceRepositoryError> {
    let value: Vec<u8> = row
        .get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    RepositoryPath::try_from_bytes(value)
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], VerificationEvidenceRepositoryError> {
    let value: Vec<u8> = row
        .get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    value
        .try_into()
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, VerificationEvidenceRepositoryError> {
    let value: Option<Vec<u8>> = row
        .get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    value
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, VerificationEvidenceRepositoryError> {
    row.get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, VerificationEvidenceRepositoryError> {
    row.get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, VerificationEvidenceRepositoryError> {
    row.get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)
}

fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, VerificationEvidenceRepositoryError> {
    u64::try_from(read_i64(row, index)?)
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, VerificationEvidenceRepositoryError> {
    u32::try_from(read_i64(row, index)?)
        .map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
}

fn read_optional_i32(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<i32>, VerificationEvidenceRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(VerificationEvidenceRepositoryError::Read)?;
    value
        .map(|value| {
            i32::try_from(value).map_err(|_| VerificationEvidenceRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, VerificationEvidenceRepositoryError> {
    match read_i64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(VerificationEvidenceRepositoryError::InvalidStoredData),
    }
}

fn id_bytes(bytes: &[u8; 32]) -> Vec<u8> {
    bytes.to_vec()
}

const fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn u64_to_i64(value: u64) -> Result<i64, VerificationEvidenceRepositoryError> {
    i64::try_from(value).map_err(|_| VerificationEvidenceRepositoryError::ResourceLimit)
}

fn sequence_to_i64(index: usize) -> Result<i64, VerificationEvidenceRepositoryError> {
    index
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(VerificationEvidenceRepositoryError::ResourceLimit)
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_write(source: libsql::Error) -> VerificationEvidenceRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        VerificationEvidenceRepositoryError::InvalidStoredData
    } else {
        VerificationEvidenceRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum VerificationEvidenceRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredData,
    ResourceLimit,
    ProjectMismatch,
    SnapshotMismatch,
    EvidenceConflict,
    Cancelled,
    TimedOut,
}

impl VerificationEvidenceRepositoryError {
    pub(crate) fn classify(&self) -> VerificationEvidenceStoreFailure {
        match self {
            Self::InvalidStoredData | Self::ResourceLimit => {
                VerificationEvidenceStoreFailure::InvalidStoredData
            }
            Self::SnapshotMismatch => VerificationEvidenceStoreFailure::SnapshotMismatch,
            Self::ProjectMismatch => VerificationEvidenceStoreFailure::InvalidStoredData,
            Self::EvidenceConflict => VerificationEvidenceStoreFailure::EvidenceConflict,
            Self::Cancelled => VerificationEvidenceStoreFailure::Cancelled,
            Self::TimedOut => VerificationEvidenceStoreFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    VerificationEvidenceStoreFailure::Corrupt
                } else {
                    VerificationEvidenceStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for VerificationEvidenceRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "verification evidence transaction could not begin",
            Self::Read(_) => "verification evidence could not be read",
            Self::Write(_) => "verification evidence could not be written",
            Self::Commit(_) => "verification evidence transaction could not commit",
            Self::Rollback(_) => "verification evidence transaction could not roll back",
            Self::InvalidStoredData => "verification evidence is invalid",
            Self::ResourceLimit => "verification evidence exceeds a fixed boundary",
            Self::ProjectMismatch => "verification evidence belongs to another task or worktree",
            Self::SnapshotMismatch => "verification state does not match the requested snapshot",
            Self::EvidenceConflict => "verification evidence identity conflicts",
            Self::Cancelled => "verification evidence operation was cancelled",
            Self::TimedOut => "verification evidence operation timed out",
        })
    }
}

impl std::error::Error for VerificationEvidenceRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            _ => None,
        }
    }
}
