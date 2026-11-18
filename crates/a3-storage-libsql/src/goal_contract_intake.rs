//! Captured source and initial Goal promotion share the existing Goal transaction boundary.

use super::{
    GoalContractRepositoryError as Error, classify_unexpected_constraint, close_read_transaction,
    close_write_transaction, create_initial_in_transaction, read_i64, read_id, read_timestamp,
    task_current_revision, timestamp_to_i64,
};
use a3_application::{GoalIntakeLoadResult, GoalIntakeStoreVersion, StoredPendingGoalIntake};
use a3_domain::{
    GoalContract, GoalContractReference, GoalOriginalDigest, GoalOriginalRequest,
    MAX_GOAL_ORIGINAL_BYTES, PendingGoalIntake, TaskId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};

pub(crate) async fn list_pending(
    connection: &Connection,
    worktree: WorktreeId,
    after: Option<TaskId>,
) -> Result<a3_application::PendingGoalIntakePage, Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        require_supported_schema(&tx).await?;
        let cursor = after.map_or_else(Vec::new, |id| id.as_bytes().to_vec());
        let mut rows = tx
            .query(
                "SELECT length(task_id),CASE WHEN length(task_id)=32 THEN task_id END
             FROM goal_pending_intakes WHERE worktree_id=?1 AND (?2=X'' OR task_id>?2)
             ORDER BY task_id LIMIT ?3",
                params![
                    worktree.as_bytes().to_vec(),
                    cursor,
                    i64::try_from(a3_application::PENDING_GOAL_INTAKE_PAGE_SIZE + 1)
                        .map_err(|_| Error::ResourceLimit)?
                ],
            )
            .await
            .map_err(Error::Read)?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::Read)? {
            if ids.len() > a3_application::PENDING_GOAL_INTAKE_PAGE_SIZE {
                return Err(Error::ResourceLimit);
            }
            if read_i64(&row, 0)? != 32 {
                return Err(Error::InvalidStoredData);
            }
            ids.push(TaskId::from_bytes(read_id(&row, 1)?));
        }
        a3_application::PendingGoalIntakePage::from_lookahead(after, ids)
            .map_err(|_| Error::InvalidStoredData)
    }
    .await;
    close_read_transaction(tx, result).await
}

pub(crate) async fn capture(
    connection: &Connection,
    worktree: WorktreeId,
    pending: &PendingGoalIntake,
) -> Result<GoalIntakeStoreVersion, Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        require_supported_schema(&tx).await?;
        if load_in_transaction(&tx, worktree, pending.task_id())
            .await?
            .is_some()
        {
            return Err(Error::TaskAlreadyExists);
        }
        let original = pending.original();
        tx.execute(
            "INSERT INTO goal_pending_intakes
            (task_id,worktree_id,original_digest,byte_length,original_bytes,
             created_at_unix_millis,store_version) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                pending.task_id().as_bytes().to_vec(),
                worktree.as_bytes().to_vec(),
                original.digest().as_bytes().to_vec(),
                i64::try_from(original.as_bytes().len()).map_err(|_| Error::ResourceLimit)?,
                original.as_bytes().to_vec(),
                timestamp_to_i64(pending.created_at())?,
                version_i64(GoalIntakeStoreVersion::INITIAL)?
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
        Ok(GoalIntakeStoreVersion::INITIAL)
    }
    .await;
    close_write_transaction(tx, result).await
}

pub(crate) async fn load(
    connection: &Connection,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<GoalIntakeLoadResult>, Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        require_supported_schema(&tx).await?;
        load_in_transaction(&tx, worktree, task).await
    }
    .await;
    close_read_transaction(tx, result).await
}

