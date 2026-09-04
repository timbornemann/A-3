use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const SCHEMA: &str = include_str!("../schemas/ask-research-decision-v2.schema.json");
const MAX_OUTPUT_BYTES: usize = 320 * 1024;

/// Static strict JSON Schema for one bounded multi-round research decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskResearchDecisionJsonSchema;

impl AskResearchDecisionJsonSchema {
    /// Returns the version-two provider-neutral JSON Schema document.
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AskResearchAction {
    /// Recompile Task Lens with a more specific query.
    SearchIndex(String),
    /// Search safe current source for one to eight literals.
    SearchSourceText(Vec<String>),
    /// Inspect an exact path resolved against the pinned index.
    InspectPath(String),
    /// Inspect a previously issued turn-local source reference.
    InspectSource(u16),
    /// Follow one closed relationship class from a known source.
    InspectRelations {
        /// Turn-local source ordinal used as the traversal anchor.
        source_ordinal: u16,
        /// Closed relation direction or semantic kind.
        relation: AskResearchRelation,
    },
    /// List direct indexed children below one repository directory.
    ListDirectory(String),
    /// Inspect the current staged, unstaged, and safely readable untracked paths.
    InspectWorkingChanges,
    /// Read bounded parser and index diagnostics from the pinned publication.
    QueryIndexDiagnostics,
    /// Inspect bounded internal and manifest-backed dependency relations.
    InspectDependencyGraph,
    /// Inspect indexed test-to-subject relations without claiming runtime coverage.
    InspectTestTopology,
    /// Search current source with the versioned local security candidate rules.
    ScanSecurityCandidates,
}

/// Closed relationship that the read-only research controller may inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AskResearchRelation {
    /// Symbols that call the selected symbol.
    Callers,
    /// Symbols called by the selected symbol.
    Callees,
    /// Direct import relationships.
    Imports,
    /// Direct export relationships.
    Exports,
    /// Direct test relationships.
    Tests,
}

/// Epistemic classification of one public research finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResearchFindingKind {
    /// Directly observed in current source or index evidence.
    Observation,
    /// Explicitly unproven search lead.
    Hypothesis,
    /// Evidence-backed conclusion across one or more sources.
    Conclusion,
}

/// Bounded public work note. This is presentation data and never executable input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchDecisionNote {
    /// Current sub-goal.
    pub goal: String,
    /// Epistemic status of the finding.
    pub finding_kind: AskResearchFindingKind,
    /// Public observation, hypothesis, or conclusion.
    pub finding: String,
    /// Turn-local sources supporting an observation or conclusion.
    pub source_ordinals: Vec<u16>,
    /// Evidence still missing.
    pub gap: String,
    /// Purpose of the next action or final response.
    pub next_step: String,
}

/// Validated answer or one bounded read-only research round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskResearchDecision {
    /// Final user-facing Markdown plus the source ordinals claimed by the model.
    Answer {
        /// Bounded user-facing Markdown.
        markdown: String,
        /// Turn-local source ordinals explicitly used by the answer.
        source_ordinals: Vec<u16>,
        /// Public, non-authoritative work note for this decision.
        note: AskResearchDecisionNote,
    },
    /// One to four bounded read-only actions.
    Research {
        /// Public, non-authoritative work note for this decision.
        note: AskResearchDecisionNote,
        /// Sequentially executed read-only actions.
        actions: Vec<AskResearchAction>,
    },
}

/// Strict version-two decoder paired with the provider schema.
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
        if root.get("schema_version").and_then(Value::as_u64) != Some(2) {
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
    exact(value, &["kind", "note", "markdown", "source_refs"])?;
    let note = decode_note(
        value
            .get("note")
            .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
    )?;
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
    if markdown_source_ordinals(&markdown)? != seen {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    Ok(AskResearchDecision::Answer {
        markdown,
        source_ordinals: ordinals,
        note,
    })
}

fn markdown_source_ordinals(
    markdown: &str,
) -> Result<BTreeSet<u16>, AskResearchDecisionDecodeError> {
    let mut result = BTreeSet::new();
    let mut fenced = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || line.starts_with("    ") {
            continue;
        }
        let characters = line.char_indices().collect::<Vec<_>>();
        let mut index = 0usize;
        let mut inline_delimiter = None;
        while index < characters.len() {
            if characters[index].1 == '`' {
                let start = index;
                while index < characters.len() && characters[index].1 == '`' {
                    index += 1;
                }
                let width = index.saturating_sub(start);
                inline_delimiter = match inline_delimiter {
                    Some(open) if open == width => None,
                    Some(open) => Some(open),
                    None => Some(width),
                };
                continue;
            }
            if inline_delimiter.is_none() && characters[index].1 == '【' {
                let byte_start = characters[index].0;
                if let Some(rest) = line[byte_start..].strip_prefix("【S")
                    && let Some(end) = rest.find('】')
                {
                    let marker = &rest[..end];
                    if !marker.is_empty() && marker.bytes().all(|byte| byte.is_ascii_digit()) {
                        let ordinal = source_ordinal(&format!("S{marker}"))?;
                        result.insert(ordinal);
                        let consumed_bytes = "【S".len() + end + '】'.len_utf8();
                        while index < characters.len()
                            && characters[index].0 < byte_start.saturating_add(consumed_bytes)
                        {
                            index += 1;
                        }
                        continue;
                    }
                }
            }
            index += 1;
        }
    }
    Ok(result)
}

