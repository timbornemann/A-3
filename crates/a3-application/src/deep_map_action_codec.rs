use a3_domain::{
    Confidence, ExpectedInformationGain, ExplorerAction, ExplorerActionSchemaVersion,
    ExplorerInspectAction, ExplorerSearchAction, ExplorerSearchKind, ExplorerSearchLimit,
    InformationGainRationale, MapperProfileVersion, ModuleCardEvidenceId, ModuleCardField,
    ModuleCardId, ModuleCardProposal, ModuleCardProposalEnvelope, ModuleCardProposalError,
    ModuleCardSchema, ModuleCardSchemaVersion, ModuleId, ProposedModuleCardField, SnapshotId,
    SymbolId,
};
use serde_json::{Map, Value};
use std::error::Error;
use std::fmt;

const EXPLORER_ACTION_SCHEMA_V1: &str =
    include_str!("../schemas/deep-map-explorer-action-v1.schema.json");

/// Versioned JSON Schema document supplied to structured-output provider adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerActionJsonSchema {
    version: ExplorerActionSchemaVersion,
}

impl ExplorerActionJsonSchema {
    /// Returns the accepted version-one JSON Schema document.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            version: ExplorerActionSchemaVersion::V1,
        }
    }

    /// Returns the schema version named by the JSON document.
    #[must_use]
    pub const fn version(self) -> ExplorerActionSchemaVersion {
        self.version
    }

    /// Returns the embedded static JSON Schema without provider-specific wrapping.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        EXPLORER_ACTION_SCHEMA_V1
    }
}

/// Strict decoder for one complete versioned explorer action document.
#[derive(Debug, Clone, Copy)]
pub struct DecodeExplorerAction {
    schema_version: ExplorerActionSchemaVersion,
}

impl DecodeExplorerAction {
    /// Returns the accepted version-one action decoder.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            schema_version: ExplorerActionSchemaVersion::V1,
        }
    }

    /// Returns the exact JSON Schema paired with this strict runtime decoder.
    #[must_use]
    pub const fn json_schema(self) -> ExplorerActionJsonSchema {
        ExplorerActionJsonSchema {
            version: self.schema_version,
        }
    }

    /// Validates raw size, JSON shape, unknown fields, versions, bounds, and domain invariants.
    pub fn decode(self, raw: &str) -> Result<ExplorerAction, ExplorerActionDecodeError> {
        if raw.len() > ModuleCardSchema::v1().max_document_bytes() as usize {
            return Err(ExplorerActionDecodeError::OutputTooLarge(raw.len()));
        }
        let root = serde_json::from_str::<Value>(raw)
            .map_err(|_| ExplorerActionDecodeError::MalformedJson)?;
        let root = object(&root)?;
        exact_keys(root, &["schema_version", "action"])?;
        if unsigned(root, "schema_version")? != u64::from(self.schema_version.get()) {
            return Err(ExplorerActionDecodeError::UnsupportedVersion);
        }
        decode_action(object(required(root, "action")?)?, raw.len())
    }
}

fn decode_action(
    action: &Map<String, Value>,
    encoded_bytes: usize,
) -> Result<ExplorerAction, ExplorerActionDecodeError> {
    let kind = string(action, "kind")?;
    match kind {
        "inspect" => decode_inspect(action),
        "search" => decode_search(action),
        "propose" => decode_proposal_action(action, encoded_bytes),
        _ => Err(ExplorerActionDecodeError::InvalidValue),
    }
}

fn decode_inspect(
    action: &Map<String, Value>,
) -> Result<ExplorerAction, ExplorerActionDecodeError> {
    exact_keys(
        action,
        &["kind", "expected_gain_basis_points", "gain_rationale"],
    )?;
    let gain = expected_gain(action)?;
    let rationale = rationale(action)?;
    Ok(ExplorerAction::Inspect(ExplorerInspectAction::new(
        gain, rationale,
    )))
}

