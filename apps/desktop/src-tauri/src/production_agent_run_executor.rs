use crate::agent_conversation_runtime::AgentConversationRuntime;
use crate::agent_session_manager::AgentSessionRunReporter;
use a3_application::{
    AcceptanceVerificationRequest, AdvanceAgentController, AgentActionStore,
    AgentContextCompileInput, AgentControllerSignal, AgentRecoveryStore, AgentRunExecutionFailure,
    AgentRunExecutionFuture, AgentRunExecutionOutcome, AgentRunExecutionRequest,
    AgentRunExecutionTrigger, AgentRunExecutor, AgentTurnOutcome, AgentTurnRejectionReason,
    AppendAgentRead, AppendRunEvent, ApplyAgentLedgerUpdate, AskResearchStore,
    CommandAllowlistStore, CompileTaskLens, ConservativeProcessVerificationEvidenceFactory,
    ContextCompileControl, ContextCompilePhase, ContinueVerifiedAgentPlan,
    ContinueVerifiedAgentPlanOutcome, DeterministicAcceptanceVerifier, DiscoverProjectCommands,
    ExecuteAgentTurn, ExecuteMutatingAgentAction, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeSearchStore,
    LoadProjectCommandAllowlist, ModelCancellationFuture, ModelOperationControl,
    MutationCommandSelection, MutationContextSeed, MutationControllerOutcome, MutationExecutionIds,
    PersistAgentLedgerMutation, PolicyStore, ProcessEventSink, ProcessEventSinkError,
    ProcessRunControl, RefreshRepositoryIndex, RepositoryIndexControl, RepositoryIndexControlError,
    RequestAgentFinish, ResearchHandoff, RunJournalStore, TaskLensClaimStore, TaskLensControlError,
    TaskLensIndexStore, TaskLensWorkspaceStore, VerificationEvidenceStore, VerifyAgentAcceptance,
    WorkspacePatchControl, WorkspacePatchProgressError, WorktreeMutationCoordinator,
};
use a3_context::{DeterministicAgentContextCompiler, DeterministicAgentReadTools};
use a3_domain::{
    AgentAction, AgentControllerState, AgentRun, AgentRunTimestamp, AgentToolEvidenceSet,
    ApprovalGrant, ApprovalRequestId, PolicyDecisionId, ProcessEnvironmentVariable, ProcessEvent,
    Progress, ProjectIdentity, RunEventId, RunMemoryCheckpoint, StepVerificationId, TaskId,
    TaskLedger, TaskStepId, TaskStepStatus, ToolRunId, VerificationRunId, WorkspacePolicy,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_workspace::{
    ProcessHostEnvironment, WorkspaceAgentSourceReader, WorkspacePatchAdapter,
    WorkspaceProcessRunner,
};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_AGENT_TURNS_PER_ATTEMPT: u64 = 64;
const APPROVAL_LIFETIME_MILLIS: u64 = 10 * 60 * 1_000;

/// Narrow production capabilities used by the existing deterministic Agent harness.
#[derive(Clone)]
pub(crate) struct ProductionAgentRunPorts {
    pub(crate) workspace: Arc<dyn TaskLensWorkspaceStore>,
    pub(crate) journal: Arc<dyn RunJournalStore>,
    pub(crate) actions: Arc<dyn AgentActionStore>,
    pub(crate) recovery: Arc<dyn AgentRecoveryStore>,
    pub(crate) policy: Arc<dyn PolicyStore>,
    pub(crate) evidence: Arc<dyn VerificationEvidenceStore>,
    pub(crate) index: Arc<dyn KnowledgeIndexStore>,
    pub(crate) lens_index: Arc<dyn TaskLensIndexStore>,
    pub(crate) search: Arc<dyn KnowledgeSearchStore>,
    pub(crate) claims: Arc<dyn TaskLensClaimStore>,
    pub(crate) allowlist: Arc<dyn CommandAllowlistStore>,
    pub(crate) research: Option<Arc<dyn AskResearchStore>>,
}

/// Complete production executor. It never gives the WebView a tool or provider capability.
pub(crate) struct ProductionAgentRunExecutor {
    ports: ProductionAgentRunPorts,
    runtime: AgentConversationRuntime,
    inspection: Arc<a3_application::AgentInspectionBuffer>,
    approval: Arc<a3_application::AgentApprovalBuffer>,
    reporter: Option<Arc<AgentSessionRunReporter>>,
    coordinator: WorktreeMutationCoordinator,
    pending_mutations: Mutex<BTreeMap<TaskId, AgentAction>>,
    process_environment: ProcessHostEnvironment,
}

impl ProductionAgentRunExecutor {
    pub(crate) fn new(
        ports: ProductionAgentRunPorts,
        runtime: AgentConversationRuntime,
        inspection: Arc<a3_application::AgentInspectionBuffer>,
        approval: Arc<a3_application::AgentApprovalBuffer>,
        reporter: Option<Arc<AgentSessionRunReporter>>,
    ) -> Result<Self, AgentRunExecutionFailure> {
        let variables = ["PATH", "TEMP", "TMP", "TMPDIR"]
            .into_iter()
            .map(|name| ProcessEnvironmentVariable::try_from_string(name.to_owned()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let process_environment = ProcessHostEnvironment::capture(variables)
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        Ok(Self {
            ports,
            runtime,
            inspection,
            approval,
            reporter,
            coordinator: WorktreeMutationCoordinator::new(),
            pending_mutations: Mutex::new(BTreeMap::new()),
            process_environment,
        })
    }

    async fn execute_inner(
        &self,
        project: &ProjectIdentity,
        request: AgentRunExecutionRequest,
        control: &a3_application::JobContext,
    ) -> Result<AgentRunExecutionOutcome, AgentRunExecutionFailure> {
        if control.cancellation_token().is_cancelled() {
            return Ok(AgentRunExecutionOutcome::Cancelled);
        }
        let attempt_control = AgentAttemptControl { context: control };
        let task = self
            .ports
            .workspace
            .load_current_task(project, request.task_id(), control)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        let stored = task
            .task_ledger()
            .cloned()
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        if task.goal_contract().reference() != stored.ledger().goal_contract()
            || stored.ledger().revision() != request.ledger_revision()
            || stored.version() != request.ledger_store_version()
        {
            return Err(AgentRunExecutionFailure::AnchorsChanged);
        }
        let (mut ledger, mut ledger_version) = stored.into_parts();
        let mut step_id = active_step_id(&ledger)?;
        let run_id = ledger
            .step(step_id)
            .and_then(|step| step.attempts().last())
            .map(a3_domain::TaskStepAttempt::run_id)
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        let mut run = self
            .ports
            .journal
            .load_agent_run(project, run_id)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        if run.goal_contract() != task.goal_contract().reference()
            || run.task_ledger_revision() != ledger.revision()
        {
            return Err(AgentRunExecutionFailure::AnchorsChanged);
        }
        let (provider, profile) = self
            .runtime
            .execution_model()
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let initial_research_handoff = match &self.ports.research {
            Some(store) => store
                .load_handoff_for_task(project, request.task_id())
                .await
                .map_err(|_| AgentRunExecutionFailure::Unavailable)?,
            None => None,
        };
        if run.model_profile() != Some(profile.reference()) {
            return Err(AgentRunExecutionFailure::AnchorsChanged);
        }

        let source = WorkspaceAgentSourceReader;
        let read_tools = DeterministicAgentReadTools::new(
            self.ports.lens_index.as_ref(),
            self.ports.search.as_ref(),
            self.ports.claims.as_ref(),
            &source,
        );
        let context_compiler = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
            self.ports.lens_index.as_ref(),
            self.ports.search.as_ref(),
            self.ports.claims.as_ref(),
        ));
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            Arc::clone(&self.ports.index),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let patch_tool = WorkspacePatchAdapter::new();
        let process_runner = WorkspaceProcessRunner::new(self.process_environment.clone());
        let evidence_factory = ConservativeProcessVerificationEvidenceFactory;
        let mut index_compiler = BuiltinIncrementalIndexCompiler::new(
            ParserPoolSize::new(2).map_err(|_| AgentRunExecutionFailure::Unavailable)?,
        )
        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let mutation_controller = ExecuteMutatingAgentAction::new(
            &self.coordinator,
            self.ports.policy.as_ref(),
            self.ports.journal.as_ref(),
            self.ports.actions.as_ref(),
            self.ports.recovery.as_ref(),
            self.ports.evidence.as_ref(),
            self.inspection.as_ref(),
            self.approval.as_ref(),
            &patch_tool,
            &process_runner,
            &evidence_factory,
            &context_compiler,
            &refresh,
        );
        let context_seed = MutationContextSeed::new(
            task.goal_contract().clone(),
            profile.clone(),
            Vec::new(),
            Vec::new(),
        );
        let mut context_results = Vec::new();
        let mut read_evidence: Option<AgentToolEvidenceSet> = None;

        if let AgentRunExecutionTrigger::ApprovalGranted(approval_id) = request.trigger() {
            let action = lock_recovering_poison(&self.pending_mutations)
                .remove(&request.task_id())
                .ok_or(AgentRunExecutionFailure::InvalidState)?;
            let mut grant = self
                .ports
                .policy
                .load_approval(project, approval_id)
                .await
                .map_err(|_| AgentRunExecutionFailure::Unavailable)?
                .ok_or(AgentRunExecutionFailure::InvalidState)?;
            let published =
                current_index(self.ports.index.as_ref(), project, &attempt_control).await?;
            let outcome = self
                .execute_mutation(
                    project,
                    &mut run,
                    &mut ledger,
                    &mut ledger_version,
                    &published,
                    action,
                    Some(&mut grant),
                    &context_seed,
                    &mut index_compiler,
                    &mutation_controller,
                    &attempt_control,
                )
                .await?;
            if matches!(&outcome, MutationControllerOutcome::StepVerified { .. })
                && let Some(next_step_id) = self
                    .continue_verified_plan(
                        project,
                        &mut run,
                        &mut ledger,
                        &mut ledger_version,
                        &attempt_control,
                    )
                    .await?
            {
                step_id = next_step_id;
            }
            if self.handle_mutation_outcome(request.task_id(), outcome)? {
                return Ok(AgentRunExecutionOutcome::Completed);
            }
        } else if run.state() == AgentControllerState::AwaitApproval {
            return Err(AgentRunExecutionFailure::InvalidState);
        }

        for turn in 0..MAX_AGENT_TURNS_PER_ATTEMPT {
            if control.cancellation_token().is_cancelled() {
                return Ok(AgentRunExecutionOutcome::Cancelled);
            }
            control
                .report_progress(
                    Progress::determinate(turn.saturating_add(1), MAX_AGENT_TURNS_PER_ATTEMPT)
                        .map_err(|_| AgentRunExecutionFailure::ProgressUnavailable)?,
                )
                .map_err(|_| AgentRunExecutionFailure::ProgressUnavailable)?;
            match run.state() {
                AgentControllerState::Execute => {}
                AgentControllerState::Verify => {
                    self.verify_acceptance(
                        project,
                        &mut run,
                        &ledger,
                        task.goal_contract(),
                        &attempt_control,
                    )
                    .await?;
                    return Ok(AgentRunExecutionOutcome::Completed);
                }
                AgentControllerState::Done
                | AgentControllerState::Failed
                | AgentControllerState::Cancelled
                | AgentControllerState::Replan
                | AgentControllerState::AwaitApproval => {
                    return Ok(AgentRunExecutionOutcome::Completed);
                }
                _ => return Err(AgentRunExecutionFailure::InvalidState),
            }

            let input = AgentContextCompileInput::new(
                project.clone(),
                task.goal_contract().clone(),
                ledger.clone(),
                step_id,
                profile.clone(),
                None,
                Vec::new(),
                context_results.clone(),
            )
            .map_err(|_| AgentRunExecutionFailure::AnchorsChanged)?;
            let input = match &initial_research_handoff {
                Some(handoff) => {
                    let published =
                        current_index(self.ports.index.as_ref(), project, &attempt_control).await?;
                    input.with_research_handoff(revalidate_research_handoff(handoff, &published)?)
                }
                None => input,
            };
            let observed_at = timestamp()?;
            let turn_outcome = ExecuteAgentTurn::new(
                &context_compiler,
                provider.as_ref(),
                &read_tools,
                self.ports.recovery.as_ref(),
            )
            .execute(&run, &input, observed_at, &attempt_control)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
            let expected_sequence = run.last_event_sequence();
            let event = turn_outcome
                .record(&mut run, run_event_id()?, observed_at)
                .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
            AppendRunEvent::new(self.ports.journal.as_ref())
                .execute(project, expected_sequence, &run, &event)
                .await
                .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
            let mut execution = match turn_outcome {
                AgentTurnOutcome::Executed(execution) => execution,
                AgentTurnOutcome::Rejected(rejected) => {
                    let signal =
                        if rejected.reason() == AgentTurnRejectionReason::CancelledBeforeAction {
                            AgentControllerSignal::CancelRequested
                        } else {
                            AgentControllerSignal::FatalFailure
                        };
                    let expected_sequence = run.last_event_sequence();
                    let snapshot_id = run.current_snapshot_id();
                    let observed_at = timestamp()?;
                    let advance = AdvanceAgentController
                        .execute(
                            &mut run,
                            signal,
                            run_event_id()?,
                            snapshot_id,
                            observed_at,
                            control.cancellation_token().is_cancelled(),
                        )
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                    AppendRunEvent::new(self.ports.journal.as_ref())
                        .execute(project, expected_sequence, &run, advance.event())
                        .await
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                    return Ok(if run.state() == AgentControllerState::Cancelled {
                        AgentRunExecutionOutcome::Cancelled
                    } else {
                        AgentRunExecutionOutcome::Completed
                    });
                }
            };
            let action = execution.action().clone();
            match action {
                AgentAction::Search(_) | AgentAction::Inspect(_) => {
                    let result = execution
                        .take_tool_result()
                        .ok_or(AgentRunExecutionFailure::Unavailable)?;
                    let expected_sequence = run.last_event_sequence();
                    let recorded = result
                        .record(&mut run, run_event_id()?, timestamp()?)
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                    context_results.push(recorded.context_result().clone());
                    read_evidence = Some(recorded.evidence().clone());
                    AppendAgentRead::new(self.ports.journal.as_ref())
                        .execute(project, expected_sequence, &run, &recorded)
                        .await
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                }
                AgentAction::ApplyPatch(_) | AgentAction::Run(_) => {
                    let published =
                        current_index(self.ports.index.as_ref(), project, &attempt_control).await?;
                    let replay = action.clone();
                    let outcome = self
                        .execute_mutation(
                            project,
                            &mut run,
                            &mut ledger,
                            &mut ledger_version,
                            &published,
                            action,
                            None,
                            &context_seed,
                            &mut index_compiler,
                            &mutation_controller,
                            &attempt_control,
                        )
                        .await?;
                    if matches!(&outcome, MutationControllerOutcome::StepVerified { .. })
                        && let Some(next_step_id) = self
                            .continue_verified_plan(
                                project,
                                &mut run,
                                &mut ledger,
                                &mut ledger_version,
                                &attempt_control,
                            )
                            .await?
                    {
                        step_id = next_step_id;
                    }
                    if matches!(outcome, MutationControllerOutcome::AwaitingApproval(_)) {
                        lock_recovering_poison(&self.pending_mutations)
                            .insert(request.task_id(), replay);
                    }
                    if self.handle_mutation_outcome(request.task_id(), outcome)? {
                        return Ok(AgentRunExecutionOutcome::Completed);
                    }
                    context_results.clear();
                    read_evidence = None;
                }
                AgentAction::UpdateLedger(update) => {
                    let expected_sequence = run.last_event_sequence();
                    let snapshot_id = run.current_snapshot_id();
                    let outcome = ApplyAgentLedgerUpdate
                        .execute(
                            &mut run,
                            &mut ledger,
                            &update,
                            read_evidence.as_ref(),
                            run_event_id()?,
                            snapshot_id,
                            timestamp()?,
                            &attempt_control,
                        )
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                    ledger_version = PersistAgentLedgerMutation::new(self.ports.actions.as_ref())
                        .execute(
                            project,
                            ledger_version,
                            expected_sequence,
                            &ledger,
                            &run,
                            &outcome,
                        )
                        .await
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                }
                AgentAction::Finish(finish) => {
                    let expected_sequence = run.last_event_sequence();
                    let snapshot_id = run.current_snapshot_id();
                    let advance = RequestAgentFinish
                        .execute(
                            &mut run,
                            finish,
                            run_event_id()?,
                            snapshot_id,
                            timestamp()?,
                            &attempt_control,
                        )
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                    AppendRunEvent::new(self.ports.journal.as_ref())
                        .execute(project, expected_sequence, &run, advance.event())
                        .await
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
                }
            }
        }
        Err(AgentRunExecutionFailure::Unavailable)
    }

    async fn continue_verified_plan(
        &self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut a3_application::TaskLedgerStoreVersion,
        control: &AgentAttemptControl<'_>,
    ) -> Result<Option<TaskStepId>, AgentRunExecutionFailure> {
        match ContinueVerifiedAgentPlan::new(self.ports.actions.as_ref())
            .execute(
                project,
                *ledger_version,
                run,
                ledger,
                run_event_id()?,
                timestamp()?,
                control,
            )
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?
        {
            ContinueVerifiedAgentPlanOutcome::ReadyForAcceptance => Ok(None),
            ContinueVerifiedAgentPlanOutcome::StepStarted {
                step_id,
                ledger_version: next_version,
            } => {
                *ledger_version = next_version;
                Ok(Some(step_id))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_mutation(
        &self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut a3_application::TaskLedgerStoreVersion,
        published: &a3_domain::PublishedIndex,
        action: AgentAction,
        approval: Option<&mut ApprovalGrant>,
        context_seed: &MutationContextSeed,
        index_compiler: &mut BuiltinIncrementalIndexCompiler,
        controller: &ExecuteMutatingAgentAction<'_>,
        control: &AgentAttemptControl<'_>,
    ) -> Result<MutationControllerOutcome, AgentRunExecutionFailure> {
        let catalog = DiscoverProjectCommands
            .execute(project.worktree().id(), published)
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let confirmation = LoadProjectCommandAllowlist::new(self.ports.allowlist.as_ref())
            .execute(project)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let selection = confirmation
            .as_ref()
            .map(|confirmation| MutationCommandSelection::new(&catalog, confirmation));
        controller
            .execute(
                project,
                run,
                ledger,
                ledger_version,
                published,
                action,
                selection,
                &WorkspacePolicy::unrestricted(),
                approval,
                mutation_ids()?,
                timestamp()?,
                approval_expiration()?,
                context_seed,
                index_compiler,
                &NoopProcessEvents,
                control,
            )
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)
    }

    fn handle_mutation_outcome(
        &self,
        task_id: TaskId,
        outcome: MutationControllerOutcome,
    ) -> Result<bool, AgentRunExecutionFailure> {
        Ok(match outcome {
            MutationControllerOutcome::AwaitingApproval(_) => true,
            MutationControllerOutcome::NextAction(_) => false,
            MutationControllerOutcome::StepVerified { .. } => false,
            MutationControllerOutcome::Denied
            | MutationControllerOutcome::ReplanRequired { .. }
            | MutationControllerOutcome::ReconciliationRequired { .. }
            | MutationControllerOutcome::Stopped { .. } => {
                lock_recovering_poison(&self.pending_mutations).remove(&task_id);
                true
            }
        })
    }

    async fn verify_acceptance(
        &self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &TaskLedger,
        goal: &a3_domain::GoalContract,
        control: &AgentAttemptControl<'_>,
    ) -> Result<(), AgentRunExecutionFailure> {
        let published = current_index(self.ports.index.as_ref(), project, control).await?;
        let memory = RunMemoryCheckpoint::compile(goal, ledger, run, &published, Vec::new())
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let request = AcceptanceVerificationRequest::new(
            project.clone(),
            run,
            goal.clone(),
            ledger.clone(),
            memory,
        )
        .map_err(|_| AgentRunExecutionFailure::AnchorsChanged)?;
        let expected_sequence = run.last_event_sequence();
        let verifier = DeterministicAcceptanceVerifier::new(self.ports.evidence.as_ref());
        let accepted = VerifyAgentAcceptance::new(&verifier)
            .execute(run, &request, run_event_id()?, timestamp()?, control)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        AppendRunEvent::new(self.ports.journal.as_ref())
            .execute(project, expected_sequence, run, accepted.event())
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)
    }

    async fn synchronize_session(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        outcome: &Result<AgentRunExecutionOutcome, AgentRunExecutionFailure>,
        control: &a3_application::JobContext,
    ) {
        let Some(reporter) = &self.reporter else {
            return;
        };
        if matches!(outcome, Ok(AgentRunExecutionOutcome::Cancelled)) {
            let _reported = reporter
                .report(
                    project,
                    task_id,
                    a3_domain::AgentSessionState::Cancelled,
                    "Der Agentenlauf wurde abgebrochen. Bereits verifizierte Auditdaten bleiben erhalten.",
                )
                .await;
            return;
        }
        let state = match self
            .ports
            .workspace
            .load_current_task(project, task_id, control)
            .await
            .ok()
            .flatten()
            .and_then(|task| task.task_ledger().cloned())
        {
            Some(stored) => {
                let run_id = stored
                    .ledger()
                    .steps()
                    .filter_map(|step| step.attempts().last())
                    .last()
                    .map(a3_domain::TaskStepAttempt::run_id);
                match run_id {
                    Some(run_id) => self
                        .ports
                        .journal
                        .load_agent_run(project, run_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|run| run.state()),
                    None => None,
                }
            }
            None => None,
        };
        let (session_state, message) = match state {
            Some(AgentControllerState::Done) => (
                a3_domain::AgentSessionState::Completed,
                "Die Aufgabe ist verifiziert abgeschlossen. Änderungen und Evidence stehen im Review bereit.",
            ),
            Some(AgentControllerState::AwaitApproval) => (
                a3_domain::AgentSessionState::AwaitingApproval,
                "Der Agent wartet auf die exakte Freigabe der im Review sichtbaren Aktion.",
            ),
            Some(AgentControllerState::Cancelled) => (
                a3_domain::AgentSessionState::Cancelled,
                "Der Agentenlauf wurde abgebrochen. Bereits verifizierte Auditdaten bleiben erhalten.",
            ),
            Some(AgentControllerState::Replan) => (
                a3_domain::AgentSessionState::Failed,
                "Die aktuelle Ausführung benötigt eine neue Planung. Der bisherige Lauf bleibt prüfbar.",
            ),
            Some(AgentControllerState::Failed) => (
                a3_domain::AgentSessionState::Failed,
                "Der Agentenlauf wurde sicher angehalten. Details und Recovery stehen im Inspector bereit.",
            ),
            Some(AgentControllerState::Execute | AgentControllerState::Verify)
            | Some(
                AgentControllerState::Intake
                | AgentControllerState::Localize
                | AgentControllerState::Plan,
            )
            | None => (
                a3_domain::AgentSessionState::Failed,
                "Der Agentenlauf endete ohne verifizierten Abschluss. Details stehen im Inspector bereit.",
            ),
        };
        let _reported = reporter
            .report(project, task_id, session_state, message)
            .await;
    }
}

impl AgentRunExecutor for ProductionAgentRunExecutor {
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: AgentRunExecutionRequest,
        control: &'a a3_application::JobContext,
    ) -> AgentRunExecutionFuture<'a> {
        Box::pin(async move {
            let outcome = self.execute_inner(project, request, control).await;
            self.synchronize_session(project, request.task_id(), &outcome, control)
                .await;
            outcome
        })
    }
}

