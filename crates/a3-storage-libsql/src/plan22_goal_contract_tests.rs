//! Real, isolated storage integration. Intake proposals are fixture inputs, not model results.

#[path = "plan22_goal_acceptance_source_tests.rs"]
mod acceptance_source_tests;

#[path = "plan22_goal_annotation_tests.rs"]
mod annotation_tests;

#[path = "plan22_goal_intake_tests.rs"]
mod intake_tests;

#[path = "plan22_goal_intake_review_tests.rs"]
mod intake_review_tests;

#[path = "plan22_step_criterion_role_tests.rs"]
mod criterion_role_tests;

#[path = "plan22_horizon_checkpoint_tests.rs"]
mod horizon_checkpoint_tests;

#[path = "plan22_existing_test_tests.rs"]
mod existing_test_tests;

#[path = "plan22_fixture_root.rs"]
mod fixture_root;

use crate::agent_session_repository;
use crate::{KnowledgeDatabase, KnowledgeSchemaVersion, LibsqlKnowledgeStore, StorageLayout};
use a3_application::{
    CreateGoalContract, GoalContractStore, GoalContractStoreFailure, MaterializeOriginalTask,
    MaterializeOriginalTaskFailure, TaskLedgerStore, validate_agent_session_transition,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentSession,
    AgentSessionEntry, AgentSessionEntryKind, AgentSessionId, AgentSessionMode,
    AgentSessionRevision, AgentSessionSequence, AgentSessionState, AgentSessionText,
    AgentSessionTimestamp, AgentSessionTitle, AgentWorkItem, AgentWorkItemId, CanonicalDirectory,
    CriterionEvidenceRequirement, CriterionOrigin, DiffInvariantMode, DiffInvariantVerification,
    DiscoveredCommandId, ExpectedTaskEvidence, GitHead, GitReferenceName, GoalConstraint,
    GoalContract, GoalContractDraft, GoalContractReference, GoalContractTimestamp, GoalObjective,
    GoalOriginalCoverage, GoalOriginalRequest, GoalRevisionReason, MinimumTestCaseCount,
    ProjectIdentity, RepositoryId, RepositoryIdentity, RepositoryPath, SuccessVerification, TaskId,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, TestCaseSelector,
    TestCaseSelectorName, VerificationRequirement, VerificationScope, VerificationSpec,
    VerificationSpecId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use libsql::{Connection, params};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[test]
fn exact_64k_original_and_late_constraint_survive_presentation_delete_and_full_reopen() -> TestResult
{
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let bytes = large_original_ending_with(
            "\r\nR30: implement a later option\r\nDo not modify foreign files\r\n  ",
        );
        let goal = source_goal_with_late_constraint(bytes.clone())?;
        let original = goal.original().ok_or("fixture original missing")?;
        assert_eq!(original.as_bytes().len(), 65_536);
        assert_eq!(original.parts().len(), 33);
        let late = goal.draft().acceptance_criteria()[1]
            .origin()
            .ok_or("fixture late origin missing")?
            .original_range();
        assert!(late.start_byte() > 16_384);
        let constraint = &goal.draft().constraints()[0];
        let constraint_origin = constraint
            .origin()
            .ok_or("late constraint origin missing")?;
        assert!(constraint_origin.start_byte() > 16_384);
        assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete());

        let first = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let prepared = MaterializeOriginalTask::new(&first, &first)
            .execute(&fixture.project, &goal, vec![step(test_spec()?)?])
            .await?;
        assert_eq!(
            prepared.unplanned_criteria(),
            &[AcceptanceCriterionId::from_bytes([3; 32])]
        );
        assert_eq!(prepared.stored_ledger().ledger().revision().get(), 1);
        let preview = prepared.preview();
        assert_eq!(preview.goal, goal.reference());
        assert_eq!(preview.revision.get(), 1);
        assert_eq!(preview.steps.len(), 1);
        let first_step = preview.steps.first().ok_or("preview step missing")?;
        assert_eq!(preview.next_step, Some(first_step.id));
        assert_eq!(first_step.status, a3_domain::TaskStepStatus::Ready);
        assert_eq!(first_step.verification, a3_domain::VerificationMethod::Test);
        assert_eq!(
            preview.unplanned_criteria,
            &[AcceptanceCriterionId::from_bytes([3; 32])]
        );
        drop(first);

        let session_id = delete_original_presentation(&fixture, &goal).await?;

        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let loaded = reopened
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("persisted goal missing")?;
        assert_eq!(loaded, goal);
        let restored = loaded.original().ok_or("restored original missing")?;
        assert!(restored.as_bytes() == bytes);
        assert!(restored.resolve(late)?.starts_with("R30:"));
        assert_eq!(restored.digest(), original.digest());
        assert_eq!(
            loaded.draft().constraints(),
            std::slice::from_ref(constraint)
        );
        assert_eq!(
            restored.resolve(constraint_origin)?,
            "Do not modify foreign files\r\n  "
        );
        assert!(GoalOriginalCoverage::derive_from(&loaded)?.is_complete());
        let repeated = MaterializeOriginalTask::new(&reopened, &reopened)
            .execute(&fixture.project, &loaded, vec![step(test_spec()?)?])
            .await?;
        assert_eq!(
            repeated, prepared,
            "exact reentry must retain the stored ledger/version"
        );
        assert_eq!(repeated.preview(), preview);
        drop(reopened);

        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        assert!(
            agent_session_repository::load(
                &connection,
                fixture.project.worktree().id(),
                session_id,
                None,
                1,
            )
            .await
            .map_err(|error| error.classify())?
            .is_none()
        );
        assert_eq!(
            count(&connection, "SELECT count(*) FROM agent_session_entries").await?,
            0
        );
        drop(connection);
        drop(db);
        fixture.cleanup()?;
        Ok(())
    })
}

