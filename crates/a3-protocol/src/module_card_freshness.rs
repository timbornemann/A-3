use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict pathless input for the current module-card lifecycle projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryModuleCardFreshnessRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryModuleCardFreshnessRequestV1 {
    /// Creates a request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a request for the protocol version emitted by this build.
    #[must_use]
    pub const fn current() -> Self {
        Self::new(ProtocolVersion::CURRENT)
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Versioned lifecycle result selected from the Core-owned active project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardFreshnessResponseV1 {
    protocol_version: ProtocolVersion,
    result: ModuleCardFreshnessResultV1,
}

impl ModuleCardFreshnessResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleCardFreshnessResultV1::NoProject,
        }
    }

    /// Creates the response used before the first atomic publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleCardFreshnessResultV1::NoPublishedIndex,
        }
    }

    /// Creates an available exact freshness projection.
    #[must_use]
    pub fn available(freshness: ModuleCardFreshnessV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ModuleCardFreshnessResultV1::Available {
                freshness: Box::new(freshness),
            },
        }
    }

    /// Returns the mutually exclusive project/publication result.
    #[must_use]
    pub const fn result(&self) -> &ModuleCardFreshnessResultV1 {
        &self.result
    }
}

/// Whether an active project and current atomic publication exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ModuleCardFreshnessResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// A project is active but no index has crossed the publish boundary.
    NoPublishedIndex,
    /// Current lifecycle counts are available.
    Available {
        /// Exact counts and reasons without card contents or paths.
        freshness: Box<ModuleCardFreshnessV1>,
    },
}

/// WebView-safe aggregate of the latest card for every known module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardFreshnessV1 {
    index_run_id: String,
    snapshot_id: String,
    counts: ModuleCardFreshnessCountsV1,
    reasons: Vec<ModuleCardFreshnessReasonCountV1>,
}

impl ModuleCardFreshnessV1 {
    /// Creates a projection from application-validated bounded values.
    #[must_use]
    pub const fn new(
        index_run_id: String,
        snapshot_id: String,
        counts: ModuleCardFreshnessCountsV1,
        reasons: Vec<ModuleCardFreshnessReasonCountV1>,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            counts,
            reasons,
        }
    }

    /// Returns the atomic index publication causing the lifecycle state.
    #[must_use]
    pub fn index_run_id(&self) -> &str {
        &self.index_run_id
    }

    /// Returns the immutable snapshot underlying that publication.
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Returns exact lossless aggregate counts.
    #[must_use]
    pub const fn counts(&self) -> &ModuleCardFreshnessCountsV1 {
        &self.counts
    }

    /// Returns at most five positive reason buckets in canonical order.
    #[must_use]
    pub fn reasons(&self) -> &[ModuleCardFreshnessReasonCountV1] {
        &self.reasons
    }
}

/// Exact latest-card counters represented without JavaScript integer loss.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardFreshnessCountsV1 {
    published_count: String,
    stale_count: String,
    needs_review_count: String,
    total_count: String,
}

impl ModuleCardFreshnessCountsV1 {
    /// Creates exact canonical decimal counters.
    #[must_use]
    pub const fn new(
        published_count: String,
        stale_count: String,
        needs_review_count: String,
        total_count: String,
    ) -> Self {
        Self {
            published_count,
            stale_count,
            needs_review_count,
            total_count,
        }
    }

    /// Returns current published cards.
    #[must_use]
    pub fn published_count(&self) -> &str {
        &self.published_count
    }

    /// Returns stale cards.
    #[must_use]
    pub fn stale_count(&self) -> &str {
        &self.stale_count
    }

    /// Returns cards needing conservative review.
    #[must_use]
    pub fn needs_review_count(&self) -> &str {
        &self.needs_review_count
    }

    /// Returns all latest cards exactly once.
    #[must_use]
    pub fn total_count(&self) -> &str {
        &self.total_count
    }
}

/// User-visible invalid lifecycle category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardFreshnessStatusV1 {
    /// The card's own evidence or tool compatibility changed.
    Stale,
    /// A direct dependency changed.
    NeedsReview,
}

/// Stable auditable cause for one invalid lifecycle bucket.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleCardFreshnessReasonV1 {
    /// Direct evidence changed or disappeared.
    EvidenceChanged,
    /// The deterministic index no longer contains the module.
    ModuleRemoved,
    /// Parser evidence was produced by an incompatible revision.
    ParserVersionChanged,
    /// The mapping profile changed.
    MapperVersionChanged,
    /// A directly depended-on module changed.
    DirectDependencyChanged,
}

/// One positive lifecycle-reason aggregate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModuleCardFreshnessReasonCountV1 {
    status: ModuleCardFreshnessStatusV1,
    reason: ModuleCardFreshnessReasonV1,
    count: String,
}

impl ModuleCardFreshnessReasonCountV1 {
    /// Creates an application-validated reason bucket.
    #[must_use]
    pub const fn new(
        status: ModuleCardFreshnessStatusV1,
        reason: ModuleCardFreshnessReasonV1,
        count: String,
    ) -> Self {
        Self {
            status,
            reason,
            count,
        }
    }

    /// Returns stale or needs-review.
    #[must_use]
    pub const fn status(&self) -> ModuleCardFreshnessStatusV1 {
        self.status
    }

    /// Returns the auditable invalidation reason.
    #[must_use]
    pub const fn reason(&self) -> ModuleCardFreshnessReasonV1 {
        self.reason
    }

    /// Returns the exact positive count.
    #[must_use]
    pub fn count(&self) -> &str {
        &self.count
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModuleCardFreshnessCountsV1, ModuleCardFreshnessReasonCountV1, ModuleCardFreshnessReasonV1,
        ModuleCardFreshnessResponseV1, ModuleCardFreshnessStatusV1, ModuleCardFreshnessV1,
        QueryModuleCardFreshnessRequestV1,
    };

    #[test]
    fn available_response_serializes_exact_counts_without_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = ModuleCardFreshnessResponseV1::available(ModuleCardFreshnessV1::new(
            "11".repeat(32),
            "22".repeat(32),
            ModuleCardFreshnessCountsV1::new(
                "7".to_owned(),
                "1".to_owned(),
                "1".to_owned(),
                "9".to_owned(),
            ),
            vec![ModuleCardFreshnessReasonCountV1::new(
                ModuleCardFreshnessStatusV1::NeedsReview,
                ModuleCardFreshnessReasonV1::DirectDependencyChanged,
                "1".to_owned(),
            )],
        ));

        let value = serde_json::to_value(response)?;
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["result"]["status"], "available");
        assert_eq!(value["result"]["freshness"]["counts"]["totalCount"], "9");
        assert_eq!(
            value["result"]["freshness"]["reasons"][0]["status"],
            "needsReview"
        );
        assert!(!value.to_string().contains("path"));
        Ok(())
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let value = serde_json::json!({ "protocolVersion": 1, "projectId": "untrusted" });
        assert!(serde_json::from_value::<QueryModuleCardFreshnessRequestV1>(value).is_err());
    }
}
