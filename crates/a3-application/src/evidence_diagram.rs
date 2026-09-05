use a3_domain::{AgentDiagramArtifactId, AskResearchSourceId};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const SCHEMA: &str = include_str!("../schemas/evidence-diagram-v1.schema.json");
const MAX_OUTPUT_BYTES: usize = 192 * 1024;

/// Closed diagram family supported by the safe deterministic compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceDiagramKind {
    /// Directed component or control flow.
    Flowchart,
    /// Time-ordered participant interaction.
    Sequence,
    /// Static type or module relationship.
    Class,
    /// State and transition model.
    State,
    /// Entity relationship model.
    EntityRelationship,
}

impl EvidenceDiagramKind {
    /// Returns the stable persistence and IPC name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::Class => "class",
            Self::State => "state",
            Self::EntityRelationship => "er",
        }
    }

    /// Parses the closed persistence name.
    #[must_use]
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "flowchart" => Some(Self::Flowchart),
            "sequence" => Some(Self::Sequence),
            "class" => Some(Self::Class),
            "state" => Some(Self::State),
            "er" => Some(Self::EntityRelationship),
            _ => None,
        }
    }
}

/// One evidence-backed node emitted by the model under the strict diagram schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDiagramElement {
    key: String,
    label: String,
    _category: String,
    source_ordinals: Vec<u16>,
}

impl EvidenceDiagramElement {
    /// Returns the turn-local evidence references supporting the element.
    #[must_use]
    pub fn source_ordinals(&self) -> &[u16] {
        &self.source_ordinals
    }
}

/// One evidence-backed directed relationship between known diagram nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDiagramRelationship {
    from: String,
    to: String,
    label: String,
    source_ordinals: Vec<u16>,
}

impl EvidenceDiagramRelationship {
    /// Returns the turn-local evidence references supporting the relationship.
    #[must_use]
    pub fn source_ordinals(&self) -> &[u16] {
        &self.source_ordinals
    }
}

/// Strict evidence diagram before turn-local references are bound to durable source IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDiagramDraft {
    kind: EvidenceDiagramKind,
    title: String,
    description: String,
    elements: Vec<EvidenceDiagramElement>,
    relationships: Vec<EvidenceDiagramRelationship>,
}

impl EvidenceDiagramDraft {
    /// Returns the closed diagram kind.
    #[must_use]
    pub const fn kind(&self) -> EvidenceDiagramKind {
        self.kind
    }
    /// Returns the bounded title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Returns the bounded explanation.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Iterates every source ordinal used anywhere in this diagram.
    pub fn source_ordinals(&self) -> impl Iterator<Item = u16> + '_ {
        self.elements
            .iter()
            .flat_map(|element| element.source_ordinals.iter().copied())
            .chain(
                self.relationships
                    .iter()
                    .flat_map(|relationship| relationship.source_ordinals.iter().copied()),
            )
    }

    /// Compiles safe deterministic Mermaid from typed elements; model-authored Mermaid is never accepted.
    #[must_use]
    pub fn compile_mermaid(&self) -> String {
        let aliases = self
            .elements
            .iter()
            .enumerate()
            .map(|(index, element)| (element.key.as_str(), format!("n{index}")))
            .collect::<BTreeMap<_, _>>();
        let mut output = String::new();
        match self.kind {
            EvidenceDiagramKind::Flowchart => output.push_str("flowchart TD\n"),
            EvidenceDiagramKind::Sequence => output.push_str("sequenceDiagram\n"),
            EvidenceDiagramKind::Class => output.push_str("classDiagram\n"),
            EvidenceDiagramKind::State => output.push_str("stateDiagram-v2\n"),
            EvidenceDiagramKind::EntityRelationship => output.push_str("erDiagram\n"),
        }
        for element in &self.elements {
            let alias = aliases
                .get(element.key.as_str())
                .map_or("n", String::as_str);
            let label = mermaid_text(&element.label);
            match self.kind {
                EvidenceDiagramKind::Flowchart => {
                    output.push_str(&format!("  {alias}[\"{label}\"]\n"));
                }
                EvidenceDiagramKind::Sequence => {
                    output.push_str(&format!("  participant {alias} as {label}\n"));
                }
                EvidenceDiagramKind::Class => {
                    output.push_str(&format!("  class {alias}[\"{label}\"]\n"));
                }
                EvidenceDiagramKind::State => {
                    output.push_str(&format!("  state \"{label}\" as {alias}\n"));
                }
                EvidenceDiagramKind::EntityRelationship => {
                    output.push_str(&format!(
                        "  {alias} {{\n    string label \"{label}\"\n  }}\n"
                    ));
                }
            }
        }
        for relationship in &self.relationships {
            let Some(from) = aliases.get(relationship.from.as_str()) else {
                continue;
            };
            let Some(to) = aliases.get(relationship.to.as_str()) else {
                continue;
            };
            let label = mermaid_text(&relationship.label);
            match self.kind {
                EvidenceDiagramKind::Flowchart => {
                    output.push_str(&format!("  {from} -->|\"{label}\"| {to}\n"));
                }
                EvidenceDiagramKind::Sequence => {
                    output.push_str(&format!("  {from}->>{to}: {label}\n"));
                }
                EvidenceDiagramKind::Class => {
                    output.push_str(&format!("  {from} --> {to} : {label}\n"));
                }
                EvidenceDiagramKind::State => {
                    output.push_str(&format!("  {from} --> {to}: {label}\n"));
                }
                EvidenceDiagramKind::EntityRelationship => {
                    output.push_str(&format!("  {from} ||--o{{ {to} : \"{label}\"\n"));
                }
            }
        }
        output
    }
}

