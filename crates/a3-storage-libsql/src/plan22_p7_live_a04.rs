//! Registered frozen A04 through real Plan/Edit jobs and the existing owned controller.

#[path = "plan22_p7_a04_approvals.rs"]
mod approvals;
#[path = "plan22_p7_a04_model_phases.rs"]
mod phases;

use super::super::super::super::super::p5_live_a01::model_phases::ObservedPhaseProvider;
use super::*;
use a3_application::{
    ActivateAgentHorizonPlan, AppendAgentHorizonPlan, ContinueSourceAgentPlan,
    EditPhaseReservationStore, IntakePhaseReservationStore, SourcePlanContinuationOutcome,
};
use a3_domain::{
    AgentActionLimit, AgentRepairLimit, AgentRunDurationLimit, AgentTokenLimit, AgentTurnLimit,
    DiffInvariantMode, DiffInvariantVerification, ExpectedTaskEvidence, PatchOperation,
    PendingPlanningStage, StepDependency, TaskLedger, TaskStepDefinition, TaskStepOutcome,
    TaskStepRationale, VerificationSpecId,
};
use phases::A04Phases;
use serde::Deserialize;
use std::{
    io::Write,
    sync::Mutex,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const HTTP_STEP: TaskStepId = TaskStepId::from_bytes([236; 32]);
const SCHEMA: &str = "a3.plan22.p7.a04";
const TASK_SECONDS: u64 = 900;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    schema: String,
    attempt_id: String,
    case_id: String,
    variant: String,
    model: String,
    context_limit: u32,
    output_limit: u32,
    task_timeout_seconds: u64,
    probe_timeout_seconds: u64,
    outer_timeout_seconds: u64,
    request_sha256: String,
    original_sha256: String,
    worker_sha256: String,
}

struct Input {
    root: PathBuf,
    launch: Launch,
    original: Vec<u8>,
    launch_sha256: String,
}

impl Input {
    fn load() -> TestResult<Self> {
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P7_A04_RUN_DIR")
                .ok_or("explicit registered A04 run directory required")?,
        );
        if !root.is_absolute() || fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err("A04 needs a real absolute run directory".into());
        }
        let root = fs::canonicalize(root)?;
        let launch_bytes = read_bounded(&root.join("launch.json"), 8_192)?;
        let request_bytes = read_bounded(&root.join("request.json"), 16_384)?;
        let launch: Launch = serde_json::from_slice(&launch_bytes)?;
        let request: Value = serde_json::from_slice(&request_bytes)?;
        let original = read_frozen_a04_original(A04_ORIGINAL_ENV)?;
        let recipe = frozen_a04_recipe()?;
        let network: Vec<_> = [18_765, 8_765]
            .into_iter()
            .flat_map(|port| {
                recipe.checks().iter().map(move |check| {
                    json!({
                        "method":check.probe().method().as_str(),
                        "url":format!("http://127.0.0.1:{port}{}",check.probe().path())
                    })
                })
            })
            .collect();
        let processes = json!([
            {"argv":["python","-B","server.py","--host","127.0.0.1","--port","18765"],"cwd":".","timeout_seconds":30},
            {"argv":["python","-B","server.py","--port","invalid"],"cwd":".","timeout_seconds":30},
            {"argv":["python","-B","server.py"],"cwd":".","timeout_seconds":30}
        ]);
        if launch.schema != format!("{SCHEMA}.launch.v1")
            || launch.case_id != "A04"
            || launch.variant != "development"
            || !matches!(launch.model.as_str(), "qwen38-4k:latest" | "ornith-1.5:9b")
            || (launch.context_limit, launch.output_limit) != (4_096, 1_024)
            || launch.task_timeout_seconds != TASK_SECONDS
            || launch.probe_timeout_seconds != 120
            || launch.outer_timeout_seconds != TASK_SECONDS + 150
            || root.file_name().and_then(|name| name.to_str()) != Some(launch.attempt_id.as_str())
            || launch.request_sha256 != sha256(&request_bytes)
            || launch.original_sha256 != A04_ORIGINAL_SHA256
            || launch.worker_sha256 != hash_file(&std::env::current_exe()?)?
            || request["case_id"] != "A04"
            || request["variant"] != "development"
            || request["mode"] != "agent"
            || request["objective"].as_str().map(str::as_bytes) != Some(original.as_slice())
            || request["allowed_write_paths"] != json!(["server.py"])
            || request["allowed_processes"] != processes
            || request["allowed_network_requests"] != json!(network)
            || request["process_effect_paths"] != json!([])
            || request["expected_test_ids"] != json!([])
        {
            return Err("A04 launch or frozen effect envelope differs".into());
        }
        Ok(Self {
            root,
            launch,
            original,
            launch_sha256: sha256(&launch_bytes),
        })
    }

    fn identity(&self, phase: &str) -> Value {
        json!({"schema":format!("{SCHEMA}.{phase}.v1"),"attempt_id":self.launch.attempt_id,
            "launch_sha256":self.launch_sha256,"request_sha256":self.launch.request_sha256,
            "original_sha256":self.launch.original_sha256,"model":self.launch.model})
    }
}

