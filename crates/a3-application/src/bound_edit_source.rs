//! Turns a selected current Task Lens span into one exact safe file inspection.

use crate::{AgentReadAction, AgentReadResult, AgentSourcePage, ContextToolResultStatus};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, AgentInspectAction,
    AgentInspectTarget, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint, GraphSymbol,
    IndexRunId, ProjectIdentity, PublishedIndex, SnapshotId, SourcePosition, SourceRange, SymbolId,
    SyntaxRelationKind, TaskLens, TaskLensTarget, ToolRunId, WorktreeId,
};
use std::{error::Error, fmt};

/// One mandatory span selected by the Core in the exact current Task Lens publication.
///
/// The owning caller invokes the existing AgentReadTools and journals its result.
/// Taking the original page does not claim that a model received it.
#[derive(Debug)]
pub struct BoundEditSource {
    worktree: WorktreeId,
    index_run: IndexRunId,
    snapshot: SnapshotId,
    required: EvidenceRef,
    request: AgentFileInspection,
    direct_callee: Option<SymbolId>,
}

impl BoundEditSource {
    /// Exact current Task Lens publication that bound this descriptor; not read authority.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run
    }

    /// Binds an explicitly chosen current range in a file selected by the lens.
    pub fn for_file_span(
        project: &ProjectIdentity,
        published: &PublishedIndex,
        lens: &TaskLens,
        required: &EvidenceRef,
    ) -> Result<Self, BoundEditSourceFailure> {
        require_current_lens(published, lens)?;
        if !has_current_revision(published, required.revision()) {
            return Err(BoundEditSourceFailure::StalePublication);
        }
        if !lens.entries().iter().any(|entry| match entry.target() {
            TaskLensTarget::File(revision) => revision == required.revision(),
            TaskLensTarget::SourceSpan { evidence, .. } => {
                evidence.revision() == required.revision()
                    && evidence.range().contains(required.range())
            }
            _ => false,
        }) {
            return Err(BoundEditSourceFailure::MissingSelection);
        }
        Self::bind(project, published, required)
    }

    /// Binds the full declaration range of an explicitly chosen structural symbol.
    pub fn for_symbol(
        project: &ProjectIdentity,
        published: &PublishedIndex,
        lens: &TaskLens,
        symbol: SymbolId,
    ) -> Result<Self, BoundEditSourceFailure> {
        require_current_lens(published, lens)?;
        let evidence = lens
            .entries()
            .iter()
            .find_map(|entry| match entry.target() {
                TaskLensTarget::SourceSpan {
                    symbol_id,
                    evidence,
                } if *symbol_id == symbol => Some(evidence),
                _ => None,
            })
            .ok_or(BoundEditSourceFailure::MissingSelection)?;
        let current =
            published_symbol(published, symbol).ok_or(BoundEditSourceFailure::StalePublication)?;
        if evidence.revision() != current.revision()
            || evidence.range() != current.parsed().declaration_range()
        {
            return Err(BoundEditSourceFailure::StalePublication);
        }
        Self::bind(project, published, evidence)
    }

    /// Binds a selected caller declaration with a current, directed call edge to the callee.
    pub fn for_direct_caller(
        project: &ProjectIdentity,
        published: &PublishedIndex,
        lens: &TaskLens,
        callee: SymbolId,
        caller: SymbolId,
    ) -> Result<Self, BoundEditSourceFailure> {
        let mut bound = Self::for_symbol(project, published, lens, caller)?;
        if !has_call_in_span(
            published.publication().graph().edges(),
            caller,
            callee,
            &bound.required,
        ) {
            return Err(BoundEditSourceFailure::MissingDirectCaller);
        }
        bound.direct_callee = Some(callee);
        Ok(bound)
    }

    pub(crate) fn is_full_declaration(&self, published: &PublishedIndex) -> bool {
        published
            .publication()
            .graph()
            .symbols()
            .iter()
            .any(|symbol| {
                symbol.revision() == self.required.revision()
                    && symbol.parsed().declaration_range() == self.required.range()
            })
    }

    pub(crate) fn direct_callee<'a>(
        &self,
        published: &'a PublishedIndex,
    ) -> Option<&'a GraphSymbol> {
        published_symbol(published, self.direct_callee?)
    }

    fn bind(
        project: &ProjectIdentity,
        published: &PublishedIndex,
        required: &EvidenceRef,
    ) -> Result<Self, BoundEditSourceFailure> {
        let run = published.run();
        Ok(Self {
            worktree: project.worktree().id(),
            index_run: run.id(),
            snapshot: run.snapshot_id(),
            request: inspection_for_span(required)?,
            required: required.clone(),
            direct_callee: None,
        })
    }

    /// Returns the exact required span, before expanding it to complete lines.
    #[must_use]
    pub const fn required(&self) -> &EvidenceRef {
        &self.required
    }

    /// Returns the only snapshot eligible for this read.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot
    }

    /// Returns the Core-bound worktree of this selection.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree
    }

    /// Prepares the existing read-only inspection against the current publication.
    pub fn read_action(
        &self,
        project: &ProjectIdentity,
        published: &PublishedIndex,
    ) -> Result<AgentReadAction, BoundEditSourceFailure> {
        self.require_current(project, published)?;
        Ok(AgentReadAction::Inspect(AgentInspectAction::new(
            AgentInspectTarget::File(self.request.clone()),
        )))
    }

    /// Validates the real tool result before releasing its volatile original page.
    ///
    /// The caller retains the tool result's content-free journal metadata.
    pub fn take_original(
        &self,
        project: &ProjectIdentity,
        published: &PublishedIndex,
        expected_tool: ToolRunId,
        read: &mut AgentReadResult,
    ) -> Result<AgentSourcePage, BoundEditSourceFailure> {
        self.require_current(project, published)?;
        if read.snapshot_id() != self.snapshot || read.evidence().snapshot_id() != self.snapshot {
            return Err(BoundEditSourceFailure::StalePublication);
        }
        if read.tool_run_id() != expected_tool
            || read.status() != ContextToolResultStatus::Succeeded
        {
            return Err(BoundEditSourceFailure::InvalidReadResult);
        }
        let page = read
            .take_original_page()
            .ok_or(BoundEditSourceFailure::MissingOriginal)?;
        if !read.evidence().evidence().contains(&page.evidence()) {
            return Err(BoundEditSourceFailure::InvalidReadResult);
        }
        self.require_bound_page(&page)?;
        Ok(page)
    }

    pub(crate) fn require_bound_page(
        &self,
        page: &AgentSourcePage,
    ) -> Result<(), BoundEditSourceFailure> {
        if page.revision() != self.required.revision() {
            return Err(BoundEditSourceFailure::InvalidReadResult);
        }
        validate_source_page(page, &self.request, Some(self.required.range()))
    }

    pub(crate) fn require_current(
        &self,
        project: &ProjectIdentity,
        published: &PublishedIndex,
    ) -> Result<(), BoundEditSourceFailure> {
        let run = published.run();
        if project.worktree().id() != self.worktree
            || run.id() != self.index_run
            || run.snapshot_id() != self.snapshot
            || !has_current_revision(published, self.required.revision())
        {
            return Err(BoundEditSourceFailure::StalePublication);
        }
        Ok(())
    }
}

