use a3_domain::{
    AcceptanceVerificationReceipt, AgentBudgetDimension, AgentBudgetEvaluationError,
    AgentBudgetExhaustion, AgentControllerState, AgentRun, AgentRunError, AgentRunId,
    AgentRunTimestamp, GoalContract, ProjectIdentity, RunEvent, RunEventCode, RunEventId,
    RunEventOutcome, RunEventPayload, SnapshotId, TaskLedger, TaskStepStatus,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_ACCEPTANCE_VERIFIER_TIMEOUT_MILLIS: u64 = 120_000;

/// Cooperative cancellation boundary shared by deterministic controller operations.
pub trait AgentControllerControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning run requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl AgentControllerControl for crate::JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Closed non-success signals that advance the finite controller by one state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentControllerSignal {
    /// Intake anchors were validated.
    AnchorsAccepted,
    /// Current deterministic repository context was localized.
    LocalizationComplete,
    /// A valid current Task Ledger step is ready.
    PlanReady,
    /// Exactly one bounded turn completed and now requires verification.
    TurnNeedsVerification,
    /// The current action needs one scoped approval.
    ApprovalRequired,
    /// Verification needs another bounded execution turn.
    VerificationNeedsExecution,
    /// Verification requires a material replan.
    VerificationNeedsReplan,
    /// The immediate next Task Ledger revision was applied.
    ReplanApplied,
    /// Scoped approval was granted.
    ApprovalGranted,
    /// Scoped approval was denied.
    ApprovalDenied,
    /// A non-recoverable content-free controller failure occurred.
    FatalFailure,
    /// Cooperative cancellation was observed.
    CancelRequested,
}

/// Why one durable controller transition was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentControllerAdvanceKind {
    /// The supplied finite-state signal was applied.
    Signal(AgentControllerSignal),
    /// A hard resource ceiling stopped further autonomous work.
    BudgetExhausted(AgentBudgetExhaustion),
    /// The acceptance verifier rejected current completion.
    AcceptanceRejected(AcceptanceRejection),
    /// The acceptance verifier itself failed at a bounded external boundary.
    AcceptanceVerifierFailed(AcceptanceVerifierFailure),
    /// Current acceptance evidence passed every mandatory criterion.
    AcceptanceVerified,
}

/// One applied transition event and its resulting materialized state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentControllerAdvance {
    event: RunEvent,
    state: AgentControllerState,
    kind: AgentControllerAdvanceKind,
}

impl AgentControllerAdvance {
    fn new(event: RunEvent, state: AgentControllerState, kind: AgentControllerAdvanceKind) -> Self {
        Self { event, state, kind }
    }

    /// Returns the append-only event that must be persisted with the materialized run.
    #[must_use]
    pub const fn event(&self) -> &RunEvent {
        &self.event
    }

    /// Moves the event to the compare-and-swap journal boundary.
    #[must_use]
    pub fn into_event(self) -> RunEvent {
        self.event
    }

    /// Returns the resulting finite controller state.
    #[must_use]
    pub const fn state(&self) -> AgentControllerState {
        self.state
    }

    /// Returns the content-free decision class.
    #[must_use]
    pub const fn kind(&self) -> AgentControllerAdvanceKind {
        self.kind
    }
}

/// Deterministic single-transition H9 state-machine use case.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdvanceAgentController;

