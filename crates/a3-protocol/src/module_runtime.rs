use crate::{ModuleDependencyEdgeEvidenceV1, ModuleDependencySourceRangeV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict bounded request for adapter-proven runtime roots of one primary module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleRuntimeMapRequestV1 {
    protocol_version: ProtocolVersion,
    module_id: String,
    entrypoint_limit: u16,
    test_limit: u16,
}

impl QueryModuleRuntimeMapRequestV1 {
    /// Creates an untrusted request validated by the Rust command boundary.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        module_id: String,
        entrypoint_limit: u16,
        test_limit: u16,
    ) -> Self {
        Self {
            protocol_version,
            module_id,
            entrypoint_limit,
            test_limit,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted stable module token.
    #[must_use]
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    /// Returns the untrusted entrypoint-prefix boundary.
    #[must_use]
    pub const fn entrypoint_limit(&self) -> u16 {
        self.entrypoint_limit
    }

    /// Returns the untrusted test-prefix boundary.
    #[must_use]
    pub const fn test_limit(&self) -> u16 {
        self.test_limit
    }
}

/// Versioned runtime-root result selected from Core-owned project state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeMapResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleRuntimeMapResultV1,
}

impl ModuleRuntimeMapResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ModuleRuntimeMapResultV1::NoProject)
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ModuleRuntimeMapResultV1::NoPublishedIndex)
    }

    /// Creates the response used when a historical publication predates module schema V8.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ModuleRuntimeMapResultV1::ProjectionUnavailable)
    }

    /// Creates the response used when the selected module is no longer current or primary.
    #[must_use]
    pub const fn module_unavailable() -> Self {
        Self::with_result(ModuleRuntimeMapResultV1::ModuleUnavailable)
    }

    /// Creates an available map from application-validated current roots.
    #[must_use]
    pub fn available(map: ModuleRuntimeMapV1) -> Self {
        Self::with_result(ModuleRuntimeMapResultV1::Available { map })
    }

    const fn with_result(result: ModuleRuntimeMapResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive project/publication result.
    #[must_use]
    pub const fn result(&self) -> &ModuleRuntimeMapResultV1 {
        &self.result
    }
}

/// Whether current deterministic runtime roots exist for the selected primary module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleRuntimeMapResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// The latest historical publication has no deterministic module marker.
    ProjectionUnavailable,
    /// The selected stable ID is absent or names a supplementary graph community.
    ModuleUnavailable,
    /// One bounded pair of current role-specific root prefixes is available.
    Available {
        /// Atomic runtime map carrying exact current symbol evidence.
        map: ModuleRuntimeMapV1,
    },
}

/// Atomic current entrypoint and test prefixes for one primary module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeMapV1 {
    index_run_id: String,
    snapshot_id: String,
    module_id: String,
    entrypoints: ModuleRuntimeRootSetV1,
    tests: ModuleRuntimeRootSetV1,
}

impl ModuleRuntimeMapV1 {
    /// Creates one application-validated atomic runtime map.
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        module_id: String,
        entrypoints: ModuleRuntimeRootSetV1,
        tests: ModuleRuntimeRootSetV1,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            module_id,
            entrypoints,
            tests,
        }
    }
}

/// One bounded role-specific prefix with explicit storage and formation boundaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeRootSetV1 {
    roots: Vec<ModuleRuntimeRootV1>,
    stored_count: String,
    projection_truncated: bool,
    visible_truncated: bool,
}

impl ModuleRuntimeRootSetV1 {
    /// Creates one already validated current root prefix.
    #[must_use]
    pub const fn new(
        roots: Vec<ModuleRuntimeRootV1>,
        stored_count: String,
        projection_truncated: bool,
        visible_truncated: bool,
    ) -> Self {
        Self {
            roots,
            stored_count,
            projection_truncated,
            visible_truncated,
        }
    }
}

/// Semantic class of a visible runtime-map root.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleRuntimeRootKindV1 {
    /// Adapter-proven program, library, or script entrypoint.
    Entrypoint,
    /// Adapter-proven test definition.
    Test,
}

/// One rank-ordered runtime root and its exact current symbol evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeRootV1 {
    kind: ModuleRuntimeRootKindV1,
    rank: u16,
    symbol: ModuleRuntimeSymbolV1,
}

