//! Actual acquired target, reserved Edit and approved Patch after the applied HTTP plan.
use super::*;
#[cfg(any(windows, target_os = "linux"))]
#[path = "plan22_http_correction_functional_tests.rs"]
pub(super) mod functional;
#[path = "plan22_edit_scope_frame_tests.rs"]
mod scope_frame;
use crate::plan22_goal_contract_tests::existing_test_tests::changed_tests::{
    execute_approved_until, native_runner,
};
use a3_context::DeterministicAgentReadTools;
#[cfg(any(windows, target_os = "linux"))]
#[path = "plan22_edit_no_change_tests.rs"]
pub(super) mod no_change;

pub(super) fn assert_fixture_diagnostic(
    compiled: &CompiledEditPhaseRequest,
    observed: &LoopbackHttpMismatchDiagnostic,
    recipe: &LoopbackHttpRecipe,
) -> TestResult {
    // Independent expectations for this real pre-existing fixture, not input to activation/model.
    let mismatch = observed.mismatch();
    assert_eq!(mismatch.phase(), LoopbackStartupPhase::FreeExplicit);
    assert_eq!(mismatch.check_index(), 0);
    assert_eq!(mismatch.observation().status(), 200);
    assert_eq!(mismatch.observation().body_bytes(), 13);
    let expected = recipe.checks().first().ok_or("original check missing")?;
    assert_eq!(expected.status(), 200);
    assert!(matches!(
        expected.body(),
        LoopbackExpectedBody::Exact { bytes: 14, .. }
    ));
    let actual = compiled.request().messages()[1].content();
    assert!(actual.contains("Observed: FreeExplicit GET / status200/200 bytes13/14 different\n"));
    assert!(actual.contains("Scope: \"server.py\":22-31; whole method \"do_GET\".\n"));
    assert!(
        actual
            .contains("Core verification: scoped diff pending; dependent original HTTP pending.\n")
    );
    let format = compiled
        .request()
        .structured_output()
        .ok_or("Edit v3 schema")?
        .value();
    assert!(!format.to_string().contains("test"));
    // This real 370-byte complete-line hull remains outside exact parser-range Need offers.
    assert!(!format.to_string().contains("\"need\""));
    Ok(())
}

pub(super) struct HttpEditFixture {
    pub(super) http: PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    pub(super) run: AgentRun,
    pub(super) stored: StoredTaskLedger,
}

pub(super) async fn cold(f: CurrentPlanFixture) -> TestResult<HttpEditFixture> {
    // Deliberately drop the old PreparedHttpCorrectionPlan, all stores and index owners.
    let CurrentPlanFixture {
        http,
        run,
        stored,
        phase,
    } = f;
    drop(phase);
    reopen(HttpEditFixture { http, run, stored }).await
}

pub(super) async fn reopen(f: HttpEditFixture) -> TestResult<HttpEditFixture> {
    let HttpEditFixture { http, run, stored } = f;
    let PreparedHttpFixture {
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
        run: old_run,
        stored: old_stored,
        settings,
        prepared,
    } = http;
    drop((store, refresh, compiler));
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let loaded_goal = store
        .load_current_goal_contract(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold original missing")?;
    assert_eq!(loaded_goal, goal);
    let loaded_run = store
        .load_agent_run(&fixture.project, run.id())
        .await?
        .ok_or("cold applied run missing")?;
    let loaded_ledger = store
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold applied plan missing")?;
    assert_eq!(loaded_run, run);
    assert_eq!(loaded_ledger, stored);
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    Ok(HttpEditFixture {
        http: PreparedHttpFixture {
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
            run: old_run,
            stored: old_stored,
            settings,
            prepared,
        },
        run: loaded_run,
        stored: loaded_ledger,
    })
}

pub(super) async fn after_phases(f: &HttpEditFixture) -> TestResult<AgentRunTimestamp> {
    let db = f.http.fixture.candidate_database().await?;
    let conn = db.connection_for_operation().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
        .await?;
    let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
        &tx,
        f.http.fixture.project.worktree().id(),
        &f.run,
    )
    .await?;
    let millis = latest
        .map_or(f.run.updated_at().unix_millis(), |v| v.unix_millis())
        .max(f.run.updated_at().unix_millis())
        .checked_add(1)
        .ok_or("phase clock overflow")?;
    tx.commit().await?;
    at(millis)
}

pub(super) fn state<'a>(
    f: &'a HttpEditFixture,
    published: &'a PublishedIndex,
) -> TestResult<EditPhaseState<'a>> {
    Ok(EditPhaseState::new(
        &f.http.fixture.project,
        &f.http.goal,
        &f.stored,
        &f.run,
        published,
    )?)
}

