//! Actual Intake and initial Plan with one ready source step and a budget-bound Run.

use super::*;
use a3_application::{
    ExecuteInitialPlan, InitialPlanExecutionOutcome, InitialPlanReservationStore, PlanPhaseState,
    PlanStepOffer, PreparedPlanPhase, RunEventPageLimit, StoredPendingGoalIntake,
};
use a3_domain::{ProjectCommandCatalog, RunEventKind, StepDependency};

const PLAN_ORIGINAL: &str = "Fix the arithmetic mean in average.py.";
const PLAN_TASK: TaskId = TaskId::from_bytes([84; 32]);
const PLAN_RUN: AgentRunId = AgentRunId::from_bytes([85; 32]);
const CHANGE_STEP: TaskStepId = TaskStepId::from_bytes([199; 32]);
const CHECK_STEP: TaskStepId = TaskStepId::from_bytes([200; 32]);

#[test]
#[ignore = "explicit local 4k measurement; set A3_PLAN22_P4_PLAN_RUN_DIR to an unused path"]
fn live_ollama_4k_initial_plan_starts_budget_bound_run() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = PathBuf::from(
            std::env::var_os("A3_PLAN22_P4_PLAN_RUN_DIR")
                .ok_or("explicit Plan directory required")?,
        );
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("Plan fixture requires an absolute unused output directory".into());
        }
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        write_report(
            &root,
            "native-start.json",
            json!({"version":1,"phase":"initial_plan","tag":LIVE_TAG}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_live_initial_plan(&root));
        write_report(
            &root,
            "native-final.json",
            json!({"version":1,"success":result.is_ok(),"goal_done":false}),
        )?;
        result
    })
}

async fn run_live_initial_plan(root: &Path) -> TestResult {
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    fs::create_dir(worktree.join("tests"))?;
    fs::write(worktree.join("average.py"), WRONG_SUBJECT)?;
    fs::write(worktree.join("tests/__init__.py"), b"")?;
    fs::write(worktree.join("tests/test_average.py"), TESTS)?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let before = inventory(&worktree)?;
    let started = prepare_and_start_run(&fixture).await?;
    check_reopened_plan(&fixture, &started).await?;
    assert!(inventory(&worktree)? == before);
    write_report(
        root,
        "plan-reopen.json",
        json!({
            "original_bytes":PLAN_ORIGINAL.len(),"goal_done":false,"files_unchanged":true,
            "ready_steps":1,"pending_steps":1,"test_proof_selected":true,"test_processes_run":0,
            "mutations":0,"intake_usage":usage_json(&started.accounting.usage()),
            "plan_usage":usage_json(&started.plan_usage),"linked_usage":usage_json(&started.linked_usage),
            "original_deadline_millis":started.accounting.deadline().unix_millis(),
            "run_started_millis":started.run.created_at().unix_millis(),
            "run_remaining_millis":started.run.budget().duration_limit().millis(),
        }),
    )
}

struct StartedPlan {
    goal: GoalContract,
    review: GoalIntakeReview,
    ledger: StoredTaskLedger,
    run: AgentRun,
    accounting: IntakePhaseAccounting,
    plan_usage: EditPhaseReservedUsage,
    linked_usage: EditPhaseReservedUsage,
}

struct LivePlan<'a> {
    fixture: &'a Fixture,
    store: &'a LibsqlKnowledgeStore,
    provider: &'a OllamaModelProvider,
    profile: ModelProfile,
    clock: Instant,
    allowance: AgentRunBudget,
}