pub(crate) fn validate_source_page(
    page: &AgentSourcePage,
    request: &AgentFileInspection,
    required: Option<SourceRange>,
) -> Result<(), BoundEditSourceFailure> {
    let actual = page.range();
    let text = page.text();
    if page.start_line() != request.start_line()
        || u32::try_from(text.len()).ok() != Some(actual.len())
    {
        return Err(BoundEditSourceFailure::InvalidReadResult);
    }
    if actual.is_empty() {
        return Err(BoundEditSourceFailure::MissingSpan);
    }
    let first_row = request.start_line().get() - 1;
    let end = position_at(text, first_row)?;
    let lines = covered_lines(first_row, end).ok_or(BoundEditSourceFailure::InvalidReadResult)?;
    let next_line = request.start_line().get().checked_add(lines);
    if actual.start_position() != SourcePosition::new(first_row, 0)
        || actual.end_position() != end
        || lines > u32::from(request.line_count().get())
        || page
            .next_start_line()
            .is_some_and(|next| end.column() != 0 || Some(next.get()) != next_line)
    {
        return Err(BoundEditSourceFailure::InvalidReadResult);
    }

    let Some(required) = required else {
        return Ok(());
    };
    if !actual.contains(required) {
        return Err(
            if page.truncated()
                && actual.start_byte() <= required.start_byte()
                && actual.end_byte() < required.end_byte()
            {
                BoundEditSourceFailure::TooLarge
            } else {
                BoundEditSourceFailure::MissingSpan
            },
        );
    }
    // Exact byte bounds must identify the same UTF-8 source points as the index.
    for (offset, point) in [
        (required.start_byte(), required.start_position()),
        (required.end_byte(), required.end_position()),
    ] {
        let relative = usize::try_from(offset - actual.start_byte())
            .map_err(|_| BoundEditSourceFailure::InvalidReadResult)?;
        let prefix = text
            .get(..relative)
            .ok_or(BoundEditSourceFailure::InvalidReadResult)?;
        if position_at(prefix, first_row)? != point {
            return Err(BoundEditSourceFailure::InvalidReadResult);
        }
    }
    Ok(())
}