impl AdvanceAgentController {
    /// Applies one legal non-success signal, with cancellation and budgets taking precedence.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        self,
        run: &mut AgentRun,
        signal: AgentControllerSignal,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        observed_at: AgentRunTimestamp,
        cancellation_requested: bool,
    ) -> Result<AgentControllerAdvance, AgentControllerError> {
        validate_active_snapshot(run, snapshot_id)?;
        if cancellation_requested || signal == AgentControllerSignal::CancelRequested {
            return transition(
                run,
                event_id,
                AgentControllerState::Cancelled,
                RunEventPayload::new(
                    RunEventCode::Cancellation,
                    Some(RunEventOutcome::Cancelled),
                    None,
                ),
                snapshot_id,
                observed_at,
                AgentControllerAdvanceKind::Signal(AgentControllerSignal::CancelRequested),
            );
        }
        if signal != AgentControllerSignal::TurnNeedsVerification
            && let Some(exhaustion) = run.budget_exhaustion(observed_at)?
        {
            return exhaust_budget(run, event_id, snapshot_id, observed_at, exhaustion);
        }
        let next = next_state(run.state(), signal)?;
        let (code, outcome) = match signal {
            AgentControllerSignal::ApprovalDenied => {
                (RunEventCode::PolicyDecision, Some(RunEventOutcome::Denied))
            }
            AgentControllerSignal::FatalFailure => (
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Failed),
            ),
            _ => (
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
            ),
        };
        transition(
            run,
            event_id,
            next,
            RunEventPayload::new(code, outcome, None),
            snapshot_id,
            observed_at,
            AgentControllerAdvanceKind::Signal(signal),
        )
    }

    /// Rejects a new model turn when cancellation or a hard ceiling is already visible.
    pub fn preflight(
        self,
        run: &AgentRun,
        observed_at: AgentRunTimestamp,
        cancellation_requested: bool,
    ) -> Result<(), AgentControllerPreflightFailure> {
        if run.state() != AgentControllerState::Execute {
            return Err(AgentControllerPreflightFailure::InvalidState(run.state()));
        }
        if cancellation_requested {
            return Err(AgentControllerPreflightFailure::Cancelled);
        }
        if let Some(exhaustion) = run.budget_exhaustion(observed_at)? {
            return Err(AgentControllerPreflightFailure::BudgetExhausted(exhaustion));
        }
        Ok(())
    }
}

fn next_state(
    current: AgentControllerState,
    signal: AgentControllerSignal,
) -> Result<AgentControllerState, AgentControllerError> {
    match (current, signal) {
        (AgentControllerState::Intake, AgentControllerSignal::AnchorsAccepted) => {
            Ok(AgentControllerState::Localize)
        }
        (AgentControllerState::Localize, AgentControllerSignal::LocalizationComplete) => {
            Ok(AgentControllerState::Plan)
        }
        (AgentControllerState::Plan, AgentControllerSignal::PlanReady) => {
            Ok(AgentControllerState::Execute)
        }
        (AgentControllerState::Execute, AgentControllerSignal::TurnNeedsVerification) => {
            Ok(AgentControllerState::Verify)
        }
        (AgentControllerState::Execute, AgentControllerSignal::ApprovalRequired) => {
            Ok(AgentControllerState::AwaitApproval)
        }
        (AgentControllerState::Verify, AgentControllerSignal::VerificationNeedsExecution) => {
            Ok(AgentControllerState::Execute)
        }
        (AgentControllerState::Verify, AgentControllerSignal::VerificationNeedsReplan) => {
            Ok(AgentControllerState::Replan)
        }
        (AgentControllerState::Replan, AgentControllerSignal::ReplanApplied) => {
            Ok(AgentControllerState::Localize)
        }
        (AgentControllerState::AwaitApproval, AgentControllerSignal::ApprovalGranted) => {
            Ok(AgentControllerState::Execute)
        }
        (AgentControllerState::AwaitApproval, AgentControllerSignal::ApprovalDenied) => {
            Ok(AgentControllerState::Failed)
        }
        (state, AgentControllerSignal::FatalFailure) if !state.is_terminal() => {
            Ok(AgentControllerState::Failed)
        }
        _ => Err(AgentControllerError::InvalidSignal {
            state: current,
            signal,
        }),
    }
}

fn exhaust_budget(
    run: &mut AgentRun,
    event_id: RunEventId,
    snapshot_id: SnapshotId,
    observed_at: AgentRunTimestamp,
    exhaustion: AgentBudgetExhaustion,
) -> Result<AgentControllerAdvance, AgentControllerError> {
    let next = if run.state() == AgentControllerState::Execute {
        AgentControllerState::AwaitApproval
    } else {
        AgentControllerState::Failed
    };
    let code = if exhaustion.dimension() == AgentBudgetDimension::Time {
        RunEventCode::Timeout
    } else {
        RunEventCode::ControllerDecision
    };
    transition(
        run,
        event_id,
        next,
        RunEventPayload::new(code, Some(RunEventOutcome::Failed), None),
        snapshot_id,
        observed_at,
        AgentControllerAdvanceKind::BudgetExhausted(exhaustion),
    )
}

