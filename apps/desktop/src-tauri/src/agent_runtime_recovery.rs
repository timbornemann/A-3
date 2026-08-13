use crate::DesktopBoundedReadControl;
use crate::agent_recovery_metadata::SystemAgentRecoveryMetadata;
use crate::agent_run_manager::{
    AgentPauseCheckpoint, AgentRuntimeRecovery, AgentRuntimeRecoveryFailure,
    AgentRuntimeRecoveryFuture,
};
use a3_application::{
    AgentRecoveryChoice, AgentRecoveryOutcomeKind, AgentTaskControlResult,
    AgentTaskRecoveryLoadResult, ControlAgentTaskRun, InspectAgentTaskRecovery,
};
use a3_domain::{AgentControllerState, ProjectIdentity};
use std::fmt;

/// Bridges scheduler-owned Agent termination to the existing authoritative H11/E8 use cases.
#[derive(Clone)]
pub(crate) struct CoreAgentRuntimeRecovery {
    inspector: InspectAgentTaskRecovery,
    controller: ControlAgentTaskRun,
}

impl CoreAgentRuntimeRecovery {
    pub(crate) const fn new(
        inspector: InspectAgentTaskRecovery,
        controller: ControlAgentTaskRun,
    ) -> Self {
        Self {
            inspector,
            controller,
        }
    }
}

impl fmt::Debug for CoreAgentRuntimeRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CoreAgentRuntimeRecovery")
    }
}

impl AgentRuntimeRecovery for CoreAgentRuntimeRecovery {
    fn validate_pause<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: a3_application::AgentRunExecutionRequest,
    ) -> AgentRuntimeRecoveryFuture<'a, AgentPauseCheckpoint> {
        Box::pin(async move {
            let metadata = SystemAgentRecoveryMetadata;
            let observed_at = metadata
                .now()
                .map_err(|_| AgentRuntimeRecoveryFailure::Unavailable)?;
            let recovery = self
                .inspector
                .execute(
                    project,
                    request.task_id(),
                    observed_at,
                    &DesktopBoundedReadControl::new(),
                    &DesktopBoundedReadControl::new(),
                )
                .await
                .map_err(|_| AgentRuntimeRecoveryFailure::Unavailable)?;
            let AgentTaskRecoveryLoadResult::Available(recovery) = recovery else {
                return Err(AgentRuntimeRecoveryFailure::InvalidCheckpoint);
            };
            AgentPauseCheckpoint::new(
                request.task_id(),
                a3_domain::TaskLedgerRevision::new(recovery.ledger_revision())
                    .map_err(|_| AgentRuntimeRecoveryFailure::InvalidCheckpoint)?,
                recovery.ledger_store_version(),
                recovery.state(),
            )
        })
    }

    fn cancel<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: a3_application::AgentRunExecutionRequest,
    ) -> AgentRuntimeRecoveryFuture<'a, ()> {
        Box::pin(async move {
            let metadata = SystemAgentRecoveryMetadata;
            let event_id = metadata
                .next_event_id()
                .map_err(|_| AgentRuntimeRecoveryFailure::Unavailable)?;
            let observed_at = metadata
                .now()
                .map_err(|_| AgentRuntimeRecoveryFailure::Unavailable)?;
            let result = self
                .controller
                .execute(
                    project,
                    request.task_id(),
                    request.ledger_revision().get(),
                    request.ledger_store_version(),
                    AgentRecoveryChoice::Cancel,
                    event_id,
                    observed_at,
                    &DesktopBoundedReadControl::new(),
                    &DesktopBoundedReadControl::new(),
                )
                .await
                .map_err(|_| AgentRuntimeRecoveryFailure::Unavailable)?;
            match result {
                AgentTaskControlResult::Applied {
                    outcome: AgentRecoveryOutcomeKind::Cancelled,
                    state: AgentControllerState::Cancelled,
                    ..
                } => Ok(()),
                _ => Err(AgentRuntimeRecoveryFailure::InvalidCheckpoint),
            }
        })
    }
}
