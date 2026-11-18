//! Owned live A01 composition through the existing controller and exact approvals.

#[path = "plan22_p5_a01_model_phases.rs"]
pub(super) mod model_phases;

use super::middle_edit_tests::p4_live_edit::probe_projected_profile;
use super::*;
use a3_application::{
    AgentApprovalAction, AgentApprovalNetworkScope, AgentApprovalWorkingDirectory,
    ContinueSourceAgentPlan, ContinueVerifiedAgentPlanOutcome, EditPhaseReservationStore,
    LanguageParseControl, LanguageParseControlError, SourcePlanContinuationOutcome,
    StoredVerificationState,
};
use a3_domain::{
    AgentActionLimit, AgentRepairLimit, AgentRunBudget, AgentRunDurationLimit, AgentTokenLimit,
    AgentTurnLimit, FileRevision, PatchOperation, PythonIntegerTranslation, SymbolName,
    TestCaseOutcome, WorkspaceDirectory,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use a3_repo_index::{ParserPoolSize, PythonLanguageAdapter};
use model_phases::{CHANGE_STEP, CHECK_STEP, LiveA01Phases, observed_at};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    sync::Mutex,
    time::Instant,
};

const SUBJECT_PATH: &str = "subject.py";
const ARGV: [&str; 7] = ["python", "-B", "-m", "unittest", "discover", "-s", "tests"];
const SCHEMA: &str = "a3.plan22.p5.a01";
const TASK_SECONDS: u64 = 360;

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

#[derive(Deserialize)]
struct FrozenRequest {
    case_id: String,
    variant: String,
    mode: String,
    objective: String,
    allowed_write_paths: Vec<String>,
    allowed_processes: Vec<FrozenProcess>,
    allowed_network_requests: Vec<Value>,
    process_effect_paths: Vec<String>,
    expected_test_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenProcess {
    argv: Vec<String>,
    cwd: String,
    timeout_seconds: u64,
}

struct Input {
    root: PathBuf,
    launch: Launch,
    request: FrozenRequest,
    launch_sha256: String,
}

impl Input {
    fn load() -> TestResult<Self> {
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P5_RUN_DIR")
                .ok_or("explicit A01 run directory required")?,
        );
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("A01 requires an absolute registered slot directory".into());
        }
        let root = fs::canonicalize(root)?;
        let launch_bytes = read_bounded(&root.join("launch.json"), 8_192)?;
        let request_bytes = read_bounded(&root.join("request.json"), 16_384)?;
        let launch: Launch = serde_json::from_slice(&launch_bytes)?;
        let request: FrozenRequest = serde_json::from_slice(&request_bytes)?;
        let expected_profile = match launch.model.as_str() {
            "ornith-1.5:9b" | "qwen38-27b-iq3s:latest" => (8_192, 2_048),
            "qwen38-4k:latest" => (4_096, 1_024),
            _ => return Err("unregistered A01 model".into()),
        };
        let [allowed] = request.allowed_processes.as_slice() else {
            return Err("A01 requires exactly one frozen process".into());
        };
        if launch.schema != format!("{SCHEMA}.launch.v1")
            || launch.case_id != "A01"
            || launch.variant != "development"
            || root.file_name().and_then(|name| name.to_str()) != Some(launch.attempt_id.as_str())
            || (launch.context_limit, launch.output_limit) != expected_profile
            || launch.task_timeout_seconds != TASK_SECONDS
            || launch.probe_timeout_seconds != 120
            || launch.outer_timeout_seconds != 510
            || launch.request_sha256 != sha256(&request_bytes)
            || launch.original_sha256 != sha256(request.objective.as_bytes())
            || launch.worker_sha256 != hash_worker()?
            || request.case_id != "A01"
            || request.variant != "development"
            || request.mode != "agent"
            || request.allowed_write_paths != [SUBJECT_PATH]
            || allowed.argv != ARGV
            || allowed.cwd != "."
            || allowed.timeout_seconds != 30
            || !request.allowed_network_requests.is_empty()
            || !request.process_effect_paths.is_empty()
            || request.expected_test_ids != ["test_behavior.Behavior.test_known_example"]
        {
            return Err("A01 launch and frozen contract do not match".into());
        }
        Ok(Self {
            root,
            launch,
            request,
            launch_sha256: sha256(&launch_bytes),
        })
    }

    fn identity(&self, phase: &str) -> Value {
        json!({
            "schema": format!("{SCHEMA}.{phase}.v1"),
            "attempt_id": self.launch.attempt_id,
            "launch_sha256": self.launch_sha256,
            "request_sha256": self.launch.request_sha256,
        })
    }
}