pub(super) async fn acquire(
    f: &mut HttpEditFixture,
) -> TestResult<(
    PreparedEditPhase,
    CompiledEditPhaseRequest,
    AgentSourcePage,
    HttpFailureEditDiagnostic,
)> {
    let floor = after_phases(f).await?;
    acquire_with_floor(f, floor).await
}

async fn live_helper_time(
    f: &HttpEditFixture,
    start: Instant,
    floor: AgentRunTimestamp,
) -> TestResult<AgentRunTimestamp> {
    let durable = after_phases(f).await?;
    let now = floor
        .unix_millis()
        .checked_add(u64::try_from(start.elapsed().as_millis())?)
        .ok_or("helper clock overflow")?
        .max(durable.unix_millis());
    let now = at(now)?;
    if f.run.budget_exhaustion(now)?.is_some() {
        return Err("original helper deadline exhausted".into());
    }
    Ok(now)
}

pub(super) async fn acquire_with_floor(
    f: &mut HttpEditFixture,
    floor: AgentRunTimestamp,
) -> TestResult<(
    PreparedEditPhase,
    CompiledEditPhaseRequest,
    AgentSourcePage,
    HttpFailureEditDiagnostic,
)> {
    let live_started = Instant::now();
    let project = &f.http.fixture.project;
    let published = f
        .http
        .store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("current HTTP edit publication missing")?;
    let now = live_helper_time(f, live_started, floor).await?;
    let (mut ledger, mut version) = f.stored.clone().into_parts();
    let diagnostic = ActivateHttpCorrectionEdit::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    )
    .execute(
        project,
        &f.http.goal,
        &mut f.run,
        &mut ledger,
        &mut version,
        &published,
        &f.http.model,
        now,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, version);
    assert_eq!(f.run.state(), AgentControllerState::Execute);
    let seed = TaskLensSeedText::try_from_string(
        "Read the current applied HTTP correction target".to_owned(),
    )?;
    let lens = CompileTaskLens::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
    )
    .execute(
        project,
        TaskLensSeedSet::new(
            seed.clone(),
            seed,
            vec![TaskLensSeed::ExplicitPath(
                diagnostic.required_source().revision().path().clone(),
            )],
        )?,
        TaskLensTokenBudget::new(4096)?,
        &ActiveControl,
    )
    .await?;
    let bound =
        BoundEditSource::for_file_span(project, &published, &lens, diagnostic.required_source())?;
    let read_tool = ToolRunId::from_bytes([240; 32]);
    f.http
        .store
        .begin_source_read(
            project,
            state(f, &published)?.anchor(),
            &bound,
            read_tool,
            live_helper_time(f, live_started, floor).await?,
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
            published.run().snapshot_id(),
            read_tool,
            &bound.read_action(project, &published)?,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = bound.take_original(project, &published, read_tool, &mut read)?;
    let sequence = f.run.last_event_sequence();
    let now = live_helper_time(f, live_started, floor).await?;
    let recorded = read.record(&mut f.run, RunEventId::from_bytes([240; 32]), now)?;
    AppendAgentRead::new(f.http.store.as_ref())
        .execute(project, sequence, &f.run, &recorded)
        .await?;
    let reads = f
        .http
        .store
        .load_source_read_state(project, state(f, &published)?.anchor())
        .await?;
    let acquired = reads
        .acquired()
        .ok_or("actual HTTP target read stamp missing")?;
    let phase = PreparedEditPhase::update(state(f, &published)?, &page)?;
    let compiled = build_edit_phase_request(
        &f.http.goal,
        &f.stored,
        &phase,
        state(f, &published)?,
        f.http.model.clone(),
        None,
        EditPhasePrerequisites::none()
            .with_http_failure(&diagnostic)
            .with_required_source(
                EditEvidenceNeed::Definition,
                &bound,
                Some((&page, acquired)),
            ),
    )?;
    let mut compiled = compiled;
    let phase = crate::plan22_goal_contract_tests::prepare_indexed_edit(
        &f.http.compiler,
        phase,
        &mut compiled,
        state(f, &published)?,
        live_helper_time(f, live_started, floor).await?,
        &WorkspacePatchAdapter::with_change_window_preview(),
        &ActiveControl,
    )
    .await?;
    // Retaining the original in storage is not a source/requirement delivery proof.
    let actual = compiled.request().messages()[1].content();
    let active = f
        .stored
        .ledger()
        .step(phase.anchor().step())
        .ok_or("actual Edit step missing")?;
    assert!(actual.contains(&format!(
        "Step: {}\n",
        active.definition().intended_outcome().as_str()
    )));
    let active_criteria = f
        .http
        .goal
        .draft()
        .acceptance_criteria()
        .iter()
        .filter(|criterion| {
            active
                .definition()
                .addressed_criteria()
                .contains(&criterion.id())
                || active
                    .definition()
                    .acceptance_criteria()
                    .contains(&criterion.id())
        })
        .collect::<Vec<_>>();
    assert!(actual.contains(&format!(
        "Coverage total={} outside={}",
        f.http.goal.draft().acceptance_criteria().len(),
        f.http.goal.draft().acceptance_criteria().len() - active_criteria.len(),
    )));
    for criterion in active_criteria {
        assert!(actual.contains(&format!(
            "Address must: {}\n",
            criterion.statement().as_str()
        )));
    }
    for constraint in f.http.goal.draft().constraints() {
        assert!(actual.contains(&format!("Rule: {}\n", constraint.as_str())));
    }
    assert!(actual.contains(&format!("Observed: {}\n", diagnostic.summary())));
    let encoder = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    for (kind, wire) in [
        &compiled,
        &compiled.for_structural_repair()?,
        &compiled.for_truncated_output_repair()?,
    ]
    .into_iter()
    .enumerate()
    {
        let projection = encoder.project_wire_request(wire.request())?;
        eprintln!(
            "http_correction_edit_wire kind={kind} bytes={} context={} output={}",
            projection.request_bytes(),
            projection.context_tokens(),
            projection.output_tokens()
        );
        PhaseWireBudget::fit(projection, wire.request().profile())?;
    }
    Ok((phase, compiled, page, diagnostic))
}

