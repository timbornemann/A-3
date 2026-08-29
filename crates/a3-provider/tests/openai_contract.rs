//! Offline HTTP contract for the native OpenAI provider adapter.

use a3_application::{
    DiscoverProviderModels, EmbeddingCapabilityProbeRequest, EmbeddingOperationControl,
    EmbeddingProvider, EmbeddingRequestTimeout, ModelCancellationFuture,
    ModelCapabilityProbeRequest, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelOutputChunk, ModelProvider, ModelProviderCompletion,
    ModelProviderFailure, ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout,
    ProbeEmbeddingModelProfile, ProbeModelProfile, ProviderApiKey, ProviderEvent,
    StructuredOutputSchema,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingModelId, ModelCapabilities, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP, NormalizedSemanticCard, SemanticCardId, SnapshotId,
};
use a3_model_provider_contract_tests::verify_model_provider_stream;
use a3_provider::{
    LocalOnlyOpenAiEndpointPolicy, OpenAiEndpoint, OpenAiModelProvider,
    StandardOpenAiEndpointPolicy,
};
use futures::StreamExt;
use futures::task::AtomicWaker;
use serde_json::{Value, json};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
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
            if ModelOperationControl::is_cancelled(self) {
                return Poll::Ready(());
            }
            self.waiter.register(context.waker());
            if ModelOperationControl::is_cancelled(self) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }))
    }
}

