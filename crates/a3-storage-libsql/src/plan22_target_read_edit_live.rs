//! Explicit real local-model component smoke. Never GoalDone, A02 or P7 acceptance.
use super::*;
use a3_application::{LanguageParseControl, LanguageParseControlError};
use a3_domain::FileRevision;
use a3_domain::{GoalConstraint, PythonIntegerTranslation, SymbolName};
use a3_repo_index::PythonLanguageAdapter;
use futures::StreamExt;
use std::{io::Write, path::Path, time::Instant};

const SUBJECT: &str = "# untouched α\r\ndef target(value):\r\n    return value + 1\r\n";
const NEIGHBOR: &str = "def other(value):\r\n    return value + 1\r\n";
const REQUIREMENT: &str = "In subject.py, target(value) must return value + 2 for every integer, including negative and arbitrarily large integers.\n";
const PRESERVE: &str = "Keep neighbor.py and notes.txt byte-identical. Preserve CRLF, the Unicode comment and every byte outside the current target window.\n";
const NO_EXTRA: &str = "Do not add files, tests, dependencies or helpers.\n";

fn write_once(root: &Path, name: &str, value: Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn live_goal() -> TestResult<(GoalContract, TaskStepDefinition)> {
    let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(TaskId::from_bytes([1; 32])),
        [REQUIREMENT, PRESERVE, NO_EXTRA].concat().into_bytes(),
    )?;
    let main = original.range(0, REQUIREMENT.len())?;
    let preserve = original.range(REQUIREMENT.len(), REQUIREMENT.len() + PRESERVE.len())?;
    let no_extra = original.range(
        REQUIREMENT.len() + PRESERVE.len(),
        original.as_bytes().len(),
    )?;
    let criterion = criterion(2, REQUIREMENT)?.with_origin(CriterionOrigin::new(
        main,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    ));
    let criterion_id = criterion.id();
    let goal = GoalContract::initial(
        TaskId::from_bytes([1; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string(
                "Implement the current integer translation without changing protected files"
                    .to_owned(),
            )?,
            vec![criterion],
            vec![
                GoalConstraint::try_from_string(PRESERVE.to_owned())?.with_origin(preserve),
                GoalConstraint::try_from_string(NO_EXTRA.to_owned())?.with_origin(no_extra),
            ],
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string(
                "Current implementation diff plus a still-open product runtime proof".to_owned(),
            )?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    )
    .with_original(original)?;
    let step = TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Implement target's required integer translation".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Address the supplied requirement inside the actual current target window".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Current diff for only subject.py".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string(
                "Change only subject.py; this diff is not a runtime proof".to_owned(),
            )?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?,
        ),
    )?
    .with_criterion_coverage(vec![criterion_id], Vec::new())?;
    Ok((goal, step))
}

