use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const SCHEMA: &str = include_str!("../schemas/ask-research-decision-v1.schema.json");
const MAX_OUTPUT_BYTES: usize = 320 * 1024;

/// Static strict JSON Schema for one adaptive Ask decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskResearchDecisionJsonSchema;

impl AskResearchDecisionJsonSchema {
    /// Returns the version-one provider-neutral JSON Schema document.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        SCHEMA
    }
    /// Parses the embedded schema for the provider format boundary.
    pub fn as_json(self) -> Result<Value, AskResearchDecisionDecodeError> {
        serde_json::from_str(SCHEMA).map_err(|_| AskResearchDecisionDecodeError::InvalidSchema)
    }
}

/// One strictly bounded, read-only follow-up action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskResearchAction {
    /// Recompile Task Lens with a more specific query.
    SearchIndex(String),
    /// Search safe current source for one to eight literals.
    SearchSourceText(Vec<String>),
    /// Inspect an exact path resolved against the pinned index.
    InspectPath(String),
    /// Inspect a previously issued turn-local source reference.
    InspectSource(u16),
}

/// Validated answer or one permitted adaptive read-only round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskResearchDecision {
    /// Final user-facing Markdown plus the source ordinals claimed by the model.
    Answer {
        /// Bounded user-facing Markdown.
        markdown: String,
        /// Turn-local source ordinals explicitly used by the answer.
        source_ordinals: Vec<u16>,
    },
    /// One to four bounded read-only actions.
    Research(Vec<AskResearchAction>),
}

/// Strict version-one decoder paired with the provider schema.
#[derive(Debug, Clone, Copy)]
pub struct DecodeAskResearchDecision;

impl DecodeAskResearchDecision {
    /// Returns the exact JSON Schema paired with this decoder.
    #[must_use]
    pub const fn json_schema(self) -> AskResearchDecisionJsonSchema {
        AskResearchDecisionJsonSchema
    }

    /// Validates size, shape, unknown fields, values, and all action bounds.
    pub fn decode(self, raw: &str) -> Result<AskResearchDecision, AskResearchDecisionDecodeError> {
        if raw.len() > MAX_OUTPUT_BYTES {
            return Err(AskResearchDecisionDecodeError::OutputTooLarge);
        }
        let root: Value =
            serde_json::from_str(raw).map_err(|_| AskResearchDecisionDecodeError::MalformedJson)?;
        let root = object(&root)?;
        exact(root, &["schema_version", "decision"])?;
        if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(AskResearchDecisionDecodeError::UnsupportedVersion);
        }
        let decision = object(
            root.get("decision")
                .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
        )?;
        match string(decision, "kind")? {
            "answer" => decode_answer(decision),
            "research" => decode_research(decision),
            _ => Err(AskResearchDecisionDecodeError::InvalidValue),
        }
    }
}

