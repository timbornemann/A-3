//! Dedicated exact B material/activation prefix. No generic historical origin bypass.
use super::closed_no_change::AuthenticatedClosedNoChange;
use super::origin::{OriginFrame, SqlOriginFrame};
use super::*;
use crate::http_context_intent_repository as context_store;
use a3_application::{
    AppliedHttpNoChangeWindowOrigin, ClosedEditNoChangeFact, ClosedHttpNoChangeOrigin,
    EditPhaseState, HttpCorrectionOrigin, HttpCorrectionPlanDecision, HttpCorrectionProducer,
    HttpNoChangeReplanSettlement, HttpNoChangeWindowMaterialProof, TaskLedgerStoreVersion,
    derive_http_correction_material_plan, derive_http_no_change_window_material_plan,
};

// Constructible only in this composition after actual A/call/first-prefix checks.
// The original physical receipt is read once; it never calls an origin loader.
/// Private receipt purpose for exactly the authenticated old-F/A/accepted-V6/B-prefix.
pub(crate) struct AppliedWindowMismatchPurpose<'a> {
    anchor: &'a HttpCorrectionPlanAnchor,
    first: &'a RunEvent,
    applied: &'a RunEvent,
    closed: &'a AuthenticatedClosedNoChange,
    settlement: &'a HttpNoChangeReplanSettlement,
    retired: [TaskStepId; 2],
    revision: TaskLedgerRevision,
    version: TaskLedgerStoreVersion,
    tail: RunEventSequence,
}
impl AppliedWindowMismatchPurpose<'_> {
    /// Original physical HTTP-failure anchor.
    pub(crate) fn anchor(&self) -> &HttpCorrectionPlanAnchor {
        self.anchor
    }
    /// Actual first material revision event.
    pub(crate) fn first_applied(&self) -> &RunEvent {
        self.first
    }
    /// Actual second material candidate, rederived before the origin is returned.
    pub(crate) fn applied(&self) -> &RunEvent {
        self.applied
    }
    /// Authentic old byte-identical call and source-read coordinates.
    pub(crate) fn fact(&self) -> &ClosedEditNoChangeFact {
        self.closed.fact()
    }
    /// Exact old A closure, independent of the current journal tail.
    pub(crate) fn closure(&self) -> &RunEvent {
        self.closed.closure()
    }
    /// Authentic accepted V6 call; never model-selected outcome authority.
    pub(crate) fn settlement(&self) -> &HttpNoChangeReplanSettlement {
        self.settlement
    }
    /// Exact old first correction and unstarted HTTP proof.
    pub(crate) const fn retired_steps(&self) -> &[TaskStepId; 2] {
        &self.retired
    }
    /// Actual current material Ledger revision.
    pub(crate) const fn revision(&self) -> TaskLedgerRevision {
        self.revision
    }
    /// Actual current stored Ledger CAS version.
    pub(crate) const fn version(&self) -> TaskLedgerStoreVersion {
        self.version
    }
    /// Actual current journal tail from the authenticated frame.
    pub(crate) const fn tail(&self) -> RunEventSequence {
        self.tail
    }
}

/// Reads only the exact N+1..N+5 material/activation/read prefix.
pub(super) fn load<'a>(
    store: &'a LibsqlKnowledgeStore,
    frame: OriginFrame<'a>,
) -> EditPhaseReservationFuture<'a, AppliedHttpNoChangeWindowOrigin> {
    Box::pin(async move {
        let connection = open(store, frame.project).await?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result = load_from_transaction(&tx, frame).await;
        close(tx, result).await
    })
}

