//! Actual isolated store and Edit executor controls; no Ollama I/O or model-quality claim.
use super::*;
use a3_application::*;
use a3_domain::*;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use std::sync::atomic::{AtomicUsize, Ordering};

fn model() -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("native-edit-need-control".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(16384)?,
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
async fn fixture() -> TestResult<ActiveEdit> {
    ActiveEdit::start_with_model(false, true, &model()?).await
}
fn compiled(f: &ActiveEdit, phase: &PreparedEditPhase) -> TestResult<CompiledEditPhaseRequest> {
    Ok(build_edit_phase_request(
        &f.goal,
        &f.ledger,
        phase,
        EditPhaseState::new(&f.fixture.project, &f.goal, &f.ledger, &f.run, &f.published)?,
        model()?,
        None,
        EditPhasePrerequisites::none(),
    )?)
}
fn wide_wire(byte: u8) -> TestResult<EditPhaseWireCost> {
    Ok(EditPhaseWireCost::new(
        ContentHash::from_bytes([byte; 32]),
        1200,
        2400,
        3605,
    )?)
}
async fn scalar(f: &ActiveEdit, sql: &str) -> TestResult<i64> {
    let db = f.fixture.candidate_database().await?;
    let c = db.connection_for_operation().await?;
    let mut rows = c.query(sql, ()).await?;
    Ok(rows.next().await?.ok_or("scalar")?.get(0)?)
}
fn encoder() -> TestResult<OllamaModelProvider> {
    Ok(OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?)
}
struct NativeNeed<'a> {
    f: &'a ActiveEdit,
    encoder: OllamaModelProvider,
    reply: String,
    calls: AtomicUsize,
}
impl std::fmt::Debug for NativeNeed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeNeed").finish_non_exhaustive()
    }
}
impl ModelProvider for NativeNeed<'_> {
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
            let u = self
                .f
                .store
                .load_edit_phase_usage(&self.f.fixture.project, self.f.run.id())
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if u.unobserved_calls != 1
                || u.unknown_calls != 0
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
                    ModelProviderUsage::new(Some(128), Some(6)),
                ))),
            ])) as ProviderEventStream<'a>)
        })
    }
}

