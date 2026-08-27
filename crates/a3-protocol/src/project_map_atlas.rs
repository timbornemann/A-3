use crate::{ProjectMapMappingStatusV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Core-issued entity selection accepted by progressive Atlas reads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum ProjectMapEntitySelectionV1 {
    /// One current primary module.
    Module {
        /// Stable module identity.
        module_id: String,
    },
    /// One current file at its deterministic module rank.
    File {
        /// Stable owning module identity.
        module_id: String,
        /// One-based Core-issued rank, never a caller-controlled limit.
        ordinal: u32,
        /// Exact current file-revision Evidence identity.
        evidence_id: String,
    },
    /// One current content-bound structural symbol.
    Symbol {
        /// Stable owning module identity.
        module_id: String,
        /// Content-bound symbol identity.
        symbol_id: String,
        /// Exact current symbol Evidence identity.
        evidence_id: String,
    },
}

/// Exact current index Evidence selection emitted by an Atlas response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum ProjectMapIndexEvidenceSelectionV1 {
    /// Current file revision.
    File {
        /// Stable owning module identity.
        module_id: String,
        /// One-based deterministic file rank.
        ordinal: u32,
        /// Exact file-revision Evidence identity.
        evidence_id: String,
    },
    /// Current structural symbol.
    Symbol {
        /// Stable owning module identity.
        module_id: String,
        /// Content-bound symbol identity.
        symbol_id: String,
        /// Exact symbol Evidence identity.
        evidence_id: String,
    },
    /// Current resolved graph relation.
    Relation {
        /// Primary module containing the evidence source.
        module_id: String,
        /// One-based canonical edge position.
        edge_sequence: String,
        /// Exact graph-edge Evidence identity.
        evidence_id: String,
    },
    /// Current unresolved relation candidate.
    UnresolvedRelation {
        /// Primary module containing the evidence source.
        module_id: String,
        /// One-based canonical candidate position.
        candidate_sequence: String,
        /// Exact evidence-file revision identity.
        evidence_id: String,
    },
}

/// Request for a project overview or one semantic-zoom level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapAtlasSceneRequestV1 {
    protocol_version: ProtocolVersion,
    selection: Option<ProjectMapEntitySelectionV1>,
}

impl QueryProjectMapAtlasSceneRequestV1 {
    /// Returns the requested protocol revision.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns a previously emitted selection, never a path or limit.
    #[must_use]
    pub const fn selection(&self) -> Option<&ProjectMapEntitySelectionV1> {
        self.selection.as_ref()
    }
}

/// Request for progressive metadata and direct relationships of one selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapEntityContextRequestV1 {
    protocol_version: ProtocolVersion,
    selection: ProjectMapEntitySelectionV1,
}

impl QueryProjectMapEntityContextRequestV1 {
    /// Returns the requested protocol revision.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the Core-issued entity selection.
    #[must_use]
    pub const fn selection(&self) -> &ProjectMapEntitySelectionV1 {
        &self.selection
    }
}

/// Closed inventory projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapInventoryViewV1 {
    /// Files uniquely owned by a module.
    Files,
    /// Structural symbols in a module or file.
    Symbols,
    /// Direct members of a type or symbol.
    Members,
}

/// Request for exactly one fixed fifty-entry inventory page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapInventoryPageRequestV1 {
    protocol_version: ProtocolVersion,
    selection: ProjectMapEntitySelectionV1,
    view: ProjectMapInventoryViewV1,
    cursor: Option<String>,
}

impl QueryProjectMapInventoryPageRequestV1 {
    /// Returns the requested protocol revision.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the Core-issued inventory scope.
    #[must_use]
    pub const fn selection(&self) -> &ProjectMapEntitySelectionV1 {
        &self.selection
    }
    /// Returns the closed inventory projection.
    #[must_use]
    pub const fn view(&self) -> ProjectMapInventoryViewV1 {
        self.view
    }
    /// Returns the optional opaque publication- and scope-bound cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Closed focused-flow preset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapFlowPresetV1 {
    /// Incoming calls, at most two hops.
    Callers,
    /// Outgoing calls, at most two hops.
    Callees,
    /// Direct test/subject relations in both relevant directions.
    Tests,
    /// Direct reads and writes.
    DataAccess,
}

