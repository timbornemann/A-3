//! Genuine same-Run Context provenance and existing controller contracts, not model quality.
use super::*;
use a3_application::{
    HttpCorrectionEditFailure, HttpCorrectionPlanApplyFailure, HttpCorrectionProducer,
};

async fn ready(null: bool, repair: bool) -> TestResult<ContextFixture> {
    let c = acquire_context().await?;
    let provider = controlled_provider(&c, null, repair)?;
    if repair {
        assert!(
            execute_context(&c, &provider, EditPhaseCallKind::Initial)
                .await
                .is_err()
        );
        execute_context(&c, &provider, EditPhaseCallKind::StructuralRepair).await?;
    } else {
        execute_context(&c, &provider, EditPhaseCallKind::Initial).await?;
    }
    drop(provider);
    Ok(c)
}

async fn apply(c: &mut ContextFixture) -> TestResult<HttpCorrectionPlanApplied> {
    let current =
        c.f.http
            .store
            .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("Context material publication missing")?;
    let (mut ledger, version) = c.f.stored.clone().into_parts();
    let now = edit::after_phases(&c.f).await?;
    let applied = ApplyHttpCorrectionPlan::new(
        c.f.http.store.as_ref(),
        c.f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    )
    .execute_context(
        &c.f.http.fixture.project,
        &c.f.http.goal,
        &mut c.f.run,
        &mut ledger,
        version,
        &current,
        &c.phase,
        now,
        &ActiveControl,
    )
    .await?;
    c.f.stored = StoredTaskLedger::new(ledger, applied.version);
    Ok(applied)
}

#[test]
fn http_context_applied_history_checks_selected_and_prior_context_output_limits_after_reopen()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (repair, selected) in [
            (false, "initial"),
            (true, "structural_repair"),
            (true, "initial"),
        ] {
            for output in [1025, 3605] {
                let mut c = ready(false, repair).await?;
                apply(&mut c).await?;
                let db = c.f.http.fixture.candidate_database().await?;
                let conn = db.connection_for_operation().await?;
                conn.execute("DROP TRIGGER http_context_intent_update_guard", ())
                    .await?;
                // Disposable native DB only. Actual encoded wire and charged reserve stay genuine.
                conn.execute("UPDATE http_context_intent_reservations SET reported_output=?1 WHERE call_kind=?2",
                    libsql::params![output,selected]).await?;
                drop((conn, db));
                let current =
                    c.f.http
                        .store
                        .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("index")?;
                assert!(matches!(
                    c.f.http
                        .store
                        .load_applied_http_correction_origin(
                            &c.f.http.fixture.project,
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
                drop(current);
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
                    .ok_or("cold index")?;
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
                            edit::after_phases(&f).await?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::InvalidStoredData)
                ));
                assert_eq!(
                    count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
                    1
                );
                assert_eq!(f.run.state(), AgentControllerState::Replan);
                drop(current);
                cleanup_open(f)?;
            }
        }
        Ok(())
    })
}

#[test]
fn http_context_applied_receipt_requires_exact_original_read_token_and_no_current_retirement_bypass()
-> TestResult {
    crate::run_native_libsql_test(async {
        for fault in 0..3 {
            let mut c = ready(false, false).await?;
            apply(&mut c).await?;
            let root = c.origin.parent_anchor.clone();
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            // Genuine applied Context works, while the generic Current receipt remains ineligible.
            let origin =
                c.f.http
                    .store
                    .load_applied_http_correction_origin(
                        &c.f.http.fixture.project,
                        &c.f.http.goal,
                        &c.f.stored,
                        &c.f.run,
                        &current,
                        &c.f.http.model,
                        edit::after_phases(&c.f).await?,
                    )
                    .await?;
            assert!(matches!(
                origin.producer,
                HttpCorrectionProducer::ContextV1Intent { .. }
            ));
            assert!(
                c.f.http
                    .store
                    .load_loopback_mismatch(
                        &c.f.http.fixture.project,
                        root.run,
                        root.tool,
                        root.tool_attempt
                    )
                    .await
                    .is_err()
            );
            let db = c.f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            if fault == 2 {
                conn.execute("DROP TRIGGER http_context_intent_update_guard", ())
                    .await?;
                let mut wrong = c.phase.anchor().clone();
                wrong.read_event = RunEventId::from_bytes([219; 32]);
                let bytes = crate::http_context_intent_repository::anchor_codec::encode(&wrong)?;
                conn.execute(
                    "UPDATE http_context_intent_reservations SET anchor=?1",
                    libsql::params![bytes],
                )
                .await?;
            } else {
                conn.execute("DROP TRIGGER source_read_identity_immutable", ())
                    .await?;
                if fault == 0 {
                    conn.execute(
                        "UPDATE source_read_acquisitions SET scope_digest=zeroblob(32)",
                        (),
                    )
                    .await?;
                } else {
                    conn.execute(
                        "UPDATE source_read_acquisitions SET after_sequence=after_sequence+1",
                        (),
                    )
                    .await?;
                }
            }
            drop((conn, db));
            assert!(
                c.f.http
                    .store
                    .load_applied_http_correction_origin(
                        &c.f.http.fixture.project,
                        &c.f.http.goal,
                        &c.f.stored,
                        &c.f.run,
                        &current,
                        &c.f.http.model,
                        edit::after_phases(&c.f).await?
                    )
                    .await
                    .is_err()
            );
            drop(current);
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
                .ok_or("cold index")?;
            let reader = ContextDenySource {
                calls: AtomicUsize::new(0),
                deny: 1,
            };
            let (mut ledger, mut version) = f.stored.clone().into_parts();
            let mut run = f.run.clone();
            assert!(
                ActivateHttpCorrectionEdit::new(
                    f.http.store.as_ref(),
                    f.http.store.as_ref(),
                    f.http.store.as_ref(),
                    &reader
                )
                .execute(
                    &f.http.fixture.project,
                    &f.http.goal,
                    &mut run,
                    &mut ledger,
                    &mut version,
                    &current,
                    &f.http.model,
                    edit::after_phases(&f).await?,
                    &ActiveControl
                )
                .await
                .is_err()
            );
            assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
            assert_eq!(run, f.run);
            assert_eq!(StoredTaskLedger::new(ledger, version), f.stored);
            assert_eq!(
                count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
                1
            );
            drop(current);
            cleanup_open(f)?;
        }
        Ok(())
    })
}

