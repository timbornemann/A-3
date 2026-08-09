use crate::fixture::{ContractWorkspace, project, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    CreateGoalContract, CreateTaskLedger, SaveTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, AgentRunId, DiscoveredCommandId, ExpectedTaskEvidence,
    GoalConstraint, GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective,
    MinimumTestCaseCount, NonGoal, RepositoryId, StepDependency, StepVerification,
    StepVerificationId, StepVerificationOutcome, SuccessVerification, TaskEvidenceId, TaskId,
    TaskLedger, TaskLedgerTimestamp, TaskReplanReason, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, TaskStepResultSummary, TestCaseSelector, UserDecision,
    VerificationRequirement, VerificationScope, VerificationSpec, VerificationSpecId, WorktreeId,
};

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("task-ledgers");
    let common = workspace.create_directory("task-ledger-common")?;
    let first_root = workspace.create_directory("task-ledger-first")?;
    let second_root = workspace.create_directory("task-ledger-second")?;
    let repository_id = RepositoryId::from_bytes([220; 32]);
    let first = project(
        repository_id,
        WorktreeId::from_bytes([221; 32]),
        &common,
        &first_root,
        unborn_head()?,
    )?;
    let second = project(
        repository_id,
        WorktreeId::from_bytes([222; 32]),
        &common,
        &second_root,
        unborn_head()?,
    )?;
    let task_id = TaskId::from_bytes([223; 32]);
    let goal = GoalContract::initial(
        task_id,
        goal_draft()?,
        GoalContractTimestamp::from_unix_millis(1_000)?,
    );
    let first_step_id = TaskStepId::from_bytes([224; 32]);
    let second_step_id = TaskStepId::from_bytes([225; 32]);
    let future_step_id = TaskStepId::from_bytes([226; 32]);
    let initial = TaskLedger::new(
        goal.reference(),
        vec![
            step(first_step_id, None, 227)?,
            step(
                second_step_id,
                Some(StepDependency::new(first_step_id)),
                228,
            )?,
            step(future_step_id, None, 229)?,
        ],
        TaskLedgerTimestamp::from_unix_millis(1_010)?,
    )?;

    let store = factory.open(&app_data_root).await?;
    CreateGoalContract::new(&store)
        .execute(&first, &goal)
        .await?;
    let created = CreateTaskLedger::new(&store)
        .execute(&first, &initial)
        .await
        .map_err(|error| std::io::Error::other(format!("create Task Ledger: {error:?}")))?;
    assert_eq!(created.version(), TaskLedgerStoreVersion::INITIAL);
    assert_eq!(created.ledger(), &initial);
    assert_eq!(
        store.load_task_ledger(&first, task_id).await?,
        Some(created)
    );
    assert_eq!(
        store.create_task_ledger(&first, &initial).await,
        Err(TaskLedgerStoreFailure::LedgerAlreadyExists)
    );
    assert_eq!(
        store.load_task_ledger(&second, task_id).await?,
        None,
        "a linked worktree must not see another worktree's Task Ledger"
    );

    let stale_writer = initial.clone();
    let failed_evidence = TaskEvidenceId::from_bytes([230; 32]);
    let first_run = AgentRunId::from_bytes([231; 32]);
    let mut ledger = initial;
    ledger.start_step(
        first_step_id,
        first_run,
        TaskLedgerTimestamp::from_unix_millis(1_020)?,
    )?;
    ledger.begin_step_verification(
        first_step_id,
        first_run,
        Some(TaskStepResultSummary::try_from_string(
            "first implementation attempt".to_owned(),
        )?),
        vec![failed_evidence],
        TaskLedgerTimestamp::from_unix_millis(1_021)?,
    )?;
    ledger.finish_step_verification(
        first_step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([232; 32]),
            VerificationSpecId::from_bytes([227; 32]),
            first_run,
            StepVerificationOutcome::Failed {
                summary: a3_domain::VerificationFailureSummary::try_from_string(
                    "one assertion failed".to_owned(),
                )?,
            },
            vec![failed_evidence],
            TaskLedgerTimestamp::from_unix_millis(1_022)?,
        )?,
    )?;
    let saved = SaveTaskLedger::new(&store)
        .execute(&first, TaskLedgerStoreVersion::INITIAL, &ledger)
        .await?;
    assert_eq!(saved.version().get(), 2);
    assert_eq!(
        store
            .replace_task_ledger(&first, TaskLedgerStoreVersion::INITIAL, &stale_writer)
            .await,
        Err(TaskLedgerStoreFailure::VersionConflict),
        "a stale writer must not replace verified history"
    );
    assert_eq!(
        store
            .replace_task_ledger(&first, saved.version(), &stale_writer)
            .await,
        Err(TaskLedgerStoreFailure::InvalidStoredData),
        "the current writer must not erase retained attempt history"
    );

    let passed_evidence = TaskEvidenceId::from_bytes([233; 32]);
    let second_run = AgentRunId::from_bytes([234; 32]);
    ledger.start_step(
        first_step_id,
        second_run,
        TaskLedgerTimestamp::from_unix_millis(1_023)?,
    )?;
    complete_step(
        &mut ledger,
        first_step_id,
        VerificationSpecId::from_bytes([227; 32]),
        second_run,
        passed_evidence,
        235,
        1_024,
    )?;
    let dependent_evidence = TaskEvidenceId::from_bytes([236; 32]);
    let dependent_run = AgentRunId::from_bytes([237; 32]);
    ledger.start_step(
        second_step_id,
        dependent_run,
        TaskLedgerTimestamp::from_unix_millis(1_026)?,
    )?;
    complete_step(
        &mut ledger,
        second_step_id,
        VerificationSpecId::from_bytes([228; 32]),
        dependent_run,
        dependent_evidence,
        238,
        1_027,
    )?;
    let saved = SaveTaskLedger::new(&store)
        .execute(&first, saved.version(), &ledger)
        .await?;
    assert_eq!(saved.version().get(), 3);

    let invalidation = ledger.invalidate_verification_evidence(
        vec![passed_evidence],
        TaskLedgerTimestamp::from_unix_millis(1_029)?,
    )?;
    assert_eq!(invalidation.direct_step_ids(), &[first_step_id]);
    assert_eq!(invalidation.dependent_step_ids(), &[second_step_id]);
    let saved = SaveTaskLedger::new(&store)
        .execute(&first, saved.version(), &ledger)
        .await?;
    assert_eq!(saved.version().get(), 4);

    ledger.replan(
        vec![future_step_id],
        vec![step(TaskStepId::from_bytes([239; 32]), None, 240)?],
        TaskReplanReason::try_from_string(
            "replace the remaining future step after evidence invalidation".to_owned(),
        )?,
        TaskLedgerTimestamp::from_unix_millis(1_030)?,
    )?;
    let saved = SaveTaskLedger::new(&store)
        .execute(&first, saved.version(), &ledger)
        .await?;
    assert_eq!(saved.version().get(), 5);
    assert_eq!(saved.ledger(), &ledger);
    crate::release_contract_store(store);

    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened.load_task_ledger(&first, task_id).await?,
        Some(saved),
        "restart must recover exact materialized state, attempts, verification, stale causes, and replans"
    );
    crate::release_contract_store(reopened);
    crate::complete_contract_phase()
}

