//! Actual guarded A01 failure and deterministic Source correction after a cold reopen.

#[path = "plan22_p5_live_source_correction.rs"]
mod live_correction;

use super::*;
use a3_application::{
    AdvanceAgentController, AgentSourcePage, AgentSourceReader, ApplySourceTestCorrection,
    EditPhaseOutcome, EditPhasePrerequisites, EditPhaseState, MutationFailureClass,
    PreparedEditPhase, RunEventPageLimit, SourceTestCorrectionFailure, build_edit_phase_request,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, ContentHash, RunEventCode,
    RunEventKind, RunEventOutcome, RunEventSubject, TaskLedgerReplanCause, VerificationTarget,
};
use a3_workspace::WorkspaceAgentSourceReader;
use serde_json::json;

const INCORRECT_LINE: &str = "    return value + 3\n";
const CORRECT_LINE: &str = "    return value + 1\n";
const FAILURE_REASON: &str = "Python unittest failed (1/1). An assertion compared 44 and 42.";

#[test]
fn guarded_source_unittest_red_replans_and_retains_the_exact_attempt_after_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = prepare_manifest_bound_fixture()?;
            let worktree = fixture.root.join("worktree");
            let expected = inventory_with_subject(&worktree, INCORRECT)?;
            exercise_guarded_red(&fixture, profile()?, a3_domain::AgentRunBudget::DEFAULT).await?;
            assert_eq!(inventory(&worktree)?, expected);
            fixture.cleanup()
        })
    })
}

#[test]
fn guarded_source_unittest_red_decomposes_and_fixed_edit_reaches_current_done() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = prepare_manifest_bound_fixture()?;
            let worktree = fixture.root.join("worktree");
            let expected = inventory_with_subject(&worktree, CORRECT)?;
            let profile = profile()?;
            let run = exercise_guarded_red(
                &fixture,
                profile.clone(),
                a3_domain::AgentRunBudget::DEFAULT,
            )
            .await?;
            exercise_correction(&fixture, run, profile, None).await?;
            assert_eq!(inventory(&worktree)?, expected);
            fixture.cleanup()
        })
    })
}

fn inventory_with_subject(
    worktree: &Path,
    replacement: &[u8],
) -> TestResult<Vec<(PathBuf, Vec<u8>)>> {
    let mut expected = inventory(worktree)?;
    let (_, subject) = expected
        .iter_mut()
        .find(|(path, _)| path == Path::new(SUBJECT_PATH))
        .ok_or("subject missing from original inventory")?;
    *subject = replacement.to_vec();
    Ok(expected)
}

