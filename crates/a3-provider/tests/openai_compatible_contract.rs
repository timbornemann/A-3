//! Offline HTTP contract for the configurable OpenAI-compatible provider adapter.

use a3_application::{
    DiscoverProviderModels, EmbeddingCapabilityProbeRequest, ModelCancellationFuture,
    ModelCapabilityProbe, ModelCapabilityProbeRequest, ModelFinishReason, ModelMessage,
    ModelMessageRole, ModelOperationControl, ModelOutputChunk, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderRequest, ModelProviderUsage,
    ModelRequestTimeout, ProbeEmbeddingModelProfile, ProbeModelProfile, ProviderApiKey,
    ProviderEvent, StructuredOutputSchema,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingModelId, ModelCapabilities, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP,
};
use a3_model_provider_contract_tests::verify_model_provider_stream;
use a3_provider::{
    LocalOnlyOpenAiCompatibleEndpointPolicy, OpenAiCompatibleEndpoint,
    OpenAiCompatibleModelProvider,
};
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
            if self.cancelled.load(Ordering::Acquire) {
                return Poll::Ready(());
            }
            self.waiter.register(context.waker());
            if self.cancelled.load(Ordering::Acquire) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_adapter_passes_stream_contract_and_encodes_chat_completions()
-> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        let events = [
            format!(
                "data: {}\n\n",
                json!({
                    "object": "chat.completion.chunk",
                    "model": "openai/gpt-fixture",
                    "choices": [{
                        "index": 0,
                        "delta": {"role": "assistant", "content": "Hello "},
                        "finish_reason": null
                    }]
                })
            ),
            format!(
                "data: {}\n\n",
                json!({
                    "object": "chat.completion.chunk",
                    "model": "openai/gpt-fixture",
                    "choices": [{
                        "index": 0,
                        "delta": {"content": "world!"},
                        "finish_reason": null
                    }]
                })
            ),
            format!(
                "data: {}\n\n",
                json!({
                    "object": "chat.completion.chunk",
                    "model": "openai/gpt-fixture",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
                })
            ),
            "data: [DONE]\n\n".to_owned(),
        ];
        write_http_chunk(&mut stream, events.concat().as_bytes()).await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("openai/gpt-fixture", true)?;
    let expected = vec![
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string("Hello ".to_owned())?),
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string("world!".to_owned())?),
        ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            ModelProviderUsage::new(None, None),
        )),
    ];
    verify_model_provider_stream(
        &provider,
        &request,
        timeout()?,
        &TestControl::default(),
        &expected,
    )
    .await?;

    let wire = server.await??;
    assert_eq!(wire.method, "POST");
    assert_eq!(wire.path, "/custom/api/v1/chat/completions");
    assert_eq!(
        wire.header("authorization"),
        Some("Bearer compatible-test-key")
    );
    assert_eq!(wire.header("user-agent"), Some("a3/0.1.0"));
    let payload: Value = serde_json::from_slice(&wire.body)?;
    assert_eq!(payload["model"], "openai/gpt-fixture");
    assert_eq!(payload["stream"], true);
    assert_eq!(payload["messages"][0]["role"], "system");
    assert_eq!(payload["messages"][1]["role"], "user");
    assert_eq!(payload["max_tokens"], 2048);
    assert_eq!(payload["response_format"]["type"], "json_schema");
    assert_eq!(payload["response_format"]["json_schema"]["strict"], true);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_catalog_and_capability_probe_are_observed_not_inferred() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut catalog_stream, _) = listener.accept().await?;
        let catalog_request = read_http_request(&mut catalog_stream).await?;
        write_json_response(
            &mut catalog_stream,
            "200 OK",
            br#"{"object":"list","data":[
              {"id":"llama-3.3-70b-versatile","object":"model"},
              {"id":"openai/gpt-fixture","object":"model"}
            ]}"#,
        )
        .await?;

        let (mut probe_stream, _) = listener.accept().await?;
        let probe_request = read_http_request(&mut probe_stream).await?;
        write_json_response(
            &mut probe_stream,
            "200 OK",
            br#"{"object":"chat.completion","model":"openai/gpt-fixture","choices":[
              {"index":0,"message":{"role":"assistant","content":"{\"a3_probe\":\"ok\"}"},"finish_reason":"stop"}
            ]}"#,
        )
        .await?;
        Ok::<(StubHttpRequest, StubHttpRequest), TestError>((catalog_request, probe_request))
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let catalog = DiscoverProviderModels::new(&provider)
        .execute(timeout()?, &control)
        .await?;
    assert_eq!(
        catalog
            .model_ids()
            .iter()
            .map(|model| model.as_str())
            .collect::<Vec<_>>(),
        vec!["llama-3.3-70b-versatile", "openai/gpt-fixture"]
    );
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &ModelCapabilityProbeRequest::new(
                ModelId::try_from_string("openai/gpt-fixture".to_owned())?,
                sample_settings()?,
            ),
            timeout()?,
            &control,
        )
        .await?;
    assert_eq!(
        profile.capabilities().structured_output(),
        ModelStructuredOutputCapability::Verified
    );

    let (catalog_request, probe_request) = server.await??;
    assert_eq!(catalog_request.method, "GET");
    assert_eq!(catalog_request.path, "/custom/api/v1/models");
    assert_eq!(probe_request.path, "/custom/api/v1/chat/completions");
    let probe_payload: Value = serde_json::from_slice(&probe_request.body)?;
    assert_eq!(probe_payload["stream"], false);
    assert_eq!(
        probe_payload["response_format"]["json_schema"]["name"],
        "a3_capability_probe"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_embedding_probe_validates_dimension_and_base_path() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "200 OK",
            br#"{"object":"list","model":"embed-fixture","data":[
              {"object":"embedding","embedding":[0.25,-0.5,0.75],"index":0}
            ]}"#,
        )
        .await?;
        Ok::<StubHttpRequest, TestError>(request)
    });
    let provider = test_provider(endpoint)?;
    let profile = ProbeEmbeddingModelProfile::new(&provider)
        .execute(
            &EmbeddingCapabilityProbeRequest::new(
                EmbeddingModelId::new("embed-fixture".to_owned())?,
                EmbeddingBatchSize::new(8)?,
            ),
            timeout()?,
            &TestControl::default(),
        )
        .await?;
    assert_eq!(profile.dimension().get(), 3);
    let request = server.await??;
    assert_eq!(request.path, "/custom/api/v1/embeddings");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_redirects_are_not_followed_or_given_the_api_key() -> Result<(), TestError> {
    let redirected_listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirected_address = redirected_listener.local_addr()?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{redirected_address}/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let result = DiscoverProviderModels::new(&provider)
        .execute(timeout()?, &TestControl::default())
        .await;
    assert_eq!(result, Err(ModelProviderFailure::Rejected));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), redirected_listener.accept())
            .await
            .is_err(),
        "redirect target must not receive a request"
    );
    let request = server.await??;
    assert_eq!(
        request.header("authorization"),
        Some("Bearer compatible-test-key")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_pre_cancel_and_timeout_stop_bounded_discovery() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let provider = test_provider(endpoint)?;
    let cancelled = TestControl::default();
    cancelled.cancel();
    assert_eq!(
        DiscoverProviderModels::new(&provider)
            .execute(timeout()?, &cancelled)
            .await,
        Err(ModelProviderFailure::Cancelled)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "pre-cancelled discovery must not cross the network boundary"
    );

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok::<(), TestError>(())
    });
    assert_eq!(
        DiscoverProviderModels::new(&provider)
            .execute(
                ModelRequestTimeout::from_millis(25)?,
                &TestControl::default(),
            )
            .await,
        Err(ModelProviderFailure::TimedOut)
    );
    server.abort();
    let _ = server.await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_stream_body_read_is_wakeably_cancelled() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        let prefix = format!(
            "data: {}\n\n",
            json!({
                "object": "chat.completion.chunk",
                "model": "openai/gpt-fixture",
                "choices": [{
                    "index": 0,
                    "delta": {"role": "assistant", "content": "partial"},
                    "finish_reason": null
                }]
            })
        );
        write_http_chunk(&mut stream, prefix.as_bytes()).await?;
        let _ = ready_sender.send(());
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("openai/gpt-fixture", false)?;
    let control = TestControl::default();
    let mut events = provider.stream(&request, timeout()?, &control).await?;
    ready_receiver.await?;
    assert!(matches!(
        events.next().await,
        Some(Ok(ProviderEvent::OutputText(_)))
    ));
    control.cancel();
    assert_eq!(
        events.next().await,
        Some(Err(ModelProviderFailure::Cancelled))
    );
    server.abort();
    let _ = server.await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_http_errors_are_content_free_and_classified() -> Result<(), TestError> {
    for (status, expected) in [
        ("401 Unauthorized", ModelProviderFailure::Rejected),
        ("429 Too Many Requests", ModelProviderFailure::Unavailable),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = endpoint_for(&listener)?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let _ = read_http_request(&mut stream).await?;
            write_json_response(
                &mut stream,
                status,
                br#"{"error":{"message":"sensitive provider detail"}}"#,
            )
            .await?;
            Ok::<(), TestError>(())
        });
        let provider = test_provider(endpoint)?;
        let result = DiscoverProviderModels::new(&provider)
            .execute(timeout()?, &TestControl::default())
            .await;
        assert_eq!(result, Err(expected));
        assert!(!expected.to_string().contains("sensitive provider detail"));
        server.await??;
    }

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "401 Unauthorized",
            br#"{"error":{"message":"invalid key"}}"#,
        )
        .await?;
        Ok::<(), TestError>(())
    });
    let provider = test_provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("openai/gpt-fixture".to_owned())?,
        sample_settings()?,
    );
    assert_eq!(
        provider
            .probe(&probe_request, timeout()?, &TestControl::default())
            .await,
        Err(ModelProviderFailure::Rejected),
        "authentication failures must not become capability-limited profiles"
    );
    server.await??;
    Ok(())
}

