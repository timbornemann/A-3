use super::*;
use a3_application::{AgentRecoveryStoreFailure, ReconcileUnknownMutation};
use a3_domain::MutationReconciliation;

#[test]
fn applied_window_without_durable_result_stays_unknown_until_real_reconciliation() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            let before = format!("{OLD_LINE}\r\n{OLD_LINE}\r\n");
            let expected = format!("{OLD_LINE}\r\n{NEW_LINE}\r\n");
            fs::write(worktree.join(SUBJECT_PATH), before.as_bytes())?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;
            let (run_id, original_snapshot, fingerprint) = apply_without_result(&fixture).await?;
            assert_eq!(fs::read(worktree.join(SUBJECT_PATH))?, expected.as_bytes());

            // The prior store, index compiler, grant and adapter have all been dropped.
            {
                let store =
                    Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
                let mut run = store
                    .load_agent_run(&fixture.project, run_id)
                    .await?
                    .ok_or("run missing after reopen")?;
                assert_eq!(run.current_snapshot_id(), original_snapshot);
                assert_eq!(run.state(), AgentControllerState::Execute);
                let attempts = store
                    .load_agent_mutation_attempts(&fixture.project, run_id)
                    .await?;
                let [attempt] = attempts.as_slice() else {
                    return Err("expected one unresolved attempt".into());
                };
                assert_eq!(attempt.fingerprint(), fingerprint);
                assert_eq!(
                    attempt.tool_attempt().status(),
                    AgentToolAttemptStatus::InFlight
                );
                assert_eq!(
                    attempt.disposition(),
                    AgentMutationDisposition::Unknown(MutationReconciliation::Required)
                );
                assert_eq!(
                    store
                        .begin_agent_mutation_attempt(
                            &fixture.project,
                            run_id,
                            original_snapshot,
                            ToolRunId::from_bytes([195; 32]),
                            fingerprint,
                            AgentMutationKind::Patch,
                            at(20)?,
                        )
                        .await,
                    Err(AgentRecoveryStoreFailure::MutationReconciliationRequired)
                );
                let refresh = RefreshRepositoryIndex::new(
                    Arc::new(Blake3RepositorySnapshotBuilder::new()),
                    store.clone(),
                    Arc::new(Blake3IndexRunIdFactory),
                );
                let coordinator = WorktreeMutationCoordinator::new();
                let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
                let reconciled =
                    ReconcileUnknownMutation::new(&coordinator, store.as_ref(), &refresh)
                        .execute(
                            &fixture.project,
                            &mut run,
                            attempt.tool_attempt().tool_run_id(),
                            attempt.tool_attempt().attempt(),
                            RunEventId::from_bytes([196; 32]),
                            at(21)?,
                            &mut compiler,
                            &ActiveControl,
                        )
                        .await?;
                let snapshot_id = reconciled.published_index().run().snapshot_id();
                assert_ne!(snapshot_id, original_snapshot);
                assert_eq!(run.current_snapshot_id(), snapshot_id);
                assert_eq!(
                    reconciled.attempt().disposition(),
                    AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled {
                        snapshot_id
                    })
                );
                let ledger = store
                    .load_task_ledger(&fixture.project, run.goal_contract().task_id())
                    .await?
                    .ok_or("ledger missing after reconciliation")?;
                let step = ledger
                    .ledger()
                    .step(TaskStepId::from_bytes([STEP; 32]))
                    .ok_or("step missing")?;
                assert_eq!(step.status(), TaskStepStatus::InProgress);
                assert!(
                    step.attempts()
                        .last()
                        .ok_or("attempt missing")?
                        .verification()
                        .is_none()
                );
            }
            assert_eq!(fs::read(worktree.join(SUBJECT_PATH))?, expected.as_bytes());
            fixture.cleanup()?;
            Ok(())
        })
    })
}

async fn apply_without_result(
    fixture: &Fixture,
) -> TestResult<(AgentRunId, SnapshotId, MutationActionFingerprint)> {
    let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
    let (goal, step) = path_edit_goal(&path, b"Change only the second assignment in subject.py.")?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let original_snapshot = published.run().snapshot_id();
    let profile = profile()?;
    let (mut run, stored, _) = start_fixture_step(
        fixture,
        &store,
        &goal,
        &profile,
        original_snapshot,
        vec![step],
    )
    .await?;
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("published subject missing")?;
    let page = WorkspaceAgentSourceReader
        .read_page(
            &fixture.project,
            revision,
            &AgentFileInspection::new(
                path,
                AgentFileStartLine::new(2)?,
                AgentFileLineCount::new(1)?,
            ),
            &ActiveControl,
        )
        .await?;
    let state = EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published)?;
    let phase = PreparedEditPhase::update(state, &page)?;
    let reply = serde_json::to_vec(&json!({
        "code": format!("{NEW_LINE}\n"),
    }))?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let reply = phase.bind_reply(&reply)?;
    let outcome = phase.complete(reply, state, &patch, &ActiveControl).await?;
    let EditPhaseOutcome::Proposal(action) = outcome else {
        return Err("the bound replacement did not produce a proposal".into());
    };
    let fingerprint = MutationActionFingerprint::from_action(&action)?;

    // Fault injection on the actual success receipt transaction. Starting the attempt and
    // applying the patch must still go through the regular controller and native adapters.
    let database = fixture.candidate_database().await?;
    let connection = database.connection_for_operation().await?;
    connection
        .execute_batch(
            "CREATE TRIGGER abort_window_result BEFORE INSERT ON tool_runs
         BEGIN SELECT RAISE(ABORT, 'fixture receipt failure'); END;",
        )
        .await?;
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let runner = native_runner()?;
    let factory = BuiltinProcessVerificationEvidenceFactory;
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
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal, profile, Vec::new(), Vec::new());
    let (mut ledger, mut version) = stored.into_parts();
    let result = execute_approved(
        controller,
        fixture,
        store.as_ref(),
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        action,
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
    )
    .await;
    connection
        .execute_batch("DROP TRIGGER abort_window_result")
        .await?;
    let Err(error) = result else {
        return Err("the native receipt transaction unexpectedly succeeded".into());
    };
    assert!(
        matches!(
            error.downcast_ref::<MutationControllerFailure>(),
            Some(MutationControllerFailure::MutationResultStore(_))
        ),
        "unexpected controller failure {error:?}"
    );
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    Ok((run.id(), original_snapshot, fingerprint))
}
