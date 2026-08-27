use crate::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, GetModuleCardEvidence,
    JobContext, ModuleCardEvidenceControl, ModuleCardEvidenceControlError,
    ModuleCardEvidenceFailure, ModuleCardEvidenceFreshness, ModuleCardEvidenceLoadResult,
    ModuleCardEvidencePayload, ModuleCardEvidenceQuery, ModuleCardEvidenceStore,
    ModuleCardLifecycle, ProjectMapAtlasControl, ProjectMapAtlasControlError,
    ProjectMapAtlasFailure, ProjectMapAtlasLoadResult, ProjectMapAtlasStore,
    ProjectMapIndexEvidenceSelection,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, FileRevision, IndexLanguage,
    Progress, ProjectIdentity, RepositoryPath, SourceRange,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const MAX_PREVIEW_LINES: u16 = 64;
const CONTEXT_LINES: u32 = 8;
const MAX_PREVIEW_BYTES: usize = 16 * 1_024;
const MAX_PATH_DISPLAY_CHARS: usize = 512;

/// One exact Evidence selection previously issued by a Card or current static-index response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectMapSourcePreviewQuery {
    /// Current Evidence previously issued for a verified Module Card.
    ModuleCard(ModuleCardEvidenceQuery),
    /// Current Evidence previously issued by the progressive Atlas.
    Index(ProjectMapIndexEvidenceSelection),
}

/// Visible intersection of the selected Evidence range and the bounded source page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapSourceHighlight {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

impl ProjectMapSourceHighlight {
    /// Returns the one-based first highlighted line.
    #[must_use]
    pub const fn start_line(self) -> u32 {
        self.start_line
    }

    /// Returns the zero-based UTF-8 byte column of the highlight start.
    #[must_use]
    pub const fn start_column(self) -> u32 {
        self.start_column
    }

    /// Returns the one-based last highlighted line.
    #[must_use]
    pub const fn end_line(self) -> u32 {
        self.end_line
    }

    /// Returns the zero-based UTF-8 byte column of the highlight end.
    #[must_use]
    pub const fn end_column(self) -> u32 {
        self.end_column
    }
}

/// WebView-safe plain-text source page derived from one revalidated Evidence hook.
#[derive(Clone, PartialEq, Eq)]
pub struct ProjectMapSourcePreview {
    language: IndexLanguage,
    path_display: String,
    start_line: u32,
    line_count: u16,
    highlight: Option<ProjectMapSourceHighlight>,
    text: String,
    truncated_before: bool,
    truncated_after: bool,
}

impl ProjectMapSourcePreview {
    /// Returns the conservative language family inferred from the verified revision path.
    #[must_use]
    pub const fn language(&self) -> IndexLanguage {
        self.language
    }

    /// Returns bounded display-only path text that is never accepted as an input.
    #[must_use]
    pub fn path_display(&self) -> &str {
        &self.path_display
    }

    /// Returns the one-based first displayed line.
    #[must_use]
    pub const fn start_line(&self) -> u32 {
        self.start_line
    }

    /// Returns the exact number of complete displayed source lines.
    #[must_use]
    pub const fn line_count(&self) -> u16 {
        self.line_count
    }

    /// Returns the visible Evidence highlight, when the Evidence carries a source span.
    #[must_use]
    pub const fn highlight(&self) -> Option<ProjectMapSourceHighlight> {
        self.highlight
    }

    /// Returns bounded plain text, never markup.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns whether context before the displayed page was omitted.
    #[must_use]
    pub const fn truncated_before(&self) -> bool {
        self.truncated_before
    }

    /// Returns whether complete source lines remain after the displayed page.
    #[must_use]
    pub const fn truncated_after(&self) -> bool {
        self.truncated_after
    }
}

