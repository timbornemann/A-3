//! Read-only closed A fact for the first HTTP correction. No historical execution admission.
use super::origin::{OriginFrame, SqlOriginFrame};
use super::*;
use a3_application::{
    ClosedEditNoChangeCall, ClosedEditNoChangeFact, ClosedEditNoChangeOwner, ClosedEditWindow,
    ClosedHttpNoChangeOrigin, ClosedNoChangeProfileLimits,
};

// Constructed only below after the actual closed row/aggregate/event checks. It is
// a purpose for verifying the old abandoned Original, never a SourceDelivery token.
pub(crate) struct ClosedReadPurpose<'a> {
    fact: &'a ClosedEditNoChangeFact,
    closure: &'a RunEvent,
}
impl ClosedReadPurpose<'_> {
    pub(crate) fn fact(&self) -> &ClosedEditNoChangeFact {
        self.fact
    }
    pub(crate) fn closure(&self) -> &RunEvent {
        self.closure
    }
}
// This stronger private token exists only after the real Original/ToolAction has
// also been authenticated. The old live origin entrance cannot construct it.
pub(super) struct AuthenticatedClosedNoChange {
    fact: ClosedEditNoChangeFact,
    closure: RunEvent,
    tool: ToolRunId,
}
impl AuthenticatedClosedNoChange {
    pub(super) fn fact(&self) -> &ClosedEditNoChangeFact {
        &self.fact
    }
    pub(super) fn closure(&self) -> &RunEvent {
        &self.closure
    }
    pub(super) const fn tool(&self) -> ToolRunId {
        self.tool
    }
}

pub(super) fn load<'a>(
    store: &'a LibsqlKnowledgeStore,
    frame: OriginFrame<'a>,
) -> EditPhaseReservationFuture<'a, Option<ClosedHttpNoChangeOrigin>> {
    Box::pin(async move {
        let frame: SqlOriginFrame<'_> = frame.into();
        let connection = open(store, frame.project).await?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result = async {
            let Some(authenticated) = authenticate_fact(&tx, frame).await? else {
                return Ok(None);
            };
            let (usage, latest) =
                edit::read_run_usage(&tx, frame.project.worktree().id(), frame.run).await?;
            if usage.unobserved_calls != 0 || usage.unknown_calls != 0 {
                return Err(E::UncertainProviderCall);
            }
            if latest != Some(authenticated.fact.call().observation().finished_at()) {
                return Err(E::StaleAnchor);
            }
            let parent =
                super::origin::load_for_closed_no_change(&tx, frame, &authenticated).await?;
            Ok(Some(ClosedHttpNoChangeOrigin::from_recorded(
                parent,
                authenticated.fact,
                authenticated.closure,
                frame.stored.version(),
            )?))
        }
        .await;
        close(tx, result).await
    })
}

// Fact/closure/Original authority only. The caller separately authenticates the
// first parent plus either the strict B1 clock or one exact private V6 history.
pub(super) async fn authenticate_fact(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
) -> Result<Option<AuthenticatedClosedNoChange>, E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < 67
    {
        return Err(E::UnsupportedSchema);
    }
    validate_frame(tx, frame).await?;
    let Some(fact) = read_fact(tx, frame.project.worktree().id(), frame.goal, frame.run).await?
    else {
        return Ok(None);
    };
    let closure = journal::read_event(tx, frame.run.id(), frame.run.last_event_sequence())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    validate_closed(&fact, &closure, frame)?;
    require_lineage(tx, &fact).await?;
    let purpose = ClosedReadPurpose {
        fact: &fact,
        closure: &closure,
    };
    let original = crate::source_read_repository::closed_no_change_original(tx, &purpose).await?;
    if original.original != *fact.window().original() {
        return Err(E::InvalidStoredData);
    }
    Ok(Some(AuthenticatedClosedNoChange {
        fact,
        closure,
        tool: original.tool,
    }))
}

