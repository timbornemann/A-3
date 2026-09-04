use crate::{
    AdvanceAgentController, AgentControllerAdvance, AgentControllerControl, AgentControllerError,
    AgentControllerSignal, TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, AgentFinishAction, AgentLedgerUpdate, AgentRun, AgentRunTimestamp,
    AgentToolEvidenceSet, AgentUpdateLedgerAction, ProjectIdentity, RunEvent, RunEventCode,
    RunEventId, RunEventOutcome, RunEventPayload, RunEventSequence, SnapshotId, TaskLedger,
    TaskLedgerError, TaskLedgerTimestamp, TaskReplanReason, TaskStepDefinition, TaskStepId,
    TaskStepStatus,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_LEDGER_TOOL_EVIDENCE: usize = 64;

/// Owned future returned by the atomic agent-action persistence boundary.
pub type AgentActionStoreFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TaskLedgerStoreVersion, AgentActionStoreFailure>> + Send + 'a>,
>;

/// Storage boundary atomically replacing a Ledger and appending its resulting run event.
pub trait AgentActionStore: fmt::Debug + Send + Sync {
    /// Commits both compare-and-swap dimensions in one storage transaction.
    #[allow(clippy::too_many_arguments)]
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a>;
}

/// Stable failure of an atomic Ledger/run action commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActionStoreFailure {
    /// Local worktree storage could not be reached or written.
    Unavailable,
    /// Local worktree storage failed integrity checks.
    Corrupt,
    /// Database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated relational or domain invariants.
    InvalidStoredData,
    /// The durable task or Ledger anchor does not exist.
    TaskNotFound,
    /// The durable run or one of its immutable anchors does not exist.
    RunNotFound,
    /// Another writer already replaced the Ledger projection.
    LedgerVersionConflict,
    /// Another writer already appended the next run event.
    RunSequenceConflict,
}

impl fmt::Display for AgentActionStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "agent action storage is unavailable",
            Self::Corrupt => "agent action storage is corrupt",
            Self::UnsupportedSchema => "agent action schema is newer than this build",
            Self::InvalidStoredData => "agent action storage contains invalid data",
            Self::TaskNotFound => "agent action Task Ledger anchor was not found",
            Self::RunNotFound => "agent action run anchor was not found",
            Self::LedgerVersionConflict => "agent action Ledger version changed concurrently",
            Self::RunSequenceConflict => "agent action run sequence changed concurrently",
        })
    }
}

impl Error for AgentActionStoreFailure {}

/// Inbound boundary atomically persisting one Ledger-mutating `UpdateLedger` result.
#[derive(Debug, Clone, Copy)]
pub struct PersistAgentLedgerMutation<'a> {
    store: &'a dyn AgentActionStore,
}

impl<'a> PersistAgentLedgerMutation<'a> {
    /// Creates the use case from the atomic action persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }

    /// Persists only outcomes that actually changed the Task Ledger aggregate.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &TaskLedger,
        run: &AgentRun,
        outcome: &AgentLedgerActionOutcome,
    ) -> Result<TaskLedgerStoreVersion, PersistAgentLedgerMutationError> {
        if !matches!(
            outcome.kind(),
            AgentLedgerActionOutcomeKind::VerificationPrepared
                | AgentLedgerActionOutcomeKind::Blocked
                | AgentLedgerActionOutcomeKind::ReplanRequested(_)
        ) {
            return Err(PersistAgentLedgerMutationError::LedgerWasNotMutated);
        }
        self.store
            .commit_ledger_action(
                project,
                expected_ledger_version,
                expected_last_sequence,
                ledger,
                run,
                outcome.advance().event(),
            )
            .await
            .map_err(PersistAgentLedgerMutationError::Storage)
    }
}

/// A Ledger mutation could not be admitted to the atomic persistence boundary.
#[derive(Debug)]
pub enum PersistAgentLedgerMutationError {
    /// Replan, cancellation, and Finish do not replace the Ledger projection here.
    LedgerWasNotMutated,
    /// Atomic adapter commit failed.
    Storage(AgentActionStoreFailure),
}

impl fmt::Display for PersistAgentLedgerMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LedgerWasNotMutated => "agent action did not mutate the Task Ledger",
            Self::Storage(_) => "agent Ledger mutation could not be persisted atomically",
        })
    }
}

impl Error for PersistAgentLedgerMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LedgerWasNotMutated => None,
            Self::Storage(error) => Some(error),
        }
    }
}

/// Applied non-verifying Ledger intent and its finite controller transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLedgerActionOutcome {
    kind: AgentLedgerActionOutcomeKind,
    advance: AgentControllerAdvance,
}

