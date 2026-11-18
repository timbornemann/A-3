//! Candidate HTTP controller with real policy, journal and ledger persistence.

#[path = "plan22_loopback_approval_tests.rs"]
mod approvals;
#[path = "plan22_loopback_startup_controller_tests.rs"]
mod startup;

use super::*;
use a3_application::{
    AgentActionStoreFuture, AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryOutcomeKind,
    AgentRecoveryStoreFailure, AuthorizedLoopbackVerification, InspectAgentRunRecovery,
    LoopbackProbeApproval, LoopbackVerificationAuthority, LoopbackVerificationFailure,
    LoopbackVerificationFuture, LoopbackVerificationOutcome, LoopbackVerificationRequest,
    LoopbackVerificationResume, LoopbackVerificationRunner, MutationControllerFailure,
    RecoverAgentRun, RunEventPageLimit,
};
use a3_domain::{
    AgentMutationDisposition, ApprovalGrantState, FileRevision, LoopbackProcessSettings, RunEvent,
    RunEventSequence, TaskLedger,
};
#[cfg(any(windows, target_os = "linux"))]
use std::net::TcpListener;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::AtomicUsize;

const START_REQUEST: ApprovalRequestId = ApprovalRequestId::from_bytes([111; 32]);
const TOOL: ToolRunId = ToolRunId::from_bytes([114; 32]);
const PROBE_REQUESTS: [ApprovalRequestId; 2] = [
    ApprovalRequestId::from_bytes([131; 32]),
    ApprovalRequestId::from_bytes([141; 32]),
];
const USED_DECISIONS: [PolicyDecisionId; 3] = [
    PolicyDecisionId::from_bytes([110; 32]),
    PolicyDecisionId::from_bytes([132; 32]),
    PolicyDecisionId::from_bytes([142; 32]),
];

const SERVER: &str = r"from argparse import ArgumentParser
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
parser.add_argument('--host', required=True)
parser.add_argument('--port', required=True, type=int)
args = parser.parse_args()
with HTTPServer((args.host, args.port), Handler) as server:
    server.serve_forever()
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Pass,
    WrongBody,
    Unknown,
    ReceiptRecovery { stale: bool },
}

impl Case {
    const fn body(self) -> &'static str {
        match self {
            Self::WrongBody => "Broke",
            Self::Pass | Self::Unknown | Self::ReceiptRecovery { .. } => "Ready",
        }
    }

    const fn recovering(self) -> bool {
        matches!(self, Self::ReceiptRecovery { .. })
    }
}

#[test]
fn http_controller_passes_exact_observations_and_reopens_into_done() -> TestResult {
    crate::run_native_libsql_test(known_case(Case::Pass))
}

#[test]
fn http_controller_keeps_mismatched_body_evidence_and_never_completes() -> TestResult {
    crate::run_native_libsql_test(known_case(Case::WrongBody))
}

#[test]
fn http_controller_unknown_cleanup_keeps_durable_reconciliation_gate() -> TestResult {
    crate::run_native_libsql_test(run_case(Case::Unknown, &ControlledRunner(Case::Unknown)))
}

#[test]
fn http_controller_resumes_the_same_settled_receipt_after_e6_conflict_and_reopen() -> TestResult {
    let case = Case::ReceiptRecovery { stale: false };
    crate::run_native_libsql_test(run_case(case, &ControlledRunner(case)))
}

#[test]
fn http_controller_cannot_resume_the_settled_receipt_after_a_foreign_publication() -> TestResult {
    let case = Case::ReceiptRecovery { stale: true };
    crate::run_native_libsql_test(run_case(case, &ControlledRunner(case)))
}

#[test]
fn http_recovery_reopens_the_completed_step_after_an_unrelated_publication() -> TestResult {
    crate::run_native_libsql_test(run_case_with_completed_recovery(
        Case::Pass,
        &ControlledRunner(Case::Pass),
        true,
    ))
}

async fn known_case(case: Case) -> TestResult {
    #[cfg(any(windows, target_os = "linux"))]
    {
        run_case(case, &native_http_runner()?).await
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        // Portable adapter observations exercise policy and storage, not OS ownership.
        run_case(case, &ControlledRunner(case)).await
    }
}

#[cfg(any(windows, target_os = "linux"))]
fn native_http_runner() -> TestResult<a3_workspace::WorkspaceLoopbackVerificationRunner> {
    let mut env = vec![ProcessEnvironmentVariable::try_from_string(
        "PATH".to_owned(),
    )?];
    if cfg!(windows) {
        env.push(ProcessEnvironmentVariable::try_from_string(
            "SYSTEMROOT".to_owned(),
        )?);
    }
    Ok(a3_workspace::WorkspaceLoopbackVerificationRunner::new(
        ProcessHostEnvironment::capture(env)?,
    ))
}

