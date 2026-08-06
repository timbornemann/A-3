use a3_domain::{
    AgentAction, AgentActionSchemaVersion, AgentFileInspection, AgentFileLineCount,
    AgentFileStartLine, AgentFinishAction, AgentGraphInspection, AgentInspectAction,
    AgentInspectTarget, AgentLedgerUpdate, AgentSearchAction, AgentSearchLimit, AgentSearchQuery,
    AgentTestSelector, AgentUpdateLedgerAction, ModuleCardClaimId, RepositoryPath, SymbolId,
    SyntaxRelationKind, TaskReplanReason, TaskStepBlockingReason, TaskStepId,
    TaskStepResultSummary, TraversalDepth, TraversalDirection, TraversalResultLimit,
};
use serde_json::{Map, Value};
use std::error::Error;
use std::fmt;

const AGENT_ACTION_SCHEMA_V1: &str = include_str!("../schemas/agent-action-v1.schema.json");
const MAX_AGENT_ACTION_DOCUMENT_BYTES: usize = 64 * 1_024;

/// Versioned JSON Schema supplied to a structured-output model provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentActionJsonSchema {
    version: AgentActionSchemaVersion,
}

impl AgentActionJsonSchema {
    /// Returns the accepted V1 AgentAction schema.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            version: AgentActionSchemaVersion::V1,
        }
    }

    /// Returns the schema version named by the document.
    #[must_use]
    pub const fn version(self) -> AgentActionSchemaVersion {
        self.version
    }

    /// Returns the embedded provider-neutral JSON Schema.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        AGENT_ACTION_SCHEMA_V1
    }

    /// Parses the statically embedded schema for the neutral provider request boundary.
    pub fn as_json(self) -> Result<Value, AgentActionSchemaError> {
        serde_json::from_str(self.as_str()).map_err(|_| AgentActionSchemaError)
    }
}

/// The build-embedded AgentAction schema could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentActionSchemaError;

impl fmt::Display for AgentActionSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("embedded AgentAction schema is invalid")
    }
}

impl Error for AgentActionSchemaError {}

/// Strict runtime decoder paired with one AgentAction schema version.
#[derive(Debug, Clone, Copy)]
pub struct DecodeAgentAction {
    version: AgentActionSchemaVersion,
}

impl DecodeAgentAction {
    /// Creates the V1 decoder.
    #[must_use]
    pub const fn version_one() -> Self {
        Self {
            version: AgentActionSchemaVersion::V1,
        }
    }

    /// Returns the exact JSON Schema paired with this decoder.
    #[must_use]
    pub const fn json_schema(self) -> AgentActionJsonSchema {
        AgentActionJsonSchema {
            version: self.version,
        }
    }

    /// Validates one complete untrusted JSON document and every domain boundary.
    pub fn decode(self, raw: &str) -> Result<AgentAction, AgentActionDecodeError> {
        if raw.len() > MAX_AGENT_ACTION_DOCUMENT_BYTES {
            return Err(AgentActionDecodeError::OutputTooLarge(raw.len()));
        }
        let root = serde_json::from_str::<Value>(raw)
            .map_err(|_| AgentActionDecodeError::MalformedJson)?;
        let root = object(&root)?;
        exact_keys(root, &["schema_version", "action"])?;
        if unsigned(root, "schema_version")? != u64::from(self.version.get()) {
            return Err(AgentActionDecodeError::UnsupportedVersion);
        }
        decode_action(object(required(root, "action")?)?)
    }
}

fn decode_action(action: &Map<String, Value>) -> Result<AgentAction, AgentActionDecodeError> {
    match string(action, "kind")? {
        "search" => decode_search(action),
        "inspect" => decode_inspect(action),
        "update_ledger" => decode_update_ledger(action),
        "finish" => decode_finish(action),
        _ => Err(AgentActionDecodeError::UnknownAction),
    }
}

fn decode_search(action: &Map<String, Value>) -> Result<AgentAction, AgentActionDecodeError> {
    exact_keys(action, &["kind", "query", "limit"])?;
    let query = AgentSearchQuery::try_from_string(string(action, "query")?.to_owned())
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    let limit = AgentSearchLimit::new(unsigned_u16(action, "limit")?)
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    Ok(AgentAction::Search(AgentSearchAction::new(query, limit)))
}

