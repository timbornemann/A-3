//! Direct application-loader accounting boundaries; controlled retained debits are not model runs.
use super::*;
#[path = "plan22_http_correction_activation_boundary_tests.rs"]
mod activation;

#[allow(clippy::too_many_arguments)]
async fn retained_execute_debit(
    connection: &libsql::Connection,
    phase: &PreparedHttpCorrectionPlan,
    run: &AgentRun,
    seed: u8,
    call_kind: &str,
    prompt: u32,
    output: u32,
    outcome: Option<&str>,
    finished: u64,
) -> TestResult {
    let a = phase.anchor();
    // Same prior-Execute debit seam as the existing shared-accounting contract. The actual
    // public Execute/Plan admission paths remain unchanged and are independently tested.
    let job = hash(&[seed, 0]);
    let wire = hash(&[seed, u8::from(call_kind == "structural_repair")]);
    connection.execute(
        "INSERT INTO edit_phase_reservations(worktree_id,run_id,job_digest,call_kind,task_id,goal_revision,ledger_revision,ledger_store_version,run_sequence,step_id,attempt_number,verification_id,snapshot_id,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,?11,?12,?13,100,?14,?15,79,?16)",
        libsql::params![
            a.worktree.as_bytes().to_vec(), a.run.as_bytes().to_vec(), job.as_bytes().to_vec(), call_kind,
            a.goal.task_id().as_bytes().to_vec(), i64::from(a.goal.revision().get()),
            i64::from(a.ledger.get()), i64::try_from(a.version.get())?, i64::try_from(a.sequence.get())? - 2,
            a.step.as_bytes().to_vec(), a.verification.as_bytes().to_vec(), a.snapshot.as_bytes().to_vec(),
            wire.as_bytes().to_vec(), i64::from(prompt), i64::from(output),
            i64::try_from(run.created_at().unix_millis().checked_add(run.budget().duration_limit().millis()).ok_or("fixture deadline overflow")?)?,
        ],
    ).await?;
    if let Some(outcome) = outcome {
        connection.execute(
            "UPDATE edit_phase_reservations SET outcome=?1,finished_at=?2,reported_prompt=64,reported_output=16
             WHERE run_id=?3 AND job_digest=?4 AND call_kind=?5",
            libsql::params![outcome, i64::try_from(finished)?, a.run.as_bytes().to_vec(), job.as_bytes().to_vec(), call_kind],
        ).await?;
    }
    Ok(())
}

