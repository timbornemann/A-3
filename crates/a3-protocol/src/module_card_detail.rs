use crate::{ModuleCardFreshnessReasonV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict stable-ID request for the latest durable card of one selected module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleCardDetailRequestV1 {
    protocol_version: ProtocolVersion,
    module_id: String,
}

impl QueryModuleCardDetailRequestV1 {
    /// Creates an untrusted request validated by the Rust command boundary.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, module_id: String) -> Self {
        Self {
            protocol_version,
            module_id,
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
}

/// Versioned latest-card result selected from Core-owned project state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardDetailResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleCardDetailResultV1,
}

impl ModuleCardDetailResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(ModuleCardDetailResultV1::NoProject)
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(ModuleCardDetailResultV1::NoPublishedIndex)
    }

    /// Creates the response for historical publications without the module projection.
    #[must_use]
    pub const fn projection_unavailable() -> Self {
        Self::with_result(ModuleCardDetailResultV1::ProjectionUnavailable)
    }

    /// Creates the response when the selection is absent or supplementary.
    #[must_use]
    pub const fn module_unavailable() -> Self {
        Self::with_result(ModuleCardDetailResultV1::ModuleUnavailable)
    }

    /// Creates the response when the selected current module has no verified card.
    #[must_use]
    pub const fn card_unavailable() -> Self {
        Self::with_result(ModuleCardDetailResultV1::CardUnavailable)
    }

    /// Creates an available detail from an application-validated durable card.
    #[must_use]
    pub fn available(detail: ModuleCardDetailV1) -> Self {
        Self::with_result(ModuleCardDetailResultV1::Available {
            detail: Box::new(detail),
        })
    }

    const fn with_result(result: ModuleCardDetailResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive project, publication, module, and card result.
    #[must_use]
    pub const fn result(&self) -> &ModuleCardDetailResultV1 {
        &self.result
    }
}

/// Whether one latest durable Module Card is available for the explicit selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleCardDetailResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index crossed the publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic modules.
    ProjectionUnavailable,
    /// The selected stable ID is absent or supplementary.
    ModuleUnavailable,
    /// The selected current primary module has no durable verified Card.
    CardUnavailable,
    /// One bounded latest Card is available with explicit freshness labels.
    Available {
        /// Complete bounded Card detail.
        detail: Box<ModuleCardDetailV1>,
    },
}

/// Latest durable Card body plus current and historical publication anchors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardDetailV1 {
    current_index_run_id: String,
    current_snapshot_id: String,
    source_index_run_id: String,
    source_snapshot_id: String,
    card_id: String,
    module_id: String,
    schema_version: u16,
    mapper_profile_version: u16,
    confidence_basis_points: u16,
    lifecycle: ModuleCardLifecycleV1,
    fields: Vec<ModuleCardDetailFieldV1>,
}

impl ModuleCardDetailV1 {
    /// Creates one already validated WebView-safe Card projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        current_index_run_id: String,
        current_snapshot_id: String,
        source_index_run_id: String,
        source_snapshot_id: String,
        card_id: String,
        module_id: String,
        schema_version: u16,
        mapper_profile_version: u16,
        confidence_basis_points: u16,
        lifecycle: ModuleCardLifecycleV1,
        fields: Vec<ModuleCardDetailFieldV1>,
    ) -> Self {
        Self {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            schema_version,
            mapper_profile_version,
            confidence_basis_points,
            lifecycle,
            fields,
        }
    }
}

/// Effective card lifecycle relative to the latest publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleCardLifecycleV1 {
    /// Card and every visible claim remain current.
    Current,
    /// Direct evidence or compatibility invalidated the Card.
    Stale {
        /// Publication that recorded invalidation.
        invalidated_by_index_run_id: String,
        /// Auditable deterministic cause.
        reason: ModuleCardFreshnessReasonV1,
    },
    /// A direct dependency changed and conservative review is required.
    NeedsReview {
        /// Publication that recorded the review requirement.
        invalidated_by_index_run_id: String,
        /// Auditable deterministic cause.
        reason: ModuleCardFreshnessReasonV1,
    },
}

