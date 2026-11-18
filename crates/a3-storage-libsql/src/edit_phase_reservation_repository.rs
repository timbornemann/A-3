use crate::{
    LibsqlKnowledgeStore, agent_recovery_repository, goal_contract_repository,
    run_journal_repository, task_ledger_repository,
};
use a3_application::{
    AcquiredSourceRead, CompiledEditPhaseRequest, EditEvidenceNeed, EditNoChangeReceipt,
    EditNoChangeWindow, EditPhaseAnchor, EditPhaseCallKind, EditPhaseHalt, EditPhaseHaltReceipt,
    EditPhaseObservation, EditPhaseObservationOutcome, EditPhaseReservation,
    EditPhaseReservationFailure as Failure, EditPhaseReservationFuture, EditPhaseReservationStore,
    EditPhaseReservedUsage, EditPhaseWireCost, PreparedEditPhase,
};
use a3_domain::{
    AgentBudgetDimension, AgentControllerState, AgentRun, AgentRunBudget, AgentRunId,
    AgentRunTimestamp, AgentRunUsage, ContentHash, ModelProfile, ProjectIdentity, PublishedIndex,
    TaskStepAttemptOutcome, TaskStepStatus, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::time::Instant;

const RESERVATION_SCHEMA: u32 = 49;

impl EditPhaseReservationStore for LibsqlKnowledgeStore {
    fn reserve_edit_job<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedEditPhase,
        compiled: &'a CompiledEditPhaseRequest,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation> {
        let started = Instant::now();
        Box::pin(async move {
            phase
                .require_scope_binding(compiled)
                .map_err(|_| Failure::StaleAnchor)?;
            if compiled.anchor() != phase.anchor()
                || compiled.job_digest() != phase.job_digest()
                || phase.anchor().worktree() != project.worktree().id()
            {
                return Err(Failure::StaleAnchor);
            }
            let connection = open(self, project).await?;
            reserve(
                &connection,
                phase,
                compiled.kind(),
                wire,
                now,
                compiled.acquired_source(),
                Some(compiled),
                Some(started),
            )
            .await
        })
    }
    fn finish_edit_job<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: EditPhaseReservation,
        compiled: &'a CompiledEditPhaseRequest,
        observation: EditPhaseObservation,
        halt: Option<EditPhaseHalt>,
    ) -> EditPhaseReservationFuture<'a, ()> {
        let started = Instant::now();
        Box::pin(finish_edit_job_entry(
            self,
            project,
            reservation,
            compiled,
            observation,
            halt,
            started,
            finish_ready,
        ))
    }
    fn load_edit_halt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        compiled: &'a CompiledEditPhaseRequest,
    ) -> EditPhaseReservationFuture<'a, Option<EditPhaseHaltReceipt>> {
        Box::pin(async move {
            if compiled.anchor().worktree() != project.worktree().id() {
                return Err(Failure::StaleAnchor);
            }
            let connection = open(self, project).await?;
            load_halt(&connection, compiled).await
        })
    }

    fn reserve_edit_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedEditPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation> {
        Box::pin(async move {
            let worktree = project.worktree().id();
            if phase.anchor().worktree() != worktree {
                return Err(Failure::StaleAnchor);
            }
            let connection = open(self, project).await?;
            reserve(&connection, phase, kind, wire, now, None, None, None).await
        })
    }

    fn reserve_edit_phase_with_source<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedEditPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
        source: &'a AcquiredSourceRead,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservation> {
        Box::pin(async move {
            if phase.anchor().worktree() != project.worktree().id() {
                return Err(Failure::StaleAnchor);
            }
            let connection = open(self, project).await?;
            reserve(
                &connection,
                phase,
                kind,
                wire,
                now,
                Some(source),
                None,
                None,
            )
            .await
        })
    }

    fn finish_edit_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: EditPhaseReservation,
        observation: EditPhaseObservation,
    ) -> EditPhaseReservationFuture<'a, ()> {
        Box::pin(async move {
            if reservation.anchor().worktree() != project.worktree().id() {
                return Err(Failure::StaleAnchor);
            }
            let connection = open(self, project).await?;
            finish(
                &connection,
                &reservation,
                observation,
                None,
                None,
                None,
                finish_ready,
            )
            .await
        })
    }

    fn finish_edit_no_change<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: EditPhaseReservation,
        observation: EditPhaseObservation,
        window: &'a EditNoChangeWindow,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
    ) -> EditPhaseReservationFuture<'a, ()> {
        let started = Instant::now();
        Box::pin(crate::edit_no_change_repository::finish(
            self,
            project,
            reservation,
            observation,
            window,
            current,
            profile,
            started,
        ))
    }

    fn load_edit_no_change<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedEditPhase,
        profile: &'a ModelProfile,
        current: &'a PublishedIndex,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<EditNoChangeReceipt>> {
        Box::pin(crate::edit_no_change_repository::load(
            self, project, phase, profile, current, now,
        ))
    }

    fn load_edit_phase_usage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run: AgentRunId,
    ) -> EditPhaseReservationFuture<'a, EditPhaseReservedUsage> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(unavailable)?;
            let result = async {
                let worktree = project.worktree().id();
                let run = load_run(&transaction, worktree, run).await?;
                Ok(read_run_usage(&transaction, worktree, &run).await?.0)
            }
            .await;
            close(transaction, result).await
        })
    }
}

fn finish_ready(_: &Transaction) -> std::future::Ready<Result<(), Failure>> {
    std::future::ready(Ok(()))
}

// Production and the one test-only bridge enter the identical authentication/SQL path.
#[allow(clippy::too_many_arguments)]
async fn finish_edit_job_entry<F, W>(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    reservation: EditPhaseReservation,
    compiled: &CompiledEditPhaseRequest,
    observation: EditPhaseObservation,
    halt: Option<EditPhaseHalt>,
    started: Instant,
    after_writes: F,
) -> Result<(), Failure>
where
    F: FnOnce(&Transaction) -> W + Send,
    W: std::future::Future<Output = Result<(), Failure>> + Send,
{
    if reservation.anchor().worktree() != project.worktree().id() {
        return Err(Failure::StaleAnchor);
    }
    let connection = open(store, project).await?;
    finish(
        &connection,
        &reservation,
        observation,
        Some(compiled),
        halt,
        Some(started),
        after_writes,
    )
    .await
}