impl fmt::Debug for ProjectMapSourcePreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectMapSourcePreview")
            .field("language", &self.language)
            .field("path_display", &self.path_display)
            .field("start_line", &self.start_line)
            .field("line_count", &self.line_count)
            .field("highlight", &self.highlight)
            .field("text_bytes", &self.text.len())
            .field("truncated_before", &self.truncated_before)
            .field("truncated_after", &self.truncated_after)
            .finish()
    }
}

/// Availability result of the capability-bound source-preview request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectMapSourcePreviewResult {
    /// No index crossed the atomic publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic modules.
    ProjectionUnavailable,
    /// The selected primary module is no longer current.
    ModuleUnavailable,
    /// The module has no verified Card.
    CardUnavailable,
    /// A replacement publication invalidated the visible selection anchors.
    SelectionChanged,
    /// The opaque Evidence ID is not a member of the selected Card.
    EvidenceUnavailable,
    /// Historical or stale evidence is intentionally metadata-only.
    StaleEvidence,
    /// One current Evidence hook produced a bounded plain-text page.
    Available(ProjectMapSourcePreview),
}

/// Cooperative cancellation and bounded progress for one source-preview read.
pub trait ProjectMapSourcePreviewControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning UI generation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports only start and terminal progress, never source content.
    fn report_progress(
        &self,
        progress: Progress,
    ) -> Result<(), ProjectMapSourcePreviewControlError>;
}

impl ProjectMapSourcePreviewControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(
        &self,
        progress: Progress,
    ) -> Result<(), ProjectMapSourcePreviewControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ProjectMapSourcePreviewControlError::Unavailable)
    }
}

/// Progress could not reach the owning runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSourcePreviewControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ProjectMapSourcePreviewControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project map source-preview progress is unavailable")
    }
}

impl Error for ProjectMapSourcePreviewControlError {}

/// Stable content-free failure classes at the source-preview trust boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSourcePreviewFailure {
    /// Evidence selection or persistence validation failed.
    Evidence(ModuleCardEvidenceFailure),
    /// Current static-index Evidence selection or publication validation failed.
    IndexEvidence(ProjectMapAtlasFailure),
    /// The secure source reader rejected or could not revalidate the selected revision.
    Source(AgentSourceReadFailure),
    /// A lower boundary returned data outside the fixed preview contract.
    InvalidProjection,
    /// The owning UI generation cancelled the read.
    Cancelled,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ProjectMapSourcePreviewFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evidence(error) => write!(formatter, "source-preview evidence failed: {error}"),
            Self::IndexEvidence(error) => {
                write!(formatter, "source-preview index evidence failed: {error}")
            }
            Self::Source(error) => write!(formatter, "source-preview read failed: {error}"),
            Self::InvalidProjection => formatter.write_str("source-preview projection is invalid"),
            Self::Cancelled => formatter.write_str("source-preview read was cancelled"),
            Self::ProgressUnavailable => {
                formatter.write_str("source-preview progress is unavailable")
            }
        }
    }
}

impl Error for ProjectMapSourcePreviewFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Evidence(source) => Some(source),
            Self::IndexEvidence(source) => Some(source),
            Self::Source(source) => Some(source),
            Self::InvalidProjection | Self::Cancelled | Self::ProgressUnavailable => None,
        }
    }
}

/// Application use case that turns only a revalidated current Evidence hook into source text.
#[derive(Debug)]
pub struct GetProjectMapSourcePreview {
    evidence: GetModuleCardEvidence,
    atlas: Arc<dyn ProjectMapAtlasStore>,
    source: Arc<dyn AgentSourceReader>,
}

impl GetProjectMapSourcePreview {
    /// Wires the existing Evidence membership check to the safe workspace source reader.
    #[must_use]
    pub fn new(
        evidence_store: Arc<dyn ModuleCardEvidenceStore>,
        atlas_store: Arc<dyn ProjectMapAtlasStore>,
        source: Arc<dyn AgentSourceReader>,
    ) -> Self {
        Self {
            evidence: GetModuleCardEvidence::new(evidence_store),
            atlas: atlas_store,
            source,
        }
    }

