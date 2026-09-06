use a3_application::{
    ConfigureDesktopModelEndpoint, ConfigureDesktopModelEndpointError,
    DeleteDesktopProviderCredential, DesktopSettingsStore, DesktopSettingsStoreFailure,
    DesktopSettingsStoreVersion, DiscoverProviderModels, EmbeddingCapabilityProbeRequest,
    GetDesktopSettings, LlmModelRole, LlmProfileActivation, LoadDesktopProviderCredential,
    ManageDesktopProviderCredentialError, ModelCancellationFuture, ModelEndpointAccess,
    ModelEndpointScope, ModelOperationControl, ModelProviderFailure, ModelProviderKind,
    ModelRequestTimeout, ProbeEmbeddingModelProfile, ProbeModelProfile, ProbeModelProfileFailure,
    ProviderApiKey, ProviderCredentialAccessError, ProviderCredentialLifecycle,
    ProviderCredentialRequirement, ProviderCredentialStore, ProviderCredentialStoreFailure,
    ProviderHealthStatus, RecordDesktopModelProbe, RecordDesktopModelProbeError,
    SetDesktopProviderCredential, SettingsTimestamp, StoredDesktopSettings,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingModelId, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfileSettings, ModelPromptSchemaGrounding, ModelSamplingProfile,
    ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
};
use a3_protocol::{
    CancelModelProbeResponseV1, CommandErrorV1, DataPrivacySettingsV1, EmbeddingRoleProfileV1,
    EmbeddingRoleProfileV2, ErrorCodeV1, LlmRoleProfileV1, LlmRoleProfileV2, ModelEndpointAccessV1,
    ModelEndpointScopeV1, ModelEndpointV1, ModelProfileActivationV1, ModelProviderKindV1,
    ModelProviderKindV2, ModelRoleV1, ModelToolCallModeV1, ProbeModelRoleRequestV1,
    ProbeModelRoleRequestV2, ProviderCredentialStatusV1, ProviderCredentialV1,
    ProviderHealthStatusV1, ProviderHealthV1, ProviderModelsResponseV1, ProviderModelsResponseV2,
    ProviderSettingsV2, SettingsResponseV1, SettingsResponseV2, SettingsV1, SettingsV2,
    StructuredOutputCapabilityV1,
};
use a3_provider::{
    ExactGeminiEndpointPolicy, ExactOpenAiEndpointPolicy, GeminiEndpoint, GeminiEndpointPolicy,
    GeminiModelProvider, GeminiSettingsEndpointValidator, LocalOnlyOllamaEndpointPolicy,
    OllamaEndpoint, OllamaModelProvider, OllamaSettingsEndpointValidator, OpenAiEndpoint,
    OpenAiEndpointPolicy, OpenAiModelProvider, OpenAiSettingsEndpointValidator,
    StandardGeminiEndpointPolicy, StandardOpenAiEndpointPolicy,
};
use futures::task::AtomicWaker;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::Poll;
use std::time::{SystemTime, UNIX_EPOCH};

const MODEL_PROBE_TIMEOUT_MILLIS: u64 = 30_000;
const MODEL_DISCOVERY_TIMEOUT_MILLIS: u64 = 15_000;

/// Owns the single explicit local model operation and durable Settings use cases.
pub struct ModelSettingsManager {
    store: Arc<dyn DesktopSettingsStore>,
    credential_store: Arc<dyn ProviderCredentialStore>,
    operation_active: AtomicBool,
    active_probe: Mutex<Option<Arc<ProbeCancellation>>>,
}

impl ModelSettingsManager {
    /// Wires local settings persistence and credential-free endpoint validation.
    #[must_use]
    pub fn new(
        store: Arc<dyn DesktopSettingsStore>,
        credential_store: Arc<dyn ProviderCredentialStore>,
    ) -> Self {
        Self {
            store,
            credential_store,
            operation_active: AtomicBool::new(false),
            active_probe: Mutex::new(None),
        }
    }

