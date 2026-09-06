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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

const OPENAI_PROVIDER_ID: &str = "openai";
const DEFAULT_OPENAI_ORIGIN: &str = "https://api.openai.com";
const JSON_CONTENT_TYPE: &str = "application/json";
const EVENT_STREAM_CONTENT_TYPE: &str = "text/event-stream";
const OPENAI_USER_AGENT: &str = "a3/0.1.0";
const OPENAI_PROBE_PROMPT: &str =
    "Return exactly this JSON object and nothing else: {\"a3_probe\":\"ok\"}.";
const OPENAI_EMBED_PROBE_INPUT: &str = "A3 embedding capability probe";
const OPENAI_SCHEMA_NAME: &str = "a3_response";
const OPENAI_PROBE_SCHEMA_NAME: &str = "a3_capability_probe";
const OPENAI_PROBE_OUTPUT_TOKENS: u32 = 256;

const MAX_OPENAI_MODELS_BYTES: usize = 2 * 1024 * 1024;
const MAX_OPENAI_MODELS_OBSERVED: usize = 1_000;
const MAX_OPENAI_MODELS_COUNT: usize = 256;
const MAX_OPENAI_PROBE_BYTES: usize = 256 * 1024;
const MAX_OPENAI_EMBED_PROBE_BYTES: usize = 256 * 1024;
const MAX_OPENAI_EMBED_BYTES: usize = 8 * 1024 * 1024;
const MAX_OPENAI_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_OPENAI_SSE_LINE_BYTES: usize = 5 * 1024 * 1024;
const MAX_OPENAI_SSE_BUFFER_BYTES: usize = MAX_OPENAI_SSE_LINE_BYTES + 64 * 1024;
const MAX_OPENAI_EVENT_TYPE_BYTES: usize = 128;
const MAX_OPENAI_ITEM_ID_BYTES: usize = 256;
const MAX_OPENAI_REASONING_ITEMS: usize = 64;
const MAX_OPENAI_SCHEMA_DEPTH: usize = 64;
const MAX_OPENAI_SCHEMA_NODES: usize = 4_096;

/// Whether an OpenAI endpoint stays on loopback for tests or reaches the OpenAI API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenAiEndpointScope {
    /// Literal IPv4 or IPv6 loopback used by deterministic offline tests.
    LocalLoopback,
    /// Remote HTTPS origin.
    Remote,
}

/// Validated credential-free origin for the OpenAI API.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiEndpoint {
    url: reqwest::Url,
    scope: OpenAiEndpointScope,
}

impl OpenAiEndpoint {
    /// Returns the only production OpenAI origin.
    pub fn default_origin() -> Result<Self, OpenAiEndpointError> {
        Self::parse(DEFAULT_OPENAI_ORIGIN)
    }

    /// Parses an origin and rejects credentials, API paths, queries, and insecure remotes.
    pub fn parse(value: &str) -> Result<Self, OpenAiEndpointError> {
        let mut url = reqwest::Url::parse(value).map_err(|_| OpenAiEndpointError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(OpenAiEndpointError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OpenAiEndpointError::CredentialsForbidden);
        }
        if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
            return Err(OpenAiEndpointError::OriginRequired);
        }
        let host = url.host_str().ok_or(OpenAiEndpointError::MissingHost)?;
        if host.eq_ignore_ascii_case("localhost") {
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| OpenAiEndpointError::InvalidUrl)?;
        }
        let scope = url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .map_or(OpenAiEndpointScope::Remote, openai_endpoint_scope);
        if scope == OpenAiEndpointScope::Remote && url.scheme() != "https" {
            return Err(OpenAiEndpointError::InsecureRemote);
        }
        Ok(Self { url, scope })
    }

    /// Returns whether this origin is loopback or remote.
    #[must_use]
    pub const fn scope(&self) -> OpenAiEndpointScope {
        self.scope
    }

    /// Returns the normalized origin without an API path.
    #[must_use]
    pub fn canonical_origin(&self) -> String {
        self.url.as_str().trim_end_matches('/').to_owned()
    }

    fn models_url(&self) -> reqwest::Url {
        self.url_with_path("/v1/models")
    }

    fn responses_url(&self) -> reqwest::Url {
        self.url_with_path("/v1/responses")
    }

    fn embeddings_url(&self) -> reqwest::Url {
        self.url_with_path("/v1/embeddings")
    }

    fn url_with_path(&self, path: &str) -> reqwest::Url {
        let mut url = self.url.clone();
        url.set_path(path);
        url
    }
}

impl fmt::Debug for OpenAiEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiEndpoint")
            .field("scheme", &self.url.scheme())
            .field("scope", &self.scope)
            .field("port", &self.url.port_or_known_default())
            .finish()
    }
}

fn openai_endpoint_scope(address: IpAddr) -> OpenAiEndpointScope {
    if address.is_loopback() {
        OpenAiEndpointScope::LocalLoopback
    } else {
        OpenAiEndpointScope::Remote
    }
}

/// Invalid or unsafe OpenAI endpoint configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiEndpointError {
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
    /// Non-loopback endpoints require HTTPS.
    InsecureRemote,
}

impl fmt::Display for OpenAiEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUrl => "openai endpoint is not a valid absolute URL",
            Self::UnsupportedScheme => "openai endpoint scheme is unsupported",
            Self::MissingHost => "openai endpoint has no host",
            Self::CredentialsForbidden => "openai endpoint must not contain credentials",
            Self::OriginRequired => "openai endpoint must be an origin without path or query",
            Self::InsecureRemote => "remote openai endpoint must use HTTPS",
        })
    }
}

impl Error for OpenAiEndpointError {}

/// Settings adapter accepting any validated HTTPS OpenAI-compatible origin.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenAiSettingsEndpointValidator;

impl ModelEndpointValidator for OpenAiSettingsEndpointValidator {
    fn validate(
        &self,
        input: &str,
    ) -> Result<ConfiguredModelEndpoint, ModelEndpointValidationFailure> {
        let endpoint =
            OpenAiEndpoint::parse(input).map_err(|_| ModelEndpointValidationFailure::Invalid)?;
        let provider_id = ModelProviderId::try_from_string(OPENAI_PROVIDER_ID.to_owned())
            .map_err(|_| ModelEndpointValidationFailure::ProviderUnavailable)?;
        let scope = match endpoint.scope() {
            OpenAiEndpointScope::LocalLoopback => ModelEndpointScope::LocalLoopback,
            OpenAiEndpointScope::Remote => ModelEndpointScope::Remote,
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

/// Dynamic authorization checked before every OpenAI request.
pub trait OpenAiEndpointPolicy: fmt::Debug + Send + Sync {
    /// Authorizes the exact current endpoint or returns a content-free denial.
    fn authorize(&self, endpoint: &OpenAiEndpoint) -> Result<(), OpenAiEndpointPolicyError>;
}

/// Production policy allowing only `https://api.openai.com`.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardOpenAiEndpointPolicy;

impl OpenAiEndpointPolicy for StandardOpenAiEndpointPolicy {
    fn authorize(&self, endpoint: &OpenAiEndpoint) -> Result<(), OpenAiEndpointPolicyError> {
        if endpoint.canonical_origin() == DEFAULT_OPENAI_ORIGIN {
            Ok(())
        } else {
            Err(OpenAiEndpointPolicyError::Denied)
        }
    }
}

/// Exact-origin policy used after the native Settings confirmation step.
#[derive(Debug, Clone)]
pub struct ExactOpenAiEndpointPolicy {
    origin: String,
}

impl ExactOpenAiEndpointPolicy {
    /// Binds requests to one canonical origin without allowing redirects or proxies.
    #[must_use]
    pub fn new(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
        }
    }
}

impl OpenAiEndpointPolicy for ExactOpenAiEndpointPolicy {
    fn authorize(&self, endpoint: &OpenAiEndpoint) -> Result<(), OpenAiEndpointPolicyError> {
        if endpoint.canonical_origin() == self.origin {
            Ok(())
        } else {
            Err(OpenAiEndpointPolicyError::Denied)
        }
    }
}

/// Test-only policy allowing loopback origins and denying every remote.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalOnlyOpenAiEndpointPolicy;

impl OpenAiEndpointPolicy for LocalOnlyOpenAiEndpointPolicy {
    fn authorize(&self, endpoint: &OpenAiEndpoint) -> Result<(), OpenAiEndpointPolicyError> {
        if endpoint.scope() == OpenAiEndpointScope::LocalLoopback {
            Ok(())
        } else {
            Err(OpenAiEndpointPolicyError::Denied)
        }
    }
}

/// Endpoint was not authorized by the injected policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiEndpointPolicyError {
    /// Exact configured endpoint is outside the current allowlist.
    Denied,
}

impl fmt::Display for OpenAiEndpointPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("openai endpoint is not authorized by current policy")
    }
}

impl Error for OpenAiEndpointPolicyError {}

/// Native OpenAI implementation of A^3 model and embedding provider ports.
pub struct OpenAiModelProvider {
    provider_id: ModelProviderId,
    embedding_provider_id: EmbeddingProviderId,
    endpoint: OpenAiEndpoint,
    endpoint_policy: Arc<dyn OpenAiEndpointPolicy>,
    client: reqwest::Client,
    api_key: ProviderApiKey,
}

impl OpenAiModelProvider {
    /// Creates an OpenAI provider for one validated origin and short-lived API key.
    pub fn new(
        endpoint: OpenAiEndpoint,
        endpoint_policy: Arc<dyn OpenAiEndpointPolicy>,
        api_key: ProviderApiKey,
    ) -> Result<Self, OpenAiProviderCreateError> {
        let provider_id = ModelProviderId::try_from_string(OPENAI_PROVIDER_ID.to_owned())
            .map_err(|_| OpenAiProviderCreateError::InvalidProviderIdentity)?;
        let embedding_provider_id = EmbeddingProviderId::new(OPENAI_PROVIDER_ID.to_owned())
            .map_err(|_| OpenAiProviderCreateError::InvalidProviderIdentity)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| OpenAiProviderCreateError::HttpClient)?;
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
            .header(reqwest::header::USER_AGENT, OPENAI_USER_AGENT))
    }

    async fn probe_structured_output(
        &self,
        request: &ModelCapabilityProbeRequest,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<ModelStructuredOutputCapability, ModelProviderFailure> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "a3_probe": {"type": "string", "enum": ["ok"]}
            },
            "required": ["a3_probe"],
            "additionalProperties": false
        });
        let wire_request = OpenAiResponseRequest::probe(request.model_id().as_str(), &schema)?;
        let http_request = self.attach_auth(
            self.client
                .post(self.endpoint.responses_url())
                .json(&wire_request),
        )?;
        let response = send_before_deadline(http_request, deadline, control).await?;
        if let Err(error) = validate_json_response_head(&response) {
            return match error {
                ModelProviderFailure::Rejected | ModelProviderFailure::InvalidResponse => {
                    Ok(ModelStructuredOutputCapability::Unavailable)
                }
                other => Err(other),
            };
        }
        let body = match read_bounded_response(response, MAX_OPENAI_PROBE_BYTES, control).await {
            Ok(body) => body,
            Err(ModelProviderFailure::Rejected | ModelProviderFailure::InvalidResponse) => {
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

impl fmt::Debug for OpenAiModelProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiModelProvider")
            .field("provider_id", &self.provider_id)
            .field("embedding_provider_id", &self.embedding_provider_id)
            .field("endpoint", &self.endpoint)
            .field("endpoint_policy", &self.endpoint_policy)
            .field("has_api_key", &true)
            .finish()
    }
}

/// Failure creating an OpenAI provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiProviderCreateError {
    /// Provider identity failed domain validation.
    InvalidProviderIdentity,
    /// HTTP client initialization failed.
    HttpClient,
}

impl fmt::Display for OpenAiProviderCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProviderIdentity => "openai provider identity is invalid",
            Self::HttpClient => "failed to initialize HTTP client for openai provider",
        })
    }
}

impl Error for OpenAiProviderCreateError {}

impl ModelProvider for OpenAiModelProvider {
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
            let wire_request = OpenAiResponseRequest::from_request(request, true)?;
            let http_request = self.attach_auth(
                self.client
                    .post(self.endpoint.responses_url())
                    .timeout(timeout.duration())
                    .json(&wire_request),
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
            let state = OpenAiStreamState::new(body, control, request.model_id().as_str());
            let stream = futures::stream::try_unfold(state, next_openai_event);
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

impl ModelCatalogProvider for OpenAiModelProvider {
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
            let request = self.attach_auth(self.client.get(self.endpoint.models_url()))?;
            let response = send_before_deadline(request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body = read_bounded_response(response, MAX_OPENAI_MODELS_BYTES, control).await?;
            let (models, truncated) = parse_model_catalog(&body)?;
            Ok(ProviderModelCatalog::from_observation(
                self.provider_id.clone(),
                models,
                truncated,
            ))
        })
    }
}

