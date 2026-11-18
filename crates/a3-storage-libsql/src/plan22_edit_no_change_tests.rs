//! Actual source/DB/Executor regressions; the provider response is a declared native control.
//! No local-model quality, product HTTP success or P7 acceptance is claimed.
use super::*;
use a3_application::{
    CloseEditNoChange, EditNoChangeFailure, EditPhaseExecutionFailure, EditPhaseFailure,
    EditPhaseObservation, EditPhaseObservationOutcome, EditPhaseWireCost, PhaseWireBudget,
};
use a3_domain::TaskStepAttemptOutcome;

async fn prepared() -> TestResult<(
    HttpEditFixture,
    PreparedEditPhase,
    CompiledEditPhaseRequest,
    AgentSourcePage,
)> {
    let intent = HttpCorrectionPlanIntent::try_from_string(
        "Change only the observed HTTP handler; preserve the complete original proof".to_owned(),
    )?;
    let (mut f, _) = intent_preflight_fixture("qwen38-4k:latest", &intent).await?;
    let (phase, compiled, page, _) = acquire(&mut f).await?;
    Ok((f, phase, compiled, page))
}
async fn unchanged() -> TestResult<(HttpEditFixture, AgentSourcePage)> {
    let (f, phase, mut compiled, page) = prepared().await?;
    let published = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let provider = NativeEdit {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        store: f.http.store.as_ref(),
        project: &f.http.fixture.project,
        run: f.run.id(),
        // Exactly the actual acquired current Original, no repair/golden replacement.
        reply: json!({"code":page.text()}).to_string(),
        calls: AtomicUsize::new(0),
    };
    let window = phase.no_change_window();
    let result = ExecuteEditPhase::new(
        &provider,
        f.http.store.as_ref(),
        &WorkspacePatchAdapter::with_change_window_preview(),
    )
    .execute(
        Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
            &f.http.compiler,
            phase,
            &mut compiled,
            state(&f, &published)?,
            after_phases(&f).await?,
            &WorkspacePatchAdapter::with_change_window_preview(),
            &ActiveControl,
        ))
        .await?,
        state(&f, &published)?,
        &compiled,
        after_phases(&f).await?,
        ModelRequestTimeout::from_millis(120000)?,
        &ActiveControl,
    )
    .await;
    assert!(
        matches!(
            &result,
            Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
        ),
        "actual NoChange execution: {result:?}; window={window:?}; Original={:?}",
        page.range()
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    drop((provider, compiled, published));
    Ok((f, page))
}
async fn scalar(f: &HttpEditFixture, sql: &str) -> TestResult<i64> {
    let db = f.http.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let mut rows = connection.query(sql, ()).await?;
    let row = rows.next().await?.ok_or("scalar")?;
    Ok(row.get(0)?)
}
fn cleanup(f: HttpEditFixture) -> TestResult {
    drop((f.http.store, f.http.refresh, f.http.compiler));
    f.http.fixture.cleanup()
}

#[test]
fn no_change_actual_finish_cold_replay_and_atomic_failed_replan_keep_all_costs() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, page) = unchanged().await?;
        let before = inventory(&f.http.root)?;
        let charged = f
            .http
            .store
            .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
            .await?;
        let mutations = scalar(&f, "SELECT count(*) FROM mutation_attempts").await?;
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            1
        );
        assert_eq!(scalar(&f,"SELECT count(*) FROM edit_phase_reservations WHERE outcome='rejected_reply' AND terminal_reply_kind='no_change'").await?,1);
        assert_eq!(
            scalar(
                &f,
                "SELECT count(*) FROM edit_phase_reservations WHERE outcome='accepted_reply'"
            )
            .await?,
            0
        );
        // Cut after settlement/before closure: discard the process-local permit/phase/stores.
        let mut f = reopen(f).await?;
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
                after_phases(&f).await?,
            )
            .await?
            .ok_or("authentic cold NoChange missing")?;
        assert_eq!(receipt.original().range(), page.range());
        let read = f
            .http
            .store
            .load_source_read_state(&f.http.fixture.project, phase.anchor())
            .await?;
        assert!(read.progress().receipts().is_empty());
        assert!(read.acquired().is_none());
        let original_run = f.run.clone();
        let original_goal = f.http.goal.clone();
        let (mut ledger, version) = f.stored.clone().into_parts();
        let mut run = f.run.clone();
        let next = CloseEditNoChange::new(
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
        .await?;
        assert_eq!(run.state(), AgentControllerState::Replan);
        assert_eq!(run.created_at(), original_run.created_at());
        assert_eq!(run.budget(), original_run.budget());
        assert_eq!(run.usage(), original_run.usage());
        let failed = ledger.step(receipt.anchor().step()).ok_or("failed step")?;
        assert_eq!(failed.status(), TaskStepStatus::Failed);
        assert!(matches!(
            failed.attempts().last().map(|a| a.outcome()),
            Some(TaskStepAttemptOutcome::Failed { .. })
        ));
        assert_eq!(
            scalar(
                &f,
                "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
            )
            .await?,
            1
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, run.id())
                .await?,
            charged
        );
        assert_eq!(inventory(&f.http.root)?, before);
        assert_eq!(f.http.goal, original_goal);
        f.run = run;
        f.stored = StoredTaskLedger::new(ledger, next);
        drop((current, phase, receipt, read));
        // Cut after closure: actual cold Ledger/Run retained, no duplicate closure/model job.
        let f = reopen(f).await?;
        assert_eq!(f.run.state(), AgentControllerState::Replan);
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?,
            charged
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            1
        );
        cleanup(f)
    })
}

