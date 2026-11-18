//! Pure admission of complete original-backed intake before Agent model or tool work.

use a3_domain::{GoalContract, GoalOriginalCoverage, TaskLedger};
use std::error::Error;
use std::fmt;

/// Checks source readiness without mutating, materializing or starting any work.
///
/// A clean legacy goal keeps its existing path. Partial source goals remain valid
/// durable intake state, but cannot enter Agent execution. This is structural
/// admission, not evidence that the requirements were interpreted correctly.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdmitAgentGoalExecution;

impl AdmitAgentGoalExecution {
    /// Rejects orphan bindings and any uncovered original or unbound text entry.
    pub fn execute(self, goal: &GoalContract) -> Result<(), AgentGoalAdmissionFailure> {
        goal.validate_original_binding()
            .map_err(|_| AgentGoalAdmissionFailure::InvalidOriginalBinding)?;
        if goal.original().is_none() {
            return Ok(());
        }
        let coverage = GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| AgentGoalAdmissionFailure::InvalidOriginalBinding)?;
        if !coverage.is_complete() {
            return Err(AgentGoalAdmissionFailure::IntakeIncomplete);
        }
        Ok(())
    }

    /// Revalidates every active Source definition before any model or tool work.
    /// This leaves clean legacy execution and separate mutation recovery unchanged.
    pub fn execute_task(
        self,
        goal: &GoalContract,
        ledger: &TaskLedger,
    ) -> Result<(), AgentGoalAdmissionFailure> {
        self.execute(goal)?;
        if goal.original().is_none() {
            return Ok(());
        }
        if ledger.goal_contract() != goal.reference() {
            return Err(AgentGoalAdmissionFailure::InvalidSourceStep);
        }
        for step in ledger.steps().filter(|step| step.is_active_plan_step()) {
            crate::AdmitSourceTaskStep
                .execute(goal, step.definition())
                .map_err(|_| AgentGoalAdmissionFailure::InvalidSourceStep)?;
        }
        Ok(())
    }
}

/// Closed, content-free reason why a Goal cannot enter Agent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentGoalAdmissionFailure {
    /// An original or a present provenance range has invalid ownership or binding.
    InvalidOriginalBinding,
    /// Some original bytes or existing text entries remain unassigned.
    IntakeIncomplete,
    /// An active Source step has invalid assignment or verification obligations.
    InvalidSourceStep,
}

impl fmt::Display for AgentGoalAdmissionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidOriginalBinding => "agent goal original binding is invalid",
            Self::IntakeIncomplete => "agent goal original intake is incomplete",
            Self::InvalidSourceStep => "agent goal source step is invalid",
        })
    }
}

impl Error for AgentGoalAdmissionFailure {}