pub(crate) async fn promote(
    connection: &Connection,
    worktree: WorktreeId,
    task: TaskId,
    expected: GoalIntakeStoreVersion,
    goal: &GoalContract,
) -> Result<(), Error> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(Error::Begin)?;
    let result = async {
        require_supported_schema(&tx).await?;
        let Some(GoalIntakeLoadResult::Pending(stored)) =
            load_in_transaction(&tx, worktree, task).await?
        else {
            return Err(Error::IntakeNotPending);
        };
        if stored.version() != expected {
            return Err(Error::IntakeVersionConflict);
        }
        stored
            .pending()
            .validate_promotion(goal)
            .map_err(|_| Error::InvalidIntakePromotion)?;
        let reviewed = super::intake_review::validate_promotion_if_present(
            &tx,
            worktree,
            stored.pending(),
            goal,
        )
        .await?;
        super::phase_reservations::validate_promotion(&tx, worktree, &stored, reviewed).await?;
        // Delete first so the ordinary Task INSERT guard also prevents every bypass.
        // This is invisible to other readers until COMMIT; every failure restores it.
        let changed = tx
            .execute(
                "DELETE FROM goal_pending_intakes
            WHERE task_id=?1 AND worktree_id=?2 AND store_version=?3 AND original_digest=?4",
                params![
                    task.as_bytes().to_vec(),
                    worktree.as_bytes().to_vec(),
                    version_i64(expected)?,
                    stored.pending().original().digest().as_bytes().to_vec()
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        if changed != 1 {
            return Err(Error::IntakeVersionConflict);
        }
        create_initial_in_transaction(&tx, worktree, goal).await
    }
    .await;
    close_write_transaction(tx, result).await
}

pub(super) async fn reject_pending_for_create(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<(), Error> {
    // The unchanged production V42 path has no pending-intake table.
    if crate::migration::read_user_version(tx)
        .await
        .map_err(Error::Read)?
        < 45
    {
        return Ok(());
    }
    if load_pending(tx, worktree, task).await?.is_some() {
        return Err(Error::TaskAlreadyExists);
    }
    Ok(())
}

async fn require_supported_schema(tx: &Transaction) -> Result<(), Error> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(Error::Read)?
        < 45
    {
        return Err(Error::OriginalSchemaUnsupported);
    }
    Ok(())
}

pub(super) async fn load_in_transaction(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<GoalIntakeLoadResult>, Error> {
    let pending = load_pending(tx, worktree, task).await?;
    let current = task_current_revision(tx, worktree, task).await?;
    if current.is_some() {
        if pending.is_some() {
            return Err(Error::InvalidStoredData);
        }
        let mut rows = tx
            .query(
                "SELECT EXISTS(SELECT 1 FROM goal_contract_revisions
            WHERE task_id=?1 AND revision=1)",
                params![task.as_bytes().to_vec()],
            )
            .await
            .map_err(Error::Read)?;
        let row = rows
            .next()
            .await
            .map_err(Error::Read)?
            .ok_or(Error::InvalidStoredData)?;
        if read_i64(&row, 0)? != 1 || rows.next().await.map_err(Error::Read)?.is_some() {
            return Err(Error::InvalidStoredData);
        }
        return Ok(Some(GoalIntakeLoadResult::Materialized(
            GoalContractReference::initial(task),
        )));
    }
    Ok(pending.map(GoalIntakeLoadResult::Pending))
}

async fn load_pending(
    tx: &Transaction,
    worktree: WorktreeId,
    task: TaskId,
) -> Result<Option<StoredPendingGoalIntake>, Error> {
    let mut rows = tx
        .query(
            "SELECT byte_length,length(original_bytes),
        CASE WHEN length(original_bytes) BETWEEN 1 AND 65536 THEN original_bytes END,
        CASE WHEN length(original_digest)=32 THEN original_digest END,
        length(original_digest),created_at_unix_millis,store_version
        FROM goal_pending_intakes WHERE task_id=?1 AND worktree_id=?2",
            params![task.as_bytes().to_vec(), worktree.as_bytes().to_vec()],
        )
        .await
        .map_err(Error::Read)?;
    let Some(row) = rows.next().await.map_err(Error::Read)? else {
        return Ok(None);
    };
    let declared = usize::try_from(read_i64(&row, 0)?).map_err(|_| Error::InvalidStoredData)?;
    let actual = usize::try_from(read_i64(&row, 1)?).map_err(|_| Error::InvalidStoredData)?;
    if actual > MAX_GOAL_ORIGINAL_BYTES {
        return Err(Error::ResourceLimit);
    }
    if actual == 0 || declared != actual || read_i64(&row, 4)? != 32 {
        return Err(Error::InvalidStoredData);
    }
    let digest = GoalOriginalDigest::from_bytes(read_id(&row, 3)?);
    let created = read_timestamp(&row, 5)?;
    let version = GoalIntakeStoreVersion::new(
        u64::try_from(read_i64(&row, 6)?).map_err(|_| Error::InvalidStoredData)?,
    )
    .map_err(|_| Error::InvalidStoredData)?;
    let bytes: Vec<u8> = row.get(2).map_err(Error::Read)?;
    if bytes.len() != actual || rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    let original =
        GoalOriginalRequest::reconstruct(GoalContractReference::initial(task), bytes, digest)
            .map_err(|_| Error::InvalidStoredData)?;
    let pending =
        PendingGoalIntake::new(original, created).map_err(|_| Error::InvalidStoredData)?;
    Ok(Some(StoredPendingGoalIntake::new(pending, version)))
}

fn version_i64(version: GoalIntakeStoreVersion) -> Result<i64, Error> {
    i64::try_from(version.get()).map_err(|_| Error::InvalidInput)
}
