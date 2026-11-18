//! Current-run Plan debit and immutable non-executable proposal; no Ledger writes.
use crate::{
    LibsqlKnowledgeStore, agent_recovery_repository as recovery,
    edit_phase_reservation_repository as edit, goal_contract_repository as goals,
    run_journal_repository as journal, task_ledger_repository as ledgers,
};
use a3_application::{
    EditPhaseCallKind, EditPhaseObservation, EditPhaseObservationOutcome as Outcome,
    EditPhaseReservationFailure as E, EditPhaseReservationFuture, EditPhaseReservedUsage,
    EditPhaseWireCost, HttpCorrectionPlanAnchor, HttpCorrectionPlanProposal,
    HttpCorrectionPlanReservation, HttpCorrectionPlanStore, PreparedHttpCorrectionPlan,
};
use a3_domain::*;
use libsql::{Connection, Row, Transaction, TransactionBehavior, params};

#[path = "http_correction_anchor_codec.rs"]
mod anchor_codec;
#[path = "http_correction_origin_repository.rs"]
mod origin;

#[path = "http_closed_no_change_repository.rs"]
pub(crate) mod closed_no_change;

#[path = "http_no_change_replan_repository.rs"]
pub(crate) mod no_change_replan;
#[path = "http_no_change_window_commit_repository.rs"]
pub(crate) mod no_change_window_commit;
#[path = "http_no_change_window_index_repository.rs"]
pub(crate) mod no_change_window_index;
#[path = "http_no_change_window_origin_repository.rs"]
pub(crate) mod no_change_window_origin;

