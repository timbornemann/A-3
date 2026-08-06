//! Offline HTTP contract for the Ollama-compatible streaming adapter.

use a3_application::{
    ModelCancellationFuture, ModelCapabilityProbeRequest, ModelFinishReason, ModelMessage,
    ModelMessageRole, ModelOperationControl, ModelProvider, ModelProviderFailure,
    ModelProviderRequest, ModelRequestTimeout, ProbeModelProfile, ProbeModelProfileFailure,
    ProviderEvent, StructuredOutputSchema,
};
use a3_domain::{
    ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
    ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
    ModelSamplingProfile, ModelStopSequence, ModelStopSequences, ModelStructuredOutputCapability,
    ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use futures::StreamExt;
use futures::task::AtomicWaker;
use serde_json::{Value, json};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_STUB_REQUEST_BYTES: usize = 256 * 1024;
type TestError = Box<dyn Error + Send + Sync>;

#[derive(Default)]
struct TestControl {
    cancelled: AtomicBool,
    waiter: AtomicWaker,
}

impl TestControl {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.waiter.wake();
    }
}

impl std::fmt::Debug for TestControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestControl")
            .field("cancelled", &self.cancelled.load(Ordering::Acquire))
            .finish()
    }
}

impl ModelOperationControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ollama_adapter_streams_neutral_events_and_encodes_strict_request() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let body = read_request_body(&mut stream).await?;
        write_chunked_head(&mut stream).await?;
        write_http_chunk(&mut stream, br#"{"model":"gem"#).await?;
        write_http_chunk(
            &mut stream,
            br#"ma3","message":{"role":"assistant","content":"hel"},"done":false}
{"model":"gemma3","message":{"role":"assistant","content":"lo"},"done":false}
"#,
        )
        .await?;
        write_http_chunk(
            &mut stream,
            br#"{"model":"gemma3","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":7,"eval_count":2}
"#,
        )
        .await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<Vec<u8>, TestError>(body)
    });
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let request = request()?;
    let events = provider
        .stream(&request, ModelRequestTimeout::from_millis(2_000)?, &control)
        .await?
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let body = server.await??;

    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        ProviderEvent::OutputText(chunk) if chunk.as_str() == "hel"
    ));
    assert!(matches!(
        &events[1],
        ProviderEvent::OutputText(chunk) if chunk.as_str() == "lo"
    ));
    assert!(matches!(
        events[2],
        ProviderEvent::Completed(completion)
            if completion.reason() == ModelFinishReason::Stop
                && completion.usage().prompt_tokens() == Some(7)
                && completion.usage().output_tokens() == Some(2)
    ));
    assert_eq!(provider.provider_id().as_str(), "ollama");
    let encoded = serde_json::from_slice::<Value>(&body)?;
    assert_eq!(encoded["model"], "gemma3");
    assert_eq!(encoded["stream"], true);
    assert_eq!(encoded["think"], false);
    assert_eq!(encoded["messages"][0]["role"], "system");
    assert_eq!(encoded["messages"][1]["content"], "inspect the repository");
    assert_eq!(encoded["format"]["additionalProperties"], false);
    assert_eq!(encoded["options"]["num_ctx"], 16_384);
    assert_eq!(encoded["options"]["num_predict"], 2_048);
    assert_eq!(encoded["options"]["temperature"], 0.25);
    assert_eq!(encoded["options"]["top_p"], 0.9);
    assert_eq!(encoded["options"]["stop"][0], "END");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_cancellation_drops_the_in_flight_response() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let (first_chunk_sender, first_chunk_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        read_request_body(&mut stream).await?;
        write_chunked_head(&mut stream).await?;
        write_http_chunk(
            &mut stream,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"first"},"done":false}
