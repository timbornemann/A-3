//! Actual compiler contracts for source materialization and typed tool provenance.
use super::*;
use a3_application::{
    AgentReadResult, AgentSourcePage, AgentSourceReadControl, AgentSourceReadFailure,
    AgentSourceReader, AgentSourceReaderFuture, CompiledAgentContext, ContextToolResult,
    ContextToolResultDigest, ContextToolResultPreview, ContextToolResultStatus,
};
use a3_domain::{
    AgentFileInspection, AgentFileStartLine, AgentToolEvidenceSet, RunEventId, ToolRunId,
};

#[derive(Debug)]
struct Source {
    text: String,
    failure: Option<AgentSourceReadFailure>,
    invalid_range: bool,
    calls: Mutex<Vec<(RepositoryPath, u32, u16)>>,
}

impl Source {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            failure: None,
            invalid_range: false,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl AgentSourceReader for Source {
    fn read_page<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(AgentSourceReadFailure::Cancelled);
            }
            self.calls
                .lock()
                .map_err(|_| AgentSourceReadFailure::Unavailable)?
                .push((
                    request.path().clone(),
                    request.start_line().get(),
                    request.line_count().get(),
                ));
            if let Some(error) = self.failure {
                return Err(error);
            }
            let text = self
                .text
                .lines()
                .take(usize::from(request.line_count().get()))
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            let returned_lines = u32::try_from(text.lines().count())
                .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
            let has_more = self.text.lines().count() > usize::from(request.line_count().get());
            let start = request.start_line().get() - 1;
            let range = SourceRange::new(
                0,
                text.len() + usize::from(self.invalid_range),
                SourcePosition::new(start, 0),
                SourcePosition::new(start + returned_lines, 0),
            )
            .map_err(|_| AgentSourceReadFailure::InvalidPage)?;
            AgentSourcePage::new(
                revision.clone(),
                range,
                request.start_line(),
                text,
                has_more
                    .then(|| AgentFileStartLine::new(start + returned_lines + 1))
                    .transpose()
                    .map_err(|_| AgentSourceReadFailure::InvalidPage)?,
                has_more,
            )
            .map_err(|_| AgentSourceReadFailure::InvalidPage)
        })
    }
}

fn compile(
    source: &Source,
    input: &AgentContextCompileInput,
) -> Result<CompiledAgentContext, Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published,
        symbol_id: fixture.symbol_id,
        module_id: fixture.module_id,
        calls: &calls,
    };
    Ok(block_on(
        DeterministicAgentContextCompiler::new(
            CompileTaskLens::new(&store, &store, &store),
            source,
        )
        .compile(input, &RecordingControl::default()),
    )?)
}

fn pack(compiled: &CompiledAgentContext) -> String {
    compiled
        .request()
        .messages()
        .iter()
        .map(|m| m.content())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_counted(compiled: &CompiledAgentContext) -> Result<(), Box<dyn Error>> {
    let profile = compiled.request().profile();
    let actual = compiled
        .request()
        .messages()
        .iter()
        .try_fold(0u32, |sum, message| {
            profile
                .settings()
                .token_counting()
                .count_text(message.content())
                .map(|n| sum + n.get())
        })?;
    assert_eq!(actual, compiled.budget_usage().prompt_total());
    assert!(
        actual + compiled.budget_plan().output_reserve() + compiled.budget_plan().safety_reserve()
            <= compiled.budget_plan().context_limit()
    );
    Ok(())
}

#[test]
fn originals_are_current_counted_deterministic_and_not_duplicated_by_lens_metadata()
-> Result<(), Box<dyn Error>> {
    let input = input(Fixture::new()?.snapshot_id)?;
    let source = Source::new("fn compile_context() {\n    deliver_original();\n}");
    let first = compile(&source, &input)?;
    let second = compile(&source, &input)?;
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.request(), second.request());
    assert_eq!(first.original_sources(), second.original_sources());
    let delivered = first.original_sources();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].snapshot_id(), first.snapshot_id());
    assert_eq!(delivered[0].revision().path(), &path("src/context.rs")?);
    assert!(delivered[0].covers(
        first.snapshot_id(),
        &AgentFileInspection::new(
            path("src/context.rs")?,
            AgentFileStartLine::new(1)?,
            a3_domain::AgentFileLineCount::new(1)?,
        )
    ));
    let text = pack(&first);
    assert_eq!(
        text.matches("[ORIGINAL_SOURCE path=src/context.rs ")
            .count(),
        1
    );
    assert_eq!(text.matches("deliver_original();").count(), 1);
    assert!(text.contains("hash=0a0a0a0a"));
    assert_eq!(source.calls.lock().map_err(|_| "lock")?.len(), 2); // one per compile
    assert_counted(&first)?;
    let changed = compile(
        &Source::new("fn compile_context() {\n    different_body();\n}"),
        &input,
    )?;
    assert_ne!(first.digest(), changed.digest()); // digest includes bytes, not just metadata
    Ok(())
}

