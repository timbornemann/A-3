//! Native accounting tests with typed fixture replies and no provider transport.

#[path = "plan22_initial_plan_reservation_tests.rs"]
mod initial_plan_tests;
use super::*;
use a3_application::{
    EditPhaseCallKind, EditPhaseReservationFailure as AccountingError, EditPhaseReservationStore,
    EditPhaseReservedUsage, EditPhaseWireCost, GoalIntakeLoadResult, GoalIntakeStoreVersion,
    IntakePhaseAnchor, IntakePhaseFinish, IntakePhaseRejection,
    IntakePhaseReservationFailure as Failure, IntakePhaseReservationStore, StoredPendingGoalIntake,
};
use a3_domain::{
    AgentBudgetDimension, AgentRunBudget, AgentRunDurationLimit, AgentTurnLimit, GoalIntakeFinding,
    GoalIntakeFindingKind, GoalIntakeReview, GoalIntakeReviewBatchProposal,
    GoalIntakeReviewRevision, PendingGoalIntake,
};

struct IntakeFixture {
    fixture: Fixture,
    goal: GoalContract,
    step: TaskStepDefinition,
    path: RepositoryPath,
    pending: PendingGoalIntake,
}

impl IntakeFixture {
    fn new() -> TestResult<Self> {
        Self::for_creation(
            SUBJECT_PATH,
            b"Create only subject.py.\n",
            Some(("notes.txt", OTHER_FILE)),
        )
    }

    fn for_creation(
        path: &str,
        request: &[u8],
        existing: Option<(&str, &[u8])>,
    ) -> TestResult<Self> {
        let mut fixture = Fixture::new()?;
        let worktree = fixture.root.join("worktree");
        initialize_git(&worktree)?;
        if let Some((path, bytes)) = existing {
            fs::write(worktree.join(path), bytes)?;
        }
        fixture.project = RepositoryInspector::new().inspect(&worktree)?;
        Self::with_goal(fixture, path, request)
    }

    fn with_goal(fixture: Fixture, path: &str, request: &[u8]) -> TestResult<Self> {
        let path = RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?;
        let (goal, step) = path_edit_goal(&path, request)?;
        let pending = PendingGoalIntake::new(
            goal.original().ok_or("missing original")?.clone(),
            goal.created_at(),
        )?;
        Ok(Self {
            fixture,
            goal,
            step,
            path,
            pending,
        })
    }

    async fn capture(&self) -> TestResult<LibsqlKnowledgeStore> {
        let store = self.open().await?;
        assert_eq!(
            store
                .capture_goal_intake(&self.fixture.project, &self.pending)
                .await?,
            GoalIntakeStoreVersion::INITIAL,
        );
        Ok(store)
    }

    async fn open(&self) -> TestResult<LibsqlKnowledgeStore> {
        Ok(LibsqlKnowledgeStore::open_plan22_candidate(&self.fixture.layout).await?)
    }

    async fn pending(&self, store: &LibsqlKnowledgeStore) -> TestResult<StoredPendingGoalIntake> {
        let Some(GoalIntakeLoadResult::Pending(pending)) = store
            .load_goal_intake(&self.fixture.project, self.goal.task_id())
            .await?
        else {
            return Err("expected durable Pending capture".into());
        };
        Ok(pending)
    }

    async fn anchor(
        &self,
        store: &LibsqlKnowledgeStore,
        revision: Option<GoalIntakeReviewRevision>,
    ) -> TestResult<IntakePhaseAnchor> {
        let pending = self.pending(store).await?;
        let original = pending.pending().original();
        Ok(IntakePhaseAnchor::new(
            &self.fixture.project,
            &pending,
            revision,
            original.range(0, original.as_bytes().len())?,
            profile()?.reference(),
        )?)
    }

    fn review(&self) -> TestResult<GoalIntakeReview> {
        let criterion = self
            .goal
            .draft()
            .acceptance_criteria()
            .first()
            .ok_or("missing criterion")?;
        let origin = criterion.origin().ok_or("missing criterion origin")?;
        let range = origin.original_range();
        let finding = GoalIntakeFinding::new(
            GoalIntakeFindingKind::Outcome {
                requirement: criterion.requirement(),
                evidence_requirement: origin.evidence_requirement().clone(),
            },
            range,
            criterion.statement().as_str().to_owned(),
        )?;
        let proposal = GoalIntakeReviewBatchProposal::new(range, vec![finding], Vec::new())?;
        Ok(GoalIntakeReview::begin(&self.pending, proposal)?)
    }

    async fn promote(
        &self,
        store: &LibsqlKnowledgeStore,
    ) -> Result<(), a3_application::GoalContractStoreFailure> {
        store
            .promote_goal_intake(
                &self.fixture.project,
                self.goal.task_id(),
                GoalIntakeStoreVersion::INITIAL,
                &self.goal,
            )
            .await
    }
}

