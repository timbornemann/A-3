use crate::{
    AcceptanceRejection, AcceptanceVerificationRequest, AcceptanceVerifier,
    AcceptanceVerifierFailure, AcceptanceVerifierFuture, AcceptanceVerifierOutcome,
    AcceptanceVerifierTimeout, AgentControllerControl,
};
use a3_domain::{
    AcceptanceCriterionRequirement, AcceptanceCriterionVerification, AcceptanceVerificationReceipt,
    AgentRunId, EvidenceFreshness, GoalOriginalCoverage, ProjectIdentity, PublishedIndex,
    StepVerification, StepVerificationError, StepVerificationId, StepVerificationOutcome,
    TaskEvidenceId, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskVerificationTextError,
    TestCaseSelector, VerificationEvidence, VerificationEvidenceEvaluation,
    VerificationEvidenceFailure, VerificationFailureSummary, VerificationMethod, VerificationScope,
    VerificationSpec, VerificationSpecId, VerificationTarget,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_ACCEPTANCE_EVIDENCE: usize = 256 * 64;

/// Pure deterministic ordering of relevant verification specs from narrowest to broadest.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrderVerificationSpecs;

impl OrderVerificationSpecs {
    /// Rejects legacy or duplicate specs, then applies stable breadth, semantics, and ID ordering.
    pub fn execute<'a>(
        self,
        specs: impl IntoIterator<Item = &'a VerificationSpec>,
    ) -> Result<Vec<&'a VerificationSpec>, VerificationOrderingError> {
        let mut ordered = specs.into_iter().collect::<Vec<_>>();
        if ordered.iter().any(|spec| !spec.is_operational()) {
            return Err(VerificationOrderingError::LegacySpecification);
        }
        ordered.sort_by_key(|spec| verification_order_key(spec));
        if ordered.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(VerificationOrderingError::DuplicateSpecification);
        }
        Ok(ordered)
    }
}

/// Relevant verification specs could not form one executable deterministic order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationOrderingError {
    /// Historical method-plus-text specs require migration before execution.
    LegacySpecification,
    /// The same immutable specification appeared more than once.
    DuplicateSpecification,
}

impl fmt::Display for VerificationOrderingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LegacySpecification => "legacy verification specification is not executable",
            Self::DuplicateSpecification => "verification order repeats a specification",
        })
    }
}

impl Error for VerificationOrderingError {}

/// Creates a Task Ledger verification result only from typed semantic evidence and freshness.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvaluateStepVerification;

impl EvaluateStepVerification {
    /// Derives Passed or a stable failure from one exact artifact; callers cannot supply success.
    pub fn execute(
        self,
        verification_id: StepVerificationId,
        spec: &VerificationSpec,
        run_id: AgentRunId,
        evidence: &VerificationEvidence,
        current: &PublishedIndex,
        verified_at: TaskLedgerTimestamp,
    ) -> Result<StepVerification, EvaluateStepVerificationError> {
        if spec.method() == VerificationMethod::LoopbackHttp {
            return Err(EvaluateStepVerificationError::EvidenceMismatch);
        }
        Self::evaluate(
            verification_id,
            spec,
            run_id,
            evidence,
            current,
            verified_at,
        )
    }

    /// Derives the verdict with the owning step explicitly bound to composite HTTP evidence.
    pub fn execute_for_step(
        self,
        verification_id: StepVerificationId,
        step: &TaskStepDefinition,
        run_id: AgentRunId,
        evidence: &VerificationEvidence,
        current: &PublishedIndex,
        verified_at: TaskLedgerTimestamp,
    ) -> Result<StepVerification, EvaluateStepVerificationError> {
        if !evidence_matches_step(evidence, step.id()) {
            return Err(EvaluateStepVerificationError::EvidenceMismatch);
        }
        Self::evaluate(
            verification_id,
            step.verification_spec(),
            run_id,
            evidence,
            current,
            verified_at,
        )
    }

    fn evaluate(
        verification_id: StepVerificationId,
        spec: &VerificationSpec,
        run_id: AgentRunId,
        evidence: &VerificationEvidence,
        current: &PublishedIndex,
        verified_at: TaskLedgerTimestamp,
    ) -> Result<StepVerification, EvaluateStepVerificationError> {
        if !spec.is_operational() {
            return Err(EvaluateStepVerificationError::LegacySpecification);
        }
        if evidence.spec_id() != spec.id() || evidence.method() != spec.method() {
            return Err(EvaluateStepVerificationError::EvidenceMismatch);
        }
        if evidence.run_id() != run_id {
            return Err(EvaluateStepVerificationError::RunMismatch);
        }
        let semantic = VerificationEvidenceEvaluation::evaluate(spec, evidence);
        let freshness = EvidenceFreshness::evaluate(evidence, current);
        let outcome = match (semantic, freshness) {
            (VerificationEvidenceEvaluation::Passed, EvidenceFreshness::Fresh) => {
                StepVerificationOutcome::Passed
            }
            (VerificationEvidenceEvaluation::Failed(failure), _) => {
                failed_outcome(evidence_failure_message(failure))?
            }
            (_, EvidenceFreshness::Stale(_)) => failed_outcome("verification evidence is stale")?,
        };
        StepVerification::new(
            verification_id,
            spec.id(),
            run_id,
            outcome,
            vec![evidence.id()],
            verified_at,
        )
        .map_err(EvaluateStepVerificationError::InvalidVerification)
    }
}

pub(crate) fn evidence_matches_step(evidence: &VerificationEvidence, step_id: TaskStepId) -> bool {
    !matches!(evidence, VerificationEvidence::LoopbackHttp(http) if http.context().step_id != step_id)
}

/// Typed verification evidence could not produce a valid Step Ledger result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluateStepVerificationError {
    /// Historical method-plus-text specs require migration before execution.
    LegacySpecification,
    /// Evidence targeted another spec or semantic method.
    EvidenceMismatch,
    /// Evidence belongs to another controlled agent run.
    RunMismatch,
    /// A fixed content-free failure summary violated its domain boundary.
    InvalidFailureSummary(TaskVerificationTextError),
    /// The final Task Ledger verification shape was invalid.
    InvalidVerification(StepVerificationError),
}

impl fmt::Display for EvaluateStepVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LegacySpecification => "legacy verification specification is not executable",
            Self::EvidenceMismatch => "verification evidence does not match the specification",
            Self::RunMismatch => "verification evidence belongs to another agent run",
            Self::InvalidFailureSummary(_) => "verification failure summary is invalid",
            Self::InvalidVerification(_) => "verification result violates Task Ledger invariants",
        })
    }
}

impl Error for EvaluateStepVerificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidFailureSummary(error) => Some(error),
            Self::InvalidVerification(error) => Some(error),
            Self::LegacySpecification | Self::EvidenceMismatch | Self::RunMismatch => None,
        }
    }
}

fn failed_outcome(
    message: &'static str,
) -> Result<StepVerificationOutcome, EvaluateStepVerificationError> {
    let summary = VerificationFailureSummary::try_from_string(message.to_owned())
        .map_err(EvaluateStepVerificationError::InvalidFailureSummary)?;
    Ok(StepVerificationOutcome::Failed { summary })
}

fn evidence_failure_message(failure: VerificationEvidenceFailure) -> &'static str {
    match failure {
        VerificationEvidenceFailure::LegacySpecification => {
            "legacy verification specification is not executable"
        }
        VerificationEvidenceFailure::SpecificationMismatch => {
            "verification evidence targets another specification"
        }
        VerificationEvidenceFailure::EvidenceKindMismatch => {
            "verification evidence has the wrong semantic kind"
        }
        VerificationEvidenceFailure::CommandMismatch => {
            "verification evidence targets another command"
        }
        VerificationEvidenceFailure::ProcessUnsuccessful => {
            "verification process did not complete successfully"
        }
        VerificationEvidenceFailure::MissingStructuredTestCases => {
            "structured test-case evidence is missing"
        }
        VerificationEvidenceFailure::TooFewPassingTestCases => "too few selected test cases passed",
        VerificationEvidenceFailure::SelectedTestCaseFailed => {
            "a selected structured test case failed"
        }
        VerificationEvidenceFailure::SourceProofMismatch => {
            "the required current source proof is missing or does not match"
        }
        VerificationEvidenceFailure::LoopbackHttpMismatch => {
            "the observed loopback recipe does not meet the required HTTP behavior"
        }
        VerificationEvidenceFailure::IncompleteChangeSet => "the patch change set is incomplete",
        VerificationEvidenceFailure::DiffInvariantMismatch => {
            "actual changed paths violate the diff invariant"
        }
        VerificationEvidenceFailure::ErrorDiagnosticsPresent => {
            "error diagnostics violate the verification policy"
        }
        VerificationEvidenceFailure::WarningDiagnosticsPresent => {
            "warning diagnostics violate the verification policy"
        }
        VerificationEvidenceFailure::ConfirmationScopeMismatch => {
            "user confirmation targets another scope"
        }
    }
}