fn decode_search(action: &Map<String, Value>) -> Result<ExplorerAction, ExplorerActionDecodeError> {
    exact_keys(
        action,
        &[
            "kind",
            "search_kind",
            "query",
            "limit",
            "expected_gain_basis_points",
            "gain_rationale",
        ],
    )?;
    let kind = match string(action, "search_kind")? {
        "exact" => ExplorerSearchKind::Exact,
        "lexical" => ExplorerSearchKind::Lexical,
        "callers" => ExplorerSearchKind::Callers,
        "callees" => ExplorerSearchKind::Callees,
        "imports" => ExplorerSearchKind::Imports,
        "exports" => ExplorerSearchKind::Exports,
        "tests" => ExplorerSearchKind::Tests,
        _ => return Err(ExplorerActionDecodeError::InvalidValue),
    };
    let limit = ExplorerSearchLimit::new(unsigned_u16(action, "limit")?)
        .map_err(|_| ExplorerActionDecodeError::InvalidValue)?;
    let gain = expected_gain(action)?;
    let rationale = rationale(action)?;
    let query = string(action, "query")?;
    let search = if matches!(
        kind,
        ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical
    ) {
        ExplorerSearchAction::text(kind, query.to_owned(), limit, gain, rationale)
    } else {
        ExplorerSearchAction::graph(
            kind,
            SymbolId::from_bytes(hex_id(query)?),
            limit,
            gain,
            rationale,
        )
    }
    .map_err(|_| ExplorerActionDecodeError::InvalidValue)?;
    Ok(ExplorerAction::Search(search))
}

fn decode_proposal_action(
    action: &Map<String, Value>,
    encoded_bytes: usize,
) -> Result<ExplorerAction, ExplorerActionDecodeError> {
    exact_keys(action, &["kind", "proposal"])?;
    let proposal = object(required(action, "proposal")?)?;
    exact_keys(
        proposal,
        &[
            "card_id",
            "module_id",
            "snapshot_id",
            "schema_version",
            "mapper_profile_version",
            "confidence_basis_points",
            "fields",
        ],
    )?;
    if unsigned(proposal, "schema_version")? != u64::from(ModuleCardSchemaVersion::V1.get())
        || unsigned(proposal, "mapper_profile_version")?
            != u64::from(MapperProfileVersion::V1.get())
    {
        return Err(ExplorerActionDecodeError::UnsupportedVersion);
    }
    let fields = array(proposal, "fields")?
        .iter()
        .map(decode_proposal_field)
        .collect::<Result<Vec<_>, _>>()?;
    let confidence =
        Confidence::from_basis_points(unsigned_u16(proposal, "confidence_basis_points")?)
            .map_err(|_| ExplorerActionDecodeError::InvalidValue)?;
    let proposal = ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            ModuleCardId::from_bytes(hex_id(string(proposal, "card_id")?)?),
            ModuleId::from_bytes(hex_id(string(proposal, "module_id")?)?),
            SnapshotId::from_bytes(hex_id(string(proposal, "snapshot_id")?)?),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            confidence,
        ),
        fields,
        encoded_bytes,
    )?;
    Ok(ExplorerAction::Propose(proposal))
}

fn decode_proposal_field(
    value: &Value,
) -> Result<ProposedModuleCardField, ExplorerActionDecodeError> {
    let field = object(value)?;
    exact_keys(field, &["field", "values", "evidence_ids"])?;
    let field_name = string(field, "field")?;
    let field_kind = parse_field(field_name)?;
    let values = array(field, "values")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or(ExplorerActionDecodeError::InvalidShape)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence_ids = array(field, "evidence_ids")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or(ExplorerActionDecodeError::InvalidShape)
                .and_then(hex_id)
                .map(ModuleCardEvidenceId::from_bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProposedModuleCardField::new(field_kind, values, evidence_ids).map_err(Into::into)
}

fn parse_field(value: &str) -> Result<ModuleCardField, ExplorerActionDecodeError> {
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
        _ => Err(ExplorerActionDecodeError::InvalidValue),
    }
}