async fn run_case(case: Case, runner: &dyn LoopbackVerificationRunner) -> TestResult {
    run_case_with_completed_recovery(case, runner, false).await
}

struct PreparedHttpFixture<P = PreparedLoopbackVerification> {
    fixture: Fixture,
    root: PathBuf,
    before: Vec<(PathBuf, Vec<u8>)>,
    goal: GoalContract,
    spec: VerificationSpec,
    step: TaskStepId,
    store: Arc<LibsqlKnowledgeStore>,
    refresh: RefreshRepositoryIndex,
    compiler: BuiltinIncrementalIndexCompiler,
    model: ModelProfile,
    run: AgentRun,
    stored: StoredTaskLedger,
    settings: LoopbackProcessSettings,
    prepared: P,
}

async fn prepare_http_fixture(case: Case) -> TestResult<PreparedHttpFixture> {
    prepare_http_fixture_using(case, false, SERVER, PreparedLoopbackVerification::new).await
}

async fn prepare_http_fixture_using<P>(
    case: Case,
    aggregate: bool,
    server: &str,
    prepare: impl FnOnce(
        ProcessSpec,
        FileRevision,
        SnapshotId,
        LoopbackHttpRecipe,
    ) -> Result<P, LoopbackVerificationFailure>,
) -> TestResult<PreparedHttpFixture<P>> {
    let mut fixture = Fixture::new()?;
    let root = fixture.root.join("worktree");
    initialize_git(&root)?;
    fs::write(
        root.join("server.py"),
        server.replace("RESPONSE", case.body()),
    )?;
    fixture.project = RepositoryInspector::new().inspect(&root)?;
    let before = inventory(&root)?;
    let base = recipe()?;
    #[cfg(any(windows, target_os = "linux"))]
    let port = if aggregate || matches!(case, Case::Pass | Case::WrongBody) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        NonZeroU16::try_from(listener.local_addr()?.port())?
    } else {
        base.port()
    };
    #[cfg(not(any(windows, target_os = "linux")))]
    let port = if aggregate {
        NonZeroU16::try_from(18_765_u16)?
    } else {
        base.port()
    };
    let mut recipe = LoopbackHttpRecipe::new(
        base.script().clone(),
        LoopbackStart::Explicit,
        port,
        base.checks().to_vec(),
    )?;
    if aggregate {
        recipe = recipe.with_startup_coverage(0)?;
    }
    let goal = goal(recipe.clone())?;
    let step = TaskStepId::from_bytes([STEP; 32]);
    let spec = VerificationSpec::loopback_http(
        VerificationSpecId::from_bytes([213; 32]),
        VerificationRequirement::try_from_string("Verify the exact service responses".to_owned())?,
        recipe.clone(),
    );
    let definition = TaskStepDefinition::new(
        step,
        None,
        TaskStepOutcome::try_from_string("Check the local service".to_owned())?,
        TaskStepRationale::try_from_string("Prove the original HTTP requirement".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Owned HTTP responses".to_owned(),
        )?],
        spec.clone(),
    )?
    .with_criterion_coverage(Vec::new(), vec![goal.draft().acceptance_criteria()[0].id()])?;
    let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
    let snapshot = published.run().snapshot_id();
    let script = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == recipe.script())
        .ok_or("script is missing from the actual published index")?
        .clone();
    let model = profile()?;
    let (run, stored, unplanned) =
        start_fixture_step(&fixture, &store, &goal, &model, snapshot, vec![definition]).await?;
    assert!(unplanned.is_empty());
    let settings = LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(8_000)?,
        stdout_limit: ProcessOutputLimit::new(1024)?,
        stderr_limit: ProcessOutputLimit::new(1024)?,
        system_root: cfg!(windows),
    };
    let start = recipe.process_spec(run.id(), fixture.project.worktree().id(), step, settings)?;
    let prepared = prepare(start, script, snapshot, recipe)?;
    Ok(PreparedHttpFixture {
        fixture,
        root,
        before,
        goal,
        spec,
        step,
        store,
        refresh,
        compiler,
        model,
        run,
        stored,
        settings,
        prepared,
    })
}

