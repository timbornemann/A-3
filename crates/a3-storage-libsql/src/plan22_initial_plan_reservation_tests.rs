//! Native accounting and atomic initial Ledger tests with explicit fixture replies.

#[path = "plan22_empty_inventory_tests.rs"]
mod empty_inventory_tests;

#[path = "plan22_initial_plan_halt_tests.rs"]
mod semantic_halt_tests;

#[path = "plan22_initial_plan_addition_tests.rs"]
mod selected_addition_tests;

use super::*;
use a3_application::{
    AdmittedInitialPlan, InitialPlanFinish, InitialPlanRejection,
    InitialPlanReservationFailure as PlanFailure, InitialPlanReservationStore, PlanPhaseOutcome,
    PlanPhaseState, PlanStepOffer, PreparedPlanPhase,
};
use a3_domain::PublishedIndex;
use serde_json::json;

const INITIAL: EditPhaseCallKind = EditPhaseCallKind::Initial;
const REPAIR: EditPhaseCallKind = EditPhaseCallKind::StructuralRepair;

struct Promoted {
    data: IntakeFixture,
    store: Arc<LibsqlKnowledgeStore>,
    published: PublishedIndex,
    model: ModelProfile,
}

impl Promoted {
    async fn new(turns: u32) -> TestResult<Self> {
        Self::from_intake(IntakeFixture::new()?, budget_with_turns(turns)?).await
    }

    async fn from_intake(data: IntakeFixture, budget: AgentRunBudget) -> TestResult<Self> {
        let store = data.capture().await?;
        let project = &data.fixture.project;
        let anchor = data.anchor(&store, None).await?;
        let permit = store
            .reserve_intake_phase(project, anchor, INITIAL, budget, wire(1)?, at(3)?)
            .await?;
        store
            .finish_intake_phase(project, permit, review_finish(&data.review()?), at(4)?)
            .await?;
        data.promote(&store).await?;
        drop(store);
        let (store, _, _, published) = index_fixture(&data.fixture).await?;
        Ok(Self {
            data,
            store,
            published,
            model: profile()?,
        })
    }

    fn state(&self) -> TestResult<PlanPhaseState<'_>> {
        Ok(PlanPhaseState::new(
            &self.data.fixture.project,
            &self.data.goal,
            &self.published,
            &self.model,
            None,
        )?)
    }

    fn phase(&self, step: TaskStepId) -> TestResult<PreparedPlanPhase> {
        let criterion = self
            .data
            .goal
            .draft()
            .acceptance_criteria()
            .first()
            .ok_or("criterion missing")?;
        Ok(PreparedPlanPhase::new(
            self.state()?,
            vec![PlanStepOffer::new(
                step,
                vec![],
                vec![criterion.id()],
                self.data.step.verification_spec().clone(),
            )],
            None,
        )?)
    }

    fn admitted(&self, phase: &PreparedPlanPhase) -> TestResult<AdmittedInitialPlan> {
        let schema = phase
            .request()
            .structured_output()
            .ok_or("Plan schema missing")?;
        let alias = schema.value()["oneOf"][0]["properties"]["job"]["const"]
            .as_str()
            .ok_or("Plan job missing")?;
        let reply = serde_json::to_vec(&json!({
            "version":1,"job":alias,
            "steps":[{"offer":"P1","outcome":"Create the requested subject.py file"}],
        }))?;
        let PlanPhaseOutcome::Steps(plan) = phase.complete(self.state()?, &reply)? else {
            return Err("fixture Plan missing".into());
        };
        Ok(plan)
    }

    fn cleanup(self) -> TestResult {
        drop(self.store);
        self.data.fixture.cleanup()
    }
}

fn accepted(plan: AdmittedInitialPlan) -> InitialPlanFinish {
    InitialPlanFinish::Accepted {
        plan,
        prompt_tokens: 370,
        output_tokens: 55,
    }
}

