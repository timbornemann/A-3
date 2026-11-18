//! Bounded model-plan operation retained independently of its presentation caption.

use super::{TaskLedgerRepositoryError as Error, classify_unexpected_constraint, id_bytes};
use a3_domain::{PlannedFileAddition, RepositoryPath, TaskId, TaskStepDefinition, TaskStepId};
use libsql::{Transaction, Value, params};

pub(super) const SCHEMA_VERSION: u32 = 59;

pub(super) async fn require_schema(
    tx: &Transaction,
    ledger: &a3_domain::TaskLedger,
) -> Result<(), Error> {
    if ledger
        .steps()
        .any(|step| step.definition().planned_file_addition().is_some())
        && crate::migration::read_user_version(tx)
            .await
            .map_err(Error::Read)?
            < SCHEMA_VERSION
    {
        return Err(Error::PlannedAdditionSchemaUnsupported);
    }
    Ok(())
}

pub(super) async fn write(
    tx: &Transaction,
    task: TaskId,
    definition: &TaskStepDefinition,
) -> Result<(), Error> {
    let Some(addition) = definition.planned_file_addition() else {
        return Ok(());
    };
    tx.execute(
        "INSERT INTO task_step_planned_file_additions(task_id,step_id,target_path) VALUES (?1,?2,?3)",
        params![id_bytes(task),id_bytes(definition.id()),addition.target().as_bytes().to_vec()],
    ).await.map_err(classify_unexpected_constraint)?;
    if tx
        .execute(
            "UPDATE task_steps SET has_planned_file_addition=1 WHERE task_id=?1 AND step_id=?2",
            params![id_bytes(task), id_bytes(definition.id())],
        )
        .await
        .map_err(classify_unexpected_constraint)?
        != 1
    {
        return Err(Error::InvalidStoredData);
    }
    Ok(())
}

pub(super) async fn read(
    tx: &Transaction,
    task: TaskId,
    step: TaskStepId,
    schema: u32,
    present: bool,
) -> Result<Option<PlannedFileAddition>, Error> {
    if schema < SCHEMA_VERSION {
        return if present {
            Err(Error::InvalidStoredData)
        } else {
            Ok(None)
        };
    }
    let mut rows = tx.query(
        "SELECT CASE WHEN typeof(target_path)='blob' AND length(target_path) BETWEEN 1 AND 131072
            THEN target_path ELSE x'' END FROM task_step_planned_file_additions
         WHERE task_id=?1 AND step_id=?2 LIMIT 2",
        params![id_bytes(task),id_bytes(step)],
    ).await.map_err(Error::Read)?;
    let addition = match rows.next().await.map_err(Error::Read)? {
        Some(row) if present => {
            let Value::Blob(bytes) = row.get_value(0).map_err(|_| Error::InvalidStoredData)? else {
                return Err(Error::InvalidStoredData);
            };
            let target =
                RepositoryPath::try_from_bytes(bytes).map_err(|_| Error::InvalidStoredData)?;
            Some(PlannedFileAddition::new(target))
        }
        None if !present => None,
        _ => return Err(Error::InvalidStoredData),
    };
    if rows.next().await.map_err(Error::Read)?.is_some() {
        return Err(Error::InvalidStoredData);
    }
    Ok(addition)
}
