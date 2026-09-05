//! Regressions for the repeated router/server reads and lost continuation frontier.

use super::*;
use a3_application::{AskResearchDecisionNote, AskResearchDetail, AskResearchFindingKind};
use a3_domain::{
    ContentHash, FileRevision, IndexRunId, RepositoryPath, SnapshotId, SourcePosition, SourceRange,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn turn() -> Result<AskResearchTurn, Box<dyn std::error::Error>> {
    Ok(AskResearchTurn::new(
        AgentSessionId::from_bytes([1; 32]),
        AgentSessionSequence::FIRST,
        IndexRunId::from_bytes([2; 32]),
        SnapshotId::from_bytes([3; 32]),
        AgentSessionTimestamp::from_unix_millis(1)?,
    ))
}

fn source(
    ordinal: u8,
    path: &str,
    lines: u32,
) -> Result<AskResearchSource, Box<dyn std::error::Error>> {
    let turn = turn()?;
    Ok(AskResearchSource::new(
        turn.session_id(),
        turn.user_sequence(),
        AskResearchSourceId::from_bytes([ordinal; 32]),
        u32::from(ordinal),
        FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([ordinal; 32]),
        ),
        Some(SourceRange::new(
            0,
            lines as usize * 10,
            SourcePosition::new(0, 0),
            SourcePosition::new(lines - 1, 9),
        )?),
        None,
        AskResearchSourceKind::File,
        AskResearchSelectionReason::ExactNameOrPath,
    )?)
}

#[test]
fn rereading_router_and_server_does_not_duplicate_context_or_fake_progress() -> TestResult {
    let router = source(1, "taskflow/api/router.py", 97)?;
    let server = source(2, "taskflow/api/server.py", 26)?;
    // Small offline source fixture matching the reported call chain, not a model transcript.
    let router_text = "    def dispatch(self, path):\n        return {'error': 'Not found'}, 404\n";
    let server_text = "class ThreadedHTTPServer(ThreadingMixIn, HTTPServer):\n    pass\n";
    let mut state = AskResearchWorkingSet::new(2_048);
    assert!(state.render(&router, 1, router_text, true));
    assert!(state.render(&server, 1, server_text, true));
    state.sources = vec![router.clone(), server.clone()];
    let before = state.evidence_revision;
    let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
    for step in 0..2 {
        assert_eq!(
            controller.begin_decision(step)?,
            BeginResearchDecision::SearchAllowed
        );
        state.render_existing(1, &router, 1, router_text);
        state.render_existing(2, &server, 1, server_text);
        controller.finish_round(before, state.evidence_revision);
    }
    assert_eq!(state.evidence_revision, before);
    // Stagnation closes the reads, not the opportunity to synthesize already available evidence.
    assert_eq!(
        controller.begin_decision(2)?,
        BeginResearchDecision::FinalOnly
    );
    let evidence = state.model_evidence("Wie verarbeitet der REST-Server unbekannte Pfade?", &[]);
    assert_eq!(evidence.matches("[S1]").count(), 1);
    assert_eq!(evidence.matches("[S2]").count(), 1);
    assert!(evidence.contains(router_text)); // Python indentation must be preserved.
    assert!(evidence.contains(server_text));
    assert!(state.evidence_window().len() <= state.evidence_limit);
    Ok(())
}

