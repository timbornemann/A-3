//! One genuine S1 acquisition and one bounded ContextIntent job in the existing Run.
use crate::{
    LibsqlKnowledgeStore, agent_recovery_repository as recovery,
    edit_phase_reservation_repository as edit, goal_contract_repository as goals,
    http_correction_plan_repository as parent_store, run_journal_repository as journal,
    task_ledger_repository as ledgers,
};
use a3_application::{
    AcquiredSourceRead, BoundEditSource, EditPhaseCallKind, EditPhaseObservation,
    EditPhaseObservationOutcome as Observation, EditPhaseReservationFailure as E,
    EditPhaseReservationFuture, EditPhaseReservedUsage, EditPhaseWireCost, HttpContextIntentAnchor,
    HttpContextIntentReply, HttpContextIntentReservation, HttpContextIntentSettlement,
    HttpContextReadOrigin, HttpCorrectionPlanAnchor, HttpCorrectionPlanDecision,
    HttpCorrectionPlanProposal, PreparedHttpContextIntent, PreparedHttpCorrectionPlan,
    SourceDeliveryProgress, SourceDeliveryScope, SourceReadAdmission, SourceReadFailure,
    SourceReadFuture, StoredTaskLedger,
};
use a3_domain::*;
use libsql::{Connection, Row, Transaction, TransactionBehavior, Value, params};

#[path = "http_context_anchor_codec.rs"]
pub(crate) mod anchor_codec;

const VERSION: u32 = 66;

enum PublicationCheck<'a> {
    Supplied(&'a PublishedIndex),
    Stored,
}

/// Adapter-internal retained row data. No action or source permission is granted.
pub(crate) struct Parent {
    pub(crate) anchor: HttpCorrectionPlanAnchor,
    pub(crate) job: ContentHash,
    pub(crate) proposal: HttpCorrectionPlanProposal,
    pub(crate) kind: String,
    pub(crate) digest: ContentHash,
    pub(crate) accepted_at: AgentRunTimestamp,
    pub(crate) reserved_at: u64,
}

/// Constructible only after the immutable actual-read/acquisition checks below.
/// It stays inside the adapter and is consumed by the dedicated applied-Context receipt purpose.
pub(crate) struct AuthenticatedHttpContextRead {
    anchor: HttpContextIntentAnchor,
    event: RunEvent,
}
impl AuthenticatedHttpContextRead {
    pub(crate) fn anchor(&self) -> &HttpContextIntentAnchor {
        &self.anchor
    }
    pub(crate) fn event(&self) -> &RunEvent {
        &self.event
    }
}

pub(crate) struct AppliedContext {
    pub(crate) anchor: HttpContextIntentAnchor,
    pub(crate) job: ContentHash,
    pub(crate) reply: HttpContextIntentReply,
    pub(crate) accepted_wire: ContentHash,
    pub(crate) accepted_at: AgentRunTimestamp,
    pub(crate) read: AuthenticatedHttpContextRead,
}

pub(crate) fn admit<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpCorrectionPlan,
    source: &'a BoundEditSource,
    tool: ToolRunId,
    now: AgentRunTimestamp,
) -> SourceReadFuture<'a, SourceReadAdmission> {
    Box::pin(async move {
        let result = async {
            let conn = open(store, project).await?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(unavailable)?;
            let result = async {
                // Keep the ordinary V4 admission guard unchanged: it must still be exactly F.
                let run = parent_store::validate(&tx, project.worktree().id(), phase).await?;
                let goal = load_goal(&tx, project.worktree().id(), phase.anchor().goal).await?;
                let parent = parent(&tx, project.worktree().id(), &goal, &run, phase.request().profile()).await?;
                if &parent.anchor != phase.anchor() || parent.job != phase.job_digest() {
                    return Err(E::StaleAnchor);
                }
                settled_clock(&tx, project.worktree().id(), &run, now).await?;
                if let Some(exhausted)=run.budget_exhaustion(now).map_err(|_|E::InvalidStoredData)? {
                    return Err(E::BudgetExhausted(exhausted.dimension()));
                }
                let HttpCorrectionPlanDecision::NeedOfferedSource(offer) = parent.proposal.decision() else {
                    return Err(E::StaleAnchor);
                };
                let required = EvidenceRef::new(offer.revision().clone(), offer.range());
                if source.worktree_id() != project.worktree().id()
                    || source.index_run_id() != parent.anchor.index
                    || source.snapshot_id() != parent.anchor.snapshot
                    || source.required() != &required
                {
                    return Err(E::StaleAnchor);
                }
                if scalar(&tx,
                    "SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2 AND attempt_number=?3",
                    params![run.id().as_bytes().to_vec(), parent.anchor.step.as_bytes().to_vec(), i64::from(parent.anchor.failed_attempt.get())],
                ).await? != 0 {
                    return Err(E::AlreadyReserved);
                }
                // Preseeded Runs need this guard too; generic tool admission only has a linked
                // intake-scope limit when such a link exists. Interrupted attempts stay charged.
                if scalar(&tx, "SELECT COUNT(*) FROM tool_run_attempts WHERE run_id=?1",
                    params![run.id().as_bytes().to_vec()],
                ).await? >= u64::from(run.budget().action_limit().get()) {
                    return Err(E::BudgetExhausted(AgentBudgetDimension::Actions));
                }
                let attempt = recovery::begin_tool_attempt_in_transaction(
                    &tx, project.worktree().id(), run.id(), parent.anchor.snapshot, tool, now,
                ).await.map_err(|_| E::StaleAnchor)?;
                let key = read_key(&parent);
                tx.execute(
                    "INSERT INTO source_read_acquisitions(source_read_id,scope_digest,policy_version,
                     run_id,step_id,attempt_number,ordinal,tool_run_id,tool_attempt,after_sequence,
                     repository_path,content_hash,required_range)
                     VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8,?9,?10,?11,?12)",
                    params![key.as_bytes().to_vec(), read_scope(&parent).as_bytes().to_vec(),
                        i64::from(SourceDeliveryProgress::POLICY_VERSION), run.id().as_bytes().to_vec(),
                        parent.anchor.step.as_bytes().to_vec(), i64::from(parent.anchor.failed_attempt.get()),
                        tool.as_bytes().to_vec(), i64::from(attempt.attempt().get()),
                        number(parent.anchor.sequence.get())?, required.revision().path().as_bytes().to_vec(),
                        required.revision().content_hash().as_bytes().to_vec(), range_bytes(required.range())],
                ).await.map_err(unavailable)?;
                Ok(SourceReadAdmission::new(key, attempt))
            }.await;
            close(tx, result).await
        }.await;
        result.map_err(SourceReadFailure::Reservation)
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_origin<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    goal: &'a GoalContract,
    stored: &'a StoredTaskLedger,
    run: &'a AgentRun,
    current: &'a PublishedIndex,
    profile: &'a ModelProfile,
    now: AgentRunTimestamp,
) -> EditPhaseReservationFuture<'a, HttpContextReadOrigin> {
    Box::pin(async move {
        let conn = open(store, project).await?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result = async {
            let origin = read_origin_in_tx(
                &tx,
                project,
                goal,
                stored,
                run,
                PublicationCheck::Supplied(current),
                profile,
                now,
            )
            .await?;
            settled_clock(&tx, project.worktree().id(), run, now).await?;
            Ok(origin)
        }
        .await;
        close(tx, result).await
    })
}

