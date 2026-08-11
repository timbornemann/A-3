use crate::{catalog::is_corruption, goal_contract_repository, task_ledger_repository};
use a3_application::{
    GoalContractStoreFailure, TaskLedgerStoreFailure, TaskLensWorkspaceControl,
    TaskLensWorkspaceFailure, TaskLensWorkspaceGoalPage, TaskLensWorkspaceTask,
    TaskLensWorkspaceTaskLimit,
};
use a3_domain::{GoalContractRevision, TaskId, WorktreeId};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

pub(crate) async fn list_current_goal_contracts(
    connection: &Connection,
    worktree_id: WorktreeId,
    limit: TaskLensWorkspaceTaskLimit,
    control: &dyn TaskLensWorkspaceControl,
) -> Result<TaskLensWorkspaceGoalPage, TaskLensWorkspaceRepositoryError> {
    check_cancelled(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Begin)?;
    let result = async {
        let query_limit = i64::from(limit.get())
            .checked_add(1)
            .ok_or(TaskLensWorkspaceRepositoryError::InvalidStoredData)?;
        let mut rows = transaction
            .query(
                "SELECT task_id, current_goal_revision FROM tasks
                 WHERE worktree_id = ?1 ORDER BY task_id LIMIT ?2",
                params![worktree_id.as_bytes().to_vec(), query_limit],
            )
            .await
            .map_err(TaskLensWorkspaceRepositoryError::Read)?;
        let mut references = Vec::with_capacity(usize::from(limit.get()) + 1);
        while let Some(row) = rows
            .next()
            .await
            .map_err(TaskLensWorkspaceRepositoryError::Read)?
        {
            check_cancelled(control)?;
            references.push((read_task_id(&row, 0)?, read_revision(&row, 1)?));
        }
        drop(rows);
        let truncated = references.len() > usize::from(limit.get());
        references.truncate(usize::from(limit.get()));
        let mut goals = Vec::with_capacity(references.len());
        for (task_id, revision) in references {
            check_cancelled(control)?;
            let goal = goal_contract_repository::load_revision_from_transaction(
                &transaction,
                worktree_id,
                task_id,
                revision,
            )
            .await
            .map_err(TaskLensWorkspaceRepositoryError::Goal)?
            .ok_or(TaskLensWorkspaceRepositoryError::InvalidStoredData)?;
            goals.push(goal);
        }
        TaskLensWorkspaceGoalPage::new(goals, truncated, limit)
            .map_err(|_| TaskLensWorkspaceRepositoryError::InvalidStoredData)
    }
    .await;
    close_read_transaction(transaction, result).await
}

pub(crate) async fn load_current_task(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
    control: &dyn TaskLensWorkspaceControl,
) -> Result<Option<TaskLensWorkspaceTask>, TaskLensWorkspaceRepositoryError> {
    check_cancelled(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Begin)?;
    let result = async {
        let Some(revision) =
            read_current_revision(&transaction, worktree_id, task_id, control).await?
        else {
            return Ok(None);
        };
        check_cancelled(control)?;
        let goal_contract = goal_contract_repository::load_revision_from_transaction(
            &transaction,
            worktree_id,
            task_id,
            revision,
        )
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Goal)?
        .ok_or(TaskLensWorkspaceRepositoryError::InvalidStoredData)?;
        check_cancelled(control)?;
        let task_ledger =
            task_ledger_repository::load_from_transaction(&transaction, worktree_id, task_id)
                .await
                .map_err(TaskLensWorkspaceRepositoryError::Ledger)?;
        check_cancelled(control)?;
        Ok(Some(TaskLensWorkspaceTask::new(goal_contract, task_ledger)))
    }
    .await;
    close_read_transaction(transaction, result).await
}

async fn read_current_revision(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
    control: &dyn TaskLensWorkspaceControl,
) -> Result<Option<GoalContractRevision>, TaskLensWorkspaceRepositoryError> {
    check_cancelled(control)?;
    let mut rows = transaction
        .query(
            "SELECT current_goal_revision FROM tasks
             WHERE task_id = ?1 AND worktree_id = ?2",
            params![task_id.as_bytes().to_vec(), worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Read)?;
    let revision = rows
        .next()
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Read)?
        .map(|row| read_revision(&row, 0))
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(TaskLensWorkspaceRepositoryError::Read)?
        .is_some()
    {
        return Err(TaskLensWorkspaceRepositoryError::InvalidStoredData);
    }
    Ok(revision)
}

