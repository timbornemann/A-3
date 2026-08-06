use crate::{OllamaEndpoint, OllamaEndpointPolicy};
use a3_application::{
    ModelCapabilityObservation, ModelCapabilityProbe, ModelCapabilityProbeFuture,
    ModelCapabilityProbeRequest, ModelFinishReason, ModelMessageRole, ModelOperationControl,
    ModelOutputChunk, ModelProvider, ModelProviderCompletion, ModelProviderFailure,
    ModelProviderFuture, ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout,
    ProviderEvent, ProviderEventStream, ReportedModelContextLimit,
};
use a3_domain::{
    ModelCapabilities, ModelId, ModelProviderId, ModelStructuredOutputCapability, ModelToolCallMode,
};
use futures::future::{Either, select};
use futures::stream::{BoxStream, StreamExt};
use futures::{FutureExt, pin_mut};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

const OLLAMA_PROVIDER_ID: &str = "ollama";
const OLLAMA_CONTENT_TYPE: &str = "application/x-ndjson";
const OLLAMA_JSON_CONTENT_TYPE: &str = "application/json";
const MAX_OLLAMA_BUFFER_BYTES: usize = 256 * 1024;
const MAX_OLLAMA_LINE_BYTES: usize = 128 * 1024;
const MAX_OLLAMA_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_OLLAMA_SHOW_BYTES: usize = 512 * 1024;
const MAX_OLLAMA_PROBE_BYTES: usize = 128 * 1024;
const MAX_OLLAMA_CAPABILITIES: usize = 64;
const MAX_OLLAMA_CAPABILITY_BYTES: usize = 64;
const MAX_OLLAMA_MODEL_INFO_FIELDS: usize = 2_048;
const MAX_OLLAMA_MODEL_INFO_KEY_BYTES: usize = 256;
const OLLAMA_PROBE_CONTEXT_TOKENS: u32 = 4_096;
const OLLAMA_PROBE_OUTPUT_TOKENS: u32 = 32;
const OLLAMA_PROBE_PROMPT: &str =
    "Return exactly this JSON object and nothing else: {\"a3_probe\":\"ok\"}.";

/// Ollama-compatible implementation of the general streaming model-provider port.
pub struct OllamaModelProvider {
    provider_id: ModelProviderId,
    endpoint: OllamaEndpoint,
    endpoint_policy: Arc<dyn OllamaEndpointPolicy>,
    client: reqwest::Client,
}

impl OllamaModelProvider {
    /// Creates a reusable redirect-free, proxy-free HTTP client for one validated endpoint.
    pub fn new(
        endpoint: OllamaEndpoint,
        endpoint_policy: Arc<dyn OllamaEndpointPolicy>,
    ) -> Result<Self, OllamaProviderCreateError> {
        let provider_id = ModelProviderId::try_from_string(OLLAMA_PROVIDER_ID.to_owned())
            .map_err(|_| OllamaProviderCreateError::InvalidProviderIdentity)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| OllamaProviderCreateError::HttpClient)?;
        Ok(Self {
            provider_id,
            endpoint,
            endpoint_policy,
            client,
        })
    }
}

impl fmt::Debug for OllamaModelProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OllamaModelProvider")
            .field("provider_id", &self.provider_id)
            .field("endpoint", &self.endpoint)
            .field("endpoint_policy", &self.endpoint_policy)
            .finish_non_exhaustive()
    }
}