/// Test-only crate-internal access to this exact finish after its real terminal SQL writes.
/// The owned callback is awaited once in the same transaction; it creates no public/Release API.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_edit_job_after_writes_for_test<'a, F, W>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    reservation: EditPhaseReservation,
    compiled: &'a CompiledEditPhaseRequest,
    observation: EditPhaseObservation,
    halt: Option<EditPhaseHalt>,
    after_writes: F,
) -> EditPhaseReservationFuture<'a, ()>
where
    F: FnOnce(&Transaction) -> W + Send + 'a,
    W: std::future::Future<Output = Result<(), Failure>> + Send + 'a,
{
    let started = Instant::now();
    Box::pin(finish_edit_job_entry(
        store,
        project,
        reservation,
        compiled,
        observation,
        halt,
        started,
        after_writes,
    ))
}

async fn open(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
) -> Result<Connection, Failure> {
    let database = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| Failure::Unavailable)?;
    if database.schema_version().get() < RESERVATION_SCHEMA {
        return Err(Failure::UnsupportedSchema);
    }
    database
        .connection_for_operation()
        .await
        .map_err(|_| Failure::Unavailable)
}

#[allow(clippy::too_many_arguments)]
async fn reserve(
    connection: &Connection,
    phase: &PreparedEditPhase,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    now: AgentRunTimestamp,
    source: Option<&AcquiredSourceRead>,
    compiled: Option<&CompiledEditPhaseRequest>,
    started: Option<Instant>,
) -> Result<EditPhaseReservation, Failure> {
    if let Some(compiled) = compiled {
        phase
            .require_scope_binding(compiled)
            .map_err(|_| Failure::StaleAnchor)?;
        if started.is_none() {
            return Err(Failure::InvalidStoredData);
        }
    } else {
        phase
            .require_unqualified_scope()
            .map_err(|_| Failure::StaleAnchor)?;
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(unavailable)?;
    let result = async {
        let anchor = phase.anchor();
        let job = phase.job_digest();
        if let Some(compiled) = compiled {
            require_job_schema(&transaction).await?;
            check_profile_wire(wire, compiled.request().profile())?;
            require_scope_index(&transaction, compiled).await?;
        } else {
            require_publication(&transaction, anchor, phase.source_publication()).await?;
        }
        let run = validate_live(&transaction, anchor).await?;
        let (usage, latest_at) = read_run_usage(&transaction, anchor.worktree(), &run).await?;
        let deadline = run_deadline(&run)?;
        let admission_at = current_time(now, started)?;
        if now < run.updated_at() || latest_at.is_some_and(|latest| now < latest) {
            return Err(Failure::TimestampRegressed);
        }
        if admission_at >= deadline {
            return Err(Failure::DeadlineReached);
        }
        if usage.unobserved_calls != 0 || usage.unknown_calls != 0 {
            return Err(Failure::UncertainProviderCall);
        }
        check_call_kind(&transaction, anchor, job, kind, wire.digest()).await?;
        let supports_source = crate::migration::read_user_version(&transaction)
            .await
            .map_err(unavailable)?
            >= crate::source_read_repository::SCHEMA_VERSION;
        if supports_source {
            crate::source_read_repository::validate_reservation(
                &transaction,
                anchor,
                job,
                kind,
                source,
            )
            .await?;
        } else if source.is_some() {
            return Err(Failure::UnsupportedSchema);
        }
        check_budget(run.budget(), run.usage(), usage, kind, wire)?;
        let permit = EditPhaseReservation::issue(phase, kind, wire, now, deadline);
        let permit = match source {
            Some(source) => permit.with_source_read_key(source.key()),
            None => permit,
        };
        let permit = match compiled {
            Some(compiled) => permit.with_request_digest(compiled.request_digest()),
            None => permit,
        };
        insert(&transaction, &permit).await?;
        if let Some(compiled) = compiled {
            require_job_schema(&transaction).await?;
            let settings = compiled.request().profile().settings();
            transaction.execute("INSERT INTO edit_phase_jobs (run_id,job_digest,call_kind,request_digest,generation_cap,context_cap) VALUES(?1,?2,?3,?4,?5,?6)",params![
                anchor.run().as_bytes().to_vec(),job.as_bytes().to_vec(),call_kind_text(kind),
                compiled.request_digest().as_bytes().to_vec(),i64::from(settings.output_limit().get()),i64::from(settings.context_limit().get())
            ]).await.map_err(unavailable)?;
        }
        // Check after every admission write, immediately before the owning transaction commits.
        if current_time(now, started)? >= deadline { return Err(Failure::DeadlineReached); }
        Ok(permit)
    }
    .await;
    close(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
async fn finish<F, W>(
    connection: &Connection,
    reservation: &EditPhaseReservation,
    observation: EditPhaseObservation,
    compiled: Option<&CompiledEditPhaseRequest>,
    halt: Option<EditPhaseHalt>,
    started: Option<Instant>,
    after_writes: F,
) -> Result<(), Failure>
where
    F: FnOnce(&Transaction) -> W + Send,
    W: std::future::Future<Output = Result<(), Failure>> + Send,
{
    if compiled.is_some() && started.is_none() {
        return Err(Failure::InvalidStoredData);
    }
    let original_finished_at = observation.finished_at();
    if observation.finished_at() < reservation.reserved_at() {
        return Err(Failure::TimestampRegressed);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(unavailable)?;
    let result = async {
        check_source_permit(&transaction, reservation).await?;
        if halt.is_some() && observation.outcome() != EditPhaseObservationOutcome::AcceptedReply {
            return Err(Failure::ObservationConflict);
        }
        if let Some(compiled) = compiled { require_job(&transaction, reservation, compiled).await?; }
        else if crate::migration::read_user_version(&transaction).await.map_err(unavailable)? >= 70 {
            let mut rows = transaction.query("SELECT 1 FROM edit_phase_jobs WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",params![reservation.anchor().run().as_bytes().to_vec(),reservation.job_digest().as_bytes().to_vec(),call_kind_text(reservation.kind())]).await.map_err(unavailable)?;
            if rows.next().await.map_err(unavailable)?.is_some() { return Err(Failure::ObservationConflict); }
        }
        let scope_current = match compiled {
            Some(compiled) => match require_scope_index(&transaction, compiled).await {
                Ok(()) => None,
                Err(Failure::StaleAnchor) => Some(Failure::StaleAnchor),
                Err(error) => return Err(error),
            },
            None => None,
        };
        let observation = with_current_time(observation, started)?;
        let (mut outcome, mut reject) = if compiled.is_some() && reply_outcome(observation) && !known_terminal(observation) {
            ("unknown", Some(Failure::IncompleteProviderUsage))
        } else { classify_observation(&transaction, reservation, observation).await? };
        if let Some(stale) = scope_current
            && matches!(outcome, "accepted_reply" | "rejected_reply" | "invalid_structure")
        {
            outcome = "stale";
            reject = Some(stale);
        }
        let mut finished_at = observation.finished_at();
        if started.is_some() && finished_at >= reservation.deadline() && known_terminal(observation)
            && matches!(outcome, "accepted_reply" | "rejected_reply" | "invalid_structure") && reject.is_none()
        { outcome = "stale"; reject = Some(Failure::DeadlineReached); }
        transaction.execute("SAVEPOINT edit_terminal", ()).await.map_err(unavailable)?;
        if let Some(compiled) = compiled {
            if known_terminal(observation)
                && observation.reported_output_tokens().is_some_and(|n| n > compiled.request().profile().settings().output_limit().get()) {
                outcome = "unknown";
                reject = Some(Failure::BudgetExhausted(AgentBudgetDimension::OutputTokens));
            } else if known_terminal(observation)
                && observation.reported_prompt_tokens().is_some_and(|n| n > reservation.wire().prompt_estimate()) {
                // Every known V3 reply pays its actual prompt cost, including semantic
                // rejection and an AcceptedReply whose real Store I/O outlived the deadline.
                outcome = "budget_overrun";
                reject = Some(Failure::BudgetExhausted(AgentBudgetDimension::PromptTokens));
            }
            let retained = if outcome == "accepted_reply" && reject.is_none() { halt.map(halt_text) } else { None };
            let changed = transaction.execute("UPDATE edit_phase_jobs SET halt_kind=?1 WHERE run_id=?2 AND job_digest=?3 AND call_kind=?4",params![retained,reservation.anchor().run().as_bytes().to_vec(),reservation.job_digest().as_bytes().to_vec(),call_kind_text(reservation.kind())]).await.map_err(unavailable)?;
            if changed != 1 { return Err(Failure::ObservationConflict); }
        }
        let anchor = reservation.anchor();
        let wire = reservation.wire();
        let before_cas = current_time(original_finished_at, started)?;
        // The final check below is authoritative; this timestamp remains the observed owner clock.
        finished_at = finished_at.max(before_cas);
        let updated = transaction
            .execute(
                "UPDATE edit_phase_reservations
                 SET outcome = ?1, finished_at = ?2, reported_prompt = ?3, reported_output = ?4
                 WHERE worktree_id = ?5 AND run_id = ?6 AND job_digest = ?7 AND call_kind = ?8
                   AND wire_digest = ?9 AND wire_bytes = ?10 AND prompt_reserve = ?11
                   AND output_reserve = ?12 AND reserved_at = ?13 AND deadline = ?14
                   AND outcome IS NULL",
                params![
                    outcome,
                    stored_time(finished_at)?,
                    observation.reported_prompt_tokens().map(i64::from),
                    observation.reported_output_tokens().map(i64::from),
                    anchor.worktree().as_bytes().to_vec(),
                    anchor.run().as_bytes().to_vec(),
                    reservation.job_digest().as_bytes().to_vec(),
                    call_kind_text(reservation.kind()),
                    wire.digest().as_bytes().to_vec(),
                    i64::from(wire.request_bytes()),
                    i64::from(wire.prompt_estimate()),
                    i64::from(wire.output_reserve()),
                    stored_time(reservation.reserved_at())?,
                    stored_time(reservation.deadline())?,
                ],
            )
            .await
            .map_err(unavailable)?;
        if updated != 1 {
            return Err(Failure::ObservationConflict);
        }
        // Production is immediately ready. The single test callback may inspect these real
        // uncommitted Halt/Outcome writes, then await owned latency before this same clock check.
        after_writes(&transaction).await?;
        let last_at = current_time(original_finished_at, started)?;
        if started.is_some() && last_at >= reservation.deadline()
            && known_terminal(observation)
            && matches!(outcome, "accepted_reply" | "rejected_reply" | "invalid_structure")
            && reject.is_none()
        {
            // The SQL observation is append-once. Roll back only these uncommitted writes,
            // then make the one terminal observation stale without refunding the reservation.
            transaction.execute("ROLLBACK TO edit_terminal", ()).await.map_err(unavailable)?;
            transaction.execute("RELEASE edit_terminal", ()).await.map_err(unavailable)?;
            settle_stale(&transaction, reservation, observation, last_at).await?;
            reject = Some(Failure::DeadlineReached);
        }
        // COMMIT closes any still-owned savepoint; no asynchronous RELEASE follows
        // the final deadline check on a successful path.
        Ok(reject)
    }
    .await;
    close(transaction, result).await?.map_or(Ok(()), Err)
}

/// Advances an entry timestamp by the real elapsed time since the public Store call.
pub(crate) fn current_time(
    at: AgentRunTimestamp,
    started: Option<Instant>,
) -> Result<AgentRunTimestamp, Failure> {
    let Some(started) = started else {
        return Ok(at);
    };
    let elapsed =
        u64::try_from(started.elapsed().as_millis()).map_err(|_| Failure::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(
        at.unix_millis()
            .checked_add(elapsed)
            .ok_or(Failure::InvalidStoredData)?,
    )
    .map_err(|_| Failure::InvalidStoredData)
}
fn with_current_time(
    observation: EditPhaseObservation,
    started: Option<Instant>,
) -> Result<EditPhaseObservation, Failure> {
    Ok(EditPhaseObservation::new(
        observation.outcome(),
        observation.reported_prompt_tokens(),
        observation.reported_output_tokens(),
        current_time(observation.finished_at(), started)?,
    ))
}
fn reply_outcome(observation: EditPhaseObservation) -> bool {
    matches!(
        observation.outcome(),
        EditPhaseObservationOutcome::AcceptedReply
            | EditPhaseObservationOutcome::RejectedReply
            | EditPhaseObservationOutcome::InvalidStructure
            | EditPhaseObservationOutcome::NoChange
            | EditPhaseObservationOutcome::DeadlineReached
    )
}
pub(crate) fn known_terminal(observation: EditPhaseObservation) -> bool {
    reply_outcome(observation)
        && observation.reported_prompt_tokens().is_some_and(|n| n > 0)
        && observation.reported_output_tokens().is_some_and(|n| n > 0)
}
/// The caller has rolled back any uncommitted terminal writes before this single observation.
pub(crate) async fn settle_stale(
    tx: &Transaction,
    permit: &EditPhaseReservation,
    observation: EditPhaseObservation,
    at: AgentRunTimestamp,
) -> Result<(), Failure> {
    let changed = tx
        .execute(
            "UPDATE edit_phase_reservations SET outcome='stale',finished_at=?1,
        reported_prompt=?2,reported_output=?3 WHERE run_id=?4 AND job_digest=?5 AND call_kind=?6
        AND wire_digest=?7 AND reserved_at=?8 AND deadline=?9 AND outcome IS NULL",
            params![
                stored_time(at)?,
                observation.reported_prompt_tokens().map(i64::from),
                observation.reported_output_tokens().map(i64::from),
                permit.anchor().run().as_bytes().to_vec(),
                permit.job_digest().as_bytes().to_vec(),
                call_kind_text(permit.kind()),
                permit.wire().digest().as_bytes().to_vec(),
                stored_time(permit.reserved_at())?,
                stored_time(permit.deadline())?
            ],
        )
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(Failure::ObservationConflict);
    }
    Ok(())
}

pub(crate) async fn classify_observation(
    transaction: &Transaction,
    reservation: &EditPhaseReservation,
    observation: EditPhaseObservation,
) -> Result<(&'static str, Option<Failure>), Failure> {
    let success = match observation.outcome() {
        EditPhaseObservationOutcome::AcceptedReply => "accepted_reply",
        // Only the special atomic finish may attach the typed NoChange record.
        EditPhaseObservationOutcome::NoChange => return Err(Failure::UnsupportedSchema),
        EditPhaseObservationOutcome::InvalidStructure => "invalid_structure",
        EditPhaseObservationOutcome::DeadlineReached => {
            if !known_terminal(observation) {
                return Ok(("unknown", Some(Failure::IncompleteProviderUsage)));
            }
            if observation.finished_at() < reservation.deadline() {
                return Err(Failure::ObservationConflict);
            }
            // Known counter overruns remain budget failures even when the original
            // owner clock has also expired. Request/profile identity is not usage proof.
            if let Some(dimension) = wire_usage_overrun(reservation.wire(), observation) {
                return Ok(("budget_overrun", Some(Failure::BudgetExhausted(dimension))));
            }
            return Ok(("stale", Some(Failure::DeadlineReached)));
        }
        EditPhaseObservationOutcome::RejectedReply => return Ok(("rejected_reply", None)),
        EditPhaseObservationOutcome::ProviderFailure => return Ok(("provider_failure", None)),
        EditPhaseObservationOutcome::Cancelled => return Ok(("cancelled", None)),
        EditPhaseObservationOutcome::Unknown => return Ok(("unknown", None)),
    };
    let anchor = reservation.anchor();
    let run = match validate_live(transaction, anchor).await {
        Ok(run) => run,
        Err(error @ (Failure::StaleAnchor | Failure::MutationRecoveryRequired)) => {
            return Ok(("stale", Some(error)));
        }
        Err(error) => return Err(error),
    };
    if observation.finished_at() < run.updated_at() {
        return Ok(("stale", Some(Failure::TimestampRegressed)));
    }
    if observation.finished_at() >= reservation.deadline() {
        return Ok(("stale", Some(Failure::DeadlineReached)));
    }
    if observation
        .reported_prompt_tokens()
        .is_none_or(|tokens| tokens == 0)
        || observation
            .reported_output_tokens()
            .is_none_or(|tokens| tokens == 0)
    {
        return Ok(("unknown", Some(Failure::IncompleteProviderUsage)));
    }
    if let Some(dimension) = wire_usage_overrun(reservation.wire(), observation) {
        return Ok(("budget_overrun", Some(Failure::BudgetExhausted(dimension))));
    }
    Ok((success, None))
}

fn wire_usage_overrun(
    wire: EditPhaseWireCost,
    observation: EditPhaseObservation,
) -> Option<AgentBudgetDimension> {
    if observation
        .reported_prompt_tokens()
        .is_some_and(|reported| reported > wire.prompt_estimate())
    {
        Some(AgentBudgetDimension::PromptTokens)
    } else if observation
        .reported_output_tokens()
        .is_some_and(|reported| reported > wire.output_reserve())
    {
        Some(AgentBudgetDimension::OutputTokens)
    } else {
        None
    }
}

pub(crate) async fn validate_live(
    transaction: &Transaction,
    anchor: EditPhaseAnchor,
) -> Result<AgentRun, Failure> {
    let worktree = anchor.worktree();
    let run = load_run(transaction, worktree, anchor.run()).await?;
    if run.state() != AgentControllerState::Execute
        || run.goal_contract() != anchor.goal()
        || run.task_ledger_revision() != anchor.ledger()
        || run.last_event_sequence() != anchor.sequence()
        || run.current_snapshot_id() != anchor.snapshot()
        || run.model_profile() != Some(anchor.model_profile())
    {
        return Err(Failure::StaleAnchor);
    }
    let current_goal = goal_contract_repository::task_current_revision(
        transaction,
        worktree,
        anchor.goal().task_id(),
    )
    .await
    .map_err(|error| match error.classify() {
        a3_application::GoalContractStoreFailure::Unavailable => Failure::Unavailable,
        _ => Failure::InvalidStoredData,
    })?;
    if current_goal != Some(anchor.goal().revision()) {
        return Err(Failure::StaleAnchor);
    }
    let stored = task_ledger_repository::load_from_transaction(
        transaction,
        worktree,
        anchor.goal().task_id(),
    )
    .await
    .map_err(|error| match error.classify() {
        a3_application::TaskLedgerStoreFailure::Unavailable => Failure::Unavailable,
        _ => Failure::InvalidStoredData,
    })?
    .ok_or(Failure::StaleAnchor)?;
    let ledger = stored.ledger();
    if stored.version() != anchor.version()
        || ledger.goal_contract() != anchor.goal()
        || ledger.revision() != anchor.ledger()
        || ledger.pending_planning().is_some()
    {
        return Err(Failure::StaleAnchor);
    }
    let mut active = ledger
        .steps()
        .filter(|step| step.is_active_plan_step() && step.status() == TaskStepStatus::InProgress);
    let step = active.next().ok_or(Failure::StaleAnchor)?;
    if active.next().is_some()
        || step.definition().id() != anchor.step()
        || step.definition().verification_spec().id() != anchor.verification()
        || step
            .definition()
            .planned_file_addition()
            .map(a3_domain::PlannedFileAddition::scope_digest)
            != anchor.planned_file_addition_scope()
        || !step.attempts().last().is_some_and(|attempt| {
            attempt.run_id() == anchor.run()
                && attempt.number() == anchor.attempt()
                && matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
        })
    {
        return Err(Failure::StaleAnchor);
    }
    let mut publication = transaction
        .query(
            "WITH latest AS (
            SELECT index_run_id,snapshot_id FROM index_runs
            WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1
         )
         SELECT 1 FROM latest WHERE snapshot_id=?2
           AND EXISTS(SELECT 1 FROM file_revisions f
                      WHERE f.index_run_id=latest.index_run_id)=?3",
            params![
                worktree.as_bytes().to_vec(),
                anchor.snapshot().as_bytes().to_vec(),
                i64::from(u8::from(!anchor.empty_published_index()))
            ],
        )
        .await
        .map_err(unavailable)?;
    if publication.next().await.map_err(unavailable)?.is_none() {
        return Err(Failure::StaleAnchor);
    }
    crate::http_correction_plan_repository::no_change_window_index::guard_live(
        transaction,
        worktree,
        &run,
        &stored,
    )
    .await?;
    if agent_recovery_repository::has_unreconciled_mutation(transaction, worktree)
        .await
        .map_err(|_| Failure::Unavailable)?
    {
        return Err(Failure::MutationRecoveryRequired);
    }
    Ok(run)
}

async fn load_run(
    transaction: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
) -> Result<AgentRun, Failure> {
    run_journal_repository::load_run_from_transaction(transaction, worktree, run)
        .await
        .map_err(|error| match error.classify() {
            a3_application::RunJournalStoreFailure::Unavailable => Failure::Unavailable,
            _ => Failure::InvalidStoredData,
        })?
        .ok_or(Failure::StaleAnchor)
}

async fn check_call_kind(
    transaction: &Transaction,
    anchor: EditPhaseAnchor,
    job: ContentHash,
    kind: EditPhaseCallKind,
    wire_digest: ContentHash,
) -> Result<(), Failure> {
    let mut rows = transaction.query(
        "SELECT
          EXISTS(SELECT 1 FROM edit_phase_reservations
                 WHERE run_id = ?1 AND job_digest = ?2 AND call_kind = ?3),
          EXISTS(SELECT 1 FROM edit_phase_reservations
                 WHERE run_id = ?1 AND job_digest = ?2 AND call_kind = 'initial'
                   AND outcome = 'invalid_structure'),
          EXISTS(SELECT 1 FROM edit_phase_reservations initial
                 WHERE initial.run_id = ?1 AND initial.step_id = ?4 AND initial.attempt_number = ?5
                   AND initial.call_kind = 'initial' AND initial.outcome = 'invalid_structure'
                   AND NOT EXISTS (SELECT 1 FROM edit_phase_reservations repair
                       WHERE repair.run_id = initial.run_id AND repair.job_digest = initial.job_digest
                         AND repair.call_kind = 'structural_repair'
                         AND repair.outcome IN ('accepted_reply', 'rejected_reply'))),
          EXISTS(SELECT 1 FROM edit_phase_reservations
                 WHERE run_id = ?1 AND job_digest = ?2 AND call_kind = 'initial'
                   AND wire_digest = ?6)",
        params![
            anchor.run().as_bytes().to_vec(), job.as_bytes().to_vec(), call_kind_text(kind),
            anchor.step().as_bytes().to_vec(), i64::from(anchor.attempt().get()),
            wire_digest.as_bytes().to_vec(),
        ],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::InvalidStoredData)?;
    if nonnegative(&row, 0)? != 0 {
        return Err(Failure::AlreadyReserved);
    }
    match kind {
        EditPhaseCallKind::StructuralRepair if nonnegative(&row, 1)? == 0 => {
            Err(Failure::RepairNotAvailable)
        }
        EditPhaseCallKind::Initial if nonnegative(&row, 2)? != 0 => {
            Err(Failure::StructuralFailurePending)
        }
        EditPhaseCallKind::StructuralRepair if nonnegative(&row, 3)? != 0 => {
            Err(Failure::UnchangedRepairWire)
        }
        _ => Ok(()),
    }
}

async fn read_usage(
    transaction: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
) -> Result<(EditPhaseReservedUsage, Option<AgentRunTimestamp>), Failure> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*), COUNT(CASE WHEN call_kind = 'structural_repair' THEN 1 END),
          COALESCE(SUM(MAX(prompt_reserve, COALESCE(reported_prompt, 0))), 0),
          COALESCE(SUM(MAX(output_reserve, COALESCE(reported_output, 0))), 0),
          COUNT(CASE WHEN outcome IS NULL THEN 1 END),
          COUNT(CASE WHEN outcome = 'unknown' THEN 1 END),
          MAX(COALESCE(finished_at, reserved_at))
         FROM edit_phase_reservations WHERE worktree_id = ?1 AND run_id = ?2",
            params![worktree.as_bytes().to_vec(), run.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::InvalidStoredData)?;
    let usage = EditPhaseReservedUsage {
        calls: count(&row, 0)?,
        repairs: count(&row, 1)?,
        prompt_tokens: nonnegative(&row, 2)?,
        output_tokens: nonnegative(&row, 3)?,
        unobserved_calls: count(&row, 4)?,
        unknown_calls: count(&row, 5)?,
    };
    let latest: Option<i64> = row.get(6).map_err(|_| Failure::InvalidStoredData)?;
    let latest = latest
        .map(|value| {
            let value = u64::try_from(value).map_err(|_| Failure::InvalidStoredData)?;
            AgentRunTimestamp::from_unix_millis(value).map_err(|_| Failure::InvalidStoredData)
        })
        .transpose()?;
    Ok((usage, latest))
}

pub(crate) async fn read_run_usage(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
) -> Result<(EditPhaseReservedUsage, Option<AgentRunTimestamp>), Failure> {
    let (mut usage, latest) = read_usage(tx, worktree, run.id()).await?;
    let (correction, correction_at) =
        crate::http_correction_plan_repository::usage(tx, worktree, run.id()).await?;
    usage = merge_phase_usage(usage, correction)?;
    let (context, context_at) =
        crate::http_context_intent_repository::usage(tx, worktree, run.id()).await?;
    usage = merge_phase_usage(usage, context)?;
    let (no_change_replan, no_change_replan_at) =
        crate::http_correction_plan_repository::no_change_replan::usage(tx, worktree, run.id())
            .await?;
    usage = merge_phase_usage(usage, no_change_replan)?;
    let latest = latest
        .into_iter()
        .chain(correction_at)
        .chain(context_at)
        .chain(no_change_replan_at)
        .max();
    if let Some(pre_run) =
        goal_contract_repository::phase_reservations::linked_usage(tx, worktree, run).await?
    {
        usage = merge_phase_usage(usage, pre_run)?;
    }
    Ok((usage, latest))
}

pub(crate) fn merge_phase_usage(
    mut usage: EditPhaseReservedUsage,
    next: EditPhaseReservedUsage,
) -> Result<EditPhaseReservedUsage, Failure> {
    usage.calls = usage
        .calls
        .checked_add(next.calls)
        .ok_or(Failure::InvalidStoredData)?;
    usage.repairs = usage
        .repairs
        .checked_add(next.repairs)
        .ok_or(Failure::InvalidStoredData)?;
    usage.prompt_tokens = usage
        .prompt_tokens
        .checked_add(next.prompt_tokens)
        .ok_or(Failure::InvalidStoredData)?;
    usage.output_tokens = usage
        .output_tokens
        .checked_add(next.output_tokens)
        .ok_or(Failure::InvalidStoredData)?;
    usage.unobserved_calls = usage
        .unobserved_calls
        .checked_add(next.unobserved_calls)
        .ok_or(Failure::InvalidStoredData)?;
    usage.unknown_calls = usage
        .unknown_calls
        .checked_add(next.unknown_calls)
        .ok_or(Failure::InvalidStoredData)?;
    Ok(usage)
}

pub(crate) fn check_budget(
    budget: AgentRunBudget,
    used: AgentRunUsage,
    stored: EditPhaseReservedUsage,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
) -> Result<(), Failure> {
    for (dimension, previous, reserved, next, limit) in [
        (
            AgentBudgetDimension::Turns,
            u64::from(used.turn_count()),
            u64::from(stored.calls),
            1,
            u64::from(budget.turn_limit().get()),
        ),
        (
            AgentBudgetDimension::PromptTokens,
            used.prompt_tokens(),
            stored.prompt_tokens,
            u64::from(wire.prompt_estimate()),
            budget.prompt_token_limit().get(),
        ),
        (
            AgentBudgetDimension::OutputTokens,
            used.output_tokens(),
            stored.output_tokens,
            u64::from(wire.output_reserve()),
            budget.output_token_limit().get(),
        ),
        (
            AgentBudgetDimension::Repairs,
            u64::from(used.repair_count()),
            u64::from(stored.repairs),
            u64::from(kind == EditPhaseCallKind::StructuralRepair),
            u64::from(budget.repair_limit().get()),
        ),
    ] {
        if previous.saturating_add(reserved).saturating_add(next) > limit {
            return Err(Failure::BudgetExhausted(dimension));
        }
    }
    Ok(())
}

async fn insert(transaction: &Transaction, permit: &EditPhaseReservation) -> Result<(), Failure> {
    let anchor = permit.anchor();
    let wire = permit.wire();
    let values = params![
        anchor.worktree().as_bytes().to_vec(),
        anchor.run().as_bytes().to_vec(),
        permit.job_digest().as_bytes().to_vec(),
        call_kind_text(permit.kind()),
        anchor.goal().task_id().as_bytes().to_vec(),
        i64::from(anchor.goal().revision().get()),
        i64::from(anchor.ledger().get()),
        i64::try_from(anchor.version().get()).map_err(|_| Failure::InvalidStoredData)?,
        i64::try_from(anchor.sequence().get()).map_err(|_| Failure::InvalidStoredData)?,
        anchor.step().as_bytes().to_vec(),
        i64::from(anchor.attempt().get()),
        anchor.verification().as_bytes().to_vec(),
        anchor.snapshot().as_bytes().to_vec(),
        wire.digest().as_bytes().to_vec(),
        i64::from(wire.request_bytes()),
        i64::from(wire.prompt_estimate()),
        i64::from(wire.output_reserve()),
        stored_time(permit.reserved_at())?,
        stored_time(permit.deadline())?,
    ];
    let columns = "worktree_id, run_id, job_digest, call_kind, task_id,
           goal_revision, ledger_revision, ledger_store_version, run_sequence, step_id,
           attempt_number, verification_id, snapshot_id, wire_digest, wire_bytes, prompt_reserve,
           output_reserve, reserved_at, deadline";
    let parameters =
        "?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19";
    let updated = match permit.source_read_key() {
        Some(source) => transaction.execute(
            &format!("INSERT INTO edit_phase_reservations ({columns}, source_read_id) VALUES ({parameters}, ?20)"),
            params_from_iter(values.into_iter().chain([Ok(Value::Blob(source.as_bytes().to_vec()))])),
        ).await,
        None => transaction.execute(
            &format!("INSERT INTO edit_phase_reservations ({columns}) VALUES ({parameters})"),
            values,
        ).await,
    }.map_err(unavailable)?;
    if updated != 1 {
        return Err(Failure::InvalidStoredData);
    }
    Ok(())
}

async fn check_source_permit(
    tx: &Transaction,
    permit: &EditPhaseReservation,
) -> Result<(), Failure> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < crate::source_read_repository::SCHEMA_VERSION
    {
        return if permit.source_read_key().is_none() {
            Ok(())
        } else {
            Err(Failure::UnsupportedSchema)
        };
    }
    let mut rows = tx
        .query(
            "SELECT source_read_id FROM edit_phase_reservations
         WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",
            params![
                permit.anchor().run().as_bytes().to_vec(),
                permit.job_digest().as_bytes().to_vec(),
                call_kind_text(permit.kind()),
            ],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::ObservationConflict)?;
    let matches = match row.get_value(0).map_err(unavailable)? {
        Value::Null => permit.source_read_key().is_none(),
        Value::Blob(key) => permit
            .source_read_key()
            .is_some_and(|expected| key.as_slice() == expected.as_bytes()),
        _ => return Err(Failure::InvalidStoredData),
    };
    if matches {
        Ok(())
    } else {
        Err(Failure::ObservationConflict)
    }
}