fn decode_answer(
    value: &Map<String, Value>,
) -> Result<AskResearchDecision, AskResearchDecisionDecodeError> {
    exact(value, &["kind", "markdown", "source_refs"])?;
    let markdown = string(value, "markdown")?.trim().to_owned();
    if markdown.is_empty() || markdown.len() > 256 * 1024 {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    let refs = array(value, "source_refs")?;
    if refs.len() > 200 {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    let mut seen = BTreeSet::new();
    let mut ordinals = Vec::with_capacity(refs.len());
    for reference in refs {
        let ordinal = source_ordinal(
            reference
                .as_str()
                .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
        )?;
        if !seen.insert(ordinal) {
            return Err(AskResearchDecisionDecodeError::InvalidValue);
        }
        ordinals.push(ordinal);
    }
    Ok(AskResearchDecision::Answer {
        markdown,
        source_ordinals: ordinals,
    })
}

fn decode_research(
    value: &Map<String, Value>,
) -> Result<AskResearchDecision, AskResearchDecisionDecodeError> {
    exact(value, &["kind", "actions"])?;
    let values = array(value, "actions")?;
    if values.is_empty() || values.len() > 4 {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    let mut actions = Vec::with_capacity(values.len());
    for value in values {
        let action = object(value)?;
        actions.push(match string(action, "kind")? {
            "searchIndex" => {
                exact(action, &["kind", "query"])?;
                AskResearchAction::SearchIndex(bounded(string(action, "query")?, 4096)?)
            }
            "searchSourceText" => {
                exact(action, &["kind", "literals"])?;
                let literals = array(action, "literals")?;
                if literals.is_empty() || literals.len() > 8 {
                    return Err(AskResearchDecisionDecodeError::InvalidValue);
                }
                let mut seen = BTreeSet::new();
                let mut decoded = Vec::with_capacity(literals.len());
                for literal in literals {
                    let literal = bounded(
                        literal
                            .as_str()
                            .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
                        256,
                    )?;
                    if !seen.insert(literal.to_lowercase()) {
                        return Err(AskResearchDecisionDecodeError::InvalidValue);
                    }
                    decoded.push(literal);
                }
                AskResearchAction::SearchSourceText(decoded)
            }
            "inspectPath" => {
                exact(action, &["kind", "path"])?;
                AskResearchAction::InspectPath(bounded(string(action, "path")?, 4096)?)
            }
            "inspectSource" => {
                exact(action, &["kind", "source_ref"])?;
                AskResearchAction::InspectSource(source_ordinal(string(action, "source_ref")?)?)
            }
            _ => return Err(AskResearchDecisionDecodeError::InvalidValue),
        });
    }
    Ok(AskResearchDecision::Research(actions))
}

fn bounded(value: &str, maximum: usize) -> Result<String, AskResearchDecisionDecodeError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > maximum
        || value.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
    {
        Err(AskResearchDecisionDecodeError::InvalidValue)
    } else {
        Ok(value.to_owned())
    }
}
fn source_ordinal(value: &str) -> Result<u16, AskResearchDecisionDecodeError> {
    let digits = value
        .strip_prefix('S')
        .ok_or(AskResearchDecisionDecodeError::InvalidValue)?;
    if digits.starts_with('0') {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    let value = digits
        .parse::<u16>()
        .map_err(|_| AskResearchDecisionDecodeError::InvalidValue)?;
    if value == 0 || value > 200 {
        Err(AskResearchDecisionDecodeError::InvalidValue)
    } else {
        Ok(value)
    }
}
fn object(value: &Value) -> Result<&Map<String, Value>, AskResearchDecisionDecodeError> {
    value
        .as_object()
        .ok_or(AskResearchDecisionDecodeError::InvalidShape)
}
fn array<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], AskResearchDecisionDecodeError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(AskResearchDecisionDecodeError::InvalidShape)
}
fn string<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, AskResearchDecisionDecodeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(AskResearchDecisionDecodeError::InvalidShape)
}
fn exact(value: &Map<String, Value>, keys: &[&str]) -> Result<(), AskResearchDecisionDecodeError> {
    if value.len() == keys.len() && keys.iter().all(|key| value.contains_key(*key)) {
        Ok(())
    } else {
        Err(AskResearchDecisionDecodeError::UnknownOrMissingField)
    }
}

/// Stable structured-output rejection class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResearchDecisionDecodeError {
    /// The embedded provider schema could not be decoded.
    InvalidSchema,
    /// The model output crossed the fixed allocation boundary.
    OutputTooLarge,
    /// The model output was not complete JSON.
    MalformedJson,
    /// A required object, array, string, or integer had the wrong shape.
    InvalidShape,
    /// A required field was absent or an unknown field was present.
    UnknownOrMissingField,
    /// The document did not select schema version one.
    UnsupportedVersion,
    /// A value crossed a closed enum or resource boundary.
    InvalidValue,
}
impl fmt::Display for AskResearchDecisionDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ask research decision is invalid")
    }
}
impl Error for AskResearchDecisionDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoder_accepts_answer_and_rejects_unknown_fields() -> Result<(), Box<dyn Error>> {
        let decoded = DecodeAskResearchDecision.decode(r#"{"schema_version":1,"decision":{"kind":"answer","markdown":"Fertig","source_refs":["S2"]}}"#)?;
        assert_eq!(
            decoded,
            AskResearchDecision::Answer {
                markdown: "Fertig".to_owned(),
                source_ordinals: vec![2]
            }
        );
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":1,"decision":{"kind":"answer","markdown":"x","source_refs":[],"thought":"secret"}}"#).is_err());
        Ok(())
    }
    #[test]
    fn decoder_bounds_one_read_only_round() {
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":1,"decision":{"kind":"research","actions":[{"kind":"searchSourceText","literals":["TODO","FIXME"]}]}}"#).is_ok());
        assert!(
            DecodeAskResearchDecision
                .decode(r#"{"schema_version":1,"decision":{"kind":"research","actions":[]}}"#)
                .is_err()
        );
    }
}
