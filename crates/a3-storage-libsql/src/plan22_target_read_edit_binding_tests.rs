//! Real target acquisition and native Edit/Patch boundaries; controlled replies are no model quality result.
use super::*;
#[path = "plan22_target_read_edit_live.rs"]
mod live;
#[path = "plan22_edit_scope_bom_tests.rs"]
mod scope_bom;
use a3_application::{
    AcquiredSourceRead, AgentReadTimeout, AgentReadTools, AppendAgentRead, BoundEditSource,
    CompileTaskLens, CompiledEditPhaseRequest, EditEvidenceNeed, EditPhaseExecutionFailure,
    EditPhasePrerequisites, EditPhaseRequestFailure, EditPhaseReservationFailure,
    EditPhaseReservationStore, ExecuteEditPhase, ModelFinishReason, ModelMessage,
    ModelOperationControl, ModelOutputChunk, ModelProvider, ModelProviderCompletion,
    ModelProviderFailure, ModelProviderFuture, ModelProviderRequest, ModelProviderUsage,
    ModelRequestTimeout, ModelWireProjection, PhaseWireBudget, ProviderEvent, ProviderEventStream,
    SourceDeliveryOutcome, SourceReadStore, build_edit_phase_request,
    http_correction_definition_page_matches,
};
use a3_context::DeterministicAgentReadTools;
use a3_domain::{
    EvidenceRef, SourcePosition, SourceRange, TaskLensSeed, TaskLensSeedSet, TaskLensSeedText,
    TaskLensTokenBudget,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use std::sync::atomic::AtomicUsize;

const BEFORE: &str = "# untouched α\r\ndef target(value):\r\n    return value + 1\r\n\r\ndef other(value):\r\n    return value + 1\r\n";
const SINGLE: &str = "def target(value):\n    return value + 1\n";
const READ_TOOL: ToolRunId = ToolRunId::from_bytes([190; 32]);

fn target_profile() -> TestResult<ModelProfile> {
    // Controlled transport profile only. This test does not probe or invoke an Ollama model.
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("qwen38-4k:latest".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4096)?,
            ModelOutputLimit::new(1024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

struct TargetFixture {
    fixture: Fixture,
    store: Arc<LibsqlKnowledgeStore>,
    refresh: RefreshRepositoryIndex,
    compiler: BuiltinIncrementalIndexCompiler,
    published: PublishedIndex,
    goal: GoalContract,
    stored: StoredTaskLedger,
    run: AgentRun,
    profile: ModelProfile,
    bound: BoundEditSource,
    page: AgentSourcePage,
    acquired: AcquiredSourceRead,
}

impl TargetFixture {
    async fn start(contents: &str) -> TestResult<Self> {
        let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
        let (goal, step) = path_edit_goal(
            &path,
            b"Apply the bounded change to subject.py; keep every other file byte-identical.\n",
        )?;
        Self::start_using(contents, target_profile()?, goal, step, Fixture::new()?).await
    }

    async fn start_using(
        contents: &str,
        profile: ModelProfile,
        goal: GoalContract,
        step: TaskStepDefinition,
        fixture: Fixture,
    ) -> TestResult<Self> {
        Self::start_using_span(contents, profile, goal, step, fixture, None).await
    }

    async fn start_using_span(
        contents: &str,
        profile: ModelProfile,
        goal: GoalContract,
        step: TaskStepDefinition,
        mut fixture: Fixture,
        supplied_span: Option<SourceRange>,
    ) -> TestResult<Self> {
        let root = fixture.root.join("worktree");
        if !root.join(".git").try_exists()? {
            initialize_git(&root)?;
        }
        fs::write(root.join("subject.py"), contents)?;
        fs::write(root.join("notes.txt"), OTHER_FILE)?;
        fixture.project = RepositoryInspector::new().inspect(&root)?;
        let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
        let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
        let snapshot = published.run().snapshot_id();
        let (mut run, stored, _) = start_fixture_step(
            &fixture,
            store.as_ref(),
            &goal,
            &profile,
            snapshot,
            vec![step],
        )
        .await?;
        let symbol = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| {
                symbol.revision().path() == &path && symbol.parsed().name().as_str() == "target"
            })
            .ok_or("target symbol missing")?;
        let required = EvidenceRef::new(
            symbol.revision().clone(),
            supplied_span.unwrap_or_else(|| symbol.parsed().declaration_range()),
        );
        let seed = TaskLensSeedText::try_from_string(
            "Read the current selected target implementation".to_owned(),
        )?;
        let lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref())
            .execute(
                &fixture.project,
                TaskLensSeedSet::new(seed.clone(), seed, vec![TaskLensSeed::ExplicitPath(path)])?,
                TaskLensTokenBudget::new(4096)?,
                &ActiveControl,
            )
            .await?;
        let bound = BoundEditSource::for_file_span(&fixture.project, &published, &lens, &required)?;
        let anchor =
            EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published)?.anchor();
        store
            .begin_source_read(&fixture.project, anchor, &bound, READ_TOOL, at(20)?)
            .await?;
        let reader = DeterministicAgentReadTools::new(
            store.as_ref(),
            store.as_ref(),
            store.as_ref(),
            &WorkspaceAgentSourceReader,
        );
        let mut read = reader
            .execute(
                &fixture.project,
                snapshot,
                READ_TOOL,
                &bound.read_action(&fixture.project, &published)?,
                AgentReadTimeout::DEFAULT,
                &ActiveControl,
            )
            .await?;
        let page = bound.take_original(&fixture.project, &published, READ_TOOL, &mut read)?;
        let sequence = run.last_event_sequence();
        let recorded = read.record(&mut run, RunEventId::from_bytes([190; 32]), at(21)?)?;
        AppendAgentRead::new(store.as_ref())
            .execute(&fixture.project, sequence, &run, &recorded)
            .await?;
        let anchor =
            EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published)?.anchor();
        let history = store
            .load_source_read_state(&fixture.project, anchor)
            .await?;
        assert!(history.progress().receipts().is_empty() && history.progress().pending().is_some());
        let acquired = history
            .acquired()
            .ok_or("real target read stamp missing")?
            .clone();
        assert_eq!(acquired.original().range(), page.range());
        Ok(Self {
            fixture,
            store,
            refresh,
            compiler,
            published,
            goal,
            stored,
            run,
            profile,
            bound,
            page,
            acquired,
        })
    }

    fn state(&self) -> TestResult<EditPhaseState<'_>> {
        Ok(EditPhaseState::new(
            &self.fixture.project,
            &self.goal,
            &self.stored,
            &self.run,
            &self.published,
        )?)
    }

    fn compile(
        &self,
        phase: &PreparedEditPhase,
        acquired: &AcquiredSourceRead,
    ) -> TestResult<CompiledEditPhaseRequest> {
        Ok(build_edit_phase_request(
            &self.goal,
            &self.stored,
            phase,
            self.state()?,
            self.profile.clone(),
            None,
            EditPhasePrerequisites::none().with_required_source(
                EditEvidenceNeed::Definition,
                &self.bound,
                Some((&self.page, acquired)),
            ),
        )?)
    }

    async fn reopen(self) -> TestResult<Self> {
        let Self {
            fixture,
            store,
            refresh,
            compiler,
            published: _,
            goal,
            stored: _,
            run,
            profile,
            bound,
            page,
            acquired,
        } = self;
        let run_id = run.id();
        drop((store, refresh, compiler));
        let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
        let loaded_goal = store
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("cold goal missing")?;
        assert_eq!(loaded_goal, goal);
        let run = store
            .load_agent_run(&fixture.project, run_id)
            .await?
            .ok_or("cold run missing")?;
        let stored = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("cold ledger missing")?;
        let published = store
            .latest_published_index(&fixture.project, &ActiveControl)
            .await?
            .ok_or("cold publication missing")?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        Ok(Self {
            fixture,
            store,
            refresh,
            compiler,
            published,
            goal: loaded_goal,
            stored,
            run,
            profile,
            bound,
            page,
            acquired,
        })
    }

    fn cleanup(self) -> TestResult {
        let Self {
            fixture,
            store,
            refresh,
            compiler,
            ..
        } = self;
        drop((store, refresh, compiler));
        fixture.cleanup()
    }
}

