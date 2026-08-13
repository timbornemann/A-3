use crate::{
    AdvanceAgentController, AgentActionStore, AgentActionStoreFailure, AgentContextCompileInput,
    AgentContextCompiler, AgentControllerControl, AgentControllerError, AgentControllerSignal,
    AgentInspectionContext, AgentInspectionSink, AgentInspectionSinkFailure,
    AgentMutationResultRecord, AgentProcessInspectionKind, AgentRecoveryStore,
    AgentRecoveryStoreFailure, AppendRunEvent, ContextCompileControl, ContextCompileFailure,
    ContextToolResult, ContextToolResultDigest, EvaluateActionPolicy, EvaluateActionPolicyError,
    EvaluateStepVerification, EvaluateStepVerificationError, MutationActionFingerprint,
    MutationActionFingerprintError, MutationFailureClass, MutationProgressDecision,
    PatchApplyFailure, PatchAuthorizationError, PatchPreviewFailure, PersistPolicyEvaluation,
    PolicyEvaluationContext, PolicyStore, PolicyStoreFailure, PrepareDiscoveredCommand,
    ProcessAuthorizationError, ProcessEventSink, ProcessRunControl, ProcessRunFailure,
    ProcessRunner, RefreshRepositoryIndex, RefreshRepositoryIndexError, RepositoryChangeBatch,
    RepositoryChangeBatchError, RepositoryIndexCompiler, RepositoryIndexControl, RunJournalStore,
    RunJournalStoreFailure, StoredProjectCommandAllowlist, TaskLedgerStoreVersion,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure, WorkspacePatchControl,
    WorkspacePatchTool, WorktreeMutationBusy, WorktreeMutationCoordinator,
};
use a3_domain::{
    AgentAction, AgentControllerState, AgentMutationDisposition, AgentMutationKind, AgentRun,
    AgentRunAction, AgentRunTimestamp, AgentToolAttemptStatus, ApprovalGrant, ApprovalRequestId,
    CommandEvidence, CommandEvidenceContext, DiscoveredCommandId, DiscoveredCommandKind,
    EvidenceDependency, GoalContract, ModelProfile, MutationApplicationState,
    MutationReconciliation, PatchAction, PatchChangeSet, PolicyDecision, PolicyDecisionId,
    PolicyDecisionOutcome, PolicyEvaluationTiming, ProcessOutputRedaction, ProcessRunResult,
    ProcessTermination, ProjectCommandCatalog, ProjectIdentity, PublishedIndex, RunEventCode,
    RunEventId, RunEventKind, RunEventOutcome, RunEventPayload, RunEventRedaction,
    RunEventRedactionSource, RunEventSubject, SnapshotId, StepVerificationId, TaskEvidenceId,
    TaskLedger, TaskLedgerTimestamp, TaskLensSeed, TaskStepBlockingReason, TaskStepFailureReason,
    TaskStepId, TaskStepResultSummary, TaskStepStatus, ToolRunId, VerificationDependencies,
    VerificationEvidence, VerificationMethod, VerificationRunId, VerificationSpec,
    VerificationTarget, WorkspacePolicy,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;

const VERIFICATION_EVIDENCE_TIMEOUT: Duration = Duration::from_secs(30);
const APPROVAL_WAIT_REASON: &str = "exact mutation approval is required";
const MUTATION_FAILURE_REASON: &str = "repeated identical mutation action failed";

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
    supplemental_seeds: Vec<TaskLensSeed>,
    tool_results: Vec<ContextToolResult>,
}

impl MutationContextSeed {
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
        .map_err(|_| MutationControllerFailure::InvalidContextSeed)
    }
}

/// Current E5 evidence and exact stored confirmation needed to resolve one `run` action.
#[derive(Debug, Clone, Copy)]
pub struct MutationCommandSelection<'a> {
    catalog: &'a ProjectCommandCatalog,
    confirmation: &'a StoredProjectCommandAllowlist,
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
            confirmation,
        }
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
            | VerificationTarget::Legacy(_) => {
                Err(ProcessVerificationEvidenceFailure::EvidenceKindMismatch)
            }
        }
    }
}

/// Completed controller response; no variant allows the model to assert verification or success.
pub enum MutationControllerOutcome {
    /// Exact policy scope is durable and the controller is waiting for the named request.
    AwaitingApproval(ApprovalRequestId),
    /// Policy denied the mutation and the finite run stopped.
    Denied,
    /// The action succeeded but verification requires another freshly compiled Execute turn.
    NextAction(Box<crate::CompiledAgentContext>),
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
            Self::NextAction(context) => formatter
                .debug_struct("NextAction")
                .field("context_digest", &context.digest())
                .field("snapshot_id", &context.snapshot_id())
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
    Patch(PatchAction),
    Run {
        action: AgentRunAction,
        command_kind: DiscoveredCommandKind,
        result_spec: a3_domain::ProcessSpec,
        dependencies: VerificationDependencies,
    },
}

