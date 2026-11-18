//! Explicit local Plan measurement and optional current-source coding; no generated code execution.
use super::*;
use futures::StreamExt;
use std::io::Write;

#[derive(Default)]
pub(super) struct ReplyTrace {
    pub(super) text: String,
    pub(super) bytes: usize,
    pub(super) hash: blake3::Hasher,
    pub(super) truncated: bool,
    pub(super) completion: Option<ModelProviderCompletion>,
    pub(super) failure: Option<ModelProviderFailure>,
}
impl std::fmt::Debug for ReplyTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplyTrace")
            .field("bytes", &self.bytes)
            .field("truncated", &self.truncated)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub(super) struct LiveProvider<'a> {
    pub(super) inner: &'a dyn ModelProvider,
    pub(super) store: &'a LibsqlKnowledgeStore,
    pub(super) project: &'a ProjectIdentity,
    pub(super) run: AgentRunId,
    pub(super) root: &'a Path,
    pub(super) phase_label: &'static str,
    pub(super) calls: AtomicUsize,
    pub(super) replies: Mutex<Vec<Arc<Mutex<ReplyTrace>>>>,
}
// Test-private measurement boundary; it does not wait for or drive provider work.
struct LiveIoWindow<'a> {
    root: &'a Path,
    phase: &'static str,
    call: usize,
    open: serde_json::Value,
    closed: bool,
}
fn io_clock_millis() -> Result<u64, ModelProviderFailure> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ModelProviderFailure::Unavailable)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| ModelProviderFailure::Unavailable)
}
fn write_io_receipt(root: &Path, name: &str, value: &serde_json::Value) -> TestResult {
    // A final receipt is published without replacement only after complete sync.
    // A leftover pending link is an explicit measurement failure for the launcher.
    let pending = root.join(format!("{name}.pending"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    fs::hard_link(&pending, root.join(name))?;
    fs::remove_file(pending)?;
    Ok(())
}
impl<'a> LiveIoWindow<'a> {
    fn open(
        root: &'a Path,
        phase: &'static str,
        call: usize,
        request: &ModelProviderRequest,
        projection: &ModelWireProjection,
    ) -> Result<Self, ModelProviderFailure> {
        let open = json!({"schema":"a3.plan22.live-provider-io-open.v1", "phase":phase,
            "call":call,"model":request.profile().model_id().as_str(),
            "dispatch_wire_digest":format!("{:?}",projection.digest()),
            "context":projection.context_tokens(),"output":projection.output_tokens(),
            "opened_utc_unix_ms":io_clock_millis()?});
        write_io_receipt(root, &format!("{phase}-io-{call}-open.json"), &open)
            .map_err(|_| ModelProviderFailure::Unavailable)?;
        Ok(Self {
            root,
            phase,
            call,
            open,
            closed: false,
        })
    }
    fn close(&mut self, reason: &'static str) -> Result<(), ModelProviderFailure> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let close = json!({"schema":"a3.plan22.live-provider-io-close.v1", "open":self.open,
            "closed_utc_unix_ms":io_clock_millis()?,"reason":reason});
        write_io_receipt(
            self.root,
            &format!("{}-io-{}-close.json", self.phase, self.call),
            &close,
        )
        .map_err(|_| ModelProviderFailure::Unavailable)
    }
}
impl Drop for LiveIoWindow<'_> {
    fn drop(&mut self) {
        // No panic or retry; failed/missing closure cannot establish runtime evidence.
        let _ = self.close("stream_dropped");
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
                .store
                .load_edit_phase_usage(self.project, self.run)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1
                || usage.unknown_calls != 0
                || self.project_wire_request(request)? != projection
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call > 1 {
                return Err(ModelProviderFailure::Rejected);
            }
            let trace = Arc::new(Mutex::new(ReplyTrace::default()));
            self.replies
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?
                .push(trace.clone());
            write_once(self.root,&format!("{}-dispatch-{call}.json",self.phase_label),json!({"schema":"a3.plan22.http-correction-dispatch.v4","protocol_version":match self.phase_label {"plan"=>4,"context"=>1,"edit"|"followup-edit"=>3,"no-change-plan"=>6,_=>return Err(ModelProviderFailure::Rejected)},"model":request.profile().model_id().as_str(),"messages":request.messages().iter().map(|m|json!({"role":format!("{:?}",m.role()),"content":m.content()})).collect::<Vec<_>>(),"format":request.structured_output().ok_or(ModelProviderFailure::Rejected)?.value(),"wire_bytes":projection.request_bytes(),"wire_digest":format!("{:?}",projection.digest()),"context":projection.context_tokens(),"output":projection.output_tokens(),"thinking":format!("{:?}",projection.thinking()),"before_io":usage_json(usage)})).map_err(|_|ModelProviderFailure::Unavailable)?;
            let mut io =
                LiveIoWindow::open(self.root, self.phase_label, call, request, &projection)?;
            let stream = match self
                .inner
                .stream_projected(request, projection, timeout, control)
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    let error = if io.close("provider_error").is_err() {
                        ModelProviderFailure::Unavailable
                    } else {
                        error
                    };
                    if let Ok(mut record) = trace.lock() {
                        record.failure = Some(error);
                    }
                    return Err(error);
                }
            };
            let retain_plan_text = matches!(self.phase_label, "plan" | "context");
            Ok(Box::pin(stream.map(move |event| {
                let close = match &event {
                    Ok(ProviderEvent::Completed(_)) => io.close("completed"),
                    Err(_) => io.close("stream_error"),
                    _ => Ok(()),
                };
                let event = if close.is_err() {
                    Err(ModelProviderFailure::Unavailable)
                } else {
                    event
                };
                if let Ok(mut record) = trace.lock() {
                    match &event {
                        Ok(ProviderEvent::OutputText(text)) => {
                            record.bytes = record.bytes.saturating_add(text.as_str().len());
                            record.hash.update(text.as_str().as_bytes());
                            if retain_plan_text && record.text.len() + text.as_str().len() <= 1024 {
                                record.text.push_str(text.as_str());
                            } else if retain_plan_text {
                                record.truncated = true;
                            }
                        }
                        Ok(ProviderEvent::Completed(end)) => record.completion = Some(*end),
                        Err(error) => record.failure = Some(*error),
                    }
                }
                event
            })) as ProviderEventStream<'a>)
        })
    }
}
fn write_once(root: &Path, name: &str, value: serde_json::Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))?;
    serde_json::to_writer_pretty(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
fn reply_text_is_safe(phase: &PreparedHttpCorrectionPlan, text: &str, truncated: bool) -> bool {
    !truncated && phase.decode_proposal(text.as_bytes()).is_ok()
}

fn usage_json(u: EditPhaseReservedUsage) -> serde_json::Value {
    json!({"calls":u.calls,"repairs":u.repairs,"prompt":u.prompt_tokens,"output":u.output_tokens,"unobserved":u.unobserved_calls,"unknown":u.unknown_calls})
}

fn decision_json(decision: HttpCorrectionPlanDecision) -> serde_json::Value {
    match decision {
        HttpCorrectionPlanDecision::FocusLine(line) => json!({"kind":"focus","line":line.get()}),
        HttpCorrectionPlanDecision::NeedEvidence => json!({"kind":"needEvidence"}),
        HttpCorrectionPlanDecision::NeedOfferedSource(offer) => {
            json!({"kind":"needOfferedSource","offer":{"alias":"S1","kind":"readOnlyCurrentScriptContext","path":String::from_utf8_lossy(offer.revision().path().as_bytes()),"source_hash":format!("{:?}",offer.revision().content_hash()),"range":{"start_byte":offer.range().start_byte(),"end_byte":offer.range().end_byte()},"first_line":offer.inspection().start_line().get(),"line_count":offer.inspection().line_count().get()},"read_executed":false})
        }
        HttpCorrectionPlanDecision::TooLarge => json!({"kind":"tooLarge"}),
        HttpCorrectionPlanDecision::Refuse => json!({"kind":"refuse"}),
    }
}

#[test]
fn http_correction_plan_live_report_uses_structured_focus_and_halt_decisions() -> TestResult {
    assert_eq!(
        decision_json(HttpCorrectionPlanDecision::FocusLine(
            AgentFileStartLine::new(9)?
        )),
        json!({"kind":"focus","line":9})
    );
    for (decision, kind) in [
        (HttpCorrectionPlanDecision::NeedEvidence, "needEvidence"),
        (HttpCorrectionPlanDecision::TooLarge, "tooLarge"),
        (HttpCorrectionPlanDecision::Refuse, "refuse"),
    ] {
        assert_eq!(decision_json(decision), json!({"kind":kind}));
    }
    Ok(())
}

#[test]
#[ignore = "explicit local Ollama and retained isolated Plan-only output required"]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_plan_local_ollama_only_retains_proposal_after_physical_mismatch() -> TestResult {
    run_local_http(false)
}

