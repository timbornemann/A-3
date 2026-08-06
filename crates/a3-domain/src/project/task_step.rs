use super::{
    AgentRunId, ExpectedTaskEvidence, StepVerification, TaskEvidenceId, TaskLedgerRevision,
    TaskLedgerTimestamp, TaskStepId, VerificationSpec,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_STEP_OUTCOME_BYTES: usize = 8 * 1_024;
const MAX_STEP_RATIONALE_BYTES: usize = 8 * 1_024;
const MAX_STEP_RESULT_BYTES: usize = 8 * 1_024;
const MAX_STEP_REASON_BYTES: usize = 4 * 1_024;
const MAX_STEP_DEPENDENCIES: usize = 64;
const MAX_EXPECTED_EVIDENCE: usize = 32;
const MAX_ATTEMPT_EVIDENCE: usize = 64;

macro_rules! step_text_type {
    ($(#[$metadata:meta])* $name:ident, $field:literal, $maximum:expr) => {
        $(#[$metadata])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Normalizes and validates one bounded non-empty step text value.
            pub fn try_from_string(value: String) -> Result<Self, TaskStepTextError> {
                normalize_text(value, $field, $maximum).map(Self)
            }

            /// Returns the normalized text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("bytes", &self.0.len())
                    .finish_non_exhaustive()
            }
        }
    };
}

step_text_type!(
    /// Concrete result that one task step intends to produce.
    TaskStepOutcome,
    "task-step outcome",
    MAX_STEP_OUTCOME_BYTES
);
step_text_type!(
    /// Bounded explanation for why one task step belongs in the plan.
    TaskStepRationale,
    "task-step rationale",
    MAX_STEP_RATIONALE_BYTES
);
step_text_type!(
    /// Safe retained summary of one execution attempt.
    TaskStepResultSummary,
    "task-step result summary",
    MAX_STEP_RESULT_BYTES
);
step_text_type!(
    /// Reason why execution cannot currently continue.
    TaskStepBlockingReason,
    "task-step blocking reason",
    MAX_STEP_REASON_BYTES
);
step_text_type!(
    /// Reason why one task-step attempt failed before successful verification.
    TaskStepFailureReason,
    "task-step failure reason",
    MAX_STEP_REASON_BYTES
);
step_text_type!(
    /// Reason why a non-completed task step was explicitly cancelled.
    TaskStepCancellationReason,
    "task-step cancellation reason",
    MAX_STEP_REASON_BYTES
);

fn normalize_text(
    value: String,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<String, TaskStepTextError> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.len() > maximum_bytes {
        return Err(TaskStepTextError {
            field,
            violation: TaskStepTextViolation::InvalidLength,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    if trimmed.chars().any(|character| {
        character == '\0' || (character.is_control() && character != '\n' && character != '\t')
    }) {
        return Err(TaskStepTextError {
            field,
            violation: TaskStepTextViolation::InvalidCharacter,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    Ok(trimmed.to_owned())
}

/// Machine-readable rejection class for one task-step text value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStepTextViolation {
    /// Normalized text was empty or exceeded its field-specific UTF-8 byte limit.
    InvalidLength,
    /// Text contained NUL or an unsupported control character.
    InvalidCharacter,
}

/// Invalid bounded task-step text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskStepTextError {
    field: &'static str,
    violation: TaskStepTextViolation,
    actual_bytes: usize,
    maximum_bytes: usize,
}

impl TaskStepTextError {
    /// Returns the stable field name safe for diagnostics.
    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }

    /// Returns the rejected grammar class.
    #[must_use]
    pub const fn violation(self) -> TaskStepTextViolation {
        self.violation
    }

    /// Returns the observed normalized UTF-8 byte length.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }

    /// Returns the fixed field limit.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

impl fmt::Display for TaskStepTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.violation {
            TaskStepTextViolation::InvalidLength => write!(
                formatter,
                "{} has {} bytes; expected 1 through {}",
                self.field, self.actual_bytes, self.maximum_bytes
            ),
            TaskStepTextViolation::InvalidCharacter => {
                write!(
                    formatter,
                    "{} contains an unsupported character",
                    self.field
                )
            }
        }
    }
}

impl Error for TaskStepTextError {}

/// Directed prerequisite edge from one step to an earlier or independent step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepDependency {
    prerequisite: TaskStepId,
}

impl StepDependency {
    /// Creates a dependency on one stable task-step identity.
    #[must_use]
    pub const fn new(prerequisite: TaskStepId) -> Self {
        Self { prerequisite }
    }

    /// Returns the prerequisite step identity.
    #[must_use]
    pub const fn prerequisite(self) -> TaskStepId {
        self.prerequisite
    }
}

/// Immutable definition retained across attempts and replans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStepDefinition {
    id: TaskStepId,
    parent_step_id: Option<TaskStepId>,
    intended_outcome: TaskStepOutcome,
    rationale: TaskStepRationale,
    dependencies: Vec<StepDependency>,
    expected_evidence: Vec<ExpectedTaskEvidence>,
    verification_spec: VerificationSpec,
}

impl TaskStepDefinition {
    /// Creates one bounded definition with unique non-self dependencies and expected evidence.
    pub fn new(
        id: TaskStepId,
        parent_step_id: Option<TaskStepId>,
        intended_outcome: TaskStepOutcome,
        rationale: TaskStepRationale,
        dependencies: Vec<StepDependency>,
        expected_evidence: Vec<ExpectedTaskEvidence>,
        verification_spec: VerificationSpec,
    ) -> Result<Self, TaskStepDefinitionError> {
        if parent_step_id == Some(id) {
            return Err(TaskStepDefinitionError::SelfParent);
        }
        if dependencies.len() > MAX_STEP_DEPENDENCIES {
            return Err(TaskStepDefinitionError::TooManyDependencies(
                dependencies.len(),
            ));
        }
        let dependency_ids = dependencies
            .iter()
            .map(|dependency| dependency.prerequisite())
            .collect::<BTreeSet<_>>();
        if dependency_ids.contains(&id) {
            return Err(TaskStepDefinitionError::SelfDependency);
        }
        if dependency_ids.len() != dependencies.len() {
            return Err(TaskStepDefinitionError::DuplicateDependency);
        }
        if expected_evidence.is_empty() || expected_evidence.len() > MAX_EXPECTED_EVIDENCE {
            return Err(TaskStepDefinitionError::InvalidExpectedEvidenceCount(
                expected_evidence.len(),
            ));
        }
        if expected_evidence.iter().collect::<BTreeSet<_>>().len() != expected_evidence.len() {
            return Err(TaskStepDefinitionError::DuplicateExpectedEvidence);
        }
        Ok(Self {
            id,
            parent_step_id,
            intended_outcome,
            rationale,
            dependencies,
            expected_evidence,
            verification_spec,
        })
    }

    /// Returns the stable step identity.
    #[must_use]
    pub const fn id(&self) -> TaskStepId {
        self.id
    }

    /// Returns the optional structural parent, which does not imply scheduling by itself.
    #[must_use]
    pub const fn parent_step_id(&self) -> Option<TaskStepId> {
        self.parent_step_id
    }

    /// Returns the intended concrete result.
    #[must_use]
    pub const fn intended_outcome(&self) -> &TaskStepOutcome {
        &self.intended_outcome
    }

    /// Returns the bounded planning rationale.
    #[must_use]
    pub const fn rationale(&self) -> &TaskStepRationale {
        &self.rationale
    }

    /// Returns the ordered explicit scheduling prerequisites.
    #[must_use]
    pub fn dependencies(&self) -> &[StepDependency] {
        &self.dependencies
    }

    /// Returns the ordered evidence outcomes expected before completion.
    #[must_use]
    pub fn expected_evidence(&self) -> &[ExpectedTaskEvidence] {
        &self.expected_evidence
    }

    /// Returns the immutable verification specification.
    #[must_use]
    pub const fn verification_spec(&self) -> &VerificationSpec {
        &self.verification_spec
    }
}

/// Invalid local shape of one task-step definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStepDefinitionError {
    /// A step cannot be its own structural parent.
    SelfParent,
    /// A step cannot depend on itself.
    SelfDependency,
    /// One prerequisite appeared more than once.
    DuplicateDependency,
    /// The dependency list exceeded the fixed bound.
    TooManyDependencies(usize),
    /// Every step requires between one and 32 expected evidence descriptions.
    InvalidExpectedEvidenceCount(usize),
    /// One expected evidence description appeared more than once.
    DuplicateExpectedEvidence,
}