    /// Reads the current local snapshot without provider access.
    pub async fn query(&self) -> Result<SettingsResponseV1, CommandErrorV1> {
        let stored = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings(&stored, self.probe_is_active()).await)
    }

    /// Reads the complete three-provider Settings V2 snapshot without provider access.
    pub async fn query_v2(&self) -> Result<SettingsResponseV2, CommandErrorV1> {
        let stored = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings_v2(&stored, self.probe_is_active()).await)
    }

    /// Configures one provider slot without contacting its endpoint.
    pub async fn configure_provider_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKindV2,
        endpoint: Option<&str>,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(invalid_request());
        }
        let kind = provider_kind_v2(kind);
        let validator: Arc<dyn a3_application::ModelEndpointValidator> = match kind {
            ModelProviderKind::Ollama => Arc::new(OllamaSettingsEndpointValidator),
            ModelProviderKind::Gemini => Arc::new(GeminiSettingsEndpointValidator),
            ModelProviderKind::OpenAi => Arc::new(OpenAiSettingsEndpointValidator),
        };
        let configured = endpoint
            .map(|value| validator.validate(value))
            .transpose()
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let endpoint_changed = current
            .settings()
            .provider(kind)
            .endpoint()
            .map(|value| value.canonical_origin())
            != configured.as_ref().map(|value| value.canonical_origin());
        let mut current = current;
        let mut expected = expected;
        if endpoint_changed
            && current
                .settings()
                .provider(kind)
                .endpoint()
                .is_some_and(|value| {
                    value.credential_requirement() == ProviderCredentialRequirement::ApiKey
                })
            && matches!(
                current.settings().provider(kind).credential().lifecycle(),
                ProviderCredentialLifecycle::Configured
                    | ProviderCredentialLifecycle::Storing
                    | ProviderCredentialLifecycle::Deleting
            )
        {
            let (deleting_settings, generation) = current
                .settings()
                .clone()
                .begin_provider_credential_delete(kind)
                .map_err(|_| {
                    CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
                })?;
            let deleting = self
                .store
                .append(expected, &deleting_settings)
                .await
                .map_err(map_store_error)?;
            let provider_id =
                a3_domain::ModelProviderId::try_from_string(kind.provider_id().to_owned())
                    .map_err(|_| invalid_request())?;
            let fingerprint = current
                .settings()
                .provider(kind)
                .endpoint()
                .map(|value| value.origin_fingerprint())
                .unwrap_or_default();
            self.credential_store
                .delete_bound(&provider_id, &fingerprint)
                .await
                .map_err(|error| {
                    map_credential_mutation_error(
                        ManageDesktopProviderCredentialError::CredentialStore(error),
                    )
                })?;
            let missing = deleting
                .settings()
                .clone()
                .complete_provider_credential_delete(kind, generation)
                .map_err(|_| {
                    CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
                })?;
            current = self
                .store
                .append(deleting.version(), &missing)
                .await
                .map_err(map_store_error)?;
            expected = current.version();
        }
        let updated = current
            .settings()
            .clone()
            .with_provider_endpoint(kind, configured);
        let stored = self
            .store
            .append(expected, &updated)
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings_v2(&stored, false).await)
    }

    /// Stores a key for one provider slot and keeps the secret outside Settings/IPC.
    pub async fn set_credential_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKindV2,
        secret: ProviderApiKey,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let kind = provider_kind_v2(kind);
        let current = self.load_expected(expected).await?;
        let (storing_settings, generation) = current
            .settings()
            .clone()
            .begin_provider_credential_store(kind)
            .map_err(|_| {
                CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
            })?;
        let storing = self
            .store
            .append(expected, &storing_settings)
            .await
            .map_err(map_store_error)?;
        let provider_id =
            a3_domain::ModelProviderId::try_from_string(kind.provider_id().to_owned())
                .map_err(|_| invalid_request())?;
        let fingerprint = current
            .settings()
            .provider(kind)
            .endpoint()
            .map(|value| value.origin_fingerprint())
            .unwrap_or_default();
        let credential = a3_application::ProviderCredential::new(generation, secret);
        self.credential_store
            .store_bound(&provider_id, &fingerprint, &credential)
            .await
            .map_err(|error| {
                map_credential_mutation_error(
                    ManageDesktopProviderCredentialError::CredentialStore(error),
                )
            })?;
        let configured = storing
            .settings()
            .clone()
            .complete_provider_credential_store(kind, generation)
            .map_err(|_| {
                CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
            })?;
        let stored = self
            .store
            .append(storing.version(), &configured)
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings_v2(&stored, false).await)
    }

    /// Deletes one provider key while retaining its endpoint configuration.
    pub async fn delete_credential_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKindV2,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let kind = provider_kind_v2(kind);
        let current = self.load_expected(expected).await?;
        let (deleting_settings, generation) = current
            .settings()
            .clone()
            .begin_provider_credential_delete(kind)
            .map_err(|_| {
                CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
            })?;
        let deleting = self
            .store
            .append(expected, &deleting_settings)
            .await
            .map_err(map_store_error)?;
        let provider_id =
            a3_domain::ModelProviderId::try_from_string(kind.provider_id().to_owned())
                .map_err(|_| invalid_request())?;
        let fingerprint = current
            .settings()
            .provider(kind)
            .endpoint()
            .map(|value| value.origin_fingerprint())
            .unwrap_or_default();
        self.credential_store
            .delete_bound(&provider_id, &fingerprint)
            .await
            .map_err(|error| {
                map_credential_mutation_error(
                    ManageDesktopProviderCredentialError::CredentialStore(error),
                )
            })?;
        let missing = deleting
            .settings()
            .clone()
            .complete_provider_credential_delete(kind, generation)
            .map_err(|_| {
                CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired)
            })?;
        let stored = self
            .store
            .append(deleting.version(), &missing)
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings_v2(&stored, false).await)
    }

    /// Enables or disables one provider slot. Enabling is gated by verified connectivity.
    pub async fn set_enabled_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKindV2,
        enabled: bool,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let current = self.load_expected(expected).await?;
        let kind = provider_kind_v2(kind);
        if enabled
            && current
                .settings()
                .provider(kind)
                .endpoint()
                .is_some_and(|endpoint| {
                    endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
                })
        {
            self.load_provider_api_key_for(current.settings(), kind)
                .await?;
        }
        let updated = current
            .settings()
            .clone()
            .with_provider_enabled(kind, enabled)
            .map_err(|error| match error {
                a3_application::DesktopSettingsUpdateError::ConnectionUnverified => {
                    CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)
                }
                a3_application::DesktopSettingsUpdateError::CredentialUnavailable => {
                    CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialMissing)
                }
                _ => invalid_request(),
            })?;
        let stored = self
            .store
            .append(expected, &updated)
            .await
            .map_err(map_store_error)?;
        Ok(self.map_settings_v2(&stored, false).await)
    }

    async fn load_expected(
        &self,
        expected: DesktopSettingsStoreVersion,
    ) -> Result<StoredDesktopSettings, CommandErrorV1> {
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(invalid_request());
        }
        Ok(current)
    }

    /// Replaces or clears the credential-free active provider without performing a request.
    pub async fn configure_provider(
        &self,
        expected: DesktopSettingsStoreVersion,
        provider_kind: ModelProviderKindV1,
        endpoint: Option<&str>,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(invalid_request());
        }
        let switching_away_from_credential = current.settings().endpoint().is_some_and(|current| {
            current.credential_requirement() == ProviderCredentialRequirement::ApiKey
                && (endpoint.is_none()
                    || current.provider_id().as_str() != provider_kind_id(provider_kind))
        });
        let expected = if switching_away_from_credential {
            DeleteDesktopProviderCredential::new(
                Arc::clone(&self.store),
                Arc::clone(&self.credential_store),
            )
            .execute(expected)
            .await
            .map_err(map_credential_mutation_error)?
            .version()
        } else {
            expected
        };
        let validator: Arc<dyn a3_application::ModelEndpointValidator> = match provider_kind {
            ModelProviderKindV1::Ollama => Arc::new(OllamaSettingsEndpointValidator),
            ModelProviderKindV1::Gemini => Arc::new(GeminiSettingsEndpointValidator),
            ModelProviderKindV1::OpenAi => Arc::new(OpenAiSettingsEndpointValidator),
        };
        let stored = ConfigureDesktopModelEndpoint::new(Arc::clone(&self.store), validator)
            .execute(expected, endpoint)
            .await
            .map_err(map_configure_error)?;
        Ok(self.map_settings(&stored, false).await)
    }

    /// Stores a bounded API key for the Core-owned active provider without network access.
    pub async fn set_credential(
        &self,
        expected: DesktopSettingsStoreVersion,
        secret: ProviderApiKey,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        self.validate_credential_endpoint(expected).await?;
        let stored = SetDesktopProviderCredential::new(
            Arc::clone(&self.store),
            Arc::clone(&self.credential_store),
        )
        .execute(expected, secret)
        .await
        .map_err(map_credential_mutation_error)?;
        Ok(self.map_settings(&stored, false).await)
    }

    /// Deletes the current provider credential without contacting the provider.
    pub async fn delete_credential(
        &self,
        expected: DesktopSettingsStoreVersion,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let stored = DeleteDesktopProviderCredential::new(
            Arc::clone(&self.store),
            Arc::clone(&self.credential_store),
        )
        .execute(expected)
        .await
        .map_err(map_credential_mutation_error)?;
        Ok(self.map_settings(&stored, false).await)
    }

    /// Reads one bounded local model catalog after an explicit user action.
    pub async fn discover_models(
        &self,
        expected: DesktopSettingsStoreVersion,
    ) -> Result<ProviderModelsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let cancellation = self.acquire_probe()?;
        let result = self
            .discover_models_owned(expected, cancellation.as_ref())
            .await;
        self.release_probe(&cancellation);
        result
    }

    /// Explicitly tests and discovers one provider, persisting its content-free verification.
    pub async fn discover_provider_models_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKindV2,
    ) -> Result<ProviderModelsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        let cancellation = self.acquire_probe()?;
        let kind = provider_kind_v2(kind);
        let result = self
            .discover_provider_models_v2_owned(expected, kind, cancellation.as_ref())
            .await;
        self.release_probe(&cancellation);
        result
    }

    /// Runs one explicit capability probe for a provider/model tuple.
    pub async fn probe_v2(
        &self,
        expected: DesktopSettingsStoreVersion,
        request: &ProbeModelRoleRequestV2,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        validate_probe_shape_v2(request)?;
        let cancellation = self.acquire_probe()?;
        let kind = provider_kind_v2(request.provider_kind());
        let result = self
            .probe_v2_owned(expected, kind, request, cancellation.as_ref())
            .await;
        self.release_probe(&cancellation);
        result
    }

    async fn probe_v2_owned(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        request: &ProbeModelRoleRequestV2,
        control: &dyn ModelOperationControl,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let current = self.load_expected(expected).await?;
        let slot = current.settings().provider(kind);
        if !slot.enabled() || slot.connection_verified_at().is_none() {
            return Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid));
        }
        let endpoint = current
            .settings()
            .provider(kind)
            .endpoint()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let timeout = ModelRequestTimeout::from_millis(MODEL_PROBE_TIMEOUT_MILLIS)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;
        let recorder = RecordDesktopModelProbe::new(Arc::clone(&self.store));
        match kind {
            ModelProviderKind::Ollama => {
                if endpoint.scope() != ModelEndpointScope::LocalLoopback {
                    return Err(invalid_request());
                }
                let endpoint = OllamaEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| invalid_request())?;
                let provider =
                    OllamaModelProvider::new(endpoint, Arc::new(LocalOnlyOllamaEndpointPolicy))
                        .map_err(|_| invalid_request())?;
                self.execute_probe_v2(
                    &provider, &provider, expected, kind, request, timeout, recorder, control,
                )
                .await
            }
            ModelProviderKind::Gemini => {
                let origin = endpoint.canonical_origin().to_owned();
                let endpoint = GeminiEndpoint::parse(&origin).map_err(|_| invalid_request())?;
                let policy = ExactGeminiEndpointPolicy::new(origin);
                let key = self
                    .load_provider_api_key_for(current.settings(), kind)
                    .await?;
                let provider = GeminiModelProvider::new(endpoint, Arc::new(policy), key)
                    .map_err(|_| invalid_request())?;
                self.execute_probe_v2(
                    &provider, &provider, expected, kind, request, timeout, recorder, control,
                )
                .await
            }
            ModelProviderKind::OpenAi => {
                let origin = endpoint.canonical_origin().to_owned();
                let endpoint = OpenAiEndpoint::parse(&origin).map_err(|_| invalid_request())?;
                let policy = ExactOpenAiEndpointPolicy::new(origin);
                let key = self
                    .load_provider_api_key_for(current.settings(), kind)
                    .await?;
                let provider = OpenAiModelProvider::new(endpoint, Arc::new(policy), key)
                    .map_err(|_| invalid_request())?;
                self.execute_probe_v2(
                    &provider, &provider, expected, kind, request, timeout, recorder, control,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_probe_v2(
        &self,
        llm_prober: &dyn a3_application::ModelCapabilityProbe,
        embedding_prober: &dyn a3_application::EmbeddingCapabilityProbe,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        request: &ProbeModelRoleRequestV2,
        timeout: ModelRequestTimeout,
        recorder: RecordDesktopModelProbe,
        control: &dyn ModelOperationControl,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let result = match request.role() {
            ModelRoleV1::Coding | ModelRoleV1::Mapping => {
                let limits = request.llm_limits().ok_or_else(invalid_request)?;
                let settings = llm_settings(limits)?;
                let probe_request = a3_application::ModelCapabilityProbeRequest::new(
                    ModelId::try_from_string(request.model_id().to_owned())
                        .map_err(|_| invalid_request())?,
                    settings,
                );
                match ProbeModelProfile::new(llm_prober)
                    .execute(&probe_request, timeout, control)
                    .await
                {
                    Ok(profile) => {
                        let role = match request.role() {
                            ModelRoleV1::Coding => LlmModelRole::Coding,
                            ModelRoleV1::Mapping => LlmModelRole::Mapping,
                            ModelRoleV1::Embedding => return Err(invalid_request()),
                        };
                        recorder
                            .record_provider_llm(expected, kind, role, profile, settings_now()?)
                            .await
                    }
                    Err(ProbeModelProfileFailure::Provider(error)) => {
                        return self
                            .record_provider_probe_failure(
                                recorder,
                                expected,
                                kind,
                                error,
                                settings_now()?,
                            )
                            .await;
                    }
                    Err(ProbeModelProfileFailure::ContextLimitExceedsProvider { .. }) => {
                        return Err(invalid_request());
                    }
                }
            }
            ModelRoleV1::Embedding => {
                let limits = request.embedding_limits().ok_or_else(invalid_request)?;
                let probe_request = EmbeddingCapabilityProbeRequest::new(
                    EmbeddingModelId::new(request.model_id().to_owned())
                        .map_err(|_| invalid_request())?,
                    EmbeddingBatchSize::new(limits.max_batch_size())
                        .map_err(|_| invalid_request())?,
                );
                match ProbeEmbeddingModelProfile::new(embedding_prober)
                    .execute(&probe_request, timeout, control)
                    .await
                {
                    Ok(profile) => {
                        recorder
                            .record_provider_embedding(expected, kind, profile, settings_now()?)
                            .await
                    }
                    Err(error) => {
                        return self
                            .record_provider_probe_failure(
                                recorder,
                                expected,
                                kind,
                                error,
                                settings_now()?,
                            )
                            .await;
                    }
                }
            }
        }
        .map_err(map_record_error)?;
        Ok(self.map_settings_v2(&result, false).await)
    }

    async fn record_provider_probe_failure(
        &self,
        recorder: RecordDesktopModelProbe,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        error: ModelProviderFailure,
        at: SettingsTimestamp,
    ) -> Result<SettingsResponseV2, CommandErrorV1> {
        let status = if error == ModelProviderFailure::Cancelled {
            ProviderHealthStatus::Cancelled
        } else {
            ProviderHealthStatus::Unreachable
        };
        let stored = recorder
            .record_provider_failure(expected, kind, status, at)
            .await
            .map_err(map_record_error)?;
        Ok(self.map_settings_v2(&stored, false).await)
    }

    async fn discover_provider_models_v2_owned(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        control: &dyn ModelOperationControl,
    ) -> Result<ProviderModelsResponseV2, CommandErrorV1> {
        let current = self.load_expected(expected).await?;
        let endpoint = current
            .settings()
            .provider(kind)
            .endpoint()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let timeout = ModelRequestTimeout::from_millis(MODEL_DISCOVERY_TIMEOUT_MILLIS)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;
        let catalog_result = match kind {
            ModelProviderKind::Ollama => {
                if endpoint.scope() != ModelEndpointScope::LocalLoopback {
                    return Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid));
                }
                let endpoint = OllamaEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| invalid_request())?;
                let provider =
                    OllamaModelProvider::new(endpoint, Arc::new(LocalOnlyOllamaEndpointPolicy))
                        .map_err(|_| invalid_request())?;
                DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
            }
            ModelProviderKind::Gemini => {
                let origin = endpoint.canonical_origin().to_owned();
                let endpoint = GeminiEndpoint::parse(&origin).map_err(|_| invalid_request())?;
                let policy = ExactGeminiEndpointPolicy::new(origin);
                policy.authorize(&endpoint).map_err(|_| invalid_request())?;
                let key = self
                    .load_provider_api_key_for(current.settings(), kind)
                    .await?;
                let provider = GeminiModelProvider::new(endpoint, Arc::new(policy), key)
                    .map_err(|_| invalid_request())?;
                DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
            }
            ModelProviderKind::OpenAi => {
                let origin = endpoint.canonical_origin().to_owned();
                let endpoint = OpenAiEndpoint::parse(&origin).map_err(|_| invalid_request())?;
                let policy = ExactOpenAiEndpointPolicy::new(origin);
                policy.authorize(&endpoint).map_err(|_| invalid_request())?;
                let key = self
                    .load_provider_api_key_for(current.settings(), kind)
                    .await?;
                let provider = OpenAiModelProvider::new(endpoint, Arc::new(policy), key)
                    .map_err(|_| invalid_request())?;
                DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
            }
        };
        let catalog = match catalog_result {
            Ok(catalog) => catalog,
            Err(error) => {
                let status = if error == ModelProviderFailure::Cancelled {
                    ProviderHealthStatus::Cancelled
                } else {
                    ProviderHealthStatus::Unreachable
                };
                let failed = current
                    .settings()
                    .clone()
                    .with_provider_probe_failure(kind, status, settings_now()?)
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                self.store
                    .append(expected, &failed)
                    .await
                    .map_err(map_store_error)?;
                return Err(map_model_operation_error(error));
            }
        };
        let verified_at = settings_now()?;
        let updated = current
            .settings()
            .clone()
            .with_provider_connection_verified(kind, verified_at)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let stored = self
            .store
            .append(expected, &updated)
            .await
            .map_err(map_store_error)?;
        Ok(ProviderModelsResponseV2::new(
            self.map_settings_v2_snapshot(&stored, false).await,
            provider_kind_v2_back(kind),
            catalog
                .model_ids()
                .iter()
                .map(|model| model.as_str().to_owned())
                .collect(),
            catalog.truncated(),
        ))
    }

    /// Runs one bounded explicit local provider probe and persists its redacted result.
    pub async fn probe(
        &self,
        expected: DesktopSettingsStoreVersion,
        request: &ProbeModelRoleRequestV1,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let _operation = self.acquire_operation()?;
        validate_probe_shape(request)?;
        let cancellation = self.acquire_probe()?;
        let result = self
            .probe_owned(expected, request, cancellation.as_ref())
            .await;
        self.release_probe(&cancellation);
        result
    }

    /// Cooperatively cancels the one Core-owned explicit probe, if present.
    #[must_use]
    pub fn cancel_probe(&self) -> CancelModelProbeResponseV1 {
        let requested = lock_recovering_poison(&self.active_probe)
            .as_ref()
            .is_some_and(|probe| probe.request());
        CancelModelProbeResponseV1::new(requested)
    }

    async fn discover_models_owned(
        &self,
        expected: DesktopSettingsStoreVersion,
        control: &dyn ModelOperationControl,
    ) -> Result<ProviderModelsResponseV1, CommandErrorV1> {
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(invalid_request());
        }
        let endpoint = current
            .settings()
            .endpoint()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let timeout = ModelRequestTimeout::from_millis(MODEL_DISCOVERY_TIMEOUT_MILLIS)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;

        match endpoint.provider_id().as_str() {
            "ollama" => {
                if endpoint.scope() != ModelEndpointScope::LocalLoopback {
                    return Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid));
                }
                let endpoint = OllamaEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let provider =
                    OllamaModelProvider::new(endpoint, Arc::new(LocalOnlyOllamaEndpointPolicy))
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                let catalog = DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
                    .map_err(map_model_operation_error)?;
                Ok(ProviderModelsResponseV1::new(
                    current.version().get().to_string(),
                    ModelProviderKindV1::Ollama,
                    catalog
                        .model_ids()
                        .iter()
                        .map(|model| model.as_str().to_owned())
                        .collect(),
                    catalog.truncated(),
                ))
            }
            "gemini" => {
                let endpoint = GeminiEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                StandardGeminiEndpointPolicy
                    .authorize(&endpoint)
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let key = self.load_provider_api_key(current.settings()).await?;
                let provider =
                    GeminiModelProvider::new(endpoint, Arc::new(StandardGeminiEndpointPolicy), key)
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                let catalog = DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
                    .map_err(map_model_operation_error)?;
                Ok(ProviderModelsResponseV1::new(
                    current.version().get().to_string(),
                    ModelProviderKindV1::Gemini,
                    catalog
                        .model_ids()
                        .iter()
                        .map(|model| model.as_str().to_owned())
                        .collect(),
                    catalog.truncated(),
                ))
            }
            "openai" => {
                let endpoint = OpenAiEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                StandardOpenAiEndpointPolicy
                    .authorize(&endpoint)
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let key = self.load_provider_api_key(current.settings()).await?;
                let provider =
                    OpenAiModelProvider::new(endpoint, Arc::new(StandardOpenAiEndpointPolicy), key)
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                let catalog = DiscoverProviderModels::new(&provider)
                    .execute(timeout, control)
                    .await
                    .map_err(map_model_operation_error)?;
                Ok(ProviderModelsResponseV1::new(
                    current.version().get().to_string(),
                    ModelProviderKindV1::OpenAi,
                    catalog
                        .model_ids()
                        .iter()
                        .map(|model| model.as_str().to_owned())
                        .collect(),
                    catalog.truncated(),
                ))
            }
            _ => Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)),
        }
    }

    async fn probe_owned(
        &self,
        expected: DesktopSettingsStoreVersion,
        request: &ProbeModelRoleRequestV1,
        control: &dyn ModelOperationControl,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(CommandErrorV1::settings(
                ErrorCodeV1::InvalidSettingsRequest,
            ));
        }
        let endpoint = current
            .settings()
            .endpoint()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        let timeout = ModelRequestTimeout::from_millis(MODEL_PROBE_TIMEOUT_MILLIS)
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;
        let recorder = RecordDesktopModelProbe::new(Arc::clone(&self.store));

        match endpoint.provider_id().as_str() {
            "ollama" => {
                if endpoint.scope() != ModelEndpointScope::LocalLoopback {
                    return Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid));
                }
                let endpoint = OllamaEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let provider =
                    OllamaModelProvider::new(endpoint, Arc::new(LocalOnlyOllamaEndpointPolicy))
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                self.execute_probe(
                    &provider, &provider, expected, request, timeout, recorder, control,
                )
                .await
            }
            "gemini" => {
                let endpoint = GeminiEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                StandardGeminiEndpointPolicy
                    .authorize(&endpoint)
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let key = self.load_provider_api_key(current.settings()).await?;
                let provider =
                    GeminiModelProvider::new(endpoint, Arc::new(StandardGeminiEndpointPolicy), key)
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                self.execute_probe(
                    &provider, &provider, expected, request, timeout, recorder, control,
                )
                .await
            }
            "openai" => {
                let endpoint = OpenAiEndpoint::parse(endpoint.canonical_origin())
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                StandardOpenAiEndpointPolicy
                    .authorize(&endpoint)
                    .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
                let key = self.load_provider_api_key(current.settings()).await?;
                let provider =
                    OpenAiModelProvider::new(endpoint, Arc::new(StandardOpenAiEndpointPolicy), key)
                        .map_err(|_| {
                            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
                        })?;
                self.execute_probe(
                    &provider, &provider, expected, request, timeout, recorder, control,
                )
                .await
            }
            _ => Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_probe(
        &self,
        llm_prober: &dyn a3_application::ModelCapabilityProbe,
        embedding_prober: &dyn a3_application::EmbeddingCapabilityProbe,
        expected: DesktopSettingsStoreVersion,
        request: &ProbeModelRoleRequestV1,
        timeout: ModelRequestTimeout,
        recorder: RecordDesktopModelProbe,
        control: &dyn ModelOperationControl,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let result = match request.role() {
            ModelRoleV1::Coding | ModelRoleV1::Mapping => {
                let limits = request.llm_limits().ok_or_else(invalid_request)?;
                let settings = llm_settings(limits)?;
                let probe_request = a3_application::ModelCapabilityProbeRequest::new(
                    ModelId::try_from_string(request.model_id().to_owned())
                        .map_err(|_| invalid_request())?,
                    settings,
                );
                match ProbeModelProfile::new(llm_prober)
                    .execute(&probe_request, timeout, control)
                    .await
                {
                    Ok(profile) => {
                        let role = match request.role() {
                            ModelRoleV1::Coding => LlmModelRole::Coding,
                            ModelRoleV1::Mapping => LlmModelRole::Mapping,
                            ModelRoleV1::Embedding => return Err(invalid_request()),
                        };
                        recorder
                            .record_llm(expected, role, profile, settings_now()?)
                            .await
                    }
                    Err(ProbeModelProfileFailure::Provider(error)) => {
                        return self
                            .record_probe_failure(recorder, expected, error, settings_now()?)
                            .await;
                    }
                    Err(ProbeModelProfileFailure::ContextLimitExceedsProvider { .. }) => {
                        return Err(invalid_request());
                    }
                }
            }
            ModelRoleV1::Embedding => {
                let limits = request.embedding_limits().ok_or_else(invalid_request)?;
                let probe_request = EmbeddingCapabilityProbeRequest::new(
                    EmbeddingModelId::new(request.model_id().to_owned())
                        .map_err(|_| invalid_request())?,
                    EmbeddingBatchSize::new(limits.max_batch_size())
                        .map_err(|_| invalid_request())?,
                );
                match ProbeEmbeddingModelProfile::new(embedding_prober)
                    .execute(&probe_request, timeout, control)
                    .await
                {
                    Ok(profile) => {
                        recorder
                            .record_embedding(expected, profile, settings_now()?)
                            .await
                    }
                    Err(error) => {
                        return self
                            .record_probe_failure(recorder, expected, error, settings_now()?)
                            .await;
                    }
                }
            }
        }
        .map_err(map_record_error)?;
        Ok(self.map_settings(&result, false).await)
    }

    async fn record_probe_failure(
        &self,
        recorder: RecordDesktopModelProbe,
        expected: DesktopSettingsStoreVersion,
        error: ModelProviderFailure,
        at: SettingsTimestamp,
    ) -> Result<SettingsResponseV1, CommandErrorV1> {
        let status = if error == ModelProviderFailure::Cancelled {
            ProviderHealthStatus::Cancelled
        } else {
            ProviderHealthStatus::Unreachable
        };
        let stored = recorder
            .record_failure(expected, status, at)
            .await
            .map_err(map_record_error)?;
        Ok(self.map_settings(&stored, false).await)
    }

    fn acquire_probe(&self) -> Result<Arc<ProbeCancellation>, CommandErrorV1> {
        let mut active = lock_recovering_poison(&self.active_probe);
        if active.is_some() {
            return Err(CommandErrorV1::settings(
                ErrorCodeV1::ModelProbeAlreadyActive,
            ));
        }
        let cancellation = Arc::new(ProbeCancellation::new());
        *active = Some(Arc::clone(&cancellation));
        Ok(cancellation)
    }

    fn acquire_operation(&self) -> Result<ModelSettingsOperationPermit<'_>, CommandErrorV1> {
        self.operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ModelSettingsOperationPermit {
                active: &self.operation_active,
            })
            .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelProbeAlreadyActive))
    }

    async fn validate_credential_endpoint(
        &self,
        expected: DesktopSettingsStoreVersion,
    ) -> Result<(), CommandErrorV1> {
        let current = GetDesktopSettings::new(Arc::clone(&self.store))
            .execute()
            .await
            .map_err(map_store_error)?;
        if current.version() != expected {
            return Err(invalid_request());
        }
        let endpoint = current
            .settings()
            .endpoint()
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))?;
        if endpoint.access() != ModelEndpointAccess::ExplicitUserInitiatedRemote {
            return Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid));
        }
        if credential_origin_is_authorized(endpoint) {
            Ok(())
        } else {
            Err(CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid))
        }
    }

    fn release_probe(&self, completed: &Arc<ProbeCancellation>) {
        let mut active = lock_recovering_poison(&self.active_probe);
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, completed))
        {
            *active = None;
        }
    }

    fn probe_is_active(&self) -> bool {
        lock_recovering_poison(&self.active_probe).is_some()
    }

    async fn load_provider_api_key(
        &self,
        settings: &a3_application::DesktopSettings,
    ) -> Result<ProviderApiKey, CommandErrorV1> {
        LoadDesktopProviderCredential::new(Arc::clone(&self.credential_store))
            .execute(settings)
            .await
            .map_err(map_credential_access_error)?
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialMissing))
    }

    async fn load_provider_api_key_for(
        &self,
        settings: &a3_application::DesktopSettings,
        kind: ModelProviderKind,
    ) -> Result<ProviderApiKey, CommandErrorV1> {
        LoadDesktopProviderCredential::new(Arc::clone(&self.credential_store))
            .execute_for(settings, kind)
            .await
            .map_err(map_credential_access_error)?
            .ok_or_else(|| CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialMissing))
    }

    async fn map_settings(
        &self,
        stored: &StoredDesktopSettings,
        probe_active: bool,
    ) -> SettingsResponseV1 {
        let credential = credential_projection(stored, &self.credential_store).await;
        map_settings(stored, probe_active, credential)
    }

    async fn map_settings_v2(
        &self,
        stored: &StoredDesktopSettings,
        probe_active: bool,
    ) -> SettingsResponseV2 {
        SettingsResponseV2::new(self.map_settings_v2_snapshot(stored, probe_active).await)
    }

    async fn map_settings_v2_snapshot(
        &self,
        stored: &StoredDesktopSettings,
        probe_active: bool,
    ) -> SettingsV2 {
        let settings = stored.settings();
        let providers = [
            self.map_provider_settings_v2(settings, ModelProviderKind::Ollama)
                .await,
            self.map_provider_settings_v2(settings, ModelProviderKind::Gemini)
                .await,
            self.map_provider_settings_v2(settings, ModelProviderKind::OpenAi)
                .await,
        ];
        let privacy = settings.privacy();
        SettingsV2::new(
            stored.version().get().to_string(),
            providers,
            settings
                .llm_profile(LlmModelRole::Coding)
                .map(map_llm_profile_v2),
            settings
                .llm_profile(LlmModelRole::Mapping)
                .map(map_llm_profile_v2),
            settings.embedding_profile().map(map_embedding_profile_v2),
            DataPrivacySettingsV1::new(
                privacy.telemetry_enabled(),
                privacy.cloud_sync_enabled(),
                privacy.automatic_provider_discovery_enabled(),
                privacy.prompt_response_logging_enabled(),
                privacy.remote_requests_without_approval_enabled(),
            ),
            probe_active,
        )
    }

    async fn map_provider_settings_v2(
        &self,
        settings: &a3_application::DesktopSettings,
        kind: ModelProviderKind,
    ) -> ProviderSettingsV2 {
        let slot = settings.provider(kind);
        let credential = if slot.endpoint().is_some_and(|endpoint| {
            endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
        }) {
            Some(ProviderCredentialV1::api_key(
                self.provider_credential_status_v2(settings, kind).await,
            ))
        } else {
            None
        };
        let endpoint = slot.endpoint().map(map_endpoint);
        let health = Some(ProviderHealthV1::new(
            map_health_status(slot.health().status()),
            slot.health()
                .checked_at()
                .map(|value| value.unix_millis().to_string()),
        ));
        ProviderSettingsV2::new(
            provider_kind_v2_back(slot.kind()),
            slot.default_origin().to_owned(),
            endpoint,
            slot.enabled(),
            slot.configuration_revision().to_string(),
            credential,
            slot.connection_verified_at()
                .map(|value| value.unix_millis().to_string()),
            health,
        )
    }

    async fn provider_credential_status_v2(
        &self,
        settings: &a3_application::DesktopSettings,
        kind: ModelProviderKind,
    ) -> ProviderCredentialStatusV1 {
        match LoadDesktopProviderCredential::new(Arc::clone(&self.credential_store))
            .execute_for(settings, kind)
            .await
        {
            Ok(Some(_)) => ProviderCredentialStatusV1::Configured,
            Ok(None) | Err(ProviderCredentialAccessError::Missing) => {
                ProviderCredentialStatusV1::Missing
            }
            Err(ProviderCredentialAccessError::RecoveryRequired)
            | Err(ProviderCredentialAccessError::Store(ProviderCredentialStoreFailure::Corrupt)) => {
                ProviderCredentialStatusV1::RecoveryRequired
            }
            Err(ProviderCredentialAccessError::Store(
                ProviderCredentialStoreFailure::Unavailable
                | ProviderCredentialStoreFailure::ResourceLimit,
            )) => ProviderCredentialStatusV1::Unavailable,
        }
    }
}

