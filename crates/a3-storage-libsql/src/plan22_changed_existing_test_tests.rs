//! Actual failing check and patch-to-check controls; no model or invented verification artifacts.
#[path = "plan22_role_evidence_tests.rs"]
mod role_evidence_tests;

#[path = "plan22_actual_horizon_tests.rs"]
mod actual_horizon_tests;

#[path = "plan22_p4_plan_projection_tests.rs"]
mod p4_plan_projection_tests;

#[path = "plan22_documentation_change_tests.rs"]
mod documentation_change_tests;

#[path = "plan22_diff_recovery_tests.rs"]
mod diff_recovery_tests;

#[path = "plan22_middle_edit_tests.rs"]
mod middle_edit_tests;

#[path = "plan22_patch_conflict_tests.rs"]
mod patch_conflict_tests;

#[path = "plan22_manifest_bound_test_tests.rs"]
mod manifest_bound_test_tests;

#[path = "plan22_p5_live_a01.rs"]
mod p5_live_a01;

#[path = "plan22_project_ask_tests.rs"]
mod project_ask_tests;

use super::*;
use a3_application::{
    AcceptanceVerificationRequestError, MutationControllerFailure, RequestAgentFinish,
};
use a3_domain::{AgentFinishAction, ProjectCommandCatalog, PublishedIndex};

impl a3_application::IndexPersistenceControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(
        &self,
        _: Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}

const WRONG_SUBJECT: &[u8] = b"def average(values):\n    if not values:\n        raise ValueError('empty')\n    return sum(values) // len(values)\n";
const REPAIR_GOAL: &[u8] = b"Fix average: average([1, 2]) returns 1.5; average([]) raises ValueError.\nDo not create or change tests.\n";

#[test]
fn actual_patch_then_real_context_and_bound_existing_test_survive_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = wrong_fixture()?;
            let worktree = fixture.root.join("worktree");
            let mut expected = inventory(&worktree)?;
            let subject = expected
                .iter_mut()
                .find(|(path, _)| path == Path::new("average.py"))
                .ok_or("initial subject missing")?;
            subject.1 = SUBJECT.to_vec();
            let (goal, evidence, snapshot) = run_patch_then_existing_check(&fixture).await?;
            assert_eq!(inventory(&worktree)?, expected);
            finish_reopened_source(&fixture, goal, evidence, snapshot, 40).await?;
            assert_eq!(inventory(&worktree)?, expected);
            fixture.cleanup()?;
            Ok(())
        })
    })
}