async fn direct_actual_wire(kind: EditPhaseCallKind) -> TestResult<CurrentPlanFixture> {
    let f = full_fixture(
        ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::SixteenK)?,
        true,
    )
    .await?;
    let focus = f.phase.anchor().source_range.start_position().row() + 1;
    let proposal = f
        .phase
        .decode_proposal(&reply(&f.phase, &format!("L{focus}"))?)?;
    let base = f.run.updated_at().unix_millis() + 1;
    let kinds = [
        Some(EditPhaseCallKind::Initial),
        (kind == EditPhaseCallKind::StructuralRepair).then_some(kind),
    ];
    for (i, call) in kinds.into_iter().flatten().enumerate() {
        let request = f.phase.request_for(call)?;
        let projection = encoder()?.project_wire_request(&request)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())?;
        assert_eq!(
            (
                projection.context_tokens(),
                projection.output_tokens(),
                budget.output_tokens()
            ),
            (16384, 1024, 3605)
        );
        let permit = f
            .http
            .store
            .reserve_http_correction_plan(
                &f.http.fixture.project,
                &f.phase,
                call,
                EditPhaseWireCost::new(
                    projection.digest(),
                    projection.request_bytes(),
                    budget.prompt_tokens(),
                    budget.output_tokens(),
                )?,
                at(base + u64::try_from(i)? * 2)?,
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
                    at(base + u64::try_from(i)? * 2 + 1)?,
                ),
                (!invalid).then_some(proposal.clone()),
            )
            .await?;
    }
    Ok(f)
}

#[test]
fn http_direct_applied_origin_full_profile_rejects_generation_overage_in_selected_and_prior_rows()
-> TestResult {
    crate::run_native_libsql_test(async {
        for (kind, selected) in [
            (EditPhaseCallKind::Initial, "initial"),
            (EditPhaseCallKind::StructuralRepair, "structural_repair"),
            (EditPhaseCallKind::StructuralRepair, "initial"),
        ] {
            let mut f = direct_actual_wire(kind).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
            let (mut ledger, version) = f.stored.clone().into_parts();
            let now = at(f.run.updated_at().unix_millis() + 10)?;
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
                now,
                &ActiveControl,
            )
            .await?;
            f.stored = StoredTaskLedger::new(ledger, applied.version);
            drop(current);
            let f = edit::cold(f).await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute("DROP TRIGGER http_correction_plan_update_guard", ())
                .await?;
            for output in [1024, 1025, 3605] {
                conn.execute("UPDATE http_correction_plan_reservations SET reported_output=?1 WHERE call_kind=?2",
                    libsql::params![output,selected]).await?;
                let current = f
                    .http
                    .store
                    .latest_published_index(&f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
                let result = f
                    .http
                    .store
                    .load_applied_http_correction_origin(
                        &f.http.fixture.project,
                        &f.http.goal,
                        &f.stored,
                        &f.run,
                        &current,
                        &f.http.model,
                        edit::after_phases(&f).await?,
                    )
                    .await;
                if output == 1024 {
                    assert!(matches!(
                        result?.producer,
                        HttpCorrectionProducer::DirectV4Focus { .. }
                    ));
                } else {
                    assert!(matches!(
                        result,
                        Err(EditPhaseReservationFailure::InvalidStoredData)
                    ));
                }
            }
            drop((conn, db));
            let f = edit::reopen(f).await?;
            let current = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold index")?;
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
                        edit::after_phases(&f).await?
                    )
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            ));
            assert_eq!(
                count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
                0
            );
            drop(current);
            cleanup_open(f)?;
        }
        Ok(())
    })
}