impl fmt::Display for TaskStepDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfParent => formatter.write_str("task step cannot be its own parent"),
            Self::SelfDependency => formatter.write_str("task step cannot depend on itself"),
            Self::DuplicateDependency => {
                formatter.write_str("task step contains a duplicate dependency")
            }
            Self::TooManyDependencies(count) => write!(
                formatter,
                "task step has {count} dependencies; maximum is {MAX_STEP_DEPENDENCIES}"
            ),
            Self::InvalidExpectedEvidenceCount(count) => write!(
                formatter,
                "task step has {count} expected evidence items; expected 1 through {MAX_EXPECTED_EVIDENCE}"
            ),
            Self::DuplicateExpectedEvidence => {
                formatter.write_str("task step contains duplicate expected evidence")
            }
        }
    }
}

impl Error for TaskStepDefinitionError {}

/// Materialized task-step state; transition detail remains in immutable attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TaskStepStatus {
    /// At least one prerequisite is not currently completed.
    Pending,
    /// All prerequisites are completed and execution may begin.
    Ready,
    /// One owned run is actively executing this step.
    InProgress,
    /// The active attempt cannot continue without resolving a blocker.
    Blocked,
    /// The active attempt is waiting for scoped user approval.
    AwaitingApproval,
    /// The attempt result is being checked against its immutable specification.
    Verifying,
    /// The latest attempt has successful fresh verification evidence.
    Completed,
    /// Execution failed without a retry transition.
    Failed,
    /// The step was explicitly cancelled.
    Cancelled,
    /// A completed step lost verification freshness and must be rerun.
    Stale,
}