pub(crate) fn run_deadline(run: &AgentRun) -> Result<AgentRunTimestamp, Failure> {
    let deadline = run
        .created_at()
        .unix_millis()
        .checked_add(run.budget().duration_limit().millis())
        .ok_or(Failure::InvalidStoredData)?;
    AgentRunTimestamp::from_unix_millis(deadline).map_err(|_| Failure::InvalidStoredData)
}

pub(crate) const fn call_kind_text(kind: EditPhaseCallKind) -> &'static str {
    match kind {
        EditPhaseCallKind::Initial => "initial",
        EditPhaseCallKind::StructuralRepair => "structural_repair",
    }
}

fn stored_time(value: AgentRunTimestamp) -> Result<i64, Failure> {
    i64::try_from(value.unix_millis()).map_err(|_| Failure::InvalidStoredData)
}

fn nonnegative(row: &Row, index: i32) -> Result<u64, Failure> {
    let value: i64 = row.get(index).map_err(|_| Failure::InvalidStoredData)?;
    u64::try_from(value).map_err(|_| Failure::InvalidStoredData)
}

fn count(row: &Row, index: i32) -> Result<u32, Failure> {
    u32::try_from(nonnegative(row, index)?).map_err(|_| Failure::InvalidStoredData)
}

fn unavailable(_: libsql::Error) -> Failure {
    Failure::Unavailable
}

