use crate::{DeepMapFailureV3, DeepMapSafeActionV2, DeepMapTargetKindV2, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Reads the user-facing dashboard for one Core-issued Deep-Map run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapRunDashboardRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
}

impl QueryDeepMapRunDashboardRequestV1 {
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the project-bound run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
}

/// Reads one page of module summaries for a selected run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapRunModulesRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
    cursor: Option<String>,
}

impl QueryDeepMapRunModulesRequestV1 {
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the project-bound run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
    /// Returns the optional project- and run-bound cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Reads one page of understandable exploration targets for a selected module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapModuleStepsRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
    module_selection: String,
    cursor: Option<String>,
}

impl QueryDeepMapModuleStepsRequestV1 {
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the project-bound run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
    /// Returns the project- and run-bound module selection.
    #[must_use]
    pub fn module_selection(&self) -> &str {
        &self.module_selection
    }
    /// Returns the optional project-, run-, and module-bound cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Reads one page of exact current Atlas effects for a selected module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapAtlasImpactRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
    module_selection: String,
    cursor: Option<String>,
}

impl QueryDeepMapAtlasImpactRequestV1 {
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the project-bound run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
    /// Returns the project- and run-bound module selection.
    #[must_use]
    pub fn module_selection(&self) -> &str {
        &self.module_selection
    }
    /// Returns the optional project-, run-, and module-bound cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Overall user-facing state of a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapDashboardStateV1 {
    /// Waiting to start.
    Queued,
    /// Work is progressing.
    Running,
    /// Finishing the current safe unit before pausing.
    Pausing,
    /// A resumable checkpoint is retained.
    Paused,
    /// Cooperative cancellation is completing.
    Cancelling,
    /// Verified cards were published.
    Completed,
    /// The Atlas already contained the current mapping.
    AlreadyCurrent,
    /// Deliberately cancelled.
    Cancelled,
    /// Failed with safe help text.
    Failed,
    /// Interrupted by a prior process exit.
    Interrupted,
}

/// Relationship between a run and the latest published index.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapDashboardFreshnessV1 {
    /// Current cards and Atlas projections can safely be joined.
    Current,
    /// The run describes an older project state.
    Historical,
}

/// Five stable user-facing Deep-Map phases.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapDashboardPhaseV1 {
    /// Plan exploration.
    Planning,
    /// Explore targets.
    Exploring,
    /// Assemble Module Cards.
    CreatingCards,
    /// Verify claims and evidence.
    Verifying,
    /// Publish into the Atlas.
    UpdatingAtlas,
}

/// State of one phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapDashboardPhaseStateV1 {
    /// Not reached.
    Pending,
    /// Currently active.
    Active,
    /// Passed successfully.
    Completed,
    /// The run stopped here.
    Stopped,
}

/// One phase in the stable product stepper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapDashboardPhaseProgressV1 {
    /// Product phase.
    pub phase: DeepMapDashboardPhaseV1,
    /// Core-derived state.
    pub state: DeepMapDashboardPhaseStateV1,
}

/// Why the deterministic planner selected an exploration target.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapSelectionReasonV1 {
    /// Package metadata establishes dependencies and boundaries.
    Manifest,
    /// An execution entry reveals public behavior and flows.
    Entrypoint,
    /// A central symbol reveals core responsibilities.
    CentralSymbol,
    /// A test root reveals rules and risks.
    TestRoot,
    /// Strong graph coupling reveals an architectural area.
    GraphCommunity,
    /// Mandatory Card fields still lacked evidence.
    UncoveredModule,
}

/// Canonical Card field without values or internal metadata.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapCardFieldV1 {
    /// Human-readable title.
    Title,
    /// Owned paths.
    Paths,
    /// Reason the module exists.
    Purpose,
    /// Owned behavior.
    Responsibilities,
    /// Public interfaces.
    PublicSurface,
    /// Entrypoints.
    Entrypoints,
    /// Dependencies.
    Dependencies,
    /// Data movement.
    DataFlows,
    /// Rules that must hold.
    Invariants,
    /// Tests and test roots.
    Tests,
    /// Failure modes and maintenance risks.
    Risks,
    /// Explicit unknowns.
    OpenQuestions,
}

/// Current work resolved to safe display labels from the run's index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapCurrentActivityV1 {
    /// Current product phase.
    pub phase: DeepMapDashboardPhaseV1,
    /// Closed action category.
    pub action: Option<DeepMapSafeActionV2>,
    /// Closed target category.
    pub target_kind: Option<DeepMapTargetKindV2>,
    /// Safe module display name.
    pub module_name: Option<String>,
    /// Safe file, symbol, or module display label.
    pub target_label: Option<String>,
    /// Why the target was selected.
    pub selection_reason: Option<DeepMapSelectionReasonV1>,
    /// Card information expected from this target.
    pub card_fields: Vec<DeepMapCardFieldV1>,
}