#[test]
fn edit_need_boundary_all_closed_requests_and_size_survive_real_finish_and_cold_reopen_without_repair_or_patch()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for (reply, expected) in [
                (
                    r#"{"need":"definition"}"#,
                    EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Definition),
                ),
                (
                    r#"{"need":"caller"}"#,
                    EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Caller),
                ),
                (
                    r#"{"need":"test"}"#,
                    EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test),
                ),
                (
                    r#"{"need":"unsupported_private_name"}"#,
                    EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Unknown),
                ),
                (r#"{"tooLarge":true}"#, EditPhaseHalt::TooLarge),
            ] {
                let f = fixture().await?;
                let phase = f.phase(&f.path)?;
                let mut request = compiled(&f, &phase)?;
                let p = NativeNeed {
                    f: &f,
                    encoder: encoder()?,
                    reply: reply.to_owned(),
                    calls: AtomicUsize::new(0),
                };
                let state = EditPhaseState::new(
                    &f.fixture.project,
                    &f.goal,
                    &f.ledger,
                    &f.run,
                    &f.published,
                )?;
                let report = ExecuteEditPhase::new(
                    &p,
                    f.store.as_ref(),
                    &WorkspacePatchAdapter::with_change_window_preview(),
                )
                .execute(
                    Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                        phase,
                        &mut request,
                        state,
                        at(8)?,
                        &WorkspacePatchAdapter::with_change_window_preview(),
                        &ActiveControl,
                    ))
                    .await?,
                    state,
                    &request,
                    at(8)?,
                    ModelRequestTimeout::from_millis(120000)?,
                    &ActiveControl,
                )
                .await?;
                assert!(report.source_delivery.is_none());
                assert_eq!(p.calls.load(Ordering::SeqCst), 1);
                let actual = match report.outcome {
                    EditPhaseOutcome::NeedEvidence(n) => EditPhaseHalt::NeedEvidence(n),
                    EditPhaseOutcome::TooLarge => EditPhaseHalt::TooLarge,
                    _ => return Err("halt became proposal".into()),
                };
                assert_eq!(actual, expected);
                let charged = f.usage().await?;
                assert_eq!(
                    (
                        charged.calls,
                        charged.repairs,
                        charged.unobserved_calls,
                        charged.unknown_calls
                    ),
                    (1, 0, 0, 0)
                );
                drop(p);
                let f = f.reopen().await?;
                let receipt = f
                    .store
                    .load_edit_halt(&f.fixture.project, &request)
                    .await?
                    .ok_or("cold halt missing")?;
                assert_eq!(receipt.disposition(), expected);
                assert_eq!(receipt.request_digest(), request.request_digest());
                assert_eq!(f.usage().await?, charged);
                let phase = f.phase(&f.path)?;
                assert!(matches!(
                    f.store
                        .reserve_edit_job(
                            &f.fixture.project,
                            &phase,
                            &request,
                            wide_wire(2)?,
                            at(receipt.finished_at().unix_millis() + 1)?
                        )
                        .await,
                    Err(Failure::AlreadyReserved)
                ));
                let repair = request.for_structural_repair()?;
                assert!(matches!(
                    f.store
                        .reserve_edit_job(
                            &f.fixture.project,
                            &phase,
                            &repair,
                            wide_wire(3)?,
                            at(receipt.finished_at().unix_millis() + 1)?
                        )
                        .await,
                    Err(Failure::RepairNotAvailable)
                ));
                assert_eq!(
                    scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                    0
                );
                assert_eq!(
                    scalar(&f, "SELECT count(*) FROM mutation_attempts").await?,
                    0
                );
                assert!(
                    !f.fixture
                        .project
                        .worktree()
                        .root()
                        .as_path()
                        .join("subject.py")
                        .exists()
                );
                f.assert_run_and_ledger_unchanged().await?;
                f.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn edit_need_boundary_actual_generation_cap_rejects_1025_below_3605_reserve_without_refund()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = fixture().await?;
            let phase = f.phase(&f.path)?;
            let request = compiled(&f, &phase)?;
            let permit = f
                .store
                .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                .await?;
            assert_eq!(
                f.store
                    .finish_edit_job(
                        &f.fixture.project,
                        permit,
                        &request,
                        EditPhaseObservation::new(
                            Outcome::AcceptedReply,
                            Some(128),
                            Some(1025),
                            at(9)?
                        ),
                        Some(EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test))
                    )
                    .await,
                Err(Failure::BudgetExhausted(AgentBudgetDimension::OutputTokens))
            );
            let f = f.reopen().await?;
            assert!(
                f.store
                    .load_edit_halt(&f.fixture.project, &request)
                    .await?
                    .is_none()
            );
            let u = f.usage().await?;
            assert_eq!(
                (
                    u.calls,
                    u.unknown_calls,
                    u.unobserved_calls,
                    u.output_tokens
                ),
                (1, 1, 0, 3605)
            );
            let phase = f.phase(&f.path)?;
            let repair = request.for_structural_repair()?;
            assert!(matches!(
                f.store
                    .reserve_edit_job(&f.fixture.project, &phase, &repair, wide_wire(2)?, at(10)?)
                    .await,
                Err(Failure::UncertainProviderCall)
            ));
            assert_eq!(
                scalar(
                    &f,
                    "SELECT count(*) FROM edit_phase_jobs WHERE halt_kind IS NOT NULL"
                )
                .await?,
                0
            );
            f.cleanup()
        })
    })
}

