use crate::{
    AdvanceAgentController, AgentActionStore, AgentActionStoreFailure, AgentApprovalSink,
    AgentApprovalSinkFailure, AgentContextCompileInput, AgentContextCompiler,
    AgentControllerControl, AgentControllerError, AgentControllerSignal, AgentInspectionContext,
    AgentInspectionSink, AgentInspectionSinkFailure, AgentMutationResultRecord,
    AgentProcessInspectionKind, AgentRecoveryStore, AgentRecoveryStoreFailure, AppendRunEvent,
    ContextCompileControl, ContextCompileFailure, ContextToolResult, ContextToolResultDigest,
    EvaluateActionPolicy, EvaluateActionPolicyError, EvaluateStepVerification,
    EvaluateStepVerificationError, LanguageParseControl, LanguageParseControlError,
    LanguageParseFailure, MutationActionFingerprint, MutationActionFingerprintError,
    MutationFailureClass, MutationProgressDecision, PatchApplyFailure, PatchAuthorizationError,
    PatchPreviewFailure, PersistPolicyEvaluation, PolicyEvaluationContext, PolicyStore,
    PolicyStoreFailure, PrepareDiscoveredCommand, ProcessAuthorizationError, ProcessEventSink,
    ProcessRunControl, ProcessRunFailure, ProcessRunner, RefreshRepositoryIndex,
    RefreshRepositoryIndexError, RepositoryChangeBatch, RepositoryChangeBatchError,
    RepositoryIndexCompiler, RepositoryIndexControl, RunEventPageLimit, RunJournalStore,
    RunJournalStoreFailure, StoredProjectCommandAllowlist, TaskLedgerStoreVersion,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure, WorkspacePatchControl,
    WorkspacePatchProgressError, WorkspacePatchTool, WorktreeMutationBusy,
    WorktreeMutationCoordinator,
};
use a3_domain::{
    ActionClass, AgentAction, AgentControllerState, AgentMutationDisposition, AgentMutationKind,
    AgentRun, AgentRunAction, AgentRunTimestamp, AgentToolAttemptStatus, ApprovalGrant,
    ApprovalRequestId, CommandEvidence, CommandEvidenceContext, DiscoveredCommandId,
    DiscoveredCommandKind, EvidenceDependency, FileRevision, GoalContract,
    MAX_PYTHON_TRANSLATION_SOURCE_BYTES, ModelProfile, MutationApplicationState,
    MutationReconciliation, PatchAction, PatchChangeSet, PatchPreview, PolicyDecision,
    PolicyDecisionId, PolicyDecisionOutcome, PolicyEvaluationTiming,
    PreparedDiscoveredCommandApproval, ProcessOutputRedaction, ProcessRunResult,
    ProcessTermination, Progress, ProjectCommandCatalog, ProjectIdentity, PublishedIndex,
    PythonIntegerTranslation, PythonIntegerTranslationWitness, RepositoryPath, RunEventCode,
    RunEventId, RunEventKind, RunEventOutcome, RunEventPayload, RunEventRedaction,
    RunEventRedactionSource, RunEventSubject, SnapshotId, StepVerificationId, TaskEvidenceId,
    TaskLedger, TaskLedgerTimestamp, TaskLensSeed, TaskStepBlockingReason, TaskStepFailureReason,
    TaskStepId, TaskStepResultSummary, TaskStepStatus, TestCaseEvidence, TestCaseName,
    TestCaseOutcome, TestEvidence, ToolRunId, VerificationDependencies, VerificationEvidence,
    VerificationEvidenceEvaluation, VerificationMethod, VerificationRunId, VerificationSpec,
    VerificationTarget, WorkspacePolicy, WorkspacePolicyRestriction, WorkspacePolicyRule,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

#[path = "mutating_agent_machine.rs"]
mod machine;
use machine::PreparedMachineMutation;

mod loopback;
pub use loopback::{
    LoopbackApprovalAdmission, LoopbackApprovalSequence, LoopbackProbeApproval,
    LoopbackVerificationAuthority, LoopbackVerificationRequest, LoopbackVerificationResume,
};

const VERIFICATION_EVIDENCE_TIMEOUT: Duration = Duration::from_secs(30);
const APPROVAL_WAIT_REASON: &str = "exact mutation approval is required";
const MUTATION_FAILURE_REASON: &str = "repeated identical mutation action failed";
const MAX_BUILTIN_TEST_CASES: u32 = 100_000;

/// Caller-owned stable identities for every durable event and evidence record in one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationExecutionIds {
    policy_decision_id: PolicyDecisionId,
    approval_request_id: ApprovalRequestId,
    policy_event_id: RunEventId,
    approval_transition_event_id: RunEventId,
    tool_run_id: ToolRunId,
    tool_event_id: RunEventId,
    verification_transition_event_id: RunEventId,
    resolution_transition_event_id: RunEventId,
    context_event_id: RunEventId,
    verification_run_id: VerificationRunId,
    step_verification_id: StepVerificationId,
}

impl MutationExecutionIds {
    /// Groups explicit identities without deriving them from untrusted model content.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        policy_decision_id: PolicyDecisionId,
        approval_request_id: ApprovalRequestId,
        policy_event_id: RunEventId,
        approval_transition_event_id: RunEventId,
        tool_run_id: ToolRunId,
        tool_event_id: RunEventId,
        verification_transition_event_id: RunEventId,
        resolution_transition_event_id: RunEventId,
        context_event_id: RunEventId,
        verification_run_id: VerificationRunId,
        step_verification_id: StepVerificationId,
    ) -> Self {
        Self {
            policy_decision_id,
            approval_request_id,
            policy_event_id,
            approval_transition_event_id,
            tool_run_id,
            tool_event_id,
            verification_transition_event_id,
            resolution_transition_event_id,
            context_event_id,
            verification_run_id,
            step_verification_id,
        }
    }
}

/// Trusted non-repository inputs needed to compile a new context after a mutation retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationContextSeed {
    goal_contract: GoalContract,
    model_profile: ModelProfile,
    action_generation: crate::AgentActionGeneration,
    supplemental_seeds: Vec<TaskLensSeed>,
    tool_results: Vec<ContextToolResult>,
}

impl MutationContextSeed {
    /// Retains the execution strategy when recompiling after a real mutation.
    #[must_use]
    pub fn with_action_generation(mut self, generation: crate::AgentActionGeneration) -> Self {
        self.action_generation = generation;
        self
    }

    /// Retains only authoritative Goal/Profile state plus bounded optional retrieval inputs.
    #[must_use]
    pub fn new(
        goal_contract: GoalContract,
        model_profile: ModelProfile,
        supplemental_seeds: Vec<TaskLensSeed>,
        tool_results: Vec<ContextToolResult>,
    ) -> Self {
        Self {
            goal_contract,
            model_profile,
            action_generation: crate::AgentActionGeneration::SingleAction,
            supplemental_seeds,
            tool_results,
        }
    }

    fn compile_input(
        &self,
        project: &ProjectIdentity,
        ledger: &TaskLedger,
        step_id: TaskStepId,
    ) -> Result<AgentContextCompileInput, MutationControllerFailure> {
        AgentContextCompileInput::new(
            project.clone(),
            self.goal_contract.clone(),
            ledger.clone(),
            step_id,
            self.model_profile.clone(),
            None,
            self.supplemental_seeds.clone(),
            self.tool_results.clone(),
        )
        .map(|input| input.with_action_generation(self.action_generation))
        .map_err(|_| MutationControllerFailure::InvalidContextSeed)
    }
}

/// Current E5 evidence and exact stored confirmation needed to resolve one `run` action.
#[derive(Debug)]
pub struct MutationCommandSelection<'a> {
    catalog: &'a ProjectCommandCatalog,
    authorization: MutationCommandAuthorization<'a>,
}

#[derive(Debug)]
enum MutationCommandAuthorization<'a> {
    Confirmed(&'a StoredProjectCommandAllowlist),
    ApprovalRequired(Box<PreparedDiscoveredCommandApproval>),
}

impl<'a> MutationCommandSelection<'a> {
    /// Binds an exact current catalog to its worktree-local allowlist revision.
    #[must_use]
    pub const fn new(
        catalog: &'a ProjectCommandCatalog,
        confirmation: &'a StoredProjectCommandAllowlist,
    ) -> Self {
        Self {
            catalog,
            authorization: MutationCommandAuthorization::Confirmed(confirmation),
        }
    }

    /// Binds only a current catalog and forces exact one-time central-policy approval.
    pub fn requiring_approval(
        catalog: &'a ProjectCommandCatalog,
        run: &AgentRun,
        ledger: &TaskLedger,
        step_id: TaskStepId,
        command_id: DiscoveredCommandId,
    ) -> Result<Self, a3_domain::DiscoveredCommandProcessError> {
        Ok(Self {
            catalog,
            authorization: MutationCommandAuthorization::ApprovalRequired(Box::new(
                catalog.prepare_for_approval(run, ledger, step_id, command_id)?,
            )),
        })
    }
}

/// Trusted semantic classifier for a completely drained discovered-command result.
pub trait ProcessVerificationEvidenceFactory: fmt::Debug + Send + Sync {
    /// Produces evidence whose semantic fields are derived by a trusted adapter, never the model.
    fn create(
        &self,
        request: ProcessVerificationEvidenceRequest<'_>,
    ) -> Result<VerificationEvidence, ProcessVerificationEvidenceFailure>;
}

/// Proves the closed Python recipe from one bounded, complete current revision.
pub trait PythonIntegerTranslationVerifier: fmt::Debug + Send + Sync {
    /// Checks source without filesystem, persistence, or process access.
    fn prove(
        &self,
        required: &PythonIntegerTranslation,
        revision: &FileRevision,
        source: &[u8],
        control: &dyn LanguageParseControl,
    ) -> Result<Option<PythonIntegerTranslationWitness>, LanguageParseFailure>;
}

#[derive(Debug)]
struct PythonTranslationProofControl<'a, C> {
    owner: &'a C,
    deadline: Instant,
}

impl<C: AgentControllerControl + WorkspacePatchControl> WorkspacePatchControl
    for PythonTranslationProofControl<'_, C>
{
    fn is_cancelled(&self) -> bool {
        AgentControllerControl::is_cancelled(self.owner)
            || WorkspacePatchControl::is_cancelled(self.owner)
            || Instant::now() >= self.deadline
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        self.owner.report_progress(progress)
    }
}

impl<C: AgentControllerControl + WorkspacePatchControl> LanguageParseControl
    for PythonTranslationProofControl<'_, C>
{
    fn is_cancelled(&self) -> bool {
        WorkspacePatchControl::is_cancelled(self)
    }

    fn report_progress(&self, progress: Progress) -> Result<(), LanguageParseControlError> {
        WorkspacePatchControl::report_progress(self, progress)
            .map_err(|_| LanguageParseControlError::Unavailable)
    }
}

/// Exact typed inputs from which a process verification artifact may be derived.
#[derive(Debug, Clone, Copy)]
pub struct ProcessVerificationEvidenceRequest<'a> {
    spec: &'a VerificationSpec,
    run_id: a3_domain::AgentRunId,
    tool_run_id: ToolRunId,
    command_id: DiscoveredCommandId,
    snapshot_id: SnapshotId,
    verification_run_id: VerificationRunId,
    dependencies: &'a VerificationDependencies,
    result: &'a ProcessRunResult,
    adapter: ProcessVerificationAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessVerificationAdapter {
    Generic,
    PythonUnittest,
}

impl ProcessVerificationEvidenceRequest<'_> {
    /// Returns the operational verification contract.
    #[must_use]
    pub const fn spec(&self) -> &VerificationSpec {
        self.spec
    }

    /// Returns the exact discovered command selected by the model.
    #[must_use]
    pub const fn command_id(&self) -> DiscoveredCommandId {
        self.command_id
    }

    /// Returns the fully drained, bounded process result.
    #[must_use]
    pub const fn result(&self) -> &ProcessRunResult {
        self.result
    }

    /// Returns exact current manifest/source dependencies supporting command discovery.
    #[must_use]
    pub const fn dependencies(&self) -> &VerificationDependencies {
        self.dependencies
    }

    /// Constructs base command evidence for trusted specialized Test or Diagnostic wrappers.
    #[must_use]
    pub fn command_evidence(&self) -> CommandEvidence {
        CommandEvidence::new(
            CommandEvidenceContext::new(
                self.verification_run_id,
                self.spec.id(),
                self.run_id,
                self.tool_run_id,
                self.command_id,
                self.snapshot_id,
            ),
            self.dependencies.clone(),
            self.result,
        )
    }
}

/// A trusted classifier could not produce the exact semantics required by the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessVerificationEvidenceFailure {
    /// Command identity did not match the operational specification.
    CommandMismatch,
    /// A language/framework adapter is required for structured Test or Diagnostic semantics.
    SemanticAdapterUnavailable,
    /// The requested evidence kind cannot originate from a process result.
    EvidenceKindMismatch,
    /// Derived data violated the typed evidence boundary.
    InvalidEvidence,
}

impl fmt::Display for ProcessVerificationEvidenceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CommandMismatch => "process verification command does not match its spec",
            Self::SemanticAdapterUnavailable => {
                "process verification needs a structured semantic adapter"
            }
            Self::EvidenceKindMismatch => "verification kind cannot be derived from a process",
            Self::InvalidEvidence => "process verification evidence is invalid",
        })
    }
}

impl Error for ProcessVerificationEvidenceFailure {}

/// Safe production baseline: generic Command semantics are operational; specialized parsers must
/// be injected for Test and Diagnostic evidence instead of treating exit code as proof.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConservativeProcessVerificationEvidenceFactory;

