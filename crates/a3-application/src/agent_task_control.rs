use crate::{
    AgentActivityLoadResult, AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryInspection,
    AgentRecoveryOutcomeKind, AgentRecoveryStore, GetAgentActivity, IndexPersistenceControl,
    InspectAgentRunRecovery, KnowledgeIndexStore, RecoverAgentRun, RunJournalStore,
    TaskLedgerStore, TaskLedgerStoreVersion, TaskLensWorkspaceControl, TaskLensWorkspaceStore,
};
use a3_domain::{
    AgentControllerState, AgentRunId, AgentRunTimestamp, ProjectIdentity, RunEventId, SnapshotId,
    TaskId,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Content-free recovery facts for the one active run derived from a durable task ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskRecovery {
    ledger_revision: u32,
    ledger_store_version: TaskLedgerStoreVersion,
    state: AgentControllerState,
    run_snapshot_id: SnapshotId,
    published_snapshot_id: SnapshotId,
    interrupted_tool_attempts: u32,
    stale_evidence_count: u32,
    mutation_reconciliation_required: bool,
    mutation_replan_required: bool,
    can_resume: bool,
}

impl AgentTaskRecovery {
    /// Returns the exact current Task Ledger revision inspected with this recovery state.
    #[must_use]
    pub const fn ledger_revision(&self) -> u32 {
        self.ledger_revision
    }

    /// Returns the optimistic Task Ledger persistence version inspected with this recovery state.
    #[must_use]
    pub const fn ledger_store_version(&self) -> TaskLedgerStoreVersion {
        self.ledger_store_version
    }

    /// Returns the currently materialized finite controller state.
    #[must_use]
    pub const fn state(&self) -> AgentControllerState {
        self.state
    }

    /// Returns the snapshot held by the selected run before recovery inspection.
    #[must_use]
    pub const fn run_snapshot_id(&self) -> SnapshotId {
        self.run_snapshot_id
    }

    /// Returns the latest atomically published repository snapshot.
    #[must_use]
    pub const fn published_snapshot_id(&self) -> SnapshotId {
        self.published_snapshot_id
    }

    /// Returns whether repository state advanced since the run last adopted a snapshot.
    #[must_use]
    pub fn snapshot_changed(&self) -> bool {
        self.run_snapshot_id != self.published_snapshot_id
    }

    /// Returns abandoned in-flight attempts durably marked interrupted by this inspection.
    #[must_use]
    pub const fn interrupted_tool_attempts(&self) -> u32 {
        self.interrupted_tool_attempts
    }

    /// Returns the bounded number of completed verification evidence records now stale.
    #[must_use]
    pub const fn stale_evidence_count(&self) -> u32 {
        self.stale_evidence_count
    }

    /// Returns whether an Unknown mutation still needs an authoritative full-scan baseline.
    #[must_use]
    pub const fn mutation_reconciliation_required(&self) -> bool {
        self.mutation_reconciliation_required
    }

    /// Returns whether a reconciled Unknown still requires the explicit Replan choice.
    #[must_use]
    pub const fn mutation_replan_required(&self) -> bool {
        self.mutation_replan_required
    }

    /// Returns whether Resume is allowed by current evidence and mutation dispositions.
    #[must_use]
    pub const fn can_resume(&self) -> bool {
        self.can_resume
    }
}

impl AgentTaskRecovery {
    fn try_from_inspection(
        value: AgentRecoveryInspection,
        target: ControlTarget,
    ) -> Result<Self, AgentTaskControlFailure> {
        let stale_evidence_count = u32::try_from(value.stale_evidence_ids().len())
            .map_err(|_| AgentTaskControlFailure::ResourceLimit)?;
        Ok(Self {
            ledger_revision: target.ledger_revision,
            ledger_store_version: target.ledger_store_version,
            state: value.state(),
            run_snapshot_id: value.run_snapshot_id(),
            published_snapshot_id: value.published_snapshot_id(),
            interrupted_tool_attempts: value.interrupted_tool_attempts(),
            stale_evidence_count,
            mutation_reconciliation_required: value.mutation_reconciliation_required(),
            mutation_replan_required: value.mutation_replan_required(),
            can_resume: value.can_resume(),
        })
    }
}

/// Expected states while deriving recovery controls from one WebView-selected task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentTaskRecoveryLoadResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no Task Ledger yet.
    LedgerUnavailable,
    /// The ledger still refers to an earlier immutable Goal Contract revision.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Task Ledger.
        ledger_revision: u32,
    },
    /// Durable task or run anchors changed while the bounded read was in progress.
    ActivityChanged,
    /// No retained step attempt has created a run yet.
    RunUnavailable,
    /// The latest retained run is terminal or no longer belongs to an active step attempt.
    RunNotControllable {
        /// Last materialized finite controller state.
        state: AgentControllerState,
    },
    /// Recovery facts are current and the explicit controls may be evaluated.
    Available(AgentTaskRecovery),
}