/// Safe actionable failure explanation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapDashboardFailureV1 {
    /// Closed diagnostic class.
    pub cause: DeepMapFailureV3,
    /// Whether already confirmed steps remain represented in the journal.
    pub confirmed_work_retained: bool,
    /// Closed optional short code for support situations.
    pub diagnostic_code: Option<String>,
}

/// Complete lightweight run dashboard; Card bodies remain separate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapRunDashboardResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Project-bound run selection echoed by the Core.
    pub run_selection: String,
    /// Overall product state.
    pub state: DeepMapDashboardStateV1,
    /// Whether current content may be joined.
    pub freshness: DeepMapDashboardFreshnessV1,
    /// Exactly five phases in stable order.
    pub phases: Vec<DeepMapDashboardPhaseProgressV1>,
    /// Lossless confirmed-step count.
    pub confirmed_steps: String,
    /// Lossless planned-step count.
    pub total_steps: String,
    /// Lossless local start timestamp.
    pub started_at_unix_millis: String,
    /// Lossless latest update timestamp.
    pub updated_at_unix_millis: String,
    /// Current safe activity, when the journal identifies one.
    pub current_activity: Option<DeepMapCurrentActivityV1>,
    /// Actionable safe failure data.
    pub failure: Option<DeepMapDashboardFailureV1>,
    /// Whether the journal lost non-critical details.
    pub details_incomplete: bool,
    /// Whether exact V29 plan targets are absent for this older run.
    pub historical_plan_limited: bool,
}

/// Product state of one planned module.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapModuleStateV1 {
    /// Planned but not reached.
    Planned,
    /// Targets are being explored.
    Exploring,
    /// Claims are being verified.
    Verifying,
    /// A current verified Card is available.
    Published,
    /// The run stopped without a current complete Card.
    Incomplete,
}

/// One module summary without exposing its stable internal ID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapRunModuleV1 {
    /// Core-issued project- and run-bound module selection.
    pub selection: String,
    /// Safe current or historical display name.
    pub display_name: String,
    /// Core-derived product state.
    pub state: DeepMapModuleStateV1,
    /// Lossless planned target count.
    pub planned_steps: String,
    /// Lossless confirmed target count.
    pub confirmed_steps: String,
    /// Whether a current verified Card can be expanded.
    pub card_available: bool,
}

/// Bounded page of at most twenty module summaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapRunModulesResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Modules in canonical order.
    pub modules: Vec<DeepMapRunModuleV1>,
    /// Core-issued cursor for the next page.
    pub next_cursor: Option<String>,
}

/// State of one exploration target.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapPlanStepStateV1 {
    /// Not reached.
    Planned,
    /// Currently being explored.
    Exploring,
    /// Confirmed by evidence.
    Confirmed,
}

/// One resolved safe exploration target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapModuleStepV1 {
    /// Lossless one-based planner position.
    pub position: String,
    /// Closed target category.
    pub target_kind: DeepMapTargetKindV2,
    /// Safe current display label, absent when historical resolution is impossible.
    pub target_label: Option<String>,
    /// Why this target matters.
    pub selection_reason: DeepMapSelectionReasonV1,
    /// Card fields expected from this target, absent for pre-V29 history.
    pub card_fields: Option<Vec<DeepMapCardFieldV1>>,
    /// Core-derived target state.
    pub state: DeepMapPlanStepStateV1,
}

/// Bounded page of at most fifty module steps.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapModuleStepsResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Safe planner targets.
    pub steps: Vec<DeepMapModuleStepV1>,
    /// Core-issued cursor for the next page.
    pub next_cursor: Option<String>,
    /// Whether this page contains legacy steps without exact V29 details.
    pub historical_details_limited: bool,
}

/// Exact current Atlas target class enriched by verified evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapAtlasImpactKindV1 {
    /// Current file revision.
    File,
    /// Current structural symbol.
    Symbol,
    /// Current deterministic relation.
    Relation,
}

/// One safe Atlas impact with an exact claim count.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapAtlasImpactItemV1 {
    /// Atlas entity class.
    pub kind: DeepMapAtlasImpactKindV1,
    /// Safe path, symbol, or relation label.
    pub label: String,
    /// Number of current verified claims attached here.
    pub confirmed_claim_count: String,
}

