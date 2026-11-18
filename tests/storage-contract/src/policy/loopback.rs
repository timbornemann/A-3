//! Every closed HTTP method must fit the real policy store without weakening its risk.

use super::*;
use a3_domain::{
    ActionClass, AgentPermissionMode, AgentPermissionRevision, AgentPermissionSettings,
    LoopbackMethod, PolicyResourceId, RiskLevel, RunEventSequence,
};

pub(super) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("policy-loopback");
    let common = workspace.create_directory("policy-loopback-common")?;
    let root = workspace.create_directory("policy-loopback-root")?;
    let worktree = WorktreeId::from_bytes(id(1, 0));
    let project = project(
        RepositoryId::from_bytes(id(2, 0)),
        worktree,
        &common,
        &root,
        unborn_head()?,
    )?;
    let snapshot_id = SnapshotId::from_bytes(id(3, 0));
    let current = snapshot(*snapshot_id.as_bytes(), worktree, None, 1, Vec::new())?;
    let goal = GoalContract::initial(
        TaskId::from_bytes(id(4, 0)),
        goal_draft()?,
        GoalContractTimestamp::from_unix_millis(6_000)?,
    );
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![task_step()?],
        TaskLedgerTimestamp::from_unix_millis(6_001)?,
    )?;
    let (mut run, start) = AgentRun::start(
        AgentRunId::from_bytes(id(5, 0)),
        goal.reference(),
        ledger.revision(),
        ModelProfileReference::new(
            ModelProfileId::from_bytes(id(6, 0)),
            ModelProfileVersion::V1,
        ),
        snapshot_id,
        RunEventId::from_bytes(id(7, 0)),
        AgentRunTimestamp::from_unix_millis(6_002)?,
    )?;
    let store = factory.open(&app_data_root).await?;
    store.append_snapshot(&project, &current).await?;
    CreateGoalContract::new(&store)
        .execute(&project, &goal)
        .await?;
    CreateTaskLedger::new(&store)
        .execute(&project, &ledger)
        .await?;
    CreateAgentRun::new(&store)
        .execute(&project, &run, &start)
        .await?;
    crate::release_contract_store(store);

    let mut millis = 6_003;
    for (mode_index, mode) in [
        AgentPermissionMode::AskPermissions,
        AgentPermissionMode::FullMachine,
    ]
    .into_iter()
    .enumerate()
    {
        let permissions = AgentPermissionSettings::new(
            mode,
            AgentPermissionRevision::new(u64::try_from(mode_index)? + 1)?,
        );
        for (method_index, method) in [
            LoopbackMethod::Get,
            LoopbackMethod::Post,
            LoopbackMethod::Put,
            LoopbackMethod::Delete,
            LoopbackMethod::Patch,
        ]
        .into_iter()
        .enumerate()
        {
            let index = u8::try_from(mode_index * 5 + method_index)?;
            let action = PolicyAction::LoopbackHttpProbe {
                worktree_id: worktree,
                step_id: task_step()?.id(),
                target_id: PolicyResourceId::from_bytes(id(8, index)),
                method,
            };
            let (class, risk) = if method == LoopbackMethod::Delete {
                (ActionClass::Destructive, RiskLevel::Critical)
            } else {
                (ActionClass::Network, RiskLevel::High)
            };
            let engine = EvaluateActionPolicy::new().with_permissions(permissions);
            let store = factory.open(&app_data_root).await?;
            let expected = run.last_event_sequence();
            let pending = engine.execute(
                &mut run,
                &action,
                &WorkspacePolicy::unrestricted(),
                None,
                context(20, index, snapshot_id, tick(&mut millis)?)?,
            )?;
            assert_eq!(
                pending.decision().outcome(),
                PolicyDecisionOutcome::ApprovalRequired
            );
            assert_eq!(
                pending.decision().reason(),
                PolicyDecisionReason::SystemApprovalRequired
            );
            PersistPolicyEvaluation::new(&store)
                .execute(&project, expected, &run, &pending)
                .await?;
            assert_eq!(action.class(), class);
            assert_eq!(action.risk(), risk);
            assert_eq!(pending.decision().action_class(), class);
            assert_eq!(pending.decision().risk_level(), risk);
            assert_eq!(pending.decision().permission_settings(), Some(permissions));
            let request = pending
                .approval_request()
                .ok_or_else(|| {
                    std::io::Error::other("loopback method did not request exact approval")
                })?
                .clone();
            crate::release_contract_store(store);

            let reopened = factory.open(&app_data_root).await?;
            assert_eq!(
                reopened
                    .load_approval_request(&project, request.id())
                    .await?,
                Some(request.clone())
            );
            assert_eq!(
                reopened
                    .load_policy_decision(&project, pending.decision().id())
                    .await?,
                Some(pending.decision().clone())
            );
            assert_eq!(
                reopened.load_agent_run(&project, run.id()).await?,
                Some(run.clone())
            );
            let granted = GrantPolicyApproval::new(&reopened)
                .execute(
                    &project,
                    &mut run,
                    request.id(),
                    ApprovalId::from_bytes(id(30, index)),
                    RunEventId::from_bytes(id(31, index)),
                    snapshot_id,
                    tick(&mut millis)?,
                )
                .await?;
            assert_eq!(granted.state(), ApprovalGrantState::Active);
            crate::release_contract_store(reopened);

            let reopened = factory.open(&app_data_root).await?;
            let mut grant = reopened
                .load_approval_for_request(&project, request.id())
                .await?
                .ok_or_else(|| std::io::Error::other("cold loopback grant missing"))?;
            assert_eq!(grant, granted);
            let mut restrictions = vec![ActionClass::Network];
            if method != LoopbackMethod::Get {
                restrictions.push(ActionClass::Write);
            }
            if method == LoopbackMethod::Delete {
                restrictions.push(ActionClass::Destructive);
            }
            for (restriction_index, restriction) in restrictions.into_iter().enumerate() {
                let deny = WorkspacePolicy::new(vec![WorkspacePolicyRule::new(
                    restriction,
                    WorkspacePolicyRestriction::Deny,
                )])?;
                let expected = run.last_event_sequence();
                let denied = engine.execute(
                    &mut run,
                    &action,
                    &deny,
                    Some(&mut grant),
                    context(
                        50,
                        index * 3 + u8::try_from(restriction_index)?,
                        snapshot_id,
                        tick(&mut millis)?,
                    )?,
                )?;
                assert_eq!(denied.decision().outcome(), PolicyDecisionOutcome::Denied);
                assert_eq!(
                    denied.decision().reason(),
                    PolicyDecisionReason::WorkspaceDenied
                );
                assert!(denied.approval_request().is_none());
                assert_eq!(grant.state(), ApprovalGrantState::Active);
                PersistPolicyEvaluation::new(&reopened)
                    .execute(&project, expected, &run, &denied)
                    .await?;
                let reader = factory.open(&app_data_root).await?;
                assert_eq!(
                    reader
                        .load_policy_decision(&project, denied.decision().id())
                        .await?,
                    Some(denied.decision().clone())
                );
                assert_eq!(
                    reader
                        .load_approval_for_request(&project, request.id())
                        .await?,
                    Some(grant.clone())
                );
                crate::release_contract_store(reader);
            }

            let before = run.clone();
            let active = grant.clone();
            let expected = run.last_event_sequence();
            let approved = engine.execute(
                &mut run,
                &action,
                &WorkspacePolicy::unrestricted(),
                Some(&mut grant),
                context(40, index, snapshot_id, tick(&mut millis)?)?,
            )?;
            assert_eq!(
                approved.decision().outcome(),
                PolicyDecisionOutcome::Allowed
            );
            assert_eq!(
                approved.decision().reason(),
                PolicyDecisionReason::ApprovalGranted
            );
            assert!(approved.approval_request().is_none());
            if method == LoopbackMethod::Delete {
                assert_eq!(
                    PersistPolicyEvaluation::new(&reopened)
                        .execute(&project, RunEventSequence::FIRST, &run, &approved)
                        .await,
                    Err(PolicyStoreFailure::RunSequenceConflict)
                );
                assert_eq!(
                    reopened.load_agent_run(&project, run.id()).await?,
                    Some(before)
                );
                assert!(
                    reopened
                        .load_policy_decision(&project, approved.decision().id())
                        .await?
                        .is_none()
                );
                assert_eq!(
                    reopened
                        .load_approval_for_request(&project, request.id())
                        .await?,
                    Some(active)
                );
            }
            PersistPolicyEvaluation::new(&reopened)
                .execute(&project, expected, &run, &approved)
                .await?;
            crate::release_contract_store(reopened);

            let reopened = factory.open(&app_data_root).await?;
            assert_eq!(
                reopened
                    .load_policy_decision(&project, approved.decision().id())
                    .await?,
                Some(approved.decision().clone())
            );
            assert_eq!(
                reopened.load_agent_run(&project, run.id()).await?,
                Some(run.clone())
            );
            let consumed = reopened
                .load_approval_for_request(&project, request.id())
                .await?
                .ok_or_else(|| std::io::Error::other("consumed loopback grant missing"))?;
            assert_eq!(consumed, grant);
            assert!(
                matches!(consumed.state(), ApprovalGrantState::Consumed { decision_id, .. }
                if decision_id == approved.decision().id())
            );
            crate::release_contract_store(reopened);
        }
    }
    Ok(())
}

fn context(
    role: u8,
    index: u8,
    snapshot: SnapshotId,
    at: AgentRunTimestamp,
) -> ContractResult<PolicyEvaluationContext> {
    Ok(PolicyEvaluationContext::new(
        PolicyDecisionId::from_bytes(id(role, index)),
        ApprovalRequestId::from_bytes(id(role + 1, index)),
        RunEventId::from_bytes(id(role + 2, index)),
        snapshot,
        PolicyEvaluationTiming::new(at, at)?,
        AgentRunTimestamp::from_unix_millis(20_000)?,
    ))
}

fn tick(millis: &mut u64) -> ContractResult<AgentRunTimestamp> {
    let at = AgentRunTimestamp::from_unix_millis(*millis)?;
    *millis += 1;
    Ok(at)
}

fn id(role: u8, index: u8) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[0] = role;
    bytes[1] = index;
    bytes[2] = 0xA3;
    bytes
}
