use crate::{
    AgentActionPrimaryOutcome, AgentContextCompileInput, AgentContextCompiler,
    AgentControllerControl, AgentControllerPreflightFailure, AgentReadResult, AgentRecoveryStore,
    AgentRecoveryStoreFailure, AskResearchDecisionNote, ContextCompileControl,
    ContextCompileFailure, DecodeAgentActionTurn, ModelFinishReason, ModelMessageError,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderRequest,
    ModelProviderRequestError, ModelRequestTimeout, ProviderEvent,
};
use a3_domain::{
    AgentAction, AgentInspectAction, AgentRun, AgentRunError, AgentRunTimestamp, AgentSearchAction,
    AgentToolAttemptStatus, AgentTurnActionClass, AgentTurnCharge, AgentTurnRepairUsage,
    ContextDigest, ModelTokenCount, ModelTokenCountError, ProjectIdentity, RunEvent, RunEventCode,
    RunEventId, RunEventOutcome, RunEventPayload, RunEventRedaction, RunEventRedactionSource,
    SnapshotId, TaskStepId, TaskStepStatus, ToolRunId,
};
use futures::StreamExt;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_AGENT_RAW_OUTPUT_BYTES: usize = 64 * 1_024;
const MAX_AGENT_READ_TIMEOUT_MILLIS: u64 = 120_000;

/// Closed read-only subset that can cross the H9 tool capability boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentReadAction {
    /// Bounded deterministic retrieval.
    Search(AgentSearchAction),
    /// Bounded targeted inspection.
    Inspect(AgentInspectAction),
}

/// Positive total deadline for one read-only agent action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentReadTimeout(Duration);

impl AgentReadTimeout {
    /// Default local read deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(30));

    /// Creates a non-zero deadline capped at two minutes.
    pub fn from_millis(value: u64) -> Result<Self, AgentReadTimeoutError> {
        if value == 0 || value > MAX_AGENT_READ_TIMEOUT_MILLIS {
            return Err(AgentReadTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the bounded neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Read deadline was zero or exceeded the fixed local boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentReadTimeoutError {
    value: u64,
}

impl fmt::Display for AgentReadTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "agent read timeout {} ms must be between 1 and {MAX_AGENT_READ_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for AgentReadTimeoutError {}

/// Future returned by the object-safe read-only H9 tool capability.
pub type AgentReadToolsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AgentReadResult, AgentReadToolFailure>> + Send + 'a>>;

/// Narrow tool capability; no patch, process, shell, Git, network, or publish method exists.
pub trait AgentReadTools: fmt::Debug + Send + Sync {
    /// Executes exactly one already validated read action against one immutable snapshot.
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        action: &'a AgentReadAction,
        timeout: AgentReadTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AgentReadToolsFuture<'a>;
}

/// Stable read-only tool boundary failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentReadToolFailure {
    /// Required local index or repository content was unavailable.
    Unavailable,
    /// The read request or result violated a typed boundary.
    InvalidResult,
    /// Central policy denied the read.
    Denied,
    /// The bounded read timed out.
    TimedOut,
    /// Cooperative cancellation interrupted the read.
    Cancelled,
}

impl fmt::Display for AgentReadToolFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "agent read tool is unavailable",
            Self::InvalidResult => "agent read tool returned an invalid result",
            Self::Denied => "agent read tool was denied",
            Self::TimedOut => "agent read tool timed out",
            Self::Cancelled => "agent read tool was cancelled",
        })
    }
}

impl Error for AgentReadToolFailure {}

/// Successful H9 model turn containing one decoded action and at most one read result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTurnExecution {
    action: AgentAction,
    public_note: Option<AskResearchDecisionNote>,
    charge: AgentTurnCharge,
    context_digest: ContextDigest,
    snapshot_id: SnapshotId,
    current_step_id: TaskStepId,
    tool_result: Option<AgentReadResult>,
    observed_model_output_bytes: u64,
}

impl AgentTurnExecution {
    /// Returns the sole strictly decoded model action.
    #[must_use]
    pub const fn action(&self) -> &AgentAction {
        &self.action
    }

    /// Returns the bounded presentation-only work note emitted beside the action.
    #[must_use]
    pub const fn public_note(&self) -> Option<&AskResearchDecisionNote> {
        self.public_note.as_ref()
    }

    /// Returns the complete primary-plus-repair resource charge.
    #[must_use]
    pub const fn charge(&self) -> AgentTurnCharge {
        self.charge
    }

    /// Returns the freshly compiled H7 context identity.
    #[must_use]
    pub const fn context_digest(&self) -> ContextDigest {
        self.context_digest
    }

    /// Returns the immutable snapshot used by context, model, and optional read.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact current Task Ledger step included in context.
    #[must_use]
    pub const fn current_step_id(&self) -> TaskStepId {
        self.current_step_id
    }

    /// Returns the sole normalized read result for Search or Inspect.
    #[must_use]
    pub const fn tool_result(&self) -> Option<&AgentReadResult> {
        self.tool_result.as_ref()
    }

    /// Takes the sole read result so it can be journaled after this turn's model event.
    #[must_use]
    pub fn take_tool_result(&mut self) -> Option<AgentReadResult> {
        self.tool_result.take()
    }
}

/// Invalid or unauthorized turn that yielded no admissible controller continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedAgentTurn {
    charge: AgentTurnCharge,
    reason: AgentTurnRejectionReason,
    snapshot_id: SnapshotId,
    observed_model_output_bytes: u64,
}

impl RejectedAgentTurn {
    /// Returns consumed model resources, including the sole repair when present.
    #[must_use]
    pub const fn charge(self) -> AgentTurnCharge {
        self.charge
    }

    /// Returns why no controller continuation is admissible from this outcome.
    #[must_use]
    pub const fn reason(self) -> AgentTurnRejectionReason {
        self.reason
    }
}

