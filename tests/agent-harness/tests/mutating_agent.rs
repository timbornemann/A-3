//! E7 acceptance over the real policy, patch, index, context, journal, and libSQL boundaries.

mod support;

use a3_application::{
    AdvanceAgentController, AgentControllerControl, AgentControllerSignal, AppendRunEvent,
    CompileTaskLens, ConfirmProjectCommandAllowlist,
    ConservativeProcessVerificationEvidenceFactory, ContextCompileControl, ContextCompilePhase,
    CreateAgentRun, CreateGoalContract, CreateTaskLedger, DiscoverProjectCommands,
    ExecuteMutatingAgentAction, GrantPolicyApproval, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeStore, MutationActionFingerprint,
    MutationCommandSelection, MutationContextSeed, MutationControllerFailure,
    MutationControllerOutcome, MutationExecutionIds, PolicyStore, ProcessEventSink,
    ProcessEventSinkError, ProcessRunControl, ProcessRunFailure, ProcessRunFuture, ProcessRunner,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryRescanReason, RunEventPageLimit, RunJournalStore,
    TaskLedgerStoreVersion, TaskLensControlError, WorkspacePatchControl,
    WorkspacePatchProgressError, WorktreeMutationCoordinator,
};
use a3_context::DeterministicAgentContextCompiler;
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentAction,
    AgentControllerState, AgentRun, AgentRunAction, AgentRunId, AgentRunTimestamp, ApprovalId,
    ApprovalRequestId, ApprovalStatus, ContentHash, DiffInvariantMode, DiffInvariantVerification,
    DiscoveredCommandKind, ExpectedTaskEvidence, FileRevision, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, ModelCapabilities, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy,
    ModelToolCallMode, ModelTopP, PatchAction, PatchActionSchemaVersion, PatchFileContent,
    PatchOperation, PatchRationale, PatchUpdate, PolicyDecisionId, PolicyResourceId, ProcessEvent,
    Progress, ProjectIdentity, PublishedIndex, RepositoryPath, RunEventId, RunEventKind,
    SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, TaskStepStatus, ToolRunId, VerificationRequirement,
    VerificationRunId, VerificationScope, VerificationSpec, VerificationSpecId, WorkspacePolicy,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::{RepositoryInspector, WorkspacePatchAdapter};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use support::{TempDirectory, run_libsql_test};

const ORIGINAL_SOURCE: &[u8] = b"pub fn value() -> u32 { 1 }\n";
const UPDATED_SOURCE: &[u8] = b"pub fn value() -> u32 { 2 }\n";

