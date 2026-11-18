//! Frozen A04's complete HTTP obligation through exact visible one-time approvals.

use super::*;
use a3_application::{
    AgentApprovalAction, AgentApprovalCenter, AgentApprovalControlAction,
    AgentApprovalControlMetadata, AgentApprovalControlOutcome, AgentApprovalControlResult,
    AgentApprovalLoadResult, AgentApprovalNetworkScope, AgentApprovalProcess, AgentApprovalStatus,
    AuthorizedLoopbackStartupCoverage, AuthorizedLoopbackVerification, ControlAgentApproval,
    GetAgentApprovalCenter, GoalContractStore, LoopbackApprovalAdmission, LoopbackProbeApproval,
    LoopbackStartupVerificationFuture, LoopbackStartupVerificationOutcome,
    LoopbackVerificationAuthority, LoopbackVerificationFailure, LoopbackVerificationFuture,
    LoopbackVerificationOutcome, LoopbackVerificationRequest, LoopbackVerificationRunner,
    PreparedLoopbackStartupCoverage, RunEventPageLimit,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, ApprovalGrantState, LoopbackProcessSettings,
    LoopbackStartupPhase, PolicyAction, ProcessExecutionMode, ProcessNetworkScope,
    ProcessOutputLimit, ProcessPlanBinding, ProcessSpec, ProcessTimeout, RunEvent, RunEventKind,
    RunEventSubject, VerificationTarget,
};
use a3_workspace::WorkspaceLoopbackVerificationRunner;
use std::sync::atomic::{AtomicUsize, Ordering};

