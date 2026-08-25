use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_model_stream};
use crate::{
    ExplorerModelControl, ExplorerModelFailure, ExplorerModelFuture, ExplorerModelProvider,
    ExplorerModelRequest, ExplorerModelRequestPhase, ExplorerModelTimeout,
    ExplorerObservationStatus, ExplorerRepairReason, ModelCancellationFuture, ModelFinishReason,
    ModelMessage, ModelMessageRole, ModelOperationControl, ModelProvider, ModelProviderFailure,
    ModelProviderRequest, ModelRequestTimeout, RawExplorerModelOutput, StructuredOutputSchema,
};
use a3_domain::{
    ExploreTarget, ModelProfile, ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleId,
    SnapshotId,
};
use serde_json::{Value, json};
use std::fmt;

const MAX_EXPLORER_OUTPUT_BYTES: usize = 65_536;
const MAX_PROPOSAL_VALUES_PER_FIELD: usize = 1;
const MAX_PROPOSAL_EVIDENCE_PER_FIELD: usize = 1;
const MAX_PROPOSAL_VALUE_BYTES: usize = 384;
const EXPLORER_SYSTEM_PROMPT: &str = "You are the bounded A^3 Deep Map explorer. Return exactly one JSON object matching the supplied schema and no prose. When observation is absent, request inspect for the exact planned target with expected_gain_basis_points set to 100. When observation is present, propose every expected field in the supplied order, using exactly one concise synthesized value and exactly one relevant supplied evidence_id per field. Copy the supplied card, module, snapshot and evidence IDs exactly. Never invent an identifier, path, symbol, command, tool, evidence ID or fact. Explicitly describe uncertainty.";

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
            let schema = request_schema(request)?;
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
            let collected = collect_model_stream(
                self.provider,
                &request,
                timeout,
                &bridge,
                MAX_EXPLORER_OUTPUT_BYTES,
            )
            .await
            .map_err(map_collection_failure)?;
            let (output, completion) = collected.into_parts();
            if control.is_cancelled() {
                return Err(ExplorerModelFailure::Cancelled);
            }
            if completion.reason() == ModelFinishReason::OutputLimit {
                return Err(ExplorerModelFailure::InvalidResponse);
            }
            RawExplorerModelOutput::new(output).map_err(|_| ExplorerModelFailure::InvalidResponse)
        })
    }
}

fn request_schema(request: &ExplorerModelRequest) -> Result<Value, ExplorerModelFailure> {
    let schema = serde_json::from_str::<Value>(request.action_json_schema().as_str())
        .map_err(|_| ExplorerModelFailure::InvalidResponse)?;
    match request.observation() {
        Some(observation) => specialize_proposal_schema(
            schema,
            request.module_id(),
            request.snapshot_id(),
            request.expected_fields(),
            observation.evidence_ids(),
        ),
        None => restrict_action(schema, "#/$defs/inspect"),
    }
}

fn restrict_action(mut schema: Value, reference: &str) -> Result<Value, ExplorerModelFailure> {
    let action = schema
        .pointer_mut("/properties/action")
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    *action = json!({"$ref": reference});
    Ok(schema)
}

fn specialize_proposal_schema(
    mut schema: Value,
    module_id: ModuleId,
    snapshot_id: SnapshotId,
    expected_fields: &[ModuleCardField],
    evidence_ids: &[ModuleCardEvidenceId],
) -> Result<Value, ExplorerModelFailure> {
    if expected_fields.is_empty() || evidence_ids.is_empty() {
        return Err(ExplorerModelFailure::InvalidResponse);
    }
    let field_template = schema
        .pointer("/$defs/proposalField")
        .cloned()
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    let allowed_evidence = evidence_ids
        .iter()
        .map(|id| hex(id.as_bytes()))
        .collect::<Vec<_>>();
    let definitions = schema
        .pointer_mut("/$defs")
        .and_then(Value::as_object_mut)
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    definitions.insert(
        "allowedEvidenceId".to_owned(),
        json!({"enum": allowed_evidence}),
    );
    let field_schemas = expected_fields
        .iter()
        .map(|field| specialize_field_schema(field_template.clone(), *field, evidence_ids.len()))
        .collect::<Result<Vec<_>, _>>()?;

    let action = schema
        .pointer_mut("/properties/action")
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    *action = json!({"$ref": "#/$defs/propose"});
    let proposal = schema
        .pointer_mut("/$defs/proposal/properties")
        .and_then(Value::as_object_mut)
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    proposal.insert(
        "card_id".to_owned(),
        json!({"const": hex(ModuleCardId::for_module_fields_v1(module_id, expected_fields).as_bytes())}),
    );
    proposal.insert(
        "module_id".to_owned(),
        json!({"const": module_id.to_string()}),
    );
    proposal.insert(
        "snapshot_id".to_owned(),
        json!({"const": snapshot_id.to_string()}),
    );
    proposal.insert(
        "fields".to_owned(),
        json!({
            "type": "array",
            "minItems": field_schemas.len(),
            "maxItems": field_schemas.len(),
            "prefixItems": field_schemas,
        }),
    );
    Ok(schema)
}