"#,
        )
        .await?;
        let _ = first_chunk_sender.send(());
        let mut byte = [0_u8; 1];
        let disconnected = matches!(
            tokio::time::timeout(Duration::from_secs(2), stream.read(&mut byte)).await,
            Ok(Ok(0)) | Ok(Err(_))
        );
        Ok::<bool, TestError>(disconnected)
    });
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let request = request()?;
    let mut events = provider
        .stream(&request, ModelRequestTimeout::from_millis(5_000)?, &control)
        .await?;
    first_chunk_receiver.await?;
    assert!(matches!(
        events.next().await,
        Some(Ok(ProviderEvent::OutputText(_)))
    ));
    control.cancel();
    assert_eq!(
        events.next().await,
        Some(Err(ModelProviderFailure::Cancelled))
    );
    drop(events);
    assert!(
        server.await??,
        "server must observe the cancelled response closing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn total_timeout_is_normalized_before_response_headers() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        read_request_body(&mut stream).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        Ok::<(), TestError>(())
    });
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let request = request()?;

    assert!(matches!(
        provider
            .stream(&request, ModelRequestTimeout::from_millis(50)?, &control)
            .await,
        Err(ModelProviderFailure::TimedOut)
    ));
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn total_timeout_remains_active_while_streaming_the_response_body() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        read_request_body(&mut stream).await?;
        write_chunked_head(&mut stream).await?;
        write_http_chunk(
            &mut stream,
            br#"{"model":"gemma3","message":{"role":"assistant","content":"first"},"done":false}
"#,
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        Ok::<(), TestError>(())
    });
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let request = request()?;
    let mut events = provider
        .stream(&request, ModelRequestTimeout::from_millis(75)?, &control)
        .await?;

    assert!(matches!(
        events.next().await,
        Some(Ok(ProviderEvent::OutputText(_)))
    ));
    assert_eq!(
        events.next().await,
        Some(Err(ModelProviderFailure::TimedOut))
    );
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capability_probe_uses_show_metadata_and_a_real_strict_schema_request()
-> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut show_stream, _) = listener.accept().await?;
        let show_request = read_http_request(&mut show_stream).await?;
        let show_response = serde_json::to_vec(&json!({
            "capabilities": ["completion", "tools"],
            "model_info": {"gemma3.context_length": 32_768}
        }))?;
        write_json_response(&mut show_stream, "200 OK", &show_response).await?;
        drop(show_stream);

        let (mut chat_stream, _) = listener.accept().await?;
        let chat_request = read_http_request(&mut chat_stream).await?;
        let chat_response = serde_json::to_vec(&json!({
            "model": "gemma3",
            "message": {
                "role": "assistant",
                "content": "{\"a3_probe\":\"ok\"}"
            },
            "done": true,
            "done_reason": "stop"
        }))?;
        write_json_response(&mut chat_stream, "200 OK", &chat_response).await?;
        Ok::<(StubHttpRequest, StubHttpRequest), TestError>((show_request, chat_request))
    });
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemma3".to_owned())?,
        ollama_settings()?,
    );
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(2_000)?,
            &control,
        )
        .await?;
    let (show_request, chat_request) = server.await??;

    assert!(profile.executable_actions_enabled());
    assert_eq!(
        profile.capabilities().tool_call_mode(),
        ModelToolCallMode::NativeProviderReported
    );
    assert_eq!(show_request.path, "/api/show");
    let show_body = serde_json::from_slice::<Value>(&show_request.body)?;
    assert_eq!(show_body, json!({"model": "gemma3", "verbose": false}));
    assert_eq!(chat_request.path, "/api/chat");
    let chat_body = serde_json::from_slice::<Value>(&chat_request.body)?;
    assert_eq!(chat_body["model"], "gemma3");
    assert_eq!(chat_body["stream"], false);
    assert_eq!(chat_body["think"], false);
    assert_eq!(chat_body["options"]["num_ctx"], 4_096);
    assert_eq!(chat_body["options"]["num_predict"], 32);
    assert_eq!(chat_body["options"]["temperature"], 0.0);
    assert_eq!(chat_body["options"]["top_p"], 1.0);
    assert_eq!(chat_body["format"]["additionalProperties"], false);
    assert_eq!(chat_body["format"]["properties"]["a3_probe"]["const"], "ok");
    assert!(
        chat_body["messages"][0]["content"]
            .as_str()
            .is_some_and(|content| content.contains("{\"a3_probe\":\"ok\"}"))
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_structured_probe_output_creates_a_non_executable_profile() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut show_stream, _) = listener.accept().await?;
        read_http_request(&mut show_stream).await?;
        let show_response = serde_json::to_vec(&json!({
            "capabilities": ["tools"],
            "model_info": {"gemma3.context_length": 32_768}
        }))?;
        write_json_response(&mut show_stream, "200 OK", &show_response).await?;
        drop(show_stream);

        let (mut chat_stream, _) = listener.accept().await?;
        read_http_request(&mut chat_stream).await?;
        let chat_response = serde_json::to_vec(&json!({
            "model": "gemma3",
            "message": {
                "role": "assistant",
                "content": "{\"a3_probe\":\"wrong\",\"extra\":true}"
            },
            "done": true
        }))?;
        write_json_response(&mut chat_stream, "200 OK", &chat_response).await?;
        Ok::<(), TestError>(())
    });
    let provider = provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemma3".to_owned())?,
        ollama_settings()?,
    );
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(2_000)?,
            &TestControl::default(),
        )
        .await?;

    assert!(!profile.executable_actions_enabled());
    assert_eq!(
        profile.capabilities().structured_output(),
        ModelStructuredOutputCapability::Unavailable
    );
    assert_eq!(
        profile.capabilities().tool_call_mode(),
        ModelToolCallMode::NativeProviderReported
    );
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capability_probe_enforces_one_total_deadline_across_show_and_chat() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut show_stream, _) = listener.accept().await?;
        read_http_request(&mut show_stream).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        let show_response = serde_json::to_vec(&json!({
            "model_info": {"gemma3.context_length": 32_768}
        }))?;
        write_json_response(&mut show_stream, "200 OK", &show_response).await?;
        drop(show_stream);

        let (mut chat_stream, _) = listener.accept().await?;
        read_http_request(&mut chat_stream).await?;
        let mut byte = [0_u8; 1];
        let disconnected = matches!(
            tokio::time::timeout(Duration::from_secs(2), chat_stream.read(&mut byte)).await,
            Ok(Ok(0)) | Ok(Err(_))
        );
        Ok::<bool, TestError>(disconnected)
    });
    let provider = provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemma3".to_owned())?,
        ollama_settings()?,
    );
    let started = std::time::Instant::now();
    let result = ProbeModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(600)?,
            &TestControl::default(),
        )
        .await;
    let elapsed = started.elapsed();

    assert!(matches!(
        result,
        Err(ProbeModelProfileFailure::Provider(
            ModelProviderFailure::TimedOut
        ))
    ));
    assert!(elapsed < Duration::from_millis(750));
    assert!(
        server.await??,
        "server must observe the timed-out probe closing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_cancelled_capability_probe_never_crosses_the_network_boundary() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    control.cancel();
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemma3".to_owned())?,
        ollama_settings()?,
    );

    assert!(matches!(
        ProbeModelProfile::new(&provider)
            .execute(
                &probe_request,
                ModelRequestTimeout::from_millis(2_000)?,
                &control,
            )
            .await,
        Err(ProbeModelProfileFailure::Provider(
            ModelProviderFailure::Cancelled
        ))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(75), listener.accept())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denied_remote_endpoint_fails_before_any_network_attempt() -> Result<(), TestError> {
    let endpoint = OllamaEndpoint::parse("https://models.example.invalid")?;
    let provider = provider(endpoint)?;
    let control = TestControl::default();
    let request = request()?;

    assert!(matches!(
        provider
            .stream(&request, ModelRequestTimeout::from_millis(2_000)?, &control)
            .await,
        Err(ModelProviderFailure::EndpointDenied)
    ));
    Ok(())
}

fn provider(endpoint: OllamaEndpoint) -> Result<OllamaModelProvider, TestError> {
    Ok(OllamaModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?)
}

fn request() -> Result<ModelProviderRequest, TestError> {
    Ok(ModelProviderRequest::new(
        ollama_profile()?,
        vec![
            ModelMessage::try_from_string(
                ModelMessageRole::System,
                "return one bounded object".to_owned(),
            )?,
            ModelMessage::try_from_string(
                ModelMessageRole::User,
                "inspect the repository".to_owned(),
            )?,
        ],
        Some(StructuredOutputSchema::new(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"],
            "additionalProperties": false
        }))?),
    )?)
}