async fn exercise_guarded_red(
    fixture: &Fixture,
    profile: ModelProfile,
    budget: a3_domain::AgentRunBudget,
) -> TestResult<AgentRunId> {
    let project = &fixture.project;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let catalog = DiscoverProjectCommands.execute_plan22(project.worktree().id(), &published)?;
    let command = manifest_test_command(&catalog)?;
    let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
    let translation = PythonIntegerTranslation::new(
        path.clone(),
        SymbolName::try_from_string("adjust".to_owned())?,
        1,
    )?;
    let (goal, definitions) = manifest_bound_goal_and_steps(command, &path, Some(translation))?;
    let [_, proof] = definitions.as_slice() else {
        return Err("guarded proof envelope missing".into());
    };
    let combined = TaskStepDefinition::new(
        CHANGE_STEP,
        None,
        TaskStepOutcome::try_from_string(
            "Correct adjust and run the existing unittest".to_owned(),
        )?,
        proof.rationale().clone(),
        Vec::new(),
        proof.expected_evidence().to_vec(),
        proof.verification_spec().clone(),
    )?
    .with_acceptance_criteria(proof.acceptance_criteria().to_vec())?;
    let (mut run, stored, open) = start_fixture_step_with_budget(
        fixture,
        &store,
        &goal,
        &profile,
        base,
        vec![combined],
        (budget, at(3)?),
    )
    .await?;
    assert!(open.is_empty());
    let budget = run.budget();
    let usage = run.usage();

    let page = read_subject_line(project, &published, &path).await?;
    assert_eq!(page.text(), "    return value + (2)\n");
    let state = EditPhaseState::new(project, &goal, &stored, &run, &published)?;
    let phase = PreparedEditPhase::update(state, &page)?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let action = complete_fixed_edit(phase, state, &patch, INCORRECT_LINE).await?;
    let AgentAction::ApplyPatch(proposal) = &action else {
        return Err("expected one existing E3 patch".into());
    };
    let policy = proposal.policy_action();

    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let runner = native_runner()?;
    let verifier = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approvals,
        &patch,
        &runner,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref())
    .with_python_integer_translation_verifier(&verifier)
    .with_source_failure_replan(&goal);
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let (mut ledger, mut version) = stored.into_parts();
    let outcome = execute_approved_observing(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        action,
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let pending = approvals
                .presentation(project, goal.task_id())?
                .ok_or("patch approval missing")?;
            assert_eq!(pending.request_id(), request);
            assert_eq!(pending.action_fingerprint(), policy.fingerprint());
            assert_eq!(pending.scope_digest(), policy.scope_digest());
            let overview = inspection
                .overview(project, goal.task_id())?
                .ok_or("patch preview inspection missing")?;
            let diff = overview.patch().ok_or("patch preview missing")?;
            assert_eq!(diff.context(), pending.context());
            assert_eq!(diff.context().snapshot_id(), base);
            let [file] = diff.files() else {
                return Err("expected one approved file change".into());
            };
            assert_eq!(file.source_path(), Some(&path));
            assert_eq!(file.target_path(), Some(&path));
            let before = file.before().ok_or("preview before missing")?;
            let after = file.after().ok_or("preview after missing")?;
            assert_eq!(before.text(), "    return value + (2)\n");
            assert_eq!(after.text(), INCORRECT_LINE);
            assert_eq!(
                before.content_hash(),
                ContentHash::from_bytes(*blake3::hash(INITIAL).as_bytes()),
            );
            assert_eq!(
                after.content_hash(),
                ContentHash::from_bytes(*blake3::hash(INCORRECT).as_bytes()),
            );
            assert_eq!(
                fs::read(fixture.root.join("worktree").join(SUBJECT_PATH))?,
                INITIAL
            );
            Ok(())
        },
    )
    .await?;
    let MutationControllerOutcome::BoundVerificationReady {
        command: selected,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("guarded Apply did not bind its check: {outcome:?}").into());
    };
    assert_eq!(selected.command_id(), command);
    assert_eq!(selected.step_id(), CHANGE_STEP);
    assert_ne!(snapshot_id, base);
    assert_eq!(run.current_snapshot_id(), snapshot_id);
    assert_eq!(run.state(), AgentControllerState::Execute);
    assert_eq!(
        ledger
            .step(CHANGE_STEP)
            .ok_or("combined step missing")?
            .status(),
        TaskStepStatus::InProgress,
    );
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "event capture poisoned")?
            .is_empty()
    );

    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("patched publication missing")?;
    assert_eq!(current.run().snapshot_id(), snapshot_id);
    let current_catalog =
        DiscoverProjectCommands.execute_plan22(project.worktree().id(), &current)?;
    assert_eq!(manifest_test_command(&current_catalog)?, command);
    let confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(project, &current_catalog, vec![command], at(15)?, None)
        .await?;
    let outcome = execute_approved_observing(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(selected),
        Some((&current_catalog, &confirmation)),
        100,
        16,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let pending = approvals
                .presentation(project, goal.task_id())?
                .ok_or("unittest approval missing")?;
            assert_eq!(pending.request_id(), request);
            assert_eq!(pending.context().snapshot_id(), snapshot_id);
            let AgentApprovalAction::Process(spec) = pending.action() else {
                return Err("expected exact unittest approval".into());
            };
            assert_eq!(spec.executable(), "python");
            assert_eq!(
                spec.arguments()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                TEST_ARGV
            );
            assert_eq!(
                spec.working_directory(),
                &AgentApprovalWorkingDirectory::Root
            );
            assert_eq!(spec.network(), AgentApprovalNetworkScope::Denied);
            assert_eq!(spec.timeout_millis(), 30_000);
            assert!(
                events
                    .values
                    .lock()
                    .map_err(|_| "event capture poisoned")?
                    .is_empty()
            );
            Ok(())
        },
    )
    .await?;
    assert!(matches!(
        outcome,
        MutationControllerOutcome::ReplanRequired {
            failure: MutationFailureClass::VerificationFailed,
            snapshot_id: failed_snapshot,
        } if failed_snapshot == snapshot_id
    ));
    assert_eq!(run.state(), AgentControllerState::Replan);
    assert_eq!(run.budget(), budget);
    assert_eq!(run.usage(), usage);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    let overview = inspection
        .overview(project, goal.task_id())?
        .ok_or("actual unittest inspection missing")?;
    let [process] = overview.processes() else {
        return Err("expected exactly one actual unittest".into());
    };
    assert!(matches!(
        process.termination(),
        ProcessTermination::Exited(exit) if exit.code() == Some(1)
    ));
    assert_eq!(process.context().snapshot_id(), snapshot_id);
    drop(refresh);
    drop(store);

    assert_reopened_red(fixture, &goal, &run, &ledger, version).await?;
    Ok(run.id())
}

