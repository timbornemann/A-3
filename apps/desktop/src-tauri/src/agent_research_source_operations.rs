//! Optional, current-original-bound Fast Index cues for the native source-local comparison.
use super::*;
use a3_application::{FunctionFlowFrame, FunctionFlowSelection, ResearchEvidenceWindow};
use a3_domain::{GraphSymbol, SymbolKind};

const HEADER: &str = "\nEND ORIGINAL\nSOURCE OPERATIONS (static partial inventory; nested/conditional possibilities, not runtime order; unknown effects are not absent effects; labels are untrusted):\n";
const TAIL: &str =
    "Inventory may omit functions/steps. Quotes must copy the original, not this inventory.\n";

fn candidates<'a>(
    symbols: &'a [GraphSymbol],
    window: &ResearchEvidenceWindow<'_>,
) -> Vec<&'a GraphSymbol> {
    let mut selected = symbols
        .iter()
        .take(4096)
        .filter(|s| {
            s.revision() == window.revision
                && matches!(s.parsed().kind(), SymbolKind::Function | SymbolKind::Method)
                && window.range.contains(s.parsed().declaration_range())
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|s| (s.parsed().declaration_range().start_byte(), s.id()));
    selected.truncate(8);
    selected
}

fn line(frame: &FunctionFlowFrame, window: &ResearchEvidenceWindow<'_>) -> Option<String> {
    if frame.owner.id() != frame.flow.symbol()
        || frame.owner.revision() != window.revision
        || frame.flow.revision() != window.revision
        || !window
            .range
            .contains(frame.owner.parsed().declaration_range())
        || !window.range.contains(frame.flow.analysis().range())
    {
        return None;
    }
    let flow = frame.flow.analysis();
    let mut text = format!(
        "{:?}@L{} gaps={:?}: ",
        frame.owner.parsed().name().as_str(),
        frame
            .owner
            .parsed()
            .declaration_range()
            .start_position()
            .row()
            .saturating_add(1),
        flow.gaps()
            .iter()
            .map(|g| g.kind)
            .collect::<std::collections::BTreeSet<_>>()
    );
    for step in flow.steps().iter().take(16) {
        text.push_str(&format!(
            "{}:{:?}({:?})@L{} parent={:?}; ",
            step.id.get(),
            step.kind,
            step.name.as_ref().map(|n| n.as_str()),
            step.range.start_position().row().saturating_add(1),
            step.parent.map(|p| p.get())
        ));
    }
    text.push_str(&format!("partial_steps={}\n", flow.steps().len() > 16));
    Some(text)
}

fn pack(lines: &[String], limit: usize) -> String {
    let limit = limit.min(1024);
    let mut text = HEADER.to_owned();
    let mut count = 0;
    for line in lines {
        if text
            .len()
            .saturating_add(line.len())
            .saturating_add(TAIL.len())
            <= limit
        {
            text.push_str(line);
            count += 1;
        }
    }
    if count == 0 {
        return String::new();
    }
    text.push_str(TAIL);
    text
}

