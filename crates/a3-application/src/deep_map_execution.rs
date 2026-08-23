use crate::JobContext;
use a3_domain::{
    ExploreBudget, ExplorePlan, ExplorerCheckpoint, ModelProfile, ModelProfileReference,
    ProjectIdentity,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by a complete Deep-Map execution capability.
pub type DeepMapExecutionFuture<'a> = Pin<
    Box<dyn Future<Output = Result<DeepMapExecutionOutcome, DeepMapExecutionFailure>> + Send + 'a>,
>;

/// Safe, content-free description of the live-verified mapping model used by a Deep Map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapModelDescriptor {
    profile: ModelProfileReference,
    provider_id: String,
    model_id: String,
    context_tokens: u32,
    output_tokens: u32,
}

impl DeepMapModelDescriptor {
    /// Projects only a profile that passed the executable structured-output capability probe.
    pub fn from_verified_profile(
        profile: &ModelProfile,
    ) -> Result<Self, DeepMapModelDescriptorError> {
        if !profile.executable_actions_enabled() {
            return Err(DeepMapModelDescriptorError::StructuredOutputNotVerified);
        }
        Ok(Self {
            profile: profile.reference(),
            provider_id: profile.provider_id().as_str().to_owned(),
            model_id: profile.model_id().as_str().to_owned(),
            context_tokens: profile.settings().context_limit().get(),
            output_tokens: profile.settings().output_limit().get(),
        })
    }

    /// Returns the exact immutable model-profile identity.
    #[must_use]
    pub const fn profile(&self) -> ModelProfileReference {
        self.profile
    }

    /// Returns the safe provider identifier without endpoint or credential material.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the opaque provider-native model identifier without inferring capabilities.
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Returns the effective per-request context limit of the verified profile.
    #[must_use]
    pub const fn context_tokens(&self) -> u32 {
        self.context_tokens
    }

    /// Returns the effective per-response output limit of the verified profile.
    #[must_use]
    pub const fn output_tokens(&self) -> u32 {
        self.output_tokens
    }
}

/// A mapping model lacked evidence required for executable structured actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapModelDescriptorError {
    /// The live structured-output probe was not successful.
    StructuredOutputNotVerified,
}

impl fmt::Display for DeepMapModelDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Deep-Map model profile is not verified for structured output")
    }
}

impl Error for DeepMapModelDescriptorError {}

/// Immutable plan and confirmed prefix retained by the Core after a cooperative pause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapResumeState {
    plan: ExplorePlan,
    checkpoint: ExplorerCheckpoint,
    budget: ExploreBudget,
}

impl DeepMapResumeState {
    /// Retains only a checkpoint that belongs to the exact immutable plan.
    pub fn new(
        plan: ExplorePlan,
        checkpoint: ExplorerCheckpoint,
        budget: ExploreBudget,
    ) -> Result<Self, DeepMapExecutionFailure> {
        if plan.budget() != budget {
            return Err(DeepMapExecutionFailure::InvalidCheckpoint);
        }
        checkpoint
            .validate_for(&plan)
            .map_err(|_| DeepMapExecutionFailure::InvalidCheckpoint)?;
        Ok(Self {
            plan,
            checkpoint,
            budget,
        })
    }

    /// Returns the immutable deterministic exploration plan.
    #[must_use]
    pub const fn plan(&self) -> &ExplorePlan {
        &self.plan
    }

    /// Returns the consecutive confirmed proposal prefix.
    #[must_use]
    pub const fn checkpoint(&self) -> &ExplorerCheckpoint {
        &self.checkpoint
    }

    /// Returns the unchanged start-time budget.
    #[must_use]
    pub const fn budget(&self) -> ExploreBudget {
        self.budget
    }

    /// Returns how many plan steps are durably resumable in this process.
    #[must_use]
    pub fn completed_steps(&self) -> usize {
        self.checkpoint.confirmed_step_count()
    }

    /// Returns the fixed number of deterministic plan steps.
    #[must_use]
    pub fn total_steps(&self) -> usize {
        self.plan.steps().len()
    }
}

/// Either a deliberate fresh start or a Core-owned exact-checkpoint continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepMapExecutionRequest {
    /// Plan against the latest atomically published index using the selected hard budget.
    Start {
        /// Token, wall-time, and read-only-tool limits shown before execution.
        budget: ExploreBudget,
    },
    /// Resume the same immutable plan without repeating confirmed steps.
    Resume(Box<DeepMapResumeState>),
}

impl DeepMapExecutionRequest {
    /// Returns the hard budget that must remain unchanged for this run.
    #[must_use]
    pub const fn budget(&self) -> ExploreBudget {
        match self {
            Self::Start { budget } => *budget,
            Self::Resume(state) => state.budget(),
        }
    }
}

/// Terminal result of one scheduler-owned Deep-Map attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepMapExecutionOutcome {
    /// Every step completed; the retained state proves the checkpoint covers the full plan.
    Completed(DeepMapResumeState),
    /// Cooperative cancellation retained a validated prefix suitable for a deliberate resume.
    Cancelled(DeepMapResumeState),
}