#[allow(clippy::too_many_arguments)]
async fn read_origin_in_tx(
    tx: &Transaction,
    project: &ProjectIdentity,
    goal: &GoalContract,
    stored: &StoredTaskLedger,
    run: &AgentRun,
    publication: PublicationCheck<'_>,
    profile: &ModelProfile,
    now: AgentRunTimestamp,
) -> Result<HttpContextReadOrigin, E> {
    let worktree = project.worktree().id();
    let durable_run = journal::load_run_from_transaction(tx, worktree, run.id())
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
        || durable_ledger != *stored
        || load_goal(tx, worktree, goal.reference()).await? != *goal
        || run.goal_contract() != goal.reference()
        || stored.ledger().goal_contract() != goal.reference()
        || run.state() != AgentControllerState::Replan
        || stored.ledger().pending_planning().is_some()
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
    {
        return Err(E::StaleAnchor);
    }
    let parent = parent(tx, worktree, goal, run, profile).await?;
    let a = &parent.anchor;
    let read_tail = a
        .sequence
        .get()
        .checked_add(1)
        .ok_or(E::InvalidStoredData)?;
    if a.goal != goal.reference()
        || a.original != goal.original().ok_or(E::StaleAnchor)?.digest()
        || a.ledger != stored.ledger().revision()
        || a.version != stored.version()
        || a.ledger != run.task_ledger_revision()
        || a.snapshot != run.current_snapshot_id()
        || run.last_event_sequence().get() != read_tail
        || run.model_profile() != Some(a.profile)
        || goals::task_current_revision(tx, worktree, goal.task_id())
            .await
            .map_err(|error| match error.classify() {
                a3_application::GoalContractStoreFailure::Unavailable => E::Unavailable,
                _ => E::InvalidStoredData,
            })?
            != Some(a.goal.revision())
        || recovery::has_unreconciled_mutation(tx, worktree)
            .await
            .map_err(|_| E::Unavailable)?
    {
        return Err(E::StaleAnchor);
    }
    if let PublicationCheck::Supplied(current) = publication
        && (a.snapshot != current.run().snapshot_id()
            || a.index != current.run().id()
            || !current.publication().graph().files().contains(&a.source))
    {
        return Err(E::StaleAnchor);
    }
    current_file(tx, a).await?;
    let failure = event(tx, a.run, a.sequence).await?;
    let failed = stored.ledger().step(a.step).ok_or(E::StaleAnchor)?;
    let attempt = failed.attempts().last().ok_or(E::StaleAnchor)?;
    if !failed.is_active_plan_step()
        || failed.status() != TaskStepStatus::Failed
        || failed.definition().verification_spec().id() != a.verification
        || attempt.run_id() != a.run
        || attempt.number() != a.failed_attempt
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|t| t.unix_millis())
            != Some(failure.occurred_at().unix_millis())
        || failure.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || failure.snapshot_id() != a.snapshot
        || failure.subject().is_some()
        || failure.turn_charge().is_some()
        || failure.payload()
            != &RunEventPayload::new(
                RunEventCode::VerificationFailure,
                Some(RunEventOutcome::Failed),
                None,
            )
    {
        return Err(E::StaleAnchor);
    }
    // This original-step receipt remains strict/current: no retirement or historical completion bypass.
    let diagnostic =
        recovery::mismatch::load_from_transaction(tx, worktree, a.run, a.tool, a.tool_attempt)
            .await
            .map_err(|_| E::StaleAnchor)?
            .ok_or(E::StaleAnchor)?;
    if diagnostic.digest() != a.diagnostic
        || diagnostic.script() != &a.source
        || diagnostic.context().step_id != a.step
        || diagnostic.context().spec_id != a.verification
    {
        return Err(E::StaleAnchor);
    }
    let previous = a
        .sequence
        .get()
        .checked_sub(1)
        .and_then(|s| RunEventSequence::new(s).ok())
        .ok_or(E::StaleAnchor)?;
    let mismatch = event(tx, a.run, previous).await?;
    if mismatch.kind() != RunEventKind::ToolAction
        || mismatch.snapshot_id() != a.snapshot
        || mismatch.subject() != Some(RunEventSubject::Tool(a.tool))
        || mismatch.occurred_at() != failure.occurred_at()
        || mismatch.payload().code() != RunEventCode::VerificationFailure
        || mismatch.payload().outcome() != Some(RunEventOutcome::Failed)
    {
        return Err(E::StaleAnchor);
    }
    if scalar(tx,
        "SELECT COUNT(*) FROM loopback_mismatch_diagnostics WHERE tool_run_id=?1 AND attempt_sequence=?2 AND digest=?3 AND event_id=?4",
        params![a.tool.as_bytes().to_vec(),i64::from(a.tool_attempt.get()),a.diagnostic.as_bytes().to_vec(),mismatch.id().as_bytes().to_vec()],
    ).await?!=1 {return Err(E::StaleAnchor)}
    let read = event(tx, a.run, run.last_event_sequence()).await?;
    if read.kind() != RunEventKind::ToolAction
        || read.snapshot_id() != a.snapshot
        || read.occurred_at() != run.updated_at()
        || read.occurred_at() < parent.accepted_at
        || read.payload().code() != RunEventCode::None
        || read.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || read.turn_charge().is_some()
        || read
            .payload()
            .redaction()
            .is_none_or(|r| r.source() != RunEventRedactionSource::ToolOutput)
    {
        return Err(E::StaleAnchor);
    }
    if now < read.occurred_at() {
        return Err(E::TimestampRegressed);
    }
    if now >= edit::run_deadline(run)? {
        return Err(E::DeadlineReached);
    }
    let Some(RunEventSubject::Tool(tool)) = read.subject() else {
        return Err(E::StaleAnchor);
    };
    let (admission, acquired) = acquired(tx, &parent, &read, tool).await?;
    if scalar(
        tx,
        "SELECT MAX(event_sequence) FROM run_events WHERE run_id=?1",
        params![a.run.as_bytes().to_vec()],
    )
    .await?
        != read_tail
    {
        return Err(E::StaleAnchor);
    }
    Ok(HttpContextReadOrigin {
        parent_anchor: parent.anchor,
        parent_job: parent.job,
        parent_proposal: parent.proposal,
        diagnostic,
        read_admission: admission,
        acquired,
        read_event: read,
    })
}