#[test]
fn no_change_generic_rejected_cannot_be_promoted_and_unsettled_call_stays_charged() -> TestResult {
    crate::run_native_libsql_test(async {
        for settled in [false, true] {
            let (f, phase, compiled, page) = prepared().await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let encoder = OllamaModelProvider::for_projected_wire(
                OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                Arc::new(LocalOnlyOllamaEndpointPolicy),
            )?;
            let projection = encoder.project_wire_request(compiled.request())?;
            let budget = PhaseWireBudget::fit(projection, compiled.request().profile())?;
            let wire = EditPhaseWireCost::new(
                projection.digest(),
                projection.request_bytes(),
                budget.prompt_tokens(),
                budget.output_tokens(),
            )?;
            let read = f
                .http
                .store
                .load_source_read_state(&f.http.fixture.project, phase.anchor())
                .await?;
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
            if settled {
                let finished = at(permit.reserved_at().unix_millis() + 1)?;
                f.http
                    .store
                    .finish_edit_job(
                        &f.http.fixture.project,
                        permit,
                        &compiled,
                        EditPhaseObservation::new(
                            EditPhaseObservationOutcome::RejectedReply,
                            Some(128),
                            Some(32),
                            finished,
                        ),
                        None,
                    )
                    .await?;
            } else {
                let _unsettled = permit;
            }
            let charged = f
                .http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?;
            drop((phase, compiled, current, encoder, read));
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
            assert!(matches!(result, Err(EditNoChangeFailure::NoObservation)));
            assert_eq!(run, f.run);
            assert_eq!(StoredTaskLedger::new(ledger, version), f.stored);
            assert_eq!(
                f.http
                    .store
                    .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                    .await?,
                charged
            );
            assert_eq!(
                scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                0
            );
            assert_eq!(
                scalar(
                    &f,
                    "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
                )
                .await?,
                0
            );
            if settled {
                let db = f.http.fixture.candidate_database().await?;
                let connection = db.connection_for_operation().await?;
                // observe-once makes a late generic-rejected -> NoChange promotion impossible.
                assert!(
                    connection
                        .execute(
                            "UPDATE edit_phase_reservations SET terminal_reply_kind='no_change'",
                            ()
                        )
                        .await
                        .is_err()
                );
            } else {
                assert_eq!(charged.unobserved_calls, 1)
            }
            drop((current, phase));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn no_change_failure_after_abandon_rolls_back_the_entire_existing_cas() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, page) = unchanged().await?;
        let charged = f
            .http
            .store
            .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
            .await?;
        let events = f
            .http
            .store
            .load_run_events(
                &f.http.fixture.project,
                f.run.id(),
                None,
                RunEventPageLimit::new(256)?,
            )
            .await?;
        let db = f.http.fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        // Native private-fixture fault: failure occurs after the fact guard has set abandoned=1.
        connection
            .execute_batch(
                "CREATE TRIGGER no_change_fail_final_ledger BEFORE UPDATE ON task_ledgers
            BEGIN SELECT RAISE(ABORT,'injected private final Ledger CAS failure'); END;",
            )
            .await?;
        drop((connection, db));
        let current = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("index")?;
        let mut run = f.run.clone();
        let (mut ledger, version) = f.stored.clone().into_parts();
        assert!(
            CloseEditNoChange::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspacePatchAdapter::with_change_window_preview()
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
                &ActiveControl
            )
            .await
            .is_err()
        );
        assert_eq!(run, f.run);
        assert_eq!(StoredTaskLedger::new(ledger, version), f.stored);
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
            events.events()
        );
        assert_eq!(
            scalar(
                &f,
                "SELECT count(*) FROM source_read_acquisitions WHERE abandoned=1"
            )
            .await?,
            0
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            1
        );
        drop(current);
        cleanup(f)
    })
}

#[path = "plan22_edit_no_change_boundary_tests.rs"]
pub(in super::super) mod boundaries;

#[path = "plan22_closed_http_no_change_tests.rs"]
mod closed_origin;
