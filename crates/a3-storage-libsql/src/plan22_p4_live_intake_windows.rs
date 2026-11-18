//! Two actual bounded 4k Intake windows through durable Review and Goal promotion.

use super::*;
use a3_application::{
    IntakeWindowByteLimit, prepare_goal_from_intake_review, prepare_next_intake_phase,
    select_intake_window,
};

const REQUEST: &str = "Rename the README heading to Guide.\r\n\r\nKeep source.py unchanged.";
const OUTCOME: &str = "Rename the README heading to Guide.";
const CONSTRAINT: &str = "Keep source.py unchanged.";
const FIRST_BYTES: usize = OUTCOME.len() + "\r\n\r\n".len();
const TASK: TaskId = TaskId::from_bytes([85; 32]);

#[test]
#[ignore = "explicit local P4 measurement; requires an unused A3_PLAN22_P4_INTAKE_WINDOWS_RUN_DIR"]
fn live_ollama_4k_two_window_intake_promotes_exact_goal() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P4_INTAKE_WINDOWS_RUN_DIR")
                .ok_or("explicit two-window Intake directory required")?,
        );
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("two-window Intake requires an absolute unused output directory".into());
        }
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        write_report(
            &root,
            "native-start.json",
            json!({"version":1,"phase":"intake_windows","tag":LIVE_TAG}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run(&root));
        write_report(
            &root,
            "native-final.json",
            json!({"version":1,"success":result.is_ok(),"goal_done":false}),
        )?;
        result
    })
}