#[test]
fn http_context_apply_and_activation_revalidate_full_file_and_new_publication() -> TestResult {
    crate::run_native_libsql_test(async {
        let mut c = ready(false, false).await?;
        let path = c.f.http.root.join("server.py");
        let original = fs::read(&path)?;
        let a = c.phase.anchor().clone();
        let offsets = [
            usize::try_from(a.context_range.start_byte())?,
            usize::try_from(a.primary_range.start_byte())?,
            original.len() - 1,
        ];
        for offset in offsets {
            let mut changed = original.clone();
            changed[offset] = if changed[offset] == b' ' { b'\t' } else { b' ' };
            fs::write(&path, &changed)?;
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            let (mut ledger, version) = c.f.stored.clone().into_parts();
            let mut run = c.f.run.clone();
            assert!(matches!(
                ApplyHttpCorrectionPlan::new(
                    c.f.http.store.as_ref(),
                    c.f.http.store.as_ref(),
                    &WorkspaceAgentSourceReader
                )
                .execute_context(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                    &ActiveControl
                )
                .await,
                Err(HttpCorrectionPlanApplyFailure::SourceChanged)
            ));
            assert_eq!(run, c.f.run);
            assert_eq!(ledger, *c.f.stored.ledger());
            fs::write(&path, &original)?;
        }
        apply(&mut c).await?;
        for offset in offsets {
            let mut changed = original.clone();
            changed[offset] = if changed[offset] == b' ' { b'\t' } else { b' ' };
            fs::write(&path, &changed)?;
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            let (mut ledger, mut version) = c.f.stored.clone().into_parts();
            let mut run = c.f.run.clone();
            assert!(
                ActivateHttpCorrectionEdit::new(
                    c.f.http.store.as_ref(),
                    c.f.http.store.as_ref(),
                    c.f.http.store.as_ref(),
                    &WorkspaceAgentSourceReader
                )
                .execute(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    &mut version,
                    &current,
                    &c.f.http.model,
                    edit::after_phases(&c.f).await?,
                    &ActiveControl
                )
                .await
                .is_err()
            );
            assert_eq!(run, c.f.run);
            assert_eq!(StoredTaskLedger::new(ledger, version), c.f.stored);
            fs::write(&path, &original)?;
        }
        // Same script bytes in a newer publication cannot silently rebind the original index anchor.
        fs::write(c.f.http.root.join("note.txt"), "new publication\n")?;
        c.f.http
            .refresh
            .execute(
                &c.f.http.fixture.project,
                &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
                &mut c.f.http.compiler,
                &ActiveControl,
            )
            .await?;
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("new index")?;
        assert!(
            c.f.http
                .store
                .load_applied_http_correction_origin(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &c.f.stored,
                    &c.f.run,
                    &current,
                    &c.f.http.model,
                    edit::after_phases(&c.f).await?
                )
                .await
                .is_err()
        );
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM source_read_acquisitions").await?,
            1
        );
        drop(current);
        cleanup(c)
    })
}

#[test]
fn http_context_actual_ready_reaches_existing_edit_patch_full_http_and_cold_done() -> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let mut c = ready(false, false).await?;
                apply(&mut c).await?;
                let ContextFixture {
                    f,
                    phase,
                    origin,
                    primary,
                    context,
                } = c;
                drop((phase, origin, primary, context));
                let f = edit::reopen(f).await?;
                edit::complete_controlled_context_http(f).await
            })
    })
}

