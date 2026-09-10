use crate::openai::translate_openai_json_schema;
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
use std::collections::{BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

const PROVIDER_ID: &str = "openai-compatible";
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const USER_AGENT: &str = "a3/0.1.0";
const JSON_CONTENT_TYPE: &str = "application/json";
const EVENT_STREAM_CONTENT_TYPE: &str = "text/event-stream";
const PROBE_PROMPT: &str =
    "Return exactly this JSON object and nothing else: {\"a3_probe\":\"ok\"}.";
const EMBED_PROBE_INPUT: &str = "A3 embedding capability probe";
const SCHEMA_NAME: &str = "a3_response";
const PROBE_SCHEMA_NAME: &str = "a3_capability_probe";
const MAX_MODELS_BYTES: usize = 2 * 1024 * 1024;
const MAX_MODELS_OBSERVED: usize = 1_000;
const MAX_MODELS_COUNT: usize = 256;
const MAX_PROBE_BYTES: usize = 256 * 1024;
const MAX_EMBED_PROBE_BYTES: usize = 256 * 1024;
const MAX_EMBED_BYTES: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_SSE_LINE_BYTES: usize = 256 * 1024;
const MAX_SSE_BUFFER_BYTES: usize = MAX_SSE_LINE_BYTES + 64 * 1024;
const MAX_SSE_WIRE_BYTES: usize = 8 * 1024 * 1024;

/// Whether a compatible endpoint is loopback-only or remote HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenAiCompatibleEndpointScope {
    /// Literal IPv4 or IPv6 loopback, used only by deterministic adapter tests.
    LocalLoopback,
    /// User-configured remote HTTPS base URL.
    Remote,
}

/// Validated base URL for an OpenAI Chat-Completions-compatible API.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleEndpoint {
    url: reqwest::Url,
    scope: OpenAiCompatibleEndpointScope,
}

impl OpenAiCompatibleEndpoint {
    /// Returns the user-editable default based on OpenRouter's documented API base URL.
    pub fn default_base_url() -> Result<Self, OpenAiCompatibleEndpointError> {
        Self::parse(DEFAULT_BASE_URL)
    }

    /// Parses a credential-free URL with an optional safe API path prefix.
    pub fn parse(value: &str) -> Result<Self, OpenAiCompatibleEndpointError> {
        if value.contains(['%', '\\']) {
            return Err(OpenAiCompatibleEndpointError::UnsafeBasePath);
        }
        let mut url =
            reqwest::Url::parse(value).map_err(|_| OpenAiCompatibleEndpointError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(OpenAiCompatibleEndpointError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OpenAiCompatibleEndpointError::CredentialsForbidden);
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(OpenAiCompatibleEndpointError::QueryOrFragmentForbidden);
        }
        let host = url
            .host_str()
            .ok_or(OpenAiCompatibleEndpointError::MissingHost)?;
        if host.eq_ignore_ascii_case("localhost") {
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| OpenAiCompatibleEndpointError::InvalidUrl)?;
        }
        validate_base_path(url.path())?;
        let scope = url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .map_or(OpenAiCompatibleEndpointScope::Remote, |address| {
                if address.is_loopback() {
                    OpenAiCompatibleEndpointScope::LocalLoopback
                } else {
                    OpenAiCompatibleEndpointScope::Remote
                }
            });
        if scope == OpenAiCompatibleEndpointScope::Remote && url.scheme() != "https" {
            return Err(OpenAiCompatibleEndpointError::InsecureRemote);
        }
        if url.path() != "/" {
            let trimmed = url.path().trim_end_matches('/').to_owned();
            url.set_path(&trimmed);
        }
        Ok(Self { url, scope })
    }

    /// Returns whether this endpoint stays on loopback or reaches a remote HTTPS service.
    #[must_use]
    pub const fn scope(&self) -> OpenAiCompatibleEndpointScope {
        self.scope
    }

    /// Returns the normalized credential-free base URL including its API path prefix.
    #[must_use]
    pub fn canonical_base_url(&self) -> String {
        self.url.as_str().trim_end_matches('/').to_owned()
    }

    fn models_url(&self) -> Result<reqwest::Url, ModelProviderFailure> {
        self.resource_url("models")
    }

    fn chat_completions_url(&self) -> Result<reqwest::Url, ModelProviderFailure> {
        self.resource_url("chat/completions")
    }

    fn embeddings_url(&self) -> Result<reqwest::Url, ModelProviderFailure> {
        self.resource_url("embeddings")
    }

    fn resource_url(&self, resource: &str) -> Result<reqwest::Url, ModelProviderFailure> {
        reqwest::Url::parse(&format!("{}/{resource}", self.canonical_base_url()))
            .map_err(|_| ModelProviderFailure::Rejected)
    }
}

