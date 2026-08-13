use crate::{JobContext, TaskLedgerStoreVersion};
use a3_domain::{ProjectIdentity, TaskId, TaskLedgerRevision};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by one complete scheduler-owned Agent execution attempt.
pub type AgentRunExecutionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AgentRunExecutionOutcome, AgentRunExecutionFailure>> + Send + 'a,
    >,
>;

/// Exact durable task anchors accepted before an Agent attempt enters the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunExecutionRequest {
    task_id: TaskId,
    ledger_revision: TaskLedgerRevision,
    ledger_store_version: TaskLedgerStoreVersion,
}

impl AgentRunExecutionRequest {
    /// Binds execution to a Core-selected task and its current optimistic Ledger anchors.
    #[must_use]
    pub const fn new(
        task_id: TaskId,
        ledger_revision: TaskLedgerRevision,
        ledger_store_version: TaskLedgerStoreVersion,
    ) -> Self {
        Self {
            task_id,
            ledger_revision,
            ledger_store_version,
        }
    }

    /// Returns the durable task selected through the bounded product workflow.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    /// Returns the Task Ledger revision revalidated before scheduling.
    #[must_use]
    pub const fn ledger_revision(self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the optimistic Task Ledger store version revalidated before scheduling.
    #[must_use]
    pub const fn ledger_store_version(self) -> TaskLedgerStoreVersion {
        self.ledger_store_version
    }
}

/// Terminal result of one scheduler-owned Agent attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunExecutionOutcome {
    /// The executor reached its current durable stopping condition without cancellation.
    Completed,
    /// Operational cancellation stopped work without committing a controller Cancel transition.
    Cancelled,
}

/// Application-owned complete Agent runtime capability.
///
/// Implementations compose the deterministic controller, context, provider, safe tools, policy,
/// verification, and persistence. Provider payloads and adapter errors never cross this port.
pub trait AgentRunExecutor: fmt::Debug + Send + Sync {
    /// Executes one task-derived attempt under the scheduler's owned cancellation boundary.
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: AgentRunExecutionRequest,
        control: &'a JobContext,
    ) -> AgentRunExecutionFuture<'a>;
}

/// Stable complete-attempt failure without provider, source, process, or storage details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunExecutionFailure {
    /// Task, Goal, Ledger, Run, or snapshot anchors changed before work could continue.
    AnchorsChanged,
    /// The current durable controller state cannot execute a new attempt.
    InvalidState,
    /// A required local model, context, tool, policy, verification, or storage capability failed.
    Unavailable,
    /// Progress could not reach the owning bounded scheduler.
    ProgressUnavailable,
}

impl fmt::Display for AgentRunExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AnchorsChanged => "Agent execution anchors changed",
            Self::InvalidState => "Agent execution state is not runnable",
            Self::Unavailable => "Agent execution capability is unavailable",
            Self::ProgressUnavailable => "Agent execution progress is unavailable",
        })
    }
}

impl Error for AgentRunExecutionFailure {}

#[cfg(test)]
mod tests {
    use super::AgentRunExecutionRequest;
    use crate::TaskLedgerStoreVersion;
    use a3_domain::{TaskId, TaskLedgerRevision};
    use std::error::Error;

    #[test]
    fn request_retains_only_task_and_exact_ledger_anchors() -> Result<(), Box<dyn Error>> {
        let request = AgentRunExecutionRequest::new(
            TaskId::from_bytes([11; 32]),
            TaskLedgerRevision::new(3)?,
            TaskLedgerStoreVersion::new(7)?,
        );

        assert_eq!(request.task_id(), TaskId::from_bytes([11; 32]));
        assert_eq!(request.ledger_revision().get(), 3);
        assert_eq!(request.ledger_store_version().get(), 7);
        Ok(())
    }
}
