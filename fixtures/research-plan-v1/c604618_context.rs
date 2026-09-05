//! Frozen source-window algorithm from c604618, used ONLY for the plan regression comparison.
//! Adaptation: the previous single mutable focus is represented by the last selected focus.
use super::*;
use research_context::{CoveredRange, end_position};

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
    pub(super) fn legacy_plan_evidence_window(
        &mut self,
        required: &[FileRevision],
        limit: usize,
    ) -> String {
        let mut candidates = self.excerpts.iter().collect::<Vec<_>>();
        candidates.sort_by_key(|item| {
            let focused = self.focus.last().is_some_and(|focus| {
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
                .last()
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
            let focus = self.focus.last().filter(|focus| {
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
}