#[test]
fn http_context_codec_exact_existing_bytes_and_fixed_wire_projection_survive_replay() -> TestResult
{
    crate::run_native_libsql_test(async {
        let c = acquire_context().await?;
        let a = c.phase.anchor();
        let encoded = crate::http_context_intent_repository::anchor_codec::encode(a)?;
        assert_eq!(
            crate::http_context_intent_repository::anchor_codec::decode(&encoded, &c.f.http.goal)?,
            *a
        );
        for end in 0..encoded.len() {
            assert!(
                crate::http_context_intent_repository::anchor_codec::decode(
                    &encoded[..end],
                    &c.f.http.goal
                )
                .is_err(),
                "truncation{end}"
            );
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(
            crate::http_context_intent_repository::anchor_codec::decode(&trailing, &c.f.http.goal)
                .is_err()
        );
        for (offset, width, value) in [(0, 1, 2_u64), (1, 4, u64::from(u32::MAX)), (5, 1, 3)] {
            let mut bad = encoded.clone();
            bad[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
            assert!(
                crate::http_context_intent_repository::anchor_codec::decode(&bad, &c.f.http.goal)
                    .is_err()
            );
        }
        let parent_len = usize::try_from(u32::from_le_bytes(encoded[1..5].try_into()?))?;
        for offset in [5 + parent_len + 224, 5 + parent_len + 224 + 32] {
            let mut bad = encoded.clone();
            bad[offset..offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
            assert!(
                crate::http_context_intent_repository::anchor_codec::decode(&bad, &c.f.http.goal)
                    .is_err()
            );
        }
        let mut wrong = a.clone();
        wrong.context_range = wrong.primary_range;
        assert!(
            crate::http_context_intent_repository::anchor_codec::decode(
                &crate::http_context_intent_repository::anchor_codec::encode(&wrong)?,
                &c.f.http.goal
            )
            .is_err()
        );
        let mut wrong = a.clone();
        wrong.parent.goal = GoalContractReference::initial(TaskId::from_bytes([217; 32]));
        assert!(
            crate::http_context_intent_repository::anchor_codec::decode(
                &crate::http_context_intent_repository::anchor_codec::encode(&wrong)?,
                &c.f.http.goal
            )
            .is_err()
        );
        // Golden data comes from the immutable pre-refactor003 actual provider encoding;
        // only request content/schema/wire metadata is reused, never its expired owner/Run.
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/plan22-p7/http-context-wire-v1-golden.json"
        ))?;
        for (name, request) in [
            ("initial", c.phase.request_for(EditPhaseCallKind::Initial)?),
            (
                "repair",
                c.phase.request_for(EditPhaseCallKind::StructuralRepair)?,
            ),
            (
                "repair-truncated",
                c.phase
                    .for_truncated_output_repair()?
                    .request_for(EditPhaseCallKind::StructuralRepair)?,
            ),
        ] {
            let messages = request
                .messages()
                .iter()
                .map(|v| json!({"role":format!("{:?}",v.role()),"content":v.content()}))
                .collect::<Vec<_>>();
            let p = encoder()?.project_wire_request(&request)?;
            assert_eq!(json!(messages), golden[name]["messages"]);
            assert_eq!(
                request
                    .structured_output()
                    .ok_or("context schema missing")?
                    .value(),
                &golden[name]["format"]
            );
            assert_eq!(
                p.request_bytes(),
                golden[name]["actual_encoded_wire"]["request_bytes"]
                    .as_u64()
                    .ok_or("golden bytes")? as u32
            );
            assert_eq!(
                format!("{:?}", p.digest()),
                golden[name]["actual_encoded_wire"]["digest"]
                    .as_str()
                    .ok_or("golden digest")?
            );
            assert_eq!((p.context_tokens(), p.output_tokens()), (16384, 1024));
        }
        cleanup(c)
    })
}

#[test]
fn http_context_ready_material_and_cold_origin_preserve_f_r_and_current_admission_guard()
-> TestResult {
    crate::run_native_libsql_test(async {
        let mut c = ready(false, false).await?;
        let old_run = c.f.run.clone();
        let old_stored = c.f.stored.clone();
        let root = c.origin.parent_anchor.clone();
        let r = c.origin.read_event.sequence();
        let source_usage =
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?;
        let applied = apply(&mut c).await?;
        assert_eq!(c.f.run.last_event_sequence().get(), r.get() + 1);
        assert_eq!(c.f.run.state(), AgentControllerState::Replan);
        assert_eq!(c.f.run.created_at(), old_run.created_at());
        assert_eq!(c.f.run.budget(), old_run.budget());
        assert_eq!(c.f.run.usage(), old_run.usage());
        assert_eq!(
            c.f.stored.ledger().revision().get(),
            old_stored.ledger().revision().get() + 1
        );
        let failed =
            c.f.stored
                .ledger()
                .step(root.step)
                .ok_or("retired original missing")?;
        assert!(!failed.is_active_plan_step());
        assert_eq!(
            failed.attempts(),
            old_stored
                .ledger()
                .step(root.step)
                .ok_or("old failure")?
                .attempts()
        );
        assert_eq!(
            c.f.stored
                .ledger()
                .step(applied.proof)
                .ok_or("proof missing")?
                .status(),
            TaskStepStatus::Pending
        );
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
        assert!(
            c.f.http
                .store
                .load_http_context_read_origin(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &c.f.stored,
                    &c.f.run,
                    &current,
                    &c.f.http.model,
                    edit::after_phases(&c.f).await?
                )
                .await
                .is_err()
        );
        assert!(
            PreparedHttpContextIntent::new(
                &c.f.http.fixture.project,
                &c.f.http.goal,
                &c.f.stored,
                &c.f.run,
                &current,
                c.f.http.model.clone(),
                &c.origin,
                &c.primary,
                &c.context
            )
            .is_err()
        );
        assert!(
            c.f.http
                .store
                .load_loopback_mismatch(
                    &c.f.http.fixture.project,
                    root.run,
                    root.tool,
                    root.tool_attempt
                )
                .await
                .is_err()
        );
        let origin =
            c.f.http
                .store
                .load_applied_http_correction_origin(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &c.f.stored,
                    &c.f.run,
                    &current,
                    &c.f.http.model,
                    edit::after_phases(&c.f).await?,
                )
                .await?;
        assert_eq!(origin.anchor, root);
        assert_eq!(origin.producer.application_tail(&root)?, r);
        let HttpCorrectionProducer::ContextV1Intent {
            anchor, job, reply, ..
        } = origin.producer
        else {
            return Err("fake Direct origin".into());
        };
        assert_eq!(*anchor, *c.phase.anchor());
        assert_eq!(job, c.phase.job_digest());
        assert!(reply.intent().is_some());
        assert_eq!(
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?,
            source_usage
        );
        drop(current);
        let ContextFixture {
            f,
            phase,
            origin,
            primary,
            context,
        } = c;
        drop((phase, origin, primary, context));
        let mut f = edit::reopen(f).await?;
        let (phase, compiled, page, diagnostic) = edit::acquire(&mut f).await?;
        assert_eq!(f.run.last_event_sequence().get(), r.get() + 5);
        assert_eq!(page.text().len(), 370);
        assert_eq!(
            count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
            2
        );
        let published = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("target publication")?;
        let reads = f
            .http
            .store
            .load_source_read_state(
                &f.http.fixture.project,
                edit::state(&f, &published)?.anchor(),
            )
            .await?;
        assert!(reads.progress().receipts().is_empty()); // Context719 was not an Edit delivery.
        assert_eq!(
            reads
                .acquired()
                .ok_or("genuine Edit acquisition")?
                .original()
                .range(),
            page.range()
        );
        drop((phase, compiled, diagnostic, reads, published, page));
        let f = edit::reopen(f).await?;
        let published = f
            .http
            .store
            .latest_published_index(&f.http.fixture.project, &ActiveControl)
            .await?
            .ok_or("cold target index")?;
        let origin = f
            .http
            .store
            .load_applied_http_correction_origin(
                &f.http.fixture.project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &published,
                &f.http.model,
                edit::after_phases(&f).await?,
            )
            .await?;
        assert_eq!(origin.producer.application_tail(&root)?, r);
        let before_run = f.run.clone();
        let before_stored = f.stored.clone();
        let (mut ledger, mut version) = f.stored.clone().into_parts();
        let mut run = f.run.clone();
        ActivateHttpCorrectionEdit::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &WorkspaceAgentSourceReader,
        )
        .execute(
            &f.http.fixture.project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            &f.http.model,
            edit::after_phases(&f).await?,
            &ActiveControl,
        )
        .await?;
        assert_eq!(run, before_run);
        assert_eq!(StoredTaskLedger::new(ledger, version), before_stored);
        drop((published, origin));
        cleanup_open(f)
    })
}

