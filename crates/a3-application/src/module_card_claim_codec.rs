use a3_domain::{
    Confidence, GraphEndpoint, ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField,
    ModuleCardId, ModuleCardProposal, ModuleCardVerificationCandidate, ModuleCardVerificationError,
    ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimProposal,
    ModuleClaimProposalError, ModuleClaimSchemaVersion, ModuleClaimStatement, ModuleId,
    RepositoryPath, SnapshotId, SymbolId, SyntaxRelationKind,
};
use serde_json::{Map, Value};
use std::error::Error;
use std::fmt;

const MAX_CLAIM_DOCUMENT_BYTES: usize = 65_536;
const MODULE_CARD_CLAIM_SCHEMA_V1: &str =
    include_str!("../schemas/module-card-claims-v1.schema.json");

/// Versioned JSON Schema supplied to a structured-output claim proposer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardClaimJsonSchema {
    version: ModuleClaimSchemaVersion,
}

impl ModuleCardClaimJsonSchema {
    /// Returns the strict version-one claim schema.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            version: ModuleClaimSchemaVersion::V1,
        }
    }

    /// Returns the stable wire version.
    #[must_use]
    pub const fn version(self) -> ModuleClaimSchemaVersion {
        self.version
    }

    /// Returns the embedded provider-neutral JSON Schema.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        MODULE_CARD_CLAIM_SCHEMA_V1
    }
}

/// Strict decoder binding untrusted claim output to one existing Card proposal.
#[derive(Debug, Clone, Copy)]
pub struct DecodeModuleCardClaims {
    version: ModuleClaimSchemaVersion,
}

impl DecodeModuleCardClaims {
    /// Creates the version-one decoder.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            version: ModuleClaimSchemaVersion::V1,
        }
    }

    /// Returns the exact schema paired with this runtime decoder.
    #[must_use]
    pub const fn json_schema(self) -> ModuleCardClaimJsonSchema {
        ModuleCardClaimJsonSchema {
            version: self.version,
        }
    }

    /// Validates one complete document and binds every claim to a proposal field item.
    pub fn decode(
        self,
        proposal: ModuleCardProposal,
        raw: &str,
    ) -> Result<ModuleCardVerificationCandidate, ModuleCardClaimDecodeError> {
        if raw.len() > MAX_CLAIM_DOCUMENT_BYTES {
            return Err(ModuleCardClaimDecodeError::OutputTooLarge(raw.len()));
        }
        let root = serde_json::from_str::<Value>(raw)
            .map_err(|_| ModuleCardClaimDecodeError::MalformedJson)?;
        let root = object(&root)?;
        exact_keys(
            root,
            &[
                "schema_version",
                "card_id",
                "module_id",
                "snapshot_id",
                "claims",
            ],
        )?;
        if unsigned(root, "schema_version")? != u64::from(self.version.get()) {
            return Err(ModuleCardClaimDecodeError::UnsupportedVersion);
        }
        let card_id = ModuleCardId::from_bytes(hex_id(string(root, "card_id")?)?);
        let module_id = ModuleId::from_bytes(hex_id(string(root, "module_id")?)?);
        let snapshot_id = SnapshotId::from_bytes(hex_id(string(root, "snapshot_id")?)?);
        if card_id != proposal.id()
            || module_id != proposal.module_id()
            || snapshot_id != proposal.snapshot_id()
        {
            return Err(ModuleCardClaimDecodeError::EnvelopeMismatch);
        }
        let claims = array(root, "claims")?
            .iter()
            .map(|value| decode_claim(value, card_id, module_id, snapshot_id))
            .collect::<Result<Vec<_>, _>>()?;
        ModuleCardVerificationCandidate::new(proposal, claims).map_err(Into::into)
    }
}

