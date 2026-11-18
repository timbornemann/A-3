//! Manifest-bound A01-shaped checks through the candidate's existing mutation controller.

#[path = "plan22_source_failure_replan_tests.rs"]
mod source_failure_replan_tests;

use super::*;
use a3_application::{
    AcceptanceRejection, AcceptanceVerifier, AcceptanceVerifierOutcome, AcceptanceVerifierTimeout,
    AgentApprovalAction, AgentApprovalFileOperation, AgentApprovalNetworkScope,
    AgentApprovalWorkingDirectory, AgentLogPageLimit, AgentLogPageOffset, ContinueSourceAgentPlan,
    ContinueVerifiedAgentPlanOutcome, MutationFailureClass, SourcePlanContinuationOutcome,
};
use a3_domain::{
    CommandDiscoveryEvidence, EvidenceDependency, PatchAction, PatchActionSchemaVersion,
    PatchFileContent, PatchOperation, PatchRationale, PatchUpdate, ProcessStream,
    PythonIntegerTranslation, StepDependency, SymbolName, TaskStepAttemptOutcome, TestCaseOutcome,
    VerificationEvidenceEvaluation, VerificationEvidenceFailure, WorkspaceDirectory,
};
use a3_repo_index::PythonLanguageAdapter;

const MANIFEST: &[u8] =
    b"[project]\nname = \"a3-eval-local\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\n";
const TEST_BODY: &[u8] = b"import unittest\nfrom subject import adjust\n\nclass Behavior(unittest.TestCase):\n    def test_known_example(self):\n        self.assertEqual(adjust(41), 42)\n";
const SUBJECT_PATH: &str = "subject.py";
const INITIAL: &[u8] = b"def adjust(value: int) -> int:\n    return value + (2)\n";
const CORRECT: &[u8] = b"def adjust(value: int) -> int:\n    return value + 1\n";
const INCORRECT: &[u8] = b"def adjust(value: int) -> int:\n    return value + 3\n";
const CONSTANT: &[u8] = b"def adjust(value: int) -> int:\n    return 42\n";
const TEST_ARGV: [&str; 6] = ["-B", "-m", "unittest", "discover", "-s", "tests"];
const CHANGE_STEP: TaskStepId = TaskStepId::from_bytes([STEP; 32]);
const CHECK_STEP: TaskStepId = TaskStepId::from_bytes([218; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckResult {
    Pass,
    Fail,
    GuardedPass,
    GuardedRejection,
    GuardedZeroCases,
}

impl CheckResult {
    fn guarded(self) -> bool {
        matches!(
            self,
            Self::GuardedPass | Self::GuardedRejection | Self::GuardedZeroCases
        )
    }

    fn stopped(self) -> bool {
        matches!(self, Self::GuardedRejection | Self::GuardedZeroCases)
    }
}

#[test]
fn manifest_bound_unittest_verifies_current_correct_patch_but_leaves_runtime_obligation_open()
-> TestResult {
    run_manifest_bound_case(CORRECT, CheckResult::Pass)
}

#[test]
fn manifest_bound_unittest_observes_a_real_red_check_after_incorrect_patch() -> TestResult {
    run_manifest_bound_case(INCORRECT, CheckResult::Fail)
}

#[test]
fn manifest_bound_unittest_and_current_translation_reach_source_done_after_reopen() -> TestResult {
    run_manifest_bound_case(CORRECT, CheckResult::GuardedPass)
}

#[test]
fn manifest_bound_unittest_passes_constant_but_source_proof_keeps_run_open() -> TestResult {
    run_manifest_bound_case(CONSTANT, CheckResult::GuardedRejection)
}

#[test]
fn guarded_unittest_with_green_exit_but_zero_cases_stops_after_reopen() -> TestResult {
    run_manifest_bound_case(CORRECT, CheckResult::GuardedZeroCases)
}

fn run_manifest_bound_case(replacement: &[u8], expected: CheckResult) -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = prepare_manifest_bound_fixture()?;
            let worktree = fixture.root.join("worktree");
            if expected == CheckResult::GuardedZeroCases {
                // A class-level SkipTest yields a real green exit with zero tests run.
                let mut tests = TEST_BODY.to_vec();
                tests.extend_from_slice(
                    concat!(
                        "    @classmethod\n",
                        "    def setUpClass(cls):\n",
                        "        raise unittest.SkipTest('fixture skips before any test')\n",
                    )
                    .as_bytes(),
                );
                fs::write(worktree.join("tests/test_behavior.py"), tests)?;
            }
            let mut expected_files = inventory(&worktree)?;
            let subject = expected_files
                .iter_mut()
                .find(|(path, _)| path == Path::new(SUBJECT_PATH))
                .ok_or("subject missing from the initial inventory")?;
            subject.1 = replacement.to_vec();
            exercise_manifest_bound_check(&fixture, replacement, expected).await?;
            assert_eq!(inventory(&worktree)?, expected_files);
            fixture.cleanup()
        })
    })
}

