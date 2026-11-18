//! One durable Core-owned horizon job; no provider, queue, or model budget is executed here.
use super::{
    AgentRunId, GoalContractReference, RunEventId, RunEventSequence, SnapshotId, TaskLedger,
    TaskLedgerError, TaskLedgerReplanCause, TaskLedgerRevision, TaskStepId,
};

/// Checkpoint kept until the first appended step starts in the atomic Ledger/run CAS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPlanning {
    run_id: AgentRunId,
    goal_ref: GoalContractReference,
    base_revision: TaskLedgerRevision,
    snapshot_id: SnapshotId,
    requested_event_id: RunEventId,
    requested_sequence: RunEventSequence,
    stage: PendingPlanningStage,
}

/// Durable handoff state, never selected by model output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingPlanningStage {
    /// The original job still requires an admitted bounded next horizon.
    NeedsPlan,
    /// The next horizon exists; restart must activate it instead of planning again.
    PlanAppended {
        /// Actual immediate next Ledger revision.
        revision: TaskLedgerRevision,
        /// Exact canonical IDs introduced by this horizon, one through three.
        added_step_ids: Vec<TaskStepId>,
    },
}

impl PendingPlanning {
    /// Binds the Core job to its immutable request event, independent of scheduler ID reuse.
    pub fn new(
        run_id: AgentRunId,
        goal_ref: GoalContractReference,
        base_revision: TaskLedgerRevision,
        snapshot_id: SnapshotId,
        requested_event_id: RunEventId,
        requested_sequence: RunEventSequence,
    ) -> Result<Self, TaskLedgerError> {
        Self::reconstruct(
            run_id,
            goal_ref,
            base_revision,
            snapshot_id,
            requested_event_id,
            requested_sequence,
            PendingPlanningStage::NeedsPlan,
        )
    }
    /// Restores bounded intrinsic shape only; pair/event admission happens after both TX writes.
    pub fn reconstruct(
        run_id: AgentRunId,
        goal_ref: GoalContractReference,
        base_revision: TaskLedgerRevision,
        snapshot_id: SnapshotId,
        requested_event_id: RunEventId,
        requested_sequence: RunEventSequence,
        mut stage: PendingPlanningStage,
    ) -> Result<Self, TaskLedgerError> {
        if requested_sequence.get() <= 1 {
            return Err(TaskLedgerError::InvalidPendingPlanning);
        }
        if let PendingPlanningStage::PlanAppended {
            revision,
            added_step_ids,
        } = &mut stage
        {
            if base_revision.get().checked_add(1) != Some(revision.get())
                || added_step_ids.is_empty()
                || added_step_ids.len() > 3
            {
                return Err(TaskLedgerError::InvalidPendingPlanning);
            }
            added_step_ids.sort_unstable();
            if added_step_ids.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(TaskLedgerError::InvalidPendingPlanning);
            }
        }
        Ok(Self {
            run_id,
            goal_ref,
            base_revision,
            snapshot_id,
            requested_event_id,
            requested_sequence,
            stage,
        })
    }
    /// Returns the original owning run; it is never replaced to reset budgets.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }
    /// Returns the immutable original Goal reference.
    #[must_use]
    pub const fn goal_ref(&self) -> GoalContractReference {
        self.goal_ref
    }
    /// Returns the exhausted horizon's Ledger revision.
    #[must_use]
    pub const fn base_revision(&self) -> TaskLedgerRevision {
        self.base_revision
    }
    /// Returns the current source snapshot bound before planning.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the persistent Core job identity, not a process-local scheduler ID.
    #[must_use]
    pub const fn requested_event_id(&self) -> RunEventId {
        self.requested_event_id
    }
    /// Returns the exact journal position for lookup beyond presentation pages.
    #[must_use]
    pub const fn requested_sequence(&self) -> RunEventSequence {
        self.requested_sequence
    }
    /// Returns whether to append a plan or activate its already durable definitions.
    #[must_use]
    pub const fn stage(&self) -> &PendingPlanningStage {
        &self.stage
    }

    /// Validates only same-Ledger shape, including retained horizon history.
    /// Terminal inertness and the actual request event require the owning Run pair.
    pub fn validate_for_ledger(&self, ledger: &TaskLedger) -> Result<(), TaskLedgerError> {
        if self.goal_ref != ledger.goal_contract() {
            return Err(TaskLedgerError::InvalidPendingPlanning);
        }
        match &self.stage {
            PendingPlanningStage::NeedsPlan if ledger.revision() == self.base_revision => Ok(()),
            PendingPlanningStage::PlanAppended {
                revision,
                added_step_ids,
            } => {
                let record = ledger
                    .replans()
                    .last()
                    .ok_or(TaskLedgerError::InvalidPendingPlanning)?;
                if ledger.revision() != *revision
                    || record.revision() != *revision
                    || record.previous_revision() != self.base_revision
                    || record.cause() != TaskLedgerReplanCause::HorizonExtension
                    || !record.retired_step_ids().is_empty()
                    || record.added_step_ids() != added_step_ids
                    || added_step_ids.iter().any(|id| {
                        ledger
                            .step(*id)
                            .is_none_or(|step| step.introduced_in_revision() != *revision)
                    })
                {
                    return Err(TaskLedgerError::InvalidPendingPlanning);
                }
                Ok(())
            }
            _ => Err(TaskLedgerError::InvalidPendingPlanning),
        }
    }
    pub(super) fn appended(
        &self,
        revision: TaskLedgerRevision,
        ids: Vec<TaskStepId>,
    ) -> Result<Self, TaskLedgerError> {
        Self::reconstruct(
            self.run_id,
            self.goal_ref,
            self.base_revision,
            self.snapshot_id,
            self.requested_event_id,
            self.requested_sequence,
            PendingPlanningStage::PlanAppended {
                revision,
                added_step_ids: ids,
            },
        )
    }
}
