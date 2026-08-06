use crate::{OllamaEndpoint, OllamaEndpointPolicy};
use a3_application::{
    ModelFinishReason, ModelMessageRole, ModelOperationControl, ModelOutputChunk, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture, ModelProviderRequest,
    ModelProviderUsage, ModelRequestTimeout, ProviderEvent, ProviderEventStream,
};
use a3_domain::{ModelId, ModelProviderId};
use futures::future::{Either, select};
use futures::stream::{BoxStream, StreamExt};
use futures::{FutureExt, pin_mut};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const OLLAMA_PROVIDER_ID: &str = "ollama";
const OLLAMA_CONTENT_TYPE: &str = "application/x-ndjson";
const MAX_OLLAMA_BUFFER_BYTES: usize = 256 * 1024;
const MAX_OLLAMA_LINE_BYTES: usize = 128 * 1024;
const MAX_OLLAMA_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

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
        take_complete_line,
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
}
