use super::{
    AgentRunId, GoalContractReference, StepVerification, TaskEvidenceId, TaskId,
    TaskLedgerTimestamp, TaskStep, TaskStepBlockingReason, TaskStepCancellationReason,
    TaskStepDefinition, TaskStepFailureReason, TaskStepId, TaskStepResultSummary,
    TaskStepStaleCause, TaskStepStatus, TaskStepTransitionError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const MAX_ACTIVE_STEPS: usize = 256;
const MAX_RETAINED_STEPS: usize = 1_024;
const MAX_INVALIDATED_EVIDENCE: usize = 64;
const MAX_REPLAN_REASON_BYTES: usize = 4 * 1_024;

/// One-based immutable revision of the current Task Ledger plan graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLedgerRevision(u32);

impl TaskLedgerRevision {
    /// Initial plan revision.
    pub const INITIAL: Self = Self(1);

    /// Creates a non-zero ledger revision.
    pub const fn new(value: u32) -> Result<Self, TaskLedgerRevisionError> {
        if value == 0 {
            return Err(TaskLedgerRevisionError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn next(self) -> Result<Self, TaskLedgerRevisionError> {
        match self.0.checked_add(1) {
            Some(value) => Ok(Self(value)),
            None => Err(TaskLedgerRevisionError),
        }
    }
}

/// Task Ledger revision was zero or overflowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLedgerRevisionError;

impl fmt::Display for TaskLedgerRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Task Ledger revision must be non-zero and cannot overflow")
    }
}

impl Error for TaskLedgerRevisionError {}

/// Bounded material reason retained for every replan after revision one.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskReplanReason(String);

impl TaskReplanReason {
    /// Normalizes line endings and validates a non-empty replan explanation.
    pub fn try_from_string(value: String) -> Result<Self, TaskReplanReasonError> {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_REPLAN_REASON_BYTES {
            return Err(TaskReplanReasonError {
                violation: TaskReplanReasonViolation::InvalidLength,
                actual_bytes: trimmed.len(),
            });
        }
        if trimmed.chars().any(|character| {
            character == '\0' || (character.is_control() && character != '\n' && character != '\t')
        }) {
            return Err(TaskReplanReasonError {
                violation: TaskReplanReasonViolation::InvalidCharacter,
                actual_bytes: trimmed.len(),
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the normalized explanation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TaskReplanReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskReplanReason")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Machine-readable rejection class for a replan reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskReplanReasonViolation {
    /// Normalized text was empty or exceeded 4 KiB UTF-8.
    InvalidLength,
    /// Text contained NUL or an unsupported control character.
    InvalidCharacter,
}

/// Invalid bounded replan reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskReplanReasonError {
    violation: TaskReplanReasonViolation,
    actual_bytes: usize,
}

impl TaskReplanReasonError {
    /// Returns the rejected grammar class.
    #[must_use]
    pub const fn violation(self) -> TaskReplanReasonViolation {
        self.violation
    }

    /// Returns the observed normalized UTF-8 byte length.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }
}

impl fmt::Display for TaskReplanReasonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.violation {
            TaskReplanReasonViolation::InvalidLength => write!(
                formatter,
                "replan reason has {} bytes; expected 1 through {MAX_REPLAN_REASON_BYTES}",
                self.actual_bytes
            ),
            TaskReplanReasonViolation::InvalidCharacter => {
                formatter.write_str("replan reason contains an unsupported character")
            }
        }
    }
}

impl Error for TaskReplanReasonError {}

/// Immutable audit record for one material Task Ledger replan.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskLedgerReplan {
    revision: TaskLedgerRevision,
    previous_revision: TaskLedgerRevision,
    reason: TaskReplanReason,
    retired_step_ids: Vec<TaskStepId>,
    added_step_ids: Vec<TaskStepId>,
    created_at: TaskLedgerTimestamp,
}

impl TaskLedgerReplan {
    /// Returns the plan revision produced by this replan.
    #[must_use]
    pub const fn revision(&self) -> TaskLedgerRevision {
        self.revision
    }

    /// Returns its immediate predecessor revision.
    #[must_use]
    pub const fn previous_revision(&self) -> TaskLedgerRevision {
        self.previous_revision
    }

    /// Returns the required material explanation.
    #[must_use]
    pub const fn reason(&self) -> &TaskReplanReason {
        &self.reason
    }

    /// Returns future steps removed from the current plan graph.
    #[must_use]
    pub fn retired_step_ids(&self) -> &[TaskStepId] {
        &self.retired_step_ids
    }

    /// Returns immutable definitions introduced by this revision.
    #[must_use]
    pub fn added_step_ids(&self) -> &[TaskStepId] {
        &self.added_step_ids
    }

    /// Returns when the replan was committed.
    #[must_use]
    pub const fn created_at(&self) -> TaskLedgerTimestamp {
        self.created_at
    }
}

impl fmt::Debug for TaskLedgerReplan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskLedgerReplan")
            .field("revision", &self.revision)
            .field("previous_revision", &self.previous_revision)
            .field("reason", &self.reason)
            .field("retired_step_ids", &self.retired_step_ids)
            .field("added_step_ids", &self.added_step_ids)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

/// Result of invalidating verification evidence in one Task Ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLedgerInvalidation {
    direct_step_ids: Vec<TaskStepId>,
    dependent_step_ids: Vec<TaskStepId>,
}

impl TaskLedgerInvalidation {
    /// Returns completed steps whose own verification evidence became invalid.
    #[must_use]
    pub fn direct_step_ids(&self) -> &[TaskStepId] {
        &self.direct_step_ids
    }

    /// Returns completed transitive dependents reopened for re-verification.
    #[must_use]
    pub fn dependent_step_ids(&self) -> &[TaskStepId] {
        &self.dependent_step_ids
    }

    /// Returns whether no current completed step depended on the invalidated evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.direct_step_ids.is_empty() && self.dependent_step_ids.is_empty()
    }
}

