//! Real Goal-transaction compatibility and immutable optional-provenance controls.
use super::*;
use a3_domain::{GoalOriginalCoverage, NonGoal, UserDecision};

const TABLES: [&str; 3] = [
    "goal_constraint_origins",
    "goal_non_goal_origins",
    "goal_user_decision_origins",
];

fn annotated_goal(bind: bool) -> TestResult<GoalContract> {
    let template = source_goal(small_original(), test_requirement()?)?;
    let lines = [
        "R01: repair average\n",
        "Constraint: preserve ünicode\n",
        "NonGoal: avoid a broad rewrite\n",
        "Decision: keep the current API\n",
    ];
    let original =
        GoalOriginalRequest::try_from_bytes(template.reference(), lines.concat().into_bytes())?;
    let mut offset = 0;
    let mut ranges = Vec::new();
    for line in lines {
        ranges.push(original.range(offset, offset + line.len())?);
        offset += line.len();
    }
    let criterion = template.draft().acceptance_criteria()[0]
        .clone()
        .with_origin(CriterionOrigin::new(ranges[0], test_requirement()?));
    let constraint = GoalConstraint::try_from_string("preserve ünicode".to_owned())?;
    let non_goal = NonGoal::try_from_string("avoid a broad rewrite".to_owned())?;
    let decision = UserDecision::try_from_string("keep the current API".to_owned())?;
    let draft = GoalContractDraft::new(
        template.draft().objective().clone(),
        vec![criterion],
        vec![if bind {
            constraint.with_origin(ranges[1])
        } else {
            constraint
        }],
        vec![if bind {
            non_goal.with_origin(ranges[2])
        } else {
            non_goal
        }],
        vec![if bind {
            decision.with_origin(ranges[3])
        } else {
            decision
        }],
        template.draft().success_verification().clone(),
    )?;
    Ok(
        GoalContract::initial(template.task_id(), draft, template.created_at())
            .with_original(original)?,
    )
}

async fn database44(fixture: &Fixture) -> TestResult<KnowledgeDatabase> {
    let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
    let database = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
    assert_eq!(
        database.schema_version().get(),
        crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION
    );
    assert_eq!(KnowledgeSchemaVersion::CURRENT.get(), 42);
    Ok(database)
}