#[allow(clippy::too_many_arguments)]
pub(super) async fn verify_http(
    fixture: &Fixture,
    store: &Arc<LibsqlKnowledgeStore>,
    goal: &GoalContract,
    profile: &ModelProfile,
    run: &mut AgentRun,
    ledger: &mut TaskLedger,
    version: &mut TaskLedgerStoreVersion,
    refresh: &RefreshRepositoryIndex,
    compiler: &mut BuiltinIncrementalIndexCompiler,
    clock: &A04Clock,
) -> TestResult<TaskEvidenceId> {
    let project = &fixture.project;
    let task = goal.task_id();
    assert_eq!(run.state(), AgentControllerState::Execute);
    assert_eq!(run.goal_contract(), goal.reference());
    assert_eq!(ledger.goal_contract(), goal.reference());
    assert_eq!(
        store
            .load_current_goal_contract(project, task)
            .await?
            .as_ref(),
        Some(goal)
    );
    assert_eq!(
        store.load_agent_run(project, run.id()).await?.as_ref(),
        Some(&*run)
    );
    assert_eq!(
        store.load_task_ledger(project, task).await?,
        Some(StoredTaskLedger::new(ledger.clone(), *version))
    );
    let step = ledger
        .step(HTTP_STEP)
        .ok_or("A04 active HTTP step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    let spec = step.definition().verification_spec().clone();
    let VerificationTarget::LoopbackHttp(recipe) = spec.target() else {
        return Err("A04 active step lacks its bound HTTP recipe".into());
    };
    let [criterion] = goal.draft().acceptance_criteria() else {
        return Err("A04 frozen original must retain one complete outcome".into());
    };
    criterion
        .origin()
        .ok_or("A04 HTTP criterion origin missing")?
        .evidence_requirement()
        .validate_spec(&spec)?;
    assert_eq!(recipe.script().as_bytes(), b"server.py");
    assert_eq!(recipe.port().get(), 18_765);
    assert_eq!(recipe.checks().len(), 6);
    assert!(recipe.default_start_check_index().is_some());

    let publication = refresh
        .execute(
            project,
            &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
            compiler,
            &ActiveControl,
        )
        .await?;
    let published = publication.published_index();
    assert_eq!(published.run().snapshot_id(), run.current_snapshot_id());
    let script = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == recipe.script())
        .ok_or("A04 current server is absent from the published index")?
        .clone();
    let limit = ProcessOutputLimit::new(1024)?;
    let settings = LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(12_000)?,
        stdout_limit: limit,
        stderr_limit: limit,
        system_root: cfg!(windows),
    };
    let prepared = PreparedLoopbackStartupCoverage::new(
        run.id(),
        project.worktree().id(),
        HTTP_STEP,
        run.current_snapshot_id(),
        script,
        recipe.clone(),
        settings,
    )?;
    let actions = prepared.policy_actions();
    assert_eq!(actions.len(), 12);
    assert_ne!(actions[1].fingerprint(), actions[2].fingerprint());
    let requests = (0..12)
        .map(|index| ApprovalRequestId::from_bytes(identity("p7.a04.http.request", index)))
        .collect::<Vec<_>>();
    let decisions = (0..12)
        .map(|index| PolicyDecisionId::from_bytes(identity("p7.a04.http.used-decision", index)))
        .collect::<Vec<_>>();
    let probes = (1..12)
        .map(|index| LoopbackProbeApproval {
            request_id: ApprovalRequestId::from_bytes(identity("p7.a04.http.request", index)),
            decision_id: PolicyDecisionId::from_bytes(identity("p7.a04.http.used-decision", index)),
            event_id: RunEventId::from_bytes(identity("p7.a04.http.used-policy-event", index)),
        })
        .collect::<Vec<_>>();
    let issuance = (0..12)
        .map(|index| {
            (
                PolicyDecisionId::from_bytes(identity("p7.a04.http.issued-decision", index)),
                RunEventId::from_bytes(identity("p7.a04.http.issued-policy-event", index)),
            )
        })
        .collect::<Vec<_>>();
    let tool = ToolRunId::from_bytes(identity("p7.a04.http.tool", 0));
    let ids = MutationExecutionIds::new(
        decisions[0],
        requests[0],
        RunEventId::from_bytes(identity("p7.a04.http.used-policy-event", 0)),
        RunEventId::from_bytes(identity("p7.a04.http.approval-wait-event", 0)),
        tool,
        RunEventId::from_bytes(identity("p7.a04.http.tool-event", 0)),
        RunEventId::from_bytes(identity("p7.a04.http.verification-event", 0)),
        RunEventId::from_bytes(identity("p7.a04.http.resolution-event", 0)),
        RunEventId::from_bytes(identity("p7.a04.http.context-event", 0)),
        VerificationRunId::from_bytes(identity("p7.a04.http.verification-run", 0)),
        StepVerificationId::from_bytes(identity("p7.a04.http.step-verification", 0)),
    );
    let before_attempts = store
        .load_agent_mutation_attempts(project, run.id())
        .await?;
    assert!(
        before_attempts
            .iter()
            .all(|attempt| attempt.tool_attempt().tool_run_id() != tool)
    );
    let before_events = journal(store, project, run.id()).await?;
    let before_tools = tool_events(&before_events);
    let mut environment = vec![ProcessEnvironmentVariable::try_from_string(
        "PATH".to_owned(),
    )?];
    if cfg!(windows) {
        environment.push(ProcessEnvironmentVariable::try_from_string(
            "SYSTEMROOT".to_owned(),
        )?);
    }
    let native =
        WorkspaceLoopbackVerificationRunner::new(ProcessHostEnvironment::capture(environment)?);
    let watched = StartupObservedRunner {
        store,
        runner: &native,
        run: run.id(),
        tool,
        prepared: &prepared,
        requests: &requests,
        decisions: &decisions,
        before_attempts: &before_attempts,
        before_events: &before_events,
        before_tools: &before_tools,
        calls: AtomicUsize::new(0),
    };
    let approval = Arc::new(AgentApprovalBuffer::new());
    approval.activate_project(project);
    let query = GetAgentApprovalCenter::new(
        store.clone(),
        store.clone(),
        store.clone(),
        approval.clone(),
    );
    let control = ControlAgentApproval::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        approval.clone(),
    );
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let coordinator = WorktreeMutationCoordinator::new();
    let patch = WorkspacePatchAdapter::new();
    let process = WorkspaceProcessRunner::default();
    let context = UnexpectedContext::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        approval.as_ref(),
        &patch,
        &process,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        refresh,
    )
    .with_permissions(store.as_ref());
    let authority = LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile.clone(), Vec::new(), Vec::new());
    let policy = WorkspacePolicy::unrestricted();
    let permissions = store.load_agent_permissions().await?;
    assert_eq!(permissions.mode(), AgentPermissionMode::AskPermissions);
    let expires_at = AgentRunTimestamp::from_unix_millis(
        run.created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            .ok_or("A04 original deadline overflow")?,
    )?;
    let mut sequence = controller
        .begin_loopback_approval(
            project,
            run,
            ledger,
            version,
            LoopbackApprovalAdmission {
                execution: LoopbackVerificationRequest {
                    step_id: HTTP_STEP,
                    settings,
                    probe_approvals: &probes,
                    ids,
                    observed_at: clock.now()?,
                },
                issuance: &issuance,
                resume_event_id: RunEventId::from_bytes(identity("p7.a04.http.resume-event", 0)),
                expires_at,
            },
            authority,
            &watched,
            &policy,
            &seed,
            compiler,
            &ActiveControl,
        )
        .await?;

    let mut evidence = None;
    for (index, &request_id) in requests.iter().enumerate() {
        watched.no_new_effects(project).await?;
        assert_eq!(store.load_agent_permissions().await?, permissions);
        assert_eq!(sequence.next_request_id(), Some(request_id));
        assert_eq!(run.state(), AgentControllerState::AwaitApproval);
        let pending = visible(&query, project, task, clock.now()?).await?;
        assert_eq!(pending.status(), AgentApprovalStatus::Pending);
        assert_eq!(pending.ledger_revision(), ledger.revision().get());
        assert_eq!(pending.ledger_store_version(), *version);
        assert!(pending.can_allow_once());
        assert!(!pending.can_continue());
        assert_eq!(
            ledger
                .step(HTTP_STEP)
                .ok_or("A04 waiting HTTP step missing")?
                .definition()
                .verification_spec(),
            &spec
        );
        let presentation = pending.presentation();
        assert_eq!(presentation.request_id(), request_id);
        let anchors = presentation.context();
        assert_eq!(anchors.task_id(), task);
        assert_eq!(anchors.run_id(), run.id());
        assert_eq!(anchors.step_id(), HTTP_STEP);
        assert_eq!(anchors.verification_spec_id(), spec.id());
        assert_eq!(anchors.snapshot_id(), prepared.snapshot());
        assert_eq!(presentation.expires_at(), expires_at);
        assert_action(presentation.action(), &prepared, index)?;
        let action = &actions[index];
        if let PolicyAction::LoopbackStartup {
            worktree_id,
            step_id,
            target_id,
            ..
        } = action
        {
            assert_eq!(*worktree_id, project.worktree().id());
            assert_eq!(*step_id, HTTP_STEP);
            assert_eq!(*target_id, prepared.effect_resource());
        }
        assert_eq!(presentation.action_fingerprint(), action.fingerprint());
        assert_eq!(presentation.scope_digest(), action.scope_digest());
        assert_eq!(presentation.action_class(), action.class());
        assert_eq!(presentation.risk_level(), action.risk());
        let durable = store
            .load_approval_request(project, request_id)
            .await?
            .ok_or("A04 displayed request is not durable")?;
        assert_eq!(durable.action_fingerprint(), action.fingerprint());
        assert_eq!(durable.scope_digest(), action.scope_digest());
        for future in &requests[index + 1..] {
            assert!(
                store
                    .load_approval_request(project, *future)
                    .await?
                    .is_none()
            );
        }
        let grant = control
            .execute(
                project,
                task,
                presentation.revision(),
                pending.ledger_revision(),
                pending.ledger_store_version(),
                AgentApprovalControlAction::AllowOnce,
                AgentApprovalControlMetadata::new(
                    ApprovalId::from_bytes(identity("p7.a04.http.grant", u32::try_from(index)?)),
                    RunEventId::from_bytes(identity(
                        "p7.a04.http.grant-event",
                        u32::try_from(index)?,
                    )),
                    clock.now()?,
                ),
                &ActiveControl,
            )
            .await?;
        let AgentApprovalControlResult::Applied(AgentApprovalControlOutcome::GrantStored {
            approval_revision,
        }) = grant
        else {
            return Err("A04 AllowOnce did not grant the displayed exact action".into());
        };
        assert_ne!(approval_revision, presentation.revision());
        *run = store
            .load_agent_run(project, run.id())
            .await?
            .ok_or("A04 run missing after grant")?;
        watched.no_new_effects(project).await?;
        for prior in &requests[..=index] {
            let granted = store
                .load_approval_for_request(project, *prior)
                .await?
                .ok_or("A04 grant missing")?;
            assert_eq!(granted.state(), ApprovalGrantState::Active);
        }
        let active = visible(&query, project, task, clock.now()?).await?;
        assert_eq!(active.status(), AgentApprovalStatus::Active);
        assert_eq!(active.presentation().revision(), approval_revision);
        assert_eq!(active.ledger_revision(), ledger.revision().get());
        assert_eq!(active.ledger_store_version(), *version);
        assert!(active.can_continue());
        let continued = control
            .execute(
                project,
                task,
                active.presentation().revision(),
                active.ledger_revision(),
                active.ledger_store_version(),
                AgentApprovalControlAction::Continue,
                AgentApprovalControlMetadata::new(
                    ApprovalId::from_bytes(identity(
                        "p7.a04.http.continue-metadata",
                        u32::try_from(index)?,
                    )),
                    RunEventId::from_bytes(identity(
                        "p7.a04.http.continue-metadata-event",
                        u32::try_from(index)?,
                    )),
                    clock.now()?,
                ),
                &ActiveControl,
            )
            .await?;
        let AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::LoopbackContinueReady { continuation },
        ) = continued
        else {
            return Err("A04 explicit Continue did not return its opaque capability".into());
        };
        watched.no_new_effects(project).await?;
        let outcome = controller
            .continue_loopback_approval(
                project,
                run,
                ledger,
                version,
                &mut sequence,
                continuation,
                clock.now()?,
                authority,
                &watched,
                &policy,
                &seed,
                compiler,
                &ActiveControl,
            )
            .await?;
        match outcome {
            MutationControllerOutcome::AwaitingApproval(next) if index + 1 < requests.len() => {
                assert_eq!(next, requests[index + 1]);
                watched.no_new_effects(project).await?;
            }
            MutationControllerOutcome::StepVerified {
                evidence_id,
                snapshot_id,
            } if index + 1 == requests.len() => {
                assert_eq!(snapshot_id, prepared.snapshot());
                evidence = Some(evidence_id);
            }
            other => return Err(format!(
                "A04 HTTP verification did not complete its bounded approval sequence: {other:?}"
            )
            .into()),
        }
    }
    assert_eq!(sequence.next_request_id(), None);
    assert_eq!(watched.calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(
        ledger
            .step(HTTP_STEP)
            .ok_or("A04 verified HTTP step missing")?
            .status(),
        TaskStepStatus::Completed
    );
    assert_eq!(
        store.load_agent_run(project, run.id()).await?.as_ref(),
        Some(&*run)
    );
    assert_eq!(
        store.load_task_ledger(project, task).await?,
        Some(StoredTaskLedger::new(ledger.clone(), *version))
    );
    let after_attempts = store
        .load_agent_mutation_attempts(project, run.id())
        .await?;
    watched.assert_attempt_prefix(&after_attempts);
    let current = after_attempts
        .iter()
        .filter(|attempt| attempt.tool_attempt().tool_run_id() == tool)
        .copied()
        .collect::<Vec<_>>();
    let [attempt] = current.as_slice() else {
        return Err("A04 requires exactly one final HTTP attempt".into());
    };
    assert_eq!(
        attempt.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_eq!(attempt.disposition(), AgentMutationDisposition::Applied);
    let events = journal(store, project, run.id()).await?;
    assert!(events.starts_with(&before_events));
    let tools = tool_events(&events);
    assert!(tools.starts_with(&before_tools));
    assert_eq!(tools.len(), before_tools.len() + 1);
    assert_eq!(
        tools.last().and_then(|event| event.subject()),
        Some(RunEventSubject::Tool(tool))
    );
    evidence.ok_or("A04 final Continue produced no current HTTP evidence".into())
}

async fn visible(
    query: &GetAgentApprovalCenter,
    project: &ProjectIdentity,
    task: TaskId,
    now: AgentRunTimestamp,
) -> TestResult<Box<AgentApprovalCenter>> {
    let AgentApprovalLoadResult::Available(center) =
        query.execute(project, task, now, &ActiveControl).await?
    else {
        return Err("A04 exact approval presentation unavailable".into());
    };
    Ok(center)
}

fn assert_action(
    action: &AgentApprovalAction,
    prepared: &PreparedLoopbackStartupCoverage,
    stage: usize,
) -> TestResult {
    let count = prepared.recipe().checks().len();
    match (stage, action) {
        (
            0..=2,
            AgentApprovalAction::LoopbackStartup {
                phase,
                process,
                port,
            },
        ) => {
            let expected = match stage {
                0 => LoopbackStartupPhase::ReserveEndpoint,
                1 => LoopbackStartupPhase::HeldExplicit,
                _ => LoopbackStartupPhase::FreeExplicit,
            };
            assert_eq!(*phase, expected);
            assert_eq!(*port, prepared.recipe().port());
            if expected == LoopbackStartupPhase::ReserveEndpoint {
                assert!(process.is_none());
            } else {
                assert_process(
                    process
                        .as_ref()
                        .ok_or("A04 explicit process projection missing")?,
                    prepared
                        .process(expected)
                        .ok_or("A04 explicit process missing")?,
                );
            }
        }
        (
            index,
            AgentApprovalAction::LoopbackStartup {
                phase,
                process,
                port,
            },
        ) if index == count + 3 || index == count + 5 => {
            let expected = if index == count + 3 {
                LoopbackStartupPhase::Default
            } else {
                LoopbackStartupPhase::InvalidPort
            };
            assert_eq!(*phase, expected);
            assert_eq!(
                port.get(),
                if expected == LoopbackStartupPhase::Default {
                    LOOPBACK_DEFAULT_PORT
                } else {
                    prepared.recipe().port().get()
                }
            );
            assert_process(
                process
                    .as_ref()
                    .ok_or("A04 default/invalid process projection missing")?,
                prepared
                    .process(expected)
                    .ok_or("A04 default/invalid process missing")?,
            );
        }
        (index, AgentApprovalAction::LoopbackProbe { method, url })
            if (3..3 + count).contains(&index) || index == count + 4 =>
        {
            let default = index == count + 4;
            let check = if default {
                usize::from(
                    prepared
                        .recipe()
                        .default_start_check_index()
                        .ok_or("A04 default check missing")?,
                )
            } else {
                index - 3
            };
            let probe = prepared
                .recipe()
                .checks()
                .get(check)
                .ok_or("A04 current HTTP probe missing")?
                .probe();
            assert_eq!(*method, probe.method());
            let port = if default {
                LOOPBACK_DEFAULT_PORT
            } else {
                prepared.recipe().port().get()
            };
            assert_eq!(url, &format!("http://127.0.0.1:{port}{}", probe.path()));
        }
        _ => return Err("A04 approval action does not match the exact frozen phase order".into()),
    }
    Ok(())
}

fn assert_process(actual: &AgentApprovalProcess, spec: &ProcessSpec) {
    assert_eq!(actual.specification_id(), spec.specification_id());
    assert_eq!(actual.executable(), spec.executable().as_str());
    assert!(
        actual
            .arguments()
            .iter()
            .map(String::as_str)
            .eq(spec.arguments().iter().map(|arg| arg.as_str()))
    );
    assert!(actual.working_directory().path().is_none());
    assert!(
        actual
            .environment_allowlist()
            .iter()
            .map(String::as_str)
            .eq(spec
                .environment_allowlist()
                .iter()
                .map(|name| name.as_str()))
    );
    assert_eq!(actual.timeout_millis(), 12_000);
    assert_eq!(actual.stdout_limit(), 1024);
    assert_eq!(actual.stderr_limit(), 1024);
    assert_eq!(actual.execution_mode(), ProcessExecutionMode::Open);
    assert_eq!(
        actual.plan_binding(),
        ProcessPlanBinding::Validated(HTTP_STEP)
    );
    assert_eq!(
        actual.network(),
        match spec.network() {
            ProcessNetworkScope::Denied => AgentApprovalNetworkScope::Denied,
            ProcessNetworkScope::Requested(target) => AgentApprovalNetworkScope::Requested(target),
        }
    );
}

async fn journal(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: AgentRunId,
) -> TestResult<Vec<RunEvent>> {
    let mut result = Vec::new();
    let mut cursor = None;
    for _ in 0..16 {
        let page = store
            .load_run_events(project, run, cursor, RunEventPageLimit::new(256)?)
            .await?;
        result.extend_from_slice(page.events());
        if !page.has_more() {
            return Ok(result);
        }
        cursor = Some(
            page.events()
                .last()
                .ok_or("A04 journal continuation is empty")?
                .sequence(),
        );
    }
    Err("A04 fixture journal exceeded its bounded inspection allowance".into())
}

fn tool_events(events: &[RunEvent]) -> Vec<RunEvent> {
    events
        .iter()
        .filter(|event| event.kind() == RunEventKind::ToolAction)
        .cloned()
        .collect()
}

#[derive(Debug)]
struct StartupObservedRunner<'a> {
    store: &'a LibsqlKnowledgeStore,
    runner: &'a WorkspaceLoopbackVerificationRunner,
    run: AgentRunId,
    tool: ToolRunId,
    prepared: &'a PreparedLoopbackStartupCoverage,
    requests: &'a [ApprovalRequestId],
    decisions: &'a [PolicyDecisionId],
    before_attempts: &'a [AgentMutationAttempt],
    before_events: &'a [RunEvent],
    before_tools: &'a [RunEvent],
    calls: AtomicUsize,
}