#[test]
fn edit_need_boundary_forged_permit_and_wrong_request_roll_back_both_disposition_and_finish()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = fixture().await?;
            let phase = f.phase(&f.path)?;
            let request = compiled(&f, &phase)?;
            let permit = f
                .store
                .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                .await?;
            let deadline = permit.deadline();
            let repair = request.for_structural_repair()?;
            assert_eq!(
                f.store
                    .finish_edit_job(
                        &f.fixture.project,
                        permit,
                        &repair,
                        observation(Outcome::AcceptedReply, 9)?,
                        Some(EditPhaseHalt::TooLarge)
                    )
                    .await,
                Err(Failure::ObservationConflict)
            );
            let forged =
                EditPhaseReservation::issue(&phase, Initial, wide_wire(2)?, at(8)?, deadline);
            assert_eq!(
                f.store
                    .finish_edit_job(
                        &f.fixture.project,
                        forged,
                        &request,
                        observation(Outcome::AcceptedReply, 9)?,
                        Some(EditPhaseHalt::TooLarge)
                    )
                    .await,
                Err(Failure::ObservationConflict)
            );
            assert_eq!(f.usage().await?.unobserved_calls, 1);
            assert_eq!(
                scalar(
                    &f,
                    "SELECT count(*) FROM edit_phase_jobs WHERE halt_kind IS NOT NULL"
                )
                .await?,
                0
            );
            let permit =
                EditPhaseReservation::issue(&phase, Initial, wide_wire(1)?, at(8)?, deadline);
            // Generic observation cannot bypass the v3 typed contract.
            assert_eq!(
                f.store
                    .finish_edit_phase(
                        &f.fixture.project,
                        permit,
                        observation(Outcome::AcceptedReply, 9)?
                    )
                    .await,
                Err(Failure::ObservationConflict)
            );
            let permit =
                EditPhaseReservation::issue(&phase, Initial, wide_wire(1)?, at(8)?, deadline);
            f.store
                .finish_edit_job(
                    &f.fixture.project,
                    permit,
                    &request,
                    observation(Outcome::AcceptedReply, 9)?,
                    Some(EditPhaseHalt::TooLarge),
                )
                .await?;
            let f = f.reopen().await?;
            assert_eq!(
                f.store
                    .load_edit_halt(&f.fixture.project, &request)
                    .await?
                    .ok_or("halt")?
                    .disposition(),
                EditPhaseHalt::TooLarge
            );
            f.cleanup()
        })
    })
}

#[test]
fn edit_need_boundary_missing_usage_cancelled_and_original_deadline_never_retain_a_need()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for case in 0..3 {
                let f = fixture().await?;
                let phase = f.phase(&f.path)?;
                let request = compiled(&f, &phase)?;
                let permit = f
                    .store
                    .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                    .await?;
                let (observation, halt) = match case {
                    0 => (
                        EditPhaseObservation::new(Outcome::AcceptedReply, Some(128), None, at(9)?),
                        Some(EditPhaseHalt::TooLarge),
                    ),
                    1 => (
                        EditPhaseObservation::new(Outcome::Cancelled, Some(128), Some(6), at(9)?),
                        None,
                    ),
                    _ => (
                        EditPhaseObservation::new(
                            Outcome::AcceptedReply,
                            Some(128),
                            Some(6),
                            permit.deadline(),
                        ),
                        Some(EditPhaseHalt::TooLarge),
                    ),
                };
                let result = f
                    .store
                    .finish_edit_job(&f.fixture.project, permit, &request, observation, halt)
                    .await;
                match case {
                    0 => assert_eq!(result, Err(Failure::IncompleteProviderUsage)),
                    1 => result?,
                    _ => assert_eq!(result, Err(Failure::DeadlineReached)),
                }
                let f = f.reopen().await?;
                assert!(
                    f.store
                        .load_edit_halt(&f.fixture.project, &request)
                        .await?
                        .is_none()
                );
                assert_eq!(f.usage().await?.calls, 1);
                assert_eq!(f.usage().await?.output_tokens, 3605);
                assert_eq!(
                    scalar(
                        &f,
                        "SELECT count(*) FROM edit_phase_jobs WHERE halt_kind IS NOT NULL"
                    )
                    .await?,
                    0
                );
                f.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn edit_need_boundary_ledger_cas_change_settles_stale_without_installing_disposition() -> TestResult
{
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = fixture().await?;
            let phase = f.phase(&f.path)?;
            let request = compiled(&f, &phase)?;
            let permit = f
                .store
                .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                .await?;
            f.store
                .replace_task_ledger(&f.fixture.project, f.ledger.version(), f.ledger.ledger())
                .await?;
            assert_eq!(
                f.store
                    .finish_edit_job(
                        &f.fixture.project,
                        permit,
                        &request,
                        observation(Outcome::AcceptedReply, 9)?,
                        Some(EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Caller))
                    )
                    .await,
                Err(Failure::StaleAnchor)
            );
            assert_eq!(f.usage().await?.unobserved_calls, 0);
            assert_eq!(f.usage().await?.calls, 1);
            assert_eq!(
                scalar(
                    &f,
                    "SELECT count(*) FROM edit_phase_jobs WHERE halt_kind IS NOT NULL"
                )
                .await?,
                0
            );
            f.cleanup()
        })
    })
}

