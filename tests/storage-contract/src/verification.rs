use crate::fixture::{ContractWorkspace, change, project, run, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    AcceptanceRejection, AcceptanceVerificationRequest, AcceptanceVerifier,
    AcceptanceVerifierOutcome, AcceptanceVerifierTimeout, AdvanceAgentController,
    AgentControllerControl, AgentControllerSignal, AgentReadResult, AgentRecoveryStore,
    ContextToolResultDigest, ContextToolResultPreview, ContextToolResultStatus, CreateAgentRun,
    CreateGoalContract, CreateTaskLedger, DeterministicAcceptanceVerifier, EvaluateActionPolicy,
    KnowledgeIndexStore, PersistPolicyEvaluation, PolicyEvaluationContext, RunJournalStore,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, AgentRun, AgentRunId, AgentRunTimestamp, AgentToolEvidence,
    AgentToolEvidenceSet, ApprovalRequestId, CommandEvidence, CommandEvidenceContext, ContentHash,
    DiagnosticCount, DiagnosticEvidence, DiagnosticPolicy, DiffEvidence, DiffInvariantMode,
    DiffInvariantVerification, DiscoveredCommandId, EvidenceDependency, ExpectedTaskEvidence,
    FileRevision, GoalConstraint, GoalContract, GoalContractDraft, GoalContractTimestamp,
    GoalObjective, MinimumTestCaseCount, ModelProfileId, ModelProfileReference,
    ModelProfileVersion, NonGoal, PatchAction, PatchActionSchemaVersion, PatchChange,
    PatchChangeSet, PatchFileContent, PatchOperation, PatchRationale, PatchUpdate,
    PathPolicyOperation, PathScopeCoverage, PolicyAction, PolicyDecisionId, PolicyDecisionOutcome,
    PolicyEvaluationTiming, PolicyPathScope, PolicyResourceId, ProcessDuration, ProcessExit,
    ProcessOutputCapture, ProcessOutputContent, ProcessOutputDigest, ProcessOutputRedaction,
    ProcessRunResult, ProcessStream, ProcessTermination, RepositoryId, RepositoryPath, RunEventId,
    RunMemoryCheckpoint, SnapshotChangeKind, StepVerification, StepVerificationId,
    StepVerificationOutcome, SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, TaskStepResultSummary,
    TestCaseEvidence, TestCaseName, TestCaseOutcome, TestCaseSelector, TestEvidence, ToolRunId,
    UserConfirmationEvidence, UserDecision, VerificationDependencies, VerificationEvidence,
    VerificationRequirement, VerificationRunId, VerificationScope, VerificationSpec,
    VerificationSpecId, WorkspacePolicy, WorktreeId,
};
use std::time::Duration;

#[derive(Debug)]
struct ActiveControl;

impl AgentControllerControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct CancelledControl;