impl StartupObservedRunner<'_> {
    async fn no_new_effects(&self, project: &ProjectIdentity) -> TestResult {
        assert_eq!(self.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            self.store
                .load_agent_mutation_attempts(project, self.run)
                .await?,
            self.before_attempts
        );
        let events = journal(self.store, project, self.run).await?;
        assert!(events.starts_with(self.before_events));
        assert_eq!(tool_events(&events), self.before_tools);
        Ok(())
    }

    fn assert_attempt_prefix(&self, attempts: &[AgentMutationAttempt]) {
        let prior = attempts
            .iter()
            .filter(|attempt| attempt.tool_attempt().tool_run_id() != self.tool)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(prior, self.before_attempts);
        assert_eq!(attempts.len(), self.before_attempts.len() + 1);
    }
}

impl LoopbackVerificationRunner for StartupObservedRunner<'_> {
    fn supports_owned_cleanup(&self) -> bool {
        self.runner.supports_owned_cleanup()
    }
    fn supports_startup_coverage(&self) -> bool {
        self.runner.supports_startup_coverage()
    }

    fn verify_startup<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackStartupCoverage,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackStartupVerificationFuture<'a> {
        Box::pin(async move {
            assert_eq!(self.calls.fetch_add(1, Ordering::SeqCst), 0);
            assert_eq!(authorized.prepared(), self.prepared);
            let Ok(attempts) = self
                .store
                .load_agent_mutation_attempts(project, self.run)
                .await
            else {
                return LoopbackStartupVerificationOutcome::NotStarted(
                    LoopbackVerificationFailure::Unavailable,
                );
            };
            self.assert_attempt_prefix(&attempts);
            let current = attempts
                .iter()
                .filter(|attempt| attempt.tool_attempt().tool_run_id() == self.tool)
                .copied()
                .collect::<Vec<_>>();
            let [attempt] = current.as_slice() else {
                return LoopbackStartupVerificationOutcome::NotStarted(
                    LoopbackVerificationFailure::Denied,
                );
            };
            assert_eq!(attempt.fingerprint(), self.prepared.effect_fingerprint());
            assert_eq!(attempt.kind(), AgentMutationKind::MachineProcess);
            assert_eq!(attempt.tool_attempt().run_id(), self.run);
            assert_eq!(
                attempt.tool_attempt().snapshot_id(),
                self.prepared.snapshot()
            );
            assert_eq!(
                attempt.tool_attempt().status(),
                AgentToolAttemptStatus::InFlight
            );
            assert!(attempt.disposition().requires_reconciliation());
            let Ok(Some(scope)) = self
                .store
                .load_machine_effect_scope(project, self.tool, attempt.tool_attempt().attempt())
                .await
            else {
                return LoopbackStartupVerificationOutcome::NotStarted(
                    LoopbackVerificationFailure::Unavailable,
                );
            };
            assert_eq!(scope.step(), HTTP_STEP);
            assert_eq!(scope.kind(), AgentMutationKind::MachineProcess);
            assert_eq!(scope.resource(), self.prepared.effect_resource());
            assert_eq!(scope.target(), "python");
            assert_eq!(
                scope.expected(),
                Some(self.prepared.script().content_hash())
            );
            assert!(scope.proposed().is_none());
            let Ok(events) = journal(self.store, project, self.run).await else {
                return LoopbackStartupVerificationOutcome::NotStarted(
                    LoopbackVerificationFailure::Unavailable,
                );
            };
            assert!(events.starts_with(self.before_events));
            assert_eq!(tool_events(&events), self.before_tools);
            assert_eq!(self.requests.len(), 12);
            assert_eq!(self.decisions.len(), 12);
            for (&request, &decision) in self.requests.iter().zip(self.decisions) {
                let grant = self.store.load_approval_for_request(project, request).await;
                assert!(matches!(grant, Ok(Some(grant)) if matches!(grant.state(),
                    ApprovalGrantState::Consumed { decision_id, .. } if decision_id == decision)));
            }
            self.runner
                .verify_startup(project, authorized, control)
                .await
        })
    }

    fn verify<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: AuthorizedLoopbackVerification,
        _: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a> {
        Box::pin(async {
            LoopbackVerificationOutcome::NotStarted(LoopbackVerificationFailure::Denied)
        })
    }
}