impl AgentLedgerActionOutcome {
    /// Returns the exact safe Ledger intent that was materialized or retained.
    #[must_use]
    pub const fn kind(&self) -> &AgentLedgerActionOutcomeKind {
        &self.kind
    }

    /// Returns the resulting controller transition that must be journaled.
    #[must_use]
    pub const fn advance(&self) -> &AgentControllerAdvance {
        &self.advance
    }

    /// Consumes the result into its safe classification and journal transition.
    #[must_use]
    pub fn into_parts(self) -> (AgentLedgerActionOutcomeKind, AgentControllerAdvance) {
        (self.kind, self.advance)
    }
}

/// Stable result class of one safe `UpdateLedger` action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLedgerActionOutcomeKind {
    /// Current step moved only to Verifying with controller-owned source evidence.
    VerificationPrepared,
    /// Current attempt and run stopped with the bounded blocker.
    Blocked,
    /// The reason was retained for the controller's validated Replan path.
    ReplanRequested(TaskReplanReason),
    /// Cancellation won before any Ledger mutation.
    Cancelled,
}

/// Applies one model-selected Ledger intent without granting verification or completion.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyAgentLedgerUpdate;

impl ApplyAgentLedgerUpdate {
    /// Validates all anchors on copies and commits both in-memory aggregates only on success.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        self,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        action: &AgentUpdateLedgerAction,
        evidence: Option<&AgentToolEvidenceSet>,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<AgentLedgerActionOutcome, ApplyAgentLedgerUpdateError> {
        validate_anchors(run, ledger, action.step_id(), snapshot_id)?;
        let mut next_run = run.clone();
        if control.is_cancelled() {
            let advance = AdvanceAgentController.execute(
                &mut next_run,
                AgentControllerSignal::CancelRequested,
                event_id,
                snapshot_id,
                observed_at,
                true,
            )?;
            *run = next_run;
            return Ok(AgentLedgerActionOutcome {
                kind: AgentLedgerActionOutcomeKind::Cancelled,
                advance,
            });
        }

        let mut next_ledger = ledger.clone();
        let timestamp = TaskLedgerTimestamp::from_unix_millis(observed_at.unix_millis())
            .map_err(|_| ApplyAgentLedgerUpdateError::InvalidTimestamp)?;
        let (kind, signal) = match action.update() {
            AgentLedgerUpdate::RecordResult(summary) => {
                let evidence = evidence.ok_or(ApplyAgentLedgerUpdateError::EvidenceRequired)?;
                if evidence.snapshot_id() != snapshot_id {
                    return Err(ApplyAgentLedgerUpdateError::EvidenceSnapshotMismatch);
                }
                if evidence.is_empty() {
                    return Err(ApplyAgentLedgerUpdateError::EvidenceRequired);
                }
                if evidence.evidence().len() > MAX_LEDGER_TOOL_EVIDENCE {
                    return Err(ApplyAgentLedgerUpdateError::TooMuchEvidence {
                        actual: evidence.evidence().len(),
                    });
                }
                let evidence_ids = evidence
                    .evidence()
                    .iter()
                    .map(a3_domain::AgentToolEvidence::id)
                    .collect();
                next_ledger.begin_step_verification(
                    action.step_id(),
                    run.id(),
                    Some(summary.clone()),
                    evidence_ids,
                    timestamp,
                )?;
                (
                    AgentLedgerActionOutcomeKind::VerificationPrepared,
                    AgentControllerSignal::TurnNeedsVerification,
                )
            }
            AgentLedgerUpdate::ReportBlocked(reason) => {
                next_ledger.block_step(action.step_id(), run.id(), reason.clone(), timestamp)?;
                (
                    AgentLedgerActionOutcomeKind::Blocked,
                    AgentControllerSignal::FatalFailure,
                )
            }
            AgentLedgerUpdate::RequestReplan(reason) => {
                next_ledger.block_step(
                    action.step_id(),
                    run.id(),
                    a3_domain::TaskStepBlockingReason::from_replan_reason(reason),
                    timestamp,
                )?;
                (
                    AgentLedgerActionOutcomeKind::ReplanRequested(reason.clone()),
                    AgentControllerSignal::ExecutionNeedsReplan,
                )
            }
        };
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            signal,
            event_id,
            snapshot_id,
            observed_at,
            false,
        )?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(AgentLedgerActionOutcome { kind, advance })
    }
}

/// Applies the content-free `Finish` request without granting success.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestAgentFinish;