#[test]
fn patch_waits_for_approval_then_reindexes_before_compiling_context() -> Result<(), Box<dyn Error>>
{
    run_libsql_test(async {
        let fixture = Fixture::new().await?;
        let criterion_id = AcceptanceCriterionId::from_bytes(id(20));
        let step_id = TaskStepId::from_bytes(id(21));
        let spec_id = VerificationSpecId::from_bytes(id(22));
        let spec = VerificationSpec::user_confirm(
            spec_id,
            requirement("the user confirms the indexed patch result")?,
            PolicyResourceId::from_bytes(id(23)),
        );
        let mut durable = DurableMutation::new(&fixture, criterion_id, step_id, spec).await?;
        let replacement = PatchFileContent::try_from_bytes(UPDATED_SOURCE.to_vec())?;
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            durable.run.id(),
            fixture.project.worktree().id(),
            fixture.published.run().snapshot_id(),
            step_id,
            spec_id,
            PatchRationale::try_from_string(
                "apply the E7 context freshness acceptance fixture".to_owned(),
            )?,
            vec![PatchOperation::Update(PatchUpdate::new(
                FileRevision::new(path("src/lib.rs")?, hash(ORIGINAL_SOURCE)),
                replacement,
            )?)],
        )?;

        let refresh = refresh(fixture.store.clone());
        let context = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
        ));
        let coordinator = WorktreeMutationCoordinator::new();
        let patch_tool = WorkspacePatchAdapter::new();
        let process_runner = FailingProcessRunner::default();
        let evidence_factory = ConservativeProcessVerificationEvidenceFactory;
        let controller = ExecuteMutatingAgentAction::new(
            &coordinator,
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &patch_tool,
            &process_runner,
            &evidence_factory,
            &context,
            &refresh,
        );
        let seed = durable.context_seed();
        let mut compiler = compiler()?;
        let events = NoopProcessEvents;

        let first = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                AgentAction::ApplyPatch(Box::new(action.clone())),
                None,
                &WorkspacePolicy::unrestricted(),
                None,
                mutation_ids(40),
                timestamp(20)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        let MutationControllerOutcome::AwaitingApproval(request_id) = first else {
            return Err(test_error("patch crossed policy without exact approval"));
        };
        if durable.run.state() != AgentControllerState::AwaitApproval
            || durable.ledger.step(step_id).map(|step| step.status())
                != Some(TaskStepStatus::AwaitingApproval)
            || std::fs::read(fixture.repository.path().join("src/lib.rs"))? != ORIGINAL_SOURCE
        {
            return Err(test_error(
                "approval wait changed the worktree or lost durable state",
            ));
        }

        let approval_id = ApprovalId::from_bytes(id(70));
        let mut approval = GrantPolicyApproval::new(fixture.store.as_ref())
            .execute(
                &fixture.project,
                &mut durable.run,
                request_id,
                approval_id,
                RunEventId::from_bytes(id(71)),
                fixture.published.run().snapshot_id(),
                timestamp(21)?,
            )
            .await?;
        let original_snapshot = fixture.published.run().snapshot_id();
        let outcome = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                AgentAction::ApplyPatch(Box::new(action)),
                None,
                &WorkspacePolicy::unrestricted(),
                Some(&mut approval),
                mutation_ids(80),
                timestamp(22)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        let MutationControllerOutcome::NextAction(compiled) = outcome else {
            return Err(test_error(
                "non-diff patch did not request fresh verification context",
            ));
        };
        let latest = fixture
            .store
            .latest_published_index(&fixture.project, &ActiveControl)
            .await?
            .ok_or_else(|| test_error("patched index publication is missing"))?;
        if compiled.snapshot_id() == original_snapshot
            || compiled.snapshot_id() != latest.run().snapshot_id()
            || compiled.snapshot_id() != durable.run.current_snapshot_id()
            || durable.run.state() != AgentControllerState::Execute
            || durable.ledger.step(step_id).map(|step| step.status())
                != Some(TaskStepStatus::InProgress)
            || std::fs::read(fixture.repository.path().join("src/lib.rs"))? != UPDATED_SOURCE
        {
            return Err(test_error(
                "patch result, index, run, ledger, and context are not fresh",
            ));
        }
        let stored_approval = fixture
            .store
            .load_approval(&fixture.project, approval_id)
            .await?
            .ok_or_else(|| test_error("consumed approval is missing"))?;
        if stored_approval.status_at(timestamp(22)?) != ApprovalStatus::Consumed {
            return Err(test_error("patch approval was not consumed durably"));
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
        let tool_position = page
            .events()
            .iter()
            .position(|event| event.kind() == RunEventKind::ToolAction)
            .ok_or_else(|| test_error("patch tool event is missing"))?;
        let context_position = page
            .events()
            .iter()
            .rposition(|event| event.kind() == RunEventKind::ContextCompiled)
            .ok_or_else(|| test_error("fresh context event is missing"))?;
        if tool_position >= context_position
            || page.events()[context_position].snapshot_id() != latest.run().snapshot_id()
        {
            return Err(test_error(
                "context was recorded before patch invalidation completed",
            ));
        }
        Ok(())
    })
}

