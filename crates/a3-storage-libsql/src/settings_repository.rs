use crate::catalog::is_corruption;
use crate::{CatalogDatabase, CatalogOpenError};
use a3_application::{
    ConfiguredModelEndpoint, DesktopSettings, DesktopSettingsStoreFailure,
    DesktopSettingsStoreVersion, LlmModelRole, LlmRoleProfile, ModelEndpointScope,
    ProviderHealthObservation, ProviderHealthStatus, SettingsTimestamp, StoredDesktopSettings,
    VerifiedEmbeddingProfile,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProviderId, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelProfileSource,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::fmt;

const SQLITE_CONSTRAINT: i32 = 19;

pub(crate) async fn load(
    catalog: &CatalogDatabase,
) -> Result<StoredDesktopSettings, SettingsRepositoryError> {
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(SettingsRepositoryError::Open)?;
    load_from_connection(&connection).await
}

pub(crate) async fn append(
    catalog: &CatalogDatabase,
    expected: DesktopSettingsStoreVersion,
    settings: &DesktopSettings,
) -> Result<StoredDesktopSettings, SettingsRepositoryError> {
    validate_persistable(settings)?;
    let connection = catalog
        .connection_for_operation()
        .await
        .map_err(SettingsRepositoryError::Open)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(SettingsRepositoryError::Begin)?;
    let result = append_in_transaction(&transaction, expected, settings).await;
    close(transaction, result).await
}

async fn load_from_connection(
    connection: &Connection,
) -> Result<StoredDesktopSettings, SettingsRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT revision, endpoint_provider_id, endpoint_origin, endpoint_scope,
             health_status, health_checked_at_unix_millis
             FROM desktop_settings_revisions ORDER BY revision DESC LIMIT 1",
            (),
        )
        .await
        .map_err(SettingsRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(SettingsRepositoryError::Read)? else {
        return Ok(StoredDesktopSettings::initial());
    };
    let version = read_version(&row, 0)?;
    let provider_id = read_optional_string(&row, 1)?;
    let origin = read_optional_string(&row, 2)?;
    let scope = read_optional_string(&row, 3)?;
    let health_status = read_optional_string(&row, 4)?;
    let health_checked_at = read_optional_i64(&row, 5)?;
    if rows
        .next()
        .await
        .map_err(SettingsRepositoryError::Read)?
        .is_some()
    {
        return Err(SettingsRepositoryError::InvalidStoredData);
    }

    let endpoint = decode_endpoint(provider_id, origin, scope)?;
    let health = decode_health(endpoint.as_ref(), health_status, health_checked_at)?;
    let (coding, mapping) = load_llm_profiles(connection, version).await?;
    let embedding = load_embedding_profile(connection, version).await?;
    let settings = DesktopSettings::from_stored_parts(endpoint, health, coding, mapping, embedding)
        .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    Ok(StoredDesktopSettings::new(version, settings))
}

async fn append_in_transaction(
    transaction: &Transaction,
    expected: DesktopSettingsStoreVersion,
    settings: &DesktopSettings,
) -> Result<StoredDesktopSettings, SettingsRepositoryError> {
    let current = load_latest_version(transaction).await?;
    if current != expected {
        return Err(SettingsRepositoryError::VersionConflict);
    }
    let next_value = expected
        .get()
        .checked_add(1)
        .ok_or(SettingsRepositoryError::ResourceLimit)?;
    let next = DesktopSettingsStoreVersion::new(next_value)
        .map_err(|_| SettingsRepositoryError::ResourceLimit)?;
    let endpoint_provider_id = settings
        .endpoint()
        .map(|endpoint| endpoint.provider_id().as_str().to_owned());
    let endpoint_origin = settings
        .endpoint()
        .map(|endpoint| endpoint.canonical_origin().to_owned());
    let endpoint_scope = settings
        .endpoint()
        .map(|endpoint| encode_scope(endpoint.scope()));
    let health_status = settings
        .provider_health()
        .map(|health| encode_health_status(health.status()));
    let health_checked_at = settings
        .provider_health()
        .and_then(ProviderHealthObservation::checked_at)
        .map(|timestamp| u64_to_i64(timestamp.unix_millis()))
        .transpose()?;
    transaction
        .execute(
            "INSERT INTO desktop_settings_revisions (
             revision, endpoint_provider_id, endpoint_origin, endpoint_scope,
             health_status, health_checked_at_unix_millis
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                u64_to_i64(next.get())?,
                endpoint_provider_id,
                endpoint_origin,
                endpoint_scope,
                health_status,
                health_checked_at
            ],
        )
        .await
        .map_err(classify_write)?;
    for role in [LlmModelRole::Coding, LlmModelRole::Mapping] {
        if let Some(profile) = settings.llm_profile(role) {
            insert_llm_profile(transaction, next, role, profile).await?;
        }
    }
    if let Some(profile) = settings.embedding_profile() {
        insert_embedding_profile(transaction, next, profile).await?;
    }
    Ok(StoredDesktopSettings::new(next, settings.clone()))
}

