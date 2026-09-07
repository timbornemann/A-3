// Shares the existing controller/port fixtures without replacing production boundaries.
use super::*;
#[derive(Debug)]
struct RepeatStagedCompiler {
    template: CompiledAgentContext,
    calls: AtomicUsize,
    change_after: Option<usize>,
}

impl AgentContextCompiler for RepeatStagedCompiler {
    fn compile<'a>(
        &'a self,
        _: &'a AgentContextCompileInput,
        _: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let t = &self.template;
        let digest = if self.change_after.is_some_and(|limit| call >= limit) {
            ContextDigest::from_bytes([77; 32])
        } else {
            t.digest()
        };
        let result = CompiledAgentContext::new(
            t.request().clone(),
            t.policy_version(),
            digest,
            t.goal_contract(),
            t.ledger_revision(),
            t.current_step_id(),
            t.index_run_id(),
            t.snapshot_id(),
            t.task_lens_digest(),
            t.run_memory_digest(),
            t.budget_plan(),
            t.budget_usage(),
            0,
            false,
        )
        .with_original_sources(t.original_sources().to_vec());
        Box::pin(async move { result })
    }
}

fn staged_fixture(raw: &[&str]) -> Result<TurnFixture, Box<dyn Error>> {
    staged_fixture_with_command(raw, None)
}

fn staged_fixture_with_command(
    raw: &[&str],
    command: Option<a3_domain::DiscoveredCommandId>,
) -> Result<TurnFixture, Box<dyn Error>> {
    let mut fixture = turn_fixture_with_command(
        raw.iter()
            .map(|raw| provider_response(raw))
            .collect::<Result<_, _>>()?,
        command,
    )?;
    let step = fixture
        .input
        .task_ledger()
        .step(fixture.input.current_step_id())
        .ok_or("missing step")?;
    let prepared = crate::AgentPromptContract::prepare_current_step(
        &fixture.profile,
        fixture.input.project(),
        step,
    )?;
    let (system, grounding, schema) = prepared.into_parts();
    let mut messages = vec![system];
    messages.extend(grounding);
    messages.extend(fixture.compiled.request().messages().iter().cloned());
    let request = ModelProviderRequest::new(fixture.profile.clone(), messages, Some(schema))?;
    let t = &fixture.compiled;
    fixture.compiled = CompiledAgentContext::new(
        request,
        t.policy_version(),
        t.digest(),
        t.goal_contract(),
        t.ledger_revision(),
        t.current_step_id(),
        t.index_run_id(),
        t.snapshot_id(),
        t.task_lens_digest(),
        t.run_memory_digest(),
        t.budget_plan(),
        t.budget_usage(),
        0,
        false,
    );
    Ok(fixture)
}

mod after_change;
mod source_guidance;

const SEARCH_CHOICE: &str = r#"{"version":1,"choice":"search"}"#;
const SEARCH_ARGUMENTS: &str = r#"{"version":1,"parameters":{"query":"increment","limit":3}}"#;

#[derive(Debug)]
struct StagedDeadlineProvider {
    inner: ScriptedProvider,
    timeouts: Mutex<Vec<Duration>>,
}

impl ModelProvider for StagedDeadlineProvider {
    fn provider_id(&self) -> &ModelProviderId {
        self.inner.provider_id()
    }

    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.timeouts
                .lock()
                .map_err(|_| crate::ModelProviderFailure::Unavailable)?
                .push(timeout.duration());
            self.inner.stream(request, timeout, control).await
        })
    }
}

