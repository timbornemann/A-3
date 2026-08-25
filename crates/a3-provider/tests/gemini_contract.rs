//! Offline HTTP contract for the Google Gemini streaming adapter.

use a3_application::{
    DecodeExplorerAction, DiscoverProviderModels, EmbeddingCapabilityProbeRequest,
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingRequestTimeout, ModelCancellationFuture,
    ModelCapabilityProbeRequest, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderRequest,
    ModelRequestTimeout, ProbeEmbeddingModelProfile, ProbeModelProfile, ProbeModelProfileFailure,
    ProviderApiKey, ProviderEvent, StructuredOutputSchema,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProviderId, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
    ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding,
    ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
    ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    NormalizedSemanticCard, SemanticCardId, SnapshotId,
};
use a3_model_provider_contract_tests::verify_model_provider_stream;
use a3_provider::{
    GeminiEndpoint, GeminiModelProvider, LocalOnlyGeminiEndpointPolicy,
    StandardGeminiEndpointPolicy,
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
async fn gemini_adapter_discovers_a_bounded_canonical_model_catalog() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut first_stream, _) = listener.accept().await?;
        let first_request = read_http_request(&mut first_stream).await?;
        write_json_response(
            &mut first_stream,
            "200 OK",
            br#"{"models":[
                {"name":"models/gemini-2.5-flash","supportedGenerationMethods":["generateContent","streamGenerateContent"]},
                {"name":"models/count-only","supportedGenerationMethods":["countTokens"]}
            ],"nextPageToken":"page 2"}"#,
        )
        .await?;

        let (mut second_stream, _) = listener.accept().await?;
        let second_request = read_http_request(&mut second_stream).await?;
        write_json_response(
            &mut second_stream,
            "200 OK",
            br#"{"models":[
                {"name":"models/gemini-embedding-001","supportedGenerationMethods":["embedContent"]},
                {"name":"models/gemini-2.5-flash","supportedGenerationMethods":["generateContent"]}
            ]}"#,
        )
        .await?;
        Ok::<(StubHttpRequest, StubHttpRequest), TestError>((first_request, second_request))
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let catalog = DiscoverProviderModels::new(&provider)
        .execute(
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_app_error)?;

    let (first_request, second_request) = server.await??;
    assert_eq!(first_request.path, "/v1beta/models?pageSize=100");
    assert_eq!(
        second_request.path,
        "/v1beta/models?pageSize=100&pageToken=page+2"
    );
    assert_eq!(
        first_request.header("x-goog-api-key"),
        Some("test-gemini-key")
    );
    assert_eq!(first_request.header("x-goog-api-client"), Some("a3/0.1.0"));
    assert_eq!(
        second_request.header("x-goog-api-key"),
        Some("test-gemini-key")
    );
    assert_eq!(
        catalog
            .model_ids()
            .iter()
            .map(|model| model.as_str())
            .collect::<Vec<_>>(),
        vec!["gemini-2.5-flash", "gemini-embedding-001"]
    );
    assert!(!catalog.truncated());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_discovery_rejects_a_repeated_pagination_token() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await?;
            let _ = read_http_request(&mut stream).await?;
            write_json_response(
                &mut stream,
                "200 OK",
                br#"{"models":[],"nextPageToken":"repeated"}"#,
            )
            .await?;
        }
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let result = DiscoverProviderModels::new(&provider)
        .execute(
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await;
    assert_eq!(result, Err(ModelProviderFailure::InvalidResponse));
    server.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gemini_adapter_streams_neutral_events_and_encodes_strict_request() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        write_http_chunk(
            &mut stream,
            b"data: {\"candidates\":[{\"index\":1,\"content\":{\"parts\":[{\"text\":\"ignore\"}]}},{\"index\":0,\"content\":{\"parts\":[{\"thought\":true,\"text\":\"secret\"},{\"text\":\"Hel",
        )
        .await?;
        write_http_chunk(
            &mut stream,
            b"lo \"}]}}]}\n\ndata: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"text\":\"world!\"}]}}]}\n\n",
        )
        .await?;
        write_http_chunk(
            &mut stream,
            b"data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":4}}\n\n",
        )
        .await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let request = sample_request("gemini-2.5-flash", false)?;
    let control = TestControl::default();
    let expected = vec![
        ProviderEvent::OutputText(
            a3_application::ModelOutputChunk::try_from_string("Hello ".to_owned())
                .map_err(map_app_error)?,
        ),
        ProviderEvent::OutputText(
            a3_application::ModelOutputChunk::try_from_string("world!".to_owned())
                .map_err(map_app_error)?,
        ),
        ProviderEvent::Completed(a3_application::ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            a3_application::ModelProviderUsage::new(Some(12), Some(4)),
        )),
    ];
    let events = verify_model_provider_stream(
        &provider,
        &request,
        ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
        &control,
        &expected,
    )
    .await?;

    let wire_request = server.await??;
    assert_eq!(
        wire_request.path,
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
    );
    assert_eq!(
        wire_request.header("x-goog-api-key"),
        Some("test-gemini-key")
    );
    let payload: Value = serde_json::from_slice(&wire_request.body)?;
    assert_eq!(
        payload["systemInstruction"]["parts"][0]["text"],
        "System instruction"
    );
    assert_eq!(payload["contents"][0]["role"], "user");
    assert_eq!(
        payload["contents"][0]["parts"][0]["text"],
        "Hello assistant"
    );
    assert_eq!(payload["generationConfig"]["temperature"], 0.7);
    assert_eq!(payload["generationConfig"]["topP"], 0.9);
    assert_eq!(payload["generationConfig"]["maxOutputTokens"], 2048);

    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0],
        ProviderEvent::OutputText(
            a3_application::ModelOutputChunk::try_from_string("Hello ".to_owned())
                .map_err(map_app_error)?
        )
    );
    assert_eq!(
        events[1],
        ProviderEvent::OutputText(
            a3_application::ModelOutputChunk::try_from_string("world!".to_owned())
                .map_err(map_app_error)?
        )
    );
    let completed = match &events[2] {
        ProviderEvent::Completed(completion) => completion,
        _ => return Err("expected completed event".into()),
    };
    assert_eq!(completed.reason(), ModelFinishReason::Stop);
    assert_eq!(completed.usage().prompt_tokens(), Some(12));
    assert_eq!(completed.usage().output_tokens(), Some(4));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deep_map_schema_is_translated_to_geminis_supported_wire_dialect() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        write_http_chunk(
            &mut stream,
            b"data: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"text\":\"{\\\"schema_version\\\":1,\\\"action\\\":{\\\"kind\\\":\\\"inspect\\\",\\\"expected_gain_basis_points\\\":100,\\\"gain_rationale\\\":\\\"Inspect the planned target.\\\"}}\"}]}}]}\n\ndata: {\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\"}]}\n\n",
        )
        .await?;
        finish_http_chunks(&mut stream).await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let request = structured_request_with_schema(
        "gemini-flash-latest",
        serde_json::from_str(DecodeExplorerAction::version_one().json_schema().as_str())?,
    )?;
    let control = TestControl::default();
    let mut events = provider
        .stream(
            &request,
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_app_error)?;
    while let Some(event) = events.next().await {
        event.map_err(map_app_error)?;
    }

    let wire_request = server.await??;
    let payload: Value = serde_json::from_slice(&wire_request.body)?;
    let schema = &payload["generationConfig"]["responseJsonSchema"];
    for unsupported in [
        "$schema",
        "$id",
        "$anchor",
        "oneOf",
        "pattern",
        "minLength",
        "maxLength",
        "uniqueItems",
    ] {
        assert!(!contains_key_recursive(schema, unsupported));
    }
    assert!(contains_key_recursive(schema, "anyOf"));
    assert!(contains_key_recursive(schema, "$defs"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_rejects_safety_finish_and_data_after_a_terminal_candidate() -> Result<(), TestError>
{
    for payload in [
        b"data: {\"candidates\":[{\"index\":0,\"finishReason\":\"SAFETY\"}]}\n\n".as_slice(),
        b"data: {\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\"}]}\n\ndata: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"text\":\"late\"}]}}]}\n\n".as_slice(),
        b"data: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"unsafe\",\"args\":{}}}]},\"finishReason\":\"STOP\"}]}\n\n".as_slice(),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = endpoint_for(&listener)?;
        let body = payload.to_vec();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let _ = read_http_request(&mut stream).await?;
            write_event_stream_head(&mut stream).await?;
            write_http_chunk(&mut stream, &body).await?;
            finish_http_chunks(&mut stream).await?;
            Ok::<(), TestError>(())
        });

        let provider = test_provider(endpoint)?;
        let request = sample_request("gemini-2.5-flash", false)?;
        let control = TestControl::default();
        let mut stream = provider
            .stream(
                &request,
                ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
                &control,
            )
            .await
            .map_err(map_app_error)?;
        assert!(matches!(
            stream.next().await,
            Some(Err(ModelProviderFailure::Rejected | ModelProviderFailure::InvalidResponse))
        ));
        server.await??;
    }
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
        write_json_response(
            &mut show_stream,
            "200 OK",
            br#"{"name":"models/gemini-2.5-flash","inputTokenLimit":1048576,"outputTokenLimit":8192,"supportedGenerationMethods":["generateContent","streamGenerateContent"]}"#,
        )
        .await?;

        let (mut chat_stream, _) = listener.accept().await?;
        let chat_request = read_http_request(&mut chat_stream).await?;
        write_json_response(
            &mut chat_stream,
            "200 OK",
            br#"{"candidates":[{"content":{"parts":[{"text":"{\"a3_probe\":\"ok\"}"}]}}]}"#,
        )
        .await?;

        Ok::<(StubHttpRequest, StubHttpRequest), TestError>((show_request, chat_request))
    });

    let provider = test_provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemini-2.5-flash".to_owned()).map_err(map_app_error)?,
        sample_settings()?,
    );
    let control = TestControl::default();
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_probe_error)?;

    let (show_request, chat_request) = server.await??;
    assert_eq!(show_request.path, "/v1beta/models/gemini-2.5-flash");
    assert_eq!(
        chat_request.path,
        "/v1beta/models/gemini-2.5-flash:generateContent"
    );
    let chat_body: Value = serde_json::from_slice(&chat_request.body)?;
    assert_eq!(
        chat_body["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert!(chat_body["generationConfig"]["responseJsonSchema"].is_object());
    assert_eq!(
        chat_body["generationConfig"]["maxOutputTokens"], 256,
        "thinking-capable Gemini models need enough bounded output budget to emit the probe JSON"
    );
    assert!(
        chat_body["generationConfig"]
            .get("responseSchema")
            .is_none()
    );

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
async fn invalid_structured_probe_output_creates_a_non_executable_profile() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut show_stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut show_stream).await?;
        write_json_response(
            &mut show_stream,
            "200 OK",
            br#"{"name":"models/gemini-2.5-flash","inputTokenLimit":1048576,"supportedGenerationMethods":["generateContent"]}"#,
        )
        .await?;

        let (mut chat_stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut chat_stream).await?;
        write_json_response(
            &mut chat_stream,
            "200 OK",
            br#"{"candidates":[{"content":{"parts":[{"text":"invalid json probe output"}]}}]}"#,
        )
        .await?;

        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let probe_request = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string("gemini-2.5-flash".to_owned()).map_err(map_app_error)?,
        sample_settings()?,
    );
    let control = TestControl::default();
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_probe_error)?;

    server.await??;
    assert_eq!(
        profile.capabilities().structured_output(),
        ModelStructuredOutputCapability::Unavailable
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_probe_observes_real_dimension_without_accepting_a_ui_dimension()
-> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "200 OK",
            br#"{"embedding":{"values":[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]}}"#,
        )
        .await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let probe_request = EmbeddingCapabilityProbeRequest::new(
        EmbeddingModelId::new("gemini-embedding-001".to_owned()).map_err(map_app_error)?,
        EmbeddingBatchSize::new(16).map_err(map_app_error)?,
    );
    let control = TestControl::default();
    let profile = ProbeEmbeddingModelProfile::new(&provider)
        .execute(
            &probe_request,
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_app_error)?;

    let request = server.await??;
    assert_eq!(
        request.path,
        "/v1beta/models/gemini-embedding-001:embedContent"
    );
    assert_eq!(profile.dimension().get(), 8);
    assert_eq!(profile.max_batch_size().get(), 16);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_adapter_encodes_bounded_canonical_card_bodies() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_request(&mut stream).await?;
        write_json_response(
            &mut stream,
            "200 OK",
            br#"{"embeddings":[
                {"values":[0.1, 0.2, 0.3, 0.4]},
                {"values":[0.5, 0.6, 0.7, 0.8]}
            ]}"#,
        )
        .await?;
        Ok::<StubHttpRequest, TestError>(request)
    });

    let provider = test_provider(endpoint)?;
    let profile = EmbeddingModelProfile::v1(
        EmbeddingProviderId::new("gemini".to_owned()).map_err(map_app_error)?,
        EmbeddingModelId::new("gemini-embedding-001".to_owned()).map_err(map_app_error)?,
        EmbeddingDimension::new(4).map_err(map_app_error)?,
        EmbeddingBatchSize::new(16).map_err(map_app_error)?,
    );
    let cards = [
        NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            "First card body",
        )
        .map_err(map_app_error)?,
        NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([3; 32]),
            SnapshotId::from_bytes([2; 32]),
            "Second card body",
        )
        .map_err(map_app_error)?,
    ];

    let control = TestControl::default();
    let batch = provider
        .embed(
            &profile,
            &cards,
            EmbeddingRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await
        .map_err(map_embedding_error)?;

    let request = server.await??;
    assert_eq!(
        request.path,
        "/v1beta/models/gemini-embedding-001:batchEmbedContents"
    );
    assert_eq!(
        batch.into_vectors(),
        vec![vec![0.1, 0.2, 0.3, 0.4], vec![0.5, 0.6, 0.7, 0.8]]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denied_remote_endpoint_fails_before_any_network_attempt() -> Result<(), TestError> {
    let endpoint = GeminiEndpoint::default_origin()?;
    let provider = GeminiModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyGeminiEndpointPolicy),
        test_api_key()?,
    )?;
    let control = TestControl::default();
    let result = DiscoverProviderModels::new(&provider)
        .execute(
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await;
    assert_eq!(result, Err(ModelProviderFailure::EndpointDenied));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_policy_rejects_an_arbitrary_https_origin_before_network()
-> Result<(), TestError> {
    let endpoint = GeminiEndpoint::parse("https://example.invalid")?;
    let provider = GeminiModelProvider::new(
        endpoint,
        Arc::new(StandardGeminiEndpointPolicy),
        test_api_key()?,
    )?;
    let control = TestControl::default();
    let result = DiscoverProviderModels::new(&provider)
        .execute(
            ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
            &control,
        )
        .await;
    assert_eq!(result, Err(ModelProviderFailure::EndpointDenied));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_cancellation_drops_the_in_flight_response() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let (first_chunk_sender, first_chunk_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let _ = read_http_request(&mut stream).await?;
        write_event_stream_head(&mut stream).await?;
        write_http_chunk(
            &mut stream,
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"chunk 1\"}]}}]}\n\n",
        )
        .await?;
        let _ = first_chunk_sender.send(());
        let mut buffer = [0_u8; 128];
        let _ = stream.read(&mut buffer).await;
        Ok::<(), TestError>(())
    });

    let provider = test_provider(endpoint)?;
    let control = TestControl::default();
    let request = sample_request("gemini-2.5-flash", false)?;
    let stream = provider.stream(
        &request,
        ModelRequestTimeout::from_millis(5_000).map_err(map_app_error)?,
        &control,
    );
    let mut stream = stream.await.map_err(map_app_error)?;
    first_chunk_receiver.await?;
    let first = stream.next().await;
    assert!(first.is_some());
    control.cancel();
    let second = stream.next().await;
    assert_eq!(second, Some(Err(ModelProviderFailure::Cancelled)));
    server.await??;
    Ok(())
}

fn endpoint_for(listener: &TcpListener) -> Result<GeminiEndpoint, TestError> {
    Ok(GeminiEndpoint::parse(&format!(
        "http://127.0.0.1:{}",
        listener.local_addr()?.port()
    ))?)
}

fn test_api_key() -> Result<ProviderApiKey, TestError> {
    ProviderApiKey::from_bytes(b"test-gemini-key".to_vec()).map_err(map_app_error)
}

fn test_provider(endpoint: GeminiEndpoint) -> Result<GeminiModelProvider, TestError> {
    GeminiModelProvider::new(
        endpoint,
        Arc::new(LocalOnlyGeminiEndpointPolicy),
        test_api_key()?,
    )
    .map_err(Into::into)
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
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
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

    let mut headers = Vec::new();
    for line in header_str.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_owned(), v.trim().to_owned()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
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
    let header = "HTTP/1.1 200 OK\r\n\
                  Content-Type: text/event-stream\r\n\
                  Transfer-Encoding: chunked\r\n\
                  Connection: close\r\n\r\n";
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await
}

async fn write_http_chunk(stream: &mut TcpStream, chunk: &[u8]) -> std::io::Result<()> {
    let prefix = format!("{:X}\r\n", chunk.len());
    stream.write_all(prefix.as_bytes()).await?;
    stream.write_all(chunk).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await
}

async fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn finish_http_chunks(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"0\r\n\r\n").await?;
    stream.flush().await
}