    /// Resolves, revalidates, reads, and bounds one exact current Evidence selection.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapSourcePreviewQuery,
        control: &dyn ProjectMapSourcePreviewControl,
    ) -> Result<ProjectMapSourcePreviewResult, ProjectMapSourcePreviewFailure> {
        report(control, 0)?;
        checkpoint(control)?;
        let (revision, evidence_range) = match query {
            ProjectMapSourcePreviewQuery::ModuleCard(query) => {
                let evidence = self
                    .evidence
                    .execute(project, query, &PreviewEvidenceControl(control))
                    .await
                    .map_err(ProjectMapSourcePreviewFailure::Evidence)?;
                let detail = match evidence {
                    ModuleCardEvidenceLoadResult::NoPublishedIndex => {
                        return Ok(ProjectMapSourcePreviewResult::NoPublishedIndex);
                    }
                    ModuleCardEvidenceLoadResult::ProjectionUnavailable => {
                        return Ok(ProjectMapSourcePreviewResult::ProjectionUnavailable);
                    }
                    ModuleCardEvidenceLoadResult::ModuleUnavailable => {
                        return Ok(ProjectMapSourcePreviewResult::ModuleUnavailable);
                    }
                    ModuleCardEvidenceLoadResult::CardUnavailable => {
                        return Ok(ProjectMapSourcePreviewResult::CardUnavailable);
                    }
                    ModuleCardEvidenceLoadResult::SelectionChanged => {
                        return Ok(ProjectMapSourcePreviewResult::SelectionChanged);
                    }
                    ModuleCardEvidenceLoadResult::EvidenceUnavailable => {
                        return Ok(ProjectMapSourcePreviewResult::EvidenceUnavailable);
                    }
                    ModuleCardEvidenceLoadResult::Detail(detail) => detail,
                };
                if detail.freshness() != ModuleCardEvidenceFreshness::Current
                    || matches!(detail.card_lifecycle(), ModuleCardLifecycle::Stale { .. })
                {
                    return Ok(ProjectMapSourcePreviewResult::StaleEvidence);
                }
                let (revision, range) = preview_target(detail.payload());
                (revision.clone(), range)
            }
            ProjectMapSourcePreviewQuery::Index(selection) => {
                let target = self
                    .atlas
                    .load_index_evidence(project, *selection, &PreviewAtlasControl(control))
                    .await
                    .map_err(ProjectMapSourcePreviewFailure::IndexEvidence)?;
                match target {
                    ProjectMapAtlasLoadResult::NoPublishedIndex => {
                        return Ok(ProjectMapSourcePreviewResult::NoPublishedIndex);
                    }
                    ProjectMapAtlasLoadResult::ProjectionUnavailable => {
                        return Ok(ProjectMapSourcePreviewResult::ProjectionUnavailable);
                    }
                    ProjectMapAtlasLoadResult::SelectionChanged => {
                        return Ok(ProjectMapSourcePreviewResult::SelectionChanged);
                    }
                    ProjectMapAtlasLoadResult::Available(target) => {
                        (target.revision().clone(), target.range())
                    }
                }
            }
        };
        checkpoint(control)?;
        let (start_line, line_count) = preview_window(evidence_range)?;
        let request = AgentFileInspection::new(revision.path().clone(), start_line, line_count);
        let page = self
            .source
            .read_page(project, &revision, &request, &PreviewSourceControl(control))
            .await
            .map_err(map_source_failure)?;
        checkpoint(control)?;
        if page.revision() != &revision
            || page.start_line() != start_line
            || page.text().len() > MAX_PREVIEW_BYTES
        {
            return Err(ProjectMapSourcePreviewFailure::InvalidProjection);
        }
        let actual_line_count = source_line_count(page.text())?;
        if actual_line_count > MAX_PREVIEW_LINES {
            return Err(ProjectMapSourcePreviewFailure::InvalidProjection);
        }
        let preview = ProjectMapSourcePreview {
            language: language_for_path(revision.path()),
            path_display: path_display(revision.path()),
            start_line: start_line.get(),
            line_count: actual_line_count,
            highlight: evidence_range
                .and_then(|range| visible_highlight(range, start_line.get(), actual_line_count)),
            text: page.text().to_owned(),
            truncated_before: start_line.get() > 1,
            truncated_after: page.truncated(),
        };
        report(control, 2)?;
        Ok(ProjectMapSourcePreviewResult::Available(preview))
    }
}

