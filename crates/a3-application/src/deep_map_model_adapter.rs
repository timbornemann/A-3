use crate::{
    ExplorerModelControl, ExplorerModelFailure, ExplorerModelFuture, ExplorerModelProvider,
    ExplorerModelRequest, ExplorerModelRequestPhase, ExplorerModelTimeout,
    ExplorerObservationStatus, ExplorerRepairReason, ModelCancellationFuture, ModelFinishReason,
    ModelMessage, ModelMessageRole, ModelOperationControl, ModelProvider, ModelProviderFailure,
    ModelProviderRequest, ModelRequestTimeout, ProviderEvent, RawExplorerModelOutput,
    StructuredOutputSchema,
};
use a3_domain::{ExploreTarget, ModelProfile, ModuleCardField};
use futures::StreamExt;
use serde_json::{Value, json};
use std::fmt;

const MAX_EXPLORER_OUTPUT_BYTES: usize = 65_536;
const EXPLORER_SYSTEM_PROMPT: &str = "You are the bounded A^3 Deep Map explorer. Return exactly one JSON object matching the supplied schema and no prose. When observation is absent, request inspect for the exact planned target with expected_gain_basis_points set to 100. When observation is present, propose every expected field using only evidence_ids supplied in that observation. Copy module, snapshot and evidence IDs exactly; create only the proposal's new card_id as lowercase 64-character hexadecimal. Never invent a path, symbol, command, tool, evidence ID or fact. Keep values concise and explicitly describe uncertainty.";

/// Adapts the general streaming model boundary to the narrower Deep-Map explorer port.
pub struct ModelBackedExplorerProvider<'a> {
    provider: &'a dyn ModelProvider,
    profile: ModelProfile,
}

impl<'a> ModelBackedExplorerProvider<'a> {
    /// Binds one exact live-probed profile to its matching concrete provider.
    pub fn new(
        provider: &'a dyn ModelProvider,
        profile: ModelProfile,
    ) -> Result<Self, ExplorerModelFailure> {
        if provider.provider_id() != profile.provider_id() || !profile.executable_actions_enabled()
        {
            return Err(ExplorerModelFailure::Rejected);
        }
        Ok(Self { provider, profile })
    }
}

impl fmt::Debug for ModelBackedExplorerProvider<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelBackedExplorerProvider")
            .field("provider_id", self.provider.provider_id())
            .field("profile", &self.profile.reference())
            .finish()
    }
}

impl ExplorerModelProvider for ModelBackedExplorerProvider<'_> {
    fn complete<'a>(
        &'a self,
        request: &'a ExplorerModelRequest,
        timeout: ExplorerModelTimeout,
        control: &'a dyn ExplorerModelControl,
    ) -> ExplorerModelFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(ExplorerModelFailure::Cancelled);
            }
            let schema = serde_json::from_str::<Value>(request.action_json_schema().as_str())
                .map_err(|_| ExplorerModelFailure::InvalidResponse)?;
            let request = ModelProviderRequest::new(
                self.profile.clone(),
                vec![
                    ModelMessage::try_from_string(
                        ModelMessageRole::System,
                        EXPLORER_SYSTEM_PROMPT.to_owned(),
                    )
                    .map_err(|_| ExplorerModelFailure::InvalidResponse)?,
                    ModelMessage::try_from_string(ModelMessageRole::User, encode_request(request)?)
                        .map_err(|_| ExplorerModelFailure::InvalidResponse)?,
                ],
                Some(
                    StructuredOutputSchema::new(schema)
                        .map_err(|_| ExplorerModelFailure::InvalidResponse)?,
                ),
            )
            .map_err(|_| ExplorerModelFailure::Rejected)?;
            let timeout = u64::try_from(timeout.duration().as_millis())
                .ok()
                .and_then(|millis| ModelRequestTimeout::from_millis(millis).ok())
                .ok_or(ExplorerModelFailure::TimedOut)?;
            let bridge = ExplorerOperationControl(control);
            let mut stream = self
                .provider
                .stream(&request, timeout, &bridge)
                .await
                .map_err(map_provider_failure)?;
            let mut output = String::new();
            let mut completion = None;
            while let Some(event) = stream.next().await {
                match event.map_err(map_provider_failure)? {
                    ProviderEvent::OutputText(chunk) if completion.is_none() => {
                        let next = output
                            .len()
                            .checked_add(chunk.as_str().len())
                            .ok_or(ExplorerModelFailure::InvalidResponse)?;
                        if next > MAX_EXPLORER_OUTPUT_BYTES {
                            return Err(ExplorerModelFailure::InvalidResponse);
                        }
                        output.push_str(chunk.as_str());
                    }
                    ProviderEvent::Completed(value) if completion.is_none() => {
                        completion = Some(value);
                    }
                    ProviderEvent::OutputText(_) | ProviderEvent::Completed(_) => {
                        return Err(ExplorerModelFailure::InvalidResponse);
                    }
                }
            }
            if control.is_cancelled() {
                return Err(ExplorerModelFailure::Cancelled);
            }
            let completion = completion.ok_or(ExplorerModelFailure::InvalidResponse)?;
            if completion.reason() == ModelFinishReason::OutputLimit {
                return Err(ExplorerModelFailure::InvalidResponse);
            }
            RawExplorerModelOutput::new(output).map_err(|_| ExplorerModelFailure::InvalidResponse)
        })
    }
}

