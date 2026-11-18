//! Port controls only: these do not attest real persistence or executed product verification.

use a3_application::{
    CreateGoalContractFailure, GoalContractStore, GoalContractStoreFailure,
    GoalContractStoreFuture, MaterializeOriginalTask, MaterializeOriginalTaskFailure,
    StoredTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure, TaskLedgerStoreFuture,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRunId,
    CanonicalDirectory, CriterionEvidenceRequirement, CriterionOrigin,
    CriterionVerificationMismatch, DiagnosticPolicy, DiffInvariantMode, DiffInvariantVerification,
    DiscoveredCommandId, ExpectedTaskEvidence, GitHead, GitReferenceName, GoalContract,
    GoalContractDraft, GoalContractRevision, GoalContractTimestamp, GoalObjective,
    GoalOriginalRequest, MinimumTestCaseCount, ProjectIdentity, RemoteIdentity, RepositoryId,
    RepositoryIdentity, RepositoryPath, StepVerification, StepVerificationId,
    StepVerificationOutcome, SuccessVerification, TaskEvidenceId, TaskId, TaskLedger,
    TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale,
    TestCaseSelector, VerificationMethod, VerificationRequirement, VerificationScope,
    VerificationSpec, VerificationSpecId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use futures::executor::block_on;
use std::error::Error;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Default)]
struct State {
    goal: Option<GoalContract>,
    ledger: Option<StoredTaskLedger>,
    events: Vec<&'static str>,
    goal_create_error: Option<GoalContractStoreFailure>,
    ledger_create_error: Option<TaskLedgerStoreFailure>,
}

#[derive(Debug, Default)]
struct RecordingStore(Mutex<State>);

impl RecordingStore {
    fn state(&self) -> Result<MutexGuard<'_, State>, Box<dyn Error>> {
        self.0.lock().map_err(|_| TestError.into())
    }
}

#[derive(Debug)]
struct TestError;
impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("test lock failed")
    }
}
impl Error for TestError {}

impl GoalContractStore for RecordingStore {
    fn create_goal_contract<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        goal: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            state.events.push("create_goal");
            if let Some(error) = state.goal_create_error {
                return Err(error);
            }
            if state.goal.is_some() {
                return Err(GoalContractStoreFailure::TaskAlreadyExists);
            }
            state.goal = Some(goal.clone());
            Ok(())
        })
    }
    fn append_goal_contract_revision<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _goal: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async { Err(GoalContractStoreFailure::Unavailable) })
    }
    fn load_current_goal_contract<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        task: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            state.events.push("load_goal");
            Ok(state
                .goal
                .as_ref()
                .filter(|goal| goal.task_id() == task)
                .cloned())
        })
    }
    fn load_goal_contract_revision<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        task: TaskId,
        revision: GoalContractRevision,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async move {
            let state = self
                .0
                .lock()
                .map_err(|_| GoalContractStoreFailure::Unavailable)?;
            Ok(state
                .goal
                .as_ref()
                .filter(|goal| goal.task_id() == task && goal.revision() == revision)
                .cloned())
        })
    }
}

impl TaskLedgerStore for RecordingStore {
    fn create_task_ledger<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| TaskLedgerStoreFailure::Unavailable)?;
            state.events.push("create_ledger");
            if let Some(error) = state.ledger_create_error {
                return Err(error);
            }
            if state.ledger.is_some() {
                return Err(TaskLedgerStoreFailure::LedgerAlreadyExists);
            }
            state.ledger = Some(StoredTaskLedger::new(
                ledger.clone(),
                TaskLedgerStoreVersion::INITIAL,
            ));
            Ok(TaskLedgerStoreVersion::INITIAL)
        })
    }
    fn replace_task_ledger<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _version: TaskLedgerStoreVersion,
        _ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion> {
        Box::pin(async { Err(TaskLedgerStoreFailure::Unavailable) })
    }
    fn load_task_ledger<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        task: TaskId,
    ) -> TaskLedgerStoreFuture<'a, Option<StoredTaskLedger>> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| TaskLedgerStoreFailure::Unavailable)?;
            state.events.push("load_ledger");
            Ok(state
                .ledger
                .as_ref()
                .filter(|ledger| ledger.ledger().task_id() == task)
                .cloned())
        })
    }
}

fn draft(
    objective: &str,
    criteria: Vec<AcceptanceCriterion>,
) -> Result<GoalContractDraft, Box<dyn Error>> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(objective.to_owned())?,
        criteria,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("use the original proof obligations".to_owned())?,
    )?)
}