#[test]
fn staged_deadline_is_shared_and_expired_run_cannot_start_choice() -> Result<(), Box<dyn Error>> {
    for remaining_millis in [0, 10_000] {
        let fixture = staged_fixture(&[SEARCH_CHOICE, SEARCH_ARGUMENTS])?;
        let compiler = RepeatStagedCompiler {
            template: fixture.compiled,
            calls: AtomicUsize::new(0),
            change_after: None,
        };
        let provider = StagedDeadlineProvider {
            inner: ScriptedProvider {
                provider_id: fixture.profile.provider_id().clone(),
                responses: Mutex::new(fixture.responses),
            },
            timeouts: Mutex::new(Vec::new()),
        };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();
        let observed_at = timestamp(
            fixture.run.created_at().unix_millis() + fixture.run.budget().duration_limit().millis()
                - remaining_millis,
        )?;
        // Exercise the exchange's own deadline, independently of the outer preflight.
        let result = futures::executor::block_on(super::super::staged::generate(
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery),
            &fixture.run,
            &fixture.input,
            &compiler.template,
            observed_at,
            &TestControl,
        ))?;
        let timeouts = provider.timeouts.lock().map_err(|_| "poison")?;
        if remaining_millis == 0 {
            let Err(rejected) = result else {
                return Err("expired exchange accepted".into());
            };
            assert_eq!(
                rejected.reason,
                AgentTurnRejectionReason::Staged(StagedActionFailure::Deadline)
            );
            assert_eq!(rejected.charge.prompt_tokens().get(), 0);
            assert!(timeouts.is_empty());
        } else {
            assert!(result.is_ok());
            assert_eq!(timeouts.len(), 2);
            assert!(timeouts[0] <= Duration::from_millis(remaining_millis));
            assert!(timeouts[1] <= timeouts[0]);
        }
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
    }
    Ok(())
}

#[test]
fn staged_every_advertised_choice_has_a_projectable_arguments_contract()
-> Result<(), Box<dyn Error>> {
    use super::staged_contract::{Arguments, choice_schema, decode_choice};
    let fixture = staged_fixture(&[])?;
    let base = fixture
        .compiled
        .request()
        .structured_output()
        .ok_or("schema")?
        .value();
    let choices = choice_schema();
    let choices = choices
        .pointer("/properties/choice/enum")
        .and_then(serde_json::Value::as_array)
        .ok_or("choices")?;
    assert_eq!(choices.len(), 16);
    for choice in choices {
        let choice = decode_choice(&serde_json::json!({"version":1,"choice":choice}).to_string())
            .ok_or("advertised choice cannot be decoded")?;
        let args = Arguments::new(base, choice).ok_or("advertised choice cannot be projected")?;
        assert!(StructuredOutputSchema::new(args.schema).is_ok());
    }
    Ok(())
}