impl ProcessVerificationEvidenceFactory for ConservativeProcessVerificationEvidenceFactory {
    fn create(
        &self,
        request: ProcessVerificationEvidenceRequest<'_>,
    ) -> Result<VerificationEvidence, ProcessVerificationEvidenceFailure> {
        match request.spec.target() {
            VerificationTarget::Command { command_id, .. } if *command_id == request.command_id => {
                Ok(VerificationEvidence::Command(request.command_evidence()))
            }
            VerificationTarget::Command { .. } | VerificationTarget::Test { .. } => {
                if matches!(request.spec.target(), VerificationTarget::Command { .. }) {
                    Err(ProcessVerificationEvidenceFailure::CommandMismatch)
                } else {
                    Err(ProcessVerificationEvidenceFailure::SemanticAdapterUnavailable)
                }
            }
            VerificationTarget::Diagnostic { .. } => {
                Err(ProcessVerificationEvidenceFailure::SemanticAdapterUnavailable)
            }
            VerificationTarget::DiffInvariant(_)
            | VerificationTarget::UserConfirm { .. }
            | VerificationTarget::DeferredCommand(_)
            | VerificationTarget::LoopbackHttp(_)
            | VerificationTarget::Legacy(_) => {
                Err(ProcessVerificationEvidenceFailure::EvidenceKindMismatch)
            }
        }
    }
}

/// Production evidence factory for generic commands and the closed standard-library test adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinProcessVerificationEvidenceFactory;

impl ProcessVerificationEvidenceFactory for BuiltinProcessVerificationEvidenceFactory {
    fn create(
        &self,
        request: ProcessVerificationEvidenceRequest<'_>,
    ) -> Result<VerificationEvidence, ProcessVerificationEvidenceFailure> {
        match request.spec.target() {
            VerificationTarget::Command { command_id, .. } if *command_id == request.command_id => {
                Ok(VerificationEvidence::Command(request.command_evidence()))
            }
            VerificationTarget::Command { .. } => {
                Err(ProcessVerificationEvidenceFailure::CommandMismatch)
            }
            VerificationTarget::Test { command_id, .. } if *command_id != request.command_id => {
                Err(ProcessVerificationEvidenceFailure::CommandMismatch)
            }
            VerificationTarget::Test { .. }
                if request.adapter == ProcessVerificationAdapter::PythonUnittest =>
            {
                python_unittest_evidence(request)
            }
            VerificationTarget::Test { .. } | VerificationTarget::Diagnostic { .. } => {
                Err(ProcessVerificationEvidenceFailure::SemanticAdapterUnavailable)
            }
            VerificationTarget::DiffInvariant(_)
            | VerificationTarget::UserConfirm { .. }
            | VerificationTarget::DeferredCommand(_)
            | VerificationTarget::LoopbackHttp(_)
            | VerificationTarget::Legacy(_) => {
                Err(ProcessVerificationEvidenceFailure::EvidenceKindMismatch)
            }
        }
    }
}

fn python_unittest_evidence(
    request: ProcessVerificationEvidenceRequest<'_>,
) -> Result<VerificationEvidence, ProcessVerificationEvidenceFailure> {
    if !matches!(request.result.termination(), ProcessTermination::Exited(exit) if exit.success())
        || request.result.stdout().truncated()
        || request.result.stderr().truncated()
    {
        return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
    }
    let stdout = request
        .result
        .stdout()
        .content()
        .as_text()
        .ok_or(ProcessVerificationEvidenceFailure::InvalidEvidence)?;
    let stderr = request
        .result
        .stderr()
        .content()
        .as_text()
        .ok_or(ProcessVerificationEvidenceFailure::InvalidEvidence)?;
    let mut count = None;
    let mut successful = false;
    for line in stdout.lines().chain(stderr.lines()) {
        let line = line.trim();
        if line == "OK" {
            if successful {
                return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
            }
            successful = true;
        }
        if let Some(rest) = line.strip_prefix("Ran ") {
            let Some((value, suffix)) = rest.split_once(' ') else {
                return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
            };
            if suffix != "test" && !suffix.starts_with("tests ") && !suffix.starts_with("test ") {
                continue;
            }
            let parsed = value
                .parse::<u32>()
                .map_err(|_| ProcessVerificationEvidenceFailure::InvalidEvidence)?;
            if count.replace(parsed).is_some() {
                return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
            }
        }
    }
    let count = count.ok_or(ProcessVerificationEvidenceFailure::InvalidEvidence)?;
    if !successful || count == 0 || count > MAX_BUILTIN_TEST_CASES {
        return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
    }
    let mut cases = Vec::with_capacity(
        usize::try_from(count).map_err(|_| ProcessVerificationEvidenceFailure::InvalidEvidence)?,
    );
    for index in 1..=count {
        let name = TestCaseName::try_from_string(format!("unittest-case-{index:06}"))
            .map_err(|_| ProcessVerificationEvidenceFailure::InvalidEvidence)?;
        cases.push(TestCaseEvidence::new(name, TestCaseOutcome::Passed));
    }
    let evidence = TestEvidence::new(request.command_evidence(), cases)
        .map_err(|_| ProcessVerificationEvidenceFailure::InvalidEvidence)?;
    Ok(VerificationEvidence::Test(evidence))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceUnittestFailure {
    cases: u32,
    failures: u32,
    assertion_operands: Option<(i64, i64)>,
}

impl SourceUnittestFailure {
    fn summary(self) -> String {
        let mut summary = format!("Python unittest failed ({}/{}).", self.failures, self.cases);
        if let Some((left, right)) = self.assertion_operands {
            summary.push_str(&format!(" An assertion compared {left} and {right}."));
        }
        summary
    }
}

fn source_unittest_failure(result: &ProcessRunResult) -> Option<SourceUnittestFailure> {
    const MAX_REPORT_BYTES: u64 = 16 * 1024;
    let stdout = result.stdout();
    let stderr = result.stderr();
    if !matches!(result.termination(), ProcessTermination::Exited(exit) if exit.code() == Some(1))
        || stdout.truncated()
        || stderr.truncated()
        || stdout
            .observed_bytes()
            .checked_add(stderr.observed_bytes())?
            > MAX_REPORT_BYTES
    {
        return None;
    }

    let mut cases = None;
    let mut failures = None;
    let mut failure_headers = 0_u32;
    let mut assertion_operands = None;
    let mut multiple_integer_assertions = false;
    // unittest reports to stderr. Ordinary stdout is never diagnostic text.
    stdout.content().as_text()?;
    for line in stderr.content().as_text()?.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Ran ") {
            let (count, rest) = rest.split_once(' ')?;
            let duration = rest
                .strip_prefix("test in ")
                .or_else(|| rest.strip_prefix("tests in "))?;
            let seconds: f64 = duration.strip_suffix('s')?.parse().ok()?;
            if !seconds.is_finite()
                || seconds.is_sign_negative()
                || failure_headers == 0
                || cases.replace(count.parse().ok()?).is_some()
            {
                return None;
            }
        } else if let Some(count) = line
            .strip_prefix("FAILED (failures=")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            if cases.is_none() || failures.replace(count.parse().ok()?).is_some() {
                return None;
            }
        } else if line
            .strip_prefix("FAIL: ")
            .is_some_and(|name| !name.is_empty())
        {
            if cases.is_some() {
                return None;
            }
            failure_headers = failure_headers.checked_add(1)?;
        } else if line.starts_with("OK") || line.starts_with("FAILED") || line.starts_with("ERROR:")
        {
            return None;
        } else if failure_headers == 1
            && cases.is_none()
            && let Some((left, right)) = line
                .strip_prefix("AssertionError: ")
                .and_then(|rest| rest.split_once(" != "))
            && let (Ok(left), Ok(right)) = (left.parse::<i64>(), right.parse::<i64>())
        {
            multiple_integer_assertions |= assertion_operands.replace((left, right)).is_some();
        }
    }
    let (cases, failures) = (cases?, failures?);
    (cases <= MAX_BUILTIN_TEST_CASES
        && failures > 0
        && failures <= cases
        && failures == failure_headers)
        .then_some(SourceUnittestFailure {
            cases,
            failures,
            assertion_operands: if failures == 1 && !multiple_integer_assertions {
                assertion_operands
            } else {
                None
            },
        })
}

fn is_source_check_failure_event(event: &a3_domain::RunEvent) -> bool {
    event.kind()
        == (RunEventKind::StateTransition {
            from: AgentControllerState::Execute,
            to: AgentControllerState::Replan,
        })
        && event.payload().code() == RunEventCode::VerificationFailure
        && event.payload().outcome() == Some(RunEventOutcome::Failed)
}

/// Completed controller response; no variant allows the model to assert verification or success.
pub enum MutationControllerOutcome {
    /// Exact policy scope is durable and the controller is waiting for the named request.
    AwaitingApproval(ApprovalRequestId),
    /// Policy denied the mutation and the finite run stopped.
    Denied,
    /// The action succeeded but verification requires another freshly compiled Execute turn.
    NextAction(Box<crate::CompiledAgentContext>),
    /// A complete candidate patch is published and its current step has a bound test.
    BoundVerificationReady {
        /// Planned check selection only. Execution still requires current policy and approval.
        command: AgentRunAction,
        /// Published snapshot after the patch and dependent evidence invalidation.
        snapshot_id: SnapshotId,
    },
    /// Actual bounded machine observation; never repository verification evidence.
    MachineObserved(Box<ContextToolResult>),
    /// Typed current evidence passed and completed the current Task Ledger step.
    StepVerified {
        /// Immutable evidence referenced by the completed step attempt.
        evidence_id: TaskEvidenceId,
        /// Published snapshot against which evidence was checked.
        snapshot_id: SnapshotId,
    },
    /// The progress detector selected the explicit Replan state.
    ReplanRequired {
        /// Content-free terminal failure class.
        failure: MutationFailureClass,
        /// Snapshot safe for subsequent localization.
        snapshot_id: SnapshotId,
    },
    /// A process could have changed the worktree and must be reconciled before another mutation.
    ReconciliationRequired {
        /// Logical tool action whose exact attempt remains Unknown.
        tool_run_id: ToolRunId,
        /// One-based attempt carrying the durable Unknown disposition.
        attempt: a3_domain::AgentToolAttemptNumber,
        /// Last snapshot known before authoritative reconciliation.
        snapshot_id: SnapshotId,
    },
    /// The progress detector or an unreconciled index failure stopped the run.
    Stopped {
        /// Content-free terminal failure class.
        failure: MutationFailureClass,
        /// Last snapshot that remains authoritative for this run.
        snapshot_id: SnapshotId,
    },
}

impl fmt::Debug for MutationControllerOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AwaitingApproval(id) => {
                formatter.debug_tuple("AwaitingApproval").field(id).finish()
            }
            Self::Denied => formatter.write_str("Denied"),
            Self::MachineObserved(result) => formatter
                .debug_tuple("MachineObserved")
                .field(&result.digest())
                .finish(),
            Self::NextAction(context) => formatter
                .debug_struct("NextAction")
                .field("context_digest", &context.digest())
                .field("snapshot_id", &context.snapshot_id())
                .finish(),
            Self::BoundVerificationReady {
                command,
                snapshot_id,
            } => formatter
                .debug_struct("BoundVerificationReady")
                .field("command", command)
                .field("snapshot_id", snapshot_id)
                .finish(),
            Self::StepVerified {
                evidence_id,
                snapshot_id,
            } => formatter
                .debug_struct("StepVerified")
                .field("evidence_id", evidence_id)
                .field("snapshot_id", snapshot_id)
                .finish(),
            Self::ReplanRequired {
                failure,
                snapshot_id,
            } => formatter
                .debug_struct("ReplanRequired")
                .field("failure", failure)
                .field("snapshot_id", snapshot_id)
                .finish(),
            Self::ReconciliationRequired {
                tool_run_id,
                attempt,
                snapshot_id,
            } => formatter
                .debug_struct("ReconciliationRequired")
                .field("tool_run_id", tool_run_id)
                .field("attempt", attempt)
                .field("snapshot_id", snapshot_id)
                .finish(),
            Self::Stopped {
                failure,
                snapshot_id,
            } => formatter
                .debug_struct("Stopped")
                .field("failure", failure)
                .field("snapshot_id", snapshot_id)
                .finish(),
        }
    }
}

#[derive(Debug)]
enum PreparedMutation {
    Machine(Box<PreparedMachineMutation>),
    Patch(PatchAction),
    Run {
        action: AgentRunAction,
        command_kind: DiscoveredCommandKind,
        verification_adapter: ProcessVerificationAdapter,
        result_spec: a3_domain::ProcessSpec,
        dependencies: VerificationDependencies,
        approval_required: bool,
        core_owned_check: bool,
    },
}

impl PreparedMutation {
    fn step_id(&self) -> TaskStepId {
        match self {
            Self::Machine(action) => action.step_id(),
            Self::Patch(action) => action.task_step_id(),
            Self::Run { action, .. } => action.step_id(),
        }
    }

    fn policy_action(&self) -> a3_domain::PolicyAction {
        match self {
            Self::Machine(action) => action.policy_action(),
            Self::Patch(action) => action.policy_action(),
            Self::Run { result_spec, .. } => result_spec.policy_action(),
        }
    }

    const fn kind(&self) -> AgentMutationKind {
        match self {
            Self::Machine(action) => action.kind(),
            Self::Patch(_) => AgentMutationKind::Patch,
            Self::Run { .. } => AgentMutationKind::Process,
        }
    }

    const fn requires_process_approval(&self) -> bool {
        matches!(
            self,
            Self::Run {
                approval_required: true,
                ..
            }
        )
    }
}

/// E7 application use case composing the existing patch, process, policy, index, verification,
/// context, journal, and ledger boundaries without creating a second controller loop.
#[derive(Debug, Clone, Copy)]
pub struct ExecuteMutatingAgentAction<'a> {
    permissions: Option<&'a dyn crate::AgentPermissionStore>,
    machine_files: Option<&'a dyn crate::MachineFileTool>,
    machine_network: Option<&'a dyn crate::MachineNetworkTool>,
    machine_environment: &'a [a3_domain::ProcessEnvironmentVariable],
    coordinator: &'a WorktreeMutationCoordinator,
    policy_store: &'a dyn PolicyStore,
    journal: &'a dyn RunJournalStore,
    action_store: &'a dyn AgentActionStore,
    recovery: &'a dyn AgentRecoveryStore,
    evidence_store: &'a dyn VerificationEvidenceStore,
    inspection: &'a dyn AgentInspectionSink,
    approval: &'a dyn AgentApprovalSink,
    patch_tool: &'a dyn WorkspacePatchTool,
    process_runner: &'a dyn ProcessRunner,
    evidence_factory: &'a dyn ProcessVerificationEvidenceFactory,
    python_integer_translation_verifier: Option<&'a dyn PythonIntegerTranslationVerifier>,
    source_failure_goal: Option<&'a GoalContract>,
    context_compiler: &'a dyn AgentContextCompiler,
    refresh: &'a RefreshRepositoryIndex,
}

