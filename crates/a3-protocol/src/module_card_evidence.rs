use crate::{ModuleCardLifecycleV1, ModuleDependencyEdgeEvidenceV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict capability-bound request for one Evidence hook of one visible Module Card.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleCardEvidenceRequestV1 {
    protocol_version: ProtocolVersion,
    current_index_run_id: String,
    current_snapshot_id: String,
    source_index_run_id: String,
    source_snapshot_id: String,
    card_id: String,
    module_id: String,
    evidence_id: String,
}

impl QueryModuleCardEvidenceRequestV1 {
    /// Creates an untrusted request whose Core-issued anchors are revalidated by the command.
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

    /// Returns the untrusted current publication-run anchor.
    #[must_use]
    pub fn current_index_run_id(&self) -> &str {
        &self.current_index_run_id
    }

    /// Returns the untrusted current publication-snapshot anchor.
    #[must_use]
    pub fn current_snapshot_id(&self) -> &str {
        &self.current_snapshot_id
    }

    /// Returns the untrusted historical Card-run anchor.
    #[must_use]
    pub fn source_index_run_id(&self) -> &str {
        &self.source_index_run_id
    }

    /// Returns the untrusted historical Card-snapshot anchor.
    #[must_use]
    pub fn source_snapshot_id(&self) -> &str {
        &self.source_snapshot_id
    }

    /// Returns the untrusted visible Card identity.
    #[must_use]
    pub fn card_id(&self) -> &str {
        &self.card_id
    }

    /// Returns the untrusted current primary module identity.
    #[must_use]
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    /// Returns the untrusted opaque Evidence hook.
    #[must_use]
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
}

/// Versioned result of one Card-bound Evidence Inspector read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardEvidenceResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleCardEvidenceResultV1,
}

impl ModuleCardEvidenceResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::NoProject)
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::NoPublishedIndex)
    }

    /// Creates the response for historical publications without deterministic modules.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::ProjectionUnavailable)
    }

    /// Creates the response when the selected module is absent or supplementary.
    #[must_use]
    pub const fn module_unavailable() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::ModuleUnavailable)
    }

    /// Creates the response when the current module has no durable verified Card.
    #[must_use]
    pub const fn card_unavailable() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::CardUnavailable)
    }

    /// Creates the response when a publish or Card replacement invalidated visible anchors.
    #[must_use]
    pub const fn selection_changed() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::SelectionChanged)
    }

    /// Creates the response for an ID that does not belong to the selected latest Card.
    #[must_use]
    pub const fn evidence_unavailable() -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::EvidenceUnavailable)
    }

    /// Creates an available bounded Evidence projection.
    #[must_use]
    pub fn available(detail: ModuleCardEvidenceV1) -> Self {
        Self::with_result(ModuleCardEvidenceResultV1::Available {
            detail: Box::new(detail),
        })
    }

    const fn with_result(result: ModuleCardEvidenceResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive selection and availability result.
    #[must_use]
    pub const fn result(&self) -> &ModuleCardEvidenceResultV1 {
        &self.result
    }
}

/// Whether the requested Evidence hook can be inspected for the exact visible selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleCardEvidenceResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index crossed the publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic modules.
    ProjectionUnavailable,
    /// The selected stable module is absent or supplementary.
    ModuleUnavailable,
    /// The selected current module has no durable verified Card.
    CardUnavailable,
    /// The current publication or latest Card no longer matches the visible anchors.
    SelectionChanged,
    /// The opaque Evidence ID is not a member of the selected latest Card.
    EvidenceUnavailable,
    /// One bounded typed Evidence projection is available.
    Available {
        /// Complete source-free Evidence detail.
        detail: Box<ModuleCardEvidenceV1>,
    },
}

/// Whether the exact Evidence payload still resolves in the current published index.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardEvidenceFreshnessV1 {
    /// The exact payload remains present in the current publication.
    Current,
    /// The payload is retained only as clearly marked historical provenance.
    Stale,
}

/// Language-neutral relation retained by a graph-edge Evidence payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardEvidenceRelationV1 {
    /// Lexical containment.
    Contains,
    /// Definition by a file or containing symbol.
    Defines,
    /// Import relationship.
    Imports,
    /// Export relationship.
    Exports,
    /// Syntactically visible call candidate.
    Calls,
    /// Trait or interface implementation.
    Implements,
    /// Type extension or inheritance.
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