#[test]
fn staged_non_stop_and_missing_completion_do_not_repair_or_execute() -> Result<(), Box<dyn Error>> {
    for in_arguments in [false, true] {
        for reason in [
            None,
            Some(ModelFinishReason::OutputLimit),
            Some(ModelFinishReason::Other),
        ] {
            let mut fixture = staged_fixture(&[])?;
            if in_arguments {
                fixture
                    .responses
                    .push_back(provider_response(SEARCH_CHOICE)?);
            }
            fixture.responses.push_back(match reason {
                None => vec![],
                Some(reason) => vec![
                    ProviderEvent::OutputText(ModelOutputChunk::try_from_string(
                        SEARCH_ARGUMENTS.to_owned(),
                    )?),
                    ProviderEvent::Completed(ModelProviderCompletion::new(
                        reason,
                        ModelProviderUsage::new(Some(100), Some(10)),
                    )),
                ],
            });
            let compiler = RepeatStagedCompiler {
                template: fixture.compiled,
                calls: AtomicUsize::new(0),
                change_after: None,
            };
            let provider = ScriptedProvider {
                provider_id: fixture.profile.provider_id().clone(),
                responses: Mutex::new(fixture.responses),
            };
            let tools = CountingReadTools {
                calls: AtomicUsize::new(0),
            };
            let recovery = TestRecoveryStore::default();
            let outcome = futures::executor::block_on(
                ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
                    .with_action_generation(AgentActionGeneration::SelectThenFill)
                    .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
            )?;
            let AgentTurnOutcome::Rejected(rejected) = outcome else {
                return Err("incomplete output executed".into());
            };
            assert!(rejected.charge.prompt_tokens().get() > 0);
            assert_eq!(rejected.charge.repair(), AgentTurnRepairUsage::None);
            assert_eq!(
                rejected.reason,
                match reason {
                    Some(r) => AgentTurnRejectionReason::IncompleteModelOutput(r),
                    None => AgentTurnRejectionReason::ModelFailed(
                        crate::ModelProviderFailure::InvalidResponse
                    ),
                }
            );
            assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
            assert!(provider.responses.lock().map_err(|_| "poison")?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn staged_second_call_cannot_spend_past_run_output_budget() -> Result<(), Box<dyn Error>> {
    let mut fixture = staged_fixture(&[SEARCH_CHOICE, SEARCH_ARGUMENTS])?;
    let old = &fixture.run;
    let default = old.budget();
    let budget = a3_domain::AgentRunBudget::new(
        default.turn_limit(),
        default.prompt_token_limit(),
        a3_domain::AgentTokenLimit::new(4096)?,
        default.action_limit(),
        default.duration_limit(),
        default.repair_limit(),
    );
    fixture.run = AgentRun::reconstruct(
        a3_domain::AgentRunIdentity::with_budget(
            old.id(),
            old.goal_contract(),
            old.task_ledger_revision(),
            old.model_profile(),
            budget,
        ),
        a3_domain::AgentRunMaterializedState::with_usage(
            old.state(),
            old.last_event_sequence(),
            old.current_snapshot_id(),
            old.usage(),
        ),
        a3_domain::AgentRunTiming::new(old.created_at(), old.updated_at()),
    )?;
    let compiler = RepeatStagedCompiler {
        template: fixture.compiled,
        calls: AtomicUsize::new(0),
        change_after: None,
    };
    let provider = ScriptedProvider {
        provider_id: fixture.profile.provider_id().clone(),
        responses: Mutex::new(fixture.responses),
    };
    let tools = CountingReadTools {
        calls: AtomicUsize::new(0),
    };
    let recovery = TestRecoveryStore::default();
    let outcome = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_action_generation(AgentActionGeneration::SelectThenFill)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;
    let AgentTurnOutcome::Rejected(rejected) = outcome else {
        return Err("budget exceeded".into());
    };
    assert_eq!(
        rejected.reason,
        AgentTurnRejectionReason::Staged(StagedActionFailure::BudgetExceeded)
    );
    assert_eq!(rejected.charge.prompt_tokens().get(), 100);
    assert_eq!(provider.responses.lock().map_err(|_| "poison")?.len(), 1);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[derive(Debug)]
struct StagedCancellation(std::sync::atomic::AtomicBool);
impl AgentControllerControl for StagedCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
impl ContextCompileControl for StagedCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    fn report_phase(&self, _: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}
impl ModelOperationControl for StagedCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(futures::future::pending())
    }
}
#[derive(Debug)]
struct CancellingStagedProvider<'a> {
    inner: ScriptedProvider,
    control: &'a StagedCancellation,
}
impl ModelProvider for CancellingStagedProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.inner.provider_id()
    }
    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        self.control.0.store(true, Ordering::SeqCst);
        self.inner.stream(request, timeout, control)
    }
}

#[test]
fn staged_cancel_after_choice_keeps_usage_and_prevents_arguments() -> Result<(), Box<dyn Error>> {
    let fixture = staged_fixture(&[SEARCH_CHOICE, SEARCH_ARGUMENTS])?;
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
    let tools = CountingReadTools {
        calls: AtomicUsize::new(0),
    };
    let recovery = TestRecoveryStore::default();
    let outcome = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_action_generation(AgentActionGeneration::SelectThenFill)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &control),
    )?;
    let AgentTurnOutcome::Rejected(rejected) = outcome else {
        return Err("cancel ignored".into());
    };
    assert_eq!(
        rejected.reason,
        AgentTurnRejectionReason::CancelledBeforeAction
    );
    assert_eq!(rejected.charge.prompt_tokens().get(), 100);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        provider.inner.responses.lock().map_err(|_| "poison")?.len(),
        1
    );
    Ok(())
}

