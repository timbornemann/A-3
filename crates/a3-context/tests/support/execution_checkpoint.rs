//! Actual-context contracts for the content-free durable execution projection.
use super::*;
use a3_application::{
    AgentExecutionCheckpoint, AgentExecutionCheckpointError, ExecutedAgentMutation, RunEventPage,
    RunEventPageLimit,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind, AgentToolAttempt,
    AgentToolAttemptNumber, AgentToolAttemptStatus, MutationActionFingerprint,
    MutationReconciliation, RunEvent, RunEventCode, RunEventId, RunEventIdentity, RunEventKind,
    RunEventOccurrence, RunEventOutcome, RunEventPayload, RunEventSubject, ToolRunId,
};

type ExecutionReceiptFixture = (
    AgentContextCompileInput,
    AgentRun,
    Vec<RunEvent>,
    AgentMutationAttempt,
);

fn receipt_fixture(
    snapshot: SnapshotId,
    context: u32,
    output: u32,
) -> Result<ExecutionReceiptFixture, Box<dyn Error>> {
    let base = input(snapshot)?;
    let run_id = AgentRunId::from_bytes([42; 32]);
    let mut ledger = base.task_ledger().clone();
    ledger.start_step(
        base.current_step_id(),
        run_id,
        TaskLedgerTimestamp::from_unix_millis(10)?,
    )?;
    let input = AgentContextCompileInput::new(
        base.project().clone(),
        base.goal_contract().clone(),
        ledger,
        base.current_step_id(),
        profile_with_limits(context, output)?,
        None,
        Vec::new(),
        Vec::new(),
    )?;
    let time = AgentRunTimestamp::from_unix_millis(10)?;
    let mut run = AgentRun::reconstruct(
        AgentRunIdentity::new(
            run_id,
            input.goal_contract().reference(),
            input.task_ledger().revision(),
            Some(input.model_profile().reference()),
        ),
        AgentRunMaterializedState::new(
            AgentControllerState::Execute,
            RunEventSequence::FIRST,
            snapshot,
        ),
        AgentRunTiming::new(time, time),
    )?;
    let start = RunEvent::reconstruct(
        RunEventIdentity::new(
            RunEventId::from_bytes([40; 32]),
            run_id,
            RunEventSequence::FIRST,
        ),
        RunEventOccurrence::new(time, snapshot, None),
        RunEventKind::RunStarted,
        RunEventPayload::empty(),
    )?;
    let tool_id = ToolRunId::from_bytes([43; 32]);
    let event = run.record(
        RunEventId::from_bytes([41; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
        snapshot,
        Some(RunEventSubject::Tool(tool_id)),
        time,
    )?;
    let attempt = AgentMutationAttempt::new(
        AgentToolAttempt::new(
            tool_id,
            AgentToolAttemptNumber::FIRST,
            run_id,
            SnapshotId::from_bytes([99; 32]),
            AgentToolAttemptStatus::Succeeded,
            time,
            time,
        )?,
        MutationActionFingerprint::from_bytes([44; 32]),
        AgentMutationKind::Patch,
        AgentMutationDisposition::Applied,
    )?;
    Ok((input, run, vec![start, event], attempt))
}

fn page(events: &[RunEvent]) -> Result<RunEventPage, Box<dyn Error>> {
    let first = events.first().ok_or("empty test events")?.sequence().get();
    let after = if first == 1 {
        None
    } else {
        Some(RunEventSequence::new(first - 1)?)
    };
    Ok(RunEventPage::new(
        after,
        RunEventPageLimit::new(256)?,
        events.to_vec(),
        false,
    )?)
}

#[test]
fn execution_receipt_is_mandatory_counted_deterministic_and_not_verification()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published.clone(),
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let compiler = DeterministicAgentContextCompiler::new(
        CompileTaskLens::new(&store, &store, &store),
        &UnavailableSource,
    );
    for (context, output) in [(8192, 2048), (16384, 4096)] {
        let (base, run, events, attempt) = receipt_fixture(fixture.snapshot_id, context, output)?;
        let before = block_on(compiler.compile(&base, &RecordingControl::default()))?;
        for (kind, label) in [
            (AgentMutationKind::Patch, "patch_applied"),
            (AgentMutationKind::Process, "process_result_observed"),
        ] {
            let observed = AgentMutationAttempt::new(
                attempt.tool_attempt(),
                attempt.fingerprint(),
                kind,
                AgentMutationDisposition::Applied,
            )?;
            let receipt = AgentExecutionCheckpoint::reconstruct(
                base.project(),
                &run,
                &page(&events)?,
                &[observed],
            )?
            .ok_or("execution receipt")?;
            let request = base.clone().with_execution_checkpoint(receipt)?;
            let after = block_on(compiler.compile(&request, &RecordingControl::default()))?;
            assert_ne!(before.digest(), after.digest());
            assert_eq!(
                after.digest(),
                block_on(compiler.compile(&request, &RecordingControl::default()))?.digest()
            );
            assert_eq!(after.budget_plan().output_reserve(), output);
            assert_eq!(
                after.budget_plan().safety_reserve(),
                before.budget_plan().safety_reserve()
            );
            assert!(
                after
                    .budget_usage()
                    .section(a3_domain::ContextSection::GoalAndLedger)
                    > before
                        .budget_usage()
                        .section(a3_domain::ContextSection::GoalAndLedger)
            );
            assert!(
                after.budget_usage().prompt_total() + output + after.budget_plan().safety_reserve()
                    <= context
            );
            let text = after.request().messages().last().ok_or("pack")?.content();
            if kind == AgentMutationKind::Patch {
                eprintln!(
                    "A3_EXECUTION_CHECKPOINT_MEASURE context={context} output={output} pack_bytes_before={} pack_bytes_after={} goal_tokens_before={} goal_tokens_after={}",
                    before
                        .request()
                        .messages()
                        .last()
                        .ok_or("baseline pack")?
                        .content()
                        .len(),
                    text.len(),
                    before
                        .budget_usage()
                        .section(a3_domain::ContextSection::GoalAndLedger),
                    after
                        .budget_usage()
                        .section(a3_domain::ContextSection::GoalAndLedger)
                );
            }
            assert!(text.contains(&format!("last_confirmed_run_action={label}")));
            assert!(text.contains("step_verified=false"));
            assert!(text.contains("not source evidence or test success"));
            assert!(!text.contains("step_verified=true"));
            assert_eq!(request.task_ledger(), base.task_ledger());
        }
    }
    Ok(())
}

#[test]
fn execution_receipt_rejects_foreign_run_ledger_project_and_stale_publication()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let (base, run, events, attempt) = receipt_fixture(fixture.snapshot_id, 8192, 2048)?;
    let receipt =
        AgentExecutionCheckpoint::reconstruct(base.project(), &run, &page(&events)?, &[attempt])?
            .ok_or("receipt")?;
    // No active run / older ledger is not the receipt's owner.
    assert!(
        input(fixture.snapshot_id)?
            .with_execution_checkpoint(receipt.clone())
            .is_err()
    );
    let mut later_ledger = base.task_ledger().clone();
    later_ledger.begin_step_verification(
        base.current_step_id(),
        run.id(),
        None,
        Vec::new(),
        TaskLedgerTimestamp::from_unix_millis(11)?,
    )?;
    let later = AgentContextCompileInput::new(
        base.project().clone(),
        base.goal_contract().clone(),
        later_ledger,
        base.current_step_id(),
        base.model_profile().clone(),
        None,
        Vec::new(),
        Vec::new(),
    )?;
    // A status update within the same plan revision is current Ledger data, not a stale receipt.
    assert!(later.with_execution_checkpoint(receipt.clone()).is_ok());
    let other_revision = AgentRun::reconstruct(
        AgentRunIdentity::new(
            run.id(),
            run.goal_contract(),
            a3_domain::TaskLedgerRevision::new(2)?,
            run.model_profile(),
        ),
        AgentRunMaterializedState::new(
            run.state(),
            run.last_event_sequence(),
            run.current_snapshot_id(),
        ),
        AgentRunTiming::new(run.created_at(), run.updated_at()),
    )?;
    let future_receipt = AgentExecutionCheckpoint::reconstruct(
        base.project(),
        &other_revision,
        &page(&events)?,
        &[attempt],
    )?
    .ok_or("future revision receipt")?;
    assert!(
        base.clone()
            .with_execution_checkpoint(future_receipt)
            .is_err()
    );
    let other = ProjectIdentity::new(
        base.project().repository().clone(),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([97; 32]),
            base.project().worktree().anchor_id(),
            base.project().repository().id(),
            base.project().worktree().root().clone(),
        ),
        base.project().head().clone(),
    )?;
    let wrong_project = AgentContextCompileInput::new(
        other,
        base.goal_contract().clone(),
        base.task_ledger().clone(),
        base.current_step_id(),
        base.model_profile().clone(),
        None,
        Vec::new(),
        Vec::new(),
    )?;
    assert!(wrong_project.with_execution_checkpoint(receipt).is_err());
    let (stale_input, stale_run, stale_events, stale_attempt) =
        receipt_fixture(SnapshotId::from_bytes([98; 32]), 8192, 2048)?;
    let stale = AgentExecutionCheckpoint::reconstruct(
        stale_input.project(),
        &stale_run,
        &page(&stale_events)?,
        &[stale_attempt],
    )?
    .ok_or("stale")?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published,
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    let compiler = DeterministicAgentContextCompiler::new(
        CompileTaskLens::new(&store, &store, &store),
        &UnavailableSource,
    );
    assert!(matches!(
        block_on(compiler.compile(
            &stale_input.with_execution_checkpoint(stale)?,
            &RecordingControl::default()
        )),
        Err(ContextCompileFailure::StaleOrMismatchedInput)
    ));
    Ok(())
}

