use crate::{
    AgentControllerControl, AgentRecoveryStore, AgentRecoveryStoreFailure, RefreshRepositoryIndex,
    RefreshRepositoryIndexError, RepositoryChangeBatch, RepositoryChangeBatchError,
    RepositoryIndexCompiler, RepositoryIndexControl, RepositoryRescanReason, WorktreeMutationBusy,
    WorktreeMutationCoordinator,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentRun, AgentRunError, AgentRunTimestamp,
    AgentToolAttemptNumber, MutationReconciliation, ProjectIdentity, PublishedIndex, RunEventCode,
    RunEventId, RunEventKind, RunEventOutcome, RunEventPayload, RunEventSubject, ToolRunId,
};
use std::error::Error;
use std::fmt;

/// Safe result of adopting a full repository baseline for one historically Unknown mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationReconciliationOutcome {
    run: AgentRun,
    attempt: AgentMutationAttempt,
    published_index: PublishedIndex,
}

impl MutationReconciliationOutcome {
    /// Returns the run after its content-free recovery event adopted the new snapshot.
    #[must_use]
    pub const fn run(&self) -> &AgentRun {
        &self.run
    }

    /// Returns the still-Unknown but now reconciled mutation projection.
    #[must_use]
    pub const fn attempt(&self) -> AgentMutationAttempt {
        self.attempt
    }

    /// Returns the authoritative full-scan publication used as the safe baseline.
    #[must_use]
    pub const fn published_index(&self) -> &PublishedIndex {
        &self.published_index
    }
}

/// E8 use case that never reapplies or rolls back an Unknown mutation.
#[derive(Debug, Clone, Copy)]
pub struct ReconcileUnknownMutation<'a> {
    coordinator: &'a WorktreeMutationCoordinator,
    recovery: &'a dyn AgentRecoveryStore,
    refresh: &'a RefreshRepositoryIndex,
}

impl<'a> ReconcileUnknownMutation<'a> {
    /// Wires the runtime lease, durable recovery boundary, and authoritative index refresh.
    #[must_use]
    pub const fn new(
        coordinator: &'a WorktreeMutationCoordinator,
        recovery: &'a dyn AgentRecoveryStore,
        refresh: &'a RefreshRepositoryIndex,
    ) -> Self {
        Self {
            coordinator,
            recovery,
            refresh,
        }
    }

    /// Full-scans current foreign and agent changes, then atomically binds the resulting snapshot.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        tool_run_id: ToolRunId,
        attempt_number: AgentToolAttemptNumber,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
    ) -> Result<MutationReconciliationOutcome, MutationReconciliationError>
    where
        C: AgentControllerControl + RepositoryIndexControl,
    {
        let selected = self
            .recovery
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .into_iter()
            .find(|candidate| {
                let tool_attempt = candidate.tool_attempt();
                tool_attempt.tool_run_id() == tool_run_id
                    && tool_attempt.attempt() == attempt_number
            })
            .ok_or(MutationReconciliationError::AttemptNotFound)?;
        if selected.tool_attempt().run_id() != run.id()
            || !selected.disposition().requires_reconciliation()
        {
            return Err(MutationReconciliationError::AttemptState);
        }
        let lease = self.coordinator.try_acquire(
            run.id(),
            project.worktree().id(),
            selected.fingerprint(),
        )?;
        if AgentControllerControl::is_cancelled(control) {
            return Err(MutationReconciliationError::Cancelled);
        }

        self.recovery
            .interrupt_agent_tool_attempts(project, run.id(), observed_at)
            .await?;
        let selected = self
            .recovery
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .into_iter()
            .find(|candidate| {
                let tool_attempt = candidate.tool_attempt();
                tool_attempt.tool_run_id() == tool_run_id
                    && tool_attempt.attempt() == attempt_number
            })
            .ok_or(MutationReconciliationError::AttemptNotFound)?;
        if !selected.tool_attempt().status().is_terminal()
            || !selected.disposition().requires_reconciliation()
        {
            return Err(MutationReconciliationError::AttemptState);
        }

        let batch =
            RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?;
        let refresh = self
            .refresh
            .execute(project, &batch, index_compiler, control)
            .await?;
        let published_index = refresh.published_index().clone();
        let snapshot_id = published_index.run().snapshot_id();
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let event = next_run.record(
            event_id,
            RunEventKind::Diagnostic,
            RunEventPayload::new(
                RunEventCode::StateRecovered,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            snapshot_id,
            Some(RunEventSubject::Tool(tool_run_id)),
            observed_at,
        )?;
        let attempt = self
            .recovery
            .reconcile_agent_mutation(
                project,
                expected_sequence,
                &next_run,
                &event,
                tool_run_id,
                attempt_number,
            )
            .await?;
        if attempt.disposition()
            != AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled { snapshot_id })
        {
            return Err(MutationReconciliationError::InvalidStoreResult);
        }
        lease.record_success();
        *run = next_run.clone();
        Ok(MutationReconciliationOutcome {
            run: next_run,
            attempt,
            published_index,
        })
    }
}

