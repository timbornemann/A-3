//! Native preparation and materialization through the actual Ollama encoder.
//! The decoded reply is an explicit fixture. A live Plan needs a durable pre-Run reservation
//! and settlement in the same transaction as Ready Ledger creation.

use super::*;
use a3_application::{
    AgentSourceReader, GoalIntakeStoreVersion, ModelProvider, ModelWireThinking, PlanPhaseOutcome,
    PlanPhaseState, PlanStepOffer, PreparedPlanPhase,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, GoalIntakeFinding,
    GoalIntakeFindingKind, GoalIntakeReview, GoalIntakeReviewBatchProposal, PendingGoalIntake,
    StepDependency,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use a3_workspace::WorkspaceAgentSourceReader;
use serde_json::json;

const TAG: &str = "qwen38-4k:latest";

#[test]
fn promoted_average_plan_fits_real_ollama_4k_encoder_and_materializes_on_fixture_reply()
-> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = wrong_fixture()?;
            let worktree = fixture.root.join("worktree");
            let before = inventory(&worktree)?;
            let (store, _, _, published) = index_fixture(&fixture).await?;
            let project = &fixture.project;
            let catalog = DiscoverProjectCommands.execute(project.worktree().id(), &published)?;
            let mut tests = catalog
                .commands()
                .iter()
                .filter(|command| command.kind() == DiscoveredCommandKind::Test);
            let command = tests.next().ok_or("no indexed unittest command")?;
            assert!(tests.next().is_none());
            assert_eq!(command.executable().as_str(), "python");
            assert_eq!(
                command
                    .arguments()
                    .iter()
                    .map(|arg| arg.as_str())
                    .collect::<Vec<_>>(),
                ["-B", "-m", "unittest", "discover", "-s", "tests"],
            );
            let goal = repair_goal(command.id())?;
            let task = goal.task_id();
            promote_fixture_goal(store.as_ref(), project, &goal).await?;
            let goal = store
                .load_current_goal_contract(project, task)
                .await?
                .ok_or("promoted Goal missing")?;
            assert!(store.load_task_ledger(project, task).await?.is_none());

            let path = RepositoryPath::try_from_bytes(b"average.py".to_vec())?;
            let revision = published
                .publication()
                .graph()
                .files()
                .iter()
                .find(|file| file.path() == &path)
                .ok_or("actual average.py revision missing")?;
            let page = WorkspaceAgentSourceReader
                .read_page(
                    project,
                    revision,
                    &AgentFileInspection::new(
                        path.clone(),
                        AgentFileStartLine::new(1)?,
                        AgentFileLineCount::new(8)?,
                    ),
                    &ActiveControl,
                )
                .await?;
            assert_eq!(page.text().as_bytes(), WRONG_SUBJECT);
            assert!(!page.truncated());

            // The same settings as the existing explicit P4 probe. This offline test only
            // uses the encoder, with an unreachable endpoint and no capability-probe call.
            let provider = OllamaModelProvider::for_projected_wire(
                OllamaEndpoint::parse("http://127.0.0.1:1")?,
                Arc::new(LocalOnlyOllamaEndpointPolicy),
            )?;
            let profile = projection_profile(&provider)?;
            let state = PlanPhaseState::new(project, &goal, &published, &profile, Some(&catalog))?;
            let criterion = goal
                .draft()
                .acceptance_criteria()
                .first()
                .ok_or("criterion missing")?;
            let addressed = TaskStepId::from_bytes([199; 32]);
            let proved = TaskStepId::from_bytes([200; 32]);
            let diff = VerificationSpec::diff_invariant(
                VerificationSpecId::from_bytes([199; 32]),
                VerificationRequirement::try_from_string(
                    "Check exactly the average.py path".to_owned(),
                )?,
                DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?,
            );
            let test = VerificationSpec::test(
                VerificationSpecId::from_bytes([200; 32]),
                VerificationRequirement::try_from_string(
                    "Run the two existing average tests".to_owned(),
                )?,
                command.id(),
                TestCaseSelector::All,
                MinimumTestCaseCount::new(2)?,
                VerificationScope::Workspace,
            );
            let prepared = PreparedPlanPhase::new(
                state,
                vec![
                    PlanStepOffer::new(addressed, vec![criterion.id()], vec![], diff.clone()),
                    PlanStepOffer::new(proved, vec![], vec![criterion.id()], test.clone()),
                ],
                Some(&page),
            )?;
            let (wire, budget) = prepared.project_wire(&provider)?;
            assert_eq!(wire.context_tokens(), 4096);
            assert_eq!(wire.output_tokens(), 1024);
            assert_eq!(wire.thinking(), ModelWireThinking::Disabled);
            assert!(wire.format_bytes() > 0);
            assert!(budget.prompt_tokens() > wire.request_bytes());
            assert!(budget.prompt_tokens() + budget.output_tokens() <= 4096);

            let schema = prepared
                .request()
                .structured_output()
                .ok_or("Plan schema missing")?;
            let job = schema.value()["oneOf"][0]["properties"]["job"]["const"]
                .as_str()
                .ok_or("Plan job alias missing")?;
            // Deterministic decoder/ledger fixture, never attributed to an Ollama generation.
            let reply = serde_json::to_vec(&json!({
                "version":1,"job":job,
                "steps":[
                    {"offer":"P1","outcome":"Preserve fractional averages and the existing ValueError path"},
                    {"offer":"P2","outcome":"Verify both unchanged average tests","after":["P1"]},
                ],
            }))?;
            let PlanPhaseOutcome::Steps(plan) = prepared.complete(state, &reply)? else {
                return Err("expected two Plan fixture steps".into());
            };
            let steps = plan
                .ledger()
                .steps()
                .map(|step| step.definition().clone())
                .collect::<Vec<_>>();
            assert_eq!(steps.len(), 2);
            assert_eq!(steps[0].id(), addressed);
            assert_eq!(steps[0].addressed_criteria(), &[criterion.id()]);
            assert!(steps[0].acceptance_criteria().is_empty());
            assert_eq!(steps[0].verification_spec(), &diff);
            assert_eq!(steps[1].id(), proved);
            assert_eq!(steps[1].acceptance_criteria(), &[criterion.id()]);
            assert_eq!(steps[1].verification_spec(), &test);
            assert_eq!(steps[1].dependencies(), &[StepDependency::new(addressed)]);

            let materialized = MaterializeOriginalTask::new(store.as_ref(), store.as_ref())
                .execute(project, &goal, steps)
                .await?;
            assert_eq!(materialized.preview().next_step, Some(addressed));
            assert!(materialized.preview().unplanned_criteria.is_empty());
            let expected = materialized.stored_ledger();
            drop(store);

            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let actual = reopened
                .load_task_ledger(project, task)
                .await?
                .ok_or("Ledger missing after reopen")?;
            assert_eq!(actual.version(), expected.version());
            assert_eq!(actual.ledger(), expected.ledger());
            assert_eq!(inventory(&worktree)?, before);
            drop(reopened);
            {
                let database = fixture.candidate_database().await?;
                let connection = database.connection_for_operation().await?;
                let mut rows = connection
                    .query("SELECT COUNT(*) FROM agent_runs", ())
                    .await?;
                let row = rows.next().await?.ok_or("missing Run count")?;
                assert_eq!(row.get::<i64>(0)?, 0);
            }
            fixture.cleanup()
        })
    })
}