fn budget() -> TestResult<AgentRunBudget> {
    budget_with_turns(2)
}

fn budget_with_turns(turns: u32) -> TestResult<AgentRunBudget> {
    let default = AgentRunBudget::DEFAULT;
    Ok(AgentRunBudget::new(
        AgentTurnLimit::new(turns)?,
        default.prompt_token_limit(),
        default.output_token_limit(),
        default.action_limit(),
        AgentRunDurationLimit::from_millis(50)?,
        default.repair_limit(),
    ))
}

fn wire(byte: u8) -> TestResult<EditPhaseWireCost> {
    Ok(EditPhaseWireCost::new(
        ContentHash::from_bytes([byte; 32]),
        1200,
        400,
        200,
    )?)
}

fn review_finish(review: &GoalIntakeReview) -> IntakePhaseFinish<'_> {
    IntakePhaseFinish::Review {
        review,
        prompt_tokens: 360,
        output_tokens: 60,
    }
}

#[test]
fn bound_intake_commits_literal_review_and_zero_call_scope_together() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let task = data.goal.task_id();
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let window = anchor.window();
            let literal = data.pending.original().resolve(window)?;
            let kind = data
                .review()?
                .findings()
                .next()
                .ok_or("fixture outcome missing")?
                .kind()
                .clone();
            let review_for = |statement: &str| -> TestResult<GoalIntakeReview> {
                let finding = GoalIntakeFinding::new(kind.clone(), window, statement.to_owned())?;
                let batch = GoalIntakeReviewBatchProposal::new(window, vec![finding], Vec::new())?;
                Ok(GoalIntakeReview::begin(&data.pending, batch)?)
            };
            let review = review_for(literal)?;
            let allowance = budget()?;
            assert_eq!(
                store
                    .commit_bound_intake(
                        project,
                        anchor,
                        &review_for("Different task.")?,
                        allowance,
                        at(3)?
                    )
                    .await,
                Err(Failure::InvalidReview),
            );
            assert_eq!(
                store
                    .commit_bound_intake(project, anchor, &review, allowance, at(51)?)
                    .await,
                Err(Failure::Accounting(AccountingError::DeadlineReached)),
            );

            let database = data.fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute(
                    "CREATE TRIGGER fail_bound_review BEFORE INSERT ON goal_intake_review_batches
                     BEGIN SELECT RAISE(ABORT, 'injected bound review failure'); END",
                    (),
                )
                .await?;
            assert!(
                store
                    .commit_bound_intake(project, anchor, &review, allowance, at(3)?)
                    .await
                    .is_err()
            );
            assert_eq!(
                store.load_intake_phase_accounting(project, task).await?,
                None
            );
            assert_eq!(store.load_goal_intake_review(project, task).await?, None);
            connection
                .execute("DROP TRIGGER fail_bound_review", ())
                .await?;
            drop(connection);
            drop(database);

            let committed = store
                .commit_bound_intake(project, anchor, &review, allowance, at(3)?)
                .await?;
            assert_eq!(committed.usage(), EditPhaseReservedUsage::default());
            assert_eq!(committed.profile(), anchor.model_profile());
            assert_eq!(committed.deadline(), at(51)?);
            drop(store);

            let store = data.open().await?;
            assert_eq!(
                store.load_intake_phase_accounting(project, task).await?,
                Some(committed)
            );
            assert_eq!(
                store.load_goal_intake_review(project, task).await?.as_ref(),
                Some(&review)
            );
            assert_eq!(
                store
                    .commit_bound_intake(project, anchor, &review, allowance, at(4)?)
                    .await,
                Err(Failure::ReviewConflict),
            );
            let current = data.anchor(&store, Some(review.revision())).await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        current,
                        EditPhaseCallKind::Initial,
                        allowance,
                        wire(1)?,
                        at(4)?
                    )
                    .await,
                Err(Failure::Accounting(AccountingError::AlreadyReserved)),
            ));
            let goal = a3_application::prepare_goal_from_intake_review(&data.pending, &review)?;
            store
                .promote_goal_intake(project, task, GoalIntakeStoreVersion::INITIAL, &goal)
                .await?;
            assert_eq!(
                store.load_intake_phase_accounting(project, task).await?,
                Some(committed)
            );
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn repaired_intake_review_and_run_share_charge_and_absolute_deadline_across_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let task = data.goal.task_id();
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let allowance = budget_with_turns(3)?;
            let deadline = at(data.pending.created_at().unix_millis() + 50)?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        anchor,
                        EditPhaseCallKind::Initial,
                        allowance,
                        wire(1)?,
                        deadline
                    )
                    .await,
                Err(Failure::Accounting(AccountingError::DeadlineReached))
            ));
            assert!(
                store
                    .load_intake_phase_accounting(project, task)
                    .await?
                    .is_none()
            );
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    allowance,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            assert_eq!(permit.deadline(), deadline);
            assert_eq!(
                store
                    .load_intake_phase_accounting(project, task)
                    .await?
                    .ok_or("scope missing")?
                    .usage()
                    .unobserved_calls,
                1
            );
            store
                .finish_intake_phase(
                    project,
                    permit,
                    IntakePhaseFinish::ObservedFailure {
                        kind: IntakePhaseRejection::InvalidStructure,
                        reported_prompt_tokens: Some(90),
                        reported_output_tokens: Some(15),
                    },
                    at(4)?,
                )
                .await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::StructuralRepair,
                    allowance,
                    wire(2)?,
                    at(5)?,
                )
                .await?;
            let review = data.review()?;
            store
                .finish_intake_phase(project, permit, review_finish(&review), at(6)?)
                .await?;
            drop(store);

            let store = data.open().await?;
            assert_eq!(
                store.load_goal_intake_review(project, task).await?,
                Some(review.clone())
            );
            let scope = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?;
            assert_eq!(scope.deadline(), deadline);
            assert_eq!(scope.usage().calls, 2);
            assert_eq!(scope.usage().repairs, 1);
            assert_eq!(scope.usage().prompt_tokens, 800);
            assert_eq!(scope.usage().output_tokens, 400);
            let next = data.anchor(&store, Some(review.revision())).await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        next,
                        EditPhaseCallKind::Initial,
                        AgentRunBudget::DEFAULT,
                        wire(2)?,
                        at(7)?
                    )
                    .await,
                Err(Failure::ScopeConflict)
            ));
            data.promote(&store).await?;
            drop(store);

            let (store, _, _, published) = index_fixture(&data.fixture).await?;
            assert!(matches!(
                store.load_goal_intake(project, task).await?,
                Some(GoalIntakeLoadResult::Materialized(reference)) if reference == data.goal.reference()
            ));
            let scope = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("lost scope")?;
            assert_eq!(scope.linked_run(), None);
            let materialized = MaterializeOriginalTask::new(store.as_ref(), store.as_ref())
                .execute(project, &data.goal, vec![data.step.clone()])
                .await?;
            let ledger = materialized.stored_ledger().ledger();
            let snapshot = published.run().snapshot_id();
            let started_at = at(10)?;
            let model = profile()?;
            let (wrong, event) = AgentRun::start_with_budget(
                AgentRunId::from_bytes([177; 32]),
                data.goal.reference(),
                ledger.revision(),
                model.reference(),
                allowance,
                snapshot,
                RunEventId::from_bytes([177; 32]),
                started_at,
            )?;
            assert!(
                CreateAgentRun::new(store.as_ref())
                    .execute(project, &wrong, &event)
                    .await
                    .is_err()
            );
            assert!(store.load_agent_run(project, wrong.id()).await?.is_none());

            let effective = scope.run_budget(started_at)?;
            let (run, stored, _) = start_fixture_step_with_budget(
                &data.fixture,
                store.as_ref(),
                &data.goal,
                &model,
                snapshot,
                vec![data.step.clone()],
                (effective, started_at),
            )
            .await?;
            assert_eq!(run.created_at(), started_at);
            assert_eq!(
                started_at.unix_millis() + run.budget().duration_limit().millis(),
                deadline.unix_millis()
            );
            assert_eq!(run.usage(), a3_domain::AgentRunUsage::ZERO);
            drop(store);

            let store = data.open().await?;
            let linked = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("lost scope")?;
            assert_eq!(linked.linked_run(), Some(run.id()));
            assert_eq!(linked.deadline(), deadline);
            let before_edit = store.load_edit_phase_usage(project, run.id()).await?;
            assert_eq!(before_edit, linked.usage());
            let phase = PreparedEditPhase::add(
                EditPhaseState::new(project, &data.goal, &stored, &run, &published)?,
                data.path.clone(),
            )?;
            let permit = store
                .reserve_edit_phase(
                    project,
                    &phase,
                    EditPhaseCallKind::Initial,
                    wire(2)?,
                    at(15)?,
                )
                .await?;
            assert_eq!(permit.deadline(), deadline);
            store
                .finish_edit_phase(
                    project,
                    permit,
                    a3_application::EditPhaseObservation::new(
                        a3_application::EditPhaseObservationOutcome::AcceptedReply,
                        Some(350),
                        Some(65),
                        at(16)?,
                    ),
                )
                .await?;
            let next_phase = PreparedEditPhase::add(
                EditPhaseState::new(project, &data.goal, &stored, &run, &published)?,
                RepositoryPath::try_from_bytes(b"alternate.py".to_vec())?,
            )?;
            assert!(matches!(
                store
                    .reserve_edit_phase(
                        project,
                        &next_phase,
                        EditPhaseCallKind::Initial,
                        wire(3)?,
                        at(17)?
                    )
                    .await,
                Err(AccountingError::BudgetExhausted(
                    AgentBudgetDimension::Turns
                ))
            ));
            let (duplicate, event) = AgentRun::start_with_budget(
                AgentRunId::from_bytes([178; 32]),
                data.goal.reference(),
                stored.ledger().revision(),
                model.reference(),
                linked.run_budget(at(18)?)?,
                snapshot,
                RunEventId::from_bytes([178; 32]),
                at(18)?,
            )?;
            assert!(
                CreateAgentRun::new(&store)
                    .execute(project, &duplicate, &event)
                    .await
                    .is_err()
            );
            assert!(
                store
                    .load_agent_run(project, duplicate.id())
                    .await?
                    .is_none()
            );
            assert_eq!(
                store.load_edit_phase_usage(project, run.id()).await?,
                EditPhaseReservedUsage {
                    calls: 3,
                    repairs: 1,
                    prompt_tokens: 1200,
                    output_tokens: 600,
                    unobserved_calls: 0,
                    unknown_calls: 0,
                }
            );
            assert_eq!(
                store
                    .load_agent_run(project, run.id())
                    .await?
                    .ok_or("lost Run")?,
                run
            );
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn rejected_intake_call_cannot_reissue_same_window_and_review_after_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let allowance = budget()?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    allowance,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            store
                .finish_intake_phase(
                    project,
                    permit,
                    IntakePhaseFinish::ObservedFailure {
                        kind: IntakePhaseRejection::RejectedReply,
                        reported_prompt_tokens: Some(300),
                        reported_output_tokens: Some(27),
                    },
                    at(4)?,
                )
                .await?;
            drop(store);

            let store = data.open().await?;
            let anchor = data.anchor(&store, None).await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        anchor,
                        EditPhaseCallKind::Initial,
                        allowance,
                        wire(9)?,
                        at(5)?
                    )
                    .await,
                Err(Failure::Accounting(AccountingError::AlreadyReserved))
            ));
            assert!(
                store
                    .load_goal_intake_review(project, data.goal.task_id())
                    .await?
                    .is_none()
            );
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        anchor,
                        EditPhaseCallKind::StructuralRepair,
                        allowance,
                        wire(9)?,
                        at(5)?,
                    )
                    .await,
                Err(Failure::Accounting(AccountingError::RepairNotAvailable))
            ));
            let scope = store
                .load_intake_phase_accounting(project, data.goal.task_id())
                .await?
                .ok_or("scope missing")?;
            assert_eq!(scope.usage().calls, 1);
            assert_eq!(scope.usage().output_tokens, 200);
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn newer_review_cas_settles_old_call_stale_without_overwriting_review() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let task = data.goal.task_id();
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    budget()?,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            let review = data.review()?;
            store
                .append_goal_intake_review(
                    project,
                    task,
                    GoalIntakeStoreVersion::INITIAL,
                    None,
                    &review,
                )
                .await?;
            assert_eq!(
                store
                    .finish_intake_phase(project, permit, review_finish(&review), at(4)?)
                    .await,
                Err(Failure::ReviewConflict),
            );
            drop(store);
            let store = data.open().await?;
            assert_eq!(
                store.load_goal_intake_review(project, task).await?,
                Some(review)
            );
            let usage = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?
                .usage();
            assert_eq!(usage.calls, 1);
            assert_eq!(usage.unobserved_calls, 0);
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn failed_settlement_rolls_review_back_but_keeps_original_charge_unobserved() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let task = data.goal.task_id();
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    budget()?,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            let database = data.fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection.execute(
                "CREATE TRIGGER fail_intake_settlement BEFORE UPDATE ON intake_phase_reservations
                 BEGIN SELECT RAISE(ABORT, 'injected settlement failure'); END",
                (),
            ).await?;
            assert_eq!(
                store
                    .finish_intake_phase(project, permit, review_finish(&data.review()?), at(4)?)
                    .await,
                Err(Failure::Accounting(AccountingError::Unavailable)),
            );
            drop(connection);
            drop(database);
            drop(store);

            let store = data.open().await?;
            assert!(
                store
                    .load_goal_intake_review(project, task)
                    .await?
                    .is_none()
            );
            let current = data.anchor(&store, None).await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        current,
                        EditPhaseCallKind::Initial,
                        budget()?,
                        wire(1)?,
                        at(5)?
                    )
                    .await,
                Err(Failure::Accounting(AccountingError::UncertainProviderCall)),
            ));
            let usage = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?
                .usage();
            assert_eq!(usage.unobserved_calls, 1);
            assert_eq!(usage.prompt_tokens, 400);
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn an_unobserved_or_unknown_call_blocks_promotion_even_with_complete_review() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let task = data.goal.task_id();
            let store = data.capture().await?;
            let review = data.review()?;
            store
                .append_goal_intake_review(
                    project,
                    task,
                    GoalIntakeStoreVersion::INITIAL,
                    None,
                    &review,
                )
                .await?;
            let anchor = data.anchor(&store, Some(review.revision())).await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    budget()?,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            assert_eq!(
                data.promote(&store).await,
                Err(a3_application::GoalContractStoreFailure::InvalidIntakePromotion)
            );
            store
                .finish_intake_phase(
                    project,
                    permit,
                    IntakePhaseFinish::ObservedFailure {
                        kind: IntakePhaseRejection::Unknown,
                        reported_prompt_tokens: None,
                        reported_output_tokens: None,
                    },
                    at(4)?,
                )
                .await?;
            drop(store);

            let store = data.open().await?;
            assert_eq!(
                data.promote(&store).await,
                Err(a3_application::GoalContractStoreFailure::InvalidIntakePromotion)
            );
            assert_eq!(data.pending(&store).await?.pending(), &data.pending);
            assert_eq!(
                store.load_goal_intake_review(project, task).await?,
                Some(review)
            );
            let usage = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("scope missing")?
                .usage();
            assert_eq!(usage.unknown_calls, 1);
            assert_eq!(usage.prompt_tokens, 400);
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn linked_run_charges_real_core_read_and_enforces_cap_after_reopen_without_charging_model_turn()
-> TestResult {
    use a3_application::{
        AgentReadAction, AgentReadTimeout, AgentReadTools, AgentRecoveryStoreFailure,
        AppendAgentRead,
    };
    use a3_context::DeterministicAgentReadTools;
    use a3_domain::{
        AgentActionLimit, AgentInspectAction, AgentInspectTarget, AgentToolAttemptStatus,
    };

    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let defaults = budget()?;
            let allowance = AgentRunBudget::new(
                defaults.turn_limit(),
                defaults.prompt_token_limit(),
                defaults.output_token_limit(),
                AgentActionLimit::new(1)?,
                defaults.duration_limit(),
                defaults.repair_limit(),
            );
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    anchor,
                    EditPhaseCallKind::Initial,
                    allowance,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            store
                .finish_intake_phase(project, permit, review_finish(&data.review()?), at(4)?)
                .await?;
            data.promote(&store).await?;
            drop(store);

            let (store, _, _, published) = index_fixture(&data.fixture).await?;
            let scope = store
                .load_intake_phase_accounting(project, data.goal.task_id())
                .await?
                .ok_or("missing allowance")?;
            let (mut run, stored, _) = start_fixture_step_with_budget(
                &data.fixture,
                store.as_ref(),
                &data.goal,
                &profile()?,
                published.run().snapshot_id(),
                vec![data.step.clone()],
                (scope.run_budget(at(10)?)?, at(10)?),
            )
            .await?;
            let snapshot = run.current_snapshot_id();
            let tool = ToolRunId::from_bytes([181; 32]);
            assert!(matches!(
                store
                    .begin_agent_tool_attempt(project, run.id(), snapshot, tool, scope.deadline())
                    .await,
                Err(AgentRecoveryStoreFailure::ResourceLimit)
            ));
            let attempt = store
                .begin_agent_tool_attempt(project, run.id(), snapshot, tool, at(14)?)
                .await?;
            assert_eq!(attempt.status(), AgentToolAttemptStatus::InFlight);
            let reader = DeterministicAgentReadTools::new(
                store.as_ref(),
                store.as_ref(),
                store.as_ref(),
                &WorkspaceAgentSourceReader,
            );
            let target = AgentInspectTarget::File(AgentFileInspection::new(
                RepositoryPath::try_from_bytes(b"notes.txt".to_vec())?,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(1)?,
            ));
            let mut read = reader
                .execute(
                    project,
                    snapshot,
                    tool,
                    &AgentReadAction::Inspect(AgentInspectAction::new(target)),
                    AgentReadTimeout::DEFAULT,
                    &ActiveControl,
                )
                .await?;
            assert_eq!(
                read.take_original_page()
                    .ok_or("missing actual safe-reader page")?
                    .text()
                    .as_bytes(),
                OTHER_FILE
            );
            let sequence = run.last_event_sequence();
            let result = read.record(&mut run, RunEventId::from_bytes([181; 32]), at(15)?)?;
            AppendAgentRead::new(store.as_ref())
                .execute(project, sequence, &run, &result)
                .await?;
            assert_eq!(run.usage(), a3_domain::AgentRunUsage::ZERO);
            drop(store);

            let store = data.open().await?;
            let run = store
                .load_agent_run(project, run.id())
                .await?
                .ok_or("Run missing")?;
            let forbidden = ToolRunId::from_bytes([182; 32]);
            assert!(matches!(
                store
                    .begin_agent_tool_attempt(project, run.id(), snapshot, forbidden, at(16)?)
                    .await,
                Err(AgentRecoveryStoreFailure::ResourceLimit)
            ));
            let phase = PreparedEditPhase::add(
                EditPhaseState::new(project, &data.goal, &stored, &run, &published)?,
                data.path.clone(),
            )?;
            let permit = store
                .reserve_edit_phase(
                    project,
                    &phase,
                    EditPhaseCallKind::Initial,
                    wire(2)?,
                    at(17)?,
                )
                .await?;
            store
                .finish_edit_phase(
                    project,
                    permit,
                    a3_application::EditPhaseObservation::new(
                        a3_application::EditPhaseObservationOutcome::RejectedReply,
                        Some(300),
                        Some(30),
                        at(18)?,
                    ),
                )
                .await?;
            assert_eq!(
                store.load_edit_phase_usage(project, run.id()).await?.calls,
                2
            );
            assert_eq!(run.usage(), a3_domain::AgentRunUsage::ZERO);

            {
                let database = data.fixture.candidate_database().await?;
                let connection = database.connection_for_operation().await?;
                let mut rows = connection.query(
                    "SELECT
                        (SELECT COUNT(*) FROM tool_run_attempts WHERE run_id=?1),
                        (SELECT COUNT(*) FROM tool_run_attempts WHERE run_id=?1 AND status='succeeded'),
                        (SELECT COUNT(*) FROM run_events WHERE run_id=?1 AND event_kind='model_interaction')",
                    [run.id().as_bytes().to_vec()],
                ).await?;
                let row = rows.next().await?.ok_or("missing attempt counts")?;
                assert_eq!(
                    (row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?),
                    (1, 1, 0)
                );
            }
            drop(store);
            data.fixture.cleanup()
        })
    })
}