#[test]
fn diff_patch_completes_step_only_after_typed_current_verification() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let fixture = Fixture::new().await?;
        let criterion_id = AcceptanceCriterionId::from_bytes(id(90));
        let step_id = TaskStepId::from_bytes(id(91));
        let spec_id = VerificationSpecId::from_bytes(id(92));
        let source_path = path("src/lib.rs")?;
        let spec = VerificationSpec::diff_invariant(
            spec_id,
            requirement("exactly src/lib.rs changes")?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![source_path.clone()],
            )?,
        );
        let mut durable = DurableMutation::new(&fixture, criterion_id, step_id, spec).await?;
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            durable.run.id(),
            fixture.project.worktree().id(),
            fixture.published.run().snapshot_id(),
            step_id,
            spec_id,
            PatchRationale::try_from_string("verify the exact E7 patch path".to_owned())?,
            vec![PatchOperation::Update(PatchUpdate::new(
                FileRevision::new(source_path, hash(ORIGINAL_SOURCE)),
                PatchFileContent::try_from_bytes(UPDATED_SOURCE.to_vec())?,
            )?)],
        )?;
        let refresh = refresh(fixture.store.clone());
        let context = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
        ));
        let coordinator = WorktreeMutationCoordinator::new();
        let patch_tool = WorkspacePatchAdapter::new();
        let process_runner = FailingProcessRunner::default();
        let evidence_factory = ConservativeProcessVerificationEvidenceFactory;
        let controller = ExecuteMutatingAgentAction::new(
            &coordinator,
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &patch_tool,
            &process_runner,
            &evidence_factory,
            &context,
            &refresh,
        );
        let seed = durable.context_seed();
        let mut compiler = compiler()?;
        let events = NoopProcessEvents;
        let first = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                AgentAction::ApplyPatch(Box::new(action.clone())),
                None,
                &WorkspacePolicy::unrestricted(),
                None,
                mutation_ids(100),
                timestamp(20)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        let MutationControllerOutcome::AwaitingApproval(request_id) = first else {
            return Err(test_error("diff patch did not wait for approval"));
        };
        let mut approval = GrantPolicyApproval::new(fixture.store.as_ref())
            .execute(
                &fixture.project,
                &mut durable.run,
                request_id,
                ApprovalId::from_bytes(id(130)),
                RunEventId::from_bytes(id(131)),
                fixture.published.run().snapshot_id(),
                timestamp(21)?,
            )
            .await?;
        let outcome = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                AgentAction::ApplyPatch(Box::new(action)),
                None,
                &WorkspacePolicy::unrestricted(),
                Some(&mut approval),
                mutation_ids(140),
                timestamp(22)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        let MutationControllerOutcome::StepVerified { snapshot_id, .. } = outcome else {
            return Err(test_error(
                "diff patch completed without a typed passing verification",
            ));
        };
        if snapshot_id == fixture.published.run().snapshot_id()
            || snapshot_id != durable.run.current_snapshot_id()
            || durable.run.state() != AgentControllerState::Verify
            || durable.ledger.step(step_id).map(|step| step.status())
                != Some(TaskStepStatus::Completed)
        {
            return Err(test_error(
                "typed diff verification did not complete the current step",
            ));
        }
        Ok(())
    })
}