impl fmt::Debug for OpenAiCompatibleEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleEndpoint")
            .field("scheme", &self.url.scheme())
            .field("scope", &self.scope)
            .field("port", &self.url.port_or_known_default())
            .field(
                "path_segments",
                &self.url.path_segments().map(Iterator::count),
            )
            .finish()
    }
}

fn validate_base_path(path: &str) -> Result<(), OpenAiCompatibleEndpointError> {
    if path == "/" || path.is_empty() {
        return Ok(());
    }
    let path = path.trim_matches('/');
    if path.is_empty()
        || path.contains('%')
        || path.contains('\\')
        || path.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || !segment.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
                })
        })
    {
        Err(OpenAiCompatibleEndpointError::UnsafeBasePath)
    } else {
        Ok(())
    }
}

/// Invalid or unsafe OpenAI-compatible endpoint configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiCompatibleEndpointError {
    /// Input was not an absolute URL.
    InvalidUrl,
    /// Only HTTP and HTTPS are supported.
    UnsupportedScheme,
    /// URL did not contain a host.
    MissingHost,
    /// Userinfo could expose credentials through configuration or logs.
    CredentialsForbidden,
    /// Query strings and fragments are not part of a stable API base URL.
    QueryOrFragmentForbidden,
    /// The optional API path prefix was encoded, traversing, or otherwise unsafe.
    UnsafeBasePath,
    /// Non-loopback compatible endpoints require HTTPS.
    InsecureRemote,
}

impl fmt::Display for OpenAiCompatibleEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUrl => "openai-compatible endpoint is not a valid absolute URL",
            Self::UnsupportedScheme => "openai-compatible endpoint scheme is unsupported",
            Self::MissingHost => "openai-compatible endpoint has no host",
            Self::CredentialsForbidden => "openai-compatible endpoint must not contain credentials",
            Self::QueryOrFragmentForbidden => {
                "openai-compatible endpoint must not contain a query or fragment"
            }
            Self::UnsafeBasePath => "openai-compatible endpoint base path is unsafe",
            Self::InsecureRemote => "remote openai-compatible endpoint must use HTTPS",
        })
    }
}

impl Error for OpenAiCompatibleEndpointError {}

/// Settings validator for remote OpenAI-compatible HTTPS base URLs.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenAiCompatibleSettingsEndpointValidator;

impl ModelEndpointValidator for OpenAiCompatibleSettingsEndpointValidator {
    fn validate(
        &self,
        input: &str,
    ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure> {
        let endpoint = OpenAiCompatibleEndpoint::parse(input)
            .map_err(|_| ModelEndpointValidationFailure::Invalid)?;
        if endpoint.scope() != OpenAiCompatibleEndpointScope::Remote {
            return Err(ModelEndpointValidationFailure::Invalid);
        }
        let provider_id = ModelProviderId::try_from_string(PROVIDER_ID.to_owned())
            .map_err(|_| ModelEndpointValidationFailure::ProviderUnavailable)?;
        ConfiguredModelEndpoint::from_validated_adapter_with_security(
            provider_id,
            endpoint.canonical_base_url(),
            ModelEndpointScope::Remote,
            ModelEndpointAccess::ExplicitUserInitiatedRemote,
            ProviderCredentialRequirement::ApiKey,
        )
        .map_err(|_| ModelEndpointValidationFailure::Invalid)
    }
}

/// Dynamic authorization checked before each compatible-provider request.
pub trait OpenAiCompatibleEndpointPolicy: fmt::Debug + Send + Sync {
    /// Authorizes the exact current endpoint or returns a content-free denial.
    fn authorize(
        &self,
        endpoint: &OpenAiCompatibleEndpoint,
    ) -> Result<(), OpenAiCompatibleEndpointPolicyError>;
}

/// Exact-base-URL policy installed after native settings confirmation.
#[derive(Debug, Clone)]
pub struct ExactOpenAiCompatibleEndpointPolicy {
    base_url: String,
}

impl ExactOpenAiCompatibleEndpointPolicy {
    /// Binds all requests to one canonical base URL.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

impl OpenAiCompatibleEndpointPolicy for ExactOpenAiCompatibleEndpointPolicy {
    fn authorize(
        &self,
        endpoint: &OpenAiCompatibleEndpoint,
    ) -> Result<(), OpenAiCompatibleEndpointPolicyError> {
        if endpoint.canonical_base_url() == self.base_url {
            Ok(())
        } else {
            Err(OpenAiCompatibleEndpointPolicyError::Denied)
        }
    }
}

/// Test-only policy allowing loopback endpoints and denying every remote endpoint.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalOnlyOpenAiCompatibleEndpointPolicy;

impl OpenAiCompatibleEndpointPolicy for LocalOnlyOpenAiCompatibleEndpointPolicy {
    fn authorize(
        &self,
        endpoint: &OpenAiCompatibleEndpoint,
    ) -> Result<(), OpenAiCompatibleEndpointPolicyError> {
        if endpoint.scope() == OpenAiCompatibleEndpointScope::LocalLoopback {
            Ok(())
        } else {
            Err(OpenAiCompatibleEndpointPolicyError::Denied)
        }
    }
}

/// Endpoint was not authorized by the injected compatible-provider policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiCompatibleEndpointPolicyError {
    /// Exact configured endpoint is outside the active allowlist.
    Denied,
}

impl fmt::Display for OpenAiCompatibleEndpointPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("openai-compatible endpoint is not authorized by current policy")
    }
}

