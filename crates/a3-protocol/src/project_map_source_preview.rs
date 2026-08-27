use crate::{IndexLanguageV1, ProjectMapIndexEvidenceSelectionV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict request for a source preview selected from a Core-issued Evidence hook.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapSourcePreviewRequestV1 {
    protocol_version: ProtocolVersion,
    selection: ProjectMapSourcePreviewSelectionV1,
}

/// Closed Evidence origin accepted by the source-preview trust boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum ProjectMapSourcePreviewSelectionV1 {
    /// Evidence selected from one visible current verified Module Card.
    ModuleCard {
        /// Current visible publication run.
        current_index_run_id: String,
        /// Current visible publication snapshot.
        current_snapshot_id: String,
        /// Publication run that supplied the Card.
        source_index_run_id: String,
        /// Snapshot that supplied the Card.
        source_snapshot_id: String,
        /// Exact visible Card identity.
        card_id: String,
        /// Exact primary module identity.
        module_id: String,
        /// Exact Card Evidence identity.
        evidence_id: String,
    },
    /// Evidence selected from the current deterministic static index.
    Index {
        /// Exact Core-issued file, symbol, or relation selection.
        evidence: ProjectMapIndexEvidenceSelectionV1,
    },
}

impl QueryProjectMapSourcePreviewRequestV1 {
    /// Creates an untrusted request containing a typed selection, never a path or range.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        selection: ProjectMapSourcePreviewSelectionV1,
    ) -> Self {
        Self {
            protocol_version,
            selection,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the closed Core-issued Evidence selection.
    #[must_use]
    pub const fn selection(&self) -> &ProjectMapSourcePreviewSelectionV1 {
        &self.selection
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
            "selection": {
                "kind": "index",
                "evidence": {
                    "kind": "file",
                    "moduleId": "66",
                    "ordinal": 1,
                    "evidenceId": "77",
                    "path": "src/lib.rs"
                }
            }
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