#[derive(Clone, Copy)]
enum ControlledReply {
    Proposal,
    InvalidThenProposal,
    Unknown,
    StaleNeed,
    StaleTooLarge,
}

struct TargetProvider<'a> {
    inner: OllamaModelProvider,
    fixture: &'a TargetFixture,
    mode: ControlledReply,
    calls: AtomicUsize,
}
impl TargetProvider<'_> {
    fn new(fixture: &TargetFixture, mode: ControlledReply) -> TestResult<TargetProvider<'_>> {
        Ok(TargetProvider {
            inner: OllamaModelProvider::for_projected_wire(
                OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                Arc::new(LocalOnlyOllamaEndpointPolicy),
            )?,
            fixture,
            mode,
            calls: AtomicUsize::new(0),
        })
    }
}
impl std::fmt::Debug for TargetProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TargetProvider")
            .finish_non_exhaustive()
    }
}

impl ModelProvider for TargetProvider<'_> {
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
            let f = self.fixture;
            let usage = f
                .store
                .load_edit_phase_usage(&f.fixture.project, f.run.id())
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1 || self.project_wire_request(request)? != projection {
                return Err(ModelProviderFailure::Rejected);
            }
            let state = f.state().map_err(|_| ModelProviderFailure::Rejected)?;
            let read = f
                .store
                .load_source_read_state(&f.fixture.project, state.anchor())
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if read.progress().pending().is_none()
                || read.acquired().is_some()
                || !read.progress().receipts().is_empty()
            {
                return Err(ModelProviderFailure::Rejected);
            }
            // The selected target appears exactly once, in the existing final Edit JSON message.
            if request.messages().len() != 3 {
                return Err(ModelProviderFailure::Rejected);
            }
            let input: Value = serde_json::from_str(
                request
                    .messages()
                    .last()
                    .ok_or(ModelProviderFailure::Rejected)?
                    .content(),
            )
            .map_err(|_| ModelProviderFailure::Rejected)?;
            if input["current"].as_str() != Some(f.page.text()) {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if matches!(self.mode, ControlledReply::Unknown) {
                return Err(ModelProviderFailure::Unavailable);
            }
            let text = if matches!(self.mode, ControlledReply::InvalidThenProposal) && call == 0 {
                "{}".to_owned()
            } else if matches!(
                self.mode,
                ControlledReply::StaleNeed | ControlledReply::StaleTooLarge
            ) {
                let path = f.fixture.root.join("worktree/subject.py");
                let before =
                    fs::read_to_string(&path).map_err(|_| ModelProviderFailure::Unavailable)?;
                fs::write(path, before.replace("untouched", "user_edit"))
                    .map_err(|_| ModelProviderFailure::Unavailable)?;
                if matches!(self.mode, ControlledReply::StaleNeed) {
                    json!({"need":"test"}).to_string()
                } else {
                    json!({"tooLarge":true}).to_string()
                }
            } else {
                json!({"code":f.page.text().replace(" + 1", " + 2").replace("\r\n", "\n")})
                    .to_string()
            };
            Ok(Box::pin(futures::stream::iter([
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(text)
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(64), Some(16)),
                ))),
            ])) as ProviderEventStream<'a>)
        })
    }
}

