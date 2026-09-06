use a3_application::{
    ConfiguredModelEndpoint, DeepMapExecutor, DeepMapPublicationStateStore, DesktopSettings,
    DesktopSettingsStore, GetDesktopSettings, KnowledgeIndexStore, LlmModelRole,
    LlmProfileActivation, LoadDesktopProviderCredential, ModelEndpointScope, ModelProvider,
    ModelProviderKind, ProviderCredentialStore, RunDeepMap, VerifiedModuleCardPublisher,
};
use a3_provider::{
    ExactGeminiEndpointPolicy, ExactOpenAiEndpointPolicy, GeminiEndpoint, GeminiModelProvider,
    LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider, OpenAiEndpoint,
    OpenAiModelProvider,
};
use std::fmt;
use std::sync::Arc;

/// Resolves a complete executor from the durable verified Mapping role without provider I/O.
#[derive(Clone)]
pub(crate) struct DeepMapRuntime {
    settings: Arc<dyn DesktopSettingsStore>,
    credentials: Arc<dyn ProviderCredentialStore>,
    index: Arc<dyn KnowledgeIndexStore>,
    publisher: Arc<dyn VerifiedModuleCardPublisher>,
    publication_state: Arc<dyn DeepMapPublicationStateStore>,
}

impl DeepMapRuntime {
    #[must_use]
    pub(crate) fn new(
        settings: Arc<dyn DesktopSettingsStore>,
        credentials: Arc<dyn ProviderCredentialStore>,
        index: Arc<dyn KnowledgeIndexStore>,
        publisher: Arc<dyn VerifiedModuleCardPublisher>,
        publication_state: Arc<dyn DeepMapPublicationStateStore>,
    ) -> Self {
        Self {
            settings,
            credentials,
            index,
            publisher,
            publication_state,
        }
    }

    /// Returns `None` fail-closed when any persisted endpoint, profile, or credential anchor differs.
    pub(crate) async fn resolve(&self) -> Option<Arc<dyn DeepMapExecutor>> {
        let stored = GetDesktopSettings::new(Arc::clone(&self.settings))
            .execute()
            .await
            .ok()?;
        let settings = stored.settings();
        let (endpoint, profile) = executable_mapping(settings)?;
        let provider: Arc<dyn ModelProvider> = match endpoint.provider_id().as_str() {
            "ollama" if endpoint.scope() == ModelEndpointScope::LocalLoopback => {
                let endpoint = OllamaEndpoint::parse(endpoint.canonical_origin()).ok()?;
                Arc::new(
                    OllamaModelProvider::new(endpoint, Arc::new(LocalOnlyOllamaEndpointPolicy))
                        .ok()?,
                )
            }
            "gemini" => {
                let endpoint = GeminiEndpoint::parse(endpoint.canonical_origin()).ok()?;
                let origin = endpoint.canonical_origin();
                let key = LoadDesktopProviderCredential::new(Arc::clone(&self.credentials))
                    .execute_for(settings, ModelProviderKind::Gemini)
                    .await
                    .ok()??;
                Arc::new(
                    GeminiModelProvider::new(
                        endpoint,
                        Arc::new(ExactGeminiEndpointPolicy::new(origin)),
                        key,
                    )
                    .ok()?,
                )
            }
            "openai" => {
                let endpoint = OpenAiEndpoint::parse(endpoint.canonical_origin()).ok()?;
                let origin = endpoint.canonical_origin();
                let key = LoadDesktopProviderCredential::new(Arc::clone(&self.credentials))
                    .execute_for(settings, ModelProviderKind::OpenAi)
                    .await
                    .ok()??;
                Arc::new(
                    OpenAiModelProvider::new(
                        endpoint,
                        Arc::new(ExactOpenAiEndpointPolicy::new(origin)),
                        key,
                    )
                    .ok()?,
                )
            }
            _ => return None,
        };
        RunDeepMap::new(
            profile,
            provider,
            Arc::clone(&self.index),
            Arc::clone(&self.publisher),
            Arc::clone(&self.publication_state),
        )
        .ok()
        .map(|executor| Arc::new(executor) as Arc<dyn DeepMapExecutor>)
    }
}

