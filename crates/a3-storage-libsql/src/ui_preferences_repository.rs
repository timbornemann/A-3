use crate::{CatalogDatabase, catalog::is_corruption};
use a3_application::{
    AgentWorkspaceLayout, StoredUiPreferences, UiPreferencesError, UiPreferencesStoreVersion,
};
use libsql::{Transaction, TransactionBehavior, params};

pub(crate) async fn load(
    catalog: &CatalogDatabase,
) -> Result<StoredUiPreferences, UiPreferencesRepositoryError> {
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(|_| UiPreferencesRepositoryError::Unavailable)?;
    let mut rows = connection
        .query(
            "SELECT revision, agent_session_rail_width, agent_inspector_width,
             agent_session_rail_collapsed, agent_inspector_collapsed
             FROM ui_preference_revisions ORDER BY revision DESC LIMIT 1",
            (),
        )
        .await
        .map_err(UiPreferencesRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(UiPreferencesRepositoryError::Read)?
    else {
        return Ok(StoredUiPreferences::new(
            UiPreferencesStoreVersion::EMPTY,
            AgentWorkspaceLayout::DEFAULT,
        ));
    };
    let version = UiPreferencesStoreVersion::new(read_u64(&row, 0)?)
        .map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)?;
    let layout = AgentWorkspaceLayout::new(
        read_u16(&row, 1)?,
        read_u16(&row, 2)?,
        read_bool(&row, 3)?,
        read_bool(&row, 4)?,
    )
    .map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)?;
    Ok(StoredUiPreferences::new(version, layout))
}

pub(crate) async fn append(
    catalog: &CatalogDatabase,
    expected: UiPreferencesStoreVersion,
    layout: AgentWorkspaceLayout,
) -> Result<StoredUiPreferences, UiPreferencesRepositoryError> {
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(|_| UiPreferencesRepositoryError::Unavailable)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(UiPreferencesRepositoryError::Begin)?;
    let result = append_in_transaction(&transaction, expected, layout).await;
    close(transaction, result).await
}

async fn append_in_transaction(
    transaction: &Transaction,
    expected: UiPreferencesStoreVersion,
    layout: AgentWorkspaceLayout,
) -> Result<StoredUiPreferences, UiPreferencesRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT revision FROM ui_preference_revisions ORDER BY revision DESC LIMIT 1",
            (),
        )
        .await
        .map_err(UiPreferencesRepositoryError::Read)?;
    let current = rows
        .next()
        .await
        .map_err(UiPreferencesRepositoryError::Read)?
        .map(|row| read_u64(&row, 0))
        .transpose()?
        .unwrap_or(0);
    if current != expected.get() {
        return Err(UiPreferencesRepositoryError::Conflict);
    }
    let next_value = current
        .checked_add(1)
        .ok_or(UiPreferencesRepositoryError::InvalidStoredData)?;
    let next = UiPreferencesStoreVersion::new(next_value)
        .map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)?;
    transaction
        .execute(
            "INSERT INTO ui_preference_revisions (
             revision, agent_session_rail_width, agent_inspector_width,
             agent_session_rail_collapsed, agent_inspector_collapsed
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                u64_to_i64(next.get())?,
                i64::from(layout.session_rail_width()),
                i64::from(layout.inspector_width()),
                i64::from(layout.session_rail_collapsed()),
                i64::from(layout.inspector_collapsed())
            ],
        )
        .await
        .map_err(UiPreferencesRepositoryError::Write)?;
    Ok(StoredUiPreferences::new(next, layout))
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, UiPreferencesRepositoryError>,
) -> Result<T, UiPreferencesRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(UiPreferencesRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(UiPreferencesRepositoryError::Rollback(source)),
        },
    }
}

fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, UiPreferencesRepositoryError> {
    let value: i64 = row.get(index).map_err(UiPreferencesRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)
}

fn read_u16(row: &libsql::Row, index: i32) -> Result<u16, UiPreferencesRepositoryError> {
    let value = read_u64(row, index)?;
    u16::try_from(value).map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, UiPreferencesRepositoryError> {
    match read_u64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(UiPreferencesRepositoryError::InvalidStoredData),
    }
}

fn u64_to_i64(value: u64) -> Result<i64, UiPreferencesRepositoryError> {
    i64::try_from(value).map_err(|_| UiPreferencesRepositoryError::InvalidStoredData)
}

#[derive(Debug)]
pub(crate) enum UiPreferencesRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Conflict,
    InvalidStoredData,
    Unavailable,
}

impl UiPreferencesRepositoryError {
    pub(crate) fn classify(&self) -> UiPreferencesError {
        match self {
            Self::Conflict => UiPreferencesError::Conflict,
            Self::InvalidStoredData | Self::Unavailable => UiPreferencesError::Unavailable,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                let _corrupt = is_corruption(error);
                UiPreferencesError::Unavailable
            }
        }
    }
}