impl ModuleRuntimeRootV1 {
    /// Creates one root whose role and rank were validated by the application layer.
    #[must_use]
    pub const fn new(
        kind: ModuleRuntimeRootKindV1,
        rank: u16,
        symbol: ModuleRuntimeSymbolV1,
    ) -> Self {
        Self { kind, rank, symbol }
    }
}

/// Language-neutral structural symbol category exposed to the untrusted WebView.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleRuntimeSymbolKindV1 {
    /// File or language module.
    Module,
    /// Namespace or package scope.
    Namespace,
    /// Free function.
    Function,
    /// Type-associated function or method.
    Method,
    /// Struct or equivalent record.
    Struct,
    /// Enumeration type.
    Enum,
    /// Trait or protocol.
    Trait,
    /// Interface declaration.
    Interface,
    /// Class declaration.
    Class,
    /// Language implementation block.
    Implementation,
    /// Type alias.
    TypeAlias,
    /// Constant declaration.
    Constant,
    /// Static storage declaration.
    Static,
    /// Variable declaration.
    Variable,
    /// Field or property.
    Field,
    /// Enumeration variant.
    Variant,
    /// Function or method parameter.
    Parameter,
}

/// Current structural symbol with only navigation-safe evidence fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeSymbolV1 {
    symbol_id: String,
    symbol_kind: ModuleRuntimeSymbolKindV1,
    name: String,
    evidence_id: String,
    path_hex: String,
    content_hash: String,
    selection_range: ModuleDependencySourceRangeV1,
}

impl ModuleRuntimeSymbolV1 {
    /// Creates a WebView-safe exact current structural-symbol projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        symbol_id: String,
        symbol_kind: ModuleRuntimeSymbolKindV1,
        name: String,
        evidence_id: String,
        path_hex: String,
        content_hash: String,
        selection_range: ModuleDependencySourceRangeV1,
    ) -> Self {
        Self {
            symbol_id,
            symbol_kind,
            name,
            evidence_id,
            path_hex,
            content_hash,
            selection_range,
        }
    }
}

/// Strict freshness- and role-bound request for one fixed runtime-flow preset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleRuntimeFlowRequestV1 {
    protocol_version: ProtocolVersion,
    expected_index_run_id: String,
    expected_snapshot_id: String,
    module_id: String,
    root_symbol_id: String,
    kind: ModuleRuntimeFlowKindV1,
    result_limit: u16,
}

impl QueryModuleRuntimeFlowRequestV1 {
    /// Creates an untrusted request validated by the Rust command boundary.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        expected_index_run_id: String,
        expected_snapshot_id: String,
        module_id: String,
        root_symbol_id: String,
        kind: ModuleRuntimeFlowKindV1,
        result_limit: u16,
    ) -> Self {
        Self {
            protocol_version,
            expected_index_run_id,
            expected_snapshot_id,
            module_id,
            root_symbol_id,
            kind,
            result_limit,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted publication-run token visible with the selected root.
    #[must_use]
    pub fn expected_index_run_id(&self) -> &str {
        &self.expected_index_run_id
    }

    /// Returns the untrusted snapshot token visible with the selected root.
    #[must_use]
    pub fn expected_snapshot_id(&self) -> &str {
        &self.expected_snapshot_id
    }

    /// Returns the untrusted primary-module token.
    #[must_use]
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    /// Returns the untrusted role-specific root-symbol token.
    #[must_use]
    pub fn root_symbol_id(&self) -> &str {
        &self.root_symbol_id
    }

    /// Returns the only requested fixed traversal preset.
    #[must_use]
    pub const fn kind(&self) -> ModuleRuntimeFlowKindV1 {
        self.kind
    }

    /// Returns the untrusted target-count boundary.
    #[must_use]
    pub const fn result_limit(&self) -> u16 {
        self.result_limit
    }
}

/// Only the two role-specific graph presets allowed across the IPC boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleRuntimeFlowKindV1 {
    /// At most two outgoing syntactic `Calls` hops from an entrypoint.
    EntrypointCalls,
    /// One direct outgoing syntactic `Tests` hop from a test definition.
    TestTargets,
}