/// Controlled Intake response; all metadata and original reads use the actual candidate tools.
#[test]
fn linked_metadata_leaves_source_progress_open_and_two_unsent_reads_halt_after_reopen() -> TestResult
{
    use a3_application::{
        AgentReadAction, AgentRecoveryStoreFailure, BoundEditSource, CompileTaskLens,
        RunEventPageLimit, SourceDeliveryFailure, SourceDeliveryOutcome, SourceReadFailure,
        SourceReadStore,
    };
    use a3_domain::{
        AgentActionLimit, AgentInspectAction, AgentInspectTarget, RunEventKind, RunEventSubject,
        TaskLensSeed, TaskLensSeedSet, TaskLensSeedText, TaskLensTokenBudget,
    };

    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let worktree = data.fixture.root.join("worktree");
            const SOURCE: &[u8] = b"def read_note(value):\n    return value + 1\n";
            fs::write(worktree.join("notes.py"), SOURCE)?;
            let before = inventory(&worktree)?;
            let defaults = budget()?;
            let allowance = AgentRunBudget::new(
                defaults.turn_limit(),
                defaults.prompt_token_limit(),
                defaults.output_token_limit(),
                AgentActionLimit::new(8)?,
                defaults.duration_limit(),
                defaults.repair_limit(),
            );
            let store = data.capture().await?;
            let permit = store
                .reserve_intake_phase(
                    project,
                    data.anchor(&store, None).await?,
                    EditPhaseCallKind::Initial,
                    allowance,
                    wire(1)?,
                    at(3)?,
                )
                .await?;
            store
                .finish_intake_phase(project, permit, review_finish(&data.review()?), at(4)?)
                .await?;
            data.promote(&store).await?;
            drop(store);

            let (store, _, _, published) = index_fixture(&data.fixture).await?;
            let task = data.goal.task_id();
            let scope = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("original action allowance missing")?;
            let snapshot = published.run().snapshot_id();
            let (mut run, ledger, _) = start_fixture_step_with_budget(
                &data.fixture,
                &store,
                &data.goal,
                &profile()?,
                snapshot,
                vec![data.step.clone()],
                (scope.run_budget(at(10)?)?, at(10)?),
            )
            .await?;
            let run_id = run.id();
            let linked = store
                .load_intake_phase_accounting(project, task)
                .await?
                .ok_or("Run did not link to Intake")?;
            assert_eq!(linked.linked_run(), Some(run_id));
            assert_eq!(linked.budget().action_limit().get(), 8);
            assert_eq!(linked.deadline(), scope.deadline());
            assert_eq!(linked.usage(), scope.usage());

            let path = RepositoryPath::try_from_bytes(b"notes.py".to_vec())?;
            let symbol = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .find(|symbol| {
                    symbol.revision().path() == &path
                        && symbol.parsed().name().as_str() == "read_note"
                })
                .ok_or("real indexed source declaration missing")?
                .id();
            let seed = TaskLensSeedText::try_from_string("Read the notes declaration".to_owned())?;
            let lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref())
                .execute(
                    project,
                    TaskLensSeedSet::new(
                        seed.clone(),
                        seed,
                        vec![
                            TaskLensSeed::ExplicitPath(path),
                            TaskLensSeed::ExplicitSymbol(symbol),
                        ],
                    )?,
                    TaskLensTokenBudget::new(4_096)?,
                    &ActiveControl,
                )
                .await?;
            let bound = BoundEditSource::for_symbol(project, &published, &lens, symbol)?;
            let metadata = AgentReadAction::Inspect(AgentInspectAction::new(
                AgentInspectTarget::Symbol(symbol),
            ));
            let original = bound.read_action(project, &published)?;
            let before_reads = run.last_event_sequence();

            for (seed, time) in [(181, 14), (182, 16), (183, 18), (184, 20), (185, 22)] {
                let tool = ToolRunId::from_bytes([seed; 32]);
                store
                    .begin_agent_tool_attempt(project, run_id, snapshot, tool, at(time)?)
                    .await?;
                assert!(
                    record_linked_read(&store, project, &mut run, &metadata, (seed, time + 1))
                        .await?
                        .is_none()
                );
            }
            let anchor =
                EditPhaseState::new(project, &data.goal, &ledger, &run, &published)?.anchor();
            let only_metadata = store.load_source_read_state(project, anchor).await?;
            assert!(only_metadata.progress().receipts().is_empty());
            assert!(
                only_metadata.progress().pending().is_none() && only_metadata.acquired().is_none()
            );

            for (seed, time) in [(186, 24), (187, 26)] {
                let anchor =
                    EditPhaseState::new(project, &data.goal, &ledger, &run, &published)?.anchor();
                let admission = store
                    .begin_source_read(
                        project,
                        anchor,
                        &bound,
                        ToolRunId::from_bytes([seed; 32]),
                        at(time)?,
                    )
                    .await?;
                let page =
                    record_linked_read(&store, project, &mut run, &original, (seed, time + 1))
                        .await?
                        .ok_or("actual bounded source page missing")?;
                assert_eq!(page.text().as_bytes(), SOURCE);
                assert_eq!(page.revision(), bound.required().revision());
                assert!(page.range().contains(bound.required().range()));
                let anchor =
                    EditPhaseState::new(project, &data.goal, &ledger, &run, &published)?.anchor();
                let unsent = store.load_source_read_state(project, anchor).await?;
                assert_eq!(
                    unsent.acquired().map(|stamp| stamp.key()),
                    Some(admission.key())
                );
                assert!(unsent.progress().pending().is_some());
                store
                    .abandon_source_read(project, anchor, admission.key())
                    .await?;
            }
            drop(store);

            let store = data.open().await?;
            let mut run = store
                .load_agent_run(project, run_id)
                .await?
                .ok_or("Run lost after reopen")?;
            let goal = store
                .load_current_goal_contract(project, task)
                .await?
                .ok_or("Goal lost after reopen")?;
            let ledger = store
                .load_task_ledger(project, task)
                .await?
                .ok_or("Ledger lost after reopen")?;
            let published = store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("publication lost after reopen")?;
            let anchor = EditPhaseState::new(project, &goal, &ledger, &run, &published)?.anchor();
            let replayed = store.load_source_read_state(project, anchor).await?;
            let [first, second] = replayed.progress().receipts() else {
                return Err("expected two durable unsent acquisitions".into());
            };
            for (ordinal, receipt) in [(1, first), (2, second)] {
                assert_eq!(receipt.admission().ordinal().get(), ordinal);
                assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NotDelivered);
                assert!(receipt.delivery().is_none());
            }
            assert!(replayed.progress().pending().is_none() && replayed.acquired().is_none());
            assert!(matches!(
                store
                    .begin_source_read(
                        project,
                        anchor,
                        &bound,
                        ToolRunId::from_bytes([188; 32]),
                        at(28)?,
                    )
                    .await,
                Err(SourceReadFailure::Progress(
                    SourceDeliveryFailure::NoveltyLimit
                ))
            ));

            // One ordinary action still fits. The rejected source admission consumed no tool charge.
            let last = ToolRunId::from_bytes([188; 32]);
            store
                .begin_agent_tool_attempt(project, run_id, snapshot, last, at(28)?)
                .await?;
            assert!(
                record_linked_read(&store, project, &mut run, &metadata, (188, 29))
                    .await?
                    .is_none()
            );
            assert!(matches!(
                store
                    .begin_agent_tool_attempt(
                        project,
                        run_id,
                        snapshot,
                        ToolRunId::from_bytes([189; 32]),
                        at(30)?,
                    )
                    .await,
                Err(AgentRecoveryStoreFailure::ResourceLimit)
            ));
            let events = store
                .load_run_events(
                    project,
                    run_id,
                    Some(before_reads),
                    RunEventPageLimit::new(16)?,
                )
                .await?;
            assert!(!events.has_more());
            assert_eq!(events.events().len(), 8);
            for (event, seed) in events.events().iter().zip(181..=188) {
                assert_eq!(event.kind(), RunEventKind::ToolAction);
                assert_eq!(
                    event.subject(),
                    Some(RunEventSubject::Tool(ToolRunId::from_bytes([seed; 32])))
                );
                assert!(event.turn_charge().is_none());
            }
            assert_eq!(run.state(), AgentControllerState::Execute);
            assert_eq!(run.usage(), a3_domain::AgentRunUsage::ZERO);
            assert_eq!(
                store.load_edit_phase_usage(project, run_id).await?,
                linked.usage()
            );
            assert!(
                store
                    .load_agent_mutation_attempts(project, run_id)
                    .await?
                    .is_empty()
            );
            assert_eq!(inventory(&worktree)?, before);
            drop(store);
            data.fixture.cleanup()
        })
    })
}

