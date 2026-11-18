//! A bound decoder's selected Add is committed with the original charged Plan observation.

use super::*;
use a3_domain::PlannedFileAddition;

fn addition_phase(fixture: &Promoted) -> TestResult<PreparedPlanPhase> {
    let criterion = fixture
        .data
        .goal
        .draft()
        .acceptance_criteria()
        .first()
        .ok_or("criterion missing")?;
    let offer = PlanStepOffer::new(
        fixture.data.step.id(),
        Vec::new(),
        vec![criterion.id()],
        fixture.data.step.verification_spec().clone(),
    )
    .with_planned_file_addition(PlannedFileAddition::new(fixture.data.path.clone()))?;
    Ok(PreparedPlanPhase::new(fixture.state()?, vec![offer], None)?)
}

fn selected_addition(
    fixture: &Promoted,
    phase: &PreparedPlanPhase,
) -> TestResult<AdmittedInitialPlan> {
    let schema = phase
        .request()
        .structured_output()
        .ok_or("selection schema missing")?;
    let job = schema.value()["properties"]["job"]["const"]
        .as_str()
        .ok_or("selection job missing")?;
    let reply = serde_json::to_vec(&json!({"version":3,"job":job,"decision":"add"}))?;
    let PlanPhaseOutcome::Steps(plan) = phase.complete(fixture.state()?, &reply)? else {
        return Err("explicit fixture Add selection missing".into());
    };
    Ok(plan)
}

#[test]
fn selected_add_scope_and_charged_plan_settlement_commit_or_roll_back_together() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for fail_scope_insert in [false, true] {
                let fixture = Promoted::from_intake(
                    IntakeFixture::for_creation("chosen.py", b"Create only chosen.py.\n", None)?,
                    budget_with_turns(3)?,
                )
                .await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                let phase = addition_phase(&fixture)?;
                let permit = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(10)?, at(6)?)
                    .await?;
                let plan = selected_addition(&fixture, &phase)?;
                let expected = plan.ledger().clone();
                let db = fixture.data.fixture.candidate_database().await?;
                let connection = db.connection_for_operation().await?;
                if fail_scope_insert {
                    connection.execute_batch(
                        "CREATE TRIGGER abort_selected_add BEFORE INSERT ON task_step_planned_file_additions
                         BEGIN SELECT RAISE(ABORT,'fixture selected Add failure'); END;"
                    ).await?;
                }
                let result = fixture
                    .store
                    .finish_initial_plan(project, permit, accepted(plan), at(7)?)
                    .await;
                if fail_scope_insert {
                    assert_eq!(
                        result,
                        Err(PlanFailure::Accounting(AccountingError::Unavailable))
                    );
                } else {
                    assert_eq!(
                        result?.ok_or("selected Add Ledger missing")?.ledger(),
                        &expected
                    );
                }
                drop((connection, db));
                let Promoted {
                    data,
                    store,
                    published,
                    model,
                } = fixture;
                drop(store);
                let store = Arc::new(data.open().await?);
                let fixture = Promoted {
                    data,
                    store,
                    published,
                    model,
                };
                let project = &fixture.data.fixture.project;
                let loaded = fixture.store.load_task_ledger(project, task).await?;
                if fail_scope_insert {
                    assert!(loaded.is_none());
                } else {
                    let loaded = loaded.ok_or("cold selected Add missing")?;
                    assert_eq!(loaded.ledger(), &expected);
                    let step = loaded
                        .ledger()
                        .step(fixture.data.step.id())
                        .ok_or("cold Add step missing")?;
                    assert_eq!(
                        step.definition().intended_outcome().as_str(),
                        "Create chosen.py"
                    );
                    assert_eq!(
                        step.definition()
                            .planned_file_addition()
                            .ok_or("cold Add scope missing")?
                            .target(),
                        &fixture.data.path
                    );
                    assert_eq!(step.status(), TaskStepStatus::Ready);
                    assert!(step.attempts().is_empty());
                }
                let usage = fixture.store.load_initial_plan_usage(project, task).await?;
                assert_eq!(
                    (usage.calls, usage.repairs, usage.unobserved_calls),
                    (1, 0, u32::from(fail_scope_insert))
                );
                assert_eq!((usage.prompt_tokens, usage.output_tokens), (400, 200));
                assert_eq!(
                    fixture.store.load_initial_plan_halt(project, task).await?,
                    None
                );
                assert_eq!(
                    fixture
                        .store
                        .load_current_goal_contract(project, task)
                        .await?,
                    Some(fixture.data.goal.clone())
                );
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}