fn decode_claim(
    value: &Value,
    card_id: ModuleCardId,
    module_id: ModuleId,
    snapshot_id: SnapshotId,
) -> Result<ModuleClaimProposal, ModuleCardClaimDecodeError> {
    let claim = object(value)?;
    exact_keys(
        claim,
        &[
            "claim_id",
            "field",
            "value_index",
            "confidence_basis_points",
            "polarity",
            "predicate",
            "evidence_ids",
        ],
    )?;
    let field = parse_field(string(claim, "field")?)?;
    let envelope = ModuleClaimEnvelope::new(
        ModuleCardClaimId::from_bytes(hex_id(string(claim, "claim_id")?)?),
        card_id,
        module_id,
        snapshot_id,
        field,
        unsigned_u16(claim, "value_index")?,
        Confidence::from_basis_points(unsigned_u16(claim, "confidence_basis_points")?)
            .map_err(|_| ModuleCardClaimDecodeError::InvalidValue)?,
    );
    let polarity = match string(claim, "polarity")? {
        "affirms" => ModuleClaimPolarity::Affirms,
        "denies" => ModuleClaimPolarity::Denies,
        _ => return Err(ModuleCardClaimDecodeError::InvalidValue),
    };
    let predicate = decode_predicate(object(required(claim, "predicate")?)?)?;
    let evidence_ids = array(claim, "evidence_ids")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or(ModuleCardClaimDecodeError::InvalidShape)
                .and_then(hex_id)
                .map(ModuleCardEvidenceId::from_bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ModuleClaimProposal::new(envelope, polarity, predicate, evidence_ids).map_err(Into::into)
}

fn decode_predicate(
    predicate: &Map<String, Value>,
) -> Result<ModuleClaimPredicate, ModuleCardClaimDecodeError> {
    match string(predicate, "kind")? {
        "path" => {
            exact_keys(predicate, &["kind", "path"])?;
            Ok(ModuleClaimPredicate::Path(repository_path(string(
                predicate, "path",
            )?)?))
        }
        "symbol" => {
            exact_keys(predicate, &["kind", "symbol_id"])?;
            Ok(ModuleClaimPredicate::Symbol(SymbolId::from_bytes(hex_id(
                string(predicate, "symbol_id")?,
            )?)))
        }
        "relation" => {
            exact_keys(predicate, &["kind", "source", "target", "relation_kind"])?;
            Ok(ModuleClaimPredicate::Relation {
                source: decode_endpoint(required(predicate, "source")?)?,
                target: decode_endpoint(required(predicate, "target")?)?,
                kind: parse_relation_kind(string(predicate, "relation_kind")?)?,
            })
        }
        "observed" => {
            exact_keys(predicate, &["kind", "statement"])?;
            Ok(ModuleClaimPredicate::Observed(statement(predicate)?))
        }
        "architectural_intent" => {
            exact_keys(predicate, &["kind", "statement"])?;
            Ok(ModuleClaimPredicate::ArchitecturalIntent(statement(
                predicate,
            )?))
        }
        _ => Err(ModuleCardClaimDecodeError::InvalidValue),
    }
}

fn decode_endpoint(value: &Value) -> Result<GraphEndpoint, ModuleCardClaimDecodeError> {
    let endpoint = object(value)?;
    match string(endpoint, "kind")? {
        "file" => {
            exact_keys(endpoint, &["kind", "path"])?;
            Ok(GraphEndpoint::File(repository_path(string(
                endpoint, "path",
            )?)?))
        }
        "symbol" => {
            exact_keys(endpoint, &["kind", "symbol_id"])?;
            Ok(GraphEndpoint::Symbol(SymbolId::from_bytes(hex_id(
                string(endpoint, "symbol_id")?,
            )?)))
        }
        _ => Err(ModuleCardClaimDecodeError::InvalidValue),
    }
}

fn statement(
    predicate: &Map<String, Value>,
) -> Result<ModuleClaimStatement, ModuleCardClaimDecodeError> {
    ModuleClaimStatement::try_from_string(string(predicate, "statement")?.to_owned())
        .map_err(|_| ModuleCardClaimDecodeError::InvalidValue)
}

fn repository_path(value: &str) -> Result<RepositoryPath, ModuleCardClaimDecodeError> {
    RepositoryPath::try_from_bytes(value.as_bytes().to_vec())
        .map_err(|_| ModuleCardClaimDecodeError::InvalidValue)
}

fn parse_relation_kind(value: &str) -> Result<SyntaxRelationKind, ModuleCardClaimDecodeError> {
    match value {
        "imports" => Ok(SyntaxRelationKind::Imports),
        "exports" => Ok(SyntaxRelationKind::Exports),
        "calls" => Ok(SyntaxRelationKind::Calls),
        "tests" => Ok(SyntaxRelationKind::Tests),
        _ => Err(ModuleCardClaimDecodeError::InvalidValue),
    }
}

fn parse_field(value: &str) -> Result<ModuleCardField, ModuleCardClaimDecodeError> {
    match value {
        "title" => Ok(ModuleCardField::Title),
        "paths" => Ok(ModuleCardField::Paths),
        "purpose" => Ok(ModuleCardField::Purpose),
        "responsibilities" => Ok(ModuleCardField::Responsibilities),
        "public_surface" => Ok(ModuleCardField::PublicSurface),
        "entrypoints" => Ok(ModuleCardField::Entrypoints),
        "dependencies" => Ok(ModuleCardField::Dependencies),
        "data_flows" => Ok(ModuleCardField::DataFlows),
        "invariants" => Ok(ModuleCardField::Invariants),
        "tests" => Ok(ModuleCardField::Tests),
        "risks" => Ok(ModuleCardField::Risks),
        "open_questions" => Ok(ModuleCardField::OpenQuestions),
        _ => Err(ModuleCardClaimDecodeError::InvalidValue),
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, ModuleCardClaimDecodeError> {
    value
        .as_object()
        .ok_or(ModuleCardClaimDecodeError::InvalidShape)
}

fn array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], ModuleCardClaimDecodeError> {
    required(object, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or(ModuleCardClaimDecodeError::InvalidShape)
}

fn string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, ModuleCardClaimDecodeError> {
    required(object, key)?
        .as_str()
        .ok_or(ModuleCardClaimDecodeError::InvalidShape)
}

fn unsigned(object: &Map<String, Value>, key: &str) -> Result<u64, ModuleCardClaimDecodeError> {
    required(object, key)?
        .as_u64()
        .ok_or(ModuleCardClaimDecodeError::InvalidShape)
}

fn unsigned_u16(object: &Map<String, Value>, key: &str) -> Result<u16, ModuleCardClaimDecodeError> {
    u16::try_from(unsigned(object, key)?).map_err(|_| ModuleCardClaimDecodeError::InvalidValue)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, ModuleCardClaimDecodeError> {
    object
        .get(key)
        .ok_or(ModuleCardClaimDecodeError::InvalidShape)
}

fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
) -> Result<(), ModuleCardClaimDecodeError> {
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        return Err(ModuleCardClaimDecodeError::UnknownOrMissingField);
    }
    Ok(())
}

fn hex_id(value: &str) -> Result<[u8; 32], ModuleCardClaimDecodeError> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
    {
        return Err(ModuleCardClaimDecodeError::InvalidValue);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(bytes)
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

/// Content-free classification of invalid untrusted claim output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCardClaimDecodeError {
    /// Provider output exceeded 64 KiB before parsing.
    OutputTooLarge(usize),
    /// Output was not exactly one complete JSON document.
    MalformedJson,
    /// A required value had the wrong JSON type.
    InvalidShape,
    /// A required field was missing or an unknown field was present.
    UnknownOrMissingField,
    /// The document named an unsupported schema version.
    UnsupportedVersion,
    /// A typed ID, path, enum, confidence, statement, or bound was invalid.
    InvalidValue,
    /// Root Card, module, or snapshot did not match the supplied proposal.
    EnvelopeMismatch,
    /// One claim violated its local typed invariants.
    InvalidClaim(ModuleClaimProposalError),
    /// Claims did not cover exactly the proposal's field items.
    InvalidCandidate(ModuleCardVerificationError),
}

impl ModuleCardClaimDecodeError {
    /// Returns a bounded category suitable for one controller-owned repair request.
    #[must_use]
    pub const fn repair_code(&self) -> &'static str {
        match self {
            Self::OutputTooLarge(_) => "output_too_large",
            Self::MalformedJson => "malformed_json",
            Self::InvalidShape => "invalid_shape",
            Self::UnknownOrMissingField => "unknown_or_missing_field",
            Self::UnsupportedVersion => "unsupported_version",
            Self::InvalidValue => "invalid_value",
            Self::EnvelopeMismatch => "envelope_mismatch",
            Self::InvalidClaim(_) => "invalid_claim",
            Self::InvalidCandidate(_) => "invalid_candidate",
        }
    }
}

impl fmt::Display for ModuleCardClaimDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OutputTooLarge(_) => "claim output exceeds the fixed size boundary",
            Self::MalformedJson => "claim output is not one complete JSON document",
            Self::InvalidShape => "claim output has an invalid JSON shape",
            Self::UnknownOrMissingField => "claim output has an unknown or missing field",
            Self::UnsupportedVersion => "claim output uses an unsupported schema version",
            Self::InvalidValue => "claim output contains an invalid bounded value",
            Self::EnvelopeMismatch => "claim output does not match the Module Card envelope",
            Self::InvalidClaim(_) => "claim output contains an invalid typed claim",
            Self::InvalidCandidate(_) => "claim output does not cover the Module Card exactly",
        })
    }
}