#[test]
#[ignore = "explicit local HTTP Plan-to-Edit component; requires new retained output and code action"]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_edit_local_ollama_applies_current_source_after_physical_mismatch() -> TestResult
{
    run_local_http(true)
}

fn run_local_http(code_entry: bool) -> TestResult {
    crate::run_native_libsql_live_test(async {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let tag = std::env::var("A3_HTTP_CORRECTION_MODEL")?;
        let apply_plan = match (code_entry,std::env::var("A3_HTTP_CORRECTION_PLAN_ACTION").as_deref()) {
            (false,Err(std::env::VarError::NotPresent) | Ok("retain")) => false,
            (false,Ok("apply")) | (true,Ok("code")) => true,
            _ => return Err("unsupported HTTP plan measurement action".into()),
        };
        if !["qwen38-4k:latest", "ornith-1.5:9b"].contains(&tag.as_str()) {
            return Err("explicit supported Plan-only measurement tag required".into());
        }
        let root = PathBuf::from(
            std::env::var_os("A3_HTTP_CORRECTION_OUTPUT")
                .ok_or("create-new output directory required")?,
        );
        let allowed = fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plan22-p7-runs"),
        )?;
        if !root.is_absolute()
            || root.file_name().is_none()
            || fs::canonicalize(root.parent().ok_or("parent missing")?)? != allowed
        {
            return Err(
                "measurement output must be a new direct child of target/plan22-p7-runs".into(),
            );
        }
        fs::create_dir(&root)?;
        fs::create_dir(root.join("worktree"))?;
        initialize_git(&root.join("worktree"))?;
        let fixture = Fixture {
            layout: StorageLayout::prepare(root.join("app-data"))?,
            project: RepositoryInspector::new().inspect(root.join("worktree"))?,
            root: root.clone(),
        };
        let provider = OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?;
        let context_setting = match std::env::var("A3_HTTP_CORRECTION_CONTEXT") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.into()),
        };
        let context = HttpFixtureContext::from_setting(context_setting.as_deref())?;
        let request = ModelCapabilityProbeRequest::new(
            ModelId::try_from_string(tag.clone())?,
            ollama_profile_with_context(&tag, context)?.settings().clone(),
        );
        let probed = ProbeModelProfile::new(&provider)
            .execute(
                &request,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await;
        write_once(
            &root,
            "probe.json",
            json!({"tag":tag,"context_tokens":context.tokens(),"output_tokens":1024,"verified":probed.as_ref().is_ok_and(|p|p.executable_actions_enabled()),"error":probed.as_ref().err().map(|e|format!("{e:?}"))}),
        )?;
        let f = full_fixture_using(probed?, true, fixture).await?;
        let project = &f.http.fixture.project;
        let observed_failure = f
            .http
            .store
            .load_loopback_mismatch(project, f.run.id(), TOOL, AgentToolAttemptNumber::FIRST)
            .await?
            .ok_or("physical diagnostic missing")?;
        write_once(
            &root,
            "diagnostic.json",
            json!({"scope":"fresh physical mismatch; test-only existing Replan signal; no automatic admission or full A04 claim","digest":format!("{:?}",observed_failure.digest()),"source":format!("{:?}",observed_failure.script().content_hash()),"phase":format!("{:?}",observed_failure.mismatch().phase()),"check":observed_failure.mismatch().check_index(),"status":observed_failure.mismatch().observation().status(),"body_bytes":observed_failure.mismatch().observation().body_bytes(),"body_hash":format!("{:?}",observed_failure.mismatch().observation().body_hash()),"run":format!("{:?}",f.run.id()),"deadline":f.run.created_at().unix_millis()+f.run.budget().duration_limit().millis()}),
        )?;
        let observed = LiveProvider {
            inner: &provider,
            store: &f.http.store,
            project,
            run: f.run.id(),
            root: &root,
            phase_label: "plan",
            calls: AtomicUsize::new(0),
            replies: Mutex::new(Vec::new()),
        };
        let started = Instant::now();
        let base = f.run.updated_at().unix_millis() + 1;
        let first = ExecuteHttpCorrectionPlan::new(
            &observed,
            &WorkspaceAgentSourceReader,
            f.http.store.as_ref(),
        )
        .execute(
            project,
            &f.phase,
            EditPhaseCallKind::Initial,
            at(base)?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await;
        let result = if matches!(first, Err(HttpCorrectionPlanExecutionFailure::InvalidReply)) {
            ExecuteHttpCorrectionPlan::new(
                &observed,
                &WorkspaceAgentSourceReader,
                f.http.store.as_ref(),
            )
            .execute(
                project,
                &f.phase,
                EditPhaseCallKind::StructuralRepair,
                at(base + u64::try_from(started.elapsed().as_millis())?)?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await
        } else {
            first
        };
        let usage = f
            .http
            .store
            .load_edit_phase_usage(project, f.run.id())
            .await?;
        {
            let replies = observed.replies.lock().map_err(|_| "trace lock")?;
            for (index, reply) in replies.iter().enumerate() {
                let reply = reply.lock().map_err(|_| "reply lock")?;
                let retain_text=reply_text_is_safe(&f.phase,&reply.text,reply.truncated);
                write_once(
                    &root,
                    &format!("plan-reply-{index}.json"),
                    json!({"text":retain_text.then_some(&reply.text),"text_rejected":!retain_text,"captured_text_bytes":reply.text.len(),"captured_text_hash":format!("{:?}",hash(reply.text.as_bytes())),"truncated":reply.truncated,"finish":reply.completion.map(|c|format!("{:?}",c.reason())),"prompt":reply.completion.and_then(|c|c.usage().prompt_tokens()),"output":reply.completion.and_then(|c|c.usage().output_tokens()),"failure":reply.failure.map(|e|format!("{e:?}"))}),
                )?;
            }
        }
        drop(observed);
        assert_eq!(
            f.http.store.load_agent_run(project, f.run.id()).await?,
            Some(f.run.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored.clone())
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        let cold = reopened
            .load_http_correction_plan(&f.http.fixture.project, &f.phase)
            .await;
        write_once(
            &root,
            "plan-final.json",
            json!({"scope":"Plan-only isolated current-run reservation/proposal; no execution authority; no automatic mismatch admission; not full A04/P7","protocol_version":4,"model":tag,"elapsed_ms":started.elapsed().as_millis(),"execution_error":result.as_ref().err().map(|e|format!("{e:?}")),"accepted_decision":result.as_ref().ok().map(|r|decision_json(r.proposal.decision())),"accepted_intent":result.as_ref().ok().map(|r|r.proposal.intent().as_str()),"cold_decision":cold.as_ref().ok().and_then(|p|p.as_ref()).map(|p|decision_json(p.decision())),"cold_intent":cold.as_ref().ok().and_then(|p|p.as_ref()).map(|p|p.intent().as_str()),"intent_verified":false,"read_executed":false,"cold_error":cold.as_ref().err().map(|e|format!("{e:?}")),"usage":usage_json(usage),"run_state":"Replan","ledger_unchanged":true,"model_code_executed":false,"http_reexecuted":false}),
        )?;
        let cold = cold?;
        let report = result?;
        assert_eq!(cold, Some(report.proposal));
        drop(reopened);
        if apply_plan {
                let store = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
                let current = store.latest_published_index(project, &ActiveControl).await?.ok_or("current index missing")?;
                let mut run = f.run.clone();
                let (mut ledger, version) = f.stored.clone().into_parts();
                let before = inventory(&f.http.root)?;
                let charged = store.load_edit_phase_usage(project, run.id()).await?;
                let applied = ApplyHttpCorrectionPlan::new(&store, &store, &WorkspaceAgentSourceReader)
                    .execute(project, &f.http.goal, &mut run, &mut ledger, version, &current,
                        &f.phase, at(base + u64::try_from(started.elapsed().as_millis())?)?, &ActiveControl).await?;
                assert_eq!(run.state(), AgentControllerState::Replan);
                assert_eq!(run.budget(), f.run.budget());
                assert_eq!(run.usage(), f.run.usage());
                assert_eq!(store.load_edit_phase_usage(project, run.id()).await?, charged);
                assert_eq!(inventory(&f.http.root)?, before);
                let active = ledger.steps().filter(|step| step.is_active_plan_step()).collect::<Vec<_>>();
                assert_eq!(active.len(), 2);
                assert!(active.iter().all(|step| step.attempts().is_empty()));
                drop(store);
                let cold_store = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
                assert_eq!(cold_store.load_agent_run(project, run.id()).await?, Some(run.clone()));
                let cold_ledger = cold_store.load_task_ledger(project, f.http.goal.task_id()).await?.ok_or("cold applied ledger missing")?;
                assert_eq!(cold_ledger.ledger(), &ledger);
                assert_eq!(cold_ledger.version(), applied.version);
                let cold_goal = cold_store.load_current_goal_contract(project, f.http.goal.task_id()).await?.ok_or("cold original missing")?;
                assert_eq!(cold_goal.original(), f.http.goal.original());
                assert_eq!(inventory(&f.http.root)?, before);
                write_once(&root, "applied-plan-final.json", json!({
                    "scope":"Material plan only; no activation or automatically resumable correction origin",
                    "plan_applied":true,"cold_equal":true,"original_unchanged":true,
                    "budget_and_usage_unchanged":true,"source_unchanged":true,
                    "source_revalidated":true,"source_acquisition_executed":false,
                    "active_steps":active.len(),"attempts_started":0,"run_state":"Replan",
                    "correction":format!("{:?}",applied.correction),"proof":format!("{:?}",applied.proof),
                    "http_reexecuted":false,"code_corrected":false,"a04_completed":false,"p7_completed":false
                }))?;
                if code_entry {
                    // The initial cold Plan assertion already dropped all three owners.
                    // Rebuild exactly those moved fields before passing an owned fixture onward.
                    let store=Arc::new(cold_store);
                    let refresh=RefreshRepositoryIndex::new(Arc::new(Blake3RepositorySnapshotBuilder::new()),store.clone(),Arc::new(Blake3IndexRunIdFactory));
                    let compiler=BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
                    let http=PreparedHttpFixture{store,refresh,compiler,..f.http};
                    let applied=CurrentPlanFixture{http,phase:f.phase,run,stored:cold_ledger};
                    return run_code(&provider, &root, &tag, edit::cold(applied).await?, &observed_failure).await;
                }
        }
        // Explicitly retained isolated fixture and bounded reports; no cleanup of user output.
        Ok(())
        })
    })
}

