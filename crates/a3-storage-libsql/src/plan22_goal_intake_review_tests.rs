//! Actual append-only Goal transactions; proposals below are fixtures, not model outputs.
#[path = "plan22_goal_loopback_intake_tests.rs"]
mod loopback_http_tests;

use super::*;
use a3_application::{GoalIntakeLoadResult, GoalIntakeStoreVersion, RecordGoalIntakeReview};
use a3_domain::{
    AcceptanceCriterionRequirement, CriterionEvidenceRequirement, GoalContractReference,
    GoalIntakeFinding, GoalIntakeFindingKind, GoalIntakeQuestionDraft, GoalIntakeQuestionKind,
    GoalIntakeReview, GoalIntakeReviewBatchProposal, GoalIntakeReviewRevision, GoalOriginalRange,
    NonGoal, PendingGoalIntake, UserDecision,
};

fn reviewed_goal(diff: bool) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([1; 32]);
    let lines = [
        "R01: preserve fractions\n",
        "Constraint: preserve ünicode\n",
        "NonGoal: avoid broad rewrites\n",
        "Decision: keep the current API\n",
    ];
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        lines.concat().into_bytes(),
    )?;
    let mut ranges = Vec::new();
    let mut cursor = 0;
    for line in lines {
        ranges.push(original.range(cursor, cursor + line.len())?);
        cursor += line.len();
    }
    let evidence = if diff {
        CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
            DiffInvariantMode::OnlyPaths,
            vec![RepositoryPath::try_from_bytes(b"average.py".to_vec())?],
        )?)
    } else {
        test_requirement()?
    };
    let criterion = criterion(2, "The average preserves fractions")?
        .with_origin(CriterionOrigin::new(ranges[0], evidence));
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Implement the captured average request".to_owned())?,
        vec![criterion],
        vec![
            GoalConstraint::try_from_string("preserve ünicode".to_owned())?.with_origin(ranges[1]),
        ],
        vec![NonGoal::try_from_string("avoid broad rewrites".to_owned())?.with_origin(ranges[2])],
        vec![
            UserDecision::try_from_string("keep the current API".to_owned())?
                .with_origin(ranges[3]),
        ],
        SuccessVerification::try_from_string("Verify the requested result".to_owned())?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(10)?)
            .with_original(original)?,
    )
}
fn pending(goal: &GoalContract) -> TestResult<PendingGoalIntake> {
    Ok(PendingGoalIntake::new(
        goal.original().ok_or("original missing")?.clone(),
        goal.created_at(),
    )?)
}
fn finding(
    kind: GoalIntakeFindingKind,
    range: GoalOriginalRange,
    text: &str,
) -> TestResult<GoalIntakeFinding> {
    Ok(GoalIntakeFinding::new(kind, range, text.to_owned())?)
}
fn proposal(
    goal: &GoalContract,
    second: bool,
    question: bool,
) -> TestResult<GoalIntakeReviewBatchProposal> {
    let original = goal.original().ok_or("original missing")?;
    let criterion = &goal.draft().acceptance_criteria()[0];
    let origin = criterion.origin().ok_or("criterion origin missing")?;
    let constraint = &goal.draft().constraints()[0];
    let non_goal = &goal.draft().non_goals()[0];
    let decision = &goal.draft().user_decisions()[0];
    let (window, findings) = if second {
        (
            original.range(
                non_goal
                    .origin()
                    .ok_or("nonGoal origin missing")?
                    .start_byte() as usize,
                original.as_bytes().len(),
            )?,
            vec![
                finding(
                    GoalIntakeFindingKind::NonGoal,
                    non_goal.origin().ok_or("nonGoal origin missing")?,
                    non_goal.as_str(),
                )?,
                finding(
                    GoalIntakeFindingKind::ExistingUserDecision,
                    decision.origin().ok_or("decision origin missing")?,
                    decision.as_str(),
                )?,
            ],
        )
    } else {
        (
            original.range(
                0,
                constraint
                    .origin()
                    .ok_or("constraint origin missing")?
                    .end_byte() as usize,
            )?,
            vec![
                finding(
                    GoalIntakeFindingKind::Outcome {
                        requirement: criterion.requirement(),
                        evidence_requirement: origin.evidence_requirement().clone(),
                    },
                    origin.original_range(),
                    criterion.statement().as_str(),
                )?,
                finding(
                    GoalIntakeFindingKind::Constraint,
                    constraint.origin().ok_or("constraint origin missing")?,
                    constraint.as_str(),
                )?,
            ],
        )
    };
    let questions = if question {
        vec![GoalIntakeQuestionDraft::new(
            GoalIntakeQuestionKind::MissingUserChoice,
            "Which requested behavior should take priority?".to_owned(),
            vec![window],
        )?]
    } else {
        Vec::new()
    };
    Ok(GoalIntakeReviewBatchProposal::new(
        window, findings, questions,
    )?)
}
async fn captured(fixture: &Fixture, goal: &GoalContract) -> TestResult<LibsqlKnowledgeStore> {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        store
            .capture_goal_intake(&fixture.project, &pending(goal)?)
            .await?,
        GoalIntakeStoreVersion::INITIAL
    );
    Ok(store)
}
async fn first(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    goal: &GoalContract,
) -> TestResult<GoalIntakeReview> {
    Ok(RecordGoalIntakeReview::new(store)
        .execute(
            &fixture.project,
            goal.task_id(),
            GoalIntakeStoreVersion::INITIAL,
            None,
            proposal(goal, false, false)?,
        )
        .await?)
}
async fn complete(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    goal: &GoalContract,
    question: bool,
) -> TestResult<GoalIntakeReview> {
    let first = first(store, fixture, goal).await?;
    Ok(RecordGoalIntakeReview::new(store)
        .execute(
            &fixture.project,
            goal.task_id(),
            GoalIntakeStoreVersion::INITIAL,
            Some(first.revision()),
            proposal(goal, true, question)?,
        )
        .await?)
}
async fn assert_pending(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    goal: &GoalContract,
) -> TestResult {
    let Some(GoalIntakeLoadResult::Pending(stored)) = store
        .load_goal_intake(&fixture.project, goal.task_id())
        .await?
    else {
        return Err("pending lost".into());
    };
    assert_eq!(stored.version(), GoalIntakeStoreVersion::INITIAL);
    assert_eq!(stored.pending(), &pending(goal)?);
    assert!(
        store
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .is_none()
    );
    Ok(())
}