/// Persistable diagram after every source reference has been rebound to durable turn evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDiagramArtifact {
    id: AgentDiagramArtifactId,
    kind: EvidenceDiagramKind,
    title: String,
    description: String,
    mermaid: String,
    source_ids: Vec<AskResearchSourceId>,
}

impl EvidenceDiagramArtifact {
    /// Binds one validated draft to a new opaque artifact and deduplicated durable sources.
    #[must_use]
    pub fn new(
        id: AgentDiagramArtifactId,
        draft: &EvidenceDiagramDraft,
        source_ids: Vec<AskResearchSourceId>,
    ) -> Self {
        Self {
            id,
            kind: draft.kind,
            title: draft.title.clone(),
            description: draft.description.clone(),
            mermaid: draft.compile_mermaid(),
            source_ids,
        }
    }
    /// Revalidates one persisted deterministic artifact at the storage boundary.
    pub fn restore(
        id: AgentDiagramArtifactId,
        kind: EvidenceDiagramKind,
        title: String,
        description: String,
        mermaid: String,
        source_ids: Vec<AskResearchSourceId>,
    ) -> Result<Self, EvidenceDiagramDecodeError> {
        let expected_prefix = match kind {
            EvidenceDiagramKind::Flowchart => "flowchart TD\n",
            EvidenceDiagramKind::Sequence => "sequenceDiagram\n",
            EvidenceDiagramKind::Class => "classDiagram\n",
            EvidenceDiagramKind::State => "stateDiagram-v2\n",
            EvidenceDiagramKind::EntityRelationship => "erDiagram\n",
        };
        if bounded(&title, 256).is_err()
            || bounded(&description, 2048).is_err()
            || mermaid.is_empty()
            || mermaid.len() > 65_536
            || !mermaid.starts_with(expected_prefix)
            || mermaid.contains('<')
            || mermaid.contains("click ")
            || mermaid.contains("%%{")
            || source_ids.is_empty()
            || source_ids.len() > 200
            || source_ids
                .iter()
                .enumerate()
                .any(|(index, source_id)| source_ids[..index].contains(source_id))
        {
            return Err(EvidenceDiagramDecodeError::InvalidValue);
        }
        Ok(Self {
            id,
            kind,
            title,
            description,
            mermaid,
            source_ids,
        })
    }
    /// Returns the opaque artifact ID.
    #[must_use]
    pub const fn id(&self) -> AgentDiagramArtifactId {
        self.id
    }
    /// Returns the closed kind.
    #[must_use]
    pub const fn kind(&self) -> EvidenceDiagramKind {
        self.kind
    }
    /// Returns the title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Returns the description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Returns the deterministic, Core-compiled Mermaid source.
    #[must_use]
    pub fn mermaid(&self) -> &str {
        &self.mermaid
    }
    /// Returns exact source IDs supporting the artifact.
    #[must_use]
    pub fn source_ids(&self) -> &[AskResearchSourceId] {
        &self.source_ids
    }
}

/// Embedded strict schema paired with the evidence-diagram decoder.
#[derive(Debug, Clone, Copy)]
pub struct EvidenceDiagramJsonSchema;

