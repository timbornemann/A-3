use crate::{
    AgentControllerControl, GetTaskLensTask, StoredTaskLedger, StoredVerificationState,
    TaskLensTaskAnchor, TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
    TaskLensWorkspaceStore, VerificationEvidenceStore, VerificationEvidenceStoreFailure,
};
use a3_domain::{
    AcceptanceCriterion, CommandEvidence, DiagnosticCount, DiagnosticEvidence, DiffEvidence,
    DiffEvidenceSource, EvidenceFreshness, ProcessDuration, ProcessOutputDigest,
    ProcessOutputRedaction, ProcessStreamEvidence, ProcessTermination, ProjectIdentity,
    PublishedIndex, RepositoryPath, SnapshotId, StepVerificationOutcome, TaskEvidenceId, TaskId,
    TaskStepAttemptNumber, TaskStepDefinition, TaskStepId, TaskStepStaleCause, TaskStepStatus,
    TestCaseEvidence, TestCaseOutcome, TestEvidence, UserConfirmationEvidence,
    VerificationEvidence, VerificationEvidenceEvaluation, VerificationMethod, VerificationSpec,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

const INSPECTION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_INSPECTION_EVIDENCE: usize = 256 * 64;
const MAX_VISIBLE_TEST_CASES: usize = 100;

/// Cooperative cancellation shared by task-anchor and verification-evidence reads.
pub trait VerificationInspectionControl: AgentControllerControl + TaskLensWorkspaceControl {}

impl<T> VerificationInspectionControl for T where
    T: AgentControllerControl + TaskLensWorkspaceControl + ?Sized
{
}

/// Content-free metadata for one completely drained durable process stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationProcessStreamInspection {
    digest: ProcessOutputDigest,
    observed_bytes: u64,
    retained_limit: u32,
    truncated: bool,
    redaction: Option<ProcessOutputRedaction>,
}

impl VerificationProcessStreamInspection {
    fn from_evidence(value: ProcessStreamEvidence) -> Self {
        Self {
            digest: value.digest(),
            observed_bytes: value.observed_bytes(),
            retained_limit: value.retained_limit(),
            truncated: value.truncated(),
            redaction: value.redaction(),
        }
    }

    /// Returns the digest of all observed bytes.
    #[must_use]
    pub const fn digest(self) -> ProcessOutputDigest {
        self.digest
    }

    /// Returns all drained bytes including discarded overflow.
    #[must_use]
    pub const fn observed_bytes(self) -> u64 {
        self.observed_bytes
    }

    /// Returns the E4 retention limit used during execution.
    #[must_use]
    pub const fn retained_limit(self) -> u32 {
        self.retained_limit
    }

    /// Returns whether overflow was discarded after hashing.
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }

    /// Returns why retained text was withheld, if applicable.
    #[must_use]
    pub const fn redaction(self) -> Option<ProcessOutputRedaction> {
        self.redaction
    }
}

/// Durable process metadata shared by Command, Test, and Diagnostic evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationCommandInspection {
    command_id: a3_domain::DiscoveredCommandId,
    termination: ProcessTermination,
    duration: ProcessDuration,
    stdout: VerificationProcessStreamInspection,
    stderr: VerificationProcessStreamInspection,
}

impl VerificationCommandInspection {
    fn from_evidence(value: &CommandEvidence) -> Self {
        Self {
            command_id: value.command_id(),
            termination: value.termination(),
            duration: value.duration(),
            stdout: VerificationProcessStreamInspection::from_evidence(value.stdout()),
            stderr: VerificationProcessStreamInspection::from_evidence(value.stderr()),
        }
    }

    /// Returns the exact allowlisted command identity.
    #[must_use]
    pub const fn command_id(&self) -> a3_domain::DiscoveredCommandId {
        self.command_id
    }

    /// Returns exit, timeout, or cancellation.
    #[must_use]
    pub const fn termination(&self) -> ProcessTermination {
        self.termination
    }

    /// Returns the monotonic runtime.
    #[must_use]
    pub const fn duration(&self) -> ProcessDuration {
        self.duration
    }