fn transition(
    run: &mut AgentRun,
    event_id: RunEventId,
    next: AgentControllerState,
    payload: RunEventPayload,
    snapshot_id: SnapshotId,
    observed_at: AgentRunTimestamp,
    kind: AgentControllerAdvanceKind,
) -> Result<AgentControllerAdvance, AgentControllerError> {
    let event = run.transition(event_id, next, payload, snapshot_id, observed_at)?;
    Ok(AgentControllerAdvance::new(event, run.state(), kind))
}

fn validate_active_snapshot(
    run: &AgentRun,
    snapshot_id: SnapshotId,
) -> Result<(), AgentControllerError> {
    if run.state().is_terminal() {
        return Err(AgentControllerError::TerminalRun);
    }
    if run.current_snapshot_id() != snapshot_id {
        return Err(AgentControllerError::SnapshotMismatch);
    }
    Ok(())
}

/// Request given only to the deterministic acceptance-verifier port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceVerificationRequest {
    project: ProjectIdentity,
    run_id: AgentRunId,
    goal_contract: GoalContract,
    task_ledger: TaskLedger,
    snapshot_id: SnapshotId,
}

impl AcceptanceVerificationRequest {
    /// Validates exact run anchors and requires every active Ledger step to be Completed.
    pub fn new(
        project: ProjectIdentity,
        run: &AgentRun,
        goal_contract: GoalContract,
        task_ledger: TaskLedger,
    ) -> Result<Self, AcceptanceVerificationRequestError> {
        if run.state() != AgentControllerState::Verify {
            return Err(AcceptanceVerificationRequestError::InvalidRunState);
        }
        if run.goal_contract() != goal_contract.reference()
            || task_ledger.goal_contract() != goal_contract.reference()
            || run.task_ledger_revision() != task_ledger.revision()
        {
            return Err(AcceptanceVerificationRequestError::AnchorMismatch);
        }
        if task_ledger
            .steps()
            .filter(|step| step.is_active_plan_step())
            .any(|step| step.status() != TaskStepStatus::Completed)
        {
            return Err(AcceptanceVerificationRequestError::IncompleteLedger);
        }
        Ok(Self {
            project,
            run_id: run.id(),
            goal_contract,
            task_ledger,
            snapshot_id: run.current_snapshot_id(),
        })
    }

    /// Returns the worktree-scoped project identity.
    #[must_use]
    pub const fn project(&self) -> &ProjectIdentity {
        &self.project
    }

    /// Returns the run requesting successful completion.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact immutable Goal Contract revision.
    #[must_use]
    pub const fn goal_contract(&self) -> &GoalContract {
        &self.goal_contract
    }

    /// Returns the fully verified current Task Ledger.
    #[must_use]
    pub const fn task_ledger(&self) -> &TaskLedger {
        &self.task_ledger
    }

    /// Returns the repository snapshot whose evidence must remain current.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
}

/// Acceptance request was not anchored to a fully completed current plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceVerificationRequestError {
    /// The run was not in Verify.
    InvalidRunState,
    /// Goal or Ledger revisions differed from the run.
    AnchorMismatch,
    /// At least one active step lacked current successful verification.
    IncompleteLedger,
}

impl fmt::Display for AcceptanceVerificationRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRunState => "acceptance verification requires Verify state",
            Self::AnchorMismatch => "acceptance verification anchors do not match the run",
            Self::IncompleteLedger => "acceptance verification requires every active step complete",
        })
    }
}

impl Error for AcceptanceVerificationRequestError {}

/// Content-free deterministic reason current acceptance did not pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceRejection {
    /// At least one criterion lacked sufficient current evidence.
    InsufficientEvidence,
    /// Current evidence contradicted at least one criterion.
    CriterionFailed,
    /// Evidence became stale against the requested snapshot.
    StaleEvidence,
    /// An unresolved blocking hypothesis remained.
    BlockingHypothesis,
}

