//! Genuine closed A provenance only. No B dispatch/materialization or model calls.
use super::boundaries::{Retained, actual_wire, prepared_16k, prove_unchanged};
use super::*;
use a3_application::{
    ClosedHttpNoChangeOrigin, HttpCorrectionProducer, replay_closed_no_change_edit_job,
};

#[derive(Clone, Copy, Debug)]
enum History {
    Direct,
    DirectEditRepair,
    Context,
    ContextRepair,
}

async fn context_unchanged(repair: bool) -> TestResult<(HttpEditFixture, AgentSourcePage)> {
    let mut f=super::super::super::context_intent::context_application::applied_context_for_closed_no_change(repair).await?;
    let (phase, mut compiled, page, _) = acquire(&mut f).await?;
    let current = f
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
        reply: json!({"code":page.text()}).to_string(),
        calls: AtomicUsize::new(0),
    };
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
            state(&f, &current)?,
            after_phases(&f).await?,
            &WorkspacePatchAdapter::with_change_window_preview(),
            &ActiveControl,
        ))
        .await?,
        state(&f, &current)?,
        &compiled,
        after_phases(&f).await?,
        ModelRequestTimeout::from_millis(120000)?,
        &ActiveControl,
    )
    .await;
    assert!(
        matches!(
            result,
            Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
        ),
        "actual Context NoChange: {result:?}"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    drop((provider, compiled, current));
    Ok((f, page))
}
async fn repair_unchanged() -> TestResult<(HttpEditFixture, AgentSourcePage)> {
    let (f, phase, compiled, page) = prepared_16k().await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let reads = f
        .http
        .store
        .load_source_read_state(&f.http.fixture.project, phase.anchor())
        .await?;
    assert!(reads.acquired().is_some());
    let initial = f
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
    let finished = at(initial.reserved_at().unix_millis() + 1)?;
    f.http
        .store
        .finish_edit_job(
            &f.http.fixture.project,
            initial,
            &compiled,
            EditPhaseObservation::new(
                EditPhaseObservationOutcome::InvalidStructure,
                Some(128),
                Some(32),
                finished,
            ),
            None,
        )
        .await?;
    let repair = compiled.for_structural_repair()?;
    let permit = f
        .http
        .store
        .reserve_edit_job(
            &f.http.fixture.project,
            &phase,
            &repair,
            actual_wire(&repair)?,
            after_phases(&f).await?,
        )
        .await?;
    let window = prove_unchanged(&f, &phase, &page, &current).await?;
    let finished = at(permit.reserved_at().unix_millis() + 1)?;
    f.http
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
        .await?;
    drop((phase, compiled, repair, reads, current));
    Ok((f, page))
}
async fn closed(
    history: History,
) -> TestResult<(HttpEditFixture, AgentSourcePage, PreparedEditPhase)> {
    // Bound the nested fixture state on the ordinary native test stack. Keep
    // each finite setup future separately owned; no stack or timeout is raised.
    let (f, page) = match history {
        History::Direct => Box::pin(unchanged()).await?,
        History::DirectEditRepair => Box::pin(repair_unchanged()).await?,
        History::Context => Box::pin(context_unchanged(false)).await?,
        History::ContextRepair => Box::pin(context_unchanged(true)).await?,
    };
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    // This real live phase is retained BEFORE closure solely to test the old
    // loader's unchanged denial afterwards; no historical Execute is fabricated.
    let old_live = PreparedEditPhase::update(state(&f, &current)?, &page)?;
    let mut run = f.run.clone();
    let (mut ledger, version) = f.stored.clone().into_parts();
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
    let mut f = f;
    f.run = run;
    f.stored = StoredTaskLedger::new(ledger, next);
    drop(current);
    Ok((reopen(f).await?, page, old_live))
}
async fn load(
    f: &HttpEditFixture,
    current: &PublishedIndex,
) -> Result<Option<ClosedHttpNoChangeOrigin>, EditPhaseReservationFailure> {
    let now = after_phases(f)
        .await
        .map_err(|_| EditPhaseReservationFailure::Unavailable)?;
    f.http
        .store
        .load_closed_no_change_http_origin(
            &f.http.fixture.project,
            &f.http.goal,
            &f.stored,
            &f.run,
            current,
            &f.http.model,
            now,
        )
        .await
}
fn inspection(origin: &ClosedHttpNoChangeOrigin) -> TestResult<AgentFileInspection> {
    let original = origin.fact().window().original();
    let range = original.range();
    let lines = range
        .end_position()
        .row()
        .checked_add(u32::from(range.end_position().column() > 0))
        .and_then(|end| end.checked_sub(range.start_position().row()))
        .ok_or("line envelope")?;
    Ok(AgentFileInspection::new(
        original.revision().path().clone(),
        AgentFileStartLine::new(range.start_position().row() + 1)?,
        AgentFileLineCount::new(u16::try_from(lines)?)?,
    ))
}
async fn assert_read_only(f: &HttpEditFixture, before: &Retained) -> TestResult {
    before
        .assert_same(f, &f.run, f.stored.ledger(), f.stored.version())
        .await
}

