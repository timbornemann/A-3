//! Opt-in real mutation after a fixed approved plan; never a private repository or scripted patch.
use super::super::super::{FixtureClock, FixtureControl, recovery_contract, support};
use super::LiveResearchModel;
use crate::agent_conversation_runtime::AgentConversationRuntime;
use crate::production_agent_run_executor::{ProductionAgentRunExecutor, ProductionAgentRunPorts};
use a3_application::*;
use a3_domain::*;
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "agent_live_coding_cases.rs"]
mod cases;
use cases::LiveCodingCase;

#[derive(Debug)]
struct ReadOnlySettings(StoredDesktopSettings);

#[derive(Debug)]
struct PreflightControl<'a>(&'a JobContext);

impl ContextCompileControl for PreflightControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.cancellation_token().is_cancelled()
    }
    fn report_phase(&self, _: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl DesktopSettingsStore for ReadOnlySettings {
    fn load(&self) -> DesktopSettingsStoreFuture<'_, StoredDesktopSettings> {
        Box::pin(async { Ok(self.0.clone()) })
    }

    fn append<'a>(
        &'a self,
        _: DesktopSettingsStoreVersion,
        _: &'a DesktopSettings,
    ) -> DesktopSettingsStoreFuture<'a, StoredDesktopSettings> {
        Box::pin(async { Err(DesktopSettingsStoreFailure::VersionConflict) })
    }
}

fn now() -> Result<AgentRunTimestamp, Box<dyn Error>> {
    Ok(AgentRunTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?,
    )?)
}

fn check_scope(case: LiveCodingCase, action: &AgentApprovalAction) -> Result<(), Box<dyn Error>> {
    let allowed = match action {
        AgentApprovalAction::Patch(patch) => {
            !patch.files().is_empty()
                && patch.files().len() <= case.sources().len()
                && patch.files().iter().all(|file| {
                    case.patch_scope(
                        file.operation(),
                        file.source_path()
                            .and_then(|p| std::str::from_utf8(p.as_bytes()).ok()),
                        file.target_path()
                            .and_then(|p| std::str::from_utf8(p.as_bytes()).ok()),
                    )
                })
        }
        AgentApprovalAction::Process(process) => process_scope(
            process.executable(),
            process.arguments(),
            process.working_directory(),
            process.execution_mode(),
            process.network(),
        ),
    };
    if allowed {
        Ok(())
    } else {
        Err("live fixture refused an action outside the preapproved exact scope".into())
    }
}

fn process_scope(
    executable: &str,
    arguments: &[String],
    directory: &AgentApprovalWorkingDirectory,
    mode: ProcessExecutionMode,
    network: AgentApprovalNetworkScope,
) -> bool {
    executable == "python"
        && arguments == ["-m", "pytest"]
        && directory == &AgentApprovalWorkingDirectory::Root
        && mode == ProcessExecutionMode::KnownSafe
        && network == AgentApprovalNetworkScope::Denied
}

/// Independent physical check; the model cannot change the locked runner or tests.
fn run_locked_tests(path: &std::path::Path, control: &JobContext) -> Result<bool, Box<dyn Error>> {
    run_check(
        std::process::Command::new("python")
            .args(["-B", "-m", "pytest"])
            .current_dir(path),
        || control.cancellation_token().is_cancelled(),
    )
}

fn run_oracle(
    case: LiveCodingCase,
    path: &std::path::Path,
    cancelled: impl Fn() -> bool,
) -> Result<bool, Box<dyn Error>> {
    run_check(
        std::process::Command::new("python")
            .args(["-I", "-B", "-c", cases::ORACLE, case.id()])
            .arg(path)
            .current_dir(path),
        cancelled,
    )
}

fn require_oracle_passed(passed: bool) -> Result<(), Box<dyn Error>> {
    if passed {
        Ok(())
    } else {
        Err("Done alone is insufficient: independent additional-input oracle failed".into())
    }
}

