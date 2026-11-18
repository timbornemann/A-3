//! Complete startup approvals through the real approval center, policy store and runtime.

use super::*;
use a3_application::{
    AgentApprovalAction, AgentApprovalCenter, AgentApprovalControlAction,
    AgentApprovalControlMetadata, AgentApprovalControlOutcome, AgentApprovalControlResult,
    AgentApprovalLoadResult, AgentApprovalNetworkScope, AgentApprovalProcess,
    AgentApprovalSinkFailure, AgentApprovalStatus, ControlAgentApproval, GetAgentApprovalCenter,
    LoopbackApprovalAdmission,
};
use a3_domain::{LOOPBACK_DEFAULT_PORT, LoopbackStartupPhase};

const REQUESTS: [ApprovalRequestId; 8] = [
    START_REQUEST,
    ApprovalRequestId::from_bytes([150; 32]),
    ApprovalRequestId::from_bytes([151; 32]),
    ApprovalRequestId::from_bytes([152; 32]),
    ApprovalRequestId::from_bytes([153; 32]),
    ApprovalRequestId::from_bytes([154; 32]),
    ApprovalRequestId::from_bytes([155; 32]),
    ApprovalRequestId::from_bytes([156; 32]),
];
const PROBE_APPROVALS: [LoopbackProbeApproval; 7] = [
    grant_ids(0),
    grant_ids(1),
    grant_ids(2),
    grant_ids(3),
    grant_ids(4),
    grant_ids(5),
    grant_ids(6),
];
const ISSUANCE: [(PolicyDecisionId, RunEventId); 8] = [
    issuance_ids(0),
    issuance_ids(1),
    issuance_ids(2),
    issuance_ids(3),
    issuance_ids(4),
    issuance_ids(5),
    issuance_ids(6),
    issuance_ids(7),
];

const fn grant_ids(index: u8) -> LoopbackProbeApproval {
    LoopbackProbeApproval {
        request_id: ApprovalRequestId::from_bytes([150 + index; 32]),
        decision_id: PolicyDecisionId::from_bytes([180 + index; 32]),
        event_id: RunEventId::from_bytes([200 + index; 32]),
    }
}

const fn issuance_ids(index: u8) -> (PolicyDecisionId, RunEventId) {
    (
        PolicyDecisionId::from_bytes([20 + index; 32]),
        RunEventId::from_bytes([30 + index; 32]),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Current,
    ForeignPublicationAfterStartGrant,
    ClearedPresentationAfterFinalContinue,
    PermissionRevisionAfterFinalContinue,
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn startup_controller_presents_eight_exact_actions_and_starts_only_after_final_continue()
-> TestResult {
    crate::run_native_libsql_test(async {
        let runner = native_http_runner()?;
        run_approval_scenario(Scenario::Current, &runner).await
    })
}

#[test]
fn startup_controller_stale_reservation_continue_does_not_issue_held_start_or_create_an_attempt()
-> TestResult {
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::ForeignPublicationAfterStartGrant,
        &ControlledRunner(Case::Pass),
    ))
}

#[test]
fn startup_controller_final_continue_with_cleared_presentation_does_not_consume_any_grant()
-> TestResult {
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::ClearedPresentationAfterFinalContinue,
        &ControlledRunner(Case::Pass),
    ))
}

#[test]
fn startup_controller_final_continue_with_new_permission_revision_does_not_consume_any_grant()
-> TestResult {
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::PermissionRevisionAfterFinalContinue,
        &ControlledRunner(Case::Pass),
    ))
}