    /// Returns durable stdout metadata without text.
    #[must_use]
    pub const fn stdout(&self) -> VerificationProcessStreamInspection {
        self.stdout
    }

    /// Returns durable stderr metadata without text.
    #[must_use]
    pub const fn stderr(&self) -> VerificationProcessStreamInspection {
        self.stderr
    }
}

/// Bounded structured test semantics retained for the desktop inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationTestInspection {
    command: VerificationCommandInspection,
    passed: u64,
    failed: u64,
    ignored: u64,
    visible_cases: Vec<TestCaseEvidence>,
    cases_truncated: bool,
}

impl VerificationTestInspection {
    fn from_evidence(value: &TestEvidence) -> Result<Self, VerificationInspectionBuildError> {
        let mut passed = 0u64;
        let mut failed = 0u64;
        let mut ignored = 0u64;
        for case in value.cases() {
            match case.outcome() {
                TestCaseOutcome::Passed => passed = checked_increment(passed)?,
                TestCaseOutcome::Failed => failed = checked_increment(failed)?,
                TestCaseOutcome::Ignored => ignored = checked_increment(ignored)?,
            }
        }
        Ok(Self {
            command: VerificationCommandInspection::from_evidence(value.command()),
            passed,
            failed,
            ignored,
            visible_cases: value
                .cases()
                .iter()
                .take(MAX_VISIBLE_TEST_CASES)
                .cloned()
                .collect(),
            cases_truncated: value.cases().len() > MAX_VISIBLE_TEST_CASES,
        })
    }

    /// Returns the underlying bounded command metadata.
    #[must_use]
    pub const fn command(&self) -> &VerificationCommandInspection {
        &self.command
    }

    /// Returns the exact number of passed cases.
    #[must_use]
    pub const fn passed(&self) -> u64 {
        self.passed
    }

    /// Returns the exact number of failed cases.
    #[must_use]
    pub const fn failed(&self) -> u64 {
        self.failed
    }

    /// Returns the exact number of ignored cases.
    #[must_use]
    pub const fn ignored(&self) -> u64 {
        self.ignored
    }

    /// Returns at most the first one hundred canonical structured cases.
    #[must_use]
    pub fn visible_cases(&self) -> &[TestCaseEvidence] {
        &self.visible_cases
    }

    /// Returns whether additional structured cases were omitted from this projection.
    #[must_use]
    pub const fn cases_truncated(&self) -> bool {
        self.cases_truncated
    }
}

/// Structured diagnostic counts and their underlying process metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationDiagnosticInspection {
    command: VerificationCommandInspection,
    errors: DiagnosticCount,
    warnings: DiagnosticCount,
}

impl VerificationDiagnosticInspection {
    fn from_evidence(value: &DiagnosticEvidence) -> Self {
        Self {
            command: VerificationCommandInspection::from_evidence(value.command()),
            errors: value.errors(),
            warnings: value.warnings(),
        }
    }

    /// Returns the underlying bounded command metadata.
    #[must_use]
    pub const fn command(&self) -> &VerificationCommandInspection {
        &self.command
    }

    /// Returns structured error diagnostics.
    #[must_use]
    pub const fn errors(&self) -> DiagnosticCount {
        self.errors
    }

    /// Returns structured warning diagnostics.
    #[must_use]
    pub const fn warnings(&self) -> DiagnosticCount {
        self.warnings
    }
}

/// Actual content-free changed-path semantics and trusted source provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationDiffInspection {
    source: DiffEvidenceSource,
    base_snapshot_id: SnapshotId,
    snapshot_id: SnapshotId,
    changed_paths: Vec<RepositoryPath>,
    complete: bool,
}

impl VerificationDiffInspection {
    fn from_evidence(value: &DiffEvidence) -> Self {
        Self {
            source: value.source(),
            base_snapshot_id: value.base_snapshot_id(),
            snapshot_id: value.snapshot_id(),
            changed_paths: value.changed_paths().to_vec(),
            complete: value.complete(),
        }
    }

    /// Returns exact E3-patch or ordered-index provenance.
    #[must_use]
    pub const fn source(&self) -> DiffEvidenceSource {
        self.source
    }