#[test]
fn closed_no_change_direct_and_real_context_cold_origins_preserve_all_old_live_guards() -> TestResult
{
    crate::run_native_libsql_test(async {
        for history in [
            History::Direct,
            History::DirectEditRepair,
            History::Context,
            History::ContextRepair,
        ] {
            let (f, page, old_live) = closed(history).await?;
            let before = Retained::capture(&f).await?;
            let inventory_before = inventory(&f.http.root)?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let origin = load(&f, &current).await?.ok_or("authentic closed origin")?;
            let p = origin
                .parent()
                .producer
                .application_tail(&origin.parent().anchor)?;
            assert_eq!(origin.fact().owner().sequence.get(), p.get() + 5);
            assert_eq!(origin.closure().sequence().get(), p.get() + 6);
            assert_eq!(
                origin.fact().owner().version.get() + 1,
                f.stored.version().get()
            );
            assert_eq!(origin.fact().window().original().range(), page.range());
            assert_eq!(
                origin.fact().call().deadline().unix_millis(),
                f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis()
            );
            assert_eq!(
                replay_closed_no_change_edit_job(origin.fact(), &page)?,
                old_live.job_digest()
            );
            assert_eq!(origin.fact().job_digest(), old_live.job_digest());
            match history {
                History::Direct | History::DirectEditRepair => assert!(matches!(
                    origin.parent().producer,
                    HttpCorrectionProducer::DirectV4Focus { .. }
                )),
                History::Context | History::ContextRepair => assert!(matches!(
                    origin.parent().producer,
                    HttpCorrectionProducer::ContextV1Intent { .. }
                )),
            }
            let actual = WorkspaceAgentSourceReader
                .read_page(
                    &f.http.fixture.project,
                    page.revision(),
                    &inspection(&origin)?,
                    &ActiveControl,
                )
                .await?;
            assert_eq!(
                replay_closed_no_change_edit_job(origin.fact(), &actual)?,
                origin.fact().job_digest()
            );
            assert!(matches!(
                f.http
                    .store
                    .load_edit_no_change(
                        &f.http.fixture.project,
                        &old_live,
                        &f.http.model,
                        &current,
                        after_phases(&f).await?
                    )
                    .await,
                Err(EditPhaseReservationFailure::StaleAnchor)
            ));
            assert!(matches!(
                f.http
                    .store
                    .load_source_read_state(&f.http.fixture.project, old_live.anchor())
                    .await,
                Err(a3_application::SourceReadFailure::Reservation(
                    EditPhaseReservationFailure::StaleAnchor
                ))
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
            assert_read_only(&f, &before).await?;
            assert_eq!(inventory(&f.http.root)?, inventory_before);
            drop((origin, current, old_live));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn closed_no_change_sql_fact_requires_actual_closed_row_owner_profile_original_and_closure()
-> TestResult {
    crate::run_native_libsql_test(async {
        for fault in [
            "missing-receipt",
            "missing-tag",
            "profile",
            "same-ref-limits",
            "goal",
            "range",
            "original",
            "scope",
            "unabandoned",
            "closure-wire",
            "missing-closure",
            "closure-id",
            "owner-version",
            "owner-sequence",
            "verification",
            "failed-step",
            "output-cap",
            "original-coordinates",
        ] {
            let (f, _, old_live) = closed(History::Direct).await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            match fault {
                "missing-receipt" => {
                    conn.execute("DROP TRIGGER edit_no_change_no_delete", ())
                        .await?;
                    assert_eq!(
                        conn.execute("DELETE FROM edit_phase_no_change_receipts", ())
                            .await?,
                        1
                    );
                }
                "missing-tag" => {
                    conn.execute("DROP TRIGGER edit_no_change_tag_finish_once", ())
                        .await?;
                    conn.execute("DROP TRIGGER edit_phase_observe_once", ())
                        .await?;
                    assert_eq!(
                        conn.execute(
                            "UPDATE edit_phase_reservations SET terminal_reply_kind=NULL",
                            ()
                        )
                        .await?,
                        1
                    );
                }
                "original" | "scope" | "unabandoned" => {
                    conn.execute("DROP TRIGGER source_read_identity_immutable", ())
                        .await?;
                    let sql = match fault {
                        "original" => {
                            "UPDATE source_read_acquisitions SET original_evidence_id=NULL"
                        }
                        "scope" => "UPDATE source_read_acquisitions SET scope_digest=zeroblob(32)",
                        _ => "UPDATE source_read_acquisitions SET abandoned=0",
                    };
                    assert_eq!(conn.execute(sql, ()).await?, 1);
                }
                "missing-closure" => {
                    assert_eq!(conn.execute(
                        "DELETE FROM run_events WHERE event_sequence=(SELECT MAX(event_sequence) FROM run_events)", ()
                    ).await?, 1);
                }
                "closure-id" => {
                    assert_eq!(conn.execute(
                        "UPDATE run_events SET event_id=zeroblob(32) WHERE event_sequence=(SELECT MAX(event_sequence) FROM run_events)", ()
                    ).await?, 1);
                }
                "owner-version" | "owner-sequence" | "verification" | "failed-step"
                | "output-cap" => {
                    conn.execute("DROP TRIGGER edit_phase_observe_once", ())
                        .await?;
                    if fault == "failed-step" {
                        assert_eq!(
                            conn.execute(
                                "UPDATE edit_phase_reservations SET step_id=?1",
                                libsql::params![f.http.step.as_bytes().to_vec()]
                            )
                            .await?,
                            1
                        );
                    } else {
                        let sql = match fault {
                            "owner-version" => {
                                "UPDATE edit_phase_reservations SET ledger_store_version=ledger_store_version+1"
                            }
                            "owner-sequence" => {
                                "UPDATE edit_phase_reservations SET run_sequence=run_sequence+1"
                            }
                            "verification" => {
                                "UPDATE edit_phase_reservations SET verification_id=zeroblob(32)"
                            }
                            _ => "UPDATE edit_phase_reservations SET reported_output=1025",
                        };
                        assert_eq!(conn.execute(sql, ()).await?, 1);
                    }
                }
                "closure-wire" => {
                    conn.execute("DROP TRIGGER edit_phase_observe_once", ())
                        .await?;
                    assert_eq!(
                        conn.execute(
                            "UPDATE edit_phase_reservations SET wire_digest=zeroblob(32)",
                            ()
                        )
                        .await?,
                        1
                    );
                }
                _ => {
                    conn.execute("DROP TRIGGER edit_no_change_no_update", ())
                        .await?;
                    let sql = match fault {
                        "profile" => {
                            "UPDATE edit_phase_no_change_receipts SET profile_id=zeroblob(32)"
                        }
                        "same-ref-limits" => {
                            "UPDATE edit_phase_no_change_receipts SET context_limit=context_limit+1"
                        }
                        "goal" => {
                            "UPDATE edit_phase_no_change_receipts SET original_goal_digest=zeroblob(32)"
                        }
                        "range" => {
                            "UPDATE edit_phase_no_change_receipts SET window_start=window_start+1"
                        }
                        "original-coordinates" => {
                            "UPDATE edit_phase_no_change_receipts SET original_range=zeroblob(24)"
                        }
                        _ => return Err("unknown private closed fact fault".into()),
                    };
                    assert_eq!(conn.execute(sql, ()).await?, 1);
                }
            }
            drop((conn, db));
            let f = reopen(f).await?;
            let before = Retained::capture(&f).await?;
            let files = inventory(&f.http.root)?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let result = load(&f, &current).await;
            assert!(
                matches!(
                    result,
                    Err(EditPhaseReservationFailure::StaleAnchor
                        | EditPhaseReservationFailure::InvalidStoredData)
                ),
                "closed fault {fault}: {result:?}"
            );
            assert_read_only(&f, &before).await?;
            assert_eq!(inventory(&f.http.root)?, files);
            drop((current, old_live));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn closed_no_change_parent_and_prior_edit_history_generation_caps_have_no_fallback() -> TestResult {
    crate::run_native_libsql_test(async {
        for fault in ["prior-edit-cap", "context-cap", "context-null"] {
            let history = if fault == "prior-edit-cap" {
                History::DirectEditRepair
            } else {
                History::ContextRepair
            };
            let (f, _, old_live) = closed(history).await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            if fault == "prior-edit-cap" {
                conn.execute("DROP TRIGGER edit_phase_observe_once", ())
                    .await?;
                assert_eq!(conn.execute("UPDATE edit_phase_reservations SET reported_output=1025 WHERE call_kind='initial'",()).await?,1);
            } else {
                conn.execute("DROP TRIGGER http_context_intent_update_guard", ())
                    .await?;
                if fault == "context-cap" {
                    assert_eq!(conn.execute("UPDATE http_context_intent_reservations SET reported_output=1025 WHERE call_kind='initial'",()).await?,1);
                } else {
                    let bytes = b"{\"intent\":null}";
                    assert_eq!(conn.execute("UPDATE http_context_intent_reservations SET reply=?1,reply_digest=?2 WHERE call_kind='structural_repair'",
                        libsql::params![bytes.to_vec(),blake3::hash(bytes).as_bytes().to_vec()]).await?,1);
                }
            }
            drop((conn, db));
            let f = reopen(f).await?;
            let before = Retained::capture(&f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let result = load(&f, &current).await;
            assert!(
                matches!(
                    result,
                    Err(EditPhaseReservationFailure::StaleAnchor
                        | EditPhaseReservationFailure::InvalidStoredData)
                ),
                "{fault}: {result:?}"
            );
            assert_read_only(&f, &before).await?;
            drop((current, old_live));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn closed_no_change_current_byte_replay_and_exact_publication_freshness_are_separate_from_dto_shape()
-> TestResult {
    crate::run_native_libsql_test(async {
        for fault in [
            "digest",
            "physical-source",
            "same-snapshot-index",
            "new-snapshot",
            "deadline",
        ] {
            let (f, page, old_live) = closed(History::Direct).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let before = Retained::capture(&f).await?;
            let files = inventory(&f.http.root)?;
            let mut changed_files = None;
            if fault == "digest" {
                let db = f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                conn.execute("DROP TRIGGER edit_no_change_no_update", ())
                    .await?;
                conn.execute(
                    "UPDATE edit_phase_no_change_receipts SET window_digest=zeroblob(32)",
                    (),
                )
                .await?;
                drop((conn, db));
                // SQL proves recorded metadata; only actual current source replay
                // can reject an internally valid but corrupt byte digest.
                let origin = load(&f, &current).await?.ok_or("recorded descriptor")?;
                let actual = WorkspaceAgentSourceReader
                    .read_page(
                        &f.http.fixture.project,
                        page.revision(),
                        &inspection(&origin)?,
                        &ActiveControl,
                    )
                    .await?;
                assert!(matches!(
                    replay_closed_no_change_edit_job(origin.fact(), &actual),
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
            } else if fault == "physical-source" {
                let original = fs::read(f.http.root.join("server.py"))?;
                let mut changed = original.clone();
                changed.extend_from_slice(b"\n# private freshness race\n");
                fs::write(f.http.root.join("server.py"), changed)?;
                let origin = load(&f, &current)
                    .await?
                    .ok_or("recorded history descriptor")?;
                assert!(matches!(
                    WorkspaceAgentSourceReader
                        .read_page(
                            &f.http.fixture.project,
                            page.revision(),
                            &inspection(&origin)?,
                            &ActiveControl
                        )
                        .await,
                    Err(AgentSourceReadFailure::Stale)
                ));
                changed_files = Some(original);
            } else if fault == "same-snapshot-index" {
                super::super::super::apply::republish_unchanged_index(&f.http.fixture).await?;
                let latest = f
                    .http
                    .store
                    .latest_published_index(&f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("republished")?;
                assert_ne!(latest.run().id(), current.run().id());
                assert_eq!(latest.run().snapshot_id(), current.run().snapshot_id());
                assert!(matches!(
                    load(&f, &latest).await,
                    Err(EditPhaseReservationFailure::StaleAnchor)
                ));
            } else if fault == "new-snapshot" {
                fs::write(
                    f.http.root.join("closed-origin-marker.txt"),
                    b"changed actual index\n",
                )?;
                let (store, refresh, compiler, latest) = index_fixture(&f.http.fixture).await?;
                assert_ne!(latest.run().snapshot_id(), current.run().snapshot_id());
                assert!(matches!(
                    load(&f, &latest).await,
                    Err(EditPhaseReservationFailure::StaleAnchor)
                ));
                drop((store, refresh, compiler, latest));
                fs::remove_file(f.http.root.join("closed-origin-marker.txt"))?;
            } else {
                let deadline = at(
                    f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis()
                )?;
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
                            deadline
                        )
                        .await,
                    Err(EditPhaseReservationFailure::DeadlineReached)
                ));
            }
            assert_read_only(&f, &before).await?;
            if let Some(original) = changed_files {
                fs::write(f.http.root.join("server.py"), original)?;
            }
            assert_eq!(inventory(&f.http.root)?, files);
            drop((current, old_live));
            cleanup(f)?;
        }
        Ok(())
    })
}

#[path = "plan22_http_no_change_v5_tests.rs"]
mod v5;
