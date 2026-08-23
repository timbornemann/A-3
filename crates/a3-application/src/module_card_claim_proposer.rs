use crate::{
    DecodeModuleCardClaims, JobContext, ModelFinishReason, ModelMessage, ModelMessageRole,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderRequest,
    ModelRequestTimeout, ModuleCardClaimDecodeError, ProviderEvent, StructuredOutputSchema,
};
use a3_domain::{
    ModelProfile, ModuleCardField, ModuleCardProposal, ModuleCardVerificationCandidate,
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;

const CLAIM_REQUEST_TIMEOUT_MILLIS: u64 = 30_000;
const MAX_CLAIM_OUTPUT_BYTES: usize = 65_536;
const CLAIM_SYSTEM_PROMPT: &str = "You are the bounded A^3 Module Card claim proposer. Return exactly one JSON object matching the supplied schema and no prose. For every zero-based values index of every supplied field, produce exactly one claim with that field, value_index, confidence_basis_points and polarity affirms. Copy the supplied card, module, snapshot and evidence IDs exactly; create only each new claim_id as a unique lowercase 64-character hexadecimal value. Use observed with statement exactly equal to the indexed field value and at most 16 of that field's supplied evidence IDs. Use architectural_intent only for explicitly uncertain intent. Do not use structural path, symbol or relation predicates because resolved evidence objects are not supplied at this boundary. Never invent evidence or executable actions.";

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
            return Err(ProposeModuleCardClaimsFailure::Model);
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
        match DecodeModuleCardClaims::version_one().decode(proposal.clone(), &primary) {
            Ok(candidate) => Ok(candidate),
            Err(error) => {
                let repaired = self
                    .complete(&proposal, Some(error.repair_code()), control)
                    .await?;
                DecodeModuleCardClaims::version_one()
                    .decode(proposal, &repaired)
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
        let schema = serde_json::from_str::<Value>(
            DecodeModuleCardClaims::version_one().json_schema().as_str(),
        )
        .map_err(|_| ProposeModuleCardClaimsFailure::InvalidRequest)?;
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
        let mut stream = self
            .provider
            .stream(&request, timeout, control)
            .await
            .map_err(map_provider_failure)?;
        let mut output = String::new();
        let mut completion = None;
        while let Some(event) = stream.next().await {
            match event.map_err(map_provider_failure)? {
                ProviderEvent::OutputText(chunk) if completion.is_none() => {
                    let next = output.len().checked_add(chunk.as_str().len()).ok_or(
                        ProposeModuleCardClaimsFailure::InvalidOutput(
                            ModuleCardClaimDecodeError::OutputTooLarge(usize::MAX),
                        ),
                    )?;
                    if next > MAX_CLAIM_OUTPUT_BYTES {
                        return Err(ProposeModuleCardClaimsFailure::InvalidOutput(
                            ModuleCardClaimDecodeError::OutputTooLarge(next),
                        ));
                    }
                    output.push_str(chunk.as_str());
                }
                ProviderEvent::Completed(value) if completion.is_none() => {
                    completion = Some(value);
                }
                ProviderEvent::OutputText(_) | ProviderEvent::Completed(_) => {
                    return Err(ProposeModuleCardClaimsFailure::Model);
                }
            }
        }
        if control.is_cancelled() {
            return Err(ProposeModuleCardClaimsFailure::Cancelled);
        }
        let completion = completion.ok_or(ProposeModuleCardClaimsFailure::Model)?;
        if completion.reason() == ModelFinishReason::OutputLimit {
            return Err(ProposeModuleCardClaimsFailure::InvalidOutput(
                ModuleCardClaimDecodeError::OutputTooLarge(output.len()),
            ));
        }
        Ok(output)
    }
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
            json!({
                "field": field_name(field.field()),
                "values": field.values(),
                "evidence_ids": field
                    .evidence_ids()
                    .iter()
                    .map(|id| hex(id.as_bytes()))
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
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
        ModelProviderFailure::Unavailable
        | ModelProviderFailure::Rejected
        | ModelProviderFailure::InvalidResponse
        | ModelProviderFailure::TimedOut
        | ModelProviderFailure::EndpointDenied => ProposeModuleCardClaimsFailure::Model,
    }
}

/// Stable claim-proposal failure retaining no provider payload or credential data.
#[derive(Debug)]
pub enum ProposeModuleCardClaimsFailure {
    /// The verified profile or provider operation failed.
    Model,
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
            Self::Model => "Module Card claim model failed",
            Self::InvalidRequest => "Module Card claim request is invalid",
            Self::InvalidOutput(_) => "Module Card claim output remained invalid",
            Self::Cancelled => "Module Card claim generation was cancelled",
        })
    }
}

impl Error for ProposeModuleCardClaimsFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutput(source) => Some(source),
            Self::Model | Self::InvalidRequest | Self::Cancelled => None,
        }
    }
}
