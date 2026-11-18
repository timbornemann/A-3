//! Native storage contract for HTTP evidence, with controlled observations and no service I/O.

#[path = "plan22_loopback_controller_tests.rs"]
mod controller_tests;
#[path = "plan22_loopback_startup_storage_tests.rs"]
mod startup_tests;

use super::*;
use a3_application::{
    AdvanceAgentController, AgentMutationResultRecord, AgentRecoveryStoreFailure,
    ContextToolResultDigest, EvaluateActionPolicy, EvaluateStepVerification, MachineEffectScope,
    PersistPolicyEvaluation, PolicyEvaluationContext, PreparedLoopbackVerification,
    RunEventPageLimit, VerificationEvidenceStoreFailure,
};
use a3_domain::{
    ContentHash, LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck,
    LoopbackHttpEvidence, LoopbackHttpEvidenceContext, LoopbackHttpObservation,
    LoopbackHttpProcess, LoopbackHttpRecipe, LoopbackMethod, LoopbackProbe, LoopbackStart,
    MutationActionFingerprint, PolicyAction, PolicyEvaluationTiming, PolicyResourceId,
    ProcessArgument, ProcessOutputLimit, ProcessSpec, ProcessTimeout, RunEventCode, RunEventKind,
    RunEventOutcome, RunEventPayload, RunEventSubject,
};
use std::num::NonZeroU16;

const GOAL: &[u8] = b"Serve Ready followed by LF at /health, and refuse POST with Allow: GET.";
const STARTUP_GOAL: &[u8] = b"Serve Ready followed by LF at /health, and refuse POST with Allow: GET. Check explicit and default startup and error exits for invalid arguments and a nonbindable port.";

#[test]
fn settled_http_receipt_requires_exact_authority_and_survives_reopen() -> TestResult {
    settled_http_receipt(recipe()?)
}

#[test]
fn single_start_receipt_cannot_complete_an_aggregate_recipe_even_with_exact_authority() -> TestResult
{
    let base = recipe()?;
    let aggregate = LoopbackHttpRecipe::new(
        base.script().clone(),
        LoopbackStart::Explicit,
        NonZeroU16::try_from(18_765_u16)?,
        base.checks().to_vec(),
    )?
    .with_startup_coverage(0)?;
    settled_http_receipt(aggregate)
}

