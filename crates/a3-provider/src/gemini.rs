use a3_application::{
    ConfiguredModelEndpoint, EmbeddingCapabilityProbe, EmbeddingCapabilityProbeFuture,
    EmbeddingCapabilityProbeRequest, EmbeddingOperationControl, EmbeddingProvider,
    EmbeddingProviderFailure, EmbeddingProviderFuture, EmbeddingRequestTimeout,
    ModelCapabilityObservation, ModelCapabilityProbe, ModelCapabilityProbeFuture,
    ModelCapabilityProbeRequest, ModelCatalogFuture, ModelCatalogProvider, ModelEndpointAccess,
    ModelEndpointScope, ModelEndpointValidationFailure, ModelEndpointValidator, ModelFinishReason,
    ModelMessageRole, ModelOperationControl, ModelOutputChunk, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture, ModelProviderRequest,
    ModelProviderUsage, ModelRequestTimeout, ProviderApiKey, ProviderCredentialRequirement,
    ProviderEvent, ProviderEventStream, ProviderModelCatalog, RawEmbeddingBatch,
    ReportedModelContextLimit,
};
use a3_domain::{
    EmbeddingDimension, EmbeddingProviderId, ModelCapabilities, ModelId, ModelProviderId,
    ModelStructuredOutputCapability, ModelToolCallMode, NormalizedSemanticCard,
};
use futures::future::{Either, select};
use futures::stream::{BoxStream, StreamExt};
use futures::{FutureExt, pin_mut};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

const GEMINI_PROVIDER_ID: &str = "gemini";
const DEFAULT_GEMINI_ORIGIN: &str = "https://generativelanguage.googleapis.com";
const JSON_CONTENT_TYPE: &str = "application/json";
const EVENT_STREAM_CONTENT_TYPE: &str = "text/event-stream";
const API_KEY_HEADER: &str = "x-goog-api-key";

const MAX_GEMINI_BUFFER_BYTES: usize = 256 * 1024;
const MAX_GEMINI_LINE_BYTES: usize = 128 * 1024;
const MAX_GEMINI_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_GEMINI_MODELS_BYTES: usize = 512 * 1024;
const MAX_GEMINI_MODELS_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_GEMINI_SHOW_BYTES: usize = 512 * 1024;
const MAX_GEMINI_PROBE_BYTES: usize = 128 * 1024;
const MAX_GEMINI_EMBED_BYTES: usize = 8 * 1024 * 1024;
const MAX_GEMINI_EMBED_PROBE_BYTES: usize = 256 * 1024;
const MAX_GEMINI_MODELS_COUNT: usize = 256;
const MAX_GEMINI_MODEL_PAGES: usize = 10;
const MAX_GEMINI_MODELS_OBSERVED: usize = 1_000;
const MAX_GEMINI_PAGE_TOKEN_BYTES: usize = 4_096;
const MAX_GEMINI_SCHEMA_DEPTH: usize = 64;
const MAX_GEMINI_SCHEMA_NODES: usize = 4_096;

// Gemini 2.5 and 3.x account for internal thinking inside the output budget. A 32-token
// probe can therefore end before the tiny visible JSON value is emitted. Keep the probe
// bounded, but leave enough room for the provider's default thinking behavior.
const GEMINI_PROBE_OUTPUT_TOKENS: u32 = 256;
const GEMINI_PROBE_PROMPT: &str =
    "Return exactly this JSON object and nothing else: {\"a3_probe\":\"ok\"}.";
const GEMINI_EMBED_PROBE_INPUT: &str = "A3 embedding capability probe";

/// Whether a Gemini endpoint stays on the host loopback boundary or reaches Google APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeminiEndpointScope {
    /// Literal IPv4 or IPv6 loopback (e.g. for offline testing / proxies).
    LocalLoopback,
    /// Remote HTTPS origin (e.g. `https://generativelanguage.googleapis.com`).
    Remote,
}

/// Validated credential-free origin for a Google Gemini API endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct GeminiEndpoint {
    url: reqwest::Url,
    scope: GeminiEndpointScope,
}

impl GeminiEndpoint {
    /// Returns the default canonical Gemini origin (`https://generativelanguage.googleapis.com`).
    pub fn default_origin() -> Result<Self, GeminiEndpointError> {
        Self::parse(DEFAULT_GEMINI_ORIGIN)
    }

    /// Parses an origin, normalizes `localhost` to IPv4 loopback, and rejects path/query/credentials.
    pub fn parse(value: &str) -> Result<Self, GeminiEndpointError> {
        let mut url = reqwest::Url::parse(value).map_err(|_| GeminiEndpointError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(GeminiEndpointError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(GeminiEndpointError::CredentialsForbidden);
        }
        if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
            return Err(GeminiEndpointError::OriginRequired);
        }
        let host = url.host_str().ok_or(GeminiEndpointError::MissingHost)?;
        if host.eq_ignore_ascii_case("localhost") {
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| GeminiEndpointError::InvalidUrl)?;
        }
        let scope = url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .map_or(GeminiEndpointScope::Remote, endpoint_scope);
        if scope == GeminiEndpointScope::Remote && url.scheme() != "https" {
            return Err(GeminiEndpointError::InsecureRemote);
        }
        Ok(Self { url, scope })
    }

    /// Returns whether the validated origin is loopback or remote.
    #[must_use]
    pub const fn scope(&self) -> GeminiEndpointScope {
        self.scope
    }

    /// Returns the normalized credential-free origin without an API path.
    #[must_use]
    pub fn canonical_origin(&self) -> String {
        self.url.as_str().trim_end_matches('/').to_owned()
    }

    pub(crate) fn models_url(&self, page_token: Option<&str>) -> reqwest::Url {
        let mut url = self.url.clone();
        url.set_path("/v1beta/models");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("pageSize", "100");
            if let Some(token) = page_token {
                query.append_pair("pageToken", token);
            }
        }
        url
    }

    pub(crate) fn model_url(&self, model: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        let normalized = normalize_model_path(model);
        url.set_path(&format!("/v1beta/models/{normalized}"));
        url
    }

    pub(crate) fn generate_content_url(&self, model: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        let normalized = normalize_model_path(model);
        url.set_path(&format!("/v1beta/models/{normalized}:generateContent"));
        url
    }

    pub(crate) fn stream_generate_content_url(&self, model: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        let normalized = normalize_model_path(model);
        url.set_path(&format!(
            "/v1beta/models/{normalized}:streamGenerateContent"
        ));
        url.set_query(Some("alt=sse"));
        url
    }

    pub(crate) fn embed_content_url(&self, model: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        let normalized = normalize_model_path(model);
        url.set_path(&format!("/v1beta/models/{normalized}:embedContent"));
        url
    }

    pub(crate) fn batch_embed_contents_url(&self, model: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        let normalized = normalize_model_path(model);
        url.set_path(&format!("/v1beta/models/{normalized}:batchEmbedContents"));
        url
    }
}

impl fmt::Debug for GeminiEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiEndpoint")
            .field("scheme", &self.url.scheme())
            .field("scope", &self.scope)
            .field("port", &self.url.port_or_known_default())
            .finish()
    }
}

fn endpoint_scope(address: IpAddr) -> GeminiEndpointScope {
    if address.is_loopback() {
        GeminiEndpointScope::LocalLoopback
    } else {
        GeminiEndpointScope::Remote
    }
}

