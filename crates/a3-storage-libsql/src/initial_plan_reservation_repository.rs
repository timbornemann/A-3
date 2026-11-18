//! The first Plan's reservation and Ready Ledger share the existing Goal transaction.

use super::{Scope, fixed_bytes, has_uncertain_calls, nonnegative, read_scope, time};
use crate::{
    LibsqlKnowledgeStore, edit_phase_reservation_repository as edit, goal_contract_repository,
    migration::read_user_version,
    run_journal_repository::RunJournalRepositoryError as RunError,
    task_ledger_repository::{self, TaskLedgerRepositoryError as LedgerError},
};
use a3_application::{
    AdmittedInitialPlan, EditPhaseCallKind, EditPhaseReservationFailure as AccountingError,
    EditPhaseReservedUsage, EditPhaseWireCost, InitialPlanFinish, InitialPlanRejection,
    InitialPlanReservation, InitialPlanReservationFailure as Failure, InitialPlanReservationFuture,
    InitialPlanReservationStore, PlanPhaseAnchor, PlanPhaseHalt, PreparedPlanPhase,
    StoredTaskLedger,
};
use a3_domain::{
    AgentBudgetDimension, AgentRun, AgentRunTimestamp, AgentRunUsage, ContentHash, GoalContract,
    GoalContractRevision, IndexRunId, ProjectIdentity, SnapshotId, TaskId, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, params, params_from_iter};

#[path = "initial_plan_semantic_halt.rs"]
mod semantic_halt;

pub(super) const SCHEMA_VERSION: u32 = 52;

impl InitialPlanReservationStore for LibsqlKnowledgeStore {
    fn reserve_initial_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedPlanPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, InitialPlanReservation> {
        Box::pin(async move {
            if phase.anchor().worktree() != project.worktree().id() {
                return Err(Failure::StalePlan);
            }
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = reserve(&tx, phase, kind, wire, now).await;
            let deadline = close(tx, result).await?;
            Ok(InitialPlanReservation::issue(
                phase, kind, wire, now, deadline,
            ))
        })
    }

    fn finish_initial_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: InitialPlanReservation,
        outcome: InitialPlanFinish,
        now: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, Option<StoredTaskLedger>> {
        Box::pin(async move {
            if reservation.anchor().worktree() != project.worktree().id() {
                return Err(Failure::StalePlan);
            }
            if now < reservation.reserved_at() {
                return Err(AccountingError::TimestampRegressed.into());
            }
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = finish(&tx, &reservation, outcome, now).await;
            close(tx, result).await?
        })
    }

    fn load_initial_plan_usage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> InitialPlanReservationFuture<'a, EditPhaseReservedUsage> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(unavailable)?;
            let result = async {
                scope(&tx, project.worktree().id(), task).await?;
                Ok(read_summary(&tx, task).await?.usage)
            }
            .await;
            close(tx, result).await
        })
    }

    fn load_initial_plan_halt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> InitialPlanReservationFuture<'a, Option<PlanPhaseHalt>> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(unavailable)?;
            let result = semantic_halt::load(&tx, project.worktree().id(), task).await;
            close(tx, result).await
        })
    }
}

async fn open(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
) -> Result<Connection, Failure> {
    let db = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| Failure::from(AccountingError::Unavailable))?;
    if db.schema_version().get() < SCHEMA_VERSION {
        return Err(AccountingError::UnsupportedSchema.into());
    }
    db.connection_for_operation()
        .await
        .map_err(|_| AccountingError::Unavailable.into())
}

async fn scope(tx: &Transaction, worktree: WorktreeId, task: TaskId) -> Result<Scope, Failure> {
    read_scope(tx, worktree, task)
        .await
        .map_err(intake_failure)?
        .ok_or(Failure::ScopeMissing)
}