/// Request for one fixed focused flow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapFlowSceneRequestV1 {
    protocol_version: ProtocolVersion,
    selection: ProjectMapEntitySelectionV1,
    preset: ProjectMapFlowPresetV1,
}

impl QueryProjectMapFlowSceneRequestV1 {
    /// Returns the requested protocol revision.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the Core-issued root selection.
    #[must_use]
    pub const fn selection(&self) -> &ProjectMapEntitySelectionV1 {
        &self.selection
    }
    /// Returns the closed flow preset.
    #[must_use]
    pub const fn preset(&self) -> ProjectMapFlowPresetV1 {
        self.preset
    }
}

/// Progressive semantic zoom level.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapAtlasLevelV1 {
    /// Primary module overview.
    Project,
    /// Ranked files within one module.
    Module,
    /// Architecture symbols within one file.
    File,
    /// One symbol and its direct members or neighbors.
    Symbol,
}

/// Stable category of one Atlas region.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapAtlasNodeKindV1 {
    /// Manifest-backed module boundary.
    ManifestModule,
    /// Deterministic path module boundary.
    PathModule,
    /// Current source file.
    File,
    /// Module or namespace declaration.
    Namespace,
    /// Class, struct, interface, trait, enum, implementation, or alias.
    Type,
    /// Function or method.
    Callable,
    /// Field, constant, variable, variant, static, or parameter.
    Member,
    /// External or unresolved target.
    Boundary,
}

/// All thirteen language-neutral deterministic relation kinds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapRelationKindV1 {
    /// Lexical containment.
    Contains,
    /// Definition relationship.
    Defines,
    /// Import relationship.
    Imports,
    /// Export relationship.
    Exports,
    /// Static call candidate.
    Calls,
    /// Implementation relationship.
    Implements,
    /// Extension or inheritance.
    Extends,
    /// Read access candidate.
    Reads,
    /// Write access candidate.
    Writes,
    /// Configuration relationship.
    Configures,
    /// Test-to-subject relationship.
    Tests,
    /// Build relationship.
    Builds,
    /// Documentation relationship.
    Documents,
}

/// Deterministic relation observer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapRelationProviderV1 {
    /// Direct Tree-sitter observation.
    TreeSitter,
    /// Deterministic manifest interpretation.
    Manifest,
    /// Bounded language-specific syntax heuristic.
    LanguageHeuristic,
}

/// Closed reason for a dashed boundary route.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapAtlasUncertaintyV1 {
    /// Repository-external target.
    External,
    /// No exact local match exists.
    NoDeterministicMatch,
    /// More than one target matched.
    AmbiguousMatch,
    /// Runtime semantics are required.
    DynamicReference,
    /// Adapter-emitted file target is absent.
    MissingFile,
}

/// One bounded progressive Atlas node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapAtlasNodeV1 {
    node_id: String,
    parent_node_id: Option<String>,
    selection: Option<ProjectMapEntitySelectionV1>,
    kind: ProjectMapAtlasNodeKindV1,
    display_name: String,
    detail: Option<String>,
    rank: u16,
    volume: String,
    file_count: String,
    symbol_count: String,
    member_count: String,
    mapping_status: Option<ProjectMapMappingStatusV1>,
    purpose: Option<String>,
    current_risk_count: String,
    evidence_id: Option<String>,
    claim_badge_count: u16,
    dimmed: bool,
}

impl ProjectMapAtlasNodeV1 {
    /// Creates one already validated application node projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        node_id: String,
        parent_node_id: Option<String>,
        selection: Option<ProjectMapEntitySelectionV1>,
        kind: ProjectMapAtlasNodeKindV1,
        display_name: String,
        detail: Option<String>,
        rank: u16,
        volume: String,
        file_count: String,
        symbol_count: String,
        member_count: String,
        mapping_status: Option<ProjectMapMappingStatusV1>,
        purpose: Option<String>,
        current_risk_count: String,
        evidence_id: Option<String>,
        claim_badge_count: u16,
        dimmed: bool,
    ) -> Self {
        Self {
            node_id,
            parent_node_id,
            selection,
            kind,
            display_name,
            detail,
            rank,
            volume,
            file_count,
            symbol_count,
            member_count,
            mapping_status,
            purpose,
            current_risk_count,
            evidence_id,
            claim_badge_count,
            dimmed,
        }
    }
}

