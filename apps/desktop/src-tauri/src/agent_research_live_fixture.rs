//! Explicitly opted-in model smoke. No desktop configuration or original repository is changed.
#[path = "agent_action_wire_probe.rs"]
mod action_wire_probe;
#[path = "agent_live_coding_fixture.rs"]
mod coding;

use super::*;
use a3_application::{
    ConfiguredModelEndpoint, DesktopSettings, DiscoverProviderModels,
    LoadDesktopProviderCredential, ModelCapabilityProbe, ModelCapabilityProbeRequest,
    ModelCatalogProvider, ModelProvider, ModelProviderKind, ModelRequestTimeout, ProbeModelProfile,
    ProviderCredentialStore,
};
use a3_domain::*;
use a3_provider::{
    ExactGeminiEndpointPolicy, ExactOpenAiEndpointPolicy, GeminiEndpoint, GeminiModelProvider,
    LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider, OpenAiEndpoint,
    OpenAiModelProvider,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplicitResearchTarget {
    Luna,
    GoogleGemma,
    GoogleFlash,
}

impl ExplicitResearchTarget {
    fn parse(provider: Option<&str>, model: Option<&str>) -> Result<Option<Self>, &'static str> {
        match (provider, model) {
            (None, None) => Ok(None),
            (Some("openai"), Some("gpt-5.6-luna")) => Ok(Some(Self::Luna)),
            (Some("gemini"), Some("gemma-4-26b-a4b-it")) => Ok(Some(Self::GoogleGemma)),
            (Some("gemini"), Some("gemini-3.8-flash")) => Ok(Some(Self::GoogleFlash)),
            _ => Err("explicit research provider/model pair is not reviewed"),
        }
    }

    fn identity(self) -> (ModelProviderKind, &'static str) {
        match self {
            Self::Luna => (ModelProviderKind::OpenAi, "gpt-5.6-luna"),
            Self::GoogleGemma => (ModelProviderKind::Gemini, "gemma-4-26b-a4b-it"),
            Self::GoogleFlash => (ModelProviderKind::Gemini, "gemini-3.8-flash"),
        }
    }

    fn endpoint(
        self,
        settings: &DesktopSettings,
    ) -> Result<&ConfiguredModelEndpoint, &'static str> {
        let slot = settings.provider(self.identity().0);
        if !slot.enabled() || slot.connection_verified_at().is_none() {
            return Err("explicit research provider is disabled or unverified");
        }
        slot.endpoint()
            .filter(|endpoint| endpoint.provider_id().as_str() == self.identity().0.provider_id())
            .ok_or("explicit research provider endpoint is unavailable")
    }
}

fn optional_env(name: &str) -> Result<Option<String>, std::env::VarError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error),
    }
}

fn fixture_profile_settings(model: &str) -> Result<ModelProfileSettings, Box<dyn Error>> {
    let (context, output) = match model {
        "qwen38-8k:latest" => (8192, 2048),
        "gpt-5.6-luna" => (16_384, 2048),
        _ => (16_384, 4096),
    };
    Ok(ModelProfileSettings::new(
        ModelContextLimit::new(context)?,
        ModelOutputLimit::new(output)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(0)?,
            ModelTopP::from_milli(1000)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::FormatFieldOnly,
    )?)
}

#[derive(Clone)]
pub(super) struct LiveResearchModel {
    provider: Arc<dyn a3_application::ModelProvider>,
    profile: ModelProfile,
}

