use crate::{ModuleDependencySourceRangeV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict pathless request for one consciously triggered deterministic Project Map search.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectMapSearchRequestV1 {
    protocol_version: ProtocolVersion,
    query: String,
}

impl QueryProjectMapSearchRequestV1 {
    /// Creates an untrusted request whose text is validated again by the Rust command boundary.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, query: String) -> Self {
        Self {
            protocol_version,
            query,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted user query without granting FTS-expression or path authority.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }
}

/// Versioned result of one bounded exact-plus-lexical Project Map search.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSearchResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectMapSearchResultV1,
}

impl ProjectMapSearchResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ProjectMapSearchResultV1::NoProject)
    }

    /// Creates the response used before the first atomic index publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ProjectMapSearchResultV1::NoPublishedIndex)
    }

    /// Creates the response for a historical publication missing a required search projection.
    #[must_use]
    pub const fn projection_unavailable(channel: ProjectMapSearchChannelV1) -> Self {
        Self::with_result(ProjectMapSearchResultV1::ProjectionUnavailable { channel })
    }

    /// Creates a bounded evidence-bearing search result.
    #[must_use]
    pub fn available(search: ProjectMapSearchV1) -> Self {
        Self::with_result(ProjectMapSearchResultV1::Available { search })
    }

    const fn with_result(result: ProjectMapSearchResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive project/publication state.
    #[must_use]
    pub const fn result(&self) -> &ProjectMapSearchResultV1 {
        &self.result
    }
}

/// Availability of one consciously triggered current-index search.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectMapSearchResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index crossed the publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates one required deterministic projection.
    ProjectionUnavailable {
        /// Missing exact or lexical projection.
        channel: ProjectMapSearchChannelV1,
    },
    /// One bounded cross-channel result is available.
    Available {
        /// Current-index search result with complete visible provenance.
        search: ProjectMapSearchV1,
    },
}

/// Retrieval channel exposed without storage-engine or FTS syntax details.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapSearchChannelV1 {
    /// Deterministic identifier, signature, path, or role projection.
    Exact,
    /// Typo-tolerant weighted full-text projection.
    Lexical,
}

/// One atomic bounded hybrid-search result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSearchV1 {
    query: String,
    index_run_id: String,
    snapshot_id: String,
    fusion_policy_version: u32,
    hits: Vec<ProjectMapSearchHitV1>,
    truncated: bool,
}

impl ProjectMapSearchV1 {
    /// Creates an application-validated result bound to one published run and query.
    #[must_use]
    pub const fn new(
        query: String,
        index_run_id: String,
        snapshot_id: String,
        fusion_policy_version: u32,
        hits: Vec<ProjectMapSearchHitV1>,
        truncated: bool,
    ) -> Self {
        Self {
            query,
            index_run_id,
            snapshot_id,
            fusion_policy_version,
            hits,
            truncated,
        }
    }
}

/// Hard provenance priority retained before any weighted score.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapSearchPriorityV1 {
    /// At least one exact deterministic channel matched.
    Exact,
    /// At least one non-semantic evidence-bearing channel matched.
    Evidence,
}

/// One deduplicated current target with auditable ranking provenance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSearchHitV1 {
    rank: u16,
    priority: ProjectMapSearchPriorityV1,
    final_score: u32,
    sources: Vec<ProjectMapSearchSourceV1>,
    target: ProjectMapSearchTargetV1,
}

impl ProjectMapSearchHitV1 {
    /// Creates one rank-ordered target after deterministic fusion.
    #[must_use]
    pub const fn new(
        rank: u16,
        priority: ProjectMapSearchPriorityV1,
        final_score: u32,
        sources: Vec<ProjectMapSearchSourceV1>,
        target: ProjectMapSearchTargetV1,
    ) -> Self {
        Self {
            rank,
            priority,
            final_score,
            sources,
            target,
        }
    }
}

/// Channel-native explanation retained after target deduplication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "channel")]
pub enum ProjectMapSearchSourceV1 {
    /// Exact identifier, signature, path, prefix, or role match.
    Exact {
        /// Machine-readable deterministic match reason.
        explanation: ProjectMapExactExplanationV1,
        /// Versioned native relevance normalized to basis points.
        normalized_score_basis_points: u16,
    },
    /// Weighted typo-tolerant full-text match.
    Lexical {
        /// Highest-weight matching projection field.
        explanation: ProjectMapLexicalExplanationV1,
        /// Native deterministic FTS score before normalization.
        native_score: u32,
        /// Versioned native relevance normalized to basis points.
        normalized_score_basis_points: u16,
    },
}