async fn insert_llm_profile(
    transaction: &Transaction,
    version: DesktopSettingsStoreVersion,
    role: LlmModelRole,
    selected: &LlmRoleProfile,
) -> Result<(), SettingsRepositoryError> {
    let profile = selected.profile();
    let settings = profile.settings();
    let sampling = settings.sampling();
    transaction
        .execute(
            "INSERT INTO desktop_llm_profiles (
             revision, role, provider_id, model_id, context_tokens, output_tokens,
             parallelism, temperature_milli, top_p_milli, schema_grounding,
             structured_output, tool_call_mode, probed_at_unix_millis
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                u64_to_i64(version.get())?,
                encode_role(role),
                profile.provider_id().as_str(),
                profile.model_id().as_str(),
                i64::from(settings.context_limit().get()),
                i64::from(settings.output_limit().get()),
                i64::from(settings.parallelism_limit().get()),
                i64::from(sampling.temperature().milli()),
                i64::from(sampling.top_p().milli()),
                encode_schema_grounding(settings.schema_grounding()),
                encode_structured_output(profile.capabilities().structured_output()),
                encode_tool_call_mode(profile.capabilities().tool_call_mode()),
                u64_to_i64(selected.probed_at().unix_millis())?
            ],
        )
        .await
        .map_err(classify_write)?;
    Ok(())
}

async fn insert_embedding_profile(
    transaction: &Transaction,
    version: DesktopSettingsStoreVersion,
    selected: &VerifiedEmbeddingProfile,
) -> Result<(), SettingsRepositoryError> {
    let profile = selected.profile();
    transaction
        .execute(
            "INSERT INTO desktop_embedding_profiles (
             revision, provider_id, model_id, dimension, max_batch_size,
             probed_at_unix_millis
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                u64_to_i64(version.get())?,
                profile.provider_id().as_str(),
                profile.model_id().as_str(),
                i64::from(profile.dimension().get()),
                i64::from(profile.max_batch_size().get()),
                u64_to_i64(selected.probed_at().unix_millis())?
            ],
        )
        .await
        .map_err(classify_write)?;
    Ok(())
}

async fn load_latest_version(
    transaction: &Transaction,
) -> Result<DesktopSettingsStoreVersion, SettingsRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT revision FROM desktop_settings_revisions ORDER BY revision DESC LIMIT 1",
            (),
        )
        .await
        .map_err(SettingsRepositoryError::Read)?;
    let version = rows
        .next()
        .await
        .map_err(SettingsRepositoryError::Read)?
        .as_ref()
        .map(|row| read_version(row, 0))
        .transpose()?
        .unwrap_or_else(DesktopSettingsStoreVersion::initial);
    if rows
        .next()
        .await
        .map_err(SettingsRepositoryError::Read)?
        .is_some()
    {
        return Err(SettingsRepositoryError::InvalidStoredData);
    }
    Ok(version)
}

async fn load_llm_profiles(
    connection: &Connection,
    version: DesktopSettingsStoreVersion,
) -> Result<(Option<LlmRoleProfile>, Option<LlmRoleProfile>), SettingsRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT role, provider_id, model_id, context_tokens, output_tokens,
             parallelism, temperature_milli, top_p_milli, schema_grounding,
             structured_output, tool_call_mode, probed_at_unix_millis
             FROM desktop_llm_profiles WHERE revision = ?1 ORDER BY role",
            [u64_to_i64(version.get())?],
        )
        .await
        .map_err(SettingsRepositoryError::Read)?;
    let mut coding = None;
    let mut mapping = None;
    while let Some(row) = rows.next().await.map_err(SettingsRepositoryError::Read)? {
        let role = decode_role(&read_string(&row, 0)?)?;
        let selected = decode_llm_profile(&row)?;
        let slot = match role {
            LlmModelRole::Coding => &mut coding,
            LlmModelRole::Mapping => &mut mapping,
        };
        if slot.replace(selected).is_some() {
            return Err(SettingsRepositoryError::InvalidStoredData);
        }
    }
    Ok((coding, mapping))
}