impl LiveResearchModel {
    pub(super) async fn probe() -> Result<Self, Box<dyn Error>> {
        let target = ExplicitResearchTarget::parse(
            optional_env("A3_RESEARCH_EVAL_PROVIDER")?.as_deref(),
            optional_env("A3_RESEARCH_EVAL_MODEL")?.as_deref(),
        )?;
        let catalog = optional_env("A3_CONFIGURED_RESEARCH_CATALOG")?;
        let local = optional_env("A3_LOCAL_RESEARCH_MODEL")?;
        if (catalog.is_some() && local.is_some()) || (target.is_some() && catalog.is_none()) {
            return Err(
                "research selection requires one unambiguous catalog or local target".into(),
            );
        }
        if let Some(path) = catalog {
            let stored = a3_storage_libsql::LibsqlKnowledgeStore::read_settings_snapshot(
                std::path::Path::new(&path),
            )
            .await?;
            let settings = stored.settings();
            if let Some(target) = target {
                return Self::probe_explicit(target, settings).await;
            }
            let (endpoint, profile) =
                crate::agent_conversation_runtime::executable_coding(settings)
                    .ok_or("configured coding profile is not executable")?;
            println!(
                "configured-research-profile: provider={} model={} context={} output={}",
                profile.provider_id().as_str(),
                profile.model_id().as_str(),
                profile.settings().context_limit().get(),
                profile.settings().output_limit().get()
            );
            let credentials: Arc<dyn a3_application::ProviderCredentialStore> =
                Arc::new(a3_credentials::NativeProviderCredentialStore::new());
            let provider = crate::agent_conversation_runtime::resolve_provider(
                endpoint,
                settings,
                &credentials,
            )
            .await?;
            return Ok(Self { provider, profile });
        }
        let model = local.ok_or("no research model explicitly selected")?;
        if !matches!(
            model.as_str(),
            "qwen3.5:4b"
                | "qwen38-16k:latest"
                | "qwen38-8k:latest"
                | "ornith-1.5:9b"
                | "gemma4:12b"
                | "gpt-oss:20b"
                | "granite4.2:8b"
        ) {
            return Err("fixture requires an explicitly reviewed installed local model".into());
        }
        // Endpoint is loopback-only. The opt-in operator must also confirm local model residency.
        let provider = Arc::new(OllamaModelProvider::new(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?);
        let settings = fixture_profile_settings(&model)?;
        Self::probe_provider(provider, &model, settings).await
    }

    async fn probe_explicit(
        target: ExplicitResearchTarget,
        settings: &DesktopSettings,
    ) -> Result<Self, Box<dyn Error>> {
        let endpoint = target.endpoint(settings)?;
        let (kind, model) = target.identity();
        let profile_settings = crate::agent_conversation_runtime::executable_coding(settings)
            .filter(|(_, profile)| {
                profile.provider_id().as_str() == kind.provider_id()
                    && profile.model_id().as_str() == model
            })
            .map(|(_, profile)| profile.settings().clone())
            .map_or_else(|| fixture_profile_settings(model), Ok)?;
        let credentials: Arc<dyn ProviderCredentialStore> =
            Arc::new(a3_credentials::NativeProviderCredentialStore::new());
        let key = LoadDesktopProviderCredential::new(credentials)
            .execute_for(settings, kind)
            .await?
            .ok_or("explicit research provider credential is unavailable")?;
        let origin = endpoint.canonical_origin();
        match target {
            ExplicitResearchTarget::GoogleGemma | ExplicitResearchTarget::GoogleFlash => {
                Self::probe_provider(
                    Arc::new(GeminiModelProvider::new(
                        GeminiEndpoint::parse(origin)?,
                        Arc::new(ExactGeminiEndpointPolicy::new(origin)),
                        key,
                    )?),
                    model,
                    profile_settings,
                )
                .await
            }
            ExplicitResearchTarget::Luna => {
                Self::probe_provider(
                    Arc::new(OpenAiModelProvider::new(
                        OpenAiEndpoint::parse(origin)?,
                        Arc::new(ExactOpenAiEndpointPolicy::new(origin)),
                        key,
                    )?),
                    model,
                    profile_settings,
                )
                .await
            }
        }
    }

    async fn probe_provider<
        P: ModelProvider + ModelCatalogProvider + ModelCapabilityProbe + 'static,
    >(
        provider: Arc<P>,
        model: &str,
        settings: ModelProfileSettings,
    ) -> Result<Self, Box<dyn Error>> {
        let control = ProbeControl;
        let catalog = DiscoverProviderModels::new(provider.as_ref())
            .execute(ModelRequestTimeout::DEFAULT, &control)
            .await?;
        if !catalog.model_ids().iter().any(|id| id.as_str() == model) {
            return Err("selected research model is not in the observed provider catalog".into());
        }
        let profile = ProbeModelProfile::new(provider.as_ref())
            .execute(
                &ModelCapabilityProbeRequest::new(
                    ModelId::try_from_string(model.to_owned())?,
                    settings,
                ),
                ModelRequestTimeout::DEFAULT,
                &control,
            )
            .await?;
        if profile.capabilities().structured_output() != ModelStructuredOutputCapability::Verified {
            return Err("research structured-output capability was not verified".into());
        }
        println!(
            "verified-research-profile: provider={} model={} context={} output={}",
            profile.provider_id().as_str(),
            model,
            profile.settings().context_limit().get(),
            profile.settings().output_limit().get()
        );
        Ok(Self { provider, profile })
    }

    pub(super) fn evidence_budget(
        &self,
        mode: AgentSessionMode,
    ) -> Result<usize, AgentConversationFailure> {
        crate::agent_conversation_runtime::research_evidence_budget_for_profile(
            &self.profile,
            mode,
            None,
        )
    }

    pub(super) async fn complete(
        &self,
        mode: AgentSessionMode,
        search: bool,
        phase: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        use crate::agent_conversation_runtime::{
            complete_with_provider, research_contract_schema, research_phase_system_prompt,
        };
        let result = complete_with_provider(
            self.provider.as_ref(),
            self.profile.clone(),
            &research_phase_system_prompt(mode, search, phase, None),
            transcript,
            Some(research_contract_schema(search, phase)?),
            control,
        )
        .await;
        if let Err(failure) = &result {
            println!("local-provider-category: {failure:?}");
        }
        result
    }
}

#[derive(Debug)]
struct ProbeControl;
impl a3_application::ModelOperationControl for ProbeControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn cancelled(&self) -> a3_application::ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}

