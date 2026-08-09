//! Gate M6 acceptance over the real multilingual index, context, tool, and durable run path.

mod support;

use a3_application::{
    AcceptanceVerificationRequest, AcceptanceVerifier, AcceptanceVerifierFailure,
    AcceptanceVerifierFuture, AcceptanceVerifierOutcome, AdvanceAgentController,
    AgentContextCompileInput, AgentControllerControl, AgentControllerSignal, AgentReadAction,
    AgentReadTimeout, AgentReadTools, AgentReadToolsFuture, AgentRecoveryStore, AppendAgentRead,
    AppendRunEvent, ApplyAgentLedgerUpdate, CompileTaskLens, ContextCompileControl,
    ContextCompilePhase, CreateAgentRun, CreateGoalContract, CreateTaskLedger,
    ExecuteReadOnlyAgentTurn, GoalContractStore, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeStore, ModelCancellationFuture,
    ModelFinishReason, ModelOperationControl, ModelOutputChunk, ModelProviderCompletion,
    ModelProviderUsage, PersistAgentLedgerMutation, ProviderEvent, RefreshRepositoryIndex,
    RepositoryChangeBatch, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryRescanReason, RunEventPageLimit, RunJournalStore, SaveTaskLedger, TaskLedgerStore,
    TaskLedgerStoreVersion, TaskLensControlError, VerifyAgentAcceptance,
};
use a3_context::{DeterministicAgentContextCompiler, DeterministicAgentReadTools};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
    AcceptanceCriterionVerification, AcceptanceVerificationReceipt, AgentAction,
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, AgentToolEvidenceSet,
    AgentTurnRepairUsage, ExpectedTaskEvidence, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, IndexLanguage, ModelCapabilities, ModelContextLimit,
    ModelId, ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP, Progress, ProjectIdentity, RunEventCode, RunEventId,
    RunEventKind, RunMemoryCheckpoint, SnapshotId, StepVerification, StepVerificationId,
    StepVerificationOutcome, SuccessVerification, TaskEvidenceId, TaskId, TaskLedger,
    TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale,
    TaskStepStatus, VerificationMethod, VerificationRequirement, VerificationSpec,
    VerificationSpecId,
};
use a3_model_provider_contract_tests::{StubModelProvider, StubModelProviderBehavior};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::{RepositoryInspector, WorkspaceAgentSourceReader};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use support::{TempDirectory, run_libsql_test};

const RUST_FILES: &[(&str, &[u8])] = &[
    (
        "Cargo.toml",
        include_bytes!("../../../fixtures/rust-adapter/Cargo.toml"),
    ),
    (
        "invalid.rs",
        include_bytes!("../../../fixtures/rust-adapter/invalid.rs"),
    ),
    (
        "src/main.rs",
        include_bytes!("../../../fixtures/rust-adapter/src/main.rs"),
    ),
];

const TYPESCRIPT_FILES: &[(&str, &[u8])] = &[
    (
        "invalid.ts",
        include_bytes!("../../../fixtures/typescript-monorepo/invalid.ts"),
    ),
    (
        "package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/package.json"),
    ),
    (
        "pnpm-workspace.yaml",
        include_bytes!("../../../fixtures/typescript-monorepo/pnpm-workspace.yaml"),
    ),
    (
        "packages/core/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/core/package.json"),
    ),
    (
        "packages/core/src/index.ts",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/core/src/index.ts"),
    ),
    (
        "packages/legacy/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/legacy/package.json"),
    ),
    (
        "packages/legacy/src/index.cjs",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/legacy/src/index.cjs"),
    ),
    (
        "packages/web/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/web/package.json"),
    ),
    (
        "packages/web/src/App.tsx",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/web/src/App.tsx"),
    ),
];