// Distinct applied-B composition: actual current rows only. No old Run or Ledger
// is reconstructed, and the public N-only fact entry above is unchanged.
/// Authenticates old A rows only within the real current B material prefix.
pub(super) async fn authenticate_applied_fact(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
) -> Result<AuthenticatedClosedNoChange, E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < 68
    {
        return Err(E::UnsupportedSchema);
    }
    let worktree = frame.project.worktree().id();
    let run = frame.run;
    let stored = frame.stored;
    let goal = frame.goal;
    if !frame.profile.executable_actions_enabled()
        || run.goal_contract() != goal.reference()
        || stored.ledger().goal_contract() != goal.reference()
        || run.task_ledger_revision() != stored.ledger().revision()
        || run.current_snapshot_id() != frame.snapshot
        || run.model_profile() != Some(frame.profile.reference())
        || stored.ledger().pending_planning().is_some()
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
        || journal::load_run_from_transaction(tx, worktree, run.id())
            .await
            .map_err(|_| E::InvalidStoredData)?
            != Some(run.clone())
        || ledgers::load_from_transaction(tx, worktree, goal.task_id())
            .await
            .map_err(|_| E::InvalidStoredData)?
            != Some(stored.clone())
        || goals::load_revision_from_transaction(
            tx,
            worktree,
            goal.task_id(),
            goal.reference().revision(),
        )
        .await
        .map_err(|_| E::InvalidStoredData)?
            != Some(goal.clone())
        || goals::task_current_revision(tx, worktree, goal.task_id())
            .await
            .map_err(unavailable)?
            != Some(goal.reference().revision())
    {
        return Err(E::StaleAnchor);
    }
    if frame.now < run.updated_at() {
        return Err(E::TimestampRegressed);
    }
    if let Some(exhausted) = run
        .budget_exhaustion(frame.now)
        .map_err(|_| E::InvalidStoredData)?
    {
        return Err(E::BudgetExhausted(exhausted.dimension()));
    }
    let fact = read_fact(tx, frame.project.worktree().id(), goal, run)
        .await?
        .ok_or(E::StaleAnchor)?;
    let a = fact.owner();
    let closure_sequence = RunEventSequence::new(
        a.sequence
            .get()
            .checked_add(1)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)?;
    let closure = journal::read_event(tx, run.id(), closure_sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    let delta = run
        .last_event_sequence()
        .get()
        .checked_sub(closure_sequence.get())
        .ok_or(E::StaleAnchor)?;
    let execute = run.state() == AgentControllerState::Execute;
    let current_revision = a.ledger.get().checked_add(1).ok_or(E::InvalidStoredData)?;
    if !matches!(
        (run.state(), delta),
        (AgentControllerState::Replan, 1)
            | (AgentControllerState::Localize, 2)
            | (AgentControllerState::Plan, 3)
            | (AgentControllerState::Execute, 4 | 5)
    ) || stored.ledger().revision().get() != current_revision
        || a.version.get().checked_add(if execute { 3 } else { 2 }) != Some(stored.version().get())
        || a.worktree != worktree
        || a.goal != goal.reference()
        || a.run != run.id()
        || goal.original().map(|g| g.digest()) != Some(a.original_goal)
        || a.index != frame.index
        || a.snapshot != frame.snapshot
        || a.profile != frame.profile.reference()
        || fact.limits() != ClosedNoChangeProfileLimits::from_profile(frame.profile)?
        || closure.id() != fact.closure_event_id()
        || closure.sequence() != closure_sequence
        || closure.run_id() != a.run
        || closure.snapshot_id() != a.snapshot
        || closure.occurred_at() < fact.call().observation().finished_at()
        || closure.occurred_at() >= fact.call().deadline()
        || closure.occurred_at() > run.updated_at()
        || closure.subject().is_some()
        || closure.turn_charge().is_some()
        || closure.payload().redaction().is_some()
        || closure.payload().code() != RunEventCode::ControllerDecision
        || closure.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || closure.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || run
            .created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            != Some(fact.call().deadline().unix_millis())
    {
        return Err(E::StaleAnchor);
    }
    let old = stored.ledger().step(a.step).ok_or(E::StaleAnchor)?;
    let [attempt] = old.attempts() else {
        return Err(E::StaleAnchor);
    };
    if old.is_active_plan_step()
        || old.retired_in_revision() != Some(stored.ledger().revision())
        || old.introduced_in_revision() != a.ledger
        || old.status() != TaskStepStatus::Failed
        || old.definition().verification_spec().id() != a.verification
        || old.definition().planned_file_addition().is_some()
        || a.attempt.get() != 1
        || attempt.number() != a.attempt
        || attempt.run_id() != a.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|t| t.unix_millis())
            != Some(closure.occurred_at().unix_millis())
        || attempt.started_at().unix_millis() > fact.call().reserved_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    let mut current = tx.query("SELECT 1 FROM index_runs i JOIN file_revisions f USING(index_run_id)
        WHERE i.worktree_id=?1 AND i.status='published' AND i.index_run_id=?2 AND i.snapshot_id=?3
        AND i.run_sequence=(SELECT MAX(run_sequence) FROM index_runs WHERE worktree_id=?1 AND status='published')
        AND f.repository_path=?4 AND f.content_hash=?5",params![worktree.as_bytes().to_vec(),frame.index.as_bytes().to_vec(),
            frame.snapshot.as_bytes().to_vec(),fact.window().revision().path().as_bytes().to_vec(),fact.window().revision().content_hash().as_bytes().to_vec()])
        .await.map_err(unavailable)?;
    if current.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    require_lineage(tx, &fact).await?;
    let purpose = ClosedReadPurpose {
        fact: &fact,
        closure: &closure,
    };
    let original = crate::source_read_repository::closed_no_change_original(tx, &purpose).await?;
    if original.original != *fact.window().original() {
        return Err(E::InvalidStoredData);
    }
    Ok(AuthenticatedClosedNoChange {
        fact,
        closure,
        tool: original.tool,
    })
}

