//! Revision-bound cache focus and actual model-window coverage. No tool execution or authority.

use super::{AskResearchWorkingSet, ResearchSourceExcerpt, utf8_prefix};
use a3_application::AskResearchAction;
use a3_domain::{FileRevision, SourcePosition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CoveredRange {
    pub revision: FileRevision,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Clone, Debug)]
pub(super) struct SourceFocus {
    pub revision: FileRevision,
    pub start: SourcePosition,
}

/// Merge overlapping/touching intervals, retaining revision and byte-column precision.
pub(super) fn cover(ranges: &mut Vec<CoveredRange>, mut new: CoveredRange) -> bool {
    if new.start >= new.end
        || ranges
            .iter()
            .any(|old| old.revision == new.revision && old.start <= new.start && old.end >= new.end)
    {
        return false;
    }
    while let Some(index) = ranges.iter().position(|old| {
        old.revision == new.revision && old.start <= new.end && old.end >= new.start
    }) {
        let old = ranges.remove(index);
        new.start = new.start.min(old.start);
        new.end = new.end.max(old.end);
    }
    ranges.push(new);
    ranges.sort_by_key(|range| range.start);
    true
}

pub(super) fn end_position(start: SourcePosition, text: &str) -> SourcePosition {
    let rows =
        u32::try_from(text.bytes().filter(|byte| *byte == b'\n').count()).unwrap_or(u32::MAX);
    let column = text.rsplit_once('\n').map_or_else(
        || {
            start
                .column()
                .saturating_add(u32::try_from(text.len()).unwrap_or(u32::MAX))
        },
        |(_, tail)| u32::try_from(tail.len()).unwrap_or(u32::MAX),
    );
    SourcePosition::new(start.row().saturating_add(rows), column)
}

fn offset(item: &ResearchSourceExcerpt, point: SourcePosition) -> Option<usize> {
    let rows = point.row().checked_sub(item.start_line.checked_sub(1)?)?;
    let mut result = 0;
    for _ in 0..rows {
        result += item.text.get(result..)?.find('\n')? + 1;
    }
    let column = usize::try_from(point.column()).ok()?;
    if column
        > item
            .text
            .get(result..)?
            .find('\n')
            .unwrap_or(item.text.len() - result)
    {
        return None;
    }
    result = result.checked_add(column)?;
    (result < item.text.len() && item.text.is_char_boundary(result)).then_some(result)
}