fn decode_inspect(action: &Map<String, Value>) -> Result<AgentAction, AgentActionDecodeError> {
    exact_keys(action, &["kind", "target"])?;
    let target = object(required(action, "target")?)?;
    let target = match string(target, "kind")? {
        "file" => decode_file_target(target)?,
        "symbol" => decode_symbol_target(target)?,
        "graph" => decode_graph_target(target)?,
        "claim" => decode_claim_target(target)?,
        "test" => decode_test_target(target)?,
        _ => return Err(AgentActionDecodeError::InvalidValue),
    };
    Ok(AgentAction::Inspect(AgentInspectAction::new(target)))
}

fn decode_file_target(
    target: &Map<String, Value>,
) -> Result<AgentInspectTarget, AgentActionDecodeError> {
    exact_keys(target, &["kind", "path", "start_line", "line_count"])?;
    let path = RepositoryPath::try_from_bytes(string(target, "path")?.as_bytes().to_vec())
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    let start_line = AgentFileStartLine::new(unsigned_u32(target, "start_line")?)
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    let line_count = AgentFileLineCount::new(unsigned_u16(target, "line_count")?)
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    Ok(AgentInspectTarget::File(AgentFileInspection::new(
        path, start_line, line_count,
    )))
}

fn decode_symbol_target(
    target: &Map<String, Value>,
) -> Result<AgentInspectTarget, AgentActionDecodeError> {
    exact_keys(target, &["kind", "symbol_id"])?;
    Ok(AgentInspectTarget::Symbol(SymbolId::from_bytes(hex_id(
        string(target, "symbol_id")?,
    )?)))
}

fn decode_graph_target(
    target: &Map<String, Value>,
) -> Result<AgentInspectTarget, AgentActionDecodeError> {
    exact_keys(
        target,
        &[
            "kind",
            "symbol_id",
            "direction",
            "relation",
            "depth",
            "limit",
        ],
    )?;
    let direction = match string(target, "direction")? {
        "outgoing" => TraversalDirection::Outgoing,
        "incoming" => TraversalDirection::Incoming,
        _ => return Err(AgentActionDecodeError::InvalidValue),
    };
    let relation = match string(target, "relation")? {
        "imports" => SyntaxRelationKind::Imports,
        "exports" => SyntaxRelationKind::Exports,
        "calls" => SyntaxRelationKind::Calls,
        "tests" => SyntaxRelationKind::Tests,
        _ => return Err(AgentActionDecodeError::InvalidValue),
    };
    let depth = TraversalDepth::new(unsigned_u8(target, "depth")?)
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    let limit = TraversalResultLimit::new(unsigned_u16(target, "limit")?)
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    Ok(AgentInspectTarget::Graph(AgentGraphInspection::new(
        SymbolId::from_bytes(hex_id(string(target, "symbol_id")?)?),
        direction,
        relation,
        depth,
        limit,
    )))
}

fn decode_claim_target(
    target: &Map<String, Value>,
) -> Result<AgentInspectTarget, AgentActionDecodeError> {
    exact_keys(target, &["kind", "claim_id"])?;
    Ok(AgentInspectTarget::Claim(ModuleCardClaimId::from_bytes(
        hex_id(string(target, "claim_id")?)?,
    )))
}

fn decode_test_target(
    target: &Map<String, Value>,
) -> Result<AgentInspectTarget, AgentActionDecodeError> {
    exact_keys(target, &["kind", "selector"])?;
    let selector = AgentTestSelector::try_from_string(string(target, "selector")?.to_owned())
        .map_err(|_| AgentActionDecodeError::InvalidValue)?;
    Ok(AgentInspectTarget::Test(selector))
}