async fn run_case_with_completed_recovery(
    case: Case,
    runner: &dyn LoopbackVerificationRunner,
    stale_after_e6: bool,
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
    } = prepare_http_fixture(case).await?;
    let (mut ledger, mut version) = stored.into_parts();
    let recipe = prepared.recipe().clone();
    let snapshot = prepared.snapshot();
    let script = prepared.script().clone();
    let port = prepared.port();
    let policy = WorkspacePolicy::unrestricted();
    let actions = [
        (START_REQUEST, prepared.start().policy_action()),
        (
            PROBE_REQUESTS[0],
            prepared
                .probe_policy_action(0)
                .ok_or("missing GET action")?,
        ),
        (
            PROBE_REQUESTS[1],
            prepared
                .probe_policy_action(1)
                .ok_or("missing POST action")?,
        ),
    ];
    for (index, (request, action)) in actions.iter().enumerate() {
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
    let probe_approvals = [
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
    let approval = AgentApprovalBuffer::new();
    approval.activate_project(&fixture.project);
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let context = UnexpectedContext::default();
    let watched = ObservedRunner {
        store: &store,
        runner,
        calls: AtomicUsize::new(0),
    };
    let coordinator = WorktreeMutationCoordinator::new();
    let patch = WorkspacePatchAdapter::new();
    let process_runner = WorkspaceProcessRunner::default();
    let context_seed = MutationContextSeed::new(goal.clone(), model, Vec::new(), Vec::new());
    let interrupted_commit = FailVerificationCommit {
        store: &store,
        calls: AtomicUsize::new(0),
    };
    let action_store: &dyn AgentActionStore = if case.recovering() {
        &interrupted_commit
    } else {
        store.as_ref()
    };
    let outcome = {
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
            &process_runner,
            &BuiltinProcessVerificationEvidenceFactory,
            &context,
            &refresh,
        )
        .with_permissions(store.as_ref());
        let authority = LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref());
        let request = LoopbackVerificationRequest {
            step_id: step,
            settings,
            probe_approvals: &probe_approvals,
            ids: execution_ids(110),
            observed_at: at(30)?,
        };
        if case == Case::Pass {
            let swapped = [probe_approvals[1], probe_approvals[0]];
            let sequence = run.last_event_sequence();
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
                    &watched,
                    &policy,
                    &context_seed,
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
            assert_eq!(watched.calls.load(Ordering::SeqCst), 0);
            assert_eq!(run.last_event_sequence(), sequence);
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
                &watched,
                &policy,
                &context_seed,
                &mut compiler,
                &ActiveControl,
            )
            .await;
        if case.recovering() {
            let Err(error) = &result else {
                return Err("settled tool action did not hit the injected E6 conflict".into());
            };
            assert!(matches!(
                error,
                MutationControllerFailure::LoopbackStepCommit(
                    AgentActionStoreFailure::LedgerVersionConflict
                )
            ));
            assert_eq!(
                error.mutation_application_state(),
                a3_domain::MutationApplicationState::Applied
            );
            assert_eq!(interrupted_commit.calls.load(Ordering::SeqCst), 1);
        } else if let Err(error) = result {
            return Err(error.into());
        }
        let replay_at = at(run.updated_at().unix_millis() + 1)?;
        let sequence = run.last_event_sequence();
        let replay = controller
            .execute_loopback_verification(
                &fixture.project,
                &mut run,
                &mut ledger,
                &mut version,
                LoopbackVerificationRequest {
                    observed_at: replay_at,
                    ..request
                },
                authority,
                &watched,
                &policy,
                &context_seed,
                &mut compiler,
                &ActiveControl,
            )
            .await;
        assert!(match case {
            Case::Pass | Case::WrongBody =>
                matches!(replay, Err(MutationControllerFailure::AnchorMismatch)),
            Case::Unknown => matches!(
                replay,
                Err(MutationControllerFailure::MutationStartStore(
                    AgentRecoveryStoreFailure::MutationReconciliationRequired
                ))
            ),
            Case::ReceiptRecovery { .. } => matches!(
                replay,
                Err(MutationControllerFailure::LoopbackRecoveryPending)
            ),
        });
        assert_eq!(run.last_event_sequence(), sequence);
        result
    };
    let runner_calls = watched.calls;
    assert_eq!(runner_calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inventory(&root)?, before);
    assert_listener_gone(case, SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.get()))?;
    if let Case::ReceiptRecovery { stale } = case {
        let receipt = LoopbackHttpEvidence::new(
            LoopbackHttpEvidenceContext {
                verification_run_id: VerificationRunId::from_bytes([119; 32]),
                spec_id: spec.id(),
                run_id: run.id(),
                tool_run_id: TOOL,
                step_id: step,
                snapshot_id: snapshot,
            },
            recipe.id(),
            script,
            LoopbackHttpProcess::from_spec(prepared.start())?,
            USED_DECISIONS[0],
            observations(case, [USED_DECISIONS[1], USED_DECISIONS[2]])?.to_vec(),
        )?;
        drop((compiler, refresh, store));
        resume_settled_receipt(&fixture, goal, &context_seed, &receipt, stale).await?;
        if !stale {
            assert_eq!(inventory(&root)?, before);
        }
        fixture.cleanup()?;
        return Ok(());
    }
    let outcome_evidence = match (case, outcome?) {
        (
            Case::Pass,
            MutationControllerOutcome::StepVerified {
                evidence_id,
                snapshot_id,
            },
        ) => {
            assert_eq!(snapshot_id, snapshot);
            Some(evidence_id)
        }
        (
            Case::WrongBody,
            MutationControllerOutcome::ReplanRequired {
                failure: a3_application::MutationFailureClass::VerificationFailed,
                snapshot_id,
            },
        ) => {
            assert_eq!(snapshot_id, snapshot);
            None
        }
        (
            Case::Unknown,
            MutationControllerOutcome::ReconciliationRequired {
                tool_run_id,
                snapshot_id,
                ..
            },
        ) => {
            assert_eq!(tool_run_id, TOOL);
            assert_eq!(snapshot_id, snapshot);
            None
        }
        (_, other) => return Err(format!("unexpected controller outcome: {other:?}").into()),
    };
    let run_id = run.id();
    drop(refresh);
    drop(store);

    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let durable = reopened
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("run missing after reopen")?;
    assert_eq!(durable, run);
    assert_eq!(durable.usage().turn_count(), 0);
    assert_eq!(
        durable.state(),
        match case {
            Case::Pass => AgentControllerState::Verify,
            Case::WrongBody => AgentControllerState::Replan,
            Case::Unknown => AgentControllerState::Execute,
            Case::ReceiptRecovery { .. } =>
                return Err("receipt case should have reopened separately".into()),
        }
    );
    let stored = reopened
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("ledger missing after reopen")?;
    assert_eq!(stored.ledger(), &ledger);
    assert_eq!(stored.version(), version);
    let attempts = reopened
        .load_agent_mutation_attempts(&fixture.project, run_id)
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("expected one durable composite tool attempt".into());
    };
    assert_eq!(attempt.tool_attempt().tool_run_id(), TOOL);
    assert_eq!(attempt.kind(), AgentMutationKind::MachineProcess);
    let history = reopened
        .load_run_events(&fixture.project, run_id, None, RunEventPageLimit::new(64)?)
        .await?;
    let success_count = history
        .events()
        .iter()
        .filter(|event| {
            event.kind() == RunEventKind::ToolAction
                && event.subject() == Some(RunEventSubject::Tool(TOOL))
                && event.payload().outcome() == Some(RunEventOutcome::Succeeded)
        })
        .count();
    let step = stored
        .ledger()
        .step(step)
        .ok_or("step missing after reopen")?;
    let [step_attempt] = step.attempts() else {
        return Err("expected one durable step attempt".into());
    };
    if case == Case::Unknown {
        assert!(attempt.disposition().requires_reconciliation());
        let observed_at = at(run.updated_at().unix_millis() + 1)?;
        let inspection = InspectAgentRunRecovery::new(&reopened, &reopened, &reopened, &reopened)
            .execute(&fixture.project, run_id, observed_at, &ActiveControl)
            .await?;
        assert!(inspection.mutation_reconciliation_required());
        assert!(!inspection.loopback_resume_pending());
        assert!(!inspection.can_resume());
        let resumed = RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
            .execute(
                &fixture.project,
                run_id,
                AgentRecoveryChoice::Resume,
                RunEventId::from_bytes([164; 32]),
                observed_at,
                &ActiveControl,
            )
            .await;
        assert!(matches!(
            resumed,
            Err(AgentRecoveryError::MutationReconciliationRequired)
        ));
        assert_eq!(
            attempt.tool_attempt().status(),
            AgentToolAttemptStatus::Failed
        );
        assert_eq!(success_count, 0);
        assert_ne!(step.status(), TaskStepStatus::Completed);
        assert!(step_attempt.evidence_ids().is_empty());
        assert!(step_attempt.verification().is_none());
        assert!(
            !reopened
                .machine_recovery_acknowledged(
                    &fixture.project,
                    TOOL,
                    attempt.tool_attempt().attempt()
                )
                .await?
        );
        assert_eq!(
            reopened
                .begin_agent_mutation_attempt(
                    &fixture.project,
                    run_id,
                    snapshot,
                    ToolRunId::from_bytes([151; 32]),
                    attempt.fingerprint(),
                    AgentMutationKind::Patch,
                    at(run.updated_at().unix_millis() + 1)?,
                )
                .await,
            Err(AgentRecoveryStoreFailure::MutationReconciliationRequired)
        );
    } else {
        assert!(!attempt.disposition().requires_reconciliation());
        assert_eq!(
            attempt.tool_attempt().status(),
            AgentToolAttemptStatus::Succeeded
        );
        assert_eq!(success_count, 1);
        let passed = case == Case::Pass;
        assert_eq!(
            step.status(),
            if passed {
                TaskStepStatus::Completed
            } else {
                TaskStepStatus::Ready
            }
        );
        assert_eq!(
            step_attempt.outcome(),
            &if passed {
                a3_domain::TaskStepAttemptOutcome::Completed
            } else {
                a3_domain::TaskStepAttemptOutcome::VerificationFailed
            }
        );
        let verification = step_attempt
            .verification()
            .ok_or("verification missing after known cleanup")?;
        assert_eq!(verification.passed(), passed);
        let [id] = verification.evidence_ids() else {
            return Err("expected one typed HTTP evidence artifact".into());
        };
        assert_eq!(outcome_evidence, passed.then_some(*id));
        let evidence = reopened
            .load_verification_state(
                &fixture.project,
                goal.task_id(),
                &[*id],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let [VerificationEvidence::LoopbackHttp(receipt)] = evidence.evidence() else {
            return Err("expected actual HTTP observation evidence".into());
        };
        assert_eq!(receipt.context().run_id, run_id);
        assert_eq!(receipt.context().tool_run_id, TOOL);
        assert_eq!(receipt.context().step_id, step.definition().id());
        assert_eq!(receipt.context().spec_id, spec.id());
        assert_eq!(receipt.context().snapshot_id, snapshot);
        assert_eq!(receipt.script(), &script);
        assert_eq!(
            receipt.process_specification(),
            prepared.start().specification_id()
        );
        assert_eq!(receipt.start_decision(), USED_DECISIONS[0]);
        assert_eq!(receipt.matches(&recipe), passed);
        assert_eq!(
            receipt.observations(),
            observations(case, [USED_DECISIONS[1], USED_DECISIONS[2]])?
        );
    }
    let accepted = if let Some(evidence_id) = outcome_evidence {
        assert_completed_http_recovery(
            &fixture,
            &reopened,
            &run,
            &stored,
            evidence_id,
            stale_after_e6,
        )
        .await?
        .map(|accepted_at| (evidence_id, accepted_at))
    } else {
        None
    };
    drop(reopened);
    if let Some((evidence_id, accepted_at)) = accepted {
        finish_reopened_source(&fixture, goal, evidence_id, snapshot, accepted_at).await?;
    }
    if !stale_after_e6 {
        assert_eq!(inventory(&root)?, before);
    }
    assert_eq!(runner_calls.load(Ordering::SeqCst), 1);
    fixture.cleanup()?;
    Ok(())
}