fn decode_llm_profile(row: &libsql::Row) -> Result<LlmRoleProfile, SettingsRepositoryError> {
    let provider_id = ModelProviderId::try_from_string(read_string(row, 1)?)
        .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    let model_id = ModelId::try_from_string(read_string(row, 2)?)
        .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    let settings = ModelProfileSettings::new(
        ModelContextLimit::new(read_u32(row, 3)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        ModelOutputLimit::new(read_u32(row, 4)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(read_u16(row, 5)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(read_u16(row, 6)?)
                .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
            ModelTopP::from_milli(read_u16(row, 7)?)
                .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        ),
        ModelStopSequences::empty(),
        decode_schema_grounding(&read_string(row, 8)?)?,
    )
    .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    let capabilities = ModelCapabilities::new(
        decode_structured_output(&read_string(row, 9)?)?,
        decode_tool_call_mode(&read_string(row, 10)?)?,
    );
    let probed_at = read_timestamp(row, 11)?;
    Ok(LlmRoleProfile::from_probe(
        ModelProfile::from_probe(provider_id, model_id, settings, capabilities),
        probed_at,
    ))
}

async fn load_embedding_profile(
    connection: &Connection,
    version: DesktopSettingsStoreVersion,
) -> Result<Option<VerifiedEmbeddingProfile>, SettingsRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT provider_id, model_id, dimension, max_batch_size,
             probed_at_unix_millis FROM desktop_embedding_profiles WHERE revision = ?1",
            [u64_to_i64(version.get())?],
        )
        .await
        .map_err(SettingsRepositoryError::Read)?;
    let Some(row) = rows.next().await.map_err(SettingsRepositoryError::Read)? else {
        return Ok(None);
    };
    let profile = EmbeddingModelProfile::v1(
        EmbeddingProviderId::new(read_string(&row, 0)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        EmbeddingModelId::new(read_string(&row, 1)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        EmbeddingDimension::new(read_u16(&row, 2)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
        EmbeddingBatchSize::new(read_u16(&row, 3)?)
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
    );
    let selected = VerifiedEmbeddingProfile::from_probe(profile, read_timestamp(&row, 4)?);
    if rows
        .next()
        .await
        .map_err(SettingsRepositoryError::Read)?
        .is_some()
    {
        return Err(SettingsRepositoryError::InvalidStoredData);
    }
    Ok(Some(selected))
}

fn decode_endpoint(
    provider_id: Option<String>,
    origin: Option<String>,
    scope: Option<String>,
) -> Result<Option<ConfiguredModelEndpoint>, SettingsRepositoryError> {
    match (provider_id, origin, scope) {
        (None, None, None) => Ok(None),
        (Some(provider_id), Some(origin), Some(scope)) => {
            let provider_id = ModelProviderId::try_from_string(provider_id)
                .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
            let scope = decode_scope(&scope)?;
            ConfiguredModelEndpoint::from_validated_adapter(provider_id, origin, scope)
                .map(Some)
                .map_err(|_| SettingsRepositoryError::InvalidStoredData)
        }
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn decode_health(
    endpoint: Option<&ConfiguredModelEndpoint>,
    status: Option<String>,
    checked_at: Option<i64>,
) -> Result<Option<ProviderHealthObservation>, SettingsRepositoryError> {
    match (endpoint, status, checked_at) {
        (None, None, None) => Ok(None),
        (Some(endpoint), Some(status), None) => {
            let decoded = decode_health_status(&status)?;
            let expected = ProviderHealthObservation::initial(endpoint.scope());
            (decoded == expected.status())
                .then_some(Some(expected))
                .ok_or(SettingsRepositoryError::InvalidStoredData)
        }
        (Some(_), Some(status), Some(checked_at)) => {
            let status = decode_health_status(&status)?;
            let checked_at = SettingsTimestamp::from_unix_millis(
                u64::try_from(checked_at)
                    .map_err(|_| SettingsRepositoryError::InvalidStoredData)?,
            )
            .map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
            ProviderHealthObservation::checked(status, checked_at)
                .map(Some)
                .map_err(|_| SettingsRepositoryError::InvalidStoredData)
        }
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn validate_persistable(settings: &DesktopSettings) -> Result<(), SettingsRepositoryError> {
    for role in [LlmModelRole::Coding, LlmModelRole::Mapping] {
        if let Some(profile) = settings.llm_profile(role)
            && (profile.profile().source() != ModelProfileSource::Probe
                || !profile
                    .profile()
                    .settings()
                    .stop_sequences()
                    .as_slice()
                    .is_empty())
        {
            return Err(SettingsRepositoryError::InvalidStoredData);
        }
    }
    Ok(())
}

fn encode_scope(scope: ModelEndpointScope) -> &'static str {
    match scope {
        ModelEndpointScope::LocalLoopback => "local_loopback",
        ModelEndpointScope::Remote => "remote",
    }
}

fn decode_scope(value: &str) -> Result<ModelEndpointScope, SettingsRepositoryError> {
    match value {
        "local_loopback" => Ok(ModelEndpointScope::LocalLoopback),
        "remote" => Ok(ModelEndpointScope::Remote),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn encode_health_status(status: ProviderHealthStatus) -> &'static str {
    match status {
        ProviderHealthStatus::NotChecked => "not_checked",
        ProviderHealthStatus::Healthy => "healthy",
        ProviderHealthStatus::CapabilityLimited => "capability_limited",
        ProviderHealthStatus::Unreachable => "unreachable",
        ProviderHealthStatus::Cancelled => "cancelled",
        ProviderHealthStatus::RemoteBlocked => "remote_blocked",
    }
}

fn decode_health_status(value: &str) -> Result<ProviderHealthStatus, SettingsRepositoryError> {
    match value {
        "not_checked" => Ok(ProviderHealthStatus::NotChecked),
        "healthy" => Ok(ProviderHealthStatus::Healthy),
        "capability_limited" => Ok(ProviderHealthStatus::CapabilityLimited),
        "unreachable" => Ok(ProviderHealthStatus::Unreachable),
        "cancelled" => Ok(ProviderHealthStatus::Cancelled),
        "remote_blocked" => Ok(ProviderHealthStatus::RemoteBlocked),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn encode_role(role: LlmModelRole) -> &'static str {
    match role {
        LlmModelRole::Coding => "coding",
        LlmModelRole::Mapping => "mapping",
    }
}

fn decode_role(value: &str) -> Result<LlmModelRole, SettingsRepositoryError> {
    match value {
        "coding" => Ok(LlmModelRole::Coding),
        "mapping" => Ok(LlmModelRole::Mapping),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn encode_schema_grounding(value: ModelPromptSchemaGrounding) -> &'static str {
    match value {
        ModelPromptSchemaGrounding::FormatFieldOnly => "format_only",
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt => "repeat_in_prompt",
    }
}

fn decode_schema_grounding(
    value: &str,
) -> Result<ModelPromptSchemaGrounding, SettingsRepositoryError> {
    match value {
        "format_only" => Ok(ModelPromptSchemaGrounding::FormatFieldOnly),
        "repeat_in_prompt" => Ok(ModelPromptSchemaGrounding::RepeatSchemaInPrompt),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn encode_structured_output(value: ModelStructuredOutputCapability) -> &'static str {
    match value {
        ModelStructuredOutputCapability::Verified => "verified",
        ModelStructuredOutputCapability::Unavailable => "unavailable",
    }
}

fn decode_structured_output(
    value: &str,
) -> Result<ModelStructuredOutputCapability, SettingsRepositoryError> {
    match value {
        "verified" => Ok(ModelStructuredOutputCapability::Verified),
        "unavailable" => Ok(ModelStructuredOutputCapability::Unavailable),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

fn encode_tool_call_mode(value: ModelToolCallMode) -> &'static str {
    match value {
        ModelToolCallMode::Disabled => "disabled",
        ModelToolCallMode::NativeProviderReported => "native_reported",
    }
}

fn decode_tool_call_mode(value: &str) -> Result<ModelToolCallMode, SettingsRepositoryError> {
    match value {
        "disabled" => Ok(ModelToolCallMode::Disabled),
        "native_reported" => Ok(ModelToolCallMode::NativeProviderReported),
        _ => Err(SettingsRepositoryError::InvalidStoredData),
    }
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, SettingsRepositoryError>,
) -> Result<T, SettingsRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(SettingsRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(SettingsRepositoryError::Rollback(source)),
        },
    }
}

fn read_version(
    row: &libsql::Row,
    index: i32,
) -> Result<DesktopSettingsStoreVersion, SettingsRepositoryError> {
    let value: i64 = row.get(index).map_err(SettingsRepositoryError::Read)?;
    let value = u64::try_from(value).map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    DesktopSettingsStoreVersion::new(value).map_err(|_| SettingsRepositoryError::InvalidStoredData)
}

fn read_timestamp(
    row: &libsql::Row,
    index: i32,
) -> Result<SettingsTimestamp, SettingsRepositoryError> {
    let value: i64 = row.get(index).map_err(SettingsRepositoryError::Read)?;
    let value = u64::try_from(value).map_err(|_| SettingsRepositoryError::InvalidStoredData)?;
    SettingsTimestamp::from_unix_millis(value)
        .map_err(|_| SettingsRepositoryError::InvalidStoredData)
}

fn read_string(row: &libsql::Row, index: i32) -> Result<String, SettingsRepositoryError> {
    row.get(index).map_err(SettingsRepositoryError::Read)
}

fn read_optional_string(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, SettingsRepositoryError> {
    row.get(index).map_err(SettingsRepositoryError::Read)
}

fn read_optional_i64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<i64>, SettingsRepositoryError> {
    row.get(index).map_err(SettingsRepositoryError::Read)
}

fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, SettingsRepositoryError> {
    let value: i64 = row.get(index).map_err(SettingsRepositoryError::Read)?;
    u32::try_from(value).map_err(|_| SettingsRepositoryError::InvalidStoredData)
}

fn read_u16(row: &libsql::Row, index: i32) -> Result<u16, SettingsRepositoryError> {
    let value: i64 = row.get(index).map_err(SettingsRepositoryError::Read)?;
    u16::try_from(value).map_err(|_| SettingsRepositoryError::InvalidStoredData)
}

fn u64_to_i64(value: u64) -> Result<i64, SettingsRepositoryError> {
    i64::try_from(value).map_err(|_| SettingsRepositoryError::ResourceLimit)
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn classify_write(source: libsql::Error) -> SettingsRepositoryError {
    if sqlite_primary_code(&source) == Some(SQLITE_CONSTRAINT) {
        SettingsRepositoryError::InvalidStoredData
    } else {
        SettingsRepositoryError::Write(source)
    }
}

#[derive(Debug)]
pub(crate) enum SettingsRepositoryError {
    Open(CatalogOpenError),
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredData,
    ResourceLimit,
    VersionConflict,
}

impl SettingsRepositoryError {
    pub(crate) fn classify(&self) -> DesktopSettingsStoreFailure {
        match self {
            Self::InvalidStoredData => DesktopSettingsStoreFailure::InvalidStoredData,
            Self::ResourceLimit => DesktopSettingsStoreFailure::ResourceLimit,
            Self::VersionConflict => DesktopSettingsStoreFailure::VersionConflict,
            Self::Open(CatalogOpenError::NewerSchema { .. }) => {
                DesktopSettingsStoreFailure::UnsupportedSchema
            }
            Self::Open(
                CatalogOpenError::CorruptDatabase
                | CatalogOpenError::IntegrityCheckFailed
                | CatalogOpenError::MigrationHistoryMismatch { .. },
            ) => DesktopSettingsStoreFailure::Corrupt,
            Self::Open(_) => DesktopSettingsStoreFailure::Unavailable,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    DesktopSettingsStoreFailure::Corrupt
                } else {
                    DesktopSettingsStoreFailure::Unavailable
                }
            }
        }
    }
}

impl fmt::Display for SettingsRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Open(_) => "desktop settings catalog could not be opened",
            Self::Begin(_) => "desktop settings transaction could not begin",
            Self::Read(_) => "desktop settings could not be read",
            Self::Write(_) => "desktop settings could not be written",
            Self::Commit(_) => "desktop settings transaction could not commit",
            Self::Rollback(_) => "desktop settings transaction could not roll back",
            Self::InvalidStoredData => "desktop settings data is invalid",
            Self::ResourceLimit => "desktop settings data exceeds a fixed bound",
            Self::VersionConflict => "desktop settings changed concurrently",
        })
    }
}