impl EvidenceDiagramJsonSchema {
    /// Parses the schema for the provider structured-output boundary.
    pub fn as_json(self) -> Result<Value, EvidenceDiagramDecodeError> {
        serde_json::from_str(SCHEMA).map_err(|_| EvidenceDiagramDecodeError::InvalidSchema)
    }
}

/// Strict decoder for up to three model-proposed typed diagrams.
#[derive(Debug, Clone, Copy)]
pub struct DecodeEvidenceDiagrams;

impl DecodeEvidenceDiagrams {
    /// Returns the paired provider-neutral schema.
    #[must_use]
    pub const fn json_schema(self) -> EvidenceDiagramJsonSchema {
        EvidenceDiagramJsonSchema
    }

    /// Validates shape, values, source references, topology, and fixed size limits.
    pub fn decode(
        self,
        raw: &str,
    ) -> Result<Vec<EvidenceDiagramDraft>, EvidenceDiagramDecodeError> {
        if raw.len() > MAX_OUTPUT_BYTES {
            return Err(EvidenceDiagramDecodeError::OutputTooLarge);
        }
        let root: Value =
            serde_json::from_str(raw).map_err(|_| EvidenceDiagramDecodeError::MalformedJson)?;
        let root = object(&root)?;
        exact(root, &["schema_version", "diagrams"])?;
        if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(EvidenceDiagramDecodeError::UnsupportedVersion);
        }
        let values = array(root, "diagrams")?;
        if values.is_empty() || values.len() > 3 {
            return Err(EvidenceDiagramDecodeError::InvalidValue);
        }
        values.iter().map(decode_diagram).collect()
    }
}

fn decode_diagram(value: &Value) -> Result<EvidenceDiagramDraft, EvidenceDiagramDecodeError> {
    let value = object(value)?;
    exact(
        value,
        &["type", "title", "description", "elements", "relationships"],
    )?;
    let kind = match string(value, "type")? {
        "flowchart" => EvidenceDiagramKind::Flowchart,
        "sequence" => EvidenceDiagramKind::Sequence,
        "class" => EvidenceDiagramKind::Class,
        "state" => EvidenceDiagramKind::State,
        "er" => EvidenceDiagramKind::EntityRelationship,
        _ => return Err(EvidenceDiagramDecodeError::InvalidValue),
    };
    let element_values = array(value, "elements")?;
    let relationship_values = array(value, "relationships")?;
    if element_values.is_empty() || element_values.len() > 64 || relationship_values.len() > 128 {
        return Err(EvidenceDiagramDecodeError::InvalidValue);
    }
    let mut keys = BTreeSet::new();
    let mut elements = Vec::with_capacity(element_values.len());
    for candidate in element_values {
        let candidate = object(candidate)?;
        exact(candidate, &["id", "label", "category", "source_refs"])?;
        let key = bounded(string(candidate, "id")?, 32)?;
        if !valid_key(&key) || !keys.insert(key.clone()) {
            return Err(EvidenceDiagramDecodeError::InvalidValue);
        }
        elements.push(EvidenceDiagramElement {
            key,
            label: bounded(string(candidate, "label")?, 160)?,
            _category: bounded(string(candidate, "category")?, 64)?,
            source_ordinals: source_refs(candidate, "source_refs", 16)?,
        });
    }
    let mut edge_keys = BTreeSet::new();
    let mut relationships = Vec::with_capacity(relationship_values.len());
    for candidate in relationship_values {
        let candidate = object(candidate)?;
        exact(candidate, &["from", "to", "label", "source_refs"])?;
        let from = bounded(string(candidate, "from")?, 32)?;
        let to = bounded(string(candidate, "to")?, 32)?;
        let label = bounded(string(candidate, "label")?, 160)?;
        if from == to
            || !keys.contains(&from)
            || !keys.contains(&to)
            || !edge_keys.insert((from.clone(), to.clone(), label.clone()))
        {
            return Err(EvidenceDiagramDecodeError::InvalidValue);
        }
        relationships.push(EvidenceDiagramRelationship {
            from,
            to,
            label,
            source_ordinals: source_refs(candidate, "source_refs", 16)?,
        });
    }
    Ok(EvidenceDiagramDraft {
        kind,
        title: bounded(string(value, "title")?, 256)?,
        description: bounded(string(value, "description")?, 2048)?,
        elements,
        relationships,
    })
}