fn settled_http_receipt(recipe: LoopbackHttpRecipe) -> TestResult {
    crate::run_native_libsql_test(async move {
        let mut fixture = Fixture::new()?;
        let worktree = fixture.root.join("worktree");
        initialize_git(&worktree)?;
        fs::write(
            worktree.join("server.py"),
            b"# codec fixture; never executed\n",
        )?;
        fs::write(
            worktree.join("other.py"),
            b"# unrelated script; never executed\n",
        )?;
        fixture.project = RepositoryInspector::new().inspect(&worktree)?;
        let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
        let snapshot = published.run().snapshot_id();
        let aggregate = recipe.default_start_check_index().is_some();
        let script = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|file| file.path() == recipe.script())
            .ok_or("fixture script not published")?
            .clone();
        let goal = goal(recipe.clone())?;
        let spec = VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string("Check the exact HTTP contract".to_owned())?,
            recipe.clone(),
        );
        let step = TaskStepId::from_bytes([STEP; 32]);
        let definition = TaskStepDefinition::new(
            step,
            None,
            TaskStepOutcome::try_from_string("Verify the local HTTP service".to_owned())?,
            TaskStepRationale::try_from_string("Check the original runtime outcome".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "Owned HTTP observations".to_owned(),
            )?],
            spec.clone(),
        )?
        .with_criterion_coverage(Vec::new(), vec![goal.draft().acceptance_criteria()[0].id()])?;
        let (mut run, _, unplanned) = start_fixture_step(
            &fixture,
            &store,
            &goal,
            &profile()?,
            snapshot,
            vec![definition],
        )
        .await?;
        assert!(unplanned.is_empty());
        let start = start_spec(&fixture, &run, step, &recipe)?;
        let start_action = start.policy_action();
        let probe = |index| {
            PreparedLoopbackVerification::bound_probe_policy_action(
                fixture.project.worktree().id(),
                step,
                start.specification_id(),
                snapshot,
                &recipe,
                &script,
                index,
            )
            .ok_or("missing exact probe action")
        };
        // In the aggregate case, bind the two old observations to the aggregate ID
        // and exact explicit start. Only the required additional starts are missing.
        let get = probe(0)?;
        let post = probe(1)?;
        let actions = [&start_action, &get, &post];
        let mut decisions = Vec::new();
        for (index, action) in actions.into_iter().enumerate() {
            let sequence = u8::try_from(index)?;
            decisions.push(approve(&fixture, &store, &mut run, action, 10 + 5 * sequence).await?);
        }
        let context = LoopbackHttpEvidenceContext {
            verification_run_id: VerificationRunId::from_bytes([71; 32]),
            spec_id: spec.id(),
            run_id: run.id(),
            tool_run_id: ToolRunId::from_bytes([72; 32]),
            step_id: step,
            snapshot_id: snapshot,
        };
        let process = LoopbackHttpProcess::from_spec(&start)?;
        let get_observation = LoopbackHttpObservation::new(
            decisions[1],
            200,
            hash(b"Ready\n"),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?;
        let post_observation = LoopbackHttpObservation::new(
            decisions[2],
            405,
            hash(b""),
            0,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?;
        let receipt = LoopbackHttpEvidence::new(
            context,
            recipe.id(),
            script.clone(),
            process,
            decisions[0],
            vec![get_observation, post_observation],
        )?;
        let evidence = VerificationEvidence::LoopbackHttp(receipt.clone());
        let scope = MachineEffectScope::new(
            AgentMutationKind::MachineProcess,
            step,
            process.specification(),
            "python".to_owned(),
            Some(script.content_hash()),
            None,
        )?;
        let attempt = store
            .begin_machine_mutation_attempt(
                &fixture.project,
                run.id(),
                snapshot,
                context.tool_run_id,
                MutationActionFingerprint::from_bytes(*start_action.fingerprint().as_bytes()),
                &scope,
                at(40)?,
            )
            .await?;
        assert_rejected(&fixture, &store, &evidence).await;
        let before_completion = run.clone();
        let expected = run.last_event_sequence();
        let event = run.record(
            RunEventId::from_bytes([73; 32]),
            RunEventKind::ToolAction,
            RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
            snapshot,
            Some(RunEventSubject::Tool(context.tool_run_id)),
            at(41)?,
        )?;
        let rejected_receipt = if aggregate {
            assert_eq!(receipt.recipe_id(), recipe.id());
            assert!(!receipt.matches(&recipe));
            receipt.clone()
        } else {
            let wrong_recipe = LoopbackHttpEvidence::new(
                context,
                PolicyResourceId::from_bytes([79; 32]),
                script.clone(),
                process,
                decisions[0],
                vec![get_observation, post_observation],
            )?;
            assert_ne!(wrong_recipe.recipe_id(), recipe.id());
            wrong_recipe
        };
        assert_eq!(
            store
                .complete_loopback_mutation(
                    &fixture.project,
                    expected,
                    &run,
                    &event,
                    attempt.tool_attempt().attempt(),
                    &rejected_receipt,
                    Duration::from_secs(10),
                    &ActiveControl,
                )
                .await,
            Err(AgentRecoveryStoreFailure::InvalidStoredData),
        );
        assert_eq!(
            store.load_agent_run(&fixture.project, run.id()).await?,
            Some(before_completion.clone())
        );
        let later = store
            .load_run_events(
                &fixture.project,
                run.id(),
                Some(expected),
                RunEventPageLimit::new(1)?,
            )
            .await?;
        assert!(later.events().is_empty());
        let attempts = store
            .load_agent_mutation_attempts(&fixture.project, run.id())
            .await?;
        assert_eq!(attempts, [attempt]);
        assert_eq!(
            attempt.tool_attempt().status(),
            AgentToolAttemptStatus::InFlight
        );
        assert!(attempt.disposition().requires_reconciliation());
        assert!(matches!(
            store
                .load_verification_state(
                    &fixture.project,
                    goal.task_id(),
                    &[rejected_receipt.id()],
                    snapshot,
                    Duration::from_secs(10),
                    &ActiveControl,
                )
                .await,
            Err(VerificationEvidenceStoreFailure::InvalidStoredData)
        ));
        if aggregate {
            drop((compiler, refresh, store));
            let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            assert_eq!(
                reopened.load_agent_run(&fixture.project, run.id()).await?,
                Some(before_completion)
            );
            assert_eq!(
                reopened
                    .load_agent_mutation_attempts(&fixture.project, run.id())
                    .await?,
                [attempt]
            );
            drop(reopened);
            assert_no_settlement_rows(&fixture, &rejected_receipt).await?;
            return fixture.cleanup();
        }
        let settled = store
            .complete_loopback_mutation(
                &fixture.project,
                expected,
                &run,
                &event,
                attempt.tool_attempt().attempt(),
                &receipt,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        assert_eq!(
            settled.tool_attempt().status(),
            AgentToolAttemptStatus::Succeeded
        );
        for wrong in [
            LoopbackHttpEvidence::new(
                LoopbackHttpEvidenceContext {
                    step_id: TaskStepId::from_bytes([74; 32]),
                    ..context
                },
                recipe.id(),
                script.clone(),
                process,
                decisions[0],
                vec![get_observation, post_observation],
            )?,
            LoopbackHttpEvidence::new(
                LoopbackHttpEvidenceContext {
                    snapshot_id: SnapshotId::from_bytes([75; 32]),
                    ..context
                },
                recipe.id(),
                script.clone(),
                process,
                decisions[0],
                vec![get_observation, post_observation],
            )?,
            LoopbackHttpEvidence::new(
                context,
                recipe.id(),
                script,
                process,
                decisions[0],
                vec![post_observation, get_observation],
            )?,
        ] {
            assert_rejected(&fixture, &store, &VerificationEvidence::LoopbackHttp(wrong)).await;
        }
        reject_settled_other_script(&fixture, &store, &mut run, &recipe, &receipt).await?;
        drop((compiler, refresh, store));

        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let state = reopened
            .load_verification_state(
                &fixture.project,
                goal.task_id(),
                &[evidence.id()],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        assert_eq!(state.evidence(), std::slice::from_ref(&evidence));
        let stored = reopened
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        let recovered_step = stored
            .ledger()
            .step(step)
            .ok_or("step missing")?
            .definition();
        let recovered_spec = recovered_step.verification_spec();
        assert_eq!(recovered_spec, &spec);
        let recovered_goal = reopened
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("goal missing")?;
        recovered_goal.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("criterion origin missing")?
            .evidence_requirement()
            .validate_spec(recovered_spec)?;
        let verification = EvaluateStepVerification.execute_for_step(
            StepVerificationId::from_bytes([76; 32]),
            recovered_step,
            run.id(),
            &state.evidence()[0],
            state.published_index(),
            TaskLedgerTimestamp::from_unix_millis(80)?,
        )?;
        assert!(verification.passed());
        let mut next_ledger = stored.ledger().clone();
        next_ledger.begin_step_verification(
            step,
            run.id(),
            None,
            vec![evidence.id()],
            verification.verified_at(),
        )?;
        next_ledger.finish_step_verification(step, verification)?;
        let mut next_run = run.clone();
        let verify_event = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::TurnNeedsVerification,
            RunEventId::from_bytes([81; 32]),
            snapshot,
            at(80)?,
            false,
        )?;
        fs::write(worktree.join("foreign.txt"), b"new independent revision\n")?;
        let (publisher, _, _, newer) = index_fixture(&fixture).await?;
        assert_ne!(newer.run().snapshot_id(), snapshot);
        assert_eq!(
            reopened
                .commit_current_ledger_action(
                    &fixture.project,
                    stored.version(),
                    run.last_event_sequence(),
                    &next_ledger,
                    &next_run,
                    verify_event.event(),
                )
                .await,
            Err(AgentActionStoreFailure::SnapshotConflict)
        );
        assert_eq!(
            reopened
                .load_task_ledger(&fixture.project, goal.task_id())
                .await?,
            Some(stored)
        );
        assert_eq!(
            reopened.load_agent_run(&fixture.project, run.id()).await?,
            Some(run)
        );
        drop(publisher);
        drop(reopened);
        fixture.cleanup()
    })
}

