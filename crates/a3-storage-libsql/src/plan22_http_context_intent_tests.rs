//! Real S1 acquisition and metadata-only ContextIntent; Ready is not executable.
#![cfg(any(windows, target_os = "linux"))]
use super::*;
#[path = "plan22_http_context_apply_tests.rs"]
pub(super) mod context_application;
#[path = "plan22_http_context_intent_live.rs"]
mod live;
#[path = "plan22_http_same_run_correction_live.rs"]
mod same_run_live;
use a3_context::DeterministicAgentReadTools;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ContextFixture {
    f: edit::HttpEditFixture,
    phase: PreparedHttpContextIntent,
    origin: HttpContextReadOrigin,
    primary: AgentSourcePage,
    context: AgentSourcePage,
}

fn encoder() -> TestResult<OllamaModelProvider> {
    Ok(OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?)
}

async fn count(f: &edit::HttpEditFixture, sql: &str) -> TestResult<i64> {
    fixture_count(&f.http.fixture, sql).await
}

async fn fixture_count(fixture: &Fixture, sql: &str) -> TestResult<i64> {
    let db = fixture.candidate_database().await?;
    let conn = db.connection_for_operation().await?;
    let mut rows = conn.query(sql, ()).await?;
    Ok(rows
        .next()
        .await?
        .ok_or("native context count missing")?
        .get(0)?)
}

async fn rebuild_context(f: edit::HttpEditFixture) -> TestResult<ContextFixture> {
    let project = &f.http.fixture.project;
    let current = f
        .http
        .store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("cold context publication missing")?;
    let origin = f
        .http
        .store
        .load_http_context_read_origin(
            project,
            &f.http.goal,
            &f.stored,
            &f.run,
            &current,
            &f.http.model,
            edit::after_phases(&f).await?,
        )
        .await?;
    let parent = &origin.parent_anchor;
    let range = parent.source_range;
    let first = range.start_position().row() + 1;
    let last = range.end_position().row() + u32::from(range.end_position().column() > 0);
    let primary = WorkspaceAgentSourceReader
        .read_page(
            project,
            &parent.source,
            &AgentFileInspection::new(
                parent.source.path().clone(),
                AgentFileStartLine::new(first)?,
                AgentFileLineCount::new(u16::try_from(last - first + 1)?)?,
            ),
            &ActiveControl,
        )
        .await?;
    let offer = parent
        .source_offer
        .as_ref()
        .ok_or("cold S1 offer missing")?;
    let context = WorkspaceAgentSourceReader
        .read_page(
            project,
            offer.revision(),
            offer.inspection(),
            &ActiveControl,
        )
        .await?;
    let phase = PreparedHttpContextIntent::new(
        project,
        &f.http.goal,
        &f.stored,
        &f.run,
        &current,
        f.http.model.clone(),
        &origin,
        &primary,
        &context,
    )?;
    assert_eq!(primary.text().len(), 370);
    assert_eq!(context.text().len(), 719);
    assert_eq!(phase.anchor().primary_range, primary.range());
    assert!(primary.range().contains(phase.anchor().definition_range));
    assert_eq!(
        phase.core_focus().get(),
        phase.anchor().definition_range.start_position().row() + 1
    );
    assert_eq!(phase.anchor().context_range, context.range());
    assert_eq!(
        origin.acquired.scope(),
        SourceDeliveryScope::from_http_context(parent, origin.parent_job)
    );
    assert_eq!(origin.acquired.key(), origin.read_admission.key());
    Ok(ContextFixture {
        f,
        phase,
        origin,
        primary,
        context,
    })
}

async fn accepted_s1() -> TestResult<CurrentPlanFixture> {
    accepted_parent(HttpFixtureContext::SixteenK, EditPhaseCallKind::Initial).await
}

async fn accepted_parent(
    context: HttpFixtureContext,
    kind: EditPhaseCallKind,
) -> TestResult<CurrentPlanFixture> {
    let f = full_fixture(
        ollama_profile_with_context("qwen38-4k:latest", context)?,
        true,
    )
    .await?;
    preseed_parent_s1(f, kind).await
}

// Controlled fixture observation only; this helper never invokes a Plan model.
async fn preseed_parent_s1(
    f: CurrentPlanFixture,
    kind: EditPhaseCallKind,
) -> TestResult<CurrentPlanFixture> {
    let proposal = f.phase.decode_proposal(&reply(&f.phase, "S1")?)?;
    assert!(matches!(
        proposal.decision(),
        HttpCorrectionPlanDecision::NeedOfferedSource(_)
    ));
    let base = f.run.updated_at().unix_millis() + 1;
    let project = &f.http.fixture.project;
    let initial = f
        .http
        .store
        .reserve_http_correction_plan(
            project,
            &f.phase,
            EditPhaseCallKind::Initial,
            wire(&f.phase, EditPhaseCallKind::Initial)?,
            at(base)?,
        )
        .await?;
    if kind == EditPhaseCallKind::StructuralRepair {
        f.http
            .store
            .finish_http_correction_plan(
                project,
                &f.phase,
                initial,
                observation(EditPhaseObservationOutcome::InvalidStructure, base + 1)?,
                None,
            )
            .await?;
        let repair = f
            .http
            .store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                kind,
                wire(&f.phase, kind)?,
                at(base + 2)?,
            )
            .await?;
        f.http
            .store
            .finish_http_correction_plan(
                project,
                &f.phase,
                repair,
                observation(EditPhaseObservationOutcome::AcceptedReply, base + 3)?,
                Some(proposal),
            )
            .await?;
    } else {
        f.http
            .store
            .finish_http_correction_plan(
                project,
                &f.phase,
                initial,
                observation(EditPhaseObservationOutcome::AcceptedReply, base + 1)?,
                Some(proposal),
            )
            .await?;
    }
    Ok(f)
}

async fn bound_offer(
    f: &CurrentPlanFixture,
    current: &PublishedIndex,
) -> TestResult<BoundEditSource> {
    let offer = f
        .phase
        .anchor()
        .source_offer
        .as_ref()
        .ok_or("real S1 descriptor missing")?;
    bound_source(
        f,
        current,
        &EvidenceRef::new(offer.revision().clone(), offer.range()),
    )
    .await
}

async fn bound_source(
    f: &CurrentPlanFixture,
    current: &PublishedIndex,
    required: &EvidenceRef,
) -> TestResult<BoundEditSource> {
    let project = &f.http.fixture.project;
    let seed = TaskLensSeedText::try_from_string(
        "Read the offered current HTTP script context".to_owned(),
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
                required.revision().path().clone(),
            )],
        )?,
        TaskLensTokenBudget::new(4096)?,
        &ActiveControl,
    )
    .await?;
    Ok(BoundEditSource::for_file_span(
        project, current, &lens, required,
    )?)
}

async fn acquire_context() -> TestResult<ContextFixture> {
    acquire_context_from(accepted_s1().await?).await
}