fn verification_order_key(spec: &VerificationSpec) -> (u8, u8, usize, VerificationSpecId) {
    match spec.target() {
        VerificationTarget::DiffInvariant(invariant) => (0, 0, invariant.paths().len(), spec.id()),
        VerificationTarget::Test {
            selector, scope, ..
        } => (
            scope_rank(*scope),
            1,
            usize::from(matches!(selector, TestCaseSelector::All)),
            spec.id(),
        ),
        VerificationTarget::Diagnostic { scope, .. } => (scope_rank(*scope), 2, 0, spec.id()),
        VerificationTarget::Command { scope, .. } => (scope_rank(*scope), 3, 0, spec.id()),
        VerificationTarget::LoopbackHttp(_) => (0, 4, 0, spec.id()),
        VerificationTarget::DeferredCommand(deferred) => (
            scope_rank(deferred.scope()),
            4,
            deferred.preferred_kinds().len(),
            spec.id(),
        ),
        VerificationTarget::UserConfirm { .. } => (4, 4, 0, spec.id()),
        VerificationTarget::Legacy(method) => (5, legacy_method_rank(*method), 0, spec.id()),
    }
}

const fn scope_rank(scope: VerificationScope) -> u8 {
    match scope {
        VerificationScope::Targeted => 0,
        VerificationScope::Package => 1,
        VerificationScope::Workspace => 2,
    }
}

const fn legacy_method_rank(method: VerificationMethod) -> u8 {
    match method {
        VerificationMethod::DiffInvariant => 0,
        VerificationMethod::Test => 1,
        VerificationMethod::Diagnostic => 2,
        VerificationMethod::Command => 3,
        VerificationMethod::UserConfirm => 4,
        VerificationMethod::DeferredCommand => 5,
        VerificationMethod::LoopbackHttp => 6,
    }
}

/// Exact current index and verification artifacts loaded together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVerificationState {
    published_index: PublishedIndex,
    evidence: Vec<VerificationEvidence>,
}

impl StoredVerificationState {
    /// Canonicalizes a bounded unique evidence set returned by local persistence.
    pub fn new(
        published_index: PublishedIndex,
        mut evidence: Vec<VerificationEvidence>,
    ) -> Result<Self, StoredVerificationStateError> {
        if evidence.len() > MAX_ACCEPTANCE_EVIDENCE {
            return Err(StoredVerificationStateError::TooMuchEvidence {
                actual: evidence.len(),
            });
        }
        evidence.sort_by_key(VerificationEvidence::id);
        if evidence.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(StoredVerificationStateError::DuplicateEvidence);
        }
        Ok(Self {
            published_index,
            evidence,
        })
    }

    /// Returns the latest atomically published repository index.
    #[must_use]
    pub const fn published_index(&self) -> &PublishedIndex {
        &self.published_index
    }

    /// Returns evidence in stable ID order.
    #[must_use]
    pub fn evidence(&self) -> &[VerificationEvidence] {
        &self.evidence
    }
}

/// Local stored verification state violated its bounded set contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredVerificationStateError {
    /// More than the maximum active-step evidence capacity was returned.
    TooMuchEvidence {
        /// Observed artifact count.
        actual: usize,
    },
    /// Two artifacts used the same durable TaskEvidenceId.
    DuplicateEvidence,
}

impl fmt::Display for StoredVerificationStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooMuchEvidence { .. } => "stored verification state exceeds its evidence limit",
            Self::DuplicateEvidence => "stored verification state repeats an evidence identity",
        })
    }
}

impl Error for StoredVerificationStateError {}

/// Owned future returned by the verification-evidence persistence port.
pub type VerificationEvidenceStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, VerificationEvidenceStoreFailure>> + Send + 'a>>;

/// Durable worktree-local evidence and acceptance-state boundary.
pub trait VerificationEvidenceStore: fmt::Debug + Send + Sync {
    /// Appends one immutable typed evidence artifact; identical IDs are idempotent only if equal.
    fn append_verification_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        evidence: &'a VerificationEvidence,
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, ()>;

    /// Loads exactly the requested artifacts plus the current published index.
    fn load_verification_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: a3_domain::TaskId,
        evidence_ids: &'a [TaskEvidenceId],
        expected_snapshot_id: a3_domain::SnapshotId,
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState>;

    /// Loads requested artifacts with the latest published index even when evidence is stale.
    fn load_verification_inspection_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: a3_domain::TaskId,
        evidence_ids: &'a [TaskEvidenceId],
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState>;
}

/// Stable classification of verification-evidence persistence failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationEvidenceStoreFailure {
    /// Worktree-local storage was unavailable.
    Unavailable,
    /// Durable verification rows failed integrity checks.
    Corrupt,
    /// Storage uses a schema newer than this application build.
    UnsupportedSchema,
    /// Durable rows violated domain or exact-coverage invariants.
    InvalidStoredData,
    /// An existing evidence ID had different immutable content.
    EvidenceConflict,
    /// The currently published index no longer matches the requested snapshot.
    SnapshotMismatch,
    /// Cooperative cancellation stopped the bounded read or append.
    Cancelled,
    /// The bounded local operation exceeded its deadline.
    TimedOut,
}

impl fmt::Display for VerificationEvidenceStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "verification evidence storage is unavailable",
            Self::Corrupt => "verification evidence storage is corrupt",
            Self::UnsupportedSchema => "verification evidence schema is unsupported",
            Self::InvalidStoredData => "stored verification evidence is invalid",
            Self::EvidenceConflict => "verification evidence identity conflicts",
            Self::SnapshotMismatch => "verification evidence snapshot is no longer current",
            Self::Cancelled => "verification evidence operation was cancelled",
            Self::TimedOut => "verification evidence operation timed out",
        })
    }
}

impl Error for VerificationEvidenceStoreFailure {}

/// Productive, deterministic implementation of the controller's sole Done-verifier port.
#[derive(Debug, Clone, Copy)]
pub struct DeterministicAcceptanceVerifier<'a> {
    store: &'a dyn VerificationEvidenceStore,
}

impl<'a> DeterministicAcceptanceVerifier<'a> {
    /// Creates the verifier from its narrow durable evidence capability.
    #[must_use]
    pub const fn new(store: &'a dyn VerificationEvidenceStore) -> Self {
        Self { store }
    }
}

