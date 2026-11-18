//! Native candidate plan application only; no automatic admission, activation or tool execution.
use super::*;
#[path = "plan22_http_correction_apply_boundary_tests.rs"]
mod boundaries;

async fn settle_focus(f: &CurrentPlanFixture, decision: &str) -> TestResult {
    let permit = f
        .http
        .store
        .reserve_http_correction_plan(
            &f.http.fixture.project,
            &f.phase,
            EditPhaseCallKind::Initial,
            wire(&f.phase, EditPhaseCallKind::Initial)?,
            at(81)?,
        )
        .await?;
    let proposal = f.phase.decode_proposal(&reply(&f.phase, decision)?)?;
    f.http
        .store
        .finish_http_correction_plan(
            &f.http.fixture.project,
            &f.phase,
            permit,
            observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
            Some(proposal),
        )
        .await?;
    Ok(())
}

#[test]
fn http_correction_apply_persists_only_material_plan_and_cold_replan() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        settle_focus(&f, "L6").await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let mut run = f.run.clone();
        let (mut ledger, version) = f.stored.clone().into_parts();
        let old_failed = ledger
            .step(f.phase.anchor().step)
            .ok_or("old failed missing")?
            .clone();
        let old_recipe = old_failed.definition().verification_spec().target().clone();
        let old_usage = run.usage();
        let old_budget = run.budget();
        let old_original = f.http.goal.original().ok_or("original missing")?.clone();
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        let HttpCorrectionPlanDecision::FocusLine(focus) = proposal.decision() else {
            return Err("controlled applied proposal lost its focus".into());
        };
        let planned = derive_http_correction_material_plan(
            f.phase.anchor(),
            f.phase.job_digest(),
            old_failed.definition(),
            focus,
            proposal.intent(),
        )?;
        let different_intent = HttpCorrectionPlanIntent::try_from_string(
            "Preserve routing while correcting the selected response body".to_owned(),
        )?;
        let alternative = derive_http_correction_material_plan(
            f.phase.anchor(),
            f.phase.job_digest(),
            old_failed.definition(),
            focus,
            &different_intent,
        )?;
        assert_eq!(
            planned.change().intended_outcome().as_str(),
            proposal.intent().as_str()
        );
        assert_eq!(
            alternative.change().intended_outcome().as_str(),
            different_intent.as_str()
        );
        assert_ne!(planned.change().id(), alternative.change().id());
        assert_ne!(planned.proof().id(), alternative.proof().id());
        assert_ne!(
            planned.change().verification_spec().id(),
            alternative.change().verification_spec().id()
        );
        assert_ne!(
            planned.proof().verification_spec().id(),
            alternative.proof().verification_spec().id()
        );
        assert_ne!(planned.event_id(), alternative.event_id());
        for (actual, other) in planned
            .activation_events()
            .iter()
            .zip(alternative.activation_events())
        {
            assert_ne!(actual, other);
        }
        assert_eq!(
            planned.change().addressed_criteria(),
            alternative.change().addressed_criteria()
        );
        assert_eq!(
            planned.proof().acceptance_criteria(),
            alternative.proof().acceptance_criteria()
        );
        assert_eq!(
            planned.proof().verification_spec().target(),
            alternative.proof().verification_spec().target()
        );
        assert_eq!(
            alternative.proof().dependencies(),
            [StepDependency::new(alternative.change().id())]
        );
        let before = inventory(&f.http.root)?;
        let charged = f
            .http
            .store
            .load_edit_phase_usage(project, run.id())
            .await?;
        let applied = ApplyHttpCorrectionPlan::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            &WorkspaceAgentSourceReader,
        )
        .execute(
            project,
            &f.http.goal,
            &mut run,
            &mut ledger,
            version,
            &current,
            &f.phase,
            at(83)?,
            &ActiveControl,
        )
        .await?;
        assert_eq!(run.id(), f.run.id());
        assert_eq!(run.state(), AgentControllerState::Replan);
        assert_eq!(run.created_at(), f.run.created_at());
        assert_eq!(run.budget(), old_budget);
        assert_eq!(run.usage(), old_usage);
        assert_eq!(run.current_snapshot_id(), f.run.current_snapshot_id());
        assert_eq!(
            ledger.steps().count(),
            f.stored.ledger().steps().count() + 2
        );
        let failed = ledger
            .step(old_failed.definition().id())
            .ok_or("retired failure missing")?;
        assert!(!failed.is_active_plan_step());
        assert_eq!(failed.definition(), old_failed.definition());
        assert_eq!(failed.attempts(), old_failed.attempts());
        let change = ledger.step(applied.correction).ok_or("change missing")?;
        let proof = ledger.step(applied.proof).ok_or("proof missing")?;
        assert_eq!(change.definition(), planned.change());
        assert_eq!(proof.definition(), planned.proof());
        assert_eq!(
            change.definition().intended_outcome().as_str(),
            proposal.intent().as_str()
        );
        assert!(change.attempts().is_empty() && proof.attempts().is_empty());
        assert_eq!(change.status(), TaskStepStatus::Ready);
        assert_eq!(
            change.definition().addressed_criteria(),
            old_failed.definition().acceptance_criteria()
        );
        assert!(change.definition().acceptance_criteria().is_empty());
        assert_eq!(proof.definition().verification_spec().target(), &old_recipe);
        assert_eq!(
            proof.definition().acceptance_criteria(),
            old_failed.definition().acceptance_criteria()
        );
        assert_eq!(
            proof.definition().dependencies(),
            &[StepDependency::new(applied.correction)]
        );
        assert_eq!(inventory(&f.http.root)?, before);
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(project, run.id())
                .await?,
            charged
        );
        let retained = f
            .http
            .store
            .load_current_goal_contract(project, f.http.goal.task_id())
            .await?
            .ok_or("goal missing")?;
        assert_eq!(retained.original(), Some(&old_original));
        let committed_run = run.clone();
        let committed_ledger = ledger.clone();
        assert!(
            ApplyHttpCorrectionPlan::new(
                f.http.store.as_ref(),
                f.http.store.as_ref(),
                &WorkspaceAgentSourceReader
            )
            .execute(
                project,
                &f.http.goal,
                &mut run,
                &mut ledger,
                applied.version,
                &current,
                &f.phase,
                at(84)?,
                &ActiveControl
            )
            .await
            .is_err()
        );
        assert_eq!(run, committed_run);
        assert_eq!(ledger, committed_ledger);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        let cold_goal = reopened
            .load_current_goal_contract(project, f.http.goal.task_id())
            .await?
            .ok_or("cold goal missing")?;
        assert_eq!(cold_goal.original(), Some(&old_original));
        assert!(GoalOriginalCoverage::derive_from(&cold_goal)?.is_complete());
        assert_eq!(reopened.load_agent_run(project, run.id()).await?, Some(run));
        let cold = reopened
            .load_task_ledger(project, f.http.goal.task_id())
            .await?
            .ok_or("cold ledger missing")?;
        assert_eq!(cold.ledger(), &ledger);
        assert_eq!(cold.version(), applied.version);
        assert_eq!(
            cold.ledger()
                .step(applied.correction)
                .ok_or("cold correction missing")?
                .definition()
                .intended_outcome()
                .as_str(),
            proposal.intent().as_str()
        );
        // This ordinary cold read is not a resumable correction-origin capability.
        assert!(
            cold.ledger()
                .steps()
                .filter(|step| step.is_active_plan_step())
                .all(|step| step.attempts().is_empty())
        );
        drop(reopened);
        f.http.fixture.cleanup()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceReadPublication {
    ChangedSnapshot,
    SameSnapshot,
}

