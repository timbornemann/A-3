use crate::fixture::{ContractWorkspace, project, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateAgentRun, CreateGoalContract, CreateTaskLedger, EvaluateActionPolicy,
    GrantPolicyApproval, KnowledgeIndexStore, PersistPolicyEvaluation, PolicyEvaluationContext,
    PolicyStore, PolicyStoreFailure, RevokePolicyApproval, RunJournalStore,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRun, AgentRunId,
    AgentRunTimestamp, ApprovalGrantState, ApprovalId, ApprovalRequestId, ExpectedTaskEvidence,
    GoalConstraint, GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective,
    ModelProfileId, ModelProfileReference, ModelProfileVersion, NonGoal, PathPolicyOperation,
    PathScopeCoverage, PolicyAction, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyEvaluationTiming, PolicyPathScope, RepositoryId, RepositoryPath, RunEventId, SnapshotId,
    SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, UserDecision, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId, WorkspacePolicy, WorkspacePolicyRestriction,
    WorkspacePolicyRule, WorktreeId,
};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("policy");
    let common = workspace.create_directory("policy-common")?;
    let root = workspace.create_directory("policy-root")?;
    let worktree_id = WorktreeId::from_bytes([181; 32]);
    let project = project(
        RepositoryId::from_bytes([180; 32]),
        worktree_id,
        &common,
        &root,
        unborn_head()?,
    )?;
    let snapshot_id = SnapshotId::from_bytes([182; 32]);
    let current_snapshot = snapshot(*snapshot_id.as_bytes(), worktree_id, None, 1, Vec::new())?;
    let goal = GoalContract::initial(
        TaskId::from_bytes([183; 32]),
        goal_draft()?,
        GoalContractTimestamp::from_unix_millis(4_000)?,
    );
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![task_step()?],
        TaskLedgerTimestamp::from_unix_millis(4_001)?,
    )?;
    let (mut run, start_event) = AgentRun::start(
        AgentRunId::from_bytes([184; 32]),
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes([185; 32]),
            ModelProfileVersion::V1,
        ),
        snapshot_id,
        RunEventId::from_bytes([186; 32]),
        AgentRunTimestamp::from_unix_millis(4_002)?,
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
        .execute(&project, &run, &start_event)
        .await?;

    let first_action = write_action(worktree_id, "src/first.rs")?;
    let expected_sequence = run.last_event_sequence();
    let first_evaluation = EvaluateActionPolicy::new().execute(
        &mut run,
        &first_action,
        &WorkspacePolicy::unrestricted(),
        None,
        evaluation_context(187, 188, 189, snapshot_id, 4_003, 5_000)?,
    )?;
    assert_eq!(
        first_evaluation.decision().outcome(),
        PolicyDecisionOutcome::ApprovalRequired
    );
    PersistPolicyEvaluation::new(&store)
        .execute(&project, expected_sequence, &run, &first_evaluation)
        .await?;
    let first_request = first_evaluation
        .approval_request()
        .ok_or_else(|| std::io::Error::other("write action did not create an approval request"))?
        .clone();

    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened
            .load_approval_request(&project, first_request.id())
            .await?,
        Some(first_request.clone())
    );
    assert_eq!(
        reopened
            .load_policy_decision(&project, first_evaluation.decision().id())
            .await?,
        Some(first_evaluation.decision().clone())
    );

    let mut approval = GrantPolicyApproval::new(&reopened)
        .execute(
            &project,
            &mut run,
            first_request.id(),
            ApprovalId::from_bytes([190; 32]),
            RunEventId::from_bytes([191; 32]),
            snapshot_id,
            AgentRunTimestamp::from_unix_millis(4_004)?,
        )
        .await?;
    assert_eq!(approval.state(), ApprovalGrantState::Active);

    let second_action = write_action(worktree_id, "src/second.rs")?;
    let mismatch_sequence = run.last_event_sequence();
    let mismatch = EvaluateActionPolicy::new().execute(
        &mut run,
        &second_action,
        &WorkspacePolicy::unrestricted(),
        Some(&mut approval),
        evaluation_context(192, 193, 194, snapshot_id, 4_005, 5_000)?,
    )?;
    assert_eq!(
        mismatch.decision().reason(),
        PolicyDecisionReason::ApprovalScopeMismatch
    );
    assert_eq!(approval.state(), ApprovalGrantState::Active);
    PersistPolicyEvaluation::new(&reopened)
        .execute(&project, mismatch_sequence, &run, &mismatch)
        .await?;
    assert_eq!(
        reopened.load_approval(&project, approval.id()).await?,
        Some(approval.clone())
    );

    let consume_sequence = run.last_event_sequence();
    let approved = EvaluateActionPolicy::new().execute(
        &mut run,
        &first_action,
        &WorkspacePolicy::unrestricted(),
        Some(&mut approval),
        evaluation_context(195, 196, 197, snapshot_id, 4_006, 5_000)?,
    )?;
    assert_eq!(
        approved.decision().outcome(),
        PolicyDecisionOutcome::Allowed
    );
    assert_eq!(approved.approval_request(), None);
    PersistPolicyEvaluation::new(&reopened)
        .execute(&project, consume_sequence, &run, &approved)
        .await?;
    assert_eq!(
        reopened.load_approval(&project, approval.id()).await?,
        Some(approval.clone())
    );
    assert!(matches!(
        approval.state(),
        ApprovalGrantState::Consumed { decision_id, .. }
            if decision_id == approved.decision().id()
    ));

    let third_action = write_action(worktree_id, "src/third.rs")?;
    let third_sequence = run.last_event_sequence();
    let third_evaluation = EvaluateActionPolicy::new().execute(
        &mut run,
        &third_action,
        &WorkspacePolicy::unrestricted(),
        None,
        evaluation_context(198, 199, 200, snapshot_id, 4_007, 5_000)?,
    )?;
    PersistPolicyEvaluation::new(&reopened)
        .execute(&project, third_sequence, &run, &third_evaluation)
        .await?;
    let third_request = third_evaluation
        .approval_request()
        .ok_or_else(|| std::io::Error::other("third action did not request approval"))?;
    let third_approval = GrantPolicyApproval::new(&reopened)
        .execute(
            &project,
            &mut run,
            third_request.id(),
            ApprovalId::from_bytes([201; 32]),
            RunEventId::from_bytes([202; 32]),
            snapshot_id,
            AgentRunTimestamp::from_unix_millis(4_008)?,
        )
        .await?;
    let revoked = RevokePolicyApproval::new(&reopened)
        .execute(
            &project,
            &mut run,
            third_approval.id(),
            RunEventId::from_bytes([203; 32]),
            snapshot_id,
            AgentRunTimestamp::from_unix_millis(4_009)?,
        )
        .await?;
    assert!(matches!(
        revoked.state(),
        ApprovalGrantState::Revoked { .. }
    ));
    assert_eq!(
        reopened.load_approval(&project, revoked.id()).await?,
        Some(revoked)
    );

    let denied_policy = WorkspacePolicy::new(vec![WorkspacePolicyRule::new(
        a3_domain::ActionClass::Write,
        WorkspacePolicyRestriction::Deny,
    )])?;
    let denied_sequence = run.last_event_sequence();
    let denied = EvaluateActionPolicy::new().execute(
        &mut run,
        &third_action,
        &denied_policy,
        None,
        evaluation_context(204, 205, 206, snapshot_id, 4_010, 5_000)?,
    )?;
    assert_eq!(denied.decision().outcome(), PolicyDecisionOutcome::Denied);
    PersistPolicyEvaluation::new(&reopened)
        .execute(&project, denied_sequence, &run, &denied)
        .await?;

    let durable_before_conflict = reopened
        .load_agent_run(&project, run.id())
        .await?
        .ok_or_else(|| std::io::Error::other("policy run disappeared"))?;
    let conflicting = EvaluateActionPolicy::new().execute(
        &mut run,
        &write_action(worktree_id, "src/fourth.rs")?,
        &WorkspacePolicy::unrestricted(),
        None,
        evaluation_context(207, 208, 209, snapshot_id, 4_011, 5_000)?,
    )?;
    assert_eq!(
        PersistPolicyEvaluation::new(&reopened)
            .execute(
                &project,
                a3_domain::RunEventSequence::FIRST,
                &run,
                &conflicting,
            )
            .await,
        Err(PolicyStoreFailure::RunSequenceConflict)
    );
    assert_eq!(
        reopened
            .load_policy_decision(&project, conflicting.decision().id())
            .await?,
        None
    );
    assert_eq!(
        reopened
            .load_approval_request(
                &project,
                conflicting
                    .approval_request()
                    .ok_or_else(|| std::io::Error::other("conflicting write lacked a request"))?
                    .id(),
            )
            .await?,
        None
    );
    assert_eq!(
        reopened.load_agent_run(&project, run.id()).await?,
        Some(durable_before_conflict)
    );

    crate::release_contract_store(reopened);
    crate::release_contract_store(store);
    crate::complete_contract_phase()
}

