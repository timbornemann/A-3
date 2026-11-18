//! Atomic failed-attempt diagnostics. No verification artifact is created here.
use super::*;
use crate::{loopback_http_codec::mismatch as codec, policy_repository};
use a3_application::PreparedLoopbackStartupCoverage;
use a3_domain::{
    ApprovalGrantState, LoopbackHttpMismatchDiagnostic, PolicyDecisionOutcome,
    PolicyDecisionReason, RunEventRedactionSource, TaskId,
};

use crate::http_correction_plan_repository::no_change_window_origin::AppliedWindowMismatchPurpose;

const SCHEMA: u32 = 60;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn complete(
    connection: &Connection,
    schema: u32,
    worktree: WorktreeId,
    expected: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
    diagnostic: &LoopbackHttpMismatchDiagnostic,
    timeout: Duration,
    control: &dyn AgentControllerControl,
) -> Result<AgentMutationAttempt, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let c = diagnostic.context();
    if schema < SCHEMA
        || run.state() != a3_domain::AgentControllerState::Execute
        || run
            .budget_exhaustion(event.occurred_at())
            .map_err(|_| E::InvalidInput)?
            .is_some()
        || c.run_id != run.id()
        || c.snapshot_id != run.current_snapshot_id()
        || diagnostic.worktree_id() != worktree
        || event.run_id() != run.id()
        || event.snapshot_id() != c.snapshot_id
        || event.kind() != RunEventKind::ToolAction
        || event.payload().code() != RunEventCode::VerificationFailure
        || event.payload().redaction().is_some()
        || event.turn_charge().is_some()
        || event.subject() != Some(RunEventSubject::Tool(c.tool_run_id))
        || event.payload().outcome() != Some(RunEventOutcome::Failed)
    {
        return Err(E::InvalidInput);
    }
    let guard = verification::OperationGuard::new(timeout, control).map_err(E::Verification)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(E::Begin)?;
    let result=async {
        guard.checkpoint().map_err(E::Verification)?;
        let old=read_mutation_attempt(&tx,worktree,c.tool_run_id,diagnostic.attempt()).await?.ok_or(E::ToolAttemptConflict)?;
        let tool=old.tool_attempt();
        if tool.status()!=AgentToolAttemptStatus::InFlight || tool.run_id()!=run.id() || tool.snapshot_id()!=c.snapshot_id
            || old.disposition()!=AgentMutationDisposition::Unknown(MutationReconciliation::Required) || event.occurred_at()<tool.started_at()
        {return Err(E::ToolAttemptConflict);}
        require_binding(&tx,worktree,diagnostic,event.sequence(),tool.started_at()).await?;
        run_journal_repository::append_in_transaction(&tx,worktree,expected,run,event).await.map_err(E::RunJournal)?;
        let changed=tx.execute("UPDATE tool_run_attempts SET status='failed',updated_at_unix_millis=?1,has_loopback_mismatch=1 WHERE tool_run_id=?2 AND attempt_sequence=?3 AND status='in_flight' AND has_loopback_mismatch=0",params![timestamp_to_i64(event.occurred_at()),id_bytes(c.tool_run_id),i64::from(diagnostic.attempt().get())]).await.map_err(E::Write)?;
        if changed!=1{return Err(E::ToolAttemptConflict);}
        let changed=tx.execute("UPDATE mutation_attempts SET application_state='applied',reconciliation_state='not_required' WHERE tool_run_id=?1 AND attempt_sequence=?2 AND application_state='unknown' AND reconciliation_state='required'",params![id_bytes(c.tool_run_id),i64::from(diagnostic.attempt().get())]).await.map_err(E::Write)?;
        if changed!=1{return Err(E::ToolAttemptConflict);}
        tx.execute("INSERT INTO tool_runs(tool_run_id,run_id,event_sequence,status,result_digest,result_truncated,snapshot_before_id,snapshot_after_id,observed_output_bytes) VALUES(?1,?2,?3,'failed',?4,0,?5,?5,?6)",params![id_bytes(c.tool_run_id),id_bytes(c.run_id),i64::try_from(event.sequence().get()).map_err(|_|E::ResourceLimit)?,diagnostic.digest().as_bytes().to_vec(),id_bytes(c.snapshot_id),i64::from(diagnostic.mismatch().observation().body_bytes())]).await.map_err(E::Write)?;
        tx.execute("INSERT INTO loopback_mismatch_diagnostics(tool_run_id,attempt_sequence,event_id,digest,diagnostic) VALUES(?1,?2,?3,?4,?5)",params![id_bytes(c.tool_run_id),i64::from(diagnostic.attempt().get()),(event.id()).as_bytes().to_vec(),diagnostic.digest().as_bytes().to_vec(),codec::encode(diagnostic)]).await.map_err(E::Write)?;
        guard.checkpoint().map_err(E::Verification)?;
        mutation_attempt(c.tool_run_id,diagnostic.attempt(),c.run_id,c.snapshot_id,AgentToolAttemptStatus::Failed,tool.started_at(),event.occurred_at(),old.fingerprint(),old.kind(),AgentMutationDisposition::Applied)
    }.await;
    close_write_transaction(tx, result).await
}

