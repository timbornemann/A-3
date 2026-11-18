//! Actual local-model correction from a known bad seed in one fresh owned Run.
//! Test composition only; no Greenfield creation or whole-P7 claim.
use super::super::live::LiveProvider;
use super::*;
use futures::FutureExt;
use sha2::Digest;
use std::{io::Write, panic::AssertUnwindSafe};
#[path = "plan22_http_live_owner.rs"]
mod owner;
use owner::{LiveOwnerClock, validate_review_decision};

const TEST_SCOPE: &str = "known-bad-seed actual local Plan/optionalContext/Edit/optional V6 core-window-choice/followup Edit/full HTTP; not Greenfield A04/P7";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
fn atomic_json(root: &Path, name: &str, value: &serde_json::Value) -> TestResult<Vec<u8>> {
    if name.len() > 96
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("invalid private report name".into());
    }
    let temporary = root.join(format!(".{name}.pending"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    let linked = fs::hard_link(&temporary, root.join(name));
    fs::remove_file(&temporary)?;
    linked?;
    Ok(bytes)
}
fn stage(root: &Path, name: &str) -> TestResult {
    atomic_json(
        root,
        &format!("stage-{name}.json"),
        &json!({"scope":TEST_SCOPE,"stage":name}),
    )?;
    Ok(())
}
async fn now(
    http: &PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    run: &AgentRun,
    clock: &mut LiveOwnerClock,
) -> TestResult<AgentRunTimestamp> {
    fixture_now(&http.fixture, run, clock).await
}
async fn fixture_now(
    fixture: &Fixture,
    run: &AgentRun,
    clock: &mut LiveOwnerClock,
) -> TestResult<AgentRunTimestamp> {
    let db = fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let tx = connection
        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
        .await?;
    let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
        &tx,
        fixture.project.worktree().id(),
        run,
    )
    .await?;
    tx.commit().await?;
    Ok(clock.now(run, latest, &ActiveControl)?)
}
fn wire_metadata(p: ModelWireProjection) -> serde_json::Value {
    json!({"digest":format!("{:?}",p.digest()),"request_bytes":p.request_bytes(),
        "message_bytes":p.message_bytes(),"format_bytes":p.format_bytes(),
        "context":p.context_tokens(),"output":p.output_tokens(),"thinking":format!("{:?}",p.thinking())})
}
fn preflight(
    root: &Path,
    label: &str,
    provider: &dyn ModelProvider,
    goal: &GoalContract,
    requests: Vec<(&str, ModelProviderRequest)>,
) -> TestResult {
    let mut reports = Vec::new();
    for (variant, request) in requests {
        let text = request
            .messages()
            .iter()
            .map(|m| m.content())
            .collect::<String>();
        if text.len() > 32768 || SecretCandidateClassifierV1::classify(&text).is_some() {
            return Err("bounded safe fixture request required".into());
        }
        for criterion in goal.draft().acceptance_criteria() {
            if !text.contains(criterion.statement().as_str()) {
                return Err("criterion projection missing".into());
            }
        }
        for rule in goal.draft().constraints() {
            if !text.contains(rule.as_str()) {
                return Err("constraint projection missing".into());
            }
        }
        let projection = provider.project_wire_request(&request)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())?;
        if projection.context_tokens() != 16384
            || projection.output_tokens() != 1024
            || budget.output_tokens() != 3605
        {
            return Err("actual16k/1024 wire budget required".into());
        }
        reports.push(json!({"variant":variant,"wire":wire_metadata(projection),
            "prompt_reserve":budget.prompt_tokens(),"output_reserve":budget.output_tokens()}));
    }
    atomic_json(
        root,
        &format!("{label}-preflight.json"),
        &json!({"phase":label,"variants":reports}),
    )?;
    Ok(())
}
fn live_provider<'a>(
    provider: &'a dyn ModelProvider,
    http: &'a PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    run: AgentRunId,
    root: &'a Path,
    label: &'static str,
) -> LiveProvider<'a> {
    LiveProvider {
        inner: provider,
        store: http.store.as_ref(),
        project: &http.fixture.project,
        run,
        root,
        phase_label: label,
        calls: AtomicUsize::new(0),
        replies: Mutex::new(Vec::new()),
    }
}
fn truncated(observed: &LiveProvider<'_>) -> TestResult<bool> {
    Ok(observed
        .replies
        .lock()
        .map_err(|_| "trace lock")?
        .first()
        .and_then(|r| r.lock().ok().and_then(|t| t.completion))
        .is_some_and(|c| c.reason() == ModelFinishReason::OutputLimit))
}
fn traces(root: &Path, label: &str, observed: &LiveProvider<'_>) -> TestResult {
    for (index, trace) in observed
        .replies
        .lock()
        .map_err(|_| "trace lock")?
        .iter()
        .enumerate()
    {
        let trace = trace.lock().map_err(|_| "trace lock")?;
        atomic_json(
            root,
            &format!("{label}-observation-{index}.json"),
            &json!({
            "phase":label,"call":index,"raw_text_retained":false,"observed_bytes":trace.bytes,
            "observed_blake3":trace.hash.finalize().to_hex().to_string(),"capture_truncated":trace.truncated,
            "finish":trace.completion.map(|c|format!("{:?}",c.reason())),
            "prompt":trace.completion.and_then(|c|c.usage().prompt_tokens()),
            "output":trace.completion.and_then(|c|c.usage().output_tokens()),
            "failure":trace.failure.map(|e|format!("{e:?}"))}),
        )?;
    }
    Ok(())
}
async fn state_report(
    http: &PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    run: &AgentRun,
) -> TestResult<serde_json::Value> {
    let stored = http
        .store
        .load_task_ledger(&http.fixture.project, http.goal.task_id())
        .await?
        .ok_or("current ledger missing")?;
    let current = http
        .store
        .load_agent_run(&http.fixture.project, run.id())
        .await?
        .ok_or("current run missing")?;
    let usage = http
        .store
        .load_edit_phase_usage(&http.fixture.project, run.id())
        .await?;
    Ok(
        json!({"run_bytes":run.id().as_bytes(),"run_state":format!("{:?}",current.state()),
        "run_sequence":current.last_event_sequence().get(),"created_at":current.created_at().unix_millis(),
        "ledger":stored.ledger().revision().get(),"store_version":stored.version().get(),
        "calls":usage.calls,"repairs":usage.repairs,"prompt_charged":usage.prompt_tokens,
        "output_charged":usage.output_tokens,"unknown":usage.unknown_calls,"unobserved":usage.unobserved_calls}),
    )
}
async fn halt(
    http: &PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    run: &AgentRun,
    stage: &str,
    kind: &str,
) -> TestResult<serde_json::Value> {
    Ok(
        json!({"terminal":"regular_halt","halt_stage":stage,"halt_kind":kind,
        "functional_component_passed":false,"state":state_report(http,run).await?,
        "a04_completed":false,"p7_completed":false,"semantic_retries":0}),
    )
}