async fn close<T>(transaction: Transaction, result: Result<T, Failure>) -> Result<T, Failure> {
    match result {
        Ok(value) => {
            transaction.commit().await.map_err(unavailable)?;
            Ok(value)
        }
        Err(error) => {
            transaction.rollback().await.map_err(unavailable)?;
            Err(error)
        }
    }
}

// V3 stores only closed request dispositions. Raw provider text is never persisted.
async fn require_job_schema(tx: &Transaction) -> Result<(), Failure> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < 70
    {
        return Err(Failure::UnsupportedSchema);
    }
    Ok(())
}
/// Request fingerprints cannot replace actual current publication authentication.
async fn require_scope_index(
    tx: &Transaction,
    compiled: &CompiledEditPhaseRequest,
) -> Result<(), Failure> {
    let source = compiled.source_publication();
    match (compiled.scope_frame_digest(), compiled.scope_frame_index()) {
        (None, None) => {}
        (Some(_), Some(index)) if Some(index) == source => {}
        _ => return Err(Failure::InvalidStoredData),
    }
    require_publication(tx, compiled.anchor(), source).await
}

async fn require_publication(
    tx: &Transaction,
    a: a3_application::EditPhaseAnchor,
    index: Option<a3_domain::IndexRunId>,
) -> Result<(), Failure> {
    let Some(index) = index else {
        return Ok(());
    };
    let mut rows = tx.query("SELECT 1 WHERE ?2=(SELECT index_run_id FROM index_runs
        WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1)
        AND ?3=(SELECT snapshot_id FROM index_runs WHERE index_run_id=?2 AND worktree_id=?1 AND status='published')",
        params![a.worktree().as_bytes().to_vec(),index.as_bytes().to_vec(),a.snapshot().as_bytes().to_vec()])
        .await.map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(Failure::StaleAnchor);
    }
    Ok(())
}

