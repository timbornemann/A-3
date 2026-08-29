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

/// Closed user-selectable Deep-Map modes; resource limits remain Core-owned.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapModeV2 {
    /// Small fixed envelope for a quick map.
    Fast,
    /// Balanced fixed envelope used by default.
    Standard,
    /// Largest fixed envelope for broad exploration.
    Thorough,
}

/// Strict V2 start input without caller-selected resource limits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartDeepMapRequestV2 {
    protocol_version: ProtocolVersion,
    mode: DeepMapModeV2,
}

impl StartDeepMapRequestV2 {
    /// Creates a strict start request with a Core-owned resource envelope.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, mode: DeepMapModeV2) -> Self {
        Self {
            protocol_version,
            mode,
        }
    }
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the selected closed mode.
    #[must_use]
    pub const fn mode(self) -> DeepMapModeV2 {
        self.mode
    }
}

/// Outcome of one V2 start request after the Core preflight.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapStartOutcomeV2 {
    /// A new run was accepted by the scheduler.
    Queued,
    /// The latest index already has its immutable Module Cards.
    AlreadyCurrent,
}

/// Acknowledgement returned after the Core has completed the start preflight.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapStartResponseV2 {
    protocol_version: ProtocolVersion,
    outcome: DeepMapStartOutcomeV2,
}

impl DeepMapStartResponseV2 {
    /// Creates an acknowledgement for a newly queued run.
    #[must_use]
    pub const fn queued() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            outcome: DeepMapStartOutcomeV2::Queued,
        }
    }
    /// Creates an acknowledgement for an already-current index.
    #[must_use]
    pub const fn already_current() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            outcome: DeepMapStartOutcomeV2::AlreadyCurrent,
        }
    }
}

/// Distinct safe V3 failure code used by status and journal details.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepMapFailureV3 {
    /// No atomically published Fast Index exists.
    NoPublishedIndex,
    /// The index changed while Deep Map was running.
    StaleIndex,
    /// Deterministic plan construction failed.
    Planning,
    /// The selected provider could not be reached or used.
    ModelUnavailable,
    /// The provider rejected the model request.
    ModelRejected,
    /// The provider exceeded the bounded deadline.
    ModelTimeout,
    /// Structured model output was invalid after the bounded repair attempt.
    InvalidModelResponse,
    /// An approved bounded index read failed.
    Read,
    /// Evidence resolution or deterministic verification failed.
    Verification,
    /// Storage rejected a stale or contradictory publication batch.
    PublicationRejected,
    /// Atomic publication storage failed.
    PublicationStorage,
    /// Atomic publication exceeded its deadline.
    PublicationTimeout,
    /// Publication progress could not reach the owning job.
    PublicationProgress,
    /// A retained resume checkpoint was invalid.
    InvalidCheckpoint,
    /// General lifecycle progress could not reach the owning job.
    ProgressUnavailable,
    /// A non-terminal run was reconciled after application restart.
    Interrupted,
}

/// Compact pipeline location rendered in the permanent one-line control.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapCompactProgressV3 {
    confirmed_steps: String,
    total_steps: String,
    phase: Option<DeepMapPhaseV2>,
    action: Option<DeepMapSafeActionV2>,
}

impl DeepMapCompactProgressV3 {
    /// Creates a compact content-free progress projection.
    #[must_use]
    pub const fn new(
        confirmed_steps: String,
        total_steps: String,
        phase: Option<DeepMapPhaseV2>,
        action: Option<DeepMapSafeActionV2>,
    ) -> Self {
        Self {
            confirmed_steps,
            total_steps,
            phase,
            action,
        }
    }
}

/// Compact lifecycle without the obsolete in-memory event feed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "state")]
pub enum DeepMapLifecycleV3 {
    /// The latest index can be mapped.
    Ready,
    /// The latest index already has its immutable Module Cards.
    Current {
        /// Number of Cards consistently found across publication projections.
        card_count: String,
        /// Whether a durable journal exists for the current anchor.
        details_available: bool,
    },
    /// A run was accepted but has not begun work.
    Queued {
        /// Current compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// The worker is executing.
    Running {
        /// Current compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// A cooperative pause was requested.
    Pausing {
        /// Current compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// A verified checkpoint is retained for resume.
    Paused {
        /// Current compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// Cooperative cancellation was requested.
    Cancelling {
        /// Current compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// A run completed successfully before the storage projection refreshed.
    Succeeded {
        /// Final compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// A run terminated with a safe closed diagnosis.
    Failed {
        /// Final compact progress.
        progress: DeepMapCompactProgressV3,
        /// Content-free failure category.
        failure: DeepMapFailureV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
    /// A run was deliberately cancelled.
    Cancelled {
        /// Final compact progress.
        progress: DeepMapCompactProgressV3,
        /// Whether non-critical journal writes were lost.
        details_incomplete: bool,
    },
}

/// V3 status response: selected model plus one compact lifecycle projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapStatusResponseV3 {
    protocol_version: ProtocolVersion,
    result: DeepMapStatusResultV3,
}

impl DeepMapStatusResponseV3 {
    /// Creates the projection used when no project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV3::NoProject,
        }
    }
    /// Creates the projection used when no verified model is configured.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV3::Unavailable,
        }
    }
    /// Creates an available projection for one verified model and lifecycle.
    #[must_use]
    pub fn available(model: DeepMapModelV1, lifecycle: DeepMapLifecycleV3) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: DeepMapStatusResultV3::Available {
                model: Box::new(model),
                lifecycle: Box::new(lifecycle),
            },
        }
    }