async fn delete_original_presentation(
    fixture: &Fixture,
    goal: &GoalContract,
) -> TestResult<AgentSessionId> {
    let db = fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let worktree = fixture.project.worktree().id();
    let session_id = AgentSessionId::from_bytes([201; 32]);
    let work_item = AgentWorkItem::new(
        AgentWorkItemId::from_bytes([202; 32]),
        goal.task_id(),
        AgentSessionMode::Agent,
    );
    let created_at = AgentSessionTimestamp::from_unix_millis(10)?;
    let session = AgentSession::from_parts(
        session_id,
        AgentSessionRevision::INITIAL,
        AgentSessionTitle::try_from_string("Original presentation".to_owned())?,
        AgentSessionMode::Agent,
        AgentSessionState::Completed,
        created_at,
        created_at,
        Some(AgentSessionSequence::FIRST),
        Some(work_item),
        None,
        false,
    );
    let original = goal.original().ok_or("original missing")?;
    let user = AgentSessionEntry::try_new(
        session_id,
        AgentSessionSequence::FIRST,
        AgentSessionEntryKind::UserMessage,
        AgentSessionText::try_from_string(original.as_str().to_owned())?,
        created_at,
        Some(work_item.id()),
        Some(goal.task_id()),
        None,
    )?;
    agent_session_repository::create(&connection, worktree, &session, Some(&user), None)
        .await
        .map_err(|error| error.classify())?;
    let before = agent_session_repository::load(&connection, worktree, session_id, None, 1)
        .await
        .map_err(|error| error.classify())?
        .ok_or("presentation missing before deletion")?;
    assert_eq!(before.session(), &session);
    assert_eq!(before.entries(), &[user]);

    let tombstone = AgentSession::from_parts(
        session_id,
        AgentSessionRevision::new(2)?,
        AgentSessionTitle::try_from_string("Deleted conversation".to_owned())?,
        AgentSessionMode::Agent,
        AgentSessionState::Archived,
        created_at,
        AgentSessionTimestamp::from_unix_millis(11)?,
        None,
        Some(work_item),
        None,
        true,
    );
    validate_agent_session_transition(&session, &tombstone)?;
    agent_session_repository::delete_presentation(
        &connection,
        worktree,
        session_id,
        session.revision(),
        &tombstone,
    )
    .await
    .map_err(|error| error.classify())?;
    Ok(session_id)
}