fn expected_gain(
    object: &Map<String, Value>,
) -> Result<ExpectedInformationGain, ExplorerActionDecodeError> {
    ExpectedInformationGain::new(unsigned_u16(object, "expected_gain_basis_points")?)
        .map_err(|_| ExplorerActionDecodeError::InvalidValue)
}

fn rationale(
    object: &Map<String, Value>,
) -> Result<InformationGainRationale, ExplorerActionDecodeError> {
    InformationGainRationale::try_from_string(string(object, "gain_rationale")?.to_owned())
        .map_err(|_| ExplorerActionDecodeError::InvalidValue)
}

fn object(value: &Value) -> Result<&Map<String, Value>, ExplorerActionDecodeError> {
    value
        .as_object()
        .ok_or(ExplorerActionDecodeError::InvalidShape)
}

fn array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], ExplorerActionDecodeError> {
    required(object, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or(ExplorerActionDecodeError::InvalidShape)
}

fn string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, ExplorerActionDecodeError> {
    required(object, key)?
        .as_str()
        .ok_or(ExplorerActionDecodeError::InvalidShape)
}

fn unsigned(object: &Map<String, Value>, key: &str) -> Result<u64, ExplorerActionDecodeError> {
    required(object, key)?
        .as_u64()
        .ok_or(ExplorerActionDecodeError::InvalidShape)
}

fn unsigned_u16(object: &Map<String, Value>, key: &str) -> Result<u16, ExplorerActionDecodeError> {
    u16::try_from(unsigned(object, key)?).map_err(|_| ExplorerActionDecodeError::InvalidValue)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, ExplorerActionDecodeError> {
    object
        .get(key)
        .ok_or(ExplorerActionDecodeError::InvalidShape)
}

fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
) -> Result<(), ExplorerActionDecodeError> {
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        return Err(ExplorerActionDecodeError::UnknownOrMissingField);
    }
    Ok(())
}

fn hex_id(value: &str) -> Result<[u8; 32], ExplorerActionDecodeError> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
    {
        return Err(ExplorerActionDecodeError::InvalidValue);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]);
        let low = hex_nibble(pair[1]);
        bytes[index] = (high << 4) | low;
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

/// Stable, content-free classification of invalid untrusted model output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerActionDecodeError {
    /// Provider output exceeded the pre-parse 64 KiB boundary.
    OutputTooLarge(usize),
    /// Output was not exactly one JSON document.
    MalformedJson,
    /// A required value had the wrong JSON type.
    InvalidShape,
    /// A required field was absent or an unknown field was present.
    UnknownOrMissingField,
    /// Action, Module Card, or mapper version was unsupported.
    UnsupportedVersion,
    /// A typed value or bound was invalid.
    InvalidValue,
    /// Module Card field or envelope invariants failed.
    InvalidProposal(ModuleCardProposalError),
}

impl ExplorerActionDecodeError {
    /// Returns a bounded category suitable for the single repair request.
    #[must_use]
    pub const fn repair_code(self) -> &'static str {
        match self {
            Self::OutputTooLarge(_) => "output_too_large",
            Self::MalformedJson => "malformed_json",
            Self::InvalidShape => "invalid_shape",
            Self::UnknownOrMissingField => "unknown_or_missing_field",
            Self::UnsupportedVersion => "unsupported_version",
            Self::InvalidValue => "invalid_value",
            Self::InvalidProposal(_) => "invalid_proposal",
        }
    }
}

impl fmt::Display for ExplorerActionDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OutputTooLarge(_) => "explorer model output exceeds the fixed size boundary",
            Self::MalformedJson => "explorer model output is not one complete JSON document",
            Self::InvalidShape => "explorer model output has an invalid JSON shape",
            Self::UnknownOrMissingField => "explorer model output has an unknown or missing field",
            Self::UnsupportedVersion => "explorer model output uses an unsupported schema version",
            Self::InvalidValue => "explorer model output contains an invalid bounded value",
            Self::InvalidProposal(_) => "explorer model output contains an invalid proposal",
        })
    }
}

