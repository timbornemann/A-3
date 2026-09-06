use a3_domain::{
    EmbeddingModelProfile, ModelProfile, ModelProviderId, ModelStructuredOutputCapability,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::{
    ProviderApiKey, ProviderCredential, ProviderCredentialGeneration, ProviderCredentialStore,
    ProviderCredentialStoreFailure,
};

const MAX_ENDPOINT_ORIGIN_BYTES: usize = 2_048;
const MAX_SETTINGS_STORE_VERSION: u64 = i64::MAX as u64;
const MAX_SETTINGS_TIMESTAMP_MILLIS: u64 = i64::MAX as u64;

/// Whether a validated provider origin stays on the host loopback boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelEndpointScope {
    /// The origin resolves from a literal loopback address and needs no network approval.
    LocalLoopback,
    /// The credential-free HTTPS origin is non-local and remains blocked without exact approval.
    Remote,
}

/// Provider-neutral authorization class attached by the concrete endpoint validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelEndpointAccess {
    /// Requests stay on a literal loopback origin.
    Local,
    /// The remote origin is retained for display but no settings operation may contact it.
    RemoteBlocked,
    /// Only an explicit user-initiated settings operation may contact the fixed remote origin.
    ExplicitUserInitiatedRemote,
}

/// Credential shape required by a validated provider connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderCredentialRequirement {
    /// The provider connection does not use a stored credential.
    None,
    /// The provider connection requires one OS-stored API key.
    ApiKey,
}

/// Durable content-free phase of the cross-store credential lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderCredentialLifecycle {
    /// No credential belongs to the configured provider.
    NotRequired,
    /// A credential is required but no usable generation is configured.
    Missing,
    /// Settings were committed before writing the external credential.
    Storing,
    /// Settings and the external credential must contain the same generation.
    Configured,
    /// Settings were committed before deleting the external credential.
    Deleting,
}

/// Persistable credential metadata that never contains secret-derived material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCredentialMetadata {
    lifecycle: ProviderCredentialLifecycle,
    generation: ProviderCredentialGeneration,
}

impl ProviderCredentialMetadata {
    /// Returns metadata for a connection that cannot own a credential.
    #[must_use]
    pub const fn not_required() -> Self {
        Self {
            lifecycle: ProviderCredentialLifecycle::NotRequired,
            generation: ProviderCredentialGeneration::initial(),
        }
    }

    /// Returns initial metadata for a provider requiring an API key.
    #[must_use]
    pub const fn missing() -> Self {
        Self {
            lifecycle: ProviderCredentialLifecycle::Missing,
            generation: ProviderCredentialGeneration::initial(),
        }
    }

    /// Reconstructs validated content-free metadata from persistence.
    pub const fn from_stored_parts(
        lifecycle: ProviderCredentialLifecycle,
        generation: ProviderCredentialGeneration,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let valid = match lifecycle {
            ProviderCredentialLifecycle::NotRequired => generation.get() == 0,
            ProviderCredentialLifecycle::Missing => true,
            ProviderCredentialLifecycle::Storing
            | ProviderCredentialLifecycle::Configured
            | ProviderCredentialLifecycle::Deleting => generation.get() > 0,
        };
        if valid {
            Ok(Self {
                lifecycle,
                generation,
            })
        } else {
            Err(DesktopSettingsUpdateError::InvalidCredentialState)
        }
    }

    /// Returns the durable lifecycle phase.
    #[must_use]
    pub const fn lifecycle(self) -> ProviderCredentialLifecycle {
        self.lifecycle
    }

    /// Returns the monotone content-free generation.
    #[must_use]
    pub const fn generation(self) -> ProviderCredentialGeneration {
        self.generation
    }
}

/// Credential-free canonical origin produced by a concrete provider adapter.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfiguredModelEndpoint {
    provider_id: ModelProviderId,
    canonical_origin: String,
    scope: ModelEndpointScope,
    access: ModelEndpointAccess,
    credential_requirement: ProviderCredentialRequirement,
}

impl ConfiguredModelEndpoint {
    /// Revalidates the provider-neutral safety envelope around an adapter-canonicalized origin.
    pub fn from_validated_adapter(
        provider_id: ModelProviderId,
        canonical_origin: String,
        scope: ModelEndpointScope,
    ) -> Result<Self, ConfiguredModelEndpointError> {
        let access = match scope {
            ModelEndpointScope::LocalLoopback => ModelEndpointAccess::Local,
            ModelEndpointScope::Remote => ModelEndpointAccess::RemoteBlocked,
        };
        Self::from_validated_adapter_with_security(
            provider_id,
            canonical_origin,
            scope,
            access,
            ProviderCredentialRequirement::None,
        )
    }

    /// Revalidates an adapter origin together with its provider-neutral access and credential policy.
    pub fn from_validated_adapter_with_security(
        provider_id: ModelProviderId,
        canonical_origin: String,
        scope: ModelEndpointScope,
        access: ModelEndpointAccess,
        credential_requirement: ProviderCredentialRequirement,
    ) -> Result<Self, ConfiguredModelEndpointError> {
        if canonical_origin.is_empty() || canonical_origin.len() > MAX_ENDPOINT_ORIGIN_BYTES {
            return Err(ConfiguredModelEndpointError::InvalidLength {
                actual: canonical_origin.len(),
            });
        }
        if canonical_origin.chars().any(char::is_control) {
            return Err(ConfiguredModelEndpointError::UnsafeCharacter);
        }
        let remainder = canonical_origin
            .strip_prefix("http://")
            .or_else(|| canonical_origin.strip_prefix("https://"))
            .ok_or(ConfiguredModelEndpointError::InvalidOrigin)?;
        if remainder.is_empty()
            || remainder.contains(['/', '?', '#', '@'])
            || (scope == ModelEndpointScope::Remote && !canonical_origin.starts_with("https://"))
        {
            return Err(ConfiguredModelEndpointError::InvalidOrigin);
        }
        let policy_is_valid = matches!(
            (scope, access, credential_requirement),
            (
                ModelEndpointScope::LocalLoopback,
                ModelEndpointAccess::Local,
                ProviderCredentialRequirement::None
            ) | (
                ModelEndpointScope::Remote,
                ModelEndpointAccess::RemoteBlocked,
                ProviderCredentialRequirement::None
            ) | (
                ModelEndpointScope::Remote,
                ModelEndpointAccess::ExplicitUserInitiatedRemote,
                ProviderCredentialRequirement::ApiKey
            )
        );
        if !policy_is_valid {
            return Err(ConfiguredModelEndpointError::InvalidSecurityPolicy);
        }
        Ok(Self {
            provider_id,
            canonical_origin,
            scope,
            access,
            credential_requirement,
        })
    }

    /// Returns the stable provider identity without endpoint material.
    #[must_use]
    pub const fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    /// Returns the adapter-canonicalized credential-free origin.
    #[must_use]
    pub fn canonical_origin(&self) -> &str {
        &self.canonical_origin
    }

    /// Returns a stable, content-free fingerprint for credential origin binding.
    #[must_use]
    pub fn origin_fingerprint(&self) -> String {
        blake3::hash(self.canonical_origin.as_bytes())
            .to_hex()
            .to_string()
    }

    /// Returns whether requests remain on literal host loopback.
    #[must_use]
    pub const fn scope(&self) -> ModelEndpointScope {
        self.scope
    }

    /// Returns the provider-neutral request authorization class.
    #[must_use]
    pub const fn access(&self) -> ModelEndpointAccess {
        self.access
    }

    /// Returns whether the connection requires an OS-stored API key.
    #[must_use]
    pub const fn credential_requirement(&self) -> ProviderCredentialRequirement {
        self.credential_requirement
    }
}

impl fmt::Debug for ConfiguredModelEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfiguredModelEndpoint")
            .field("provider_id", &self.provider_id)
            .field("scope", &self.scope)
            .field("access", &self.access)
            .field("credential_requirement", &self.credential_requirement)
            .finish_non_exhaustive()
    }
}

/// Adapter output did not satisfy the bounded credential-free origin envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfiguredModelEndpointError {
    /// The origin was empty or exceeded the settings allocation boundary.
    InvalidLength {
        /// Observed UTF-8 byte length.
        actual: usize,
    },
    /// The origin contained a control character.
    UnsafeCharacter,
    /// The value was not a pathless HTTP(S) origin or a remote origin was not HTTPS.
    InvalidOrigin,
    /// Locality, access, and credential policy formed an unsupported combination.
    InvalidSecurityPolicy,
}

impl fmt::Display for ConfiguredModelEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "model endpoint origin length is invalid",
            Self::UnsafeCharacter => "model endpoint origin contains an unsafe character",
            Self::InvalidOrigin => "model endpoint is not a credential-free canonical origin",
            Self::InvalidSecurityPolicy => "model endpoint security policy is inconsistent",
        })
    }
}

impl Error for ConfiguredModelEndpointError {}

/// Stable validation failure from a concrete provider endpoint adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelEndpointValidationFailure {
    /// The supplied value is not a supported safe origin.
    Invalid,
    /// The selected provider configuration is not available in this build.
    ProviderUnavailable,
}

impl fmt::Display for ModelEndpointValidationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Invalid => "model endpoint is invalid or unsafe",
            Self::ProviderUnavailable => "model endpoint provider is unavailable",
        })
    }
}

impl Error for ModelEndpointValidationFailure {}

/// Provider-owned pure validation boundary for one user-entered endpoint.
pub trait ModelEndpointValidator: fmt::Debug + Send + Sync {
    /// Parses and canonicalizes an origin without performing network access.
    fn validate(
        &self,
        input: &str,
    ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure>;
}

/// Core-owned timestamp attached to settings and probe observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingsTimestamp(u64);

impl SettingsTimestamp {
    /// Validates a Unix-millisecond value representable by local persistence.
    pub const fn from_unix_millis(value: u64) -> Result<Self, SettingsTimestampError> {
        if value > MAX_SETTINGS_TIMESTAMP_MILLIS {
            Err(SettingsTimestampError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

/// Settings timestamp exceeded the local persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsTimestampError {
    value: u64,
}

impl fmt::Display for SettingsTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "settings timestamp {} is out of range",
            self.value
        )
    }
}

impl Error for SettingsTimestampError {}

/// Closed executable LLM role exposed by desktop settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LlmModelRole {
    /// Profile used for coding-agent turns.
    Coding,
    /// Profile used for Deep Map exploration.
    Mapping,
}

/// Whether a probed LLM profile is eligible for executable structured actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LlmProfileActivation {
    /// Live strict-schema evidence permits executable requests.
    Executable,
    /// The candidate remains visible but cannot drive executable requests.
    CapabilityLimited,
}