fn specialize_field_schema(
    mut schema: Value,
    field: ModuleCardField,
    allowed_evidence_count: usize,
) -> Result<Value, ExplorerModelFailure> {
    let properties = schema
        .pointer_mut("/properties")
        .and_then(Value::as_object_mut)
        .ok_or(ExplorerModelFailure::InvalidResponse)?;
    properties.insert("field".to_owned(), json!({"const": field_name(field)}));
    properties.insert(
        "values".to_owned(),
        json!({
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_PROPOSAL_VALUES_PER_FIELD,
            "uniqueItems": true,
            "items": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_PROPOSAL_VALUE_BYTES,
            },
        }),
    );
    properties.insert(
        "evidence_ids".to_owned(),
        json!({
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_PROPOSAL_EVIDENCE_PER_FIELD.min(allowed_evidence_count),
            "uniqueItems": true,
            "items": {"$ref": "#/$defs/allowedEvidenceId"},
        }),
    );
    Ok(schema)
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

const fn map_collection_failure(failure: ModelStreamCollectionFailure) -> ExplorerModelFailure {
    match failure {
        ModelStreamCollectionFailure::Provider(failure) => map_provider_failure(failure),
        ModelStreamCollectionFailure::OutputTooLarge(_) => ExplorerModelFailure::InvalidResponse,
    }
}

#[cfg(test)]
mod tests {
    use super::{hex, restrict_action, specialize_proposal_schema};
    use crate::{DecodeExplorerAction, ExplorerModelFailure, StructuredOutputSchema};
    use a3_domain::{ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleId, SnapshotId};
    use serde_json::Value;
    use std::error::Error;

    #[test]
    fn inspect_request_exposes_only_the_authorized_action() -> Result<(), Box<dyn Error>> {
        let schema = serde_json::from_str::<Value>(
            DecodeExplorerAction::version_one().json_schema().as_str(),
        )?;
        let schema = restrict_action(schema, "#/$defs/inspect")?;
        assert_eq!(
            schema.pointer("/properties/action/$ref"),
            Some(&Value::String("#/$defs/inspect".to_owned()))
        );
        Ok(())
    }

    #[test]
    fn proposal_schema_binds_ids_fields_and_evidence() -> Result<(), ExplorerModelFailure> {
        let schema = serde_json::from_str::<Value>(
            DecodeExplorerAction::version_one().json_schema().as_str(),
        )
        .map_err(|_| ExplorerModelFailure::InvalidResponse)?;
        let schema = specialize_proposal_schema(
            schema,
            ModuleId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            &[ModuleCardField::Title, ModuleCardField::Purpose],
            &[
                ModuleCardEvidenceId::from_bytes([3; 32]),
                ModuleCardEvidenceId::from_bytes([4; 32]),
            ],
        )?;

        assert_eq!(
            schema.pointer("/properties/action/$ref"),
            Some(&Value::String("#/$defs/propose".to_owned()))
        );
        assert_eq!(
            schema.pointer("/$defs/proposal/properties/card_id/const"),
            Some(&Value::String(hex(ModuleCardId::for_module_fields_v1(
                ModuleId::from_bytes([1; 32]),
                &[ModuleCardField::Title, ModuleCardField::Purpose]
            )
            .as_bytes())))
        );
        assert_eq!(
            schema.pointer("/$defs/proposal/properties/fields/maxItems"),
            Some(&Value::from(2))
        );
        assert_eq!(
            schema
                .pointer("/$defs/proposal/properties/fields/prefixItems/0/properties/field/const"),
            Some(&Value::String("title".to_owned()))
        );
        assert_eq!(
            schema.pointer(
                "/$defs/proposal/properties/fields/prefixItems/0/properties/values/maxItems"
            ),
            Some(&Value::from(1))
        );
        assert_eq!(
            schema.pointer(
                "/$defs/proposal/properties/fields/prefixItems/1/properties/evidence_ids/maxItems"
            ),
            Some(&Value::from(1))
        );
        assert_eq!(
            schema.pointer(
                "/$defs/proposal/properties/fields/prefixItems/1/properties/evidence_ids/items/$ref"
            ),
            Some(&Value::String("#/$defs/allowedEvidenceId".to_owned()))
        );
        assert_eq!(
            schema.pointer("/$defs/allowedEvidenceId/enum/0"),
            Some(&Value::String("03".repeat(32)))
        );
        assert!(
            schema
                .pointer("/$defs/proposal/properties/fields/items")
                .is_none(),
            "Ollama streaming rejects boolean tuple-array schemas"
        );
        Ok(())
    }

    #[test]
    fn largest_observation_schema_stays_within_the_provider_boundary()
    -> Result<(), ExplorerModelFailure> {
        let schema = serde_json::from_str::<Value>(
            DecodeExplorerAction::version_one().json_schema().as_str(),
        )
        .map_err(|_| ExplorerModelFailure::InvalidResponse)?;
        let fields = [
            ModuleCardField::Title,
            ModuleCardField::Paths,
            ModuleCardField::Purpose,
            ModuleCardField::Responsibilities,
            ModuleCardField::PublicSurface,
            ModuleCardField::Entrypoints,
            ModuleCardField::Dependencies,
            ModuleCardField::DataFlows,
            ModuleCardField::Invariants,
            ModuleCardField::Tests,
            ModuleCardField::Risks,
            ModuleCardField::OpenQuestions,
        ];
        let evidence_ids = (0_u8..100)
            .map(|value| ModuleCardEvidenceId::from_bytes([value; 32]))
            .collect::<Vec<_>>();
        let schema = specialize_proposal_schema(
            schema,
            ModuleId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            &fields,
            &evidence_ids,
        )?;
        StructuredOutputSchema::new(schema).map_err(|_| ExplorerModelFailure::InvalidResponse)?;
        Ok(())
    }
}