impl DeepMapExecutionOutcome {
    /// Constructs a complete result only when every planned step is confirmed.
    pub fn completed(state: DeepMapResumeState) -> Result<Self, DeepMapExecutionFailure> {
        if !state.checkpoint().is_complete_for(state.plan()) {
            return Err(DeepMapExecutionFailure::InvalidCheckpoint);
        }
        Ok(Self::Completed(state))
    }

    /// Constructs a cooperative cancellation result from already validated resumable state.
    #[must_use]
    pub const fn cancelled(state: DeepMapResumeState) -> Self {
        Self::Cancelled(state)
    }

    /// Returns the exact plan and confirmed prefix retained at termination.
    #[must_use]
    pub const fn state(&self) -> &DeepMapResumeState {
        match self {
            Self::Completed(state) | Self::Cancelled(state) => state,
        }
    }
}

/// Application-owned complete Deep-Map capability composed from planner, model, reads, and publish.
pub trait DeepMapExecutor: fmt::Debug + Send + Sync {
    /// Returns the live-verified profile shown before any model work may start.
    fn model(&self) -> &DeepMapModelDescriptor;

    /// Executes one fresh or resumed attempt under the scheduler's owned cancellation boundary.
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: DeepMapExecutionRequest,
        control: &'a JobContext,
    ) -> DeepMapExecutionFuture<'a>;
}

/// Stable complete-run failure without provider payloads, endpoints, source, or storage rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapExecutionFailure {
    /// No complete Fast-Index publication exists for planning.
    NoPublishedIndex,
    /// The published snapshot changed before a resume or publish boundary.
    StaleSnapshot,
    /// The deterministic planner rejected publication, coverage, or budget input.
    Planning,
    /// The configured provider could not be reached or ended its response unexpectedly.
    ModelUnavailable,
    /// The provider rejected the exact bounded structured request.
    ModelRejected,
    /// The complete structured response exceeded its request deadline.
    ModelTimedOut,
    /// The provider stream or decoded structured model output was invalid.
    InvalidModelResponse,
    /// A bounded read-only exploration capability failed.
    Read,
    /// Proposal or claim verification failed.
    Verification,
    /// Verified Module Cards could not be atomically published.
    Publication,
    /// Resume state did not match its immutable plan or completion claim.
    InvalidCheckpoint,
    /// Progress could not reach the owning scheduler.
    ProgressUnavailable,
}

impl fmt::Display for DeepMapExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoPublishedIndex => "Deep Map requires a complete published index",
            Self::StaleSnapshot => "Deep Map snapshot is no longer current",
            Self::Planning => "Deep Map planning failed",
            Self::ModelUnavailable => "Deep Map model provider is unavailable",
            Self::ModelRejected => "Deep Map model request was rejected",
            Self::ModelTimedOut => "Deep Map model response timed out",
            Self::InvalidModelResponse => "Deep Map model response is invalid",
            Self::Read => "Deep Map read-only exploration failed",
            Self::Verification => "Deep Map verification failed",
            Self::Publication => "Deep Map publication failed",
            Self::InvalidCheckpoint => "Deep Map checkpoint is invalid for its plan",
            Self::ProgressUnavailable => "Deep Map progress is unavailable",
        })
    }
}

impl Error for DeepMapExecutionFailure {}

#[cfg(test)]
mod tests {
    use super::{DeepMapExecutionFailure, DeepMapModelDescriptor, DeepMapModelDescriptorError};
    use a3_domain::{
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
    };
    use std::error::Error;

    #[test]
    fn descriptor_accepts_only_live_verified_structured_output() -> Result<(), Box<dyn Error>> {
        let verified = profile(ModelStructuredOutputCapability::Verified)?;
        let descriptor = DeepMapModelDescriptor::from_verified_profile(&verified)?;
        assert_eq!(descriptor.profile(), verified.reference());
        assert_eq!(descriptor.provider_id(), "local");
        assert_eq!(descriptor.model_id(), "mapper");
        assert_eq!(descriptor.context_tokens(), 16_384);
        assert_eq!(descriptor.output_tokens(), 2_048);

        let unverified = profile(ModelStructuredOutputCapability::Unavailable)?;
        assert_eq!(
            DeepMapModelDescriptor::from_verified_profile(&unverified),
            Err(DeepMapModelDescriptorError::StructuredOutputNotVerified)
        );
        Ok(())
    }

    #[test]
    fn failure_messages_do_not_expose_boundary_details() {
        assert_eq!(
            DeepMapExecutionFailure::ModelTimedOut.to_string(),
            "Deep Map model response timed out"
        );
    }

    fn profile(
        structured_output: ModelStructuredOutputCapability,
    ) -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("local".to_owned())?,
            ModelId::try_from_string("mapper".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(2_048)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            ModelCapabilities::new(structured_output, ModelToolCallMode::Disabled),
        ))
    }
}
