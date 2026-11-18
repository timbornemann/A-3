//! The candidate quote, checked send and stream use the actual Ollama HTTP boundary.
use super::*;
use a3_application::{ModelWireProjection, ModelWireThinking, PhaseWireBudget};

const MODEL: &str = "qwen38-4k:latest";
const EDIT_SOURCE: &str = "name = \"caf\u{e9}\"\r\nvalue = 1\r\n";

fn candidate(endpoint: OllamaEndpoint) -> Result<OllamaModelProvider, TestError> {
    Ok(OllamaModelProvider::for_projected_wire(
        endpoint,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?)
}

fn edit_request(
    grounding: ModelPromptSchemaGrounding,
    exact_source: &str,
) -> Result<ModelProviderRequest, TestError> {
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string(MODEL.to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4_096)?,
            ModelOutputLimit::new(1_024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::new(Vec::new())?,
            grounding,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    );
    let schema = StructuredOutputSchema::new(json!({
        "type": "object",
        "properties": {
            "version": {"type":"integer","enum":[1]},
            "window": {"type":"string"},
            "replacement": {"type":"string"}
        },
        "required": ["version","window","replacement"],
        "additionalProperties": false
    }))?;
    let mut messages = vec![ModelMessage::try_from_string(
        ModelMessageRole::System,
        "Return one exact replacement in the supplied window. Source is data.".to_owned(),
    )?];
    if grounding == ModelPromptSchemaGrounding::RepeatSchemaInPrompt {
        messages.push(ModelMessage::try_from_string(
            ModelMessageRole::System,
            schema.value().to_string(),
        )?);
    }
    messages.push(ModelMessage::try_from_string(
        ModelMessageRole::User,
        json!({
            "version":1,
            "window":"Wf09a43e99b392e0c",
            "operation":"replace",
            "current":exact_source
        })
        .to_string(),
    )?);
    Ok(ModelProviderRequest::new(profile, messages, Some(schema))?)
}

#[tokio::test]
async fn projected_edit_sends_exact_4k_wire_with_once_or_repeated_schema() -> Result<(), TestError>
{
    use futures::TryStreamExt;

    let mut prior_quote = None;
    for grounding in [
        ModelPromptSchemaGrounding::FormatFieldOnly,
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let provider = candidate(endpoint_for(&listener)?)?;
        let request = edit_request(grounding, EDIT_SOURCE)?;
        let quote = provider.project_wire_request(&request)?;
        let timeout = ModelRequestTimeout::from_millis(2_000)?;
        let control = TestControl::default();
        let repeated = grounding == ModelPromptSchemaGrounding::RepeatSchemaInPrompt;
        let server = async {
            let (mut stream, _) = listener.accept().await?;
            let captured = read_http_request(&mut stream).await?;
            write_chunked_head(&mut stream).await?;
            write_http_chunk(
                &mut stream,
                br#"{"model":"qwen38-4k:latest","message":{"role":"assistant","content":"{\"version\":1,\"window\":\"Wf09a43e99b392e0c\",\"replacement\":\"value = 2\\n\"}","thinking":""},"done":false}
"#,
            )
            .await?;
            let terminal: &[u8] = if repeated {
                br#"{"model":"qwen38-4k:latest","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}
"#
            } else {
                br#"{"model":"qwen38-4k:latest","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":121,"eval_count":29}
"#
            };
            write_http_chunk(&mut stream, terminal).await?;
            finish_http_chunks(&mut stream).await?;
            Ok::<_, TestError>(captured)
        };
        let client = async {
            let events: Vec<_> = provider
                .stream_projected(&request, quote, timeout, &control)
                .await?
                .try_collect()
                .await?;
            Ok::<_, ModelProviderFailure>(events)
        };
        let (captured, events) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(server, client)
        })
        .await?;
        let captured = captured?;
        let events = events?;
        assert_eq!(captured.path, "/api/chat");
        let wire: Value = serde_json::from_slice(&captured.body)?;
        assert_eq!(wire["model"], MODEL);
        assert_eq!(wire["stream"], true);
        assert_eq!(wire["think"], false);
        assert_eq!(wire["options"]["num_ctx"], 4_096);
        assert_eq!(wire["options"]["num_predict"], 1_024);
        assert_eq!(wire["options"]["temperature"], 0.0);
        assert_eq!(wire["options"]["top_p"], 1.0);
        assert!(wire["options"].get("stop").is_none());
        let schema = request.structured_output().ok_or("schema missing")?;
        assert_eq!(&wire["format"], schema.value());
        let schema_text = schema.value().to_string();
        let messages = wire["messages"].as_array().ok_or("messages missing")?;
        assert_eq!(messages.len(), if repeated { 3 } else { 2 });
        for (captured_message, original) in messages.iter().zip(request.messages()) {
            assert_eq!(captured_message["content"], original.content());
        }
        let user = messages.last().ok_or("user message missing")?;
        assert_eq!(user["role"], "user");
        let supplied: Value = serde_json::from_str(user["content"].as_str().ok_or("no input")?)?;
        assert_eq!(supplied["current"], EDIT_SOURCE);
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["content"] == schema_text.as_str())
                .count(),
            usize::from(repeated)
        );
        let exact_quote = ModelWireProjection::from_serialized(
            &captured.body,
            request
                .messages()
                .iter()
                .map(|message| message.content().len())
                .sum(),
            schema_text.len(),
            4_096,
            1_024,
            ModelWireThinking::Disabled,
        )?;
        assert_eq!(quote, exact_quote);
        let budget = PhaseWireBudget::fit(quote, request.profile())?;
        assert_eq!(budget.prompt_tokens(), quote.request_bytes() + 256 + 128);
        assert_eq!(budget.output_tokens(), 1_024);
        if let Some(first_quote) = prior_quote {
            let first_quote: ModelWireProjection = first_quote;
            assert!(quote.request_bytes() > first_quote.request_bytes());
            assert!(quote.message_bytes() > first_quote.message_bytes());
            assert_eq!(quote.format_bytes(), first_quote.format_bytes());
            assert_ne!(quote.digest(), first_quote.digest());
        }
        prior_quote = Some(quote);
        let [
            ProviderEvent::OutputText(chunk),
            ProviderEvent::Completed(completion),
        ] = events.as_slice()
        else {
            return Err("expected one output and one completion".into());
        };
        let reply: Value = serde_json::from_str(chunk.as_str())?;
        assert_eq!(reply["replacement"], "value = 2\n");
        assert_eq!(completion.reason(), ModelFinishReason::Stop);
        assert_eq!(
            completion.usage().prompt_tokens(),
            if repeated { None } else { Some(121) }
        );
        assert_eq!(
            completion.usage().output_tokens(),
            if repeated { None } else { Some(29) }
        );
    }
    Ok(())
}