async fn validate_frame(tx: &Transaction, frame: SqlOriginFrame<'_>) -> Result<(), E> {
    let SqlOriginFrame {
        project,
        goal,
        stored,
        run,
        index,
        snapshot,
        profile,
        now: _,
    } = frame;
    let worktree = project.worktree().id();
    if !profile.executable_actions_enabled()
        || run.state() != AgentControllerState::Replan
        || run.goal_contract() != goal.reference()
        || stored.ledger().goal_contract() != goal.reference()
        || run.task_ledger_revision() != stored.ledger().revision()
        || run.current_snapshot_id() != snapshot
        || run.model_profile() != Some(profile.reference())
        || stored.ledger().pending_planning().is_some()
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
        || journal::load_run_from_transaction(tx, worktree, run.id())
            .await
            .map_err(|_| E::InvalidStoredData)?
            != Some(run.clone())
        || ledgers::load_from_transaction(tx, worktree, goal.task_id())
            .await
            .map_err(|_| E::InvalidStoredData)?
            != Some(stored.clone())
        || goals::load_revision_from_transaction(
            tx,
            worktree,
            goal.task_id(),
            goal.reference().revision(),
        )
        .await
        .map_err(|_| E::InvalidStoredData)?
            != Some(goal.clone())
        || goals::task_current_revision(tx, worktree, goal.task_id())
            .await
            .map_err(unavailable)?
            != Some(goal.reference().revision())
    {
        return Err(E::StaleAnchor);
    }
    let mut rows = tx.query("SELECT 1 FROM index_runs WHERE worktree_id=?1 AND status='published'
        AND index_run_id=?2 AND snapshot_id=?3 AND run_sequence=(SELECT MAX(run_sequence) FROM index_runs
        WHERE worktree_id=?1 AND status='published')", params![worktree.as_bytes().to_vec(),
        index.as_bytes().to_vec(), snapshot.as_bytes().to_vec()]).await.map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    Ok(())
}