/// Completed model exchange either yielded one safe action or terminally rejected all actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentTurnOutcome {
    /// One action passed strict decoding and controller authorization.
    Executed(Box<AgentTurnExecution>),
    /// No continuation may use an action from this outcome; usage remains chargeable and auditable.
    Rejected(RejectedAgentTurn),
}

impl AgentTurnOutcome {
    /// Records the bounded model exchange and cumulative charge before a later state transition.
    pub fn record(
        &self,
        run: &mut AgentRun,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
    ) -> Result<RunEvent, AgentRunError> {
        let (charge, snapshot_id, code, outcome, observed_bytes) = match self {
            Self::Executed(execution) => (
                execution.charge,
                execution.snapshot_id,
                RunEventCode::ControllerDecision,
                RunEventOutcome::Succeeded,
                execution.observed_model_output_bytes,
            ),
            Self::Rejected(rejected) => {
                let (code, outcome) = match rejected.reason {
                    AgentTurnRejectionReason::InvalidAfterRepair
                    | AgentTurnRejectionReason::IncompleteModelOutput => {
                        (RunEventCode::InvalidModelOutput, RunEventOutcome::Failed)
                    }
                    AgentTurnRejectionReason::StepMismatch => {
                        (RunEventCode::PolicyDecision, RunEventOutcome::Denied)
                    }
                    AgentTurnRejectionReason::CancelledBeforeAction => {
                        (RunEventCode::Cancellation, RunEventOutcome::Cancelled)
                    }
                    AgentTurnRejectionReason::InvalidReadResult => {
                        (RunEventCode::ToolFailure, RunEventOutcome::Failed)
                    }
                };
                (
                    rejected.charge,
                    rejected.snapshot_id,
                    code,
                    outcome,
                    rejected.observed_model_output_bytes,
                )
            }
        };
        run.record_turn(
            event_id,
            RunEventPayload::new(
                code,
                Some(outcome),
                Some(RunEventRedaction::new(
                    RunEventRedactionSource::ModelOutput,
                    observed_bytes,
                    false,
                )),
            ),
            snapshot_id,
            observed_at,
            charge,
        )
    }
}

/// Content-free reason a completed turn cannot continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTurnRejectionReason {
    /// Primary output was invalid and the sole corrected output remained invalid.
    InvalidAfterRepair,
    /// Provider did not report a normal stop, so potentially incomplete JSON was never decoded.
    IncompleteModelOutput,
    /// A Ledger update named a step other than the current anchored step.
    StepMismatch,
    /// Cancellation arrived after generation and before action execution.
    CancelledBeforeAction,
    /// A read result did not preserve the immutable context snapshot.
    InvalidReadResult,
}

/// Turn use case composing fresh context, neutral provider, strict decoding, and the bounded read
/// capability. Mutating actions are returned to the E7 controller and never executed here.
#[derive(Debug, Clone, Copy)]
pub struct ExecuteAgentTurn<'a> {
    compiler: &'a dyn AgentContextCompiler,
    provider: &'a dyn ModelProvider,
    tools: &'a dyn AgentReadTools,
    recovery: &'a dyn AgentRecoveryStore,
    model_timeout: ModelRequestTimeout,
    read_timeout: AgentReadTimeout,
}

impl<'a> ExecuteAgentTurn<'a> {
    /// Creates the H9 executor with fixed local model and read deadlines.
    #[must_use]
    pub const fn new(
        compiler: &'a dyn AgentContextCompiler,
        provider: &'a dyn ModelProvider,
        tools: &'a dyn AgentReadTools,
        recovery: &'a dyn AgentRecoveryStore,
    ) -> Self {
        Self {
            compiler,
            provider,
            tools,
            recovery,
            model_timeout: ModelRequestTimeout::DEFAULT,
            read_timeout: AgentReadTimeout::DEFAULT,
        }
    }