#[test]
fn edit_need_boundary_sql_closed_kind_and_append_once_no_refund_guards_are_real() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = fixture().await?;
            let phase = f.phase(&f.path)?;
            let request = compiled(&f, &phase)?;
            let permit = f
                .store
                .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                .await?;
            let db = f.fixture.candidate_database().await?;
            let c = db.connection_for_operation().await?;
            assert!(
                c.execute("UPDATE edit_phase_jobs SET halt_kind='free command'", ())
                    .await
                    .is_err()
            );
            assert!(c.execute("DELETE FROM edit_phase_jobs", ()).await.is_err());
            assert!(
                c.execute("UPDATE edit_phase_jobs SET generation_cap=3605", ())
                    .await
                    .is_err()
            );
            f.store
                .finish_edit_job(
                    &f.fixture.project,
                    permit,
                    &request,
                    observation(Outcome::AcceptedReply, 9)?,
                    Some(EditPhaseHalt::TooLarge),
                )
                .await?;
            assert!(
                c.execute("UPDATE edit_phase_jobs SET halt_kind='test'", ())
                    .await
                    .is_err()
            );
            assert!(
                c.execute("DELETE FROM edit_phase_reservations", ())
                    .await
                    .is_err()
            );
            drop((c, db));
            let f = f.reopen().await?;
            assert_eq!(f.usage().await?.calls, 1);
            f.cleanup()
        })
    })
}

#[test]
fn edit_need_boundary_complete_ollama_wire_before_after_preserves_profile_and_reserve() -> TestResult
{
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = ActiveEdit::start_with_model(true, true, &model()?).await?;
            let phase = f.phase(&f.path)?;
            let current = compiled(&f, &phase)?;
            let provider = encoder()?;
            // Declared historical v2 transformation for the SAME fixture/profile/bytes.
            // Source006's v2 code/need(definition,caller,test)/tooLarge grammar is reproduced
            // only here for measurement, never as a second production encoder.
            let branch = |key: &str, shape: serde_json::Value| json!({"type":"object","properties":{(key):shape},"required":[key],"additionalProperties":false});
            let old_schema = StructuredOutputSchema::new(
                json!({"oneOf":[branch("code",json!({"type":"string"})),branch("need",json!({"enum":["definition","caller","test"]})),branch("tooLarge",json!({"const":true}))]}),
            )?;
            for (kind, next) in [
                ("initial", compiled(&f, &phase)?),
                ("repair", current.for_structural_repair()?),
            ] {
                let messages = next
                    .request()
                    .messages()
                    .iter()
                    .map(|m| {
                        ModelMessage::try_from_string(
                            m.role(),
                            m.content()
                                .replace("Edit v3", "Edit v2")
                                .replace("\"version\":3", "\"version\":2")
                                .replace("Else: tooLarge.", "Else: need/tooLarge.")
                                .lines()
                                .filter(|line| !line.starts_with("Core verification:"))
                                .collect::<Vec<_>>()
                                .join("\n")
                                + if m.content().ends_with('\n') {
                                    "\n"
                                } else {
                                    ""
                                },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let old = ModelProviderRequest::new(
                    next.request().profile().clone(),
                    messages,
                    Some(old_schema.clone()),
                )?;
                let before = provider.project_wire_request(&old)?;
                let after = provider.project_wire_request(next.request())?;
                let b = PhaseWireBudget::fit(before, old.profile())?;
                let a = PhaseWireBudget::fit(after, next.request().profile())?;
                assert_eq!(
                    (before.context_tokens(), before.output_tokens()),
                    (after.context_tokens(), after.output_tokens())
                );
                assert_eq!(b.output_tokens(), a.output_tokens());
                assert_eq!(a.output_tokens(), 3605);
                eprintln!(
                    "edit_need_wire kind={kind} before_bytes={} after_bytes={} before_prompt={} after_prompt={} reserve={} generation=1024 context=16384",
                    before.request_bytes(),
                    after.request_bytes(),
                    b.prompt_tokens(),
                    a.prompt_tokens(),
                    a.output_tokens()
                );
            }
            f.cleanup()
        })
    })
}