struct A04Clock {
    base: AgentRunTimestamp,
    started: Instant,
    floor: AtomicU64,
}

impl A04Clock {
    fn new() -> TestResult<Self> {
        let base = live_timestamp()?;
        Ok(Self {
            base,
            started: Instant::now(),
            floor: AtomicU64::new(base.unix_millis()),
        })
    }
    fn now(&self) -> TestResult<AgentRunTimestamp> {
        let millis = self
            .base
            .unix_millis()
            .checked_add(u64::try_from(self.started.elapsed().as_millis())?)
            .ok_or("A04 monotonic clock overflow")?;
        Ok(AgentRunTimestamp::from_unix_millis(
            self.floor.fetch_max(millis, Ordering::SeqCst).max(millis),
        )?)
    }
    fn observe(&self, at: AgentRunTimestamp) {
        self.floor.fetch_max(at.unix_millis(), Ordering::SeqCst);
    }
}

#[derive(Default)]
struct Receipts {
    stage: &'static str,
    dispatches: Mutex<Vec<Value>>,
    run_id: Option<AgentRunId>,
    review_sha256: Option<String>,
    product_http: Option<Value>,
    reopen: Option<Value>,
    cold_reopen_done: bool,
}

#[test]
#[ignore = "explicit registered frozen A04 with local Ollama and exact source review; requires A3_PLAN22_P7_A04_RUN_DIR"]
fn live_ollama_frozen_a04_creates_server_and_reopens_done() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let input = Input::load()?;
        write_once(
            &input.root.join("native-start.json"),
            &input.identity("native-start"),
        )?;
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            let mut receipts = Receipts::default();
            let result = run_attempt(&input, &mut receipts).await;
            let state_result = durable_state(&input, receipts.run_id).await;
            let mismatch_result = durable_mismatch(&input, receipts.run_id).await;
            let durable_state = state_result.as_ref().ok().copied().flatten();
            let mut report = input.identity("native-final");
            let fields = report.as_object_mut().ok_or("A04 identity is not an object")?;
            fields.extend([
                ("run_state".to_owned(), json!(durable_state.map(|state| format!("{state:?}")))),
                ("formal_completed".to_owned(), json!(formal_completion(
                    result.is_ok(), receipts.stage, receipts.cold_reopen_done, durable_state))),
                ("durable_state_available".to_owned(), json!(state_result.is_ok())),
                ("source_controller".to_owned(), json!(true)),
                ("last_stage".to_owned(), json!(receipts.stage)),
                ("bound_intake".to_owned(), json!("frozen original plus exact reviewed outcome and three constraints; no intake model call")),
                ("phase_dispatches".to_owned(), json!(*receipts.dispatches.lock().map_err(|_| "A04 phase recorder poisoned")?)),
                ("review_sha256".to_owned(), json!(receipts.review_sha256)),
                ("product_http".to_owned(), json!(receipts.product_http)),
                ("product_http_mismatch".to_owned(), json!(mismatch_result.as_ref().ok().cloned().flatten())),
                ("product_http_mismatch_load_error".to_owned(), json!(mismatch_result.as_ref().err().map(|_| "diagnostic_load_failed"))),
                ("reopen".to_owned(), json!(receipts.reopen)),
            ]);
            write_once(&input.root.join("native-final.json"), &report)?;
            state_result?;
            result
        })
    })
}

