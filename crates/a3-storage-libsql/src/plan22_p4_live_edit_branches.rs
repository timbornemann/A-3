//! Real local Edit replies that keep the original obligation open.

use super::*;
use a3_application::{
    AcquiredSourceRead, AgentReadTimeout, AgentReadTools, AppendAgentRead, BoundEditSource,
    CompileTaskLens, EditEvidenceNeed, EditPhasePrerequisites, EditPhaseRequestFailure,
    RunEventPageLimit, SourceDeliveryOutcome, SourceDeliveryProgress, SourceReadStore,
    SourceWireDelivery,
};
use a3_context::DeterministicAgentReadTools;
use a3_domain::{
    AgentRunUsage, AgentToolEvidence, FileRevision, GraphEndpoint, RunEventKind, RunEventSequence,
    RunEventSubject, SymbolId, SyntaxRelationKind, TaskLensSeed, TaskLensSeedSet, TaskLensSeedText,
    TaskLensTokenBudget,
};
use std::collections::BTreeSet;

const TIMEOUT_TARGET: &str = "def cap_timeout(value: int) -> int:\n    return value\n";
const JOBS: &str = "from timeout_policy import cap_timeout as apply_cap\n\ndef configure_timeout(seconds: int) -> int:\n    return apply_cap(seconds * 1_000)\n";
const BANNER_TARGET: &str = "DEVICE_BANNER = ''\n";
const BANNER_BYTES: usize = 12_000;
const CALLER_TOOL: ToolRunId = ToolRunId::from_bytes([181; 32]);
const BONSAI_TAG: &str = "bonsai-2:27b";
const ORNITH_TAG: &str = "ornith-1.5:9b";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Branch {
    Caller,
    CallerContinuation,
    KnownCaller,
    TooLarge,
}

impl Branch {
    fn key(self) -> &'static str {
        match self {
            Self::Caller => "caller",
            Self::CallerContinuation => "caller_continuation",
            Self::KnownCaller => "known_caller",
            Self::TooLarge => "too_large",
        }
    }

    fn needs_caller(self) -> bool {
        self != Self::TooLarge
    }

    fn expected(self) -> &'static str {
        match self {
            Self::Caller | Self::CallerContinuation => "caller",
            Self::KnownCaller => "proposal",
            Self::TooLarge => "too_large",
        }
    }

    fn target(self) -> (&'static str, &'static str, u32) {
        match self {
            Self::Caller | Self::CallerContinuation | Self::KnownCaller => {
                ("timeout_policy.py", TIMEOUT_TARGET, 2)
            }
            Self::TooLarge => ("device_banner.py", BANNER_TARGET, 1),
        }
    }

    fn request(self) -> (&'static str, &'static str) {
        match self {
            Self::Caller | Self::CallerContinuation => (
                "Cap a background job's configured timeout at 30 seconds.",
                "Read the existing caller to establish its unit before editing. Change only cap_timeout and leave smaller timeouts unchanged.",
            ),
            Self::KnownCaller => (
                "Cap a background job's configured timeout at 30 seconds.",
                "Read the existing caller to establish its unit. Preserve the cap_timeout signature and use a single min call, leaving smaller timeouts unchanged.",
            ),
            Self::TooLarge => (
                "Set DEVICE_BANNER to one Python string literal containing exactly 12000 ASCII A bytes.",
                "Check whether the full literal fits before drafting it. Use the tooLarge result if the edit exceeds the replacement limit. The supplied assignment is the complete target. Do not use expressions or partial edits.",
            ),
        }
    }
}

#[test]
#[ignore = "explicit 4k component run; set A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR to a fresh path"]
fn live_ollama_4k_edit_requests_actual_caller() -> TestResult {
    run_live_branch(Branch::Caller)
}

#[test]
#[ignore = "explicit 4k component run; set A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR to a fresh path"]
fn live_ollama_4k_edit_reports_indivisible_output_too_large() -> TestResult {
    run_live_branch(Branch::TooLarge)
}

#[test]
#[ignore = "explicit two-call 4k component; set A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR to a fresh path"]
fn live_ollama_4k_edit_proposes_cap_after_actual_caller_delivery() -> TestResult {
    run_live_branch(Branch::CallerContinuation)
}

#[test]
#[ignore = "explicit one-call 4k component; set A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR to a fresh path"]
fn live_ollama_4k_edit_requires_current_caller_before_dispatch() -> TestResult {
    run_live_branch(Branch::KnownCaller)
}

fn run_live_branch(branch: Branch) -> TestResult {
    crate::run_native_libsql_live_test(async {
        let tag = match std::env::var("A3_PLAN22_P4_EDIT_BRANCH_MODEL") {
            Err(std::env::VarError::NotPresent) => LIVE_TAG,
            Ok(tag) if branch == Branch::TooLarge && tag == BONSAI_TAG => BONSAI_TAG,
            Ok(tag) if branch == Branch::TooLarge && tag == ORNITH_TAG => ORNITH_TAG,
            _ => return Err("only the TooLarge branch supports alternate local tags".into()),
        };
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR")
                .ok_or("explicit Edit branch output directory required")?,
        );
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("Edit branch requires an absolute unused output directory".into());
        }
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        write_report(
            &root,
            "native-start.json",
            json!({"version":1,"phase":"edit_branch","branch":branch.key(),"tag":tag}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_branch(&root, branch, tag));
        write_report(
            &root,
            "native-final.json",
            json!({"version":1,"success":result.is_ok(),"branch":branch.key(),"goal_done":false}),
        )?;
        result
    })
}