#[derive(Debug)]
struct GuardControl(Instant);
impl LanguageParseControl for GuardControl {
    fn is_cancelled(&self) -> bool {
        self.0.elapsed() >= Duration::from_secs(10)
    }
    fn report_progress(&self, _: Progress) -> Result<(), LanguageParseControlError> {
        if self.is_cancelled() {
            Err(LanguageParseControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn prove_complete_source(bytes: &[u8], revision: &FileRevision) -> TestResult {
    let source = std::str::from_utf8(bytes)?;
    if a3_domain::SecretCandidateClassifierV1::classify(source).is_some() {
        return Err("proposed source is not safe to retain or approve".into());
    }
    let required = PythonIntegerTranslation::new(
        RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
        SymbolName::try_from_string("target".to_owned())?,
        2,
    )?;
    let guard = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    if guard
        .prove_python_integer_translation(
            &required,
            revision,
            bytes,
            &GuardControl(Instant::now()),
        )?
        .is_none()
    {
        return Err("complete source does not prove the required integer translation".into());
    }
    Ok(())
}

struct ReplyTrace {
    hash: blake3::Hasher,
    bytes: u64,
    completion: Option<ModelProviderCompletion>,
    failure: Option<ModelProviderFailure>,
}
impl Default for ReplyTrace {
    fn default() -> Self {
        Self {
            hash: blake3::Hasher::new(),
            bytes: 0,
            completion: None,
            failure: None,
        }
    }
}
struct LiveProvider<'a> {
    inner: &'a OllamaModelProvider,
    fixture: &'a TargetFixture,
    root: &'a Path,
    calls: AtomicUsize,
    replies: Mutex<Vec<Arc<Mutex<ReplyTrace>>>>,
}
impl std::fmt::Debug for LiveProvider<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveTargetProvider")
            .field("calls", &self.calls.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}
impl ModelProvider for LiveProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(self.inner)
    }
    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async { Err(ModelProviderFailure::Rejected) })
    }
    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.inner.project_wire_request(request)
    }
    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .fixture
                .store
                .load_edit_phase_usage(&self.fixture.fixture.project, self.fixture.run.id())
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            let state = self
                .fixture
                .state()
                .map_err(|_| ModelProviderFailure::Rejected)?;
            let read = self
                .fixture
                .store
                .load_source_read_state(&self.fixture.fixture.project, state.anchor())
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1
                || usage.unknown_calls != 0
                || read.progress().pending().is_none()
                || read.acquired().is_some()
                || self.project_wire_request(request)? != projection
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call > 1 {
                return Err(ModelProviderFailure::Rejected);
            }
            write_once(self.root, &format!("edit-dispatch-{call}.json"), json!({
                "messages":request.messages().iter().map(|m|json!({"role":format!("{:?}",m.role()),"content":m.content()})).collect::<Vec<_>>(),
                "format":request.structured_output().ok_or(ModelProviderFailure::Rejected)?.value(),
                "wire_bytes":projection.request_bytes(),"wire_digest":format!("{:?}",projection.digest()),"context":projection.context_tokens(),"output":projection.output_tokens(),
                "thinking":format!("{:?}",projection.thinking()),"reserved_before_io":usage_json(usage)
            })).map_err(|_| ModelProviderFailure::Unavailable)?;
            let trace = Arc::new(Mutex::new(ReplyTrace::default()));
            self.replies
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?
                .push(trace.clone());
            let stream = match self
                .inner
                .stream_projected(request, projection, timeout, control)
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    if let Ok(mut r) = trace.lock() {
                        r.failure = Some(error);
                    }
                    return Err(error);
                }
            };
            Ok(Box::pin(stream.inspect(move |event| {
                if let Ok(mut r) = trace.lock() {
                    match event {
                        Ok(ProviderEvent::OutputText(text)) => {
                            r.hash.update(text.as_str().as_bytes());
                            r.bytes = r.bytes.saturating_add(text.as_str().len() as u64);
                        }
                        Ok(ProviderEvent::Completed(done)) => r.completion = Some(*done),
                        Err(error) => r.failure = Some(*error),
                    }
                }
            })) as ProviderEventStream<'a>)
        })
    }
}
fn usage_json(u: a3_application::EditPhaseReservedUsage) -> Value {
    json!({"calls":u.calls,"repairs":u.repairs,"prompt":u.prompt_tokens,"output":u.output_tokens,"unknown":u.unknown_calls,"unobserved":u.unobserved_calls})
}
fn outcome_json(
    result: &Result<a3_application::EditPhaseExecutionReport, EditPhaseExecutionFailure>,
) -> Value {
    json!({"outcome":result.as_ref().ok().map(|r|match &r.outcome {EditPhaseOutcome::Proposal(_)=>"proposal",EditPhaseOutcome::NeedEvidence(_)=>"needEvidence",EditPhaseOutcome::TooLarge=>"tooLarge"}),
        "error":result.as_ref().err().map(|e|format!("{e:?}")),"source_delivery":result.as_ref().is_ok_and(|r|r.source_delivery.is_some())})
}

