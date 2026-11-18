//! Exact retained HTTP correction origin for the closed first correction; no action permission.
use super::*;
use crate::http_context_intent_repository as context_store;
use a3_application::{
    EditPhaseState, HttpCorrectionOrigin, HttpCorrectionPlanDecision, HttpCorrectionProducer,
    StoredTaskLedger, derive_http_correction_material_plan,
    http_correction_definition_page_matches, http_correction_definition_source,
};

use super::closed_no_change::AuthenticatedClosedNoChange;

/// One immutable read frame; neither purpose grants execution or source credit.
#[derive(Clone, Copy)]
pub(super) struct OriginFrame<'a> {
    pub(super) project: &'a ProjectIdentity,
    pub(super) goal: &'a GoalContract,
    pub(super) stored: &'a StoredTaskLedger,
    pub(super) run: &'a AgentRun,
    pub(super) current: &'a PublishedIndex,
    pub(super) profile: &'a ModelProfile,
    pub(super) now: AgentRunTimestamp,
}

/// SQL identity frame for the same bounded origin. No index payload is trusted as rows.
#[derive(Clone, Copy)]
pub(super) struct SqlOriginFrame<'a> {
    pub(super) project: &'a ProjectIdentity,
    pub(super) goal: &'a GoalContract,
    pub(super) stored: &'a StoredTaskLedger,
    pub(super) run: &'a AgentRun,
    pub(super) index: IndexRunId,
    pub(super) snapshot: SnapshotId,
    pub(super) profile: &'a ModelProfile,
    pub(super) now: AgentRunTimestamp,
}
impl<'a> From<OriginFrame<'a>> for SqlOriginFrame<'a> {
    fn from(frame: OriginFrame<'a>) -> Self {
        Self {
            project: frame.project,
            goal: frame.goal,
            stored: frame.stored,
            run: frame.run,
            index: frame.current.run().id(),
            snapshot: frame.current.run().snapshot_id(),
            profile: frame.profile,
            now: frame.now,
        }
    }
}

// Distinct private purposes. The Current entry cannot supply a closed token or
// acquire relaxed retired/read admission by choosing a boolean.
#[derive(Clone, Copy)]
enum OriginPurpose<'a> {
    Current(&'a PublishedIndex),
    ClosedNoChange(&'a AuthenticatedClosedNoChange),
    ClosedReplan(
        &'a AuthenticatedClosedNoChange,
        &'a super::no_change_replan::AuthenticatedReplanHistory,
    ),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    goal: &'a GoalContract,
    stored: &'a StoredTaskLedger,
    run: &'a AgentRun,
    current: &'a PublishedIndex,
    profile: &'a ModelProfile,
    now: AgentRunTimestamp,
) -> EditPhaseReservationFuture<'a, HttpCorrectionOrigin> {
    Box::pin(async move {
        let connection = open(store, project).await?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result = load_from_transaction(
            &tx,
            OriginFrame {
                project,
                goal,
                stored,
                run,
                current,
                profile,
                now,
            }
            .into(),
            OriginPurpose::Current(current),
        )
        .await;
        close(tx, result).await
    })
}

pub(super) async fn load_for_closed_no_change(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
    closed: &AuthenticatedClosedNoChange,
) -> Result<HttpCorrectionOrigin, E> {
    load_from_transaction(tx, frame, OriginPurpose::ClosedNoChange(closed)).await
}

pub(super) async fn load_for_closed_replan(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
    closed: &AuthenticatedClosedNoChange,
    history: &super::no_change_replan::AuthenticatedReplanHistory,
) -> Result<HttpCorrectionOrigin, E> {
    load_from_transaction(tx, frame, OriginPurpose::ClosedReplan(closed, history)).await
}