#[test]
fn source_reservation_precedes_optional_history_at_8k_and_16k() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    for (context, output) in [(8192, 2048), (16384, 4096)] {
        let (input, _, _) = input_with_run_memory_profile(
            &fixture,
            &"old summary ".repeat(300),
            profile_with_limits(context, output)?,
        )?;
        let compiled = compile(
            &Source::new("fn original() {\n    current_body();\n}"),
            &input,
        )?;
        assert!(pack(&compiled).contains("current_body();"));
        assert!(pack(&compiled).contains("[RUN_MEMORY]"));
        assert_counted(&compiled)?;
    }
    Ok(())
}

#[test]
fn expanded_repeated_schema_keeps_mandatory_anchors_and_reports_original_capacity()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let objective = "Keep open failures and all original goal constraints. ".repeat(18);
    for context in [16_384, 32_768] {
        let (input, _, _) = input_with_run_memory_goal(
            &fixture,
            "prior verified work",
            profile_with_grounding(
                context,
                2_048,
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            &objective,
        )?;
        let source = Source::new("fn compile_context() {\n    deliver_original();\n}");
        let compiled = compile(&source, &input)?;
        let text = pack(&compiled);
        assert!(text.contains(input.goal_contract().draft().objective().as_str()));
        assert!(text.contains("L0 repository snapshot="));
        assert!(text.contains("kind=verification_failed"));
        if context == 16_384 {
            // V6's exact repeated schema and this long goal consume the bounded capacity.
            // Never pretend that metadata delivered the original body, or drop mandatory memory.
            assert!(compiled.original_sources().is_empty());
            assert!(!text.contains("deliver_original();"));
            assert!(compiled.truncated());
        } else {
            assert!(
                text.contains("deliver_original();"),
                "optional metadata must not crowd out the current original"
            );
            assert!(!compiled.original_sources().is_empty());
        }
        assert_counted(&compiled)?;
        assert_eq!(compiled.digest(), compile(&source, &input)?.digest());
    }
    Ok(())
}

#[test]
fn oversized_page_is_not_partially_injected_or_replaced_by_old_preview()
-> Result<(), Box<dyn Error>> {
    let input = input(Fixture::new()?.snapshot_id)?;
    let compiled = compile(&Source::new(&"x".repeat(12000)), &input)?;
    assert!(!pack(&compiled).contains("[ORIGINAL_SOURCE "));
    assert!(compiled.original_sources().is_empty());
    assert!(compiled.truncated());
    assert_counted(&compiled)?;
    Ok(())
}