impl RequestAgentFinish {
    /// Moves Execute to Verify; only the existing AcceptanceVerifier may later grant Done.
    pub fn execute(
        self,
        run: &mut AgentRun,
        _action: AgentFinishAction,
        event_id: RunEventId,
        snapshot_id: SnapshotId,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<AgentControllerAdvance, AgentControllerError> {
        AdvanceAgentController.execute(
            run,
            AgentControllerSignal::TurnNeedsVerification,
            event_id,
            snapshot_id,
            observed_at,
            control.is_cancelled(),
        )
    }
}

fn validate_anchors(
    run: &AgentRun,
    ledger: &TaskLedger,
    step_id: TaskStepId,
    snapshot_id: SnapshotId,
) -> Result<(), ApplyAgentLedgerUpdateError> {
    if run.state() != AgentControllerState::Execute {
        return Err(ApplyAgentLedgerUpdateError::InvalidRunState);
    }
    if run.current_snapshot_id() != snapshot_id {
        return Err(ApplyAgentLedgerUpdateError::SnapshotMismatch);
    }
    if run.goal_contract() != ledger.goal_contract()
        || run.task_ledger_revision() != ledger.revision()
        || ledger.step(step_id).is_none()
    {
        return Err(ApplyAgentLedgerUpdateError::AnchorMismatch);
    }
    Ok(())
}

/// A safe Ledger action failed before either aggregate was changed.
#[derive(Debug)]
pub enum ApplyAgentLedgerUpdateError {
    /// Run was not in Execute.
    InvalidRunState,
    /// Requested snapshot differed from the run anchor.
    SnapshotMismatch,
    /// Goal, Ledger revision, or current step differed.
    AnchorMismatch,
    /// `RecordResult` lacked controller-owned current evidence.
    EvidenceRequired,
    /// Tool evidence belonged to another snapshot.
    EvidenceSnapshotMismatch,
    /// More evidence existed than one Task Ledger attempt can retain.
    TooMuchEvidence {
        /// Observed source count.
        actual: usize,
    },
    /// Run audit time could not be represented as a Ledger timestamp.
    InvalidTimestamp,
    /// The Task Ledger rejected the requested materialization.
    Ledger(TaskLedgerError),
    /// The finite controller rejected the resulting transition.
    Controller(AgentControllerError),
}

impl fmt::Display for ApplyAgentLedgerUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRunState => "agent Ledger update requires Execute state",
            Self::SnapshotMismatch => "agent Ledger update snapshot does not match the run",
            Self::AnchorMismatch => "agent Ledger update does not match run and Ledger anchors",
            Self::EvidenceRequired => "agent result update requires current tool evidence",
            Self::EvidenceSnapshotMismatch => "agent result evidence belongs to another snapshot",
            Self::TooMuchEvidence { .. } => "agent result exceeds the Ledger evidence boundary",
            Self::InvalidTimestamp => "agent Ledger update timestamp is invalid",
            Self::Ledger(_) => "agent Ledger update violated a Task Ledger invariant",
            Self::Controller(_) => "agent Ledger update controller transition was rejected",
        })
    }
}

impl Error for ApplyAgentLedgerUpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ledger(error) => Some(error),
            Self::Controller(error) => Some(error),
            Self::InvalidRunState
            | Self::SnapshotMismatch
            | Self::AnchorMismatch
            | Self::EvidenceRequired
            | Self::EvidenceSnapshotMismatch
            | Self::TooMuchEvidence { .. }
            | Self::InvalidTimestamp => None,
        }
    }
}

impl From<TaskLedgerError> for ApplyAgentLedgerUpdateError {
    fn from(value: TaskLedgerError) -> Self {
        Self::Ledger(value)
    }
}

impl From<AgentControllerError> for ApplyAgentLedgerUpdateError {
    fn from(value: AgentControllerError) -> Self {
        Self::Controller(value)
    }
}

/// Atomically installs one immediate, Core-validated Task Ledger revision while a run replans.
#[derive(Debug, Clone, Copy)]
pub struct ApplyAgentPlanRevision<'a> {
    store: &'a dyn AgentActionStore,
}

impl<'a> ApplyAgentPlanRevision<'a> {
    /// Creates the use case from the existing atomic Ledger/run persistence boundary.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }

    /// Retires only non-active historical candidates and advances the run's exact Ledger anchor.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        retire_step_ids: Vec<TaskStepId>,
        additions: Vec<TaskStepDefinition>,
        reason: TaskReplanReason,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<TaskLedgerStoreVersion, ApplyAgentPlanRevisionError> {
        if control.is_cancelled() {
            return Err(ApplyAgentPlanRevisionError::Cancelled);
        }
        if run.state() != AgentControllerState::Replan
            || run.goal_contract() != ledger.goal_contract()
            || run.task_ledger_revision() != ledger.revision()
        {
            return Err(ApplyAgentPlanRevisionError::InvalidState);
        }
        let timestamp = TaskLedgerTimestamp::from_unix_millis(observed_at.unix_millis())
            .map_err(|_| ApplyAgentPlanRevisionError::InvalidTimestamp)?;
        let mut next_ledger = ledger.clone();
        let next_revision = next_ledger.replan(retire_step_ids, additions, reason, timestamp)?;
        let mut next_run = run.clone();
        let event = next_run.record_ledger_update(
            event_id,
            next_revision,
            RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            run.current_snapshot_id(),
            observed_at,
        )?;
        let next_version = self
            .store
            .commit_ledger_action(
                project,
                expected_ledger_version,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                &event,
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(next_version)
    }
}

/// A proposed immediate Agent plan revision failed before any partial state became authoritative.
#[derive(Debug)]
pub enum ApplyAgentPlanRevisionError {
    /// Run and Ledger were not the exact current replan pair.
    InvalidState,
    /// Cancellation won before the atomic persistence boundary.
    Cancelled,
    /// The monotone timestamp could not be represented.
    InvalidTimestamp,
    /// The new graph violated a Task Ledger invariant.
    Ledger(TaskLedgerError),
    /// The run rejected the immediate Ledger revision event.
    Run(a3_domain::AgentRunError),
    /// Compare-and-swap persistence rejected the update.
    Storage(AgentActionStoreFailure),
}

impl fmt::Display for ApplyAgentPlanRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidState => "Agent plan revision is not anchored to the current replan",
            Self::Cancelled => "Agent plan revision was cancelled",
            Self::InvalidTimestamp => "Agent plan revision timestamp is invalid",
            Self::Ledger(_) => "Agent plan revision violates the Task Ledger",
            Self::Run(_) => "Agent plan revision violates the durable run",
            Self::Storage(_) => "Agent plan revision could not be committed atomically",
        })
    }
}

impl Error for ApplyAgentPlanRevisionError {}

impl From<TaskLedgerError> for ApplyAgentPlanRevisionError {
    fn from(value: TaskLedgerError) -> Self {
        Self::Ledger(value)
    }
}

impl From<a3_domain::AgentRunError> for ApplyAgentPlanRevisionError {
    fn from(value: a3_domain::AgentRunError) -> Self {
        Self::Run(value)
    }
}

impl From<AgentActionStoreFailure> for ApplyAgentPlanRevisionError {
    fn from(value: AgentActionStoreFailure) -> Self {
        Self::Storage(value)
    }
}

/// Result of advancing a verified multi-step Agent plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueVerifiedAgentPlanOutcome {
    /// Every current plan step is verified, so acceptance may run.
    ReadyForAcceptance,
    /// Exactly one further step now owns the existing Agent run.
    StepStarted {
        /// Stable identity of the newly active step.
        step_id: TaskStepId,
        /// Optimistic store version after the atomic Ledger/run transition.
        ledger_version: TaskLedgerStoreVersion,
    },
}

/// Atomically starts the next ready Ledger step after the preceding step was verified.
#[derive(Debug, Clone, Copy)]
pub struct ContinueVerifiedAgentPlan<'a> {
    store: &'a dyn AgentActionStore,
}

impl<'a> ContinueVerifiedAgentPlan<'a> {
    /// Creates the use case from the existing atomic Ledger/run persistence boundary.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }

    /// Preserves one run and one mutating action at a time while progressing a multi-step plan.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<ContinueVerifiedAgentPlanOutcome, ContinueVerifiedAgentPlanError> {
        if control.is_cancelled() {
            return Err(ContinueVerifiedAgentPlanError::Cancelled);
        }
        if run.state() != AgentControllerState::Verify
            || run.goal_contract() != ledger.goal_contract()
            || run.task_ledger_revision() != ledger.revision()
            || ledger.steps().any(|step| {
                matches!(
                    step.status(),
                    TaskStepStatus::InProgress
                        | TaskStepStatus::AwaitingApproval
                        | TaskStepStatus::Verifying
                )
            })
        {
            return Err(ContinueVerifiedAgentPlanError::InvalidState);
        }
        let next_step_id = ledger
            .steps()
            .filter(|step| step.is_active_plan_step() && step.status() == TaskStepStatus::Ready)
            .map(|step| step.definition().id())
            .next();
        let Some(next_step_id) = next_step_id else {
            if ledger
                .steps()
                .filter(|step| step.is_active_plan_step())
                .all(|step| step.status() == TaskStepStatus::Completed)
            {
                return Ok(ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance);
            }
            return Err(ContinueVerifiedAgentPlanError::InvalidState);
        };

