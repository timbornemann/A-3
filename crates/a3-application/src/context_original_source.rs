//! Typed delivery metadata, not inferred source coverage or durable task verification.
use crate::AgentSourcePage;
use a3_domain::{AgentFileInspection, AgentFileStartLine, FileRevision, SnapshotId, SourceRange};

/// A complete, validated Reader page that the compiler actually included in this pack.
/// Construction is a trusted compiler responsibility, after freshness and budget checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOriginalSource {
    snapshot_id: SnapshotId,
    revision: FileRevision,
    range: SourceRange,
    start_line: AgentFileStartLine,
    next_start_line: Option<AgentFileStartLine>,
}

impl ContextOriginalSource {
    /// Projects an already validated and completely packed source page without its bytes.
    #[must_use]
    pub fn from_packed_page(snapshot_id: SnapshotId, page: &AgentSourcePage) -> Self {
        Self {
            snapshot_id,
            revision: page.revision().clone(),
            range: page.range(),
            start_line: page.start_line(),
            next_start_line: page.next_start_line(),
        }
    }

    /// Returns the immutable snapshot revalidated by this context compile.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the revalidated full-content identity, not a hash supplied by the model.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the exact source range delivered, which may be only part of the file.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }

    /// Whether this request adds no source lines to this exact current page.
    /// Partial overlaps are deliberately allowed; an empty EOF marker is not source delivery.
    #[must_use]
    pub fn covers(&self, snapshot: SnapshotId, request: &AgentFileInspection) -> bool {
        if self.snapshot_id != snapshot
            || self.revision.path() != request.path()
            || self.range.is_empty()
            || request.start_line() < self.start_line
        {
            return false;
        }
        let end = self.range.end_position();
        let last_line = end.row().saturating_add(u32::from(end.column() != 0));
        let start = request.start_line().get();
        if start > last_line {
            return false;
        }
        self.next_start_line.is_none()
            || start
                .checked_add(u32::from(request.line_count().get()) - 1)
                .is_some_and(|requested_last| requested_last <= last_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{AgentFileLineCount, ContentHash, RepositoryPath, SourcePosition};
    use std::error::Error;

    #[test]
    fn coverage_is_current_exact_complete_lines_and_eof_not_path_or_overlap()
    -> Result<(), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"source.rs".to_vec())?;
        let snapshot = SnapshotId::from_bytes([1; 32]);
        let request = |start, lines| -> Result<_, Box<dyn Error>> {
            Ok(AgentFileInspection::new(
                path.clone(),
                AgentFileStartLine::new(start)?,
                AgentFileLineCount::new(lines)?,
            ))
        };
        for (text, end, eof) in [
            ("a\nb\n", SourcePosition::new(11, 0), false),
            ("a\r\nb\r\n", SourcePosition::new(11, 0), false),
            ("a\nb", SourcePosition::new(10, 1), true),
            ("a\nb\n", SourcePosition::new(11, 0), true),
        ] {
            let page = AgentSourcePage::new(
                FileRevision::new(path.clone(), ContentHash::from_bytes([2; 32])),
                SourceRange::new(100, 100 + text.len(), SourcePosition::new(9, 0), end)?,
                AgentFileStartLine::new(10)?,
                text.to_owned(),
                if eof {
                    None
                } else {
                    Some(AgentFileStartLine::new(12)?)
                },
                !eof,
            )?;
            let source = ContextOriginalSource::from_packed_page(snapshot, &page);
            assert!(source.covers(snapshot, &request(10, 2)?));
            assert!(source.covers(snapshot, &request(11, 1)?));
            assert_eq!(source.covers(snapshot, &request(10, 64)?), eof);
            assert!(!source.covers(snapshot, &request(9, 2)?));
            assert!(!source.covers(snapshot, &request(12, 1)?));
            assert!(!source.covers(SnapshotId::from_bytes([3; 32]), &request(10, 1)?));
            let other = AgentFileInspection::new(
                RepositoryPath::try_from_bytes(b"other.rs".to_vec())?,
                AgentFileStartLine::new(10)?,
                AgentFileLineCount::new(1)?,
            );
            assert!(!source.covers(snapshot, &other));
        }
        let empty_request = request(1, 1)?;
        let empty = AgentSourcePage::new(
            FileRevision::new(path, ContentHash::from_bytes([2; 32])),
            SourceRange::new(0, 0, SourcePosition::new(0, 0), SourcePosition::new(0, 0))?,
            AgentFileStartLine::new(1)?,
            String::new(),
            None,
            false,
        )?;
        assert!(
            !ContextOriginalSource::from_packed_page(snapshot, &empty)
                .covers(snapshot, &empty_request)
        );
        Ok(())
    }
}