pub(crate) async fn load(
    connection: &Connection,
    worktree: WorktreeId,
    run: AgentRunId,
    tool: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(E::Begin)?;
    let result = load_from_transaction(&tx, worktree, run, tool, attempt).await;
    close_write_transaction(tx, result).await
}

/// Preserves the original strict active-step path; retired definitions remain ineligible here.
pub(crate) async fn load_from_transaction(
    tx: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
    tool: ToolRunId,
    attempt: AgentToolAttemptNumber,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    load_receipt(tx, worktree, run, tool, attempt, ReceiptBinding::Current).await
}

/// Reuses immutable process/grant receipts only for an independently authenticated immediate material revision.
pub(crate) async fn load_for_applied_http_correction(
    tx: &Transaction,
    worktree: WorktreeId,
    anchor: &a3_application::HttpCorrectionPlanAnchor,
    applied: &RunEvent,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    load_receipt(
        tx,
        worktree,
        anchor.run,
        anchor.tool,
        anchor.tool_attempt,
        ReceiptBinding::AppliedHttpCorrection { anchor, applied },
    )
    .await
}

/// Narrow applied-Context purpose; the token was built only from the exact immutable real read.
pub(crate) async fn load_for_applied_http_context(
    tx: &Transaction,
    worktree: WorktreeId,
    read: &crate::http_context_intent_repository::AuthenticatedHttpContextRead,
    applied: &RunEvent,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    let anchor = &read.anchor().parent;
    load_receipt(
        tx,
        worktree,
        anchor.run,
        anchor.tool,
        anchor.tool_attempt,
        ReceiptBinding::AppliedHttpContext { read, applied },
    )
    .await
}

/// Sole physical-F read purpose for the exact authenticated A-to-B material prefix.
/// The token is privately constructed by that composition, never from model or DTO data.
pub(crate) async fn load_for_applied_http_no_change_window(
    tx: &Transaction,
    worktree: WorktreeId,
    purpose: &AppliedWindowMismatchPurpose<'_>,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    let anchor = purpose.anchor();
    load_receipt(
        tx,
        worktree,
        anchor.run,
        anchor.tool,
        anchor.tool_attempt,
        ReceiptBinding::AppliedHttpNoChangeWindow(purpose),
    )
    .await
}

#[derive(Clone, Copy)]
enum ReceiptBinding<'a> {
    Current,
    AppliedHttpNoChangeWindow(&'a AppliedWindowMismatchPurpose<'a>),
    AppliedHttpCorrection {
        anchor: &'a a3_application::HttpCorrectionPlanAnchor,
        applied: &'a RunEvent,
    },
    AppliedHttpContext {
        read: &'a crate::http_context_intent_repository::AuthenticatedHttpContextRead,
        applied: &'a RunEvent,
    },
}