#[test]
fn http_context_apply_null_missing_unknown_never_materialize_or_reread() -> TestResult {
    crate::run_native_libsql_test(async {
        for mode in 0..3 {
            let c = acquire_context().await?;
            if mode != 0 {
                let mut provider = controlled_provider(&c, mode == 1, false)?;
                if mode == 2 {
                    provider.reported_output_tokens = 1025;
                }
                let result = execute_context(&c, &provider, EditPhaseCallKind::Initial).await;
                assert_eq!(result.is_ok(), mode == 1);
                drop(provider);
            }
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            let reader = ContextDenySource {
                calls: AtomicUsize::new(0),
                deny: 1,
            };
            let mut run = c.f.run.clone();
            let (mut ledger, version) = c.f.stored.clone().into_parts();
            let result = ApplyHttpCorrectionPlan::new(
                c.f.http.store.as_ref(),
                c.f.http.store.as_ref(),
                &reader,
            )
            .execute_context(
                &c.f.http.fixture.project,
                &c.f.http.goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                &c.phase,
                edit::after_phases(&c.f).await?,
                &ActiveControl,
            )
            .await;
            assert!(matches!(
                result,
                Err(HttpCorrectionPlanApplyFailure::NoCorrectionIntent
                    | HttpCorrectionPlanApplyFailure::UnsettledOrStale)
            ));
            assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
            assert_eq!(run, c.f.run);
            assert_eq!(ledger, c.f.stored.ledger().clone());
            assert_metadata_only(&c).await?;
            drop(current);
            cleanup(c)?;
        }
        Ok(())
    })
}

#[derive(Debug)]
struct ContextDenySource {
    calls: AtomicUsize,
    deny: usize,
}