impl TaskStepStatus {
    pub(super) const fn owns_active_attempt(self) -> bool {
        matches!(
            self,
            Self::InProgress | Self::AwaitingApproval | Self::Verifying
        )
    }
}

/// One-based attempt number local to a single task step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskStepAttemptNumber(u32);

impl TaskStepAttemptNumber {
    /// First execution attempt.
    pub const FIRST: Self = Self(1);

    /// Creates a non-zero attempt number.
    pub const fn new(value: u32) -> Result<Self, TaskStepAttemptNumberError> {
        if value == 0 {
            return Err(TaskStepAttemptNumberError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn next(self) -> Result<Self, TaskStepAttemptNumberError> {
        match self.0.checked_add(1) {
            Some(value) => Ok(Self(value)),
            None => Err(TaskStepAttemptNumberError),
        }
    }
}

/// Attempt number was zero or overflowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskStepAttemptNumberError;

impl fmt::Display for TaskStepAttemptNumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task-step attempt number must be non-zero and cannot overflow")
    }
}

impl Error for TaskStepAttemptNumberError {}

/// Immutable terminal classification of one attempt, or `Active` while it is owned by a run.
#[derive(Clone, PartialEq, Eq)]
pub enum TaskStepAttemptOutcome {
    /// Attempt is currently executing, awaiting approval, or verifying.
    Active,
    /// Attempt ended because execution was blocked.
    Blocked {
        /// Bounded reason retained after the attempt ended.
        reason: TaskStepBlockingReason,
    },
    /// Verification failed and the step returned to Ready for a new attempt.
    VerificationFailed,
    /// Verification passed and completed the step.
    Completed,
    /// Execution failed before successful verification.
    Failed {
        /// Bounded execution failure retained for diagnosis and replan.
        reason: TaskStepFailureReason,
    },
    /// Execution was explicitly cancelled.
    Cancelled {
        /// Bounded explicit cancellation reason.
        reason: TaskStepCancellationReason,
    },
}

impl fmt::Debug for TaskStepAttemptOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => formatter.write_str("Active"),
            Self::Blocked { reason } => formatter
                .debug_struct("Blocked")
                .field("reason_bytes", &reason.as_str().len())
                .finish_non_exhaustive(),
            Self::VerificationFailed => formatter.write_str("VerificationFailed"),
            Self::Completed => formatter.write_str("Completed"),
            Self::Failed { reason } => formatter
                .debug_struct("Failed")
                .field("reason_bytes", &reason.as_str().len())
                .finish_non_exhaustive(),
            Self::Cancelled { reason } => formatter
                .debug_struct("Cancelled")
                .field("reason_bytes", &reason.as_str().len())
                .finish_non_exhaustive(),
        }
    }
}