// Only the two closed supervisor checks above use this helper; no model-supplied argv.
fn run_check(
    command: &mut std::process::Command,
    cancelled: impl Fn() -> bool,
) -> Result<bool, Box<dyn Error>> {
    if cancelled() {
        return Err("independent locked test cancelled before spawn".into());
    }
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {}
            Err(error) => {
                let _kill = child.kill();
                let _joined = child.wait();
                return Err(error.into());
            }
        }
        if cancelled() || start.elapsed() > Duration::from_secs(30) {
            let _kill = child.kill();
            let _joined = child.wait();
            return Err("independent locked test cancelled or timed out".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

// Each approval continuation owns a new progress scope, matching the production scheduler.
fn run_attempt(
    executor: Arc<ProductionAgentRunExecutor>,
    project: ProjectIdentity,
    request: AgentRunExecutionRequest,
    outer: &JobContext,
) -> Result<AgentRunExecutionOutcome, Box<dyn Error>> {
    let (scheduler, events) =
        JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixtureClock))?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let id = JobId::new(1);
    scheduler.submit(id, JobOwner::new(1), move |control: JobContext| {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| AgentRunExecutionFailure::Unavailable)
            .and_then(|runtime| runtime.block_on(executor.execute(&project, request, &control)));
        let succeeded = result.is_ok();
        let _sent = send.send(result);
        if succeeded {
            JobCompletion::Succeeded
        } else {
            JobCompletion::Failed
        }
    })?;
    let start = Instant::now();
    let mut cancelled = false;
    loop {
        if !cancelled
            && (outer.cancellation_token().is_cancelled()
                || start.elapsed() >= Duration::from_secs(120))
        {
            scheduler.cancel(id)?;
            cancelled = true;
        }
        if let Some(event) = events.next_timeout(Duration::from_millis(100))?
            && matches!(
                event.kind(),
                JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
            )
        {
            break;
        }
    }
    if cancelled {
        return Err("live Agent attempt cancelled or timed out".into());
    }
    Ok(receive.recv_timeout(Duration::from_secs(1))??)
}

#[test]
fn live_coding_scope_rejects_process_scope_changes() -> Result<(), Box<dyn Error>> {
    let args = ["-m".to_owned(), "pytest".to_owned()];
    let root = AgentApprovalWorkingDirectory::Root;
    let allowed = ProcessExecutionMode::KnownSafe;
    let denied = AgentApprovalNetworkScope::Denied;
    assert!(process_scope("python", &args, &root, allowed, denied));
    assert!(!process_scope("cmd", &args, &root, allowed, denied));
    assert!(!process_scope(
        "python",
        &["-m".to_owned(), "pip".to_owned()],
        &root,
        allowed,
        denied
    ));
    assert!(!process_scope(
        "python",
        &args,
        &root,
        ProcessExecutionMode::Shell,
        denied
    ));
    assert!(!process_scope(
        "python",
        &args,
        &root,
        ProcessExecutionMode::Open,
        denied
    ));
    assert!(!process_scope(
        "python",
        &args,
        &AgentApprovalWorkingDirectory::Subtree(RepositoryPath::try_from_bytes(b"tests".to_vec())?),
        allowed,
        denied
    ));
    assert!(!process_scope(
        "python",
        &args,
        &root,
        allowed,
        AgentApprovalNetworkScope::Requested(PolicyResourceId::from_bytes([99; 32]))
    ));
    Ok(())
}

#[test]
fn live_coding_scope_rejects_test_rewrites_moves_and_extra_paths() {
    let case = LiveCodingCase::Bugfix;
    assert!(case.patch_scope(
        AgentApprovalFileOperation::Update,
        Some("increment.py"),
        Some("increment.py")
    ));
    for operation in [
        AgentApprovalFileOperation::Add,
        AgentApprovalFileOperation::Move,
        AgentApprovalFileOperation::Delete,
    ] {
        assert!(!case.patch_scope(operation, Some("increment.py"), Some("increment.py")));
    }
    for path in [
        "tests/test_increment.py",
        "pytest.py",
        "pyproject.toml",
        "unrelated.txt",
        "../increment.py",
        "increment.py/other",
    ] {
        assert!(!case.patch_scope(AgentApprovalFileOperation::Update, Some(path), Some(path)));
    }
}