async fn assert_completed_http_recovery(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &AgentRun,
    stored: &StoredTaskLedger,
    evidence_id: TaskEvidenceId,
    stale: bool,
) -> TestResult<Option<u64>> {
    let project = &fixture.project;
    let step = stored
        .ledger()
        .steps()
        .find(|step| step.status() == TaskStepStatus::Completed)
        .ok_or("completed HTTP step missing")?;
    let step_id = step.definition().id();
    let attempts = store
        .load_agent_mutation_attempts(project, run.id())
        .await?;
    if stale {
        let root = project.worktree().root().as_path();
        let original = fs::read(root.join("server.py"))?;
        fs::write(root.join("foreign.txt"), b"unrelated published change\n")?;
        let (publisher, _, _, newer) = index_fixture(fixture).await?;
        assert_ne!(newer.run().snapshot_id(), run.current_snapshot_id());
        assert_eq!(fs::read(root.join("server.py"))?, original);
        drop(publisher);
    }
    let observed_at = at(run.updated_at().unix_millis() + 1)?;
    let inspection = InspectAgentRunRecovery::new(store, store, store, store)
        .execute(project, run.id(), observed_at, &ActiveControl)
        .await?;
    assert_eq!(inspection.can_resume(), !stale);
    assert!(!inspection.loopback_resume_pending());
    let recovery = RecoverAgentRun::new(store, store, store, store);
    let resumed = recovery
        .execute(
            project,
            run.id(),
            AgentRecoveryChoice::Resume,
            RunEventId::from_bytes([162; 32]),
            observed_at,
            &ActiveControl,
        )
        .await;
    let accepted_at = if stale {
        assert_eq!(inspection.stale_evidence_ids(), &[evidence_id]);
        assert!(matches!(
            resumed,
            Err(AgentRecoveryError::ResumeRequiresReplan)
        ));
        assert_eq!(
            store.load_agent_run(project, run.id()).await?.as_ref(),
            Some(run)
        );
        assert_eq!(
            store
                .load_task_ledger(project, run.goal_contract().task_id())
                .await?
                .as_ref(),
            Some(stored)
        );
        let replanned = recovery
            .execute(
                project,
                run.id(),
                AgentRecoveryChoice::Replan,
                RunEventId::from_bytes([163; 32]),
                observed_at,
                &ActiveControl,
            )
            .await?;
        assert_eq!(replanned.kind(), AgentRecoveryOutcomeKind::ReplanRequired);
        assert_eq!(replanned.reopened_step_ids(), &[step_id]);
        let step = replanned
            .ledger()
            .ledger()
            .step(step_id)
            .ok_or("reopened step missing")?;
        assert_ne!(step.status(), TaskStepStatus::Completed);
        None
    } else {
        assert!(inspection.stale_evidence_ids().is_empty());
        let resumed = resumed?;
        assert_eq!(resumed.kind(), AgentRecoveryOutcomeKind::Resumed);
        assert!(resumed.reopened_step_ids().is_empty());
        assert_eq!(resumed.run().state(), AgentControllerState::Verify);
        assert_eq!(resumed.run().usage(), run.usage());
        assert_eq!(resumed.ledger().ledger(), stored.ledger());
        Some(resumed.run().updated_at().unix_millis() + 1)
    };
    let later = store
        .load_run_events(
            project,
            run.id(),
            Some(run.last_event_sequence()),
            RunEventPageLimit::new(2)?,
        )
        .await?;
    let [event] = later.events() else {
        return Err("expected only a single generic recovery event".into());
    };
    assert_eq!(event.kind(), RunEventKind::Diagnostic);
    assert_eq!(event.payload().code(), RunEventCode::StateRecovered);
    assert_eq!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?,
        attempts
    );
    Ok(accepted_at)
}

