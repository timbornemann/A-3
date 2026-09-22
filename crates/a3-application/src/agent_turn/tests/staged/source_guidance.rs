//! Source-guided routing and repeat admission use typed compiler output, not prompt parsing.
use super::*;
use crate::{AgentSourcePage, ContextOriginalSource, ContextToolResult};
use a3_domain::{
    AgentFileStartLine, ContentHash, FileRevision, IndexPublication, IndexRunId, IndexRunRecord,
    IndexRunSequence, IndexRunStatus, LinkedGraph, ModulePolicyVersion, ModuleProjection,
    ModuleSymbolSet, PublishedIndex, RankProjection, RankingPolicyVersion, RepositoryCard,
    RepositoryPath, SourcePosition, SourceRange,
};
use serde_json::json;

const READ: &str = r#"{"version":1,"next":"need_evidence"}"#;
const VERIFY: &str = r#"{"version":1,"next":"verify"}"#;
const CHANGE: &str = r#"{"version":1,"next":"change"}"#;
const FILE: &str = r#"{"version":1,"choice":"inspect_file"}"#;
const REPEAT: &str = r#"{"version":1,"parameters":{"target":{"path":"increment.py","start_line":1,"line_count":64}}}"#;
const OTHER: &str =
    r#"{"version":1,"parameters":{"target":{"path":"helper.py","start_line":1,"line_count":64}}}"#;