pub(super) async fn read_fact(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
) -> Result<Option<ClosedEditNoChangeFact>, E> {
    read_fact_for_ledger(tx, worktree, goal, run, None).await
}

// Select the actual old A epoch through its V6/receipt foreign identity.
// A later horizon changes the current Ledger revision; it must not move this origin.
// Public B1/Applied reads retain their original all-first-facts uniqueness rule.
pub(super) async fn read_fact_for_replan_origin(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
) -> Result<ClosedEditNoChangeFact, E> {
    let mut rows=tx.query("SELECT DISTINCT e.ledger_revision FROM http_no_change_replan_reservations v
        JOIN edit_phase_reservations e ON e.run_id=v.run_id AND e.job_digest=v.closed_job_digest AND e.call_kind=v.closed_call_kind
        JOIN edit_phase_no_change_receipts n ON n.run_id=e.run_id AND n.job_digest=e.job_digest AND n.call_kind=e.call_kind
        WHERE v.run_id=?1 AND v.worktree_id=?2 AND e.worktree_id=?2 LIMIT 2",
        params![run.id().as_bytes().to_vec(),worktree.as_bytes().to_vec()]).await.map_err(unavailable)?;
    let old = TaskLedgerRevision::new(count(
        &rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(E::StaleAnchor)?,
        0,
    )?)
    .map_err(|_| E::InvalidStoredData)?;
    if rows.next().await.map_err(unavailable)?.is_some() {
        return Err(E::InvalidStoredData);
    }
    read_fact_for_ledger(tx, worktree, goal, run, Some(old))
        .await?
        .ok_or(E::StaleAnchor)
}