        let timestamp = TaskLedgerTimestamp::from_unix_millis(observed_at.unix_millis())
            .map_err(|_| ContinueVerifiedAgentPlanError::InvalidTimestamp)?;
        let mut next_ledger = ledger.clone();
        next_ledger.start_step(next_step_id, run.id(), timestamp)?;
        let mut next_run = run.clone();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::VerificationNeedsExecution,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            false,
        )?;
        let next_version = self
            .store
            .commit_ledger_action(
                project,
                expected_ledger_version,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(ContinueVerifiedAgentPlanOutcome::StepStarted {
            step_id: next_step_id,
            ledger_version: next_version,
        })
    }
}

/// A verified plan could not safely advance to its next independently verified step.
#[derive(Debug)]
pub enum ContinueVerifiedAgentPlanError {
    /// The run or Ledger was not in the exact post-verification state.
    InvalidState,
    /// Cancellation won before any aggregate or persistence mutation.
    Cancelled,
    /// The monotone timestamp could not be represented by the Ledger.
    InvalidTimestamp,
    /// Starting the next ready step violated a Ledger invariant.
    Ledger(TaskLedgerError),
    /// The finite controller rejected Verify to Execute.
    Controller(AgentControllerError),
    /// The atomic compare-and-swap persistence boundary rejected the transition.
    Storage(AgentActionStoreFailure),
}

impl fmt::Display for ContinueVerifiedAgentPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidState => "verified Agent plan is not ready to advance",
            Self::Cancelled => "verified Agent plan advance was cancelled",
            Self::InvalidTimestamp => "verified Agent plan timestamp is invalid",
            Self::Ledger(_) => "verified Agent plan violated a Task Ledger invariant",
            Self::Controller(_) => "verified Agent plan controller transition was rejected",
            Self::Storage(_) => "verified Agent plan advance could not be persisted atomically",
        })
    }
}

impl Error for ContinueVerifiedAgentPlanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ledger(error) => Some(error),
            Self::Controller(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::InvalidState | Self::Cancelled | Self::InvalidTimestamp => None,
        }
    }
}

impl From<TaskLedgerError> for ContinueVerifiedAgentPlanError {
    fn from(value: TaskLedgerError) -> Self {
        Self::Ledger(value)
    }
}

impl From<AgentControllerError> for ContinueVerifiedAgentPlanError {
    fn from(value: AgentControllerError) -> Self {
        Self::Controller(value)
    }
}

impl From<AgentActionStoreFailure> for ContinueVerifiedAgentPlanError {
    fn from(value: AgentActionStoreFailure) -> Self {
        Self::Storage(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
        AgentToolEvidence, CanonicalDirectory, ContentHash, EvidenceRef, ExpectedTaskEvidence,
        GitHead, GitReferenceName, GoalContract, GoalContractDraft, GoalContractTimestamp,
        GoalObjective, ModelProfileId, ModelProfileReference, ModelProfileVersion, RepositoryId,
        RepositoryIdentity, RepositoryPath, SourcePosition, SourceRange, StepDependency,
        StepVerification, StepVerificationId, StepVerificationOutcome, SuccessVerification,
        TaskEvidenceId, TaskId, TaskStepBlockingReason, TaskStepDefinition, TaskStepOutcome,
        TaskStepRationale, TaskStepResultSummary, TaskStepStatus, VerificationMethod,
        VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct Active;

    impl AgentControllerControl for Active {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    #[derive(Debug)]
    struct Cancelled;

    impl AgentControllerControl for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[derive(Debug, Default)]
    struct RecordingActionStore {
        commits: Mutex<u32>,
    }

    impl AgentActionStore for RecordingActionStore {
        fn commit_ledger_action<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            expected_ledger_version: TaskLedgerStoreVersion,
            _expected_last_sequence: RunEventSequence,
            _ledger: &'a TaskLedger,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
        ) -> AgentActionStoreFuture<'a> {
            Box::pin(async move {
                let mut commits = self
                    .commits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *commits = commits.saturating_add(1);
                TaskLedgerStoreVersion::new(expected_ledger_version.get().saturating_add(1))
                    .map_err(|_| AgentActionStoreFailure::Unavailable)
            })
        }
    }

    #[test]
    fn result_only_prepares_verification_with_controller_evidence() -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let evidence = evidence(snapshot())?;
        let action = AgentUpdateLedgerAction::new(
            step_id,
            AgentLedgerUpdate::RecordResult(TaskStepResultSummary::try_from_string(
                "read-only architecture answer prepared".to_owned(),
            )?),
        );
        let outcome = ApplyAgentLedgerUpdate.execute(
            &mut run,
            &mut ledger,
            &action,
            Some(&evidence),
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Active,
        )?;

        assert_eq!(run.state(), AgentControllerState::Verify);
        assert_eq!(
            ledger.step(step_id).map(|step| step.status()),
            Some(TaskStepStatus::Verifying)
        );
        assert_eq!(
            ledger
                .step(step_id)
                .and_then(|step| step.attempts().last())
                .map(|attempt| attempt.evidence_ids()),
            Some(&[evidence.evidence()[0].id()][..])
        );
        assert_eq!(
            outcome.kind(),
            &AgentLedgerActionOutcomeKind::VerificationPrepared
        );
        assert_ne!(run.state(), AgentControllerState::Done);
        Ok(())
    }