#[test]
fn http_context_apply_second_s1_read_cannot_backdate_original_deadline() -> TestResult {
    crate::run_native_libsql_test(async {
        let c = ready(false, false).await?;
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
        let charged =
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?;
        let events =
            c.f.http
                .store
                .load_run_events(
                    &c.f.http.fixture.project,
                    c.f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?,
                )
                .await?;
        let mutations = count(&c.f, "SELECT count(*) FROM mutation_attempts").await?;
        let reader = super::super::apply::DelayedHttpPlanSourceRead {
            reads: AtomicUsize::new(0),
            delay_on: 2,
            ranges: std::sync::Mutex::new([None, None]),
        };
        let mut run = c.f.run.clone();
        let (mut ledger, version) = c.f.stored.clone().into_parts();
        let last_millisecond =
            c.f.run.created_at().unix_millis() + c.f.run.budget().duration_limit().millis() - 1;
        let result =
            ApplyHttpCorrectionPlan::new(c.f.http.store.as_ref(), c.f.http.store.as_ref(), &reader)
                .execute_context(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &c.phase,
                    at(last_millisecond)?,
                    &ActiveControl,
                )
                .await;
        assert!(
            matches!(result, Err(HttpCorrectionPlanApplyFailure::BudgetExhausted)),
            "{result:?}"
        );
        assert_eq!(reader.reads.load(Ordering::SeqCst), 2);
        assert_eq!(
            *reader.ranges.lock().map_err(|_| "reader ranges poisoned")?,
            [
                Some(c.phase.anchor().primary_range),
                Some(c.phase.anchor().context_range)
            ]
        );
        assert_eq!(run, c.f.run);
        assert_eq!(StoredTaskLedger::new(ledger, version), c.f.stored);
        assert_eq!(
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?,
            charged
        );
        assert_eq!(
            c.f.http
                .store
                .load_run_events(
                    &c.f.http.fixture.project,
                    c.f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?
                )
                .await?
                .events(),
            events.events()
        );
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
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
            1
        );
        assert_metadata_only(&c).await?;
        drop(current);
        cleanup(c)
    })
}

#[derive(Debug)]
struct ContextApplyCancellation {
    cancelled: std::sync::atomic::AtomicBool,
}
impl AgentControllerControl for ContextApplyCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
impl AgentSourceReadControl for ContextApplyCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
struct CancelAfterSecondContextSource<'a> {
    reader: &'a super::super::apply::DelayedHttpPlanSourceRead,
    cancellation: &'a ContextApplyCancellation,
}
impl AgentSourceReader for CancelAfterSecondContextSource<'_> {
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            let page = self
                .reader
                .read_page(project, revision, request, control)
                .await?;
            if self.reader.reads.load(Ordering::SeqCst) == 2 {
                self.cancellation.cancelled.store(true, Ordering::SeqCst);
            }
            Ok(page)
        })
    }
}

#[test]
fn http_context_apply_cancellation_after_actual_second_original_keeps_state_and_charges()
-> TestResult {
    crate::run_native_libsql_test(async {
        let c = ready(false, false).await?;
        let current =
            c.f.http
                .store
                .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("index")?;
        let charged =
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?;
        let events =
            c.f.http
                .store
                .load_run_events(
                    &c.f.http.fixture.project,
                    c.f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?,
                )
                .await?;
        let mutations = count(&c.f, "SELECT count(*) FROM mutation_attempts").await?;
        let base = super::super::apply::DelayedHttpPlanSourceRead {
            reads: AtomicUsize::new(0),
            delay_on: usize::MAX,
            ranges: std::sync::Mutex::new([None, None]),
        };
        let cancellation = ContextApplyCancellation {
            cancelled: std::sync::atomic::AtomicBool::new(false),
        };
        let reader = CancelAfterSecondContextSource {
            reader: &base,
            cancellation: &cancellation,
        };
        let mut run = c.f.run.clone();
        let (mut ledger, version) = c.f.stored.clone().into_parts();
        let result =
            ApplyHttpCorrectionPlan::new(c.f.http.store.as_ref(), c.f.http.store.as_ref(), &reader)
                .execute_context(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &c.phase,
                    edit::after_phases(&c.f).await?,
                    &cancellation,
                )
                .await;
        // Source revalidation deliberately normalizes the read cancellation at this Apply boundary.
        assert!(
            matches!(result, Err(HttpCorrectionPlanApplyFailure::SourceChanged)),
            "{result:?}"
        );
        assert!(cancellation.cancelled.load(Ordering::SeqCst));
        assert_eq!(base.reads.load(Ordering::SeqCst), 2);
        assert_eq!(
            *base.ranges.lock().map_err(|_| "reader ranges poisoned")?,
            [
                Some(c.phase.anchor().primary_range),
                Some(c.phase.anchor().context_range)
            ]
        );
        assert_eq!(run, c.f.run);
        assert_eq!(StoredTaskLedger::new(ledger, version), c.f.stored);
        assert_eq!(
            c.f.http
                .store
                .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                .await?,
            charged
        );
        assert_eq!(
            c.f.http
                .store
                .load_run_events(
                    &c.f.http.fixture.project,
                    c.f.run.id(),
                    None,
                    RunEventPageLimit::new(256)?
                )
                .await?
                .events(),
            events.events()
        );
        assert_eq!(
            count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
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
            1
        );
        assert_metadata_only(&c).await?;
        drop(current);
        cleanup(c)
    })
}