#[test]
fn research_live_target_requires_an_exact_reviewed_pair() -> Result<(), Box<dyn Error>> {
    assert_eq!(ExplicitResearchTarget::parse(None, None)?, None);
    for target in [
        ExplicitResearchTarget::Luna,
        ExplicitResearchTarget::GoogleGemma,
        ExplicitResearchTarget::GoogleFlash,
    ] {
        let (kind, model) = target.identity();
        assert_eq!(
            ExplicitResearchTarget::parse(Some(kind.provider_id()), Some(model))?,
            Some(target)
        );
        assert!(ExplicitResearchTarget::parse(Some(kind.provider_id()), None).is_err());
        assert!(ExplicitResearchTarget::parse(None, Some(model)).is_err());
    }
    for (provider, model) in [
        ("openai", "gemma-4-26b-a4b-it"),
        ("gemini", "gpt-5.6-luna"),
        ("https://example.invalid", "gpt-5.6-luna"),
        ("openai", "gpt-5.6-luna "),
        ("ollama", "gpt-oss:20b"),
    ] {
        assert!(ExplicitResearchTarget::parse(Some(provider), Some(model)).is_err());
    }
    Ok(())
}

#[test]
fn research_live_target_never_enables_or_reconfigures_a_provider() -> Result<(), Box<dyn Error>> {
    let kind = ModelProviderKind::Gemini;
    let target = ExplicitResearchTarget::GoogleGemma;
    let endpoint = ConfiguredModelEndpoint::from_validated_adapter_with_security(
        ModelProviderId::try_from_string(kind.provider_id().to_owned())?,
        kind.default_origin().to_owned(),
        a3_application::ModelEndpointScope::Remote,
        a3_application::ModelEndpointAccess::ExplicitUserInitiatedRemote,
        a3_application::ProviderCredentialRequirement::ApiKey,
    )?;
    let settings = DesktopSettings::unconfigured().with_provider_endpoint(kind, Some(endpoint));
    let before = settings.clone();
    assert!(target.endpoint(&settings).is_err());
    assert_eq!(settings, before);
    let (settings, generation) = settings.begin_provider_credential_store(kind)?;
    let settings = settings
        .complete_provider_credential_store(kind, generation)?
        .with_provider_connection_verified(
            kind,
            a3_application::SettingsTimestamp::from_unix_millis(1)?,
        )?;
    assert!(target.endpoint(&settings).is_err());
    let settings = settings.with_provider_enabled(kind, true)?;
    let before = settings.clone();
    assert_eq!(target.endpoint(&settings)?.provider_id().as_str(), "gemini");
    assert!(ExplicitResearchTarget::Luna.endpoint(&settings).is_err());
    assert_eq!(settings, before);
    Ok(())
}

#[test]
fn research_live_profiles_preserve_the_eight_k_boundary() -> Result<(), Box<dyn Error>> {
    let small = fixture_profile_settings("qwen38-8k:latest")?;
    assert_eq!(small.context_limit().get(), 8192);
    assert_eq!(small.output_limit().get(), 2048);
    for model in [
        "ornith-1.5:9b",
        "gpt-oss:20b",
        "granite4.2:8b",
        "gemma-4-26b-a4b-it",
    ] {
        let profile = fixture_profile_settings(model)?;
        assert_eq!(profile.context_limit().get(), 16_384);
        assert_eq!(profile.output_limit().get(), 4096);
        assert_eq!(profile.parallelism_limit().get(), 1);
    }
    Ok(())
}

