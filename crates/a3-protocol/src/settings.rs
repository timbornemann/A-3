use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Read-only request for the current global settings snapshot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuerySettingsRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QuerySettingsRequestV1 {
    /// Returns the version checked before reading local settings.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Closed provider implementation available to the desktop Settings boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelProviderKindV1 {
    /// Local Ollama-compatible API using its native provider contracts.
    Ollama,
    /// Google Gemini API using its native REST/SSE contracts.
    Gemini,
}

/// Optimistic active-provider replacement; omission explicitly returns to model-free mode.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConfigureModelProviderRequestV1 {
    protocol_version: ProtocolVersion,
    expected_settings_revision: String,
    provider_kind: ModelProviderKindV1,
    endpoint_origin: Option<String>,
}

impl ConfigureModelProviderRequestV1 {
    /// Returns the request schema version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the decimal CAS revision visible to the user.
    #[must_use]
    pub fn expected_settings_revision(&self) -> &str {
        &self.expected_settings_revision
    }

    /// Returns the closed concrete provider adapter selected by the user.
    #[must_use]
    pub const fn provider_kind(&self) -> ModelProviderKindV1 {
        self.provider_kind
    }

    /// Returns the user-entered origin or `None` to clear provider configuration.
    #[must_use]
    pub fn endpoint_origin(&self) -> Option<&str> {
        self.endpoint_origin.as_deref()
    }
}

impl fmt::Debug for ConfigureModelProviderRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigureModelProviderRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field(
                "expected_settings_revision",
                &self.expected_settings_revision,
            )
            .field("provider_kind", &self.provider_kind)
            .field("has_endpoint", &self.endpoint_origin.is_some())
            .finish()
    }
}

/// Explicit model-catalog read bound only to the current Core-owned Settings revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DiscoverProviderModelsRequestV1 {
    protocol_version: ProtocolVersion,
    expected_settings_revision: String,
}

impl DiscoverProviderModelsRequestV1 {
    /// Returns the request schema version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the exact Settings revision whose Core-owned provider must be queried.
    #[must_use]
    pub fn expected_settings_revision(&self) -> &str {
        &self.expected_settings_revision
    }
}

/// Closed model role that can be explicitly probed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelRoleV1 {
    /// Coding-agent LLM profile.
    Coding,
    /// Deep-Map LLM profile.
    Mapping,
    /// Semantic embedding profile.
    Embedding,
}

/// User-selected resource bounds for an LLM capability probe.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LlmProbeLimitsV1 {
    context_tokens: u32,
    output_tokens: u32,
    parallelism: u16,
}

impl LlmProbeLimitsV1 {
    /// Returns the requested effective context window.
    #[must_use]
    pub const fn context_tokens(self) -> u32 {
        self.context_tokens
    }

    /// Returns the requested output reservation.
    #[must_use]
    pub const fn output_tokens(self) -> u32 {
        self.output_tokens
    }

    /// Returns the local concurrency bound.
    #[must_use]
    pub const fn parallelism(self) -> u16 {
        self.parallelism
    }
}

/// User-selected operational limit for an embedding capability probe.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EmbeddingProbeLimitsV1 {
    max_batch_size: u16,
}

impl EmbeddingProbeLimitsV1 {
    /// Returns the local provider batch bound; vector dimension remains provider-derived.
    #[must_use]
    pub const fn max_batch_size(self) -> u16 {
        self.max_batch_size
    }
}

/// Explicit capability probe without endpoint, provider, capability, profile identity, or time.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProbeModelRoleRequestV1 {
    protocol_version: ProtocolVersion,
    expected_settings_revision: String,
    role: ModelRoleV1,
    model_id: String,
    llm_limits: Option<LlmProbeLimitsV1>,
    embedding_limits: Option<EmbeddingProbeLimitsV1>,
}

impl ProbeModelRoleRequestV1 {
    /// Returns the request schema version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the exact settings revision whose Core-owned endpoint must be used.
    #[must_use]
    pub fn expected_settings_revision(&self) -> &str {
        &self.expected_settings_revision
    }

    /// Returns the selected closed role.
    #[must_use]
    pub const fn role(&self) -> ModelRoleV1 {
        self.role
    }

    /// Returns the opaque provider-native model identity.
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Returns LLM limits supplied only for Coding or Mapping.
    #[must_use]
    pub const fn llm_limits(&self) -> Option<LlmProbeLimitsV1> {
        self.llm_limits
    }

    /// Returns embedding limits supplied only for Embedding.
    #[must_use]
    pub const fn embedding_limits(&self) -> Option<EmbeddingProbeLimitsV1> {
        self.embedding_limits
    }
}

impl fmt::Debug for ProbeModelRoleRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProbeModelRoleRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field(
                "expected_settings_revision",
                &self.expected_settings_revision,
            )
            .field("role", &self.role)
            .field("has_model_id", &!self.model_id.is_empty())
            .field("llm_limits", &self.llm_limits)
            .field("embedding_limits", &self.embedding_limits)
            .finish()
    }
}

/// Explicit cancellation request for the single Core-owned model probe.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CancelModelProbeRequestV1 {
    protocol_version: ProtocolVersion,
}