impl fmt::Debug for ModelSettingsManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSettingsManager")
            .field("probe_active", &self.probe_is_active())
            .finish_non_exhaustive()
    }
}

struct ModelSettingsOperationPermit<'a> {
    active: &'a AtomicBool,
}

impl Drop for ModelSettingsOperationPermit<'_> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
struct ProbeCancellation {
    requested: AtomicBool,
    waiter: AtomicWaker,
}

impl ProbeCancellation {
    const fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            waiter: AtomicWaker::new(),
        }
    }

    fn request(&self) -> bool {
        let previous = self.requested.swap(true, Ordering::AcqRel);
        if !previous {
            self.waiter.wake();
        }
        !previous
    }
}

impl ModelOperationControl for ProbeCancellation {
    fn is_cancelled(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(futures::future::poll_fn(|context| {
            if self.is_cancelled() {
                return Poll::Ready(());
            }
            self.waiter.register(context.waker());
            if self.is_cancelled() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }))
    }
}

fn validate_probe_shape(request: &ProbeModelRoleRequestV1) -> Result<(), CommandErrorV1> {
    let valid = match request.role() {
        ModelRoleV1::Coding | ModelRoleV1::Mapping => {
            request.llm_limits().is_some() && request.embedding_limits().is_none()
        }
        ModelRoleV1::Embedding => {
            request.llm_limits().is_none() && request.embedding_limits().is_some()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_request())
    }
}