async fn acquire_context_from(mut f: CurrentPlanFixture) -> TestResult<ContextFixture> {
    let parent = f.phase.anchor().clone();
    let parent_job = f.phase.job_digest();
    let proposal = f.phase.decode_proposal(&reply(&f.phase, "S1")?)?;
    let now = f.run.updated_at().unix_millis() + 3;
    let project = &f.http.fixture.project;
    let current = f
        .http
        .store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("context publication missing")?;
    let bound = bound_offer(&f, &current).await?;
    let tool = ToolRunId::from_bytes([242; 32]);
    let admission = f
        .http
        .store
        .admit_http_context_read(project, &f.phase, &bound, tool, at(now + 2)?)
        .await?;
    assert_eq!(admission.attempt().tool_run_id(), tool);
    assert!(matches!(
        f.http
            .store
            .admit_http_context_read(
                project,
                &f.phase,
                &bound,
                ToolRunId::from_bytes([243; 32]),
                at(now + 3)?
            )
            .await,
        Err(SourceReadFailure::Reservation(
            EditPhaseReservationFailure::AlreadyReserved
        ))
    ));
    let tools = DeterministicAgentReadTools::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let started = Instant::now();
    let mut read = tools
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
    assert_eq!(page.text().len(), 719);
    let expected = f.run.last_event_sequence();
    let read_at = at(now + 4 + u64::try_from(started.elapsed().as_millis())?)?;
    let recorded = read.record(&mut f.run, RunEventId::from_bytes([242; 32]), read_at)?;
    let read_event = recorded.event().clone();
    AppendAgentRead::new(f.http.store.as_ref())
        .execute(project, expected, &f.run, &recorded)
        .await?;
    assert!(matches!(
        f.http
            .store
            .admit_http_context_read(
                project,
                &f.phase,
                &bound,
                ToolRunId::from_bytes([244; 32]),
                at(read_at.unix_millis() + 1)?
            )
            .await,
        Err(SourceReadFailure::Reservation(
            EditPhaseReservationFailure::StaleAnchor
        ))
    ));
    drop((tools, bound, current, page));
    // The original PreparedHttpCorrectionPlan and all index/store owners are dropped here.
    let f = edit::cold(f).await?;
    let c = rebuild_context(f).await?;
    assert_eq!(c.origin.parent_anchor, parent);
    assert_eq!(c.origin.parent_job, parent_job);
    assert_eq!(c.origin.parent_proposal, proposal);
    assert_eq!(c.origin.read_admission.key(), admission.key());
    let retained = c.origin.read_admission.attempt();
    let admitted = admission.attempt();
    assert_eq!(retained.tool_run_id(), admitted.tool_run_id());
    assert_eq!(retained.attempt(), admitted.attempt());
    assert_eq!(retained.started_at(), admitted.started_at());
    assert_eq!(retained.run_id(), admitted.run_id());
    assert_eq!(retained.snapshot_id(), admitted.snapshot_id());
    assert_eq!(c.origin.read_event, read_event);
    assert_eq!(count(&c.f, "SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL AND abandoned=0").await?, 1);
    assert_eq!(
        count(&c.f, "SELECT count(*) FROM tool_run_attempts").await?,
        2
    );
    Ok(c)
}

async fn cold_context(c: ContextFixture) -> TestResult<ContextFixture> {
    let ContextFixture {
        f,
        phase,
        origin,
        primary,
        context,
    } = c;
    drop((phase, origin, primary, context));
    rebuild_context(edit::reopen(f).await?).await
}

async fn assert_metadata_only(c: &ContextFixture) -> TestResult {
    let project = &c.f.http.fixture.project;
    assert_eq!(c.f.run.state(), AgentControllerState::Replan);
    assert_eq!(
        c.f.run.last_event_sequence(),
        c.origin.read_event.sequence()
    );
    assert_eq!(
        c.f.http.store.load_agent_run(project, c.f.run.id()).await?,
        Some(c.f.run.clone())
    );
    assert_eq!(
        c.f.http
            .store
            .load_task_ledger(project, c.f.http.goal.task_id())
            .await?,
        Some(c.f.stored.clone())
    );
    assert_eq!(
        c.f.http
            .store
            .load_current_goal_contract(project, c.f.http.goal.task_id())
            .await?,
        Some(c.f.http.goal.clone())
    );
    assert_eq!(inventory(&c.f.http.root)?, c.f.http.before);
    assert_eq!(
        count(&c.f, "PRAGMA user_version").await?,
        i64::from(crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION)
    );
    assert_eq!(
        count(&c.f, "SELECT count(*) FROM source_read_acquisitions").await?,
        1
    );
    assert_eq!(
        count(
            &c.f,
            "SELECT count(*) FROM http_correction_plan_reservations WHERE outcome='accepted_reply'"
        )
        .await?,
        1
    );
    Ok(())
}

fn assert_settlement(
    c: &ContextFixture,
    settled: &HttpContextIntentSettlement,
    reply: &HttpContextIntentReply,
    wire: ContentHash,
) -> TestResult {
    assert_eq!(&settled.reply, reply);
    assert!(settled.progress.pending().is_none());
    let [receipt] = settled.progress.receipts() else {
        return Err("one actual S1 delivery receipt required".into());
    };
    assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NewBytes(719));
    assert_eq!(
        receipt.admission().scope(),
        SourceDeliveryScope::from_http_context(&c.origin.parent_anchor, c.origin.parent_job)
    );
    let delivery = receipt
        .delivery()
        .ok_or("accepted S1 wire metadata missing")?;
    assert_eq!(delivery.wire_digest(), wire);
    assert_eq!(delivery.source().range(), c.context.range());
    assert_eq!(delivery.source().revision(), c.context.revision());
    assert_eq!(settled.primary_delivery.wire_digest(), wire);
    assert_eq!(settled.primary_delivery.source().range(), c.primary.range());
    assert_eq!(
        settled.primary_delivery.source().revision(),
        c.primary.revision()
    );
    Ok(())
}

fn cleanup(c: ContextFixture) -> TestResult {
    let ContextFixture {
        f,
        phase,
        origin,
        primary,
        context,
    } = c;
    drop((phase, origin, primary, context));
    let PreparedHttpFixture {
        fixture,
        store,
        refresh,
        compiler,
        ..
    } = f.http;
    drop((store, refresh, compiler));
    fixture.cleanup()
}

#[derive(Debug)]
struct ObservedContextProvider<'a> {
    inner: OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    run: AgentRunId,
    null: bool,
    invalid_initial: bool,
    reported_output_tokens: u64,
    calls: AtomicUsize,
}
impl ModelProvider for ObservedContextProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(&self.inner)
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
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .store
                .load_edit_phase_usage(self.project, self.run)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1 || self.project_wire_request(request)? != projection {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = if self.invalid_initial && call == 0 {
                "{}".to_owned()
            } else if self.null {
                json!({"intent": null}).to_string()
            } else {
                json!({"intent": "Adjust the current handler to return the required response body."}).to_string()
            };
            let events = futures::stream::iter([
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(text)
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(64), Some(self.reported_output_tokens)),
                ))),
            ]);
            Ok(Box::pin(events) as ProviderEventStream<'a>)
        })
    }
}

fn controlled_provider(
    c: &ContextFixture,
    null: bool,
    invalid_initial: bool,
) -> TestResult<ObservedContextProvider<'_>> {
    Ok(ObservedContextProvider {
        inner: encoder()?,
        store: c.f.http.store.as_ref(),
        project: &c.f.http.fixture.project,
        run: c.f.run.id(),
        null,
        invalid_initial,
        reported_output_tokens: 16,
        calls: AtomicUsize::new(0),
    })
}

async fn execute_context(
    c: &ContextFixture,
    provider: &ObservedContextProvider<'_>,
    kind: EditPhaseCallKind,
) -> Result<HttpContextIntentExecutionReport, HttpContextIntentExecutionFailure> {
    let now = edit::after_phases(&c.f)
        .await
        .map_err(|_| HttpContextIntentExecutionFailure::Binding)?;
    ExecuteHttpContextIntent::new(
        provider,
        &WorkspaceAgentSourceReader,
        c.f.http.store.as_ref(),
    )
    .execute(
        &c.f.http.fixture.project,
        &c.phase,
        kind,
        now,
        ModelRequestTimeout::from_millis(10000)
            .map_err(|_| HttpContextIntentExecutionFailure::Binding)?,
        &ActiveControl,
    )
    .await
}