fn normalize_model_path(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

/// Invalid or unsafe Gemini endpoint configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiEndpointError {
    /// Input was not an absolute URL.
    InvalidUrl,
    /// Only HTTP and HTTPS are supported.
    UnsupportedScheme,
    /// URL did not contain a host.
    MissingHost,
    /// Userinfo could expose credentials through configuration or logs.
    CredentialsForbidden,
    /// Configuration must be an origin without API path, query, or fragment.
    OriginRequired,
    /// Non-loopback endpoints require HTTPS in addition to explicit policy approval.
    InsecureRemote,
}

impl fmt::Display for GeminiEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUrl => "gemini endpoint is not a valid absolute URL",
            Self::UnsupportedScheme => "gemini endpoint scheme is unsupported",
            Self::MissingHost => "gemini endpoint has no host",
            Self::CredentialsForbidden => "gemini endpoint must not contain credentials",
            Self::OriginRequired => "gemini endpoint must be an origin without path or query",
            Self::InsecureRemote => "remote gemini endpoint must use HTTPS",
        })
    }
}

impl Error for GeminiEndpointError {}

/// Pure Settings adapter for Google Gemini endpoint validation and canonicalization.
#[derive(Debug, Clone, Copy, Default)]
pub struct GeminiSettingsEndpointValidator;

impl ModelEndpointValidator for GeminiSettingsEndpointValidator {
    fn validate(
        &self,
        input: &str,
    ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure> {
        let endpoint =
            GeminiEndpoint::parse(input).map_err(|_| ModelEndpointValidationFailure::Invalid)?;
        if endpoint.canonical_origin() != DEFAULT_GEMINI_ORIGIN {
            return Err(ModelEndpointValidationFailure::Invalid);
        }
        let provider_id = ModelProviderId::try_from_string(GEMINI_PROVIDER_ID.to_owned())
            .map_err(|_| ModelEndpointValidationFailure::ProviderUnavailable)?;
        let scope = match endpoint.scope() {
            GeminiEndpointScope::LocalLoopback => ModelEndpointScope::LocalLoopback,
            GeminiEndpointScope::Remote => ModelEndpointScope::Remote,
        };
        ConfiguredModelEndpoint::from_validated_adapter_with_security(
            provider_id,
            endpoint.canonical_origin(),
            scope,
            ModelEndpointAccess::ExplicitUserInitiatedRemote,
            ProviderCredentialRequirement::ApiKey,
        )
        .map_err(|_| ModelEndpointValidationFailure::Invalid)
    }
}

/// Dynamic authorization checked before every Gemini model request.
pub trait GeminiEndpointPolicy: fmt::Debug + Send + Sync {
    /// Authorizes the exact current endpoint or returns a content-free denial.
    fn authorize(&self, endpoint: &GeminiEndpoint) -> Result<(), GeminiEndpointPolicyError>;
}

/// Production policy allowing only the canonical Google Gemini HTTPS origin.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardGeminiEndpointPolicy;

impl GeminiEndpointPolicy for StandardGeminiEndpointPolicy {
    fn authorize(&self, endpoint: &GeminiEndpoint) -> Result<(), GeminiEndpointPolicyError> {
        if endpoint.canonical_origin() == DEFAULT_GEMINI_ORIGIN {
            Ok(())
        } else {
            Err(GeminiEndpointPolicyError::Denied)
        }
    }
}

/// Test-only policy allowing loopback endpoints only.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalOnlyGeminiEndpointPolicy;

impl GeminiEndpointPolicy for LocalOnlyGeminiEndpointPolicy {
    fn authorize(&self, endpoint: &GeminiEndpoint) -> Result<(), GeminiEndpointPolicyError> {
        if endpoint.scope() == GeminiEndpointScope::LocalLoopback {
            Ok(())
        } else {
            Err(GeminiEndpointPolicyError::Denied)
        }
    }
}

/// Endpoint was not authorized by current policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiEndpointPolicyError {
    /// Exact configured endpoint is not authorized.
    Denied,
}

impl fmt::Display for GeminiEndpointPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("gemini endpoint is not authorized by current policy")
    }
}

impl Error for GeminiEndpointPolicyError {}

/// Google Gemini implementation of the general streaming model-provider port.
pub struct GeminiModelProvider {
    provider_id: ModelProviderId,
    embedding_provider_id: EmbeddingProviderId,
    endpoint: GeminiEndpoint,
    endpoint_policy: Arc<dyn GeminiEndpointPolicy>,
    client: reqwest::Client,
    api_key: ProviderApiKey,
}

impl GeminiModelProvider {
    /// Creates a Gemini provider for one validated endpoint and explicit short-lived API key.
    pub fn new(
        endpoint: GeminiEndpoint,
        endpoint_policy: Arc<dyn GeminiEndpointPolicy>,
        api_key: ProviderApiKey,
    ) -> Result<Self, GeminiProviderCreateError> {
        let provider_id = ModelProviderId::try_from_string(GEMINI_PROVIDER_ID.to_owned())
            .map_err(|_| GeminiProviderCreateError::InvalidProviderIdentity)?;
        let embedding_provider_id = EmbeddingProviderId::new(GEMINI_PROVIDER_ID.to_owned())
            .map_err(|_| GeminiProviderCreateError::InvalidProviderIdentity)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| GeminiProviderCreateError::HttpClient)?;
        Ok(Self {
            provider_id,
            embedding_provider_id,
            endpoint,
            endpoint_policy,
            client,
            api_key,
        })
    }

    fn authorize_request(
        &self,
        control: &dyn ModelOperationControl,
    ) -> Result<(), ModelProviderFailure> {
        self.endpoint_policy
            .authorize(&self.endpoint)
            .map_err(|_| ModelProviderFailure::EndpointDenied)?;
        if control.is_cancelled() {
            return Err(ModelProviderFailure::Cancelled);
        }
        Ok(())
    }

    fn attach_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header(API_KEY_HEADER, self.api_key.as_bytes())
            .header("x-goog-api-client", "a3/0.1.0")
    }
}

impl fmt::Debug for GeminiModelProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiModelProvider")
            .field("provider_id", &self.provider_id)
            .field("embedding_provider_id", &self.embedding_provider_id)
            .field("endpoint", &self.endpoint)
            .field("endpoint_policy", &self.endpoint_policy)
            .field("has_api_key", &true)
            .finish()
    }
}

/// Failure creating a Gemini provider adapter instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiProviderCreateError {
    /// Provider identity was rejected by domain validation rules.
    InvalidProviderIdentity,
    /// HTTP client initialization failed.
    HttpClient,
}

impl fmt::Display for GeminiProviderCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProviderIdentity => "gemini provider identity is invalid",
            Self::HttpClient => "failed to initialize HTTP client for gemini provider",
        })
    }
}

impl Error for GeminiProviderCreateError {}

impl ModelProvider for GeminiModelProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.authorize_request(control)?;
            if request.profile().provider_id() != &self.provider_id {
                return Err(ModelProviderFailure::Rejected);
            }
            let wire_request = GeminiGenerateContentRequest::from_request(request)?;
            let target_url = self
                .endpoint
                .stream_generate_content_url(request.model_id().as_str());
            let http_request = self.attach_auth(
                self.client
                    .post(target_url)
                    .timeout(timeout.duration())
                    .json(&wire_request),
            );
            let send = http_request.send().fuse();
            let cancelled = control.cancelled().fuse();
            pin_mut!(send, cancelled);
            let response = match select(cancelled, send).await {
                Either::Left(((), _)) => return Err(ModelProviderFailure::Cancelled),
                Either::Right((result, _)) => result.map_err(classify_reqwest_error)?,
            };
            validate_stream_response_head(&response)?;
            let body = response
                .bytes_stream()
                .map(|item| item.map(|bytes| bytes.to_vec()))
                .boxed();
            let state = GeminiStreamState::new(body, control);
            let stream = futures::stream::try_unfold(state, next_gemini_event);
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

