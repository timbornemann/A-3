use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict bounded request for one page of the active project's deterministic module tree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleTreeRequestV1 {
    protocol_version: ProtocolVersion,
    parent_module_id: Option<String>,
    after_module_id: Option<String>,
    limit: u16,
}

impl QueryModuleTreeRequestV1 {
    /// Creates a request whose stable IDs and limit are validated by the Rust command boundary.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        parent_module_id: Option<String>,
        after_module_id: Option<String>,
        limit: u16,
    ) -> Self {
        Self {
            protocol_version,
            parent_module_id,
            after_module_id,
            limit,
        }
    }

    /// Creates the first top-level page with the product default size.
    #[must_use]
    pub const fn root() -> Self {
        Self::new(ProtocolVersion::CURRENT, None, None, 50)
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the selected parent module, or None for top level.
    #[must_use]
    pub fn parent_module_id(&self) -> Option<&str> {
        self.parent_module_id.as_deref()
    }

    /// Returns the exclusive stable module cursor.
    #[must_use]
    pub fn after_module_id(&self) -> Option<&str> {
        self.after_module_id.as_deref()
    }

    /// Returns the untrusted page-size value for application validation.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

/// Versioned progressive module-tree result selected from the Core-owned active project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreeResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleTreeResultV1,
}

impl ModuleTreeResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleTreeResultV1::NoProject,
        }
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleTreeResultV1::NoPublishedIndex,
        }
    }

    /// Creates the response used when a historical publication predates module schema V8.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleTreeResultV1::ProjectionUnavailable,
        }
    }

    /// Creates an available page from application-validated bounded values.
    #[must_use]
    pub fn available(page: ModuleTreePageV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleTreeResultV1::Available {
                page: Box::new(page),
            },
        }
    }

    /// Returns the mutually exclusive project/publication result.
    #[must_use]
    pub const fn result(&self) -> &ModuleTreeResultV1 {
        &self.result
    }
}

/// Whether an active project and current deterministic module projection exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleTreeResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// The latest historical publication has no deterministic module marker.
    ProjectionUnavailable,
    /// One bounded top-level or direct-child module page is available.
    Available {
        /// Current atomic page containing at most one hundred primary modules.
        page: Box<ModuleTreePageV1>,
    },
}

/// Bounded WebView-safe page of direct deterministic primary modules.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreePageV1 {
    index_run_id: String,
    snapshot_id: String,
    parent_module_id: Option<String>,
    primary_module_count: String,
    graph_community_count: String,
    entries: Vec<ModuleTreeEntryV1>,
    next_after_module_id: Option<String>,
}

impl ModuleTreePageV1 {
    /// Creates one strict page from already validated application values.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        parent_module_id: Option<String>,
        primary_module_count: String,
        graph_community_count: String,
        entries: Vec<ModuleTreeEntryV1>,
        next_after_module_id: Option<String>,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            parent_module_id,
            primary_module_count,
            graph_community_count,
            entries,
            next_after_module_id,
        }
    }
}

/// Deterministic signal that established a primary module boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleTreeEntryKindV1 {
    /// One or more current package manifests establish the boundary.
    ManifestBoundary,
    /// A deterministic repository path establishes the boundary.
    PathBoundary,
}

/// Whether another direct child-module page can exist below a node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleTreeChildStateV1 {
    /// No nested primary boundary exists.
    Leaf,
    /// At least one nested primary boundary exists.
    HasChildren,
}

/// Exact current file-revision evidence without source content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreeRevisionV1 {
    path_hex: String,
    content_hash: String,
}

impl ModuleTreeRevisionV1 {
    /// Creates one lossless relative path and content-hash pair.
    #[must_use]
    pub const fn new(path_hex: String, content_hash: String) -> Self {
        Self {
            path_hex,
            content_hash,
        }
    }
}

/// Representative membership and optional package-manifest evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreeBoundaryEvidenceV1 {
    representative_revision: Option<ModuleTreeRevisionV1>,
    manifest_revision: Option<ModuleTreeRevisionV1>,
}

impl ModuleTreeBoundaryEvidenceV1 {
    /// Creates evidence already validated against module kind and counts in Application.
    #[must_use]
    pub const fn new(
        representative_revision: Option<ModuleTreeRevisionV1>,
        manifest_revision: Option<ModuleTreeRevisionV1>,
    ) -> Self {
        Self {
            representative_revision,
            manifest_revision,
        }
    }
}

/// One bounded featured-symbol category and its visible truncation state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreeFeatureCountV1 {
    count: String,
    truncated: bool,
}