async fn read_subject_line(
    project: &ProjectIdentity,
    published: &PublishedIndex,
    path: &RepositoryPath,
) -> TestResult<AgentSourcePage> {
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path() == path)
        .ok_or("subject missing from current publication")?;
    Ok(WorkspaceAgentSourceReader
        .read_page(
            project,
            revision,
            &AgentFileInspection::new(
                path.clone(),
                AgentFileStartLine::new(2)?,
                AgentFileLineCount::new(1)?,
            ),
            &ActiveControl,
        )
        .await?)
}

async fn complete_fixed_edit(
    phase: PreparedEditPhase,
    state: EditPhaseState<'_>,
    patch: &WorkspacePatchAdapter,
    replacement: &str,
) -> TestResult<AgentAction> {
    let reply = serde_json::to_vec(&json!({
        "code": replacement,
    }))?;
    let reply = phase.bind_reply(&reply)?;
    let EditPhaseOutcome::Proposal(action) =
        phase.complete(reply, state, patch, &ActiveControl).await?
    else {
        return Err("fixed edit did not compile to a patch proposal".into());
    };
    Ok(action)
}

async fn exercise_correction(
    fixture: &Fixture,
    run_id: AgentRunId,
    profile: ModelProfile,
    live: Option<&live_correction::LiveCorrection>,
) -> TestResult {
    let observed_at = |minimum| live.map_or_else(|| at(minimum), |live| live.observed_at(minimum));
    let project = &fixture.project;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let mut run = store
        .load_agent_run(project, run_id)
        .await?
        .ok_or("red Run missing for correction")?;
    let goal = store
        .load_current_goal_contract(project, run.goal_contract().task_id())
        .await?
        .ok_or("red Goal missing for correction")?;
    let (mut ledger, version) = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("red Ledger missing for correction")?
        .into_parts();
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("red publication missing for correction")?;
    let failed = ledger
        .step(CHANGE_STEP)
        .ok_or("combined step missing")?
        .clone();
    let budget = run.budget();
    let usage = run.usage();
    let previous_revision = ledger.revision();
    let red_snapshot = run.current_snapshot_id();
    let started = run.created_at();
    let correction = ApplySourceTestCorrection::new(&store, &store);
    let deadline = started.unix_millis() + budget.duration_limit().millis();
    assert!(matches!(
        correction
            .execute(
                project,
                &goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                at(deadline)?,
                &ActiveControl
            )
            .await,
        Err(SourceTestCorrectionFailure::BudgetExhausted)
    ));
    assert_eq!(ledger.revision(), previous_revision);
    assert_eq!(run.state(), AgentControllerState::Replan);

    let split = correction
        .execute(
            project,
            &goal,
            &mut run,
            &mut ledger,
            version,
            &current,
            observed_at(20)?,
            &ActiveControl,
        )
        .await?;
    let change_id = split.change_step_id();
    let proof_id = split.proof_step_id();
    let mut version = split.ledger_version();
    assert_ne!(change_id, CHANGE_STEP);
    assert_ne!(proof_id, CHANGE_STEP);
    let [replan] = ledger.replans() else {
        return Err("expected exactly one retained decomposition".into());
    };
    assert_eq!(replan.cause(), TaskLedgerReplanCause::ErrorCorrection);
    assert_eq!(replan.previous_revision(), previous_revision);
    assert_eq!(replan.revision(), run.task_ledger_revision());
    assert_eq!(replan.retired_step_ids(), &[CHANGE_STEP]);
    assert_eq!(replan.added_step_ids().len(), 2);
    assert!(replan.added_step_ids().contains(&change_id));
    assert!(replan.added_step_ids().contains(&proof_id));
    let retired = ledger
        .step(CHANGE_STEP)
        .ok_or("failed step lost on split")?;
    assert!(!retired.is_active_plan_step());
    assert_eq!(retired.definition(), failed.definition());
    assert_eq!(retired.attempts(), failed.attempts());
    let change = ledger.step(change_id).ok_or("change step missing")?;
    let proof = ledger.step(proof_id).ok_or("proof step missing")?;
    assert_eq!(change.status(), TaskStepStatus::Ready);
    assert_eq!(proof.status(), TaskStepStatus::Pending);
    assert_eq!(
        change.definition().addressed_criteria(),
        failed.definition().acceptance_criteria()
    );
    assert!(change.definition().acceptance_criteria().is_empty());
    assert!(change.definition().dependencies().is_empty());
    let VerificationTarget::DiffInvariant(diff) = change.definition().verification_spec().target()
    else {
        return Err("correction lacks its independent change check".into());
    };
    let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
    assert_eq!(diff.mode(), DiffInvariantMode::ExactPaths);
    assert_eq!(diff.paths(), std::slice::from_ref(&path));
    assert!(proof.definition().addressed_criteria().is_empty());
    assert_eq!(
        proof.definition().acceptance_criteria(),
        failed.definition().acceptance_criteria()
    );
    assert_eq!(
        proof.definition().dependencies(),
        &[StepDependency::new(change_id)]
    );
    let original_spec = failed.definition().verification_spec();
    let replacement_spec = proof.definition().verification_spec();
    assert_ne!(replacement_spec.id(), original_spec.id());
    assert_eq!(replacement_spec.requirement(), original_spec.requirement());
    assert_eq!(replacement_spec.target(), original_spec.target());
    assert_eq!(
        replacement_spec.python_integer_translation(),
        original_spec.python_integer_translation()
    );
    let sequence = run.last_event_sequence();
    assert!(matches!(
        correction
            .execute(
                project,
                &goal,
                &mut run,
                &mut ledger,
                version,
                &current,
                observed_at(21)?,
                &ActiveControl
            )
            .await,
        Err(SourceTestCorrectionFailure::InvalidState)
    ));
    assert_eq!(run.last_event_sequence(), sequence);
    assert_eq!(run.current_snapshot_id(), red_snapshot);
    assert_eq!(run.budget(), budget);
    assert_eq!(run.usage(), usage);
    drop(store);

    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    assert_eq!(
        store.load_agent_run(project, run_id).await?.as_ref(),
        Some(&run)
    );
    let stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("split Ledger missing after reopen")?;
    assert_eq!(stored.ledger(), &ledger);
    assert_eq!(stored.version(), version);
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("red publication missing after split")?;
    version = start_correction_step(
        store.as_ref(),
        project,
        &mut run,
        &mut ledger,
        version,
        change_id,
        observed_at,
    )
    .await?;
    let diagnostic = ApplySourceTestCorrection::new(store.as_ref(), store.as_ref())
        .load_edit_diagnostic(project, &goal, &run, &ledger, &current, change_id)
        .await?;
    assert_eq!(diagnostic.summary(), FAILURE_REASON);
    assert_eq!(diagnostic.failed_step_id(), CHANGE_STEP);
    assert_eq!(diagnostic.run_id(), run_id);
    assert_eq!(diagnostic.snapshot_id(), red_snapshot);
    assert_eq!(diagnostic.change_step_id(), change_id);
    assert_eq!(diagnostic.source_revision().path(), &path);
    assert_eq!(
        diagnostic.source_revision().content_hash().as_bytes(),
        blake3::hash(INCORRECT).as_bytes()
    );

    let stored = StoredTaskLedger::new(ledger.clone(), version);
    let state = EditPhaseState::new(project, &goal, &stored, &run, &current)?;
    let page = read_subject_line(project, &current, &path).await?;
    assert_eq!(page.text(), INCORRECT_LINE);
    let phase = PreparedEditPhase::update(state, &page)?;
    let memory = RunMemoryCheckpoint::compile(&goal, &ledger, &run, &current, Vec::new())?;
    let mut compiled = build_edit_phase_request(
        &goal,
        &stored,
        &phase,
        state,
        profile.clone(),
        Some(&memory),
        EditPhasePrerequisites::none().with_source_failure(&diagnostic),
    )?;
    let user = compiled
        .request()
        .messages()
        .get(1)
        .ok_or("active Edit view missing")?;
    assert!(
        user.content()
            .contains(&format!("Observed: {FAILURE_REASON}\n"))
    );
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let action = match live {
        Some(live) => {
            live.edit(
                store.as_ref(),
                project,
                &patch,
                phase,
                state,
                &mut compiled,
                observed_at(25)?,
            )
            .await?
        }
        None => complete_fixed_edit(phase, state, &patch, CORRECT_LINE).await?,
    };
    let AgentAction::ApplyPatch(proposal) = &action else {
        return Err("correction did not propose a workspace patch".into());
    };
    let [PatchOperation::Update(update)] = proposal.operations() else {
        return Err("correction escaped its exact one-file scope".into());
    };
    assert_eq!(update.expected(), diagnostic.source_revision());
    let corrected_hash = update.content().content_hash();
    if let Some(live) = live {
        let required = original_spec
            .python_integer_translation()
            .ok_or("correction guard missing")?;
        live.require_safe_source(
            required,
            corrected_hash,
            update.content().as_bytes(),
            at(deadline)?,
        )?;
    }

    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let runner = native_runner()?;
    let verifier = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approvals,
        &patch,
        &runner,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref())
    .with_python_integer_translation_verifier(&verifier)
    .with_source_failure_replan(&goal);
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let now = observed_at(30)?.max(run.updated_at());
    let changed = execute_approved_until(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        action,
        None,
        140,
        now.unix_millis(),
        at(deadline)?,
        &seed,
        &mut compiler,
        &events,
        async |_| Ok(()),
    )
    .await?;
    let MutationControllerOutcome::StepVerified { snapshot_id, .. } = changed else {
        return Err("fixed change did not pass its current Diff verification".into());
    };
    assert_ne!(snapshot_id, red_snapshot);
    assert_eq!(run.state(), AgentControllerState::Verify);

    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("corrected source not published")?;
    let now = observed_at(35)?.max(run.updated_at());
    let continuation = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            RunEventId::from_bytes([200; 32]),
            now,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::Continued(ContinueVerifiedAgentPlanOutcome::StepStarted {
        step_id,
        ledger_version,
    }) = continuation
    else {
        return Err("Core did not start the dependent proof".into());
    };
    assert_eq!(step_id, proof_id);
    version = ledger_version;
    let catalog = DiscoverProjectCommands.execute_plan22(project.worktree().id(), &current)?;
    let command = manifest_test_command(&catalog)?;
    let confirmation = a3_application::CommandAllowlistStore::load_current(store.as_ref(), project)
        .await?
        .ok_or("previous exact command confirmation missing")?;
    let check = RequestAgentFinish
        .verification_command(ledger.step(proof_id).ok_or("proof step missing")?)
        .ok_or("Core did not select the original unittest")?;
    assert_eq!(check.command_id(), command);
    let now = observed_at(40)?.max(run.updated_at());
    let checked = execute_approved_until(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(check),
        Some((&catalog, &confirmation)),
        170,
        now.unix_millis(),
        at(deadline)?,
        &seed,
        &mut compiler,
        &events,
        async |_| Ok(()),
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id: verified,
    } = checked
    else {
        return Err("real green unittest and guarded proof did not verify".into());
    };
    assert_eq!(verified, snapshot_id);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.budget(), budget);
    assert_eq!(run.created_at(), started);
    assert_eq!(run.usage(), usage);
    assert_eq!(
        ledger
            .step(CHANGE_STEP)
            .ok_or("failed history missing")?
            .attempts(),
        failed.attempts()
    );
    drop(refresh);
    drop(store);

    let now = observed_at(50)?.max(run.updated_at());
    if now >= at(deadline)? {
        return Err("correction exhausted the original deadline".into());
    }
    finish_reopened_correction(
        fixture,
        &goal,
        run,
        ledger,
        evidence_id,
        (snapshot_id, corrected_hash),
        now,
    )
    .await
}