async fn prepare_and_start_run(fixture: &Fixture) -> TestResult<StartedPlan> {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let catalog = DiscoverProjectCommands.execute(project.worktree().id(), &published)?;
    let mut tests = catalog
        .commands()
        .iter()
        .filter(|command| command.kind() == DiscoveredCommandKind::Test);
    let command = tests.next().ok_or("no indexed existing unittest command")?;
    assert!(tests.next().is_none());
    assert_eq!(command.executable().as_str(), "python");
    assert_eq!(
        command
            .arguments()
            .iter()
            .map(|arg| arg.as_str())
            .collect::<Vec<_>>(),
        ["-B", "-m", "unittest", "discover", "-s", "tests"]
    );
    assert!(!command.evidence().is_empty());

    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let captured_at = GoalContractTimestamp::from_unix_millis(CAPTURE_MILLIS)?;
    let clock = Instant::now();
    let captured = CaptureGoalIntake::new(store.as_ref())
        .execute(
            project,
            PLAN_TASK,
            PLAN_ORIGINAL.as_bytes().to_vec(),
            captured_at,
        )
        .await?;
    let allowance = AgentRunBudget::new(
        AgentTurnLimit::new(4)?,
        AgentTokenLimit::new(12_288)?,
        AgentTokenLimit::new(4_096)?,
        AgentActionLimit::new(4)?,
        AgentRunDurationLimit::from_millis(540_000)?,
        AgentRepairLimit::new(1)?,
    );
    let profile = probe_profile(&fixture.root, &provider).await?;
    let owner = LivePlan {
        fixture,
        store: store.as_ref(),
        provider: &provider,
        profile,
        clock,
        allowance,
    };
    let (goal, review, accounting) = owner.intake(&captured, command.id()).await?;
    assert!(store.load_task_ledger(project, PLAN_TASK).await?.is_none());
    assert!(store.load_agent_run(project, PLAN_RUN).await?.is_none());
    let (ledger, plan_usage) = owner
        .plan(&goal, &published, &catalog, command.id())
        .await?;
    assert_eq!(
        store.load_task_ledger(project, PLAN_TASK).await?.as_ref(),
        Some(&ledger)
    );
    assert!(store.load_agent_run(project, PLAN_RUN).await?.is_none());
    let started_at = intake_time(clock)?;
    let (run, event) = AgentRun::start_with_budget(
        PLAN_RUN,
        goal.reference(),
        ledger.ledger().revision(),
        owner.profile.reference(),
        accounting.run_budget(started_at)?,
        published.run().snapshot_id(),
        RunEventId::from_bytes([86; 32]),
        started_at,
    )?;
    CreateAgentRun::new(store.as_ref())
        .execute(project, &run, &event)
        .await?;
    let linked_usage = store.load_edit_phase_usage(project, run.id()).await?;
    assert_eq!(
        (
            linked_usage.calls,
            linked_usage.repairs,
            linked_usage.unknown_calls,
            linked_usage.unobserved_calls
        ),
        (2, 0, 0, 0)
    );
    assert_eq!(
        linked_usage.prompt_tokens,
        accounting.usage().prompt_tokens + plan_usage.prompt_tokens
    );
    assert_eq!(
        linked_usage.output_tokens,
        accounting.usage().output_tokens + plan_usage.output_tokens
    );
    Ok(StartedPlan {
        goal,
        review,
        ledger,
        run,
        accounting,
        plan_usage,
        linked_usage,
    })
}

