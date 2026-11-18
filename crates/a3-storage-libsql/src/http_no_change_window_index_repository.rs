//! Exact accepted B2 publication at ordinary read/edit boundaries, without model admission.
use super::*;
use a3_application::{
    BoundEditSource, EditPhaseAnchor, StoredTaskLedger, http_no_change_window_material_identity,
};

const MATERIAL_REASON: &str = "Replace the actual byte-identical failed correction with a strictly smaller source window and preserve the complete HTTP proof";

struct CurrentWindow {
    index: IndexRunId,
    required: EvidenceRef,
}

/// Adds no permission: non-B2 edits keep their old contract, actual B2 material
/// must retain its exact publication even when a new index has the same snapshot.
pub(crate) async fn guard_live(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
    stored: &StoredTaskLedger,
) -> Result<(), E> {
    Box::pin(current_window(tx, worktree, run, stored))
        .await
        .map(|_| ())
}

pub(crate) async fn guard_bound_source(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
    source: &BoundEditSource,
) -> Result<(), E> {
    let (run, stored) = current_rows(tx, anchor.worktree(), anchor.run()).await?;
    if let Some(window) = Box::pin(current_window(tx, anchor.worktree(), &run, &stored)).await?
        && (source.index_run_id() != window.index || source.required() != &window.required)
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}

// Runs before the actual Original marker is written, in the same Journal transaction.
pub(crate) async fn guard_original_commit(
    tx: &Transaction,
    worktree: WorktreeId,
    run_id: AgentRunId,
    previous: RunEventSequence,
    required: &EvidenceRef,
) -> Result<(), E> {
    let (run, stored) = current_rows(tx, worktree, run_id).await?;
    if let Some(window) = Box::pin(current_window(tx, worktree, &run, &stored)).await?
        && (previous.get().checked_add(1) != Some(run.last_event_sequence().get())
            || required != &window.required)
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}

