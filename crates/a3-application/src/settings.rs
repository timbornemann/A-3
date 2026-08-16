use a3_domain::{
    EmbeddingModelProfile, ModelProfile, ModelProviderId, ModelStructuredOutputCapability,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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

/// Credential-free canonical origin produced by a concrete provider adapter.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfiguredModelEndpoint {
    provider_id: ModelProviderId,
    canonical_origin: String,
    scope: ModelEndpointScope,
}

impl ConfiguredModelEndpoint {
    /// Revalidates the provider-neutral safety envelope around an adapter-canonicalized origin.
    pub fn from_validated_adapter(
        provider_id: ModelProviderId,
        canonical_origin: String,
        scope: ModelEndpointScope,
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
        Ok(Self {
            provider_id,
            canonical_origin,
            scope,
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

    /// Returns whether requests remain on literal host loopback.
    #[must_use]
    pub const fn scope(&self) -> ModelEndpointScope {
        self.scope
    }
}

impl fmt::Debug for ConfiguredModelEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfiguredModelEndpoint")
            .field("provider_id", &self.provider_id)
            .field("scope", &self.scope)
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
}

impl fmt::Display for ConfiguredModelEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "model endpoint origin length is invalid",
            Self::UnsafeCharacter => "model endpoint origin contains an unsafe character",
            Self::InvalidOrigin => "model endpoint is not a credential-free canonical origin",
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
            status: if endpoint.scope() == ModelEndpointScope::LocalLoopback
                || endpoint.provider_id().as_str() == "gemini"
            {
                ProviderHealthStatus::NotChecked
            } else {
                ProviderHealthStatus::RemoteBlocked
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
    endpoint: Option<ConfiguredModelEndpoint>,
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
            endpoint: None,
            provider_health: None,
            coding_profile: None,
            mapping_profile: None,
            embedding_profile: None,
            privacy: DataPrivacySettings::offline_first_v1(),
        }
    }

    /// Returns the configured provider origin, if any.
    #[must_use]
    pub const fn endpoint(&self) -> Option<&ConfiguredModelEndpoint> {
        self.endpoint.as_ref()
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
            self.provider_health = self
                .endpoint
                .as_ref()
                .map(ProviderHealthObservation::initial_for_endpoint);
            self.coding_profile = None;
            self.mapping_profile = None;
            self.embedding_profile = None;
        }
        self
    }

    /// Records one profile only when it belongs to the current local provider endpoint.
    pub fn with_llm_probe(
        mut self,
        role: LlmModelRole,
        profile: ModelProfile,
        probed_at: SettingsTimestamp,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        self.require_matching_provider(profile.provider_id())?;
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
        if endpoint.scope() != ModelEndpointScope::LocalLoopback
            && endpoint.provider_id().as_str() != "gemini"
        {
            return Err(DesktopSettingsUpdateError::RemoteBlocked);
        }
        Ok(endpoint)
    }

    /// Reconstructs one fully validated durable snapshot.
    pub fn from_stored_parts(
        endpoint: Option<ConfiguredModelEndpoint>,
        provider_health: Option<ProviderHealthObservation>,
        coding_profile: Option<LlmRoleProfile>,
        mapping_profile: Option<LlmRoleProfile>,
        embedding_profile: Option<VerifiedEmbeddingProfile>,
    ) -> Result<Self, DesktopSettingsUpdateError> {
        let mut settings = Self::unconfigured().with_endpoint(endpoint);
        match settings.endpoint.as_ref() {
            None => {
                if provider_health.is_some()
                    || coding_profile.is_some()
                    || mapping_profile.is_some()
                    || embedding_profile.is_some()
                {
                    return Err(DesktopSettingsUpdateError::EndpointUnavailable);
                }
            }
            Some(endpoint) => {
                let health = provider_health.ok_or(DesktopSettingsUpdateError::InvalidHealth)?;
                let is_authorized_remote = endpoint.scope() == ModelEndpointScope::Remote
                    && endpoint.provider_id().as_str() == "gemini";
                if endpoint.scope() == ModelEndpointScope::Remote && !is_authorized_remote {
                    if health != ProviderHealthObservation::initial(ModelEndpointScope::Remote)
                        || coding_profile.is_some()
                        || mapping_profile.is_some()
                        || embedding_profile.is_some()
                    {
                        return Err(DesktopSettingsUpdateError::RemoteBlocked);
                    }
                } else {
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
                settings.coding_profile = coding_profile;
                settings.mapping_profile = mapping_profile;
                settings.embedding_profile = embedding_profile;
            }
        }
        Ok(settings)
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
    /// Profile provider identity did not match the configured endpoint.
    ProviderMismatch,
    /// Health state contradicted the endpoint or probe result.
    InvalidHealth,
}

impl fmt::Display for DesktopSettingsUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EndpointUnavailable => "model endpoint is not configured",
            Self::RemoteBlocked => "remote model endpoint is blocked pending exact approval",
            Self::ProviderMismatch => "model profile belongs to another provider",
            Self::InvalidHealth => "provider health evidence is inconsistent",
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
        ConfigureDesktopModelEndpoint, ConfiguredModelEndpoint, DesktopSettings,
        DesktopSettingsStore, DesktopSettingsStoreFailure, DesktopSettingsStoreFuture,
        DesktopSettingsStoreVersion, LlmModelRole, LlmProfileActivation, ModelEndpointScope,
        ModelEndpointValidationFailure, ModelEndpointValidator, ProviderHealthStatus,
        RecordDesktopModelProbe, SettingsTimestamp, StoredDesktopSettings,
    };
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use std::error::Error;
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