/// Reconciliation stopped without modifying or retrying the original worktree mutation.
#[derive(Debug)]
pub enum MutationReconciliationError {
    /// The selected mutation attempt did not exist in the run.
    AttemptNotFound,
    /// The selected attempt was not terminal Unknown with reconciliation required.
    AttemptState,
    /// The runtime already owns a mutating action for this worktree.
    Busy(WorktreeMutationBusy),
    /// Cancellation won before the full scan started.
    Cancelled,
    /// The explicit full-rescan request violated its fixed boundary.
    ChangeBatch(RepositoryChangeBatchError),
    /// The authoritative full scan or publication failed.
    Index(RefreshRepositoryIndexError),
    /// Durable mutation inspection, interruption, or reconciliation failed.
    Store(AgentRecoveryStoreFailure),
    /// Storage returned a disposition not bound to the published reconciliation snapshot.
    InvalidStoreResult,
    /// The content-free recovery event violated a Run invariant.
    Run(AgentRunError),
}

impl fmt::Display for MutationReconciliationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptNotFound => "unknown mutation attempt was not found",
            Self::AttemptState => "mutation attempt does not require reconciliation",
            Self::Busy(_) => "worktree mutation boundary is busy during reconciliation",
            Self::Cancelled => "mutation reconciliation was cancelled before full scan",
            Self::ChangeBatch(_) => "mutation reconciliation full-scan request is invalid",
            Self::Index(_) => "mutation reconciliation could not publish a full index",
            Self::Store(_) => "mutation reconciliation persistence failed",
            Self::InvalidStoreResult => "mutation reconciliation store result is invalid",
            Self::Run(_) => "mutation reconciliation recovery event is invalid",
        })
    }
}

impl Error for MutationReconciliationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Busy(error) => Some(error),
            Self::ChangeBatch(error) => Some(error),
            Self::Index(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::AttemptNotFound
            | Self::AttemptState
            | Self::Cancelled
            | Self::InvalidStoreResult => None,
        }
    }
}

impl From<WorktreeMutationBusy> for MutationReconciliationError {
    fn from(value: WorktreeMutationBusy) -> Self {
        Self::Busy(value)
    }
}

impl From<RepositoryChangeBatchError> for MutationReconciliationError {
    fn from(value: RepositoryChangeBatchError) -> Self {
        Self::ChangeBatch(value)
    }
}

impl From<RefreshRepositoryIndexError> for MutationReconciliationError {
    fn from(value: RefreshRepositoryIndexError) -> Self {
        Self::Index(value)
    }
}

impl From<AgentRecoveryStoreFailure> for MutationReconciliationError {
    fn from(value: AgentRecoveryStoreFailure) -> Self {
        Self::Store(value)
    }
}

impl From<AgentRunError> for MutationReconciliationError {
    fn from(value: AgentRunError) -> Self {
        Self::Run(value)
    }
}