#[test]
fn http_context_real_s1_cold_origin_ready_and_wrong_parent_metadata() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let encoder = encoder()?;
        for request in [
            c.phase.request_for(EditPhaseCallKind::Initial)?,
            c.phase.request_for(EditPhaseCallKind::StructuralRepair)?,
            c.phase
                .for_truncated_output_repair()?
                .request_for(EditPhaseCallKind::StructuralRepair)?,
        ] {
            let actual = request
                .messages()
                .iter()
                .map(|m| m.content())
                .collect::<String>();
            assert!(actual.contains(c.context.text()));
            assert!(actual.contains(c.primary.text()));
            let users = request
                .messages()
                .iter()
                .filter(|message| message.role() == ModelMessageRole::User)
                .collect::<Vec<_>>();
            assert_eq!(users.len(), 1);
            let (objective, original) = users[0]
                .content()
                .split_once("\n\nOriginal goal and rules (all mandatory):\n")
                .ok_or("local correction objective missing")?;
            let definition = c.phase.anchor().definition_range;
            let first = c.phase.core_focus().get();
            let last =
                definition.end_position().row() + u32::from(definition.end_position().column() > 0);
            assert!(objective.starts_with("Current correction objective:\n"));
            assert!(objective.contains(&format!(
                "callable \"do_GET\" in \"server.py\", lines {first}-{last}"
            )));
            assert!(objective.contains("observed FreeExplicit GET / mismatch"));
            assert!(objective.contains("Full original HTTP proof remains unverified."));
            assert!(!objective.contains("Hello, world!") && !objective.contains("following LF"));
            assert!(original.starts_with("Goal:\n"));
            assert!(
                original
                    .contains("actual/expected status200/200; bodyBytes13/14; bodyDifferenttrue")
            );
            for criterion in c.f.http.goal.draft().acceptance_criteria() {
                assert!(actual.contains(criterion.statement().as_str()));
            }
            for constraint in c.f.http.goal.draft().constraints() {
                assert!(actual.contains(constraint.as_str()));
            }
            let projection = encoder.project_wire_request(&request)?;
            assert_eq!(
                (projection.context_tokens(), projection.output_tokens()),
                (16384, 1024)
            );
            assert_eq!(
                PhaseWireBudget::fit(projection, request.profile())?.output_tokens(),
                3605
            );
        }
        let provider = controlled_provider(&c, false, false)?;
        let report = execute_context(&c, &provider, EditPhaseCallKind::Initial).await?;
        assert!(report.reply.intent().is_some());
        assert_eq!(report.reply.job_digest(), c.phase.job_digest());
        assert_eq!(report.source_delivery.source().range(), c.context.range());
        assert_eq!(
            report.source_delivery.source().revision(),
            c.context.revision()
        );
        assert_eq!(
            report.source_delivery.wire_digest(),
            report.projection.digest()
        );
        assert_eq!(report.primary_delivery.source().range(), c.primary.range());
        assert_eq!(
            report.primary_delivery.source().revision(),
            c.primary.revision()
        );
        assert_eq!(
            report.primary_delivery.wire_digest(),
            report.projection.digest()
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::AlreadyReserved
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let settled_wire = report.projection.digest();
        let reply = report.reply;
        let job = c.phase.job_digest();
        drop(provider);
        assert_metadata_only(&c).await?;
        let c = cold_context(c).await?;
        assert_eq!(c.phase.job_digest(), job);
        let store = c.f.http.store.as_ref();
        let settled = store
            .load_http_context_intent_for_application(
                &c.f.http.fixture.project,
                &c.phase,
                edit::after_phases(&c.f).await?,
            )
            .await?
            .ok_or("cold ready settlement missing")?;
        assert_settlement(&c, &settled, &reply, settled_wire)?;
        assert_metadata_only(&c).await?;
        // Controlled stored corruption: the loader must compare parent metadata independently.
        let db = c.f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        conn.execute("DROP TRIGGER http_context_intent_update_guard", ())
            .await?;
        conn.execute(
            "UPDATE http_context_intent_reservations SET parent_proposal_digest=zeroblob(32)",
            (),
        )
        .await?;
        assert!(matches!(
            store
                .load_http_context_intent_for_application(
                    &c.f.http.fixture.project,
                    &c.phase,
                    edit::after_phases(&c.f).await?
                )
                .await,
            Err(EditPhaseReservationFailure::InvalidStoredData)
        ));
        conn.execute(
            "UPDATE http_context_intent_reservations SET parent_proposal_digest=?1",
            libsql::params![c.phase.anchor().parent_proposal_digest.as_bytes().to_vec()],
        )
        .await?;
        conn.execute("DROP TRIGGER source_read_identity_immutable", ())
            .await?;
        let wrong_owner = conn
            .execute(
                "UPDATE source_read_acquisitions SET original_evidence_id=zeroblob(32)",
                (),
            )
            .await;
        assert!(
            matches!(wrong_owner, Err(libsql::Error::SqliteFailure(code, _)) if code & 0xff == 19)
        );
        let still_valid = store
            .load_http_context_intent_for_application(
                &c.f.http.fixture.project,
                &c.phase,
                edit::after_phases(&c.f).await?,
            )
            .await?
            .ok_or("FK rejection lost the real settlement")?;
        assert_settlement(&c, &still_valid, &reply, settled_wire)?;
        conn.execute(
            "UPDATE source_read_acquisitions SET scope_digest=zeroblob(32)",
            (),
        )
        .await?;
        assert!(matches!(
            store
                .load_http_context_intent_for_application(
                    &c.f.http.fixture.project,
                    &c.phase,
                    edit::after_phases(&c.f).await?
                )
                .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        drop((conn, db));
        cleanup(c)
    })
}

#[test]
fn http_context_null_is_settled_cold_metadata_without_semantic_retry() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let provider = controlled_provider(&c, true, false)?;
        let report = execute_context(&c, &provider, EditPhaseCallKind::Initial).await?;
        assert!(report.reply.intent().is_none());
        let canonical: serde_json::Value =
            serde_json::from_slice(&report.reply.canonical_retained_bytes()?)?;
        assert!(
            canonical
                .get("intent")
                .is_some_and(serde_json::Value::is_null)
        );
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::StructuralRepair).await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::RepairNotAvailable
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let settled_wire = report.projection.digest();
        let reply = report.reply;
        drop(provider);
        let c = cold_context(c).await?;
        let settled =
            c.f.http
                .store
                .load_http_context_intent_for_application(
                    &c.f.http.fixture.project,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                )
                .await?
                .ok_or("cold null settlement missing")?;
        assert_settlement(&c, &settled, &reply, settled_wire)?;
        assert!(settled.reply.intent().is_none());
        let usage =
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?;
        assert_eq!(
            (
                usage.calls,
                usage.repairs,
                usage.unobserved_calls,
                usage.unknown_calls
            ),
            (2, 0, 0, 0)
        );
        assert_metadata_only(&c).await?;
        cleanup(c)
    })
}

#[test]
fn http_context_invalid_structure_allows_only_one_same_job_repair() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let provider = controlled_provider(&c, false, true)?;
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::InvalidReply)
        ));
        let report = execute_context(&c, &provider, EditPhaseCallKind::StructuralRepair).await?;
        assert!(report.reply.intent().is_some());
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::StructuralRepair).await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::AlreadyReserved
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            2
        );
        assert_eq!(
            count(
                &c.f,
                "SELECT count(DISTINCT job_digest) FROM http_context_intent_reservations"
            )
            .await?,
            1
        );
        assert_eq!(
            count(
                &c.f,
                "SELECT count(DISTINCT wire_digest) FROM http_context_intent_reservations"
            )
            .await?,
            2
        );
        let usage =
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?;
        assert_eq!(
            (
                usage.calls,
                usage.repairs,
                usage.unobserved_calls,
                usage.unknown_calls
            ),
            (3, 1, 0, 0)
        );
        drop(provider);
        let c = cold_context(c).await?;
        let settled =
            c.f.http
                .store
                .load_http_context_intent_for_application(
                    &c.f.http.fixture.project,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                )
                .await?
                .ok_or("cold repaired settlement missing")?;
        assert_settlement(&c, &settled, &report.reply, report.projection.digest())?;
        assert_metadata_only(&c).await?;
        cleanup(c)
    })
}