    /// Returns the pre-observation snapshot.
    #[must_use]
    pub const fn base_snapshot_id(&self) -> SnapshotId {
        self.base_snapshot_id
    }

    /// Returns the observed resulting snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns canonical actual changed paths.
    #[must_use]
    pub fn changed_paths(&self) -> &[RepositoryPath] {
        &self.changed_paths
    }

    /// Returns whether every authorized operation completed.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
}

/// Closed semantic details for every durable E6 artifact kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationEvidenceDetail {
    /// Generic command evidence.
    Command(VerificationCommandInspection),
    /// Structured test evidence.
    Test(VerificationTestInspection),
    /// Actual changed-path evidence.
    Diff(VerificationDiffInspection),
    /// Structured diagnostic evidence.
    Diagnostic(VerificationDiagnosticInspection),
    /// Exact user-confirmed policy scope.
    UserConfirmation {
        /// Confirmed content-free scope identity.
        scope_id: a3_domain::PolicyResourceId,
        /// Durable confirmation timestamp.
        confirmed_at: a3_domain::TaskLedgerTimestamp,
    },
}

/// One durable artifact re-evaluated against its spec and latest published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationEvidenceInspection {
    id: TaskEvidenceId,
    run_id: a3_domain::AgentRunId,
    snapshot_id: SnapshotId,
    method: VerificationMethod,
    semantic: VerificationEvidenceEvaluation,
    freshness: EvidenceFreshness,
    detail: VerificationEvidenceDetail,
}

impl VerificationEvidenceInspection {
    fn build(
        spec: &VerificationSpec,
        evidence: &VerificationEvidence,
        published: &PublishedIndex,
    ) -> Result<Self, VerificationInspectionBuildError> {
        if evidence.spec_id() != spec.id() || evidence.method() != spec.method() {
            return Err(VerificationInspectionBuildError::EvidenceMismatch);
        }
        let detail = match evidence {
            VerificationEvidence::Command(value) => VerificationEvidenceDetail::Command(
                VerificationCommandInspection::from_evidence(value),
            ),
            VerificationEvidence::Test(value) => {
                VerificationEvidenceDetail::Test(VerificationTestInspection::from_evidence(value)?)
            }
            VerificationEvidence::Diff(value) => {
                VerificationEvidenceDetail::Diff(VerificationDiffInspection::from_evidence(value))
            }
            VerificationEvidence::Diagnostic(value) => VerificationEvidenceDetail::Diagnostic(
                VerificationDiagnosticInspection::from_evidence(value),
            ),
            VerificationEvidence::UserConfirmation(value) => user_confirmation_detail(value),
        };
        Ok(Self {
            id: evidence.id(),
            run_id: evidence.run_id(),
            snapshot_id: evidence.snapshot_id(),
            method: evidence.method(),
            semantic: VerificationEvidenceEvaluation::evaluate(spec, evidence),
            freshness: EvidenceFreshness::evaluate(evidence, published),
            detail,
        })
    }

    /// Returns the immutable durable evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the controlled run owning the evidence.
    #[must_use]
    pub const fn run_id(&self) -> a3_domain::AgentRunId {
        self.run_id
    }

    /// Returns the observation snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact verification category.
    #[must_use]
    pub const fn method(&self) -> VerificationMethod {
        self.method
    }

    /// Returns freshly derived typed semantics, independent of exit code alone.
    #[must_use]
    pub const fn semantic(&self) -> VerificationEvidenceEvaluation {
        self.semantic
    }

    /// Returns freshly derived dependency or snapshot freshness.
    #[must_use]
    pub const fn freshness(&self) -> EvidenceFreshness {
        self.freshness
    }

    /// Returns method-specific bounded details.
    #[must_use]
    pub const fn detail(&self) -> &VerificationEvidenceDetail {
        &self.detail
    }
}

/// One retained step attempt that reached typed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationAttemptInspection {
    number: TaskStepAttemptNumber,
    outcome: StepVerificationOutcome,
    evidence: Vec<VerificationEvidenceInspection>,
}