const PYTHON_FILES: &[(&str, &[u8])] = &[
    (
        "invalid.py",
        include_bytes!("../../../fixtures/python-package/invalid.py"),
    ),
    (
        "pyproject.toml",
        include_bytes!("../../../fixtures/python-package/pyproject.toml"),
    ),
    (
        "requirements/base.in",
        include_bytes!("../../../fixtures/python-package/requirements/base.in"),
    ),
    (
        "requirements-dev.txt",
        include_bytes!("../../../fixtures/python-package/requirements-dev.txt"),
    ),
    (
        "requirements.txt",
        include_bytes!("../../../fixtures/python-package/requirements.txt"),
    ),
    (
        "setup.cfg",
        include_bytes!("../../../fixtures/python-package/setup.cfg"),
    ),
    (
        "setup.py",
        include_bytes!("../../../fixtures/python-package/setup.py"),
    ),
    (
        "src/sample/__init__.py",
        include_bytes!("../../../fixtures/python-package/src/sample/__init__.py"),
    ),
    (
        "src/sample/base.py",
        include_bytes!("../../../fixtures/python-package/src/sample/base.py"),
    ),
    (
        "src/sample/cli.py",
        include_bytes!("../../../fixtures/python-package/src/sample/cli.py"),
    ),
    (
        "src/sample/helpers.py",
        include_bytes!("../../../fixtures/python-package/src/sample/helpers.py"),
    ),
    (
        "src/sample/service.py",
        include_bytes!("../../../fixtures/python-package/src/sample/service.py"),
    ),
    (
        "tests/test_service.py",
        include_bytes!("../../../fixtures/python-package/tests/test_service.py"),
    ),
];

#[derive(Debug, Clone, Copy)]
struct FixtureDefinition {
    seed: u8,
    name: &'static str,
    language: IndexLanguage,
    query: &'static str,
    expected_path: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}

const FIXTURES: &[FixtureDefinition] = &[
    FixtureDefinition {
        seed: 1,
        name: "rust-adapter-v1",
        language: IndexLanguage::Rust,
        query: "Model",
        expected_path: "src/main.rs",
        files: RUST_FILES,
    },
    FixtureDefinition {
        seed: 2,
        name: "typescript-monorepo-v1",
        language: IndexLanguage::TypeScriptJavaScript,
        query: "Service",
        expected_path: "packages/core/src/index.ts",
        files: TYPESCRIPT_FILES,
    },
    FixtureDefinition {
        seed: 3,
        name: "python-package-v1",
        language: IndexLanguage::Python,
        query: "build_service",
        expected_path: "src/sample/service.py",
        files: PYTHON_FILES,
    },
];

#[test]
fn read_only_agent_reaches_verified_done_on_three_fixture_languages() -> Result<(), Box<dyn Error>>
{
    run_libsql_test(async {
        for fixture in FIXTURES {
            if let Err(error) = evaluate_fixture(*fixture).await {
                return Err(std::io::Error::other(format!(
                    "{} acceptance failed: {error:?}",
                    fixture.name
                ))
                .into());
            }
        }
        Ok(())
    })
}

