//! Intrinsic Ledger checkpoint projection. Run/event admission belongs after the paired write.
use super::{
    TaskLedgerRepositoryError as Error, classify_unexpected_constraint, id_bytes, read_i64,
    read_id, read_ledger_revision, read_text, sequence_to_i64,
};
use a3_domain::{
    AgentRunId, GoalContractReference, PendingPlanning, PendingPlanningStage, RunEventId,
    RunEventSequence, SnapshotId, TaskId, TaskLedger, TaskLedgerReplanCause,
};
use libsql::{Transaction, params};

pub(super) async fn supported(tx: &Transaction) -> Result<bool, Error> {
    crate::migration::read_user_version(tx)
        .await
        .map(|version| version >= 47)
        .map_err(Error::Read)
}

pub(super) async fn require_schema(tx: &Transaction, ledger: &TaskLedger) -> Result<(), Error> {
    if (ledger.pending_planning().is_some()
        || ledger
            .replans()
            .iter()
            .any(|r| r.cause() == TaskLedgerReplanCause::HorizonExtension))
        && !supported(tx).await?
    {
        return Err(Error::PlanningSchemaUnsupported);
    }
    Ok(())
}

pub(super) fn validate_create(ledger: &TaskLedger) -> Result<(), Error> {
    if ledger.pending_planning().is_some()
        || ledger
            .replans()
            .iter()
            .any(|r| r.cause() == TaskLedgerReplanCause::HorizonExtension)
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_standalone(
    existing: &TaskLedger,
    candidate: &TaskLedger,
) -> Result<(), Error> {
    if existing.pending_planning() != candidate.pending_planning()
        || candidate
            .replans()
            .iter()
            .skip(existing.replans().len())
            .any(|r| r.cause() == TaskLedgerReplanCause::HorizonExtension)
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub(super) async fn clear(tx: &Transaction, task: TaskId) -> Result<(), Error> {
    if supported(tx).await? {
        tx.execute(
            "DELETE FROM task_ledger_pending_planning WHERE task_id=?1",
            params![id_bytes(task)],
        )
        .await
        .map_err(Error::Write)?;
    }
    Ok(())
}

pub(super) async fn write(tx: &Transaction, ledger: &TaskLedger) -> Result<(), Error> {
    let Some(pending) = ledger.pending_planning() else {
        return Ok(());
    };
    let (stage, revision, ids) = match pending.stage() {
        PendingPlanningStage::NeedsPlan => ("needs_plan", None, &[][..]),
        PendingPlanningStage::PlanAppended {
            revision,
            added_step_ids,
        } => (
            "plan_appended",
            Some(i64::from(revision.get())),
            added_step_ids.as_slice(),
        ),
    };
    tx.execute(
        "INSERT INTO task_ledger_pending_planning
        (task_id,goal_revision,run_id,base_ledger_revision,snapshot_id,requested_event_id,
         requested_sequence,stage,appended_revision,added_step_count)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            id_bytes(ledger.task_id()),
            i64::from(pending.goal_ref().revision().get()),
            pending.run_id().as_bytes().to_vec(),
            i64::from(pending.base_revision().get()),
            pending.snapshot_id().as_bytes().to_vec(),
            pending.requested_event_id().as_bytes().to_vec(),
            i64::try_from(pending.requested_sequence().get()).map_err(|_| Error::ResourceLimit)?,
            stage,
            revision,
            i64::try_from(ids.len()).map_err(|_| Error::ResourceLimit)?
        ],
    )
    .await
    .map_err(classify_unexpected_constraint)?;
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO task_ledger_pending_planning_steps(task_id,step_sequence,step_id)
            VALUES (?1,?2,?3)",
            params![
                id_bytes(ledger.task_id()),
                sequence_to_i64(index)?,
                id_bytes(*id)
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}

pub(super) async fn load(
    tx: &Transaction,
    goal: GoalContractReference,
) -> Result<Option<PendingPlanning>, Error> {
    if !supported(tx).await? {
        return Ok(None);
    }
    let mut rows = tx
        .query(
            "SELECT goal_revision,
        CASE WHEN length(run_id)=32 THEN run_id END,base_ledger_revision,
        CASE WHEN length(snapshot_id)=32 THEN snapshot_id END,
        CASE WHEN length(requested_event_id)=32 THEN requested_event_id END,
        requested_sequence,CASE WHEN length(CAST(stage AS BLOB))<=16 THEN stage END,
        appended_revision,added_step_count,length(run_id),length(snapshot_id),
        length(requested_event_id),length(CAST(stage AS BLOB))
        FROM task_ledger_pending_planning WHERE task_id=?1",
            params![id_bytes(goal.task_id())],
        )
        .await
        .map_err(Error::Read)?;
    let Some(row) = rows.next().await.map_err(Error::Read)? else {
        let mut children = tx
            .query(
                "SELECT EXISTS(SELECT 1 FROM task_ledger_pending_planning_steps WHERE task_id=?1)",
                params![id_bytes(goal.task_id())],
            )
            .await
            .map_err(Error::Read)?;
        let orphan = children
            .next()
            .await
            .map_err(Error::Read)?
            .ok_or(Error::InvalidStoredData)?;
        if read_i64(&orphan, 0)? != 0 {
            return Err(Error::InvalidStoredData);
        }
        return Ok(None);
    };
    if read_i64(&row, 0)? != i64::from(goal.revision().get())
        || read_i64(&row, 9)? != 32
        || read_i64(&row, 10)? != 32
        || read_i64(&row, 11)? != 32
        || read_i64(&row, 12)? > 16
    {
        return Err(Error::InvalidStoredData);
    }
    let run = AgentRunId::from_bytes(read_id(&row, 1)?);
    let base = read_ledger_revision(&row, 2)?;
    let snapshot = SnapshotId::from_bytes(read_id(&row, 3)?);
    let event = RunEventId::from_bytes(read_id(&row, 4)?);
    let sequence = RunEventSequence::new(
        u64::try_from(read_i64(&row, 5)?).map_err(|_| Error::InvalidStoredData)?,
    )
    .map_err(|_| Error::InvalidStoredData)?;
    let count = usize::try_from(read_i64(&row, 8)?).map_err(|_| Error::InvalidStoredData)?;
    if count > 3 {
        return Err(Error::ResourceLimit);
    }
    let ids = load_added_steps(tx, goal.task_id(), count).await?;
    let revision: Option<i64> = row.get(7).map_err(Error::Read)?;
    let stage = match (read_text(&row, 6)?.as_str(), revision) {
        ("needs_plan", None) if count == 0 => PendingPlanningStage::NeedsPlan,
        ("plan_appended", Some(_)) if count > 0 => PendingPlanningStage::PlanAppended {
            revision: read_ledger_revision(&row, 7)?,
            added_step_ids: ids,
        },
        _ => return Err(Error::InvalidStoredData),
    };
    if rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    PendingPlanning::reconstruct(run, goal, base, snapshot, event, sequence, stage)
        .map(Some)
        .map_err(|_| Error::InvalidStoredData)
}

async fn load_added_steps(
    tx: &Transaction,
    task: TaskId,
    count: usize,
) -> Result<Vec<a3_domain::TaskStepId>, Error> {
    let mut rows=tx.query("SELECT step_sequence,CASE WHEN length(step_id)=32 THEN step_id END,
        length(step_id) FROM task_ledger_pending_planning_steps WHERE task_id=?1 ORDER BY step_sequence",
        params![id_bytes(task)]).await.map_err(Error::Read)?;
    let mut ids = Vec::with_capacity(count);
    while let Some(row) = rows.next().await.map_err(Error::Read)? {
        if ids.len() >= count
            || read_i64(&row, 0)? != sequence_to_i64(ids.len())?
            || read_i64(&row, 2)? != 32
        {
            return Err(Error::InvalidStoredData);
        }
        ids.push(a3_domain::TaskStepId::from_bytes(read_id(&row, 1)?));
    }
    if ids.len() != count {
        return Err(Error::InvalidStoredData);
    }
    Ok(ids)
}

pub(super) async fn action_changes_planning(
    tx: &Transaction,
    ledger: &TaskLedger,
) -> Result<bool, Error> {
    if !supported(tx).await? {
        return Ok(false);
    }
    if ledger.pending_planning().is_some() {
        return Ok(true);
    }
    let mut rows = tx
        .query(
            "SELECT l.plan_revision,EXISTS(SELECT 1 FROM task_ledger_pending_planning p
        WHERE p.task_id=l.task_id) FROM task_ledgers l WHERE l.task_id=?1",
            params![id_bytes(ledger.task_id())],
        )
        .await
        .map_err(Error::Read)?;
    let Some(row) = rows.next().await.map_err(Error::Read)? else {
        return Ok(false);
    };
    let current = read_ledger_revision(&row, 0)?;
    let pending = read_i64(&row, 1)? == 1;
    if rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    Ok(pending
        || ledger.replans().iter().any(|r| {
            r.revision() > current && r.cause() == TaskLedgerReplanCause::HorizonExtension
        }))
}
