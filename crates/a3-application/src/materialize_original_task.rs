use crate::{
    CreateGoalContract, CreateGoalContractFailure, CreateTaskLedger, GoalContractStore,
    GoalContractStoreFailure, StoredTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure,
};
use a3_domain::{
    AcceptanceCriterionId, CriterionVerificationMismatch, GoalContract, GoalContractReference,
    GoalOriginalBindingError, ProjectIdentity, TaskLedger, TaskLedgerError, TaskLedgerRevision,
    TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepStatus,
    VerificationMethod,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

/// Initial small ledger and its still-unplanned criteria; neither is a Done claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedOriginalTask {
    goal_contract: GoalContractReference,
    stored_ledger: StoredTaskLedger,
    unplanned_criteria: Vec<AcceptanceCriterionId>,
}

impl MaterializedOriginalTask {
    /// Returns the immutable original-bound goal that owns this ledger.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the created or exactly reused initial ledger and its store version.
    #[must_use]
    pub const fn stored_ledger(&self) -> &StoredTaskLedger {
        &self.stored_ledger
    }

    /// Returns criteria without a final Proves assignment in this horizon, in goal order.
    /// Progress-only Addresses do not remove any original final-proof obligation.
    #[must_use]
    pub fn unplanned_criteria(&self) -> &[AcceptanceCriterionId] {
        &self.unplanned_criteria
    }

    /// Projects the persisted initial horizon for display in deterministic ledger order.
    #[must_use]
    pub fn preview(&self) -> OriginalPlanPreview<'_> {
        let ledger = self.stored_ledger.ledger();
        let steps: Vec<_> = ledger
            .steps()
            .map(|step| OriginalPlanPreviewStep {
                id: step.definition().id(),
                outcome: step.definition().intended_outcome(),
                status: step.status(),
                verification: step.definition().verification_spec().method(),
            })
            .collect();
        let next_step = steps
            .iter()
            .find(|step| step.status == TaskStepStatus::Ready)
            .map(|step| step.id);
        OriginalPlanPreview {
            goal: self.goal_contract,
            revision: ledger.revision(),
            steps,
            next_step,
            unplanned_criteria: &self.unplanned_criteria,
        }
    }
}

/// Presentation of the one-to-three-step initial horizon derived from the stored ledger.
#[derive(Debug, PartialEq, Eq)]
pub struct OriginalPlanPreview<'a> {
    /// Exact goal revision that owns these steps.
    pub goal: GoalContractReference,
    /// Revision of the displayed plan.
    pub revision: TaskLedgerRevision,
    /// Bounded intended results and their checks, in ledger order.
    pub steps: Vec<OriginalPlanPreviewStep<'a>>,
    /// The first Ready step in ledger order.
    pub next_step: Option<TaskStepId>,
    /// Original criteria whose final proofs have yet to be assigned.
    pub unplanned_criteria: &'a [AcceptanceCriterionId],
}

/// One display row, with its outcome borrowed from the durable definition.
#[derive(Debug, PartialEq, Eq)]
pub struct OriginalPlanPreviewStep<'a> {
    /// Identity of the stored step.
    pub id: TaskStepId,
    /// Concrete intended result.
    pub outcome: &'a TaskStepOutcome,
    /// Current scheduling status of the initial step.
    pub status: TaskStepStatus,
    /// Method required to verify this step.
    pub verification: VerificationMethod,
}

/// Materializes only Core-prepared original-bound goals and one to three next steps.
///
/// Uses existing Goal/Ledger ports without starting a run or executing any tool.
/// Goal and Ledger writes are separate: if Ledger persistence fails, the valid Goal
/// remains durable. Only an explicit later call can reuse that exact Goal and retry.
#[derive(Debug, Clone, Copy)]
pub struct MaterializeOriginalTask<'a> {
    goals: &'a dyn GoalContractStore,
    ledgers: &'a dyn TaskLedgerStore,
}

impl<'a> MaterializeOriginalTask<'a> {
    /// Selects existing persistence capabilities; no new storage or execution authority is added.
    #[must_use]
    pub const fn new(goals: &'a dyn GoalContractStore, ledgers: &'a dyn TaskLedgerStore) -> Self {
        Self { goals, ledgers }
    }