async fn start_correction_step(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: &mut AgentRun,
    ledger: &mut a3_domain::TaskLedger,
    version: TaskLedgerStoreVersion,
    change: TaskStepId,
    observed_at: impl Fn(u64) -> TestResult<AgentRunTimestamp>,
) -> TestResult<TaskLedgerStoreVersion> {
    let snapshot = run.current_snapshot_id();
    for (id, time, signal) in [
        (220, 22, AgentControllerSignal::ReplanApplied),
        (221, 23, AgentControllerSignal::LocalizationComplete),
    ] {
        let expected = run.last_event_sequence();
        let now = observed_at(time)?.max(run.updated_at());
        let advance = AdvanceAgentController.execute(
            run,
            signal,
            RunEventId::from_bytes([id; 32]),
            snapshot,
            now,
            false,
        )?;
        AppendRunEvent::new(store)
            .execute(project, expected, run, advance.event())
            .await?;
    }
    assert_eq!(run.state(), AgentControllerState::Plan);
    let expected = run.last_event_sequence();
    let now = observed_at(24)?.max(run.updated_at());
    ledger.start_step(
        change,
        run.id(),
        TaskLedgerTimestamp::from_unix_millis(now.unix_millis())?,
    )?;
    let advance = AdvanceAgentController.execute(
        run,
        AgentControllerSignal::PlanReady,
        RunEventId::from_bytes([222; 32]),
        snapshot,
        now,
        false,
    )?;
    Ok(store
        .commit_ledger_action(project, version, expected, ledger, run, advance.event())
        .await?)
}