#[derive(Debug)]
struct PreviewAtlasControl<'a>(&'a dyn ProjectMapSourcePreviewControl);

impl ProjectMapAtlasControl for PreviewAtlasControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), ProjectMapAtlasControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct PreviewEvidenceControl<'a>(&'a dyn ProjectMapSourcePreviewControl);

impl ModuleCardEvidenceControl for PreviewEvidenceControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), ModuleCardEvidenceControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct PreviewSourceControl<'a>(&'a dyn ProjectMapSourcePreviewControl);

impl AgentSourceReadControl for PreviewSourceControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn preview_target(payload: &ModuleCardEvidencePayload) -> (&FileRevision, Option<SourceRange>) {
    match payload {
        ModuleCardEvidencePayload::File { revision } => (revision, None),
        ModuleCardEvidencePayload::Symbol {
            revision,
            declaration_range,
            ..
        } => (revision, Some(*declaration_range)),
        ModuleCardEvidencePayload::GraphEdge { edge } => {
            (edge.evidence().revision(), Some(edge.evidence().range()))
        }
    }
}

fn preview_window(
    evidence_range: Option<SourceRange>,
) -> Result<(AgentFileStartLine, AgentFileLineCount), ProjectMapSourcePreviewFailure> {
    let Some(range) = evidence_range else {
        return bounded_window(1, MAX_PREVIEW_LINES);
    };
    let first = range
        .start_position()
        .row()
        .checked_add(1)
        .ok_or(ProjectMapSourcePreviewFailure::InvalidProjection)?;
    let last = inclusive_end_line(range)?;
    let target_lines = last
        .checked_sub(first)
        .and_then(|value| value.checked_add(1))
        .ok_or(ProjectMapSourcePreviewFailure::InvalidProjection)?;
    let start = first.saturating_sub(CONTEXT_LINES).max(1);
    let requested = target_lines
        .saturating_add(CONTEXT_LINES.saturating_mul(2))
        .min(u32::from(MAX_PREVIEW_LINES));
    let requested =
        u16::try_from(requested).map_err(|_| ProjectMapSourcePreviewFailure::InvalidProjection)?;
    bounded_window(start, requested)
}

fn bounded_window(
    start: u32,
    lines: u16,
) -> Result<(AgentFileStartLine, AgentFileLineCount), ProjectMapSourcePreviewFailure> {
    Ok((
        AgentFileStartLine::new(start)
            .map_err(|_| ProjectMapSourcePreviewFailure::InvalidProjection)?,
        AgentFileLineCount::new(lines)
            .map_err(|_| ProjectMapSourcePreviewFailure::InvalidProjection)?,
    ))
}

fn inclusive_end_line(range: SourceRange) -> Result<u32, ProjectMapSourcePreviewFailure> {
    let start_row = range.start_position().row();
    let end = range.end_position();
    if end.column() == 0 && end.row() > start_row {
        Ok(end.row())
    } else {
        end.row()
            .checked_add(1)
            .ok_or(ProjectMapSourcePreviewFailure::InvalidProjection)
    }
}