#[test]
fn invalid_primary_and_repair_never_execute_the_real_read_tools() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let fixture = IndexedFixture::new(FIXTURES[0]).await?;
        let baseline = fixture.repository_tree.clone();
        let mut durable = DurableRun::new(&fixture).await?;
        let source = WorkspaceAgentSourceReader;
        let actual_tools = DeterministicAgentReadTools::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &source,
        );
        let tools = CountingReadTools {
            inner: actual_tools,
            calls: AtomicUsize::new(0),
        };
        let compiler = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
        ));
        let provider = StubModelProvider::new(
            durable.profile.provider_id().clone(),
            StubModelProviderBehavior::Events(provider_events("not-json")?),
        );
        let input = durable.context_input(&fixture, Vec::new())?;
        let outcome =
            ExecuteReadOnlyAgentTurn::new(&compiler, &provider, &tools, fixture.store.as_ref())
                .execute(&durable.run, &input, timestamp(20)?, &ActiveControl)
                .await?;
        let expected_sequence = durable.run.last_event_sequence();
        let event = outcome.record(
            &mut durable.run,
            event_id(fixture.definition.seed, 20),
            timestamp(20)?,
        )?;
        AppendRunEvent::new(fixture.store.as_ref())
            .execute(&fixture.project, expected_sequence, &durable.run, &event)
            .await?;

        let a3_application::AgentTurnOutcome::Rejected(rejected) = outcome else {
            return Err(test_error("invalid repaired model output was executed"));
        };
        if rejected.reason() != a3_application::AgentTurnRejectionReason::InvalidAfterRepair
            || rejected.charge().action().is_some()
            || rejected.charge().repair() != AgentTurnRepairUsage::One
        {
            return Err(test_error("invalid-output rejection metadata is incorrect"));
        }
        if tools.calls.load(Ordering::SeqCst) != 0 || provider.calls()?.len() != 2 {
            return Err(test_error("invalid model output crossed a read boundary"));
        }
        let interrupted = fixture
            .store
            .interrupt_agent_tool_attempts(&fixture.project, durable.run.id(), timestamp(21)?)
            .await?;
        if interrupted != 0 {
            return Err(test_error(
                "invalid model output created a durable tool attempt",
            ));
        }
        let page = fixture
            .store
            .load_run_events(
                &fixture.project,
                durable.run.id(),
                None,
                RunEventPageLimit::new(32)?,
            )
            .await?;
        if page
            .events()
            .iter()
            .any(|event| event.kind() == RunEventKind::ToolAction)
            || page.events().last().map(|event| event.payload().code())
                != Some(RunEventCode::InvalidModelOutput)
        {
            return Err(test_error(
                "invalid model output produced an executable journal event",
            ));
        }
        if fixture.repository.repository_tree()? != baseline {
            return Err(test_error("invalid output changed the fixture worktree"));
        }
        Ok(())
    })
}