#[test]
fn http_context_apply_final_cas_reject_and_actual_second_read_publication_race_are_atomic()
-> TestResult {
    crate::run_native_libsql_test(async {
        for publication in [
            None,
            Some(super::super::apply::SourceReadPublication::ChangedSnapshot),
            Some(super::super::apply::SourceReadPublication::SameSnapshot),
        ] {
            let publish = publication.is_some();
            let c = ready(false, false).await?;
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            let charged =
                c.f.http
                    .store
                    .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                    .await?;
            let events =
                c.f.http
                    .store
                    .load_run_events(
                        &c.f.http.fixture.project,
                        c.f.run.id(),
                        None,
                        RunEventPageLimit::new(256)?,
                    )
                    .await?;
            let mutations = count(&c.f, "SELECT count(*) FROM mutation_attempts").await?;
            let original = fs::read(c.f.http.root.join("server.py"))?;
            let read = super::super::apply::DelayedHttpPlanSourceRead {
                reads: AtomicUsize::new(0),
                delay_on: usize::MAX,
                ranges: std::sync::Mutex::new([None, None]),
            };
            let race = super::super::apply::PublishAfterSourceRead {
                fixture: &c.f.http.fixture,
                reads: AtomicUsize::new(0),
                publish_on: 2,
                publication: publication
                    .unwrap_or(super::super::apply::SourceReadPublication::ChangedSnapshot),
                ranges: std::sync::Mutex::new([None, None]),
            };
            let mut run = c.f.run.clone();
            let (mut ledger, version) = c.f.stored.clone().into_parts();
            let now = edit::after_phases(&c.f).await?;
            let result = if publish {
                ApplyHttpCorrectionPlan::new(
                    c.f.http.store.as_ref(),
                    c.f.http.store.as_ref(),
                    &race,
                )
                .execute_context(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &c.phase,
                    now,
                    &ActiveControl,
                )
                .await
            } else {
                ApplyHttpCorrectionPlan::new(
                    c.f.http.store.as_ref(),
                    &super::super::apply::RejectPlanCommit,
                    &read,
                )
                .execute_context(
                    &c.f.http.fixture.project,
                    &c.f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &c.phase,
                    now,
                    &ActiveControl,
                )
                .await
            };
            if publish {
                assert_eq!(race.reads.load(Ordering::SeqCst), 2);
                assert_eq!(
                    *race.ranges.lock().map_err(|_| "reader ranges poisoned")?,
                    [
                        Some(c.phase.anchor().primary_range),
                        Some(c.phase.anchor().context_range)
                    ]
                );
                let latest =
                    c.f.http
                        .store
                        .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                        .await?
                        .ok_or("new index")?;
                assert_ne!(
                    latest.run().id(),
                    current.run().id(),
                    "{publication:?}: {result:?}"
                );
                match publication {
                    Some(super::super::apply::SourceReadPublication::ChangedSnapshot) => {
                        assert_ne!(latest.run().snapshot_id(), current.run().snapshot_id());
                        assert!(c.f.http.root.join("source-race-marker.txt").is_file());
                    }
                    Some(super::super::apply::SourceReadPublication::SameSnapshot) => {
                        assert_ne!(
                            latest.run().ranking_policy_version(),
                            current.run().ranking_policy_version()
                        );
                        assert_eq!(latest.run().snapshot_id(), current.run().snapshot_id());
                        assert_eq!(inventory(&c.f.http.root)?, c.f.http.before);
                    }
                    None => return Err("missing publication case".into()),
                }
                assert!(
                    matches!(
                        result,
                        Err(HttpCorrectionPlanApplyFailure::Replan(
                            ApplyAgentPlanRevisionError::Storage(
                                AgentActionStoreFailure::SnapshotConflict
                            )
                        ))
                    ),
                    "a new published index must invalidate the old anchor: {publication:?}: {result:?}"
                );
            } else {
                assert!(
                    matches!(
                        result,
                        Err(HttpCorrectionPlanApplyFailure::Replan(
                            ApplyAgentPlanRevisionError::Storage(
                                AgentActionStoreFailure::LedgerVersionConflict
                            )
                        ))
                    ),
                    "{result:?}"
                );
                assert_eq!(read.reads.load(Ordering::SeqCst), 2);
                assert_eq!(
                    *read.ranges.lock().map_err(|_| "reader ranges poisoned")?,
                    [
                        Some(c.phase.anchor().primary_range),
                        Some(c.phase.anchor().context_range)
                    ]
                );
                assert_metadata_only(&c).await?;
            }
            assert_eq!(run, c.f.run);
            assert_eq!(StoredTaskLedger::new(ledger, version), c.f.stored);
            assert_eq!(
                c.f.http
                    .store
                    .load_agent_run(&c.f.http.fixture.project, run.id())
                    .await?,
                Some(c.f.run.clone())
            );
            assert_eq!(
                c.f.http
                    .store
                    .load_task_ledger(&c.f.http.fixture.project, c.f.http.goal.task_id())
                    .await?,
                Some(c.f.stored.clone())
            );
            assert_eq!(
                c.f.http
                    .store
                    .load_edit_phase_usage(&c.f.http.fixture.project, c.f.run.id())
                    .await?,
                charged
            );
            assert_eq!(
                c.f.http
                    .store
                    .load_run_events(
                        &c.f.http.fixture.project,
                        c.f.run.id(),
                        None,
                        RunEventPageLimit::new(256)?
                    )
                    .await?
                    .events(),
                events.events()
            );
            assert_eq!(
                count(&c.f, "SELECT count(*) FROM mutation_attempts").await?,
                mutations
            );
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
                1
            );
            assert_eq!(fs::read(c.f.http.root.join("server.py"))?, original);
            drop(current);
            cleanup(c)?;
        }
        Ok(())
    })
}
impl AgentSourceReader for ContextDenySource {
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.deny {
                return Err(AgentSourceReadFailure::Denied);
            }
            WorkspaceAgentSourceReader
                .read_page(project, revision, request, control)
                .await
        })
    }
}