impl fmt::Debug for ProductionAgentRunExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionAgentRunExecutor")
            .field(
                "pending_mutations",
                &lock_recovering_poison(&self.pending_mutations).len(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct NoopProcessEvents;

impl ProcessEventSink for NoopProcessEvents {
    fn emit(&self, _event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        Ok(())
    }
}

fn active_step_id(ledger: &TaskLedger) -> Result<TaskStepId, AgentRunExecutionFailure> {
    let mut active = ledger.steps().filter(|step| {
        matches!(
            step.status(),
            TaskStepStatus::InProgress
                | TaskStepStatus::AwaitingApproval
                | TaskStepStatus::Verifying
        )
    });
    let id = active
        .next()
        .map(|step| step.definition().id())
        .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
    if active.next().is_some() {
        return Err(AgentRunExecutionFailure::AnchorsChanged);
    }
    Ok(id)
}

async fn current_index(
    store: &dyn KnowledgeIndexStore,
    project: &ProjectIdentity,
    control: &dyn IndexPersistenceControl,
) -> Result<a3_domain::PublishedIndex, AgentRunExecutionFailure> {
    store
        .latest_published_index(project, control)
        .await
        .map_err(|_| AgentRunExecutionFailure::Unavailable)?
        .ok_or(AgentRunExecutionFailure::AnchorsChanged)
}

fn revalidate_research_handoff(
    handoff: &ResearchHandoff,
    current: &a3_domain::PublishedIndex,
) -> Result<ResearchHandoff, AgentRunExecutionFailure> {
    let revisions = handoff
        .revisions()
        .iter()
        .filter(|revision| {
            current
                .publication()
                .graph()
                .files()
                .iter()
                .any(|candidate| candidate == *revision)
        })
        .cloned()
        .collect();
    let revalidated =
        ResearchHandoff::new(current.run().id(), current.run().snapshot_id(), revisions)
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
    Ok(match handoff.command() {
        Some(command) => revalidated.with_command(command.clone()),
        None => revalidated,
    })
}

/// Keeps nested context, patch, process, and index progress inside one Agent-turn scope.
/// The owning job reports only the monotone turn count, so a nested operation can never
/// replace the scheduler's fixed total or regress it when the next turn begins.
#[derive(Debug)]
struct AgentAttemptControl<'a> {
    context: &'a a3_application::JobContext,
}

impl a3_application::AgentControllerControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }
}

