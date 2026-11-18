//! The pending Goal's provider budget and its one-time link to the existing Run.

use super::{GoalContractRepositoryError as GoalError, intake, intake_review};
use crate::{
    LibsqlKnowledgeStore, edit_phase_reservation_repository as edit, migration::read_user_version,
    run_journal_repository::RunJournalRepositoryError as RunError,
};
use a3_application::{
    EditPhaseCallKind, EditPhaseReservationFailure as AccountingError, EditPhaseReservedUsage,
    EditPhaseWireCost, GoalIntakeLoadResult, GoalIntakeStoreVersion, IntakePhaseAccounting,
    IntakePhaseAnchor, IntakePhaseFinish, IntakePhaseRejection, IntakePhaseReservation,
    IntakePhaseReservationFailure as Failure, IntakePhaseReservationFuture,
    IntakePhaseReservationStore, StoredPendingGoalIntake, prepare_goal_from_intake_review,
};
use a3_domain::{
    AgentActionLimit, AgentBudgetDimension, AgentRepairLimit, AgentRun, AgentRunBudget,
    AgentRunDurationLimit, AgentRunId, AgentRunTimestamp, AgentRunUsage, AgentTokenLimit,
    AgentTurnLimit, GoalContractRevision, GoalContractTimestamp, GoalIntakeReview,
    GoalIntakeReviewRevision, GoalOriginalDigest, ModelProfileId, ModelProfileReference,
    ModelProfileVersion, PendingGoalIntake, ProjectIdentity, TaskId, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, params};

#[path = "initial_plan_reservation_repository.rs"]
pub(crate) mod initial_plan;

pub(crate) const SCHEMA_VERSION: u32 = 51;

impl IntakePhaseReservationStore for LibsqlKnowledgeStore {
    fn commit_bound_intake<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: IntakePhaseAnchor,
        review: &'a GoalIntakeReview,
        budget: AgentRunBudget,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, IntakePhaseAccounting> {
        Box::pin(async move {
            if project.worktree().id() != anchor.worktree() || anchor.review_revision().is_some() {
                return Err(Failure::InvalidAnchor);
            }
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = commit_bound(&tx, anchor, review, budget, now).await;
            close(tx, result).await
        })
    }

    fn reserve_intake_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: IntakePhaseAnchor,
        kind: EditPhaseCallKind,
        budget: AgentRunBudget,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, IntakePhaseReservation> {
        Box::pin(async move {
            if project.worktree().id() != anchor.worktree() {
                return Err(Failure::InvalidAnchor);
            }
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(unavailable)?;
            let result = reserve(&tx, anchor, kind, budget, wire, now).await;
            let deadline = close(tx, result).await?;
            Ok(IntakePhaseReservation::issue(
                anchor, kind, wire, now, deadline,
            ))
        })
    }

    fn finish_intake_phase<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        reservation: IntakePhaseReservation,
        outcome: IntakePhaseFinish<'a>,
        now: AgentRunTimestamp,
    ) -> IntakePhaseReservationFuture<'a, ()> {
        Box::pin(async move {
            if project.worktree().id() != reservation.anchor().worktree() {
                return Err(Failure::InvalidAnchor);
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
            close(tx, result).await?.map_or(Ok(()), Err)
        })
    }

    fn load_intake_phase_accounting<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task: TaskId,
    ) -> IntakePhaseReservationFuture<'a, Option<IntakePhaseAccounting>> {
        Box::pin(async move {
            let connection = open(self, project).await?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(unavailable)?;
            let result = async {
                let worktree = project.worktree().id();
                let Some(scope) = read_scope(&tx, worktree, task).await? else {
                    return Ok(None);
                };
                validate_owner(&tx, worktree, task, &scope).await?;
                Ok(Some(scope.accounting))
            }
            .await;
            close(tx, result).await
        })
    }
}

async fn open(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
) -> Result<Connection, Failure> {
    let database = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| Failure::from(AccountingError::Unavailable))?;
    if database.schema_version().get() < SCHEMA_VERSION {
        return Err(AccountingError::UnsupportedSchema.into());
    }
    database
        .connection_for_operation()
        .await
        .map_err(|_| AccountingError::Unavailable.into())
}