fn executable_mapping(
    settings: &DesktopSettings,
) -> Option<(&ConfiguredModelEndpoint, a3_domain::ModelProfile)> {
    let selected = settings.llm_profile(LlmModelRole::Mapping)?;
    if selected.activation() != LlmProfileActivation::Executable {
        return None;
    }
    let kind = ModelProviderKind::from_provider_id(selected.profile().provider_id().as_str())?;
    let slot = settings.provider(kind);
    if !slot.enabled() || slot.connection_verified_at().is_none() {
        return None;
    }
    Some((slot.endpoint()?, selected.profile().clone()))
}

impl fmt::Debug for DeepMapRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeepMapRuntime")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::executable_mapping;
    use a3_application::{
        ConfiguredModelEndpoint, DesktopSettings, LlmModelRole, ModelEndpointAccess,
        ModelEndpointScope, ProviderCredentialRequirement, SettingsTimestamp,
    };
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use std::error::Error;

    #[test]
    fn persisted_verified_mapping_profile_is_executable_runtime_configuration()
    -> Result<(), Box<dyn Error>> {
        let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            ModelProviderId::try_from_string("ollama".to_owned())?,
            "http://127.0.0.1:11434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?;
        let profile = profile("ollama", ModelStructuredOutputCapability::Verified)?;
        let settings = DesktopSettings::unconfigured()
            .with_endpoint(Some(endpoint))
            .with_llm_probe(
                LlmModelRole::Mapping,
                profile.clone(),
                SettingsTimestamp::from_unix_millis(1)?,
            )?;
        let (_, resolved) = executable_mapping(&settings).ok_or("mapping runtime unavailable")?;
        assert_eq!(resolved.reference(), profile.reference());
        Ok(())
    }

    #[test]
    fn every_supported_provider_uses_the_same_verified_mapping_contract()
    -> Result<(), Box<dyn Error>> {
        for (provider, origin, scope) in [
            (
                "ollama",
                "http://127.0.0.1:11434",
                ModelEndpointScope::LocalLoopback,
            ),
            (
                "gemini",
                "https://generativelanguage.googleapis.com",
                ModelEndpointScope::Remote,
            ),
            (
                "openai",
                "https://api.openai.com",
                ModelEndpointScope::Remote,
            ),
        ] {
            let provider_id = ModelProviderId::try_from_string(provider.to_owned())?;
            let endpoint = match scope {
                ModelEndpointScope::LocalLoopback => {
                    ConfiguredModelEndpoint::from_validated_adapter(
                        provider_id,
                        origin.to_owned(),
                        scope,
                    )?
                }
                ModelEndpointScope::Remote => {
                    ConfiguredModelEndpoint::from_validated_adapter_with_security(
                        provider_id,
                        origin.to_owned(),
                        scope,
                        ModelEndpointAccess::ExplicitUserInitiatedRemote,
                        ProviderCredentialRequirement::ApiKey,
                    )?
                }
            };
            let profile = profile(provider, ModelStructuredOutputCapability::Verified)?;
            let settings = DesktopSettings::unconfigured().with_endpoint(Some(endpoint));
            let settings = if scope == ModelEndpointScope::Remote {
                let (settings, generation) = settings.begin_credential_store()?;
                settings.complete_credential_store(generation)?
            } else {
                settings
            };
            let settings = settings.with_llm_probe(
                LlmModelRole::Mapping,
                profile.clone(),
                SettingsTimestamp::from_unix_millis(1)?,
            )?;

            let (resolved_endpoint, resolved_profile) =
                executable_mapping(&settings).ok_or("mapping runtime unavailable")?;

            assert_eq!(resolved_endpoint.provider_id().as_str(), provider);
            assert_eq!(resolved_profile.reference(), profile.reference());
        }
        Ok(())
    }

    #[test]
    fn capability_limited_mapping_profile_stays_unavailable() -> Result<(), Box<dyn Error>> {
        let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            ModelProviderId::try_from_string("ollama".to_owned())?,
            "http://127.0.0.1:11434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?;
        let settings = DesktopSettings::unconfigured()
            .with_endpoint(Some(endpoint))
            .with_llm_probe(
                LlmModelRole::Mapping,
                profile("ollama", ModelStructuredOutputCapability::Unavailable)?,
                SettingsTimestamp::from_unix_millis(1)?,
            )?;
        assert!(executable_mapping(&settings).is_none());
        Ok(())
    }

    fn profile(
        provider: &str,
        capability: ModelStructuredOutputCapability,
    ) -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string(provider.to_owned())?,
            ModelId::try_from_string("mapper".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(2_048)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            ModelCapabilities::new(capability, ModelToolCallMode::Disabled),
        ))
    }
}