impl ContextCompileControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_phase(&self, _phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl ModelOperationControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(self.context.cancellation_token().cancelled())
    }
}

impl IndexPersistenceControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

impl RepositoryIndexControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl WorkspacePatchControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}

impl ProcessRunControl for AgentAttemptControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn wait_cancelled_timeout(&self, timeout: Duration) -> bool {
        self.context
            .cancellation_token()
            .wait_cancelled_timeout(timeout)
    }
}

fn mutation_ids() -> Result<MutationExecutionIds, AgentRunExecutionFailure> {
    Ok(MutationExecutionIds::new(
        PolicyDecisionId::from_bytes(random_id()?),
        ApprovalRequestId::from_bytes(random_id()?),
        run_event_id()?,
        run_event_id()?,
        ToolRunId::from_bytes(random_id()?),
        run_event_id()?,
        run_event_id()?,
        run_event_id()?,
        run_event_id()?,
        VerificationRunId::from_bytes(random_id()?),
        StepVerificationId::from_bytes(random_id()?),
    ))
}

fn run_event_id() -> Result<RunEventId, AgentRunExecutionFailure> {
    Ok(RunEventId::from_bytes(random_id()?))
}

fn timestamp() -> Result<AgentRunTimestamp, AgentRunExecutionFailure> {
    AgentRunTimestamp::from_unix_millis(now_millis()?)
        .map_err(|_| AgentRunExecutionFailure::Unavailable)
}