fn prepare_manifest_bound_fixture() -> TestResult<Fixture> {
    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    fs::create_dir(worktree.join("tests"))?;
    fs::write(worktree.join("pyproject.toml"), MANIFEST)?;
    fs::write(worktree.join("tests/__init__.py"), b"")?;
    fs::write(worktree.join("tests/test_behavior.py"), TEST_BODY)?;
    fs::write(worktree.join(SUBJECT_PATH), INITIAL)?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    Ok(fixture)
}

async fn exercise_manifest_bound_check(
    fixture: &Fixture,
    replacement: &[u8],
    expected: CheckResult,
) -> TestResult {
    let project = &fixture.project;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let legacy = DiscoverProjectCommands.execute(project.worktree().id(), &published)?;
    assert!(
        legacy
            .commands()
            .iter()
            .all(|item| item.kind() != DiscoveredCommandKind::Test)
    );
    let catalog = DiscoverProjectCommands.execute_plan22(project.worktree().id(), &published)?;
    let command = manifest_test_command(&catalog)?;
    let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
    let translation = if expected.guarded() {
        Some(PythonIntegerTranslation::new(
            path.clone(),
            SymbolName::try_from_string("adjust".to_owned())?,
            1,
        )?)
    } else {
        None
    };
    let (goal, steps) = manifest_bound_goal_and_steps(command, &path, translation)?;
    let profile = profile()?;
    let (mut run, stored, open) =
        start_fixture_step(fixture, &store, &goal, &profile, base, steps).await?;
    if expected.guarded() {
        assert!(open.is_empty());
    } else {
        assert_eq!(open, [goal.draft().acceptance_criteria()[0].id()]);
    }
    let (mut ledger, mut version) = stored.into_parts();
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let runner = native_runner()?;
    let source_verifier = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
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
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref())
    .with_python_integer_translation_verifier(&source_verifier);
    let controller = if expected.guarded() {
        controller.with_source_failure_replan(&goal)
    } else {
        controller
    };
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path() == &path)
        .ok_or("subject is missing from the current index")?;
    assert_eq!(
        revision.content_hash().as_bytes(),
        blake3::hash(INITIAL).as_bytes()
    );
    let patch_action = PatchAction::new(
        PatchActionSchemaVersion::V1,
        run.id(),
        project.worktree().id(),
        base,
        CHANGE_STEP,
        VerificationSpecId::from_bytes([160; 32]),
        PatchRationale::try_from_string("Apply the one controlled source edit".to_owned())?,
        vec![PatchOperation::Update(PatchUpdate::new(
            revision.clone(),
            PatchFileContent::try_from_bytes(replacement.to_vec())?,
        )?)],
    )?;
    let patch_fingerprint = patch_action.policy_action().fingerprint();
    let outcome = execute_approved_observing(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::ApplyPatch(Box::new(patch_action)),
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let presentation = approvals
                .presentation(project, goal.task_id())?
                .ok_or("exact patch approval presentation missing")?;
            assert_eq!(presentation.request_id(), request);
            assert_eq!(presentation.action_fingerprint(), patch_fingerprint);
            let AgentApprovalAction::Patch(patch) = presentation.action() else {
                return Err("expected patch approval".into());
            };
            let [file] = patch.files() else {
                return Err("unexpected patch approval scope".into());
            };
            assert_eq!(file.operation(), AgentApprovalFileOperation::Update);
            assert_eq!(file.source_path(), Some(&path));
            assert_eq!(file.target_path(), Some(&path));
            assert_eq!(
                fs::read(fixture.root.join("worktree").join(SUBJECT_PATH))?,
                INITIAL
            );
            Ok(())
        },
    )
    .await?;
    let MutationControllerOutcome::StepVerified { snapshot_id, .. } = outcome else {
        return Err(format!("scoped patch did not verify: {outcome:?}").into());
    };
    assert_ne!(snapshot_id, base);
    assert_eq!(run.current_snapshot_id(), snapshot_id);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "process event sink poisoned")?
            .is_empty()
    );
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("post-patch publication missing")?;
    assert_eq!(current.run().snapshot_id(), snapshot_id);
    let current_catalog =
        DiscoverProjectCommands.execute_plan22(project.worktree().id(), &current)?;
    assert_eq!(manifest_test_command(&current_catalog)?, command);

    let continuation = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot_id,
            RunEventId::from_bytes([80; 32]),
            at(14)?,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::Continued(ContinueVerifiedAgentPlanOutcome::StepStarted {
        step_id,
        ledger_version,
    }) = continuation
    else {
        return Err("Core did not start the dependent verification step".into());
    };
    assert_eq!(step_id, CHECK_STEP);
    version = ledger_version;
    let confirmation = ConfirmProjectCommandAllowlist::new(store.as_ref())
        .execute(project, &current_catalog, vec![command], at(15)?, None)
        .await?;
    let check = RequestAgentFinish
        .verification_command(ledger.step(step_id).ok_or("active check step missing")?)
        .ok_or("Core did not select the current verification command")?;
    assert_eq!(check.command_id(), command);
    let outcome = execute_approved_observing(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &current,
        AgentAction::Run(check),
        Some((&current_catalog, &confirmation)),
        100,
        16,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let presentation = approvals
                .presentation(project, goal.task_id())?
                .ok_or("exact process approval presentation missing")?;
            assert_eq!(presentation.request_id(), request);
            assert_eq!(presentation.context().snapshot_id(), snapshot_id);
            let AgentApprovalAction::Process(spec) = presentation.action() else {
                return Err("expected process approval".into());
            };
            assert_eq!(spec.executable(), "python");
            assert_eq!(
                spec.arguments()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                TEST_ARGV
            );
            assert_eq!(
                spec.working_directory(),
                &AgentApprovalWorkingDirectory::Root
            );
            assert_eq!(spec.timeout_millis(), 30_000);
            assert_eq!(spec.network(), AgentApprovalNetworkScope::Denied);
            assert!(
                events
                    .values
                    .lock()
                    .map_err(|_| "process event sink poisoned")?
                    .is_empty()
            );
            Ok(())
        },
    )
    .await;
    let overview = inspection
        .overview(project, goal.task_id())?
        .ok_or("actual unittest inspection missing")?;
    let [process] = overview.processes() else {
        return Err("expected one actual process observation".into());
    };
    assert!(matches!(
        process.termination(),
        ProcessTermination::Exited(exit) if exit.success() == (expected != CheckResult::Fail)
    ));

    let evidence = match expected {
        CheckResult::Pass | CheckResult::GuardedPass => {
            let MutationControllerOutcome::StepVerified {
                evidence_id,
                snapshot_id: verified,
            } = outcome?
            else {
                return Err("actual green unittest did not complete the bound check".into());
            };
            assert_eq!(verified, snapshot_id);
            assert_eq!(context.calls.load(Ordering::SeqCst), 0);
            Some(evidence_id)
        }
        CheckResult::GuardedZeroCases => {
            let stderr = inspection.load_process_log_page(
                project,
                goal.task_id(),
                overview.revision(),
                process.id(),
                ProcessStream::Stderr,
                AgentLogPageOffset::START,
                AgentLogPageLimit::DEFAULT,
            )?;
            assert!(!stderr.page_truncated());
            assert!(!stderr.source_truncated());
            assert!(stderr.redaction().is_none());
            assert!(
                stderr
                    .text()
                    .lines()
                    .any(|line| line.starts_with("Ran 0 tests "))
            );
            let MutationControllerOutcome::Stopped {
                failure: MutationFailureClass::VerificationFailed,
                snapshot_id: stopped,
            } = outcome?
            else {
                return Err("zero-case test did not stop the guarded check".into());
            };
            assert_eq!(stopped, snapshot_id);
            assert_eq!(run.state(), AgentControllerState::Failed);
            assert_eq!(context.calls.load(Ordering::SeqCst), 0);
            None
        }
        CheckResult::GuardedRejection => {
            let MutationControllerOutcome::Stopped {
                failure: MutationFailureClass::VerificationFailed,
                snapshot_id: stopped,
            } = outcome?
            else {
                return Err("missing source proof did not stop the guarded check".into());
            };
            assert_eq!(stopped, snapshot_id);
            assert_eq!(run.state(), AgentControllerState::Failed);
            assert_eq!(context.calls.load(Ordering::SeqCst), 0);
            let verification = ledger
                .step(CHECK_STEP)
                .and_then(|step| step.attempts().first())
                .and_then(|attempt| attempt.verification())
                .ok_or("failed verification missing its actual test evidence")?;
            let [evidence_id] = verification.evidence_ids() else {
                return Err("expected exactly one actual unguarded test artifact".into());
            };
            Some(*evidence_id)
        }
        CheckResult::Fail => {
            let error = outcome
                .err()
                .ok_or("incomplete product verification unexpectedly passed")?;
            assert!(matches!(
                error.downcast_ref::<MutationControllerFailure>(),
                Some(MutationControllerFailure::Context(
                    ContextCompileFailure::InvalidPack
                ))
            ));
            assert_eq!(context.calls.load(Ordering::SeqCst), 1);
            let expected = run.last_event_sequence();
            let advance = RequestAgentFinish.execute(
                &mut run,
                AgentFinishAction,
                RunEventId::from_bytes([190; 32]),
                snapshot_id,
                at(20)?,
                &ActiveControl,
            )?;
            AppendRunEvent::new(store.as_ref())
                .execute(project, expected, &run, advance.event())
                .await?;
            let step = ledger.step(CHECK_STEP).ok_or("failed check step missing")?;
            match step
                .attempts()
                .first()
                .and_then(|attempt| attempt.verification())
            {
                Some(verification) => {
                    assert!(!verification.passed());
                    let [evidence_id] = verification.evidence_ids() else {
                        return Err("expected one actual unguarded test artifact".into());
                    };
                    Some(*evidence_id)
                }
                None => None,
            }
        }
    };
    assert_eq!(
        evidence.is_none(),
        matches!(expected, CheckResult::Fail | CheckResult::GuardedZeroCases)
    );

    drop(refresh);
    drop(store);
    assert_reopened_manifest_check(fixture, &goal, &run, &ledger, evidence, command, expected).await
}