impl EmbeddingOperationControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_adapter_discovers_a_bounded_role_candidate_catalog() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "200 OK",
            br#"{"object":"list","data":[
                {"id":"whisper-1","object":"model","created":1,"owned_by":"openai"},
                {"id":"gpt-5.4","object":"model","created":2,"owned_by":"openai"},
                {"id":"text-embedding-3-small","object":"model","created":3,"owned_by":"openai"},
                {"id":"gpt-5.4","object":"model","created":2,"owned_by":"openai"}
            ]}"#,
        )
        .await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let catalog = DiscoverProviderModels::new(&provider)
        .execute(timeout()?, &control)
        .await
        .map_err(map_app_error)?;
    let request = server.await??;

    assert_eq!(request.path, "/v1/models");
    assert_eq!(
        request.header("authorization"),
        Some("Bearer test-openai-key")
    );
    assert_eq!(request.header("user-agent"), Some("a3/0.1.0"));
    assert_eq!(
        catalog
            .model_ids()
            .iter()
            .map(|model| model.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5.4", "text-embedding-3-small"]
    );
    assert!(!catalog.truncated());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_adapter_streams_neutral_events_and_encodes_responses_request()
-> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;

        let events = [
            sse_response_event("response.created", "in_progress", None),
            sse_event(
                "response.output_item.added",
                json!({
                    "output_index": 0,
                    "item": {"id":"msg_1","type":"message","status":"in_progress","role":"assistant","content":[]}
                }),
            ),
            sse_event(
                "response.content_part.added",
                json!({
                    "item_id":"msg_1","output_index":0,"content_index":0,
                    "part":{"type":"output_text","text":"","annotations":[]}
                }),
            ),
            sse_event(
                "response.output_text.delta",
                json!({"item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hello "}),
            ),
            sse_event(
                "response.output_text.delta",
                json!({"item_id":"msg_1","output_index":0,"content_index":0,"delta":"world!"}),
            ),
            sse_event(
                "response.output_text.done",
                json!({"item_id":"msg_1","output_index":0,"content_index":0,"text":"Hello world!"}),
            ),
            sse_event(
                "response.content_part.done",
                json!({
                    "item_id":"msg_1","output_index":0,"content_index":0,
                    "part":{"type":"output_text","text":"Hello world!","annotations":[]}
                }),
            ),
            sse_event(
                "response.output_item.done",
                json!({
                    "output_index":0,
                    "item":{"id":"msg_1","type":"message","status":"completed","role":"assistant","content":[]}
                }),
            ),
            sse_response_event("response.completed", "completed", Some((12, 4))),
        ];
        let body = events.concat();
        let split = body.len() / 2;
        write_http_chunk(&mut stream, &body.as_bytes()[..split]).await?;
        write_http_chunk(&mut stream, &body.as_bytes()[split..]).await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gpt-5.4", true)?;
    let control = TestControl::default();
    let expected = vec![
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string("Hello ".to_owned())?),
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string("world!".to_owned())?),
        ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            ModelProviderUsage::new(Some(12), Some(4)),
        )),
    ];
    verify_model_provider_stream(&provider, &request, timeout()?, &control, &expected).await?;

    let wire = server.await??;
    assert_eq!(wire.path, "/v1/responses");
    assert_eq!(wire.header("authorization"), Some("Bearer test-openai-key"));
    let payload: Value = serde_json::from_slice(&wire.body)?;
    assert_eq!(payload["model"], "gpt-5.4");
    assert_eq!(payload["stream"], true);
    assert_eq!(payload["store"], false);
    assert_eq!(payload["tools"], json!([]));
    assert_eq!(payload["tool_choice"], "none");
    assert_eq!(payload["parallel_tool_calls"], false);
    assert_eq!(payload["truncation"], "disabled");
    assert_eq!(payload["input"][0]["role"], "system");
    assert_eq!(payload["input"][0]["content"], "System instruction");
    assert_eq!(payload["input"][1]["role"], "user");
    assert_eq!(payload["temperature"], 0.7);
    assert_eq!(payload["top_p"], 0.9);
    assert_eq!(payload["reasoning"]["effort"], "none");
    assert_eq!(payload["max_output_tokens"], 2048);
    assert_eq!(payload["text"]["format"]["type"], "json_schema");
    assert_eq!(payload["text"]["format"]["name"], "a3_response");
    assert_eq!(payload["text"]["format"]["strict"], true);
    let schema = &payload["text"]["format"]["schema"];
    assert!(schema.is_object());
    for unsupported in ["const", "oneOf", "prefixItems", "uniqueItems"] {
        assert!(
            !contains_schema_key(schema, unsupported),
            "OpenAI wire schema retained unsupported keyword {unsupported}"
        );
    }
    assert_eq!(schema["properties"]["schema_version"]["enum"], json!([1]));
    assert!(schema["properties"]["action"]["anyOf"].is_array());
    assert_eq!(
        schema["properties"]["action"]["anyOf"][0]["properties"]["observations"]["items"]["enum"],
        json!(["observation-a", "observation-b"])
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_adapter_streams_plain_agent_conversation_without_a_schema() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;

        let events = [
            sse_response_event("response.created", "in_progress", None),
            sse_event(
                "response.output_item.added",
                json!({
                    "output_index":0,
                    "item":{"id":"msg_agent","type":"message","status":"in_progress","role":"assistant","content":[]}
                }),
            ),
            sse_event(
                "response.content_part.added",
                json!({
                    "item_id":"msg_agent","output_index":0,"content_index":0,
                    "part":{"type":"output_text","text":"","annotations":[]}
                }),
            ),
            sse_event(
                "response.output_text.delta",
                json!({"item_id":"msg_agent","output_index":0,"content_index":0,"delta":"Agent reply"}),
            ),
            sse_event(
                "response.output_text.done",
                json!({"item_id":"msg_agent","output_index":0,"content_index":0,"text":"Agent reply"}),
            ),
            sse_event(
                "response.content_part.done",
                json!({
                    "item_id":"msg_agent","output_index":0,"content_index":0,
                    "part":{"type":"output_text","text":"Agent reply","annotations":[]}
                }),
            ),
            sse_event(
                "response.output_item.done",
                json!({
                    "output_index":0,
                    "item":{"id":"msg_agent","type":"message","status":"completed","role":"assistant","content":[]}
                }),
            ),
            sse_response_event("response.completed", "completed", Some((10, 2))),
        ];
        write_http_chunk(&mut stream, events.concat().as_bytes()).await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gpt-5.4", false)?;
    let control = TestControl::default();
    let expected = vec![
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string("Agent reply".to_owned())?),
        ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            ModelProviderUsage::new(Some(10), Some(2)),
        )),
    ];
    verify_model_provider_stream(&provider, &request, timeout()?, &control, &expected).await?;

    let wire = server.await??;
    let payload: Value = serde_json::from_slice(&wire.body)?;
    assert_eq!(wire.path, "/v1/responses");
    assert_eq!(payload["model"], "gpt-5.4");
    assert_eq!(payload["input"][1]["content"], "Hello assistant");
    assert!(payload.get("text").is_none());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capability_probe_requires_a_real_strict_schema_response() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        let response = response_summary("completed", Some((8, 3)));
        let body = json!({
            "id":"resp_probe",
            "created_at":1,
            "output":[{
                "id":"msg_probe","type":"message","status":"completed","role":"assistant",
                "content":[{"type":"output_text","text":"{\"a3_probe\":\"ok\"}","annotations":[]}]
            }]
        });
        let mut body_object = body.as_object().cloned().ok_or("probe response object")?;
        body_object.extend(
            response
                .as_object()
                .cloned()
                .ok_or("response summary object")?,
        );
        let encoded = serde_json::to_vec(&body_object)?;
        write_json_response(&mut stream, "200 OK", &encoded).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gpt-5.4".to_owned())?,
        sample_settings()?,
    );
    let control = TestControl::default();
    let profile = ProbeModelProfile::new(&provider)
        .execute(&probe_request, timeout()?, &control)
        .await
        .map_err(map_app_error)?;
    let wire = server.await??;
    let payload: Value = serde_json::from_slice(&wire.body)?;

    assert_eq!(wire.path, "/v1/responses");
    assert_eq!(payload["stream"], false);
    assert_eq!(payload["store"], false);
    assert_eq!(payload["max_output_tokens"], 256);
    assert_eq!(payload["reasoning"]["effort"], "none");
    assert_eq!(payload["text"]["format"]["name"], "a3_capability_probe");
    assert_eq!(
        profile.capabilities().structured_output(),
        ModelStructuredOutputCapability::Verified
    );
    assert_eq!(
        profile.capabilities().tool_call_mode(),
        ModelToolCallMode::Disabled
    );
    assert_eq!(profile.settings().context_limit().get(), 4096);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_probe_and_batch_validate_dimension_order_and_input() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut probe_stream, _) = listener.accept().await?;
        let probe_request = read_http_request(&mut probe_stream).await?;
        write_json_response(
            &mut probe_stream,
            "200 OK",
            br#"{"object":"list","model":"text-embedding-3-small","data":[{"object":"embedding","index":0,"embedding":[0.1,0.2,0.3,0.4]}],"usage":{"prompt_tokens":4,"total_tokens":4}}"#,
        )
        .await?;

        let (mut batch_stream, _) = listener.accept().await?;
        let batch_request = read_http_request(&mut batch_stream).await?;
        write_json_response(
            &mut batch_stream,
            "200 OK",
            br#"{"object":"list","model":"text-embedding-3-small","data":[
                {"object":"embedding","index":1,"embedding":[0.5,0.6,0.7,0.8]},
                {"object":"embedding","index":0,"embedding":[0.1,0.2,0.3,0.4]}
            ],"usage":{"prompt_tokens":8,"total_tokens":8}}"#,
        )
        .await?;
        Ok::<(StubHttpRequest, StubHttpRequest), TestError>((probe_request, batch_request))
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let probe_request = EmbeddingCapabilityProbeRequest::new(
        EmbeddingModelId::new("text-embedding-3-small".to_owned())?,
        EmbeddingBatchSize::new(8)?,
    );
    let profile = ProbeEmbeddingModelProfile::new(&provider)
        .execute(&probe_request, timeout()?, &control)
        .await
        .map_err(map_app_error)?;
    assert_eq!(profile.dimension().get(), 4);

    let cards = [
        NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            "First card body",
        )?,
        NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([3; 32]),
            SnapshotId::from_bytes([2; 32]),
            "Second card body",
        )?,
    ];
    let batch = provider
        .embed(
            &profile,
            &cards,
            EmbeddingRequestTimeout::from_millis(5_000)?,
            &control,
        )
        .await
        .map_err(map_embedding_error)?;
    assert_eq!(
        batch.into_vectors(),
        vec![vec![0.1, 0.2, 0.3, 0.4], vec![0.5, 0.6, 0.7, 0.8]]
    );

    let (probe_wire, batch_wire) = server.await??;
    assert_eq!(probe_wire.path, "/v1/embeddings");
    assert_eq!(batch_wire.path, "/v1/embeddings");
    let probe_payload: Value = serde_json::from_slice(&probe_wire.body)?;
    assert_eq!(probe_payload["encoding_format"], "float");
    assert_eq!(
        probe_payload["input"],
        json!(["A3 embedding capability probe"])
    );
    let batch_payload: Value = serde_json::from_slice(&batch_wire.body)?;
    assert_eq!(
        batch_payload["input"],
        json!(["First card body", "Second card body"])
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_output_and_sensitive_http_failures_stay_fail_closed() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        let body = sse_response_event("response.created", "in_progress", None)
            + &sse_event(
                "response.output_item.added",
                json!({
                    "output_index":0,
                    "item":{"id":"call_1","type":"function_call","status":"in_progress"}
                }),
            );
        write_http_chunk(&mut stream, body.as_bytes()).await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gpt-5.4", false)?;
    let control = TestControl::default();
    let mut events = provider.stream(&request, timeout()?, &control).await?;
    assert_eq!(
        events.next().await,
        Some(Err(ModelProviderFailure::Rejected))
    );
    server.await??;

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "429 Too Many Requests",
            br#"{"error":{"message":"sensitive provider detail"}}"#,
        )
        .await?;
        Ok::<(), TestError>(())
    });
    let provider = test_provider(endpoint)?;
    let result = DiscoverProviderModels::new(&provider)
        .execute(timeout()?, &control)
        .await;
    assert_eq!(result, Err(ModelProviderFailure::Unavailable));
    assert!(
        !ModelProviderFailure::Unavailable
            .to_string()
            .contains("sensitive provider detail")
    );
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_rejects_provider_model_drift_before_projecting_output() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        let body = sse_event(
            "response.created",
            json!({"response": response_summary_for_model("in_progress", None, "gpt-4o")}),
        );
        write_http_chunk(&mut stream, body.as_bytes()).await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gpt-5.4", false)?;
    let control = TestControl::default();
    let mut events = provider.stream(&request, timeout()?, &control).await?;
    assert_eq!(
        events.next().await,
        Some(Err(ModelProviderFailure::InvalidResponse))
    );
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_body_read_is_wakeably_cancelled() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        let prefix = sse_response_event("response.created", "in_progress", None)
            + &sse_event(
                "response.output_item.added",
                json!({
                    "output_index":0,
                    "item":{"id":"msg_1","type":"message","status":"in_progress","role":"assistant"}
                }),
            )
            + &sse_event(
                "response.content_part.added",
                json!({
                    "item_id":"msg_1","output_index":0,"content_index":0,
                    "part":{"type":"output_text"}
                }),
            )
            + &sse_event(
                "response.output_text.delta",
                json!({"item_id":"msg_1","output_index":0,"content_index":0,"delta":"partial"}),
            );
        write_http_chunk(&mut stream, prefix.as_bytes()).await?;
        let _ = ready_sender.send(());
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gpt-5.4", false)?;
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
async fn denied_production_endpoint_fails_before_network_access() -> Result<(), TestError> {
    let endpoint = OpenAiEndpoint::default_origin()?;
    let provider = OpenAiModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyOpenAiEndpointPolicy),
        test_api_key()?,
    )?;
    let control = TestControl::default();
    let result = DiscoverProviderModels::new(&provider)
        .execute(timeout()?, &control)
        .await;
    assert_eq!(result, Err(ModelProviderFailure::EndpointDenied));
    Ok(())
}

