//! Regressions for the repeated router/server reads and lost continuation frontier.

use super::*;
use a3_application::{AskResearchDecisionNote, AskResearchDetail, AskResearchFindingKind};
use a3_domain::{
    ContentHash, FileRevision, IndexRunId, RepositoryPath, SnapshotId, SourcePosition, SourceRange,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[path = "../../../../fixtures/research-plan-v1/c604618_context.rs"]
mod plan_baseline;

#[test]
fn retained_units_are_source_owned_deduplicated_bounded_and_do_not_lock_focus() -> TestResult {
    use research_context::{CoveredRange, cover};
    for budget in [512, 1024, 2048, 4096, 8192] {
        let mut state = AskResearchWorkingSet::new(budget);
        let item = source(1, "flow.py", 103)?;
        let revision = item.revision().clone();
        let body = format!(
            "def first():\n    return 'ä'\n{}def last():\n    return '🦀'\n",
            "# filler\n".repeat(99)
        );
        state.render(&item, 1, &body, true);
        state.sources.push(item);
        for (start, end) in [(0, 2), (1, 2), (101, 103)] {
            cover(
                &mut state.retained_units,
                CoveredRange {
                    revision: revision.clone(),
                    start: SourcePosition::new(start, 0),
                    end: SourcePosition::new(end, 0),
                },
            );
        }
        let packet = state.model_evidence("Compare first and last", &[]);
        assert!(packet.len() <= budget);
        assert_eq!(packet.matches("return 'ä'").count(), 1);
        assert_eq!(packet.matches("return '🦀'").count(), 1);
        assert!(!packet.contains("# filler"));
        assert_eq!(state.current_delivery.len(), 2);
        state.commit_delivery();
        let before = state.progress_with_pending();
        assert_eq!(state.model_evidence("Compare first and last", &[]), packet);
        assert_eq!(state.progress_with_pending(), before);
        assert!(state.focus_cached(&AskResearchAction::InspectPath {
            path: "flow.py".to_owned(),
            start_line: 50
        }));
        let focused = state.evidence_window();
        assert!(focused.contains("ab Zeile 50"));
        assert!(focused.contains("# filler"));
        assert!(focused.len() <= budget);
        for (index, range) in state.current_delivery.iter().enumerate() {
            assert_eq!(range.revision, revision);
            assert!(
                !state.current_delivery[..index]
                    .iter()
                    .any(|old| old.start < range.end && old.end > range.start)
            );
        }
        assert_eq!(state.evidence_revision, 1, "no new read for refocusing");
    }
    Ok(())
}

#[test]
fn research_repair_reports_wrong_json_types_without_raw_data() {
    use research_model::{DecisionIssue, validate_decision};
    let valid = serde_json::json!({"schema_version":4,"decision":{"kind":"research","evidence_status":"incomplete","note":{"goal":"Trace","finding_kind":"hypothesis","finding":"Check","finding_source_refs":[],"gap":"Unknown","next_step":"Read"},"actions":[{"kind":"inspectPath","path":"file.py","start_line":1}]}});
    for (pointer, value, issue) in [
        (
            "/decision/note",
            serde_json::json!("PRIVATE_INVALID_TEXT"),
            DecisionIssue::Object,
        ),
        (
            "/decision/actions",
            serde_json::json!({}),
            DecisionIssue::Array,
        ),
        (
            "/decision/note/gap",
            serde_json::Value::Null,
            DecisionIssue::String,
        ),
    ] {
        let mut raw = valid.clone();
        if let Some(field) = raw.pointer_mut(pointer) {
            *field = value;
        }
        assert_eq!(
            validate_decision(&raw.to_string(), BeginResearchDecision::SearchAllowed, 1),
            Err(issue)
        );
        assert!(!issue.repair_hint(1).contains("PRIVATE_INVALID_TEXT"));
        assert!(issue.repair_hint(1).len() <= 768);
        assert_ne!(issue.code(), DecisionIssue::Stream.code());
    }
}

#[test]
fn retained_history_cannot_starve_a_new_active_file_in_a_small_packet() -> TestResult {
    let mut state = AskResearchWorkingSet::new(512);
    for ordinal in 1..=9 {
        let item = source(ordinal, &format!("file{ordinal}.py"), 3)?;
        let body = format!("def entry{ordinal}():\n    return {ordinal}\n");
        state.render(&item, 1, &body, false);
        if ordinal < 9 {
            state.retained_units.push(research_context::CoveredRange {
                revision: item.revision().clone(),
                start: SourcePosition::new(0, 0),
                end: SourcePosition::new(2, 0),
            });
        }
        state.sources.push(item);
    }
    let reads = state.evidence_revision;
    assert!(state.focus_cached(&AskResearchAction::InspectPath {
        path: "file9.py".to_owned(),
        start_line: 1
    }));
    let packet = state.evidence_window();
    assert!(packet.contains("def entry9():\n    return 9\n"));
    assert!(packet.len() <= 512);
    assert_eq!(state.evidence_revision, reads);
    Ok(())
}

#[test]
fn working_findings_survive_long_obsolete_gaps_with_intact_references() -> TestResult {
    for budget in [1024, 2048, 4096] {
        let mut state = AskResearchWorkingSet::new(budget);
        let item = source(1, "manager.py", 2)?;
        state.render(&item, 1, "def add_task(title):\n    save(title)\n", true);
        state.sources.push(item);
        state.record_note(
            "CSV planen",
            &AskResearchDecisionNote {
                goal: "CSV planen".to_owned(),
                finding_kind: AskResearchFindingKind::Observation,
                finding: "add_task speichert Aufgaben".to_owned(),
                source_ordinals: vec![1],
                gap: "Eine früher fehlende Schnittstelle erneut suchen. ".repeat(8),
                next_step: "Aktuelle Quelle prüfen".to_owned(),
            },
        )?;
        let packet = state.model_evidence("CSV planen", &[]);
        assert!(packet.contains("PUBLIC WORKING NOTES (not evidence"));
        assert!(packet.contains("[S1]"));
        assert!(packet.contains("Observation: add_task"));
        assert!(!packet.contains("Eine früher fehlende"));
        assert!(packet.len() <= budget);
    }
    Ok(())
}

#[test]
fn sufficient_plan_requires_a_compilable_plan_and_explicit_questions_remain_valid() -> TestResult {
    use research_model::{DecisionIssue, validate_decision, validate_outcome};
    for (markdown, sufficient, valid) in [
        ("PLAN:\n## Summary\nHalbfertig 【S1】", true, false),
        (
            "PLAN:\n## Summary\nBereit 【S1】\n## Implementation Changes\n## Interfaces\nKeine\n## Test Plan\n## Assumptions\nKeine",
            true,
            false,
        ),
        ("Ich brauche einen Neustart. 【S1】", true, false),
        (
            "PLAN:\n## Summary\nBereit 【S1】\n## Implementation Changes\n1. Import ergänzen.\n## Interfaces\n## Test Plan\n1. Prüfen.\n## Assumptions\nKeine",
            true,
            false,
        ),
        (
            "PLAN:\nQuelle 【S1】\n```markdown\n## Summary\nBereit\n## Implementation Changes\n1. Import ergänzen.\n## Interfaces\nNeu.\n## Test Plan\n1. Prüfen.\n## Assumptions\nKeine\n```",
            true,
            false,
        ),
        (
            "PLAN:\n## Summary\nBereit 【S1】\n## Implementation Changes\n1. Import ergänzen.\n## Interfaces\nNeue CSV-Spalten.\n## Test Plan\n1. Zeilen validieren.\n## Assumptions\nCSV ist eine neue Schnittstelle.",
            true,
            true,
        ),
        ("PLAN:\nNoch offen. 【S1】", false, true),
    ] {
        let raw = serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":if sufficient {"sufficient"} else {"incomplete"},"markdown":markdown,"source_refs":["S1"],"note":{"goal":"Planen","finding_kind":"hypothesis","finding":"Noch prüfen","finding_source_refs":[],"gap":"Prüfen","next_step":"Planen"}}}).to_string();
        let decision = validate_decision(&raw, BeginResearchDecision::SearchAllowed, 1)
            .map_err(|_| "decode")?;
        assert!(validate_outcome(decision.clone(), AgentSessionMode::Ask).is_ok());
        for mode in [AgentSessionMode::Plan, AgentSessionMode::Agent] {
            let result = validate_outcome(decision.clone(), mode);
            assert_eq!(result.is_ok(), valid, "{mode:?}: {markdown}");
            if !valid {
                assert_eq!(result, Err(DecisionIssue::PlanShape));
            }
        }
    }
    Ok(())
}

#[test]
fn plan_research_keeps_all_requested_interfaces_and_the_complete_goal() -> TestResult {
    let query = "Erstelle einen CLI-Befehl python main.py import-csv <filepath.csv>, der Aufgaben validiert und ausschließlich über den TaskFlowManager speichert; bestehende Aufgaben bleiben erhalten.";
    for budget in [2048, 4096, 8192] {
        let mut state = AskResearchWorkingSet::new(budget);
        let mut actions = Vec::new();
        for (ordinal, path, api) in [
            (1, "main.py", "cli_main()"),
            (2, "taskflow/cli.py", "def cli_main():"),
            (
                3,
                "taskflow/manager.py",
                "def add_task(title, description):",
            ),
            (4, "taskflow/models.py", "class Task:"),
        ] {
            let body = format!(
                "{}\n{api}\n{}",
                "# earlier code\n".repeat(20),
                "# later code\n".repeat(200)
            );
            let item = source(ordinal, path, 223)?;
            state.render(&item, 1, &body, false);
            state.sources.push(item);
            actions.push(AskResearchAction::InspectPath {
                path: path.to_owned(),
                start_line: 22,
            });
        }
        state.focus_actions(&actions);
        let baseline = state.legacy_plan_evidence_window(&[], budget);
        let packet = state.model_evidence(query, &[]);
        assert!(
            packet.contains(query),
            "complete task must survive at {budget} bytes"
        );
        assert!(packet.len() <= budget);
        for api in [
            "cli_main()",
            "def cli_main():",
            "def add_task(title, description):",
            "class Task:",
        ] {
            assert!(packet.contains(api), "missing {api} at {budget} bytes");
        }
        assert_eq!(state.current_delivery.len(), 4);
        let old_visible = [
            "cli_main()",
            "def cli_main():",
            "def add_task(title, description):",
            "class Task:",
        ]
        .iter()
        .filter(|api| baseline.contains(**api))
        .count();
        eprintln!(
            "plan-window fixture: {budget} bytes; c604618 {old_visible}/4 interfaces, current 4/4; current packet {} bytes; no adaptive reads",
            packet.len()
        );
        if budget <= 4096 {
            assert!(old_visible < 4);
        }
    }
    Ok(())
}

#[test]
fn precise_api_lines_and_source_references_can_revisit_delivered_evidence() -> TestResult {
    let mut state = AskResearchWorkingSet::new(1024);
    let item = source(1, "cli.py", 200)?;
    state.render(
        &item,
        1,
        &format!("# CLI\ndef cli_main():\n{}", "    pass\n".repeat(198)),
        true,
    );
    state.sources.push(item);
    state.evidence_window();
    state.commit_delivery();
    for action in [
        AskResearchAction::InspectPath {
            path: "cli.py".to_owned(),
            start_line: 2,
        },
        AskResearchAction::InspectSource(1),
    ] {
        assert!(state.focus_cached(&action));
        assert!(state.evidence_window().contains("def cli_main():"));
        assert_eq!(state.evidence_revision, 1);
    }
    Ok(())
}

#[test]
fn repeated_cached_line_read_advances_past_previously_delivered_lines() -> TestResult {
    let mut state = AskResearchWorkingSet::new(1024);
    let item = source(1, "cli.py", 200)?;
    state.render(&item, 1, &"# some code here\n".repeat(200), true);
    state.sources.push(item);
    state.evidence_window();
    let frontier = state.current_delivery[0].end;
    state.commit_delivery();
    assert!(state.focus_cached(&AskResearchAction::InspectPath {
        path: "cli.py".to_owned(),
        start_line: 1
    }));
    state.evidence_window();
    assert_eq!(state.current_delivery[0].start, frontier);
    assert_eq!(state.evidence_revision, 1);
    Ok(())
}

#[path = "../../../../fixtures/research-progressive-v1/legacy_context.rs"]
mod legacy_context;

#[test]
fn progressive_cache_delivers_late_code_with_exact_utf8_ranges_without_new_reads() -> TestResult {
    let manager = include_str!("../../../../fixtures/research-progressive-v1/taskflow/manager.py");
    assert_eq!(manager.lines().count(), 143);
    for budget in [1024, 2048, 4096, 8192] {
        let item = source(1, "taskflow/manager.py", 143)?;
        let revision = item.revision().clone();
        let mut state = AskResearchWorkingSet::new(budget);
        state.render(&item, 1, manager, true);
        state.sources.push(item);
        let read_before = state.evidence_revision;
        let initial = state.model_evidence("Erkläre die Aufgabenerstellung", &[]);
        assert!(initial.len() <= budget);
        state.commit_delivery();
        // The old beginning-only packet misses the actual call site at these small budgets.
        assert!(!initial.contains("self.plugins.dispatch"));
        let action = AskResearchAction::InspectPath {
            path: "taskflow/manager.py".to_owned(),
            start_line: 130,
        };
        assert!(state.focus_cached(&action));
        let before_upgrade = state.legacy_evidence_window(&[], budget);
        assert!(!before_upgrade.contains("self.plugins.dispatch"));
        let packet = state.model_evidence("Erkläre die Aufgabenerstellung", &[]);
        assert!(packet.len() <= budget);
        assert!(packet.contains("self.plugins.dispatch"), "budget {budget}");
        assert_eq!(state.evidence_revision, read_before);
        assert_eq!(state.current_delivery.len(), 1);
        assert_eq!(state.current_delivery[0].start, SourcePosition::new(129, 0));
        state.commit_delivery();

        // Shifted overlapping ranges of the SAME revision are not new source coverage.
        let overlap = AskResearchSource::new(
            turn()?.session_id(),
            turn()?.user_sequence(),
            AskResearchSourceId::from_bytes([9; 32]),
            2,
            revision,
            None,
            None,
            AskResearchSourceKind::File,
            AskResearchSelectionReason::ExactNameOrPath,
        )?;
        state.render(
            &overlap,
            7,
            &manager.lines().skip(6).collect::<Vec<_>>().join("\n"),
            true,
        );
        state.sources.push(overlap);
        assert_eq!(state.evidence_revision, read_before);
        assert_eq!(
            state
                .model_evidence("Aufgabenerstellung", &[])
                .matches("self.plugins.dispatch")
                .count(),
            1
        );
    }

    let item = source(1, "wide.py", 1)?;
    let body = "ä🦀".repeat(1000);
    let mut state = AskResearchWorkingSet::new(1024);
    state.render(&item, 1, &body, true);
    state.sources.push(item);
    let first = state.evidence_window();
    assert!(first.len() <= 1024);
    let boundary = state.current_delivery[0].end;
    assert_eq!(boundary.row(), 0);
    assert!(body.is_char_boundary(boundary.column() as usize));
    state.commit_delivery();
    assert!(state.focus_cached(&AskResearchAction::InspectPath {
        path: "wide.py".to_owned(),
        start_line: 1
    }));
    let second = state.evidence_window();
    assert!(second.len() <= 1024);
    assert_eq!(state.current_delivery[0].start, boundary);
    assert_eq!(state.evidence_revision, 1);
    Ok(())
}

#[test]
fn delivery_interval_union_is_revision_bound_and_order_independent() -> TestResult {
    use research_context::{CoveredRange, cover};
    let revision = source(1, "manager.py", 143)?.revision().clone();
    let mut ranges = Vec::new();
    for (start, end) in [(30, 40), (10, 20), (20, 30), (12, 19)] {
        cover(
            &mut ranges,
            CoveredRange {
                revision: revision.clone(),
                start: SourcePosition::new(start, 0),
                end: SourcePosition::new(end, 0),
            },
        );
    }
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start.row(), 10);
    assert_eq!(ranges[0].end.row(), 40);
    let duplicate = ranges[0].clone();
    assert!(!cover(&mut ranges, duplicate));
    Ok(())
}