impl AcceptanceVerifier for DeterministicAcceptanceVerifier<'_> {
    fn verify<'a>(
        &'a self,
        request: &'a AcceptanceVerificationRequest,
        timeout: AcceptanceVerifierTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AcceptanceVerifierFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(AcceptanceVerifierFailure::Cancelled);
            }
            if let Some(rejection) = original_acceptance_admission(request)? {
                return Ok(AcceptanceVerifierOutcome::Rejected(rejection));
            }
            let required = required_acceptance_evidence(request)?;
            let requested_ids = required
                .iter()
                .flat_map(|(_, steps)| steps.iter().map(|(_, _, _, evidence_id)| *evidence_id))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let has_must_criterion = request
                .goal_contract()
                .draft()
                .acceptance_criteria()
                .iter()
                .any(|criterion| criterion.requirement() == AcceptanceCriterionRequirement::Must);
            if requested_ids.is_empty() && has_must_criterion {
                return Ok(AcceptanceVerifierOutcome::Rejected(
                    AcceptanceRejection::InsufficientEvidence,
                ));
            }
            let state = match self
                .store
                .load_verification_state(
                    request.project(),
                    request.goal_contract().task_id(),
                    &requested_ids,
                    request.snapshot_id(),
                    timeout.duration(),
                    control,
                )
                .await
            {
                Ok(state) => state,
                Err(VerificationEvidenceStoreFailure::SnapshotMismatch) => {
                    return Ok(AcceptanceVerifierOutcome::Rejected(
                        AcceptanceRejection::StaleEvidence,
                    ));
                }
                Err(failure) => return Err(map_store_failure(failure)),
            };
            if control.is_cancelled() {
                return Err(AcceptanceVerifierFailure::Cancelled);
            }
            if request.run_memory().open_hypotheses().next().is_some() {
                return Ok(AcceptanceVerifierOutcome::Rejected(
                    AcceptanceRejection::BlockingHypothesis,
                ));
            }
            if state.published_index().publication().graph().snapshot_id() != request.snapshot_id()
            {
                return Ok(AcceptanceVerifierOutcome::Rejected(
                    AcceptanceRejection::StaleEvidence,
                ));
            }
            let stored_ids = state
                .evidence()
                .iter()
                .map(VerificationEvidence::id)
                .collect::<Vec<_>>();
            if stored_ids != requested_ids {
                return Err(AcceptanceVerifierFailure::InvalidResult);
            }
            let evidence_by_id = state
                .evidence()
                .iter()
                .map(|evidence| (evidence.id(), evidence))
                .collect::<BTreeMap<_, _>>();
            let mut criteria = Vec::with_capacity(required.len());
            for (criterion_id, steps) in required {
                let mut criterion_evidence = Vec::with_capacity(steps.len());
                for (spec, expected_step_id, expected_run_id, evidence_id) in steps {
                    let evidence = evidence_by_id
                        .get(&evidence_id)
                        .copied()
                        .ok_or(AcceptanceVerifierFailure::InvalidResult)?;
                    if evidence.spec_id() != spec.id()
                        || evidence.run_id() != expected_run_id
                        || !evidence_matches_step(evidence, expected_step_id)
                    {
                        return Err(AcceptanceVerifierFailure::InvalidResult);
                    }
                    if request.goal_contract().original().is_some()
                        && evidence.snapshot_id() != request.snapshot_id()
                    {
                        return Ok(AcceptanceVerifierOutcome::Rejected(
                            AcceptanceRejection::StaleEvidence,
                        ));
                    }
                    if VerificationEvidenceEvaluation::evaluate(spec, evidence)
                        != VerificationEvidenceEvaluation::Passed
                    {
                        return Ok(AcceptanceVerifierOutcome::Rejected(
                            AcceptanceRejection::CriterionFailed,
                        ));
                    }
                    if EvidenceFreshness::evaluate(evidence, state.published_index())
                        != EvidenceFreshness::Fresh
                    {
                        return Ok(AcceptanceVerifierOutcome::Rejected(
                            AcceptanceRejection::StaleEvidence,
                        ));
                    }
                    criterion_evidence.push(evidence_id);
                }
                criteria.push(
                    AcceptanceCriterionVerification::new(criterion_id, criterion_evidence)
                        .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?,
                );
            }
            let receipt = AcceptanceVerificationReceipt::new(
                request.run_id(),
                request.goal_contract(),
                request.task_ledger().revision(),
                request.snapshot_id(),
                criteria,
            )
            .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?;
            Ok(AcceptanceVerifierOutcome::Accepted(receipt))
        })
    }
}

/// Admission of the new original-bound contract at the existing sole verifier entry.
/// Clean legacy goals keep their previous path. This neither selects an executor
/// nor migrates a schema; normal production CURRENT V42 still has only legacy goals.
fn original_acceptance_admission(
    request: &AcceptanceVerificationRequest,
) -> Result<Option<AcceptanceRejection>, AcceptanceVerifierFailure> {
    let goal = request.goal_contract();
    goal.validate_original_binding()
        .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?;
    if goal.original().is_none() {
        return Ok(None);
    }
    let coverage = GoalOriginalCoverage::derive_from(goal)
        .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?;
    if !coverage.is_complete() {
        return Ok(Some(AcceptanceRejection::InsufficientEvidence));
    }
    for step in request
        .task_ledger()
        .steps()
        .filter(|step| step.is_active_plan_step())
    {
        match crate::AdmitSourceTaskStep.execute(goal, step.definition()) {
            Ok(()) => {}
            Err(
                crate::SourceTaskStepAdmissionFailure::ProofMismatch { .. }
                | crate::SourceTaskStepAdmissionFailure::NonOperationalVerification,
            ) => {
                return Ok(Some(AcceptanceRejection::CriterionFailed));
            }
            Err(crate::SourceTaskStepAdmissionFailure::MissingCriterionMapping) => {
                return Ok(Some(AcceptanceRejection::InsufficientEvidence));
            }
            Err(_) => return Err(AcceptanceVerifierFailure::InvalidResult),
        }
    }
    // Structural coverage and admitted specs do not prove execution or semantics.
    // The existing verifier must still check evidence, freshness and hypotheses.
    Ok(None)
}

type RequiredStepEvidence<'a> = (&'a VerificationSpec, TaskStepId, AgentRunId, TaskEvidenceId);
type RequiredAcceptanceEvidence<'a> = Vec<(
    a3_domain::AcceptanceCriterionId,
    Vec<RequiredStepEvidence<'a>>,
)>;

fn required_acceptance_evidence(
    request: &AcceptanceVerificationRequest,
) -> Result<RequiredAcceptanceEvidence<'_>, AcceptanceVerifierFailure> {
    let mut required = request
        .goal_contract()
        .draft()
        .acceptance_criteria()
        .iter()
        .filter(|criterion| criterion.requirement() == AcceptanceCriterionRequirement::Must)
        .map(|criterion| (criterion.id(), Vec::new()))
        .collect::<BTreeMap<_, Vec<RequiredStepEvidence<'_>>>>();
    for step in request
        .task_ledger()
        .steps()
        .filter(|step| step.is_active_plan_step())
    {
        for criterion_id in step.definition().acceptance_criteria() {
            let Some(steps) = required.get_mut(criterion_id) else {
                continue;
            };
            let verification = step
                .attempts()
                .last()
                .and_then(|attempt| attempt.verification())
                .filter(|verification| verification.passed())
                .ok_or(AcceptanceVerifierFailure::InvalidResult)?;
            if verification.spec_id() != step.definition().verification_spec().id()
                || verification.evidence_ids().len() != 1
            {
                return Err(AcceptanceVerifierFailure::InvalidResult);
            }
            steps.push((
                step.definition().verification_spec(),
                step.definition().id(),
                verification.run_id(),
                verification.evidence_ids()[0],
            ));
        }
    }
    if required.values().any(Vec::is_empty) {
        return Ok(Vec::new());
    }
    Ok(required.into_iter().collect())
}