async fn parent(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
    profile: &ModelProfile,
) -> Result<Parent, E> {
    let mut rows = parent_rows(tx, run).await?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::StaleAnchor)?;
    let parent = decode_parent_row(tx, worktree, goal, run, profile, &row).await?;
    if !matches!(
        parent.proposal.decision(),
        HttpCorrectionPlanDecision::NeedOfferedSource(_)
    ) || rows.next().await.map_err(unavailable)?.is_some()
    {
        return Err(E::InvalidStoredData);
    }
    Ok(parent)
}

/// Exact latest accepted parent, never fallback to an older valid row.
/// Current S1 admission retains its independent exactly-one/S1 wrapper above.
pub(crate) async fn selected_parent(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
    profile: &ModelProfile,
) -> Result<Parent, E> {
    let mut rows = parent_rows(tx, run).await?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::StaleAnchor)?;
    // Preserve the existing applied-origin alias rejection instead of falling back.
    let selected_anchor = parent_store::decode_retained_anchor(&blob(&row, 1)?, goal)?;
    if hash(&row, 9)? != selected_anchor.diagnostic {
        return Err(E::StaleAnchor);
    }
    decode_parent_row(tx, worktree, goal, run, profile, &row).await
}

async fn parent_rows(tx: &Transaction, run: &AgentRun) -> Result<libsql::Rows, E> {
    tx.query(
        "SELECT job_digest,CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 526 AND 133000 THEN anchor ELSE x'' END,
         CASE WHEN typeof(proposal)='blob' AND length(proposal) BETWEEN 1 AND 512 THEN proposal ELSE x'' END,
         proposal_digest,call_kind,wire_digest,reserved_at,finished_at,protocol_version,diagnostic_digest,
         worktree_id,tool_run_id,tool_attempt,wire_bytes,prompt_reserve,output_reserve,
         reported_prompt,reported_output,deadline FROM http_correction_plan_reservations
         WHERE run_id=?1 AND outcome='accepted_reply' ORDER BY finished_at DESC,CASE call_kind WHEN 'structural_repair' THEN 0 ELSE 1 END LIMIT 2",
        params![run.id().as_bytes().to_vec()],
    ).await.map_err(unavailable)
}

async fn decode_parent_row(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
    profile: &ModelProfile,
    row: &Row,
) -> Result<Parent, E> {
    if !profile.executable_actions_enabled() || run.model_profile() != Some(profile.reference()) {
        return Err(E::StaleAnchor);
    }
    let anchor = parent_store::decode_retained_anchor(&blob(row, 1)?, goal)?;
    let job = hash(row, 0)?;
    let bytes = blob(row, 2)?;
    let proposal = HttpCorrectionPlanProposal::decode_retained(&anchor, job, &bytes)
        .map_err(|_| E::InvalidStoredData)?;
    let digest = ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes());
    let kind: String = row.get(4).map_err(|_| E::InvalidStoredData)?;
    let wire = blob(row, 5)?;
    let reserved = n(row, 6)?;
    let accepted_at = timestamp(n(row, 7)?)?;
    if n(row, 8)? != 4
        || hash(row, 3)? != digest
        || hash(row, 9)? != anchor.diagnostic
        || blob(row, 10)? != worktree.as_bytes()
        || blob(row, 11)? != anchor.tool.as_bytes()
        || n(row, 12)? != u64::from(anchor.tool_attempt.get())
        || anchor.worktree != worktree
        || anchor.profile != profile.reference()
        || anchor.run != run.id()
        || anchor.goal != goal.reference()
        || wire.len() != 32
        || n(row, 13)? == 0
        || n(row, 13)? > 16_777_216
        || n(row, 14)? == 0
        || n(row, 14)? > u64::from(u32::MAX)
        || n(row, 15)? == 0
        || n(row, 15)? > u64::from(u32::MAX)
        || n(row, 16)? == 0
        || n(row, 17)? == 0
        || n(row, 16)? > n(row, 14)?
        || n(row, 17)? > n(row, 15)?
        || n(row, 17)? > u64::from(profile.settings().output_limit().get())
        || accepted_at.unix_millis() < reserved
        || accepted_at.unix_millis() >= n(row, 18)?
        || n(row, 18)? != edit::run_deadline(run)?.unix_millis()
    {
        return Err(E::InvalidStoredData);
    }
    let failure = event(tx, anchor.run, anchor.sequence).await?;
    parent_store::require_context_cold_lineage(
        tx,
        &anchor,
        job,
        &kind,
        &wire,
        reserved,
        failure.occurred_at(),
        run,
        profile,
    )
    .await?;
    Ok(Parent {
        anchor,
        job,
        proposal,
        kind,
        digest,
        accepted_at,
        reserved_at: reserved,
    })
}