#[test]
fn http_correction_apply_loader_directly_rechecks_shared_debits_and_settlement_time() -> TestResult
{
    crate::run_native_libsql_test(async {
        for case in [
            "unobserved",
            "unknown",
            "clock",
            "prompt",
            "output",
            "turns",
            "repairs",
            "known",
        ] {
            let f = current_fixture().await?;
            settle_focus(&f, "L6").await?;
            let project = &f.http.fixture.project;
            let db = f.http.fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            match case {
                "turns" => {
                    // The Plan call is already charged: these retained calls make total limit+1.
                    for n in 0..f.run.budget().turn_limit().get() {
                        retained_execute_debit(
                            &connection,
                            &f.phase,
                            &f.run,
                            u8::try_from(n)?,
                            "initial",
                            100,
                            50,
                            Some("accepted_reply"),
                            81,
                        )
                        .await?;
                    }
                }
                "repairs" => {
                    for n in 0..=f.run.budget().repair_limit().get() {
                        let seed = u8::try_from(n)?;
                        retained_execute_debit(
                            &connection,
                            &f.phase,
                            &f.run,
                            seed,
                            "initial",
                            100,
                            50,
                            Some("invalid_structure"),
                            80,
                        )
                        .await?;
                        retained_execute_debit(
                            &connection,
                            &f.phase,
                            &f.run,
                            seed,
                            "structural_repair",
                            100,
                            50,
                            Some("accepted_reply"),
                            81,
                        )
                        .await?;
                    }
                }
                _ => {
                    retained_execute_debit(
                        &connection,
                        &f.phase,
                        &f.run,
                        91,
                        "initial",
                        if case == "prompt" {
                            u32::try_from(f.run.budget().prompt_token_limit().get() + 1)?
                        } else {
                            100
                        },
                        if case == "output" {
                            u32::try_from(f.run.budget().output_token_limit().get() + 1)?
                        } else {
                            50
                        },
                        if case == "unobserved" {
                            None
                        } else if case == "unknown" {
                            Some("unknown")
                        } else {
                            Some("accepted_reply")
                        },
                        if case == "clock" { 84 } else { 81 },
                    )
                    .await?;
                }
            }
            let ordinary = f
                .http
                .store
                .load_http_correction_plan(project, &f.phase)
                .await?;
            assert!(
                ordinary.is_some(),
                "the immutable current proposal itself remains valid: {case}"
            );
            let charged = f
                .http
                .store
                .load_edit_phase_usage(project, f.run.id())
                .await?;
            let check = f
                .http
                .store
                .load_http_correction_plan_for_application(project, &f.phase, at(83)?)
                .await;
            match case {
                "unobserved" | "unknown" => assert!(
                    matches!(
                        check,
                        Err(EditPhaseReservationFailure::UncertainProviderCall)
                    ),
                    "{case}"
                ),
                "clock" => assert!(matches!(
                    check,
                    Err(EditPhaseReservationFailure::TimestampRegressed)
                )),
                "prompt" => assert!(matches!(
                    check,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::PromptTokens
                    ))
                )),
                "output" => assert!(matches!(
                    check,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::OutputTokens
                    ))
                )),
                "turns" => assert!(matches!(
                    check,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::Turns
                    ))
                )),
                "repairs" => assert!(matches!(
                    check,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::Repairs
                    ))
                )),
                "known" => assert_eq!(check?, ordinary),
                _ => return Err("unexpected direct-loader test case".into()),
            }
            if case == "clock" {
                // Equality with the latest actual stored finish is allowed, without a new debit.
                assert_eq!(
                    f.http
                        .store
                        .load_http_correction_plan_for_application(project, &f.phase, at(84)?)
                        .await?,
                    ordinary
                );
            }
            assert!(matches!(
                f.http
                    .store
                    .load_http_correction_plan_for_application(
                        project,
                        &f.phase,
                        at(f.run
                            .created_at()
                            .unix_millis()
                            .checked_add(f.run.budget().duration_limit().millis())
                            .ok_or("fixture deadline overflow")?)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::DeadlineReached)
            ));
            assert_eq!(
                f.http
                    .store
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(f.stored.clone())
            );
            assert_eq!(
                f.http.store.load_agent_run(project, f.run.id()).await?,
                Some(f.run.clone())
            );
            assert_eq!(
                f.http
                    .store
                    .load_edit_phase_usage(project, f.run.id())
                    .await?,
                charged
            );
            drop((
                connection,
                db,
                f.http.compiler,
                f.http.refresh,
                f.http.store,
            ));
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            assert_eq!(
                reopened.load_edit_phase_usage(project, f.run.id()).await?,
                charged
            );
            if ["unknown", "unobserved"].contains(&case) {
                assert!(matches!(
                    reopened
                        .load_http_correction_plan_for_application(project, &f.phase, at(85)?)
                        .await,
                    Err(EditPhaseReservationFailure::UncertainProviderCall)
                ));
            }
            assert_eq!(
                reopened
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(f.stored.clone())
            );
            assert_eq!(
                reopened.load_agent_run(project, f.run.id()).await?,
                Some(f.run.clone())
            );
            drop(reopened);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

