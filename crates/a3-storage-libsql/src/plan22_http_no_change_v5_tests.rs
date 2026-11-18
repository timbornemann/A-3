//! Native controlled V6/Window continuation, never local-model or P7 acceptance.
use super::*;
use std::sync::atomic::AtomicBool;

#[path = "plan22_http_no_change_v6_tests.rs"]
mod core_window_v6;

async fn prepared_v5(
    history: History,
) -> TestResult<(HttpEditFixture, PreparedHttpNoChangeReplanV6Job)> {
    let (f, page, old_live) = Box::pin(closed(history)).await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
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
            after_phases(&f).await?,
        )
        .await?
        .ok_or("closed origin")?;
    assert!(matches!(
        resume.state(),
        HttpNoChangeReplanResumeState::Unreserved
    ));
    let actual = WorkspaceAgentSourceReader
        .read_page(
            &f.http.fixture.project,
            page.revision(),
            &inspection(resume.closed())?,
            &ActiveControl,
        )
        .await?;
    let phase = PreparedHttpNoChangeReplanV6Job::prepare(
        &f.http.fixture.project,
        &f.http.goal,
        &f.stored,
        &f.run,
        &current,
        f.http.model.clone(),
        resume.closed().clone(),
        &actual,
    )?;
    let old_intent = match &phase.closed_origin().parent().producer {
        HttpCorrectionProducer::DirectV4Focus { proposal, .. } => proposal.intent(),
        HttpCorrectionProducer::ContextV1Intent { reply, .. } => {
            reply.intent().ok_or("old context intent")?
        }
    };
    assert!(phase.request().messages()[1].content().contains(&format!(
        "Prior model intent (unverified; no fact or proof): {}",
        old_intent.as_str()
    )));
    drop((current, old_live, page, resume));
    Ok((f, phase))
}
fn selection(phase: &PreparedHttpNoChangeReplanV6Job) -> TestResult<String> {
    let r = phase.source_page().range();
    let first = r.start_position().row() + 1;
    let last = r.end_position().row() + u32::from(r.end_position().column() > 0);
    if first >= last {
        return Err("native fixture needs at least two full lines".into());
    }
    // Controlled component fixture only: the handler response body is in W1.
    Ok(json!({"window":"W1",
        "intent":"Correct only the selected handler body and preserve the complete HTTP proof"})
    .to_string())
}
async fn shared(f: &HttpEditFixture) -> TestResult<EditPhaseReservedUsage> {
    let db = f.http.fixture.candidate_database().await?;
    let conn = db.connection_for_operation().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
        .await?;
    let (usage, _) = crate::edit_phase_reservation_repository::read_run_usage(
        &tx,
        f.http.fixture.project.worktree().id(),
        &f.run,
    )
    .await?;
    tx.commit().await?;
    Ok(usage)
}
struct NativeReplan<'a> {
    encoder: OllamaModelProvider,
    f: &'a HttpEditFixture,
    reply: String,
    invalid_initial: bool,
    finish_reason: ModelFinishReason,
    calls: AtomicUsize,
}
impl std::fmt::Debug for NativeReplan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeV5Control").finish_non_exhaustive()
    }
}
impl ModelProvider for NativeReplan<'_> {
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
            if scalar(
                self.f,
                "SELECT count(*) FROM http_no_change_replan_reservations WHERE outcome IS NULL",
            )
            .await
            .map_err(|_| ModelProviderFailure::Unavailable)?
                != 1
                || self.project_wire_request(r)? != p
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = if self.invalid_initial && call == 0 {
                "{}".to_owned()
            } else {
                self.reply.clone()
            };
            Ok(Box::pin(futures::stream::iter([
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(text)
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    self.finish_reason,
                    ModelProviderUsage::new(Some(128), Some(32)),
                ))),
            ])) as ProviderEventStream<'a>)
        })
    }
}
fn provider<'a>(
    f: &'a HttpEditFixture,
    reply: String,
    invalid_initial: bool,
) -> TestResult<NativeReplan<'a>> {
    Ok(NativeReplan {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        f,
        reply,
        invalid_initial,
        finish_reason: ModelFinishReason::Stop,
        calls: AtomicUsize::new(0),
    })
}
async fn execute_v5(
    f: &HttpEditFixture,
    phase: &PreparedHttpNoChangeReplanV6Job,
    p: &NativeReplan<'_>,
    kind: EditPhaseCallKind,
) -> Result<HttpNoChangeReplanExecutionReport, HttpNoChangeReplanExecutionFailure> {
    let now = after_phases(f)
        .await
        .map_err(|_| HttpNoChangeReplanExecutionFailure::Binding)?;
    ExecuteHttpNoChangeReplan::new(p, &WorkspaceAgentSourceReader, f.http.store.as_ref())
        .execute(
            &f.http.fixture.project,
            phase,
            kind,
            now,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        )
        .await
}
async fn apply_window(
    f: &mut HttpEditFixture,
    phase: &PreparedHttpNoChangeReplanV6Job,
) -> TestResult<HttpCorrectionPlanApplied> {
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let (mut ledger, version) = f.stored.clone().into_parts();
    let now = after_phases(f).await?;
    let applied = ApplyHttpNoChangeWindowPlan::new(
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
        phase,
        now,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, applied.version);
    Ok(applied)
}
async fn cold_v5(f: HttpEditFixture) -> TestResult<(HttpEditFixture, HttpNoChangeReplanResume)> {
    let f = reopen(f).await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("cold index")?;
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
            after_phases(&f).await?,
        )
        .await?
        .ok_or("cold V5")?;
    drop(current);
    Ok((f, resume))
}