fn endpoint_for(listener: &TcpListener) -> Result<OpenAiCompatibleEndpoint, TestError> {
    Ok(OpenAiCompatibleEndpoint::parse(&format!(
        "http://{}/custom/api/v1",
        listener.local_addr()?
    ))?)
}

fn test_provider(
    endpoint: OpenAiCompatibleEndpoint,
) -> Result<OpenAiCompatibleModelProvider, TestError> {
    Ok(OpenAiCompatibleModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyOpenAiCompatibleEndpointPolicy),
        ProviderApiKey::from_bytes(b"compatible-test-key".to_vec())?,
    )?)
}

fn timeout() -> Result<ModelRequestTimeout, TestError> {
    Ok(ModelRequestTimeout::from_millis(2_000)?)
}

fn sample_settings() -> Result<ModelProfileSettings, TestError> {
    Ok(ModelProfileSettings::new(
        ModelContextLimit::new(16_384)?,
        ModelOutputLimit::new(2_048)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(700)?,
            ModelTopP::from_milli(900)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )?)
}

fn sample_request(model_id: &str, structured: bool) -> Result<ModelProviderRequest, TestError> {
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("openai-compatible".to_owned())?,
        ModelId::try_from_string(model_id.to_owned())?,
        sample_settings()?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    );
    let messages = vec![
        ModelMessage::try_from_string(ModelMessageRole::System, "System instruction".to_owned())?,
        ModelMessage::try_from_string(ModelMessageRole::User, "User question".to_owned())?,
    ];
    let schema = structured
        .then(|| {
            StructuredOutputSchema::new(json!({
                "type": "object",
                "properties": {"answer": {"type": "string"}},
                "required": ["answer"],
                "additionalProperties": false
            }))
        })
        .transpose()?;
    Ok(ModelProviderRequest::new(profile, messages, schema)?)
}

