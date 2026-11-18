//! Pending source is real storage input, never a fabricated executable Goal or model result.
use super::*;
use a3_application::{
    CaptureGoalIntake, GoalIntakeLoadResult, GoalIntakeStoreVersion, PromoteGoalIntake,
    StoredPendingGoalIntake,
};
use a3_domain::{GoalContractReference, PendingGoalIntake};

fn complete_goal(bytes: Vec<u8>, created: u64) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([1; 32]);
    let original =
        GoalOriginalRequest::try_from_bytes(GoalContractReference::initial(task), bytes)?;
    let criterion = criterion(2, "The captured change has the requested result")?.with_origin(
        CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            test_requirement()?,
        ),
    );
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Complete the captured request".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Verify the requested result".to_owned())?,
    )?;
    Ok(GoalContract::initial(
        task,
        draft,
        GoalContractTimestamp::from_unix_millis(created)?,
    )
    .with_original(original)?)
}

fn pending(goal: &GoalContract) -> TestResult<PendingGoalIntake> {
    Ok(PendingGoalIntake::new(
        goal.original().ok_or("original missing")?.clone(),
        goal.created_at(),
    )?)
}

async fn load_pending(
    store: &LibsqlKnowledgeStore,
    fixture: &Fixture,
    task: TaskId,
) -> TestResult<StoredPendingGoalIntake> {
    match store.load_goal_intake(&fixture.project, task).await? {
        Some(GoalIntakeLoadResult::Pending(stored)) => Ok(stored),
        _ => Err("expected pending input".into()),
    }
}