impl Error for OpenAiCompatibleEndpointPolicyError {}

/// OpenAI Chat-Completions-compatible model and embedding adapter.
pub struct OpenAiCompatibleModelProvider {
    provider_id: ModelProviderId,
    embedding_provider_id: EmbeddingProviderId,
    endpoint: OpenAiCompatibleEndpoint,
    endpoint_policy: Arc<dyn OpenAiCompatibleEndpointPolicy>,
    client: reqwest::Client,
    api_key: ProviderApiKey,
}

impl OpenAiCompatibleModelProvider {
    /// Creates a provider bound to one validated base URL and short-lived API key.
    pub fn new(
        endpoint: OpenAiCompatibleEndpoint,
        endpoint_policy: Arc<dyn OpenAiCompatibleEndpointPolicy>,
        api_key: ProviderApiKey,
    ) -> Result<Self, OpenAiCompatibleProviderCreateError> {
        let provider_id = ModelProviderId::try_from_string(PROVIDER_ID.to_owned())
            .map_err(|_| OpenAiCompatibleProviderCreateError::InvalidProviderIdentity)?;
        let embedding_provider_id = EmbeddingProviderId::new(PROVIDER_ID.to_owned())
            .map_err(|_| OpenAiCompatibleProviderCreateError::InvalidProviderIdentity)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| OpenAiCompatibleProviderCreateError::HttpClient)?;
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

    fn attach_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ModelProviderFailure> {
        let mut bytes = Vec::with_capacity(7 + self.api_key.as_bytes().len());
        bytes.extend_from_slice(b"Bearer ");
        bytes.extend_from_slice(self.api_key.as_bytes());
        let header_result = reqwest::header::HeaderValue::from_bytes(&bytes);
        bytes.fill(0);
        let mut header = header_result.map_err(|_| ModelProviderFailure::Rejected)?;
        header.set_sensitive(true);
        Ok(request
            .header(reqwest::header::AUTHORIZATION, header)
            .header(reqwest::header::USER_AGENT, USER_AGENT))
    }

    async fn probe_structured_output(
        &self,
        request: &ModelCapabilityProbeRequest,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<ModelStructuredOutputCapability, ModelProviderFailure> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"a3_probe": {"type": "string", "enum": ["ok"]}},
            "required": ["a3_probe"],
            "additionalProperties": false
        });
        let wire = ChatRequest::probe(request.model_id().as_str(), &schema)?;
        let http_request = self.attach_auth(
            self.client
                .post(self.endpoint.chat_completions_url()?)
                .json(&wire),
        )?;
        let response = send_before_deadline(http_request, deadline, control).await?;
        let status = response.status();
        if let Err(error) = validate_json_response_head(&response) {
            return if status.is_success()
                || matches!(
                    status,
                    reqwest::StatusCode::BAD_REQUEST
                        | reqwest::StatusCode::NOT_FOUND
                        | reqwest::StatusCode::UNPROCESSABLE_ENTITY
                        | reqwest::StatusCode::NOT_IMPLEMENTED
                ) {
                Ok(ModelStructuredOutputCapability::Unavailable)
            } else {
                Err(error)
            };
        }
        let body = match read_bounded_response(response, MAX_PROBE_BYTES, control).await {
            Ok(body) => body,
            Err(ModelProviderFailure::InvalidResponse) => {
                return Ok(ModelStructuredOutputCapability::Unavailable);
            }
            Err(other) => return Err(other),
        };
        if parse_probe_response(&body, request.model_id().as_str()).unwrap_or(false) {
            Ok(ModelStructuredOutputCapability::Verified)
        } else {
            Ok(ModelStructuredOutputCapability::Unavailable)
        }
    }
}