#[derive(Debug)]
struct StubHttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl StubHttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

async fn read_http_request(stream: &mut TcpStream) -> Result<StubHttpRequest, TestError> {
    let mut buffer = Vec::new();
    let header_end = loop {
        if buffer.len() > MAX_STUB_REQUEST_BYTES {
            return Err("stub request exceeded boundary".into());
        }
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Err("stub request ended before headers".into());
        }
        buffer.extend_from_slice(&chunk[..count]);
    };
    let header_text = std::str::from_utf8(&buffer[..header_end])?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().ok_or("missing method")?.to_owned();
    let path = request_parts.next().ok_or("missing path")?.to_owned();
    let headers = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (name, value) = line.split_once(':').ok_or("invalid header")?;
            Ok::<_, TestError>((name.to_owned(), value.trim().to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    while buffer.len() < header_end.saturating_add(content_length) {
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Err("stub request ended before body".into());
        }
        buffer.extend_from_slice(&chunk[..count]);
    }
    Ok(StubHttpRequest {
        method,
        path,
        headers,
        body: buffer[header_end..header_end + content_length].to_vec(),
    })
}

async fn write_event_stream_head(stream: &mut TcpStream) -> std::io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        )
        .await
}

async fn write_http_chunk(stream: &mut TcpStream, chunk: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
        .await?;
    stream.write_all(chunk).await?;
    stream.write_all(b"\r\n").await
}

async fn finish_http_chunks(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"0\r\n\r\n").await
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
    stream.write_all(body).await
}