#[test]
fn staged_repeat_schema_keeps_current_originals_and_open_memory_at_16k_and_32k()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let objective = "Keep open failures and all original goal constraints. ".repeat(18);
    for context in [16_384, 32_768] {
        let (input, _, _) = input_with_run_memory_goal(
            &fixture,
            "prior verified work",
            profile_with_grounding(
                context,
                2_048,
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            &objective,
        )?;
        let input =
            input.with_action_generation(a3_application::AgentActionGeneration::SourceGuided);
        let source = Source::new("fn compile_context() {\n    deliver_original();\n}");
        let compiled = compile(&source, &input)?;
        let text = pack(&compiled);
        assert!(text.contains(input.goal_contract().draft().objective().as_str()));
        assert!(text.contains("kind=verification_failed"));
        assert!(text.contains("deliver_original();"));
        assert!(!compiled.original_sources().is_empty());
        assert_eq!(compiled.request().profile(), input.model_profile());
        assert!(
            compiled.budget_usage().prompt_total()
                + compiled.budget_plan().output_reserve()
                + compiled.budget_plan().safety_reserve()
                <= context
        );
        assert_eq!(compiled.digest(), compile(&source, &input)?.digest());
    }
    Ok(())
}

#[test]
fn oversized_multiline_original_is_retried_as_a_smaller_complete_page() -> Result<(), Box<dyn Error>>
{
    let base = input(Fixture::new()?.snapshot_id)?;
    let body = (0..80)
        .map(|line| format!("line_{line:02}_{}\n", "x".repeat(90)))
        .collect::<String>();
    let recorded_page = AgentSourcePage::new(
        FileRevision::new(path("src/context.rs")?, ContentHash::from_bytes([10; 32])),
        SourceRange::new(
            0,
            body.len(),
            SourcePosition::new(0, 0),
            SourcePosition::new(80, 0),
        )?,
        AgentFileStartLine::new(1)?,
        body.clone(),
        None,
        false,
    )?;
    let input = AgentContextCompileInput::new(
        base.project().clone(),
        base.goal_contract().clone(),
        base.task_ledger().clone(),
        base.current_step_id(),
        profile_with_limits(13_000, 2_048)?,
        None,
        Vec::new(),
        vec![recorded_original(&base, recorded_page)?],
    )?;
    let source = Source::new(&body);

    let compiled = compile(&source, &input)?;
    let calls = source.calls.lock().map_err(|_| "lock")?;

    assert!(
        calls.len() > 1,
        "the 64-line page should exceed its allowance: {calls:?}"
    );
    assert_eq!(calls[0].2, 64);
    assert!(calls.last().ok_or("source call")?.2 < 64);
    assert_eq!(compiled.original_sources().len(), 1);
    assert!(compiled.truncated());
    assert!(pack(&compiled).contains("line_00_"));
    assert!(!pack(&compiled).contains("line_79_"));
    assert_counted(&compiled)?;
    Ok(())
}

fn recorded_original(
    input: &AgentContextCompileInput,
    page: AgentSourcePage,
) -> Result<ContextToolResult, Box<dyn Error>> {
    let snapshot = Fixture::new()?.snapshot_id;
    let (mut run, _) = AgentRun::start(
        AgentRunId::from_bytes([71; 32]),
        input.goal_contract().reference(),
        input.task_ledger().revision(),
        input.model_profile().reference(),
        snapshot,
        RunEventId::from_bytes([72; 32]),
        AgentRunTimestamp::from_unix_millis(1)?,
    )?;
    let result = AgentReadResult::new(
        ToolRunId::from_bytes([73; 32]),
        ContextToolResultStatus::Succeeded,
        ContextToolResultPreview::try_from_string("bounded original result".to_owned())?,
        ContextToolResultDigest::from_bytes([74; 32]),
        false,
        snapshot,
        AgentToolEvidenceSet::new(snapshot, vec![page.evidence()])?,
        u64::try_from(page.text().len())?,
    )?
    .with_original_page(page)?;
    Ok(result
        .record(
            &mut run,
            RunEventId::from_bytes([75; 32]),
            AgentRunTimestamp::from_unix_millis(2)?,
        )?
        .into_parts()
        .1)
}