/// One role-bound live-probed LLM profile and its derived activation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRoleProfile {
    profile: ModelProfile,
    probed_at: SettingsTimestamp,
    activation: LlmProfileActivation,
}

impl LlmRoleProfile {
    /// Derives activation only from the profile's live structured-output evidence.
    #[must_use]
    pub fn from_probe(profile: ModelProfile, probed_at: SettingsTimestamp) -> Self {
        let activation = if profile.executable_actions_enabled() {
            LlmProfileActivation::Executable
        } else {
            LlmProfileActivation::CapabilityLimited
        };
        Self {
            profile,
            probed_at,
            activation,
        }
    }

    /// Returns the complete immutable model profile.
    #[must_use]
    pub const fn profile(&self) -> &ModelProfile {
        &self.profile
    }

    /// Returns the Core-owned probe time.
    #[must_use]
    pub const fn probed_at(&self) -> SettingsTimestamp {
        self.probed_at
    }

    /// Returns the capability-derived activation state.
    #[must_use]
    pub const fn activation(&self) -> LlmProfileActivation {
        self.activation
    }
}

/// One embedding profile proven by a bounded live provider vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEmbeddingProfile {
    profile: EmbeddingModelProfile,
    probed_at: SettingsTimestamp,
}

impl VerifiedEmbeddingProfile {
    /// Records an embedding profile created from a successful live dimension probe.
    #[must_use]
    pub const fn from_probe(profile: EmbeddingModelProfile, probed_at: SettingsTimestamp) -> Self {
        Self { profile, probed_at }
    }

    /// Returns the exact vector-shaping profile.
    #[must_use]
    pub const fn profile(&self) -> &EmbeddingModelProfile {
        &self.profile
    }

    /// Returns the Core-owned probe time.
    #[must_use]
    pub const fn probed_at(&self) -> SettingsTimestamp {
        self.probed_at
    }
}

/// Last explicit provider-health observation for the configured endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderHealthStatus {
    /// No explicit probe has run for this local endpoint.
    NotChecked,
    /// At least one role probe succeeded with its required capability.
    Healthy,
    /// The provider answered but the requested executable capability was unavailable.
    CapabilityLimited,
    /// The provider request failed without exposing adapter details.
    Unreachable,
    /// The user cancelled the most recent explicit probe.
    Cancelled,
    /// The configured non-local origin is not authorized for a request.
    RemoteBlocked,
}

/// The three closed provider slots exposed by the desktop settings boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelProviderKind {
    /// Local Ollama-compatible provider.
    Ollama,
    /// Google Gemini provider.
    Gemini,
    /// OpenAI-compatible provider using the native OpenAI wire contract.
    OpenAi,
}

impl ModelProviderKind {
    /// Returns the stable provider identifier used by domain profiles and keyring entries.
    #[must_use]
    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Gemini => "gemini",
            Self::OpenAi => "openai",
        }
    }

    /// Returns the official credential-free origin shown as the slot default.
    #[must_use]
    pub const fn default_origin(self) -> &'static str {
        match self {
            Self::Ollama => "http://127.0.0.1:11434",
            Self::Gemini => "https://generativelanguage.googleapis.com",
            Self::OpenAi => "https://api.openai.com",
        }
    }

    /// Returns the fingerprint used to permit one-time legacy-key migration for the official origin.
    #[must_use]
    pub fn default_origin_fingerprint(self) -> String {
        blake3::hash(self.default_origin().as_bytes())
            .to_hex()
            .to_string()
    }

    /// Returns the canonical ordering used by persistence and IPC.
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Ollama, Self::Gemini, Self::OpenAi]
    }

    /// Resolves a stable provider identifier.
    #[must_use]
    pub fn from_provider_id(value: &str) -> Option<Self> {
        match value {
            "ollama" => Some(Self::Ollama),
            "gemini" => Some(Self::Gemini),
            "openai" => Some(Self::OpenAi),
            _ => None,
        }
    }
}

/// One durable provider slot. Secrets are represented only by content-free metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    kind: ModelProviderKind,
    endpoint: Option<ConfiguredModelEndpoint>,
    enabled: bool,
    configuration_revision: u64,
    credential: ProviderCredentialMetadata,
    health: ProviderHealthObservation,
    connection_verified_at: Option<SettingsTimestamp>,
}

impl ProviderSettings {
    /// Creates an untouched, disabled provider slot.
    #[must_use]
    pub const fn initial(kind: ModelProviderKind) -> Self {
        Self {
            kind,
            endpoint: None,
            enabled: false,
            configuration_revision: 0,
            credential: ProviderCredentialMetadata::not_required(),
            health: ProviderHealthObservation {
                status: ProviderHealthStatus::NotChecked,
                checked_at: None,
            },
            connection_verified_at: None,
        }
    }

    /// Returns the closed provider kind.
    #[must_use]
    pub const fn kind(&self) -> ModelProviderKind {
        self.kind
    }

    /// Returns the configured endpoint, if one was explicitly saved.
    #[must_use]
    pub const fn endpoint(&self) -> Option<&ConfiguredModelEndpoint> {
        self.endpoint.as_ref()
    }

    /// Returns the canonical official origin used for this slot's default.
    #[must_use]
    pub const fn default_origin(&self) -> &'static str {
        self.kind.default_origin()
    }

    /// Returns whether the provider is currently enabled for model selection.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the provider-local configuration revision.
    #[must_use]
    pub const fn configuration_revision(&self) -> u64 {
        self.configuration_revision
    }

    /// Returns content-free credential metadata.
    #[must_use]
    pub const fn credential(&self) -> ProviderCredentialMetadata {
        self.credential
    }

    /// Returns the most recent explicit health observation.
    #[must_use]
    pub const fn health(&self) -> ProviderHealthObservation {
        self.health
    }

    /// Returns the content-free successful connection time.
    #[must_use]
    pub const fn connection_verified_at(&self) -> Option<SettingsTimestamp> {
        self.connection_verified_at
    }
}

/// Timestamped health evidence, or the initial state without a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderHealthObservation {
    status: ProviderHealthStatus,
    checked_at: Option<SettingsTimestamp>,
}

impl ProviderHealthObservation {
    /// Creates the non-networked initial state for an endpoint.
    #[must_use]
    pub const fn initial(scope: ModelEndpointScope) -> Self {
        Self {
            status: match scope {
                ModelEndpointScope::LocalLoopback => ProviderHealthStatus::NotChecked,
                ModelEndpointScope::Remote => ProviderHealthStatus::RemoteBlocked,
            },
            checked_at: None,
        }
    }

    /// Creates the non-networked initial state for a configured endpoint, allowing Gemini.
    #[must_use]
    pub fn initial_for_endpoint(endpoint: &ConfiguredModelEndpoint) -> Self {
        Self {
            status: match endpoint.access() {
                ModelEndpointAccess::Local | ModelEndpointAccess::ExplicitUserInitiatedRemote => {
                    ProviderHealthStatus::NotChecked
                }
                ModelEndpointAccess::RemoteBlocked => ProviderHealthStatus::RemoteBlocked,
            },
            checked_at: None,
        }
    }

    /// Records one completed explicit local probe.
    pub const fn checked(
        status: ProviderHealthStatus,
        checked_at: SettingsTimestamp,
    ) -> Result<Self, ProviderHealthObservationError> {
        match status {
            ProviderHealthStatus::NotChecked | ProviderHealthStatus::RemoteBlocked => {
                Err(ProviderHealthObservationError)
            }
            ProviderHealthStatus::Healthy
            | ProviderHealthStatus::CapabilityLimited
            | ProviderHealthStatus::Unreachable
            | ProviderHealthStatus::Cancelled => Ok(Self {
                status,
                checked_at: Some(checked_at),
            }),
        }
    }

    /// Returns the closed health state.
    #[must_use]
    pub const fn status(self) -> ProviderHealthStatus {
        self.status
    }

    /// Returns the observation time when a request completed.
    #[must_use]
    pub const fn checked_at(self) -> Option<SettingsTimestamp> {
        self.checked_at
    }
}

/// An initial-only provider-health state was incorrectly timestamped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderHealthObservationError;

impl fmt::Display for ProviderHealthObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider health status is inconsistent with a completed probe")
    }
}

impl Error for ProviderHealthObservationError {}

/// Immutable offline-first data-boundary projection shown by Settings V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPrivacySettings {
    telemetry_enabled: bool,
    cloud_sync_enabled: bool,
    automatic_provider_discovery_enabled: bool,
    prompt_response_logging_enabled: bool,
    remote_requests_without_approval_enabled: bool,
}

impl DataPrivacySettings {
    /// Returns the fixed V1 privacy baseline; unsupported capabilities cannot be toggled on.
    #[must_use]
    pub const fn offline_first_v1() -> Self {
        Self {
            telemetry_enabled: false,
            cloud_sync_enabled: false,
            automatic_provider_discovery_enabled: false,
            prompt_response_logging_enabled: false,
            remote_requests_without_approval_enabled: false,
        }
    }

    /// Returns whether telemetry is enabled.
    #[must_use]
    pub const fn telemetry_enabled(self) -> bool {
        self.telemetry_enabled
    }

    /// Returns whether cloud synchronization is enabled.
    #[must_use]
    pub const fn cloud_sync_enabled(self) -> bool {
        self.cloud_sync_enabled
    }

    /// Returns whether endpoints are discovered without user configuration.
    #[must_use]
    pub const fn automatic_provider_discovery_enabled(self) -> bool {
        self.automatic_provider_discovery_enabled
    }

    /// Returns whether full provider prompt/response content is logged.
    #[must_use]
    pub const fn prompt_response_logging_enabled(self) -> bool {
        self.prompt_response_logging_enabled
    }

    /// Returns whether remote requests can bypass exact approval.
    #[must_use]
    pub const fn remote_requests_without_approval_enabled(self) -> bool {
        self.remote_requests_without_approval_enabled
    }
}

/// Complete application-owned Settings V1 snapshot without persistence metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSettings {
    providers: [ProviderSettings; 3],
    endpoint: Option<ConfiguredModelEndpoint>,
    credential: ProviderCredentialMetadata,
    provider_health: Option<ProviderHealthObservation>,
    coding_profile: Option<LlmRoleProfile>,
    mapping_profile: Option<LlmRoleProfile>,
    embedding_profile: Option<VerifiedEmbeddingProfile>,
    privacy: DataPrivacySettings,
}

impl DesktopSettings {
    /// Returns the valid model-free offline-first initial state.
    #[must_use]
    pub const fn unconfigured() -> Self {
        Self {
            providers: [
                ProviderSettings::initial(ModelProviderKind::Ollama),
                ProviderSettings::initial(ModelProviderKind::Gemini),
                ProviderSettings::initial(ModelProviderKind::OpenAi),
            ],
            endpoint: None,
            credential: ProviderCredentialMetadata::not_required(),
            provider_health: None,
            coding_profile: None,
            mapping_profile: None,
            embedding_profile: None,
            privacy: DataPrivacySettings::offline_first_v1(),
        }
    }

