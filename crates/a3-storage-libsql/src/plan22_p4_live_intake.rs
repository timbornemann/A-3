//! Explicit local Intake component measurement from capture through a newly started Run.

#[path = "plan22_p4_live_initial_plan.rs"]
mod live_initial_plan;

#[path = "plan22_p4_live_intake_windows.rs"]
mod live_intake_windows;

use super::*;
use a3_application::{
    CaptureGoalIntake, ExecuteIntakePhase, GoalIntakeLoadResult, IntakePhaseAccounting,
    IntakePhaseAnchor, IntakePhaseReservationStore, PreparedIntakePhase, PromoteGoalIntake,
};
use a3_domain::{
    AcceptanceCriterionRequirement, AgentActionLimit, AgentRepairLimit, AgentRunBudget,
    AgentRunDurationLimit, AgentRunUsage, AgentTokenLimit, AgentTurnLimit, GoalIntakeFindingKind,
    GoalIntakeReview,
};

const ORIGINAL: &str = "Rename the README heading to Guide.";
const TASK: TaskId = TaskId::from_bytes([81; 32]);
const RUN_ID: AgentRunId = AgentRunId::from_bytes([82; 32]);
const CAPTURE_MILLIS: u64 = 1;

#[test]
#[ignore = "explicit local P4 measurement; requires an unused A3_PLAN22_P4_INTAKE_RUN_DIR"]
fn live_ollama_4k_intake_starts_budget_bound_run() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P4_INTAKE_RUN_DIR")
                .ok_or("explicit Intake directory required")?,
        );
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("Intake fixture requires an absolute unused output directory".into());
        }
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        write_report(
            &root,
            "native-start.json",
            json!({"version":1,"phase":"intake","tag":LIVE_TAG}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_intake_fixture(&root));
        write_report(
            &root,
            "native-final.json",
            json!({"version":1,"success":result.is_ok(),"goal_done":false}),
        )?;
        result
    })
}

async fn run_intake_fixture(root: &Path) -> TestResult {
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    fs::write(worktree.join("README.md"), b"# Draft\n")?;
    fs::write(worktree.join("source.py"), b"VALUE = 7\n")?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let before = inventory(&worktree)?;
    let result = capture_to_run(&fixture).await?;
    check_reopened_intake(&fixture, &result).await?;
    assert!(inventory(&worktree)? == before);
    write_report(
        root,
        "intake-reopen.json",
        json!({
            "original_bytes":ORIGINAL.len(), "review_revision":result.review.revision().get(),
            "run_started":true, "run_state":"intake", "plan_model_calls":0, "goal_done":false,
            "protected_worktree_unchanged":true, "usage":usage_json(&result.accounting.usage()),
            "original_deadline_millis":result.accounting.deadline().unix_millis(),
            "run_started_millis":result.run.created_at().unix_millis(),
            "run_remaining_millis":result.run.budget().duration_limit().millis(),
        }),
    )
}

struct StartedIntake {
    goal: GoalContract,
    run: AgentRun,
    review: GoalIntakeReview,
    accounting: IntakePhaseAccounting,
}