async fn evaluate_fixture(fixture: FixtureDefinition) -> Result<(), Box<dyn Error>> {
    let fixture = IndexedFixture::new(fixture).await?;
    let baseline = fixture.repository_tree.clone();
    let mut durable = DurableRun::new(&fixture).await?;
    let source = WorkspaceAgentSourceReader;
    let tools = DeterministicAgentReadTools::new(
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        &source,
    );
    let compiler = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
    ));

    let search_provider = StubModelProvider::new(
        durable.profile.provider_id().clone(),
        StubModelProviderBehavior::Events(provider_events(&format!(
            r#"{{"schema_version":1,"action":{{"kind":"search","query":"{}","limit":5}}}}"#,
            fixture.definition.query
        ))?),
    );
    let search_input = durable.context_input(&fixture, Vec::new())?;
    let search_outcome =
        ExecuteReadOnlyAgentTurn::new(&compiler, &search_provider, &tools, fixture.store.as_ref())
            .execute(&durable.run, &search_input, timestamp(20)?, &ActiveControl)
            .await
            .map_err(|error| std::io::Error::other(format!("search turn failed: {error:?}")))?;
    let expected_sequence = durable.run.last_event_sequence();
    let model_event = search_outcome.record(
        &mut durable.run,
        event_id(fixture.definition.seed, 20),
        timestamp(20)?,
    )?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            expected_sequence,
            &durable.run,
            &model_event,
        )
        .await?;
    let a3_application::AgentTurnOutcome::Executed(mut search_execution) = search_outcome else {
        return Err(test_error("valid fixture search was rejected"));
    };
    if !matches!(search_execution.action(), AgentAction::Search(_)) {
        return Err(test_error("fixture search decoded another action"));
    }
    let search_result = search_execution
        .take_tool_result()
        .ok_or_else(|| test_error("fixture search produced no read result"))?;
    if !search_result
        .preview()
        .as_str()
        .contains(fixture.definition.query)
        || !search_result
            .preview()
            .as_str()
            .contains(fixture.definition.expected_path)
        || !search_result.evidence().evidence().iter().any(|evidence| {
            evidence.location().revision().path().as_bytes()
                == fixture.definition.expected_path.as_bytes()
        })
    {
        return Err(test_error(
            "fixture search lacks expected current source evidence",
        ));
    }
    let expected_sequence = durable.run.last_event_sequence();
    let recorded = search_result.record(
        &mut durable.run,
        event_id(fixture.definition.seed, 21),
        timestamp(21)?,
    )?;
    let context_result = recorded.context_result().clone();
    let evidence = recorded.evidence().clone();
    AppendAgentRead::new(fixture.store.as_ref())
        .execute(&fixture.project, expected_sequence, &durable.run, &recorded)
        .await?;

    let update_document = format!(
        r#"{{"schema_version":1,"action":{{"kind":"update_ledger","step_id":"{}","update":{{"kind":"record_result","summary":"located current source evidence for {}"}}}}}}"#,
        durable.step_id, fixture.definition.query
    );
    let update_provider = StubModelProvider::new(
        durable.profile.provider_id().clone(),
        StubModelProviderBehavior::Events(provider_events(&update_document)?),
    );
    let update_input = durable.context_input(&fixture, vec![context_result])?;
    let update_outcome =
        ExecuteReadOnlyAgentTurn::new(&compiler, &update_provider, &tools, fixture.store.as_ref())
            .execute(&durable.run, &update_input, timestamp(30)?, &ActiveControl)
            .await
            .map_err(|error| std::io::Error::other(format!("update turn failed: {error:?}")))?;
    let expected_sequence = durable.run.last_event_sequence();
    let model_event = update_outcome.record(
        &mut durable.run,
        event_id(fixture.definition.seed, 30),
        timestamp(30)?,
    )?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            expected_sequence,
            &durable.run,
            &model_event,
        )
        .await?;
    let a3_application::AgentTurnOutcome::Executed(update_execution) = update_outcome else {
        return Err(test_error("valid fixture Ledger update was rejected"));
    };
    let AgentAction::UpdateLedger(update_action) = update_execution.action() else {
        return Err(test_error("fixture result did not select a Ledger update"));
    };
    if update_execution.tool_result().is_some() {
        return Err(test_error(
            "Ledger update unexpectedly executed a read tool",
        ));
    }
    let expected_sequence = durable.run.last_event_sequence();
    let action_outcome = ApplyAgentLedgerUpdate.execute(
        &mut durable.run,
        &mut durable.ledger,
        update_action,
        Some(&evidence),
        event_id(fixture.definition.seed, 31),
        fixture.snapshot_id,
        timestamp(31)?,
        &ActiveControl,
    )?;
    durable.ledger_version = PersistAgentLedgerMutation::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            durable.ledger_version,
            expected_sequence,
            &durable.ledger,
            &durable.run,
            &action_outcome,
        )
        .await?;

    verify_current_step(&mut durable, &evidence, fixture.definition.seed)?;
    durable.ledger_version = SaveTaskLedger::new(fixture.store.as_ref())
        .execute(&fixture.project, durable.ledger_version, &durable.ledger)
        .await?
        .version();
    let published = fixture
        .store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or_else(|| test_error("fixture published index is missing before acceptance"))?;
    let run_memory = RunMemoryCheckpoint::compile(
        &durable.goal,
        &durable.ledger,
        &durable.run,
        &published,
        Vec::new(),
    )?;
    let request = AcceptanceVerificationRequest::new(
        fixture.project.clone(),
        &durable.run,
        durable.goal.clone(),
        durable.ledger.clone(),
        run_memory,
    )?;
    let verifier = FixtureAcceptanceVerifier {
        criterion_id: durable.criterion_id,
        evidence_ids: evidence
            .evidence()
            .iter()
            .map(a3_domain::AgentToolEvidence::id)
            .collect(),
        snapshot_id: fixture.snapshot_id,
    };
    let expected_sequence = durable.run.last_event_sequence();
    let accepted = VerifyAgentAcceptance::new(&verifier)
        .execute(
            &mut durable.run,
            &request,
            event_id(fixture.definition.seed, 40),
            timestamp(40)?,
            &ActiveControl,
        )
        .await?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(
            &fixture.project,
            expected_sequence,
            &durable.run,
            accepted.event(),
        )
        .await?;

    validate_durable_outcome(&fixture, &durable).await?;
    if search_provider.calls()?.len() != 1
        || update_provider.calls()?.len() != 1
        || !search_provider.calls()?[0].has_structured_output()
        || !update_provider.calls()?[0].has_structured_output()
    {
        return Err(test_error(
            "fixture provider did not remain behind the neutral port",
        ));
    }
    if fixture.repository.repository_tree()? != baseline {
        return Err(test_error("read-only agent changed the fixture worktree"));
    }
    Ok(())
}