    /// Returns the exactly three canonical provider slots in stable order.
    #[must_use]
    pub const fn providers(&self) -> &[ProviderSettings; 3] {
        &self.providers
    }

    /// Returns one canonical provider slot.
    #[must_use]
    pub fn provider(&self, kind: ModelProviderKind) -> &ProviderSettings {
        &self.providers[provider_index(kind)]
    }

    /// Replaces one provider origin and invalidates only that provider's derived evidence.
    #[must_use]
    pub fn with_provider_endpoint(
        mut self,
        kind: ModelProviderKind,
        endpoint: Option<ConfiguredModelEndpoint>,
    ) -> Self {
        let index = provider_index(kind);
        let changed = self.providers[index].endpoint != endpoint;
        if changed {
            let slot = &mut self.providers[index];
            slot.endpoint = endpoint;
            slot.configuration_revision = slot.configuration_revision.saturating_add(1);
            slot.enabled = false;
            slot.connection_verified_at = None;
            slot.health = slot.endpoint.as_ref().map_or_else(
                || ProviderHealthObservation::initial(ModelEndpointScope::LocalLoopback),
                ProviderHealthObservation::initial_for_endpoint,
            );
            slot.credential = slot.endpoint.as_ref().map_or_else(
                ProviderCredentialMetadata::not_required,
                |configured| match configured.credential_requirement() {
                    ProviderCredentialRequirement::None => {
                        ProviderCredentialMetadata::not_required()
                    }
                    ProviderCredentialRequirement::ApiKey => ProviderCredentialMetadata::missing(),
                },
            );
            self.remove_provider_profiles(kind);
        }
        self
    }

    /// Enables or disables one provider. Enabling requires a successful explicit connection.
    pub fn with_provider_enabled(
        mut self,
        kind: ModelProviderKind,
        enabled: bool,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let index = provider_index(kind);
        if enabled {
            let slot = &self.providers[index];
            if slot.endpoint.is_none() || slot.connection_verified_at.is_none() {
                return Err(DesktopSettingsUpdateError::ConnectionUnverified);
            }
            if slot.endpoint.as_ref().is_some_and(|endpoint| {
                endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
            }) && slot.credential.lifecycle() != ProviderCredentialLifecycle::Configured
            {
                return Err(DesktopSettingsUpdateError::CredentialUnavailable);
            }
        } else {
            self.remove_provider_profiles(kind);
        }
        self.providers[index].enabled = enabled;
        Ok(self)
    }

    /// Records an explicit successful connection/model-catalog test for one provider.
    pub fn with_provider_connection_verified(
        mut self,
        kind: ModelProviderKind,
        verified_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let slot = &mut self.providers[provider_index(kind)];
        let endpoint = slot
            .endpoint
            .as_ref()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
            && slot.credential.lifecycle() != ProviderCredentialLifecycle::Configured
        {
            return Err(DesktopSettingsUpdateError::CredentialUnavailable);
        }
        slot.connection_verified_at = Some(verified_at);
        slot.health =
            ProviderHealthObservation::checked(ProviderHealthStatus::Healthy, verified_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?;
        Ok(self)
    }

    /// Records a non-destructive failed retest for one provider.
    pub fn with_provider_probe_failure(
        mut self,
        kind: ModelProviderKind,
        status: ProviderHealthStatus,
        checked_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        if !matches!(
            status,
            ProviderHealthStatus::Unreachable | ProviderHealthStatus::Cancelled
        ) {
            return Err(DesktopSettingsUpdateError::InvalidHealth);
        }
        let slot = &mut self.providers[provider_index(kind)];
        if slot.endpoint.is_none() {
            return Err(DesktopSettingsUpdateError::EndpointUnavailable);
        }
        slot.health = ProviderHealthObservation::checked(status, checked_at)
            .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?;
        Ok(self)
    }

    /// Reconstructs one provider slot from validated persistence fields without network access.
    #[allow(clippy::too_many_arguments)]
    pub fn with_stored_provider_state(
        mut self,
        kind: ModelProviderKind,
        endpoint: Option<ConfiguredModelEndpoint>,
        enabled: bool,
        configuration_revision: u64,
        credential: ProviderCredentialMetadata,
        health: ProviderHealthObservation,
        connection_verified_at: Option<SettingsTimestamp>,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let index = provider_index(kind);
        if endpoint
            .as_ref()
            .is_some_and(|value| value.provider_id().as_str() != kind.provider_id())
        {
            return Err(DesktopSettingsUpdateError::ProviderMismatch);
        }
        if endpoint.is_none() && (enabled || connection_verified_at.is_some()) {
            return Err(DesktopSettingsUpdateError::EndpointUnavailable);
        }
        if enabled && connection_verified_at.is_none() {
            return Err(DesktopSettingsUpdateError::ConnectionUnverified);
        }
        self.providers[index] = ProviderSettings {
            kind,
            endpoint,
            enabled,
            configuration_revision,
            credential,
            health,
            connection_verified_at,
        };
        Ok(self)
    }

    fn remove_provider_profiles(&mut self, kind: ModelProviderKind) {
        let provider_id = kind.provider_id();
        self.coding_profile = self
            .coding_profile
            .take()
            .filter(|profile| profile.profile().provider_id().as_str() != provider_id);
        self.mapping_profile = self
            .mapping_profile
            .take()
            .filter(|profile| profile.profile().provider_id().as_str() != provider_id);
        self.embedding_profile = self
            .embedding_profile
            .take()
            .filter(|profile| profile.profile().provider_id().as_str() != provider_id);
    }

    /// Returns the configured provider origin, if any.
    #[must_use]
    pub const fn endpoint(&self) -> Option<&ConfiguredModelEndpoint> {
        self.endpoint.as_ref()
    }

    /// Returns content-free metadata for the current provider credential.
    #[must_use]
    pub const fn credential(&self) -> ProviderCredentialMetadata {
        self.credential
    }

    /// Returns content-free metadata for one provider slot.
    #[must_use]
    pub fn provider_credential(&self, kind: ModelProviderKind) -> ProviderCredentialMetadata {
        self.provider(kind).credential()
    }

    /// Starts a provider-specific credential write and invalidates only that provider's evidence.
    pub fn begin_provider_credential_store(
        mut self,
        kind: ModelProviderKind,
    ) -> Result<(Self, ProviderCredentialGeneration), DesktopSettingsUpdateError> {
        let index = provider_index(kind);
        let slot = &mut self.providers[index];
        let endpoint = slot
            .endpoint()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.credential_requirement() != ProviderCredentialRequirement::ApiKey {
            return Err(DesktopSettingsUpdateError::CredentialNotRequired);
        }
        let generation = slot
            .credential()
            .generation()
            .next()
            .map_err(|_| DesktopSettingsUpdateError::InvalidCredentialState)?;
        slot.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Storing,
            generation,
        )?;
        slot.configuration_revision = slot.configuration_revision.saturating_add(1);
        slot.connection_verified_at = None;
        slot.enabled = false;
        self.remove_provider_profiles(kind);
        Ok((self, generation))
    }

