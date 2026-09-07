//! Guided work decisions share the same turn, decoder and mutation boundary.
use super::*;
use crate::agent_turn::after_change as decision;
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind, AgentToolAttempt,
    AgentToolAttemptNumber, AgentToolAttemptStatus, MutationActionFingerprint, RunEventKind,
    RunEventOutcome, RunEventSubject, ToolRunId,
};

const VERIFY: &str = r#"{"version":1,"next":"verify"}"#;
const MORE: &str = r#"{"version":1,"next":"continue_change"}"#;
const READ: &str = r#"{"version":1,"next":"need_evidence"}"#;

#[test]
fn guided_incomplete_response_and_cancel_cannot_request_a_command() -> Result<(), Box<dyn Error>> {
    for cancel in [false, true] {
        let mut fixture = fixture(&[], AgentMutationKind::Patch, true)?;
        fixture.responses.push_back(vec![
            ProviderEvent::OutputText(ModelOutputChunk::try_from_string(VERIFY.to_owned())?),
            ProviderEvent::Completed(ModelProviderCompletion::new(
                if cancel {
                    ModelFinishReason::Stop
                } else {
                    ModelFinishReason::OutputLimit
                },
                ModelProviderUsage::new(Some(100), Some(10)),
            )),
        ]);
        let compiler = RepeatStagedCompiler {
            template: fixture.compiled,
            calls: AtomicUsize::new(0),
            change_after: None,
        };
        let control = StagedCancellation(std::sync::atomic::AtomicBool::new(false));
        let provider = CancellingStagedProvider {
            inner: ScriptedProvider {
                provider_id: fixture.profile.provider_id().clone(),
                responses: Mutex::new(fixture.responses),
            },
            control: &control,
        };
        let selected: &dyn ModelProvider = if cancel { &provider } else { &provider.inner };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();
        let result = futures::executor::block_on(
            ExecuteAgentTurn::new(&compiler, selected, &tools, &recovery)
                .with_action_generation(AgentActionGeneration::ReviewThenSelect)
                .execute(&fixture.run, &fixture.input, timestamp(6)?, &control),
        )?;
        let AgentTurnOutcome::Rejected(rejected) = result else {
            return Err("incomplete/cancelled work decision became executable".into());
        };
        assert_eq!(
            rejected.reason(),
            if cancel {
                AgentTurnRejectionReason::CancelledBeforeAction
            } else {
                AgentTurnRejectionReason::IncompleteModelOutput(ModelFinishReason::OutputLimit)
            }
        );
        assert_eq!(rejected.charge().prompt_tokens().get(), 100);
        assert_eq!(rejected.charge().repair(), AgentTurnRepairUsage::None);
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
        assert!(
            provider
                .inner
                .responses
                .lock()
                .map_err(|_| "poison")?
                .is_empty()
        );
    }
    Ok(())
}

fn fixture(
    raw: &[&str],
    kind: AgentMutationKind,
    operational: bool,
) -> Result<TurnFixture, Box<dyn Error>> {
    let mut fixture = staged_fixture_with_command(raw, operational.then_some(command_id()))?;
    let (mut run, started) = AgentRun::start(
        fixture.run.id(),
        fixture.run.goal_contract(),
        fixture.run.task_ledger_revision(),
        fixture.profile.reference(),
        snapshot(),
        event_id(1),
        timestamp(1)?,
    )?;
    let mut events = vec![started];
    for (id, state) in [
        (2, AgentControllerState::Localize),
        (3, AgentControllerState::Plan),
        (4, AgentControllerState::Execute),
    ] {
        events.push(run.transition(
            event_id(id),
            state,
            RunEventPayload::empty(),
            snapshot(),
            timestamp(u64::from(id))?,
        )?);
    }
    let tool = ToolRunId::from_bytes([91; 32]);
    events.push(run.record(
        event_id(5),
        RunEventKind::ToolAction,
        RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
        snapshot(),
        Some(RunEventSubject::Tool(tool)),
        timestamp(5)?,
    )?);
    let attempt = AgentMutationAttempt::new(
        AgentToolAttempt::new(
            tool,
            AgentToolAttemptNumber::FIRST,
            run.id(),
            snapshot(),
            AgentToolAttemptStatus::Succeeded,
            timestamp(5)?,
            timestamp(5)?,
        )?,
        MutationActionFingerprint::from_bytes([92; 32]),
        kind,
        AgentMutationDisposition::Applied,
    )?;
    let page = crate::RunEventPage::new(None, crate::RunEventPageLimit::new(64)?, events, false)?;
    let receipt = crate::AgentExecutionCheckpoint::reconstruct(
        fixture.input.project(),
        &run,
        &page,
        &[attempt],
    )?
    .ok_or("receipt")?;
    fixture.input = fixture.input.with_execution_checkpoint(receipt)?;
    fixture.run = run;
    Ok(fixture)
}