#[test]
fn decision_repair_distinguishes_shape_sources_and_closed_reads_without_raw_replay() {
    use research_model::{DecisionIssue, validate_decision};
    let raw = serde_json::json!({"schema_version":4,"decision":{
        "kind":"research","evidence_status":"incomplete",
        "note":{"goal":"Read","finding_kind":"hypothesis","finding":"Check", "finding_source_refs":[],"gap":"Missing caller","next_step":"Read caller"},
        "actions":[{"kind":"inspectSource","source_ref":"S2"}]
    }}).to_string();
    assert_eq!(
        validate_decision(&raw, BeginResearchDecision::SearchAllowed, 1),
        Err(DecisionIssue::UnknownSource)
    );
    assert_eq!(
        validate_decision(&raw, BeginResearchDecision::FinalOnly, 2),
        Err(DecisionIssue::ReadsClosed)
    );
    assert!(validate_decision(&raw, BeginResearchDecision::SearchAllowed, 2).is_ok());
    assert!(
        DecisionIssue::ReadsClosed
            .repair_hint(2)
            .contains("sufficient or incomplete evidence_status")
    );
    assert_eq!(
        validate_decision(
            "raw invalid output",
            BeginResearchDecision::SearchAllowed,
            2
        ),
        Err(DecisionIssue::Json)
    );
    for issue in [
        DecisionIssue::Shape,
        DecisionIssue::UnknownSource,
        DecisionIssue::ReadsClosed,
    ] {
        let hint = issue.repair_hint(2);
        assert!(!hint.contains("raw invalid output"));
        assert!(hint.contains("No actions from the invalid output were executed"));
    }
}