fn evaluation_context(
    decision: u8,
    request: u8,
    event: u8,
    snapshot_id: SnapshotId,
    decided_at: u64,
    expires_at: u64,
) -> ContractResult<PolicyEvaluationContext> {
    let decided_at = AgentRunTimestamp::from_unix_millis(decided_at)?;
    Ok(PolicyEvaluationContext::new(
        PolicyDecisionId::from_bytes([decision; 32]),
        ApprovalRequestId::from_bytes([request; 32]),
        RunEventId::from_bytes([event; 32]),
        snapshot_id,
        PolicyEvaluationTiming::new(decided_at, decided_at)?,
        AgentRunTimestamp::from_unix_millis(expires_at)?,
    ))
}

fn write_action(worktree_id: WorktreeId, path: &str) -> ContractResult<PolicyAction> {
    Ok(PolicyAction::Path {
        scope: PolicyPathScope::Worktree {
            worktree_id,
            path: RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            coverage: PathScopeCoverage::Exact,
        },
        operation: PathPolicyOperation::Write,
    })
}

fn goal_draft() -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("persist central policy decisions".to_owned())?,
        vec![AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([210; 32]),
            AcceptanceCriterionStatement::try_from_string(
                "approvals are exact, one-time, and auditable".to_owned(),
            )?,
        )],
        vec![GoalConstraint::try_from_string(
            "workspace policy cannot loosen system policy".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not execute a privileged action".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "retain content-free policy audit".to_owned(),
        )?],
        SuccessVerification::try_from_string("run the shared policy contract".to_owned())?,
    )?)
}

fn task_step() -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([211; 32]),
        None,
        TaskStepOutcome::try_from_string("persist one policy lifecycle".to_owned())?,
        TaskStepRationale::try_from_string("exercise the policy boundary".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "the shared policy contract passes".to_owned(),
        )?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([212; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string(
                "the shared policy contract passes".to_owned(),
            )?,
        ),
    )?)
}