    #[test]
    fn stale_result_evidence_changes_neither_aggregate() -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let original_run = run.clone();
        let original_ledger = ledger.clone();
        let action = AgentUpdateLedgerAction::new(
            step_id,
            AgentLedgerUpdate::RecordResult(TaskStepResultSummary::try_from_string(
                "stale result".to_owned(),
            )?),
        );

        assert!(matches!(
            ApplyAgentLedgerUpdate.execute(
                &mut run,
                &mut ledger,
                &action,
                Some(&evidence(SnapshotId::from_bytes([99; 32]))?),
                event_id(6),
                snapshot(),
                timestamp(6)?,
                &Active,
            ),
            Err(ApplyAgentLedgerUpdateError::EvidenceSnapshotMismatch)
        ));
        assert_eq!(run, original_run);
        assert_eq!(ledger, original_ledger);
        Ok(())
    }

    #[test]
    fn finish_can_request_verify_but_cannot_grant_done() -> Result<(), Box<dyn Error>> {
        let (mut run, _, _) = fixture()?;

        let advance = RequestAgentFinish.execute(
            &mut run,
            AgentFinishAction,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Active,
        )?;

        assert_eq!(advance.state(), AgentControllerState::Verify);
        assert_ne!(advance.state(), AgentControllerState::Done);
        Ok(())
    }

    #[test]
    fn blocked_update_stops_both_step_and_run_without_claiming_success()
    -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let outcome = ApplyAgentLedgerUpdate.execute(
            &mut run,
            &mut ledger,
            &AgentUpdateLedgerAction::new(
                step_id,
                AgentLedgerUpdate::ReportBlocked(TaskStepBlockingReason::try_from_string(
                    "required local evidence is unavailable".to_owned(),
                )?),
            ),
            None,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Active,
        )?;

        assert_eq!(outcome.kind(), &AgentLedgerActionOutcomeKind::Blocked);
        assert_eq!(run.state(), AgentControllerState::Failed);
        assert_eq!(
            ledger.step(step_id).map(|step| step.status()),
            Some(TaskStepStatus::Blocked)
        );
        Ok(())
    }

    #[test]
    fn replan_request_closes_the_attempt_before_the_plan_revision() -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let reason =
            TaskReplanReason::try_from_string("the inspected dependency changed shape".to_owned())?;
        let outcome = ApplyAgentLedgerUpdate.execute(
            &mut run,
            &mut ledger,
            &AgentUpdateLedgerAction::new(
                step_id,
                AgentLedgerUpdate::RequestReplan(reason.clone()),
            ),
            None,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Active,
        )?;

        assert_eq!(
            outcome.kind(),
            &AgentLedgerActionOutcomeKind::ReplanRequested(reason)
        );
        assert_eq!(
            ledger.step(step_id).map(|step| step.status()),
            Some(TaskStepStatus::Blocked)
        );
        assert_eq!(run.state(), AgentControllerState::Replan);
        assert!(ledger.replans().is_empty());
        Ok(())
    }

    #[test]
    fn plan_revision_is_applied_atomically_after_a_replan_request() -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let reason =
            TaskReplanReason::try_from_string("an additional adapter is required".to_owned())?;
        let outcome = ApplyAgentLedgerUpdate.execute(
            &mut run,
            &mut ledger,
            &AgentUpdateLedgerAction::new(
                step_id,
                AgentLedgerUpdate::RequestReplan(reason.clone()),
            ),
            None,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Active,
        )?;
        let store = RecordingActionStore::default();
        let replacement_id = TaskStepId::from_bytes([31; 32]);
        let replacement = TaskStepDefinition::new(
            replacement_id,
            None,
            TaskStepOutcome::try_from_string("implement the required adapter".to_owned())?,
            TaskStepRationale::try_from_string("new evidence changed the plan".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "adapter source and verification result".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([32; 32]),
                VerificationMethod::Diagnostic,
                VerificationRequirement::try_from_string("run the adapter test".to_owned())?,
            ),
        )?;
        let version = block_on(ApplyAgentPlanRevision::new(&store).execute(
            &project()?,
            TaskLedgerStoreVersion::INITIAL,
            &mut run,
            &mut ledger,
            vec![step_id],
            vec![replacement],
            reason,
            event_id(7),
            timestamp(7)?,
            &Active,
        ))?;

        assert_eq!(
            outcome.kind(),
            &AgentLedgerActionOutcomeKind::ReplanRequested(TaskReplanReason::try_from_string(
                "an additional adapter is required".to_owned()
            )?)
        );
        assert_eq!(version, TaskLedgerStoreVersion::new(2)?);
        assert_eq!(run.state(), AgentControllerState::Replan);
        assert_eq!(run.task_ledger_revision(), ledger.revision());
        assert_eq!(ledger.replans().len(), 1);
        assert_eq!(
            ledger
                .step(step_id)
                .and_then(|step| step.retired_in_revision()),
            Some(ledger.revision())
        );
        assert_eq!(
            ledger.step(replacement_id).map(|step| step.status()),
            Some(TaskStepStatus::Ready)
        );
        Ok(())
    }

    #[test]
    fn cancellation_wins_before_any_ledger_mutation() -> Result<(), Box<dyn Error>> {
        let (mut run, mut ledger, step_id) = fixture()?;
        let original_ledger = ledger.clone();
        let outcome = ApplyAgentLedgerUpdate.execute(
            &mut run,
            &mut ledger,
            &AgentUpdateLedgerAction::new(
                step_id,
                AgentLedgerUpdate::RequestReplan(TaskReplanReason::try_from_string(
                    "cancelled request must not be retained".to_owned(),
                )?),
            ),
            None,
            event_id(6),
            snapshot(),
            timestamp(6)?,
            &Cancelled,
        )?;

        assert_eq!(outcome.kind(), &AgentLedgerActionOutcomeKind::Cancelled);
        assert_eq!(ledger, original_ledger);
        assert_eq!(run.state(), AgentControllerState::Cancelled);
        Ok(())
    }

    #[test]
    fn verified_step_atomically_starts_the_next_independent_plan_step() -> Result<(), Box<dyn Error>>
    {
        let (mut run, mut ledger, first_step_id, second_step_id) = multi_step_fixture()?;
        let store = RecordingActionStore::default();
        let outcome = block_on(ContinueVerifiedAgentPlan::new(&store).execute(
            &project()?,
            TaskLedgerStoreVersion::INITIAL,
            &mut run,
            &mut ledger,
            event_id(8),
            timestamp(8)?,
            &Active,
        ))?;

        assert_eq!(
            outcome,
            ContinueVerifiedAgentPlanOutcome::StepStarted {
                step_id: second_step_id,
                ledger_version: TaskLedgerStoreVersion::new(2)?,
            }
        );
        assert_eq!(run.state(), AgentControllerState::Execute);
        assert_eq!(
            ledger.step(first_step_id).map(|step| step.status()),
            Some(TaskStepStatus::Completed)
        );
        assert_eq!(
            ledger.step(second_step_id).map(|step| step.status()),
            Some(TaskStepStatus::InProgress)
        );
        let second_evidence_id = TaskEvidenceId::from_bytes([9; 32]);
        ledger.begin_step_verification(
            second_step_id,
            run_id(),
            None,
            vec![second_evidence_id],
            TaskLedgerTimestamp::from_unix_millis(9)?,
        )?;
        ledger.finish_step_verification(
            second_step_id,
            StepVerification::new(
                StepVerificationId::from_bytes([10; 32]),
                VerificationSpecId::from_bytes([6; 32]),
                run_id(),
                StepVerificationOutcome::Passed,
                vec![second_evidence_id],
                TaskLedgerTimestamp::from_unix_millis(10)?,
            )?,
        )?;
        run.transition(
            event_id(10),
            AgentControllerState::Verify,
            a3_domain::RunEventPayload::empty(),
            snapshot(),
            timestamp(10)?,
        )?;
        assert_eq!(
            block_on(ContinueVerifiedAgentPlan::new(&store).execute(
                &project()?,
                TaskLedgerStoreVersion::new(2)?,
                &mut run,
                &mut ledger,
                event_id(11),
                timestamp(11)?,
                &Active,
            ))?,
            ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance
        );
        assert_eq!(
            *store
                .commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            1
        );
        Ok(())
    }

    fn fixture() -> Result<(AgentRun, TaskLedger, TaskStepId), Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("apply a safe ledger action".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "result remains evidence grounded".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run targeted tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let step_id = TaskStepId::from_bytes([3; 32]);
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![TaskStepDefinition::new(
                step_id,
                None,
                TaskStepOutcome::try_from_string("prepare result".to_owned())?,
                TaskStepRationale::try_from_string("verify before completion".to_owned())?,
                Vec::new(),
                vec![ExpectedTaskEvidence::try_from_string(
                    "source span".to_owned(),
                )?],
                VerificationSpec::new(
                    VerificationSpecId::from_bytes([4; 32]),
                    VerificationMethod::Diagnostic,
                    VerificationRequirement::try_from_string("inspect evidence".to_owned())?,
                ),
            )?],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        let (mut run, _) = AgentRun::start(
            run_id(),
            goal.reference(),
            ledger.revision(),
            ModelProfileReference::new(
                ModelProfileId::from_bytes([5; 32]),
                ModelProfileVersion::V1,
            ),
            snapshot(),
            event_id(1),
            timestamp(1)?,
        )?;
        for (event, state) in [
            (2, AgentControllerState::Localize),
            (3, AgentControllerState::Plan),
            (4, AgentControllerState::Execute),
        ] {
            run.transition(
                event_id(event),
                state,
                a3_domain::RunEventPayload::empty(),
                snapshot(),
                timestamp(u64::from(event))?,
            )?;
        }
        ledger.start_step(step_id, run_id(), TaskLedgerTimestamp::from_unix_millis(5)?)?;
        Ok((run, ledger, step_id))
    }

    fn multi_step_fixture() -> Result<(AgentRun, TaskLedger, TaskStepId, TaskStepId), Box<dyn Error>>
    {
        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("repair two confirmed findings".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "both findings are independently verified".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run targeted checks".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let first_step_id = TaskStepId::from_bytes([3; 32]);
        let second_step_id = TaskStepId::from_bytes([4; 32]);
        let definition = |step_id, spec_id, dependencies| -> Result<_, Box<dyn Error>> {
            Ok(TaskStepDefinition::new(
                step_id,
                None,
                TaskStepOutcome::try_from_string(format!("repair finding {step_id:?}"))?,
                TaskStepRationale::try_from_string("confirmed by current evidence".to_owned())?,
                dependencies,
                vec![ExpectedTaskEvidence::try_from_string(
                    "fresh verification".to_owned(),
                )?],
                VerificationSpec::new(
                    spec_id,
                    VerificationMethod::Diagnostic,
                    VerificationRequirement::try_from_string("inspect evidence".to_owned())?,
                ),
            )?)
        };
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![
                definition(
                    first_step_id,
                    VerificationSpecId::from_bytes([5; 32]),
                    Vec::new(),
                )?,
                definition(
                    second_step_id,
                    VerificationSpecId::from_bytes([6; 32]),
                    vec![StepDependency::new(first_step_id)],
                )?,
            ],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        let (mut run, _) = AgentRun::start(
            run_id(),
            goal.reference(),
            ledger.revision(),
            ModelProfileReference::new(
                ModelProfileId::from_bytes([5; 32]),
                ModelProfileVersion::V1,
            ),
            snapshot(),
            event_id(1),
            timestamp(1)?,
        )?;
        for (event, state) in [
            (2, AgentControllerState::Localize),
            (3, AgentControllerState::Plan),
            (4, AgentControllerState::Execute),
        ] {
            run.transition(
                event_id(event),
                state,
                a3_domain::RunEventPayload::empty(),
                snapshot(),
                timestamp(u64::from(event))?,
            )?;
        }
        ledger.start_step(
            first_step_id,
            run_id(),
            TaskLedgerTimestamp::from_unix_millis(5)?,
        )?;
        let evidence_id = TaskEvidenceId::from_bytes([7; 32]);
        ledger.begin_step_verification(
            first_step_id,
            run_id(),
            None,
            vec![evidence_id],
            TaskLedgerTimestamp::from_unix_millis(6)?,
        )?;
        ledger.finish_step_verification(
            first_step_id,
            StepVerification::new(
                StepVerificationId::from_bytes([8; 32]),
                VerificationSpecId::from_bytes([5; 32]),
                run_id(),
                StepVerificationOutcome::Passed,
                vec![evidence_id],
                TaskLedgerTimestamp::from_unix_millis(7)?,
            )?,
        )?;
        run.transition(
            event_id(7),
            AgentControllerState::Verify,
            a3_domain::RunEventPayload::empty(),
            snapshot(),
            timestamp(7)?,
        )?;
        Ok((run, ledger, first_step_id, second_step_id))
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

    fn evidence(snapshot_id: SnapshotId) -> Result<AgentToolEvidenceSet, Box<dyn Error>> {
        let revision = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([8; 32]),
        );
        let source = AgentToolEvidence::for_span(EvidenceRef::new(
            revision,
            SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?,
        ));
        Ok(AgentToolEvidenceSet::new(snapshot_id, vec![source])?)
    }

    const fn run_id() -> AgentRunId {
        AgentRunId::from_bytes([9; 32])
    }

    const fn snapshot() -> SnapshotId {
        SnapshotId::from_bytes([10; 32])
    }

    const fn event_id(value: u8) -> RunEventId {
        RunEventId::from_bytes([value; 32])
    }

    fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
        Ok(AgentRunTimestamp::from_unix_millis(value)?)
    }
}