impl VerificationAttemptInspection {
    /// Returns the one-based step-local attempt number.
    #[must_use]
    pub const fn number(&self) -> TaskStepAttemptNumber {
        self.number
    }

    /// Returns the immutable ledger verification outcome.
    #[must_use]
    pub const fn outcome(&self) -> &StepVerificationOutcome {
        &self.outcome
    }

    /// Returns exact artifacts re-evaluated against the latest index.
    #[must_use]
    pub fn evidence(&self) -> &[VerificationEvidenceInspection] {
        &self.evidence
    }
}

/// Active plan step plus every retained typed verification attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationStepInspection {
    definition: TaskStepDefinition,
    status: TaskStepStatus,
    stale_cause: Option<TaskStepStaleCause>,
    attempts: Vec<VerificationAttemptInspection>,
}

impl VerificationStepInspection {
    /// Returns the immutable step definition and criterion mapping.
    #[must_use]
    pub const fn definition(&self) -> &TaskStepDefinition {
        &self.definition
    }

    /// Returns the current materialized step status.
    #[must_use]
    pub const fn status(&self) -> TaskStepStatus {
        self.status
    }

    /// Returns why previously completed evidence became stale.
    #[must_use]
    pub const fn stale_cause(&self) -> Option<&TaskStepStaleCause> {
        self.stale_cause.as_ref()
    }

    /// Returns verification attempts in one-based chronological order.
    #[must_use]
    pub fn attempts(&self) -> &[VerificationAttemptInspection] {
        &self.attempts
    }
}

/// Current proof state for one Goal Contract criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceCriterionProofState {
    /// Every mapped active step has successful fresh typed evidence.
    Proven,
    /// Mapped work has not reached verification yet.
    Pending,
    /// At least one mapped verification failed semantically.
    Failed,
    /// At least one mapped step or evidence artifact is stale.
    Stale,
    /// No active plan step maps to the criterion.
    Missing,
}

/// Exact fresh artifacts proving one mapped step for a criterion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriterionProof {
    step_id: TaskStepId,
    evidence_ids: Vec<TaskEvidenceId>,
}

impl AcceptanceCriterionProof {
    /// Returns the active plan step contributing this proof.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step_id
    }

    /// Returns exact durable artifacts used by the successful verification.
    #[must_use]
    pub fn evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.evidence_ids
    }
}

/// Goal criterion with its Core-derived current proof state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriterionInspection {
    criterion: AcceptanceCriterion,
    state: AcceptanceCriterionProofState,
    proofs: Vec<AcceptanceCriterionProof>,
}

impl AcceptanceCriterionInspection {
    /// Returns the immutable statement and Must/Should requirement.
    #[must_use]
    pub const fn criterion(&self) -> &AcceptanceCriterion {
        &self.criterion
    }

    /// Returns the freshly derived criterion proof state.
    #[must_use]
    pub const fn state(&self) -> AcceptanceCriterionProofState {
        self.state
    }

    /// Returns exact fresh proofs only when the criterion is Proven.
    #[must_use]
    pub fn proofs(&self) -> &[AcceptanceCriterionProof] {
        &self.proofs
    }
}

/// Consistent task anchors plus current/stale verification and Must/Should proof projections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskVerificationInspection {
    goal_contract: a3_domain::GoalContract,
    task_ledger: StoredTaskLedger,
    published_snapshot_id: SnapshotId,
    criteria: Vec<AcceptanceCriterionInspection>,
    steps: Vec<VerificationStepInspection>,
}

impl TaskVerificationInspection {
    /// Returns the exact current Goal Contract.
    #[must_use]
    pub const fn goal_contract(&self) -> &a3_domain::GoalContract {
        &self.goal_contract
    }

    /// Returns the exact current ledger and optimistic store version.
    #[must_use]
    pub const fn task_ledger(&self) -> &StoredTaskLedger {
        &self.task_ledger
    }

    /// Returns the latest atomically published snapshot used for freshness.
    #[must_use]
    pub const fn published_snapshot_id(&self) -> SnapshotId {
        self.published_snapshot_id
    }

