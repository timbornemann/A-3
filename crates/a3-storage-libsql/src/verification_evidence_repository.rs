use crate::catalog::is_corruption;
use a3_application::{
    AgentControllerControl, StoredVerificationState, VerificationEvidenceStoreFailure,
};
use a3_domain::{
    AgentRunId, CommandEvidence, CommandEvidenceContext, ContentHash, DiagnosticCount,
    DiagnosticEvidence, DiffEvidence, DiffEvidenceSource, EvidenceDependency, FileRevision,
    IndexRunId, PatchActionDigest, PolicyDecisionId, PolicyResourceId, ProcessDuration,
    ProcessExit, ProcessOutputDigest, ProcessOutputRedaction, ProcessStreamEvidence,
    ProcessTermination, PublishedIndex, RepositoryPath, SnapshotId, StoredDiffEvidenceContext,
    StoredProcessEvidence, TaskEvidenceId, TaskId, TaskLedgerTimestamp, TestCaseEvidence,
    TestCaseName, TestCaseOutcome, TestEvidence, ToolRunId, UserConfirmationEvidence,
    VerificationDependencies, VerificationEvidence, VerificationRunId, VerificationSpecId,
    WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::fmt;
use std::time::{Duration, Instant};

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn append(
    connection: &Connection,
    worktree_id: WorktreeId,
    evidence: &VerificationEvidence,
    timeout: Duration,
    control: &dyn AgentControllerControl,
) -> Result<(), VerificationEvidenceRepositoryError> {
    let guard = OperationGuard::new(timeout, control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(VerificationEvidenceRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let task_id = evidence_task_id(
            &transaction,
            worktree_id,
            evidence.spec_id(),
            evidence.run_id(),
        )
        .await?;
        if let Some(existing) =
            read_evidence(&transaction, worktree_id, task_id, evidence.id()).await?
        {
            return if &existing == evidence {
                guard.checkpoint()?;
                Ok(())
            } else {
                Err(VerificationEvidenceRepositoryError::EvidenceConflict)
            };
        }
        transaction
            .execute(
                "INSERT INTO verification_evidence (
                 evidence_id, task_id, worktree_id, evidence_kind, schema_version,
                 verification_run_id, verification_spec_id, run_id, snapshot_id
                 ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)",
                params![
                    id_bytes(evidence.id().as_bytes()),
                    id_bytes(task_id.as_bytes()),
                    id_bytes(worktree_id.as_bytes()),
                    evidence_kind(evidence),
                    id_bytes(evidence.verification_run_id().as_bytes()),
                    id_bytes(evidence.spec_id().as_bytes()),
                    id_bytes(evidence.run_id().as_bytes()),
                    id_bytes(evidence.snapshot_id().as_bytes())
                ],
            )
            .await
            .map_err(classify_write)?;
        write_variant(&transaction, evidence, &guard).await?;
        guard.checkpoint()
    }
    .await;
    close_write(transaction, result).await
}

pub(crate) struct VerificationStateQuery<'a> {
    worktree_id: WorktreeId,
    task_id: TaskId,
    evidence_ids: &'a [TaskEvidenceId],
    expected_snapshot_id: SnapshotId,
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
            expected_snapshot_id,
            published,
            timeout,
        }
    }
}

pub(crate) async fn load_state(
    connection: &Connection,
    query: VerificationStateQuery<'_>,
    control: &dyn AgentControllerControl,
) -> Result<StoredVerificationState, VerificationEvidenceRepositoryError> {
    let guard = OperationGuard::new(query.timeout, control)?;
    if query.published.publication().graph().snapshot_id() != query.expected_snapshot_id {
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
                read_evidence(&transaction, query.worktree_id, query.task_id, *evidence_id)
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
) -> Result<Option<VerificationEvidence>, VerificationEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT evidence_kind, verification_run_id, verification_spec_id, run_id, snapshot_id
             FROM verification_evidence
             WHERE evidence_id = ?1 AND task_id = ?2 AND worktree_id = ?3",
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
    if rows
        .next()
        .await
        .map_err(VerificationEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(VerificationEvidenceRepositoryError::InvalidStoredData);
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
    Ok(Some(evidence))
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

struct OperationGuard<'a> {
    started: Instant,
    timeout: Duration,
    control: &'a dyn AgentControllerControl,
}

impl<'a> OperationGuard<'a> {
    fn new(
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

    fn checkpoint(&self) -> Result<(), VerificationEvidenceRepositoryError> {
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