async fn finish_reopened_correction(
    fixture: &Fixture,
    goal: &GoalContract,
    mut run: AgentRun,
    ledger: a3_domain::TaskLedger,
    evidence_id: TaskEvidenceId,
    expected_evidence: (SnapshotId, ContentHash),
    observed_at: AgentRunTimestamp,
) -> TestResult {
    let (snapshot, corrected_hash) = expected_evidence;
    let project = &fixture.project;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        store.load_agent_run(project, run.id()).await?.as_ref(),
        Some(&run)
    );
    let stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("verified Ledger missing")?;
    assert_eq!(stored.ledger(), &ledger);
    let state = store
        .load_verification_state(
            project,
            goal.task_id(),
            &[evidence_id],
            snapshot,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::Test(test)] = state.evidence() else {
        return Err("current guarded TestEvidence missing after reopen".into());
    };
    let [case] = test.cases() else {
        return Err("expected one actual existing unittest".into());
    };
    assert_eq!(case.outcome(), TestCaseOutcome::Passed);
    let witness = test
        .python_integer_translation()
        .ok_or("current translation witness missing")?;
    assert_eq!(
        witness.revision().content_hash().as_bytes(),
        corrected_hash.as_bytes()
    );
    assert_eq!(test.command().snapshot_id(), snapshot);
    let memory = RunMemoryCheckpoint::compile(
        goal,
        stored.ledger(),
        &run,
        state.published_index(),
        Vec::new(),
    )?;
    let request =
        AcceptanceVerificationRequest::new(project.clone(), &run, goal.clone(), ledger, memory)?;
    let expected = run.last_event_sequence();
    let verifier = DeterministicAcceptanceVerifier::new(&store);
    let advance = VerifyAgentAcceptance::new(&verifier)
        .execute(
            &mut run,
            &request,
            RunEventId::from_bytes([251; 32]),
            observed_at,
            &ActiveControl,
        )
        .await?;
    AppendRunEvent::new(&store)
        .execute(project, expected, &run, advance.event())
        .await?;
    assert_eq!(run.state(), AgentControllerState::Done);
    drop(store);

    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        reopened.load_agent_run(project, run.id()).await?.as_ref(),
        Some(&run)
    );
    assert_eq!(
        reopened
            .load_current_goal_contract(project, goal.task_id())
            .await?
            .as_ref(),
        Some(goal)
    );
    Ok(())
}