impl AgentControllerControl for CancelledControl {
    fn is_cancelled(&self) -> bool {
        true
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("verification-evidence");
    let common = workspace.create_directory("verification-evidence-common")?;
    let root = workspace.create_directory("verification-evidence-worktree")?;
    let project = project(
        RepositoryId::from_bytes([10; 32]),
        WorktreeId::from_bytes([11; 32]),
        &common,
        &root,
        unborn_head()?,
    )?;
    let snapshot_id = a3_domain::SnapshotId::from_bytes([12; 32]);
    let current_snapshot = snapshot(
        *snapshot_id.as_bytes(),
        project.worktree().id(),
        None,
        1,
        vec![change(b"src/lib.rs", [13; 32], SnapshotChangeKind::Upsert)?],
    )?;
    let task_id = TaskId::from_bytes([14; 32]);
    let must_id = AcceptanceCriterionId::from_bytes([15; 32]);
    let should_id = AcceptanceCriterionId::from_bytes([16; 32]);
    let spec_id = VerificationSpecId::from_bytes([17; 32]);
    let scope_id = PolicyResourceId::from_bytes([18; 32]);
    let run_id = AgentRunId::from_bytes([19; 32]);
    let verification_run_id = VerificationRunId::from_bytes([20; 32]);
    let evidence = VerificationEvidence::UserConfirmation(UserConfirmationEvidence::new(
        verification_run_id,
        spec_id,
        run_id,
        snapshot_id,
        scope_id,
        TaskLedgerTimestamp::from_unix_millis(1_005)?,
    ));
    let goal = GoalContract::initial(
        task_id,
        goal_draft(must_id, should_id)?,
        GoalContractTimestamp::from_unix_millis(1_000)?,
    );
    let step_id = TaskStepId::from_bytes([21; 32]);
    let step = TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string("receive the exact user confirmation".to_owned())?,
        TaskStepRationale::try_from_string("the mandatory criterion requires it".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "snapshot-bound confirmation evidence".to_owned(),
        )?],
        VerificationSpec::user_confirm(
            spec_id,
            VerificationRequirement::try_from_string(
                "the displayed scope is explicitly confirmed".to_owned(),
            )?,
            scope_id,
        ),
    )?
    .with_acceptance_criteria(vec![must_id])?;
    let source_path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
    let command_spec_id = VerificationSpecId::from_bytes([40; 32]);
    let test_spec_id = VerificationSpecId::from_bytes([41; 32]);
    let diagnostic_spec_id = VerificationSpecId::from_bytes([42; 32]);
    let diff_spec_id = VerificationSpecId::from_bytes([43; 32]);
    let command_id = DiscoveredCommandId::from_bytes([44; 32]);
    let test_command_id = DiscoveredCommandId::from_bytes([45; 32]);
    let diagnostic_command_id = DiscoveredCommandId::from_bytes([46; 32]);
    let mut steps = vec![step];
    steps.push(should_step(
        TaskStepId::from_bytes([47; 32]),
        should_id,
        VerificationSpec::command(
            command_spec_id,
            verification_requirement()?,
            command_id,
            VerificationScope::Targeted,
        ),
    )?);
    steps.push(should_step(
        TaskStepId::from_bytes([48; 32]),
        should_id,
        VerificationSpec::test(
            test_spec_id,
            verification_requirement()?,
            test_command_id,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Package,
        ),
    )?);
    steps.push(should_step(
        TaskStepId::from_bytes([49; 32]),
        should_id,
        VerificationSpec::diagnostic(
            diagnostic_spec_id,
            verification_requirement()?,
            diagnostic_command_id,
            DiagnosticPolicy::NoErrors,
            VerificationScope::Workspace,
        ),
    )?);
    let diff_step_id = TaskStepId::from_bytes([50; 32]);
    steps.push(should_step(
        diff_step_id,
        should_id,
        VerificationSpec::diff_invariant(
            diff_spec_id,
            verification_requirement()?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![source_path.clone()],
            )?,
        ),
    )?);
    let mut ledger = TaskLedger::new(
        goal.reference(),
        steps,
        TaskLedgerTimestamp::from_unix_millis(1_001)?,
    )?;
    ledger.start_step(
        step_id,
        run_id,
        TaskLedgerTimestamp::from_unix_millis(1_003)?,
    )?;
    ledger.begin_step_verification(
        step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            "confirmation captured".to_owned(),
        )?),
        vec![evidence.id()],
        TaskLedgerTimestamp::from_unix_millis(1_004)?,
    )?;
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([22; 32]),
            spec_id,
            run_id,
            StepVerificationOutcome::Passed,
            vec![evidence.id()],
            TaskLedgerTimestamp::from_unix_millis(1_006)?,
        )?,
    )?;
    let (mut agent_run, start_event) = AgentRun::start(
        run_id,
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([23; 32]),
            ModelProfileVersion::V1,
        ),
        snapshot_id,
        RunEventId::from_bytes([24; 32]),
        AgentRunTimestamp::from_unix_millis(1_002)?,
    )?;

    let store = factory.open(&app_data_root).await?;
    store.append_snapshot(&project, &current_snapshot).await?;
    CreateGoalContract::new(&store)
        .execute(&project, &goal)
        .await?;
    CreateTaskLedger::new(&store)
        .execute(&project, &ledger)
        .await?;
    CreateAgentRun::new(&store)
        .execute(&project, &agent_run, &start_event)
        .await?;
    let policy_sequence = agent_run.last_event_sequence();
    let policy_time = AgentRunTimestamp::from_unix_millis(1_007)?;
    let policy_evaluation = EvaluateActionPolicy::new().execute(
        &mut agent_run,
        &PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: project.worktree().id(),
                path: source_path.clone(),
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Read,
        },
        &WorkspacePolicy::unrestricted(),
        None,
        PolicyEvaluationContext::new(
            PolicyDecisionId::from_bytes([51; 32]),
            ApprovalRequestId::from_bytes([52; 32]),
            RunEventId::from_bytes([53; 32]),
            snapshot_id,
            PolicyEvaluationTiming::new(policy_time, policy_time)?,
            AgentRunTimestamp::from_unix_millis(2_000)?,
        ),
    )?;
    assert_eq!(
        policy_evaluation.decision().outcome(),
        PolicyDecisionOutcome::Allowed
    );
    let policy_decision_id = policy_evaluation.decision().id();
    PersistPolicyEvaluation::new(&store)
        .execute(&project, policy_sequence, &agent_run, &policy_evaluation)
        .await?;
    let command_tool_run_id = ToolRunId::from_bytes([54; 32]);
    let test_tool_run_id = ToolRunId::from_bytes([55; 32]);
    let diagnostic_tool_run_id = ToolRunId::from_bytes([56; 32]);
    for (offset, tool_run_id) in [
        command_tool_run_id,
        test_tool_run_id,
        diagnostic_tool_run_id,
    ]
    .into_iter()
    .enumerate()
    {
        complete_tool_run(
            &store,
            &project,
            &mut agent_run,
            tool_run_id,
            snapshot_id,
            FileRevision::new(source_path.clone(), ContentHash::from_bytes([13; 32])),
            1_008 + u64::try_from(offset)?.saturating_mul(2),
            70_u8.saturating_add(u8::try_from(offset)?),
        )
        .await?;
    }
    publish(
        &store,
        &project,
        current_snapshot.id(),
        [25; 32],
        [13; 32],
        26,
    )
    .await?;

    assert_eq!(
        store
            .append_verification_evidence(&project, &evidence, Duration::ZERO, &ActiveControl,)
            .await,
        Err(VerificationEvidenceStoreFailure::TimedOut)
    );
    assert_eq!(
        store
            .append_verification_evidence(
                &project,
                &evidence,
                Duration::from_secs(5),
                &CancelledControl,
            )
            .await,
        Err(VerificationEvidenceStoreFailure::Cancelled)
    );
    store
        .append_verification_evidence(&project, &evidence, Duration::from_secs(5), &ActiveControl)
        .await?;
    store
        .append_verification_evidence(&project, &evidence, Duration::from_secs(5), &ActiveControl)
        .await?;
    move_to_verify(&mut agent_run, snapshot_id)?;
    let published = store
        .latest_published_index(&project, &ContractIndexControl)
        .await?
        .ok_or_else(|| std::io::Error::other("verification index publication is missing"))?;
    let run_memory =
        RunMemoryCheckpoint::compile(&goal, &ledger, &agent_run, &published, Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        project.clone(),
        &agent_run,
        goal.clone(),
        ledger.clone(),
        run_memory,
    )?;
    let verifier = DeterministicAcceptanceVerifier::new(&store);
    let accepted = verifier
        .verify(
            &request,
            AcceptanceVerifierTimeout::from_millis(5_000)?,
            &ActiveControl,
        )
        .await?;
    assert!(matches!(accepted, AcceptanceVerifierOutcome::Accepted(_)));
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data_root).await?;
    let state = reopened
        .load_verification_state(
            &project,
            task_id,
            &[evidence.id()],
            snapshot_id,
            Duration::from_secs(5),
            &ActiveControl,
        )
        .await?;
    assert_eq!(state.evidence(), std::slice::from_ref(&evidence));
    assert_eq!(
        state.published_index().publication().graph().snapshot_id(),
        snapshot_id
    );

    let initial_revision =
        FileRevision::new(source_path.clone(), ContentHash::from_bytes([13; 32]));
    let dependencies = VerificationDependencies::new(vec![
        EvidenceDependency::Present(initial_revision.clone()),
        EvidenceDependency::Absent(RepositoryPath::try_from_bytes(b"src/missing.rs".to_vec())?),
    ])?;
    let command_evidence = VerificationEvidence::Command(CommandEvidence::new(
        CommandEvidenceContext::new(
            VerificationRunId::from_bytes([57; 32]),
            command_spec_id,
            run_id,
            command_tool_run_id,
            command_id,
            snapshot_id,
        ),
        dependencies.clone(),
        &successful_process(policy_decision_id, 58)?,
    ));
    let test_evidence = VerificationEvidence::Test(TestEvidence::new(
        CommandEvidence::new(
            CommandEvidenceContext::new(
                VerificationRunId::from_bytes([59; 32]),
                test_spec_id,
                run_id,
                test_tool_run_id,
                test_command_id,
                snapshot_id,
            ),
            dependencies.clone(),
            &successful_process(policy_decision_id, 60)?,
        ),
        vec![
            TestCaseEvidence::new(
                TestCaseName::try_from_string("contract::passes".to_owned())?,
                TestCaseOutcome::Passed,
            ),
            TestCaseEvidence::new(
                TestCaseName::try_from_string("contract::ignored".to_owned())?,
                TestCaseOutcome::Ignored,
            ),
        ],
    )?);
    let diagnostic_evidence = VerificationEvidence::Diagnostic(DiagnosticEvidence::new(
        CommandEvidence::new(
            CommandEvidenceContext::new(
                VerificationRunId::from_bytes([61; 32]),
                diagnostic_spec_id,
                run_id,
                diagnostic_tool_run_id,
                diagnostic_command_id,
                snapshot_id,
            ),
            dependencies,
            &successful_process(policy_decision_id, 62)?,
        ),
        DiagnosticCount::new(0),
        DiagnosticCount::new(2),
    ));
    let mut process_evidence = vec![command_evidence, test_evidence, diagnostic_evidence];
    for (index, artifact) in process_evidence.iter().enumerate() {
        reopened
            .append_verification_evidence(
                &project,
                artifact,
                Duration::from_secs(5),
                &ActiveControl,
            )
            .await
            .map_err(|error| {
                std::io::Error::other(format!(
                    "append process verification artifact {index}: {error}"
                ))
            })?;
    }
    let process_ids = process_evidence
        .iter()
        .map(VerificationEvidence::id)
        .collect::<Vec<_>>();
    let process_state = reopened
        .load_verification_state(
            &project,
            task_id,
            &process_ids,
            snapshot_id,
            Duration::from_secs(5),
            &ActiveControl,
        )
        .await
        .map_err(|error| std::io::Error::other(format!("load process evidence: {error}")))?;
    process_evidence.sort_by_key(VerificationEvidence::id);
    assert_eq!(process_state.evidence(), process_evidence);

    let replacement_id = a3_domain::SnapshotId::from_bytes([27; 32]);
    let replacement_content =
        PatchFileContent::try_from_bytes(b"pub fn replacement() {}\n".to_vec())?;
    let replacement_hash = replacement_content.content_hash();
    let replacement = snapshot(
        *replacement_id.as_bytes(),
        project.worktree().id(),
        Some(snapshot_id),
        2,
        vec![change(
            b"src/lib.rs",
            *replacement_hash.as_bytes(),
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    reopened.append_snapshot(&project, &replacement).await?;
    publish(
        &reopened,
        &project,
        replacement_id,
        [29; 32],
        *replacement_hash.as_bytes(),
        30,
    )
    .await?;
    let replacement_revision = FileRevision::new(source_path.clone(), replacement_hash);
    let patch_action = PatchAction::new(
        PatchActionSchemaVersion::V1,
        run_id,
        project.worktree().id(),
        snapshot_id,
        diff_step_id,
        diff_spec_id,
        PatchRationale::try_from_string("replace the exact contract fixture".to_owned())?,
        vec![PatchOperation::Update(PatchUpdate::new(
            initial_revision.clone(),
            replacement_content,
        )?)],
    )?;
    let change_set = PatchChangeSet::new(
        &patch_action,
        policy_decision_id,
        vec![PatchChange::Updated {
            previous: initial_revision,
            current: replacement_revision.clone(),
        }],
    )?;
    let diff_evidence = VerificationEvidence::Diff(DiffEvidence::from_change_set(
        VerificationRunId::from_bytes([63; 32]),
        replacement_id,
        VerificationDependencies::new(vec![EvidenceDependency::Present(replacement_revision)])?,
        &change_set,
    )?);
    reopened
        .append_verification_evidence(
            &project,
            &diff_evidence,
            Duration::from_secs(5),
            &ActiveControl,
        )
        .await
        .map_err(|error| std::io::Error::other(format!("append diff evidence: {error}")))?;
    let replacement_published = reopened
        .latest_published_index(&project, &ContractIndexControl)
        .await?
        .ok_or_else(|| std::io::Error::other("replacement index publication is missing"))?;
    let index_diff_evidence = VerificationEvidence::Diff(DiffEvidence::from_published_indexes(
        VerificationRunId::from_bytes([64; 32]),
        diff_spec_id,
        run_id,
        &published,
        &replacement_published,
    )?);
    reopened
        .append_verification_evidence(
            &project,
            &index_diff_evidence,
            Duration::from_secs(5),
            &ActiveControl,
        )
        .await
        .map_err(|error| {
            std::io::Error::other(format!("append published-index diff evidence: {error}"))
        })?;
    let stale = DeterministicAcceptanceVerifier::new(&reopened)
        .verify(
            &request,
            AcceptanceVerifierTimeout::from_millis(5_000)?,
            &ActiveControl,
        )
        .await
        .map_err(|error| std::io::Error::other(format!("load all evidence: {error}")))?;
    assert_eq!(
        stale,
        AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::StaleEvidence)
    );
    let mut all_evidence = vec![evidence.clone(), diff_evidence, index_diff_evidence];
    all_evidence.extend(process_evidence);
    let all_ids = all_evidence
        .iter()
        .map(VerificationEvidence::id)
        .collect::<Vec<_>>();
    crate::release_contract_store(reopened);
    let final_reopen = factory.open(&app_data_root).await?;
    let final_state = final_reopen
        .load_verification_state(
            &project,
            task_id,
            &all_ids,
            replacement_id,
            Duration::from_secs(5),
            &ActiveControl,
        )
        .await?;
    all_evidence.sort_by_key(VerificationEvidence::id);
    assert_eq!(final_state.evidence(), all_evidence);
    crate::release_contract_store(final_reopen);
    crate::complete_contract_phase()
}

fn goal_draft(
    must_id: AcceptanceCriterionId,
    should_id: AcceptanceCriterionId,
) -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("prove current mandatory acceptance".to_owned())?,
        vec![
            AcceptanceCriterion::new(
                must_id,
                AcceptanceCriterionStatement::try_from_string(
                    "the exact displayed scope is confirmed".to_owned(),
                )?,
            ),
            AcceptanceCriterion::with_requirement(
                should_id,
                AcceptanceCriterionStatement::try_from_string(
                    "the confirmation remains convenient".to_owned(),
                )?,
                AcceptanceCriterionRequirement::Should,
            ),
        ],
        vec![GoalConstraint::try_from_string(
            "never infer confirmation from process exit".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not execute a mutation".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "the explicit confirmation is authoritative".to_owned(),
        )?],
        SuccessVerification::try_from_string(
            "load current evidence and evaluate exact semantics".to_owned(),
        )?,
    )?)
}