/// Revisioned task plan with a materialized current state and retained attempts/replans.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskLedger {
    goal_contract: GoalContractReference,
    revision: TaskLedgerRevision,
    steps: BTreeMap<TaskStepId, TaskStep>,
    replans: Vec<TaskLedgerReplan>,
    created_at: TaskLedgerTimestamp,
    updated_at: TaskLedgerTimestamp,
}

impl TaskLedger {
    /// Creates revision one only from a valid Goal Contract reference and an acyclic bounded plan.
    pub fn new(
        goal_contract: GoalContractReference,
        definitions: Vec<TaskStepDefinition>,
        created_at: TaskLedgerTimestamp,
    ) -> Result<Self, TaskLedgerError> {
        if definitions.is_empty() || definitions.len() > MAX_ACTIVE_STEPS {
            return Err(TaskLedgerError::InvalidActiveStepCount(definitions.len()));
        }
        let mut steps = BTreeMap::new();
        let mut specification_ids = BTreeSet::new();
        for definition in definitions {
            let id = definition.id();
            if !specification_ids.insert(definition.verification_spec().id()) {
                return Err(TaskLedgerError::DuplicateVerificationSpec);
            }
            if steps
                .insert(id, TaskStep::new(definition, TaskLedgerRevision::INITIAL))
                .is_some()
            {
                return Err(TaskLedgerError::DuplicateStep(id));
            }
        }
        validate_active_graph(&steps)?;
        let mut ledger = Self {
            goal_contract,
            revision: TaskLedgerRevision::INITIAL,
            steps,
            replans: Vec::new(),
            created_at,
            updated_at: created_at,
        };
        ledger.refresh_readiness();
        Ok(ledger)
    }

    /// Returns the durable task identity inherited from the Goal Contract.
    #[must_use]
    pub const fn task_id(&self) -> TaskId {
        self.goal_contract.task_id()
    }

    /// Returns the exact Goal Contract revision this plan currently serves.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the current plan revision.
    #[must_use]
    pub const fn revision(&self) -> TaskLedgerRevision {
        self.revision
    }

    /// Returns every retained step in stable identity order, including retired history.
    pub fn steps(&self) -> impl ExactSizeIterator<Item = &TaskStep> {
        self.steps.values()
    }

    /// Returns one current or historical step by stable identity.
    #[must_use]
    pub fn step(&self, step_id: TaskStepId) -> Option<&TaskStep> {
        self.steps.get(&step_id)
    }

    /// Returns the append-only replan history after revision one.
    #[must_use]
    pub fn replans(&self) -> &[TaskLedgerReplan] {
        &self.replans
    }

    /// Returns initial plan creation time.
    #[must_use]
    pub const fn created_at(&self) -> TaskLedgerTimestamp {
        self.created_at
    }

    /// Returns the latest successful materialized transition time.
    #[must_use]
    pub const fn updated_at(&self) -> TaskLedgerTimestamp {
        self.updated_at
    }

    /// Starts one ready step and prevents a second concurrently active ledger attempt.
    pub fn start_step(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        started_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(started_at)?;
        if self.has_active_step() {
            return Err(TaskLedgerError::ActiveStepExists);
        }
        if !self.dependencies_completed(step_id)? {
            return Err(TaskLedgerError::DependencyNotCompleted);
        }
        self.step_mut(step_id)?
            .start(run_id, started_at)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = started_at;
        Ok(())
    }