#[test]
#[ignore = "Explicit approved provider wire diagnostics with public constants only; never CI"]
fn research_live_wire_diagnostic() -> Result<(), Box<dyn Error>> {
    use a3_application::{
        ModelMessage, ModelProviderRequest, ProviderEvent, ResearchOutputPhase,
        StructuredOutputSchema,
    };
    use futures::StreamExt;
    use serde_json::json;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let live = LiveResearchModel::probe().await?;
        let simple = json!({"type":"object", "properties":{"answer":{"type":"string"}},"required":["answer"],"additionalProperties":false});
        let union = json!({"type":"object", "properties":{"answer":{"anyOf":[{"type":"string"},{"type":"integer"}]}},"required":["answer"],"additionalProperties":false});
        let initialize = a3_application::research_work_phase_schema(ResearchOutputPhase::Initialize, true)?;
        let analyze_phase = ResearchOutputPhase::Analyze(a3_domain::ResearchQuestionId::FIRST);
        let v5_analyze = a3_application::research_work_phase_schema(analyze_phase, true)?;
        let v6_analyze = a3_application::research_work_v6_phase_schema(analyze_phase, true)?;
        let v7_analyze = a3_application::research_work_current_phase_schema(analyze_phase, true)?;
        let mut v7_flat = v7_analyze.clone();
        let mut flat_result = v7_flat["$defs"]["resultPayload"].clone();
        flat_result["properties"]["kind"] = v7_flat["$defs"]["result"]["properties"]["kind"].clone();
        flat_result["required"].as_array_mut().ok_or("flat required")?.insert(1, json!("kind"));
        v7_flat["$defs"]["result"] = flat_result;
        v7_flat["$defs"].as_object_mut().ok_or("flat definitions")?.remove("resultPayload");
        let mut v7_result_only = v7_flat.clone();
        v7_result_only["properties"]["response"] = json!({"$ref":"#/$defs/result"});
        let v7_wrapped = v7_analyze.clone();
        let mut small_initialize = initialize.clone();
        small_initialize["$defs"]["work"]["properties"]["questions"]["maxItems"] = json!(2);
        small_initialize["$defs"]["question"]["properties"]["dependencies"]["maxItems"] = json!(1);
        let mut typed_initialize = initialize.clone();
        typed_initialize["properties"]["schema_version"]["type"] = json!("integer");
        let mut relaxed_initialize = initialize.clone();
        remove_diagnostic_array_bounds(&mut relaxed_initialize);
        let cases = [
            ("v5_analyze", v5_analyze),
            ("v6_analyze", v6_analyze),
            ("v6_initialize", a3_application::research_work_v6_phase_schema(ResearchOutputPhase::Initialize, true)?),
            ("v7_analyze", v7_analyze),
            ("v7_flat_analyze", v7_flat),
            ("v7_result_only", v7_result_only),
            ("v7_wrapped_analyze", v7_wrapped),
            ("v7_initialize", a3_application::research_work_current_phase_schema(ResearchOutputPhase::Initialize, true)?),
            ("relaxed_initialize", relaxed_initialize),
            ("simple", simple),
            ("union", union),
            ("null_array", json!({"type":"object","properties":{"answer":{"type":"array","maxItems":0,"items":{"type":"null"}}},"required":["answer"],"additionalProperties":false})),
            ("numeric_const", json!({"type":"object","properties":{"answer":{"const":5}},"required":["answer"],"additionalProperties":false})),
            ("small_initialize", small_initialize),
            ("typed_initialize", typed_initialize),
            ("inline_initialize", inline_diagnostic_refs(&initialize, &initialize, 0)?),
        ];
        let selected = optional_env("A3_WIRE_DIAGNOSTIC_CASE")?;
        if selected.as_deref().is_some_and(|selection| !cases.iter().any(|(name, _)| *name == selection)) {
            return Err("unknown public wire diagnostic case".into());
        }
        for (name, schema) in cases {
            if selected.as_deref().is_some_and(|selection| selection != name) { continue; }
            let question = if matches!(name, "v7_analyze" | "v7_wrapped_analyze") {
                "Analyze ACTIVE Q1: What does helper() return? Current delivered original [E1]: def helper(): return 7. Return schema_version=7 and response kind=interpretation, with result={question_id:1,text:concrete answer,evidence:[{anchor_ref:E1}]}. No work, decision, note, progress or new questions. This is public synthetic data; no tools are available."
            } else if matches!(name, "v7_flat_analyze" | "v7_result_only") {
                "Analyze ACTIVE Q1: What does helper() return? Current delivered original [E1]: def helper(): return 7. Return schema_version=7 and one response kind=interpretation, question_id=1, concrete text and evidence with anchor_ref E1. No work, decision, note, progress or new questions. This is public synthetic data; no tools are available."
            } else if name == "v7_initialize" {
                "Return schema_version=7 and response kind=questions with one required repository question about a fixture, no dependencies. No work, decision, note, progress or results. Public synthetic data; no research or tools are available."
            } else if name.ends_with("_analyze") {
                "Analyze ACTIVE Q1: What does helper() return? Current delivered original [E1]: def helper(): return 7. Return one interpretation for question_id 1 with anchor_ref E1, no new questions. Use a progress decision. If the schema requires a note, use brief status with finding_kind=hypothesis and finding_source_refs=[]. This is public synthetic data; no tools are available."
            } else {
                "Answer briefly using the supplied schema. Where applicable use a progress decision and one question about a fixture. No research or tools are available."
            };
            println!("research-wire case={name} schema_bytes={} context_bytes={}", schema.to_string().len(), question.len());
            let request = ModelProviderRequest::new(live.profile.clone(), vec![
                ModelMessage::try_from_string(ModelMessageRole::System, "Return a short JSON object matching the supplied schema. This is a synthetic wire diagnostic, no tools or actions.".to_owned())?,
                ModelMessage::try_from_string(ModelMessageRole::User, question.to_owned())?,
            ], Some(StructuredOutputSchema::new(schema)?))?;
            let result = live.provider.stream(&request, ModelRequestTimeout::from_millis(30_000)?, &ProbeControl).await;
            match result {
                Err(error) => println!("research-wire case={name} start={error:?}"),
                Ok(mut stream) => {
                    let mut bytes = 0;
                    let mut diagnostic_prefix = String::new();
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(ProviderEvent::OutputText(chunk)) => { bytes += chunk.as_str().len(); if diagnostic_prefix.len() < 1024 { diagnostic_prefix.extend(chunk.as_str().chars().take(1024 - diagnostic_prefix.len())); } },
                            Ok(ProviderEvent::Completed(done)) => println!("research-wire case={name} completion={:?} bytes={bytes}", done.reason()),
                            Err(error) => { println!("research-wire case={name} stream={error:?} bytes={bytes}"); break; }
                        }
                    }
                    // Only fixed, source-free public diagnostic output, never production content.
                    println!("research-wire public-prefix={diagnostic_prefix:?}");
                }
            }
        }
        Ok(())
    })
}