fn should_step(
    step_id: TaskStepId,
    should_id: AcceptanceCriterionId,
    spec: VerificationSpec,
) -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string("retain one typed evidence variant".to_owned())?,
        TaskStepRationale::try_from_string("exercise the complete V1 evidence union".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "an immutable typed verification artifact".to_owned(),
        )?],
        spec,
    )?
    .with_acceptance_criteria(vec![should_id])?)
}

fn verification_requirement() -> ContractResult<VerificationRequirement> {
    Ok(VerificationRequirement::try_from_string(
        "the typed semantic result is retained exactly".to_owned(),
    )?)
}

fn successful_process(
    policy_decision_id: PolicyDecisionId,
    resource_byte: u8,
) -> ContractResult<ProcessRunResult> {
    let stdout_text = "ok\n".to_owned();
    let stdout = ProcessOutputCapture::new(
        ProcessStream::Stdout,
        ProcessOutputContent::text(stdout_text.clone())?,
        u64::try_from(stdout_text.len())?,
        16,
        false,
        ProcessOutputDigest::from_bytes([resource_byte; 32]),
    )?;
    let stderr = ProcessOutputCapture::new(
        ProcessStream::Stderr,
        ProcessOutputContent::redacted(ProcessOutputRedaction::InvalidUtf8),
        4,
        16,
        false,
        ProcessOutputDigest::from_bytes([resource_byte.saturating_add(1); 32]),
    )?;
    Ok(ProcessRunResult::new(
        PolicyResourceId::from_bytes([resource_byte; 32]),
        policy_decision_id,
        ProcessTermination::Exited(ProcessExit::new(Some(0), true)?),
        ProcessDuration::from_millis(25),
        stdout,
        stderr,
    )?)
}

