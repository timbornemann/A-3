//! Explicit local P4 measurement with one initial call and at most one structural repair.

#[path = "plan22_p4_live_intake.rs"]
mod live_intake;

#[path = "plan22_p4_live_edit_branches.rs"]
mod live_edit_branches;

use super::*;
use a3_application::{
    AcceptanceRejection, AcceptanceVerifier, AcceptanceVerifierOutcome, AcceptanceVerifierTimeout,
    CompiledEditPhaseRequest, EditPhaseCallKind, EditPhaseExecutionFailure,
    EditPhaseExecutionReport, EditPhasePrerequisites, EditPhaseReservationStore,
    EditPhaseReservedUsage, ExecuteEditPhase, ModelCancellationFuture, ModelCapabilityProbeRequest,
    ModelFinishReason, ModelOperationControl, ModelProvider, ModelProviderFailure,
    ModelProviderFuture, ModelProviderRequest, ModelRequestTimeout, ModelWireProjection,
    ModelWireThinking, PhaseWireBudget, ProbeModelProfile, ProviderEvent, ProviderEventStream,
    build_edit_phase_request,
};
use a3_domain::GoalOriginalCoverage;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use futures::StreamExt;
use std::io::{Read, Write};

const LIVE_TAG: &str = "qwen38-4k:latest";
const ORIGINAL_BYTES: usize = 34_371;
const TARGET: &str = "step_one.py";
const LATE: &str = "Use exact int arithmetic for signed and huge values. No floats, rounding, clipping, bool or string results, tables, or constant answers.";
const CONSTRAINTS: [&str; 6] = [
    "Deliver step_one, step_two, step_three, step_four separately in order. Run each existing module check before starting the next.",
    "Keep pyproject.toml, pipeline.py and all existing tests byte-identical. Preserve the existing pipeline wiring.",
    "Change only the four compute implementations. Add no files, tests, dependencies, or helpers. Report unavailable checks.",
    LATE,
    "Preserve USER_NOTES.md byte-identically, including Unicode and line endings, after restart.",
    "After the supervisor restart, resume the same goal and open obligations without repeating applied changes. Preserve interface decisions and restrictions. Only use declared check commands.",
];

#[test]
#[ignore = "explicit local P4 measurement; requires a newly prepared A3_PLAN22_P4_RUN_DIR"]
fn live_ollama_4k_a10_first_edit_window() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = explicit_run_directory()?;
        write_report(
            &root,
            "native-start.json",
            json!({"version":1,"case":"P4-A10-first-edit","tag":LIVE_TAG}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run(&root));
        write_report(
            &root,
            "native-final.json",
            json!({"version":1,"success":result.is_ok(),"full_a10_done":false}),
        )?;
        result
    })
}

fn explicit_run_directory() -> TestResult<PathBuf> {
    let root = PathBuf::from(
        std::env::var_os("A3_PLAN22_P4_RUN_DIR").ok_or("explicit P4 run directory required")?,
    );
    if !root.is_absolute() || fs::symlink_metadata(&root)?.file_type().is_symlink() {
        return Err("P4 run directory must be a canonical owned directory".into());
    }
    let root = fs::canonicalize(root)?;
    if !root.join("p4-input.json").is_file()
        || !root.join("worktree").is_dir()
        || fs::symlink_metadata(root.join("worktree"))?
            .file_type()
            .is_symlink()
        || root.join("worktree/.git").try_exists()?
        || root.join("app-data").try_exists()?
    {
        return Err("P4 fixture is missing or already used".into());
    }
    Ok(root)
}