#[test]
fn staged_actions_share_one_repair_and_only_final_admission_reads() -> Result<(), Box<dyn Error>> {
    for (raw, expected_calls, accepted, repaired) in [
        (vec![SEARCH_CHOICE, SEARCH_ARGUMENTS], 2, true, false),
        (vec!["{}", SEARCH_CHOICE, SEARCH_ARGUMENTS], 3, true, true),
        (vec![SEARCH_CHOICE, "{}", SEARCH_ARGUMENTS], 3, true, true),
        (
            vec!["{}", SEARCH_CHOICE, "{}", SEARCH_ARGUMENTS],
            3,
            false,
            true,
        ),
        (
            vec!["{}", "{}", SEARCH_CHOICE, SEARCH_ARGUMENTS],
            2,
            false,
            true,
        ),
        (
            vec![
                SEARCH_CHOICE,
                r#"{"version":1,"parameters":{"kind":"finish"}}"#,
                "{}",
            ],
            3,
            false,
            true,
        ),
    ] {
        let fixture = staged_fixture(&raw)?;
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
        let outcome = futures::executor::block_on(
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
                .with_action_generation(AgentActionGeneration::SelectThenFill)
                .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
        )?;
        let charge = match &outcome {
            AgentTurnOutcome::Executed(turn) => {
                assert!(accepted);
                assert_ne!(turn.context_digest, ContextDigest::from_bytes([11; 32]));
                turn.charge
            }
            AgentTurnOutcome::Rejected(turn) => {
                assert!(!accepted);
                turn.charge
            }
            other => return Err(format!("unexpected staged outcome {other:?}").into()),
        };
        assert_eq!(tools.calls.load(Ordering::SeqCst), usize::from(accepted));
        assert_eq!(
            recovery.begins.load(Ordering::SeqCst),
            usize::from(accepted)
        );
        assert_eq!(charge.prompt_tokens().get(), expected_calls * 100);
        assert_eq!(charge.output_tokens().get(), expected_calls * 10);
        assert_eq!(
            charge.repair(),
            if repaired {
                AgentTurnRepairUsage::One
            } else {
                AgentTurnRepairUsage::None
            }
        );
        let requests = provider.requests.lock().map_err(|_| "poison")?;
        assert_eq!(requests.len(), expected_calls as usize);
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
        let mut recorded = fixture.run;
        outcome.record(&mut recorded, event_id(6), timestamp(6)?)?;
        assert_eq!(recorded.usage().turn_count(), 1);
        assert_eq!(
            recorded.usage().prompt_tokens(),
            u64::from(expected_calls * 100)
        );
    }
    Ok(())
}