async fn read_fact_for_ledger(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    run: &AgentRun,
    ledger: Option<TaskLedgerRevision>,
) -> Result<Option<ClosedEditNoChangeFact>, E> {
    let mut rows = tx.query("SELECT e.worktree_id,e.task_id,e.goal_revision,e.ledger_revision,
        e.ledger_store_version,e.run_sequence,e.step_id,e.attempt_number,e.verification_id,e.snapshot_id,
        e.source_read_id,e.wire_digest,e.wire_bytes,e.prompt_reserve,e.output_reserve,e.reserved_at,e.deadline,
        e.outcome,e.finished_at,e.reported_prompt,e.reported_output,e.terminal_reply_kind,
        n.record_version,n.index_run_id,n.profile_id,n.profile_version,n.generation_limit,n.context_limit,
        n.original_goal_digest,n.repository_path,n.content_hash,n.window_start,n.window_end,n.window_digest,
        n.source_read_id,n.original_range,e.job_digest,e.call_kind FROM edit_phase_reservations e
        LEFT JOIN edit_phase_no_change_receipts n ON n.run_id=e.run_id AND n.job_digest=e.job_digest AND n.call_kind=e.call_kind
        WHERE e.run_id=?1 AND (e.terminal_reply_kind='no_change' OR n.run_id IS NOT NULL)
        AND (?2 IS NULL OR e.ledger_revision=?2)
        ORDER BY e.call_kind LIMIT 2", params![run.id().as_bytes().to_vec(),ledger.map(|r|i64::from(r.get()))]).await.map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    if bytes32(&row, 0)? != *worktree.as_bytes()
        || bytes32(&row, 1)? != *goal.task_id().as_bytes()
        || count(&row, 2)? != goal.reference().revision().get()
        || text(&row, 17)? != "rejected_reply"
        || text(&row, 21)? != "no_change"
        || count(&row, 22)? != 1
        || bytes32(&row, 10)? != bytes32(&row, 34)?
    {
        return Err(E::InvalidStoredData);
    }
    let owner = ClosedEditNoChangeOwner {
        worktree,
        goal: goal.reference(),
        original_goal: GoalOriginalDigest::from_bytes(bytes32(&row, 28)?),
        run: run.id(),
        ledger: TaskLedgerRevision::new(count(&row, 3)?).map_err(|_| E::InvalidStoredData)?,
        version: a3_application::TaskLedgerStoreVersion::new(n(&row, 4)?)
            .map_err(|_| E::InvalidStoredData)?,
        sequence: RunEventSequence::new(n(&row, 5)?).map_err(|_| E::InvalidStoredData)?,
        step: TaskStepId::from_bytes(bytes32(&row, 6)?),
        attempt: TaskStepAttemptNumber::new(count(&row, 7)?).map_err(|_| E::InvalidStoredData)?,
        verification: VerificationSpecId::from_bytes(bytes32(&row, 8)?),
        snapshot: SnapshotId::from_bytes(bytes32(&row, 9)?),
        index: IndexRunId::from_bytes(bytes32(&row, 23)?),
        profile: ModelProfileReference::new(
            ModelProfileId::from_bytes(bytes32(&row, 24)?),
            ModelProfileVersion::from_u16(
                u16::try_from(n(&row, 25)?).map_err(|_| E::InvalidStoredData)?,
            )
            .map_err(|_| E::InvalidStoredData)?,
        ),
    };
    let range = decode_range(&row, 35)?;
    if range.start_byte() != count(&row, 31)? || range.end_byte() != count(&row, 32)? {
        return Err(E::InvalidStoredData);
    }
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(blob(&row, 29)?).map_err(|_| E::InvalidStoredData)?,
        ContentHash::from_bytes(bytes32(&row, 30)?),
    );
    let window = ClosedEditWindow::from_recorded(
        EvidenceRef::new(revision, range),
        ContentHash::from_bytes(bytes32(&row, 33)?),
    )?;
    let call = ClosedEditNoChangeCall::from_recorded(
        kind(&row, 37)?,
        EditPhaseWireCost::new(
            ContentHash::from_bytes(bytes32(&row, 11)?),
            count(&row, 12)?,
            count(&row, 13)?,
            count(&row, 14)?,
        )?,
        time(&row, 15)?,
        time(&row, 16)?,
        EditPhaseObservation::new(
            Outcome::NoChange,
            Some(count(&row, 19)?),
            Some(count(&row, 20)?),
            time(&row, 18)?,
        ),
    )?;
    let limits = ClosedNoChangeProfileLimits::from_recorded(
        ModelContextLimit::new(count(&row, 27)?).map_err(|_| E::InvalidStoredData)?,
        ModelOutputLimit::new(count(&row, 26)?).map_err(|_| E::InvalidStoredData)?,
    )?;
    let fact = ClosedEditNoChangeFact::from_recorded(
        owner,
        window,
        ContentHash::from_bytes(bytes32(&row, 36)?),
        ContentHash::from_bytes(bytes32(&row, 10)?),
        call,
        limits,
    )?;
    // Every field is now owned. Native libSQL Row shares the stepped statement.
    if rows.next().await.map_err(unavailable)?.is_some() {
        return Err(E::InvalidStoredData);
    }
    Ok(Some(fact))
}