fn approval_expiration() -> Result<AgentRunTimestamp, AgentRunExecutionFailure> {
    AgentRunTimestamp::from_unix_millis(
        now_millis()?
            .checked_add(APPROVAL_LIFETIME_MILLIS)
            .ok_or(AgentRunExecutionFailure::Unavailable)?,
    )
    .map_err(|_| AgentRunExecutionFailure::Unavailable)
}

fn now_millis() -> Result<u64, AgentRunExecutionFailure> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AgentRunExecutionFailure::Unavailable)?
        .as_millis();
    u64::try_from(value).map_err(|_| AgentRunExecutionFailure::Unavailable)
}

fn random_id() -> Result<[u8; 32], AgentRunExecutionFailure> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AgentRunExecutionFailure::Unavailable)?;
    Ok(bytes)
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::AgentAttemptControl;
    use a3_application::{
        ContextCompileControl, ContextCompilePhase, JobClock, JobCompletion, JobContext,
        JobEventKind, JobScheduler, JobSchedulerConfig, JobTimestamp, RepositoryIndexControl,
        WorkspacePatchControl,
    };
    use a3_domain::{JobId, JobOwner, Progress};
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Debug)]
    struct FixedClock;

    impl JobClock for FixedClock {
        fn now(&self) -> JobTimestamp {
            JobTimestamp::from_millis(1)
        }
    }

    #[test]
    fn nested_agent_operations_cannot_replace_the_attempt_progress_scale()
    -> Result<(), Box<dyn std::error::Error>> {
        let (scheduler, events) =
            JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixedClock))?;
        scheduler.submit(JobId::new(92), JobOwner::new(3), |context: JobContext| {
            if context
                .report_progress(Progress::determinate(1, 64).unwrap_or(Progress::Indeterminate))
                .is_err()
            {
                return JobCompletion::Failed;
            }
            let nested = AgentAttemptControl { context: &context };
            if ContextCompileControl::report_phase(&nested, ContextCompilePhase::Anchor).is_err()
                || ContextCompileControl::report_phase(&nested, ContextCompilePhase::Complete)
                    .is_err()
                || RepositoryIndexControl::report_progress(
                    &nested,
                    Progress::determinate(0, 6).unwrap_or(Progress::Indeterminate),
                )
                .is_err()
                || WorkspacePatchControl::report_progress(
                    &nested,
                    Progress::determinate(0, 2).unwrap_or(Progress::Indeterminate),
                )
                .is_err()
                || context
                    .report_progress(
                        Progress::determinate(2, 64).unwrap_or(Progress::Indeterminate),
                    )
                    .is_err()
            {
                return JobCompletion::Failed;
            }
            JobCompletion::Succeeded
        })?;

        let mut succeeded = false;
        while let Some(event) = events.next_timeout(Duration::from_secs(2))? {
            if event.kind() == JobEventKind::Succeeded {
                succeeded = true;
                break;
            }
            if matches!(event.kind(), JobEventKind::Failed | JobEventKind::Cancelled) {
                break;
            }
        }
        assert!(succeeded);
        Ok(())
    }
}