const VERSION: u32 = 66;
impl HttpCorrectionPlanStore for LibsqlKnowledgeStore {
    fn load_http_no_change_replan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a a3_application::StoredTaskLedger,
        run: &'a AgentRun,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<a3_application::HttpNoChangeReplanResume>> {
        no_change_replan::load(
            self,
            origin::OriginFrame {
                project,
                goal,
                stored,
                run,
                current,
                profile,
                now,
            },
        )
    }
    fn reserve_http_no_change_replan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a a3_application::PreparedHttpNoChangeReplanV6Job,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, HttpCorrectionPlanReservation> {
        no_change_replan::reserve(self, project, phase, kind, wire, now)
    }
    fn finish_http_no_change_replan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a a3_application::PreparedHttpNoChangeReplanV6Job,
        permit: HttpCorrectionPlanReservation,
        observation: EditPhaseObservation,
        decision: Option<a3_application::HttpNoChangeReplanDecision>,
    ) -> EditPhaseReservationFuture<'a, a3_application::HttpNoChangeReplanSettlement> {
        no_change_replan::finish(self, project, phase, permit, observation, decision)
    }

    fn load_applied_http_no_change_window_origin<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a a3_application::StoredTaskLedger,
        run: &'a AgentRun,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, a3_application::AppliedHttpNoChangeWindowOrigin> {
        no_change_window_origin::load(
            self,
            origin::OriginFrame {
                project,
                goal,
                stored,
                run,
                current,
                profile,
                now,
            },
        )
    }

    fn reserve_http_correction_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpCorrectionPlan,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, HttpCorrectionPlanReservation> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = async {
                let run = validate(&tx, project.worktree().id(), phase).await?;
                let (usage, latest) =
                    edit::read_run_usage(&tx, project.worktree().id(), &run).await?;
                if now < run.updated_at() || latest.is_some_and(|latest| now < latest) {
                    return Err(E::TimestampRegressed);
                }
                let deadline = edit::run_deadline(&run)?;
                if now >= deadline {
                    return Err(E::DeadlineReached);
                }
                if usage.unobserved_calls != 0 || usage.unknown_calls != 0 {
                    return Err(E::UncertainProviderCall);
                }
                check_kind(&tx, phase, kind, wire).await?;
                edit::check_budget(run.budget(), run.usage(), usage, kind, wire)?;
                let a = phase.anchor();
                tx.execute(
                    "INSERT INTO http_correction_plan_reservations(
                     worktree_id,run_id,diagnostic_digest,tool_run_id,tool_attempt,job_digest,
                     call_kind,anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,
                     reserved_at,deadline,protocol_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,4)",
                    params![
                        a.worktree.as_bytes().to_vec(),
                        a.run.as_bytes().to_vec(),
                        a.diagnostic.as_bytes().to_vec(),
                        a.tool.as_bytes().to_vec(),
                        i64::from(a.tool_attempt.get()),
                        phase.job_digest().as_bytes().to_vec(),
                        edit::call_kind_text(kind),
                        anchor_bytes(a),
                        wire.digest().as_bytes().to_vec(),
                        i64::from(wire.request_bytes()),
                        i64::from(wire.prompt_estimate()),
                        i64::from(wire.output_reserve()),
                        time(now)?,
                        time(deadline)?
                    ],
                )
                .await
                .map_err(unavailable)?;
                Ok(HttpCorrectionPlanReservation {
                    job: phase.job_digest(),
                    kind,
                    wire,
                    reserved_at: now,
                    deadline,
                })
            }
            .await;
            close(tx, result).await
        })
    }
    fn finish_http_correction_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpCorrectionPlan,
        permit: HttpCorrectionPlanReservation,
        observation: EditPhaseObservation,
        proposal: Option<HttpCorrectionPlanProposal>,
    ) -> EditPhaseReservationFuture<'a, Option<HttpCorrectionPlanProposal>> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = async {
                if permit.job != phase.job_digest()
                    || observation.finished_at() < permit.reserved_at
                {
                    return Err(E::ObservationConflict);
                }
                let current = validate(&tx, project.worktree().id(), phase).await;
                let (mut outcome, mut rejection) = match observation.outcome() {
                    Outcome::AcceptedReply => ("accepted_reply", None),
                    Outcome::NoChange | Outcome::DeadlineReached => {
                        return Err(E::InvalidStoredData);
                    }
                    Outcome::InvalidStructure => ("invalid_structure", None),
                    Outcome::RejectedReply => ("rejected_reply", None),
                    Outcome::ProviderFailure => ("provider_failure", None),
                    Outcome::Cancelled => ("cancelled", None),
                    Outcome::Unknown => ("unknown", None),
                };
                match current {
                    Ok(ref run) if observation.finished_at() < run.updated_at() => {
                        if outcome != "unknown" {
                            outcome = "stale";
                        }
                        rejection = Some(E::TimestampRegressed);
                    }
                    Err(error @ (E::StaleAnchor | E::MutationRecoveryRequired)) => {
                        if outcome != "unknown" {
                            outcome = "stale";
                        }
                        rejection = Some(error);
                    }
                    Err(error) => return Err(error),
                    _ => {}
                }
                if observation.finished_at() >= permit.deadline {
                    if outcome != "unknown" {
                        outcome = "stale";
                    }
                    rejection = Some(E::DeadlineReached);
                }
                let prompt = observation.reported_prompt_tokens();
                let output = observation.reported_output_tokens();
                if prompt.is_some_and(|n| n > permit.wire.prompt_estimate()) {
                    outcome = "budget_overrun";
                    rejection = Some(E::BudgetExhausted(AgentBudgetDimension::PromptTokens));
                } else if output.is_some_and(|n| n > permit.wire.output_reserve()) {
                    outcome = "budget_overrun";
                    rejection = Some(E::BudgetExhausted(AgentBudgetDimension::OutputTokens));
                } else if matches!(outcome, "accepted_reply" | "invalid_structure")
                    && (prompt.is_none_or(|v| v == 0) || output.is_none_or(|v| v == 0))
                {
                    outcome = "unknown";
                    rejection = Some(E::IncompleteProviderUsage);
                }
                let encoded = if outcome == "accepted_reply" {
                    Some(
                        phase
                            .encode_proposal(proposal.as_ref().ok_or(E::InvalidStoredData)?)
                            .map_err(|_| E::InvalidStoredData)?,
                    )
                } else {
                    None
                };
                let digest = encoded
                    .as_ref()
                    .map(|bytes| blake3::hash(bytes).as_bytes().to_vec());
                let a = phase.anchor();
                let changed = tx
                    .execute(
                        "UPDATE http_correction_plan_reservations SET outcome=?1,finished_at=?2,
                     reported_prompt=?3,reported_output=?4,proposal=?5,proposal_digest=?6
                     WHERE run_id=?7 AND diagnostic_digest=?8 AND call_kind=?9 AND job_digest=?10
                     AND anchor=?11 AND wire_digest=?12 AND wire_bytes=?13 AND prompt_reserve=?14
                     AND output_reserve=?15 AND reserved_at=?16 AND deadline=?17 AND outcome IS NULL
                     AND protocol_version=4",
                        params![
                            outcome,
                            time(observation.finished_at())?,
                            prompt.map(i64::from),
                            output.map(i64::from),
                            encoded,
                            digest,
                            a.run.as_bytes().to_vec(),
                            a.diagnostic.as_bytes().to_vec(),
                            edit::call_kind_text(permit.kind),
                            permit.job.as_bytes().to_vec(),
                            anchor_bytes(a),
                            permit.wire.digest().as_bytes().to_vec(),
                            i64::from(permit.wire.request_bytes()),
                            i64::from(permit.wire.prompt_estimate()),
                            i64::from(permit.wire.output_reserve()),
                            time(permit.reserved_at)?,
                            time(permit.deadline)?
                        ],
                    )
                    .await
                    .map_err(unavailable)?;
                if changed != 1 {
                    return Err(E::ObservationConflict);
                }
                Ok((
                    if outcome == "accepted_reply" {
                        proposal
                    } else {
                        None
                    },
                    rejection,
                ))
            }
            .await;
            let (proposal, rejection) = close(tx, result).await?;
            match rejection {
                Some(error) => Err(error),
                None => Ok(proposal),
            }
        })
    }
    fn load_http_correction_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpCorrectionPlan,
    ) -> EditPhaseReservationFuture<'a, Option<HttpCorrectionPlanProposal>> {
        load_checked(self, project, phase, None)
    }
    fn load_http_correction_plan_for_application<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpCorrectionPlan,
        observed_at: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<HttpCorrectionPlanProposal>> {
        load_checked(self, project, phase, Some(observed_at))
    }

    fn admit_http_context_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpCorrectionPlan,
        source: &'a a3_application::BoundEditSource,
        tool: ToolRunId,
        now: AgentRunTimestamp,
    ) -> a3_application::SourceReadFuture<'a, a3_application::SourceReadAdmission> {
        crate::http_context_intent_repository::admit(self, project, phase, source, tool, now)
    }

    fn load_http_context_read_origin<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a a3_application::StoredTaskLedger,
        run: &'a AgentRun,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, a3_application::HttpContextReadOrigin> {
        crate::http_context_intent_repository::read_origin(
            self, project, goal, stored, run, current, profile, now,
        )
    }

    fn reserve_http_context_intent<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a a3_application::PreparedHttpContextIntent,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, a3_application::HttpContextIntentReservation> {
        crate::http_context_intent_repository::reserve(self, project, phase, kind, wire, now)
    }

    fn finish_http_context_intent<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a a3_application::PreparedHttpContextIntent,
        permit: a3_application::HttpContextIntentReservation,
        observation: EditPhaseObservation,
        reply: Option<a3_application::HttpContextIntentReply>,
    ) -> EditPhaseReservationFuture<'a, Option<a3_application::HttpContextIntentReply>> {
        crate::http_context_intent_repository::finish(
            self,
            project,
            phase,
            permit,
            observation,
            reply,
        )
    }

    fn load_http_context_intent_for_application<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a a3_application::PreparedHttpContextIntent,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<a3_application::HttpContextIntentSettlement>> {
        crate::http_context_intent_repository::load(self, project, phase, now)
    }

    fn load_closed_no_change_http_origin<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a a3_application::StoredTaskLedger,
        run: &'a AgentRun,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, Option<a3_application::ClosedHttpNoChangeOrigin>> {
        closed_no_change::load(
            self,
            origin::OriginFrame {
                project,
                goal,
                stored,
                run,
                current,
                profile,
                now,
            },
        )
    }

    fn load_applied_http_correction_origin<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a a3_application::StoredTaskLedger,
        run: &'a AgentRun,
        current: &'a PublishedIndex,
        profile: &'a ModelProfile,
        now: AgentRunTimestamp,
    ) -> EditPhaseReservationFuture<'a, a3_application::HttpCorrectionOrigin> {
        origin::load(self, project, goal, stored, run, current, profile, now)
    }
}