#[test]
fn http_no_change_v6_regular_halts_settle_once_without_material_credit_or_repair() -> TestResult {
    crate::run_native_libsql_test(async {
        for halt in ["needEvidence", "tooLarge", "refuse"] {
            let (mut f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
            let before = Retained::capture(&f).await?;
            let old = shared(&f).await?;
            let inventory_before = inventory(&f.http.root)?;
            let reply = json!({"halt":halt}).to_string();
            let p = provider(&f, reply, false)?;
            let report = execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
            assert_eq!(p.calls.load(Ordering::SeqCst), 1);
            assert_eq!(
                report.settlement.deadline(),
                phase.closed_origin().fact().call().deadline()
            );
            let charged = shared(&f).await?;
            assert_eq!(charged.calls, old.calls + 1);
            assert_eq!(charged.repairs, old.repairs);
            assert_eq!(
                charged.prompt_tokens,
                old.prompt_tokens + u64::from(report.budget.prompt_tokens())
            );
            assert_eq!(
                charged.output_tokens,
                old.output_tokens + u64::from(report.budget.output_tokens())
            );
            let expected = "accepted_reply";
            assert_eq!(scalar(&f,&format!("SELECT count(*) FROM http_no_change_replan_reservations WHERE outcome='{expected}'")).await?,1);
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await,
                Err(HttpNoChangeReplanExecutionFailure::Reservation(
                    EditPhaseReservationFailure::AlreadyReserved
                ))
            ));
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::StructuralRepair).await,
                Err(HttpNoChangeReplanExecutionFailure::Reservation(
                    EditPhaseReservationFailure::AlreadyReserved
                ))
            ));
            assert_eq!(p.calls.load(Ordering::SeqCst), 1);
            drop(p);
            assert!(apply_window(&mut f, &phase).await.is_err());
            let (f, resume) = cold_v5(f).await?;
            let HttpNoChangeReplanResumeState::Observed(s) = resume.state() else {
                return Err("cold settlement".into());
            };
            assert_eq!(s.as_ref(), &report.settlement);
            assert_eq!(shared(&f).await?, charged);
            before
                .assert_same_with_usage(&f, &f.run, f.stored.ledger(), f.stored.version(), charged)
                .await?;
            assert_eq!(inventory(&f.http.root)?, inventory_before);
            drop((phase, resume));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn http_no_change_v5_real_single_repair_and_old_generation_caps_survive_cold_reopen() -> TestResult
{
    crate::run_native_libsql_test(async {
        for corrupt in [false, true] {
            let (f, phase) = Box::pin(prepared_v5(History::ContextRepair)).await?;
            let old = shared(&f).await?;
            let p = provider(&f, selection(&phase)?, true)?;
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await,
                Err(HttpNoChangeReplanExecutionFailure::InvalidReply)
            ));
            let report = execute_v5(&f, &phase, &p, EditPhaseCallKind::StructuralRepair).await?;
            assert_eq!(p.calls.load(Ordering::SeqCst), 2);
            assert_eq!(
                report.settlement.call_kind(),
                EditPhaseCallKind::StructuralRepair
            );
            let charged = shared(&f).await?;
            assert_eq!(charged.calls, old.calls + 2);
            assert_eq!(charged.repairs, old.repairs + 1);
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::StructuralRepair).await,
                Err(HttpNoChangeReplanExecutionFailure::Reservation(
                    EditPhaseReservationFailure::AlreadyReserved
                ))
            ));
            drop(p);
            if corrupt {
                let db = f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                conn.execute("DROP TRIGGER http_no_change_replan_update_guard", ())
                    .await?;
                assert_eq!(conn.execute("UPDATE http_no_change_replan_reservations SET reported_output=1025 WHERE call_kind='initial'",()).await?,1);
                drop((conn, db));
                let f = reopen(f).await?;
                let current = f
                    .http
                    .store
                    .latest_published_index(&f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
                assert!(matches!(
                    f.http
                        .store
                        .load_http_no_change_replan(
                            &f.http.fixture.project,
                            &f.http.goal,
                            &f.stored,
                            &f.run,
                            &current,
                            &f.http.model,
                            after_phases(&f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                drop(current);
                cleanup(f)?;
            } else {
                let (f, resume) = cold_v5(f).await?;
                let HttpNoChangeReplanResumeState::Observed(s) = resume.state() else {
                    return Err("cold selected repair".into());
                };
                assert_eq!(s.as_ref(), &report.settlement);
                assert_eq!(shared(&f).await?, charged);
                drop(resume);
                cleanup(f)?;
            }
        }
        Ok(())
    })
}

async fn applied_origin(f: &HttpEditFixture) -> TestResult<AppliedHttpNoChangeWindowOrigin> {
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    Ok(f.http
        .store
        .load_applied_http_no_change_window_origin(
            &f.http.fixture.project,
            &f.http.goal,
            &f.stored,
            &f.run,
            &current,
            &f.http.model,
            after_phases(f).await?,
        )
        .await?)
}
async fn own_window_read(
    f: &mut HttpEditFixture,
) -> TestResult<(PreparedEditPhase, CompiledEditPhaseRequest, AgentSourcePage)> {
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let (mut ledger, mut version) = f.stored.clone().into_parts();
    let now = after_phases(f).await?;
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
        now,
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
    let mut tool_id = blake3::Hasher::new_derive_key("a3.native-http-no-change-window-tool.v1");
    tool_id.update(f.run.id().as_bytes());
    tool_id.update(state(f, &current)?.anchor().step().as_bytes());
    let tool = ToolRunId::from_bytes(*tool_id.finalize().as_bytes());
    f.http
        .store
        .begin_source_read(
            &f.http.fixture.project,
            state(f, &current)?.anchor(),
            &bound,
            tool,
            after_phases(f).await?,
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
    let read_at = after_phases(f).await?;
    let mut event_id = blake3::Hasher::new_derive_key("a3.native-http-no-change-window-read.v1");
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
        .load_source_read_state(&f.http.fixture.project, state(f, &current)?.anchor())
        .await?;
    assert!(reads.progress().receipts().is_empty());
    let acquired = reads.acquired().ok_or("new actual B Original")?;
    assert_eq!(acquired.original(), required);
    assert_eq!(acquired.original().range(), page.range());
    let phase = PreparedEditPhase::update(state(f, &current)?, &page)?;
    let compiled = build_edit_phase_request(
        &f.http.goal,
        &f.stored,
        &phase,
        state(f, &current)?,
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
    assert!(
        compiled.request().messages()[1]
            .content()
            .contains("previous correction changed zero source bytes")
    );
    drop((reads, current));
    Ok((phase, compiled, page))
}
struct PatchedWindow {
    fixture: Fixture,
    run: AgentRunId,
    profile: ModelProfile,
    settings: LoopbackProcessSettings,
    floor: AgentRunTimestamp,
    reviewed: ContentHash,
}

async fn patch_controlled_window(mut f: HttpEditFixture) -> TestResult<PatchedWindow> {
    let old_run = f.run.id();
    let old_created = f.run.created_at();
    let old_budget = f.run.budget();
    let old_abandoned = scalar(
        &f,
        "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1",
    )
    .await?;
    let (phase, mut compiled, page) = Box::pin(own_window_read(&mut f)).await?;
    assert_eq!(f.run.id(), old_run);
    assert_eq!(f.run.created_at(), old_created);
    assert_eq!(f.run.budget(), old_budget);
    assert_eq!(
        scalar(
            &f,
            "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
        )
        .await?,
        old_abandoned
    );
    let f = reopen(f).await?;
    let origin = applied_origin(&f).await?;
    assert_eq!(
        f.run.last_event_sequence().get(),
        origin.material().closed_origin().closure().sequence().get() + 5
    );
    assert_eq!(
        &EvidenceRef::new(page.revision().clone(), page.range()),
        origin.material().selected_window()
    );
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let before = fs::read(f.http.root.join("server.py"))?;
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
    let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
        &f.http.compiler,
        phase,
        &mut compiled,
        state(&f, &current)?,
        after_phases(&f).await?,
        &patch_tool,
        &ActiveControl,
    ))
    .await?;
    // Actual W1 is a partial window. B neither parses it nor silently expands its contract.
    assert_eq!(page.text().len(), 197);
    assert_eq!(compiled.scope_frame_digest(), None);
    assert_eq!(compiled.scope_frame_index(), None);
    assert!(
        compiled.request().messages()[1]
            .content()
            .contains("Scope: \"server.py\":22-26; part method \"do_GET\".\n")
    );
    let report = ExecuteEditPhase::new(&provider, f.http.store.as_ref(), &patch_tool)
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                state(&f, &current)?,
                after_phases(&f).await?,
                &patch_tool,
                &ActiveControl,
            ))
            .await?,
            state(&f, &current)?,
            &compiled,
            after_phases(&f).await?,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let delivery = report.source_delivery.ok_or("own B source delivery")?;
    assert_eq!(delivery.source().range(), page.range());
    let EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("ordinary B patch proposal".into());
    };
    let runtime_profile = f.http.model.clone();
    let runtime_settings = f.http.settings;
    let floor = f.run.updated_at();
    let old_span = origin.material().closed_origin().fact().window().range();
    assert!(old_span.contains(page.range()));
    assert!(page.range().len() < old_span.len());
    drop((provider, current, origin, compiled));
    let fixture = Box::pin(super::super::super::patch(f, action)).await?;
    let expected = FROZEN_SOURCE.replacen("b\"Hello, world!\"", "b\"Hello, world!\\n\"", 1);
    let after = fs::read(fixture.root.join("worktree/server.py"))?;
    assert_eq!(after, expected.as_bytes());
    let span = page.range();
    let start = span.start_byte() as usize;
    let end = span.end_byte() as usize;
    assert_eq!(&after[..start], &before[..start]);
    assert_eq!(&after[start + corrected.len()..], &before[end..]);
    drop(page);
    Ok(PatchedWindow {
        fixture,
        run: old_run,
        profile: runtime_profile,
        settings: runtime_settings,
        floor,
        reviewed: hash(expected.as_bytes()),
    })
}