/// One retained execution attempt; terminal attempts are never overwritten by retries.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskStepAttempt {
    number: TaskStepAttemptNumber,
    run_id: AgentRunId,
    started_at: TaskLedgerTimestamp,
    finished_at: Option<TaskLedgerTimestamp>,
    outcome: TaskStepAttemptOutcome,
    result_summary: Option<TaskStepResultSummary>,
    evidence_ids: Vec<TaskEvidenceId>,
    verification: Option<StepVerification>,
}

impl TaskStepAttempt {
    /// Returns the one-based attempt number.
    #[must_use]
    pub const fn number(&self) -> TaskStepAttemptNumber {
        self.number
    }

    /// Returns the controlled run that owned this attempt.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns when execution began.
    #[must_use]
    pub const fn started_at(&self) -> TaskLedgerTimestamp {
        self.started_at
    }

    /// Returns when the attempt became terminal.
    #[must_use]
    pub const fn finished_at(&self) -> Option<TaskLedgerTimestamp> {
        self.finished_at
    }

    /// Returns the immutable terminal classification or Active.
    #[must_use]
    pub const fn outcome(&self) -> &TaskStepAttemptOutcome {
        &self.outcome
    }

    /// Returns the optional safe execution summary.
    #[must_use]
    pub const fn result_summary(&self) -> Option<&TaskStepResultSummary> {
        self.result_summary.as_ref()
    }

    /// Returns execution evidence captured before verification.
    #[must_use]
    pub fn evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.evidence_ids
    }

    /// Returns the retained verification result when one ran.
    #[must_use]
    pub const fn verification(&self) -> Option<&StepVerification> {
        self.verification.as_ref()
    }
}

impl fmt::Debug for TaskStepAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskStepAttempt")
            .field("number", &self.number)
            .field("run_id", &self.run_id)
            .field("started_at", &self.started_at)
            .field("finished_at", &self.finished_at)
            .field("outcome", &self.outcome)
            .field("has_result_summary", &self.result_summary.is_some())
            .field("evidence_count", &self.evidence_ids.len())
            .field("verification", &self.verification)
            .finish_non_exhaustive()
    }
}

/// Why a previously completed step no longer has current verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStepStaleCause {
    /// One or more evidence identities attached to its successful verification were invalidated.
    VerificationEvidence(Vec<TaskEvidenceId>),
    /// A completed prerequisite became stale first.
    Dependency(TaskStepId),
}

/// One task step with immutable definition, materialized state, and retained attempts.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskStep {
    definition: TaskStepDefinition,
    status: TaskStepStatus,
    attempts: Vec<TaskStepAttempt>,
    blocking_reason: Option<TaskStepBlockingReason>,
    stale_cause: Option<TaskStepStaleCause>,
    introduced_in_revision: TaskLedgerRevision,
    retired_in_revision: Option<TaskLedgerRevision>,
}

impl TaskStep {
    pub(super) fn new(
        definition: TaskStepDefinition,
        introduced_in_revision: TaskLedgerRevision,
    ) -> Self {
        Self {
            definition,
            status: TaskStepStatus::Pending,
            attempts: Vec::new(),
            blocking_reason: None,
            stale_cause: None,
            introduced_in_revision,
            retired_in_revision: None,
        }
    }

    /// Returns the immutable step definition.
    #[must_use]
    pub const fn definition(&self) -> &TaskStepDefinition {
        &self.definition
    }

    /// Returns the materialized current status.
    #[must_use]
    pub const fn status(&self) -> TaskStepStatus {
        self.status
    }

    /// Returns every retained attempt in one-based chronological order.
    #[must_use]
    pub fn attempts(&self) -> &[TaskStepAttempt] {
        &self.attempts
    }