async fn assert_reopened_red(
    fixture: &Fixture,
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &a3_domain::TaskLedger,
    version: TaskLedgerStoreVersion,
) -> TestResult {
    let project = &fixture.project;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    assert_eq!(
        store.load_agent_run(project, run.id()).await?.as_ref(),
        Some(run)
    );
    assert_eq!(
        store
            .load_current_goal_contract(project, goal.task_id())
            .await?
            .as_ref(),
        Some(goal)
    );
    let restored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("failed Ledger missing after reopen")?;
    assert_eq!(restored.ledger(), ledger);
    assert_eq!(restored.version(), version);
    let step = restored
        .ledger()
        .step(CHANGE_STEP)
        .ok_or("failed step missing")?;
    assert_eq!(step.status(), TaskStepStatus::Failed);
    let [failed] = step.attempts() else {
        return Err("failed attempt was lost or replayed".into());
    };
    assert_eq!(failed.run_id(), run.id());
    let TaskStepAttemptOutcome::Failed { reason } = failed.outcome() else {
        return Err("expected actual retained failure".into());
    };
    assert_eq!(reason.as_str(), FAILURE_REASON);
    assert!(failed.verification().is_none());
    assert!(failed.evidence_ids().is_empty());

    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("failed revision missing after reopen")?;
    let snapshot = current.run().snapshot_id();
    assert_eq!(snapshot, run.current_snapshot_id());
    let attempts = store
        .load_agent_mutation_attempts(project, run.id())
        .await?;
    let [patch, process] = attempts.as_slice() else {
        return Err("expected one retained patch and one actual process".into());
    };
    assert_eq!(patch.kind(), AgentMutationKind::Patch);
    assert_eq!(
        patch.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_eq!(process.kind(), AgentMutationKind::Process);
    assert_eq!(
        process.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_eq!(process.tool_attempt().snapshot_id(), snapshot);
    let history = store
        .load_run_events(project, run.id(), None, RunEventPageLimit::new(64)?)
        .await?;
    assert!(!history.has_more());
    assert!(!history.events().iter().any(|event| matches!(
        event.kind(),
        RunEventKind::ContextCompiled | RunEventKind::ModelInteraction
    )));
    let [.., completed_process, failed_verification] = history.events() else {
        return Err("failed verification journal events missing".into());
    };
    assert_eq!(completed_process.kind(), RunEventKind::ToolAction);
    assert_eq!(
        completed_process.subject(),
        Some(RunEventSubject::Tool(process.tool_attempt().tool_run_id())),
    );
    assert_eq!(
        completed_process.payload().outcome(),
        Some(RunEventOutcome::Succeeded)
    );
    assert_eq!(completed_process.snapshot_id(), snapshot);
    assert_eq!(
        failed_verification.kind(),
        RunEventKind::StateTransition {
            from: AgentControllerState::Execute,
            to: AgentControllerState::Replan,
        },
    );
    assert_eq!(
        failed_verification.payload().code(),
        RunEventCode::VerificationFailure
    );
    assert_eq!(
        failed_verification.payload().outcome(),
        Some(RunEventOutcome::Failed)
    );
    assert_eq!(failed_verification.snapshot_id(), snapshot);
    assert_eq!(failed_verification.sequence(), run.last_event_sequence());
    Ok(())
}