async fn acquired(
    tx: &Transaction,
    parent: &Parent,
    read: &RunEvent,
    tool: ToolRunId,
) -> Result<(SourceReadAdmission, AcquiredSourceRead), E> {
    let a = &parent.anchor;
    let HttpCorrectionPlanDecision::NeedOfferedSource(offer) = parent.proposal.decision() else {
        return Err(E::StaleAnchor);
    };
    if scalar(tx,"SELECT COUNT(*) FROM source_read_acquisitions WHERE run_id=?1 AND step_id=?2 AND attempt_number=?3",
        params![a.run.as_bytes().to_vec(),a.step.as_bytes().to_vec(),i64::from(a.failed_attempt.get())]).await? != 1 {
        return Err(E::StaleAnchor)
    }
    let mut rows=tx.query(
        "SELECT s.source_read_id,s.scope_digest,s.policy_version,s.ordinal,s.tool_attempt,s.after_sequence,
         s.repository_path,s.content_hash,s.required_range,s.original_evidence_id,s.abandoned,
         a.status,a.run_id,a.snapshot_id,a.started_at_unix_millis,a.updated_at_unix_millis,
         t.run_id,t.event_sequence,t.status,t.snapshot_before_id,t.snapshot_after_id,
         e.location_kind,e.repository_path,e.content_hash,e.start_byte,e.end_byte,e.start_row,e.start_column,e.end_row,e.end_column
         FROM source_read_acquisitions s
         JOIN tool_run_attempts a ON a.tool_run_id=s.tool_run_id AND a.attempt_sequence=s.tool_attempt
         JOIN tool_runs t ON t.tool_run_id=s.tool_run_id
         JOIN tool_evidence e ON e.tool_run_id=s.tool_run_id AND e.evidence_id=s.original_evidence_id
         WHERE s.run_id=?1 AND s.step_id=?2 AND s.attempt_number=?3 AND s.tool_run_id=?4",
        params![a.run.as_bytes().to_vec(),a.step.as_bytes().to_vec(),i64::from(a.failed_attempt.get()),tool.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::StaleAnchor)?;
    let key = read_key(parent);
    let required = EvidenceRef::new(a.source.clone(), decode_range(&blob(&row, 8)?)?);
    let expected = EvidenceRef::new(offer.revision().clone(), offer.range());
    let mut raw = [0_u32; 6];
    for (field, index) in raw.iter_mut().zip(24..30) {
        *field = u32::try_from(n(&row, index)?).map_err(|_| E::InvalidStoredData)?;
    }
    let original = EvidenceRef::new(revision(&row, 22)?, make_range(raw)?);
    let number =
        AgentToolAttemptNumber::new(u32::try_from(n(&row, 4)?).map_err(|_| E::InvalidStoredData)?)
            .map_err(|_| E::InvalidStoredData)?;
    let started = timestamp(n(&row, 14)?)?;
    let updated = timestamp(n(&row, 15)?)?;
    if hash(&row, 0)? != key
        || hash(&row, 1)? != read_scope(parent)
        || n(&row, 2)? != u64::from(SourceDeliveryProgress::POLICY_VERSION)
        || n(&row, 3)? != 1
        || n(&row, 5)? != a.sequence.get()
        || required != expected
        || revision(&row, 6)? != a.source
        || n(&row, 10)? != 0
        || text(&row, 11)? != "succeeded"
        || text(&row, 18)? != "succeeded"
        || blob(&row, 12)? != a.run.as_bytes()
        || blob(&row, 13)? != a.snapshot.as_bytes()
        || blob(&row, 16)? != a.run.as_bytes()
        || n(&row, 17)? != read.sequence().get()
        || blob(&row, 19)? != a.snapshot.as_bytes()
        || blob(&row, 20)? != a.snapshot.as_bytes()
        || text(&row, 21)? != "span"
        || started < parent.accepted_at
        || updated != read.occurred_at()
        || original.revision() != &a.source
        || !original.range().contains(required.range())
        || hash(&row, 9)?.as_bytes()
            != AgentToolEvidence::for_span(original.clone())
                .id()
                .as_bytes()
        || rows.next().await.map_err(unavailable)?.is_some()
    {
        return Err(E::StaleAnchor);
    }
    if scalar(
        tx,
        "SELECT COUNT(*) FROM mutation_attempts WHERE tool_run_id=?1 AND attempt_sequence=?2",
        params![tool.as_bytes().to_vec(), i64::from(number.get())],
    )
    .await?
        != 0
    {
        return Err(E::StaleAnchor);
    }
    let attempt = AgentToolAttempt::new(
        tool,
        number,
        a.run,
        a.snapshot,
        AgentToolAttemptStatus::Succeeded,
        started,
        updated,
    )
    .map_err(|_| E::InvalidStoredData)?;
    let acquired = AcquiredSourceRead::from_recorded(
        key,
        SourceDeliveryScope::from_http_context(a, parent.job),
        required,
        original,
    )
    .map_err(|_| E::InvalidStoredData)?;
    Ok((SourceReadAdmission::new(key, attempt), acquired))
}

pub(crate) fn reserve<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpContextIntent,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    now: AgentRunTimestamp,
) -> EditPhaseReservationFuture<'a, HttpContextIntentReservation> {
    Box::pin(async move {
        let conn = open(store, project).await?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(unavailable)?;
        let result=async {
            let (run,parent)=validate_context(&tx,project,phase,now).await?;
            settled_clock(&tx,project.worktree().id(),&run,now).await?;
            check_kind(&tx,phase,kind,wire).await?;
            let (usage,_)=edit::read_run_usage(&tx,project.worktree().id(),&run).await?;
            edit::check_budget(run.budget(),run.usage(),usage,kind,wire)?;
            let a=phase.anchor();
            let deadline=edit::run_deadline(&run)?;
            tx.execute(
                "INSERT INTO http_context_intent_reservations(worktree_id,run_id,diagnostic_digest,call_kind,
                 parent_call_kind,parent_job_digest,parent_proposal_digest,source_read_id,job_digest,protocol_version,
                 anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,?11,?12,?13,?14,?15,?16)",
                params![a.parent.worktree.as_bytes().to_vec(),a.parent.run.as_bytes().to_vec(),a.parent.diagnostic.as_bytes().to_vec(),
                    edit::call_kind_text(kind),parent.kind,parent.job.as_bytes().to_vec(),parent.digest.as_bytes().to_vec(),
                    a.read_key.as_bytes().to_vec(),phase.job_digest().as_bytes().to_vec(),anchor_bytes(a)?,wire.digest().as_bytes().to_vec(),
                    i64::from(wire.request_bytes()),i64::from(wire.prompt_estimate()),i64::from(wire.output_reserve()),number(now.unix_millis())?,number(deadline.unix_millis())?],
            ).await.map_err(unavailable)?;
            Ok(HttpContextIntentReservation{job:phase.job_digest(),kind,wire,reserved_at:now,deadline})
        }.await;
        close(tx, result).await
    })
}