impl ModelCatalogProvider for GeminiModelProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn discover_models<'a>(
        &'a self,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelCatalogFuture<'a> {
        Box::pin(async move {
            self.authorize_request(control)?;
            let deadline = Instant::now()
                .checked_add(timeout.duration())
                .ok_or(ModelProviderFailure::TimedOut)?;
            let mut models = BTreeSet::new();
            let mut page_token: Option<String> = None;
            let mut seen_tokens = HashSet::new();
            let mut observed = 0usize;
            let mut total_bytes = 0usize;
            let mut truncated = false;
            for page_index in 0..MAX_GEMINI_MODEL_PAGES {
                let request = self.attach_auth(
                    self.client
                        .get(self.endpoint.models_url(page_token.as_deref())),
                );
                let response = send_before_deadline(request, deadline, control).await?;
                validate_json_response_head(&response)?;
                let body =
                    read_bounded_response(response, MAX_GEMINI_MODELS_BYTES, control).await?;
                total_bytes = total_bytes.saturating_add(body.len());
                if total_bytes > MAX_GEMINI_MODELS_TOTAL_BYTES {
                    return Err(ModelProviderFailure::InvalidResponse);
                }
                let page = parse_gemini_models_page(&body)?;
                for item in page.models {
                    observed = observed.saturating_add(1);
                    if observed > MAX_GEMINI_MODELS_OBSERVED {
                        truncated = true;
                        break;
                    }
                    if model_supports_a3_role(&item) {
                        let name = normalize_model_path(&item.name);
                        if let Ok(model_id) = ModelId::try_from_string(name.to_owned()) {
                            models.insert(model_id);
                            if models.len() > MAX_GEMINI_MODELS_COUNT {
                                truncated = true;
                                break;
                            }
                        }
                    }
                }
                if truncated {
                    break;
                }
                let Some(next) = page.next_page_token else {
                    break;
                };
                validate_page_token(&next)?;
                if !seen_tokens.insert(next.clone()) {
                    return Err(ModelProviderFailure::InvalidResponse);
                }
                page_token = Some(next);
                if page_index + 1 == MAX_GEMINI_MODEL_PAGES {
                    truncated = true;
                }
            }
            let mut models = models.into_iter().collect::<Vec<_>>();
            if models.len() > MAX_GEMINI_MODELS_COUNT {
                models.truncate(MAX_GEMINI_MODELS_COUNT);
                truncated = true;
            }
            Ok(ProviderModelCatalog::from_observation(
                self.provider_id.clone(),
                models,
                truncated,
            ))
        })
    }
}

impl ModelCapabilityProbe for GeminiModelProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn probe<'a>(
        &'a self,
        request: &'a ModelCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelCapabilityProbeFuture<'a> {
        Box::pin(async move {
            self.authorize_request(control)?;
            let deadline = Instant::now()
                .checked_add(timeout.duration())
                .ok_or(ModelProviderFailure::TimedOut)?;
            let show = self.show_model(request, deadline, control).await?;
            let structured_output = self
                .probe_structured_output(request, deadline, control)
                .await?;
            let tool_call_mode = ModelToolCallMode::Disabled;
            Ok(ModelCapabilityObservation::new(
                show.context_limit,
                ModelCapabilities::new(structured_output, tool_call_mode),
            ))
        })
    }
}

impl EmbeddingCapabilityProbe for GeminiModelProvider {
    fn provider_id(&self) -> &EmbeddingProviderId {
        &self.embedding_provider_id
    }

    fn probe_embedding<'a>(
        &'a self,
        request: &'a EmbeddingCapabilityProbeRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> EmbeddingCapabilityProbeFuture<'a> {
        Box::pin(async move {
            self.authorize_request(control)?;
            let deadline = Instant::now()
                .checked_add(timeout.duration())
                .ok_or(ModelProviderFailure::TimedOut)?;
            let wire_request = GeminiEmbedContentRequest {
                content: GeminiContent {
                    parts: vec![GeminiPart {
                        text: GEMINI_EMBED_PROBE_INPUT,
                    }],
                    role: None,
                },
            };
            let target_url = self.endpoint.embed_content_url(request.model_id().as_str());
            let http_request = self.attach_auth(self.client.post(target_url).json(&wire_request));
            let response = send_before_deadline(http_request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body =
                read_bounded_response(response, MAX_GEMINI_EMBED_PROBE_BYTES, control).await?;
            gemini_embedding_probe_dimension(&body)
        })
    }
}

impl EmbeddingProvider for GeminiModelProvider {
    fn embed<'a>(
        &'a self,
        profile: &'a a3_domain::EmbeddingModelProfile,
        cards: &'a [NormalizedSemanticCard],
        timeout: EmbeddingRequestTimeout,
        control: &'a dyn EmbeddingOperationControl,
    ) -> EmbeddingProviderFuture<'a> {
        Box::pin(async move {
            self.endpoint_policy
                .authorize(&self.endpoint)
                .map_err(|_| EmbeddingProviderFailure::Rejected)?;
            if control.is_cancelled() {
                return Err(EmbeddingProviderFailure::Cancelled);
            }
            if profile.provider_id() != &self.embedding_provider_id
                || cards.is_empty()
                || cards.len() > usize::from(profile.max_batch_size().get())
            {
                return Err(EmbeddingProviderFailure::Rejected);
            }
            let model_full_name = format!(
                "models/{}",
                normalize_model_path(profile.model_id().as_str())
            );
            let batch_items = cards
                .iter()
                .map(|card| GeminiBatchEmbedItem {
                    model: &model_full_name,
                    content: GeminiContent {
                        parts: vec![GeminiPart { text: card.body() }],
                        role: None,
                    },
                })
                .collect::<Vec<_>>();
            let wire_request = GeminiBatchEmbedContentsRequest {
                requests: batch_items,
            };
            let target_url = self
                .endpoint
                .batch_embed_contents_url(profile.model_id().as_str());
            let http_request = self.attach_auth(
                self.client
                    .post(target_url)
                    .timeout(timeout.duration())
                    .json(&wire_request),
            );
            let response = http_request
                .send()
                .await
                .map_err(classify_embedding_reqwest_error)?;
            if control.is_cancelled() {
                return Err(EmbeddingProviderFailure::Cancelled);
            }
            validate_json_response_head(&response).map_err(map_embedding_failure)?;
            let body =
                read_bounded_embedding_response(response, MAX_GEMINI_EMBED_BYTES, control).await?;
            let batch = parse_gemini_batch_embedding_response(&body)?;
            let vectors = batch.into_vectors();
            let expected_dimension = usize::from(profile.dimension().get());
            if vectors.len() != cards.len()
                || vectors.iter().any(|vector| {
                    vector.len() != expected_dimension
                        || vector.iter().any(|component| !component.is_finite())
                        || vector.iter().all(|component| *component == 0.0)
                })
            {
                return Err(EmbeddingProviderFailure::InvalidResponse);
            }
            RawEmbeddingBatch::new(vectors).map_err(|_| EmbeddingProviderFailure::InvalidResponse)
        })
    }
}