impl ModelCapabilityProbe for OpenAiModelProvider {
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

impl EmbeddingCapabilityProbe for OpenAiModelProvider {
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
            let input = [OPENAI_EMBED_PROBE_INPUT];
            let wire_request = OpenAiEmbeddingRequest {
                model: request.model_id().as_str(),
                input: &input,
                encoding_format: "float",
            };
            let http_request = self.attach_auth(
                self.client
                    .post(self.endpoint.embeddings_url())
                    .json(&wire_request),
            )?;
            let response = send_before_deadline(http_request, deadline, control).await?;
            validate_json_response_head(&response)?;
            let body =
                read_bounded_response(response, MAX_OPENAI_EMBED_PROBE_BYTES, control).await?;
            let vectors = parse_embedding_response(&body, request.model_id().as_str(), 1)
                .map_err(map_embedding_to_model_failure)?;
            embedding_probe_dimension(vectors)
        })
    }
}

impl EmbeddingProvider for OpenAiModelProvider {
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
            let wire_request = OpenAiEmbeddingRequest {
                model: profile.model_id().as_str(),
                input: &input,
                encoding_format: "float",
            };
            let request = self
                .attach_auth(
                    self.client
                        .post(self.endpoint.embeddings_url())
                        .timeout(timeout.duration())
                        .json(&wire_request),
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
            let body =
                read_bounded_embedding_response(response, MAX_OPENAI_EMBED_BYTES, control).await?;
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
struct OpenAiResponseRequest<'a> {
    model: &'a str,
    input: Vec<OpenAiInputMessage<'a>>,
    stream: bool,
    store: bool,
    max_output_tokens: u32,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<OpenAiReasoningConfig>,
    tools: &'static [(); 0],
    tool_choice: &'static str,
    parallel_tool_calls: bool,
    truncation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<OpenAiTextConfig>,
}

impl<'a> OpenAiResponseRequest<'a> {
    fn from_request(
        request: &'a ModelProviderRequest,
        stream: bool,
    ) -> Result<Self, ModelProviderFailure> {
        if !request
            .profile()
            .settings()
            .stop_sequences()
            .as_slice()
            .is_empty()
        {
            return Err(ModelProviderFailure::Rejected);
        }
        let input = request
            .messages()
            .iter()
            .map(|message| OpenAiInputMessage {
                role: match message.role() {
                    ModelMessageRole::System => "system",
                    ModelMessageRole::User => "user",
                    ModelMessageRole::Assistant => "assistant",
                },
                content: message.content(),
            })
            .collect();
        let text = request
            .structured_output()
            .map(|schema| {
                translate_openai_json_schema(schema.value()).map(|schema| OpenAiTextConfig {
                    format: OpenAiTextFormat {
                        format_type: "json_schema",
                        name: OPENAI_SCHEMA_NAME,
                        strict: true,
                        schema,
                    },
                })
            })
            .transpose()?;
        let sampling = request.profile().settings().sampling();
        let model = request.model_id().as_str();
        Ok(Self {
            model,
            input,
            stream,
            store: false,
            max_output_tokens: request.profile().settings().output_limit().get(),
            temperature: f64::from(sampling.temperature().milli()) / 1_000.0,
            top_p: f64::from(sampling.top_p().milli()) / 1_000.0,
            reasoning: openai_reasoning_for_sampling(model),
            tools: &[],
            tool_choice: "none",
            parallel_tool_calls: false,
            truncation: "disabled",
            text,
        })
    }

    fn probe(model: &'a str, schema: &Value) -> Result<Self, ModelProviderFailure> {
        Ok(Self {
            model,
            input: vec![OpenAiInputMessage {
                role: "user",
                content: OPENAI_PROBE_PROMPT,
            }],
            stream: false,
            store: false,
            max_output_tokens: OPENAI_PROBE_OUTPUT_TOKENS,
            temperature: 0.0,
            top_p: 1.0,
            reasoning: openai_reasoning_for_sampling(model),
            tools: &[],
            tool_choice: "none",
            parallel_tool_calls: false,
            truncation: "disabled",
            text: Some(OpenAiTextConfig {
                format: OpenAiTextFormat {
                    format_type: "json_schema",
                    name: OPENAI_PROBE_SCHEMA_NAME,
                    strict: true,
                    schema: translate_openai_json_schema(schema)?,
                },
            }),
        })
    }
}

#[derive(Serialize)]
struct OpenAiReasoningConfig {
    effort: &'static str,
}

fn openai_reasoning_for_sampling(model: &str) -> Option<OpenAiReasoningConfig> {
    let minor = model
        .strip_prefix("gpt-5.")?
        .split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse::<u16>()
        .ok()?;
    (minor >= 1).then_some(OpenAiReasoningConfig { effort: "none" })
}

#[derive(Serialize)]
struct OpenAiInputMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct OpenAiTextConfig {
    format: OpenAiTextFormat,
}

#[derive(Serialize)]
struct OpenAiTextFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
    name: &'static str,
    strict: bool,
    schema: Value,
}

fn translate_openai_json_schema(schema: &Value) -> Result<Value, ModelProviderFailure> {
    let mut nodes = 0usize;
    let mut translated = translate_openai_schema_node(schema, 0, &mut nodes)?;
    compact_openai_prefix_items(&mut translated)?;
    prune_openai_unused_definitions(&mut translated)?;
    let root = translated
        .as_object()
        .ok_or(ModelProviderFailure::Rejected)?;
    if root.get("type").and_then(Value::as_str) != Some("object") || root.contains_key("anyOf") {
        return Err(ModelProviderFailure::Rejected);
    }
    Ok(translated)
}

