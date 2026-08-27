use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict pathless status input for the Core-owned Deep-Map lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryDeepMapRequestV1 {
    /// Creates a status request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a status request for the current protocol version.
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

/// Strict explicit-start input; the WebView supplies budgets but no path, profile, or job ID.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartDeepMapRequestV1 {
    protocol_version: ProtocolVersion,
    budget: DeepMapBudgetV1,
}

impl StartDeepMapRequestV1 {
    /// Creates an explicit-start request with a visible hard budget.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, budget: DeepMapBudgetV1) -> Self {
        Self {
            protocol_version,
            budget,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the requested token, time, and tool limits.
    #[must_use]
    pub const fn budget(self) -> DeepMapBudgetV1 {
        self.budget
    }
}

/// Shared strict pathless input for pause, resume, and cancel commands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlDeepMapRequestV1 {
    protocol_version: ProtocolVersion,
}

impl ControlDeepMapRequestV1 {
    /// Creates a lifecycle-control request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a lifecycle-control request for the current protocol version.
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

/// Hard token, wall-time, and read-only-tool budget selected before model execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapBudgetV1 {
    token_limit: u32,
    time_limit_millis: u64,
    tool_call_limit: u16,
}

impl DeepMapBudgetV1 {
    /// Groups all three hard exploration dimensions.
    #[must_use]
    pub const fn new(token_limit: u32, time_limit_millis: u64, tool_call_limit: u16) -> Self {
        Self {
            token_limit,
            time_limit_millis,
            tool_call_limit,
        }
    }

    /// Returns the cumulative model-token limit.
    #[must_use]
    pub const fn token_limit(self) -> u32 {
        self.token_limit
    }

    /// Returns the wall-time limit in milliseconds.
    #[must_use]
    pub const fn time_limit_millis(self) -> u64 {
        self.time_limit_millis
    }

    /// Returns the read-only tool-call limit.
    #[must_use]
    pub const fn tool_call_limit(self) -> u16 {
        self.tool_call_limit
    }
}

/// Complete bounded Deep-Map status selected from Core-owned project and model state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapStatusResponseV1 {
    protocol_version: ProtocolVersion,
    result: DeepMapStatusResultV1,
}

impl DeepMapStatusResponseV1 {
    /// Creates the status returned before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV1::NoProject,
        }
    }

    /// Creates the status returned when no verified mapping executor is configured.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV1::Unavailable,
        }
    }

    /// Creates the complete configured-model and lifecycle response.
    #[must_use]
    pub fn available(configuration: DeepMapConfigurationV1, activity: DeepMapActivityV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV1::Available {
                configuration: Box::new(configuration),
                activity: Box::new(activity),
            },
        }
    }

    /// Returns the mutually exclusive availability result.
    #[must_use]
    pub const fn result(&self) -> &DeepMapStatusResultV1 {
        &self.result
    }
}

/// Absence, safe unavailability, or complete pre-start configuration and lifecycle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum DeepMapStatusResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// No live-verified local mapping executor is configured.
    Unavailable,
    /// Mapping can be started deliberately with the supplied configuration.
    Available {
        /// Verified model and fixed budget envelope.
        configuration: Box<DeepMapConfigurationV1>,
        /// Current Core-owned lifecycle state.
        activity: Box<DeepMapActivityV1>,
    },
}

/// Verified model and fixed budget envelope shown before the explicit start action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapConfigurationV1 {
    model: DeepMapModelV1,
    minimum_budget: DeepMapBudgetV1,
    default_budget: DeepMapBudgetV1,
    maximum_budget: DeepMapBudgetV1,
}

impl DeepMapConfigurationV1 {
    /// Groups the selected verified model with minimum, default, and maximum budgets.
    #[must_use]
    pub const fn new(
        model: DeepMapModelV1,
        minimum_budget: DeepMapBudgetV1,
        default_budget: DeepMapBudgetV1,
        maximum_budget: DeepMapBudgetV1,
    ) -> Self {
        Self {
            model,
            minimum_budget,
            default_budget,
            maximum_budget,
        }
    }
}

