//! Offline HTTP contract for the Ollama-compatible streaming adapter.

use a3_application::{
    ModelCancellationFuture, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderRequest,
    ModelRequestTimeout, ProviderEvent, StructuredOutputSchema,
};
use a3_domain::ModelId;
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
        ModelId::try_from_string("gemma3".to_owned())?,
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

fn endpoint_for(listener: &TcpListener) -> Result<OllamaEndpoint, TestError> {
    Ok(OllamaEndpoint::parse(&format!(
        "http://127.0.0.1:{}",
        listener.local_addr()?.port()
    ))?)
}

async fn read_request_body(stream: &mut TcpStream) -> Result<Vec<u8>, TestError> {
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
    Ok(received[header_end..header_end + content_length].to_vec())
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