async fn assert_no_settlement_rows(
    fixture: &Fixture,
    rejected: &LoopbackHttpEvidence,
) -> TestResult {
    let layout = fixture.layout.prepare_project(fixture.project.worktree())?;
    let reopened =
        crate::KnowledgeDatabase::open_plan22_candidate(&layout, &fixture.project).await?;
    let mut rows = reopened
        .connection()
        .query(
            "SELECT EXISTS(SELECT 1 FROM verification_evidence WHERE evidence_id=?1),
                EXISTS(SELECT 1 FROM tool_runs WHERE tool_run_id=?2)",
            libsql::params![
                rejected.id().as_bytes().to_vec(),
                rejected.context().tool_run_id.as_bytes().to_vec(),
            ],
        )
        .await?;
    let row = rows.next().await?.ok_or("missing settlement counts")?;
    assert_eq!((row.get::<i64>(0)?, row.get::<i64>(1)?), (0, 0));
    Ok(())
}

async fn reject_settled_other_script(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    recipe: &LoopbackHttpRecipe,
    valid: &LoopbackHttpEvidence,
) -> TestResult {
    let context = LoopbackHttpEvidenceContext {
        verification_run_id: VerificationRunId::from_bytes([78; 32]),
        tool_run_id: ToolRunId::from_bytes([79; 32]),
        ..valid.context()
    };
    let worktree = fixture.project.worktree().id();
    let alternate = LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"other.py".to_vec())?,
        recipe.start(),
        recipe.port(),
        recipe.checks().to_vec(),
    )?;
    let start = alternate.process_spec(
        run.id(),
        worktree,
        context.step_id,
        valid.process().settings(),
    )?;
    let process = LoopbackHttpProcess::from_spec(&start)?;
    assert_ne!(process.specification(), valid.process_specification());
    let start_action = start.policy_action();
    let start_decision = approve(fixture, store, run, &start_action, 45).await?;
    let mut observations = Vec::new();
    for (index, actual) in valid.observations().iter().enumerate() {
        // Match the target recipe's exact probes, while binding their grants to the wrong argv.
        let probe = PreparedLoopbackVerification::bound_probe_policy_action(
            worktree,
            context.step_id,
            process.specification(),
            context.snapshot_id,
            recipe,
            valid.script(),
            index,
        )
        .ok_or("missing alternate probe action")?;
        let decision = approve(fixture, store, run, &probe, 50 + 5 * u8::try_from(index)?).await?;
        observations.push(LoopbackHttpObservation::new(
            decision,
            actual.status(),
            actual.body_hash(),
            actual.body_bytes(),
            actual.headers(),
        )?);
    }
    let wrong = LoopbackHttpEvidence::new(
        context,
        recipe.id(),
        valid.script().clone(),
        process,
        start_decision,
        observations,
    )?;
    assert!(wrong.matches(recipe));
    let scope = MachineEffectScope::new(
        AgentMutationKind::MachineProcess,
        context.step_id,
        process.specification(),
        "python".to_owned(),
        None,
        None,
    )?;
    let attempt = store
        .begin_machine_mutation_attempt(
            &fixture.project,
            run.id(),
            context.snapshot_id,
            context.tool_run_id,
            MutationActionFingerprint::from_bytes(*start_action.fingerprint().as_bytes()),
            &scope,
            at(60)?,
        )
        .await?;
    let expected = run.last_event_sequence();
    let event = run.record(
        RunEventId::from_bytes([80; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
        context.snapshot_id,
        Some(RunEventSubject::Tool(context.tool_run_id)),
        at(61)?,
    )?;
    let settled = store
        .complete_agent_mutation_attempt(
            &fixture.project,
            expected,
            run,
            &event,
            context.tool_run_id,
            attempt.tool_attempt().attempt(),
            AgentMutationResultRecord::new(
                ContextToolResultDigest::from_bytes(*wrong.id().as_bytes()),
                false,
                6,
            ),
        )
        .await?;
    assert_eq!(
        settled.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );
    assert_rejected(fixture, store, &VerificationEvidence::LoopbackHttp(wrong)).await;
    Ok(())
}

async fn assert_rejected(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    evidence: &VerificationEvidence,
) {
    assert!(matches!(
        store
            .append_verification_evidence(
                &fixture.project,
                evidence,
                Duration::from_secs(10),
                &ActiveControl
            )
            .await,
        Err(VerificationEvidenceStoreFailure::InvalidStoredData)
    ));
}

fn recipe() -> TestResult<LoopbackHttpRecipe> {
    let path = "/health".to_owned();
    Ok(LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Default,
        NonZeroU16::try_from(8765_u16)?,
        vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, path.clone())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: hash(b"Ready\n"),
                    bytes: 6,
                },
                LoopbackHeaderChecks {
                    plain_utf8: true,
                    allow_get: false,
                },
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Post, path)?,
                405,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks {
                    plain_utf8: false,
                    allow_get: true,
                },
            )?,
        ],
    )?)
}