fn manifest_test_command(catalog: &ProjectCommandCatalog) -> TestResult<DiscoveredCommandId> {
    let mut tests = catalog
        .commands()
        .iter()
        .filter(|entry| entry.kind() == DiscoveredCommandKind::Test);
    let entry = tests
        .next()
        .ok_or("manifest-bound unittest discovery missing")?;
    assert!(tests.next().is_none());
    assert_eq!(entry.executable().as_str(), "python");
    assert_eq!(entry.working_directory(), &WorkspaceDirectory::Root);
    assert_eq!(
        entry
            .arguments()
            .iter()
            .map(|argument| argument.as_str())
            .collect::<Vec<_>>(),
        TEST_ARGV
    );
    assert!(!entry.is_core_owned_check());
    let mut files = entry
        .evidence()
        .iter()
        .map(|evidence| {
            let CommandDiscoveryEvidence::File(revision) = evidence else {
                return Err("unittest discovery needs current manifest and test file revisions");
            };
            Ok(revision.path().as_bytes())
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.sort_unstable();
    assert_eq!(
        files,
        [
            b"pyproject.toml".as_slice(),
            b"tests/__init__.py",
            b"tests/test_behavior.py"
        ]
    );
    Ok(entry.id())
}

fn manifest_bound_goal_and_steps(
    command: DiscoveredCommandId,
    path: &RepositoryPath,
    translation: Option<PythonIntegerTranslation>,
) -> TestResult<(GoalContract, Vec<TaskStepDefinition>)> {
    let task = TaskId::from_bytes([1; 32]);
    let behavior = "Korrigiere subject.py: adjust(value) erhöht jede ganze Zahl exakt um eins. ";
    let no_constants = "Keine Konstantantworten; ";
    let test = "vorhandene Tests unverändert lassen und ausführen. ";
    let bounds = "Keine neuen Tests oder Abhängigkeiten.";
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        format!("{behavior}{no_constants}{test}{bounds}").into_bytes(),
    )?;
    let no_constants_start = behavior.len();
    let test_start = no_constants_start + no_constants.len();
    let bounds_start = test_start + test.len();
    let minimum = MinimumTestCaseCount::new(1)?;
    let test_requirement = CriterionEvidenceRequirement::ExistingTest {
        command,
        selector: TestCaseSelector::All,
        minimum_cases: minimum,
        scope: VerificationScope::Workspace,
        python_integer_translation: translation.clone(),
    };
    let runtime = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([2; 32]),
        AcceptanceCriterionStatement::try_from_string(behavior.trim().to_owned())?,
    )
    .with_origin(CriterionOrigin::new(
        original.range(0, no_constants_start)?,
        if translation.is_some() {
            test_requirement.clone()
        } else {
            CriterionEvidenceRequirement::RuntimeNeedsRecipe
        },
    ));
    let check = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([3; 32]),
        AcceptanceCriterionStatement::try_from_string(test.trim().to_owned())?,
    )
    .with_origin(CriterionOrigin::new(
        original.range(test_start, bounds_start)?,
        test_requirement,
    ));
    let (runtime_id, check_id) = (runtime.id(), check.id());
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string(
            "Correct the integer adjustment and run existing tests".to_owned(),
        )?,
        vec![runtime, check],
        vec![
            GoalConstraint::try_from_string(no_constants.trim().to_owned())?
                .with_origin(original.range(no_constants_start, test_start)?),
            GoalConstraint::try_from_string(bounds.to_owned())?
                .with_origin(original.range(bounds_start, original.as_bytes().len())?),
        ],
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string(
            "Use the current manifest-bound unittest evidence".to_owned(),
        )?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete());
    let change = TaskStepDefinition::new(
        CHANGE_STEP,
        None,
        TaskStepOutcome::try_from_string("Apply the scoped adjustment change".to_owned())?,
        TaskStepRationale::try_from_string(
            "Keep the existing tests and manifest intact".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Current exact-path diff evidence".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([160; 32]),
            VerificationRequirement::try_from_string(
                "Only the adjustment source changes".to_owned(),
            )?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?,
        ),
    )?
    .with_criterion_coverage(vec![runtime_id], Vec::new())?;
    let verification = VerificationRequirement::try_from_string(
        "The existing unittest executes and passes".to_owned(),
    )?;
    let spec_id = VerificationSpecId::from_bytes([161; 32]);
    let (spec, proves) = match translation {
        Some(translation) => (
            VerificationSpec::test_with_python_integer_translation(
                spec_id,
                verification,
                command,
                TestCaseSelector::All,
                minimum,
                VerificationScope::Workspace,
                translation,
            ),
            vec![runtime_id, check_id],
        ),
        None => (
            VerificationSpec::test(
                spec_id,
                verification,
                command,
                TestCaseSelector::All,
                minimum,
                VerificationScope::Workspace,
            ),
            vec![check_id],
        ),
    };
    let proof = TaskStepDefinition::new(
        CHECK_STEP,
        None,
        TaskStepOutcome::try_from_string(
            "Run the existing unittest against the change".to_owned(),
        )?,
        TaskStepRationale::try_from_string(
            "Check the existing sample on the published revision".to_owned(),
        )?,
        vec![StepDependency::new(CHANGE_STEP)],
        vec![ExpectedTaskEvidence::try_from_string(
            "Current existing unittest evidence".to_owned(),
        )?],
        spec,
    )?
    .with_criterion_coverage(Vec::new(), proves)?;
    Ok((goal, vec![change, proof]))
}

