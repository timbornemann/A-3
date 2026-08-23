use crate::{
    DecodeExplorerAction, ExplorerActionJsonSchema, ExplorerObservation, JobContext,
    ModelCancellationFuture,
};
use a3_domain::{
    ExplorePlan, ExploreStep, ExploreTarget, ExplorerActionSchemaVersion, IndexRunId,
    ModuleCardField, ModuleCardSchemaVersion, ModuleId, SnapshotId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_RAW_EXPLORER_OUTPUT_BYTES: usize = 65_536;
const MAX_EXPLORER_REQUEST_TIMEOUT_MILLIS: u64 = 120_000;

/// Owned future returned by the object-safe structured explorer model port.
pub type ExplorerModelFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RawExplorerModelOutput, ExplorerModelFailure>> + Send + 'a>>;

/// Cooperative cancellation visible to local model-provider adapters.
pub trait ExplorerModelControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Returns a wakeable future for cancellation during a stalled provider request.
    fn cancelled(&self) -> ModelCancellationFuture<'_>;
}

impl ExplorerModelControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(self.cancellation_token().cancelled())
    }
}

/// Positive per-request deadline enforced by a concrete local provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerModelTimeout(Duration);

impl ExplorerModelTimeout {
    /// Default local structured-generation deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(120));

    /// Creates a timeout capped at two minutes.
    pub fn from_millis(value: u64) -> Result<Self, ExplorerModelTimeoutError> {
        if value == 0 || value > MAX_EXPLORER_REQUEST_TIMEOUT_MILLIS {
            return Err(ExplorerModelTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Explorer model deadline was zero or exceeded the fixed local boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerModelTimeoutError {
    value: u64,
}

impl fmt::Display for ExplorerModelTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "explorer model timeout {} ms must be between 1 and {MAX_EXPLORER_REQUEST_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for ExplorerModelTimeoutError {}

/// Bounded raw structured output before JSON and domain validation.
#[derive(Clone, PartialEq, Eq)]
pub struct RawExplorerModelOutput(String);

impl RawExplorerModelOutput {
    /// Applies the allocation boundary before provider output enters orchestration.
    pub fn new(value: String) -> Result<Self, RawExplorerModelOutputError> {
        if value.is_empty() || value.len() > MAX_RAW_EXPLORER_OUTPUT_BYTES {
            return Err(RawExplorerModelOutputError {
                actual: value.len(),
            });
        }
        Ok(Self(value))
    }

    /// Returns raw bytes only to the strict action decoder.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RawExplorerModelOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawExplorerModelOutput")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Raw provider output was empty or exceeded 64 KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawExplorerModelOutputError {
    actual: usize,
}

impl fmt::Display for RawExplorerModelOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "raw explorer output has {} bytes and violates its boundary",
            self.actual
        )
    }
}

impl Error for RawExplorerModelOutputError {}

/// Content-free reason supplied to the sole optional repair request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerRepairReason {
    /// Raw JSON or strict schema validation failed.
    InvalidStructuredOutput,
    /// A read action was outside the current plan or budget.
    UnauthorizedRead,
    /// Proposal did not match the step, observed evidence, or expected fields.
    InvalidProposal,
}

/// Whether the request is a normal turn or the one permitted repair turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerModelRequestPhase {
    /// Normal structured action request.
    Primary,
    /// Single correction request containing only a safe error category.
    Repair(ExplorerRepairReason),
}

/// Provider-neutral, plan-bound structured generation request.
#[derive(Clone, PartialEq, Eq)]
pub struct ExplorerModelRequest {
    action_schema_version: ExplorerActionSchemaVersion,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    card_schema_version: ModuleCardSchemaVersion,
    step_sequence: u16,
    module_id: ModuleId,
    target: ExploreTarget,
    expected_fields: Vec<ModuleCardField>,
    observation: Option<ExplorerObservation>,
    phase: ExplorerModelRequestPhase,
}

impl ExplorerModelRequest {
    /// Creates one deterministic request from the current immutable plan step.
    #[must_use]
    pub fn for_step(
        plan: &ExplorePlan,
        step: &ExploreStep,
        observation: Option<ExplorerObservation>,
        phase: ExplorerModelRequestPhase,
    ) -> Self {
        Self {
            action_schema_version: ExplorerActionSchemaVersion::V1,
            index_run_id: plan.index_run_id(),
            snapshot_id: plan.snapshot_id(),
            card_schema_version: plan.schema_version(),
            step_sequence: step.sequence(),
            module_id: step.module_id(),
            target: step.target().clone(),
            expected_fields: step.coverage_fields().to_vec(),
            observation,
            phase,
        }
    }