fn visible_highlight(
    range: SourceRange,
    page_start: u32,
    page_line_count: u16,
) -> Option<ProjectMapSourceHighlight> {
    if page_line_count == 0 {
        return None;
    }
    let first = range.start_position().row().checked_add(1)?;
    let last = inclusive_end_line(range).ok()?;
    let page_end = page_start.checked_add(u32::from(page_line_count) - 1)?;
    let visible_first = first.max(page_start);
    let visible_last = last.min(page_end);
    if visible_first > visible_last {
        return None;
    }
    Some(ProjectMapSourceHighlight {
        start_line: visible_first,
        start_column: if visible_first == first {
            range.start_position().column()
        } else {
            0
        },
        end_line: visible_last,
        end_column: if visible_last == last {
            range.end_position().column()
        } else {
            0
        },
    })
}

fn source_line_count(text: &str) -> Result<u16, ProjectMapSourcePreviewFailure> {
    if text.is_empty() {
        return Ok(0);
    }
    let newlines = text.bytes().filter(|byte| *byte == b'\n').count();
    let lines = newlines + usize::from(!text.ends_with('\n'));
    u16::try_from(lines).map_err(|_| ProjectMapSourcePreviewFailure::InvalidProjection)
}

fn language_for_path(path: &RepositoryPath) -> IndexLanguage {
    let lower = path
        .as_bytes()
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if lower.ends_with(b".rs") {
        IndexLanguage::Rust
    } else if [b".ts".as_slice(), b".tsx", b".js", b".jsx", b".svelte"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        IndexLanguage::TypeScriptJavaScript
    } else if lower.ends_with(b".py") {
        IndexLanguage::Python
    } else {
        IndexLanguage::Generic
    }
}