pub(super) struct PublishAfterSourceRead<'a> {
    pub(super) fixture: &'a Fixture,
    pub(super) reads: AtomicUsize,
    pub(super) publish_on: usize,
    pub(super) publication: SourceReadPublication,
    pub(super) ranges: std::sync::Mutex<[Option<SourceRange>; 2]>,
}
impl std::fmt::Debug for PublishAfterSourceRead<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublishAfterSourceRead")
            .finish_non_exhaustive()
    }
}
impl AgentSourceReader for PublishAfterSourceRead<'_> {
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
            let index = self.reads.fetch_add(1, Ordering::SeqCst);
            {
                let mut ranges = self
                    .ranges
                    .lock()
                    .map_err(|_| AgentSourceReadFailure::Unavailable)?;
                *ranges
                    .get_mut(index)
                    .ok_or(AgentSourceReadFailure::Denied)? = Some(page.range());
            }
            if index + 1 != self.publish_on {
                return Ok(page);
            }
            if self.publication == SourceReadPublication::SameSnapshot {
                // Test-fixture failure must be visible before judging the product's Apply result.
                // Do not map a failed publication into an apparently stale product source.
                let publication = republish_unchanged_index(self.fixture).await;
                assert!(
                    publication.is_ok(),
                    "actual unchanged-source publication fixture failed: {publication:?}"
                );
                return Ok(page);
            }
            let marker = self.fixture.root.join("worktree/source-race-marker.txt");
            if marker.exists() {
                return Err(AgentSourceReadFailure::Denied);
            }
            fs::write(marker, "new published worktree state\n")
                .map_err(|_| AgentSourceReadFailure::Unavailable)?;
            // Real snapshot/index publication after the binder has loaded its current proposal
            // and after the source was safely read, immediately before the actual commit.
            let (store, refresh, compiler, _) = index_fixture(self.fixture)
                .await
                .map_err(|_| AgentSourceReadFailure::Unavailable)?;
            drop((store, refresh, compiler));
            Ok(page)
        })
    }
}