// Real S1 admission/read, using the actual returned parent and current global time.
async fn live_context(
    mut f: CurrentPlanFixture,
    parent: HttpCorrectionPlanProposal,
    clock: &mut LiveOwnerClock,
    root: &Path,
) -> TestResult<ContextFixture> {
    let parent_job = f.phase.job_digest();
    if !matches!(
        parent.decision(),
        HttpCorrectionPlanDecision::NeedOfferedSource(_)
    ) {
        return Err("actual S1 required".into());
    }
    let project = &f.http.fixture.project;
    let current = f
        .http
        .store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("current index missing")?;
    let bound = bound_offer(&f, &current).await?;
    let tool = ToolRunId::from_bytes([242; 32]);
    let admission = f
        .http
        .store
        .admit_http_context_read(
            project,
            &f.phase,
            &bound,
            tool,
            now(&f.http, &f.run, clock).await?,
        )
        .await?;
    let reader = DeterministicAgentReadTools::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let mut read = reader
        .execute(
            project,
            current.run().snapshot_id(),
            tool,
            &bound.read_action(project, &current)?,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = bound.take_original(project, &current, tool, &mut read)?;
    let previous = f.run.last_event_sequence();
    let at = now(&f.http, &f.run, clock).await?;
    let recorded = read.record(&mut f.run, RunEventId::from_bytes([242; 32]), at)?;
    AppendAgentRead::new(f.http.store.as_ref())
        .execute(project, previous, &f.run, &recorded)
        .await?;
    atomic_json(
        root,
        "s1-read.json",
        &json!({"actual_read":true,"source_bytes":page.text().len(),
        "F":previous.get(),"R":f.run.last_event_sequence().get(),"parent_job":format!("{parent_job:?}"),
        "read_key":format!("{:?}",admission.key()),"scope":"ordinary actual S1 Original; no Edit delivery"}),
    )?;
    drop((reader, bound, current, page));
    let f = edit::cold(f).await?;
    let c = rebuild_context(f).await?;
    if c.origin.parent_job != parent_job || c.origin.parent_proposal != parent {
        return Err("actual cold parent differs".into());
    }
    now(&c.f.http, &c.f.run, clock).await?;
    Ok(c)
}

// Shared owned test-worker patch boundary. Both callers exercise the same real
// approval, lease, revalidation, publication and current Diff path on the heap.
async fn apply_live_patch(
    fixture: edit::HttpEditFixture,
    action: AgentAction,
    floor: AgentRunTimestamp,
) -> TestResult<Fixture> {
    Box::pin(edit::patch_with_floor(fixture, action, floor)).await
}

fn publish_review_ready(
    root: &Path,
    binding: &serde_json::Value,
    source: &[u8],
    patch_state: &serde_json::Value,
) -> TestResult<String> {
    let source_hash = sha(source);
    let blake = blake3::hash(source);
    let manifest = json!({"version":1,"scope":TEST_SCOPE,"owner":binding,"state":patch_state,
        "source_sha256":source_hash,"source_blake3":blake.to_hex().to_string(),
        "source_bytes":source.len(),"relative_script":"server.py","same_owner_required":true});
    let bytes = atomic_json(root, "review-ready.json", &manifest)?;
    Ok(sha(&bytes))
}

async fn wait_review(
    root: &Path,
    fixture: &Fixture,
    run: &AgentRun,
    clock: &mut LiveOwnerClock,
    binding: &serde_json::Value,
    source: &[u8],
    patch_state: &serde_json::Value,
) -> TestResult<bool> {
    let source_hash = sha(source);
    let blake = blake3::hash(source);
    let ready_hash = publish_review_ready(root, binding, source, patch_state)?;
    let nonce = binding["owner_nonce"]
        .as_str()
        .ok_or("owner nonce missing")?;
    let mut progress = Instant::now();
    loop {
        fixture_now(fixture, run, clock).await?;
        let path = root.join("review-decision.json");
        if path.exists() {
            let canonical = fs::canonicalize(&path)?;
            if canonical.parent() != Some(root)
                || canonical.file_name() != path.file_name()
                || fs::metadata(&canonical)?.len() > 8192
            {
                return Err("review decision escaped private root".into());
            }
            let decision = fs::read(canonical)?;
            match validate_review_decision(&decision, nonce, &ready_hash) {
                Ok(reviewer) => {
                    let current = fs::read(
                        fixture
                            .project
                            .worktree()
                            .root()
                            .as_path()
                            .join("server.py"),
                    )?;
                    if current != source || sha(&current) != source_hash {
                        return Err("reviewed source changed".into());
                    }
                    atomic_json(
                        root,
                        "review-accepted.json",
                        &json!({"reviewer":reviewer,"ready_sha256":ready_hash,
                        "source_sha256":source_hash,"source_blake3":blake.to_hex().to_string(),
                        "product_grant":false,"same_owner":true}),
                    )?;
                    return Ok(true);
                }
                Err(owner::LiveOwnerFailure::ReviewDenied) => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        if progress.elapsed() >= Duration::from_secs(5) {
            eprintln!("live correction awaiting exact source review");
            progress = Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[test]
#[ignore = "fresh preregistered actual local Plan/Context/Edit and exact in-owner code review; known seed component only"]
fn http_correction_same_run_local_ollama_executes_full_recipe_from_seed() -> TestResult {
    crate::run_native_libsql_live_test(async {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            let input=PathBuf::from(std::env::var_os("A3_HTTP_SAME_RUN_OUTPUT").ok_or("fresh output required")?);
            let allowed=fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plan22-p7-runs"))?;
            if !input.is_absolute() || fs::canonicalize(input.parent().ok_or("parent missing")?)?!=allowed {return Err("exact retained root required".into());}
            fs::create_dir(&input)?;let root=fs::canonicalize(&input)?;
            let control_path=PathBuf::from(std::env::var_os("A3_HTTP_SAME_RUN_LAUNCH").ok_or("launch required")?);
            let control_path=fs::canonicalize(control_path)?;
            if control_path.parent()!=Some(allowed.as_path()) || fs::metadata(&control_path)?.len()>16384 {return Err("bounded launch required".into());}
            let binding:serde_json::Value=serde_json::from_slice(&fs::read(control_path)?)?;
            let mut current_stage="bootstrap";
            let result=AssertUnwindSafe(Box::pin(run_case(&root,&binding,&mut current_stage))).catch_unwind().await;
            let mut final_report=match result {
                Ok(Ok(value))=>value,
                Ok(Err(error))=>match error.downcast_ref::<owner::LiveOwnerFailure>() {
                    Some(kind)=>json!({"terminal":"owner_halt","stage":current_stage,
                        "halt_kind":format!("{kind:?}"),"functional_component_passed":false}),
                    None=>json!({"terminal":"unverified_error","stage":current_stage,"functional_component_passed":false}),
                },
                Err(_)=>json!({"terminal":"unverified_worker_panic","stage":current_stage,"functional_component_passed":false}),
            };
            let mut dispatch_counts=serde_json::Map::new();
            let mut started_calls=0usize;
            let mut valid_inventory=true;
            for label in ["plan","context","edit","no-change-plan","followup-edit"] {
                let calls=(0..2).filter(|index|root.join(format!("{label}-dispatch-{index}.json")).is_file()).count();
                if root.join(format!("{label}-dispatch-1.json")).is_file()
                    && !root.join(format!("{label}-dispatch-0.json")).is_file() {valid_inventory=false;}
                started_calls+=calls;dispatch_counts.insert(label.to_owned(),json!(calls));
            }
            final_report["phase_calls_by_label"]=json!(dispatch_counts);
            final_report["actual_started_phase_calls"]=json!(started_calls);
            final_report["phase_inventory_valid"]=json!(valid_inventory);
            final_report["b2_entered"]=json!(root.join("no-change-closure.json").is_file());
            if !valid_inventory {final_report["functional_component_passed"]=json!(false);}
            final_report["scope"]=json!(TEST_SCOPE);final_report["semantic_retries"]=json!(0);
            final_report["a04_completed"]=json!(false);final_report["p7_completed"]=json!(false);
            atomic_json(&root,"same-run-final.json",&final_report)?;
            Ok(())
        })
    })
}

// One bounded same-Run continuation, never a second NoChange planning loop.
struct LiveWindowReady {
    fixture: edit::HttpEditFixture,
    page: AgentSourcePage,
    current: PublishedIndex,
    report: EditPhaseExecutionReport,
    correction: TaskStepId,
    plan_calls: usize,
    edit_calls: usize,
}
enum LiveWindowResult {
    Halt(serde_json::Value),
    Ready(Box<LiveWindowReady>),
}
fn closed_inspection(original: &EvidenceRef) -> TestResult<AgentFileInspection> {
    let range = original.range();
    let count = range
        .end_position()
        .row()
        .checked_add(u32::from(range.end_position().column() > 0))
        .and_then(|last| last.checked_sub(range.start_position().row()))
        .ok_or("closed Original line envelope")?;
    Ok(AgentFileInspection::new(
        original.revision().path().clone(),
        AgentFileStartLine::new(range.start_position().row() + 1)?,
        AgentFileLineCount::new(u16::try_from(count)?)?,
    ))
}
fn range_report(range: SourceRange) -> serde_json::Value {
    json!([
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column()
    ])
}
// Both actual and declared native consumers own precisely this concrete continuation.
// Size is the unpolled future representation, never a claim about peak thread stack.
#[derive(Clone, Copy)]
enum LiveWindowInput {
    ActualLocalModel,
    DeclaredNativeControl,
}
impl LiveWindowInput {
    const fn controlled(self) -> bool {
        matches!(self, Self::DeclaredNativeControl)
    }
}
fn owned_window_continuation<'a>(
    f: edit::HttpEditFixture,
    provider: &'a dyn ModelProvider,
    root: &'a Path,
    clock: &'a mut LiveOwnerClock,
    current_stage: &'a mut &'static str,
    input: LiveWindowInput,
) -> (
    usize,
    impl std::future::Future<Output = TestResult<LiveWindowResult>> + 'a,
) {
    let future = continue_after_no_change(f, provider, root, clock, current_stage, input);
    let bytes = std::mem::size_of_val(&future);
    (bytes, Box::pin(future))
}

async fn continue_after_no_change(
    mut f: edit::HttpEditFixture,
    provider: &dyn ModelProvider,
    root: &Path,
    clock: &mut LiveOwnerClock,
    current_stage: &mut &'static str,
    input: LiveWindowInput,
) -> TestResult<LiveWindowResult> {
    *current_stage = "no-change-plan-prepare";
    stage(root, current_stage)?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("closed current index missing")?;
    let resume = f
        .http
        .store
        .load_http_no_change_replan(
            &f.http.fixture.project,
            &f.http.goal,
            &f.stored,
            &f.run,
            &current,
            &f.http.model,
            now(&f.http, &f.run, clock).await?,
        )
        .await?
        .ok_or("genuine closed HTTP NoChange missing")?;
    if !matches!(resume.state(), HttpNoChangeReplanResumeState::Unreserved) {
        return Err("fresh V6 job already consumed; never resume or repeat".into());
    }
    let original = resume.closed().fact().window().original();
    let old_span = original.range();
    let exact = WorkspaceAgentSourceReader
        .read_page(
            &f.http.fixture.project,
            original.revision(),
            &closed_inspection(original)?,
            &ActiveControl,
        )
        .await?;
    now(&f.http, &f.run, clock).await?;
    let phase = match PreparedHttpNoChangeReplanV6Job::prepare(
        &f.http.fixture.project,
        &f.http.goal,
        &f.stored,
        &f.run,
        &current,
        f.http.model.clone(),
        resume.closed().clone(),
        &exact,
    ) {
        Ok(phase) => phase,
        Err(HttpNoChangeReplanFailure::NoSmallerWindow) => {
            atomic_json(
                root,
                "no-change-plan-result.json",
                &json!({
                "calls":0,"dispatched":false,"error":null,"canonical":null,
                "regular_halt":"noSmallerWindow","state":state_report(&f.http,&f.run).await?}),
            )?;
            return Ok(LiveWindowResult::Halt(
                halt(&f.http, &f.run, "no-change-plan", "noSmallerWindow").await?,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    atomic_json(
        root,
        "no-change-plan-origin.json",
        &json!({
        "same_run":true,"closed_sequence":resume.closed().closure().sequence().get(),
        "old_window":range_report(old_span),"old_job":format!("{:?}",resume.closed().fact().job_digest()),
        "v5_job":format!("{:?}",phase.job_digest()),"actual_safe_reread":true,
        "inherited_delivery_credit":false,"controlled_model":input.controlled()}),
    )?;
    drop((resume, exact, current));
    *current_stage = "no-change-plan";
    stage(root, current_stage)?;
    preflight(
        root,
        "no-change-plan",
        provider,
        &f.http.goal,
        vec![
            ("initial", phase.request_for(EditPhaseCallKind::Initial)?),
            (
                "repair",
                phase.request_for(EditPhaseCallKind::StructuralRepair)?,
            ),
        ],
    )?;
    let observed = live_provider(provider, &f.http, f.run.id(), root, "no-change-plan");
    let first = ExecuteHttpNoChangeReplan::new(
        &observed,
        &WorkspaceAgentSourceReader,
        f.http.store.as_ref(),
    )
    .execute(
        &f.http.fixture.project,
        &phase,
        EditPhaseCallKind::Initial,
        now(&f.http, &f.run, clock).await?,
        ModelRequestTimeout::from_millis(120000)?,
        &ActiveControl,
    )
    .await;
    // InvalidReply alone is insufficient: only the stored actual initial
    // InvalidStructure of this exact job can consume the one structural repair.
    let repair_allowed = if matches!(
        &first,
        Err(HttpNoChangeReplanExecutionFailure::InvalidReply)
    ) {
        let current = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("V6 repair index missing")?;
        match f
            .http
            .store
            .load_http_no_change_replan(
                &f.http.fixture.project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &current,
                &f.http.model,
                now(&f.http, &f.run, clock).await?,
            )
            .await
        {
            Ok(Some(resume)) => matches!(resume.state(),HttpNoChangeReplanResumeState::Observed(s)
                if s.job_digest()==phase.job_digest() && s.call_kind()==EditPhaseCallKind::Initial
                && s.observation().outcome()==EditPhaseObservationOutcome::InvalidStructure),
            _ => false,
        }
    } else {
        false
    };
    let result = if repair_allowed {
        ExecuteHttpNoChangeReplan::new(
            &observed,
            &WorkspaceAgentSourceReader,
            f.http.store.as_ref(),
        )
        .execute(
            &f.http.fixture.project,
            &phase,
            EditPhaseCallKind::StructuralRepair,
            now(&f.http, &f.run, clock).await?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await
    } else {
        first
    };
    traces(root, "no-change-plan", &observed)?;
    let plan_calls = observed.calls.load(Ordering::SeqCst);
    drop(observed);
    let canonical = match &result {
        Ok(report) => report
            .settlement
            .decision()
            .map(|decision| decision.canonical_retained_bytes(phase.job_digest()))
            .transpose()?
            .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes))
            .transpose()?,
        Err(_) => None,
    };
    atomic_json(
        root,
        "no-change-plan-result.json",
        &json!({
        "calls":plan_calls,"error":result.as_ref().err().map(|e|format!("{e:?}")),
        "canonical":canonical,"actual_stored_structural_repair":repair_allowed,
        "controlled_model":input.controlled(),"state":state_report(&f.http,&f.run).await?}),
    )?;
    let report = result?;
    match report
        .settlement
        .decision()
        .ok_or("V6 settled decision missing")?
    {
        HttpNoChangeReplanDecision::Window { .. } => {}
        HttpNoChangeReplanDecision::NoProgress { .. } => {
            return Ok(LiveWindowResult::Halt(
                halt(&f.http, &f.run, "no-change-plan", "noProgress").await?,
            ));
        }
        HttpNoChangeReplanDecision::Halt(reason) => {
            let kind = match reason {
                HttpNoChangeReplanHalt::NeedEvidence => "needEvidence",
                HttpNoChangeReplanHalt::TooLarge => "tooLarge",
                HttpNoChangeReplanHalt::Refuse => "refuse",
            };
            return Ok(LiveWindowResult::Halt(
                halt(&f.http, &f.run, "no-change-plan", kind).await?,
            ));
        }
    }
    drop(report);
    *current_stage = "no-change-material";
    stage(root, current_stage)?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("V6 material current index missing")?;
    let (mut ledger, version) = f.stored.clone().into_parts();
    let apply_at = now(&f.http, &f.run, clock).await?;
    let applied = Box::pin(
        ApplyHttpNoChangeWindowPlan::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &WorkspaceAgentSourceReader,
        )
        .execute(
            &f.http.fixture.project,
            &f.http.goal,
            &mut f.run,
            &mut ledger,
            version,
            &current,
            &phase,
            apply_at,
            &ActiveControl,
        ),
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, applied.version);
    drop((current, phase));
    f = edit::reopen(f).await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold V6 material index missing")?;
    let origin = f
        .http
        .store
        .load_applied_http_no_change_window_origin(
            &f.http.fixture.project,
            &f.http.goal,
            &f.stored,
            &f.run,
            &current,
            &f.http.model,
            now(&f.http, &f.run, clock).await?,
        )
        .await?;
    let selected = origin.material().selected_window().range();
    if !old_span.contains(selected) || selected.is_empty() || selected.len() >= old_span.len() {
        return Err("actual V6 proper subset required".into());
    }
    atomic_json(
        root,
        "no-change-window-material.json",
        &json!({
        "same_run":true,"actual_material":true,"proper_subset":true,
        "old_window":range_report(old_span),"new_window":range_report(selected),
        "closed_sequence":origin.material().closed_origin().closure().sequence().get(),
        "material_sequence":f.run.last_event_sequence().get(),
        "correction":format!("{:?}",applied.correction),"proof":format!("{:?}",applied.proof),
        "state":state_report(&f.http,&f.run).await?}),
    )?;
    drop((origin, current));
    *current_stage = "followup-read";
    stage(root, current_stage)?;
    let (phase, mut compiled, page) = Box::pin(own_followup_window_read(&mut f, clock)).await?;
    let correction = phase.anchor().step();
    if page.range() != selected {
        return Err("new-step Original differs from accepted V6 window".into());
    }
    atomic_json(
        root,
        "followup-read.json",
        &json!({
        "actual_new_step_read":true,"old_read_reused":false,"new_window":range_report(page.range()),
        "state":state_report(&f.http,&f.run).await?}),
    )?;
    *current_stage = "followup-edit";
    stage(root, current_stage)?;
    let structural = compiled.for_structural_repair()?;
    let cut = compiled.for_truncated_output_repair()?;
    preflight(
        root,
        "followup-edit",
        provider,
        &f.http.goal,
        vec![
            ("initial", compiled.request().clone()),
            ("repair", structural.request().clone()),
            ("repair-truncated", cut.request().clone()),
        ],
    )?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("followup current index missing")?;
    let observed = live_provider(provider, &f.http, f.run.id(), root, "followup-edit");
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let first = Box::pin(
        ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch).execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                edit::state(&f, &current)?,
                now(&f.http, &f.run, clock).await?,
                &patch,
                &ActiveControl,
            ))
            .await?,
            edit::state(&f, &current)?,
            &compiled,
            now(&f.http, &f.run, clock).await?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        ),
    )
    .await;
    let repair = match &first {
        Err(EditPhaseExecutionFailure::OutputLimit) => Some(&cut),
        Err(EditPhaseExecutionFailure::Reply(
            EditPhaseFailure::InvalidReply
            | EditPhaseFailure::InvalidReplacement
            | EditPhaseFailure::TooLarge,
        )) => Some(&structural),
        _ => None,
    };
    let result = if let Some(repair) = repair {
        Box::pin(
            ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch).execute(
                Box::pin(
                    crate::plan22_goal_contract_tests::prepare_indexed_bound_edit(
                        &f.http.compiler,
                        PreparedEditPhase::update(edit::state(&f, &current)?, &page)?,
                        repair,
                        edit::state(&f, &current)?,
                        now(&f.http, &f.run, clock).await?,
                        &patch,
                        &ActiveControl,
                    ),
                )
                .await?,
                edit::state(&f, &current)?,
                repair,
                now(&f.http, &f.run, clock).await?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            ),
        )
        .await
    } else {
        first
    };
    traces(root, "followup-edit", &observed)?;
    let edit_calls = observed.calls.load(Ordering::SeqCst);
    drop(observed);
    atomic_json(
        root,
        "followup-edit-result.json",
        &json!({
        "calls":edit_calls,"request_digest":format!("{:?}",repair.unwrap_or(&compiled).request_digest()),"error":result.as_ref().err().map(|e|format!("{e:?}")),
        "controlled_model":input.controlled(),"second_v6_allowed":false,"state":state_report(&f.http,&f.run).await?}),
    )?;
    if matches!(
        &result,
        Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
    ) {
        let source = fs::read(f.http.root.join("server.py"))?;
        let (mut ledger, version) = f.stored.clone().into_parts();
        let closure_at = now(&f.http, &f.run, clock).await?;
        // Use only the ordinary current A contract. A denial remains an error;
        // never broaden first-F/closed lineage or begin another V6 job.
        let version = CloseEditNoChange::new(f.http.store.as_ref(), f.http.store.as_ref(), &patch)
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut f.run,
                &mut ledger,
                version,
                &current,
                &f.http.model,
                &page,
                closure_at,
                &ActiveControl,
            )
            .await?;
        f.stored = StoredTaskLedger::new(ledger, version);
        drop(current);
        f = edit::reopen(f).await?;
        let failed = f
            .stored
            .ledger()
            .step(correction)
            .ok_or("second NoChange step missing")?;
        if f.run.state() != AgentControllerState::Replan
            || failed.status() != TaskStepStatus::Failed
            || fs::read(f.http.root.join("server.py"))? != source
        {
            return Err("second NoChange ordinary cold closure differs".into());
        }
        atomic_json(
            root,
            "followup-no-change-closure.json",
            &json!({
            "typed_current_no_change":true,"cold_failed_attempt":true,"cold_replan":true,
            "second_v6_started":false,"semantic_retries":0,"source_sha256":sha(&source),
            "state":state_report(&f.http,&f.run).await?}),
        )?;
        return Ok(LiveWindowResult::Halt(
            halt(&f.http, &f.run, "followup-edit", "noChange").await?,
        ));
    }
    let report = result?;
    let disposition = match &report.outcome {
        EditPhaseOutcome::NeedEvidence(need) => Some(EditPhaseHalt::NeedEvidence(*need)),
        EditPhaseOutcome::TooLarge => Some(EditPhaseHalt::TooLarge),
        EditPhaseOutcome::Proposal(_) => None,
    };
    if let Some(disposition) = disposition {
        if report.source_delivery.is_some() {
            return Err("halt acquired false source credit".into());
        }
        return Ok(LiveWindowResult::Halt(
            Box::pin(durable_edit_halt(
                f,
                repair.unwrap_or(&compiled),
                disposition,
                "followup-edit",
                root,
            ))
            .await?,
        ));
    }
    Ok(LiveWindowResult::Ready(Box::new(LiveWindowReady {
        fixture: f,
        page,
        current,
        report,
        correction,
        plan_calls,
        edit_calls,
    })))
}