    /// Returns Goal Contract criteria in their durable user-defined order.
    #[must_use]
    pub fn criteria(&self) -> &[AcceptanceCriterionInspection] {
        &self.criteria
    }

    /// Returns active plan steps in durable ledger order.
    #[must_use]
    pub fn steps(&self) -> &[VerificationStepInspection] {
        &self.steps
    }
}

/// Expected task availability states for the read-only U6 inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskVerificationInspectionLoadResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no durable Task Ledger.
    LedgerUnavailable,
    /// The ledger still targets an older Goal Contract revision.
    GoalRevisionMismatch,
    /// Goal or Ledger changed during the bounded multi-store read.
    InspectionChanged,
    /// A consistent task-bound inspection is available.
    Available(Box<TaskVerificationInspection>),
}

/// Read-only use case deriving UI truth from current durable task and E6 artifacts.
#[derive(Debug, Clone)]
pub struct GetTaskVerificationInspection {
    task: GetTaskLensTask,
    evidence: Arc<dyn VerificationEvidenceStore>,
}

impl GetTaskVerificationInspection {
    /// Creates the use case from the existing task workspace and E6 evidence ports.
    #[must_use]
    pub const fn new(
        workspace: Arc<dyn TaskLensWorkspaceStore>,
        evidence: Arc<dyn VerificationEvidenceStore>,
    ) -> Self {
        Self {
            task: GetTaskLensTask::new(workspace),
            evidence,
        }
    }

    /// Loads exact referenced artifacts, retains stale evidence, and rechecks task anchors.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        control: &dyn VerificationInspectionControl,
    ) -> Result<TaskVerificationInspectionLoadResult, GetTaskVerificationInspectionFailure> {
        let initial = match self
            .task
            .execute(project, task_id, control)
            .await
            .map_err(GetTaskVerificationInspectionFailure::Workspace)?
        {
            TaskLensTaskLoadResult::NotFound => {
                return Ok(TaskVerificationInspectionLoadResult::TaskNotFound);
            }
            TaskLensTaskLoadResult::LedgerUnavailable { .. } => {
                return Ok(TaskVerificationInspectionLoadResult::LedgerUnavailable);
            }
            TaskLensTaskLoadResult::GoalRevisionMismatch { .. } => {
                return Ok(TaskVerificationInspectionLoadResult::GoalRevisionMismatch);
            }
            TaskLensTaskLoadResult::Available(anchor) => anchor,
        };
        let evidence_ids = referenced_evidence_ids(&initial)?;
        let initial_state = self
            .evidence
            .load_verification_inspection_state(
                project,
                task_id,
                &evidence_ids,
                INSPECTION_TIMEOUT,
                control,
            )
            .await
            .map_err(GetTaskVerificationInspectionFailure::Evidence)?;
        if initial_state
            .evidence()
            .iter()
            .map(VerificationEvidence::id)
            .collect::<Vec<_>>()
            != evidence_ids
        {
            return Err(GetTaskVerificationInspectionFailure::InvalidStoredData);
        }
        let current = match self
            .task
            .execute(project, task_id, control)
            .await
            .map_err(GetTaskVerificationInspectionFailure::Workspace)?
        {
            TaskLensTaskLoadResult::Available(anchor) if anchor == initial => anchor,
            TaskLensTaskLoadResult::NotFound
            | TaskLensTaskLoadResult::LedgerUnavailable { .. }
            | TaskLensTaskLoadResult::GoalRevisionMismatch { .. }
            | TaskLensTaskLoadResult::Available(_) => {
                return Ok(TaskVerificationInspectionLoadResult::InspectionChanged);
            }
        };
        let current_state = self
            .evidence
            .load_verification_inspection_state(
                project,
                task_id,
                &evidence_ids,
                INSPECTION_TIMEOUT,
                control,
            )
            .await
            .map_err(GetTaskVerificationInspectionFailure::Evidence)?;
        if current_state != initial_state {
            return Ok(TaskVerificationInspectionLoadResult::InspectionChanged);
        }
        build_task_inspection(current, current_state)
            .map(|inspection| TaskVerificationInspectionLoadResult::Available(Box::new(inspection)))
            .map_err(|_| GetTaskVerificationInspectionFailure::InvalidStoredData)
    }
}

