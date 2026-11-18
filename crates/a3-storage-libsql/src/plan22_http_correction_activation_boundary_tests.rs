//! Real durable activation boundaries; retained control fixtures are not local-model quality.
use super::*;

async fn applied_fixture() -> TestResult<CurrentPlanFixture> {
    let mut f = current_fixture().await?;
    settle_focus(&f, "L6").await?;
    let project = &f.http.fixture.project;
    let current = f
        .http
        .store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("current index missing")?;
    let (mut ledger, version) = f.stored.clone().into_parts();
    let applied = ApplyHttpCorrectionPlan::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspaceAgentSourceReader,
    )
    .execute(
        project,
        &f.http.goal,
        &mut f.run,
        &mut ledger,
        version,
        &current,
        &f.phase,
        at(83)?,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, applied.version);
    Ok(f)
}

#[test]
fn http_correction_applied_origin_rejects_selected_diagnostic_alias_with_valid_original_retained()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = applied_fixture().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let now = at(f
            .run
            .updated_at()
            .unix_millis()
            .checked_add(1)
            .ok_or("clock overflow")?)?;
        let before = inventory(&f.http.root)?;
        // The exact original accepted row is valid before a distinct alias is added.
        let baseline = f
            .http
            .store
            .load_applied_http_correction_origin(
                project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &current,
                &f.http.model,
                now,
            )
            .await?;
        assert_eq!(baseline.anchor, *f.phase.anchor());
        let original = f.phase.anchor().diagnostic;
        let alias = ContentHash::from_bytes([247; 32]);
        assert_ne!(alias, original);
        let db = f.http.fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        let mut rows = connection.query(
            "SELECT proposal,proposal_digest,reported_prompt,reported_output,finished_at
             FROM http_correction_plan_reservations WHERE run_id=?1 AND diagnostic_digest=?2 AND call_kind='initial'",
            libsql::params![f.run.id().as_bytes().to_vec(), original.as_bytes().to_vec()],
        ).await?;
        let row = rows
            .next()
            .await?
            .ok_or("valid original reservation missing")?;
        let proposal: Vec<u8> = row.get(0)?;
        let digest: Vec<u8> = row.get(1)?;
        let prompt: i64 = row.get(2)?;
        let output: i64 = row.get(3)?;
        let finished: i64 = row.get(4)?;
        assert!(f.run.updated_at().unix_millis() > u64::try_from(finished)?);
        // Native libSQL rows retain their statement/connection until explicitly released.
        drop(row);
        assert!(rows.next().await?.is_none());
        drop(rows);
        // Keep every trigger/CHECK/FK enabled: insert unobserved, then settle exactly once.
        // Only this duplicate's diagnostic key differs; immutable anchor/job/tool/wire stay exact.
        assert_eq!(connection.execute(
            "INSERT INTO http_correction_plan_reservations(
             worktree_id,run_id,diagnostic_digest,tool_run_id,tool_attempt,job_digest,call_kind,
             anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,protocol_version)
             SELECT worktree_id,run_id,?1,tool_run_id,tool_attempt,job_digest,call_kind,
             anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,protocol_version
             FROM http_correction_plan_reservations WHERE run_id=?2 AND diagnostic_digest=?3 AND call_kind='initial'",
            libsql::params![alias.as_bytes().to_vec(), f.run.id().as_bytes().to_vec(), original.as_bytes().to_vec()],
        ).await?, 1);
        assert_eq!(connection.execute(
            "UPDATE http_correction_plan_reservations SET outcome='accepted_reply',finished_at=?1,
             reported_prompt=?2,reported_output=?3,proposal=?4,proposal_digest=?5
             WHERE run_id=?6 AND diagnostic_digest=?7 AND call_kind='initial'",
            libsql::params![i64::try_from(f.run.updated_at().unix_millis())?, prompt, output, proposal.clone(), digest.clone(), f.run.id().as_bytes().to_vec(), alias.as_bytes().to_vec()],
        ).await?, 1);
        let mut selected = connection.query(
            "SELECT diagnostic_digest FROM http_correction_plan_reservations WHERE run_id=?1 AND outcome='accepted_reply'
             ORDER BY finished_at DESC,CASE call_kind WHEN 'structural_repair' THEN 0 ELSE 1 END LIMIT 1",
            libsql::params![f.run.id().as_bytes().to_vec()],
        ).await?;
        assert_eq!(
            selected
                .next()
                .await?
                .ok_or("selected alias missing")?
                .get::<Vec<u8>>(0)?,
            alias.as_bytes().to_vec()
        );
        drop(selected);
        let mut originals = connection.query(
            "SELECT proposal,proposal_digest,reported_prompt,reported_output,finished_at,
             (SELECT COUNT(*) FROM http_correction_plan_reservations WHERE run_id=?1)
             FROM http_correction_plan_reservations WHERE run_id=?1 AND diagnostic_digest=?2 AND call_kind='initial'",
            libsql::params![f.run.id().as_bytes().to_vec(), original.as_bytes().to_vec()],
        ).await?;
        let retained = originals.next().await?.ok_or("original was lost")?;
        assert_eq!(retained.get::<Vec<u8>>(0)?, proposal);
        assert_eq!(retained.get::<Vec<u8>>(1)?, digest);
        assert_eq!(retained.get::<i64>(2)?, prompt);
        assert_eq!(retained.get::<i64>(3)?, output);
        assert_eq!(retained.get::<i64>(4)?, finished);
        assert_eq!(retained.get::<i64>(5)?, 2);
        drop(retained);
        drop(originals);
        assert!(matches!(
            f.http
                .store
                .load_applied_http_correction_origin(
                    project,
                    &f.http.goal,
                    &f.stored,
                    &f.run,
                    &current,
                    &f.http.model,
                    now
                )
                .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        let mut run = f.run.clone();
        let (mut ledger, mut version) = f.stored.clone().into_parts();
        let reader = DenyNthSource {
            calls: AtomicUsize::new(0),
            deny: 1,
        };
        let result = ActivateHttpCorrectionEdit::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &reader,
        )
        .execute(
            project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            &mut version,
            &current,
            &f.http.model,
            now,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            result,
            Err(HttpCorrectionEditFailure::Origin(
                EditPhaseReservationFailure::StaleAnchor
            ))
        ));
        assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
        assert_eq!(run, f.run);
        assert_eq!(ledger, *f.stored.ledger());
        assert_eq!(version, f.stored.version());
        assert_eq!(inventory(&f.http.root)?, before);
        assert_eq!(
            f.http.store.load_agent_run(project, run.id()).await?,
            Some(f.run.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored.clone())
        );
        drop((
            connection,
            db,
            f.http.compiler,
            f.http.refresh,
            f.http.store,
        ));
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        assert!(matches!(
            cold.load_applied_http_correction_origin(
                project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &current,
                &f.http.model,
                now
            )
            .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        assert_eq!(cold.load_agent_run(project, f.run.id()).await?, Some(f.run));
        assert_eq!(
            cold.load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        drop(cold);
        f.http.fixture.cleanup()
    })
}

