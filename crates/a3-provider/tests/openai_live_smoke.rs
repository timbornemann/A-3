//! Explicit opt-in live contract for an OpenAI credential already stored by A^3.

use a3_application::{
    DecodeExplorerAction, DiscoverProviderModels, EmbeddingCapabilityProbeRequest,
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingRequestTimeout, ModelCancellationFuture,
    ModelCapabilityProbeRequest, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelProvider, ModelProviderRequest, ModelRequestTimeout,
    ProbeEmbeddingModelProfile, ProbeModelProfile, ProviderCredentialStore, ProviderEvent,
    StructuredOutputSchema,
};
use a3_credentials::NativeProviderCredentialStore;
use a3_domain::{
    EmbeddingBatchSize, EmbeddingModelId, ExplorerAction, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelTemperature, ModelTokenCountingStrategy, ModelTopP, NormalizedSemanticCard,
    SemanticCardId, SnapshotId,
};
use a3_provider::{OpenAiEndpoint, OpenAiModelProvider, StandardOpenAiEndpointPolicy};
use futures::StreamExt;
use serde_json::json;
use std::error::Error;
use std::sync::Arc;

type LiveTestError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct LiveControl;

impl ModelOperationControl for LiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}

impl EmbeddingOperationControl for LiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a real OpenAI key stored by A^3 and explicit live-network approval"]
async fn stored_user_key_lists_streams_structures_and_embeds_against_openai()
-> Result<(), LiveTestError> {
    let provider_id = ModelProviderId::try_from_string("openai".to_owned())?;
    let credential = NativeProviderCredentialStore::new()
        .load(&provider_id)
        .await?
        .ok_or_else(|| std::io::Error::other("no A^3 OpenAI credential is configured"))?;
    let provider = OpenAiModelProvider::new(
        OpenAiEndpoint::default_origin()?,
        Arc::new(StandardOpenAiEndpointPolicy),
        credential.into_secret(),
    )?;
    let control = LiveControl;
    let timeout = ModelRequestTimeout::from_millis(180_000)?;

    let catalog = DiscoverProviderModels::new(&provider)
        .execute(timeout, &control)
        .await?;
    let generation_model = select_generation_model(catalog.model_ids())
        .ok_or_else(|| std::io::Error::other("OpenAI advertised no supported GPT smoke model"))?;
    let embedding_model = catalog
        .model_ids()
        .iter()
        .find(|model| model.as_str() == "text-embedding-3-small")
        .cloned()
        .ok_or_else(|| std::io::Error::other("OpenAI did not advertise text-embedding-3-small"))?;

    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &ModelCapabilityProbeRequest::new(generation_model, live_settings()?),
            timeout,
            &control,
        )
        .await?;
    let text = collect_stream(
        &provider,
        ModelProviderRequest::new(
            profile.clone(),
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "Reply with the single word OK.".to_owned(),
            )?],
            None,
        )?,
        timeout,
        &control,
    )
    .await?;
    if text.trim().is_empty() {
        return Err(std::io::Error::other("OpenAI returned empty streamed text").into());
    }

    let structured = collect_stream(
        &provider,
        structured_request(profile.clone())?,
        timeout,
        &control,
    )
    .await?;
    let value: serde_json::Value = serde_json::from_str(structured.trim())?;
    if value != json!({"result": "ok"}) {
        return Err(std::io::Error::other("OpenAI returned unexpected structured output").into());
    }

    let deep_map = collect_stream(
        &provider,
        deep_map_inspect_request(profile)?,
        timeout,
        &control,
    )
    .await?;
    if !matches!(
        DecodeExplorerAction::version_one().decode(deep_map.trim())?,
        ExplorerAction::Inspect(_)
    ) {
        return Err(std::io::Error::other("OpenAI returned a non-inspect Deep Map action").into());
    }

    let embedding_profile = ProbeEmbeddingModelProfile::new(&provider)
        .execute(
            &EmbeddingCapabilityProbeRequest::new(
                EmbeddingModelId::new(embedding_model.as_str().to_owned())?,
                EmbeddingBatchSize::new(1)?,
            ),
            timeout,
            &control,
        )
        .await?;
    let cards = [NormalizedSemanticCard::normalize_v1(
        SemanticCardId::from_bytes([1; 32]),
        SnapshotId::from_bytes([2; 32]),
        "A^3 OpenAI live embedding smoke",
    )?];
    let vectors = provider
        .embed(
            &embedding_profile,
            &cards,
            EmbeddingRequestTimeout::from_millis(180_000)?,
            &control,
        )
        .await?
        .into_vectors();
    if vectors.len() != 1 || vectors[0].len() != usize::from(embedding_profile.dimension().get()) {
        return Err(std::io::Error::other("OpenAI returned an invalid embedding batch").into());
    }
    Ok(())
}