impl Error for ExplorerActionDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidProposal(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ModuleCardProposalError> for ExplorerActionDecodeError {
    fn from(value: ModuleCardProposalError) -> Self {
        Self::InvalidProposal(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_accepts_typed_inspect_and_search_actions() -> Result<(), Box<dyn std::error::Error>>
    {
        let decoder = DecodeExplorerAction::version_one();
        let inspect = decoder.decode(
            r#"{"schema_version":1,"action":{"kind":"inspect","expected_gain_basis_points":500,"gain_rationale":"read current manifest"}}"#,
        )?;
        assert!(matches!(inspect, ExplorerAction::Inspect(_)));

        let symbol = "11".repeat(32);
        let graph = format!(
            r#"{{"schema_version":1,"action":{{"kind":"search","search_kind":"callers","query":"{symbol}","limit":20,"expected_gain_basis_points":400,"gain_rationale":"find direct callers"}}}}"#
        );
        assert!(matches!(decoder.decode(&graph)?, ExplorerAction::Search(_)));
        Ok(())
    }

    #[test]
    fn embedded_json_schema_is_versioned_strict_and_parseable()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = DecodeExplorerAction::version_one().json_schema();
        assert_eq!(schema.version(), ExplorerActionSchemaVersion::V1);
        let document = serde_json::from_str::<Value>(schema.as_str())?;
        assert_eq!(document["properties"]["schema_version"]["const"], 1);
        assert_eq!(document["additionalProperties"], false);
        assert_eq!(document["$defs"]["propose"]["additionalProperties"], false);
        assert!(schema.as_str().contains("evidence_ids"));
        assert!(!schema.as_str().contains("execute"));
        Ok(())
    }

    #[test]
    fn decoder_rejects_unknown_fields_trailing_text_and_evidence_free_fields() {
        let decoder = DecodeExplorerAction::version_one();
        assert_eq!(
            decoder.decode(
                r#"{"schema_version":1,"action":{"kind":"inspect","expected_gain_basis_points":500,"gain_rationale":"read","command":"git status"}}"#,
            ),
            Err(ExplorerActionDecodeError::UnknownOrMissingField)
        );
        assert_eq!(
            decoder.decode(
                r#"{"schema_version":1,"action":{"kind":"inspect","expected_gain_basis_points":500,"gain_rationale":"read"}} execute this"#,
            ),
            Err(ExplorerActionDecodeError::MalformedJson)
        );

        let id = "22".repeat(32);
        let proposal = format!(
            r#"{{"schema_version":1,"action":{{"kind":"propose","proposal":{{"card_id":"{id}","module_id":"{id}","snapshot_id":"{id}","schema_version":1,"mapper_profile_version":1,"confidence_basis_points":5000,"fields":[{{"field":"title","values":["Core"],"evidence_ids":[]}}]}}}}}}"#
        );
        assert_eq!(
            decoder.decode(&proposal),
            Err(ExplorerActionDecodeError::InvalidProposal(
                ModuleCardProposalError::MissingFieldEvidence(ModuleCardField::Title)
            ))
        );
    }

    #[test]
    fn decoder_accepts_a_field_evidence_bound_proposal() -> Result<(), Box<dyn std::error::Error>> {
        let card = "22".repeat(32);
        let module = "33".repeat(32);
        let snapshot = "44".repeat(32);
        let evidence = "55".repeat(32);
        let raw = format!(
            r#"{{"schema_version":1,"action":{{"kind":"propose","proposal":{{"card_id":"{card}","module_id":"{module}","snapshot_id":"{snapshot}","schema_version":1,"mapper_profile_version":1,"confidence_basis_points":5000,"fields":[{{"field":"title","values":["Core"],"evidence_ids":["{evidence}"]}}]}}}}}}"#
        );
        let decoded = DecodeExplorerAction::version_one().decode(&raw)?;
        let ExplorerAction::Propose(proposal) = decoded else {
            return Err("expected proposal action".into());
        };
        assert_eq!(proposal.fields()[0].field(), ModuleCardField::Title);
        assert_eq!(proposal.evidence_ids().len(), 1);
        Ok(())
    }
}