#[test]
#[ignore = "explicit reviewed model/network opt-in; isolated public patch and exact offline test command only"]
fn agent_approved_live_coding_fixture() -> Result<(), Box<dyn Error>> {
    support::run_libsql_test_selected(
        async {
            recovery_contract::owned_with_timeout(Duration::from_secs(360), |control, _| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(evaluate(&control))
            })
        },
        true,
    )
}

async fn evaluate(control: &JobContext) -> Result<(), Box<dyn Error>> {
    let case = LiveCodingCase::parse(super::optional_env("A3_LIVE_AGENT_CASE")?.as_deref())?;
    let generation = match std::env::var("A3_LIVE_AGENT_GENERATION").as_deref() {
        Ok("staged") => AgentActionGeneration::SelectThenFill,
        Ok("guided") => AgentActionGeneration::ReviewThenSelect,
        Ok("source-guided") => AgentActionGeneration::SourceGuided,
        Ok("baseline") | Err(std::env::VarError::NotPresent) => AgentActionGeneration::SingleAction,
        _ => {
            return Err(
                "A3_LIVE_AGENT_GENERATION must be baseline, staged, guided or source-guided".into(),
            );
        }
    };
    println!(
        "A3_LIVE_CODING version=2 case={} generation={generation:?}",
        case.id()
    );
    let catalog_path = super::optional_env("A3_LIVE_AGENT_CATALOG")?
        .or(super::optional_env("A3_CONFIGURED_RESEARCH_CATALOG")?)
        .ok_or("live Agent requires an explicit read-only settings catalog")?;
    let catalog_path = std::path::Path::new(&catalog_path);
    let original = LibsqlKnowledgeStore::read_settings_snapshot(catalog_path).await?;
    let live = LiveResearchModel::probe().await?;
    let kind = match live.profile.provider_id().as_str() {
        "openai" => ModelProviderKind::OpenAi,
        "gemini" => ModelProviderKind::Gemini,
        "ollama" => ModelProviderKind::Ollama,
        "openai-compatible" => ModelProviderKind::OpenAiCompatible,
        _ => return Err("unreviewed live Agent provider".into()),
    };
    let settings = original.settings().clone().with_provider_llm_probe(
        kind,
        LlmModelRole::Coding,
        live.profile.clone(),
        SettingsTimestamp::from_unix_millis(now()?.unix_millis())?,
    )?;
    let runtime = AgentConversationRuntime::new(
        Arc::new(ReadOnlySettings(StoredDesktopSettings::new(
            original.version(),
            settings,
        ))),
        Arc::new(a3_credentials::NativeProviderCredentialStore::new()),
    );
    assert_eq!(
        runtime.execution_model().await?.1.reference(),
        live.profile.reference()
    );
    let repository = support::TempDirectory::new()?;
    repository.git(["init", "--initial-branch=main"])?;
    for (path, content) in case.files() {
        repository.write(path, content)?;
    }
    repository.git(["add", "."])?;
    assert!(
        !run_locked_tests(repository.path(), control)?,
        "fixture must start red"
    );
    assert!(
        !run_oracle(case, repository.path(), || control
            .cancellation_token()
            .is_cancelled())?,
        "independent oracle must start red"
    );
    let project = RepositoryInspector::new().inspect(repository.path())?;
    let data = support::TempDirectory::new()?;
    let store = Arc::new(
        LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?).await?,
    );
    store.record_opened_project(&project).await?;
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let indexed = refresh
        .execute(
            &project,
            &RepositoryChangeBatch::full_rescan(
                Vec::new(),
                RepositoryRescanReason::InitialObservation,
            )?,
            &mut BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?,
            &FixtureControl,
        )
        .await?;
    let commands =
        DiscoverProjectCommands.execute(project.worktree().id(), indexed.published_index())?;
    let command = commands
        .commands()
        .iter()
        .find(|c| c.kind() == DiscoveredCommandKind::Test)
        .ok_or("test command not discovered")?;
    assert_eq!(command.executable().as_str(), "python");
    assert_eq!(
        command
            .arguments()
            .iter()
            .map(ProcessArgument::as_str)
            .collect::<Vec<_>>(),
        ["-m", "pytest"]
    );
    ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(&project, &commands, vec![command.id()], now()?, None)
        .await?;
    let criterion = AcceptanceCriterionId::from_bytes([1; 32]);
    let step_id = TaskStepId::from_bytes([2; 32]);
    let task_id = TaskId::from_bytes([3; 32]);
    let run_id = AgentRunId::from_bytes([4; 32]);
    let goal = GoalContract::initial(
        task_id,
        GoalContractDraft::new(
            GoalObjective::try_from_string(case.objective().to_owned())?,
            vec![AcceptanceCriterion::new(
                criterion,
                AcceptanceCriterionStatement::try_from_string(
                    "The unchanged existing tests pass after the requested implementation"
                        .to_owned(),
                )?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string(
                "Run python -m pytest using the confirmed command profile".to_owned(),
            )?,
        )?,
        GoalContractTimestamp::from_unix_millis(now()?.unix_millis())?,
    );
    let definition = TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string(case.outcome().to_owned())?,
        TaskStepRationale::try_from_string(
            "Implement the requested behavior and verify with existing tests".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            case.evidence().to_owned(),
        )?],
        VerificationSpec::command(
            VerificationSpecId::from_bytes([5; 32]),
            VerificationRequirement::try_from_string("Existing tests pass".to_owned())?,
            command.id(),
            VerificationScope::Workspace,
        ),
    )?
    .with_acceptance_criteria(vec![criterion])?;
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![definition],
        TaskLedgerTimestamp::from_unix_millis(now()?.unix_millis())?,
    )?;
    ledger.start_step(
        step_id,
        run_id,
        TaskLedgerTimestamp::from_unix_millis(now()?.unix_millis())?,
    )?;
    CreateGoalContract::new(store.as_ref())
        .execute(&project, &goal)
        .await?;
    let version = CreateTaskLedger::new(store.as_ref())
        .execute(&project, &ledger)
        .await?
        .version();
    let (mut run, event) = AgentRun::start(
        run_id,
        goal.reference(),
        ledger.revision(),
        live.profile.reference(),
        indexed.published_index().run().snapshot_id(),
        RunEventId::from_bytes([6; 32]),
        now()?,
    )?;
    CreateAgentRun::new(store.as_ref())
        .execute(&project, &run, &event)
        .await?;
    // This fixture begins at the accepted one-step plan, just like E7. Research has separate live cases.
    for (i, signal) in [
        AgentControllerSignal::AnchorsAccepted,
        AgentControllerSignal::LocalizationComplete,
        AgentControllerSignal::PlanReady,
    ]
    .into_iter()
    .enumerate()
    {
        let sequence = run.last_event_sequence();
        let snapshot = run.current_snapshot_id();
        let change = AdvanceAgentController.execute(
            &mut run,
            signal,
            RunEventId::from_bytes([10 + u8::try_from(i)?; 32]),
            snapshot,
            now()?,
            false,
        )?;
        AppendRunEvent::new(store.as_ref())
            .execute(&project, sequence, &run, change.event())
            .await?;
    }
    let inspection = Arc::new(AgentInspectionBuffer::new());
    let preflight = a3_context::DeterministicAgentContextCompiler::new(
        CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref()),
        &a3_workspace::WorkspaceAgentSourceReader,
    );
    let prompt = AgentPromptContract::prepare_current_step(
        &live.profile,
        &project,
        ledger.step(step_id).ok_or("missing current step")?,
    )?;
    println!(
        "A3_LIVE_CODING grounding={:?} static_bytes={} grounding_bytes={}",
        live.profile.settings().schema_grounding(),
        prompt.system_message().content().len(),
        prompt
            .schema_grounding_message()
            .map_or(0, |m| m.content().len())
    );
    let input = AgentContextCompileInput::new(
        project.clone(),
        goal.clone(),
        ledger.clone(),
        step_id,
        live.profile.clone(),
        None,
        Vec::new(),
        Vec::new(),
    )?;
    // A diagnostic compile must not complete the production attempt's monotone progress.
    match preflight.compile(&input, &PreflightControl(control)).await {
        Ok(compiled) => {
            let originals_delivered: usize = compiled
                .request()
                .messages()
                .iter()
                .map(|message| case.originals_delivered(message.content()))
                .sum();
            if originals_delivered == 0 {
                println!(
                    "A3_LIVE_CODING preflight_code_allowance={} prompt_tokens={}",
                    compiled
                        .budget_plan()
                        .allowance(a3_domain::ContextSection::CodeAndEvidence),
                    compiled.budget_usage().prompt_total()
                );
                // Only known public-fixture source metadata; never print a model response or credentials.
                for line in compiled
                    .request()
                    .messages()
                    .iter()
                    .flat_map(|m| m.content().lines())
                    .filter(|line| {
                        line.starts_with("L3 ")
                            || line.starts_with("L2 ")
                            || line.starts_with("[ORIGINAL_SOURCE ")
                    })
                    .take(16)
                {
                    println!("A3_LIVE_CODING preflight_source={line}");
                }
                return Err("normal agent preflight omitted the current original source".into());
            }
            // Initial hydration is bounded; further originals may require normal safe reads.
            println!(
                "A3_LIVE_CODING context_preflight=passed original_sources={originals_delivered} required_sources={}",
                case.sources().len()
            );
        }
        Err(error) => {
            println!("A3_LIVE_CODING context_preflight={error:?}");
            return Err(error.into());
        }
    }
    inspection.activate_project(&project);
    let approvals = Arc::new(AgentApprovalBuffer::new());
    approvals.activate_project(&project);
    let executor = Arc::new(
        ProductionAgentRunExecutor::new(
            ProductionAgentRunPorts {
                workspace: store.clone(),
                journal: store.clone(),
                actions: store.clone(),
                recovery: store.clone(),
                policy: store.clone(),
                evidence: store.clone(),
                index: store.clone(),
                lens_index: store.clone(),
                search: store.clone(),
                claims: store.clone(),
                allowlist: store.clone(),
                research: None,
            },
            runtime,
            inspection.clone(),
            approvals.clone(),
            None,
        )?
        .with_generation_probe(generation),
    );
    let query = GetAgentApprovalCenter::new(
        store.clone(),
        store.clone(),
        store.clone(),
        approvals.clone(),
    );
    let approve = ControlAgentApproval::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        approvals,
    );
    let mut request = AgentRunExecutionRequest::new(task_id, ledger.revision(), version);
    for attempt in 0..8u8 {
        println!(
            "A3_LIVE_CODING attempt={attempt} model={}",
            live.profile.model_id().as_str()
        );
        let outcome = run_attempt(executor.clone(), project.clone(), request, control);
        let run = store
            .load_agent_run(&project, run_id)
            .await?
            .ok_or("run disappeared")?;
        println!(
            "A3_LIVE_CODING outcome={outcome:?} state={:?} sequence={:?}",
            run.state(),
            run.last_event_sequence()
        );
        println!("A3_LIVE_CODING usage={:?}", run.usage());
        let mutations = store.load_agent_mutation_attempts(&project, run_id).await?;
        // Only durable, content-free receipts; process application is not test success.
        // The store bounds the history, and at most 32 entries are printed per attempt.
        println!(
            "A3_LIVE_CODING mutation_receipts={} omitted={} changed_sources={} required_sources={} snapshot_changed={}",
            mutations.len(),
            mutations.len().saturating_sub(32),
            case.changed_sources(repository.path())?,
            case.sources().len(),
            run.current_snapshot_id() != indexed.published_index().run().snapshot_id()
        );
        for (ordinal, mutation) in mutations.iter().take(32).enumerate() {
            println!(
                "A3_LIVE_CODING receipt={} kind={:?} status={:?} application={:?}",
                ordinal + 1,
                mutation.kind(),
                mutation.tool_attempt().status(),
                mutation.disposition().application_state()
            );
        }
        let observed = store
            .load_task_ledger(&project, task_id)
            .await?
            .ok_or("ledger disappeared")?;
        if let Some(overview) = inspection.overview(&project, task_id)? {
            for process in overview.processes() {
                println!(
                    "A3_LIVE_CODING process={:?} termination={:?}",
                    process.kind(),
                    process.termination()
                );
            }
        }
        for step in observed.ledger().steps() {
            println!(
                "A3_LIVE_CODING step={:?} active={} verified={}",
                step.status(),
                step.is_active_plan_step(),
                step.attempts()
                    .last()
                    .and_then(TaskStepAttempt::verification)
                    .is_some_and(|v| v.passed())
            );
        }
        if outcome.is_err() || run.state() == AgentControllerState::Failed {
            println!(
                "A3_LIVE_CODING failure_physical_test_passed={} independent_oracle_passed={} protected_files_unchanged={} settings_unchanged={}",
                run_locked_tests(repository.path(), control)?,
                run_oracle(case, repository.path(), || control
                    .cancellation_token()
                    .is_cancelled())?,
                case.protected_unchanged(repository.path()),
                LibsqlKnowledgeStore::read_settings_snapshot(catalog_path).await? == original
            );
        }
        outcome?;
        if run.state() == AgentControllerState::Failed {
            return Err("live Agent reached terminal Failed; recorded failure and physical verification above".into());
        }
        if run.state() == AgentControllerState::Done {
            break;
        }
        let AgentApprovalLoadResult::Available(center) =
            query.execute(&project, task_id, now()?, control).await?
        else {
            return Err(
                "nonterminal live Agent stopped without an actionable exact approval".into(),
            );
        };
        check_scope(case, center.presentation().action())?;
        assert!(center.can_allow_once());
        let approval_id = ApprovalId::from_bytes([30 + attempt; 32]);
        let result = approve
            .execute(
                &project,
                task_id,
                center.presentation().revision(),
                center.ledger_revision(),
                center.ledger_store_version(),
                AgentApprovalControlAction::AllowOnce,
                AgentApprovalControlMetadata::new(
                    approval_id,
                    RunEventId::from_bytes([50 + attempt; 32]),
                    now()?,
                ),
                control,
            )
            .await?;
        assert!(matches!(
            result,
            AgentApprovalControlResult::Applied(AgentApprovalControlOutcome::GrantStored { .. })
        ));
        let stored = store
            .load_task_ledger(&project, task_id)
            .await?
            .ok_or("ledger disappeared")?;
        request = AgentRunExecutionRequest::after_approval(
            task_id,
            stored.ledger().revision(),
            stored.version(),
            approval_id,
        );
    }
    let run = store
        .load_agent_run(&project, run_id)
        .await?
        .ok_or("run disappeared")?;
    let stored = store
        .load_task_ledger(&project, task_id)
        .await?
        .ok_or("ledger disappeared")?;
    assert_eq!(
        run.state(),
        AgentControllerState::Done,
        "attempt completion alone is not success"
    );
    assert!(stored.ledger().steps().any(|s| {
        s.status() == TaskStepStatus::Completed
            && s.attempts()
                .last()
                .and_then(TaskStepAttempt::verification)
                .is_some_and(|v| v.passed() && !v.evidence_ids().is_empty())
    }));
    assert!(
        run_locked_tests(repository.path(), control)?,
        "physical post-run verification failed"
    );
    let oracle_passed = run_oracle(case, repository.path(), || {
        control.cancellation_token().is_cancelled()
    })?;
    // Check preserved data after both physical executions, not just before them.
    for (path, content) in case.protected() {
        assert_eq!(
            std::fs::read(repository.path().join(path))?,
            content.as_bytes(),
            "protected fixture changed: {path}"
        );
    }
    assert_eq!(
        case.changed_sources(repository.path())?,
        case.sources().len(),
        "all requested source files must change"
    );
    assert_eq!(
        LibsqlKnowledgeStore::read_settings_snapshot(catalog_path).await?,
        original
    );
    println!(
        "A3_LIVE_CODING independent_oracle_passed={oracle_passed} protected_files_unchanged=true settings_unchanged=true"
    );
    // An expected negative live result must reach the owned supervisor as an error,
    // not panic its worker and obscure the oracle failure with a closed-channel message.
    require_oracle_passed(oracle_passed)?;
    println!(
        "A3_LIVE_CODING passed=true done=true verified=true independent_oracle_passed=true protected_files_unchanged=true settings_unchanged=true"
    );
    Ok(())
}