    /// Completes a provider-specific credential write.
    pub fn complete_provider_credential_store(
        mut self,
        kind: ModelProviderKind,
        generation: ProviderCredentialGeneration,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let slot = &mut self.providers[provider_index(kind)];
        if slot.credential().lifecycle() != ProviderCredentialLifecycle::Storing
            || slot.credential().generation() != generation
        {
            return Err(DesktopSettingsUpdateError::InvalidCredentialState);
        }
        slot.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Configured,
            generation,
        )?;
        Ok(self)
    }

    /// Starts deletion of one provider credential while retaining its endpoint.
    pub fn begin_provider_credential_delete(
        mut self,
        kind: ModelProviderKind,
    ) -> Result<(Self, ProviderCredentialGeneration), DesktopSettingsUpdateError> {
        let index = provider_index(kind);
        let slot = &mut self.providers[index];
        let endpoint = slot
            .endpoint()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.credential_requirement() != ProviderCredentialRequirement::ApiKey {
            return Err(DesktopSettingsUpdateError::CredentialNotRequired);
        }
        let generation = slot
            .credential()
            .generation()
            .next()
            .map_err(|_| DesktopSettingsUpdateError::InvalidCredentialState)?;
        slot.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Deleting,
            generation,
        )?;
        slot.configuration_revision = slot.configuration_revision.saturating_add(1);
        slot.connection_verified_at = None;
        slot.enabled = false;
        self.remove_provider_profiles(kind);
        Ok((self, generation))
    }

    /// Completes deletion of one provider credential.
    pub fn complete_provider_credential_delete(
        mut self,
        kind: ModelProviderKind,
        generation: ProviderCredentialGeneration,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let slot = &mut self.providers[provider_index(kind)];
        if slot.credential().lifecycle() != ProviderCredentialLifecycle::Deleting
            || slot.credential().generation() != generation
        {
            return Err(DesktopSettingsUpdateError::InvalidCredentialState);
        }
        slot.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Missing,
            generation,
        )?;
        Ok(self)
    }

    /// Returns the health state belonging to the current endpoint.
    #[must_use]
    pub const fn provider_health(&self) -> Option<ProviderHealthObservation> {
        self.provider_health
    }

    /// Returns one role candidate; only its activation state decides executability.
    #[must_use]
    pub const fn llm_profile(&self, role: LlmModelRole) -> Option<&LlmRoleProfile> {
        match role {
            LlmModelRole::Coding => self.coding_profile.as_ref(),
            LlmModelRole::Mapping => self.mapping_profile.as_ref(),
        }
    }

    /// Returns the live-verified embedding role profile.
    #[must_use]
    pub const fn embedding_profile(&self) -> Option<&VerifiedEmbeddingProfile> {
        self.embedding_profile.as_ref()
    }

    /// Returns the immutable V1 privacy boundary.
    #[must_use]
    pub const fn privacy(&self) -> DataPrivacySettings {
        self.privacy
    }

    /// Replaces or clears the endpoint and atomically invalidates all derived evidence.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Option<ConfiguredModelEndpoint>) -> Self {
        if self.endpoint != endpoint {
            self.endpoint = endpoint;
            self.credential = self.endpoint.as_ref().map_or_else(
                ProviderCredentialMetadata::not_required,
                |configured| match configured.credential_requirement() {
                    ProviderCredentialRequirement::None => {
                        ProviderCredentialMetadata::not_required()
                    }
                    ProviderCredentialRequirement::ApiKey => ProviderCredentialMetadata::missing(),
                },
            );
            self.provider_health = self
                .endpoint
                .as_ref()
                .map(ProviderHealthObservation::initial_for_endpoint);
            self.coding_profile = None;
            self.mapping_profile = None;
            self.embedding_profile = None;
            for kind in ModelProviderKind::all() {
                let matches = self
                    .endpoint
                    .as_ref()
                    .is_some_and(|value| value.provider_id().as_str() == kind.provider_id());
                if matches {
                    let index = provider_index(kind);
                    let slot = &mut self.providers[index];
                    slot.endpoint = self.endpoint.clone();
                    slot.configuration_revision = slot.configuration_revision.saturating_add(1);
                    slot.enabled = false;
                    slot.connection_verified_at = None;
                    slot.credential = self.credential;
                    slot.health = match (self.provider_health, self.endpoint.as_ref()) {
                        (Some(health), _) => health,
                        (None, Some(endpoint)) => {
                            ProviderHealthObservation::initial_for_endpoint(endpoint)
                        }
                        (None, None) => {
                            ProviderHealthObservation::initial(ModelEndpointScope::LocalLoopback)
                        }
                    };
                } else if self.endpoint.is_none() {
                    self.providers[provider_index(kind)] = ProviderSettings::initial(kind);
                }
            }
        }
        self
    }

    /// Begins a credential write and invalidates all provider-derived evidence.
    pub fn begin_credential_store(
        mut self,
    ) -> Result<(Self, ProviderCredentialGeneration), DesktopSettingsUpdateError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.credential_requirement() != ProviderCredentialRequirement::ApiKey {
            return Err(DesktopSettingsUpdateError::CredentialNotRequired);
        }
        let generation = self
            .credential
            .generation()
            .next()
            .map_err(|_| DesktopSettingsUpdateError::InvalidCredentialState)?;
        self.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Storing,
            generation,
        )?;
        if let Some(endpoint) = self.endpoint.as_ref()
            && let Some(kind) = ModelProviderKind::from_provider_id(endpoint.provider_id().as_str())
        {
            self.providers[provider_index(kind)].credential = self.credential;
        }
        self.invalidate_provider_evidence();
        Ok((self, generation))
    }

    /// Marks the exact externally stored generation usable.
    pub fn complete_credential_store(
        mut self,
        generation: ProviderCredentialGeneration,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        if self.credential.lifecycle() != ProviderCredentialLifecycle::Storing
            || self.credential.generation() != generation
        {
            return Err(DesktopSettingsUpdateError::InvalidCredentialState);
        }
        self.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Configured,
            generation,
        )?;
        if let Some(endpoint) = self.endpoint.as_ref()
            && let Some(kind) = ModelProviderKind::from_provider_id(endpoint.provider_id().as_str())
        {
            self.providers[provider_index(kind)].credential = self.credential;
        }
        Ok(self)
    }

    /// Begins deletion of the current provider credential and invalidates its evidence.
    pub fn begin_credential_delete(
        mut self,
    ) -> Result<(Self, ProviderCredentialGeneration), DesktopSettingsUpdateError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.credential_requirement() != ProviderCredentialRequirement::ApiKey {
            return Err(DesktopSettingsUpdateError::CredentialNotRequired);
        }
        let generation = self
            .credential
            .generation()
            .next()
            .map_err(|_| DesktopSettingsUpdateError::InvalidCredentialState)?;
        self.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Deleting,
            generation,
        )?;
        if let Some(endpoint) = self.endpoint.as_ref()
            && let Some(kind) = ModelProviderKind::from_provider_id(endpoint.provider_id().as_str())
        {
            self.providers[provider_index(kind)].credential = self.credential;
        }
        self.invalidate_provider_evidence();
        Ok((self, generation))
    }

    /// Completes deletion without removing the provider connection itself.
    pub fn complete_credential_delete(
        mut self,
        generation: ProviderCredentialGeneration,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        if self.credential.lifecycle() != ProviderCredentialLifecycle::Deleting
            || self.credential.generation() != generation
        {
            return Err(DesktopSettingsUpdateError::InvalidCredentialState);
        }
        self.credential = ProviderCredentialMetadata::from_stored_parts(
            ProviderCredentialLifecycle::Missing,
            generation,
        )?;
        if let Some(endpoint) = self.endpoint.as_ref()
            && let Some(kind) = ModelProviderKind::from_provider_id(endpoint.provider_id().as_str())
        {
            self.providers[provider_index(kind)].credential = self.credential;
        }
        Ok(self)
    }

    fn invalidate_provider_evidence(&mut self) {
        self.provider_health = self
            .endpoint
            .as_ref()
            .map(ProviderHealthObservation::initial_for_endpoint);
        self.coding_profile = None;
        self.mapping_profile = None;
        self.embedding_profile = None;
    }

    /// Records one profile only when it belongs to the current local provider endpoint.
    pub fn with_llm_probe(
        mut self,
        role: LlmModelRole,
        profile: ModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        self.require_matching_provider(profile.provider_id())?;
        let profile_kind = ModelProviderKind::from_provider_id(profile.provider_id().as_str());
        let role_profile = LlmRoleProfile::from_probe(profile, probed_at);
        let health = match role_profile.activation() {
            LlmProfileActivation::Executable => ProviderHealthStatus::Healthy,
            LlmProfileActivation::CapabilityLimited => ProviderHealthStatus::CapabilityLimited,
        };
        match role {
            LlmModelRole::Coding => self.coding_profile = Some(role_profile),
            LlmModelRole::Mapping => self.mapping_profile = Some(role_profile),
        }
        self.provider_health = Some(
            ProviderHealthObservation::checked(health, probed_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?,
        );
        if let Some(kind) = profile_kind {
            let slot = &mut self.providers[provider_index(kind)];
            slot.enabled = true;
            slot.connection_verified_at = Some(probed_at);
            if let Some(health) = self.provider_health {
                slot.health = health;
            }
        }
        Ok(self)
    }

    /// Records one role profile against a specific connected provider slot.
    pub fn with_provider_llm_probe(
        mut self,
        kind: ModelProviderKind,
        role: LlmModelRole,
        profile: ModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let slot = self.provider(kind);
        if !slot.enabled() || slot.connection_verified_at().is_none() {
            return Err(DesktopSettingsUpdateError::ConnectionUnverified);
        }
        if profile.provider_id().as_str() != kind.provider_id() {
            return Err(DesktopSettingsUpdateError::ProviderMismatch);
        }
        let role_profile = LlmRoleProfile::from_probe(profile, probed_at);
        let health = match role_profile.activation() {
            LlmProfileActivation::Executable => ProviderHealthStatus::Healthy,
            LlmProfileActivation::CapabilityLimited => ProviderHealthStatus::CapabilityLimited,
        };
        match role {
            LlmModelRole::Coding => self.coding_profile = Some(role_profile),
            LlmModelRole::Mapping => self.mapping_profile = Some(role_profile),
        }
        self.providers[provider_index(kind)].health =
            ProviderHealthObservation::checked(health, probed_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?;
        Ok(self)
    }

    /// Records one embedding profile only after the current local provider proved its dimension.
    pub fn with_embedding_probe(
        mut self,
        profile: EmbeddingModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let endpoint = self.active_endpoint()?;
        if endpoint.provider_id().as_str() != profile.provider_id().as_str() {
            return Err(DesktopSettingsUpdateError::ProviderMismatch);
        }
        self.embedding_profile = Some(VerifiedEmbeddingProfile::from_probe(profile, probed_at));
        self.provider_health = Some(
            ProviderHealthObservation::checked(ProviderHealthStatus::Healthy, probed_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?,
        );
        Ok(self)
    }

    /// Records one embedding profile against a specific connected provider slot.
    pub fn with_provider_embedding_probe(
        mut self,
        kind: ModelProviderKind,
        profile: EmbeddingModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let slot = self.provider(kind);
        if !slot.enabled() || slot.connection_verified_at().is_none() {
            return Err(DesktopSettingsUpdateError::ConnectionUnverified);
        }
        if profile.provider_id().as_str() != kind.provider_id() {
            return Err(DesktopSettingsUpdateError::ProviderMismatch);
        }
        self.embedding_profile = Some(VerifiedEmbeddingProfile::from_probe(profile, probed_at));
        self.providers[provider_index(kind)].health =
            ProviderHealthObservation::checked(ProviderHealthStatus::Healthy, probed_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?;
        Ok(self)
    }

    /// Records a redacted failure from one explicit local probe without changing role profiles.
    pub fn with_probe_failure(
        mut self,
        status: ProviderHealthStatus,
        checked_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        self.active_endpoint()?;
        if !matches!(
            status,
            ProviderHealthStatus::Unreachable | ProviderHealthStatus::Cancelled
        ) {
            return Err(DesktopSettingsUpdateError::InvalidHealth);
        }
        self.provider_health = Some(
            ProviderHealthObservation::checked(status, checked_at)
                .map_err(|_| DesktopSettingsUpdateError::InvalidHealth)?,
        );
        Ok(self)
    }

    fn require_matching_provider(
        &self,
        provider_id: &ModelProviderId,
    ) -> Result<(), DesktopSettingsUpdateError> {
        let endpoint = self.active_endpoint()?;
        if endpoint.provider_id() != provider_id {
            return Err(DesktopSettingsUpdateError::ProviderMismatch);
        }
        Ok(())
    }

    fn active_endpoint(&self) -> Result<&ConfiguredModelEndpoint, DesktopSettingsUpdateError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(DesktopSettingsUpdateError::EndpointUnavailable)?;
        if endpoint.access() == ModelEndpointAccess::RemoteBlocked {
            return Err(DesktopSettingsUpdateError::RemoteBlocked);
        }
        if endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
            && self.credential.lifecycle() != ProviderCredentialLifecycle::Configured
        {
            return Err(DesktopSettingsUpdateError::CredentialUnavailable);
        }
        Ok(endpoint)
    }

    /// Reconstructs one fully validated durable snapshot.
    pub fn from_stored_parts(
        endpoint: Option<ConfiguredModelEndpoint>,
        credential: ProviderCredentialMetadata,
        provider_health: Option<ProviderHealthObservation>,
        coding_profile: Option<LlmRoleProfile>,
        mapping_profile: Option<LlmRoleProfile>,
        embedding_profile: Option<VerifiedEmbeddingProfile>,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let mut settings = Self::unconfigured().with_endpoint(endpoint);
        match settings.endpoint.as_ref() {
            None => {
                if credential != ProviderCredentialMetadata::not_required()
                    || provider_health.is_some()
                    || coding_profile.is_some()
                    || mapping_profile.is_some()
                    || embedding_profile.is_some()
                {
                    return Err(DesktopSettingsUpdateError::EndpointUnavailable);
                }
            }
            Some(endpoint) => {
                let credential_is_valid = match endpoint.credential_requirement() {
                    ProviderCredentialRequirement::None => {
                        credential == ProviderCredentialMetadata::not_required()
                    }
                    ProviderCredentialRequirement::ApiKey => {
                        credential.lifecycle() != ProviderCredentialLifecycle::NotRequired
                    }
                };
                if !credential_is_valid {
                    return Err(DesktopSettingsUpdateError::InvalidCredentialState);
                }
                let health = provider_health.ok_or(DesktopSettingsUpdateError::InvalidHealth)?;
                if endpoint.access() == ModelEndpointAccess::RemoteBlocked {
                    if health != ProviderHealthObservation::initial(ModelEndpointScope::Remote)
                        || coding_profile.is_some()
                        || mapping_profile.is_some()
                        || embedding_profile.is_some()
                    {
                        return Err(DesktopSettingsUpdateError::RemoteBlocked);
                    }
                } else {
                    if endpoint.credential_requirement() == ProviderCredentialRequirement::ApiKey
                        && credential.lifecycle() != ProviderCredentialLifecycle::Configured
                        && (coding_profile.is_some()
                            || mapping_profile.is_some()
                            || embedding_profile.is_some())
                    {
                        return Err(DesktopSettingsUpdateError::CredentialUnavailable);
                    }
                    for profile in [&coding_profile, &mapping_profile].into_iter().flatten() {
                        if profile.profile().provider_id() != endpoint.provider_id()
                            || (profile.profile().capabilities().structured_output()
                                == ModelStructuredOutputCapability::Verified)
                                != (profile.activation() == LlmProfileActivation::Executable)
                        {
                            return Err(DesktopSettingsUpdateError::ProviderMismatch);
                        }
                    }
                    if embedding_profile.as_ref().is_some_and(|profile| {
                        profile.profile().provider_id().as_str() != endpoint.provider_id().as_str()
                    }) {
                        return Err(DesktopSettingsUpdateError::ProviderMismatch);
                    }
                }
                settings.provider_health = Some(health);
                settings.credential = credential;
                settings.coding_profile = coding_profile;
                settings.mapping_profile = mapping_profile;
                settings.embedding_profile = embedding_profile;
            }
        }
        Ok(settings)
    }
}

fn provider_index(kind: ModelProviderKind) -> usize {
    match kind {
        ModelProviderKind::Ollama => 0,
        ModelProviderKind::Gemini => 1,
        ModelProviderKind::OpenAi => 2,
    }
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self::unconfigured()
    }
}

/// A settings mutation contradicted endpoint, provider, capability, or health evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSettingsUpdateError {
    /// No endpoint exists for provider-derived evidence.
    EndpointUnavailable,
    /// A non-local endpoint cannot be probed without exact network approval.
    RemoteBlocked,
    /// The configured provider cannot use a credential in this build.
    CredentialNotRequired,
    /// A required credential is missing, in recovery, or inconsistent.
    CredentialUnavailable,
    /// Persisted credential lifecycle metadata contradicted its generation or endpoint.
    InvalidCredentialState,
    /// Profile provider identity did not match the configured endpoint.
    ProviderMismatch,
    /// Health state contradicted the endpoint or probe result.
    InvalidHealth,
    /// A provider cannot be enabled before its explicit connection test succeeds.
    ConnectionUnverified,
}

