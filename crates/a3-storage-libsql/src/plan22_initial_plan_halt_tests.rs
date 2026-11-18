//! Real-store model halts retain positive usage without materializing executable work.

use super::*;
use a3_application::PlanPhaseHalt;

fn halted(reason: PlanPhaseHalt) -> InitialPlanFinish {
    InitialPlanFinish::Halted {
        reason,
        prompt_tokens: 370,
        output_tokens: 55,
    }
}

async fn reopen(fixture: Promoted) -> TestResult<Promoted> {
    let Promoted {
        data,
        store,
        published,
        model,
    } = fixture;
    drop(store);
    let store = Arc::new(data.open().await?);
    Ok(Promoted {
        data,
        store,
        published,
        model,
    })
}

#[test]
fn refuse_and_clarify_reopen_distinctly_charged_without_ledger_run_or_repair() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for reason in [PlanPhaseHalt::Refuse, PlanPhaseHalt::Clarify] {
                let fixture = Promoted::new(3).await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    None
                );
                let phase = fixture.phase(fixture.data.step.id())?;
                let permit = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                    .await?;
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    None
                );
                assert_eq!(
                    fixture
                        .store
                        .finish_initial_plan(project, permit, halted(reason), at(7)?)
                        .await?,
                    None
                );
                let fixture = reopen(fixture).await?;
                let project = &fixture.data.fixture.project;
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    Some(reason)
                );
                let usage = fixture.store.load_initial_plan_usage(project, task).await?;
                assert_eq!(
                    (
                        usage.calls,
                        usage.repairs,
                        usage.unobserved_calls,
                        usage.unknown_calls
                    ),
                    (1, 0, 0, 0)
                );
                assert_eq!((usage.prompt_tokens, usage.output_tokens), (400, 200));
                assert!(
                    fixture
                        .store
                        .load_task_ledger(project, task)
                        .await?
                        .is_none()
                );
                assert_eq!(
                    fixture
                        .store
                        .load_current_goal_contract(project, task)
                        .await?,
                    Some(fixture.data.goal.clone())
                );
                let db = fixture.data.fixture.candidate_database().await?;
                let connection = db.connection_for_operation().await?;
                assert_eq!(
                    count(&connection, "SELECT COUNT(*) FROM agent_runs").await?,
                    0
                );
                assert_eq!(
                    count(&connection, "SELECT COUNT(*) FROM initial_plan_phase_halts").await?,
                    1
                );
                let phase = fixture.phase(fixture.data.step.id())?;
                for (kind, expected) in [
                    (INITIAL, AccountingError::AlreadyReserved),
                    (REPAIR, AccountingError::RepairNotAvailable),
                ] {
                    assert!(matches!(
                        fixture.store.reserve_initial_plan(project, &phase, kind, wire(11)?, at(8)?).await,
                        Err(PlanFailure::Accounting(error)) if error == expected
                    ));
                }
                drop((connection, db));
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn halt_child_failure_rolls_back_settlement_but_retains_unknown_dispatch_charge() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = Promoted::new(3).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                .await?;
            let db = fixture.data.fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            connection
                .execute_batch(
                    "CREATE TRIGGER abort_semantic_halt BEFORE INSERT ON initial_plan_phase_halts
                 BEGIN SELECT RAISE(ABORT,'fixture halt failure'); END;",
                )
                .await?;
            assert_eq!(
                fixture
                    .store
                    .finish_initial_plan(project, permit, halted(PlanPhaseHalt::Clarify), at(7)?)
                    .await,
                Err(PlanFailure::Accounting(AccountingError::Unavailable))
            );
            assert_eq!(
                count(&connection, "SELECT COUNT(*) FROM initial_plan_phase_halts").await?,
                0
            );
            assert_eq!(count(&connection, "SELECT COUNT(*) FROM initial_plan_phase_reservations WHERE outcome IS NULL AND has_semantic_halt=0").await?, 1);
            drop((connection, db));
            let fixture = reopen(fixture).await?;
            let project = &fixture.data.fixture.project;
            assert_eq!(
                fixture.store.load_initial_plan_halt(project, task).await?,
                None
            );
            assert!(
                fixture
                    .store
                    .load_task_ledger(project, task)
                    .await?
                    .is_none()
            );
            let usage = fixture.store.load_initial_plan_usage(project, task).await?;
            assert_eq!(
                (usage.calls, usage.unobserved_calls, usage.prompt_tokens),
                (1, 1, 400)
            );
            let phase = fixture.phase(fixture.data.step.id())?;
            assert!(matches!(
                fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(11)?, at(8)?)
                    .await,
                Err(PlanFailure::Accounting(
                    AccountingError::UncertainProviderCall
                ))
            ));
            fixture.cleanup()
        })
    })
}