fn validate_closed(
    fact: &ClosedEditNoChangeFact,
    closure: &RunEvent,
    frame: SqlOriginFrame<'_>,
) -> Result<(), E> {
    let SqlOriginFrame {
        goal,
        stored,
        run,
        index,
        profile,
        ..
    } = frame;
    let a = fact.owner();
    let observed = fact.call().observation();
    if goal.original().map(|g| g.digest()) != Some(a.original_goal)
        || a.goal != goal.reference()
        || a.run != run.id()
        || a.ledger != stored.ledger().revision()
        || a.version.get().checked_add(1) != Some(stored.version().get())
        || a.sequence.get().checked_add(1) != Some(run.last_event_sequence().get())
        || a.index != index
        || a.snapshot != run.current_snapshot_id()
        || a.profile != profile.reference()
        || fact.limits() != ClosedNoChangeProfileLimits::from_profile(profile)?
        || closure.id() != fact.closure_event_id()
        || closure.sequence() != run.last_event_sequence()
        || closure.run_id() != a.run
        || closure.snapshot_id() != a.snapshot
        || closure.occurred_at() != run.updated_at()
        || closure.occurred_at() < observed.finished_at()
        || closure.occurred_at() >= fact.call().deadline()
        || closure.subject().is_some()
        || closure.turn_charge().is_some()
        || closure.payload().redaction().is_some()
        || closure.payload().code() != RunEventCode::ControllerDecision
        || closure.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || closure.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || run
            .created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            != Some(fact.call().deadline().unix_millis())
    {
        return Err(E::StaleAnchor);
    }
    let step = stored.ledger().step(a.step).ok_or(E::StaleAnchor)?;
    if !step.is_active_plan_step()
        || step.status() != TaskStepStatus::Failed
        || step.definition().verification_spec().id() != a.verification
        || step.definition().planned_file_addition().is_some()
        || step.attempts().len() != 1
    {
        return Err(E::StaleAnchor);
    }
    let attempt = step.attempts().last().ok_or(E::StaleAnchor)?;
    if attempt.number() != a.attempt
        || a.attempt.get() != 1
        || attempt.run_id() != a.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|t| t.unix_millis())
            != Some(closure.occurred_at().unix_millis())
        || stored.ledger().updated_at().unix_millis() != closure.occurred_at().unix_millis()
        || attempt.started_at().unix_millis() > fact.call().reserved_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}

async fn require_lineage(tx: &Transaction, fact: &ClosedEditNoChangeFact) -> Result<(), E> {
    let a = fact.owner();
    let selected = fact.call();
    let mut rows = tx.query("SELECT call_kind,outcome,terminal_reply_kind,wire_digest,wire_bytes,
        prompt_reserve,output_reserve,reserved_at,deadline,finished_at,reported_prompt,reported_output,
        job_digest=?2 AND worktree_id=?4 AND task_id=?5 AND goal_revision=?6 AND ledger_revision=?7
        AND ledger_store_version=?8 AND run_sequence=?9 AND step_id=?10 AND attempt_number=?11
        AND verification_id=?12 AND snapshot_id=?13 AND source_read_id=?3
        FROM edit_phase_reservations WHERE run_id=?1 AND (job_digest=?2 OR source_read_id=?3)
        ORDER BY CASE call_kind WHEN 'initial' THEN 0 ELSE 1 END LIMIT 3",
        params![a.run.as_bytes().to_vec(),fact.job_digest().as_bytes().to_vec(),fact.source_read_key().as_bytes().to_vec(),
            a.worktree.as_bytes().to_vec(),a.goal.task_id().as_bytes().to_vec(),i64::from(a.goal.revision().get()),
            i64::from(a.ledger.get()),signed(a.version.get())?,signed(a.sequence.get())?,a.step.as_bytes().to_vec(),
            i64::from(a.attempt.get()),a.verification.as_bytes().to_vec(),a.snapshot.as_bytes().to_vec()]).await.map_err(unavailable)?;
    let mut actual = 0;
    let mut first_wire = None;
    let mut first_finished = None;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        let k = kind(&row, 0)?;
        let disposition = text(&row, 1)?;
        let tag: Option<String> = row.get(2).map_err(|_| E::InvalidStoredData)?;
        let wire = EditPhaseWireCost::new(
            ContentHash::from_bytes(bytes32(&row, 3)?),
            count(&row, 4)?,
            count(&row, 5)?,
            count(&row, 6)?,
        )?;
        let reserved = time(&row, 7)?;
        let deadline = time(&row, 8)?;
        let finished = time(&row, 9)?;
        let prompt = count(&row, 10)?;
        let output = count(&row, 11)?;
        if n(&row, 12)? != 1
            || deadline != selected.deadline()
            || reserved > finished
            || finished >= deadline
            || prompt == 0
            || prompt > wire.prompt_estimate()
            || output == 0
            || output > fact.limits().generation_limit().get()
            || wire.output_reserve() != selected.wire().output_reserve()
            || wire
                .prompt_estimate()
                .checked_add(wire.output_reserve())
                .is_none_or(|n| n > fact.limits().context_limit().get())
        {
            return Err(E::InvalidStoredData);
        }
        if k == selected.kind() {
            if actual != u8::from(k == EditPhaseCallKind::StructuralRepair)
                || disposition != "rejected_reply"
                || tag.as_deref() != Some("no_change")
                || wire != selected.wire()
                || reserved != selected.reserved_at()
                || finished != selected.observation().finished_at()
                || Some(prompt) != selected.observation().reported_prompt_tokens()
                || Some(output) != selected.observation().reported_output_tokens()
                || (k == EditPhaseCallKind::StructuralRepair
                    && (first_wire == Some(wire.digest())
                        || first_finished.is_none_or(|t| t > reserved)))
            {
                return Err(E::InvalidStoredData);
            }
        } else if actual != 0
            || k != EditPhaseCallKind::Initial
            || selected.kind() != EditPhaseCallKind::StructuralRepair
            || disposition != "invalid_structure"
            || tag.is_some()
        {
            return Err(E::InvalidStoredData);
        }
        first_wire.get_or_insert(wire.digest());
        first_finished.get_or_insert(finished);
        actual += 1;
    }
    if actual
        != if selected.kind() == EditPhaseCallKind::Initial {
            1
        } else {
            2
        }
    {
        return Err(E::InvalidStoredData);
    }
    Ok(())
}