fn validate_probe_shape_v2(request: &ProbeModelRoleRequestV2) -> Result<(), CommandErrorV1> {
    let valid = match request.role() {
        ModelRoleV1::Coding | ModelRoleV1::Mapping => {
            request.llm_limits().is_some() && request.embedding_limits().is_none()
        }
        ModelRoleV1::Embedding => {
            request.llm_limits().is_none() && request.embedding_limits().is_some()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_request())
    }
}

fn llm_settings(
    limits: a3_protocol::LlmProbeLimitsV1,
) -> Result<ModelProfileSettings, CommandErrorV1> {
    ModelProfileSettings::new(
        ModelContextLimit::new(limits.context_tokens()).map_err(|_| invalid_request())?,
        ModelOutputLimit::new(limits.output_tokens()).map_err(|_| invalid_request())?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(limits.parallelism()).map_err(|_| invalid_request())?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(0).map_err(|_| invalid_request())?,
            ModelTopP::from_milli(1_000).map_err(|_| invalid_request())?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )
    .map_err(|_| invalid_request())
}

fn parse_version(value: &str) -> Result<DesktopSettingsStoreVersion, CommandErrorV1> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_request());
    }
    value
        .parse::<u64>()
        .ok()
        .and_then(|parsed| DesktopSettingsStoreVersion::new(parsed).ok())
        .ok_or_else(invalid_request)
}