impl GeminiModelProvider {
    async fn show_model(
        &self,
        request: &ModelCapabilityProbeRequest,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<GeminiShowObservation, ModelProviderFailure> {
        let target_url = self.endpoint.model_url(request.model_id().as_str());
        let http_request = self.attach_auth(self.client.get(target_url));
        let response = send_before_deadline(http_request, deadline, control).await?;
        validate_json_response_head(&response)?;
        let body = read_bounded_response(response, MAX_GEMINI_SHOW_BYTES, control).await?;
        parse_gemini_show_observation(&body)
    }

    async fn probe_structured_output(
        &self,
        request: &ModelCapabilityProbeRequest,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<ModelStructuredOutputCapability, ModelProviderFailure> {
        let probe_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "a3_probe": {
                    "type": "string",
                    "enum": ["ok"]
                }
            },
            "required": ["a3_probe"],
            "additionalProperties": false
        });
        let wire_request = GeminiGenerateContentRequest {
            contents: Some(vec![GeminiContent {
                role: Some("user"),
                parts: vec![GeminiPart {
                    text: GEMINI_PROBE_PROMPT,
                }],
            }]),
            system_instruction: None,
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(0.0),
                top_p: Some(1.0),
                max_output_tokens: Some(GEMINI_PROBE_OUTPUT_TOKENS),
                stop_sequences: None,
                response_mime_type: Some("application/json"),
                response_json_schema: Some(probe_schema),
            }),
        };
        let target_url = self
            .endpoint
            .generate_content_url(request.model_id().as_str());
        let http_request = self.attach_auth(self.client.post(target_url).json(&wire_request));
        let response = send_before_deadline(http_request, deadline, control).await?;
        if let Err(error) = validate_json_response_head(&response) {
            return match error {
                ModelProviderFailure::Rejected | ModelProviderFailure::InvalidResponse => {
                    Ok(ModelStructuredOutputCapability::Unavailable)
                }
                other => Err(other),
            };
        }
        let body = match read_bounded_response(response, MAX_GEMINI_PROBE_BYTES, control).await {
            Ok(bytes) => bytes,
            Err(ModelProviderFailure::InvalidResponse | ModelProviderFailure::Rejected) => {
                return Ok(ModelStructuredOutputCapability::Unavailable);
            }
            Err(other) => return Err(other),
        };
        match parse_gemini_probe_response(&body) {
            Ok(true) => Ok(ModelStructuredOutputCapability::Verified),
            Ok(false) | Err(ModelProviderFailure::InvalidResponse) => {
                Ok(ModelStructuredOutputCapability::Unavailable)
            }
            Err(other) => Err(other),
        }
    }
}

#[derive(Serialize)]
struct GeminiGenerateContentRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    contents: Option<Vec<GeminiContent<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    system_instruction: Option<GeminiSystemInstruction<'a>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig<'a>>,
}

impl<'a> GeminiGenerateContentRequest<'a> {
    fn from_request(request: &'a ModelProviderRequest) -> Result<Self, ModelProviderFailure> {
        let mut system_parts = Vec::new();
        let mut user_contents = Vec::new();

        for message in request.messages() {
            match message.role() {
                ModelMessageRole::System => {
                    system_parts.push(GeminiPart {
                        text: message.content(),
                    });
                }
                ModelMessageRole::User => {
                    user_contents.push(GeminiContent {
                        role: Some("user"),
                        parts: vec![GeminiPart {
                            text: message.content(),
                        }],
                    });
                }
                ModelMessageRole::Assistant => {
                    user_contents.push(GeminiContent {
                        role: Some("model"),
                        parts: vec![GeminiPart {
                            text: message.content(),
                        }],
                    });
                }
            }
        }

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            Some(GeminiSystemInstruction {
                parts: system_parts,
            })
        };

        let settings = request.profile().settings();
        let stops = settings.stop_sequences().as_slice();
        let stop_sequences =
            (!stops.is_empty()).then(|| stops.iter().map(|stop| stop.as_str()).collect());

        let (response_mime_type, response_json_schema) = match request.structured_output() {
            Some(schema) => (
                Some("application/json"),
                Some(translate_response_json_schema(schema.value())?),
            ),
            None => (None, None),
        };

        let generation_config = Some(GeminiGenerationConfig {
            temperature: Some(f32::from(settings.sampling().temperature().milli()) / 1_000.0),
            top_p: Some(f32::from(settings.sampling().top_p().milli()) / 1_000.0),
            max_output_tokens: Some(settings.output_limit().get()),
            stop_sequences,
            response_mime_type,
            response_json_schema,
        });

        Ok(Self {
            contents: Some(user_contents),
            system_instruction,
            generation_config,
        })
    }
}

#[derive(Serialize)]
struct GeminiContent<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GeminiSystemInstruction<'a> {
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiGenerationConfig<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "topP")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopSequences")]
    stop_sequences: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "responseMimeType")]
    response_mime_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "responseJsonSchema")]
    response_json_schema: Option<Value>,
}

fn translate_response_json_schema(schema: &Value) -> Result<Value, ModelProviderFailure> {
    let mut nodes = 0usize;
    let mut translated = translate_schema_node(schema, 0, &mut nodes)?;
    compact_equivalent_prefix_items(&mut translated);
    prune_unused_definitions(&mut translated)?;
    Ok(translated)
}