async fn finish_controlled_window(f: HttpEditFixture) -> TestResult {
    let patched = Box::pin(patch_controlled_window(f)).await?;
    let (fixture, report) = Box::pin(super::super::super::functional::finish_http(
        patched.fixture,
        patched.run,
        patched.profile,
        patched.settings,
        patched.floor,
        patched.reviewed,
    ))
    .await?;
    assert_eq!(report["fresh_exact_grants"], 12);
    assert_eq!(report["total_http_observations"], 7);
    assert_eq!(report["owned_starts"], 4);
    assert_eq!(report["cold_done"], true);
    assert_eq!(report["a04_completed"], false);
    fixture.cleanup()
}

#[test]
fn http_no_change_v5_direct_and_context_proper_subset_reach_cold_material_all_edges_own_read_patch_and_full_http()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                for history in [History::DirectEditRepair, History::ContextRepair] {
                    let (mut f, phase) = Box::pin(prepared_v5(history)).await?;
                    let origin = phase.closed_origin().clone();
                    let p = provider(&f, selection(&phase)?, false)?;
                    let report = execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
                    assert!(matches!(
                        report.settlement.decision(),
                        Some(HttpNoChangeReplanDecision::Window { .. })
                    ));
                    let old_usage = shared(&f).await?;
                    drop(p);
                    let material = apply_window(&mut f, &phase).await?;
                    assert_eq!(f.run.id(), origin.fact().owner().run);
                    assert_eq!(
                        f.run.last_event_sequence().get(),
                        origin.closure().sequence().get() + 1
                    );
                    assert_eq!(shared(&f).await?, old_usage);
                    let mut f = reopen(f).await?;
                    let cold = applied_origin(&f).await?;
                    assert_eq!(cold.applied_event().sequence(), f.run.last_event_sequence());
                    let current = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("index")?;
                    assert!(matches!(
                        f.http
                            .store
                            .load_closed_no_change_http_origin(
                                &f.http.fixture.project,
                                &f.http.goal,
                                &f.stored,
                                &f.run,
                                &current,
                                &f.http.model,
                                after_phases(&f).await?
                            )
                            .await,
                        Err(EditPhaseReservationFailure::StaleAnchor)
                    ));
                    assert!(matches!(
                        f.http
                            .store
                            .load_applied_http_correction_origin(
                                &f.http.fixture.project,
                                &f.http.goal,
                                &f.stored,
                                &f.run,
                                &current,
                                &f.http.model,
                                after_phases(&f).await?
                            )
                            .await,
                        Err(EditPhaseReservationFailure::StaleAnchor)
                    ));
                    drop(current);
                    let old_correction = f
                        .stored
                        .ledger()
                        .step(origin.fact().owner().step)
                        .ok_or("old correction")?;
                    assert_eq!(old_correction.status(), TaskStepStatus::Failed);
                    assert_eq!(
                        old_correction.retired_in_revision(),
                        Some(f.stored.ledger().revision())
                    );
                    let old_proof = f
                        .stored
                        .ledger()
                        .steps()
                        .find(|s| {
                            !s.is_active_plan_step()
                                && s.status() == TaskStepStatus::Pending
                                && s.definition().dependencies()
                                    == [StepDependency::new(origin.fact().owner().step)]
                        })
                        .ok_or("retired old proof")?;
                    assert!(old_proof.attempts().is_empty());
                    let plan = derive_http_no_change_window_material_plan(
                        cold.material(),
                        old_correction.definition(),
                        old_proof.definition(),
                    )?;
                    assert_eq!(plan.change().id(), material.correction);
                    assert_eq!(plan.proof().id(), material.proof);
                    let (steps, event, activation) = http_no_change_window_material_identity(
                        cold.material().closed_origin().fact(),
                        cold.material().settlement(),
                    )?;
                    assert_eq!(steps, [plan.change().id(), plan.proof().id()]);
                    assert_eq!(event, plan.event_id());
                    assert_eq!(activation, *plan.activation_events());
                    replay_http_no_change_replan_job(
                        &f.http.goal,
                        f.stored.ledger(),
                        &f.http.model,
                        cold.material().closed_origin(),
                        phase.source_page(),
                        cold.material().settlement(),
                    )?;
                    let actual = cold.material().settlement();
                    let wrong_job = HttpNoChangeReplanSettlement::from_recorded(
                        ContentHash::from_bytes([17; 32]),
                        actual.call_kind(),
                        actual.wire(),
                        actual.reserved_at(),
                        actual.deadline(),
                        actual.observation(),
                        actual.decision().cloned(),
                    )?;
                    assert!(
                        replay_http_no_change_replan_job(
                            &f.http.goal,
                            f.stored.ledger(),
                            &f.http.model,
                            cold.material().closed_origin(),
                            phase.source_page(),
                            &wrong_job
                        )
                        .is_err()
                    );
                    let mut wrong_decision = actual.decision().cloned().ok_or("window decision")?;
                    if let HttpNoChangeReplanDecision::Window { text_digest, .. } =
                        &mut wrong_decision
                    {
                        *text_digest = ContentHash::from_bytes([18; 32]);
                    }
                    let wrong_selection = HttpNoChangeReplanSettlement::from_recorded(
                        actual.job_digest(),
                        actual.call_kind(),
                        actual.wire(),
                        actual.reserved_at(),
                        actual.deadline(),
                        actual.observation(),
                        Some(wrong_decision),
                    )?;
                    assert!(
                        replay_http_no_change_replan_job(
                            &f.http.goal,
                            f.stored.ledger(),
                            &f.http.model,
                            cold.material().closed_origin(),
                            phase.source_page(),
                            &wrong_selection
                        )
                        .is_err()
                    );

                    let mut recipe = None;
                    if let VerificationTarget::LoopbackHttp(r) =
                        old_proof.definition().verification_spec().target()
                    {
                        recipe = Some(r.clone());
                    }
                    let VerificationTarget::LoopbackHttp(new_recipe) =
                        plan.proof().verification_spec().target()
                    else {
                        return Err("full B HTTP proof".into());
                    };
                    assert_eq!(recipe.as_ref(), Some(new_recipe));
                    // Two ordinary non-Execute edges, with an actual cold read at each N+2/N+3.
                    for (signal, event) in [
                        (
                            AgentControllerSignal::ReplanApplied,
                            plan.activation_events()[0],
                        ),
                        (
                            AgentControllerSignal::LocalizationComplete,
                            plan.activation_events()[1],
                        ),
                    ] {
                        let now = after_phases(&f).await?;
                        let mut next = f.run.clone();
                        let advance = AdvanceAgentController.execute(
                            &mut next,
                            signal,
                            event,
                            f.run.current_snapshot_id(),
                            now,
                            false,
                        )?;
                        AppendRunEvent::new(f.http.store.as_ref())
                            .execute(
                                &f.http.fixture.project,
                                f.run.last_event_sequence(),
                                &next,
                                advance.event(),
                            )
                            .await?;
                        f.run = next;
                        f = reopen(f).await?;
                        let prefix = applied_origin(&f).await?;
                        assert_eq!(prefix.applied_event().id(), cold.applied_event().id());
                    }
                    drop((plan, cold, phase, origin));
                    Box::pin(finish_controlled_window(f)).await?;
                }
                Ok(())
            })
    })
}