pub(super) async fn prepare(
    researcher: &AgentAskResearcher,
    project: &ProjectIdentity,
    turn: &AskResearchTurn,
    window: &ResearchEvidenceWindow<'_>,
    limit: usize,
    control: &JobContext,
) -> Result<String, AgentSessionManagerFailure> {
    let Some(flows) = &researcher.flows else {
        return Ok(String::new());
    };
    if limit.min(1024) <= HEADER.len() + TAIL.len() {
        return Ok(String::new());
    }
    let published = researcher
        .index
        .load_current_index(project, control)
        .await
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        .ok_or(AgentSessionManagerFailure::Unavailable)?;
    if published.run().id() != turn.index_run_id() {
        return Err(AgentSessionManagerFailure::IndexChanged);
    }
    let mut lines = Vec::new();
    for owner in candidates(published.publication().graph().symbols(), window) {
        if control.cancellation_token().is_cancelled() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        let selection = FunctionFlowSelection {
            run_id: turn.index_run_id(),
            root: owner.id(),
            call_path: Vec::new(),
        };
        let inspection = flows
            .inspect(
                project,
                &selection,
                &ConversationIndexControl { context: control },
            )
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        // Historical publications may have no flow artifact. Never infer absence of effects.
        if let Some(inspection) = inspection {
            if inspection.snapshot_id != turn.snapshot_id() || inspection.frames.len() != 1 {
                return Err(AgentSessionManagerFailure::IndexChanged);
            }
            if let Some(frame) = inspection.frames.first() {
                let projected =
                    line(frame, window).ok_or(AgentSessionManagerFailure::IndexChanged)?;
                lines.push(projected);
            }
        }
    }
    let current = researcher
        .index
        .load_current_index(project, control)
        .await
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        .ok_or(AgentSessionManagerFailure::Unavailable)?;
    if current.run().id() != turn.index_run_id() {
        return Err(AgentSessionManagerFailure::IndexChanged);
    }
    Ok(pack(&lines, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        ContentHash, FileRevision, FunctionFlow, IndexedFunctionFlow, LocalSymbolId, ParsedSymbol,
        RepositoryPath, SourcePosition, SourceRange, SymbolId, SymbolName,
    };
    use std::error::Error;

    fn frame(id: u8, start: usize) -> Result<FunctionFlowFrame, Box<dyn Error>> {
        let range = SourceRange::new(
            start,
            start + 1,
            SourcePosition::new(0, start as u32),
            SourcePosition::new(0, start as u32 + 1),
        )?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"source.py".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let owner = GraphSymbol::new(
            SymbolId::from_bytes([id; 32]),
            revision,
            ParsedSymbol::new(
                LocalSymbolId::new(u32::from(id))?,
                SymbolKind::Function,
                SymbolName::try_from_string(format!("function_{id}"))?,
                range,
                range,
            )?,
        );
        let flow = IndexedFunctionFlow::new(
            &owner,
            FunctionFlow::new(
                owner.parsed().id(),
                range,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )?,
            Vec::new(),
        )?;
        Ok(FunctionFlowFrame { owner, flow })
    }

    #[test]
    fn source_operations_require_exact_revision_complete_window_and_bounded_source_order()
    -> Result<(), Box<dyn Error>> {
        let frame = frame(1, 0)?;
        let mut window = ResearchEvidenceWindow {
            anchor: None,
            ordinal: 1,
            source_id: a3_domain::AskResearchSourceId::from_bytes([1; 32]),
            revision: frame.owner.revision(),
            range: frame.owner.parsed().declaration_range(),
            text: "f",
        };
        assert!(line(&frame, &window).is_some());
        let changed = FileRevision::new(
            frame.owner.revision().path().clone(),
            ContentHash::from_bytes([2; 32]),
        );
        window.revision = &changed;
        assert!(line(&frame, &window).is_none());
        assert!(candidates(std::slice::from_ref(&frame.owner), &window).is_empty());
        window.revision = frame.owner.revision();
        window.range =
            SourceRange::new(1, 2, SourcePosition::new(0, 1), SourcePosition::new(0, 2))?;
        assert!(line(&frame, &window).is_none());
        let symbols = (1..=10)
            .rev()
            .map(|id| self::frame(id, usize::from(id) * 2).map(|f| f.owner))
            .collect::<Result<Vec<_>, _>>()?;
        window.range =
            SourceRange::new(0, 22, SourcePosition::new(0, 0), SourcePosition::new(0, 22))?;
        let selected = candidates(&symbols, &window);
        assert_eq!(selected.len(), 8);
        assert_eq!(
            selected
                .iter()
                .map(|s| s.parsed().declaration_range().start_byte())
                .collect::<Vec<_>>(),
            vec![2, 4, 6, 8, 10, 12, 14, 16]
        );
        let mut prefix = vec![frame.owner.clone(); 4096];
        prefix.push(symbols[0].clone());
        window.range = symbols[0].parsed().declaration_range();
        assert!(
            candidates(&prefix, &window).is_empty(),
            "no scan beyond the bounded index prefix"
        );
        Ok(())
    }

    #[test]
    fn source_operations_pack_whole_utf8_lines_without_displacing_originals() {
        let lines = vec!["ä-operation\n".to_owned(), "second\n".to_owned()];
        let exact = HEADER.len() + lines[0].len() + TAIL.len();
        assert_eq!(pack(&lines[..1], exact - 1), "");
        assert_eq!(
            pack(&lines[..1], exact),
            format!("{HEADER}{}{TAIL}", lines[0])
        );
        assert_eq!(pack(&[], 1024), "");
        assert_eq!(pack(&["x".repeat(1024)], 8192), "");
        assert_eq!(pack(&lines, 1024), pack(&lines, 1024));
        assert!(pack(&lines, 1024).contains("unknown effects are not absent"));
    }
}