#[test]
fn one_worktree_lock_and_repeated_failed_run_force_replan() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let fixture = Fixture::new().await?;
        let catalog =
            DiscoverProjectCommands.execute(fixture.project.worktree().id(), &fixture.published)?;
        let command = catalog
            .commands()
            .iter()
            .find(|command| command.kind() == DiscoveredCommandKind::Test)
            .ok_or_else(|| test_error("fixture test command was not discovered"))?;
        let confirmation = ConfirmProjectCommandAllowlist::new(fixture.store.as_ref())
            .execute(
                &fixture.project,
                &catalog,
                vec![command.id()],
                timestamp(10)?,
                None,
            )
            .await?;
        let criterion_id = AcceptanceCriterionId::from_bytes(id(120));
        let step_id = TaskStepId::from_bytes(id(121));
        let spec = VerificationSpec::command(
            VerificationSpecId::from_bytes(id(122)),
            requirement("the discovered command exits successfully")?,
            command.id(),
            VerificationScope::Workspace,
        );
        let mut durable = DurableMutation::new(&fixture, criterion_id, step_id, spec).await?;
        let action = AgentAction::Run(AgentRunAction::new(step_id, command.id()));
        let selection = MutationCommandSelection::new(&catalog, &confirmation);
        let refresh = refresh(fixture.store.clone());
        let context = DeterministicAgentContextCompiler::new(CompileTaskLens::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
        ));
        let coordinator = WorktreeMutationCoordinator::new();
        let patch_tool = WorkspacePatchAdapter::new();
        let process_runner = FailingProcessRunner::default();
        let evidence_factory = ConservativeProcessVerificationEvidenceFactory;
        let controller = ExecuteMutatingAgentAction::new(
            &coordinator,
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &patch_tool,
            &process_runner,
            &evidence_factory,
            &context,
            &refresh,
        );
        let seed = durable.context_seed();
        let mut compiler = compiler()?;
        let events = NoopProcessEvents;

        let fingerprint = MutationActionFingerprint::from_action(&action)?;
        let blocking_lease = coordinator.try_acquire(
            durable.run.id(),
            fixture.project.worktree().id(),
            fingerprint,
        )?;
        let blocked = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                action.clone(),
                Some(selection),
                &WorkspacePolicy::unrestricted(),
                None,
                mutation_ids(140),
                timestamp(20)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await;
        if !matches!(blocked, Err(MutationControllerFailure::Busy(_)))
            || process_runner.calls.load(Ordering::SeqCst) != 0
        {
            return Err(test_error(
                "a second worktree mutation crossed the shared lock",
            ));
        }
        drop(blocking_lease);

        let first = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                action.clone(),
                Some(selection),
                &WorkspacePolicy::unrestricted(),
                None,
                mutation_ids(160),
                timestamp(21)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        if !matches!(first, MutationControllerOutcome::NextAction(_))
            || durable.run.state() != AgentControllerState::Execute
        {
            return Err(test_error(
                "first failed command did not take the one bounded retry",
            ));
        }
        let second = controller
            .execute(
                &fixture.project,
                &mut durable.run,
                &mut durable.ledger,
                &mut durable.ledger_version,
                &fixture.published,
                action,
                Some(selection),
                &WorkspacePolicy::unrestricted(),
                None,
                mutation_ids(180),
                timestamp(22)?,
                timestamp(100)?,
                &seed,
                &mut compiler,
                &events,
                &ActiveControl,
            )
            .await?;
        if !matches!(second, MutationControllerOutcome::ReplanRequired { .. })
            || durable.run.state() != AgentControllerState::Replan
            || durable.ledger.step(step_id).map(|step| step.status())
                != Some(TaskStepStatus::Failed)
            || process_runner.calls.load(Ordering::SeqCst) != 2
        {
            return Err(test_error(
                "identical failed command did not force durable replan",
            ));
        }
        Ok(())
    })
}

struct Fixture {
    repository: TempDirectory,
    _app_data: TempDirectory,
    project: ProjectIdentity,
    store: Arc<LibsqlKnowledgeStore>,
    published: PublishedIndex,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write(
            "Cargo.toml",
            b"[package]\nname = \"e7-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        repository.write("src/lib.rs", ORIGINAL_SOURCE)?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = refresh(store.clone());
        let mut compiler = compiler()?;
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
        let published = indexed.published_index().clone();
        Ok(Self {
            repository,
            _app_data: app_data,
            project,
            store,
            published,
        })
    }
}

struct DurableMutation {
    goal: GoalContract,
    ledger: TaskLedger,
    ledger_version: TaskLedgerStoreVersion,
    run: AgentRun,
    profile: ModelProfile,
}

impl DurableMutation {
    async fn new(
        fixture: &Fixture,
        criterion_id: AcceptanceCriterionId,
        step_id: TaskStepId,
        spec: VerificationSpec,
    ) -> Result<Self, Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes(*criterion_id.as_bytes()),
            GoalContractDraft::new(
                GoalObjective::try_from_string("exercise the finite E7 mutation path".to_owned())?,
                vec![AcceptanceCriterion::new(
                    criterion_id,
                    AcceptanceCriterionStatement::try_from_string(
                        "the mutation is policy-bound, fresh, and objectively resolved".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    "run the offline E7 acceptance contract".to_owned(),
                )?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let definition = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("complete one bounded mutation".to_owned())?,
            TaskStepRationale::try_from_string(
                "prove the composed mutation controller invariants".to_owned(),
            )?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "durable controller and repository evidence".to_owned(),
            )?],
            spec,
        )?
        .with_acceptance_criteria(vec![criterion_id])?;
        let run_id = AgentRunId::from_bytes(*step_id.as_bytes());
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![definition],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(3)?)?;
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
            fixture.published.run().snapshot_id(),
            RunEventId::from_bytes(id(4)),
            timestamp(4)?,
        )?;
        CreateAgentRun::new(fixture.store.as_ref())
            .execute(&fixture.project, &run, &start_event)
            .await?;
        for (signal, event, at) in [
            (AgentControllerSignal::AnchorsAccepted, 5, 5),
            (AgentControllerSignal::LocalizationComplete, 6, 6),
            (AgentControllerSignal::PlanReady, 7, 7),
        ] {
            advance(
                fixture.store.as_ref(),
                &fixture.project,
                &mut run,
                signal,
                RunEventId::from_bytes(id(event)),
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
        })
    }

    fn context_seed(&self) -> MutationContextSeed {
        MutationContextSeed::new(
            self.goal.clone(),
            self.profile.clone(),
            Vec::new(),
            Vec::new(),
        )
    }
}