#[test]
fn original_write_failure_rolls_back_task_header_parts_and_criteria() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = source_goal(large_original(), test_requirement()?)?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "CREATE TRIGGER plan22_original_abort BEFORE INSERT ON goal_original_parts
             WHEN NEW.part_sequence = 2 BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;",
            )
            .await?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store.create_goal_contract(&fixture.project, &goal).await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        assert!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .is_none()
        );
        for table in [
            "tasks",
            "goal_contract_revisions",
            "acceptance_criteria",
            "goal_original_headers",
            "goal_original_parts",
            "goal_criterion_origins",
        ] {
            assert_eq!(
                count(&connection, &format!("SELECT count(*) FROM {table}")).await?,
                0
            );
        }
        connection
            .execute_batch("DROP TRIGGER plan22_original_abort;")
            .await?;
        MaterializeOriginalTask::new(&store, &store)
            .execute(&fixture.project, &goal, vec![step(test_spec()?)?])
            .await?;
        drop(store);
        drop(connection);
        drop(database);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn corrupt_part_bytes_and_unicode_midpoint_ranges_fail_after_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        for corrupt_range in [false, true] {
            let fixture = Fixture::new()?;
            let goal = source_goal(large_original(), test_requirement()?)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            CreateGoalContract::new(&store)
                .execute(&fixture.project, &goal)
                .await?;
            drop(store);
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            if corrupt_range {
                // Simulate externally corrupted data after removing only the test DB's guard.
                connection
                    .execute_batch("DROP TRIGGER goal_criterion_origins_update_guard;")
                    .await?;
                let text = goal.original().ok_or("fixture source missing")?.as_str();
                let start = text.find('\u{1f4a1}').ok_or("fixture unicode missing")? + 1;
                connection
                    .execute(
                        "UPDATE goal_criterion_origins SET start_byte = ?1, end_byte = ?2
                     WHERE criterion_id = ?3",
                        params![
                            i64::try_from(start)?,
                            i64::try_from(start + 1)?,
                            AcceptanceCriterionId::from_bytes([2; 32])
                                .as_bytes()
                                .to_vec()
                        ],
                    )
                    .await?;
            } else {
                connection
                    .execute_batch("DROP TRIGGER goal_original_parts_update_guard;")
                    .await?;
                let original = goal.original().ok_or("fixture source missing")?;
                let mut part = original.resolve(original.parts()[0])?.as_bytes().to_vec();
                part[0] = b'X';
                connection.execute(
                    "UPDATE goal_original_parts SET original_bytes = ?1 WHERE part_sequence = 1",
                    params![part],
                ).await?;
            }
            drop(connection);
            drop(database);
            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            assert_eq!(
                reopened
                    .load_current_goal_contract(&fixture.project, goal.task_id())
                    .await,
                Err(GoalContractStoreFailure::InvalidStoredData)
            );
            drop(reopened);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn missing_source_cannot_be_reinterpreted_as_a_legacy_goal() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = source_goal(small_original(), test_requirement()?)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        drop(store);
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "BEGIN IMMEDIATE;
             DROP TRIGGER goal_criterion_diff_paths_delete_guard;
             DROP TRIGGER goal_criterion_origins_delete_guard;
             DROP TRIGGER goal_original_parts_delete_guard;
             DROP TRIGGER goal_original_headers_delete_guard;
             DELETE FROM goal_criterion_diff_paths;
             DELETE FROM goal_criterion_origins;
             DELETE FROM goal_original_parts;
             DELETE FROM goal_original_headers;
             COMMIT;",
            )
            .await?;
        drop(connection);
        drop(database);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn source_revision_cas_preserves_history_and_rejects_manual_source_drop() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let initial = source_goal(small_original(), test_requirement()?)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &initial)
            .await?;
        let reason = GoalRevisionReason::try_from_string("User clarified R30".to_owned())?;
        let next = initial.next_reference()?;
        let stripped = GoalContract::reconstruct(
            initial.task_id(),
            next.revision(),
            Some(initial.revision()),
            Some(reason.clone()),
            plain_draft("Changed summary")?,
            GoalContractTimestamp::from_unix_millis(11)?,
        )?;
        assert_eq!(
            store
                .append_goal_contract_revision(&fixture.project, &stripped)
                .await,
            Err(GoalContractStoreFailure::InvalidStoredData)
        );
        let accepted = revised_goal(&initial, "R30: first clarification")?;
        let stale = revised_goal(&initial, "R30: competing clarification")?;
        store
            .append_goal_contract_revision(&fixture.project, &accepted)
            .await?;
        assert_eq!(
            store
                .append_goal_contract_revision(&fixture.project, &stale)
                .await,
            Err(GoalContractStoreFailure::RevisionConflict)
        );
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened
                .load_current_goal_contract(&fixture.project, initial.task_id())
                .await?,
            Some(accepted)
        );
        assert_eq!(
            reopened
                .load_goal_contract_revision(
                    &fixture.project,
                    initial.task_id(),
                    initial.revision()
                )
                .await?,
            Some(initial)
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn reopened_test_obligation_rejects_diff_and_change_only_roundtrips_without_runtime_claim()
-> TestResult {
    crate::run_native_libsql_test(async {
        for change_only in [false, true] {
            let fixture = Fixture::new()?;
            let invariant = DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
            )?;
            let requirement = if change_only {
                CriterionEvidenceRequirement::ChangeOnly(invariant.clone())
            } else {
                test_requirement()?
            };
            let bytes = if change_only {
                b"R01: update README documentation\r\nR30: implement a later option\r\n".to_vec()
            } else {
                small_original()
            };
            let goal = source_goal(bytes, requirement)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let spec = if change_only {
                diff_spec(invariant.clone())?
            } else {
                test_spec()?
            };
            let first = MaterializeOriginalTask::new(&store, &store)
                .execute(&fixture.project, &goal, vec![step(spec.clone())?])
                .await?;
            drop(store);
            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let restored = reopened
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .ok_or("stored source missing")?;
            assert_eq!(restored, goal);
            if !change_only {
                let rejected = MaterializeOriginalTask::new(&reopened, &reopened)
                    .execute(
                        &fixture.project,
                        &restored,
                        vec![step(diff_spec(invariant)?)?],
                    )
                    .await;
                assert!(matches!(
                    rejected,
                    Err(MaterializeOriginalTaskFailure::ProofMismatch { .. })
                ));
            }
            assert_eq!(
                reopened
                    .load_task_ledger(&fixture.project, goal.task_id())
                    .await?
                    .as_ref(),
                Some(first.stored_ledger())
            );
            drop(reopened);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn well_formed_proof_weakening_and_late_metadata_inserts_are_blocked() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = source_goal(small_original(), test_requirement()?)?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        CreateGoalContract::new(&store)
            .execute(&fixture.project, &goal)
            .await?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        for sql in [
            "UPDATE goal_criterion_origins SET requirement_kind='change_only', command_id=NULL,
             selector_kind=NULL, selector_name=NULL, minimum_cases=NULL, verification_scope=NULL,
             diff_mode='non_empty_changes', diff_path_count=0 WHERE requirement_kind='existing_test'",
            "UPDATE acceptance_criteria SET requirement='should'",
            "DELETE FROM goal_contract_constraints",
            "UPDATE goal_contract_revisions SET original_contract_version=NULL",
            "INSERT INTO goal_contract_constraints(task_id,goal_revision,item_sequence,statement)
             SELECT task_id,revision,2,'late reinterpretation' FROM goal_contract_revisions",
        ] {
            assert!(connection.execute(sql, ()).await.is_err());
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
        Ok(())
    })
}

#[test]
fn production_store_rejects_an_original_before_any_goal_is_committed() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = source_goal(small_original(), test_requirement()?)?;
        let store = LibsqlKnowledgeStore::open(&fixture.layout).await?;
        assert_eq!(
            store.create_goal_contract(&fixture.project, &goal).await,
            Err(GoalContractStoreFailure::UnsupportedSchema)
        );
        assert!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?
                .is_none()
        );
        let project_layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let database = KnowledgeDatabase::open(&project_layout, &fixture.project).await?;
        assert_eq!(database.schema_version().get(), 42);
        assert_eq!(
            count(database.connection(), "SELECT count(*) FROM tasks").await?,
            0
        );
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn real_ledger_abort_keeps_the_exact_goal_until_explicit_reentry() -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let goal = source_goal(small_original(), test_requirement()?)?;
        let database = fixture.candidate_database().await?;
        let connection = database.connection_for_operation().await?;
        connection
            .execute_batch(
                "CREATE TRIGGER plan22_ledger_abort BEFORE INSERT ON task_ledgers
          BEGIN SELECT RAISE(ABORT,'fixture ledger failure'); END;",
            )
            .await?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let result = MaterializeOriginalTask::new(&store, &store)
            .execute(&fixture.project, &goal, vec![step(test_spec()?)?])
            .await;
        assert!(matches!(
            result,
            Err(MaterializeOriginalTaskFailure::Ledger(_))
        ));
        assert_eq!(
            store
                .load_current_goal_contract(&fixture.project, goal.task_id())
                .await?,
            Some(goal.clone())
        );
        assert!(
            store
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?
                .is_none()
        );
        assert_eq!(
            count(&connection, "SELECT count(*) FROM agent_runs").await?,
            0
        );
        connection
            .execute_batch("DROP TRIGGER plan22_ledger_abort;")
            .await?;
        MaterializeOriginalTask::new(&store, &store)
            .execute(&fixture.project, &goal, vec![step(test_spec()?)?])
            .await?;
        assert_eq!(
            count(&connection, "SELECT count(*) FROM goal_original_headers").await?,
            1
        );
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn orphan_original_rows_are_not_ignored_by_cached_loads() -> TestResult {
    crate::run_native_libsql_test(async {
        for source in [false, true] {
            let fixture = Fixture::new()?;
            let goal = if source {
                source_goal(small_original(), test_requirement()?)?
            } else {
                GoalContract::initial(
                    TaskId::from_bytes([1; 32]),
                    plain_draft("Legacy goal")?,
                    GoalContractTimestamp::from_unix_millis(10)?,
                )
            };
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            CreateGoalContract::new(&store)
                .execute(&fixture.project, &goal)
                .await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            // An external damaged writer is simulated only in this owned test DB.
            connection.execute_batch("PRAGMA foreign_keys=OFF;").await?;
            if source {
                connection
                    .execute_batch("DROP TRIGGER goal_criterion_diff_paths_insert_guard;")
                    .await?;
                connection.execute(
                    "INSERT INTO goal_criterion_diff_paths(task_id,goal_revision,criterion_id,path_sequence,path)
                     VALUES(?1,1,?2,1,?3)",
                    params![goal.task_id().as_bytes().to_vec(), vec![99_u8;32], b"orphan.py".to_vec()],
                ).await?;
            } else {
                connection
                    .execute_batch("DROP TRIGGER goal_original_parts_insert_guard;")
                    .await?;
                connection.execute(
                    "INSERT INTO goal_original_parts(task_id,goal_revision,part_sequence,start_byte,end_byte,original_bytes)
                     VALUES(?1,1,1,0,1,?2)",
                    params![goal.task_id().as_bytes().to_vec(), b"x".to_vec()],
                ).await?;
            }
            assert_eq!(
                store
                    .load_current_goal_contract(&fixture.project, goal.task_id())
                    .await,
                Err(GoalContractStoreFailure::InvalidStoredData)
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
fn oversized_stored_blobs_hit_the_resource_guard_before_decoding() -> TestResult {
    crate::run_native_libsql_test(async {
        for path_blob in [false, true] {
            let fixture = Fixture::new()?;
            let invariant = DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
            )?;
            let requirement = if path_blob {
                CriterionEvidenceRequirement::ChangeOnly(invariant)
            } else {
                test_requirement()?
            };
            let goal = source_goal(small_original(), requirement)?;
            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            CreateGoalContract::new(&store)
                .execute(&fixture.project, &goal)
                .await?;
            let database = fixture.candidate_database().await?;
            let connection = database.connection_for_operation().await?;
            connection
                .execute_batch("PRAGMA ignore_check_constraints=ON;")
                .await?;
            if path_blob {
                connection
                    .execute_batch(
                        "DROP TRIGGER goal_criterion_diff_paths_update_guard;
                  UPDATE goal_criterion_diff_paths SET path=zeroblob(8388608);",
                    )
                    .await?;
            } else {
                connection.execute_batch("DROP TRIGGER goal_original_parts_update_guard;
                  UPDATE goal_original_parts SET original_bytes=zeroblob(8388608) WHERE part_sequence=1;").await?;
            }
            // Exercise the repository on an already-owned connection, so a preflight
            // integrity rejection cannot hide the bounded row decoder being checked.
            let result = crate::goal_contract_repository::load_current(
                &connection,
                fixture.project.worktree().id(),
                goal.task_id(),
            )
            .await;
            assert!(matches!(
                result,
                Err(crate::goal_contract_repository::GoalContractRepositoryError::ResourceLimit)
            ));
            drop(connection);
            drop(database);
            drop(store);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug)]
struct CandidateContractFactory;

impl a3_storage_contract_tests::KnowledgeStoreContractFactory for CandidateContractFactory {
    type Store = LibsqlKnowledgeStore;
    fn open<'a>(
        &'a self,
        root: &'a std::path::Path,
    ) -> a3_storage_contract_tests::ContractFactoryFuture<'a, Self::Store> {
        Box::pin(async move {
            let layout = StorageLayout::prepare(root)?;
            Ok(LibsqlKnowledgeStore::open_plan22_candidate(&layout).await?)
        })
    }
}

#[test]
fn candidate_goal_and_ledger_pass_the_existing_shared_port_contracts() -> TestResult {
    crate::run_native_libsql_test(async {
        use a3_storage_contract_tests::{
            KnowledgeStoreContractGroup, verify_knowledge_store_contract_group,
        };
        for group in [
            KnowledgeStoreContractGroup::GoalContracts,
            KnowledgeStoreContractGroup::TaskLedgers,
        ] {
            verify_knowledge_store_contract_group(&CandidateContractFactory, group)
                .await
                .map_err(|error| error as Box<dyn Error>)?;
        }
        Ok(())
    })
}

fn large_original() -> Vec<u8> {
    large_original_ending_with("\r\nR30: implement a later option\r\n  ")
}

fn large_original_ending_with(suffix: &str) -> Vec<u8> {
    let prefix = "  R01: repair average\r\n";
    let remaining = 65_536 - prefix.len() - suffix.len();
    format!(
        "{prefix}{}{padding}{suffix}",
        "\u{1f4a1}".repeat(remaining / 4),
        padding = "x".repeat(remaining % 4)
    )
    .into_bytes()
}

fn small_original() -> Vec<u8> {
    b"  R01: repair average\r\nR30: implement a later option\r\n  ".to_vec()
}

fn test_requirement() -> TestResult<CriterionEvidenceRequirement> {
    Ok(CriterionEvidenceRequirement::ExistingTest {
        command: DiscoveredCommandId::from_bytes([41; 32]),
        selector: TestCaseSelector::Exact(TestCaseSelectorName::try_from_string(
            "test_behavior.Average.test_fractional".to_owned(),
        )?),
        minimum_cases: MinimumTestCaseCount::new(1)?,
        scope: VerificationScope::Targeted,
        python_integer_translation: None,
    })
}

fn plain_draft(summary: &str) -> TestResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(summary.to_owned())?,
        vec![
            criterion(2, "Average returns the requested result")?,
            criterion(3, "Later requirement R30 remains open")?,
        ],
        vec![GoalConstraint::try_from_string(
            "Do not modify foreign files".to_owned(),
        )?],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Verify each required outcome with its own appropriate evidence".to_owned(),
        )?,
    )?)
}

fn criterion(id: u8, text: &str) -> TestResult<AcceptanceCriterion> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([id; 32]),
        AcceptanceCriterionStatement::try_from_string(text.to_owned())?,
    ))
}