/// Result returned by the acceptance verifier; only Accepted carries a Done capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceVerifierOutcome {
    /// Every mandatory criterion passed with current evidence.
    Accepted(AcceptanceVerificationReceipt),
    /// Completion is currently blocked for a stable content-free reason.
    Rejected(AcceptanceRejection),
}

/// Owned future returned by the object-safe acceptance-verifier port.
pub type AcceptanceVerifierFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AcceptanceVerifierOutcome, AcceptanceVerifierFailure>>
            + Send
            + 'a,
    >,
>;

/// Positive total deadline for one deterministic acceptance-verifier call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptanceVerifierTimeout(Duration);

impl AcceptanceVerifierTimeout {
    /// Default local acceptance-verification deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(30));

    /// Creates a non-zero deadline capped at two minutes.
    pub fn from_millis(value: u64) -> Result<Self, AcceptanceVerifierTimeoutError> {
        if value == 0 || value > MAX_ACCEPTANCE_VERIFIER_TIMEOUT_MILLIS {
            return Err(AcceptanceVerifierTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the bounded neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Acceptance-verifier deadline was zero or exceeded the fixed local boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptanceVerifierTimeoutError {
    value: u64,
}

impl fmt::Display for AcceptanceVerifierTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "acceptance verifier timeout {} ms must be between 1 and {MAX_ACCEPTANCE_VERIFIER_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for AcceptanceVerifierTimeoutError {}

/// Deterministic evidence-grounded completion capability.
pub trait AcceptanceVerifier: fmt::Debug + Send + Sync {
    /// Verifies every mandatory criterion against the request snapshot and retained evidence.
    fn verify<'a>(
        &'a self,
        request: &'a AcceptanceVerificationRequest,
        timeout: AcceptanceVerifierTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AcceptanceVerifierFuture<'a>;
}

/// Stable acceptance-verifier boundary failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceVerifierFailure {
    /// Required local evidence could not be read.
    Unavailable,
    /// Verifier output violated its typed anchors or coverage contract.
    InvalidResult,
    /// Bounded verification exceeded its deadline.
    TimedOut,
    /// Cooperative cancellation interrupted verification.
    Cancelled,
}

impl fmt::Display for AcceptanceVerifierFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "acceptance verifier is unavailable",
            Self::InvalidResult => "acceptance verifier returned an invalid result",
            Self::TimedOut => "acceptance verifier timed out",
            Self::Cancelled => "acceptance verification was cancelled",
        })
    }
}

impl Error for AcceptanceVerifierFailure {}

/// Verify-state use case that owns the only Application path to Done.
#[derive(Debug, Clone, Copy)]
pub struct VerifyAgentAcceptance<'a> {
    verifier: &'a dyn AcceptanceVerifier,
    timeout: AcceptanceVerifierTimeout,
}

impl<'a> VerifyAgentAcceptance<'a> {
    /// Creates the Done gate from its narrow deterministic verifier capability.
    #[must_use]
    pub const fn new(verifier: &'a dyn AcceptanceVerifier) -> Self {
        Self {
            verifier,
            timeout: AcceptanceVerifierTimeout::DEFAULT,
        }
    }

