//! Negative native contracts for the narrow current NoChange fact and closure.
//! Controlled provider accounting is not an Ollama answer or a functional HTTP pass.
use super::*;
use a3_application::{
    AgentActionStoreFailure, AgentControllerControl, AuthorizedPatchAction, EditNoChangeWindow,
    EditPhaseReservationFailure, EditPhaseReservedUsage, PatchApplyFuture, PatchPreviewFuture,
    PatchUpdateBaseFuture, RunEventPageLimit, WorkspacePatchControl, WorkspacePatchProgressError,
    WorkspacePatchTool,
};
use a3_domain::{PatchAction, Progress, RunEvent};
use std::sync::atomic::AtomicBool;

// A bounded assertion snapshot, not an alternate persistence path. Capture after
// deliberate fixture corruption, before invoking the real loader/usecase.
pub(super) struct Retained {
    charged: EditPhaseReservedUsage,
    events: Vec<RunEvent>,
    reads: i64,
    abandoned: i64,
    receipts: i64,
    mutations: i64,
}
impl Retained {
    pub(super) async fn capture(f: &HttpEditFixture) -> TestResult<Self> {
        Ok(Self {
            charged: f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?,
            events: f
                .http
                .store
                .load_run_events(
                    &f.http.fixture.project,
                    f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?,
                )
                .await?
                .events()
                .to_vec(),
            reads: scalar(f, "SELECT count(*) FROM source_read_acquisitions").await?,
            abandoned: scalar(
                f,
                "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1",
            )
            .await?,
            receipts: scalar(f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            mutations: scalar(f, "SELECT count(*) FROM mutation_attempts").await?,
        })
    }
    pub(super) async fn assert_same(
        &self,
        f: &HttpEditFixture,
        run: &AgentRun,
        ledger: &TaskLedger,
        version: TaskLedgerStoreVersion,
    ) -> TestResult {
        self.assert_same_with_usage(f, run, ledger, version, self.charged)
            .await
    }
    // A real later model call must keep its debit while all non-model state stays unchanged.
    pub(super) async fn assert_same_with_usage(
        &self,
        f: &HttpEditFixture,
        run: &AgentRun,
        ledger: &TaskLedger,
        version: TaskLedgerStoreVersion,
        charged: EditPhaseReservedUsage,
    ) -> TestResult {
        assert_eq!(run, &f.run);
        assert_eq!(&StoredTaskLedger::new(ledger.clone(), version), &f.stored);
        assert_eq!(
            f.http
                .store
                .load_agent_run(&f.http.fixture.project, f.run.id())
                .await?,
            Some(f.run.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_current_goal_contract(&f.http.fixture.project, f.http.goal.task_id())
                .await?,
            Some(f.http.goal.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(&f.http.fixture.project, f.http.goal.task_id())
                .await?,
            Some(f.stored.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?,
            charged
        );
        assert_eq!(
            f.http
                .store
                .load_run_events(
                    &f.http.fixture.project,
                    f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?
                )
                .await?
                .events(),
            self.events
        );
        assert_eq!(
            scalar(f, "SELECT count(*) FROM source_read_acquisitions").await?,
            self.reads
        );
        assert_eq!(
            scalar(
                f,
                "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
            )
            .await?,
            self.abandoned
        );
        assert_eq!(
            scalar(f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            self.receipts
        );
        assert_eq!(
            scalar(f, "SELECT count(*) FROM mutation_attempts").await?,
            self.mutations
        );
        assert_eq!(
            scalar(
                f,
                "SELECT count(*) FROM edit_phase_reservations WHERE outcome='accepted_reply'"
            )
            .await?,
            0
        );
        Ok(())
    }
}

#[test]
fn no_change_cold_fact_requires_exact_goal_window_profile_and_actual_original_read() -> TestResult {
    crate::run_native_libsql_test(async {
        // Direct private-fixture faults exercise fail-closed replay, never a
        // product mutation API. Each fixture starts from an actual NoChange.
        for fault in [
            "missing",
            "foreign-profile",
            "foreign-index",
            "goal",
            "window",
            "same-ref-generation",
            "same-ref-context",
            "original-range",
            "missing-original",
            "foreign-read",
        ] {
            let (f, page) = unchanged().await?;
            let before = inventory(&f.http.root)?;
            let db = f.http.fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            if fault == "missing" {
                connection
                    .execute("DROP TRIGGER edit_no_change_no_delete", ())
                    .await?;
                assert_eq!(
                    connection
                        .execute("DELETE FROM edit_phase_no_change_receipts", ())
                        .await?,
                    1
                );
            } else if matches!(fault, "missing-original" | "foreign-read") {
                connection
                    .execute("DROP TRIGGER source_read_identity_immutable", ())
                    .await?;
                let sql = if fault == "missing-original" {
                    "UPDATE source_read_acquisitions SET original_evidence_id=NULL"
                } else {
                    "UPDATE source_read_acquisitions SET scope_digest=zeroblob(32)"
                };
                assert_eq!(connection.execute(sql, ()).await?, 1);
            } else {
                connection
                    .execute("DROP TRIGGER edit_no_change_no_update", ())
                    .await?;
                let sql = match fault {
                    "foreign-profile" => {
                        "UPDATE edit_phase_no_change_receipts SET profile_id=zeroblob(32)"
                    }
                    "foreign-index" => {
                        "UPDATE edit_phase_no_change_receipts SET index_run_id=zeroblob(32)"
                    }
                    "goal" => {
                        "UPDATE edit_phase_no_change_receipts SET original_goal_digest=zeroblob(32)"
                    }
                    "window" => {
                        "UPDATE edit_phase_no_change_receipts SET window_digest=zeroblob(32)"
                    }
                    // Deliberately keep profile_id AND profile_version unchanged:
                    // public safe ModelProfile constructors cannot forge its reference.
                    "same-ref-generation" => {
                        "UPDATE edit_phase_no_change_receipts SET generation_limit=generation_limit+1"
                    }
                    "same-ref-context" => {
                        "UPDATE edit_phase_no_change_receipts SET context_limit=context_limit+1"
                    }
                    "original-range" => {
                        "UPDATE edit_phase_no_change_receipts SET original_range=zeroblob(24)"
                    }
                    _ => return Err("unknown private NoChange fault".into()),
                };
                assert_eq!(connection.execute(sql, ()).await?, 1);
            }
            drop((connection, db));
            let f = reopen(f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold index")?;
            let phase = PreparedEditPhase::update(state(&f, &current)?, &page)?;
            let retained = Retained::capture(&f).await?;
            let result = f
                .http
                .store
                .load_edit_no_change(
                    &f.http.fixture.project,
                    &phase,
                    &f.http.model,
                    &current,
                    after_phases(&f).await?,
                )
                .await;
            assert!(
                matches!(result, Err(EditPhaseReservationFailure::InvalidStoredData)),
                "fault {fault} was not an integrity rejection: {result:?}"
            );
            let mut run = f.run.clone();
            let (mut ledger, version) = f.stored.clone().into_parts();
            let result = CloseEditNoChange::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspacePatchAdapter::with_change_window_preview(),
            )
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                &f.http.model,
                &page,
                after_phases(&f).await?,
                &ActiveControl,
            )
            .await;
            assert!(
                matches!(
                    result,
                    Err(EditNoChangeFailure::Reservation(
                        EditPhaseReservationFailure::InvalidStoredData
                    ))
                ),
                "fault {fault} escaped the real closure: {result:?}"
            );
            retained.assert_same(&f, &run, &ledger, version).await?;
            assert_eq!(inventory(&f.http.root)?, before);
            drop((current, phase));
            cleanup(f)?;
        }
        Ok(())
    })
}

pub(in super::super::super) async fn prepared_16k() -> TestResult<(
    HttpEditFixture,
    PreparedEditPhase,
    CompiledEditPhaseRequest,
    AgentSourcePage,
)> {
    let intent = HttpCorrectionPlanIntent::try_from_string(
        "Change only the observed HTTP handler; preserve the complete original proof".to_owned(),
    )?;
    let model = ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::SixteenK)?;
    let (mut f, _) = intent_preflight_fixture_with_profile(model, &intent).await?;
    let (phase, compiled, page, _) = acquire(&mut f).await?;
    Ok((f, phase, compiled, page))
}
pub(super) fn actual_wire(compiled: &CompiledEditPhaseRequest) -> TestResult<EditPhaseWireCost> {
    let encoder = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let projection = encoder.project_wire_request(compiled.request())?;
    let budget = PhaseWireBudget::fit(projection, compiled.request().profile())?;
    Ok(EditPhaseWireCost::new(
        projection.digest(),
        projection.request_bytes(),
        budget.prompt_tokens(),
        budget.output_tokens(),
    )?)
}
pub(super) async fn prove_unchanged(
    f: &HttpEditFixture,
    phase: &PreparedEditPhase,
    page: &AgentSourcePage,
    current: &PublishedIndex,
) -> TestResult<EditNoChangeWindow> {
    // Independent fixture equality for intentionally forged usage/time observations.
    // The separate real Executor NoChange test proves production lowering/Stop.
    assert_eq!(phase.anchor(), state(f, current)?.anchor());
    let full = fs::read(f.http.root.join("server.py"))?;
    assert_eq!(
        ContentHash::from_bytes(*blake3::hash(&full).as_bytes()),
        page.revision().content_hash()
    );
    assert_eq!(
        full.get(page.range().start_byte() as usize..page.range().end_byte() as usize),
        Some(page.text().as_bytes())
    );
    let window = phase.no_change_window().ok_or("actual Replace window")?;
    assert!(window.matches_original(&EvidenceRef::new(page.revision().clone(), page.range())));
    Ok(window)
}

async fn terminal(f: &HttpEditFixture, expected: &str, output: u32) -> TestResult {
    let db = f.http.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let mut rows = connection.query("SELECT outcome,reported_output,terminal_reply_kind FROM edit_phase_reservations WHERE call_kind='initial'", ()).await?;
    let row = rows.next().await?.ok_or("terminal observation missing")?;
    assert_eq!(row.get::<String>(0)?, expected);
    assert_eq!(row.get::<i64>(1)?, i64::from(output));
    assert!(row.get::<Option<String>>(2)?.is_none());
    assert!(rows.next().await?.is_none());
    Ok(())
}

#[test]
fn no_change_finish_generation_cap_and_deadline_settle_without_null_tag_update_or_refund()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (output, expired) in [(1024, false), (1025, false), (3605, false), (32, true)] {
            let (f, phase, compiled, page) = prepared_16k().await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let before = inventory(&f.http.root)?;
            let wire = actual_wire(&compiled)?;
            assert_eq!(f.http.model.settings().output_limit().get(), 1024);
            assert_eq!(f.http.model.settings().context_limit().get(), 16384);
            assert_eq!(wire.output_reserve(), 3605);
            let read = f
                .http
                .store
                .load_source_read_state(&f.http.fixture.project, phase.anchor())
                .await?;
            assert!(read.acquired().is_some());
            let permit = f
                .http
                .store
                .reserve_edit_job(
                    &f.http.fixture.project,
                    &phase,
                    &compiled,
                    wire,
                    after_phases(&f).await?,
                )
                .await?;
            let reserved = f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?;
            let window = prove_unchanged(&f, &phase, &page, &current).await?;
            let finished = if expired {
                permit.deadline()
            } else {
                at(permit.reserved_at().unix_millis() + 1)?
            };
            let result = f
                .http
                .store
                .finish_edit_no_change(
                    &f.http.fixture.project,
                    permit,
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::NoChange,
                        Some(128),
                        Some(output),
                        finished,
                    ),
                    &window,
                    &current,
                    &f.http.model,
                )
                .await;
            if output == 1024 {
                result?;
                assert_eq!(
                    scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                    1
                );
            } else {
                let expected = if expired {
                    EditPhaseReservationFailure::DeadlineReached
                } else {
                    EditPhaseReservationFailure::BudgetExhausted(AgentBudgetDimension::OutputTokens)
                };
                assert_eq!(result, Err(expected));
                terminal(&f, if expired { "stale" } else { "unknown" }, output).await?;
                assert_eq!(
                    scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                    0
                );
            }
            let settled = f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?;
            assert_eq!(settled.calls, reserved.calls);
            assert_eq!(settled.repairs, reserved.repairs);
            assert_eq!(settled.prompt_tokens, reserved.prompt_tokens);
            assert_eq!(settled.output_tokens, reserved.output_tokens);
            assert_eq!(settled.unobserved_calls, 0);
            assert_eq!(settled.unknown_calls, u32::from(!expired && output > 1024));
            drop((current, compiled, phase, read));
            let f = reopen(f).await?;
            let observed_now = after_phases(&f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold index")?;
            let phase = PreparedEditPhase::update(state(&f, &current)?, &page)?;
            let retained = Retained::capture(&f).await?;
            let receipt = f
                .http
                .store
                .load_edit_no_change(
                    &f.http.fixture.project,
                    &phase,
                    &f.http.model,
                    &current,
                    observed_now,
                )
                .await?;
            assert_eq!(receipt.is_some(), output == 1024);
            let mut run = f.run.clone();
            let (mut ledger, version) = f.stored.clone().into_parts();
            let result = CloseEditNoChange::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspacePatchAdapter::with_change_window_preview(),
            )
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                &f.http.model,
                &page,
                observed_now,
                &ActiveControl,
            )
            .await;
            if output == 1024 {
                result?;
                assert_eq!(run.state(), AgentControllerState::Replan);
                assert_eq!(run.created_at(), f.run.created_at());
                assert_eq!(run.budget(), f.run.budget());
                assert_eq!(run.usage(), f.run.usage());
                assert_eq!(
                    f.http
                        .store
                        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                        .await?,
                    settled
                );
                assert_eq!(
                    scalar(
                        &f,
                        "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
                    )
                    .await?,
                    1
                );
            } else {
                assert!(
                    matches!(
                        result,
                        Err(EditNoChangeFailure::NoObservation
                            | EditNoChangeFailure::BudgetExhausted)
                    ),
                    "negative terminal result must remain typed: {result:?}"
                );
                retained.assert_same(&f, &run, &ledger, version).await?;
            }
            assert_eq!(inventory(&f.http.root)?, before);
            drop((phase, current, receipt));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn no_change_repair_authenticates_initial_positive_usage_and_actual_generation_cap() -> TestResult {
    crate::run_native_libsql_test(async {
        for prior_output in [1024, 1025, 3605] {
            let (f, phase, compiled, page) = prepared_16k().await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let before = inventory(&f.http.root)?;
            let read = f
                .http
                .store
                .load_source_read_state(&f.http.fixture.project, phase.anchor())
                .await?;
            assert!(read.acquired().is_some());
            let first = f
                .http
                .store
                .reserve_edit_job(
                    &f.http.fixture.project,
                    &phase,
                    &compiled,
                    actual_wire(&compiled)?,
                    after_phases(&f).await?,
                )
                .await?;
            let deadline = first.deadline();
            let first_finish = at(first.reserved_at().unix_millis() + 1)?;
            // The current Edit3 Initial binds the actual full profile before any repair.
            // Over-generation is Unknown immediately; no legacy reservation can bypass it.
            let first_result = f
                .http
                .store
                .finish_edit_job(
                    &f.http.fixture.project,
                    first,
                    &compiled,
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::InvalidStructure,
                        Some(128),
                        Some(prior_output),
                        first_finish,
                    ),
                    None,
                )
                .await;
            if prior_output > 1024 {
                assert_eq!(
                    first_result,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::OutputTokens
                    ))
                );
                terminal(&f, "unknown", prior_output).await?;
                let before_repair = f
                    .http
                    .store
                    .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                    .await?;
                assert_eq!(before_repair.unknown_calls, 1);
                let repair = compiled.for_structural_repair()?;
                assert!(matches!(
                    f.http
                        .store
                        .reserve_edit_job(
                            &f.http.fixture.project,
                            &phase,
                            &repair,
                            actual_wire(&repair)?,
                            after_phases(&f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::UncertainProviderCall)
                ));
                assert_eq!(
                    f.http
                        .store
                        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                        .await?,
                    before_repair
                );
                assert_eq!(
                    scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                    0
                );
                assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE call_kind='structural_repair'").await?,0);
                assert_eq!(inventory(&f.http.root)?, before);
                drop((phase, compiled, read, current));
                let f = reopen(f).await?;
                let current = f
                    .http
                    .store
                    .latest_published_index(&f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("cold index")?;
                let phase = PreparedEditPhase::update(state(&f, &current)?, &page)?;
                assert!(
                    f.http
                        .store
                        .load_edit_no_change(
                            &f.http.fixture.project,
                            &phase,
                            &f.http.model,
                            &current,
                            after_phases(&f).await?
                        )
                        .await?
                        .is_none()
                );
                drop((phase, current));
                cleanup(f)?;
                continue;
            }
            first_result?;
            let repair = compiled.for_structural_repair()?;
            let wire = actual_wire(&repair)?;
            assert_ne!(wire.digest(), actual_wire(&compiled)?.digest());
            assert_eq!(wire.output_reserve(), 3605);
            let permit = f
                .http
                .store
                .reserve_edit_job(
                    &f.http.fixture.project,
                    &phase,
                    &repair,
                    wire,
                    after_phases(&f).await?,
                )
                .await?;
            assert_eq!(permit.deadline(), deadline);
            let window = prove_unchanged(&f, &phase, &page, &current).await?;
            let finished = at(permit.reserved_at().unix_millis() + 1)?;
            let charged = f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?;
            let result = f
                .http
                .store
                .finish_edit_no_change(
                    &f.http.fixture.project,
                    permit,
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::NoChange,
                        Some(128),
                        Some(32),
                        finished,
                    ),
                    &window,
                    &current,
                    &f.http.model,
                )
                .await;
            if prior_output == 1024 {
                result?;
            } else {
                assert_eq!(result, Err(EditPhaseReservationFailure::InvalidStoredData));
            }
            let settled = f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?;
            assert_eq!(
                (
                    settled.calls,
                    settled.repairs,
                    settled.prompt_tokens,
                    settled.output_tokens
                ),
                (
                    charged.calls,
                    charged.repairs,
                    charged.prompt_tokens,
                    charged.output_tokens
                )
            );
            assert_eq!(settled.unobserved_calls, 0);
            assert_eq!(settled.unknown_calls, u32::from(prior_output > 1024));
            assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE call_kind='initial' AND outcome='invalid_structure'").await?,1);
            assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE call_kind='structural_repair' AND finished_at IS NOT NULL").await?,1);
            drop((phase, compiled, repair, read, current));
            let f = reopen(f).await?;
            let observed_now = after_phases(&f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold index")?;
            let phase = PreparedEditPhase::update(state(&f, &current)?, &page)?;
            let receipt = f
                .http
                .store
                .load_edit_no_change(
                    &f.http.fixture.project,
                    &phase,
                    &f.http.model,
                    &current,
                    observed_now,
                )
                .await?;
            assert_eq!(receipt.is_some(), prior_output == 1024);
            let retained = Retained::capture(&f).await?;
            let mut run = f.run.clone();
            let (mut ledger, version) = f.stored.clone().into_parts();
            let result = CloseEditNoChange::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspacePatchAdapter::with_change_window_preview(),
            )
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                &f.http.model,
                &page,
                observed_now,
                &ActiveControl,
            )
            .await;
            if prior_output == 1024 {
                result?;
                assert_eq!(run.state(), AgentControllerState::Replan);
                assert_eq!(run.created_at(), f.run.created_at());
                assert_eq!(run.budget(), f.run.budget());
                assert_eq!(run.usage(), f.run.usage());
                assert_eq!(
                    f.http
                        .store
                        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                        .await?,
                    settled
                );
                assert_eq!(
                    scalar(
                        &f,
                        "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
                    )
                    .await?,
                    1
                );
            } else {
                assert!(matches!(result, Err(EditNoChangeFailure::NoObservation)));
                retained.assert_same(&f, &run, &ledger, version).await?;
            }
            assert_eq!(inventory(&f.http.root)?, before);
            drop((phase, current, receipt));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[derive(Debug, Default)]
struct NoChangeControl(AtomicBool);
impl AgentControllerControl for NoChangeControl {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
impl WorkspacePatchControl for NoChangeControl {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    fn report_progress(&self, _: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterSecondRead {
    SameSnapshot,
    ChangedSnapshot,
    Cancel,
    Deadline,
}
struct ObserveSafeBase<'a> {
    base: WorkspacePatchAdapter,
    fixture: &'a Fixture,
    control: &'a NoChangeControl,
    effect: AfterSecondRead,
    reads: AtomicUsize,
    revisions: Mutex<Vec<FileRevision>>,
}
impl std::fmt::Debug for ObserveSafeBase<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObserveSafeBase")
            .field("effect", &self.effect)
            .finish_non_exhaustive()
    }
}
impl WorkspacePatchTool for ObserveSafeBase<'_> {
    fn read_update_base<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        current: &'a PublishedIndex,
        expected: &'a FileRevision,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchUpdateBaseFuture<'a> {
        Box::pin(async move {
            // The actual safety adapter must succeed before any effect/cancellation.
            let content = self
                .base
                .read_update_base(project, current, expected, control)
                .await?;
            assert_eq!(hash(content.as_bytes()), expected.content_hash());
            let count = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            assert!(count <= 2, "unexpected extra validation read");
            self.revisions
                .lock()
                .map_err(|_| a3_application::PatchPreviewFailure::Unavailable)?
                .push(expected.clone());
            if count == 2 {
                match self.effect {
                    AfterSecondRead::SameSnapshot => {
                        let result =
                            super::super::super::apply::republish_unchanged_index(self.fixture)
                                .await;
                        assert!(
                            result.is_ok(),
                            "actual same-snapshot publisher failed: {result:?}"
                        );
                    }
                    AfterSecondRead::ChangedSnapshot => {
                        let marker = self
                            .fixture
                            .root
                            .join("worktree/nochange-publication-marker.txt");
                        assert!(!marker.exists());
                        let write_result = fs::write(marker, b"new actually published state\n");
                        assert!(
                            write_result.is_ok(),
                            "private marker write failed: {write_result:?}"
                        );
                        let publication_result = index_fixture(self.fixture).await;
                        assert!(
                            publication_result.is_ok(),
                            "actual changed-snapshot publisher failed: {:?}",
                            publication_result.as_ref().err()
                        );
                        if let Ok((store, refresh, compiler, publication)) = publication_result {
                            assert_ne!(publication.run().id(), current.run().id());
                            assert_ne!(
                                publication.run().snapshot_id(),
                                current.run().snapshot_id()
                            );
                            drop((store, refresh, compiler, publication));
                        }
                    }
                    AfterSecondRead::Cancel => self.control.0.store(true, Ordering::SeqCst),
                    AfterSecondRead::Deadline => std::thread::sleep(Duration::from_millis(2200)),
                }
            }
            Ok(content)
        })
    }
    fn preview<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        current: &'a PublishedIndex,
        action: &'a PatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchPreviewFuture<'a> {
        self.base.preview(project, current, action, control)
    }
    fn apply<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        current: &'a PublishedIndex,
        action: AuthorizedPatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchApplyFuture<'a> {
        self.base.apply(project, current, action, control)
    }
}

#[test]
fn no_change_second_actual_safe_read_cannot_hide_publication_cancel_or_elapsed_deadline()
-> TestResult {
    crate::run_native_libsql_test(async {
        for effect in [
            AfterSecondRead::SameSnapshot,
            AfterSecondRead::ChangedSnapshot,
            AfterSecondRead::Cancel,
            AfterSecondRead::Deadline,
        ] {
            let (f, page) = unchanged().await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let before = inventory(&f.http.root)?;
            let server = fs::read(f.http.root.join("server.py"))?;
            let retained = Retained::capture(&f).await?;
            assert_eq!(retained.abandoned, 0);
            let control = NoChangeControl::default();
            let patch = ObserveSafeBase {
                base: WorkspacePatchAdapter::with_change_window_preview(),
                fixture: &f.http.fixture,
                control: &control,
                effect,
                reads: AtomicUsize::new(0),
                revisions: Mutex::new(Vec::new()),
            };
            let observed = if effect == AfterSecondRead::Deadline {
                at(
                    f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis()
                        - 2000,
                )?
            } else {
                after_phases(&f).await?
            };
            let mut run = f.run.clone();
            let (mut ledger, version) = f.stored.clone().into_parts();
            let result =
                CloseEditNoChange::new(f.http.store.as_ref(), f.http.store.as_ref(), &patch)
                    .execute(
                        &f.http.fixture.project,
                        &f.http.goal,
                        &mut run,
                        &mut ledger,
                        version,
                        &current,
                        &f.http.model,
                        &page,
                        observed,
                        &control,
                    )
                    .await;
            assert_eq!(
                patch.reads.load(Ordering::SeqCst),
                2,
                "both real source validations are required: {result:?}"
            );
            assert_eq!(
                *patch.revisions.lock().map_err(|_| "read trace")?,
                vec![page.revision().clone(); 2]
            );
            match effect {
                AfterSecondRead::SameSnapshot | AfterSecondRead::ChangedSnapshot => {
                    let latest = f
                        .http
                        .store
                        .latest_published_index(&f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("actual new publication")?;
                    assert_ne!(
                        latest.run().id(),
                        current.run().id(),
                        "race fixture did not publish a new index"
                    );
                    assert_eq!(
                        latest.run().snapshot_id() == current.run().snapshot_id(),
                        effect == AfterSecondRead::SameSnapshot
                    );
                    assert!(
                        latest
                            .publication()
                            .graph()
                            .files()
                            .contains(page.revision())
                    );
                    assert!(
                        matches!(
                            result,
                            Err(EditNoChangeFailure::Storage(
                                AgentActionStoreFailure::SnapshotConflict
                            ))
                        ),
                        "exact-index CAS must reject after both validations: {result:?}"
                    );
                }
                AfterSecondRead::Cancel => {
                    assert!(matches!(result, Err(EditNoChangeFailure::Cancelled)))
                }
                AfterSecondRead::Deadline => {
                    assert!(matches!(result, Err(EditNoChangeFailure::BudgetExhausted)))
                }
            }
            retained.assert_same(&f, &run, &ledger, version).await?;
            let phase = PreparedEditPhase::update(state(&f, &current)?, &page)?;
            let reads = f
                .http
                .store
                .load_source_read_state(&f.http.fixture.project, phase.anchor())
                .await;
            if effect == AfterSecondRead::ChangedSnapshot {
                // The generic loader is current-only. A real changed publication
                // must reject the old owner; no historical read bypass is introduced.
                assert!(matches!(
                    reads,
                    Err(a3_application::SourceReadFailure::Reservation(
                        EditPhaseReservationFailure::StaleAnchor
                    ))
                ));
            } else {
                // SameSnapshot deliberately keeps this current-only source loader
                // admissible even though the exact-index action CAS rejected.
                let reads = reads?;
                assert!(reads.progress().receipts().is_empty());
                assert!(reads.acquired().is_none());
            }
            assert_eq!(fs::read(f.http.root.join("server.py"))?, server);
            if effect != AfterSecondRead::ChangedSnapshot {
                assert_eq!(inventory(&f.http.root)?, before);
            } else {
                fs::remove_file(f.http.root.join("nochange-publication-marker.txt"))?;
                assert_eq!(inventory(&f.http.root)?, before);
            }
            drop((phase, patch, current));
            cleanup(f)?;
        }
        Ok(())
    })
}