#[test]
fn execution_receipt_requires_unique_successful_attempt_and_exact_journal_tail()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let (input, mut run, mut events, attempt) = receipt_fixture(fixture.snapshot_id, 8192, 2048)?;
    let original = page(&events)?;
    assert!(
        AgentExecutionCheckpoint::reconstruct(input.project(), &run, &original, &[])?.is_none()
    );
    assert!(matches!(
        AgentExecutionCheckpoint::reconstruct(
            input.project(),
            &run,
            &original,
            &[attempt, attempt]
        ),
        Err(AgentExecutionCheckpointError::Inconsistent)
    ));
    for (status, disposition) in [
        (
            AgentToolAttemptStatus::Failed,
            AgentMutationDisposition::Applied,
        ),
        (
            AgentToolAttemptStatus::Denied,
            AgentMutationDisposition::NotApplied,
        ),
        (
            AgentToolAttemptStatus::InFlight,
            AgentMutationDisposition::Unknown(MutationReconciliation::Required),
        ),
        (
            AgentToolAttemptStatus::Interrupted,
            AgentMutationDisposition::Unknown(MutationReconciliation::Reconciled {
                snapshot_id: fixture.snapshot_id,
            }),
        ),
    ] {
        let tool = attempt.tool_attempt();
        let observed = AgentMutationAttempt::new(
            AgentToolAttempt::new(
                tool.tool_run_id(),
                tool.attempt(),
                tool.run_id(),
                tool.snapshot_id(),
                status,
                tool.started_at(),
                tool.updated_at(),
            )?,
            attempt.fingerprint(),
            AgentMutationKind::Patch,
            disposition,
        )?;
        assert!(
            AgentExecutionCheckpoint::reconstruct(input.project(), &run, &original, &[observed])?
                .is_none()
        );
    }
    let tool = attempt.tool_attempt();
    for (run_id, tool_id, time) in [
        (
            AgentRunId::from_bytes([88; 32]),
            tool.tool_run_id(),
            tool.updated_at(),
        ),
        (
            tool.run_id(),
            ToolRunId::from_bytes([88; 32]),
            tool.updated_at(),
        ),
        (
            tool.run_id(),
            tool.tool_run_id(),
            AgentRunTimestamp::from_unix_millis(11)?,
        ),
    ] {
        let foreign = AgentMutationAttempt::new(
            AgentToolAttempt::new(
                tool_id,
                tool.attempt(),
                run_id,
                tool.snapshot_id(),
                tool.status(),
                tool.started_at(),
                time,
            )?,
            attempt.fingerprint(),
            attempt.kind(),
            attempt.disposition(),
        )?;
        let result =
            AgentExecutionCheckpoint::reconstruct(input.project(), &run, &original, &[foreign]);
        assert!(result.is_err() || result?.is_none());
    }
    // Journal order, not ToolRunId sort order, determines the latest receipt.
    for n in 3..=70_u8 {
        events.push(run.record(
            RunEventId::from_bytes([n; 32]),
            RunEventKind::ContextCompiled,
            RunEventPayload::empty(),
            fixture.snapshot_id,
            None,
            AgentRunTimestamp::from_unix_millis(u64::from(n) + 10)?,
        )?);
    }
    assert!(
        AgentExecutionCheckpoint::reconstruct(input.project(), &run, &page(&events)?, &[attempt])
            .is_err()
    );
    assert!(
        AgentExecutionCheckpoint::reconstruct(
            input.project(),
            &run,
            &page(&events[6..])?,
            &[attempt]
        )?
        .is_none()
    );
    assert!(
        AgentExecutionCheckpoint::reconstruct(input.project(), &run, &original, &[attempt])
            .is_err()
    );
    let time = AgentRunTimestamp::from_unix_millis(81)?;
    let newer_tool_id = ToolRunId::from_bytes([1; 32]);
    let newer = AgentMutationAttempt::new(
        AgentToolAttempt::new(
            newer_tool_id,
            tool.attempt(),
            run.id(),
            tool.snapshot_id(),
            AgentToolAttemptStatus::Succeeded,
            time,
            time,
        )?,
        attempt.fingerprint(),
        AgentMutationKind::Process,
        AgentMutationDisposition::Applied,
    )?;
    events.push(run.record(
        RunEventId::from_bytes([71; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
        fixture.snapshot_id,
        Some(RunEventSubject::Tool(newer_tool_id)),
        time,
    )?);
    let receipt = AgentExecutionCheckpoint::reconstruct(
        input.project(),
        &run,
        &page(&events[7..])?,
        &[newer, attempt],
    )?
    .ok_or("latest")?;
    assert_eq!(receipt.tool_run_id(), newer_tool_id);
    assert_eq!(receipt.mutation(), ExecutedAgentMutation::ProcessObserved);
    Ok(())
}