#[test]
fn edit_need_boundary_cold_loader_rejects_over_cap_costs_and_request_corruption() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let f = fixture().await?;
            let phase = f.phase(&f.path)?;
            let request = compiled(&f, &phase)?;
            for reserve in [1, 1024, 3604, 3606] {
                let wire =
                    EditPhaseWireCost::new(ContentHash::from_bytes([4; 32]), 1200, 2400, reserve)?;
                assert!(matches!(
                    f.store
                        .reserve_edit_job(&f.fixture.project, &phase, &request, wire, at(8)?)
                        .await,
                    Err(Failure::InvalidWireCost)
                ));
                assert_eq!(f.usage().await?, EditPhaseReservedUsage::default());
            }
            f.cleanup()?;
            for corruption in 0..3 {
                let f = fixture().await?;
                let phase = f.phase(&f.path)?;
                let request = compiled(&f, &phase)?;
                let permit = f
                    .store
                    .reserve_edit_job(&f.fixture.project, &phase, &request, wide_wire(1)?, at(8)?)
                    .await?;
                let observed = observation(Outcome::AcceptedReply, 9)?;
                f.store
                    .finish_edit_job(
                        &f.fixture.project,
                        permit,
                        &request,
                        observed,
                        Some(EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test)),
                    )
                    .await?;
                assert_eq!(f.usage().await?.output_tokens, 3605);
                let db = f.fixture.candidate_database().await?;
                let c = db.connection_for_operation().await?;
                if corruption == 0 {
                    // Declared damaged-store fixture, not an application update path.
                    c.execute_batch("DROP TRIGGER edit_phase_observe_once; UPDATE edit_phase_reservations SET reported_output=1025").await?;
                } else if corruption == 1 {
                    c.execute_batch("DROP TRIGGER edit_job_observe_once; UPDATE edit_phase_jobs SET request_digest=zeroblob(32)").await?;
                } else {
                    c.execute_batch("DROP TRIGGER edit_phase_observe_once; UPDATE edit_phase_reservations SET output_reserve=1").await?;
                }
                drop((c, db));
                let f = f.reopen().await?;
                assert!(
                    f.store
                        .load_edit_halt(&f.fixture.project, &request)
                        .await
                        .is_err()
                );
                assert_eq!(f.usage().await?.calls, 1);
                // The raw damaged-store fixture alters the accounting row itself. Coldload
                // rejects it; legal API no-refund behavior is covered by the other groups.
                let damaged_sum = if corruption == 2 {
                    u64::from(observed.reported_output_tokens().ok_or("fixture usage")?)
                } else {
                    3605
                };
                assert_eq!(f.usage().await?.output_tokens, damaged_sum);
                f.cleanup()?;
            }
            Ok(())
        })
    })
}