async fn current_rows(
    tx: &Transaction,
    worktree: WorktreeId,
    run_id: AgentRunId,
) -> Result<(AgentRun, StoredTaskLedger), E> {
    let run = journal::load_run_from_transaction(tx, worktree, run_id)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let stored = ledgers::load_from_transaction(tx, worktree, run.goal_contract().task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    Ok((run, stored))
}

async fn current_window(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
    stored: &StoredTaskLedger,
) -> Result<Option<CurrentWindow>, E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < 68
    {
        return Ok(None);
    }
    let ledger = stored.ledger();
    let mut active = ledger
        .steps()
        .filter(|step| step.is_active_plan_step() && step.status() == TaskStepStatus::InProgress);
    let Some(change) = active.next() else {
        return Ok(None);
    };
    if active.next().is_some() {
        return Err(E::StaleAnchor);
    }
    // A successful B patch legitimately publishes a new snapshot before its
    // separate full HTTP proof. Only the Addresses correction owns this guard.
    if !matches!(
        change.definition().verification_spec().target(),
        VerificationTarget::DiffInvariant(_)
    ) {
        return Ok(None);
    }
    let marker = ledger.replans().last().is_some_and(|r| {
        r.revision() == ledger.revision() && r.reason().as_str() == MATERIAL_REASON
    });
    let mut rows = tx
        .query(
            "SELECT COUNT(*) FROM http_no_change_replan_reservations WHERE run_id=?1",
            params![run.id().as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let count: i64 = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::InvalidStoredData)?
        .get(0)
        .map_err(|_| E::InvalidStoredData)?;
    if count == 0 && !marker {
        return Ok(None);
    }
    if !(1..=2).contains(&count) {
        return Err(E::InvalidStoredData);
    }
    let goal_ref = run.goal_contract();
    let goal = goals::load_revision_from_transaction(
        tx,
        worktree,
        goal_ref.task_id(),
        goal_ref.revision(),
    )
    .await
    .map_err(|_| E::InvalidStoredData)?
    .ok_or(E::StaleAnchor)?;
    if goals::task_current_revision(tx, worktree, goal_ref.task_id())
        .await
        .map_err(unavailable)?
        != Some(goal_ref.revision())
    {
        return Err(E::StaleAnchor);
    }
    let fact =
        super::closed_no_change::read_fact_for_replan_origin(tx, worktree, &goal, run).await?;
    let auth = Box::pin(super::closed_no_change::authenticate_material_history_fact(
        tx, worktree, &goal, stored, run, fact,
    ))
    .await?;
    let fact = auth.fact();
    let settlement =
        super::no_change_replan::accepted_window_for_current_edit_index(tx, &goal, &auth).await?;
    let (steps, material_event, edges) = http_no_change_window_material_identity(fact, &settlement)
        .map_err(|_| E::InvalidStoredData)?;
    let material_revision =
        require_material_scope_identity(tx, stored, &auth, &settlement, steps, material_event)
            .await?;
    let original_attempt = change
        .attempts()
        .last()
        .is_some_and(|attempt| attempt.number() == TaskStepAttemptNumber::FIRST);
    if change.definition().id() != steps[0]
        || ledger.revision() != material_revision
        || !original_attempt
    {
        // This is another ordinary Core-admitted scope, not the original B edit.
        // Its normal live/source/usage/CAS contracts remain the admission authority.
        return Ok(None);
    }
    super::closed_no_change::require_current_edit_frame(&auth, stored, run)?;
    if !marker {
        return Err(E::StaleAnchor);
    }
    let Some(a3_application::HttpNoChangeReplanDecision::Window {
        original, intent, ..
    }) = settlement.decision()
    else {
        return Err(E::StaleAnchor);
    };
    let owner = fact.owner();
    let n = auth.closure().sequence();
    let applied = super::no_change_window_origin::event_at(tx, run.id(), plus(n, 1)?).await?;
    if applied.id() != material_event
        || applied.run_id() != owner.run
        || applied.snapshot_id() != owner.snapshot
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: owner.ledger,
                to: ledger.revision(),
            })
        || applied.payload() != &success()
        || applied.subject().is_some()
        || applied.turn_charge().is_some()
        || applied.occurred_at() < settlement.observation().finished_at()
        || applied.occurred_at() >= settlement.deadline()
    {
        return Err(E::StaleAnchor);
    }
    let replan = ledger.replans().last().ok_or(E::StaleAnchor)?;
    let mut added = steps.to_vec();
    added.sort_unstable();
    let old = ledger.step(owner.step).ok_or(E::StaleAnchor)?;
    let old_proof = ledger
        .steps()
        .find(|step| {
            step.definition().dependencies() == [StepDependency::new(owner.step)]
                && step.retired_in_revision() == Some(ledger.revision())
        })
        .ok_or(E::StaleAnchor)?;
    let proof = ledger.step(steps[1]).ok_or(E::StaleAnchor)?;
    let mut retired = vec![owner.step, old_proof.definition().id()];
    retired.sort_unstable();
    let VerificationTarget::LoopbackHttp(old_recipe) =
        old_proof.definition().verification_spec().target()
    else {
        return Err(E::StaleAnchor);
    };
    if replan.previous_revision() != owner.ledger
        || replan.revision() != ledger.revision()
        || replan.cause() != TaskLedgerReplanCause::ErrorCorrection
        || replan.added_step_ids() != added
        || replan.retired_step_ids() != retired
        || replan.created_at().unix_millis() != applied.occurred_at().unix_millis()
        || change.introduced_in_revision() != ledger.revision()
        || change.attempts().len() != 1
        || change.definition().intended_outcome().as_str() != intent.as_str()
        || change.definition().addressed_criteria() != old.definition().addressed_criteria()
        || !change.definition().acceptance_criteria().is_empty()
        || !matches!(change.definition().verification_spec().target(), VerificationTarget::DiffInvariant(diff)
            if diff.mode()==DiffInvariantMode::ExactPaths && diff.paths()==std::slice::from_ref(original.revision().path()))
        || !proof.is_active_plan_step()
        || proof.status() != TaskStepStatus::Pending
        || !proof.attempts().is_empty()
        || proof.introduced_in_revision() != ledger.revision()
        || proof.definition().dependencies() != [StepDependency::new(steps[0])]
        || proof.definition().acceptance_criteria() != old_proof.definition().acceptance_criteria()
        || !proof.definition().addressed_criteria().is_empty()
        || !matches!(proof.definition().verification_spec().target(), VerificationTarget::LoopbackHttp(recipe) if recipe==old_recipe)
        || old_proof.status() != TaskStepStatus::Pending
        || !old_proof.attempts().is_empty()
        || old_proof.is_active_plan_step()
    {
        return Err(E::StaleAnchor);
    }
    let (mut last, execute) = super::no_change_window_origin::activation_prefix(
        tx,
        owner.run,
        owner.snapshot,
        n,
        &edges,
        3,
        applied,
    )
    .await?;
    let attempt = &change.attempts()[0];
    if attempt.number().get() != 1
        || attempt.run_id() != owner.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
        || attempt.finished_at().is_some()
        || Some(attempt.started_at().unix_millis()) != execute.map(|at| at.unix_millis())
    {
        return Err(E::StaleAnchor);
    }
    if run.last_event_sequence() == plus(n, 5)? {
        let read = super::no_change_window_origin::event_at(tx, owner.run, plus(n, 5)?).await?;
        super::no_change_window_origin::require_read_event(
            &read,
            owner.run,
            owner.snapshot,
            last.occurred_at(),
        )?;
        last = read;
    }
    if last.sequence() != run.last_event_sequence() || last.occurred_at() != run.updated_at() {
        return Err(E::StaleAnchor);
    }
    let mut latest=tx.query("SELECT 1 FROM index_runs i JOIN file_revisions f USING(index_run_id)
        WHERE i.worktree_id=?1 AND i.status='published' AND i.index_run_id=?2 AND i.snapshot_id=?3
        AND i.run_sequence=(SELECT MAX(run_sequence) FROM index_runs WHERE worktree_id=?1 AND status='published')
        AND f.repository_path=?4 AND f.content_hash=?5",params![worktree.as_bytes().to_vec(),owner.index.as_bytes().to_vec(),
            owner.snapshot.as_bytes().to_vec(),original.revision().path().as_bytes().to_vec(),original.revision().content_hash().as_bytes().to_vec()])
        .await.map_err(unavailable)?;
    if latest.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    Ok(Some(CurrentWindow {
        index: owner.index,
        required: original.clone(),
    }))
}
fn plus(base: RunEventSequence, delta: u64) -> Result<RunEventSequence, E> {
    RunEventSequence::new(base.get().checked_add(delta).ok_or(E::InvalidStoredData)?)
        .map_err(|_| E::InvalidStoredData)
}
fn success() -> RunEventPayload {
    RunEventPayload::new(
        RunEventCode::ControllerDecision,
        Some(RunEventOutcome::Succeeded),
        None,
    )
}