#[derive(Debug)]
struct DenyNthSource {
    calls: AtomicUsize,
    deny: usize,
}
impl AgentSourceReader for DenyNthSource {
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.deny {
                return Err(AgentSourceReadFailure::Denied);
            }
            WorkspaceAgentSourceReader
                .read_page(project, revision, request, control)
                .await
        })
    }
}

#[test]
fn http_correction_activation_cold_resumes_each_exact_committed_prefix_once() -> TestResult {
    crate::run_native_libsql_test(async {
        for (deny, expected) in [
            (1, AgentControllerState::Replan),
            (2, AgentControllerState::Localize),
            (3, AgentControllerState::Plan),
            (4, AgentControllerState::Execute),
        ] {
            let f = applied_fixture().await?;
            let project = &f.http.fixture.project;
            let current = f
                .http
                .store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("index missing")?;
            let mut run = f.run.clone();
            let (mut ledger, mut version) = f.stored.clone().into_parts();
            let before = inventory(&f.http.root)?;
            let charged = f
                .http
                .store
                .load_edit_phase_usage(project, run.id())
                .await?;
            let reader = DenyNthSource {
                calls: AtomicUsize::new(0),
                deny,
            };
            let result = ActivateHttpCorrectionEdit::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &reader,
            )
            .execute(
                project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                &mut version,
                &current,
                &f.http.model,
                at(f.run.updated_at().unix_millis() + 1)?,
                &ActiveControl,
            )
            .await;
            assert!(matches!(
                result,
                Err(HttpCorrectionEditFailure::Source(
                    AgentSourceReadFailure::Denied
                ))
            ));
            assert_eq!(run.state(), expected);
            assert_eq!(
                run.last_event_sequence().get(),
                f.run.last_event_sequence().get() + u64::try_from(deny - 1)?
            );
            assert_eq!(run.usage(), f.run.usage());
            assert_eq!(run.budget(), f.run.budget());
            assert_eq!(inventory(&f.http.root)?, before);
            let correction = ledger
                .steps()
                .find(|s| {
                    s.is_active_plan_step() && !s.definition().addressed_criteria().is_empty()
                })
                .ok_or("correction missing")?;
            assert_eq!(
                correction.attempts().len(),
                usize::from(expected == AgentControllerState::Execute)
            );
            assert_eq!(
                f.http.store.load_agent_run(project, run.id()).await?,
                Some(run.clone())
            );
            assert_eq!(
                f.http
                    .store
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(StoredTaskLedger::new(ledger.clone(), version))
            );
            let goal = f.http.goal.clone();
            drop((f.http.compiler, f.http.refresh, f.http.store));
            let cold = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            assert_eq!(
                cold.load_current_goal_contract(project, goal.task_id())
                    .await?,
                Some(goal.clone())
            );
            assert_eq!(
                cold.load_agent_run(project, run.id()).await?,
                Some(run.clone())
            );
            let retained = cold
                .load_task_ledger(project, goal.task_id())
                .await?
                .ok_or("cold ledger missing")?;
            assert_eq!(retained, StoredTaskLedger::new(ledger.clone(), version));
            let now = at(run.updated_at().unix_millis() + 1)?;
            let diagnostic =
                ActivateHttpCorrectionEdit::new(&cold, &cold, &cold, &WorkspaceAgentSourceReader)
                    .execute(
                        project,
                        &goal,
                        &mut run,
                        &mut ledger,
                        &mut version,
                        &current,
                        &f.http.model,
                        now,
                        &ActiveControl,
                    )
                    .await?;
            assert_eq!(run.state(), AgentControllerState::Execute);
            assert_eq!(
                run.last_event_sequence().get(),
                f.run.last_event_sequence().get() + 3
            );
            assert_eq!(
                ledger
                    .steps()
                    .filter(|s| s.is_active_plan_step())
                    .flat_map(|s| s.attempts())
                    .count(),
                1
            );
            assert_eq!(
                diagnostic.required_source().revision(),
                &f.phase.anchor().source
            );
            assert_eq!(
                cold.load_edit_phase_usage(project, run.id()).await?,
                charged
            );
            assert_eq!(inventory(&f.http.root)?, before);
            drop(cold);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug, Default)]
struct StopAfterRead {
    cancelled: std::sync::atomic::AtomicBool,
}
impl AgentControllerControl for StopAfterRead {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
impl AgentSourceReadControl for StopAfterRead {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
impl AgentSourceReader for StopAfterRead {
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            let page = WorkspaceAgentSourceReader
                .read_page(project, revision, request, control)
                .await?;
            self.cancelled.store(true, Ordering::SeqCst);
            Ok(page)
        })
    }
}