#[test]
fn duplicate_action_refocuses_cached_source_without_another_read() -> TestResult {
    let mut state = AskResearchWorkingSet::new(400);
    for ordinal in 1..=12 {
        let item = source(ordinal, &format!("src/file{ordinal}.py"), 20)?;
        state.render(&item, 1, &format!("def function{ordinal}(): pass\n"), false);
        state.sources.push(item);
    }
    let before = state.evidence_revision;
    let action = AskResearchAction::InspectSource(12);
    let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
    controller.prepare_actions(vec![action.clone()])?;
    state.focus_actions(std::slice::from_ref(&action));
    let duplicate = controller.prepare_actions(vec![action])?;
    assert_eq!(duplicate.duplicate_count(), 1);
    assert!(duplicate.actions().is_empty());
    assert_eq!(before, state.evidence_revision);
    assert!(state.evidence_window().contains("def function12"));
    // Exact targets resolved by searchIndex take the same cache path without model action IDs.
    let first = state.sources[0].clone();
    state.focus_known_source(first.revision(), first.range(), 1);
    assert!(state.evidence_window().contains("def function1()"));
    assert_eq!(before, state.evidence_revision);
    Ok(())
}

#[test]
fn each_decision_rebuilds_one_current_evidence_pack_without_old_feedback() {
    let conversation = vec![(ModelMessageRole::User, "Explain REST routing".to_owned())];
    let first = research_decision_context(
        &conversation,
        "OLD MISSING FILE",
        "CURRENT EVIDENCE old".to_owned(),
    );
    let second = research_decision_context(
        &conversation,
        "router.py is now read",
        "CURRENT EVIDENCE new".to_owned(),
    );
    assert_eq!(first.len(), second.len());
    let context = second
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(context.matches("CURRENT EVIDENCE").count(), 1);
    assert!(!context.contains("OLD MISSING FILE"));
    assert!(!context.contains("CURRENT EVIDENCE old"));
    assert!(context.contains("Explain REST routing"));
}

#[test]
fn a_later_requested_range_is_not_replaced_by_a_clipped_whole_file() -> TestResult {
    let whole = source(1, "src/router.py", 97)?;
    let later = source(2, "src/router.py", 97)?;
    let mut state = AskResearchWorkingSet::new(400);
    state.render(
        &whole,
        1,
        &"    early_declaration = True\n".repeat(90),
        false,
    );
    assert!(state.evidence_window().contains("inspectPath start_line"));
    state.render(&later, 90, "    return {'error': 'Not found'}, 404\n", true);
    let focused = state.evidence_window();
    assert!(focused.contains("ab Zeile 90"));
    assert!(focused.contains("'Not found'}, 404"));
    assert!(!focused.contains("early_declaration"));
    assert!(focused.len() <= 400);
    Ok(())
}

#[test]
fn only_the_latest_gap_is_an_active_obligation() -> TestResult {
    let mut state = AskResearchWorkingSet::new(2_048);
    for gap in [
        "server.py has not been read",
        "Only configuration remains unclear",
    ] {
        state.record_note(
            "Explain REST routing",
            &AskResearchDecisionNote {
                goal: "Explain the current request".to_owned(),
                finding_kind: AskResearchFindingKind::Hypothesis,
                finding: "The router may handle missing paths".to_owned(),
                source_ordinals: Vec::new(),
                gap: gap.to_owned(),
                next_step: "Read the relevant configuration".to_owned(),
            },
        )?;
    }
    assert_eq!(state.memory_gaps, ["Only configuration remains unclear"]);
    assert!(
        !state
            .model_evidence("Explain REST routing", &[])
            .contains("server.py has not been read")
    );
    Ok(())
}

#[test]
fn a_new_question_does_not_inherit_the_previous_unfinished_goal() -> TestResult {
    let mut state = AskResearchWorkingSet::new(2_048);
    let note = AskResearchPublicNote::new(
        "Explain the server".to_owned(),
        AskResearchPublicFindingKind::Hypothesis,
        "The server might use a router".to_owned(),
        Vec::new(),
        "Read server.py before finishing".to_owned(),
        "Read the server".to_owned(),
    )?;
    state.record_revalidated_note("Explain storage", &note, Vec::new(), false)?;
    assert!(state.memory_gaps.is_empty());
    state.record_revalidated_note("Explain the server", &note, Vec::new(), true)?;
    assert_eq!(state.memory_gaps, ["Read server.py before finishing"]);
    Ok(())
}