/// Summary of current verified information projected into the Atlas.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapAtlasImpactSummaryV1 {
    /// Current verified purpose, when present.
    pub purpose: Option<String>,
    /// Number of current verified visible risks.
    pub risk_count: String,
    /// Number of exact current file hints.
    pub file_count: String,
    /// Number of exact current symbol hints.
    pub symbol_count: String,
    /// Number of exact current relation hints.
    pub relation_count: String,
}

/// Current Atlas impact, historical explanation, or missing-card result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum DeepMapAtlasImpactResultV1 {
    /// This run cannot be joined to today's Atlas.
    Historical,
    /// No current verified published Card is available.
    CardUnavailable,
    /// Current exact Atlas effects are available.
    Available {
        /// Aggregated current Card effect.
        summary: DeepMapAtlasImpactSummaryV1,
        /// Bounded exact hints.
        items: Vec<DeepMapAtlasImpactItemV1>,
        /// Core-issued cursor for the next page.
        #[serde(rename = "nextCursor")]
        next_cursor: Option<String>,
    },
}

/// Atlas impact response envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapAtlasImpactResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Safe bounded result.
    pub result: DeepMapAtlasImpactResultV1,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_requests_reject_unknown_fields() {
        let payload = serde_json::json!({
            "protocolVersion": 1,
            "runSelection": "selection",
            "indexSnapshotId": "must-not-be-accepted"
        });

        assert!(serde_json::from_value::<QueryDeepMapRunDashboardRequestV1>(payload).is_err());
    }

    #[test]
    fn atlas_impact_serializes_its_variant_cursor_in_camel_case()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = DeepMapAtlasImpactResponseV1 {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapAtlasImpactResultV1::Available {
                summary: DeepMapAtlasImpactSummaryV1 {
                    purpose: None,
                    risk_count: "0".to_owned(),
                    file_count: "0".to_owned(),
                    symbol_count: "0".to_owned(),
                    relation_count: "0".to_owned(),
                },
                items: Vec::new(),
                next_cursor: Some("cursor".to_owned()),
            },
        };

        let payload = serde_json::to_value(response)?;
        assert_eq!(payload["result"]["nextCursor"], "cursor");
        assert!(payload["result"].get("next_cursor").is_none());
        Ok(())
    }

    #[test]
    fn dashboard_response_shape_contains_no_provider_budget_or_internal_identity_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = DeepMapRunDashboardResponseV1 {
            protocol_version: ProtocolVersion::V1,
            run_selection: "opaque-run-selection".to_owned(),
            state: DeepMapDashboardStateV1::Running,
            freshness: DeepMapDashboardFreshnessV1::Current,
            phases: vec![
                phase(
                    DeepMapDashboardPhaseV1::Planning,
                    DeepMapDashboardPhaseStateV1::Completed,
                ),
                phase(
                    DeepMapDashboardPhaseV1::Exploring,
                    DeepMapDashboardPhaseStateV1::Active,
                ),
                phase(
                    DeepMapDashboardPhaseV1::CreatingCards,
                    DeepMapDashboardPhaseStateV1::Pending,
                ),
                phase(
                    DeepMapDashboardPhaseV1::Verifying,
                    DeepMapDashboardPhaseStateV1::Pending,
                ),
                phase(
                    DeepMapDashboardPhaseV1::UpdatingAtlas,
                    DeepMapDashboardPhaseStateV1::Pending,
                ),
            ],
            confirmed_steps: "1".to_owned(),
            total_steps: "3".to_owned(),
            started_at_unix_millis: "1000".to_owned(),
            updated_at_unix_millis: "1200".to_owned(),
            current_activity: Some(DeepMapCurrentActivityV1 {
                phase: DeepMapDashboardPhaseV1::Exploring,
                action: Some(DeepMapSafeActionV2::Inspect),
                target_kind: Some(DeepMapTargetKindV2::Symbol),
                module_name: Some("application".to_owned()),
                target_label: Some("RunDeepMap".to_owned()),
                selection_reason: Some(DeepMapSelectionReasonV1::CentralSymbol),
                card_fields: vec![DeepMapCardFieldV1::Purpose],
            }),
            failure: None,
            details_incomplete: false,
            historical_plan_limited: false,
        };

        let encoded = serde_json::to_string(&response)?;
        for forbidden in [
            "provider",
            "modelId",
            "tokenBudget",
            "snapshotId",
            "indexRunId",
            "moduleId",
            "symbolId",
            "evidenceId",
            "confidence",
            "prompt",
            "sourceText",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "forbidden field leaked: {forbidden}"
            );
        }
        Ok(())
    }

    fn phase(
        phase: DeepMapDashboardPhaseV1,
        state: DeepMapDashboardPhaseStateV1,
    ) -> DeepMapDashboardPhaseProgressV1 {
        DeepMapDashboardPhaseProgressV1 { phase, state }
    }
}
