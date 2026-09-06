//! Exercise the production message builder and streaming collector without network or credentials.
use super::*;
use a3_application::{
    ModelCancellationFuture, ModelOutputChunk, ModelProviderCompletion, ModelProviderFuture,
    ModelProviderUsage,
};
use a3_domain::*;

#[derive(Debug)]
struct Control;

#[derive(Debug)]
struct BrokenStream {
    id: ModelProviderId,
    failure: ConversationStreamFailure,
}
impl ModelProvider for BrokenStream {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }
    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let chunk = || {
                ModelOutputChunk::try_from_string("{}".to_owned())
                    .map(ProviderEvent::OutputText)
                    .map_err(|_| ModelProviderFailure::InvalidResponse)
            };
            let done = || {
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(None, None),
                )))
            };
            let events = match self.failure {
                ConversationStreamFailure::MissingCompletion => vec![chunk()],
                ConversationStreamFailure::EmptyDocument => vec![done()],
                ConversationStreamFailure::AfterCompletion => vec![chunk(), done(), chunk()],
                ConversationStreamFailure::ProviderProtocol => {
                    vec![Err(ModelProviderFailure::InvalidResponse)]
                }
            };
            Ok(Box::pin(futures::stream::iter(events)) as a3_application::ProviderEventStream<'a>)
        })
    }
}

#[test]
fn research_stream_subcauses_are_closed_content_free_and_never_partial_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let profile = profile(8192, ModelPromptSchemaGrounding::FormatFieldOnly)?;
    for failure in [
        ConversationStreamFailure::MissingCompletion,
        ConversationStreamFailure::EmptyDocument,
        ConversationStreamFailure::AfterCompletion,
        ConversationStreamFailure::ProviderProtocol,
    ] {
        let provider = BrokenStream {
            id: profile.provider_id().clone(),
            failure,
        };
        assert_eq!(
            runtime.block_on(complete_with_provider(
                &provider,
                profile.clone(),
                "system",
                &[(
                    ModelMessageRole::User,
                    "CURRENT QUESTION:\nFixture".to_owned()
                )],
                None,
                &Control
            )),
            Err(AgentConversationFailure::Stream(failure))
        );
        assert!(failure.code().starts_with("research-v2/stream-"));
    }
    Ok(())
}
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
    profile_with_output(context, 1024, grounding)
}

fn profile_with_output(
    context: u32,
    output: u32,
    grounding: ModelPromptSchemaGrounding,
) -> Result<ModelProfile, Box<dyn std::error::Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("offline-fixture".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(context)?,
            ModelOutputLimit::new(output)?,
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
fn research_full_current_packet_and_maximum_repair_fit_real_provider_limits()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for (context, output) in [(8192, 2048), (16384, 4096)] {
        for grounding in [
            ModelPromptSchemaGrounding::FormatFieldOnly,
            ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
        ] {
            let profile = profile_with_output(context, output, grounding)?;
            for mode in [
                AgentSessionMode::Ask,
                AgentSessionMode::Plan,
                AgentSessionMode::Agent,
            ] {
                let budget = research_evidence_budget_for_profile(&profile, mode, None)?;
                // Repeated large schemas can exhaust a genuinely small profile before source packing.
                if budget < 256 {
                    continue;
                }
                let head = "CURRENT QUESTION:\nPreserve the entire design.\n";
                let tail = "\nLate policy: retain earlier writes; Größe 🦀.";
                let packet = format!(
                    "{head}{}{tail}",
                    "x".repeat(budget - head.len() - tail.len())
                );
                assert_eq!(packet.len(), budget);
                let hint = "R".repeat(768);
                for phase in [
                    a3_application::ResearchOutputPhase::Initialize,
                    a3_application::ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST),
                    a3_application::ResearchOutputPhase::SummarizeOriginals(
                        ResearchQuestionId::FIRST,
                    ),
                    a3_application::ResearchOutputPhase::Design(ResearchQuestionId::FIRST),
                    a3_application::ResearchOutputPhase::Finalize,
                ] {
                    let provider = CapturingProvider {
                        id: profile.provider_id().clone(),
                        requests: std::sync::Mutex::new(Vec::new()),
                        finish: ModelFinishReason::Stop,
                    };
                    let system = research_phase_system_prompt(mode, true, phase, None);
                    let schema = research_contract_schema(true, phase)?;
                    let mut transcript = vec![
                        (
                            ModelMessageRole::Assistant,
                            "optional historical dialogue ".repeat(2000),
                        ),
                        (ModelMessageRole::User, packet.clone()),
                    ];
                    for repair in [false, true] {
                        if repair {
                            transcript.push((ModelMessageRole::User, hint.clone()));
                        }
                        runtime.block_on(complete_with_provider(
                            &provider,
                            profile.clone(),
                            &system,
                            &transcript,
                            Some(schema.clone()),
                            &Control,
                        ))?;
                    }
                    let requests = provider.requests.lock().map_err(|_| "capture lock")?;
                    assert_eq!(requests.len(), 2);
                    for request in requests.iter() {
                        assert!(request.messages().iter().any(|m| m.content() == packet));
                        let bytes: usize =
                            request.messages().iter().map(|m| m.content().len()).sum();
                        assert!(bytes + output as usize + 1024 <= context as usize);
                    }
                    assert!(requests[1].messages().iter().any(|m| m.content() == hint));
                }
            }
        }
    }
    Ok(())
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
            for phase in 0..4 {
                let schema = match phase {
                    0 => research_phase_schema(true)?,
                    1 => research_contract_schema(
                        true,
                        a3_application::ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST),
                    )?,
                    2 => research_contract_schema(
                        false,
                        a3_application::ResearchOutputPhase::Finalize,
                    )?,
                    _ => evidence_diagram_schema()?,
                };
                let system = if phase == 3 {
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
                    if phase == 2 {
                        assert_eq!(
                            actual.pointer("/$defs/planDecision/properties/kind/const"),
                            Some(&serde_json::json!("plan"))
                        );
                    }
                    if phase == 0 {
                        assert_eq!(
                            actual.pointer("/$defs/work/properties/questions/minItems"),
                            Some(&serde_json::json!(1))
                        );
                    }
                    if phase == 1 {
                        assert_eq!(
                            actual.pointer("/$defs/result/properties/question_id/const"),
                            Some(&serde_json::json!(1))
                        );
                    }
                    if phase == 3 {
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
