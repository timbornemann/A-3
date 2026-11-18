//! Additional new-role contract; the frozen baseline TaskLedger fixtures stay unchanged.
use crate::fixture::{ContractWorkspace, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateGoalContract, CreateTaskLedger, SaveTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
    DiscoveredCommandId, ExpectedTaskEvidence, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, MinimumTestCaseCount, RepositoryId, SuccessVerification,
    TaskId, TaskLedger, TaskLedgerTimestamp, TaskReplanReason, TaskStepBlockingReason,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, TestCaseSelector,
    VerificationRequirement, VerificationScope, VerificationSpec, VerificationSpecId, WorktreeId,
};

pub(crate) async fn verify<F: KnowledgeStoreContractFactory>(
    factory: &F,
    workspace: &ContractWorkspace,
) -> ContractResult<()> {
    let app_data = workspace.app_data_root("step-criterion-roles");
    let common = workspace.create_directory("roles-common")?;
    let worktree = workspace.create_directory("roles-worktree")?;
    let project = project(
        RepositoryId::from_bytes([221; 32]),
        WorktreeId::from_bytes([222; 32]),
        &common,
        &worktree,
        unborn_head()?,
    )?;
    let addresses = AcceptanceCriterionId::from_bytes([241; 32]);
    let proves = AcceptanceCriterionId::from_bytes([242; 32]);
    let goal = GoalContract::initial(
        TaskId::from_bytes([223; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string("Preserve both step criterion roles".to_owned())?,
            vec![
                AcceptanceCriterion::new(
                    addresses,
                    AcceptanceCriterionStatement::try_from_string(
                        "Implementation progress".to_owned(),
                    )?,
                ),
                AcceptanceCriterion::new(
                    proves,
                    AcceptanceCriterionStatement::try_from_string(
                        "A distinct verified result".to_owned(),
                    )?,
                ),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("Current appropriate verification".to_owned())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1000)?,
    );
    let first = role_step(224, 225)?.with_criterion_coverage(vec![addresses], vec![proves])?;
    let original_definition = first.clone();
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![first],
        TaskLedgerTimestamp::from_unix_millis(1010)?,
    )?;
    let store = factory.open(&app_data).await?;
    CreateGoalContract::new(&store)
        .execute(&project, &goal)
        .await?;
    let initial = CreateTaskLedger::new(&store)
        .execute(&project, &ledger)
        .await?;
    crate::release_contract_store(store);
    let store = factory.open(&app_data).await?;
    assert_eq!(
        store.load_task_ledger(&project, goal.task_id()).await?,
        Some(initial.clone())
    );
    ledger.start_step(
        original_definition.id(),
        AgentRunId::from_bytes([226; 32]),
        TaskLedgerTimestamp::from_unix_millis(1020)?,
    )?;
    ledger.block_step(
        original_definition.id(),
        AgentRunId::from_bytes([226; 32]),
        TaskStepBlockingReason::try_from_string("Revised implementation is needed".to_owned())?,
        TaskLedgerTimestamp::from_unix_millis(1021)?,
    )?;
    let saved = SaveTaskLedger::new(&store)
        .execute(&project, initial.version(), &ledger)
        .await?;
    assert_eq!(
        store
            .replace_task_ledger(&project, initial.version(), initial.ledger())
            .await,
        Err(TaskLedgerStoreFailure::VersionConflict)
    );
    let before_replan = saved.clone();
    let replacement = role_step(227, 228)?.with_criterion_coverage(
        original_definition.addressed_criteria().to_vec(),
        original_definition.acceptance_criteria().to_vec(),
    )?;
    ledger.replan(
        vec![original_definition.id()],
        vec![replacement],
        TaskReplanReason::try_from_string(
            "Change the implementation while preserving its obligations".to_owned(),
        )?,
        TaskLedgerTimestamp::from_unix_millis(1030)?,
    )?;
    let revised = SaveTaskLedger::new(&store)
        .execute(&project, saved.version(), &ledger)
        .await?;
    assert_eq!(
        store
            .replace_task_ledger(&project, saved.version(), before_replan.ledger())
            .await,
        Err(TaskLedgerStoreFailure::VersionConflict)
    );
    assert_eq!(
        initial
            .ledger()
            .step(original_definition.id())
            .map(|s| s.definition()),
        Some(&original_definition)
    );
    assert_eq!(
        before_replan
            .ledger()
            .step(original_definition.id())
            .map(|s| s.definition()),
        Some(&original_definition)
    );
    crate::release_contract_store(store);
    let reopened = factory.open(&app_data).await?;
    let loaded = reopened
        .load_task_ledger(&project, goal.task_id())
        .await?
        .ok_or("ledger missing")?;
    assert_eq!(loaded, revised);
    let retained = loaded
        .ledger()
        .step(original_definition.id())
        .ok_or("old step missing")?;
    assert_eq!(retained.definition(), &original_definition);
    assert_eq!(
        retained.attempts(),
        before_replan
            .ledger()
            .step(original_definition.id())
            .ok_or("old attempt missing")?
            .attempts()
    );
    let next = loaded
        .ledger()
        .step(TaskStepId::from_bytes([227; 32]))
        .ok_or("replacement missing")?;
    assert_eq!(next.definition().addressed_criteria(), &[addresses]);
    assert_eq!(next.definition().acceptance_criteria(), &[proves]);
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

fn role_step(id: u8, spec: u8) -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([id; 32]),
        None,
        TaskStepOutcome::try_from_string(
            "Produce the next bounded implementation result".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "The implementation and proof roles remain distinct".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Matching current evidence".to_owned(),
        )?],
        VerificationSpec::test(
            VerificationSpecId::from_bytes([spec; 32]),
            VerificationRequirement::try_from_string("The bound test passes".to_owned())?,
            DiscoveredCommandId::from_bytes([229; 32]),
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        ),
    )?)
}
