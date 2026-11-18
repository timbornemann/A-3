//! Fixed model-plan operation inputs through native persistence, not original-user facts.

use super::*;
use a3_application::StoredTaskLedger;
use a3_domain::{PlannedFileAddition, TaskStepStatus};

fn addition_step(path: &RepositoryPath) -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([44; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "A caption that is not parsed for operation or path".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Retain the selected model operation independently of display".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Only the exact selected path changes".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([45; 32]),
            VerificationRequirement::try_from_string(
                "Observe the selected path change".to_owned(),
            )?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?,
        ),
    )?
    .with_criterion_coverage(vec![AcceptanceCriterionId::from_bytes([3; 32])], Vec::new())?
    .with_planned_file_addition(PlannedFileAddition::new(path.clone()))?)
}

#[test]
fn selected_addition_reopens_with_exact_target_and_survives_cas_without_proving_runtime()
-> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = role_goal()?;
        let target = RepositoryPath::try_from_bytes("src/überblick.py".as_bytes().to_vec())?;
        let definition = addition_step(&target)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let prepared = MaterializeOriginalTask::new(&store, &store)
            .execute(&fixture.project, &goal, vec![definition.clone()])
            .await?;
        assert_eq!(prepared.unplanned_criteria().len(), 2);
        let initial = prepared.stored_ledger().clone();
        drop(store);
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let loaded = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(loaded, initial);
        let step = loaded
            .ledger()
            .step(definition.id())
            .ok_or("step missing")?;
        assert_eq!(step.status(), TaskStepStatus::Ready);
        assert_eq!(step.definition(), &definition);
        assert_eq!(
            step.definition()
                .planned_file_addition()
                .ok_or("selected Add lost")?
                .target(),
            &target
        );
        assert!(step.definition().acceptance_criteria().is_empty());
        let mut started = loaded.ledger().clone();
        started.start_step(
            definition.id(),
            AgentRunId::from_bytes([61; 32]),
            TaskLedgerTimestamp::from_unix_millis(11)?,
        )?;
        let next = store
            .replace_task_ledger(&fixture.project, loaded.version(), &started)
            .await?;
        assert_eq!(
            store
                .replace_task_ledger(&fixture.project, loaded.version(), loaded.ledger())
                .await,
            Err(TaskLedgerStoreFailure::VersionConflict)
        );
        drop(store);
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let recovered = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("active ledger missing")?;
        assert_eq!(recovered.version(), next);
        assert_eq!(recovered.ledger(), &started);
        assert_eq!(
            recovered
                .ledger()
                .step(definition.id())
                .ok_or("active step missing")?
                .definition()
                .planned_file_addition()
                .ok_or("active Add lost")?
                .target(),
            &target
        );
        assert_eq!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?,
            Some(goal)
        );
        drop(store);
        fixture.cleanup()
    })
}

#[test]
fn addition_scope_insert_failure_rolls_back_create_and_replacement_atomically() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = role_goal()?;
        let target = RepositoryPath::try_from_bytes(b"new.py".to_vec())?;
        let definition = addition_step(&target)?;
        let ledger = TaskLedger::new(
            goal.reference(),
            vec![definition.clone()],
            TaskLedgerTimestamp::from_unix_millis(10)?,
        )?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        let trigger =
            "CREATE TRIGGER abort_add_scope BEFORE INSERT ON task_step_planned_file_additions
            BEGIN SELECT RAISE(ABORT,'fixture scope failure'); END;";
        connection.execute_batch(trigger).await?;
        assert_eq!(
            store.create_task_ledger(&fixture.project, &ledger).await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        for table in [
            "task_ledgers",
            "task_steps",
            "verification_specs_v1",
            "task_step_planned_file_additions",
        ] {
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
        }
        connection
            .execute_batch("DROP TRIGGER abort_add_scope")
            .await?;
        let version = store.create_task_ledger(&fixture.project, &ledger).await?;
        let mut started = ledger.clone();
        started.start_step(
            definition.id(),
            AgentRunId::from_bytes([61; 32]),
            TaskLedgerTimestamp::from_unix_millis(11)?,
        )?;
        connection.execute_batch(trigger).await?;
        assert_eq!(
            store
                .replace_task_ledger(&fixture.project, version, &started)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        assert_eq!(
            store
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?,
            Some(StoredTaskLedger::new(ledger.clone(), version))
        );
        connection
            .execute_batch("DROP TRIGGER abort_add_scope")
            .await?;
        assert!(
            store
                .replace_task_ledger(&fixture.project, version, &started)
                .await?
                > version
        );
        drop((connection, db, store));
        fixture.cleanup()
    })
}

#[test]
fn corrupt_or_partly_missing_addition_scope_is_rejected_before_recovery_or_cas() -> TestResult {
    crate::run_native_libsql_test(async {
        for corruption in 0..5 {
            let fixture = Fixture::new()?;
            let goal = role_goal()?;
            let target = RepositoryPath::try_from_bytes(b"chosen.py".to_vec())?;
            let definition = addition_step(&target)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let initial = MaterializeOriginalTask::new(&store, &store)
                .execute(&fixture.project, &goal, vec![definition])
                .await?
                .stored_ledger()
                .clone();
            let db = fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            match corruption {
                0 => {
                    connection
                        .execute(
                            "UPDATE task_step_planned_file_additions SET target_path=?1",
                            params![b"wrong.py".to_vec()],
                        )
                        .await?;
                }
                1 => {
                    connection
                        .execute(
                            "UPDATE task_step_planned_file_additions SET target_path=?1",
                            params![vec![0_u8]],
                        )
                        .await?;
                }
                2 => {
                    connection
                        .execute("DELETE FROM task_step_planned_file_additions", ())
                        .await?;
                }
                3 => {
                    connection
                        .execute("UPDATE task_steps SET has_planned_file_addition=0", ())
                        .await?;
                }
                4 => {
                    connection
                        .execute_batch(
                            "PRAGMA ignore_check_constraints=ON;
                    UPDATE task_step_planned_file_additions SET target_path=zeroblob(131073);
                    PRAGMA ignore_check_constraints=OFF;",
                        )
                        .await?;
                }
                _ => return Err("unknown fixed corruption case".into()),
            }
            assert_eq!(
                store
                    .load_task_ledger(&fixture.project, goal.task_id())
                    .await,
                Err(TaskLedgerStoreFailure::InvalidStoredData)
            );
            assert_eq!(
                store
                    .replace_task_ledger(&fixture.project, initial.version(), initial.ledger())
                    .await,
                Err(TaskLedgerStoreFailure::InvalidStoredData)
            );
            assert_eq!(
                count(&connection, "SELECT store_version FROM task_ledgers").await?,
                i64::try_from(initial.version().get())?
            );
            drop((connection, db, store));
            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            // Only case 4 violates a SQL CHECK. Cold open's integrity check
            // rejects it before the bounded Ledger decoder can classify it.
            let expected = if corruption == 4 {
                TaskLedgerStoreFailure::Corrupt
            } else {
                TaskLedgerStoreFailure::InvalidStoredData
            };
            assert_eq!(
                reopened
                    .load_task_ledger(&fixture.project, goal.task_id())
                    .await,
                Err(expected),
                "cold corruption case {corruption}"
            );
            drop(reopened);
            fixture.cleanup()?;
        }
        Ok(())
    })
}