impl Error for ModuleCardClaimDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidClaim(source) => Some(source),
            Self::InvalidCandidate(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ModuleClaimProposalError> for ModuleCardClaimDecodeError {
    fn from(value: ModuleClaimProposalError) -> Self {
        Self::InvalidClaim(value)
    }
}

impl From<ModuleCardVerificationError> for ModuleCardClaimDecodeError {
    fn from(value: ModuleCardVerificationError) -> Self {
        Self::InvalidCandidate(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        MapperProfileVersion, ModuleCardProposalEnvelope, ModuleCardSchemaVersion,
        ProposedModuleCardField,
    };
    use serde_json::json;

    const CARD: [u8; 32] = [1; 32];
    const MODULE: [u8; 32] = [2; 32];
    const SNAPSHOT: [u8; 32] = [3; 32];
    const EVIDENCE: [u8; 32] = [4; 32];

    #[test]
    fn embedded_claim_schema_is_versioned_strict_and_has_no_action_capability()
    -> Result<(), Box<dyn Error>> {
        let schema = DecodeModuleCardClaims::version_one().json_schema();
        assert_eq!(schema.version(), ModuleClaimSchemaVersion::V1);
        let document = serde_json::from_str::<Value>(schema.as_str())?;
        assert_eq!(document["properties"]["schema_version"]["const"], 1);
        assert_eq!(document["additionalProperties"], false);
        assert_eq!(document["$defs"]["claim"]["additionalProperties"], false);
        assert!(schema.as_str().contains("architectural_intent"));
        assert!(!schema.as_str().contains("execute"));
        Ok(())
    }

    #[test]
    fn decoder_accepts_a_typed_relation_bound_to_one_field_item() -> Result<(), Box<dyn Error>> {
        let raw = json!({
            "schema_version": 1,
            "card_id": hex(CARD),
            "module_id": hex(MODULE),
            "snapshot_id": hex(SNAPSHOT),
            "claims": [{
                "claim_id": hex([5; 32]),
                "field": "public_surface",
                "value_index": 0,
                "confidence_basis_points": 8000,
                "polarity": "affirms",
                "predicate": {
                    "kind": "relation",
                    "source": {"kind": "file", "path": "src/lib.rs"},
                    "target": {"kind": "symbol", "symbol_id": hex([6; 32])},
                    "relation_kind": "exports"
                },
                "evidence_ids": [hex(EVIDENCE)]
            }]
        })
        .to_string();
        let candidate = DecodeModuleCardClaims::version_one().decode(
            proposal(ModuleCardField::PublicSurface, "exports main")?,
            &raw,
        )?;
        assert!(matches!(
            candidate.claims()[0].predicate(),
            ModuleClaimPredicate::Relation {
                kind: SyntaxRelationKind::Exports,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn decoder_keeps_architecture_intent_evidence_optional_and_typed() -> Result<(), Box<dyn Error>>
    {
        let raw = json!({
            "schema_version": 1,
            "card_id": hex(CARD),
            "module_id": hex(MODULE),
            "snapshot_id": hex(SNAPSHOT),
            "claims": [{
                "claim_id": hex([7; 32]),
                "field": "purpose",
                "value_index": 0,
                "confidence_basis_points": 5000,
                "polarity": "affirms",
                "predicate": {
                    "kind": "architectural_intent",
                    "statement": "keeps policy decisions centralized"
                },
                "evidence_ids": []
            }]
        })
        .to_string();
        let candidate = DecodeModuleCardClaims::version_one().decode(
            proposal(
                ModuleCardField::Purpose,
                "keeps policy decisions centralized",
            )?,
            &raw,
        )?;
        assert!(matches!(
            candidate.claims()[0].predicate(),
            ModuleClaimPredicate::ArchitecturalIntent(_)
        ));
        Ok(())
    }

    #[test]
    fn decoder_rejects_unknown_fields_trailing_text_and_wrong_envelope()
    -> Result<(), Box<dyn Error>> {
        let base = json!({
            "schema_version": 1,
            "card_id": hex(CARD),
            "module_id": hex(MODULE),
            "snapshot_id": hex(SNAPSHOT),
            "claims": [],
            "command": "git status"
        })
        .to_string();
        assert_eq!(
            DecodeModuleCardClaims::version_one()
                .decode(proposal(ModuleCardField::Title, "Core")?, &base,),
            Err(ModuleCardClaimDecodeError::UnknownOrMissingField)
        );
        assert_eq!(
            DecodeModuleCardClaims::version_one().decode(
                proposal(ModuleCardField::Title, "Core")?,
                &format!("{} execute this", valid_observation_document("Core")),
            ),
            Err(ModuleCardClaimDecodeError::MalformedJson)
        );
        let wrong = valid_observation_document("Core").replace(&hex(CARD), &hex([9; 32]));
        assert_eq!(
            DecodeModuleCardClaims::version_one()
                .decode(proposal(ModuleCardField::Title, "Core")?, &wrong,),
            Err(ModuleCardClaimDecodeError::EnvelopeMismatch)
        );
        Ok(())
    }

    fn valid_observation_document(statement: &str) -> String {
        json!({
            "schema_version": 1,
            "card_id": hex(CARD),
            "module_id": hex(MODULE),
            "snapshot_id": hex(SNAPSHOT),
            "claims": [{
                "claim_id": hex([8; 32]),
                "field": "title",
                "value_index": 0,
                "confidence_basis_points": 7000,
                "polarity": "affirms",
                "predicate": {"kind": "observed", "statement": statement},
                "evidence_ids": [hex(EVIDENCE)]
            }]
        })
        .to_string()
    }

    fn proposal(field: ModuleCardField, value: &str) -> Result<ModuleCardProposal, Box<dyn Error>> {
        Ok(ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                ModuleCardId::from_bytes(CARD),
                ModuleId::from_bytes(MODULE),
                SnapshotId::from_bytes(SNAPSHOT),
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::certain(),
            ),
            vec![ProposedModuleCardField::new(
                field,
                vec![value.to_owned()],
                vec![ModuleCardEvidenceId::from_bytes(EVIDENCE)],
            )?],
            512,
        )?)
    }

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