impl ModelProvider for OllamaModelProvider {
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
            self.endpoint_policy
                .authorize(&self.endpoint)
                .map_err(|_| ModelProviderFailure::EndpointDenied)?;
            if control.is_cancelled() {
                return Err(ModelProviderFailure::Cancelled);
            }
            if request.profile().provider_id() != &self.provider_id {
                return Err(ModelProviderFailure::Rejected);
            }
            let wire_request = OllamaChatRequest::from_request(request);
            let send = self
                .client
                .post(self.endpoint.chat_url())
                .timeout(timeout.duration())
                .json(&wire_request)
                .send()
                .fuse();
            let cancelled = control.cancelled().fuse();
            pin_mut!(send, cancelled);
            let response = match select(cancelled, send).await {
                Either::Left(((), _)) => return Err(ModelProviderFailure::Cancelled),
                Either::Right((result, _)) => result.map_err(classify_reqwest_error)?,
            };
            validate_response_head(&response)?;
            let body = response
                .bytes_stream()
                .map(|item| item.map(|bytes| bytes.to_vec()))
                .boxed();
            let state = OllamaStreamState::new(body, request.model_id().clone(), control);
            let stream = futures::stream::try_unfold(state, next_provider_event);
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

impl ModelCapabilityProbe for OllamaModelProvider {
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
            let deadline = Instant::now()
                .checked_add(timeout.duration())
                .ok_or(ModelProviderFailure::TimedOut)?;
            let show = self.show_model(request, deadline, control).await?;
            let structured_output = self
                .probe_structured_output(request, show.context_limit, deadline, control)
                .await?;
            let tool_call_mode = if show.tools_reported {
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

impl OllamaModelProvider {
    async fn show_model(
        &self,
        request: &ModelCapabilityProbeRequest,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<OllamaShowObservation, ModelProviderFailure> {
        self.authorize_probe_request(control)?;
        let wire_request = OllamaShowRequest {
            model: request.model_id().as_str(),
            verbose: false,
        };
        let response = send_before_deadline(
            self.client
                .post(self.endpoint.show_url())
                .json(&wire_request),
            deadline,
            control,
        )
        .await?;
        validate_json_response_head(&response)?;
        let body = read_bounded_response(response, MAX_OLLAMA_SHOW_BYTES, control).await?;
        parse_show_observation(&body)
    }

    async fn probe_structured_output(
        &self,
        request: &ModelCapabilityProbeRequest,
        reported_context_limit: Option<ReportedModelContextLimit>,
        deadline: Instant,
        control: &dyn ModelOperationControl,
    ) -> Result<ModelStructuredOutputCapability, ModelProviderFailure> {
        self.authorize_probe_request(control)?;
        let schema = ollama_probe_schema();
        let wire_request = OllamaProbeChatRequest {
            model: request.model_id().as_str(),
            messages: [OllamaRequestMessage {
                role: "user",
                content: OLLAMA_PROBE_PROMPT,
            }],
            stream: false,
            think: false,
            format: &schema,
            options: OllamaChatOptions::for_probe(request, reported_context_limit),
        };
        let response = send_before_deadline(
            self.client
                .post(self.endpoint.chat_url())
                .json(&wire_request),
            deadline,
            control,
        )
        .await?;
        if let Err(error) = validate_json_response_head(&response) {
            return match error {
                ModelProviderFailure::Rejected | ModelProviderFailure::InvalidResponse => {
                    Ok(ModelStructuredOutputCapability::Unavailable)
                }
                other => Err(other),
            };
        }
        let body = match read_bounded_response(response, MAX_OLLAMA_PROBE_BYTES, control).await {
            Ok(body) => body,
            Err(ModelProviderFailure::InvalidResponse) => {
                return Ok(ModelStructuredOutputCapability::Unavailable);
            }
            Err(other) => return Err(other),
        };
        Ok(
            if valid_structured_probe_response(&body, request.model_id()) {
                ModelStructuredOutputCapability::Verified
            } else {
                ModelStructuredOutputCapability::Unavailable
            },
        )
    }

    fn authorize_probe_request(
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
}

async fn send_before_deadline(
    request: reqwest::RequestBuilder,
    deadline: Instant,
    control: &dyn ModelOperationControl,
) -> Result<reqwest::Response, ModelProviderFailure> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or(ModelProviderFailure::TimedOut)?;
    let send = request.timeout(remaining).send().fuse();
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
        let item = match select(cancelled, read).await {
            Either::Left(((), _)) => return Err(ModelProviderFailure::Cancelled),
            Either::Right((item, _)) => item,
        };
        let Some(chunk) = item else {
            return Ok(bytes);
        };
        let chunk = chunk.map_err(classify_reqwest_error)?;
        if bytes.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
}

#[derive(Serialize)]
struct OllamaShowRequest<'a> {
    model: &'a str,
    verbose: bool,
}

#[derive(Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    model_info: serde_json::Map<String, Value>,
}

struct OllamaShowObservation {
    context_limit: Option<ReportedModelContextLimit>,
    tools_reported: bool,
}

fn parse_show_observation(body: &[u8]) -> Result<OllamaShowObservation, ModelProviderFailure> {
    let response = serde_json::from_slice::<OllamaShowResponse>(body)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if response.capabilities.len() > MAX_OLLAMA_CAPABILITIES
        || response.capabilities.iter().any(|capability| {
            capability.len() > MAX_OLLAMA_CAPABILITY_BYTES
                || capability.chars().any(char::is_control)
        })
        || response.model_info.len() > MAX_OLLAMA_MODEL_INFO_FIELDS
        || response.model_info.keys().any(|key| {
            key.len() > MAX_OLLAMA_MODEL_INFO_KEY_BYTES || key.chars().any(char::is_control)
        })
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut context_limit = None;
    for (key, value) in &response.model_info {
        if !key.ends_with(".context_length") {
            continue;
        }
        let raw = value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(ModelProviderFailure::InvalidResponse)?;
        let reported = ReportedModelContextLimit::new(raw)
            .map_err(|_| ModelProviderFailure::InvalidResponse)?;
        if context_limit.is_some_and(|existing| existing != reported) {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        context_limit = Some(reported);
    }
    Ok(OllamaShowObservation {
        context_limit,
        tools_reported: response
            .capabilities
            .iter()
            .any(|capability| capability == "tools"),
    })
}

#[derive(Serialize)]
struct OllamaProbeChatRequest<'a> {
    model: &'a str,
    messages: [OllamaRequestMessage<'a>; 1],
    stream: bool,
    think: bool,
    format: &'a Value,
    options: OllamaChatOptions<'a>,
}

fn ollama_probe_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "a3_probe": {
                "type": "string",
                "const": "ok"
            }
        },
        "required": ["a3_probe"],
        "additionalProperties": false
    })
}