/// One grouped deterministic route.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapAtlasRelationV1 {
    source_node_id: String,
    target_node_id: String,
    relation: ProjectMapRelationKindV1,
    evidence_count: String,
    confidence_basis_points: u16,
    provider: ProjectMapRelationProviderV1,
    evidence: Option<ProjectMapIndexEvidenceSelectionV1>,
    claim_badge_count: u16,
    uncertainty: Option<ProjectMapAtlasUncertaintyV1>,
}

impl ProjectMapAtlasRelationV1 {
    /// Creates one already validated application relation projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source_node_id: String,
        target_node_id: String,
        relation: ProjectMapRelationKindV1,
        evidence_count: String,
        confidence_basis_points: u16,
        provider: ProjectMapRelationProviderV1,
        evidence: Option<ProjectMapIndexEvidenceSelectionV1>,
        claim_badge_count: u16,
        uncertainty: Option<ProjectMapAtlasUncertaintyV1>,
    ) -> Self {
        Self {
            source_node_id,
            target_node_id,
            relation,
            evidence_count,
            confidence_basis_points,
            provider,
            evidence,
            claim_badge_count,
            uncertainty,
        }
    }
}

/// One semantic-zoom breadcrumb step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapAtlasBreadcrumbV1 {
    label: String,
    selection: Option<ProjectMapEntitySelectionV1>,
}

impl ProjectMapAtlasBreadcrumbV1 {
    /// Creates one application-validated breadcrumb.
    #[must_use]
    pub const fn new(label: String, selection: Option<ProjectMapEntitySelectionV1>) -> Self {
        Self { label, selection }
    }
}

/// Complete bounded progressive scene.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapAtlasSceneV1 {
    index_run_id: String,
    snapshot_id: String,
    policy_version: u16,
    level: ProjectMapAtlasLevelV1,
    selection: Option<ProjectMapEntitySelectionV1>,
    breadcrumb: Vec<ProjectMapAtlasBreadcrumbV1>,
    nodes: Vec<ProjectMapAtlasNodeV1>,
    node_count: String,
    relations: Vec<ProjectMapAtlasRelationV1>,
    relation_count: String,
    boundary_count: String,
    unresolved_count: String,
    inspected_edge_count: String,
    nodes_truncated: bool,
    relations_truncated: bool,
    boundaries_truncated: bool,
    source_edges_truncated: bool,
}

impl ProjectMapAtlasSceneV1 {
    /// Creates one already validated current scene.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        level: ProjectMapAtlasLevelV1,
        selection: Option<ProjectMapEntitySelectionV1>,
        breadcrumb: Vec<ProjectMapAtlasBreadcrumbV1>,
        nodes: Vec<ProjectMapAtlasNodeV1>,
        node_count: String,
        relations: Vec<ProjectMapAtlasRelationV1>,
        relation_count: String,
        boundary_count: String,
        unresolved_count: String,
        inspected_edge_count: String,
        nodes_truncated: bool,
        relations_truncated: bool,
        boundaries_truncated: bool,
        source_edges_truncated: bool,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            policy_version: 1,
            level,
            selection,
            breadcrumb,
            nodes,
            node_count,
            relations,
            relation_count,
            boundary_count,
            unresolved_count,
            inspected_edge_count,
            nodes_truncated,
            relations_truncated,
            boundaries_truncated,
            source_edges_truncated,
        }
    }
}

/// One non-zero incoming/outgoing relationship aggregate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapRelationCountV1 {
    relation: ProjectMapRelationKindV1,
    incoming: String,
    outgoing: String,
}
impl ProjectMapRelationCountV1 {
    /// Creates one validated count projection.
    #[must_use]
    pub const fn new(
        relation: ProjectMapRelationKindV1,
        incoming: String,
        outgoing: String,
    ) -> Self {
        Self {
            relation,
            incoming,
            outgoing,
        }
    }
}

/// Exact current verified claim reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapClaimReferenceV1 {
    card_id: String,
    claim_id: String,
    confidence_basis_points: u16,
}
impl ProjectMapClaimReferenceV1 {
    /// Creates one already verified claim reference.
    #[must_use]
    pub const fn new(card_id: String, claim_id: String, confidence_basis_points: u16) -> Self {
        Self {
            card_id,
            claim_id,
            confidence_basis_points,
        }
    }
}