fn graph_with_unsupported_prerequisite(
    ledger: &TaskLedger,
    failed_id: TaskStepId,
    parent: bool,
) -> TestResult<TaskLedger> {
    let old = ledger.step(failed_id).ok_or("failed source step missing")?;
    let definition = old.definition();
    let prerequisite_id = TaskStepId::from_bytes([170; 32]);
    let changed = TaskStepDefinition::new(
        definition.id(),
        if parent { Some(prerequisite_id) } else { None },
        definition.intended_outcome().clone(),
        definition.rationale().clone(),
        if parent {
            Vec::new()
        } else {
            vec![StepDependency::new(prerequisite_id)]
        },
        definition.expected_evidence().to_vec(),
        definition.verification_spec().clone(),
    )?
    .with_criterion_coverage(
        definition.addressed_criteria().to_vec(),
        definition.acceptance_criteria().to_vec(),
    )?;
    let replacement = TaskStep::reconstruct(
        changed,
        old.introduced_in_revision(),
        TaskStepMaterializedState::new(
            old.status(),
            old.blocking_reason().cloned(),
            old.stale_cause().cloned(),
            old.retired_in_revision(),
        ),
        old.attempts().to_vec(),
    )?;
    let prerequisite = TaskStepDefinition::new(
        prerequisite_id,
        None,
        TaskStepOutcome::try_from_string("Prior source prerequisite".to_owned())?,
        TaskStepRationale::try_from_string("Preserve graph prerequisites".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Bound source difference".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([171; 32]),
            VerificationRequirement::try_from_string(
                "Preserve the source prerequisite".to_owned(),
            )?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(b"server.py".to_vec())?],
            )?,
        ),
    )?
    .with_criterion_coverage(definition.acceptance_criteria().to_vec(), Vec::new())?;
    let prerequisite = TaskStep::reconstruct(
        prerequisite,
        TaskLedgerRevision::INITIAL,
        TaskStepMaterializedState::new(TaskStepStatus::Ready, None, None, None),
        Vec::new(),
    )?;
    let mut steps = ledger
        .steps()
        .filter(|step| step.definition().id() != failed_id)
        .cloned()
        .collect::<Vec<_>>();
    steps.extend([replacement, prerequisite]);
    Ok(TaskLedger::reconstruct(
        ledger.goal_contract(),
        ledger.revision(),
        steps,
        ledger.replans().to_vec(),
        ledger.created_at(),
        ledger.updated_at(),
    )?)
}

#[test]
fn http_correction_apply_rejects_valid_but_unsupported_in_memory_graph_and_foreign_project()
-> TestResult {
    crate::run_native_libsql_test(async {
        for parent in [false, true] {
            let f = current_fixture().await?;
            settle_focus(&f, "L6").await?;
            let project = &f.http.fixture.project;
            let current = f
                .http
                .store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("index missing")?;
            let mut ledger = graph_with_unsupported_prerequisite(
                f.stored.ledger(),
                f.phase.anchor().step,
                parent,
            )?;
            let supplied = ledger.clone();
            let mut run = f.run.clone();
            let result = ApplyHttpCorrectionPlan::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspaceAgentSourceReader,
            )
            .execute(
                project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                f.stored.version(),
                &current,
                &f.phase,
                at(83)?,
                &ActiveControl,
            )
            .await;
            assert!(matches!(
                result,
                Err(HttpCorrectionPlanApplyFailure::UnsupportedGraph)
            ));
            assert_eq!(ledger, supplied);
            assert_eq!(run, f.run);
            // These valid in-memory alternatives are intentionally never persisted or treated as current history.
            assert_eq!(
                f.http
                    .store
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(f.stored.clone())
            );
            let foreign = Fixture::new()?;
            assert_ne!(foreign.project.worktree().id(), project.worktree().id());
            let (mut ordinary, version) = f.stored.clone().into_parts();
            assert!(matches!(
                ApplyHttpCorrectionPlan::new(
                    f.http.store.as_ref(),
                    f.http.store.as_ref(),
                    &WorkspaceAgentSourceReader
                )
                .execute(
                    &foreign.project,
                    &f.http.goal,
                    &mut run,
                    &mut ordinary,
                    version,
                    &current,
                    &f.phase,
                    at(83)?,
                    &ActiveControl
                )
                .await,
                Err(HttpCorrectionPlanApplyFailure::Binding)
            ));
            assert_eq!(run, f.run);
            assert_eq!(&ordinary, f.stored.ledger());
            foreign.cleanup()?;
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}