    /// Returns the current blocker for Blocked or AwaitingApproval.
    #[must_use]
    pub const fn blocking_reason(&self) -> Option<&TaskStepBlockingReason> {
        self.blocking_reason.as_ref()
    }

    /// Returns the reason a completed step became stale.
    #[must_use]
    pub const fn stale_cause(&self) -> Option<&TaskStepStaleCause> {
        self.stale_cause.as_ref()
    }

    /// Returns the plan revision that introduced this immutable definition.
    #[must_use]
    pub const fn introduced_in_revision(&self) -> TaskLedgerRevision {
        self.introduced_in_revision
    }

    /// Returns the replan revision that retired this future step.
    #[must_use]
    pub const fn retired_in_revision(&self) -> Option<TaskLedgerRevision> {
        self.retired_in_revision
    }

    /// Returns whether this step still participates in the current plan graph.
    #[must_use]
    pub const fn is_active_plan_step(&self) -> bool {
        self.retired_in_revision.is_none()
    }

    pub(super) fn mark_ready(&mut self) {
        if self.status == TaskStepStatus::Pending {
            self.status = TaskStepStatus::Ready;
        }
    }

    pub(super) fn mark_pending(&mut self) {
        if self.status == TaskStepStatus::Ready {
            self.status = TaskStepStatus::Pending;
        }
    }