    /// Verifies completion and applies exactly one resulting transition.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        run: &mut AgentRun,
        request: &AcceptanceVerificationRequest,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<AgentControllerAdvance, AgentControllerError> {
        validate_acceptance_request(run, request)?;
        if control.is_cancelled() {
            return transition(
                run,
                event_id,
                AgentControllerState::Cancelled,
                RunEventPayload::new(
                    RunEventCode::Cancellation,
                    Some(RunEventOutcome::Cancelled),
                    None,
                ),
                request.snapshot_id(),
                observed_at,
                AgentControllerAdvanceKind::Signal(AgentControllerSignal::CancelRequested),
            );
        }
        let outcome = self.verifier.verify(request, self.timeout, control).await;
        if control.is_cancelled() || matches!(outcome, Err(AcceptanceVerifierFailure::Cancelled)) {
            return transition(
                run,
                event_id,
                AgentControllerState::Cancelled,
                RunEventPayload::new(
                    RunEventCode::Cancellation,
                    Some(RunEventOutcome::Cancelled),
                    None,
                ),
                request.snapshot_id(),
                observed_at,
                AgentControllerAdvanceKind::Signal(AgentControllerSignal::CancelRequested),
            );
        }
        match outcome {
            Ok(AcceptanceVerifierOutcome::Accepted(receipt)) => {
                let event = run.complete_verified(
                    event_id,
                    &receipt,
                    RunEventPayload::new(
                        RunEventCode::ControllerDecision,
                        Some(RunEventOutcome::Succeeded),
                        None,
                    ),
                    observed_at,
                )?;
                Ok(AgentControllerAdvance::new(
                    event,
                    run.state(),
                    AgentControllerAdvanceKind::AcceptanceVerified,
                ))
            }
            Ok(AcceptanceVerifierOutcome::Rejected(rejection)) => {
                if let Some(exhaustion) = run.budget_exhaustion(observed_at)? {
                    return exhaust_budget(
                        run,
                        event_id,
                        request.snapshot_id(),
                        observed_at,
                        exhaustion,
                    );
                }
                let next = match rejection {
                    AcceptanceRejection::InsufficientEvidence => AgentControllerState::Execute,
                    AcceptanceRejection::CriterionFailed
                    | AcceptanceRejection::StaleEvidence
                    | AcceptanceRejection::BlockingHypothesis => AgentControllerState::Replan,
                };
                transition(
                    run,
                    event_id,
                    next,
                    RunEventPayload::new(
                        RunEventCode::VerificationFailure,
                        Some(RunEventOutcome::Failed),
                        None,
                    ),
                    request.snapshot_id(),
                    observed_at,
                    AgentControllerAdvanceKind::AcceptanceRejected(rejection),
                )
            }
            Err(failure) => transition(
                run,
                event_id,
                AgentControllerState::Failed,
                RunEventPayload::new(
                    if failure == AcceptanceVerifierFailure::TimedOut {
                        RunEventCode::Timeout
                    } else {
                        RunEventCode::VerificationFailure
                    },
                    Some(RunEventOutcome::Failed),
                    None,
                ),
                request.snapshot_id(),
                observed_at,
                AgentControllerAdvanceKind::AcceptanceVerifierFailed(failure),
            ),
        }
    }
}

fn validate_acceptance_request(
    run: &AgentRun,
    request: &AcceptanceVerificationRequest,
) -> Result<(), AgentControllerError> {
    if run.state().is_terminal() {
        return Err(AgentControllerError::TerminalRun);
    }
    if run.state() != AgentControllerState::Verify
        || run.id() != request.run_id()
        || run.goal_contract() != request.goal_contract().reference()
        || run.task_ledger_revision() != request.task_ledger().revision()
        || run.current_snapshot_id() != request.snapshot_id()
    {
        return Err(AgentControllerError::AcceptanceRequestMismatch);
    }
    Ok(())
}

/// A new model turn could not begin under current state, cancellation, or hard budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentControllerPreflightFailure {
    /// Only Execute may start a model turn.
    InvalidState(AgentControllerState),
    /// Cooperative cancellation was already visible.
    Cancelled,
    /// One hard ceiling was already exhausted.
    BudgetExhausted(AgentBudgetExhaustion),
    /// Wall-clock observation preceded run creation.
    BudgetEvaluation(AgentBudgetEvaluationError),
}

impl fmt::Display for AgentControllerPreflightFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidState(_) => "agent turn requires Execute state",
            Self::Cancelled => "agent turn was cancelled before execution",
            Self::BudgetExhausted(_) => "agent turn budget is exhausted",
            Self::BudgetEvaluation(_) => "agent turn budget timestamp is invalid",
        })
    }
}

impl Error for AgentControllerPreflightFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BudgetEvaluation(error) => Some(error),
            Self::InvalidState(_) | Self::Cancelled | Self::BudgetExhausted(_) => None,
        }
    }
}