fn criterion(id: u8) -> Result<AcceptanceCriterion, Box<dyn Error>> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([id; 32]),
        AcceptanceCriterionStatement::try_from_string(format!("original outcome {id}"))?,
    ))
}

fn goal(objective: &str) -> Result<GoalContract, Box<dyn Error>> {
    let task = TaskId::from_bytes([1; 32]);
    let at = GoalContractTimestamp::from_unix_millis(1_000)?;
    let provisional = GoalContract::initial(
        task,
        draft(objective, vec![criterion(2)?, criterion(3)?])?,
        at,
    );
    let original_text = "Runtime behavior must pass.\nChange README.md.\n";
    let split = "Runtime behavior must pass.\n".len();
    let original = GoalOriginalRequest::try_from_bytes(
        provisional.reference(),
        original_text.as_bytes().to_vec(),
    )?;
    let runtime = criterion(2)?.with_origin(CriterionOrigin::new(
        original.range(0, split)?,
        CriterionEvidenceRequirement::ExistingTest {
            command: DiscoveredCommandId::from_bytes([20; 32]),
            selector: TestCaseSelector::All,
            minimum_cases: MinimumTestCaseCount::new(2)?,
            scope: VerificationScope::Package,
            python_integer_translation: None,
        },
    ));
    let change = criterion(3)?.with_origin(CriterionOrigin::new(
        original.range(split, original_text.len())?,
        CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
        )?),
    ));
    Ok(
        GoalContract::initial(task, draft(objective, vec![runtime, change])?, at)
            .with_original(original)?,
    )
}

fn runtime_spec(minimum: u32) -> Result<VerificationSpec, Box<dyn Error>> {
    Ok(VerificationSpec::test(
        VerificationSpecId::from_bytes([30; 32]),
        VerificationRequirement::try_from_string("current behavioral tests pass".to_owned())?,
        DiscoveredCommandId::from_bytes([20; 32]),
        TestCaseSelector::All,
        MinimumTestCaseCount::new(minimum)?,
        VerificationScope::Package,
    ))
}

fn step(
    id: u8,
    criterion_id: Option<u8>,
    spec: VerificationSpec,
) -> Result<TaskStepDefinition, Box<dyn Error>> {
    let definition = TaskStepDefinition::new(
        TaskStepId::from_bytes([id; 32]),
        None,
        TaskStepOutcome::try_from_string(format!("implement outcome {id}"))?,
        TaskStepRationale::try_from_string("the original criterion requires it".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "matching current evidence".to_owned(),
        )?],
        spec,
    )?;
    Ok(match criterion_id {
        Some(id) => definition
            .with_acceptance_criteria(vec![AcceptanceCriterionId::from_bytes([id; 32])])?,
        None => definition,
    })
}

fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = std::env::current_dir()?.canonicalize()?;
    let repository_id = RepositoryId::from_bytes([40; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            Some(RemoteIdentity::from_bytes([41; 32])),
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([42; 32]),
            WorktreeAnchorId::from_bytes([43; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
        },
    )?)
}

#[test]
fn wrong_diff_lint_unknown_criterion_and_weakened_test_make_no_port_calls()
-> Result<(), Box<dyn Error>> {
    let goal = goal("preserve runtime truth")?;
    let id = VerificationSpecId::from_bytes([30; 32]);
    let requirement =
        VerificationRequirement::try_from_string("do not weaken the original".to_owned())?;
    let diff = VerificationSpec::diff_invariant(
        id,
        requirement.clone(),
        DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
    );
    let lint = VerificationSpec::diagnostic(
        id,
        requirement,
        DiscoveredCommandId::from_bytes([20; 32]),
        DiagnosticPolicy::NoWarnings,
        VerificationScope::Package,
    );
    for (definition, unknown) in [
        (step(4, Some(2), diff)?, false),
        (step(4, Some(2), lint)?, false),
        (step(4, Some(99), runtime_spec(2)?)?, true),
        (step(4, Some(2), runtime_spec(1)?)?, false),
    ] {
        let store = RecordingStore::default();
        let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            vec![definition],
        ));
        if unknown {
            assert!(matches!(
                result,
                Err(MaterializeOriginalTaskFailure::UnknownCriterion { .. })
            ));
        } else {
            assert!(matches!(
                result,
                Err(MaterializeOriginalTaskFailure::ProofMismatch { .. })
            ));
        }
        assert!(store.state()?.events.is_empty());
        assert!(store.state()?.goal.is_none());
    }
    Ok(())
}