/// Content-free profile identity and effective model limits; no endpoint or credential data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapModelV1 {
    profile_id: String,
    profile_version: u16,
    provider_id: String,
    model_id: String,
    context_tokens: u32,
    output_tokens: u32,
}

impl DeepMapModelV1 {
    /// Creates a safe model projection without endpoint or credential data.
    #[must_use]
    pub const fn new(
        profile_id: String,
        profile_version: u16,
        provider_id: String,
        model_id: String,
        context_tokens: u32,
        output_tokens: u32,
    ) -> Self {
        Self {
            profile_id,
            profile_version,
            provider_id,
            model_id,
            context_tokens,
            output_tokens,
        }
    }
}

/// In-memory product lifecycle layered over scheduler-owned attempts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapActivityV1 {
    state: DeepMapActivityStateV1,
    budget: Option<DeepMapBudgetV1>,
    progress: Option<DeepMapProgressV1>,
    failure: Option<DeepMapFailureV1>,
    confirmed_steps: String,
    total_steps: String,
}

impl DeepMapActivityV1 {
    /// Creates one bounded lifecycle snapshot.
    #[must_use]
    pub const fn new(
        state: DeepMapActivityStateV1,
        budget: Option<DeepMapBudgetV1>,
        progress: Option<DeepMapProgressV1>,
        failure: Option<DeepMapFailureV1>,
        confirmed_steps: String,
        total_steps: String,
    ) -> Self {
        Self {
            state,
            budget,
            progress,
            failure,
            confirmed_steps,
            total_steps,
        }
    }
}

/// Stable content-free failure category suitable for user recovery guidance.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapFailureV1 {
    /// No complete Fast-Index publication exists yet.
    NoPublishedIndex,
    /// The source snapshot changed across a retained lifecycle boundary.
    StaleSnapshot,
    /// Deterministic planning could not produce a valid bounded run.
    Planning,
    /// The configured provider could not be reached or ended unexpectedly.
    ModelUnavailable,
    /// The provider rejected the bounded structured request.
    ModelRejected,
    /// A complete structured model answer exceeded its deadline.
    ModelTimedOut,
    /// The provider stream or structured answer was invalid.
    InvalidModelResponse,
    /// A bounded published-index read failed.
    Read,
    /// Evidence or claim verification failed closed.
    Verification,
    /// Verified Module Cards could not be published atomically.
    Publication,
    /// Retained progress contradicted its immutable plan.
    InvalidCheckpoint,
    /// Scheduler progress could not be reconciled safely.
    ProgressUnavailable,
}

/// User-visible session state; Paused exists above the terminal scheduler state machine.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapActivityStateV1 {
    /// No mapping attempt has been requested.
    Idle,
    /// The explicit attempt is waiting for an owned worker.
    Queued,
    /// The owned worker is executing the mapping pipeline.
    Running,
    /// Cooperative cancellation is retaining a checkpoint for resume.
    Pausing,
    /// A validated checkpoint is retained and no model work is running.
    Paused,
    /// Cooperative cancellation will discard any returned checkpoint.
    Cancelling,
    /// The complete mapping attempt succeeded.
    Succeeded,
    /// The attempt failed without claiming completion.
    Failed,
    /// The attempt or paused checkpoint was deliberately cancelled.
    Cancelled,
}

/// Safe phase of the complete Deep-Map pipeline.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapPhaseV2 {
    /// Deterministic plan construction.
    Planning,
    /// Bounded evidence exploration.
    Exploring,
    /// Evidence-bound claim generation.
    Claiming,
    /// Independent evidence verification.
    Verifying,
    /// Atomic publication of verified cards.
    Publishing,
}