impl fmt::Debug for OpenAiCompatibleModelProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleModelProvider")
            .field("provider_id", &self.provider_id)
            .field("embedding_provider_id", &self.embedding_provider_id)
            .field("endpoint", &self.endpoint)
            .field("endpoint_policy", &self.endpoint_policy)
            .field("has_api_key", &true)
            .finish()
    }
}

/// Failure creating an OpenAI-compatible provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiCompatibleProviderCreateError {
    /// Provider identity failed domain validation.
    InvalidProviderIdentity,
    /// HTTP client initialization failed.
    HttpClient,
}

impl fmt::Display for OpenAiCompatibleProviderCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProviderIdentity => "openai-compatible provider identity is invalid",
            Self::HttpClient => "failed to initialize HTTP client for openai-compatible provider",
        })
    }
}

impl Error for OpenAiCompatibleProviderCreateError {}

impl ModelProvider for OpenAiCompatibleModelProvider {
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
            let wire = ChatRequest::from_request(request, true)?;
            let http_request = self.attach_auth(
                self.client
                    .post(self.endpoint.chat_completions_url()?)
                    .timeout(timeout.duration())
                    .json(&wire),
            )?;
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
            let state = CompatibleStreamState::new(body, control, request.model_id().as_str());
            let stream = futures::stream::try_unfold(state, next_compatible_event);
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

impl ModelCatalogProvider for OpenAiCompatibleModelProvider {
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
            let request = self.attach_auth(self.client.get(self.endpoint.models_url()?))?;
            let response = send_before_deadline(request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body = read_bounded_response(response, MAX_MODELS_BYTES, control).await?;
            let (models, truncated) = parse_model_catalog(&body)?;
            Ok(ProviderModelCatalog::from_observation(
                self.provider_id.clone(),
                models,
                truncated,
            ))
        })
    }
}

impl ModelCapabilityProbe for OpenAiCompatibleModelProvider {
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
            let structured_output = self
                .probe_structured_output(request, deadline, control)
                .await?;
            Ok(ModelCapabilityObservation::new(
                None,
                ModelCapabilities::new(structured_output, ModelToolCallMode::Disabled),
            ))
        })
    }
}

impl EmbeddingCapabilityProbe for OpenAiCompatibleModelProvider {
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
            let input = [EMBED_PROBE_INPUT];
            let wire = EmbeddingRequest {
                model: request.model_id().as_str(),
                input: &input,
                encoding_format: "float",
            };
            let http_request = self.attach_auth(
                self.client
                    .post(self.endpoint.embeddings_url()?)
                    .json(&wire),
            )?;
            let response = send_before_deadline(http_request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body = read_bounded_response(response, MAX_EMBED_PROBE_BYTES, control).await?;
            let vectors = parse_embedding_response(&body, request.model_id().as_str(), 1)
                .map_err(map_embedding_to_model_failure)?;
            embedding_probe_dimension(vectors)
        })
    }
}

impl EmbeddingProvider for OpenAiCompatibleModelProvider {
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
            let input = cards
                .iter()
                .map(NormalizedSemanticCard::body)
                .collect::<Vec<_>>();
            let wire = EmbeddingRequest {
                model: profile.model_id().as_str(),
                input: &input,
                encoding_format: "float",
            };
            let request = self
                .attach_auth(
                    self.client
                        .post(
                            self.endpoint
                                .embeddings_url()
                                .map_err(map_model_to_embedding_failure)?,
                        )
                        .timeout(timeout.duration())
                        .json(&wire),
                )
                .map_err(map_model_to_embedding_failure)?;
            let response = request
                .send()
                .await
                .map_err(classify_embedding_reqwest_error)?;
            if control.is_cancelled() {
                return Err(EmbeddingProviderFailure::Cancelled);
            }
            validate_json_response_head(&response).map_err(map_model_to_embedding_failure)?;
            let body = read_bounded_embedding_response(response, MAX_EMBED_BYTES, control).await?;
            let vectors =
                parse_embedding_response(&body, profile.model_id().as_str(), cards.len())?;
            let expected_dimension = usize::from(profile.dimension().get());
            if vectors.iter().any(|vector| {
                vector.len() != expected_dimension
                    || vector.iter().any(|component| !component.is_finite())
                    || vector.iter().all(|component| *component == 0.0)
            }) {
                return Err(EmbeddingProviderFailure::InvalidResponse);
            }
            RawEmbeddingBatch::new(vectors).map_err(|_| EmbeddingProviderFailure::InvalidResponse)
        })
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
    max_tokens: u32,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ChatResponseFormat>,
}