async fn run(root: &Path) -> TestResult {
    let original_path = root.join("original.utf8");
    if fs::metadata(&original_path)?.len() != ORIGINAL_BYTES as u64 {
        return Err("frozen A10 original size differs".into());
    }
    let mut original = Vec::with_capacity(ORIGINAL_BYTES);
    fs::File::open(&original_path)?
        .take((ORIGINAL_BYTES + 1) as u64)
        .read_to_end(&mut original)?;
    if original.len() != ORIGINAL_BYTES {
        return Err("frozen A10 original changed while reading".into());
    }
    let worktree = root.join("worktree");
    initialize_git(&worktree)?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let before = inventory(&worktree)?;
    let (goal, evidence, snapshot, settled) = edit_first_window(&fixture, original).await?;
    let after = inventory(&worktree)?;
    assert_eq!(before.len(), after.len());
    let mut changed = 0;
    for ((before_path, before_bytes), (after_path, after_bytes)) in before.iter().zip(&after) {
        assert_eq!(before_path, after_path);
        if before_path == Path::new(TARGET) {
            assert_ne!(before_bytes, after_bytes);
            changed += 1;
        } else {
            assert!(
                before_bytes == after_bytes,
                "a protected fixture file changed"
            );
        }
    }
    assert_eq!(changed, 1);
    verify_reopened_partial_goal(&fixture, &goal, evidence, snapshot, settled).await
}