    /// Returns the discriminated compact status result.
    #[must_use]
    pub const fn result(&self) -> &DeepMapStatusResultV3 {
        &self.result
    }
}

/// Discriminated result of the compact V3 status query.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum DeepMapStatusResultV3 {
    /// No project is active.
    NoProject,
    /// No verified Deep-Map model is available.
    Unavailable,
    /// Deep Map is available for the selected model.
    Available {
        /// Selected verified provider/model/profile projection.
        model: Box<DeepMapModelV1>,
        /// Compact lifecycle without an event feed.
        lifecycle: Box<DeepMapLifecycleV3>,
    },
}

/// Project-bound opaque cursor and selection request for the newest 20 runs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapRunsRequestV1 {
    protocol_version: ProtocolVersion,
    cursor: Option<String>,
}

impl QueryDeepMapRunsRequestV1 {
    /// Creates a newest-first run-page request.
    #[must_use]
    pub fn new(protocol_version: ProtocolVersion, cursor: Option<String>) -> Self {
        Self {
            protocol_version,
            cursor,
        }
    }
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the optional Core-issued cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Project-bound request for at most 50 chronological entries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapEntriesRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
    cursor: Option<String>,
}

impl QueryDeepMapEntriesRequestV1 {
    /// Creates a journal-page request for one Core-issued run selection.
    #[must_use]
    pub fn new(
        protocol_version: ProtocolVersion,
        run_selection: String,
        cursor: Option<String>,
    ) -> Self {
        Self {
            protocol_version,
            run_selection,
            cursor,
        }
    }
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
    /// Returns the optional Core-issued event cursor.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Exact safe detail request using Core-issued opaque selections.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryDeepMapEntryDetailRequestV1 {
    protocol_version: ProtocolVersion,
    run_selection: String,
    entry_selection: String,
}

impl QueryDeepMapEntryDetailRequestV1 {
    /// Creates an exact detail request from Core-issued selections.
    #[must_use]
    pub fn new(
        protocol_version: ProtocolVersion,
        run_selection: String,
        entry_selection: String,
    ) -> Self {
        Self {
            protocol_version,
            run_selection,
            entry_selection,
        }
    }
    /// Returns the caller protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque run selection.
    #[must_use]
    pub fn run_selection(&self) -> &str {
        &self.run_selection
    }
    /// Returns the opaque event selection.
    #[must_use]
    pub fn entry_selection(&self) -> &str {
        &self.entry_selection
    }
}

/// Safe materialized summary of one durable Deep-Map run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapRunV1 {
    /// Core-issued project-bound run selection.
    pub selection: String,
    /// Fixed-budget mode.
    pub mode: DeepMapModeV2,
    /// Closed materialized run state.
    pub state: String,
    /// Lossless local start timestamp.
    pub started_at_unix_millis: String,
    /// Lossless timestamp of the latest journal update.
    pub updated_at_unix_millis: String,
    /// Lossless confirmed step count.
    pub confirmed_steps: String,
    /// Lossless planned step count.
    pub total_steps: String,
    /// Safe closed diagnosis for a failed run.
    pub failure: Option<DeepMapFailureV3>,
    /// Whether non-critical journal writes were lost.
    pub details_incomplete: bool,
}

/// Newest-first page of at most 20 durable Deep-Map runs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapRunPageResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Bounded newest-first run summaries.
    pub runs: Vec<DeepMapRunV1>,
    /// Core-issued cursor for the next older page.
    pub next_cursor: Option<String>,
}

/// Safe summary of one chronological journal event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapEntryV1 {
    /// Core-issued project- and run-bound event selection.
    pub selection: String,
    /// Lossless monotone event sequence.
    pub sequence: String,
    /// Materialized run state after this event.
    pub state: String,
    /// Lossless local event timestamp.
    pub occurred_at_unix_millis: String,
    /// Safe pipeline phase.
    pub phase: Option<DeepMapPhaseV2>,
    /// Safe action category.
    pub action: Option<DeepMapSafeActionV2>,
    /// Safe target category.
    pub target_kind: Option<DeepMapTargetKindV2>,
    /// Lossless one-based planner step position.
    pub step_position: Option<String>,
    /// Lossless total planner step count.
    pub total_steps: Option<String>,
    /// Whether evidence-backed work was confirmed.
    pub confirmed: bool,
    /// Closed content-free event result.
    pub result: String,
    /// Safe closed diagnosis for a failed event.
    pub failure: Option<DeepMapFailureV3>,
}

/// Chronological page of at most 50 safe journal events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapEntryPageResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Bounded chronological events.
    pub entries: Vec<DeepMapEntryV1>,
    /// Core-issued cursor for the next older page.
    pub next_cursor: Option<String>,
}

