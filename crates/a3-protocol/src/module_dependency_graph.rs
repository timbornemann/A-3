use crate::{ModuleTreeEntryKindV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict bounded request for the active project's direct module neighborhood.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleDependencyGraphRequestV1 {
    protocol_version: ProtocolVersion,
    center_module_id: String,
    node_limit: u16,
}

impl QueryModuleDependencyGraphRequestV1 {
    /// Creates an untrusted request validated by the Rust command boundary.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        center_module_id: String,
        node_limit: u16,
    ) -> Self {
        Self {
            protocol_version,
            center_module_id,
            node_limit,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted stable module token.
    #[must_use]
    pub fn center_module_id(&self) -> &str {
        &self.center_module_id
    }

    /// Returns the untrusted total node boundary.
    #[must_use]
    pub const fn node_limit(&self) -> u16 {
        self.node_limit
    }
}

/// Versioned direct module-neighborhood result selected from Core-owned project state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyGraphResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleDependencyGraphResultV1,
}

impl ModuleDependencyGraphResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleDependencyGraphResultV1::NoProject,
        }
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleDependencyGraphResultV1::NoPublishedIndex,
        }
    }

    /// Creates the response used when a historical publication predates module schema V8.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleDependencyGraphResultV1::ProjectionUnavailable,
        }
    }

    /// Creates the response used when the selected module is no longer current or primary.
    #[must_use]
    pub const fn center_unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleDependencyGraphResultV1::CenterUnavailable,
        }
    }

    /// Creates an available graph from application-validated bounded values.
    #[must_use]
    pub fn available(graph: ModuleDependencyGraphV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleDependencyGraphResultV1::Available {
                graph: Box::new(graph),
            },
        }
    }

    /// Returns the mutually exclusive project/publication result.
    #[must_use]
    pub const fn result(&self) -> &ModuleDependencyGraphResultV1 {
        &self.result
    }
}

/// Whether a current deterministic module neighborhood exists.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleDependencyGraphResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// The latest historical publication has no deterministic module marker.
    ProjectionUnavailable,
    /// The selected stable ID is absent or names a supplementary graph community.
    CenterUnavailable,
    /// One bounded evidence-bearing direct neighborhood is available.
    Available {
        /// Current atomic graph containing at most one hundred primary modules.
        graph: Box<ModuleDependencyGraphV1>,
    },
}

/// Bounded direct module neighborhood and all visible incompleteness signals.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyGraphV1 {
    index_run_id: String,
    snapshot_id: String,
    center_module_id: String,
    nodes: Vec<ModuleDependencyNodeV1>,
    observed_neighbor_count: String,
    nodes_truncated: bool,
    edges: Vec<ModuleDependencyEdgeV1>,
    observed_edge_group_count: String,
    edges_truncated: bool,
    inspected_edge_count: String,
    source_edges_truncated: bool,
    unmapped_edge_count: String,
}

impl ModuleDependencyGraphV1 {
    /// Creates one strict graph from application-validated data.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        center_module_id: String,
        nodes: Vec<ModuleDependencyNodeV1>,
        observed_neighbor_count: String,
        nodes_truncated: bool,
        edges: Vec<ModuleDependencyEdgeV1>,
        observed_edge_group_count: String,
        edges_truncated: bool,
        inspected_edge_count: String,
        source_edges_truncated: bool,
        unmapped_edge_count: String,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            center_module_id,
            nodes,
            observed_neighbor_count,
            nodes_truncated,
            edges,
            observed_edge_group_count,
            edges_truncated,
            inspected_edge_count,
            source_edges_truncated,
            unmapped_edge_count,
        }
    }
}

/// One current file revision supporting a module node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyNodeEvidenceV1 {
    evidence_id: String,
    path_hex: String,
    content_hash: String,
}

impl ModuleDependencyNodeEvidenceV1 {
    /// Creates one exact current revision and its stable inspector identity.
    #[must_use]
    pub const fn new(evidence_id: String, path_hex: String, content_hash: String) -> Self {
        Self {
            evidence_id,
            path_hex,
            content_hash,
        }
    }
}