// Publish a controlled TEST-only ranking revision of the actual unchanged compiler result.
// The real publisher forbids a second published run for the same Snapshot+ranking policy.
// Revision2 deliberately retains the actual v1 symbol scores; it is not a production ranker.
// All graph/module/analysis/manifest/flow data and every source byte remain unchanged.
// Domain constructors and real IndexStore Start/Publish are used, never SQL row faults.
pub(super) async fn republish_unchanged_index(fixture: &Fixture) -> TestResult {
    let before = inventory(&fixture.root.join("worktree"))?;
    let (store, refresh, compiler, indexed) =
        index_fixture_refresh(fixture, &ActiveControl).await?;
    assert!(
        !indexed.published(),
        "unchanged refresh unexpectedly published"
    );
    let prior = indexed.published_index().run();
    assert_eq!(indexed.snapshot().id(), prior.snapshot_id());
    let compiled = indexed.compilation().publication();
    let test_policy = a3_domain::RankingPolicyVersion::new(
        prior
            .ranking_policy_version()
            .get()
            .checked_add(1)
            .ok_or("test ranking revision overflow")?,
    )?;
    let publication = a3_domain::IndexPublication::new_with_file_analyses(
        compiled.graph().clone(),
        a3_domain::RankProjection::new(
            prior.snapshot_id(),
            test_policy,
            compiled.ranking().symbols().to_vec(),
        )?,
        compiled.manifest_files().to_vec(),
        compiled.modules().clone(),
        compiled.file_analyses().to_vec(),
    )?;
    assert_eq!(publication.graph(), compiled.graph());
    assert_eq!(publication.modules(), compiled.modules());
    assert_eq!(publication.manifest_files(), compiled.manifest_files());
    assert_eq!(publication.file_analyses(), compiled.file_analyses());
    assert_eq!(
        publication.ranking().symbols(),
        compiled.ranking().symbols()
    );
    let sequence = store.next_index_run_sequence(&fixture.project).await?;
    let id = a3_application::IndexRunIdFactory::create(
        &Blake3IndexRunIdFactory,
        &fixture.project,
        indexed.snapshot(),
        test_policy,
        sequence.get(),
    )?;
    assert_ne!(id, prior.id());
    let run = store
        .start_index_run(
            &fixture.project,
            a3_domain::IndexRunStart::new(id, prior.snapshot_id(), test_policy, sequence),
        )
        .await?;
    let committed = store
        .publish_index_with_flows(
            &fixture.project,
            run.id(),
            &publication,
            indexed.compilation().function_flows(),
            &ActiveControl,
        )
        .await?;
    assert_eq!(committed.id(), id);
    assert_eq!(committed.snapshot_id(), prior.snapshot_id());
    assert_eq!(committed.ranking_policy_version(), test_policy);
    assert_eq!(
        store.latest_published_index_run(&fixture.project).await?,
        Some(committed)
    );
    let published = store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or("republished index missing")?;
    assert_eq!(published.run().id(), id);
    assert_eq!(published.run().snapshot_id(), prior.snapshot_id());
    assert_eq!(published.run().ranking_policy_version(), test_policy);
    assert_eq!(published.publication(), &publication);
    assert_eq!(inventory(&fixture.root.join("worktree"))?, before);
    drop((published, indexed, store, refresh, compiler));
    Ok(())
}

