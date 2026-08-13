use crate::{catalog::is_corruption, run_journal_repository};
use a3_application::{EvaluatedPolicyAction, PolicyStoreFailure};
use a3_domain::{
    ActionClass, AgentRun, AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalGrantState,
    ApprovalId, ApprovalRequest, ApprovalRequestId, PolicyActionFingerprint, PolicyDecision,
    PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason, PolicyEvaluationTiming,
    PolicyScopeDigest, RiskLevel, RunEvent, RunEventCode, RunEventKind, RunEventOutcome,
    RunEventSequence, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn record_evaluation(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    evaluation: &EvaluatedPolicyAction,
) -> Result<(), PolicyRepositoryError> {
    validate_evaluation(run, evaluation)?;
    let transaction = begin(connection).await?;
    let result = async {
        if row_exists(
            &transaction,
            "SELECT 1 FROM policy_decisions WHERE policy_decision_id = ?1",
            evaluation.decision().id().as_bytes(),
        )
        .await?
        {
            return Err(PolicyRepositoryError::AlreadyExists);
        }
        if let Some(request) = evaluation.approval_request()
            && row_exists(
                &transaction,
                "SELECT 1 FROM approval_requests WHERE approval_request_id = ?1",
                request.id().as_bytes(),
            )
            .await?
        {
            return Err(PolicyRepositoryError::AlreadyExists);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            evaluation.event(),
        )
        .await
        .map_err(PolicyRepositoryError::RunJournal)?;
        if let Some(request) = evaluation.approval_request() {
            write_request(&transaction, request).await?;
        }
        write_decision(&transaction, evaluation.decision(), evaluation.event()).await?;
        if let Some(approval_id) = evaluation.decision().approval_id() {
            let changed = transaction
                .execute(
                    "UPDATE approval_grants SET status = 'consumed',
                     consumed_decision_id = ?1, consumed_at_unix_millis = ?2
                     WHERE approval_id = ?3 AND run_id = ?4 AND status = 'active'",
                    params![
                        id_bytes(evaluation.decision().id()),
                        timestamp_to_i64(evaluation.decision().timing().decided_at())?,
                        id_bytes(approval_id),
                        id_bytes(run.id())
                    ],
                )
                .await
                .map_err(classify_write)?;
            if changed != 1 {
                return Err(PolicyRepositoryError::ApprovalConflict);
            }
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn load_request(
    connection: &Connection,
    request_id: ApprovalRequestId,
) -> Result<Option<ApprovalRequest>, PolicyRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT approval_request_id, run_id, action_fingerprint, scope_digest,
             action_class, risk_level, requested_at_unix_millis, expires_at_unix_millis
             FROM approval_requests WHERE approval_request_id = ?1",
            params![id_bytes(request_id)],
        )
        .await
        .map_err(PolicyRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(PolicyRepositoryError::Read)? else {
        return Ok(None);
    };
    let request = read_request(&row)?;
    if rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some()
    {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    Ok(Some(request))
}

pub(crate) async fn load_approval(
    connection: &Connection,
    approval_id: ApprovalId,
) -> Result<Option<ApprovalGrant>, PolicyRepositoryError> {
    load_approval_by_column(connection, "g.approval_id", approval_id.as_bytes().to_vec()).await
}

pub(crate) async fn load_approval_for_request(
    connection: &Connection,
    request_id: ApprovalRequestId,
) -> Result<Option<ApprovalGrant>, PolicyRepositoryError> {
    load_approval_by_column(
        connection,
        "g.approval_request_id",
        request_id.as_bytes().to_vec(),
    )
    .await
}

async fn load_approval_by_column(
    connection: &Connection,
    column: &str,
    identity: Vec<u8>,
) -> Result<Option<ApprovalGrant>, PolicyRepositoryError> {
    let query = format!(
        "SELECT g.approval_id, g.approval_request_id, g.run_id,
         r.action_fingerprint, r.scope_digest, r.action_class, r.risk_level,
         r.requested_at_unix_millis, r.expires_at_unix_millis,
         g.granted_at_unix_millis, g.expires_at_unix_millis, g.status,
         g.consumed_decision_id, g.consumed_at_unix_millis, g.revoked_at_unix_millis
         FROM approval_grants AS g JOIN approval_requests AS r
           ON r.approval_request_id = g.approval_request_id
         WHERE {column} = ?1"
    );
    let mut rows = connection
        .query(&query, params![identity])
        .await
        .map_err(PolicyRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(PolicyRepositoryError::Read)? else {
        return Ok(None);
    };
    let request = ApprovalRequest::reconstruct(
        ApprovalRequestId::from_bytes(read_id(&row, 1)?),
        AgentRunId::from_bytes(read_id(&row, 2)?),
        PolicyActionFingerprint::from_bytes(read_id(&row, 3)?),
        PolicyScopeDigest::from_bytes(read_id(&row, 4)?),
        read_action_class(&row, 5)?,
        read_risk(&row, 6)?,
        read_timestamp(&row, 7)?,
        read_timestamp(&row, 8)?,
    )
    .map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
    let grant_expiry = read_timestamp(&row, 10)?;
    if request.expires_at() != grant_expiry {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    let state_text: String = row.get(11).map_err(PolicyRepositoryError::Read)?;
    let consumed_decision_id = read_optional_id(&row, 12)?;
    let consumed_at = read_optional_timestamp(&row, 13)?;
    let revoked_at = read_optional_timestamp(&row, 14)?;
    let state = match (
        state_text.as_str(),
        consumed_decision_id,
        consumed_at,
        revoked_at,
    ) {
        ("active", None, None, None) => ApprovalGrantState::Active,
        ("consumed", Some(decision_id), Some(consumed_at), None) => ApprovalGrantState::Consumed {
            decision_id: PolicyDecisionId::from_bytes(decision_id),
            consumed_at,
        },
        ("revoked", None, None, Some(revoked_at)) => ApprovalGrantState::Revoked { revoked_at },
        _ => return Err(PolicyRepositoryError::InvalidStoredData),
    };
    let approval = ApprovalGrant::reconstruct(
        ApprovalId::from_bytes(read_id(&row, 0)?),
        request.id(),
        request.run_id(),
        request.action_fingerprint(),
        request.scope_digest(),
        request.action_class(),
        request.risk_level(),
        read_timestamp(&row, 9)?,
        grant_expiry,
        state,
    )
    .map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
    if rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some()
    {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    Ok(Some(approval))
}

pub(crate) async fn load_decision(
    connection: &Connection,
    decision_id: PolicyDecisionId,
) -> Result<Option<PolicyDecision>, PolicyRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT policy_decision_id, run_id, action_fingerprint, scope_digest,
             action_class, risk_level, outcome, reason, approval_request_id, approval_id,
             started_at_unix_millis, decided_at_unix_millis, duration_millis
             FROM policy_decisions WHERE policy_decision_id = ?1",
            params![id_bytes(decision_id)],
        )
        .await
        .map_err(PolicyRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(PolicyRepositoryError::Read)? else {
        return Ok(None);
    };
    let timing = PolicyEvaluationTiming::new(read_timestamp(&row, 10)?, read_timestamp(&row, 11)?)
        .map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
    let duration: i64 = row.get(12).map_err(PolicyRepositoryError::Read)?;
    if u64::try_from(duration).ok() != Some(timing.duration_millis()) {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    let decision = PolicyDecision::reconstruct(
        PolicyDecisionId::from_bytes(read_id(&row, 0)?),
        AgentRunId::from_bytes(read_id(&row, 1)?),
        PolicyActionFingerprint::from_bytes(read_id(&row, 2)?),
        PolicyScopeDigest::from_bytes(read_id(&row, 3)?),
        read_action_class(&row, 4)?,
        read_risk(&row, 5)?,
        read_outcome(&row, 6)?,
        read_reason(&row, 7)?,
        read_optional_id(&row, 8)?.map(ApprovalRequestId::from_bytes),
        read_optional_id(&row, 9)?.map(ApprovalId::from_bytes),
        timing,
    )
    .map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
    if rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some()
    {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    Ok(Some(decision))
}

pub(crate) async fn grant(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    approval: &ApprovalGrant,
    event: &RunEvent,
) -> Result<(), PolicyRepositoryError> {
    validate_grant(run, approval, event)?;
    let transaction = begin(connection).await?;
    let result = async {
        let request = load_request_from_transaction(&transaction, approval.request_id())
            .await?
            .ok_or(PolicyRepositoryError::NotFound)?;
        if approval.run_id() != request.run_id()
            || approval.action_fingerprint() != request.action_fingerprint()
            || approval.scope_digest() != request.scope_digest()
            || approval.action_class() != request.action_class()
            || approval.risk_level() != request.risk_level()
            || approval.expires_at() != request.expires_at()
            || approval.granted_at() < request.requested_at()
            || approval.granted_at() >= request.expires_at()
        {
            return Err(PolicyRepositoryError::InvalidInput);
        }
        if row_exists_two(
            &transaction,
            "SELECT 1 FROM approval_grants WHERE approval_id = ?1 OR approval_request_id = ?2",
            approval.id().as_bytes(),
            approval.request_id().as_bytes(),
        )
        .await?
        {
            return Err(PolicyRepositoryError::AlreadyExists);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(PolicyRepositoryError::RunJournal)?;
        transaction
            .execute(
                "INSERT INTO approval_grants (
                 approval_id, approval_request_id, run_id, granted_by, granted_event_id,
                 granted_at_unix_millis, expires_at_unix_millis, status,
                 consumed_decision_id, consumed_at_unix_millis, revoked_at_unix_millis,
                 revoked_by, revoked_event_id
                 ) VALUES (?1, ?2, ?3, 'user', ?4, ?5, ?6, 'active',
                 NULL, NULL, NULL, NULL, NULL)",
                params![
                    id_bytes(approval.id()),
                    id_bytes(approval.request_id()),
                    id_bytes(approval.run_id()),
                    id_bytes(event.id()),
                    timestamp_to_i64(approval.granted_at())?,
                    timestamp_to_i64(approval.expires_at())?
                ],
            )
            .await
            .map_err(classify_write)?;
        Ok(())
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn revoke(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_last_sequence: RunEventSequence,
    run: &AgentRun,
    expected_state: ApprovalGrantState,
    approval: &ApprovalGrant,
    event: &RunEvent,
) -> Result<(), PolicyRepositoryError> {
    let ApprovalGrantState::Revoked { revoked_at } = approval.state() else {
        return Err(PolicyRepositoryError::InvalidInput);
    };
    if expected_state != ApprovalGrantState::Active {
        return Err(PolicyRepositoryError::ApprovalConflict);
    }
    validate_approval_event(
        run,
        approval.run_id(),
        event,
        revoked_at,
        RunEventOutcome::Cancelled,
    )?;
    let transaction = begin(connection).await?;
    let result = async {
        if !row_exists(
            &transaction,
            "SELECT 1 FROM approval_grants WHERE approval_id = ?1",
            approval.id().as_bytes(),
        )
        .await?
        {
            return Err(PolicyRepositoryError::NotFound);
        }
        run_journal_repository::append_in_transaction(
            &transaction,
            worktree_id,
            expected_last_sequence,
            run,
            event,
        )
        .await
        .map_err(PolicyRepositoryError::RunJournal)?;
        let changed = transaction
            .execute(
                "UPDATE approval_grants SET status = 'revoked', revoked_at_unix_millis = ?1,
                 revoked_by = 'user', revoked_event_id = ?2
                 WHERE approval_id = ?3 AND run_id = ?4 AND approval_request_id = ?5
                   AND granted_at_unix_millis = ?6 AND expires_at_unix_millis = ?7
                   AND status = 'active'",
                params![
                    timestamp_to_i64(revoked_at)?,
                    id_bytes(event.id()),
                    id_bytes(approval.id()),
                    id_bytes(run.id()),
                    id_bytes(approval.request_id()),
                    timestamp_to_i64(approval.granted_at())?,
                    timestamp_to_i64(approval.expires_at())?
                ],
            )
            .await
            .map_err(classify_write)?;
        if changed != 1 {
            return Err(PolicyRepositoryError::ApprovalConflict);
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

fn validate_evaluation(
    run: &AgentRun,
    evaluation: &EvaluatedPolicyAction,
) -> Result<(), PolicyRepositoryError> {
    let decision = evaluation.decision();
    let event = evaluation.event();
    if decision.run_id() != run.id()
        || event.run_id() != run.id()
        || event.occurred_at() != decision.timing().decided_at()
        || event.snapshot_id() != run.current_snapshot_id()
        || event.kind() != RunEventKind::ApprovalRecorded
        || event.payload().code() != RunEventCode::PolicyDecision
        || event.payload().redaction().is_some()
        || event.subject().is_some()
        || event.turn_charge().is_some()
    {
        return Err(PolicyRepositoryError::InvalidInput);
    }
    let expected_outcome = match decision.outcome() {
        PolicyDecisionOutcome::Allowed => RunEventOutcome::Succeeded,
        PolicyDecisionOutcome::ApprovalRequired | PolicyDecisionOutcome::Denied => {
            RunEventOutcome::Denied
        }
    };
    if event.payload().outcome() != Some(expected_outcome) {
        return Err(PolicyRepositoryError::InvalidInput);
    }
    match (
        evaluation.approval_request(),
        decision.approval_request_id(),
    ) {
        (None, None) => Ok(()),
        (Some(request), Some(request_id))
            if request.id() == request_id
                && request.run_id() == decision.run_id()
                && request.action_fingerprint() == decision.action_fingerprint()
                && request.scope_digest() == decision.scope_digest()
                && request.action_class() == decision.action_class()
                && request.risk_level() == decision.risk_level()
                && request.requested_at() == decision.timing().decided_at() =>
        {
            Ok(())
        }
        _ => Err(PolicyRepositoryError::InvalidInput),
    }
}

fn validate_grant(
    run: &AgentRun,
    approval: &ApprovalGrant,
    event: &RunEvent,
) -> Result<(), PolicyRepositoryError> {
    if approval.state() != ApprovalGrantState::Active {
        return Err(PolicyRepositoryError::InvalidInput);
    }
    validate_approval_event(
        run,
        approval.run_id(),
        event,
        approval.granted_at(),
        RunEventOutcome::Succeeded,
    )
}

fn validate_approval_event(
    run: &AgentRun,
    approval_run_id: AgentRunId,
    event: &RunEvent,
    occurred_at: AgentRunTimestamp,
    outcome: RunEventOutcome,
) -> Result<(), PolicyRepositoryError> {
    if approval_run_id != run.id()
        || event.run_id() != run.id()
        || event.snapshot_id() != run.current_snapshot_id()
        || event.occurred_at() != occurred_at
        || event.kind() != RunEventKind::ApprovalRecorded
        || event.payload().code() != RunEventCode::UserRequest
        || event.payload().outcome() != Some(outcome)
        || event.payload().redaction().is_some()
        || event.subject().is_some()
        || event.turn_charge().is_some()
    {
        return Err(PolicyRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn write_request(
    transaction: &Transaction,
    request: &ApprovalRequest,
) -> Result<(), PolicyRepositoryError> {
    transaction
        .execute(
            "INSERT INTO approval_requests (
             approval_request_id, run_id, requested_by, action_fingerprint, scope_digest,
             action_class, risk_level, requested_at_unix_millis, expires_at_unix_millis
             ) VALUES (?1, ?2, 'controller', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id_bytes(request.id()),
                id_bytes(request.run_id()),
                request.action_fingerprint().as_bytes().to_vec(),
                request.scope_digest().as_bytes().to_vec(),
                action_class_text(request.action_class()),
                risk_text(request.risk_level()),
                timestamp_to_i64(request.requested_at())?,
                timestamp_to_i64(request.expires_at())?
            ],
        )
        .await
        .map_err(classify_write)?;
    Ok(())
}

async fn write_decision(
    transaction: &Transaction,
    decision: &PolicyDecision,
    event: &RunEvent,
) -> Result<(), PolicyRepositoryError> {
    transaction
        .execute(
            "INSERT INTO policy_decisions (
             policy_decision_id, run_id, event_id, actor, action_fingerprint, scope_digest,
             action_class, risk_level, outcome, reason, approval_request_id, approval_id,
             started_at_unix_millis, decided_at_unix_millis, duration_millis
             ) VALUES (?1, ?2, ?3, 'controller', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id_bytes(decision.id()),
                id_bytes(decision.run_id()),
                id_bytes(event.id()),
                decision.action_fingerprint().as_bytes().to_vec(),
                decision.scope_digest().as_bytes().to_vec(),
                action_class_text(decision.action_class()),
                risk_text(decision.risk_level()),
                outcome_text(decision.outcome()),
                reason_text(decision.reason()),
                decision.approval_request_id().map(id_bytes),
                decision.approval_id().map(id_bytes),
                timestamp_to_i64(decision.timing().started_at())?,
                timestamp_to_i64(decision.timing().decided_at())?,
                u64_to_i64(decision.timing().duration_millis())?
            ],
        )
        .await
        .map_err(classify_write)?;
    Ok(())
}

fn read_request(row: &libsql::Row) -> Result<ApprovalRequest, PolicyRepositoryError> {
    ApprovalRequest::reconstruct(
        ApprovalRequestId::from_bytes(read_id(row, 0)?),
        AgentRunId::from_bytes(read_id(row, 1)?),
        PolicyActionFingerprint::from_bytes(read_id(row, 2)?),
        PolicyScopeDigest::from_bytes(read_id(row, 3)?),
        read_action_class(row, 4)?,
        read_risk(row, 5)?,
        read_timestamp(row, 6)?,
        read_timestamp(row, 7)?,
    )
    .map_err(|_| PolicyRepositoryError::InvalidStoredData)
}

async fn begin(connection: &Connection) -> Result<Transaction, PolicyRepositoryError> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(PolicyRepositoryError::Begin)
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, PolicyRepositoryError>,
) -> Result<T, PolicyRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(PolicyRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(PolicyRepositoryError::Rollback(source)),
        },
    }
}

async fn row_exists(
    transaction: &Transaction,
    sql: &str,
    id: &[u8; 32],
) -> Result<bool, PolicyRepositoryError> {
    let mut rows = transaction
        .query(sql, params![id.to_vec()])
        .await
        .map_err(PolicyRepositoryError::Read)?;
    Ok(rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some())
}

async fn row_exists_two(
    transaction: &Transaction,
    sql: &str,
    first: &[u8; 32],
    second: &[u8; 32],
) -> Result<bool, PolicyRepositoryError> {
    let mut rows = transaction
        .query(sql, params![first.to_vec(), second.to_vec()])
        .await
        .map_err(PolicyRepositoryError::Read)?;
    Ok(rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some())
}

async fn load_request_from_transaction(
    transaction: &Transaction,
    request_id: ApprovalRequestId,
) -> Result<Option<ApprovalRequest>, PolicyRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT approval_request_id, run_id, action_fingerprint, scope_digest,
             action_class, risk_level, requested_at_unix_millis, expires_at_unix_millis
             FROM approval_requests WHERE approval_request_id = ?1",
            params![id_bytes(request_id)],
        )
        .await
        .map_err(PolicyRepositoryError::Read)?;
    let request = rows.next().await.map_err(PolicyRepositoryError::Read)?;
    let request = request.as_ref().map(read_request).transpose()?;
    if rows
        .next()
        .await
        .map_err(PolicyRepositoryError::Read)?
        .is_some()
    {
        return Err(PolicyRepositoryError::InvalidStoredData);
    }
    Ok(request)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], PolicyRepositoryError> {
    let value: Vec<u8> = row.get(index).map_err(PolicyRepositoryError::Read)?;
    value
        .try_into()
        .map_err(|_| PolicyRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, PolicyRepositoryError> {
    let value: Option<Vec<u8>> = row.get(index).map_err(PolicyRepositoryError::Read)?;
    value
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| PolicyRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<AgentRunTimestamp, PolicyRepositoryError> {
    let value: i64 = row.get(index).map_err(PolicyRepositoryError::Read)?;
    let value = u64::try_from(value).map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(value).map_err(|_| PolicyRepositoryError::InvalidStoredData)
}

fn read_optional_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<AgentRunTimestamp>, PolicyRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(PolicyRepositoryError::Read)?;
    value
        .map(|value| {
            let value =
                u64::try_from(value).map_err(|_| PolicyRepositoryError::InvalidStoredData)?;
            AgentRunTimestamp::from_unix_millis(value)
                .map_err(|_| PolicyRepositoryError::InvalidStoredData)
        })
        .transpose()
}

fn read_action_class(row: &libsql::Row, index: i32) -> Result<ActionClass, PolicyRepositoryError> {
    let value: String = row.get(index).map_err(PolicyRepositoryError::Read)?;
    match value.as_str() {
        "read" => Ok(ActionClass::Read),
        "derive" => Ok(ActionClass::Derive),
        "write" => Ok(ActionClass::Write),
        "execute_safe" => Ok(ActionClass::ExecuteSafe),
        "execute_open" => Ok(ActionClass::ExecuteOpen),
        "network" => Ok(ActionClass::Network),
        "destructive" => Ok(ActionClass::Destructive),
        "publish" => Ok(ActionClass::Publish),
        "outside_root" => Ok(ActionClass::OutsideRoot),
        _ => Err(PolicyRepositoryError::InvalidStoredData),
    }
}

fn read_risk(row: &libsql::Row, index: i32) -> Result<RiskLevel, PolicyRepositoryError> {
    let value: String = row.get(index).map_err(PolicyRepositoryError::Read)?;
    match value.as_str() {
        "low" => Ok(RiskLevel::Low),
        "moderate" => Ok(RiskLevel::Moderate),
        "high" => Ok(RiskLevel::High),
        "critical" => Ok(RiskLevel::Critical),
        _ => Err(PolicyRepositoryError::InvalidStoredData),
    }
}

fn read_outcome(
    row: &libsql::Row,
    index: i32,
) -> Result<PolicyDecisionOutcome, PolicyRepositoryError> {
    let value: String = row.get(index).map_err(PolicyRepositoryError::Read)?;
    match value.as_str() {
        "allowed" => Ok(PolicyDecisionOutcome::Allowed),
        "approval_required" => Ok(PolicyDecisionOutcome::ApprovalRequired),
        "denied" => Ok(PolicyDecisionOutcome::Denied),
        _ => Err(PolicyRepositoryError::InvalidStoredData),
    }
}

fn read_reason(
    row: &libsql::Row,
    index: i32,
) -> Result<PolicyDecisionReason, PolicyRepositoryError> {
    let value: String = row.get(index).map_err(PolicyRepositoryError::Read)?;
    match value.as_str() {
        "system_automatic" => Ok(PolicyDecisionReason::SystemAutomatic),
        "system_approval_required" => Ok(PolicyDecisionReason::SystemApprovalRequired),
        "workspace_approval_required" => Ok(PolicyDecisionReason::WorkspaceApprovalRequired),
        "workspace_denied" => Ok(PolicyDecisionReason::WorkspaceDenied),
        "approval_granted" => Ok(PolicyDecisionReason::ApprovalGranted),
        "approval_run_mismatch" => Ok(PolicyDecisionReason::ApprovalRunMismatch),
        "approval_scope_mismatch" => Ok(PolicyDecisionReason::ApprovalScopeMismatch),
        "approval_action_mismatch" => Ok(PolicyDecisionReason::ApprovalActionMismatch),
        "approval_expired" => Ok(PolicyDecisionReason::ApprovalExpired),
        "approval_revoked" => Ok(PolicyDecisionReason::ApprovalRevoked),
        "approval_already_consumed" => Ok(PolicyDecisionReason::ApprovalAlreadyConsumed),
        "approval_timestamp_regressed" => Ok(PolicyDecisionReason::ApprovalTimestampRegressed),
        _ => Err(PolicyRepositoryError::InvalidStoredData),
    }
}

const fn action_class_text(value: ActionClass) -> &'static str {
    match value {
        ActionClass::Read => "read",
        ActionClass::Derive => "derive",
        ActionClass::Write => "write",
        ActionClass::ExecuteSafe => "execute_safe",
        ActionClass::ExecuteOpen => "execute_open",
        ActionClass::Network => "network",
        ActionClass::Destructive => "destructive",
        ActionClass::Publish => "publish",
        ActionClass::OutsideRoot => "outside_root",
    }
}

const fn risk_text(value: RiskLevel) -> &'static str {
    match value {
        RiskLevel::Low => "low",
        RiskLevel::Moderate => "moderate",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

const fn outcome_text(value: PolicyDecisionOutcome) -> &'static str {
    match value {
        PolicyDecisionOutcome::Allowed => "allowed",
        PolicyDecisionOutcome::ApprovalRequired => "approval_required",
        PolicyDecisionOutcome::Denied => "denied",
    }
}

const fn reason_text(value: PolicyDecisionReason) -> &'static str {
    match value {
        PolicyDecisionReason::SystemAutomatic => "system_automatic",
        PolicyDecisionReason::SystemApprovalRequired => "system_approval_required",
        PolicyDecisionReason::WorkspaceApprovalRequired => "workspace_approval_required",
        PolicyDecisionReason::WorkspaceDenied => "workspace_denied",
        PolicyDecisionReason::ApprovalGranted => "approval_granted",
        PolicyDecisionReason::ApprovalRunMismatch => "approval_run_mismatch",
        PolicyDecisionReason::ApprovalScopeMismatch => "approval_scope_mismatch",
        PolicyDecisionReason::ApprovalActionMismatch => "approval_action_mismatch",
        PolicyDecisionReason::ApprovalExpired => "approval_expired",
        PolicyDecisionReason::ApprovalRevoked => "approval_revoked",
        PolicyDecisionReason::ApprovalAlreadyConsumed => "approval_already_consumed",
        PolicyDecisionReason::ApprovalTimestampRegressed => "approval_timestamp_regressed",
    }
}

fn timestamp_to_i64(value: AgentRunTimestamp) -> Result<i64, PolicyRepositoryError> {
    u64_to_i64(value.unix_millis())
}

fn u64_to_i64(value: u64) -> Result<i64, PolicyRepositoryError> {
    i64::try_from(value).map_err(|_| PolicyRepositoryError::ResourceLimit)
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
    AgentRunId,
    ApprovalId,
    ApprovalRequestId,
    PolicyDecisionId,
    a3_domain::RunEventId,
);

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_write(source: libsql::Error) -> PolicyRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        PolicyRepositoryError::InvalidStoredData
    } else {
        PolicyRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum PolicyRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    RunJournal(run_journal_repository::RunJournalRepositoryError),
    InvalidInput,
    InvalidStoredData,
    ResourceLimit,
    NotFound,
    AlreadyExists,
    ApprovalConflict,
}

impl PolicyRepositoryError {
    pub(crate) fn classify(&self) -> PolicyStoreFailure {
        match self {
            Self::InvalidInput | Self::InvalidStoredData | Self::ResourceLimit => {
                PolicyStoreFailure::InvalidStoredData
            }
            Self::NotFound => PolicyStoreFailure::NotFound,
            Self::AlreadyExists => PolicyStoreFailure::AlreadyExists,
            Self::ApprovalConflict => PolicyStoreFailure::ApprovalConflict,
            Self::RunJournal(error) => match error.classify() {
                a3_application::RunJournalStoreFailure::Unavailable => {
                    PolicyStoreFailure::Unavailable
                }
                a3_application::RunJournalStoreFailure::Corrupt => PolicyStoreFailure::Corrupt,
                a3_application::RunJournalStoreFailure::UnsupportedSchema => {
                    PolicyStoreFailure::UnsupportedSchema
                }
                a3_application::RunJournalStoreFailure::InvalidStoredData
                | a3_application::RunJournalStoreFailure::RunAlreadyExists => {
                    PolicyStoreFailure::InvalidStoredData
                }
                a3_application::RunJournalStoreFailure::RunNotFound => PolicyStoreFailure::NotFound,
                a3_application::RunJournalStoreFailure::SequenceConflict => {
                    PolicyStoreFailure::RunSequenceConflict
                }
            },
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    PolicyStoreFailure::Corrupt
                } else {
                    PolicyStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for PolicyRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "policy transaction could not begin",
            Self::Read(_) => "policy data could not be read",
            Self::Write(_) => "policy data could not be written",
            Self::Commit(_) => "policy transaction could not commit",
            Self::Rollback(_) => "policy transaction could not roll back",
            Self::RunJournal(_) => "policy run journal could not be updated",
            Self::InvalidInput => "policy persistence input is invalid",
            Self::InvalidStoredData => "stored policy data is invalid",
            Self::ResourceLimit => "policy value exceeds storage limits",
            Self::NotFound => "policy aggregate was not found",
            Self::AlreadyExists => "policy aggregate already exists",
            Self::ApprovalConflict => "approval lifecycle changed concurrently",
        })
    }
}

impl std::error::Error for PolicyRepositoryError {}
