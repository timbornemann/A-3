//! Explicitly opted-in local smoke. No desktop configuration or original repository is changed.
use super::*;
use a3_application::{ModelCapabilityProbeRequest, ModelRequestTimeout, ProbeModelProfile};
use a3_domain::*;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};

#[derive(Clone)]
pub(super) struct LiveResearchModel {
    provider: Arc<dyn a3_application::ModelProvider>,
    profile: ModelProfile,
}

impl LiveResearchModel {
    pub(super) async fn probe() -> Result<Self, Box<dyn Error>> {
        if let Ok(path) = std::env::var("A3_CONFIGURED_RESEARCH_CATALOG") {
            let stored = a3_storage_libsql::LibsqlKnowledgeStore::read_settings_snapshot(
                std::path::Path::new(&path),
            )
            .await?;
            let settings = stored.settings();
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
        let model = std::env::var("A3_LOCAL_RESEARCH_MODEL")?;
        if !matches!(
            model.as_str(),
            "qwen3.5:4b"
                | "qwen38-16k:latest"
                | "qwen38-8k:latest"
                | "ornith-1.5:9b"
                | "gemma4:12b"
        ) {
            return Err("fixture requires an explicitly reviewed installed local model".into());
        }
        // Endpoint is loopback-only. The opt-in operator must also confirm local model residency.
        let provider = Arc::new(OllamaModelProvider::new(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?);
        let (context, output) = if model == "qwen38-8k:latest" {
            (8192, 2048)
        } else {
            (16_384, 4096)
        };
        println!("local-research-profile: model={model} context={context} output={output}");
        let settings = ModelProfileSettings::new(
            ModelContextLimit::new(context)?,
            ModelOutputLimit::new(output)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::new(vec![])?,
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?;
        let control = ProbeControl;
        let profile = ProbeModelProfile::new(provider.as_ref())
            .execute(
                &ModelCapabilityProbeRequest::new(ModelId::try_from_string(model)?, settings),
                ModelRequestTimeout::DEFAULT,
                &control,
            )
            .await?;
        if profile.capabilities().structured_output() != ModelStructuredOutputCapability::Verified {
            return Err("local structured-output capability was not verified".into());
        }
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