fn translate_openai_schema_node(
    schema: &Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<Value, ModelProviderFailure> {
    if depth > MAX_OPENAI_SCHEMA_DEPTH || *nodes >= MAX_OPENAI_SCHEMA_NODES {
        return Err(ModelProviderFailure::Rejected);
    }
    *nodes += 1;
    let object = schema.as_object().ok_or(ModelProviderFailure::Rejected)?;
    let mut translated = serde_json::Map::new();
    for (key, value) in object {
        match key.as_str() {
            "$schema" | "$id" | "$anchor" | "title" | "minLength" | "maxLength" | "uniqueItems" => {
            }
            "const" => {
                if object.contains_key("enum") || !is_openai_enum_scalar(value) {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert("enum".to_owned(), Value::Array(vec![value.clone()]));
            }
            "$ref" => {
                let reference = value.as_str().ok_or(ModelProviderFailure::Rejected)?;
                if reference != "#" && !reference.starts_with("#/$defs/") {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert(key.clone(), value.clone());
            }
            "type" => {
                validate_openai_schema_type(value)?;
                translated.insert(key.clone(), value.clone());
            }
            "description" => {
                if !value.is_string() {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert(key.clone(), value.clone());
            }
            "enum" => {
                let values = value.as_array().ok_or(ModelProviderFailure::Rejected)?;
                if values.is_empty() || values.iter().any(|value| !is_openai_enum_scalar(value)) {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert(key.clone(), value.clone());
            }
            "pattern" | "format" | "minItems" | "maxItems" | "multipleOf" | "minimum"
            | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "required" => {
                translated.insert(key.clone(), value.clone());
            }
            "items" => {
                translated.insert(
                    key.clone(),
                    translate_openai_schema_node(value, depth + 1, nodes)?,
                );
            }
            "additionalProperties" => {
                if value.as_bool() != Some(false) {
                    return Err(ModelProviderFailure::Rejected);
                }
                translated.insert(key.clone(), Value::Bool(false));
            }
            "prefixItems" | "anyOf" | "oneOf" => {
                let items = value.as_array().ok_or(ModelProviderFailure::Rejected)?;
                if items.is_empty() {
                    return Err(ModelProviderFailure::Rejected);
                }
                let translated_items = items
                    .iter()
                    .map(|item| translate_openai_schema_node(item, depth + 1, nodes))
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
                        translate_openai_schema_node(child, depth + 1, nodes)?,
                    );
                }
                translated.insert(key.clone(), Value::Object(translated_entries));
            }
            _ => return Err(ModelProviderFailure::Rejected),
        }
    }
    infer_openai_enum_type(&mut translated)?;
    validate_openai_object_schema(&translated)?;
    Ok(Value::Object(translated))
}

fn validate_openai_schema_type(value: &Value) -> Result<(), ModelProviderFailure> {
    let valid = match value {
        Value::String(value) => is_openai_schema_type(value),
        Value::Array(values) => {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| value.as_str().is_some_and(is_openai_schema_type))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ModelProviderFailure::Rejected)
    }
}

fn is_openai_schema_type(value: &str) -> bool {
    matches!(
        value,
        "string" | "number" | "boolean" | "integer" | "object" | "array" | "null"
    )
}

fn is_openai_enum_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn infer_openai_enum_type(
    schema: &mut serde_json::Map<String, Value>,
) -> Result<(), ModelProviderFailure> {
    if schema.contains_key("type") || schema.contains_key("$ref") {
        return Ok(());
    }
    let Some(values) = schema.get("enum").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut inferred: Option<&'static str> = None;
    for value in values {
        let current = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) | Value::Object(_) => return Err(ModelProviderFailure::Rejected),
        };
        inferred = match (inferred, current) {
            (None, current) => Some(current),
            (Some("integer"), "number") | (Some("number"), "integer") => Some("number"),
            (Some(previous), current) if previous == current => Some(previous),
            _ => return Err(ModelProviderFailure::Rejected),
        };
    }
    let inferred = inferred.ok_or(ModelProviderFailure::Rejected)?;
    schema.insert("type".to_owned(), Value::String(inferred.to_owned()));
    Ok(())
}