fn bytes32(row: &Row, at: i32) -> Result<[u8; 32], E> {
    blob(row, at)?.try_into().map_err(|_| E::InvalidStoredData)
}
fn count(row: &Row, at: i32) -> Result<u32, E> {
    u32::try_from(n(row, at)?).map_err(|_| E::InvalidStoredData)
}
fn signed(n: u64) -> Result<i64, E> {
    i64::try_from(n).map_err(|_| E::InvalidStoredData)
}
fn time(row: &Row, at: i32) -> Result<AgentRunTimestamp, E> {
    AgentRunTimestamp::from_unix_millis(n(row, at)?).map_err(|_| E::InvalidStoredData)
}
fn text(row: &Row, at: i32) -> Result<String, E> {
    row.get(at).map_err(|_| E::InvalidStoredData)
}
fn kind(row: &Row, at: i32) -> Result<EditPhaseCallKind, E> {
    match text(row, at)?.as_str() {
        "initial" => Ok(EditPhaseCallKind::Initial),
        "structural_repair" => Ok(EditPhaseCallKind::StructuralRepair),
        _ => Err(E::InvalidStoredData),
    }
}
fn decode_range(row: &Row, at: i32) -> Result<SourceRange, E> {
    let bytes: [u8; 24] = blob(row, at)?
        .try_into()
        .map_err(|_| E::InvalidStoredData)?;
    let mut values = [0u32; 6];
    for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(4)) {
        *value = u32::from_le_bytes(chunk.try_into().map_err(|_| E::InvalidStoredData)?);
    }
    SourceRange::new(
        values[0] as usize,
        values[1] as usize,
        SourcePosition::new(values[2], values[3]),
        SourcePosition::new(values[4], values[5]),
    )
    .map_err(|_| E::InvalidStoredData)
}