fn source(snapshot: SnapshotId, hash: u8) -> Result<ContextOriginalSource, Box<dyn Error>> {
    let page = AgentSourcePage::new(
        FileRevision::new(
            RepositoryPath::try_from_bytes(b"increment.py".to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ),
        SourceRange::new(0, 10, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
        AgentFileStartLine::new(1)?,
        "value = 1\n".to_owned(),
        None,
        false,
    )?;
    Ok(ContextOriginalSource::from_packed_page(snapshot, &page))
}

#[test]
fn source_work_routes_without_self_verification_and_redirects_redundant_reads_before_tool()
-> Result<(), Box<dyn Error>> {
    let patch = r#"{"version":1,"choice":"patch_update"}"#;
    let patch_args = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Implement the current requirement","operations":[{{"path":"increment.py","expected_hash":"{}","content":"value = 2\n"}}]}}}}"#,
        "ab".repeat(32)
    );
    for (raw, accepted, reads, repaired, kind) in [
        (vec![VERIFY], true, 0, false, "run"),
        (vec!["{}", VERIFY], true, 0, true, "run"),
        (vec![CHANGE, patch, &patch_args], true, 0, false, "patch"),
        (
            vec![READ, SEARCH_CHOICE, SEARCH_ARGUMENTS],
            true,
            1,
            false,
            "search",
        ),
        (vec![READ, FILE, OTHER], true, 1, false, "file"),
        (vec![READ, FILE, REPEAT, VERIFY], true, 0, false, "run"),
        (
            vec![READ, FILE, REPEAT, CHANGE, patch, &patch_args],
            true,
            0,
            false,
            "patch",
        ),
        (
            vec![READ, FILE, REPEAT, "{}", "{}"],
            false,
            0,
            true,
            "invalid",
        ),
    ] {
        let mut fixture = staged_fixture_with_command(
            &raw,
            Some(a3_domain::DiscoveredCommandId::from_bytes([90; 32])),
        )?;
        fixture.compiled = fixture
            .compiled
            .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
                .with_action_generation(AgentActionGeneration::SourceGuided)
                .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
        )?;
        assert_eq!(
            matches!(result, AgentTurnOutcome::Executed(_)),
            accepted,
            "{result:?}"
        );
        let charge = match &result {
            AgentTurnOutcome::Executed(execution) => {
                assert!(match execution.action() {
                    AgentAction::Run(_) => kind == "run",
                    AgentAction::ApplyPatch(_) => kind == "patch",
                    AgentAction::Search(_) => kind == "search",
                    AgentAction::Inspect(_) => kind == "file",
                    _ => false,
                });
                execution.charge()
            }
            AgentTurnOutcome::Rejected(rejected) => rejected.charge(),
            _ => return Err("unexpected outcome".into()),
        };
        let requests = provider.requests.lock().map_err(|_| "poison")?;
        assert_eq!(requests.len(), raw.len());
        assert_eq!(
            requests[0].structured_output().ok_or("schema")?.value()["title"],
            "A^3 SourceWork V1"
        );
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
        assert_eq!(
            charge.prompt_tokens().get(),
            u32::try_from(requests.len())? * 100
        );
        assert_eq!(
            charge.repair(),
            if repaired {
                AgentTurnRepairUsage::One
            } else {
                AgentTurnRepairUsage::None
            }
        );
        assert_eq!(tools.calls.load(Ordering::SeqCst), reads);
        assert_eq!(recovery.begins.load(Ordering::SeqCst), reads);
        assert!(
            fixture
                .input
                .task_ledger()
                .step(fixture.input.current_step_id())
                .ok_or("step")?
                .attempts()
                .last()
                .ok_or("attempt")?
                .verification()
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn source_work_requires_actual_current_delivery_and_operational_step() -> Result<(), Box<dyn Error>>
{
    use crate::agent_turn::source_guidance as decision;
    let command = a3_domain::DiscoveredCommandId::from_bytes([90; 32]);
    for (operational, delivery, expected) in [
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let fixture = staged_fixture_with_command(&[], operational.then_some(command))?;
        let sources = if delivery {
            vec![source(snapshot(), 0xab)?]
        } else {
            vec![]
        };
        assert_eq!(
            decision::planned_verification(&fixture.input, &fixture.run, &sources).is_some(),
            expected
        );
        assert_eq!(
            decision::available(&fixture.input, &fixture.run, &sources),
            delivery
        );
        assert!(
            decision::planned_verification(
                &fixture.input,
                &fixture.run,
                &[source(SnapshotId::from_bytes([77; 32]), 0xab)?]
            )
            .is_none()
        );
    }
    for raw in [
        r#"{"version":1,"next":"done"}"#,
        r#"{"version":2,"next":"change"}"#,
        r#"{"version":1,"next":"verify","passed":true}"#,
    ] {
        assert!(decision::decode(raw, true, true).is_none());
    }
    assert!(decision::decode(READ, false, false).is_none());
    assert_eq!(
        decision::schema(false, false)["properties"]["next"]["enum"],
        json!(["change"])
    );
    assert!(decision::prompt(false, false, true).contains("read budget"));
    let fixture = staged_fixture(&[])?;
    assert!(
        fixture
            .compiled
            .with_original_sources(vec![source(SnapshotId::from_bytes([77; 32]), 0xab)?])
            .is_err()
    );
    let fixture = staged_fixture(&[])?;
    assert!(
        fixture
            .compiled
            .with_original_sources(vec![source(snapshot(), 1)?, source(snapshot(), 2)?])
            .is_err()
    );
    assert_ne!(source(snapshot(), 1)?, source(snapshot(), 2)?);
    Ok(())
}

#[test]
fn source_work_routes_non_operational_change_steps_without_offering_verify()
-> Result<(), Box<dyn Error>> {
    let patch = r#"{"version":1,"choice":"patch_update"}"#;
    let patch_args = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Implement the current requirement","operations":[{{"path":"increment.py","expected_hash":"{}","content":"value = 2\n"}}]}}}}"#,
        "ab".repeat(32)
    );
    let mut fixture = staged_fixture_with_command(&[CHANGE, patch, &patch_args], None)?;
    fixture.compiled = fixture
        .compiled
        .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;

    assert!(matches!(
        result,
        AgentTurnOutcome::Executed(execution)
            if matches!(execution.action(), AgentAction::ApplyPatch(_))
    ));
    let requests = provider.requests.lock().map_err(|_| "poison")?;
    let first = requests.first().ok_or("source decision request")?;
    let choices = first.structured_output().ok_or("schema")?.value()["properties"]["next"]["enum"]
        .as_array()
        .ok_or("choice enum")?;
    assert_eq!(choices, &vec![json!("change"), json!("need_evidence")]);
    assert!(first.messages().iter().any(|message| {
        message
            .content()
            .contains("This step has no directly requestable command")
    }));
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn exhausted_reads_route_to_change_even_without_a_current_original() -> Result<(), Box<dyn Error>> {
    let add = r#"{"version":1,"choice":"patch_add"}"#;
    let add_args = r#"{"version":1,"parameters":{"rationale":"Implement the current requirement","operations":[{"path":"server.py","content":"print('hello')\n"}]}}"#;
    let mut fixture = staged_fixture_with_command(&[CHANGE, add, add_args], None)?;
    let tool_results = (1_u8..=4)
        .map(|id| {
            Ok(ContextToolResult::new(
                RunEventSequence::new(u64::from(id) + 10)?,
                ToolRunId::from_bytes([id; 32]),
                ContextToolResultStatus::Succeeded,
                ContextToolResultPreview::try_from_string("bounded read receipt".to_owned())?,
                ContextToolResultDigest::from_bytes([id; 32]),
                false,
                snapshot(),
                snapshot(),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    fixture.input = AgentContextCompileInput::new(
        fixture.input.project().clone(),
        fixture.input.goal_contract().clone(),
        fixture.input.task_ledger().clone(),
        fixture.input.current_step_id(),
        fixture.profile.clone(),
        None,
        Vec::new(),
        tool_results,
    )?;
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
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;

    assert!(matches!(
        result,
        AgentTurnOutcome::Executed(execution)
            if matches!(execution.action(), AgentAction::ApplyPatch(_))
    ));
    let requests = provider.requests.lock().map_err(|_| "poison")?;
    let first = requests.first().ok_or("bounded decision request")?;
    assert_eq!(
        first.structured_output().ok_or("schema")?.value()["properties"]["next"]["enum"],
        json!(["change"])
    );
    assert!(first.messages().iter().any(|message| {
        message
            .content()
            .contains("No further read may be requested in this attempt")
    }));
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn existing_add_is_deterministically_reconciled_to_a_current_revision_update()
-> Result<(), Box<dyn Error>> {
    let add = r#"{"version":1,"choice":"patch_add"}"#;
    let add_args = r#"{"version":1,"parameters":{"rationale":"Implement the current requirement","operations":[{"path":"increment.py","content":"value = 2\n"}]}}"#;
    let mixed_update_args = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Implement the current requirement","operations":[{{"kind":"update","path":"increment.py","expected_hash":"{}","content":"value = 2\n"}}]}}}}"#,
        "ab".repeat(32)
    );
    let mut fixture =
        staged_fixture_with_command(&[CHANGE, add, add_args, &mixed_update_args], None)?;
    fixture.compiled = fixture
        .compiled
        .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
    let published = published_with_increment()?;

    let result = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_patch_snapshot(&published)
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;

    let AgentTurnOutcome::Executed(execution) = result else {
        return Err(format!("repair did not produce an update: {result:?}").into());
    };
    let AgentAction::ApplyPatch(patch) = execution.action() else {
        return Err("current inventory reconciliation did not produce a patch".into());
    };
    assert!(matches!(
        patch.operations(),
        [a3_domain::PatchOperation::Update(_)]
    ));
    assert_eq!(execution.charge().repair(), AgentTurnRepairUsage::None);
    let requests = provider.requests.lock().map_err(|_| "poison")?;
    assert_eq!(requests.len(), 3);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn absent_update_is_deterministically_reconciled_to_an_add() -> Result<(), Box<dyn Error>> {
    let update = r#"{"version":1,"choice":"patch_update"}"#;
    let update_args = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Implement the current requirement","operations":[{{"path":"server.py","expected_hash":"{}","content":"print('hello')\n"}}]}}}}"#,
        "ab".repeat(32)
    );
    let mixed_add_args = r#"{"version":1,"parameters":{"rationale":"Implement the current requirement","operations":[{"kind":"add","path":"server.py","content":"print('hello')\n"}]}}"#;
    let mut fixture =
        staged_fixture_with_command(&[CHANGE, update, &update_args, mixed_add_args], None)?;
    fixture.compiled = fixture
        .compiled
        .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
    let published = published_with_increment()?;

    let result = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_patch_snapshot(&published)
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;

    let AgentTurnOutcome::Executed(execution) = result else {
        return Err(format!("repair did not produce an add: {result:?}").into());
    };
    let AgentAction::ApplyPatch(patch) = execution.action() else {
        return Err("current inventory reconciliation did not produce a patch".into());
    };
    assert!(matches!(
        patch.operations(),
        [a3_domain::PatchOperation::Add(_)]
    ));
    assert_eq!(execution.charge().repair(), AgentTurnRepairUsage::None);
    let requests = provider.requests.lock().map_err(|_| "poison")?;
    assert_eq!(requests.len(), 3);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn staged_update_binds_the_current_index_revision_instead_of_model_hash()
-> Result<(), Box<dyn Error>> {
    let update = r#"{"version":1,"choice":"patch_update"}"#;
    let update_args = format!(
        r#"{{"version":1,"parameters":{{"rationale":"Implement the current requirement","operations":[{{"path":"increment.py","expected_hash":"{}","content":"value = 2\n"}}]}}}}"#,
        "cd".repeat(32)
    );
    let mut fixture = staged_fixture_with_command(&[CHANGE, update, &update_args], None)?;
    fixture.compiled = fixture
        .compiled
        .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
    let published = published_with_increment()?;

    let result = futures::executor::block_on(
        ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
            .with_patch_snapshot(&published)
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
    )?;

    let AgentTurnOutcome::Executed(execution) = result else {
        return Err(format!("current revision binding rejected the update: {result:?}").into());
    };
    let AgentAction::ApplyPatch(patch) = execution.action() else {
        return Err("current revision binding did not produce a patch".into());
    };
    let a3_domain::PatchOperation::Update(update) = &patch.operations()[0] else {
        return Err("current revision binding did not retain update".into());
    };
    assert_eq!(
        update.expected().content_hash(),
        ContentHash::from_bytes([0xab; 32])
    );
    assert_eq!(execution.charge().repair(), AgentTurnRepairUsage::None);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

fn published_with_increment() -> Result<PublishedIndex, Box<dyn Error>> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"increment.py".to_vec())?,
        ContentHash::from_bytes([0xab; 32]),
    );
    let graph = LinkedGraph::new(
        snapshot(),
        vec![revision],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    let ranking = RankProjection::new(snapshot(), RankingPolicyVersion::v1(), Vec::new())?;
    let policy = ModulePolicyVersion::v1();
    let card = RepositoryCard::new(
        snapshot(),
        policy,
        Vec::new(),
        Vec::new(),
        ModuleSymbolSet::empty(),
        1,
        0,
    )?;
    let modules = ModuleProjection::new(snapshot(), policy, Vec::new(), Vec::new(), card)?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([12; 32]),
        snapshot(),
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(PublishedIndex::new(run, publication)?)
}

#[test]
fn source_work_context_change_after_decision_stops_before_tool_and_keeps_cost()
-> Result<(), Box<dyn Error>> {
    for metadata_only in [false, true] {
        let mut fixture = staged_fixture_with_command(
            &[READ, FILE, OTHER],
            Some(a3_domain::DiscoveredCommandId::from_bytes([90; 32])),
        )?;
        fixture.compiled = fixture
            .compiled
            .with_original_sources(vec![source(snapshot(), 0xab)?])?;
        let compiler = RepeatStagedCompiler {
            template: fixture.compiled,
            calls: AtomicUsize::new(0),
            change_after: if metadata_only { None } else { Some(1) },
        };
        let metadata = ChangedSourceCompiler(&compiler);
        let selected: &dyn AgentContextCompiler = if metadata_only { &metadata } else { &compiler };
        let provider = ScriptedProvider {
            provider_id: fixture.profile.provider_id().clone(),
            responses: Mutex::new(fixture.responses),
        };
        let tools = CountingReadTools {
            calls: AtomicUsize::new(0),
        };
        let recovery = TestRecoveryStore::default();
        let result = futures::executor::block_on(
            ExecuteAgentTurn::new(selected, &provider, &tools, &recovery)
                .with_action_generation(AgentActionGeneration::SourceGuided)
                .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
        )?;
        let AgentTurnOutcome::Rejected(rejected) = result else {
            return Err("changed context admitted".into());
        };
        assert_eq!(
            rejected.reason(),
            AgentTurnRejectionReason::Staged(StagedActionFailure::ContextChanged)
        );
        assert_eq!(rejected.charge().prompt_tokens().get(), 100);
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    }
    Ok(())
}

#[derive(Debug)]
struct ChangedSourceCompiler<'a>(&'a RepeatStagedCompiler);

impl AgentContextCompiler for ChangedSourceCompiler<'_> {
    fn compile<'a>(
        &'a self,
        input: &'a AgentContextCompileInput,
        control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        Box::pin(async move {
            let compiled = self.0.compile(input, control).await?;
            if self.0.calls.load(Ordering::SeqCst) > 1 {
                // Same messages and digest, but an inconsistent trusted compiler projection.
                compiled.with_original_sources(vec![
                    source(snapshot(), 0xac).map_err(|_| ContextCompileFailure::InvalidPack)?,
                ])
            } else {
                Ok(compiled)
            }
        })
    }
}

#[test]
fn source_work_delivery_does_not_switch_the_existing_comparison_strategies()
-> Result<(), Box<dyn Error>> {
    for generation in [
        AgentActionGeneration::SelectThenFill,
        AgentActionGeneration::ReviewThenSelect,
    ] {
        let mut fixture = staged_fixture_with_command(
            &[FILE, REPEAT],
            Some(a3_domain::DiscoveredCommandId::from_bytes([90; 32])),
        )?;
        fixture.compiled = fixture
            .compiled
            .with_original_sources(vec![source(snapshot(), 0xab)?])?;
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
        let result = futures::executor::block_on(
            ExecuteAgentTurn::new(&compiler, &provider, &tools, &recovery)
                .with_action_generation(generation)
                .execute(&fixture.run, &fixture.input, timestamp(5)?, &TestControl),
        )?;
        assert!(
            matches!(result, AgentTurnOutcome::Executed(_)),
            "{result:?}"
        );
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        assert!(provider.responses.lock().map_err(|_| "poison")?.is_empty());
    }
    Ok(())
}