fn validate_openai_object_schema(
    schema: &serde_json::Map<String, Value>,
) -> Result<(), ModelProviderFailure> {
    let declares_object = schema.get("type").and_then(Value::as_str) == Some("object")
        || schema.contains_key("properties");
    if !declares_object {
        return Ok(());
    }
    if schema.get("type").and_then(Value::as_str) != Some("object")
        || schema.get("additionalProperties").and_then(Value::as_bool) != Some(false)
    {
        return Err(ModelProviderFailure::Rejected);
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or(ModelProviderFailure::Rejected)?;
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or(ModelProviderFailure::Rejected)?;
    if required.len() != properties.len() {
        return Err(ModelProviderFailure::Rejected);
    }
    let mut required_names = BTreeSet::new();
    for name in required {
        let name = name.as_str().ok_or(ModelProviderFailure::Rejected)?;
        if !properties.contains_key(name) || !required_names.insert(name) {
            return Err(ModelProviderFailure::Rejected);
        }
    }
    Ok(())
}

fn compact_openai_prefix_items(schema: &mut Value) -> Result<(), ModelProviderFailure> {
    match schema {
        Value::Object(object) => {
            for value in object.values_mut() {
                compact_openai_prefix_items(value)?;
            }
            let Some(prefix_items) = object.get("prefixItems").and_then(Value::as_array) else {
                return Ok(());
            };
            if prefix_items.is_empty()
                || object.contains_key("items")
                || !openai_exact_array_length(object, prefix_items.len())
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let mut merged = prefix_items[0].clone();
            let equivalent = prefix_items.iter().skip(1).all(|item| {
                let mut candidate = merged.clone();
                if merge_openai_schema_shape(&mut candidate, item, None) {
                    merged = candidate;
                    true
                } else {
                    false
                }
            });
            let item_schema = if equivalent {
                merged
            } else {
                let mut union = serde_json::Map::new();
                union.insert("anyOf".to_owned(), Value::Array(prefix_items.clone()));
                Value::Object(union)
            };
            object.remove("prefixItems");
            object.insert("items".to_owned(), item_schema);
        }
        Value::Array(items) => {
            for item in items {
                compact_openai_prefix_items(item)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn openai_exact_array_length(object: &serde_json::Map<String, Value>, length: usize) -> bool {
    let Ok(length) = u64::try_from(length) else {
        return false;
    };
    object.get("minItems").and_then(Value::as_u64) == Some(length)
        && object.get("maxItems").and_then(Value::as_u64) == Some(length)
}

fn merge_openai_schema_shape(left: &mut Value, right: &Value, key: Option<&str>) -> bool {
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
                if !merge_openai_schema_shape(left_value, right_value, Some(name)) {
                    return false;
                }
            }
            true
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => left
            .iter_mut()
            .zip(right)
            .all(|(left, right)| merge_openai_schema_shape(left, right, None)),
        (left, right) => left == right,
    }
}

fn prune_openai_unused_definitions(schema: &mut Value) -> Result<(), ModelProviderFailure> {
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
            collect_openai_definition_references(value, &mut referenced)?;
        }
    }
    let mut pending = referenced.iter().cloned().collect::<VecDeque<_>>();
    while let Some(name) = pending.pop_front() {
        let definition = definitions
            .get(&name)
            .ok_or(ModelProviderFailure::Rejected)?;
        let mut nested = BTreeSet::new();
        collect_openai_definition_references(definition, &mut nested)?;
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

fn collect_openai_definition_references(
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
                collect_openai_definition_references(value, referenced)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_openai_definition_references(item, referenced)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[derive(Serialize)]
struct OpenAiEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
    encoding_format: &'static str,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    object: String,
    data: Vec<OpenAiModelRecord>,
}

#[derive(Deserialize)]
struct OpenAiModelRecord {
    id: String,
    object: String,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    object: String,
    model: String,
    data: Vec<OpenAiEmbeddingRecord>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingRecord {
    object: String,
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct OpenAiEventEnvelope {
    #[serde(rename = "type")]
    event_type: String,
}

#[derive(Deserialize)]
struct OpenAiResponseEvent {
    response: OpenAiResponseSummary,
}

#[derive(Deserialize)]
struct OpenAiResponseSummary {
    object: String,
    status: String,
    model: String,
    store: Option<bool>,
    tools: Option<Vec<Value>>,
    tool_choice: Option<Value>,
    parallel_tool_calls: Option<bool>,
    usage: Option<OpenAiUsage>,
    incomplete_details: Option<OpenAiIncompleteDetails>,
    error: Option<OpenAiError>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Deserialize)]
struct OpenAiIncompleteDetails {
    reason: String,
}

#[derive(Deserialize)]
struct OpenAiError {
    code: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiOutputItemEvent {
    output_index: usize,
    item: OpenAiOutputItemSummary,
}

#[derive(Deserialize)]
struct OpenAiOutputItemSummary {
    id: String,
    #[serde(rename = "type")]
    item_type: String,
    status: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiContentPartEvent {
    item_id: String,
    output_index: usize,
    content_index: usize,
    part: OpenAiContentPartSummary,
}

#[derive(Deserialize)]
struct OpenAiContentPartSummary {
    #[serde(rename = "type")]
    part_type: String,
}

#[derive(Deserialize)]
struct OpenAiTextDeltaEvent {
    item_id: String,
    output_index: usize,
    content_index: usize,
    delta: String,
}

#[derive(Deserialize)]
struct OpenAiTextDoneEvent {
    item_id: String,
    output_index: usize,
    content_index: usize,
}

#[derive(Deserialize)]
struct OpenAiErrorEvent {
    code: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiProbeResponse {
    #[serde(flatten)]
    summary: OpenAiResponseSummary,
    output: Vec<OpenAiProbeOutputItem>,
}

#[derive(Deserialize)]
struct OpenAiProbeOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    status: Option<String>,
    role: Option<String>,
    content: Option<Vec<OpenAiProbeContent>>,
}

#[derive(Deserialize)]
struct OpenAiProbeContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

type OpenAiByteStream = BoxStream<'static, Result<Vec<u8>, reqwest::Error>>;

struct OpenAiTextTarget {
    item_id: String,
    output_index: usize,
    content_index: Option<usize>,
    text_done: bool,
    content_done: bool,
    item_done: bool,
}

struct OpenAiStreamState<'a> {
    body: OpenAiByteStream,
    control: &'a dyn ModelOperationControl,
    requested_model: &'a str,
    buffer: Vec<u8>,
    queued: VecDeque<ProviderEvent>,
    announced_event: Option<String>,
    text_target: Option<OpenAiTextTarget>,
    reasoning_items: BTreeMap<String, bool>,
    output_bytes: usize,
    finish_reason: Option<ModelFinishReason>,
    usage: ModelProviderUsage,
    terminal_seen: bool,
    body_ended: bool,
}

impl<'a> OpenAiStreamState<'a> {
    fn new(
        body: OpenAiByteStream,
        control: &'a dyn ModelOperationControl,
        requested_model: &'a str,
    ) -> Self {
        Self {
            body,
            control,
            requested_model,
            buffer: Vec::new(),
            queued: VecDeque::new(),
            announced_event: None,
            text_target: None,
            reasoning_items: BTreeMap::new(),
            output_bytes: 0,
            finish_reason: None,
            usage: ModelProviderUsage::new(None, None),
            terminal_seen: false,
            body_ended: false,
        }
    }
}

async fn next_openai_event(
    mut state: OpenAiStreamState<'_>,
) -> Result<Option<(ProviderEvent, OpenAiStreamState<'_>)>, ModelProviderFailure> {
    loop {
        if let Some(event) = state.queued.pop_front() {
            return Ok(Some((event, state)));
        }
        if state.body_ended {
            return Ok(None);
        }
        if let Some(line) = take_complete_line(&mut state.buffer)? {
            parse_openai_sse_line(&mut state, &line)?;
            continue;
        }
        match read_stream_body_or_cancel(&mut state).await? {
            Some(bytes) => append_body_bytes(&mut state.buffer, &bytes)?,
            None => finish_openai_body(&mut state)?,
        }
    }
}

async fn read_stream_body_or_cancel(
    state: &mut OpenAiStreamState<'_>,
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
    if buffer.len().saturating_add(bytes.len()) > MAX_OPENAI_SSE_BUFFER_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn take_complete_line(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, ModelProviderFailure> {
    let Some(position) = buffer.iter().position(|byte| *byte == b'\n') else {
        if buffer.len() > MAX_OPENAI_SSE_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        return Ok(None);
    };
    if position > MAX_OPENAI_SSE_LINE_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut line = buffer.drain(..=position).collect::<Vec<_>>();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn parse_openai_sse_line(
    state: &mut OpenAiStreamState<'_>,
    line: &[u8],
) -> Result<(), ModelProviderFailure> {
    let line = std::str::from_utf8(line).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if line.is_empty() {
        state.announced_event = None;
        return Ok(());
    }
    if line.starts_with(':') {
        return Ok(());
    }
    if let Some(event_type) = line.strip_prefix("event:") {
        let event_type = event_type.trim();
        validate_event_type(event_type)?;
        if state
            .announced_event
            .replace(event_type.to_owned())
            .is_some()
        {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        return Ok(());
    }
    let data = line
        .strip_prefix("data:")
        .map(str::trim)
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if data == "[DONE]" {
        return if state.terminal_seen {
            Ok(())
        } else {
            Err(ModelProviderFailure::InvalidResponse)
        };
    }
    let envelope: OpenAiEventEnvelope =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    validate_event_type(&envelope.event_type)?;
    if state
        .announced_event
        .as_deref()
        .is_some_and(|announced| announced != envelope.event_type)
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if state.terminal_seen {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    parse_openai_event(state, &envelope.event_type, data)
}

fn parse_openai_event(
    state: &mut OpenAiStreamState<'_>,
    event_type: &str,
    data: &str,
) -> Result<(), ModelProviderFailure> {
    match event_type {
        "response.created" => parse_response_lifecycle(data, "in_progress", state.requested_model),
        "response.queued" => parse_response_lifecycle(data, "queued", state.requested_model),
        "response.in_progress" => {
            parse_response_lifecycle(data, "in_progress", state.requested_model)
        }
        "response.output_item.added" => parse_output_item_event(state, data, false),
        "response.output_item.done" => parse_output_item_event(state, data, true),
        "response.content_part.added" => parse_content_part_event(state, data, false),
        "response.content_part.done" => parse_content_part_event(state, data, true),
        "response.output_text.delta" => parse_text_delta_event(state, data),
        "response.output_text.done" => parse_text_done_event(state, data),
        "response.completed" => parse_terminal_event(state, data, ModelFinishReason::Stop),
        "response.incomplete" => parse_terminal_event(state, data, ModelFinishReason::OutputLimit),
        "response.failed" => {
            let event: OpenAiResponseEvent =
                serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
            validate_response_summary(&event.response, "failed")?;
            validate_response_model_binding(&event.response.model, state.requested_model)?;
            Err(classify_openai_error(event.response.error.as_ref()))
        }
        "error" => {
            let event: OpenAiErrorEvent =
                serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
            Err(classify_openai_error_code(event.code.as_deref()))
        }
        _ => Err(ModelProviderFailure::InvalidResponse),
    }
}

fn parse_response_lifecycle(
    data: &str,
    status: &str,
    requested_model: &str,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiResponseEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    validate_response_summary(&event.response, status)?;
    validate_response_model_binding(&event.response.model, requested_model)
}

fn parse_output_item_event(
    state: &mut OpenAiStreamState<'_>,
    data: &str,
    done: bool,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiOutputItemEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    validate_item_id(&event.item.id)?;
    if event.output_index > 256 {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    match event.item.item_type.as_str() {
        "message" => {
            if event.item.role.as_deref() != Some("assistant") {
                return Err(ModelProviderFailure::InvalidResponse);
            }
            let expected_status = if done { "completed" } else { "in_progress" };
            if event.item.status.as_deref() != Some(expected_status) {
                return Err(ModelProviderFailure::InvalidResponse);
            }
            if done {
                let target = state
                    .text_target
                    .as_mut()
                    .ok_or(ModelProviderFailure::InvalidResponse)?;
                if target.item_id != event.item.id
                    || target.output_index != event.output_index
                    || !target.content_done
                    || target.item_done
                {
                    return Err(ModelProviderFailure::InvalidResponse);
                }
                target.item_done = true;
            } else if state.text_target.is_some() {
                return Err(ModelProviderFailure::InvalidResponse);
            } else {
                state.text_target = Some(OpenAiTextTarget {
                    item_id: event.item.id,
                    output_index: event.output_index,
                    content_index: None,
                    text_done: false,
                    content_done: false,
                    item_done: false,
                });
            }
            Ok(())
        }
        "reasoning" => {
            if event.item.role.is_some() {
                return Err(ModelProviderFailure::InvalidResponse);
            }
            if done {
                let completed = state
                    .reasoning_items
                    .get_mut(&event.item.id)
                    .ok_or(ModelProviderFailure::InvalidResponse)?;
                if *completed {
                    return Err(ModelProviderFailure::InvalidResponse);
                }
                *completed = true;
            } else if state.reasoning_items.len() >= MAX_OPENAI_REASONING_ITEMS
                || state.reasoning_items.insert(event.item.id, false).is_some()
            {
                return Err(ModelProviderFailure::InvalidResponse);
            }
            Ok(())
        }
        _ => Err(ModelProviderFailure::Rejected),
    }
}

fn parse_content_part_event(
    state: &mut OpenAiStreamState<'_>,
    data: &str,
    done: bool,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiContentPartEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if event.part.part_type != "output_text" {
        return Err(ModelProviderFailure::Rejected);
    }
    let target = matching_text_target(state, &event.item_id, event.output_index)?;
    if done {
        if target.content_index != Some(event.content_index)
            || !target.text_done
            || target.content_done
        {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        target.content_done = true;
    } else if target.content_index.replace(event.content_index).is_some() {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn parse_text_delta_event(
    state: &mut OpenAiStreamState<'_>,
    data: &str,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiTextDeltaEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let target = matching_text_target(state, &event.item_id, event.output_index)?;
    if target.content_index != Some(event.content_index)
        || target.text_done
        || event.delta.is_empty()
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    state.output_bytes = state.output_bytes.saturating_add(event.delta.len());
    if state.output_bytes > MAX_OPENAI_OUTPUT_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let chunk = ModelOutputChunk::try_from_string(event.delta)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    state.queued.push_back(ProviderEvent::OutputText(chunk));
    Ok(())
}

fn parse_text_done_event(
    state: &mut OpenAiStreamState<'_>,
    data: &str,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiTextDoneEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let target = matching_text_target(state, &event.item_id, event.output_index)?;
    if target.content_index != Some(event.content_index) || target.text_done {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    target.text_done = true;
    Ok(())
}

fn matching_text_target<'a>(
    state: &'a mut OpenAiStreamState<'_>,
    item_id: &str,
    output_index: usize,
) -> Result<&'a mut OpenAiTextTarget, ModelProviderFailure> {
    validate_item_id(item_id)?;
    let target = state
        .text_target
        .as_mut()
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if target.item_id != item_id || target.output_index != output_index || target.item_done {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(target)
}

fn parse_terminal_event(
    state: &mut OpenAiStreamState<'_>,
    data: &str,
    reason: ModelFinishReason,
) -> Result<(), ModelProviderFailure> {
    let event: OpenAiResponseEvent =
        serde_json::from_str(data).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    let expected_status = match reason {
        ModelFinishReason::Stop => "completed",
        ModelFinishReason::OutputLimit => "incomplete",
        ModelFinishReason::Other => return Err(ModelProviderFailure::InvalidResponse),
    };
    validate_response_summary(&event.response, expected_status)?;
    validate_response_model_binding(&event.response.model, state.requested_model)?;
    if reason == ModelFinishReason::OutputLimit
        && event
            .response
            .incomplete_details
            .as_ref()
            .map(|details| details.reason.as_str())
            != Some("max_output_tokens")
    {
        return Err(ModelProviderFailure::Rejected);
    }
    let target = state
        .text_target
        .as_ref()
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    if !target.item_done || state.reasoning_items.values().any(|done| !done) {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let usage = event
        .response
        .usage
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    state.finish_reason = Some(reason);
    state.usage = ModelProviderUsage::new(Some(usage.input_tokens), Some(usage.output_tokens));
    state.terminal_seen = true;
    Ok(())
}

fn validate_response_summary(
    response: &OpenAiResponseSummary,
    expected_status: &str,
) -> Result<(), ModelProviderFailure> {
    if response.object != "response"
        || response.status != expected_status
        || response.store != Some(false)
        || response
            .tools
            .as_ref()
            .is_none_or(|tools| !tools.is_empty())
        || response.parallel_tool_calls != Some(false)
        || response.tool_choice.as_ref().and_then(Value::as_str) != Some("none")
        || ModelId::try_from_string(response.model.clone()).is_err()
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn finish_openai_body(state: &mut OpenAiStreamState<'_>) -> Result<(), ModelProviderFailure> {
    if !state.buffer.is_empty() {
        if state.buffer.len() > MAX_OPENAI_SSE_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        let line = std::mem::take(&mut state.buffer);
        parse_openai_sse_line(state, &line)?;
    }
    if !state.terminal_seen {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let reason = state
        .finish_reason
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    state
        .queued
        .push_back(ProviderEvent::Completed(ModelProviderCompletion::new(
            reason,
            state.usage,
        )));
    state.body_ended = true;
    Ok(())
}

fn validate_event_type(event_type: &str) -> Result<(), ModelProviderFailure> {
    if event_type.is_empty()
        || event_type.len() > MAX_OPENAI_EVENT_TYPE_BYTES
        || !event_type.is_ascii()
        || event_type
            .bytes()
            .any(|byte| !(byte.is_ascii_lowercase() || byte == b'.' || byte == b'_'))
    {
        Err(ModelProviderFailure::InvalidResponse)
    } else {
        Ok(())
    }
}

fn validate_item_id(item_id: &str) -> Result<(), ModelProviderFailure> {
    if item_id.is_empty()
        || item_id.len() > MAX_OPENAI_ITEM_ID_BYTES
        || !item_id.is_ascii()
        || item_id
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    {
        Err(ModelProviderFailure::InvalidResponse)
    } else {
        Ok(())
    }
}

fn parse_model_catalog(body: &[u8]) -> Result<(Vec<ModelId>, bool), ModelProviderFailure> {
    let response = serde_json::from_slice::<OpenAiModelsResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if response.object != "list" {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut models = BTreeSet::new();
    let mut truncated = response.data.len() > MAX_OPENAI_MODELS_OBSERVED;
    for record in response.data.into_iter().take(MAX_OPENAI_MODELS_OBSERVED) {
        if record.object != "model" {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        if !record.id.starts_with("gpt-") && !record.id.starts_with("text-embedding-") {
            continue;
        }
        if let Ok(model) = ModelId::try_from_string(record.id) {
            models.insert(model);
            if models.len() > MAX_OPENAI_MODELS_COUNT {
                truncated = true;
                break;
            }
        }
    }
    let mut models = models.into_iter().collect::<Vec<_>>();
    if models.len() > MAX_OPENAI_MODELS_COUNT {
        models.truncate(MAX_OPENAI_MODELS_COUNT);
        truncated = true;
    }
    Ok((models, truncated))
}

fn parse_probe_response(body: &[u8], requested_model: &str) -> Result<bool, ModelProviderFailure> {
    let response = serde_json::from_slice::<OpenAiProbeResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    validate_response_summary(&response.summary, "completed")?;
    validate_response_model_binding(&response.summary.model, requested_model)?;
    let mut output_text: Option<String> = None;
    for item in response.output {
        match item.item_type.as_str() {
            "reasoning" if item.role.is_none() => {}
            "message"
                if item.role.as_deref() == Some("assistant")
                    && item.status.as_deref() == Some("completed") =>
            {
                for content in item.content.unwrap_or_default() {
                    if content.content_type != "output_text" || output_text.is_some() {
                        return Ok(false);
                    }
                    output_text = content.text;
                }
            }
            _ => return Ok(false),
        }
    }
    let Some(text) = output_text else {
        return Ok(false);
    };
    let value: Value =
        serde_json::from_str(&text).map_err(|_| ModelProviderFailure::InvalidResponse)?;
    Ok(value == serde_json::json!({"a3_probe": "ok"}))
}

fn parse_embedding_response(
    body: &[u8],
    requested_model: &str,
    expected_count: usize,
) -> Result<Vec<Vec<f32>>, EmbeddingProviderFailure> {
    let response = serde_json::from_slice::<OpenAiEmbeddingResponse>(body)
        .map_err(|_| EmbeddingProviderFailure::InvalidResponse)?;
    if response.object != "list"
        || !response_model_matches_requested(&response.model, requested_model)
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

fn validate_response_model_binding(
    actual: &str,
    requested: &str,
) -> Result<(), ModelProviderFailure> {
    if response_model_matches_requested(actual, requested) {
        Ok(())
    } else {
        Err(ModelProviderFailure::InvalidResponse)
    }
}

fn response_model_matches_requested(actual: &str, requested: &str) -> bool {
    if actual == requested {
        return true;
    }
    let Some(snapshot) = actual
        .strip_prefix(requested)
        .and_then(|suffix| suffix.strip_prefix('-'))
    else {
        return false;
    };
    let bytes = snapshot.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn validate_json_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    if let Some(failure) = classify_http_status(response.status()) {
        return Err(failure);
    }
    validate_content_type(response, JSON_CONTENT_TYPE)
}

fn validate_stream_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    if let Some(failure) = classify_http_status(response.status()) {
        return Err(failure);
    }
    validate_content_type(response, EVENT_STREAM_CONTENT_TYPE)
}

fn validate_content_type(
    response: &reqwest::Response,
    expected: &str,
) -> Result<(), ModelProviderFailure> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if matches!(content_type, Some(value) if value.eq_ignore_ascii_case(expected)) {
        Ok(())
    } else {
        Err(ModelProviderFailure::InvalidResponse)
    }
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

fn classify_openai_error(error: Option<&OpenAiError>) -> ModelProviderFailure {
    classify_openai_error_code(error.and_then(|error| error.code.as_deref()))
}

fn classify_openai_error_code(code: Option<&str>) -> ModelProviderFailure {
    match code {
        Some("rate_limit_exceeded" | "server_error" | "temporarily_unavailable") => {
            ModelProviderFailure::Unavailable
        }
        Some(_) => ModelProviderFailure::Rejected,
        None => ModelProviderFailure::InvalidResponse,
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
        if control.is_cancelled() {
            return Err(ModelProviderFailure::Cancelled);
        }
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
        LocalOnlyOpenAiEndpointPolicy, OpenAiEndpoint, OpenAiEndpointError, OpenAiEndpointPolicy,
        OpenAiEndpointScope, StandardOpenAiEndpointPolicy, classify_http_status,
        classify_openai_error_code, openai_reasoning_for_sampling, parse_model_catalog,
        response_model_matches_requested, translate_openai_json_schema,
    };
    use a3_application::{
        AgentActionJsonSchema, DecodeExplorerAction, DecodeModuleCardClaims, ModelProviderFailure,
    };
    use serde_json::{Value, json};

    #[test]
    fn endpoint_and_policies_keep_production_and_test_origins_separate()
    -> Result<(), Box<dyn std::error::Error>> {
        let local = OpenAiEndpoint::parse("http://localhost:8080")?;
        assert_eq!(local.scope(), OpenAiEndpointScope::LocalLoopback);
        assert_eq!(local.canonical_origin(), "http://127.0.0.1:8080");
        assert!(LocalOnlyOpenAiEndpointPolicy.authorize(&local).is_ok());
        assert!(StandardOpenAiEndpointPolicy.authorize(&local).is_err());

        let production = OpenAiEndpoint::default_origin()?;
        assert_eq!(production.canonical_origin(), "https://api.openai.com");
        assert!(StandardOpenAiEndpointPolicy.authorize(&production).is_ok());
        assert!(
            LocalOnlyOpenAiEndpointPolicy
                .authorize(&production)
                .is_err()
        );
        assert!(matches!(
            OpenAiEndpoint::parse("http://api.openai.com"),
            Err(OpenAiEndpointError::InsecureRemote)
        ));
        assert!(OpenAiEndpoint::parse("https://user:key@api.openai.com").is_err());
        assert!(OpenAiEndpoint::parse("https://api.openai.com/v1").is_err());
        Ok(())
    }

    #[test]
    fn catalog_filters_candidates_without_inferring_capabilities()
    -> Result<(), Box<dyn std::error::Error>> {
        let body = br#"{"object":"list","data":[
            {"id":"whisper-1","object":"model"},
            {"id":"gpt-5.4","object":"model"},
            {"id":"text-embedding-3-small","object":"model"},
            {"id":"gpt-5.4","object":"model"}
        ]}"#;
        let (models, truncated) = parse_model_catalog(body)?;
        assert_eq!(
            models
                .iter()
                .map(|model| model.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.4", "text-embedding-3-small"]
        );
        assert!(!truncated);
        Ok(())
    }

    #[test]
    fn failures_are_content_free_and_preserve_transience() {
        assert_eq!(
            classify_http_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            Some(ModelProviderFailure::Unavailable)
        );
        assert_eq!(
            classify_http_status(reqwest::StatusCode::BAD_REQUEST),
            Some(ModelProviderFailure::Rejected)
        );
        assert_eq!(
            classify_openai_error_code(Some("server_error")),
            ModelProviderFailure::Unavailable
        );
        assert_eq!(
            classify_openai_error_code(Some("invalid_prompt")),
            ModelProviderFailure::Rejected
        );
        assert_eq!(
            classify_openai_error_code(None),
            ModelProviderFailure::InvalidResponse
        );
    }

    #[test]
    fn response_model_binding_accepts_only_the_requested_alias_or_dated_snapshot() {
        assert!(response_model_matches_requested("gpt-5-mini", "gpt-5-mini"));
        assert!(response_model_matches_requested(
            "gpt-5-mini-2025-08-07",
            "gpt-5-mini"
        ));
        assert!(!response_model_matches_requested(
            "gpt-5-mini-audio",
            "gpt-5-mini"
        ));
        assert!(!response_model_matches_requested(
            "gpt-4o-mini-2024-07-18",
            "gpt-4o"
        ));
    }

    #[test]
    fn current_gpt_five_families_disable_reasoning_when_sampling_is_bound() {
        assert!(openai_reasoning_for_sampling("gpt-5.6-terra").is_some());
        assert!(openai_reasoning_for_sampling("gpt-5.4-mini").is_some());
        assert!(openai_reasoning_for_sampling("gpt-5-mini").is_none());
        assert!(openai_reasoning_for_sampling("gpt-4.1").is_none());
    }

    #[test]
    fn production_schemas_translate_to_the_openai_strict_subset()
    -> Result<(), Box<dyn std::error::Error>> {
        let schemas = [
            (
                "Deep Map explorer",
                DecodeExplorerAction::version_one()
                    .json_schema()
                    .as_str()
                    .to_owned(),
            ),
            (
                "Module Card claims",
                DecodeModuleCardClaims::version_one()
                    .json_schema()
                    .as_str()
                    .to_owned(),
            ),
            (
                "AgentAction",
                serde_json::to_string(&AgentActionJsonSchema::current().as_json()?)?,
            ),
        ];
        for (name, schema) in schemas {
            let schema: Value = serde_json::from_str(&schema)?;
            let translated = translate_openai_json_schema(&schema).map_err(|error| {
                std::io::Error::other(format!("{name} schema was rejected: {error:?}"))
            })?;
            assert_eq!(translated["type"], "object");
            for unsupported in [
                "$schema",
                "$id",
                "$anchor",
                "const",
                "oneOf",
                "prefixItems",
                "uniqueItems",
                "minLength",
                "maxLength",
            ] {
                assert!(!contains_key(&translated, unsupported));
            }
        }
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
            let translated = translate_openai_json_schema(&original)?;
            assert_eq!(translated["type"], "object");
            assert_eq!(translated["additionalProperties"], false);
            assert!(!contains_key(&translated, "oneOf"));
            assert!(!contains_key(&translated, "const"));
            assert!(translated["$defs"].get("research").is_none());
        }
        Ok(())
    }

    #[test]
    fn openai_schema_translation_rejects_optional_object_fields() {
        let result = translate_openai_json_schema(&json!({
            "type": "object",
            "properties": {
                "required_value": {"type": "string"},
                "optional_value": {"type": "string"}
            },
            "required": ["required_value"],
            "additionalProperties": false
        }));
        assert_eq!(result, Err(ModelProviderFailure::Rejected));
    }

    fn contains_key(value: &Value, target: &str) -> bool {
        match value {
            Value::Object(object) => {
                object.contains_key(target)
                    || object.values().any(|value| contains_key(value, target))
            }
            Value::Array(items) => items.iter().any(|value| contains_key(value, target)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
    }
}