fn decode_research(
    value: &Map<String, Value>,
) -> Result<AskResearchDecision, AskResearchDecisionDecodeError> {
    exact(value, &["kind", "note", "actions"])?;
    let note = decode_note(
        value
            .get("note")
            .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
    )?;
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
            "inspectRelations" => {
                exact(action, &["kind", "source_ref", "relation"])?;
                AskResearchAction::InspectRelations {
                    source_ordinal: source_ordinal(string(action, "source_ref")?)?,
                    relation: match string(action, "relation")? {
                        "callers" => AskResearchRelation::Callers,
                        "callees" => AskResearchRelation::Callees,
                        "imports" => AskResearchRelation::Imports,
                        "exports" => AskResearchRelation::Exports,
                        "tests" => AskResearchRelation::Tests,
                        _ => return Err(AskResearchDecisionDecodeError::InvalidValue),
                    },
                }
            }
            "listDirectory" => {
                exact(action, &["kind", "path"])?;
                AskResearchAction::ListDirectory(bounded_allow_empty(
                    string(action, "path")?,
                    4096,
                )?)
            }
            "inspectWorkingChanges" => {
                exact(action, &["kind"])?;
                AskResearchAction::InspectWorkingChanges
            }
            "queryIndexDiagnostics" => {
                exact(action, &["kind"])?;
                AskResearchAction::QueryIndexDiagnostics
            }
            "inspectDependencyGraph" => {
                exact(action, &["kind"])?;
                AskResearchAction::InspectDependencyGraph
            }
            "inspectTestTopology" => {
                exact(action, &["kind"])?;
                AskResearchAction::InspectTestTopology
            }
            "scanSecurityCandidates" => {
                exact(action, &["kind"])?;
                AskResearchAction::ScanSecurityCandidates
            }
            _ => return Err(AskResearchDecisionDecodeError::InvalidValue),
        });
    }
    Ok(AskResearchDecision::Research { note, actions })
}