impl LivePlan<'_> {
    async fn intake(
        &self,
        captured: &StoredPendingGoalIntake,
        command: DiscoveredCommandId,
    ) -> TestResult<(GoalContract, GoalIntakeReview, IntakePhaseAccounting)> {
        let project = &self.fixture.project;
        let root = &self.fixture.root;
        let captured_at = captured.pending().created_at();
        let original = captured.pending().original();
        let window = original.range(0, PLAN_ORIGINAL.len())?;
        let minimum_cases = MinimumTestCaseCount::new(2)?;
        let proof = CriterionEvidenceRequirement::ExistingTest {
            command,
            selector: TestCaseSelector::All,
            minimum_cases,
            scope: VerificationScope::Workspace,
            python_integer_translation: None,
        };
        let anchor =
            IntakePhaseAnchor::new(project, captured, None, window, self.profile.reference())?;
        let phase = PreparedIntakePhase::new(
            anchor,
            captured,
            None,
            self.profile.clone(),
            Some((window, proof.clone())),
        )?;
        let (projection, budget) =
            self.quote("intake-intent.json", phase.request(), anchor.job_digest())?;
        let started = Instant::now();
        let execution = ExecuteIntakePhase::new(self.provider, self.store, self.store)
            .execute(
                project,
                &phase,
                self.allowance,
                intake_time(self.clock)?,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        let accounting = self
            .store
            .load_intake_phase_accounting(project, PLAN_TASK)
            .await;
        write_report(
            root,
            "intake-result.json",
            json!({
                "elapsed_millis":started.elapsed().as_millis(),"observed_review":execution.is_ok(),
                "failure":execution.as_ref().err().map(|error| format!("{error:?}")),
                "provider_prompt_tokens":execution.as_ref().ok().and_then(|report| report.usage.prompt_tokens()),
                "provider_output_tokens":execution.as_ref().ok().and_then(|report| report.usage.output_tokens()),
                "durable_usage":accounting.as_ref().ok().and_then(|value| value.map(|scope| usage_json(&scope.usage()))),
            }),
        )?;
        let report = execution?;
        let accounting = accounting?.ok_or("durable Intake scope missing")?;
        assert_eq!(report.projection, projection);
        assert_eq!(report.budget, budget);
        assert_eq!(accounting.budget(), self.allowance);
        assert_eq!(accounting.profile(), self.profile.reference());
        assert_eq!(accounting.captured_at(), captured_at);
        assert_eq!(accounting.linked_run(), None);
        assert_eq!(
            accounting.deadline().unix_millis(),
            CAPTURE_MILLIS + self.allowance.duration_limit().millis()
        );
        verify_one_settled_call(accounting.usage(), budget)?;
        let review = report.review;
        let [batch] = review.batches() else {
            return Err("expected one actual Intake review batch".into());
        };
        let [finding] = batch.findings() else {
            return Err("expected one actual literal outcome".into());
        };
        let required = matches!(finding.kind(), GoalIntakeFindingKind::Outcome {
            requirement:AcceptanceCriterionRequirement::Must, evidence_requirement,
        } if evidence_requirement == &proof);
        let exact = batch.questions().is_empty()
            && finding.range() == window
            && finding.statement() == PLAN_ORIGINAL;
        write_report(
            root,
            "intake-review-check.json",
            json!({"must_with_existing_test":required,"exact_original_quote":exact}),
        )?;
        if !required || !exact {
            return Err(
                "actual Intake failed the required literal outcome and proof binding".into(),
            );
        }
        let criterion = AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([2; 32]),
            AcceptanceCriterionStatement::try_from_string(finding.statement().to_owned())?,
        )
        .with_origin(CriterionOrigin::new(finding.range(), proof));
        let draft = GoalContractDraft::new(
            GoalObjective::try_from_string(finding.statement().to_owned())?,
            vec![criterion],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string(
                "Run the two unchanged arithmetic-mean tests".to_owned(),
            )?,
        )?;
        let goal =
            GoalContract::initial(PLAN_TASK, draft, captured_at).with_original(original.clone())?;
        assert_eq!(
            PromoteGoalIntake::new(self.store)
                .execute(project, PLAN_TASK, captured.version(), &goal)
                .await?,
            goal.reference()
        );
        Ok((goal, review, accounting))
    }

    async fn plan(
        &self,
        goal: &GoalContract,
        published: &PublishedIndex,
        catalog: &ProjectCommandCatalog,
        command: DiscoveredCommandId,
    ) -> TestResult<(StoredTaskLedger, EditPhaseReservedUsage)> {
        let project = &self.fixture.project;
        let root = &self.fixture.root;
        let path = RepositoryPath::try_from_bytes(b"average.py".to_vec())?;
        let revision = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|file| file.path() == &path)
            .ok_or("average.py missing from the actual publication")?;
        let page = WorkspaceAgentSourceReader
            .read_page(
                project,
                revision,
                &AgentFileInspection::new(
                    path.clone(),
                    AgentFileStartLine::new(1)?,
                    AgentFileLineCount::new(8)?,
                ),
                &ActiveControl,
            )
            .await?;
        assert!(page.text().as_bytes() == WRONG_SUBJECT && !page.truncated());
        let state = PlanPhaseState::new(project, goal, published, &self.profile, Some(catalog))?;
        let criterion = goal.draft().acceptance_criteria()[0].id();
        let diff = VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([199; 32]),
            VerificationRequirement::try_from_string("Only average.py changes".to_owned())?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?,
        );
        let test = VerificationSpec::test(
            VerificationSpecId::from_bytes([200; 32]),
            VerificationRequirement::try_from_string(
                "Run both existing arithmetic-mean tests".to_owned(),
            )?,
            command,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(2)?,
            VerificationScope::Workspace,
        );
        let phase = PreparedPlanPhase::new(
            state,
            vec![
                PlanStepOffer::new(CHANGE_STEP, vec![criterion], vec![], diff.clone()),
                PlanStepOffer::new(CHECK_STEP, vec![], vec![criterion], test.clone())
                    .with_required_offers(vec![CHANGE_STEP]),
            ],
            Some(&page),
        )?;
        let messages = phase.request().messages();
        assert_eq!(messages.len(), 2);
        let view: Value = serde_json::from_str(messages[1].content())?;
        assert!(view["source"]["file"] == "average.py");
        assert!(view["source"]["text"].as_str() == Some(page.text()));
        let (projection, budget) =
            self.quote("plan-intent.json", phase.request(), phase.job_digest())?;
        let observer = ObservedPlanProvider {
            root,
            inner: self.provider,
            store: self.store,
            project,
        };
        let started = Instant::now();
        let execution = ExecuteInitialPlan::new(
            &observer,
            self.store,
            self.store,
            &WorkspaceAgentSourceReader,
            self.store,
        )
        .execute(
            project,
            &phase,
            EditPhaseCallKind::Initial,
            intake_time(self.clock)?,
            ModelRequestTimeout::from_millis(180_000)?,
            &ActiveControl,
        )
        .await;
        let usage = self.store.load_initial_plan_usage(project, PLAN_TASK).await;
        write_report(
            root,
            "plan-result.json",
            json!({
                "elapsed_millis":started.elapsed().as_millis(),
                "failure":execution.as_ref().err().map(|error| format!("{error:?}")),
                "outcome":execution.as_ref().ok().map(|report| match &report.outcome {
                    InitialPlanExecutionOutcome::Materialized(_) => "materialized",
                    InitialPlanExecutionOutcome::NeedEvidence { .. } => "need_evidence",
                    InitialPlanExecutionOutcome::TooLarge => "too_large",
                    InitialPlanExecutionOutcome::Halted(_) => "halted",
                }),
                "provider_prompt_tokens":execution.as_ref().ok().and_then(|report| report.usage.prompt_tokens()),
                "provider_output_tokens":execution.as_ref().ok().and_then(|report| report.usage.output_tokens()),
                "durable_usage":usage.as_ref().ok().map(usage_json),
            }),
        )?;
        let report = execution?;
        let usage = usage?;
        assert_eq!(report.projection, projection);
        assert_eq!(report.budget, budget);
        verify_one_settled_call(usage, budget)?;
        let InitialPlanExecutionOutcome::Materialized(stored) = report.outcome else {
            return Err("actual Plan did not materialize the bounded initial ledger".into());
        };
        let ledger = stored.ledger();
        assert_eq!(ledger.steps().len(), 2);
        let change = ledger
            .step(CHANGE_STEP)
            .ok_or("actual Plan omitted the scoped change offer")?;
        let check = ledger
            .step(CHECK_STEP)
            .ok_or("actual Plan omitted the existing test offer")?;
        assert_eq!(change.status(), TaskStepStatus::Ready);
        assert_eq!(check.status(), TaskStepStatus::Pending);
        assert_eq!(change.definition().addressed_criteria(), &[criterion]);
        assert!(change.definition().acceptance_criteria().is_empty());
        assert!(change.definition().dependencies().is_empty());
        assert_eq!(change.definition().verification_spec(), &diff);
        assert!(check.definition().addressed_criteria().is_empty());
        assert_eq!(check.definition().acceptance_criteria(), &[criterion]);
        assert_eq!(check.definition().verification_spec(), &test);
        assert_eq!(
            check.definition().dependencies(),
            &[StepDependency::new(CHANGE_STEP)]
        );
        assert!(change.definition().intended_outcome() != check.definition().intended_outcome());
        assert!(change.attempts().is_empty() && check.attempts().is_empty());
        write_report(
            root,
            "plan-ledger-check.json",
            json!({
                "selected_change_and_test":true,"test_depends_on_change":true,"distinct_outcomes":true,
                "change_ready":true,"test_pending":true,"test_minimum_cases":2,
            }),
        )?;
        Ok((stored, usage))
    }

    fn quote(
        &self,
        file: &str,
        request: &ModelProviderRequest,
        job: ContentHash,
    ) -> TestResult<(ModelWireProjection, PhaseWireBudget)> {
        let projection = self.provider.project_wire_request(request)?;
        assert_eq!(
            (projection.context_tokens(), projection.output_tokens()),
            (4_096, 1_024)
        );
        assert_eq!(projection.thinking(), ModelWireThinking::Disabled);
        let fit = PhaseWireBudget::fit(projection, &self.profile);
        write_report(
            &self.fixture.root,
            file,
            json!({
                "job_blake3":hash_hex(job),"wire":wire_json(projection),
                "charged_prompt_tokens":fit.as_ref().ok().map(|value| value.prompt_tokens()),
                "charged_output_tokens":fit.as_ref().ok().map(|value| value.output_tokens()),
                "capacity_error":fit.as_ref().err().map(|error| format!("{error:?}")),
            }),
        )?;
        Ok((projection, fit?))
    }
}