async fn load_receipt(
    tx: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
    tool: ToolRunId,
    attempt: AgentToolAttemptNumber,
    binding: ReceiptBinding<'_>,
) -> Result<Option<LoopbackHttpMismatchDiagnostic>, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    async {
        let mut rows=tx.query("SELECT a.has_loopback_mismatch,d.event_id,d.digest,CASE WHEN typeof(d.diagnostic)='blob' AND length(d.diagnostic) BETWEEN 400 AND 133000 THEN d.diagnostic WHEN d.diagnostic IS NOT NULL THEN x'' END,a.status,m.application_state,m.reconciliation_state,a.started_at_unix_millis,a.snapshot_id,t.status,t.result_digest,t.result_truncated,t.event_sequence,e.snapshot_id,e.event_kind,e.payload_outcome,e.subject_kind,e.subject_id,t.snapshot_before_id,t.snapshot_after_id,t.observed_output_bytes,e.event_sequence,e.occurred_at_unix_millis,a.updated_at_unix_millis,e.payload_code,e.payload_digest,e.redaction_source,t.run_id FROM tool_run_attempts a JOIN agent_runs r ON r.run_id=a.run_id JOIN tasks k ON k.task_id=r.task_id JOIN mutation_attempts m USING(tool_run_id,attempt_sequence) LEFT JOIN loopback_mismatch_diagnostics d USING(tool_run_id,attempt_sequence) LEFT JOIN tool_runs t ON t.tool_run_id=a.tool_run_id LEFT JOIN run_events e ON e.event_id=d.event_id AND e.run_id=a.run_id WHERE a.tool_run_id=?1 AND a.attempt_sequence=?2 AND a.run_id=?3 AND k.worktree_id=?4",params![id_bytes(tool),i64::from(attempt.get()),id_bytes(run),id_bytes(worktree)]).await.map_err(E::Read)?;
        let Some(row)=rows.next().await.map_err(E::Read)? else{return Ok(None);};
        let marked=read_i64(&row,0)?;
        let blob=match row.get_value(3).map_err(E::Read)? {Value::Null=>None,Value::Blob(b)=>Some(b),_=>return Err(E::InvalidStoredData)};
        match (marked,blob) { (0,None)=>Ok(None),(1,Some(bytes))=> {
            let c=codec::context(&bytes).map_err(|_|E::InvalidStoredData)?;
            if c.run_id!=run || c.tool_run_id!=tool {return Err(E::InvalidStoredData);}
            let task=match binding {
                ReceiptBinding::Current=>task_for_context(tx,worktree,c).await?,
                ReceiptBinding::AppliedHttpNoChangeWindow(purpose)=>
                    task_for_applied_no_change_window(tx,worktree,c,purpose,read_timestamp(&row,7)?).await?,
                ReceiptBinding::AppliedHttpCorrection{anchor,applied}=>
                    task_for_applied_correction(tx,worktree,c,anchor,applied,read_timestamp(&row,7)?).await?,
                ReceiptBinding::AppliedHttpContext{read,applied}=>
                    task_for_applied_context(tx,worktree,c,read,applied,read_timestamp(&row,7)?).await?,
            };
            let recipe=task_ledger_repository::read_bound_loopback_recipe(tx,task,c.step_id,c.spec_id).await.map_err(|_|E::InvalidStoredData)?;
            let d=codec::decode(&bytes,&recipe).map_err(|_|E::InvalidStoredData)?;
            let seq=RunEventSequence::new(read_u64(&row,12)?).map_err(|_|E::InvalidStoredData)?;
            if d.attempt()!=attempt || d.worktree_id()!=worktree || read_id(&row,2)?!=*d.digest().as_bytes()
                || read_text(&row,4)?!="failed" || read_text(&row,5)?!="applied" || read_text(&row,6)?!="not_required"
                || read_id(&row,8)?!=*c.snapshot_id.as_bytes() || read_text(&row,9)?!="failed" || read_id(&row,10)?!=*d.digest().as_bytes() || read_i64(&row,11)?!=0
                || read_id(&row,13)?!=*c.snapshot_id.as_bytes() || read_text(&row,14)?!="tool_action" || read_text(&row,15)?!="failed" || read_text(&row,16)?!="tool" || read_id(&row,17)?!=*tool.as_bytes()
                || read_id(&row,18)?!=*c.snapshot_id.as_bytes() || read_id(&row,19)?!=*c.snapshot_id.as_bytes() || read_u64(&row,20)?!=u64::from(d.mismatch().observation().body_bytes()) || read_u64(&row,21)?!=seq.get() || read_timestamp(&row,22)?!=read_timestamp(&row,23)? || read_timestamp(&row,22)?<read_timestamp(&row,7)? || read_text(&row,24)?!="verification_failure" || read_id(&row,25)? != *a3_domain::RunEventPayload::new(RunEventCode::VerificationFailure,Some(RunEventOutcome::Failed),None).digest().as_bytes() || row.get_value(26).map_err(E::Read)? != Value::Null || read_id(&row,27)? != *run.as_bytes()
            {return Err(E::InvalidStoredData);}
            match binding {
                ReceiptBinding::Current=>require_binding(tx,worktree,&d,seq,read_timestamp(&row,7)?).await?,
                ReceiptBinding::AppliedHttpNoChangeWindow(purpose)=>
                    require_applied_process_receipt(tx,worktree,task,&d,seq,read_timestamp(&row,7)?,purpose.anchor(),read_timestamp(&row,22)?).await?,
                ReceiptBinding::AppliedHttpCorrection{anchor,..}=>
                    require_applied_process_receipt(tx,worktree,task,&d,seq,read_timestamp(&row,7)?,anchor,read_timestamp(&row,22)?).await?,
                ReceiptBinding::AppliedHttpContext{read,..}=>
                    require_applied_process_receipt(tx,worktree,task,&d,seq,read_timestamp(&row,7)?,&read.anchor().parent,read_timestamp(&row,22)?).await?,
            }
            Ok(Some(d))
        },_=>Err(E::InvalidStoredData) }
    }.await
}