#[test]
fn production_policy_does_not_authorize_arbitrary_https_origins() -> Result<(), TestError> {
    use a3_provider::OpenAiEndpointPolicy;

    let arbitrary = OpenAiEndpoint::parse("https://openai.example.test")?;
    assert!(StandardOpenAiEndpointPolicy.authorize(&arbitrary).is_err());
    assert!(format!("{arbitrary:?}").contains("Remote"));
    assert!(!format!("{arbitrary:?}").contains("openai.example.test"));
    Ok(())
}

fn response_summary(status: &str, usage: Option<(u64, u64)>) -> Value {
    response_summary_for_model(status, usage, "gpt-5.4")
}

fn response_summary_for_model(status: &str, usage: Option<(u64, u64)>, model: &str) -> Value {
    json!({
        "object":"response",
        "status":status,
        "model":model,
        "store":false,
        "tools":[],
        "tool_choice":"none",
        "parallel_tool_calls":false,
        "usage":usage.map(|(input_tokens, output_tokens)| json!({
            "input_tokens":input_tokens,
            "output_tokens":output_tokens,
            "total_tokens":input_tokens + output_tokens
        })),
        "incomplete_details":null,
        "error":null
    })
}

fn sse_response_event(event_type: &str, status: &str, usage: Option<(u64, u64)>) -> String {
    sse_event(
        event_type,
        json!({"response": response_summary(status, usage)}),
    )
}