fn remove_diagnostic_array_bounds(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            if object
                .get("maxItems")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|n| n > 1)
            {
                object.remove("maxItems");
            }
            for child in object.values_mut() {
                remove_diagnostic_array_bounds(child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                remove_diagnostic_array_bounds(child);
            }
        }
        _ => {}
    }
}

// Diagnostic only: compare equivalent inlined public schemas, never change production requests.
fn inline_diagnostic_refs(
    value: &serde_json::Value,
    root: &serde_json::Value,
    depth: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
    if depth > 32 {
        return Err("diagnostic schema depth exceeded".into());
    }
    if let Some(reference) = value.get("$ref").and_then(serde_json::Value::as_str) {
        let path = reference
            .strip_prefix('#')
            .ok_or("nonlocal diagnostic reference")?;
        return inline_diagnostic_refs(
            root.pointer(path).ok_or("missing diagnostic reference")?,
            root,
            depth + 1,
        );
    }
    match value {
        serde_json::Value::Object(object) => Ok(serde_json::Value::Object(
            object
                .iter()
                .filter(|(name, _)| name.as_str() != "$defs")
                .map(|(key, child)| {
                    Ok((key.clone(), inline_diagnostic_refs(child, root, depth + 1)?))
                })
                .collect::<Result<_, Box<dyn Error>>>()?,
        )),
        serde_json::Value::Array(array) => Ok(serde_json::Value::Array(
            array
                .iter()
                .map(|child| inline_diagnostic_refs(child, root, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        _ => Ok(value.clone()),
    }
}