#[test]
fn target_read_same_page_wire_cold_delivery_and_actual_approved_patch() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = TargetFixture::start(BEFORE).await?;
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let plain = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none(),
            )?;
            let mut compiled = f.compile(&phase, &f.acquired)?;
            let provider = TargetProvider::new(&f, ControlledReply::Proposal)?;
            // v3 truthfully narrows the grammar only when this job has a bound role.
            // Attaching metadata never duplicates or expands the actual target Original.
            assert_eq!(
                plain.request().messages().last(),
                compiled.request().messages().last()
            );
            assert_eq!(
                plain.request().messages().len(),
                compiled.request().messages().len()
            );
            assert!(
                !compiled
                    .request()
                    .structured_output()
                    .ok_or("schema")?
                    .value()
                    .to_string()
                    .contains("test")
            );
            for request in [
                &compiled,
                &compiled.for_structural_repair()?,
                &compiled.for_truncated_output_repair()?,
            ] {
                PhaseWireBudget::fit(
                    provider.project_wire_request(request.request())?,
                    request.request().profile(),
                )?;
            }
            let patch = WorkspacePatchAdapter::with_change_window_preview();
            let report = ExecuteEditPhase::new(&provider, f.store.as_ref(), &patch)
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
                    ModelRequestTimeout::DEFAULT,
                    &ActiveControl,
                )
                .await?;
            assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
            let delivery = report
                .source_delivery
                .as_ref()
                .ok_or("target actual wire missing")?;
            assert_eq!(delivery.source().revision(), f.page.revision());
            assert_eq!(delivery.source().range(), f.page.range());
            assert_eq!(delivery.wire_digest(), report.projection.digest());
            let EditPhaseOutcome::Proposal(action) = report.outcome else {
                return Err("controlled Edit did not produce exact patch".into());
            };
            drop(provider);
            let f = f.reopen().await?;
            let reads = f
                .store
                .load_source_read_state(&f.fixture.project, f.state()?.anchor())
                .await?;
            let [receipt] = reads.progress().receipts() else {
                return Err("cold target delivery missing".into());
            };
            assert_eq!(
                receipt.outcome(),
                SourceDeliveryOutcome::NewBytes(u32::try_from(f.page.text().len())?)
            );
            assert_eq!(receipt.delivery(), Some(delivery));
            assert!(reads.progress().pending().is_none() && reads.acquired().is_none());
            let proof = apply_target_patch(f, action).await?;
            assert_eq!(
                fs::read(proof.fixture.root.join("worktree/subject.py"))?,
                BEFORE
                    .replacen("return value + 1", "return value + 2", 1)
                    .as_bytes()
            );
            assert_eq!(
                fs::read(proof.fixture.root.join("worktree/notes.txt"))?,
                OTHER_FILE
            );
            proof.fixture.cleanup()
        })
    })
}

