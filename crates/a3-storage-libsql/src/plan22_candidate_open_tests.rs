use super::*;
use crate::migration::PLAN22_CANDIDATE_KNOWLEDGE_VERSION;
use crate::{LibsqlKnowledgeStore, StorageLayout};
use a3_application::{GoalContractStore, TaskLedgerStore};
use a3_domain::{
    CanonicalDirectory, GitHead, GitReferenceName, RepositoryIdentity, TaskId, WorktreeAnchorId,
    WorktreeIdentity,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    layout: StorageLayout,
    project: ProjectIdentity,
}
impl Fixture {
    fn new(label: &str) -> Result<Self, Box<dyn Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let base = std::env::temp_dir().join(format!(
            "a3-plan22-schema-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&base)?;
        let canonical = |name: &str| -> Result<CanonicalDirectory, Box<dyn Error>> {
            let path = base.join(name);
            fs::create_dir(&path)?;
            Ok(CanonicalDirectory::from_canonicalized(fs::canonicalize(
                path,
            )?)?)
        };
        let repository_id = RepositoryId::from_bytes([121; 32]);
        let worktree_id = WorktreeId::from_bytes([122; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, canonical("common")?, None),
            WorktreeIdentity::new(
                worktree_id,
                WorktreeAnchorId::from_bytes([123; 32]),
                repository_id,
                canonical("worktree")?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        Ok(Self {
            layout: StorageLayout::prepare(base.join("appdata"))?,
            project,
            root: base,
        })
    }

    fn cleanup(self) -> Result<(), Box<dyn Error>> {
        let target = fs::canonicalize(&self.root)?;
        let parent = fs::canonicalize(std::env::temp_dir())?;
        if target.parent() != Some(parent.as_path())
            || !target
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("a3-plan22-schema-"))
        {
            return Err("candidate fixture escaped its owned temp root".into());
        }
        fs::remove_dir_all(target)?;
        Ok(())
    }
}
async fn scalar(connection: &Connection, sql: &str) -> Result<i64, Box<dyn Error>> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows.next().await?.ok_or("missing scalar")?;
    Ok(row.get(0)?)
}

#[test]
fn candidate_rejects_populated_v42_through_v68_before_any_database_write()
-> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        use a3_domain::{
            AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, GoalContract,
            GoalContractDraft, GoalContractTimestamp, GoalObjective, SuccessVerification,
        };
        for version in 42_u32..=69 {
            let fixture = Fixture::new("reject-populated")?;
            let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
            let old = KnowledgeDatabase::open(&layout, &fixture.project).await?;
            let connection = old.connection_for_operation().await?;
            let goal = GoalContract::initial(
                TaskId::from_bytes([124; 32]),
                GoalContractDraft::new(
                    GoalObjective::try_from_string("Keep this existing task".to_owned())?,
                    vec![AcceptanceCriterion::new(
                        AcceptanceCriterionId::from_bytes([125; 32]),
                        AcceptanceCriterionStatement::try_from_string(
                            "Retain every old row untouched".to_owned(),
                        )?,
                    )],
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    SuccessVerification::try_from_string("Inspect the durable outcome".to_owned())?,
                )?,
                GoalContractTimestamp::from_unix_millis(10)?,
            );
            crate::goal_contract_repository::create(
                &connection,
                fixture.project.worktree().id(),
                &goal,
            )
            .await?;
            // Raw negative fixtures only, not reusable old-data openers or upgrade paths.
            for (old_version, name, sql) in [
                (
                    43_u32,
                    "plan22_goal_original_contract",
                    include_str!("migrations/plan22_candidate_v43.sql"),
                ),
                (
                    44_u32,
                    "plan22_goal_annotations",
                    include_str!("migrations/plan22_candidate_v44.sql"),
                ),
                (
                    45_u32,
                    "plan22_pending_goal_intake",
                    include_str!("migrations/plan22_candidate_v45.sql"),
                ),
                (
                    46_u32,
                    "plan22_task_step_addresses",
                    include_str!("migrations/plan22_candidate_v46.sql"),
                ),
                (
                    47_u32,
                    "plan22_pending_horizon_planning",
                    include_str!("migrations/plan22_candidate_v47.sql"),
                ),
                (
                    48_u32,
                    "plan22_goal_intake_reviews",
                    include_str!("migrations/plan22_candidate_v48.sql"),
                ),
                (
                    49_u32,
                    "plan22_edit_phase_reservations",
                    include_str!("migrations/plan22_candidate_v49.sql"),
                ),
                (
                    50_u32,
                    "plan22_intake_phase_reservations",
                    include_str!("migrations/plan22_candidate_v50.sql"),
                ),
                (
                    51_u32,
                    "plan22_intake_structural_repair",
                    include_str!("migrations/plan22_candidate_v51.sql"),
                ),
                (
                    52_u32,
                    "plan22_initial_plan_reservations",
                    include_str!("migrations/plan22_candidate_v52.sql"),
                ),
                (
                    53_u32,
                    "plan22_source_read_acquisitions",
                    include_str!("migrations/plan22_candidate_v53.sql"),
                ),
                (
                    54_u32,
                    "plan22_caller_relocalizations",
                    include_str!("migrations/plan22_candidate_v54.sql"),
                ),
                (
                    55_u32,
                    "plan22_guarded_python_integer_tests",
                    include_str!("migrations/plan22_candidate_v55.sql"),
                ),
                (
                    56_u32,
                    "plan22_loopback_http_verification",
                    include_str!("migrations/plan22_candidate_v56.sql"),
                ),
                (57_u32, "plan22_loopback_http_startup_recipe", "SELECT 1;"),
                (58_u32, "plan22_loopback_http_startup_evidence", "SELECT 1;"),
                (
                    59_u32,
                    "plan22_model_selected_file_additions",
                    include_str!("migrations/plan22_candidate_v59.sql"),
                ),
                (
                    60_u32,
                    "plan22_loopback_mismatch_diagnostics",
                    include_str!("migrations/plan22_candidate_v60.sql"),
                ),
                (
                    61_u32,
                    "plan22_current_run_http_correction_plan",
                    include_str!("migrations/plan22_candidate_v61.sql"),
                ),
                (
                    62_u32,
                    "plan22_http_correction_focus_v2",
                    include_str!("migrations/plan22_candidate_v62.sql"),
                ),
                (
                    63_u32,
                    "plan22_http_correction_public_basis_v3",
                    include_str!("migrations/plan22_candidate_v63.sql"),
                ),
                (64_u32, "plan22_edit_semantic_output_v2", "SELECT 1;"),
                (
                    65_u32,
                    "plan22_http_correction_intent_v4",
                    include_str!("migrations/plan22_candidate_v65.sql"),
                ),
                (
                    66_u32,
                    "plan22_http_context_intent_v1",
                    include_str!("migrations/plan22_candidate_v66.sql"),
                ),
                (
                    67_u32,
                    "plan22_edit_no_change_receipts",
                    include_str!("migrations/plan22_candidate_v67.sql"),
                ),
                (
                    68_u32,
                    "plan22_http_no_change_replan_v5",
                    include_str!("migrations/plan22_candidate_v68.sql"),
                ),
                (
                    69_u32,
                    "plan22_http_no_change_core_window_v6",
                    include_str!("migrations/plan22_candidate_v69.sql"),
                ),
            ] {
                if old_version > version {
                    break;
                }
                let migration_version = old_version.to_le_bytes();
                let mut checksum = blake3::Hasher::new();
                for field in [
                    b"a3.knowledge-migration.v1".as_slice(),
                    migration_version.as_slice(),
                    name.as_bytes(),
                    sql.as_bytes(),
                ] {
                    checksum.update(&(field.len() as u128).to_le_bytes());
                    checksum.update(field);
                }
                let tx = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .await?;
                tx.execute_batch(sql).await?;
                tx.execute(
                    "INSERT INTO schema_migrations(version,name,checksum) VALUES (?1,?2,?3)",
                    params![
                        i64::from(old_version),
                        name,
                        checksum.finalize().as_bytes().to_vec()
                    ],
                )
                .await?;
                tx.execute_batch(&format!("PRAGMA user_version={old_version}"))
                    .await?;
                tx.commit().await?;
                crate::migration::verify_plan22_candidate_migration_history(
                    &connection,
                    old_version,
                )
                .await?;
            }
            assert_eq!(scalar(&connection, "SELECT count(*) FROM tasks").await?, 1);
            assert_eq!(
                scalar(&connection, "SELECT count(*) FROM acceptance_criteria").await?,
                1
            );
            connection
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .await?;
            drop(connection);
            drop(old);
            let before = fs::read(layout.knowledge_path())?;
            let result = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await;
            assert!(matches!(result,
                Err(KnowledgeOpenError::UnexpectedSchemaVersion {expected,found})
                if expected.get()==PLAN22_CANDIDATE_KNOWLEDGE_VERSION && found.get()==version));
            assert!(fs::read(layout.knowledge_path())? == before);
            let readonly = libsql::Builder::new_local(layout.knowledge_path())
                .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .build()
                .await?;
            let connection = readonly.connect()?;
            assert_eq!(
                scalar(&connection, "PRAGMA user_version").await?,
                i64::from(version)
            );
            assert_eq!(scalar(&connection, "SELECT count(*) FROM tasks").await?, 1);
            assert_eq!(
                scalar(&connection, "SELECT count(*) FROM acceptance_criteria").await?,
                1
            );
            assert_eq!(
                scalar(
                    &connection,
                    "SELECT count(*) FROM sqlite_schema WHERE name='source_read_acquisitions'"
                )
                .await?,
                i64::from(version >= 53)
            );
            assert_eq!(
                scalar(
                    &connection,
                    "SELECT count(*) FROM sqlite_schema WHERE name='source_caller_relocalizations'"
                )
                .await?,
                i64::from(version >= 54)
            );
            assert_eq!(
                scalar(
                    &connection,
                    "SELECT count(*) FROM pragma_table_info('verification_evidence')
                     WHERE name='python_integer_translation_witness'"
                )
                .await?,
                i64::from(version >= 55)
            );
            assert_eq!(scalar(&connection, "SELECT count(*) FROM pragma_table_info('verification_evidence') WHERE name='loopback_http_evidence'").await?, i64::from(version >= 56));
            assert_eq!(scalar(&connection, "SELECT count(*) FROM sqlite_schema WHERE name='http_context_intent_reservations'").await?, i64::from(version >= 66));
            assert_eq!(scalar(&connection, "SELECT count(*) FROM schema_migrations WHERE version=66 AND name='plan22_http_context_intent_v1'").await?, i64::from(version >= 66));
            assert_eq!(
                scalar(
                    &connection,
                    "SELECT count(*) FROM sqlite_schema WHERE name='edit_phase_no_change_receipts'"
                )
                .await?,
                i64::from(version >= 67)
            );
            assert_eq!(scalar(&connection, "SELECT count(*) FROM pragma_table_info('edit_phase_reservations') WHERE name='terminal_reply_kind'").await?, i64::from(version >= 67));
            assert_eq!(scalar(&connection, "SELECT count(*) FROM schema_migrations WHERE version=67 AND name='plan22_edit_no_change_receipts'").await?, i64::from(version >= 67));
            assert_eq!(scalar(&connection, "SELECT count(*) FROM sqlite_schema WHERE name='http_no_change_replan_reservations'").await?, i64::from(version >= 68));
            if version >= 68 {
                assert_eq!(scalar(&connection, "SELECT CAST(dflt_value AS INTEGER) FROM pragma_table_info('http_no_change_replan_reservations') WHERE name='protocol_version'").await?, if version==68 {5} else {6});
            }
            if version >= 65 {
                assert_eq!(scalar(&connection, "SELECT count(*) FROM schema_migrations WHERE version=65 AND name='plan22_http_correction_intent_v4'").await?, 1);
                assert_eq!(scalar(&connection, "SELECT CAST(dflt_value AS INTEGER) FROM pragma_table_info('http_correction_plan_reservations') WHERE name='protocol_version'").await?, 4);
            }
            drop(connection);
            drop(readonly);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn candidate_rejects_unversioned_nonempty_database_without_reset() -> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("unversioned-nonempty")?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let unknown = libsql::Builder::new_local(layout.knowledge_path())
            .build()
            .await?;
        let connection = unknown.connect()?;
        connection
            .execute_batch(
                "CREATE TABLE existing_content(value TEXT NOT NULL);
            INSERT INTO existing_content VALUES('preserve this row');",
            )
            .await?;
        assert_eq!(scalar(&connection, "PRAGMA user_version").await?, 0);
        drop(connection);
        drop(unknown);
        let before = fs::read(layout.knowledge_path())?;
        let result = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await;
        assert!(matches!(result,
            Err(KnowledgeOpenError::UnexpectedSchemaVersion {expected,found})
            if expected.get()==PLAN22_CANDIDATE_KNOWLEDGE_VERSION && found.get()==0));
        assert!(fs::read(layout.knowledge_path())? == before);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn candidate_initializes_an_existing_truly_empty_v0_file_and_reopens_current_candidate()
-> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("empty-v0")?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        fs::File::create(layout.knowledge_path())?;
        let initialized =
            KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
        assert_eq!(
            initialized.verify().await?.schema_version().get(),
            PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        );
        assert_eq!(
            scalar(
                initialized.connection(),
                "SELECT count(*) FROM schema_migrations"
            )
            .await?,
            i64::from(PLAN22_CANDIDATE_KNOWLEDGE_VERSION)
        );
        drop(initialized);
        let reopened = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
        assert_eq!(
            reopened.verify().await?.schema_version().get(),
            PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        );
        assert_eq!(scalar(reopened.connection(), "SELECT count(*) FROM pragma_table_info('verification_evidence') WHERE name='loopback_http_evidence'").await?, 1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_index_list('http_context_intent_reservations') WHERE name='http_context_intent_one_accepted_per_source_read' AND \"unique\"=1 AND partial=1").await?,1);
        // V66 adds a separate context-intent job; the V65 HTTP Plan V4 history stays immutable.
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=64 AND name='plan22_edit_semantic_output_v2'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=63 AND name='plan22_http_correction_public_basis_v3'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=65 AND name='plan22_http_correction_intent_v4'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT CAST(dflt_value AS INTEGER) FROM pragma_table_info('http_correction_plan_reservations') WHERE name='protocol_version'").await?,4);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=66 AND name='plan22_http_context_intent_v1'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT CAST(dflt_value AS INTEGER) FROM pragma_table_info('http_context_intent_reservations') WHERE name='protocol_version'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_table_info('http_context_intent_reservations') WHERE (name='run_id' AND pk=1) OR (name='diagnostic_digest' AND pk=2) OR (name='call_kind' AND pk=3)").await?,3);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_foreign_key_list('http_context_intent_reservations') WHERE \"table\"='http_correction_plan_reservations' AND ((\"from\"='run_id' AND \"to\"='run_id') OR (\"from\"='diagnostic_digest' AND \"to\"='diagnostic_digest') OR (\"from\"='parent_call_kind' AND \"to\"='call_kind'))").await?,3);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(DISTINCT id) FROM pragma_foreign_key_list('http_context_intent_reservations') WHERE \"table\"='http_correction_plan_reservations'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_foreign_key_list('http_context_intent_reservations') WHERE \"table\"='source_read_acquisitions' AND \"from\"='source_read_id' AND \"to\"='source_read_id'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=67 AND name='plan22_edit_no_change_receipts'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_table_info('edit_phase_reservations') WHERE name='terminal_reply_kind'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM sqlite_schema WHERE name='edit_phase_no_change_receipts' AND type='table'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name IN ('edit_no_change_tag_insert_null','edit_no_change_tag_finish_once','edit_no_change_only_after_known_finish','edit_no_change_no_update','edit_no_change_no_delete')").await?,5);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=68 AND name='plan22_http_no_change_replan_v5'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM schema_migrations WHERE version=69 AND name='plan22_http_no_change_core_window_v6'").await?,1);
        assert_eq!(scalar(reopened.connection(),"SELECT count(*) FROM schema_migrations WHERE version=70 AND name='plan22_edit_request_dispositions_v3'").await?,1);
        assert_eq!(
            scalar(
                reopened.connection(),
                "SELECT count(*) FROM sqlite_schema WHERE name='edit_phase_jobs' AND type='table'"
            )
            .await?,
            1
        );
        assert_eq!(scalar(reopened.connection(),"SELECT count(*) FROM sqlite_schema WHERE name IN ('edit_job_before_io','edit_job_observe_once','edit_job_no_refund') AND type='trigger'").await?,3);
        assert_eq!(scalar(reopened.connection(),
            "SELECT CAST(dflt_value AS INTEGER) FROM pragma_table_info('http_no_change_replan_reservations') WHERE name='protocol_version'").await?,6);
        assert_eq!(
            scalar(
                reopened.connection(),
                "SELECT count(*) FROM sqlite_schema WHERE name='plan22_v69_empty_replan_guard'"
            )
            .await?,
            0
        );
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM pragma_table_info('http_no_change_replan_reservations') WHERE (name='run_id' AND pk=1) OR (name='closed_job_digest' AND pk=2) OR (name='closed_call_kind' AND pk=3) OR (name='call_kind' AND pk=4)").await?,4);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(DISTINCT id) FROM pragma_foreign_key_list('http_no_change_replan_reservations') WHERE \"table\"='edit_phase_no_change_receipts'").await?,1);
        assert_eq!(scalar(reopened.connection(),
            "SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name IN ('http_no_change_replan_insert_guard','http_no_change_replan_repair_guard','http_no_change_replan_update_guard','http_no_change_replan_delete_guard')").await?,4);
        assert_eq!(
            scalar(
                reopened.connection(),
                "SELECT count(*) FROM pragma_foreign_key_check"
            )
            .await?,
            0
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn default_knowledge_open_still_uses_v42_without_candidate_tables() -> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("default")?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let database = KnowledgeDatabase::open(&layout, &fixture.project).await?;
        assert_eq!(KnowledgeSchemaVersion::CURRENT.get(), 42);
        assert_eq!(database.schema_version().get(), 42);
        assert_eq!(database.verify().await?.schema_version().get(), 42);
        assert_eq!(
            scalar(
                database.connection(),
                "SELECT count(*) FROM sqlite_master WHERE name='goal_original_headers'"
            )
            .await?,
            0
        );
        assert_eq!(scalar(database.connection(),"SELECT count(*) FROM pragma_table_info('goal_contract_revisions') WHERE name='original_contract_version'").await?,0);
        assert_eq!(scalar(database.connection(), "SELECT count(*) FROM pragma_table_info('verification_evidence') WHERE name='loopback_http_evidence'").await?, 0);
        assert_eq!(
            scalar(
                database.connection(),
                "SELECT count(*) FROM sqlite_schema WHERE name='http_context_intent_reservations'"
            )
            .await?,
            0
        );
        assert_eq!(
            scalar(
                database.connection(),
                "SELECT count(*) FROM sqlite_schema WHERE name='http_no_change_replan_reservations'"
            )
            .await?,
            0
        );
        drop(database);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn candidate_goal_ledger_lazy_open_and_fresh_store_reopen_use_real_current_candidate()
-> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("lazy")?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(!layout.knowledge_path().exists());
        let task = TaskId::from_bytes([124; 32]);
        assert!(
            store
                .load_current_goal_contract(&fixture.project, task)
                .await?
                .is_none()
        );
        assert!(
            store
                .load_task_ledger(&fixture.project, task)
                .await?
                .is_none()
        );
        drop(store);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert!(
            reopened
                .load_task_ledger(&fixture.project, task)
                .await?
                .is_none()
        );
        assert!(
            reopened
                .load_current_goal_contract(&fixture.project, task)
                .await?
                .is_none()
        );
        drop(reopened);
        let database = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
        assert_eq!(
            database.verify().await?.schema_version().get(),
            PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        );
        assert_eq!(
            scalar(
                database.connection(),
                "SELECT count(*) FROM sqlite_master WHERE name='goal_original_headers'"
            )
            .await?,
            1
        );
        assert_eq!(scalar(database.connection(),"SELECT count(*) FROM pragma_table_info('goal_contract_revisions') WHERE name='original_contract_version'").await?,1);
        assert_eq!(scalar(database.connection(),"SELECT count(*) FROM schema_migrations WHERE version=43 AND name='plan22_goal_original_contract'").await?,1);
        drop(database);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn production_rejects_current_candidate_before_database_mutation() -> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("reject")?;
        let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
        let candidate = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
        candidate
            .connection()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .await?;
        drop(candidate);
        let before = fs::read(layout.knowledge_path())?;
        let result = KnowledgeDatabase::open(&layout, &fixture.project).await;
        assert!(
            matches!(result,Err(KnowledgeOpenError::NewerSchema{found,supported}) if found.get()==PLAN22_CANDIDATE_KNOWLEDGE_VERSION&&supported.get()==42)
        );
        assert!(fs::read(layout.knowledge_path())? == before);
        let reopened = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
        assert_eq!(
            reopened.verify().await?.schema_version().get(),
            PLAN22_CANDIDATE_KNOWLEDGE_VERSION
        );
        drop(reopened);
        fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn candidate_preflight_rejects_tampered_history_without_rewriting_database()
-> Result<(), Box<dyn Error>> {
    crate::run_native_libsql_test(async {
        for tampered in [
            43_u32,
            64,
            65,
            66,
            67,
            68,
            PLAN22_CANDIDATE_KNOWLEDGE_VERSION,
        ] {
            let fixture = Fixture::new("history")?;
            let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
            let candidate =
                KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
            candidate
                .connection()
                .execute(
                    "UPDATE schema_migrations SET checksum=zeroblob(32) WHERE version=?1",
                    params![i64::from(tampered)],
                )
                .await?;
            candidate
                .connection()
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .await?;
            drop(candidate);
            let before = fs::read(layout.knowledge_path())?;
            let result = KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await;
            assert!(
                matches!(result,Err(KnowledgeOpenError::MigrationHistoryMismatch{version}) if version.get()==tampered)
            );
            assert!(fs::read(layout.knowledge_path())? == before);
            fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn guarded_test_recipe_survives_intake_goal_and_ledger_reopen() -> Result<(), Box<dyn Error>> {
    use crate::python_translation_codec as guard;
    use a3_application::{GoalContractStoreFailure, MaterializeOriginalTask};
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, ContentHash,
        CriterionEvidenceRequirement, CriterionOrigin, DiscoveredCommandId, ExpectedTaskEvidence,
        FileRevision, GoalContract, GoalContractDraft, GoalContractReference,
        GoalContractTimestamp, GoalIntakeFinding, GoalIntakeFindingKind, GoalIntakeReview,
        GoalIntakeReviewBatchProposal, GoalObjective, GoalOriginalRequest, MinimumTestCaseCount,
        PendingGoalIntake, PythonIntegerTranslation, PythonIntegerTranslationWitness,
        RepositoryPath, SourcePosition, SourceRange, SuccessVerification, SymbolName,
        TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, TestCaseSelector,
        VerificationRequirement, VerificationScope, VerificationSpec, VerificationSpecId,
    };

    crate::run_native_libsql_test(async {
        let fixture = Fixture::new("guarded-test")?;
        let project = &fixture.project;
        let task = TaskId::from_bytes([126; 32]);
        let statement = "Increase adjust by one.";
        let original = GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(task),
            statement.as_bytes().to_vec(),
        )?;
        let range = original.range(0, original.as_bytes().len())?;
        let translation = PythonIntegerTranslation::new(
            RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
            SymbolName::try_from_string("adjust".to_owned())?,
            1,
        )?;
        let command = DiscoveredCommandId::from_bytes([127; 32]);
        let minimum = MinimumTestCaseCount::new(1)?;
        let requirement = CriterionEvidenceRequirement::ExistingTest {
            command,
            selector: TestCaseSelector::All,
            minimum_cases: minimum,
            scope: VerificationScope::Workspace,
            python_integer_translation: Some(translation.clone()),
        };
        let criterion = AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([128; 32]),
            AcceptanceCriterionStatement::try_from_string(statement.to_owned())?,
        )
        .with_origin(CriterionOrigin::new(range, requirement.clone()));
        let goal = GoalContract::initial(
            task,
            GoalContractDraft::new(
                GoalObjective::try_from_string(statement.to_owned())?,
                vec![criterion.clone()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    "Verify the bound test and original source".to_owned(),
                )?,
            )?,
            GoalContractTimestamp::from_unix_millis(10)?,
        )
        .with_original(original.clone())?;
        let pending = PendingGoalIntake::new(original, goal.created_at())?;
        let finding = GoalIntakeFinding::new(
            GoalIntakeFindingKind::Outcome {
                requirement: criterion.requirement(),
                evidence_requirement: requirement,
            },
            range,
            statement.to_owned(),
        )?;
        let review = GoalIntakeReview::begin(
            &pending,
            GoalIntakeReviewBatchProposal::new(range, vec![finding], Vec::new())?,
        )?;
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let capture = store.capture_goal_intake(project, &pending).await?;
        store
            .append_goal_intake_review(project, task, capture, None, &review)
            .await?;
        drop(store);

        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store.load_goal_intake_review(project, task).await?,
            Some(review.clone())
        );
        store
            .promote_goal_intake(project, task, capture, &goal)
            .await?;
        let step_id = TaskStepId::from_bytes([129; 32]);
        let spec = VerificationSpec::test_with_python_integer_translation(
            VerificationSpecId::from_bytes([130; 32]),
            VerificationRequirement::try_from_string("Run the exact existing check".to_owned())?,
            command,
            TestCaseSelector::All,
            minimum,
            VerificationScope::Workspace,
            translation.clone(),
        );
        let step = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string(statement.to_owned())?,
            TaskStepRationale::try_from_string("Check the existing Python function".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "One current guarded test".to_owned(),
            )?],
            spec.clone(),
        )?
        .with_acceptance_criteria(vec![criterion.id()])?;
        MaterializeOriginalTask::new(&store, &store)
            .execute(project, &goal, vec![step])
            .await?;
        drop(store);

        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            store.load_current_goal_contract(project, task).await?,
            Some(goal)
        );
        assert_eq!(
            store.load_goal_intake_review(project, task).await?,
            Some(review)
        );
        let ledger = store
            .load_task_ledger(project, task)
            .await?
            .ok_or("guarded Ledger lost")?;
        assert_eq!(
            ledger
                .ledger()
                .step(step_id)
                .ok_or("guarded step lost")?
                .definition()
                .verification_spec(),
            &spec
        );

        let layout = fixture.layout.prepare_project(project.worktree())?;
        let database = KnowledgeDatabase::open_plan22_candidate(&layout, project).await?;
        let connection = database.connection_for_operation().await?;
        // Controlled source-witness codec check. The product parser and process are separate.
        let declaration =
            SourceRange::new(0, 35, SourcePosition::new(0, 0), SourcePosition::new(1, 20))?;
        let expression = SourceRange::new(
            24,
            34,
            SourcePosition::new(1, 9),
            SourcePosition::new(1, 19),
        )?;
        let witness = PythonIntegerTranslationWitness::new(
            translation.clone(),
            FileRevision::new(
                translation.path().clone(),
                ContentHash::from_bytes([131; 32]),
            ),
            declaration,
            expression,
        )?;
        let encoded = guard::write_witness(&witness);
        let mut corrupt = encoded.clone();
        let last = corrupt.last_mut().ok_or("empty witness")?;
        *last ^= 1;
        let mut rows = connection
            .query(
                "SELECT ?1,?2,?3,NULL",
                params![
                    encoded.clone(),
                    corrupt,
                    encoded[..encoded.len() - 1].to_vec()
                ],
            )
            .await?;
        let row = rows.next().await?.ok_or("witness codec row missing")?;
        assert_eq!(
            guard::read_witness(&row, 0).map_err(|_| "valid witness rejected")?,
            Some(witness)
        );
        assert!(guard::read_witness(&row, 1).is_err());
        assert!(guard::read_witness(&row, 2).is_err());
        assert!(
            guard::read_witness(&row, 3)
                .map_err(|_| "NULL witness rejected")?
                .is_none()
        );
        assert!(guard::read_requirement(&row, 0).is_err());
        drop(row);
        drop(rows);
        let mut corrupted_guard = guard::write_requirement(&translation);
        let last = corrupted_guard.last_mut().ok_or("empty guard")?;
        *last ^= 1;
        connection
            .execute("DROP TRIGGER goal_criterion_origins_update_guard", ())
            .await?;
        connection
            .execute(
                "UPDATE goal_criterion_origins SET python_integer_translation=?1 WHERE task_id=?2",
                params![corrupted_guard, task.as_bytes().to_vec()],
            )
            .await?;
        assert_eq!(
            store.load_current_goal_contract(project, task).await,
            Err(GoalContractStoreFailure::InvalidStoredData),
        );
        drop(connection);
        drop(database);
        drop(store);
        fixture.cleanup()
    })
}