/// Result of one task-bound explicit Resume, Replan, or Cancel command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentTaskControlResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no Task Ledger yet.
    LedgerUnavailable,
    /// The ledger still refers to an earlier immutable Goal Contract revision.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Task Ledger.
        ledger_revision: u32,
    },
    /// The requested task anchors no longer match the current durable projections.
    ActivityChanged,
    /// No retained step attempt has created a run yet.
    RunUnavailable,
    /// The selected run is terminal or no longer belongs to an active step attempt.
    RunNotControllable {
        /// Last materialized finite controller state.
        state: AgentControllerState,
    },
    /// An Unknown mutation must be reconciled before Resume or Replan.
    MutationReconciliationRequired,
    /// Resume is unsafe; the user must explicitly choose Replan or Cancel.
    ResumeRequiresReplan,
    /// The explicit choice was atomically committed.
    Applied {
        /// Stable effect of the committed recovery choice.
        outcome: AgentRecoveryOutcomeKind,
        /// New optimistic Task Ledger store version.
        ledger_store_version: TaskLedgerStoreVersion,
        /// Resulting finite controller state.
        state: AgentControllerState,
        /// Number of stale completed steps reopened by the choice.
        reopened_step_count: u32,
        /// Number of abandoned tool attempts marked interrupted during the operation.
        interrupted_tool_attempts: u32,
    },
}

#[derive(Clone)]
struct AgentTaskControlPorts {
    workspace: Arc<dyn TaskLensWorkspaceStore>,
    recovery: Arc<dyn AgentRecoveryStore>,
    journal: Arc<dyn RunJournalStore>,
    ledgers: Arc<dyn TaskLedgerStore>,
    index: Arc<dyn KnowledgeIndexStore>,
}

impl fmt::Debug for AgentTaskControlPorts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AgentTaskControlPorts")
    }
}

impl AgentTaskControlPorts {
    fn activity(&self) -> GetAgentActivity {
        GetAgentActivity::new(Arc::clone(&self.workspace), Arc::clone(&self.journal))
    }
}

/// Inspects H11/E8 recovery state without accepting a run identity from the WebView.
#[derive(Debug, Clone)]
pub struct InspectAgentTaskRecovery {
    ports: AgentTaskControlPorts,
}

impl InspectAgentTaskRecovery {
    /// Composes the existing task, journal, ledger, index, and recovery capabilities.
    #[must_use]
    pub fn new(
        workspace: Arc<dyn TaskLensWorkspaceStore>,
        recovery: Arc<dyn AgentRecoveryStore>,
        journal: Arc<dyn RunJournalStore>,
        ledgers: Arc<dyn TaskLedgerStore>,
        index: Arc<dyn KnowledgeIndexStore>,
    ) -> Self {
        Self {
            ports: AgentTaskControlPorts {
                workspace,
                recovery,
                journal,
                ledgers,
                index,
            },
        }
    }

    /// Derives the one active run and marks attempts abandoned by a prior app stop interrupted.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        observed_at: AgentRunTimestamp,
        workspace_control: &dyn TaskLensWorkspaceControl,
        index_control: &dyn IndexPersistenceControl,
    ) -> Result<AgentTaskRecoveryLoadResult, AgentTaskControlFailure> {
        let target = match load_target(&self.ports, project, task_id, workspace_control).await? {
            ControlTargetResult::Expected(result) => return Ok(result),
            ControlTargetResult::Available(target) => target,
        };
        let inspection = InspectAgentRunRecovery::new(
            self.ports.recovery.as_ref(),
            self.ports.journal.as_ref(),
            self.ports.ledgers.as_ref(),
            self.ports.index.as_ref(),
        )
        .execute(project, target.run_id, observed_at, index_control)
        .await
        .map_err(AgentTaskControlFailure::Recovery)?;
        if inspection.run_id() != target.run_id || inspection.state() != target.state {
            return Ok(AgentTaskRecoveryLoadResult::ActivityChanged);
        }
        Ok(AgentTaskRecoveryLoadResult::Available(
            AgentTaskRecovery::try_from_inspection(inspection, target)?,
        ))
    }
}