async fn assert_reopened_manifest_check(
    fixture: &Fixture,
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &a3_domain::TaskLedger,
    evidence: Option<TaskEvidenceId>,
    command: DiscoveredCommandId,
    expected: CheckResult,
) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let project = &fixture.project;
    let mut loaded = store
        .load_agent_run(project, run.id())
        .await?
        .ok_or("run lost after reopen")?;
    assert_eq!(&loaded, run);
    assert_eq!(
        loaded.state(),
        if expected.stopped() {
            AgentControllerState::Failed
        } else {
            AgentControllerState::Verify
        }
    );
    assert_eq!(
        store
            .load_current_goal_contract(project, goal.task_id())
            .await?
            .as_ref(),
        Some(goal)
    );
    let restored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("ledger lost after reopen")?;
    assert_eq!(restored.ledger(), ledger);
    let check = restored
        .ledger()
        .step(CHECK_STEP)
        .ok_or("check step lost after reopen")?;
    let current = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("published source lost after reopen")?;
    let memory = RunMemoryCheckpoint::compile(goal, ledger, &loaded, &current, Vec::new())?;
    let request = AcceptanceVerificationRequest::new(
        project.clone(),
        &loaded,
        goal.clone(),
        ledger.clone(),
        memory,
    );
    if let Some(evidence_id) = evidence {
        let state = store
            .load_verification_state(
                project,
                goal.task_id(),
                &[evidence_id],
                current.run().snapshot_id(),
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let [VerificationEvidence::Test(actual)] = state.evidence() else {
            return Err("current TestEvidence missing after reopen".into());
        };
        let [case] = actual.cases() else {
            return Err("expected the one executed existing unittest case".into());
        };
        assert_eq!(case.outcome(), TestCaseOutcome::Passed);
        assert_eq!(actual.command().command_id(), command);
        assert_eq!(actual.command().snapshot_id(), current.run().snapshot_id());
        let spec = check.definition().verification_spec();
        assert_eq!(
            spec.python_integer_translation().is_some(),
            expected.guarded()
        );
        if let Some(required) = spec.python_integer_translation() {
            let revision = current
                .publication()
                .graph()
                .files()
                .iter()
                .find(|revision| revision.path() == required.path())
                .ok_or("guarded source revision missing after reopen")?;
            assert!(
                actual
                    .command()
                    .dependencies()
                    .as_slice()
                    .contains(&EvidenceDependency::Present(revision.clone()))
            );
        }
        match expected {
            CheckResult::Pass => {
                assert_eq!(check.status(), TaskStepStatus::Completed);
                assert!(actual.python_integer_translation().is_none());
                assert_eq!(
                    DeterministicAcceptanceVerifier::new(&store)
                        .verify(
                            &request?,
                            AcceptanceVerifierTimeout::DEFAULT,
                            &ActiveControl
                        )
                        .await?,
                    AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
                );
            }
            CheckResult::GuardedPass => {
                assert_eq!(check.status(), TaskStepStatus::Completed);
                let witness = actual
                    .python_integer_translation()
                    .ok_or("guarded green test lost its source witness")?;
                assert_eq!(
                    Some(witness.requirement()),
                    spec.python_integer_translation()
                );
                assert!(
                    current
                        .publication()
                        .graph()
                        .files()
                        .contains(witness.revision())
                );
                assert_eq!(
                    witness.revision().content_hash().as_bytes(),
                    blake3::hash(CORRECT).as_bytes()
                );
                let sequence = loaded.last_event_sequence();
                let verifier = DeterministicAcceptanceVerifier::new(&store);
                let advance = VerifyAgentAcceptance::new(&verifier)
                    .execute(
                        &mut loaded,
                        &request?,
                        RunEventId::from_bytes([251; 32]),
                        at(21)?,
                        &ActiveControl,
                    )
                    .await?;
                assert_eq!(loaded.state(), AgentControllerState::Done);
                AppendRunEvent::new(&store)
                    .execute(project, sequence, &loaded, advance.event())
                    .await?;
                drop(store);
                let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                assert_eq!(
                    reopened.load_agent_run(project, run.id()).await?.as_ref(),
                    Some(&loaded)
                );
            }
            CheckResult::GuardedRejection => {
                assert_eq!(check.status(), TaskStepStatus::Ready);
                let [failed] = check.attempts() else {
                    return Err("expected one closed verification attempt".into());
                };
                assert!(matches!(
                    failed.outcome(),
                    TaskStepAttemptOutcome::VerificationFailed
                ));
                let verification = failed
                    .verification()
                    .ok_or("rejected verification lost after reopen")?;
                assert!(!verification.passed());
                assert_eq!(verification.evidence_ids(), [evidence_id]);
                assert!(failed.finished_at().is_some());
                assert!(actual.python_integer_translation().is_none());
                assert_eq!(
                    VerificationEvidenceEvaluation::evaluate(spec, &state.evidence()[0]),
                    VerificationEvidenceEvaluation::Failed(
                        VerificationEvidenceFailure::SourceProofMismatch
                    )
                );
                assert!(matches!(
                    request,
                    Err(AcceptanceVerificationRequestError::InvalidRunState)
                ));
            }
            CheckResult::Fail | CheckResult::GuardedZeroCases => {
                return Err("unverified unittest produced usable evidence".into());
            }
        }
    } else if expected == CheckResult::GuardedZeroCases {
        assert_eq!(check.status(), TaskStepStatus::Failed);
        let [failed] = check.attempts() else {
            return Err("expected one closed attempt for the zero-case test".into());
        };
        assert!(matches!(
            failed.outcome(),
            TaskStepAttemptOutcome::Failed { .. }
        ));
        assert!(failed.finished_at().is_some());
        assert!(failed.verification().is_none());
        assert!(matches!(
            request,
            Err(AcceptanceVerificationRequestError::InvalidRunState)
        ));
    } else {
        assert_eq!(check.status(), TaskStepStatus::InProgress);
        assert!(
            check
                .attempts()
                .iter()
                .all(|attempt| attempt.verification().is_none())
        );
        assert!(matches!(
            request,
            Err(AcceptanceVerificationRequestError::IncompleteLedger)
        ));
    }
    Ok(())
}