#[test]
fn exhausted_repair_reports_the_actual_time_or_decision_limit() -> TestResult {
    let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
    assert_eq!(
        repair_stop_reason(&controller, 300_000, 0),
        ResearchStopReason::TimeLimit
    );
    assert_eq!(
        repair_stop_reason(&controller, 0, 0),
        ResearchStopReason::InvalidDecision
    );
    for time in 0..12 {
        controller.begin_decision(time)?;
    }
    assert_eq!(
        repair_stop_reason(&controller, 6, 0),
        ResearchStopReason::DecisionLimit
    );
    Ok(())
}

const STORAGE_FACTORY: &str =
    include_str!("../../../../fixtures/research-storage-v1/taskflow/storage/factory.py");
const STORAGE_CONFIG: &str =
    include_str!("../../../../fixtures/research-storage-v1/taskflow/config.py");
const STORAGE_INI: &str = include_str!("../../../../fixtures/research-storage-v1/config.ini");

#[test]
fn storage_selection_evidence_fits_without_reading_short_files_in_rounds() -> TestResult {
    let mut state = AskResearchWorkingSet::new(4_096);
    for (ordinal, path, body) in [
        (1, "taskflow/storage/factory.py", STORAGE_FACTORY),
        (2, "config.ini", STORAGE_INI),
        (3, "taskflow/config.py", STORAGE_CONFIG),
    ] {
        let item = source(ordinal, path, u32::try_from(body.lines().count())?)?;
        state.render(&item, 1, body, false);
        state.sources.push(item);
    }
    for ordinal in 4..=8 {
        let item = source(ordinal, &format!("unrelated/module{ordinal}.py"), 100)?;
        state.render(&item, 1, &"unrelated_detail = True\n".repeat(100), false);
        state.sources.push(item);
    }
    let evidence = state.model_evidence(
        "Wie wählen factory.py und config.ini SQLite, JSON und Memory aus?",
        &[],
    );
    let complete = [STORAGE_FACTORY, STORAGE_CONFIG, STORAGE_INI]
        .iter()
        .filter(|text| evidence.contains(**text))
        .count();
    eprintln!(
        "storage-research fixture: {complete}/3 complete relevant files, {} context bytes, 4096-byte budget",
        evidence.len()
    );
    assert!(evidence.contains(STORAGE_FACTORY));
    assert!(evidence.contains(STORAGE_CONFIG));
    assert!(evidence.contains(STORAGE_INI));
    assert!(evidence.len() <= 4_096);
    Ok(())
}

