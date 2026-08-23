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
