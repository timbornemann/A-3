use crate::{
    AcceptanceRejection, AcceptanceVerificationRequest, AcceptanceVerifier,
    AcceptanceVerifierFailure, AcceptanceVerifierFuture, AcceptanceVerifierOutcome,
    AcceptanceVerifierTimeout, AgentControllerControl,
};
use a3_domain::{
    AcceptanceCriterionRequirement, AcceptanceCriterionVerification, AcceptanceVerificationReceipt,
    AgentRunId, EvidenceFreshness, ProjectIdentity, PublishedIndex, StepVerification,
    StepVerificationError, StepVerificationId, StepVerificationOutcome, TaskEvidenceId,
    TaskLedgerTimestamp, TaskVerificationTextError, TestCaseSelector, VerificationEvidence,
    VerificationEvidenceEvaluation, VerificationEvidenceFailure, VerificationFailureSummary,
    VerificationMethod, VerificationScope, VerificationSpec, VerificationSpecId,
    VerificationTarget,
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
            let required = required_acceptance_evidence(request)?;
            let requested_ids = required
                .iter()
                .flat_map(|(_, steps)| steps.iter().map(|(_, _, evidence_id)| *evidence_id))
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
                for (spec, expected_run_id, evidence_id) in steps {
                    let evidence = evidence_by_id
                        .get(&evidence_id)
                        .copied()
                        .ok_or(AcceptanceVerifierFailure::InvalidResult)?;
                    if evidence.spec_id() != spec.id() || evidence.run_id() != expected_run_id {
                        return Err(AcceptanceVerifierFailure::InvalidResult);
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

type RequiredAcceptanceEvidence<'a> = Vec<(
    a3_domain::AcceptanceCriterionId,
    Vec<(&'a VerificationSpec, AgentRunId, TaskEvidenceId)>,
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
        .collect::<BTreeMap<_, Vec<(&VerificationSpec, AgentRunId, TaskEvidenceId)>>>();
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
}