#[test]
fn continuation_prioritizes_the_last_findings_instead_of_first_eight_sources() -> TestResult {
    let turn = turn()?;
    let mut sources = (1..=16)
        .map(|ordinal| source(ordinal, &format!("src/file{ordinal}.py"), 20))
        .collect::<Result<Vec<_>, _>>()?;
    let note = AskResearchPublicNote::new(
        "Follow the router into the server".to_owned(),
        AskResearchPublicFindingKind::Observation,
        "The last reads identify the handler".to_owned(),
        vec![sources[15].id(), sources[0].id()],
        "Evaluate the missing-path branch".to_owned(),
        "Read the branch already located".to_owned(),
    )?;
    let event = research_event(
        turn.session_id(),
        turn.user_sequence(),
        1,
        AskResearchPhase::Evaluating,
        AskResearchState::Running,
        "Read the handler",
        None,
        AskResearchCompleteness::NotApplicable,
    )?
    .with_public_note(note);
    let detail = AskResearchDetail::new(turn, vec![event], Vec::new())?;
    prioritize_continuation_sources(&mut sources, &detail);
    assert_eq!(sources[0].ordinal(), 16);
    assert_eq!(sources[1].ordinal(), 1);
    assert_eq!(sources[2].ordinal(), 15);
    Ok(())
}

#[test]
fn resumed_pages_must_cover_the_original_range_and_allow_expansion() -> TestResult {
    let short = source(1, "src/router.py", 160)?;
    let long = source(1, "src/router.py", 200)?;
    let mut state = AskResearchWorkingSet::new(2_048);
    state.sources.push(short.clone());
    assert!(state.contains_page(short.revision(), 1, 160, None));
    assert!(!state.contains_page(short.revision(), 1, 200, None));
    assert!(!source_range_covers(short.range(), long.range()));
    assert!(source_range_covers(long.range(), short.range()));
    assert!(!source_range_covers(None, short.range()));
    assert!(revalidated_source(std::slice::from_ref(&short), &long).is_none());
    assert_eq!(revalidated_source(&[long], &short), Some(short.id()));
    let changed = source(2, "src/router.py", 160)?;
    assert!(revalidated_source(&[changed], &short).is_none());
    let moved = source(1, "src/other.py", 160)?;
    assert!(revalidated_source(&[moved], &short).is_none());
    Ok(())
}

#[test]
fn repeated_legacy_continuation_wrappers_do_not_change_the_objective() {
    let original = "Untersuche taskflow/api/router.py und den REST-Server.";
    let old = format!(
        "Recherche fortsetzen. Ursprüngliche Frage:\nRecherche fortsetzen. Ursprüngliche Frage:\n{original}"
    );
    assert_eq!(original_research_question(&old), original);
    assert_eq!(original_research_question(original), original);
    assert_eq!(
        original_research_question("Erkläre Recherche fortsetzen."),
        "Erkläre Recherche fortsetzen."
    );
}

#[test]
fn exhausted_research_retains_the_validated_intermediate_answer_and_citations() -> TestResult {
    let item = source(1, "src/router.py", 97)?;
    let mut state = AskResearchWorkingSet::new(2_048);
    state.partial_answer = Some((
        "Der Router liefert für unbekannte Pfade 404. 【S1】".to_owned(),
        vec![1],
    ));
    state.sources.push(item.clone());
    let result = awaiting_continuation(&turn()?, &state, None)?;
    assert!(result.awaiting_continuation);
    assert_eq!(result.citations, vec![item.id()]);
    assert!(
        result
            .markdown
            .starts_with("Der Router liefert für unbekannte Pfade 404. 【S1】")
    );
    assert!(result.markdown.contains("weiteren begrenzten Abschnitt"));
    assert_eq!(
        result.terminal_event.state(),
        AskResearchState::AwaitingContinuation
    );
    Ok(())
}