fn path_display(path: &RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .take(MAX_PATH_DISPLAY_CHARS)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn checkpoint(
    control: &dyn ProjectMapSourcePreviewControl,
) -> Result<(), ProjectMapSourcePreviewFailure> {
    if control.is_cancelled() {
        Err(ProjectMapSourcePreviewFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn report(
    control: &dyn ProjectMapSourcePreviewControl,
    completed: u64,
) -> Result<(), ProjectMapSourcePreviewFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ProjectMapSourcePreviewFailure::InvalidProjection)?,
        )
        .map_err(|_| ProjectMapSourcePreviewFailure::ProgressUnavailable)
}

const fn map_source_failure(error: AgentSourceReadFailure) -> ProjectMapSourcePreviewFailure {
    match error {
        AgentSourceReadFailure::Cancelled => ProjectMapSourcePreviewFailure::Cancelled,
        other => ProjectMapSourcePreviewFailure::Source(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, ModuleCardEvidenceId, ModuleId,
        RepositoryId, RepositoryIdentity, SourcePosition, SourceRange, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;

    #[derive(Debug)]
    struct SelectionChangedAtlas;

    impl ProjectMapAtlasStore for SelectionChangedAtlas {
        fn load_atlas_scene<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a crate::ProjectMapAtlasSceneQuery,
            _control: &'a dyn ProjectMapAtlasControl,
        ) -> crate::ProjectMapAtlasFuture<'a, crate::ProjectMapAtlasScene> {
            Box::pin(async { Ok(ProjectMapAtlasLoadResult::SelectionChanged) })
        }

        fn load_entity_context<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _selection: crate::ProjectMapEntitySelection,
            _control: &'a dyn ProjectMapAtlasControl,
        ) -> crate::ProjectMapAtlasFuture<'a, crate::ProjectMapEntityContext> {
            Box::pin(async { Ok(ProjectMapAtlasLoadResult::SelectionChanged) })
        }

        fn load_inventory_page<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a crate::ProjectMapInventoryPageQuery,
            _control: &'a dyn ProjectMapAtlasControl,
        ) -> crate::ProjectMapAtlasFuture<'a, crate::ProjectMapInventoryPage> {
            Box::pin(async { Ok(ProjectMapAtlasLoadResult::SelectionChanged) })
        }

        fn load_flow_scene<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a crate::ProjectMapFlowSceneQuery,
            _control: &'a dyn ProjectMapAtlasControl,
        ) -> crate::ProjectMapAtlasFuture<'a, crate::ProjectMapFlowScene> {
            Box::pin(async { Ok(ProjectMapAtlasLoadResult::SelectionChanged) })
        }

        fn load_index_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _selection: ProjectMapIndexEvidenceSelection,
            _control: &'a dyn ProjectMapAtlasControl,
        ) -> crate::ProjectMapAtlasFuture<'a, crate::ProjectMapIndexEvidenceTarget> {
            Box::pin(async { Ok(ProjectMapAtlasLoadResult::SelectionChanged) })
        }
    }

    #[derive(Debug)]
    struct UnusedEvidenceStore;

    impl ModuleCardEvidenceStore for UnusedEvidenceStore {
        fn load_module_card_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ModuleCardEvidenceQuery,
            _control: &'a dyn ModuleCardEvidenceControl,
        ) -> crate::ModuleCardEvidenceFuture<'a> {
            Box::pin(async { Ok(ModuleCardEvidenceLoadResult::NoPublishedIndex) })
        }
    }

    #[derive(Debug)]
    struct UnusedSourceReader;

    impl AgentSourceReader for UnusedSourceReader {
        fn read_page<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_revision: &'a FileRevision,
            _request: &'a AgentFileInspection,
            _control: &'a dyn AgentSourceReadControl,
        ) -> crate::AgentSourceReaderFuture<'a> {
            Box::pin(async { Err(AgentSourceReadFailure::Cancelled) })
        }
    }

    #[derive(Debug)]
    struct AcceptingControl;

    impl ProjectMapSourcePreviewControl for AcceptingControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(
            &self,
            _progress: Progress,
        ) -> Result<(), ProjectMapSourcePreviewControlError> {
            Ok(())
        }
    }

    #[test]
    fn graph_window_has_exact_eight_line_context_and_fixed_cap() -> Result<(), Box<dyn Error>> {
        let range = SourceRange::new(
            10,
            20,
            SourcePosition::new(20, 2),
            SourcePosition::new(23, 5),
        )?;
        let (start, count) = preview_window(Some(range))?;

        assert_eq!(start.get(), 13);
        assert_eq!(count.get(), 20);
        assert_eq!(
            visible_highlight(range, start.get(), count.get()),
            Some(ProjectMapSourceHighlight {
                start_line: 21,
                start_column: 2,
                end_line: 24,
                end_column: 5,
            })
        );
        Ok(())
    }

    #[test]
    fn long_evidence_is_bounded_to_sixty_four_complete_lines() -> Result<(), Box<dyn Error>> {
        let range = SourceRange::new(
            0,
            100,
            SourcePosition::new(100, 0),
            SourcePosition::new(300, 1),
        )?;
        let (start, count) = preview_window(Some(range))?;

        assert_eq!(start.get(), 93);
        assert_eq!(count.get(), MAX_PREVIEW_LINES);
        Ok(())
    }

    #[test]
    fn source_line_count_handles_terminal_newline_without_inventing_a_line()
    -> Result<(), Box<dyn Error>> {
        assert_eq!(source_line_count("one\ntwo\n")?, 2);
        assert_eq!(source_line_count("one\ntwo")?, 2);
        assert_eq!(source_line_count("")?, 0);
        Ok(())
    }

    #[test]
    fn replacement_publish_invalidates_index_preview_selection() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let query = ProjectMapSourcePreviewQuery::Index(ProjectMapIndexEvidenceSelection::File {
            module_id: ModuleId::from_bytes([6; 32]),
            ordinal: crate::ProjectMapFileOrdinal::new(1)?,
            evidence_id: ModuleCardEvidenceId::from_bytes([7; 32]),
        });
        let preview = GetProjectMapSourcePreview::new(
            Arc::new(UnusedEvidenceStore),
            Arc::new(SelectionChangedAtlas),
            Arc::new(UnusedSourceReader),
        );

        assert_eq!(
            block_on(preview.execute(&project, &query, &AcceptingControl))?,
            ProjectMapSourcePreviewResult::SelectionChanged
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([3; 32]),
            WorktreeAnchorId::from_bytes([4; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