/// One bounded Evidence item with independent Card and Evidence freshness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardEvidenceV1 {
    current_index_run_id: String,
    current_snapshot_id: String,
    source_index_run_id: String,
    source_snapshot_id: String,
    card_id: String,
    module_id: String,
    evidence_id: String,
    card_lifecycle: ModuleCardLifecycleV1,
    freshness: ModuleCardEvidenceFreshnessV1,
    payload: ModuleCardEvidencePayloadV1,
}

impl ModuleCardEvidenceV1 {
    /// Creates one application-validated WebView-safe Evidence projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        current_index_run_id: String,
        current_snapshot_id: String,
        source_index_run_id: String,
        source_snapshot_id: String,
        card_id: String,
        module_id: String,
        evidence_id: String,
        card_lifecycle: ModuleCardLifecycleV1,
        freshness: ModuleCardEvidenceFreshnessV1,
        payload: ModuleCardEvidencePayloadV1,
    ) -> Self {
        Self {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            evidence_id,
            card_lifecycle,
            freshness,
            payload,
        }
    }
}

/// Exact content-addressed repository revision without live filesystem authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardEvidenceRevisionV1 {
    path_hex: String,
    content_hash: String,
}

impl ModuleCardEvidenceRevisionV1 {
    /// Creates one already validated relative revision token.
    #[must_use]
    pub const fn new(path_hex: String, content_hash: String) -> Self {
        Self {
            path_hex,
            content_hash,
        }
    }
}

/// Closed source-free Evidence payload union.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum ModuleCardEvidencePayloadV1 {
    /// Exact file revision.
    File {
        /// Content-addressed relative repository revision.
        revision: ModuleCardEvidenceRevisionV1,
    },
    /// Exact structural symbol identity and containing revision.
    Symbol {
        /// Content- and adapter-bound symbol identity.
        symbol_id: String,
        /// Content-addressed relative repository revision.
        revision: ModuleCardEvidenceRevisionV1,
    },
    /// Exact deterministic graph relation and source range.
    GraphEdge {
        /// Language-neutral relation observed by the adapter.
        relation: ModuleCardEvidenceRelationV1,
        /// Existing bounded graph-edge Evidence DTO.
        edge: Box<ModuleDependencyEdgeEvidenceV1>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModuleDependencyEndpointV1, ModuleDependencyProviderV1, ModuleDependencyResolutionV1,
        ModuleDependencySourcePositionV1, ModuleDependencySourceRangeV1,
    };

    #[test]
    fn stale_graph_evidence_serializes_with_independent_card_and_evidence_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let edge = ModuleDependencyEdgeEvidenceV1::new(
            "77".repeat(32),
            ModuleDependencyEndpointV1::File {
                path_hex: "7372632f6c69622e7273".to_owned(),
            },
            ModuleDependencyEndpointV1::Symbol {
                symbol_id: "88".repeat(32),
            },
            "7372632f6c69622e7273".to_owned(),
            "99".repeat(32),
            ModuleDependencySourceRangeV1::new(
                10,
                20,
                ModuleDependencySourcePositionV1::new(1, 2),
                ModuleDependencySourcePositionV1::new(1, 12),
            ),
            ModuleDependencyProviderV1::TreeSitter,
            8_000,
            ModuleDependencyResolutionV1::AdapterLocalSymbol,
        );
        let response = ModuleCardEvidenceResponseV1::available(ModuleCardEvidenceV1::new(
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32),
            "44".repeat(32),
            "55".repeat(32),
            "66".repeat(32),
            "77".repeat(32),
            ModuleCardLifecycleV1::Stale {
                invalidated_by_index_run_id: "11".repeat(32),
                reason: crate::ModuleCardFreshnessReasonV1::EvidenceChanged,
            },
            ModuleCardEvidenceFreshnessV1::Stale,
            ModuleCardEvidencePayloadV1::GraphEdge {
                relation: ModuleCardEvidenceRelationV1::Calls,
                edge: Box::new(edge),
            },
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["detail"]["freshness"], "stale");
        assert_eq!(
            value["result"]["detail"]["cardLifecycle"]["status"],
            "stale"
        );
        assert_eq!(value["result"]["detail"]["payload"]["kind"], "graphEdge");
        assert_eq!(value["result"]["detail"]["payload"]["relation"], "calls");
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let value = serde_json::json!({
            "protocolVersion": 1,
            "currentIndexRunId": "11".repeat(32),
            "currentSnapshotId": "22".repeat(32),
            "sourceIndexRunId": "33".repeat(32),
            "sourceSnapshotId": "44".repeat(32),
            "cardId": "55".repeat(32),
            "moduleId": "66".repeat(32),
            "evidenceId": "77".repeat(32),
            "source": true
        });
        assert!(serde_json::from_value::<QueryModuleCardEvidenceRequestV1>(value).is_err());
    }
}