async fn edit_first_window(
    fixture: &Fixture,
    original: Vec<u8>,
) -> TestResult<(
    GoalContract,
    TaskEvidenceId,
    SnapshotId,
    EditPhaseReservedUsage,
)> {
    let path = RepositoryPath::try_from_bytes(TARGET.as_bytes().to_vec())?;
    let (goal, step) = original_goal(original, &path)?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("frozen step_one revision not indexed")?;
    let inspection = AgentFileInspection::new(
        path.clone(),
        AgentFileStartLine::new(1)?,
        AgentFileLineCount::new(2)?,
    );
    let page = WorkspaceAgentSourceReader
        .read_page(&fixture.project, revision, &inspection, &ActiveControl)
        .await?;
    assert_eq!(
        page.text(),
        "def compute(value: int) -> int:\n    return value + 2\n"
    );
    assert!(!page.truncated());

    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_profile(&fixture.root, &provider).await?;
    let (mut run, stored, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, base, vec![step]).await?;
    assert_eq!(unplanned.len(), 24);
    let state = EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published)?;
    let mut phase = PreparedEditPhase::update(state, &page)?;
    let memory =
        RunMemoryCheckpoint::compile(&goal, stored.ledger(), &run, &published, Vec::new())?;
    let mut compiled = build_edit_phase_request(
        &goal,
        &stored,
        &phase,
        state,
        profile.clone(),
        Some(&memory),
        EditPhasePrerequisites::none(),
    )?;
    let messages = compiled.request().messages();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].content(), phase.input_json());
    let view = messages[1].content();
    for constraint in CONSTRAINTS {
        assert!(view.contains(&format!("Rule: {constraint}\n")));
    }
    assert_eq!(
        view.lines()
            .filter(|line| line.starts_with("Address "))
            .count(),
        6
    );
    assert!(!view.lines().any(|line| line.starts_with("Prove ")));
    assert!(view.contains("Coverage total=24 outside=18 no-proof-step=24.\n"));
    assert!(!messages[1].content().contains("Erwartete Beispiele"));

    let facts = EditIntentFacts {
        original_blake3: blake3::hash(goal.original().ok_or("original missing")?.as_bytes())
            .to_hex()
            .to_string(),
        source_bytes: page.text().len(),
    };
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let started = Instant::now();
    let (report, usage) = loop {
        let observed = ObservedEditProvider {
            inner: &provider,
            fixture,
            store: &store,
            run_id: run.id(),
            kind: compiled.kind(),
            stats: Mutex::new(ObservedEditStats::default()),
        };
        let now = u64::try_from(started.elapsed().as_millis())?
            .checked_add(10)
            .ok_or("live clock overflow")?;
        let measured = observed
            .measure(phase, state, &mut compiled, &patch, at(now)?, &facts)
            .await?;
        match measured.result {
            Ok(report) => break (report, measured.usage),
            Err(error)
                if compiled.kind() == EditPhaseCallKind::Initial
                    && permits_structural_repair(error, measured.finish) =>
            {
                assert_eq!(measured.usage.calls, 1);
                assert_eq!(measured.usage.repairs, 0);
                assert_eq!(measured.usage.unobserved_calls, 0);
                assert_eq!(measured.usage.unknown_calls, 0);
                let reread = WorkspaceAgentSourceReader
                    .read_page(&fixture.project, revision, &inspection, &ActiveControl)
                    .await?;
                if reread != page {
                    return Err("source page changed before structural repair".into());
                }
                phase = PreparedEditPhase::update(state, &reread)?;
                assert_eq!(phase.anchor(), compiled.anchor());
                assert_eq!(phase.job_digest(), compiled.job_digest());
                let repair = compiled.for_structural_repair()?;
                assert_eq!(repair.kind(), EditPhaseCallKind::StructuralRepair);
                assert_eq!(repair.anchor(), compiled.anchor());
                assert_eq!(repair.job_digest(), compiled.job_digest());
                assert_ne!(repair.request_digest(), compiled.request_digest());
                assert_eq!(
                    &repair.request().messages()[1..],
                    &compiled.request().messages()[1..]
                );
                compiled = repair;
            }
            Err(error) => return Err(error.into()),
        }
    };
    let elapsed = u64::try_from(started.elapsed().as_millis())?;
    let a3_application::EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("positive P4 component requires a real edit proposal".into());
    };
    let AgentAction::ApplyPatch(compiled_patch) = &action else {
        return Err("candidate returned no patch action".into());
    };
    let [PatchOperation::Update(update)] = compiled_patch.operations() else {
        return Err("candidate returned effects outside the one window".into());
    };
    assert_eq!(update.expected().path(), &path);

    let (mut ledger, mut version) = stored.into_parts();
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let runner = native_runner()?;
    let factory = BuiltinProcessVerificationEvidenceFactory;
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
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let policy = WorkspacePolicy::unrestricted();
    let before_grant = 14_u64.checked_add(elapsed).ok_or("live clock overflow")?;
    let expires = at(before_grant
        .checked_add(1_000)
        .ok_or("approval clock overflow")?)?;
    let outcome = controller
        .execute(
            &fixture.project,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            action.clone(),
            None,
            &policy,
            None,
            execution_ids(60),
            at(before_grant)?,
            expires,
            &seed,
            &mut compiler,
            &events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request) = outcome else {
        return Err("live edit must await actual exact approval".into());
    };
    let pending = approvals
        .presentation(&fixture.project, goal.task_id())?
        .ok_or("live edit approval presentation missing")?;
    assert_eq!(pending.request_id(), request);
    assert_eq!(
        pending.action_fingerprint(),
        compiled_patch.policy_action().fingerprint()
    );
    let inspection_view = inspection
        .overview(&fixture.project, goal.task_id())?
        .ok_or("live edit inspection missing")?;
    let preview = inspection_view.patch().ok_or("live edit preview missing")?;
    assert_eq!(preview.context(), pending.context());
    let [file] = preview.files() else {
        return Err("live edit preview must contain one file".into());
    };
    assert_eq!(file.target_path(), Some(&path));
    assert!(!file.hunks().is_empty());
    assert!(
        store
            .load_agent_mutation_attempts(&fixture.project, run.id())
            .await?
            .is_empty()
    );
    assert!(fs::read(fixture.root.join("worktree").join(TARGET))? == page.text().as_bytes());
    let mut grant = GrantPolicyApproval::new(store.as_ref())
        .execute(
            &fixture.project,
            &mut run,
            request,
            ApprovalId::from_bytes([71; 32]),
            RunEventId::from_bytes([71; 32]),
            base,
            at(before_grant + 1)?,
        )
        .await?;
    let outcome = controller
        .execute(
            &fixture.project,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            action,
            None,
            &policy,
            Some(&mut grant),
            execution_ids(72),
            at(before_grant + 2)?,
            expires,
            &seed,
            &mut compiler,
            &events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err("actual scoped edit was not verified".into());
    };
    assert_ne!(snapshot_id, base);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    Ok((goal, evidence_id, snapshot_id, usage))
}

async fn probe_profile(root: &Path, provider: &OllamaModelProvider) -> TestResult<ModelProfile> {
    probe_profile_for_tag(root, provider, LIVE_TAG).await
}