impl AskResearchWorkingSet {
    pub(super) fn evidence_guard<'a>(
        &self,
        project: &'a a3_domain::ProjectIdentity,
    ) -> super::research_model::EvidenceGuard<'a> {
        let mut revisions = Vec::new();
        for range in &self.current_delivery {
            if !revisions
                .iter()
                .any(|(revision, _)| revision == &range.revision)
            {
                revisions.push((range.revision.clone(), range.start.row().saturating_add(1)));
            }
        }
        super::research_model::EvidenceGuard { project, revisions }
    }
    pub(super) fn record_read_coverage(
        &mut self,
        source: &a3_application::AskResearchSource,
        start_line: u32,
        text: &str,
    ) {
        let start = SourcePosition::new(start_line.saturating_sub(1), 0);
        let end = end_position(start, text);
        if cover(
            &mut self.read_coverage,
            CoveredRange {
                revision: source.revision().clone(),
                start,
                end,
            },
        ) {
            self.evidence_revision = self.evidence_revision.saturating_add(1);
        }
    }

    pub(super) fn focus_cached(&mut self, action: &AskResearchAction) -> bool {
        let target = match action {
            AskResearchAction::InspectPath { path, start_line } => self
                .sources
                .iter()
                .find(|source| super::model_safe_path(source.revision().path()) == *path)
                .map(|source| {
                    (
                        source.revision().clone(),
                        SourcePosition::new(start_line.saturating_sub(1), 0),
                    )
                }),
            AskResearchAction::InspectSource(ordinal) => self
                .sources
                .get(usize::from(ordinal.saturating_sub(1)))
                .map(|source| {
                    (
                        source.revision().clone(),
                        source
                            .range()
                            .map_or(SourcePosition::new(0, 0), |range| range.start_position()),
                    )
                }),
            _ => None,
        };
        target.is_some_and(|(revision, start)| {
            // The tool has a line cursor; the Core owns precise continuation inside a long line.
            let continuation = self
                .delivered
                .iter()
                .find(|range| {
                    range.revision == revision
                        && range.end.row() == start.row()
                        && range.end.column() > start.column()
                })
                .map(|range| range.end);
            if let Some(next) = continuation
                && self.focus_at(revision.clone(), next)
            {
                return true;
            }
            self.focus_at(revision, start)
        })
    }

    pub(super) fn focus_at(&mut self, revision: FileRevision, start: SourcePosition) -> bool {
        let cached = self.excerpts.iter().any(|item| {
            self.revision_for(item) == Some(&revision) && offset(item, start).is_some()
        });
        if cached {
            self.focus = Some(SourceFocus { revision, start });
        }
        cached
    }

    pub(super) fn revision_for(&self, item: &ResearchSourceExcerpt) -> Option<&FileRevision> {
        self.sources
            .iter()
            .find(|source| source.ordinal() == item.ordinal)
            .map(|source| source.revision())
    }

    /// Use only names and ranges from the pinned index. Unvalidated output never reaches here.
    pub(super) fn focus_hint(&mut self, published: &a3_domain::PublishedIndex, hint: &str) -> bool {
        let tokens = hint
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|token| token.len() >= 3 && token.len() <= 128)
            .take(256)
            .collect::<std::collections::BTreeSet<_>>();
        let mut symbols = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .take(32_768)
            .collect::<Vec<_>>();
        symbols.sort_by_key(|symbol| symbol.parsed().kind() == a3_domain::SymbolKind::Class);
        for symbol in symbols {
            if !tokens.contains(symbol.parsed().name().as_str())
                || !matches!(
                    symbol.parsed().kind(),
                    a3_domain::SymbolKind::Function
                        | a3_domain::SymbolKind::Method
                        | a3_domain::SymbolKind::Class
                )
            {
                continue;
            }
            let range = symbol.parsed().declaration_range();
            let mut start = SourcePosition::new(range.start_position().row(), 0);
            if self.delivered.iter().any(|old| {
                old.revision == *symbol.revision()
                    && old.start <= start
                    && old.end >= range.end_position()
            }) {
                continue;
            }
            for old in &self.delivered {
                if old.revision == *symbol.revision() && old.start <= start && old.end > start {
                    start = old.end;
                }
            }
            if self.focus_at(symbol.revision().clone(), start) {
                return true;
            }
        }
        false
    }

    /// Select a byte-position frontier in cached current source, not another file read.
    pub(super) fn advance_cached_frontier(&mut self) -> bool {
        let mut items = self.excerpts.iter().collect::<Vec<_>>();
        items.sort_by_key(|item| {
            self.focus
                .as_ref()
                .is_none_or(|focus| self.revision_for(item) != Some(&focus.revision))
        });
        for item in items {
            let Some(revision) = self.revision_for(item) else {
                continue;
            };
            let mut start = SourcePosition::new(item.start_line.saturating_sub(1), 0);
            for covered in &self.delivered {
                if covered.revision == *revision && covered.start <= start && covered.end > start {
                    start = covered.end;
                }
            }
            if offset(item, start).is_some() {
                self.focus = Some(SourceFocus {
                    revision: revision.clone(),
                    start,
                });
                return true;
            }
        }
        false
    }

    pub(super) fn compile_evidence_window(
        &mut self,
        required: &[FileRevision],
        limit: usize,
    ) -> String {
        let mut candidates = self.excerpts.iter().collect::<Vec<_>>();
        candidates.sort_by_key(|item| {
            let focused = self.focus.as_ref().is_some_and(|focus| {
                self.revision_for(item) == Some(&focus.revision)
                    && offset(item, focus.start).is_some()
            });
            (
                !focused,
                !(item.text.len() <= limit / 2
                    && self
                        .revision_for(item)
                        .is_some_and(|revision| required.contains(revision))),
            )
        });
        let mut selected: Vec<&ResearchSourceExcerpt> = Vec::new();
        for item in candidates {
            if selected.iter().any(|old| {
                old.ordinal == item.ordinal
                    || (self.revision_for(item).is_some()
                        && self.revision_for(item) == self.revision_for(old))
            }) {
                continue;
            }
            let item = if self
                .focus
                .as_ref()
                .is_some_and(|focus| self.revision_for(item) == Some(&focus.revision))
            {
                item
            } else {
                self.excerpts
                    .iter()
                    .filter(|outer| {
                        outer.text.len() <= limit / 2
                            && self.revision_for(item).is_some()
                            && self.revision_for(outer) == self.revision_for(item)
                            && offset(
                                outer,
                                SourcePosition::new(item.start_line.saturating_sub(1), 0),
                            )
                            .is_some_and(|start| outer.text[start..].starts_with(&item.text))
                    })
                    .max_by_key(|outer| outer.text.len())
                    .unwrap_or(item)
            };
            selected.push(item);
            if selected.len() >= 8.min((limit / 256).max(1)) {
                break;
            }
        }
        let mut output = String::new();
        let mut delivered = Vec::new();
        for (index, item) in selected.iter().enumerate() {
            let focus = self.focus.as_ref().filter(|focus| {
                self.revision_for(item) == Some(&focus.revision)
                    && offset(item, focus.start).is_some()
            });
            let start = focus.map_or(
                SourcePosition::new(item.start_line.saturating_sub(1), 0),
                |focus| focus.start,
            );
            let body = &item.text[offset(item, start).unwrap_or(0)..];
            let header = format!(
                "\n[S{}] {} ab Zeile {} (Spalte {}; aktueller Ausschnitt)\n",
                item.ordinal,
                item.path,
                start.row().saturating_add(1),
                start.column()
            );
            let remaining = limit.saturating_sub(output.len());
            let reserve = (selected.len() - index - 1) * 64;
            let available = remaining
                .saturating_sub(reserve)
                .saturating_sub(header.len());
            let clipped = body.len().saturating_add(1) > available;
            let allowance = if clipped {
                available.saturating_sub(160)
            } else {
                body.len()
            };
            if allowance == 0 {
                continue;
            }
            let prefix = utf8_prefix(body, allowance);
            let retained = if clipped {
                prefix.rfind('\n').map_or(prefix, |end| &prefix[..=end])
            } else {
                prefix
            };
            if retained.is_empty() {
                continue;
            }
            let end = end_position(start, retained);
            output.push_str(&header);
            output.push_str(retained);
            output.push('\n');
            if clipped {
                output.push_str(&format!("[Kontext gekürzt; Rest im Cache, Zeile {}, Spalte {}. inspectPath fokussiert ohne neuen Read.]\n", end.row().saturating_add(1), end.column()));
            }
            if let Some(revision) = self.revision_for(item) {
                delivered.push(CoveredRange {
                    revision: revision.clone(),
                    start,
                    end,
                });
            }
        }
        self.current_delivery = delivered;
        output
    }

    /// Commit only the packet handed to the model boundary, not speculative packing for progress.
    pub(super) fn commit_delivery(&mut self) {
        for range in self.current_delivery.clone() {
            if cover(&mut self.delivered, range) {
                self.delivery_revision = self.delivery_revision.saturating_add(1);
            }
        }
    }

    pub(super) fn progress_with_pending(&self) -> usize {
        let mut delivered = self.delivered.clone();
        let added = self
            .current_delivery
            .iter()
            .filter(|range| cover(&mut delivered, (*range).clone()))
            .count();
        self.evidence_revision
            .saturating_add(self.delivery_revision)
            .saturating_add(added)
    }
}