fn require_current_lens(
    published: &PublishedIndex,
    lens: &TaskLens,
) -> Result<(), BoundEditSourceFailure> {
    let run = published.run();
    if lens.index_run_id() != run.id() || lens.snapshot_id() != run.snapshot_id() {
        return Err(BoundEditSourceFailure::StalePublication);
    }
    Ok(())
}

fn published_symbol(published: &PublishedIndex, id: SymbolId) -> Option<&GraphSymbol> {
    let symbols = published.publication().graph().symbols();
    let index = symbols.binary_search_by_key(&id, GraphSymbol::id).ok()?;
    symbols.get(index)
}

fn has_current_revision(published: &PublishedIndex, expected: &FileRevision) -> bool {
    let files = published.publication().graph().files();
    files
        .binary_search_by(|revision| revision.path().cmp(expected.path()))
        .is_ok_and(|index| &files[index] == expected)
}

fn has_call_in_span(
    edges: &[GraphEdge],
    caller: SymbolId,
    callee: SymbolId,
    span: &EvidenceRef,
) -> bool {
    let source = GraphEndpoint::Symbol(caller);
    let target = GraphEndpoint::Symbol(callee);
    let key = (&source, &target, SyntaxRelationKind::Calls);
    // LinkedGraph canonically orders the source, target and relation before edge evidence.
    let first = edges.partition_point(|edge| (edge.source(), edge.target(), edge.kind()) < key);
    edges[first..]
        .iter()
        .take_while(|edge| (edge.source(), edge.target(), edge.kind()) == key)
        .any(|edge| {
            edge.evidence().revision() == span.revision()
                && !edge.evidence().range().is_empty()
                && span.range().contains(edge.evidence().range())
        })
}

pub(crate) fn inspection_for_span(
    required: &EvidenceRef,
) -> Result<AgentFileInspection, BoundEditSourceFailure> {
    let range = required.range();
    if range.is_empty() || range.start_position() == range.end_position() {
        return Err(BoundEditSourceFailure::MissingSpan);
    }
    let first_row = range.start_position().row();
    let start = first_row
        .checked_add(1)
        .ok_or(BoundEditSourceFailure::TooLarge)?;
    let lines =
        covered_lines(first_row, range.end_position()).ok_or(BoundEditSourceFailure::TooLarge)?;
    Ok(AgentFileInspection::new(
        required.revision().path().clone(),
        AgentFileStartLine::new(start).map_err(|_| BoundEditSourceFailure::TooLarge)?,
        AgentFileLineCount::new(
            u16::try_from(lines).map_err(|_| BoundEditSourceFailure::TooLarge)?,
        )
        .map_err(|_| BoundEditSourceFailure::TooLarge)?,
    ))
}

