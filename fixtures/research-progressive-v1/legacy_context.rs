//! Test-only frozen packer from ee85cbe, for a reproducible ADR-0046 before/after comparison.

use super::{AskResearchWorkingSet, ResearchSourceExcerpt, utf8_prefix};
use a3_domain::FileRevision;

impl AskResearchWorkingSet {
    pub(super) fn legacy_evidence_window(&self, required: &[FileRevision], limit: usize) -> String {
        let revision = |item: &ResearchSourceExcerpt| {
            self.sources
                .iter()
                .find(|source| source.ordinal() == item.ordinal)
                .map(|source| source.revision())
        };
        let mut candidates = self.excerpts.iter().collect::<Vec<_>>();
        // Compact named files remain intact even when later symbol/tail reads refocus the cache.
        // Large files keep the explicit page focus; substituting their beginning would hide it.
        candidates.sort_by_key(|item| {
            !(item.text.len() <= limit / 2
                && revision(item).is_some_and(|value| required.contains(value)))
        });
        let mut selected: Vec<&ResearchSourceExcerpt> = Vec::new();
        for item in candidates {
            let item = self
                .excerpts
                .iter()
                .filter(|outer| {
                    outer.text.len() <= limit / 2
                        && revision(item).is_some()
                        && revision(outer) == revision(item)
                        && contains_excerpt(outer, item)
                })
                .max_by_key(|outer| outer.text.len())
                .unwrap_or(item);
            if selected.iter().any(|previous| {
                previous.ordinal == item.ordinal
                    || (revision(item).is_some()
                        && revision(previous) == revision(item)
                        && contains_excerpt(previous, item))
            }) {
                continue;
            }
            selected.push(item);
            if selected.len() >= 8.min((limit / 256).max(1)) {
                break;
            }
        }
        let mut output = String::new();
        for (index, item) in selected.iter().enumerate() {
            let header = excerpt_header(item);
            let remaining = limit.saturating_sub(output.len());
            let reserve = selected[index + 1..]
                .iter()
                .map(|next| (excerpt_header(next).len() + next.text.len() + 1).min(192))
                .sum::<usize>();
            let full_size = header.len() + item.text.len() + 1;
            if full_size <= remaining.saturating_sub(reserve) {
                output.push_str(&header);
                output.push_str(&item.text);
                output.push('\n');
                continue;
            }
            let allowance = remaining / (selected.len() - index);
            const NOTICE_RESERVE: usize = 128;
            if allowance <= header.len() + NOTICE_RESERVE {
                continue;
            }
            let prefix = utf8_prefix(&item.text, allowance - header.len() - NOTICE_RESERVE);
            // Prefer whole lines, retaining indentation. An overlong first line remains explicitly
            // clipped with a cursor for that same line, never falsely marked as fully inspected.
            let body = prefix.rfind('\n').map_or(prefix, |end| &prefix[..=end]);
            let next_line = item.start_line.saturating_add(
                u32::try_from(body.bytes().filter(|byte| *byte == b'\n').count())
                    .unwrap_or(u32::MAX),
            );
            output.push_str(&header);
            output.push_str(body);
            output.push_str(&format!(
                "\n[Kontextauszug gekürzt; weiter mit inspectPath start_line {next_line} statt erneutem Dateianfang.]\n"));
        }
        output
    }
}

fn excerpt_header(item: &ResearchSourceExcerpt) -> String {
    format!(
        "\n[S{}] {} ab Zeile {} (aktuell gelesen)\n",
        item.ordinal, item.path, item.start_line
    )
}

fn contains_excerpt(outer: &ResearchSourceExcerpt, inner: &ResearchSourceExcerpt) -> bool {
    let Some(offset) = inner.start_line.checked_sub(outer.start_line) else {
        return false;
    };
    let mut tail = outer.text.as_str();
    for _ in 0..offset {
        let Some((_, next)) = tail.split_once('\n') else {
            return false;
        };
        tail = next;
    }
    tail.starts_with(&inner.text)
}