async fn require_job(
    tx: &Transaction,
    permit: &EditPhaseReservation,
    compiled: &CompiledEditPhaseRequest,
) -> Result<(), Failure> {
    require_job_schema(tx).await?;
    check_profile_wire(permit.wire(), compiled.request().profile())?;
    if compiled.anchor() != permit.anchor()
        || compiled.job_digest() != permit.job_digest()
        || compiled.kind() != permit.kind()
        || compiled.request().profile().reference() != permit.anchor().model_profile()
        || compiled.acquired_source().map(AcquiredSourceRead::key) != permit.source_read_key()
        || permit.request_digest() != Some(compiled.request_digest())
    {
        return Err(Failure::ObservationConflict);
    }
    require_request(tx, compiled).await
}
/// The actual recorded request identity, not a parent-pass or a reconstructed model request.
pub(crate) async fn recorded_request_digest(
    tx: &Transaction,
    run: AgentRunId,
    job: ContentHash,
    kind: EditPhaseCallKind,
) -> Result<Option<ContentHash>, Failure> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < 70
    {
        return Ok(None);
    }
    let mut rows = tx.query("SELECT request_digest FROM edit_phase_jobs WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",
        params![run.as_bytes().to_vec(),job.as_bytes().to_vec(),call_kind_text(kind)]).await.map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    let bytes: Vec<u8> = row.get(0).map_err(|_| Failure::InvalidStoredData)?;
    let digest = ContentHash::from_bytes(bytes.try_into().map_err(|_| Failure::InvalidStoredData)?);
    if rows.next().await.map_err(unavailable)?.is_some() {
        return Err(Failure::InvalidStoredData);
    }
    Ok(Some(digest))
}