fn verify_current_step(
    durable: &mut DurableRun,
    evidence: &AgentToolEvidenceSet,
    seed: u8,
) -> Result<(), Box<dyn Error>> {
    let specification_id = durable
        .ledger
        .step(durable.step_id)
        .ok_or_else(|| test_error("current fixture step disappeared"))?
        .definition()
        .verification_spec()
        .id();
    let evidence_ids = evidence
        .evidence()
        .iter()
        .map(a3_domain::AgentToolEvidence::id)
        .collect::<Vec<_>>();
    durable.ledger.finish_step_verification(
        durable.step_id,
        StepVerification::new(
            StepVerificationId::from_bytes(id_bytes(seed, 9)),
            specification_id,
            durable.run.id(),
            StepVerificationOutcome::Passed,
            evidence_ids,
            TaskLedgerTimestamp::from_unix_millis(32)?,
        )?,
    )?;
    Ok(())
}

async fn validate_durable_outcome(
    fixture: &IndexedFixture,
    durable: &DurableRun,
) -> Result<(), Box<dyn Error>> {
    let stored_goal = fixture
        .store
        .load_current_goal_contract(&fixture.project, durable.goal.task_id())
        .await?
        .ok_or_else(|| test_error("durable fixture Goal Contract is missing"))?;
    let stored_ledger = fixture
        .store
        .load_task_ledger(&fixture.project, durable.goal.task_id())
        .await?
        .ok_or_else(|| test_error("durable fixture Task Ledger is missing"))?;
    let stored_run = fixture
        .store
        .load_agent_run(&fixture.project, durable.run.id())
        .await?
        .ok_or_else(|| test_error("durable fixture run is missing"))?;
    if stored_goal != durable.goal
        || stored_run.state() != AgentControllerState::Done
        || stored_run.current_snapshot_id() != fixture.snapshot_id
        || stored_ledger.version() != durable.ledger_version
        || stored_ledger
            .ledger()
            .step(durable.step_id)
            .map(|step| step.status())
            != Some(TaskStepStatus::Completed)
    {
        return Err(test_error("durable fixture outcome is incomplete or stale"));
    }
    let page = fixture
        .store
        .load_run_events(
            &fixture.project,
            durable.run.id(),
            None,
            RunEventPageLimit::new(32)?,
        )
        .await?;
    let model_events = page
        .events()
        .iter()
        .filter(|event| event.kind() == RunEventKind::ModelInteraction)
        .count();
    let tool_events = page
        .events()
        .iter()
        .filter(|event| event.kind() == RunEventKind::ToolAction)
        .count();
    if page.has_more()
        || page.events().len() != usize::try_from(stored_run.last_event_sequence().get())?
        || model_events != 2
        || tool_events != 1
        || !matches!(
            page.events().last().map(|event| event.kind()),
            Some(RunEventKind::StateTransition {
                to: AgentControllerState::Done,
                ..
            })
        )
    {
        return Err(test_error("durable fixture journal is incomplete"));
    }
    Ok(())
}

struct IndexedFixture {
    definition: FixtureDefinition,
    repository: TempDirectory,
    _app_data: TempDirectory,
    repository_tree: BTreeMap<String, Vec<u8>>,
    project: ProjectIdentity,
    store: Arc<LibsqlKnowledgeStore>,
    snapshot_id: SnapshotId,
}