/// Versioned explicit-flow result selected from Core-owned project state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeFlowResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleRuntimeFlowResultV1,
}

impl ModuleRuntimeFlowResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::NoProject)
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::NoPublishedIndex)
    }

    /// Creates the response used when the latest publication predates required projections.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::ProjectionUnavailable)
    }

    /// Creates the response used after another publication replaced the visible seed.
    #[must_use]
    pub const fn publication_changed() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::PublicationChanged)
    }

    /// Creates the response used when the selected primary module disappeared.
    #[must_use]
    pub const fn module_unavailable() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::ModuleUnavailable)
    }

    /// Creates the response used when the symbol no longer proves the required role.
    #[must_use]
    pub const fn root_unavailable() -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::RootUnavailable)
    }

    /// Creates an available flow from application-validated current evidence paths.
    #[must_use]
    pub fn available(flow: ModuleRuntimeFlowV1) -> Self {
        Self::with_result(ModuleRuntimeFlowResultV1::Available { flow })
    }

    const fn with_result(result: ModuleRuntimeFlowResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive project/publication/seed result.
    #[must_use]
    pub const fn result(&self) -> &ModuleRuntimeFlowResultV1 {
        &self.result
    }
}

/// Whether the visible seed can still be traversed under its fixed role preset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleRuntimeFlowResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// The latest historical publication lacks a required deterministic projection.
    ProjectionUnavailable,
    /// Another atomic index publication replaced the visible root list.
    PublicationChanged,
    /// The selected stable module is absent or no longer primary.
    ModuleUnavailable,
    /// The symbol does not currently carry the required feature role in the module.
    RootUnavailable,
    /// One current bounded deterministic traversal is available.
    Available {
        /// Current targets and complete shortest evidence paths.
        flow: ModuleRuntimeFlowV1,
    },
}

/// Current bounded targets and complete shortest evidence paths for one fixed preset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeFlowV1 {
    index_run_id: String,
    snapshot_id: String,
    module_id: String,
    root_symbol_id: String,
    kind: ModuleRuntimeFlowKindV1,
    hits: Vec<ModuleRuntimeFlowHitV1>,
    truncated: bool,
}

impl ModuleRuntimeFlowV1 {
    /// Creates one application-validated, publication-bound flow result.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        module_id: String,
        root_symbol_id: String,
        kind: ModuleRuntimeFlowKindV1,
        hits: Vec<ModuleRuntimeFlowHitV1>,
        truncated: bool,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            module_id,
            root_symbol_id,
            kind,
            hits,
            truncated,
        }
    }
}

/// Current file or structural symbol reached by the fixed traversal preset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum ModuleRuntimeFlowTargetV1 {
    /// One current repository file revision.
    File {
        /// Stable evidence-inspector identity for the exact file revision.
        #[serde(rename = "evidenceId")]
        evidence_id: String,
        /// Lowercase hexadecimal canonical repository-relative path bytes.
        #[serde(rename = "pathHex")]
        path_hex: String,
        /// Lowercase hexadecimal content hash of the current revision.
        #[serde(rename = "contentHash")]
        content_hash: String,
    },
    /// One current structural symbol.
    Symbol {
        /// Navigation-safe exact symbol projection.
        symbol: ModuleRuntimeSymbolV1,
    },
}

/// One deterministic target paired with its complete shortest evidence path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeFlowHitV1 {
    target: ModuleRuntimeFlowTargetV1,
    path: Vec<ModuleRuntimeFlowEdgeV1>,
}

impl ModuleRuntimeFlowHitV1 {
    /// Creates one target with its complete application-validated shortest path.
    #[must_use]
    pub const fn new(
        target: ModuleRuntimeFlowTargetV1,
        path: Vec<ModuleRuntimeFlowEdgeV1>,
    ) -> Self {
        Self { target, path }
    }
}

/// One observed relation and its exact current source evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleRuntimeFlowRelationV1 {
    /// Syntactically observed outgoing call.
    Calls,
    /// Adapter-proven direct test relationship.
    Tests,
}

/// One observed relation and its exact current source evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleRuntimeFlowEdgeV1 {
    relation: ModuleRuntimeFlowRelationV1,
    evidence: ModuleDependencyEdgeEvidenceV1,
}