#[allow(clippy::too_many_arguments)]
async fn require_applied_process_receipt(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    d: &LoopbackHttpMismatchDiagnostic,
    seq: RunEventSequence,
    started: AgentRunTimestamp,
    anchor: &a3_application::HttpCorrectionPlanAnchor,
    completed: AgentRunTimestamp,
) -> Result<(), AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let failure = run_journal_repository::read_event(tx, anchor.run, anchor.sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    if seq.get().checked_add(1) != Some(anchor.sequence.get()) || failure.occurred_at() != completed
    {
        return Err(E::InvalidStoredData);
    }
    require_process_binding(tx, worktree, task, d, seq, started).await
}

async fn task_for_context(
    tx: &Transaction,
    worktree: WorktreeId,
    c: a3_domain::LoopbackHttpEvidenceContext,
) -> Result<TaskId, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let mut rows=tx.query("SELECT r.task_id FROM agent_runs r JOIN tasks k USING(task_id) JOIN task_ledgers l ON l.task_id=r.task_id AND l.goal_revision=r.goal_revision AND l.plan_revision=r.task_ledger_revision JOIN task_steps s ON s.task_id=r.task_id AND s.step_id=?3 JOIN task_step_attempts a ON a.task_id=s.task_id AND a.step_id=s.step_id WHERE r.run_id=?1 AND k.worktree_id=?2 AND k.current_goal_revision=r.goal_revision AND r.current_snapshot_id=?4 AND s.retired_plan_revision IS NULL AND s.verification_spec_id=?5 AND a.run_id=r.run_id AND a.attempt_number=(SELECT MAX(attempt_number) FROM task_step_attempts WHERE task_id=s.task_id AND step_id=s.step_id) AND a.outcome IN ('active','failed')",params![id_bytes(c.run_id),id_bytes(worktree),(c.step_id).as_bytes().to_vec(),id_bytes(c.snapshot_id),(c.spec_id).as_bytes().to_vec()]).await.map_err(E::Read)?;
    let row = rows
        .next()
        .await
        .map_err(E::Read)?
        .ok_or(E::InvalidStoredData)?;
    Ok(TaskId::from_bytes(read_id(&row, 0)?))
}

// This is not an active-step admission bypass: exactly one already-committed immediate
// LedgerUpdated and its retired failed attempt own this receipt. The origin loader checks
// full Goal/Ledger/Run equality, canonical material definitions and the closed event prefix.
async fn task_for_applied_correction(
    tx: &Transaction,
    worktree: WorktreeId,
    c: a3_domain::LoopbackHttpEvidenceContext,
    anchor: &a3_application::HttpCorrectionPlanAnchor,
    applied: &RunEvent,
    started: AgentRunTimestamp,
) -> Result<TaskId, AgentRecoveryRepositoryError> {
    let expected = anchor
        .sequence
        .get()
        .checked_add(1)
        .ok_or(AgentRecoveryRepositoryError::InvalidStoredData)?;
    task_for_applied_material(tx, worktree, c, anchor, applied, started, expected).await
}

async fn task_for_applied_context(
    tx: &Transaction,
    worktree: WorktreeId,
    c: a3_domain::LoopbackHttpEvidenceContext,
    read: &crate::http_context_intent_repository::AuthenticatedHttpContextRead,
    applied: &RunEvent,
    started: AgentRunTimestamp,
) -> Result<TaskId, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let context = read.anchor();
    let root = &context.parent;
    let actual = read.event();
    if root.sequence.get().checked_add(1) != Some(context.read_tail.get())
        || actual.sequence() != context.read_tail
        || actual.id() != context.read_event
        || actual.run_id() != root.run
        || actual.snapshot_id() != root.snapshot
        || actual.kind() != RunEventKind::ToolAction
        || actual.subject() != Some(RunEventSubject::Tool(context.read_tool))
        || actual.payload().code() != RunEventCode::None
        || actual.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || actual
            .payload()
            .redaction()
            .is_none_or(|v| v.source() != RunEventRedactionSource::ToolOutput)
        || actual.turn_charge().is_some()
        || actual.occurred_at() != context.read_at
        || actual.occurred_at() > applied.occurred_at()
    {
        return Err(E::InvalidStoredData);
    }
    let expected = context
        .read_tail
        .get()
        .checked_add(1)
        .ok_or(E::InvalidStoredData)?;
    task_for_applied_material(tx, worktree, c, root, applied, started, expected).await
}