async fn run(root: &Path, tag: &str) -> TestResult {
    fs::create_dir(root.join("worktree"))?;
    initialize_git(&root.join("worktree"))?;
    fs::write(root.join("worktree/neighbor.py"), NEIGHBOR)?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(root.join("worktree"))?,
    };
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile =
        super::super::p4_live_edit::probe_projected_profile(root, &provider, tag, 4096, 1024)
            .await?;
    let (goal, step) = live_goal()?;
    let f = TargetFixture::start_using(SUBJECT, profile, goal, step, fixture).await?;
    let before = inventory(&root.join("worktree"))?;
    let original_window = f.page.range();
    let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
    let mut compiled = f.compile(&phase, &f.acquired)?;
    let active = compiled
        .request()
        .messages()
        .iter()
        .map(|m| m.content())
        .collect::<Vec<_>>()
        .join("\n");
    for requirement in [
        "value + 2",
        "every integer",
        "neighbor.py",
        "notes.txt",
        "Do not add files, tests",
        "Preserve CRLF",
    ] {
        if !active.contains(requirement) {
            return Err("actual active Edit projection omitted a mandatory requirement".into());
        }
    }
    let mut wires = Vec::new();
    for (kind, request) in [
        ("initial", compiled.request().clone()),
        (
            "structural_repair",
            compiled.for_structural_repair()?.request().clone(),
        ),
        (
            "truncated_repair",
            compiled.for_truncated_output_repair()?.request().clone(),
        ),
    ] {
        let projection = provider.project_wire_request(&request)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())?;
        wires.push(json!({"kind":kind,"bytes":projection.request_bytes(),"digest":format!("{:?}",projection.digest()),"prompt_reserve":budget.prompt_tokens(),"output_reserve":budget.output_tokens()}));
    }
    write_once(
        root,
        "preflight.json",
        json!({"scope":"real local component smoke; not A02/P7/GoalDone","model":tag,"context":4096,"output":1024,"wires":wires,
        "original_bytes":f.goal.original().ok_or("original missing")?.as_bytes().len(),"original_digest":format!("{:?}",f.goal.original().ok_or("original missing")?.digest()),
        "source_bytes":f.page.text().len(),"source_hash":blake3::hash(f.page.text().as_bytes()).to_hex().to_string(),"source_range":[f.page.range().start_byte(),f.page.range().end_byte()],
        "active_math_and_all_constraints_present":true,"coverage":"Addresses only; RuntimeNeedsRecipe remains globally open"}),
    )?;
    let live = LiveProvider {
        inner: &provider,
        fixture: &f,
        root,
        calls: AtomicUsize::new(0),
        replies: Mutex::new(Vec::new()),
    };
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let clock = Instant::now();
    let first = ExecuteEditPhase::new(&live, f.store.as_ref(), &patch)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.compiler,
                phase,
                &mut compiled,
                f.state()?,
                at(22)?,
                &patch,
                &ActiveControl,
            ))
            .await?,
            f.state()?,
            &compiled,
            at(22)?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await;
    write_once(root, "edit-initial-result.json", outcome_json(&first))?;
    let repair = match &first {
        Err(EditPhaseExecutionFailure::OutputLimit) => {
            Some(compiled.for_truncated_output_repair()?)
        }
        Err(EditPhaseExecutionFailure::Reply(
            EditPhaseFailure::InvalidReply
            | EditPhaseFailure::InvalidReplacement
            | EditPhaseFailure::TooLarge,
        )) => Some(compiled.for_structural_repair()?),
        _ => None,
    };
    let result = if let Some(repair) = repair {
        ExecuteEditPhase::new(&live, f.store.as_ref(), &patch)
            .execute(
                Box::pin(
                    crate::plan22_goal_contract_tests::prepare_indexed_bound_edit(
                        &f.compiler,
                        PreparedEditPhase::update(f.state()?, &f.page)?,
                        &repair,
                        f.state()?,
                        at(23 + u64::try_from(clock.elapsed().as_millis())?)?,
                        &patch,
                        &ActiveControl,
                    ),
                )
                .await?,
                f.state()?,
                &repair,
                at(23 + u64::try_from(clock.elapsed().as_millis())?)?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await
    } else {
        first
    };
    let usage = f
        .store
        .load_edit_phase_usage(&f.fixture.project, f.run.id())
        .await?;
    write_once(
        root,
        "edit-result.json",
        json!({"result":outcome_json(&result),"usage":usage_json(usage),"elapsed_ms":clock.elapsed().as_millis(),"edit_dispatches":live.calls.load(Ordering::SeqCst),"semantic_retries":0}),
    )?;
    for (n, trace) in live
        .replies
        .lock()
        .map_err(|_| "reply trace lock")?
        .iter()
        .enumerate()
    {
        let r = trace.lock().map_err(|_| "reply trace lock")?;
        write_once(
            root,
            &format!("edit-reply-{n}.json"),
            json!({"text":null,"raw_text_retained":false,"bytes":r.bytes,"blake3":r.hash.finalize().to_hex().to_string(),
            "finish":r.completion.map(|c|format!("{:?}",c.reason())),"prompt":r.completion.and_then(|c|c.usage().prompt_tokens()),"output":r.completion.and_then(|c|c.usage().output_tokens()),"failure":r.failure.map(|e|format!("{e:?}"))}),
        )?;
    }
    drop(live);
    let report = result?;
    let delivery = report
        .source_delivery
        .ok_or("accepted model wire lacks target delivery")?;
    let EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("model returned a legal non-coding halt; no semantic retry".into());
    };
    let AgentAction::ApplyPatch(proposal) = &action else {
        return Err("model result escaped patch scope".into());
    };
    let [PatchOperation::Update(update)] = proposal.operations() else {
        return Err("model result escaped one Update".into());
    };
    let proposed_revision = FileRevision::new(
        update.expected().path().clone(),
        update.content().content_hash(),
    );
    let safe = prove_complete_source(update.content().as_bytes(), &proposed_revision);
    write_once(
        root,
        "preapproval.json",
        json!({"complete_proposed_subject_blake3":format!("{:?}",proposed_revision.content_hash()),"independent_translation_proved":safe.is_ok(),"guard_version":1,"code_executed":false}),
    )?;
    safe?;
    let f = f.reopen().await?;
    let history = f
        .store
        .load_source_read_state(&f.fixture.project, f.state()?.anchor())
        .await?;
    let [receipt] = history.progress().receipts() else {
        return Err("cold real source delivery missing".into());
    };
    if receipt.delivery() != Some(&delivery) {
        return Err("cold wire delivery differs".into());
    }
    let proof = apply_target_patch(f, action).await?;
    let after = inventory(&root.join("worktree"))?;
    let protected = before.len() == after.len()
        && before
            .iter()
            .zip(&after)
            .all(|((p, b), (q, a))| p == q && (p == Path::new("subject.py") || b == a));
    let current = fs::read(root.join("worktree/subject.py"))?;
    let prefix_end = usize::try_from(original_window.start_byte())?;
    let suffix_start = usize::try_from(original_window.end_byte())?;
    let prefix = SUBJECT
        .as_bytes()
        .get(..prefix_end)
        .ok_or("original window prefix invalid")?;
    let suffix = SUBJECT
        .as_bytes()
        .get(suffix_start..)
        .ok_or("original window suffix invalid")?;
    let outside_window = current.starts_with(prefix)
        && current.ends_with(suffix)
        && current.len() >= prefix.len() + suffix.len();
    write_once(
        root,
        "protected-scope.json",
        json!({"protected_files_unchanged":protected,"outside_window_preserved":outside_window,
        "window":[original_window.start_byte(),original_window.end_byte()],"prefix_bytes":prefix.len(),"suffix_bytes":suffix.len()}),
    )?;
    if !protected || !outside_window {
        return Err("protected source inventory or outside-window bytes changed".into());
    }
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&proof.fixture.layout).await?;
    let published = cold
        .latest_published_index(&proof.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold publication missing")?;
    let current_revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path().as_bytes() == b"subject.py")
        .ok_or("published subject missing")?;
    prove_complete_source(&current, current_revision)?;
    let usage = cold
        .load_edit_phase_usage(&proof.fixture.project, proof.run_id)
        .await?;
    let cold_goal = cold
        .load_current_goal_contract(&proof.fixture.project, proof.goal.task_id())
        .await?
        .ok_or("cold current goal missing")?;
    let cold_ledger = cold
        .load_task_ledger(&proof.fixture.project, proof.goal.task_id())
        .await?
        .ok_or("cold current ledger missing")?;
    let cold_run = cold
        .load_agent_run(&proof.fixture.project, proof.run_id)
        .await?
        .ok_or("cold current run missing")?;
    let [math] = cold_goal.draft().acceptance_criteria() else {
        return Err("current math criterion not unique".into());
    };
    let goal_preserved = cold_goal == proof.goal;
    let step = cold_ledger
        .ledger()
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("current Addresses step missing")?;
    let local_completed = step.is_active_plan_step()
        && step.status() == TaskStepStatus::Completed
        && step.definition().addressed_criteria() == std::slice::from_ref(&math.id())
        && step.definition().acceptance_criteria().is_empty();
    let global_open = goal_preserved
        && math.statement().as_str() == REQUIREMENT.trim()
        && math.origin().is_some_and(|origin| {
            matches!(
                origin.evidence_requirement(),
                CriterionEvidenceRequirement::RuntimeNeedsRecipe
            )
        })
        && cold_ledger.ledger().goal_contract() == cold_goal.reference()
        && cold_run.goal_contract() == cold_goal.reference()
        && cold_run.task_ledger_revision() == cold_ledger.ledger().revision()
        && cold_run.state() == AgentControllerState::Verify
        && local_completed
        && !cold_ledger
            .ledger()
            .steps()
            .filter(|step| step.is_active_plan_step())
            .any(|step| step.definition().acceptance_criteria().contains(&math.id()));
    if !global_open {
        return Err(
            "cold original runtime criterion was not retained open after local Addresses proof"
                .into(),
        );
    }
    write_once(
        root,
        "applied-final.json",
        json!({"scope":"real local target-read/Edit/Patch component only; not A02/P7/GoalDone","model":tag,
        "product_verification":"current Diff/Addresses; no product runtime proof","run_state":format!("{:?}",cold_run.state()),"goal_done":cold_run.state()==AgentControllerState::Done,"global_runtime_requirement_open":global_open,
        "goal_original_and_contract_preserved":goal_preserved,"current_completed_addresses_without_proves":local_completed,"outside_window_preserved":outside_window,
        "independent_complete_current_translation_verified":true,"guard_version":1,"protected_files_unchanged":protected,"cold_diff_verified":true,
        "source_before_blake3":blake3::hash(SUBJECT.as_bytes()).to_hex().to_string(),"source_after_blake3":blake3::hash(&current).to_hex().to_string(),
        "evidence":format!("{:?}",proof.evidence_id),"snapshot":format!("{:?}",proof.snapshot_id),"usage":usage_json(usage),"semantic_retries":0,"actual_code_executed":false}),
    )?;
    drop(cold);
    Ok(())
}