/// Stable failure while reading or deriving a U6 verification projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetTaskVerificationInspectionFailure {
    /// Current task workspace storage failed.
    Workspace(TaskLensWorkspaceFailure),
    /// Verification evidence or current publication storage failed.
    Evidence(VerificationEvidenceStoreFailure),
    /// Durable cross-store content violated U6 invariants.
    InvalidStoredData,
}

impl fmt::Display for GetTaskVerificationInspectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Workspace(_) => "verification inspection task workspace failed",
            Self::Evidence(_) => "verification inspection evidence storage failed",
            Self::InvalidStoredData => "verification inspection data is invalid",
        })
    }
}

impl Error for GetTaskVerificationInspectionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            Self::Evidence(error) => Some(error),
            Self::InvalidStoredData => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerificationInspectionBuildError {
    EvidenceMismatch,
    DuplicateSpecification,
    InvalidCount,
}

fn referenced_evidence_ids(
    anchor: &TaskLensTaskAnchor,
) -> Result<Vec<TaskEvidenceId>, GetTaskVerificationInspectionFailure> {
    let mut ids = BTreeSet::new();
    for step in anchor
        .task_ledger()
        .ledger()
        .steps()
        .filter(|step| step.is_active_plan_step())
    {
        for verification in step
            .attempts()
            .iter()
            .filter_map(|attempt| attempt.verification())
        {
            ids.extend(verification.evidence_ids().iter().copied());
        }
        if let Some(TaskStepStaleCause::VerificationEvidence(stale)) = step.stale_cause() {
            ids.extend(stale.iter().copied());
        }
    }
    if ids.len() > MAX_INSPECTION_EVIDENCE {
        return Err(GetTaskVerificationInspectionFailure::InvalidStoredData);
    }
    Ok(ids.into_iter().collect())
}

