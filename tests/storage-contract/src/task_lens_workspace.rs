use crate::fixture::{ContractWorkspace, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateGoalContract, CreateTaskLedger, ReviseGoalContract, TaskLensWorkspaceControl,
    TaskLensWorkspaceFailure, TaskLensWorkspaceStore, TaskLensWorkspaceTaskLimit,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, ExpectedTaskEvidence,
    GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, GoalRevisionReason,
    RepositoryId, SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition,
    TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId, WorktreeId,
};

#[derive(Debug)]
struct Control(bool);

impl TaskLensWorkspaceControl for Control {
    fn is_cancelled(&self) -> bool {
        self.0
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("task-lens-workspace");
    let common = workspace.create_directory("task-lens-workspace-common")?;
    let first_root = workspace.create_directory("task-lens-workspace-first")?;
    let second_root = workspace.create_directory("task-lens-workspace-second")?;
    let repository_id = RepositoryId::from_bytes([40; 32]);
    let first = project(
        repository_id,
        WorktreeId::from_bytes([41; 32]),
        &common,
        &first_root,
        unborn_head()?,
    )?;
    let second = project(
        repository_id,
        WorktreeId::from_bytes([42; 32]),
        &common,
        &second_root,
        unborn_head()?,
    )?;
    let lower = goal(43, "lower stable task")?;
    let higher = goal(44, "higher stable task")?;

    let store = factory.open(&app_data_root).await?;
    let empty = store
        .list_current_goal_contracts(&first, TaskLensWorkspaceTaskLimit::DEFAULT, &Control(false))
        .await?;
    assert!(empty.goals().is_empty());
    assert!(!empty.truncated());
    assert_eq!(
        store
            .list_current_goal_contracts(
                &first,
                TaskLensWorkspaceTaskLimit::DEFAULT,
                &Control(true),
            )
            .await,
        Err(TaskLensWorkspaceFailure::Cancelled)
    );

    CreateGoalContract::new(&store)
        .execute(&first, &higher)
        .await?;
    CreateGoalContract::new(&store)
        .execute(&first, &lower)
        .await?;
    let page = store
        .list_current_goal_contracts(&first, TaskLensWorkspaceTaskLimit::DEFAULT, &Control(false))
        .await?;
    assert_eq!(page.goals(), &[lower.clone(), higher.clone()]);
    assert!(!page.truncated());
    let truncated = store
        .list_current_goal_contracts(&first, TaskLensWorkspaceTaskLimit::new(1)?, &Control(false))
        .await?;
    assert_eq!(truncated.goals(), std::slice::from_ref(&lower));
    assert!(truncated.truncated());

    let goal_only = store
        .load_current_task(&first, lower.task_id(), &Control(false))
        .await?
        .ok_or_else(|| std::io::Error::other("current task was missing"))?;
    assert_eq!(goal_only.goal_contract(), &lower);
    assert!(goal_only.task_ledger().is_none());
    assert!(
        store
            .load_current_task(&second, lower.task_id(), &Control(false))
            .await?
            .is_none(),
        "a linked worktree must not see another worktree's task anchor"
    );

    let ledger = TaskLedger::new(
        lower.reference(),
        vec![step(45, 46)?],
        TaskLedgerTimestamp::from_unix_millis(2_000)?,
    )?;
    let stored = CreateTaskLedger::new(&store)
        .execute(&first, &ledger)
        .await?;
    let anchored = store
        .load_current_task(&first, lower.task_id(), &Control(false))
        .await?
        .ok_or_else(|| std::io::Error::other("anchored task was missing"))?;
    assert_eq!(anchored.goal_contract(), &lower);
    assert_eq!(anchored.task_ledger(), Some(&stored));

    let revised = ReviseGoalContract::new(&store)
        .execute(
            &first,
            lower.task_id(),
            draft("revised stable task")?,
            GoalRevisionReason::try_from_string("the user changed the goal".to_owned())?,
            GoalContractTimestamp::from_unix_millis(3_000)?,
        )
        .await?;
    let mismatched = store
        .load_current_task(&first, lower.task_id(), &Control(false))
        .await?
        .ok_or_else(|| std::io::Error::other("revised task was missing"))?;
    assert_eq!(mismatched.goal_contract(), &revised);
    assert_eq!(
        mismatched
            .task_ledger()
            .map(|value| value.ledger().goal_contract()),
        Some(lower.reference()),
        "the adapter must expose, not hide, a current-goal/current-ledger mismatch"
    );
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data_root).await?;
    let durable = reopened
        .load_current_task(&first, lower.task_id(), &Control(false))
        .await?
        .ok_or_else(|| std::io::Error::other("reopened task was missing"))?;
    assert_eq!(durable.goal_contract(), &revised);
    assert_eq!(
        durable
            .task_ledger()
            .map(|value| value.ledger().goal_contract()),
        Some(lower.reference())
    );
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

fn goal(id: u8, objective: &str) -> ContractResult<GoalContract> {
    Ok(GoalContract::initial(
        TaskId::from_bytes([id; 32]),
        draft(objective)?,
        GoalContractTimestamp::from_unix_millis(1_000 + u64::from(id))?,
    ))
}

fn draft(objective: &str) -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(objective.to_owned())?,
        vec![AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([47; 32]),
            AcceptanceCriterionStatement::try_from_string(
                "the task lens uses durable anchors".to_owned(),
            )?,
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("run the storage contract".to_owned())?,
    )?)
}

fn step(id: u8, spec: u8) -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([id; 32]),
        None,
        TaskStepOutcome::try_from_string("compile a current Task Lens".to_owned())?,
        TaskStepRationale::try_from_string("the selected task needs current context".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "current index evidence".to_owned(),
        )?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([spec; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string("run the Task Lens test".to_owned())?,
        ),
    )?)
}