fn select_generation_model(models: &[ModelId]) -> Option<ModelId> {
    [
        "gpt-5.6-luna",
        "gpt-5.4-mini",
        "gpt-4.1-mini",
        "gpt-4o-mini",
    ]
    .into_iter()
    .find_map(|target| models.iter().find(|model| model.as_str() == target))
    .cloned()
}

fn live_settings() -> Result<ModelProfileSettings, LiveTestError> {
    Ok(ModelProfileSettings::new(
        ModelContextLimit::new(4_096)?,
        ModelOutputLimit::new(4_096)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(0)?,
            ModelTopP::from_milli(1_000)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
    )?)
}

fn structured_request(profile: ModelProfile) -> Result<ModelProviderRequest, LiveTestError> {
    Ok(ModelProviderRequest::new(
        profile,
        vec![ModelMessage::try_from_string(
            ModelMessageRole::User,
            "Return exactly the requested JSON value with result set to ok.".to_owned(),
        )?],
        Some(StructuredOutputSchema::new(json!({
            "type": "object",
            "properties": {
                "result": {"type": "string", "const": "ok"}
            },
            "required": ["result"],
            "additionalProperties": false
        }))?),
    )?)
}

fn deep_map_inspect_request(profile: ModelProfile) -> Result<ModelProviderRequest, LiveTestError> {
    let decoder = DecodeExplorerAction::version_one();
    let mut schema = serde_json::from_str::<serde_json::Value>(decoder.json_schema().as_str())?;
    *schema
        .pointer_mut("/properties/action")
        .ok_or_else(|| std::io::Error::other("Deep Map schema has no action property"))? =
        json!({"$ref": "#/$defs/inspect"});
    Ok(ModelProviderRequest::new(
        profile,
        vec![
            ModelMessage::try_from_string(
                ModelMessageRole::System,
                "Return exactly one JSON object matching the supplied schema and no prose."
                    .to_owned(),
            )?,
            ModelMessage::try_from_string(
                ModelMessageRole::User,
                "Return schema_version 1 and an inspect action with expected_gain_basis_points 100 and a short non-empty gain_rationale."
                    .to_owned(),
            )?,
        ],
        Some(StructuredOutputSchema::new(schema)?),
    )?)
}

async fn collect_stream(
    provider: &OpenAiModelProvider,
    request: ModelProviderRequest,
    timeout: ModelRequestTimeout,
    control: &LiveControl,
) -> Result<String, LiveTestError> {
    let mut events = provider.stream(&request, timeout, control).await?;
    let mut text = String::new();
    let mut completed = false;
    while let Some(event) = events.next().await {
        match event? {
            ProviderEvent::OutputText(chunk) => text.push_str(chunk.as_str()),
            ProviderEvent::Completed(completion) => {
                if completion.reason() != ModelFinishReason::Stop {
                    return Err(std::io::Error::other("OpenAI stream hit its output limit").into());
                }
                completed = true;
            }
        }
    }
    if !completed {
        return Err(std::io::Error::other("OpenAI stream ended without completion").into());
    }
    Ok(text)
}