pub(super) async fn patch(f: HttpEditFixture, action: AgentAction) -> TestResult<Fixture> {
    let floor = after_phases(&f).await?;
    patch_with_floor(f, action, floor).await
}

pub(super) async fn patch_with_floor(
    f: HttpEditFixture,
    action: AgentAction,
    floor: AgentRunTimestamp,
) -> TestResult<Fixture> {
    let live_started = Instant::now();
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("HTTP patch publication missing")?;
    let correction = state(&f, &published)?.anchor().step();
    let original_http = f.http.spec.target().clone();
    let original_criteria = f
        .stored
        .ledger()
        .step(correction)
        .ok_or("HTTP correction criteria missing")?
        .definition()
        .addressed_criteria()
        .to_vec();
    assert!(!original_criteria.is_empty());
    let now = live_helper_time(&f, live_started, floor)
        .await?
        .unix_millis();
    let HttpEditFixture {
        http,
        mut run,
        stored,
    } = f;
    let PreparedHttpFixture {
        fixture,
        store,
        refresh,
        mut compiler,
        goal,
        model,
        ..
    } = http;
    let (mut ledger, mut version) = stored.into_parts();
    let mutation = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let runner = native_runner()?;
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &mutation,
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
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), model, Vec::new(), Vec::new());
    let deadline = at(run
        .created_at()
        .unix_millis()
        .checked_add(run.budget().duration_limit().millis())
        .ok_or("deadline overflow")?)?;
    let changed = execute_approved_until(
        controller,
        &fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        action,
        None,
        180,
        now,
        deadline,
        &seed,
        &mut compiler,
        &events,
        async |_| Ok(()),
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
        ..
    } = changed
    else {
        return Err("actual HTTP correction patch did not verify current Diff".into());
    };
    assert_eq!(run.state(), AgentControllerState::Verify);
    // The new proof remains pending. The Addresses diff cannot satisfy HTTP functionality.
    let proofs = ledger
        .steps()
        .filter(|s| {
            s.is_active_plan_step()
                && matches!(
                    s.definition().verification_spec().target(),
                    VerificationTarget::LoopbackHttp(_)
                )
        })
        .collect::<Vec<_>>();
    let [proof] = proofs.as_slice() else {
        return Err("original HTTP proof not unique".into());
    };
    assert_eq!(
        proof.definition().verification_spec().target(),
        &original_http
    );
    assert_eq!(proof.definition().acceptance_criteria(), original_criteria);
    for id in &original_criteria {
        let criterion = goal
            .draft()
            .acceptance_criteria()
            .iter()
            .find(|criterion| criterion.id() == *id)
            .ok_or("original HTTP criterion missing")?;
        criterion
            .origin()
            .ok_or("HTTP criterion origin missing")?
            .evidence_requirement()
            .validate_spec(proof.definition().verification_spec())?;
    }
    assert_eq!(
        proof.definition().dependencies(),
        [StepDependency::new(correction)]
    );
    assert!(proof.attempts().is_empty() && proof.status() != TaskStepStatus::Completed);
    let run_id = run.id();
    drop((store, refresh, compiler));
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        cold.load_agent_run(&fixture.project, run_id).await?,
        Some(run)
    );
    assert_eq!(
        cold.load_current_goal_contract(&fixture.project, goal.task_id())
            .await?,
        Some(goal.clone())
    );
    let loaded = cold
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold ledger missing")?;
    assert_eq!(loaded.ledger(), &ledger);
    assert_eq!(loaded.version(), version);
    let evidence = cold
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[evidence_id],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    assert!(
        matches!(evidence.evidence(), [VerificationEvidence::Diff(d)] if d.complete() && d.snapshot_id()==snapshot_id)
    );
    drop(cold);
    Ok(fixture)
}