async fn capture_to_run(fixture: &Fixture) -> TestResult<StartedIntake> {
    let (store, _, _, published) = index_fixture(fixture).await?;
    let project = &fixture.project;
    let root = &fixture.root;
    let captured_at = GoalContractTimestamp::from_unix_millis(CAPTURE_MILLIS)?;
    let clock = Instant::now();
    let captured = CaptureGoalIntake::new(store.as_ref())
        .execute(project, TASK, ORIGINAL.as_bytes().to_vec(), captured_at)
        .await?;
    let original = captured.pending().original();
    let window = original.range(0, ORIGINAL.len())?;
    let path = RepositoryPath::try_from_bytes(b"README.md".to_vec())?;
    let invariant = DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?;
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_profile(root, &provider).await?;
    let anchor = IntakePhaseAnchor::new(project, &captured, None, window, profile.reference())?;
    let phase = PreparedIntakePhase::new(
        anchor,
        &captured,
        None,
        profile.clone(),
        Some((
            window,
            CriterionEvidenceRequirement::ChangeOnly(invariant.clone()),
        )),
    )?;
    let allowance = AgentRunBudget::new(
        AgentTurnLimit::new(2)?,
        AgentTokenLimit::new(8_192)?,
        AgentTokenLimit::new(2_048)?,
        AgentActionLimit::new(2)?,
        AgentRunDurationLimit::from_millis(360_000)?,
        AgentRepairLimit::new(1)?,
    );
    let projection = provider.project_wire_request(phase.request())?;
    assert_eq!(projection.context_tokens(), 4_096);
    assert_eq!(projection.output_tokens(), 1_024);
    assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
    let fit = PhaseWireBudget::fit(projection, &profile);
    write_report(
        root,
        "intake-intent.json",
        json!({
            "wire":wire_json(projection), "job_blake3":hash_hex(anchor.job_digest()),
            "profile":profile.id().to_string(), "source_bytes":ORIGINAL.len(),
            "charged_prompt_tokens":fit.as_ref().ok().map(|budget| budget.prompt_tokens()),
            "charged_output_tokens":fit.as_ref().ok().map(|budget| budget.output_tokens()),
            "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
        }),
    )?;
    let budget = fit?;
    let started = Instant::now();
    let executed = ExecuteIntakePhase::new(&provider, store.as_ref(), store.as_ref())
        .execute(
            project,
            &phase,
            allowance,
            intake_time(clock)?,
            ModelRequestTimeout::from_millis(180_000)?,
            &ActiveControl,
        )
        .await;
    let accounting = store.load_intake_phase_accounting(project, TASK).await;
    write_report(
        root,
        "intake-result.json",
        json!({
            "elapsed_millis":started.elapsed().as_millis(), "observed_review":executed.is_ok(),
            "failure":executed.as_ref().err().map(|error| format!("{error:?}")),
            "reported_prompt_tokens":executed.as_ref().ok().and_then(|report| report.usage.prompt_tokens()),
            "reported_output_tokens":executed.as_ref().ok().and_then(|report| report.usage.output_tokens()),
            "charged_usage":accounting.as_ref().ok().and_then(|value| value.map(|scope| usage_json(&scope.usage()))),
        }),
    )?;
    let executed = executed?;
    let accounting = accounting?.ok_or("durable Intake budget missing")?;
    assert_eq!(executed.projection, projection);
    assert_eq!(executed.budget, budget);
    assert_eq!(accounting.budget(), allowance);
    assert_eq!(accounting.profile(), profile.reference());
    assert_eq!(accounting.captured_at(), captured_at);
    assert_eq!(accounting.linked_run(), None);
    assert_eq!(
        accounting.deadline().unix_millis(),
        CAPTURE_MILLIS + allowance.duration_limit().millis()
    );
    let charged = accounting.usage();
    assert_eq!(
        (
            charged.calls,
            charged.repairs,
            charged.unobserved_calls,
            charged.unknown_calls
        ),
        (1, 0, 0, 0)
    );
    let prompt = executed
        .usage
        .prompt_tokens()
        .ok_or("actual prompt usage missing")?;
    let output = executed
        .usage
        .output_tokens()
        .ok_or("actual output usage missing")?;
    assert_eq!(
        charged.prompt_tokens,
        prompt.max(u64::from(budget.prompt_tokens()))
    );
    assert_eq!(
        charged.output_tokens,
        output.max(u64::from(budget.output_tokens()))
    );

    let review = executed.review;
    let [batch] = review.batches() else {
        return Err("expected one actual Intake batch".into());
    };
    write_report(
        root,
        "intake-review-check.json",
        json!({
            "finding_count":batch.findings().len(), "question_count":batch.questions().len(),
            "findings":batch.findings().iter().map(|finding| json!({
                "kind":match finding.kind() {
                    GoalIntakeFindingKind::Outcome { requirement: AcceptanceCriterionRequirement::Must, .. } => "must",
                    GoalIntakeFindingKind::Outcome { requirement: AcceptanceCriterionRequirement::Should, .. } => "should",
                    GoalIntakeFindingKind::Constraint => "constraint",
                    GoalIntakeFindingKind::NonGoal => "non_goal",
                    GoalIntakeFindingKind::ExistingUserDecision => "existing_decision",
                },
                "start":finding.range().start_byte(), "end":finding.range().end_byte(),
            })).collect::<Vec<_>>(),
        }),
    )?;
    let [finding] = batch.findings() else {
        return Err("expected one actual Intake finding".into());
    };
    let GoalIntakeFindingKind::Outcome {
        requirement: AcceptanceCriterionRequirement::Must,
        evidence_requirement,
    } = finding.kind()
    else {
        return Err("Intake did not classify the literal requested outcome as required".into());
    };
    if !batch.questions().is_empty()
        || finding.range() != window
        || finding.statement() != ORIGINAL
        || evidence_requirement != &CriterionEvidenceRequirement::ChangeOnly(invariant.clone())
    {
        return Err(
            "actual Intake changed or failed to cover the original document request".into(),
        );
    }
    let criterion = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([2; 32]),
        AcceptanceCriterionStatement::try_from_string(finding.statement().to_owned())?,
    )
    .with_origin(CriterionOrigin::new(
        finding.range(),
        evidence_requirement.clone(),
    ));
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string(finding.statement().to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Verify the requested README-only change".to_owned())?,
    )?;
    let goal = GoalContract::initial(TASK, draft, captured_at).with_original(original.clone())?;
    assert_eq!(
        PromoteGoalIntake::new(store.as_ref())
            .execute(project, TASK, captured.version(), &goal)
            .await?,
        goal.reference()
    );

    // The component supplies the one later step to exercise Goal/Run carryover.
    let definition = step(diff_spec(invariant)?)?;
    let materialized = MaterializeOriginalTask::new(store.as_ref(), store.as_ref())
        .execute(project, &goal, vec![definition])
        .await?;
    assert!(materialized.preview().unplanned_criteria.is_empty());
    let ledger = materialized.stored_ledger().ledger();
    let now = intake_time(clock)?;
    let (run, event) = AgentRun::start_with_budget(
        RUN_ID,
        goal.reference(),
        ledger.revision(),
        profile.reference(),
        accounting.run_budget(now)?,
        published.run().snapshot_id(),
        RunEventId::from_bytes([83; 32]),
        now,
    )?;
    CreateAgentRun::new(store.as_ref())
        .execute(project, &run, &event)
        .await?;
    Ok(StartedIntake {
        goal,
        run,
        review,
        accounting,
    })
}