async fn promote_fixture_goal(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    goal: &GoalContract,
) -> TestResult {
    let original = goal.original().ok_or("original missing")?;
    let pending = PendingGoalIntake::new(original.clone(), goal.created_at())?;
    let version = store.capture_goal_intake(project, &pending).await?;
    assert_eq!(version, GoalIntakeStoreVersion::INITIAL);
    let criterion = goal
        .draft()
        .acceptance_criteria()
        .first()
        .ok_or("criterion missing")?;
    let origin = criterion.origin().ok_or("criterion provenance missing")?;
    let constraint = goal
        .draft()
        .constraints()
        .first()
        .ok_or("constraint missing")?;
    let proposal = GoalIntakeReviewBatchProposal::new(
        original.range(0, original.as_bytes().len())?,
        vec![
            GoalIntakeFinding::new(
                GoalIntakeFindingKind::Outcome {
                    requirement: criterion.requirement(),
                    evidence_requirement: origin.evidence_requirement().clone(),
                },
                origin.original_range(),
                criterion.statement().as_str().to_owned(),
            )?,
            GoalIntakeFinding::new(
                GoalIntakeFindingKind::Constraint,
                constraint.origin().ok_or("constraint provenance missing")?,
                constraint.as_str().to_owned(),
            )?,
        ],
        vec![],
    )?;
    let review = GoalIntakeReview::begin(&pending, proposal)?;
    store
        .append_goal_intake_review(project, goal.task_id(), version, None, &review)
        .await?;
    store
        .promote_goal_intake(project, goal.task_id(), version, goal)
        .await?;
    Ok(())
}

fn projection_profile(provider: &OllamaModelProvider) -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        provider.provider_id().clone(),
        ModelId::try_from_string(TAG.to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(4096)?,
            ModelOutputLimit::new(1024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}
