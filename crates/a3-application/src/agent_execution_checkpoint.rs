//! Bounded execution receipts, never source evidence or verification results.

use crate::{
    AgentRecoveryStore, AgentRecoveryStoreFailure, ContextCompileControl, RunEventPage,
    RunEventPageLimit, RunJournalStore, RunJournalStoreFailure,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind, AgentRun, AgentRunId,
    AgentToolAttemptStatus, GoalContractReference, ProjectIdentity, RepositoryId, RunEventKind,
    RunEventOutcome, RunEventSequence, RunEventSubject, SnapshotId, TaskLedgerRevision, ToolRunId,
    WorktreeId,
};
use std::{error::Error, fmt, time::Duration};

const EVENT_WINDOW: u16 = 64;
const MAX_ATTEMPTS: usize = 4096;
const LOAD_TIMEOUT: Duration = Duration::from_secs(5);

/// What the mutation boundary actually observed; neither variant proves a step passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutedAgentMutation {
    /// A complete patch result was atomically recorded after index publication.
    PatchApplied,
    /// A drained process result was recorded; its exit/test outcome is not projected here.
    ProcessObserved,
}

/// Content-free projection derived only from matching durable journal/attempt records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentExecutionCheckpoint {
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    run_id: AgentRunId,
    goal: GoalContractReference,
    ledger_revision: TaskLedgerRevision,
    snapshot_id: SnapshotId,
    event_sequence: RunEventSequence,
    tool_run_id: ToolRunId,
    mutation: ExecutedAgentMutation,
}

impl AgentExecutionCheckpoint {
    /// Reconstructs at most one receipt from the exact bounded tail of this run.
    /// Missing/old evidence is not a claim; contradictory projections fail closed.
    pub fn reconstruct(
        project: &ProjectIdentity,
        run: &AgentRun,
        page: &RunEventPage,
        attempts: &[AgentMutationAttempt],
    ) -> Result<Option<Self>, AgentExecutionCheckpointError> {
        let first = run
            .last_event_sequence()
            .get()
            .saturating_sub(u64::from(EVENT_WINDOW) - 1)
            .max(1);
        if page.has_more()
            || page.events().len() > usize::from(EVENT_WINDOW)
            || page
                .events()
                .first()
                .is_none_or(|e| e.sequence().get() != first)
            || page
                .events()
                .last()
                .is_none_or(|e| e.sequence() != run.last_event_sequence())
            || page
                .events()
                .iter()
                .any(|e| e.run_id() != run.id() || e.occurred_at() > run.updated_at())
            || attempts.len() > MAX_ATTEMPTS
            || attempts
                .iter()
                .any(|a| a.tool_attempt().run_id() != run.id())
        {
            return Err(AgentExecutionCheckpointError::Inconsistent);
        }
        for event in page.events().iter().rev() {
            if event.kind() != RunEventKind::ToolAction
                || event.payload().outcome() != Some(RunEventOutcome::Succeeded)
                || event.snapshot_id() != run.current_snapshot_id()
            {
                continue;
            }
            let Some(RunEventSubject::Tool(tool_run_id)) = event.subject() else {
                continue;
            };
            let mut matches = attempts.iter().filter(|a| {
                let tool = a.tool_attempt();
                tool.tool_run_id() == tool_run_id
                    && tool.updated_at() == event.occurred_at()
                    && tool.status() == AgentToolAttemptStatus::Succeeded
                    && a.disposition() == AgentMutationDisposition::Applied
            });
            let Some(attempt) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                return Err(AgentExecutionCheckpointError::Inconsistent);
            }
            let mutation = match attempt.kind() {
                AgentMutationKind::Patch => ExecutedAgentMutation::PatchApplied,
                AgentMutationKind::Process => ExecutedAgentMutation::ProcessObserved,
                AgentMutationKind::UnclassifiedLegacy => continue,
            };
            return Ok(Some(Self {
                repository_id: project.repository().id(),
                worktree_id: project.worktree().id(),
                run_id: run.id(),
                goal: run.goal_contract(),
                ledger_revision: run.task_ledger_revision(),
                snapshot_id: event.snapshot_id(),
                event_sequence: event.sequence(),
                tool_run_id,
                mutation,
            }));
        }
        Ok(None)
    }

    /// Checks ownership before accepting a receipt into a compile input.
    pub(crate) fn matches(&self, input: &crate::AgentContextCompileInput) -> bool {
        self.repository_id == input.project().repository().id()
            && self.worktree_id == input.project().worktree().id()
            && self.goal == input.goal_contract().reference()
            && self.ledger_revision == input.task_ledger().revision()
            && input
                .task_ledger()
                .step(input.current_step_id())
                .and_then(|step| step.attempts().last())
                .is_some_and(|attempt| attempt.run_id() == self.run_id)
    }

    /// Returns the post-execution publication that must still be current.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact supporting journal event, not a model-proposed action.
    #[must_use]
    pub const fn event_sequence(&self) -> RunEventSequence {
        self.event_sequence
    }

    /// Returns the supporting tool identity without exposing payloads.
    #[must_use]
    pub const fn tool_run_id(&self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the closed observed boundary result, never test success.
    #[must_use]
    pub const fn mutation(&self) -> ExecutedAgentMutation {
        self.mutation
    }
}