    /// Compiles a fresh turn, executes at most one read, and returns mutations unexecuted.
    pub async fn execute<C>(
        self,
        run: &AgentRun,
        input: &AgentContextCompileInput,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<AgentTurnOutcome, ExecuteAgentTurnFailure>
    where
        C: AgentControllerControl + ContextCompileControl + ModelOperationControl,
    {
        crate::AdvanceAgentController.preflight(
            run,
            observed_at,
            AgentControllerControl::is_cancelled(control),
        )?;
        validate_turn_input(run, input)?;
        let compiled = self.compiler.compile(input, control).await?;
        if AgentControllerControl::is_cancelled(control) {
            return Err(ExecuteAgentTurnFailure::Cancelled);
        }
        if compiled.goal_contract() != run.goal_contract()
            || compiled.ledger_revision() != run.task_ledger_revision()
            || compiled.current_step_id() != input.current_step_id()
            || compiled.snapshot_id() != run.current_snapshot_id()
        {
            return Err(ExecuteAgentTurnFailure::ContextMismatch);
        }
        let context_digest = compiled.digest();
        let snapshot_id = compiled.snapshot_id();
        let current_step_id = compiled.current_step_id();
        let request = compiled.into_request();
        let primary =
            complete_request(self.provider, &request, self.model_timeout, control).await?;
        if primary.reason != ModelFinishReason::Stop {
            return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                charge: AgentTurnCharge::new(
                    primary.prompt_tokens,
                    primary.output_tokens,
                    None,
                    AgentTurnRepairUsage::None,
                ),
                reason: AgentTurnRejectionReason::IncompleteModelOutput,
                snapshot_id,
                observed_model_output_bytes: usize_to_u64(primary.raw.len())?,
            }));
        }
        let (decoded, prompt_tokens, output_tokens, repair, observed_model_output_bytes) =
            match DecodeAgentActionTurn::current().decode_primary(&primary.raw) {
                AgentActionPrimaryOutcome::Accepted(action) => (
                    action,
                    primary.prompt_tokens,
                    primary.output_tokens,
                    AgentTurnRepairUsage::None,
                    usize_to_u64(primary.raw.len())?,
                ),
                AgentActionPrimaryOutcome::RepairRequired(repair) => {
                    if AgentControllerControl::is_cancelled(control) {
                        return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                            charge: AgentTurnCharge::new(
                                primary.prompt_tokens,
                                primary.output_tokens,
                                None,
                                AgentTurnRepairUsage::None,
                            ),
                            reason: AgentTurnRejectionReason::CancelledBeforeAction,
                            snapshot_id,
                            observed_model_output_bytes: usize_to_u64(primary.raw.len())?,
                        }));
                    }
                    let prepared = repair.prepare()?;
                    let mut messages = request.messages().to_vec();
                    messages.push(prepared.instruction().clone());
                    let repair_request = ModelProviderRequest::new(
                        request.profile().clone(),
                        messages,
                        request.structured_output().cloned(),
                    )?;
                    let corrected = complete_request(
                        self.provider,
                        &repair_request,
                        self.model_timeout,
                        control,
                    )
                    .await?;
                    let prompt_tokens =
                        add_token_counts(primary.prompt_tokens, corrected.prompt_tokens)?;
                    let output_tokens =
                        add_token_counts(primary.output_tokens, corrected.output_tokens)?;
                    let observed_model_output_bytes =
                        combined_output_bytes(primary.raw.len(), corrected.raw.len())?;
                    if corrected.reason != ModelFinishReason::Stop {
                        return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                            charge: AgentTurnCharge::new(
                                prompt_tokens,
                                output_tokens,
                                None,
                                AgentTurnRepairUsage::One,
                            ),
                            reason: AgentTurnRejectionReason::IncompleteModelOutput,
                            snapshot_id,
                            observed_model_output_bytes,
                        }));
                    }
                    let Ok(action) = prepared.decode(&corrected.raw) else {
                        return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                            charge: AgentTurnCharge::new(
                                prompt_tokens,
                                output_tokens,
                                None,
                                AgentTurnRepairUsage::One,
                            ),
                            reason: AgentTurnRejectionReason::InvalidAfterRepair,
                            snapshot_id,
                            observed_model_output_bytes,
                        }));
                    };
                    (
                        action,
                        prompt_tokens,
                        output_tokens,
                        AgentTurnRepairUsage::One,
                        observed_model_output_bytes,
                    )
                }
            };
        let (action, public_note) = decoded.into_parts();
        let action_class = AgentTurnActionClass::from_action(&action);
        let charge = AgentTurnCharge::new(prompt_tokens, output_tokens, Some(action_class), repair);
        if AgentControllerControl::is_cancelled(control) {
            return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                charge,
                reason: AgentTurnRejectionReason::CancelledBeforeAction,
                snapshot_id,
                observed_model_output_bytes,
            }));
        }
        if let AgentAction::UpdateLedger(update) = &action
            && update.step_id() != current_step_id
        {
            return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                charge,
                reason: AgentTurnRejectionReason::StepMismatch,
                snapshot_id,
                observed_model_output_bytes,
            }));
        }
        let read_action = match &action {
            AgentAction::Search(action) => Some(AgentReadAction::Search(action.clone())),
            AgentAction::Inspect(action) => Some(AgentReadAction::Inspect(action.clone())),
            AgentAction::UpdateLedger(_)
            | AgentAction::Finish(_)
            | AgentAction::ApplyPatch(_)
            | AgentAction::Run(_) => None,
        };
        let tool_result = if let Some(read_action) = read_action {
            let action_ordinal = run
                .usage()
                .action_count()
                .checked_add(1)
                .ok_or(ExecuteAgentTurnFailure::InvalidToolIdentity)?;
            let tool_run_id = ToolRunId::for_agent_action_v1(run.id(), action_ordinal)
                .map_err(|_| ExecuteAgentTurnFailure::InvalidToolIdentity)?;
            let attempt = self
                .recovery
                .begin_agent_tool_attempt(
                    input.project(),
                    run.id(),
                    snapshot_id,
                    tool_run_id,
                    observed_at,
                )
                .await?;
            let result = match self
                .tools
                .execute(
                    input.project(),
                    snapshot_id,
                    tool_run_id,
                    &read_action,
                    self.read_timeout,
                    control,
                )
                .await
            {
                Ok(result) => result,
                Err(failure) => {
                    let status = match failure {
                        AgentReadToolFailure::Denied => AgentToolAttemptStatus::Denied,
                        AgentReadToolFailure::Cancelled => AgentToolAttemptStatus::Cancelled,
                        AgentReadToolFailure::Unavailable
                        | AgentReadToolFailure::InvalidResult
                        | AgentReadToolFailure::TimedOut => AgentToolAttemptStatus::Failed,
                    };
                    self.recovery
                        .finish_agent_tool_attempt(
                            input.project(),
                            tool_run_id,
                            attempt.attempt(),
                            status,
                            observed_at,
                        )
                        .await?;
                    return Err(ExecuteAgentTurnFailure::Read(failure));
                }
            };
            if result.snapshot_id() != snapshot_id || result.tool_run_id() != tool_run_id {
                self.recovery
                    .finish_agent_tool_attempt(
                        input.project(),
                        tool_run_id,
                        attempt.attempt(),
                        AgentToolAttemptStatus::Failed,
                        observed_at,
                    )
                    .await?;
                return Ok(AgentTurnOutcome::Rejected(RejectedAgentTurn {
                    charge,
                    reason: AgentTurnRejectionReason::InvalidReadResult,
                    snapshot_id,
                    observed_model_output_bytes,
                }));
            }
            Some(result)
        } else {
            None
        };
        Ok(AgentTurnOutcome::Executed(Box::new(AgentTurnExecution {
            action,
            public_note,
            charge,
            context_digest,
            snapshot_id,
            current_step_id,
            tool_result,
            observed_model_output_bytes,
        })))
    }
}

