//! Candidate HTTP approvals through the real approval center and durable policy store.

use super::*;
use a3_application::{
    AgentApprovalAction, AgentApprovalCenter, AgentApprovalControlAction,
    AgentApprovalControlMetadata, AgentApprovalControlOutcome, AgentApprovalControlResult,
    AgentApprovalLoadResult, AgentApprovalNetworkScope, AgentApprovalSinkFailure,
    AgentApprovalStatus, ControlAgentApproval, GetAgentApprovalCenter, LoopbackApprovalAdmission,
    TaskLensWorkspaceControl,
};

const REQUESTS: [ApprovalRequestId; 3] = [START_REQUEST, PROBE_REQUESTS[0], PROBE_REQUESTS[1]];
const PROBE_APPROVALS: [LoopbackProbeApproval; 2] = [
    LoopbackProbeApproval {
        request_id: PROBE_REQUESTS[0],
        decision_id: USED_DECISIONS[1],
        event_id: RunEventId::from_bytes([133; 32]),
    },
    LoopbackProbeApproval {
        request_id: PROBE_REQUESTS[1],
        decision_id: USED_DECISIONS[2],
        event_id: RunEventId::from_bytes([143; 32]),
    },
];
const ISSUANCE: [(PolicyDecisionId, RunEventId); 3] = [
    (
        PolicyDecisionId::from_bytes([20; 32]),
        RunEventId::from_bytes([21; 32]),
    ),
    (
        PolicyDecisionId::from_bytes([22; 32]),
        RunEventId::from_bytes([23; 32]),
    ),
    (
        PolicyDecisionId::from_bytes([24; 32]),
        RunEventId::from_bytes([25; 32]),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Current,
    ForeignPublicationAfterStartGrant,
    ClearedPresentationAfterFinalContinue,
    PermissionRevisionAfterFinalContinue,
}

#[test]
fn http_controller_presents_start_get_post_in_order_and_starts_only_after_final_continue()
-> TestResult {
    crate::run_native_libsql_test(async {
        #[cfg(any(windows, target_os = "linux"))]
        let runner = native_http_runner()?;
        #[cfg(not(any(windows, target_os = "linux")))]
        let runner = ControlledRunner(Case::Pass);
        run_approval_scenario(Scenario::Current, &runner).await
    })
}

#[test]
fn http_controller_stale_start_continue_does_not_issue_get_or_create_an_attempt() -> TestResult {
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::ForeignPublicationAfterStartGrant,
        &ControlledRunner(Case::Pass),
    ))
}

#[test]
fn http_controller_final_continue_with_cleared_presentation_does_not_consume_grants() -> TestResult
{
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::ClearedPresentationAfterFinalContinue,
        &ControlledRunner(Case::Pass),
    ))
}

#[test]
fn http_controller_final_continue_with_new_permission_revision_does_not_consume_grants()
-> TestResult {
    crate::run_native_libsql_test(run_approval_scenario(
        Scenario::PermissionRevisionAfterFinalContinue,
        &ControlledRunner(Case::Pass),
    ))
}

impl TaskLensWorkspaceControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
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
    } = prepare_http_fixture(Case::Pass).await?;
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
    let watched = ObservedRunner {
        store: &store,
        runner,
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
                fs::write(root.join("server.py"), SERVER.replace("RESPONSE", "Broke"))?;
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
            (Scenario::ClearedPresentationAfterFinalContinue, 2) => {
                approval.deactivate_project();
                approval.activate_project(project);
                assert!(approval.presentation(project, task)?.is_none());
                true
            }
            (Scenario::PermissionRevisionAfterFinalContinue, 2) => {
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
            MutationControllerOutcome::AwaitingApproval(next) if index < 2 => {
                assert_eq!(next, REQUESTS[index + 1]);
                assert_no_effects(&store, project, run.id(), &watched).await?;
            }
            MutationControllerOutcome::StepVerified {
                evidence_id,
                snapshot_id,
            } if index == 2 => {
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
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, prepared.port().get()),
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
    prepared: &PreparedLoopbackVerification,
    index: usize,
) -> TestResult<PolicyAction> {
    match (index, action) {
        (0, AgentApprovalAction::LoopbackStart { process, port }) => {
            let start = prepared.start();
            assert_eq!(*port, prepared.port());
            assert_eq!(process.specification_id(), start.specification_id());
            assert_eq!(process.executable(), start.executable().as_str());
            assert!(
                process
                    .arguments()
                    .iter()
                    .map(String::as_str)
                    .eq(start.arguments().iter().map(ProcessArgument::as_str))
            );
            assert!(process.working_directory().path().is_none());
            assert!(
                process
                    .environment_allowlist()
                    .iter()
                    .map(String::as_str)
                    .eq(start
                        .environment_allowlist()
                        .iter()
                        .map(ProcessEnvironmentVariable::as_str))
            );
            assert_eq!(process.timeout_millis(), start.timeout().as_millis());
            assert_eq!(process.stdout_limit(), start.stdout_limit().get());
            assert_eq!(process.stderr_limit(), start.stderr_limit().get());
            assert_eq!(process.execution_mode(), start.execution_mode());
            assert_eq!(process.plan_binding(), start.plan_binding());
            assert_eq!(
                process.network(),
                AgentApprovalNetworkScope::Requested(PreparedLoopbackVerification::network_target(
                    *port
                ))
            );
            Ok(start.policy_action())
        }
        (1 | 2, AgentApprovalAction::LoopbackProbe { method, url }) => {
            let probe = prepared.recipe().checks()[index - 1].probe();
            assert_eq!(*method, probe.method());
            assert_eq!(
                url,
                &format!("http://127.0.0.1:{}{}", prepared.port(), probe.path())
            );
            prepared
                .probe_policy_action(index - 1)
                .ok_or("missing exact probe policy action".into())
        }
        _ => Err("wrong action kind or approval order".into()),
    }
}

async fn assert_no_effects(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: AgentRunId,
    watched: &ObservedRunner<'_>,
) -> TestResult {
    assert_eq!(watched.calls.load(Ordering::SeqCst), 0);
    assert!(
        store
            .load_agent_mutation_attempts(project, run)
            .await?
            .is_empty()
    );
    let history = store
        .load_run_events(project, run, None, RunEventPageLimit::new(64)?)
        .await?;
    assert!(
        history
            .events()
            .iter()
            .all(|event| event.kind() != RunEventKind::ToolAction)
    );
    Ok(())
}