async fn run(root: &Path) -> TestResult {
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
    let (store, _, _, _) = index_fixture(&fixture).await?;
    let project = &fixture.project;
    let clock = Instant::now();
    let captured_at = GoalContractTimestamp::from_unix_millis(CAPTURE_MILLIS)?;
    let captured = CaptureGoalIntake::new(store.as_ref())
        .execute(project, TASK, REQUEST.as_bytes().to_vec(), captured_at)
        .await?;
    let allowance = AgentRunBudget::new(
        AgentTurnLimit::new(2)?,
        AgentTokenLimit::new(8_192)?,
        AgentTokenLimit::new(2_048)?,
        AgentActionLimit::new(2)?,
        AgentRunDurationLimit::from_millis(360_000)?,
        AgentRepairLimit::new(1)?,
    );
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_profile(root, &provider).await?;

    let original = captured.pending().original();
    let first = select_intake_window(
        captured.pending(),
        None,
        IntakeWindowByteLimit::new(FIRST_BYTES + 4)?,
    )?
    .ok_or("first original window missing")?;
    let last = original.range(FIRST_BYTES, REQUEST.len())?;
    if first != original.range(0, FIRST_BYTES)? {
        return Err("first window did not retain the original paragraph separator".into());
    }
    let proof = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
    )?);
    let first_phase = PreparedIntakePhase::new(
        IntakePhaseAnchor::new(project, &captured, None, first, profile.reference())?,
        &captured,
        None,
        profile.clone(),
        Some((first, proof.clone())),
    )?;
    let (review, first_accounting) = observe_window(
        &fixture,
        store.as_ref(),
        &provider,
        &first_phase,
        allowance,
        clock,
        1,
    )
    .await?;
    let first_finding = sole_finding(&review)?;
    if first_finding.range() != first
        || first_finding.statement() != OUTCOME
        || first_finding.kind()
            != &(GoalIntakeFindingKind::Outcome {
                requirement: AcceptanceCriterionRequirement::Must,
                evidence_requirement: proof.clone(),
            })
    {
        return Err(
            "first Intake window did not preserve the exact required outcome and proof".into(),
        );
    }
    drop(store);

    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let Some(GoalIntakeLoadResult::Pending(reopened)) =
        store.load_goal_intake(project, TASK).await?
    else {
        return Err("pending capture missing after first Intake window".into());
    };
    let reopened_review = store
        .load_goal_intake_review(project, TASK)
        .await?
        .ok_or("first Intake review missing after reopen")?;
    if reopened != captured
        || reopened_review != review
        || store.load_intake_phase_accounting(project, TASK).await? != Some(first_accounting)
    {
        return Err("first Intake capture, Review or accounting changed after reopen".into());
    }

    let second_phase = prepare_next_intake_phase(
        project,
        &reopened,
        Some(&reopened_review),
        &profile,
        &provider,
        None,
    )?
    .ok_or("second original window missing")?;
    if second_phase.anchor().window() != last {
        return Err("second Intake phase did not select the exact original tail".into());
    }
    let (review, accounting) = observe_window(
        &fixture,
        &store,
        &provider,
        &second_phase,
        allowance,
        clock,
        2,
    )
    .await?;
    let second_finding = sole_finding(&review)?;
    if second_finding.range() != last
        || second_finding.statement() != CONSTRAINT
        || second_finding.kind() != &GoalIntakeFindingKind::Constraint
        || review.revision().get() != 2
        || !review.is_fully_reviewed(reopened.pending())?
        || accounting.deadline() != first_accounting.deadline()
    {
        return Err(
            "second Intake window did not preserve the exact constraint and deadline".into(),
        );
    }

    let goal = prepare_goal_from_intake_review(reopened.pending(), &review)?;
    let [criterion] = goal.draft().acceptance_criteria() else {
        return Err("prepared Goal did not contain exactly one criterion".into());
    };
    let [constraint] = goal.draft().constraints() else {
        return Err("prepared Goal did not contain exactly one constraint".into());
    };
    let origin = criterion
        .origin()
        .ok_or("prepared Goal criterion lacks source attribution")?;
    if origin.original_range() != first
        || origin.evidence_requirement() != &proof
        || constraint.origin() != Some(last)
        || goal
            .original()
            .ok_or("prepared Goal original missing")?
            .as_bytes()
            != REQUEST.as_bytes()
    {
        return Err("prepared Goal changed original bytes, attribution or proof".into());
    }
    let reference = PromoteGoalIntake::new(&store)
        .execute(project, TASK, reopened.version(), &goal)
        .await?;
    drop(store);

    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let reopened_goal = store.load_current_goal_contract(project, TASK).await?;
    let reopened_accounting = store.load_intake_phase_accounting(project, TASK).await?;
    if reference != goal.reference()
        || !matches!(
            store.load_goal_intake(project, TASK).await?,
            Some(GoalIntakeLoadResult::Materialized(actual)) if actual == reference
        )
        || reopened_goal.as_ref() != Some(&goal)
        || store.load_goal_intake_review(project, TASK).await?.as_ref() != Some(&review)
        || reopened_accounting != Some(accounting)
        || store.load_task_ledger(project, TASK).await?.is_some()
        || inventory(&worktree)? != before
    {
        return Err("reopened Goal, Review, accounting or protected worktree did not match".into());
    }

    write_report(
        root,
        "intake-windows-reopen.json",
        json!({
            "original_bytes":REQUEST.len(),"review_revision":review.revision().get(),
            "goal_original_exact":true,"goal_promoted":true,"goal_done":false,
            "run_started":false,"protected_worktree_unchanged":true,
            "first_range":[first.start_byte(),first.end_byte()],
            "second_range":[last.start_byte(),last.end_byte()],
            "original_deadline_millis":accounting.deadline().unix_millis(),
            "captured_at_millis":captured_at.unix_millis(),
            "duration_limit_millis":allowance.duration_limit().millis(),
            "usage":usage_json(&accounting.usage()),
        }),
    )
}