fn source_refs(
    value: &Map<String, Value>,
    key: &str,
    limit: usize,
) -> Result<Vec<u16>, EvidenceDiagramDecodeError> {
    let refs = array(value, key)?;
    if refs.is_empty() || refs.len() > limit {
        return Err(EvidenceDiagramDecodeError::InvalidValue);
    }
    let mut seen = BTreeSet::new();
    refs.iter()
        .map(|value| {
            let value = value
                .as_str()
                .ok_or(EvidenceDiagramDecodeError::InvalidShape)?;
            let digits = value
                .strip_prefix('S')
                .ok_or(EvidenceDiagramDecodeError::InvalidValue)?;
            let ordinal = digits
                .parse::<u16>()
                .map_err(|_| EvidenceDiagramDecodeError::InvalidValue)?;
            if ordinal == 0 || ordinal > 200 || !seen.insert(ordinal) {
                return Err(EvidenceDiagramDecodeError::InvalidValue);
            }
            Ok(ordinal)
        })
        .collect()
}

fn mermaid_text(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '|' => "&#124;".to_owned(),
            ';' => "&#59;".to_owned(),
            '`' => "&#96;".to_owned(),
            '\n' | '\r' => " ".to_owned(),
            character if character.is_control() => " ".to_owned(),
            character => character.to_string(),
        })
        .collect()
}

fn valid_key(value: &str) -> bool {
    value
        .bytes()
        .enumerate()
        .all(|(index, byte)| byte.is_ascii_alphanumeric() || byte == b'_' && index > 0)
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
}

fn bounded(value: &str, maximum: usize) -> Result<String, EvidenceDiagramDecodeError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > maximum
        || value
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        Err(EvidenceDiagramDecodeError::InvalidValue)
    } else {
        Ok(value.to_owned())
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, EvidenceDiagramDecodeError> {
    value
        .as_object()
        .ok_or(EvidenceDiagramDecodeError::InvalidShape)
}

fn array<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], EvidenceDiagramDecodeError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(EvidenceDiagramDecodeError::InvalidShape)
}

fn string<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, EvidenceDiagramDecodeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(EvidenceDiagramDecodeError::InvalidShape)
}

fn exact(value: &Map<String, Value>, keys: &[&str]) -> Result<(), EvidenceDiagramDecodeError> {
    if value.len() == keys.len() && keys.iter().all(|key| value.contains_key(*key)) {
        Ok(())
    } else {
        Err(EvidenceDiagramDecodeError::UnknownOrMissingField)
    }
}

/// Stable rejection class for untrusted diagram model output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceDiagramDecodeError {
    /// Embedded schema is invalid.
    InvalidSchema,
    /// Output crossed its fixed byte budget.
    OutputTooLarge,
    /// Output was not complete JSON.
    MalformedJson,
    /// A JSON value had the wrong shape.
    InvalidShape,
    /// Required or unknown fields violated the closed schema.
    UnknownOrMissingField,
    /// The schema version is not supported.
    UnsupportedVersion,
    /// A value, topology, or source reference crossed a fixed invariant.
    InvalidValue,
}

impl fmt::Display for EvidenceDiagramDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("evidence diagram output is invalid")
    }
}