pub(crate) fn decode_note(
    value: &Value,
) -> Result<AskResearchDecisionNote, AskResearchDecisionDecodeError> {
    let note = object(value)?;
    exact(
        note,
        &[
            "goal",
            "finding_kind",
            "finding",
            "finding_source_refs",
            "gap",
            "next_step",
        ],
    )?;
    let finding_kind = match string(note, "finding_kind")? {
        "observation" => AskResearchFindingKind::Observation,
        "hypothesis" => AskResearchFindingKind::Hypothesis,
        "conclusion" => AskResearchFindingKind::Conclusion,
        _ => return Err(AskResearchDecisionDecodeError::InvalidValue),
    };
    let refs = array(note, "finding_source_refs")?;
    if refs.len() > 32 {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    let mut seen = BTreeSet::new();
    let mut source_ordinals = Vec::with_capacity(refs.len());
    for reference in refs {
        let ordinal = source_ordinal(
            reference
                .as_str()
                .ok_or(AskResearchDecisionDecodeError::InvalidShape)?,
        )?;
        if !seen.insert(ordinal) {
            return Err(AskResearchDecisionDecodeError::InvalidValue);
        }
        source_ordinals.push(ordinal);
    }
    if finding_kind != AskResearchFindingKind::Hypothesis && source_ordinals.is_empty() {
        return Err(AskResearchDecisionDecodeError::InvalidValue);
    }
    Ok(AskResearchDecisionNote {
        goal: bounded(string(note, "goal")?, 1024)?,
        finding_kind,
        finding: bounded(string(note, "finding")?, 4096)?,
        source_ordinals,
        gap: bounded(string(note, "gap")?, 1024)?,
        next_step: bounded(string(note, "next_step")?, 1024)?,
    })
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

fn bounded_allow_empty(
    value: &str,
    maximum: usize,
) -> Result<String, AskResearchDecisionDecodeError> {
    if value.len() > maximum
        || value.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
    {
        Err(AskResearchDecisionDecodeError::InvalidValue)
    } else {
        Ok(value.trim().to_owned())
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
        let decoded = DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"answer","note":{"goal":"Frage beantworten","finding_kind":"observation","finding":"Quelle gelesen","finding_source_refs":["S2"],"gap":"Keine","next_step":"Antworten"},"markdown":"Fertig 【S2】","source_refs":["S2"]}}"#)?;
        assert_eq!(
            decoded,
            AskResearchDecision::Answer {
                markdown: "Fertig 【S2】".to_owned(),
                source_ordinals: vec![2],
                note: AskResearchDecisionNote {
                    goal: "Frage beantworten".to_owned(),
                    finding_kind: AskResearchFindingKind::Observation,
                    finding: "Quelle gelesen".to_owned(),
                    source_ordinals: vec![2],
                    gap: "Keine".to_owned(),
                    next_step: "Antworten".to_owned(),
                }
            }
        );
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"answer","note":{"goal":"g","finding_kind":"hypothesis","finding":"f","finding_source_refs":[],"gap":"g","next_step":"n"},"markdown":"x","source_refs":[],"thought":"secret"}}"#).is_err());
        Ok(())
    }
    #[test]
    fn decoder_bounds_one_read_only_round() {
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"research","note":{"goal":"Aufrufe finden","finding_kind":"hypothesis","finding":"Aufrufer sind noch unbekannt","finding_source_refs":[],"gap":"Aufrufstellen","next_step":"Symbol und Aufrufer suchen"},"actions":[{"kind":"searchSourceText","literals":["TODO","FIXME"]},{"kind":"listDirectory","path":"src"}]}}"#).is_ok());
        assert!(
            DecodeAskResearchDecision
                .decode(r#"{"schema_version":2,"decision":{"kind":"research","note":{"goal":"g","finding_kind":"hypothesis","finding":"f","finding_source_refs":[],"gap":"g","next_step":"n"},"actions":[]}}"#)
                .is_err()
        );
    }

    #[test]
    fn decoder_accepts_only_closed_parameterless_analysis_actions() {
        let decoded = DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"research","note":{"goal":"Review","finding_kind":"hypothesis","finding":"Prüfung steht aus","finding_source_refs":[],"gap":"Aktuelle Analyse","next_step":"Begrenzte Analysen ausführen"},"actions":[{"kind":"inspectWorkingChanges"},{"kind":"queryIndexDiagnostics"},{"kind":"inspectDependencyGraph"},{"kind":"inspectTestTopology"}]}}"#);
        assert!(
            matches!(decoded, Ok(AskResearchDecision::Research { actions, .. }) if actions == vec![
                AskResearchAction::InspectWorkingChanges,
                AskResearchAction::QueryIndexDiagnostics,
                AskResearchAction::InspectDependencyGraph,
                AskResearchAction::InspectTestTopology,
            ])
        );
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"research","note":{"goal":"Security","finding_kind":"hypothesis","finding":"Prüfung steht aus","finding_source_refs":[],"gap":"Kandidaten","next_step":"Lokal scannen"},"actions":[{"kind":"scanSecurityCandidates","path":"."}]}}"#).is_err());
    }

    #[test]
    fn decoder_requires_sources_for_public_observations() {
        assert!(DecodeAskResearchDecision.decode(r#"{"schema_version":2,"decision":{"kind":"research","note":{"goal":"g","finding_kind":"observation","finding":"f","finding_source_refs":[],"gap":"g","next_step":"n"},"actions":[{"kind":"searchIndex","query":"q"}]}}"#).is_err());
    }

    #[test]
    fn answer_markers_must_match_structured_sources_outside_code() {
        let valid = r#"{"schema_version":2,"decision":{"kind":"answer","note":{"goal":"g","finding_kind":"observation","finding":"f","finding_source_refs":["S1","S3"],"gap":"g","next_step":"n"},"markdown":"Text 【S1】 und 【S3】\n\n```text\n【S99】\n``` sowie `【S88】`","source_refs":["S1","S3"]}}"#;
        assert!(DecodeAskResearchDecision.decode(valid).is_ok());
        for invalid in [
            r#"{"schema_version":2,"decision":{"kind":"answer","note":{"goal":"g","finding_kind":"observation","finding":"f","finding_source_refs":["S1"],"gap":"g","next_step":"n"},"markdown":"Text","source_refs":["S1"]}}"#,
            r#"{"schema_version":2,"decision":{"kind":"answer","note":{"goal":"g","finding_kind":"observation","finding":"f","finding_source_refs":["S1"],"gap":"g","next_step":"n"},"markdown":"Text 【S2】","source_refs":["S1"]}}"#,
        ] {
            assert!(DecodeAskResearchDecision.decode(invalid).is_err());
        }
    }
}