    /// Validates every mapping before writes, then creates or exactly reuses initial state.
    ///
    /// Concurrent creates and differing durable state are returned, never repaired or retried.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        definitions: Vec<TaskStepDefinition>,
    ) -> Result<MaterializedOriginalTask, MaterializeOriginalTaskFailure> {
        let ledger = prepare_initial_task_ledger(goal, definitions)?;
        match self
            .goals
            .load_current_goal_contract(project, goal.task_id())
            .await
            .map_err(MaterializeOriginalTaskFailure::GoalRead)?
        {
            Some(stored) if stored == *goal => {}
            Some(_) => return Err(MaterializeOriginalTaskFailure::GoalConflict),
            None => {
                CreateGoalContract::new(self.goals)
                    .execute(project, goal)
                    .await
                    .map_err(MaterializeOriginalTaskFailure::GoalCreate)?;
            }
        }
        let stored_ledger = match self
            .ledgers
            .load_task_ledger(project, goal.task_id())
            .await
            .map_err(MaterializeOriginalTaskFailure::Ledger)?
        {
            Some(stored) if stored.ledger() == &ledger => stored,
            Some(_) => return Err(MaterializeOriginalTaskFailure::LedgerConflict),
            None => CreateTaskLedger::new(self.ledgers)
                .execute(project, &ledger)
                .await
                .map_err(MaterializeOriginalTaskFailure::Ledger)?,
        };
        // Progress does not erase the original final-proof planning obligation.
        let mapped = ledger
            .steps()
            .flat_map(|step| step.definition().acceptance_criteria().iter().copied())
            .collect::<BTreeSet<_>>();
        let unplanned_criteria = goal
            .draft()
            .acceptance_criteria()
            .iter()
            .filter(|criterion| !mapped.contains(&criterion.id()))
            .map(|criterion| criterion.id())
            .collect();
        Ok(MaterializedOriginalTask {
            goal_contract: goal.reference(),
            stored_ledger,
            unplanned_criteria,
        })
    }
}

pub(crate) fn prepare_initial_task_ledger(
    goal: &GoalContract,
    definitions: Vec<TaskStepDefinition>,
) -> Result<TaskLedger, MaterializeOriginalTaskFailure> {
    if goal.original().is_none() {
        return Err(MaterializeOriginalTaskFailure::MissingOriginal);
    }
    goal.validate_original_binding()
        .map_err(MaterializeOriginalTaskFailure::OriginalBinding)?;
    if definitions.is_empty() || definitions.len() > 3 {
        return Err(MaterializeOriginalTaskFailure::StepCount {
            actual: definitions.len(),
        });
    }
    for definition in &definitions {
        crate::AdmitSourceTaskStep
            .execute(goal, definition)
            .map_err(|failure| step_admission_failure(definition.id(), failure))?;
    }
    let at = TaskLedgerTimestamp::from_unix_millis(goal.created_at().unix_millis())
        .map_err(|_| MaterializeOriginalTaskFailure::InvalidTimestamp)?;
    TaskLedger::new(goal.reference(), definitions, at)
        .map_err(MaterializeOriginalTaskFailure::InvalidLedger)
}