/// Coarse target category without source content or path data.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapTargetKindV2 {
    /// The complete current project publication.
    Project,
    /// One stable module identity.
    Module,
    /// One current manifest revision.
    Manifest,
    /// One current symbol identity.
    Symbol,
}

/// Safe action category suitable for user-visible activity reporting.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapSafeActionV2 {
    /// Builds the immutable exploration plan.
    BuildPlan,
    /// Reads one exact evidence target.
    Inspect,
    /// Runs one bounded published-index search.
    Search,
    /// Confirms one evidence-bound module proposal.
    Propose,
    /// Generates structured claims for a module proposal.
    GenerateClaims,
    /// Revalidates evidence and claims.
    VerifyEvidence,
    /// Atomically publishes verified cards.
    PublishCards,
}

/// One monotonically sequenced, content-free event from the retained ring buffer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapEventV2 {
    sequence: String,
    phase: DeepMapPhaseV2,
    current_module_id: Option<String>,
    target_kind: DeepMapTargetKindV2,
    safe_action: DeepMapSafeActionV2,
    step_position: Option<String>,
    total_steps: Option<String>,
    confirmed: bool,
}

impl DeepMapEventV2 {
    /// Creates one already-sanitized activity event.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        sequence: String,
        phase: DeepMapPhaseV2,
        current_module_id: Option<String>,
        target_kind: DeepMapTargetKindV2,
        safe_action: DeepMapSafeActionV2,
        step_position: Option<String>,
        total_steps: Option<String>,
        confirmed: bool,
    ) -> Self {
        Self {
            sequence,
            phase,
            current_module_id,
            target_kind,
            safe_action,
            step_position,
            total_steps,
            confirmed,
        }
    }
}

/// Terminal publication summary containing no generated content.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapPublicationSummaryV2 {
    atomically_published: bool,
}

impl DeepMapPublicationSummaryV2 {
    /// Reports that verification completed and the replacement publish committed atomically.
    #[must_use]
    pub const fn succeeded() -> Self {
        Self {
            atomically_published: true,
        }
    }
}

/// Bounded V2 activity snapshot with live pipeline position and at most 32 events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapActivityV2 {
    state: DeepMapActivityStateV1,
    budget: Option<DeepMapBudgetV1>,
    progress: Option<DeepMapProgressV1>,
    failure: Option<DeepMapFailureV1>,
    confirmed_steps: String,
    total_steps: String,
    phase: Option<DeepMapPhaseV2>,
    current_module_id: Option<String>,
    target_kind: Option<DeepMapTargetKindV2>,
    safe_action: Option<DeepMapSafeActionV2>,
    step_position: Option<String>,
    events: Vec<DeepMapEventV2>,
    publication_summary: Option<DeepMapPublicationSummaryV2>,
}

impl DeepMapActivityV2 {
    /// Creates one complete bounded activity projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        state: DeepMapActivityStateV1,
        budget: Option<DeepMapBudgetV1>,
        progress: Option<DeepMapProgressV1>,
        failure: Option<DeepMapFailureV1>,
        confirmed_steps: String,
        total_steps: String,
        phase: Option<DeepMapPhaseV2>,
        current_module_id: Option<String>,
        target_kind: Option<DeepMapTargetKindV2>,
        safe_action: Option<DeepMapSafeActionV2>,
        step_position: Option<String>,
        events: Vec<DeepMapEventV2>,
        publication_summary: Option<DeepMapPublicationSummaryV2>,
    ) -> Self {
        debug_assert!(events.len() <= 32);
        Self {
            state,
            budget,
            progress,
            failure,
            confirmed_steps,
            total_steps,
            phase,
            current_module_id,
            target_kind,
            safe_action,
            step_position,
            events,
            publication_summary,
        }
    }
}

/// Complete V2 status response for safe live activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapStatusResponseV2 {
    protocol_version: ProtocolVersion,
    result: DeepMapStatusResultV2,
}