pub(crate) fn settings_version_from_v1(
    value: &str,
) -> Result<DesktopSettingsStoreVersion, CommandErrorV1> {
    parse_version(value)
}

fn settings_now() -> Result<SettingsTimestamp, CommandErrorV1> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;
    let millis = u64::try_from(duration.as_millis())
        .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))?;
    SettingsTimestamp::from_unix_millis(millis)
        .map_err(|_| CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable))
}

async fn credential_projection(
    stored: &StoredDesktopSettings,
    credential_store: &Arc<dyn ProviderCredentialStore>,
) -> Option<ProviderCredentialV1> {
    let endpoint = stored.settings().endpoint()?;
    if endpoint.credential_requirement() == ProviderCredentialRequirement::None {
        return None;
    }
    let authorized_origin = credential_origin_is_authorized(endpoint);
    if !authorized_origin {
        return Some(ProviderCredentialV1::api_key(
            ProviderCredentialStatusV1::RecoveryRequired,
        ));
    }
    let status = match stored.settings().credential().lifecycle() {
        ProviderCredentialLifecycle::Missing => ProviderCredentialStatusV1::Missing,
        ProviderCredentialLifecycle::Storing | ProviderCredentialLifecycle::Deleting => {
            ProviderCredentialStatusV1::RecoveryRequired
        }
        ProviderCredentialLifecycle::NotRequired => ProviderCredentialStatusV1::RecoveryRequired,
        ProviderCredentialLifecycle::Configured => {
            match LoadDesktopProviderCredential::new(Arc::clone(credential_store))
                .execute(stored.settings())
                .await
            {
                Ok(Some(_)) => ProviderCredentialStatusV1::Configured,
                Ok(None) | Err(ProviderCredentialAccessError::Missing) => {
                    ProviderCredentialStatusV1::RecoveryRequired
                }
                Err(ProviderCredentialAccessError::RecoveryRequired)
                | Err(ProviderCredentialAccessError::Store(
                    ProviderCredentialStoreFailure::Corrupt,
                )) => ProviderCredentialStatusV1::RecoveryRequired,
                Err(ProviderCredentialAccessError::Store(
                    ProviderCredentialStoreFailure::Unavailable
                    | ProviderCredentialStoreFailure::ResourceLimit,
                )) => ProviderCredentialStatusV1::Unavailable,
            }
        }
    };
    Some(ProviderCredentialV1::api_key(status))
}