/// Existing real Edit/Patch/full-HTTP machinery for a controlled Context material fixture.
/// This is native component evidence; the controlled code is not a local-model quality result.
#[cfg(any(windows, target_os = "linux"))]
pub(super) async fn complete_controlled_context_http(mut f: HttpEditFixture) -> TestResult {
    let (phase, mut compiled, page, _diagnostic) = acquire(&mut f).await?;
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("Context Edit publication missing")?;
    let reads = f
        .http
        .store
        .load_source_read_state(&f.http.fixture.project, state(&f, &published)?.anchor())
        .await?;
    assert!(reads.progress().receipts().is_empty());
    assert_eq!(
        reads
            .acquired()
            .ok_or("real Edit source required")?
            .original()
            .range(),
        page.range()
    );
    assert_eq!(page.text().len(), 370);
    let corrected = page
        .text()
        .replace("b\"Hello, world!\"", "b\"Hello, world!\\n\"");
    assert_ne!(corrected, page.text());
    let provider = NativeEdit {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        store: f.http.store.as_ref(),
        project: &f.http.fixture.project,
        run: f.run.id(),
        reply: json!({"code":corrected}).to_string(),
        calls: AtomicUsize::new(0),
    };
    let patch_tool = WorkspacePatchAdapter::with_change_window_preview();
    let report = ExecuteEditPhase::new(&provider, f.http.store.as_ref(), &patch_tool)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                state(&f, &published)?,
                after_phases(&f).await?,
                &patch_tool,
                &ActiveControl,
            ))
            .await?,
            state(&f, &published)?,
            &compiled,
            after_phases(&f).await?,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let delivery = report
        .source_delivery
        .ok_or("actual Edit delivery missing")?;
    let EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("controlled Context Edit proposal missing".into());
    };
    drop((provider, published, reads));
    let f = reopen(f).await?;
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold index")?;
    let reads = f
        .http
        .store
        .load_source_read_state(&f.http.fixture.project, state(&f, &published)?.anchor())
        .await?;
    let [receipt] = reads.progress().receipts() else {
        return Err("exactly one actual Edit delivery".into());
    };
    assert_eq!(receipt.delivery(), Some(&delivery));
    assert_eq!(delivery.source().range(), page.range());
    assert_ne!(delivery.source().range().len(), 719); // earlier Context delivery is not this Edit delivery.
    assert!(reads.progress().pending().is_none() && reads.acquired().is_none());
    let runtime_run = f.run.id();
    let runtime_profile = f.http.model.clone();
    let runtime_settings = f.http.settings;
    let runtime_floor = f.run.updated_at();
    drop((published, reads, page));
    let fixture = patch(f, action).await?;
    let expected = FROZEN_SOURCE.replacen("b\"Hello, world!\"", "b\"Hello, world!\\n\"", 1);
    assert_eq!(
        fs::read(fixture.root.join("worktree/server.py"))?,
        expected.as_bytes()
    );
    let (fixture, report) = functional::finish_http(
        fixture,
        runtime_run,
        runtime_profile,
        runtime_settings,
        runtime_floor,
        hash(expected.as_bytes()),
    )
    .await?;
    assert_eq!(report["fresh_exact_grants"], 12);
    assert_eq!(report["total_http_observations"], 7);
    assert_eq!(report["owned_starts"], 4);
    assert_eq!(report["cold_done"], true);
    assert_eq!(report["a04_completed"], false);
    fixture.cleanup()
}

/// Existing strict controlled provider, exported only to sibling native test consumers.
/// It projects actual Ollama wire but never invokes Ollama or a model.
pub(super) fn controlled_native_edit_provider<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    run: AgentRunId,
    reply: String,
) -> TestResult<impl ModelProvider + 'a> {
    Ok(NativeEdit {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        store,
        project,
        run,
        reply,
        calls: AtomicUsize::new(0),
    })
}

struct NativeEdit<'a> {
    encoder: OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    run: AgentRunId,
    reply: String,
    calls: AtomicUsize,
}
impl std::fmt::Debug for NativeEdit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEdit").finish_non_exhaustive()
    }
}
impl ModelProvider for NativeEdit<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(&self.encoder)
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
        r: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.encoder.project_wire_request(r)
    }
    fn stream_projected<'a>(
        &'a self,
        r: &'a ModelProviderRequest,
        p: ModelWireProjection,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .store
                .load_edit_phase_usage(self.project, self.run)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1
                || usage.unknown_calls != 0
                || self.project_wire_request(r)? != p
                || self.calls.fetch_add(1, Ordering::SeqCst) != 0
            {
                return Err(ModelProviderFailure::Rejected);
            }
            Ok(Box::pin(futures::stream::iter([
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(self.reply.clone())
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(128), Some(32)),
                ))),
            ])) as ProviderEventStream<'a>)
        })
    }
}

