//! Source-guided routing and repeat admission use typed compiler output, not prompt parsing.
use super::*;
use crate::{AgentSourcePage, ContextOriginalSource};
use a3_domain::{
    AgentFileStartLine, ContentHash, FileRevision, RepositoryPath, SourcePosition, SourceRange,
};

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
fn source_work_routes_without_self_verification_and_rejects_redundant_file_before_tool()
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
        (vec![READ, FILE, REPEAT, OTHER], true, 1, true, "file"),
        (
            vec![READ, FILE, REPEAT, REPEAT],
            false,
            0,
            true,
            "duplicate",
        ),
        (
            vec!["{}", READ, FILE, REPEAT, OTHER],
            false,
            0,
            true,
            "duplicate",
        ),
        (
            vec![READ, FILE, REPEAT, SEARCH_ARGUMENTS],
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
            AgentTurnOutcome::Rejected(rejected) => {
                if kind == "duplicate" {
                    assert_eq!(
                        rejected.reason(),
                        AgentTurnRejectionReason::Staged(
                            StagedActionFailure::SourceAlreadySupplied
                        )
                    );
                }
                rejected.charge()
            }
            _ => return Err("unexpected outcome".into()),
        };
        let requests = provider.requests.lock().map_err(|_| "poison")?;
        assert_eq!(requests.len(), raw.len().min(4));
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
        assert!(decision::decode(raw).is_none());
    }
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