async fn run_branch(root: &Path, branch: Branch, tag: &str) -> TestResult {
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    let (file, text, _) = branch.target();
    fs::write(worktree.join(file), text.as_bytes())?;
    fs::write(worktree.join("notes.txt"), OTHER_FILE)?;
    if branch.needs_caller() {
        fs::write(worktree.join("jobs.py"), JOBS.as_bytes())?;
    }
    let before = inventory(&worktree)?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let completed = dispatch_branch(&fixture, branch, tag).await?;
    verify_branch_reopened(&fixture, &completed).await?;
    assert!(inventory(&worktree)? == before);
    write_report(
        root,
        "edit-branch-reopen.json",
        json!({
            "branch":branch.key(),"goal_done":false,"runtime_obligations_open":1,
            "files_unchanged":true,"file_count":before.len(),"mutations":0,"policy_decisions":0,
            "read_only_tool_events":usize::from(completed.caller.is_some()),
            "caller_acquired":completed.caller.is_some(),"caller_delivered_to_model":completed.caller_delivered,
            "usage":usage_json(&completed.usage),
        }),
    )
}

struct CompletedBranch {
    goal: GoalContract,
    ledger: StoredTaskLedger,
    run: AgentRun,
    sequence_before: RunEventSequence,
    usage: EditPhaseReservedUsage,
    caller: Option<AgentToolEvidence>,
    caller_delivered: bool,
}

struct KnownCaller {
    bound: BoundEditSource,
    page: AgentSourcePage,
    acquired: AcquiredSourceRead,
    initial_job: ContentHash,
    dispatch_at: AgentRunTimestamp,
}