async fn require_request(
    tx: &Transaction,
    compiled: &CompiledEditPhaseRequest,
) -> Result<(), Failure> {
    let a = compiled.anchor();
    let mut rows=tx.query("SELECT j.request_digest,j.generation_cap,j.context_cap FROM edit_phase_jobs j JOIN edit_phase_reservations r USING(run_id,job_digest,call_kind)
        WHERE r.run_id=?1 AND r.job_digest=?2 AND r.call_kind=?3 AND r.worktree_id=?4 AND r.task_id=?5
        AND r.goal_revision=?6 AND r.ledger_revision=?7 AND r.ledger_store_version=?8 AND r.run_sequence=?9
        AND r.step_id=?10 AND r.attempt_number=?11 AND r.verification_id=?12 AND r.snapshot_id=?13",params![
        a.run().as_bytes().to_vec(),compiled.job_digest().as_bytes().to_vec(),call_kind_text(compiled.kind()),
        a.worktree().as_bytes().to_vec(),a.goal().task_id().as_bytes().to_vec(),i64::from(a.goal().revision().get()),i64::from(a.ledger().get()),
        i64::try_from(a.version().get()).map_err(|_|Failure::InvalidStoredData)?,i64::try_from(a.sequence().get()).map_err(|_|Failure::InvalidStoredData)?,
        a.step().as_bytes().to_vec(),i64::from(a.attempt().get()),a.verification().as_bytes().to_vec(),a.snapshot().as_bytes().to_vec()
    ]).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::ObservationConflict)?;
    let digest: Vec<u8> = row.get(0).map_err(|_| Failure::InvalidStoredData)?;
    let settings = compiled.request().profile().settings();
    if digest.as_slice() != compiled.request_digest().as_bytes()
        || count(&row, 1)? != settings.output_limit().get()
        || count(&row, 2)? != settings.context_limit().get()
    {
        return Err(Failure::ObservationConflict);
    }
    Ok(())
}

