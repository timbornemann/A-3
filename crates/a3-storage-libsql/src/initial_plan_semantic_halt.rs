//! Bounded semantic halt storage in the same transaction as the observed Plan call.

use super::*;

pub(super) const SCHEMA_VERSION: u32 = 59;

pub(super) async fn write(
    tx: &Transaction,
    task: TaskId,
    kind: EditPhaseCallKind,
    reason: PlanPhaseHalt,
) -> Result<(), Failure> {
    let reason = match reason {
        PlanPhaseHalt::Refuse => "refuse",
        PlanPhaseHalt::Clarify => "clarify",
    };
    let changed = tx
        .execute(
            "INSERT INTO initial_plan_phase_halts(task_id,call_kind,reason) VALUES (?1,?2,?3)",
            params![task.as_bytes().to_vec(), kind_text(kind), reason],
        )
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(AccountingError::InvalidStoredData.into());
    }
    Ok(())
}

pub(super) async fn load(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<PlanPhaseHalt>, Failure> {
    if read_user_version(tx).await.map_err(unavailable)? < SCHEMA_VERSION {
        return Err(AccountingError::UnsupportedSchema.into());
    }
    let scope = scope(tx, worktree, task).await?;
    // Inspect the latest call, including an unobserved or non-halt repair. Filtering
    // for halt rows here could resurrect an older decision after another result.
    let mut rows = tx
        .query(
            "SELECT CASE WHEN call_kind IN ('initial','structural_repair') THEN call_kind ELSE '' END,
                    goal_revision,has_semantic_halt,
                    CASE WHEN outcome IS NULL OR outcome IN (
                        'accepted','need_evidence','too_large','rejected_reply','invalid_structure',
                        'provider_failure','cancelled','unknown','stale','budget_overrun')
                    THEN outcome ELSE '' END,
                    reported_prompt,reported_output,accepted_ledger_version,
                    EXISTS(SELECT 1 FROM task_ledgers WHERE task_id=?1),
                    prompt_reserve,output_reserve
             FROM initial_plan_phase_reservations WHERE task_id=?1
             ORDER BY reserved_at DESC,CASE call_kind WHEN 'structural_repair' THEN 1 ELSE 0 END DESC
             LIMIT 1",
            [task.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    let kind: String = row.get(0).map_err(|_| AccountingError::InvalidStoredData)?;
    if !matches!(kind.as_str(), "initial" | "structural_repair") {
        return Err(AccountingError::InvalidStoredData.into());
    }
    let revision: i64 = row.get(1).map_err(|_| AccountingError::InvalidStoredData)?;
    if revision != i64::from(GoalContractRevision::INITIAL.get()) {
        return Err(AccountingError::InvalidStoredData.into());
    }
    let present: i64 = row.get(2).map_err(|_| AccountingError::InvalidStoredData)?;
    if !matches!(present, 0 | 1) {
        return Err(AccountingError::InvalidStoredData.into());
    }
    let reason = read_reason(tx, task, &kind, present == 1).await?;
    let Some(reason) = reason else {
        return Ok(None);
    };
    let outcome: Option<String> = row.get(3).map_err(|_| AccountingError::InvalidStoredData)?;
    let prompt: Option<i64> = row.get(4).map_err(|_| AccountingError::InvalidStoredData)?;
    let output: Option<i64> = row.get(5).map_err(|_| AccountingError::InvalidStoredData)?;
    let accepted: Option<i64> = row.get(6).map_err(|_| AccountingError::InvalidStoredData)?;
    let has_ledger: i64 = row.get(7).map_err(|_| AccountingError::InvalidStoredData)?;
    let prompt_reserve: i64 = row.get(8).map_err(|_| AccountingError::InvalidStoredData)?;
    let output_reserve: i64 = row.get(9).map_err(|_| AccountingError::InvalidStoredData)?;
    // An invalid/stale/over-budget observation retains its reason for history,
    // but cannot be presented as the current successfully settled model halt.
    match outcome.as_deref() {
        Some("rejected_reply") => {}
        Some("unknown" | "stale" | "budget_overrun") => return Ok(None),
        _ => return Err(AccountingError::InvalidStoredData.into()),
    }
    if !prompt.is_some_and(|n| n > 0 && n <= prompt_reserve)
        || !output.is_some_and(|n| n > 0 && n <= output_reserve)
        || accepted.is_some()
        || has_ledger != 0
    {
        return Err(AccountingError::InvalidStoredData.into());
    }
    if scope.accounting.linked_run().is_some()
        || goal_contract_repository::task_current_revision(tx, worktree, task)
            .await
            .map_err(|_| AccountingError::Unavailable)?
            != Some(GoalContractRevision::INITIAL)
    {
        return Ok(None);
    }
    Ok(Some(reason))
}

async fn read_reason(
    tx: &Transaction,
    task: TaskId,
    kind: &str,
    present: bool,
) -> Result<Option<PlanPhaseHalt>, Failure> {
    let mut rows = tx
        .query(
            "SELECT CASE WHEN typeof(reason)='text' AND length(reason) BETWEEN 6 AND 7
                         THEN reason ELSE '' END
             FROM initial_plan_phase_halts WHERE task_id=?1 AND call_kind=?2 LIMIT 2",
            params![task.as_bytes().to_vec(), kind],
        )
        .await
        .map_err(unavailable)?;
    let row = rows.next().await.map_err(unavailable)?;
    match (present, row) {
        (false, None) => Ok(None),
        (true, Some(row)) => {
            let reason: String = row.get(0).map_err(|_| AccountingError::InvalidStoredData)?;
            let reason = match reason.as_str() {
                "refuse" => PlanPhaseHalt::Refuse,
                "clarify" => PlanPhaseHalt::Clarify,
                _ => return Err(AccountingError::InvalidStoredData.into()),
            };
            if rows.next().await.map_err(unavailable)?.is_some() {
                return Err(AccountingError::InvalidStoredData.into());
            }
            Ok(Some(reason))
        }
        _ => Err(AccountingError::InvalidStoredData.into()),
    }
}