struct ObservedPlanProvider<'a> {
    root: &'a Path,
    inner: &'a OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
}

impl std::fmt::Debug for ObservedPlanProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ObservedPlanProvider")
    }
}

impl ModelProvider for ObservedPlanProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(self.inner)
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.inner.project_wire_request(request)
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .store
                .load_initial_plan_usage(self.project, PLAN_TASK)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if (
                usage.calls,
                usage.repairs,
                usage.unobserved_calls,
                usage.unknown_calls,
            ) != (1, 0, 1, 0)
            {
                return Err(ModelProviderFailure::Rejected);
            }
            write_report(
                self.root,
                "plan-dispatch.json",
                json!({"wire":wire_json(projection),"before_io":usage_json(&usage)}),
            )
            .map_err(|_| ModelProviderFailure::Unavailable)?;
            self.inner
                .stream_projected(request, projection, timeout, control)
                .await
        })
    }
}

fn verify_one_settled_call(usage: EditPhaseReservedUsage, budget: PhaseWireBudget) -> TestResult {
    if (
        usage.calls,
        usage.repairs,
        usage.unobserved_calls,
        usage.unknown_calls,
    ) != (1, 0, 0, 0)
        || usage.prompt_tokens != u64::from(budget.prompt_tokens())
        || usage.output_tokens != u64::from(budget.output_tokens())
    {
        return Err("settled phase usage differs from the charged wire".into());
    }
    Ok(())
}