#[allow(clippy::too_many_arguments)]
async fn complete_tool_run<S>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    run: &mut AgentRun,
    tool_run_id: ToolRunId,
    snapshot_id: a3_domain::SnapshotId,
    revision: FileRevision,
    started_at: u64,
    identity_byte: u8,
) -> ContractResult<()>
where
    S: AgentRecoveryStore + RunJournalStore,
{
    store
        .begin_agent_tool_attempt(
            project,
            run.id(),
            snapshot_id,
            tool_run_id,
            AgentRunTimestamp::from_unix_millis(started_at)?,
        )
        .await?;
    let expected_sequence = run.last_event_sequence();
    let preview = "verification process completed".to_owned();
    let read = AgentReadResult::new(
        tool_run_id,
        ContextToolResultStatus::Succeeded,
        ContextToolResultPreview::try_from_string(preview.clone())?,
        ContextToolResultDigest::from_bytes([identity_byte; 32]),
        false,
        snapshot_id,
        AgentToolEvidenceSet::new(snapshot_id, vec![AgentToolEvidence::for_file(revision)])?,
        u64::try_from(preview.len())?,
    )?
    .record(
        run,
        RunEventId::from_bytes([identity_byte.saturating_add(10); 32]),
        AgentRunTimestamp::from_unix_millis(started_at.saturating_add(1))?,
    )?;
    store
        .append_agent_read(project, expected_sequence, run, &read)
        .await?;
    Ok(())
}