fn bound_draft(
    original: &GoalOriginalRequest,
    first: CriterionEvidenceRequirement,
    summary: &str,
) -> TestResult<GoalContractDraft> {
    let late = original
        .as_str()
        .find("R30:")
        .ok_or("fixture R30 missing")?;
    let line_end = original
        .as_str()
        .find('\n')
        .ok_or("fixture first line missing")?
        + 1;
    let statement = if matches!(&first, CriterionEvidenceRequirement::ChangeOnly(_)) {
        "Requested documentation artifact changes"
    } else {
        "Average returns the requested result"
    };
    let criteria = vec![
        criterion(2, statement)?
            .with_origin(CriterionOrigin::new(original.range(0, line_end)?, first)),
        criterion(3, "Later requirement R30 remains open")?.with_origin(CriterionOrigin::new(
            original.range(late, original.as_bytes().len())?,
            CriterionEvidenceRequirement::RuntimeNeedsRecipe,
        )),
    ];
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(summary.to_owned())?,
        criteria,
        vec![GoalConstraint::try_from_string(
            "Do not modify foreign files".to_owned(),
        )?],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Verify each required outcome with its own appropriate evidence".to_owned(),
        )?,
    )?)
}

fn source_goal(
    bytes: Vec<u8>,
    requirement: CriterionEvidenceRequirement,
) -> TestResult<GoalContract> {
    let summary = if matches!(&requirement, CriterionEvidenceRequirement::ChangeOnly(_)) {
        "Update requested documentation"
    } else {
        "Repair average"
    };
    let legacy = GoalContract::initial(
        TaskId::from_bytes([1; 32]),
        plain_draft(summary)?,
        GoalContractTimestamp::from_unix_millis(10)?,
    );
    let original = GoalOriginalRequest::try_from_bytes(legacy.reference(), bytes)?;
    Ok(GoalContract::initial(
        legacy.task_id(),
        bound_draft(&original, requirement, summary)?,
        legacy.created_at(),
    )
    .with_original(original)?)
}