struct RepublishAtMaterial<'a> {
    inner: &'a LibsqlKnowledgeStore,
    fixture: &'a Fixture,
    fired: AtomicBool,
}
impl std::fmt::Debug for RepublishAtMaterial<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepublishAtMaterial")
            .finish_non_exhaustive()
    }
}
impl AgentActionStore for RepublishAtMaterial<'_> {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        self.inner
            .commit_ledger_action(project, version, sequence, ledger, run, event)
    }
    fn commit_http_no_change_window_plan_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        index: IndexRunId,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
        proof: &'a HttpNoChangeWindowMaterialProof,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async move {
            if !self.fired.swap(true, Ordering::SeqCst) {
                super::super::super::super::apply::republish_unchanged_index(self.fixture)
                    .await
                    .map_err(|_| AgentActionStoreFailure::Unavailable)?;
            }
            self.inner
                .commit_http_no_change_window_plan_action(
                    project, version, sequence, index, ledger, run, event, proof,
                )
                .await
        })
    }
}
#[test]
fn http_no_change_v5_material_immediate_cas_rejects_same_snapshot_new_index_without_partial_revision()
-> TestResult {
    crate::run_native_libsql_test(async {
        let (mut f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
        let p = provider(&f, selection(&phase)?, false)?;
        execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
        drop(p);
        let old = Retained::capture(&f).await?;
        let costs = shared(&f).await?;
        let files = inventory(&f.http.root)?;
        let current = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("index")?;
        let actions = RepublishAtMaterial {
            inner: f.http.store.as_ref(),
            fixture: &f.http.fixture,
            fired: AtomicBool::new(false),
        };
        let (mut ledger, version) = f.stored.clone().into_parts();
        let now = after_phases(&f).await?;
        let result = ApplyHttpNoChangeWindowPlan::new(
            f.http.store.as_ref(),
            &actions,
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
            now,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            result,
            Err(HttpNoChangeWindowFailure::Replan(
                ApplyAgentPlanRevisionError::Storage(AgentActionStoreFailure::SnapshotConflict)
            ))
        ));
        assert!(actions.fired.load(Ordering::SeqCst));
        assert_eq!(&ledger, f.stored.ledger());
        let latest = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("latest")?;
        assert_ne!(latest.run().id(), current.run().id());
        assert_eq!(latest.run().snapshot_id(), current.run().snapshot_id());
        old.assert_same(&f, &f.run, f.stored.ledger(), version)
            .await?;
        assert_eq!(shared(&f).await?, costs);
        assert_eq!(inventory(&f.http.root)?, files);
        drop((latest, current, phase));
        cleanup(f)
    })
}
#[test]
fn http_no_change_v5_sql_history_corruption_pending_unknown_and_original_deadline_never_fall_back()
-> TestResult {
    crate::run_native_libsql_test(async {
        for fault in [
            "anchor",
            "proposal-digest",
            "no-progress-tag",
            "pending",
            "unknown",
            "deadline",
        ] {
            let (f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
            let p = provider(&f, selection(&phase)?, false)?;
            execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
            drop(p);
            if fault != "deadline" {
                let db = f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                conn.execute("DROP TRIGGER http_no_change_replan_update_guard", ())
                    .await?;
                let sql = match fault {
                    "anchor" => {
                        "UPDATE http_no_change_replan_reservations SET anchor=zeroblob(length(anchor))"
                    }
                    "proposal-digest" => {
                        "UPDATE http_no_change_replan_reservations SET proposal_digest=zeroblob(32)"
                    }
                    "no-progress-tag" => {
                        "UPDATE http_no_change_replan_reservations SET outcome='no_progress'"
                    }
                    "pending" => {
                        "UPDATE http_no_change_replan_reservations SET outcome=NULL,finished_at=NULL,reported_prompt=NULL,reported_output=NULL,proposal=NULL,proposal_digest=NULL"
                    }
                    _ => {
                        "UPDATE http_no_change_replan_reservations SET outcome='unknown',proposal=NULL,proposal_digest=NULL"
                    }
                };
                assert_eq!(conn.execute(sql, ()).await?, 1);
                drop((conn, db));
            }
            let f = reopen(f).await?;
            let old = Retained::capture(&f).await?;
            let costs = shared(&f).await?;
            let files = inventory(&f.http.root)?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let now = if fault == "deadline" {
                phase.closed_origin().fact().call().deadline()
            } else {
                after_phases(&f).await?
            };
            let result = f
                .http
                .store
                .load_http_no_change_replan(
                    &f.http.fixture.project,
                    &f.http.goal,
                    &f.stored,
                    &f.run,
                    &current,
                    &f.http.model,
                    now,
                )
                .await;
            assert!(
                matches!(
                    result,
                    Err(EditPhaseReservationFailure::InvalidStoredData
                        | EditPhaseReservationFailure::UncertainProviderCall
                        | EditPhaseReservationFailure::DeadlineReached)
                ),
                "fault {fault}: {result:?}"
            );
            old.assert_same(&f, &f.run, f.stored.ledger(), f.stored.version())
                .await?;
            assert_eq!(shared(&f).await?, costs);
            assert_eq!(inventory(&f.http.root)?, files);
            drop((current, phase));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn http_no_change_v5_escaped_secret_with_stop_or_output_limit_is_rejected_without_repair_or_retention()
-> TestResult {
    crate::run_native_libsql_test(async {
        for reason in [ModelFinishReason::Stop, ModelFinishReason::OutputLimit] {
            let (f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
            let before = Retained::capture(&f).await?;
            let costs = shared(&f).await?;
            let files = inventory(&f.http.root)?;
            // Complete JSON even when the provider reports OutputLimit. A Value map
            // would overwrite the first decoded credential. No raw reply is retained.
            let reply = r#"{"halt":"refuse","extra":"\u0041KIAIOSFODNN7EXAMPLE","extra":"safe"}"#
                .to_owned();
            let mut p = provider(&f, reply, false)?;
            p.finish_reason = reason;
            let wire = p.project_wire_request(phase.request())?;
            let budget = PhaseWireBudget::fit(wire, phase.profile())?;
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await,
                Err(HttpNoChangeReplanExecutionFailure::UnsafeReply)
            ));
            assert_eq!(p.calls.load(Ordering::SeqCst), 1);
            assert_eq!(scalar(&f,"SELECT count(*) FROM http_no_change_replan_reservations WHERE outcome='rejected_reply' AND proposal IS NULL AND proposal_digest IS NULL").await?,1);
            assert!(matches!(
                execute_v5(&f, &phase, &p, EditPhaseCallKind::StructuralRepair).await,
                Err(HttpNoChangeReplanExecutionFailure::Reservation(
                    EditPhaseReservationFailure::AlreadyReserved
                ))
            ));
            assert_eq!(p.calls.load(Ordering::SeqCst), 1);
            drop(p);
            let charged = shared(&f).await?;
            assert_eq!(charged.calls, costs.calls + 1);
            assert_eq!(charged.repairs, costs.repairs);
            assert_eq!(
                charged.prompt_tokens,
                costs.prompt_tokens + u64::from(budget.prompt_tokens())
            );
            assert_eq!(
                charged.output_tokens,
                costs.output_tokens + u64::from(budget.output_tokens())
            );
            let (f, resume) = cold_v5(f).await?;
            let HttpNoChangeReplanResumeState::Observed(s) = resume.state() else {
                return Err("cold rejected reply".into());
            };
            assert_eq!(
                s.observation().outcome(),
                EditPhaseObservationOutcome::RejectedReply
            );
            assert!(s.decision().is_none());
            before
                .assert_same_with_usage(&f, &f.run, f.stored.ledger(), f.stored.version(), charged)
                .await?;
            assert_eq!(shared(&f).await?, charged);
            assert_eq!(inventory(&f.http.root)?, files);
            drop((resume, phase));
            cleanup(f)?;
        }
        Ok(())
    })
}

// A publication re-index may retain identical source bytes and SnapshotId.
// These races exercise the ordinary real read/edit boundaries after B material.
async fn index_race_window_lens(
    f: &HttpEditFixture,
    required: &EvidenceRef,
) -> TestResult<TaskLens> {
    let seed = TaskLensSeedText::try_from_string(
        "Read the accepted smaller current HTTP correction window".to_owned(),
    )?;
    Ok(CompileTaskLens::new(
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
    .await?)
}
async fn activated_index_race_window() -> TestResult<(
    HttpEditFixture,
    HttpNoChangeWindowEditDiagnostic,
    PublishedIndex,
    TaskLens,
)> {
    // This is a publication-race contract using the existing genuine16k producer;
    // the old Direct4k fixture halts at full-wire capacity before reaching the race.
    let (mut f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
    let p = provider(&f, selection(&phase)?, false)?;
    execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
    drop(p);
    apply_window(&mut f, &phase).await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let (mut ledger, mut version) = f.stored.clone().into_parts();
    let now = after_phases(&f).await?;
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
        now,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, version);
    let lens = index_race_window_lens(&f, diagnostic.required_source()).await?;
    drop(phase);
    Ok((f, diagnostic, current, lens))
}
async fn republish_index_race(
    f: &HttpEditFixture,
    old: &PublishedIndex,
) -> TestResult<PublishedIndex> {
    super::super::super::super::apply::republish_unchanged_index(&f.http.fixture).await?;
    let new = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("new index")?;
    assert_ne!(new.run().id(), old.run().id());
    assert_eq!(new.run().snapshot_id(), old.run().snapshot_id());
    Ok(new)
}

#[derive(Clone, Copy)]
enum IndexReadRace {
    Bind,
    Admission,
    Original,
}
#[test]
fn http_no_change_v5_same_snapshot_republication_rejects_descriptor_read_admission_and_original_commit()
-> TestResult {
    crate::run_native_libsql_test(async {
        for race in [
            IndexReadRace::Bind,
            IndexReadRace::Admission,
            IndexReadRace::Original,
        ] {
            let (mut f, diagnostic, current, lens) =
                Box::pin(activated_index_race_window()).await?;
            let files = inventory(&f.http.root)?;
            let original_count=scalar(&f,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?;
            let old = Retained::capture(&f).await?;
            if matches!(race, IndexReadRace::Bind) {
                let new = republish_index_race(&f, &current).await?;
                let new_lens = index_race_window_lens(&f, diagnostic.required_source()).await?;
                assert!(
                    diagnostic
                        .bind_source(&f.http.fixture.project, &new, &new_lens)
                        .is_err()
                );
                old.assert_same(&f, &f.run, f.stored.ledger(), f.stored.version())
                    .await?;
                drop((new, new_lens));
            } else {
                let bound = diagnostic.bind_source(&f.http.fixture.project, &current, &lens)?;
                let mut h =
                    blake3::Hasher::new_derive_key("a3.native-http-window-index-race-read.v1");
                h.update(f.run.id().as_bytes());
                let tool = ToolRunId::from_bytes(*h.finalize().as_bytes());
                if matches!(race, IndexReadRace::Admission) {
                    let new = republish_index_race(&f, &current).await?;
                    assert!(matches!(
                        f.http
                            .store
                            .begin_source_read(
                                &f.http.fixture.project,
                                state(&f, &current)?.anchor(),
                                &bound,
                                tool,
                                after_phases(&f).await?
                            )
                            .await,
                        Err(SourceReadFailure::Reservation(
                            EditPhaseReservationFailure::StaleAnchor
                        ))
                    ));
                    old.assert_same(&f, &f.run, f.stored.ledger(), f.stored.version())
                        .await?;
                    drop(new);
                } else {
                    f.http
                        .store
                        .begin_source_read(
                            &f.http.fixture.project,
                            state(&f, &current)?.anchor(),
                            &bound,
                            tool,
                            after_phases(&f).await?,
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
                    let page = bound.take_original(
                        &f.http.fixture.project,
                        &current,
                        tool,
                        &mut result,
                    )?;
                    let new = republish_index_race(&f, &current).await?;
                    let before = f.run.clone();
                    let previous = before.last_event_sequence();
                    let at = after_phases(&f).await?;
                    let mut event_id =
                        blake3::Hasher::new_derive_key("a3.native-http-window-index-race-event.v1");
                    event_id.update(tool.as_bytes());
                    let recorded = result.record(
                        &mut f.run,
                        RunEventId::from_bytes(*event_id.finalize().as_bytes()),
                        at,
                    )?;
                    assert!(
                        AppendAgentRead::new(f.http.store.as_ref())
                            .execute(&f.http.fixture.project, previous, &f.run, &recorded)
                            .await
                            .is_err()
                    );
                    assert_eq!(
                        f.http
                            .store
                            .load_agent_run(&f.http.fixture.project, before.id())
                            .await?,
                        Some(before.clone())
                    );
                    f.run = before;
                    assert_eq!(scalar(&f,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?,original_count);
                    assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE outcome='accepted_reply'").await?,0);
                    drop((page, new, recorded));
                }
            }
            assert_eq!(inventory(&f.http.root)?, files);
            assert_eq!(scalar(&f,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?,original_count);
            drop((diagnostic, current, lens));
            cleanup(f)?;
        }
        Ok(())
    })
}

struct RepublishDuringWindowEdit<'a> {
    inner: NativeEdit<'a>,
    fixture: &'a Fixture,
    fired: AtomicBool,
}
impl std::fmt::Debug for RepublishDuringWindowEdit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepublishDuringWindowEdit")
            .finish_non_exhaustive()
    }
}
impl ModelProvider for RepublishDuringWindowEdit<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.inner.provider_id()
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
        self.inner.project_wire_request(r)
    }
    fn stream_projected<'a>(
        &'a self,
        r: &'a ModelProviderRequest,
        p: ModelWireProjection,
        t: ModelRequestTimeout,
        c: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let stream = self.inner.stream_projected(r, p, t, c).await?;
            if self.fired.swap(true, Ordering::SeqCst) {
                return Err(ModelProviderFailure::Rejected);
            }
            super::super::super::super::apply::republish_unchanged_index(self.fixture)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            Ok(stream)
        })
    }
}
#[test]
fn http_no_change_v5_same_snapshot_republication_rejects_normal_edit_reserve_and_successful_finish()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async{
            for at_finish in [false,true]{
                let(mut f,phase)=Box::pin(prepared_v5(History::DirectEditRepair)).await?;
                let p=provider(&f,selection(&phase)?,false)?;
                execute_v5(&f,&phase,&p,EditPhaseCallKind::Initial).await?;
                drop(p);
                apply_window(&mut f,&phase).await?;
                let(edit,mut compiled,page)=Box::pin(own_window_read(&mut f)).await?;
                let current=f.http.store.latest_published_index(&f.http.fixture.project,&ActiveControl).await?.ok_or("current")?;
                let files=inventory(&f.http.root)?;
                let usage=shared(&f).await?;
                let calls=scalar(&f,"SELECT count(*) FROM edit_phase_reservations").await?;
                let provider=RepublishDuringWindowEdit{
                    inner:NativeEdit{encoder:OllamaModelProvider::for_projected_wire(OllamaEndpoint::parse("http://127.0.0.1:11434")?,Arc::new(LocalOnlyOllamaEndpointPolicy))?,
                        store:f.http.store.as_ref(),project:&f.http.fixture.project,run:f.run.id(),
                        reply:json!({"code":page.text().replace("b\"Hello, world!\"","b\"Hello, world!\\n\"")}).to_string(),calls:AtomicUsize::new(0)},
                    fixture:&f.http.fixture,fired:AtomicBool::new(false)};
                if !at_finish{drop(republish_index_race(&f,&current).await?);}
                let result=ExecuteEditPhase::new(&provider,f.http.store.as_ref(),&WorkspacePatchAdapter::with_change_window_preview())
                    .execute(
Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(&f.http.compiler, edit, &mut compiled, state(&f,&current)?, after_phases(&f).await?, &WorkspacePatchAdapter::with_change_window_preview(), &ActiveControl)).await?,
state(&f,&current)?,
&compiled,
after_phases(&f).await?,
ModelRequestTimeout::DEFAULT,
&ActiveControl,
).await;
                assert!(matches!(result,Err(EditPhaseExecutionFailure::Reservation(EditPhaseReservationFailure::StaleAnchor))));
                assert_eq!(provider.inner.calls.load(Ordering::SeqCst),usize::from(at_finish));
                assert_eq!(provider.fired.load(Ordering::SeqCst),at_finish);
                assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations").await?,calls+i64::from(at_finish));
                assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE outcome='accepted_reply'").await?,0);
                if at_finish{
                    assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE outcome='stale'").await?,1);
                    let after=shared(&f).await?;assert_eq!(after.calls,usage.calls+1);assert_eq!(after.unobserved_calls,0);assert_eq!(after.unknown_calls,0);
                }else{assert_eq!(shared(&f).await?,usage);}
                assert_eq!(inventory(&f.http.root)?,files);
                drop((provider,compiled,page,current,phase));
                cleanup(f)?;
            }
            Ok(())
        })
    })
}