    /// Moves the currently executing attempt to scoped approval wait.
    pub fn await_step_approval(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        reason: TaskStepBlockingReason,
        transitioned_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(transitioned_at)?;
        self.step_mut(step_id)?
            .await_approval(run_id, reason)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = transitioned_at;
        Ok(())
    }

    /// Resumes the same open attempt after scoped approval.
    pub fn resume_step_after_approval(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        transitioned_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(transitioned_at)?;
        self.step_mut(step_id)?
            .resume_after_approval(run_id)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = transitioned_at;
        Ok(())
    }

    /// Captures the bounded attempt result before deterministic verification.
    pub fn begin_step_verification(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        result_summary: Option<TaskStepResultSummary>,
        evidence_ids: Vec<TaskEvidenceId>,
        transitioned_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(transitioned_at)?;
        self.step_mut(step_id)?
            .begin_verification(run_id, result_summary, evidence_ids)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = transitioned_at;
        Ok(())
    }

    /// Applies one verification result; only Passed may materialize Completed.
    pub fn finish_step_verification(
        &mut self,
        step_id: TaskStepId,
        verification: StepVerification,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(verification.verified_at())?;
        let verified_at = verification.verified_at();
        self.step_mut(step_id)?
            .finish_verification(verification)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = verified_at;
        self.refresh_readiness();
        Ok(())
    }

    /// Ends the active attempt as blocked while retaining it for later diagnosis.
    pub fn block_step(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        reason: TaskStepBlockingReason,
        blocked_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(blocked_at)?;
        self.step_mut(step_id)?
            .block(run_id, reason, blocked_at)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = blocked_at;
        Ok(())
    }

    /// Reopens one blocked step and recalculates dependency readiness.
    pub fn unblock_step(
        &mut self,
        step_id: TaskStepId,
        transitioned_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(transitioned_at)?;
        self.step_mut(step_id)?
            .unblock()
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = transitioned_at;
        self.refresh_readiness();
        Ok(())
    }

    /// Ends the active attempt and materializes Failed without fabricating verification.
    pub fn fail_step(
        &mut self,
        step_id: TaskStepId,
        run_id: AgentRunId,
        reason: TaskStepFailureReason,
        failed_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(failed_at)?;
        self.step_mut(step_id)?
            .fail(run_id, reason, failed_at)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = failed_at;
        self.refresh_readiness();
        Ok(())
    }

    /// Cancels one non-completed current-plan step.
    pub fn cancel_step(
        &mut self,
        step_id: TaskStepId,
        reason: TaskStepCancellationReason,
        cancelled_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(cancelled_at)?;
        self.step_mut(step_id)?
            .cancel(reason, cancelled_at)
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = cancelled_at;
        self.refresh_readiness();
        Ok(())
    }