#[test]
fn target_read_repair_retains_one_stamp_and_unknown_or_stale_halts_never_credit() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for mode in [
                ControlledReply::InvalidThenProposal,
                ControlledReply::Unknown,
                ControlledReply::StaleNeed,
                ControlledReply::StaleTooLarge,
            ] {
                let f = TargetFixture::start(BEFORE).await?;
                let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
                let mut compiled = f.compile(&phase, &f.acquired)?;
                let provider = TargetProvider::new(&f, mode)?;
                let patch = WorkspacePatchAdapter::with_change_window_preview();
                let first = ExecuteEditPhase::new(&provider, f.store.as_ref(), &patch)
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
                        ModelRequestTimeout::DEFAULT,
                        &ActiveControl,
                    )
                    .await;
                assert!(first.is_err());
                if matches!(mode, ControlledReply::InvalidThenProposal) {
                    let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
                    let mut repair = compiled.for_structural_repair()?;
                    let db = f.fixture.candidate_database().await?;
                    let connection = db.connection_for_operation().await?;
                    let tx = connection
                        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
                        .await?;
                    let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
                        &tx,
                        f.fixture.project.worktree().id(),
                        &f.run,
                    )
                    .await?;
                    let now = latest.ok_or("first settlement missing")?.unix_millis() + 1;
                    tx.commit().await?;
                    drop((connection, db));
                    let report = ExecuteEditPhase::new(&provider, f.store.as_ref(), &patch)
                        .execute(
                            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                                &f.compiler,
                                phase,
                                &mut repair,
                                f.state()?,
                                at(now)?,
                                &patch,
                                &ActiveControl,
                            ))
                            .await?,
                            f.state()?,
                            &repair,
                            at(now)?,
                            ModelRequestTimeout::DEFAULT,
                            &ActiveControl,
                        )
                        .await?;
                    assert!(report.source_delivery.is_some());
                    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
                } else {
                    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                }
                drop(provider);
                let f = f.reopen().await?;
                let reads = f
                    .store
                    .load_source_read_state(&f.fixture.project, f.state()?.anchor())
                    .await?;
                let usage = f
                    .store
                    .load_edit_phase_usage(&f.fixture.project, f.run.id())
                    .await?;
                if matches!(mode, ControlledReply::InvalidThenProposal) {
                    assert_eq!((usage.calls, usage.repairs, usage.unknown_calls), (2, 1, 0));
                    assert_eq!(reads.progress().receipts().len(), 1);
                    assert!(reads.progress().pending().is_none());
                } else {
                    assert_eq!(usage.calls, 1);
                    assert_eq!(
                        usage.unknown_calls,
                        u32::from(matches!(mode, ControlledReply::Unknown))
                    );
                    assert!(
                        reads.progress().receipts().is_empty()
                            && reads.progress().pending().is_some()
                    );
                    assert!(reads.acquired().is_none());
                }
                f.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn target_read_rejects_foreign_stamp_window_bom_append_add_and_unsupported_roles() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = TargetFixture::start(BEFORE).await?;
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let foreign = TargetFixture::start(BEFORE).await?;
            assert_ne!(
                foreign.fixture.project.worktree().id(),
                f.fixture.project.worktree().id()
            );
            assert!(f.compile(&phase, &foreign.acquired).is_err());
            foreign.cleanup()?;
            let unsupported = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none().with_required_source(
                    EditEvidenceNeed::Test,
                    &f.bound,
                    Some((&f.page, &f.acquired)),
                ),
            );
            assert!(matches!(
                unsupported,
                Err(EditPhaseRequestFailure::UnsupportedSourceNeed(
                    EditEvidenceNeed::Test
                ))
            ));
            // A plain FileSpan is not a directed supplemental Caller; do not fake that role.
            let caller = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none().with_required_source(
                    EditEvidenceNeed::Caller,
                    &f.bound,
                    Some((&f.page, &f.acquired)),
                ),
            );
            assert!(matches!(
                caller,
                Err(EditPhaseRequestFailure::InvalidCallerTarget)
            ));
            let other = WorkspaceAgentSourceReader
                .read_page(
                    &f.fixture.project,
                    f.page.revision(),
                    &AgentFileInspection::new(
                        f.page.revision().path().clone(),
                        AgentFileStartLine::new(1)?,
                        AgentFileLineCount::new(1)?,
                    ),
                    &ActiveControl,
                )
                .await?;
            let other_phase = PreparedEditPhase::update(f.state()?, &other)?;
            assert!(matches!(
                build_edit_phase_request(
                    &f.goal,
                    &f.stored,
                    &other_phase,
                    f.state()?,
                    f.profile.clone(),
                    None,
                    EditPhasePrerequisites::none().with_required_source(
                        EditEvidenceNeed::Definition,
                        &f.bound,
                        Some((&f.page, &f.acquired))
                    )
                ),
                Err(EditPhaseRequestFailure::InvalidTargetSource)
            ));
            let add = PreparedEditPhase::add(
                f.state()?,
                RepositoryPath::try_from_bytes(b"new.py".to_vec())?,
            )?;
            assert!(f.compile(&add, &f.acquired).is_err());
            let forged = AcquiredSourceRead::from_recorded(
                ContentHash::from_bytes([191; 32]),
                f.acquired.scope(),
                f.acquired.required().clone(),
                f.acquired.original().clone(),
            )?;
            let mut compiled = f.compile(&phase, &forged)?;
            let provider = TargetProvider::new(&f, ControlledReply::Proposal)?;
            let patch = WorkspacePatchAdapter::with_change_window_preview();
            assert!(matches!(
                ExecuteEditPhase::new(&provider, f.store.as_ref(), &patch)
                    .execute(
                        Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                            &f.compiler,
                            phase,
                            &mut compiled,
                            f.state()?,
                            at(22)?,
                            &patch,
                            &ActiveControl
                        ))
                        .await?,
                        f.state()?,
                        &compiled,
                        at(22)?,
                        ModelRequestTimeout::DEFAULT,
                        &ActiveControl,
                    )
                    .await,
                Err(EditPhaseExecutionFailure::Reservation(
                    EditPhaseReservationFailure::StaleAnchor
                ))
            ));
            assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
            assert_eq!(
                f.store
                    .load_edit_phase_usage(&f.fixture.project, f.run.id())
                    .await?
                    .calls,
                0
            );
            drop(provider);
            f.cleanup()?;
            for content in [SINGLE.to_owned(), format!("\u{feff}{SINGLE}")] {
                let f = TargetFixture::start(&content).await?;
                let phase = if content.starts_with('\u{feff}') {
                    PreparedEditPhase::update(f.state()?, &f.page)?
                } else {
                    PreparedEditPhase::append(f.state()?, &f.page)?
                };
                assert!(f.compile(&phase, &f.acquired).is_err());
                assert_eq!(
                    f.store
                        .load_edit_phase_usage(&f.fixture.project, f.run.id())
                        .await?
                        .calls,
                    0
                );
                let read = f
                    .store
                    .load_source_read_state(&f.fixture.project, f.state()?.anchor())
                    .await?;
                assert!(read.progress().receipts().is_empty() && read.acquired().is_some());
                f.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn target_read_rejects_forged_page_or_live_stale_bytes_before_reservation() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for forged in [false, true] {
                let f = TargetFixture::start(BEFORE).await?;
                let page = if forged {
                    AgentSourcePage::new(
                        f.page.revision().clone(),
                        f.page.range(),
                        f.page.start_line(),
                        f.page.text().replace(" + 1", " - 1"),
                        f.page.next_start_line(),
                        f.page.truncated(),
                    )?
                } else {
                    f.page.clone()
                };
                let phase = PreparedEditPhase::update(f.state()?, &page)?;
                let mut compiled = build_edit_phase_request(
                    &f.goal,
                    &f.stored,
                    &phase,
                    f.state()?,
                    f.profile.clone(),
                    None,
                    EditPhasePrerequisites::none().with_required_source(
                        EditEvidenceNeed::Definition,
                        &f.bound,
                        Some((&page, &f.acquired)),
                    ),
                )?;
                if !forged {
                    fs::write(
                        f.fixture.root.join("worktree/subject.py"),
                        BEFORE.replace("untouched", "user_edit"),
                    )?;
                }
                let before = inventory(&f.fixture.root.join("worktree"))?;
                let patch = WorkspacePatchAdapter::with_change_window_preview();
                let adapter = f
                    .compiler
                    .shared_language_adapter_for(page.revision().path())?;
                let result = Box::pin(phase.prepare_scope_frame(
                    &mut compiled,
                    f.state()?,
                    adapter,
                    at(22)?,
                    &patch,
                    &ActiveControl,
                ))
                .await;
                let expected = if forged {
                    a3_application::EditPhaseFailure::InvalidWindow
                } else {
                    a3_application::EditPhaseFailure::ReadBase(
                        a3_application::PatchPreviewFailure::Conflict(
                            a3_application::PatchConflictKind::SourceChangedOnDisk,
                        ),
                    )
                };
                assert_eq!(result.err(), Some(expected));
                assert_eq!(
                    f.store
                        .load_edit_phase_usage(&f.fixture.project, f.run.id())
                        .await?
                        .calls,
                    0
                );
                assert_eq!(inventory(&f.fixture.root.join("worktree"))?, before);
                let reads = f
                    .store
                    .load_source_read_state(&f.fixture.project, f.state()?.anchor())
                    .await?;
                assert!(reads.progress().receipts().is_empty() && reads.acquired().is_some());
                f.cleanup()?;
            }
            Ok(())
        })
    })
}

