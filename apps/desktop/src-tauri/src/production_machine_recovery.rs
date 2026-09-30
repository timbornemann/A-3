//! Human-only recovery of an exact durable machine scope; never replays the original action.
use super::*;
use a3_application::{
    AgentRecoveryChoice, AuthorizedMachineFileAction, EvaluateActionPolicy, GrantPolicyApproval,
    MachineFileControl, MachineFileTool, PersistPolicyEvaluation, PolicyEvaluationContext,
    ReconcileUnknownMutation, RecoverAgentRun,
};
use a3_domain::{
    AgentMutationKind, AgentRunId, AgentToolAttemptStatus, ApprovalId, MachineFileAction,
    MachineFileOperation, MachineFilePath, MutationActionFingerprint, PolicyDecisionOutcome,
    PolicyEvaluationTiming,
};
use a3_protocol::{AgentMachineRecoveryScopeV1, MachineRecoveryKindV1};

impl ProductionAgentRunExecutor {
    pub(crate) async fn machine_recovery_scope(
        &self,
        project: &ProjectIdentity,
        task: TaskId,
    ) -> Result<Option<AgentMachineRecoveryScopeV1>, AgentRunExecutionFailure> {
        let Some(task) = self
            .ports
            .workspace
            .load_current_task(project, task, &crate::DesktopBoundedReadControl::new())
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?
        else {
            return Ok(None);
        };
        let Some(stored) = task.task_ledger() else {
            return Ok(None);
        };
        let Some((attempt, scope)) = self
            .machine_recovery_candidate(project, stored.ledger(), None)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(AgentMachineRecoveryScopeV1 {
            ledger_revision: stored.ledger().revision().get(),
            ledger_store_version: stored.version().get().to_string(),
            scope: hex_bytes(&attempt.fingerprint().as_bytes()),
            resource_kind: match scope.kind() {
                AgentMutationKind::MachineFile => MachineRecoveryKindV1::File,
                AgentMutationKind::MachineProcess => MachineRecoveryKindV1::Process,
                _ => MachineRecoveryKindV1::Http,
            },
            target: scope.target().to_owned(),
        }))
    }

    pub(super) async fn machine_recovery_anchor(
        &self,
        project: &ProjectIdentity,
        ledger: &TaskLedger,
        expected: MutationActionFingerprint,
    ) -> Result<(TaskStepId, AgentRunId), AgentRunExecutionFailure> {
        let (attempt, scope) = self
            .machine_recovery_candidate(project, ledger, Some(expected))
            .await?
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        Ok((scope.step(), attempt.tool_attempt().run_id()))
    }

    async fn machine_recovery_candidate(
        &self,
        project: &ProjectIdentity,
        ledger: &TaskLedger,
        expected: Option<MutationActionFingerprint>,
    ) -> Result<
        Option<(
            a3_domain::AgentMutationAttempt,
            a3_application::MachineEffectScope,
        )>,
        AgentRunExecutionFailure,
    > {
        let runs: BTreeSet<_> = ledger
            .steps()
            .filter_map(|step| step.attempts().last().map(|a| a.run_id()))
            .collect();
        let mut selected = None;
        for run_id in runs {
            let Some(run) = self
                .ports
                .journal
                .load_agent_run(project, run_id)
                .await
                .map_err(invalid)?
            else {
                continue;
            };
            if run.state().is_terminal()
                || run.goal_contract() != ledger.goal_contract()
                || run.task_ledger_revision() != ledger.revision()
            {
                continue;
            }
            for attempt in self
                .ports
                .recovery
                .load_agent_mutation_attempts(project, run_id)
                .await
                .map_err(invalid)?
            {
                if !is_machine(attempt.kind())
                    || expected.is_some_and(|fp| fp != attempt.fingerprint())
                {
                    continue;
                }
                let unresolved = attempt.disposition().requires_reconciliation()
                    || attempt.disposition().requires_replan();
                let replanned = matches!(
                    attempt.disposition(),
                    a3_domain::AgentMutationDisposition::Unknown(
                        a3_domain::MutationReconciliation::Replanned { .. }
                    )
                );
                if !unresolved && !replanned {
                    continue;
                }
                let scope = self
                    .ports
                    .recovery
                    .load_machine_effect_scope(
                        project,
                        attempt.tool_attempt().tool_run_id(),
                        attempt.tool_attempt().attempt(),
                    )
                    .await
                    .map_err(invalid)?
                    .ok_or(AgentRunExecutionFailure::InvalidState)?;
                let Some(owner) = ledger.step(scope.step()) else {
                    return Err(AgentRunExecutionFailure::AnchorsChanged);
                };
                if owner.attempts().last().is_none_or(|a| a.run_id() != run_id) {
                    continue;
                }
                let interrupted_replan = replanned
                    && owner.status() == TaskStepStatus::Cancelled
                    && (owner.is_active_plan_step()
                        || matches!(
                            run.state(),
                            AgentControllerState::Replan
                                | AgentControllerState::Localize
                                | AgentControllerState::Plan
                        ));
                if !(unresolved || interrupted_replan) {
                    continue;
                }
                if selected.is_some() {
                    return Err(AgentRunExecutionFailure::AnchorsChanged);
                }
                selected = Some((attempt, scope));
            }
        }
        Ok(selected)
    }

    pub(super) async fn recover_machine(
        &self,
        project: &ProjectIdentity,
        request: AgentRunExecutionRequest,
        run: &mut AgentRun,
        expected: MutationActionFingerprint,
        control: &AgentAttemptControl<'_>,
    ) -> Result<
        (
            TaskLedger,
            a3_application::TaskLedgerStoreVersion,
            TaskStepId,
        ),
        AgentRunExecutionFailure,
    > {
        let selected = self
            .ports
            .recovery
            .load_agent_mutation_attempts(project, run.id())
            .await
            .map_err(invalid)?
            .into_iter()
            .find(|attempt| {
                attempt.fingerprint() == expected
                    && is_machine(attempt.kind())
                    && matches!(
                        attempt.disposition(),
                        a3_domain::AgentMutationDisposition::Unknown(_)
                    )
            })
            .ok_or(AgentRunExecutionFailure::AnchorsChanged)?;
        let tool = selected.tool_attempt().tool_run_id();
        let number = selected.tool_attempt().attempt();
        // Restart recovery may have interrupted the abandoned attempt already. An owned
        // running action cannot enter this job because the manager and lease serialize it.
        if selected.tool_attempt().status() == AgentToolAttemptStatus::InFlight {
            self.ports
                .recovery
                .interrupt_agent_tool_attempts(project, run.id(), timestamp()?)
                .await
                .map_err(invalid)?;
        }
        let scope = self
            .ports
            .recovery
            .load_machine_effect_scope(project, tool, number)
            .await
            .map_err(invalid)?
            .ok_or(AgentRunExecutionFailure::InvalidState)?;
        if !self
            .ports
            .recovery
            .machine_recovery_acknowledged(project, tool, number)
            .await
            .map_err(invalid)?
        {
            let (observed, decision) = if scope.kind() == AgentMutationKind::MachineFile {
                let lease = self
                    .coordinator
                    .try_acquire(run.id(), project.worktree().id(), expected)
                    .map_err(invalid)?;
                let permissions = self
                    .ports
                    .permissions
                    .as_ref()
                    .ok_or(AgentRunExecutionFailure::InvalidState)?;
                let files = a3_workspace::WorkspaceMachineFileTool::new(Arc::clone(permissions));
                let read = MachineFileAction::new(
                    scope.step(),
                    MachineFilePath::new(scope.target().to_owned()).map_err(invalid)?,
                    MachineFileOperation::Read(None),
                )
                .map_err(invalid)?;
                let file_control = RecoveryFileControl(control);
                let prepared = files
                    .prepare(project, &read, &file_control)
                    .await
                    .map_err(invalid)?;
                let policy = prepared.policy_action();
                if !matches!(policy, a3_domain::PolicyAction::MachineFile {resource_id, ..} if *resource_id == scope.resource())
                {
                    return Err(AgentRunExecutionFailure::AnchorsChanged);
                }
                let mut decision = self
                    .recovery_read_policy(project, run, policy, None)
                    .await?;
                if decision.outcome() == PolicyDecisionOutcome::ApprovalRequired {
                    // This is a real exact grant from the user's displayed observe-and-replan
                    // choice. Full machine uses an actual automatic decision and creates no grant.
                    let snapshot = run.current_snapshot_id();
                    let mut grant = Some(
                        GrantPolicyApproval::new(self.ports.policy.as_ref())
                            .execute(
                                project,
                                run,
                                decision
                                    .approval_request_id()
                                    .ok_or(AgentRunExecutionFailure::InvalidState)?,
                                ApprovalId::from_bytes(random_id()?),
                                run_event_id()?,
                                snapshot,
                                timestamp()?,
                            )
                            .await
                            .map_err(invalid)?,
                    );
                    decision = self
                        .recovery_read_policy(project, run, policy, grant.as_mut())
                        .await?;
                }
                let receipt = files
                    .execute(
                        AuthorizedMachineFileAction::new(prepared, &decision).map_err(invalid)?,
                        &file_control,
                    )
                    .await
                    .map_err(invalid)?;
                lease.record_success();
                (receipt.hash, Some(receipt.decision))
            } else {
                (None, None)
            };
            self.ports
                .recovery
                .acknowledge_machine_recovery(
                    project,
                    tool,
                    number,
                    expected,
                    scope.resource(),
                    observed,
                    decision,
                    timestamp()?,
                )
                .await
                .map_err(invalid)?;
        }
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            Arc::clone(&self.ports.index),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler =
            BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(2).map_err(invalid)?)
                .map_err(invalid)?;
        if selected.disposition().requires_reconciliation() {
            ReconcileUnknownMutation::new(
                &self.coordinator,
                self.ports.recovery.as_ref(),
                &refresh,
            )
            .execute(
                project,
                run,
                tool,
                number,
                run_event_id()?,
                timestamp()?,
                &mut compiler,
                control,
            )
            .await
            .map_err(invalid)?;
        }
        let (mut ledger, mut version) = if selected.disposition().requires_reconciliation()
            || selected.disposition().requires_replan()
        {
            let recovered = RecoverAgentRun::new(
                self.ports.recovery.as_ref(),
                self.ports.journal.as_ref(),
                self.ports.ledgers.as_ref(),
                self.ports.index.as_ref(),
            )
            .execute(
                project,
                run.id(),
                AgentRecoveryChoice::Replan,
                run_event_id()?,
                timestamp()?,
                control,
            )
            .await
            .map_err(invalid)?;
            *run = recovered.run().clone();
            recovered.ledger().clone().into_parts()
        } else {
            self.ports
                .ledgers
                .load_task_ledger(project, request.task_id())
                .await
                .map_err(invalid)?
                .ok_or(AgentRunExecutionFailure::AnchorsChanged)?
                .into_parts()
        };
        let owner_retired = ledger
            .step(scope.step())
            .is_some_and(|step| !step.is_active_plan_step());
        if !owner_retired && run.state() == AgentControllerState::Execute {
            let sequence = run.last_event_sequence();
            let event = AdvanceAgentController
                .execute(
                    run,
                    AgentControllerSignal::ExecutionNeedsReplan,
                    run_event_id()?,
                    run.current_snapshot_id(),
                    timestamp()?,
                    false,
                )
                .map_err(invalid)?;
            AppendRunEvent::new(self.ports.journal.as_ref())
                .execute(project, sequence, run, event.event())
                .await
                .map_err(invalid)?;
        }
        let step = if owner_retired {
            self.resume_replanned_execution(project, run, &mut ledger, &mut version)
                .await?
        } else {
            self.apply_automatic_replan(project, run, &mut ledger, &mut version,
            TaskReplanReason::try_from_string("Externe Wirkung bleibt historisch Unknown; der Nutzer hat den angezeigten Umfang bestätigt. Aktuellen Zustand neu prüfen und planen, ursprüngliche Aktion nicht wiederholen.".to_owned()).map_err(invalid)?, control).await?
        };
        Ok((ledger, version, step))
    }

    async fn recovery_read_policy(
        &self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        action: &a3_domain::PolicyAction,
        grant: Option<&mut ApprovalGrant>,
    ) -> Result<a3_domain::PolicyDecision, AgentRunExecutionFailure> {
        let settings = self
            .ports
            .permissions
            .as_ref()
            .ok_or(AgentRunExecutionFailure::InvalidState)?
            .load_agent_permissions()
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        let now = timestamp()?;
        let mut next = run.clone();
        let mut candidate = grant.as_deref().cloned();
        let evaluation = EvaluateActionPolicy::new()
            .with_permissions(settings)
            .execute(
                &mut next,
                action,
                &WorkspacePolicy::unrestricted(),
                candidate.as_mut(),
                PolicyEvaluationContext::new(
                    PolicyDecisionId::from_bytes(random_id()?),
                    ApprovalRequestId::from_bytes(random_id()?),
                    run_event_id()?,
                    run.current_snapshot_id(),
                    PolicyEvaluationTiming::new(now, now)
                        .map_err(|_| AgentRunExecutionFailure::Unavailable)?,
                    approval_expiration()?,
                ),
            )
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        PersistPolicyEvaluation::new(self.ports.policy.as_ref())
            .execute(project, run.last_event_sequence(), &next, &evaluation)
            .await
            .map_err(|_| AgentRunExecutionFailure::Unavailable)?;
        *run = next;
        if let (Some(grant), Some(candidate)) = (grant, candidate) {
            *grant = candidate;
        }
        Ok(evaluation.decision().clone())
    }
}

#[derive(Debug)]
struct RecoveryFileControl<'a>(&'a AgentAttemptControl<'a>);
impl MachineFileControl for RecoveryFileControl<'_> {
    fn is_cancelled(&self) -> bool {
        a3_application::AgentControllerControl::is_cancelled(self.0)
    }
}
fn is_machine(kind: AgentMutationKind) -> bool {
    matches!(
        kind,
        AgentMutationKind::MachineFile
            | AgentMutationKind::MachineProcess
            | AgentMutationKind::MachineNetwork
    )
}
fn hex_bytes(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid<E>(_: E) -> AgentRunExecutionFailure {
    AgentRunExecutionFailure::Unavailable
}
