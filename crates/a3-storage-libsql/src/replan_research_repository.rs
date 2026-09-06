//! Adapter-local shared checkpoint encoding and atomic run-event ownership.
use super::*;
use crate::agent_ask_research_repository::work_state;
use a3_application::ReplanResearchCheckpoint;
use a3_domain::TaskStepId;

pub(crate) async fn originals(
    connection: &Connection,
    worktree: WorktreeId,
    run: AgentRunId,
    step: TaskStepId,
    snapshot: SnapshotId,
) -> Result<Vec<a3_domain::AgentToolEvidence>, RunJournalRepositoryError> {
    if load_run(connection, worktree, run).await?.is_none() {
        return Err(RunJournalRepositoryError::RunNotFound);
    }
    let mut rows = connection.query("SELECT e.evidence_id,e.location_kind,e.repository_path,e.content_hash,e.start_byte,e.end_byte,e.start_row,e.start_column,e.end_row,e.end_column
        FROM agent_replan_originals o JOIN agent_replan_research_checkpoints c USING(run_id,event_sequence)
        JOIN tool_runs t ON t.run_id=o.run_id AND t.event_sequence=o.event_sequence
        JOIN tool_evidence e ON e.tool_run_id=t.tool_run_id AND e.evidence_id=o.evidence_id
        WHERE c.run_id=?1 AND c.step_id=?2 AND c.snapshot_id=?3 AND t.status='succeeded' AND e.location_kind='span'
        ORDER BY o.event_sequence ASC LIMIT 9",
        params![id_bytes(run), step.as_bytes().to_vec(), id_bytes(snapshot)])
        .await.map_err(RunJournalRepositoryError::Read)?;
    let mut evidence = Vec::new();
    while let Some(row) = rows.next().await.map_err(RunJournalRepositoryError::Read)? {
        evidence.push(
            crate::agent_recovery_repository::read_evidence(&row)
                .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?,
        );
    }
    if evidence.len() > 8 {
        return Err(RunJournalRepositoryError::InvalidStoredData);
    }
    Ok(evidence)
}