impl Error for EvidenceDiagramDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_compiles_typed_content_without_raw_directives() -> Result<(), Box<dyn Error>> {
        let diagrams = DecodeEvidenceDiagrams.decode(
            r#"{"schema_version":1,"diagrams":[{"type":"flowchart","title":"Flow","description":"Current flow","elements":[{"id":"entry","label":"Entry <script>","category":"function","source_refs":["S1"]},{"id":"store","label":"Store","category":"module","source_refs":["S2"]}],"relationships":[{"from":"entry","to":"store","label":"writes | data","source_refs":["S1","S2"]}]}]}"#,
        )?;
        let mermaid = diagrams[0].compile_mermaid();
        assert!(mermaid.starts_with("flowchart TD"));
        assert!(mermaid.contains("&lt;script&gt;"));
        assert!(!mermaid.contains("click"));
        Ok(())
    }

    #[test]
    fn compiler_encodes_same_line_mermaid_separators() -> Result<(), Box<dyn Error>> {
        let diagrams = DecodeEvidenceDiagrams.decode(
            r#"{"schema_version":1,"diagrams":[{"type":"sequence","title":"Flow","description":"Current flow","elements":[{"id":"entry","label":"Entry; style entry fill:red`","category":"function","source_refs":["S1"]},{"id":"store","label":"Store","category":"module","source_refs":["S2"]}],"relationships":[{"from":"entry","to":"store","label":"calls; click entry","source_refs":["S1","S2"]}]}]}"#,
        )?;
        let mermaid = diagrams[0].compile_mermaid();
        assert!(mermaid.contains("Entry&#59; style entry fill:red&#96;"));
        assert!(mermaid.contains("calls&#59; click entry"));
        assert!(!mermaid.contains("Entry; style"));
        assert!(!mermaid.contains("calls; click"));
        Ok(())
    }

    #[test]
    fn decoder_rejects_unknown_endpoints_and_raw_mermaid() {
        assert!(DecodeEvidenceDiagrams.decode(
            r#"{"schema_version":1,"diagrams":[{"type":"state","title":"x","description":"x","elements":[{"id":"a","label":"A","category":"state","source_refs":["S1"]}],"relationships":[{"from":"a","to":"missing","label":"go","source_refs":["S1"]}],"mermaid":"click a"}]}"#,
        ).is_err());
    }

    #[test]
    fn compiler_covers_every_closed_mermaid_family() -> Result<(), Box<dyn Error>> {
        for (kind, header, node, edge) in [
            (
                "flowchart",
                "flowchart TD\n",
                "n0[\"A\"]",
                "n0 -->|\"uses\"| n1",
            ),
            (
                "sequence",
                "sequenceDiagram\n",
                "participant n0 as A",
                "n0->>n1: uses",
            ),
            (
                "class",
                "classDiagram\n",
                "class n0[\"A\"]",
                "n0 --> n1 : uses",
            ),
            (
                "state",
                "stateDiagram-v2\n",
                "state \"A\" as n0",
                "n0 --> n1: uses",
            ),
            ("er", "erDiagram\n", "n0 {", "n0 ||--o{ n1 : \"uses\""),
        ] {
            let raw = format!(
                r#"{{"schema_version":1,"diagrams":[{{"type":"{kind}","title":"Map","description":"Current structure","elements":[{{"id":"a","label":"A","category":"module","source_refs":["S1"]}},{{"id":"b","label":"B","category":"module","source_refs":["S2"]}}],"relationships":[{{"from":"a","to":"b","label":"uses","source_refs":["S1","S2"]}}]}}]}}"#
            );
            let diagrams = DecodeEvidenceDiagrams.decode(&raw)?;
            let mermaid = diagrams[0].compile_mermaid();
            assert!(mermaid.starts_with(header));
            assert!(mermaid.contains(node));
            assert!(mermaid.contains(edge));
        }
        Ok(())
    }

    #[test]
    fn flowchart_compiler_quotes_method_shaped_edge_labels() -> Result<(), Box<dyn Error>> {
        let diagrams = DecodeEvidenceDiagrams.decode(
            r#"{"schema_version":1,"diagrams":[{"type":"flowchart","title":"Task flow","description":"Current flow","elements":[{"id":"manager","label":"TaskFlowManager.add_task(...)","category":"function","source_refs":["S1"]},{"id":"plugin","label":"AuditLogPlugin","category":"class","source_refs":["S2"]}],"relationships":[{"from":"manager","to":"plugin","label":"ruft p.on_task_created(task_data) auf","source_refs":["S1","S2"]}]}]}"#,
        )?;

        assert!(
            diagrams[0]
                .compile_mermaid()
                .contains("n0 -->|\"ruft p.on_task_created(task_data) auf\"| n1")
        );
        Ok(())
    }

    #[test]
    fn persisted_artifact_rejects_non_adjacent_duplicate_sources() {
        let first = AskResearchSourceId::from_bytes([1; 32]);
        let second = AskResearchSourceId::from_bytes([2; 32]);
        let restored = EvidenceDiagramArtifact::restore(
            AgentDiagramArtifactId::from_bytes([3; 32]),
            EvidenceDiagramKind::Flowchart,
            "Flow".to_owned(),
            "Current flow".to_owned(),
            "flowchart TD\n    n0[\"A\"]\n".to_owned(),
            vec![first, second, first],
        );

        assert_eq!(restored, Err(EvidenceDiagramDecodeError::InvalidValue));
    }
}