fn load_checked<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpCorrectionPlan,
    observed_at: Option<AgentRunTimestamp>,
) -> EditPhaseReservationFuture<'a, Option<HttpCorrectionPlanProposal>> {
    Box::pin(async move {
        let connection = open(store, project).await?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(unavailable)?;
        let result = async {
                let run = validate(&tx, project.worktree().id(), phase).await?;
                let a = phase.anchor();
                let mut rows = tx.query(
                    "SELECT job_digest,
                     CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 256 AND 133000 THEN anchor ELSE x'' END,
                     CASE WHEN typeof(proposal)='blob' AND length(proposal) BETWEEN 1 AND 512 THEN proposal ELSE x'' END,
                     proposal_digest,reported_prompt,reported_output,prompt_reserve,output_reserve,
                     reserved_at,finished_at,deadline,call_kind,wire_digest,worktree_id,tool_run_id,
                     tool_attempt,protocol_version FROM http_correction_plan_reservations
                     WHERE run_id=?1 AND diagnostic_digest=?2 AND outcome='accepted_reply'
                     ORDER BY CASE call_kind WHEN 'structural_repair' THEN 0 ELSE 1 END",
                    params![a.run.as_bytes().to_vec(), a.diagnostic.as_bytes().to_vec()],
                ).await.map_err(unavailable)?;
                let Some(row) = rows.next().await.map_err(unavailable)? else { return Ok(None); };
                let bytes: Vec<u8> = row.get(2).map_err(unavailable)?;
                let kind: String = row.get(11).map_err(|_| E::InvalidStoredData)?;
                let wire = blob(&row, 12)?;
                let reserved_at = n(&row, 8)?;
                let deadline = n(&row, 10)?;
                if blob(&row, 0)? != phase.job_digest().as_bytes()
                    || blob(&row, 1)? != anchor_bytes(a)
                    || blob(&row, 3)? != blake3::hash(&bytes).as_bytes()
                    || n(&row, 4)? == 0 || n(&row, 5)? == 0
                    || n(&row, 4)? > n(&row, 6)? || n(&row, 5)? > n(&row, 7)?
                    || n(&row, 9)? < n(&row, 8)? || n(&row, 9)? >= n(&row, 10)?
                    || deadline != edit::run_deadline(&run)?.unix_millis()
                    || reserved_at < run.updated_at().unix_millis()
                    || blob(&row, 13)? != a.worktree.as_bytes()
                    || blob(&row, 14)? != a.tool.as_bytes()
                    || n(&row, 15)? != u64::from(a.tool_attempt.get())
                    || n(&row, 16)? != 4
                    || rows.next().await.map_err(unavailable)?.is_some() {
                    return Err(E::InvalidStoredData);
                }
                require_cold_lineage(&tx, a, phase.job_digest(), &kind, &wire, reserved_at, run.updated_at(), &run).await?;
                if let Some(now) = observed_at {
                    if now >= edit::run_deadline(&run)? { return Err(E::DeadlineReached); }
                    let (usage, latest) = edit::read_run_usage(&tx, a.worktree, &run).await?;
                    if now < run.updated_at() || latest.is_some_and(|at| now < at) {
                        return Err(E::TimestampRegressed);
                    }
                    if usage.unobserved_calls > 0 || usage.unknown_calls > 0 {
                        return Err(E::UncertainProviderCall);
                    }
                    for (dimension, used, reserved, limit) in [
                        (AgentBudgetDimension::Turns, u64::from(run.usage().turn_count()), u64::from(usage.calls), u64::from(run.budget().turn_limit().get())),
                        (AgentBudgetDimension::PromptTokens, run.usage().prompt_tokens(), usage.prompt_tokens, run.budget().prompt_token_limit().get()),
                        (AgentBudgetDimension::OutputTokens, run.usage().output_tokens(), usage.output_tokens, run.budget().output_token_limit().get()),
                        (AgentBudgetDimension::Repairs, u64::from(run.usage().repair_count()), u64::from(usage.repairs), u64::from(run.budget().repair_limit().get())),
                    ] {
                        if used.checked_add(reserved).ok_or(E::InvalidStoredData)? > limit {
                            return Err(E::BudgetExhausted(dimension));
                        }
                    }
                }
                Ok(Some(phase.decode_proposal(&bytes).map_err(|_| E::InvalidStoredData)?))
            }.await;
        close(tx, result).await
    })
}