/// Revalidates task-visible anchors and atomically commits Resume, Replan, or Cancel.
#[derive(Debug, Clone)]
pub struct ControlAgentTaskRun {
    ports: AgentTaskControlPorts,
}

impl ControlAgentTaskRun {
    /// Composes the same authoritative capabilities as recovery inspection.
    #[must_use]
    pub fn new(
        workspace: Arc<dyn TaskLensWorkspaceStore>,
        recovery: Arc<dyn AgentRecoveryStore>,
        journal: Arc<dyn RunJournalStore>,
        ledgers: Arc<dyn TaskLedgerStore>,
        index: Arc<dyn KnowledgeIndexStore>,
    ) -> Self {
        Self {
            ports: AgentTaskControlPorts {
                workspace,
                recovery,
                journal,
                ledgers,
                index,
            },
        }
    }

    /// Applies one explicit recovery choice after matching the exact UI-visible Ledger anchors.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_ledger_revision: u32,
        expected_ledger_store_version: TaskLedgerStoreVersion,
        choice: AgentRecoveryChoice,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        workspace_control: &dyn TaskLensWorkspaceControl,
        index_control: &dyn IndexPersistenceControl,
    ) -> Result<AgentTaskControlResult, AgentTaskControlFailure> {
        let target = match load_target(&self.ports, project, task_id, workspace_control).await? {
            ControlTargetResult::Expected(result) => return Ok(map_expected_control(result)),
            ControlTargetResult::Available(target) => target,
        };
        if target.ledger_revision != expected_ledger_revision
            || target.ledger_store_version != expected_ledger_store_version
        {
            return Ok(AgentTaskControlResult::ActivityChanged);
        }
        let outcome = RecoverAgentRun::new(
            self.ports.recovery.as_ref(),
            self.ports.journal.as_ref(),
            self.ports.ledgers.as_ref(),
            self.ports.index.as_ref(),
        )
        .execute(
            project,
            target.run_id,
            choice,
            event_id,
            observed_at,
            index_control,
        )
        .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(AgentRecoveryError::MutationReconciliationRequired) => {
                return Ok(AgentTaskControlResult::MutationReconciliationRequired);
            }
            Err(AgentRecoveryError::ResumeRequiresReplan) => {
                return Ok(AgentTaskControlResult::ResumeRequiresReplan);
            }
            Err(AgentRecoveryError::TerminalRun) => {
                return Ok(AgentTaskControlResult::RunNotControllable {
                    state: target.state,
                });
            }
            Err(error) if recovery_conflicted(&error) => {
                return Ok(AgentTaskControlResult::ActivityChanged);
            }
            Err(error) => return Err(AgentTaskControlFailure::Recovery(error)),
        };
        let reopened_step_count = u32::try_from(outcome.reopened_step_ids().len())
            .map_err(|_| AgentTaskControlFailure::ResourceLimit)?;
        Ok(AgentTaskControlResult::Applied {
            outcome: outcome.kind(),
            ledger_store_version: outcome.ledger().version(),
            state: outcome.run().state(),
            reopened_step_count,
            interrupted_tool_attempts: outcome.interrupted_tool_attempts(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ControlTarget {
    run_id: AgentRunId,
    state: AgentControllerState,
    ledger_revision: u32,
    ledger_store_version: TaskLedgerStoreVersion,
}

enum ControlTargetResult {
    Expected(AgentTaskRecoveryLoadResult),
    Available(ControlTarget),
}

async fn load_target(
    ports: &AgentTaskControlPorts,
    project: &ProjectIdentity,
    task_id: TaskId,
    control: &dyn TaskLensWorkspaceControl,
) -> Result<ControlTargetResult, AgentTaskControlFailure> {
    let activity = ports
        .activity()
        .execute(project, task_id, control)
        .await
        .map_err(AgentTaskControlFailure::Activity)?;
    let activity = match activity {
        AgentActivityLoadResult::TaskNotFound => {
            return Ok(ControlTargetResult::Expected(
                AgentTaskRecoveryLoadResult::TaskNotFound,
            ));
        }
        AgentActivityLoadResult::LedgerUnavailable => {
            return Ok(ControlTargetResult::Expected(
                AgentTaskRecoveryLoadResult::LedgerUnavailable,
            ));
        }
        AgentActivityLoadResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => {
            return Ok(ControlTargetResult::Expected(
                AgentTaskRecoveryLoadResult::GoalRevisionMismatch {
                    current_revision,
                    ledger_revision,
                },
            ));
        }
        AgentActivityLoadResult::ActivityChanged => {
            return Ok(ControlTargetResult::Expected(
                AgentTaskRecoveryLoadResult::ActivityChanged,
            ));
        }
        AgentActivityLoadResult::Available(activity) => activity,
    };
    let Some(selected) = activity.run() else {
        return Ok(ControlTargetResult::Expected(
            AgentTaskRecoveryLoadResult::RunUnavailable,
        ));
    };
    if !selected.is_active_attempt() || selected.run().state().is_terminal() {
        return Ok(ControlTargetResult::Expected(
            AgentTaskRecoveryLoadResult::RunNotControllable {
                state: selected.run().state(),
            },
        ));
    }
    let stored = activity.anchor().task_ledger();
    if selected.run().task_ledger_revision() != stored.ledger().revision() {
        return Ok(ControlTargetResult::Expected(
            AgentTaskRecoveryLoadResult::ActivityChanged,
        ));
    }
    Ok(ControlTargetResult::Available(ControlTarget {
        run_id: selected.run().id(),
        state: selected.run().state(),
        ledger_revision: stored.ledger().revision().get(),
        ledger_store_version: stored.version(),
    }))
}

fn map_expected_control(result: AgentTaskRecoveryLoadResult) -> AgentTaskControlResult {
    match result {
        AgentTaskRecoveryLoadResult::TaskNotFound => AgentTaskControlResult::TaskNotFound,
        AgentTaskRecoveryLoadResult::LedgerUnavailable => AgentTaskControlResult::LedgerUnavailable,
        AgentTaskRecoveryLoadResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        } => AgentTaskControlResult::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        },
        AgentTaskRecoveryLoadResult::ActivityChanged => AgentTaskControlResult::ActivityChanged,
        AgentTaskRecoveryLoadResult::RunUnavailable => AgentTaskControlResult::RunUnavailable,
        AgentTaskRecoveryLoadResult::RunNotControllable { state } => {
            AgentTaskControlResult::RunNotControllable { state }
        }
        AgentTaskRecoveryLoadResult::Available(_) => AgentTaskControlResult::ActivityChanged,
    }
}

fn recovery_conflicted(error: &AgentRecoveryError) -> bool {
    matches!(
        error,
        AgentRecoveryError::RunNotFound
            | AgentRecoveryError::LedgerNotFound
            | AgentRecoveryError::AnchorMismatch
            | AgentRecoveryError::Store(
                crate::AgentRecoveryStoreFailure::RunSequenceConflict
                    | crate::AgentRecoveryStoreFailure::LedgerVersionConflict
                    | crate::AgentRecoveryStoreFailure::PublishedSnapshotConflict
            )
    )
}

/// Stable failure classification for task-derived Agent run controls.
#[derive(Debug)]
pub enum AgentTaskControlFailure {
    /// Task or run activity could not be loaded safely.
    Activity(crate::GetAgentActivityFailure),
    /// H11/E8 inspection or recovery failed before a safe result existed.
    Recovery(AgentRecoveryError),
    /// A fixed UI projection count exceeded its exact representation.
    ResourceLimit,
}

impl fmt::Display for AgentTaskControlFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Activity(_) => "Agent task control activity load failed",
            Self::Recovery(_) => "Agent task recovery failed",
            Self::ResourceLimit => "Agent task recovery exceeded a fixed resource limit",
        })
    }
}

impl Error for AgentTaskControlFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Activity(error) => Some(error),
            Self::Recovery(error) => Some(error),
            Self::ResourceLimit => None,
        }
    }
}