/// Compatibility name retained for H9 callers; new code should use [`ExecuteAgentTurn`].
pub type ExecuteReadOnlyAgentTurn<'a> = ExecuteAgentTurn<'a>;

fn validate_turn_input(
    run: &AgentRun,
    input: &AgentContextCompileInput,
) -> Result<(), ExecuteAgentTurnFailure> {
    let current_step = input
        .task_ledger()
        .step(input.current_step_id())
        .ok_or(ExecuteAgentTurnFailure::InputMismatch)?;
    if run.goal_contract() != input.goal_contract().reference()
        || run.task_ledger_revision() != input.task_ledger().revision()
        || run.model_profile() != Some(input.model_profile().reference())
        || current_step.status() != TaskStepStatus::InProgress
        || current_step
            .attempts()
            .last()
            .is_none_or(|attempt| attempt.run_id() != run.id())
        || input
            .run_memory()
            .is_some_and(|memory| memory.snapshot_id() != run.current_snapshot_id())
    {
        return Err(ExecuteAgentTurnFailure::InputMismatch);
    }
    Ok(())
}

#[derive(Debug)]
struct CompletedModelRequest {
    raw: String,
    reason: ModelFinishReason,
    prompt_tokens: ModelTokenCount,
    output_tokens: ModelTokenCount,
}

async fn complete_request<C>(
    provider: &dyn ModelProvider,
    request: &ModelProviderRequest,
    timeout: ModelRequestTimeout,
    control: &C,
) -> Result<CompletedModelRequest, ExecuteAgentTurnFailure>
where
    C: ModelOperationControl,
{
    if provider.provider_id() != request.profile().provider_id() {
        return Err(ExecuteAgentTurnFailure::ProviderMismatch);
    }
    let mut stream = provider.stream(request, timeout, control).await?;
    let mut raw = String::new();
    let mut completion = None;
    while let Some(event) = stream.next().await {
        match event? {
            ProviderEvent::OutputText(chunk) if completion.is_none() => {
                let next = raw
                    .len()
                    .checked_add(chunk.as_str().len())
                    .ok_or(ExecuteAgentTurnFailure::OutputTooLarge)?;
                if next > MAX_AGENT_RAW_OUTPUT_BYTES {
                    return Err(ExecuteAgentTurnFailure::OutputTooLarge);
                }
                raw.push_str(chunk.as_str());
            }
            ProviderEvent::Completed(value) if completion.is_none() => completion = Some(value),
            ProviderEvent::OutputText(_) | ProviderEvent::Completed(_) => {
                return Err(ExecuteAgentTurnFailure::InvalidProviderStream);
            }
        }
    }
    let completion = completion.ok_or(ExecuteAgentTurnFailure::InvalidProviderStream)?;
    let fallback_prompt = count_request_tokens(request)?;
    let fallback_output = request
        .profile()
        .settings()
        .token_counting()
        .count_text(&raw)?;
    Ok(CompletedModelRequest {
        raw,
        reason: completion.reason(),
        prompt_tokens: reported_or_fallback(completion.usage().prompt_tokens(), fallback_prompt)?,
        output_tokens: reported_or_fallback(completion.usage().output_tokens(), fallback_output)?,
    })
}

fn count_request_tokens(
    request: &ModelProviderRequest,
) -> Result<ModelTokenCount, ExecuteAgentTurnFailure> {
    request
        .messages()
        .iter()
        .try_fold(ModelTokenCount::new(0), |total, message| {
            let count = request
                .profile()
                .settings()
                .token_counting()
                .count_text(message.content())?;
            add_token_counts(total, count)
        })
}

fn reported_or_fallback(
    reported: Option<u64>,
    fallback: ModelTokenCount,
) -> Result<ModelTokenCount, ExecuteAgentTurnFailure> {
    match reported {
        Some(value) => Ok(ModelTokenCount::new(
            u32::try_from(value).map_err(|_| ExecuteAgentTurnFailure::TokenOverflow)?,
        )),
        None => Ok(fallback),
    }
}

fn add_token_counts(
    left: ModelTokenCount,
    right: ModelTokenCount,
) -> Result<ModelTokenCount, ExecuteAgentTurnFailure> {
    Ok(ModelTokenCount::new(
        left.get()
            .checked_add(right.get())
            .ok_or(ExecuteAgentTurnFailure::TokenOverflow)?,
    ))
}

fn usize_to_u64(value: usize) -> Result<u64, ExecuteAgentTurnFailure> {
    u64::try_from(value).map_err(|_| ExecuteAgentTurnFailure::OutputTooLarge)
}

fn combined_output_bytes(primary: usize, corrected: usize) -> Result<u64, ExecuteAgentTurnFailure> {
    usize_to_u64(
        primary
            .checked_add(corrected)
            .ok_or(ExecuteAgentTurnFailure::OutputTooLarge)?,
    )
}

/// Turn failed before a safe completed action outcome could be returned.
#[derive(Debug)]
pub enum ExecuteAgentTurnFailure {
    /// Execute-state, cancellation, or budget preflight rejected the turn.
    Preflight(AgentControllerPreflightFailure),
    /// Context input did not match current run anchors.
    InputMismatch,
    /// Fresh compiled context did not match current run anchors.
    ContextMismatch,
    /// H7 context compilation failed.
    Context(ContextCompileFailure),
    /// Configured provider did not match the selected immutable profile.
    ProviderMismatch,
    /// Neutral model provider failed.
    Model(ModelProviderFailure),
    /// Provider event ordering was incomplete or invalid.
    InvalidProviderStream,
    /// Accumulated raw output exceeded 64 KiB.
    OutputTooLarge,
    /// Provider usage or cumulative repair usage exceeded the typed range.
    TokenOverflow,
    /// Repair message could not be constructed safely.
    RepairMessage(ModelMessageError),
    /// Repair provider request violated the neutral boundary.
    RepairRequest(ModelProviderRequestError),
    /// Profile token counting exceeded its shared range.
    TokenCount(ModelTokenCountError),
    /// The sole read-only action failed at its capability boundary.
    Read(AgentReadToolFailure),
    /// A unique run-local tool identity could not be derived.
    InvalidToolIdentity,
    /// Durable tool-attempt lifecycle persistence failed.
    ToolLifecycle(AgentRecoveryStoreFailure),
    /// Cancellation was observed before a complete model exchange.
    Cancelled,
}