const fn map_store_failure(failure: VerificationEvidenceStoreFailure) -> AcceptanceVerifierFailure {
    match failure {
        VerificationEvidenceStoreFailure::TimedOut => AcceptanceVerifierFailure::TimedOut,
        VerificationEvidenceStoreFailure::Cancelled => AcceptanceVerifierFailure::Cancelled,
        VerificationEvidenceStoreFailure::InvalidStoredData
        | VerificationEvidenceStoreFailure::EvidenceConflict => {
            AcceptanceVerifierFailure::InvalidResult
        }
        VerificationEvidenceStoreFailure::SnapshotMismatch => {
            AcceptanceVerifierFailure::InvalidResult
        }
        VerificationEvidenceStoreFailure::Unavailable
        | VerificationEvidenceStoreFailure::Corrupt
        | VerificationEvidenceStoreFailure::UnsupportedSchema => {
            AcceptanceVerifierFailure::Unavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRun,
        AgentRunIdentity, AgentRunMaterializedState, AgentRunTimestamp, AgentRunTiming,
        CanonicalDirectory, CommandEvidence, CommandEvidenceContext, DiffInvariantMode,
        DiffInvariantVerification, DiscoveredCommandId, ExpectedTaskEvidence, GitHead,
        GitReferenceName, GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective,
        IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus,
        LinkedGraph, ModulePolicyVersion, ModuleProjection, ModuleSymbolSet, PolicyDecisionId,
        PolicyResourceId, ProcessDuration, ProcessExit, ProcessOutputCapture, ProcessOutputContent,
        ProcessOutputDigest, ProcessRunResult, ProcessStream, ProcessTermination, ProjectIdentity,
        RankProjection, RankingPolicyVersion, RepositoryCard, RepositoryId, RepositoryIdentity,
        RunEventSequence, SnapshotId, StepVerification, StepVerificationId,
        StepVerificationOutcome, SuccessVerification, TaskId, TaskLedger, TaskStepDefinition,
        TaskStepId, TaskStepOutcome, TaskStepRationale, TestCaseSelectorName, TestEvidence,
        ToolRunId, VerificationDependencies, VerificationRequirement, VerificationRunId,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use a3_domain::{
        CriterionEvidenceRequirement, CriterionOrigin, DiagnosticCount, DiagnosticEvidence,
        DiffEvidence, GoalConstraint, GoalOriginalRequest, MinimumTestCaseCount, NonGoal,
        TestCaseEvidence, TestCaseName, TestCaseOutcome, UserDecision,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn loopback_http_verdict_requires_the_exact_step_even_with_matching_spec_and_run()
    -> Result<(), Box<dyn Error>> {
        use a3_domain::{
            ContentHash, FileRevision, LoopbackExpectedBody, LoopbackHeaderChecks,
            LoopbackHttpCheck, LoopbackHttpEvidence, LoopbackHttpEvidenceContext,
            LoopbackHttpObservation, LoopbackHttpProcess, LoopbackHttpRecipe, LoopbackMethod,
            LoopbackProbe, LoopbackProcessSettings, LoopbackStart, ProcessOutputLimit,
            ProcessTimeout, RepositoryPath,
        };
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let recipe = LoopbackHttpRecipe::new(
            path.clone(),
            LoopbackStart::Explicit,
            std::num::NonZeroU16::new(18_765).ok_or("invalid fixture port")?,
            vec![LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
                200,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?],
        )?;
        let recipe_id = recipe.id();
        let spec = VerificationSpec::loopback_http(
            spec_id(151),
            VerificationRequirement::try_from_string("Verify the owned HTTP service".to_owned())?,
            recipe,
        );
        let snapshot = SnapshotId::from_bytes([152; 32]);
        let run = AgentRunId::from_bytes([153; 32]);
        let step = TaskStepDefinition::new(
            TaskStepId::from_bytes([154; 32]),
            None,
            TaskStepOutcome::try_from_string("Verify the selected endpoint".to_owned())?,
            TaskStepRationale::try_from_string("Observe the actual response".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "Owned HTTP response".to_owned(),
            )?],
            spec.clone(),
        )?;
        let evidence = VerificationEvidence::LoopbackHttp(LoopbackHttpEvidence::new(
            LoopbackHttpEvidenceContext {
                verification_run_id: VerificationRunId::from_bytes([155; 32]),
                spec_id: spec.id(),
                run_id: run,
                tool_run_id: ToolRunId::from_bytes([156; 32]),
                step_id: TaskStepId::from_bytes([157; 32]),
                snapshot_id: snapshot,
            },
            recipe_id,
            FileRevision::new(path, ContentHash::from_bytes([158; 32])),
            LoopbackHttpProcess::new(
                PolicyResourceId::from_bytes([159; 32]),
                LoopbackProcessSettings {
                    timeout: ProcessTimeout::from_millis(10_000)?,
                    stdout_limit: ProcessOutputLimit::new(512)?,
                    stderr_limit: ProcessOutputLimit::new(512)?,
                    system_root: false,
                },
            ),
            PolicyDecisionId::from_bytes([160; 32]),
            vec![LoopbackHttpObservation::new(
                PolicyDecisionId::from_bytes([161; 32]),
                200,
                ContentHash::from_bytes([162; 32]),
                0,
                LoopbackHeaderChecks::default(),
            )?],
        )?);
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &evidence),
            VerificationEvidenceEvaluation::Passed
        );
        let current = empty_index(snapshot)?;
        let id = StepVerificationId::from_bytes([163; 32]);
        let at = TaskLedgerTimestamp::from_unix_millis(5)?;
        assert!(matches!(
            EvaluateStepVerification.execute(id, &spec, run, &evidence, &current, at),
            Err(EvaluateStepVerificationError::EvidenceMismatch)
        ));
        assert!(matches!(
            EvaluateStepVerification.execute_for_step(id, &step, run, &evidence, &current, at),
            Err(EvaluateStepVerificationError::EvidenceMismatch)
        ));
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum SourceCase {
        Complete,
        ProgressOnly,
        Gap,
        Constraint,
        NonGoal,
        Decision,
        Orphan,
        MissingRecipe,
        Optional,
    }

    /// Controlled state fixture, not a model or real-process correctness claim.
    fn original_fixture(
        case: SourceCase,
        spec: VerificationSpec,
        cases: Vec<TestCaseEvidence>,
        progress: Option<VerificationSpec>,
    ) -> Result<(AcceptanceVerificationRequest, CountingVerificationStore), Box<dyn Error>> {
        let task_id = TaskId::from_bytes([101; 32]);
        let criterion_id = AcceptanceCriterionId::from_bytes([102; 32]);
        let kind = if matches!(case, SourceCase::Optional) {
            AcceptanceCriterionRequirement::Should
        } else {
            AcceptanceCriterionRequirement::Must
        };
        let plain = AcceptanceCriterion::with_requirement(
            criterion_id,
            AcceptanceCriterionStatement::try_from_string("the selected check passes".to_owned())?,
            kind,
        );
        let draft = |criterion| -> Result<GoalContractDraft, Box<dyn Error>> {
            Ok(GoalContractDraft::new(
                GoalObjective::try_from_string(
                    "preserve the complete original obligation".to_owned(),
                )?,
                vec![criterion],
                if matches!(case, SourceCase::Constraint) {
                    vec![GoalConstraint::try_from_string(
                        "keep the protected notes unchanged".to_owned(),
                    )?]
                } else {
                    Vec::new()
                },
                if matches!(case, SourceCase::NonGoal) {
                    vec![NonGoal::try_from_string(
                        "do not extend the feature scope".to_owned(),
                    )?]
                } else {
                    Vec::new()
                },
                if matches!(case, SourceCase::Decision) {
                    vec![UserDecision::try_from_string(
                        "the user selected the existing interface".to_owned(),
                    )?]
                } else {
                    Vec::new()
                },
                SuccessVerification::try_from_string("use current matching evidence".to_owned())?,
            )?)
        };
        let at = GoalContractTimestamp::from_unix_millis(1)?;
        let provisional = GoalContract::initial(task_id, draft(plain.clone())?, at);
        let first = "The selected behavioral check must pass.\n";
        let text = if matches!(case, SourceCase::Gap) {
            format!("{first}A late global requirement must remain open.\n")
        } else {
            first.to_owned()
        };
        let original =
            GoalOriginalRequest::try_from_bytes(provisional.reference(), text.into_bytes())?;
        let end = if matches!(case, SourceCase::Gap) {
            first.len()
        } else {
            original.as_bytes().len()
        };
        let proof = if matches!(case, SourceCase::MissingRecipe) {
            CriterionEvidenceRequirement::RuntimeNeedsRecipe
        } else {
            CriterionEvidenceRequirement::ExistingTest {
                command: DiscoveredCommandId::from_bytes([103; 32]),
                selector: TestCaseSelector::All,
                minimum_cases: MinimumTestCaseCount::new(1)?,
                scope: VerificationScope::Targeted,
                python_integer_translation: None,
            }
        };
        let criterion = plain.with_origin(CriterionOrigin::new(original.range(0, end)?, proof));
        let base = GoalContract::initial(task_id, draft(criterion)?, at);
        let goal = if matches!(case, SourceCase::Orphan) {
            base
        } else {
            base.with_original(original)?
        };
        let run_id = AgentRunId::from_bytes([104; 32]);
        let snapshot = SnapshotId::from_bytes([105; 32]);
        let index = empty_index(snapshot)?;
        let publication = if matches!(case, SourceCase::ProgressOnly) {
            let graph = LinkedGraph::new(
                snapshot,
                vec![a3_domain::FileRevision::new(
                    a3_domain::RepositoryPath::try_from_bytes(b"scaffold.py".to_vec())?,
                    a3_domain::ContentHash::from_bytes([115; 32]),
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )?;
            let policy = ModulePolicyVersion::v1();
            let card = RepositoryCard::new(
                snapshot,
                policy,
                Vec::new(),
                Vec::new(),
                ModuleSymbolSet::empty(),
                1,
                0,
            )?;
            IndexPublication::new(
                graph,
                index.publication().ranking().clone(),
                Vec::new(),
                ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?,
            )?
        } else {
            index.publication().clone()
        };
        let current = PublishedIndex::new(
            IndexRunRecord::new(
                IndexRunId::from_bytes([106; 32]),
                snapshot,
                RankingPolicyVersion::v1(),
                IndexRunSequence::new(2)?,
                IndexRunStatus::Published,
            ),
            publication,
        )?;
        let command_id = match spec.target() {
            VerificationTarget::Command { command_id, .. }
            | VerificationTarget::Test { command_id, .. }
            | VerificationTarget::Diagnostic { command_id, .. } => *command_id,
            _ => DiscoveredCommandId::from_bytes([103; 32]),
        };
        let command = CommandEvidence::new(
            CommandEvidenceContext::new(
                VerificationRunId::from_bytes([107; 32]),
                spec.id(),
                run_id,
                ToolRunId::from_bytes([108; 32]),
                command_id,
                snapshot,
            ),
            VerificationDependencies::new(Vec::new())?,
            &successful_process()?,
        );
        let evidence = match spec.target() {
            VerificationTarget::Diagnostic { .. } => VerificationEvidence::Diagnostic(
                DiagnosticEvidence::new(command, DiagnosticCount::new(0), DiagnosticCount::new(0)),
            ),
            VerificationTarget::Command { .. } => VerificationEvidence::Command(command),
            VerificationTarget::DiffInvariant(_) => {
                VerificationEvidence::Diff(DiffEvidence::from_published_indexes(
                    VerificationRunId::from_bytes([107; 32]),
                    spec.id(),
                    run_id,
                    &empty_index(SnapshotId::from_bytes([109; 32]))?,
                    &current,
                )?)
            }
            _ => VerificationEvidence::Test(TestEvidence::new(command, cases)?),
        };
        let main = TaskStepId::from_bytes([110; 32]);
        let definition = |id, spec| -> Result<TaskStepDefinition, Box<dyn Error>> {
            Ok(TaskStepDefinition::new(
                id,
                None,
                TaskStepOutcome::try_from_string("complete the bounded work".to_owned())?,
                TaskStepRationale::try_from_string("the goal requires this result".to_owned())?,
                Vec::new(),
                vec![ExpectedTaskEvidence::try_from_string(
                    "current proof".to_owned(),
                )?],
                spec,
            )?)
        };
        let main_definition = definition(main, spec.clone())?;
        let main_definition = if matches!(case, SourceCase::ProgressOnly) {
            main_definition.with_criterion_coverage(vec![criterion_id], Vec::new())?
        } else {
            main_definition.with_acceptance_criteria(vec![criterion_id])?
        };
        let mut definitions = vec![main_definition];
        if let Some(progress) = progress {
            definitions.push(
                definition(TaskStepId::from_bytes([111; 32]), progress)?
                    .with_criterion_coverage(vec![criterion_id], Vec::new())?,
            );
        }
        let mut ledger = TaskLedger::new(
            goal.reference(),
            definitions,
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        let mut timestamp = 3;
        let steps = ledger
            .steps()
            .map(|step| {
                (
                    step.definition().id(),
                    step.definition().verification_spec().id(),
                )
            })
            .collect::<Vec<_>>();
        for (step, verification_spec) in steps {
            // Pre-existing Passed fixture state deliberately tests acceptance's
            // independent checks instead of trusting the stored success marker.
            ledger.start_step(
                step,
                run_id,
                TaskLedgerTimestamp::from_unix_millis(timestamp)?,
            )?;
            ledger.begin_step_verification(
                step,
                run_id,
                None,
                vec![evidence.id()],
                TaskLedgerTimestamp::from_unix_millis(timestamp + 1)?,
            )?;
            ledger.finish_step_verification(
                step,
                StepVerification::new(
                    StepVerificationId::from_bytes([timestamp as u8; 32]),
                    verification_spec,
                    run_id,
                    StepVerificationOutcome::Passed,
                    vec![evidence.id()],
                    TaskLedgerTimestamp::from_unix_millis(timestamp + 2)?,
                )?,
            )?;
            timestamp += 3;
        }
        let now = AgentRunTimestamp::from_unix_millis(timestamp)?;
        let run = AgentRun::reconstruct(
            AgentRunIdentity::new(run_id, goal.reference(), ledger.revision(), None),
            AgentRunMaterializedState::new(
                a3_domain::AgentControllerState::Verify,
                RunEventSequence::FIRST,
                snapshot,
            ),
            AgentRunTiming::new(now, now),
        )?;
        let memory =
            a3_domain::RunMemoryCheckpoint::compile(&goal, &ledger, &run, &current, Vec::new())?;
        let request =
            AcceptanceVerificationRequest::new(test_project()?, &run, goal, ledger, memory)?;
        Ok((
            request,
            CountingVerificationStore {
                state: StoredVerificationState::new(current, vec![evidence])?,
                reads: AtomicUsize::new(0),
            },
        ))
    }

    #[derive(Debug)]
    struct CountingVerificationStore {
        state: StoredVerificationState,
        reads: AtomicUsize,
    }

    impl VerificationEvidenceStore for CountingVerificationStore {
        fn append_verification_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _evidence: &'a VerificationEvidence,
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, ()> {
            Box::pin(async { Err(VerificationEvidenceStoreFailure::Unavailable) })
        }
        fn load_verification_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task: TaskId,
            _ids: &'a [TaskEvidenceId],
            _snapshot: SnapshotId,
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(self.state.clone()) })
        }
        fn load_verification_inspection_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task: TaskId,
            _ids: &'a [TaskEvidenceId],
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(self.state.clone()) })
        }
    }

    fn original_test_spec() -> Result<VerificationSpec, Box<dyn Error>> {
        Ok(VerificationSpec::test(
            spec_id(112),
            requirement()?,
            DiscoveredCommandId::from_bytes([103; 32]),
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        ))
    }

    fn passing_original_case() -> Result<Vec<TestCaseEvidence>, Box<dyn Error>> {
        Ok(vec![TestCaseEvidence::new(
            TestCaseName::try_from_string("behavior::selected".to_owned())?,
            TestCaseOutcome::Passed,
        )])
    }

    #[test]
    fn original_gap_and_unbound_text_rules_reject_before_state_reads() -> Result<(), Box<dyn Error>>
    {
        for case in [
            SourceCase::Gap,
            SourceCase::Constraint,
            SourceCase::NonGoal,
            SourceCase::Decision,
        ] {
            let (request, store) =
                original_fixture(case, original_test_spec()?, passing_original_case()?, None)?;
            let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
                &DeterministicAcceptanceVerifier::new(&store),
                &request,
                AcceptanceVerifierTimeout::DEFAULT,
                &ActiveControl,
            ))?;
            assert_eq!(
                outcome,
                AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
            );
            assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        }
        Ok(())
    }

    #[test]
    fn original_orphan_cannot_fall_back_to_the_legacy_trait_path() -> Result<(), Box<dyn Error>> {
        let (request, store) = original_fixture(
            SourceCase::Orphan,
            original_test_spec()?,
            passing_original_case()?,
            None,
        )?;
        let result = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ));
        assert_eq!(result, Err(AcceptanceVerifierFailure::InvalidResult));
        assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn original_trait_path_rejects_otherwise_green_lint_command_and_diff_specs()
    -> Result<(), Box<dyn Error>> {
        let command = DiscoveredCommandId::from_bytes([103; 32]);
        for spec in [
            VerificationSpec::command(
                spec_id(112),
                requirement()?,
                command,
                VerificationScope::Targeted,
            ),
            VerificationSpec::diagnostic(
                spec_id(112),
                requirement()?,
                command,
                a3_domain::DiagnosticPolicy::NoWarnings,
                VerificationScope::Targeted,
            ),
            VerificationSpec::diff_invariant(
                spec_id(112),
                requirement()?,
                DiffInvariantVerification::new(DiffInvariantMode::NoChanges, Vec::new())?,
            ),
        ] {
            let (request, store) = original_fixture(
                SourceCase::Complete,
                spec.clone(),
                passing_original_case()?,
                None,
            )?;
            let artifact = store
                .state
                .evidence()
                .first()
                .ok_or_else(|| std::io::Error::other("missing fixture evidence"))?;
            assert_eq!(
                VerificationEvidenceEvaluation::evaluate(&spec, artifact),
                VerificationEvidenceEvaluation::Passed
            );
            assert_eq!(
                EvidenceFreshness::evaluate(artifact, store.state.published_index()),
                EvidenceFreshness::Fresh
            );
            let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
                &DeterministicAcceptanceVerifier::new(&store),
                &request,
                AcceptanceVerifierTimeout::DEFAULT,
                &ActiveControl,
            ))?;
            assert_eq!(
                outcome,
                AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::CriterionFailed)
            );
            assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        }
        Ok(())
    }

    #[test]
    fn original_guard_checks_optional_mapped_criteria_too() -> Result<(), Box<dyn Error>> {
        let spec = VerificationSpec::command(
            spec_id(112),
            requirement()?,
            DiscoveredCommandId::from_bytes([103; 32]),
            VerificationScope::Targeted,
        );
        let (request, store) =
            original_fixture(SourceCase::Optional, spec, passing_original_case()?, None)?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::CriterionFailed)
        );
        assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn original_runtime_without_recipe_stays_open_before_state_reads() -> Result<(), Box<dyn Error>>
    {
        let (request, store) = original_fixture(
            SourceCase::MissingRecipe,
            original_test_spec()?,
            passing_original_case()?,
            None,
        )?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::CriterionFailed)
        );
        assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn original_structural_coverage_does_not_turn_zero_cases_into_success()
    -> Result<(), Box<dyn Error>> {
        let (request, store) = original_fixture(
            SourceCase::Complete,
            original_test_spec()?,
            Vec::new(),
            None,
        )?;
        assert!(GoalOriginalCoverage::derive_from(request.goal_contract())?.is_complete());
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::CriterionFailed)
        );
        assert_eq!(store.reads.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn original_matching_evidence_still_passes_without_inventing_progress_obligations()
    -> Result<(), Box<dyn Error>> {
        let progress = VerificationSpec::diff_invariant(
            spec_id(113),
            requirement()?,
            DiffInvariantVerification::new(DiffInvariantMode::NoChanges, Vec::new())?,
        );
        let (request, store) = original_fixture(
            SourceCase::Complete,
            original_test_spec()?,
            passing_original_case()?,
            Some(progress),
        )?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert!(matches!(outcome, AcceptanceVerifierOutcome::Accepted(_)));
        assert_eq!(store.reads.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn clean_legacy_goal_keeps_its_existing_trait_behavior() -> Result<(), Box<dyn Error>> {
        let (request, original_store) = should_only_fixture(false)?;
        let store = CountingVerificationStore {
            state: original_store.state,
            reads: AtomicUsize::new(0),
        };
        assert!(request.goal_contract().original().is_none());
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert!(matches!(outcome, AcceptanceVerifierOutcome::Accepted(_)));
        assert_eq!(store.reads.load(Ordering::SeqCst), 1);
        Ok(())
    }

    /// Controlled stored-state counterexample, not a real-process correctness claim.
    fn snapshot_transition_fixture(
        source: bool,
        current_evidence: bool,
    ) -> Result<(AcceptanceVerificationRequest, CountingVerificationStore), Box<dyn Error>> {
        let spec = original_test_spec()?;
        let (template, _) = original_fixture(
            SourceCase::Complete,
            spec.clone(),
            passing_original_case()?,
            None,
        )?;
        let source_goal = template.goal_contract();
        let criterion = &source_goal.draft().acceptance_criteria()[0];
        let criterion_id = criterion.id();
        let goal = if source {
            source_goal.clone()
        } else {
            GoalContract::initial(
                source_goal.task_id(),
                GoalContractDraft::new(
                    source_goal.draft().objective().clone(),
                    vec![AcceptanceCriterion::with_requirement(
                        criterion.id(),
                        criterion.statement().clone(),
                        criterion.requirement(),
                    )],
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    source_goal.draft().success_verification().clone(),
                )?,
                source_goal.created_at(),
            )
        };
        let test = a3_domain::FileRevision::new(
            a3_domain::RepositoryPath::try_from_bytes(b"tests/test_behavior.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([203; 32]),
        );
        let index = |snapshot, subject_hash| -> Result<PublishedIndex, Box<dyn Error>> {
            let empty = empty_index(snapshot)?;
            let graph = LinkedGraph::new(
                snapshot,
                vec![
                    a3_domain::FileRevision::new(
                        a3_domain::RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
                        a3_domain::ContentHash::from_bytes([subject_hash; 32]),
                    ),
                    test.clone(),
                ],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )?;
            let policy = ModulePolicyVersion::v1();
            let card = RepositoryCard::new(
                snapshot,
                policy,
                Vec::new(),
                Vec::new(),
                ModuleSymbolSet::empty(),
                2,
                0,
            )?;
            let modules = ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?;
            Ok(PublishedIndex::new(
                empty.run(),
                IndexPublication::new(
                    graph,
                    empty.publication().ranking().clone(),
                    Vec::new(),
                    modules,
                )?,
            )?)
        };
        let before = index(SnapshotId::from_bytes([201; 32]), 201)?;
        let current = index(SnapshotId::from_bytes([202; 32]), 202)?;
        let observed = if current_evidence { &current } else { &before };
        let run_id = template.run_id();
        let command = CommandEvidence::new(
            CommandEvidenceContext::new(
                VerificationRunId::from_bytes([204; 32]),
                spec.id(),
                run_id,
                ToolRunId::from_bytes([205; 32]),
                DiscoveredCommandId::from_bytes([103; 32]),
                observed.run().snapshot_id(),
            ),
            VerificationDependencies::new(vec![a3_domain::EvidenceDependency::Present(test)])?,
            &successful_process()?,
        );
        let evidence =
            VerificationEvidence::Test(TestEvidence::new(command, passing_original_case()?)?);
        // The previous dependency rule really does keep this artifact fresh despite
        // a changed subject.py. The Source-only guard must supply the missing check.
        assert_eq!(
            EvidenceFreshness::evaluate(&evidence, &current),
            EvidenceFreshness::Fresh
        );
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &evidence),
            VerificationEvidenceEvaluation::Passed
        );
        let step_id = TaskStepId::from_bytes([206; 32]);
        let definition = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("verify the existing behavior".to_owned())?,
            TaskStepRationale::try_from_string("run the bound existing check".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "current executed cases".to_owned(),
            )?],
            spec.clone(),
        )?
        .with_acceptance_criteria(vec![criterion_id])?;
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![definition],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(3)?)?;
        ledger.begin_step_verification(
            step_id,
            run_id,
            None,
            vec![evidence.id()],
            TaskLedgerTimestamp::from_unix_millis(4)?,
        )?;
        let verification = EvaluateStepVerification.execute(
            StepVerificationId::from_bytes([207; 32]),
            &spec,
            run_id,
            &evidence,
            observed,
            TaskLedgerTimestamp::from_unix_millis(5)?,
        )?;
        assert!(verification.passed());
        ledger.finish_step_verification(step_id, verification)?;
        let now = AgentRunTimestamp::from_unix_millis(6)?;
        let run = AgentRun::reconstruct(
            AgentRunIdentity::new(run_id, goal.reference(), ledger.revision(), None),
            AgentRunMaterializedState::new(
                a3_domain::AgentControllerState::Verify,
                RunEventSequence::FIRST,
                current.run().snapshot_id(),
            ),
            AgentRunTiming::new(now, now),
        )?;
        let memory =
            a3_domain::RunMemoryCheckpoint::compile(&goal, &ledger, &run, &current, Vec::new())?;
        Ok((
            AcceptanceVerificationRequest::new(test_project()?, &run, goal, ledger, memory)?,
            CountingVerificationStore {
                state: StoredVerificationState::new(current, vec![evidence])?,
                reads: AtomicUsize::new(0),
            },
        ))
    }

    #[test]
    fn source_subject_edit_rejects_old_green_evidence_despite_unchanged_test_dependencies()
    -> Result<(), Box<dyn Error>> {
        let (request, store) = snapshot_transition_fixture(true, false)?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::StaleEvidence)
        );
        Ok(())
    }

    #[test]
    fn source_current_snapshot_matching_check_still_passes() -> Result<(), Box<dyn Error>> {
        let (request, store) = snapshot_transition_fixture(true, true)?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert!(matches!(outcome, AcceptanceVerifierOutcome::Accepted(_)));
        Ok(())
    }

    #[test]
    fn legacy_subject_edit_keeps_the_previous_dependency_freshness_behavior()
    -> Result<(), Box<dyn Error>> {
        let (request, store) = snapshot_transition_fixture(false, false)?;
        let outcome = futures::executor::block_on(AcceptanceVerifier::verify(
            &DeterministicAcceptanceVerifier::new(&store),
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl,
        ))?;
        assert!(request.goal_contract().original().is_none());
        assert!(matches!(outcome, AcceptanceVerifierOutcome::Accepted(_)));
        Ok(())
    }

    #[test]
    fn relevant_specs_are_ordered_by_breadth_then_specialized_semantics()
    -> Result<(), Box<dyn Error>> {
        let command_id = DiscoveredCommandId::from_bytes([1; 32]);
        let diff = VerificationSpec::diff_invariant(
            spec_id(1),
            requirement()?,
            DiffInvariantVerification::new(DiffInvariantMode::NoChanges, Vec::new())?,
        );
        let exact_test = VerificationSpec::test(
            spec_id(2),
            requirement()?,
            command_id,
            TestCaseSelector::Exact(TestCaseSelectorName::try_from_string(
                "selected".to_owned(),
            )?),
            a3_domain::MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        );
        let package_test = VerificationSpec::test(
            spec_id(3),
            requirement()?,
            command_id,
            TestCaseSelector::All,
            a3_domain::MinimumTestCaseCount::new(1)?,
            VerificationScope::Package,
        );
        let workspace_command = VerificationSpec::command(
            spec_id(4),
            requirement()?,
            command_id,
            VerificationScope::Workspace,
        );
        let confirmation = VerificationSpec::user_confirm(
            spec_id(5),
            requirement()?,
            PolicyResourceId::from_bytes([5; 32]),
        );
        let input = [
            &workspace_command,
            &confirmation,
            &package_test,
            &exact_test,
            &diff,
        ];

        let ordered = OrderVerificationSpecs.execute(input)?;
        assert_eq!(
            ordered.iter().map(|spec| spec.id()).collect::<Vec<_>>(),
            vec![spec_id(1), spec_id(2), spec_id(3), spec_id(4), spec_id(5)]
        );
        Ok(())
    }

    #[test]
    fn step_result_is_derived_from_semantics_and_current_snapshot() -> Result<(), Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([9; 32]);
        let run_id = AgentRunId::from_bytes([2; 32]);
        let command_id = DiscoveredCommandId::from_bytes([1; 32]);
        let spec = VerificationSpec::command(
            spec_id(1),
            requirement()?,
            command_id,
            VerificationScope::Targeted,
        );
        let evidence = VerificationEvidence::Command(CommandEvidence::new(
            CommandEvidenceContext::new(
                VerificationRunId::from_bytes([3; 32]),
                spec.id(),
                run_id,
                ToolRunId::from_bytes([4; 32]),
                command_id,
                snapshot_id,
            ),
            VerificationDependencies::new(Vec::new())?,
            &successful_process()?,
        ));
        let passed = EvaluateStepVerification.execute(
            StepVerificationId::from_bytes([5; 32]),
            &spec,
            run_id,
            &evidence,
            &empty_index(snapshot_id)?,
            TaskLedgerTimestamp::from_unix_millis(10)?,
        )?;
        assert!(passed.passed());

        let stale = EvaluateStepVerification.execute(
            StepVerificationId::from_bytes([6; 32]),
            &spec,
            run_id,
            &evidence,
            &empty_index(SnapshotId::from_bytes([8; 32]))?,
            TaskLedgerTimestamp::from_unix_millis(11)?,
        )?;
        assert!(!stale.passed());
        Ok(())
    }

    #[test]
    fn exit_zero_without_test_semantics_blocks_mandatory_acceptance() -> Result<(), Box<dyn Error>>
    {
        let snapshot_id = SnapshotId::from_bytes([31; 32]);
        let run_id = AgentRunId::from_bytes([32; 32]);
        let criterion_id = AcceptanceCriterionId::from_bytes([33; 32]);
        let command_id = DiscoveredCommandId::from_bytes([34; 32]);
        let spec = VerificationSpec::test(
            spec_id(35),
            requirement()?,
            command_id,
            TestCaseSelector::All,
            a3_domain::MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        );
        let evidence = VerificationEvidence::Test(TestEvidence::new(
            CommandEvidence::new(
                CommandEvidenceContext::new(
                    VerificationRunId::from_bytes([36; 32]),
                    spec.id(),
                    run_id,
                    ToolRunId::from_bytes([37; 32]),
                    command_id,
                    snapshot_id,
                ),
                VerificationDependencies::new(Vec::new())?,
                &successful_process()?,
            ),
            Vec::new(),
        )?);
        let goal = GoalContract::initial(
            TaskId::from_bytes([38; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("require structured test semantics".to_owned())?,
                vec![AcceptanceCriterion::new(
                    criterion_id,
                    AcceptanceCriterionStatement::try_from_string(
                        "the selected tests pass".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("evaluate structured evidence".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let step_id = TaskStepId::from_bytes([39; 32]);
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![
                TaskStepDefinition::new(
                    step_id,
                    None,
                    TaskStepOutcome::try_from_string("execute the selected tests".to_owned())?,
                    TaskStepRationale::try_from_string("the criterion is mandatory".to_owned())?,
                    Vec::new(),
                    vec![ExpectedTaskEvidence::try_from_string(
                        "structured test cases".to_owned(),
                    )?],
                    spec,
                )?
                .with_acceptance_criteria(vec![criterion_id])?,
            ],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(3)?)?;
        ledger.begin_step_verification(
            step_id,
            run_id,
            None,
            vec![evidence.id()],
            TaskLedgerTimestamp::from_unix_millis(4)?,
        )?;
        ledger.finish_step_verification(
            step_id,
            StepVerification::new(
                StepVerificationId::from_bytes([40; 32]),
                spec_id(35),
                run_id,
                StepVerificationOutcome::Passed,
                vec![evidence.id()],
                TaskLedgerTimestamp::from_unix_millis(5)?,
            )?,
        )?;
        let timestamp = AgentRunTimestamp::from_unix_millis(6)?;
        let run = AgentRun::reconstruct(
            AgentRunIdentity::new(run_id, goal.reference(), ledger.revision(), None),
            AgentRunMaterializedState::new(
                a3_domain::AgentControllerState::Verify,
                RunEventSequence::FIRST,
                snapshot_id,
            ),
            AgentRunTiming::new(timestamp, timestamp),
        )?;
        let run_memory = a3_domain::RunMemoryCheckpoint::compile(
            &goal,
            &ledger,
            &run,
            &empty_index(snapshot_id)?,
            Vec::new(),
        )?;
        let request =
            AcceptanceVerificationRequest::new(test_project()?, &run, goal, ledger, run_memory)?;
        let store = StaticVerificationStore {
            state: StoredVerificationState::new(empty_index(snapshot_id)?, vec![evidence])?,
        };
        let outcome =
            futures::executor::block_on(DeterministicAcceptanceVerifier::new(&store).verify(
                &request,
                AcceptanceVerifierTimeout::DEFAULT,
                &ActiveControl,
            ))?;
        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::CriterionFailed)
        );
        Ok(())
    }

    #[test]
    fn should_only_goal_can_complete_without_verification_evidence() -> Result<(), Box<dyn Error>> {
        let (request, store) = should_only_fixture(false)?;

        let outcome =
            futures::executor::block_on(DeterministicAcceptanceVerifier::new(&store).verify(
                &request,
                AcceptanceVerifierTimeout::DEFAULT,
                &ActiveControl,
            ))?;

        assert!(matches!(outcome, AcceptanceVerifierOutcome::Accepted(_)));
        Ok(())
    }

    #[test]
    fn current_task_hypothesis_blocks_even_a_should_only_goal() -> Result<(), Box<dyn Error>> {
        let (request, store) = should_only_fixture(true)?;

        let outcome =
            futures::executor::block_on(DeterministicAcceptanceVerifier::new(&store).verify(
                &request,
                AcceptanceVerifierTimeout::DEFAULT,
                &ActiveControl,
            ))?;

        assert_eq!(
            outcome,
            AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::BlockingHypothesis)
        );
        Ok(())
    }

    fn should_only_fixture(
        blocking_hypothesis: bool,
    ) -> Result<(AcceptanceVerificationRequest, StaticVerificationStore), Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([61; 32]);
        let criterion_id = AcceptanceCriterionId::from_bytes([62; 32]);
        let spec_id = VerificationSpecId::from_bytes([63; 32]);
        let goal = GoalContract::initial(
            TaskId::from_bytes([64; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("complete the optional check".to_owned())?,
                vec![AcceptanceCriterion::with_requirement(
                    criterion_id,
                    AcceptanceCriterionStatement::try_from_string(
                        "the optional confirmation is available".to_owned(),
                    )?,
                    AcceptanceCriterionRequirement::Should,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("enforce only mandatory criteria".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let ledger = TaskLedger::new(
            goal.reference(),
            vec![
                TaskStepDefinition::new(
                    TaskStepId::from_bytes([65; 32]),
                    None,
                    TaskStepOutcome::try_from_string("capture optional confirmation".to_owned())?,
                    TaskStepRationale::try_from_string(
                        "the criterion is explicitly non-blocking".to_owned(),
                    )?,
                    Vec::new(),
                    vec![ExpectedTaskEvidence::try_from_string(
                        "optional user confirmation".to_owned(),
                    )?],
                    VerificationSpec::user_confirm(
                        spec_id,
                        VerificationRequirement::try_from_string(
                            "confirm the optional scope".to_owned(),
                        )?,
                        PolicyResourceId::from_bytes([66; 32]),
                    ),
                )?
                .with_acceptance_criteria(vec![criterion_id])?,
            ],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        let timestamp = AgentRunTimestamp::from_unix_millis(3)?;
        let run = AgentRun::reconstruct(
            AgentRunIdentity::new(
                AgentRunId::from_bytes([67; 32]),
                goal.reference(),
                ledger.revision(),
                None,
            ),
            AgentRunMaterializedState::new(
                a3_domain::AgentControllerState::Verify,
                RunEventSequence::FIRST,
                snapshot_id,
            ),
            AgentRunTiming::new(timestamp, timestamp),
        )?;
        let module_id = a3_domain::ModuleId::from_bytes([68; 32]);
        let published = acceptance_index(snapshot_id, blocking_hypothesis.then_some(module_id))?;
        let claims = if blocking_hypothesis {
            vec![a3_domain::TaskLensClaim::new(
                published.run().id(),
                snapshot_id,
                a3_domain::ModuleCardClaimId::from_bytes([69; 32]),
                module_id,
                a3_domain::ModuleClaimPolarity::Affirms,
                a3_domain::ModuleClaimPredicate::ArchitecturalIntent(
                    a3_domain::ModuleClaimStatement::try_from_string(
                        "the optional path may still violate an invariant".to_owned(),
                    )?,
                ),
                a3_domain::VerifiedClaimKind::Hypothesis,
                a3_domain::VerifiedClaimStatus::Active,
                a3_domain::Confidence::from_basis_points(5_000)?,
                Vec::new(),
            )?]
        } else {
            Vec::new()
        };
        let run_memory =
            a3_domain::RunMemoryCheckpoint::compile(&goal, &ledger, &run, &published, claims)?;
        let request =
            AcceptanceVerificationRequest::new(test_project()?, &run, goal, ledger, run_memory)?;
        Ok((
            request,
            StaticVerificationStore {
                state: StoredVerificationState::new(published, Vec::new())?,
            },
        ))
    }

    #[derive(Debug)]
    struct StaticVerificationStore {
        state: StoredVerificationState,
    }

    impl VerificationEvidenceStore for StaticVerificationStore {
        fn append_verification_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _evidence: &'a VerificationEvidence,
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }

        fn load_verification_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task_id: TaskId,
            _evidence_ids: &'a [TaskEvidenceId],
            _expected_snapshot_id: SnapshotId,
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
            Box::pin(async move { Ok(self.state.clone()) })
        }

        fn load_verification_inspection_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task_id: TaskId,
            _evidence_ids: &'a [TaskEvidenceId],
            _timeout: Duration,
            _control: &'a dyn AgentControllerControl,
        ) -> VerificationEvidenceStoreFuture<'a, StoredVerificationState> {
            Box::pin(async move { Ok(self.state.clone()) })
        }
    }

    #[derive(Debug)]
    struct ActiveControl;

    impl AgentControllerControl for ActiveControl {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    fn requirement() -> Result<VerificationRequirement, Box<dyn Error>> {
        Ok(VerificationRequirement::try_from_string(
            "verification semantics pass".to_owned(),
        )?)
    }

    const fn spec_id(value: u8) -> VerificationSpecId {
        VerificationSpecId::from_bytes([value; 32])
    }

    fn successful_process() -> Result<ProcessRunResult, Box<dyn Error>> {
        let empty = ProcessOutputContent::text(String::new())?;
        let capture = |stream| {
            ProcessOutputCapture::new(
                stream,
                empty.clone(),
                0,
                1_024,
                false,
                ProcessOutputDigest::from_bytes([0; 32]),
            )
        };
        Ok(ProcessRunResult::new(
            PolicyResourceId::from_bytes([6; 32]),
            PolicyDecisionId::from_bytes([7; 32]),
            ProcessTermination::Exited(ProcessExit::new(Some(0), true)?),
            ProcessDuration::from_millis(1),
            capture(ProcessStream::Stdout)?,
            capture(ProcessStream::Stderr)?,
        )?)
    }

    fn empty_index(snapshot_id: SnapshotId) -> Result<PublishedIndex, Box<dyn Error>> {
        acceptance_index(snapshot_id, None)
    }

    fn acceptance_index(
        snapshot_id: SnapshotId,
        module_id: Option<a3_domain::ModuleId>,
    ) -> Result<PublishedIndex, Box<dyn Error>> {
        let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let policy = ModulePolicyVersion::v1();
        let modules = module_id
            .map(|id| {
                a3_domain::RepositoryModule::new(
                    id,
                    a3_domain::ModuleKind::PathBoundary,
                    Some(a3_domain::ModuleRoot::Repository),
                    Vec::new(),
                    ModuleSymbolSet::empty(),
                    ModuleSymbolSet::empty(),
                    ModuleSymbolSet::empty(),
                )
            })
            .transpose()?
            .into_iter()
            .collect::<Vec<_>>();
        let card = RepositoryCard::new(
            snapshot_id,
            policy,
            module_id.into_iter().collect(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            0,
            0,
        )?;
        let modules = ModuleProjection::new(snapshot_id, policy, modules, Vec::new(), card)?;
        let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([8; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(PublishedIndex::new(run, publication)?)
    }

    fn test_project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([41; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([42; 32]),
                WorktreeAnchorId::from_bytes([43; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
    mod criterion_roles;
}