impl IndexedFixture {
    async fn new(definition: FixtureDefinition) -> Result<Self, Box<dyn Error>> {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, source) in definition.files {
            repository.write(path, source)?;
        }
        repository.git(["add", "."])?;
        let repository_tree = repository.repository_tree()?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let index_store: Arc<dyn KnowledgeIndexStore> = store.clone();
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            index_store,
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(2)?)?;
        let indexed = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &ActiveControl,
            )
            .await?;
        if !indexed.published() {
            return Err(test_error("fixture index was not published"));
        }
        let published = store
            .latest_published_index(&project, &ActiveControl)
            .await?
            .ok_or_else(|| test_error("fixture published index is missing"))?;
        if published.run().snapshot_id() != indexed.snapshot().id()
            || !published
                .publication()
                .modules()
                .repository_card()
                .languages()
                .contains(&definition.language)
        {
            return Err(test_error(
                "fixture published index has incompatible anchors",
            ));
        }
        Ok(Self {
            definition,
            repository,
            _app_data: app_data,
            repository_tree,
            project,
            store,
            snapshot_id: indexed.snapshot().id(),
        })
    }
}

struct DurableRun {
    goal: GoalContract,
    ledger: TaskLedger,
    ledger_version: TaskLedgerStoreVersion,
    run: AgentRun,
    profile: ModelProfile,
    step_id: TaskStepId,
    criterion_id: AcceptanceCriterionId,
}

impl DurableRun {
    async fn new(fixture: &IndexedFixture) -> Result<Self, Box<dyn Error>> {
        let seed = fixture.definition.seed;
        let criterion_id = AcceptanceCriterionId::from_bytes(id_bytes(seed, 2));
        let goal = GoalContract::initial(
            TaskId::from_bytes(id_bytes(seed, 1)),
            GoalContractDraft::new(
                GoalObjective::try_from_string(format!(
                    "Locate {} in {} and retain current source evidence",
                    fixture.definition.query, fixture.definition.name
                ))?,
                vec![AcceptanceCriterion::new(
                    criterion_id,
                    AcceptanceCriterionStatement::try_from_string(format!(
                        "{} is resolved from the current published snapshot",
                        fixture.definition.query
                    ))?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    "verify the retained source evidence against the published snapshot".to_owned(),
                )?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let step_id = TaskStepId::from_bytes(id_bytes(seed, 3));
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![TaskStepDefinition::new(
                step_id,
                None,
                TaskStepOutcome::try_from_string(format!(
                    "Resolve {} with clickable current source evidence",
                    fixture.definition.query
                ))?,
                TaskStepRationale::try_from_string(
                    "exercise the complete bounded read-only controller path".to_owned(),
                )?,
                Vec::new(),
                vec![ExpectedTaskEvidence::try_from_string(
                    "content-addressed source location".to_owned(),
                )?],
                VerificationSpec::new(
                    VerificationSpecId::from_bytes(id_bytes(seed, 4)),
                    VerificationMethod::Diagnostic,
                    VerificationRequirement::try_from_string(
                        "resolve the expected path from current tool evidence".to_owned(),
                    )?,
                ),
            )?],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        let run_id = AgentRunId::from_bytes(id_bytes(seed, 5));
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(2)?)?;
        CreateGoalContract::new(fixture.store.as_ref())
            .execute(&fixture.project, &goal)
            .await?;
        let ledger_version = CreateTaskLedger::new(fixture.store.as_ref())
            .execute(&fixture.project, &ledger)
            .await?
            .version();
        let profile = model_profile()?;
        let (mut run, start_event) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            profile.reference(),
            fixture.snapshot_id,
            event_id(seed, 10),
            timestamp(10)?,
        )?;
        CreateAgentRun::new(fixture.store.as_ref())
            .execute(&fixture.project, &run, &start_event)
            .await?;
        for (signal, ordinal, at) in [
            (AgentControllerSignal::AnchorsAccepted, 11, 11),
            (AgentControllerSignal::LocalizationComplete, 12, 12),
            (AgentControllerSignal::PlanReady, 13, 13),
        ] {
            advance_run(
                fixture.store.as_ref(),
                &fixture.project,
                &mut run,
                signal,
                event_id(seed, ordinal),
                timestamp(at)?,
            )
            .await?;
        }
        Ok(Self {
            goal,
            ledger,
            ledger_version,
            run,
            profile,
            step_id,
            criterion_id,
        })
    }

    fn context_input(
        &self,
        fixture: &IndexedFixture,
        tool_results: Vec<a3_application::ContextToolResult>,
    ) -> Result<AgentContextCompileInput, Box<dyn Error>> {
        Ok(AgentContextCompileInput::new(
            fixture.project.clone(),
            self.goal.clone(),
            self.ledger.clone(),
            self.step_id,
            self.profile.clone(),
            None,
            Vec::new(),
            tool_results,
        )?)
    }
}

async fn advance_run(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: &mut AgentRun,
    signal: AgentControllerSignal,
    event_id: RunEventId,
    observed_at: AgentRunTimestamp,
) -> Result<(), Box<dyn Error>> {
    let expected_sequence = run.last_event_sequence();
    let advance = AdvanceAgentController.execute(
        run,
        signal,
        event_id,
        run.current_snapshot_id(),
        observed_at,
        false,
    )?;
    AppendRunEvent::new(store)
        .execute(project, expected_sequence, run, advance.event())
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct FixtureAcceptanceVerifier {
    criterion_id: AcceptanceCriterionId,
    evidence_ids: Vec<TaskEvidenceId>,
    snapshot_id: SnapshotId,
}

impl AcceptanceVerifier for FixtureAcceptanceVerifier {
    fn verify<'a>(
        &'a self,
        request: &'a AcceptanceVerificationRequest,
        _timeout: a3_application::AcceptanceVerifierTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AcceptanceVerifierFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(AcceptanceVerifierFailure::Cancelled);
            }
            if request.snapshot_id() != self.snapshot_id || self.evidence_ids.is_empty() {
                return Err(AcceptanceVerifierFailure::InvalidResult);
            }
            let criterion =
                AcceptanceCriterionVerification::new(self.criterion_id, self.evidence_ids.clone())
                    .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?;
            let receipt = AcceptanceVerificationReceipt::new(
                request.run_id(),
                request.goal_contract(),
                request.task_ledger().revision(),
                request.snapshot_id(),
                vec![criterion],
            )
            .map_err(|_| AcceptanceVerifierFailure::InvalidResult)?;
            Ok(AcceptanceVerifierOutcome::Accepted(receipt))
        })
    }
}