#[test]
#[ignore = "explicit real local-model target bridge smoke; requires a fresh A3_TARGET_READ_OUTPUT"]
fn target_read_local_ollama_edits_and_applies_current_window() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = PathBuf::from(
            std::env::var_os("A3_TARGET_READ_OUTPUT").ok_or("explicit output path required")?,
        );
        let tag = std::env::var("A3_TARGET_READ_MODEL")?;
        if !matches!(tag.as_str(), "qwen38-4k:latest" | "ornith-1.5:9b") {
            return Err("only the two registered local tags are allowed".into());
        }
        let allowed = fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plan22-p7-runs"),
        )?;
        if !root.is_absolute()
            || root.file_name().is_none()
            || fs::canonicalize(root.parent().ok_or("output parent missing")?)? != allowed
            || root.try_exists()?
        {
            return Err("smoke output must be a new direct child of target/plan22-p7-runs".into());
        }
        fs::create_dir(&root)?;
        write_once(
            &root,
            "native-start.json",
            json!({"schema":"a3.plan22.target-read-live.v1","model":tag,"scope":"real local component smoke","max_structural_repairs":1,"semantic_retries":0,"goal_done":false}),
        )?;
        let result = with_live_runtime(async { run(&root, &tag).await });
        write_once(
            &root,
            "native-final.json",
            json!({"schema":"a3.plan22.target-read-live.v1","component_passed":result.is_ok(),"goal_done":false,"a02_completed":false,"p7_completed":false,
            "failure":result.as_ref().err().map(|_|"target_component_failed; inspect stage artifacts"),"model":tag}),
        )?;
        result
    })
}