impl fmt::Display for DesktopSettingsUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EndpointUnavailable => "model endpoint is not configured",
            Self::RemoteBlocked => "remote model endpoint is blocked pending exact approval",
            Self::CredentialNotRequired => "model provider does not require a credential",
            Self::CredentialUnavailable => "model provider credential is not available",
            Self::InvalidCredentialState => "model provider credential state is inconsistent",
            Self::ProviderMismatch => "model profile belongs to another provider",
            Self::InvalidHealth => "provider health evidence is inconsistent",
            Self::ConnectionUnverified => "provider connection has not been explicitly verified",
        })
    }
}

impl Error for DesktopSettingsUpdateError {}

/// Monotone compare-and-swap revision of the global settings snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DesktopSettingsStoreVersion(u64);

impl DesktopSettingsStoreVersion {
    /// Empty stores use zero; persisted snapshots use positive revisions.
    pub const fn new(value: u64) -> Result<Self, DesktopSettingsStoreVersionError> {
        if value > MAX_SETTINGS_STORE_VERSION {
            Err(DesktopSettingsStoreVersionError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the initial empty-store revision.
    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }

    /// Returns the persistence integer.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Settings store revision exceeded the local SQL range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopSettingsStoreVersionError {
    value: u64,
}

impl fmt::Display for DesktopSettingsStoreVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "desktop settings store version {} is invalid",
            self.value
        )
    }
}

impl Error for DesktopSettingsStoreVersionError {}

/// One complete settings snapshot and its durable CAS revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDesktopSettings {
    version: DesktopSettingsStoreVersion,
    settings: DesktopSettings,
}

impl StoredDesktopSettings {
    /// Binds a fully validated snapshot to its store revision.
    #[must_use]
    pub const fn new(version: DesktopSettingsStoreVersion, settings: DesktopSettings) -> Self {
        Self { version, settings }
    }

    /// Returns the valid model-free empty-store view.
    #[must_use]
    pub const fn initial() -> Self {
        Self::new(
            DesktopSettingsStoreVersion::initial(),
            DesktopSettings::unconfigured(),
        )
    }

    /// Returns the CAS revision.
    #[must_use]
    pub const fn version(&self) -> DesktopSettingsStoreVersion {
        self.version
    }

    /// Returns the complete settings snapshot.
    #[must_use]
    pub const fn settings(&self) -> &DesktopSettings {
        &self.settings
    }
}

/// Future returned by the object-safe global settings persistence boundary.
pub type DesktopSettingsStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DesktopSettingsStoreFailure>> + Send + 'a>>;

/// Durable local boundary for complete global settings snapshots.
pub trait DesktopSettingsStore: fmt::Debug + Send + Sync {
    /// Loads the latest snapshot or the valid zero-revision initial state.
    fn load<'a>(&'a self) -> DesktopSettingsStoreFuture<'a, StoredDesktopSettings>;

    /// Appends one complete snapshot only when the current revision still matches.
    fn append<'a>(
        &'a self,
        expected: DesktopSettingsStoreVersion,
        settings: &'a DesktopSettings,
    ) -> DesktopSettingsStoreFuture<'a, StoredDesktopSettings>;
}

/// Stable failure classification for global settings storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSettingsStoreFailure {
    /// Local storage was unavailable.
    Unavailable,
    /// Database integrity checks failed.
    Corrupt,
    /// The catalog schema is newer than this application build.
    UnsupportedSchema,
    /// Durable fields contradicted settings invariants.
    InvalidStoredData,
    /// Another writer committed a settings revision first.
    VersionConflict,
    /// The snapshot exceeded a fixed storage bound.
    ResourceLimit,
}

impl fmt::Display for DesktopSettingsStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "desktop settings storage is unavailable",
            Self::Corrupt => "desktop settings storage is corrupt",
            Self::UnsupportedSchema => "desktop settings schema is unsupported",
            Self::InvalidStoredData => "desktop settings contain invalid data",
            Self::VersionConflict => "desktop settings changed concurrently",
            Self::ResourceLimit => "desktop settings exceed a fixed resource limit",
        })
    }
}

impl Error for DesktopSettingsStoreFailure {}

/// Reads the latest complete global settings snapshot.
#[derive(Debug, Clone)]
pub struct GetDesktopSettings {
    store: Arc<dyn DesktopSettingsStore>,
}

impl GetDesktopSettings {
    /// Binds the query to its narrow persistence capability.
    #[must_use]
    pub fn new(store: Arc<dyn DesktopSettingsStore>) -> Self {
        Self { store }
    }

    /// Loads the current settings snapshot without provider or network access.
    pub async fn execute(&self) -> Result<StoredDesktopSettings, DesktopSettingsStoreFailure> {
        self.store.load().await
    }
}

/// Validates and persists one endpoint configuration without performing network access.
#[derive(Debug, Clone)]
pub struct ConfigureDesktopModelEndpoint {
    store: Arc<dyn DesktopSettingsStore>,
    validator: Arc<dyn ModelEndpointValidator>,
}

impl ConfigureDesktopModelEndpoint {
    /// Binds persistence and concrete provider validation capabilities.
    #[must_use]
    pub fn new(
        store: Arc<dyn DesktopSettingsStore>,
        validator: Arc<dyn ModelEndpointValidator>,
    ) -> Self {
        Self { store, validator }
    }

    /// Replaces or clears the endpoint and invalidates all previous probe evidence atomically.
    pub async fn execute(
        &self,
        expected: DesktopSettingsStoreVersion,
        input: Option<&str>,
    ) -> Result<StoredDesktopSettings, ConfigureDesktopModelEndpointError> {
        let current = self
            .store
            .load()
            .await
            .map_err(ConfigureDesktopModelEndpointError::Store)?;
        if current.version() != expected {
            return Err(ConfigureDesktopModelEndpointError::Store(
                DesktopSettingsStoreFailure::VersionConflict,
            ));
        }
        let endpoint = input
            .map(|value| self.validator.validate(value))
            .transpose()
            .map_err(ConfigureDesktopModelEndpointError::Validation)?;
        let updated = current.settings().clone().with_endpoint(endpoint);
        self.store
            .append(expected, &updated)
            .await
            .map_err(ConfigureDesktopModelEndpointError::Store)
    }
}

/// Endpoint settings failed before a provider request could occur.
#[derive(Debug)]
pub enum ConfigureDesktopModelEndpointError {
    /// Concrete endpoint parsing rejected the value.
    Validation(ModelEndpointValidationFailure),
    /// The durable snapshot could not be read or appended.
    Store(DesktopSettingsStoreFailure),
}

impl fmt::Display for ConfigureDesktopModelEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl Error for ConfigureDesktopModelEndpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}

/// Coordinates durable metadata with the external OS credential boundary.
#[derive(Debug, Clone)]
pub struct SetDesktopProviderCredential {
    settings_store: Arc<dyn DesktopSettingsStore>,
    credential_store: Arc<dyn ProviderCredentialStore>,
}

impl SetDesktopProviderCredential {
    /// Binds both stores required by the fail-closed cross-store transition.
    #[must_use]
    pub fn new(
        settings_store: Arc<dyn DesktopSettingsStore>,
        credential_store: Arc<dyn ProviderCredentialStore>,
    ) -> Self {
        Self {
            settings_store,
            credential_store,
        }
    }

