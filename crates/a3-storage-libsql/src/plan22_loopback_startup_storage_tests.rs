//! Complete startup receipts through native policy, journal and evidence persistence.

use super::*;
use a3_application::PreparedLoopbackStartupCoverage;
use a3_domain::{LoopbackFailureExitCode, LoopbackProcessSettings, LoopbackStartupEvidence};

#[test]
fn complete_startup_receipt_requires_each_phase_and_reopens_at_the_same_journal_tail() -> TestResult
{
    crate::run_native_libsql_test(async {
        let mut fixture = Fixture::new()?;
        let worktree = fixture.root.join("worktree");
        initialize_git(&worktree)?;
        fs::write(
            worktree.join("server.py"),
            b"# controlled storage fixture, never executed\n",
        )?;
        fixture.project = RepositoryInspector::new().inspect(&worktree)?;
        let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
        let snapshot = published.run().snapshot_id();
        let base = recipe()?;
        let recipe = LoopbackHttpRecipe::new(
            base.script().clone(),
            LoopbackStart::Explicit,
            NonZeroU16::try_from(18_765_u16)?,
            base.checks().to_vec(),
        )?
        .with_startup_coverage(0)?;
        let script = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|file| file.path() == recipe.script())
            .ok_or("script not published")?
            .clone();
        let goal = goal(recipe.clone())?;
        let step = TaskStepId::from_bytes([STEP; 32]);
        let spec = VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string(
                "Prove all starts and current HTTP responses".to_owned(),
            )?,
            recipe.clone(),
        );
        let definition = TaskStepDefinition::new(
            step,
            None,
            TaskStepOutcome::try_from_string(
                "Implement the requested service behavior".to_owned(),
            )?,
            TaskStepRationale::try_from_string(
                "Verify the complete original runtime obligation".to_owned(),
            )?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "All owned startup observations".to_owned(),
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
        let output = ProcessOutputLimit::new(1024)?;
        let settings = LoopbackProcessSettings {
            timeout: ProcessTimeout::from_millis(8000)?,
            stdout_limit: output,
            stderr_limit: output,
            system_root: false,
        };
        let prepared = PreparedLoopbackStartupCoverage::new(
            run.id(),
            fixture.project.worktree().id(),
            step,
            snapshot,
            script.clone(),
            recipe.clone(),
            settings,
        )?;
        let actions = prepared.policy_actions();
        assert_eq!(actions.len(), 8);
        assert_ne!(actions[1].fingerprint(), actions[2].fingerprint());
        let mut decisions = Vec::with_capacity(actions.len());
        for (index, action) in actions.iter().enumerate() {
            decisions.push(
                approve(
                    &fixture,
                    &store,
                    &mut run,
                    action,
                    10 + 5 * u8::try_from(index)?,
                )
                .await?,
            );
        }
        let positive = LoopbackHttpObservation::new(
            decisions[3],
            200,
            hash(b"Ready\n"),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?;
        let negative = LoopbackHttpObservation::new(
            decisions[4],
            405,
            hash(b""),
            0,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?;
        let default_observation = LoopbackHttpObservation::new(
            decisions[6],
            200,
            hash(b"Ready\n"),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?;
        let startup = |held, default_probe| {
            LoopbackStartupEvidence::new(
                decisions[0],
                held,
                decisions[5],
                decisions[7],
                LoopbackFailureExitCode::new(1)?,
                LoopbackFailureExitCode::new(2)?,
                default_probe,
            )
        };
        let context = LoopbackHttpEvidenceContext {
            verification_run_id: VerificationRunId::from_bytes([71; 32]),
            spec_id: spec.id(),
            run_id: run.id(),
            tool_run_id: ToolRunId::from_bytes([72; 32]),
            step_id: step,
            snapshot_id: snapshot,
        };
        let process = LoopbackHttpProcess::from_spec(&recipe.process_spec(
            run.id(),
            fixture.project.worktree().id(),
            step,
            settings,
        )?)?;
        let receipt = LoopbackHttpEvidence::new_startup(
            context,
            recipe.id(),
            script.clone(),
            process,
            decisions[2],
            vec![positive, negative],
            startup(decisions[1], default_observation)?,
        )?;
        assert!(receipt.matches(&recipe));
        let scope = MachineEffectScope::new(
            AgentMutationKind::MachineProcess,
            step,
            prepared.effect_resource(),
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
                prepared.effect_fingerprint(),
                &scope,
                at(70)?,
            )
            .await?;
        let before_completion = run.clone();
        let expected = run.last_event_sequence();
        let event = run.record(
            RunEventId::from_bytes([73; 32]),
            RunEventKind::ToolAction,
            RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
            snapshot,
            Some(RunEventSubject::Tool(context.tool_run_id)),
            at(80)?,
        )?;
        let swapped = LoopbackHttpEvidence::new_startup(
            context,
            recipe.id(),
            script.clone(),
            process,
            decisions[1],
            vec![positive, negative],
            startup(decisions[2], default_observation)?,
        )?;
        let foreign_default = LoopbackHttpObservation::new(
            PolicyDecisionId::from_bytes([90; 32]),
            200,
            hash(b"Ready\n"),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?;
        let unapproved_default = LoopbackHttpEvidence::new_startup(
            context,
            recipe.id(),
            script,
            process,
            decisions[2],
            vec![positive, negative],
            startup(decisions[1], foreign_default)?,
        )?;
        for wrong in [swapped, unapproved_default] {
            assert_eq!(
                store
                    .complete_loopback_mutation(
                        &fixture.project,
                        expected,
                        &run,
                        &event,
                        attempt.tool_attempt().attempt(),
                        &wrong,
                        Duration::from_secs(10),
                        &ActiveControl,
                    )
                    .await,
                Err(AgentRecoveryStoreFailure::InvalidStoredData)
            );
            assert_eq!(
                store.load_agent_run(&fixture.project, run.id()).await?,
                Some(before_completion.clone())
            );
            assert_eq!(
                store
                    .load_agent_mutation_attempts(&fixture.project, run.id())
                    .await?,
                [attempt]
            );
            assert_no_settlement_rows(&fixture, &wrong).await?;
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
        assert_eq!(
            store
                .load_settled_loopback_evidence_id(
                    &fixture.project,
                    run.id(),
                    context.tool_run_id,
                    attempt.tool_attempt().attempt(),
                )
                .await?,
            Some(receipt.id())
        );
        drop((compiler, refresh, store));

        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            reopened.load_agent_run(&fixture.project, run.id()).await?,
            Some(run.clone())
        );
        let evidence = VerificationEvidence::LoopbackHttp(receipt.clone());
        let state = reopened
            .load_verification_state(
                &fixture.project,
                goal.task_id(),
                &[receipt.id()],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        assert_eq!(state.evidence(), std::slice::from_ref(&evidence));
        assert_eq!(
            reopened
                .load_settled_loopback_evidence_id(
                    &fixture.project,
                    run.id(),
                    context.tool_run_id,
                    attempt.tool_attempt().attempt(),
                )
                .await?,
            Some(receipt.id())
        );
        let stored = reopened
            .load_task_ledger(&fixture.project, goal.task_id())
            .await?
            .ok_or("ledger missing")?;
        let recovered_step = stored
            .ledger()
            .step(step)
            .ok_or("step missing")?
            .definition();
        assert_eq!(recovered_step.verification_spec(), &spec);
        let verification = EvaluateStepVerification.execute_for_step(
            StepVerificationId::from_bytes([76; 32]),
            recovered_step,
            run.id(),
            &state.evidence()[0],
            state.published_index(),
            TaskLedgerTimestamp::from_unix_millis(90)?,
        )?;
        assert!(verification.passed());
        drop(reopened);
        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        let mut rows = connection
            .query(
                "SELECT observed_output_bytes FROM tool_runs WHERE tool_run_id=?1",
                libsql::params![context.tool_run_id.as_bytes().to_vec()],
            )
            .await?;
        let row = rows.next().await?.ok_or("completed tool row missing")?;
        assert_eq!(row.get::<i64>(0)?, 12);
        drop((row, rows, connection, db));
        fixture.cleanup()
    })
}
