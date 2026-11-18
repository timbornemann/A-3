//! The real turn preflight must stop before compiler, provider or read-tool ownership.
use super::*;
use crate::{AdmitAgentGoalExecution, AgentGoalAdmissionFailure};
use a3_domain::{
    CriterionEvidenceRequirement, CriterionOrigin, GoalConstraint, GoalOriginalCoverage,
    GoalOriginalRequest, NonGoal, UserDecision,
};

#[derive(Debug, Clone, Copy)]
enum Case {
    Legacy,
    CompleteSource,
    OriginalGap,
    OnlyUnboundConstraint,
    OrphanOrigin,
}

fn admission_goal(base: &GoalContract, case: Case) -> Result<GoalContract, Box<dyn Error>> {
    if matches!(case, Case::Legacy) {
        return Ok(base.clone());
    }
    let lines = [
        "R: inspect controller\n",
        "C: preserve files\n",
        "N: avoid publishing\n",
        "D: use Python\n",
    ];
    let mut bytes = lines.concat().into_bytes();
    if matches!(case, Case::OriginalGap) {
        bytes.extend_from_slice(b"Tail must remain open\n");
    }
    let original = GoalOriginalRequest::try_from_bytes(base.reference(), bytes)?;
    let mut offset = 0;
    let mut ranges = Vec::new();
    for line in lines {
        ranges.push(original.range(offset, offset + line.len())?);
        offset += line.len();
    }
    let criterion_range = if matches!(case, Case::OnlyUnboundConstraint) {
        original.range(0, original.as_bytes().len())?
    } else {
        ranges[0]
    };
    let criterion =
        base.draft().acceptance_criteria()[0]
            .clone()
            .with_origin(CriterionOrigin::new(
                criterion_range,
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
            ));
    let constraint = GoalConstraint::try_from_string("preserve files".to_owned())?;
    let constraint = if matches!(case, Case::OnlyUnboundConstraint) {
        constraint
    } else {
        constraint.with_origin(ranges[1])
    };
    let draft = GoalContractDraft::new(
        base.draft().objective().clone(),
        vec![criterion],
        vec![constraint],
        vec![NonGoal::try_from_string("avoid publishing".to_owned())?.with_origin(ranges[2])],
        vec![UserDecision::try_from_string("use Python".to_owned())?.with_origin(ranges[3])],
        base.draft().success_verification().clone(),
    )?;
    let result = GoalContract::initial(base.task_id(), draft, base.created_at());
    if matches!(case, Case::OrphanOrigin) {
        Ok(result)
    } else {
        Ok(result.with_original(original)?)
    }
}

#[derive(Debug)]
struct AdmissionCompiler {
    template: Mutex<Option<CompiledAgentContext>>,
    calls: AtomicUsize,
}

impl AgentContextCompiler for AdmissionCompiler {
    fn compile<'a>(
        &'a self,
        _input: &'a AgentContextCompileInput,
        _control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self
            .template
            .lock()
            .map_err(|_| ContextCompileFailure::InvalidPack)
            .and_then(|mut template| template.take().ok_or(ContextCompileFailure::InvalidPack));
        Box::pin(async move { result })
    }
}

fn run_admission_case(
    case: Case,
    expected: Option<AgentGoalAdmissionFailure>,
) -> Result<(), Box<dyn Error>> {
    let fixture = turn_fixture_with_command(
        vec![provider_response(
            r#"{"schema_version":6,"action":{"kind":"search","query":"controller","limit":5}}"#,
        )?],
        Some(a3_domain::DiscoveredCommandId::from_bytes([90; 32])),
    )?;
    let original_run = fixture.run.clone();
    let goal = admission_goal(fixture.input.goal_contract(), case)?;
    assert_eq!(
        AdmitAgentGoalExecution.execute(&goal),
        expected.map_or(Ok(()), Err)
    );
    match case {
        Case::OriginalGap => {
            let coverage = GoalOriginalCoverage::derive_from(&goal)?;
            assert!(!coverage.gaps().is_empty());
            assert!(coverage.unbound_entries().is_empty());
        }
        Case::OnlyUnboundConstraint => {
            let coverage = GoalOriginalCoverage::derive_from(&goal)?;
            assert!(coverage.gaps().is_empty());
            assert_eq!(coverage.unbound_entries().len(), 1);
        }
        Case::CompleteSource => assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete()),
        Case::Legacy | Case::OrphanOrigin => {}
    }
    let ledger = if matches!(case, Case::CompleteSource) {
        let definition = fixture
            .input
            .task_ledger()
            .step(fixture.input.current_step_id())
            .ok_or("step")?
            .definition()
            .clone()
            .with_criterion_coverage(
                vec![goal.draft().acceptance_criteria()[0].id()],
                Vec::new(),
            )?;
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![definition],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        ledger.start_step(
            fixture.input.current_step_id(),
            fixture.run.id(),
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        ledger
    } else {
        fixture.input.task_ledger().clone()
    };
    let input = AgentContextCompileInput::new(
        fixture.input.project().clone(),
        goal,
        ledger,
        fixture.input.current_step_id(),
        fixture.profile.clone(),
        None,
        Vec::new(),
        Vec::new(),
    )?;
    let compiler = AdmissionCompiler {
        template: Mutex::new(Some(fixture.compiled)),
        calls: AtomicUsize::new(0),
    };
    let provider = RecordingReplanProvider {
        inner: ScriptedProvider {
            provider_id: fixture.profile.provider_id().clone(),
            responses: Mutex::new(fixture.responses),
        },
        requests: Mutex::new(Vec::new()),
    };
    let tools = CountingReadTools {
        calls: AtomicUsize::new(0),
    };
    let recovery = TestRecoveryStore::default();
    let outcome = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery).execute(
            &fixture.run,
            &input,
            timestamp(5)?,
            &TestControl,
        ),
    );
    let count = if expected.is_some() { 0 } else { 1 };
    if expected.is_some() {
        assert!(matches!(
            outcome,
            Err(ExecuteAgentTurnFailure::InputMismatch)
        ));
    } else {
        assert!(matches!(outcome?, AgentTurnOutcome::Executed(_)));
    }
    assert_eq!(compiler.calls.load(Ordering::SeqCst), count);
    assert_eq!(provider.requests.lock().map_err(|_| "poison")?.len(), count);
    assert_eq!(tools.calls.load(Ordering::SeqCst), count);
    assert_eq!(recovery.begins.load(Ordering::SeqCst), count);
    assert_eq!(
        fixture.run, original_run,
        "admission never mutates the run or its budgets"
    );
    Ok(())
}