async fn run_code(
    provider: &OllamaModelProvider,
    root: &Path,
    tag: &str,
    mut f: edit::HttpEditFixture,
    observed_failure: &LoopbackHttpMismatchDiagnostic,
) -> TestResult {
    let before = inventory(&f.http.root)?;
    let goal = f.http.goal.clone();
    let run_id = f.run.id();
    let (phase, mut compiled, page, _) = edit::acquire(&mut f).await?;
    let VerificationTarget::LoopbackHttp(recipe) = f.http.spec.target() else {
        return Err("original HTTP recipe missing".into());
    };
    edit::assert_fixture_diagnostic(&compiled, observed_failure, recipe)?;
    let correction = phase.anchor().step();
    let range = page.range();
    let original_source = fs::read(f.http.root.join("server.py"))?;
    let structural = compiled.for_structural_repair()?;
    let truncated = compiled.for_truncated_output_repair()?;
    let mut wires = Vec::new();
    for (kind, request) in [
        ("initial", &compiled),
        ("structuralRepair", &structural),
        ("truncatedRepair", &truncated),
    ] {
        let projection = provider.project_wire_request(request.request())?;
        let budget = PhaseWireBudget::fit(projection, request.request().profile())?;
        wires.push(json!({"kind":kind,"bytes":projection.request_bytes(),"digest":format!("{:?}",projection.digest()),
            "context":projection.context_tokens(),"output":projection.output_tokens(),"prompt_reserve":budget.prompt_tokens(),
            "output_reserve":budget.output_tokens()}));
    }
    write_once(
        root,
        "coding-context.json",
        json!({"scope":"Current applied HTTP origin to real target Edit; no HTTP re-execution or full A04",
        "original_bytes":goal.original().ok_or("original missing")?.as_bytes().len(),
        "source_hash":format!("{:?}",page.revision().content_hash()),"range":[range.start_byte(),range.end_byte()],
        "source_bytes":page.text().len(),"run":format!("{:?}",run_id),"run_bytes":run_id.as_bytes(),"correction":format!("{:?}",correction),
        "old_prepared_plan_dropped":true,"active_criterion_and_all_constraints_checked":true,
        "context_tokens":compiled.request().profile().settings().context_limit().get(),
        "all_three_actual_profile_wires_fit":true,
        "all_three_actual_four_k_wires_fit":compiled.request().profile().settings().context_limit().get()==4096,"wires":wires}),
    )?;
    let observed = LiveProvider {
        inner: provider,
        store: f.http.store.as_ref(),
        project: &f.http.fixture.project,
        run: run_id,
        root,
        phase_label: "edit",
        calls: AtomicUsize::new(0),
        replies: Mutex::new(Vec::new()),
    };
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("publication missing")?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let started = Instant::now();
    let first = ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                edit::state(&f, &published)?,
                edit::after_phases(&f).await?,
                &patch,
                &ActiveControl,
            ))
            .await?,
            edit::state(&f, &published)?,
            &compiled,
            edit::after_phases(&f).await?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await;
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
        ExecuteEditPhase::new(&observed, f.http.store.as_ref(), &patch)
            .execute(
                Box::pin(
                    crate::plan22_goal_contract_tests::prepare_indexed_bound_edit(
                        &f.http.compiler,
                        PreparedEditPhase::update(edit::state(&f, &published)?, &page)?,
                        &repair,
                        edit::state(&f, &published)?,
                        edit::after_phases(&f).await?,
                        &patch,
                        &ActiveControl,
                    ),
                )
                .await?,
                edit::state(&f, &published)?,
                &repair,
                edit::after_phases(&f).await?,
                ModelRequestTimeout::from_millis(120000)?,
                &ActiveControl,
            )
            .await
    } else {
        first
    };
    let edit_calls = observed.calls.load(Ordering::SeqCst);
    let usage = f
        .http
        .store
        .load_edit_phase_usage(&f.http.fixture.project, run_id)
        .await?;
    write_once(
        root,
        "edit-result.json",
        json!({"outcome":result.as_ref().ok().map(|r|match &r.outcome {
        EditPhaseOutcome::Proposal(_)=>"proposal",EditPhaseOutcome::NeedEvidence(_)=>"needEvidence",EditPhaseOutcome::TooLarge=>"tooLarge"}),
        "error":result.as_ref().err().map(|e|format!("{e:?}")),"source_delivery":result.as_ref().is_ok_and(|r|r.source_delivery.is_some()),
        "edit_calls":edit_calls,"semantic_retries":0,"elapsed_ms":started.elapsed().as_millis(),"usage":usage_json(usage)}),
    )?;
    for (n, trace) in observed
        .replies
        .lock()
        .map_err(|_| "trace lock")?
        .iter()
        .enumerate()
    {
        let trace = trace.lock().map_err(|_| "trace lock")?;
        write_once(
            root,
            &format!("edit-reply-{n}.json"),
            json!({"text":null,"raw_text_retained":false,"bytes":trace.bytes,
            "blake3":trace.hash.finalize().to_hex().to_string(),"finish":trace.completion.map(|c|format!("{:?}",c.reason())),
            "prompt":trace.completion.and_then(|c|c.usage().prompt_tokens()),"output":trace.completion.and_then(|c|c.usage().output_tokens()),
            "failure":trace.failure.map(|e|format!("{e:?}"))}),
        )?;
    }
    drop(observed);
    let report = result?;
    let delivery = report
        .source_delivery
        .ok_or("accepted HTTP Edit lacks target delivery")?;
    let EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("legal non-coding halt; no semantic retry".into());
    };
    let f = edit::reopen(f).await?;
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold publication missing")?;
    let reads = f
        .http
        .store
        .load_source_read_state(
            &f.http.fixture.project,
            edit::state(&f, &published)?.anchor(),
        )
        .await?;
    let [receipt] = reads.progress().receipts() else {
        return Err("cold HTTP target delivery missing".into());
    };
    if receipt.delivery() != Some(&delivery) {
        return Err("cold target delivery differs from actual accepted wire".into());
    }
    assert!(reads.progress().pending().is_none() && reads.acquired().is_none());
    let fixture = edit::patch(f, action).await?;
    let current = fs::read(root.join("worktree/server.py"))?;
    let after = inventory(&root.join("worktree"))?;
    let protected = before.len() == after.len()
        && before
            .iter()
            .zip(&after)
            .all(|((p, b), (q, a))| p == q && (p == Path::new("server.py") || b == a));
    let prefix = &original_source[..usize::try_from(range.start_byte())?];
    let suffix = &original_source[usize::try_from(range.end_byte())?..];
    let outside = current.starts_with(prefix)
        && current.ends_with(suffix)
        && current.len() >= prefix.len() + suffix.len();
    assert!(protected && outside && current != original_source);
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let cold_goal = cold
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold goal missing")?;
    let ledger = cold
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold ledger missing")?;
    let run = cold
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("cold run missing")?;
    let step = ledger
        .ledger()
        .step(correction)
        .ok_or("cold correction missing")?;
    let local = step.is_active_plan_step()
        && step.status() == TaskStepStatus::Completed
        && step.definition().acceptance_criteria().is_empty();
    let open = ledger
        .ledger()
        .steps()
        .filter(|s| s.is_active_plan_step())
        .any(|s| {
            matches!(
                s.definition().verification_spec().target(),
                VerificationTarget::LoopbackHttp(_)
            ) && s.status() != TaskStepStatus::Completed
                && s.attempts().is_empty()
        });
    assert!(cold_goal == goal && local && open && run.state() == AgentControllerState::Verify);
    write_once(
        root,
        "coding-final.json",
        json!({"scope":"Actual local HTTP focus to applied source coding component; original full HTTP verification remains open",
        "model":tag,"actual_model_edit":true,"edit_calls":edit_calls,"semantic_retries":0,"cold_source_delivery":true,
        "normal_approved_patch":true,"cold_current_diff":true,"outside_window_preserved":outside,"protected_files_unchanged":protected,
        "source_changed":current!=original_source,"source_before_blake3":blake3::hash(&original_source).to_hex().to_string(),
        "source_after_blake3":blake3::hash(&current).to_hex().to_string(),"original_unchanged":cold_goal==goal,
        "run_state":format!("{:?}",run.state()),"completed_addresses_without_proves":local,"original_http_proof_open":open,
        "goal_done":false,"generated_code_executed":false,"http_reexecuted":false,"a04_completed":false,"p7_completed":false,
        "semantic_code_quality":"Requires independent source review and the separate original HTTP proof","usage":usage_json(usage)}),
    )?;
    drop(cold);
    Ok(())
}