/// Read-only reconstruction shared by normal turns and post-mutation compilation.
#[derive(Debug, Clone, Copy)]
pub struct LoadAgentExecutionCheckpoint<'a> {
    journal: &'a dyn RunJournalStore,
    recovery: &'a dyn AgentRecoveryStore,
}

impl<'a> LoadAgentExecutionCheckpoint<'a> {
    /// Reuses existing journal/recovery capabilities; no new storage boundary.
    #[must_use]
    pub const fn new(
        journal: &'a dyn RunJournalStore,
        recovery: &'a dyn AgentRecoveryStore,
    ) -> Self {
        Self { journal, recovery }
    }

    /// Loads one finite tail with a caller-owned timeout and cooperative cancellation.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        control: &dyn ContextCompileControl,
    ) -> Result<Option<AgentExecutionCheckpoint>, AgentExecutionCheckpointError> {
        let load = async {
            check_cancelled(control)?;
            let after = run
                .last_event_sequence()
                .get()
                .checked_sub(u64::from(EVENT_WINDOW))
                .filter(|n| *n > 0)
                .map(RunEventSequence::new)
                .transpose()
                .map_err(|_| AgentExecutionCheckpointError::Inconsistent)?;
            let limit = RunEventPageLimit::new(EVENT_WINDOW)
                .map_err(|_| AgentExecutionCheckpointError::Inconsistent)?;
            let page = self
                .journal
                .load_run_events(project, run.id(), after, limit)
                .await
                .map_err(AgentExecutionCheckpointError::Journal)?;
            check_cancelled(control)?;
            let attempts = self
                .recovery
                .load_agent_mutation_attempts(project, run.id())
                .await
                .map_err(AgentExecutionCheckpointError::Recovery)?;
            check_cancelled(control)?;
            AgentExecutionCheckpoint::reconstruct(project, run, &page, &attempts)
        };
        bounded_load(load, control, LOAD_TIMEOUT).await
    }
}