    /// Returns the strict action schema required from the provider.
    #[must_use]
    pub const fn action_schema_version(&self) -> ExplorerActionSchemaVersion {
        self.action_schema_version
    }

    /// Returns the exact versioned JSON Schema required for structured generation.
    #[must_use]
    pub const fn action_json_schema(&self) -> ExplorerActionJsonSchema {
        DecodeExplorerAction::version_one().json_schema()
    }

    /// Returns the immutable published index run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the requested Module Card schema.
    #[must_use]
    pub const fn card_schema_version(&self) -> ModuleCardSchemaVersion {
        self.card_schema_version
    }

    /// Returns the one-based current plan step.
    #[must_use]
    pub const fn step_sequence(&self) -> u16 {
        self.step_sequence
    }

    /// Returns the deterministic module the current step must describe.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the exact plan-owned inspection target.
    #[must_use]
    pub const fn target(&self) -> &ExploreTarget {
        &self.target
    }

    /// Returns fields the proposal must cover for this step.
    #[must_use]
    pub fn expected_fields(&self) -> &[ModuleCardField] {
        &self.expected_fields
    }

    /// Returns the normalized previous read, if a tool was called.
    #[must_use]
    pub const fn observation(&self) -> Option<&ExplorerObservation> {
        self.observation.as_ref()
    }

    /// Returns whether this is the one bounded repair request.
    #[must_use]
    pub const fn phase(&self) -> ExplorerModelRequestPhase {
        self.phase
    }
}

impl fmt::Debug for ExplorerModelRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let target_kind = match &self.target {
            ExploreTarget::Module(_) => "module",
            ExploreTarget::Manifest { .. } => "manifest",
            ExploreTarget::Symbol(_) => "symbol",
        };
        formatter
            .debug_struct("ExplorerModelRequest")
            .field("action_schema_version", &self.action_schema_version)
            .field("index_run_id", &self.index_run_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("card_schema_version", &self.card_schema_version)
            .field("step_sequence", &self.step_sequence)
            .field("module_id", &self.module_id)
            .field("target_kind", &target_kind)
            .field("expected_fields", &self.expected_fields)
            .field("has_observation", &self.observation.is_some())
            .field("phase", &self.phase)
            .finish()
    }
}

/// Neutral local structured-generation capability needed by the R8 explorer.
pub trait ExplorerModelProvider: fmt::Debug + Send + Sync {
    /// Produces exactly one bounded structured action document.
    fn complete<'a>(
        &'a self,
        request: &'a ExplorerModelRequest,
        timeout: ExplorerModelTimeout,
        control: &'a dyn ExplorerModelControl,
    ) -> ExplorerModelFuture<'a>;
}

/// Stable provider failure without endpoints, payloads, or credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerModelFailure {
    /// Configured local provider could not be reached.
    Unavailable,
    /// Provider rejected the bounded request.
    Rejected,
    /// Provider output exceeded the neutral response boundary.
    InvalidResponse,
    /// Provider enforced the request timeout.
    TimedOut,
    /// Provider observed cooperative cancellation.
    Cancelled,
}

impl fmt::Display for ExplorerModelFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "local explorer model provider is unavailable",
            Self::Rejected => "local explorer model provider rejected the request",
            Self::InvalidResponse => "local explorer model provider returned an invalid response",
            Self::TimedOut => "local explorer model provider timed out",
            Self::Cancelled => "local explorer model provider request was cancelled",
        })
    }
}

impl Error for ExplorerModelFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_and_raw_output_are_bounded_and_redacted() {
        assert!(ExplorerModelTimeout::from_millis(0).is_err());
        assert!(ExplorerModelTimeout::from_millis(120_001).is_err());
        let output = RawExplorerModelOutput::new("secret output".to_owned());
        assert!(output.is_ok());
        if let Ok(output) = output {
            assert!(!format!("{output:?}").contains("secret output"));
        }
        assert!(RawExplorerModelOutput::new("x".repeat(65_537)).is_err());
    }
}
