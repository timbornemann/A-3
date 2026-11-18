//! Diagnostic atomicity, independent bindings and the crash before terminal step CAS.
use super::*;
use a3_domain::{LoopbackHttpMismatch, LoopbackHttpMismatchDiagnostic, LoopbackStartupPhase};
#[path = "plan22_http_correction_plan_tests.rs"]
mod correction_plan;

async fn prepared_mismatch() -> TestResult<(
    PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    LoopbackHttpMismatchDiagnostic,
    AgentRun,
    RunEvent,
    RunEventSequence,
)> {
    let mut f = prepare_startup_fixture(Case::WrongBody).await?;
    let mut decisions = Vec::new();
    for (index, action) in f.prepared.policy_actions().iter().enumerate() {
        decisions.push(
            super::super::approve(
                &f.fixture,
                &f.store,
                &mut f.run,
                action,
                10 + 5 * u8::try_from(index)?,
            )
            .await?,
        );
    }
    let c = a3_domain::LoopbackHttpEvidenceContext {
        verification_run_id: VerificationRunId::from_bytes([230; 32]),
        spec_id: f.spec.id(),
        run_id: f.run.id(),
        tool_run_id: TOOL,
        step_id: f.step,
        snapshot_id: f.prepared.snapshot(),
    };
    let scope = MachineEffectScope::new(
        AgentMutationKind::MachineProcess,
        f.step,
        f.prepared.effect_resource(),
        "python".to_owned(),
        Some(f.prepared.script().content_hash()),
        None,
    )?;
    let attempt = f
        .store
        .begin_machine_mutation_attempt(
            &f.fixture.project,
            f.run.id(),
            c.snapshot_id,
            TOOL,
            f.prepared.effect_fingerprint(),
            &scope,
            at(70)?,
        )
        .await?;
    let actual = LoopbackHttpObservation::new(
        decisions[3],
        200,
        hash(b"Broke\n"),
        6,
        LoopbackHeaderChecks {
            plain_utf8: true,
            allow_get: false,
        },
    )?;
    let mismatch = LoopbackHttpMismatch::new(
        f.prepared.recipe(),
        LoopbackStartupPhase::FreeExplicit,
        0,
        actual,
    )?;
    let diagnostic = LoopbackHttpMismatchDiagnostic::new(
        c,
        attempt.tool_attempt().attempt(),
        f.fixture.project.worktree().id(),
        f.prepared.recipe(),
        f.prepared.script().clone(),
        LoopbackHttpProcess::from_spec(f.prepared.start())?,
        mismatch,
        decisions,
    )?;
    let expected = f.run.last_event_sequence();
    let mut next = f.run.clone();
    let event = next.record(
        RunEventId::from_bytes([231; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(
            RunEventCode::VerificationFailure,
            Some(RunEventOutcome::Failed),
            None,
        ),
        c.snapshot_id,
        Some(RunEventSubject::Tool(TOOL)),
        at(80)?,
    )?;
    Ok((f, diagnostic, next, event, expected))
}

#[test]
fn mismatch_atomic_failure_or_cas_conflict_leaves_unknown_without_diagnosis() -> TestResult {
    crate::run_native_libsql_test(async {
        for trigger in [false, true] {
            let (f, d, next, event, expected) = prepared_mismatch().await?;
            let db = f.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            if trigger {
                conn.execute_batch("CREATE TRIGGER reject_mismatch BEFORE INSERT ON loopback_mismatch_diagnostics BEGIN SELECT RAISE(ABORT,'fixture mismatch rollback'); END;").await?;
            }
            let result = f
                .store
                .complete_loopback_mismatch(
                    &f.fixture.project,
                    if trigger {
                        expected
                    } else {
                        RunEventSequence::FIRST
                    },
                    &next,
                    &event,
                    &d,
                    Duration::from_secs(10),
                    &ActiveControl,
                )
                .await;
            assert_eq!(
                result,
                Err(if trigger {
                    AgentRecoveryStoreFailure::Unavailable
                } else {
                    AgentRecoveryStoreFailure::RunSequenceConflict
                })
            );
            assert_eq!(
                f.store
                    .load_agent_run(&f.fixture.project, f.run.id())
                    .await?,
                Some(f.run.clone())
            );
            let attempts = f
                .store
                .load_agent_mutation_attempts(&f.fixture.project, f.run.id())
                .await?;
            assert_eq!(
                attempts[0].tool_attempt().status(),
                AgentToolAttemptStatus::InFlight
            );
            assert_eq!(
                attempts[0].disposition(),
                AgentMutationDisposition::Unknown(a3_domain::MutationReconciliation::Required)
            );
            assert!(
                f.store
                    .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                    .await?
                    .is_none()
            );
            drop((conn, db, f.compiler, f.refresh, f.store));
            f.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn mismatch_crash_after_failed_tool_commit_cannot_resume_or_produce_evidence() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, d, next, event, expected) = prepared_mismatch().await?;
        f.store
            .complete_loopback_mismatch(
                &f.fixture.project,
                expected,
                &next,
                &event,
                &d,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let run = f.run.id();
        drop((f.compiler, f.refresh, f.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.fixture.layout).await?;
        assert_eq!(
            reopened
                .load_loopback_mismatch(&f.fixture.project, run, TOOL, d.attempt())
                .await?,
            Some(d.clone())
        );
        assert_eq!(
            reopened.load_agent_run(&f.fixture.project, run).await?,
            Some(next)
        );
        assert!(
            reopened
                .load_settled_loopback_evidence_id(&f.fixture.project, run, TOOL, d.attempt())
                .await?
                .is_none()
        );
        let inspection = InspectAgentRunRecovery::new(&reopened, &reopened, &reopened, &reopened)
            .execute(&f.fixture.project, run, at(81)?, &ActiveControl)
            .await?;
        assert!(inspection.loopback_resume_pending());
        assert!(!inspection.can_resume());
        assert!(!inspection.mutation_reconciliation_required());
        let result = RecoverAgentRun::new(&reopened, &reopened, &reopened, &reopened)
            .execute(
                &f.fixture.project,
                run,
                AgentRecoveryChoice::Resume,
                RunEventId::from_bytes([232; 32]),
                at(81)?,
                &ActiveControl,
            )
            .await;
        assert!(matches!(
            result,
            Err(AgentRecoveryError::LoopbackResumePending)
        ));
        drop(reopened);
        f.fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn mismatch_store_rejects_foreign_source_scope_attempt_and_consumed_grant() -> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..9 {
            let (f, mut d, next, event, expected) = prepared_mismatch().await?;
            let db = f.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            match kind {
                0 => {
                    conn.execute_batch("DROP TRIGGER machine_effect_scopes_update_guard;")
                        .await?;
                    conn.execute(
                        "UPDATE machine_effect_scopes SET expected_hash=?1 WHERE tool_run_id=?2",
                        libsql::params![vec![99_u8; 32], TOOL.as_bytes().to_vec()],
                    )
                    .await?;
                }
                1 => {
                    conn.execute_batch("DROP TRIGGER approval_grants_consumption_guard; DROP TRIGGER approval_grants_lifecycle_guard;")
                        .await?;
                    conn.execute("UPDATE approval_grants SET consumed_decision_id=?1 WHERE consumed_decision_id=?2",libsql::params![d.decisions()[4].as_bytes().to_vec(),d.decisions()[3].as_bytes().to_vec()]).await?;
                }
                2 => {
                    conn.execute(
                        "UPDATE file_revisions SET content_hash=?1 WHERE repository_path=?2",
                        libsql::params![vec![99_u8; 32], b"server.py".to_vec()],
                    )
                    .await?;
                }
                3 => {
                    conn.execute(
                        "UPDATE task_steps SET retired_plan_revision=introduced_plan_revision+1",
                        (),
                    )
                    .await?;
                }
                4 => {
                    conn.execute("UPDATE task_step_attempts SET started_at_unix_millis=71 WHERE outcome='active'",()).await?;
                }
                _ => {
                    let mut context = d.context();
                    let attempt = if kind == 5 {
                        a3_domain::AgentToolAttemptNumber::new(2)?
                    } else {
                        d.attempt()
                    };
                    match kind {
                        6 => context.spec_id = a3_domain::VerificationSpecId::from_bytes([99; 32]),
                        7 => context.snapshot_id = SnapshotId::from_bytes([99; 32]),
                        8 => context.run_id = AgentRunId::from_bytes([99; 32]),
                        _ => {}
                    }
                    let process = if kind == 8 {
                        LoopbackHttpProcess::from_spec(&f.prepared.recipe().process_spec(
                            context.run_id,
                            d.worktree_id(),
                            context.step_id,
                            d.process().settings(),
                        )?)?
                    } else {
                        d.process()
                    };
                    d = LoopbackHttpMismatchDiagnostic::new(
                        context,
                        attempt,
                        d.worktree_id(),
                        f.prepared.recipe(),
                        d.script().clone(),
                        process,
                        d.mismatch(),
                        d.decisions().to_vec(),
                    )?;
                }
            }
            assert_eq!(
                f.store
                    .complete_loopback_mismatch(
                        &f.fixture.project,
                        expected,
                        &next,
                        &event,
                        &d,
                        Duration::from_secs(10),
                        &ActiveControl
                    )
                    .await,
                Err(if kind == 5 {
                    AgentRecoveryStoreFailure::ToolAttemptConflict
                } else if kind == 2 {
                    AgentRecoveryStoreFailure::PublishedSnapshotConflict
                } else {
                    AgentRecoveryStoreFailure::InvalidStoredData
                })
            );
            drop((conn, db, f.compiler, f.refresh, f.store));
            f.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug)]
struct Cancelled;
impl a3_application::AgentControllerControl for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn mismatch_ports_reject_production42_without_candidate_schema_or_data_changes() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, d, next, event, expected) = prepared_mismatch().await?;
        let baseline = Fixture::new()?;
        let layout = baseline
            .layout
            .prepare_project(baseline.project.worktree())?;
        let db = KnowledgeDatabase::open(&layout, &baseline.project).await?;
        let conn = db.connection_for_operation().await?;
        conn.execute_batch("CREATE TABLE retained_fixture(value TEXT NOT NULL); INSERT INTO retained_fixture VALUES('unchanged baseline');").await?;
        let production = LibsqlKnowledgeStore::open(&baseline.layout).await?;
        assert_eq!(
            production
                .load_loopback_mismatch(&baseline.project, next.id(), TOOL, d.attempt())
                .await,
            Err(AgentRecoveryStoreFailure::UnsupportedSchema)
        );
        assert_eq!(
            production
                .complete_loopback_mismatch(
                    &baseline.project,
                    expected,
                    &next,
                    &event,
                    &d,
                    Duration::from_secs(10),
                    &ActiveControl
                )
                .await,
            Err(AgentRecoveryStoreFailure::UnsupportedSchema)
        );
        assert_eq!(db.schema_version().get(), 42);
        let mut rows=conn.query("SELECT value,(SELECT count(*) FROM sqlite_master WHERE name='loopback_mismatch_diagnostics'),(SELECT count(*) FROM pragma_table_info('tool_run_attempts') WHERE name='has_loopback_mismatch') FROM retained_fixture",()).await?;
        let row = rows.next().await?.ok_or("baseline retained data missing")?;
        assert_eq!(row.get::<String>(0)?, "unchanged baseline");
        assert_eq!(row.get::<i64>(1)?, 0);
        assert_eq!(row.get::<i64>(2)?, 0);
        drop((row, rows, conn, db, production));
        baseline.cleanup()?;
        drop((f.compiler, f.refresh, f.store));
        f.fixture.cleanup()?;
        Ok(())
    })
}

#[test]
fn mismatch_cancelled_or_exhausted_write_has_no_diagnostic_effect() -> TestResult {
    crate::run_native_libsql_test(async {
        for cancel in [false, true] {
            let (f, d, next, event, expected) = prepared_mismatch().await?;
            let control: &dyn a3_application::AgentControllerControl =
                if cancel { &Cancelled } else { &ActiveControl };
            assert_eq!(
                f.store
                    .complete_loopback_mismatch(
                        &f.fixture.project,
                        expected,
                        &next,
                        &event,
                        &d,
                        if cancel {
                            Duration::from_secs(10)
                        } else {
                            Duration::ZERO
                        },
                        control
                    )
                    .await,
                Err(AgentRecoveryStoreFailure::Unavailable)
            );
            assert!(
                f.store
                    .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                    .await?
                    .is_none()
            );
            assert_eq!(
                f.store
                    .load_agent_run(&f.fixture.project, f.run.id())
                    .await?,
                Some(f.run.clone())
            );
            drop((f.compiler, f.refresh, f.store));
            f.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn mismatch_presence_marker_and_sealed_blob_corruption_fail_closed() -> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..5 {
            let (f, d, next, event, expected) = prepared_mismatch().await?;
            f.store
                .complete_loopback_mismatch(
                    &f.fixture.project,
                    expected,
                    &next,
                    &event,
                    &d,
                    Duration::from_secs(10),
                    &ActiveControl,
                )
                .await?;
            let db = f.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            match kind {
                0 => {
                    conn.execute_batch("UPDATE tool_run_attempts SET has_loopback_mismatch=0;")
                        .await?;
                }
                1 => {
                    conn.execute_batch("DROP TRIGGER loopback_mismatch_no_delete; DELETE FROM loopback_mismatch_diagnostics;").await?;
                }
                2 => {
                    conn.execute_batch("DROP TRIGGER loopback_mismatch_immutable; UPDATE loopback_mismatch_diagnostics SET diagnostic=zeroblob(500);").await?;
                }
                3 => {
                    conn.execute_batch("DROP TRIGGER loopback_mismatch_immutable; PRAGMA ignore_check_constraints=ON; UPDATE loopback_mismatch_diagnostics SET diagnostic=zeroblob(200000);").await?;
                }
                _ => {
                    conn.execute_batch("DROP TRIGGER loopback_mismatch_immutable;")
                        .await?;
                    let encoded = crate::loopback_http_codec::mismatch::encode(&d);
                    let mut trailing = encoded[..encoded.len() - 32].to_vec();
                    trailing.push(0);
                    let digest = blake3::derive_key("a3 durable loopback mismatch v1", &trailing);
                    trailing.extend_from_slice(&digest);
                    conn.execute(
                        "UPDATE loopback_mismatch_diagnostics SET diagnostic=?1",
                        libsql::params![trailing],
                    )
                    .await?;
                }
            }
            assert_eq!(
                f.store
                    .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                    .await,
                Err(AgentRecoveryStoreFailure::InvalidStoredData)
            );
            drop((conn, db, f.compiler, f.refresh, f.store));
            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.fixture.layout).await?;
            assert_eq!(
                reopened
                    .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                    .await,
                Err(if kind == 3 {
                    AgentRecoveryStoreFailure::Corrupt
                } else {
                    AgentRecoveryStoreFailure::InvalidStoredData
                })
            );
            drop(reopened);
            f.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn mismatch_regular_published_user_edit_is_a_freshness_conflict_after_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        let (mut f, d, next, event, expected) = prepared_mismatch().await?;
        f.store
            .complete_loopback_mismatch(
                &f.fixture.project,
                expected,
                &next,
                &event,
                &d,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let script = f.root.join("server.py");
        let mut bytes = fs::read(&script)?;
        bytes.extend_from_slice(b"\n# independent user edit\n");
        fs::write(&script, bytes)?;
        let published = f
            .refresh
            .execute(
                &f.fixture.project,
                &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
                &mut f.compiler,
                &ActiveControl,
            )
            .await?;
        assert_ne!(
            published.published_index().run().snapshot_id(),
            d.context().snapshot_id
        );
        assert_eq!(
            f.store
                .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                .await,
            Err(AgentRecoveryStoreFailure::PublishedSnapshotConflict)
        );
        drop((f.compiler, f.refresh, f.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.fixture.layout).await?;
        assert_eq!(
            reopened
                .load_loopback_mismatch(&f.fixture.project, f.run.id(), TOOL, d.attempt())
                .await,
            Err(AgentRecoveryStoreFailure::PublishedSnapshotConflict)
        );
        drop(reopened);
        f.fixture.cleanup()?;
        Ok(())
    })
}