struct TargetPatchProof {
    fixture: Fixture,
    goal: GoalContract,
    run_id: AgentRunId,
    evidence_id: TaskEvidenceId,
    snapshot_id: SnapshotId,
}

async fn apply_target_patch(f: TargetFixture, action: AgentAction) -> TestResult<TargetPatchProof> {
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let db = f.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let tx = connection
        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
        .await?;
    let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
        &tx,
        f.fixture.project.worktree().id(),
        &f.run,
    )
    .await?;
    let now = latest.ok_or("settled edit time missing")?.unix_millis() + 1;
    tx.commit().await?;
    drop((connection, db));
    let TargetFixture {
        fixture,
        store,
        refresh,
        mut compiler,
        published,
        goal,
        stored,
        mut run,
        profile,
        ..
    } = f;
    let (mut ledger, mut version) = stored.into_parts();
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
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
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
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
        60,
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
        return Err("actual target patch did not verify its current Diff".into());
    };
    assert_eq!(run.state(), AgentControllerState::Verify);
    let run_id = run.id();
    drop((store, refresh, compiler));
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
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
    let [VerificationEvidence::Diff(diff)] = evidence.evidence() else {
        return Err("actual cold Diff missing".into());
    };
    assert!(diff.complete());
    assert_eq!(diff.snapshot_id(), snapshot_id);
    assert_eq!(
        cold.load_agent_run(&fixture.project, run_id)
            .await?
            .ok_or("cold run missing")?
            .state(),
        AgentControllerState::Verify
    );
    assert_eq!(
        cold.load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("cold goal missing")?
            .original(),
        goal.original()
    );
    drop(cold);
    Ok(TargetPatchProof {
        fixture,
        goal,
        run_id,
        evidence_id,
        snapshot_id,
    })
}