fn goal(recipe: LoopbackHttpRecipe) -> TestResult<GoalContract> {
    let task = TaskId::from_bytes([70; 32]);
    let original_bytes = if recipe.default_start_check_index().is_some() {
        STARTUP_GOAL
    } else {
        GOAL
    };
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        original_bytes.to_vec(),
    )?;
    let origin = CriterionOrigin::new(
        original.range(0, original_bytes.len())?,
        CriterionEvidenceRequirement::LoopbackHttp(recipe),
    );
    let criterion = AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([77; 32]),
        AcceptanceCriterionStatement::try_from_string(
            std::str::from_utf8(original_bytes)?.to_owned(),
        )?,
    )
    .with_origin(origin);
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Verify the HTTP contract".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Exact current local responses".to_owned())?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
            .with_original(original)?,
    )
}

fn start_spec(
    fixture: &Fixture,
    run: &AgentRun,
    step: TaskStepId,
    recipe: &LoopbackHttpRecipe,
) -> TestResult<ProcessSpec> {
    let limit = ProcessOutputLimit::new(1024)?;
    Ok(recipe.process_spec(
        run.id(),
        fixture.project.worktree().id(),
        step,
        a3_domain::LoopbackProcessSettings {
            timeout: ProcessTimeout::from_millis(5000)?,
            stdout_limit: limit,
            stderr_limit: limit,
            system_root: false,
        },
    )?)
}