async fn own_followup_window_read(
    f: &mut edit::HttpEditFixture,
    clock: &mut LiveOwnerClock,
) -> TestResult<(PreparedEditPhase, CompiledEditPhaseRequest, AgentSourcePage)> {
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let (mut ledger, mut version) = f.stored.clone().into_parts();
    let activate_at = now(&f.http, &f.run, clock).await?;
    let diagnostic = ActivateHttpNoChangeWindowEdit::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    )
    .execute(
        &f.http.fixture.project,
        &f.http.goal,
        &mut f.run,
        &mut ledger,
        &mut version,
        &current,
        &f.http.model,
        activate_at,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, version);
    let required = diagnostic.required_source();
    let seed = TaskLensSeedText::try_from_string(
        "Read the accepted smaller current HTTP correction window".to_owned(),
    )?;
    let lens = CompileTaskLens::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
    )
    .execute(
        &f.http.fixture.project,
        TaskLensSeedSet::new(
            seed.clone(),
            seed,
            vec![TaskLensSeed::ExplicitPath(
                required.revision().path().clone(),
            )],
        )?,
        TaskLensTokenBudget::new(4096)?,
        &ActiveControl,
    )
    .await?;
    let bound = diagnostic.bind_source(&f.http.fixture.project, &current, &lens)?;
    let mut tool_id = blake3::Hasher::new_derive_key("a3.live-http-no-change-window-tool.v1");
    tool_id.update(f.run.id().as_bytes());
    tool_id.update(edit::state(f, &current)?.anchor().step().as_bytes());
    let tool = ToolRunId::from_bytes(*tool_id.finalize().as_bytes());
    f.http
        .store
        .begin_source_read(
            &f.http.fixture.project,
            edit::state(f, &current)?.anchor(),
            &bound,
            tool,
            now(&f.http, &f.run, clock).await?,
        )
        .await?;
    let reader = DeterministicAgentReadTools::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let mut result = reader
        .execute(
            &f.http.fixture.project,
            current.run().snapshot_id(),
            tool,
            &bound.read_action(&f.http.fixture.project, &current)?,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = bound.take_original(&f.http.fixture.project, &current, tool, &mut result)?;
    let seq = f.run.last_event_sequence();
    let read_at = now(&f.http, &f.run, clock).await?;
    let mut event_id = blake3::Hasher::new_derive_key("a3.live-http-no-change-window-read.v1");
    event_id.update(tool.as_bytes());
    event_id.update(&seq.get().to_le_bytes());
    let recorded = result.record(
        &mut f.run,
        RunEventId::from_bytes(*event_id.finalize().as_bytes()),
        read_at,
    )?;
    AppendAgentRead::new(f.http.store.as_ref())
        .execute(&f.http.fixture.project, seq, &f.run, &recorded)
        .await?;
    let reads = f
        .http
        .store
        .load_source_read_state(&f.http.fixture.project, edit::state(f, &current)?.anchor())
        .await?;
    if !reads.progress().receipts().is_empty() {
        return Err("new B read has prior credit".into());
    }
    let acquired = reads.acquired().ok_or("new actual B Original")?;
    if acquired.original() != required || acquired.original().range() != page.range() {
        return Err("actual new B Original differs".into());
    }
    let phase = PreparedEditPhase::update(edit::state(f, &current)?, &page)?;
    let mut compiled = build_edit_phase_request(
        &f.http.goal,
        &f.stored,
        &phase,
        edit::state(f, &current)?,
        f.http.model.clone(),
        None,
        EditPhasePrerequisites::none()
            .with_http_no_change_window_failure(&diagnostic)
            .with_required_source(
                EditEvidenceNeed::Definition,
                &bound,
                Some((&page, acquired)),
            ),
    )?;
    if !compiled.request().messages()[1]
        .content()
        .contains("previous correction changed zero source bytes")
    {
        return Err("new Edit NoChange diagnosis missing".into());
    }
    let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
        &f.http.compiler,
        phase,
        &mut compiled,
        edit::state(f, &current)?,
        now(&f.http, &f.run, clock).await?,
        &WorkspacePatchAdapter::with_change_window_preview(),
        &ActiveControl,
    ))
    .await?;
    drop((reads, current));
    Ok((phase, compiled, page))
}