    /// Marks directly and transitively dependent completed steps stale in one deterministic pass.
    pub fn invalidate_verification_evidence(
        &mut self,
        evidence_ids: Vec<TaskEvidenceId>,
        invalidated_at: TaskLedgerTimestamp,
    ) -> Result<TaskLedgerInvalidation, TaskLedgerError> {
        self.ensure_time(invalidated_at)?;
        if evidence_ids.is_empty() || evidence_ids.len() > MAX_INVALIDATED_EVIDENCE {
            return Err(TaskLedgerError::InvalidEvidenceCount(evidence_ids.len()));
        }
        let invalidated = evidence_ids.iter().copied().collect::<BTreeSet<_>>();
        if invalidated.len() != evidence_ids.len() {
            return Err(TaskLedgerError::DuplicateEvidence);
        }

        let mut direct = BTreeSet::new();
        let mut direct_evidence = BTreeMap::new();
        for (step_id, step) in &self.steps {
            if !step.is_active_plan_step() || step.status() != TaskStepStatus::Completed {
                continue;
            }
            let affected = step
                .successful_verification()
                .into_iter()
                .flat_map(StepVerification::evidence_ids)
                .filter(|id| invalidated.contains(id))
                .copied()
                .collect::<Vec<_>>();
            if !affected.is_empty() {
                direct.insert(*step_id);
                direct_evidence.insert(*step_id, affected);
            }
        }

        let mut all_stale = direct.clone();
        let mut dependency_causes = BTreeMap::new();
        loop {
            let mut changed = false;
            for (step_id, step) in &self.steps {
                if !step.is_active_plan_step()
                    || step.status() != TaskStepStatus::Completed
                    || all_stale.contains(step_id)
                {
                    continue;
                }
                if let Some(prerequisite) = step
                    .definition()
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.prerequisite())
                    .find(|dependency| all_stale.contains(dependency))
                {
                    all_stale.insert(*step_id);
                    dependency_causes.insert(*step_id, prerequisite);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for step_id in &all_stale {
            let cause = if let Some(affected) = direct_evidence.remove(step_id) {
                TaskStepStaleCause::VerificationEvidence(affected)
            } else {
                TaskStepStaleCause::Dependency(
                    *dependency_causes
                        .get(step_id)
                        .ok_or(TaskLedgerError::InvalidMaterializedState)?,
                )
            };
            self.step_mut(*step_id)?
                .mark_stale(cause)
                .map_err(TaskLedgerError::StepTransition)?;
        }

        let dependent = all_stale.difference(&direct).copied().collect::<Vec<_>>();
        let direct = direct.into_iter().collect::<Vec<_>>();
        if !direct.is_empty() || !dependent.is_empty() {
            self.updated_at = invalidated_at;
            self.refresh_readiness();
        }
        Ok(TaskLedgerInvalidation {
            direct_step_ids: direct,
            dependent_step_ids: dependent,
        })
    }

    /// Reopens one stale step; it becomes Ready only after all prerequisites are completed again.
    pub fn reopen_stale_step(
        &mut self,
        step_id: TaskStepId,
        transitioned_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskLedgerError> {
        self.ensure_time(transitioned_at)?;
        self.step_mut(step_id)?
            .reopen_stale()
            .map_err(TaskLedgerError::StepTransition)?;
        self.updated_at = transitioned_at;
        self.refresh_readiness();
        Ok(())
    }

    /// Retires selected future steps and adds immutable replacements as the next plan revision.
    pub fn replan(
        &mut self,
        retire_step_ids: Vec<TaskStepId>,
        additions: Vec<TaskStepDefinition>,
        reason: TaskReplanReason,
        replanned_at: TaskLedgerTimestamp,
    ) -> Result<TaskLedgerRevision, TaskLedgerError> {
        self.ensure_time(replanned_at)?;
        if self.has_active_step() {
            return Err(TaskLedgerError::ActiveStepExists);
        }
        if retire_step_ids.is_empty() && additions.is_empty() {
            return Err(TaskLedgerError::ReplanHasNoMaterialChange);
        }
        let retired = retire_step_ids.iter().copied().collect::<BTreeSet<_>>();
        if retired.len() != retire_step_ids.len() {
            return Err(TaskLedgerError::DuplicateRetirement);
        }
        let revision = self
            .revision
            .next()
            .map_err(|_| TaskLedgerError::RevisionOverflow)?;
        let mut candidate = self.clone();
        for step_id in &retired {
            candidate
                .step_mut(*step_id)?
                .retire(revision)
                .map_err(TaskLedgerError::StepTransition)?;
        }

        let mut specification_ids = candidate
            .steps
            .values()
            .map(|step| step.definition().verification_spec().id())
            .collect::<BTreeSet<_>>();
        let mut added_ids = Vec::with_capacity(additions.len());
        for definition in additions {
            let step_id = definition.id();
            if candidate.steps.contains_key(&step_id) {
                return Err(TaskLedgerError::StepIdAlreadyUsed(step_id));
            }
            if !specification_ids.insert(definition.verification_spec().id()) {
                return Err(TaskLedgerError::DuplicateVerificationSpec);
            }
            candidate
                .steps
                .insert(step_id, TaskStep::new(definition, revision));
            added_ids.push(step_id);
        }
        if candidate.steps.len() > MAX_RETAINED_STEPS {
            return Err(TaskLedgerError::TooManyRetainedSteps(candidate.steps.len()));
        }
        let active_count = candidate
            .steps
            .values()
            .filter(|step| step.is_active_plan_step())
            .count();
        if active_count == 0 || active_count > MAX_ACTIVE_STEPS {
            return Err(TaskLedgerError::InvalidActiveStepCount(active_count));
        }
        validate_active_graph(&candidate.steps)?;

        let mut retired_step_ids = retired.into_iter().collect::<Vec<_>>();
        retired_step_ids.sort_unstable();
        added_ids.sort_unstable();
        candidate.replans.push(TaskLedgerReplan {
            revision,
            previous_revision: self.revision,
            reason,
            retired_step_ids,
            added_step_ids: added_ids,
            created_at: replanned_at,
        });
        candidate.revision = revision;
        candidate.updated_at = replanned_at;
        candidate.refresh_readiness();
        *self = candidate;
        Ok(revision)
    }

    fn step_mut(&mut self, step_id: TaskStepId) -> Result<&mut TaskStep, TaskLedgerError> {
        self.steps
            .get_mut(&step_id)
            .ok_or(TaskLedgerError::UnknownStep(step_id))
    }

    fn ensure_time(&self, timestamp: TaskLedgerTimestamp) -> Result<(), TaskLedgerError> {
        if timestamp < self.updated_at {
            Err(TaskLedgerError::TimestampRegressed)
        } else {
            Ok(())
        }
    }

    fn has_active_step(&self) -> bool {
        self.steps
            .values()
            .any(|step| step.status().owns_active_attempt())
    }

    fn dependencies_completed(&self, step_id: TaskStepId) -> Result<bool, TaskLedgerError> {
        let step = self
            .steps
            .get(&step_id)
            .ok_or(TaskLedgerError::UnknownStep(step_id))?;
        Ok(step.definition().dependencies().iter().all(|dependency| {
            self.steps
                .get(&dependency.prerequisite())
                .is_some_and(|prerequisite| {
                    prerequisite.is_active_plan_step()
                        && prerequisite.status() == TaskStepStatus::Completed
                })
        }))
    }

    fn refresh_readiness(&mut self) {
        let completed = self
            .steps
            .iter()
            .filter_map(|(id, step)| {
                (step.is_active_plan_step() && step.status() == TaskStepStatus::Completed)
                    .then_some(*id)
            })
            .collect::<BTreeSet<_>>();
        for step in self.steps.values_mut() {
            if !step.is_active_plan_step() {
                continue;
            }
            let dependencies_completed = step
                .definition()
                .dependencies()
                .iter()
                .all(|dependency| completed.contains(&dependency.prerequisite()));
            if dependencies_completed {
                step.mark_ready();
            } else {
                step.mark_pending();
            }
        }
    }
}

impl fmt::Debug for TaskLedger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskLedger")
            .field("task_id", &self.task_id())
            .field("goal_revision", &self.goal_contract.revision())
            .field("revision", &self.revision)
            .field("steps", &self.steps)
            .field("replans", &self.replans)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

fn validate_active_graph(steps: &BTreeMap<TaskStepId, TaskStep>) -> Result<(), TaskLedgerError> {
    let active = steps
        .iter()
        .filter_map(|(id, step)| step.is_active_plan_step().then_some((*id, step)))
        .collect::<BTreeMap<_, _>>();
    let mut dependency_edges = BTreeMap::<TaskStepId, Vec<TaskStepId>>::new();
    let mut dependency_indegree = active
        .keys()
        .copied()
        .map(|id| (id, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut parent_edges = BTreeMap::<TaskStepId, Vec<TaskStepId>>::new();
    let mut parent_indegree = dependency_indegree.clone();

    for (step_id, step) in &active {
        if let Some(parent_id) = step.definition().parent_step_id() {
            if !active.contains_key(&parent_id) {
                return Err(TaskLedgerError::MissingParent {
                    step_id: *step_id,
                    parent_id,
                });
            }
            parent_edges.entry(parent_id).or_default().push(*step_id);
            *parent_indegree
                .get_mut(step_id)
                .ok_or(TaskLedgerError::InvalidMaterializedState)? += 1;
        }
        for dependency in step.definition().dependencies() {
            let prerequisite = dependency.prerequisite();
            if !active.contains_key(&prerequisite) {
                return Err(TaskLedgerError::MissingDependency {
                    step_id: *step_id,
                    prerequisite,
                });
            }
            dependency_edges
                .entry(prerequisite)
                .or_default()
                .push(*step_id);
            *dependency_indegree
                .get_mut(step_id)
                .ok_or(TaskLedgerError::InvalidMaterializedState)? += 1;
        }
    }
    if !is_acyclic(&dependency_edges, dependency_indegree) {
        return Err(TaskLedgerError::DependencyCycle);
    }
    if !is_acyclic(&parent_edges, parent_indegree) {
        return Err(TaskLedgerError::ParentCycle);
    }
    Ok(())
}

fn is_acyclic(
    edges: &BTreeMap<TaskStepId, Vec<TaskStepId>>,
    mut indegree: BTreeMap<TaskStepId, usize>,
) -> bool {
    let mut ready = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut visited = 0_usize;
    while let Some(step_id) = ready.pop_first() {
        visited += 1;
        for dependent in edges.get(&step_id).into_iter().flatten() {
            let Some(degree) = indegree.get_mut(dependent) else {
                return false;
            };
            let Some(next) = degree.checked_sub(1) else {
                return false;
            };
            *degree = next;
            if next == 0 {
                ready.insert(*dependent);
            }
        }
    }
    visited == indegree.len()
}

/// Invalid Task Ledger graph, replan, evidence invalidation, or state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLedgerError {
    /// Current plan must contain between one and 256 active steps.
    InvalidActiveStepCount(usize),
    /// Historical retention exceeded the fixed 1,024-step bound.
    TooManyRetainedSteps(usize),
    /// Two definitions used the same stable step identity.
    DuplicateStep(TaskStepId),
    /// A replan attempted to reuse an identity retained in history.
    StepIdAlreadyUsed(TaskStepId),
    /// Two steps used the same immutable verification specification identity.
    DuplicateVerificationSpec,
    /// Structural parent was missing from the current plan graph.
    MissingParent {
        /// Step containing the invalid parent reference.
        step_id: TaskStepId,
        /// Missing current-plan parent.
        parent_id: TaskStepId,
    },
    /// Scheduling prerequisite was missing from the current plan graph.
    MissingDependency {
        /// Step containing the invalid dependency.
        step_id: TaskStepId,
        /// Missing current-plan prerequisite.
        prerequisite: TaskStepId,
    },
    /// Explicit scheduling dependencies contain a cycle.
    DependencyCycle,
    /// Structural parent relationships contain a cycle.
    ParentCycle,
    /// Requested step identity is unknown.
    UnknownStep(TaskStepId),
    /// Another step already owns the ledger's active attempt.
    ActiveStepExists,
    /// A step was started before every current prerequisite completed.
    DependencyNotCompleted,
    /// A transition timestamp preceded the latest committed ledger transition.
    TimestampRegressed,
    /// Replan neither retired nor added a step.
    ReplanHasNoMaterialChange,
    /// Replan repeated a retirement identity.
    DuplicateRetirement,
    /// Plan revision could not advance.
    RevisionOverflow,
    /// Invalidation requires one through 64 evidence identities.
    InvalidEvidenceCount(usize),
    /// Invalidation repeated an evidence identity.
    DuplicateEvidence,
    /// Persisted or internally derived state contradicted aggregate invariants.
    InvalidMaterializedState,
    /// One task-step transition was rejected.
    StepTransition(TaskStepTransitionError),
}

impl fmt::Display for TaskLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidActiveStepCount(count) => write!(
                formatter,
                "Task Ledger has {count} active steps; expected 1 through {MAX_ACTIVE_STEPS}"
            ),
            Self::TooManyRetainedSteps(count) => write!(
                formatter,
                "Task Ledger retains {count} steps; maximum is {MAX_RETAINED_STEPS}"
            ),
            Self::DuplicateStep(_) => formatter.write_str("Task Ledger contains duplicate steps"),
            Self::StepIdAlreadyUsed(_) => {
                formatter.write_str("Task Ledger step identity is already retained in history")
            }
            Self::DuplicateVerificationSpec => {
                formatter.write_str("Task Ledger contains a duplicate verification specification")
            }
            Self::MissingParent { .. } => {
                formatter.write_str("Task Ledger step references a missing parent")
            }
            Self::MissingDependency { .. } => {
                formatter.write_str("Task Ledger step references a missing dependency")
            }
            Self::DependencyCycle => {
                formatter.write_str("Task Ledger dependencies contain a cycle")
            }
            Self::ParentCycle => formatter.write_str("Task Ledger parents contain a cycle"),
            Self::UnknownStep(_) => formatter.write_str("Task Ledger step was not found"),
            Self::ActiveStepExists => {
                formatter.write_str("Task Ledger already has an active step attempt")
            }
            Self::DependencyNotCompleted => {
                formatter.write_str("Task Ledger step dependencies are not completed")
            }
            Self::TimestampRegressed => {
                formatter.write_str("Task Ledger transition timestamp regressed")
            }
            Self::ReplanHasNoMaterialChange => {
                formatter.write_str("Task Ledger replan did not change the plan")
            }
            Self::DuplicateRetirement => {
                formatter.write_str("Task Ledger replan repeats a retirement")
            }
            Self::RevisionOverflow => formatter.write_str("Task Ledger revision overflowed"),
            Self::InvalidEvidenceCount(count) => write!(
                formatter,
                "Task Ledger invalidation has {count} evidence IDs; expected 1 through {MAX_INVALIDATED_EVIDENCE}"
            ),
            Self::DuplicateEvidence => {
                formatter.write_str("Task Ledger invalidation repeats evidence")
            }
            Self::InvalidMaterializedState => {
                formatter.write_str("Task Ledger materialized state is invalid")
            }
            Self::StepTransition(error) => {
                write!(formatter, "Task Ledger transition failed: {error}")
            }
        }
    }
}

impl Error for TaskLedgerError {}

#[cfg(test)]
mod tests {
    use super::{TaskLedger, TaskLedgerError, TaskLedgerTimestamp, TaskReplanReason};
    use crate::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
        ExpectedTaskEvidence, GoalContract, GoalContractDraft, GoalContractTimestamp,
        GoalObjective, StepDependency, StepVerification, StepVerificationId,
        StepVerificationOutcome, SuccessVerification, TaskEvidenceId, TaskId, TaskStepDefinition,
        TaskStepId, TaskStepOutcome, TaskStepRationale, TaskStepStatus, VerificationFailureSummary,
        VerificationMethod, VerificationRequirement, VerificationSpec, VerificationSpecId,
    };
    use std::error::Error;

