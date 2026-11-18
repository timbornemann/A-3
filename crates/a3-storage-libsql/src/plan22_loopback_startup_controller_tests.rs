//! Complete startup observations through the one mutation controller and native storage.

use super::*;
#[path = "plan22_loopback_startup_approval_tests.rs"]
mod approvals;
#[path = "plan22_loopback_mismatch_tests.rs"]
mod mismatch;
use a3_application::{
    AuthorizedLoopbackStartupCoverage, LoopbackStartupVerificationFuture,
    LoopbackStartupVerificationOutcome, PreparedLoopbackStartupCoverage,
};

const STARTUP_SERVER: &str = r"from argparse import ArgumentParser
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b'RESPONSE\n'
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        self.send_response(405)
        self.send_header('Allow', 'GET')
        self.send_header('Content-Length', '0')
        self.end_headers()

parser = ArgumentParser()
parser.add_argument('--host', default='127.0.0.1')
parser.add_argument('--port', default=8765, type=int)
args = parser.parse_args()
with HTTPServer((args.host, args.port), Handler) as server:
    server.serve_forever()
";

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn startup_controller_observes_all_four_starts_and_reopens_into_done() -> TestResult {
    crate::run_native_libsql_test(run_startup_case(Case::Pass))
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn startup_controller_resumes_e6_without_repeating_any_start() -> TestResult {
    crate::run_native_libsql_test(run_startup_case(Case::ReceiptRecovery { stale: false }))
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn startup_controller_rejects_e6_resume_after_changed_publication() -> TestResult {
    crate::run_native_libsql_test(run_startup_case(Case::ReceiptRecovery { stale: true }))
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn startup_controller_partial_failure_cannot_create_complete_evidence() -> TestResult {
    crate::run_native_libsql_test(run_startup_case(Case::WrongBody))
}

#[test]
fn startup_controller_unknown_cleanup_remains_unreplayable_after_reopen() -> TestResult {
    crate::run_native_libsql_test(run_startup_case(Case::Unknown))
}

async fn prepare_startup_fixture(
    case: Case,
) -> TestResult<PreparedHttpFixture<PreparedLoopbackStartupCoverage>> {
    prepare_http_fixture_using(
        case,
        true,
        STARTUP_SERVER,
        |start: ProcessSpec, script, snapshot, recipe| {
            let a3_domain::ProcessPlanBinding::Validated(step) = start.plan_binding() else {
                return Err(LoopbackVerificationFailure::Denied);
            };
            let settings = LoopbackHttpProcess::from_spec(&start)
                .map_err(|_| LoopbackVerificationFailure::Denied)?
                .settings();
            PreparedLoopbackStartupCoverage::new(
                start.run_id(),
                start.worktree_id(),
                step,
                snapshot,
                script,
                recipe,
                settings,
            )
        },
    )
    .await
}

async fn run_startup_case(case: Case) -> TestResult {
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
    } = prepare_startup_fixture(case).await?;
    let (mut ledger, mut version) = stored.into_parts();
    let snapshot = prepared.snapshot();
    let recipe = prepared.recipe().clone();
    let effect = prepared.effect_fingerprint();
    let actions = prepared.policy_actions();
    let mut requests = vec![START_REQUEST];
    requests.extend((0_u8..7).map(|i| ApprovalRequestId::from_bytes([150 + i; 32])));
    let grants: Vec<_> = (0_u8..7)
        .map(|i| LoopbackProbeApproval {
            request_id: requests[usize::from(i) + 1],
            decision_id: PolicyDecisionId::from_bytes([180 + i; 32]),
            event_id: RunEventId::from_bytes([200 + i; 32]),
        })
        .collect();
    assert_eq!(actions.len(), requests.len());
    let policy = WorkspacePolicy::unrestricted();
    for (index, (action, request)) in actions.iter().zip(&requests).enumerate() {
        grant_recipe_action(
            &fixture,
            &store,
            &mut run,
            &policy,
            action,
            *request,
            10 + 5 * u8::try_from(index)?,
        )
        .await?;
    }
    let approval = AgentApprovalBuffer::new();
    approval.activate_project(&fixture.project);
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    #[cfg(any(windows, target_os = "linux"))]
    let native = native_http_runner()?;
    #[cfg(not(any(windows, target_os = "linux")))]
    let native = ControlledRunner(Case::Unknown);
    let runner = StartupObservedRunner {
        store: &store,
        runner: &native,
        requests: &requests,
        decisions: std::iter::once(PolicyDecisionId::from_bytes([110; 32]))
            .chain(grants.iter().map(|grant| grant.decision_id))
            .collect(),
        fingerprint: effect,
        unknown: case == Case::Unknown,
        calls: AtomicUsize::new(0),
    };
    let coordinator = WorktreeMutationCoordinator::new();
    let patch = WorkspacePatchAdapter::new();
    let process = WorkspaceProcessRunner::default();
    let context = UnexpectedContext::default();
    let seed = MutationContextSeed::new(goal.clone(), model, Vec::new(), Vec::new());
    let interrupted = FailVerificationCommit {
        store: &store,
        calls: AtomicUsize::new(0),
    };
    let action_store: &dyn AgentActionStore = if case.recovering() {
        &interrupted
    } else {
        store.as_ref()
    };
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        action_store,
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approval,
        &patch,
        &process,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let authority = LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref());
    let request = LoopbackVerificationRequest {
        step_id: step,
        settings,
        probe_approvals: &grants,
        ids: execution_ids(110),
        observed_at: at(100)?,
    };
    // Held and free use identical argv, yet their grants cannot exchange positions.
    if case == Case::Pass {
        let mut swapped = grants.clone();
        swapped.swap(0, 1);
        let tail = run.last_event_sequence();
        let result = controller
            .execute_loopback_verification(
                &fixture.project,
                &mut run,
                &mut ledger,
                &mut version,
                LoopbackVerificationRequest {
                    probe_approvals: &swapped,
                    ..request
                },
                authority,
                &runner,
                &policy,
                &seed,
                &mut compiler,
                &ActiveControl,
            )
            .await;
        assert!(matches!(
            result,
            Err(MutationControllerFailure::LoopbackAdmission(
                LoopbackVerificationFailure::Denied
            ))
        ));
        assert_eq!(run.last_event_sequence(), tail);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
        assert!(
            store
                .load_agent_mutation_attempts(&fixture.project, run.id())
                .await?
                .is_empty()
        );
    }
    let result = controller
        .execute_loopback_verification(
            &fixture.project,
            &mut run,
            &mut ledger,
            &mut version,
            request,
            authority,
            &runner,
            &policy,
            &seed,
            &mut compiler,
            &ActiveControl,
        )
        .await;
    let mut completed_evidence = None;
    if case.recovering() {
        assert!(matches!(
            result,
            Err(MutationControllerFailure::LoopbackStepCommit(
                AgentActionStoreFailure::LedgerVersionConflict
            ))
        ));
        assert_eq!(interrupted.calls.load(Ordering::SeqCst), 1);
    } else {
        match (case, result?) {
            (
                Case::Pass,
                MutationControllerOutcome::StepVerified {
                    evidence_id,
                    snapshot_id,
                },
            ) => {
                assert_eq!(snapshot_id, snapshot);
                completed_evidence = Some(evidence_id);
            }
            (
                Case::WrongBody,
                MutationControllerOutcome::Stopped {
                    failure: a3_application::MutationFailureClass::VerificationFailed,
                    ..
                },
            ) => {}
            (Case::Unknown, MutationControllerOutcome::ReconciliationRequired { .. }) => {}
            (_, other) => return Err(format!("unexpected aggregate outcome: {other:?}").into()),
        }
    }
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inventory(&root)?, before);
    let attempts = store
        .load_agent_mutation_attempts(&fixture.project, run.id())
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("one aggregate effect required".into());
    };
    let mismatch = store
        .load_loopback_mismatch(
            &fixture.project,
            run.id(),
            TOOL,
            attempt.tool_attempt().attempt(),
        )
        .await?;
    if case == Case::WrongBody {
        let diagnostic = mismatch
            .as_ref()
            .ok_or("actual HTTP mismatch not retained")?;
        assert_eq!(
            diagnostic.mismatch().phase(),
            a3_domain::LoopbackStartupPhase::FreeExplicit
        );
        assert_eq!(diagnostic.mismatch().check_index(), 0);
        assert_eq!(diagnostic.mismatch().observation().status(), 200);
        assert_eq!(
            diagnostic.mismatch().observation().body_hash(),
            hash(b"Broke\n")
        );
        assert_eq!(diagnostic.mismatch().observation().body_bytes(), 6);
        assert_eq!(diagnostic.context().snapshot_id, snapshot);
        assert_eq!(diagnostic.recipe_id(), recipe.id());
        assert_eq!(diagnostic.decisions(), runner.decisions);
        assert_eq!(
            attempt.tool_attempt().status(),
            AgentToolAttemptStatus::Failed
        );
        assert_eq!(attempt.disposition(), AgentMutationDisposition::Applied);
        assert_eq!(run.state(), AgentControllerState::Failed);
    } else {
        assert!(mismatch.is_none());
    }
    assert_eq!(attempt.fingerprint(), effect);
    let id = match completed_evidence {
        Some(id) => Some(id),
        None => {
            store
                .load_settled_loopback_evidence_id(
                    &fixture.project,
                    run.id(),
                    TOOL,
                    attempt.tool_attempt().attempt(),
                )
                .await?
        }
    };
    let receipt = if case == Case::Pass || case.recovering() {
        let id = id.ok_or("complete aggregate evidence missing")?;
        let state = store
            .load_verification_state(
                &fixture.project,
                goal.task_id(),
                &[id],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let [VerificationEvidence::LoopbackHttp(receipt)] = state.evidence() else {
            return Err("typed aggregate receipt missing".into());
        };
        assert!(receipt.matches(&recipe));
        assert!(receipt.startup().is_some());
        assert_eq!(receipt.context().spec_id, spec.id());
        Some(receipt.clone())
    } else {
        assert!(id.is_none());
        assert_ne!(
            ledger.step(step).ok_or("missing step")?.status(),
            TaskStepStatus::Completed
        );
        None
    };
    let run_id = run.id();
    let calls = runner.calls.load(Ordering::SeqCst);
    drop(runner);
    drop((compiler, refresh));
    drop(store);
    if let Case::ReceiptRecovery { stale } = case {
        resume_settled_receipt(
            &fixture,
            goal,
            &seed,
            &receipt.ok_or("missing recovery receipt")?,
            stale,
        )
        .await?;
    } else {
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_loopback_mismatch(
                    &fixture.project,
                    run_id,
                    TOOL,
                    attempt.tool_attempt().attempt()
                )
                .await?,
            mismatch
        );
        assert_eq!(
            reopened.load_agent_run(&fixture.project, run_id).await?,
            Some(run.clone())
        );
        let stored = reopened
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("reopened ledger missing")?;
        assert_eq!(stored.ledger(), &ledger);
        if case == Case::Pass {
            let id = receipt.ok_or("missing pass receipt")?.id();
            let accepted_at =
                assert_completed_http_recovery(&fixture, &reopened, &run, &stored, id, false)
                    .await?
                    .ok_or("no recovered acceptance")?;
            drop(reopened);
            finish_reopened_source(&fixture, goal, id, snapshot, accepted_at).await?;
        } else if case == Case::Unknown {
            let inspection =
                InspectAgentRunRecovery::new(&reopened, &reopened, &reopened, &reopened)
                    .execute(
                        &fixture.project,
                        run_id,
                        at(run.updated_at().unix_millis() + 1)?,
                        &ActiveControl,
                    )
                    .await?;
            assert!(inspection.mutation_reconciliation_required());
            assert!(!inspection.can_resume());
            assert_eq!(
                reopened
                    .begin_agent_mutation_attempt(
                        &fixture.project,
                        run_id,
                        snapshot,
                        ToolRunId::from_bytes([240; 32]),
                        effect,
                        AgentMutationKind::Patch,
                        at(run.updated_at().unix_millis() + 1)?
                    )
                    .await,
                Err(AgentRecoveryStoreFailure::MutationReconciliationRequired)
            );
        }
    }
    assert_eq!(calls, 1);
    fixture.cleanup()?;
    Ok(())
}