#[test]
fn converging_continuation_sources_keep_one_reference_and_require_the_full_fresh_chain()
-> TestResult {
    let first = AskResearchSourceId::from_bytes([1; 32]);
    let second = AskResearchSourceId::from_bytes([2; 32]);
    let current = AskResearchSourceId::from_bytes([3; 32]);
    let note = AskResearchPublicNote::new(
        "Explain storage".to_owned(),
        AskResearchPublicFindingKind::Conclusion,
        "The factory selects the configured driver".to_owned(),
        vec![first, second],
        "Check overrides".to_owned(),
        "Inspect config".to_owned(),
    )?;
    assert!(rebind_public_note(&note, &[(first, current)])?.is_none());
    let mapped = rebind_public_note(&note, &[(first, current), (second, current)])?
        .ok_or("expected revalidated finding")?;
    assert_eq!(mapped.source_ids(), &[current]);
    let mut state = AskResearchWorkingSet::new(4096);
    state.record_revalidated_note(
        "Explain storage",
        &mapped,
        mapped.source_ids().to_vec(),
        true,
    )?;
    assert_eq!(state.memory_findings[0].sources, vec![current]);
    assert_eq!(state.memory_gaps, ["Check overrides"]);
    Ok(())
}

#[test]
fn task_lens_receives_unique_resolved_paths_and_not_ambiguous_names() -> TestResult {
    let revision = source(1, "taskflow/storage/factory.py", 24)?
        .revision()
        .clone();
    let targets = vec![
        ResolvedQueryTarget {
            requested: "factory.py".to_owned(),
            revision: Some(revision.clone()),
        },
        ResolvedQueryTarget {
            requested: "taskflow/storage/factory.py".to_owned(),
            revision: Some(revision.clone()),
        },
        ResolvedQueryTarget {
            requested: "config.py".to_owned(),
            revision: None,
        },
    ];
    let query = "Explain storage selection in factory.py";
    let seeds = research_lens_seeds(query, &targets)?;
    assert_eq!(seeds.goal().as_str(), query);
    assert_eq!(seeds.step().as_str(), query);
    assert_eq!(
        seeds.supplemental(),
        &[a3_domain::TaskLensSeed::ExplicitPath(
            revision.path().clone()
        )]
    );
    Ok(())
}