async fn run_patch_then_existing_check(
    fixture: &Fixture,
) -> TestResult<(GoalContract, TaskEvidenceId, SnapshotId)> {
    // This is the existing legacy native-context fixture envelope, not a model
    // benchmark profile or a claim about the new P4/4k phase protocol.
    let PreparedExistingRun {
        store,
        refresh,
        mut compiler,
        published,
        catalog,
        confirmation,
        goal,
        mut ledger,
        mut version,
        profile,
        mut run,
        ..
    } = prepare_existing_run(fixture, legacy_context_profile()?, repair_goal).await?;
    let initial_snapshot = published.run().snapshot_id();
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::new();
    let runner = native_runner()?;
    let factory = BuiltinProcessVerificationEvidenceFactory;
    let source = a3_workspace::WorkspaceAgentSourceReader;
    let context = ObservedRealContext {
        inner: a3_context::DeterministicAgentContextCompiler::new(
            a3_application::CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref()),
            &source,
        ),
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
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
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let spec_id = ledger
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("step missing")?
        .definition()
        .verification_spec()
        .id();
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path().as_bytes() == b"average.py")
        .ok_or("current source revision missing")?
        .clone();
    assert_eq!(
        revision.content_hash().as_bytes(),
        blake3::hash(WRONG_SUBJECT).as_bytes()
    );
    let action = a3_domain::PatchAction::new(
        a3_domain::PatchActionSchemaVersion::V1,
        run.id(),
        fixture.project.worktree().id(),
        initial_snapshot,
        TaskStepId::from_bytes([STEP; 32]),
        spec_id,
        a3_domain::PatchRationale::try_from_string(
            "Replace incorrect integer division with arithmetic division".to_owned(),
        )?,
        vec![a3_domain::PatchOperation::Update(
            a3_domain::PatchUpdate::new(
                revision,
                a3_domain::PatchFileContent::try_from_bytes(SUBJECT.to_vec())?,
            )?,
        )],
    )?;
    let patched = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::ApplyPatch(Box::new(action)),
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
    )
    .await?;
    let MutationControllerOutcome::NextAction(compiled) = patched else {
        return Err(format!("patch did not produce a real current context: {patched:?}").into());
    };
    assert_ne!(run.current_snapshot_id(), initial_snapshot);
    assert_eq!(compiled.snapshot_id(), run.current_snapshot_id());
    assert_eq!(compiled.goal_contract(), goal.reference());
    assert_eq!(compiled.ledger_revision(), ledger.revision());
    assert_eq!(
        compiled.current_step_id(),
        TaskStepId::from_bytes([STEP; 32])
    );
    assert_eq!(context.calls.load(Ordering::SeqCst), 1);
    let step = ledger
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("post-patch step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    assert_eq!(step.definition().verification_spec().id(), spec_id);
    assert!(
        step.attempts()
            .last()
            .ok_or("post-patch attempt missing")?
            .verification()
            .is_none()
    );
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "event capture poisoned")?
            .is_empty(),
        "a patch must not fabricate process evidence"
    );

    let current = store
        .latest_published_index(&fixture.project, &ActiveControl)
        .await?
        .ok_or("post-patch publication missing")?;
    assert_eq!(current.run().snapshot_id(), run.current_snapshot_id());
    assert_cold_context_ports(fixture, &current).await?;
    let current_catalog =
        DiscoverProjectCommands.execute(fixture.project.worktree().id(), &current)?;
    let check = RequestAgentFinish
        .verification_command(step)
        .ok_or("bound planned check missing")?;
    assert_ne!(catalog.id(), current_catalog.id());
    assert!(matches!(
        current_catalog.bind_confirmed(
            confirmation.allowlist(),
            run.id(),
            check.step_id(),
            check.command_id(),
        ),
        Err(a3_domain::DiscoveredCommandProcessError::StaleAllowlist)
    ));
    assert!(current_catalog.commands().iter().any(
        |entry| entry.id() == check.command_id() && entry.kind() == DiscoveredCommandKind::Test
    ));
    // Reconfirm the current exact catalog, then separately grant this actual
    // process action on snapshot B. Neither the patch grant nor old catalog is reused.
    let current_confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(
            &fixture.project,
            &current_catalog,
            vec![check.command_id()],
            at(19)?,
            Some(confirmation.version()),
        )
        .await?;
    let outcome = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(check),
        Some((&current_catalog, &current_confirmation)),
        100,
        20,
        &seed,
        &mut compiler,
        &events,
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("bound current check did not verify: {outcome:?}").into());
    };
    assert_eq!(context.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(snapshot_id, current.run().snapshot_id());
    let state = store
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[evidence_id],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::Test(evidence)] = state.evidence() else {
        return Err("actual TestEvidence missing".into());
    };
    assert_eq!(evidence.cases().len(), 2);
    assert_eq!(evidence.command().spec_id(), spec_id);
    assert_eq!(evidence.command().snapshot_id(), snapshot_id);
    assert_eq!(evidence.command().command_id(), check.command_id());
    assert!(
        matches!(evidence.command().termination(), ProcessTermination::Exited(exit) if exit.success())
    );
    let attempts = store
        .load_agent_mutation_attempts(&fixture.project, run.id())
        .await?;
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        attempts
            .iter()
            .filter(|attempt| attempt.kind() == AgentMutationKind::Patch)
            .count(),
        1
    );
    assert_eq!(
        attempts
            .iter()
            .filter(|attempt| attempt.kind() == AgentMutationKind::Process)
            .count(),
        1
    );
    assert!(
        attempts
            .iter()
            .all(|attempt| attempt.tool_attempt().status() == AgentToolAttemptStatus::Succeeded)
    );
    Ok((goal, evidence_id, snapshot_id))
}