async fn commit_bound(
    tx: &Transaction,
    anchor: IntakePhaseAnchor,
    review: &GoalIntakeReview,
    budget: AgentRunBudget,
    now: AgentRunTimestamp,
) -> Result<IntakePhaseAccounting, Failure> {
    let pending = validate_pending(tx, anchor).await?;
    let worktree = anchor.worktree();
    let task = anchor.goal().task_id();
    if read_scope(tx, worktree, task).await?.is_some() {
        return Err(Failure::ScopeConflict);
    }
    let [batch] = review.batches() else {
        return Err(Failure::InvalidReview);
    };
    let original = pending.pending().original();
    if batch.window() != anchor.window()
        || anchor.window().start_byte() != 0
        || anchor.window().end_byte() as usize != original.as_bytes().len()
    {
        return Err(Failure::InvalidReview);
    }
    prepare_goal_from_intake_review(pending.pending(), review)
        .map_err(|_| Failure::InvalidReview)?;
    let accounting = IntakePhaseAccounting::reconstruct(
        budget,
        anchor.captured_at(),
        anchor.model_profile(),
        None,
        EditPhaseReservedUsage::default(),
    )?;
    accounting.run_budget(now)?;
    insert_scope(tx, anchor, accounting).await?;
    intake_review::append_in_transaction(
        tx,
        worktree,
        task,
        anchor.capture_version(),
        None,
        review,
    )
    .await
    .map_err(classify_goal)?;
    Ok(accounting)
}