async fn check_reopened_intake(fixture: &Fixture, result: &StartedIntake) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let project = &fixture.project;
    assert!(matches!(store.load_goal_intake(project, TASK).await?,
        Some(GoalIntakeLoadResult::Materialized(reference)) if reference == result.goal.reference()
    ));
    assert_eq!(
        store
            .load_current_goal_contract(project, TASK)
            .await?
            .as_ref(),
        Some(&result.goal)
    );
    assert_eq!(
        store.load_goal_intake_review(project, TASK).await?.as_ref(),
        Some(&result.review)
    );
    let run = store
        .load_agent_run(project, RUN_ID)
        .await?
        .ok_or("reopened Run missing")?;
    assert_eq!(run, result.run);
    assert_eq!(run.state(), AgentControllerState::Intake);
    assert_eq!(run.usage(), AgentRunUsage::ZERO);
    let linked = store
        .load_intake_phase_accounting(project, TASK)
        .await?
        .ok_or("reopened Intake budget missing")?;
    assert_eq!(linked.linked_run(), Some(run.id()));
    assert_eq!(linked.profile(), result.accounting.profile());
    assert_eq!(linked.budget(), result.accounting.budget());
    assert_eq!(linked.captured_at(), result.accounting.captured_at());
    assert_eq!(linked.deadline(), result.accounting.deadline());
    assert_eq!(linked.usage(), result.accounting.usage());
    assert_eq!(
        store.load_edit_phase_usage(project, run.id()).await?,
        linked.usage()
    );
    assert_eq!(run.budget(), linked.run_budget(run.created_at())?);
    assert!(run.budget().duration_limit().millis() < linked.budget().duration_limit().millis());
    assert_eq!(
        run.created_at().unix_millis() + run.budget().duration_limit().millis(),
        linked.deadline().unix_millis()
    );
    let stored = store
        .load_task_ledger(project, TASK)
        .await?
        .ok_or("reopened Ledger missing")?;
    let mut steps = stored.ledger().steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(
        steps.next().ok_or("ready step missing")?.status(),
        TaskStepStatus::Ready
    );
    assert!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .is_empty()
    );
    Ok(())
}

fn intake_time(clock: Instant) -> TestResult<AgentRunTimestamp> {
    at(CAPTURE_MILLIS
        .checked_add(u64::try_from(clock.elapsed().as_millis())?)
        .ok_or("Intake fixture clock overflow")?)
}