#[test]
fn original_mapping_and_horizon_limits_are_checked_before_ports() -> Result<(), Box<dyn Error>> {
    let plain = GoalContract::initial(
        TaskId::from_bytes([1; 32]),
        draft("legacy", vec![criterion(2)?])?,
        GoalContractTimestamp::from_unix_millis(1_000)?,
    );
    let store = RecordingStore::default();
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &plain,
            vec![step(4, Some(2), runtime_spec(2)?)?]
        )),
        Err(MaterializeOriginalTaskFailure::MissingOriginal)
    ));
    let goal = goal("bounded horizon")?;
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            vec![step(4, None, runtime_spec(2)?)?]
        )),
        Err(MaterializeOriginalTaskFailure::MissingCriterionMapping { .. })
    ));
    for definitions in [
        Vec::new(),
        (4..8)
            .map(|id| step(id, Some(2), runtime_spec(2)?))
            .collect::<Result<Vec<_>, _>>()?,
    ] {
        assert!(matches!(
            block_on(MaterializeOriginalTask::new(&store, &store).execute(
                &project()?,
                &goal,
                definitions
            )),
            Err(MaterializeOriginalTaskFailure::StepCount { .. })
        ));
    }
    assert!(store.state()?.events.is_empty());
    Ok(())
}

#[test]
fn exact_reentry_reuses_initial_state_and_keeps_unplanned_goal_order() -> Result<(), Box<dyn Error>>
{
    let goal = goal("materialize the next step only")?;
    let definitions = vec![step(4, Some(2), runtime_spec(2)?)?];
    let store = RecordingStore::default();
    let first = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        definitions.clone(),
    ))?;
    assert_eq!(first.goal_contract(), goal.reference());
    assert_eq!(
        first.unplanned_criteria(),
        &[AcceptanceCriterionId::from_bytes([3; 32])]
    );
    assert_eq!(
        first.stored_ledger().ledger().created_at().unix_millis(),
        goal.created_at().unix_millis()
    );
    assert_eq!(
        store.state()?.events,
        ["load_goal", "create_goal", "load_ledger", "create_ledger"]
    );
    let second = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        definitions,
    ))?;
    assert_eq!(first, second);
    assert_eq!(
        store.state()?.events,
        [
            "load_goal",
            "create_goal",
            "load_ledger",
            "create_ledger",
            "load_goal",
            "load_ledger"
        ]
    );
    Ok(())
}

#[test]
fn conflicting_goal_or_initial_definition_is_not_overwritten() -> Result<(), Box<dyn Error>> {
    let original = goal("original objective")?;
    let definitions = vec![step(4, Some(2), runtime_spec(2)?)?];
    let store = RecordingStore::default();
    block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &original,
        definitions.clone(),
    ))?;
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal("a different objective")?,
            definitions
        )),
        Err(MaterializeOriginalTaskFailure::GoalConflict)
    ));
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &original,
            vec![step(5, Some(2), runtime_spec(2)?)?]
        )),
        Err(MaterializeOriginalTaskFailure::LedgerConflict)
    ));
    assert_eq!(store.state()?.goal.as_ref(), Some(&original));
    assert_eq!(
        store
            .state()?
            .events
            .iter()
            .filter(|event| **event == "create_goal")
            .count(),
        1
    );
    assert_eq!(
        store
            .state()?
            .events
            .iter()
            .filter(|event| **event == "create_ledger")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn active_and_completed_ledger_conflict_with_initial_reentry() -> Result<(), Box<dyn Error>> {
    for completed in [false, true] {
        let goal = goal("no restarting existing work")?;
        let definitions = vec![step(4, Some(2), runtime_spec(2)?)?];
        let store = RecordingStore::default();
        let initial = block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            definitions.clone(),
        ))?;
        let mut ledger = initial.stored_ledger().ledger().clone();
        let step_id = TaskStepId::from_bytes([4; 32]);
        let run_id = AgentRunId::from_bytes([8; 32]);
        ledger.start_step(
            step_id,
            run_id,
            TaskLedgerTimestamp::from_unix_millis(1_001)?,
        )?;
        if completed {
            let evidence = TaskEvidenceId::from_bytes([9; 32]);
            ledger.begin_step_verification(
                step_id,
                run_id,
                None,
                vec![evidence],
                TaskLedgerTimestamp::from_unix_millis(1_002)?,
            )?;
            // Fixture state only, not claimed actual execution evidence.
            ledger.finish_step_verification(
                step_id,
                StepVerification::new(
                    StepVerificationId::from_bytes([10; 32]),
                    VerificationSpecId::from_bytes([30; 32]),
                    run_id,
                    StepVerificationOutcome::Passed,
                    vec![evidence],
                    TaskLedgerTimestamp::from_unix_millis(1_003)?,
                )?,
            )?;
        }
        store.state()?.ledger = Some(StoredTaskLedger::new(
            ledger.clone(),
            TaskLedgerStoreVersion::new(2)?,
        ));
        assert!(matches!(
            block_on(MaterializeOriginalTask::new(&store, &store).execute(
                &project()?,
                &goal,
                definitions
            )),
            Err(MaterializeOriginalTaskFailure::LedgerConflict)
        ));
        assert_eq!(
            store.state()?.ledger.as_ref().map(StoredTaskLedger::ledger),
            Some(&ledger)
        );
    }
    Ok(())
}