#[test]
fn http_context_activation_cold_resumes_all_four_committed_prefixes_without_refunding() -> TestResult
{
    crate::run_native_libsql_test(async {
        for (cut, expected) in [
            (0, AgentControllerState::Replan),
            (1, AgentControllerState::Localize),
            (2, AgentControllerState::Plan),
            (3, AgentControllerState::Execute),
        ] {
            let mut c = ready(false, false).await?;
            apply(&mut c).await?;
            let r = c.phase.anchor().read_tail;
            let current =
                c.f.http
                    .store
                    .latest_published_index(&c.f.http.fixture.project, &ActiveControl)
                    .await?
                    .ok_or("index")?;
            let reader = ContextDenySource {
                calls: AtomicUsize::new(0),
                deny: 1 + 2 * cut,
            };
            let mut run = c.f.run.clone();
            let (mut ledger, mut version) = c.f.stored.clone().into_parts();
            let result = ActivateHttpCorrectionEdit::new(
                c.f.http.store.as_ref(),
                c.f.http.store.as_ref(),
                c.f.http.store.as_ref(),
                &reader,
            )
            .execute(
                &c.f.http.fixture.project,
                &c.f.http.goal,
                &mut run,
                &mut ledger,
                &mut version,
                &current,
                &c.f.http.model,
                edit::after_phases(&c.f).await?,
                &ActiveControl,
            )
            .await;
            assert!(matches!(
                result,
                Err(HttpCorrectionEditFailure::Source(
                    AgentSourceReadFailure::Denied
                ))
            ));
            assert_eq!(run.state(), expected);
            assert_eq!(
                run.last_event_sequence().get(),
                r.get() + 1 + u64::try_from(cut)?
            );
            assert_eq!(run.budget(), c.f.run.budget());
            assert_eq!(run.usage(), c.f.run.usage());
            c.f.run = run;
            c.f.stored = StoredTaskLedger::new(ledger, version);
            drop(current);
            let ContextFixture {
                f,
                phase,
                origin,
                primary,
                context,
            } = c;
            drop((phase, origin, primary, context));
            let mut f = edit::reopen(f).await?;
            let published = f
                .http
                .store
                .latest_published_index(&f.http.fixture.project, &ActiveControl)
                .await?
                .ok_or("cold index")?;
            let mut run = f.run.clone();
            let (mut ledger, mut version) = f.stored.clone().into_parts();
            ActivateHttpCorrectionEdit::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspaceAgentSourceReader,
            )
            .execute(
                &f.http.fixture.project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                &mut version,
                &published,
                &f.http.model,
                edit::after_phases(&f).await?,
                &ActiveControl,
            )
            .await?;
            assert_eq!(run.state(), AgentControllerState::Execute);
            assert_eq!(run.last_event_sequence().get(), r.get() + 4);
            assert_eq!(
                ledger
                    .steps()
                    .filter(|v| v.is_active_plan_step())
                    .flat_map(|v| v.attempts())
                    .count(),
                1
            );
            f.run = run;
            f.stored = StoredTaskLedger::new(ledger, version);
            assert_eq!(
                count(&f, "SELECT count(*) FROM source_read_acquisitions").await?,
                1
            );
            drop(published);
            cleanup_open(f)?;
        }
        Ok(())
    })
}

// Existing real Context-fixture path reused by the closed-origin native contract.
pub(in super::super) async fn applied_context_for_closed_no_change(
    repair: bool,
) -> TestResult<edit::HttpEditFixture> {
    let mut c = ready(false, repair).await?;
    apply(&mut c).await?;
    let ContextFixture {
        f,
        phase,
        origin,
        primary,
        context,
    } = c;
    drop((phase, origin, primary, context));
    edit::reopen(f).await
}