#[test]
fn overlapping_short_file_sources_do_not_displace_named_branches() -> TestResult {
    let mut state = AskResearchWorkingSet::new(4096);
    let whole = source(1, "taskflow/storage/factory.py", 24)?;
    let tail = AskResearchSource::new(
        whole.session_id(),
        whole.user_sequence(),
        AskResearchSourceId::from_bytes([2; 32]),
        2,
        whole.revision().clone(),
        whole.range(),
        None,
        AskResearchSourceKind::File,
        AskResearchSelectionReason::ExactNameOrPath,
    )?;
    let tail_text = STORAGE_FACTORY
        .lines()
        .skip(8)
        .collect::<Vec<_>>()
        .join("\n");
    state.render(&whole, 1, STORAGE_FACTORY, false);
    state.render(&tail, 9, &tail_text, true);
    state.sources = vec![whole.clone(), tail];
    let packed = state.compile_evidence_window(&[whole.revision().clone()], 4096);
    assert!(packed.contains(STORAGE_FACTORY));
    assert_eq!(packed.matches("return MemoryStorage()").count(), 1);
    assert!(packed.contains("[S1]"));
    assert!(!packed.contains("[S2]"));
    assert_eq!(state.sources.len(), 2); // Presentation history is not modified by packing.
    let changed = source(3, "taskflow/storage/factory.py", 24)?;
    state.render(&changed, 9, &tail_text, true);
    state.sources.push(changed);
    assert!(state.evidence_window().contains("[S3]")); // Never merge across revisions.
    Ok(())
}