async fn record_linked_read(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: &mut AgentRun,
    action: &a3_application::AgentReadAction,
    event: (u8, u64),
) -> TestResult<Option<AgentSourcePage>> {
    use a3_application::{
        AgentReadTimeout, AgentReadTools, AppendAgentRead, ContextToolResultStatus,
    };
    use a3_context::DeterministicAgentReadTools;

    let (seed, time) = event;
    let tool = ToolRunId::from_bytes([seed; 32]);
    let reader = DeterministicAgentReadTools::new(store, store, store, &WorkspaceAgentSourceReader);
    let mut read = reader
        .execute(
            project,
            run.current_snapshot_id(),
            tool,
            action,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    assert_eq!(read.status(), ContextToolResultStatus::Succeeded);
    let page = read.take_original_page();
    let previous = run.last_event_sequence();
    let record = read.record(run, RunEventId::from_bytes([seed; 32]), at(time)?)?;
    assert_eq!(
        record.original_evidence(),
        page.as_ref().map(|page| page.evidence().id())
    );
    AppendAgentRead::new(store)
        .execute(project, previous, run, &record)
        .await?;
    Ok(page)
}

#[test]
fn structural_intake_repair_requires_invalid_same_job_and_survives_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let data = IntakeFixture::new()?;
            let project = &data.fixture.project;
            let store = data.capture().await?;
            let anchor = data.anchor(&store, None).await?;
            let allowance = budget_with_turns(3)?;
            let initial = EditPhaseCallKind::Initial;
            let repair = EditPhaseCallKind::StructuralRepair;
            assert!(matches!(
                store
                    .reserve_intake_phase(project, anchor, repair, allowance, wire(2)?, at(3)?)
                    .await,
                Err(Failure::Accounting(AccountingError::RepairNotAvailable))
            ));
            let first = store
                .reserve_intake_phase(project, anchor, initial, allowance, wire(1)?, at(3)?)
                .await?;
            let bad_structure = IntakePhaseFinish::ObservedFailure {
                kind: IntakePhaseRejection::InvalidStructure,
                reported_prompt_tokens: Some(13),
                reported_output_tokens: Some(9),
            };
            store
                .finish_intake_phase(project, first, bad_structure, at(4)?)
                .await?;
            drop(store);

            let store = data.open().await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(project, anchor, repair, allowance, wire(1)?, at(5)?)
                    .await,
                Err(Failure::Accounting(AccountingError::RepairNotAvailable))
            ));
            let retry = store
                .reserve_intake_phase(project, anchor, repair, allowance, wire(2)?, at(5)?)
                .await?;
            assert_eq!(retry.kind(), repair);
            assert_eq!(retry.anchor(), anchor);
            assert_eq!(retry.deadline().unix_millis(), 51);
            store
                .finish_intake_phase(project, retry, bad_structure, at(6)?)
                .await?;
            drop(store);

            let store = data.open().await?;
            assert!(matches!(
                store
                    .reserve_intake_phase(project, anchor, repair, allowance, wire(3)?, at(7)?)
                    .await,
                Err(Failure::Accounting(AccountingError::AlreadyReserved))
            ));
            let pending = data.pending(&store).await?;
            let smaller = pending.pending().original().range(0, 6)?;
            let reframed =
                IntakePhaseAnchor::new(project, &pending, None, smaller, anchor.model_profile())?;
            assert!(matches!(
                store
                    .reserve_intake_phase(
                        project,
                        reframed,
                        initial,
                        AgentRunBudget::DEFAULT,
                        wire(3)?,
                        at(7)?,
                    )
                    .await,
                Err(Failure::ScopeConflict)
            ));
            assert!(matches!(
                store
                    .reserve_intake_phase(project, reframed, initial, allowance, wire(3)?, at(7)?,)
                    .await,
                Err(Failure::Accounting(AccountingError::AlreadyReserved))
            ));
            let scope = store
                .load_intake_phase_accounting(project, data.goal.task_id())
                .await?
                .ok_or("Intake allowance missing")?;
            assert_eq!(scope.usage().calls, 2);
            assert_eq!(scope.usage().repairs, 1);
            assert_eq!(scope.usage().unobserved_calls, 0);
            assert_eq!(scope.usage().unknown_calls, 0);
            assert_eq!(scope.deadline().unix_millis(), 51);
            assert!(
                store
                    .load_goal_intake_review(project, data.goal.task_id())
                    .await?
                    .is_none()
            );
            drop(store);
            data.fixture.cleanup()?;
            Ok(())
        })
    })
}