#[derive(Default)]
struct Receipts {
    dispatches: Mutex<Vec<Value>>,
    stage: &'static str,
    probe_verified: bool,
    bound_intake: Option<Value>,
    product_test: Option<Value>,
    reopen: Option<Value>,
    scope_violations: Vec<&'static str>,
}

#[test]
#[ignore = "explicit preregistered A01 model attempt; use the P5 supervisor"]
fn live_ollama_a01_completes_budget_bound_source_run() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let input = Input::load()?;
        write_once(
            &input.root.join("native-start.json"),
            &input.identity("native-start"),
        )?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let mut receipts = Receipts::default();
                let mut fixture = None;
                let result = run_attempt(&input, &mut fixture, &mut receipts).await;
                let state = match fixture.as_ref() {
                    Some(fixture) => durable_run_state(fixture).await,
                    None => Ok(None),
                };
                let run_state = match &state {
                    Ok(Some(state)) => format!("{state:?}"),
                    Ok(None) => "NotStarted".to_owned(),
                    Err(_) => "Unavailable".to_owned(),
                };
                let mut final_receipt = input.identity("native-final");
                let object = final_receipt
                    .as_object_mut()
                    .ok_or("native identity not an object")?;
                object.extend([
                    ("run_state".to_owned(), json!(run_state)),
                    (
                        "formal_completed".to_owned(),
                        json!(matches!(state, Ok(Some(AgentControllerState::Done)))),
                    ),
                    ("source_controller".to_owned(), json!(true)),
                    ("last_stage".to_owned(), json!(receipts.stage)),
                    (
                        "capability_probe_verified".to_owned(),
                        json!(receipts.probe_verified),
                    ),
                    ("bound_intake".to_owned(), json!(receipts.bound_intake)),
                    (
                        "phase_dispatches".to_owned(),
                        json!(
                            receipts
                                .dispatches
                                .into_inner()
                                .map_err(|_| "dispatch recorder poisoned")?
                        ),
                    ),
                    ("product_test".to_owned(), json!(receipts.product_test)),
                    ("reopen".to_owned(), json!(receipts.reopen)),
                    (
                        "effect_scope_violations".to_owned(),
                        json!(receipts.scope_violations),
                    ),
                ]);
                write_once(&input.root.join("native-final.json"), &final_receipt)?;
                result
            })
    })
}