async fn resume_settled_receipt(
    fixture: &Fixture,
    goal: GoalContract,
    context_seed: &MutationContextSeed,
    receipt: &LoopbackHttpEvidence,
    stale: bool,
) -> TestResult {
    let coordinates = receipt.context();
    let project = &fixture.project;
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let state = store
        .load_verification_state(
            project,
            goal.task_id(),
            &[receipt.id()],
            coordinates.snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    assert_eq!(
        state.evidence(),
        &[VerificationEvidence::LoopbackHttp(receipt.clone())]
    );
    let mut run = store
        .load_agent_run(project, coordinates.run_id)
        .await?
        .ok_or("settled run missing")?;
    assert_eq!(run.state(), AgentControllerState::Execute);
    let settled_sequence = run.last_event_sequence();
    let stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("active ledger missing")?;
    let (mut ledger, mut version) = stored.into_parts();
    let step = ledger
        .step(coordinates.step_id)
        .ok_or("active step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    let [step_attempt] = step.attempts() else {
        return Err("expected original step attempt".into());
    };
    assert!(step_attempt.verification().is_none());
    assert!(step_attempt.evidence_ids().is_empty());
    let attempts = store
        .load_agent_mutation_attempts(project, run.id())
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("expected original settled tool attempt".into());
    };
    assert_eq!(attempt.tool_attempt().tool_run_id(), TOOL);
    assert_eq!(
        attempt.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_eq!(attempt.disposition(), AgentMutationDisposition::Applied);
    let before_events = store
        .load_run_events(project, run.id(), None, RunEventPageLimit::new(64)?)
        .await?;
    let tail = before_events
        .events()
        .last()
        .ok_or("settled journal is empty")?;
    assert_eq!(tail.id(), RunEventId::from_bytes([115; 32]));
    assert_eq!(tail.sequence(), settled_sequence);
    assert_eq!(tail.kind(), RunEventKind::ToolAction);
    assert_eq!(tail.subject(), Some(RunEventSubject::Tool(TOOL)));
    assert_eq!(tail.payload().outcome(), Some(RunEventOutcome::Succeeded));

    let observed_at = at(run.updated_at().unix_millis() + 1)?;
    let generic = InspectAgentRunRecovery::new(&*store, &*store, &*store, &*store)
        .execute(project, run.id(), observed_at, &ActiveControl)
        .await?;
    assert!(generic.loopback_resume_pending());
    assert!(!generic.can_resume());
    assert!(!generic.mutation_reconciliation_required());
    let generic = RecoverAgentRun::new(&*store, &*store, &*store, &*store)
        .execute(
            project,
            run.id(),
            AgentRecoveryChoice::Resume,
            RunEventId::from_bytes([161; 32]),
            observed_at,
            &ActiveControl,
        )
        .await;
    assert!(matches!(
        generic,
        Err(AgentRecoveryError::LoopbackResumePending)
    ));
    let mut premature_run = run.clone();
    let premature_event = premature_run.record(
        RunEventId::from_bytes([161; 32]),
        RunEventKind::Diagnostic,
        RunEventPayload::new(
            RunEventCode::StateRecovered,
            Some(RunEventOutcome::Succeeded),
            None,
        ),
        coordinates.snapshot_id,
        None,
        observed_at,
    )?;
    assert_eq!(
        store
            .commit_agent_recovery(
                project,
                AgentRecoveryChoice::Resume,
                coordinates.snapshot_id,
                version,
                settled_sequence,
                &ledger,
                &premature_run,
                &premature_event,
            )
            .await,
        Err(AgentRecoveryStoreFailure::ToolAttemptConflict)
    );
    assert_eq!(
        store.load_agent_run(project, run.id()).await?,
        Some(run.clone())
    );
    assert_eq!(
        store
            .load_run_events(project, run.id(), None, RunEventPageLimit::new(64)?)
            .await?,
        before_events
    );

    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    if stale {
        let script = project.worktree().root().as_path().join("server.py");
        fs::write(&script, SERVER.replace("RESPONSE", "Broke"))?;
        let updated = refresh
            .execute(
                project,
                &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
                &mut compiler,
                &ActiveControl,
            )
            .await?;
        assert_ne!(
            updated.published_index().run().snapshot_id(),
            coordinates.snapshot_id
        );
    }
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approval = AgentApprovalBuffer::new();
    approval.activate_project(project);
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
        &approval,
        &patch,
        &process,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    );
    let authority = LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref());
    let observed_at = at(run.updated_at().unix_millis() + 1)?;
    let outcome = controller
        .resume_loopback_verification(
            project,
            &mut run,
            &mut ledger,
            &mut version,
            LoopbackVerificationResume {
                step_id: coordinates.step_id,
                ids: execution_ids(110),
                observed_at,
            },
            authority,
            context_seed,
            &mut compiler,
            &ActiveControl,
        )
        .await;
    if stale {
        assert!(matches!(
            outcome,
            Err(MutationControllerFailure::AnchorMismatch)
        ));
        assert_eq!(run.state(), AgentControllerState::Execute);
        assert_eq!(run.last_event_sequence(), settled_sequence);
        assert_eq!(
            ledger
                .step(coordinates.step_id)
                .ok_or("stale step missing")?
                .status(),
            TaskStepStatus::InProgress
        );
    } else {
        assert!(
            matches!(outcome?, MutationControllerOutcome::StepVerified { evidence_id, snapshot_id }
            if evidence_id == receipt.id() && snapshot_id == coordinates.snapshot_id)
        );
        assert_eq!(run.state(), AgentControllerState::Verify);
        let step = ledger
            .step(coordinates.step_id)
            .ok_or("resumed step missing")?;
        assert_eq!(step.status(), TaskStepStatus::Completed);
        let [step_attempt] = step.attempts() else {
            return Err("resume created an extra step attempt".into());
        };
        let verification = step_attempt
            .verification()
            .ok_or("resumed verification missing")?;
        assert!(verification.passed());
        assert_eq!(verification.evidence_ids(), &[receipt.id()]);
        let resumed_sequence = run.last_event_sequence();
        let observed_at = at(run.updated_at().unix_millis() + 1)?;
        let repeated = controller
            .resume_loopback_verification(
                project,
                &mut run,
                &mut ledger,
                &mut version,
                LoopbackVerificationResume {
                    step_id: coordinates.step_id,
                    ids: execution_ids(110),
                    observed_at,
                },
                authority,
                context_seed,
                &mut compiler,
                &ActiveControl,
            )
            .await;
        assert!(matches!(
            repeated,
            Err(MutationControllerFailure::AnchorMismatch)
        ));
        assert_eq!(run.last_event_sequence(), resumed_sequence);
    }
    let later = store
        .load_run_events(
            project,
            run.id(),
            Some(settled_sequence),
            RunEventPageLimit::new(2)?,
        )
        .await?;
    if stale {
        assert!(later.events().is_empty());
    } else {
        let [event] = later.events() else {
            return Err("resume did not persist exactly one verification transition".into());
        };
        assert_eq!(
            event.kind(),
            RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Verify,
            }
        );
    }
    assert_eq!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?,
        attempts
    );
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.load_agent_run(project, run.id()).await?,
        Some(run.clone())
    );
    assert_eq!(
        store.load_task_ledger(project, goal.task_id()).await?,
        Some(StoredTaskLedger::new(ledger, version))
    );
    drop((compiler, refresh, store));
    if !stale {
        finish_reopened_source(
            fixture,
            goal,
            receipt.id(),
            coordinates.snapshot_id,
            run.updated_at().unix_millis() + 1,
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug)]
struct FailVerificationCommit<'a> {
    store: &'a LibsqlKnowledgeStore,
    calls: AtomicUsize,
}

