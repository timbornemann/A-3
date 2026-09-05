//! Exercise the production message builder and streaming collector without network or credentials.
use super::*;
use a3_application::{
    ModelCancellationFuture, ModelOutputChunk, ModelProviderCompletion, ModelProviderFuture,
    ModelProviderUsage,
};
use a3_domain::*;

#[derive(Debug)]
struct Control;
impl ModelOperationControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}

#[derive(Debug)]
struct CapturingProvider {
    id: ModelProviderId,
    requests: std::sync::Mutex<Vec<ModelProviderRequest>>,
    finish: ModelFinishReason,
}
impl ModelProvider for CapturingProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }
    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?
                .push(request.clone());
            let events = vec![
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string("{}".to_owned())
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    self.finish,
                    ModelProviderUsage::new(None, None),
                ))),
            ];
            Ok(Box::pin(futures::stream::iter(events)) as a3_application::ProviderEventStream<'a>)
        })
    }
}

fn profile(
    context: u32,
    grounding: ModelPromptSchemaGrounding,
) -> Result<ModelProfile, Box<dyn std::error::Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("offline-fixture".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(context)?,
            ModelOutputLimit::new(1024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::new(Vec::new())?,
            grounding,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

#[test]
fn actual_provider_packets_preserve_goal_and_evidence_during_repair_and_use_phase_schemas()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for budget in [1024, 2048, 4096, 8192] {
        for grounding in [
            ModelPromptSchemaGrounding::FormatFieldOnly,
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        ] {
            for phase in 0..3 {
                let schema = match phase {
                    0 => research_phase_schema(true)?,
                    1 => research_phase_schema(false)?,
                    _ => evidence_diagram_schema()?,
                };
                let system = if phase == 2 {
                    DIAGRAM_SYSTEM_PROMPT.to_owned()
                } else {
                    research_system_prompt(AgentSessionMode::Ask, phase == 0, None)
                };
                let grounded = schema_grounded_system(&system, Some(&schema), grounding)?;
                // A near-full profile leaves just enough room for the protected packet and one hint.
                let profile = profile(
                    u32::try_from(grounded.len() + budget + 2048 + 768)?,
                    grounding,
                )?;
                let provider = CapturingProvider {
                    id: profile.provider_id().clone(),
                    requests: std::sync::Mutex::new(Vec::new()),
                    finish: ModelFinishReason::Stop,
                };
                let packet = format!(
                    "CURRENT QUESTION:\nExplain task creation\nCURRENT EVIDENCE:\n[S1] manager.py\n{}\nself.plugins.dispatch(task) END_OF_EVIDENCE",
                    "padding ".repeat((budget - 180) / 8)
                );
                assert!(packet.len() <= budget);
                let original = vec![
                    (ModelMessageRole::User, "historical question ".repeat(4000)),
                    (
                        ModelMessageRole::Assistant,
                        "old unsupported answer ".repeat(4000),
                    ),
                    (ModelMessageRole::User, packet.clone()),
                ];
                let mut repair = original.clone();
                repair.push((
                    ModelMessageRole::User,
                    "REPAIR: Return a shorter complete JSON object.".to_owned(),
                ));
                for transcript in [&original, &repair] {
                    runtime.block_on(complete_with_provider(
                        &provider,
                        profile.clone(),
                        &system,
                        transcript,
                        Some(schema.clone()),
                        &Control,
                    ))?;
                }
                let requests = provider.requests.lock().map_err(|_| "capture lock")?;
                assert_eq!(requests.len(), 2);
                for request in requests.iter() {
                    assert!(
                        request
                            .messages()
                            .iter()
                            .any(|message| message.content() == packet)
                    );
                    let actual = request.structured_output().ok_or("phase schema")?.value();
                    if phase == 1 {
                        assert_eq!(
                            actual.pointer("/properties/decision/properties/kind/const"),
                            Some(&serde_json::json!("answer"))
                        );
                    }
                    if phase == 0 {
                        assert!(actual.pointer("/properties/decision/oneOf/1").is_some());
                    }
                    if phase == 2 {
                        assert!(actual.pointer("/properties/diagrams").is_some());
                    }
                    let cost: usize = request
                        .messages()
                        .iter()
                        .map(|message| message.content().len())
                        .sum();
                    assert!(cost + 2048 <= profile.settings().context_limit().get() as usize);
                }
                assert!(
                    requests[1]
                        .messages()
                        .iter()
                        .any(|message| message.content().starts_with("REPAIR:"))
                );
            }
        }
    }
    Ok(())
}

#[test]
fn provider_output_limit_is_not_conflated_with_malformed_json_and_protected_overflow_fails_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let profile = profile(8192, ModelPromptSchemaGrounding::FormatFieldOnly)?;
    let provider = CapturingProvider {
        id: profile.provider_id().clone(),
        requests: std::sync::Mutex::new(Vec::new()),
        finish: ModelFinishReason::OutputLimit,
    };
    assert_eq!(
        runtime.block_on(complete_with_provider(
            &provider,
            profile.clone(),
            "system",
            &[(ModelMessageRole::User, "CURRENT QUESTION:\nTest".to_owned())],
            None,
            &Control
        )),
        Err(AgentConversationFailure::OutputTruncated)
    );
    assert!(matches!(
        budgeted_messages(
            &profile,
            "system",
            &[(
                ModelMessageRole::User,
                format!("CURRENT QUESTION:\n{}", "x".repeat(8192))
            )]
        ),
        Err(AgentConversationFailure::InvalidInput)
    ));
    Ok(())
}