pub(crate) fn finish<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpContextIntent,
    permit: HttpContextIntentReservation,
    observation: EditPhaseObservation,
    reply: Option<HttpContextIntentReply>,
) -> EditPhaseReservationFuture<'a, Option<HttpContextIntentReply>> {
    Box::pin(async move {
        let conn = open(store, project).await?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(unavailable)?;
        let result=async {
            if permit.job!=phase.job_digest() || observation.finished_at()<permit.reserved_at {
                return Err(E::ObservationConflict);
            }
            let (mut outcome,mut rejection)=match observation.outcome() {
                Observation::AcceptedReply=>("accepted_reply",None),
                Observation::NoChange | Observation::DeadlineReached=>return Err(E::InvalidStoredData),
                Observation::InvalidStructure=>("invalid_structure",None),
                Observation::RejectedReply=>("rejected_reply",None),
                Observation::ProviderFailure=>("provider_failure",None),
                Observation::Cancelled=>("cancelled",None),
                Observation::Unknown=>("unknown",None),
            };
            match validate_context(&tx,project,phase,observation.finished_at()).await {
                Err(error @ (E::StaleAnchor|E::MutationRecoveryRequired|E::DeadlineReached|E::TimestampRegressed))=>{
                    if outcome!="unknown" {outcome="stale";}
                    rejection=Some(error);
                }
                Err(error)=>return Err(error),
                Ok(_)=>{}
            }
            if observation.finished_at()>=permit.deadline {
                if outcome!="unknown" {outcome="stale";}
                rejection=Some(E::DeadlineReached);
            }
            let prompt=observation.reported_prompt_tokens();
            let output=observation.reported_output_tokens();
            if prompt.is_some_and(|v|v>permit.wire.prompt_estimate()) {
                outcome="budget_overrun"; rejection=Some(E::BudgetExhausted(AgentBudgetDimension::PromptTokens));
            } else if output.is_some_and(|v|v>permit.wire.output_reserve()) {
                outcome="budget_overrun"; rejection=Some(E::BudgetExhausted(AgentBudgetDimension::OutputTokens));
            } else if matches!(outcome,"accepted_reply"|"invalid_structure")
                && output.is_some_and(|v|v>phase.request().profile().settings().output_limit().get()) {
                // Headroom is a charge, not permission to exceed actual generation settings.
                // Preserve an executor's existing Unknown disposition and its Usage failure.
                outcome="unknown";rejection=Some(E::BudgetExhausted(AgentBudgetDimension::OutputTokens));
            } else if matches!(outcome,"accepted_reply"|"invalid_structure")
                && (prompt.is_none_or(|v|v==0)||output.is_none_or(|v|v==0)) {
                outcome="unknown";rejection=Some(E::IncompleteProviderUsage);
            }
            let encoded=if outcome=="accepted_reply" {
                Some(phase.encode_reply(reply.as_ref().ok_or(E::InvalidStoredData)?).map_err(|_|E::InvalidStoredData)?)
            } else {None};
            let digest=encoded.as_ref().map(|v|blake3::hash(v).as_bytes().to_vec());
            let a=phase.anchor();
            let changed=tx.execute(
                "UPDATE http_context_intent_reservations SET outcome=?1,finished_at=?2,
                 reported_prompt=?3,reported_output=?4,reply=?5,reply_digest=?6
                 WHERE run_id=?7 AND diagnostic_digest=?8 AND call_kind=?9 AND job_digest=?10
                 AND anchor=?11 AND wire_digest=?12 AND wire_bytes=?13 AND prompt_reserve=?14
                 AND output_reserve=?15 AND reserved_at=?16 AND deadline=?17 AND outcome IS NULL
                 AND parent_job_digest=?18 AND parent_proposal_digest=?19 AND source_read_id=?20 AND protocol_version=1
                 AND parent_call_kind=(SELECT call_kind FROM http_correction_plan_reservations
                    WHERE run_id=?7 AND diagnostic_digest=?8 AND outcome='accepted_reply' AND job_digest=?18 AND proposal_digest=?19)",
                params![outcome,number(observation.finished_at().unix_millis())?,prompt.map(i64::from),output.map(i64::from),encoded,digest,
                    a.parent.run.as_bytes().to_vec(),a.parent.diagnostic.as_bytes().to_vec(),edit::call_kind_text(permit.kind),permit.job.as_bytes().to_vec(),
                    anchor_bytes(a)?,permit.wire.digest().as_bytes().to_vec(),i64::from(permit.wire.request_bytes()),i64::from(permit.wire.prompt_estimate()),
                    i64::from(permit.wire.output_reserve()),number(permit.reserved_at.unix_millis())?,number(permit.deadline.unix_millis())?,
                    a.parent_job.as_bytes().to_vec(),a.parent_proposal_digest.as_bytes().to_vec(),a.read_key.as_bytes().to_vec()],
            ).await.map_err(unavailable)?;
            if changed!=1 {return Err(E::ObservationConflict)}
            Ok((if outcome=="accepted_reply" {reply}else{None},rejection))
        }.await;
        // A stale/cancelled provider observation remains durably charged; return the
        // typed rejection only after its terminal observation has committed.
        match result {
            Ok((reply, rejection)) => {
                tx.commit().await.map_err(unavailable)?;
                match rejection {
                    Some(error) => Err(error),
                    None => Ok(reply),
                }
            }
            Err(error) => {
                tx.rollback().await.map_err(unavailable)?;
                Err(error)
            }
        }
    })
}

pub(crate) fn load<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpContextIntent,
    now: AgentRunTimestamp,
) -> EditPhaseReservationFuture<'a, Option<HttpContextIntentSettlement>> {
    Box::pin(async move {
        let conn = open(store, project).await?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result=async {
            let (run,_)=validate_context(&tx,project,phase,now).await?;
            settled_clock(&tx,project.worktree().id(),&run,now).await?;
            let a=phase.anchor();
            let mut rows=tx.query(
                "SELECT CASE WHEN typeof(reply)='blob' AND length(reply) BETWEEN 1 AND 1024 THEN reply ELSE x'' END,
                 reply_digest,call_kind,wire_digest,reserved_at FROM http_context_intent_reservations
                 WHERE run_id=?1 AND diagnostic_digest=?2 AND outcome='accepted_reply' LIMIT 2",
                params![a.parent.run.as_bytes().to_vec(),a.parent.diagnostic.as_bytes().to_vec()],
            ).await.map_err(unavailable)?;
            let Some(row)=rows.next().await.map_err(unavailable)? else{return Ok(None)};
            let bytes=blob(&row,0)?;
            let kind=text(&row,2)?;
            let wire=blob(&row,3)?;
            let reserved=n(&row,4)?;
            if hash(&row,1)?.as_bytes()!=blake3::hash(&bytes).as_bytes() || rows.next().await.map_err(unavailable)?.is_some() {
                return Err(E::InvalidStoredData);
            }
            cold_lineage(&tx,phase,&kind,&wire,reserved,&run).await?;
            let reply=HttpContextIntentReply::decode_retained(phase.job_digest(),&bytes).map_err(|_|E::InvalidStoredData)?;
            if phase.encode_reply(&reply).map_err(|_|E::InvalidStoredData)?!=bytes {return Err(E::InvalidStoredData)}
            let wire=ContentHash::from_bytes(wire.try_into().map_err(|_|E::InvalidStoredData)?);
            let settlement=phase.settlement(reply,wire).map_err(|_|E::InvalidStoredData)?;
            Ok(Some(settlement))
        }.await;
        close(tx, result).await
    })
}