#[test]
fn original_gap_stops_before_compiler_provider_and_tools() -> Result<(), Box<dyn Error>> {
    run_admission_case(
        Case::OriginalGap,
        Some(AgentGoalAdmissionFailure::IntakeIncomplete),
    )
}

#[test]
fn only_unbound_constraint_stops_even_with_full_byte_coverage() -> Result<(), Box<dyn Error>> {
    run_admission_case(
        Case::OnlyUnboundConstraint,
        Some(AgentGoalAdmissionFailure::IntakeIncomplete),
    )
}

#[test]
fn orphan_origin_cannot_use_the_legacy_admission_path() -> Result<(), Box<dyn Error>> {
    run_admission_case(
        Case::OrphanOrigin,
        Some(AgentGoalAdmissionFailure::InvalidOriginalBinding),
    )
}

#[test]
fn fully_annotated_source_keeps_the_existing_read_path() -> Result<(), Box<dyn Error>> {
    run_admission_case(Case::CompleteSource, None)
}

#[test]
fn clean_legacy_keeps_the_existing_read_path() -> Result<(), Box<dyn Error>> {
    run_admission_case(Case::Legacy, None)
}

#[test]
fn invalid_source_step_roles_stop_before_context_model_and_tools() -> Result<(), Box<dyn Error>> {
    for mode in 0..4 {
        let fixture = turn_fixture_with_command(
            vec![provider_response(
                r#"{"schema_version":6,"action":{"kind":"search","query":"controller","limit":5}}"#,
            )?],
            Some(a3_domain::DiscoveredCommandId::from_bytes([90; 32])),
        )?;
        let goal = admission_goal(fixture.input.goal_contract(), Case::CompleteSource)?;
        let criterion = goal.draft().acceptance_criteria()[0].id();
        let base = fixture
            .input
            .task_ledger()
            .step(fixture.input.current_step_id())
            .ok_or("step")?
            .definition()
            .clone();
        let definition = match mode {
            0 => base,
            1 => base.with_criterion_coverage(
                vec![a3_domain::AcceptanceCriterionId::from_bytes([99; 32])],
                Vec::new(),
            )?,
            2 => step_definition_with_command(fixture.input.current_step_id(), None)?
                .with_criterion_coverage(vec![criterion], Vec::new())?,
            _ => base.with_acceptance_criteria(vec![criterion])?,
        };
        assert!(
            crate::AdmitSourceTaskStep
                .execute(&goal, &definition)
                .is_err()
        );
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![definition],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        ledger.start_step(
            fixture.input.current_step_id(),
            fixture.run.id(),
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        let input = AgentContextCompileInput::new(
            fixture.input.project().clone(),
            goal,
            ledger,
            fixture.input.current_step_id(),
            fixture.profile.clone(),
            None,
            Vec::new(),
            Vec::new(),
        )?;
        let compiler = AdmissionCompiler {
            template: Mutex::new(Some(fixture.compiled)),
            calls: AtomicUsize::new(0),
        };
        let provider = RecordingReplanProvider {
            inner: ScriptedProvider {
                provider_id: fixture.profile.provider_id().clone(),
                responses: Mutex::new(fixture.responses),
            },
            requests: Mutex::new(Vec::new()),
        };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();
        let result = futures::executor::block_on(
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery).execute(
                &fixture.run,
                &input,
                timestamp(5)?,
                &TestControl,
            ),
        );
        assert!(matches!(
            result,
            Err(ExecuteAgentTurnFailure::InputMismatch)
        ));
        assert_eq!(compiler.calls.load(Ordering::SeqCst), 0);
        assert!(provider.requests.lock().map_err(|_| "poison")?.is_empty());
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
    }
    Ok(())
}