/// Progressive Inspector context for one entity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapEntityContextV1 {
    index_run_id: String,
    snapshot_id: String,
    entity: ProjectMapAtlasNodeV1,
    relation_counts: Vec<ProjectMapRelationCountV1>,
    related_nodes: Vec<ProjectMapAtlasNodeV1>,
    architecture_relations: Vec<ProjectMapAtlasRelationV1>,
    architecture_relation_count: String,
    boundary_nodes: Vec<ProjectMapAtlasNodeV1>,
    boundary_relations: Vec<ProjectMapAtlasRelationV1>,
    boundary_count: String,
    document_relation_count: String,
    claims: Vec<ProjectMapClaimReferenceV1>,
    source_edges_truncated: bool,
}
impl ProjectMapEntityContextV1 {
    /// Creates one already validated current context.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        entity: ProjectMapAtlasNodeV1,
        relation_counts: Vec<ProjectMapRelationCountV1>,
        related_nodes: Vec<ProjectMapAtlasNodeV1>,
        architecture_relations: Vec<ProjectMapAtlasRelationV1>,
        architecture_relation_count: String,
        boundary_nodes: Vec<ProjectMapAtlasNodeV1>,
        boundary_relations: Vec<ProjectMapAtlasRelationV1>,
        boundary_count: String,
        document_relation_count: String,
        claims: Vec<ProjectMapClaimReferenceV1>,
        source_edges_truncated: bool,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            entity,
            relation_counts,
            related_nodes,
            architecture_relations,
            architecture_relation_count,
            boundary_nodes,
            boundary_relations,
            boundary_count,
            document_relation_count,
            claims,
            source_edges_truncated,
        }
    }
}

/// One fixed fifty-entry inventory page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapInventoryPageV1 {
    index_run_id: String,
    snapshot_id: String,
    selection: ProjectMapEntitySelectionV1,
    view: ProjectMapInventoryViewV1,
    page_number: u32,
    page_size: u16,
    total_count: String,
    items: Vec<ProjectMapAtlasNodeV1>,
    previous_cursor: Option<String>,
    next_cursor: Option<String>,
}
impl ProjectMapInventoryPageV1 {
    /// Creates one already validated current page.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        selection: ProjectMapEntitySelectionV1,
        view: ProjectMapInventoryViewV1,
        page_number: u32,
        total_count: String,
        items: Vec<ProjectMapAtlasNodeV1>,
        previous_cursor: Option<String>,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            selection,
            view,
            page_number,
            page_size: 50,
            total_count,
            items,
            previous_cursor,
            next_cursor,
        }
    }
}

/// One current relation step on a shortest flow path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapFlowStepV1 {
    source_node_id: String,
    target_node_id: String,
    relation: ProjectMapRelationKindV1,
    evidence: ProjectMapIndexEvidenceSelectionV1,
}
impl ProjectMapFlowStepV1 {
    /// Creates one already validated flow step.
    #[must_use]
    pub const fn new(
        source_node_id: String,
        target_node_id: String,
        relation: ProjectMapRelationKindV1,
        evidence: ProjectMapIndexEvidenceSelectionV1,
    ) -> Self {
        Self {
            source_node_id,
            target_node_id,
            relation,
            evidence,
        }
    }
}

/// One flow target with its complete shortest evidence path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapFlowTargetV1 {
    node_id: String,
    depth: u8,
    path: Vec<ProjectMapFlowStepV1>,
}
impl ProjectMapFlowTargetV1 {
    /// Creates one already validated target path.
    #[must_use]
    pub const fn new(node_id: String, depth: u8, path: Vec<ProjectMapFlowStepV1>) -> Self {
        Self {
            node_id,
            depth,
            path,
        }
    }
}

/// Complete bounded focused flow scene.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapFlowSceneV1 {
    index_run_id: String,
    snapshot_id: String,
    preset: ProjectMapFlowPresetV1,
    root: ProjectMapAtlasNodeV1,
    nodes: Vec<ProjectMapAtlasNodeV1>,
    targets: Vec<ProjectMapFlowTargetV1>,
    target_count: String,
    inspected_edge_count: String,
    targets_truncated: bool,
    source_edges_truncated: bool,
}
impl ProjectMapFlowSceneV1 {
    /// Creates one already validated focused flow.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        preset: ProjectMapFlowPresetV1,
        root: ProjectMapAtlasNodeV1,
        nodes: Vec<ProjectMapAtlasNodeV1>,
        targets: Vec<ProjectMapFlowTargetV1>,
        target_count: String,
        inspected_edge_count: String,
        targets_truncated: bool,
        source_edges_truncated: bool,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            preset,
            root,
            nodes,
            targets,
            target_count,
            inspected_edge_count,
            targets_truncated,
            source_edges_truncated,
        }
    }
}