const SCOPE_PREFIX: &str = "# scope α\nclass Handler:\n";
const SCOPE_HEADER: &str = "    def target(self, value):\n";
const SCOPE_BODY: &str = "        return value + 1\n";
async fn scope_fixture(whole: bool) -> TestResult<TargetFixture> {
    let text = [SCOPE_PREFIX, SCOPE_HEADER, SCOPE_BODY].concat();
    let (start, first) = if whole {
        (SCOPE_PREFIX.len(), 2)
    } else {
        (SCOPE_PREFIX.len() + SCOPE_HEADER.len(), 3)
    };
    let span = SourceRange::new(
        start,
        text.len(),
        SourcePosition::new(first, 0),
        SourcePosition::new(4, 0),
    )?;
    let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
    let (goal, step) = path_edit_goal(
        &path,
        b"Edit only the current target window; preserve all other bytes.\n",
    )?;
    TargetFixture::start_using_span(
        &text,
        target_profile()?,
        goal,
        step,
        Fixture::new()?,
        Some(span),
    )
    .await
}

#[test]
fn edit_scope_projection_complete_line_hull_is_whole_callable_without_changing_need_offer()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = Box::pin(scope_fixture(true)).await?;
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let compiled = f.compile(&phase, &f.acquired)?;
            let view = compiled.request().messages()[1].content();
            assert!(view.contains("Scope: \"subject.py\":3-4; whole method \"target\".\n"));
            let declaration = f
                .published
                .publication()
                .graph()
                .symbols()
                .iter()
                .find(|symbol| symbol.parsed().name().as_str() == "target")
                .ok_or("target")?;
            assert_ne!(
                f.bound.required().range(),
                declaration.parsed().declaration_range()
            );
            assert!(http_correction_definition_page_matches(
                &EvidenceRef::new(
                    declaration.revision().clone(),
                    declaration.parsed().declaration_range()
                ),
                &EvidenceRef::new(f.page.revision().clone(), f.page.range())
            ));
            // Full line hull is not the exact parser range used by the independent Need contract.
            assert!(
                !compiled
                    .request()
                    .structured_output()
                    .ok_or("schema")?
                    .value()
                    .to_string()
                    .contains("\"need\"")
            );
            assert_eq!(
                compiled
                    .request()
                    .messages()
                    .last()
                    .map(ModelMessage::content),
                Some(phase.input_json())
            );
            let provider = TargetProvider::new(&f, ControlledReply::Proposal)?;
            for (kind, request) in [
                ("initial", f.compile(&phase, &f.acquired)?),
                ("repair", compiled.for_structural_repair()?),
                ("truncated", compiled.for_truncated_output_repair()?),
            ] {
                let after = provider.project_wire_request(request.request())?;
                let budget = PhaseWireBudget::fit(after, request.request().profile())?;
                // Exact same fixture and schema; remove only this new data line for before measurement.
                let messages = request
                    .request()
                    .messages()
                    .iter()
                    .map(|message| {
                        ModelMessage::try_from_string(
                            message.role(),
                            message
                                .content()
                                .split_inclusive('\n')
                                .filter(|line| !line.starts_with("Scope:"))
                                .collect::<String>(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let before_request = ModelProviderRequest::new(
                    request.request().profile().clone(),
                    messages,
                    request.request().structured_output().cloned(),
                )?;
                let before = provider.project_wire_request(&before_request)?;
                let before_budget = PhaseWireBudget::fit(before, before_request.profile())?;
                assert_eq!(
                    (after.context_tokens(), after.output_tokens()),
                    (4096, 1024)
                );
                assert_eq!(budget.output_tokens(), before_budget.output_tokens());
                eprintln!(
                    "edit_scope_wire kind={kind} before_bytes={} after_bytes={} before_prompt={} after_prompt={} reserve={}",
                    before.request_bytes(),
                    after.request_bytes(),
                    before_budget.prompt_tokens(),
                    budget.prompt_tokens(),
                    budget.output_tokens()
                );
            }
            assert_eq!(
                f.store
                    .load_edit_phase_usage(&f.fixture.project, f.run.id())
                    .await?
                    .calls,
                0
            );
            drop(provider);
            f.cleanup()
        })
    })
}