async fn bounded_load(
    load: impl std::future::Future<
        Output = Result<Option<AgentExecutionCheckpoint>, AgentExecutionCheckpointError>,
    >,
    control: &dyn ContextCompileControl,
    timeout: Duration,
) -> Result<Option<AgentExecutionCheckpoint>, AgentExecutionCheckpointError> {
    check_cancelled(control)?;
    tokio::runtime::Handle::try_current()
        .map_err(|_| AgentExecutionCheckpointError::RuntimeUnavailable)?;
    // Both futures belong to this call. Cancellation drops the read rather than detaching it.
    let cancelled = async {
        loop {
            check_cancelled(control)?;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let timed = tokio::time::timeout(timeout, load);
    futures::pin_mut!(cancelled, timed);
    match futures::future::select(cancelled, timed).await {
        futures::future::Either::Left((result, _)) => result,
        futures::future::Either::Right((result, _)) => {
            check_cancelled(control)?;
            result.map_err(|_| AgentExecutionCheckpointError::TimedOut)?
        }
    }
}

fn check_cancelled(
    control: &dyn ContextCompileControl,
) -> Result<(), AgentExecutionCheckpointError> {
    if control.is_cancelled() {
        Err(AgentExecutionCheckpointError::Cancelled)
    } else {
        Ok(())
    }
}

/// Content-free reason why execution state could not safely be reconstructed.
#[derive(Debug)]
pub enum AgentExecutionCheckpointError {
    /// Durable projections are incomplete, ambiguous, outside bounds or from another run.
    Inconsistent,
    /// Caller cancelled this owned operation.
    Cancelled,
    /// The bounded storage reconstruction timed out.
    TimedOut,
    /// Caller did not provide the owned Tokio runtime required for timed operations.
    RuntimeUnavailable,
    /// Journal read failed.
    Journal(RunJournalStoreFailure),
    /// Mutation history read failed.
    Recovery(AgentRecoveryStoreFailure),
}

impl fmt::Display for AgentExecutionCheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inconsistent => "execution checkpoint projections are inconsistent",
            Self::Cancelled => "execution checkpoint cancelled",
            Self::TimedOut => "execution checkpoint timed out",
            Self::RuntimeUnavailable => "execution checkpoint runtime unavailable",
            Self::Journal(_) => "execution checkpoint journal unavailable",
            Self::Recovery(_) => "execution checkpoint mutation history unavailable",
        })
    }
}
impl Error for AgentExecutionCheckpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Journal(e) => Some(e),
            Self::Recovery(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug, Default)]
    struct Control(AtomicBool);
    impl ContextCompileControl for Control {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
        fn report_phase(
            &self,
            _: crate::ContextCompilePhase,
        ) -> Result<(), crate::TaskLensControlError> {
            Ok(())
        }
    }
    struct Dropped<'a>(&'a AtomicBool);
    impl Drop for Dropped<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn cancelled_or_missing_runtime_never_polls_storage() {
        let control = Control::default();
        let polled = AtomicBool::new(false);
        let load = async {
            polled.store(true, Ordering::SeqCst);
            Ok(None)
        };
        assert!(matches!(
            futures::executor::block_on(bounded_load(load, &control, LOAD_TIMEOUT)),
            Err(AgentExecutionCheckpointError::RuntimeUnavailable)
        ));
        assert!(!polled.load(Ordering::SeqCst));
        control.0.store(true, Ordering::SeqCst);
        let load = async {
            polled.store(true, Ordering::SeqCst);
            Ok(None)
        };
        assert!(matches!(
            futures::executor::block_on(bounded_load(load, &control, LOAD_TIMEOUT)),
            Err(AgentExecutionCheckpointError::Cancelled)
        ));
        assert!(!polled.load(Ordering::SeqCst));
    }

    #[test]
    fn timeout_and_inflight_cancellation_drop_owned_reads() -> Result<(), Box<dyn Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        runtime.block_on(async {
            for cancel in [false, true] {
                let control = Control::default();
                let dropped = AtomicBool::new(false);
                let load = async {
                    let _guard = Dropped(&dropped);
                    if cancel {
                        control.0.store(true, Ordering::SeqCst);
                    }
                    std::future::pending().await
                };
                let result = bounded_load(load, &control, Duration::from_millis(100)).await;
                assert!(if cancel {
                    matches!(result, Err(AgentExecutionCheckpointError::Cancelled))
                } else {
                    matches!(result, Err(AgentExecutionCheckpointError::TimedOut))
                });
                assert!(dropped.load(Ordering::SeqCst));
            }
        });
        Ok(())
    }
}