fn sse_event(event_type: &str, body: Value) -> String {
    let mut object = body.as_object().cloned().unwrap_or_default();
    object.insert("type".to_owned(), Value::String(event_type.to_owned()));
    format!("event: {event_type}\ndata: {}\n\n", Value::Object(object))
}

fn endpoint_for(listener: &TcpListener) -> Result<OpenAiEndpoint, TestError> {
    Ok(OpenAiEndpoint::parse(&format!(
        "http://127.0.0.1:{}",
        listener.local_addr()?.port()
    ))?)
}

fn test_api_key() -> Result<ProviderApiKey, TestError> {
    ProviderApiKey::from_bytes(b"test-openai-key".to_vec()).map_err(map_app_error)
}

fn test_provider(endpoint: OpenAiEndpoint) -> Result<OpenAiModelProvider, TestError> {
    OpenAiModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyOpenAiEndpointPolicy),
        test_api_key()?,
    )
    .map_err(Into::into)
}

fn timeout() -> Result<ModelRequestTimeout, TestError> {
    ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)
}

fn sample_settings() -> Result<ModelProfileSettings, TestError> {
    ModelProfileSettings::new(
        ModelContextLimit::new(4096)?,
        ModelOutputLimit::new(2048)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(700)?,
            ModelTopP::from_milli(900)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )
    .map_err(map_app_error)
}