    #[test]
    fn cyclic_dependencies_are_rejected() -> Result<(), Box<dyn Error>> {
        let first = step(1, vec![StepDependency::new(step_id(2))])?;
        let second = step(2, vec![StepDependency::new(step_id(1))])?;

        assert_eq!(
            TaskLedger::new(goal_reference()?, vec![first, second], timestamp(1)?),
            Err(TaskLedgerError::DependencyCycle)
        );
        Ok(())
    }

    #[test]
    fn failed_verification_never_completes_and_retry_preserves_the_attempt()
    -> Result<(), Box<dyn Error>> {
        let first_id = step_id(1);
        let second_id = step_id(2);
        let mut ledger = TaskLedger::new(
            goal_reference()?,
            vec![
                step(1, Vec::new())?,
                step(2, vec![StepDependency::new(first_id)])?,
            ],
            timestamp(1)?,
        )?;
        assert_eq!(
            ledger.step(first_id).map(|step| step.status()),
            Some(TaskStepStatus::Ready)
        );
        assert_eq!(
            ledger.step(second_id).map(|step| step.status()),
            Some(TaskStepStatus::Pending)
        );

        let run = AgentRunId::from_bytes([9; 32]);
        ledger.start_step(first_id, run, timestamp(2)?)?;
        ledger.begin_step_verification(first_id, run, None, Vec::new(), timestamp(3)?)?;
        ledger.finish_step_verification(first_id, verification(1, 1, run, false, 4, 41)?)?;
        assert_eq!(
            ledger.step(first_id).map(|step| step.status()),
            Some(TaskStepStatus::Ready)
        );

        ledger.start_step(first_id, run, timestamp(5)?)?;
        ledger.begin_step_verification(first_id, run, None, Vec::new(), timestamp(6)?)?;
        ledger.finish_step_verification(first_id, verification(2, 1, run, true, 7, 42)?)?;

        let first = ledger.step(first_id).ok_or("first step missing")?;
        assert_eq!(first.status(), TaskStepStatus::Completed);
        assert_eq!(first.attempts().len(), 2);
        assert!(matches!(
            first.attempts()[0]
                .verification()
                .map(StepVerification::outcome),
            Some(StepVerificationOutcome::Failed { .. })
        ));
        assert_eq!(
            ledger.step(second_id).map(|step| step.status()),
            Some(TaskStepStatus::Ready)
        );
        Ok(())
    }

