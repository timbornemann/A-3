use crate::catalog::is_corruption;
use a3_application::GoalContractStoreFailure;
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, GoalConstraint, GoalContract, GoalContractDraft,
    GoalContractRevision, GoalContractTimestamp, GoalObjective, GoalRevisionReason, NonGoal,
    SuccessVerification, TaskId, UserDecision, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;
const MAX_COLLECTION_ITEMS: usize = 64;

pub(crate) async fn create(
    connection: &Connection,
    worktree_id: WorktreeId,
    contract: &GoalContract,
) -> Result<(), GoalContractRepositoryError> {
    if contract.revision() != GoalContractRevision::INITIAL
        || contract.previous_revision().is_some()
        || contract.revision_reason().is_some()
    {
        return Err(GoalContractRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(GoalContractRepositoryError::Begin)?;
    let result = async {
        if task_current_revision(&transaction, worktree_id, contract.task_id())
            .await?
            .is_some()
        {
            return Err(GoalContractRepositoryError::TaskAlreadyExists);
        }
        transaction
            .execute(
                "INSERT INTO tasks (task_id, worktree_id, created_at_unix_millis,\n\
                 current_goal_revision) VALUES (?1, ?2, ?3, ?4)",
                params![
                    contract.task_id().as_bytes().to_vec(),
                    worktree_id.as_bytes().to_vec(),
                    timestamp_to_i64(contract.created_at())?,
                    i64::from(contract.revision().get())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        write_revision(&transaction, contract).await
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn append_revision(
    connection: &Connection,
    worktree_id: WorktreeId,
    contract: &GoalContract,
) -> Result<(), GoalContractRepositoryError> {
    let Some(previous_revision) = contract.previous_revision() else {
        return Err(GoalContractRepositoryError::InvalidInput);
    };
    if contract.revision() == GoalContractRevision::INITIAL
        || contract.revision_reason().is_none()
        || previous_revision.get().checked_add(1) != Some(contract.revision().get())
    {
        return Err(GoalContractRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(GoalContractRepositoryError::Begin)?;
    let result = async {
        let current = task_current_revision(&transaction, worktree_id, contract.task_id())
            .await?
            .ok_or(GoalContractRepositoryError::TaskNotFound)?;
        if current != previous_revision {
            return Err(GoalContractRepositoryError::RevisionConflict);
        }
        let previous_created_at =
            revision_created_at(&transaction, contract.task_id(), previous_revision)
                .await?
                .ok_or(GoalContractRepositoryError::InvalidStoredData)?;
        if contract.created_at() < previous_created_at {
            return Err(GoalContractRepositoryError::InvalidInput);
        }
        write_revision(&transaction, contract).await?;
        let changed = transaction
            .execute(
                "UPDATE tasks SET current_goal_revision = ?1\n\
                 WHERE task_id = ?2 AND worktree_id = ?3 AND current_goal_revision = ?4",
                params![
                    i64::from(contract.revision().get()),
                    contract.task_id().as_bytes().to_vec(),
                    worktree_id.as_bytes().to_vec(),
                    i64::from(previous_revision.get())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
        if changed != 1 {
            return Err(GoalContractRepositoryError::RevisionConflict);
        }
        Ok(())
    }
    .await;
    close_write_transaction(transaction, result).await
}

pub(crate) async fn load_current(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<GoalContract>, GoalContractRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(GoalContractRepositoryError::Begin)?;
    let result = async {
        let Some(revision) = task_current_revision(&transaction, worktree_id, task_id).await?
        else {
            return Ok(None);
        };
        load_revision_from_transaction(&transaction, worktree_id, task_id, revision).await
    }
    .await;
    close_read_transaction(transaction, result).await
}

pub(crate) async fn load_revision(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
    revision: GoalContractRevision,
) -> Result<Option<GoalContract>, GoalContractRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(GoalContractRepositoryError::Begin)?;
    let result = load_revision_from_transaction(&transaction, worktree_id, task_id, revision).await;
    close_read_transaction(transaction, result).await
}

async fn write_revision(
    transaction: &Transaction,
    contract: &GoalContract,
) -> Result<(), GoalContractRepositoryError> {
    transaction
        .execute(
            "INSERT INTO goal_contract_revisions (\n\
             task_id, revision, previous_revision, objective, success_verification,\n\
             revision_reason, created_at_unix_millis\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                contract.task_id().as_bytes().to_vec(),
                i64::from(contract.revision().get()),
                contract
                    .previous_revision()
                    .map(|value| i64::from(value.get())),
                contract.draft().objective().as_str(),
                contract.draft().success_verification().as_str(),
                contract.revision_reason().map(GoalRevisionReason::as_str),
                timestamp_to_i64(contract.created_at())?
            ],
        )
        .await
        .map_err(classify_unexpected_constraint)?;
    write_acceptance_criteria(transaction, contract).await?;
    write_text_items(
        transaction,
        "goal_contract_constraints",
        contract,
        contract
            .draft()
            .constraints()
            .iter()
            .map(GoalConstraint::as_str),
    )
    .await?;
    write_text_items(
        transaction,
        "goal_contract_non_goals",
        contract,
        contract.draft().non_goals().iter().map(NonGoal::as_str),
    )
    .await?;
    write_text_items(
        transaction,
        "goal_contract_user_decisions",
        contract,
        contract
            .draft()
            .user_decisions()
            .iter()
            .map(UserDecision::as_str),
    )
    .await
}

async fn write_acceptance_criteria(
    transaction: &Transaction,
    contract: &GoalContract,
) -> Result<(), GoalContractRepositoryError> {
    for (index, criterion) in contract.draft().acceptance_criteria().iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO acceptance_criteria (\n\
                 task_id, goal_revision, item_sequence, criterion_id, statement, requirement\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    contract.task_id().as_bytes().to_vec(),
                    i64::from(contract.revision().get()),
                    sequence_to_i64(index)?,
                    criterion.id().as_bytes().to_vec(),
                    criterion.statement().as_str(),
                    requirement_text(criterion.requirement())
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}

async fn write_text_items<'a>(
    transaction: &Transaction,
    table: &str,
    contract: &GoalContract,
    items: impl Iterator<Item = &'a str>,
) -> Result<(), GoalContractRepositoryError> {
    let sql = format!(
        "INSERT INTO {table} (task_id, goal_revision, item_sequence, statement)\n\
         VALUES (?1, ?2, ?3, ?4)"
    );
    for (index, statement) in items.enumerate() {
        transaction
            .execute(
                &sql,
                params![
                    contract.task_id().as_bytes().to_vec(),
                    i64::from(contract.revision().get()),
                    sequence_to_i64(index)?,
                    statement
                ],
            )
            .await
            .map_err(classify_unexpected_constraint)?;
    }
    Ok(())
}

pub(crate) async fn load_revision_from_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
    revision: GoalContractRevision,
) -> Result<Option<GoalContract>, GoalContractRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT r.previous_revision, r.objective, r.success_verification,\n\
             r.revision_reason, r.created_at_unix_millis\n\
             FROM tasks t JOIN goal_contract_revisions r ON r.task_id = t.task_id\n\
             WHERE t.task_id = ?1 AND t.worktree_id = ?2 AND r.revision = ?3",
            params![
                task_id.as_bytes().to_vec(),
                worktree_id.as_bytes().to_vec(),
                i64::from(revision.get())
            ],
        )
        .await
        .map_err(GoalContractRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let previous_revision = read_optional_revision(&row, 0)?;
    let objective = GoalObjective::try_from_string(read_text(&row, 1)?)
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let success_verification = SuccessVerification::try_from_string(read_text(&row, 2)?)
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let revision_reason = read_optional_text(&row, 3)?
        .map(GoalRevisionReason::try_from_string)
        .transpose()
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let created_at = read_timestamp(&row, 4)?;
    if rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
        .is_some()
    {
        return Err(GoalContractRepositoryError::InvalidStoredData);
    }

    let acceptance_criteria = read_acceptance_criteria(transaction, task_id, revision).await?;
    let constraints = read_text_items(transaction, "goal_contract_constraints", task_id, revision)
        .await?
        .into_iter()
        .map(GoalConstraint::try_from_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let non_goals = read_text_items(transaction, "goal_contract_non_goals", task_id, revision)
        .await?
        .into_iter()
        .map(NonGoal::try_from_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let user_decisions = read_text_items(
        transaction,
        "goal_contract_user_decisions",
        task_id,
        revision,
    )
    .await?
    .into_iter()
    .map(UserDecision::try_from_string)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    let draft = GoalContractDraft::new(
        objective,
        acceptance_criteria,
        constraints,
        non_goals,
        user_decisions,
        success_verification,
    )
    .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    GoalContract::reconstruct(
        task_id,
        revision,
        previous_revision,
        revision_reason,
        draft,
        created_at,
    )
    .map(Some)
    .map_err(|_| GoalContractRepositoryError::InvalidStoredData)
}

async fn read_acceptance_criteria(
    transaction: &Transaction,
    task_id: TaskId,
    revision: GoalContractRevision,
) -> Result<Vec<AcceptanceCriterion>, GoalContractRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT item_sequence, criterion_id, statement, requirement FROM acceptance_criteria\n\
             WHERE task_id = ?1 AND goal_revision = ?2 ORDER BY item_sequence",
            params![task_id.as_bytes().to_vec(), i64::from(revision.get())],
        )
        .await
        .map_err(GoalContractRepositoryError::Read)?;
    let mut criteria = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
    {
        validate_sequence(&row, criteria.len())?;
        if criteria.len() == MAX_COLLECTION_ITEMS {
            return Err(GoalContractRepositoryError::ResourceLimit);
        }
        criteria.push(AcceptanceCriterion::with_requirement(
            AcceptanceCriterionId::from_bytes(read_id(&row, 1)?),
            AcceptanceCriterionStatement::try_from_string(read_text(&row, 2)?)
                .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?,
            parse_requirement(&read_text(&row, 3)?)?,
        ));
    }
    Ok(criteria)
}

const fn requirement_text(requirement: AcceptanceCriterionRequirement) -> &'static str {
    match requirement {
        AcceptanceCriterionRequirement::Must => "must",
        AcceptanceCriterionRequirement::Should => "should",
    }
}

fn parse_requirement(
    value: &str,
) -> Result<AcceptanceCriterionRequirement, GoalContractRepositoryError> {
    match value {
        "must" => Ok(AcceptanceCriterionRequirement::Must),
        "should" => Ok(AcceptanceCriterionRequirement::Should),
        _ => Err(GoalContractRepositoryError::InvalidStoredData),
    }
}

async fn read_text_items(
    transaction: &Transaction,
    table: &str,
    task_id: TaskId,
    revision: GoalContractRevision,
) -> Result<Vec<String>, GoalContractRepositoryError> {
    let sql = format!(
        "SELECT item_sequence, statement FROM {table}\n\
         WHERE task_id = ?1 AND goal_revision = ?2 ORDER BY item_sequence"
    );
    let mut rows = transaction
        .query(
            &sql,
            params![task_id.as_bytes().to_vec(), i64::from(revision.get())],
        )
        .await
        .map_err(GoalContractRepositoryError::Read)?;
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
    {
        validate_sequence(&row, items.len())?;
        if items.len() == MAX_COLLECTION_ITEMS {
            return Err(GoalContractRepositoryError::ResourceLimit);
        }
        items.push(read_text(&row, 1)?);
    }
    Ok(items)
}

async fn task_current_revision(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<GoalContractRevision>, GoalContractRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT current_goal_revision FROM tasks WHERE task_id = ?1 AND worktree_id = ?2",
            params![task_id.as_bytes().to_vec(), worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(GoalContractRepositoryError::Read)?;
    let revision = rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
        .map(|row| read_revision(&row, 0))
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
        .is_some()
    {
        return Err(GoalContractRepositoryError::InvalidStoredData);
    }
    Ok(revision)
}

async fn revision_created_at(
    transaction: &Transaction,
    task_id: TaskId,
    revision: GoalContractRevision,
) -> Result<Option<GoalContractTimestamp>, GoalContractRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT created_at_unix_millis FROM goal_contract_revisions\n\
             WHERE task_id = ?1 AND revision = ?2",
            params![task_id.as_bytes().to_vec(), i64::from(revision.get())],
        )
        .await
        .map_err(GoalContractRepositoryError::Read)?;
    let timestamp = rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
        .map(|row| read_timestamp(&row, 0))
        .transpose()?;
    if rows
        .next()
        .await
        .map_err(GoalContractRepositoryError::Read)?
        .is_some()
    {
        return Err(GoalContractRepositoryError::InvalidStoredData);
    }
    Ok(timestamp)
}

fn validate_sequence(
    row: &libsql::Row,
    zero_based_index: usize,
) -> Result<(), GoalContractRepositoryError> {
    if read_i64(row, 0)? == sequence_to_i64(zero_based_index)? {
        Ok(())
    } else {
        Err(GoalContractRepositoryError::InvalidStoredData)
    }
}

fn sequence_to_i64(zero_based_index: usize) -> Result<i64, GoalContractRepositoryError> {
    zero_based_index
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(GoalContractRepositoryError::ResourceLimit)
}

fn timestamp_to_i64(timestamp: GoalContractTimestamp) -> Result<i64, GoalContractRepositoryError> {
    i64::try_from(timestamp.unix_millis()).map_err(|_| GoalContractRepositoryError::InvalidInput)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<GoalContractTimestamp, GoalContractRepositoryError> {
    let value = u64::try_from(read_i64(row, index)?)
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    GoalContractTimestamp::from_unix_millis(value)
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)
}

fn read_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<GoalContractRevision, GoalContractRepositoryError> {
    let value = u32::try_from(read_i64(row, index)?)
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)?;
    GoalContractRevision::new(value).map_err(|_| GoalContractRepositoryError::InvalidStoredData)
}

fn read_optional_revision(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<GoalContractRevision>, GoalContractRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(GoalContractRepositoryError::Read)?;
    value
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| GoalContractRepositoryError::InvalidStoredData)
                .and_then(|value| {
                    GoalContractRevision::new(value)
                        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)
                })
        })
        .transpose()
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], GoalContractRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(GoalContractRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| GoalContractRepositoryError::InvalidStoredData)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, GoalContractRepositoryError> {
    row.get(index).map_err(GoalContractRepositoryError::Read)
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, GoalContractRepositoryError> {
    row.get(index).map_err(GoalContractRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, GoalContractRepositoryError> {
    row.get(index).map_err(GoalContractRepositoryError::Read)
}

async fn close_write_transaction<T>(
    transaction: Transaction,
    result: Result<T, GoalContractRepositoryError>,
) -> Result<T, GoalContractRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(GoalContractRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn close_read_transaction<T>(
    transaction: Transaction,
    result: Result<T, GoalContractRepositoryError>,
) -> Result<T, GoalContractRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(GoalContractRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: GoalContractRepositoryError,
) -> Result<T, GoalContractRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(GoalContractRepositoryError::Rollback(source)),
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_unexpected_constraint(source: libsql::Error) -> GoalContractRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        GoalContractRepositoryError::InvalidStoredData
    } else {
        GoalContractRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum GoalContractRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidInput,
    InvalidStoredData,
    ResourceLimit,
    TaskAlreadyExists,
    TaskNotFound,
    RevisionConflict,
}

impl GoalContractRepositoryError {
    pub(crate) fn classify(&self) -> GoalContractStoreFailure {
        match self {
            Self::InvalidInput | Self::InvalidStoredData | Self::ResourceLimit => {
                GoalContractStoreFailure::InvalidStoredData
            }
            Self::TaskAlreadyExists => GoalContractStoreFailure::TaskAlreadyExists,
            Self::TaskNotFound => GoalContractStoreFailure::TaskNotFound,
            Self::RevisionConflict => GoalContractStoreFailure::RevisionConflict,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    GoalContractStoreFailure::Corrupt
                } else {
                    GoalContractStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for GoalContractRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "Goal Contract transaction could not begin",
            Self::Read(_) => "Goal Contract data could not be read",
            Self::Write(_) => "Goal Contract data could not be written",
            Self::Commit(_) => "Goal Contract transaction could not commit",
            Self::Rollback(_) => "Goal Contract transaction could not roll back",
            Self::InvalidInput => "Goal Contract input was invalid",
            Self::InvalidStoredData => "Goal Contract data was invalid",
            Self::ResourceLimit => "Goal Contract data exceeded a resource limit",
            Self::TaskAlreadyExists => "Goal Contract task already exists",
            Self::TaskNotFound => "Goal Contract task was not found",
            Self::RevisionConflict => "Goal Contract revision conflicted",
        })
    }
}

impl Error for GoalContractRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => Some(error),
            Self::InvalidInput
            | Self::InvalidStoredData
            | Self::ResourceLimit
            | Self::TaskAlreadyExists
            | Self::TaskNotFound
            | Self::RevisionConflict => None,
        }
    }
}
