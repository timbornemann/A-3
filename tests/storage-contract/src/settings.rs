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

    // Reproduce the desktop sequence: a legacy OpenAI endpoint, both roles on
    // OpenAI, then only Coding moved to Ollama. Provider slots, not the legacy
    // singleton, own role validity in the multi-provider schema.
    use a3_application::{ModelEndpointAccess, ModelProviderKind, ProviderCredentialRequirement};
    let openai = ConfiguredModelEndpoint::from_validated_adapter_with_security(
        ModelProviderId::try_from_string("openai".to_owned())?,
        "https://api.openai.com".to_owned(),
        ModelEndpointScope::Remote,
        ModelEndpointAccess::ExplicitUserInitiatedRemote,
        ProviderCredentialRequirement::ApiKey,
    )?;
    let settings = changed.settings().clone().with_endpoint(Some(openai));
    let (settings, generation) = settings.begin_credential_store()?;
    let at = SettingsTimestamp::from_unix_millis(20_000)?;
    let settings = settings
        .complete_credential_store(generation)?
        .with_llm_probe(
            LlmModelRole::Coding,
            llm_profile_for("openai", ModelStructuredOutputCapability::Verified)?,
            at,
        )?
        .with_llm_probe(
            LlmModelRole::Mapping,
            llm_profile_for("openai", ModelStructuredOutputCapability::Verified)?,
            at,
        )?
        .with_provider_connection_verified(ModelProviderKind::Ollama, at)?
        .with_provider_enabled(ModelProviderKind::Ollama, true)?;
    let before = reopened.append(changed.version(), &settings).await?;
    assert_eq!(reopened.load().await?, before);
    let mixed = settings.with_provider_llm_probe(
        ModelProviderKind::Ollama,
        LlmModelRole::Coding,
        llm_profile(ModelStructuredOutputCapability::Verified)?,
        at,
    )?;
    let mixed = reopened.append(before.version(), &mixed).await?;
    assert_eq!(
        reopened.load().await?,
        mixed,
        "mixed roles must survive the next read"
    );
    let third = factory.open(&app_data_root).await?;
    assert_eq!(
        third.load().await?,
        mixed,
        "mixed roles must survive reopening"
    );
    assert_eq!(
        mixed.settings().llm_profile(LlmModelRole::Mapping),
        before.settings().llm_profile(LlmModelRole::Mapping)
    );
    assert_eq!(
        mixed.settings().provider(ModelProviderKind::OpenAi),
        before.settings().provider(ModelProviderKind::OpenAi)
    );

    // All three roles may use independent slots; changing Coding repeatedly must
    // preserve the exact Mapping profile, including probe time and credentials.
    let gemini = ConfiguredModelEndpoint::from_validated_adapter_with_security(
        ModelProviderId::try_from_string("gemini".to_owned())?,
        "https://generativelanguage.googleapis.com".to_owned(),
        ModelEndpointScope::Remote,
        ModelEndpointAccess::ExplicitUserInitiatedRemote,
        ProviderCredentialRequirement::ApiKey,
    )?;
    let settings = mixed
        .settings()
        .clone()
        .with_provider_endpoint(ModelProviderKind::Gemini, Some(gemini));
    let (settings, generation) =
        settings.begin_provider_credential_store(ModelProviderKind::Gemini)?;
    let settings = settings
        .complete_provider_credential_store(ModelProviderKind::Gemini, generation)?
        .with_provider_connection_verified(ModelProviderKind::Gemini, at)?
        .with_provider_enabled(ModelProviderKind::Gemini, true)?
        .with_provider_embedding_probe(ModelProviderKind::Ollama, embedding_profile()?, at)?;
    let compatible = ConfiguredModelEndpoint::from_validated_adapter_with_security(
        ModelProviderId::try_from_string("openai-compatible".to_owned())?,
        "https://openrouter.ai/api/v1".to_owned(),
        ModelEndpointScope::Remote,
        ModelEndpointAccess::ExplicitUserInitiatedRemote,
        ProviderCredentialRequirement::ApiKey,
    )?;
    let settings =
        settings.with_provider_endpoint(ModelProviderKind::OpenAiCompatible, Some(compatible));
    let (settings, generation) =
        settings.begin_provider_credential_store(ModelProviderKind::OpenAiCompatible)?;
    let settings = settings
        .complete_provider_credential_store(ModelProviderKind::OpenAiCompatible, generation)?
        .with_provider_connection_verified(ModelProviderKind::OpenAiCompatible, at)?
        .with_provider_enabled(ModelProviderKind::OpenAiCompatible, true)?;
    let mut current = third.append(mixed.version(), &settings).await?;
    for kind in [
        ModelProviderKind::Gemini,
        ModelProviderKind::OpenAi,
        ModelProviderKind::OpenAiCompatible,
        ModelProviderKind::Ollama,
    ] {
        let changed = current.settings().clone().with_provider_llm_probe(
            kind,
            LlmModelRole::Coding,
            llm_profile_for(
                kind.provider_id(),
                ModelStructuredOutputCapability::Verified,
            )?,
            at,
        )?;
        current = third.append(current.version(), &changed).await?;
        assert_eq!(third.load().await?, current);
        assert_eq!(
            current.settings().llm_profile(LlmModelRole::Mapping),
            before.settings().llm_profile(LlmModelRole::Mapping)
        );
        assert_eq!(
            current
                .settings()
                .provider(ModelProviderKind::OpenAi)
                .credential(),
            before
                .settings()
                .provider(ModelProviderKind::OpenAi)
                .credential()
        );
        assert!(current.settings().embedding_profile().is_some());
    }
    let fourth = factory.open(&app_data_root).await?;
    assert_eq!(fourth.load().await?, current);
    crate::release_contract_store(fourth);
    crate::release_contract_store(third);

    crate::release_contract_store(reopened);
    crate::release_contract_store(store);
    crate::complete_contract_phase()
}

fn provider_id() -> ContractResult<ModelProviderId> {
    Ok(ModelProviderId::try_from_string("ollama".to_owned())?)
}

fn llm_profile(capability: ModelStructuredOutputCapability) -> ContractResult<ModelProfile> {
    llm_profile_for("ollama", capability)
}

fn llm_profile_for(
    provider: &str,
    capability: ModelStructuredOutputCapability,
) -> ContractResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string(provider.to_owned())?,
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