    #[test]
    fn only_one_step_attempt_can_be_active() -> Result<(), Box<dyn Error>> {
        let mut ledger = TaskLedger::new(
            goal_reference()?,
            vec![step(1, Vec::new())?, step(2, Vec::new())?],
            timestamp(1)?,
        )?;
        ledger.start_step(step_id(1), AgentRunId::from_bytes([1; 32]), timestamp(2)?)?;

        assert_eq!(
            ledger.start_step(step_id(2), AgentRunId::from_bytes([2; 32]), timestamp(3)?),
            Err(TaskLedgerError::ActiveStepExists)
        );
        Ok(())
    }

    #[test]
    fn invalidation_marks_direct_and_transitive_completed_steps_stale() -> Result<(), Box<dyn Error>>
    {
        let first_id = step_id(1);
        let second_id = step_id(2);
        let mut ledger = TaskLedger::new(
            goal_reference()?,
            vec![
                step(1, Vec::new())?,
                step(2, vec![StepDependency::new(first_id)])?,
            ],
            timestamp(1)?,
        )?;
        let run = AgentRunId::from_bytes([9; 32]);
        complete(&mut ledger, first_id, 1, run, 2, 51)?;
        complete(&mut ledger, second_id, 2, run, 5, 52)?;

        let report = ledger.invalidate_verification_evidence(
            vec![TaskEvidenceId::from_bytes([51; 32])],
            timestamp(8)?,
        )?;

        assert_eq!(report.direct_step_ids(), &[first_id]);
        assert_eq!(report.dependent_step_ids(), &[second_id]);
        assert_eq!(
            ledger.step(first_id).map(|step| step.status()),
            Some(TaskStepStatus::Stale)
        );
        assert_eq!(
            ledger.step(second_id).map(|step| step.status()),
            Some(TaskStepStatus::Stale)
        );
        Ok(())
    }