fn read_task_id(row: &libsql::Row, index: i32) -> Result<TaskId, TaskLensWorkspaceRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(TaskLensWorkspaceRepositoryError::Read)?;
    let bytes = <[u8; 32]>::try_from(bytes)
        .map_err(|_| TaskLensWorkspaceRepositoryError::InvalidStoredData)?;
    Ok(TaskId::from_bytes(bytes))
}

fn read_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<GoalContractRevision, TaskLensWorkspaceRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(TaskLensWorkspaceRepositoryError::Read)?;
    let value =
        u32::try_from(value).map_err(|_| TaskLensWorkspaceRepositoryError::InvalidStoredData)?;
    GoalContractRevision::new(value)
        .map_err(|_| TaskLensWorkspaceRepositoryError::InvalidStoredData)
}

fn check_cancelled(
    control: &dyn TaskLensWorkspaceControl,
) -> Result<(), TaskLensWorkspaceRepositoryError> {
    if control.is_cancelled() {
        Err(TaskLensWorkspaceRepositoryError::Cancelled)
    } else {
        Ok(())
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, TaskLensWorkspaceRepositoryError>,
) -> Result<T, TaskLensWorkspaceRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(TaskLensWorkspaceRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(TaskLensWorkspaceRepositoryError::Rollback(source)),
        },
    }
}

#[derive(Debug)]
pub(crate) enum TaskLensWorkspaceRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Goal(goal_contract_repository::GoalContractRepositoryError),
    Ledger(task_ledger_repository::TaskLedgerRepositoryError),
    InvalidStoredData,
    Cancelled,
}

impl TaskLensWorkspaceRepositoryError {
    pub(crate) fn classify(&self) -> TaskLensWorkspaceFailure {
        match self {
            Self::Goal(error) => map_goal_failure(error.classify()),
            Self::Ledger(error) => map_ledger_failure(error.classify()),
            Self::InvalidStoredData => TaskLensWorkspaceFailure::InvalidStoredData,
            Self::Cancelled => TaskLensWorkspaceFailure::Cancelled,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    TaskLensWorkspaceFailure::Corrupt
                } else {
                    TaskLensWorkspaceFailure::Unavailable
                }
            }
        }
    }
}

const fn map_goal_failure(failure: GoalContractStoreFailure) -> TaskLensWorkspaceFailure {
    match failure {
        GoalContractStoreFailure::Unavailable => TaskLensWorkspaceFailure::Unavailable,
        GoalContractStoreFailure::Corrupt => TaskLensWorkspaceFailure::Corrupt,
        GoalContractStoreFailure::UnsupportedSchema => TaskLensWorkspaceFailure::UnsupportedSchema,
        GoalContractStoreFailure::InvalidStoredData
        | GoalContractStoreFailure::TaskAlreadyExists
        | GoalContractStoreFailure::TaskNotFound
        | GoalContractStoreFailure::RevisionConflict => TaskLensWorkspaceFailure::InvalidStoredData,
    }
}

const fn map_ledger_failure(failure: TaskLedgerStoreFailure) -> TaskLensWorkspaceFailure {
    match failure {
        TaskLedgerStoreFailure::Unavailable => TaskLensWorkspaceFailure::Unavailable,
        TaskLedgerStoreFailure::Corrupt => TaskLensWorkspaceFailure::Corrupt,
        TaskLedgerStoreFailure::UnsupportedSchema => TaskLensWorkspaceFailure::UnsupportedSchema,
        TaskLedgerStoreFailure::InvalidStoredData
        | TaskLedgerStoreFailure::LedgerAlreadyExists
        | TaskLedgerStoreFailure::TaskNotFound
        | TaskLedgerStoreFailure::VersionConflict => TaskLensWorkspaceFailure::InvalidStoredData,
    }
}

impl fmt::Display for TaskLensWorkspaceRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "Task Lens workspace transaction could not begin",
            Self::Read(_) => "Task Lens workspace data could not be read",
            Self::Commit(_) => "Task Lens workspace transaction could not commit",
            Self::Rollback(_) => "Task Lens workspace transaction could not roll back",
            Self::Goal(_) => "Task Lens Goal Contract projection was invalid",
            Self::Ledger(_) => "Task Lens Task Ledger projection was invalid",
            Self::InvalidStoredData => "Task Lens workspace data was invalid",
            Self::Cancelled => "Task Lens workspace read was cancelled",
        })
    }
}

impl Error for TaskLensWorkspaceRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            Self::Goal(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::InvalidStoredData | Self::Cancelled => None,
        }
    }
}