async fn run_approval_scenario(
    scenario: Scenario,
    runner: &dyn LoopbackVerificationRunner,
) -> TestResult {
    let PreparedHttpFixture {
        fixture,
        root,
        before,
        goal,
        spec,
        step,
        store,
        refresh,
        mut compiler,
        model,
        mut run,
        stored,
        settings,
        prepared,
    } = prepare_startup_fixture(Case::Pass).await?;
    let project = &fixture.project;
    let task = goal.task_id();
    let (mut ledger, mut version) = stored.into_parts();
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
    let watched = StartupObservedRunner {
        store: &store,
        runner,
        requests: &REQUESTS,
        decisions: std::iter::once(PolicyDecisionId::from_bytes([110; 32]))
            .chain(PROBE_APPROVALS.iter().map(|grant| grant.decision_id))
            .collect(),
        fingerprint: prepared.effect_fingerprint(),
        unknown: false,
        calls: AtomicUsize::new(0),
    };
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
        &refresh,
    )
    .with_permissions(store.as_ref());
    let authority = LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), model, Vec::new(), Vec::new());
    let policy = WorkspacePolicy::unrestricted();
    let requested_at = at(run.updated_at().unix_millis() + 1)?;
    let expires_at = at(run
        .created_at()
        .unix_millis()
        .checked_add(run.budget().duration_limit().millis())
        .ok_or("fixture run deadline overflow")?)?;
    let mut sequence = controller
        .begin_loopback_approval(
            project,
            &mut run,
            &mut ledger,
            &mut version,
            LoopbackApprovalAdmission {
                execution: LoopbackVerificationRequest {
                    step_id: step,
                    settings,
                    probe_approvals: &PROBE_APPROVALS,
                    ids: execution_ids(110),
                    observed_at: requested_at,
                },
                issuance: &ISSUANCE,
                resume_event_id: RunEventId::from_bytes([126; 32]),
                expires_at,
            },
            authority,
            &watched,
            &policy,
            &seed,
            &mut compiler,
            &ActiveControl,
        )
        .await?;

    let mut evidence = None;
    for (index, request_id) in REQUESTS.into_iter().enumerate() {
        assert_eq!(sequence.next_request_id(), Some(request_id));
        assert_eq!(run.state(), AgentControllerState::AwaitApproval);
        assert_no_effects(&store, project, run.id(), &watched).await?;
        let pending = visible_approval(
            &query,
            project,
            task,
            at(run.updated_at().unix_millis() + 1)?,
        )
        .await?;
        assert_eq!(pending.status(), AgentApprovalStatus::Pending);
        assert_eq!(pending.ledger_store_version(), version);
        assert!(pending.can_allow_once());
        assert!(!pending.can_continue());
        let presentation = pending.presentation();
        assert_eq!(presentation.request_id(), request_id);
        assert_eq!(presentation.context().run_id(), run.id());
        assert_eq!(presentation.context().task_id(), task);
        assert_eq!(presentation.context().step_id(), step);
        assert_eq!(presentation.context().verification_spec_id(), spec.id());
        assert_eq!(presentation.context().snapshot_id(), prepared.snapshot());
        assert_eq!(presentation.expires_at(), expires_at);
        let action = assert_exact_action(presentation.action(), &prepared, index)?;
        assert_eq!(presentation.action_fingerprint(), action.fingerprint());
        assert_eq!(presentation.scope_digest(), action.scope_digest());
        assert_eq!(presentation.action_class(), action.class());
        assert_eq!(presentation.risk_level(), action.risk());
        let durable_request = store
            .load_approval_request(project, request_id)
            .await?
            .ok_or("visible request was not durable")?;
        assert_eq!(durable_request.action_fingerprint(), action.fingerprint());
        assert_eq!(durable_request.scope_digest(), action.scope_digest());
        for future in &REQUESTS[index + 1..] {
            assert!(
                store
                    .load_approval_request(project, *future)
                    .await?
                    .is_none()
            );
        }

        let grant_at = at(run.updated_at().unix_millis() + 2)?;
        let id = u8::try_from(index)? + 40;
        let metadata = AgentApprovalControlMetadata::new(
            ApprovalId::from_bytes([id; 32]),
            RunEventId::from_bytes([id; 32]),
            grant_at,
        );
        let granted = control
            .execute(
                project,
                task,
                presentation.revision(),
                pending.ledger_revision(),
                pending.ledger_store_version(),
                AgentApprovalControlAction::AllowOnce,
                metadata,
                &ActiveControl,
            )
            .await?;
        let AgentApprovalControlResult::Applied(AgentApprovalControlOutcome::GrantStored {
            approval_revision,
        }) = granted
        else {
            return Err("AllowOnce did not store the exact visible grant".into());
        };
        assert_ne!(approval_revision, presentation.revision());
        run = store
            .load_agent_run(project, run.id())
            .await?
            .ok_or("run missing after grant")?;
        assert_eq!(run.state(), AgentControllerState::AwaitApproval);
        assert_no_effects(&store, project, run.id(), &watched).await?;
        for prior in &REQUESTS[..=index] {
            let grant = store
                .load_approval_for_request(project, *prior)
                .await?
                .ok_or("expected live grant")?;
            assert_eq!(grant.state(), ApprovalGrantState::Active);
        }

        let continue_at = at(run.updated_at().unix_millis() + 1)?;
        let metadata = AgentApprovalControlMetadata::new(
            ApprovalId::from_bytes([id + 10; 32]),
            RunEventId::from_bytes([id + 10; 32]),
            continue_at,
        );
        let stale_display = control
            .execute(
                project,
                task,
                presentation.revision(),
                pending.ledger_revision(),
                pending.ledger_store_version(),
                AgentApprovalControlAction::Continue,
                metadata,
                &ActiveControl,
            )
            .await?;
        assert!(matches!(
            stale_display,
            AgentApprovalControlResult::ActivityChanged
        ));
        let active = visible_approval(&query, project, task, continue_at).await?;
        assert_eq!(active.status(), AgentApprovalStatus::Active);
        assert_eq!(active.presentation().revision(), approval_revision);
        assert_eq!(active.presentation().request_id(), request_id);
        assert!(active.can_continue());
        let continuation = control
            .execute(
                project,
                task,
                active.presentation().revision(),
                active.ledger_revision(),
                active.ledger_store_version(),
                AgentApprovalControlAction::Continue,
                metadata,
                &ActiveControl,
            )
            .await?;
        let AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::LoopbackContinueReady { continuation },
        ) = continuation
        else {
            return Err("explicit Continue did not yield the bound loopback capability".into());
        };
        assert_no_effects(&store, project, run.id(), &watched).await?;
        let invalidated = match (scenario, index) {
            (Scenario::ForeignPublicationAfterStartGrant, 0) => {
                fs::write(
                    root.join("server.py"),
                    STARTUP_SERVER.replace("RESPONSE", "Broke"),
                )?;
                let changed = refresh
                    .execute(
                        project,
                        &RepositoryChangeBatch::full_rescan(
                            Vec::new(),
                            RepositoryRescanReason::Explicit,
                        )?,
                        &mut compiler,
                        &ActiveControl,
                    )
                    .await?;
                assert_ne!(
                    changed.published_index().run().snapshot_id(),
                    prepared.snapshot()
                );
                true
            }
            (Scenario::ClearedPresentationAfterFinalContinue, 7) => {
                approval.deactivate_project();
                approval.activate_project(project);
                assert!(approval.presentation(project, task)?.is_none());
                true
            }
            (Scenario::PermissionRevisionAfterFinalContinue, 7) => {
                let original = store.load_agent_permissions().await?;
                assert_eq!(original.mode(), AgentPermissionMode::AskPermissions);
                let changed = store
                    .update_agent_permissions(original.revision(), AgentPermissionMode::FullMachine)
                    .await?;
                let restored = store
                    .update_agent_permissions(changed.revision(), original.mode())
                    .await?;
                assert_eq!(restored.mode(), original.mode());
                assert_ne!(restored.revision(), original.revision());
                true
            }
            _ => false,
        };
        let previous_sequence = run.last_event_sequence();
        let previous_version = version;
        let continued = controller
            .continue_loopback_approval(
                project,
                &mut run,
                &mut ledger,
                &mut version,
                &mut sequence,
                continuation,
                continue_at,
                authority,
                &watched,
                &policy,
                &seed,
                &mut compiler,
                &ActiveControl,
            )
            .await;
        if invalidated {
            assert!(
                matches!(
                    (scenario, &continued),
                    (
                        Scenario::ForeignPublicationAfterStartGrant,
                        Err(MutationControllerFailure::AnchorMismatch)
                    ) | (
                        Scenario::ClearedPresentationAfterFinalContinue,
                        Err(MutationControllerFailure::ApprovalPresentation(
                            AgentApprovalSinkFailure::AnchorMismatch
                        ))
                    ) | (
                        Scenario::PermissionRevisionAfterFinalContinue,
                        Err(MutationControllerFailure::PermissionsChanged)
                    )
                ),
                "unexpected invalidated continuation result: {continued:?}"
            );
            assert_no_effects(&store, project, run.id(), &watched).await?;
            assert_eq!(sequence.next_request_id(), Some(request_id));
            assert_eq!(run.state(), AgentControllerState::AwaitApproval);
            assert_eq!(run.last_event_sequence(), previous_sequence);
            assert_eq!(version, previous_version);
            assert_eq!(store.load_agent_run(project, run.id()).await?, Some(run));
            assert_eq!(
                store.load_task_ledger(project, task).await?,
                Some(StoredTaskLedger::new(ledger, version))
            );
            for prior in &REQUESTS[..=index] {
                let grant = store
                    .load_approval_for_request(project, *prior)
                    .await?
                    .ok_or("exact grant missing")?;
                assert_eq!(grant.state(), ApprovalGrantState::Active);
            }
            for future in &REQUESTS[index + 1..] {
                assert!(
                    store
                        .load_approval_request(project, *future)
                        .await?
                        .is_none()
                );
            }
            assert_eq!(context.calls.load(Ordering::SeqCst), 0);
            drop((query, control, compiler, refresh, store));
            fixture.cleanup()?;
            return Ok(());
        }
        match continued? {
            MutationControllerOutcome::AwaitingApproval(next) if index < 7 => {
                assert_eq!(next, REQUESTS[index + 1]);
                assert_no_effects(&store, project, run.id(), &watched).await?;
            }
            MutationControllerOutcome::StepVerified {
                evidence_id,
                snapshot_id,
            } if index == 7 => {
                assert_eq!(snapshot_id, prepared.snapshot());
                evidence = Some(evidence_id);
            }
            other => {
                return Err(format!("unexpected sequential approval outcome: {other:?}").into());
            }
        }
    }
    assert_eq!(scenario, Scenario::Current);
    assert_eq!(watched.calls.load(Ordering::SeqCst), 1);
    assert_eq!(sequence.next_request_id(), None);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(inventory(&root)?, before);
    assert_listener_gone(
        Case::Pass,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, prepared.recipe().port().get()),
    )?;
    assert_listener_gone(
        Case::Pass,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, LOOPBACK_DEFAULT_PORT),
    )?;
    drop((query, control, compiler, refresh, store));
    finish_reopened_source(
        &fixture,
        goal,
        evidence.ok_or("final Continue did not produce HTTP evidence")?,
        prepared.snapshot(),
        run.updated_at().unix_millis() + 1,
    )
    .await?;
    fixture.cleanup()?;
    Ok(())
}