/// Safe technical metadata for a planner-produced step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapStepDetailV1 {
    /// Closed target class.
    pub target_kind: DeepMapTargetKindV2,
    /// Closed deterministic seed reason.
    pub seed_reason: String,
    /// Conservative token reservation, not measured provider usage.
    pub reserved_tokens: u32,
    /// Lossless conservative time reservation.
    pub reserved_time_millis: String,
    /// Conservative bounded tool-call reservation.
    pub reserved_tool_calls: u16,
    /// Deterministic information-gain estimate in basis points.
    pub information_gain_basis_points: u16,
    /// Number of Module-Card fields expected to gain evidence.
    pub coverage_field_count: u16,
    /// Safe closed evidence requirement.
    pub evidence_requirement: String,
    /// Safe closed verification method.
    pub verification_method: String,
    /// Whether this step was confirmed.
    pub confirmed: bool,
}

/// Full safe detail for one selected journal entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeepMapEntryDetailResponseV1 {
    /// Current protocol version.
    pub protocol_version: ProtocolVersion,
    /// Materialized run summary.
    pub run: DeepMapRunV1,
    /// Selected safe event.
    pub entry: DeepMapEntryV1,
    /// Lossless duration since the preceding event.
    pub duration_millis: String,
    /// Safe provider identifier.
    pub provider_id: String,
    /// Safe model identifier.
    pub model_id: String,
    /// Opaque model-profile reference.
    pub profile_id: String,
    /// Model-profile schema version.
    pub profile_version: u16,
    /// Fixed token reservation for the selected mode.
    pub token_budget: u32,
    /// Lossless fixed time reservation for the selected mode.
    pub time_budget_millis: String,
    /// Fixed bounded tool-call reservation for the selected mode.
    pub tool_call_budget: u16,
    /// Safe short reference to the immutable index run.
    pub index_reference: String,
    /// Safe short reference to the immutable snapshot.
    pub snapshot_reference: String,
    /// Safe recommended recovery action for a failure.
    pub next_action: Option<String>,
    /// Closed deterministic plan stop reason.
    pub plan_stop_reason: Option<String>,
    /// Closed publication outcome.
    pub publication_result: Option<String>,
    /// Planner metadata when the event corresponds to a numbered step.
    pub step: Option<DeepMapStepDetailV1>,
}

#[cfg(test)]
mod tests {
    use super::{
        DeepMapActivityStateV1, DeepMapActivityV1, DeepMapBudgetV1, DeepMapConfigurationV1,
        DeepMapFailureV1, DeepMapLifecycleV3, DeepMapModeV2, DeepMapModelV1,
        DeepMapStartResponseV2, DeepMapStatusResponseV1, DeepMapStatusResponseV3,
        QueryDeepMapEntriesRequestV1, StartDeepMapRequestV2,
    };
    use crate::ProtocolVersion;
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

    #[test]
    fn v2_start_accepts_only_the_three_closed_modes_and_no_extra_fields()
    -> Result<(), serde_json::Error> {
        for mode in ["fast", "standard", "thorough"] {
            let request: StartDeepMapRequestV2 = serde_json::from_value(json!({
                "protocolVersion": ProtocolVersion::CURRENT.get(),
                "mode": mode,
            }))?;
            assert_eq!(request.protocol_version(), ProtocolVersion::CURRENT);
        }
        assert!(
            serde_json::from_value::<StartDeepMapRequestV2>(json!({
                "protocolVersion": ProtocolVersion::CURRENT.get(),
                "mode": "custom",
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<StartDeepMapRequestV2>(json!({
                "protocolVersion": ProtocolVersion::CURRENT.get(),
                "mode": "standard",
                "tokenBudget": 1,
            }))
            .is_err()
        );
        assert_eq!(
            StartDeepMapRequestV2::new(ProtocolVersion::CURRENT, DeepMapModeV2::Standard).mode(),
            DeepMapModeV2::Standard
        );
        Ok(())
    }

    #[test]
    fn v3_status_contains_no_event_feed_and_current_is_explicit() -> Result<(), serde_json::Error> {
        let response = DeepMapStatusResponseV3::available(
            DeepMapModelV1::new(
                "11".repeat(32),
                1,
                "openai".to_owned(),
                "gpt-5.4".to_owned(),
                128_000,
                16_384,
            ),
            DeepMapLifecycleV3::Current {
                card_count: "3".to_owned(),
                details_available: true,
            },
        );
        let value = serde_json::to_value(response)?;
        assert_eq!(value["result"]["lifecycle"]["state"], json!("current"));
        assert!(!value.to_string().contains("events"));
        assert_eq!(
            serde_json::to_value(DeepMapStartResponseV2::already_current())?["outcome"],
            json!("alreadyCurrent")
        );
        Ok(())
    }

    #[test]
    fn journal_requests_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<QueryDeepMapEntriesRequestV1>(json!({
                "protocolVersion": ProtocolVersion::CURRENT.get(),
                "runSelection": "opaque",
                "cursor": null,
                "projectId": "invented",
            }))
            .is_err()
        );
    }
}