async fn advance(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run: &mut AgentRun,
    signal: AgentControllerSignal,
    event_id: RunEventId,
    observed_at: AgentRunTimestamp,
) -> Result<(), Box<dyn Error>> {
    let expected = run.last_event_sequence();
    let outcome = AdvanceAgentController.execute(
        run,
        signal,
        event_id,
        run.current_snapshot_id(),
        observed_at,
        false,
    )?;
    AppendRunEvent::new(store)
        .execute(project, expected, run, outcome.event())
        .await?;
    Ok(())
}

fn refresh(store: Arc<LibsqlKnowledgeStore>) -> RefreshRepositoryIndex {
    let index_store: Arc<dyn KnowledgeIndexStore> = store;
    RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        index_store,
        Arc::new(Blake3IndexRunIdFactory),
    )
}

fn compiler() -> Result<BuiltinIncrementalIndexCompiler, Box<dyn Error>> {
    Ok(BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(
        2,
    )?)?)
}

fn model_profile() -> Result<ModelProfile, Box<dyn Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("e7-contract".to_owned())?,
        ModelId::try_from_string("e7-local-model".to_owned())?,
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

fn mutation_ids(base: u8) -> MutationExecutionIds {
    MutationExecutionIds::new(
        PolicyDecisionId::from_bytes(id(base)),
        ApprovalRequestId::from_bytes(id(base.wrapping_add(1))),
        RunEventId::from_bytes(id(base.wrapping_add(2))),
        RunEventId::from_bytes(id(base.wrapping_add(3))),
        ToolRunId::from_bytes(id(base.wrapping_add(4))),
        RunEventId::from_bytes(id(base.wrapping_add(5))),
        RunEventId::from_bytes(id(base.wrapping_add(6))),
        RunEventId::from_bytes(id(base.wrapping_add(7))),
        RunEventId::from_bytes(id(base.wrapping_add(8))),
        VerificationRunId::from_bytes(id(base.wrapping_add(9))),
        a3_domain::StepVerificationId::from_bytes(id(base.wrapping_add(10))),
    )
}

fn requirement(value: &str) -> Result<VerificationRequirement, Box<dyn Error>> {
    Ok(VerificationRequirement::try_from_string(value.to_owned())?)
}

fn path(value: &str) -> Result<RepositoryPath, Box<dyn Error>> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

fn hash(value: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(value).as_bytes())
}

const fn id(value: u8) -> [u8; 32] {
    [value; 32]
}

fn timestamp(value: u64) -> Result<AgentRunTimestamp, Box<dyn Error>> {
    Ok(AgentRunTimestamp::from_unix_millis(value)?)
}

fn test_error(message: &'static str) -> Box<dyn Error> {
    std::io::Error::other(message).into()
}

#[derive(Debug, Default)]
struct FailingProcessRunner {
    calls: AtomicUsize,
}

impl ProcessRunner for FailingProcessRunner {
    fn run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _authorized: a3_application::AuthorizedProcessSpec,
        _control: &'a dyn ProcessRunControl,
        _events: &'a dyn ProcessEventSink,
    ) -> ProcessRunFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ProcessRunFailure::SpawnUnavailable) })
    }
}

#[derive(Debug, Clone, Copy)]
struct NoopProcessEvents;

impl ProcessEventSink for NoopProcessEvents {
    fn emit(&self, _event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        Ok(())
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

impl WorkspacePatchControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}

impl ProcessRunControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn wait_cancelled_timeout(&self, _timeout: Duration) -> bool {
        false
    }
}