impl From<AgentBudgetEvaluationError> for AgentControllerPreflightFailure {
    fn from(value: AgentBudgetEvaluationError) -> Self {
        Self::BudgetEvaluation(value)
    }
}

/// Invalid controller input or rejected domain transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentControllerError {
    /// The run already reached a terminal state.
    TerminalRun,
    /// The observed snapshot differed from the materialized run snapshot.
    SnapshotMismatch,
    /// The signal is not meaningful in the current finite state.
    InvalidSignal {
        /// Current state.
        state: AgentControllerState,
        /// Rejected signal.
        signal: AgentControllerSignal,
    },
    /// Acceptance request no longer matched current materialized anchors.
    AcceptanceRequestMismatch,
    /// A domain transition or receipt invariant rejected the operation.
    Domain(AgentRunError),
    /// Wall-clock observation preceded run creation.
    BudgetEvaluation(AgentBudgetEvaluationError),
}

impl fmt::Display for AgentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TerminalRun => "agent controller run is terminal",
            Self::SnapshotMismatch => "agent controller snapshot does not match the run",
            Self::InvalidSignal { .. } => "agent controller signal is invalid in the current state",
            Self::AcceptanceRequestMismatch => {
                "acceptance verification request no longer matches the run"
            }
            Self::Domain(_) => "agent controller domain transition failed",
            Self::BudgetEvaluation(_) => "agent controller budget timestamp is invalid",
        })
    }
}

impl Error for AgentControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain(error) => Some(error),
            Self::BudgetEvaluation(error) => Some(error),
            Self::TerminalRun
            | Self::SnapshotMismatch
            | Self::InvalidSignal { .. }
            | Self::AcceptanceRequestMismatch => None,
        }
    }
}

impl From<AgentRunError> for AgentControllerError {
    fn from(value: AgentRunError) -> Self {
        Self::Domain(value)
    }
}

impl From<AgentBudgetEvaluationError> for AgentControllerError {
    fn from(value: AgentBudgetEvaluationError) -> Self {
        Self::BudgetEvaluation(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
        AcceptanceCriterionVerification, AcceptanceVerificationReceipt, AgentActionLimit,
        AgentRepairLimit, AgentRunBudget, AgentRunDurationLimit, AgentTokenLimit, AgentTurnCharge,
        AgentTurnLimit, AgentTurnRepairUsage, CanonicalDirectory, ExpectedTaskEvidence, GitHead,
        GitReferenceName, GoalContractDraft, GoalContractTimestamp, GoalObjective, ModelProfileId,
        ModelProfileReference, ModelProfileVersion, ModelTokenCount, RepositoryId,
        RepositoryIdentity, StepVerification, StepVerificationId, StepVerificationOutcome,
        SuccessVerification, TaskId, TaskLedgerRevision, TaskLedgerTimestamp, TaskStepDefinition,
        TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod,
        VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;

    #[derive(Debug)]
    struct NeverCancelled;

    impl AgentControllerControl for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    #[derive(Debug, Clone)]
    struct StaticVerifier(AcceptanceVerifierOutcome);

    impl AcceptanceVerifier for StaticVerifier {
        fn verify<'a>(
            &'a self,
            _request: &'a AcceptanceVerificationRequest,
            _timeout: AcceptanceVerifierTimeout,
            _control: &'a dyn AgentControllerControl,
        ) -> AcceptanceVerifierFuture<'a> {
            Box::pin(async move { Ok(self.0.clone()) })
        }
    }