fn sample_request(model_id: &str, structured: bool) -> Result<ModelProviderRequest, TestError> {
    let schema = if structured {
        Some(StructuredOutputSchema::new(json!({
            "type":"object",
            "properties":{
                "schema_version":{"type":"integer", "const":1},
                "action":{
                    "oneOf":[{
                        "type":"object",
                        "properties":{
                            "kind":{"type":"string", "const":"inspect"},
                            "observations":{
                                "type":"array",
                                "prefixItems":[
                                    {"type":"string", "enum":["observation-a"]},
                                    {"type":"string", "enum":["observation-b"]}
                                ],
                                "minItems":2,
                                "maxItems":2,
                                "uniqueItems":true
                            }
                        },
                        "required":["kind", "observations"],
                        "additionalProperties":false
                    }]
                }
            },
            "required":["schema_version", "action"],
            "additionalProperties":false
        }))?)
    } else {
        None
    };
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("openai".to_owned())?,
        ModelId::try_from_string(model_id.to_owned())?,
        sample_settings()?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    );
    ModelProviderRequest::new(
        profile,
        vec![
            ModelMessage::try_from_string(
                ModelMessageRole::System,
                "System instruction".to_owned(),
            )?,
            ModelMessage::try_from_string(ModelMessageRole::User, "Hello assistant".to_owned())?,
        ],
        schema,
    )
    .map_err(map_app_error)
}

fn contains_schema_key(value: &Value, target: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(target)
                || object
                    .values()
                    .any(|value| contains_schema_key(value, target))
        }
        Value::Array(items) => items.iter().any(|value| contains_schema_key(value, target)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

struct StubHttpRequest {
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl StubHttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
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
    let header_str = std::str::from_utf8(&received[..header_end])?;
    let path = header_str
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| std::io::Error::other("stub request line was invalid"))?
        .to_owned();
    let headers = header_str
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect::<Vec<_>>();
    let content_length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
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
    }
    Ok(StubHttpRequest {
        path,
        headers,
        body: received[header_end..header_end + content_length].to_vec(),
    })
}

async fn write_event_stream_head(stream: &mut TcpStream) -> std::io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        )
        .await?;
    stream.flush().await
}

async fn write_http_chunk(stream: &mut TcpStream, chunk: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(format!("{:X}\r\n", chunk.len()).as_bytes())
        .await?;
    stream.write_all(chunk).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await
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

async fn finish_http_chunks(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"0\r\n\r\n").await?;
    stream.flush().await
}

fn map_app_error<E: std::fmt::Debug>(error: E) -> TestError {
    format!("{error:?}").into()
}

fn map_embedding_error(error: a3_application::EmbeddingProviderFailure) -> TestError {
    format!("{error:?}").into()
}