#[test]
fn http_correction_plan_live_retention_is_strict_and_secret_safe() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let good = String::from_utf8(reply(&f.phase, "needEvidence")?)?;
        assert!(reply_text_is_safe(&f.phase, &good, false));
        assert!(!reply_text_is_safe(&f.phase, &good, true));
        let duplicate = good.replacen('{', "{\"intent\":\"password=fixture-secret-value\",", 1);
        let mut unsafe_value: serde_json::Value = serde_json::from_str(&good)?;
        unsafe_value["intent"] = json!("password=fixture-secret-value");
        let escaped = unsafe_value.to_string().replace('=', "\\u003d");
        let malformed = "{\"intent\":\"password=fixture-secret-value\"";
        for raw in [malformed, &duplicate, &escaped, &good[..good.len() - 1]] {
            assert!(!reply_text_is_safe(&f.phase, raw, false));
        }
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[cfg(test)]
mod io_receipt_tests {
    use super::*;

    fn request_and_projection() -> TestResult<(ModelProviderRequest, ModelWireProjection)> {
        let request = ModelProviderRequest::new(
            ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::SixteenK)?,
            vec![ModelMessage::try_from_string(
                ModelMessageRole::User,
                "Native IO receipt lifecycle fixture; no provider is invoked.".to_owned(),
            )?],
            None,
        )?;
        let projection = ModelWireProjection::from_serialized(
            br#"{"fixture":"native-io-receipt"}"#,
            0,
            0,
            16384,
            1024,
            ModelWireThinking::Disabled,
        )?;
        Ok((request, projection))
    }

    fn read_receipt(path: &Path) -> TestResult<serde_json::Value> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    #[test]
    fn http_same_run_io_receipt_open_completed_publishes_exact_bytes_without_pending_or_reclose()
    -> TestResult {
        let fixture = Fixture::new()?;
        let root = &fixture.root;
        let (request, projection) = request_and_projection()?;
        let mut io = LiveIoWindow::open(root, "plan", 0, &request, &projection)?;
        let opened = read_receipt(&root.join("plan-io-0-open.json"))?;
        assert_eq!(opened["schema"], "a3.plan22.live-provider-io-open.v1");
        assert_eq!(opened["phase"], "plan");
        assert_eq!(opened["call"], 0);
        assert_eq!(opened["model"], request.profile().model_id().as_str());
        assert_eq!(
            opened["dispatch_wire_digest"],
            format!("{:?}", projection.digest())
        );
        assert_eq!(opened["context"], 16384);
        assert_eq!(opened["output"], 1024);
        assert!(!root.join("plan-io-0-open.json.pending").exists());
        assert!(!root.join("plan-io-0-close.json").exists());
        let open_bytes = fs::read(root.join("plan-io-0-open.json"))?;
        io.close("completed")?;
        let closed = read_receipt(&root.join("plan-io-0-close.json"))?;
        assert_eq!(closed["schema"], "a3.plan22.live-provider-io-close.v1");
        assert_eq!(closed["open"], opened);
        assert_eq!(closed["reason"], "completed");
        let opened_at = opened["opened_utc_unix_ms"]
            .as_u64()
            .ok_or("missing open time")?;
        let closed_at = closed["closed_utc_unix_ms"]
            .as_u64()
            .ok_or("missing close time")?;
        assert!(opened_at > 0 && closed_at >= opened_at);
        assert!(!root.join("plan-io-0-close.json.pending").exists());
        let close_bytes = fs::read(root.join("plan-io-0-close.json"))?;
        io.close("stream_error")?;
        drop(io);
        assert_eq!(fs::read(root.join("plan-io-0-open.json"))?, open_bytes);
        assert_eq!(fs::read(root.join("plan-io-0-close.json"))?, close_bytes);
        fixture.cleanup()
    }

    #[test]
    fn http_same_run_io_receipt_actual_drop_closes_once_and_preserves_an_existing_final()
    -> TestResult {
        let fixture = Fixture::new()?;
        let root = &fixture.root;
        let (request, projection) = request_and_projection()?;
        {
            let _io = LiveIoWindow::open(root, "edit", 0, &request, &projection)?;
            assert!(!root.join("edit-io-0-close.json").exists());
        }
        let closed = read_receipt(&root.join("edit-io-0-close.json"))?;
        assert_eq!(
            closed["open"],
            read_receipt(&root.join("edit-io-0-open.json"))?
        );
        assert_eq!(closed["reason"], "stream_dropped");
        assert!(!root.join("edit-io-0-close.json.pending").exists());
        let occupied = root.join("edit-io-1-close.json");
        let io = LiveIoWindow::open(root, "edit", 1, &request, &projection)?;
        fs::write(&occupied, b"earlier-final-must-survive")?;
        drop(io);
        assert_eq!(fs::read(&occupied)?, b"earlier-final-must-survive");
        // Failed publication leaves a pending receipt, never a replacement final.
        let pending = root.join("edit-io-1-close.json.pending");
        assert_eq!(read_receipt(&pending)?["reason"], "stream_dropped");
        assert_eq!(fs::read(&occupied)?, b"earlier-final-must-survive");
        fixture.cleanup()
    }

    #[test]
    fn http_same_run_io_receipt_existing_pending_final_and_real_write_errors_fail_closed()
    -> TestResult {
        let fixture = Fixture::new()?;
        let root = &fixture.root;
        let (request, projection) = request_and_projection()?;
        let final_path = root.join("plan-io-0-open.json");
        fs::write(&final_path, b"earlier-open-final")?;
        assert!(matches!(
            LiveIoWindow::open(root, "plan", 0, &request, &projection),
            Err(ModelProviderFailure::Unavailable)
        ));
        assert_eq!(fs::read(&final_path)?, b"earlier-open-final");
        assert!(root.join("plan-io-0-open.json.pending").is_file());
        assert!(!root.join("plan-io-0-close.json").exists());
        let open_pending = root.join("context-io-0-open.json.pending");
        fs::write(&open_pending, b"earlier-open-pending")?;
        assert!(matches!(
            LiveIoWindow::open(root, "context", 0, &request, &projection),
            Err(ModelProviderFailure::Unavailable)
        ));
        assert_eq!(fs::read(open_pending)?, b"earlier-open-pending");
        assert!(!root.join("context-io-0-open.json").exists());
        let mut io = LiveIoWindow::open(root, "followup-edit", 0, &request, &projection)?;
        let close_pending = root.join("followup-edit-io-0-close.json.pending");
        fs::write(&close_pending, b"earlier-close-pending")?;
        assert_eq!(
            io.close("completed"),
            Err(ModelProviderFailure::Unavailable)
        );
        assert!(io.closed);
        drop(io);
        assert_eq!(fs::read(close_pending)?, b"earlier-close-pending");
        assert!(!root.join("followup-edit-io-0-close.json").exists());
        let blocked_root = root.join("not-a-directory");
        fs::write(&blocked_root, b"owned-file-preserved")?;
        assert!(matches!(
            LiveIoWindow::open(&blocked_root, "plan", 0, &request, &projection),
            Err(ModelProviderFailure::Unavailable)
        ));
        assert_eq!(fs::read(blocked_root)?, b"owned-file-preserved");
        fixture.cleanup()
    }
}