#[test]
fn the_complete_context_packet_respects_even_tiny_and_unicode_budgets() -> TestResult {
    for limit in [0, 20, 64, 128, 320, 1024, 4096] {
        let mut state = AskResearchWorkingSet::new(limit);
        let item = source(1, "src/überblick.py", 200)?;
        state.render(&item, 1, &"    return 'Größe 🦀'\n".repeat(200), true);
        state.sources.push(item);
        let first = state.model_evidence(&"Wie funktioniert Größe? ".repeat(200), &[]);
        assert!(first.len() <= limit, "{limit}: {}", first.len());
        assert_eq!(
            first,
            state.model_evidence(&"Wie funktioniert Größe? ".repeat(200), &[])
        );
    }
    Ok(())
}

#[test]
fn missing_attribution_repairs_once_without_reopening_reads() -> TestResult {
    let factory = source(1, "taskflow/storage/factory.py", 24)?;
    let config = source(2, "config.ini", 16)?;
    let required = vec![factory.revision().clone(), config.revision().clone()];
    let mut state = AskResearchWorkingSet::new(4096);
    state.sources = vec![factory, config];
    assert!(!answer_requires_deeper_research(
        AskResearchEvidenceStatus::Sufficient,
        state.covers(&required)
    ));
    assert!(!state.citations_cover(&[1], &required));
    assert!(state.citations_cover(&[1, 2], &required));
    assert!(!state.citations_cover(&[1, 3], &required));
    let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
    controller.begin_decision(0)?;
    controller.use_repair()?;
    let next = controller.begin_decision(1)?;
    assert_eq!(
        restrict_research_permission(BeginResearchDecision::FinalOnly, next),
        BeginResearchDecision::FinalOnly
    );
    // A later independent document retains its repair allowance; FinalOnly stays restrictive.
    assert!(reserve_research_repair_decision(&mut controller, 2, 0).is_some());
    state.sources.pop();
    assert!(answer_requires_deeper_research(
        AskResearchEvidenceStatus::Sufficient,
        state.covers(&required)
    ));
    Ok(())
}

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
    assert!(state.evidence_window().contains("Rest im Cache"));
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
    let feedback = state.context_feedback("Current read results");
    assert!(feedback.contains("Only configuration remains unclear"));
    assert!(feedback.contains("Read the relevant configuration"));
    assert!(feedback.contains("not evidence; may now be resolved"));
    assert!(!feedback.contains("server.py has not been read"));
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
    let result = awaiting_continuation(&turn()?, &state, None, ResearchStopReason::Stagnation)?;
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