impl DeepMapStatusResponseV2 {
    /// Creates the response before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV2::NoProject,
        }
    }

    /// Creates the response when no verified mapping executor is configured.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV2::Unavailable,
        }
    }

    /// Creates a configured response with one bounded activity snapshot.
    #[must_use]
    pub fn available(configuration: DeepMapConfigurationV1, activity: DeepMapActivityV2) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV2::Available {
                configuration: Box::new(configuration),
                activity: Box::new(activity),
            },
        }
    }

    /// Returns the mutually exclusive V2 availability result.
    #[must_use]
    pub const fn result(&self) -> &DeepMapStatusResultV2 {
        &self.result
    }
}

/// V2 availability result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum DeepMapStatusResultV2 {
    /// No project is active.
    NoProject,
    /// No live-verified local mapping executor is configured.
    Unavailable,
    /// Mapping configuration and current safe activity are available.
    Available {
        /// Verified model and validated budget envelope.
        configuration: Box<DeepMapConfigurationV1>,
        /// Bounded in-memory live activity.
        activity: Box<DeepMapActivityV2>,
    },
}

/// Monotone scheduler progress, encoded losslessly for the WebView.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapProgressV1 {
    completed: String,
    total: String,
}

impl DeepMapProgressV1 {
    /// Creates a determinate lossless progress pair.
    #[must_use]
    pub const fn new(completed: String, total: String) -> Self {
        Self { completed, total }
    }
}

/// Stable acknowledgement emitted only after the Core accepted a lifecycle transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapControlResponseV1 {
    protocol_version: ProtocolVersion,
    accepted: bool,
}

impl DeepMapControlResponseV1 {
    /// Creates the acknowledgement for an accepted transition.
    #[must_use]
    pub const fn accepted() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            accepted: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeepMapActivityStateV1, DeepMapActivityV1, DeepMapBudgetV1, DeepMapConfigurationV1,
        DeepMapFailureV1, DeepMapModelV1, DeepMapStatusResponseV1,
    };
    use serde_json::json;

    #[test]
    fn available_status_exposes_verified_model_and_budgets_before_start()
    -> Result<(), serde_json::Error> {
        let response = DeepMapStatusResponseV1::available(
            DeepMapConfigurationV1::new(
                DeepMapModelV1::new(
                    "11".repeat(32),
                    1,
                    "ollama".to_owned(),
                    "mapper".to_owned(),
                    16_384,
                    2_048,
                ),
                DeepMapBudgetV1::new(1, 1, 1),
                DeepMapBudgetV1::new(32_000, 120_000, 64),
                DeepMapBudgetV1::new(1_000_000, 86_400_000, 4_096),
            ),
            DeepMapActivityV1::new(
                DeepMapActivityStateV1::Idle,
                None,
                None,
                None,
                "0".to_owned(),
                "0".to_owned(),
            ),
        );

        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["status"], json!("available"));
        assert_eq!(
            value["result"]["configuration"]["model"]["modelId"],
            json!("mapper")
        );
        assert_eq!(
            value["result"]["configuration"]["defaultBudget"]["tokenLimit"],
            json!(32_000)
        );
        assert_eq!(value["result"]["activity"]["state"], json!("idle"));
        assert_eq!(value["result"]["activity"]["failure"], json!(null));
        Ok(())
    }

    #[test]
    fn failed_activity_exposes_only_a_closed_content_free_reason() -> Result<(), serde_json::Error>
    {
        let activity = DeepMapActivityV1::new(
            DeepMapActivityStateV1::Failed,
            Some(DeepMapBudgetV1::new(32_000, 120_000, 64)),
            None,
            Some(DeepMapFailureV1::ModelTimedOut),
            "0".to_owned(),
            "0".to_owned(),
        );

        let value = serde_json::to_value(activity)?;
        assert_eq!(value["failure"], json!("modelTimedOut"));
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(6));
        Ok(())
    }
}