async fn run_attempt(input: &Input, receipts: &mut Receipts) -> TestResult {
    receipts.stage = "empty_inventory";
    let worktree = input.root.join("worktree");
    if !fs::symlink_metadata(worktree.join(".git"))?
        .file_type()
        .is_dir()
        || !inventory(&worktree)?.is_empty()
    {
        return Err("A04 requires the registered initialized empty worktree".into());
    }
    let fixture = Fixture {
        root: input.root.clone(),
        layout: StorageLayout::prepare(input.root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    receipts.stage = "capability_probe";
    let profile =
        probe_projected_profile(&input.root, &provider, &input.launch.model, 4_096, 1_024).await?;
    let clock = A04Clock::new()?;
    let budget = AgentRunBudget::new(
        AgentTurnLimit::new(12)?,
        AgentTokenLimit::new(45_000)?,
        AgentTokenLimit::new(24_000)?,
        AgentActionLimit::new(8)?,
        AgentRunDurationLimit::from_millis(TASK_SECONDS * 1_000)?,
        AgentRepairLimit::new(2)?,
    );
    let recipe = frozen_a04_recipe()?;
    let promoted = promote_live_inventory_with_budget(
        fixture,
        profile,
        Some(&recipe),
        Some(&input.original),
        budget,
    )
    .await?;
    let fixture = &promoted.data.fixture;
    let store = &promoted.store;
    let profile = &promoted.model;
    let goal = &promoted.data.goal;
    let project = &fixture.project;
    let step = promoted.data.step.id();
    let accounting = store
        .load_intake_phase_accounting(project, goal.task_id())
        .await?
        .ok_or("A04 Intake accounting missing")?;
    assert_eq!(accounting.budget(), budget);
    assert_eq!(
        accounting.deadline().unix_millis(),
        accounting.captured_at().unix_millis() + TASK_SECONDS * 1_000
    );
    receipts.stage = "plan";
    let phases = A04Phases {
        fixture,
        store,
        provider: &provider,
        profile,
        clock: &clock,
        dispatches: &receipts.dispatches,
    };
    let stored = phases.plan(&promoted).await?;
    let initial_usage = store
        .load_initial_plan_usage(project, goal.task_id())
        .await?;
    let started_at = clock.now()?;
    let (mut run, stored) = start_materialized_fixture_step(
        fixture,
        store,
        goal,
        profile,
        promoted.published.run().snapshot_id(),
        stored,
        step,
        (accounting.run_budget(started_at)?, started_at),
    )
    .await?;
    clock.observe(run.updated_at());
    receipts.run_id = Some(run.id());
    assert_eq!(
        run.created_at().unix_millis() + run.budget().duration_limit().millis(),
        accounting.deadline().unix_millis()
    );
    let linked = store
        .load_intake_phase_accounting(project, goal.task_id())
        .await?
        .ok_or("A04 linked Intake missing")?;
    assert_eq!(linked.linked_run(), Some(run.id()));
    let used = store.load_edit_phase_usage(project, run.id()).await?;
    assert_eq!(used.calls, initial_usage.calls + accounting.usage().calls);
    assert_eq!(
        used.prompt_tokens,
        initial_usage.prompt_tokens + accounting.usage().prompt_tokens
    );
    assert_eq!(
        used.output_tokens,
        initial_usage.output_tokens + accounting.usage().output_tokens
    );
    receipts.stage = "edit";
    let action = phases.add(goal, &stored, &run, &promoted.published).await?;
    receipts.stage = "source_review";
    let source_sha = review_proposal(input, &action, &clock, accounting.deadline())?;
    receipts.review_sha256 = Some(source_sha.clone());
    let (mut ledger, mut version) = stored.into_parts();
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let current = promoted.published.clone();
    let seed = MutationContextSeed::new(goal.clone(), profile.clone(), Vec::new(), Vec::new());
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let coordinator = WorktreeMutationCoordinator::new();
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let process = WorkspaceProcessRunner::default();
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approvals,
        &patch,
        &process,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    receipts.stage = "apply_patch";
    let changed = execute_approved_until(
        controller,
        fixture,
        store,
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        action,
        None,
        60,
        clock.now()?.unix_millis(),
        accounting.deadline(),
        &seed,
        &mut compiler,
        &events,
        async |_| Ok(()),
    )
    .await?;
    clock.observe(run.updated_at());
    let MutationControllerOutcome::StepVerified { snapshot_id, .. } = changed else {
        return Err("A04 Add did not produce current path evidence".into());
    };
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("A04 current publication missing")?;
    assert_eq!(current.run().snapshot_id(), snapshot_id);
    assert_eq!(
        sha256(&read_bounded(&worktree.join("server.py"), 16_384)?),
        source_sha
    );
    assert_eq!(inventory(&worktree)?.len(), 1);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    receipts.stage = "horizon";
    let continued = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            RunEventId::from_bytes(identity("p7.a04.need-plan", 0)),
            clock.now()?,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::PlanningRequested {
        ledger_version,
        request_event,
    } = continued
    else {
        return Err("A04 implementation horizon did not leave HTTP coverage open".into());
    };
    version = ledger_version;
    assert!(matches!(
        ledger.pending_planning().map(|pending| pending.stage()),
        Some(PendingPlanningStage::NeedsPlan)
    ));
    let criterion = goal
        .draft()
        .acceptance_criteria()
        .first()
        .ok_or("A04 outcome missing")?
        .id();
    let proof = TaskStepDefinition::new(
        HTTP_STEP,
        None,
        TaskStepOutcome::try_from_string(
            "Verify the owned HTTP service and startup failures".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Use the already original-bound operational recipe".to_owned(),
        )?,
        vec![StepDependency::new(step)],
        vec![ExpectedTaskEvidence::try_from_string(
            "Current owned startup and HTTP observations".to_owned(),
        )?],
        VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes(identity("p7.a04.http-spec", 0)),
            VerificationRequirement::try_from_string(
                "Observe the exact original startup and response requirements".to_owned(),
            )?,
            recipe.clone(),
        ),
    )?
    .with_criterion_coverage(Vec::new(), vec![criterion])?;
    let before_horizon = run.usage();
    version = AppendAgentHorizonPlan::new(store.as_ref())
        .execute(
            project,
            version,
            goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            &request_event,
            vec![proof],
            RunEventId::from_bytes(identity("p7.a04.append-plan", 0)),
            clock.now()?,
            &ActiveControl,
        )
        .await?;
    version = ActivateAgentHorizonPlan::new(store.as_ref())
        .execute(
            project,
            version,
            goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            &request_event,
            HTTP_STEP,
            RunEventId::from_bytes(identity("p7.a04.activate-plan", 0)),
            clock.now()?,
            &ActiveControl,
        )
        .await?;
    clock.observe(run.updated_at());
    assert_eq!(run.usage(), before_horizon);
    assert_eq!(
        store
            .load_initial_plan_usage(project, goal.task_id())
            .await?,
        initial_usage
    );
    assert_eq!(
        run.created_at().unix_millis() + run.budget().duration_limit().millis(),
        accounting.deadline().unix_millis()
    );
    receipts.stage = "http_verification";
    let evidence = approvals::verify_http(
        fixture,
        store,
        goal,
        profile,
        &mut run,
        &mut ledger,
        &mut version,
        &refresh,
        &mut compiler,
        &clock,
    )
    .await?;
    clock.observe(run.updated_at());
    let state = store
        .load_verification_state(
            project,
            goal.task_id(),
            &[evidence],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::LoopbackHttp(receipt)] = state.evidence() else {
        return Err("A04 complete typed HTTP receipt missing".into());
    };
    assert!(receipt.matches(&recipe));
    assert!(receipt.startup().is_some());
    assert_eq!(
        sha256(&read_bounded(&worktree.join("server.py"), 16_384)?),
        source_sha
    );
    assert_eq!(inventory(&worktree)?.len(), 1);
    receipts.product_http = Some(
        json!({"typed_evidence":true,"complete_startup":true,"explicit_checks":6,
        "default_checks":1,"source_sha256":source_sha,"deadline_unreset":true}),
    );
    receipts.stage = "acceptance";
    let memory =
        RunMemoryCheckpoint::compile(goal, &ledger, &run, state.published_index(), Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        project.clone(),
        &run,
        goal.clone(),
        ledger.clone(),
        memory,
    )?;
    let previous = run.last_event_sequence();
    let verifier = DeterministicAcceptanceVerifier::new(store.as_ref());
    let advance = within_original_deadline(&clock, accounting.deadline(), async {
        Ok(VerifyAgentAcceptance::new(&verifier)
            .execute(
                &mut run,
                &request,
                RunEventId::from_bytes(identity("p7.a04.acceptance", 0)),
                clock.now()?,
                &ActiveControl,
            )
            .await?)
    })
    .await?;
    within_original_deadline(&clock, accounting.deadline(), async {
        AppendRunEvent::new(store.as_ref())
            .execute(project, previous, &run, advance.event())
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(run.state(), AgentControllerState::Done);
    let used = store.load_edit_phase_usage(project, run.id()).await?;
    let final_run = run.clone();
    let final_ledger = ledger.clone();
    let final_goal = goal.clone();
    let run_id = run.id();
    drop((compiler, refresh));
    let Promoted { data, store, .. } = promoted;
    drop(store);
    receipts.stage = "reopen";
    let reopened = data.open().await?;
    let project = &data.fixture.project;
    assert_eq!(
        reopened.load_agent_run(project, run_id).await?,
        Some(final_run)
    );
    assert_eq!(
        reopened
            .load_task_ledger(project, final_goal.task_id())
            .await?
            .map(|stored| stored.ledger().clone()),
        Some(final_ledger)
    );
    assert_eq!(
        reopened
            .load_current_goal_contract(project, final_goal.task_id())
            .await?,
        Some(final_goal.clone())
    );
    assert_eq!(reopened.load_edit_phase_usage(project, run_id).await?, used);
    let loaded = reopened
        .load_verification_state(
            project,
            final_goal.task_id(),
            &[evidence],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::LoopbackHttp(receipt)] = loaded.evidence() else {
        return Err("A04 lost its typed HTTP receipt on reopen".into());
    };
    assert!(receipt.matches(&recipe));
    assert_eq!(
        final_goal.original().map(|original| original.as_bytes()),
        Some(input.original.as_slice())
    );
    receipts.reopen = Some(
        json!({"run_state":"Done","current_evidence_bound":true,"usage":{
        "calls":used.calls,"repairs":used.repairs,"prompt_tokens":used.prompt_tokens,"output_tokens":used.output_tokens}}),
    );
    ensure_before_deadline(&clock, accounting.deadline())?;
    receipts.cold_reopen_done = true;
    receipts.stage = "complete";
    Ok(())
}

fn ensure_before_deadline(clock: &A04Clock, deadline: AgentRunTimestamp) -> TestResult {
    if clock.now()? >= deadline {
        return Err("A04 original deadline reached".into());
    }
    Ok(())
}

fn formal_completion(
    succeeded: bool,
    stage: &str,
    cold_reopen_done: bool,
    durable_state: Option<AgentControllerState>,
) -> bool {
    succeeded
        && stage == "complete"
        && cold_reopen_done
        && durable_state == Some(AgentControllerState::Done)
}

async fn within_original_deadline<T>(
    clock: &A04Clock,
    deadline: AgentRunTimestamp,
    operation: impl std::future::Future<Output = TestResult<T>>,
) -> TestResult<T> {
    ensure_before_deadline(clock, deadline)?;
    let value = operation.await?;
    ensure_before_deadline(clock, deadline)?;
    Ok(value)
}

#[test]
fn a04_worker_rejects_a_delayed_acceptance_and_never_polls_an_expired_job() -> TestResult {
    use std::sync::atomic::AtomicBool;
    let clock = A04Clock::new()?;
    let deadline = AgentRunTimestamp::from_unix_millis(clock.now()?.unix_millis() + 60_000)?;
    let polled = AtomicBool::new(false);
    let delayed = futures::executor::block_on(within_original_deadline(&clock, deadline, async {
        polled.store(true, Ordering::SeqCst);
        clock.observe(deadline);
        Ok(42)
    }));
    assert!(polled.load(Ordering::SeqCst));
    assert!(delayed.is_err());
    polled.store(false, Ordering::SeqCst);
    let expired = futures::executor::block_on(within_original_deadline(&clock, deadline, async {
        polled.store(true, Ordering::SeqCst);
        Ok(42)
    }));
    assert!(!polled.load(Ordering::SeqCst));
    assert!(expired.is_err());
    Ok(())
}

#[test]
fn a04_worker_rejects_a_delayed_commit_while_retaining_its_observed_done_state() -> TestResult {
    use std::sync::atomic::AtomicBool;
    let clock = A04Clock::new()?;
    let deadline = AgentRunTimestamp::from_unix_millis(clock.now()?.unix_millis() + 60_000)?;
    let durable_done = AtomicBool::new(false);
    let committed =
        futures::executor::block_on(within_original_deadline(&clock, deadline, async {
            durable_done.store(true, Ordering::SeqCst);
            clock.observe(deadline);
            Ok(())
        }));
    assert!(durable_done.load(Ordering::SeqCst));
    assert!(committed.is_err());
    assert!(!formal_completion(
        committed.is_ok(),
        "acceptance",
        false,
        Some(AgentControllerState::Done)
    ));
    let polled = AtomicBool::new(false);
    let expired = futures::executor::block_on(within_original_deadline(&clock, deadline, async {
        polled.store(true, Ordering::SeqCst);
        Ok(())
    }));
    assert!(!polled.load(Ordering::SeqCst));
    assert!(expired.is_err());
    assert!(durable_done.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn a04_worker_formal_completion_requires_the_successful_complete_cold_attempt() {
    assert!(formal_completion(
        true,
        "complete",
        true,
        Some(AgentControllerState::Done)
    ));
    for (succeeded, stage, cold, state) in [
        (false, "complete", true, Some(AgentControllerState::Done)),
        (true, "acceptance", false, Some(AgentControllerState::Done)),
        (true, "reopen", true, Some(AgentControllerState::Done)),
        (true, "complete", false, Some(AgentControllerState::Done)),
        (true, "complete", true, Some(AgentControllerState::Verify)),
        (true, "complete", true, None),
    ] {
        assert!(!formal_completion(succeeded, stage, cold, state));
    }
}

fn review_proposal(
    input: &Input,
    action: &AgentAction,
    clock: &A04Clock,
    deadline: AgentRunTimestamp,
) -> TestResult<String> {
    let AgentAction::ApplyPatch(patch) = action else {
        return Err("A04 Edit did not propose a patch".into());
    };
    let [PatchOperation::Add(add)] = patch.operations() else {
        return Err("A04 proposed more than one bounded Add".into());
    };
    if add.path().as_bytes() != b"server.py" {
        return Err("A04 Add escaped its exact file scope".into());
    }
    let bytes = add.content().as_bytes();
    let sha = sha256(bytes);
    write_proposed_source(&input.root, bytes)?;
    let mut awaiting = input.identity("awaiting-source-review");
    awaiting["source_sha256"] = json!(sha);
    write_once(&input.root.join("native-awaiting-review.json"), &awaiting)?;
    let approval = input.root.join("source-review.json");
    loop {
        if clock.now()? >= deadline {
            return Err("A04 original deadline expired during source review".into());
        }
        match read_bounded(&approval, 4_096) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)?;
                if value["attempt_id"] != input.launch.attempt_id
                    || value["source_sha256"] != sha
                    || value["launch_sha256"] != input.launch_sha256
                    || value["approved"] != true
                {
                    return Err("A04 exact source review denied or stale".into());
                }
                return Ok(sha);
            }
            Err(_) if !approval.exists() => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(error),
        }
    }
}

async fn durable_state(
    input: &Input,
    run: Option<AgentRunId>,
) -> TestResult<Option<AgentControllerState>> {
    let Some(run) = run else {
        return Ok(None);
    };
    let project = RepositoryInspector::new().inspect(input.root.join("worktree"))?;
    let layout = StorageLayout::prepare(input.root.join("app-data"))?;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&layout).await?;
    Ok(store
        .load_agent_run(&project, run)
        .await?
        .map(|run| run.state()))
}

async fn durable_mismatch(input: &Input, run: Option<AgentRunId>) -> TestResult<Option<Value>> {
    let Some(run) = run else {
        return Ok(None);
    };
    let project = RepositoryInspector::new().inspect(input.root.join("worktree"))?;
    let layout = StorageLayout::prepare(input.root.join("app-data"))?;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&layout).await?;
    let diagnostic = store
        .load_loopback_mismatch(
            &project,
            run,
            ToolRunId::from_bytes(identity("p7.a04.http.tool", 0)),
            a3_domain::AgentToolAttemptNumber::FIRST,
        )
        .await?;
    Ok(diagnostic.map(|d| {
        let observation = d.mismatch().observation();
        let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        json!({"phase":format!("{:?}", d.mismatch().phase()),"check_index":d.mismatch().check_index(),
            "status":observation.status(),"body_bytes":observation.body_bytes(),"body_hash":hex(observation.body_hash().as_bytes()),
            "script_hash":hex(d.script().content_hash().as_bytes()),"snapshot":hex(d.context().snapshot_id.as_bytes()),
            "diagnostic_digest":hex(d.digest().as_bytes()),"attempt":d.attempt().get()})
    }))
}

fn identity(role: &str, index: u32) -> [u8; 32] {
    let mut hash = blake3::Hasher::new_derive_key("a3.plan22.p7.a04.worker-identities.v1");
    hash.update(role.as_bytes());
    hash.update(&index.to_le_bytes());
    *hash.finalize().as_bytes()
}
fn write_proposed_source(root: &Path, bytes: &[u8]) -> TestResult {
    let mut source = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("proposed-server.py"))?;
    source.write_all(bytes)?;
    source.sync_all()?;
    Ok(())
}

#[test]
fn a04_review_source_releases_its_writer_before_review_begins() -> TestResult {
    let fixture = Fixture::new()?;
    let expected = b"# reviewed source\n";
    write_proposed_source(&fixture.root, expected)?;
    let bytes = {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let mut source = options.open(fixture.root.join("proposed-server.py"))?;
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes)?;
        bytes
    };
    assert_eq!(bytes, expected);
    fixture.cleanup()
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> TestResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn read_bounded(path: &Path, maximum: usize) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("A04 artifact exceeds its bound".into());
    }
    Ok(bytes)
}
fn write_once(path: &Path, value: &Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_all()?;
    Ok(())
}