fn valid_structured_probe_response(body: &[u8], expected_model: &ModelId) -> bool {
    let Ok(response) = serde_json::from_slice::<OllamaProbeChatResponse>(body) else {
        return false;
    };
    if response.model != expected_model.as_str()
        || response.message.role != "assistant"
        || !response.done
        || response
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    {
        return false;
    }
    serde_json::from_str::<Value>(&response.message.content)
        .is_ok_and(|value| value == serde_json::json!({"a3_probe": "ok"}))
}

#[derive(Deserialize)]
struct OllamaProbeChatResponse {
    model: String,
    message: OllamaResponseMessage,
    done: bool,
}

fn validate_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    let status = response.status();
    if status.is_client_error() || status.is_redirection() {
        return Err(ModelProviderFailure::Rejected);
    }
    if !status.is_success() {
        return Err(ModelProviderFailure::Unavailable);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !matches!(content_type, Some(value) if value.eq_ignore_ascii_case(OLLAMA_CONTENT_TYPE)) {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn validate_json_response_head(response: &reqwest::Response) -> Result<(), ModelProviderFailure> {
    let status = response.status();
    if status.is_client_error() || status.is_redirection() {
        return Err(ModelProviderFailure::Rejected);
    }
    if !status.is_success() {
        return Err(ModelProviderFailure::Unavailable);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !matches!(content_type, Some(value) if value.eq_ignore_ascii_case(OLLAMA_JSON_CONTENT_TYPE))
    {
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

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaRequestMessage<'a>>,
    stream: bool,
    think: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a Value>,
    options: OllamaChatOptions<'a>,
}

impl<'a> OllamaChatRequest<'a> {
    fn from_request(request: &'a ModelProviderRequest) -> Self {
        Self {
            model: request.model_id().as_str(),
            messages: request
                .messages()
                .iter()
                .map(|message| OllamaRequestMessage {
                    role: ollama_role(message.role()),
                    content: message.content(),
                })
                .collect(),
            stream: true,
            think: false,
            format: request
                .structured_output()
                .map(a3_application::StructuredOutputSchema::value),
            options: OllamaChatOptions::from_request(request),
        }
    }
}

#[derive(Serialize)]
struct OllamaChatOptions<'a> {
    num_ctx: u32,
    num_predict: u32,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<&'a str>>,
}

impl<'a> OllamaChatOptions<'a> {
    fn from_request(request: &'a ModelProviderRequest) -> Self {
        let settings = request.profile().settings();
        let stops = settings.stop_sequences().as_slice();
        Self {
            num_ctx: settings.context_limit().get(),
            num_predict: settings.output_limit().get(),
            temperature: f64::from(settings.sampling().temperature().milli()) / 1_000.0,
            top_p: f64::from(settings.sampling().top_p().milli()) / 1_000.0,
            stop: (!stops.is_empty()).then(|| stops.iter().map(|stop| stop.as_str()).collect()),
        }
    }

    fn for_probe(
        request: &'a ModelCapabilityProbeRequest,
        reported_context_limit: Option<ReportedModelContextLimit>,
    ) -> Self {
        let num_ctx = reported_context_limit.map_or_else(
            || {
                request
                    .settings()
                    .context_limit()
                    .get()
                    .min(OLLAMA_PROBE_CONTEXT_TOKENS)
            },
            |reported| {
                request
                    .settings()
                    .context_limit()
                    .get()
                    .min(reported.get())
                    .min(OLLAMA_PROBE_CONTEXT_TOKENS)
            },
        );
        Self {
            num_ctx,
            num_predict: OLLAMA_PROBE_OUTPUT_TOKENS,
            temperature: 0.0,
            top_p: 1.0,
            stop: None,
        }
    }
}

#[derive(Serialize)]
struct OllamaRequestMessage<'a> {
    role: &'static str,
    content: &'a str,
}

const fn ollama_role(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
    }
}

type OllamaByteStream = BoxStream<'static, Result<Vec<u8>, reqwest::Error>>;

struct OllamaStreamState<'a> {
    body: OllamaByteStream,
    expected_model: ModelId,
    control: &'a dyn ModelOperationControl,
    buffer: Vec<u8>,
    queued: VecDeque<ProviderEvent>,
    completion: Option<ModelProviderCompletion>,
    output_bytes: usize,
    done_seen: bool,
    body_ended: bool,
}