    /// Stores one new API key without contacting the provider.
    pub async fn execute(
        &self,
        expected: DesktopSettingsStoreVersion,
        secret: ProviderApiKey,
    ) -> Result<StoredDesktopSettings, ManageDesktopProviderCredentialError> {
        let current = self
            .settings_store
            .load()
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)?;
        if current.version() != expected {
            return Err(ManageDesktopProviderCredentialError::SettingsStore(
                DesktopSettingsStoreFailure::VersionConflict,
            ));
        }
        let provider_id = current
            .settings()
            .endpoint()
            .ok_or(ManageDesktopProviderCredentialError::InvalidState(
                DesktopSettingsUpdateError::EndpointUnavailable,
            ))?
            .provider_id()
            .clone();
        let (storing_settings, generation) = current
            .settings()
            .clone()
            .begin_credential_store()
            .map_err(ManageDesktopProviderCredentialError::InvalidState)?;
        let storing = self
            .settings_store
            .append(expected, &storing_settings)
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)?;
        let credential = ProviderCredential::new(generation, secret);
        self.credential_store
            .store(&provider_id, &credential)
            .await
            .map_err(ManageDesktopProviderCredentialError::CredentialStore)?;
        let configured = storing
            .settings()
            .clone()
            .complete_credential_store(generation)
            .map_err(ManageDesktopProviderCredentialError::InvalidState)?;
        self.settings_store
            .append(storing.version(), &configured)
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)
    }
}

/// Coordinates deletion of a provider credential before provider removal or replacement.
#[derive(Debug, Clone)]
pub struct DeleteDesktopProviderCredential {
    settings_store: Arc<dyn DesktopSettingsStore>,
    credential_store: Arc<dyn ProviderCredentialStore>,
}

impl DeleteDesktopProviderCredential {
    /// Binds both stores required by the fail-closed delete transition.
    #[must_use]
    pub fn new(
        settings_store: Arc<dyn DesktopSettingsStore>,
        credential_store: Arc<dyn ProviderCredentialStore>,
    ) -> Self {
        Self {
            settings_store,
            credential_store,
        }
    }

    /// Deletes the current provider credential without contacting the provider.
    pub async fn execute(
        &self,
        expected: DesktopSettingsStoreVersion,
    ) -> Result<StoredDesktopSettings, ManageDesktopProviderCredentialError> {
        let current = self
            .settings_store
            .load()
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)?;
        if current.version() != expected {
            return Err(ManageDesktopProviderCredentialError::SettingsStore(
                DesktopSettingsStoreFailure::VersionConflict,
            ));
        }
        let provider_id = current
            .settings()
            .endpoint()
            .ok_or(ManageDesktopProviderCredentialError::InvalidState(
                DesktopSettingsUpdateError::EndpointUnavailable,
            ))?
            .provider_id()
            .clone();
        let (deleting_settings, generation) = current
            .settings()
            .clone()
            .begin_credential_delete()
            .map_err(ManageDesktopProviderCredentialError::InvalidState)?;
        let deleting = self
            .settings_store
            .append(expected, &deleting_settings)
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)?;
        self.credential_store
            .delete(&provider_id)
            .await
            .map_err(ManageDesktopProviderCredentialError::CredentialStore)?;
        let missing = deleting
            .settings()
            .clone()
            .complete_credential_delete(generation)
            .map_err(ManageDesktopProviderCredentialError::InvalidState)?;
        self.settings_store
            .append(deleting.version(), &missing)
            .await
            .map_err(ManageDesktopProviderCredentialError::SettingsStore)
    }
}

/// Loads a provider credential only when durable metadata and native generation agree.
#[derive(Debug, Clone)]
pub struct LoadDesktopProviderCredential {
    credential_store: Arc<dyn ProviderCredentialStore>,
}

impl LoadDesktopProviderCredential {
    /// Binds the native credential capability.
    #[must_use]
    pub fn new(credential_store: Arc<dyn ProviderCredentialStore>) -> Self {
        Self { credential_store }
    }

    /// Returns `None` for credential-free providers and a key only for a consistent generation.
    pub async fn execute(
        &self,
        settings: &DesktopSettings,
    ) -> Result<Option<ProviderApiKey>, ProviderCredentialAccessError> {
        let endpoint = settings
            .endpoint()
            .ok_or(ProviderCredentialAccessError::Missing)?;
        let kind = ModelProviderKind::from_provider_id(endpoint.provider_id().as_str())
            .ok_or(ProviderCredentialAccessError::Missing)?;
        self.execute_for(settings, kind).await
    }

    /// Loads a credential for a specific provider slot after validating its generation.
    pub async fn execute_for(
        &self,
        settings: &DesktopSettings,
        kind: ModelProviderKind,
    ) -> Result<Option<ProviderApiKey>, ProviderCredentialAccessError> {
        let slot = settings.provider(kind);
        let endpoint = slot
            .endpoint()
            .ok_or(ProviderCredentialAccessError::Missing)?;
        if endpoint.credential_requirement() == ProviderCredentialRequirement::None {
            return Ok(None);
        }
        let metadata = slot.credential();
        match metadata.lifecycle() {
            ProviderCredentialLifecycle::Missing => {
                return Err(ProviderCredentialAccessError::Missing);
            }
            ProviderCredentialLifecycle::Storing | ProviderCredentialLifecycle::Deleting => {
                return Err(ProviderCredentialAccessError::RecoveryRequired);
            }
            ProviderCredentialLifecycle::NotRequired => {
                return Err(ProviderCredentialAccessError::RecoveryRequired);
            }
            ProviderCredentialLifecycle::Configured => {}
        }
        let fingerprint = endpoint.origin_fingerprint();
        let stored = self
            .credential_store
            .load_bound(endpoint.provider_id(), &fingerprint)
            .await
            .map_err(ProviderCredentialAccessError::Store)?
            .ok_or(ProviderCredentialAccessError::RecoveryRequired)?;
        if stored.generation() != metadata.generation() {
            return Err(ProviderCredentialAccessError::RecoveryRequired);
        }
        Ok(Some(stored.into_secret()))
    }
}

/// Cross-store credential mutation failed without exposing the credential.
#[derive(Debug)]
pub enum ManageDesktopProviderCredentialError {
    /// Durable Settings could not advance its append-only transition.
    SettingsStore(DesktopSettingsStoreFailure),
    /// The native credential backend rejected the operation.
    CredentialStore(ProviderCredentialStoreFailure),
    /// The current endpoint or lifecycle does not permit this transition.
    InvalidState(DesktopSettingsUpdateError),
}

impl fmt::Display for ManageDesktopProviderCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SettingsStore(error) => error.fmt(formatter),
            Self::CredentialStore(error) => error.fmt(formatter),
            Self::InvalidState(error) => error.fmt(formatter),
        }
    }
}

impl Error for ManageDesktopProviderCredentialError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SettingsStore(error) => Some(error),
            Self::CredentialStore(error) => Some(error),
            Self::InvalidState(error) => Some(error),
        }
    }
}

/// A configured provider credential could not be loaded safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialAccessError {
    /// The provider requires a credential but none was configured.
    Missing,
    /// Settings and the native backend require explicit repair.
    RecoveryRequired,
    /// The native credential backend is unavailable or contains invalid data.
    Store(ProviderCredentialStoreFailure),
}

impl fmt::Display for ProviderCredentialAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "provider credential is missing",
            Self::RecoveryRequired => "provider credential requires recovery",
            Self::Store(_) => "provider credential storage is unavailable",
        })
    }
}

impl Error for ProviderCredentialAccessError {}

/// Atomically persists Core-validated provider probe evidence.
#[derive(Debug, Clone)]
pub struct RecordDesktopModelProbe {
    store: Arc<dyn DesktopSettingsStore>,
}

impl RecordDesktopModelProbe {
    /// Binds probe persistence to the global settings store.
    #[must_use]
    pub fn new(store: Arc<dyn DesktopSettingsStore>) -> Self {
        Self { store }
    }

    /// Records one LLM role result against the exact current settings revision.
    pub async fn record_llm(
        &self,
        expected: DesktopSettingsStoreVersion,
        role: LlmModelRole,
        profile: ModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_llm_probe(role, profile, probed_at)
        })
        .await
    }

    /// Records one live-proven embedding result against the exact current revision.
    pub async fn record_embedding(
        &self,
        expected: DesktopSettingsStoreVersion,
        profile: EmbeddingModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_embedding_probe(profile, probed_at)
        })
        .await
    }

    /// Records one LLM probe against a provider slot.
    pub async fn record_provider_llm(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        role: LlmModelRole,
        profile: ModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_provider_llm_probe(kind, role, profile, probed_at)
        })
        .await
    }

    /// Records one embedding probe against a provider slot.
    pub async fn record_provider_embedding(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        profile: EmbeddingModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_provider_embedding_probe(kind, profile, probed_at)
        })
        .await
    }

    /// Records a non-destructive provider-specific retest failure.
    pub async fn record_provider_failure(
        &self,
        expected: DesktopSettingsStoreVersion,
        kind: ModelProviderKind,
        status: ProviderHealthStatus,
        checked_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_provider_probe_failure(kind, status, checked_at)
        })
        .await
    }

    /// Records a cancelled or unreachable explicit probe without invalidating older profiles.
    pub async fn record_failure(
        &self,
        expected: DesktopSettingsStoreVersion,
        status: ProviderHealthStatus,
        checked_at: SettingsTimestamp,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        self.update(expected, |settings| {
            settings.with_probe_failure(status, checked_at)
        })
        .await
    }

    async fn update(
        &self,
        expected: DesktopSettingsStoreVersion,
        mutate: impl FnOnce(DesktopSettings) -> Result<DesktopSettings, DesktopSettingsUpdateError>,
    ) -> Result<StoredDesktopSettings, RecordDesktopModelProbeError> {
        let current = self
            .store
            .load()
            .await
            .map_err(RecordDesktopModelProbeError::Store)?;
        if current.version() != expected {
            return Err(RecordDesktopModelProbeError::Store(
                DesktopSettingsStoreFailure::VersionConflict,
            ));
        }
        let updated = mutate(current.settings().clone())
            .map_err(RecordDesktopModelProbeError::InvalidEvidence)?;
        self.store
            .append(expected, &updated)
            .await
            .map_err(RecordDesktopModelProbeError::Store)
    }
}

/// Probe evidence could not be attached to the current durable settings snapshot.
#[derive(Debug)]
pub enum RecordDesktopModelProbeError {
    /// Evidence contradicted the current endpoint or provider.
    InvalidEvidence(DesktopSettingsUpdateError),
    /// The durable snapshot could not be read or appended.
    Store(DesktopSettingsStoreFailure),
}

