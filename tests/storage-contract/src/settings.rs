use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    ConfiguredModelEndpoint, DesktopSettings, DesktopSettingsStore, DesktopSettingsStoreFailure,
    DesktopSettingsStoreVersion, LlmModelRole, ModelEndpointScope, ProviderHealthStatus,
    SettingsTimestamp,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProviderId, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding,
    ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
    ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
};

pub(crate) async fn verify<F>(
    factory: &F,
    workspace: &crate::fixture::ContractWorkspace,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("desktop-settings");
    let store = factory.open(&app_data_root).await?;
    let initial = store.load().await?;
    assert_eq!(initial.version(), DesktopSettingsStoreVersion::initial());
    assert!(initial.settings().endpoint().is_none());

    let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
        provider_id()?,
        "http://127.0.0.1:11434".to_owned(),
        ModelEndpointScope::LocalLoopback,
    )?;
    let configured_settings = DesktopSettings::unconfigured().with_endpoint(Some(endpoint));
    let configured = store
        .append(initial.version(), &configured_settings)
        .await?;
    assert_eq!(configured.version(), DesktopSettingsStoreVersion::new(1)?);

    let coding_settings = configured.settings().clone().with_llm_probe(
        LlmModelRole::Coding,
        llm_profile(ModelStructuredOutputCapability::Verified)?,
        SettingsTimestamp::from_unix_millis(10_000)?,
    )?;
    let coding = store.append(configured.version(), &coding_settings).await?;
    let embedding_settings = coding.settings().clone().with_embedding_probe(
        embedding_profile()?,
        SettingsTimestamp::from_unix_millis(10_001)?,
    )?;
    let embedding = store.append(coding.version(), &embedding_settings).await?;
    assert_eq!(embedding.version(), DesktopSettingsStoreVersion::new(3)?);
    assert_eq!(
        embedding
            .settings()
            .provider_health()
            .map(|health| health.status()),
        Some(ProviderHealthStatus::Healthy)
    );

    let reopened = factory.open(&app_data_root).await?;
    let loaded = reopened.load().await?;
    assert_eq!(loaded, embedding);
    assert!(matches!(
        reopened
            .append(DesktopSettingsStoreVersion::new(1)?, loaded.settings())
            .await,
        Err(DesktopSettingsStoreFailure::VersionConflict)
    ));
    assert_eq!(reopened.load().await?, embedding);

    let changed = embedding.settings().clone().with_endpoint(Some(
        ConfiguredModelEndpoint::from_validated_adapter(
            provider_id()?,
            "http://127.0.0.1:22434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?,
    ));
    let changed = reopened.append(embedding.version(), &changed).await?;
    assert!(
        changed
            .settings()
            .llm_profile(LlmModelRole::Coding)
            .is_none()
    );
    assert!(changed.settings().embedding_profile().is_none());

    crate::release_contract_store(reopened);
    crate::release_contract_store(store);
    crate::complete_contract_phase()
}

fn provider_id() -> ContractResult<ModelProviderId> {
    Ok(ModelProviderId::try_from_string("ollama".to_owned())?)
}

fn llm_profile(capability: ModelStructuredOutputCapability) -> ContractResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        provider_id()?,
        ModelId::try_from_string("coder-local".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(32_768)?,
            ModelOutputLimit::new(4_096)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(2)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        )?,
        ModelCapabilities::new(capability, ModelToolCallMode::NativeProviderReported),
    ))
}

fn embedding_profile() -> ContractResult<EmbeddingModelProfile> {
    Ok(EmbeddingModelProfile::v1(
        EmbeddingProviderId::new("ollama".to_owned())?,
        EmbeddingModelId::new("embed-local".to_owned())?,
        EmbeddingDimension::new(768)?,
        EmbeddingBatchSize::new(8)?,
    ))
}
