use crate::fixture::{ContractWorkspace, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateGoalContract, GoalContractStore, GoalContractStoreFailure, ReviseGoalContract,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, GoalConstraint, GoalContract, GoalContractDraft,
    GoalContractRevision, GoalContractTimestamp, GoalObjective, GoalRevisionReason, NonGoal,
    RepositoryId, SuccessVerification, TaskId, UserDecision, WorktreeId,
};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("goal-contracts");
    let common = workspace.create_directory("goal-contract-common")?;
    let first_root = workspace.create_directory("goal-contract-first")?;
    let second_root = workspace.create_directory("goal-contract-second")?;
    let repository_id = RepositoryId::from_bytes([210; 32]);
    let first = project(
        repository_id,
        WorktreeId::from_bytes([211; 32]),
        &common,
        &first_root,
        unborn_head()?,
    )?;
    let second = project(
        repository_id,
        WorktreeId::from_bytes([212; 32]),
        &common,
        &second_root,
        unborn_head()?,
    )?;
    let task_id = TaskId::from_bytes([213; 32]);
    let initial = GoalContract::initial(
        task_id,
        initial_draft()?,
        GoalContractTimestamp::from_unix_millis(1_000)?,
    );

    let store = factory.open(&app_data_root).await?;
    assert_eq!(
        CreateGoalContract::new(&store)
            .execute(&first, &initial)
            .await?,
        initial.reference()
    );
    assert_eq!(
        store.load_current_goal_contract(&first, task_id).await?,
        Some(initial.clone())
    );
    assert_eq!(
        store
            .load_goal_contract_revision(&first, task_id, GoalContractRevision::INITIAL)
            .await?,
        Some(initial.clone())
    );
    assert_eq!(
        store.create_goal_contract(&first, &initial).await,
        Err(GoalContractStoreFailure::TaskAlreadyExists)
    );
    assert_eq!(
        store.load_current_goal_contract(&second, task_id).await?,
        None,
        "the same repository's linked worktree must not see another worktree's task"
    );

    let stale_second = initial.revise(
        alternate_draft("stale competing goal")?,
        GoalRevisionReason::try_from_string("stale writer".to_owned())?,
        GoalContractTimestamp::from_unix_millis(1_001)?,
    )?;
    let revised = ReviseGoalContract::new(&store)
        .execute(
            &first,
            task_id,
            alternate_draft("accepted revised goal")?,
            GoalRevisionReason::try_from_string("user clarified the outcome".to_owned())?,
            GoalContractTimestamp::from_unix_millis(1_001)?,
        )
        .await?;
    assert_eq!(revised.revision().get(), 2);
    assert_eq!(
        store
            .append_goal_contract_revision(&first, &stale_second)
            .await,
        Err(GoalContractStoreFailure::RevisionConflict)
    );
    assert_eq!(
        store
            .load_goal_contract_revision(&first, task_id, GoalContractRevision::INITIAL)
            .await?,
        Some(initial.clone()),
        "a revision append must not overwrite the audit history"
    );
    assert_eq!(
        store.load_current_goal_contract(&first, task_id).await?,
        Some(revised.clone())
    );
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened
            .load_goal_contract_revision(&first, task_id, GoalContractRevision::INITIAL)
            .await?,
        Some(initial)
    );
    assert_eq!(
        reopened
            .load_goal_contract_revision(&first, task_id, revised.revision())
            .await?,
        Some(revised.clone())
    );
    assert_eq!(
        reopened.load_current_goal_contract(&first, task_id).await?,
        Some(revised)
    );
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

fn initial_draft() -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("implement the durable goal contract".to_owned())?,
        vec![
            criterion(214, "the goal survives reopen")?,
            criterion_with_requirement(
                215,
                "old revisions remain auditable",
                AcceptanceCriterionRequirement::Should,
            )?,
        ],
        vec![GoalConstraint::try_from_string(
            "remain local-only".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not start the controller".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "use append-only revisions".to_owned(),
        )?],
        SuccessVerification::try_from_string("reopen and compare both revisions".to_owned())?,
    )?)
}

fn alternate_draft(objective: &str) -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(objective.to_owned())?,
        vec![
            criterion(214, "the goal survives reopen")?,
            criterion_with_requirement(
                216,
                "conflicting writers are rejected",
                AcceptanceCriterionRequirement::Should,
            )?,
        ],
        vec![GoalConstraint::try_from_string(
            "remain local-only".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not start the controller".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "use append-only revisions".to_owned(),
        )?],
        SuccessVerification::try_from_string("reopen and compare both revisions".to_owned())?,
    )?)
}

fn criterion(id: u8, statement: &str) -> ContractResult<AcceptanceCriterion> {
    criterion_with_requirement(id, statement, AcceptanceCriterionRequirement::Must)
}

fn criterion_with_requirement(
    id: u8,
    statement: &str,
    requirement: AcceptanceCriterionRequirement,
) -> ContractResult<AcceptanceCriterion> {
    Ok(AcceptanceCriterion::with_requirement(
        AcceptanceCriterionId::from_bytes([id; 32]),
        AcceptanceCriterionStatement::try_from_string(statement.to_owned())?,
        requirement,
    ))
}