async fn dispatch_branch(
    fixture: &Fixture,
    branch: Branch,
    tag: &str,
) -> TestResult<CompletedBranch> {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let snapshot = published.run().snapshot_id();
    let (file, expected_source, lines) = branch.target();
    let path = RepositoryPath::try_from_bytes(file.as_bytes().to_vec())?;
    let (goal, step) = branch_goal(branch, &path)?;
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("target revision missing from real index")?;
    let page = WorkspaceAgentSourceReader
        .read_page(
            project,
            revision,
            &AgentFileInspection::new(
                path,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(u16::try_from(lines)?)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert!(page.text().as_bytes() == expected_source.as_bytes());
    assert_eq!(page.range().start_byte(), 0);
    assert_eq!(
        page.range().end_byte(),
        u32::try_from(expected_source.len())?
    );
    assert!(!page.truncated());

    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_profile_for_tag(&fixture.root, &provider, tag).await?;
    let (mut run, ledger, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, snapshot, vec![step]).await?;
    assert_eq!(unplanned.len(), 1);
    let sequence_before = run.last_event_sequence();
    let observer = ObservedEditProvider {
        inner: &provider,
        fixture,
        store: &store,
        run_id: run.id(),
        kind: EditPhaseCallKind::Initial,
        stats: Mutex::new(ObservedEditStats::default()),
    };
    let known = if branch == Branch::KnownCaller {
        let started = Instant::now();
        let bound = bind_caller(
            project,
            &store,
            &published,
            target_callee(&published, revision)?,
        )
        .await?;
        let state = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
        let phase = PreparedEditPhase::update(state, &page)?;
        let missing = build_edit_phase_request(
            &goal,
            &ledger,
            &phase,
            state,
            profile.clone(),
            None,
            EditPhasePrerequisites::none().with_required_source(
                EditEvidenceNeed::Caller,
                &bound,
                None,
            ),
        );
        assert!(matches!(
            missing,
            Err(EditPhaseRequestFailure::NeedEvidence(
                EditEvidenceNeed::Caller
            ))
        ));
        let before_usage = store.load_edit_phase_usage(project, run.id()).await?;
        assert_eq!(before_usage, EditPhaseReservedUsage::default());
        let before_dispatches = observer
            .stats
            .lock()
            .map_err(|_| "Edit observer poisoned")?
            .dispatches;
        assert_eq!(before_dispatches, 0);
        assert_eq!(run.last_event_sequence(), sequence_before);
        write_report(
            &fixture.root,
            "edit-known-caller-preflight.json",
            json!({
                "outcome":"need_caller","dispatches":before_dispatches,"usage":usage_json(&before_usage),
                "job_blake3":hash_hex(phase.job_digest()),
                "source_revision_blake3":hash_hex(bound.required().revision().content_hash()),
            }),
        )?;
        let anchor = state.anchor();
        let read_at = elapsed_after(10, started)?;
        let (caller, acquired) = acquire_caller(
            fixture, &store, &published, &mut run, anchor, &bound, read_at,
        )
        .await?;
        assert_eq!(run.last_event_sequence().get(), sequence_before.get() + 1);
        Some(KnownCaller {
            bound,
            page: caller,
            acquired,
            initial_job: phase.job_digest(),
            dispatch_at: at(elapsed_after(12, started)?)?,
        })
    } else {
        None
    };
    let state = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
    let phase = PreparedEditPhase::update(state, &page)?;
    let memory =
        RunMemoryCheckpoint::compile(&goal, ledger.ledger(), &run, &published, Vec::new())?;
    let prerequisites = known
        .as_ref()
        .map_or_else(EditPhasePrerequisites::none, |caller| {
            EditPhasePrerequisites::none().with_required_source(
                EditEvidenceNeed::Caller,
                &caller.bound,
                Some((&caller.page, &caller.acquired)),
            )
        });
    let mut compiled = build_edit_phase_request(
        &goal,
        &ledger,
        &phase,
        state,
        profile.clone(),
        Some(&memory),
        prerequisites,
    )?;
    let messages = compiled.request().messages();
    if let Some(caller) = &known {
        assert_ne!(compiled.job_digest(), caller.initial_job);
        assert_packed_caller(&compiled, &phase, &caller.page)?;
    } else {
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].content(), phase.input_json());
        assert!(
            messages
                .iter()
                .all(|message| !message.content().contains("apply_cap("))
        );
    }
    let active = messages[1].content();
    assert!(active.contains(&format!("Rule: {}\n", branch.request().1)));
    assert!(active.contains("Coverage total=1 outside=0 no-proof-step=1.\n"));
    let projection = provider.project_wire_request(compiled.request())?;
    assert_eq!(
        (projection.context_tokens(), projection.output_tokens()),
        (4_096, 1_024)
    );
    assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
    let fit = PhaseWireBudget::fit(projection, &profile);
    let literal_lower_bound = (branch == Branch::TooLarge).then_some(BANNER_BYTES);
    if let Some(bytes) = literal_lower_bound {
        assert!(bytes > 8 * 1_024);
    }
    write_report(
        &fixture.root,
        "edit-intent.json",
        json!({
            "branch":branch.key(),"job_blake3":hash_hex(compiled.job_digest()),"wire":wire_json(projection),
            "request_projection_blake3":hash_hex(compiled.request_digest()),
            "source_bytes":page.text().len(),"literal_payload_minimum_bytes":literal_lower_bound,
            "charged_prompt_tokens":fit.as_ref().ok().map(|value| value.prompt_tokens()),
            "charged_output_tokens":fit.as_ref().ok().map(|value| value.output_tokens()),
            "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
        }),
    )?;
    let budget = fit?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let started = Instant::now();
    let result = ExecuteEditPhase::new(&observer, store.as_ref(), &patch)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                phase,
                &mut compiled,
                state,
                known.as_ref().map_or(at(10)?, |caller| caller.dispatch_at),
                &patch,
                &ActiveControl,
            ))
            .await?,
            state,
            &compiled,
            known.as_ref().map_or(at(10)?, |caller| caller.dispatch_at),
            ModelRequestTimeout::from_millis(180_000)?,
            &ActiveControl,
        )
        .await;
    let elapsed = u64::try_from(started.elapsed().as_millis())?;
    let usage = store.load_edit_phase_usage(project, run.id()).await;
    let stream = *observer
        .stats
        .lock()
        .map_err(|_| "Edit observer poisoned")?;
    let actual = result
        .as_ref()
        .ok()
        .map(|report| branch_reply(&report.outcome));
    write_report(
        &fixture.root,
        "edit-result.json",
        json!({
            "expected":branch.expected(),"actual":actual,"elapsed_millis":elapsed,
            "failure":result.as_ref().err().map(|error| format!("{error:?}")),
            "dispatches":stream.dispatches,"finish":stream.finish.map(|end| format!("{:?}",end.reason())),
            "provider_prompt_tokens":stream.finish.and_then(|end| end.usage().prompt_tokens()),
            "provider_output_tokens":stream.finish.and_then(|end| end.usage().output_tokens()),
            "durable_usage":usage.as_ref().ok().map(usage_json),
        }),
    )?;
    if branch == Branch::TooLarge
        && tag == ORNITH_TAG
        && matches!(&result, Err(EditPhaseExecutionFailure::OutputLimit))
    {
        let previous = usage?;
        assert_eq!(stream.dispatches, 1);
        assert_eq!(
            stream.finish.map(|end| end.reason()),
            Some(ModelFinishReason::OutputLimit)
        );
        assert_eq!(
            previous,
            EditPhaseReservedUsage {
                calls: 1,
                prompt_tokens: u64::from(budget.prompt_tokens()),
                output_tokens: u64::from(budget.output_tokens()),
                ..EditPhaseReservedUsage::default()
            }
        );
        let usage = observer
            .measure_truncated_repair(
                PreparedEditPhase::update(state, &page)?,
                state,
                &compiled,
                &patch,
                previous,
                at(elapsed_after(11, started)?)?,
            )
            .await?;
        return Ok(CompletedBranch {
            goal,
            ledger,
            run,
            sequence_before,
            usage,
            caller: None,
            caller_delivered: false,
        });
    }
    let report = result?;
    let mut usage = usage?;
    assert_eq!(report.projection, projection);
    assert_eq!(report.budget, budget);
    assert_eq!(stream.dispatches, 1);
    assert_eq!(
        stream.finish.map(|end| end.reason()),
        Some(ModelFinishReason::Stop)
    );
    assert_eq!(
        (
            usage.calls,
            usage.repairs,
            usage.unobserved_calls,
            usage.unknown_calls
        ),
        (1, 0, 0, 0)
    );
    assert_eq!(usage.prompt_tokens, u64::from(budget.prompt_tokens()));
    assert_eq!(usage.output_tokens, u64::from(budget.output_tokens()));
    if let Some(caller) = known {
        let delivery = verified_caller_delivery(&report, projection, snapshot, &caller.page)?;
        let durable = store
            .load_source_read_state(project, state.anchor())
            .await?;
        let [receipt] = durable.progress().receipts() else {
            return Err("observed caller lacks one durable source receipt".into());
        };
        assert_eq!(receipt.delivery(), Some(delivery));
        let SourceDeliveryOutcome::NewBytes(new_bytes) = receipt.outcome() else {
            return Err("observed caller did not add new source bytes".into());
        };
        assert_eq!(new_bytes, u32::try_from(caller.page.text().len())?);
        assert!(durable.progress().pending().is_none());
        write_report(
            &fixture.root,
            "edit-known-caller-delivery.json",
            json!({
                "policy_version":SourceDeliveryProgress::POLICY_VERSION,
                "ordinal":receipt.admission().ordinal().get(),"new_bytes":new_bytes,
                "wire_blake3":hash_hex(delivery.wire_digest()),
                "revision_blake3":hash_hex(delivery.source().revision().content_hash()),
                "start":delivery.source().range().start_byte(),"end":delivery.source().range().end_byte(),
            }),
        )?;
        record_caller_oracle(
            &fixture.root,
            "edit-known-caller-oracle.json",
            &report.outcome,
            delivery,
        )?;
        return Ok(CompletedBranch {
            goal,
            ledger,
            run,
            sequence_before,
            usage,
            caller: Some(caller.page.evidence()),
            caller_delivered: true,
        });
    }

    assert!(report.source_delivery.is_none());
    if actual != Some(branch.expected()) {
        return Err("real Edit did not return the expected safe branch".into());
    }
    let caller = if branch.needs_caller() {
        let callee = target_callee(&published, revision)?;
        let time = 10_u64
            .checked_add(elapsed)
            .and_then(|time| time.checked_add(1))
            .ok_or("branch clock overflow")?;
        let acquisition_started = Instant::now();
        let bound = bind_caller(project, &store, &published, callee).await?;
        let anchor = EditPhaseState::new(project, &goal, &ledger, &run, &published)?.anchor();
        let (caller, acquired) =
            acquire_caller(fixture, &store, &published, &mut run, anchor, &bound, time).await?;
        if branch == Branch::CallerContinuation {
            let state = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
            let next = PreparedEditPhase::update(state, &page)?;
            assert_ne!(next.job_digest(), compiled.job_digest());
            let memory =
                RunMemoryCheckpoint::compile(&goal, ledger.ledger(), &run, &published, Vec::new())?;
            let prerequisites = EditPhasePrerequisites::none().with_required_source(
                EditEvidenceNeed::Caller,
                &bound,
                Some((&caller, &acquired)),
            );
            let mut next_request = build_edit_phase_request(
                &goal,
                &ledger,
                &next,
                state,
                profile,
                Some(&memory),
                prerequisites,
            )?;
            let continuation_time = time
                .checked_add(u64::try_from(acquisition_started.elapsed().as_millis())?)
                .and_then(|time| time.checked_add(2))
                .ok_or("continuation clock overflow")?;
            usage = ContinuationProbe {
                root: &fixture.root,
                project,
                provider: &provider,
                store: store.as_ref(),
            }
            .measure(
                next,
                state,
                &mut next_request,
                &caller,
                usage,
                at(continuation_time)?,
            )
            .await?;
        }
        Some(caller.evidence())
    } else {
        None
    };
    Ok(CompletedBranch {
        goal,
        ledger,
        run,
        sequence_before,
        usage,
        caller,
        caller_delivered: branch == Branch::CallerContinuation,
    })
}