fn with_live_runtime(future: impl std::future::Future<Output = TestResult>) -> TestResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future)
}

#[test]
fn target_read_live_template_has_complete_goal_and_three_four_k_wires_without_inference()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_live_runtime(async {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
            assert!(listener.local_addr()?.ip().is_loopback());
            drop(listener);
            let fixture = Fixture::new()?;
            fs::write(fixture.root.join("worktree/neighbor.py"), NEIGHBOR)?;
            let (goal, step) = live_goal()?;
            let f =
                TargetFixture::start_using(SUBJECT, target_profile()?, goal, step, fixture).await?;
            let original = f.goal.original().ok_or("full original missing")?;
            assert_eq!(
                original.as_bytes(),
                [REQUIREMENT, PRESERVE, NO_EXTRA].concat().as_bytes()
            );
            assert!(a3_domain::GoalOriginalCoverage::derive_from(&f.goal)?.is_complete());
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let compiled = f.compile(&phase, &f.acquired)?;
            let active = compiled.request().messages()[1].content();
            assert!(active.contains(&format!("Address must: {}\n", REQUIREMENT.trim())));
            for constraint in [PRESERVE.trim(), NO_EXTRA.trim()] {
                assert!(active.contains(&format!("Rule: {constraint}\n")));
            }
            assert!(active.contains("Coverage total=1 outside=0 no-proof-step=1.\n"));
            assert_eq!(
                f.page.range().start_byte() as usize,
                "# untouched α\r\n".len()
            );
            assert_eq!(f.page.range().end_byte() as usize, SUBJECT.len());
            assert_eq!(
                f.page.text(),
                "def target(value):\r\n    return value + 1\r\n"
            );
            let provider = OllamaModelProvider::for_projected_wire(
                OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                Arc::new(LocalOnlyOllamaEndpointPolicy),
            )?;
            let expected = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none().with_required_source(
                    EditEvidenceNeed::Definition,
                    &f.bound,
                    Some((&f.page, &f.acquired)),
                ),
            )?;
            assert_eq!(
                provider.project_wire_request(expected.request())?,
                provider.project_wire_request(compiled.request())?
            );
            for (kind, request) in [
                ("initial", compiled.request().clone()),
                (
                    "structural_repair",
                    compiled.for_structural_repair()?.request().clone(),
                ),
                (
                    "truncated_repair",
                    compiled.for_truncated_output_repair()?.request().clone(),
                ),
            ] {
                let wire = provider.project_wire_request(&request)?;
                PhaseWireBudget::fit(wire, request.profile())?;
                println!(
                    "target live template {kind}: {} bytes; no provider I/O",
                    wire.request_bytes()
                );
            }
            assert_eq!(
                f.store
                    .load_edit_phase_usage(&f.fixture.project, f.run.id())
                    .await?
                    .calls,
                0
            );
            f.cleanup()
        })
    })
}