// Shared immutable material/retired-attempt checks. Only the two explicit purposes above
// choose a sequence, and Context requires its privately constructed authenticated-read token.
#[allow(clippy::too_many_arguments)]
async fn task_for_applied_material(
    tx: &Transaction,
    worktree: WorktreeId,
    c: a3_domain::LoopbackHttpEvidenceContext,
    anchor: &a3_application::HttpCorrectionPlanAnchor,
    applied: &RunEvent,
    started: AgentRunTimestamp,
    expected_applied_sequence: u64,
) -> Result<TaskId, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let next = anchor
        .ledger
        .get()
        .checked_add(1)
        .ok_or(E::InvalidStoredData)?;
    if worktree != anchor.worktree
        || c.run_id != anchor.run
        || c.step_id != anchor.step
        || c.spec_id != anchor.verification
        || c.tool_run_id != anchor.tool
        || c.snapshot_id != anchor.snapshot
        || applied.run_id() != anchor.run
        || applied.snapshot_id() != anchor.snapshot
        || applied.sequence().get() != expected_applied_sequence
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: anchor.ledger,
                to: a3_domain::TaskLedgerRevision::new(next).map_err(|_| E::InvalidStoredData)?,
            })
        || applied.payload()
            != &a3_domain::RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            )
        || applied.subject().is_some()
        || applied.turn_charge().is_some()
    {
        return Err(E::InvalidStoredData);
    }
    let failure = run_journal_repository::read_event(tx, anchor.run, anchor.sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    if failure.kind()
        != (RunEventKind::StateTransition {
            from: a3_domain::AgentControllerState::Execute,
            to: a3_domain::AgentControllerState::Replan,
        })
        || failure.run_id() != anchor.run
        || failure.snapshot_id() != anchor.snapshot
        || failure.payload()
            != &a3_domain::RunEventPayload::new(
                RunEventCode::VerificationFailure,
                Some(RunEventOutcome::Failed),
                None,
            )
        || failure.subject().is_some()
        || failure.turn_charge().is_some()
        || failure.occurred_at() > applied.occurred_at()
    {
        return Err(E::InvalidStoredData);
    }
    let mut rows=tx.query("SELECT r.task_id FROM agent_runs r JOIN tasks k USING(task_id)
        JOIN task_ledgers l ON l.task_id=r.task_id AND l.goal_revision=r.goal_revision AND l.plan_revision=r.task_ledger_revision
        JOIN task_steps s ON s.task_id=r.task_id AND s.step_id=?3
        JOIN task_step_attempts a ON a.task_id=s.task_id AND a.step_id=s.step_id
        WHERE r.run_id=?1 AND k.worktree_id=?2 AND r.task_id=?4 AND r.goal_revision=?5
        AND k.current_goal_revision=r.goal_revision AND r.current_snapshot_id=?6 AND r.task_ledger_revision=?7
        AND s.retired_plan_revision=?7 AND s.verification_spec_id=?8 AND s.status='failed'
        AND a.run_id=r.run_id AND a.attempt_number=?9 AND a.outcome='failed'
        AND a.attempt_number=(SELECT MAX(attempt_number) FROM task_step_attempts WHERE task_id=s.task_id AND step_id=s.step_id)
        AND a.started_at_unix_millis<=?10 AND a.finished_at_unix_millis=?11",
        params![id_bytes(anchor.run),id_bytes(worktree),anchor.step.as_bytes().to_vec(),anchor.goal.task_id().as_bytes().to_vec(),i64::from(anchor.goal.revision().get()),id_bytes(anchor.snapshot),i64::from(next),anchor.verification.as_bytes().to_vec(),i64::from(anchor.failed_attempt.get()),i64::try_from(started.unix_millis()).map_err(|_|E::InvalidStoredData)?,i64::try_from(failure.occurred_at().unix_millis()).map_err(|_|E::InvalidStoredData)?]).await.map_err(E::Read)?;
    let row = rows
        .next()
        .await
        .map_err(E::Read)?
        .ok_or(E::InvalidStoredData)?;
    let task = TaskId::from_bytes(read_id(&row, 0)?);
    if rows.next().await.map_err(E::Read)?.is_some() {
        return Err(E::InvalidStoredData);
    }
    Ok(task)
}