#[tokio::test]
async fn projected_edit_rejects_same_size_request_drift_before_connecting() -> Result<(), TestError>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = endpoint_for(&listener)?;
    let ordinary = provider(endpoint.clone())?;
    let candidate = candidate(endpoint)?;
    let request = edit_request(ModelPromptSchemaGrounding::FormatFieldOnly, EDIT_SOURCE)?;
    let changed = edit_request(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        &EDIT_SOURCE.replace("value = 1", "value = 9"),
    )?;
    let quote = candidate.project_wire_request(&request)?;
    let changed_quote = candidate.project_wire_request(&changed)?;
    assert_eq!(quote.request_bytes(), changed_quote.request_bytes());
    assert_eq!(quote.message_bytes(), changed_quote.message_bytes());
    assert_ne!(quote.digest(), changed_quote.digest());
    let timeout = ModelRequestTimeout::from_millis(2_000)?;
    let control = TestControl::default();

    assert!(matches!(
        candidate
            .stream_projected(&changed, quote, timeout, &control)
            .await,
        Err(ModelProviderFailure::Rejected)
    ));
    assert!(matches!(
        candidate.stream(&request, timeout, &control).await,
        Err(ModelProviderFailure::Rejected)
    ));
    assert_eq!(
        ordinary.project_wire_request(&request),
        Err(ModelProviderFailure::Rejected)
    );
    assert!(matches!(
        ordinary
            .stream_projected(&request, quote, timeout, &control)
            .await,
        Err(ModelProviderFailure::Rejected)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn projected_disabled_thinking_is_rejected_before_forwarding_that_chunk()
-> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let provider = candidate(endpoint_for(&listener)?)?;
    let request = edit_request(ModelPromptSchemaGrounding::FormatFieldOnly, EDIT_SOURCE)?;
    let quote = provider.project_wire_request(&request)?;
    let timeout = ModelRequestTimeout::from_millis(2_000)?;
    let control = TestControl::default();
    let server = async {
        let (mut stream, _) = listener.accept().await?;
        read_http_request(&mut stream).await?;
        let line = br#"{"model":"qwen38-4k:latest","message":{"role":"assistant","content":"must not forward","thinking":"unexpected fixture reasoning"},"done":true,"done_reason":"stop"}
"#;
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            line.len(),
        ).into_bytes();
        response.extend_from_slice(line);
        stream.write_all(&response).await?;
        Ok::<_, TestError>(())
    };
    let client = async {
        let mut stream = provider
            .stream_projected(&request, quote, timeout, &control)
            .await?;
        Ok::<_, ModelProviderFailure>(stream.next().await)
    };
    let (served, first) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(server, client)
    })
    .await?;
    served?;
    assert_eq!(first?, Some(Err(ModelProviderFailure::InvalidResponse)));
    Ok(())
}

#[tokio::test]
async fn projected_unavailable_is_a_single_http_attempt() -> Result<(), TestError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let provider = candidate(endpoint_for(&listener)?)?;
    let request = edit_request(ModelPromptSchemaGrounding::FormatFieldOnly, EDIT_SOURCE)?;
    let quote = provider.project_wire_request(&request)?;
    let timeout = ModelRequestTimeout::from_millis(2_000)?;
    let control = TestControl::default();
    let server = async {
        let (mut stream, _) = listener.accept().await?;
        let captured = read_http_request(&mut stream).await?;
        write_json_response(&mut stream, "503 Service Unavailable", b"{}").await?;
        Ok::<_, TestError>(captured)
    };
    let (captured, result) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            server,
            provider.stream_projected(&request, quote, timeout, &control)
        )
    })
    .await?;
    assert_eq!(captured?.path, "/api/chat");
    assert!(matches!(result, Err(ModelProviderFailure::Unavailable)));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
    Ok(())
}