impl fmt::Display for ExecuteAgentTurnFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Preflight(_) => "agent turn failed controller preflight",
            Self::InputMismatch => "agent turn input does not match the run",
            Self::ContextMismatch => "compiled agent context does not match the run",
            Self::Context(_) => "agent turn context compilation failed",
            Self::ProviderMismatch => "agent turn provider does not match its model profile",
            Self::Model(_) => "agent turn model provider failed",
            Self::InvalidProviderStream => "agent turn provider stream is invalid",
            Self::OutputTooLarge => "agent turn model output exceeds its boundary",
            Self::TokenOverflow => "agent turn token usage exceeds its boundary",
            Self::RepairMessage(_) => "agent turn repair message is invalid",
            Self::RepairRequest(_) => "agent turn repair request is invalid",
            Self::TokenCount(_) => "agent turn token count failed",
            Self::Read(_) => "agent turn read action failed",
            Self::InvalidToolIdentity => "agent turn tool identity is invalid",
            Self::ToolLifecycle(_) => "agent turn tool lifecycle persistence failed",
            Self::Cancelled => "agent turn was cancelled",
        })
    }
}

impl ExecuteAgentTurnFailure {
    /// Model, context, provider, and read failures occur before any mutating action is returned.
    #[must_use]
    pub const fn mutation_application_state(&self) -> a3_domain::MutationApplicationState {
        a3_domain::MutationApplicationState::NotApplied
    }
}

impl Error for ExecuteAgentTurnFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Preflight(error) => Some(error),
            Self::Context(error) => Some(error),
            Self::Model(error) => Some(error),
            Self::RepairMessage(error) => Some(error),
            Self::RepairRequest(error) => Some(error),
            Self::TokenCount(error) => Some(error),
            Self::Read(error) => Some(error),
            Self::ToolLifecycle(error) => Some(error),
            Self::InputMismatch
            | Self::ContextMismatch
            | Self::ProviderMismatch
            | Self::InvalidProviderStream
            | Self::OutputTooLarge
            | Self::TokenOverflow
            | Self::InvalidToolIdentity
            | Self::Cancelled => None,
        }
    }
}

impl From<AgentControllerPreflightFailure> for ExecuteAgentTurnFailure {
    fn from(value: AgentControllerPreflightFailure) -> Self {
        Self::Preflight(value)
    }
}

impl From<ContextCompileFailure> for ExecuteAgentTurnFailure {
    fn from(value: ContextCompileFailure) -> Self {
        Self::Context(value)
    }
}

impl From<ModelProviderFailure> for ExecuteAgentTurnFailure {
    fn from(value: ModelProviderFailure) -> Self {
        Self::Model(value)
    }
}

impl From<ModelMessageError> for ExecuteAgentTurnFailure {
    fn from(value: ModelMessageError) -> Self {
        Self::RepairMessage(value)
    }
}

impl From<ModelProviderRequestError> for ExecuteAgentTurnFailure {
    fn from(value: ModelProviderRequestError) -> Self {
        Self::RepairRequest(value)
    }
}

impl From<ModelTokenCountError> for ExecuteAgentTurnFailure {
    fn from(value: ModelTokenCountError) -> Self {
        Self::TokenCount(value)
    }
}

impl From<AgentReadToolFailure> for ExecuteAgentTurnFailure {
    fn from(value: AgentReadToolFailure) -> Self {
        Self::Read(value)
    }
}

impl From<AgentRecoveryStoreFailure> for ExecuteAgentTurnFailure {
    fn from(value: AgentRecoveryStoreFailure) -> Self {
        Self::ToolLifecycle(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentActionJsonSchema, AgentContextCompilerFuture, CompiledAgentContext,
        ContextCompilePhase, ContextToolResultDigest, ContextToolResultPreview,
        ContextToolResultStatus, ModelCancellationFuture, ModelMessage, ModelMessageRole,
        ModelOutputChunk, ModelProviderCompletion, ModelProviderFuture, ModelProviderUsage,
        StructuredOutputSchema, TaskLensControlError,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
        AgentControllerState, AgentRunId, AgentToolEvidenceSet, CanonicalDirectory,
        ContextBudgetPlan, ContextBudgetUsage, ContextCompilerPolicyVersion, ExpectedTaskEvidence,
        GitHead, GitReferenceName, GoalContract, GoalContractDraft, GoalContractTimestamp,
        GoalObjective, IndexRunId, ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit,
        ModelParallelismLimit, ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding,
        ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, RepositoryId,
        RepositoryIdentity, RunEventId, RunEventPayload, RunEventSequence, SnapshotId,
        SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskLensDigest,
        TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, ToolRunId,
        VerificationMethod, VerificationRequirement, VerificationSpec, VerificationSpecId,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::stream;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestControl;

    impl AgentControllerControl for TestControl {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    impl ContextCompileControl for TestControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_phase(&self, _phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
            Ok(())
        }
    }

    impl ModelOperationControl for TestControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> ModelCancellationFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    #[derive(Debug)]
    struct OneContextCompiler(Mutex<Option<CompiledAgentContext>>);

    impl AgentContextCompiler for OneContextCompiler {
        fn compile<'a>(
            &'a self,
            _input: &'a AgentContextCompileInput,
            _control: &'a dyn ContextCompileControl,
        ) -> AgentContextCompilerFuture<'a> {
            let result = self
                .0
                .lock()
                .map_err(|_| ContextCompileFailure::InvalidPack)
                .and_then(|mut value| value.take().ok_or(ContextCompileFailure::InvalidPack));
            Box::pin(async move { result })
        }
    }