fn decode_update_ledger(
    action: &Map<String, Value>,
) -> Result<AgentAction, AgentActionDecodeError> {
    exact_keys(action, &["kind", "step_id", "update"])?;
    let step_id = TaskStepId::from_bytes(hex_id(string(action, "step_id")?)?);
    let update = object(required(action, "update")?)?;
    let update = match string(update, "kind")? {
        "record_result" => {
            exact_keys(update, &["kind", "summary"])?;
            AgentLedgerUpdate::RecordResult(
                TaskStepResultSummary::try_from_string(string(update, "summary")?.to_owned())
                    .map_err(|_| AgentActionDecodeError::InvalidValue)?,
            )
        }
        "report_blocked" => {
            exact_keys(update, &["kind", "reason"])?;
            AgentLedgerUpdate::ReportBlocked(
                TaskStepBlockingReason::try_from_string(string(update, "reason")?.to_owned())
                    .map_err(|_| AgentActionDecodeError::InvalidValue)?,
            )
        }
        "request_replan" => {
            exact_keys(update, &["kind", "reason"])?;
            AgentLedgerUpdate::RequestReplan(
                TaskReplanReason::try_from_string(string(update, "reason")?.to_owned())
                    .map_err(|_| AgentActionDecodeError::InvalidValue)?,
            )
        }
        _ => return Err(AgentActionDecodeError::InvalidValue),
    };
    Ok(AgentAction::UpdateLedger(AgentUpdateLedgerAction::new(
        step_id, update,
    )))
}

fn decode_finish(action: &Map<String, Value>) -> Result<AgentAction, AgentActionDecodeError> {
    exact_keys(action, &["kind"])?;
    Ok(AgentAction::Finish(AgentFinishAction))
}

fn object(value: &Value) -> Result<&Map<String, Value>, AgentActionDecodeError> {
    value
        .as_object()
        .ok_or(AgentActionDecodeError::InvalidShape)
}

fn string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, AgentActionDecodeError> {
    required(object, key)?
        .as_str()
        .ok_or(AgentActionDecodeError::InvalidShape)
}

fn unsigned(object: &Map<String, Value>, key: &str) -> Result<u64, AgentActionDecodeError> {
    required(object, key)?
        .as_u64()
        .ok_or(AgentActionDecodeError::InvalidShape)
}

fn unsigned_u32(object: &Map<String, Value>, key: &str) -> Result<u32, AgentActionDecodeError> {
    u32::try_from(unsigned(object, key)?).map_err(|_| AgentActionDecodeError::InvalidValue)
}

fn unsigned_u16(object: &Map<String, Value>, key: &str) -> Result<u16, AgentActionDecodeError> {
    u16::try_from(unsigned(object, key)?).map_err(|_| AgentActionDecodeError::InvalidValue)
}

fn unsigned_u8(object: &Map<String, Value>, key: &str) -> Result<u8, AgentActionDecodeError> {
    u8::try_from(unsigned(object, key)?).map_err(|_| AgentActionDecodeError::InvalidValue)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, AgentActionDecodeError> {
    object.get(key).ok_or(AgentActionDecodeError::InvalidShape)
}

fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
) -> Result<(), AgentActionDecodeError> {
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        return Err(AgentActionDecodeError::UnknownOrMissingField);
    }
    Ok(())
}

fn hex_id(value: &str) -> Result<[u8; 32], AgentActionDecodeError> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
    {
        return Err(AgentActionDecodeError::InvalidValue);
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

/// Stable content-free classification of invalid untrusted AgentAction output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActionDecodeError {
    /// Provider output exceeded 64 KiB before parsing.
    OutputTooLarge(usize),
    /// Output was not exactly one complete JSON document.
    MalformedJson,
    /// A required JSON value had the wrong shape.
    InvalidShape,
    /// A required field was absent or an unknown field was present.
    UnknownOrMissingField,
    /// The root named another schema version.
    UnsupportedVersion,
    /// The action kind is not in the closed V1 union.
    UnknownAction,
    /// A typed ID, path, enum, number, or bounded text value was invalid.
    InvalidValue,
}

impl AgentActionDecodeError {
    /// Returns a content-free category suitable for the one repair request.
    #[must_use]
    pub const fn repair_code(self) -> &'static str {
        match self {
            Self::OutputTooLarge(_) => "output_too_large",
            Self::MalformedJson => "malformed_json",
            Self::InvalidShape => "invalid_shape",
            Self::UnknownOrMissingField => "unknown_or_missing_field",
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownAction => "unknown_action",
            Self::InvalidValue => "invalid_value",
        }
    }
}