    #[test]
    fn signal_matrix_is_exhaustive_and_never_grants_done() {
        let states = [
            AgentControllerState::Intake,
            AgentControllerState::Localize,
            AgentControllerState::Plan,
            AgentControllerState::Execute,
            AgentControllerState::Verify,
            AgentControllerState::Replan,
            AgentControllerState::AwaitApproval,
            AgentControllerState::Done,
            AgentControllerState::Failed,
            AgentControllerState::Cancelled,
        ];
        let signals = [
            AgentControllerSignal::AnchorsAccepted,
            AgentControllerSignal::LocalizationComplete,
            AgentControllerSignal::PlanReady,
            AgentControllerSignal::TurnNeedsVerification,
            AgentControllerSignal::ApprovalRequired,
            AgentControllerSignal::VerificationNeedsExecution,
            AgentControllerSignal::VerificationNeedsReplan,
            AgentControllerSignal::ReplanApplied,
            AgentControllerSignal::ApprovalGranted,
            AgentControllerSignal::ApprovalDenied,
            AgentControllerSignal::FatalFailure,
            AgentControllerSignal::CancelRequested,
        ];
        let expected_successes = [
            (
                AgentControllerState::Intake,
                AgentControllerSignal::AnchorsAccepted,
            ),
            (
                AgentControllerState::Localize,
                AgentControllerSignal::LocalizationComplete,
            ),
            (AgentControllerState::Plan, AgentControllerSignal::PlanReady),
            (
                AgentControllerState::Execute,
                AgentControllerSignal::TurnNeedsVerification,
            ),
            (
                AgentControllerState::Execute,
                AgentControllerSignal::ApprovalRequired,
            ),
            (
                AgentControllerState::Verify,
                AgentControllerSignal::VerificationNeedsExecution,
            ),
            (
                AgentControllerState::Verify,
                AgentControllerSignal::VerificationNeedsReplan,
            ),
            (
                AgentControllerState::Replan,
                AgentControllerSignal::ReplanApplied,
            ),
            (
                AgentControllerState::AwaitApproval,
                AgentControllerSignal::ApprovalGranted,
            ),
            (
                AgentControllerState::AwaitApproval,
                AgentControllerSignal::ApprovalDenied,
            ),
        ];

        for state in states {
            for signal in signals {
                let result = next_state(state, signal);
                let expected = expected_successes.contains(&(state, signal))
                    || signal == AgentControllerSignal::FatalFailure && !state.is_terminal();
                assert_eq!(result.is_ok(), expected, "{state:?} + {signal:?}");
                assert_ne!(result.ok(), Some(AgentControllerState::Done));
            }
        }
    }

    #[test]
    fn exhausted_execute_run_waits_once_then_fails_instead_of_looping() -> Result<(), Box<dyn Error>>
    {
        let goal = goal()?;
        let budget = AgentRunBudget::new(
            AgentTurnLimit::new(1)?,
            AgentTokenLimit::new(100)?,
            AgentTokenLimit::new(100)?,
            AgentActionLimit::new(1)?,
            AgentRunDurationLimit::from_millis(1_000)?,
            AgentRepairLimit::new(1)?,
        );
        let (mut run, _) = AgentRun::start_with_budget(
            run_id(),
            goal.reference(),
            TaskLedgerRevision::INITIAL,
            model_profile(),
            budget,
            snapshot(),
            event_id(1),
            timestamp(1)?,
        )?;
        reach_execute(&mut run)?;
        run.record_turn(
            event_id(5),
            RunEventPayload::empty(),
            snapshot(),
            timestamp(5)?,
            AgentTurnCharge::new(
                ModelTokenCount::new(10),
                ModelTokenCount::new(2),
                None,
                AgentTurnRepairUsage::None,
            ),
        )?;

        let waiting = AdvanceAgentController.execute(
            &mut run,
            AgentControllerSignal::AnchorsAccepted,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            false,
        )?;
        assert_eq!(waiting.state(), AgentControllerState::AwaitApproval);
        assert!(matches!(
            waiting.kind(),
            AgentControllerAdvanceKind::BudgetExhausted(_)
        ));

        let failed = AdvanceAgentController.execute(
            &mut run,
            AgentControllerSignal::ApprovalGranted,
            event_id(7),
            snapshot(),
            timestamp(7)?,
            false,
        )?;
        assert_eq!(failed.state(), AgentControllerState::Failed);
        Ok(())
    }