async fn reserve(
    tx: &Transaction,
    anchor: IntakePhaseAnchor,
    kind: EditPhaseCallKind,
    budget: AgentRunBudget,
    wire: EditPhaseWireCost,
    now: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, Failure> {
    let pending = validate_pending(tx, anchor).await?;
    let worktree = anchor.worktree();
    let task = anchor.goal().task_id();
    let mut scope = read_scope(tx, worktree, task).await?;
    // A provider-created scope always has at least one reservation in the same transaction.
    if scope
        .as_ref()
        .is_some_and(|scope| scope.accounting.usage().calls == 0)
    {
        return Err(AccountingError::AlreadyReserved.into());
    }
    if scope.is_none() {
        let accounting = IntakePhaseAccounting::reconstruct(
            budget,
            anchor.captured_at(),
            anchor.model_profile(),
            None,
            EditPhaseReservedUsage::default(),
        )?;
        insert_scope(tx, anchor, accounting).await?;
        scope = Some(Scope {
            accounting,
            original_digest: anchor.window().digest(),
            capture_version: pending.version(),
            latest_at: None,
        });
    }
    let scope = scope.ok_or(AccountingError::InvalidStoredData)?;
    if !scope.matches(anchor)
        || scope.accounting.budget() != budget
        || scope.accounting.linked_run().is_some()
    {
        return Err(Failure::ScopeConflict);
    }
    if now.unix_millis() < anchor.captured_at().unix_millis()
        || scope.latest_at.is_some_and(|latest| now < latest)
    {
        return Err(AccountingError::TimestampRegressed.into());
    }
    let deadline = scope.accounting.deadline();
    if now >= deadline {
        return Err(AccountingError::DeadlineReached.into());
    }
    let usage = scope.accounting.usage();
    if has_uncertain_calls(usage) {
        return Err(AccountingError::UncertainProviderCall.into());
    }
    check_call_kind(tx, anchor, kind, wire).await?;
    edit::check_budget(budget, AgentRunUsage::ZERO, usage, kind, wire)?;
    let inserted = tx
        .execute(
            "INSERT INTO intake_phase_reservations
          (task_id, review_revision, window_start, window_end, job_digest, wire_digest,
           wire_bytes, prompt_reserve, output_reserve, reserved_at, call_kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.as_bytes().to_vec(),
                review_coordinate(anchor.review_revision()),
                i64::from(anchor.window().start_byte()),
                i64::from(anchor.window().end_byte()),
                anchor.job_digest().as_bytes().to_vec(),
                wire.digest().as_bytes().to_vec(),
                i64::from(wire.request_bytes()),
                i64::from(wire.prompt_estimate()),
                i64::from(wire.output_reserve()),
                time(now)?,
                edit::call_kind_text(kind),
            ],
        )
        .await
        .map_err(unavailable)?;
    if inserted != 1 {
        return Err(AccountingError::InvalidStoredData.into());
    }
    Ok(deadline)
}

async fn check_call_kind(
    tx: &Transaction,
    anchor: IntakePhaseAnchor,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
) -> Result<(), Failure> {
    let mut rows = tx
        .query(
            "SELECT EXISTS(
             SELECT 1 FROM intake_phase_reservations
             WHERE task_id=?1 AND call_kind=?6 AND (job_digest=?5 OR
               (review_revision=?2 AND window_start=?3 AND window_end=?4))
         ), EXISTS(
             SELECT 1 FROM intake_phase_reservations
             WHERE task_id=?1 AND review_revision=?2 AND window_start=?3 AND window_end=?4
               AND job_digest=?5 AND call_kind='initial' AND outcome='invalid_structure'
               AND wire_digest!=?7
         ), EXISTS(
             SELECT 1 FROM intake_phase_reservations
             WHERE task_id=?1 AND call_kind='initial'
               AND window_start<?4 AND ?3<window_end
         )",
            params![
                anchor.goal().task_id().as_bytes().to_vec(),
                review_coordinate(anchor.review_revision()),
                i64::from(anchor.window().start_byte()),
                i64::from(anchor.window().end_byte()),
                anchor.job_digest().as_bytes().to_vec(),
                edit::call_kind_text(kind),
                wire.digest().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(AccountingError::InvalidStoredData)?;
    if count(&row, 0)? != 0 || (kind == EditPhaseCallKind::Initial && count(&row, 2)? != 0) {
        return Err(AccountingError::AlreadyReserved.into());
    }
    if kind == EditPhaseCallKind::StructuralRepair && count(&row, 1)? != 1 {
        return Err(AccountingError::RepairNotAvailable.into());
    }
    Ok(())
}

async fn finish(
    tx: &Transaction,
    reservation: &IntakePhaseReservation,
    outcome: IntakePhaseFinish<'_>,
    now: AgentRunTimestamp,
) -> Result<Option<Failure>, Failure> {
    let anchor = reservation.anchor();
    let worktree = anchor.worktree();
    let task = anchor.goal().task_id();
    let scope = read_scope(tx, worktree, task)
        .await?
        .ok_or(AccountingError::ObservationConflict)?;
    if !scope.matches(anchor) || scope.accounting.deadline() != reservation.deadline() {
        return Err(AccountingError::ObservationConflict.into());
    }
    let (status, reported_prompt, reported_output, rejection) = match outcome {
        IntakePhaseFinish::Review {
            review,
            prompt_tokens,
            output_tokens,
        } => {
            let (status, rejection) =
                accept_review(tx, reservation, review, prompt_tokens, output_tokens, now).await?;
            (status, Some(prompt_tokens), Some(output_tokens), rejection)
        }
        IntakePhaseFinish::ObservedFailure {
            kind,
            reported_prompt_tokens,
            reported_output_tokens,
        } => (
            rejection_text(kind),
            reported_prompt_tokens,
            reported_output_tokens,
            None,
        ),
    };
    let wire = reservation.wire();
    let changed = tx
        .execute(
            "UPDATE intake_phase_reservations
         SET outcome=?1, finished_at=?2, reported_prompt=?3, reported_output=?4
         WHERE task_id=?5 AND review_revision=?6 AND window_start=?7 AND window_end=?8
           AND job_digest=?9 AND wire_digest=?10 AND wire_bytes=?11 AND prompt_reserve=?12
           AND output_reserve=?13 AND reserved_at=?14 AND call_kind=?15 AND outcome IS NULL",
            params![
                status,
                time(now)?,
                reported_prompt.map(i64::from),
                reported_output.map(i64::from),
                task.as_bytes().to_vec(),
                review_coordinate(anchor.review_revision()),
                i64::from(anchor.window().start_byte()),
                i64::from(anchor.window().end_byte()),
                anchor.job_digest().as_bytes().to_vec(),
                wire.digest().as_bytes().to_vec(),
                i64::from(wire.request_bytes()),
                i64::from(wire.prompt_estimate()),
                i64::from(wire.output_reserve()),
                time(reservation.reserved_at())?,
                edit::call_kind_text(reservation.kind()),
            ],
        )
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        // Any proposed Review append in this transaction is rolled back too.
        return Err(AccountingError::ObservationConflict.into());
    }
    Ok(rejection)
}

async fn accept_review(
    tx: &Transaction,
    reservation: &IntakePhaseReservation,
    review: &GoalIntakeReview,
    prompt_tokens: u32,
    output_tokens: u32,
    now: AgentRunTimestamp,
) -> Result<(&'static str, Option<Failure>), Failure> {
    let anchor = reservation.anchor();
    if now >= reservation.deadline() {
        return Ok(("stale", Some(AccountingError::DeadlineReached.into())));
    }
    match validate_pending(tx, anchor).await {
        Ok(_) => {}
        Err(error @ (Failure::InvalidAnchor | Failure::ReviewConflict)) => {
            return Ok(("stale", Some(error)));
        }
        Err(error) => return Err(error),
    }
    let wire = reservation.wire();
    let overrun = if prompt_tokens > wire.prompt_estimate() {
        Some(AgentBudgetDimension::PromptTokens)
    } else if output_tokens > wire.output_reserve() {
        Some(AgentBudgetDimension::OutputTokens)
    } else {
        None
    };
    if let Some(dimension) = overrun {
        return Ok((
            "budget_overrun",
            Some(AccountingError::BudgetExhausted(dimension).into()),
        ));
    }
    if review
        .batches()
        .last()
        .map(a3_domain::GoalIntakeReviewBatch::window)
        != Some(anchor.window())
    {
        return Ok(("rejected_reply", Some(Failure::InvalidReview)));
    }
    match intake_review::append_in_transaction(
        tx,
        anchor.worktree(),
        anchor.goal().task_id(),
        anchor.capture_version(),
        anchor.review_revision(),
        review,
    )
    .await
    {
        Ok(_) => Ok(("accepted_review", None)),
        // Domain admission happens before any Review writes. A write error still rolls back
        // the whole transaction and leaves the prior reservation unsettled.
        Err(GoalError::InvalidIntakeReview) => Ok(("rejected_reply", Some(Failure::InvalidReview))),
        Err(error) => Err(classify_goal(error)),
    }
}

async fn validate_pending(
    tx: &Transaction,
    anchor: IntakePhaseAnchor,
) -> Result<StoredPendingGoalIntake, Failure> {
    let task = anchor.goal().task_id();
    let worktree = anchor.worktree();
    let Some(GoalIntakeLoadResult::Pending(stored)) =
        intake::load_in_transaction(tx, worktree, task)
            .await
            .map_err(classify_goal)?
    else {
        return Err(Failure::InvalidAnchor);
    };
    let pending = stored.pending();
    if stored.version() != anchor.capture_version()
        || pending.reference() != anchor.goal()
        || pending.created_at() != anchor.captured_at()
        || pending.original().resolve(anchor.window()).is_err()
    {
        return Err(Failure::InvalidAnchor);
    }
    let review = intake_review::load_for_capture(tx, worktree, pending)
        .await
        .map_err(classify_goal)?;
    if review.as_ref().map(GoalIntakeReview::revision) != anchor.review_revision() {
        return Err(Failure::ReviewConflict);
    }
    Ok(stored)
}

struct Scope {
    accounting: IntakePhaseAccounting,
    original_digest: GoalOriginalDigest,
    capture_version: GoalIntakeStoreVersion,
    latest_at: Option<AgentRunTimestamp>,
}

impl Scope {
    fn matches(&self, anchor: IntakePhaseAnchor) -> bool {
        self.accounting.profile() == anchor.model_profile()
            && self.accounting.captured_at() == anchor.captured_at()
            && self.original_digest == anchor.window().digest()
            && self.capture_version == anchor.capture_version()
    }

    fn matches_pending(&self, pending: &PendingGoalIntake) -> bool {
        self.accounting.captured_at() == pending.created_at()
            && self.original_digest == pending.original().digest()
    }
}

async fn insert_scope(
    tx: &Transaction,
    anchor: IntakePhaseAnchor,
    accounting: IntakePhaseAccounting,
) -> Result<(), Failure> {
    let budget = accounting.budget();
    let profile = accounting.profile();
    let changed = tx
        .execute(
            "INSERT INTO intake_phase_budget_scopes
          (task_id, worktree_id, original_digest, capture_version, captured_at, model_profile_id,
           model_profile_version, turn_limit, prompt_limit, output_limit, action_limit,
           repair_limit, deadline)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                anchor.goal().task_id().as_bytes().to_vec(),
                anchor.worktree().as_bytes().to_vec(),
                anchor.window().digest().as_bytes().to_vec(),
                to_i64(anchor.capture_version().get())?,
                to_i64(accounting.captured_at().unix_millis())?,
                profile.id().as_bytes().to_vec(),
                i64::from(profile.version().get()),
                i64::from(budget.turn_limit().get()),
                to_i64(budget.prompt_token_limit().get())?,
                to_i64(budget.output_token_limit().get())?,
                i64::from(budget.action_limit().get()),
                i64::from(budget.repair_limit().get()),
                time(accounting.deadline())?,
            ],
        )
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(AccountingError::InvalidStoredData.into());
    }
    Ok(())
}