async fn assert_cold_context_ports(fixture: &Fixture, index: &PublishedIndex) -> TestResult {
    use a3_application::{KnowledgeSearchStore, TaskLensClaimStore};
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let result = store
            .search_exact(
                &fixture.project,
                &a3_domain::ExactSearchQuery::Path(RepositoryPath::try_from_bytes(
                    b"average.py".to_vec(),
                )?),
                a3_domain::ExactSearchPageSize::new(1)?,
                None,
                &ActiveControl,
            )
            .await?;
        assert_eq!(result.hits().len(), 1);
    }
    {
        let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let result = store
            .load_claims(
                &fixture.project,
                index,
                a3_application::TaskLensClaimLimit::DEFAULT,
                &ActiveControl,
            )
            .await?;
        assert!(result.claims().is_empty());
        assert!(!result.truncated());
    }
    Ok(())
}

impl a3_application::KnowledgeSearchControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl a3_application::TaskLensControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

struct ObservedRealContext<'a> {
    inner: a3_context::DeterministicAgentContextCompiler<'a>,
    calls: std::sync::atomic::AtomicUsize,
}
impl std::fmt::Debug for ObservedRealContext<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ObservedRealContext")
    }
}
impl AgentContextCompiler for ObservedRealContext<'_> {
    fn compile<'a>(
        &'a self,
        input: &'a AgentContextCompileInput,
        control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.compile(input, control)
    }
}

fn legacy_context_profile() -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("native-existing-test-fixture".to_owned())?,
        ModelId::try_from_string("not-invoked".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(16_384)?,
            ModelOutputLimit::new(4_096)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

#[test]
fn actual_wrong_existing_code_stays_open_without_invented_failed_test_evidence() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = wrong_fixture()?;
            let before = inventory(&fixture.root.join("worktree"))?;
            let goal = run_actual_failed_check(&fixture).await?;
            assert_eq!(inventory(&fixture.root.join("worktree"))?, before);
            {
                let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                let run = store
                    .load_agent_run(&fixture.project, AgentRunId::from_bytes([RUN; 32]))
                    .await?
                    .ok_or("failed-check run missing after reopen")?;
                let ledger = store
                    .load_task_ledger(&fixture.project, goal.task_id())
                    .await?
                    .ok_or("failed-check ledger missing after reopen")?;
                assert_eq!(run.state(), AgentControllerState::Verify);
                let step = ledger
                    .ledger()
                    .step(TaskStepId::from_bytes([STEP; 32]))
                    .ok_or("step missing")?;
                assert_eq!(step.status(), TaskStepStatus::InProgress);
                assert!(
                    step.attempts()
                        .last()
                        .ok_or("attempt missing")?
                        .verification()
                        .is_none()
                );
                let current = store
                    .latest_published_index(&fixture.project, &ActiveControl)
                    .await?
                    .ok_or("failed-check publication missing")?;
                let memory = RunMemoryCheckpoint::compile(
                    &goal,
                    ledger.ledger(),
                    &run,
                    &current,
                    Vec::new(),
                )?;
                assert!(matches!(
                    AcceptanceVerificationRequest::new(
                        fixture.project.clone(),
                        &run,
                        goal.clone(),
                        ledger.ledger().clone(),
                        memory,
                    ),
                    Err(AcceptanceVerificationRequestError::IncompleteLedger)
                ));
                assert_eq!(
                    store
                        .load_current_goal_contract(&fixture.project, goal.task_id())
                        .await?
                        .ok_or("failed-check original missing")?,
                    goal
                );
            }
            fixture.cleanup()?;
            Ok(())
        })
    })
}