#[test]
fn review_cold_reopen_and_promotion_retain_all_typed_proposals_without_second_raw_authority()
-> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = reviewed_goal(false)?;
        let store = captured(&fixture, &goal).await?;
        let one = first(&store, &fixture, &goal).await?;
        assert_eq!(one.revision().get(), 1);
        assert_pending(&store, &fixture, &goal).await?;
        drop(store);
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            cold.load_goal_intake_review(&fixture.project, goal.task_id())
                .await?,
            Some(one.clone())
        );
        let two = RecordGoalIntakeReview::new(&cold)
            .execute(
                &fixture.project,
                goal.task_id(),
                GoalIntakeStoreVersion::INITIAL,
                Some(one.revision()),
                proposal(&goal, true, false)?,
            )
            .await?;
        assert_eq!(two.revision().get(), 2);
        assert_eq!(two.findings().count(), 4);
        cold.promote_goal_intake(
            &fixture.project,
            goal.task_id(),
            GoalIntakeStoreVersion::INITIAL,
            &goal,
        )
        .await?;
        drop(cold);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?,
            Some(goal.clone())
        );
        assert_eq!(
            reopened
                .load_goal_intake_review(&fixture.project, goal.task_id())
                .await?,
            Some(two.clone())
        );
        assert_eq!(
            reopened
                .load_goal_intake(&fixture.project, goal.task_id())
                .await?,
            Some(GoalIntakeLoadResult::Materialized(goal.reference()))
        );
        assert!(
            reopened
                .list_pending_goal_intakes(&fixture.project, None)
                .await?
                .task_ids()
                .is_empty()
        );
        assert_eq!(
            reopened
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    Some(two.revision()),
                    &two
                )
                .await,
            Err(GoalContractStoreFailure::IntakeNotPending)
        );
        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_pending_intakes").await?,
            0
        );
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_original_headers").await?,
            1
        );
        assert_eq!(
            count(
                &connection,
                "SELECT count(*) FROM goal_intake_review_batches"
            )
            .await?,
            2
        );
        drop(connection);
        drop(db);
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn separate_review_cas_and_immutable_prefix_reject_stale_capture_and_forged_history() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = reviewed_goal(false)?;
        let store = captured(&fixture, &goal).await?;
        let one = first(&store, &fixture, &goal).await?;
        let mut two = one.clone();
        two.append(&pending(&goal)?, proposal(&goal, true, false)?)?;
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::new(2)?,
                    Some(one.revision()),
                    &two
                )
                .await,
            Err(GoalContractStoreFailure::IntakeVersionConflict)
        );
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    None,
                    &two
                )
                .await,
            Err(GoalContractStoreFailure::IntakeReviewVersionConflict)
        );
        let mut altered = proposal(&goal, false, false)?.findings().to_vec();
        altered[0] = finding(
            altered[0].kind().clone(),
            altered[0].range(),
            "A different proposed outcome",
        )?;
        let mut forged = GoalIntakeReview::begin(
            &pending(&goal)?,
            GoalIntakeReviewBatchProposal::new(one.batches()[0].window(), altered, Vec::new())?,
        )?;
        forged.append(&pending(&goal)?, proposal(&goal, true, false)?)?;
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    Some(one.revision()),
                    &forged
                )
                .await,
            Err(GoalContractStoreFailure::InvalidIntakeReview)
        );
        assert_eq!(
            store
                .load_goal_intake_review(&fixture.project, goal.task_id())
                .await?,
            Some(one.clone())
        );
        assert_pending(&store, &fixture, &goal).await?;
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    Some(one.revision()),
                    &two
                )
                .await?,
            two.revision()
        );
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    Some(one.revision()),
                    &two
                )
                .await,
            Err(GoalContractStoreFailure::IntakeReviewVersionConflict)
        );
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn actual_questions_incomplete_review_and_changed_findings_cannot_be_promoted() -> TestResult {
    crate::run_native_libsql_test(async {
        for failure in [
            "question",
            "incomplete",
            "changed_constraint",
            "weakened_evidence",
            "requiredness",
        ] {
            let fixture = Fixture::new()?;
            let goal = reviewed_goal(false)?;
            let store = captured(&fixture, &goal).await?;
            let review = if failure == "incomplete" {
                first(&store, &fixture, &goal).await?
            } else {
                complete(&store, &fixture, &goal, failure == "question").await?
            };
            let mut draft = goal.draft().clone();
            if failure == "changed_constraint"
                || failure == "weakened_evidence"
                || failure == "requiredness"
            {
                let old = &draft.acceptance_criteria()[0];
                let criterion = if failure == "requiredness" {
                    AcceptanceCriterion::with_requirement(
                        old.id(),
                        old.statement().clone(),
                        AcceptanceCriterionRequirement::Should,
                    )
                    .with_origin(old.origin().ok_or("criterion origin missing")?.clone())
                } else if failure == "weakened_evidence" {
                    old.clone().with_origin(CriterionOrigin::new(
                        old.origin()
                            .ok_or("criterion origin missing")?
                            .original_range(),
                        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
                    ))
                } else {
                    old.clone()
                };
                let constraints = if failure == "changed_constraint" {
                    vec![
                        GoalConstraint::try_from_string("a different boundary".to_owned())?
                            .with_origin(
                                draft.constraints()[0]
                                    .origin()
                                    .ok_or("constraint origin missing")?,
                            ),
                    ]
                } else {
                    draft.constraints().to_vec()
                };
                draft = GoalContractDraft::new(
                    draft.objective().clone(),
                    vec![criterion],
                    constraints,
                    draft.non_goals().to_vec(),
                    draft.user_decisions().to_vec(),
                    draft.success_verification().clone(),
                )?;
            }
            let proposed = GoalContract::initial(goal.task_id(), draft, goal.created_at())
                .with_original(goal.original().ok_or("original missing")?.clone())?;
            // Exact raw capture and full Goal byte attribution alone genuinely pass.
            pending(&goal)?.validate_promotion(&proposed)?;
            assert_eq!(
                store
                    .promote_goal_intake(
                        &fixture.project,
                        goal.task_id(),
                        GoalIntakeStoreVersion::INITIAL,
                        &proposed
                    )
                    .await,
                Err(GoalContractStoreFailure::InvalidIntakePromotion)
            );
            assert_pending(&store, &fixture, &goal).await?;
            assert_eq!(
                store
                    .load_goal_intake_review(&fixture.project, goal.task_id())
                    .await?,
                Some(review)
            );
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn failed_batch_children_header_and_goal_promotion_restore_the_actual_prior_transaction_state()
-> TestResult {
    crate::run_native_libsql_test(async {
        for table in [
            "goal_intake_review_findings",
            "goal_intake_review_question_ranges",
            "goal_intake_review_batches",
        ] {
            let fixture = Fixture::new()?;
            let goal = reviewed_goal(false)?;
            let store = captured(&fixture, &goal).await?;
            let one = first(&store, &fixture, &goal).await?;
            let mut two = one.clone();
            two.append(&pending(&goal)?, proposal(&goal, true, true)?)?;
            let db = fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            connection
                .execute_batch(&format!(
                    "CREATE TRIGGER fail_review AFTER INSERT ON {table} WHEN NEW.review_sequence=2
                BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;"
                ))
                .await?;
            assert!(
                store
                    .append_goal_intake_review(
                        &fixture.project,
                        goal.task_id(),
                        GoalIntakeStoreVersion::INITIAL,
                        Some(one.revision()),
                        &two
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                store
                    .load_goal_intake_review(&fixture.project, goal.task_id())
                    .await?,
                Some(one)
            );
            assert_pending(&store, &fixture, &goal).await?;
            for table in [
                "goal_intake_review_batches",
                "goal_intake_review_findings",
                "goal_intake_review_questions",
                "goal_intake_review_diff_paths",
                "goal_intake_review_question_ranges",
            ] {
                assert_eq!(
                    count(
                        &connection,
                        &format!("SELECT count(*) FROM {table} WHERE review_sequence=2")
                    )
                    .await?,
                    0
                );
            }
            drop(connection);
            drop(db);
            drop(store);
            fixture.cleanup()?;
        }
        for table in ["tasks", "goal_original_headers"] {
            let fixture = Fixture::new()?;
            let goal = reviewed_goal(false)?;
            let store = captured(&fixture, &goal).await?;
            let review = complete(&store, &fixture, &goal, false).await?;
            let db = fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            connection
                .execute_batch(&format!(
                    "CREATE TRIGGER fail_promotion AFTER INSERT ON {table}
                BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;"
                ))
                .await?;
            assert!(
                store
                    .promote_goal_intake(
                        &fixture.project,
                        goal.task_id(),
                        GoalIntakeStoreVersion::INITIAL,
                        &goal
                    )
                    .await
                    .is_err()
            );
            assert_pending(&store, &fixture, &goal).await?;
            assert_eq!(
                store
                    .load_goal_intake_review(&fixture.project, goal.task_id())
                    .await?,
                Some(review.clone())
            );
            assert_eq!(count(&connection, "SELECT count(*) FROM tasks").await?, 0);
            assert_eq!(
                count(&connection, "SELECT count(*) FROM goal_original_headers").await?,
                0
            );
            connection
                .execute("DROP TRIGGER fail_promotion", ())
                .await?;
            store
                .promote_goal_intake(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    &goal,
                )
                .await?;
            assert_eq!(
                store
                    .load_goal_intake_review(&fixture.project, goal.task_id())
                    .await?,
                Some(review)
            );
            drop(connection);
            drop(db);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn sealed_history_is_immutable_and_unsealed_children_cannot_commit_or_bind_late() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = reviewed_goal(true)?;
        let store = captured(&fixture, &goal).await?;
        let review = complete(&store, &fixture, &goal, true).await?;
        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        for table in [
            "goal_intake_review_batches",
            "goal_intake_review_findings",
            "goal_intake_review_questions",
            "goal_intake_review_diff_paths",
            "goal_intake_review_question_ranges",
        ] {
            assert!(
                connection
                    .execute(&format!("UPDATE {table} SET task_id=task_id"), ())
                    .await
                    .is_err()
            );
            assert!(
                connection
                    .execute(&format!("DELETE FROM {table}"), ())
                    .await
                    .is_err()
            );
        }
        assert!(connection.execute("INSERT INTO goal_intake_review_diff_paths
            SELECT task_id,review_sequence,finding_sequence,2,X'6f746865722e7079' FROM goal_intake_review_diff_paths",()).await.is_err());
        // A deferred FK, not an artificial adapter flag, rejects a transaction without its seal.
        connection.execute_batch("BEGIN IMMEDIATE").await?;
        connection.execute("INSERT INTO goal_intake_review_findings (
                task_id,review_sequence,finding_sequence,kind,start_byte,end_byte,statement,requiredness,evidence_kind,
                command_id,selector_kind,selector_name,minimum_cases,verification_scope,diff_mode,diff_path_count
            )
            SELECT task_id,3,finding_sequence,kind,start_byte,end_byte,statement,requiredness,evidence_kind,
                command_id,selector_kind,selector_name,minimum_cases,verification_scope,diff_mode,diff_path_count
            FROM goal_intake_review_findings WHERE review_sequence=2 AND finding_sequence=1",()).await?;
        assert!(connection.execute_batch("COMMIT").await.is_err());
        connection.execute_batch("ROLLBACK").await?;
        assert_eq!(
            store
                .load_goal_intake_review(&fixture.project, goal.task_id())
                .await?,
            Some(review)
        );
        assert_pending(&store, &fixture, &goal).await?;
        drop(connection);
        drop(db);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn corrupt_review_lengths_ranges_digests_counts_and_orphans_fail_closed_before_unbounded_payload_decode()
-> TestResult {
    crate::run_native_libsql_test(async {
        for corruption in [
            "statement",
            "path",
            "kind",
            "digest",
            "unicode",
            "question_id",
            "question_ranges",
            "finding_count",
            "orphan",
        ] {
            let fixture = Fixture::new()?;
            let goal = reviewed_goal(true)?;
            let store = captured(&fixture, &goal).await?;
            complete(&store, &fixture, &goal, true).await?;
            let db = fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            // Corruption is deliberately confined to this owned negative fixture.
            connection
                .execute_batch("PRAGMA foreign_keys=OFF;PRAGMA ignore_check_constraints=ON")
                .await?;
            let sql=match corruption{
                "statement"=>"DROP TRIGGER goal_intake_review_findings_update_guard;UPDATE goal_intake_review_findings SET statement=CAST(zeroblob(8388608) AS TEXT) WHERE review_sequence=1 AND finding_sequence=1".to_owned(),
                "path"=>"DROP TRIGGER goal_intake_review_diff_paths_update_guard;UPDATE goal_intake_review_diff_paths SET path=zeroblob(8388608)".to_owned(),
                "kind"=>"DROP TRIGGER goal_intake_review_findings_update_guard;UPDATE goal_intake_review_findings SET kind=CAST(zeroblob(8388608) AS TEXT) WHERE review_sequence=1 AND finding_sequence=1".to_owned(),
                "digest"=>"DROP TRIGGER goal_intake_review_batches_update_guard;UPDATE goal_intake_review_batches SET original_digest=zeroblob(32)".to_owned(),
                "unicode"=>{
                    let middle=goal.original().ok_or("original missing")?.as_str().find('ü').ok_or("unicode missing")?+1;
                    format!("DROP TRIGGER goal_intake_review_findings_update_guard;UPDATE goal_intake_review_findings SET start_byte={middle},end_byte={} WHERE review_sequence=1 AND finding_sequence=2",middle+1)
                }
                "question_id"=>"DROP TRIGGER goal_intake_review_questions_update_guard;DROP TRIGGER goal_intake_review_question_ranges_update_guard;UPDATE goal_intake_review_questions SET question_id=2;UPDATE goal_intake_review_question_ranges SET question_id=2".to_owned(),
                "question_ranges"=>"DROP TRIGGER goal_intake_review_questions_update_guard;UPDATE goal_intake_review_questions SET range_count=9".to_owned(),
                "finding_count"=>"DROP TRIGGER goal_intake_review_batches_update_guard;UPDATE goal_intake_review_batches SET finding_count=5 WHERE review_sequence=1".to_owned(),
                _=>"DROP TRIGGER goal_intake_review_batches_delete_guard;DELETE FROM goal_intake_review_batches".to_owned(),
            };
            connection.execute_batch(&sql).await?;
            let result = crate::goal_contract_repository::intake_review::load(
                &connection,
                fixture.project.worktree().id(),
                goal.task_id(),
            )
            .await;
            assert!(matches!(result,Err(crate::goal_contract_repository::GoalContractRepositoryError::InvalidStoredData
                |crate::goal_contract_repository::GoalContractRepositoryError::ResourceLimit)));
            assert_pending(&store, &fixture, &goal).await?;
            drop(connection);
            drop(db);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn old_production_adapter_rejects_review_ports_without_creating_or_changing_goal_rows() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = reviewed_goal(false)?;
        let review = GoalIntakeReview::begin(&pending(&goal)?, proposal(&goal, false, false)?)?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        let legacy = GoalContract::initial(
            goal.task_id(),
            plain_draft("Retain the baseline task")?,
            goal.created_at(),
        );
        store
            .create_goal_contract(&fixture.project, &legacy)
            .await?;
        assert_eq!(
            store
                .load_goal_intake_review(&fixture.project, goal.task_id())
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            store
                .append_goal_intake_review(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    Some(GoalIntakeReviewRevision::new(1)?),
                    &review
                )
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?,
            Some(legacy)
        );
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let db = KnowledgeDatabase::open(&layout, &fixture.project).await?;
        let connection = db.connection_for_operation().await?;
        assert_eq!(count(&connection, "PRAGMA user_version").await?, 42);
        assert_eq!(count(&connection, "SELECT count(*) FROM tasks").await?, 1);
        assert_eq!(
            count(
                &connection,
                "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'goal_intake_review_%'"
            )
            .await?,
            0
        );
        drop(connection);
        drop(db);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}