async fn approve(
    fixture: &Fixture,
    store: &LibsqlKnowledgeStore,
    run: &mut AgentRun,
    action: &PolicyAction,
    seed: u8,
) -> TestResult<PolicyDecisionId> {
    let engine =
        EvaluateActionPolicy::new().with_permissions(store.load_agent_permissions().await?);
    let snapshot = run.current_snapshot_id();
    let policy = WorkspacePolicy::unrestricted();
    let context = |n| -> TestResult<PolicyEvaluationContext> {
        let when = at(u64::from(n))?;
        Ok(PolicyEvaluationContext::new(
            PolicyDecisionId::from_bytes([n; 32]),
            ApprovalRequestId::from_bytes([n; 32]),
            RunEventId::from_bytes([n; 32]),
            snapshot,
            PolicyEvaluationTiming::new(when, when)?,
            at(100)?,
        ))
    };
    let expected = run.last_event_sequence();
    let waiting = engine.execute(run, action, &policy, None, context(seed)?)?;
    PersistPolicyEvaluation::new(store)
        .execute(&fixture.project, expected, run, &waiting)
        .await?;
    let mut grant = GrantPolicyApproval::new(store)
        .execute(
            &fixture.project,
            run,
            waiting
                .approval_request()
                .ok_or("approval not requested")?
                .id(),
            ApprovalId::from_bytes([seed + 1; 32]),
            RunEventId::from_bytes([seed + 1; 32]),
            snapshot,
            at(u64::from(seed + 1))?,
        )
        .await?;
    let expected = run.last_event_sequence();
    let allowed = engine.execute(run, action, &policy, Some(&mut grant), context(seed + 2)?)?;
    assert_eq!(
        allowed.decision().reason(),
        PolicyDecisionReason::ApprovalGranted
    );
    PersistPolicyEvaluation::new(store)
        .execute(&fixture.project, expected, run, &allowed)
        .await?;
    Ok(allowed.decision().id())
}

fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(bytes).as_bytes())
}
