use crate::agent_conversation_runtime::AgentConversationRuntime;
use crate::agent_session_manager::AgentSessionRunReporter;
use a3_application::{
    AcceptanceVerificationRequest, AdvanceAgentController, AgentActionStore,
    AgentContextCompileInput, AgentControllerSignal, AgentRecoveryStore, AgentRunExecutionFailure,
    AgentRunExecutionFuture, AgentRunExecutionOutcome, AgentRunExecutionRequest,
    AgentRunExecutionTrigger, AgentRunExecutor, AgentTurnOutcome, AgentTurnRejectionReason,
    AppendAgentRead, AppendRunEvent, ApplyAgentLedgerUpdate, CommandAllowlistStore,
    CompileTaskLens, ConservativeProcessVerificationEvidenceFactory,
    DeterministicAcceptanceVerifier, DiscoverProjectCommands, ExecuteAgentTurn,
    ExecuteMutatingAgentAction, IndexPersistenceControl, KnowledgeIndexStore, KnowledgeSearchStore,
    LoadProjectCommandAllowlist, MutationCommandSelection, MutationContextSeed,
    MutationControllerOutcome, MutationExecutionIds, PersistAgentLedgerMutation, PolicyStore,
    ProcessEventSink, ProcessEventSinkError, RefreshRepositoryIndex, RequestAgentFinish,
    RunJournalStore, TaskLensClaimStore, TaskLensIndexStore, TaskLensWorkspaceStore,
    VerificationEvidenceStore, VerifyAgentAcceptance, WorktreeMutationCoordinator,
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
use std::time::{SystemTime, UNIX_EPOCH};

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
        let step_id = active_step_id(&ledger)?;
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
            let published = current_index(self.ports.index.as_ref(), project, control).await?;
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
                    control,
                )
                .await?;
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
                        control,
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
            let observed_at = timestamp()?;
            let turn_outcome = ExecuteAgentTurn::new(
                &context_compiler,
                provider.as_ref(),
                &read_tools,
                self.ports.recovery.as_ref(),
            )
            .execute(&run, &input, observed_at, control)
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
                        current_index(self.ports.index.as_ref(), project, control).await?;
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
                            control,
                        )
                        .await?;
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
                            control,
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
                            control,
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
        control: &a3_application::JobContext,
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
        control: &a3_application::JobContext,
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