/// Canonical version-one Card field identifier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardFieldKindV1 {
    /// Human-readable module title.
    Title,
    /// Canonical owned repository paths.
    Paths,
    /// Concise reason the module exists.
    Purpose,
    /// Behaviors owned by the module.
    Responsibilities,
    /// Publicly consumed symbols or interfaces.
    PublicSurface,
    /// Confirmed execution or package entrypoints.
    Entrypoints,
    /// Incoming and outgoing module dependencies.
    Dependencies,
    /// Important data movement through the module.
    DataFlows,
    /// Behavioral or structural rules.
    Invariants,
    /// Tests and test roots covering the module.
    Tests,
    /// Known failure modes or maintenance risks.
    Risks,
    /// Explicit unresolved questions.
    OpenQuestions,
}

/// One bounded Card field with evidence identities and classified values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardDetailFieldV1 {
    kind: ModuleCardFieldKindV1,
    evidence_ids: Vec<String>,
    values: Vec<ModuleCardValueV1>,
}

impl ModuleCardDetailFieldV1 {
    /// Creates one application-validated field.
    #[must_use]
    pub const fn new(
        kind: ModuleCardFieldKindV1,
        evidence_ids: Vec<String>,
        values: Vec<ModuleCardValueV1>,
    ) -> Self {
        Self {
            kind,
            evidence_ids,
            values,
        }
    }
}

/// One Card value and the sole verified claim classifying it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardValueV1 {
    value: String,
    claim: ModuleCardClaimV1,
}

impl ModuleCardValueV1 {
    /// Creates one application-validated value and claim pair.
    #[must_use]
    pub const fn new(value: String, claim: ModuleCardClaimV1) -> Self {
        Self { value, claim }
    }
}

/// Epistemic classification of a verified Card value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardClaimKindV1 {
    /// Positive deterministic structure with matching evidence.
    Fact,
    /// Direct observation whose meaning is not a structural invariant.
    Observation,
    /// Architecture intent, interpretation, or negative absence.
    Hypothesis,
}

/// Effective freshness label that prevents stale facts from looking current.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardClaimStateV1 {
    /// The claim remains current for the latest publication.
    Current,
    /// The claim is stale regardless of its original epistemic kind.
    Stale,
    /// The claim requires review regardless of its original epistemic kind.
    NeedsReview,
}

/// One claim identity, classification, independent confidence, state, and evidence hooks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardClaimV1 {
    claim_id: String,
    kind: ModuleCardClaimKindV1,
    state: ModuleCardClaimStateV1,
    confidence_basis_points: u16,
    evidence_ids: Vec<String>,
}

impl ModuleCardClaimV1 {
    /// Creates one already validated claim presentation.
    #[must_use]
    pub const fn new(
        claim_id: String,
        kind: ModuleCardClaimKindV1,
        state: ModuleCardClaimStateV1,
        confidence_basis_points: u16,
        evidence_ids: Vec<String>,
    ) -> Self {
        Self {
            claim_id,
            kind,
            state,
            confidence_basis_points,
            evidence_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_fact_serializes_with_independent_kind_and_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = ModuleCardDetailResponseV1::available(ModuleCardDetailV1::new(
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32),
            "44".repeat(32),
            "55".repeat(32),
            "66".repeat(32),
            1,
            1,
            8_000,
            ModuleCardLifecycleV1::Stale {
                invalidated_by_index_run_id: "11".repeat(32),
                reason: ModuleCardFreshnessReasonV1::EvidenceChanged,
            },
            vec![ModuleCardDetailFieldV1::new(
                ModuleCardFieldKindV1::PublicSurface,
                vec!["77".repeat(32)],
                vec![ModuleCardValueV1::new(
                    "exports main".to_owned(),
                    ModuleCardClaimV1::new(
                        "88".repeat(32),
                        ModuleCardClaimKindV1::Fact,
                        ModuleCardClaimStateV1::Stale,
                        7_000,
                        vec!["77".repeat(32)],
                    ),
                )],
            )],
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["detail"]["lifecycle"]["status"], "stale");
        assert_eq!(
            value["result"]["detail"]["fields"][0]["values"][0]["claim"]["kind"],
            "fact"
        );
        assert_eq!(
            value["result"]["detail"]["fields"][0]["values"][0]["claim"]["state"],
            "stale"
        );
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let value = serde_json::json!({
            "protocolVersion": 1,
            "moduleId": "11".repeat(32),
            "path": "C:/private"
        });
        assert!(serde_json::from_value::<QueryModuleCardDetailRequestV1>(value).is_err());
    }
}