fn complete_step(
    ledger: &mut TaskLedger,
    step_id: TaskStepId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    evidence_id: TaskEvidenceId,
    verification_id: u8,
    transition_time: u64,
) -> ContractResult<()> {
    ledger.begin_step_verification(
        step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            "implementation finished".to_owned(),
        )?),
        vec![evidence_id],
        TaskLedgerTimestamp::from_unix_millis(transition_time)?,
    )?;
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([verification_id; 32]),
            spec_id,
            run_id,
            StepVerificationOutcome::Passed,
            vec![evidence_id],
            TaskLedgerTimestamp::from_unix_millis(transition_time + 1)?,
        )?,
    )?;
    Ok(())
}

fn step(
    id: TaskStepId,
    dependency: Option<StepDependency>,
    spec_id: u8,
) -> ContractResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        id,
        None,
        TaskStepOutcome::try_from_string(format!("produce step {spec_id}"))?,
        TaskStepRationale::try_from_string("required by the contract scenario".to_owned())?,
        dependency.into_iter().collect(),
        vec![ExpectedTaskEvidence::try_from_string(
            "the deterministic check output".to_owned(),
        )?],
        VerificationSpec::test(
            VerificationSpecId::from_bytes([spec_id; 32]),
            VerificationRequirement::try_from_string("the targeted check passes".to_owned())?,
            DiscoveredCommandId::from_bytes([spec_id.saturating_add(1); 32]),
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        ),
    )?
    .with_acceptance_criteria(vec![AcceptanceCriterionId::from_bytes([241; 32])])?)
}

fn goal_draft() -> ContractResult<GoalContractDraft> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("persist the verified Task Ledger".to_owned())?,
        vec![
            AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([241; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "restart restores exact ledger state".to_owned(),
                )?,
            ),
            AcceptanceCriterion::with_requirement(
                AcceptanceCriterionId::from_bytes([242; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "diagnostic detail remains convenient".to_owned(),
                )?,
                AcceptanceCriterionRequirement::Should,
            ),
        ],
        vec![GoalConstraint::try_from_string(
            "retain verification evidence".to_owned(),
        )?],
        vec![NonGoal::try_from_string(
            "do not execute tool commands".to_owned(),
        )?],
        vec![UserDecision::try_from_string(
            "use optimistic versions".to_owned(),
        )?],
        SuccessVerification::try_from_string("reopen and compare the complete ledger".to_owned())?,
    )?)
}