impl CancelModelProbeRequestV1 {
    /// Returns the schema version checked before cancellation.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Locality classification of the configured credential-free origin.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelEndpointScopeV1 {
    /// Literal loopback origin.
    LocalLoopback,
    /// Non-local HTTPS origin, blocked until a later exact request approval.
    Remote,
}

/// Safe configured endpoint projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModelEndpointV1 {
    provider_id: String,
    origin: String,
    scope: ModelEndpointScopeV1,
}

impl ModelEndpointV1 {
    /// Creates an adapter-validated safe endpoint projection.
    #[must_use]
    pub const fn new(provider_id: String, origin: String, scope: ModelEndpointScopeV1) -> Self {
        Self {
            provider_id,
            origin,
            scope,
        }
    }
}

/// Explicit provider health state; no background liveness claim is implied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderHealthStatusV1 {
    /// Local endpoint has not been checked.
    NotChecked,
    /// A required role capability was live-verified.
    Healthy,
    /// Provider answered but strict executable output was unavailable.
    CapabilityLimited,
    /// Provider could not complete the explicit probe.
    Unreachable,
    /// User cancelled the explicit probe.
    Cancelled,
    /// Remote endpoint is configured but cannot be probed without exact approval.
    RemoteBlocked,
}

/// Timestamped result of the last explicit probe, or an untimestamped initial state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderHealthV1 {
    status: ProviderHealthStatusV1,
    checked_at_unix_millis: Option<String>,
}

impl ProviderHealthV1 {
    /// Creates a safe Core-derived health projection.
    #[must_use]
    pub const fn new(
        status: ProviderHealthStatusV1,
        checked_at_unix_millis: Option<String>,
    ) -> Self {
        Self {
            status,
            checked_at_unix_millis,
        }
    }
}

/// Structured-output evidence retained by an LLM profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StructuredOutputCapabilityV1 {
    /// Exact live probe succeeded.
    Verified,
    /// Probe completed without required exact output.
    Unavailable,
}

/// Native provider tool metadata independent of executable structured output.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelToolCallModeV1 {
    /// Provider tools are not reported or used.
    Disabled,
    /// Provider metadata reported native tools.
    NativeProviderReported,
}

/// Core-derived activation state for one visible LLM candidate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelProfileActivationV1 {
    /// Profile can drive executable structured actions.
    Executable,
    /// Profile is visible for diagnosis but cannot drive executable work.
    CapabilityLimited,
}

/// Complete bounded presentation of one role-bound LLM profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LlmRoleProfileV1 {
    profile_id: String,
    model_id: String,
    context_tokens: u32,
    output_tokens: u32,
    parallelism: u16,
    structured_output: StructuredOutputCapabilityV1,
    tool_call_mode: ModelToolCallModeV1,
    activation: ModelProfileActivationV1,
    probed_at_unix_millis: String,
}

impl LlmRoleProfileV1 {
    /// Creates a complete already validated role profile projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        profile_id: String,
        model_id: String,
        context_tokens: u32,
        output_tokens: u32,
        parallelism: u16,
        structured_output: StructuredOutputCapabilityV1,
        tool_call_mode: ModelToolCallModeV1,
        activation: ModelProfileActivationV1,
        probed_at_unix_millis: String,
    ) -> Self {
        Self {
            profile_id,
            model_id,
            context_tokens,
            output_tokens,
            parallelism,
            structured_output,
            tool_call_mode,
            activation,
            probed_at_unix_millis,
        }
    }
}

/// Complete bounded presentation of the live-proven embedding profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EmbeddingRoleProfileV1 {
    profile_id: String,
    model_id: String,
    dimension: u16,
    max_batch_size: u16,
    probed_at_unix_millis: String,
}

impl EmbeddingRoleProfileV1 {
    /// Creates a provider-proven embedding profile projection.
    #[must_use]
    pub const fn new(
        profile_id: String,
        model_id: String,
        dimension: u16,
        max_batch_size: u16,
        probed_at_unix_millis: String,
    ) -> Self {
        Self {
            profile_id,
            model_id,
            dimension,
            max_batch_size,
            probed_at_unix_millis,
        }
    }
}

/// Read-only fail-closed privacy boundary implemented by this build.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DataPrivacySettingsV1 {
    telemetry_enabled: bool,
    cloud_sync_enabled: bool,
    automatic_provider_discovery_enabled: bool,
    prompt_response_logging_enabled: bool,
    remote_requests_without_approval_enabled: bool,
}

impl DataPrivacySettingsV1 {
    /// Creates the exact Core-derived privacy projection.
    #[must_use]
    pub const fn new(
        telemetry_enabled: bool,
        cloud_sync_enabled: bool,
        automatic_provider_discovery_enabled: bool,
        prompt_response_logging_enabled: bool,
        remote_requests_without_approval_enabled: bool,
    ) -> Self {
        Self {
            telemetry_enabled,
            cloud_sync_enabled,
            automatic_provider_discovery_enabled,
            prompt_response_logging_enabled,
            remote_requests_without_approval_enabled,
        }
    }
}