impl<'a> ChatRequest<'a> {
    fn from_request(
        request: &'a ModelProviderRequest,
        stream: bool,
    ) -> Result<Self, ModelProviderFailure> {
        let messages = request
            .messages()
            .iter()
            .map(|message| ChatMessage {
                role: match message.role() {
                    ModelMessageRole::System => "system",
                    ModelMessageRole::User => "user",
                    ModelMessageRole::Assistant => "assistant",
                },
                content: message.content(),
            })
            .collect();
        let response_format = request
            .structured_output()
            .map(|schema| response_format(SCHEMA_NAME, schema.value()))
            .transpose()?;
        let stop = request.profile().settings().stop_sequences().as_slice();
        let stop = (!stop.is_empty()).then(|| stop.iter().map(|value| value.as_str()).collect());
        let sampling = request.profile().settings().sampling();
        Ok(Self {
            model: request.model_id().as_str(),
            messages,
            stream,
            max_tokens: request.profile().settings().output_limit().get(),
            temperature: f64::from(sampling.temperature().milli()) / 1_000.0,
            top_p: f64::from(sampling.top_p().milli()) / 1_000.0,
            stop,
            response_format,
        })
    }

    fn probe(model: &'a str, schema: &Value) -> Result<Self, ModelProviderFailure> {
        Ok(Self {
            model,
            messages: vec![ChatMessage {
                role: "user",
                content: PROBE_PROMPT,
            }],
            stream: false,
            max_tokens: 256,
            temperature: 0.0,
            top_p: 1.0,
            stop: None,
            response_format: Some(response_format(PROBE_SCHEMA_NAME, schema)?),
        })
    }
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
    json_schema: ChatJsonSchema,
}

#[derive(Serialize)]
struct ChatJsonSchema {
    name: &'static str,
    strict: bool,
    schema: Value,
}

fn response_format(
    name: &'static str,
    schema: &Value,
) -> Result<ChatResponseFormat, ModelProviderFailure> {
    Ok(ChatResponseFormat {
        format_type: "json_schema",
        json_schema: ChatJsonSchema {
            name,
            strict: true,
            schema: translate_openai_json_schema(schema)?,
        },
    })
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
    encoding_format: &'static str,
}

#[derive(Deserialize)]
struct ModelsResponse {
    object: Option<String>,
    data: Vec<ModelRecord>,
}