#[path = "plan22_http_no_change_horizon_tests.rs"]
mod horizon;

#[test]
fn http_no_change_v5_deep_inspection_failure_with_stop_or_output_limit_has_no_repair_retention_or_refund()
-> TestResult {
    crate::run_native_libsql_test(async {
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        for reason in [ModelFinishReason::Stop, ModelFinishReason::OutputLimit] {
            for reply in [
                format!(r#"{{"halt":"refuse","early":"\u0041KIAIOSFODNN7EXAMPLE","pad":{deep}}}"#),
                format!(
                    r#"{{"halt":"refuse","x":"\u0041KIAIOSFODNN7EXAMPLE","x":"safe","pad":{deep}}}"#
                ),
                format!(r#"{{"pad":{deep},"late":"\u0041KIAIOSFODNN7EXAMPLE","halt":"refuse"}}"#),
            ] {
                assert!(reply.len() <= 512);
                let (mut f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
                let before = Retained::capture(&f).await?;
                let costs = shared(&f).await?;
                let files = inventory(&f.http.root)?;
                let mut p = provider(&f, reply, false)?;
                p.finish_reason = reason;
                let wire = p.project_wire_request(phase.request())?;
                let budget = PhaseWireBudget::fit(wire, phase.profile())?;
                assert!(matches!(
                    Box::pin(execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial)).await,
                    Err(HttpNoChangeReplanExecutionFailure::UnsafeInspection)
                ));
                assert_eq!(p.calls.load(Ordering::SeqCst), 1);
                assert_eq!(scalar(&f,"SELECT count(*) FROM http_no_change_replan_reservations WHERE outcome='rejected_reply' AND proposal IS NULL AND proposal_digest IS NULL").await?,1);
                for kind in [
                    EditPhaseCallKind::Initial,
                    EditPhaseCallKind::StructuralRepair,
                ] {
                    assert!(matches!(
                        Box::pin(execute_v5(&f, &phase, &p, kind)).await,
                        Err(HttpNoChangeReplanExecutionFailure::Reservation(
                            EditPhaseReservationFailure::AlreadyReserved
                        ))
                    ));
                }
                assert_eq!(p.calls.load(Ordering::SeqCst), 1);
                drop(p);
                let charged = shared(&f).await?;
                assert_eq!(charged.calls, costs.calls + 1);
                assert_eq!(charged.repairs, costs.repairs);
                assert_eq!(
                    charged.prompt_tokens,
                    costs.prompt_tokens + u64::from(budget.prompt_tokens())
                );
                assert_eq!(
                    charged.output_tokens,
                    costs.output_tokens + u64::from(budget.output_tokens())
                );
                assert!(Box::pin(apply_window(&mut f, &phase)).await.is_err());
                let (f, resume) = Box::pin(cold_v5(f)).await?;
                let HttpNoChangeReplanResumeState::Observed(s) = resume.state() else {
                    return Err("cold uninspectable reply".into());
                };
                assert_eq!(
                    s.observation().outcome(),
                    EditPhaseObservationOutcome::RejectedReply
                );
                assert!(s.decision().is_none());
                before
                    .assert_same_with_usage(
                        &f,
                        &f.run,
                        f.stored.ledger(),
                        f.stored.version(),
                        charged,
                    )
                    .await?;
                assert_eq!(shared(&f).await?, charged);
                assert_eq!(inventory(&f.http.root)?, files);
                drop((resume, phase));
                cleanup(f)?;
            }
        }
        Ok(())
    })
}

// Append only this snippet to the exact Source005 V5 native test module for red.
// It uses existing APIs and fails before a repair can be attempted on Source005.
#[test]
fn http_no_change_v5_deep_reply_must_not_be_settled_as_repairable_structure() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, phase) = Box::pin(prepared_v5(History::DirectEditRepair)).await?;
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        let reply =
            format!(r#"{{"halt":"refuse","early":"\u0041KIAIOSFODNN7EXAMPLE","pad":{deep}}}"#);
        assert!(reply.len() <= 512);
        let p = provider(&f, reply, false)?;
        let result = Box::pin(execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial)).await;
        assert!(result.is_err());
        assert_eq!(p.calls.load(Ordering::SeqCst), 1);
        assert_eq!(scalar(&f,"SELECT count(*) FROM http_no_change_replan_reservations WHERE outcome='invalid_structure'").await?, 0,
            "uninspectable reply must not grant the stored structural repair");
        drop((p, phase));
        cleanup(f)?;
        Ok(())
    })
}