impl AgentActionStore for FailVerificationCommit<'_> {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        self.store.commit_ledger_action(
            project,
            expected_ledger_version,
            expected_last_sequence,
            ledger,
            run,
            event,
        )
    }

    fn commit_current_ledger_action<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _expected_ledger_version: TaskLedgerStoreVersion,
        _expected_last_sequence: RunEventSequence,
        _ledger: &'a TaskLedger,
        _run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        assert_eq!(
            event.kind(),
            RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Verify,
            }
        );
        assert_eq!(self.calls.fetch_add(1, Ordering::SeqCst), 0);
        Box::pin(async { Err(AgentActionStoreFailure::LedgerVersionConflict) })
    }
}

async fn grant_recipe_action(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    policy: &WorkspacePolicy,
    action: &PolicyAction,
    request: ApprovalRequestId,
    seed: u8,
) -> TestResult {
    let engine =
        EvaluateActionPolicy::new().with_permissions(store.load_agent_permissions().await?);
    let snapshot = run.current_snapshot_id();
    let now = at(u64::from(seed))?;
    let expected = run.last_event_sequence();
    let waiting = engine.execute(
        run,
        action,
        policy,
        None,
        PolicyEvaluationContext::new(
            PolicyDecisionId::from_bytes([seed; 32]),
            request,
            RunEventId::from_bytes([seed; 32]),
            snapshot,
            PolicyEvaluationTiming::new(now, now)?,
            at(100_000)?,
        ),
    )?;
    assert_eq!(
        waiting.decision().outcome(),
        PolicyDecisionOutcome::ApprovalRequired
    );
    assert_eq!(
        waiting
            .approval_request()
            .ok_or("exact approval was not requested")?
            .id(),
        request
    );
    PersistPolicyEvaluation::new(store)
        .execute(&fixture.project, expected, run, &waiting)
        .await?;
    let grant = GrantPolicyApproval::new(store)
        .execute(
            &fixture.project,
            run,
            request,
            ApprovalId::from_bytes([seed + 1; 32]),
            RunEventId::from_bytes([seed + 1; 32]),
            snapshot,
            at(u64::from(seed + 1))?,
        )
        .await?;
    assert_eq!(grant.state(), ApprovalGrantState::Active);
    Ok(())
}