#[test]
fn source_owner_only_revision_is_rejected_by_the_store_and_preserves_history() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let initial = annotated_goal(true)?;
        let next = initial.next_reference()?;
        let original = GoalOriginalRequest::try_from_bytes(
            next,
            initial
                .original()
                .ok_or("initial original missing")?
                .as_bytes()
                .to_vec(),
        )?;
        let rebind = |range: a3_domain::GoalOriginalRange| {
            original.range(range.start_byte() as usize, range.end_byte() as usize)
        };
        let criteria = initial
            .draft()
            .acceptance_criteria()
            .iter()
            .map(|criterion| {
                let origin = criterion.origin().ok_or("criterion origin missing")?;
                Ok(criterion.clone().with_origin(CriterionOrigin::new(
                    rebind(origin.original_range())?,
                    origin.evidence_requirement().clone(),
                )))
            })
            .collect::<TestResult<Vec<_>>>()?;
        let constraints = initial
            .draft()
            .constraints()
            .iter()
            .map(|item| {
                Ok(item
                    .clone()
                    .with_origin(rebind(item.origin().ok_or("constraint origin missing")?)?))
            })
            .collect::<TestResult<Vec<_>>>()?;
        let non_goals = initial
            .draft()
            .non_goals()
            .iter()
            .map(|item| {
                Ok(item
                    .clone()
                    .with_origin(rebind(item.origin().ok_or("non-goal origin missing")?)?))
            })
            .collect::<TestResult<Vec<_>>>()?;
        let decisions = initial
            .draft()
            .user_decisions()
            .iter()
            .map(|item| {
                Ok(item
                    .clone()
                    .with_origin(rebind(item.origin().ok_or("decision origin missing")?)?))
            })
            .collect::<TestResult<Vec<_>>>()?;
        let draft = GoalContractDraft::new(
            initial.draft().objective().clone(),
            criteria,
            constraints,
            non_goals,
            decisions,
            initial.draft().success_verification().clone(),
        )?;
        let owner_only = GoalContract::reconstruct(
            initial.task_id(),
            next.revision(),
            Some(initial.revision()),
            Some(GoalRevisionReason::try_from_string(
                "Attempt to count only a new owner as a material change".to_owned(),
            )?),
            draft,
            GoalContractTimestamp::from_unix_millis(11)?,
        )?
        .with_original(original)?;
        assert_eq!(owner_only.validate_original_binding(), Ok(()));
        assert_eq!(
            owner_only.validate_successor_of(&initial),
            Err(a3_domain::GoalContractHistoryError::NoMaterialChange)
        );
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &initial)
            .await?;
        assert_eq!(
            store
                .append_goal_contract_revision(&fixture.project, &owner_only)
                .await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_current_goal_contract(&fixture.project, initial.task_id())
                .await?,
            Some(initial)
        );
        let database = database44(&fixture).await?;
        assert_eq!(
            count(
                database.connection(),
                "SELECT count(*) FROM goal_contract_revisions"
            )
            .await?,
            1
        );
        assert_eq!(
            count(
                database.connection(),
                "SELECT count(*) FROM goal_original_headers"
            )
            .await?,
            1
        );
        for table in TABLES {
            assert_eq!(
                count(
                    database.connection(),
                    &format!("SELECT count(*) FROM {table}")
                )
                .await?,
                1
            );
        }
        drop(database);
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn all_three_entry_origins_survive_actual_goal_store_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = annotated_goal(true)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let restored = reopened
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("goal missing")?;
        assert_eq!(restored, goal);
        assert!(GoalOriginalCoverage::derive_from(&restored)?.is_complete());
        assert!(
            restored
                .original()
                .ok_or("original missing")?
                .resolve(
                    restored.draft().constraints()[0]
                        .origin()
                        .ok_or("origin missing")?
                )?
                .contains('ü')
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn entry_origins_cannot_be_rebound_deleted_or_late_inserted() -> TestResult {
    crate::run_native_libsql_test(async {
        for bind in [false, true] {
            let fixture = Fixture::new()?;
            let goal = annotated_goal(bind)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            CreateGoalContract::new(&store)
                .execute(&fixture.project, &goal)
                .await?;
            let database = database44(&fixture).await?;
            let connection = database.connection_for_operation().await?;
            for table in TABLES {
                if bind {
                    assert!(
                        connection
                            .execute(&format!("UPDATE {table} SET start_byte=0"), ())
                            .await
                            .is_err()
                    );
                    assert!(
                        connection
                            .execute(&format!("DELETE FROM {table}"), ())
                            .await
                            .is_err()
                    );
                } else {
                    let sql = format!("INSERT INTO {table}
                        (task_id,goal_revision,item_sequence,original_digest,start_byte,end_byte)
                        SELECT task_id,goal_revision,1,original_digest,0,1 FROM goal_original_headers");
                    assert!(connection.execute(&sql, ()).await.is_err());
                }
            }
            assert_eq!(
                store
                    .load_current_goal_contract(&fixture.project, goal.task_id())
                    .await?,
                Some(goal)
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
fn deferred_original_header_is_required_at_real_commit() -> TestResult {
    crate::run_native_libsql_test(async {
        for (table, wrong_header) in TABLES
            .into_iter()
            .flat_map(|table| [false, true].map(|wrong_header| (table, wrong_header)))
        {
            let fixture = Fixture::new()?;
            let goal = annotated_goal(true)?;
            let database = database44(&fixture).await?;
            let connection = database.connection_for_operation().await?;
            let tx = connection
                .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
                .await?;
            tx.execute("INSERT INTO tasks(task_id,worktree_id,created_at_unix_millis,current_goal_revision)
                VALUES (?1,?2,10,1)",params![goal.task_id().as_bytes().to_vec(),
                fixture.project.worktree().id().as_bytes().to_vec()]).await?;
            tx.execute(
                "INSERT INTO goal_contract_revisions(task_id,revision,objective,
                success_verification,created_at_unix_millis,original_contract_version)
                VALUES (?1,1,'objective','verify',10,1)",
                params![goal.task_id().as_bytes().to_vec()],
            )
            .await?;
            let parent = match table {
                "goal_constraint_origins" => "goal_contract_constraints",
                "goal_non_goal_origins" => "goal_contract_non_goals",
                _ => "goal_contract_user_decisions",
            };
            tx.execute(
                &format!(
                    "INSERT INTO {parent}(task_id,goal_revision,item_sequence,statement)
                VALUES (?1,1,1,'entry')"
                ),
                params![goal.task_id().as_bytes().to_vec()],
            )
            .await?;
            // The deferred relation permits pre-header insertion, but never a durable orphan.
            tx.execute(
                &format!(
                    "INSERT INTO {table}(task_id,goal_revision,item_sequence,
                original_digest,start_byte,end_byte) VALUES (?1,1,1,?2,0,1)"
                ),
                params![goal.task_id().as_bytes().to_vec(), vec![9_u8; 32]],
            )
            .await?;
            if wrong_header {
                tx.execute(
                    "INSERT INTO goal_original_headers(task_id,goal_revision,original_digest,
                    byte_length,part_count) VALUES (?1,1,?2,1,1)",
                    params![goal.task_id().as_bytes().to_vec(), vec![8_u8; 32]],
                )
                .await?;
            }
            assert!(tx.commit().await.is_err());
            assert_eq!(count(&connection, "SELECT count(*) FROM tasks").await?, 0);
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
            drop(connection);
            drop(database);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn later_original_failure_rolls_back_all_entry_origins_in_same_goal_transaction() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = annotated_goal(true)?;
        let database = database44(&fixture).await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "CREATE TRIGGER fail_original_part BEFORE INSERT ON goal_original_parts
            BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            )
            .await?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store.create_goal_contract(&fixture.project, &goal).await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        for table in [
            "tasks",
            "goal_contract_revisions",
            "goal_original_headers",
            "goal_constraint_origins",
            "goal_non_goal_origins",
            "goal_user_decision_origins",
        ] {
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
        }
        connection
            .execute_batch("DROP TRIGGER fail_original_part")
            .await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn closed_entry_decoder_rejects_corrupt_digest_unicode_and_ordinal_without_large_blob_read()
-> TestResult {
    crate::run_native_libsql_test(async {
        for table in TABLES {
            for corruption in ["digest", "unicode", "ordinal", "large_digest"] {
                let fixture = Fixture::new()?;
                let goal = annotated_goal(true)?;
                let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                CreateGoalContract::new(&store)
                    .execute(&fixture.project, &goal)
                    .await?;
                let database = database44(&fixture).await?;
                let connection = database.connection_for_operation().await?;
                // Isolated corruption fixture only: bypass SQL guards to exercise the decoder itself.
                connection
                    .execute_batch(&format!(
                        "DROP TRIGGER {table}_update_guard;
                    PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON;"
                    ))
                    .await?;
                let sql = match corruption {
                    "digest" => format!("UPDATE {table} SET original_digest=zeroblob(32)"),
                    "large_digest" => {
                        format!("UPDATE {table} SET original_digest=zeroblob(8388608)")
                    }
                    "ordinal" => format!("UPDATE {table} SET item_sequence=65"),
                    _ => {
                        let middle = goal
                            .original()
                            .ok_or("source missing")?
                            .as_str()
                            .find('ü')
                            .ok_or("unicode missing")?
                            + 1;
                        format!(
                            "UPDATE {table} SET start_byte={middle},end_byte={}",
                            middle + 1
                        )
                    }
                };
                connection.execute(&sql, ()).await?;
                let result = crate::goal_contract_repository::load_current(
                    &connection,
                    fixture.project.worktree().id(),
                    goal.task_id(),
                )
                .await;
                assert!(matches!(result,Err(crate::goal_contract_repository::GoalContractRepositoryError::InvalidStoredData)),
                    "corruption {corruption} on {table}");
                drop(connection);
                drop(database);
                drop(store);
                fixture.cleanup()?;
            }
        }
        Ok(())
    })
}

#[test]
fn legacy_revision_rejects_orphans_from_each_optional_origin_table() -> TestResult {
    crate::run_native_libsql_test(async {
        for table in TABLES {
            let fixture = Fixture::new()?;
            let template = annotated_goal(false)?;
            let draft = GoalContractDraft::new(
                template.draft().objective().clone(),
                vec![criterion(2, "legacy outcome")?],
                template.draft().constraints().to_vec(),
                template.draft().non_goals().to_vec(),
                template.draft().user_decisions().to_vec(),
                template.draft().success_verification().clone(),
            )?;
            let goal = GoalContract::initial(template.task_id(), draft, template.created_at());
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            CreateGoalContract::new(&store)
                .execute(&fixture.project, &goal)
                .await?;
            let database = database44(&fixture).await?;
            let connection = database.connection_for_operation().await?;
            // Isolated orphan fixture: no header exists and the real reader must not ignore this row.
            connection.execute_batch("PRAGMA foreign_keys=OFF").await?;
            connection
                .execute(
                    &format!(
                        "INSERT INTO {table}(task_id,goal_revision,item_sequence,
                original_digest,start_byte,end_byte) VALUES (?1,1,1,?2,0,1)"
                    ),
                    params![goal.task_id().as_bytes().to_vec(), vec![9_u8; 32]],
                )
                .await?;
            assert!(matches!(
                crate::goal_contract_repository::load_current(
                    &connection,
                    fixture.project.worktree().id(),
                    goal.task_id()
                )
                .await,
                Err(
                    crate::goal_contract_repository::GoalContractRepositoryError::InvalidStoredData
                )
            ));
            drop(connection);
            drop(database);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}