impl<'a> OllamaStreamState<'a> {
    fn new(
        body: OllamaByteStream,
        expected_model: ModelId,
        control: &'a dyn ModelOperationControl,
    ) -> Self {
        Self {
            body,
            expected_model,
            control,
            buffer: Vec::new(),
            queued: VecDeque::new(),
            completion: None,
            output_bytes: 0,
            done_seen: false,
            body_ended: false,
        }
    }
}

async fn next_provider_event(
    mut state: OllamaStreamState<'_>,
) -> Result<Option<(ProviderEvent, OllamaStreamState<'_>)>, ModelProviderFailure> {
    loop {
        if let Some(event) = state.queued.pop_front() {
            return Ok(Some((event, state)));
        }
        if state.body_ended {
            return Ok(None);
        }
        if let Some(line) = take_complete_line(&mut state.buffer)? {
            parse_ollama_line(&mut state, &line)?;
            continue;
        }
        let next = read_body_or_cancel(&mut state).await?;
        match next {
            Some(bytes) => append_body_bytes(&mut state.buffer, &bytes)?,
            None => finish_body(&mut state)?,
        }
    }
}

async fn read_body_or_cancel(
    state: &mut OllamaStreamState<'_>,
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
    if buffer.len().saturating_add(bytes.len()) > MAX_OLLAMA_BUFFER_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn take_complete_line(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, ModelProviderFailure> {
    let Some(position) = buffer.iter().position(|byte| *byte == b'\n') else {
        if buffer.len() > MAX_OLLAMA_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        return Ok(None);
    };
    if position > MAX_OLLAMA_LINE_BYTES {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let mut line = buffer.drain(..=position).collect::<Vec<_>>();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn finish_body(state: &mut OllamaStreamState<'_>) -> Result<(), ModelProviderFailure> {
    if !state.buffer.is_empty() {
        if state.buffer.len() > MAX_OLLAMA_LINE_BYTES {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        let line = std::mem::take(&mut state.buffer);
        parse_ollama_line(state, &line)?;
    }
    let completion = state
        .completion
        .take()
        .ok_or(ModelProviderFailure::InvalidResponse)?;
    state.queued.push_back(ProviderEvent::Completed(completion));
    state.body_ended = true;
    Ok(())
}

fn parse_ollama_line(
    state: &mut OllamaStreamState<'_>,
    line: &[u8],
) -> Result<(), ModelProviderFailure> {
    if line.is_empty() || state.done_seen {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    let chunk = serde_json::from_slice::<OllamaChatChunk>(line)
        .map_err(|_| ModelProviderFailure::InvalidResponse)?;
    if chunk.model != state.expected_model.as_str()
        || chunk.message.role != "assistant"
        || chunk
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    if !chunk.message.content.is_empty() {
        state.output_bytes = state
            .output_bytes
            .checked_add(chunk.message.content.len())
            .filter(|total| *total <= MAX_OLLAMA_OUTPUT_BYTES)
            .ok_or(ModelProviderFailure::InvalidResponse)?;
        let output = ModelOutputChunk::try_from_string(chunk.message.content)
            .map_err(|_| ModelProviderFailure::InvalidResponse)?;
        state.queued.push_back(ProviderEvent::OutputText(output));
    }
    if chunk.done {
        state.done_seen = true;
        state.completion = Some(ModelProviderCompletion::new(
            finish_reason(chunk.done_reason.as_deref()),
            ModelProviderUsage::new(chunk.prompt_eval_count, chunk.eval_count),
        ));
    } else if chunk.done_reason.is_some() {
        return Err(ModelProviderFailure::InvalidResponse);
    }
    Ok(())
}

fn finish_reason(reason: Option<&str>) -> ModelFinishReason {
    match reason {
        Some("stop") => ModelFinishReason::Stop,
        Some("length") => ModelFinishReason::OutputLimit,
        Some(_) | None => ModelFinishReason::Other,
    }
}

#[derive(Deserialize)]
struct OllamaChatChunk {
    model: String,
    message: OllamaResponseMessage,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<Value>>,
}

/// Provider adapter construction failed without exposing endpoint or client details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OllamaProviderCreateError {
    /// The built-in stable provider identity violated its domain invariant.
    InvalidProviderIdentity,
    /// A redirect-free, proxy-free reusable client could not be built.
    HttpClient,
}

impl fmt::Display for OllamaProviderCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProviderIdentity => "Ollama provider identity is invalid",
            Self::HttpClient => "Ollama HTTP client could not be created",
        })
    }
}

impl Error for OllamaProviderCreateError {}

#[cfg(test)]
mod tests {
    use super::{
        MAX_OLLAMA_LINE_BYTES, OllamaStreamState, finish_body, finish_reason, parse_ollama_line,
        parse_show_observation, take_complete_line, valid_structured_probe_response,
    };
    use a3_application::{
        ModelCancellationFuture, ModelFinishReason, ModelOperationControl, ProviderEvent,
    };
    use a3_domain::ModelId;
    use futures::stream::{self, StreamExt};

    #[derive(Debug)]
    struct NeverCancelled;

    impl ModelOperationControl for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> ModelCancellationFuture<'_> {
            Box::pin(futures::future::pending())
        }
    }

    #[test]
    fn parser_rejects_model_mismatch_tool_calls_and_post_terminal_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let control = NeverCancelled;
        let body = stream::empty().boxed();
        let mut state = OllamaStreamState::new(
            body,
            ModelId::try_from_string("gemma3".to_owned())?,
            &control,
        );
        assert!(
            parse_ollama_line(
                &mut state,
                br#"{"model":"other","message":{"role":"assistant","content":"x"},"done":false}"#,
            )
            .is_err()
        );
        assert!(parse_ollama_line(
            &mut state,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"","tool_calls":[{}]},"done":false}"#,
        )
        .is_err());
        assert!(
            parse_ollama_line(
                &mut state,
                br#"{"model":"gemma3","message":{"role":"user","content":"x"},"done":false}"#,
            )
            .is_err()
        );
        assert!(parse_ollama_line(&mut state, br#"{"not":"a chat chunk"}"#).is_err());
        parse_ollama_line(
            &mut state,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"ok"},"done":true,"done_reason":"stop"}"#,
        )?;
        assert!(matches!(
            state.queued.front(),
            Some(ProviderEvent::OutputText(_))
        ));
        assert!(parse_ollama_line(
            &mut state,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"late"},"done":false}"#,
        )
        .is_err());
        assert_eq!(
            finish_reason(Some("length")),
            ModelFinishReason::OutputLimit
        );
        Ok(())
    }

    #[test]
    fn parser_rejects_missing_completion_and_oversized_lines()
    -> Result<(), Box<dyn std::error::Error>> {
        let control = NeverCancelled;
        let body = stream::empty().boxed();
        let mut state = OllamaStreamState::new(
            body,
            ModelId::try_from_string("gemma3".to_owned())?,
            &control,
        );
        parse_ollama_line(
            &mut state,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"partial"},"done":false}"#,
        )?;
        assert_eq!(
            finish_body(&mut state),
            Err(a3_application::ModelProviderFailure::InvalidResponse)
        );

        let mut oversized = vec![b'x'; MAX_OLLAMA_LINE_BYTES + 1];
        assert_eq!(
            take_complete_line(&mut oversized),
            Err(a3_application::ModelProviderFailure::InvalidResponse)
        );
        Ok(())
    }

    #[test]
    fn show_metadata_requires_one_unambiguous_context_limit_and_exact_tool_capability()
    -> Result<(), Box<dyn std::error::Error>> {
        let observed = parse_show_observation(
            br#"{
                "capabilities":["completion","tools"],
                "model_info":{"gemma3.context_length":32768}
            }"#,
        )?;
        assert_eq!(
            observed.context_limit.map(|limit| limit.get()),
            Some(32_768)
        );
        assert!(observed.tools_reported);
        assert!(
            parse_show_observation(
                br#"{
                    "capabilities":["tool-use"],
                    "model_info":{
                        "first.context_length":16384,
                        "second.context_length":32768
                    }
                }"#,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn structured_probe_requires_exact_model_role_completion_and_object()
    -> Result<(), Box<dyn std::error::Error>> {
        let model = ModelId::try_from_string("gemma3".to_owned())?;
        assert!(valid_structured_probe_response(
            br#"{
                "model":"gemma3",
                "message":{"role":"assistant","content":"{\"a3_probe\":\"ok\"}"},
                "done":true
            }"#,
            &model,
        ));
        assert!(!valid_structured_probe_response(
            br#"{
                "model":"gemma3",
                "message":{"role":"assistant","content":"{\"a3_probe\":\"ok\",\"extra\":true}"},
                "done":true
            }"#,
            &model,
        ));
        assert!(!valid_structured_probe_response(
            br#"{
                "model":"other",
                "message":{"role":"assistant","content":"{\"a3_probe\":\"ok\"}"},
                "done":true
            }"#,
            &model,
        ));
        Ok(())
    }
}