#[test]
fn stale_invalid_and_cancelled_readers_fail_closed_optional_failures_omit_sources()
-> Result<(), Box<dyn Error>> {
    let input = input(Fixture::new()?.snapshot_id)?;
    for (error, expected) in [
        (
            AgentSourceReadFailure::Stale,
            ContextCompileFailure::StaleOrMismatchedInput,
        ),
        (
            AgentSourceReadFailure::InvalidPage,
            ContextCompileFailure::InvalidPack,
        ),
        (
            AgentSourceReadFailure::Cancelled,
            ContextCompileFailure::Cancelled,
        ),
    ] {
        let mut source = Source::new("not delivered");
        source.failure = Some(error);
        let failure = compile(&source, &input)
            .err()
            .ok_or("must reject before provider")?;
        assert_eq!(
            failure.downcast_ref::<ContextCompileFailure>(),
            Some(&expected)
        );
    }
    for error in [
        AgentSourceReadFailure::Denied,
        AgentSourceReadFailure::Unavailable,
        AgentSourceReadFailure::SecretCandidate,
        AgentSourceReadFailure::BinaryContent,
        AgentSourceReadFailure::FileTooLarge,
        AgentSourceReadFailure::InvalidEncoding,
        AgentSourceReadFailure::LineTooLong,
    ] {
        let mut source = Source::new("not delivered");
        source.failure = Some(error);
        let compiled = compile(&source, &input)?;
        assert!(compiled.truncated());
        assert!(!pack(&compiled).contains("not delivered"));
        assert!(compiled.original_sources().is_empty());
        assert!(!pack(&compiled).contains("[ORIGINAL_SOURCE "));
    }
    let mut source = Source::new("invalid range");
    source.invalid_range = true;
    assert_eq!(
        compile(&source, &input)
            .err()
            .ok_or("invalid range was accepted")?
            .downcast_ref::<ContextCompileFailure>(),
        Some(&ContextCompileFailure::InvalidPack)
    );
    Ok(())
}