// Exact second material purpose. The current Ledger is A+1 while the physical
// original F step remains retired at its real first material revision. No active
// context admission and no original receipt validation is weakened to read it.
async fn task_for_applied_no_change_window(
    tx: &Transaction,
    worktree: WorktreeId,
    c: a3_domain::LoopbackHttpEvidenceContext,
    purpose: &AppliedWindowMismatchPurpose<'_>,
    started: AgentRunTimestamp,
) -> Result<TaskId, AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let anchor = purpose.anchor();
    let fact = purpose.fact();
    let owner = fact.owner();
    let first_revision = anchor
        .ledger
        .get()
        .checked_add(1)
        .ok_or(E::InvalidStoredData)?;
    let second_revision = first_revision.checked_add(1).ok_or(E::InvalidStoredData)?;
    let first = purpose.first_applied();
    let applied = purpose.applied();
    let closure = purpose.closure();
    let settlement = purpose.settlement();
    let first_expected = owner
        .sequence
        .get()
        .checked_sub(4)
        .ok_or(E::InvalidStoredData)?;
    if worktree != anchor.worktree
        || worktree != owner.worktree
        || c.run_id != anchor.run
        || c.step_id != anchor.step
        || c.spec_id != anchor.verification
        || c.tool_run_id != anchor.tool
        || c.snapshot_id != anchor.snapshot
        || owner.run != anchor.run
        || owner.goal != anchor.goal
        || owner.original_goal != anchor.original
        || owner.index != anchor.index
        || owner.snapshot != anchor.snapshot
        || owner.profile != anchor.profile
        || owner.ledger.get() != first_revision
        || purpose.revision().get() != second_revision
        || closure.id() != fact.closure_event_id()
        || closure.run_id() != anchor.run
        || closure.snapshot_id() != anchor.snapshot
        || closure.sequence().get()
            != owner
                .sequence
                .get()
                .checked_add(1)
                .ok_or(E::InvalidStoredData)?
        || first.run_id() != anchor.run
        || first.snapshot_id() != anchor.snapshot
        || first.sequence().get() != first_expected
        || first.kind()
            != (RunEventKind::LedgerUpdated {
                from: anchor.ledger,
                to: owner.ledger,
            })
        || first.payload()
            != &a3_domain::RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            )
        || first.subject().is_some()
        || first.turn_charge().is_some()
        || applied.run_id() != anchor.run
        || applied.snapshot_id() != anchor.snapshot
        || applied.sequence().get()
            != closure
                .sequence()
                .get()
                .checked_add(1)
                .ok_or(E::InvalidStoredData)?
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: owner.ledger,
                to: purpose.revision(),
            })
        || applied.payload()
            != &a3_domain::RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            )
        || applied.subject().is_some()
        || applied.turn_charge().is_some()
        || settlement.reserved_at() < closure.occurred_at()
        || settlement.observation().finished_at() > applied.occurred_at()
        || applied.occurred_at() >= settlement.deadline()
        || !matches!(
            settlement.decision(),
            Some(a3_application::HttpNoChangeReplanDecision::Window { .. })
        )
        || purpose.retired_steps()[0] != owner.step
    {
        return Err(E::InvalidStoredData);
    }
    let failure = run_journal_repository::read_event(tx, anchor.run, anchor.sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    if failure.kind()
        != (RunEventKind::StateTransition {
            from: a3_domain::AgentControllerState::Execute,
            to: a3_domain::AgentControllerState::Replan,
        })
        || failure.payload()
            != &a3_domain::RunEventPayload::new(
                RunEventCode::VerificationFailure,
                Some(RunEventOutcome::Failed),
                None,
            )
        || failure.run_id() != anchor.run
        || failure.snapshot_id() != anchor.snapshot
        || failure.subject().is_some()
        || failure.turn_charge().is_some()
        || failure.occurred_at() > first.occurred_at()
    {
        return Err(E::InvalidStoredData);
    }
    let mut rows=tx.query("SELECT r.task_id FROM agent_runs r JOIN tasks k USING(task_id)
        JOIN task_ledgers l ON l.task_id=r.task_id AND l.goal_revision=r.goal_revision AND l.plan_revision=r.task_ledger_revision
        JOIN task_steps s ON s.task_id=r.task_id AND s.step_id=?3
        JOIN task_step_attempts a ON a.task_id=s.task_id AND a.step_id=s.step_id
        JOIN task_steps c ON c.task_id=r.task_id AND c.step_id=?12
        JOIN task_steps h ON h.task_id=r.task_id AND h.step_id=?13
        WHERE r.run_id=?1 AND k.worktree_id=?2 AND r.task_id=?4 AND r.goal_revision=?5
        AND k.current_goal_revision=r.goal_revision AND r.current_snapshot_id=?6 AND r.task_ledger_revision=?7
        AND l.store_version=?14 AND r.last_event_sequence=?15
        AND s.retired_plan_revision=?8 AND s.verification_spec_id=?9 AND s.status='failed'
        AND a.run_id=r.run_id AND a.attempt_number=?10 AND a.outcome='failed'
        AND a.attempt_number=(SELECT MAX(attempt_number) FROM task_step_attempts WHERE task_id=s.task_id AND step_id=s.step_id)
        AND a.started_at_unix_millis<=?11 AND a.finished_at_unix_millis=?16
        AND c.introduced_plan_revision=?8 AND c.retired_plan_revision=?7 AND c.status='failed'
        AND h.introduced_plan_revision=?8 AND h.retired_plan_revision=?7 AND h.status='pending'
        AND NOT EXISTS(SELECT 1 FROM task_step_attempts x WHERE x.task_id=h.task_id AND x.step_id=h.step_id)",
        params![id_bytes(anchor.run),id_bytes(worktree),anchor.step.as_bytes().to_vec(),anchor.goal.task_id().as_bytes().to_vec(),
            i64::from(anchor.goal.revision().get()),id_bytes(anchor.snapshot),i64::from(second_revision),i64::from(first_revision),
            anchor.verification.as_bytes().to_vec(),i64::from(anchor.failed_attempt.get()),
            i64::try_from(started.unix_millis()).map_err(|_|E::InvalidStoredData)?,purpose.retired_steps()[0].as_bytes().to_vec(),
            purpose.retired_steps()[1].as_bytes().to_vec(),i64::try_from(purpose.version().get()).map_err(|_|E::InvalidStoredData)?,
            i64::try_from(purpose.tail().get()).map_err(|_|E::InvalidStoredData)?,
            i64::try_from(failure.occurred_at().unix_millis()).map_err(|_|E::InvalidStoredData)?]).await.map_err(E::Read)?;
    let row = rows
        .next()
        .await
        .map_err(E::Read)?
        .ok_or(E::InvalidStoredData)?;
    let task = TaskId::from_bytes(read_id(&row, 0)?);
    if rows.next().await.map_err(E::Read)?.is_some() {
        return Err(E::InvalidStoredData);
    }
    Ok(task)
}