async fn observe_window(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    provider: &OllamaModelProvider,
    phase: &PreparedIntakePhase,
    allowance: AgentRunBudget,
    clock: Instant,
    number: u32,
) -> TestResult<(GoalIntakeReview, IntakePhaseAccounting)> {
    let root = &fixture.root;
    let project = &fixture.project;
    let previous = store.load_intake_phase_accounting(project, TASK).await?;
    let anchor = phase.anchor();
    let projection = provider.project_wire_request(phase.request())?;
    let budget = PhaseWireBudget::fit(projection, phase.request().profile());
    write_report(
        root,
        &format!("window-{number}-intent.json"),
        json!({
            "wire":wire_json(projection),"job_blake3":hash_hex(anchor.job_digest()),
            "range":[anchor.window().start_byte(),anchor.window().end_byte()],
            "review_revision":anchor.review_revision().map(|revision| revision.get()),
            "reserved_prompt":budget.as_ref().ok().map(|budget| budget.prompt_tokens()),
            "reserved_output":budget.as_ref().ok().map(|budget| budget.output_tokens()),
            "capacity_error":budget.as_ref().err().map(|error| format!("{error:?}")),
        }),
    )?;
    let budget = budget?;
    if (
        projection.context_tokens(),
        projection.output_tokens(),
        projection.thinking(),
    ) != (4_096, 1_024, ModelWireThinking::Disabled)
    {
        return Err("two-window Intake profile differs from pinned 4k wire".into());
    }
    let started = Instant::now();
    let observed = ExecuteIntakePhase::new(provider, store, store)
        .execute(
            project,
            phase,
            allowance,
            intake_time(clock)?,
            ModelRequestTimeout::from_millis(180_000)?,
            &ActiveControl,
        )
        .await;
    let accounting = store.load_intake_phase_accounting(project, TASK).await;
    let batch = observed
        .as_ref()
        .ok()
        .and_then(|result| result.review.batches().last());
    let finding = batch.and_then(|batch| batch.findings().first());
    write_report(
        root,
        &format!("window-{number}-result.json"),
        json!({
            "elapsed_millis":started.elapsed().as_millis(),"observed_review":observed.is_ok(),
            "failure":observed.as_ref().err().map(|error| format!("{error:?}")),
            "finding_count":batch.map(|batch| batch.findings().len()),
            "question_count":batch.map(|batch| batch.questions().len()),
            "finding_range":finding.map(|finding| [finding.range().start_byte(), finding.range().end_byte()]),
            "finding_kind":finding.map(|finding| match finding.kind() {
                GoalIntakeFindingKind::Outcome { requirement: AcceptanceCriterionRequirement::Must, .. } => "must",
                GoalIntakeFindingKind::Outcome { requirement: AcceptanceCriterionRequirement::Should, .. } => "should",
                GoalIntakeFindingKind::Constraint => "constraint",
                GoalIntakeFindingKind::NonGoal => "non_goal",
                GoalIntakeFindingKind::ExistingUserDecision => "existing_decision",
            }),
            "reported_prompt_tokens":observed.as_ref().ok().and_then(|result| result.usage.prompt_tokens()),
            "reported_output_tokens":observed.as_ref().ok().and_then(|result| result.usage.output_tokens()),
            "usage":accounting.as_ref().ok().and_then(|value| value.map(|scope| usage_json(&scope.usage()))),
            "original_deadline_millis":accounting.as_ref().ok().and_then(|value| value.map(|scope| scope.deadline().unix_millis())),
        }),
    )?;
    let observed = observed?;
    let accounting = accounting?.ok_or("durable two-window Intake accounting missing")?;
    let usage = accounting.usage();
    let prompt = observed
        .usage
        .prompt_tokens()
        .ok_or("observed prompt usage missing")?;
    let output = observed
        .usage
        .output_tokens()
        .ok_or("observed output usage missing")?;
    if observed.projection != projection
        || observed.budget != budget
        || accounting.profile() != anchor.model_profile()
        || accounting.captured_at() != anchor.captured_at()
        || accounting.deadline().unix_millis()
            != CAPTURE_MILLIS + allowance.duration_limit().millis()
        || accounting.budget() != allowance
        || accounting.linked_run().is_some()
        || (
            usage.calls,
            usage.repairs,
            usage.unknown_calls,
            usage.unobserved_calls,
        ) != (number, 0, 0, 0)
        || usage.prompt_tokens
            != previous.map_or(0, |value| value.usage().prompt_tokens)
                + prompt.max(u64::from(budget.prompt_tokens()))
        || usage.output_tokens
            != previous.map_or(0, |value| value.usage().output_tokens)
                + output.max(u64::from(budget.output_tokens()))
    {
        return Err("actual Intake wire, usage or original deadline failed accounting".into());
    }
    Ok((observed.review, accounting))
}

fn sole_finding(review: &GoalIntakeReview) -> TestResult<&a3_domain::GoalIntakeFinding> {
    let last = review
        .batches()
        .last()
        .ok_or("actual Intake batch missing")?;
    let [finding] = last.findings() else {
        return Err("expected exactly one finding in actual Intake window".into());
    };
    if !last.questions().is_empty() {
        return Err("unexpected user question in actual Intake window".into());
    }
    Ok(finding)
}