#[test]
fn pending_discovery_reopens_without_session_ids_and_pages_past_promoted_tasks() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"The complete captured request".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        for number in 1..=40 {
            let task = TaskId::from_bytes([number; 32]);
            let bytes = if number == 1 {
                goal.original()
                    .ok_or("original missing")?
                    .as_bytes()
                    .to_vec()
            } else {
                format!("Uninterpreted original request {number}\r\n").into_bytes()
            };
            CaptureGoalIntake::new(&store)
                .execute(&fixture.project, task, bytes, goal.created_at())
                .await?;
        }
        PromoteGoalIntake::new(&store)
            .execute(
                &fixture.project,
                goal.task_id(),
                GoalIntakeStoreVersion::INITIAL,
                &goal,
            )
            .await?;
        drop(store);

        // First use of the reopened adapter is discovery, without a retained task/session ID.
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let first = reopened
            .list_pending_goal_intakes(&fixture.project, None)
            .await?;
        let expected: Vec<_> = (2..=40).map(|id| TaskId::from_bytes([id; 32])).collect();
        assert_eq!(first.task_ids(), &expected[..32]);
        let second = reopened
            .list_pending_goal_intakes(&fixture.project, first.next_after())
            .await?;
        assert_eq!(second.task_ids(), &expected[32..]);
        assert_eq!(second.next_after(), None);
        let found = first
            .task_ids()
            .first()
            .copied()
            .ok_or("pending not rediscovered")?;
        let loaded = load_pending(&reopened, &fixture, found).await?;
        assert_eq!(loaded.pending().task_id(), found);
        assert_eq!(
            loaded.pending().original().as_str(),
            "Uninterpreted original request 2\r\n"
        );
        assert_eq!(
            reopened
                .load_goal_intake(&fixture.project, goal.task_id())
                .await?,
            Some(GoalIntakeLoadResult::Materialized(goal.reference())),
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn pending_discovery_is_worktree_bound_and_never_reads_original_payloads() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let goal = complete_goal(b"Captured original".to_vec(), 10)?;
        store
            .capture_goal_intake(&fixture.project, &pending(&goal)?)
            .await?;

        // Listing projects only IDs: payload validity is checked by the required later load.
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute("DROP TRIGGER goal_pending_intakes_update_guard", ())
            .await?;
        connection
            .execute("PRAGMA ignore_check_constraints=ON", ())
            .await?;
        connection
            .execute(
                "UPDATE goal_pending_intakes SET original_bytes=zeroblob(70000)",
                (),
            )
            .await?;
        assert_eq!(
            store
                .list_pending_goal_intakes(&fixture.project, None)
                .await?
                .task_ids(),
            &[goal.task_id()]
        );
        assert_eq!(
            store
                .load_goal_intake(&fixture.project, goal.task_id())
                .await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );

        let other_root = fixture.root.join("other-worktree");
        fs::create_dir(&other_root)?;
        let repository = fixture.project.repository().clone();
        let other = ProjectIdentity::new(
            repository.clone(),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([111; 32]),
                WorktreeAnchorId::from_bytes([112; 32]),
                repository.id(),
                CanonicalDirectory::from_canonicalized(fs::canonicalize(other_root)?)?,
            ),
            fixture.project.head().clone(),
        )?;
        assert!(
            store
                .list_pending_goal_intakes(&other, None)
                .await?
                .task_ids()
                .is_empty()
        );
        connection
            .execute(
                "UPDATE goal_pending_intakes SET task_id=zeroblob(65536)",
                (),
            )
            .await?;
        assert_eq!(
            store
                .list_pending_goal_intakes(&fixture.project, None)
                .await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn captured_64k_unicode_source_reopens_without_any_task_goal_or_session() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let task = TaskId::from_bytes([1; 32]);
        let created = GoalContractTimestamp::from_unix_millis(10)?;
        let original = GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(task),
            large_original(),
        )?;
        assert_eq!(original.parts().len(), 33);
        let source = PendingGoalIntake::new(original, created)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let captured = CaptureGoalIntake::new(&store)
            .execute(
                &fixture.project,
                task,
                source.original().as_bytes().to_vec(),
                created,
            )
            .await?;
        assert_eq!(captured.version(), GoalIntakeStoreVersion::INITIAL);
        assert_eq!(captured.pending(), &source);
        assert_eq!(
            store
                .capture_goal_intake(&fixture.project, captured.pending())
                .await,
            Err(GoalContractStoreFailure::TaskAlreadyExists)
        );
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        for table in [
            "tasks",
            "goal_contract_revisions",
            "goal_original_headers",
            "agent_runs",
        ] {
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
        }
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_pending_intakes").await?,
            1
        );
        drop(connection);
        drop(database);
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(load_pending(&reopened, &fixture, task).await?, captured);
        assert!(
            reopened
                .load_current_goal_contract(&fixture.project, task)
                .await?
                .is_none()
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn promotion_is_cas_bound_and_reopen_derives_only_the_actual_initial_goal_reference() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"Implement the average result".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let source = pending(&goal)?;
        let version = store.capture_goal_intake(&fixture.project, &source).await?;
        assert_eq!(
            store.create_goal_contract(&fixture.project, &goal).await,
            Err(GoalContractStoreFailure::TaskAlreadyExists)
        );
        assert_eq!(
            store
                .promote_goal_intake(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::new(2)?,
                    &goal
                )
                .await,
            Err(GoalContractStoreFailure::IntakeVersionConflict)
        );
        assert_eq!(
            load_pending(&store, &fixture, goal.task_id())
                .await?
                .pending(),
            &source
        );
        assert!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .is_none()
        );
        let competing = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            load_pending(&competing, &fixture, goal.task_id())
                .await?
                .version(),
            version
        );
        assert_eq!(
            PromoteGoalIntake::new(&store)
                .execute(&fixture.project, goal.task_id(), version, &goal)
                .await?,
            goal.reference()
        );
        assert_eq!(
            competing
                .promote_goal_intake(&fixture.project, goal.task_id(), version, &goal)
                .await,
            Err(GoalContractStoreFailure::IntakeNotPending)
        );
        drop(competing);
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_pending_intakes").await?,
            0
        );
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_original_headers").await?,
            1
        );
        assert_eq!(count(&connection, "SELECT count(*) FROM tasks").await?, 1);
        drop(connection);
        drop(database);
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_goal_intake(&fixture.project, goal.task_id())
                .await?,
            Some(GoalIntakeLoadResult::Materialized(
                GoalContractReference::initial(goal.task_id())
            ))
        );
        assert_eq!(
            reopened
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?,
            Some(goal.clone())
        );
        assert_eq!(
            reopened
                .capture_goal_intake(&fixture.project, &source)
                .await,
            Err(GoalContractStoreFailure::TaskAlreadyExists)
        );
        assert_eq!(
            reopened
                .promote_goal_intake(&fixture.project, goal.task_id(), version, &goal)
                .await,
            Err(GoalContractStoreFailure::IntakeNotPending)
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn changed_original_timestamp_and_incomplete_source_never_consume_pending() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"Implement the average result".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let source = pending(&goal)?;
        let version = store.capture_goal_intake(&fixture.project, &source).await?;
        let original = goal.original().ok_or("source missing")?;
        let gap_criterion = criterion(2, "Partial interpretation remains incomplete")?.with_origin(
            CriterionOrigin::new(original.range(0, 1)?, test_requirement()?),
        );
        let gap = GoalContract::initial(
            goal.task_id(),
            GoalContractDraft::new(
                goal.draft().objective().clone(),
                vec![gap_criterion],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                goal.draft().success_verification().clone(),
            )?,
            goal.created_at(),
        )
        .with_original(original.clone())?;
        for invalid in [
            complete_goal(b"Changed original result".to_vec(), 10)?,
            complete_goal(original.as_bytes().to_vec(), 11)?,
            gap,
        ] {
            assert_eq!(
                store
                    .promote_goal_intake(&fixture.project, goal.task_id(), version, &invalid)
                    .await,
                Err(GoalContractStoreFailure::InvalidIntakePromotion)
            );
            assert_eq!(
                load_pending(&store, &fixture, goal.task_id())
                    .await?
                    .pending(),
                &source
            );
            assert!(
                store
                    .load_current_goal_contract(&fixture.project, goal.task_id())
                    .await?
                    .is_none()
            );
        }
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn failed_goal_writes_after_pending_delete_restore_capture_and_no_ready_rows() -> TestResult {
    crate::run_native_libsql_test(async {
        for table in [
            "acceptance_criteria",
            "goal_constraint_origins",
            "goal_original_headers",
            "goal_original_parts",
        ] {
            let fixture = Fixture::new()?;
            let base = complete_goal(b"Implement average\nPreserve the file\n".to_vec(), 10)?;
            let original = base.original().ok_or("source missing")?;
            let split = original.as_str().find('\n').ok_or("line missing")? + 1;
            let criterion = criterion(2, "Implement average")?.with_origin(CriterionOrigin::new(
                original.range(0, split)?,
                test_requirement()?,
            ));
            let constraint = GoalConstraint::try_from_string("Preserve the file".to_owned())?
                .with_origin(original.range(split, original.as_bytes().len())?);
            let goal = GoalContract::initial(
                base.task_id(),
                GoalContractDraft::new(
                    base.draft().objective().clone(),
                    vec![criterion],
                    vec![constraint],
                    Vec::new(),
                    Vec::new(),
                    base.draft().success_verification().clone(),
                )?,
                base.created_at(),
            )
            .with_original(original.clone())?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let source = pending(&goal)?;
            let version = store.capture_goal_intake(&fixture.project, &source).await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute_batch(&format!(
                    "CREATE TRIGGER abort_promotion BEFORE INSERT ON {table}
                BEGIN SELECT RAISE(ABORT,'fixture failure'); END;"
                ))
                .await?;
            assert_eq!(
                store
                    .promote_goal_intake(&fixture.project, goal.task_id(), version, &goal)
                    .await,
                Err(GoalContractStoreFailure::InvalidStoredData)
            );
            assert_eq!(
                load_pending(&store, &fixture, goal.task_id())
                    .await?
                    .pending(),
                &source
            );
            for target in [
                "tasks",
                "goal_contract_revisions",
                "goal_original_headers",
                "goal_original_parts",
                "goal_constraint_origins",
            ] {
                assert_eq!(
                    count(&connection, &format!("SELECT count(*) FROM {target}")).await?,
                    0
                );
            }
            connection
                .execute_batch("DROP TRIGGER abort_promotion")
                .await?;
            store
                .promote_goal_intake(&fixture.project, goal.task_id(), version, &goal)
                .await?;
            assert_eq!(
                count(&connection, "SELECT count(*) FROM goal_pending_intakes").await?,
                0
            );
            drop(connection);
            drop(database);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn capture_and_ordinary_create_cannot_bypass_each_other_or_rewrite_original() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"Implement average".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let source = pending(&goal)?;
        let version = store.capture_goal_intake(&fixture.project, &source).await?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        assert!(
            connection
                .execute("UPDATE goal_pending_intakes SET store_version=2", ())
                .await
                .is_err()
        );
        assert!(connection.execute("INSERT INTO tasks(task_id,worktree_id,created_at_unix_millis,current_goal_revision)
            VALUES (?1,?2,10,1)",params![goal.task_id().as_bytes().to_vec(),fixture.project.worktree().id().as_bytes().to_vec()]).await.is_err());
        store
            .promote_goal_intake(&fixture.project, goal.task_id(), version, &goal)
            .await?;
        assert!(connection.execute("INSERT INTO goal_pending_intakes(task_id,worktree_id,original_digest,
            byte_length,original_bytes,created_at_unix_millis,store_version) VALUES (?1,?2,?3,1,?4,10,1)",params![
            goal.task_id().as_bytes().to_vec(),fixture.project.worktree().id().as_bytes().to_vec(),
            vec![0_u8;32],b"x".to_vec()]).await.is_err());
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn inconsistent_pending_and_materialized_states_are_never_reported_as_ready() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"Implement average".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        store
            .capture_goal_intake(&fixture.project, &pending(&goal)?)
            .await?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        // Deliberately corrupt only this isolated fixture to exercise the closed reader.
        connection
            .execute_batch("DROP TRIGGER tasks_pending_intake_insert_guard")
            .await?;
        let tx = connection
            .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
            .await?;
        tx.execute(
            "INSERT INTO tasks(task_id,worktree_id,created_at_unix_millis,current_goal_revision)
            VALUES (?1,?2,10,1)",
            params![
                goal.task_id().as_bytes().to_vec(),
                fixture.project.worktree().id().as_bytes().to_vec()
            ],
        )
        .await?;
        tx.execute(
            "INSERT INTO goal_contract_revisions(task_id,revision,objective,success_verification,
            created_at_unix_millis) VALUES (?1,1,'fixture goal','verify',10)",
            params![goal.task_id().as_bytes().to_vec()],
        )
        .await?;
        tx.commit().await?;
        let result = crate::goal_contract_repository::intake::load(
            &connection,
            fixture.project.worktree().id(),
            goal.task_id(),
        )
        .await;
        assert!(matches!(
            result,
            Err(crate::goal_contract_repository::GoalContractRepositoryError::InvalidStoredData)
        ));
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn pending_decoder_bounds_raw_bytes_and_revalidates_digest_utf8_time_and_version() -> TestResult {
    crate::run_native_libsql_test(async {
        for corruption in [
            "large_bytes",
            "large_digest",
            "digest",
            "utf8",
            "length",
            "time",
            "version",
        ] {
            let fixture = Fixture::new()?;
            let goal = complete_goal(b"Implement average".to_vec(), 10)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            store
                .capture_goal_intake(&fixture.project, &pending(&goal)?)
                .await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute_batch(
                    "DROP TRIGGER goal_pending_intakes_update_guard;
                PRAGMA ignore_check_constraints=ON;",
                )
                .await?;
            let sql = match corruption {
                "large_bytes" => {
                    "UPDATE goal_pending_intakes SET original_bytes=zeroblob(8388608),byte_length=8388608"
                }
                "large_digest" => {
                    "UPDATE goal_pending_intakes SET original_digest=zeroblob(8388608)"
                }
                "digest" => "UPDATE goal_pending_intakes SET original_digest=zeroblob(32)",
                "utf8" => "UPDATE goal_pending_intakes SET original_bytes=x'ff',byte_length=1",
                "length" => "UPDATE goal_pending_intakes SET byte_length=1",
                "time" => "UPDATE goal_pending_intakes SET created_at_unix_millis=-1",
                _ => "UPDATE goal_pending_intakes SET store_version=0",
            };
            connection.execute(sql, ()).await?;
            let result = crate::goal_contract_repository::intake::load(
                &connection,
                fixture.project.worktree().id(),
                goal.task_id(),
            )
            .await;
            if corruption == "large_bytes" {
                assert!(matches!(
                    result,
                    Err(
                        crate::goal_contract_repository::GoalContractRepositoryError::ResourceLimit
                    )
                ));
            } else {
                assert!(matches!(result,Err(crate::goal_contract_repository::GoalContractRepositoryError::InvalidStoredData)),"{corruption}");
            }
            drop(connection);
            drop(database);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn original_input_methods_explicitly_reject_ordinary_production_schema() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = complete_goal(b"Implement average".to_vec(), 10)?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        assert_eq!(
            store
                .list_pending_goal_intakes(&fixture.project, None)
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            store
                .capture_goal_intake(&fixture.project, &pending(&goal)?)
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            store
                .load_goal_intake(&fixture.project, goal.task_id())
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            store
                .promote_goal_intake(
                    &fixture.project,
                    goal.task_id(),
                    GoalIntakeStoreVersion::INITIAL,
                    &goal
                )
                .await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .is_none()
        );
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}