/// One primary module in the visible bounded graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyNodeV1 {
    module_id: String,
    kind: ModuleTreeEntryKindV1,
    root_path_hex: Option<String>,
    name: String,
    name_truncated: bool,
    representative_evidence: Option<ModuleDependencyNodeEvidenceV1>,
}

impl ModuleDependencyNodeV1 {
    /// Creates one WebView-safe deterministic primary-module node.
    #[must_use]
    pub const fn new(
        module_id: String,
        kind: ModuleTreeEntryKindV1,
        root_path_hex: Option<String>,
        name: String,
        name_truncated: bool,
        representative_evidence: Option<ModuleDependencyNodeEvidenceV1>,
    ) -> Self {
        Self {
            module_id,
            kind,
            root_path_hex,
            name,
            name_truncated,
            representative_evidence,
        }
    }
}

/// Graph endpoint retained as part of an exact representative edge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum ModuleDependencyEndpointV1 {
    /// Current repository-relative file endpoint as canonical path bytes.
    File {
        /// Lowercase hexadecimal representation of the relative path.
        #[serde(rename = "pathHex")]
        path_hex: String,
    },
    /// Current structural symbol endpoint.
    Symbol {
        /// Canonical 256-bit symbol identity.
        #[serde(rename = "symbolId")]
        symbol_id: String,
    },
}

/// Language-neutral relation visible in a module dependency graph.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleDependencyRelationV1 {
    /// Import relationship.
    Imports,
    /// Export relationship.
    Exports,
    /// Syntactically visible call relationship.
    Calls,
    /// Trait or interface implementation.
    Implements,
    /// Type extension or inheritance.
    Extends,
    /// Read relationship.
    Reads,
    /// Write relationship.
    Writes,
    /// Configuration relationship.
    Configures,
    /// Test relationship.
    Tests,
    /// Build relationship.
    Builds,
    /// Documentation relationship.
    Documents,
}

/// Deterministic provider that observed one representative relation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleDependencyProviderV1 {
    /// Direct Tree-sitter syntax observation.
    TreeSitter,
    /// Deterministic manifest interpretation.
    Manifest,
    /// Bounded language-specific syntax heuristic.
    LanguageHeuristic,
}

/// Deterministic linker basis used for one representative relation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleDependencyResolutionV1 {
    /// Adapter-local symbol identity.
    AdapterLocalSymbol,
    /// Adapter-validated repository file.
    AdapterFile,
    /// Exact language-aware module reference.
    ExactModuleReference,
    /// Unique file-local simple name.
    UniqueFileLocalName,
    /// Unique qualified name in the snapshot.
    UniqueQualifiedName,
}

/// Zero-based point within representative source evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencySourcePositionV1 {
    row: u32,
    column: u32,
}

impl ModuleDependencySourcePositionV1 {
    /// Creates an already validated zero-based point.
    #[must_use]
    pub const fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }
}

/// Half-open byte and point range of representative relation evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencySourceRangeV1 {
    start_byte: u32,
    end_byte: u32,
    start: ModuleDependencySourcePositionV1,
    end: ModuleDependencySourcePositionV1,
}

impl ModuleDependencySourceRangeV1 {
    /// Creates one range already validated by the domain graph.
    #[must_use]
    pub const fn new(
        start_byte: u32,
        end_byte: u32,
        start: ModuleDependencySourcePositionV1,
        end: ModuleDependencySourcePositionV1,
    ) -> Self {
        Self {
            start_byte,
            end_byte,
            start,
            end,
        }
    }
}

/// Full exact representative graph edge supporting an aggregated module relation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyEdgeEvidenceV1 {
    evidence_id: String,
    source: ModuleDependencyEndpointV1,
    target: ModuleDependencyEndpointV1,
    path_hex: String,
    content_hash: String,
    range: ModuleDependencySourceRangeV1,
    provider: ModuleDependencyProviderV1,
    confidence_basis_points: u16,
    resolution: ModuleDependencyResolutionV1,
}

