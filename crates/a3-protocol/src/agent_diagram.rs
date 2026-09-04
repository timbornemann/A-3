use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Closed diagram family exposed to the untrusted WebView.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiagramKindV1 {
    /// Directed component or control flow.
    Flowchart,
    /// Time-ordered interaction between participants.
    Sequence,
    /// Static type or module relationships.
    Class,
    /// States and transitions.
    State,
    /// Entity relationships.
    EntityRelationship,
}

/// Content-safe summary of one evidence-bound diagram.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiagramSummaryV1 {
    /// Opaque capability bound to the active project, session, turn, and artifact.
    pub artifact_ref: String,
    /// User-message sequence that requested the diagram.
    pub user_sequence: String,
    /// Closed diagram family.
    pub kind: AgentDiagramKindV1,
    /// Bounded user-facing title.
    pub title: String,
    /// Bounded user-facing description.
    pub description: String,
    /// Whether the diagram describes an older project index.
    pub stale: bool,
}

/// One diagram plus Core-compiled Mermaid safe for local strict rendering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiagramArtifactV1 {
    /// Metadata also used by list views.
    pub summary: AgentDiagramSummaryV1,
    /// Deterministic Mermaid compiled by the Core from typed elements and edges.
    pub mermaid: String,
}

/// Requests the diagrams atomically completed with one user turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentDiagramArtifactsRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    user_sequence: String,
}

impl QueryAgentDiagramArtifactsRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque session identifier.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the decimal user-message sequence.
    #[must_use]
    pub fn user_sequence(&self) -> &str {
        &self.user_sequence
    }
}

/// Result of a bounded diagram-list request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentDiagramArtifactsResultV1 {
    /// No project is active.
    NoProject,
    /// The session or turn does not exist in the active project.
    NotFound,
    /// At most three diagrams were completed for the turn.
    Available {
        /// Bounded diagram summaries.
        artifacts: Vec<AgentDiagramSummaryV1>,
    },
}

/// Versioned diagram-list response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiagramArtifactsResponseV1 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed result projection.
    pub result: AgentDiagramArtifactsResultV1,
}

/// Requests one artifact through a Core-issued opaque reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentDiagramArtifactRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    artifact_ref: String,
}

impl QueryAgentDiagramArtifactRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque session identifier.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the opaque artifact capability.
    #[must_use]
    pub fn artifact_ref(&self) -> &str {
        &self.artifact_ref
    }
}

/// Result of loading one diagram artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentDiagramArtifactResultV1 {
    /// No project is active.
    NoProject,
    /// The capability does not resolve inside the active project and session.
    NotFound,
    /// A validated artifact is available for local rendering.
    Available {
        /// Diagram metadata and Core-compiled Mermaid.
        artifact: AgentDiagramArtifactV1,
    },
}

/// Versioned single-artifact response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiagramArtifactResponseV1 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed result projection.
    pub result: AgentDiagramArtifactResultV1,
}

/// Native export format.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiagramExportFormatV1 {
    /// Sanitized scalable vector graphics.
    Svg,
    /// Validated portable network graphics.
    Png,
}

/// Visual theme used for the rendered payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiagramExportThemeV1 {
    /// Light canvas and colors.
    Light,
    /// Dark canvas and colors.
    Dark,
    /// Transparent canvas.
    Transparent,
}

/// Requests native export without accepting a destination path from the WebView.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportAgentDiagramRequestV1 {
    protocol_version: ProtocolVersion,
    session_id: String,
    artifact_ref: String,
    format: AgentDiagramExportFormatV1,
    theme: AgentDiagramExportThemeV1,
    rendered_payload: String,
}

impl ExportAgentDiagramRequestV1 {
    /// Returns the protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque session identifier.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns the opaque artifact capability.
    #[must_use]
    pub fn artifact_ref(&self) -> &str {
        &self.artifact_ref
    }
    /// Returns the requested output format.
    #[must_use]
    pub const fn format(&self) -> AgentDiagramExportFormatV1 {
        self.format
    }
    /// Returns the requested visual theme.
    #[must_use]
    pub const fn theme(&self) -> AgentDiagramExportThemeV1 {
        self.theme
    }
    /// Returns the bounded rendered payload to validate at the privileged boundary.
    #[must_use]
    pub fn rendered_payload(&self) -> &str {
        &self.rendered_payload
    }
}

/// Native export result without disclosing the chosen file-system path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentDiagramExportResultV1 {
    /// The user closed the native dialog.
    Cancelled,
    /// The validated artifact was written atomically.
    Exported,
    /// The capability no longer resolves in the active project and session.
    NotFound,
    /// The rendered SVG or PNG failed strict validation.
    InvalidPayload,
    /// The native write could not be completed safely.
    Failed,
}

/// Versioned diagram-export response.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiagramExportResponseV1 {
    /// Current IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Path-free export result.
    pub result: AgentDiagramExportResultV1,
}