#[derive(Debug)]
struct ExplorerOperationControl<'a>(&'a dyn ExplorerModelControl);

impl ModelOperationControl for ExplorerOperationControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        self.0.cancelled()
    }
}

fn encode_request(request: &ExplorerModelRequest) -> Result<String, ExplorerModelFailure> {
    let target = match request.target() {
        ExploreTarget::Module(module_id) => json!({
            "kind": "module",
            "module_id": module_id.to_string(),
        }),
        ExploreTarget::Manifest { path, content_hash } => json!({
            "kind": "manifest",
            "path": String::from_utf8_lossy(path.as_bytes()),
            "content_hash": hex(content_hash.as_bytes()),
        }),
        ExploreTarget::Symbol(symbol_id) => json!({
            "kind": "symbol",
            "symbol_id": symbol_id.to_string(),
        }),
    };
    let observation = request.observation().map(|observation| {
        json!({
            "status": match observation.status() {
                ExplorerObservationStatus::Found => "found",
                ExplorerObservationStatus::NotFound => "not_found",
            },
            "preview": observation.preview(),
            "evidence_ids": observation
                .evidence_ids()
                .iter()
                .map(|id| hex(id.as_bytes()))
                .collect::<Vec<_>>(),
            "truncated": observation.truncated(),
        })
    });
    let phase = match request.phase() {
        ExplorerModelRequestPhase::Primary => json!({"kind": "primary"}),
        ExplorerModelRequestPhase::Repair(reason) => json!({
            "kind": "repair",
            "reason": match reason {
                ExplorerRepairReason::InvalidStructuredOutput => "invalid_structured_output",
                ExplorerRepairReason::UnauthorizedRead => "unauthorized_read",
                ExplorerRepairReason::InvalidProposal => "invalid_proposal",
            },
        }),
    };
    serde_json::to_string(&json!({
        "action_schema_version": request.action_schema_version().get(),
        "index_run_id": request.index_run_id().to_string(),
        "snapshot_id": request.snapshot_id().to_string(),
        "card_schema_version": request.card_schema_version().get(),
        "step_sequence": request.step_sequence(),
        "module_id": request.module_id().to_string(),
        "target": target,
        "expected_fields": request
            .expected_fields()
            .iter()
            .map(|field| field_name(*field))
            .collect::<Vec<_>>(),
        "observation": observation,
        "phase": phase,
    }))
    .map_err(|_| ExplorerModelFailure::InvalidResponse)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

const fn field_name(field: ModuleCardField) -> &'static str {
    match field {
        ModuleCardField::Title => "title",
        ModuleCardField::Paths => "paths",
        ModuleCardField::Purpose => "purpose",
        ModuleCardField::Responsibilities => "responsibilities",
        ModuleCardField::PublicSurface => "public_surface",
        ModuleCardField::Entrypoints => "entrypoints",
        ModuleCardField::Dependencies => "dependencies",
        ModuleCardField::DataFlows => "data_flows",
        ModuleCardField::Invariants => "invariants",
        ModuleCardField::Tests => "tests",
        ModuleCardField::Risks => "risks",
        ModuleCardField::OpenQuestions => "open_questions",
    }
}

const fn map_provider_failure(failure: ModelProviderFailure) -> ExplorerModelFailure {
    match failure {
        ModelProviderFailure::Unavailable | ModelProviderFailure::EndpointDenied => {
            ExplorerModelFailure::Unavailable
        }
        ModelProviderFailure::Rejected => ExplorerModelFailure::Rejected,
        ModelProviderFailure::InvalidResponse => ExplorerModelFailure::InvalidResponse,
        ModelProviderFailure::TimedOut => ExplorerModelFailure::TimedOut,
        ModelProviderFailure::Cancelled => ExplorerModelFailure::Cancelled,
    }
}
