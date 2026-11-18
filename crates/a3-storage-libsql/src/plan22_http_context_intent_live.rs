//! Explicit local ContextIntent component measurement; the V4 S1 parent is controlled fixture data.
use super::*;
use futures::StreamExt;
use std::io::Write;

#[derive(Default)]
struct ContextTrace {
    text: String,
    bytes: usize,
    hash: blake3::Hasher,
    truncated: bool,
    completion: Option<ModelProviderCompletion>,
    failure: Option<ModelProviderFailure>,
}
impl std::fmt::Debug for ContextTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextTrace")
            .field("bytes", &self.bytes)
            .field("truncated", &self.truncated)
            .finish_non_exhaustive()
    }
}
struct ContextLiveProvider<'a> {
    inner: &'a OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    run: AgentRunId,
    profile: ModelProfileReference,
    job: ContentHash,
    output: &'a Path,
    allowed_wires: &'a [ContentHash],
    calls: AtomicUsize,
    traces: Mutex<Vec<Arc<Mutex<ContextTrace>>>>,
}
impl std::fmt::Debug for ContextLiveProvider<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextLiveProvider")
            .field("job", &self.job)
            .finish_non_exhaustive()
    }
}
impl ModelProvider for ContextLiveProvider<'_> {
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
                || !self.allowed_wires.contains(&projection.digest())
                || request.profile().reference() != self.profile
                || projection.context_tokens() != 16384
                || projection.output_tokens() != 1024
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self
                .calls
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                    (n < 2).then_some(n + 1)
                })
                .map_err(|_| ModelProviderFailure::Rejected)?;
            let trace = Arc::new(Mutex::new(ContextTrace::default()));
            self.traces
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?
                .push(trace.clone());
            write_once(self.output,&format!("context-dispatch-{call}.json"),json!({
                "schema":"a3.plan22.http-context-dispatch.v1","protocol_version":1,
                "context_job":format!("{:?}",self.job),"call_index":call,
                "model":request.profile().model_id().as_str(),
                "profile":format!("{:?}",request.profile().reference()),
                "actual_encoded_wire":wire_json(projection),"before_io_shared_accounting":usage_json(usage),
                "controlled_parent_provider_calls":0,"execution_authority":false
            })).map_err(|_|ModelProviderFailure::Unavailable)?;
            let stream = match self
                .inner
                .stream_projected(request, projection, timeout, control)
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    if let Ok(mut t) = trace.lock() {
                        t.failure = Some(error)
                    };
                    return Err(error);
                }
            };
            Ok(Box::pin(stream.inspect(move |event| {
                if let Ok(mut t) = trace.lock() {
                    match event {
                        Ok(ProviderEvent::OutputText(chunk)) => {
                            t.bytes = t.bytes.saturating_add(chunk.as_str().len());
                            t.hash.update(chunk.as_str().as_bytes());
                            if !t.truncated
                                && t.text.len().saturating_add(chunk.as_str().len()) <= 1024
                            {
                                t.text.push_str(chunk.as_str());
                            } else {
                                t.truncated = true;
                            }
                        }
                        Ok(ProviderEvent::Completed(end)) => t.completion = Some(*end),
                        Err(error) => t.failure = Some(*error),
                    }
                }
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
fn usage_json(u: EditPhaseReservedUsage) -> serde_json::Value {
    json!({"calls":u.calls,"repairs":u.repairs,"prompt_charged":u.prompt_tokens,
        "output_charged":u.output_tokens,"unobserved":u.unobserved_calls,"unknown":u.unknown_calls})
}
fn wire_json(p: ModelWireProjection) -> serde_json::Value {
    json!({"digest":format!("{:?}",p.digest()),"request_bytes":p.request_bytes(),
        "message_bytes":p.message_bytes(),"format_bytes":p.format_bytes(),
        "context_tokens":p.context_tokens(),"output_tokens":p.output_tokens(),
        "thinking":format!("{:?}",p.thinking()),"encoded_body_retained":false})
}
fn source_json(page: &AgentSourcePage) -> serde_json::Value {
    let r = page.range();
    json!({"revision":format!("{:?}",page.revision().content_hash()),
        "range":{"start_byte":r.start_byte(),"end_byte":r.end_byte(),
            "start_row":r.start_position().row(),"start_column":r.start_position().column(),
            "end_row":r.end_position().row(),"end_column":r.end_position().column()},
        "utf8_bytes":page.text().len(),"text_digest":format!("{:?}",blake3::hash(page.text().as_bytes()))})
}
fn retain_wire(
    root: &Path,
    label: &str,
    request: &ModelProviderRequest,
    provider: &OllamaModelProvider,
    c: &ContextFixture,
) -> TestResult<ContentHash> {
    let actual = request
        .messages()
        .iter()
        .map(|m| m.content())
        .collect::<String>();
    if actual.len() > 32768
        || SecretCandidateClassifierV1::classify(&actual).is_some()
        || !actual.contains(c.primary.text())
        || !actual.contains(c.context.text())
    {
        return Err("only bounded complete seed-fixture request metadata may be retained".into());
    }
    for criterion in c.f.http.goal.draft().acceptance_criteria() {
        assert!(actual.contains(criterion.statement().as_str()));
    }
    for constraint in c.f.http.goal.draft().constraints() {
        assert!(actual.contains(constraint.as_str()));
    }
    let p = provider.project_wire_request(request)?;
    let budget = PhaseWireBudget::fit(p, request.profile())?;
    assert_eq!(
        (
            p.context_tokens(),
            p.output_tokens(),
            budget.output_tokens()
        ),
        (16384, 1024, 3605)
    );
    write_once(
        root,
        &format!("context-wire-{label}.json"),
        json!({
            "schema":"a3.plan22.http-context-wire.v1","protocol_version":1,"variant":label,
            "context_job":format!("{:?}",c.phase.job_digest()),"actual_encoded_wire":wire_json(p),
            "prompt_reserve":budget.prompt_tokens(),"output_reserve":budget.output_tokens(),
            "profile":format!("{:?}",request.profile().reference()),"model":request.profile().model_id().as_str(),
            "messages":request.messages().iter().map(|m|json!({"role":format!("{:?}",m.role()),"content":m.content()})).collect::<Vec<_>>(),
            "format":request.structured_output().ok_or("context schema missing")?.value(),
            "primary_repeat":source_json(&c.primary),"context_acquired_original":source_json(&c.context),
            "complete_request_metadata_retained":true,"preflight_only":true
        }),
    )?;
    Ok(p.digest())
}

#[test]
#[ignore = "explicit local Ollama, fresh retained output, and exact16k context selection required; Context component only"]
fn http_context_intent_local_ollama_retains_same_run_settlement_without_execution() -> TestResult {
    crate::run_native_libsql_live_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_local_context())
    })
}
async fn run_local_context() -> TestResult {
    let tag = std::env::var("A3_HTTP_CONTEXT_INTENT_MODEL")?;
    if !["qwen38-4k:latest", "ornith-1.5:9b"].contains(&tag.as_str()) {
        return Err("explicit supported Context component tag required".into());
    }
    if std::env::var("A3_HTTP_CONTEXT_INTENT_CONTEXT")?.as_str() != "16384" {
        return Err(
            "Context component requires explicit16384; tag names do not select context".into(),
        );
    }
    let output = PathBuf::from(
        std::env::var_os("A3_HTTP_CONTEXT_INTENT_OUTPUT")
            .ok_or("new retained Context component output required")?,
    );
    let allowed = fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plan22-p7-runs"),
    )?;
    if !output.is_absolute()
        || output.file_name().is_none()
        || fs::canonicalize(output.parent().ok_or("output parent missing")?)? != allowed
    {
        return Err("Context output must be a new direct child of target/plan22-p7-runs".into());
    }
    fs::create_dir(&output)?;
    fs::create_dir(output.join("worktree"))?;
    initialize_git(&output.join("worktree"))?;
    let fixture = Fixture {
        layout: StorageLayout::prepare(output.join("app-data"))?,
        project: RepositoryInspector::new().inspect(output.join("worktree"))?,
        root: output.clone(),
    };
    let provider = encoder()?;
    let probe = ModelCapabilityProbeRequest::new(
        ModelId::try_from_string(tag.clone())?,
        ollama_profile_with_context(&tag, HttpFixtureContext::SixteenK)?
            .settings()
            .clone(),
    );
    let actual = ProbeModelProfile::new(&provider)
        .execute(
            &probe,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await;
    write_once(
        &output,
        "probe.json",
        json!({"schema":"a3.plan22.http-context-probe.v1",
        "model":tag,"requested_context":16384,"requested_output":1024,
        "capability_probe_has_separate_io":true,"error":actual.as_ref().err().map(|e|format!("{e:?}")),
        "actual_profile":actual.as_ref().ok().map(|p|format!("{:?}",p.reference())),
        "actual_tool_mode":actual.as_ref().ok().map(|p|format!("{:?}",p.capabilities().tool_call_mode())),
        "verified":actual.as_ref().is_ok_and(|p|p.executable_actions_enabled())}),
    )?;
    let actual = actual?;
    assert_eq!(actual.settings().context_limit().get(), 16384);
    assert_eq!(actual.settings().output_limit().get(), 1024);
    let profile = actual.reference();
    // The only physical service execution is the known frozen pre-existing wrong seed.
    let f = full_fixture_using(actual, true, fixture).await?;
    assert_eq!(f.run.model_profile(), Some(profile));
    let diagnostic = f
        .http
        .store
        .load_loopback_mismatch(
            &f.http.fixture.project,
            f.run.id(),
            TOOL,
            AgentToolAttemptNumber::FIRST,
        )
        .await?
        .ok_or("physical mismatch missing")?;
    let f = preseed_parent_s1(f, EditPhaseCallKind::Initial).await?;
    let parent_usage = f
        .http
        .store
        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
        .await?;
    assert_eq!(
        (
            parent_usage.calls,
            parent_usage.repairs,
            parent_usage.unobserved_calls,
            parent_usage.unknown_calls
        ),
        (1, 0, 0, 0)
    );
    write_once(
        &output,
        "controlled-parent.json",
        json!({
        "schema":"a3.plan22.http-context-controlled-parent.v1","protocol_version":4,
        "origin":"controlled canonical S1 fixture observation; not an actual parent LLM call",
        "parent_provider_calls":0,"parent_job":format!("{:?}",f.phase.job_digest()),
        "fixture_reported_prompt_tokens":64,"fixture_reported_output_tokens":16,
        "shared_charged_usage_after_parent":usage_json(parent_usage),
        "physical_mismatch_digest":format!("{:?}",diagnostic.digest()),
        "physical_mismatch_source":format!("{:?}",diagnostic.script().content_hash()),
        "physical_mismatch_phase":format!("{:?}",diagnostic.mismatch().phase()),
        "failure_f":f.phase.anchor().sequence.get(),"model_profile":format!("{profile:?}"),
        "original_deadline":f.run.created_at().unix_millis()+f.run.budget().duration_limit().millis(),
        "parent_source_delivery_credit":false,"parent_model_measurement":false}),
    )?;
    let c = acquire_context_from(f).await?;
    assert_eq!(c.f.run.model_profile(), Some(profile));
    assert_metadata_only(&c).await?;
    write_once(
        &output,
        "read-origin.json",
        json!({
        "schema":"a3.plan22.http-context-read.v1","parent_failure_f":c.origin.parent_anchor.sequence.get(),
        "read_tail_r":c.origin.read_event.sequence().get(),"read_tool":format!("{:?}",c.origin.read_admission.attempt().tool_run_id()),
        "read_attempt":c.origin.read_admission.attempt().attempt().get(),
        "read_key":format!("{:?}",c.origin.read_admission.key()),"context_job":format!("{:?}",c.phase.job_digest()),
        "current_original":source_json(&c.context),"primary_repeat":source_json(&c.primary),
        "cold_reconstructed_before_dispatch":true,"acquisition_alone_is_delivery":false}),
    )?;
    let allowed_wires = vec![
        retain_wire(
            &output,
            "initial",
            &c.phase.request_for(EditPhaseCallKind::Initial)?,
            &provider,
            &c,
        )?,
        retain_wire(
            &output,
            "repair",
            &c.phase.request_for(EditPhaseCallKind::StructuralRepair)?,
            &provider,
            &c,
        )?,
        retain_wire(
            &output,
            "repair-truncated",
            &c.phase
                .for_truncated_output_repair()?
                .request_for(EditPhaseCallKind::StructuralRepair)?,
            &provider,
            &c,
        )?,
    ];
    // Owned seed process startup is already recorded as one machine-mutation attempt.
    // Context must preserve that real history, rather than pretending the Run started empty.
    let mutation_baseline = count(&c.f, "SELECT count(*) FROM mutation_attempts").await?;
    assert_eq!(mutation_baseline, 1);
    let observed = ContextLiveProvider {
        inner: &provider,
        store: c.f.http.store.as_ref(),
        project: &c.f.http.fixture.project,
        run: c.f.run.id(),
        profile,
        job: c.phase.job_digest(),
        output: &output,
        allowed_wires: &allowed_wires,
        calls: AtomicUsize::new(0),
        traces: Mutex::new(Vec::new()),
    };
    let started = Instant::now();
    let base = edit::after_phases(&c.f).await?;
    let first = ExecuteHttpContextIntent::new(
        &observed,
        &WorkspaceAgentSourceReader,
        c.f.http.store.as_ref(),
    )
    .execute(
        &c.f.http.fixture.project,
        &c.phase,
        EditPhaseCallKind::Initial,
        base,
        ModelRequestTimeout::from_millis(120000)?,
        &ActiveControl,
    )
    .await;
    let structural_repair_requested =
        matches!(first, Err(HttpContextIntentExecutionFailure::InvalidReply));
    let result = if structural_repair_requested {
        let truncated = observed
            .traces
            .lock()
            .map_err(|_| "trace lock")?
            .first()
            .and_then(|t| t.lock().ok().and_then(|t| t.completion))
            .is_some_and(|end| end.reason() == ModelFinishReason::OutputLimit);
        let repair = if truncated {
            Some(c.phase.for_truncated_output_repair()?)
        } else {
            None
        };
        ExecuteHttpContextIntent::new(
            &observed,
            &WorkspaceAgentSourceReader,
            c.f.http.store.as_ref(),
        )
        .execute(
            &c.f.http.fixture.project,
            repair.as_ref().unwrap_or(&c.phase),
            EditPhaseCallKind::StructuralRepair,
            at(base.unix_millis() + u64::try_from(started.elapsed().as_millis())?)?,
            ModelRequestTimeout::from_millis(120000)?,
            &ActiveControl,
        )
        .await
    } else {
        first
    };
    let total =
        c.f.http
            .store
            .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
            .await?;
    let calls = observed.calls.load(Ordering::SeqCst);
    let mut actual_prompt = Some(0_u64);
    let mut actual_output = Some(0_u64);
    {
        let traces = observed.traces.lock().map_err(|_| "trace lock")?;
        for (index, trace) in traces.iter().enumerate() {
            let trace = trace.lock().map_err(|_| "trace lock")?;
            let safe = if !trace.truncated {
                c.phase
                    .bind_reply(trace.text.as_bytes())
                    .and_then(|r| c.phase.decode_reply(r))
                    .ok()
            } else {
                None
            };
            let measured = trace.completion.map(|end| end.usage());
            actual_prompt = actual_prompt
                .zip(measured.and_then(|u| u.prompt_tokens()))
                .and_then(|(sum, n)| sum.checked_add(n));
            actual_output = actual_output
                .zip(measured.and_then(|u| u.output_tokens()))
                .and_then(|(sum, n)| sum.checked_add(n));
            write_once(
                &output,
                &format!("context-reply-{index}.json"),
                json!({
                "schema":"a3.plan22.http-context-reply.v1","raw_text_retained":false,
                "observed_utf8_bytes":trace.bytes,"observed_digest":trace.hash.finalize().to_hex().as_str(),
                "capture_truncated":trace.truncated,"core_schema_decoded":safe.is_some(),
                    "durable_acceptance_not_implied":true,
                "core_validated_canonical":safe.as_ref().map(|r|r.canonical_retained_bytes()).transpose()?
                    .map(String::from_utf8).transpose()?,
                "unverified_intent":safe.as_ref().and_then(|r|r.intent()).map(|i|i.as_str()),
                "intent_is_factual_evidence":false,"finish":trace.completion.map(|e|format!("{:?}",e.reason())),
                "actual_prompt_tokens":measured.and_then(|u|u.prompt_tokens()),"actual_output_tokens":measured.and_then(|u|u.output_tokens()),
                "failure":trace.failure.map(|e|format!("{e:?}"))}),
            )?;
        }
    }
    drop(observed);
    assert!((1..=2).contains(&calls));
    assert_eq!(total.calls, parent_usage.calls + u32::try_from(calls)?);
    assert_metadata_only(&c).await?;
    assert_eq!(
        count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
        mutation_baseline
    );
    let mut final_report = json!({"schema":"a3.plan22.http-context-component.v1",
        "scope":"isolated ContextIntent component; controlled parent, real S1 read and local context model; not end-to-end Agent",
        "model":tag,"profile":format!("{profile:?}"),"context_tokens":16384,"elapsed_ms":started.elapsed().as_millis(),
        "controlled_parent_provider_calls":0,"controlled_parent_charged_usage":usage_json(parent_usage),
        "actual_context_provider_calls":calls,"actual_context_prompt_tokens":actual_prompt,"actual_context_output_tokens":actual_output,
        "actual_context_repair_calls":total.repairs-parent_usage.repairs,
        "structural_repair_requested":structural_repair_requested,"semantic_retries":0,
        "repair_after_accepted_null":false,"shared_charged_usage":usage_json(total),
        "execution_error":result.as_ref().err().map(|e|format!("{e:?}")),
        "accepted_intent":result.as_ref().ok().and_then(|r|r.reply.intent()).map(|i|i.as_str()),
        "accepted_null":result.as_ref().is_ok_and(|r|r.reply.intent().is_none()),
        "intent_is_factual_evidence":false,"run_state":"Replan","original_and_ledger_unchanged":true,
        "source_unchanged":true,"apply_executed":false,"edit_executed":false,"generated_code_executed":false,
        "physical_seed_mismatch_executed":true,"physical_seed_machine_mutation_attempts":mutation_baseline,
        "corrected_http_executed":false,"new_context_mutation_attempts":0,
        "a04_completed":false,"p7_completed":false,"done":false});
    if let Ok(report) = &result {
        let reply = report.reply.clone();
        let wire = report.projection.digest();
        let job = c.phase.job_digest();
        let c = cold_context(c).await?;
        assert_eq!(c.phase.job_digest(), job);
        let settled =
            c.f.http
                .store
                .load_http_context_intent_for_application(
                    &c.f.http.fixture.project,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                )
                .await?
                .ok_or("cold actual Context settlement missing")?;
        assert_settlement(&c, &settled, &reply, wire)?;
        assert_metadata_only(&c).await?;
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
            mutation_baseline
        );
        final_report["new_context_mutation_attempts"] = json!(0);
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM tool_run_attempts").await?,
            2
        );
        final_report["cold_equal"] = json!(true);
        final_report["cold_source_new_bytes"] = json!(719);
        final_report["cold_primary_repeat_bytes"] = json!(370);
        final_report["accepted_wire_digest"] = json!(format!("{wire:?}"));
        if reply.intent().is_none() {
            // A null obtained by the one structural repair is regular too; it causes no further call.
            assert_eq!(calls, if structural_repair_requested { 2 } else { 1 });
        }
        // Retain the exact owned fixture and reports. Never invoke temp-fixture cleanup here.
        drop(c);
    } else {
        final_report["cold_settlement_not_claimed"] = json!(true);
        drop(c);
    }
    write_once(&output, "context-final.json", final_report)?;
    result.map(|_| ()).map_err(Into::into)
}