fn command_id() -> a3_domain::DiscoveredCommandId {
    a3_domain::DiscoveredCommandId::from_bytes([90; 32])
}

#[test]
fn guided_verify_uses_only_known_command_without_argument_call_or_tool_effect()
-> Result<(), Box<dyn Error>> {
    for raw in [vec![VERIFY], vec!["{}", VERIFY]] {
        let fixture = fixture(&raw, AgentMutationKind::Patch, true)?;
        let compiler = RepeatStagedCompiler {
            template: fixture.compiled,
            calls: AtomicUsize::new(0),
            change_after: None,
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
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
                .with_action_generation(AgentActionGeneration::ReviewThenSelect)
                .execute(&fixture.run, &fixture.input, timestamp(6)?, &TestControl),
        )?;
        let AgentTurnOutcome::Executed(execution) = &result else {
            return Err(format!("expected planned command, got {result:?}").into());
        };
        let AgentAction::Run(command) = execution.action() else {
            return Err("not the planned run".into());
        };
        assert_eq!(command.command_id(), command_id());
        assert_eq!(command.step_id(), fixture.input.current_step_id());
        assert_eq!(
            execution.charge().prompt_tokens().get(),
            raw.len() as u32 * 100
        );
        assert_eq!(
            execution.charge().repair(),
            if raw.len() == 1 {
                AgentTurnRepairUsage::None
            } else {
                AgentTurnRepairUsage::One
            }
        );
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
        assert_eq!(
            provider.requests.lock().map_err(|_| "poison")?.len(),
            raw.len()
        );
        assert!(
            fixture
                .input
                .task_ledger()
                .step(command.step_id())
                .ok_or("step")?
                .attempts()
                .last()
                .ok_or("attempt")?
                .verification()
                .is_none()
        );
        let mut run = fixture.run;
        result.record(&mut run, event_id(6), timestamp(6)?)?;
        assert_eq!(run.state(), AgentControllerState::Execute);
        assert_eq!(run.usage().turn_count(), 1);
    }
    Ok(())
}

#[test]
fn guided_gate_requires_a_current_patch_and_operational_step() -> Result<(), Box<dyn Error>> {
    for (kind, operational, expected) in [
        (AgentMutationKind::Patch, true, true),
        (AgentMutationKind::Patch, false, false),
        (AgentMutationKind::Process, true, false),
    ] {
        let fixture = fixture(&[], kind, operational)?;
        assert_eq!(
            decision::planned_verification(&fixture.input, &fixture.run).is_some(),
            expected
        );
        let no_receipt = staged_fixture_with_command(&[], Some(command_id()))?;
        assert!(decision::planned_verification(&no_receipt.input, &no_receipt.run).is_none());
    }
    for raw in [
        r#"{"version":1,"next":"done"}"#,
        r#"{"version":2,"next":"verify"}"#,
        r#"{"version":1,"next":"verify","command_id":"arbitrary"}"#,
        r#"{"version":1,"next":"verify","passed":true}"#,
    ] {
        assert!(decision::decode(raw).is_none());
    }
    Ok(())
}