#[test]
fn replan_localization_performs_zero_automatic_source_reads() -> Result<(), Box<dyn Error>> {
    let input = input(Fixture::new()?.snapshot_id)?.with_replan_localization(
        a3_domain::TaskReplanReason::try_from_string("locate missing helper".to_owned())?,
    );
    let mut source = Source::new("must never be read");
    source.failure = Some(AgentSourceReadFailure::Stale);
    let compiled = compile(&source, &input)?;
    assert!(source.calls.lock().map_err(|_| "lock")?.is_empty());
    assert!(compiled.original_sources().is_empty());
    assert!(!pack(&compiled).contains("[ORIGINAL_SOURCE "));
    assert!(pack(&compiled).contains("[REPLAN_LOCALIZATION]"));
    let checkpoint = a3_application::ReplanResearchCheckpoint::new(
        input.current_step_id(),
        Fixture::new()?.snapshot_id,
        input.replan_localization().ok_or("reason")?,
        "locate helper",
    )?;
    let page = AgentSourcePage::new(
        FileRevision::new(path("helper.py")?, ContentHash::from_bytes([42; 32])),
        SourceRange::new(0, 5, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
        AgentFileStartLine::new(1)?,
        "pass\n".to_owned(),
        None,
        false,
    )?;
    let analysis = input.with_replan_research(a3_application::ReplanResearchContext {
        checkpoint,
        pages: vec![page],
    })?;
    let compiled = compile(&source, &analysis)?;
    assert!(source.calls.lock().map_err(|_| "lock")?.is_empty());
    assert!(pack(&compiled).contains("pass\n"));
    assert!(!pack(&compiled).contains("[ORIGINAL_SOURCE "));
    Ok(())
}

fn recorded(
    input: &AgentContextCompileInput,
    paths: &[&str],
    marked: bool,
) -> Result<Vec<ContextToolResult>, Box<dyn Error>> {
    let snapshot = Fixture::new()?.snapshot_id;
    let (mut run, _) = AgentRun::start(
        AgentRunId::from_bytes([43; 32]),
        input.goal_contract().reference(),
        input.task_ledger().revision(),
        input.model_profile().reference(),
        snapshot,
        RunEventId::from_bytes([44; 32]),
        AgentRunTimestamp::from_unix_millis(1)?,
    )?;
    let mut results = Vec::new();
    for (i, name) in paths.iter().enumerate() {
        let page = AgentSourcePage::new(
            FileRevision::new(path(name)?, ContentHash::from_bytes([10; 32])),
            SourceRange::new(
                100,
                104,
                SourcePosition::new(9, 0),
                SourcePosition::new(10, 0),
            )?,
            AgentFileStartLine::new(10)?,
            "old\n".to_owned(),
            None,
            false,
        )?;
        let mut result = AgentReadResult::new(
            ToolRunId::from_bytes([i as u8 + 50; 32]),
            ContextToolResultStatus::Succeeded,
            ContextToolResultPreview::try_from_string(
                "old preview must not become original".to_owned(),
            )?,
            ContextToolResultDigest::from_bytes([51; 32]),
            false,
            snapshot,
            AgentToolEvidenceSet::new(snapshot, vec![page.evidence()])?,
            4,
        )?;
        if marked {
            result = result.with_original_page(page.clone())?;
            assert_eq!(result.take_original_page(), Some(page)); // marker survives handoff
        }
        let (_, context, _) = result
            .record(
                &mut run,
                RunEventId::from_bytes([i as u8 + 60; 32]),
                AgentRunTimestamp::from_unix_millis(i as u64 + 2)?,
            )?
            .into_parts();
        assert_eq!(context.original_source().is_some(), marked);
        results.push(context);
    }
    Ok(results)
}

#[test]
fn actual_read_markers_survive_take_are_prioritized_bounded_and_deduplicated()
-> Result<(), Box<dyn Error>> {
    let base = input(Fixture::new()?.snapshot_id)?;
    let results = recorded(&base, &["third.rs", "second.rs", "src/context.rs"], true)?;
    let input = AgentContextCompileInput::new(
        base.project().clone(),
        base.goal_contract().clone(),
        base.task_ledger().clone(),
        base.current_step_id(),
        base.model_profile().clone(),
        None,
        Vec::new(),
        results,
    )?;
    let source = Source::new("fresh_body();");
    let compiled = compile(&source, &input)?;
    let calls = source.calls.lock().map_err(|_| "lock")?;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], (path("src/context.rs")?, 10, 1));
    assert_eq!(calls[1], (path("second.rs")?, 10, 1));
    let text = pack(&compiled);
    assert_eq!(text.matches("fresh_body();").count(), 2);
    assert!(!text.contains("old preview must not become original"));
    assert!(compiled.truncated()); // third distinct source excluded, not silently complete
    assert_counted(&compiled)?;
    Ok(())
}

#[test]
fn search_span_metadata_cannot_create_original_provenance() -> Result<(), Box<dyn Error>> {
    let search_path = path("search_only.rs")?;
    let base = input(Fixture::new()?.snapshot_id)?;
    let results = recorded(&base, &["search_only.rs"], false)?;
    let input = AgentContextCompileInput::new(
        base.project().clone(),
        base.goal_contract().clone(),
        base.task_ledger().clone(),
        base.current_step_id(),
        base.model_profile().clone(),
        None,
        Vec::new(),
        results,
    )?;
    let source = Source::new("fresh_body();");
    let compiled = compile(&source, &input)?;
    assert!(
        source
            .calls
            .lock()
            .map_err(|_| "lock")?
            .iter()
            .all(|(p, _, _)| p != &search_path)
    );
    assert!(!pack(&compiled).contains("[ORIGINAL_SOURCE path=search_only.rs "));
    Ok(())
}