/// Exact-channel match reason.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapExactExplanationV1 {
    /// Query equalled canonical repository-relative path bytes.
    NormalizedPathExact,
    /// Query equalled the deterministic qualified name.
    QualifiedNameExact,
    /// Query equalled the adapter-derived simple name.
    SymbolNameExact,
    /// Query equalled the declaration signature.
    SignatureExact,
    /// Deterministic qualified name starts with the query.
    QualifiedNamePrefix,
    /// Simple symbol name starts with the query.
    SymbolNamePrefix,
    /// Declaration signature starts with the query.
    SignaturePrefix,
    /// File is a deterministic manifest.
    ManifestRole,
    /// Symbol is a syntactic entrypoint.
    EntrypointRole,
    /// Symbol is a syntactic test.
    TestRole,
}

/// Lexical field that supplied the strongest weighted match.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapLexicalExplanationV1 {
    /// Canonical path supplied the strongest match.
    Path,
    /// Qualified symbol name supplied the strongest match.
    QualifiedName,
    /// Simple symbol name supplied the strongest match.
    SymbolName,
    /// Declaration signature supplied the strongest match.
    Signature,
}

/// Current file or structural symbol serving as its own exact evidence hook.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum ProjectMapSearchTargetV1 {
    /// Current immutable file revision.
    File {
        /// Exact current evidence revision.
        evidence: ProjectMapSearchEvidenceV1,
    },
    /// Current structural symbol and declaration range.
    Symbol {
        /// Stable content- and adapter-derived identity.
        symbol_id: String,
        /// Language-neutral symbol category.
        symbol_kind: ProjectMapSearchSymbolKindV1,
        /// Bounded simple display name.
        name: String,
        /// Deterministic containment-derived display name.
        qualified_name: String,
        /// Optional bounded adapter-derived declaration signature.
        signature: Option<String>,
        /// Exact current revision and declaration range.
        evidence: ProjectMapSearchEvidenceV1,
    },
}

/// Current source-free revision identity exposed as navigable evidence metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectMapSearchEvidenceV1 {
    path_display: String,
    path_hex: String,
    content_hash: String,
    declaration_range: Option<ModuleDependencySourceRangeV1>,
}

impl ProjectMapSearchEvidenceV1 {
    /// Creates one current exact revision without granting a filesystem-read capability.
    #[must_use]
    pub const fn new(
        path_display: String,
        path_hex: String,
        content_hash: String,
        declaration_range: Option<ModuleDependencySourceRangeV1>,
    ) -> Self {
        Self {
            path_display,
            path_hex,
            content_hash,
            declaration_range,
        }
    }
}

/// Language-neutral structural symbol category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMapSearchSymbolKindV1 {
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

#[cfg(test)]
mod tests {
    use super::{
        ProjectMapSearchChannelV1, ProjectMapSearchResponseV1, ProjectMapSearchResultV1,
        QueryProjectMapSearchRequestV1,
    };
    use crate::ProtocolVersion;

    #[test]
    fn request_rejects_unknown_fields() {
        let json = r#"{"protocolVersion":1,"query":"parser","path":"C:/secret"}"#;
        assert!(serde_json::from_str::<QueryProjectMapSearchRequestV1>(json).is_err());
    }

    #[test]
    fn projection_unavailable_retains_the_missing_channel() {
        assert!(matches!(
            ProjectMapSearchResponseV1::projection_unavailable(ProjectMapSearchChannelV1::Lexical)
                .result(),
            ProjectMapSearchResultV1::ProjectionUnavailable {
                channel: ProjectMapSearchChannelV1::Lexical
            }
        ));
    }

    #[test]
    fn request_retains_version_and_untrusted_query_only() {
        let request =
            QueryProjectMapSearchRequestV1::new(ProtocolVersion::CURRENT, "parser".to_owned());
        assert_eq!(request.protocol_version(), ProtocolVersion::CURRENT);
        assert_eq!(request.query(), "parser");
    }
}
