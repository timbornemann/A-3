use crate::{ModuleDependencyRelationV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict map-scene request without paths, cursors, or caller-controlled limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapSceneRequestV1 {
    protocol_version: ProtocolVersion,
    focus_module_id: Option<String>,
}

impl QueryProjectMapSceneRequestV1 {
    /// Creates an overview or current-module focus request.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, focus_module_id: Option<String>) -> Self {
        Self {
            protocol_version,
            focus_module_id,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the optional untrusted stable module identity.
    #[must_use]
    pub fn focus_module_id(&self) -> Option<&str> {
        self.focus_module_id.as_deref()
    }
}

/// Versioned deterministic atlas scene response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSceneResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectMapSceneResultV1,
}

impl ProjectMapSceneResponseV1 {
    /// Creates the response used when no project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ProjectMapSceneResultV1::NoProject)
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ProjectMapSceneResultV1::NoPublishedIndex)
    }

    /// Creates the response for publications without deterministic modules.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ProjectMapSceneResultV1::ProjectionUnavailable)
    }

    /// Creates the response for a focus module no longer in the current projection.
    #[must_use]
    pub const fn focus_unavailable() -> Self {
        Self::with_result(ProjectMapSceneResultV1::FocusUnavailable)
    }

    /// Creates one bounded current scene.
    #[must_use]
    pub fn available(scene: ProjectMapSceneV1) -> Self {
        Self::with_result(ProjectMapSceneResultV1::Available {
            scene: Box::new(scene),
        })
    }

    const fn with_result(result: ProjectMapSceneResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive scene result.
    #[must_use]
    pub const fn result(&self) -> &ProjectMapSceneResultV1 {
        &self.result
    }
}

/// Availability state for a project-map scene.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectMapSceneResultV1 {
    /// No project is active.
    NoProject,
    /// No index is published.
    NoPublishedIndex,
    /// The publication predates deterministic modules.
    ProjectionUnavailable,
    /// The requested focus is absent or supplementary.
    FocusUnavailable,
    /// One current bounded scene is available.
    Available {
        /// Complete scene bound to one run, snapshot, and policy.
        scene: Box<ProjectMapSceneV1>,
    },
}

/// Deterministic scene-selection policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScenePolicyVersionV1 {
    /// Manifest-first overview and evidence-ranked direct focus.
    V1,
}

/// Mapping lifecycle shown independently through text, shape, and color.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapMappingStatusV1 {
    /// Card is current.
    Current,
    /// Direct evidence invalidated the Card.
    Stale,
    /// A dependency change requires review.
    NeedsReview,
    /// No verified Card exists.
    Unmapped,
}

/// Primary deterministic module boundary kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapSceneModuleKindV1 {
    /// Package or workspace manifest boundary.
    ManifestBoundary,
    /// Deterministic structural path boundary.
    PathBoundary,
}

/// Current Card anchors used only for subsequent typed Inspector reads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSceneCardBindingV1 {
    card_id: String,
    source_index_run_id: String,
    source_snapshot_id: String,
}

impl ProjectMapSceneCardBindingV1 {
    /// Creates already validated Card anchors.
    #[must_use]
    pub const fn new(
        card_id: String,
        source_index_run_id: String,
        source_snapshot_id: String,
    ) -> Self {
        Self {
            card_id,
            source_index_run_id,
            source_snapshot_id,
        }
    }
}

/// One visible atlas region with bounded counts and progressive-load hooks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSceneModuleV1 {
    module_id: String,
    parent_module_id: Option<String>,
    kind: ProjectMapSceneModuleKindV1,
    display_name: String,
    rank: u16,
    manifest_count: String,
    file_count: String,
    symbol_count: String,
    central_symbol_count: String,
    entrypoint_count: String,
    test_count: String,
    mapping_status: ProjectMapMappingStatusV1,
    card_coverage_basis_points: Option<u16>,
    card_binding: Option<ProjectMapSceneCardBindingV1>,
    representative_evidence_id: Option<String>,
}

impl ProjectMapSceneModuleV1 {
    /// Creates one application-validated module projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        module_id: String,
        parent_module_id: Option<String>,
        kind: ProjectMapSceneModuleKindV1,
        display_name: String,
        rank: u16,
        manifest_count: String,
        file_count: String,
        symbol_count: String,
        central_symbol_count: String,
        entrypoint_count: String,
        test_count: String,
        mapping_status: ProjectMapMappingStatusV1,
        card_coverage_basis_points: Option<u16>,
        card_binding: Option<ProjectMapSceneCardBindingV1>,
        representative_evidence_id: Option<String>,
    ) -> Self {
        Self {
            module_id,
            parent_module_id,
            kind,
            display_name,
            rank,
            manifest_count,
            file_count,
            symbol_count,
            central_symbol_count,
            entrypoint_count,
            test_count,
            mapping_status,
            card_coverage_basis_points,
            card_binding,
            representative_evidence_id,
        }
    }
}

/// One non-authoritative SVG route between two visible regions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSceneRelationV1 {
    source_module_id: String,
    target_module_id: String,
    relation: ModuleDependencyRelationV1,
    observed_evidence_count: String,
    evidence_id: Option<String>,
}

impl ProjectMapSceneRelationV1 {
    /// Creates one application-validated grouped route.
    #[must_use]
    pub const fn new(
        source_module_id: String,
        target_module_id: String,
        relation: ModuleDependencyRelationV1,
        observed_evidence_count: String,
        evidence_id: Option<String>,
    ) -> Self {
        Self {
            source_module_id,
            target_module_id,
            relation,
            observed_evidence_count,
            evidence_id,
        }
    }
}

/// Complete bounded atlas scene with explicit omission accounting.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSceneV1 {
    index_run_id: String,
    snapshot_id: String,
    policy_version: ScenePolicyVersionV1,
    focus_module_id: Option<String>,
    primary_module_count: String,
    modules: Vec<ProjectMapSceneModuleV1>,
    modules_truncated: bool,
    observed_relation_group_count: String,
    relations: Vec<ProjectMapSceneRelationV1>,
    relations_truncated: bool,
    inspected_edge_count: String,
    unmapped_edge_count: String,
    source_edges_truncated: bool,
}

impl ProjectMapSceneV1 {
    /// Creates one application-validated scene projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        policy_version: ScenePolicyVersionV1,
        focus_module_id: Option<String>,
        primary_module_count: String,
        modules: Vec<ProjectMapSceneModuleV1>,
        modules_truncated: bool,
        observed_relation_group_count: String,
        relations: Vec<ProjectMapSceneRelationV1>,
        relations_truncated: bool,
        inspected_edge_count: String,
        unmapped_edge_count: String,
        source_edges_truncated: bool,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            policy_version,
            focus_module_id,
            primary_module_count,
            modules,
            modules_truncated,
            observed_relation_group_count,
            relations,
            relations_truncated,
            inspected_edge_count,
            unmapped_edge_count,
            source_edges_truncated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejects_rendering_limits_paths_and_unknown_fields() {
        let json = r#"{"protocolVersion":1,"focusModuleId":null,"limit":500}"#;
        assert!(serde_json::from_str::<QueryProjectMapSceneRequestV1>(json).is_err());
    }
}