fn build_task_inspection(
    anchor: TaskLensTaskAnchor,
    state: StoredVerificationState,
) -> Result<TaskVerificationInspection, VerificationInspectionBuildError> {
    let published_snapshot_id = state.published_index().publication().graph().snapshot_id();
    let evidence_by_id = state
        .evidence()
        .iter()
        .map(|evidence| (evidence.id(), evidence))
        .collect::<BTreeMap<_, _>>();
    let mut spec_ids = BTreeSet::new();
    let mut steps = Vec::new();
    for step in anchor
        .task_ledger()
        .ledger()
        .steps()
        .filter(|step| step.is_active_plan_step())
    {
        if !spec_ids.insert(step.definition().verification_spec().id()) {
            return Err(VerificationInspectionBuildError::DuplicateSpecification);
        }
        let mut attempts = Vec::new();
        for attempt in step.attempts() {
            let Some(verification) = attempt.verification() else {
                continue;
            };
            let evidence = verification
                .evidence_ids()
                .iter()
                .map(|id| {
                    let artifact = evidence_by_id
                        .get(id)
                        .copied()
                        .ok_or(VerificationInspectionBuildError::EvidenceMismatch)?;
                    VerificationEvidenceInspection::build(
                        step.definition().verification_spec(),
                        artifact,
                        state.published_index(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            attempts.push(VerificationAttemptInspection {
                number: attempt.number(),
                outcome: verification.outcome().clone(),
                evidence,
            });
        }
        steps.push(VerificationStepInspection {
            definition: step.definition().clone(),
            status: step.status(),
            stale_cause: step.stale_cause().cloned(),
            attempts,
        });
    }
    let criteria = anchor
        .goal_contract()
        .draft()
        .acceptance_criteria()
        .iter()
        .map(|criterion| inspect_criterion(criterion, &steps))
        .collect::<Vec<_>>();
    Ok(TaskVerificationInspection {
        goal_contract: anchor.goal_contract().clone(),
        task_ledger: anchor.task_ledger().clone(),
        published_snapshot_id,
        criteria,
        steps,
    })
}

fn inspect_criterion(
    criterion: &AcceptanceCriterion,
    steps: &[VerificationStepInspection],
) -> AcceptanceCriterionInspection {
    let mapped = steps
        .iter()
        .filter(|step| {
            step.definition()
                .acceptance_criteria()
                .contains(&criterion.id())
        })
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        return AcceptanceCriterionInspection {
            criterion: criterion.clone(),
            state: AcceptanceCriterionProofState::Missing,
            proofs: Vec::new(),
        };
    }
    let mut state = AcceptanceCriterionProofState::Proven;
    let mut proofs = Vec::new();
    for step in mapped {
        let step_state = inspect_step_proof(step);
        state = combine_proof_state(state, step_state.0);
        if step_state.0 == AcceptanceCriterionProofState::Proven {
            proofs.push(AcceptanceCriterionProof {
                step_id: step.definition().id(),
                evidence_ids: step_state.1,
            });
        }
    }
    if state != AcceptanceCriterionProofState::Proven {
        proofs.clear();
    }
    AcceptanceCriterionInspection {
        criterion: criterion.clone(),
        state,
        proofs,
    }
}

fn inspect_step_proof(
    step: &VerificationStepInspection,
) -> (AcceptanceCriterionProofState, Vec<TaskEvidenceId>) {
    if step.status() == TaskStepStatus::Stale || step.stale_cause().is_some() {
        return (AcceptanceCriterionProofState::Stale, Vec::new());
    }
    let Some(attempt) = step.attempts().last() else {
        return (AcceptanceCriterionProofState::Pending, Vec::new());
    };
    if attempt
        .evidence()
        .iter()
        .any(|evidence| matches!(evidence.freshness(), EvidenceFreshness::Stale(_)))
    {
        return (AcceptanceCriterionProofState::Stale, Vec::new());
    }
    if !matches!(attempt.outcome(), StepVerificationOutcome::Passed)
        || attempt.evidence().iter().any(|evidence| {
            matches!(
                evidence.semantic(),
                VerificationEvidenceEvaluation::Failed(_)
            )
        })
    {
        return (AcceptanceCriterionProofState::Failed, Vec::new());
    }
    if step.status() != TaskStepStatus::Completed || attempt.evidence().is_empty() {
        return (AcceptanceCriterionProofState::Pending, Vec::new());
    }
    (
        AcceptanceCriterionProofState::Proven,
        attempt
            .evidence()
            .iter()
            .map(VerificationEvidenceInspection::id)
            .collect(),
    )
}

const fn combine_proof_state(
    left: AcceptanceCriterionProofState,
    right: AcceptanceCriterionProofState,
) -> AcceptanceCriterionProofState {
    use AcceptanceCriterionProofState::{Failed, Missing, Pending, Proven, Stale};
    match (left, right) {
        (Stale, _) | (_, Stale) => Stale,
        (Failed, _) | (_, Failed) => Failed,
        (Missing, _) | (_, Missing) => Missing,
        (Pending, _) | (_, Pending) => Pending,
        (Proven, Proven) => Proven,
    }
}

fn user_confirmation_detail(value: &UserConfirmationEvidence) -> VerificationEvidenceDetail {
    VerificationEvidenceDetail::UserConfirmation {
        scope_id: value.scope_id(),
        confirmed_at: value.confirmed_at(),
    }
}

fn checked_increment(value: u64) -> Result<u64, VerificationInspectionBuildError> {
    value
        .checked_add(1)
        .ok_or(VerificationInspectionBuildError::InvalidCount)
}

#[cfg(test)]
mod tests {
    use super::{
        AcceptanceCriterionProofState, VerificationAttemptInspection, VerificationEvidenceDetail,
        VerificationEvidenceInspection, VerificationStepInspection, inspect_criterion,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
        EvidenceFreshness, EvidenceFreshnessFailure, ExpectedTaskEvidence, PolicyResourceId,
        SnapshotId, StepVerificationOutcome, TaskEvidenceId, TaskLedgerTimestamp,
        TaskStepAttemptNumber, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale,
        TaskStepStatus, VerificationEvidenceEvaluation, VerificationEvidenceFailure,
        VerificationMethod, VerificationRequirement, VerificationSpec, VerificationSpecId,
    };
    use std::error::Error;

    #[test]
    fn completed_step_exposes_exact_fresh_proof_for_mapped_criterion() -> Result<(), Box<dyn Error>>
    {
        let (criterion, step, evidence_id) = fixture(
            EvidenceFreshness::Fresh,
            VerificationEvidenceEvaluation::Passed,
        )?;

        let inspection = inspect_criterion(&criterion, &[step]);

        assert_eq!(inspection.state(), AcceptanceCriterionProofState::Proven);
        assert_eq!(inspection.proofs().len(), 1);
        assert_eq!(inspection.proofs()[0].evidence_ids(), &[evidence_id]);
        Ok(())
    }

    #[test]
    fn newly_stale_evidence_reopens_a_completed_criterion_projection() -> Result<(), Box<dyn Error>>
    {
        let (criterion, stale_step, _) = fixture(
            EvidenceFreshness::Stale(EvidenceFreshnessFailure::SnapshotChanged),
            VerificationEvidenceEvaluation::Passed,
        )?;

        let inspection = inspect_criterion(&criterion, &[stale_step]);

        assert_eq!(inspection.state(), AcceptanceCriterionProofState::Stale);
        assert!(inspection.proofs().is_empty());
        Ok(())
    }

    #[test]
    fn semantic_failure_cannot_be_presented_as_a_proof() -> Result<(), Box<dyn Error>> {
        let (criterion, failed_step, _) = fixture(
            EvidenceFreshness::Fresh,
            VerificationEvidenceEvaluation::Failed(
                VerificationEvidenceFailure::ConfirmationScopeMismatch,
            ),
        )?;

        let inspection = inspect_criterion(&criterion, &[failed_step]);

        assert_eq!(inspection.state(), AcceptanceCriterionProofState::Failed);
        assert!(inspection.proofs().is_empty());
        Ok(())
    }

    fn fixture(
        freshness: EvidenceFreshness,
        semantic: VerificationEvidenceEvaluation,
    ) -> Result<
        (
            AcceptanceCriterion,
            VerificationStepInspection,
            TaskEvidenceId,
        ),
        Box<dyn Error>,
    > {
        let criterion_id = AcceptanceCriterionId::from_bytes([1; 32]);
        let criterion = AcceptanceCriterion::new(
            criterion_id,
            AcceptanceCriterionStatement::try_from_string(
                "the exact approved scope is verified".to_owned(),
            )?,
        );
        let scope_id = PolicyResourceId::from_bytes([2; 32]);
        let definition = TaskStepDefinition::new(
            TaskStepId::from_bytes([3; 32]),
            None,
            TaskStepOutcome::try_from_string("verify the approved scope".to_owned())?,
            TaskStepRationale::try_from_string(
                "the mandatory criterion requires evidence".to_owned(),
            )?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "one exact confirmation".to_owned(),
            )?],
            VerificationSpec::user_confirm(
                VerificationSpecId::from_bytes([4; 32]),
                VerificationRequirement::try_from_string(
                    "confirm the exact policy scope".to_owned(),
                )?,
                scope_id,
            ),
        )?
        .with_acceptance_criteria(vec![criterion_id])?;
        let evidence_id = TaskEvidenceId::from_bytes([5; 32]);
        let evidence = VerificationEvidenceInspection {
            id: evidence_id,
            run_id: AgentRunId::from_bytes([6; 32]),
            snapshot_id: SnapshotId::from_bytes([7; 32]),
            method: VerificationMethod::UserConfirm,
            semantic,
            freshness,
            detail: VerificationEvidenceDetail::UserConfirmation {
                scope_id,
                confirmed_at: TaskLedgerTimestamp::from_unix_millis(1)?,
            },
        };
        let step = VerificationStepInspection {
            definition,
            status: TaskStepStatus::Completed,
            stale_cause: None,
            attempts: vec![VerificationAttemptInspection {
                number: TaskStepAttemptNumber::FIRST,
                outcome: StepVerificationOutcome::Passed,
                evidence: vec![evidence],
            }],
        };
        Ok((criterion, step, evidence_id))
    }
}
