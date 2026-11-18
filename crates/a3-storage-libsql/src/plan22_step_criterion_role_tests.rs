//! Real role persistence controls. Fixed proposals are inputs, not model interpretation or proof.
use super::*;
use a3_application::TaskLedgerStoreFailure;
use a3_domain::{
    AgentRunId, GoalContractReference, TaskLedger, TaskLedgerTimestamp, TaskReplanReason,
};

#[path = "plan22_planned_addition_tests.rs"]
mod planned_addition;

fn role_goal() -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"R01: fractional average\nR02: working endpoint\n".to_vec(),
    )?;
    let split = original.as_str().find('\n').ok_or("line missing")? + 1;
    let first = criterion(2, "The average preserves fractions")?.with_origin(CriterionOrigin::new(
        original.range(0, split)?,
        test_requirement()?,
    ));
    let second = criterion(3, "The endpoint has working runtime behavior")?.with_origin(
        CriterionOrigin::new(
            original.range(split, original.as_bytes().len())?,
            CriterionEvidenceRequirement::RuntimeNeedsRecipe,
        ),
    );
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Implement the requested behavior".to_owned())?,
        vec![first, second],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Current appropriate result evidence".to_owned())?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(10)?)
            .with_original(original)?,
    )
}

fn role_step(id: u8, spec: u8, addresses_only: bool) -> TestResult<TaskStepDefinition> {
    let verification = if addresses_only {
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([spec; 32]),
            VerificationRequirement::try_from_string(
                "Only the intended scaffold path changes".to_owned(),
            )?,
            DiffInvariantVerification::new(
                DiffInvariantMode::OnlyPaths,
                vec![RepositoryPath::try_from_bytes(b"average.py".to_vec())?],
            )?,
        )
    } else {
        test_spec()?.reidentified(VerificationSpecId::from_bytes([spec; 32]))
    };
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([id; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Produce a bounded part of the implementation".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Runtime coverage remains distinct from step progress".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "The selected operational evidence".to_owned(),
        )?],
        verification,
    )?
    .with_criterion_coverage(
        vec![AcceptanceCriterionId::from_bytes([3; 32])],
        if addresses_only {
            Vec::new()
        } else {
            vec![AcceptanceCriterionId::from_bytes([2; 32])]
        },
    )?)
}

#[test]
fn role_reopen_keeps_addresses_only_progress_out_of_unplanned_proof_coverage() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = role_goal()?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let prepared = MaterializeOriginalTask::new(&store, &store)
            .execute(
                &fixture.project,
                &goal,
                vec![role_step(43, 42, false)?, role_step(44, 45, true)?],
            )
            .await?;
        assert_eq!(
            prepared.unplanned_criteria(),
            &[AcceptanceCriterionId::from_bytes([3; 32])]
        );
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let loaded = reopened
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(loaded, prepared.stored_ledger().clone());
        let mixed = loaded
            .ledger()
            .step(TaskStepId::from_bytes([43; 32]))
            .ok_or("mixed step missing")?
            .definition();
        assert_eq!(
            mixed.addressed_criteria(),
            &[AcceptanceCriterionId::from_bytes([3; 32])]
        );
        assert_eq!(
            mixed.acceptance_criteria(),
            &[AcceptanceCriterionId::from_bytes([2; 32])]
        );
        let scaffold = loaded
            .ledger()
            .step(TaskStepId::from_bytes([44; 32]))
            .ok_or("scaffold missing")?
            .definition();
        assert_eq!(
            scaffold.addressed_criteria(),
            &[AcceptanceCriterionId::from_bytes([3; 32])]
        );
        assert!(scaffold.acceptance_criteria().is_empty());
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn role_insert_failure_rolls_back_create_and_replacement_without_advancing_cas() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = role_goal()?;
        let ledger = TaskLedger::new(
            goal.reference(),
            vec![role_step(43, 42, false)?],
            TaskLedgerTimestamp::from_unix_millis(10)?,
        )?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "CREATE TRIGGER abort_roles BEFORE INSERT ON task_step_addressed_criteria
            BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            )
            .await?;
        assert_eq!(
            store.create_task_ledger(&fixture.project, &ledger).await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        for table in [
            "task_ledgers",
            "task_steps",
            "task_step_acceptance_criteria",
            "task_step_addressed_criteria",
        ] {
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
        }
        connection.execute_batch("DROP TRIGGER abort_roles").await?;
        let version = store.create_task_ledger(&fixture.project, &ledger).await?;
        let initial = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        let mut changed = ledger.clone();
        changed.start_step(
            TaskStepId::from_bytes([43; 32]),
            AgentRunId::from_bytes([61; 32]),
            TaskLedgerTimestamp::from_unix_millis(11)?,
        )?;
        connection
            .execute_batch(
                "CREATE TRIGGER abort_roles BEFORE INSERT ON task_step_addressed_criteria
            BEGIN SELECT RAISE(ABORT,'fixture failure'); END;",
            )
            .await?;
        assert_eq!(
            store
                .replace_task_ledger(&fixture.project, version, &changed)
                .await,
            Err(TaskLedgerStoreFailure::InvalidStoredData)
        );
        assert_eq!(
            store
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?,
            Some(initial.clone())
        );
        connection.execute_batch("DROP TRIGGER abort_roles").await?;
        let next = store
            .replace_task_ledger(&fixture.project, version, &changed)
            .await?;
        assert_eq!(
            store
                .replace_task_ledger(&fixture.project, version, &ledger)
                .await,
            Err(TaskLedgerStoreFailure::VersionConflict)
        );
        assert_eq!(
            store
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?
                .ok_or("ledger missing")?
                .version(),
            next
        );
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
        Ok(())
    })
}