async fn read_scope(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<Scope>, Failure> {
    let mut rows = tx.query(
        "SELECT original_digest, capture_version, captured_at, model_profile_id, model_profile_version,
                turn_limit, prompt_limit, output_limit, action_limit, repair_limit, deadline, linked_run_id
         FROM intake_phase_budget_scopes WHERE task_id=?1 AND worktree_id=?2",
        params![task.as_bytes().to_vec(), worktree.as_bytes().to_vec()],
    ).await.map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    let captured = nonnegative(&row, 2)?;
    let captured_at = GoalContractTimestamp::from_unix_millis(captured)
        .map_err(|_| AccountingError::InvalidStoredData)?;
    let deadline = nonnegative(&row, 10)?;
    let duration = deadline
        .checked_sub(captured)
        .ok_or(AccountingError::InvalidStoredData)?;
    let budget = read_budget(&row, duration)?;
    let profile_version = u16::try_from(nonnegative(&row, 4)?)
        .ok()
        .and_then(|value| ModelProfileVersion::from_u16(value).ok())
        .ok_or(AccountingError::InvalidStoredData)?;
    let profile = ModelProfileReference::new(
        ModelProfileId::from_bytes(fixed_bytes(&row, 3)?),
        profile_version,
    );
    let linked_run: Option<Vec<u8>> = row
        .get(11)
        .map_err(|_| AccountingError::InvalidStoredData)?;
    let linked_run = linked_run
        .map(|bytes| {
            let bytes = bytes
                .try_into()
                .map_err(|_| AccountingError::InvalidStoredData)?;
            Ok::<_, AccountingError>(AgentRunId::from_bytes(bytes))
        })
        .transpose()?;
    let (usage, latest_at) = read_usage(tx, task).await?;
    Ok(Some(Scope {
        accounting: IntakePhaseAccounting::reconstruct(
            budget,
            captured_at,
            profile,
            linked_run,
            usage,
        )?,
        original_digest: GoalOriginalDigest::from_bytes(fixed_bytes(&row, 0)?),
        capture_version: GoalIntakeStoreVersion::new(nonnegative(&row, 1)?)
            .map_err(|_| AccountingError::InvalidStoredData)?,
        latest_at,
    }))
}

fn read_budget(row: &Row, duration: u64) -> Result<AgentRunBudget, Failure> {
    let invalid = AccountingError::InvalidStoredData;
    Ok(AgentRunBudget::new(
        AgentTurnLimit::new(count(row, 5)?).map_err(|_| invalid)?,
        AgentTokenLimit::new(nonnegative(row, 6)?).map_err(|_| invalid)?,
        AgentTokenLimit::new(nonnegative(row, 7)?).map_err(|_| invalid)?,
        AgentActionLimit::new(count(row, 8)?).map_err(|_| invalid)?,
        AgentRunDurationLimit::from_millis(duration).map_err(|_| invalid)?,
        AgentRepairLimit::new(count(row, 9)?).map_err(|_| invalid)?,
    ))
}

async fn read_usage(
    tx: &Transaction,
    task: TaskId,
) -> Result<(EditPhaseReservedUsage, Option<AgentRunTimestamp>), Failure> {
    let mut rows = tx
        .query(
            "SELECT COUNT(*), COALESCE(SUM(MAX(prompt_reserve, COALESCE(reported_prompt, 0))), 0),
                COALESCE(SUM(MAX(output_reserve, COALESCE(reported_output, 0))), 0),
                COUNT(CASE WHEN outcome IS NULL THEN 1 END),
                COUNT(CASE WHEN outcome='unknown' THEN 1 END),
                COUNT(CASE WHEN call_kind='structural_repair' THEN 1 END),
                MAX(COALESCE(finished_at, reserved_at))
         FROM intake_phase_reservations WHERE task_id=?1",
            [task.as_bytes().to_vec()],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(AccountingError::InvalidStoredData)?;
    let latest: Option<i64> = row.get(6).map_err(|_| AccountingError::InvalidStoredData)?;
    let latest = latest
        .map(|value| {
            let value = u64::try_from(value).map_err(|_| AccountingError::InvalidStoredData)?;
            AgentRunTimestamp::from_unix_millis(value)
                .map_err(|_| AccountingError::InvalidStoredData)
        })
        .transpose()?;
    Ok((
        EditPhaseReservedUsage {
            calls: count(&row, 0)?,
            repairs: count(&row, 5)?,
            prompt_tokens: nonnegative(&row, 1)?,
            output_tokens: nonnegative(&row, 2)?,
            unobserved_calls: count(&row, 3)?,
            unknown_calls: count(&row, 4)?,
        },
        latest,
    ))
}

async fn validate_owner(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
    scope: &Scope,
) -> Result<(), Failure> {
    match intake::load_in_transaction(tx, worktree, task)
        .await
        .map_err(classify_goal)?
    {
        Some(GoalIntakeLoadResult::Pending(stored))
            if scope.accounting.linked_run().is_none()
                && scope.matches_pending(stored.pending())
                && scope.capture_version == stored.version() =>
        {
            Ok(())
        }
        Some(GoalIntakeLoadResult::Materialized(initial)) => {
            let goal = super::load_revision_from_transaction(
                tx,
                worktree,
                task,
                GoalContractRevision::INITIAL,
            )
            .await
            .map_err(classify_goal)?
            .ok_or(AccountingError::InvalidStoredData)?;
            if goal.reference() == initial
                && goal.created_at() == scope.accounting.captured_at()
                && goal
                    .original()
                    .is_some_and(|original| original.digest() == scope.original_digest)
            {
                Ok(())
            } else {
                Err(AccountingError::InvalidStoredData.into())
            }
        }
        _ => Err(AccountingError::InvalidStoredData.into()),
    }
}

pub(super) async fn validate_promotion(
    tx: &Transaction,
    worktree: WorktreeId,
    pending: &StoredPendingGoalIntake,
    reviewed: bool,
) -> Result<(), GoalError> {
    if read_user_version(tx).await.map_err(GoalError::Read)? < SCHEMA_VERSION {
        return Ok(());
    }
    let Some(scope) = read_scope(tx, worktree, pending.pending().task_id())
        .await
        .map_err(|_| GoalError::InvalidStoredData)?
    else {
        return Ok(());
    };
    if !reviewed
        || !scope.matches_pending(pending.pending())
        || scope.capture_version != pending.version()
        || scope.accounting.linked_run().is_some()
        || !eligible_for_carryover(scope.accounting)
    {
        return Err(GoalError::InvalidIntakePromotion);
    }
    Ok(())
}

pub(crate) async fn link_run(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
) -> Result<(), RunError> {
    let version = read_user_version(tx).await.map_err(RunError::Read)?;
    if version < SCHEMA_VERSION {
        return Ok(());
    }
    let task = run.goal_contract().task_id();
    let Some(scope) = read_scope(tx, worktree, task)
        .await
        .map_err(|_| RunError::InvalidStoredData)?
    else {
        return Ok(());
    };
    validate_owner(tx, worktree, task, &scope)
        .await
        .map_err(|_| RunError::InvalidStoredData)?;
    let current = super::task_current_revision(tx, worktree, task)
        .await
        .map_err(RunError::GoalContract)?;
    let mut usage = scope.accounting.usage();
    let mut latest_at = scope.latest_at;
    if version >= initial_plan::SCHEMA_VERSION {
        let summary = initial_plan::read_summary(tx, task)
            .await
            .map_err(|_| RunError::InvalidStoredData)?;
        initial_plan::validate_run_link(tx, worktree, run, &summary).await?;
        usage = edit::merge_phase_usage(usage, summary.usage)
            .map_err(|_| RunError::InvalidStoredData)?;
        latest_at = latest_at.max(summary.latest_at);
    }
    if run.goal_contract().revision() != GoalContractRevision::INITIAL
        || current != Some(GoalContractRevision::INITIAL)
        || run.model_profile() != Some(scope.accounting.profile())
        || scope.accounting.linked_run().is_some()
        || !eligible_usage(scope.accounting.budget(), usage)
        || latest_at.is_some_and(|latest| run.created_at() < latest)
        || scope.accounting.run_budget(run.created_at()).ok() != Some(run.budget())
    {
        return Err(RunError::InvalidInput);
    }
    let changed = tx
        .execute(
            "UPDATE intake_phase_budget_scopes SET linked_run_id=?1
         WHERE task_id=?2 AND worktree_id=?3 AND linked_run_id IS NULL",
            params![
                run.id().as_bytes().to_vec(),
                task.as_bytes().to_vec(),
                worktree.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(RunError::Write)?;
    if changed != 1 {
        return Err(RunError::InvalidInput);
    }
    Ok(())
}

pub(crate) async fn linked_usage(
    tx: &Transaction,
    worktree: WorktreeId,
    run: &AgentRun,
) -> Result<Option<EditPhaseReservedUsage>, AccountingError> {
    let version = read_user_version(tx)
        .await
        .map_err(|_| AccountingError::Unavailable)?;
    if version < SCHEMA_VERSION {
        return Ok(None);
    }
    let task = run.goal_contract().task_id();
    let Some(scope) = read_scope(tx, worktree, task)
        .await
        .map_err(|error| match error {
            Failure::Accounting(error) => error,
            _ => AccountingError::InvalidStoredData,
        })?
    else {
        return Ok(None);
    };
    if scope.accounting.linked_run() != Some(run.id()) {
        return Err(AccountingError::InvalidStoredData);
    }
    let mut usage = scope.accounting.usage();
    if version >= initial_plan::SCHEMA_VERSION {
        let plan = initial_plan::read_summary(tx, task)
            .await
            .map_err(|_| AccountingError::InvalidStoredData)?;
        usage = edit::merge_phase_usage(usage, plan.usage)?;
    }
    Ok(Some(usage))
}

fn eligible_for_carryover(accounting: IntakePhaseAccounting) -> bool {
    eligible_usage(accounting.budget(), accounting.usage())
}

fn eligible_usage(budget: AgentRunBudget, usage: EditPhaseReservedUsage) -> bool {
    !has_uncertain_calls(usage)
        && usage.calls <= budget.turn_limit().get()
        && usage.repairs <= budget.repair_limit().get()
        && usage.prompt_tokens <= budget.prompt_token_limit().get()
        && usage.output_tokens <= budget.output_token_limit().get()
}

fn has_uncertain_calls(usage: EditPhaseReservedUsage) -> bool {
    usage.unobserved_calls != 0 || usage.unknown_calls != 0
}

const fn rejection_text(kind: IntakePhaseRejection) -> &'static str {
    match kind {
        IntakePhaseRejection::InvalidStructure => "invalid_structure",
        IntakePhaseRejection::RejectedReply => "rejected_reply",
        IntakePhaseRejection::ProviderFailure => "provider_failure",
        IntakePhaseRejection::Cancelled => "cancelled",
        IntakePhaseRejection::Unknown => "unknown",
    }
}

fn review_coordinate(revision: Option<GoalIntakeReviewRevision>) -> i64 {
    revision.map_or(0, |value| i64::from(value.get()))
}

fn fixed_bytes(row: &Row, index: i32) -> Result<[u8; 32], Failure> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(|_| AccountingError::InvalidStoredData)?;
    bytes
        .try_into()
        .map_err(|_| AccountingError::InvalidStoredData.into())
}

fn nonnegative(row: &Row, index: i32) -> Result<u64, Failure> {
    let value: i64 = row
        .get(index)
        .map_err(|_| AccountingError::InvalidStoredData)?;
    u64::try_from(value).map_err(|_| AccountingError::InvalidStoredData.into())
}

fn count(row: &Row, index: i32) -> Result<u32, Failure> {
    u32::try_from(nonnegative(row, index)?).map_err(|_| AccountingError::InvalidStoredData.into())
}

fn to_i64(value: u64) -> Result<i64, Failure> {
    i64::try_from(value).map_err(|_| AccountingError::InvalidStoredData.into())
}

fn time(value: AgentRunTimestamp) -> Result<i64, Failure> {
    to_i64(value.unix_millis())
}

fn classify_goal(error: GoalError) -> Failure {
    match error.classify() {
        a3_application::GoalContractStoreFailure::Unavailable => {
            AccountingError::Unavailable.into()
        }
        a3_application::GoalContractStoreFailure::IntakeReviewVersionConflict => {
            Failure::ReviewConflict
        }
        a3_application::GoalContractStoreFailure::InvalidIntakeReview => Failure::InvalidReview,
        _ => AccountingError::InvalidStoredData.into(),
    }
}

fn unavailable(_: libsql::Error) -> Failure {
    AccountingError::Unavailable.into()
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