#[test]
fn durable_plan_repair_commits_ready_ledger_and_consumption_carries_into_run() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = Promoted::new(3).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            assert!(matches!(
                fixture
                    .store
                    .reserve_initial_plan(project, &phase, REPAIR, wire(3)?, at(6)?)
                    .await,
                Err(PlanFailure::Accounting(AccountingError::RepairNotAvailable))
            ));
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                .await?;
            assert_eq!(permit.deadline(), at(51)?);
            let plan = fixture.admitted(&phase)?;
            assert!(
                fixture
                    .store
                    .create_task_ledger(project, plan.ledger())
                    .await
                    .is_err()
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
                    .unobserved_calls,
                1
            );
            fixture
                .store
                .finish_initial_plan(
                    project,
                    permit,
                    InitialPlanFinish::ObservedFailure {
                        kind: InitialPlanRejection::InvalidStructure,
                        reported_prompt_tokens: Some(340),
                        reported_output_tokens: Some(29),
                    },
                    at(7)?,
                )
                .await?;

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
            let phase = fixture.phase(fixture.data.step.id())?;
            let alternative = fixture.phase(TaskStepId::from_bytes([188; 32]))?;
            assert_ne!(alternative.job_digest(), phase.job_digest());
            assert!(matches!(
                fixture
                    .store
                    .reserve_initial_plan(project, &alternative, INITIAL, wire(4)?, at(8)?)
                    .await,
                Err(PlanFailure::Accounting(AccountingError::AlreadyReserved))
            ));
            assert!(matches!(
                fixture
                    .store
                    .reserve_initial_plan(project, &phase, REPAIR, wire(2)?, at(8)?)
                    .await,
                Err(PlanFailure::Accounting(
                    AccountingError::UnchangedRepairWire
                ))
            ));
            let repair = fixture
                .store
                .reserve_initial_plan(project, &phase, REPAIR, wire(3)?, at(8)?)
                .await?;
            let plan = fixture.admitted(&phase)?;
            let expected = plan.ledger().clone();
            let stored = fixture
                .store
                .finish_initial_plan(project, repair, accepted(plan), at(9)?)
                .await?
                .ok_or("accepted Plan did not store Ledger")?;
            assert_eq!(stored.ledger(), &expected);
            assert_eq!(stored.version(), TaskLedgerStoreVersion::INITIAL);
            assert_eq!(
                fixture.store.load_initial_plan_usage(project, task).await?,
                EditPhaseReservedUsage {
                    calls: 2,
                    repairs: 1,
                    prompt_tokens: 800,
                    output_tokens: 400,
                    unobserved_calls: 0,
                    unknown_calls: 0,
                }
            );

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
            assert_eq!(
                fixture.store.load_task_ledger(project, task).await?,
                Some(stored)
            );
            let allowance = fixture
                .store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("pinned Intake scope missing")?;
            assert_eq!(allowance.usage().calls, 1);
            let definitions = expected
                .steps()
                .map(|step| step.definition().clone())
                .collect();
            let started = at(12)?;
            let (run, stored, _) = start_fixture_step_with_budget(
                &fixture.data.fixture,
                fixture.store.as_ref(),
                &fixture.data.goal,
                &fixture.model,
                fixture.published.run().snapshot_id(),
                definitions,
                (allowance.run_budget(started)?, started),
            )
            .await?;
            assert_eq!(run.usage(), a3_domain::AgentRunUsage::ZERO);
            assert_eq!(
                run.created_at().unix_millis() + run.budget().duration_limit().millis(),
                51
            );
            assert_eq!(
                fixture
                    .store
                    .load_edit_phase_usage(project, run.id())
                    .await?
                    .calls,
                3
            );
            assert_eq!(
                fixture
                    .store
                    .load_edit_phase_usage(project, run.id())
                    .await?
                    .repairs,
                1
            );
            let edit = PreparedEditPhase::add(
                EditPhaseState::new(
                    project,
                    &fixture.data.goal,
                    &stored,
                    &run,
                    &fixture.published,
                )?,
                fixture.data.path.clone(),
            )?;
            assert!(matches!(
                fixture
                    .store
                    .reserve_edit_phase(project, &edit, INITIAL, wire(4)?, at(17)?)
                    .await,
                Err(AccountingError::BudgetExhausted(
                    AgentBudgetDimension::Turns
                ))
            ));
            fixture.cleanup()
        })
    })
}

