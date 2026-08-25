use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_model_stream};
use crate::{
    DecodeModuleCardClaims, JobContext, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderRequest,
    ModelRequestTimeout, ModuleCardClaimDecodeError, StructuredOutputSchema,
};
use a3_domain::{
    ModelProfile, ModuleCardClaimId, ModuleCardField, ModuleCardProposal,
    ModuleCardVerificationCandidate,
};
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;

const CLAIM_REQUEST_TIMEOUT_MILLIS: u64 = 120_000;
const MAX_CLAIM_OUTPUT_BYTES: usize = 65_536;
const MAX_CLAIM_EVIDENCE_PER_VALUE: usize = 1;
const CLAIM_SYSTEM_PROMPT: &str = "You are the bounded A^3 Module Card claim proposer. Return exactly one JSON object matching the supplied schema and no prose. For every zero-based values index of every supplied field, produce exactly one claim with that field, value_index, confidence_basis_points and polarity affirms. Copy the supplied card, module, snapshot, evidence and corresponding claim IDs exactly. Use observed with statement exactly equal to the indexed field value and exactly one of that field's supplied evidence IDs. Use architectural_intent only for explicitly uncertain intent. Do not use structural path, symbol or relation predicates because resolved evidence objects are not supplied at this boundary. Never invent identifiers, evidence or executable actions.";

/// Produces a complete typed claim candidate with one bounded repair before deterministic verify.
pub struct ProposeModuleCardClaims<'a> {
    provider: &'a dyn ModelProvider,
    profile: ModelProfile,
}

impl<'a> ProposeModuleCardClaims<'a> {
    /// Binds claim generation to the exact same live-verified mapping profile.
    pub fn new(
        provider: &'a dyn ModelProvider,
        profile: ModelProfile,
    ) -> Result<Self, ProposeModuleCardClaimsFailure> {
        if provider.provider_id() != profile.provider_id() || !profile.executable_actions_enabled()
        {
            return Err(ProposeModuleCardClaimsFailure::Model(
                ModelProviderFailure::Rejected,
            ));
        }
        Ok(Self { provider, profile })
    }

    /// Requests, strictly decodes, and binds claims to one existing structural proposal.
    pub async fn execute(
        &self,
        proposal: ModuleCardProposal,
        control: &JobContext,
    ) -> Result<ModuleCardVerificationCandidate, ProposeModuleCardClaimsFailure> {
        let primary = self.complete(&proposal, None, control).await?;
        match decode_and_bind_claim_ids(proposal.clone(), &primary) {
            Ok(candidate) => Ok(candidate),
            Err(error) => {
                let repaired = self
                    .complete(&proposal, Some(error.repair_code()), control)
                    .await?;
                decode_and_bind_claim_ids(proposal, &repaired)
                    .map_err(ProposeModuleCardClaimsFailure::InvalidOutput)
            }
        }
    }

    async fn complete(
        &self,
        proposal: &ModuleCardProposal,
        repair: Option<&str>,
        control: &JobContext,
    ) -> Result<String, ProposeModuleCardClaimsFailure> {
        if control.is_cancelled() {
            return Err(ProposeModuleCardClaimsFailure::Cancelled);
        }
        let schema = claim_schema(proposal)?;
        let request = ModelProviderRequest::new(
            self.profile.clone(),
            vec![
                ModelMessage::try_from_string(
                    ModelMessageRole::System,
                    CLAIM_SYSTEM_PROMPT.to_owned(),
                )
                .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?,
                ModelMessage::try_from_string(
                    ModelMessageRole::User,
                    encode_proposal(proposal, repair)?,
                )
                .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?,
            ],
            Some(
                StructuredOutputSchema::new(schema)
                    .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?,
            ),
        )
        .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?;
        let timeout = ModelRequestTimeout::from_millis(CLAIM_REQUEST_TIMEOUT_MILLIS)
            .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?;
        let collected = collect_model_stream(
            self.provider,
            &request,
            timeout,
            control,
            MAX_CLAIM_OUTPUT_BYTES,
        )
        .await
        .map_err(map_collection_failure)?;
        let (output, completion) = collected.into_parts();
        if control.is_cancelled() {
            return Err(ProposeModuleCardClaimsFailure::Cancelled);
        }
        if completion.reason() == ModelFinishReason::OutputLimit {
            return Err(ProposeModuleCardClaimsFailure::InvalidOutput(
                ModuleCardClaimDecodeError::OutputTooLarge(output.len()),
            ));
        }
        Ok(output)
    }
}