impl fmt::Display for RecordDesktopModelProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEvidence(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl Error for RecordDesktopModelProbeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEvidence(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigureDesktopModelEndpoint, ConfiguredModelEndpoint, DeleteDesktopProviderCredential,
        DesktopSettings, DesktopSettingsStore, DesktopSettingsStoreFailure,
        DesktopSettingsStoreFuture, DesktopSettingsStoreVersion, LlmModelRole,
        LlmProfileActivation, LoadDesktopProviderCredential, ModelEndpointAccess,
        ModelEndpointScope, ModelEndpointValidationFailure, ModelEndpointValidator,
        ModelProviderKind, ProviderApiKey, ProviderCredential, ProviderCredentialAccessError,
        ProviderCredentialLifecycle, ProviderCredentialRequirement, ProviderCredentialStore,
        ProviderCredentialStoreFailure, ProviderHealthStatus, RecordDesktopModelProbe,
        SetDesktopProviderCredential, SettingsTimestamp, StoredDesktopSettings,
    };
    use crate::ProviderCredentialStoreFuture;
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use std::error::Error;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};

    #[derive(Debug)]
    struct MemoryStore {
        value: Mutex<StoredDesktopSettings>,
    }

    impl Default for MemoryStore {
        fn default() -> Self {
            Self {
                value: Mutex::new(StoredDesktopSettings::initial()),
            }
        }
    }

    impl MemoryStore {
        fn lock(&self) -> MutexGuard<'_, StoredDesktopSettings> {
            match self.value.lock() {
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
                let mut value = self.lock();
                if value.version() != expected {
                    return Err(DesktopSettingsStoreFailure::VersionConflict);
                }
                let next = DesktopSettingsStoreVersion::new(expected.get() + 1)
                    .map_err(|_| DesktopSettingsStoreFailure::ResourceLimit)?;
                *value = StoredDesktopSettings::new(next, settings.clone());
                Ok(value.clone())
            })
        }
    }

    #[derive(Debug, Default)]
    struct MemoryCredentialStore {
        value: Mutex<Option<(u64, Vec<u8>)>>,
        fail_store: AtomicBool,
        fail_delete: AtomicBool,
        store_calls: AtomicUsize,
    }

    impl MemoryCredentialStore {
        fn lock(&self) -> MutexGuard<'_, Option<(u64, Vec<u8>)>> {
            match self.value.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            }
        }
    }

    impl ProviderCredentialStore for MemoryCredentialStore {
        fn load<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
        ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
            Box::pin(async move {
                self.lock()
                    .as_ref()
                    .map(|(generation, bytes)| {
                        Ok(ProviderCredential::new(
                            super::ProviderCredentialGeneration::new(*generation)
                                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?,
                            ProviderApiKey::from_bytes(bytes.clone())
                                .map_err(|_| ProviderCredentialStoreFailure::Corrupt)?,
                        ))
                    })
                    .transpose()
            })
        }

        fn store<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
            credential: &'a ProviderCredential,
        ) -> ProviderCredentialStoreFuture<'a, ()> {
            Box::pin(async move {
                self.store_calls.fetch_add(1, Ordering::AcqRel);
                if self.fail_store.swap(false, Ordering::AcqRel) {
                    return Err(ProviderCredentialStoreFailure::Unavailable);
                }
                *self.lock() = Some((
                    credential.generation().get(),
                    credential.secret().as_bytes().to_vec(),
                ));
                Ok(())
            })
        }

        fn delete<'a>(
            &'a self,
            _provider_id: &'a ModelProviderId,
        ) -> ProviderCredentialStoreFuture<'a, ()> {
            Box::pin(async move {
                if self.fail_delete.swap(false, Ordering::AcqRel) {
                    return Err(ProviderCredentialStoreFailure::Unavailable);
                }
                *self.lock() = None;
                Ok(())
            })
        }
    }

    #[derive(Debug)]
    struct RemoteApiKeyValidator;

    impl ModelEndpointValidator for RemoteApiKeyValidator {
        fn validate(
            &self,
            input: &str,
        ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure> {
            if input != "https://generativelanguage.googleapis.com" {
                return Err(ModelEndpointValidationFailure::Invalid);
            }
            ConfiguredModelEndpoint::from_validated_adapter_with_security(
                ModelProviderId::try_from_string("gemini".to_owned())
                    .map_err(|_| ModelEndpointValidationFailure::Invalid)?,
                input.to_owned(),
                ModelEndpointScope::Remote,
                ModelEndpointAccess::ExplicitUserInitiatedRemote,
                ProviderCredentialRequirement::ApiKey,
            )
            .map_err(|_| ModelEndpointValidationFailure::Invalid)
        }
    }

    #[derive(Debug)]
    struct LocalValidator;

    impl ModelEndpointValidator for LocalValidator {
        fn validate(
            &self,
            input: &str,
        ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure> {
            if input != "http://localhost:11434" {
                return Err(ModelEndpointValidationFailure::Invalid);
            }
            ConfiguredModelEndpoint::from_validated_adapter(
                ModelProviderId::try_from_string("ollama".to_owned())
                    .map_err(|_| ModelEndpointValidationFailure::Invalid)?,
                "http://127.0.0.1:11434".to_owned(),
                ModelEndpointScope::LocalLoopback,
            )
            .map_err(|_| ModelEndpointValidationFailure::Invalid)
        }
    }

    #[test]
    fn unconfigured_settings_are_a_valid_offline_index_browser() {
        let settings = StoredDesktopSettings::initial();
        assert_eq!(settings.version().get(), 0);
        assert!(settings.settings().endpoint().is_none());
        assert!(
            settings
                .settings()
                .llm_profile(LlmModelRole::Coding)
                .is_none()
        );
        assert!(!settings.settings().privacy().telemetry_enabled());
        assert!(!settings.settings().privacy().cloud_sync_enabled());
        assert!(
            !settings
                .settings()
                .privacy()
                .remote_requests_without_approval_enabled()
        );
    }

    #[test]
    fn initial_settings_expose_three_disabled_provider_slots() {
        let settings = DesktopSettings::unconfigured();
        assert_eq!(settings.providers().len(), 3);
        assert_eq!(
            settings
                .providers()
                .iter()
                .map(|provider| provider.kind())
                .collect::<Vec<_>>(),
            vec![
                ModelProviderKind::Ollama,
                ModelProviderKind::Gemini,
                ModelProviderKind::OpenAi,
            ]
        );
        assert!(settings.providers().iter().all(|provider| {
            !provider.enabled()
                && provider.endpoint().is_none()
                && provider.connection_verified_at().is_none()
        }));
    }

    #[test]
    fn provider_activation_requires_verification_and_retest_keeps_profiles()
    -> Result<(), Box<dyn Error>> {
        let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            provider_id()?,
            "http://127.0.0.1:11434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?;
        let settings = DesktopSettings::unconfigured()
            .with_provider_endpoint(ModelProviderKind::Ollama, Some(endpoint));
        assert!(matches!(
            settings
                .clone()
                .with_provider_enabled(ModelProviderKind::Ollama, true),
            Err(super::DesktopSettingsUpdateError::ConnectionUnverified)
        ));

        let verified = settings
            .with_provider_connection_verified(
                ModelProviderKind::Ollama,
                SettingsTimestamp::from_unix_millis(50)?,
            )?
            .with_provider_enabled(ModelProviderKind::Ollama, true)?
            .with_provider_llm_probe(
                ModelProviderKind::Ollama,
                LlmModelRole::Coding,
                profile(ModelStructuredOutputCapability::Verified)?,
                SettingsTimestamp::from_unix_millis(51)?,
            )?;
        assert!(verified.llm_profile(LlmModelRole::Coding).is_some());

        let retested = verified.with_provider_probe_failure(
            ModelProviderKind::Ollama,
            ProviderHealthStatus::Unreachable,
            SettingsTimestamp::from_unix_millis(52)?,
        )?;
        assert!(retested.provider(ModelProviderKind::Ollama).enabled());
        assert!(
            retested
                .provider(ModelProviderKind::Ollama)
                .connection_verified_at()
                .is_some()
        );
        assert!(retested.llm_profile(LlmModelRole::Coding).is_some());
        assert_eq!(
            retested
                .provider(ModelProviderKind::Ollama)
                .health()
                .status(),
            ProviderHealthStatus::Unreachable
        );
        Ok(())
    }

    #[test]
    fn provider_credential_changes_advance_only_the_provider_revision() -> Result<(), Box<dyn Error>>
    {
        let endpoint =
            RemoteApiKeyValidator.validate("https://generativelanguage.googleapis.com")?;
        let settings = DesktopSettings::unconfigured()
            .with_provider_endpoint(ModelProviderKind::Gemini, Some(endpoint));
        let before = settings
            .provider(ModelProviderKind::Gemini)
            .configuration_revision();
        let (storing, _) = settings.begin_provider_credential_store(ModelProviderKind::Gemini)?;
        assert_eq!(
            storing
                .provider(ModelProviderKind::Gemini)
                .configuration_revision(),
            before + 1
        );
        assert_eq!(
            storing
                .provider(ModelProviderKind::Ollama)
                .configuration_revision(),
            0
        );
        assert!(
            storing
                .provider(ModelProviderKind::Gemini)
                .connection_verified_at()
                .is_none()
        );
        assert!(!storing.provider(ModelProviderKind::Gemini).enabled());
        Ok(())
    }

    #[test]
    fn changing_one_provider_invalidates_only_that_provider() -> Result<(), Box<dyn Error>> {
        let ollama_endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            provider_id()?,
            "http://127.0.0.1:11434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?;
        let gemini_endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            ModelProviderId::try_from_string("gemini".to_owned())?,
            "https://generativelanguage.googleapis.com".to_owned(),
            ModelEndpointScope::Remote,
        )?;
        let settings = DesktopSettings::unconfigured()
            .with_provider_endpoint(ModelProviderKind::Ollama, Some(ollama_endpoint))
            .with_provider_endpoint(ModelProviderKind::Gemini, Some(gemini_endpoint));
        let settings = settings
            .with_provider_connection_verified(
                ModelProviderKind::Ollama,
                SettingsTimestamp::from_unix_millis(60)?,
            )?
            .with_provider_enabled(ModelProviderKind::Ollama, true)?
            .with_provider_llm_probe(
                ModelProviderKind::Ollama,
                LlmModelRole::Coding,
                profile(ModelStructuredOutputCapability::Verified)?,
                SettingsTimestamp::from_unix_millis(61)?,
            )?;
        let changed_gemini = settings.with_provider_endpoint(ModelProviderKind::Gemini, None);
        assert!(changed_gemini.llm_profile(LlmModelRole::Coding).is_some());
        assert!(changed_gemini.provider(ModelProviderKind::Ollama).enabled());
        assert!(
            changed_gemini
                .provider(ModelProviderKind::Gemini)
                .endpoint()
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn endpoint_change_invalidates_all_profile_evidence() -> Result<(), Box<dyn Error>> {
        let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
            provider_id()?,
            "http://127.0.0.1:11434".to_owned(),
            ModelEndpointScope::LocalLoopback,
        )?;
        let settings = DesktopSettings::unconfigured()
            .with_endpoint(Some(endpoint))
            .with_llm_probe(
                LlmModelRole::Coding,
                profile(ModelStructuredOutputCapability::Verified)?,
                SettingsTimestamp::from_unix_millis(10)?,
            )?;
        assert!(settings.llm_profile(LlmModelRole::Coding).is_some());

        let changed =
            settings.with_endpoint(Some(ConfiguredModelEndpoint::from_validated_adapter(
                provider_id()?,
                "http://127.0.0.1:22434".to_owned(),
                ModelEndpointScope::LocalLoopback,
            )?));
        assert!(changed.llm_profile(LlmModelRole::Coding).is_none());
        assert_eq!(
            changed.provider_health().map(|health| health.status()),
            Some(ProviderHealthStatus::NotChecked)
        );
        Ok(())
    }

    #[test]
    fn capability_limited_profile_is_visible_but_never_executable() -> Result<(), Box<dyn Error>> {
        let settings = local_settings()?.with_llm_probe(
            LlmModelRole::Mapping,
            profile(ModelStructuredOutputCapability::Unavailable)?,
            SettingsTimestamp::from_unix_millis(20)?,
        )?;
        let role = settings
            .llm_profile(LlmModelRole::Mapping)
            .ok_or("missing mapping profile")?;
        assert_eq!(role.activation(), LlmProfileActivation::CapabilityLimited);
        assert!(!role.profile().executable_actions_enabled());
        assert_eq!(
            settings.provider_health().map(|health| health.status()),
            Some(ProviderHealthStatus::CapabilityLimited)
        );
        Ok(())
    }

    #[test]
    fn remote_endpoint_is_stored_as_blocked_and_rejects_probe_evidence()
    -> Result<(), Box<dyn Error>> {
        let remote = ConfiguredModelEndpoint::from_validated_adapter(
            provider_id()?,
            "https://models.example.invalid".to_owned(),
            ModelEndpointScope::Remote,
        )?;
        let settings = DesktopSettings::unconfigured().with_endpoint(Some(remote));
        assert_eq!(
            settings.provider_health().map(|health| health.status()),
            Some(ProviderHealthStatus::RemoteBlocked)
        );
        assert!(
            settings
                .with_llm_probe(
                    LlmModelRole::Coding,
                    profile(ModelStructuredOutputCapability::Verified)?,
                    SettingsTimestamp::from_unix_millis(30)?,
                )
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn configure_and_probe_use_cases_enforce_revision_cas() -> Result<(), Box<dyn Error>> {
        futures::executor::block_on(async {
            let store = Arc::new(MemoryStore::default());
            let store_port: Arc<dyn DesktopSettingsStore> = store.clone();
            let configure = ConfigureDesktopModelEndpoint::new(
                Arc::clone(&store_port),
                Arc::new(LocalValidator),
            );
            let configured = configure
                .execute(
                    DesktopSettingsStoreVersion::initial(),
                    Some("http://localhost:11434"),
                )
                .await?;
            assert_eq!(configured.version().get(), 1);

            let recorder = RecordDesktopModelProbe::new(store_port);
            let recorded = recorder
                .record_llm(
                    configured.version(),
                    LlmModelRole::Coding,
                    profile(ModelStructuredOutputCapability::Verified)?,
                    SettingsTimestamp::from_unix_millis(40)?,
                )
                .await?;
            assert_eq!(recorded.version().get(), 2);
            assert!(
                recorder
                    .record_failure(
                        configured.version(),
                        ProviderHealthStatus::Cancelled,
                        SettingsTimestamp::from_unix_millis(41)?,
                    )
                    .await
                    .is_err()
            );
            Ok::<(), Box<dyn Error>>(())
        })
    }

    #[test]
    fn credential_set_load_delete_is_generation_bound_and_revision_safe()
    -> Result<(), Box<dyn Error>> {
        futures::executor::block_on(async {
            let settings = Arc::new(MemoryStore::default());
            let settings_port: Arc<dyn DesktopSettingsStore> = settings.clone();
            let credentials = Arc::new(MemoryCredentialStore::default());
            let credential_port: Arc<dyn ProviderCredentialStore> = credentials.clone();
            let configured = ConfigureDesktopModelEndpoint::new(
                Arc::clone(&settings_port),
                Arc::new(RemoteApiKeyValidator),
            )
            .execute(
                DesktopSettingsStoreVersion::initial(),
                Some("https://generativelanguage.googleapis.com"),
            )
            .await?;

            let set = SetDesktopProviderCredential::new(
                Arc::clone(&settings_port),
                Arc::clone(&credential_port),
            );
            let stored = set
                .execute(
                    configured.version(),
                    ProviderApiKey::from_bytes(b"first-key".to_vec())?,
                )
                .await?;
            assert_eq!(stored.version().get(), 3);
            assert_eq!(
                stored.settings().credential().lifecycle(),
                ProviderCredentialLifecycle::Configured
            );
            assert_eq!(stored.settings().credential().generation().get(), 1);
            let loaded = LoadDesktopProviderCredential::new(Arc::clone(&credential_port))
                .execute(stored.settings())
                .await?
                .ok_or("configured credential was absent")?;
            assert_eq!(loaded.as_bytes(), b"first-key");

            let stale = set
                .execute(
                    configured.version(),
                    ProviderApiKey::from_bytes(b"must-not-store".to_vec())?,
                )
                .await;
            assert!(stale.is_err());
            assert_eq!(credentials.store_calls.load(Ordering::Acquire), 1);

            let deleted = DeleteDesktopProviderCredential::new(
                Arc::clone(&settings_port),
                Arc::clone(&credential_port),
            )
            .execute(stored.version())
            .await?;
            assert_eq!(deleted.version().get(), 5);
            assert_eq!(
                deleted.settings().credential().lifecycle(),
                ProviderCredentialLifecycle::Missing
            );
            assert!(matches!(
                LoadDesktopProviderCredential::new(credential_port)
                    .execute(deleted.settings())
                    .await,
                Err(ProviderCredentialAccessError::Missing)
            ));
            Ok::<(), Box<dyn Error>>(())
        })
    }

    #[test]
    fn interrupted_credential_phases_fail_closed_and_are_retryable() -> Result<(), Box<dyn Error>> {
        futures::executor::block_on(async {
            let settings = Arc::new(MemoryStore::default());
            let settings_port: Arc<dyn DesktopSettingsStore> = settings.clone();
            let credentials = Arc::new(MemoryCredentialStore::default());
            let credential_port: Arc<dyn ProviderCredentialStore> = credentials.clone();
            let configured = ConfigureDesktopModelEndpoint::new(
                Arc::clone(&settings_port),
                Arc::new(RemoteApiKeyValidator),
            )
            .execute(
                DesktopSettingsStoreVersion::initial(),
                Some("https://generativelanguage.googleapis.com"),
            )
            .await?;

            credentials.fail_store.store(true, Ordering::Release);
            let set = SetDesktopProviderCredential::new(
                Arc::clone(&settings_port),
                Arc::clone(&credential_port),
            );
            let error = match set
                .execute(
                    configured.version(),
                    ProviderApiKey::from_bytes(b"never-log-this".to_vec())?,
                )
                .await
            {
                Err(error) => error,
                Ok(_) => return Err("injected native-store failure was accepted".into()),
            };
            assert!(!format!("{error:?}").contains("never-log-this"));
            let interrupted = settings_port.load().await?;
            assert_eq!(interrupted.version().get(), 2);
            assert_eq!(
                interrupted.settings().credential().lifecycle(),
                ProviderCredentialLifecycle::Storing
            );
            assert!(matches!(
                LoadDesktopProviderCredential::new(Arc::clone(&credential_port))
                    .execute(interrupted.settings())
                    .await,
                Err(ProviderCredentialAccessError::RecoveryRequired)
            ));

            let recovered = set
                .execute(
                    interrupted.version(),
                    ProviderApiKey::from_bytes(b"replacement-key".to_vec())?,
                )
                .await?;
            assert_eq!(recovered.settings().credential().generation().get(), 2);
            credentials.fail_delete.store(true, Ordering::Release);
            let deletion = DeleteDesktopProviderCredential::new(
                Arc::clone(&settings_port),
                Arc::clone(&credential_port),
            );
            assert!(deletion.execute(recovered.version()).await.is_err());
            let deleting = settings_port.load().await?;
            assert_eq!(
                deleting.settings().credential().lifecycle(),
                ProviderCredentialLifecycle::Deleting
            );
            assert!(matches!(
                LoadDesktopProviderCredential::new(Arc::clone(&credential_port))
                    .execute(deleting.settings())
                    .await,
                Err(ProviderCredentialAccessError::RecoveryRequired)
            ));
            let repaired = deletion.execute(deleting.version()).await?;
            assert_eq!(
                repaired.settings().credential().lifecycle(),
                ProviderCredentialLifecycle::Missing
            );
            Ok::<(), Box<dyn Error>>(())
        })
    }

    #[test]
    fn generation_mismatch_never_releases_the_native_key() -> Result<(), Box<dyn Error>> {
        futures::executor::block_on(async {
            let endpoint =
                RemoteApiKeyValidator.validate("https://generativelanguage.googleapis.com")?;
            let (storing, generation) = DesktopSettings::unconfigured()
                .with_endpoint(Some(endpoint))
                .begin_credential_store()?;
            let configured = storing.complete_credential_store(generation)?;
            let credentials = Arc::new(MemoryCredentialStore::default());
            *credentials.lock() = Some((generation.next()?.get(), b"wrong-generation".to_vec()));
            let credential_port: Arc<dyn ProviderCredentialStore> = credentials;
            assert!(matches!(
                LoadDesktopProviderCredential::new(credential_port)
                    .execute(&configured)
                    .await,
                Err(ProviderCredentialAccessError::RecoveryRequired)
            ));
            Ok::<(), Box<dyn Error>>(())
        })
    }

    fn local_settings() -> Result<DesktopSettings, Box<dyn Error>> {
        Ok(DesktopSettings::unconfigured().with_endpoint(Some(
            ConfiguredModelEndpoint::from_validated_adapter(
                provider_id()?,
                "http://127.0.0.1:11434".to_owned(),
                ModelEndpointScope::LocalLoopback,
            )?,
        )))
    }

    fn provider_id() -> Result<ModelProviderId, Box<dyn Error>> {
        Ok(ModelProviderId::try_from_string("ollama".to_owned())?)
    }

    fn profile(
        capability: ModelStructuredOutputCapability,
    ) -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            provider_id()?,
            ModelId::try_from_string("test-model".to_owned())?,
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