impl ModuleDependencyEdgeEvidenceV1 {
    /// Creates a current exact edge projection with stable inspector identity.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        evidence_id: String,
        source: ModuleDependencyEndpointV1,
        target: ModuleDependencyEndpointV1,
        path_hex: String,
        content_hash: String,
        range: ModuleDependencySourceRangeV1,
        provider: ModuleDependencyProviderV1,
        confidence_basis_points: u16,
        resolution: ModuleDependencyResolutionV1,
    ) -> Self {
        Self {
            evidence_id,
            source,
            target,
            path_hex,
            content_hash,
            range,
            provider,
            confidence_basis_points,
            resolution,
        }
    }
}

/// One relation-specific observed cross-module dependency group.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleDependencyEdgeV1 {
    source_module_id: String,
    target_module_id: String,
    relation: ModuleDependencyRelationV1,
    observed_evidence_count: String,
    representative_evidence: ModuleDependencyEdgeEvidenceV1,
}

impl ModuleDependencyEdgeV1 {
    /// Creates one group from exact application-validated endpoints and evidence.
    #[must_use]
    pub const fn new(
        source_module_id: String,
        target_module_id: String,
        relation: ModuleDependencyRelationV1,
        observed_evidence_count: String,
        representative_evidence: ModuleDependencyEdgeEvidenceV1,
    ) -> Self {
        Self {
            source_module_id,
            target_module_id,
            relation,
            observed_evidence_count,
            representative_evidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModuleDependencyEdgeEvidenceV1, ModuleDependencyEdgeV1, ModuleDependencyEndpointV1,
        ModuleDependencyGraphResponseV1, ModuleDependencyGraphV1, ModuleDependencyNodeV1,
        ModuleDependencyProviderV1, ModuleDependencyRelationV1, ModuleDependencyResolutionV1,
        ModuleDependencySourcePositionV1, ModuleDependencySourceRangeV1,
        QueryModuleDependencyGraphRequestV1,
    };
    use crate::{ModuleTreeEntryKindV1, ProtocolVersion};

    #[test]
    fn available_graph_serializes_bounds_and_exact_edge_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let position = ModuleDependencySourcePositionV1::new(0, 0);
        let response = ModuleDependencyGraphResponseV1::available(ModuleDependencyGraphV1::new(
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32),
            vec![ModuleDependencyNodeV1::new(
                "33".repeat(32),
                ModuleTreeEntryKindV1::PathBoundary,
                Some("737263".to_owned()),
                "src".to_owned(),
                false,
                None,
            )],
            "0".to_owned(),
            false,
            vec![ModuleDependencyEdgeV1::new(
                "33".repeat(32),
                "44".repeat(32),
                ModuleDependencyRelationV1::Imports,
                "2".to_owned(),
                ModuleDependencyEdgeEvidenceV1::new(
                    "55".repeat(32),
                    ModuleDependencyEndpointV1::Symbol {
                        symbol_id: "66".repeat(32),
                    },
                    ModuleDependencyEndpointV1::File {
                        path_hex: "746f6f6c732f6c69622e7273".to_owned(),
                    },
                    "7372632f6c69622e7273".to_owned(),
                    "77".repeat(32),
                    ModuleDependencySourceRangeV1::new(0, 1, position, position),
                    ModuleDependencyProviderV1::TreeSitter,
                    10_000,
                    ModuleDependencyResolutionV1::AdapterFile,
                ),
            )],
            "1".to_owned(),
            false,
            "2".to_owned(),
            false,
            "0".to_owned(),
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(value["result"]["graph"]["inspectedEdgeCount"], "2");
        assert_eq!(
            value["result"]["graph"]["edges"][0]["representativeEvidence"]["source"]["kind"],
            "symbol"
        );
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields_and_retains_values_for_core_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let request =
            serde_json::from_value::<QueryModuleDependencyGraphRequestV1>(serde_json::json!({
                "protocolVersion": 1,
                "centerModuleId": "11",
                "nodeLimit": 100
            }))?;
        assert_eq!(request.protocol_version(), ProtocolVersion::V1);
        assert_eq!(request.center_module_id(), "11");
        assert_eq!(request.node_limit(), 100);
        assert!(
            serde_json::from_value::<QueryModuleDependencyGraphRequestV1>(serde_json::json!({
                "protocolVersion": 1,
                "centerModuleId": "11",
                "nodeLimit": 50,
                "repositoryPath": "C:/untrusted"
            }))
            .is_err()
        );
        Ok(())
    }
}