impl PreparedMutation {
    fn step_id(&self) -> TaskStepId {
        match self {
            Self::Patch(action) => action.task_step_id(),
            Self::Run { action, .. } => action.step_id(),
        }
    }

    fn policy_action(&self) -> a3_domain::PolicyAction {
        match self {
            Self::Patch(action) => action.policy_action(),
            Self::Run { result_spec, .. } => result_spec.policy_action(),
        }
    }

    const fn kind(&self) -> AgentMutationKind {
        match self {
            Self::Patch(_) => AgentMutationKind::Patch,
            Self::Run { .. } => AgentMutationKind::Process,
        }
    }
}

/// E7 application use case composing the existing patch, process, policy, index, verification,
/// context, journal, and ledger boundaries without creating a second controller loop.
#[derive(Debug, Clone, Copy)]
pub struct ExecuteMutatingAgentAction<'a> {
    coordinator: &'a WorktreeMutationCoordinator,
    policy_store: &'a dyn PolicyStore,
    journal: &'a dyn RunJournalStore,
    action_store: &'a dyn AgentActionStore,
    recovery: &'a dyn AgentRecoveryStore,
    evidence_store: &'a dyn VerificationEvidenceStore,
    inspection: &'a dyn AgentInspectionSink,
    patch_tool: &'a dyn WorkspacePatchTool,
    process_runner: &'a dyn ProcessRunner,
    evidence_factory: &'a dyn ProcessVerificationEvidenceFactory,
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
        patch_tool: &'a dyn WorkspacePatchTool,
        process_runner: &'a dyn ProcessRunner,
        evidence_factory: &'a dyn ProcessVerificationEvidenceFactory,
        context_compiler: &'a dyn AgentContextCompiler,
        refresh: &'a RefreshRepositoryIndex,
    ) -> Self {
        Self {
            coordinator,
            policy_store,
            journal,
            action_store,
            recovery,
            evidence_store,
            inspection,
            patch_tool,
            process_runner,
            evidence_factory,
            context_compiler,
            refresh,
        }
    }

    /// Executes at most one patch or discovered command and resolves it through Verify.
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
        let fingerprint = MutationActionFingerprint::from_action(&action)?;
        let prepared = prepare_action(project, run, ledger, published, action, command)?;
        let step_id = prepared.step_id();
        let mutation_kind = prepared.kind();
        let lease = self
            .coordinator
            .try_acquire(run.id(), project.worktree().id(), fingerprint)?;

        let patch_preview = match &prepared {
            PreparedMutation::Patch(patch) => Some(
                self.patch_tool
                    .preview(project, published, patch, control)
                    .await?,
            ),
            PreparedMutation::Run { .. } => None,
        };

        let decision = self
            .evaluate_and_persist_policy(
                project,
                run,
                &prepared.policy_action(),
                workspace_policy,
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
                if run.state() != AgentControllerState::AwaitApproval {
                    self.await_approval(
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

        let attempt = self
            .recovery
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
            .map_err(MutationControllerFailure::MutationStartStore)?;

        match prepared {
            PreparedMutation::Patch(patch) => {
                let authorized = crate::AuthorizedPatchAction::new(patch, &decision)?;
                let applied = self
                    .patch_tool
                    .apply(project, published, authorized, control)
                    .await;
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
                    &lease,
                )
                .await
            }
            PreparedMutation::Run {
                action,
                command_kind,
                result_spec,
                dependencies,
            } => {
                let authorized = crate::AuthorizedProcessSpec::new(result_spec, &decision)?;
                let result = self
                    .process_runner
                    .run(project, authorized, control, process_events)
                    .await;
                self.finish_process(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    action,
                    command_kind,
                    dependencies,
                    ids,
                    observed_at,
                    context_seed,
                    index_compiler,
                    control,
                    attempt.tool_attempt().attempt(),
                    result,
                    &lease,
                )
                .await
            }
        }
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
        let evaluation = EvaluateActionPolicy::new().execute(
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
    async fn await_approval<C>(
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
        lease: &crate::WorktreeMutationLease<'_>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + RepositoryIndexControl
            + WorkspacePatchControl,
    {
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

        let current_index = if let Some(changes) = changes.as_ref() {
            let batch = RepositoryChangeBatch::incremental(changes.changed_paths())?;
            match self
                .refresh
                .execute(project, &batch, index_compiler, control)
                .await
            {
                Ok(refresh) if refresh.snapshot().id() != changes.base_snapshot_id() => {
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
                )
                .await;
        }

        let changes = changes.ok_or(MutationControllerFailure::InvalidToolResult)?;
        let current_index = current_index.ok_or(MutationControllerFailure::InvalidToolResult)?;
        let spec = current_step_spec(ledger, step_id)?;
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
        dependencies: VerificationDependencies,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
        attempt: a3_domain::AgentToolAttemptNumber,
        result: Result<ProcessRunResult, ProcessRunFailure>,
        lease: &crate::WorktreeMutationLease<'_>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl,
    {
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
                    )
                    .await;
            }
        };
        let spec = current_step_spec(ledger, action.step_id())?;
        let inspection_kind = AgentProcessInspectionKind::classify(spec.method(), command_kind);
        let observed_bytes = result
            .stdout()
            .observed_bytes()
            .saturating_add(result.stderr().observed_bytes());
        let truncated = result.stdout().truncated() || result.stderr().truncated();
        if matches!(
            result.termination(),
            ProcessTermination::TimedOut | ProcessTermination::Cancelled
        ) {
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
                    match result.termination() {
                        ProcessTermination::Cancelled => AgentToolAttemptStatus::Cancelled,
                        ProcessTermination::TimedOut | ProcessTermination::Exited(_) => {
                            AgentToolAttemptStatus::Failed
                        }
                    },
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
        let evidence = match self
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
            }) {
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
                    )
                    .await;
            }
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
        self.evidence_store
            .append_verification_evidence(
                project,
                &evidence,
                VERIFICATION_EVIDENCE_TIMEOUT,
                control,
            )
            .await?;
        let spec = current_step_spec(ledger, step_id)?;
        let verification = EvaluateStepVerification.execute(
            ids.step_verification_id,
            spec,
            run.id(),
            &evidence,
            current_index,
            ledger_timestamp(observed_at)?,
        )?;
        let passed = verification.passed();
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

        if passed {
            lease.record_success();
            return Ok(MutationControllerOutcome::StepVerified {
                evidence_id: evidence.id(),
                snapshot_id: run.current_snapshot_id(),
            });
        }
        let decision = lease.record_failure(MutationFailureClass::VerificationFailed);
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
            TaskStepFailureReason::try_from_string(MUTATION_FAILURE_REASON.to_owned())
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
            let selection = command.ok_or(MutationControllerFailure::CommandSelectionRequired)?;
            let spec = PrepareDiscoveredCommand.execute(
                selection.catalog,
                selection.confirmation,
                run.id(),
                action.step_id(),
                action.command_id(),
            )?;
            let command = selection
                .catalog
                .commands()
                .binary_search_by_key(&action.command_id(), |candidate| candidate.id())
                .ok()
                .and_then(|index| selection.catalog.commands().get(index))
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
                result_spec: spec,
                dependencies,
            }
        }
        AgentAction::Search(_)
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