fn source_goal_with_late_constraint(bytes: Vec<u8>) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([1; 32]);
    let original =
        GoalOriginalRequest::try_from_bytes(GoalContractReference::initial(task), bytes)?;
    let late = original
        .as_str()
        .find("R30:")
        .ok_or("late outcome missing")?;
    let constraint_text = "Do not modify foreign files";
    let constraint_start = original
        .as_str()
        .find(constraint_text)
        .ok_or("late constraint missing")?;
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Repair average".to_owned())?,
        vec![
            criterion(2, "Average returns the requested result")?.with_origin(
                CriterionOrigin::new(original.range(0, late)?, test_requirement()?),
            ),
            criterion(3, "Later requirement R30 remains open")?.with_origin(CriterionOrigin::new(
                original.range(late, constraint_start)?,
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
            )),
        ],
        vec![
            GoalConstraint::try_from_string(constraint_text.to_owned())?
                .with_origin(original.range(constraint_start, original.as_bytes().len())?),
        ],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Verify each required outcome with its own appropriate evidence".to_owned(),
        )?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(10)?)
            .with_original(original)?,
    )
}

fn revised_goal(current: &GoalContract, late: &str) -> TestResult<GoalContract> {
    let bytes = format!("R01: repair average\r\n{late}\r\n").into_bytes();
    let original = GoalOriginalRequest::try_from_bytes(current.next_reference()?, bytes)?;
    let draft = bound_draft(&original, test_requirement()?, late)?;
    Ok(current.revise_with_original(
        draft,
        GoalRevisionReason::try_from_string("User clarified R30".to_owned())?,
        GoalContractTimestamp::from_unix_millis(11)?,
        original,
    )?)
}