impl ModuleRuntimeFlowEdgeV1 {
    /// Creates one observed relation with exact current edge evidence.
    #[must_use]
    pub const fn new(
        relation: ModuleRuntimeFlowRelationV1,
        evidence: ModuleDependencyEdgeEvidenceV1,
    ) -> Self {
        Self { relation, evidence }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModuleRuntimeFlowKindV1, ModuleRuntimeFlowRelationV1, ModuleRuntimeMapResponseV1,
        ModuleRuntimeMapV1, ModuleRuntimeRootKindV1, ModuleRuntimeRootSetV1, ModuleRuntimeRootV1,
        ModuleRuntimeSymbolKindV1, ModuleRuntimeSymbolV1, QueryModuleRuntimeFlowRequestV1,
        QueryModuleRuntimeMapRequestV1,
    };
    use crate::{ModuleDependencySourcePositionV1, ModuleDependencySourceRangeV1, ProtocolVersion};

    #[test]
    fn runtime_map_serializes_current_evidence_and_both_truncation_causes()
    -> Result<(), Box<dyn std::error::Error>> {
        let point = ModuleDependencySourcePositionV1::new(1, 2);
        let symbol = ModuleRuntimeSymbolV1::new(
            "11".repeat(32),
            ModuleRuntimeSymbolKindV1::Function,
            "main".to_owned(),
            "22".repeat(32),
            "7372632f6d61696e2e7273".to_owned(),
            "33".repeat(32),
            ModuleDependencySourceRangeV1::new(4, 8, point, point),
        );
        let response = ModuleRuntimeMapResponseV1::available(ModuleRuntimeMapV1::new(
            "44".repeat(32),
            "55".repeat(32),
            "66".repeat(32),
            ModuleRuntimeRootSetV1::new(
                vec![ModuleRuntimeRootV1::new(
                    ModuleRuntimeRootKindV1::Entrypoint,
                    1,
                    symbol,
                )],
                "1".to_owned(),
                true,
                true,
            ),
            ModuleRuntimeRootSetV1::new(Vec::new(), "0".to_owned(), false, false),
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(
            value["result"]["map"]["entrypoints"]["roots"][0]["kind"],
            "entrypoint"
        );
        assert_eq!(
            value["result"]["map"]["entrypoints"]["projectionTruncated"],
            true
        );
        assert_eq!(
            value["result"]["map"]["entrypoints"]["roots"][0]["symbol"]["pathHex"],
            "7372632f6d61696e2e7273"
        );
        Ok(())
    }

    #[test]
    fn runtime_requests_reject_unknown_fields_and_retain_untrusted_bounds()
    -> Result<(), Box<dyn std::error::Error>> {
        let map = serde_json::from_value::<QueryModuleRuntimeMapRequestV1>(serde_json::json!({
            "protocolVersion": 1,
            "moduleId": "11",
            "entrypointLimit": 256,
            "testLimit": 0
        }))?;
        assert_eq!(map.module_id(), "11");
        assert_eq!(map.entrypoint_limit(), 256);
        assert_eq!(map.test_limit(), 0);
        assert!(
            serde_json::from_value::<QueryModuleRuntimeMapRequestV1>(serde_json::json!({
                "protocolVersion": 1,
                "moduleId": "11",
                "entrypointLimit": 20,
                "testLimit": 20,
                "path": "C:/untrusted"
            }))
            .is_err()
        );

        let flow = serde_json::from_value::<QueryModuleRuntimeFlowRequestV1>(serde_json::json!({
            "protocolVersion": 1,
            "expectedIndexRunId": "22",
            "expectedSnapshotId": "33",
            "moduleId": "44",
            "rootSymbolId": "55",
            "kind": "testTargets",
            "resultLimit": 100
        }))?;
        assert_eq!(flow.protocol_version(), ProtocolVersion::V1);
        assert_eq!(flow.kind(), ModuleRuntimeFlowKindV1::TestTargets);
        assert_eq!(flow.result_limit(), 100);
        assert!(
            serde_json::from_value::<ModuleRuntimeFlowRelationV1>(serde_json::json!("imports"))
                .is_err()
        );
        Ok(())
    }
}