#[test]
fn staged_changed_context_stops_before_arguments_and_keeps_charge() -> Result<(), Box<dyn Error>> {
    let fixture = staged_fixture(&[SEARCH_CHOICE, SEARCH_ARGUMENTS])?;
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
    let outcome = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_action_generation(AgentActionGeneration::SelectThenFill)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;
    let AgentTurnOutcome::Rejected(rejected) = outcome else {
        return Err("stale exchange accepted".into());
    };
    assert_eq!(
        rejected.reason,
        AgentTurnRejectionReason::Staged(StagedActionFailure::ContextChanged)
    );
    assert_eq!(rejected.charge.prompt_tokens().get(), 100);
    assert_eq!(provider.responses.lock().map_err(|_| "poison")?.len(), 1);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn staged_projected_patch_is_narrow_but_independent_decoder_remains_strict()
-> Result<(), Box<dyn Error>> {
    use super::staged_contract::{Arguments, decode_choice};
    let fixture = staged_fixture(&[])?;
    let base = fixture
        .compiled
        .request()
        .structured_output()
        .ok_or("schema")?
        .value();
    for (choice, raw) in [
        ("search",SEARCH_ARGUMENTS.to_owned()),
        ("inspect_file",r#"{"version":1,"parameters":{"target":{"path":"increment.py","start_line":1,"line_count":4}}}"#.to_owned()),
        ("patch_update",format!(r#"{{"version":1,"parameters":{{"snapshot_id":"{}","rationale":"Correct increment","operations":[{{"path":"increment.py","expected_hash":"{}","content":"def increment(value):\n    return value + 1\n"}}]}}}}"#,snapshot(),"ab".repeat(32))),
    ] {
        let selected = decode_choice(&format!(r#"{{"version":1,"choice":"{choice}"}}"#)).ok_or("choice")?;
        let args = Arguments::new(base,selected).ok_or("arguments schema")?;
        let assembled = args.assemble(&raw).ok_or("assemble")?;
        assert!(matches!(DecodeAgentActionTurn::current().decode_primary(&assembled),AgentActionPrimaryOutcome::Accepted(_)));
        let mut tampered:serde_json::Value = serde_json::from_str(&raw)?;
        tampered["parameters"]["kind"] = serde_json::json!("finish");
        assert!(args.assemble(&tampered.to_string()).is_none());
        assert!(args.assemble(r#"{"version":1,"parameters":{},"extra":true}"#).is_none());
    }
    for raw in [
        r#"{"version":1,"choice":"shell"}"#,
        r#"{"version":2,"choice":"search"}"#,
        r#"{"version":1,"choice":"search","parameters":{}}"#,
    ] {
        assert!(decode_choice(raw).is_none());
    }
    let args =
        Arguments::new(base, decode_choice(SEARCH_CHOICE).ok_or("choice")?).ok_or("schema")?;
    let invalid = args
        .assemble(r#"{"version":1,"parameters":{"query":"increment","limit":1000}}"#)
        .ok_or("assemble")?;
    assert!(matches!(
        DecodeAgentActionTurn::current().decode_primary(&invalid),
        AgentActionPrimaryOutcome::RepairRequired(_)
    ));
    Ok(())
}

#[test]
fn staged_binding_rejects_malformed_compiler_schemas_without_indexing_panics()
-> Result<(), Box<dyn Error>> {
    let fixture = staged_fixture(&[])?;
    let current = fixture
        .compiled
        .request()
        .structured_output()
        .ok_or("schema")?
        .value();
    let values = || {
        [
            "run_id",
            "worktree_id",
            "snapshot_id",
            "step_id",
            "verification_spec_id",
        ]
        .map(|key| (key, "ab".repeat(32)))
    };
    assert!(super::staged_contract::bind_patch_anchors(current, values()).is_some());
    for value in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!({"$defs":[]}),
        serde_json::json!({"properties":{"schema_version":{"const":5}},"$defs":{"applyPatch":{"properties":[]}}}),
    ] {
        assert!(super::staged_contract::bind_patch_anchors(&value, values()).is_none());
    }
    let mut missing = current.clone();
    missing["$defs"]["applyPatch"]["properties"]
        .as_object_mut()
        .ok_or("fields")?
        .remove("step_id");
    assert!(super::staged_contract::bind_patch_anchors(&missing, values()).is_none());
    Ok(())
}

#[test]
fn staged_noop_patch_preserves_closed_failure_without_executing_invalid_output()
-> Result<(), Box<dyn Error>> {
    let content = a3_domain::PatchFileContent::try_from_bytes(
        b"def increment(value):\n    return value + 1\n".to_vec(),
    )?;
    let arguments = serde_json::json!({"version":1,"parameters":{"rationale":"Fix increment", "operations":[{
        "path":"increment.py", "expected_hash":blake3::hash(content.as_bytes()).to_hex().to_string(), "content":std::str::from_utf8(content.as_bytes())?
    }]}}).to_string();
    let fixture = staged_fixture(&[
        r#"{"version":1,"choice":"patch_update"}"#,
        &arguments,
        &arguments,
    ])?;
    let compiler = RepeatStagedCompiler {
        template: fixture.compiled,
        calls: AtomicUsize::new(0),
        change_after: None,
    };
    let provider = ScriptedProvider {
        provider_id: fixture.profile.provider_id().clone(),
        responses: Mutex::new(fixture.responses),
    };
    let tools = CountingReadTools {
        calls: AtomicUsize::new(0),
    };
    let recovery = TestRecoveryStore::default();
    let outcome = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_action_generation(AgentActionGeneration::SelectThenFill)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;
    let AgentTurnOutcome::Rejected(rejected) = outcome else {
        return Err("no-op became executable".into());
    };
    assert_eq!(
        rejected.reason,
        AgentTurnRejectionReason::Staged(StagedActionFailure::InvalidAction(
            crate::AgentActionDecodeError::InvalidPatchOperation(
                a3_domain::PatchOperationError::NoContentChange
            )
        ))
    );
    assert_eq!(rejected.charge.repair(), AgentTurnRepairUsage::One);
    assert_eq!(rejected.charge.prompt_tokens().get(), 300);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    assert_eq!(recovery.begins.load(Ordering::SeqCst), 0);
    Ok(())
}