#[test]
fn http_context_read_without_intake_link_still_counts_interrupted_action_attempts() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = accepted_s1().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("budget context publication missing")?;
        let bound = bound_offer(&f, &current).await?;
        assert_eq!(f.run.budget().action_limit().get(), 8);
        assert_eq!(
            fixture_count(
                &f.http.fixture,
                "SELECT count(*) FROM intake_phase_budget_scopes WHERE linked_run_id IS NOT NULL"
            )
            .await?,
            0
        );
        assert_eq!(
            fixture_count(&f.http.fixture, "SELECT count(*) FROM tool_run_attempts").await?,
            1
        );
        let now = f.run.updated_at().unix_millis() + 5;
        // Genuine charged attempts without result/evidence; interruption never refunds an action.
        for slot in 0_u8..7 {
            f.http
                .store
                .begin_agent_tool_attempt(
                    project,
                    f.run.id(),
                    current.run().snapshot_id(),
                    ToolRunId::from_bytes([150 + slot; 32]),
                    at(now + u64::from(slot))?,
                )
                .await?;
        }
        assert_eq!(
            f.http
                .store
                .interrupt_agent_tool_attempts(project, f.run.id(), at(now + 8)?)
                .await?,
            7
        );
        assert_eq!(
            fixture_count(
                &f.http.fixture,
                "SELECT count(*) FROM tool_run_attempts WHERE status='interrupted'"
            )
            .await?,
            7
        );
        assert!(matches!(
            f.http
                .store
                .admit_http_context_read(
                    project,
                    &f.phase,
                    &bound,
                    ToolRunId::from_bytes([245; 32]),
                    at(now + 9)?
                )
                .await,
            Err(SourceReadFailure::Reservation(
                EditPhaseReservationFailure::BudgetExhausted(AgentBudgetDimension::Actions)
            ))
        ));
        assert_eq!(
            fixture_count(&f.http.fixture, "SELECT count(*) FROM tool_run_attempts").await?,
            8
        );
        assert_eq!(
            fixture_count(
                &f.http.fixture,
                "SELECT count(*) FROM source_read_acquisitions"
            )
            .await?,
            0
        );
        assert_eq!(
            fixture_count(
                &f.http.fixture,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            0
        );
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
        assert_eq!(f.run.last_event_sequence(), f.phase.anchor().sequence);
        assert_eq!(inventory(&f.http.root)?, f.http.before);
        drop((current, bound));
        let CurrentPlanFixture { http, phase, .. } = f;
        drop(phase);
        let PreparedHttpFixture {
            fixture,
            store,
            refresh,
            compiler,
            ..
        } = http;
        drop((store, refresh, compiler));
        fixture.cleanup()
    })
}

#[test]
fn http_context_sql_keeps_immutable_rows_and_one_accepted_reply_per_actual_read() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let provider = controlled_provider(&c, false, false)?;
        execute_context(&c, &provider, EditPhaseCallKind::Initial).await?;
        drop(provider);
        let db = c.f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        assert!(
            matches!(conn.execute("UPDATE http_context_intent_reservations SET wire_digest=zeroblob(32)",()).await,
            Err(libsql::Error::SqliteFailure(code,_)) if code & 0xff == 19)
        );
        assert!(
            matches!(conn.execute("DELETE FROM http_context_intent_reservations",()).await,
            Err(libsql::Error::SqliteFailure(code,_)) if code & 0xff == 19)
        );
        // Lifecycle-valid fault probe: preserve every guard and index, insert only a pending repair.
        assert_eq!(conn.execute(
            "INSERT INTO http_context_intent_reservations(worktree_id,run_id,diagnostic_digest,call_kind,
             parent_call_kind,parent_job_digest,parent_proposal_digest,source_read_id,job_digest,
             protocol_version,anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,
             deadline,outcome,finished_at,reported_prompt,reported_output,reply,reply_digest)
             SELECT worktree_id,run_id,diagnostic_digest,'structural_repair',parent_call_kind,
             parent_job_digest,parent_proposal_digest,source_read_id,job_digest,protocol_version,anchor,
             zeroblob(32),wire_bytes,prompt_reserve,output_reserve,finished_at+1,deadline,
             NULL,NULL,NULL,NULL,NULL,NULL FROM http_context_intent_reservations WHERE call_kind='initial'", ()).await?,1);
        let second_accepted = conn.execute(
            "UPDATE http_context_intent_reservations SET outcome='accepted_reply',finished_at=reserved_at+1,
             reported_prompt=64,reported_output=16,
             reply=(SELECT reply FROM http_context_intent_reservations WHERE call_kind='initial'),
             reply_digest=(SELECT reply_digest FROM http_context_intent_reservations WHERE call_kind='initial')
             WHERE call_kind='structural_repair'", ()).await;
        assert!(
            matches!(second_accepted,Err(libsql::Error::SqliteFailure(code,_)) if code & 0xff == 19)
        );
        assert_eq!(count(&c.f,"SELECT count(*) FROM http_context_intent_reservations WHERE outcome='accepted_reply'").await?,1);
        assert_eq!(count(&c.f,"SELECT count(*) FROM http_context_intent_reservations WHERE call_kind='structural_repair' AND outcome IS NULL").await?,1);
        assert_eq!(count(&c.f,"SELECT count(*) FROM sqlite_schema WHERE type='index' AND name='http_context_intent_one_accepted_per_source_read'").await?,1);
        drop((conn, db));
        assert_metadata_only(&c).await?;
        cleanup(c)
    })
}

#[test]
fn http_context_selected_parent_repair_own_usage_and_deadline_are_checked_warm_and_cold()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (corrupt, restore) in [
            (
                "UPDATE http_correction_plan_reservations SET reported_output=output_reserve+1 WHERE call_kind='structural_repair'",
                "UPDATE http_correction_plan_reservations SET reported_output=16 WHERE call_kind='structural_repair'",
            ),
            (
                "UPDATE http_correction_plan_reservations SET deadline=deadline+1 WHERE call_kind='structural_repair'",
                "UPDATE http_correction_plan_reservations SET deadline=deadline-1 WHERE call_kind='structural_repair'",
            ),
        ] {
            let f = accepted_parent(
                HttpFixtureContext::SixteenK,
                EditPhaseCallKind::StructuralRepair,
            )
            .await?;
            let project = &f.http.fixture.project;
            let current = f
                .http
                .store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("repair parent publication missing")?;
            let bound = bound_offer(&f, &current).await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute("DROP TRIGGER http_correction_plan_update_guard", ())
                .await?;
            conn.execute(corrupt, ()).await?;
            assert!(matches!(
                f.http
                    .store
                    .admit_http_context_read(
                        project,
                        &f.phase,
                        &bound,
                        ToolRunId::from_bytes([246; 32]),
                        at(f.run.updated_at().unix_millis() + 10)?
                    )
                    .await,
                Err(SourceReadFailure::Reservation(
                    EditPhaseReservationFailure::InvalidStoredData
                ))
            ));
            assert_eq!(
                fixture_count(
                    &f.http.fixture,
                    "SELECT count(*) FROM source_read_acquisitions"
                )
                .await?,
                0
            );
            // Restore only the faulted receipt, then obtain a genuine R for the cold negative.
            conn.execute(restore, ()).await?;
            drop((bound, current, conn, db));
            let c = acquire_context_from(f).await?;
            let db = c.f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute(corrupt, ()).await?;
            drop((conn, db));
            let ContextFixture {
                f,
                phase,
                origin,
                primary,
                context,
            } = c;
            drop((phase, origin, primary, context));
            let f = edit::reopen(f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold repair parent publication missing")?;
            assert!(matches!(
                f.http
                    .store
                    .load_http_context_read_origin(
                        &f.http.fixture.project,
                        &f.http.goal,
                        &f.stored,
                        &f.run,
                        &current,
                        &f.http.model,
                        edit::after_phases(&f).await?
                    )
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            ));
            assert_eq!(count(&f,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?,1);
            assert_eq!(
                count(&f, "SELECT count(*) FROM http_context_intent_reservations").await?,
                0
            );
            drop(current);
            cleanup_open(f)?;
        }
        Ok(())
    })
}