async fn probe_profile_for_tag(
    root: &Path,
    provider: &OllamaModelProvider,
    tag: &str,
) -> TestResult<ModelProfile> {
    probe_projected_profile(root, provider, tag, 4_096, 1_024).await
}

pub(in super::super) async fn probe_projected_profile(
    root: &Path,
    provider: &OllamaModelProvider,
    tag: &str,
    context_tokens: u32,
    output_tokens: u32,
) -> TestResult<ModelProfile> {
    let settings = ModelProfileSettings::new(
        ModelContextLimit::new(context_tokens)?,
        ModelOutputLimit::new(output_tokens)?,
        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
        ModelParallelismLimit::new(1)?,
        ModelSamplingProfile::new(
            ModelTemperature::from_milli(0)?,
            ModelTopP::from_milli(1_000)?,
        ),
        ModelStopSequences::empty(),
        ModelPromptSchemaGrounding::FormatFieldOnly,
    )?;
    let request =
        ModelCapabilityProbeRequest::new(ModelId::try_from_string(tag.to_owned())?, settings);
    write_report(
        root,
        "probe-start.json",
        json!({"kind":"capability_probe","tag":tag}),
    )?;
    let started = Instant::now();
    let result = ProbeModelProfile::new(provider)
        .execute(
            &request,
            ModelRequestTimeout::from_millis(120_000)?,
            &ActiveControl,
        )
        .await;
    write_report(
        root,
        "probe-result.json",
        json!({
            "invocations":1,
            "elapsed_millis":started.elapsed().as_millis(),
            "verified":result.as_ref().is_ok_and(|profile| profile.executable_actions_enabled()),
            "failure":result.as_ref().err().map(|error| format!("{error:?}")),
            "provider_prompt_tokens":null,
            "provider_output_tokens":null,
        }),
    )?;
    let profile = result?;
    if !profile.executable_actions_enabled() {
        return Err("local structured output capability was not verified".into());
    }
    Ok(profile)
}

impl ModelOperationControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}

struct EditIntentFacts {
    original_blake3: String,
    source_bytes: usize,
}

struct MeasuredEdit {
    result: Result<EditPhaseExecutionReport, EditPhaseExecutionFailure>,
    usage: EditPhaseReservedUsage,
    finish: Option<ModelFinishReason>,
}

fn permits_structural_repair(
    error: EditPhaseExecutionFailure,
    finish: Option<ModelFinishReason>,
) -> bool {
    match error {
        EditPhaseExecutionFailure::Reply(
            EditPhaseFailure::InvalidReply
            | EditPhaseFailure::InvalidReplacement
            | EditPhaseFailure::TooLarge,
        ) => true,
        EditPhaseExecutionFailure::OutputLimit => finish == Some(ModelFinishReason::OutputLimit),
        _ => false,
    }
}

fn attempt_reports(kind: EditPhaseCallKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        EditPhaseCallKind::Initial => {
            ("edit-intent.json", "edit-dispatch.json", "edit-result.json")
        }
        EditPhaseCallKind::StructuralRepair => (
            "edit-repair-intent.json",
            "edit-repair-dispatch.json",
            "edit-repair-result.json",
        ),
    }
}

fn expected_usage(kind: EditPhaseCallKind) -> (u32, u32) {
    match kind {
        EditPhaseCallKind::Initial => (1, 0),
        EditPhaseCallKind::StructuralRepair => (2, 1),
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ObservedEditStats {
    dispatches: u8,
    output_bytes: usize,
    first_output_millis: Option<u128>,
    finish: Option<a3_application::ModelProviderCompletion>,
    failure: Option<ModelProviderFailure>,
}

struct ObservedEditProvider<'a> {
    inner: &'a OllamaModelProvider,
    fixture: &'a Fixture,
    store: &'a LibsqlKnowledgeStore,
    run_id: AgentRunId,
    kind: EditPhaseCallKind,
    stats: Mutex<ObservedEditStats>,
}

impl std::fmt::Debug for ObservedEditProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ObservedEditProvider")
    }
}