async fn load_from_transaction(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
    purpose: OriginPurpose<'_>,
) -> Result<HttpCorrectionOrigin, E> {
    let SqlOriginFrame {
        project,
        goal,
        stored,
        run,
        index,
        snapshot,
        profile,
        now,
    } = frame;
    let worktree = project.worktree().id();
    let durable_run = journal::load_run_from_transaction(tx, worktree, run.id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let durable_goal = goals::load_revision_from_transaction(
        tx,
        worktree,
        goal.task_id(),
        goal.reference().revision(),
    )
    .await
    .map_err(|_| E::InvalidStoredData)?
    .ok_or(E::StaleAnchor)?;
    let durable_ledger = ledgers::load_from_transaction(tx, worktree, goal.task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    if !profile.executable_actions_enabled()
        || run.model_profile() != Some(profile.reference())
        || durable_run != *run
        || durable_goal != *goal
        || durable_ledger != *stored
        || goals::task_current_revision(tx, worktree, goal.task_id())
            .await
            .map_err(unavailable)?
            != Some(goal.reference().revision())
        || run.goal_contract() != goal.reference()
        || stored.ledger().goal_contract() != goal.reference()
        || stored.ledger().pending_planning().is_some()
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
    {
        return Err(E::StaleAnchor);
    }
    match purpose {
        OriginPurpose::ClosedReplan(_, history) => history.require_frame(&frame)?,
        OriginPurpose::Current(_) | OriginPurpose::ClosedNoChange(_) => {
            check_current_clock_and_budget(tx, worktree, run, now).await?
        }
    }
    if recovery::has_unreconciled_mutation(tx, worktree)
        .await
        .map_err(unavailable)?
    {
        return Err(E::MutationRecoveryRequired);
    }
    // Select the actual latest canonical producer. No fallback or fabricated V4 Focus.
    let parent = context_store::selected_parent(tx, worktree, goal, run, profile).await?;
    let anchor = parent.anchor.clone();
    let reserved_at = parent.reserved_at;
    let (producer, job, focus, intent, accepted_at, context_read) = match parent.proposal.decision()
    {
        HttpCorrectionPlanDecision::FocusLine(focus) => (
            HttpCorrectionProducer::DirectV4Focus {
                job: parent.job,
                proposal: parent.proposal.clone(),
            },
            parent.job,
            focus,
            parent.proposal.intent().clone(),
            parent.accepted_at.unix_millis(),
            None,
        ),
        HttpCorrectionPlanDecision::NeedOfferedSource(_) => {
            let context =
                context_store::load_applied_context(tx, project, goal, run, profile, &parent)
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
                context.accepted_at.unix_millis(),
                Some(context.read),
            )
        }
        _ => return Err(E::StaleAnchor),
    };
    let application_tail = producer
        .application_tail(&anchor)
        .map_err(|_| E::StaleAnchor)?;
    if anchor.worktree != worktree
        || anchor.run != run.id()
        || anchor.goal != goal.reference()
        || goal.original().map(|v| v.digest()) != Some(anchor.original)
        || index != anchor.index
        || snapshot != anchor.snapshot
        || run.current_snapshot_id() != anchor.snapshot
        || anchor.profile != profile.reference()
    {
        return Err(E::StaleAnchor);
    }
    let ledger = stored.ledger();
    let revision = TaskLedgerRevision::new(
        anchor
            .ledger
            .get()
            .checked_add(1)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)?;
    if ledger.revision() != revision || run.task_ledger_revision() != revision {
        return Err(E::StaleAnchor);
    }
    let failed = ledger.step(anchor.step).ok_or(E::StaleAnchor)?;
    let attempt = failed.attempts().last().ok_or(E::StaleAnchor)?;
    let failure = event_at(tx, anchor.run, anchor.sequence).await?;
    if failure.kind()
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
        || !plain_event(&failure, &anchor)
        || failed.retired_in_revision() != Some(revision)
        || failed.status() != TaskStepStatus::Failed
        || attempt.number() != anchor.failed_attempt
        || attempt.run_id() != anchor.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|at| at.unix_millis())
            != Some(failure.occurred_at().unix_millis())
        || failed.definition().verification_spec().id() != anchor.verification
        || reserved_at < failure.occurred_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    // selected_parent already validated the same initial/repair history against full profile.
    let material =
        derive_http_correction_material_plan(&anchor, job, failed.definition(), focus, &intent)
            .map_err(|_| E::StaleAnchor)?;
    let applied = event_at(tx, anchor.run, offset(application_tail, 1)?).await?;
    if applied.id() != material.event_id()
        || !plain_event(&applied, &anchor)
        || applied.kind()
            != (RunEventKind::LedgerUpdated {
                from: anchor.ledger,
                to: revision,
            })
        || applied.payload() != &success_payload()
        || applied.occurred_at().unix_millis() < accepted_at
        || applied.occurred_at() < failure.occurred_at()
    {
        return Err(E::StaleAnchor);
    }
    let replan = ledger.replans().last().ok_or(E::StaleAnchor)?;
    let mut added = vec![material.change().id(), material.proof().id()];
    added.sort_unstable();
    if replan.previous_revision() != anchor.ledger
        || replan.revision() != revision
        || replan.cause() != TaskLedgerReplanCause::ErrorCorrection
        || replan.retired_step_ids() != [anchor.step]
        || replan.added_step_ids() != added
        || replan.created_at().unix_millis() != applied.occurred_at().unix_millis()
        || replan.reason().as_str()
            != "Separate the accepted source correction from the unchanged HTTP proof"
    {
        return Err(E::StaleAnchor);
    }
    let VerificationTarget::LoopbackHttp(recipe) = failed.definition().verification_spec().target()
    else {
        return Err(E::StaleAnchor);
    };
    // Exactly this one recipe correction is eligible; changing IDs or the horizon cannot reset it.
    let corrections=ledger.replans().iter().filter(|replan|replan.cause()==TaskLedgerReplanCause::ErrorCorrection)
        .flat_map(|replan|replan.retired_step_ids()).filter(|id|ledger.step(**id).is_some_and(|step|
            matches!(step.definition().verification_spec().target(),VerificationTarget::LoopbackHttp(old) if old==recipe))).count();
    if corrections != 1 {
        return Err(E::StaleAnchor);
    }
    let change = ledger.step(material.change().id()).ok_or(E::StaleAnchor)?;
    let proof = ledger.step(material.proof().id()).ok_or(E::StaleAnchor)?;
    if change.definition() != material.change()
        || proof.definition() != material.proof()
        || !change.is_active_plan_step()
        || !proof.is_active_plan_step()
        || change.introduced_in_revision() != revision
        || proof.introduced_in_revision() != revision
        || proof.status() != TaskStepStatus::Pending
        || !proof.attempts().is_empty()
    {
        return Err(E::StaleAnchor);
    }
    let delta = run
        .last_event_sequence()
        .get()
        .checked_sub(application_tail.get())
        .ok_or(E::StaleAnchor)?;
    match purpose {
        OriginPurpose::Current(_) if !(1..=5).contains(&delta) => return Err(E::StaleAnchor),
        OriginPurpose::ClosedNoChange(closed) | OriginPurpose::ClosedReplan(closed, _) => {
            let owner = closed.fact().owner();
            if delta != 6
                || owner.worktree != worktree
                || owner.goal != anchor.goal
                || owner.original_goal != anchor.original
                || owner.run != run.id()
                || owner.ledger != revision
                || owner.index != anchor.index
                || owner.snapshot != anchor.snapshot
                || owner.profile != anchor.profile
                || owner.step != material.change().id()
                || owner.verification != material.change().verification_spec().id()
                || owner.sequence != offset(application_tail, 5)?
                || anchor.version.get().checked_add(2) != Some(owner.version.get())
            {
                return Err(E::StaleAnchor);
            }
        }
        OriginPurpose::Current(_) => {}
    }
    let states = [
        AgentControllerState::Replan,
        AgentControllerState::Localize,
        AgentControllerState::Plan,
        AgentControllerState::Execute,
    ];
    let expected_state = match purpose {
        OriginPurpose::Current(_) => {
            states[usize::try_from(delta.min(4) - 1).map_err(|_| E::InvalidStoredData)?]
        }
        OriginPurpose::ClosedNoChange(_) | OriginPurpose::ClosedReplan(_, _) => {
            AgentControllerState::Replan
        }
    };
    if run.state() != expected_state {
        return Err(E::StaleAnchor);
    }
    let mut last = applied.clone();
    let mut execute_at = None;
    for index in
        0..usize::try_from(delta.saturating_sub(1).min(3)).map_err(|_| E::InvalidStoredData)?
    {
        let event = event_at(
            tx,
            anchor.run,
            offset(
                application_tail,
                u64::try_from(index).map_err(|_| E::InvalidStoredData)? + 2,
            )?,
        )
        .await?;
        if event.id() != material.activation_events()[index]
            || !plain_event(&event, &anchor)
            || event.kind()
                != (RunEventKind::StateTransition {
                    from: states[index],
                    to: states[index + 1],
                })
            || event.payload() != &success_payload()
            || event.occurred_at() < last.occurred_at()
        {
            return Err(E::StaleAnchor);
        }
        if index == 2 {
            execute_at = Some(event.occurred_at());
        }
        last = event;
    }
    let expected_version = anchor
        .version
        .get()
        .checked_add(match purpose {
            OriginPurpose::Current(_) => {
                if delta >= 4 {
                    2
                } else {
                    1
                }
            }
            OriginPurpose::ClosedNoChange(_) | OriginPurpose::ClosedReplan(_, _) => 3,
        })
        .ok_or(E::InvalidStoredData)?;
    if stored.version().get() != expected_version {
        return Err(E::StaleAnchor);
    }
    if let Some(at) = execute_at {
        if change.attempts().len() != 1 {
            return Err(E::StaleAnchor);
        }
        let attempt = &change.attempts()[0];
        if attempt.number().get() != 1
            || attempt.run_id() != run.id()
            || attempt.started_at().unix_millis() != at.unix_millis()
        {
            return Err(E::StaleAnchor);
        }
        match purpose {
            OriginPurpose::Current(_) => {
                if change.status() != TaskStepStatus::InProgress
                    || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
                    || attempt.finished_at().is_some()
                    || ledger.updated_at().unix_millis() != at.unix_millis()
                {
                    return Err(E::StaleAnchor);
                }
            }
            OriginPurpose::ClosedNoChange(closed) | OriginPurpose::ClosedReplan(closed, _) => {
                if change.status() != TaskStepStatus::Failed
                    || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
                    || attempt.finished_at().map(|t| t.unix_millis())
                        != Some(closed.closure().occurred_at().unix_millis())
                    || ledger.updated_at().unix_millis()
                        != closed.closure().occurred_at().unix_millis()
                {
                    return Err(E::StaleAnchor);
                }
            }
        }
    } else if change.status() != TaskStepStatus::Ready
        || !change.attempts().is_empty()
        || ledger.updated_at().unix_millis() != applied.occurred_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    if delta == 5
        || matches!(
            purpose,
            OriginPurpose::ClosedNoChange(_) | OriginPurpose::ClosedReplan(_, _)
        )
    {
        let read = event_at(tx, anchor.run, offset(application_tail, 5)?).await?;
        if read.kind() != RunEventKind::ToolAction
            || read.run_id() != anchor.run
            || read.snapshot_id() != anchor.snapshot
            || read.occurred_at() < last.occurred_at()
            || read.payload().code() != RunEventCode::None
            || read.payload().outcome() != Some(RunEventOutcome::Succeeded)
            || read.turn_charge().is_some()
            || read
                .payload()
                .redaction()
                .is_none_or(|redaction| redaction.source() != RunEventRedactionSource::ToolOutput)
        {
            return Err(E::StaleAnchor);
        }
        let Some(RunEventSubject::Tool(tool)) = read.subject() else {
            return Err(E::StaleAnchor);
        };
        // The generic truncation flag also means that more file lines remain after an
        // interior page. Completeness is established below by replaying the real read
        // and requiring its exact original to cover the complete parsed definition.
        match purpose {
            OriginPurpose::Current(current) => {
                require_target_read(
                    tx,
                    project,
                    goal,
                    stored,
                    run,
                    current,
                    &anchor,
                    application_tail,
                    focus,
                    material.change().id(),
                    tool,
                )
                .await?
            }
            OriginPurpose::ClosedNoChange(closed) | OriginPurpose::ClosedReplan(closed, _) => {
                require_closed_target_read(
                    tx,
                    &anchor,
                    application_tail,
                    focus,
                    material.change().id(),
                    &read,
                    closed,
                )
                .await?
            }
        }
        last = read;
    } else {
        require_no_read(tx, run.id(), material.change().id()).await?;
    }
    if let OriginPurpose::ClosedNoChange(closed) | OriginPurpose::ClosedReplan(closed, _) = purpose
    {
        let closure = closed.closure();
        if last.sequence() != closed.fact().owner().sequence
            || closure.sequence() != offset(application_tail, 6)?
            || closure.occurred_at() < last.occurred_at()
            || closure.id() != closed.fact().closure_event_id()
        {
            return Err(E::StaleAnchor);
        }
        last = closure.clone();
    }
    if last.sequence() != run.last_event_sequence()
        || last.occurred_at() != run.updated_at()
        || now < last.occurred_at()
    {
        return Err(E::StaleAnchor);
    }
    let mut tail = tx
        .query(
            "SELECT MAX(event_sequence) FROM run_events WHERE run_id=?1",
            params![run.id().as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    if n(
        &tail
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )? != run.last_event_sequence().get()
    {
        return Err(E::StaleAnchor);
    }
    let mut source=tx.query("SELECT 1 FROM index_runs i JOIN file_revisions f USING(index_run_id)
        WHERE i.worktree_id=?1 AND i.index_run_id=?2 AND i.snapshot_id=?3 AND i.status='published'
        AND i.run_sequence=(SELECT MAX(run_sequence) FROM index_runs WHERE worktree_id=?1 AND status='published')
        AND f.repository_path=?4 AND f.content_hash=?5",params![worktree.as_bytes().to_vec(),anchor.index.as_bytes().to_vec(),anchor.snapshot.as_bytes().to_vec(),anchor.source.path().as_bytes().to_vec(),anchor.source.content_hash().as_bytes().to_vec()]).await.map_err(unavailable)?;
    if source.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    let diagnostic_result = match &context_read {
        Some(read) => {
            recovery::mismatch::load_for_applied_http_context(tx, worktree, read, &applied).await
        }
        None => {
            recovery::mismatch::load_for_applied_http_correction(tx, worktree, &anchor, &applied)
                .await
        }
    };
    let diagnostic = diagnostic_result
        .map_err(|error| match error.classify() {
            a3_application::AgentRecoveryStoreFailure::Unavailable => E::Unavailable,
            _ => E::StaleAnchor,
        })?
        .ok_or(E::StaleAnchor)?;
    if diagnostic.digest() != anchor.diagnostic || diagnostic.script() != &anchor.source {
        return Err(E::StaleAnchor);
    }
    // Descriptive provenance only. Application must re-read both originals and replay the producing job.
    Ok(HttpCorrectionOrigin {
        anchor,
        producer,
        diagnostic,
        applied_event: applied,
    })
}

fn success_payload() -> RunEventPayload {
    RunEventPayload::new(
        RunEventCode::ControllerDecision,
        Some(RunEventOutcome::Succeeded),
        None,
    )
}
fn plain_event(event: &RunEvent, a: &HttpCorrectionPlanAnchor) -> bool {
    event.run_id() == a.run
        && event.snapshot_id() == a.snapshot
        && event.subject().is_none()
        && event.turn_charge().is_none()
}
fn offset(sequence: RunEventSequence, offset: u64) -> Result<RunEventSequence, E> {
    RunEventSequence::new(
        sequence
            .get()
            .checked_add(offset)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)
}
async fn event_at(
    tx: &Transaction,
    run: AgentRunId,
    sequence: RunEventSequence,
) -> Result<RunEvent, E> {
    journal::read_event(tx, run, sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)
}

async fn require_no_read(tx: &Transaction, run: AgentRunId, step: TaskStepId) -> Result<(), E> {
    let mut rows=tx.query("SELECT (SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2)+(SELECT COUNT(*) FROM source_caller_relocalizations WHERE run_id=?1 AND step_id=?2)",params![run.as_bytes().to_vec(),step.as_bytes().to_vec()]).await.map_err(unavailable)?;
    if n(
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

#[allow(clippy::too_many_arguments)]
async fn require_target_read(
    tx: &Transaction,
    project: &ProjectIdentity,
    goal: &GoalContract,
    stored: &StoredTaskLedger,
    run: &AgentRun,
    current: &PublishedIndex,
    a: &HttpCorrectionPlanAnchor,
    application_tail: RunEventSequence,
    focus: AgentFileStartLine,
    change: TaskStepId,
    tool: ToolRunId,
) -> Result<(), E> {
    let edit_anchor = EditPhaseState::new(project, goal, stored, run, current)
        .map_err(|_| E::StaleAnchor)?
        .anchor();
    let state = crate::source_read_repository::load_recorded_state_in_transaction(tx, edit_anchor)
        .await
        .map_err(|_| E::StaleAnchor)?;
    let acquired = state.acquired().ok_or(E::StaleAnchor)?;
    let range = acquired.original().range();
    let definition =
        http_correction_definition_source(current, &a.source, focus).map_err(|_| E::StaleAnchor)?;
    if !state.progress().receipts().is_empty()
        || !state.caller_relocalizations().is_empty()
        || state
            .progress()
            .pending()
            .is_none_or(|pending| pending.ordinal().get() != 1)
        || acquired.original() != acquired.required()
        || acquired.original().revision() != &a.source
        || !a.source_range.contains(range)
        || !http_correction_definition_page_matches(&definition, acquired.original())
    {
        return Err(E::StaleAnchor);
    }
    let mut rows = tx
        .query(
            "SELECT 1 FROM source_read_acquisitions s
        WHERE s.run_id=?1 AND s.step_id=?2 AND s.attempt_number=1 AND s.ordinal=1
        AND s.source_read_id=?3 AND s.tool_run_id=?4 AND s.after_sequence=?5 AND s.abandoned=0
        AND (SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2)=1
        AND (SELECT COUNT(*) FROM source_caller_relocalizations WHERE run_id=?1 AND step_id=?2)=0",
            params![
                run.id().as_bytes().to_vec(),
                change.as_bytes().to_vec(),
                acquired.key().as_bytes().to_vec(),
                tool.as_bytes().to_vec(),
                i64::try_from(offset(application_tail, 4)?.get())
                    .map_err(|_| E::InvalidStoredData)?
            ],
        )
        .await
        .map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    Ok(())
}

pub(super) async fn require_closed_target_read(
    tx: &Transaction,
    a: &HttpCorrectionPlanAnchor,
    application_tail: RunEventSequence,
    focus: AgentFileStartLine,
    change: TaskStepId,
    read: &RunEvent,
    closed: &AuthenticatedClosedNoChange,
) -> Result<(), E> {
    let fact = closed.fact();
    let owner = fact.owner();
    let original = fact.window().original();
    let definition = recorded_definition(tx, a.index, &a.source, focus).await?;
    if owner.step != change
        || owner.attempt.get() != 1
        || owner.sequence != offset(application_tail, 5)?
        || read.subject() != Some(RunEventSubject::Tool(closed.tool()))
        || read.occurred_at() > fact.call().reserved_at()
        || original.revision() != &a.source
        || !a.source_range.contains(original.range())
        || !http_correction_definition_page_matches(&definition, original)
    {
        return Err(E::StaleAnchor);
    }
    // The private token already proved the single original read, its exact owner,
    // succeeded ToolAction, ordinal1, no caller reads and abandoned1 without credit.
    Ok(())
}

async fn check_current_clock_and_budget(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
    now: AgentRunTimestamp,
) -> Result<(), E> {
    if now >= edit::run_deadline(run)? {
        return Err(E::DeadlineReached);
    }
    let (usage, latest) = edit::read_run_usage(tx, worktree, run).await?;
    if now < run.updated_at() || latest.is_some_and(|at| now < at) {
        return Err(E::TimestampRegressed);
    }
    if usage.unknown_calls > 0 || usage.unobserved_calls > 0 {
        return Err(E::UncertainProviderCall);
    }
    for (dimension, used, reserved, limit) in [
        (
            AgentBudgetDimension::Turns,
            u64::from(run.usage().turn_count()),
            u64::from(usage.calls),
            u64::from(run.budget().turn_limit().get()),
        ),
        (
            AgentBudgetDimension::PromptTokens,
            run.usage().prompt_tokens(),
            usage.prompt_tokens,
            run.budget().prompt_token_limit().get(),
        ),
        (
            AgentBudgetDimension::OutputTokens,
            run.usage().output_tokens(),
            usage.output_tokens,
            run.budget().output_token_limit().get(),
        ),
        (
            AgentBudgetDimension::Repairs,
            u64::from(run.usage().repair_count()),
            u64::from(usage.repairs),
            u64::from(run.budget().repair_limit().get()),
        ),
    ] {
        if used.checked_add(reserved).ok_or(E::InvalidStoredData)? > limit {
            return Err(E::BudgetExhausted(dimension));
        }
    }
    Ok(())
}

async fn recorded_definition(
    tx: &Transaction,
    index: IndexRunId,
    revision: &FileRevision,
    focus: AgentFileStartLine,
) -> Result<EvidenceRef, E> {
    let row = focus.get().checked_sub(1).ok_or(E::InvalidStoredData)?;
    let mut rows = tx.query("SELECT declaration_start_byte,declaration_end_byte,declaration_start_row,
        declaration_start_column,declaration_end_row,declaration_end_column FROM symbols
        WHERE index_run_id=?1 AND repository_path=?2 AND content_hash=?3 AND kind IN ('function','method')
        AND declaration_start_row<=?4 AND declaration_end_row>=?4 ORDER BY symbol_id LIMIT 65",
        params![index.as_bytes().to_vec(),revision.path().as_bytes().to_vec(),revision.content_hash().as_bytes().to_vec(),i64::from(row)])
        .await.map_err(unavailable)?;
    let mut selected: Option<SourceRange> = None;
    let mut count = 0_u8;
    while let Some(record) = rows.next().await.map_err(unavailable)? {
        count += 1;
        if count > 64 {
            return Err(E::InvalidStoredData);
        }
        let mut value = [0_u32; 6];
        for (out, at) in value.iter_mut().zip(0..6) {
            *out = u32::try_from(n(&record, at)?).map_err(|_| E::InvalidStoredData)?;
        }
        let range = SourceRange::new(
            value[0] as usize,
            value[1] as usize,
            SourcePosition::new(value[2], value[3]),
            SourcePosition::new(value[4], value[5]),
        )
        .map_err(|_| E::InvalidStoredData)?;
        if range.is_empty()
            || (range.end_position().row() == row && range.end_position().column() == 0)
        {
            continue;
        }
        selected = Some(match selected {
            None => range,
            Some(old) if old != range && old.contains(range) => range,
            Some(old) if old != range && range.contains(old) => old,
            Some(_) => return Err(E::InvalidStoredData),
        });
    }
    let range = selected.ok_or(E::StaleAnchor)?;
    if range.len() > 8192 {
        return Err(E::InvalidStoredData);
    }
    Ok(EvidenceRef::new(revision.clone(), range))
}