fn with_context_runtime(future: impl std::future::Future<Output = TestResult>) -> TestResult {
    // The scoped owner lives inside the native harness Future, so it is dropped
    // before that harness writes completion and exits its Windows child.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let result = runtime.block_on(future);
    drop(runtime);
    result
}

async fn run_actual_failed_check(fixture: &Fixture) -> TestResult<GoalContract> {
    let PreparedExistingRun {
        store,
        refresh,
        mut compiler,
        published,
        catalog,
        confirmation,
        goal,
        mut ledger,
        mut version,
        profile,
        mut run,
        ..
    } = prepare_existing_run(fixture, profile()?, repair_goal).await?;
    let command = RequestAgentFinish
        .verification_command(
            ledger
                .step(TaskStepId::from_bytes([STEP; 32]))
                .ok_or("step missing")?,
        )
        .ok_or("planned existing check unavailable")?;
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::new();
    let runner = native_runner()?;
    let factory = BuiltinProcessVerificationEvidenceFactory;
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
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let result = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::Run(command),
        Some((&catalog, &confirmation)),
        100,
        10,
        &seed,
        &mut compiler,
        &events,
    )
    .await;
    // FailedExit cannot produce Builtin TestEvidence. Retry reaches this explicit
    // fail-closed context boundary; no successful context or runtime proof is supplied.
    let error = result.err().ok_or("wrong behavior unexpectedly verified")?;
    assert!(
        matches!(
            error.downcast_ref::<MutationControllerFailure>(),
            Some(MutationControllerFailure::Context(
                ContextCompileFailure::InvalidPack
            ))
        ),
        "unexpected closed failure: {error:?}"
    );
    assert_eq!(context.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.state(), AgentControllerState::Execute);
    let step = ledger
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    assert!(
        step.attempts()
            .last()
            .ok_or("attempt missing")?
            .verification()
            .is_none()
    );
    {
        let captured = events.values.lock().map_err(|_| "event capture poisoned")?;
        assert!(captured.iter().any(|event| matches!(event.kind(),
            ProcessEventKind::Terminated(ProcessTermination::Exited(exit)) if !exit.success())));
    }
    let overview = inspection
        .overview(&fixture.project, goal.task_id())?
        .ok_or("actual failed inspection missing")?;
    assert_eq!(overview.processes().len(), 1);
    assert!(
        matches!(overview.processes()[0].termination(), ProcessTermination::Exited(exit) if !exit.success())
    );
    // A completion request cannot turn an unverified open step into Done. Retain
    // that real finite-state transition so the same denial can be checked after reopen.
    let expected = run.last_event_sequence();
    let snapshot = run.current_snapshot_id();
    let advance = RequestAgentFinish.execute(
        &mut run,
        AgentFinishAction,
        RunEventId::from_bytes([190; 32]),
        snapshot,
        at(20)?,
        &ActiveControl,
    )?;
    AppendRunEvent::new(store.as_ref())
        .execute(&fixture.project, expected, &run, advance.event())
        .await?;
    Ok(goal)
}