async fn visible_approval(
    query: &GetAgentApprovalCenter,
    project: &ProjectIdentity,
    task: TaskId,
    at: AgentRunTimestamp,
) -> TestResult<Box<AgentApprovalCenter>> {
    let AgentApprovalLoadResult::Available(center) =
        query.execute(project, task, at, &ActiveControl).await?
    else {
        return Err("approval center did not expose the exact active request".into());
    };
    Ok(center)
}

fn assert_exact_action(
    action: &AgentApprovalAction,
    prepared: &PreparedLoopbackStartupCoverage,
    index: usize,
) -> TestResult<PolicyAction> {
    let policy_action = prepared
        .policy_actions()
        .get(index)
        .ok_or("missing startup policy action")?
        .clone();
    match (index, action) {
        (
            0,
            AgentApprovalAction::LoopbackStartup {
                phase,
                process,
                port,
            },
        ) => {
            assert_eq!(*phase, LoopbackStartupPhase::ReserveEndpoint);
            assert!(process.is_none());
            assert_eq!(*port, prepared.recipe().port());
        }
        (
            1 | 2 | 5 | 7,
            AgentApprovalAction::LoopbackStartup {
                phase,
                process,
                port,
            },
        ) => {
            let expected_phase = match index {
                1 => LoopbackStartupPhase::HeldExplicit,
                2 => LoopbackStartupPhase::FreeExplicit,
                5 => LoopbackStartupPhase::Default,
                7 => LoopbackStartupPhase::InvalidPort,
                _ => return Err("unexpected startup process stage".into()),
            };
            assert_eq!(*phase, expected_phase);
            let spec = prepared
                .process(expected_phase)
                .ok_or("missing exact phase process")?;
            assert_process(
                process.as_ref().ok_or("missing visible phase process")?,
                spec,
            );
            assert_eq!(
                port.get(),
                if index == 5 {
                    LOOPBACK_DEFAULT_PORT
                } else {
                    prepared.recipe().port().get()
                }
            );
        }
        (3 | 4 | 6, AgentApprovalAction::LoopbackProbe { method, url }) => {
            let check_index = if index == 6 {
                usize::from(
                    prepared
                        .recipe()
                        .default_start_check_index()
                        .ok_or("no default check")?,
                )
            } else {
                index - 3
            };
            let probe = prepared
                .recipe()
                .checks()
                .get(check_index)
                .ok_or("missing exact probe")?
                .probe();
            assert_eq!(*method, probe.method());
            let port = if index == 6 {
                LOOPBACK_DEFAULT_PORT
            } else {
                prepared.recipe().port().get()
            };
            assert_eq!(url, &format!("http://127.0.0.1:{port}{}", probe.path()));
        }
        _ => return Err("wrong startup action kind or approval order".into()),
    }
    Ok(policy_action)
}

