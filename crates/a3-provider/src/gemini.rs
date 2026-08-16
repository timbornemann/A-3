use a3_application::{
    ConfiguredModelEndpoint, EmbeddingCapabilityProbe, EmbeddingCapabilityProbeFuture,
    EmbeddingCapabilityProbeRequest, EmbeddingOperationControl, EmbeddingProvider,
    EmbeddingProviderFailure, EmbeddingProviderFuture, EmbeddingRequestTimeout,
    ModelCapabilityObservation, ModelCapabilityProbe, ModelCapabilityProbeFuture,
    ModelCapabilityProbeRequest, ModelCatalogFuture, ModelCatalogProvider, ModelEndpointScope,
    ModelEndpointValidationFailure, ModelEndpointValidator, ModelFinishReason, ModelMessageRole,
    ModelOperationControl, ModelOutputChunk, ModelProvider, ModelProviderCompletion,
    ModelProviderFailure, ModelProviderFuture, ModelProviderRequest, ModelProviderUsage,
    ModelRequestTimeout, ProviderEvent, ProviderEventStream, ProviderModelCatalog,
    RawEmbeddingBatch, ReportedModelContextLimit,
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
use std::collections::VecDeque;
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
const MAX_GEMINI_SHOW_BYTES: usize = 512 * 1024;
const MAX_GEMINI_PROBE_BYTES: usize = 128 * 1024;
const MAX_GEMINI_EMBED_BYTES: usize = 8 * 1024 * 1024;
const MAX_GEMINI_EMBED_PROBE_BYTES: usize = 256 * 1024;
const MAX_GEMINI_MODELS_COUNT: usize = 256;

const GEMINI_PROBE_OUTPUT_TOKENS: u32 = 32;
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

    pub(crate) fn models_url(&self) -> reqwest::Url {
        let mut url = self.url.clone();
        url.set_path("/v1beta/models");
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
        let provider_id = ModelProviderId::try_from_string(GEMINI_PROVIDER_ID.to_owned())
            .map_err(|_| ModelEndpointValidationFailure::ProviderUnavailable)?;
        let scope = match endpoint.scope() {
            GeminiEndpointScope::LocalLoopback => ModelEndpointScope::LocalLoopback,
            GeminiEndpointScope::Remote => ModelEndpointScope::Remote,
        };
        ConfiguredModelEndpoint::from_validated_adapter(
            provider_id,
            endpoint.canonical_origin(),
            scope,
        )
        .map_err(|_| ModelEndpointValidationFailure::Invalid)
    }
}

/// Dynamic authorization checked before every Gemini model request.
pub trait GeminiEndpointPolicy: fmt::Debug + Send + Sync {
    /// Authorizes the exact current endpoint or returns a content-free denial.
    fn authorize(&self, endpoint: &GeminiEndpoint) -> Result<(), GeminiEndpointPolicyError>;
}

/// Standard policy allowing local loopback and the canonical Google Gemini HTTPS origin.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardGeminiEndpointPolicy;

impl GeminiEndpointPolicy for StandardGeminiEndpointPolicy {
    fn authorize(&self, endpoint: &GeminiEndpoint) -> Result<(), GeminiEndpointPolicyError> {
        if endpoint.scope() == GeminiEndpointScope::LocalLoopback
            || endpoint.canonical_origin() == DEFAULT_GEMINI_ORIGIN
            || (endpoint.scope() == GeminiEndpointScope::Remote && endpoint.url.scheme() == "https")
        {
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
    api_key: Option<String>,
}

impl GeminiModelProvider {
    /// Creates a Gemini provider for one validated endpoint, loading API key from ambient environment.
    pub fn new(
        endpoint: GeminiEndpoint,
        endpoint_policy: Arc<dyn GeminiEndpointPolicy>,
    ) -> Result<Self, GeminiProviderCreateError> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .ok()
            .filter(|key| !key.trim().is_empty());
        Self::with_api_key(endpoint, endpoint_policy, api_key)
    }

    /// Creates a Gemini provider with an explicit API key (or None).
    pub fn with_api_key(
        endpoint: GeminiEndpoint,
        endpoint_policy: Arc<dyn GeminiEndpointPolicy>,
        api_key: Option<String>,
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

    fn attach_auth(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.api_key {
            request = request.header(API_KEY_HEADER, key.as_str());
        }
        request
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
            .field("has_api_key", &self.api_key.is_some())
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
            let wire_request = GeminiGenerateContentRequest::from_request(request);
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
            let request = self.attach_auth(self.client.get(self.endpoint.models_url()));
            let response = send_before_deadline(request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body = read_bounded_response(response, MAX_GEMINI_MODELS_BYTES, control).await?;
            parse_gemini_models(&body, self.provider_id.clone())
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
            let tool_call_mode = if show.generate_content_supported {
                ModelToolCallMode::NativeProviderReported
            } else {
                ModelToolCallMode::Disabled
            };
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
            if batch.len() != cards.len() {
                return Err(EmbeddingProviderFailure::InvalidResponse);
            }
            Ok(batch)
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
            "type": "OBJECT",
            "properties": {
                "a3_probe": {
                    "type": "STRING"
                }
            },
            "required": ["a3_probe"]
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
                response_schema: Some(&probe_schema),
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
    fn from_request(request: &'a ModelProviderRequest) -> Self {
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

        let (response_mime_type, response_schema) = match request.structured_output() {
            Some(schema) => (Some("application/json"), Some(schema.value())),
            None => (None, None),
        };

        let generation_config = Some(GeminiGenerationConfig {
            temperature: Some(f32::from(settings.sampling().temperature().milli()) / 1_000.0),
            top_p: Some(f32::from(settings.sampling().top_p().milli()) / 1_000.0),
            max_output_tokens: Some(settings.output_limit().get()),
            stop_sequences,
            response_mime_type,
            response_schema,
        });

        Self {
            contents: Some(user_contents),
            system_instruction,
            generation_config,
        }
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
    #[serde(skip_serializing_if = "Option::is_none", rename = "responseSchema")]
    response_schema: Option<&'a Value>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
    error: Option<GeminiErrorObject>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
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
    _code: Option<i64>,
    #[serde(rename = "message")]
    _message: Option<String>,
}

#[derive(Deserialize)]
struct GeminiListModelsResponse {
    models: Option<Vec<GeminiModelMetadata>>,
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
    generate_content_supported: bool,
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
    if json_data == "[DONE]" {
        return Ok(());
    }
    let response: GeminiResponse =
        serde_json::from_str(json_data).map_err(|_| ModelProviderFailure::InvalidResponse)?;

    if response.error.is_some() {
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
        for candidate in candidates {
            if let Some(reason) = candidate.finish_reason {
                state.finish_reason = Some(map_finish_reason(&reason));
            }
            if let Some(parts) = candidate.content.and_then(|content| content.parts) {
                for part in parts {
                    if let Some(text) = part.text.filter(|t| !t.is_empty()) {
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
    let usage = ModelProviderUsage::new(state.prompt_tokens, state.candidates_tokens);
    let finish_reason = state.finish_reason.unwrap_or(ModelFinishReason::Stop);
    let completion = ModelProviderCompletion::new(finish_reason, usage);
    state.queued.push_back(ProviderEvent::Completed(completion));
    state.body_ended = true;
    Ok(())
}

fn map_finish_reason(reason: &str) -> ModelFinishReason {
    match reason.to_ascii_uppercase().as_str() {
        "MAX_TOKENS" => ModelFinishReason::OutputLimit,
        _ => ModelFinishReason::Stop,
    }
}

fn validate_json_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::FORBIDDEN
            || response.status() == reqwest::StatusCode::UNAUTHORIZED
        {
            return Err(ModelProviderFailure::Rejected);
        }
        return Err(ModelProviderFailure::Unavailable);
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
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::FORBIDDEN
            || response.status() == reqwest::StatusCode::UNAUTHORIZED
        {
            return Err(ModelProviderFailure::Rejected);
        }
        return Err(ModelProviderFailure::Unavailable);
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

fn parse_gemini_models(
    body: &[u8],
    provider_id: ModelProviderId,
) -> Result<ProviderModelCatalog, ModelProviderFailure> {
    let response = serde_json::from_slice::<GeminiListModelsResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let mut models = Vec::new();
    if let Some(list) = response.models {
        for item in list {
            let name = normalize_model_path(&item.name);
            if let Ok(model_id) = ModelId::try_from_string(name.to_owned()) {
                models.push(model_id);
            }
        }
    }
    models.sort();
    models.dedup();
    let truncated = models.len() > MAX_GEMINI_MODELS_COUNT;
    models.truncate(MAX_GEMINI_MODELS_COUNT);
    Ok(ProviderModelCatalog::from_observation(
        provider_id,
        models,
        truncated,
    ))
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
    let generate_content_supported =
        metadata
            .supported_generation_methods
            .as_ref()
            .is_none_or(|methods| {
                methods
                    .iter()
                    .any(|method| method == "generateContent" || method == "streamGenerateContent")
            });
    Ok(GeminiShowObservation {
        context_limit,
        generate_content_supported,
    })
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
                if let Some(text) = part.text {
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
        LocalOnlyGeminiEndpointPolicy, StandardGeminiEndpointPolicy,
    };

    #[test]
    fn localhost_normalizes_and_remote_requires_https() -> Result<(), Box<dyn std::error::Error>> {
        let local = GeminiEndpoint::parse("http://localhost:8080")?;
        assert_eq!(local.scope(), GeminiEndpointScope::LocalLoopback);
        assert_eq!(local.canonical_origin(), "http://127.0.0.1:8080");
        assert!(StandardGeminiEndpointPolicy.authorize(&local).is_ok());
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
}