    pub(super) fn start(
        &mut self,
        run_id: AgentRunId,
        started_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if self.status != TaskStepStatus::Ready {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        if self
            .attempts
            .last()
            .and_then(TaskStepAttempt::finished_at)
            .is_some_and(|finished_at| started_at < finished_at)
        {
            return Err(TaskStepTransitionError::TimestampRegressed);
        }
        let number = match self.attempts.last().map(TaskStepAttempt::number) {
            Some(previous) => previous
                .next()
                .map_err(|_| TaskStepTransitionError::AttemptOverflow)?,
            None => TaskStepAttemptNumber::FIRST,
        };
        self.attempts.push(TaskStepAttempt {
            number,
            run_id,
            started_at,
            finished_at: None,
            outcome: TaskStepAttemptOutcome::Active,
            result_summary: None,
            evidence_ids: Vec::new(),
            verification: None,
        });
        self.status = TaskStepStatus::InProgress;
        Ok(())
    }

    pub(super) fn await_approval(
        &mut self,
        run_id: AgentRunId,
        reason: TaskStepBlockingReason,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_owned_active_attempt(run_id, TaskStepStatus::InProgress)?;
        self.blocking_reason = Some(reason);
        self.status = TaskStepStatus::AwaitingApproval;
        Ok(())
    }

    pub(super) fn resume_after_approval(
        &mut self,
        run_id: AgentRunId,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_owned_active_attempt(run_id, TaskStepStatus::AwaitingApproval)?;
        self.blocking_reason = None;
        self.status = TaskStepStatus::InProgress;
        Ok(())
    }

    pub(super) fn begin_verification(
        &mut self,
        run_id: AgentRunId,
        result_summary: Option<TaskStepResultSummary>,
        evidence_ids: Vec<TaskEvidenceId>,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_owned_active_attempt(run_id, TaskStepStatus::InProgress)?;
        validate_attempt_evidence(&evidence_ids)?;
        let attempt = self.active_attempt_mut()?;
        attempt.result_summary = result_summary;
        attempt.evidence_ids = evidence_ids;
        self.status = TaskStepStatus::Verifying;
        Ok(())
    }

    pub(super) fn finish_verification(
        &mut self,
        verification: StepVerification,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_owned_active_attempt(verification.run_id(), TaskStepStatus::Verifying)?;
        if verification.spec_id() != self.definition.verification_spec().id() {
            return Err(TaskStepTransitionError::VerificationSpecMismatch);
        }
        let attempt = self.active_attempt_mut()?;
        if verification.verified_at() < attempt.started_at {
            return Err(TaskStepTransitionError::TimestampRegressed);
        }
        let passed = verification.passed();
        attempt.finished_at = Some(verification.verified_at());
        attempt.outcome = if passed {
            TaskStepAttemptOutcome::Completed
        } else {
            TaskStepAttemptOutcome::VerificationFailed
        };
        attempt.verification = Some(verification);
        self.status = if passed {
            TaskStepStatus::Completed
        } else {
            TaskStepStatus::Ready
        };
        Ok(())
    }

    pub(super) fn block(
        &mut self,
        run_id: AgentRunId,
        reason: TaskStepBlockingReason,
        finished_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskStepTransitionError> {
        if !matches!(
            self.status,
            TaskStepStatus::InProgress | TaskStepStatus::AwaitingApproval
        ) {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.ensure_active_run(run_id)?;
        let attempt = self.active_attempt_mut()?;
        if finished_at < attempt.started_at {
            return Err(TaskStepTransitionError::TimestampRegressed);
        }
        attempt.finished_at = Some(finished_at);
        attempt.outcome = TaskStepAttemptOutcome::Blocked {
            reason: reason.clone(),
        };
        self.blocking_reason = Some(reason);
        self.status = TaskStepStatus::Blocked;
        Ok(())
    }

    pub(super) fn unblock(&mut self) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if self.status != TaskStepStatus::Blocked {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.blocking_reason = None;
        self.status = TaskStepStatus::Pending;
        Ok(())
    }

    pub(super) fn fail(
        &mut self,
        run_id: AgentRunId,
        reason: TaskStepFailureReason,
        finished_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskStepTransitionError> {
        if !self.status.owns_active_attempt() {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.ensure_active_run(run_id)?;
        let attempt = self.active_attempt_mut()?;
        if finished_at < attempt.started_at {
            return Err(TaskStepTransitionError::TimestampRegressed);
        }
        attempt.finished_at = Some(finished_at);
        attempt.outcome = TaskStepAttemptOutcome::Failed { reason };
        self.blocking_reason = None;
        self.status = TaskStepStatus::Failed;
        Ok(())
    }

    pub(super) fn cancel(
        &mut self,
        reason: TaskStepCancellationReason,
        cancelled_at: TaskLedgerTimestamp,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if matches!(
            self.status,
            TaskStepStatus::Completed | TaskStepStatus::Stale | TaskStepStatus::Cancelled
        ) {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        if self.status.owns_active_attempt() {
            let attempt = self.active_attempt_mut()?;
            if cancelled_at < attempt.started_at {
                return Err(TaskStepTransitionError::TimestampRegressed);
            }
            attempt.finished_at = Some(cancelled_at);
            attempt.outcome = TaskStepAttemptOutcome::Cancelled {
                reason: reason.clone(),
            };
        }
        self.blocking_reason = None;
        self.status = TaskStepStatus::Cancelled;
        Ok(())
    }

    pub(super) fn mark_stale(
        &mut self,
        cause: TaskStepStaleCause,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if self.status != TaskStepStatus::Completed {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.stale_cause = Some(cause);
        self.status = TaskStepStatus::Stale;
        Ok(())
    }

    pub(super) fn reopen_stale(&mut self) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if self.status != TaskStepStatus::Stale {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.stale_cause = None;
        self.status = TaskStepStatus::Pending;
        Ok(())
    }

    pub(super) fn retire(
        &mut self,
        revision: TaskLedgerRevision,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if !matches!(
            self.status,
            TaskStepStatus::Pending
                | TaskStepStatus::Ready
                | TaskStepStatus::Blocked
                | TaskStepStatus::Failed
                | TaskStepStatus::Cancelled
        ) {
            return Err(TaskStepTransitionError::CannotRetireHistoricalStep);
        }
        self.retired_in_revision = Some(revision);
        Ok(())
    }

    pub(super) fn active_run_id(&self) -> Option<AgentRunId> {
        self.status
            .owns_active_attempt()
            .then(|| self.attempts.last().map(TaskStepAttempt::run_id))
            .flatten()
    }

    pub(super) fn successful_verification(&self) -> Option<&StepVerification> {
        self.attempts
            .iter()
            .rev()
            .filter_map(TaskStepAttempt::verification)
            .find(|verification| verification.passed())
    }

    fn ensure_active(&self) -> Result<(), TaskStepTransitionError> {
        if self.retired_in_revision.is_some() {
            Err(TaskStepTransitionError::RetiredStep)
        } else {
            Ok(())
        }
    }

    fn ensure_owned_active_attempt(
        &self,
        run_id: AgentRunId,
        expected_status: TaskStepStatus,
    ) -> Result<(), TaskStepTransitionError> {
        self.ensure_active()?;
        if self.status != expected_status {
            return Err(TaskStepTransitionError::InvalidStatus);
        }
        self.ensure_active_run(run_id)
    }

    fn ensure_active_run(&self, run_id: AgentRunId) -> Result<(), TaskStepTransitionError> {
        if self.active_run_id() == Some(run_id) {
            Ok(())
        } else {
            Err(TaskStepTransitionError::RunMismatch)
        }
    }

    fn active_attempt_mut(&mut self) -> Result<&mut TaskStepAttempt, TaskStepTransitionError> {
        self.attempts
            .last_mut()
            .filter(|attempt| matches!(attempt.outcome, TaskStepAttemptOutcome::Active))
            .ok_or(TaskStepTransitionError::MissingActiveAttempt)
    }
}

impl fmt::Debug for TaskStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskStep")
            .field("definition", &self.definition)
            .field("status", &self.status)
            .field("attempts", &self.attempts)
            .field("has_blocking_reason", &self.blocking_reason.is_some())
            .field("stale_cause", &self.stale_cause)
            .field("introduced_in_revision", &self.introduced_in_revision)
            .field("retired_in_revision", &self.retired_in_revision)
            .finish_non_exhaustive()
    }
}

fn validate_attempt_evidence(
    evidence_ids: &[TaskEvidenceId],
) -> Result<(), TaskStepTransitionError> {
    if evidence_ids.len() > MAX_ATTEMPT_EVIDENCE {
        return Err(TaskStepTransitionError::TooMuchEvidence(evidence_ids.len()));
    }
    if evidence_ids.iter().copied().collect::<BTreeSet<_>>().len() != evidence_ids.len() {
        return Err(TaskStepTransitionError::DuplicateEvidence);
    }
    Ok(())
}

/// Rejected state transition for one materialized task step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStepTransitionError {
    /// The transition is not legal from the current materialized status.
    InvalidStatus,
    /// A retired future step cannot transition.
    RetiredStep,
    /// Current state claimed activity without a matching open attempt.
    MissingActiveAttempt,
    /// A different controlled run owns the open attempt.
    RunMismatch,
    /// Verification result did not evaluate this step's immutable specification.
    VerificationSpecMismatch,
    /// A transition timestamp preceded the attempt or previous terminal time.
    TimestampRegressed,
    /// Attempt numbering reached its persisted maximum.
    AttemptOverflow,
    /// Execution evidence exceeded the fixed bound.
    TooMuchEvidence(usize),
    /// Execution evidence repeated an identity.
    DuplicateEvidence,
    /// Replan attempted to retire a completed, stale, or executing step.
    CannotRetireHistoricalStep,
}

impl fmt::Display for TaskStepTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStatus => formatter.write_str("task-step transition is not allowed"),
            Self::RetiredStep => formatter.write_str("retired task step cannot transition"),
            Self::MissingActiveAttempt => {
                formatter.write_str("task step has no matching active attempt")
            }
            Self::RunMismatch => formatter.write_str("task-step attempt belongs to another run"),
            Self::VerificationSpecMismatch => {
                formatter.write_str("verification used another specification")
            }
            Self::TimestampRegressed => {
                formatter.write_str("task-step transition timestamp regressed")
            }
            Self::AttemptOverflow => formatter.write_str("task-step attempt number overflowed"),
            Self::TooMuchEvidence(count) => write!(
                formatter,
                "task-step attempt has {count} evidence items; maximum is {MAX_ATTEMPT_EVIDENCE}"
            ),
            Self::DuplicateEvidence => {
                formatter.write_str("task-step attempt contains duplicate evidence")
            }
            Self::CannotRetireHistoricalStep => {
                formatter.write_str("replan may retire only future task steps")
            }
        }
    }
}

impl Error for TaskStepTransitionError {}