fn branch_reply(outcome: &EditPhaseOutcome) -> &'static str {
    match outcome {
        EditPhaseOutcome::Proposal(_) => "proposal",
        EditPhaseOutcome::NeedEvidence(EditEvidenceNeed::Caller) => "caller",
        EditPhaseOutcome::NeedEvidence(EditEvidenceNeed::Definition) => "definition",
        EditPhaseOutcome::NeedEvidence(EditEvidenceNeed::Test) => "test",
        EditPhaseOutcome::NeedEvidence(EditEvidenceNeed::Unknown) => "unknown_need",
        EditPhaseOutcome::TooLarge => "too_large",
    }
}

impl ObservedEditProvider<'_> {
    async fn measure_truncated_repair(
        self,
        phase: PreparedEditPhase,
        state: EditPhaseState<'_>,
        initial: &CompiledEditPhaseRequest,
        patch: &WorkspacePatchAdapter,
        previous: EditPhaseReservedUsage,
        observed_at: AgentRunTimestamp,
    ) -> TestResult<EditPhaseReservedUsage> {
        let mut request = initial.for_truncated_output_repair()?;
        assert_eq!(request.anchor(), initial.anchor());
        assert_eq!(request.job_digest(), initial.job_digest());
        assert_ne!(request.request_digest(), initial.request_digest());
        assert_eq!(request.request().profile(), initial.request().profile());
        assert_eq!(
            request.request().structured_output(),
            initial.request().structured_output()
        );
        assert_eq!(
            request.request().messages()[1..],
            initial.request().messages()[1..]
        );

        let observer = Self {
            kind: request.kind(),
            stats: Mutex::new(ObservedEditStats::default()),
            ..self
        };
        let root = &observer.fixture.root;
        let projection = observer.project_wire_request(request.request())?;
        let fit = PhaseWireBudget::fit(projection, request.request().profile());
        write_report(
            root,
            "edit-repair-intent.json",
            json!({
                "reason":"output_limit","job_blake3":hash_hex(request.job_digest()),
                "request_projection_blake3":hash_hex(request.request_digest()),
                "wire":wire_json(projection),
                "charged_prompt_tokens":fit.as_ref().ok().map(|budget| budget.prompt_tokens()),
                "charged_output_tokens":fit.as_ref().ok().map(|budget| budget.output_tokens()),
                "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
            }),
        )?;
        let budget = fit?;
        assert_eq!(
            (projection.context_tokens(), projection.output_tokens()),
            (4_096, 1_024)
        );
        assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
        let started = Instant::now();
        let execution = ExecuteEditPhase::new(&observer, observer.store, patch)
            .execute(
                Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                    phase,
                    &mut request,
                    state,
                    observed_at,
                    patch,
                    &ActiveControl,
                ))
                .await?,
                state,
                &request,
                observed_at,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        let usage = observer
            .store
            .load_edit_phase_usage(&observer.fixture.project, observer.run_id)
            .await;
        let stream = *observer
            .stats
            .lock()
            .map_err(|_| "Edit repair observer poisoned")?;
        let actual = execution
            .as_ref()
            .ok()
            .map(|report| branch_reply(&report.outcome));
        write_report(
            root,
            "edit-repair-result.json",
            json!({
                "expected":"too_large","actual":actual,"elapsed_millis":started.elapsed().as_millis(),
                "failure":execution.as_ref().err().map(|error| format!("{error:?}")),
                "dispatches":stream.dispatches,"finish":stream.finish.map(|end| format!("{:?}",end.reason())),
                "provider_prompt_tokens":stream.finish.and_then(|end| end.usage().prompt_tokens()),
                "provider_output_tokens":stream.finish.and_then(|end| end.usage().output_tokens()),
                "durable_usage":usage.as_ref().ok().map(usage_json),
            }),
        )?;
        let report = execution?;
        let usage = usage?;
        assert_eq!(report.projection, projection);
        assert_eq!(report.budget, budget);
        assert!(report.source_delivery.is_none());
        assert_eq!(stream.dispatches, 1);
        assert_eq!(
            stream.finish.map(|end| end.reason()),
            Some(ModelFinishReason::Stop)
        );
        assert_eq!(
            usage,
            EditPhaseReservedUsage {
                calls: 2,
                repairs: 1,
                prompt_tokens: previous.prompt_tokens + u64::from(budget.prompt_tokens()),
                output_tokens: previous.output_tokens + u64::from(budget.output_tokens()),
                ..EditPhaseReservedUsage::default()
            }
        );
        if !matches!(report.outcome, EditPhaseOutcome::TooLarge) {
            return Err("truncated Edit repair did not report the indivisible output limit".into());
        }
        Ok(usage)
    }
}