#[allow(clippy::too_many_arguments)]
async fn execute_approved(
    controller: ExecuteMutatingAgentAction<'_>,
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    ledger: &mut a3_domain::TaskLedger,
    version: &mut TaskLedgerStoreVersion,
    published: &PublishedIndex,
    action: AgentAction,
    command: Option<(
        &ProjectCommandCatalog,
        &a3_application::StoredProjectCommandAllowlist,
    )>,
    ids: u8,
    time: u64,
    seed: &MutationContextSeed,
    compiler: &mut BuiltinIncrementalIndexCompiler,
    events: &BoundedEvents,
) -> TestResult<MutationControllerOutcome> {
    execute_approved_observing(
        controller,
        fixture,
        store,
        run,
        ledger,
        version,
        published,
        action,
        command,
        ids,
        time,
        seed,
        compiler,
        events,
        async |_| Ok(()),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_approved_observing(
    controller: ExecuteMutatingAgentAction<'_>,
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    ledger: &mut a3_domain::TaskLedger,
    version: &mut TaskLedgerStoreVersion,
    published: &PublishedIndex,
    action: AgentAction,
    command: Option<(
        &ProjectCommandCatalog,
        &a3_application::StoredProjectCommandAllowlist,
    )>,
    ids: u8,
    time: u64,
    seed: &MutationContextSeed,
    compiler: &mut BuiltinIncrementalIndexCompiler,
    events: &BoundedEvents,
    before_grant: impl std::ops::AsyncFnOnce(ApprovalRequestId) -> TestResult<()>,
) -> TestResult<MutationControllerOutcome> {
    execute_approved_until(
        controller,
        fixture,
        store,
        run,
        ledger,
        version,
        published,
        action,
        command,
        ids,
        time,
        at(1_000)?,
        seed,
        compiler,
        events,
        before_grant,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_approved_until(
    controller: ExecuteMutatingAgentAction<'_>,
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    ledger: &mut a3_domain::TaskLedger,
    version: &mut TaskLedgerStoreVersion,
    published: &PublishedIndex,
    action: AgentAction,
    command: Option<(
        &ProjectCommandCatalog,
        &a3_application::StoredProjectCommandAllowlist,
    )>,
    ids: u8,
    time: u64,
    approval_expires_at: AgentRunTimestamp,
    seed: &MutationContextSeed,
    compiler: &mut BuiltinIncrementalIndexCompiler,
    events: &BoundedEvents,
    before_grant: impl std::ops::AsyncFnOnce(ApprovalRequestId) -> TestResult<()>,
) -> TestResult<MutationControllerOutcome> {
    let selection = || {
        command.map(|(catalog, confirmation)| MutationCommandSelection::new(catalog, confirmation))
    };
    let outcome = controller
        .execute(
            &fixture.project,
            run,
            ledger,
            version,
            published,
            action.clone(),
            selection(),
            &WorkspacePolicy::unrestricted(),
            None,
            execution_ids(ids),
            at(time)?,
            approval_expires_at,
            seed,
            compiler,
            events,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::AwaitingApproval(request) = outcome else {
        return Err(format!("exact Ask-permissions request missing: {outcome:?}").into());
    };
    assert_eq!(request, ApprovalRequestId::from_bytes([ids + 1; 32]));
    before_grant(request).await?;
    let snapshot = run.current_snapshot_id();
    let mut grant = GrantPolicyApproval::new(store)
        .execute(
            &fixture.project,
            run,
            request,
            ApprovalId::from_bytes([ids + 11; 32]),
            RunEventId::from_bytes([ids + 11; 32]),
            snapshot,
            at(time + 1)?,
        )
        .await?;
    Ok(controller
        .execute(
            &fixture.project,
            run,
            ledger,
            version,
            published,
            action,
            selection(),
            &WorkspacePolicy::unrestricted(),
            Some(&mut grant),
            execution_ids(ids + 12),
            at(time + 2)?,
            approval_expires_at,
            seed,
            compiler,
            events,
            &ActiveControl,
        )
        .await?)
}

pub(super) fn native_runner() -> TestResult<WorkspaceProcessRunner> {
    let keys = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"]
        .into_iter()
        .map(|name| ProcessEnvironmentVariable::try_from_string(name.to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorkspaceProcessRunner::new(
        ProcessHostEnvironment::capture(keys)?,
    ))
}

fn wrong_fixture() -> TestResult<Fixture> {
    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    fs::create_dir(worktree.join("tests"))?;
    fs::write(worktree.join("average.py"), WRONG_SUBJECT)?;
    fs::write(worktree.join("tests/__init__.py"), b"")?;
    fs::write(worktree.join("tests/test_average.py"), TESTS)?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    Ok(fixture)
}

fn repair_goal(command: DiscoveredCommandId) -> TestResult<GoalContract> {
    goal_for_case(
        command,
        REPAIR_GOAL,
        "Fix average using the unchanged existing tests",
    )
}
