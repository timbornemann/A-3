use crate::{IndexLanguageV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict request for a source preview selected from a Core-issued Evidence hook.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapSourcePreviewRequestV1 {
    protocol_version: ProtocolVersion,
    current_index_run_id: String,
    current_snapshot_id: String,
    source_index_run_id: String,
    source_snapshot_id: String,
    card_id: String,
    module_id: String,
    evidence_id: String,
}

impl QueryProjectMapSourcePreviewRequestV1 {
    /// Creates an untrusted request containing IDs only, never a path or range.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        current_index_run_id: String,
        current_snapshot_id: String,
        source_index_run_id: String,
        source_snapshot_id: String,
        card_id: String,
        module_id: String,
        evidence_id: String,
    ) -> Self {
        Self {
            protocol_version,
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            evidence_id,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted visible publication-run anchor.
    #[must_use]
    pub fn current_index_run_id(&self) -> &str {
        &self.current_index_run_id
    }

    /// Returns the untrusted visible publication-snapshot anchor.
    #[must_use]
    pub fn current_snapshot_id(&self) -> &str {
        &self.current_snapshot_id
    }

    /// Returns the untrusted Card source-run anchor.
    #[must_use]
    pub fn source_index_run_id(&self) -> &str {
        &self.source_index_run_id
    }

    /// Returns the untrusted Card source-snapshot anchor.
    #[must_use]
    pub fn source_snapshot_id(&self) -> &str {
        &self.source_snapshot_id
    }

    /// Returns the untrusted visible Card identity.
    #[must_use]
    pub fn card_id(&self) -> &str {
        &self.card_id
    }

    /// Returns the untrusted visible module identity.
    #[must_use]
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    /// Returns the untrusted opaque Evidence identity.
    #[must_use]
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
}

/// Versioned result of one bounded source-preview read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSourcePreviewResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectMapSourcePreviewResultV1,
}

impl ProjectMapSourcePreviewResponseV1 {
    /// Creates the response used when no project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::NoProject)
    }

    /// Creates the response used before the first publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::NoPublishedIndex)
    }

    /// Creates the response for publications without the module projection.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::ProjectionUnavailable)
    }

    /// Creates the response for a missing or supplementary module.
    #[must_use]
    pub const fn module_unavailable() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::ModuleUnavailable)
    }

    /// Creates the response for a module without a verified Card.
    #[must_use]
    pub const fn card_unavailable() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::CardUnavailable)
    }

    /// Creates the response for anchors invalidated by replacement publication.
    #[must_use]
    pub const fn selection_changed() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::SelectionChanged)
    }

    /// Creates the response for an Evidence ID absent from the visible Card.
    #[must_use]
    pub const fn evidence_unavailable() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::EvidenceUnavailable)
    }

    /// Creates the metadata-only response for historical Evidence.
    #[must_use]
    pub const fn stale_evidence() -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::StaleEvidence)
    }

    /// Creates one available bounded plain-text preview.
    #[must_use]
    pub const fn available(preview: ProjectMapSourcePreviewV1) -> Self {
        Self::with_result(ProjectMapSourcePreviewResultV1::Available { preview })
    }

    const fn with_result(result: ProjectMapSourcePreviewResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive preview result.
    #[must_use]
    pub const fn result(&self) -> &ProjectMapSourcePreviewResultV1 {
        &self.result
    }
}

/// Closed availability state for the source-preview capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectMapSourcePreviewResultV1 {
    /// No project is active.
    NoProject,
    /// No atomic publication exists.
    NoPublishedIndex,
    /// The publication predates the module projection.
    ProjectionUnavailable,
    /// The selected module is not a current primary module.
    ModuleUnavailable,
    /// The selected module has no verified Card.
    CardUnavailable,
    /// The selection belongs to an older publication or Card.
    SelectionChanged,
    /// The Evidence hook is not a member of the visible Card.
    EvidenceUnavailable,
    /// Historical Evidence remains inspectable as metadata only.
    StaleEvidence,
    /// A current Evidence hook produced one bounded source page.
    Available {
        /// Plain-text source projection.
        preview: ProjectMapSourcePreviewV1,
    },
}

/// Visible source coordinates for one Evidence span.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSourceHighlightV1 {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

impl ProjectMapSourceHighlightV1 {
    /// Creates one application-validated visible highlight.
    #[must_use]
    pub const fn new(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> Self {
        Self {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

/// Bounded WebView-safe source page. Text is always plain UTF-8, never HTML.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSourcePreviewV1 {
    language: IndexLanguageV1,
    path_display: String,
    start_line: u32,
    line_count: u16,
    highlight: Option<ProjectMapSourceHighlightV1>,
    text: String,
    truncated_before: bool,
    truncated_after: bool,
}

impl ProjectMapSourcePreviewV1 {
    /// Creates one already validated source projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        language: IndexLanguageV1,
        path_display: String,
        start_line: u32,
        line_count: u16,
        highlight: Option<ProjectMapSourceHighlightV1>,
        text: String,
        truncated_before: bool,
        truncated_after: bool,
    ) -> Self {
        Self {
            language,
            path_display,
            start_line,
            line_count,
            highlight,
            text,
            truncated_before,
            truncated_after,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejects_paths_ranges_limits_and_unknown_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{
            "protocolVersion": 1,
            "currentIndexRunId": "11",
            "currentSnapshotId": "22",
            "sourceIndexRunId": "33",
            "sourceSnapshotId": "44",
            "cardId": "55",
            "moduleId": "66",
            "evidenceId": "77",
            "path": "src/lib.rs"
        }"#;
        assert!(serde_json::from_str::<QueryProjectMapSourcePreviewRequestV1>(json).is_err());
        Ok(())
    }

    #[test]
    fn available_preview_serializes_as_plain_text_with_fixed_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        let response =
            ProjectMapSourcePreviewResponseV1::available(ProjectMapSourcePreviewV1::new(
                IndexLanguageV1::Rust,
                "src/lib.rs".to_owned(),
                9,
                2,
                Some(ProjectMapSourceHighlightV1::new(10, 4, 10, 8)),
                "fn main() {}\n".to_owned(),
                true,
                false,
            ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(value["result"]["preview"]["text"], "fn main() {}\n");
        assert!(value["result"]["preview"].get("html").is_none());
        Ok(())
    }
}