fn target_callee(published: &PublishedIndex, revision: &FileRevision) -> TestResult<SymbolId> {
    published
        .publication()
        .graph()
        .symbols()
        .iter()
        .find(|symbol| {
            symbol.revision() == revision && symbol.parsed().name().as_str() == "cap_timeout"
        })
        .map(|symbol| symbol.id())
        .ok_or_else(|| "target symbol missing from real index".into())
}

fn elapsed_after(millis: u64, started: Instant) -> TestResult<u64> {
    millis
        .checked_add(u64::try_from(started.elapsed().as_millis())?)
        .ok_or_else(|| "branch clock overflow".into())
}

async fn bind_caller(
    project: &ProjectIdentity,
    store: &LibsqlKnowledgeStore,
    published: &PublishedIndex,
    callee: SymbolId,
) -> TestResult<BoundEditSource> {
    let snapshot = published.run().snapshot_id();
    let incoming = published
        .publication()
        .graph()
        .edges()
        .iter()
        .filter_map(|edge| {
            if edge.kind() == SyntaxRelationKind::Calls
                && edge.snapshot_id() == snapshot
                && edge.target() == &GraphEndpoint::Symbol(callee)
                && let GraphEndpoint::Symbol(caller) = edge.source()
            {
                Some(*caller)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    if incoming.len() != 1 {
        return Err("expected one resolved current direct caller".into());
    }
    let caller = *incoming.first().ok_or("direct caller missing")?;
    let seed =
        TaskLensSeedText::try_from_string("Resolve the existing timeout caller's unit".to_owned())?;
    let lens = CompileTaskLens::new(store, store, store)
        .execute(
            project,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![
                    TaskLensSeed::ExplicitSymbol(callee),
                    TaskLensSeed::ExplicitSymbol(caller),
                ],
            )?,
            TaskLensTokenBudget::new(4_096)?,
            &ActiveControl,
        )
        .await?;
    let bound = BoundEditSource::for_direct_caller(project, published, &lens, callee, caller)?;
    assert_eq!(bound.required().revision().path().as_bytes(), b"jobs.py");
    Ok(bound)
}

async fn acquire_caller(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    published: &PublishedIndex,
    run: &mut AgentRun,
    anchor: a3_application::EditPhaseAnchor,
    bound: &BoundEditSource,
    time: u64,
) -> TestResult<(AgentSourcePage, AcquiredSourceRead)> {
    let project = &fixture.project;
    let snapshot = published.run().snapshot_id();
    let action = bound.read_action(project, published)?;
    let admission = store
        .begin_source_read(project, anchor, bound, CALLER_TOOL, at(time)?)
        .await?;
    assert_eq!(
        admission.attempt().status(),
        AgentToolAttemptStatus::InFlight
    );
    let reader = DeterministicAgentReadTools::new(store, store, store, &WorkspaceAgentSourceReader);
    let mut read = reader
        .execute(
            project,
            snapshot,
            CALLER_TOOL,
            &action,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = bound.take_original(project, published, CALLER_TOOL, &mut read)?;
    assert!(page.text().as_bytes() == &JOBS.as_bytes()[53..135]);
    assert_eq!(
        (page.range().start_byte(), page.range().end_byte()),
        (53, 135)
    );
    assert_eq!(page.revision(), bound.required().revision());
    assert!(!page.truncated());
    let evidence = page.evidence();
    let sequence = run.last_event_sequence();
    let recorded = read.record(run, RunEventId::from_bytes([181; 32]), at(time + 1)?)?;
    assert_eq!(recorded.original_evidence(), Some(evidence.id()));
    AppendAgentRead::new(store)
        .execute(project, sequence, run, &recorded)
        .await?;
    write_report(
        &fixture.root,
        "caller-acquired.json",
        json!({
            "source_bytes":page.text().len(),"start":53,"end":135,"delivered_to_model":false,
            "revision_blake3":hash_hex(page.revision().content_hash()),
            "source_blake3":blake3::hash(page.text().as_bytes()).to_hex().to_string(),
        }),
    )?;
    let task = run.goal_contract().task_id();
    let goal = store
        .load_current_goal_contract(project, task)
        .await?
        .ok_or("source Goal missing after ToolAction")?;
    let ledger = store
        .load_task_ledger(project, task)
        .await?
        .ok_or("source Ledger missing after ToolAction")?;
    let current = EditPhaseState::new(project, &goal, &ledger, run, published)?.anchor();
    let durable = store.load_source_read_state(project, current).await?;
    assert!(durable.progress().receipts().is_empty() && durable.progress().pending().is_some());
    let acquired = durable
        .acquired()
        .ok_or("original caller stamp missing")?
        .clone();
    assert_eq!(acquired.key(), admission.key());
    Ok((page, acquired))
}

fn assert_packed_caller(
    request: &CompiledEditPhaseRequest,
    phase: &PreparedEditPhase,
    caller: &AgentSourcePage,
) -> TestResult {
    let [_, _, supplemental, target] = request.request().messages() else {
        return Err("Edit request did not include exactly one supplemental caller".into());
    };
    assert_eq!(target.content(), phase.input_json());
    let supplemental: Value = serde_json::from_str(supplemental.content())?;
    assert_eq!(
        supplemental["readOnly"],
        json!({"kind":"caller","path":"jobs.py","text":caller.text()})
    );
    Ok(())
}

fn verified_caller_delivery<'a>(
    report: &'a EditPhaseExecutionReport,
    projection: ModelWireProjection,
    snapshot: SnapshotId,
    caller: &AgentSourcePage,
) -> TestResult<&'a SourceWireDelivery> {
    let delivery = report
        .source_delivery
        .as_ref()
        .ok_or("actual caller wire delivery missing")?;
    assert_eq!(delivery.wire_digest(), projection.digest());
    assert_eq!(delivery.source().snapshot_id(), snapshot);
    assert_eq!(delivery.source().revision(), caller.revision());
    assert_eq!(delivery.source().range(), caller.range());
    Ok(delivery)
}

struct ContinuationProbe<'a> {
    root: &'a Path,
    project: &'a ProjectIdentity,
    provider: &'a OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
}