fn test_spec() -> TestResult<VerificationSpec> {
    let CriterionEvidenceRequirement::ExistingTest {
        command,
        selector,
        minimum_cases,
        scope,
        ..
    } = test_requirement()?
    else {
        return Err("fixture test requirement missing".into());
    };
    Ok(VerificationSpec::test(
        VerificationSpecId::from_bytes([42; 32]),
        VerificationRequirement::try_from_string("Selected tests execute and pass".to_owned())?,
        command,
        selector,
        minimum_cases,
        scope,
    ))
}

fn diff_spec(invariant: DiffInvariantVerification) -> TestResult<VerificationSpec> {
    Ok(VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([42; 32]),
        VerificationRequirement::try_from_string("Only the requested artifact changes".to_owned())?,
        invariant,
    ))
}

fn step(spec: VerificationSpec) -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([43; 32]),
        None,
        TaskStepOutcome::try_from_string("Implement R01".to_owned())?,
        TaskStepRationale::try_from_string(
            "Bounded implementation for the first outcome".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Appropriate current proof".to_owned(),
        )?],
        spec,
    )?
    .with_acceptance_criteria(vec![AcceptanceCriterionId::from_bytes([2; 32])])?)
}

async fn count(connection: &Connection, sql: &str) -> TestResult<i64> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows.next().await?.ok_or("fixture count row missing")?;
    Ok(row.get(0)?)
}