#[test]
fn http_correction_activation_cancel_after_real_source_await_preserves_current_replan() -> TestResult
{
    crate::run_native_libsql_test(async {
        let f = applied_fixture().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let mut run = f.run.clone();
        let (mut ledger, mut version) = f.stored.clone().into_parts();
        let stop = StopAfterRead::default();
        let result = ActivateHttpCorrectionEdit::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &stop,
        )
        .execute(
            project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            &mut version,
            &current,
            &f.http.model,
            at(f.run.updated_at().unix_millis() + 1)?,
            &stop,
        )
        .await;
        assert!(matches!(result, Err(HttpCorrectionEditFailure::Cancelled)));
        assert_eq!(run, f.run);
        assert_eq!(ledger, *f.stored.ledger());
        assert_eq!(version, f.stored.version());
        assert_eq!(
            f.http.store.load_agent_run(project, run.id()).await?,
            Some(f.run.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored.clone())
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_applied_origin_rechecks_actual_shared_uncertainty_clock_and_budget() -> TestResult
{
    crate::run_native_libsql_test(async {
        for case in [
            "unobserved",
            "unknown",
            "clock",
            "prompt",
            "output",
            "known",
            "deadline",
        ] {
            let f = applied_fixture().await?;
            let project = &f.http.fixture.project;
            let current = f
                .http
                .store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("index missing")?;
            let db = f.http.fixture.candidate_database().await?;
            let connection = db.connection_for_operation().await?;
            // A separate real retained prior-Execute debit, never overwriting the accepted Plan.
            retained_execute_debit(
                &connection,
                &f.phase,
                &f.run,
                91,
                "initial",
                if case == "prompt" {
                    u32::try_from(f.run.budget().prompt_token_limit().get() + 1)?
                } else {
                    100
                },
                if case == "output" {
                    u32::try_from(f.run.budget().output_token_limit().get() + 1)?
                } else {
                    50
                },
                if case == "unobserved" {
                    None
                } else if case == "unknown" {
                    Some("unknown")
                } else {
                    Some("accepted_reply")
                },
                if case == "clock" {
                    f.run.updated_at().unix_millis() + 2
                } else {
                    81
                },
            )
            .await?;
            let now = if case == "deadline" {
                at(f.run
                    .created_at()
                    .unix_millis()
                    .checked_add(f.run.budget().duration_limit().millis())
                    .ok_or("deadline overflow")?)?
            } else {
                at(f.run.updated_at().unix_millis() + 1)?
            };
            let result = f
                .http
                .store
                .load_applied_http_correction_origin(
                    project,
                    &f.http.goal,
                    &f.stored,
                    &f.run,
                    &current,
                    &f.http.model,
                    now,
                )
                .await;
            match case {
                "unobserved" | "unknown" => assert!(
                    matches!(
                        result,
                        Err(EditPhaseReservationFailure::UncertainProviderCall)
                    ),
                    "{case}"
                ),
                "clock" => assert!(matches!(
                    result,
                    Err(EditPhaseReservationFailure::TimestampRegressed)
                )),
                "prompt" => assert!(matches!(
                    result,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::PromptTokens
                    ))
                )),
                "output" => assert!(matches!(
                    result,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::OutputTokens
                    ))
                )),
                "deadline" => assert!(matches!(
                    result,
                    Err(EditPhaseReservationFailure::DeadlineReached)
                )),
                "known" => {
                    let _ = result?;
                }
                _ => return Err("unexpected origin case".into()),
            }
            assert_eq!(
                f.http.store.load_agent_run(project, f.run.id()).await?,
                Some(f.run.clone())
            );
            assert_eq!(
                f.http
                    .store
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(f.stored.clone())
            );
            drop((
                connection,
                db,
                f.http.compiler,
                f.http.refresh,
                f.http.store,
            ));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug)]
struct RejectActivationCommit;
impl AgentActionStore for RejectActivationCommit {
    fn commit_ledger_action<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskLedgerStoreVersion,
        _: RunEventSequence,
        _: &'a TaskLedger,
        _: &'a AgentRun,
        _: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async { Err(AgentActionStoreFailure::LedgerVersionConflict) })
    }
    fn commit_current_ledger_action<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskLedgerStoreVersion,
        _: RunEventSequence,
        _: &'a TaskLedger,
        _: &'a AgentRun,
        _: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async { Err(AgentActionStoreFailure::LedgerVersionConflict) })
    }
}