#[test]
fn guided_read_and_change_paths_keep_one_shared_repair_and_scope() -> Result<(), Box<dyn Error>> {
    let patch = r#"{"version":1,"choice":"patch_update"}"#;
    let patch_arguments = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Finish the remaining file","operations":[{{"path":"second.py","expected_hash":"{}","content":"value = 2\n"}}]}}}}"#,
        "ab".repeat(32)
    );
    for (raw, accepted, reads, repaired) in [
        (vec![READ, SEARCH_CHOICE, SEARCH_ARGUMENTS], true, 1, false),
        (vec![MORE, patch, &patch_arguments], true, 0, false),
        (
            vec!["{}", READ, SEARCH_CHOICE, SEARCH_ARGUMENTS],
            true,
            1,
            true,
        ),
        (
            vec![READ, patch, SEARCH_CHOICE, SEARCH_ARGUMENTS],
            true,
            1,
            true,
        ),
        (
            vec![MORE, SEARCH_CHOICE, patch, &patch_arguments],
            true,
            0,
            true,
        ),
        (vec![MORE, patch, "{}", &patch_arguments], true, 0, true),
        (
            vec!["{}", READ, SEARCH_CHOICE, "{}", SEARCH_ARGUMENTS],
            false,
            0,
            true,
        ),
    ] {
        let fixture = fixture(&raw, AgentMutationKind::Patch, true)?;
        let compiler = RepeatStagedCompiler {
            template: fixture.compiled,
            calls: AtomicUsize::new(0),
            change_after: None,
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
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
                .with_action_generation(AgentActionGeneration::ReviewThenSelect)
                .execute(&fixture.run, &fixture.input, timestamp(6)?, &TestControl),
        )?;
        assert_eq!(
            matches!(result, AgentTurnOutcome::Executed(_)),
            accepted,
            "{result:?}"
        );
        let charge = match &result {
            AgentTurnOutcome::Executed(v) => v.charge(),
            AgentTurnOutcome::Rejected(v) => v.charge(),
            _ => return Err("wrong outcome".into()),
        };
        let requests = provider.requests.lock().map_err(|_| "poison")?;
        assert_eq!(requests.len(), raw.len().min(4));
        assert_eq!(charge.prompt_tokens().get(), requests.len() as u32 * 100);
        assert_eq!(
            charge.repair(),
            if repaired {
                AgentTurnRepairUsage::One
            } else {
                AgentTurnRepairUsage::None
            }
        );
        assert_eq!(tools.calls.load(Ordering::SeqCst), reads);
        for request in requests.iter() {
            assert!(
                request
                    .messages()
                    .iter()
                    .any(|m| m.content() == "bounded controller context")
            );
            assert!(
                !request
                    .messages()
                    .iter()
                    .any(|m| raw.contains(&m.content()))
            );
        }
    }
    Ok(())
}

#[test]
fn guided_freshness_after_review_prevents_choice_and_preserves_charge() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(
        &[READ, SEARCH_CHOICE, SEARCH_ARGUMENTS],
        AgentMutationKind::Patch,
        true,
    )?;
    let compiler = RepeatStagedCompiler {
        template: fixture.compiled,
        calls: AtomicUsize::new(0),
        change_after: Some(1),
    };
    let provider = ScriptedProvider {
        provider_id: fixture.profile.provider_id().clone(),
        responses: Mutex::new(fixture.responses),
    };
    let tools = CountingReadTools {
        calls: AtomicUsize::new(0),
    };
    let recovery = TestRecoveryStore::default();
    let result = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_action_generation(AgentActionGeneration::ReviewThenSelect)
            .execute(&fixture.run, &fixture.input, timestamp(6)?, &TestControl),
    )?;
    let AgentTurnOutcome::Rejected(rejected) = result else {
        return Err("changed source accepted".into());
    };
    assert_eq!(
        rejected.reason(),
        AgentTurnRejectionReason::Staged(StagedActionFailure::ContextChanged)
    );
    assert_eq!(rejected.charge().prompt_tokens().get(), 100);
    assert_eq!(provider.responses.lock().map_err(|_| "poison")?.len(), 2);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}