fn claim_schema(proposal: &ModuleCardProposal) -> Result<Value, ProposeModuleCardClaimsFailure> {
    let mut schema =
        serde_json::from_str::<Value>(DecodeModuleCardClaims::version_one().json_schema().as_str())
            .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?;
    let claim_template = schema
        .pointer("/$defs/claim")
        .cloned()
        .ok_or(ProposeModuleCardClaimsFailure::InvalidRequest)?;
    let claims = proposal
        .fields()
        .iter()
        .flat_map(|field| {
            field
                .values()
                .iter()
                .enumerate()
                .map(move |(index, value)| (field, index, value))
        })
        .map(|(field, index, value)| {
            specialize_claim_item(claim_template.clone(), proposal, field, index, value)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if claims.is_empty() {
        return Err(ProposeModuleCardClaimsFailure::InvalidRequest);
    }
    let properties = schema
        .pointer_mut("/properties")
        .and_then(Value::as_object_mut)
        .ok_or(ProposeModuleCardClaimsFailure::InvalidRequest)?;
    properties.insert(
        "card_id".to_owned(),
        json!({"const": hex(proposal.id().as_bytes())}),
    );
    properties.insert(
        "module_id".to_owned(),
        json!({"const": proposal.module_id().to_string()}),
    );
    properties.insert(
        "snapshot_id".to_owned(),
        json!({"const": proposal.snapshot_id().to_string()}),
    );
    properties.insert(
        "claims".to_owned(),
        json!({
            "type": "array",
            "minItems": claims.len(),
            "maxItems": claims.len(),
            "prefixItems": claims,
        }),
    );
    Ok(schema)
}

fn specialize_claim_item(
    mut schema: Value,
    proposal: &ModuleCardProposal,
    field: &a3_domain::ProposedModuleCardField,
    value_index: usize,
    value: &str,
) -> Result<Value, ProposeModuleCardClaimsFailure> {
    let value_index =
        u16::try_from(value_index).map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?;
    let evidence_ids = field
        .evidence_ids()
        .iter()
        .map(|id| hex(id.as_bytes()))
        .collect::<Vec<_>>();
    if evidence_ids.is_empty() {
        return Err(ProposeModuleCardClaimsFailure::InvalidRequest);
    }
    let properties = schema
        .pointer_mut("/properties")
        .and_then(Value::as_object_mut)
        .ok_or(ProposeModuleCardClaimsFailure::InvalidRequest)?;
    properties.insert(
        "claim_id".to_owned(),
        json!({
            "const": hex(ModuleCardClaimId::for_card_value_v1(
                proposal.id(),
                field.field(),
                value_index,
            ).as_bytes()),
        }),
    );
    properties.insert(
        "field".to_owned(),
        json!({"const": field_name(field.field())}),
    );
    properties.insert("value_index".to_owned(), json!({"const": value_index}));
    properties.insert("polarity".to_owned(), json!({"const": "affirms"}));
    properties.insert(
        "predicate".to_owned(),
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "statement"],
                    "properties": {
                        "kind": {"const": "observed"},
                        "statement": {"const": value},
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "statement"],
                    "properties": {
                        "kind": {"const": "architectural_intent"},
                        "statement": {"const": value},
                    },
                },
            ],
        }),
    );
    properties.insert(
        "evidence_ids".to_owned(),
        json!({
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_CLAIM_EVIDENCE_PER_VALUE.min(evidence_ids.len()),
            "uniqueItems": true,
            "items": {"enum": evidence_ids},
        }),
    );
    Ok(schema)
}

impl fmt::Debug for ProposeModuleCardClaims<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProposeModuleCardClaims")
            .field("provider_id", self.provider.provider_id())
            .field("profile", &self.profile.reference())
            .finish()
    }
}

fn encode_proposal(
    proposal: &ModuleCardProposal,
    repair: Option<&str>,
) -> Result<String, ProposeModuleCardClaimsFailure> {
    let fields = proposal
        .fields()
        .iter()
        .map(|field| {
            let claim_ids = field
                .values()
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    u16::try_from(index)
                        .map(|index| {
                            hex(ModuleCardClaimId::for_card_value_v1(
                                proposal.id(),
                                field.field(),
                                index,
                            )
                            .as_bytes())
                        })
                        .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, ProposeModuleCardClaimsFailure>(json!({
                "field": field_name(field.field()),
                "values": field.values(),
                "claim_ids": claim_ids,
                "evidence_ids": field
                    .evidence_ids()
                    .iter()
                    .map(|id| hex(id.as_bytes()))
                    .collect::<Vec<_>>(),
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&json!({
        "card_id": hex(proposal.id().as_bytes()),
        "module_id": proposal.module_id().to_string(),
        "snapshot_id": proposal.snapshot_id().to_string(),
        "card_schema_version": proposal.schema_version().get(),
        "mapper_profile_version": proposal.mapper_profile_version().get(),
        "confidence_basis_points": proposal.confidence().basis_points(),
        "fields": fields,
        "repair_reason": repair,
    }))
    .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)
}

fn decode_and_bind_claim_ids(
    proposal: ModuleCardProposal,
    raw: &str,
) -> Result<ModuleCardVerificationCandidate, ModuleCardClaimDecodeError> {
    let candidate = DecodeModuleCardClaims::version_one().decode(proposal, raw)?;
    if candidate.claims().iter().any(|claim| {
        claim.id()
            != ModuleCardClaimId::for_card_value_v1(
                candidate.proposal().id(),
                claim.field(),
                claim.value_index(),
            )
    }) {
        return Err(ModuleCardClaimDecodeError::InvalidValue);
    }
    Ok(candidate)
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

const fn map_provider_failure(failure: ModelProviderFailure) -> ProposeModuleCardClaimsFailure {
    match failure {
        ModelProviderFailure::Cancelled => ProposeModuleCardClaimsFailure::Cancelled,
        failure => ProposeModuleCardClaimsFailure::Model(failure),
    }
}

const fn map_collection_failure(
    failure: ModelStreamCollectionFailure,
) -> ProposeModuleCardClaimsFailure {
    match failure {
        ModelStreamCollectionFailure::Provider(failure) => map_provider_failure(failure),
        ModelStreamCollectionFailure::OutputTooLarge(actual) => {
            ProposeModuleCardClaimsFailure::InvalidOutput(
                ModuleCardClaimDecodeError::OutputTooLarge(actual),
            )
        }
    }
}

/// Stable claim-proposal failure retaining no provider payload or credential data.
#[derive(Debug)]
pub enum ProposeModuleCardClaimsFailure {
    /// The verified profile or provider operation failed.
    Model(ModelProviderFailure),
    /// The request could not satisfy the bounded structured contract.
    InvalidRequest,
    /// Output remained structurally invalid after the sole repair.
    InvalidOutput(ModuleCardClaimDecodeError),
    /// Cooperative cancellation stopped claim generation.
    Cancelled,
}

impl fmt::Display for ProposeModuleCardClaimsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Model(_) => "Module Card claim model failed",
            Self::InvalidRequest => "Module Card claim request is invalid",
            Self::InvalidOutput(_) => "Module Card claim output remained invalid",
            Self::Cancelled => "Module Card claim generation was cancelled",
        })
    }
}

impl Error for ProposeModuleCardClaimsFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Model(source) => Some(source),
            Self::InvalidOutput(source) => Some(source),
            Self::InvalidRequest | Self::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_schema, decode_and_bind_claim_ids, encode_proposal, hex};
    use crate::ModuleCardClaimDecodeError;
    use a3_domain::{
        Confidence, MapperProfileVersion, ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField,
        ModuleCardId, ModuleCardProposal, ModuleCardProposalEnvelope, ModuleCardSchemaVersion,
        ModuleId, ProposedModuleCardField, SnapshotId,
    };
    use serde_json::{Value, json};
    use std::error::Error;

    #[test]
    fn request_supplies_core_owned_claim_ids_and_rejects_model_invented_identity()
    -> Result<(), Box<dyn Error>> {
        let proposal = proposal()?;
        let encoded = encode_proposal(&proposal, None)?;
        let request = serde_json::from_str::<Value>(&encoded)?;
        let expected =
            ModuleCardClaimId::for_card_value_v1(proposal.id(), ModuleCardField::Title, 0);
        assert_eq!(
            request["fields"][0]["claim_ids"][0],
            hex(expected.as_bytes())
        );

        let raw = json!({
            "schema_version": 1,
            "card_id": hex(proposal.id().as_bytes()),
            "module_id": proposal.module_id().to_string(),
            "snapshot_id": proposal.snapshot_id().to_string(),
            "claims": [{
                "claim_id": hex(&[8; 32]),
                "field": "title",
                "value_index": 0,
                "confidence_basis_points": 7000,
                "polarity": "affirms",
                "predicate": {"kind": "observed", "statement": "Core"},
                "evidence_ids": [hex(&[4; 32])]
            }]
        })
        .to_string();
        assert_eq!(
            decode_and_bind_claim_ids(proposal, &raw),
            Err(ModuleCardClaimDecodeError::InvalidValue)
        );
        Ok(())
    }

    #[test]
    fn structured_schema_binds_every_claim_to_the_existing_proposal() -> Result<(), Box<dyn Error>>
    {
        let proposal = proposal()?;
        let expected =
            ModuleCardClaimId::for_card_value_v1(proposal.id(), ModuleCardField::Title, 0);
        let schema = claim_schema(&proposal)?;

        assert_eq!(
            schema.pointer("/properties/claims/maxItems"),
            Some(&Value::from(1))
        );
        assert_eq!(
            schema.pointer("/properties/claims/prefixItems/0/properties/claim_id/const"),
            Some(&Value::String(hex(expected.as_bytes())))
        );
        assert_eq!(
            schema.pointer("/properties/claims/prefixItems/0/properties/predicate/oneOf/0/properties/statement/const"),
            Some(&Value::String("Core".to_owned()))
        );
        assert!(
            schema.pointer("/properties/claims/items").is_none(),
            "Ollama streaming rejects boolean tuple-array schemas"
        );
        Ok(())
    }

    fn proposal() -> Result<ModuleCardProposal, Box<dyn Error>> {
        Ok(ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                ModuleCardId::from_bytes([1; 32]),
                ModuleId::from_bytes([2; 32]),
                SnapshotId::from_bytes([3; 32]),
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::certain(),
            ),
            vec![ProposedModuleCardField::new(
                ModuleCardField::Title,
                vec!["Core".to_owned()],
                vec![ModuleCardEvidenceId::from_bytes([4; 32])],
            )?],
            512,
        )?)
    }
}