#[derive(Debug)]
struct StartupObservedRunner<'a> {
    store: &'a LibsqlKnowledgeStore,
    runner: &'a dyn LoopbackVerificationRunner,
    requests: &'a [ApprovalRequestId],
    decisions: Vec<PolicyDecisionId>,
    fingerprint: MutationActionFingerprint,
    unknown: bool,
    calls: AtomicUsize,
}

impl LoopbackVerificationRunner for StartupObservedRunner<'_> {
    fn supports_owned_cleanup(&self) -> bool {
        true
    }
    fn supports_startup_coverage(&self) -> bool {
        true
    }
    fn verify_startup<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackStartupCoverage,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackStartupVerificationFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let attempts = self
                .store
                .load_agent_mutation_attempts(project, authorized.prepared().start().run_id())
                .await;
            assert!(
                matches!(attempts.as_deref(), Ok([attempt]) if attempt.fingerprint() == self.fingerprint
                && attempt.tool_attempt().status() == AgentToolAttemptStatus::InFlight
                && attempt.disposition().requires_reconciliation())
            );
            for (&request, &expected) in self.requests.iter().zip(&self.decisions) {
                let grant = self.store.load_approval_for_request(project, request).await;
                assert!(
                    matches!(grant, Ok(Some(grant)) if matches!(grant.state(), ApprovalGrantState::Consumed { decision_id, .. } if decision_id == expected))
                );
            }
            if self.unknown {
                LoopbackStartupVerificationOutcome::UnknownCleanup
            } else {
                self.runner
                    .verify_startup(project, authorized, control)
                    .await
            }
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