// Resolve only the durable identity of the old B scope. No completed-status,
// old-attempt-count, later-horizon, source-delivery or general recovery admission
// is introduced here. Revalidation and retirement may legitimately change those.
async fn require_material_scope_identity(
    tx: &Transaction,
    stored: &StoredTaskLedger,
    auth: &super::closed_no_change::AuthenticatedClosedNoChange,
    settlement: &a3_application::HttpNoChangeReplanSettlement,
    steps: [TaskStepId; 2],
    event_id: RunEventId,
) -> Result<TaskLedgerRevision, E> {
    let owner = auth.fact().owner();
    let revision = TaskLedgerRevision::new(
        owner
            .ledger
            .get()
            .checked_add(1)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)?;
    let ledger = stored.ledger();
    let material = ledger
        .replans()
        .iter()
        .find(|record| record.revision() == revision)
        .ok_or(E::StaleAnchor)?;
    let change = ledger.step(steps[0]).ok_or(E::StaleAnchor)?;
    let proof = ledger.step(steps[1]).ok_or(E::StaleAnchor)?;
    let mut added = steps.to_vec();
    added.sort_unstable();
    let applied = super::no_change_window_origin::event_at(
        tx,
        owner.run,
        plus(auth.closure().sequence(), 1)?,
    )
    .await?;
    if ledger.revision() < revision
        || change.introduced_in_revision() != revision
        || proof.introduced_in_revision() != revision
        || material.previous_revision() != owner.ledger
        || material.cause() != TaskLedgerReplanCause::ErrorCorrection
        || material.reason().as_str() != MATERIAL_REASON
        || material.added_step_ids() != added
        || !material.retired_step_ids().contains(&owner.step)
        || material.created_at().unix_millis() != applied.occurred_at().unix_millis()
        || applied.id() != event_id
        || applied.run_id() != owner.run
        || applied.snapshot_id() != owner.snapshot
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: owner.ledger,
                to: revision,
            })
        || applied.payload() != &success()
        || applied.subject().is_some()
        || applied.turn_charge().is_some()
        || applied.occurred_at() < settlement.observation().finished_at()
        || applied.occurred_at() >= settlement.deadline()
    {
        return Err(E::StaleAnchor);
    }
    Ok(revision)
}