fn edit_halt_kind(halt: EditPhaseHalt) -> &'static str {
    match halt {
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Definition) => "definition",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Caller) => "caller",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test) => "test",
        EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Unknown) => "unknown_need",
        EditPhaseHalt::TooLarge => "too_large",
    }
}
async fn durable_edit_halt(
    f: edit::HttpEditFixture,
    compiled: &CompiledEditPhaseRequest,
    disposition: EditPhaseHalt,
    phase_stage: &str,
    root: &Path,
) -> TestResult<serde_json::Value> {
    let f = edit::reopen(f).await?;
    let receipt = f
        .http
        .store
        .load_edit_halt(&f.http.fixture.project, compiled)
        .await?
        .ok_or("cold typed Edit halt missing")?;
    if receipt.disposition() != disposition || receipt.request_digest() != compiled.request_digest()
    {
        return Err("cold typed Edit halt differs".into());
    }
    let kind = edit_halt_kind(disposition);
    let record = json!({"edit_protocol":3,"stage":phase_stage,"kind":kind,"cold_reopened":true,
        "request_digest":format!("{:?}",receipt.request_digest()),"wire_digest":format!("{:?}",receipt.wire().digest()),
        "request_fact_only":true,"source_delivery":false,"no_new_no_change_receipt":true,
        "automatic_resolution":false,"new_provider_calls":0,"patch_executed":false,"functional_component_passed":false});
    atomic_json(root, "edit-halt.json", &record)?;
    let reason = if matches!(disposition, EditPhaseHalt::TooLarge) {
        "tooLarge"
    } else {
        "needEvidence"
    };
    let mut result = halt(&f.http, &f.run, phase_stage, reason).await?;
    result["durable_edit_halt"] = record;
    Ok(result)
}