const fn provider_kind_id(kind: ModelProviderKindV1) -> &'static str {
    match kind {
        ModelProviderKindV1::Ollama => "ollama",
        ModelProviderKindV1::Gemini => "gemini",
        ModelProviderKindV1::OpenAi => "openai",
    }
}

const fn provider_kind_v2(kind: ModelProviderKindV2) -> ModelProviderKind {
    match kind {
        ModelProviderKindV2::Ollama => ModelProviderKind::Ollama,
        ModelProviderKindV2::Gemini => ModelProviderKind::Gemini,
        ModelProviderKindV2::OpenAi => ModelProviderKind::OpenAi,
    }
}

const fn provider_kind_v2_back(kind: ModelProviderKind) -> ModelProviderKindV2 {
    match kind {
        ModelProviderKind::Ollama => ModelProviderKindV2::Ollama,
        ModelProviderKind::Gemini => ModelProviderKindV2::Gemini,
        ModelProviderKind::OpenAi => ModelProviderKindV2::OpenAi,
    }
}

fn provider_kind_from_id(value: &str) -> ModelProviderKindV2 {
    match value {
        "gemini" => ModelProviderKindV2::Gemini,
        "openai" => ModelProviderKindV2::OpenAi,
        _ => ModelProviderKindV2::Ollama,
    }
}

fn credential_origin_is_authorized(endpoint: &a3_application::ConfiguredModelEndpoint) -> bool {
    match endpoint.provider_id().as_str() {
        "gemini" => GeminiEndpoint::parse(endpoint.canonical_origin())
            .ok()
            .is_some_and(|candidate| StandardGeminiEndpointPolicy.authorize(&candidate).is_ok()),
        "openai" => OpenAiEndpoint::parse(endpoint.canonical_origin())
            .ok()
            .is_some_and(|candidate| StandardOpenAiEndpointPolicy.authorize(&candidate).is_ok()),
        _ => false,
    }
}

fn map_settings(
    stored: &StoredDesktopSettings,
    probe_active: bool,
    credential: Option<ProviderCredentialV1>,
) -> SettingsResponseV1 {
    let settings = stored.settings();
    let endpoint = settings.endpoint().map(|endpoint| {
        ModelEndpointV1::new(
            endpoint.provider_id().as_str().to_owned(),
            endpoint.canonical_origin().to_owned(),
            match endpoint.scope() {
                ModelEndpointScope::LocalLoopback => ModelEndpointScopeV1::LocalLoopback,
                ModelEndpointScope::Remote => ModelEndpointScopeV1::Remote,
            },
            match endpoint.access() {
                ModelEndpointAccess::Local => ModelEndpointAccessV1::Local,
                ModelEndpointAccess::RemoteBlocked => ModelEndpointAccessV1::RemoteBlocked,
                ModelEndpointAccess::ExplicitUserInitiatedRemote => {
                    ModelEndpointAccessV1::ExplicitUserInitiatedRemote
                }
            },
        )
    });
    let health = settings.provider_health().map(|health| {
        ProviderHealthV1::new(
            map_health_status(health.status()),
            health
                .checked_at()
                .map(|timestamp| timestamp.unix_millis().to_string()),
        )
    });
    let privacy = settings.privacy();
    SettingsResponseV1::new(SettingsV1::new(
        stored.version().get().to_string(),
        endpoint,
        credential,
        health,
        settings
            .llm_profile(LlmModelRole::Coding)
            .map(map_llm_profile),
        settings
            .llm_profile(LlmModelRole::Mapping)
            .map(map_llm_profile),
        settings.embedding_profile().map(|selected| {
            let profile = selected.profile();
            EmbeddingRoleProfileV1::new(
                profile.id().to_string(),
                profile.model_id().as_str().to_owned(),
                profile.dimension().get(),
                profile.max_batch_size().get(),
                selected.probed_at().unix_millis().to_string(),
            )
        }),
        DataPrivacySettingsV1::new(
            privacy.telemetry_enabled(),
            privacy.cloud_sync_enabled(),
            privacy.automatic_provider_discovery_enabled(),
            privacy.prompt_response_logging_enabled(),
            privacy.remote_requests_without_approval_enabled(),
        ),
        probe_active,
    ))
}