#[derive(Debug)]
struct ObservedRunner<'a> {
    store: &'a LibsqlKnowledgeStore,
    runner: &'a dyn LoopbackVerificationRunner,
    calls: AtomicUsize,
}

impl LoopbackVerificationRunner for ObservedRunner<'_> {
    fn supports_owned_cleanup(&self) -> bool {
        self.runner.supports_owned_cleanup()
    }

    fn verify<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackVerification,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let run_id = authorized.prepared().start().run_id();
            let attempts = self
                .store
                .load_agent_mutation_attempts(project, run_id)
                .await;
            assert!(matches!(attempts.as_deref(), Ok([attempt])
                if attempt.tool_attempt().tool_run_id() == TOOL
                    && attempt.tool_attempt().status() == AgentToolAttemptStatus::InFlight
                    && attempt.disposition().requires_reconciliation()));
            let requests = [START_REQUEST, PROBE_REQUESTS[0], PROBE_REQUESTS[1]];
            for (request, expected) in requests.into_iter().zip(USED_DECISIONS) {
                let grant = self.store.load_approval_for_request(project, request).await;
                assert!(matches!(grant, Ok(Some(grant))
                    if matches!(grant.state(), ApprovalGrantState::Consumed { decision_id, .. } if decision_id == expected)));
            }
            self.runner.verify(project, authorized, control).await
        })
    }
}

