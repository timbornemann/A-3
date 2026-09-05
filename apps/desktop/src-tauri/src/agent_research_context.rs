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
    origin: FocusOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusOrigin {
    Navigation,
    Explicit,
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
            // Only a repeated page-start cursor resumes its unseen suffix. A precise interior
            // line or issued source reference must remain usable to compare an earlier API.
            let page_start = matches!(action, AskResearchAction::InspectPath { .. })
                && self.excerpts.iter().any(|item| {
                    self.revision_for(item) == Some(&revision)
                        && item.start_line == start.row().saturating_add(1)
                });
            let continuation = self
                .delivered
                .iter()
                .find(|range| {
                    page_start
                        && range.revision == revision
                        && range.start <= start
                        && range.end > start
                })
                .map(|range| range.end);
            if let Some(next) = continuation
                && self.focus_at(revision.clone(), next)
            {
                return true;
            }
            let cached = self.focus_at(revision.clone(), start);
            if cached
                && !page_start
                && let Some(focus) = self
                    .focus
                    .iter_mut()
                    .find(|focus| focus.revision == revision)
            {
                focus.origin = FocusOrigin::Explicit;
            }
            cached
        })
    }

    pub(super) fn focus_at(&mut self, revision: FileRevision, start: SourcePosition) -> bool {
        let cached = self.excerpts.iter().any(|item| {
            self.revision_for(item) == Some(&revision) && offset(item, start).is_some()
        });
        if cached {
            if let Some(focus) = self
                .focus
                .iter_mut()
                .find(|focus| focus.revision == revision)
            {
                focus.start = start;
                focus.origin = FocusOrigin::Navigation;
            } else {
                self.focus.push(SourceFocus {
                    revision,
                    start,
                    origin: FocusOrigin::Navigation,
                });
            }
            if self.focus.len() > 8 {
                self.focus.remove(0);
            }
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
        self.focus_index_hint(published, hint, false)
    }

    pub(super) fn refine_action_focus(
        &mut self,
        published: &a3_domain::PublishedIndex,
        hint: &str,
    ) {
        self.focus_index_hint(published, hint, true);
    }

    fn focus_index_hint(
        &mut self,
        published: &a3_domain::PublishedIndex,
        hint: &str,
        selected_only: bool,
    ) -> bool {
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
        // Keep exact, source-backed units across decisions. A newly requested constructor
        // must not evict the caller/callback/writer established by earlier valid requests.
        // This is a bounded selection of existing index ranges, not model-authored evidence.
        for symbol in &symbols {
            if !tokens.contains(symbol.parsed().name().as_str())
                || !matches!(
                    symbol.parsed().kind(),
                    a3_domain::SymbolKind::Function | a3_domain::SymbolKind::Method
                )
                || (selected_only
                    && !self
                        .focus
                        .iter()
                        .any(|focus| focus.revision == *symbol.revision()))
            {
                continue;
            }
            let range = symbol.parsed().declaration_range();
            let unit = CoveredRange {
                revision: symbol.revision().clone(),
                start: SourcePosition::new(range.start_position().row(), 0),
                end: if range.end_position().column() == 0 {
                    range.end_position()
                } else {
                    SourcePosition::new(range.end_position().row().saturating_add(1), 0)
                },
            };
            if self.excerpts.iter().any(|item| {
                self.revision_for(item) == Some(&unit.revision)
                    && offset(item, unit.start).is_some()
            }) {
                self.retain_unit(unit);
                // An indented method alone does not prove its owning class. Retain the
                // actual enclosing declaration line as source, never an invented label.
                if let Some(owner) = symbols
                    .iter()
                    .filter(|owner| {
                        owner.revision() == symbol.revision()
                            && owner.parsed().kind() == a3_domain::SymbolKind::Class
                            && owner.parsed().declaration_range().contains(range)
                    })
                    .max_by_key(|owner| owner.parsed().declaration_range().start_position())
                {
                    let row = owner.parsed().declaration_range().start_position().row();
                    self.retain_unit(CoveredRange {
                        revision: symbol.revision().clone(),
                        start: SourcePosition::new(row, 0),
                        end: SourcePosition::new(row.saturating_add(1), 0),
                    });
                }
            }
        }
        let mut focused = false;
        let mut refined = Vec::new();
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
            if selected_only
                && (refined.contains(symbol.revision())
                    || !self
                        .focus
                        .iter()
                        .any(|focus| focus.revision == *symbol.revision() && focus.start <= start))
            {
                continue;
            }
            if !selected_only
                && self.delivered.iter().any(|old| {
                    old.revision == *symbol.revision()
                        && old.start <= start
                        && old.end >= range.end_position()
                })
            {
                continue;
            }
            for old in &self.delivered {
                if old.revision == *symbol.revision()
                    && old.start <= start
                    && old.end > start
                    && old.end < range.end_position()
                {
                    start = old.end;
                }
            }
            if self.focus_at(symbol.revision().clone(), start) {
                if !selected_only {
                    return true;
                }
                focused = true;
                refined.push(symbol.revision().clone());
            }
        }
        focused
    }

    fn retain_unit(&mut self, mut unit: CoveredRange) {
        // Include at most two real blank separator lines so adjacent method/scope units
        // share one header. Never bridge executable code or an unread cache gap.
        if let Some((item, offset)) = self.excerpts.iter().find_map(|item| {
            (self.revision_for(item) == Some(&unit.revision))
                .then(|| offset(item, unit.end).map(|start| (item, start)))
                .flatten()
        }) {
            for line in item.text[offset..].split_inclusive('\n').take(2) {
                if !line.trim().is_empty() {
                    break;
                }
                unit.end = end_position(unit.end, line);
            }
        }
        cover(&mut self.retained_units, unit);
        if self.retained_units.len() > 32 {
            self.retained_units.remove(0);
        }
    }

    /// Select a byte-position frontier in cached current source, not another file read.
    pub(super) fn advance_cached_frontier(&mut self) -> bool {
        let mut items = self.excerpts.iter().collect::<Vec<_>>();
        items.sort_by_key(|item| {
            !self
                .focus
                .iter()
                .any(|focus| self.revision_for(item) == Some(&focus.revision))
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
                let revision = revision.clone();
                self.focus.clear();
                self.focus.push(SourceFocus {
                    revision,
                    start,
                    origin: FocusOrigin::Explicit,
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
        let units = self.unit_excerpts();
        let is_unit =
            |item: &ResearchSourceExcerpt| units.iter().any(|unit| std::ptr::eq(unit, item));
        let mut candidates = units.iter().chain(self.excerpts.iter()).collect::<Vec<_>>();
        candidates.sort_by_key(|item| {
            let focused = self.focus.iter().position(|focus| {
                self.revision_for(item) == Some(&focus.revision)
                    && (is_unit(item) || offset(item, focus.start).is_some())
            });
            (
                focused.unwrap_or(usize::MAX),
                !is_unit(item),
                !(item.text.len() <= limit / 2
                    && self
                        .revision_for(item)
                        .is_some_and(|revision| required.contains(revision))),
            )
        });
        let mut selected: Vec<&ResearchSourceExcerpt> = Vec::new();
        for item in candidates {
            if selected.iter().any(|old| {
                self.revision_for(item).is_some()
                    && self.revision_for(item) == self.revision_for(old)
                    && !(is_unit(item) && is_unit(old))
            }) {
                continue;
            }
            let item = if is_unit(item)
                || self
                    .focus
                    .iter()
                    .any(|focus| self.revision_for(item) == Some(&focus.revision))
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
            if selected.len()
                >= if units.is_empty() {
                    8.min((limit / 256).max(1))
                } else {
                    8
                }
            {
                break;
            }
        }
        let parts = selected
            .iter()
            .map(|item| {
                let focus = self.focus.iter().find(|focus| {
                    (!is_unit(item) || item.text.len() > limit / 2)
                        && self.revision_for(item) == Some(&focus.revision)
                        && offset(item, focus.start).is_some()
                });
                let start = focus.map_or(
                    SourcePosition::new(item.start_line.saturating_sub(1), 0),
                    |focus| focus.start,
                );
                let body = &item.text[offset(item, start).unwrap_or(0)..];
                let header = format!(
                    "\n[S{}] {} ab Zeile {} (Spalte {})\n",
                    item.ordinal,
                    item.path,
                    start.row().saturating_add(1),
                    start.column()
                );
                (item, start, body, header)
            })
            .collect::<Vec<_>>();
        // Water-fill complete packet costs. Large files cannot starve the other requested
        // interfaces; short complete sources return their unused share to remaining views.
        let mut quotas = vec![0; parts.len()];
        let weights = parts
            .iter()
            .map(|(item, _, body, _)| {
                if !units.is_empty() {
                    // Under real overflow, deliver a usable active region. Equal shares
                    // can all be smaller than their header + truncation marker, yielding
                    // an empty evidence packet while readable source remains cached.
                    return if self.focus.iter().any(|focus| {
                        self.revision_for(item) == Some(&focus.revision)
                            && offset(item, focus.start).is_some()
                    }) {
                        16
                    } else {
                        1
                    };
                }
                if (!is_unit(item) && body.len() <= limit / 2)
                    || self
                        .focus
                        .iter()
                        .any(|focus| self.revision_for(item) == Some(&focus.revision))
                {
                    4
                } else {
                    1
                }
            })
            .collect::<Vec<usize>>();
        let mut pending = (0..parts.len()).collect::<Vec<_>>();
        let mut remaining = limit;
        // When the requested units fit together, reserve them whole before background hits.
        // Equal shares of entire file suffixes repeatedly hid other needed methods.
        let protected = |item: &ResearchSourceExcerpt| {
            is_unit(item)
                || self
                    .focus
                    .iter()
                    .any(|focus| self.revision_for(item) == Some(&focus.revision))
        };
        let unit_cost = parts
            .iter()
            .filter(|(item, _, _, _)| protected(item))
            .map(|(_, _, body, header)| body.len() + header.len() + 1)
            .sum::<usize>();
        if unit_cost <= limit {
            for (index, (item, _, body, header)) in parts.iter().enumerate() {
                if protected(item) {
                    quotas[index] = body.len() + header.len() + 1;
                    remaining = remaining.saturating_sub(quotas[index]);
                    pending.retain(|candidate| *candidate != index);
                }
            }
        }
        while !pending.is_empty() {
            let total_weight = pending.iter().map(|index| weights[*index]).sum::<usize>();
            let share = remaining / total_weight;
            let complete = pending
                .iter()
                .copied()
                .filter(|index| {
                    let (_, _, body, header) = &parts[*index];
                    header.len() + body.len() < share * weights[*index]
                })
                .collect::<Vec<_>>();
            if complete.is_empty() {
                for index in pending {
                    quotas[index] = share * weights[index];
                }
                break;
            }
            for index in complete {
                let (_, _, body, header) = &parts[index];
                quotas[index] = header.len() + body.len() + 1;
                remaining = remaining.saturating_sub(quotas[index]);
                pending.retain(|candidate| *candidate != index);
            }
        }
        let mut output = String::new();
        let mut delivered = Vec::new();
        for ((item, start, body, header), quota) in parts.into_iter().zip(quotas) {
            let available = quota.saturating_sub(header.len());
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

    /// Materialize only cached, revision-matching intervals; never fill gaps from index text.
    /// Overlap is merged in the selection, so each byte appears at most once per revision.
    fn unit_excerpts(&self) -> Vec<ResearchSourceExcerpt> {
        let mut result = Vec::new();
        let mut ranges = self.retained_units.clone();
        // Retention is not a lock: exact new lines and the sole recovery frontier must
        // still be visible, including an unselected region of the SAME file.
        for focus in &self.focus {
            if focus.origin != FocusOrigin::Explicit
                || !ranges.iter().any(|unit| unit.revision == focus.revision)
                || ranges.iter().any(|unit| {
                    unit.revision == focus.revision
                        && unit.start <= focus.start
                        && unit.end > focus.start
                })
            {
                continue;
            }
            let start = SourcePosition::new(focus.start.row(), 0);
            let end = ranges
                .iter()
                .filter(|unit| unit.revision == focus.revision && unit.start > start)
                .map(|unit| unit.start)
                .min()
                .unwrap_or(SourcePosition::new(u32::MAX, 0));
            ranges.insert(
                0,
                CoveredRange {
                    revision: focus.revision.clone(),
                    start,
                    end,
                },
            );
        }
        for unit in &ranges {
            let mut cursor = unit.start;
            while cursor < unit.end && result.len() < 32 {
                let candidate = self
                    .excerpts
                    .iter()
                    .filter_map(|item| {
                        if self.revision_for(item) != Some(&unit.revision) {
                            return None;
                        }
                        let start = offset(item, cursor)?;
                        let end = end_position(
                            SourcePosition::new(item.start_line.saturating_sub(1), 0),
                            &item.text,
                        )
                        .min(unit.end);
                        Some((item, start, end))
                    })
                    .max_by_key(|(_, _, end)| *end);
                let Some((item, start, end)) = candidate else {
                    break;
                };
                // Unit starts and page ends are line-aligned. A partial overlong line uses
                // the existing byte-cursor fallback instead of inventing a new line anchor.
                if cursor.column() != 0 || end <= cursor {
                    break;
                }
                let finish = offset(item, end).unwrap_or(item.text.len());
                result.push(ResearchSourceExcerpt {
                    ordinal: item.ordinal,
                    path: item.path.clone(),
                    start_line: cursor.row().saturating_add(1),
                    text: item.text[start..finish].to_owned(),
                });
                cursor = end;
            }
        }
        result
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
