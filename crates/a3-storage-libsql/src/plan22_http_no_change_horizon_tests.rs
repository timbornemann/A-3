//! An actual later horizon after complete controlled B2 HTTP verification.
//! No model quality, automatic planning or completion of the second result is claimed.
use super::*;

const EXTRA_REQUEST: &str = "\nAfter the complete HTTP service is verified, change an explanatory comment in server.py as a separate result. Only the current source diff is required for this final comment result.\n";

fn goal_with_later_original_result() -> TestResult<GoalContract> {
    let old = a04_goal()?;
    let old_original = old.original().ok_or("A04 original")?;
    let mut bytes = old_original.as_bytes().to_vec();
    bytes.extend_from_slice(EXTRA_REQUEST.as_bytes());
    let original = GoalOriginalRequest::try_from_bytes(old.reference(), bytes)?;
    let pending = PendingGoalIntake::new(original, GoalContractTimestamp::from_unix_millis(1)?)?;
    let original = pending.original();
    let mut findings = Vec::new();
    for (index, bounds) in [0, 402, 475, 639, 1207, original.as_bytes().len()]
        .windows(2)
        .enumerate()
    {
        let range = original.range(bounds[0], bounds[1])?;
        let kind = match index {
            0 => GoalIntakeFindingKind::Outcome {
                requirement: AcceptanceCriterionRequirement::Must,
                evidence_requirement: CriterionEvidenceRequirement::LoopbackHttp(a04_recipe()?),
            },
            4 => GoalIntakeFindingKind::Outcome {
                requirement: AcceptanceCriterionRequirement::Must,
                evidence_requirement: CriterionEvidenceRequirement::ChangeOnly(
                    DiffInvariantVerification::new(
                        DiffInvariantMode::ExactPaths,
                        vec![RepositoryPath::try_from_bytes(b"server.py".to_vec())?],
                    )?,
                ),
            },
            _ => GoalIntakeFindingKind::Constraint,
        };
        findings.push(GoalIntakeFinding::new(
            kind,
            range,
            original.resolve(range)?.to_owned(),
        )?);
    }
    let extra = findings.split_off(4);
    let mut review = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            original.range(0, old_original.as_bytes().len())?,
            findings,
            Vec::new(),
        )?,
    )?;
    review.append(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            original.range(old_original.as_bytes().len(), original.as_bytes().len())?,
            extra,
            Vec::new(),
        )?,
    )?;
    let goal = prepare_goal_from_intake_review(&pending, &review)?;
    assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete());
    Ok(goal)
}

async fn closed_with_later_original_result() -> TestResult<HttpEditFixture> {
    let goal = goal_with_later_original_result()?;
    let profile = ollama_profile_with_context("qwen38-4k:latest", HttpFixtureContext::SixteenK)?;
    let f = Box::pin(full_fixture_with_goal(profile, true, Fixture::new()?, goal)).await?;
    let intent = HttpCorrectionPlanIntent::try_from_string(
        "Change only the HTTP handler; retain the complete HTTP proof and later original result"
            .to_owned(),
    )?;
    let (mut f, _) = Box::pin(intent_preflight_current_fixture(f, &intent)).await?;
    let (phase, mut compiled, page, _) = Box::pin(acquire(&mut f)).await?;
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("current")?;
    let provider = NativeEdit {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        store: f.http.store.as_ref(),
        project: &f.http.fixture.project,
        run: f.run.id(),
        reply: json!({"code":page.text()}).to_string(),
        calls: AtomicUsize::new(0),
    };
    assert!(matches!(
        ExecuteEditPhase::new(
            &provider,
            f.http.store.as_ref(),
            &WorkspacePatchAdapter::with_change_window_preview()
        )
        .execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.http.compiler,
                phase,
                &mut compiled,
                state(&f, &current)?,
                after_phases(&f).await?,
                &WorkspacePatchAdapter::with_change_window_preview(),
                &ActiveControl
            ))
            .await?,
            state(&f, &current)?,
            &compiled,
            after_phases(&f).await?,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        )
        .await,
        Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    drop((provider, compiled));
    let (mut ledger, version) = f.stored.clone().into_parts();
    let now = after_phases(&f).await?;
    let version = CloseEditNoChange::new(
        f.http.store.as_ref(),
        f.http.store.as_ref(),
        &WorkspacePatchAdapter::with_change_window_preview(),
    )
    .execute(
        &f.http.fixture.project,
        &f.http.goal,
        &mut f.run,
        &mut ledger,
        version,
        &current,
        &f.http.model,
        &page,
        now,
        &ActiveControl,
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, version);
    drop((current, page));
    reopen(f).await
}