async fn validate_context(
    tx: &Transaction,
    project: &ProjectIdentity,
    phase: &PreparedHttpContextIntent,
    now: AgentRunTimestamp,
) -> Result<(AgentRun, Parent), E> {
    let a = phase.anchor();
    if a.parent.worktree != project.worktree().id() {
        return Err(E::StaleAnchor);
    }
    let run = journal::load_run_from_transaction(tx, project.worktree().id(), a.parent.run)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let goal = load_goal(tx, project.worktree().id(), a.parent.goal).await?;
    let stored = ledgers::load_from_transaction(tx, project.worktree().id(), goal.task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    // The prepared private constructor already bound the current parsed graph. SQL still
    // verifies the latest publication/file, exact durable owner and complete real read.
    let origin = read_origin_in_tx(
        tx,
        project,
        &goal,
        &stored,
        &run,
        PublicationCheck::Stored,
        phase.request().profile(),
        now,
    )
    .await?;
    if origin.parent_anchor != a.parent
        || origin.parent_job != a.parent_job
        || ContentHash::from_bytes(
            *blake3::hash(
                &origin
                    .parent_proposal
                    .canonical_retained_bytes()
                    .map_err(|_| E::InvalidStoredData)?,
            )
            .as_bytes(),
        ) != a.parent_proposal_digest
        || origin.read_admission.key() != a.read_key
        || origin.read_admission.attempt().tool_run_id() != a.read_tool
        || origin.read_admission.attempt().attempt() != a.read_attempt
        || origin.read_admission.attempt().started_at() != a.read_started_at
        || origin.read_event.id() != a.read_event
        || origin.read_event.sequence() != a.read_tail
        || origin.read_event.occurred_at() != a.read_at
        || origin.acquired.original().range() != a.context_range
        || origin.acquired.original().revision() != &a.parent.source
        || a.primary_range != a.parent.source_range
        || a.primary_text != a.parent.source_text
        || !a.primary_range.contains(a.definition_range)
        || a.definition_range.start_position().row().checked_add(1) != Some(a.focus.get())
        || a.primary_range.is_empty()
        || a.context_range.is_empty()
        || (a.context_range.start_byte() < a.primary_range.end_byte()
            && a.primary_range.start_byte() < a.context_range.end_byte())
        || a.primary_range.len() > 8192
        || a.context_range.len() > 8192
    {
        return Err(E::StaleAnchor);
    }
    let parent = parent(
        tx,
        project.worktree().id(),
        &goal,
        &run,
        phase.request().profile(),
    )
    .await?;
    Ok((run, parent))
}

async fn check_kind(
    tx: &Transaction,
    phase: &PreparedHttpContextIntent,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
) -> Result<(), E> {
    let a = phase.anchor();
    let mut rows=tx.query(
        "SELECT call_kind,job_digest,
         CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 256 AND 137096 THEN anchor ELSE x'' END,
         parent_job_digest,parent_proposal_digest,source_read_id,
         wire_digest,outcome,protocol_version FROM http_context_intent_reservations
         WHERE run_id=?1 AND diagnostic_digest=?2 ORDER BY CASE call_kind WHEN 'initial' THEN 0 ELSE 1 END LIMIT 3",
        params![a.parent.run.as_bytes().to_vec(),a.parent.diagnostic.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let mut initial = None;
    let mut count = 0_u8;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        count += 1;
        if count > 2
            || hash(&row, 1)? != phase.job_digest()
            || blob(&row, 2)? != anchor_bytes(a)?
            || hash(&row, 3)? != a.parent_job
            || hash(&row, 4)? != a.parent_proposal_digest
            || hash(&row, 5)? != a.read_key
            || n(&row, 8)? != 1
        {
            return Err(E::InvalidStoredData);
        }
        let call = text(&row, 0)?;
        if kind == EditPhaseCallKind::Initial || call == "structural_repair" {
            return Err(E::AlreadyReserved);
        }
        if call != "initial" || initial.is_some() {
            return Err(E::InvalidStoredData);
        }
        if hash(&row, 6)? == wire.digest() {
            return Err(E::UnchangedRepairWire);
        }
        initial = Some(optional_text(&row, 7)?.as_deref() == Some("invalid_structure"));
    }
    if kind == EditPhaseCallKind::StructuralRepair && initial != Some(true) {
        return Err(E::RepairNotAvailable);
    }
    Ok(())
}

async fn cold_lineage(
    tx: &Transaction,
    phase: &PreparedHttpContextIntent,
    accepted_kind: &str,
    accepted_wire: &[u8],
    accepted_reserved: u64,
    run: &AgentRun,
) -> Result<(), E> {
    let a = phase.anchor();
    let goal = load_goal(tx, a.parent.worktree, a.parent.goal).await?;
    let parent = parent(tx, a.parent.worktree, &goal, run, phase.request().profile()).await?;
    require_context_history(
        tx,
        a,
        phase.job_digest(),
        phase.request().profile(),
        &parent,
        accepted_kind,
        accepted_wire,
        accepted_reserved,
        run,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn require_context_history(
    tx: &Transaction,
    a: &HttpContextIntentAnchor,
    job: ContentHash,
    profile: &ModelProfile,
    parent: &Parent,
    accepted_kind: &str,
    accepted_wire: &[u8],
    accepted_reserved: u64,
    run: &AgentRun,
) -> Result<(), E> {
    if a.parent != parent.anchor
        || a.parent_job != parent.job
        || a.parent_proposal_digest != parent.digest
        || profile.reference() != a.parent.profile
        || run.model_profile() != Some(profile.reference())
        || !profile.executable_actions_enabled()
    {
        return Err(E::StaleAnchor);
    }
    let mut rows=tx.query(
        "SELECT call_kind,job_digest,
         CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 256 AND 137096 THEN anchor ELSE x'' END,
         parent_call_kind,parent_job_digest,parent_proposal_digest,source_read_id,wire_digest,
         wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,outcome,finished_at,
         reported_prompt,reported_output,protocol_version,worktree_id,
         CASE WHEN reply IS NULL THEN 0 ELSE 1 END,CASE WHEN reply_digest IS NULL THEN 0 ELSE 1 END
         FROM http_context_intent_reservations WHERE run_id=?1 AND diagnostic_digest=?2
         ORDER BY CASE call_kind WHEN 'initial' THEN 0 ELSE 1 END LIMIT 3",
        params![a.parent.run.as_bytes().to_vec(),a.parent.diagnostic.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let mut count = 0_u8;
    let mut initial_finished = 0;
    let mut initial_wire = Vec::new();
    let mut initial_invalid = false;
    let mut accepted = false;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        count += 1;
        let kind = text(&row, 0)?;
        let outcome = text(&row, 13)?;
        let reserved = n(&row, 11)?;
        let finished = n(&row, 14)?;
        let wire = blob(&row, 7)?;
        if count > 2
            || hash(&row, 1)? != job
            || blob(&row, 2)? != anchor_bytes(a)?
            || text(&row, 3)? != parent.kind
            || hash(&row, 4)? != parent.job
            || hash(&row, 5)? != parent.digest
            || hash(&row, 6)? != a.read_key
            || n(&row, 17)? != 1
            || blob(&row, 18)? != a.parent.worktree.as_bytes()
            || reserved < a.read_at.unix_millis()
            || finished < reserved
            || finished >= n(&row, 12)?
            || n(&row, 12)? != edit::run_deadline(run)?.unix_millis()
            || wire.len() != 32
            || n(&row, 8)? == 0
            || n(&row, 8)? > 16_777_216
            || n(&row, 9)? == 0
            || n(&row, 9)? > u64::from(u32::MAX)
            || n(&row, 10)? == 0
            || n(&row, 10)? > u64::from(u32::MAX)
            || n(&row, 15)? == 0
            || n(&row, 16)? == 0
            || n(&row, 15)? > n(&row, 9)?
            || n(&row, 16)? > n(&row, 10)?
            || n(&row, 16)? > u64::from(profile.settings().output_limit().get())
        {
            return Err(E::InvalidStoredData);
        }
        if count == 1 {
            if kind != "initial" {
                return Err(E::InvalidStoredData);
            }
            initial_finished = finished;
            initial_wire = wire.clone();
            initial_invalid = outcome == "invalid_structure";
        } else if kind != "structural_repair"
            || !initial_invalid
            || reserved < initial_finished
            || wire == initial_wire
        {
            return Err(E::InvalidStoredData);
        }
        let is_accepted = outcome == "accepted_reply";
        if is_accepted {
            if accepted
                || kind != accepted_kind
                || wire != accepted_wire
                || reserved != accepted_reserved
                || n(&row, 19)? != 1
                || n(&row, 20)? != 1
            {
                return Err(E::InvalidStoredData);
            }
            accepted = true;
        } else if outcome != "invalid_structure"
            || count != 1
            || n(&row, 19)? != 0
            || n(&row, 20)? != 0
        {
            return Err(E::InvalidStoredData);
        }
    }
    if !accepted
        || (accepted_kind == "initial" && count != 1)
        || (accepted_kind == "structural_repair" && count != 2)
    {
        return Err(E::InvalidStoredData);
    }
    Ok(())
}

pub(crate) async fn usage(
    tx: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
) -> Result<(EditPhaseReservedUsage, Option<AgentRunTimestamp>), E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < VERSION
    {
        return Ok((EditPhaseReservedUsage::default(), None));
    }
    let mut rows=tx.query(
        "SELECT COUNT(*),COUNT(CASE WHEN call_kind='structural_repair' THEN 1 END),
         COALESCE(SUM(MAX(prompt_reserve,COALESCE(reported_prompt,0))),0),
         COALESCE(SUM(MAX(output_reserve,COALESCE(reported_output,0))),0),
         COUNT(CASE WHEN outcome IS NULL THEN 1 END),COUNT(CASE WHEN outcome IN ('unknown','budget_overrun') THEN 1 END),
         MAX(COALESCE(finished_at,reserved_at)),COUNT(CASE WHEN protocol_version IS NOT 1 THEN 1 END)
         FROM http_context_intent_reservations WHERE worktree_id=?1 AND run_id=?2",
        params![worktree.as_bytes().to_vec(),run.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::InvalidStoredData)?;
    if n(&row, 7)? != 0 {
        return Err(E::InvalidStoredData);
    }
    let latest = match row.get_value(6).map_err(unavailable)? {
        Value::Null => None,
        Value::Integer(value) => Some(timestamp(
            u64::try_from(value).map_err(|_| E::InvalidStoredData)?,
        )?),
        _ => return Err(E::InvalidStoredData),
    };
    Ok((
        EditPhaseReservedUsage {
            calls: u32::try_from(n(&row, 0)?).map_err(|_| E::InvalidStoredData)?,
            repairs: u32::try_from(n(&row, 1)?).map_err(|_| E::InvalidStoredData)?,
            prompt_tokens: n(&row, 2)?,
            output_tokens: n(&row, 3)?,
            unobserved_calls: u32::try_from(n(&row, 4)?).map_err(|_| E::InvalidStoredData)?,
            unknown_calls: u32::try_from(n(&row, 5)?).map_err(|_| E::InvalidStoredData)?,
        },
        latest,
    ))
}

async fn settled_clock(
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
    if usage.unknown_calls != 0 || usage.unobserved_calls != 0 {
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
        if used.checked_add(reserved).is_none_or(|total| total > limit) {
            return Err(E::BudgetExhausted(dimension));
        }
    }
    Ok(())
}

fn anchor_bytes(a: &HttpContextIntentAnchor) -> Result<Vec<u8>, E> {
    anchor_codec::encode(a)
}

fn read_scope(parent: &Parent) -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3.http-context-read-scope.v1");
    hash.update(&parent_store::anchor_bytes(&parent.anchor));
    hash.update(parent.job.as_bytes());
    hash.update(parent.digest.as_bytes());
    hash.update(parent.kind.as_bytes());
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}
fn read_key(parent: &Parent) -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3.http-context-read-key.v1");
    hash.update(read_scope(parent).as_bytes());
    hash.update(&[1]);
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}
fn range_bytes(range: SourceRange) -> Vec<u8> {
    [
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column(),
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}
fn decode_range(bytes: &[u8]) -> Result<SourceRange, E> {
    if bytes.len() != 24 {
        return Err(E::InvalidStoredData);
    }
    let mut values = [0_u32; 6];
    for (out, part) in values.iter_mut().zip(bytes.chunks_exact(4)) {
        *out = u32::from_le_bytes(part.try_into().map_err(|_| E::InvalidStoredData)?);
    }
    make_range(values)
}
fn make_range([start, end, row, column, end_row, end_column]: [u32; 6]) -> Result<SourceRange, E> {
    SourceRange::new(
        start as usize,
        end as usize,
        SourcePosition::new(row, column),
        SourcePosition::new(end_row, end_column),
    )
    .map_err(|_| E::InvalidStoredData)
}
fn revision(row: &Row, index: i32) -> Result<FileRevision, E> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(blob(row, index)?).map_err(|_| E::InvalidStoredData)?,
        hash(row, index + 1)?,
    ))
}
fn hash(row: &Row, index: i32) -> Result<ContentHash, E> {
    Ok(ContentHash::from_bytes(
        blob(row, index)?
            .try_into()
            .map_err(|_| E::InvalidStoredData)?,
    ))
}
fn blob(row: &Row, index: i32) -> Result<Vec<u8>, E> {
    row.get(index).map_err(|_| E::InvalidStoredData)
}
fn n(row: &Row, index: i32) -> Result<u64, E> {
    u64::try_from(row.get::<i64>(index).map_err(|_| E::InvalidStoredData)?)
        .map_err(|_| E::InvalidStoredData)
}
fn text(row: &Row, index: i32) -> Result<String, E> {
    row.get(index).map_err(|_| E::InvalidStoredData)
}
fn optional_text(row: &Row, index: i32) -> Result<Option<String>, E> {
    match row.get_value(index).map_err(unavailable)? {
        Value::Text(v) => Ok(Some(v)),
        Value::Null => Ok(None),
        _ => Err(E::InvalidStoredData),
    }
}
fn number(value: u64) -> Result<i64, E> {
    i64::try_from(value).map_err(|_| E::InvalidStoredData)
}
fn timestamp(value: u64) -> Result<AgentRunTimestamp, E> {
    AgentRunTimestamp::from_unix_millis(value).map_err(|_| E::InvalidStoredData)
}
async fn scalar(
    tx: &Transaction,
    sql: &str,
    parameters: impl libsql::params::IntoParams,
) -> Result<u64, E> {
    let mut rows = tx.query(sql, parameters).await.map_err(unavailable)?;
    n(
        &rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(E::InvalidStoredData)?,
        0,
    )
}
async fn load_goal(
    tx: &Transaction,
    worktree: WorktreeId,
    reference: GoalContractReference,
) -> Result<GoalContract, E> {
    goals::load_revision_from_transaction(tx, worktree, reference.task_id(), reference.revision())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)
}
async fn event(
    tx: &Transaction,
    run: AgentRunId,
    sequence: RunEventSequence,
) -> Result<RunEvent, E> {
    journal::read_event(tx, run, sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)
}
async fn current_file(tx: &Transaction, a: &HttpCorrectionPlanAnchor) -> Result<(), E> {
    let mut rows=tx.query(
        "SELECT 1 FROM index_runs i JOIN file_revisions f USING(index_run_id)
         WHERE i.worktree_id=?1 AND i.index_run_id=?2 AND i.snapshot_id=?3 AND i.status='published'
         AND i.run_sequence=(SELECT MAX(run_sequence) FROM index_runs WHERE worktree_id=?1 AND status='published')
         AND f.repository_path=?4 AND f.content_hash=?5",
        params![a.worktree.as_bytes().to_vec(),a.index.as_bytes().to_vec(),a.snapshot.as_bytes().to_vec(),a.source.path().as_bytes().to_vec(),a.source.content_hash().as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    Ok(())
}
async fn open(store: &LibsqlKnowledgeStore, project: &ProjectIdentity) -> Result<Connection, E> {
    let db = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| E::Unavailable)?;
    if db.schema_version().get() < VERSION {
        return Err(E::UnsupportedSchema);
    }
    db.connection_for_operation()
        .await
        .map_err(|_| E::Unavailable)
}
async fn close<T>(tx: Transaction, result: Result<T, E>) -> Result<T, E> {
    match result {
        Ok(value) => {
            tx.commit().await.map_err(unavailable)?;
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.map_err(unavailable)?;
            Err(error)
        }
    }
}
fn unavailable(_: libsql::Error) -> E {
    E::Unavailable
}

/// Authenticates one immutable actual Context read, independent of current retirement state.
async fn authenticate_context_read(
    tx: &Transaction,
    parent: &Parent,
    a: &HttpContextIntentAnchor,
) -> Result<AuthenticatedHttpContextRead, E> {
    let root = &parent.anchor;
    let read = event(tx, root.run, a.read_tail).await?;
    if a.parent != *root
        || a.parent_job != parent.job
        || a.parent_proposal_digest != parent.digest
        || root.sequence.get().checked_add(1) != Some(a.read_tail.get())
        || read.id() != a.read_event
        || read.sequence() != a.read_tail
        || read.run_id() != root.run
        || read.snapshot_id() != root.snapshot
        || read.kind() != RunEventKind::ToolAction
        || read.occurred_at() != a.read_at
        || read.occurred_at() < parent.accepted_at
        || read.turn_charge().is_some()
        || read.payload().code() != RunEventCode::None
        || read.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || read
            .payload()
            .redaction()
            .is_none_or(|v| v.source() != RunEventRedactionSource::ToolOutput)
        || read.subject() != Some(RunEventSubject::Tool(a.read_tool))
    {
        return Err(E::StaleAnchor);
    }
    let (admission, acquired) = acquired(tx, parent, &read, a.read_tool).await?;
    if admission.key() != a.read_key
        || admission.attempt().attempt() != a.read_attempt
        || admission.attempt().started_at() != a.read_started_at
        || acquired.original() != &EvidenceRef::new(root.source.clone(), a.context_range)
        || a.primary_range != root.source_range
        || a.primary_text != root.source_text
        || !a.primary_range.contains(a.definition_range)
        || a.definition_range.start_position().row().checked_add(1) != Some(a.focus.get())
    {
        return Err(E::StaleAnchor);
    }
    Ok(AuthenticatedHttpContextRead {
        anchor: a.clone(),
        event: read,
    })
}

/// Only the applied-origin composition may use this descriptive Ready reconstruction.
/// It does not retire steps, admit reads, grant source credit or invoke the model.
pub(crate) async fn load_applied_context(
    tx: &Transaction,
    project: &ProjectIdentity,
    goal: &GoalContract,
    run: &AgentRun,
    profile: &ModelProfile,
    parent: &Parent,
) -> Result<AppliedContext, E> {
    if !matches!(parent.proposal.decision(),HttpCorrectionPlanDecision::NeedOfferedSource(_))
        || scalar(tx,"SELECT COUNT(*) FROM http_correction_plan_reservations WHERE run_id=?1 AND outcome='accepted_reply'",
            params![run.id().as_bytes().to_vec()]).await?!=1 {
        return Err(E::StaleAnchor);
    }
    current_file(tx, &parent.anchor).await?;
    let mut rows=tx.query(
        "SELECT CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 256 AND 137096 THEN anchor ELSE x'' END,
         job_digest,CASE WHEN typeof(reply)='blob' AND length(reply) BETWEEN 1 AND 1024 THEN reply ELSE x'' END,
         reply_digest,call_kind,wire_digest,reserved_at,finished_at
         FROM http_context_intent_reservations WHERE run_id=?1 AND diagnostic_digest=?2 AND outcome='accepted_reply' LIMIT 2",
        params![run.id().as_bytes().to_vec(),parent.anchor.diagnostic.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::StaleAnchor)?;
    let anchor = anchor_codec::decode(&blob(&row, 0)?, goal)?;
    let job = hash(&row, 1)?;
    let canonical = blob(&row, 2)?;
    let kind = text(&row, 4)?;
    let wire = blob(&row, 5)?;
    let reserved = n(&row, 6)?;
    let accepted_at = timestamp(n(&row, 7)?)?;
    if anchor.parent.worktree != project.worktree().id()
        || hash(&row, 3)?.as_bytes() != blake3::hash(&canonical).as_bytes()
        || rows.next().await.map_err(unavailable)?.is_some()
    {
        return Err(E::InvalidStoredData);
    }
    let read = authenticate_context_read(tx, parent, &anchor).await?;
    require_context_history(
        tx, &anchor, job, profile, parent, &kind, &wire, reserved, run,
    )
    .await?;
    let reply = HttpContextIntentReply::decode_retained(job, &canonical)
        .map_err(|_| E::InvalidStoredData)?;
    if reply.intent().is_none() {
        return Err(E::StaleAnchor);
    }
    let accepted_wire = ContentHash::from_bytes(wire.try_into().map_err(|_| E::InvalidStoredData)?);
    Ok(AppliedContext {
        anchor,
        job,
        reply,
        accepted_wire,
        accepted_at,
        read,
    })
}