#[test]
fn ledger_failure_retains_valid_goal_and_requires_explicit_later_reentry()
-> Result<(), Box<dyn Error>> {
    let goal = goal("goal survives a ledger failure")?;
    let definitions = vec![step(4, Some(2), runtime_spec(2)?)?];
    let store = RecordingStore::default();
    store.state()?.ledger_create_error = Some(TaskLedgerStoreFailure::Unavailable);
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            definitions.clone()
        )),
        Err(MaterializeOriginalTaskFailure::Ledger(
            TaskLedgerStoreFailure::Unavailable
        ))
    ));
    assert_eq!(store.state()?.goal.as_ref(), Some(&goal));
    assert!(store.state()?.ledger.is_none());
    assert_eq!(
        store.state()?.events,
        ["load_goal", "create_goal", "load_ledger", "create_ledger"]
    );
    store.state()?.ledger_create_error = None;
    block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        definitions,
    ))?;
    assert_eq!(
        store.state()?.events,
        [
            "load_goal",
            "create_goal",
            "load_ledger",
            "create_ledger",
            "load_goal",
            "load_ledger",
            "create_ledger"
        ]
    );
    Ok(())
}

#[test]
fn create_races_are_typed_errors_without_reload_or_retry() -> Result<(), Box<dyn Error>> {
    let goal = goal("do not heal persistence races")?;
    let definitions = vec![step(4, Some(2), runtime_spec(2)?)?];
    let goal_race = RecordingStore::default();
    goal_race.state()?.goal_create_error = Some(GoalContractStoreFailure::TaskAlreadyExists);
    assert!(matches!(
        block_on(
            MaterializeOriginalTask::new(&goal_race, &goal_race).execute(
                &project()?,
                &goal,
                definitions.clone()
            )
        ),
        Err(MaterializeOriginalTaskFailure::GoalCreate(
            CreateGoalContractFailure::Store(GoalContractStoreFailure::TaskAlreadyExists)
        ))
    ));
    assert_eq!(goal_race.state()?.events, ["load_goal", "create_goal"]);
    let ledger_race = RecordingStore::default();
    ledger_race.state()?.ledger_create_error = Some(TaskLedgerStoreFailure::LedgerAlreadyExists);
    assert!(matches!(
        block_on(
            MaterializeOriginalTask::new(&ledger_race, &ledger_race).execute(
                &project()?,
                &goal,
                definitions
            )
        ),
        Err(MaterializeOriginalTaskFailure::Ledger(
            TaskLedgerStoreFailure::LedgerAlreadyExists
        ))
    ));
    assert_eq!(
        ledger_race.state()?.events,
        ["load_goal", "create_goal", "load_ledger", "create_ledger"]
    );
    Ok(())
}

#[test]
fn weakening_returns_the_original_closed_mismatch_reason() -> Result<(), Box<dyn Error>> {
    let store = RecordingStore::default();
    let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal("minimum cases")?,
        vec![step(4, Some(2), runtime_spec(1)?)?],
    ));
    assert!(matches!(
        result,
        Err(MaterializeOriginalTaskFailure::ProofMismatch {
            mismatch: CriterionVerificationMismatch::MinimumCasesTooSmall,
            ..
        })
    ));
    assert!(store.state()?.events.is_empty());
    Ok(())
}

#[test]
fn matching_change_only_step_materializes_without_claiming_runtime_coverage()
-> Result<(), Box<dyn Error>> {
    let goal = goal("prepare the requested change")?;
    let spec = VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([31; 32]),
        VerificationRequirement::try_from_string(
            "apply only the requested README change".to_owned(),
        )?,
        DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
        )?,
    );
    let store = RecordingStore::default();
    let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        vec![step(4, Some(3), spec)?],
    ))?;
    assert_eq!(
        result.unplanned_criteria(),
        &[AcceptanceCriterionId::from_bytes([2; 32])]
    );
    assert_eq!(result.stored_ledger().ledger().steps().len(), 1);
    Ok(())
}