fn cleanup_open(f: edit::HttpEditFixture) -> TestResult {
    let PreparedHttpFixture {
        fixture,
        store,
        refresh,
        compiler,
        ..
    } = f.http;
    drop((store, refresh, compiler));
    fixture.cleanup()
}

#[test]
fn http_context_joint_full_source_four_k_capacity_precedes_any_context_debit_or_provider_io()
-> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context_from(
            accepted_parent(HttpFixtureContext::FourK, EditPhaseCallKind::Initial).await?,
        )
        .await?;
        let request = c.phase.request();
        let actual = request
            .messages()
            .iter()
            .map(|m| m.content())
            .collect::<String>();
        assert!(actual.contains(c.primary.text()) && actual.contains(c.context.text()));
        for constraint in c.f.http.goal.draft().constraints() {
            assert!(actual.contains(constraint.as_str()));
        }
        for criterion in c.f.http.goal.draft().acceptance_criteria() {
            assert!(actual.contains(criterion.statement().as_str()));
        }
        let projection = encoder()?.project_wire_request(request)?;
        assert_eq!(
            (projection.context_tokens(), projection.output_tokens()),
            (4096, 1024)
        );
        assert!(matches!(
            PhaseWireBudget::fit(projection, request.profile()),
            Err(PhaseWireBudgetError::Capacity)
        ));
        let provider = controlled_provider(&c, false, false)?;
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::Wire(
                PhaseWireBudgetError::Capacity
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            0
        );
        assert_eq!(
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?
                .calls,
            1
        );
        drop(provider);
        assert_metadata_only(&c).await?;
        cleanup(c)
    })
}

#[derive(Debug)]
struct ContextCancelled;
impl AgentSourceReadControl for ContextCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}
impl ModelOperationControl for ContextCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(async {})
    }
}

#[test]
fn http_context_cancel_deadline_stale_file_and_new_publication_stop_before_dispatch() -> TestResult
{
    crate::run_native_libsql_test(async {
        let mut c = acquire_context().await?;
        let provider = controlled_provider(&c, false, false)?;
        let project = &c.f.http.fixture.project;
        let now = edit::after_phases(&c.f).await?;
        assert!(matches!(
            ExecuteHttpContextIntent::new(
                &provider,
                &WorkspaceAgentSourceReader,
                c.f.http.store.as_ref()
            )
            .execute(
                project,
                &c.phase,
                EditPhaseCallKind::Initial,
                now,
                ModelRequestTimeout::DEFAULT,
                &ContextCancelled
            )
            .await,
            Err(HttpContextIntentExecutionFailure::Provider(
                ModelProviderFailure::Cancelled
            ))
        ));
        let deadline =
            at(c.f.run.created_at().unix_millis() + c.f.run.budget().duration_limit().millis())?;
        assert!(matches!(
            ExecuteHttpContextIntent::new(
                &provider,
                &WorkspaceAgentSourceReader,
                c.f.http.store.as_ref()
            )
            .execute(
                project,
                &c.phase,
                EditPhaseCallKind::Initial,
                deadline,
                ModelRequestTimeout::DEFAULT,
                &ActiveControl
            )
            .await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::DeadlineReached
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        fs::write(
            c.f.http.root.join("server.py"),
            format!("{FROZEN_SOURCE}\n# user edit\n"),
        )?;
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::Source(
                AgentSourceReadFailure::Stale
            ))
        ));
        fs::write(c.f.http.root.join("server.py"), FROZEN_SOURCE)?;
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        drop(provider);
        assert_metadata_only(&c).await?;
        // A new unrelated publication invalidates the prepared snapshot even with identical script bytes.
        fs::write(
            c.f.http.root.join("note.txt"),
            "new unrelated publication\n",
        )?;
        c.f.http
            .refresh
            .execute(
                &c.f.http.fixture.project,
                &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
                &mut c.f.http.compiler,
                &ActiveControl,
            )
            .await?;
        c.phase
            .revalidate_sources(
                &c.f.http.fixture.project,
                &WorkspaceAgentSourceReader,
                &ActiveControl,
            )
            .await?;
        let provider = controlled_provider(&c, false, false)?;
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::StaleAnchor
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            0
        );
        assert_eq!(
            c.f.http
                .store
                .load_agent_run(&c.f.http.fixture.project, c.f.run.id())
                .await?,
            Some(c.f.run.clone())
        );
        assert_eq!(c.f.run.state(), AgentControllerState::Replan);
        drop(provider);
        cleanup(c)
    })
}

#[test]
fn http_context_wrong_offer_tool_and_interrupted_admission_never_create_an_original_or_second_read()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = accepted_s1().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("interrupted read publication missing")?;
        let wrong = bound_source(
            &f,
            &current,
            &EvidenceRef::new(
                f.phase.anchor().source.clone(),
                f.phase.anchor().source_range,
            ),
        )
        .await?;
        let now = f.run.updated_at().unix_millis() + 6;
        assert!(matches!(
            f.http
                .store
                .admit_http_context_read(
                    project,
                    &f.phase,
                    &wrong,
                    ToolRunId::from_bytes([247; 32]),
                    at(now)?
                )
                .await,
            Err(SourceReadFailure::Reservation(
                EditPhaseReservationFailure::StaleAnchor
            ))
        ));
        let bound = bound_offer(&f, &current).await?;
        let tool = ToolRunId::from_bytes([248; 32]);
        f.http
            .store
            .admit_http_context_read(project, &f.phase, &bound, tool, at(now + 1)?)
            .await?;
        let reader = DeterministicAgentReadTools::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &WorkspaceAgentSourceReader,
        );
        let mut wrong_tool = reader
            .execute(
                project,
                current.run().snapshot_id(),
                ToolRunId::from_bytes([249; 32]),
                &bound.read_action(project, &current)?,
                AgentReadTimeout::DEFAULT,
                &ActiveControl,
            )
            .await?;
        assert!(matches!(
            bound.take_original(project, &current, tool, &mut wrong_tool),
            Err(BoundEditSourceFailure::InvalidReadResult)
        ));
        assert_eq!(
            f.http
                .store
                .interrupt_agent_tool_attempts(project, f.run.id(), at(now + 3)?)
                .await?,
            1
        );
        assert!(matches!(
            f.http
                .store
                .admit_http_context_read(
                    project,
                    &f.phase,
                    &bound,
                    ToolRunId::from_bytes([250; 32]),
                    at(now + 4)?
                )
                .await,
            Err(SourceReadFailure::Reservation(
                EditPhaseReservationFailure::AlreadyReserved
            ))
        ));
        assert_eq!(fixture_count(&f.http.fixture,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?,0);
        assert_eq!(
            fixture_count(&f.http.fixture, "SELECT count(*) FROM tool_run_attempts").await?,
            2
        );
        drop((reader, wrong_tool, wrong, bound, current));
        let f = edit::cold(f).await?;
        let current = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("cold interrupted publication missing")?;
        assert!(matches!(
            f.http
                .store
                .load_http_context_read_origin(
                    &f.http.fixture.project,
                    &f.http.goal,
                    &f.stored,
                    &f.run,
                    &current,
                    &f.http.model,
                    edit::after_phases(&f).await?
                )
                .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        assert_eq!(count(&f,"SELECT count(*) FROM source_read_acquisitions WHERE original_evidence_id IS NOT NULL").await?,0);
        assert_eq!(
            count(&f, "SELECT count(*) FROM http_context_intent_reservations").await?,
            0
        );
        assert_eq!(inventory(&f.http.root)?, f.http.before);
        drop(current);
        cleanup_open(f)
    })
}