/// Stable orchestration failure before a safe finite controller outcome was persisted.
#[derive(Debug)]
pub enum MutationControllerFailure {
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
    /// A trusted tool returned data that violated its typed result contract.
    InvalidToolResult,
    /// Another action owns the worktree mutation boundary.
    Busy(WorktreeMutationBusy),
    /// Action fingerprint construction failed.
    Fingerprint(MutationActionFingerprintError),
    /// Patch preview failed before policy evaluation.
    PatchPreview(PatchPreviewFailure),
    /// Required exact pre-approval inspection could not be retained.
    Inspection(AgentInspectionSinkFailure),
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
    /// Run event construction failed.
    Run(a3_domain::AgentRunError),
}

impl fmt::Display for MutationControllerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
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
            Self::InvalidToolResult => "mutation tool result is invalid",
            Self::Busy(_) => "worktree mutation boundary is busy",
            Self::Fingerprint(_) => "mutation action fingerprint is invalid",
            Self::PatchPreview(_) => "mutation patch preview failed",
            Self::Inspection(_) => "mutation inspection failed",
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
            Self::Run(_) => "mutation run event failed",
        })
    }
}

impl MutationControllerFailure {
    /// Returns the conservative application state at the point orchestration stopped.
    #[must_use]
    pub const fn mutation_application_state(&self) -> MutationApplicationState {
        match self {
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
            | Self::Run(_) => MutationApplicationState::Unknown,
            Self::NotMutatingAction
            | Self::AnchorMismatch
            | Self::CommandSelectionRequired
            | Self::InvalidCommandSelection
            | Self::InvalidContextSeed
            | Self::InvalidStaticText
            | Self::InvalidTimestamp
            | Self::InvalidPolicyResult
            | Self::Busy(_)
            | Self::Fingerprint(_)
            | Self::PatchPreview(_)
            | Self::Inspection(_)
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
            Self::Busy(error) => Some(error),
            Self::Fingerprint(error) => Some(error),
            Self::PatchPreview(error) => Some(error),
            Self::Inspection(error) => Some(error),
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
            Self::Run(error) => Some(error),
            Self::NotMutatingAction
            | Self::AnchorMismatch
            | Self::CommandSelectionRequired
            | Self::InvalidCommandSelection
            | Self::InvalidContextSeed
            | Self::InvalidContextState
            | Self::StaleCompiledContext
            | Self::InvalidStaticText
            | Self::InvalidTimestamp
            | Self::InvalidPolicyResult
            | Self::InvalidToolResult => None,
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
failure_from!(a3_domain::AgentRunError, Run);