    #[test]
    fn only_acceptance_verifier_receipt_can_reach_done() -> Result<(), Box<dyn Error>> {
        let goal = goal()?;
        let ledger = completed_ledger(&goal)?;
        let (mut run, _) = AgentRun::start(
            run_id(),
            goal.reference(),
            ledger.revision(),
            model_profile(),
            snapshot(),
            event_id(1),
            timestamp(1)?,
        )?;
        reach_execute(&mut run)?;
        run.transition(
            event_id(5),
            AgentControllerState::Verify,
            RunEventPayload::empty(),
            snapshot(),
            timestamp(5)?,
        )?;
        assert_eq!(
            run.transition(
                event_id(6),
                AgentControllerState::Done,
                RunEventPayload::empty(),
                snapshot(),
                timestamp(6)?,
            ),
            Err(AgentRunError::AcceptanceVerificationRequired)
        );
        let receipt = acceptance_receipt(&goal, &ledger)?;
        let request = AcceptanceVerificationRequest::new(project()?, &run, goal, ledger.clone())?;
        let verifier = StaticVerifier(AcceptanceVerifierOutcome::Accepted(receipt));

        let advance = block_on(VerifyAgentAcceptance::new(&verifier).execute(
            &mut run,
            &request,
            event_id(6),
            timestamp(6)?,
            &NeverCancelled,
        ))?;

        assert_eq!(advance.state(), AgentControllerState::Done);
        assert_eq!(
            advance.kind(),
            AgentControllerAdvanceKind::AcceptanceVerified
        );
        assert_eq!(ledger.steps().count(), 1);
        assert_eq!(
            request.task_ledger().revision(),
            TaskLedgerRevision::INITIAL
        );
        Ok(())
    }

    fn reach_execute(run: &mut AgentRun) -> Result<(), AgentRunError> {
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
                AgentRunTimestamp::from_unix_millis(u64::from(id))
                    .map_err(|_| AgentRunError::TimestampRegressed)?,
            )?;
        }
        Ok(())
    }

    fn completed_ledger(goal: &GoalContract) -> Result<TaskLedger, Box<dyn Error>> {
        let step_id = TaskStepId::from_bytes([5; 32]);
        let evidence_id = a3_domain::TaskEvidenceId::from_bytes([7; 32]);
        let spec = VerificationSpec::new(
            VerificationSpecId::from_bytes([6; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string("targeted tests pass".to_owned())?,
        );
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![TaskStepDefinition::new(
                step_id,
                None,
                TaskStepOutcome::try_from_string("controller is verified".to_owned())?,
                TaskStepRationale::try_from_string("prove the finite state gate".to_owned())?,
                Vec::new(),
                vec![ExpectedTaskEvidence::try_from_string(
                    "test receipt".to_owned(),
                )?],
                spec,
            )?],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        ledger.start_step(step_id, run_id(), TaskLedgerTimestamp::from_unix_millis(2)?)?;
        ledger.begin_step_verification(
            step_id,
            run_id(),
            None,
            vec![evidence_id],
            TaskLedgerTimestamp::from_unix_millis(3)?,
        )?;
        ledger.finish_step_verification(
            step_id,
            StepVerification::new(
                StepVerificationId::from_bytes([8; 32]),
                VerificationSpecId::from_bytes([6; 32]),
                run_id(),
                StepVerificationOutcome::Passed,
                vec![evidence_id],
                TaskLedgerTimestamp::from_unix_millis(4)?,
            )?,
        )?;
        Ok(ledger)
    }

    fn acceptance_receipt(
        goal: &GoalContract,
        ledger: &TaskLedger,
    ) -> Result<AcceptanceVerificationReceipt, Box<dyn Error>> {
        Ok(AcceptanceVerificationReceipt::new(
            run_id(),
            goal,
            ledger.revision(),
            snapshot(),
            vec![AcceptanceCriterionVerification::new(
                AcceptanceCriterionId::from_bytes([3; 32]),
                vec![a3_domain::TaskEvidenceId::from_bytes([7; 32])],
            )?],
        )?)
    }

    fn goal() -> Result<GoalContract, Box<dyn Error>> {
        Ok(GoalContract::initial(
            TaskId::from_bytes([2; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("complete only after verification".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([3; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "the controller contract passes".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run controller tests".to_owned())?,
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

    const fn model_profile() -> ModelProfileReference {
        ModelProfileReference::new(
            ModelProfileId::from_bytes([10; 32]),
            ModelProfileVersion::V1,
        )
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