fn covered_lines(first_row: u32, end: SourcePosition) -> Option<u32> {
    end.row()
        .checked_sub(first_row)?
        .checked_add(u32::from(end.column() != 0))
}

fn position_at(prefix: &str, first_row: u32) -> Result<SourcePosition, BoundEditSourceFailure> {
    let mut lines = prefix.split('\n');
    let last_line = lines
        .next_back()
        .ok_or(BoundEditSourceFailure::InvalidReadResult)?;
    let row = u32::try_from(lines.count())
        .ok()
        .and_then(|count| first_row.checked_add(count))
        .ok_or(BoundEditSourceFailure::InvalidReadResult)?;
    let column =
        u32::try_from(last_line.len()).map_err(|_| BoundEditSourceFailure::InvalidReadResult)?;
    Ok(SourcePosition::new(row, column))
}

/// Closed reason the selected source cannot enter an edit phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundEditSourceFailure {
    /// Worktree, published index run, snapshot or full revision changed.
    StalePublication,
    /// The current Task Lens did not select the explicit target.
    MissingSelection,
    /// No current directed call site exists in the selected caller declaration.
    MissingDirectCaller,
    /// The mandatory byte range is empty or absent from the actual original.
    MissingSpan,
    /// The whole mandatory span cannot be returned in one bounded complete-line page.
    TooLarge,
    /// The successful tool result contained only metadata or a preview.
    MissingOriginal,
    /// The read did not match the requested tool, revision or source geometry.
    InvalidReadResult,
}

impl fmt::Display for BoundEditSourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StalePublication => "selected source publication changed",
            Self::MissingSelection => {
                "explicit source target is missing from the current Task Lens"
            }
            Self::MissingDirectCaller => "selected declaration has no current direct call",
            Self::MissingSpan => "mandatory original source span is missing",
            Self::TooLarge => "mandatory original source span exceeds one bounded page",
            Self::MissingOriginal => "read result did not contain an original source page",
            Self::InvalidReadResult => "source read did not match its exact request",
        })
    }
}

impl Error for BoundEditSourceFailure {}

/// Matches only the existing complete-line envelope, not source freshness or sufficiency.
pub(crate) fn definition_page_matches(definition: &EvidenceRef, page: &EvidenceRef) -> bool {
    let declaration = definition.range();
    let actual = page.range();
    let start = declaration.start_position();
    let end = declaration.end_position();
    let Some(start_byte) = declaration.start_byte().checked_sub(start.column()) else {
        return false;
    };
    if page.revision() != definition.revision()
        || actual.is_empty()
        || !actual.contains(declaration)
        || actual.start_byte() != start_byte
        || actual.start_position() != SourcePosition::new(start.row(), 0)
        || actual.end_byte().saturating_sub(actual.start_byte()) as usize
            > crate::edit_phase::MAX_EDIT_BYTES
    {
        return false;
    }
    if end.column() == 0 {
        return actual.end_position() == end && actual.end_byte() == declaration.end_byte();
    }
    if end
        .row()
        .checked_add(1)
        .is_some_and(|row| actual.end_position() == SourcePosition::new(row, 0))
    {
        return actual.end_byte() > declaration.end_byte();
    }
    // A real complete-line SafeReader page may reach EOF without a final newline.
    actual.end_position().row() == end.row()
        && actual.end_position().column() >= end.column()
        && actual
            .end_position()
            .column()
            .checked_sub(end.column())
            .and_then(|tail| declaration.end_byte().checked_add(tail))
            == Some(actual.end_byte())
}

#[cfg(test)]
mod tests;