fn sample_settings() -> Result<ModelProfileSettings, TestError> {
    ModelProfileSettings::new(
        ModelContextLimit::new(4096).map_err(map_app_error)?,
        ModelOutputLimit::new(2048).map_err(map_app_error)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1).map_err(map_app_error)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(700).map_err(map_app_error)?,
            ModelTopP::from_milli(900).map_err(map_app_error)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )
    .map_err(map_app_error)
}

fn sample_request(model_id: &str, structured: bool) -> Result<ModelProviderRequest, TestError> {
    let schema = if structured {
        Some(
            StructuredOutputSchema::new(json!({
                "type": "object",
                "properties": { "result": { "type": "string" } },
                "required": ["result"]
            }))
            .map_err(map_app_error)?,
        )
    } else {
        None
    };
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("gemini".to_owned()).map_err(map_app_error)?,
        ModelId::try_from_string(model_id.to_owned()).map_err(map_app_error)?,
        sample_settings()?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    );
    let messages = vec![
        ModelMessage::try_from_string(ModelMessageRole::System, "System instruction".to_owned())
            .map_err(map_app_error)?,
        ModelMessage::try_from_string(ModelMessageRole::User, "Hello assistant".to_owned())
            .map_err(map_app_error)?,
    ];
    ModelProviderRequest::new(profile, messages, schema).map_err(map_app_error)
}

fn structured_request_with_schema(
    model_id: &str,
    schema: Value,
) -> Result<ModelProviderRequest, TestError> {
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("gemini".to_owned()).map_err(map_app_error)?,
        ModelId::try_from_string(model_id.to_owned()).map_err(map_app_error)?,
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
                ModelMessageRole::User,
                "Return an inspect action.".to_owned(),
            )
            .map_err(map_app_error)?,
        ],
        Some(StructuredOutputSchema::new(schema).map_err(map_app_error)?),
    )
    .map_err(map_app_error)
}

fn contains_key_recursive(value: &Value, expected: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(expected)
                || object
                    .values()
                    .any(|child| contains_key_recursive(child, expected))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| contains_key_recursive(child, expected)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn map_app_error<E: std::fmt::Debug>(err: E) -> TestError {
    format!("{err:?}").into()
}

fn map_probe_error(err: ProbeModelProfileFailure) -> TestError {
    format!("{err:?}").into()
}

fn map_embedding_error(err: a3_application::EmbeddingProviderFailure) -> TestError {
    format!("{err:?}").into()
}