// The22percent headroom remains charged; it does not enlarge generation permission.
#[test]
fn http_context_finish_checks_generation_ceiling_separately_from_reserved_headroom() -> TestResult {
    crate::run_native_libsql_test(async {
        for disposition in [
            EditPhaseObservationOutcome::AcceptedReply,
            EditPhaseObservationOutcome::InvalidStructure,
            EditPhaseObservationOutcome::Unknown,
        ] {
            for reported in [1024, 1025, 3605] {
                let c = acquire_context().await?;
                let project = &c.f.http.fixture.project;
                let before =
                    c.f.http
                        .store
                        .load_edit_phase_usage(project, c.f.run.id())
                        .await?;
                let request = c.phase.request_for(EditPhaseCallKind::Initial)?;
                let projection = encoder()?.project_wire_request(&request)?;
                let budget = PhaseWireBudget::fit(projection, request.profile())?;
                assert_eq!(request.profile().settings().output_limit().get(), 1024);
                assert_eq!(budget.output_tokens(), 3605);
                let wire = EditPhaseWireCost::new(
                    projection.digest(),
                    projection.request_bytes(),
                    budget.prompt_tokens(),
                    budget.output_tokens(),
                )?;
                let now = edit::after_phases(&c.f).await?;
                let permit =
                    c.f.http
                        .store
                        .reserve_http_context_intent(
                            project,
                            &c.phase,
                            EditPhaseCallKind::Initial,
                            wire,
                            now,
                        )
                        .await?;
                let reply = c
                    .phase
                    .decode_reply(c.phase.bind_reply(br#"{"intent":null}"#)?)?;
                let candidate = (disposition == EditPhaseObservationOutcome::AcceptedReply)
                    .then_some(reply.clone());
                let result =
                    c.f.http
                        .store
                        .finish_http_context_intent(
                            project,
                            &c.phase,
                            permit,
                            EditPhaseObservation::new(
                                disposition,
                                Some(64),
                                Some(reported),
                                at(now.unix_millis() + 1)?,
                            ),
                            candidate,
                        )
                        .await;
                if reported > 1024 && disposition != EditPhaseObservationOutcome::Unknown {
                    assert!(matches!(
                        result,
                        Err(EditPhaseReservationFailure::BudgetExhausted(
                            AgentBudgetDimension::OutputTokens
                        ))
                    ));
                } else if disposition == EditPhaseObservationOutcome::AcceptedReply {
                    assert_eq!(result?, Some(reply));
                } else {
                    assert_eq!(result?, None);
                }
                let usage =
                    c.f.http
                        .store
                        .load_edit_phase_usage(project, c.f.run.id())
                        .await?;
                assert_eq!(usage.calls, before.calls + 1);
                assert_eq!(usage.output_tokens, before.output_tokens + 3605);
                assert_eq!(usage.unobserved_calls, 0);
                let unknown =
                    reported > 1024 || disposition == EditPhaseObservationOutcome::Unknown;
                assert_eq!(usage.unknown_calls, u32::from(unknown));
                let loaded =
                    c.f.http
                        .store
                        .load_http_context_intent_for_application(
                            project,
                            &c.phase,
                            edit::after_phases(&c.f).await?,
                        )
                        .await;
                if unknown {
                    assert!(matches!(
                        loaded,
                        Err(EditPhaseReservationFailure::UncertainProviderCall)
                    ));
                    assert_eq!(count(&c.f,"SELECT count(*) FROM http_context_intent_reservations WHERE outcome='unknown' AND reply IS NULL AND reply_digest IS NULL").await?,1);
                    let repair_request =
                        c.phase.request_for(EditPhaseCallKind::StructuralRepair)?;
                    let repair_projection = encoder()?.project_wire_request(&repair_request)?;
                    let repair_budget =
                        PhaseWireBudget::fit(repair_projection, repair_request.profile())?;
                    let repair_wire = EditPhaseWireCost::new(
                        repair_projection.digest(),
                        repair_projection.request_bytes(),
                        repair_budget.prompt_tokens(),
                        repair_budget.output_tokens(),
                    )?;
                    assert!(matches!(
                        c.f.http
                            .store
                            .reserve_http_context_intent(
                                project,
                                &c.phase,
                                EditPhaseCallKind::StructuralRepair,
                                repair_wire,
                                edit::after_phases(&c.f).await?,
                            )
                            .await,
                        Err(EditPhaseReservationFailure::UncertainProviderCall)
                    ));
                } else if disposition == EditPhaseObservationOutcome::AcceptedReply {
                    assert!(loaded?.is_some());
                } else {
                    assert!(loaded?.is_none());
                }
                assert_metadata_only(&c).await?;
                cleanup(c)?;
            }
        }
        Ok(())
    })
}

#[test]
fn http_context_cold_lineage_rejects_generation_overage_in_accepted_and_prior_repair_rows()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (null, repair, selected) in [
            (false, false, "initial"),
            (true, false, "initial"),
            (false, true, "structural_repair"),
            (true, true, "structural_repair"),
            (false, true, "initial"),
        ] {
            for reported in [1025, 3605] {
                let c = acquire_context().await?;
                let provider = controlled_provider(&c, null, repair)?;
                if repair {
                    assert!(matches!(
                        execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
                        Err(HttpContextIntentExecutionFailure::InvalidReply)
                    ));
                }
                let report = execute_context(
                    &c,
                    &provider,
                    if repair {
                        EditPhaseCallKind::StructuralRepair
                    } else {
                        EditPhaseCallKind::Initial
                    },
                )
                .await?;
                assert_eq!(report.budget.output_tokens(), 3605);
                assert_eq!(
                    c.phase.request().profile().settings().output_limit().get(),
                    1024
                );
                drop(provider);
                let db = c.f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                // Fresh disposable test DB only: model trial artifacts are never faulted.
                conn.execute("DROP TRIGGER http_context_intent_update_guard", ())
                    .await?;
                conn.execute("UPDATE http_context_intent_reservations SET reported_output=1024 WHERE call_kind=?1",
                    libsql::params![selected]).await?;
                let boundary =
                    c.f.http
                        .store
                        .load_http_context_intent_for_application(
                            &c.f.http.fixture.project,
                            &c.phase,
                            edit::after_phases(&c.f).await?,
                        )
                        .await?
                        .ok_or("exact1024 boundary was rejected")?;
                assert_settlement(&c, &boundary, &report.reply, report.projection.digest())?;
                conn.execute("UPDATE http_context_intent_reservations SET reported_output=?1 WHERE call_kind=?2",
                    libsql::params![reported,selected]).await?;
                assert!(matches!(
                    c.f.http
                        .store
                        .load_http_context_intent_for_application(
                            &c.f.http.fixture.project,
                            &c.phase,
                            edit::after_phases(&c.f).await?,
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                drop((boundary, conn, db));
                let c = cold_context(c).await?;
                assert!(matches!(
                    c.f.http
                        .store
                        .load_http_context_intent_for_application(
                            &c.f.http.fixture.project,
                            &c.phase,
                            edit::after_phases(&c.f).await?,
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                // The reserved headroom and durable charge were never shrunk or refunded.
                let usage =
                    c.f.http
                        .store
                        .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                        .await?;
                assert_eq!(usage.calls, if repair { 3 } else { 2 });
                assert_eq!(usage.unknown_calls, 0); // Faulted accepted labels do not become authority.
                assert_metadata_only(&c).await?;
                cleanup(c)?;
            }
        }
        Ok(())
    })
}

#[test]
fn http_context_executor_generation_overage_stays_unknown_and_cold_nonexecuting() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let project = &c.f.http.fixture.project;
        let before =
            c.f.http
                .store
                .load_edit_phase_usage(project, c.f.run.id())
                .await?;
        let mutations = count(&c.f, "SELECT count(*) FROM mutation_attempts").await?;
        let request = c.phase.request_for(EditPhaseCallKind::Initial)?;
        let projection = encoder()?.project_wire_request(&request)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())?;
        assert_eq!(
            (projection.context_tokens(), projection.output_tokens()),
            (16384, 1024)
        );
        assert_eq!(budget.output_tokens(), 3605);
        let mut provider = controlled_provider(&c, false, false)?;
        provider.reported_output_tokens = 1025;
        assert!(provider.reported_output_tokens > u64::from(projection.output_tokens()));
        assert!(provider.reported_output_tokens <= u64::from(budget.output_tokens()));
        // Valid JSON and Stop cannot make contradictory provider usage acceptable.
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
            Err(HttpContextIntentExecutionFailure::Usage)
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let usage =
            c.f.http
                .store
                .load_edit_phase_usage(project, c.f.run.id())
                .await?;
        assert_eq!(usage.calls, before.calls + 1);
        assert_eq!(usage.repairs, before.repairs);
        assert_eq!(
            usage.prompt_tokens,
            before.prompt_tokens + u64::from(budget.prompt_tokens())
        );
        assert_eq!(usage.output_tokens, before.output_tokens + 3605);
        assert_eq!((usage.unobserved_calls, usage.unknown_calls), (0, 1));
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            1
        );
        assert_eq!(count(&c.f,"SELECT count(*) FROM http_context_intent_reservations WHERE outcome='unknown' AND reported_output=1025 AND output_reserve=3605 AND reply IS NULL AND reply_digest IS NULL").await?,1);
        assert_eq!(count(&c.f,"SELECT count(*) FROM http_context_intent_reservations WHERE outcome='accepted_reply'").await?,0);
        // No successful report or authenticated Settlement exists from which delivery can be credited.
        assert!(matches!(
            c.f.http
                .store
                .load_http_context_intent_for_application(
                    project,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                )
                .await,
            Err(EditPhaseReservationFailure::UncertainProviderCall)
        ));
        assert!(matches!(
            execute_context(&c, &provider, EditPhaseCallKind::StructuralRepair).await,
            Err(HttpContextIntentExecutionFailure::Reservation(
                EditPhaseReservationFailure::UncertainProviderCall
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            1
        );
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
        assert_metadata_only(&c).await?;
        drop(provider);
        let ContextFixture {
            f,
            phase,
            origin,
            primary,
            context,
        } = c;
        drop((origin, primary, context));
        let f = edit::reopen(f).await?;
        let project = &f.http.fixture.project;
        // Unknown blocks reconstruction of a fresh origin too; never fabricate a replacement phase.
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("cold publication missing")?;
        assert!(matches!(
            f.http
                .store
                .load_http_context_read_origin(
                    project,
                    &f.http.goal,
                    &f.stored,
                    &f.run,
                    &current,
                    &f.http.model,
                    edit::after_phases(&f).await?,
                )
                .await,
            Err(EditPhaseReservationFailure::UncertainProviderCall)
        ));
        assert!(matches!(
            f.http
                .store
                .load_http_context_intent_for_application(
                    project,
                    &phase,
                    edit::after_phases(&f).await?,
                )
                .await,
            Err(EditPhaseReservationFailure::UncertainProviderCall)
        ));
        let cold_usage = f
            .http
            .store
            .load_edit_phase_usage(project, f.run.id())
            .await?;
        assert_eq!(cold_usage.calls, usage.calls);
        assert_eq!(cold_usage.repairs, usage.repairs);
        assert_eq!(cold_usage.prompt_tokens, usage.prompt_tokens);
        assert_eq!(cold_usage.output_tokens, usage.output_tokens);
        assert_eq!(
            (cold_usage.unobserved_calls, cold_usage.unknown_calls),
            (0, 1)
        );
        assert_eq!(
            f.http.store.load_agent_run(project, f.run.id()).await?,
            Some(f.run.clone())
        );
        assert_eq!(f.run.state(), AgentControllerState::Replan);
        assert_eq!(inventory(&f.http.root)?, f.http.before);
        assert_eq!(
            count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
            1
        );
        assert_eq!(
            count(&f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
        assert_eq!(count(&f,"SELECT count(*) FROM http_context_intent_reservations WHERE outcome='accepted_reply'").await?,0);
        drop((phase, current));
        cleanup_open(f)
    })
}

async fn accepted_parent_with_actual_projected_wire(
    kind: EditPhaseCallKind,
) -> TestResult<CurrentPlanFixture> {
    let f = full_fixture(
        ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::SixteenK)?,
        true,
    )
    .await?;
    let proposal = f.phase.decode_proposal(&reply(&f.phase, "S1")?)?;
    let base = f.run.updated_at().unix_millis() + 1;
    let kinds = [
        Some(EditPhaseCallKind::Initial),
        (kind == EditPhaseCallKind::StructuralRepair)
            .then_some(EditPhaseCallKind::StructuralRepair),
    ];
    for (index, call) in kinds.into_iter().flatten().enumerate() {
        let request = f.phase.request_for(call)?;
        let projection = encoder()?.project_wire_request(&request)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())?;
        // Actual encoder costs BEFORE reservation; never manufacture SQL output reserves.
        assert_eq!(
            (
                projection.context_tokens(),
                projection.output_tokens(),
                budget.output_tokens()
            ),
            (16384, 1024, 3605)
        );
        let wire = EditPhaseWireCost::new(
            projection.digest(),
            projection.request_bytes(),
            budget.prompt_tokens(),
            budget.output_tokens(),
        )?;
        let reserved_at = at(base + u64::try_from(index)? * 2)?;
        let permit = f
            .http
            .store
            .reserve_http_correction_plan(
                &f.http.fixture.project,
                &f.phase,
                call,
                wire,
                reserved_at,
            )
            .await?;
        let invalid =
            call == EditPhaseCallKind::Initial && kind == EditPhaseCallKind::StructuralRepair;
        f.http
            .store
            .finish_http_correction_plan(
                &f.http.fixture.project,
                &f.phase,
                permit,
                EditPhaseObservation::new(
                    if invalid {
                        EditPhaseObservationOutcome::InvalidStructure
                    } else {
                        EditPhaseObservationOutcome::AcceptedReply
                    },
                    Some(64),
                    Some(16),
                    at(reserved_at.unix_millis() + 1)?,
                ),
                (!invalid).then_some(proposal.clone()),
            )
            .await?;
    }
    Ok(f)
}

#[test]
fn http_context_parent_generation_limit_checks_selected_and_prior_history_warm_and_cold()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (kind, selected) in [
            (EditPhaseCallKind::Initial, "initial"),
            (EditPhaseCallKind::StructuralRepair, "structural_repair"),
            (EditPhaseCallKind::StructuralRepair, "initial"),
        ] {
            for reported in [1025, 3605] {
                let f = accepted_parent_with_actual_projected_wire(kind).await?;
                let project = &f.http.fixture.project;
                let current = f
                    .http
                    .store
                    .latest_published_index(project, &ActiveControl)
                    .await?
                    .ok_or("actual parent publication missing")?;
                let bound = bound_offer(&f, &current).await?;
                let db = f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                // Fresh disposable DB only. Original actual wire and reserves are never changed.
                conn.execute("DROP TRIGGER http_correction_plan_update_guard", ())
                    .await?;
                assert_eq!(fixture_count(&f.http.fixture,
                    "SELECT count(*) FROM http_correction_plan_reservations WHERE output_reserve=3605").await?,
                    if kind == EditPhaseCallKind::Initial { 1 } else { 2 });
                conn.execute("UPDATE http_correction_plan_reservations SET reported_output=?1 WHERE call_kind=?2",
                    libsql::params![reported, selected]).await?;
                assert!(matches!(
                    f.http
                        .store
                        .admit_http_context_read(
                            project,
                            &f.phase,
                            &bound,
                            ToolRunId::from_bytes([247; 32]),
                            at(f.run.updated_at().unix_millis() + 20)?
                        )
                        .await,
                    Err(SourceReadFailure::Reservation(
                        EditPhaseReservationFailure::InvalidStoredData
                    ))
                ));
                assert_eq!(
                    fixture_count(
                        &f.http.fixture,
                        "SELECT count(*) FROM source_read_acquisitions"
                    )
                    .await?,
                    0
                );
                // Exact1024 boundary must pass genuine admission/read and cold-origin reconstruction.
                conn.execute("UPDATE http_correction_plan_reservations SET reported_output=1024 WHERE call_kind=?1",
                    libsql::params![selected]).await?;
                drop((conn, db, bound, current));
                let c = acquire_context_from(f).await?;
                assert_eq!(
                    c.origin.read_event.sequence().get(),
                    c.origin.parent_anchor.sequence.get() + 1
                );
                let db = c.f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                conn.execute("UPDATE http_correction_plan_reservations SET reported_output=?1 WHERE call_kind=?2",
                    libsql::params![reported, selected]).await?;
                drop((conn, db));
                let project = &c.f.http.fixture.project;
                let current =
                    c.f.http
                        .store
                        .latest_published_index(project, &ActiveControl)
                        .await?
                        .ok_or("actual context publication missing")?;
                assert!(matches!(
                    c.f.http
                        .store
                        .load_http_context_read_origin(
                            project,
                            &c.f.http.goal,
                            &c.f.stored,
                            &c.f.run,
                            &current,
                            &c.f.http.model,
                            edit::after_phases(&c.f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                let provider = controlled_provider(&c, false, false)?;
                assert!(matches!(
                    execute_context(&c, &provider, EditPhaseCallKind::Initial).await,
                    Err(HttpContextIntentExecutionFailure::Reservation(
                        EditPhaseReservationFailure::InvalidStoredData
                    ))
                ));
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                assert!(matches!(
                    c.f.http
                        .store
                        .load_http_context_intent_for_application(
                            project,
                            &c.phase,
                            edit::after_phases(&c.f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                assert_eq!(
                    count(
                        &c.f,
                        "SELECT count(*) FROM http_context_intent_reservations"
                    )
                    .await?,
                    0
                );
                assert_metadata_only(&c).await?;
                drop((provider, current));
                let ContextFixture {
                    f,
                    phase,
                    origin,
                    primary,
                    context,
                } = c;
                drop((origin, primary, context));
                let f = edit::reopen(f).await?;
                let project = &f.http.fixture.project;
                let current = f
                    .http
                    .store
                    .latest_published_index(project, &ActiveControl)
                    .await?
                    .ok_or("cold actual parent publication missing")?;
                assert!(matches!(
                    f.http
                        .store
                        .load_http_context_read_origin(
                            project,
                            &f.http.goal,
                            &f.stored,
                            &f.run,
                            &current,
                            &f.http.model,
                            edit::after_phases(&f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                assert!(matches!(
                    f.http
                        .store
                        .load_http_context_intent_for_application(
                            project,
                            &phase,
                            edit::after_phases(&f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                assert_eq!(
                    count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
                    1
                );
                assert_eq!(
                    count(&f, "SELECT count(*) FROM http_context_intent_reservations").await?,
                    0
                );
                assert_eq!(
                    f.http.store.load_agent_run(project, f.run.id()).await?,
                    Some(f.run.clone())
                );
                assert_eq!(inventory(&f.http.root)?, f.http.before);
                drop((phase, current));
                cleanup_open(f)?;
            }
        }
        Ok(())
    })
}

#[test]
fn http_context_read_origin_requires_exact_immutable_profile_warm_and_cold() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = acquire_context_from(
            accepted_parent_with_actual_projected_wire(EditPhaseCallKind::Initial).await?,
        )
        .await?;
        let original = &c.f.http.model;
        let settings = original.settings();
        let same_model_higher_generation = ModelProfile::from_probe(
            original.provider_id().clone(),
            original.model_id().clone(),
            ModelProfileSettings::new(
                settings.context_limit(),
                ModelOutputLimit::new(2048)?,
                settings.token_counting(),
                settings.parallelism_limit(),
                settings.sampling(),
                settings.stop_sequences().clone(),
                settings.schema_grounding(),
            )?,
            original.capabilities(),
        );
        let wrong_profiles = [
            ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::FourK)?,
            same_model_higher_generation,
        ];
        for wrong in &wrong_profiles {
            assert_eq!(wrong.model_id(), original.model_id());
            assert_ne!(wrong.reference(), original.reference());
        }
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("profile test publication missing")?;
        for wrong in &wrong_profiles {
            assert!(matches!(
                c.f.http
                    .store
                    .load_http_context_read_origin(
                        &c.f.http.fixture.project,
                        &c.f.http.goal,
                        &c.f.stored,
                        &c.f.run,
                        &current,
                        wrong,
                        edit::after_phases(&c.f).await?
                    )
                    .await,
                Err(EditPhaseReservationFailure::StaleAnchor)
            ));
        }
        assert_metadata_only(&c).await?;
        drop(current);
        let c = cold_context(c).await?;
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold profile test publication missing")?;
        for wrong in &wrong_profiles {
            assert!(matches!(
                c.f.http
                    .store
                    .load_http_context_read_origin(
                        &c.f.http.fixture.project,
                        &c.f.http.goal,
                        &c.f.stored,
                        &c.f.run,
                        &current,
                        wrong,
                        edit::after_phases(&c.f).await?
                    )
                    .await,
                Err(EditPhaseReservationFailure::StaleAnchor)
            ));
        }
        let correct =
            c.f.http
                .store
                .load_http_context_read_origin(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &c.f.stored,
                    &c.f.run,
                    &current,
                    &c.f.http.model,
                    edit::after_phases(&c.f).await?,
                )
                .await?;
        assert_eq!(correct.parent_job, c.origin.parent_job);
        assert_eq!(correct.read_admission.key(), c.origin.read_admission.key());
        assert_eq!(correct.read_event, c.origin.read_event);
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM source_read_acquisitions").await?,
            1
        );
        assert_eq!(
            count(
                &c.f,
                "SELECT count(*) FROM http_context_intent_reservations"
            )
            .await?,
            0
        );
        assert_metadata_only(&c).await?;
        drop((correct, current));
        cleanup(c)
    })
}