fn translate_schema_node(
    schema: &Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<Value, ModelProviderFailure> {
    if depth > MAX_GEMINI_SCHEMA_DEPTH || *nodes >= MAX_GEMINI_SCHEMA_NODES {
        return Err(ModelProviderFailure::Rejected);
    }
    *nodes += 1;
    let object = schema.as_object().ok_or(ModelProviderFailure::Rejected)?;
    let mut translated = serde_json::Map::new();
    for (key, value) in object {
        match key.as_str() {
            "$schema" | "$id" | "$anchor" | "pattern" | "minLength" | "maxLength"
            | "uniqueItems" => {}
            "const" => {
                if object.contains_key("enum") {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert("enum".to_owned(), Value::Array(vec![value.clone()]));
            }
            "$ref" | "type" | "format" | "title" | "description" | "enum" | "minItems"
            | "maxItems" | "minimum" | "maximum" | "required" | "propertyOrdering" => {
                translated.insert(key.clone(), value.clone());
            }
            "items" | "additionalProperties" => {
                let value = if value.is_boolean() {
                    value.clone()
                } else {
                    translate_schema_node(value, depth + 1, nodes)?
                };
                translated.insert(key.clone(), value);
            }
            "prefixItems" | "anyOf" | "oneOf" => {
                let items = value.as_array().ok_or(ModelProviderFailure::Rejected)?;
                let translated_items = items
                    .iter()
                    .map(|item| translate_schema_node(item, depth + 1, nodes))
                    .collect::<Result<Vec<_>, _>>()?;
                let translated_key = if key == "oneOf" { "anyOf" } else { key };
                if translated.contains_key(translated_key) {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert(translated_key.to_owned(), Value::Array(translated_items));
            }
            "properties" | "$defs" => {
                let entries = value.as_object().ok_or(ModelProviderFailure::Rejected)?;
                let mut translated_entries = serde_json::Map::new();
                for (name, child) in entries {
                    translated_entries.insert(
                        name.clone(),
                        translate_schema_node(child, depth + 1, nodes)?,
                    );
                }
                translated.insert(key.clone(), Value::Object(translated_entries));
            }
            _ => return Err(ModelProviderFailure::Rejected),
        }
    }
    Ok(Value::Object(translated))
}

fn compact_equivalent_prefix_items(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            for value in object.values_mut() {
                compact_equivalent_prefix_items(value);
            }
            let merged = object
                .get("prefixItems")
                .and_then(Value::as_array)
                .filter(|items| {
                    !items.is_empty()
                        && !object.contains_key("items")
                        && exact_array_length(object, items.len())
                })
                .and_then(|items| {
                    let mut merged = items[0].clone();
                    for item in &items[1..] {
                        let mut candidate = merged.clone();
                        if !merge_schema_shape(&mut candidate, item, None) {
                            return None;
                        }
                        merged = candidate;
                    }
                    Some(merged)
                });
            if let Some(merged) = merged {
                object.remove("prefixItems");
                object.insert("items".to_owned(), merged);
            }
        }
        Value::Array(items) => {
            for item in items {
                compact_equivalent_prefix_items(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn exact_array_length(object: &serde_json::Map<String, Value>, length: usize) -> bool {
    let Ok(length) = u64::try_from(length) else {
        return false;
    };
    object.get("minItems").and_then(Value::as_u64) == Some(length)
        && object.get("maxItems").and_then(Value::as_u64) == Some(length)
}

fn merge_schema_shape(left: &mut Value, right: &Value, key: Option<&str>) -> bool {
    if key == Some("enum") {
        let (Some(left), Some(right)) = (left.as_array_mut(), right.as_array()) else {
            return false;
        };
        for value in right {
            if !left.contains(value) {
                left.push(value.clone());
            }
        }
        return true;
    }
    match (left, right) {
        (Value::Object(left), Value::Object(right)) if left.len() == right.len() => {
            for (name, right_value) in right {
                let Some(left_value) = left.get_mut(name) else {
                    return false;
                };
                if !merge_schema_shape(left_value, right_value, Some(name)) {
                    return false;
                }
            }
            true
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => left
            .iter_mut()
            .zip(right)
            .all(|(left, right)| merge_schema_shape(left, right, None)),
        (left, right) => left == right,
    }
}

fn prune_unused_definitions(schema: &mut Value) -> Result<(), ModelProviderFailure> {
    let Some(root) = schema.as_object() else {
        return Err(ModelProviderFailure::Rejected);
    };
    let Some(definitions) = root.get("$defs") else {
        return Ok(());
    };
    let definitions = definitions
        .as_object()
        .ok_or(ModelProviderFailure::Rejected)?
        .clone();
    let mut referenced = BTreeSet::new();
    for (name, value) in root {
        if name != "$defs" {
            collect_local_definition_references(value, &mut referenced)?;
        }
    }
    let mut pending = referenced.iter().cloned().collect::<VecDeque<_>>();
    while let Some(name) = pending.pop_front() {
        let definition = definitions
            .get(&name)
            .ok_or(ModelProviderFailure::Rejected)?;
        let mut nested = BTreeSet::new();
        collect_local_definition_references(definition, &mut nested)?;
        for nested_name in nested {
            if referenced.insert(nested_name.clone()) {
                pending.push_back(nested_name);
            }
        }
    }
    let retained = definitions
        .into_iter()
        .filter(|(name, _)| referenced.contains(name))
        .collect::<serde_json::Map<_, _>>();
    let root = schema
        .as_object_mut()
        .ok_or(ModelProviderFailure::Rejected)?;
    if retained.is_empty() {
        root.remove("$defs");
    } else {
        root.insert("$defs".to_owned(), Value::Object(retained));
    }
    Ok(())
}

fn collect_local_definition_references(
    schema: &Value,
    referenced: &mut BTreeSet<String>,
) -> Result<(), ModelProviderFailure> {
    match schema {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref") {
                let reference = reference.as_str().ok_or(ModelProviderFailure::Rejected)?;
                if let Some(name) = reference.strip_prefix("#/$defs/") {
                    let name = name.split('/').next().unwrap_or_default();
                    if name.is_empty() {
                        return Err(ModelProviderFailure::Rejected);
                    }
                    referenced.insert(name.to_owned());
                }
            }
            for value in object.values() {
                collect_local_definition_references(value, referenced)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_local_definition_references(item, referenced)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
    error: Option<GeminiErrorObject>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<GeminiPromptFeedback>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    index: Option<u32>,
    content: Option<GeminiResponseContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResponseContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
    thought: Option<bool>,
    #[serde(rename = "thoughtSignature")]
    _thought_signature: Option<String>,
    #[serde(rename = "functionCall")]
    function_call: Option<Value>,
    #[serde(rename = "functionResponse")]
    function_response: Option<Value>,
    #[serde(rename = "executableCode")]
    executable_code: Option<Value>,
    #[serde(rename = "codeExecutionResult")]
    code_execution_result: Option<Value>,
    #[serde(flatten)]
    unknown: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct GeminiPromptFeedback {
    #[serde(rename = "blockReason")]
    block_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u64>,
}

#[derive(Deserialize)]
struct GeminiErrorObject {
    #[serde(rename = "code")]
    code: Option<i64>,
    #[serde(rename = "message")]
    _message: Option<String>,
}

#[derive(Deserialize)]
struct GeminiListModelsResponse {
    models: Option<Vec<GeminiModelMetadata>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GeminiModelMetadata {
    name: String,
    #[serde(rename = "inputTokenLimit")]
    input_token_limit: Option<u64>,
    #[serde(rename = "supportedGenerationMethods")]
    supported_generation_methods: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct GeminiEmbedContentResponse {
    embedding: Option<GeminiEmbeddingValues>,
}

#[derive(Serialize)]
struct GeminiEmbedContentRequest<'a> {
    content: GeminiContent<'a>,
}

#[derive(Serialize)]
struct GeminiBatchEmbedContentsRequest<'a> {
    requests: Vec<GeminiBatchEmbedItem<'a>>,
}

#[derive(Serialize)]
struct GeminiBatchEmbedItem<'a> {
    model: &'a str,
    content: GeminiContent<'a>,
}

#[derive(Deserialize)]
struct GeminiBatchEmbedContentsResponse {
    embeddings: Option<Vec<GeminiEmbeddingValues>>,
}

#[derive(Deserialize)]
struct GeminiEmbeddingValues {
    values: Vec<f32>,
}

struct GeminiShowObservation {
    context_limit: Option<ReportedModelContextLimit>,
}

type GeminiByteStream = BoxStream<'static, Result<Vec<u8>, reqwest::Error>>;

struct GeminiStreamState<'a> {
    body: GeminiByteStream,
    control: &'a dyn ModelOperationControl,
    buffer: Vec<u8>,
    queued: VecDeque<ProviderEvent>,
    output_bytes: usize,
    prompt_tokens: Option<u64>,
    candidates_tokens: Option<u64>,
    finish_reason: Option<ModelFinishReason>,
    terminal_seen: bool,
    body_ended: bool,
}

impl<'a> GeminiStreamState<'a> {
    fn new(body: GeminiByteStream, control: &'a dyn ModelOperationControl) -> Self {
        Self {
            body,
            control,
            buffer: Vec::new(),
            queued: VecDeque::new(),
            output_bytes: 0,
            prompt_tokens: None,
            candidates_tokens: None,
            finish_reason: None,
            terminal_seen: false,
            body_ended: false,
        }
    }
}

async fn next_gemini_event(
    mut state: GeminiStreamState<'_>,
) -> Result<Option<(ProviderEvent, GeminiStreamState<'_>)>, ModelProviderFailure> {
    loop {
        if let Some(event) = state.queued.pop_front() {
            return Ok(Some((event, state)));
        }
        if state.body_ended {
            return Ok(None);
        }
        if let Some(line) = take_complete_line(&mut state.buffer)? {
            parse_gemini_sse_line(&mut state, &line)?;
            continue;
        }
        let next = read_body_or_cancel(&mut state).await?;
        match next {
            Some(bytes) => append_body_bytes(&mut state.buffer, &bytes)?,
            None => finish_gemini_body(&mut state)?,
        }
    }
}

async fn read_body_or_cancel(
    state: &mut GeminiStreamState<'_>,
) -> Result<Option<Vec<u8>>, ModelProviderFailure> {
    if state.control.is_cancelled() {
        return Err(ModelProviderFailure::Cancelled);
    }
    let read = state.body.next().fuse();
    let cancelled = state.control.cancelled().fuse();
    pin_mut!(read, cancelled);
    match select(cancelled, read).await {
        Either::Left(((), _)) => Err(ModelProviderFailure::Cancelled),
        Either::Right((Some(result), _)) => result.map(Some).map_err(classify_reqwest_error),
        Either::Right((None, _)) => Ok(None),
    }
}

fn append_body_bytes(buffer: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ModelProviderFailure> {
    if bytes.is_empty() {
        return Ok(());
    }
    if buffer.len().saturating_add(bytes.len()) > MAX_GEMINI_BUFFER_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn take_complete_line(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, ModelProviderFailure> {
    let Some(position) = buffer.iter().position(|byte| *byte == b'\n') else {
        if buffer.len() > MAX_GEMINI_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        return Ok(None);
    };
    if position > MAX_GEMINI_LINE_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut line = buffer.drain(..=position).collect::<Vec<_>>();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn parse_gemini_sse_line(
    state: &mut GeminiStreamState<'_>,
    line: &[u8],
) -> Result<(), ModelProviderFailure> {
    let line_str = std::str::from_utf8(line).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let trimmed = line_str.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return Ok(());
    }
    let json_data = if let Some(data) = trimmed.strip_prefix("data:") {
        data.trim()
    } else {
        return Ok(());
    };
    if state.terminal_seen {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if json_data == "[DONE]" {
        return Ok(());
    }
    let response: GeminiResponse =
        serde_json::from_str(json_data).map_err(|_| ModelProviderFailure::InvalidResponse)?;

    if let Some(error) = response.error {
        return Err(classify_gemini_error(&error));
    }
    if response
        .prompt_feedback
        .and_then(|feedback| feedback.block_reason)
        .is_some()
    {
        return Err(ModelProviderFailure::Rejected);
    }

    if let Some(usage) = response.usage_metadata {
        if let Some(prompt) = usage.prompt_token_count {
            state.prompt_tokens = Some(prompt);
        }
        if let Some(candidates) = usage.candidates_token_count {
            state.candidates_tokens = Some(candidates);
        }
    }

    if let Some(candidates) = response.candidates {
        for (position, candidate) in candidates.into_iter().enumerate() {
            if candidate.index.unwrap_or(position as u32) != 0 {
                continue;
            }
            if state.terminal_seen {
                return Err(ModelProviderFailure::InvalidResponse);
            }
            if let Some(parts) = candidate.content.and_then(|content| content.parts) {
                for part in parts {
                    if let Some(text) = validated_response_part_text(part)? {
                        state.output_bytes = state.output_bytes.saturating_add(text.len());
                        if state.output_bytes > MAX_GEMINI_OUTPUT_BYTES {
                            return Err(ModelProviderFailure::InvalidResponse);
                        }
                        let chunk = ModelOutputChunk::try_from_string(text)
                            .map_err(|_| ModelProviderFailure::InvalidResponse)?;
                        state.queued.push_back(ProviderEvent::OutputText(chunk));
                    }
                }
            }
            if let Some(reason) = candidate.finish_reason {
                if state.terminal_seen {
                    return Err(ModelProviderFailure::InvalidResponse);
                }
                state.finish_reason = Some(map_finish_reason(&reason)?);
                state.terminal_seen = true;
            }
        }
    }
    Ok(())
}

fn finish_gemini_body(state: &mut GeminiStreamState<'_>) -> Result<(), ModelProviderFailure> {
    if !state.buffer.is_empty() {
        if state.buffer.len() > MAX_GEMINI_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        let line = std::mem::take(&mut state.buffer);
        parse_gemini_sse_line(state, &line)?;
    }
    if !state.terminal_seen {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let usage = ModelProviderUsage::new(state.prompt_tokens, state.candidates_tokens);
    let finish_reason = state
        .finish_reason
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    let completion = ModelProviderCompletion::new(finish_reason, usage);
    state.queued.push_back(ProviderEvent::Completed(completion));
    state.body_ended = true;
    Ok(())
}

fn map_finish_reason(reason: &str) -> Result<ModelFinishReason, ModelProviderFailure> {
    match reason.to_ascii_uppercase().as_str() {
        "STOP" => Ok(ModelFinishReason::Stop),
        "MAX_TOKENS" => Ok(ModelFinishReason::OutputLimit),
        _ => Err(ModelProviderFailure::Rejected),
    }
}

fn validated_response_part_text(
    part: GeminiResponsePart,
) -> Result<Option<String>, ModelProviderFailure> {
    if part.function_call.is_some()
        || part.function_response.is_some()
        || part.executable_code.is_some()
        || part.code_execution_result.is_some()
    {
        return Err(ModelProviderFailure::Rejected);
    }
    if !part.unknown.is_empty() {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if part.thought.unwrap_or(false) {
        return Ok(None);
    }
    Ok(part.text.filter(|text| !text.is_empty()))
}

fn validate_json_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    if let Some(failure) = classify_http_status(response.status()) {
        return Err(failure);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !matches!(content_type, Some(value) if value.eq_ignore_ascii_case(JSON_CONTENT_TYPE)) {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn validate_stream_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    if let Some(failure) = classify_http_status(response.status()) {
        return Err(failure);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !matches!(
        content_type,
        Some(value) if value.eq_ignore_ascii_case(EVENT_STREAM_CONTENT_TYPE)
            || value.eq_ignore_ascii_case(JSON_CONTENT_TYPE)
    ) {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn classify_http_status(status: reqwest::StatusCode) -> Option<ModelProviderFailure> {
    if status.is_success() {
        return None;
    }
    if status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != reqwest::StatusCode::NOT_IMPLEMENTED)
    {
        Some(ModelProviderFailure::Unavailable)
    } else {
        Some(ModelProviderFailure::Rejected)
    }
}

fn classify_gemini_error(error: &GeminiErrorObject) -> ModelProviderFailure {
    error
        .code
        .and_then(|code| u16::try_from(code).ok())
        .and_then(|code| reqwest::StatusCode::from_u16(code).ok())
        .and_then(classify_http_status)
        .unwrap_or(ModelProviderFailure::InvalidResponse)
}

fn classify_reqwest_error(error: reqwest::Error) -> ModelProviderFailure {
    if error.is_timeout() {
        ModelProviderFailure::TimedOut
    } else if error.is_builder() {
        ModelProviderFailure::Rejected
    } else {
        ModelProviderFailure::Unavailable
    }
}

fn classify_embedding_reqwest_error(error: reqwest::Error) -> EmbeddingProviderFailure {
    if error.is_timeout() {
        EmbeddingProviderFailure::TimedOut
    } else if error.is_builder() {
        EmbeddingProviderFailure::Rejected
    } else {
        EmbeddingProviderFailure::Unavailable
    }
}

fn map_embedding_failure(error: ModelProviderFailure) -> EmbeddingProviderFailure {
    match error {
        ModelProviderFailure::Unavailable => EmbeddingProviderFailure::Unavailable,
        ModelProviderFailure::Rejected | ModelProviderFailure::EndpointDenied => {
            EmbeddingProviderFailure::Rejected
        }
        ModelProviderFailure::InvalidResponse => EmbeddingProviderFailure::InvalidResponse,
        ModelProviderFailure::TimedOut => EmbeddingProviderFailure::TimedOut,
        ModelProviderFailure::Cancelled => EmbeddingProviderFailure::Cancelled,
    }
}

async fn send_before_deadline(
    request: reqwest::RequestBuilder,
    deadline: Instant,
    control: &dyn ModelOperationControl,
) -> Result<reqwest::Response, ModelProviderFailure> {
    if control.is_cancelled() {
        return Err(ModelProviderFailure::Cancelled);
    }
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(ModelProviderFailure::TimedOut)?;
    let send = request.timeout(timeout).send().fuse();
    let cancelled = control.cancelled().fuse();
    pin_mut!(send, cancelled);
    match select(cancelled, send).await {
        Either::Left(((), _)) => Err(ModelProviderFailure::Cancelled),
        Either::Right((result, _)) => result.map_err(classify_reqwest_error),
    }
}

async fn read_bounded_response(
    response: reqwest::Response,
    maximum_bytes: usize,
    control: &dyn ModelOperationControl,
) -> Result<Vec<u8>, ModelProviderFailure> {
    if response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .is_some_and(|length| length > maximum_bytes)
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        if control.is_cancelled() {
            return Err(ModelProviderFailure::Cancelled);
        }
        let chunk = chunk.map_err(classify_reqwest_error)?;
        if bytes.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn read_bounded_embedding_response(
    response: reqwest::Response,
    maximum_bytes: usize,
    control: &dyn EmbeddingOperationControl,
) -> Result<Vec<u8>, EmbeddingProviderFailure> {
    if response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .is_some_and(|length| length > maximum_bytes)
    {
        return Err(EmbeddingProviderFailure::InvalidResponse);
    }
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        if control.is_cancelled() {
            return Err(EmbeddingProviderFailure::Cancelled);
        }
        let chunk = chunk.map_err(classify_embedding_reqwest_error)?;
        if bytes.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err(EmbeddingProviderFailure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_gemini_models_page(body: &[u8]) -> Result<GeminiModelsPage, ModelProviderFailure> {
    let response = serde_json::from_slice::<GeminiListModelsResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    Ok(GeminiModelsPage {
        models: response.models.unwrap_or_default(),
        next_page_token: response.next_page_token,
    })
}

struct GeminiModelsPage {
    models: Vec<GeminiModelMetadata>,
    next_page_token: Option<String>,
}

fn model_supports_a3_role(model: &GeminiModelMetadata) -> bool {
    model
        .supported_generation_methods
        .as_ref()
        .is_some_and(|methods| {
            methods.iter().any(|method| {
                matches!(
                    method.as_str(),
                    "generateContent"
                        | "streamGenerateContent"
                        | "embedContent"
                        | "batchEmbedContents"
                )
            })
        })
}

fn validate_page_token(token: &str) -> Result<(), ModelProviderFailure> {
    if token.is_empty()
        || token.len() > MAX_GEMINI_PAGE_TOKEN_BYTES
        || !token.is_ascii()
        || token.bytes().any(|byte| byte.is_ascii_control())
    {
        Err(ModelProviderFailure::InvalidResponse)
    } else {
        Ok(())
    }
}

fn parse_gemini_show_observation(
    body: &[u8],
) -> Result<GeminiShowObservation, ModelProviderFailure> {
    let metadata = serde_json::from_slice::<GeminiModelMetadata>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let context_limit = metadata
        .input_token_limit
        .and_then(|tokens| u32::try_from(tokens).ok())
        .and_then(|tokens| ReportedModelContextLimit::new(tokens).ok());
    Ok(GeminiShowObservation { context_limit })
}

fn parse_gemini_probe_response(body: &[u8]) -> Result<bool, ModelProviderFailure> {
    let response = serde_json::from_slice::<GeminiResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let Some(candidates) = response.candidates else {
        return Ok(false);
    };
    for candidate in candidates {
        if let Some(parts) = candidate.content.and_then(|content| content.parts) {
            for part in parts {
                if let Some(text) = validated_response_part_text(part)? {
                    let probe_ok = serde_json::from_str::<Value>(&text)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("a3_probe")
                                .and_then(Value::as_str)
                                .map(|s| s == "ok")
                        })
                        .unwrap_or(false);
                    if probe_ok {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

fn gemini_embedding_probe_dimension(
    body: &[u8],
) -> Result<EmbeddingDimension, ModelProviderFailure> {
    let response = serde_json::from_slice::<GeminiEmbedContentResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let embedding = response
        .embedding
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    let vector = embedding.values;
    if vector.is_empty()
        || vector.iter().any(|c| !c.is_finite())
        || vector.iter().all(|c| *c == 0.0)
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let dimension =
        u16::try_from(vector.len()).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    EmbeddingDimension::new(dimension).map_err(|_| ModelProviderFailure::InvalidResponse)
}

fn parse_gemini_batch_embedding_response(
    body: &[u8],
) -> Result<RawEmbeddingBatch, EmbeddingProviderFailure> {
    let response = serde_json::from_slice::<GeminiBatchEmbedContentsResponse>(body)
        .map_err(|_| EmbeddingProviderFailure::InvalidResponse)?;
    let embeddings = response
        .embeddings
        .ok_or(EmbeddingProviderFailure::InvalidResponse)?;
    let vectors = embeddings
        .into_iter()
        .map(|item| item.values)
        .collect::<Vec<_>>();
    RawEmbeddingBatch::new(vectors).map_err(|_| EmbeddingProviderFailure::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::{
        GeminiEndpoint, GeminiEndpointError, GeminiEndpointPolicy, GeminiEndpointScope,
        GeminiErrorObject, LocalOnlyGeminiEndpointPolicy, StandardGeminiEndpointPolicy,
        classify_gemini_error, classify_http_status, map_finish_reason,
        translate_response_json_schema,
    };
    use a3_application::{
        AgentActionJsonSchema, DecodeExplorerAction, DecodeModuleCardClaims, ModelFinishReason,
        ModelProviderFailure,
    };
    use serde_json::json;

    #[test]
    fn localhost_normalizes_and_remote_requires_https() -> Result<(), Box<dyn std::error::Error>> {
        let local = GeminiEndpoint::parse("http://localhost:8080")?;
        assert_eq!(local.scope(), GeminiEndpointScope::LocalLoopback);
        assert_eq!(local.canonical_origin(), "http://127.0.0.1:8080");
        assert!(StandardGeminiEndpointPolicy.authorize(&local).is_err());
        assert!(LocalOnlyGeminiEndpointPolicy.authorize(&local).is_ok());

        assert_eq!(
            GeminiEndpoint::parse("http://generativelanguage.googleapis.com"),
            Err(GeminiEndpointError::InsecureRemote)
        );

        let default_ep = GeminiEndpoint::default_origin()?;
        assert_eq!(default_ep.scope(), GeminiEndpointScope::Remote);
        assert!(StandardGeminiEndpointPolicy.authorize(&default_ep).is_ok());
        assert!(
            LocalOnlyGeminiEndpointPolicy
                .authorize(&default_ep)
                .is_err()
        );

        assert!(GeminiEndpoint::parse("http://user:secret@127.0.0.1:8080").is_err());
        assert!(GeminiEndpoint::parse("http://127.0.0.1:8080/v1beta").is_err());
        Ok(())
    }

    #[test]
    fn research_work_phases_translate_without_cloud_access()
    -> Result<(), Box<dyn std::error::Error>> {
        for phase in [
            a3_application::ResearchOutputPhase::Initialize,
            a3_application::ResearchOutputPhase::Analyze(a3_domain::ResearchQuestionId::FIRST),
            a3_application::ResearchOutputPhase::SummarizeOriginals(
                a3_domain::ResearchQuestionId::FIRST,
            ),
            a3_application::ResearchOutputPhase::Design(a3_domain::ResearchQuestionId::FIRST),
            a3_application::ResearchOutputPhase::Finalize,
        ] {
            let original = a3_application::research_work_phase_schema(phase, true)?;
            let translated = translate_response_json_schema(&original)?;
            assert_eq!(translated["type"], "object");
            assert!(translated.get("$id").is_none());
        }
        Ok(())
    }

    #[test]
    fn response_schema_translation_is_bounded_explicit_and_preserves_core_invariants()
    -> Result<(), Box<dyn std::error::Error>> {
        let translated = translate_response_json_schema(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "const": "result",
                    "pattern": "^result$",
                    "minLength": 6,
                    "maxLength": 6
                }
            },
            "required": ["kind"],
            "additionalProperties": false
        }))?;
        assert_eq!(translated["properties"]["kind"]["enum"], json!(["result"]));
        assert!(translated.get("$schema").is_none());
        assert!(translated["properties"]["kind"].get("pattern").is_none());
        let translated = translate_response_json_schema(&json!({
            "$id": "https://a3.local/schema.json",
            "$anchor": "result",
            "oneOf": [
                {"type": "string", "const": "a"},
                {"type": "string", "const": "b"}
            ],
            "uniqueItems": true
        }))?;
        assert!(translated.get("$id").is_none());
        assert!(translated.get("$anchor").is_none());
        assert!(translated.get("oneOf").is_none());
        assert!(translated.get("uniqueItems").is_none());
        assert_eq!(
            translated["anyOf"],
            json!([
                {"type": "string", "enum": ["a"]},
                {"type": "string", "enum": ["b"]}
            ])
        );
        assert_eq!(
            translate_response_json_schema(&json!({
                "anyOf": [{"type": "string"}],
                "oneOf": [{"type": "number"}]
            })),
            Err(ModelProviderFailure::Rejected)
        );
        assert_eq!(
            translate_response_json_schema(&json!({"type": "string", "default": "secret"})),
            Err(ModelProviderFailure::Rejected)
        );
        for schema in [
            AgentActionJsonSchema::version_one(),
            AgentActionJsonSchema::version_two(),
            AgentActionJsonSchema::version_three(),
        ] {
            translate_response_json_schema(&schema.as_json()?)?;
        }
        for schema in [
            DecodeExplorerAction::version_one().json_schema().as_str(),
            DecodeModuleCardClaims::version_one().json_schema().as_str(),
        ] {
            translate_response_json_schema(&serde_json::from_str(schema)?)?;
        }
        Ok(())
    }

    #[test]
    fn response_schema_translation_prunes_unreachable_definitions_and_compacts_uniform_tuples()
    -> Result<(), Box<dyn std::error::Error>> {
        let translated = translate_response_json_schema(&json!({
            "type": "object",
            "properties": {
                "action": {"$ref": "#/$defs/inspect"}
            },
            "required": ["action"],
            "$defs": {
                "inspect": {
                    "type": "object",
                    "properties": {
                        "kind": {"const": "inspect"},
                        "gain": {"$ref": "#/$defs/gain"}
                    },
                    "required": ["kind", "gain"],
                    "additionalProperties": false
                },
                "gain": {"type": "integer", "minimum": 0, "maximum": 10000},
                "unused": {"type": "string"}
            }
        }))?;
        let definitions = translated["$defs"]
            .as_object()
            .ok_or("translated schema has no definitions")?;
        assert_eq!(
            definitions.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["gain", "inspect"]
        );

        let translated = translate_response_json_schema(&json!({
            "type": "array",
            "minItems": 2,
            "maxItems": 2,
            "prefixItems": [
                {
                    "type": "object",
                    "properties": {
                        "kind": {"const": "claim"},
                        "claim_id": {"const": "first"}
                    },
                    "required": ["kind", "claim_id"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "kind": {"const": "claim"},
                        "claim_id": {"const": "second"}
                    },
                    "required": ["kind", "claim_id"],
                    "additionalProperties": false
                }
            ],
            "$defs": {
                "unused": {"type": "string"}
            }
        }))?;
        assert!(translated.get("prefixItems").is_none());
        assert!(translated.get("$defs").is_none());
        assert_eq!(
            translated["items"]["properties"]["kind"]["enum"],
            json!(["claim"])
        );
        assert_eq!(
            translated["items"]["properties"]["claim_id"]["enum"],
            json!(["first", "second"])
        );

        let translated = translate_response_json_schema(&json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 3,
            "prefixItems": [{"type": "string"}]
        }))?;
        assert!(translated.get("prefixItems").is_some());
        assert!(translated.get("items").is_none());
        Ok(())
    }

    #[test]
    fn only_explicit_success_finish_reasons_are_accepted() {
        assert_eq!(map_finish_reason("STOP"), Ok(ModelFinishReason::Stop));
        assert_eq!(
            map_finish_reason("MAX_TOKENS"),
            Ok(ModelFinishReason::OutputLimit)
        );
        for rejected in [
            "SAFETY",
            "RECITATION",
            "MALFORMED_FUNCTION_CALL",
            "OTHER",
            "",
        ] {
            assert_eq!(
                map_finish_reason(rejected),
                Err(ModelProviderFailure::Rejected)
            );
        }
    }

    #[test]
    fn only_transient_http_statuses_are_retryable() {
        use reqwest::StatusCode;

        for status in [
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert_eq!(
                classify_http_status(status),
                Some(ModelProviderFailure::Unavailable)
            );
        }
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_IMPLEMENTED,
        ] {
            assert_eq!(
                classify_http_status(status),
                Some(ModelProviderFailure::Rejected)
            );
        }
        assert_eq!(classify_http_status(StatusCode::OK), None);
    }

    #[test]
    fn streamed_error_envelopes_preserve_transient_status_without_exposing_the_message() {
        for code in [408, 429, 500, 502, 503, 504] {
            assert_eq!(
                classify_gemini_error(&GeminiErrorObject {
                    code: Some(code),
                    _message: Some("sensitive provider detail".to_owned()),
                }),
                ModelProviderFailure::Unavailable
            );
        }
        for code in [400, 401, 403, 404, 501] {
            assert_eq!(
                classify_gemini_error(&GeminiErrorObject {
                    code: Some(code),
                    _message: Some("sensitive provider detail".to_owned()),
                }),
                ModelProviderFailure::Rejected
            );
        }
        assert_eq!(
            classify_gemini_error(&GeminiErrorObject {
                code: None,
                _message: Some("sensitive provider detail".to_owned()),
            }),
            ModelProviderFailure::InvalidResponse
        );
    }
}