#[derive(Debug)]
struct CountingReadTools<'a> {
    inner: DeterministicAgentReadTools<'a>,
    calls: AtomicUsize,
}

impl AgentReadTools for CountingReadTools<'_> {
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        tool_run_id: a3_domain::ToolRunId,
        action: &'a AgentReadAction,
        timeout: AgentReadTimeout,
        control: &'a dyn AgentControllerControl,
    ) -> AgentReadToolsFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .execute(project, snapshot_id, tool_run_id, action, timeout, control)
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveControl;

impl AgentControllerControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl ContextCompileControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_phase(&self, _phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl ModelOperationControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(futures::future::pending())
    }
}

impl RepositoryIndexControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl IndexPersistenceControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

fn model_profile() -> Result<ModelProfile, Box<dyn Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("contract-stub".to_owned())?,
        ModelId::try_from_string("m6-fixture-model".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(16_384)?,
            ModelOutputLimit::new(4_096)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}

fn provider_events(raw: &str) -> Result<Vec<ProviderEvent>, Box<dyn Error>> {
    Ok(vec![
        ProviderEvent::OutputText(ModelOutputChunk::try_from_string(raw.to_owned())?),
        ProviderEvent::Completed(ModelProviderCompletion::new(
            ModelFinishReason::Stop,
            ModelProviderUsage::new(Some(100), Some(20)),
        )),
    ])
}

fn id_bytes(seed: u8, kind: u8) -> [u8; 32] {
    let mut bytes = [seed; 32];
    bytes[0] = kind;
    bytes[1] = seed;
    bytes
}

fn event_id(seed: u8, ordinal: u8) -> RunEventId {
    RunEventId::from_bytes(id_bytes(seed, ordinal))
}

fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
    Ok(AgentRunTimestamp::from_unix_millis(value)?)
}

fn test_error(message: &'static str) -> Box<dyn Error> {
    std::io::Error::other(message).into()
}