/// Complete global settings projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SettingsV1 {
    revision: String,
    endpoint: Option<ModelEndpointV1>,
    provider_health: Option<ProviderHealthV1>,
    coding_profile: Option<LlmRoleProfileV1>,
    mapping_profile: Option<LlmRoleProfileV1>,
    embedding_profile: Option<EmbeddingRoleProfileV1>,
    privacy: DataPrivacySettingsV1,
    probe_active: bool,
}

impl SettingsV1 {
    /// Creates one complete Core-derived settings snapshot.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        revision: String,
        endpoint: Option<ModelEndpointV1>,
        provider_health: Option<ProviderHealthV1>,
        coding_profile: Option<LlmRoleProfileV1>,
        mapping_profile: Option<LlmRoleProfileV1>,
        embedding_profile: Option<EmbeddingRoleProfileV1>,
        privacy: DataPrivacySettingsV1,
        probe_active: bool,
    ) -> Self {
        Self {
            revision,
            endpoint,
            provider_health,
            coding_profile,
            mapping_profile,
            embedding_profile,
            privacy,
            probe_active,
        }
    }
}

/// Settings command response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SettingsResponseV1 {
    protocol_version: ProtocolVersion,
    settings: SettingsV1,
}

/// Bounded result of one explicit local provider model-catalog read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderModelsResponseV1 {
    protocol_version: ProtocolVersion,
    settings_revision: String,
    provider_kind: ModelProviderKindV1,
    model_ids: Vec<String>,
    truncated: bool,
}

impl ProviderModelsResponseV1 {
    /// Creates a current-protocol catalog already validated by Core and adapter boundaries.
    #[must_use]
    pub const fn new(
        settings_revision: String,
        provider_kind: ModelProviderKindV1,
        model_ids: Vec<String>,
        truncated: bool,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            settings_revision,
            provider_kind,
            model_ids,
            truncated,
        }
    }
}

impl SettingsResponseV1 {
    /// Creates one current-protocol settings response.
    #[must_use]
    pub const fn new(settings: SettingsV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            settings,
        }
    }
}

/// Result of an explicit cancel request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CancelModelProbeResponseV1 {
    protocol_version: ProtocolVersion,
    cancellation_requested: bool,
}

impl CancelModelProbeResponseV1 {
    /// Creates a current-protocol cancellation acknowledgement.
    #[must_use]
    pub const fn new(cancellation_requested: bool) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            cancellation_requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiscoverProviderModelsRequestV1, ProbeModelRoleRequestV1, QuerySettingsRequestV1};
    use crate::ProtocolVersion;

    #[test]
    fn probe_request_rejects_endpoint_capability_and_dimension_authority() {
        for forbidden in [
            "endpointOrigin",
            "providerId",
            "profileId",
            "structuredOutput",
            "dimension",
            "probedAtUnixMillis",
        ] {
            let mut value = serde_json::json!({
                "protocolVersion": 1,
                "expectedSettingsRevision": "0",
                "role": "coding",
                "modelId": "coder",
                "llmLimits": {"contextTokens": 16384, "outputTokens": 2048, "parallelism": 1},
                "embeddingLimits": null
            });
            value[forbidden] = serde_json::json!("forbidden");
            assert!(serde_json::from_value::<ProbeModelRoleRequestV1>(value).is_err());
        }
    }

    #[test]
    fn settings_query_is_version_only() -> Result<(), serde_json::Error> {
        let request: QuerySettingsRequestV1 =
            serde_json::from_value(serde_json::json!({"protocolVersion": 1}))?;
        assert_eq!(request.protocol_version(), ProtocolVersion::V1);
        assert!(
            serde_json::from_value::<QuerySettingsRequestV1>(
                serde_json::json!({"protocolVersion": 1, "endpoint": "http://localhost"})
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn model_discovery_request_carries_no_endpoint_or_provider_authority() {
        for forbidden in [
            "endpointOrigin",
            "providerId",
            "providerKind",
            "modelId",
            "capability",
            "checkedAtUnixMillis",
        ] {
            let mut value = serde_json::json!({
                "protocolVersion": 1,
                "expectedSettingsRevision": "2"
            });
            value[forbidden] = serde_json::json!("forbidden");
            assert!(serde_json::from_value::<DiscoverProviderModelsRequestV1>(value).is_err());
        }
    }

    #[test]
    fn model_provider_kind_serializes_and_deserializes() -> Result<(), serde_json::Error> {
        use super::ModelProviderKindV1;
        assert_eq!(
            serde_json::to_string(&ModelProviderKindV1::Ollama)?,
            "\"ollama\""
        );
        assert_eq!(
            serde_json::to_string(&ModelProviderKindV1::Gemini)?,
            "\"gemini\""
        );
        assert_eq!(
            serde_json::from_str::<ModelProviderKindV1>("\"ollama\"")?,
            ModelProviderKindV1::Ollama
        );
        assert_eq!(
            serde_json::from_str::<ModelProviderKindV1>("\"gemini\"")?,
            ModelProviderKindV1::Gemini
        );
        assert!(serde_json::from_str::<ModelProviderKindV1>("\"other\"").is_err());
        Ok(())
    }
}
