use crate::CatalogDatabase;
use a3_application::AgentPermissionStoreFailure as Failure;
use a3_domain::{AgentPermissionMode, AgentPermissionRevision, AgentPermissionSettings};
use libsql::{Connection, TransactionBehavior, params};

pub(crate) async fn load(catalog: &CatalogDatabase) -> Result<AgentPermissionSettings, Failure> {
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(|_| Failure::Unavailable)?;
    read(&connection).await
}

async fn read(connection: &Connection) -> Result<AgentPermissionSettings, Failure> {
    let mut rows = connection
        .query(
            "SELECT revision, mode FROM agent_permission_revisions ORDER BY revision DESC LIMIT 1",
            (),
        )
        .await
        .map_err(|_| Failure::Unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(|_| Failure::Unavailable)?
        .ok_or(Failure::InvalidStoredData)?;
    let revision = row.get::<i64>(0).map_err(|_| Failure::InvalidStoredData)?;
    let revision = u64::try_from(revision)
        .ok()
        .and_then(|value| AgentPermissionRevision::new(value).ok())
        .ok_or(Failure::InvalidStoredData)?;
    let mode = match row
        .get::<String>(1)
        .map_err(|_| Failure::InvalidStoredData)?
        .as_str()
    {
        "askPermissions" => AgentPermissionMode::AskPermissions,
        "fullMachine" => AgentPermissionMode::FullMachine,
        _ => return Err(Failure::InvalidStoredData),
    };
    Ok(AgentPermissionSettings::new(mode, revision))
}

pub(crate) async fn update(
    catalog: &CatalogDatabase,
    expected: AgentPermissionRevision,
    mode: AgentPermissionMode,
) -> Result<AgentPermissionSettings, Failure> {
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(|_| Failure::Unavailable)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|_| Failure::Unavailable)?;
    let result = async {
        let previous = read(&transaction).await?;
        if previous.revision() != expected {
            return Err(Failure::Conflict);
        }
        if previous.mode() == mode {
            return Ok(previous);
        }
        let next = expected.next().map_err(|_| Failure::InvalidStoredData)?;
        let revision = i64::try_from(next.get()).map_err(|_| Failure::InvalidStoredData)?;
        transaction
            .execute(
                "INSERT INTO agent_permission_revisions (revision, mode) VALUES (?1, ?2)",
                params![revision, mode.as_str()],
            )
            .await
            .map_err(|_| Failure::Unavailable)?;
        Ok(AgentPermissionSettings::new(mode, next))
    }
    .await;
    match result {
        Ok(settings) => {
            transaction
                .commit()
                .await
                .map_err(|_| Failure::Unavailable)?;
            Ok(settings)
        }
        Err(error) => {
            transaction
                .rollback()
                .await
                .map_err(|_| Failure::Unavailable)?;
            Err(error)
        }
    }
}