#[test]
fn sql_roles_reject_overlap_in_both_directions_and_foreign_criterion_or_goal() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = role_goal()?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let prepared = MaterializeOriginalTask::new(&store, &store)
            .execute(&fixture.project, &goal, vec![role_step(43, 42, false)?])
            .await?;
        let foreign = GoalContract::initial(
            TaskId::from_bytes([100; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("Another task".to_owned())?,
                vec![criterion(99, "Foreign task criterion")?],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("Verify independently".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(10)?,
        );
        store
            .create_goal_contract(&fixture.project, &foreign)
            .await?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        for sql in [
            "INSERT INTO task_step_addressed_criteria(task_id,step_id,goal_revision,criterion_id)
                SELECT task_id,step_id,goal_revision,criterion_id FROM task_step_acceptance_criteria",
            "INSERT INTO task_step_acceptance_criteria(task_id,step_id,goal_revision,criterion_id)
                SELECT task_id,step_id,goal_revision,criterion_id FROM task_step_addressed_criteria",
            "UPDATE task_step_addressed_criteria SET criterion_id=(SELECT criterion_id FROM task_step_acceptance_criteria)",
            "UPDATE task_step_acceptance_criteria SET criterion_id=(SELECT criterion_id FROM task_step_addressed_criteria)",
        ] {assert!(connection.execute(sql,()).await.is_err());}
        assert!(connection.execute("INSERT INTO task_step_addressed_criteria(task_id,step_id,goal_revision,criterion_id)
            VALUES (?1,?2,1,?3)",params![goal.task_id().as_bytes().to_vec(),TaskStepId::from_bytes([43;32]).as_bytes().to_vec(),
            AcceptanceCriterionId::from_bytes([99;32]).as_bytes().to_vec()]).await.is_err());
        // UPDATE isolates the goal FK: the existing PK stays unchanged, so a duplicate key cannot explain rejection.
        assert!(
            connection
                .execute(
                    "UPDATE task_step_addressed_criteria SET goal_revision=2",
                    ()
                )
                .await
                .is_err()
        );
        assert_eq!(
            store
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?,
            Some(prepared.stored_ledger().clone())
        );
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn role_decoder_rejects_overlap_foreign_scope_and_large_ids_after_external_corruption() -> TestResult
{
    crate::run_native_libsql_test(async {
        for (table, corruption) in [
            ("task_step_addressed_criteria", "overlap"),
            ("task_step_addressed_criteria", "foreign"),
            ("task_step_addressed_criteria", "goal_revision"),
            ("task_step_addressed_criteria", "large_id"),
            ("task_step_acceptance_criteria", "foreign"),
            ("task_step_acceptance_criteria", "goal_revision"),
            ("task_step_acceptance_criteria", "large_id"),
        ] {
            let fixture = Fixture::new()?;
            let goal = role_goal()?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            MaterializeOriginalTask::new(&store, &store)
                .execute(&fixture.project, &goal, vec![role_step(43, 42, false)?])
                .await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            // Isolated corrupt fixture only; the production reader must enforce the contract itself.
            connection
                .execute_batch(
                    "DROP TRIGGER step_addresses_disjoint_update;
                PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON;",
                )
                .await?;
            let sql = match corruption {
                "overlap" => {
                    "UPDATE task_step_addressed_criteria SET criterion_id=(SELECT criterion_id FROM task_step_acceptance_criteria)"
                }
                "foreign" => "UPDATE task_step_addressed_criteria SET criterion_id=zeroblob(32)",
                "goal_revision" => "UPDATE task_step_addressed_criteria SET goal_revision=2",
                _ => "UPDATE task_step_addressed_criteria SET criterion_id=zeroblob(8388608)",
            };
            let sql = sql.replace("task_step_addressed_criteria", table);
            connection.execute(&sql, ()).await?;
            let result = crate::task_ledger_repository::load(
                &connection,
                fixture.project.worktree().id(),
                goal.task_id(),
            )
            .await;
            assert!(
                matches!(
                    result,
                    Err(
                        crate::task_ledger_repository::TaskLedgerRepositoryError::InvalidStoredData
                    )
                ),
                "{corruption}"
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
fn production_writer_rejects_addresses_before_create_or_replace_write() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            plain_draft("Legacy port fixture")?,
            GoalContractTimestamp::from_unix_millis(10)?,
        );
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        store.create_goal_contract(&fixture.project, &goal).await?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let database = KnowledgeDatabase::open(&layout, &fixture.project).await?;
        let connection = database.connection_for_operation().await?;
        let addressed = TaskLedger::new(
            goal.reference(),
            vec![role_step(43, 42, false)?],
            TaskLedgerTimestamp::from_unix_millis(10)?,
        )?;
        connection
            .execute_batch(
                "CREATE TRIGGER forbid_ledger_insert BEFORE INSERT ON task_ledgers
            BEGIN SELECT RAISE(ABORT,'unexpected partial write'); END;",
            )
            .await?;
        assert_eq!(
            store.create_task_ledger(&fixture.project, &addressed).await,
            Err(TaskLedgerStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            count(&connection, "SELECT count(*) FROM task_ledgers").await?,
            0
        );
        connection
            .execute_batch("DROP TRIGGER forbid_ledger_insert")
            .await?;
        let original = TaskLedger::new(
            goal.reference(),
            vec![step(test_spec()?)?],
            TaskLedgerTimestamp::from_unix_millis(10)?,
        )?;
        let version = store
            .create_task_ledger(&fixture.project, &original)
            .await?;
        let mut revised = original.clone();
        revised.replan(
            vec![TaskStepId::from_bytes([43; 32])],
            vec![role_step(44, 45, false)?],
            TaskReplanReason::try_from_string("A role-bearing replacement".to_owned())?,
            TaskLedgerTimestamp::from_unix_millis(11)?,
        )?;
        connection
            .execute_batch(
                "CREATE TRIGGER forbid_ledger_update BEFORE UPDATE ON task_ledgers
            BEGIN SELECT RAISE(ABORT,'unexpected partial write'); END;",
            )
            .await?;
        assert_eq!(
            store
                .replace_task_ledger(&fixture.project, version, &revised)
                .await,
            Err(TaskLedgerStoreFailure::UnsupportedSchema)
        );
        let unchanged = store
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(unchanged.version(), version);
        assert_eq!(unchanged.ledger(), &original);
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn candidate_roles_pass_the_additional_shared_port_contract() -> TestResult {
    crate::run_native_libsql_test(async {
        a3_storage_contract_tests::verify_task_step_criterion_roles_contract(
            &CandidateContractFactory,
        )
        .await
        .map_err(|error| error as Box<dyn Error>)
    })
}