#[test]
fn absent_runtime_recipe_remains_open_without_persistence() -> Result<(), Box<dyn Error>> {
    let previous = goal("runtime recipe is not implemented yet")?;
    let original = previous.original().cloned().ok_or(TestError)?;
    let mut criteria = previous.draft().acceptance_criteria().to_vec();
    let existing_origin = criteria[0].origin().ok_or(TestError)?;
    criteria[0] = criteria[0].clone().with_origin(CriterionOrigin::new(
        existing_origin.original_range(),
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    ));
    let goal = GoalContract::initial(
        previous.task_id(),
        draft("runtime recipe is not implemented yet", criteria)?,
        previous.created_at(),
    )
    .with_original(original)?;
    let store = RecordingStore::default();
    let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        vec![step(4, Some(2), runtime_spec(2)?)?],
    ));
    assert!(matches!(
        result,
        Err(MaterializeOriginalTaskFailure::ProofMismatch {
            mismatch: CriterionVerificationMismatch::RuntimeRecipeUnavailable,
            ..
        })
    ));
    assert!(store.state()?.events.is_empty());
    Ok(())
}

#[test]
fn progress_diff_is_materialized_without_closing_the_original_runtime_obligation()
-> Result<(), Box<dyn Error>> {
    let goal = goal("create a bounded scaffold before final tests")?;
    let diff = VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([31; 32]),
        VerificationRequirement::try_from_string("README is the only changed path".to_owned())?,
        DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
        )?,
    );
    let progress = step(4, None, diff.clone())?
        .with_criterion_coverage(vec![AcceptanceCriterionId::from_bytes([2; 32])], Vec::new())?;
    let store = RecordingStore::default();
    let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        vec![progress.clone()],
    ))?;
    assert_eq!(
        result.unplanned_criteria(),
        &[
            AcceptanceCriterionId::from_bytes([2; 32]),
            AcceptanceCriterionId::from_bytes([3; 32])
        ]
    );
    let restored = result
        .stored_ledger()
        .ledger()
        .steps()
        .next()
        .ok_or("step missing")?
        .definition();
    assert_eq!(restored, &progress);
    assert!(restored.acceptance_criteria().is_empty());
    // The same local Diff can prove the separate original change-only criterion,
    // while its Addresses role still leaves the runtime criterion open.
    let mixed = progress.with_criterion_coverage(
        vec![AcceptanceCriterionId::from_bytes([2; 32])],
        vec![AcceptanceCriterionId::from_bytes([3; 32])],
    )?;
    let store = RecordingStore::default();
    let result = block_on(MaterializeOriginalTask::new(&store, &store).execute(
        &project()?,
        &goal,
        vec![mixed],
    ))?;
    assert_eq!(
        result.unplanned_criteria(),
        &[AcceptanceCriterionId::from_bytes([2; 32])]
    );
    Ok(())
}

#[test]
fn foreign_progress_and_nonoperational_local_specs_stop_before_materializer_ports()
-> Result<(), Box<dyn Error>> {
    let goal = goal("keep source roles bound")?;
    let unknown = step(4, None, runtime_spec(2)?)?.with_criterion_coverage(
        vec![AcceptanceCriterionId::from_bytes([99; 32])],
        Vec::new(),
    )?;
    let store = RecordingStore::default();
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            vec![unknown]
        )),
        Err(MaterializeOriginalTaskFailure::UnknownCriterion { .. })
    ));
    assert!(store.state()?.events.is_empty());
    let spec = VerificationSpec::new(
        VerificationSpecId::from_bytes([31; 32]),
        VerificationMethod::DiffInvariant,
        VerificationRequirement::try_from_string(
            "legacy description is not executable proof".to_owned(),
        )?,
    );
    let progress = step(4, None, spec)?
        .with_criterion_coverage(vec![AcceptanceCriterionId::from_bytes([2; 32])], Vec::new())?;
    assert!(matches!(
        block_on(MaterializeOriginalTask::new(&store, &store).execute(
            &project()?,
            &goal,
            vec![progress]
        )),
        Err(MaterializeOriginalTaskFailure::NonOperationalVerification { .. })
    ));
    assert!(store.state()?.events.is_empty());
    Ok(())
}