impl ContinuationProbe<'_> {
    async fn measure(
        self,
        phase: PreparedEditPhase,
        state: EditPhaseState<'_>,
        request: &mut CompiledEditPhaseRequest,
        caller: &AgentSourcePage,
        previous: EditPhaseReservedUsage,
        observed_at: AgentRunTimestamp,
    ) -> TestResult<EditPhaseReservedUsage> {
        let patch = WorkspacePatchAdapter::with_change_window_preview();
        let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
            phase,
            request,
            state,
            observed_at,
            &patch,
            &ActiveControl,
        ))
        .await?;
        assert_packed_caller(request, &phase, caller)?;
        let projection = self.provider.project_wire_request(request.request())?;
        assert_eq!(
            (projection.context_tokens(), projection.output_tokens()),
            (4_096, 1_024)
        );
        assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
        let fit = PhaseWireBudget::fit(projection, request.request().profile());
        write_report(
            self.root,
            "edit-continuation-intent.json",
            json!({
                "job_blake3":hash_hex(request.job_digest()),"wire":wire_json(projection),
                "caller_source_bytes":caller.text().len(),
                "charged_prompt_tokens":fit.as_ref().ok().map(|value| value.prompt_tokens()),
                "charged_output_tokens":fit.as_ref().ok().map(|value| value.output_tokens()),
                "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
            }),
        )?;
        let budget = fit?;
        let started = Instant::now();
        let execution = ExecuteEditPhase::new(self.provider, self.store, &patch)
            .execute(
                phase,
                state,
                request,
                observed_at,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        let usage = self
            .store
            .load_edit_phase_usage(self.project, state.anchor().run())
            .await;
        write_report(
            self.root,
            "edit-continuation-result.json",
            json!({
                "elapsed_millis":started.elapsed().as_millis(),
                "failure":execution.as_ref().err().map(|error| format!("{error:?}")),
                "actual":execution.as_ref().ok().map(|report| match &report.outcome {
                    EditPhaseOutcome::Proposal(_) => "proposal",
                    EditPhaseOutcome::NeedEvidence(_) => "need_evidence",
                    EditPhaseOutcome::TooLarge => "too_large",
                }),
                "need_kind":execution.as_ref().ok().and_then(|report| match &report.outcome {
                    EditPhaseOutcome::NeedEvidence(kind) => Some(format!("{kind:?}")),
                    _ => None,
                }),
                "provider_prompt_tokens":execution.as_ref().ok().and_then(|report| report.usage.prompt_tokens()),
                "provider_output_tokens":execution.as_ref().ok().and_then(|report| report.usage.output_tokens()),
                "durable_usage":usage.as_ref().ok().map(usage_json),
            }),
        )?;
        let report = execution?;
        let usage = usage?;
        assert_eq!(report.projection, projection);
        assert_eq!(report.budget, budget);
        assert_eq!(
            (
                usage.calls,
                usage.repairs,
                usage.unobserved_calls,
                usage.unknown_calls
            ),
            (2, 0, 0, 0)
        );
        assert_eq!(
            usage.prompt_tokens,
            previous.prompt_tokens + u64::from(budget.prompt_tokens())
        );
        assert_eq!(
            usage.output_tokens,
            previous.output_tokens + u64::from(budget.output_tokens())
        );
        assert!(report.usage.prompt_tokens().is_some_and(|value| value > 0));
        assert!(report.usage.output_tokens().is_some_and(|value| value > 0));
        let delivery =
            verified_caller_delivery(&report, projection, state.anchor().snapshot(), caller)?;
        write_report(
            self.root,
            "edit-continuation-delivery.json",
            json!({
                "wire_blake3":hash_hex(delivery.wire_digest()),
                "caller_start":caller.range().start_byte(),"caller_end":caller.range().end_byte(),
            }),
        )?;
        record_caller_oracle(
            self.root,
            "edit-continuation-oracle.json",
            &report.outcome,
            delivery,
        )?;
        Ok(usage)
    }
}