fn map_endpoint(endpoint: &a3_application::ConfiguredModelEndpoint) -> ModelEndpointV1 {
    ModelEndpointV1::new(
        endpoint.provider_id().as_str().to_owned(),
        endpoint.canonical_origin().to_owned(),
        match endpoint.scope() {
            ModelEndpointScope::LocalLoopback => ModelEndpointScopeV1::LocalLoopback,
            ModelEndpointScope::Remote => ModelEndpointScopeV1::Remote,
        },
        match endpoint.access() {
            ModelEndpointAccess::Local => ModelEndpointAccessV1::Local,
            ModelEndpointAccess::RemoteBlocked => ModelEndpointAccessV1::RemoteBlocked,
            ModelEndpointAccess::ExplicitUserInitiatedRemote => {
                ModelEndpointAccessV1::ExplicitUserInitiatedRemote
            }
        },
    )
}

fn map_llm_profile_v2(selected: &a3_application::LlmRoleProfile) -> LlmRoleProfileV2 {
    let profile = selected.profile();
    LlmRoleProfileV2::new(
        provider_kind_from_id(profile.provider_id().as_str()),
        profile.id().to_string(),
        profile.model_id().as_str().to_owned(),
        profile.settings().context_limit().get(),
        profile.settings().output_limit().get(),
        profile.settings().parallelism_limit().get(),
        match profile.capabilities().structured_output() {
            ModelStructuredOutputCapability::Verified => StructuredOutputCapabilityV1::Verified,
            ModelStructuredOutputCapability::Unavailable => {
                StructuredOutputCapabilityV1::Unavailable
            }
        },
        match profile.capabilities().tool_call_mode() {
            ModelToolCallMode::Disabled => ModelToolCallModeV1::Disabled,
            ModelToolCallMode::NativeProviderReported => {
                ModelToolCallModeV1::NativeProviderReported
            }
        },
        match selected.activation() {
            LlmProfileActivation::Executable => ModelProfileActivationV1::Executable,
            LlmProfileActivation::CapabilityLimited => ModelProfileActivationV1::CapabilityLimited,
        },
        selected.probed_at().unix_millis().to_string(),
    )
}

fn map_embedding_profile_v2(
    selected: &a3_application::VerifiedEmbeddingProfile,
) -> EmbeddingRoleProfileV2 {
    let profile = selected.profile();
    EmbeddingRoleProfileV2::new(
        provider_kind_from_id(profile.provider_id().as_str()),
        profile.id().to_string(),
        profile.model_id().as_str().to_owned(),
        profile.dimension().get(),
        profile.max_batch_size().get(),
        selected.probed_at().unix_millis().to_string(),
    )
}

fn map_llm_profile(selected: &a3_application::LlmRoleProfile) -> LlmRoleProfileV1 {
    let profile = selected.profile();
    LlmRoleProfileV1::new(
        profile.id().to_string(),
        profile.model_id().as_str().to_owned(),
        profile.settings().context_limit().get(),
        profile.settings().output_limit().get(),
        profile.settings().parallelism_limit().get(),
        match profile.capabilities().structured_output() {
            ModelStructuredOutputCapability::Verified => StructuredOutputCapabilityV1::Verified,
            ModelStructuredOutputCapability::Unavailable => {
                StructuredOutputCapabilityV1::Unavailable
            }
        },
        match profile.capabilities().tool_call_mode() {
            ModelToolCallMode::Disabled => ModelToolCallModeV1::Disabled,
            ModelToolCallMode::NativeProviderReported => {
                ModelToolCallModeV1::NativeProviderReported
            }
        },
        match selected.activation() {
            LlmProfileActivation::Executable => ModelProfileActivationV1::Executable,
            LlmProfileActivation::CapabilityLimited => ModelProfileActivationV1::CapabilityLimited,
        },
        selected.probed_at().unix_millis().to_string(),
    )
}

const fn map_health_status(status: ProviderHealthStatus) -> ProviderHealthStatusV1 {
    match status {
        ProviderHealthStatus::NotChecked => ProviderHealthStatusV1::NotChecked,
        ProviderHealthStatus::Healthy => ProviderHealthStatusV1::Healthy,
        ProviderHealthStatus::CapabilityLimited => ProviderHealthStatusV1::CapabilityLimited,
        ProviderHealthStatus::Unreachable => ProviderHealthStatusV1::Unreachable,
        ProviderHealthStatus::Cancelled => ProviderHealthStatusV1::Cancelled,
        ProviderHealthStatus::RemoteBlocked => ProviderHealthStatusV1::RemoteBlocked,
    }
}

fn invalid_request() -> CommandErrorV1 {
    CommandErrorV1::settings(ErrorCodeV1::InvalidSettingsRequest)
}

fn map_store_error(error: DesktopSettingsStoreFailure) -> CommandErrorV1 {
    let code = match error {
        DesktopSettingsStoreFailure::VersionConflict => ErrorCodeV1::InvalidSettingsRequest,
        DesktopSettingsStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        DesktopSettingsStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        DesktopSettingsStoreFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
        DesktopSettingsStoreFailure::Unavailable | DesktopSettingsStoreFailure::ResourceLimit => {
            ErrorCodeV1::ModelSettingsUnavailable
        }
    };
    CommandErrorV1::settings(code)
}

fn map_configure_error(error: ConfigureDesktopModelEndpointError) -> CommandErrorV1 {
    match error {
        ConfigureDesktopModelEndpointError::Validation(_) => {
            CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)
        }
        ConfigureDesktopModelEndpointError::Store(error) => map_store_error(error),
    }
}

fn map_record_error(error: RecordDesktopModelProbeError) -> CommandErrorV1 {
    match error {
        RecordDesktopModelProbeError::InvalidEvidence(_) => {
            CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)
        }
        RecordDesktopModelProbeError::Store(error) => map_store_error(error),
    }
}

fn map_model_operation_error(error: ModelProviderFailure) -> CommandErrorV1 {
    match error {
        ModelProviderFailure::EndpointDenied => {
            CommandErrorV1::settings(ErrorCodeV1::ModelEndpointInvalid)
        }
        ModelProviderFailure::Unavailable
        | ModelProviderFailure::Rejected
        | ModelProviderFailure::InvalidResponse
        | ModelProviderFailure::TimedOut
        | ModelProviderFailure::Cancelled => {
            CommandErrorV1::settings(ErrorCodeV1::ModelSettingsUnavailable)
        }
    }
}

fn map_credential_access_error(error: ProviderCredentialAccessError) -> CommandErrorV1 {
    let code = match error {
        ProviderCredentialAccessError::Missing => ErrorCodeV1::ProviderCredentialMissing,
        ProviderCredentialAccessError::RecoveryRequired => {
            ErrorCodeV1::ProviderCredentialRecoveryRequired
        }
        ProviderCredentialAccessError::Store(ProviderCredentialStoreFailure::Corrupt) => {
            ErrorCodeV1::ProviderCredentialRecoveryRequired
        }
        ProviderCredentialAccessError::Store(
            ProviderCredentialStoreFailure::Unavailable
            | ProviderCredentialStoreFailure::ResourceLimit,
        ) => ErrorCodeV1::ProviderCredentialStoreUnavailable,
    };
    CommandErrorV1::settings(code)
}