// A-only historical identity for actual V6 material, not a model/resume capability.
// Full profile/wire/currentness admission happened at actual V6/material creation;
// this read reauthenticates A/Original ancestry without checking pending own Edit usage.
pub(super) async fn authenticate_material_history_fact(
    tx: &Transaction,
    worktree: WorktreeId,
    goal: &GoalContract,
    stored: &a3_application::StoredTaskLedger,
    run: &AgentRun,
    fact: ClosedEditNoChangeFact,
) -> Result<AuthenticatedClosedNoChange, E> {
    let a = fact.owner();
    let n = RunEventSequence::new(
        a.sequence
            .get()
            .checked_add(1)
            .ok_or(E::InvalidStoredData)?,
    )
    .map_err(|_| E::InvalidStoredData)?;
    let closure = journal::read_event(tx, run.id(), n)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let material_revision =
        TaskLedgerRevision::new(a.ledger.get().checked_add(1).ok_or(E::InvalidStoredData)?)
            .map_err(|_| E::InvalidStoredData)?;
    if a.worktree != worktree
        || a.run != run.id()
        || a.goal != goal.reference()
        || run.goal_contract() != a.goal
        || stored.ledger().goal_contract() != a.goal
        || goal.original().map(|g| g.digest()) != Some(a.original_goal)
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
        || run.task_ledger_revision() != stored.ledger().revision()
        || stored.ledger().pending_planning().is_some()
        || run.model_profile() != Some(a.profile)
        || closure.id() != fact.closure_event_id()
        || closure.sequence() != n
        || closure.run_id() != a.run
        || closure.snapshot_id() != a.snapshot
        || closure.occurred_at() < fact.call().observation().finished_at()
        || closure.occurred_at() >= fact.call().deadline()
        || closure.occurred_at() > run.updated_at()
        || closure.subject().is_some()
        || closure.turn_charge().is_some()
        || closure.payload()
            != &RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            )
        || closure.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || run
            .created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            != Some(fact.call().deadline().unix_millis())
    {
        return Err(E::StaleAnchor);
    }
    let old = stored.ledger().step(a.step).ok_or(E::StaleAnchor)?;
    let [attempt] = old.attempts() else {
        return Err(E::StaleAnchor);
    };
    if old.is_active_plan_step()
        || old.retired_in_revision() != Some(material_revision)
        || old.introduced_in_revision() != a.ledger
        || old.status() != TaskStepStatus::Failed
        || old.definition().verification_spec().id() != a.verification
        || old.definition().planned_file_addition().is_some()
        || a.attempt.get() != 1
        || attempt.number() != a.attempt
        || attempt.run_id() != a.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|t| t.unix_millis())
            != Some(closure.occurred_at().unix_millis())
        || attempt.started_at().unix_millis() > fact.call().reserved_at().unix_millis()
    {
        return Err(E::StaleAnchor);
    }
    require_lineage(tx, &fact).await?;
    let purpose = ClosedReadPurpose {
        fact: &fact,
        closure: &closure,
    };
    let original = crate::source_read_repository::closed_no_change_original(tx, &purpose).await?;
    if original.original != *fact.window().original() {
        return Err(E::InvalidStoredData);
    }
    Ok(AuthenticatedClosedNoChange {
        fact,
        closure,
        tool: original.tool,
    })
}

// The current B source scope keeps all original N+4/N+5/index-era contracts.
// This check is deliberately separate from reading old material for later horizons.
pub(super) fn require_current_edit_frame(
    auth: &AuthenticatedClosedNoChange,
    stored: &a3_application::StoredTaskLedger,
    run: &AgentRun,
) -> Result<(), E> {
    let a = auth.fact().owner();
    let delta = run
        .last_event_sequence()
        .get()
        .checked_sub(auth.closure().sequence().get())
        .ok_or(E::StaleAnchor)?;
    if run.state() != AgentControllerState::Execute
        || !matches!(delta, 4 | 5)
        || a.ledger.get().checked_add(1) != Some(stored.ledger().revision().get())
        || a.version.get().checked_add(3) != Some(stored.version().get())
        || run.current_snapshot_id() != a.snapshot
    {
        return Err(E::StaleAnchor);
    }
    Ok(())
}