fn record_caller_oracle(
    root: &Path,
    name: &str,
    outcome: &EditPhaseOutcome,
    delivery: &SourceWireDelivery,
) -> TestResult {
    let EditPhaseOutcome::Proposal(AgentAction::ApplyPatch(action)) = outcome else {
        return Err("Edit did not produce a timeout proposal".into());
    };
    let [PatchOperation::Update(update)] = action.operations() else {
        return Err("timeout proposal must change exactly one current file".into());
    };
    assert_eq!(update.expected().path().as_bytes(), b"timeout_policy.py");
    assert_eq!(
        update.expected().content_hash(),
        ContentHash::from_bytes(*blake3::hash(TIMEOUT_TARGET.as_bytes()).as_bytes())
    );
    let bytes = update.content().as_bytes();
    let checked_cap = timeout_cap_value(bytes);
    let verified = checked_cap == Some(30_000);
    write_report(
        root,
        name,
        json!({
            "caller_delivered_to_model":true,"wire_blake3":hash_hex(delivery.wire_digest()),
            "caller_start":delivery.source().range().start_byte(),
            "caller_end":delivery.source().range().end_byte(),
            "timeout_cap_milliseconds":30_000,"selected_function_verified":verified,"applied":false,
            "checked_cap_milliseconds":checked_cap,
            "signature_preserved":bytes.starts_with(b"def cap_timeout(value: int) -> int:\n"),
            "direct_min_shape":std::str::from_utf8(bytes).is_ok_and(|text| text.contains("return min(")),
        }),
    )?;
    if !verified {
        return Err("timeout proposal did not satisfy the closed integer-cap oracle".into());
    }
    Ok(())
}

// In this isolated fixture `min` is unshadowed. Accept one direct integer expression
// preserving the complete existing signature. Never execute a generated source body.
fn timeout_cap_value(bytes: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(bytes).ok()?;
    let expression = text
        .strip_prefix("def cap_timeout(value: int) -> int:\n    return ")?
        .trim_end_matches(['\n', ' ', '\t']);
    if expression.contains(['\r', '\n']) {
        return None;
    }
    let (left, right) = expression
        .trim()
        .strip_prefix("min(")
        .and_then(|inner| inner.strip_suffix(')'))
        .and_then(|inner| inner.split_once(','))?;
    let constant = match (left.trim(), right.trim()) {
        ("value", constant) | (constant, "value") => constant,
        _ => return None,
    };
    let constant = constant
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(constant);
    match constant.split_once('*') {
        Some((a, b)) => timeout_integer(a)?.checked_mul(timeout_integer(b)?),
        None => timeout_integer(constant),
    }
}