impl<'a> ExecuteMutatingAgentAction<'a> {
    /// Wires only the narrow existing capabilities required for one finite mutating action.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        coordinator: &'a WorktreeMutationCoordinator,
        policy_store: &'a dyn PolicyStore,
        journal: &'a dyn RunJournalStore,
        action_store: &'a dyn AgentActionStore,
        recovery: &'a dyn AgentRecoveryStore,
        evidence_store: &'a dyn VerificationEvidenceStore,
        inspection: &'a dyn AgentInspectionSink,
        approval: &'a dyn AgentApprovalSink,
        patch_tool: &'a dyn WorkspacePatchTool,
        process_runner: &'a dyn ProcessRunner,
        evidence_factory: &'a dyn ProcessVerificationEvidenceFactory,
        context_compiler: &'a dyn AgentContextCompiler,
        refresh: &'a RefreshRepositoryIndex,
    ) -> Self {
        Self {
            permissions: None,
            machine_files: None,
            machine_network: None,
            machine_environment: &[],
            coordinator,
            policy_store,
            journal,
            action_store,
            recovery,
            evidence_store,
            inspection,
            approval,
            patch_tool,
            process_runner,
            evidence_factory,
            python_integer_translation_verifier: None,
            source_failure_goal: None,
            context_compiler,
            refresh,
        }
    }

    /// Executes at most one patch or discovered command and resolves it through Verify.
    /// Injects the app-owned durable permission source; absence preserves historical contracts.
    #[must_use]
    pub const fn with_permissions(
        mut self,
        permissions: &'a dyn crate::AgentPermissionStore,
    ) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Injects the checked source verifier for a guarded Python test specification.
    #[must_use]
    pub const fn with_python_integer_translation_verifier(
        mut self,
        verifier: &'a dyn PythonIntegerTranslationVerifier,
    ) -> Self {
        self.python_integer_translation_verifier = Some(verifier);
        self
    }

    /// Binds guarded check failures to the candidate's immutable original-backed Goal.
    #[must_use]
    pub const fn with_source_failure_replan(mut self, goal: &'a GoalContract) -> Self {
        self.source_failure_goal = Some(goal);
        self
    }

    /// Injects only the Core-owned closed machine tools, never a generic shell or filesystem API.
    #[must_use]
    pub const fn with_machine_tools(
        mut self,
        files: &'a dyn crate::MachineFileTool,
        network: &'a dyn crate::MachineNetworkTool,
        environment: &'a [a3_domain::ProcessEnvironmentVariable],
    ) -> Self {
        self.machine_files = Some(files);
        self.machine_network = Some(network);
        self.machine_environment = environment;
        self
    }

    /// Executes one bounded action under current durable policy and evidence.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        published: &PublishedIndex,
        action: AgentAction,
        command: Option<MutationCommandSelection<'_>>,
        workspace_policy: &WorkspacePolicy,
        approval: Option<&mut ApprovalGrant>,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        approval_expires_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        process_events: &dyn ProcessEventSink,
        control: &C,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl
            + WorkspacePatchControl,
    {
        let started = Instant::now();
        let fingerprint = MutationActionFingerprint::from_action(&action)?;
        let prepared = if let AgentAction::Machine(machine) = action {
            PreparedMutation::Machine(Box::new(
                self.prepare_machine(project, run, ledger, published, machine, control)
                    .await?,
            ))
        } else {
            prepare_action(project, run, ledger, published, action, command)?
        };
        let step_id = prepared.step_id();
        if let Some(goal) = self.source_failure_goal {
            let step = ledger
                .step(step_id)
                .ok_or(MutationControllerFailure::AnchorMismatch)?;
            if goal.original().is_none()
                || goal.reference() != run.goal_contract()
                || goal.reference() != ledger.goal_contract()
                || goal != &context_seed.goal_contract
                || crate::AdmitSourceTaskStep
                    .execute(goal, step.definition())
                    .is_err()
            {
                return Err(MutationControllerFailure::AnchorMismatch);
            }
        }
        let mutation_kind = prepared.kind();
        let lease = self
            .coordinator
            .try_acquire(run.id(), project.worktree().id(), fingerprint)?;

        let patch_preview = match &prepared {
            PreparedMutation::Patch(patch) => {
                let preview = self
                    .patch_tool
                    .preview(project, published, patch, control)
                    .await;
                if let Err(PatchPreviewFailure::Conflict(kind)) = &preview
                    && context_seed.goal_contract.reference() == run.goal_contract()
                    && context_seed.goal_contract.original().is_some()
                {
                    self.fail_active_step_and_transition(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        step_id,
                        kind.repair_code(),
                        AgentControllerSignal::FatalFailure,
                        ids.resolution_transition_event_id,
                        observed_at,
                        control,
                    )
                    .await?;
                }
                Some(require_bound_patch_preview(patch, preview?)?)
            }
            PreparedMutation::Run { .. } | PreparedMutation::Machine(_) => None,
        };

        let core_check_automatic = matches!(
            &prepared,
            PreparedMutation::Run {
                core_owned_check: true,
                ..
            }
        ) && if let Some(store) = self.permissions {
            store
                .load_agent_permissions()
                .await
                .map_err(|_| MutationControllerFailure::InvalidPolicyResult)?
                .mode()
                == a3_domain::AgentPermissionMode::FullMachine
        } else {
            false
        };
        let forced_policy = if prepared.requires_process_approval() && !core_check_automatic {
            let mut rules = workspace_policy.rules().to_vec();
            if !rules
                .iter()
                .any(|rule| rule.class() == ActionClass::ExecuteSafe)
            {
                rules.push(WorkspacePolicyRule::new(
                    ActionClass::ExecuteSafe,
                    WorkspacePolicyRestriction::RequireApproval,
                ));
            }
            Some(
                WorkspacePolicy::new(rules)
                    .map_err(|_| MutationControllerFailure::InvalidPolicyResult)?,
            )
        } else {
            None
        };
        let effective_policy = forced_policy.as_ref().unwrap_or(workspace_policy);
        let decision = self
            .evaluate_and_persist_policy(
                project,
                run,
                &prepared.policy_action(),
                effective_policy,
                approval,
                ids,
                observed_at,
                approval_expires_at,
            )
            .await?;

        if let Some(preview) = patch_preview.as_ref() {
            let spec_id = current_step_spec(ledger, step_id)?.id();
            let context = inspection_context(run, step_id, spec_id, run.current_snapshot_id());
            let recorded = self
                .inspection
                .record_patch_preview(project, context, preview);
            if decision.outcome() == PolicyDecisionOutcome::ApprovalRequired {
                recorded.map_err(MutationControllerFailure::Inspection)?;
            }
        }

        match decision.outcome() {
            PolicyDecisionOutcome::ApprovalRequired => {
                let request_id = decision
                    .approval_request_id()
                    .ok_or(MutationControllerFailure::InvalidPolicyResult)?;
                self.record_approval_request(
                    project,
                    run,
                    ledger,
                    &prepared,
                    request_id,
                    decision.reason(),
                )
                .await?;
                if run.state() != AgentControllerState::AwaitApproval {
                    self.await_approval(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        step_id,
                        ids.approval_transition_event_id,
                        observed_at,
                        AgentControllerControl::is_cancelled(control),
                    )
                    .await?;
                }
                return Ok(MutationControllerOutcome::AwaitingApproval(request_id));
            }
            PolicyDecisionOutcome::Denied => {
                self.deny_action(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    ids.approval_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                return Ok(MutationControllerOutcome::Denied);
            }
            PolicyDecisionOutcome::Allowed => {}
        }

        // Admission is the start of this tool action. Later user selections govern the
        // next action; no decision based on an older automatic mode may enter here.
        if decision.reason() == a3_domain::PolicyDecisionReason::SystemAutomatic
            && let Some(store) = self.permissions
        {
            let current = store
                .load_agent_permissions()
                .await
                .map_err(|_| MutationControllerFailure::InvalidPolicyResult)?;
            if decision.permission_settings() != Some(current) {
                return Err(MutationControllerFailure::PermissionsChanged);
            }
        }

        if run.state() == AgentControllerState::AwaitApproval {
            self.resume_after_approval(
                project,
                run,
                ledger,
                ledger_version,
                step_id,
                ids.approval_transition_event_id,
                observed_at,
                control,
            )
            .await?;
        }

        let attempt = if let PreparedMutation::Machine(machine) = &prepared {
            self.recovery
                .begin_machine_mutation_attempt(
                    project,
                    run.id(),
                    run.current_snapshot_id(),
                    ids.tool_run_id,
                    fingerprint,
                    machine.scope(),
                    observed_at,
                )
                .await
        } else {
            self.recovery
                .begin_agent_mutation_attempt(
                    project,
                    run.id(),
                    run.current_snapshot_id(),
                    ids.tool_run_id,
                    fingerprint,
                    mutation_kind,
                    observed_at,
                )
                .await
        }
        .map_err(MutationControllerFailure::MutationStartStore)?;

        // Starting the durable attempt can yield to a settings writer. Revalidate again at
        // the actual tool boundary; an obsolete automatic decision has no file/process effect.
        if decision.reason() == a3_domain::PolicyDecisionReason::SystemAutomatic
            && let Some(store) = self.permissions
        {
            let current = store.load_agent_permissions().await;
            if current.as_ref().ok().copied() != decision.permission_settings() {
                self.recovery
                    .finish_agent_mutation_attempt(
                        project,
                        ids.tool_run_id,
                        attempt.tool_attempt().attempt(),
                        AgentToolAttemptStatus::Failed,
                        AgentMutationDisposition::NotApplied,
                        observed_at,
                    )
                    .await
                    .map_err(MutationControllerFailure::MutationResultStore)?;
                return Err(if current.is_ok() {
                    MutationControllerFailure::PermissionsChanged
                } else {
                    MutationControllerFailure::InvalidPolicyResult
                });
            }
        }

        match prepared {
            PreparedMutation::Machine(machine) => {
                self.execute_machine(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    *machine,
                    &decision,
                    ids,
                    observed_at,
                    context_seed,
                    index_compiler,
                    process_events,
                    control,
                    attempt.tool_attempt().attempt(),
                    &lease,
                )
                .await
            }
            PreparedMutation::Patch(patch) => {
                // These paths scope a later repository observation. Only the returned
                // change set can attribute an actual effect to the agent.
                let mut observed_paths = Vec::new();
                for operation in patch.operations() {
                    observed_paths.push(operation.primary_path().clone());
                    if let a3_domain::PatchOperation::Move(movement) = operation {
                        observed_paths.push(movement.destination().clone());
                    }
                }
                let expected_action = patch.digest();
                let authorized = crate::AuthorizedPatchAction::new(patch, &decision)?;
                let applied = self
                    .patch_tool
                    .apply(project, published, authorized, control)
                    .await;
                if self.source_guarded_test_step(ledger, step_id).is_some()
                    && let Ok(changes) = &applied
                    && (!changes.complete()
                        || changes.action_digest() != expected_action
                        || changes.policy_decision_id() != decision.id())
                {
                    // The durable attempt remains Unknown for existing reconciliation.
                    return Err(MutationControllerFailure::InvalidToolResult);
                }
                self.finish_patch(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    ids,
                    observed_at,
                    context_seed,
                    index_compiler,
                    control,
                    attempt.tool_attempt().attempt(),
                    applied,
                    observed_paths,
                    &lease,
                )
                .await
            }
            PreparedMutation::Run {
                action,
                command_kind,
                verification_adapter,
                result_spec,
                dependencies,
                ..
            } => {
                let source_guarded_test = verification_adapter
                    == ProcessVerificationAdapter::PythonUnittest
                    && self
                        .source_guarded_test_step(ledger, action.step_id())
                        .is_some();
                let authorized = crate::AuthorizedProcessSpec::new(result_spec, &decision)?;
                let specification_id = authorized.specification().specification_id();
                let proof_deadline = run
                    .created_at()
                    .unix_millis()
                    .checked_add(run.budget().duration_limit().millis())
                    .and_then(|deadline| deadline.checked_sub(observed_at.unix_millis()))
                    .and_then(|remaining| started.checked_add(Duration::from_millis(remaining)));
                let result = self
                    .process_runner
                    .run(project, authorized, control, process_events)
                    .await
                    .and_then(|result| {
                        if source_guarded_test
                            && (result.specification_id() != specification_id
                                || result.policy_decision_id() != decision.id())
                        {
                            return Err(ProcessRunFailure::InvalidResult);
                        }
                        Ok(result)
                    });
                self.finish_process(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    action,
                    command_kind,
                    verification_adapter,
                    dependencies,
                    ids,
                    observed_at,
                    context_seed,
                    index_compiler,
                    control,
                    attempt.tool_attempt().attempt(),
                    result,
                    proof_deadline,
                    source_guarded_test,
                    &lease,
                )
                .await
            }
        }
    }

    fn source_guarded_test_step(
        self,
        ledger: &TaskLedger,
        step_id: TaskStepId,
    ) -> Option<&a3_domain::TaskStep> {
        self.source_failure_goal?;
        ledger.step(step_id).filter(|step| {
            let definition = step.definition();
            let spec = definition.verification_spec();
            !definition.acceptance_criteria().is_empty()
                && matches!(spec.target(), VerificationTarget::Test { .. })
                && spec.python_integer_translation().is_some()
        })
    }

    async fn record_approval_request(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        ledger: &TaskLedger,
        prepared: &PreparedMutation,
        request_id: ApprovalRequestId,
        reason: a3_domain::PolicyDecisionReason,
    ) -> Result<(), MutationControllerFailure> {
        let request = self
            .policy_store
            .load_approval_request(project, request_id)
            .await?
            .ok_or(MutationControllerFailure::InvalidPolicyResult)?;
        let step_id = prepared.step_id();
        let spec = current_step_spec(ledger, step_id)?;
        let context = inspection_context(run, step_id, spec.id(), run.current_snapshot_id());
        match prepared {
            PreparedMutation::Machine(machine) => machine
                .record_approval(self.approval, project, context, &request, reason)
                .map(|()| None)?,
            PreparedMutation::Patch(action) => self
                .approval
                .record_patch_request(project, context, &request, reason, action)
                .map(Some)?,
            PreparedMutation::Run {
                command_kind,
                result_spec,
                ..
            } => self
                .approval
                .record_process_request(
                    project,
                    context,
                    &request,
                    reason,
                    AgentProcessInspectionKind::classify(spec.method(), *command_kind)
                        .ok_or(MutationControllerFailure::InvalidCommandSelection)?,
                    result_spec,
                )
                .map(Some)?,
        };
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn evaluate_and_persist_policy(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        action: &a3_domain::PolicyAction,
        workspace_policy: &WorkspacePolicy,
        approval: Option<&mut ApprovalGrant>,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        approval_expires_at: AgentRunTimestamp,
    ) -> Result<PolicyDecision, MutationControllerFailure> {
        let mut next_run = run.clone();
        let mut next_approval = approval.as_deref().cloned();
        let timing = PolicyEvaluationTiming::new(observed_at, observed_at)
            .map_err(|_| MutationControllerFailure::InvalidTimestamp)?;
        let engine = if let Some(store) = self.permissions {
            EvaluateActionPolicy::new().with_permissions(
                store
                    .load_agent_permissions()
                    .await
                    .map_err(|_| MutationControllerFailure::InvalidPolicyResult)?,
            )
        } else {
            EvaluateActionPolicy::new()
        };
        let evaluation = engine.execute(
            &mut next_run,
            action,
            workspace_policy,
            next_approval.as_mut(),
            PolicyEvaluationContext::new(
                ids.policy_decision_id,
                ids.approval_request_id,
                ids.policy_event_id,
                run.current_snapshot_id(),
                timing,
                approval_expires_at,
            ),
        )?;
        let decision = evaluation.decision().clone();
        PersistPolicyEvaluation::new(self.policy_store)
            .execute(project, run.last_event_sequence(), &next_run, &evaluation)
            .await?;
        *run = next_run;
        if let (Some(target), Some(candidate)) = (approval, next_approval) {
            *target = candidate;
        }
        Ok(decision)
    }

    #[allow(clippy::too_many_arguments)]
    async fn await_approval(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        cancellation_requested: bool,
    ) -> Result<(), MutationControllerFailure> {
        let mut next_run = run.clone();
        let mut next_ledger = ledger.clone();
        let timestamp = ledger_timestamp(observed_at)?;
        next_ledger.await_step_approval(
            step_id,
            run.id(),
            TaskStepBlockingReason::try_from_string(APPROVAL_WAIT_REASON.to_owned())
                .map_err(|_| MutationControllerFailure::InvalidStaticText)?,
            timestamp,
        )?;
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::ApprovalRequired,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            cancellation_requested,
        )?;
        let next_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn resume_after_approval<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<(), MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        let mut next_run = run.clone();
        let mut next_ledger = ledger.clone();
        next_ledger.resume_step_after_approval(
            step_id,
            run.id(),
            ledger_timestamp(observed_at)?,
        )?;
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::ApprovalGranted,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            AgentControllerControl::is_cancelled(control),
        )?;
        let next_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn deny_action<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<(), MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        let mut next_run = run.clone();
        let mut next_ledger = ledger.clone();
        next_ledger.block_step(
            step_id,
            run.id(),
            TaskStepBlockingReason::try_from_string(
                "central policy denied the mutation".to_owned(),
            )
            .map_err(|_| MutationControllerFailure::InvalidStaticText)?,
            ledger_timestamp(observed_at)?,
        )?;
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::FatalFailure,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            control.is_cancelled(),
        )?;
        let next_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_patch<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
        attempt: a3_domain::AgentToolAttemptNumber,
        applied: Result<PatchChangeSet, PatchApplyFailure>,
        observed_paths: Vec<RepositoryPath>,
        lease: &crate::WorktreeMutationLease<'_>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + RepositoryIndexControl
            + WorkspacePatchControl,
    {
        let observe_conflict = matches!(
            &applied,
            Err(PatchApplyFailure::Changed(_)
                | PatchApplyFailure::Conflict
                | PatchApplyFailure::StaleSnapshot)
        );
        let (changes, failure, terminal) = match applied {
            Ok(changes) => (Some(changes), None, None),
            Err(PatchApplyFailure::Changed(changes)) => (
                Some(*changes),
                Some(MutationFailureClass::Conflict),
                Some((
                    AgentToolAttemptStatus::Failed,
                    AgentMutationDisposition::Applied,
                )),
            ),
            Err(error) => {
                let failure = map_patch_failure(&error);
                (
                    None,
                    Some(failure),
                    Some((
                        map_patch_attempt_status(&error),
                        AgentMutationDisposition::NotApplied,
                    )),
                )
            }
        };

        if let Some((status, disposition)) = terminal {
            self.recovery
                .finish_agent_mutation_attempt(
                    project,
                    ids.tool_run_id,
                    attempt,
                    status,
                    disposition,
                    observed_at,
                )
                .await
                .map_err(MutationControllerFailure::MutationResultStore)?;
        }

        let refresh_paths = if observe_conflict {
            // The conflicting file may have changed outside this action. Include its
            // actual bytes in the current index before compiling another context.
            Some(observed_paths)
        } else {
            changes.as_ref().map(PatchChangeSet::changed_paths)
        };
        let current_index = if let Some(paths) = refresh_paths {
            let batch = RepositoryChangeBatch::incremental(paths)?;
            match self
                .refresh
                .execute(project, &batch, index_compiler, control)
                .await
            {
                Ok(refresh)
                    if changes.as_ref().is_none_or(|changes| {
                        refresh.snapshot().id() != changes.base_snapshot_id()
                    }) =>
                {
                    Some(refresh.published_index().clone())
                }
                Ok(_) | Err(_) => {
                    if terminal.is_none() {
                        self.recovery
                            .finish_agent_mutation_attempt(
                                project,
                                ids.tool_run_id,
                                attempt,
                                AgentToolAttemptStatus::Failed,
                                AgentMutationDisposition::Applied,
                                observed_at,
                            )
                            .await
                            .map_err(MutationControllerFailure::MutationResultStore)?;
                    }
                    self.stop_without_fresh_index(
                        project,
                        run,
                        ids.tool_event_id,
                        observed_at,
                        control,
                    )
                    .await?;
                    let _ = lease.record_failure(MutationFailureClass::IndexRefreshFailed);
                    return Ok(MutationControllerOutcome::Stopped {
                        failure: MutationFailureClass::IndexRefreshFailed,
                        snapshot_id: run.current_snapshot_id(),
                    });
                }
            }
        } else {
            None
        };

        let snapshot_id = current_index
            .as_ref()
            .map_or(run.current_snapshot_id(), |index| index.run().snapshot_id());
        if failure.is_some() {
            self.record_tool_event(
                project,
                run,
                ids.tool_event_id,
                ids.tool_run_id,
                snapshot_id,
                false,
                None,
                observed_at,
            )
            .await?;
        } else {
            let result = changes
                .as_ref()
                .ok_or(MutationControllerFailure::InvalidToolResult)?;
            self.record_successful_mutation_event(
                project,
                run,
                ids.tool_event_id,
                ids.tool_run_id,
                attempt,
                snapshot_id,
                None,
                patch_result_record(result, snapshot_id),
                observed_at,
            )
            .await?;
        }

        if let Some(failure) = failure {
            return self
                .resolve_unverified_failure(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    ids,
                    observed_at,
                    context_seed,
                    control,
                    lease,
                    failure,
                    false,
                )
                .await;
        }

        let changes = changes.ok_or(MutationControllerFailure::InvalidToolResult)?;
        let current_index = current_index.ok_or(MutationControllerFailure::InvalidToolResult)?;
        let spec = current_step_spec(ledger, step_id)?;
        if let Some(step) = self.source_guarded_test_step(ledger, step_id) {
            let command = crate::RequestAgentFinish
                .verification_command(step)
                .ok_or(MutationControllerFailure::InvalidToolResult)?;
            lease.record_success();
            return Ok(MutationControllerOutcome::BoundVerificationReady {
                command,
                snapshot_id,
            });
        }
        if spec.method() != VerificationMethod::DiffInvariant {
            lease.record_success();
            return self
                .request_next_execution(
                    project,
                    run,
                    ledger,
                    step_id,
                    ids,
                    observed_at,
                    context_seed,
                    control,
                )
                .await;
        }
        let dependencies = VerificationDependencies::from_patch_change_set(&changes)
            .map_err(|_| MutationControllerFailure::InvalidToolResult)?;
        let evidence = VerificationEvidence::Diff(
            a3_domain::DiffEvidence::from_change_set(
                ids.verification_run_id,
                snapshot_id,
                dependencies,
                &changes,
            )
            .map_err(|_| MutationControllerFailure::InvalidToolResult)?,
        );
        self.verify_evidence(
            project,
            run,
            ledger,
            ledger_version,
            step_id,
            ids,
            observed_at,
            context_seed,
            control,
            lease,
            &current_index,
            evidence,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_process<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        action: AgentRunAction,
        command_kind: DiscoveredCommandKind,
        verification_adapter: ProcessVerificationAdapter,
        dependencies: VerificationDependencies,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
        attempt: a3_domain::AgentToolAttemptNumber,
        result: Result<ProcessRunResult, ProcessRunFailure>,
        proof_deadline: Option<Instant>,
        source_guarded_test: bool,
        lease: &crate::WorktreeMutationLease<'_>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl
            + WorkspacePatchControl,
    {
        let before_process_snapshot = run.current_snapshot_id();
        let result = match result {
            Ok(result) => result,
            Err(failure) => {
                let class = map_process_failure(failure);
                let status = map_process_attempt_status(failure);
                let disposition = map_process_failure_disposition(failure);
                self.recovery
                    .finish_agent_mutation_attempt(
                        project,
                        ids.tool_run_id,
                        attempt,
                        status,
                        disposition,
                        observed_at,
                    )
                    .await
                    .map_err(MutationControllerFailure::MutationResultStore)?;
                self.record_tool_event(
                    project,
                    run,
                    ids.tool_event_id,
                    ids.tool_run_id,
                    run.current_snapshot_id(),
                    false,
                    None,
                    observed_at,
                )
                .await?;
                if disposition.requires_reconciliation() {
                    return Ok(MutationControllerOutcome::ReconciliationRequired {
                        tool_run_id: ids.tool_run_id,
                        attempt,
                        snapshot_id: run.current_snapshot_id(),
                    });
                }
                return self
                    .resolve_unverified_failure(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        action.step_id(),
                        ids,
                        observed_at,
                        context_seed,
                        control,
                        lease,
                        class,
                        source_guarded_test,
                    )
                    .await;
            }
        };
        let spec = current_step_spec(ledger, action.step_id())?;
        let inspection_kind = AgentProcessInspectionKind::classify(spec.method(), command_kind)
            .ok_or(MutationControllerFailure::InvalidCommandSelection)?;
        let observed_bytes = result
            .stdout()
            .observed_bytes()
            .saturating_add(result.stderr().observed_bytes());
        let truncated = result.stdout().truncated() || result.stderr().truncated();
        if process_termination_requires_reconciliation(result.termination()) {
            self.observe_process_result(
                project,
                run,
                action.step_id(),
                spec,
                ids.tool_run_id,
                inspection_kind,
                run.current_snapshot_id(),
                &result,
            );
            self.recovery
                .finish_agent_mutation_attempt(
                    project,
                    ids.tool_run_id,
                    attempt,
                    AgentToolAttemptStatus::Cancelled,
                    AgentMutationDisposition::Unknown(MutationReconciliation::Required),
                    observed_at,
                )
                .await
                .map_err(MutationControllerFailure::MutationResultStore)?;
            self.record_tool_event(
                project,
                run,
                ids.tool_event_id,
                ids.tool_run_id,
                run.current_snapshot_id(),
                false,
                Some(RunEventRedaction::new(
                    RunEventRedactionSource::ToolOutput,
                    observed_bytes,
                    truncated,
                )),
                observed_at,
            )
            .await?;
            return Ok(MutationControllerOutcome::ReconciliationRequired {
                tool_run_id: ids.tool_run_id,
                attempt,
                snapshot_id: run.current_snapshot_id(),
            });
        }
        let batch = RepositoryChangeBatch::full_rescan(
            Vec::new(),
            crate::RepositoryRescanReason::Explicit,
        )?;
        let current_index = match self
            .refresh
            .execute(project, &batch, index_compiler, control)
            .await
        {
            Ok(refresh) => refresh.published_index().clone(),
            Err(_) => {
                self.observe_process_result(
                    project,
                    run,
                    action.step_id(),
                    spec,
                    ids.tool_run_id,
                    inspection_kind,
                    run.current_snapshot_id(),
                    &result,
                );
                self.recovery
                    .finish_agent_mutation_attempt(
                        project,
                        ids.tool_run_id,
                        attempt,
                        AgentToolAttemptStatus::Failed,
                        AgentMutationDisposition::Applied,
                        observed_at,
                    )
                    .await
                    .map_err(MutationControllerFailure::MutationResultStore)?;
                self.stop_without_fresh_index(
                    project,
                    run,
                    ids.tool_event_id,
                    observed_at,
                    control,
                )
                .await?;
                let _ = lease.record_failure(MutationFailureClass::IndexRefreshFailed);
                return Ok(MutationControllerOutcome::Stopped {
                    failure: MutationFailureClass::IndexRefreshFailed,
                    snapshot_id: run.current_snapshot_id(),
                });
            }
        };
        let snapshot_id = current_index.run().snapshot_id();
        self.observe_process_result(
            project,
            run,
            action.step_id(),
            spec,
            ids.tool_run_id,
            inspection_kind,
            snapshot_id,
            &result,
        );
        if result.termination() == ProcessTermination::TimedOut {
            self.recovery
                .finish_agent_mutation_attempt(
                    project,
                    ids.tool_run_id,
                    attempt,
                    AgentToolAttemptStatus::Failed,
                    AgentMutationDisposition::Applied,
                    observed_at,
                )
                .await
                .map_err(MutationControllerFailure::MutationResultStore)?;
            self.record_tool_event(
                project,
                run,
                ids.tool_event_id,
                ids.tool_run_id,
                snapshot_id,
                false,
                Some(RunEventRedaction::new(
                    RunEventRedactionSource::ToolOutput,
                    observed_bytes,
                    truncated,
                )),
                observed_at,
            )
            .await?;
            return self
                .resolve_unverified_failure(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    action.step_id(),
                    ids,
                    observed_at,
                    context_seed,
                    control,
                    lease,
                    MutationFailureClass::TimedOut,
                    source_guarded_test,
                )
                .await;
        }
        self.record_successful_mutation_event(
            project,
            run,
            ids.tool_event_id,
            ids.tool_run_id,
            attempt,
            snapshot_id,
            Some(RunEventRedaction::new(
                RunEventRedactionSource::ToolOutput,
                observed_bytes,
                truncated,
            )),
            process_result_record(&result),
            observed_at,
        )
        .await?;
        if source_guarded_test
            && matches!(result.termination(), ProcessTermination::Exited(exit) if !exit.success())
        {
            return self
                .resolve_source_check_failure(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    action.step_id(),
                    ids,
                    observed_at,
                    control,
                    lease,
                    &result,
                    before_process_snapshot == snapshot_id,
                    proof_deadline,
                )
                .await;
        }
        let dependencies = if before_process_snapshot == snapshot_id {
            python_translation_dependencies(spec, &current_index, dependencies)
        } else {
            dependencies
        };
        let factory_evidence = self
            .evidence_factory
            .create(ProcessVerificationEvidenceRequest {
                spec,
                run_id: run.id(),
                tool_run_id: ids.tool_run_id,
                command_id: action.command_id(),
                snapshot_id,
                verification_run_id: ids.verification_run_id,
                dependencies: &dependencies,
                result: &result,
                adapter: verification_adapter,
            })
            .and_then(|evidence| {
                if matches!(&evidence, VerificationEvidence::Test(test) if test.python_integer_translation().is_some()) {
                    return Err(ProcessVerificationEvidenceFailure::InvalidEvidence);
                }
                Ok(evidence)
            });
        let evidence = match factory_evidence {
            Ok(evidence) => evidence,
            Err(_) => {
                return self
                    .resolve_unverified_failure(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        action.step_id(),
                        ids,
                        observed_at,
                        context_seed,
                        control,
                        lease,
                        match result.termination() {
                            ProcessTermination::TimedOut => MutationFailureClass::TimedOut,
                            ProcessTermination::Cancelled => MutationFailureClass::Cancelled,
                            ProcessTermination::Exited(_) => {
                                MutationFailureClass::VerificationFailed
                            }
                        },
                        source_guarded_test,
                    )
                    .await;
            }
        };
        let evidence = if spec.python_integer_translation().is_some() {
            let evidence = if before_process_snapshot == snapshot_id
                && evidence.snapshot_id() == snapshot_id
                && evidence.run_id() == run.id()
                && matches!(result.termination(), ProcessTermination::Exited(exit) if exit.success())
            {
                self.attach_python_integer_translation(
                    project,
                    &current_index,
                    spec,
                    evidence,
                    proof_deadline,
                    control,
                )
                .await
            } else {
                evidence
            };
            if AgentControllerControl::is_cancelled(control)
                || WorkspacePatchControl::is_cancelled(control)
            {
                self.transition_without_ledger(
                    project,
                    run,
                    AgentControllerSignal::CancelRequested,
                    ids.verification_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                return Ok(MutationControllerOutcome::Stopped {
                    failure: MutationFailureClass::Cancelled,
                    snapshot_id,
                });
            }
            if proof_deadline.is_none_or(|deadline| Instant::now() >= deadline) {
                self.fail_active_step_and_transition(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    action.step_id(),
                    "Python source verification reached the run deadline",
                    AgentControllerSignal::FatalFailure,
                    ids.verification_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                return Ok(MutationControllerOutcome::Stopped {
                    failure: MutationFailureClass::TimedOut,
                    snapshot_id,
                });
            }
            evidence
        } else {
            evidence
        };
        self.verify_evidence(
            project,
            run,
            ledger,
            ledger_version,
            action.step_id(),
            ids,
            observed_at,
            context_seed,
            control,
            lease,
            &current_index,
            evidence,
        )
        .await
    }

    async fn attach_python_integer_translation<C>(
        self,
        project: &ProjectIdentity,
        current: &PublishedIndex,
        spec: &VerificationSpec,
        evidence: VerificationEvidence,
        deadline: Option<Instant>,
        owner: &C,
    ) -> VerificationEvidence
    where
        C: AgentControllerControl + WorkspacePatchControl,
    {
        let (Some(verifier), Some(required), Some(deadline)) = (
            self.python_integer_translation_verifier,
            spec.python_integer_translation(),
            deadline,
        ) else {
            return evidence;
        };
        let VerificationTarget::Test {
            command_id,
            selector,
            minimum_cases,
            scope,
        } = spec.target()
        else {
            return evidence;
        };
        let process_only = VerificationSpec::test(
            spec.id(),
            spec.requirement().clone(),
            *command_id,
            selector.clone(),
            *minimum_cases,
            *scope,
        );
        if VerificationEvidenceEvaluation::evaluate(&process_only, &evidence)
            != VerificationEvidenceEvaluation::Passed
        {
            return evidence;
        }
        let control = PythonTranslationProofControl { owner, deadline };
        if WorkspacePatchControl::is_cancelled(&control) {
            return evidence;
        }
        let Some(revision) = current
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| revision.path() == required.path())
        else {
            return evidence;
        };
        let VerificationEvidence::Test(test) = &evidence else {
            return evidence;
        };
        if !test
            .command()
            .dependencies()
            .as_slice()
            .contains(&EvidenceDependency::Present(revision.clone()))
        {
            return evidence;
        }
        let Ok(source) = self
            .patch_tool
            .read_update_base(project, current, revision, &control)
            .await
        else {
            return evidence;
        };
        if source.as_bytes().len() > MAX_PYTHON_TRANSLATION_SOURCE_BYTES
            || source.content_hash() != revision.content_hash()
            || WorkspacePatchControl::is_cancelled(&control)
        {
            return evidence;
        }
        let Ok(Some(witness)) = verifier.prove(required, revision, source.as_bytes(), &control)
        else {
            return evidence;
        };
        if witness.requirement() != required
            || witness.revision() != revision
            || witness.declaration().end_byte() as usize > source.as_bytes().len()
            || WorkspacePatchControl::is_cancelled(&control)
        {
            return evidence;
        }
        match evidence {
            VerificationEvidence::Test(test) => {
                VerificationEvidence::Test(test.with_python_integer_translation(witness))
            }
            _ => evidence,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn observe_process_result(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        step_id: TaskStepId,
        spec: &VerificationSpec,
        tool_run_id: ToolRunId,
        kind: AgentProcessInspectionKind,
        snapshot_id: SnapshotId,
        result: &ProcessRunResult,
    ) {
        let context = inspection_context(run, step_id, spec.id(), snapshot_id);
        let _recorded =
            self.inspection
                .record_process_result(project, context, tool_run_id, kind, result);
    }

    #[allow(clippy::too_many_arguments)]
    async fn verify_evidence<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        control: &C,
        lease: &crate::WorktreeMutationLease<'_>,
        current_index: &PublishedIndex,
        evidence: VerificationEvidence,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        // HTTP observations are committed atomically with their ToolAction before this CAS.
        if !matches!(&evidence, VerificationEvidence::LoopbackHttp(_)) {
            self.evidence_store
                .append_verification_evidence(
                    project,
                    &evidence,
                    VERIFICATION_EVIDENCE_TIMEOUT,
                    control,
                )
                .await?;
        }
        let step = ledger
            .step(step_id)
            .ok_or(MutationControllerFailure::AnchorMismatch)?;
        let definition = step.definition();
        let spec = definition.verification_spec();
        let loopback = spec.method() == VerificationMethod::LoopbackHttp;
        let cancelled = AgentControllerControl::is_cancelled(control);
        if loopback && cancelled {
            return self
                .cancel_loopback_step(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await;
        }
        let verification = EvaluateStepVerification.execute_for_step(
            ids.step_verification_id,
            definition,
            run.id(),
            &evidence,
            current_index,
            ledger_timestamp(observed_at)?,
        )?;
        let passed = verification.passed();
        let missing_source_proof = self.source_failure_goal.is_some()
            && spec.python_integer_translation().is_some()
            && matches!(
                &evidence,
                VerificationEvidence::Test(test) if test.python_integer_translation().is_none()
            );
        let mut next_ledger = ledger.clone();
        next_ledger.begin_step_verification(
            step_id,
            run.id(),
            Some(
                TaskStepResultSummary::try_from_string(
                    "typed mutation result prepared for verification".to_owned(),
                )
                .map_err(|_| MutationControllerFailure::InvalidStaticText)?,
            ),
            vec![evidence.id()],
            ledger_timestamp(observed_at)?,
        )?;
        next_ledger.finish_step_verification(step_id, verification)?;
        let mut next_run = run.clone();
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::TurnNeedsVerification,
            ids.verification_transition_event_id,
            run.current_snapshot_id(),
            observed_at,
            cancelled,
        )?;
        let commit = if loopback {
            self.action_store.commit_current_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
        } else {
            self.action_store.commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
        };
        let next_version = commit.await.map_err(|error| {
            if loopback {
                MutationControllerFailure::LoopbackStepCommit(error)
            } else {
                MutationControllerFailure::ActionStore(error)
            }
        })?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;

        if passed {
            lease.record_success();
            return Ok(MutationControllerOutcome::StepVerified {
                evidence_id: evidence.id(),
                snapshot_id: run.current_snapshot_id(),
            });
        }
        let decision = lease.record_failure(MutationFailureClass::VerificationFailed);
        if loopback {
            self.transition_without_ledger(
                project,
                run,
                AgentControllerSignal::VerificationNeedsReplan,
                ids.resolution_transition_event_id,
                observed_at,
                control,
            )
            .await?;
            return Ok(if run.state() == AgentControllerState::Replan {
                MutationControllerOutcome::ReplanRequired {
                    failure: MutationFailureClass::VerificationFailed,
                    snapshot_id: run.current_snapshot_id(),
                }
            } else {
                MutationControllerOutcome::Stopped {
                    failure: MutationFailureClass::VerificationFailed,
                    snapshot_id: run.current_snapshot_id(),
                }
            });
        }
        if missing_source_proof {
            // The real TestEvidence and failed attempt are already durable. There is
            // no supported source correction for this case, so never compile a retry.
            if run.state() == AgentControllerState::Verify {
                self.transition_without_ledger(
                    project,
                    run,
                    AgentControllerSignal::FatalFailure,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
            }
            return Ok(MutationControllerOutcome::Stopped {
                failure: MutationFailureClass::VerificationFailed,
                snapshot_id: run.current_snapshot_id(),
            });
        }
        self.resolve_after_failed_verification(
            project,
            run,
            ledger,
            ledger_version,
            step_id,
            ids,
            observed_at,
            context_seed,
            control,
            decision,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_source_check_failure<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        control: &C,
        lease: &crate::WorktreeMutationLease<'_>,
        result: &ProcessRunResult,
        snapshot_unchanged: bool,
        proof_deadline: Option<Instant>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        let diagnostic = snapshot_unchanged
            .then(|| source_unittest_failure(result))
            .flatten();
        let can_replan = if diagnostic.is_some() {
            // A full bounded page proves this is the first correction on this run. A
            // missing, changed, or larger history grants no correction.
            let limit = RunEventPageLimit::new(256)
                .map_err(|_| MutationControllerFailure::InvalidToolResult)?;
            let history = self
                .journal
                .load_run_events(project, run.id(), None, limit)
                .await
                .map_err(MutationControllerFailure::Journal)?;
            !history.has_more()
                && history.events().last().is_some_and(|event| {
                    event.run_id() == run.id()
                        && event.id() == ids.tool_event_id
                        && event.sequence() == run.last_event_sequence()
                        && event.snapshot_id() == run.current_snapshot_id()
                        && event.kind() == RunEventKind::ToolAction
                        && event.subject() == Some(RunEventSubject::Tool(ids.tool_run_id))
                        && event.payload().outcome() == Some(RunEventOutcome::Succeeded)
                })
                && !history.events().iter().any(is_source_check_failure_event)
                && proof_deadline.is_some_and(|deadline| Instant::now() < deadline)
        } else {
            false
        };
        let reason = diagnostic.map_or_else(
            || "guarded Python unittest exited unsuccessfully without a current complete failure report".to_owned(),
            SourceUnittestFailure::summary,
        );
        let failure = MutationFailureClass::VerificationFailed;
        let _ = lease.record_failure(failure);
        self.fail_active_step_and_transition(
            project,
            run,
            ledger,
            ledger_version,
            step_id,
            &reason,
            AgentControllerSignal::ExecutionVerificationFailed {
                correction_allowed: can_replan,
            },
            ids.resolution_transition_event_id,
            observed_at,
            control,
        )
        .await?;
        let snapshot_id = run.current_snapshot_id();
        Ok(if run.state() == AgentControllerState::Replan {
            MutationControllerOutcome::ReplanRequired {
                failure,
                snapshot_id,
            }
        } else {
            MutationControllerOutcome::Stopped {
                failure,
                snapshot_id,
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_after_failed_verification<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        control: &C,
        decision: MutationProgressDecision,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        match decision {
            MutationProgressDecision::RetryAllowed => {
                let mut next_ledger = ledger.clone();
                next_ledger.start_step(step_id, run.id(), ledger_timestamp(observed_at)?)?;
                let context = self
                    .transition_with_ledger_and_compile(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        next_ledger,
                        step_id,
                        AgentControllerSignal::VerificationNeedsExecution,
                        ids.resolution_transition_event_id,
                        ids.context_event_id,
                        observed_at,
                        context_seed,
                        control,
                    )
                    .await?;
                Ok(MutationControllerOutcome::NextAction(Box::new(context)))
            }
            MutationProgressDecision::ReplanRequired => {
                self.transition_without_ledger(
                    project,
                    run,
                    AgentControllerSignal::VerificationNeedsReplan,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                Ok(MutationControllerOutcome::ReplanRequired {
                    failure: MutationFailureClass::VerificationFailed,
                    snapshot_id: run.current_snapshot_id(),
                })
            }
            MutationProgressDecision::StopRequired => {
                self.transition_without_ledger(
                    project,
                    run,
                    AgentControllerSignal::FatalFailure,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                Ok(MutationControllerOutcome::Stopped {
                    failure: MutationFailureClass::VerificationFailed,
                    snapshot_id: run.current_snapshot_id(),
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_unverified_failure<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        control: &C,
        lease: &crate::WorktreeMutationLease<'_>,
        failure: MutationFailureClass,
        source_guarded_process: bool,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        if source_guarded_process {
            // Unknown dispositions have already returned to reconciliation.
            let reason = match failure {
                MutationFailureClass::TimedOut => "guarded test exceeded its process timeout",
                MutationFailureClass::Cancelled => "guarded test was cancelled before execution",
                MutationFailureClass::VerificationFailed => {
                    "guarded test did not produce usable verification evidence"
                }
                _ => "guarded test could not run",
            };
            let signal = if failure == MutationFailureClass::Cancelled {
                AgentControllerSignal::CancelRequested
            } else {
                AgentControllerSignal::ExecutionVerificationFailed {
                    correction_allowed: false,
                }
            };
            let _ = lease.record_failure(failure);
            self.fail_active_step_and_transition(
                project,
                run,
                ledger,
                ledger_version,
                step_id,
                reason,
                signal,
                ids.resolution_transition_event_id,
                observed_at,
                control,
            )
            .await?;
            return Ok(MutationControllerOutcome::Stopped {
                failure,
                snapshot_id: run.current_snapshot_id(),
            });
        }
        self.transition_without_ledger(
            project,
            run,
            AgentControllerSignal::TurnNeedsVerification,
            ids.verification_transition_event_id,
            observed_at,
            control,
        )
        .await?;
        match lease.record_failure(failure) {
            MutationProgressDecision::RetryAllowed => {
                self.transition_without_ledger(
                    project,
                    run,
                    AgentControllerSignal::VerificationNeedsExecution,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                let context = self
                    .compile_and_record_context(
                        project,
                        run,
                        ledger,
                        step_id,
                        ids.context_event_id,
                        observed_at,
                        context_seed,
                        control,
                    )
                    .await?;
                Ok(MutationControllerOutcome::NextAction(Box::new(context)))
            }
            MutationProgressDecision::ReplanRequired => {
                self.fail_active_step_and_transition(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    MUTATION_FAILURE_REASON,
                    AgentControllerSignal::VerificationNeedsReplan,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                Ok(MutationControllerOutcome::ReplanRequired {
                    failure,
                    snapshot_id: run.current_snapshot_id(),
                })
            }
            MutationProgressDecision::StopRequired => {
                self.fail_active_step_and_transition(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    step_id,
                    MUTATION_FAILURE_REASON,
                    AgentControllerSignal::FatalFailure,
                    ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                Ok(MutationControllerOutcome::Stopped {
                    failure,
                    snapshot_id: run.current_snapshot_id(),
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn request_next_execution<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &TaskLedger,
        step_id: TaskStepId,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        control: &C,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        self.transition_without_ledger(
            project,
            run,
            AgentControllerSignal::TurnNeedsVerification,
            ids.verification_transition_event_id,
            observed_at,
            control,
        )
        .await?;
        self.transition_without_ledger(
            project,
            run,
            AgentControllerSignal::VerificationNeedsExecution,
            ids.resolution_transition_event_id,
            observed_at,
            control,
        )
        .await?;
        let context = self
            .compile_and_record_context(
                project,
                run,
                ledger,
                step_id,
                ids.context_event_id,
                observed_at,
                context_seed,
                control,
            )
            .await?;
        Ok(MutationControllerOutcome::NextAction(Box::new(context)))
    }

    #[allow(clippy::too_many_arguments)]
    async fn transition_with_ledger_and_compile<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        next_ledger: TaskLedger,
        step_id: TaskStepId,
        signal: AgentControllerSignal,
        transition_event_id: RunEventId,
        context_event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        control: &C,
    ) -> Result<crate::CompiledAgentContext, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        let mut next_run = run.clone();
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            signal,
            transition_event_id,
            run.current_snapshot_id(),
            observed_at,
            AgentControllerControl::is_cancelled(control),
        )?;
        let next_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;
        self.compile_and_record_context(
            project,
            run,
            ledger,
            step_id,
            context_event_id,
            observed_at,
            context_seed,
            control,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn fail_active_step_and_transition<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step_id: TaskStepId,
        reason: &str,
        signal: AgentControllerSignal,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<(), MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        let mut next_ledger = ledger.clone();
        next_ledger.fail_step(
            step_id,
            run.id(),
            TaskStepFailureReason::try_from_string(reason.to_owned())
                .map_err(|_| MutationControllerFailure::InvalidStaticText)?,
            ledger_timestamp(observed_at)?,
        )?;
        let mut next_run = run.clone();
        let expected_sequence = run.last_event_sequence();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            signal,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            control.is_cancelled(),
        )?;
        let next_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                expected_sequence,
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *run = next_run;
        *ledger = next_ledger;
        *ledger_version = next_version;
        Ok(())
    }

    async fn transition_without_ledger<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        signal: AgentControllerSignal,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<(), MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            signal,
            event_id,
            run.current_snapshot_id(),
            observed_at,
            control.is_cancelled(),
        )?;
        AppendRunEvent::new(self.journal)
            .execute(project, expected_sequence, &next_run, advance.event())
            .await?;
        *run = next_run;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_successful_mutation_event(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        event_id: RunEventId,
        tool_run_id: ToolRunId,
        attempt: a3_domain::AgentToolAttemptNumber,
        snapshot_id: SnapshotId,
        redaction: Option<RunEventRedaction>,
        result: AgentMutationResultRecord,
        observed_at: AgentRunTimestamp,
    ) -> Result<(), MutationControllerFailure> {
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let event = next_run.record(
            event_id,
            RunEventKind::ToolAction,
            RunEventPayload::new(
                RunEventCode::None,
                Some(RunEventOutcome::Succeeded),
                redaction,
            ),
            snapshot_id,
            Some(RunEventSubject::Tool(tool_run_id)),
            observed_at,
        )?;
        self.recovery
            .complete_agent_mutation_attempt(
                project,
                expected_sequence,
                &next_run,
                &event,
                tool_run_id,
                attempt,
                result,
            )
            .await
            .map_err(MutationControllerFailure::MutationResultStore)?;
        *run = next_run;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn compile_and_record_context<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &TaskLedger,
        step_id: TaskStepId,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        seed: &MutationContextSeed,
        control: &C,
    ) -> Result<crate::CompiledAgentContext, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        if run.state() != AgentControllerState::Execute
            || ledger
                .step(step_id)
                .is_none_or(|step| step.status() != TaskStepStatus::InProgress)
        {
            return Err(MutationControllerFailure::InvalidContextState);
        }
        let input = seed.compile_input(project, ledger, step_id)?;
        let checkpoint = crate::LoadAgentExecutionCheckpoint::new(self.journal, self.recovery)
            .execute(project, run, control)
            .await?;
        let input = match checkpoint {
            Some(checkpoint) => input
                .with_execution_checkpoint(checkpoint)
                .map_err(|_| MutationControllerFailure::StaleCompiledContext)?,
            None => input,
        };
        let compiled = self.context_compiler.compile(&input, control).await?;
        if compiled.snapshot_id() != run.current_snapshot_id()
            || compiled.goal_contract() != run.goal_contract()
            || compiled.ledger_revision() != run.task_ledger_revision()
            || compiled.current_step_id() != step_id
        {
            return Err(MutationControllerFailure::StaleCompiledContext);
        }
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let event = next_run.record(
            event_id,
            RunEventKind::ContextCompiled,
            RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            compiled.snapshot_id(),
            None,
            observed_at,
        )?;
        AppendRunEvent::new(self.journal)
            .execute(project, expected_sequence, &next_run, &event)
            .await?;
        *run = next_run;
        Ok(compiled)
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_tool_event(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        event_id: RunEventId,
        tool_run_id: ToolRunId,
        snapshot_id: SnapshotId,
        succeeded: bool,
        redaction: Option<RunEventRedaction>,
        observed_at: AgentRunTimestamp,
    ) -> Result<(), MutationControllerFailure> {
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let event = next_run.record(
            event_id,
            RunEventKind::ToolAction,
            RunEventPayload::new(
                if succeeded {
                    RunEventCode::None
                } else {
                    RunEventCode::ToolFailure
                },
                Some(if succeeded {
                    RunEventOutcome::Succeeded
                } else {
                    RunEventOutcome::Failed
                }),
                redaction,
            ),
            snapshot_id,
            Some(RunEventSubject::Tool(tool_run_id)),
            observed_at,
        )?;
        AppendRunEvent::new(self.journal)
            .execute(project, expected_sequence, &next_run, &event)
            .await?;
        *run = next_run;
        Ok(())
    }

    async fn stop_without_fresh_index<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<(), MutationControllerFailure>
    where
        C: AgentControllerControl,
    {
        self.transition_without_ledger(
            project,
            run,
            AgentControllerSignal::FatalFailure,
            event_id,
            observed_at,
            control,
        )
        .await
    }
}

fn require_bound_patch_preview(
    action: &PatchAction,
    preview: PatchPreview,
) -> Result<PatchPreview, PatchPreviewFailure> {
    if preview.action_digest() != action.digest() || preview.snapshot_id() != action.snapshot_id() {
        return Err(PatchPreviewFailure::InvalidResult);
    }
    Ok(preview)
}

fn prepare_action(
    project: &ProjectIdentity,
    run: &AgentRun,
    ledger: &TaskLedger,
    published: &PublishedIndex,
    action: AgentAction,
    command: Option<MutationCommandSelection<'_>>,
) -> Result<PreparedMutation, MutationControllerFailure> {
    if !matches!(
        run.state(),
        AgentControllerState::Execute | AgentControllerState::AwaitApproval
    ) || run.current_snapshot_id() != published.run().snapshot_id()
        || run.goal_contract() != ledger.goal_contract()
        || run.task_ledger_revision() != ledger.revision()
    {
        return Err(MutationControllerFailure::AnchorMismatch);
    }
    let prepared = match action {
        AgentAction::ApplyPatch(action) => {
            if action.run_id() != run.id()
                || action.worktree_id() != project.worktree().id()
                || action.snapshot_id() != run.current_snapshot_id()
            {
                return Err(MutationControllerFailure::AnchorMismatch);
            }
            PreparedMutation::Patch(*action)
        }
        AgentAction::Run(action) => {
            if current_step_spec(ledger, action.step_id())?.method()
                == VerificationMethod::LoopbackHttp
            {
                return Err(MutationControllerFailure::InvalidCommandSelection);
            }
            let selection = command.ok_or(MutationControllerFailure::CommandSelectionRequired)?;
            let catalog = selection.catalog;
            let (spec, approval_required) = match selection.authorization {
                MutationCommandAuthorization::Confirmed(confirmation) => (
                    PrepareDiscoveredCommand.execute(
                        catalog,
                        confirmation,
                        run.id(),
                        action.step_id(),
                        action.command_id(),
                    )?,
                    false,
                ),
                MutationCommandAuthorization::ApprovalRequired(prepared) => {
                    ((*prepared).into_process_spec(), true)
                }
            };
            let command = catalog
                .commands()
                .binary_search_by_key(&action.command_id(), |candidate| candidate.id())
                .ok()
                .and_then(|index| catalog.commands().get(index))
                .ok_or(MutationControllerFailure::InvalidCommandSelection)?;
            let mut revisions = BTreeMap::new();
            for evidence in command.evidence() {
                let revision = evidence.revision();
                if revisions
                    .insert(revision.path().clone(), revision.clone())
                    .is_some_and(|previous| previous != *revision)
                {
                    return Err(MutationControllerFailure::InvalidCommandSelection);
                }
            }
            let dependencies = VerificationDependencies::new(
                revisions
                    .into_values()
                    .map(EvidenceDependency::Present)
                    .collect(),
            )
            .map_err(|_| MutationControllerFailure::InvalidCommandSelection)?;
            PreparedMutation::Run {
                action,
                command_kind: command.kind(),
                verification_adapter: verification_adapter_for(command),
                result_spec: spec,
                dependencies,
                approval_required,
                core_owned_check: command.is_core_owned_check(),
            }
        }
        AgentAction::Machine(_)
        | AgentAction::Search(_)
        | AgentAction::Inspect(_)
        | AgentAction::UpdateLedger(_)
        | AgentAction::Finish(_) => return Err(MutationControllerFailure::NotMutatingAction),
    };
    let step_id = prepared.step_id();
    let step = ledger
        .step(step_id)
        .ok_or(MutationControllerFailure::AnchorMismatch)?;
    let expected_status = if run.state() == AgentControllerState::AwaitApproval {
        TaskStepStatus::AwaitingApproval
    } else {
        TaskStepStatus::InProgress
    };
    if step.status() != expected_status
        || step
            .attempts()
            .last()
            .is_none_or(|attempt| attempt.run_id() != run.id())
    {
        return Err(MutationControllerFailure::AnchorMismatch);
    }
    if let PreparedMutation::Patch(action) = &prepared
        && action.verification_spec_id() != step.definition().verification_spec().id()
    {
        return Err(MutationControllerFailure::AnchorMismatch);
    }
    Ok(prepared)
}

fn verification_adapter_for(command: &a3_domain::DiscoveredCommand) -> ProcessVerificationAdapter {
    const PYTHON_UNITTEST_ARGUMENTS: [&str; 4] = ["-B", "-m", "unittest", "discover"];
    let arguments = command
        .arguments()
        .iter()
        .map(|argument| argument.as_str())
        .collect::<Vec<_>>();
    let default_discovery = arguments == PYTHON_UNITTEST_ARGUMENTS;
    let rooted_discovery = arguments.len() == 6
        && arguments[..5] == ["-B", "-m", "unittest", "discover", "-s"]
        && RepositoryPath::try_from_bytes(arguments[5].as_bytes().to_vec()).is_ok();
    if command.kind() == DiscoveredCommandKind::Test
        && command.executable().as_str() == "python"
        && (default_discovery || rooted_discovery)
    {
        ProcessVerificationAdapter::PythonUnittest
    } else {
        ProcessVerificationAdapter::Generic
    }
}

fn python_translation_dependencies(
    spec: &VerificationSpec,
    current: &PublishedIndex,
    dependencies: VerificationDependencies,
) -> VerificationDependencies {
    let Some(required) = spec.python_integer_translation() else {
        return dependencies;
    };
    let Some(revision) = current
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path() == required.path())
    else {
        return dependencies;
    };
    let source = EvidenceDependency::Present(revision.clone());
    if dependencies.as_slice().contains(&source) {
        return dependencies;
    }
    let mut entries = dependencies.as_slice().to_vec();
    entries.push(source);
    VerificationDependencies::new(entries).unwrap_or(dependencies)
}

fn current_step_spec(
    ledger: &TaskLedger,
    step_id: TaskStepId,
) -> Result<&VerificationSpec, MutationControllerFailure> {
    ledger
        .step(step_id)
        .map(|step| step.definition().verification_spec())
        .ok_or(MutationControllerFailure::AnchorMismatch)
}

const fn inspection_context(
    run: &AgentRun,
    step_id: TaskStepId,
    verification_spec_id: a3_domain::VerificationSpecId,
    snapshot_id: SnapshotId,
) -> AgentInspectionContext {
    AgentInspectionContext::new(
        run.goal_contract().task_id(),
        run.id(),
        step_id,
        verification_spec_id,
        snapshot_id,
    )
}

fn ledger_timestamp(
    timestamp: AgentRunTimestamp,
) -> Result<TaskLedgerTimestamp, MutationControllerFailure> {
    TaskLedgerTimestamp::from_unix_millis(timestamp.unix_millis())
        .map_err(|_| MutationControllerFailure::InvalidTimestamp)
}

fn patch_result_record(
    changes: &PatchChangeSet,
    snapshot_id: SnapshotId,
) -> AgentMutationResultRecord {
    let mut digest = blake3::Hasher::new();
    digest.update(b"a3.mutation.patch-result.v1\0");
    digest.update(&changes.action_digest().as_bytes());
    digest.update(changes.policy_decision_id().as_bytes());
    digest.update(changes.base_snapshot_id().as_bytes());
    digest.update(snapshot_id.as_bytes());
    digest.update(&[u8::from(changes.complete())]);
    let change_count = u64::try_from(changes.changes().len()).map_or(u64::MAX, |value| value);
    digest.update(&change_count.to_le_bytes());
    AgentMutationResultRecord::new(
        ContextToolResultDigest::from_bytes(*digest.finalize().as_bytes()),
        false,
        0,
    )
}

fn process_result_record(result: &ProcessRunResult) -> AgentMutationResultRecord {
    let mut digest = blake3::Hasher::new();
    digest.update(b"a3.mutation.process-result.v1\0");
    digest.update(result.specification_id().as_bytes());
    digest.update(result.policy_decision_id().as_bytes());
    match result.termination() {
        ProcessTermination::Exited(exit) => {
            digest.update(b"exited\0");
            match exit.code() {
                Some(code) => {
                    digest.update(b"code\0");
                    digest.update(&code.to_le_bytes());
                }
                None => {
                    digest.update(b"no-code\0");
                }
            }
        }
        ProcessTermination::TimedOut => {
            digest.update(b"timed-out\0");
        }
        ProcessTermination::Cancelled => {
            digest.update(b"cancelled\0");
        }
    }
    digest.update(&result.duration().as_millis().to_le_bytes());
    hash_process_stream(&mut digest, result.stdout());
    hash_process_stream(&mut digest, result.stderr());
    let observed_output_bytes = result
        .stdout()
        .observed_bytes()
        .saturating_add(result.stderr().observed_bytes());
    AgentMutationResultRecord::new(
        ContextToolResultDigest::from_bytes(*digest.finalize().as_bytes()),
        result.stdout().truncated() || result.stderr().truncated(),
        observed_output_bytes,
    )
}

const fn process_termination_requires_reconciliation(termination: ProcessTermination) -> bool {
    matches!(termination, ProcessTermination::Cancelled)
}

fn hash_process_stream(digest: &mut blake3::Hasher, stream: &a3_domain::ProcessOutputCapture) {
    digest.update(&stream.digest().as_bytes());
    digest.update(&stream.observed_bytes().to_le_bytes());
    digest.update(&stream.retained_limit().to_le_bytes());
    digest.update(&[u8::from(stream.truncated())]);
    digest.update(&[match stream.content().redaction() {
        None => 0,
        Some(ProcessOutputRedaction::InvalidUtf8) => 1,
        Some(ProcessOutputRedaction::SecretCandidate) => 2,
        Some(ProcessOutputRedaction::UnsafeControl) => 3,
    }]);
}

fn map_patch_failure(failure: &PatchApplyFailure) -> MutationFailureClass {
    match failure {
        PatchApplyFailure::Denied => MutationFailureClass::Denied,
        PatchApplyFailure::StaleSnapshot
        | PatchApplyFailure::Conflict
        | PatchApplyFailure::Busy => MutationFailureClass::Conflict,
        PatchApplyFailure::Cancelled => MutationFailureClass::Cancelled,
        PatchApplyFailure::ProgressUnavailable
        | PatchApplyFailure::Unavailable
        | PatchApplyFailure::InvalidResult
        | PatchApplyFailure::Changed(_) => MutationFailureClass::ToolUnavailable,
    }
}

fn map_patch_attempt_status(failure: &PatchApplyFailure) -> AgentToolAttemptStatus {
    match failure {
        PatchApplyFailure::Denied => AgentToolAttemptStatus::Denied,
        PatchApplyFailure::Cancelled => AgentToolAttemptStatus::Cancelled,
        PatchApplyFailure::StaleSnapshot
        | PatchApplyFailure::Conflict
        | PatchApplyFailure::Busy
        | PatchApplyFailure::ProgressUnavailable
        | PatchApplyFailure::Unavailable
        | PatchApplyFailure::InvalidResult
        | PatchApplyFailure::Changed(_) => AgentToolAttemptStatus::Failed,
    }
}

fn map_process_failure(failure: ProcessRunFailure) -> MutationFailureClass {
    match failure {
        ProcessRunFailure::Denied => MutationFailureClass::Denied,
        ProcessRunFailure::Cancelled => MutationFailureClass::Cancelled,
        ProcessRunFailure::SpawnUnavailable
        | ProcessRunFailure::OutputUnavailable
        | ProcessRunFailure::TerminationUnavailable
        | ProcessRunFailure::EventUnavailable
        | ProcessRunFailure::InvalidResult => MutationFailureClass::ToolUnavailable,
    }
}

const fn map_process_attempt_status(failure: ProcessRunFailure) -> AgentToolAttemptStatus {
    match failure {
        ProcessRunFailure::Denied => AgentToolAttemptStatus::Denied,
        ProcessRunFailure::Cancelled => AgentToolAttemptStatus::Cancelled,
        ProcessRunFailure::SpawnUnavailable
        | ProcessRunFailure::OutputUnavailable
        | ProcessRunFailure::TerminationUnavailable
        | ProcessRunFailure::EventUnavailable
        | ProcessRunFailure::InvalidResult => AgentToolAttemptStatus::Failed,
    }
}

const fn map_process_failure_disposition(failure: ProcessRunFailure) -> AgentMutationDisposition {
    match failure {
        ProcessRunFailure::Denied
        | ProcessRunFailure::Cancelled
        | ProcessRunFailure::SpawnUnavailable => AgentMutationDisposition::NotApplied,
        ProcessRunFailure::OutputUnavailable
        | ProcessRunFailure::TerminationUnavailable
        | ProcessRunFailure::EventUnavailable
        | ProcessRunFailure::InvalidResult => {
            AgentMutationDisposition::Unknown(MutationReconciliation::Required)
        }
    }
}

/// Stable failure at a mutating controller boundary.
#[derive(Debug)]
pub enum MutationControllerFailure {
    /// The candidate HTTP verification could not be admitted to its first external effect.
    LoopbackAdmission(crate::LoopbackVerificationFailure),
    /// A previous settled HTTP effect for this active step needs recovery before another launch.
    LoopbackRecoveryPending,
    /// The HTTP effect and its receipt are durable, while the step commit failed.
    LoopbackStepCommit(AgentActionStoreFailure),
    /// A closed machine action could not be prepared before its effect.
    MachinePreparation,
    /// Action was not ApplyPatch or Run.
    NotMutatingAction,
    /// Run, worktree, snapshot, step, verification, or publication anchor differed.
    AnchorMismatch,
    /// A Run action lacked the exact current catalog and confirmation.
    CommandSelectionRequired,
    /// Command catalog, confirmation, evidence, or selected identity was invalid.
    InvalidCommandSelection,
    /// A context seed could not reconstruct a valid authoritative compile input.
    InvalidContextSeed,
    /// Context was requested outside Execute with one InProgress step.
    InvalidContextState,
    /// Context compiler returned an old snapshot or another run anchor.
    StaleCompiledContext,
    /// Static controller-owned bounded text unexpectedly violated a domain invariant.
    InvalidStaticText,
    /// Explicit action timing could not be represented.
    InvalidTimestamp,
    /// Central policy returned an internally inconsistent decision.
    InvalidPolicyResult,
    /// Permission selection changed before automatic tool admission.
    PermissionsChanged,
    /// A trusted tool returned data that violated its typed result contract.
    InvalidToolResult,
    /// Another action owns the worktree mutation boundary.
    Busy(WorktreeMutationBusy),
    /// Action fingerprint construction failed.
    Fingerprint(MutationActionFingerprintError),
    /// Preview failed before policy. An original-bound conflict halts its active step.
    PatchPreview(PatchPreviewFailure),
    /// Required exact pre-approval inspection could not be retained.
    Inspection(AgentInspectionSinkFailure),
    /// Required exact approval presentation could not be retained.
    ApprovalPresentation(AgentApprovalSinkFailure),
    /// Patch authorization did not match the durable central decision.
    PatchAuthorization(PatchAuthorizationError),
    /// Process authorization did not match the durable central decision.
    ProcessAuthorization(ProcessAuthorizationError),
    /// Discovered-command preparation failed.
    Command(a3_domain::DiscoveredCommandProcessError),
    /// Central policy evaluation failed.
    Policy(EvaluateActionPolicyError),
    /// Policy decision or grant lifecycle persistence failed.
    PolicyStore(PolicyStoreFailure),
    /// Journal event or materialized run persistence failed.
    Journal(RunJournalStoreFailure),
    /// Atomic Task Ledger plus run persistence failed.
    ActionStore(AgentActionStoreFailure),
    /// Persistence failed before the mutation adapter could be invoked.
    MutationStartStore(AgentRecoveryStoreFailure),
    /// Persistence failed after a mutation boundary could have produced an effect.
    MutationResultStore(AgentRecoveryStoreFailure),
    /// Index refresh input was invalid.
    ChangeBatch(RepositoryChangeBatchError),
    /// Repository index refresh failed before current context could continue.
    Index(RefreshRepositoryIndexError),
    /// Evidence persistence failed.
    EvidenceStore(VerificationEvidenceStoreFailure),
    /// Typed evidence could not be evaluated.
    Verification(EvaluateStepVerificationError),
    /// Task Ledger invariant rejected a transition.
    Ledger(a3_domain::TaskLedgerError),
    /// Finite controller rejected a transition.
    Controller(AgentControllerError),
    /// Context compilation failed.
    Context(ContextCompileFailure),
    /// Durable execution receipt could not be reconstructed safely.
    ExecutionCheckpoint(crate::AgentExecutionCheckpointError),
    /// Run event construction failed.
    Run(a3_domain::AgentRunError),
}

impl fmt::Display for MutationControllerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LoopbackAdmission(_) => "loopback verification was not admitted",
            Self::LoopbackRecoveryPending => "settled loopback verification requires recovery",
            Self::LoopbackStepCommit(_) => "settled loopback verification step commit failed",
            Self::MachinePreparation => "machine action cannot be prepared safely",
            Self::NotMutatingAction => "controller action is not a mutation",
            Self::AnchorMismatch => "mutation action does not match current controller anchors",
            Self::CommandSelectionRequired => "run action requires current command selection",
            Self::InvalidCommandSelection => "run action command selection is invalid",
            Self::InvalidContextSeed => "post-mutation context seed is invalid",
            Self::InvalidContextState => "post-mutation context state is invalid",
            Self::StaleCompiledContext => "post-mutation context is stale",
            Self::InvalidStaticText => "controller-owned mutation text is invalid",
            Self::InvalidTimestamp => "mutation timestamp is invalid",
            Self::InvalidPolicyResult => "mutation policy result is invalid",
            Self::PermissionsChanged => "agent permissions changed before tool admission",
            Self::InvalidToolResult => "mutation tool result is invalid",
            Self::Busy(_) => "worktree mutation boundary is busy",
            Self::Fingerprint(_) => "mutation action fingerprint is invalid",
            Self::PatchPreview(_) => "mutation patch preview failed",
            Self::Inspection(_) => "mutation inspection failed",
            Self::ApprovalPresentation(_) => "mutation approval presentation failed",
            Self::PatchAuthorization(_) => "mutation patch authorization failed",
            Self::ProcessAuthorization(_) => "mutation process authorization failed",
            Self::Command(_) => "mutation command preparation failed",
            Self::Policy(_) => "mutation policy evaluation failed",
            Self::PolicyStore(_) => "mutation policy persistence failed",
            Self::Journal(_) => "mutation journal persistence failed",
            Self::ActionStore(_) => "mutation Ledger persistence failed",
            Self::MutationStartStore(_) => "mutation start persistence failed",
            Self::MutationResultStore(_) => "mutation result persistence failed",
            Self::ChangeBatch(_) => "mutation changed-path batch is invalid",
            Self::Index(_) => "mutation index refresh failed",
            Self::EvidenceStore(_) => "mutation verification evidence persistence failed",
            Self::Verification(_) => "mutation verification evaluation failed",
            Self::Ledger(_) => "mutation Task Ledger transition failed",
            Self::Controller(_) => "mutation controller transition failed",
            Self::Context(_) => "post-mutation context compilation failed",
            Self::ExecutionCheckpoint(_) => "post-mutation execution checkpoint failed",
            Self::Run(_) => "mutation run event failed",
        })
    }
}

impl MutationControllerFailure {
    /// Returns the conservative application state at the point orchestration stopped.
    #[must_use]
    pub const fn mutation_application_state(&self) -> MutationApplicationState {
        match self {
            Self::LoopbackRecoveryPending | Self::LoopbackStepCommit(_) => {
                MutationApplicationState::Applied
            }
            Self::MutationStartStore(AgentRecoveryStoreFailure::MutationReconciliationRequired)
            | Self::MutationResultStore(_)
            | Self::InvalidContextState
            | Self::StaleCompiledContext
            | Self::InvalidToolResult
            | Self::ChangeBatch(_)
            | Self::Index(_)
            | Self::EvidenceStore(_)
            | Self::Verification(_)
            | Self::Ledger(_)
            | Self::Controller(_)
            | Self::Context(_)
            | Self::ExecutionCheckpoint(_)
            | Self::Run(_) => MutationApplicationState::Unknown,
            Self::LoopbackAdmission(_)
            | Self::MachinePreparation
            | Self::NotMutatingAction
            | Self::AnchorMismatch
            | Self::CommandSelectionRequired
            | Self::InvalidCommandSelection
            | Self::InvalidContextSeed
            | Self::InvalidStaticText
            | Self::InvalidTimestamp
            | Self::InvalidPolicyResult
            | Self::PermissionsChanged
            | Self::Busy(_)
            | Self::Fingerprint(_)
            | Self::PatchPreview(_)
            | Self::Inspection(_)
            | Self::ApprovalPresentation(_)
            | Self::PatchAuthorization(_)
            | Self::ProcessAuthorization(_)
            | Self::Command(_)
            | Self::Policy(_)
            | Self::PolicyStore(_)
            | Self::Journal(_)
            | Self::ActionStore(_)
            | Self::MutationStartStore(_) => MutationApplicationState::NotApplied,
        }
    }
}

impl Error for MutationControllerFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LoopbackAdmission(error) => Some(error),
            Self::LoopbackStepCommit(error) => Some(error),
            Self::Busy(error) => Some(error),
            Self::Fingerprint(error) => Some(error),
            Self::PatchPreview(error) => Some(error),
            Self::Inspection(error) => Some(error),
            Self::ApprovalPresentation(error) => Some(error),
            Self::PatchAuthorization(error) => Some(error),
            Self::ProcessAuthorization(error) => Some(error),
            Self::Command(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::PolicyStore(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::ActionStore(error) => Some(error),
            Self::MutationStartStore(error) | Self::MutationResultStore(error) => Some(error),
            Self::ChangeBatch(error) => Some(error),
            Self::Index(error) => Some(error),
            Self::EvidenceStore(error) => Some(error),
            Self::Verification(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::Controller(error) => Some(error),
            Self::Context(error) => Some(error),
            Self::ExecutionCheckpoint(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::MachinePreparation
            | Self::NotMutatingAction
            | Self::AnchorMismatch
            | Self::CommandSelectionRequired
            | Self::InvalidCommandSelection
            | Self::InvalidContextSeed
            | Self::InvalidContextState
            | Self::StaleCompiledContext
            | Self::InvalidStaticText
            | Self::InvalidTimestamp
            | Self::InvalidPolicyResult
            | Self::PermissionsChanged
            | Self::InvalidToolResult
            | Self::LoopbackRecoveryPending => None,
        }
    }
}

macro_rules! failure_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for MutationControllerFailure {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

failure_from!(WorktreeMutationBusy, Busy);
failure_from!(MutationActionFingerprintError, Fingerprint);
failure_from!(PatchPreviewFailure, PatchPreview);
failure_from!(AgentInspectionSinkFailure, Inspection);
failure_from!(AgentApprovalSinkFailure, ApprovalPresentation);
failure_from!(PatchAuthorizationError, PatchAuthorization);
failure_from!(ProcessAuthorizationError, ProcessAuthorization);
failure_from!(a3_domain::DiscoveredCommandProcessError, Command);
failure_from!(EvaluateActionPolicyError, Policy);
failure_from!(PolicyStoreFailure, PolicyStore);
failure_from!(RunJournalStoreFailure, Journal);
failure_from!(AgentActionStoreFailure, ActionStore);
failure_from!(RepositoryChangeBatchError, ChangeBatch);
failure_from!(RefreshRepositoryIndexError, Index);
failure_from!(VerificationEvidenceStoreFailure, EvidenceStore);
failure_from!(EvaluateStepVerificationError, Verification);
failure_from!(a3_domain::TaskLedgerError, Ledger);
failure_from!(AgentControllerError, Controller);
failure_from!(ContextCompileFailure, Context);
failure_from!(crate::AgentExecutionCheckpointError, ExecutionCheckpoint);
failure_from!(a3_domain::AgentRunError, Run);

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentRunId, MinimumTestCaseCount, PolicyResourceId, ProcessDuration, ProcessExit,
        ProcessOutputCapture, ProcessOutputContent, ProcessOutputDigest, ProcessStream,
        TestCaseSelector, VerificationRequirement, VerificationScope, VerificationSpecId,
    };
    use std::error::Error;

    #[test]
    fn patch_preview_requires_the_actual_action_and_base_snapshot() -> Result<(), Box<dyn Error>> {
        use a3_domain::{
            PatchActionSchemaVersion, PatchAdd, PatchContentPreview, PatchFileContent,
            PatchOperation, PatchPreviewEntry, PatchRationale, WorktreeId,
        };

        let make_preview = |snapshot, bytes: &[u8]| -> Result<_, Box<dyn Error>> {
            let path = RepositoryPath::try_from_bytes(b"fixture.txt".to_vec())?;
            let content = PatchFileContent::try_from_bytes(bytes.to_vec())?;
            let after = PatchContentPreview::from_content(&content, bytes.len())?;
            let action = PatchAction::new(
                PatchActionSchemaVersion::V1,
                AgentRunId::from_bytes([1; 32]),
                WorktreeId::from_bytes([2; 32]),
                snapshot,
                TaskStepId::from_bytes([3; 32]),
                VerificationSpecId::from_bytes([4; 32]),
                PatchRationale::try_from_string("show exact bytes".to_owned())?,
                vec![PatchOperation::Add(PatchAdd::new(path.clone(), content))],
            )?;
            let preview = PatchPreview::new(
                &action,
                vec![PatchPreviewEntry::new(None, Some(path), None, Some(after))],
            )?;
            Ok((action, preview))
        };
        let snapshot = SnapshotId::from_bytes([5; 32]);
        let (action, matching) = make_preview(snapshot, b"approved proposal\n")?;
        let (_, other_action) = make_preview(snapshot, b"different proposal\n")?;
        let (_, other_snapshot) =
            make_preview(SnapshotId::from_bytes([6; 32]), b"approved proposal\n")?;

        assert_eq!(
            require_bound_patch_preview(&action, matching.clone())?,
            matching
        );
        assert_eq!(
            require_bound_patch_preview(&action, other_action),
            Err(PatchPreviewFailure::InvalidResult)
        );
        assert_eq!(
            require_bound_patch_preview(&action, other_snapshot),
            Err(PatchPreviewFailure::InvalidResult)
        );
        Ok(())
    }

    #[test]
    fn a_clean_timeout_can_reindex_and_replan_but_cancellation_requires_reconciliation() {
        assert!(!process_termination_requires_reconciliation(
            ProcessTermination::TimedOut
        ));
        assert!(process_termination_requires_reconciliation(
            ProcessTermination::Cancelled
        ));
    }

    #[test]
    fn builtin_python_unittest_adapter_emits_bounded_structured_cases() -> Result<(), Box<dyn Error>>
    {
        let command_id = DiscoveredCommandId::from_bytes([1; 32]);
        let spec = VerificationSpec::test(
            VerificationSpecId::from_bytes([2; 32]),
            VerificationRequirement::try_from_string("all tests pass".to_owned())?,
            command_id,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Workspace,
        );
        let result = process_result(
            "",
            "..\n----------------------------------------------------------------------\nRan 2 tests in 0.001s\n\nOK\n",
        )?;
        let dependencies = VerificationDependencies::new(Vec::new())?;
        let evidence = BuiltinProcessVerificationEvidenceFactory.create(
            ProcessVerificationEvidenceRequest {
                spec: &spec,
                run_id: AgentRunId::from_bytes([3; 32]),
                tool_run_id: ToolRunId::from_bytes([4; 32]),
                command_id,
                snapshot_id: SnapshotId::from_bytes([5; 32]),
                verification_run_id: VerificationRunId::from_bytes([6; 32]),
                dependencies: &dependencies,
                result: &result,
                adapter: ProcessVerificationAdapter::PythonUnittest,
            },
        )?;
        let VerificationEvidence::Test(evidence) = evidence else {
            return Err("adapter did not emit TestEvidence".into());
        };
        assert_eq!(evidence.cases().len(), 2);
        assert!(
            evidence
                .cases()
                .iter()
                .all(|case| case.outcome() == TestCaseOutcome::Passed)
        );
        Ok(())
    }

    #[test]
    fn builtin_python_unittest_adapter_rejects_ambiguous_or_incomplete_output()
    -> Result<(), Box<dyn Error>> {
        let command_id = DiscoveredCommandId::from_bytes([11; 32]);
        let spec = VerificationSpec::test(
            VerificationSpecId::from_bytes([12; 32]),
            VerificationRequirement::try_from_string("all tests pass".to_owned())?,
            command_id,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Workspace,
        );
        let dependencies = VerificationDependencies::new(Vec::new())?;
        for stderr in [
            "Ran 2 tests in 0.001s\n",
            "Ran 2 tests in 0.001s\nOK (skipped=1)\n",
            "Ran 2 tests in 0.001s\nRan 2 tests in 0.001s\nOK\n",
        ] {
            let result = process_result("", stderr)?;
            let created = BuiltinProcessVerificationEvidenceFactory.create(
                ProcessVerificationEvidenceRequest {
                    spec: &spec,
                    run_id: AgentRunId::from_bytes([13; 32]),
                    tool_run_id: ToolRunId::from_bytes([14; 32]),
                    command_id,
                    snapshot_id: SnapshotId::from_bytes([15; 32]),
                    verification_run_id: VerificationRunId::from_bytes([16; 32]),
                    dependencies: &dependencies,
                    result: &result,
                    adapter: ProcessVerificationAdapter::PythonUnittest,
                },
            );
            assert_eq!(
                created,
                Err(ProcessVerificationEvidenceFailure::InvalidEvidence)
            );
        }
        Ok(())
    }

    #[test]
    fn source_unittest_failure_normalizes_the_actual_red_report() -> Result<(), Box<dyn Error>> {
        let report = concat!(
            "FAIL: test_known_example (test_behavior.Behavior)\n",
            "AssertionError: 44 != 42\nRan 1 test in 0.001s\nFAILED (failures=1)\n",
        );
        let result = process_result_with_status(
            "AssertionError: 99 != 100\n",
            report,
            ProcessTermination::Exited(ProcessExit::new(Some(1), false)?),
            false,
        )?;
        let failure = source_unittest_failure(&result).ok_or("expected a complete failure")?;
        assert_eq!(failure.cases, 1);
        assert_eq!(failure.failures, 1);
        assert_eq!(failure.assertion_operands, Some((44, 42)));
        assert_eq!(
            failure.summary(),
            "Python unittest failed (1/1). An assertion compared 44 and 42."
        );
        Ok(())
    }

    #[test]
    fn source_unittest_failure_rejects_ambiguous_or_incomplete_red_reports()
    -> Result<(), Box<dyn Error>> {
        let red = ProcessTermination::Exited(ProcessExit::new(Some(1), false)?);
        let report = "FAIL: test_case (Test)\nRan 1 test in 0.001s\nFAILED (failures=1)\n";
        let duplicate = report.repeat(2);
        for (stdout, stderr, status, truncated) in [
            ("", report, red, true),
            (report, "", red, false),
            ("", duplicate.as_str(), red, false),
            ("", report, ProcessTermination::TimedOut, false),
            (
                "",
                report,
                ProcessTermination::Exited(ProcessExit::new(Some(0), true)?),
                false,
            ),
            (
                "",
                report,
                ProcessTermination::Exited(ProcessExit::new(Some(2), false)?),
                false,
            ),
            (
                "",
                "Ran 1 test in 0.001s\nFAILED (failures=1)\n",
                red,
                false,
            ),
            (
                "",
                "ERROR: test_case (Test)\nRan 1 test in 0.001s\nFAILED (errors=1)\n",
                red,
                false,
            ),
            (
                "",
                "FAIL: test_case (Test)\nRan 0 tests in 0.001s\nFAILED (failures=1)\n",
                red,
                false,
            ),
        ] {
            let result = process_result_with_status(stdout, stderr, status, truncated)?;
            assert_eq!(source_unittest_failure(&result), None);
        }
        Ok(())
    }

    fn process_result(stdout: &str, stderr: &str) -> Result<ProcessRunResult, Box<dyn Error>> {
        process_result_with_status(
            stdout,
            stderr,
            ProcessTermination::Exited(ProcessExit::new(Some(0), true)?),
            false,
        )
    }

    fn process_result_with_status(
        stdout: &str,
        stderr: &str,
        status: ProcessTermination,
        stderr_truncated: bool,
    ) -> Result<ProcessRunResult, Box<dyn Error>> {
        let capture =
            |stream, text: &str, truncated| -> Result<ProcessOutputCapture, Box<dyn Error>> {
                let observed = if truncated {
                    1_025
                } else {
                    u64::try_from(text.len())?
                };
                Ok(ProcessOutputCapture::new(
                    stream,
                    ProcessOutputContent::text(text.to_owned())?,
                    observed,
                    1_024,
                    truncated,
                    ProcessOutputDigest::from_bytes([17; 32]),
                )?)
            };
        Ok(ProcessRunResult::new(
            PolicyResourceId::from_bytes([18; 32]),
            PolicyDecisionId::from_bytes([19; 32]),
            status,
            ProcessDuration::from_millis(1),
            capture(ProcessStream::Stdout, stdout, false)?,
            capture(ProcessStream::Stderr, stderr, stderr_truncated)?,
        )?)
    }
}