#[test]
fn interrupted_accepted_settlement_rolls_back_ledger_without_refunding_after_reopen() -> TestResult
{
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = Promoted::new(3).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                .await?;
            let plan = fixture.admitted(&phase)?;
            let database = fixture.data.fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection.execute(
                "CREATE TRIGGER fail_plan_settlement BEFORE UPDATE ON initial_plan_phase_reservations
                 BEGIN SELECT RAISE(ABORT, 'injected Plan settlement failure'); END",
                (),
            ).await?;
            assert_eq!(
                fixture
                    .store
                    .finish_initial_plan(project, permit, accepted(plan), at(7)?)
                    .await,
                Err(PlanFailure::Accounting(AccountingError::Unavailable)),
            );
            drop(connection);
            drop(database);
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
            assert!(
                fixture
                    .store
                    .load_task_ledger(project, task)
                    .await?
                    .is_none()
            );
            let phase = fixture.phase(fixture.data.step.id())?;
            let usage = fixture.store.load_initial_plan_usage(project, task).await?;
            assert_eq!(usage.unobserved_calls, 1);
            assert_eq!(usage.prompt_tokens, 400);
            assert!(matches!(
                fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(8)?)
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
fn stale_publication_settles_charge_and_refuses_ready_ledger() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = Promoted::new(3).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                .await?;
            let plan = fixture.admitted(&phase)?;
            fs::write(
                fixture.data.fixture.root.join("worktree/notes.txt"),
                b"user change\n",
            )?;
            let (_, _, _, updated) = index_fixture(&fixture.data.fixture).await?;
            assert_ne!(
                updated.run().snapshot_id(),
                fixture.published.run().snapshot_id()
            );
            assert_eq!(
                fixture
                    .store
                    .finish_initial_plan(project, permit, accepted(plan), at(7)?)
                    .await,
                Err(PlanFailure::StalePlan),
            );
            assert!(
                fixture
                    .store
                    .load_task_ledger(project, task)
                    .await?
                    .is_none()
            );
            let usage = fixture.store.load_initial_plan_usage(project, task).await?;
            assert_eq!(usage.calls, 1);
            assert_eq!(usage.unobserved_calls, 0);
            fixture.cleanup()
        })
    })
}

#[test]
fn regular_limits_create_no_ledger_and_do_not_reopen_same_goal_for_another_initial() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for finish in [
                InitialPlanFinish::NeedEvidence {
                    prompt_tokens: 300,
                    output_tokens: 28,
                },
                InitialPlanFinish::TooLarge {
                    prompt_tokens: 320,
                    output_tokens: 30,
                },
            ] {
                let fixture = Promoted::new(3).await?;
                let project = &fixture.data.fixture.project;
                let task = fixture.data.goal.task_id();
                let phase = fixture.phase(fixture.data.step.id())?;
                assert!(matches!(
                    fixture
                        .store
                        .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(51)?)
                        .await,
                    Err(PlanFailure::Accounting(AccountingError::DeadlineReached))
                ));
                let permit = fixture
                    .store
                    .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                    .await?;
                assert_eq!(
                    fixture
                        .store
                        .finish_initial_plan(project, permit, finish, at(7)?)
                        .await?,
                    None
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
                assert!(matches!(
                    fixture
                        .store
                        .reserve_initial_plan(project, &phase, REPAIR, wire(3)?, at(8)?)
                        .await,
                    Err(PlanFailure::Accounting(AccountingError::RepairNotAvailable))
                ));
                assert!(matches!(
                    fixture
                        .store
                        .reserve_initial_plan(project, &phase, INITIAL, wire(3)?, at(8)?)
                        .await,
                    Err(PlanFailure::Accounting(AccountingError::AlreadyReserved))
                ));
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn accepted_receipt_refuses_run_link_after_ledger_version_changes() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = Promoted::new(3).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                .await?;
            let plan = fixture.admitted(&phase)?;
            let stored = fixture
                .store
                .finish_initial_plan(project, permit, accepted(plan), at(7)?)
                .await?
                .ok_or("missing Ready Ledger")?;
            let changed = fixture
                .store
                .replace_task_ledger(project, stored.version(), stored.ledger())
                .await?;
            assert_ne!(changed, stored.version());
            let scope = fixture
                .store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?;
            let (run, event) = AgentRun::start_with_budget(
                AgentRunId::from_bytes([187; 32]),
                fixture.data.goal.reference(),
                stored.ledger().revision(),
                fixture.model.reference(),
                scope.run_budget(at(10)?)?,
                fixture.published.run().snapshot_id(),
                RunEventId::from_bytes([187; 32]),
                at(10)?,
            )?;
            assert!(
                CreateAgentRun::new(fixture.store.as_ref())
                    .execute(project, &run, &event)
                    .await
                    .is_err()
            );
            assert!(
                fixture
                    .store
                    .load_agent_run(project, run.id())
                    .await?
                    .is_none()
            );
            let scope = fixture
                .store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?;
            assert_eq!(scope.linked_run(), None);
            fixture.cleanup()
        })
    })
}