#[test]
fn edit_scope_projection_partial_current_page_is_part_and_unbound_page_makes_no_callable_claim()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = Box::pin(scope_fixture(false)).await?;
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let bound = f.compile(&phase, &f.acquired)?;
            assert!(
                bound.request().messages()[1]
                    .content()
                    .contains("Scope: \"subject.py\":4-4; part method \"target\".\n")
            );
            assert!(
                !bound.request().messages()[1]
                    .content()
                    .contains("whole method")
            );
            let plain = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none(),
            )?;
            assert!(!plain.request().messages()[1].content().contains("Scope:"));
            assert_ne!(plain.request_digest(), bound.request_digest());
            assert_eq!(
                plain.request().messages().last(),
                bound.request().messages().last()
            );
            // No role was introduced by either complete-line/partial descriptive scope.
            assert_eq!(
                plain.request().structured_output(),
                bound.request().structured_output()
            );
            f.cleanup()
        })
    })
}

#[test]
fn edit_scope_projection_rejects_same_snapshot_new_publication_before_emitting_a_scope()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = Box::pin(scope_fixture(true)).await?;
            let old = f.published.run();
            let new_index = PublishedIndex::new(
                a3_domain::IndexRunRecord::new(
                    a3_domain::IndexRunId::from_bytes([181; 32]),
                    old.snapshot_id(),
                    old.ranking_policy_version(),
                    a3_domain::IndexRunSequence::new(old.sequence().get() + 1)?,
                    a3_domain::IndexRunStatus::Published,
                ),
                f.published.publication().clone(),
            )?;
            let current =
                EditPhaseState::new(&f.fixture.project, &f.goal, &f.stored, &f.run, &new_index)?;
            let phase = PreparedEditPhase::update(current, &f.page)?;
            assert!(matches!(
                build_edit_phase_request(
                    &f.goal,
                    &f.stored,
                    &phase,
                    current,
                    f.profile.clone(),
                    None,
                    EditPhasePrerequisites::none().with_required_source(
                        EditEvidenceNeed::Definition,
                        &f.bound,
                        Some((&f.page, &f.acquired))
                    )
                ),
                Err(EditPhaseRequestFailure::Stale)
            ));
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