async fn run_case(
    root: &Path,
    binding: &serde_json::Value,
    current_stage: &mut &'static str,
) -> TestResult<serde_json::Value> {
    if binding["candidate_schema"] != crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        || binding["edit_protocol"] != 3
        || binding["http_no_change_replan_protocol"] != 6
        || binding["context"] != 16384
        || binding["output"] != 1024
    {
        return Err("fresh current candidate V6 exact-profile owner binding required".into());
    }
    let tag = binding["model"].as_str().ok_or("model missing")?;
    if !["qwen38-4k:latest", "ornith-1.5:9b"].contains(&tag) {
        return Err("unsupported model".into());
    }
    let mut clock = LiveOwnerClock::begin(
        binding["started_utc_unix_ms"]
            .as_u64()
            .ok_or("owner time missing")?,
    )?;
    *current_stage = "probe";
    stage(root, current_stage)?;
    let provider = encoder()?;
    let profile = ProbeModelProfile::new(&provider)
        .execute(
            &ModelCapabilityProbeRequest::new(
                ModelId::try_from_string(tag.to_owned())?,
                ollama_profile_with_context(tag, HttpFixtureContext::SixteenK)?
                    .settings()
                    .clone(),
            ),
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await?;
    if !profile.executable_actions_enabled()
        || profile.settings().context_limit().get() != 16384
        || profile.settings().output_limit().get() != 1024
    {
        return Err("verified exact profile required".into());
    }
    atomic_json(
        root,
        "profile.json",
        &json!({"model":tag,"profile":format!("{:?}",profile.reference()),
        "context":16384,"output":1024,"actual_probe":true,"probe_io_separate":true}),
    )?;
    fs::create_dir(root.join("worktree"))?;
    initialize_git(&root.join("worktree"))?;
    let fixture = Fixture {
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(root.join("worktree"))?,
        root: root.to_path_buf(),
    };
    *current_stage = "seed";
    stage(root, current_stage)?;
    let mut f = full_fixture_using(profile.clone(), true, fixture).await?;
    let db = f.http.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    if crate::migration::read_user_version(&connection).await?
        != crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION
    {
        return Err("actual freshly initialized candidate head required".into());
    }
    drop((connection, db));
    clock.bind(&f.run)?;
    let run_id = f.run.id();
    let original = f.http.goal.original().ok_or("original missing")?;
    if original.as_bytes().len() != 1207 {
        return Err("original goal changed".into());
    }
    atomic_json(
        root,
        "seed.json",
        &json!({"candidate_schema":crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION,"actual_schema_checked":true,"http_no_change_replan_protocol":6,
        "known_bad_seed":true,"greenfield_history":false,
        "automatic_mismatch_admission":false,"physical_mismatch":true,"run_bytes":run_id.as_bytes(),
        "original_sha256":sha(original.as_bytes()),"seed_source_sha256":sha(FROZEN_SOURCE.as_bytes()),
        "state":state_report(&f.http,&f.run).await?}),
    )?;
    *current_stage = "plan";
    stage(root, current_stage)?;
    preflight(
        root,
        "plan",
        &provider,
        &f.http.goal,
        vec![
            ("initial", f.phase.request_for(EditPhaseCallKind::Initial)?),
            (
                "repair",
                f.phase.request_for(EditPhaseCallKind::StructuralRepair)?,
            ),
        ],
    )?;
    let observed = live_provider(&provider, &f.http, run_id, root, "plan");
    let first = ExecuteHttpCorrectionPlan::new(
        &observed,
        &WorkspaceAgentSourceReader,
        f.http.store.as_ref(),
    )
    .execute(
        &f.http.fixture.project,
        &f.phase,
        EditPhaseCallKind::Initial,
        now(&f.http, &f.run, &mut clock).await?,
        ModelRequestTimeout::from_millis(120000)?,
        &ActiveControl,
    )
    .await;
    let result = if matches!(
        &first,
        Err(HttpCorrectionPlanExecutionFailure::InvalidReply)
    ) {
        ExecuteHttpCorrectionPlan::new(
            &observed,
            &WorkspaceAgentSourceReader,
            f.http.store.as_ref(),
        )
        .execute(
            &f.http.fixture.project,
            &f.phase,
            EditPhaseCallKind::StructuralRepair,
            now(&f.http, &f.run, &mut clock).await?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await
    } else {
        first
    };
    traces(root, "plan", &observed)?;
    let plan_calls = observed.calls.load(Ordering::SeqCst);
    drop(observed);
    atomic_json(
        root,
        "plan-result.json",
        &json!({"calls":plan_calls,"controlled_parent":false,
        "error":result.as_ref().err().map(|e|format!("{e:?}")),
        "canonical":result.as_ref().ok().map(|r|r.proposal.canonical_retained_bytes()).transpose()?.map(|b|serde_json::from_slice::<serde_json::Value>(&b)).transpose()?,
        "state":state_report(&f.http,&f.run).await?}),
    )?;
    let report = result?;
    let parent = report.proposal;
    let mut context_calls = 0usize;
    let mut f = match parent.decision() {
        HttpCorrectionPlanDecision::FocusLine(_) => {
            *current_stage = "direct-apply";
            stage(root, current_stage)?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("current index missing")?;
            let (mut ledger, version) = f.stored.clone().into_parts();
            let apply_at = now(&f.http, &f.run, &mut clock).await?;
            let applied = ApplyHttpCorrectionPlan::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspaceAgentSourceReader,
            )
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut f.run,
                &mut ledger,
                version,
                &current,
                &f.phase,
                apply_at,
                &ActiveControl,
            )
            .await?;
            f.stored = StoredTaskLedger::new(ledger, applied.version);
            drop(current);
            edit::cold(f).await?
        }
        HttpCorrectionPlanDecision::NeedOfferedSource(_) => {
            *current_stage = "s1-read";
            stage(root, current_stage)?;
            let mut c = live_context(f, parent, &mut clock, root).await?;
            *current_stage = "context";
            stage(root, current_stage)?;
            let cut = c.phase.for_truncated_output_repair()?;
            preflight(
                root,
                "context",
                &provider,
                &c.f.http.goal,
                vec![
                    ("initial", c.phase.request_for(EditPhaseCallKind::Initial)?),
                    (
                        "repair",
                        c.phase.request_for(EditPhaseCallKind::StructuralRepair)?,
                    ),
                    (
                        "repair-truncated",
                        cut.request_for(EditPhaseCallKind::StructuralRepair)?,
                    ),
                ],
            )?;
            let observed = live_provider(&provider, &c.f.http, run_id, root, "context");
            let first = ExecuteHttpContextIntent::new(
                &observed,
                &WorkspaceAgentSourceReader,
                c.f.http.store.as_ref(),
            )
            .execute(
                &c.f.http.fixture.project,
                &c.phase,
                EditPhaseCallKind::Initial,
                now(&c.f.http, &c.f.run, &mut clock).await?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await;
            let repair_phase = if truncated(&observed)? {
                &cut
            } else {
                &c.phase
            };
            let result = if matches!(&first, Err(HttpContextIntentExecutionFailure::InvalidReply)) {
                ExecuteHttpContextIntent::new(
                    &observed,
                    &WorkspaceAgentSourceReader,
                    c.f.http.store.as_ref(),
                )
                .execute(
                    &c.f.http.fixture.project,
                    repair_phase,
                    EditPhaseCallKind::StructuralRepair,
                    now(&c.f.http, &c.f.run, &mut clock).await?,
                    ModelRequestTimeout::from_millis(120000)?,
                    &ActiveControl,
                )
                .await
            } else {
                first
            };
            traces(root, "context", &observed)?;
            context_calls = observed.calls.load(Ordering::SeqCst);
            drop(observed);
            atomic_json(
                root,
                "context-result.json",
                &json!({"calls":context_calls,"error":result.as_ref().err().map(|e|format!("{e:?}")),
                "intent":result.as_ref().ok().and_then(|r|r.reply.intent().map(|i|i.as_str())),"state":state_report(&c.f.http,&c.f.run).await?}),
            )?;
            let report = result?;
            let ready = report.reply;
            drop(cut);
            c = cold_context(c).await?;
            let settled =
                c.f.http
                    .store
                    .load_http_context_intent_for_application(
                        &c.f.http.fixture.project,
                        &c.phase,
                        now(&c.f.http, &c.f.run, &mut clock).await?,
                    )
                    .await?
                    .ok_or("cold settlement missing")?;
            if settled.reply != ready {
                return Err("cold context differs".into());
            }
            if ready.intent().is_none() {
                return halt(&c.f.http, &c.f.run, "context", "null").await;
            }
            *current_stage = "context-apply";
            stage(root, current_stage)?;
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("current index missing")?;
            let (mut ledger, version) = c.f.stored.clone().into_parts();
            let at = now(&c.f.http, &c.f.run, &mut clock).await?;
            let applied = ApplyHttpCorrectionPlan::new(
                c.f.http.store.as_ref(),
                c.f.http.store.as_ref(),
                &WorkspaceAgentSourceReader,
            )
            .execute_context(
                &c.f.http.fixture.project,
                &c.f.http.goal,
                &mut c.f.run,
                &mut ledger,
                version,
                &current,
                &c.phase,
                at,
                &ActiveControl,
            )
            .await?;
            c.f.stored = StoredTaskLedger::new(ledger, applied.version);
            drop((current, settled, c.phase, c.origin, c.primary, c.context));
            edit::reopen(c.f).await?
        }
        HttpCorrectionPlanDecision::NeedEvidence => {
            return halt(&f.http, &f.run, "plan", "needEvidence").await;
        }
        HttpCorrectionPlanDecision::TooLarge => {
            return halt(&f.http, &f.run, "plan", "tooLarge").await;
        }
        HttpCorrectionPlanDecision::Refuse => return halt(&f.http, &f.run, "plan", "refuse").await,
    };
    *current_stage = "edit-read";
    stage(root, current_stage)?;
    let floor = now(&f.http, &f.run, &mut clock).await?;
    let (phase, mut compiled, mut page, _diagnostic) =
        edit::acquire_with_floor(&mut f, floor).await?;
    let before = inventory(&f.http.root)?;
    let original_source = fs::read(f.http.root.join("server.py"))?;
    let mut range = page.range();
    let mut correction = phase.anchor().step();
    *current_stage = "edit";
    stage(root, current_stage)?;
    let structural = compiled.for_structural_repair()?;
    let cut = compiled.for_truncated_output_repair()?;
    preflight(
        root,
        "edit",
        &provider,
        &f.http.goal,
        vec![
            ("initial", compiled.request().clone()),
            ("repair", structural.request().clone()),
            ("repair-truncated", cut.request().clone()),
        ],
    )?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("current index missing")?;
    let observed = live_provider(&provider, &f.http, run_id, root, "edit");
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let first = ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                edit::state(&f, &current)?,
                now(&f.http, &f.run, &mut clock).await?,
                &patch,
                &ActiveControl,
            ))
            .await?,
            edit::state(&f, &current)?,
            &compiled,
            now(&f.http, &f.run, &mut clock).await?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await;
    let repair = match &first {
        Err(EditPhaseExecutionFailure::OutputLimit) => Some(&cut),
        Err(EditPhaseExecutionFailure::Reply(
            EditPhaseFailure::InvalidReply
            | EditPhaseFailure::InvalidReplacement
            | EditPhaseFailure::TooLarge,
        )) => Some(&structural),
        _ => None,
    };
    let result = if let Some(repair) = repair {
        ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch)
            .execute(
                Box::pin(
                    crate::plan22_goal_contract_tests::prepare_indexed_bound_edit(
                        &f.http.compiler,
                        PreparedEditPhase::update(edit::state(&f, &current)?, &page)?,
                        repair,
                        edit::state(&f, &current)?,
                        now(&f.http, &f.run, &mut clock).await?,
                        &patch,
                        &ActiveControl,
                    ),
                )
                .await?,
                edit::state(&f, &current)?,
                repair,
                now(&f.http, &f.run, &mut clock).await?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await
    } else {
        first
    };
    traces(root, "edit", &observed)?;
    let edit_calls = observed.calls.load(Ordering::SeqCst);
    drop(observed);
    atomic_json(
        root,
        "edit-result.json",
        &json!({"calls":edit_calls,"request_digest":format!("{:?}",repair.unwrap_or(&compiled).request_digest()),"error":result.as_ref().err().map(|e|format!("{e:?}")),
        "state":state_report(&f.http,&f.run).await?,"prior_context_is_edit_delivery":false}),
    )?;
    let mut no_change_plan_calls = 0usize;
    let mut followup_edit_calls = 0usize;
    let mut b2_entered = false;
    let (report, current) = if matches!(
        &result,
        Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
    ) {
        // This is a negative closure, never a semantic retry. The Store must
        // prove a fresh69 NoChange record; generic rejected replies stay inert.
        drop(current);
        f = edit::reopen(f).await?;
        let current = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("NoChange current index missing")?;
        let (mut ledger, version) = f.stored.clone().into_parts();
        let closure_at = now(&f.http, &f.run, &mut clock).await?;
        let version = CloseEditNoChange::new(f.http.store.as_ref(), f.http.store.as_ref(), &patch)
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut f.run,
                &mut ledger,
                version,
                &current,
                &f.http.model,
                &page,
                closure_at,
                &ActiveControl,
            )
            .await?;
        f.stored = StoredTaskLedger::new(ledger, version);
        drop(current);
        let closed = edit::reopen(f).await?;
        now(&closed.http, &closed.run, &mut clock).await?;
        let failed = closed
            .stored
            .ledger()
            .step(correction)
            .ok_or("NoChange failed step missing")?;
        let source = fs::read(closed.http.root.join("server.py"))?;
        if closed.run.state() != AgentControllerState::Replan
            || failed.status() != TaskStepStatus::Failed
            || source != original_source
            || inventory(&closed.http.root)? != before
        {
            return Err("NoChange cold state or source differs".into());
        }
        atomic_json(
            root,
            "no-change-closure.json",
            &json!({"typed_current_no_change":true,
            "cold_failed_attempt":true,"cold_replan":true,"source_sha256":sha(&source),
            "new_model_calls":0,"patch_executed":false,"functional_component_passed":false,
            "state":state_report(&closed.http,&closed.run).await?}),
        )?;
        match owned_window_continuation(
            closed,
            &provider,
            root,
            &mut clock,
            current_stage,
            LiveWindowInput::ActualLocalModel,
        )
        .1
        .await?
        {
            LiveWindowResult::Halt(value) => return Ok(value),
            LiveWindowResult::Ready(ready) => {
                let LiveWindowReady {
                    fixture,
                    page: new_page,
                    current: new_current,
                    report,
                    correction: new_correction,
                    plan_calls,
                    edit_calls: new_calls,
                } = *ready;
                f = fixture;
                page = new_page;
                range = page.range();
                correction = new_correction;
                no_change_plan_calls = plan_calls;
                followup_edit_calls = new_calls;
                b2_entered = true;
                (report, new_current)
            }
        }
    } else {
        (result?, current)
    };

    let delivery = report.source_delivery;
    let action = match report.outcome {
        EditPhaseOutcome::Proposal(action) => action,
        EditPhaseOutcome::NeedEvidence(need) => {
            if delivery.is_some() {
                return Err("halt acquired false source credit".into());
            }
            return Box::pin(durable_edit_halt(
                f,
                repair.unwrap_or(&compiled),
                EditPhaseHalt::NeedEvidence(need),
                "edit",
                root,
            ))
            .await;
        }
        EditPhaseOutcome::TooLarge => {
            if delivery.is_some() {
                return Err("halt acquired false source credit".into());
            }
            return Box::pin(durable_edit_halt(
                f,
                repair.unwrap_or(&compiled),
                EditPhaseHalt::TooLarge,
                "edit",
                root,
            ))
            .await;
        }
    };
    let delivery = delivery.ok_or("actual Edit delivery missing")?;
    let patch_policy_fingerprint = match &action {
        AgentAction::ApplyPatch(patch) => patch
            .policy_action()
            .fingerprint()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        _ => return Err("actual exact patch action required".into()),
    };
    drop(current);
    f = edit::reopen(f).await?;
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold index missing")?;
    let reads = f
        .http
        .store
        .load_source_read_state(
            &f.http.fixture.project,
            edit::state(&f, &published)?.anchor(),
        )
        .await?;
    let [receipt] = reads.progress().receipts() else {
        return Err("one actual Edit delivery required".into());
    };
    if receipt.delivery() != Some(&delivery)
        || delivery.source().range() != range
        || reads.progress().pending().is_some()
        || reads.acquired().is_some()
    {
        return Err("cold Edit delivery differs".into());
    }
    let settings = f.http.settings;
    let goal = f.http.goal.clone();
    let final_profile = f.http.model.clone();
    let floor = now(&f.http, &f.run, &mut clock).await?;
    drop((published, reads, page));
    *current_stage = "patch";
    stage(root, current_stage)?;
    let fixture = Box::pin(apply_live_patch(f, action, floor)).await?;
    let source = fs::read(
        fixture
            .project
            .worktree()
            .root()
            .as_path()
            .join("server.py"),
    )?;
    if source.len() > 65536 || source == original_source {
        return Err("bounded actual changed script required".into());
    }
    let prefix = &original_source[..usize::try_from(range.start_byte())?];
    let suffix = &original_source[usize::try_from(range.end_byte())?..];
    let after = inventory(fixture.project.worktree().root().as_path())?;
    if !source.starts_with(prefix)
        || !source.ends_with(suffix)
        || before.len() != after.len()
        || !before
            .iter()
            .zip(&after)
            .all(|((p, b), (q, a))| p == q && (p == Path::new("server.py") || b == a))
    {
        return Err("protected bytes/inventory changed".into());
    }
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let run = store
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("patched run missing")?;
    let ledger = store
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("patched ledger missing")?;
    let original = store
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("original missing")?;
    if original.original() != goal.original() || run.state() != AgentControllerState::Verify {
        return Err("patched authority differs".into());
    }
    let phase_usage = store
        .load_edit_phase_usage(&fixture.project, run_id)
        .await?;
    if phase_usage.calls as usize
        != plan_calls + context_calls + edit_calls + no_change_plan_calls + followup_edit_calls
        || phase_usage.repairs > 2
        || phase_usage.calls
            > if b2_entered {
                if context_calls > 0 { 7 } else { 6 }
            } else if context_calls > 0 {
                5
            } else {
                4
            }
        || phase_usage.unknown_calls != 0
        || phase_usage.unobserved_calls != 0
    {
        return Err("cumulative phase accounting differs".into());
    }
    let review_state = json!({"run_bytes":run_id.as_bytes(),"snapshot":format!("{:?}",run.current_snapshot_id()),
        "ledger":ledger.ledger().revision().get(),"store_version":ledger.version().get(),"correction":format!("{correction:?}"),
        "original_sha256":sha(goal.original().ok_or("original missing")?.as_bytes()),"phase_calls":phase_usage.calls,"repairs":phase_usage.repairs,
        "b2_entered":b2_entered,"no_change_plan_calls":no_change_plan_calls,"followup_edit_calls":followup_edit_calls,
        "source_sha256":sha(&source),"source_blake3":blake3::hash(&source).to_hex().to_string(),
        "patch_policy_fingerprint":patch_policy_fingerprint,"outside_window_preserved":true});
    drop(store);
    // Keep the same owner/clock. No SQL transaction, process or lease is held here.
    *current_stage = "review";
    stage(root, current_stage)?;
    if !wait_review(
        root,
        &fixture,
        &run,
        &mut clock,
        binding,
        &source,
        &review_state,
    )
    .await?
    {
        return Ok(
            json!({"terminal":"regular_halt","halt_stage":"review","halt_kind":"denied",
            "functional_component_passed":false,"state":review_state}),
        );
    }
    // The existing recipe helper adds its actual elapsed time to this fresh global
    // floor and retains the original Run duration/budgets, including all review time.
    let not_before = fixture_now(&fixture, &run, &mut clock).await?;
    let original_created = run.created_at();
    let original_budget = run.budget();
    let reviewed = ContentHash::from_bytes(*blake3::hash(&source).as_bytes());
    drop((run, ledger, original));
    *current_stage = "http-proof";
    stage(root, current_stage)?;
    let (fixture, functional) = Box::pin(edit::functional::finish_http(
        fixture,
        run_id,
        final_profile.clone(),
        settings,
        not_before,
        reviewed,
    ))
    .await?;
    atomic_json(root, "http-functional-final.json", &functional)?;
    if functional["cold_done"] != true
        || functional["fresh_exact_grants"] != 12
        || functional["total_http_observations"] != 7
        || functional["owned_starts"] != 4
    {
        return Err("complete current HTTP verification required".into());
    }
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let done = store
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("cold Done missing")?;
    let original = store
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold original missing")?;
    let usage = store
        .load_edit_phase_usage(&fixture.project, run_id)
        .await?;
    let final_source = fs::read(
        fixture
            .project
            .worktree()
            .root()
            .as_path()
            .join("server.py"),
    )?;
    let final_inventory = inventory(fixture.project.worktree().root().as_path())?;
    if done.state() != AgentControllerState::Done
        || done.created_at() != original_created
        || done.budget() != original_budget
        || done.model_profile() != Some(final_profile.reference())
        || original.original() != goal.original()
        || final_source != source
        || final_inventory != after
        || usage != phase_usage
    {
        return Err("cold final authority/source/accounting differs".into());
    }
    // Include the final cold DB/file/inventory I/O in the original live owner.
    fixture_now(&fixture, &done, &mut clock).await?;
    Ok(
        json!({"terminal":"completed","functional_component_passed":true,"same_run":true,
        "plan_calls":plan_calls,"context_calls":context_calls,"edit_calls":edit_calls,
        "b2_entered":b2_entered,"no_change_plan_calls":no_change_plan_calls,"followup_edit_calls":followup_edit_calls,
        "candidate_schema":crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION,"http_no_change_replan_protocol":6,
        "shared_calls":usage.calls,"shared_repairs":usage.repairs,"shared_prompt_charged":usage.prompt_tokens,
        "shared_output_charged":usage.output_tokens,"unknown_calls":usage.unknown_calls,"unobserved_calls":usage.unobserved_calls,
        "run_bytes":run_id.as_bytes(),"created_at":done.created_at().unix_millis(),"original_budget_preserved":true,
        "owner_review_time_included":true,"source_sha256":sha(&source),"source_blake3":blake3::hash(&source).to_hex().to_string(),
        "original_sha256":sha(goal.original().ok_or("original missing")?.as_bytes()),"outside_window_preserved":true,
        "patch_policy_fingerprint":patch_policy_fingerprint,
        "cold_done":true,"actual_http_product_verification":functional,"model":tag,"context":16384,"output":1024,
        "original_model_profile":format!("{:?}",final_profile.reference()),"original_goal_preserved":true}),
    )
}

#[path = "plan22_http_same_run_stack_tests.rs"]
mod stack_tests;