#[test]
fn http_correction_activation_final_cas_failure_retains_truthful_plan_prefix() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = applied_fixture().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let mut run = f.run.clone();
        let (mut ledger, mut version) = f.stored.clone().into_parts();
        let result = ActivateHttpCorrectionEdit::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &RejectActivationCommit,
            &WorkspaceAgentSourceReader,
        )
        .execute(
            project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            &mut version,
            &current,
            &f.http.model,
            at(f.run.updated_at().unix_millis() + 1)?,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            result,
            Err(HttpCorrectionEditFailure::Commit(
                AgentActionStoreFailure::LedgerVersionConflict
            ))
        ));
        assert_eq!(run.state(), AgentControllerState::Plan);
        assert_eq!(
            run.last_event_sequence().get(),
            f.run.last_event_sequence().get() + 2
        );
        assert_eq!(ledger, *f.stored.ledger());
        assert_eq!(version, f.stored.version());
        assert!(
            ledger
                .steps()
                .filter(|s| s.is_active_plan_step())
                .all(|s| s.attempts().is_empty())
        );
        assert_eq!(
            f.http.store.load_agent_run(project, run.id()).await?,
            Some(run)
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

struct PublishBeforeActivationCommit<'a> {
    store: &'a LibsqlKnowledgeStore,
    fixture: &'a Fixture,
}
impl std::fmt::Debug for PublishBeforeActivationCommit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishBeforeActivationCommit")
            .finish_non_exhaustive()
    }
}
impl AgentActionStore for PublishBeforeActivationCommit<'_> {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        self.store
            .commit_ledger_action(project, version, sequence, ledger, run, event)
    }
    fn commit_current_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async move {
            fs::write(
                self.fixture
                    .root
                    .join("worktree/activation-publication-race.txt"),
                "new current snapshot\n",
            )
            .map_err(|_| AgentActionStoreFailure::Unavailable)?;
            let (store, refresh, compiler, _) = index_fixture(self.fixture)
                .await
                .map_err(|_| AgentActionStoreFailure::Unavailable)?;
            drop((store, refresh, compiler));
            // Delegate to the real Immediate-TX snapshot/Ledger/Run CAS after the new publication.
            self.store
                .commit_current_ledger_action(project, version, sequence, ledger, run, event)
                .await
        })
    }
}