impl ObservedEditProvider<'_> {
    async fn measure(
        &self,
        phase: PreparedEditPhase,
        state: EditPhaseState<'_>,
        compiled: &mut CompiledEditPhaseRequest,
        patch: &WorkspacePatchAdapter,
        at: AgentRunTimestamp,
        facts: &EditIntentFacts,
    ) -> TestResult<MeasuredEdit> {
        let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
            phase,
            compiled,
            state,
            at,
            patch,
            &ActiveControl,
        ))
        .await?;
        let (intent, _, result) = attempt_reports(self.kind);
        let profile = compiled.request().profile();
        let projection = self.project_wire_request(compiled.request())?;
        let fit = PhaseWireBudget::fit(projection, profile);
        assert_eq!(projection.context_tokens(), 4_096);
        assert_eq!(projection.output_tokens(), 1_024);
        assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
        write_report(
            &self.fixture.root,
            intent,
            json!({
                "job_blake3":hash_hex(compiled.job_digest()),
                "request_projection_blake3":hash_hex(compiled.request_digest()),
                "wire":wire_json(projection),
                "charged_prompt_tokens":fit.as_ref().ok().map(|budget| budget.prompt_tokens()),
                "charged_output_tokens":fit.as_ref().ok().map(|budget| budget.output_tokens()),
                "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
                "profile":profile.id().to_string(),
                "original_blake3":facts.original_blake3,
                "original_bytes":ORIGINAL_BYTES,
                "original_associations":30,
                "active_criteria":6,
                "source_bytes":facts.source_bytes,
                "late_constraint_included":true,
            }),
        )?;

        let budget = fit?;
        assert_eq!(budget.output_tokens(), 1_024);
        assert!(budget.prompt_tokens() + budget.output_tokens() <= 4_096);
        let started = Instant::now();
        let execution = ExecuteEditPhase::new(self, self.store, patch)
            .execute(
                phase,
                state,
                compiled,
                at,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        let elapsed = started.elapsed().as_millis();
        let usage = self
            .store
            .load_edit_phase_usage(&self.fixture.project, self.run_id)
            .await;
        let stats = *self.stats.lock().map_err(|_| "live observer poisoned")?;
        write_report(
            &self.fixture.root,
            result,
            json!({
                "elapsed_millis":elapsed,
                "execution":match &execution {
                    Ok(report) => match &report.outcome {
                        EditPhaseOutcome::Proposal(_) => "proposal",
                        EditPhaseOutcome::NeedEvidence(_) => "need_evidence",
                        EditPhaseOutcome::TooLarge => "too_large",
                    },
                    Err(_) => "failed",
                },
                "failure":execution.as_ref().err().map(|error| format!("{error:?}")),
                "stream":{
                    "dispatches":stats.dispatches,
                    "output_bytes":stats.output_bytes,
                    "first_output_millis":stats.first_output_millis,
                    "finish":stats.finish.map(|end| format!("{:?}",end.reason())),
                    "provider_prompt_tokens":stats.finish.and_then(|end| end.usage().prompt_tokens()),
                    "provider_output_tokens":stats.finish.and_then(|end| end.usage().output_tokens()),
                    "transport_failure":stats.failure.map(|error| format!("{error:?}")),
                },
                "durable_usage":usage.as_ref().ok().map(usage_json),
            }),
        )?;
        let usage = usage?;
        if let Ok(report) = &execution {
            assert_eq!(report.projection, projection);
            assert_eq!(report.budget, budget);
            assert_eq!((usage.calls, usage.repairs), expected_usage(self.kind));
            assert_eq!(usage.unobserved_calls, 0);
            assert_eq!(usage.unknown_calls, 0);
            assert!(
                usage.prompt_tokens
                    >= report.usage.prompt_tokens().ok_or("prompt usage missing")?
            );
            assert!(
                usage.output_tokens
                    >= report.usage.output_tokens().ok_or("output usage missing")?
            );
        }
        Ok(MeasuredEdit {
            result: execution,
            usage,
            finish: stats.finish.map(|end| end.reason()),
        })
    }
}