#[test]
fn http_correction_apply_real_publication_race_rejects_atomically() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        settle_focus(&f, "L6").await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let source_before = fs::read(f.http.root.join("server.py"))?;
        let mut run = f.run.clone();
        let (mut ledger, version) = f.stored.clone().into_parts();
        let reader = PublishAfterSourceRead {
            fixture: &f.http.fixture,
            reads: AtomicUsize::new(0),
            publish_on: 1,
            publication: SourceReadPublication::ChangedSnapshot,
            ranges: std::sync::Mutex::new([None, None]),
        };
        let result =
            ApplyHttpCorrectionPlan::new(f.http.store.as_ref(), f.http.store.as_ref(), &reader)
                .execute(
                    project,
                    &f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &f.phase,
                    at(83)?,
                    &ActiveControl,
                )
                .await;
        assert!(matches!(
            result,
            Err(HttpCorrectionPlanApplyFailure::Replan(
                ApplyAgentPlanRevisionError::Storage(AgentActionStoreFailure::SnapshotConflict)
            ))
        ));
        assert_eq!(reader.reads.load(Ordering::SeqCst), 1);
        let latest = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("new index missing")?;
        assert_ne!(latest.run().snapshot_id(), current.run().snapshot_id());
        assert_eq!(run, f.run);
        assert_eq!(&ledger, f.stored.ledger());
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
        assert_eq!(fs::read(f.http.root.join("server.py"))?, source_before);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[derive(Debug)]