impl fmt::Display for AgentActionDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OutputTooLarge(_) => "AgentAction output exceeds the fixed size boundary",
            Self::MalformedJson => "AgentAction output is not one complete JSON document",
            Self::InvalidShape => "AgentAction output has an invalid JSON shape",
            Self::UnknownOrMissingField => "AgentAction output has an unknown or missing field",
            Self::UnsupportedVersion => "AgentAction output uses an unsupported schema version",
            Self::UnknownAction => "AgentAction output names an unknown action",
            Self::InvalidValue => "AgentAction output contains an invalid bounded value",
        })
    }
}

impl Error for AgentActionDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_versioned_strict_and_contains_only_read_only_phase_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = AgentActionJsonSchema::version_one();
        let document = schema.as_json()?;

        assert_eq!(schema.version(), AgentActionSchemaVersion::V1);
        assert_eq!(document["properties"]["schema_version"]["const"], 1);
        assert_eq!(document["additionalProperties"], false);
        assert_eq!(
            document["$defs"]["graphTarget"]["additionalProperties"],
            false
        );
        assert!(schema.as_str().contains("update_ledger"));
        assert!(!schema.as_str().contains("apply_patch"));
        assert!(!schema.as_str().contains("shell"));
        assert!(!schema.as_str().contains("execute"));
        Ok(())
    }

    #[test]
    fn decoder_accepts_all_four_top_level_actions() -> Result<(), Box<dyn std::error::Error>> {
        let decoder = DecodeAgentAction::version_one();
        assert!(matches!(
            decoder.decode(
                r#"{"schema_version":1,"action":{"kind":"search","query":"ModelProfile","limit":20}}"#,
            )?,
            AgentAction::Search(_)
        ));
        assert!(matches!(
            decoder.decode(
                r#"{"schema_version":1,"action":{"kind":"inspect","target":{"kind":"file","path":"src/lib.rs","start_line":1,"line_count":200}}}"#,
            )?,
            AgentAction::Inspect(_)
        ));
        let step = "11".repeat(32);
        let update = format!(
            r#"{{"schema_version":1,"action":{{"kind":"update_ledger","step_id":"{step}","update":{{"kind":"record_result","summary":"located the model boundary"}}}}}}"#
        );
        assert!(matches!(
            decoder.decode(&update)?,
            AgentAction::UpdateLedger(_)
        ));
        assert_eq!(
            decoder.decode(r#"{"schema_version":1,"action":{"kind":"finish"}}"#)?,
            AgentAction::Finish(AgentFinishAction)
        );
        Ok(())
    }

    #[test]
    fn decoder_accepts_symbol_graph_claim_and_test_inspection_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let decoder = DecodeAgentAction::version_one();
        let id = "22".repeat(32);
        for target in [
            format!(r#"{{"kind":"symbol","symbol_id":"{id}"}}"#),
            format!(
                r#"{{"kind":"graph","symbol_id":"{id}","direction":"incoming","relation":"calls","depth":2,"limit":40}}"#
            ),
            format!(r#"{{"kind":"claim","claim_id":"{id}"}}"#),
            r#"{"kind":"test","selector":"provider::contract"}"#.to_owned(),
        ] {
            let raw = format!(
                r#"{{"schema_version":1,"action":{{"kind":"inspect","target":{target}}}}}"#
            );
            assert!(matches!(decoder.decode(&raw)?, AgentAction::Inspect(_)));
        }
        Ok(())
    }

    #[test]
    fn decoder_rejects_unknown_actions_fields_trailing_text_and_invalid_values() {
        let decoder = DecodeAgentAction::version_one();
        assert_eq!(
            decoder
                .decode(r#"{"schema_version":1,"action":{"kind":"run","argv":["git","status"]}}"#),
            Err(AgentActionDecodeError::UnknownAction)
        );
        assert_eq!(
            decoder.decode(r#"{"schema_version":1,"action":{"kind":"finish","force":true}}"#),
            Err(AgentActionDecodeError::UnknownOrMissingField)
        );
        assert_eq!(
            decoder.decode(r#"{"schema_version":1,"action":{"kind":"finish"}} trailing"#),
            Err(AgentActionDecodeError::MalformedJson)
        );
        assert_eq!(
            decoder.decode(
                r#"{"schema_version":1,"action":{"kind":"inspect","target":{"kind":"file","path":"../secret","start_line":0,"line_count":501}}}"#,
            ),
            Err(AgentActionDecodeError::InvalidValue)
        );
    }
}