    #[test]
    fn replan_retires_future_steps_without_losing_history() -> Result<(), Box<dyn Error>> {
        let retired_id = step_id(2);
        let added_id = step_id(3);
        let mut ledger = TaskLedger::new(
            goal_reference()?,
            vec![step(1, Vec::new())?, step(2, Vec::new())?],
            timestamp(1)?,
        )?;

        let revision = ledger.replan(
            vec![retired_id],
            vec![step(3, Vec::new())?],
            TaskReplanReason::try_from_string("the previous approach was blocked".to_owned())?,
            timestamp(2)?,
        )?;

        assert_eq!(revision.get(), 2);
        assert_eq!(ledger.steps().len(), 3);
        assert_eq!(
            ledger
                .step(retired_id)
                .and_then(|step| step.retired_in_revision()),
            Some(revision)
        );
        assert_eq!(
            ledger.step(added_id).map(|step| step.status()),
            Some(TaskStepStatus::Ready)
        );
        assert_eq!(ledger.replans().len(), 1);
        assert_eq!(ledger.replans()[0].retired_step_ids(), &[retired_id]);
        assert_eq!(ledger.replans()[0].added_step_ids(), &[added_id]);
        Ok(())
    }

    fn complete(
        ledger: &mut TaskLedger,
        step_id: TaskStepId,
        spec: u8,
        run: AgentRunId,
        start_time: u64,
        evidence: u8,
    ) -> Result<(), Box<dyn Error>> {
        ledger.start_step(step_id, run, timestamp(start_time)?)?;
        ledger.begin_step_verification(
            step_id,
            run,
            None,
            Vec::new(),
            timestamp(start_time + 1)?,
        )?;
        ledger.finish_step_verification(
            step_id,
            verification(spec, spec, run, true, start_time + 2, evidence)?,
        )?;
        Ok(())
    }