async fn next_horizon_time(fixture: &Fixture, run: &AgentRun) -> TestResult<AgentRunTimestamp> {
    let db = fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let tx = connection
        .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
        .await?;
    let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
        &tx,
        fixture.project.worktree().id(),
        run,
    )
    .await?;
    tx.commit().await?;
    at(latest
        .map_or(run.updated_at().unix_millis(), |time| time.unix_millis())
        .max(run.updated_at().unix_millis())
        .checked_add(1)
        .ok_or("horizon clock")?)
}
fn role(run: AgentRunId, slot: u8) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("a3.native-http-no-change-later-horizon.v1");
    h.update(run.as_bytes());
    h.update(&[slot]);
    *h.finalize().as_bytes()
}
fn later_steps(goal: &GoalContract, run: AgentRunId) -> TestResult<Vec<TaskStepDefinition>> {
    let criterion = goal
        .draft()
        .acceptance_criteria()
        .get(1)
        .ok_or("later original criterion")?
        .id();
    let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
    let change = TaskStepId::from_bytes(role(run, 1));
    let make = |id, verification, outcome: &str, dependencies| -> TestResult<TaskStepDefinition> {
        Ok(TaskStepDefinition::new(
            TaskStepId::from_bytes(role(run, id)),
            None,
            TaskStepOutcome::try_from_string(outcome.to_owned())?,
            TaskStepRationale::try_from_string(
                "Continue the still-unplanned separate original comment result".to_owned(),
            )?,
            dependencies,
            vec![ExpectedTaskEvidence::try_from_string(
                "Current exact-path source diff".to_owned(),
            )?],
            VerificationSpec::diff_invariant(
                VerificationSpecId::from_bytes(role(run, verification)),
                VerificationRequirement::try_from_string(
                    "Only the newly requested server.py comment changes".to_owned(),
                )?,
                DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?,
            ),
        )?)
    };
    Ok(vec![
        make(
            1,
            3,
            "Implement the separately requested explanatory comment",
            Vec::new(),
        )?
        .with_criterion_coverage(vec![criterion], Vec::new())?,
        make(
            2,
            4,
            "Prove the separately requested exact-path comment change",
            vec![StepDependency::new(change)],
        )?
        .with_acceptance_criteria(vec![criterion])?,
    ])
}