#[test]
fn http_correction_activation_real_published_snapshot_race_never_starts_correction() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = applied_fixture().await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let before = fs::read(f.http.root.join("server.py"))?;
        let mut run = f.run.clone();
        let (mut ledger, mut version) = f.stored.clone().into_parts();
        let racing = PublishBeforeActivationCommit {
            store: f.http.store.as_ref(),
            fixture: &f.http.fixture,
        };
        let result = ActivateHttpCorrectionEdit::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &racing,
            &WorkspaceAgentSourceReader,
        )
        .execute(
            project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            &mut version,
            &current,
            &f.http.model,
            at(f.run.updated_at().unix_millis() + 1)?,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            result,
            Err(HttpCorrectionEditFailure::Commit(
                AgentActionStoreFailure::SnapshotConflict
            ))
        ));
        assert_eq!(run.state(), AgentControllerState::Plan);
        assert_eq!(
            run.last_event_sequence().get(),
            f.run.last_event_sequence().get() + 2
        );
        assert_eq!(ledger, *f.stored.ledger());
        assert_eq!(version, f.stored.version());
        assert!(
            ledger
                .steps()
                .filter(|s| s.is_active_plan_step())
                .all(|s| s.attempts().is_empty())
        );
        assert_eq!(
            f.http.store.load_agent_run(project, run.id()).await?,
            Some(run)
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        let latest = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("new index missing")?;
        assert_ne!(latest.run().snapshot_id(), current.run().snapshot_id());
        assert_eq!(fs::read(f.http.root.join("server.py"))?, before);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}