macro_rules! atlas_response {
    ($response:ident, $result:ident, $payload:ty, $field:ident) => {
        /// Strict versioned response for one progressive Atlas projection.
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $response {
            protocol_version: ProtocolVersion,
            result: $result,
        }
        impl $response {
            /// No project is active.
            #[must_use]
            pub const fn no_project() -> Self {
                Self::with_result($result::NoProject)
            }
            /// No index has crossed the publication boundary.
            #[must_use]
            pub const fn no_published_index() -> Self {
                Self::with_result($result::NoPublishedIndex)
            }
            /// The publication predates deterministic modules.
            #[must_use]
            pub const fn projection_unavailable() -> Self {
                Self::with_result($result::ProjectionUnavailable)
            }
            /// The Core-issued selection or cursor no longer matches.
            #[must_use]
            pub const fn selection_changed() -> Self {
                Self::with_result($result::SelectionChanged)
            }
            /// One current bounded payload is available.
            #[must_use]
            pub fn available($field: $payload) -> Self {
                Self::with_result($result::Available {
                    $field: Box::new($field),
                })
            }
            const fn with_result(result: $result) -> Self {
                Self {
                    protocol_version: ProtocolVersion::CURRENT,
                    result,
                }
            }
        }
        /// Mutually exclusive availability state for one progressive Atlas projection.
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
        pub enum $result {
            /// No project is active.
            NoProject,
            /// No index has crossed the atomic publication boundary.
            NoPublishedIndex,
            /// The current publication predates deterministic modules.
            ProjectionUnavailable,
            /// The selection or cursor no longer matches the current publication.
            SelectionChanged,
            /// One current bounded projection is available.
            Available {
                /// Application-validated bounded projection.
                $field: Box<$payload>,
            },
        }
    };
}

atlas_response!(
    ProjectMapAtlasSceneResponseV1,
    ProjectMapAtlasSceneResultV1,
    ProjectMapAtlasSceneV1,
    scene
);
atlas_response!(
    ProjectMapEntityContextResponseV1,
    ProjectMapEntityContextResultV1,
    ProjectMapEntityContextV1,
    context
);
atlas_response!(
    ProjectMapInventoryPageResponseV1,
    ProjectMapInventoryPageResultV1,
    ProjectMapInventoryPageV1,
    page
);
atlas_response!(
    ProjectMapFlowSceneResponseV1,
    ProjectMapFlowSceneResultV1,
    ProjectMapFlowSceneV1,
    flow
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_request_rejects_paths_limits_and_unknown_fields() {
        let json = r#"{"protocolVersion":1,"selection":null,"limit":500}"#;
        assert!(serde_json::from_str::<QueryProjectMapAtlasSceneRequestV1>(json).is_err());
        let json = r#"{"protocolVersion":1,"selection":{"kind":"file","moduleId":"00","ordinal":1,"evidenceId":"00","path":"src/main.rs"}}"#;
        assert!(serde_json::from_str::<QueryProjectMapAtlasSceneRequestV1>(json).is_err());
    }

    #[test]
    fn atlas_selections_use_exact_camel_case_wire_fields() -> Result<(), Box<dyn std::error::Error>>
    {
        let request = r#"{"protocolVersion":1,"selection":{"kind":"file","moduleId":"module","ordinal":7,"evidenceId":"evidence"}}"#;
        let decoded = serde_json::from_str::<QueryProjectMapAtlasSceneRequestV1>(request)?;
        assert_eq!(
            serde_json::to_value(decoded)?,
            serde_json::json!({
                "protocolVersion": 1,
                "selection": {
                    "kind": "file",
                    "moduleId": "module",
                    "ordinal": 7,
                    "evidenceId": "evidence"
                }
            })
        );

        let relation = ProjectMapIndexEvidenceSelectionV1::Relation {
            module_id: "module".to_owned(),
            edge_sequence: "11".to_owned(),
            evidence_id: "evidence".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(relation)?,
            serde_json::json!({
                "kind": "relation",
                "moduleId": "module",
                "edgeSequence": "11",
                "evidenceId": "evidence"
            })
        );
        Ok(())
    }
}