    fn verification(
        id: u8,
        spec: u8,
        run: AgentRunId,
        passed: bool,
        at: u64,
        evidence: u8,
    ) -> Result<StepVerification, Box<dyn Error>> {
        let outcome = if passed {
            StepVerificationOutcome::Passed
        } else {
            StepVerificationOutcome::Failed {
                summary: VerificationFailureSummary::try_from_string(
                    "the verifier observed a failure".to_owned(),
                )?,
            }
        };
        Ok(StepVerification::new(
            StepVerificationId::from_bytes([id; 32]),
            VerificationSpecId::from_bytes([spec; 32]),
            run,
            outcome,
            vec![TaskEvidenceId::from_bytes([evidence; 32])],
            timestamp(at)?,
        )?)
    }

    fn step(
        id: u8,
        dependencies: Vec<StepDependency>,
    ) -> Result<TaskStepDefinition, Box<dyn Error>> {
        Ok(TaskStepDefinition::new(
            step_id(id),
            None,
            TaskStepOutcome::try_from_string(format!("produce outcome {id}"))?,
            TaskStepRationale::try_from_string(format!("required rationale {id}"))?,
            dependencies,
            vec![ExpectedTaskEvidence::try_from_string(format!(
                "evidence for step {id}"
            ))?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([id; 32]),
                VerificationMethod::Test,
                VerificationRequirement::try_from_string(format!("verify step {id}"))?,
            ),
        )?)
    }

    const fn step_id(value: u8) -> TaskStepId {
        TaskStepId::from_bytes([value; 32])
    }

    fn timestamp(value: u64) -> Result<TaskLedgerTimestamp, Box<dyn Error>> {
        Ok(TaskLedgerTimestamp::from_unix_millis(value)?)
    }

    fn goal_reference() -> Result<crate::GoalContractReference, Box<dyn Error>> {
        Ok(GoalContract::initial(
            TaskId::from_bytes([99; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("implement the Task Ledger".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([98; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "completed requires verification".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run ledger tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        )
        .reference())
    }
}