fn timeout_integer(input: &str) -> Option<u64> {
    let input = input.trim();
    if input
        .split('_')
        .any(|digits| digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let digits = input.replace('_', "");
    if digits.starts_with('0') && digits.bytes().any(|byte| byte != b'0') {
        return None;
    }
    digits.parse().ok()
}

#[test]
fn caller_proposal_oracle_requires_exact_bounded_integer_cap() {
    let render = |body: &str| format!("def cap_timeout(value: int) -> int:\n    return {body}\n");
    for good in [
        "min(value, 30_000)",
        "min(30000, value)",
        "min(value, 30 * 1_000)",
        "min(value, (1000*30))",
    ] {
        assert_eq!(timeout_cap_value(render(good).as_bytes()), Some(30_000));
    }
    for bad in [
        "min(value, 30)",
        "min(value, 30000.0)",
        "max(value, 30000)",
        "min(value, 30000)\nother = 1",
        "min(value, VALUE)",
        "min(value, 030000)",
        "min(value, 30*1_000+1)",
    ] {
        assert_ne!(timeout_cap_value(render(bad).as_bytes()), Some(30_000));
    }
    assert_eq!(
        timeout_cap_value(b"def cap_timeout(value):\n    return min(value, 30000)\n"),
        None
    );
}

fn branch_goal(
    branch: Branch,
    path: &RepositoryPath,
) -> TestResult<(GoalContract, TaskStepDefinition)> {
    let task = TaskId::from_bytes([1; 32]);
    let (outcome, constraint) = branch.request();
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        format!("{outcome} {constraint}").into_bytes(),
    )?;
    let split = outcome.len() + 1;
    let criterion = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([2; 32]),
        AcceptanceCriterionStatement::try_from_string(outcome.to_owned())?,
    )
    .with_origin(CriterionOrigin::new(
        original.range(0, split)?,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    ));
    let criterion_id = criterion.id();
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string(outcome.to_owned())?,
        vec![criterion],
        vec![
            GoalConstraint::try_from_string(constraint.to_owned())?
                .with_origin(original.range(split, original.as_bytes().len())?),
        ],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Verify the requested result and preserve its source-bound restrictions".to_owned(),
        )?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    let step = TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string(outcome.to_owned())?,
        TaskStepRationale::try_from_string(
            "Address the original result at its existing assignment or declaration".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "An actual scoped diff at the selected path".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string("Only the selected path changes".to_owned())?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?,
        ),
    )?
    .with_criterion_coverage(vec![criterion_id], Vec::new())?;
    Ok((goal, step))
}

async fn verify_branch_reopened(fixture: &Fixture, completed: &CompletedBranch) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let project = &fixture.project;
    let run = store
        .load_agent_run(project, completed.run.id())
        .await?
        .ok_or("Run missing after reopen")?;
    assert_eq!(run, completed.run);
    assert_eq!(run.state(), AgentControllerState::Execute);
    assert_eq!(run.usage(), AgentRunUsage::ZERO);
    let task = completed.goal.task_id();
    let goal = store
        .load_current_goal_contract(project, task)
        .await?
        .ok_or("Goal missing after reopen")?;
    assert_eq!(goal, completed.goal);
    let ledger = store
        .load_task_ledger(project, task)
        .await?
        .ok_or("Ledger missing after reopen")?;
    assert_eq!(ledger.version(), completed.ledger.version());
    assert_eq!(ledger.ledger(), completed.ledger.ledger());
    let step = ledger
        .ledger()
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("open step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    assert_eq!(step.definition().addressed_criteria().len(), 1);
    assert!(step.definition().acceptance_criteria().is_empty());
    let attempt = step.attempts().last().ok_or("active attempt missing")?;
    assert!(attempt.verification().is_none() && attempt.evidence_ids().is_empty());
    assert_eq!(
        store.load_edit_phase_usage(project, run.id()).await?,
        completed.usage
    );
    assert!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .is_empty()
    );
    let events = store
        .load_run_events(
            project,
            run.id(),
            Some(completed.sequence_before),
            RunEventPageLimit::new(8)?,
        )
        .await?;
    assert!(!events.has_more());
    match (&completed.caller, events.events()) {
        (None, []) => {}
        (Some(evidence), [event]) => {
            assert_eq!(event.kind(), RunEventKind::ToolAction);
            assert_eq!(event.subject(), Some(RunEventSubject::Tool(CALLER_TOOL)));
            assert_eq!(
                store
                    .load_agent_tool_evidence(project, run.id(), &[evidence.id()])
                    .await?,
                vec![evidence.clone()]
            );
        }
        _ => {
            return Err("unexpected tool, policy or verification event after Edit dispatch".into());
        }
    }
    let published = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("published source missing after reopen")?;
    let anchor = EditPhaseState::new(project, &goal, &ledger, &run, &published)?.anchor();
    let durable = store.load_source_read_state(project, anchor).await?;
    match (
        &completed.caller,
        completed.caller_delivered,
        durable.progress().receipts(),
    ) {
        (Some(evidence), true, [receipt]) => {
            let delivery = receipt
                .delivery()
                .ok_or("accepted caller wire missing after reopen")?;
            assert_eq!(delivery.source().revision(), evidence.location().revision());
            assert_eq!(Some(delivery.source().range()), evidence.location().range());
            assert!(durable.progress().pending().is_none());
            assert!(matches!(
                receipt.outcome(),
                SourceDeliveryOutcome::NewBytes(_)
            ));
        }
        (Some(_), false, []) => {
            assert!(durable.progress().pending().is_some() && durable.acquired().is_some());
        }
        (None, false, []) => assert!(durable.progress().pending().is_none()),
        _ => return Err("unexpected durable source acquisition state".into()),
    }
    let database = fixture.candidate_database().await?;
    let connection = database.connection_for_operation().await?;
    let expected_reads = if completed.caller.is_some() { 1 } else { 0 };
    assert_eq!(
        count(&connection, "SELECT count(*) FROM tool_run_attempts").await?,
        expected_reads
    );
    assert_eq!(
        count(
            &connection,
            "SELECT count(*) FROM tool_run_attempts WHERE status = 'succeeded'",
        )
        .await?,
        expected_reads
    );
    assert_eq!(
        count(&connection, "SELECT count(*) FROM policy_decisions").await?,
        0
    );
    assert_eq!(
        count(&connection, "SELECT count(*) FROM task_step_verifications").await?,
        0
    );
    Ok(())
}