fn ollama_profile() -> Result<ModelProfile, TestError> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("gemma3".to_owned())?,
        ollama_settings()?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

fn ollama_settings() -> Result<ModelProfileSettings, TestError> {
    Ok(ModelProfileSettings::new(
        ModelContextLimit::new(16_384)?,
        ModelOutputLimit::new(2_048)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(250)?,
            ModelTopP::from_milli(900)?,
        ),
        ModelStopSequences::new(vec![ModelStopSequence::try_from_string("END".to_owned())?])?,
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )?)
}

fn endpoint_for(listener: &TcpListener) -> Result<OllamaEndpoint, TestError> {
    Ok(OllamaEndpoint::parse(&format!(
        "http://127.0.0.1:{}",
        listener.local_addr()?.port()
    ))?)
}

async fn read_request_body(stream: &mut TcpStream) -> Result<Vec<u8>, TestError> {
    Ok(read_http_request(stream).await?.body)
}

struct StubHttpRequest {
    path: String,
    body: Vec<u8>,
}

async fn read_http_request(stream: &mut TcpStream) -> Result<StubHttpRequest, TestError> {
    let mut received = Vec::new();
    let header_end = loop {
        if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if received.len() >= MAX_STUB_REQUEST_BYTES {
            return Err(std::io::Error::other("stub request headers exceeded limit").into());
        }
        let mut buffer = [0_u8; 4096];
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(std::io::Error::other("stub request ended before headers").into());
        }
        received.extend_from_slice(&buffer[..read]);
    };
    let header = std::str::from_utf8(&received[..header_end])?;
    let path = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| std::io::Error::other("stub request line was invalid"))?
        .to_owned();
    let content_length = header
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>())
            })
        })
        .transpose()?
        .ok_or_else(|| std::io::Error::other("stub request omitted content-length"))?;
    if content_length > MAX_STUB_REQUEST_BYTES {
        return Err(std::io::Error::other("stub request body exceeded limit").into());
    }
    while received.len().saturating_sub(header_end) < content_length {
        let mut buffer = [0_u8; 4096];
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(std::io::Error::other("stub request body ended early").into());
        }
        received.extend_from_slice(&buffer[..read]);
        if received.len().saturating_sub(header_end) > MAX_STUB_REQUEST_BYTES {
            return Err(std::io::Error::other("stub request body exceeded limit").into());
        }
    }
    Ok(StubHttpRequest {
        path,
        body: received[header_end..header_end + content_length].to_vec(),
    })
}

async fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    body: &[u8],
) -> std::io::Result<()> {
    stream
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn write_chunked_head(stream: &mut TcpStream) -> std::io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n",
        )
        .await
}

async fn write_http_chunk(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
        .await?;
    stream.write_all(bytes).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await
}

async fn finish_http_chunks(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"0\r\n\r\n").await?;
    stream.flush().await
}