impl ModelProvider for ObservedEditProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(self.inner)
    }

    fn stream<'a>(
        &'a self,
        _request: &'a ModelProviderRequest,
        _timeout: ModelRequestTimeout,
        _control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
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
            let reservation = self
                .store
                .load_edit_phase_usage(&self.fixture.project, self.run_id)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if (reservation.calls, reservation.repairs) != expected_usage(self.kind)
                || reservation.unobserved_calls != 1
                || reservation.unknown_calls != 0
            {
                return Err(ModelProviderFailure::Rejected);
            }
            {
                let mut stats = self
                    .stats
                    .lock()
                    .map_err(|_| ModelProviderFailure::Unavailable)?;
                if stats.dispatches != 0 {
                    return Err(ModelProviderFailure::Rejected);
                }
                stats.dispatches = 1;
            }
            let (_, dispatch, _) = attempt_reports(self.kind);
            write_report(
                &self.fixture.root,
                dispatch,
                json!({"wire":wire_json(projection),"before_io":usage_json(&reservation)}),
            )
            .map_err(|_| ModelProviderFailure::Unavailable)?;
            let started = Instant::now();
            let stream = match self
                .inner
                .stream_projected(request, projection, timeout, control)
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    if let Ok(mut stats) = self.stats.lock() {
                        stats.failure = Some(error);
                    }
                    return Err(error);
                }
            };
            Ok(Box::pin(stream.inspect(move |event| {
                if let Ok(mut stats) = self.stats.lock() {
                    match event {
                        Ok(ProviderEvent::OutputText(chunk)) => {
                            stats
                                .first_output_millis
                                .get_or_insert_with(|| started.elapsed().as_millis());
                            stats.output_bytes =
                                stats.output_bytes.saturating_add(chunk.as_str().len());
                        }
                        Ok(ProviderEvent::Completed(end)) => stats.finish = Some(*end),
                        Err(error) => stats.failure = Some(*error),
                    }
                }
            })) as ProviderEventStream<'a>)
        })
    }
}

fn write_report(root: &Path, name: &str, value: Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn usage_json(usage: &EditPhaseReservedUsage) -> Value {
    json!({
        "calls":usage.calls,"repairs":usage.repairs,"prompt_tokens":usage.prompt_tokens,
        "output_tokens":usage.output_tokens,"unobserved_calls":usage.unobserved_calls,
        "unknown_calls":usage.unknown_calls,
    })
}

fn wire_json(wire: ModelWireProjection) -> Value {
    json!({
        "body_blake3":hash_hex(wire.digest()),
        "body_bytes":wire.request_bytes(),"message_bytes":wire.message_bytes(),
        "format_bytes":wire.format_bytes(),"num_ctx":wire.context_tokens(),
        "num_predict":wire.output_tokens(),"think":wire.thinking() == ModelWireThinking::Enabled,
    })
}

fn hash_hex(hash: ContentHash) -> String {
    blake3::Hash::from_bytes(*hash.as_bytes())
        .to_hex()
        .to_string()
}

fn original_goal(
    bytes: Vec<u8>,
    path: &RepositoryPath,
) -> TestResult<(GoalContract, TaskStepDefinition)> {
    let task = TaskId::from_bytes([1; 32]);
    let original =
        GoalOriginalRequest::try_from_bytes(GoalContractReference::initial(task), bytes)?;
    let mut starts = Vec::with_capacity(31);
    for number in 1..=30 {
        let marker = format!("\nR{number:02}: ");
        let mut found = original.as_str().match_indices(&marker);
        let (position, _) = found.next().ok_or("original requirement missing")?;
        if found.next().is_some() {
            return Err("original requirement is ambiguous".into());
        }
        starts.push(position + 1);
    }
    if starts.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("frozen requirement order differs".into());
    }
    starts[0] = 0; // Include the immutable shared preamble in the first origin.
    starts.push(original.as_bytes().len()); // Include the declared command scope in R30.
    assert_eq!(starts[27], 32_912);
    let ranges = starts
        .windows(2)
        .map(|pair| original.range(pair[0], pair[1]))
        .collect::<Result<Vec<_>, _>>()?;

    let modules = [
        ("step_one", "value + 1"),
        ("step_two", "value * 2"),
        ("step_three", "value * 3"),
        ("step_four", "value * value"),
    ];
    let families = [
        "positive",
        "negative",
        "large positive",
        "large negative",
        "huge positive",
        "huge negative",
    ];
    let mut criteria = Vec::new();
    for (index, range) in ranges[..24].iter().enumerate() {
        let (module, formula) = modules[index / 6];
        let family = families[index % 6];
        let id = u8::try_from(index + 1)?;
        criteria.push(
            AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([id; 32]),
                AcceptanceCriterionStatement::try_from_string(format!(
                    "{module}.compute returns {formula} for {family} ints (R{id:02})."
                ))?,
            )
            .with_origin(CriterionOrigin::new(
                *range,
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
            )),
        );
    }
    let addresses = criteria[..6].iter().map(AcceptanceCriterion::id).collect();
    let constraints = CONSTRAINTS
        .iter()
        .zip(&ranges[24..])
        .map(|(text, range)| {
            Ok(GoalConstraint::try_from_string((*text).to_owned())?.with_origin(*range))
        })
        .collect::<TestResult<Vec<_>>>()?;
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string(
            "Repair the four existing integer compute functions".to_owned(),
        )?,
        criteria,
        constraints,
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Verify all original input families and each ordered delivery".to_owned(),
        )?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete());
    let step = TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Correct step_one.compute for its exact integer formula".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "The original requires the first module before later deliveries".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "An actual scoped diff for step_one.py".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string(
                "Only the first module may change in this edit".to_owned(),
            )?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?,
        ),
    )?
    .with_criterion_coverage(addresses, Vec::new())?;
    Ok((goal, step))
}