async fn require_binding(
    tx: &Transaction,
    worktree: WorktreeId,
    d: &LoopbackHttpMismatchDiagnostic,
    completion: RunEventSequence,
    started: AgentRunTimestamp,
) -> Result<(), AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let c = d.context();
    let task = task_for_context(tx, worktree, c).await?;
    let mut active = tx.query(
        "SELECT a.started_at_unix_millis FROM task_step_attempts a JOIN task_steps s USING(task_id,step_id)
         WHERE a.task_id=?1 AND a.step_id=?2 AND a.run_id=?3
         AND a.attempt_number=(SELECT MAX(attempt_number) FROM task_step_attempts WHERE task_id=a.task_id AND step_id=a.step_id)
         AND ((s.status='in_progress' AND a.outcome='active') OR (s.status='failed' AND a.outcome='failed'))",
        params![task.as_bytes().to_vec(), c.step_id.as_bytes().to_vec(), id_bytes(c.run_id)]).await.map_err(E::Read)?;
    if read_timestamp(
        &active
            .next()
            .await
            .map_err(E::Read)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )? > started
    {
        return Err(E::InvalidStoredData);
    }
    require_process_binding(tx, worktree, task, d, completion, started).await
}

async fn require_process_binding(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    d: &LoopbackHttpMismatchDiagnostic,
    completion: RunEventSequence,
    started: AgentRunTimestamp,
) -> Result<(), AgentRecoveryRepositoryError> {
    use AgentRecoveryRepositoryError as E;
    let c = d.context();
    let recipe = task_ledger_repository::read_bound_loopback_recipe(tx, task, c.step_id, c.spec_id)
        .await
        .map_err(|_| E::InvalidStoredData)?;
    let rebuilt = LoopbackHttpMismatchDiagnostic::new(
        c,
        d.attempt(),
        worktree,
        &recipe,
        d.script().clone(),
        d.process(),
        d.mismatch(),
        d.decisions().to_vec(),
    )
    .map_err(|_| E::InvalidStoredData)?;
    if &rebuilt != d {
        return Err(E::InvalidStoredData);
    }
    let prepared = PreparedLoopbackStartupCoverage::new(
        c.run_id,
        worktree,
        c.step_id,
        c.snapshot_id,
        d.script().clone(),
        recipe,
        d.process().settings(),
    )
    .map_err(|_| E::InvalidStoredData)?;
    let scope = load_machine_scope(tx, worktree, c.tool_run_id, d.attempt())
        .await?
        .ok_or(E::InvalidStoredData)?;
    let old = read_mutation_attempt(tx, worktree, c.tool_run_id, d.attempt())
        .await?
        .ok_or(E::InvalidStoredData)?;
    if old.kind() != AgentMutationKind::MachineProcess
        || old.fingerprint() != prepared.effect_fingerprint()
        || scope.kind() != AgentMutationKind::MachineProcess
        || scope.step() != c.step_id
        || scope.resource() != prepared.effect_resource()
        || scope.target() != "python"
        || scope.expected() != Some(d.script().content_hash())
        || scope.proposed().is_some()
    {
        return Err(E::InvalidStoredData);
    }
    let mut source=tx.query("WITH latest AS(SELECT index_run_id,snapshot_id FROM index_runs WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1) SELECT 1 FROM latest JOIN file_revisions f USING(index_run_id) WHERE latest.snapshot_id=?2 AND f.repository_path=?3 AND f.content_hash=?4",params![id_bytes(worktree),id_bytes(c.snapshot_id),d.script().path().as_bytes().to_vec(),d.script().content_hash().as_bytes().to_vec()]).await.map_err(E::Read)?;
    if source.next().await.map_err(E::Read)?.is_none() {
        return Err(E::PublishedSnapshotConflict);
    }
    let mut latest = tx
        .query(
            "SELECT MAX(attempt_sequence) FROM tool_run_attempts WHERE tool_run_id=?1",
            params![id_bytes(c.tool_run_id)],
        )
        .await
        .map_err(E::Read)?;
    if read_u32(
        &latest
            .next()
            .await
            .map_err(E::Read)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )? != d.attempt().get()
    {
        return Err(E::InvalidStoredData);
    }
    for (id, action) in d.decisions().iter().zip(prepared.policy_actions()) {
        let decision = policy_repository::load_decision(tx, *id)
            .await
            .map_err(|_| E::InvalidStoredData)?
            .ok_or(E::InvalidStoredData)?;
        if decision.run_id() != c.run_id
            || decision.outcome() != PolicyDecisionOutcome::Allowed
            || decision.reason() != PolicyDecisionReason::ApprovalGranted
            || decision.action_fingerprint() != action.fingerprint()
            || decision.scope_digest() != action.scope_digest()
            || decision.action_class() != action.class()
            || decision.risk_level() != action.risk()
            || decision.timing().decided_at() > started
        {
            return Err(E::InvalidStoredData);
        }
        let grant = policy_repository::load_approval(
            tx,
            decision.approval_id().ok_or(E::InvalidStoredData)?,
        )
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
        if decision
            .approval_request_id()
            .is_some_and(|id| id != grant.request_id())
            || grant.run_id() != c.run_id
            || grant.action_fingerprint() != action.fingerprint()
            || grant.scope_digest() != action.scope_digest()
            || grant.action_class() != action.class()
            || grant.risk_level() != action.risk()
            || !matches!(grant.state(),ApprovalGrantState::Consumed{decision_id,consumed_at} if decision_id==*id && consumed_at==decision.timing().decided_at())
        {
            return Err(E::InvalidStoredData);
        }
        let mut events=tx.query("SELECT 1 FROM policy_decisions p JOIN run_events e ON e.event_id=p.event_id AND e.run_id=p.run_id WHERE p.policy_decision_id=?1 AND p.run_id=?2 AND e.snapshot_id=?3 AND e.event_sequence<?4 AND e.event_kind='approval_recorded' AND e.payload_code='policy_decision' AND e.payload_outcome='succeeded' AND e.occurred_at_unix_millis=p.decided_at_unix_millis AND e.subject_kind IS NULL AND e.redaction_source IS NULL",params![(*id).as_bytes().to_vec(),id_bytes(c.run_id),id_bytes(c.snapshot_id),i64::try_from(completion.get()).map_err(|_|E::ResourceLimit)?]).await.map_err(E::Read)?;
        if events.next().await.map_err(E::Read)?.is_none() {
            return Err(E::InvalidStoredData);
        }
    }
    Ok(())
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, AgentRecoveryRepositoryError> {
    row.get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}
fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, AgentRecoveryRepositoryError> {
    u64::try_from(read_i64(row, index)?)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}
fn read_text(row: &libsql::Row, index: i32) -> Result<String, AgentRecoveryRepositoryError> {
    row.get(index)
        .map_err(|_| AgentRecoveryRepositoryError::InvalidStoredData)
}