async fn reserve(
    tx: &Transaction,
    phase: &PreparedPlanPhase,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    now: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, Failure> {
    let anchor = phase.anchor();
    let task = anchor.goal().task_id();
    let scope = scope(tx, anchor.worktree(), task).await?;
    validate_live(tx, anchor, &scope).await?;
    let summary = read_summary(tx, task).await?;
    if now.unix_millis() < scope.accounting.captured_at().unix_millis()
        || scope
            .latest_at
            .max(summary.latest_at)
            .is_some_and(|latest| now < latest)
    {
        return Err(AccountingError::TimestampRegressed.into());
    }
    let deadline = scope.accounting.deadline();
    if now >= deadline {
        return Err(AccountingError::DeadlineReached.into());
    }
    let usage = edit::merge_phase_usage(scope.accounting.usage(), summary.usage)?;
    if has_uncertain_calls(usage) {
        return Err(AccountingError::UncertainProviderCall.into());
    }
    match kind {
        EditPhaseCallKind::Initial if summary.usage.calls != 0 => {
            return Err(AccountingError::AlreadyReserved.into());
        }
        EditPhaseCallKind::StructuralRepair => {
            if summary.usage.repairs != 0 {
                return Err(AccountingError::AlreadyReserved.into());
            }
            require_repair(tx, task, phase.job_digest(), wire.digest()).await?;
        }
        EditPhaseCallKind::Initial => {}
    }
    edit::check_budget(
        scope.accounting.budget(),
        AgentRunUsage::ZERO,
        usage,
        kind,
        wire,
    )?;
    let inserted = tx
        .execute(
            "INSERT INTO initial_plan_phase_reservations
          (task_id,call_kind,goal_revision,job_digest,index_run_id,snapshot_id,wire_digest,
           wire_bytes,prompt_reserve,output_reserve,reserved_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                task.as_bytes().to_vec(),
                kind_text(kind),
                i64::from(anchor.goal().revision().get()),
                phase.job_digest().as_bytes().to_vec(),
                anchor.index_run_id().as_bytes().to_vec(),
                anchor.snapshot().as_bytes().to_vec(),
                wire.digest().as_bytes().to_vec(),
                i64::from(wire.request_bytes()),
                i64::from(wire.prompt_estimate()),
                i64::from(wire.output_reserve()),
                time(now).map_err(intake_failure)?,
            ],
        )
        .await
        .map_err(unavailable)?;
    if inserted != 1 {
        return Err(AccountingError::InvalidStoredData.into());
    }
    Ok(deadline)
}

async fn require_repair(
    tx: &Transaction,
    task: TaskId,
    job: ContentHash,
    wire: ContentHash,
) -> Result<(), Failure> {
    let mut rows = tx.query(
        "SELECT wire_digest FROM initial_plan_phase_reservations
         WHERE task_id=?1 AND call_kind='initial' AND job_digest=?2 AND outcome='invalid_structure'",
        params![task.as_bytes().to_vec(), job.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(AccountingError::RepairNotAvailable)?;
    if fixed_bytes(&row, 0).map_err(intake_failure)? == *wire.as_bytes() {
        return Err(AccountingError::UnchangedRepairWire.into());
    }
    Ok(())
}

async fn validate_live(
    tx: &Transaction,
    anchor: PlanPhaseAnchor,
    scope: &Scope,
) -> Result<GoalContract, Failure> {
    let goal_ref = anchor.goal();
    let task = goal_ref.task_id();
    let worktree = anchor.worktree();
    if goal_ref.revision() != GoalContractRevision::INITIAL
        || scope.accounting.linked_run().is_some()
        || scope.original_digest != anchor.original_digest()
        || scope.accounting.profile() != anchor.model_profile()
        || goal_contract_repository::task_current_revision(tx, worktree, task)
            .await
            .map_err(|_| AccountingError::Unavailable)?
            != Some(goal_ref.revision())
    {
        return Err(Failure::StalePlan);
    }
    let goal = goal_contract_repository::load_revision_from_transaction(
        tx,
        worktree,
        task,
        goal_ref.revision(),
    )
    .await
    .map_err(|_| AccountingError::InvalidStoredData)?
    .ok_or(Failure::StalePlan)?;
    if goal
        .original()
        .is_none_or(|original| original.digest() != anchor.original_digest())
        || goal.created_at() != scope.accounting.captured_at()
        || latest_publication(tx, worktree).await?
            != Some((anchor.index_run_id(), anchor.snapshot()))
    {
        return Err(Failure::StalePlan);
    }
    let mut rows = tx
        .query(
            "SELECT 1 FROM task_ledgers WHERE task_id=?1",
            [task.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_some() {
        return Err(Failure::AlreadyMaterialized);
    }
    Ok(goal)
}

async fn latest_publication(
    tx: &Transaction,
    worktree: WorktreeId,
) -> Result<Option<(IndexRunId, SnapshotId)>, Failure> {
    let mut rows = tx
        .query(
            "SELECT index_run_id,snapshot_id FROM index_runs
         WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1",
            [worktree.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    Ok(Some((
        IndexRunId::from_bytes(fixed_bytes(&row, 0).map_err(intake_failure)?),
        SnapshotId::from_bytes(fixed_bytes(&row, 1).map_err(intake_failure)?),
    )))
}

struct Observation {
    status: &'static str,
    prompt: Option<u32>,
    output: Option<u32>,
    plan: Option<AdmittedInitialPlan>,
    halt: Option<PlanPhaseHalt>,
    needs_current: bool,
}

impl Observation {
    fn new(finish: InitialPlanFinish) -> Self {
        let observed = |status, prompt, output| Self {
            status,
            prompt,
            output,
            plan: None,
            halt: None,
            needs_current: true,
        };
        match finish {
            InitialPlanFinish::Halted {
                reason,
                prompt_tokens,
                output_tokens,
            } => Self {
                halt: Some(reason),
                ..observed("rejected_reply", Some(prompt_tokens), Some(output_tokens))
            },
            InitialPlanFinish::Accepted {
                plan,
                prompt_tokens,
                output_tokens,
            } => Self {
                plan: Some(plan),
                ..observed("accepted", Some(prompt_tokens), Some(output_tokens))
            },
            InitialPlanFinish::NeedEvidence {
                prompt_tokens,
                output_tokens,
            } => observed("need_evidence", Some(prompt_tokens), Some(output_tokens)),
            InitialPlanFinish::TooLarge {
                prompt_tokens,
                output_tokens,
            } => observed("too_large", Some(prompt_tokens), Some(output_tokens)),
            InitialPlanFinish::ObservedFailure {
                kind,
                reported_prompt_tokens,
                reported_output_tokens,
            } => {
                let status = match kind {
                    InitialPlanRejection::InvalidStructure => "invalid_structure",
                    InitialPlanRejection::RejectedReply => "rejected_reply",
                    InitialPlanRejection::ProviderFailure => "provider_failure",
                    InitialPlanRejection::Cancelled => "cancelled",
                    InitialPlanRejection::Unknown => "unknown",
                };
                Self {
                    needs_current: kind == InitialPlanRejection::InvalidStructure,
                    ..observed(status, reported_prompt_tokens, reported_output_tokens)
                }
            }
        }
    }
}

async fn finish(
    tx: &Transaction,
    reservation: &InitialPlanReservation,
    outcome: InitialPlanFinish,
    now: AgentRunTimestamp,
) -> Result<Result<Option<StoredTaskLedger>, Failure>, Failure> {
    let anchor = reservation.anchor();
    let task = anchor.goal().task_id();
    let scope = scope(tx, anchor.worktree(), task).await?;
    if scope.accounting.deadline() != reservation.deadline()
        || scope.original_digest != anchor.original_digest()
        || scope.accounting.profile() != anchor.model_profile()
    {
        return Err(AccountingError::ObservationConflict.into());
    }
    let mut observation = Observation::new(outcome);
    let semantic_schema =
        read_user_version(tx).await.map_err(unavailable)? >= semantic_halt::SCHEMA_VERSION;
    if observation.halt.is_some() && !semantic_schema {
        return Err(AccountingError::UnsupportedSchema.into());
    }
    let (stored, rejection) =
        apply_observation(tx, reservation, &scope, &mut observation, now).await?;
    let wire = reservation.wire();
    let halt_marker = if semantic_schema {
        ", has_semantic_halt=?17"
    } else {
        ""
    };
    let sql = format!("UPDATE initial_plan_phase_reservations
         SET outcome=?1, finished_at=?2, reported_prompt=?3, reported_output=?4, accepted_ledger_version=?5{halt_marker}
         WHERE task_id=?6 AND call_kind=?7 AND goal_revision=?8 AND job_digest=?9
           AND index_run_id=?10 AND snapshot_id=?11 AND wire_digest=?12 AND wire_bytes=?13
           AND prompt_reserve=?14 AND output_reserve=?15 AND reserved_at=?16 AND outcome IS NULL");
    let mut values = Vec::from(params![
        observation.status,
        time(now).map_err(intake_failure)?,
        observation.prompt.map(i64::from),
        observation.output.map(i64::from),
        stored.as_ref().map(|_| 1_i64),
        task.as_bytes().to_vec(),
        kind_text(reservation.kind()),
        i64::from(anchor.goal().revision().get()),
        reservation.job_digest().as_bytes().to_vec(),
        anchor.index_run_id().as_bytes().to_vec(),
        anchor.snapshot().as_bytes().to_vec(),
        wire.digest().as_bytes().to_vec(),
        i64::from(wire.request_bytes()),
        i64::from(wire.prompt_estimate()),
        i64::from(wire.output_reserve()),
        time(reservation.reserved_at()).map_err(intake_failure)?,
    ]);
    if semantic_schema {
        values.push(Ok(libsql::Value::Integer(i64::from(u8::from(
            observation.halt.is_some(),
        )))));
    }
    let changed = tx
        .execute(&sql, params_from_iter(values))
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(AccountingError::ObservationConflict.into());
    }
    if let Some(reason) = observation.halt {
        semantic_halt::write(tx, task, reservation.kind(), reason).await?;
    }
    Ok(rejection.map_or(Ok(stored), Err))
}

async fn apply_observation(
    tx: &Transaction,
    reservation: &InitialPlanReservation,
    scope: &Scope,
    observation: &mut Observation,
    now: AgentRunTimestamp,
) -> Result<(Option<StoredTaskLedger>, Option<Failure>), Failure> {
    if !observation.needs_current {
        return Ok((None, None));
    }
    if now >= reservation.deadline() {
        observation.status = "stale";
        return Ok((None, Some(AccountingError::DeadlineReached.into())));
    }
    let (Some(prompt), Some(output)) = (
        observation.prompt.filter(|n| *n > 0),
        observation.output.filter(|n| *n > 0),
    ) else {
        observation.status = "unknown";
        return Ok((None, Some(AccountingError::IncompleteProviderUsage.into())));
    };
    let wire = reservation.wire();
    let overrun = if prompt > wire.prompt_estimate() {
        Some(AgentBudgetDimension::PromptTokens)
    } else if output > wire.output_reserve() {
        Some(AgentBudgetDimension::OutputTokens)
    } else {
        None
    };
    if let Some(dimension) = overrun {
        observation.status = "budget_overrun";
        return Ok((
            None,
            Some(AccountingError::BudgetExhausted(dimension).into()),
        ));
    }
    let anchor = reservation.anchor();
    let goal = match validate_live(tx, anchor, scope).await {
        Ok(goal) => goal,
        Err(error @ (Failure::StalePlan | Failure::AlreadyMaterialized)) => {
            observation.status = "stale";
            return Ok((None, Some(error)));
        }
        Err(error) => return Err(error),
    };
    let Some(plan) = observation.plan.take() else {
        return Ok((None, None));
    };
    let ledger = plan.ledger();
    if plan.anchor() != anchor
        || plan.job_digest() != reservation.job_digest()
        || a3_application::AdmitAgentGoalExecution
            .execute_task(&goal, ledger)
            .is_err()
    {
        observation.status = "rejected_reply";
        return Ok((None, Some(Failure::InvalidPlan)));
    }
    let version = task_ledger_repository::create_in_transaction(tx, anchor.worktree(), ledger)
        .await
        .map_err(|_| AccountingError::Unavailable)?;
    Ok((Some(StoredTaskLedger::new(ledger.clone(), version)), None))
}

pub(super) struct Summary {
    pub(super) usage: EditPhaseReservedUsage,
    pub(super) latest_at: Option<AgentRunTimestamp>,
}

pub(super) async fn read_summary(tx: &Transaction, task: TaskId) -> Result<Summary, Failure> {
    let mut rows = tx
        .query(
            "SELECT COUNT(*), COUNT(CASE WHEN call_kind='structural_repair' THEN 1 END),
                COALESCE(SUM(MAX(prompt_reserve,COALESCE(reported_prompt,0))),0),
                COALESCE(SUM(MAX(output_reserve,COALESCE(reported_output,0))),0),
                COUNT(CASE WHEN outcome IS NULL THEN 1 END),
                COUNT(CASE WHEN outcome='unknown' THEN 1 END),
                MAX(COALESCE(finished_at,reserved_at))
         FROM initial_plan_phase_reservations WHERE task_id=?1",
            [task.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(AccountingError::InvalidStoredData)?;
    let read = |index| nonnegative(&row, index).map_err(intake_failure);
    let latest: Option<i64> = row.get(6).map_err(|_| AccountingError::InvalidStoredData)?;
    let latest_at = latest
        .map(|value| {
            let millis = u64::try_from(value).map_err(|_| AccountingError::InvalidStoredData)?;
            AgentRunTimestamp::from_unix_millis(millis)
                .map_err(|_| AccountingError::InvalidStoredData)
        })
        .transpose()?;
    Ok(Summary {
        usage: EditPhaseReservedUsage {
            calls: count(&row, 0)?,
            repairs: count(&row, 1)?,
            prompt_tokens: read(2)?,
            output_tokens: read(3)?,
            unobserved_calls: count(&row, 4)?,
            unknown_calls: count(&row, 5)?,
        },
        latest_at,
    })
}

pub(crate) async fn reject_direct_create(
    tx: &Transaction,
    task: TaskId,
) -> Result<(), LedgerError> {
    if read_user_version(tx).await.map_err(LedgerError::Read)? < SCHEMA_VERSION {
        return Ok(());
    }
    let mut rows = tx
        .query(
            "SELECT 1 FROM initial_plan_phase_reservations WHERE task_id=?1 LIMIT 1",
            [task.as_bytes().to_vec()],
        )
        .await
        .map_err(LedgerError::Read)?;
    if rows.next().await.map_err(LedgerError::Read)?.is_some() {
        return Err(LedgerError::InvalidInput);
    }
    Ok(())
}

pub(super) async fn validate_run_link(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
    summary: &Summary,
) -> Result<(), RunError> {
    if summary.usage.calls == 0 {
        return Ok(());
    }
    let mut rows = tx
        .query(
            "SELECT p.index_run_id,p.snapshot_id FROM initial_plan_phase_reservations p
         JOIN task_ledgers l ON l.task_id=p.task_id AND l.goal_revision=p.goal_revision
          AND l.store_version=p.accepted_ledger_version
         WHERE p.task_id=?1 AND p.outcome='accepted' AND l.plan_revision=1",
            [run.goal_contract().task_id().as_bytes().to_vec()],
        )
        .await
        .map_err(RunError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(RunError::Read)?
        .ok_or(RunError::InvalidInput)?;
    let index =
        IndexRunId::from_bytes(fixed_bytes(&row, 0).map_err(|_| RunError::InvalidStoredData)?);
    let snapshot =
        SnapshotId::from_bytes(fixed_bytes(&row, 1).map_err(|_| RunError::InvalidStoredData)?);
    if rows.next().await.map_err(RunError::Read)?.is_some()
        || run.current_snapshot_id() != snapshot
        || latest_publication(tx, worktree)
            .await
            .map_err(|_| RunError::InvalidStoredData)?
            != Some((index, snapshot))
    {
        return Err(RunError::InvalidInput);
    }
    Ok(())
}

const fn kind_text(kind: EditPhaseCallKind) -> &'static str {
    match kind {
        EditPhaseCallKind::Initial => "initial",
        EditPhaseCallKind::StructuralRepair => "structural_repair",
    }
}

fn count(row: &Row, index: i32) -> Result<u32, Failure> {
    u32::try_from(nonnegative(row, index).map_err(intake_failure)?)
        .map_err(|_| AccountingError::InvalidStoredData.into())
}

fn intake_failure(error: super::Failure) -> Failure {
    match error {
        super::Failure::Accounting(error) => error.into(),
        _ => AccountingError::InvalidStoredData.into(),
    }
}

fn unavailable(_: libsql::Error) -> Failure {
    AccountingError::Unavailable.into()
}

async fn close<T>(tx: Transaction, result: Result<T, Failure>) -> Result<T, Failure> {
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