pub(crate) async fn append(
    connection: &Connection,
    worktree: WorktreeId,
    expected: RunEventSequence,
    run: &AgentRun,
    event: &RunEvent,
    checkpoint: &ReplanResearchCheckpoint,
) -> Result<(), RunJournalRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(RunJournalRepositoryError::Begin)?;
    let result = async {
        append_in_transaction(&transaction, worktree, expected, run, event).await?;
        insert(&transaction, worktree, run, event, checkpoint).await
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn load(
    connection: &Connection,
    worktree: WorktreeId,
    run_id: AgentRunId,
    step: TaskStepId,
) -> Result<Option<ReplanResearchCheckpoint>, RunJournalRepositoryError> {
    if load_run(connection, worktree, run_id).await?.is_none() {
        return Err(RunJournalRepositoryError::RunNotFound);
    }
    load_owned(connection, run_id, step).await
}

async fn load_owned(
    connection: &Connection,
    run_id: AgentRunId,
    step: TaskStepId,
) -> Result<Option<ReplanResearchCheckpoint>, RunJournalRepositoryError> {
    let mut rows = connection.query("SELECT snapshot_id, payload FROM agent_replan_research_checkpoints WHERE run_id=?1 AND step_id=?2 ORDER BY event_sequence DESC LIMIT 1",
        params![id_bytes(run_id), step.as_bytes().to_vec()]).await.map_err(RunJournalRepositoryError::Read)?;
    rows.next()
        .await
        .map_err(RunJournalRepositoryError::Read)?
        .map(|row| {
            Ok(ReplanResearchCheckpoint {
                step_id: step,
                snapshot_id: SnapshotId::from_bytes(read_id(&row, 0)?),
                work: work_state::decode(&read_text(&row, 1)?)
                    .map_err(|_| RunJournalRepositoryError::InvalidStoredData)?,
            })
        })
        .transpose()
}

pub(super) async fn insert(
    transaction: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
    event: &RunEvent,
    checkpoint: &ReplanResearchCheckpoint,
) -> Result<(), RunJournalRepositoryError> {
    if checkpoint.snapshot_id != run.current_snapshot_id()
        || event.snapshot_id() != checkpoint.snapshot_id
        || checkpoint.reads() > 4
    {
        return Err(RunJournalRepositoryError::InvalidInput);
    }
    let stored = task_ledger_repository::load_from_transaction(
        transaction,
        worktree,
        run.goal_contract().task_id(),
    )
    .await
    .map_err(RunJournalRepositoryError::TaskLedger)?
    .ok_or(RunJournalRepositoryError::RunNotFound)?;
    if stored
        .ledger()
        .step(checkpoint.step_id)
        .is_none_or(|s| !s.is_active_plan_step())
    {
        return Err(RunJournalRepositoryError::InvalidInput);
    }
    if let Some(prior) = load_owned(transaction, run.id(), checkpoint.step_id).await?
        && prior.snapshot_id == checkpoint.snapshot_id
        && (prior.work.objective() != checkpoint.work.objective()
            || !prior
                .work
                .questions()
                .iter()
                .map(|q| q.definition())
                .eq(checkpoint.work.questions().iter().map(|q| q.definition()))
            || checkpoint.work.revision() <= prior.work.revision()
            || checkpoint.reads() < prior.reads())
    {
        return Err(RunJournalRepositoryError::InvalidInput);
    }
    for source in checkpoint
        .work
        .questions()
        .iter()
        .filter_map(|q| q.result())
        .flat_map(|r| r.sources())
    {
        let mut rows = transaction.query("SELECT 1 FROM tool_evidence e JOIN tool_runs t ON t.tool_run_id=e.tool_run_id
            JOIN agent_replan_originals o ON o.run_id=t.run_id AND o.event_sequence=t.event_sequence AND o.evidence_id=e.evidence_id
            JOIN agent_replan_research_checkpoints c ON c.run_id=o.run_id AND c.event_sequence=o.event_sequence
            WHERE t.run_id=?1 AND t.snapshot_after_id=?2 AND c.snapshot_id=?2 AND c.step_id=?11
            AND t.status='succeeded' AND e.location_kind='span' AND e.repository_path=?3 AND e.content_hash=?4 AND e.start_byte=?5 AND e.end_byte=?6 AND e.start_row=?7 AND e.start_column=?8 AND e.end_row=?9 AND e.end_column=?10 LIMIT 1",
            params![id_bytes(run.id()),id_bytes(checkpoint.snapshot_id),source.revision.path().as_bytes().to_vec(), source.revision.content_hash().as_bytes().to_vec(),
                i64::from(source.range.start_byte()), i64::from(source.range.end_byte()), i64::from(source.range.start_position().row()), i64::from(source.range.start_position().column()), i64::from(source.range.end_position().row()), i64::from(source.range.end_position().column()), checkpoint.step_id.as_bytes().to_vec()])
            .await.map_err(RunJournalRepositoryError::Read)?;
        if rows
            .next()
            .await
            .map_err(RunJournalRepositoryError::Read)?
            .is_none()
        {
            return Err(RunJournalRepositoryError::InvalidInput);
        }
    }
    let payload = work_state::encode(&checkpoint.work)
        .map_err(|_| RunJournalRepositoryError::InvalidInput)?;
    transaction.execute("INSERT INTO agent_replan_research_checkpoints (run_id,event_sequence,step_id,snapshot_id,payload) VALUES (?1,?2,?3,?4,?5)",
        params![id_bytes(run.id()), sequence_to_i64(event.sequence())?, checkpoint.step_id.as_bytes().to_vec(), id_bytes(checkpoint.snapshot_id), payload])
        .await.map_err(classify_unexpected_constraint)?;
    Ok(())
}