fn halt_text(halt: EditPhaseHalt) -> &'static str {
    match halt {
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Definition) => "definition",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Caller) => "caller",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test) => "test",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Unknown) => "unknown_need",
        EditPhaseHalt::TooLarge => "too_large",
    }
}
fn halt_from_text(kind: &str) -> Result<EditPhaseHalt, Failure> {
    Ok(match kind {
        "definition" => EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Definition),
        "caller" => EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Caller),
        "test" => EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test),
        "unknown_need" => EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Unknown),
        "too_large" => EditPhaseHalt::TooLarge,
        _ => return Err(Failure::InvalidStoredData),
    })
}
async fn load_halt(
    connection: &Connection,
    compiled: &CompiledEditPhaseRequest,
) -> Result<Option<EditPhaseHaltReceipt>, Failure> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(unavailable)?;
    let result=async{
        require_job_schema(&tx).await?;
        let a=compiled.anchor();
        // This is an observation of the exact still-bound job, never a permission to execute.
        let run=validate_live(&tx,a).await?;
        require_scope_index(&tx,compiled).await?;
        let mut rows=tx.query("SELECT j.halt_kind,r.wire_digest,r.wire_bytes,r.prompt_reserve,r.output_reserve,r.reserved_at,r.deadline,r.finished_at,r.reported_prompt,r.reported_output,r.source_read_id,r.outcome,r.terminal_reply_kind FROM edit_phase_jobs j JOIN edit_phase_reservations r USING(run_id,job_digest,call_kind) WHERE j.run_id=?1 AND j.job_digest=?2 AND j.call_kind=?3",params![a.run().as_bytes().to_vec(),compiled.job_digest().as_bytes().to_vec(),call_kind_text(compiled.kind())]).await.map_err(unavailable)?;
        let Some(row)=rows.next().await.map_err(unavailable)? else{return Ok(None)};
        let kind:Option<String>=row.get(0).map_err(|_|Failure::InvalidStoredData)?;
        let Some(kind)=kind else{return Ok(None)};
        let digest:Vec<u8>=row.get(1).map_err(|_|Failure::InvalidStoredData)?;
        let digest=ContentHash::from_bytes(digest.try_into().map_err(|_|Failure::InvalidStoredData)?);
        let wire=EditPhaseWireCost::new(digest,count(&row,2)?,count(&row,3)?,count(&row,4)?)?;
        check_profile_wire(wire, compiled.request().profile())?;
        let time=|column|AgentRunTimestamp::from_unix_millis(nonnegative(&row,column)?).map_err(|_|Failure::InvalidStoredData);
        let reserved=time(5)?;let deadline=time(6)?;let finished=time(7)?;
        let source:Option<Vec<u8>>=row.get(10).map_err(|_|Failure::InvalidStoredData)?;
        let expected=compiled.acquired_source().map(AcquiredSourceRead::key);
        if source.as_deref()!=expected.as_ref().map(|h|h.as_bytes().as_slice()) {return Err(Failure::InvalidStoredData)}
        let stored_outcome:String=row.get(11).map_err(|_|Failure::InvalidStoredData)?;
        let no_change:Option<String>=row.get(12).map_err(|_|Failure::InvalidStoredData)?;
        let settings=compiled.request().profile().settings();
        if stored_outcome!="accepted_reply" || no_change.is_some() || finished<reserved || finished>=deadline
            || deadline!=run_deadline(&run)? || count(&row,8)?==0 || count(&row,8)?>wire.prompt_estimate()
            || count(&row,9)?==0 || count(&row,9)?>wire.output_reserve() || count(&row,9)?>settings.output_limit().get() {
            return Err(Failure::InvalidStoredData);
        }
        // Use the stored shape for authentication; rebuilding an input/job is not enough.
        require_request(&tx,compiled).await?;
        Ok(Some(EditPhaseHaltReceipt::from_recorded(halt_from_text(&kind)?,compiled.request_digest(),wire,finished)))
    }.await;
    close(tx, result).await
}

fn check_profile_wire(wire: EditPhaseWireCost, profile: &ModelProfile) -> Result<(), Failure> {
    let settings = profile.settings();
    let context = settings.context_limit().get();
    let expected = u32::try_from((u64::from(context) * 22).div_ceil(100))
        .map_err(|_| Failure::InvalidWireCost)?
        .max(settings.output_limit().get());
    if !profile.executable_actions_enabled()
        || wire.output_reserve() != expected
        || wire.prompt_estimate() < wire.request_bytes()
        || wire
            .prompt_estimate()
            .checked_add(wire.output_reserve())
            .is_none_or(|sum| sum > context)
    {
        return Err(Failure::InvalidWireCost);
    }
    Ok(())
}