async fn check_reopened_plan(fixture: &Fixture, started: &StartedPlan) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let project = &fixture.project;
    let run = store
        .load_agent_run(project, PLAN_RUN)
        .await?
        .ok_or("Run missing after reopen")?;
    assert_eq!(run, started.run);
    assert_eq!(run.state(), AgentControllerState::Intake);
    assert_eq!(run.usage(), AgentRunUsage::ZERO);
    assert_eq!(
        store
            .load_current_goal_contract(project, PLAN_TASK)
            .await?
            .as_ref(),
        Some(&started.goal)
    );
    assert_eq!(
        store
            .load_goal_intake_review(project, PLAN_TASK)
            .await?
            .as_ref(),
        Some(&started.review)
    );
    assert_eq!(
        store.load_task_ledger(project, PLAN_TASK).await?.as_ref(),
        Some(&started.ledger)
    );
    let linked = store
        .load_intake_phase_accounting(project, PLAN_TASK)
        .await?
        .ok_or("Intake scope missing after reopen")?;
    assert_eq!(linked.linked_run(), Some(run.id()));
    assert_eq!(linked.budget(), started.accounting.budget());
    assert_eq!(linked.profile(), started.accounting.profile());
    assert_eq!(linked.captured_at(), started.accounting.captured_at());
    assert_eq!(linked.deadline(), started.accounting.deadline());
    assert_eq!(linked.usage(), started.accounting.usage());
    assert_eq!(
        store.load_initial_plan_usage(project, PLAN_TASK).await?,
        started.plan_usage
    );
    assert_eq!(
        store.load_edit_phase_usage(project, run.id()).await?,
        started.linked_usage
    );
    assert_eq!(run.budget(), linked.run_budget(run.created_at())?);
    assert!(run.budget().duration_limit().millis() < linked.budget().duration_limit().millis());
    assert_eq!(
        run.created_at().unix_millis() + run.budget().duration_limit().millis(),
        linked.deadline().unix_millis()
    );
    assert!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .is_empty()
    );
    let events = store
        .load_run_events(project, run.id(), None, RunEventPageLimit::new(8)?)
        .await?;
    assert!(!events.has_more());
    let [event] = events.events() else {
        return Err("expected only the Run start event".into());
    };
    assert_eq!(event.kind(), RunEventKind::RunStarted);
    Ok(())
}