#[derive(Deserialize)]
struct ModelRecord {
    id: String,
    object: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    object: String,
    model: String,
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    index: usize,
    message: ChatResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    role: String,
    content: Option<String>,
    refusal: Option<Value>,
    tool_calls: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct ChatCompletionChunk {
    object: String,
    model: String,
    choices: Vec<ChatChunkChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChunkChoice {
    index: usize,
    delta: ChatDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatDelta {
    role: Option<String>,
    content: Option<String>,
    refusal: Option<Value>,
    tool_calls: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    object: String,
    model: String,
    data: Vec<EmbeddingRecord>,
}

#[derive(Deserialize)]
struct EmbeddingRecord {
    object: String,
    embedding: Vec<f32>,
    index: usize,
}

type CompatibleByteStream = BoxStream<'static, Result<Vec<u8>, reqwest::Error>>;

struct CompatibleStreamState<'a> {
    body: CompatibleByteStream,
    control: &'a dyn ModelOperationControl,
    requested_model: &'a str,
    buffer: Vec<u8>,
    queued: VecDeque<ProviderEvent>,
    output_bytes: usize,
    wire_bytes: usize,
    finish_reason: Option<ModelFinishReason>,
    usage: ModelProviderUsage,
    done: bool,
}

impl<'a> CompatibleStreamState<'a> {
    fn new(
        body: CompatibleByteStream,
        control: &'a dyn ModelOperationControl,
        requested_model: &'a str,
    ) -> Self {
        Self {
            body,
            control,
            requested_model,
            buffer: Vec::new(),
            queued: VecDeque::new(),
            output_bytes: 0,
            wire_bytes: 0,
            finish_reason: None,
            usage: ModelProviderUsage::new(None, None),
            done: false,
        }
    }
}

async fn next_compatible_event(
    mut state: CompatibleStreamState<'_>,
) -> Result<Option<(ProviderEvent, CompatibleStreamState<'_>)>, ModelProviderFailure> {
    loop {
        if let Some(event) = state.queued.pop_front() {
            return Ok(Some((event, state)));
        }
        if state.done {
            return Ok(None);
        }
        if let Some(line) = take_complete_line(&mut state.buffer)? {
            parse_sse_line(&mut state, &line)?;
            continue;
        }
        let chunk = read_stream_chunk(&mut state).await?;
        let Some(chunk) = chunk else {
            return Err(ModelProviderFailure::Unavailable);
        };
        state.wire_bytes = state.wire_bytes.saturating_add(chunk.len());
        if state.wire_bytes > MAX_SSE_WIRE_BYTES
            || state.buffer.len().saturating_add(chunk.len()) > MAX_SSE_BUFFER_BYTES
        {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        state.buffer.extend_from_slice(&chunk);
    }
}

async fn read_stream_chunk(
    state: &mut CompatibleStreamState<'_>,
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

fn take_complete_line(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, ModelProviderFailure> {
    let Some(position) = buffer.iter().position(|byte| *byte == b'\n') else {
        if buffer.len() > MAX_SSE_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        return Ok(None);
    };
    if position > MAX_SSE_LINE_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut line = buffer.drain(..=position).collect::<Vec<_>>();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn parse_sse_line(
    state: &mut CompatibleStreamState<'_>,
    line: &[u8],
) -> Result<(), ModelProviderFailure> {
    let line = std::str::from_utf8(line).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
        return Ok(());
    }
    let data = line
        .strip_prefix("data:")
        .map(str::trim_start)
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if data == "[DONE]" {
        let reason = state
            .finish_reason
            .ok_or(ModelProviderFailure::InvalidResponse)?;
        state
            .queued
            .push_back(ProviderEvent::Completed(ModelProviderCompletion::new(
                reason,
                state.usage,
            )));
        state.done = true;
        return Ok(());
    }
    if state.finish_reason.is_some() {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let chunk: ChatCompletionChunk =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if chunk.object != "chat.completion.chunk" || chunk.model != state.requested_model {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if let Some(usage) = chunk.usage {
        state.usage = ModelProviderUsage::new(usage.prompt_tokens, usage.completion_tokens);
    }
    if chunk.choices.is_empty() {
        return Ok(());
    }
    if chunk.choices.len() != 1 {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let choice = chunk
        .choices
        .into_iter()
        .next()
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if choice.index != 0
        || choice
            .delta
            .role
            .as_deref()
            .is_some_and(|role| role != "assistant")
        || choice.delta.refusal.is_some_and(|value| !value.is_null())
        || choice
            .delta
            .tool_calls
            .is_some_and(|calls| !calls.is_empty())
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if let Some(content) = choice.delta.content {
        queue_output(state, content)?;
    }
    if let Some(reason) = choice.finish_reason {
        state.finish_reason = Some(parse_finish_reason(&reason)?);
    }
    Ok(())
}

fn queue_output(
    state: &mut CompatibleStreamState<'_>,
    content: String,
) -> Result<(), ModelProviderFailure> {
    if content.is_empty() {
        return Ok(());
    }
    state.output_bytes = state.output_bytes.saturating_add(content.len());
    if state.output_bytes > MAX_OUTPUT_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut start = 0;
    while start < content.len() {
        let mut end = (start + 64 * 1024).min(content.len());
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = ModelOutputChunk::try_from_string(content[start..end].to_owned())
            .map_err(|_| ModelProviderFailure::InvalidResponse)?;
        state.queued.push_back(ProviderEvent::OutputText(chunk));
        start = end;
    }
    Ok(())
}

fn parse_finish_reason(reason: &str) -> Result<ModelFinishReason, ModelProviderFailure> {
    match reason {
        "stop" => Ok(ModelFinishReason::Stop),
        "length" => Ok(ModelFinishReason::OutputLimit),
        "content_filter" => Ok(ModelFinishReason::Other),
        "tool_calls" | "function_call" => Err(ModelProviderFailure::Rejected),
        _ => Err(ModelProviderFailure::InvalidResponse),
    }
}

fn parse_model_catalog(body: &[u8]) -> Result<(Vec<ModelId>, bool), ModelProviderFailure> {
    let response = serde_json::from_slice::<ModelsResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if response
        .object
        .as_deref()
        .is_some_and(|object| object != "list")
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut models = BTreeSet::new();
    let mut truncated = response.data.len() > MAX_MODELS_OBSERVED;
    for record in response.data.into_iter().take(MAX_MODELS_OBSERVED) {
        if record
            .object
            .as_deref()
            .is_some_and(|object| object != "model")
        {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        if let Ok(model) = ModelId::try_from_string(record.id) {
            models.insert(model);
            if models.len() > MAX_MODELS_COUNT {
                truncated = true;
                break;
            }
        }
    }
    let mut models = models.into_iter().collect::<Vec<_>>();
    models.truncate(MAX_MODELS_COUNT);
    Ok((models, truncated))
}

fn parse_probe_response(body: &[u8], requested_model: &str) -> Result<bool, ModelProviderFailure> {
    let response = serde_json::from_slice::<ChatCompletionResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if response.object != "chat.completion"
        || response.model != requested_model
        || response.choices.len() != 1
    {
        return Ok(false);
    }
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if choice.index != 0
        || choice.message.role != "assistant"
        || choice.finish_reason.as_deref() != Some("stop")
        || choice.message.refusal.is_some_and(|value| !value.is_null())
        || choice
            .message
            .tool_calls
            .is_some_and(|calls| !calls.is_empty())
    {
        return Ok(false);
    }
    let Some(content) = choice.message.content else {
        return Ok(false);
    };
    let value: Value =
        serde_json::from_str(&content).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    Ok(value == serde_json::json!({"a3_probe": "ok"}))
}

fn parse_embedding_response(
    body: &[u8],
    requested_model: &str,
    expected_count: usize,
) -> Result<Vec<Vec<f32>>, EmbeddingProviderFailure> {
    let response = serde_json::from_slice::<EmbeddingResponse>(body)
        .map_err(|_| EmbeddingProviderFailure::InvalidResponse)?;
    if response.object != "list"
        || response.model != requested_model
        || response.data.len() != expected_count
    {
        return Err(EmbeddingProviderFailure::InvalidResponse);
    }
    let mut vectors = vec![None; expected_count];
    for record in response.data {
        if record.object != "embedding"
            || record.index >= expected_count
            || vectors[record.index].replace(record.embedding).is_some()
        {
            return Err(EmbeddingProviderFailure::InvalidResponse);
        }
    }
    vectors
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EmbeddingProviderFailure::InvalidResponse)
}

fn embedding_probe_dimension(
    mut vectors: Vec<Vec<f32>>,
) -> Result<EmbeddingDimension, ModelProviderFailure> {
    let vector = vectors.pop().ok_or(ModelProviderFailure::InvalidResponse)?;
    if !vectors.is_empty()
        || vector.is_empty()
        || vector.iter().any(|component| !component.is_finite())
        || vector.iter().all(|component| *component == 0.0)
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let dimension =
        u16::try_from(vector.len()).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    EmbeddingDimension::new(dimension).map_err(|_| ModelProviderFailure::InvalidResponse)
}

fn validate_json_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    validate_response_head(response, JSON_CONTENT_TYPE)
}

fn validate_stream_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    validate_response_head(response, EVENT_STREAM_CONTENT_TYPE)
}

fn validate_response_head(
    response: &reqwest::Response,
    expected_content_type: &str,
) -> Result<(), ModelProviderFailure> {
    if let Some(failure) = classify_http_status(response.status()) {
        return Err(failure);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if matches!(content_type, Some(value) if value.eq_ignore_ascii_case(expected_content_type)) {
        Ok(())
    } else {
        Err(ModelProviderFailure::InvalidResponse)
    }
}

fn classify_http_status(status: reqwest::StatusCode) -> Option<ModelProviderFailure> {
    if status.is_success() {
        None
    } else if status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != reqwest::StatusCode::NOT_IMPLEMENTED)
    {
        Some(ModelProviderFailure::Unavailable)
    } else {
        Some(ModelProviderFailure::Rejected)
    }
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

fn map_model_to_embedding_failure(error: ModelProviderFailure) -> EmbeddingProviderFailure {
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

fn map_embedding_to_model_failure(error: EmbeddingProviderFailure) -> ModelProviderFailure {
    match error {
        EmbeddingProviderFailure::Unavailable => ModelProviderFailure::Unavailable,
        EmbeddingProviderFailure::Rejected => ModelProviderFailure::Rejected,
        EmbeddingProviderFailure::InvalidResponse => ModelProviderFailure::InvalidResponse,
        EmbeddingProviderFailure::TimedOut => ModelProviderFailure::TimedOut,
        EmbeddingProviderFailure::Cancelled => ModelProviderFailure::Cancelled,
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
    loop {
        let read = body.next().fuse();
        let cancelled = control.cancelled().fuse();
        pin_mut!(read, cancelled);
        let chunk = match select(cancelled, read).await {
            Either::Left(((), _)) => return Err(ModelProviderFailure::Cancelled),
            Either::Right((Some(result), _)) => result.map_err(classify_reqwest_error)?,
            Either::Right((None, _)) => break,
        };
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

#[cfg(test)]
mod tests {
    use super::{
        ExactOpenAiCompatibleEndpointPolicy, LocalOnlyOpenAiCompatibleEndpointPolicy,
        OpenAiCompatibleEndpoint, OpenAiCompatibleEndpointPolicy, OpenAiCompatibleEndpointScope,
        OpenAiCompatibleSettingsEndpointValidator, parse_model_catalog, parse_probe_response,
    };
    use a3_application::{ModelEndpointScope, ModelEndpointValidator};
    use serde_json::json;

    #[test]
    fn endpoint_accepts_safe_https_base_paths_and_rejects_unsafe_urls()
    -> Result<(), Box<dyn std::error::Error>> {
        let endpoint = OpenAiCompatibleEndpoint::parse("https://openrouter.ai/api/v1/")?;
        assert_eq!(endpoint.scope(), OpenAiCompatibleEndpointScope::Remote);
        assert_eq!(
            endpoint.canonical_base_url(),
            "https://openrouter.ai/api/v1"
        );
        assert!(OpenAiCompatibleEndpoint::parse("http://openrouter.ai/api/v1").is_err());
        assert!(OpenAiCompatibleEndpoint::parse("https://host.test/a/%2e%2e/v1").is_err());
        assert!(OpenAiCompatibleEndpoint::parse("https://key@host.test/v1").is_err());
        assert!(OpenAiCompatibleEndpoint::parse("https://host.test/v1?secret=x").is_err());
        Ok(())
    }

    #[test]
    fn settings_require_remote_https_and_exact_policy_binds_full_base_url()
    -> Result<(), Box<dyn std::error::Error>> {
        let configured =
            OpenAiCompatibleSettingsEndpointValidator.validate("https://api.groq.com/openai/v1")?;
        assert_eq!(configured.provider_id().as_str(), "openai-compatible");
        assert_eq!(configured.scope(), ModelEndpointScope::Remote);
        assert_eq!(
            configured.canonical_origin(),
            "https://api.groq.com/openai/v1"
        );
        assert!(
            OpenAiCompatibleSettingsEndpointValidator
                .validate("http://127.0.0.1:8080/v1")
                .is_err()
        );

        let endpoint = OpenAiCompatibleEndpoint::parse("https://api.groq.com/openai/v1")?;
        let exact = ExactOpenAiCompatibleEndpointPolicy::new(endpoint.canonical_base_url());
        assert!(exact.authorize(&endpoint).is_ok());
        let other = OpenAiCompatibleEndpoint::parse("https://openrouter.ai/api/v1")?;
        assert!(exact.authorize(&other).is_err());
        assert!(
            LocalOnlyOpenAiCompatibleEndpointPolicy
                .authorize(&other)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn catalog_keeps_arbitrary_valid_compatible_model_ids() -> Result<(), Box<dyn std::error::Error>>
    {
        let body = serde_json::to_vec(&json!({
            "object": "list",
            "data": [
                {"id": "llama-3.3-70b-versatile", "object": "model"},
                {"id": "openai/gpt-5.1", "object": "model"}
            ]
        }))?;
        let (models, truncated) = parse_model_catalog(&body)?;
        assert!(!truncated);
        assert_eq!(models.len(), 2);
        assert!(
            models
                .iter()
                .any(|model| model.as_str() == "openai/gpt-5.1")
        );
        Ok(())
    }

    #[test]
    fn capability_probe_requires_exact_verified_json() -> Result<(), Box<dyn std::error::Error>> {
        let body = serde_json::to_vec(&json!({
            "object": "chat.completion",
            "model": "fixture-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "{\"a3_probe\":\"ok\"}"},
                "finish_reason": "stop"
            }]
        }))?;
        assert!(parse_probe_response(&body, "fixture-model")?);
        assert!(!parse_probe_response(&body, "other-model")?);
        Ok(())
    }
}