impl ModuleTreeFeatureCountV1 {
    /// Creates a lossless count with application-validated truncation semantics.
    #[must_use]
    pub const fn new(count: String, truncated: bool) -> Self {
        Self { count, truncated }
    }
}

/// One direct primary module with current deterministic evidence and bounded feature metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleTreeEntryV1 {
    module_id: String,
    kind: ModuleTreeEntryKindV1,
    root_path_hex: Option<String>,
    name: String,
    name_truncated: bool,
    boundary_evidence: ModuleTreeBoundaryEvidenceV1,
    manifest_count: String,
    file_count: String,
    symbol_count: String,
    central_symbols: ModuleTreeFeatureCountV1,
    entrypoints: ModuleTreeFeatureCountV1,
    tests: ModuleTreeFeatureCountV1,
    child_state: ModuleTreeChildStateV1,
}

impl ModuleTreeEntryV1 {
    /// Creates one application-validated primary module projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        module_id: String,
        kind: ModuleTreeEntryKindV1,
        root_path_hex: Option<String>,
        name: String,
        name_truncated: bool,
        boundary_evidence: ModuleTreeBoundaryEvidenceV1,
        manifest_count: String,
        file_count: String,
        symbol_count: String,
        central_symbols: ModuleTreeFeatureCountV1,
        entrypoints: ModuleTreeFeatureCountV1,
        tests: ModuleTreeFeatureCountV1,
        child_state: ModuleTreeChildStateV1,
    ) -> Self {
        Self {
            module_id,
            kind,
            root_path_hex,
            name,
            name_truncated,
            boundary_evidence,
            manifest_count,
            file_count,
            symbol_count,
            central_symbols,
            entrypoints,
            tests,
            child_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModuleTreeBoundaryEvidenceV1, ModuleTreeChildStateV1, ModuleTreeEntryKindV1,
        ModuleTreeEntryV1, ModuleTreeFeatureCountV1, ModuleTreePageV1, ModuleTreeResponseV1,
        ModuleTreeRevisionV1, QueryModuleTreeRequestV1,
    };
    use crate::ProtocolVersion;

    #[test]
    fn available_page_serializes_counts_hierarchy_and_revision_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = ModuleTreeResponseV1::available(ModuleTreePageV1::new(
            "11".repeat(32),
            "22".repeat(32),
            None,
            "1".to_owned(),
            "2".to_owned(),
            vec![ModuleTreeEntryV1::new(
                "33".repeat(32),
                ModuleTreeEntryKindV1::ManifestBoundary,
                None,
                "Repository".to_owned(),
                false,
                ModuleTreeBoundaryEvidenceV1::new(
                    Some(ModuleTreeRevisionV1::new(
                        "7372632f6c69622e7273".to_owned(),
                        "44".repeat(32),
                    )),
                    Some(ModuleTreeRevisionV1::new(
                        "436172676f2e746f6d6c".to_owned(),
                        "55".repeat(32),
                    )),
                ),
                "1".to_owned(),
                "1".to_owned(),
                "1".to_owned(),
                ModuleTreeFeatureCountV1::new("1".to_owned(), false),
                ModuleTreeFeatureCountV1::new("0".to_owned(), false),
                ModuleTreeFeatureCountV1::new("0".to_owned(), false),
                ModuleTreeChildStateV1::HasChildren,
            )],
            None,
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(value["result"]["page"]["graphCommunityCount"], "2");
        assert_eq!(
            value["result"]["page"]["entries"][0]["boundaryEvidence"]["manifestRevision"]["contentHash"],
            "55".repeat(32)
        );
        assert_eq!(
            value["result"]["page"]["entries"][0]["childState"],
            "hasChildren"
        );
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields_and_retains_untrusted_values_for_core_validation()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = serde_json::from_value::<QueryModuleTreeRequestV1>(serde_json::json!({
            "protocolVersion": 1,
            "parentModuleId": "11",
            "afterModuleId": null,
            "limit": 100
        }))?;
        assert_eq!(request.protocol_version(), ProtocolVersion::V1);
        assert_eq!(request.parent_module_id(), Some("11"));
        assert_eq!(request.limit(), 100);

        let unknown = serde_json::json!({
            "protocolVersion": 1,
            "parentModuleId": null,
            "afterModuleId": null,
            "limit": 50,
            "repositoryPath": "C:/untrusted"
        });
        assert!(serde_json::from_value::<QueryModuleTreeRequestV1>(unknown).is_err());
        Ok(())
    }
}