/// Validation, exact-state conflict or typed persistence failure; no path starts a run.
#[derive(Debug)]
pub enum MaterializeOriginalTaskFailure {
    /// The candidate requires a captured original request.
    MissingOriginal,
    /// The supplied original and criterion bindings do not form one valid aggregate.
    OriginalBinding(GoalOriginalBindingError),
    /// The first bounded horizon is empty or exceeds three steps.
    StepCount {
        /// Observed count, without plan or source content.
        actual: usize,
    },
    /// A step did not identify any original goal criterion.
    MissingCriterionMapping {
        /// Core-owned offending step.
        step_id: TaskStepId,
    },
    /// A mapped criterion does not belong to the supplied goal.
    UnknownCriterion {
        /// Core-owned offending step.
        step_id: TaskStepId,
        /// Criterion not found in that goal.
        criterion_id: AcceptanceCriterionId,
    },
    /// A mapped criterion has no proof obligation from the original.
    MissingCriterionOrigin {
        /// Criterion whose original grounding is absent.
        criterion_id: AcceptanceCriterionId,
    },
    /// A step weakens or replaces the original criterion's proof obligation.
    ProofMismatch {
        /// Core-owned offending step.
        step_id: TaskStepId,
        /// Bound original criterion.
        criterion_id: AcceptanceCriterionId,
        /// Closed mismatch reason.
        mismatch: CriterionVerificationMismatch,
    },
    /// A Source step does not yet have an executable check of its own declared result.
    NonOperationalVerification {
        /// Core-owned offending step.
        step_id: TaskStepId,
    },
    /// Goal time could not form the portable deterministic ledger timestamp.
    InvalidTimestamp,
    /// Existing Task Ledger invariants rejected the graph before persistence.
    InvalidLedger(TaskLedgerError),
    /// Reading the durable goal failed.
    GoalRead(GoalContractStoreFailure),
    /// Creating the absent initial goal failed; no automatic race retry follows.
    GoalCreate(CreateGoalContractFailure),
    /// An existing goal differs; no overwrite or automatic revision is permitted.
    GoalConflict,
    /// Ledger read/create failed; an already created valid Goal remains durable.
    Ledger(TaskLedgerStoreFailure),
    /// Existing ledger differs, including started, completed or differently prepared work.
    LedgerConflict,
}

impl fmt::Display for MaterializeOriginalTaskFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingOriginal => "original task materialization requires an original request",
            Self::OriginalBinding(_) => "original task binding is invalid",
            Self::StepCount { .. } => "original task requires one through three next steps",
            Self::MissingCriterionMapping { .. } => "task step has no goal criterion mapping",
            Self::UnknownCriterion { .. } => "task step references another goal criterion",
            Self::MissingCriterionOrigin { .. } => {
                "task criterion has no original proof obligation"
            }
            Self::ProofMismatch { .. } => {
                "task step does not preserve the original proof obligation"
            }
            Self::NonOperationalVerification { .. } => "task step verification is not operational",
            Self::InvalidTimestamp => "goal time cannot form the task ledger timestamp",
            Self::InvalidLedger(_) => "initial task ledger is invalid",
            Self::GoalRead(_) => "original task goal read failed",
            Self::GoalCreate(_) => "original task goal creation failed",
            Self::GoalConflict => "durable goal differs from the requested original task",
            Self::Ledger(_) => "original task ledger persistence failed",
            Self::LedgerConflict => "durable ledger differs from the requested initial task",
        })
    }
}

impl Error for MaterializeOriginalTaskFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OriginalBinding(error) => Some(error),
            Self::InvalidLedger(error) => Some(error),
            Self::GoalRead(error) => Some(error),
            Self::GoalCreate(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::ProofMismatch { mismatch, .. } => Some(mismatch),
            _ => None,
        }
    }
}

fn step_admission_failure(
    step_id: TaskStepId,
    failure: crate::SourceTaskStepAdmissionFailure,
) -> MaterializeOriginalTaskFailure {
    use crate::SourceTaskStepAdmissionFailure as Failure;
    match failure {
        Failure::InvalidOriginalBinding(error) => {
            MaterializeOriginalTaskFailure::OriginalBinding(error)
        }
        Failure::MissingCriterionMapping => {
            MaterializeOriginalTaskFailure::MissingCriterionMapping { step_id }
        }
        Failure::UnknownCriterion(criterion_id) => {
            MaterializeOriginalTaskFailure::UnknownCriterion {
                step_id,
                criterion_id,
            }
        }
        Failure::MissingCriterionOrigin(criterion_id) => {
            MaterializeOriginalTaskFailure::MissingCriterionOrigin { criterion_id }
        }
        Failure::NonOperationalVerification => {
            MaterializeOriginalTaskFailure::NonOperationalVerification { step_id }
        }
        Failure::ProofMismatch {
            criterion_id,
            mismatch,
        } => MaterializeOriginalTaskFailure::ProofMismatch {
            step_id,
            criterion_id,
            mismatch,
        },
    }
}