async fn run_attempt(
    input: &Input,
    fixture_out: &mut Option<Fixture>,
    receipts: &mut Receipts,
) -> TestResult {
    receipts.stage = "fixture";
    let worktree = input.root.join("worktree");
    if !worktree.is_dir()
        || !fs::symlink_metadata(worktree.join(".git"))?
            .file_type()
            .is_dir()
    {
        return Err("A01 requires the registered initialized worktree".into());
    }
    *fixture_out = Some(Fixture {
        root: input.root.clone(),
        layout: StorageLayout::prepare(input.root.join("appdata"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    });
    let fixture = fixture_out.as_ref().ok_or("missing A01 fixture")?;
    let before = inventory(&worktree)?;
    receipts.stage = "capability_probe";
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_projected_profile(
        &input.root,
        &provider,
        &input.launch.model,
        input.launch.context_limit,
        input.launch.output_limit,
    )
    .await?;
    receipts.probe_verified = true;
    let clock = Instant::now();
    let allowance = AgentRunBudget::new(
        AgentTurnLimit::new(12)?,
        AgentTokenLimit::new(45_000)?,
        AgentTokenLimit::new(24_000)?,
        AgentActionLimit::new(8)?,
        AgentRunDurationLimit::from_millis(TASK_SECONDS * 1_000)?,
        AgentRepairLimit::new(1)?,
    );
    receipts.stage = "index_and_discovery";
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let project = &fixture.project;
    let catalog = DiscoverProjectCommands.execute_plan22(project.worktree().id(), &published)?;
    let command = bound_test_command(&catalog)?;
    let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
    let translation = PythonIntegerTranslation::new(
        path.clone(),
        SymbolName::try_from_string("adjust".to_owned())?,
        1,
    )?;
    let phases = LiveA01Phases {
        fixture,
        store: store.as_ref(),
        provider: &provider,
        profile: profile.clone(),
        clock,
        allowance,
        dispatches: &receipts.dispatches,
    };
    receipts.stage = "intake_and_plan";
    let (goal, stored, accounting) = phases
        .prepare(
            &published,
            &catalog,
            &input.request.objective,
            command,
            translation.clone(),
        )
        .await?;
    receipts.bound_intake = Some(json!({
        "original_bytes": input.request.objective.len(),
        "source_complete": GoalOriginalCoverage::derive_from(&goal)?.is_complete(),
        "provider_calls": accounting.usage().calls,
        "captured_at_millis": accounting.captured_at().unix_millis(),
        "absolute_deadline_millis": accounting.deadline().unix_millis(),
    }));
    let now = observed_at(clock)?;
    let (mut run, stored) = start_materialized_fixture_step(
        fixture,
        store.as_ref(),
        &goal,
        &profile,
        published.run().snapshot_id(),
        stored,
        CHANGE_STEP,
        (accounting.run_budget(now)?, now),
    )
    .await?;
    receipts.stage = "edit";
    let action = phases.edit(&goal, &stored, &run, &published, &path).await?;
    let verifier = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let deadline = accounting.deadline();
    let source_sha256 = approve_scoped_proposal(
        &action,
        &translation,
        &verifier,
        &ApprovalParseControl { clock, deadline },
    )?;
    let (mut ledger, mut version) = stored.into_parts();
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let coordinator = WorktreeMutationCoordinator::new();
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let runner = native_runner()?;
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
        &runner,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref())
    .with_python_integer_translation_verifier(&verifier)
    .with_source_failure_replan(&goal);
    receipts.stage = "apply_patch";
    let now = observed_at(clock)?.max(run.updated_at());
    let changed = execute_approved_until(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        action,
        None,
        60,
        now.unix_millis(),
        deadline,
        &seed,
        &mut compiler,
        &events,
        async |_| Ok(()),
    )
    .await?;
    let MutationControllerOutcome::StepVerified { snapshot_id, .. } = changed else {
        return Err("A01 patch did not publish its exact path evidence".into());
    };
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("A01 publication missing after patch")?;
    if current.run().snapshot_id() != snapshot_id {
        return Err("A01 published revision differs from verified patch".into());
    }
    let current_catalog =
        DiscoverProjectCommands.execute_plan22(project.worktree().id(), &current)?;
    if bound_test_command(&current_catalog)? != command {
        return Err("A01 test command changed with source patch".into());
    }
    let now = observed_at(clock)?.max(run.updated_at());
    let continuation = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            RunEventId::from_bytes([80; 32]),
            now,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::Continued(ContinueVerifiedAgentPlanOutcome::StepStarted {
        step_id,
        ledger_version,
    }) = continuation
    else {
        return Err("A01 bound verification step was not started".into());
    };
    if step_id != CHECK_STEP {
        return Err("A01 controller selected a different verification step".into());
    }
    version = ledger_version;
    let now = observed_at(clock)?.max(run.updated_at());
    let confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(project, &current_catalog, vec![command], now, None)
        .await?;
    let check = RequestAgentFinish
        .verification_command(
            ledger
                .step(step_id)
                .ok_or("A01 active verification step missing")?,
        )
        .ok_or("A01 immutable verification command missing")?;
    if check.command_id() != command {
        return Err("A01 selected another process".into());
    }
    receipts.stage = "verification";
    let checked = execute_approved_until(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(check),
        Some((&current_catalog, &confirmation)),
        100,
        now.unix_millis(),
        deadline,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let presentation = approvals
                .presentation(project, goal.task_id())?
                .ok_or("A01 process approval missing")?;
            let AgentApprovalAction::Process(spec) = presentation.action() else {
                return Err("A01 approval did not carry the test process".into());
            };
            if presentation.request_id() != request
                || presentation.context().snapshot_id() != snapshot_id
                || spec.executable() != ARGV[0]
                || !spec
                    .arguments()
                    .iter()
                    .map(String::as_str)
                    .eq(ARGV[1..].iter().copied())
                || spec.working_directory() != &AgentApprovalWorkingDirectory::Root
                || spec.timeout_millis() != 30_000
                || spec.network() != AgentApprovalNetworkScope::Denied
                || sha256(&read_bounded(&worktree.join(SUBJECT_PATH), 8_192)?) != source_sha256
            {
                return Err("A01 process approval scope or source changed".into());
            }
            Ok(())
        },
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id: verified,
    } = checked
    else {
        return Err("A01 did not obtain current test evidence".into());
    };
    if verified != snapshot_id || context.calls.load(Ordering::SeqCst) != 0 {
        return Err("A01 test changed snapshot or invoked legacy model context".into());
    }
    if !protected_inventory_matches(&before, &inventory(&worktree)?, &source_sha256) {
        receipts.scope_violations.push("unexpected_worktree_effect");
        return Err("A01 worktree differs beyond the approved source change".into());
    }
    let state = store
        .load_verification_state(
            project,
            goal.task_id(),
            &[evidence_id],
            verified,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    require_current_test(&state, command, &translation)?;
    receipts.product_test = Some(json!({
        "argv": ARGV, "cwd": ".", "timeout_seconds": 30, "test_count": 1, "outcome": "passed",
        "snapshot_current": true, "typed_test_evidence": true, "static_source_proof": true,
        "source_sha256": source_sha256,
    }));
    receipts.stage = "acceptance";
    let memory =
        RunMemoryCheckpoint::compile(&goal, &ledger, &run, state.published_index(), Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        project.clone(),
        &run,
        goal.clone(),
        ledger.clone(),
        memory,
    )?;
    let previous = run.last_event_sequence();
    let now = observed_at(clock)?.max(run.updated_at());
    if now >= deadline {
        return Err("A01 exhausted the original deadline before acceptance".into());
    }
    let acceptance = DeterministicAcceptanceVerifier::new(store.as_ref());
    let advance = VerifyAgentAcceptance::new(&acceptance)
        .execute(
            &mut run,
            &request,
            RunEventId::from_bytes([251; 32]),
            now,
            &ActiveControl,
        )
        .await?;
    AppendRunEvent::new(store.as_ref())
        .execute(project, previous, &run, advance.event())
        .await?;
    if run.state() != AgentControllerState::Done {
        return Err("A01 durable acceptance did not reach Done".into());
    }
    let before_reopen = store.load_edit_phase_usage(project, run.id()).await?;
    drop(refresh);
    drop(store);
    receipts.stage = "reopen";
    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let loaded = reopened
        .load_agent_run(project, run.id())
        .await?
        .ok_or("A01 lost its run")?;
    let reopened_goal = reopened
        .load_current_goal_contract(project, goal.task_id())
        .await?;
    let reopened_ledger = reopened.load_task_ledger(project, goal.task_id()).await?;
    if loaded != run
        || reopened_goal.as_ref() != Some(&goal)
        || reopened_ledger.as_ref().map(StoredTaskLedger::ledger) != Some(&ledger)
        || reopened.load_edit_phase_usage(project, run.id()).await? != before_reopen
    {
        return Err("A01 durable state changed on reopen".into());
    }
    let state = reopened
        .load_verification_state(
            project,
            goal.task_id(),
            &[evidence_id],
            verified,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    require_current_test(&state, command, &translation)?;
    receipts.reopen = Some(json!({"run_state": "Done", "current_evidence_bound": true}));
    receipts.stage = "complete";
    Ok(())
}

fn bound_test_command(catalog: &ProjectCommandCatalog) -> TestResult<DiscoveredCommandId> {
    let mut commands = catalog
        .commands()
        .iter()
        .filter(|command| command.kind() == DiscoveredCommandKind::Test);
    let command = commands
        .next()
        .ok_or("A01 manifest-bound unittest missing")?;
    if commands.next().is_some()
        || command.executable().as_str() != ARGV[0]
        || command.working_directory() != &WorkspaceDirectory::Root
        || !command
            .arguments()
            .iter()
            .map(|arg| arg.as_str())
            .eq(ARGV[1..].iter().copied())
    {
        return Err("A01 discovered process differs from frozen argv".into());
    }
    Ok(command.id())
}

fn approve_scoped_proposal(
    action: &AgentAction,
    required: &PythonIntegerTranslation,
    verifier: &PythonLanguageAdapter,
    control: &ApprovalParseControl,
) -> TestResult<String> {
    let AgentAction::ApplyPatch(action) = action else {
        return Err("A01 model did not produce a bound patch".into());
    };
    let [PatchOperation::Update(update)] = action.operations() else {
        return Err("A01 patch escaped its single update scope".into());
    };
    if update.expected().path() != required.path() {
        return Err("A01 patch targets another file".into());
    }
    let content = update.content();
    let result = FileRevision::new(required.path().clone(), content.content_hash());
    // Automated fixture approval may only admit reviewed pure code to the live test process.
    if verifier
        .prove_python_integer_translation(required, &result, content.as_bytes(), control)?
        .is_none()
    {
        return Err("A01 automated approval cannot admit the proposed Python source".into());
    }
    Ok(sha256(content.as_bytes()))
}

#[derive(Debug)]
struct ApprovalParseControl {
    clock: Instant,
    deadline: AgentRunTimestamp,
}

impl LanguageParseControl for ApprovalParseControl {
    fn is_cancelled(&self) -> bool {
        observed_at(self.clock).map_or(true, |now| now >= self.deadline)
    }

    fn report_progress(&self, _: Progress) -> Result<(), LanguageParseControlError> {
        if self.is_cancelled() {
            return Err(LanguageParseControlError::Unavailable);
        }
        Ok(())
    }
}

fn protected_inventory_matches(
    before: &[(PathBuf, Vec<u8>)],
    after: &[(PathBuf, Vec<u8>)],
    subject: &str,
) -> bool {
    before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .all(|((path, previous), (actual, bytes))| {
                path == actual
                    && if path == Path::new(SUBJECT_PATH) {
                        previous != bytes && sha256(bytes) == subject
                    } else {
                        previous == bytes
                    }
            })
}

fn require_current_test(
    state: &StoredVerificationState,
    command: DiscoveredCommandId,
    translation: &PythonIntegerTranslation,
) -> TestResult {
    let [VerificationEvidence::Test(test)] = state.evidence() else {
        return Err("A01 current TestEvidence missing".into());
    };
    let [case] = test.cases() else {
        return Err("A01 TestEvidence did not retain exactly one case".into());
    };
    let witness = test
        .python_integer_translation()
        .ok_or("A01 static source witness missing")?;
    if case.outcome() != TestCaseOutcome::Passed
        || test.command().command_id() != command
        || test.command().snapshot_id() != state.published_index().run().snapshot_id()
        || witness.requirement() != translation
        || !state
            .published_index()
            .publication()
            .graph()
            .files()
            .contains(witness.revision())
    {
        return Err("A01 test or source witness is stale or mismatched".into());
    }
    Ok(())
}

async fn durable_run_state(fixture: &Fixture) -> TestResult<Option<AgentControllerState>> {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    Ok(store
        .load_agent_run(&fixture.project, AgentRunId::from_bytes([RUN; 32]))
        .await?
        .map(|run| run.state()))
}

fn read_bounded(path: &Path, maximum: usize) -> TestResult<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err("A01 input is not a regular file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(u64::try_from(maximum)? + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("A01 input exceeds its closed limit".into());
    }
    Ok(bytes)
}

fn write_once(path: &Path, receipt: &Value) -> TestResult {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer(&mut file, receipt)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn hash_worker() -> TestResult<String> {
    let mut file = fs::File::open(std::env::current_exe()?)?;
    if file.metadata()?.len() > 256 * 1024 * 1024 {
        return Err("A01 worker exceeds the registered executable limit".into());
    }
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