/// Reconstructs the once-only initial/repair history independently of write-time triggers.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn require_cold_lineage(
    tx: &Transaction,
    a: &HttpCorrectionPlanAnchor,
    job: ContentHash,
    kind: &str,
    accepted_wire: &[u8],
    reserved_at: u64,
    causal_at: AgentRunTimestamp,
    run: &AgentRun,
) -> Result<(), E> {
    require_cold_lineage_inner(
        tx,
        a,
        job,
        kind,
        accepted_wire,
        reserved_at,
        causal_at,
        run,
        None,
    )
    .await
}

/// Context continuation binds the exact immutable profile, never a caller-supplied raw ceiling.
/// Existing Direct/origin callers retain their unchanged wrapper for this bounded slice.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn require_context_cold_lineage(
    tx: &Transaction,
    a: &HttpCorrectionPlanAnchor,
    job: ContentHash,
    kind: &str,
    accepted_wire: &[u8],
    reserved_at: u64,
    causal_at: AgentRunTimestamp,
    run: &AgentRun,
    profile: &ModelProfile,
) -> Result<(), E> {
    if !profile.executable_actions_enabled()
        || profile.reference() != a.profile
        || run.model_profile() != Some(profile.reference())
    {
        return Err(E::StaleAnchor);
    }
    require_cold_lineage_inner(
        tx,
        a,
        job,
        kind,
        accepted_wire,
        reserved_at,
        causal_at,
        run,
        Some(profile.settings().output_limit()),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn require_cold_lineage_inner(
    tx: &Transaction,
    a: &HttpCorrectionPlanAnchor,
    job: ContentHash,
    kind: &str,
    accepted_wire: &[u8],
    reserved_at: u64,
    causal_at: AgentRunTimestamp,
    run: &AgentRun,
    generation_ceiling: Option<ModelOutputLimit>,
) -> Result<(), E> {
    let deadline = edit::run_deadline(run)?.unix_millis();
    let mut rows = tx.query(
        "SELECT job_digest,
         CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 256 AND 133000 THEN anchor ELSE x'' END,
         wire_digest,outcome,reserved_at,finished_at,deadline,prompt_reserve,output_reserve,
         reported_prompt,reported_output,worktree_id,tool_run_id,tool_attempt,
         (SELECT COUNT(*) FROM http_correction_plan_reservations WHERE run_id=?1 AND diagnostic_digest=?2),
         protocol_version
         FROM http_correction_plan_reservations WHERE run_id=?1 AND diagnostic_digest=?2 AND call_kind='initial'",
        params![a.run.as_bytes().to_vec(), a.diagnostic.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::InvalidStoredData)?;
    let reported_output = n(&row, 10)?;
    if blob(&row, 0)? != job.as_bytes()
        || blob(&row, 1)? != anchor_bytes(a)
        || blob(&row, 11)? != a.worktree.as_bytes()
        || blob(&row, 12)? != a.tool.as_bytes()
        || n(&row, 13)? != u64::from(a.tool_attempt.get())
        || n(&row, 15)? != 4
        || n(&row, 4)? < causal_at.unix_millis()
        || n(&row, 5)? < n(&row, 4)?
        || n(&row, 5)? >= n(&row, 6)?
        || n(&row, 6)? != deadline
        || n(&row, 9)? == 0
        || reported_output == 0
        || n(&row, 9)? > n(&row, 7)?
        || reported_output > n(&row, 8)?
        || generation_ceiling.is_some_and(|limit| reported_output > u64::from(limit.get()))
    {
        return Err(E::InvalidStoredData);
    }
    let outcome: String = row.get(3).map_err(|_| E::InvalidStoredData)?;
    let valid = match kind {
        "initial" => outcome == "accepted_reply" && n(&row, 14)? == 1,
        "structural_repair" => {
            outcome == "invalid_structure"
                && n(&row, 14)? == 2
                && n(&row, 5)? <= reserved_at
                && blob(&row, 2)? != accepted_wire
        }
        _ => false,
    };
    // libSQL native Row borrows its statement cursor; copy values before advancing it.
    if !valid || rows.next().await.map_err(unavailable)?.is_some() {
        return Err(E::InvalidStoredData);
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
    db.connection_for_operation().await.map_err(unavailable)
}
fn unavailable<T>(_: T) -> E {
    E::Unavailable
}
fn time(t: AgentRunTimestamp) -> Result<i64, E> {
    i64::try_from(t.unix_millis()).map_err(|_| E::InvalidStoredData)
}
fn n(row: &Row, col: i32) -> Result<u64, E> {
    u64::try_from(row.get::<i64>(col).map_err(|_| E::InvalidStoredData)?)
        .map_err(|_| E::InvalidStoredData)
}
fn blob(row: &Row, col: i32) -> Result<Vec<u8>, E> {
    row.get(col).map_err(|_| E::InvalidStoredData)
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

pub(crate) fn anchor_bytes(anchor: &HttpCorrectionPlanAnchor) -> Vec<u8> {
    anchor_codec::encode(anchor)
}

pub(crate) fn decode_retained_anchor(
    bytes: &[u8],
    goal: &GoalContract,
) -> Result<HttpCorrectionPlanAnchor, E> {
    anchor_codec::decode(bytes, goal)
}

async fn check_kind(
    tx: &Transaction,
    phase: &PreparedHttpCorrectionPlan,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
) -> Result<(), E> {
    let a = phase.anchor();
    let mut rows=tx.query("SELECT call_kind,job_digest,anchor,wire_digest,outcome,protocol_version FROM http_correction_plan_reservations WHERE run_id=?1 AND diagnostic_digest=?2",params![a.run.as_bytes().to_vec(),a.diagnostic.as_bytes().to_vec()]).await.map_err(unavailable)?;
    let mut initial = false;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        if n(&row, 5)? != 4 {
            return Err(E::InvalidStoredData);
        }
        let call: String = row.get(0).map_err(unavailable)?;
        if kind == EditPhaseCallKind::Initial || call == "structural_repair" {
            return Err(E::AlreadyReserved);
        }
        if blob(&row, 1)? != phase.job_digest().as_bytes() || blob(&row, 2)? != anchor_bytes(a) {
            return Err(E::StaleAnchor);
        }
        if blob(&row, 3)? == wire.digest().as_bytes() {
            return Err(E::UnchangedRepairWire);
        }
        let outcome: Option<String> = row.get(4).map_err(unavailable)?;
        initial = outcome.as_deref() == Some("invalid_structure");
    }
    if kind == EditPhaseCallKind::StructuralRepair && !initial {
        return Err(E::RepairNotAvailable);
    }
    Ok(())
}

pub(crate) async fn validate(
    tx: &Transaction,
    worktree: WorktreeId,
    phase: &PreparedHttpCorrectionPlan,
) -> Result<AgentRun, E> {
    let a = phase.anchor();
    if a.worktree != worktree {
        return Err(E::StaleAnchor);
    }
    if let Some(offer) = &a.source_offer
        && (offer.worktree_id() != worktree
            || offer.index_run_id() != a.index
            || offer.snapshot_id() != a.snapshot
            || offer.revision() != &a.source
            || offer.range().is_empty()
            || a.source_range.contains(offer.range()))
    {
        return Err(E::StaleAnchor);
    }
    let run = journal::load_run_from_transaction(tx, worktree, a.run)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    if run.state() != AgentControllerState::Replan
        || run.goal_contract() != a.goal
        || run.last_event_sequence() != a.sequence
        || run.task_ledger_revision() != a.ledger
        || run.current_snapshot_id() != a.snapshot
        || run.model_profile() != Some(a.profile)
    {
        return Err(E::StaleAnchor);
    }
    let tail = journal::read_event(tx, a.run, a.sequence)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::InvalidStoredData)?;
    if tail.run_id() != a.run
        || tail.sequence() != a.sequence
        || tail.snapshot_id() != a.snapshot
        || tail.occurred_at() != run.updated_at()
        || tail.kind()
            != (RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            })
        || tail.payload()
            != &RunEventPayload::new(
                RunEventCode::VerificationFailure,
                Some(RunEventOutcome::Failed),
                None,
            )
        || tail.subject().is_some()
        || tail.turn_charge().is_some()
    {
        return Err(E::StaleAnchor);
    }
    if goals::task_current_revision(tx, worktree, a.goal.task_id())
        .await
        .map_err(|_| E::Unavailable)?
        != Some(a.goal.revision())
    {
        return Err(E::StaleAnchor);
    }
    let goal =
        goals::load_revision_from_transaction(tx, worktree, a.goal.task_id(), a.goal.revision())
            .await
            .map_err(|_| E::InvalidStoredData)?
            .ok_or(E::StaleAnchor)?;
    if goal.original().map(|v| v.digest()) != Some(a.original)
        || !GoalOriginalCoverage::derive_from(&goal)
            .map_err(|_| E::InvalidStoredData)?
            .is_complete()
    {
        return Err(E::StaleAnchor);
    }
    let stored = ledgers::load_from_transaction(tx, worktree, a.goal.task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    if stored.version() != a.version
        || stored.ledger().revision() != a.ledger
        || stored.ledger().goal_contract() != a.goal
        || stored.ledger().pending_planning().is_some()
    {
        return Err(E::StaleAnchor);
    }
    let step = stored.ledger().step(a.step).ok_or(E::StaleAnchor)?;
    let attempt = step.attempts().last().ok_or(E::StaleAnchor)?;
    if !step.is_active_plan_step()
        || step.status() != TaskStepStatus::Failed
        || attempt.number() != a.failed_attempt
        || attempt.run_id() != a.run
        || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        || attempt.finished_at().map(|v| v.unix_millis()) != Some(run.updated_at().unix_millis())
        || step.definition().verification_spec().id() != a.verification
    {
        return Err(E::StaleAnchor);
    }
    let diagnosis =
        recovery::mismatch::load_from_transaction(tx, worktree, a.run, a.tool, a.tool_attempt)
            .await
            .map_err(|e| match e.classify() {
                a3_application::AgentRecoveryStoreFailure::Unavailable => E::Unavailable,
                _ => E::StaleAnchor,
            })?
            .ok_or(E::StaleAnchor)?;
    if diagnosis.digest() != a.diagnostic
        || diagnosis.script() != &a.source
        || diagnosis.context().step_id != a.step
        || diagnosis.context().spec_id != a.verification
    {
        return Err(E::StaleAnchor);
    }
    let mut source = tx
        .query(
            "SELECT 1 FROM index_runs i JOIN file_revisions f USING(index_run_id)
         WHERE i.worktree_id=?1 AND i.index_run_id=?2 AND i.snapshot_id=?3 AND i.status='published'
         AND i.run_sequence=(SELECT MAX(run_sequence) FROM index_runs
             WHERE worktree_id=?1 AND status='published')
         AND f.repository_path=?4 AND f.content_hash=?5",
            params![
                worktree.as_bytes().to_vec(),
                a.index.as_bytes().to_vec(),
                a.snapshot.as_bytes().to_vec(),
                a.source.path().as_bytes().to_vec(),
                a.source.content_hash().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(unavailable)?;
    if source.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    let mut causal = tx
        .query(
            "SELECT 1 FROM run_events tail JOIN run_events previous
         ON previous.run_id=tail.run_id AND previous.event_sequence=tail.event_sequence-1
         JOIN loopback_mismatch_diagnostics d ON d.event_id=previous.event_id
         WHERE tail.run_id=?1 AND tail.event_sequence=?2 AND tail.event_kind='state_transition'
         AND tail.state_from='execute' AND tail.state_to='replan'
         AND tail.payload_code='verification_failure' AND tail.payload_outcome='failed'
         AND tail.subject_kind IS NULL AND tail.snapshot_id=?3
         AND tail.occurred_at_unix_millis=previous.occurred_at_unix_millis
         AND d.tool_run_id=?4 AND d.attempt_sequence=?5 AND d.digest=?6",
            params![
                a.run.as_bytes().to_vec(),
                i64::try_from(a.sequence.get()).map_err(|_| E::InvalidStoredData)?,
                a.snapshot.as_bytes().to_vec(),
                a.tool.as_bytes().to_vec(),
                i64::from(a.tool_attempt.get()),
                a.diagnostic.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(unavailable)?;
    if causal.next().await.map_err(unavailable)?.is_none() {
        return Err(E::StaleAnchor);
    }
    if recovery::has_unreconciled_mutation(tx, worktree)
        .await
        .map_err(|_| E::Unavailable)?
    {
        return Err(E::MutationRecoveryRequired);
    }
    Ok(run)
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
    let mut rows = tx
        .query(
            "SELECT COUNT(*),COUNT(CASE WHEN call_kind='structural_repair' THEN 1 END),
         COALESCE(SUM(MAX(prompt_reserve,COALESCE(reported_prompt,0))),0),
         COALESCE(SUM(MAX(output_reserve,COALESCE(reported_output,0))),0),
         COUNT(CASE WHEN outcome IS NULL THEN 1 END),
         COUNT(CASE WHEN outcome IN ('unknown','budget_overrun') THEN 1 END),
         MAX(COALESCE(finished_at,reserved_at)),
         COUNT(CASE WHEN protocol_version IS NOT 4 THEN 1 END) FROM http_correction_plan_reservations
         WHERE worktree_id=?1 AND run_id=?2",
            params![worktree.as_bytes().to_vec(), run.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::InvalidStoredData)?;
    if n(&row, 7)? != 0 {
        return Err(E::InvalidStoredData);
    }
    let calls = u32::try_from(n(&row, 0)?).map_err(|_| E::InvalidStoredData)?;
    let latest: Option<i64> = row.get(6).map_err(unavailable)?;
    Ok((
        EditPhaseReservedUsage {
            calls,
            repairs: u32::try_from(n(&row, 1)?).map_err(|_| E::InvalidStoredData)?,
            prompt_tokens: n(&row, 2)?,
            output_tokens: n(&row, 3)?,
            unobserved_calls: u32::try_from(n(&row, 4)?).map_err(|_| E::InvalidStoredData)?,
            unknown_calls: u32::try_from(n(&row, 5)?).map_err(|_| E::InvalidStoredData)?,
        },
        latest
            .map(|v| {
                u64::try_from(v)
                    .ok()
                    .and_then(|v| AgentRunTimestamp::from_unix_millis(v).ok())
                    .ok_or(E::InvalidStoredData)
            })
            .transpose()?,
    ))
}