    #[derive(Debug)]
    struct ScriptedProvider {
        provider_id: ModelProviderId,
        responses: Mutex<VecDeque<Vec<ProviderEvent>>>,
    }

    impl ModelProvider for ScriptedProvider {
        fn provider_id(&self) -> &ModelProviderId {
            &self.provider_id
        }

        fn stream<'a>(
            &'a self,
            _request: &'a ModelProviderRequest,
            _timeout: ModelRequestTimeout,
            _control: &'a dyn ModelOperationControl,
        ) -> ModelProviderFuture<'a> {
            let response = self
                .responses
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)
                .and_then(|mut responses| {
                    responses
                        .pop_front()
                        .ok_or(ModelProviderFailure::Unavailable)
                });
            Box::pin(async move {
                response.map(|events| {
                    Box::pin(stream::iter(events.into_iter().map(Ok)))
                        as crate::ProviderEventStream<'a>
                })
            })
        }
    }

    #[derive(Debug)]
    struct CountingReadTools {
        calls: AtomicUsize,
    }

    #[test]
    fn provider_disconnect_is_explicitly_not_applied() {
        let failure = ExecuteAgentTurnFailure::Model(ModelProviderFailure::Unavailable);

        assert_eq!(
            failure.mutation_application_state(),
            a3_domain::MutationApplicationState::NotApplied
        );
    }

    impl AgentReadTools for CountingReadTools {
        fn execute<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            snapshot_id: SnapshotId,
            tool_run_id: ToolRunId,
            _action: &'a AgentReadAction,
            _timeout: AgentReadTimeout,
            _control: &'a dyn AgentControllerControl,
        ) -> AgentReadToolsFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                AgentReadResult::new(
                    tool_run_id,
                    ContextToolResultStatus::Succeeded,
                    ContextToolResultPreview::try_from_string("bounded result".to_owned())
                        .map_err(|_| AgentReadToolFailure::InvalidResult)?,
                    ContextToolResultDigest::from_bytes([31; 32]),
                    false,
                    snapshot_id,
                    AgentToolEvidenceSet::new(snapshot_id, Vec::new())
                        .map_err(|_| AgentReadToolFailure::InvalidResult)?,
                    14,
                )
                .map_err(|_| AgentReadToolFailure::InvalidResult)
            })
        }
    }

    #[derive(Debug, Default)]
    struct TestRecoveryStore {
        begins: AtomicUsize,
        finishes: Mutex<Vec<AgentToolAttemptStatus>>,
    }

    impl AgentRecoveryStore for TestRecoveryStore {
        fn begin_agent_tool_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            run_id: AgentRunId,
            snapshot_id: SnapshotId,
            tool_run_id: ToolRunId,
            started_at: AgentRunTimestamp,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentToolAttempt> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                a3_domain::AgentToolAttempt::new(
                    tool_run_id,
                    a3_domain::AgentToolAttemptNumber::FIRST,
                    run_id,
                    snapshot_id,
                    AgentToolAttemptStatus::InFlight,
                    started_at,
                    started_at,
                )
                .map_err(|_| AgentRecoveryStoreFailure::InvalidStoredData)
            })
        }

        fn begin_agent_mutation_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
            _snapshot_id: SnapshotId,
            _tool_run_id: ToolRunId,
            _fingerprint: a3_domain::MutationActionFingerprint,
            _kind: a3_domain::AgentMutationKind,
            _started_at: AgentRunTimestamp,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentMutationAttempt> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::InvalidStoredData) })
        }

        fn finish_agent_tool_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            tool_run_id: ToolRunId,
            attempt: a3_domain::AgentToolAttemptNumber,
            status: AgentToolAttemptStatus,
            finished_at: AgentRunTimestamp,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentToolAttempt> {
            let recorded = self
                .finishes
                .lock()
                .map_err(|_| AgentRecoveryStoreFailure::Unavailable)
                .map(|mut finishes| finishes.push(status));
            Box::pin(async move {
                recorded?;
                a3_domain::AgentToolAttempt::new(
                    tool_run_id,
                    attempt,
                    AgentRunId::from_bytes([0; 32]),
                    SnapshotId::from_bytes([0; 32]),
                    status,
                    finished_at,
                    finished_at,
                )
                .map_err(|_| AgentRecoveryStoreFailure::InvalidStoredData)
            })
        }

        fn finish_agent_mutation_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _tool_run_id: ToolRunId,
            _attempt: a3_domain::AgentToolAttemptNumber,
            _status: AgentToolAttemptStatus,
            _disposition: a3_domain::AgentMutationDisposition,
            _finished_at: AgentRunTimestamp,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentMutationAttempt> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::InvalidStoredData) })
        }

        fn complete_agent_tool_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_last_sequence: RunEventSequence,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
            _tool_run_id: ToolRunId,
            _attempt: a3_domain::AgentToolAttemptNumber,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentToolAttempt> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::Unavailable) })
        }

        fn complete_agent_mutation_attempt<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_last_sequence: RunEventSequence,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
            _tool_run_id: ToolRunId,
            _attempt: a3_domain::AgentToolAttemptNumber,
            _result: crate::AgentMutationResultRecord,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentMutationAttempt> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::InvalidStoredData) })
        }

        fn interrupt_agent_tool_attempts<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
            _interrupted_at: AgentRunTimestamp,
        ) -> crate::AgentRecoveryStoreFuture<'a, u32> {
            Box::pin(async { Ok(0) })
        }

        fn load_agent_mutation_attempts<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
        ) -> crate::AgentRecoveryStoreFuture<'a, Vec<a3_domain::AgentMutationAttempt>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn reconcile_agent_mutation<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_last_sequence: RunEventSequence,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
            _tool_run_id: ToolRunId,
            _attempt: a3_domain::AgentToolAttemptNumber,
        ) -> crate::AgentRecoveryStoreFuture<'a, a3_domain::AgentMutationAttempt> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::InvalidStoredData) })
        }

        fn load_agent_tool_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
            _evidence_ids: &'a [a3_domain::TaskEvidenceId],
        ) -> crate::AgentRecoveryStoreFuture<'a, Vec<a3_domain::AgentToolEvidence>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn commit_agent_recovery<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _choice: crate::AgentRecoveryChoice,
            _expected_published_snapshot: SnapshotId,
            _expected_ledger_version: crate::TaskLedgerStoreVersion,
            _expected_last_sequence: RunEventSequence,
            _ledger: &'a TaskLedger,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
        ) -> crate::AgentRecoveryStoreFuture<'a, crate::TaskLedgerStoreVersion> {
            Box::pin(async { Err(AgentRecoveryStoreFailure::Unavailable) })
        }
    }

    #[derive(Debug)]
    struct DeniedReadTools<'a> {
        durable_begins: &'a AtomicUsize,
    }

    impl AgentReadTools for DeniedReadTools<'_> {
        fn execute<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _snapshot_id: SnapshotId,
            _tool_run_id: ToolRunId,
            _action: &'a AgentReadAction,
            _timeout: AgentReadTimeout,
            _control: &'a dyn AgentControllerControl,
        ) -> AgentReadToolsFuture<'a> {
            let failure = if self.durable_begins.load(Ordering::SeqCst) == 1 {
                AgentReadToolFailure::Denied
            } else {
                AgentReadToolFailure::InvalidResult
            };
            Box::pin(async move { Err(failure) })
        }
    }

    #[test]
    fn valid_search_executes_exactly_one_read_action() -> Result<(), Box<dyn Error>> {
        let mut fixture = turn_fixture(vec![provider_response(
            r#"{"schema_version":4,"public_note":{"goal":"Controller finden","finding_kind":"hypothesis","finding":"Die Implementierung muss noch lokalisiert werden.","finding_source_refs":[],"gap":"Aktuelle Quelle","next_step":"Nach dem Controller suchen"},"action":{"kind":"search","query":"controller","limit":5}}"#,
        )?])?;
        let compiler = OneContextCompiler(Mutex::new(Some(fixture.compiled)));
        let provider = ScriptedProvider {
            provider_id: fixture.profile.provider_id().clone(),
            responses: Mutex::new(fixture.responses),
        };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();

        let outcome = futures::executor::block_on(
            ExecuteReadOnlyAgentTurn::new(&compiler, &provider, &tools, &recovery).execute(
                &fixture.run,
                &fixture.input,
                timestamp(5)?,
                &TestControl,
            ),
        )?;
        let model_event = outcome.record(&mut fixture.run, event_id(20), timestamp(20)?)?;

        let AgentTurnOutcome::Executed(mut execution) = outcome else {
            return Err("valid search was rejected".into());
        };
        assert!(matches!(execution.action(), AgentAction::Search(_)));
        assert_eq!(
            execution.public_note().map(|note| note.goal.as_str()),
            Some("Controller finden")
        );
        assert_eq!(
            execution.charge().action(),
            Some(AgentTurnActionClass::Search)
        );
        assert_eq!(execution.charge().repair(), AgentTurnRepairUsage::None);
        assert!(execution.tool_result().is_some());
        let recorded = execution
            .take_tool_result()
            .ok_or("search did not retain its read result")?
            .record(&mut fixture.run, event_id(21), timestamp(21)?)?;
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 1);
        assert!(
            recovery
                .finishes
                .lock()
                .map_err(|_| "finish log poisoned")?
                .is_empty()
        );
        assert_eq!(fixture.run.usage().turn_count(), 1);
        assert_eq!(fixture.run.usage().action_count(), 1);
        assert_eq!(fixture.run.usage().repair_count(), 0);
        assert!(model_event.payload().redaction().is_some());
        assert_eq!(model_event.sequence(), RunEventSequence::new(5)?);
        assert_eq!(recorded.event().sequence(), RunEventSequence::new(6)?);
        assert_eq!(
            recorded.context_result().sequence(),
            recorded.event().sequence()
        );
        Ok(())
    }

    #[test]
    fn invalid_primary_and_repair_never_cross_the_tool_boundary() -> Result<(), Box<dyn Error>> {
        let mut fixture = turn_fixture(vec![
            provider_response("not-json")?,
            provider_response(r#"{"schema_version":1,"action":{"kind":"shell"}}"#)?,
        ])?;
        let compiler = OneContextCompiler(Mutex::new(Some(fixture.compiled)));
        let provider = ScriptedProvider {
            provider_id: fixture.profile.provider_id().clone(),
            responses: Mutex::new(fixture.responses),
        };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();

        let outcome = futures::executor::block_on(
            ExecuteReadOnlyAgentTurn::new(&compiler, &provider, &tools, &recovery).execute(
                &fixture.run,
                &fixture.input,
                timestamp(5)?,
                &TestControl,
            ),
        )?;
        let event = outcome.record(&mut fixture.run, event_id(20), timestamp(20)?)?;

        let AgentTurnOutcome::Rejected(rejected) = outcome else {
            return Err("invalid repaired output executed".into());
        };
        assert_eq!(
            rejected.reason(),
            AgentTurnRejectionReason::InvalidAfterRepair
        );
        assert_eq!(rejected.charge().repair(), AgentTurnRepairUsage::One);
        assert_eq!(rejected.charge().action(), None);
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.run.usage().turn_count(), 1);
        assert_eq!(fixture.run.usage().action_count(), 0);
        assert_eq!(fixture.run.usage().repair_count(), 1);
        assert_eq!(event.payload().code(), RunEventCode::InvalidModelOutput);
        Ok(())
    }

    #[test]
    fn denied_tool_attempt_is_durable_before_invocation_and_then_terminal()
    -> Result<(), Box<dyn Error>> {
        let fixture = turn_fixture(vec![provider_response(
            r#"{"schema_version":4,"public_note":{"goal":"Controller finden","finding_kind":"hypothesis","finding":"Die Implementierung muss noch lokalisiert werden.","finding_source_refs":[],"gap":"Aktuelle Quelle","next_step":"Nach dem Controller suchen"},"action":{"kind":"search","query":"controller","limit":5}}"#,
        )?])?;
        let compiler = OneContextCompiler(Mutex::new(Some(fixture.compiled)));
        let provider = ScriptedProvider {
            provider_id: fixture.profile.provider_id().clone(),
            responses: Mutex::new(fixture.responses),
        };
        let recovery = TestRecoveryStore::default();
        let tools = DeniedReadTools {
            durable_begins: &recovery.begins,
        };

        let result = futures::executor::block_on(
            ExecuteReadOnlyAgentTurn::new(&compiler, &provider, &tools, &recovery).execute(
                &fixture.run,
                &fixture.input,
                timestamp(5)?,
                &TestControl,
            ),
        );

        assert!(matches!(
            result,
            Err(ExecuteAgentTurnFailure::Read(AgentReadToolFailure::Denied))
        ));
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 1);
        assert_eq!(
            *recovery
                .finishes
                .lock()
                .map_err(|_| "finish log poisoned")?,
            vec![AgentToolAttemptStatus::Denied]
        );
        Ok(())
    }

    struct TurnFixture {
        run: AgentRun,
        input: AgentContextCompileInput,
        compiled: CompiledAgentContext,
        profile: ModelProfile,
        responses: VecDeque<Vec<ProviderEvent>>,
    }

    fn turn_fixture(responses: Vec<Vec<ProviderEvent>>) -> Result<TurnFixture, Box<dyn Error>> {
        let project = project()?;
        let goal = goal()?;
        let profile = profile()?;
        let step_id = TaskStepId::from_bytes([5; 32]);
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![step_definition(step_id)?],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        let (mut run, _) = AgentRun::start(
            run_id(),
            goal.reference(),
            ledger.revision(),
            profile.reference(),
            snapshot(),
            event_id(1),
            timestamp(1)?,
        )?;
        ledger.start_step(step_id, run_id(), TaskLedgerTimestamp::from_unix_millis(2)?)?;
        for (id, state) in [
            (2, AgentControllerState::Localize),
            (3, AgentControllerState::Plan),
            (4, AgentControllerState::Execute),
        ] {
            run.transition(
                event_id(id),
                state,
                RunEventPayload::empty(),
                snapshot(),
                timestamp(u64::from(id))?,
            )?;
        }
        let input = AgentContextCompileInput::new(
            project,
            goal.clone(),
            ledger.clone(),
            step_id,
            profile.clone(),
            None,
            Vec::new(),
            Vec::new(),
        )?;
        let message = ModelMessage::try_from_string(
            ModelMessageRole::User,
            "bounded controller context".to_owned(),
        )?;
        let schema = StructuredOutputSchema::new(AgentActionJsonSchema::current().as_json()?)?;
        let request = ModelProviderRequest::new(profile.clone(), vec![message], Some(schema))?;
        let plan = ContextBudgetPlan::for_profile(&profile)?;
        let usage = ContextBudgetUsage::new(plan, 0, 0, 0, 0, 0)?;
        let compiled = CompiledAgentContext::new(
            request,
            ContextCompilerPolicyVersion::V1,
            ContextDigest::from_bytes([11; 32]),
            goal.reference(),
            ledger.revision(),
            step_id,
            IndexRunId::from_bytes([12; 32]),
            snapshot(),
            TaskLensDigest::from_bytes([13; 32]),
            None,
            plan,
            usage,
            0,
            false,
        );
        Ok(TurnFixture {
            run,
            input,
            compiled,
            profile,
            responses: responses.into(),
        })
    }

    fn provider_response(raw: &str) -> Result<Vec<ProviderEvent>, Box<dyn Error>> {
        Ok(vec![
            ProviderEvent::OutputText(ModelOutputChunk::try_from_string(raw.to_owned())?),
            ProviderEvent::Completed(ModelProviderCompletion::new(
                ModelFinishReason::Stop,
                ModelProviderUsage::new(Some(100), Some(10)),
            )),
        ])
    }

    fn step_definition(step_id: TaskStepId) -> Result<TaskStepDefinition, Box<dyn Error>> {
        Ok(TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("inspect the controller".to_owned())?,
            TaskStepRationale::try_from_string("ground the next action".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "bounded read evidence".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([6; 32]),
                VerificationMethod::Diagnostic,
                VerificationRequirement::try_from_string("read result is current".to_owned())?,
            ),
        )?)
    }

    fn profile() -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("test-provider".to_owned())?,
            ModelId::try_from_string("test-model".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(4_096)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::FormatFieldOnly,
            )?,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
        ))
    }

    fn goal() -> Result<GoalContract, Box<dyn Error>> {
        Ok(GoalContract::initial(
            TaskId::from_bytes([2; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("execute one safe controller turn".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([3; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "one action is bounded".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run agent turn tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        ))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([20; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([21; 32]),
                WorktreeAnchorId::from_bytes([22; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }

    const fn run_id() -> AgentRunId {
        AgentRunId::from_bytes([1; 32])
    }

    const fn snapshot() -> SnapshotId {
        SnapshotId::from_bytes([9; 32])
    }

    const fn event_id(value: u8) -> RunEventId {
        RunEventId::from_bytes([value; 32])
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