fn map_credential_mutation_error(error: ManageDesktopProviderCredentialError) -> CommandErrorV1 {
    match error {
        ManageDesktopProviderCredentialError::SettingsStore(error) => map_store_error(error),
        ManageDesktopProviderCredentialError::CredentialStore(
            ProviderCredentialStoreFailure::Corrupt,
        ) => CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired),
        ManageDesktopProviderCredentialError::CredentialStore(
            ProviderCredentialStoreFailure::Unavailable
            | ProviderCredentialStoreFailure::ResourceLimit,
        ) => CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialStoreUnavailable),
        ManageDesktopProviderCredentialError::InvalidState(
            a3_application::DesktopSettingsUpdateError::CredentialUnavailable
            | a3_application::DesktopSettingsUpdateError::InvalidCredentialState,
        ) => CommandErrorV1::settings(ErrorCodeV1::ProviderCredentialRecoveryRequired),
        ManageDesktopProviderCredentialError::InvalidState(_) => {
            CommandErrorV1::settings(ErrorCodeV1::InvalidSettingsRequest)
        }
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelSettingsManager, settings_version_from_v1};
    use a3_application::{
        DesktopSettings, DesktopSettingsStore, DesktopSettingsStoreFailure,
        DesktopSettingsStoreFuture, DesktopSettingsStoreVersion, ProviderCredential,
        ProviderCredentialStore, ProviderCredentialStoreFuture, StoredDesktopSettings,
    };
    use a3_domain::ModelProviderId;
    use a3_protocol::{ErrorCodeV1, ModelProviderKindV1};
    use std::sync::{Arc, Mutex, MutexGuard};

    #[derive(Debug)]
    struct MemoryStore(Mutex<StoredDesktopSettings>);

    impl MemoryStore {
        fn new() -> Self {
            Self(Mutex::new(StoredDesktopSettings::initial()))
        }

        fn lock(&self) -> MutexGuard<'_, StoredDesktopSettings> {
            match self.0.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            }
        }
    }

    impl DesktopSettingsStore for MemoryStore {
        fn load<'a>(&'a self) -> DesktopSettingsStoreFuture<'a, StoredDesktopSettings> {
            Box::pin(async move { Ok(self.lock().clone()) })
        }

        fn append<'a>(
            &'a self,
            expected: DesktopSettingsStoreVersion,
            settings: &'a DesktopSettings,
        ) -> DesktopSettingsStoreFuture<'a, StoredDesktopSettings> {
            Box::pin(async move {
                let mut current = self.lock();
                if current.version() != expected {
                    return Err(DesktopSettingsStoreFailure::VersionConflict);
                }
                let next = DesktopSettingsStoreVersion::new(expected.get() + 1)
                    .map_err(|_| DesktopSettingsStoreFailure::ResourceLimit)?;
                *current = StoredDesktopSettings::new(next, settings.clone());
                Ok(current.clone())
            })
        }
    }

    #[derive(Debug, Default)]
    struct EmptyCredentialStore;

    impl ProviderCredentialStore for EmptyCredentialStore {
        fn load<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
        ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
            Box::pin(async { Ok(None) })
        }

        fn store<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
            _credential: &'a ProviderCredential,
        ) -> ProviderCredentialStoreFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }

        fn delete<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
        ) -> ProviderCredentialStoreFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn manager(store: Arc<dyn DesktopSettingsStore>) -> ModelSettingsManager {
        let credentials: Arc<dyn ProviderCredentialStore> = Arc::new(EmptyCredentialStore);
        ModelSettingsManager::new(store, credentials)
    }

    #[test]
    fn empty_manager_projects_model_free_settings() -> Result<(), Box<dyn std::error::Error>> {
        futures::executor::block_on(async {
            let store: Arc<dyn DesktopSettingsStore> = Arc::new(MemoryStore::new());
            let manager = manager(store);
            let response = manager
                .query()
                .await
                .map_err(|error| format!("settings query failed: {:?}", error.code()))?;
            let json = serde_json::to_value(response)?;
            assert_eq!(json["settings"]["revision"], "0");
            assert!(json["settings"]["endpoint"].is_null());
            assert_eq!(json["settings"]["probeActive"], false);
            assert_eq!(json["settings"]["privacy"]["telemetryEnabled"], false);
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn decimal_revision_parser_is_canonical_and_bounded() {
        assert_eq!(
            settings_version_from_v1("0").map(|value| value.get()),
            Ok(0)
        );
        for invalid in ["", "00", "+1", "-1", " 1", "18446744073709551616"] {
            assert_eq!(
                settings_version_from_v1(invalid).map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidSettingsRequest)
            );
        }
    }

    #[test]
    fn remote_endpoint_is_visible_but_cannot_be_probed() -> Result<(), Box<dyn std::error::Error>> {
        futures::executor::block_on(async {
            let store: Arc<dyn DesktopSettingsStore> = Arc::new(MemoryStore::new());
            let manager = manager(store);
            let configured = manager
                .configure_provider(
                    DesktopSettingsStoreVersion::initial(),
                    ModelProviderKindV1::Ollama,
                    Some("https://models.example.test"),
                )
                .await
                .map_err(|error| format!("endpoint configuration failed: {:?}", error.code()))?;
            let configured_json = serde_json::to_value(configured)?;
            assert_eq!(configured_json["settings"]["endpoint"]["scope"], "remote");
            assert_eq!(
                configured_json["settings"]["providerHealth"]["status"],
                "remoteBlocked"
            );

            let request = serde_json::from_value(serde_json::json!({
                "protocolVersion": 1,
                "expectedSettingsRevision": "1",
                "role": "coding",
                "modelId": "qwen-coder",
                "llmLimits": {
                    "contextTokens": 8192,
                    "outputTokens": 1024,
                    "parallelism": 1
                },
                "embeddingLimits": null
            }))?;
            assert_eq!(
                manager
                    .probe(DesktopSettingsStoreVersion::new(1)?, &request)
                    .await
                    .map_err(|error| error.code()),
                Err(ErrorCodeV1::ModelEndpointInvalid)
            );
            assert_eq!(
                manager
                    .discover_models(DesktopSettingsStoreVersion::new(1)?)
                    .await
                    .map_err(|error| error.code()),
                Err(ErrorCodeV1::ModelEndpointInvalid)
            );
            assert!(!manager.probe_is_active());
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn gemini_provider_can_be_configured() -> Result<(), Box<dyn std::error::Error>> {
        futures::executor::block_on(async {
            let store: Arc<dyn DesktopSettingsStore> = Arc::new(MemoryStore::new());
            let manager = manager(store);
            let configured = manager
                .configure_provider(
                    DesktopSettingsStoreVersion::initial(),
                    ModelProviderKindV1::Gemini,
                    Some("https://generativelanguage.googleapis.com"),
                )
                .await
                .map_err(|error| format!("gemini configuration failed: {:?}", error.code()))?;
            let configured_json = serde_json::to_value(configured)?;
            assert_eq!(configured_json["settings"]["endpoint"]["scope"], "remote");
            assert_eq!(
                configured_json["settings"]["endpoint"]["origin"],
                "https://generativelanguage.googleapis.com"
            );
            assert_eq!(
                configured_json["settings"]["providerHealth"]["status"],
                "notChecked"
            );
            assert_eq!(
                configured_json["settings"]["credential"]["status"],
                "missing"
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn openai_provider_can_be_configured() -> Result<(), Box<dyn std::error::Error>> {
        futures::executor::block_on(async {
            let store: Arc<dyn DesktopSettingsStore> = Arc::new(MemoryStore::new());
            let manager = manager(store);
            let configured = manager
                .configure_provider(
                    DesktopSettingsStoreVersion::initial(),
                    ModelProviderKindV1::OpenAi,
                    Some("https://api.openai.com"),
                )
                .await
                .map_err(|error| format!("openai configuration failed: {:?}", error.code()))?;
            let configured_json = serde_json::to_value(configured)?;
            assert_eq!(configured_json["settings"]["endpoint"]["scope"], "remote");
            assert_eq!(
                configured_json["settings"]["endpoint"]["providerId"],
                "openai"
            );
            assert_eq!(
                configured_json["settings"]["endpoint"]["origin"],
                "https://api.openai.com"
            );
            assert_eq!(
                configured_json["settings"]["providerHealth"]["status"],
                "notChecked"
            );
            assert_eq!(
                configured_json["settings"]["credential"]["status"],
                "missing"
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