struct Fixture {
    root: PathBuf,
    layout: StorageLayout,
    project: ProjectIdentity,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let root = fixture_root::reserve_fixture_root(
            &std::env::temp_dir(),
            std::process::id(),
            &NEXT_FIXTURE,
        )?;
        let common = root.join("common");
        let worktree = root.join("worktree");
        fs::create_dir(&common)?;
        fs::create_dir(&worktree)?;
        let repository = RepositoryId::from_bytes([100; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(
                repository,
                CanonicalDirectory::from_canonicalized(fs::canonicalize(common)?)?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([101; 32]),
                WorktreeAnchorId::from_bytes([102; 32]),
                repository,
                CanonicalDirectory::from_canonicalized(fs::canonicalize(worktree)?)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        let layout = StorageLayout::prepare(root.join("app-data"))?;
        Ok(Self {
            root,
            layout,
            project,
        })
    }

    async fn candidate_database(&self) -> TestResult<KnowledgeDatabase> {
        let project_layout = self.layout.prepare_project(self.project.worktree())?;
        let database =
            KnowledgeDatabase::open_plan22_candidate(&project_layout, &self.project).await?;
        assert_eq!(
            database.schema_version().get(),
            crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        );
        assert_eq!(KnowledgeSchemaVersion::CURRENT.get(), 42);
        Ok(database)
    }

    fn cleanup(self) -> TestResult {
        let target = fs::canonicalize(&self.root)?;
        let parent = fs::canonicalize(std::env::temp_dir())?;
        if target.parent() != Some(parent.as_path())
            || !target
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("a3-plan22-goal-"))
        {
            return Err("fixture cleanup target escaped its owned temp root".into());
        }
        fs::remove_dir_all(target)?;
        Ok(())
    }
}

/// Test composition only; eligibility/qualification is always enforced by Application.
/// Older standalone consumers get one owned bounded port, never a global parser.
async fn prepare_native_edit(
    phase: a3_application::PreparedEditPhase,
    request: &mut a3_application::CompiledEditPhaseRequest,
    current: a3_application::EditPhaseState<'_>,
    at: a3_domain::AgentRunTimestamp,
    patch: &dyn a3_application::WorkspacePatchTool,
    control: &dyn a3_application::WorkspacePatchControl,
) -> TestResult<a3_application::PreparedEditPhase> {
    let compiler = a3_repo_index::BuiltinIncrementalIndexCompiler::new(
        a3_repo_index::ParserPoolSize::new(1)?,
    )?;
    prepare_indexed_edit(&compiler, phase, request, current, at, patch, control).await
}

/// Current P7 composition reuses the actual fixture's existing bounded adapter pools.
#[allow(clippy::too_many_arguments)]
async fn prepare_indexed_edit(
    compiler: &a3_repo_index::BuiltinIncrementalIndexCompiler,
    phase: a3_application::PreparedEditPhase,
    request: &mut a3_application::CompiledEditPhaseRequest,
    current: a3_application::EditPhaseState<'_>,
    at: a3_domain::AgentRunTimestamp,
    patch: &dyn a3_application::WorkspacePatchTool,
    control: &dyn a3_application::WorkspacePatchControl,
) -> TestResult<a3_application::PreparedEditPhase> {
    let adapter = phase
        .target_revision()
        .map(|r| compiler.shared_language_adapter_for(r.path()))
        .transpose()?
        .flatten();
    Ok(phase
        .prepare_scope_frame(request, current, adapter, at, patch, control)
        .await?)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_indexed_bound_edit(
    compiler: &a3_repo_index::BuiltinIncrementalIndexCompiler,
    phase: a3_application::PreparedEditPhase,
    request: &a3_application::CompiledEditPhaseRequest,
    current: a3_application::EditPhaseState<'_>,
    at: a3_domain::AgentRunTimestamp,
    patch: &dyn a3_application::WorkspacePatchTool,
    control: &dyn a3_application::WorkspacePatchControl,
) -> TestResult<a3_application::PreparedEditPhase> {
    let adapter = phase
        .target_revision()
        .map(|r| compiler.shared_language_adapter_for(r.path()))
        .transpose()?
        .flatten();
    Ok(phase
        .prepare_bound_scope_frame(request, current, adapter, at, patch, control)
        .await?)
}