async fn actual_later_horizon() -> TestResult {
    let mut f = Box::pin(closed_with_later_original_result()).await?;
    let original = f.http.goal.original().ok_or("original")?.clone();
    let run_id = f.run.id();
    let created = f.run.created_at();
    let budget = f.run.budget();
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let resume = f
        .http
        .store
        .load_http_no_change_replan(
            &f.http.fixture.project,
            &f.http.goal,
            &f.stored,
            &f.run,
            &current,
            &f.http.model,
            after_phases(&f).await?,
        )
        .await?
        .ok_or("A origin")?;
    let page = WorkspaceAgentSourceReader
        .read_page(
            &f.http.fixture.project,
            resume.closed().fact().window().revision(),
            &inspection(resume.closed())?,
            &ActiveControl,
        )
        .await?;
    let phase = PreparedHttpNoChangeReplanV6Job::prepare(
        &f.http.fixture.project,
        &f.http.goal,
        &f.stored,
        &f.run,
        &current,
        f.http.model.clone(),
        resume.closed().clone(),
        &page,
    )?;
    let p = provider(&f, selection(&phase)?, false)?;
    execute_v5(&f, &phase, &p, EditPhaseCallKind::Initial).await?;
    drop(p);
    apply_window(&mut f, &phase).await?;
    drop((current, resume, page, phase));
    let patched = Box::pin(patch_controlled_window(f)).await?;
    eprintln!("native_b2_horizon_stage=patched");
    let profile = patched.profile.clone();
    let (fixture, report) = Box::pin(
        super::super::super::super::functional::finish_http_with_remaining_original_criterion(
            patched.fixture,
            patched.run,
            patched.profile,
            patched.settings,
            patched.floor,
            patched.reviewed,
        ),
    )
    .await?;
    assert_eq!(report["cold_http_verified"], true);
    assert_eq!(report["cold_done"], false);
    assert_eq!(report["owned_starts"], 4);
    assert_eq!(report["total_http_observations"], 7);
    eprintln!("native_b2_horizon_stage=full_http_verified");
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let project = &fixture.project;
    let goal = store
        .load_current_goal_contract(project, original.goal().task_id())
        .await?
        .ok_or("goal after HTTP")?;
    assert_eq!(goal.original(), Some(&original));
    let mut run = store
        .load_agent_run(project, run_id)
        .await?
        .ok_or("same run after HTTP")?;
    let stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("same ledger after HTTP")?;
    let (mut ledger, mut version) = stored.into_parts();
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.created_at(), created);
    assert_eq!(run.budget(), budget);
    let old_history = ledger.steps().cloned().collect::<Vec<_>>();
    let phases = store.load_edit_phase_usage(project, run_id).await?;
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("post-patch publication")?;
    let at = next_horizon_time(&fixture, &run).await?;
    let continuation = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            current.run().snapshot_id(),
            RunEventId::from_bytes(role(run_id, 10)),
            at,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::PlanningRequested {
        ledger_version,
        request_event,
    } = continuation
    else {
        return Err("missing ordinary horizon request".into());
    };
    version = ledger_version;
    let additions = later_steps(&goal, run_id)?;
    let next = additions[0].id();
    let at = next_horizon_time(&fixture, &run).await?;
    version = AppendAgentHorizonPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            current.run().snapshot_id(),
            &request_event,
            additions,
            RunEventId::from_bytes(role(run_id, 11)),
            at,
            &ActiveControl,
        )
        .await?;
    let at = next_horizon_time(&fixture, &run).await?;
    version = ActivateAgentHorizonPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            current.run().snapshot_id(),
            &request_event,
            next,
            RunEventId::from_bytes(role(run_id, 12)),
            at,
            &ActiveControl,
        )
        .await?;
    assert_eq!(run.state(), AgentControllerState::Execute);
    assert_eq!(run.id(), run_id);
    assert_eq!(run.created_at(), created);
    assert_eq!(run.budget(), budget);
    assert!(ledger.pending_planning().is_none());
    for step in old_history {
        assert_eq!(ledger.step(step.definition().id()), Some(&step));
    }
    assert_eq!(store.load_edit_phase_usage(project, run_id).await?, phases);
    eprintln!("native_b2_horizon_stage=later_horizon_activated");
    let mut stored = StoredTaskLedger::new(ledger, version);
    let seed = TaskLensSeedText::try_from_string(
        "Implement the remaining explicit source comment result".to_owned(),
    )?;
    let lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref())
        .execute(
            project,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![TaskLensSeed::ExplicitPath(RepositoryPath::try_from_bytes(
                    b"server.py".to_vec(),
                )?)],
            )?,
            TaskLensTokenBudget::new(4096)?,
            &ActiveControl,
        )
        .await?;
    let handlers = current
        .publication()
        .graph()
        .symbols()
        .iter()
        .filter(|symbol| symbol.parsed().name().as_str() == "do_GET")
        .collect::<Vec<_>>();
    let [handler] = handlers.as_slice() else {
        return Err("current later handler".into());
    };
    let required = EvidenceRef::new(
        handler.revision().clone(),
        handler.parsed().declaration_range(),
    );
    let bound = BoundEditSource::for_file_span(project, &current, &lens, &required)?;
    let edit_state = EditPhaseState::new(project, &goal, &stored, &run, &current)?;
    let tool = ToolRunId::from_bytes(role(run_id, 20));
    eprintln!("native_b2_horizon_stage=ordinary_read_admission");
    store
        .begin_source_read(
            project,
            edit_state.anchor(),
            &bound,
            tool,
            next_horizon_time(&fixture, &run).await?,
        )
        .await?;
    let reader = DeterministicAgentReadTools::new(
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let mut result = reader
        .execute(
            project,
            current.run().snapshot_id(),
            tool,
            &bound.read_action(project, &current)?,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = bound.take_original(project, &current, tool, &mut result)?;
    let sequence = run.last_event_sequence();
    let at = next_horizon_time(&fixture, &run).await?;
    let recorded = result.record(&mut run, RunEventId::from_bytes(role(run_id, 21)), at)?;
    AppendAgentRead::new(store.as_ref())
        .execute(project, sequence, &run, &recorded)
        .await?;
    let edit_state = EditPhaseState::new(project, &goal, &stored, &run, &current)?;
    let reads = store
        .load_source_read_state(project, edit_state.anchor())
        .await?;
    let acquired = reads.acquired().ok_or("ordinary later Original acquired")?;
    assert!(reads.progress().receipts().is_empty());
    let phase = PreparedEditPhase::update(edit_state, &page)?;
    let mut compiled = build_edit_phase_request(
        &goal,
        &stored,
        &phase,
        edit_state,
        profile.clone(),
        None,
        EditPhasePrerequisites::none().with_required_source(
            EditEvidenceNeed::Definition,
            &bound,
            Some((&page, acquired)),
        ),
    )?;
    let provider=NativeEdit{encoder:OllamaModelProvider::for_projected_wire(OllamaEndpoint::parse("http://127.0.0.1:11434")?,Arc::new(LocalOnlyOllamaEndpointPolicy))?,
        store:store.as_ref(),project,run:run_id,reply:json!({"code":format!("    # A separate requested comment after HTTP verification.\n{}",page.text())}).to_string(),calls:AtomicUsize::new(0)};
    let files = inventory(project.worktree().root().as_path())?;
    let report = ExecuteEditPhase::new(
        &provider,
        store.as_ref(),
        &WorkspacePatchAdapter::with_change_window_preview(),
    )
    .execute(
        Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
            phase,
            &mut compiled,
            edit_state,
            next_horizon_time(&fixture, &run).await?,
            &WorkspacePatchAdapter::with_change_window_preview(),
            &ActiveControl,
        ))
        .await?,
        edit_state,
        &compiled,
        next_horizon_time(&fixture, &run).await?,
        ModelRequestTimeout::DEFAULT,
        &ActiveControl,
    )
    .await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(report.outcome, EditPhaseOutcome::Proposal(_)));
    assert!(report.source_delivery.is_some());
    let after = store.load_edit_phase_usage(project, run_id).await?;
    assert_eq!(after.calls, phases.calls + 1);
    assert_eq!(after.repairs, phases.repairs);
    assert_eq!((after.unobserved_calls, after.unknown_calls), (0, 0));
    let db = fixture.candidate_database().await?;
    let conn = db.connection_for_operation().await?;
    let mut v5 = conn
        .query(
            "SELECT COUNT(*) FROM http_no_change_replan_reservations WHERE run_id=?1",
            libsql::params![run_id.as_bytes().to_vec()],
        )
        .await?;
    assert_eq!(v5.next().await?.ok_or("V5 count")?.get::<i64>(0)?, 1);
    assert_eq!(inventory(project.worktree().root().as_path())?, files);
    assert_eq!(
        store
            .load_current_goal_contract(project, goal.task_id())
            .await?
            .ok_or("unchanged original")?
            .original(),
        Some(&original)
    );
    assert_eq!(
        store
            .load_agent_run(project, run_id)
            .await?
            .ok_or("same final run")?
            .budget(),
        budget
    );
    drop((
        v5, conn, db, report, provider, compiled, page, reads, bound, lens, current, recorded,
    ));
    stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("final stored")?;
    assert_eq!(stored.ledger().revision(), run.task_ledger_revision());
    drop(store);
    fixture.cleanup()
}

#[test]
fn http_no_change_v5_completed_http_continues_real_later_horizon_with_ordinary_read_and_edit()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(Box::pin(actual_later_horizon()))
    })
}