async fn publish<S>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    snapshot_id: a3_domain::SnapshotId,
    index_run_id: [u8; 32],
    content_hash: [u8; 32],
    symbol_byte: u8,
) -> ContractResult<()>
where
    S: KnowledgeIndexStore,
{
    let index_run = store
        .start_index_run(project, run(index_run_id, snapshot_id, 1)?)
        .await?;
    let publication =
        super::index::publication(snapshot_id, b"src/lib.rs", content_hash, symbol_byte)?;
    store
        .publish_index(project, index_run.id(), &publication, &ContractIndexControl)
        .await?;
    Ok(())
}

#[derive(Debug)]
struct ContractIndexControl;

impl a3_application::IndexPersistenceControl for ContractIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}

fn move_to_verify(run: &mut AgentRun, snapshot_id: a3_domain::SnapshotId) -> ContractResult<()> {
    let signals = [
        AgentControllerSignal::AnchorsAccepted,
        AgentControllerSignal::LocalizationComplete,
        AgentControllerSignal::PlanReady,
        AgentControllerSignal::TurnNeedsVerification,
    ];
    for (index, signal) in signals.into_iter().enumerate() {
        let sequence = u8::try_from(index)?.saturating_add(31);
        AdvanceAgentController.execute(
            run,
            signal,
            RunEventId::from_bytes([sequence; 32]),
            snapshot_id,
            AgentRunTimestamp::from_unix_millis(1_020 + u64::try_from(index)?)?,
            false,
        )?;
    }
    Ok(())
}