/// Composes all genuine A/V6/material/history checks in the caller's read transaction.
pub(super) async fn load_from_transaction(
    tx: &Transaction,
    frame: OriginFrame<'_>,
) -> Result<AppliedHttpNoChangeWindowOrigin, E> {
    let sql: SqlOriginFrame<'_> = frame.into();
    let worktree = frame.project.worktree().id();
    let auth = super::closed_no_change::authenticate_applied_fact(tx, sql).await?;
    let fact = auth.fact();
    let owner = fact.owner();
    let settlement = super::no_change_replan::accepted_window_for_applied(tx, sql, &auth).await?;
    if recovery::has_unreconciled_mutation(tx, worktree)
        .await
        .map_err(unavailable)?
    {
        return Err(E::MutationRecoveryRequired);
    }
    let parent =
        context_store::selected_parent(tx, worktree, frame.goal, frame.run, frame.profile).await?;
    let anchor = parent.anchor.clone();
    let (producer, job, focus, intent, accepted_at) = match parent.proposal.decision() {
        HttpCorrectionPlanDecision::FocusLine(focus) => (
            HttpCorrectionProducer::DirectV4Focus {
                job: parent.job,
                proposal: parent.proposal.clone(),
            },
            parent.job,
            focus,
            parent.proposal.intent().clone(),
            parent.accepted_at,
        ),
        HttpCorrectionPlanDecision::NeedOfferedSource(_) => {
            let context = context_store::load_applied_context(
                tx,
                frame.project,
                frame.goal,
                frame.run,
                frame.profile,
                &parent,
            )
            .await?;
            let intent = context.reply.intent().ok_or(E::StaleAnchor)?.clone();
            (
                HttpCorrectionProducer::ContextV1Intent {
                    anchor: Box::new(context.anchor.clone()),
                    job: context.job,
                    parent_proposal: parent.proposal.clone(),
                    reply: context.reply,
                    accepted_wire: context.accepted_wire,
                },
                context.job,
                context.anchor.focus,
                intent,
                context.accepted_at,
            )
        }
        _ => return Err(E::StaleAnchor),
    };
    let p = producer
        .application_tail(&anchor)
        .map_err(|_| E::StaleAnchor)?;
    let first_revision = TaskLedgerRevision::new(
        anchor
            .ledger
            .get()
            .checked_add(1)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)?;
    let ledger = frame.stored.ledger();
    if anchor.worktree != worktree
        || anchor.run != owner.run
        || anchor.goal != owner.goal
        || anchor.original != owner.original_goal
        || anchor.index != owner.index
        || anchor.snapshot != owner.snapshot
        || anchor.profile != owner.profile
        || anchor.source != *fact.window().revision()
        || first_revision != owner.ledger
        || anchor.version.get().checked_add(2) != Some(owner.version.get())
        || owner.sequence != offset(p, 5)?
        || auth.closure().sequence() != offset(p, 6)?
    {
        return Err(E::StaleAnchor);
    }
    let original = ledger.step(anchor.step).ok_or(E::StaleAnchor)?;
    let failed_attempt = original.attempts().last().ok_or(E::StaleAnchor)?;
    let failure = event_at(tx, owner.run, anchor.sequence).await?;
    if original.retired_in_revision() != Some(first_revision)
        || original.status() != TaskStepStatus::Failed
        || original.definition().verification_spec().id() != anchor.verification
        || failed_attempt.number() != anchor.failed_attempt
        || failed_attempt.run_id() != owner.run
        || !matches!(
            failed_attempt.outcome(),
            TaskStepAttemptOutcome::Failed { .. }
        )
        || failed_attempt.finished_at().map(|at| at.unix_millis())
            != Some(failure.occurred_at().unix_millis())
        || !plain(&failure, owner.run, owner.snapshot)
        || failure.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || failure.payload()
            != &RunEventPayload::new(
                RunEventCode::VerificationFailure,
                Some(RunEventOutcome::Failed),
                None,
            )
        || parent.reserved_at < failure.occurred_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    let first =
        derive_http_correction_material_plan(&anchor, job, original.definition(), focus, &intent)
            .map_err(|_| E::StaleAnchor)?;
    let first_applied = event_at(tx, owner.run, offset(p, 1)?).await?;
    if first_applied.id() != first.event_id()
        || !plain(&first_applied, owner.run, owner.snapshot)
        || first_applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: anchor.ledger,
                to: first_revision,
            })
        || first_applied.payload() != &success()
        || first_applied.occurred_at() < accepted_at
        || first_applied.occurred_at() < failure.occurred_at()
    {
        return Err(E::StaleAnchor);
    }
    let first_replan = ledger
        .replans()
        .iter()
        .find(|r| r.revision() == first_revision)
        .ok_or(E::StaleAnchor)?;
    let mut first_added = vec![first.change().id(), first.proof().id()];
    first_added.sort_unstable();
    if first_replan.previous_revision() != anchor.ledger
        || first_replan.cause() != TaskLedgerReplanCause::ErrorCorrection
        || first_replan.retired_step_ids() != [anchor.step]
        || first_replan.added_step_ids() != first_added
        || first_replan.created_at().unix_millis() != first_applied.occurred_at().unix_millis()
        || first_replan.reason().as_str()
            != "Separate the accepted source correction from the unchanged HTTP proof"
    {
        return Err(E::StaleAnchor);
    }
    let old_change = ledger.step(first.change().id()).ok_or(E::StaleAnchor)?;
    let old_proof = ledger.step(first.proof().id()).ok_or(E::StaleAnchor)?;
    if old_change.definition() != first.change()
        || old_proof.definition() != first.proof()
        || old_change.introduced_in_revision() != first_revision
        || old_proof.introduced_in_revision() != first_revision
        || old_change.retired_in_revision() != Some(ledger.revision())
        || old_proof.retired_in_revision() != Some(ledger.revision())
        || old_change.status() != TaskStepStatus::Failed
        || old_proof.status() != TaskStepStatus::Pending
        || !old_proof.attempts().is_empty()
        || owner.step != first.change().id()
        || owner.verification != first.change().verification_spec().id()
    {
        return Err(E::StaleAnchor);
    }
    let (last, execute_at) = activation_prefix(
        tx,
        owner.run,
        owner.snapshot,
        p,
        first.activation_events(),
        3,
        first_applied.clone(),
    )
    .await?;
    let [attempt] = old_change.attempts() else {
        return Err(E::StaleAnchor);
    };
    if attempt.started_at().unix_millis() != execute_at.ok_or(E::StaleAnchor)?.unix_millis()
        || attempt.finished_at().map(|at| at.unix_millis())
            != Some(auth.closure().occurred_at().unix_millis())
    {
        return Err(E::StaleAnchor);
    }
    let old_read = event_at(tx, owner.run, offset(p, 5)?).await?;
    require_read_event(&old_read, owner.run, owner.snapshot, last.occurred_at())?;
    super::origin::require_closed_target_read(
        tx,
        &anchor,
        p,
        focus,
        first.change().id(),
        &old_read,
        &auth,
    )
    .await?;
    if auth.closure().occurred_at() < old_read.occurred_at() {
        return Err(E::StaleAnchor);
    }
    let n = auth.closure().sequence();
    let applied = event_at(tx, owner.run, offset(n, 1)?).await?;
    if !plain(&applied, owner.run, owner.snapshot)
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: owner.ledger,
                to: ledger.revision(),
            })
        || applied.payload() != &success()
        || applied.occurred_at() < settlement.observation().finished_at()
        || applied.occurred_at() >= settlement.deadline()
    {
        return Err(E::StaleAnchor);
    }
    let purpose = AppliedWindowMismatchPurpose {
        anchor: &anchor,
        first: &first_applied,
        applied: &applied,
        closed: &auth,
        settlement: &settlement,
        retired: [first.change().id(), first.proof().id()],
        revision: ledger.revision(),
        version: frame.stored.version(),
        tail: frame.run.last_event_sequence(),
    };
    let diagnostic =
        recovery::mismatch::load_for_applied_http_no_change_window(tx, worktree, &purpose)
            .await
            .map_err(|error| match error.classify() {
                a3_application::AgentRecoveryStoreFailure::Unavailable => E::Unavailable,
                _ => E::StaleAnchor,
            })?
            .ok_or(E::StaleAnchor)?;
    if diagnostic.digest() != anchor.diagnostic || diagnostic.script() != &anchor.source {
        return Err(E::StaleAnchor);
    }
    let closed = ClosedHttpNoChangeOrigin::from_recorded(
        HttpCorrectionOrigin {
            anchor,
            producer,
            diagnostic,
            applied_event: first_applied,
        },
        fact.clone(),
        auth.closure().clone(),
        TaskLedgerStoreVersion::new(
            owner
                .version
                .get()
                .checked_add(1)
                .ok_or(E::InvalidStoredData)?,
        )
        .map_err(|_| E::InvalidStoredData)?,
    )?;
    super::no_change_replan::verify_applied_anchor(tx, frame.goal, &auth, &closed).await?;
    let material =
        HttpNoChangeWindowMaterialProof::from_recorded(closed, settlement, frame.profile.clone())
            .map_err(|_| E::StaleAnchor)?;
    let plan = derive_http_no_change_window_material_plan(&material, first.change(), first.proof())
        .map_err(|_| E::StaleAnchor)?;
    if applied.id() != plan.event_id() {
        return Err(E::StaleAnchor);
    }
    let origin = AppliedHttpNoChangeWindowOrigin::from_recorded(material, applied.clone())
        .map_err(|_| E::StaleAnchor)?;
    let replan = ledger.replans().last().ok_or(E::StaleAnchor)?;
    let mut retired = plan.retired_steps().to_vec();
    retired.sort_unstable();
    let mut added = vec![plan.change().id(), plan.proof().id()];
    added.sort_unstable();
    if replan.previous_revision() != owner.ledger
        || replan.revision() != ledger.revision()
        || replan.cause() != TaskLedgerReplanCause::ErrorCorrection
        || replan.retired_step_ids() != retired
        || replan.added_step_ids() != added
        || replan.reason() != plan.reason()
        || replan.created_at().unix_millis() != applied.occurred_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    let VerificationTarget::LoopbackHttp(recipe) = first.proof().verification_spec().target()
    else {
        return Err(E::StaleAnchor);
    };
    let corrections = ledger.replans().iter().filter(|r| r.cause() == TaskLedgerReplanCause::ErrorCorrection)
        .flat_map(|r| r.retired_step_ids()).filter(|id| ledger.step(**id).is_some_and(|step|
            matches!(step.definition().verification_spec().target(), VerificationTarget::LoopbackHttp(old) if old == recipe))).count();
    if corrections != 2 {
        return Err(E::StaleAnchor);
    }
    let delta = frame
        .run
        .last_event_sequence()
        .get()
        .checked_sub(n.get())
        .ok_or(E::StaleAnchor)?;
    let edge_count =
        usize::try_from(delta.saturating_sub(1).min(3)).map_err(|_| E::InvalidStoredData)?;
    let (mut last, execute_at) = activation_prefix(
        tx,
        owner.run,
        owner.snapshot,
        n,
        plan.activation_events(),
        edge_count,
        applied,
    )
    .await?;
    let change = ledger.step(plan.change().id()).ok_or(E::StaleAnchor)?;
    let proof = ledger.step(plan.proof().id()).ok_or(E::StaleAnchor)?;
    if change.definition() != plan.change()
        || proof.definition() != plan.proof()
        || !change.is_active_plan_step()
        || !proof.is_active_plan_step()
        || change.introduced_in_revision() != ledger.revision()
        || proof.introduced_in_revision() != ledger.revision()
        || proof.status() != TaskStepStatus::Pending
        || !proof.attempts().is_empty()
        || ledger.steps().any(|step| {
            step.is_active_plan_step()
                && step.definition().id() != change.definition().id()
                && step.definition().id() != proof.definition().id()
                && (step.status() != TaskStepStatus::Completed
                    || step
                        .definition()
                        .parent_step_id()
                        .is_some_and(|id| retired.contains(&id))
                    || step
                        .definition()
                        .dependencies()
                        .iter()
                        .any(|d| retired.contains(&d.prerequisite())))
        })
    {
        return Err(E::StaleAnchor);
    }
    if let Some(at) = execute_at {
        let [attempt] = change.attempts() else {
            return Err(E::StaleAnchor);
        };
        if change.status() != TaskStepStatus::InProgress
            || attempt.number().get() != 1
            || attempt.run_id() != owner.run
            || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
            || attempt.finished_at().is_some()
            || attempt.started_at().unix_millis() != at.unix_millis()
            || ledger.updated_at().unix_millis() != at.unix_millis()
        {
            return Err(E::StaleAnchor);
        }
    } else if change.status() != TaskStepStatus::Ready
        || !change.attempts().is_empty()
        || ledger.updated_at().unix_millis() != origin.applied_event().occurred_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    if delta == 5 {
        let read = event_at(tx, owner.run, offset(n, 5)?).await?;
        require_read_event(&read, owner.run, owner.snapshot, last.occurred_at())?;
        require_window_target_read(tx, frame, origin.material(), plan.change().id(), &read).await?;
        last = read;
    } else {
        require_no_read(tx, owner.run, plan.change().id()).await?;
    }
    if last.sequence() != frame.run.last_event_sequence()
        || last.occurred_at() != frame.run.updated_at()
        || frame.now < last.occurred_at()
    {
        return Err(E::StaleAnchor);
    }
    let mut tail = tx
        .query(
            "SELECT MAX(event_sequence) FROM run_events WHERE run_id=?1",
            params![owner.run.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    if n64(
        &tail
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )? != last.sequence().get()
    {
        return Err(E::StaleAnchor);
    }
    Ok(origin)
}

async fn require_window_target_read(
    tx: &Transaction,
    frame: OriginFrame<'_>,
    material: &HttpNoChangeWindowMaterialProof,
    change: TaskStepId,
    read: &RunEvent,
) -> Result<(), E> {
    let Some(RunEventSubject::Tool(tool)) = read.subject() else {
        return Err(E::StaleAnchor);
    };
    let edit = EditPhaseState::new(
        frame.project,
        frame.goal,
        frame.stored,
        frame.run,
        frame.current,
    )
    .map_err(|_| E::StaleAnchor)?
    .anchor();
    let state = crate::source_read_repository::load_recorded_state_in_transaction(tx, edit)
        .await
        .map_err(|_| E::StaleAnchor)?;
    let acquired = state.acquired().ok_or(E::StaleAnchor)?;
    if !state.progress().receipts().is_empty()
        || !state.caller_relocalizations().is_empty()
        || state
            .progress()
            .pending()
            .is_none_or(|p| p.ordinal().get() != 1)
        || acquired.required() != material.selected_window()
        || acquired.original() != material.selected_window()
        || acquired.key() == material.closed_origin().fact().source_read_key()
    {
        return Err(E::StaleAnchor);
    }
    let mut rows = tx.query("SELECT 1 FROM source_read_acquisitions s JOIN tool_run_attempts a
        ON a.tool_run_id=s.tool_run_id AND a.attempt_sequence=s.tool_attempt
        JOIN run_events j ON j.run_id=s.run_id AND j.event_sequence=s.after_sequence+1
        JOIN run_events b ON b.run_id=s.run_id AND b.event_sequence=s.after_sequence
        WHERE s.run_id=?1 AND s.step_id=?2 AND s.attempt_number=1 AND s.ordinal=1
        AND s.source_read_id=?3 AND s.tool_run_id=?4 AND s.after_sequence=?5 AND s.abandoned=0
        AND a.started_at_unix_millis>=b.occurred_at_unix_millis
        AND a.updated_at_unix_millis=j.occurred_at_unix_millis AND a.started_at_unix_millis<=a.updated_at_unix_millis
        AND j.event_id=?6 AND j.event_kind='tool_action' AND j.subject_kind='tool' AND j.subject_id=s.tool_run_id
        AND j.payload_outcome='succeeded' AND j.snapshot_id=?7
        AND (SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2)=1
        AND (SELECT COUNT(*) FROM source_caller_relocalizations WHERE run_id=?1 AND step_id=?2)=0",
        params![frame.run.id().as_bytes().to_vec(),change.as_bytes().to_vec(),acquired.key().as_bytes().to_vec(),
            tool.as_bytes().to_vec(),i64::try_from(offset(material.closed_origin().closure().sequence(),4)?.get()).map_err(|_|E::InvalidStoredData)?,
            read.id().as_bytes().to_vec(),frame.run.current_snapshot_id().as_bytes().to_vec()]).await.map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    Ok(())
}
pub(super) async fn activation_prefix(
    tx: &Transaction,
    run: AgentRunId,
    snapshot: SnapshotId,
    base: RunEventSequence,
    ids: &[RunEventId; 3],
    count: usize,
    mut last: RunEvent,
) -> Result<(RunEvent, Option<AgentRunTimestamp>), E> {
    let states = [
        AgentControllerState::Replan,
        AgentControllerState::Localize,
        AgentControllerState::Plan,
        AgentControllerState::Execute,
    ];
    let mut execute = None;
    for index in 0..count {
        let event = event_at(
            tx,
            run,
            offset(
                base,
                u64::try_from(index).map_err(|_| E::InvalidStoredData)? + 2,
            )?,
        )
        .await?;
        if event.id() != ids[index]
            || !plain(&event, run, snapshot)
            || event.kind()
                != (RunEventKind::StateTransition {
                    from: states[index],
                    to: states[index + 1],
                })
            || event.payload() != &success()
            || event.occurred_at() < last.occurred_at()
        {
            return Err(E::StaleAnchor);
        }
        if index == 2 {
            execute = Some(event.occurred_at());
        }
        last = event;
    }
    Ok((last, execute))
}
pub(super) fn require_read_event(
    event: &RunEvent,
    run: AgentRunId,
    snapshot: SnapshotId,
    after: AgentRunTimestamp,
) -> Result<(), E> {
    if event.run_id() != run
        || event.snapshot_id() != snapshot
        || event.kind() != RunEventKind::ToolAction
        || event.occurred_at() < after
        || event.payload().code() != RunEventCode::None
        || event.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || event.turn_charge().is_some()
        || event
            .payload()
            .redaction()
            .is_none_or(|r| r.source() != RunEventRedactionSource::ToolOutput)
        || !matches!(event.subject(), Some(RunEventSubject::Tool(_)))
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}
async fn require_no_read(tx: &Transaction, run: AgentRunId, step: TaskStepId) -> Result<(), E> {
    let mut rows=tx.query("SELECT (SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2)+(SELECT COUNT(*) FROM source_caller_relocalizations WHERE run_id=?1 AND step_id=?2)",params![run.as_bytes().to_vec(),step.as_bytes().to_vec()]).await.map_err(unavailable)?;
    if n64(
        &rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )? != 0
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}
fn success() -> RunEventPayload {
    RunEventPayload::new(
        RunEventCode::ControllerDecision,
        Some(RunEventOutcome::Succeeded),
        None,
    )
}
fn plain(event: &RunEvent, run: AgentRunId, snapshot: SnapshotId) -> bool {
    event.run_id() == run
        && event.snapshot_id() == snapshot
        && event.subject().is_none()
        && event.turn_charge().is_none()
}
fn offset(base: RunEventSequence, by: u64) -> Result<RunEventSequence, E> {
    RunEventSequence::new(base.get().checked_add(by).ok_or(E::InvalidStoredData)?)
        .map_err(|_| E::InvalidStoredData)
}
pub(super) async fn event_at(
    tx: &Transaction,
    run: AgentRunId,
    sequence: RunEventSequence,
) -> Result<RunEvent, E> {
    journal::read_event(tx, run, sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)
}
fn n64(row: &Row, at: i32) -> Result<u64, E> {
    u64::try_from(row.get::<i64>(at).map_err(|_| E::InvalidStoredData)?)
        .map_err(|_| E::InvalidStoredData)
}