#[test]
fn corrupted_halt_presence_or_reason_fails_closed_after_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for corruption in 0..4 {
                let fixture = Promoted::new(3).await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                let phase = fixture.phase(fixture.data.step.id())?;
                let permit = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                    .await?;
                fixture
                    .store
                    .finish_initial_plan(project, permit, halted(PlanPhaseHalt::Refuse), at(7)?)
                    .await?;
                let db = fixture.data.fixture.candidate_database().await?;
                let connection = db.connection_for_operation().await?;
                let sql = match corruption {
                    0 => "DROP TRIGGER initial_plan_halt_no_delete; DELETE FROM initial_plan_phase_halts;",
                    1 => "DROP TRIGGER initial_plan_observe_once; UPDATE initial_plan_phase_reservations SET has_semantic_halt=0;",
                    2 => "DROP TRIGGER initial_plan_halt_no_update; PRAGMA ignore_check_constraints=ON;
                          UPDATE initial_plan_phase_halts SET reason=replace(hex(zeroblob(65536)),'0','x');
                          PRAGMA ignore_check_constraints=OFF;",
                    3 => "DROP TRIGGER initial_plan_observe_once;
                          UPDATE initial_plan_phase_reservations SET reported_prompt=401;",
                    _ => return Err("unknown fixed halt corruption case".into()),
                };
                connection.execute_batch(sql).await?;
                drop((connection, db));
                let fixture = reopen(fixture).await?;
                let project = &fixture.data.fixture.project;
                if corruption == 2 {
                    // This case deliberately violates SQL CHECK. Cold integrity
                    // validation stops before the bounded semantic-halt decoder.
                    assert_eq!(
                        fixture.store.load_initial_plan_halt(project, task).await,
                        Err(PlanFailure::Accounting(AccountingError::Unavailable))
                    );
                    assert_eq!(
                        fixture.store.load_task_ledger(project, task).await,
                        Err(a3_application::TaskLedgerStoreFailure::Corrupt)
                    );
                    assert_eq!(
                        fixture.store.load_initial_plan_usage(project, task).await,
                        Err(PlanFailure::Accounting(AccountingError::Unavailable))
                    );
                    fixture.cleanup()?;
                    continue;
                }
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await,
                    Err(PlanFailure::Accounting(AccountingError::InvalidStoredData))
                );
                assert!(
                    fixture
                        .store
                        .load_task_ledger(project, task)
                        .await?
                        .is_none()
                );
                assert_eq!(
                    fixture
                        .store
                        .load_initial_plan_usage(project, task)
                        .await?
                        .calls,
                    1
                );
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn invalid_usage_or_deadline_cannot_publish_a_successful_semantic_halt() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for (prompt_tokens, output_tokens, finished, expected) in [
                (0, 55, 7, AccountingError::IncompleteProviderUsage),
                (
                    401,
                    55,
                    7,
                    AccountingError::BudgetExhausted(AgentBudgetDimension::PromptTokens),
                ),
                (370, 55, 51, AccountingError::DeadlineReached),
            ] {
                let fixture = Promoted::new(3).await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                let phase = fixture.phase(fixture.data.step.id())?;
                let permit = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                    .await?;
                assert_eq!(
                    fixture
                        .store
                        .finish_initial_plan(
                            project,
                            permit,
                            InitialPlanFinish::Halted {
                                reason: PlanPhaseHalt::Clarify,
                                prompt_tokens,
                                output_tokens,
                            },
                            at(finished)?
                        )
                        .await,
                    Err(PlanFailure::Accounting(expected))
                );
                let fixture = reopen(fixture).await?;
                let project = &fixture.data.fixture.project;
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    None
                );
                assert!(
                    fixture
                        .store
                        .load_task_ledger(project, task)
                        .await?
                        .is_none()
                );
                let usage = fixture.store.load_initial_plan_usage(project, task).await?;
                assert_eq!((usage.calls, usage.unobserved_calls), (1, 0));
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn latest_repair_disposition_is_used_without_resurrecting_an_old_halt() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for halt in [true, false] {
                let fixture = Promoted::new(3).await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                let phase = fixture.phase(fixture.data.step.id())?;
                let initial = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                    .await?;
                fixture
                    .store
                    .finish_initial_plan(
                        project,
                        initial,
                        InitialPlanFinish::ObservedFailure {
                            kind: InitialPlanRejection::InvalidStructure,
                            reported_prompt_tokens: Some(370),
                            reported_output_tokens: Some(55),
                        },
                        at(7)?,
                    )
                    .await?;
                let repair = fixture
                    .store
                    .reserve_initial_plan(project, &phase, REPAIR, wire(11)?, at(8)?)
                    .await?;
                let finish = if halt {
                    halted(PlanPhaseHalt::Clarify)
                } else {
                    accepted(fixture.admitted(&phase)?)
                };
                fixture
                    .store
                    .finish_initial_plan(project, repair, finish, at(9)?)
                    .await?;
                let fixture = reopen(fixture).await?;
                let project = &fixture.data.fixture.project;
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    halt.then_some(PlanPhaseHalt::Clarify)
                );
                assert_eq!(
                    fixture
                        .store
                        .load_task_ledger(project, task)
                        .await?
                        .is_none(),
                    halt
                );
                assert_eq!(
                    fixture
                        .store
                        .load_initial_plan_usage(project, task)
                        .await?
                        .calls,
                    2
                );
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}