async fn verify_reopened_partial_goal(
    fixture: &Fixture,
    goal: &GoalContract,
    evidence: TaskEvidenceId,
    snapshot: SnapshotId,
    settled: EditPhaseReservedUsage,
) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let run_id = AgentRunId::from_bytes([RUN; 32]);
    let restored_usage = store
        .load_edit_phase_usage(&fixture.project, run_id)
        .await?;
    assert_eq!(usage_json(&restored_usage), usage_json(&settled));
    let restored = store
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("original goal missing after reopen")?;
    assert_eq!(&restored, goal);
    assert_eq!(restored.draft().acceptance_criteria().len(), 24);
    assert_eq!(restored.draft().constraints().len(), 6);
    assert_eq!(restored.draft().constraints()[3].as_str(), LATE);
    assert!(GoalOriginalCoverage::derive_from(&restored)?.is_complete());
    let ledger = store
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("partial ledger missing after reopen")?;
    let steps = ledger.ledger().steps().collect::<Vec<_>>();
    let [step] = steps.as_slice() else {
        return Err("unexpected additional steps".into());
    };
    assert_eq!(step.status(), TaskStepStatus::Completed);
    assert_eq!(step.definition().addressed_criteria().len(), 6);
    assert!(step.definition().acceptance_criteria().is_empty());

    let run = store
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("partial run missing after reopen")?;
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.current_snapshot_id(), snapshot);
    let state = store
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[evidence],
            snapshot,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::Diff(diff)] = state.evidence() else {
        return Err("expected one durable local diff".into());
    };
    assert!(diff.complete());
    assert_eq!(diff.snapshot_id(), snapshot);
    assert_eq!(
        diff.changed_paths(),
        &[RepositoryPath::try_from_bytes(TARGET.as_bytes().to_vec())?]
    );
    let memory = RunMemoryCheckpoint::compile(
        &restored,
        ledger.ledger(),
        &run,
        state.published_index(),
        Vec::new(),
    )?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &run,
        restored,
        ledger.ledger().clone(),
        memory,
    )?;
    assert_eq!(
        DeterministicAcceptanceVerifier::new(&store)
            .verify(&request, AcceptanceVerifierTimeout::DEFAULT, &ActiveControl)
            .await?,
        AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
    );
    write_report(
        &fixture.root,
        "reopen-result.json",
        json!({"original_associations":30,"remaining_runtime_obligations":24,
            "local_diff_steps":1,"source_done":false,"protected_bytes_match":true,
            "durable_usage":usage_json(&restored_usage)}),
    )
}