fn assert_process(process: &AgentApprovalProcess, spec: &ProcessSpec) {
    assert_eq!(process.specification_id(), spec.specification_id());
    assert_eq!(process.executable(), spec.executable().as_str());
    assert!(
        process
            .arguments()
            .iter()
            .map(String::as_str)
            .eq(spec.arguments().iter().map(ProcessArgument::as_str))
    );
    assert!(process.working_directory().path().is_none());
    assert!(
        process
            .environment_allowlist()
            .iter()
            .map(String::as_str)
            .eq(spec
                .environment_allowlist()
                .iter()
                .map(ProcessEnvironmentVariable::as_str))
    );
    assert_eq!(process.timeout_millis(), spec.timeout().as_millis());
    assert_eq!(process.stdout_limit(), spec.stdout_limit().get());
    assert_eq!(process.stderr_limit(), spec.stderr_limit().get());
    assert_eq!(process.execution_mode(), spec.execution_mode());
    assert_eq!(process.plan_binding(), spec.plan_binding());
    assert_eq!(
        process.network(),
        match spec.network() {
            a3_domain::ProcessNetworkScope::Denied => AgentApprovalNetworkScope::Denied,
            a3_domain::ProcessNetworkScope::Requested(scope) =>
                AgentApprovalNetworkScope::Requested(scope),
        }
    );
}

async fn assert_no_effects(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: AgentRunId,
    watched: &StartupObservedRunner<'_>,
) -> TestResult {
    assert_eq!(watched.calls.load(Ordering::SeqCst), 0);
    assert!(
        store
            .load_agent_mutation_attempts(project, run)
            .await?
            .is_empty()
    );
    let history = store
        .load_run_events(project, run, None, RunEventPageLimit::new(128)?)
        .await?;
    assert!(
        history
            .events()
            .iter()
            .all(|event| event.kind() != RunEventKind::ToolAction)
    );
    Ok(())
}