#[derive(Debug)]
struct ControlledRunner(Case);

impl LoopbackVerificationRunner for ControlledRunner {
    fn supports_owned_cleanup(&self) -> bool {
        true
    }

    fn verify<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackVerification,
        _: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a> {
        Box::pin(async move {
            if self.0 == Case::Unknown {
                return LoopbackVerificationOutcome::UnknownCleanup;
            }
            let (_, start, probes) = authorized.into_parts();
            let (_, start_decision) = start.into_parts();
            let [get, post] = probes.as_slice() else {
                return LoopbackVerificationOutcome::UnknownCleanup;
            };
            LoopbackVerificationOutcome::Stopped {
                cleanup: a3_application::LoopbackCleanupProof::ProcessGroupAndListener,
                result: observations(self.0, [*get, *post])
                    .map(|probes| a3_application::LoopbackVerificationReport {
                        start_decision,
                        probes: probes.to_vec(),
                    })
                    .map_err(|_| a3_application::LoopbackVerificationFailure::InvalidResponse),
            }
        })
    }
}

fn observations(
    case: Case,
    decisions: [PolicyDecisionId; 2],
) -> Result<[LoopbackHttpObservation; 2], a3_domain::LoopbackHttpError> {
    let body = format!("{}\n", case.body());
    Ok([
        LoopbackHttpObservation::new(
            decisions[0],
            200,
            hash(body.as_bytes()),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?,
        LoopbackHttpObservation::new(
            decisions[1],
            405,
            hash(b""),
            0,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?,
    ])
}

fn assert_listener_gone(case: Case, endpoint: SocketAddrV4) -> TestResult {
    #[cfg(any(windows, target_os = "linux"))]
    if matches!(case, Case::Pass | Case::WrongBody) {
        let listener = TcpListener::bind(endpoint)?;
        assert_eq!(listener.local_addr()?, std::net::SocketAddr::V4(endpoint));
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (case, endpoint);
    Ok(())
}