async fn intent_preflight_fixture(
    tag: &str,
    intent: &HttpCorrectionPlanIntent,
) -> TestResult<(HttpEditFixture, TaskStepId)> {
    intent_preflight_fixture_with_profile(ollama_profile(tag)?, intent).await
}

// Same actual fixture path, with an explicitly selected immutable native profile.
async fn intent_preflight_fixture_with_profile(
    profile: ModelProfile,
    intent: &HttpCorrectionPlanIntent,
) -> TestResult<(HttpEditFixture, TaskStepId)> {
    intent_preflight_current_fixture(full_fixture(profile, true).await?, intent).await
}

// A real current fixture may deliberately contain additional original criteria.
async fn intent_preflight_current_fixture(
    mut f: CurrentPlanFixture,
    intent: &HttpCorrectionPlanIntent,
) -> TestResult<(HttpEditFixture, TaskStepId)> {
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("current preflight index missing")?;
    // Test-only focus comes from this current parsed method, never an oracle offset.
    let candidates = current
        .publication()
        .graph()
        .symbols()
        .iter()
        .filter(|s| {
            s.revision() == &f.phase.anchor().source
                && matches!(s.parsed().kind(), SymbolKind::Function | SymbolKind::Method)
                && s.parsed().name().as_str() == "do_GET"
        })
        .collect::<Vec<_>>();
    let [handler] = candidates.as_slice() else {
        return Err("current preflight handler is not unique".into());
    };
    let focus = handler
        .parsed()
        .declaration_range()
        .start_position()
        .row()
        .checked_add(1)
        .ok_or("current handler line overflow")?;
    let raw = reply_with_intent(&f.phase, &format!("L{focus}"), intent)?;
    let proposal = f.phase.decode_proposal(&raw)?;
    assert_eq!(proposal.intent(), intent);
    let reserve_at = f.run.updated_at().unix_millis() + 1;
    let permit = f
        .http
        .store
        .reserve_http_correction_plan(
            &f.http.fixture.project,
            &f.phase,
            EditPhaseCallKind::Initial,
            wire(&f.phase, EditPhaseCallKind::Initial)?,
            at(reserve_at)?,
        )
        .await?;
    f.http
        .store
        .finish_http_correction_plan(
            &f.http.fixture.project,
            &f.phase,
            permit,
            observation(EditPhaseObservationOutcome::AcceptedReply, reserve_at + 1)?,
            Some(proposal),
        )
        .await?;
    let (mut ledger, version) = f.stored.clone().into_parts();
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
        at(reserve_at + 2)?,
        &ActiveControl,
    )
    .await?;
    assert_eq!(
        ledger
            .step(applied.correction)
            .ok_or("preflight correction missing")?
            .definition()
            .intended_outcome()
            .as_str(),
        intent.as_str()
    );
    f.stored = StoredTaskLedger::new(ledger, applied.version);
    Ok((cold(f).await?, applied.correction))
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_edit_full_96_byte_intent_survives_cold_material_and_all_three_actual_wires()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let text = "Correct the selected HTTP handler response while preserving routing startup behavior and imports";
                assert_eq!(text.len(), 96);
                assert!(text.is_ascii() && text.bytes().all(|b| b.is_ascii_alphabetic() || b == b' '));
                let intent = HttpCorrectionPlanIntent::try_from_string(text.to_owned())?;
                for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
                    let (mut f, correction) = intent_preflight_fixture(tag, &intent).await?;
                    let before = inventory(&f.http.root)?;
                    let (_, compiled, _, _) = acquire(&mut f).await?;
                    assert!(compiled.request().messages()[1].content().contains(&format!("Step: {text}\n")));
                    let encoder = OllamaModelProvider::for_projected_wire(
                        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                        Arc::new(LocalOnlyOllamaEndpointPolicy),
                    )?;
                    for (kind, request) in [
                        &compiled, &compiled.for_structural_repair()?,
                        &compiled.for_truncated_output_repair()?,
                    ].into_iter().enumerate() {
                        let projection = encoder.project_wire_request(request.request())?;
                        eprintln!("http_intent96_wire model={tag} kind={kind} bytes={}", projection.request_bytes());
                        assert_eq!((projection.context_tokens(), projection.output_tokens()), (4096, 1024));
                        let budget = PhaseWireBudget::fit(projection, request.request().profile())?;
                        assert!(u64::from(budget.output_tokens()) * 100 >= 4096 * 22);
                        assert!(budget.prompt_tokens() + budget.output_tokens() <= 4096);
                    }
                    assert_eq!(inventory(&f.http.root)?, before);
                    let f = reopen(f).await?;
                    assert_eq!(f.stored.ledger().step(correction).ok_or("cold preflight correction missing")?
                        .definition().intended_outcome().as_str(), text);
                    assert_eq!(inventory(&f.http.root)?, before);
                    drop((f.http.compiler, f.http.refresh, f.http.store));
                    f.http.fixture.cleanup()?;
                }
                Ok(())
            })
    })
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_edit_escaped_96_byte_intent_halts_at_actual_capacity_without_edit_dispatch()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let text = "\"".repeat(96);
                let intent = HttpCorrectionPlanIntent::try_from_string(text.clone())?;
                assert_eq!(intent.as_str(), text);
                for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
                    let (mut f, correction) = intent_preflight_fixture(tag, &intent).await?;
                    let before = inventory(&f.http.root)?;
                    let charged = f
                        .http
                        .store
                        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                        .await?;
                    assert_eq!(
                        (
                            charged.calls,
                            charged.repairs,
                            charged.unobserved_calls,
                            charged.unknown_calls
                        ),
                        (1, 0, 0, 0)
                    );
                    let error = acquire(&mut f)
                        .await
                        .err()
                        .ok_or("escaped intent unexpectedly fit")?;
                    assert_eq!(
                        error.downcast_ref::<PhaseWireBudgetError>(),
                        Some(&PhaseWireBudgetError::Capacity)
                    );
                    assert_eq!(
                        f.http
                            .store
                            .load_edit_phase_usage(&f.http.fixture.project, f.run.id(),)
                            .await?,
                        charged
                    );
                    assert_eq!(
                        f.stored
                            .ledger()
                            .step(correction)
                            .ok_or("capacity correction missing")?
                            .definition()
                            .intended_outcome()
                            .as_str(),
                        text
                    );
                    assert_eq!(f.http.goal.draft().constraints().len(), 3);
                    let original = f.http.goal.original().ok_or("capacity original missing")?;
                    let full_criterion = std::str::from_utf8(
                        original
                            .as_bytes()
                            .get(..402)
                            .ok_or("original criterion range missing")?,
                    )?
                    .trim();
                    assert_eq!(
                        f.http.goal.draft().acceptance_criteria()[0]
                            .statement()
                            .as_str(),
                        full_criterion
                    );
                    assert_eq!(
                        f.http
                            .goal
                            .original()
                            .ok_or("capacity original missing")?
                            .as_bytes()
                            .len(),
                        1207
                    );
                    assert!(GoalOriginalCoverage::derive_from(&f.http.goal)?.is_complete());
                    assert_eq!(
                        f.http
                            .store
                            .load_current_goal_contract(
                                &f.http.fixture.project,
                                f.http.goal.task_id(),
                            )
                            .await?,
                        Some(f.http.goal.clone())
                    );
                    // The sole charged provider operation is the retained Plan. No Edit reservation
                    // or Patch dispatch was admitted; the real SourceRead remains accounted for.
                    {
                        let db = f.http.fixture.candidate_database().await?;
                        let conn = db.connection_for_operation().await?;
                        let mut rows = conn
                            .query(
                                "SELECT COUNT(*) FROM edit_phase_reservations WHERE run_id=?1",
                                libsql::params![f.run.id().as_bytes().to_vec()],
                            )
                            .await?;
                        let row = rows.next().await?.ok_or("Edit dispatch count missing")?;
                        assert_eq!(row.get::<i64>(0)?, 0);
                    }
                    assert_eq!(inventory(&f.http.root)?, before);
                    let f = reopen(f).await?;
                    assert_eq!(
                        f.stored
                            .ledger()
                            .step(correction)
                            .ok_or("cold capacity correction missing")?
                            .definition()
                            .intended_outcome()
                            .as_str(),
                        text
                    );
                    assert_eq!(
                        f.http
                            .store
                            .load_edit_phase_usage(&f.http.fixture.project, f.run.id(),)
                            .await?,
                        charged
                    );
                    assert_eq!(inventory(&f.http.root)?, before);
                    drop((f.http.compiler, f.http.refresh, f.http.store));
                    f.http.fixture.cleanup()?;
                }
                Ok(())
            })
    })
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_edit_actual_read_reserved_edit_approved_patch_and_cold_diff() -> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
                    let mut f = full_fixture(ollama_profile(tag)?, true).await?;
                    let observed = f
                        .http
                        .store
                        .load_loopback_mismatch(
                            &f.http.fixture.project,
                            f.run.id(),
                            TOOL,
                            AgentToolAttemptNumber::FIRST,
                        )
                        .await?
                        .ok_or("original physical mismatch missing")?;
                    let reserve_at = f.run.updated_at().unix_millis() + 1;
                    let permit = f
                        .http
                        .store
                        .reserve_http_correction_plan(
                            &f.http.fixture.project,
                            &f.phase,
                            EditPhaseCallKind::Initial,
                            wire(&f.phase, EditPhaseCallKind::Initial)?,
                            at(reserve_at)?,
                        )
                        .await?;
                    f.http
                        .store
                        .finish_http_correction_plan(
                            &f.http.fixture.project,
                            &f.phase,
                            permit,
                            observation(
                                EditPhaseObservationOutcome::AcceptedReply,
                                reserve_at + 1,
                            )?,
                            Some(f.phase.decode_proposal(&reply(&f.phase, "L24")?)?),
                        )
                        .await?;
                    let current = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("index missing")?;
                    let (mut ledger, version) = f.stored.clone().into_parts();
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
                        at(reserve_at + 2)?,
                        &ActiveControl,
                    )
                    .await?;
                    f.stored = StoredTaskLedger::new(ledger, applied.version);
                    let mut f = cold(f).await?;
                    let before = inventory(&f.http.root)?;
                    let (phase, mut compiled, page, diagnostic) = acquire(&mut f).await?;
                    assert!(page.truncated()); // More file lines exist beyond the complete definition.
                    let read_events = f
                        .http
                        .store
                        .load_run_events(
                            &f.http.fixture.project,
                            f.run.id(),
                            None,
                            a3_application::RunEventPageLimit::new(256)?,
                        )
                        .await?;
                    assert!(
                        read_events
                            .events()
                            .last()
                            .ok_or("actual read event missing")?
                            .payload()
                            .redaction()
                            .ok_or("read redaction missing")?
                            .source_was_truncated()
                    );
                    // Replay the new +4 origin prefix while the real target read is acquired
                    // but has not yet been delivered to a provider. The old owners are closed.
                    let prior_run = f.run.clone();
                    let prior_stored = f.stored.clone();
                    let prior_current = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("pre-reopen target publication missing")?;
                    let prior_reads = f
                        .http
                        .store
                        .load_source_read_state(
                            &f.http.fixture.project,
                            state(&f, &prior_current)?.anchor(),
                        )
                        .await?;
                    assert!(prior_reads.progress().pending().is_some());
                    assert!(prior_reads.progress().receipts().is_empty());
                    assert_eq!(
                        prior_reads
                            .acquired()
                            .ok_or("actual acquired target missing")?
                            .original(),
                        &EvidenceRef::new(page.revision().clone(), page.range())
                    );
                    drop(prior_current);
                    let mut f = reopen(f).await?;
                    let replay_current = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("cold acquired target publication missing")?;
                    let now = after_phases(&f).await?;
                    let origin = f
                        .http
                        .store
                        .load_applied_http_correction_origin(
                            &f.http.fixture.project,
                            &f.http.goal,
                            &f.stored,
                            &f.run,
                            &replay_current,
                            &f.http.model,
                            now,
                        )
                        .await?;
                    assert_eq!(&origin.anchor.source, page.revision());
                    let (mut replay_ledger, mut replay_version) = f.stored.clone().into_parts();
                    let repeated = ActivateHttpCorrectionEdit::new(
                        f.http.store.as_ref(),
                        f.http.store.as_ref(),
                        f.http.store.as_ref(),
                        &WorkspaceAgentSourceReader,
                    )
                    .execute(
                        &f.http.fixture.project,
                        &f.http.goal,
                        &mut f.run,
                        &mut replay_ledger,
                        &mut replay_version,
                        &replay_current,
                        &f.http.model,
                        now,
                        &ActiveControl,
                    )
                    .await?;
                    assert_eq!(repeated, diagnostic);
                    assert_eq!(f.run, prior_run);
                    assert_eq!(
                        StoredTaskLedger::new(replay_ledger, replay_version),
                        prior_stored
                    );
                    assert_eq!(f.stored, prior_stored);
                    let replay_reads = f
                        .http
                        .store
                        .load_source_read_state(
                            &f.http.fixture.project,
                            state(&f, &replay_current)?.anchor(),
                        )
                        .await?;
                    assert_eq!(
                        replay_reads.progress().pending(),
                        prior_reads.progress().pending()
                    );
                    assert_eq!(
                        replay_reads.progress().receipts(),
                        prior_reads.progress().receipts()
                    );
                    assert_eq!(replay_reads.acquired(), prior_reads.acquired());
                    assert_eq!(
                        replay_reads.caller_relocalizations(),
                        prior_reads.caller_relocalizations()
                    );
                    assert_eq!(inventory(&f.http.root)?, before);
                    let VerificationTarget::LoopbackHttp(recipe) = f.http.spec.target() else {
                        return Err("original HTTP recipe missing".into());
                    };
                    assert_fixture_diagnostic(&compiled, &observed, recipe)?;
                    // Controlled native response only; separate real local inference is required.
                    assert!(page.text().contains("body ="));
                    assert!(page.text().split_inclusive('\n').count() > 1);
                    assert!(page.text().contains("def do_GET(self):"));
                    assert!(page.text().contains("self.wfile.write(body)"));
                    let published = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("publication missing")?;
                    let range = page.range();
                    let shifted = AgentSourcePage::new(
                        page.revision().clone(),
                        SourceRange::new(
                            usize::try_from(range.start_byte())? + 1,
                            usize::try_from(range.end_byte())?,
                            SourcePosition::new(
                                range.start_position().row(),
                                range.start_position().column() + 1,
                            ),
                            range.end_position(),
                        )?,
                        page.start_line(),
                        page.text()
                            .get(1..)
                            .ok_or("native leading ASCII space missing")?
                            .to_owned(),
                        page.next_start_line(),
                        page.truncated(),
                    )?;
                    let other = PreparedEditPhase::update(state(&f, &published)?, &shifted)?;
                    assert!(matches!(
                        build_edit_phase_request(
                            &f.http.goal,
                            &f.stored,
                            &other,
                            state(&f, &published)?,
                            f.http.model.clone(),
                            None,
                            EditPhasePrerequisites::none().with_http_failure(&diagnostic)
                        ),
                        Err(EditPhaseRequestFailure::StaleDiagnostics)
                    ));
                    let corrected = page
                        .text()
                        .replace("b\"Hello, world!\"", "b\"Hello, world!\\n\"");
                    assert_ne!(corrected, page.text());
                    let provider = NativeEdit {
                        encoder: OllamaModelProvider::for_projected_wire(
                            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                            Arc::new(LocalOnlyOllamaEndpointPolicy),
                        )?,
                        store: f.http.store.as_ref(),
                        project: &f.http.fixture.project,
                        run: f.run.id(),
                        reply: json!({"code":corrected}).to_string(),
                        calls: AtomicUsize::new(0),
                    };
                    let published = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("index missing")?;
                    let patch_tool = WorkspacePatchAdapter::with_change_window_preview();
                    let report =
                        ExecuteEditPhase::new(&provider, f.http.store.as_ref(), &patch_tool)
                            .execute(
                                Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                                    &f.http.compiler,
                                    phase,
                                    &mut compiled,
                                    state(&f, &published)?,
                                    after_phases(&f).await?,
                                    &patch_tool,
                                    &ActiveControl,
                                ))
                                .await?,
                                state(&f, &published)?,
                                &compiled,
                                after_phases(&f).await?,
                                ModelRequestTimeout::DEFAULT,
                                &ActiveControl,
                            )
                            .await?;
                    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                    let delivery = report
                        .source_delivery
                        .ok_or("actual target wire delivery missing")?;
                    let EditPhaseOutcome::Proposal(action) = report.outcome else {
                        return Err("native HTTP Edit proposal missing".into());
                    };
                    drop(provider);
                    let f = reopen(f).await?;
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
                            state(&f, &published)?.anchor(),
                        )
                        .await?;
                    let [receipt] = reads.progress().receipts() else {
                        return Err("cold HTTP source delivery missing".into());
                    };
                    assert_eq!(receipt.delivery(), Some(&delivery));
                    assert!(reads.progress().pending().is_none() && reads.acquired().is_none());
                    let runtime_run = f.run.id();
                    let runtime_profile = f.http.model.clone();
                    let runtime_settings = f.http.settings;
                    let runtime_floor = f.run.updated_at();
                    let fixture = patch(f, action).await?;
                    let expected =
                        FROZEN_SOURCE.replacen("b\"Hello, world!\"", "b\"Hello, world!\\n\"", 1);
                    assert_eq!(
                        fs::read(fixture.root.join("worktree/server.py"))?,
                        expected.as_bytes()
                    );
                    let after = inventory(&fixture.root.join("worktree"))?;
                    assert_eq!(before.len(), after.len());
                    // Known controlled response only: real local-model source needs independent
                    // review and the separate ignored current-fixture verification entry.
                    let (fixture, functional) = functional::finish_http(
                        fixture,
                        runtime_run,
                        runtime_profile,
                        runtime_settings,
                        runtime_floor,
                        hash(expected.as_bytes()),
                    )
                    .await?;
                    assert_eq!(functional["fresh_exact_grants"], 12);
                    assert_eq!(functional["explicit_http_observations"], 6);
                    assert_eq!(functional["total_http_observations"], 7);
                    assert_eq!(functional["owned_starts"], 4);
                    assert_eq!(functional["cold_done"], true);
                    assert_eq!(functional["a04_completed"], false);
                    fixture.cleanup()?;
                }
                Ok(())
            })
    })
}