struct CancelApply;
impl AgentControllerControl for CancelApply {
    fn is_cancelled(&self) -> bool {
        true
    }
}
impl AgentSourceReadControl for CancelApply {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub(super) struct RejectPlanCommit;
impl AgentActionStore for RejectPlanCommit {
    fn commit_current_index_ledger_action<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskLedgerStoreVersion,
        _: RunEventSequence,
        _: a3_domain::IndexRunId,
        _: &'a TaskLedger,
        _: &'a AgentRun,
        _: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async { Err(AgentActionStoreFailure::LedgerVersionConflict) })
    }

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
fn http_correction_apply_rejects_halts_unsettled_source_clock_cancel_and_cas() -> TestResult {
    crate::run_native_libsql_test(async {
        for case in [
            "needEvidence",
            "tooLarge",
            "refuse",
            "unsettled",
            "source",
            "clock",
            "deadline",
            "cancel",
            "cas",
        ] {
            let f = current_fixture().await?;
            if case != "unsettled" {
                settle_focus(
                    &f,
                    if ["needEvidence", "tooLarge", "refuse"].contains(&case) {
                        case
                    } else {
                        "L6"
                    },
                )
                .await?;
            }
            let project = &f.http.fixture.project;
            let current = f
                .http
                .store
                .latest_published_index(project, &ActiveControl)
                .await?
                .ok_or("index missing")?;
            if case == "source" {
                fs::write(f.http.root.join("server.py"), "# concurrent user edit\n")?;
            }
            let before = inventory(&f.http.root)?;
            let mut run = f.run.clone();
            let (mut ledger, version) = f.stored.clone().into_parts();
            let result = match case {
                "cancel" => {
                    ApplyHttpCorrectionPlan::new(
                        f.http.store.as_ref(),
                        f.http.store.as_ref(),
                        &WorkspaceAgentSourceReader,
                    )
                    .execute(
                        project,
                        &f.http.goal,
                        &mut run,
                        &mut ledger,
                        version,
                        &current,
                        &f.phase,
                        at(83)?,
                        &CancelApply,
                    )
                    .await
                }
                "cas" => {
                    ApplyHttpCorrectionPlan::new(
                        f.http.store.as_ref(),
                        &RejectPlanCommit,
                        &WorkspaceAgentSourceReader,
                    )
                    .execute(
                        project,
                        &f.http.goal,
                        &mut run,
                        &mut ledger,
                        version,
                        &current,
                        &f.phase,
                        at(83)?,
                        &ActiveControl,
                    )
                    .await
                }
                _ => {
                    ApplyHttpCorrectionPlan::new(
                        f.http.store.as_ref(),
                        f.http.store.as_ref(),
                        &WorkspaceAgentSourceReader,
                    )
                    .execute(
                        project,
                        &f.http.goal,
                        &mut run,
                        &mut ledger,
                        version,
                        &current,
                        &f.phase,
                        at(if case == "clock" {
                            81
                        } else if case == "deadline" {
                            f.run.created_at().unix_millis()
                                + f.run.budget().duration_limit().millis()
                        } else {
                            83
                        })?,
                        &ActiveControl,
                    )
                    .await
                }
            };
            assert!(result.is_err(), "{case}");
            assert_eq!(run, f.run, "{case}");
            assert_eq!(&ledger, f.stored.ledger(), "{case}");
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
            assert_eq!(inventory(&f.http.root)?, before);
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug)]
pub(super) struct DelayedHttpPlanSourceRead {
    pub(super) reads: AtomicUsize,
    pub(super) delay_on: usize,
    pub(super) ranges: std::sync::Mutex<[Option<SourceRange>; 2]>,
}

impl AgentSourceReader for DelayedHttpPlanSourceRead {
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
            let index = self.reads.fetch_add(1, Ordering::SeqCst);
            {
                let mut ranges = self
                    .ranges
                    .lock()
                    .map_err(|_| AgentSourceReadFailure::Unavailable)?;
                *ranges
                    .get_mut(index)
                    .ok_or(AgentSourceReadFailure::Denied)? = Some(page.range());
            }
            // Delay only the selected actual source. Direct retains read1; Context targets S1/read2.
            if index + 1 == self.delay_on {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(page)
        })
    }
}

#[test]
fn http_correction_apply_source_revalidation_cannot_backdate_a_deadline_expired_commit()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        settle_focus(&f, "L6").await?;
        let project = &f.http.fixture.project;
        let current = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        let before = inventory(&f.http.root)?;
        let charged = f
            .http
            .store
            .load_edit_phase_usage(project, f.run.id())
            .await?;
        let events = f
            .http
            .store
            .load_run_events(project, f.run.id(), None, RunEventPageLimit::new(256)?)
            .await?;
        let reader = DelayedHttpPlanSourceRead {
            reads: AtomicUsize::new(0),
            delay_on: 1,
            ranges: std::sync::Mutex::new([None, None]),
        };
        let mut run = f.run.clone();
        let (mut ledger, version) = f.stored.clone().into_parts();
        let just_before_deadline =
            f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis() - 1;
        let result =
            ApplyHttpCorrectionPlan::new(f.http.store.as_ref(), f.http.store.as_ref(), &reader)
                .execute(
                    project,
                    &f.http.goal,
                    &mut run,
                    &mut ledger,
                    version,
                    &current,
                    &f.phase,
                    at(just_before_deadline)?,
                    &ActiveControl,
                )
                .await;
        assert_eq!(reader.reads.load(Ordering::SeqCst), 1);
        assert!(
            matches!(result, Err(HttpCorrectionPlanApplyFailure::BudgetExhausted)),
            "expected deadline halt after delayed source; got {result:?}; run_updated_at={:?}; ledger_revision={:?}",
            run.updated_at(),
            ledger.revision(),
        );
        assert_eq!(run, f.run);
        assert_eq!(&ledger, f.stored.ledger());
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
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(project, run.id())
                .await?,
            charged
        );
        let retained = f
            .http
            .store
            .load_run_events(project, run.id(), None, RunEventPageLimit::new(256)?)
            .await?;
        assert_eq!(retained.events(), events.events());
        assert_eq!(inventory(&f.http.root)?, before);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}
