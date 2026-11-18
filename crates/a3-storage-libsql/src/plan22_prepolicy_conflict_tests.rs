//! An actual late source change prevents a prepared E3 patch from reaching policy.

use super::*;
use a3_application::RunEventPageLimit;
use a3_domain::{
    RunEventCode, RunEventKind, RunEventOutcome, RunEventSequence, TaskStepAttemptOutcome,
};

const ACTION: u8 = 60;

#[test]
fn original_window_conflict_before_policy_persists_exact_halt() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            let before = format!("First guard\r\n{OLD_LINE}\r\nLast guard\r\n");
            let foreign = before.replacen("Last guard", "Last guarD", 1);
            assert_eq!(before.len(), foreign.len());
            assert_ne!(before, foreign);
            fs::write(worktree.join(MIDDLE_PATH), before.as_bytes())?;
            fs::write(worktree.join("notes.txt"), OTHER_FILE)?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;

            let (run, failed, prior) = halt_stale_window(&fixture, &before, &foreign).await?;
            check_reopened_halt(&fixture, &run, &failed, prior).await?;
            assert!(
                inventory(&worktree)?
                    == vec![
                        (PathBuf::from("notes.txt"), OTHER_FILE.to_vec()),
                        (PathBuf::from(MIDDLE_PATH), foreign.into_bytes()),
                    ]
            );
            fixture.cleanup()
        })
    })
}

async fn halt_stale_window(
    fixture: &Fixture,
    before: &str,
    foreign: &str,
) -> TestResult<(AgentRun, StoredTaskLedger, RunEventSequence)> {
    let project = &fixture.project;
    let path = RepositoryPath::try_from_bytes(MIDDLE_PATH.as_bytes().to_vec())?;
    let (goal, step) = path_edit_goal(&path, b"Replace the selected middle line in subject.md.")?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let snapshot = published.run().snapshot_id();
    let profile = profile()?;
    let (mut run, stored, _) =
        start_fixture_step(fixture, &store, &goal, &profile, snapshot, vec![step]).await?;
    assert_eq!(run.state(), AgentControllerState::Execute);
    let prior = run.last_event_sequence();
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("published source missing")?;
    let page = WorkspaceAgentSourceReader
        .read_page(
            project,
            revision,
            &AgentFileInspection::new(
                path,
                AgentFileStartLine::new(2)?,
                AgentFileLineCount::new(1)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert_eq!(page.text(), format!("{OLD_LINE}\r\n"));
    let state = EditPhaseState::new(project, &goal, &stored, &run, &published)?;
    let phase = PreparedEditPhase::update(state, &page)?;
    let reply = serde_json::to_vec(&json!({
        "code":format!("{NEW_LINE}\n"),
    }))?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let reply = phase.bind_reply(&reply)?;
    let EditPhaseOutcome::Proposal(action) =
        phase.complete(reply, state, &patch, &ActiveControl).await?
    else {
        return Err("the current window did not produce a patch proposal".into());
    };
    let AgentAction::ApplyPatch(compiled) = &action else {
        return Err("expected one compiled patch action".into());
    };
    let [PatchOperation::Update(update)] = compiled.operations() else {
        return Err("expected one compiled file update".into());
    };
    assert_eq!(update.expected(), revision);
    assert!(update.content().as_bytes() == before.replacen(OLD_LINE, NEW_LINE, 1).as_bytes());

    let target = fixture.root.join("worktree").join(MIDDLE_PATH);
    assert!(fs::read(&target)? == before.as_bytes());
    fs::write(&target, foreign.as_bytes())?;
    assert_eq!(foreign.lines().nth(1), before.lines().nth(1));

    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(project);
    let runner = native_runner()?;
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
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal, profile, Vec::new(), Vec::new());
    let (mut ledger, mut version) = stored.into_parts();
    let outcome = controller
        .execute(
            project,
            &mut run,
            &mut ledger,
            &mut version,
            &published,
            action,
            None,
            &WorkspacePolicy::unrestricted(),
            None,
            execution_ids(ACTION),
            at(10)?,
            at(1_000)?,
            &seed,
            &mut compiler,
            &events,
            &ActiveControl,
        )
        .await;
    assert!(matches!(
        outcome,
        Err(MutationControllerFailure::PatchPreview(
            PatchPreviewFailure::Conflict(PatchConflictKind::SourceChangedOnDisk)
        ))
    ));
    assert_eq!(run.state(), AgentControllerState::Failed);
    assert_eq!(run.current_snapshot_id(), snapshot);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert!(
        inspection
            .overview(project, run.goal_contract().task_id())?
            .is_none()
    );
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "fixture process events poisoned")?
            .is_empty()
    );
    assert!(
        approvals
            .presentation(project, run.goal_contract().task_id())?
            .is_none()
    );
    assert!(fs::read(&target)? == foreign.as_bytes());
    Ok((run, StoredTaskLedger::new(ledger, version), prior))
}

async fn check_reopened_halt(
    fixture: &Fixture,
    halted: &AgentRun,
    failed: &StoredTaskLedger,
    prior: RunEventSequence,
) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let project = &fixture.project;
    let run = store
        .load_agent_run(project, halted.id())
        .await?
        .ok_or("halted Run missing")?;
    assert_eq!(&run, halted);
    let ledger = store
        .load_task_ledger(project, run.goal_contract().task_id())
        .await?
        .ok_or("failed Ledger missing")?;
    assert_eq!(ledger.version(), failed.version());
    assert_eq!(ledger.ledger(), failed.ledger());
    let step = ledger
        .ledger()
        .step(TaskStepId::from_bytes([STEP; 32]))
        .ok_or("failed step missing")?;
    assert_eq!(step.status(), TaskStepStatus::Failed);
    let [attempt] = step.attempts() else {
        return Err("expected only the original step attempt".into());
    };
    let TaskStepAttemptOutcome::Failed { reason } = attempt.outcome() else {
        return Err("typed conflict did not fail the attempt".into());
    };
    assert_eq!(reason.as_str(), "patch_source_changed_on_disk");
    assert!(attempt.verification().is_none());
    assert!(attempt.evidence_ids().is_empty());

    let events = store
        .load_run_events(project, run.id(), Some(prior), RunEventPageLimit::new(8)?)
        .await?;
    assert!(!events.has_more());
    let [event] = events.events() else {
        return Err("expected just the atomic halt event".into());
    };
    assert_eq!(
        event.kind(),
        RunEventKind::StateTransition {
            from: AgentControllerState::Execute,
            to: AgentControllerState::Failed
        }
    );
    assert_eq!(event.payload().code(), RunEventCode::ControllerDecision);
    assert_eq!(event.payload().outcome(), Some(RunEventOutcome::Failed));
    assert_eq!(event.id(), RunEventId::from_bytes([ACTION + 7; 32]));
    assert_eq!(event.snapshot_id(), run.current_snapshot_id());
    assert!(
        store
            .load_policy_decision(project, PolicyDecisionId::from_bytes([ACTION; 32]))
            .await?
            .is_none()
    );
    assert!(
        store
            .load_approval_request(project, ApprovalRequestId::from_bytes([ACTION + 1; 32]))
            .await?
            .is_none()
    );
    assert!(
        store
            .load_agent_mutation_attempts(project, run.id())
            .await?
            .is_empty()
    );
    Ok(())
}
