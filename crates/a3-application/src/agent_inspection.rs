use a3_domain::{
    AgentRunId, ContentHash, DiscoveredCommandKind, PatchContentPreview, PatchLineEndings,
    PatchPreview, PatchPreviewEntry, PatchTextEncoding, ProcessDuration, ProcessOutputDigest,
    ProcessOutputRedaction, ProcessRunResult, ProcessStream, ProcessTermination, ProjectIdentity,
    RepositoryPath, SnapshotId, TaskId, TaskStepId, ToolRunId, VerificationMethod,
    VerificationSpecId, WorktreeId,
};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

const DIFF_CONTEXT_LINES: usize = 3;
const MAX_LCS_CELLS: usize = 1_000_000;
const MAX_PROCESS_INSPECTIONS: usize = 32;
const MAX_PROCESS_INSPECTION_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_LOG_PAGE_BYTES: u32 = 16 * 1_024;

/// Exact durable anchors shared by one volatile patch or process inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentInspectionContext {
    task_id: TaskId,
    run_id: AgentRunId,
    step_id: TaskStepId,
    verification_spec_id: VerificationSpecId,
    snapshot_id: SnapshotId,
}

impl AgentInspectionContext {
    /// Binds presentation data to the task-selected mutating action that produced it.
    #[must_use]
    pub const fn new(
        task_id: TaskId,
        run_id: AgentRunId,
        step_id: TaskStepId,
        verification_spec_id: VerificationSpecId,
        snapshot_id: SnapshotId,
    ) -> Self {
        Self {
            task_id,
            run_id,
            step_id,
            verification_spec_id,
            snapshot_id,
        }
    }

    /// Returns the durable task selected by the Agent workspace.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    /// Returns the controlled run that observed the presentation data.
    #[must_use]
    pub const fn run_id(self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact plan step owning the data.
    #[must_use]
    pub const fn step_id(self) -> TaskStepId {
        self.step_id
    }

    /// Returns the immutable operational verification specification.
    #[must_use]
    pub const fn verification_spec_id(self) -> VerificationSpecId {
        self.verification_spec_id
    }

    /// Returns the repository snapshot against which the data was observed.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }
}

/// Core-generated identity for one volatile inspection record.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentInspectionId([u8; 32]);

impl AgentInspectionId {
    /// Reconstructs an untrusted opaque identifier before record-level revalidation.
    #[must_use]
    pub const fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }

    /// Returns the stable binary representation used by the strict IPC mapper.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for AgentInspectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AgentInspectionId([REDACTED])")
    }
}

/// Monotone volatile revision used to reject stale WebView detail requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentInspectionRevision(u64);

impl AgentInspectionRevision {
    /// Reconstructs a positive process-local revision emitted by an overview.
    pub const fn new(value: u64) -> Result<Self, AgentInspectionRevisionError> {
        if value == 0 {
            return Err(AgentInspectionRevisionError);
        }
        Ok(Self(value))
    }

    /// Returns the positive process-local revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A WebView-supplied inspection revision was not positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentInspectionRevisionError;

impl fmt::Display for AgentInspectionRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent inspection revision must be positive")
    }
}

impl Error for AgentInspectionRevisionError {}

/// Provenance asserted only when a trusted action or observer proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentChangeAttribution {
    /// The exact E3 action is proposed and has not yet been applied.
    ProposedAgent,
    /// An actual E3 change set proves the Agent applied the transition.
    AppliedAgent,
    /// A trusted observer explicitly classified the transition as outside the Agent.
    External,
    /// No reliable actor evidence exists.
    Unattributed,
}

/// Closed file operation shown in the patch overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDiffFileOperation {
    /// A previously absent path is proposed.
    Add,
    /// One existing path receives new content.
    Update,
    /// Existing content moves to another path.
    Move,
    /// Existing content is removed.
    Delete,
}

/// Exact line terminator retained separately from display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDiffLineEnding {
    /// Line feed.
    Lf,
    /// Carriage return followed by line feed.
    Crlf,
    /// Carriage return.
    Cr,
    /// The retained prefix ended without a terminator.
    None,
}

/// One exact retained line with its original terminator classification.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentDiffLine {
    text: String,
    ending: AgentDiffLineEnding,
}

impl AgentDiffLine {
    /// Returns the line text without its separately represented terminator.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the exact terminator following the retained line text.
    #[must_use]
    pub const fn ending(&self) -> AgentDiffLineEnding {
        self.ending
    }
}

impl fmt::Debug for AgentDiffLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentDiffLine")
            .field("text_bytes", &self.text.len())
            .field("ending", &self.ending)
            .finish()
    }
}

/// One shared row from which unified and side-by-side views are rendered.
#[derive(Clone, PartialEq, Eq)]
pub enum AgentDiffRow {
    /// The same exact retained line exists on both sides.
    Context {
        /// One-based line number in the prior content.
        before_line: u32,
        /// One-based line number in the proposed content.
        after_line: u32,
        /// Exact retained line.
        line: AgentDiffLine,
    },
    /// The prior side contains a removed line.
    Removed {
        /// One-based line number in the prior content.
        before_line: u32,
        /// Exact retained prior line.
        line: AgentDiffLine,
    },
    /// The proposed side contains an added line.
    Added {
        /// One-based line number in the proposed content.
        after_line: u32,
        /// Exact retained proposed line.
        line: AgentDiffLine,
    },
}

impl fmt::Debug for AgentDiffRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context {
                before_line,
                after_line,
                line,
            } => formatter
                .debug_struct("Context")
                .field("before_line", before_line)
                .field("after_line", after_line)
                .field("line", line)
                .finish(),
            Self::Removed { before_line, line } => formatter
                .debug_struct("Removed")
                .field("before_line", before_line)
                .field("line", line)
                .finish(),
            Self::Added { after_line, line } => formatter
                .debug_struct("Added")
                .field("after_line", after_line)
                .field("line", line)
                .finish(),
        }
    }
}

/// One deterministic bounded changed region with conventional line coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDiffHunk {
    before_start: u32,
    before_count: u32,
    after_start: u32,
    after_count: u32,
    rows: Vec<AgentDiffRow>,
}

impl AgentDiffHunk {
    /// Returns the first prior line or insertion coordinate.
    #[must_use]
    pub const fn before_start(&self) -> u32 {
        self.before_start
    }

    /// Returns the number of prior-side rows covered by the hunk.
    #[must_use]
    pub const fn before_count(&self) -> u32 {
        self.before_count
    }

    /// Returns the first proposed line or deletion coordinate.
    #[must_use]
    pub const fn after_start(&self) -> u32 {
        self.after_start
    }

    /// Returns the number of proposed-side rows covered by the hunk.
    #[must_use]
    pub const fn after_count(&self) -> u32 {
        self.after_count
    }

    /// Returns the exact rows shared by both visual layouts.
    #[must_use]
    pub fn rows(&self) -> &[AgentDiffRow] {
        &self.rows
    }
}

/// Full-content metadata plus the exact E3-retained prefix for one side of a file.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentDiffContent {
    text: String,
    total_bytes: u64,
    content_hash: ContentHash,
    encoding: PatchTextEncoding,
    line_endings: PatchLineEndings,
    truncated: bool,
}

impl AgentDiffContent {
    fn from_preview(value: &PatchContentPreview) -> Result<Self, AgentInspectionBuildError> {
        let text = std::str::from_utf8(value.bytes())
            .map_err(|_| AgentInspectionBuildError::InvalidPreview)?
            .to_owned();
        Ok(Self {
            text,
            total_bytes: value.total_bytes(),
            content_hash: value.content_hash(),
            encoding: value.encoding(),
            line_endings: value.line_endings(),
            truncated: value.truncated(),
        })
    }

    /// Returns the exact retained UTF-8 prefix.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the complete content byte count, including an omitted tail.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the complete content hash, not a prefix hash.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    /// Returns the complete content encoding classification.
    #[must_use]
    pub const fn encoding(&self) -> PatchTextEncoding {
        self.encoding
    }

    /// Returns the complete content line-ending classification.
    #[must_use]
    pub const fn line_endings(&self) -> PatchLineEndings {
        self.line_endings
    }

    /// Returns whether E3 deliberately omitted a tail from this exact prefix.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for AgentDiffContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentDiffContent")
            .field("retained_bytes", &self.text.len())
            .field("total_bytes", &self.total_bytes)
            .field("content_hash", &self.content_hash)
            .field("encoding", &self.encoding)
            .field("line_endings", &self.line_endings)
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// One exact file-level patch projection and its deterministic hunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDiffFile {
    operation: AgentDiffFileOperation,
    source_path: Option<RepositoryPath>,
    target_path: Option<RepositoryPath>,
    before: Option<AgentDiffContent>,
    after: Option<AgentDiffContent>,
    hunks: Vec<AgentDiffHunk>,
    added_lines: u32,
    removed_lines: u32,
    attribution: AgentChangeAttribution,
}

impl AgentDiffFile {
    fn from_preview(entry: &PatchPreviewEntry) -> Result<Self, AgentInspectionBuildError> {
        let before = entry
            .before()
            .map(AgentDiffContent::from_preview)
            .transpose()?;
        let after = entry
            .after()
            .map(AgentDiffContent::from_preview)
            .transpose()?;
        let operation = classify_operation(entry)?;
        let before_lines = before
            .as_ref()
            .map_or_else(Vec::new, |content| split_exact_lines(content.text()));
        let after_lines = after
            .as_ref()
            .map_or_else(Vec::new, |content| split_exact_lines(content.text()));
        let operations = diff_lines(&before_lines, &after_lines)?;
        let added_lines = checked_u32(
            operations
                .iter()
                .filter(|operation| matches!(operation, LineOperation::Added(_)))
                .count(),
        )?;
        let removed_lines = checked_u32(
            operations
                .iter()
                .filter(|operation| matches!(operation, LineOperation::Removed(_)))
                .count(),
        )?;
        Ok(Self {
            operation,
            source_path: entry.source_path().cloned(),
            target_path: entry.target_path().cloned(),
            before,
            after,
            hunks: build_hunks(&operations)?,
            added_lines,
            removed_lines,
            attribution: AgentChangeAttribution::ProposedAgent,
        })
    }

    /// Returns Add, Update, Move, or Delete.
    #[must_use]
    pub const fn operation(&self) -> AgentDiffFileOperation {
        self.operation
    }

    /// Returns the prior path for Update, Move, or Delete.
    #[must_use]
    pub const fn source_path(&self) -> Option<&RepositoryPath> {
        self.source_path.as_ref()
    }

    /// Returns the proposed path for Add, Update, or Move.
    #[must_use]
    pub const fn target_path(&self) -> Option<&RepositoryPath> {
        self.target_path.as_ref()
    }

    /// Returns prior full-content metadata and its exact retained prefix.
    #[must_use]
    pub const fn before(&self) -> Option<&AgentDiffContent> {
        self.before.as_ref()
    }

    /// Returns proposed full-content metadata and its exact retained prefix.
    #[must_use]
    pub const fn after(&self) -> Option<&AgentDiffContent> {
        self.after.as_ref()
    }

    /// Returns deterministic changed regions with three context lines.
    #[must_use]
    pub fn hunks(&self) -> &[AgentDiffHunk] {
        &self.hunks
    }

    /// Returns the exact added-line count within the retained prefixes.
    #[must_use]
    pub const fn added_lines(&self) -> u32 {
        self.added_lines
    }

    /// Returns the exact removed-line count within the retained prefixes.
    #[must_use]
    pub const fn removed_lines(&self) -> u32 {
        self.removed_lines
    }

    /// Returns the trusted action provenance.
    #[must_use]
    pub const fn attribution(&self) -> AgentChangeAttribution {
        self.attribution
    }

    /// Returns whether either exact E3 prefix omitted a content tail.
    #[must_use]
    pub fn content_truncated(&self) -> bool {
        self.before
            .as_ref()
            .is_some_and(AgentDiffContent::truncated)
            || self.after.as_ref().is_some_and(AgentDiffContent::truncated)
    }
}

/// Complete volatile exact patch view recorded before policy approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPatchInspection {
    id: AgentInspectionId,
    context: AgentInspectionContext,
    files: Vec<AgentDiffFile>,
    retained_bytes: u64,
}

impl AgentPatchInspection {
    /// Projects the already bounded E3 preview without reading the worktree.
    pub fn from_preview(
        context: AgentInspectionContext,
        preview: &PatchPreview,
    ) -> Result<Self, AgentInspectionBuildError> {
        if context.snapshot_id() != preview.snapshot_id() || preview.entries().is_empty() {
            return Err(AgentInspectionBuildError::AnchorMismatch);
        }
        let files = preview
            .entries()
            .iter()
            .map(AgentDiffFile::from_preview)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: derive_patch_inspection_id(context, preview),
            context,
            files,
            retained_bytes: u64::try_from(preview.retained_bytes())
                .map_err(|_| AgentInspectionBuildError::TooMuchData)?,
        })
    }

    /// Returns the Core-derived volatile record identity.
    #[must_use]
    pub const fn id(&self) -> AgentInspectionId {
        self.id
    }

    /// Returns exact task, run, step, verification, and snapshot anchors.
    #[must_use]
    pub const fn context(&self) -> AgentInspectionContext {
        self.context
    }

    /// Returns files in the canonical E3 action order.
    #[must_use]
    pub fn files(&self) -> &[AgentDiffFile] {
        &self.files
    }

    /// Returns retained bytes across both sides of every file.
    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
}

/// Invalid trusted inputs could not form one bounded inspection projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInspectionBuildError {
    /// Preview and task context did not share the same immutable anchors.
    AnchorMismatch,
    /// A supposedly valid E3 preview contained an impossible file shape or UTF-8 prefix.
    InvalidPreview,
    /// Bounded arithmetic or allocation limits were exceeded.
    TooMuchData,
}

impl fmt::Display for AgentInspectionBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AnchorMismatch => "Agent inspection anchors do not match",
            Self::InvalidPreview => "Agent inspection preview is invalid",
            Self::TooMuchData => "Agent inspection exceeds its fixed boundary",
        })
    }
}

impl Error for AgentInspectionBuildError {}

/// Product-facing classification for a completed discovered command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProcessInspectionKind {
    /// Structured test verification.
    Test,
    /// Build command.
    Build,
    /// Structured diagnostic verification.
    Diagnostic,
    /// Lint command without a structured diagnostic adapter.
    Lint,
    /// Formatting command.
    Format,
    /// Generic command verification.
    Command,
}

impl AgentProcessInspectionKind {
    /// Uses verification semantics first, then the closed E5 command category.
    #[must_use]
    pub const fn classify(method: VerificationMethod, command_kind: DiscoveredCommandKind) -> Self {
        match method {
            VerificationMethod::Test => Self::Test,
            VerificationMethod::Diagnostic => Self::Diagnostic,
            VerificationMethod::Command
            | VerificationMethod::DiffInvariant
            | VerificationMethod::UserConfirm => match command_kind {
                DiscoveredCommandKind::Test => Self::Test,
                DiscoveredCommandKind::Build => Self::Build,
                DiscoveredCommandKind::Lint => Self::Lint,
                DiscoveredCommandKind::Format => Self::Format,
            },
        }
    }
}

/// Content-free process summary used before a log page is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentProcessStreamSummary {
    digest: ProcessOutputDigest,
    observed_bytes: u64,
    retained_bytes: u64,
    retained_limit: u32,
    source_truncated: bool,
    redaction: Option<ProcessOutputRedaction>,
}

impl AgentProcessStreamSummary {
    /// Returns the digest of the completely drained stream.
    #[must_use]
    pub const fn digest(self) -> ProcessOutputDigest {
        self.digest
    }

    /// Returns bytes observed including discarded overflow.
    #[must_use]
    pub const fn observed_bytes(self) -> u64 {
        self.observed_bytes
    }

    /// Returns safe text bytes available for explicit paging.
    #[must_use]
    pub const fn retained_bytes(self) -> u64 {
        self.retained_bytes
    }

    /// Returns the E4 retention cap.
    #[must_use]
    pub const fn retained_limit(self) -> u32 {
        self.retained_limit
    }

    /// Returns whether overflow beyond retained bytes was permanently discarded.
    #[must_use]
    pub const fn source_truncated(self) -> bool {
        self.source_truncated
    }

    /// Returns why retained content was withheld, if applicable.
    #[must_use]
    pub const fn redaction(self) -> Option<ProcessOutputRedaction> {
        self.redaction
    }
}

/// One completed process retained only by the current privileged runtime.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentProcessInspection {
    id: AgentInspectionId,
    context: AgentInspectionContext,
    tool_run_id: ToolRunId,
    kind: AgentProcessInspectionKind,
    result: ProcessRunResult,
}

impl AgentProcessInspection {
    fn new(
        context: AgentInspectionContext,
        tool_run_id: ToolRunId,
        kind: AgentProcessInspectionKind,
        result: ProcessRunResult,
    ) -> Self {
        Self {
            id: derive_process_inspection_id(context, tool_run_id, &result),
            context,
            tool_run_id,
            kind,
            result,
        }
    }

    /// Returns the Core-derived volatile record identity.
    #[must_use]
    pub const fn id(&self) -> AgentInspectionId {
        self.id
    }

    /// Returns exact task, run, step, verification, and snapshot anchors.
    #[must_use]
    pub const fn context(&self) -> AgentInspectionContext {
        self.context
    }

    /// Returns the bounded tool attempt identity.
    #[must_use]
    pub const fn tool_run_id(&self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the product-facing command classification.
    #[must_use]
    pub const fn kind(&self) -> AgentProcessInspectionKind {
        self.kind
    }

    /// Returns exit, timeout, or cancellation without inferring verification success.
    #[must_use]
    pub const fn termination(&self) -> ProcessTermination {
        self.result.termination()
    }

    /// Returns the monotonic process duration.
    #[must_use]
    pub const fn duration(&self) -> ProcessDuration {
        self.result.duration()
    }

    /// Returns content-free metadata for one stream.
    #[must_use]
    pub fn stream_summary(&self, stream: ProcessStream) -> AgentProcessStreamSummary {
        let capture = match stream {
            ProcessStream::Stdout => self.result.stdout(),
            ProcessStream::Stderr => self.result.stderr(),
        };
        AgentProcessStreamSummary {
            digest: capture.digest(),
            observed_bytes: capture.observed_bytes(),
            retained_bytes: capture
                .content()
                .as_text()
                .and_then(|text| u64::try_from(text.len()).ok())
                .unwrap_or(0),
            retained_limit: capture.retained_limit(),
            source_truncated: capture.truncated(),
            redaction: capture.content().redaction(),
        }
    }

    fn retained_bytes(&self) -> usize {
        [self.result.stdout(), self.result.stderr()]
            .into_iter()
            .filter_map(|capture| capture.content().as_text())
            .map(str::len)
            .sum()
    }

    fn log_page(
        &self,
        stream: ProcessStream,
        offset: AgentLogPageOffset,
        limit: AgentLogPageLimit,
    ) -> Result<AgentProcessLogPage, AgentInspectionQueryError> {
        let capture = match stream {
            ProcessStream::Stdout => self.result.stdout(),
            ProcessStream::Stderr => self.result.stderr(),
        };
        let summary = self.stream_summary(stream);
        let Some(text) = capture.content().as_text() else {
            if offset.get() != 0 {
                return Err(AgentInspectionQueryError::InvalidCursor);
            }
            return Ok(AgentProcessLogPage {
                text: String::new(),
                offset,
                next_offset: None,
                page_truncated: false,
                source_truncated: summary.source_truncated(),
                redaction: summary.redaction(),
            });
        };
        let start =
            usize::try_from(offset.get()).map_err(|_| AgentInspectionQueryError::InvalidCursor)?;
        if start > text.len() || !text.is_char_boundary(start) {
            return Err(AgentInspectionQueryError::InvalidCursor);
        }
        let requested_end = start
            .checked_add(
                usize::try_from(limit.get())
                    .map_err(|_| AgentInspectionQueryError::InvalidCursor)?,
            )
            .ok_or(AgentInspectionQueryError::InvalidCursor)?;
        let mut end = requested_end.min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        let page_truncated = end < text.len();
        let next_offset = if page_truncated {
            Some(AgentLogPageOffset(
                u32::try_from(end).map_err(|_| AgentInspectionQueryError::InvalidCursor)?,
            ))
        } else {
            None
        };
        Ok(AgentProcessLogPage {
            text: text[start..end].to_owned(),
            offset,
            next_offset,
            page_truncated,
            source_truncated: summary.source_truncated(),
            redaction: None,
        })
    }
}

impl fmt::Debug for AgentProcessInspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentProcessInspection")
            .field("id", &self.id)
            .field("context", &self.context)
            .field("tool_run_id", &self.tool_run_id)
            .field("kind", &self.kind)
            .field("termination", &self.result.termination())
            .field("duration", &self.result.duration())
            .field("stdout", &self.stream_summary(ProcessStream::Stdout))
            .field("stderr", &self.stream_summary(ProcessStream::Stderr))
            .finish()
    }
}

/// Byte cursor into one safe retained process stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentLogPageOffset(u32);

impl AgentLogPageOffset {
    /// Starts at the first retained byte.
    pub const START: Self = Self(0);

    /// Reconstructs a byte offset; the selected stream validates UTF-8 boundaries.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the portable byte offset.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Positive log-page limit capped at sixteen KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentLogPageLimit(u32);

impl AgentLogPageLimit {
    /// Default page used for an explicit detail load.
    pub const DEFAULT: Self = Self(8 * 1_024);

    /// Creates a positive bounded page size.
    pub const fn new(value: u32) -> Result<Self, AgentLogPageLimitError> {
        if value == 0 || value > MAX_LOG_PAGE_BYTES {
            return Err(AgentLogPageLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the portable byte limit.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Requested log-page size was zero or exceeded sixteen KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentLogPageLimitError {
    value: u32,
}

impl fmt::Display for AgentLogPageLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Agent log page limit {} must be between 1 and {MAX_LOG_PAGE_BYTES}",
            self.value
        )
    }
}

impl Error for AgentLogPageLimitError {}

/// One explicit safe retained log page with both truncation kinds kept separate.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentProcessLogPage {
    text: String,
    offset: AgentLogPageOffset,
    next_offset: Option<AgentLogPageOffset>,
    page_truncated: bool,
    source_truncated: bool,
    redaction: Option<ProcessOutputRedaction>,
}

impl AgentProcessLogPage {
    /// Returns the safe retained page text, empty for a redacted stream.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the byte offset represented by this page.
    #[must_use]
    pub const fn offset(&self) -> AgentLogPageOffset {
        self.offset
    }

    /// Returns the only valid cursor for an explicitly requested following page.
    #[must_use]
    pub const fn next_offset(&self) -> Option<AgentLogPageOffset> {
        self.next_offset
    }

    /// Returns whether additional retained safe text can be loaded.
    #[must_use]
    pub const fn page_truncated(&self) -> bool {
        self.page_truncated
    }

    /// Returns whether E4 permanently discarded overflow beyond the retained limit.
    #[must_use]
    pub const fn source_truncated(&self) -> bool {
        self.source_truncated
    }

    /// Returns why the complete retained stream was withheld, if applicable.
    #[must_use]
    pub const fn redaction(&self) -> Option<ProcessOutputRedaction> {
        self.redaction
    }
}

impl fmt::Debug for AgentProcessLogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentProcessLogPage")
            .field("text_bytes", &self.text.len())
            .field("offset", &self.offset)
            .field("next_offset", &self.next_offset)
            .field("page_truncated", &self.page_truncated)
            .field("source_truncated", &self.source_truncated)
            .field("redaction", &self.redaction)
            .finish()
    }
}

/// Content-free process row returned with a current volatile overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentProcessInspectionSummary {
    id: AgentInspectionId,
    context: AgentInspectionContext,
    kind: AgentProcessInspectionKind,
    termination: ProcessTermination,
    duration: ProcessDuration,
    stdout: AgentProcessStreamSummary,
    stderr: AgentProcessStreamSummary,
}

impl AgentProcessInspectionSummary {
    fn from_inspection(inspection: &AgentProcessInspection) -> Self {
        Self {
            id: inspection.id(),
            context: inspection.context(),
            kind: inspection.kind(),
            termination: inspection.termination(),
            duration: inspection.duration(),
            stdout: inspection.stream_summary(ProcessStream::Stdout),
            stderr: inspection.stream_summary(ProcessStream::Stderr),
        }
    }

    /// Returns the Core-issued log selection identity.
    #[must_use]
    pub const fn id(self) -> AgentInspectionId {
        self.id
    }

    /// Returns the exact durable anchors.
    #[must_use]
    pub const fn context(self) -> AgentInspectionContext {
        self.context
    }

    /// Returns Test, Build, Diagnostic, Lint, Format, or generic Command.
    #[must_use]
    pub const fn kind(self) -> AgentProcessInspectionKind {
        self.kind
    }

    /// Returns process termination without claiming verification success.
    #[must_use]
    pub const fn termination(self) -> ProcessTermination {
        self.termination
    }

    /// Returns monotonic process duration.
    #[must_use]
    pub const fn duration(self) -> ProcessDuration {
        self.duration
    }

    /// Returns stdout metadata without text.
    #[must_use]
    pub const fn stdout(self) -> AgentProcessStreamSummary {
        self.stdout
    }

    /// Returns stderr metadata without text.
    #[must_use]
    pub const fn stderr(self) -> AgentProcessStreamSummary {
        self.stderr
    }
}

/// Current task-bound volatile projection; durable verification is loaded separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInspectionOverview {
    revision: AgentInspectionRevision,
    patch: Option<AgentPatchInspection>,
    processes: Vec<AgentProcessInspectionSummary>,
}

impl AgentInspectionOverview {
    /// Returns the revision required by subsequent detail-page requests.
    #[must_use]
    pub const fn revision(&self) -> AgentInspectionRevision {
        self.revision
    }

    /// Returns the current exact pre-approval patch, when retained.
    #[must_use]
    pub const fn patch(&self) -> Option<&AgentPatchInspection> {
        self.patch.as_ref()
    }

    /// Returns bounded completed process summaries in observation order.
    #[must_use]
    pub fn processes(&self) -> &[AgentProcessInspectionSummary] {
        &self.processes
    }
}

/// Synchronous no-I/O observer injected into the finite mutating controller.
pub trait AgentInspectionSink: fmt::Debug + Send + Sync {
    /// Retains an exact E3 preview so ApprovalRequired can be inspected fail-closed.
    fn record_patch_preview(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        preview: &PatchPreview,
    ) -> Result<AgentInspectionId, AgentInspectionSinkFailure>;

    /// Retains already secret-checked E4 output; failure must not change process disposition.
    fn record_process_result(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        tool_run_id: ToolRunId,
        kind: AgentProcessInspectionKind,
        result: &ProcessRunResult,
    ) -> Result<AgentInspectionId, AgentInspectionSinkFailure>;
}

/// Bounded in-memory inspection owner instantiated and lifecycle-managed by the composition root.
pub struct AgentInspectionBuffer {
    state: Mutex<AgentInspectionBufferState>,
}

impl AgentInspectionBuffer {
    /// Creates an inactive empty buffer; project activation is explicit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(AgentInspectionBufferState::default()),
        }
    }

    /// Activates one worktree and clears data from any previous project.
    pub fn activate_project(&self, project: &ProjectIdentity) {
        let mut state = lock_recovering_poison(&self.state);
        let worktree_id = project.worktree().id();
        if state.active_worktree_id != Some(worktree_id) {
            state.clear_records();
            state.active_worktree_id = Some(worktree_id);
        }
    }

    /// Clears volatile source and log data during removal, switch, or shutdown.
    pub fn deactivate_project(&self) {
        let mut state = lock_recovering_poison(&self.state);
        state.clear_records();
        state.active_worktree_id = None;
    }

    /// Returns a content-bounded task overview without process text.
    pub fn overview(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
    ) -> Result<Option<AgentInspectionOverview>, AgentInspectionQueryError> {
        let state = lock_recovering_poison(&self.state);
        state.ensure_project(project)?;
        let patch = state
            .patch
            .as_ref()
            .filter(|patch| patch.context().task_id() == task_id)
            .cloned();
        let processes = state
            .processes
            .iter()
            .filter(|process| process.context().task_id() == task_id)
            .map(AgentProcessInspectionSummary::from_inspection)
            .collect::<Vec<_>>();
        if patch.is_none() && processes.is_empty() {
            return Ok(None);
        }
        Ok(Some(AgentInspectionOverview {
            revision: state
                .revision
                .ok_or(AgentInspectionQueryError::Unavailable)?,
            patch,
            processes,
        }))
    }

    /// Loads one retained log page after revalidating task, revision, and record ID.
    #[allow(clippy::too_many_arguments)]
    pub fn load_process_log_page(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_revision: AgentInspectionRevision,
        inspection_id: AgentInspectionId,
        stream: ProcessStream,
        offset: AgentLogPageOffset,
        limit: AgentLogPageLimit,
    ) -> Result<AgentProcessLogPage, AgentInspectionQueryError> {
        let state = lock_recovering_poison(&self.state);
        state.ensure_project(project)?;
        if state.revision != Some(expected_revision) {
            return Err(AgentInspectionQueryError::RevisionChanged);
        }
        let process = state
            .processes
            .iter()
            .find(|process| process.id() == inspection_id && process.context().task_id() == task_id)
            .ok_or(AgentInspectionQueryError::RecordUnavailable)?;
        process.log_page(stream, offset, limit)
    }
}

impl Default for AgentInspectionBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AgentInspectionBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock_recovering_poison(&self.state);
        formatter
            .debug_struct("AgentInspectionBuffer")
            .field("active", &state.active_worktree_id.is_some())
            .field("has_patch", &state.patch.is_some())
            .field("process_count", &state.processes.len())
            .field("retained_process_bytes", &state.retained_process_bytes)
            .finish()
    }
}

impl AgentInspectionSink for AgentInspectionBuffer {
    fn record_patch_preview(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        preview: &PatchPreview,
    ) -> Result<AgentInspectionId, AgentInspectionSinkFailure> {
        let patch = AgentPatchInspection::from_preview(context, preview)
            .map_err(AgentInspectionSinkFailure::InvalidInspection)?;
        let mut state = lock_recovering_poison(&self.state);
        state.ensure_active_project(project)?;
        state.retain_compatible_context(context);
        let id = patch.id();
        state.patch = Some(patch);
        state.advance_revision()?;
        Ok(id)
    }

    fn record_process_result(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        tool_run_id: ToolRunId,
        kind: AgentProcessInspectionKind,
        result: &ProcessRunResult,
    ) -> Result<AgentInspectionId, AgentInspectionSinkFailure> {
        let process = AgentProcessInspection::new(context, tool_run_id, kind, result.clone());
        let process_bytes = process.retained_bytes();
        if process_bytes > MAX_PROCESS_INSPECTION_BYTES {
            return Err(AgentInspectionSinkFailure::CapacityExceeded);
        }
        let mut state = lock_recovering_poison(&self.state);
        state.ensure_active_project(project)?;
        state.retain_compatible_context(context);
        while state.processes.len() >= MAX_PROCESS_INSPECTIONS
            || state
                .retained_process_bytes
                .checked_add(process_bytes)
                .is_none_or(|total| total > MAX_PROCESS_INSPECTION_BYTES)
        {
            let Some(evicted) = state.processes.pop_front() else {
                break;
            };
            state.retained_process_bytes = state
                .retained_process_bytes
                .saturating_sub(evicted.retained_bytes());
        }
        state.retained_process_bytes = state
            .retained_process_bytes
            .checked_add(process_bytes)
            .ok_or(AgentInspectionSinkFailure::CapacityExceeded)?;
        let id = process.id();
        state.processes.push_back(process);
        state.advance_revision()?;
        Ok(id)
    }
}

#[derive(Default)]
struct AgentInspectionBufferState {
    active_worktree_id: Option<WorktreeId>,
    revision: Option<AgentInspectionRevision>,
    next_revision: u64,
    patch: Option<AgentPatchInspection>,
    processes: VecDeque<AgentProcessInspection>,
    retained_process_bytes: usize,
}

impl AgentInspectionBufferState {
    fn ensure_active_project(
        &self,
        project: &ProjectIdentity,
    ) -> Result<(), AgentInspectionSinkFailure> {
        if self.active_worktree_id == Some(project.worktree().id()) {
            Ok(())
        } else {
            Err(AgentInspectionSinkFailure::InactiveProject)
        }
    }

    fn ensure_project(&self, project: &ProjectIdentity) -> Result<(), AgentInspectionQueryError> {
        if self.active_worktree_id == Some(project.worktree().id()) {
            Ok(())
        } else {
            Err(AgentInspectionQueryError::Unavailable)
        }
    }

    fn retain_compatible_context(&mut self, context: AgentInspectionContext) {
        let incompatible_patch = self.patch.as_ref().is_some_and(|patch| {
            patch.context().task_id() != context.task_id()
                || patch.context().run_id() != context.run_id()
        });
        let incompatible_process = self.processes.front().is_some_and(|process| {
            process.context().task_id() != context.task_id()
                || process.context().run_id() != context.run_id()
        });
        if incompatible_patch || incompatible_process {
            self.clear_records();
        }
    }

    fn advance_revision(&mut self) -> Result<(), AgentInspectionSinkFailure> {
        let next = self
            .next_revision
            .checked_add(1)
            .ok_or(AgentInspectionSinkFailure::RevisionExhausted)?;
        self.next_revision = next;
        self.revision = Some(AgentInspectionRevision(next));
        Ok(())
    }

    fn clear_records(&mut self) {
        self.revision = None;
        self.patch = None;
        self.processes.clear();
        self.retained_process_bytes = 0;
    }
}

/// Volatile sink could not retain presentation data without weakening its boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInspectionSinkFailure {
    /// The composition root has not activated the action's worktree.
    InactiveProject,
    /// Exact preview anchors or bounded data were invalid.
    InvalidInspection(AgentInspectionBuildError),
    /// The bounded process-output capacity could not accept the record.
    CapacityExceeded,
    /// The process-local revision counter exhausted u64.
    RevisionExhausted,
}

impl fmt::Display for AgentInspectionSinkFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InactiveProject => "Agent inspection project is inactive",
            Self::InvalidInspection(_) => "Agent inspection data is invalid",
            Self::CapacityExceeded => "Agent inspection capacity is exhausted",
            Self::RevisionExhausted => "Agent inspection revision is exhausted",
        })
    }
}

impl Error for AgentInspectionSinkFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidInspection(error) => Some(error),
            Self::InactiveProject | Self::CapacityExceeded | Self::RevisionExhausted => None,
        }
    }
}

/// A WebView-originated volatile detail selection no longer resolves exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInspectionQueryError {
    /// No inspection is active for the selected project.
    Unavailable,
    /// A newer volatile observation superseded the overview.
    RevisionChanged,
    /// The record is absent or belongs to another task.
    RecordUnavailable,
    /// The byte cursor did not match a safe UTF-8 boundary in the selected stream.
    InvalidCursor,
}

impl fmt::Display for AgentInspectionQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Agent inspection is unavailable",
            Self::RevisionChanged => "Agent inspection changed",
            Self::RecordUnavailable => "Agent inspection record is unavailable",
            Self::InvalidCursor => "Agent inspection cursor is invalid",
        })
    }
}

impl Error for AgentInspectionQueryError {}

#[derive(Clone, PartialEq, Eq)]
enum LineOperation {
    Context(AgentDiffLine),
    Removed(AgentDiffLine),
    Added(AgentDiffLine),
}

fn classify_operation(
    entry: &PatchPreviewEntry,
) -> Result<AgentDiffFileOperation, AgentInspectionBuildError> {
    match (
        entry.source_path(),
        entry.target_path(),
        entry.before(),
        entry.after(),
    ) {
        (None, Some(_), None, Some(_)) => Ok(AgentDiffFileOperation::Add),
        (Some(source), Some(target), Some(_), Some(_)) if source == target => {
            Ok(AgentDiffFileOperation::Update)
        }
        (Some(source), Some(target), Some(_), Some(_)) if source != target => {
            Ok(AgentDiffFileOperation::Move)
        }
        (Some(_), None, Some(_), None) => Ok(AgentDiffFileOperation::Delete),
        _ => Err(AgentInspectionBuildError::InvalidPreview),
    }
}

fn split_exact_lines(value: &str) -> Vec<AgentDiffLine> {
    if value.is_empty() {
        return Vec::new();
    }
    let bytes = value.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let (end, ending, next) = match bytes[cursor] {
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => {
                (cursor, AgentDiffLineEnding::Crlf, cursor + 2)
            }
            b'\r' => (cursor, AgentDiffLineEnding::Cr, cursor + 1),
            b'\n' => (cursor, AgentDiffLineEnding::Lf, cursor + 1),
            _ => {
                cursor += 1;
                continue;
            }
        };
        lines.push(AgentDiffLine {
            text: value[start..end].to_owned(),
            ending,
        });
        start = next;
        cursor = next;
    }
    if start < value.len() {
        lines.push(AgentDiffLine {
            text: value[start..].to_owned(),
            ending: AgentDiffLineEnding::None,
        });
    }
    lines
}

fn diff_lines(
    before: &[AgentDiffLine],
    after: &[AgentDiffLine],
) -> Result<Vec<LineOperation>, AgentInspectionBuildError> {
    let prefix = before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count();
    let maximum_suffix = before.len().min(after.len()).saturating_sub(prefix);
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take(maximum_suffix)
        .take_while(|(left, right)| left == right)
        .count();
    let before_middle = &before[prefix..before.len().saturating_sub(suffix)];
    let after_middle = &after[prefix..after.len().saturating_sub(suffix)];
    let capacity = prefix
        .checked_add(before_middle.len())
        .and_then(|value| value.checked_add(after_middle.len()))
        .and_then(|value| value.checked_add(suffix))
        .ok_or(AgentInspectionBuildError::TooMuchData)?;
    let mut result = Vec::with_capacity(capacity);
    result.extend(before[..prefix].iter().cloned().map(LineOperation::Context));
    let cell_count = before_middle
        .len()
        .checked_add(1)
        .and_then(|rows| {
            after_middle
                .len()
                .checked_add(1)
                .and_then(|columns| rows.checked_mul(columns))
        })
        .ok_or(AgentInspectionBuildError::TooMuchData)?;
    if cell_count <= MAX_LCS_CELLS {
        append_lcs_diff(&mut result, before_middle, after_middle, cell_count);
    } else {
        result.extend(before_middle.iter().cloned().map(LineOperation::Removed));
        result.extend(after_middle.iter().cloned().map(LineOperation::Added));
    }
    result.extend(
        before[before.len().saturating_sub(suffix)..]
            .iter()
            .cloned()
            .map(LineOperation::Context),
    );
    Ok(result)
}

fn append_lcs_diff(
    result: &mut Vec<LineOperation>,
    before: &[AgentDiffLine],
    after: &[AgentDiffLine],
    cell_count: usize,
) {
    let columns = after.len() + 1;
    let mut lengths = vec![0u32; cell_count];
    for before_index in (0..before.len()).rev() {
        for after_index in (0..after.len()).rev() {
            let index = before_index * columns + after_index;
            lengths[index] = if before[before_index] == after[after_index] {
                lengths[(before_index + 1) * columns + after_index + 1].saturating_add(1)
            } else {
                lengths[(before_index + 1) * columns + after_index]
                    .max(lengths[before_index * columns + after_index + 1])
            };
        }
    }
    let mut before_index = 0usize;
    let mut after_index = 0usize;
    while before_index < before.len() && after_index < after.len() {
        if before[before_index] == after[after_index] {
            result.push(LineOperation::Context(before[before_index].clone()));
            before_index += 1;
            after_index += 1;
        } else if lengths[(before_index + 1) * columns + after_index]
            >= lengths[before_index * columns + after_index + 1]
        {
            result.push(LineOperation::Removed(before[before_index].clone()));
            before_index += 1;
        } else {
            result.push(LineOperation::Added(after[after_index].clone()));
            after_index += 1;
        }
    }
    result.extend(
        before[before_index..]
            .iter()
            .cloned()
            .map(LineOperation::Removed),
    );
    result.extend(
        after[after_index..]
            .iter()
            .cloned()
            .map(LineOperation::Added),
    );
}

fn build_hunks(
    operations: &[LineOperation],
) -> Result<Vec<AgentDiffHunk>, AgentInspectionBuildError> {
    let mut ranges = Vec::<(usize, usize)>::new();
    for index in operations
        .iter()
        .enumerate()
        .filter_map(|(index, operation)| {
            (!matches!(operation, LineOperation::Context(_))).then_some(index)
        })
    {
        let start = index.saturating_sub(DIFF_CONTEXT_LINES);
        let end = index
            .checked_add(DIFF_CONTEXT_LINES + 1)
            .map_or(operations.len(), |end| end.min(operations.len()));
        if let Some(previous) = ranges.last_mut()
            && start <= previous.1
        {
            previous.1 = previous.1.max(end);
        } else {
            ranges.push((start, end));
        }
    }
    ranges
        .into_iter()
        .map(|(start, end)| build_hunk(operations, start, end))
        .collect()
}

fn build_hunk(
    operations: &[LineOperation],
    start: usize,
    end: usize,
) -> Result<AgentDiffHunk, AgentInspectionBuildError> {
    let mut before_line = checked_u32(
        operations[..start]
            .iter()
            .filter(|operation| !matches!(operation, LineOperation::Added(_)))
            .count()
            .saturating_add(1),
    )?;
    let mut after_line = checked_u32(
        operations[..start]
            .iter()
            .filter(|operation| !matches!(operation, LineOperation::Removed(_)))
            .count()
            .saturating_add(1),
    )?;
    let initial_before_line = before_line;
    let initial_after_line = after_line;
    let mut rows = Vec::with_capacity(end.saturating_sub(start));
    for operation in &operations[start..end] {
        match operation {
            LineOperation::Context(line) => {
                rows.push(AgentDiffRow::Context {
                    before_line,
                    after_line,
                    line: line.clone(),
                });
                before_line = before_line
                    .checked_add(1)
                    .ok_or(AgentInspectionBuildError::TooMuchData)?;
                after_line = after_line
                    .checked_add(1)
                    .ok_or(AgentInspectionBuildError::TooMuchData)?;
            }
            LineOperation::Removed(line) => {
                rows.push(AgentDiffRow::Removed {
                    before_line,
                    line: line.clone(),
                });
                before_line = before_line
                    .checked_add(1)
                    .ok_or(AgentInspectionBuildError::TooMuchData)?;
            }
            LineOperation::Added(line) => {
                rows.push(AgentDiffRow::Added {
                    after_line,
                    line: line.clone(),
                });
                after_line = after_line
                    .checked_add(1)
                    .ok_or(AgentInspectionBuildError::TooMuchData)?;
            }
        }
    }
    Ok(AgentDiffHunk {
        before_start: initial_before_line,
        before_count: before_line.saturating_sub(initial_before_line),
        after_start: initial_after_line,
        after_count: after_line.saturating_sub(initial_after_line),
        rows,
    })
}

fn checked_u32(value: usize) -> Result<u32, AgentInspectionBuildError> {
    u32::try_from(value).map_err(|_| AgentInspectionBuildError::TooMuchData)
}

fn derive_patch_inspection_id(
    context: AgentInspectionContext,
    preview: &PatchPreview,
) -> AgentInspectionId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.agent-patch-inspection.v1");
    hash_inspection_context(&mut hasher, context);
    hasher.update(&preview.action_digest().as_bytes());
    AgentInspectionId(*hasher.finalize().as_bytes())
}

fn derive_process_inspection_id(
    context: AgentInspectionContext,
    tool_run_id: ToolRunId,
    result: &ProcessRunResult,
) -> AgentInspectionId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.agent-process-inspection.v1");
    hash_inspection_context(&mut hasher, context);
    hasher.update(tool_run_id.as_bytes());
    hasher.update(result.specification_id().as_bytes());
    hasher.update(result.policy_decision_id().as_bytes());
    hasher.update(&result.stdout().digest().as_bytes());
    hasher.update(&result.stderr().digest().as_bytes());
    AgentInspectionId(*hasher.finalize().as_bytes())
}

fn hash_inspection_context(hasher: &mut blake3::Hasher, context: AgentInspectionContext) {
    hasher.update(context.task_id().as_bytes());
    hasher.update(context.run_id().as_bytes());
    hasher.update(context.step_id().as_bytes());
    hasher.update(context.verification_spec_id().as_bytes());
    hasher.update(context.snapshot_id().as_bytes());
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        CanonicalDirectory, FileRevision, GitHead, GitReferenceName, PatchAction,
        PatchActionSchemaVersion, PatchFileContent, PatchOperation, PatchRationale, PatchUpdate,
        PolicyDecisionId, PolicyResourceId, ProcessExit, ProcessOutputCapture,
        ProcessOutputContent, RepositoryId, RepositoryIdentity, WorktreeAnchorId, WorktreeIdentity,
    };

    #[test]
    fn ipc_selection_reconstructs_only_positive_revisions() {
        assert!(AgentInspectionRevision::new(0).is_err());
        assert_eq!(
            AgentInspectionRevision::new(7).map(|value| value.get()),
            Ok(7)
        );
        assert_eq!(AgentInspectionId::from_bytes([9; 32]).as_bytes(), [9; 32]);
    }

    #[test]
    fn patch_preview_builds_separate_hunks_and_exact_line_endings() -> Result<(), Box<dyn Error>> {
        let before = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\neleven\r\ntwelve\r\nthirteen\r\n";
        let after = "one\r\nTWO\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\neleven\r\nTWELVE\r\nthirteen\r\n";
        let (preview, context) = update_preview(before, after)?;

        let inspection = AgentPatchInspection::from_preview(context, &preview)?;

        assert_eq!(inspection.files().len(), 1);
        let file = &inspection.files()[0];
        assert_eq!(file.operation(), AgentDiffFileOperation::Update);
        assert_eq!(file.hunks().len(), 2);
        assert_eq!(file.added_lines(), 2);
        assert_eq!(file.removed_lines(), 2);
        assert_eq!(file.attribution(), AgentChangeAttribution::ProposedAgent);
        assert!(file.hunks()[0].rows().iter().any(|row| matches!(
            row,
            AgentDiffRow::Added { line, .. }
                if line.text() == "TWO" && line.ending() == AgentDiffLineEnding::Crlf
        )));
        Ok(())
    }

    #[test]
    fn large_middle_diff_falls_back_without_losing_exact_prefixes() -> Result<(), Box<dyn Error>> {
        let before = (0..1_100)
            .map(|index| format!("before-{index}\n"))
            .collect::<String>();
        let after = (0..1_100)
            .map(|index| format!("after-{index}\n"))
            .collect::<String>();
        let (preview, context) = update_preview(&before, &after)?;

        let inspection = AgentPatchInspection::from_preview(context, &preview)?;
        let file = &inspection.files()[0];

        assert_eq!(file.hunks().len(), 1);
        assert_eq!(file.removed_lines(), 1_100);
        assert_eq!(file.added_lines(), 1_100);
        assert_eq!(
            file.before().map(AgentDiffContent::text),
            Some(before.as_str())
        );
        assert_eq!(
            file.after().map(AgentDiffContent::text),
            Some(after.as_str())
        );
        Ok(())
    }

    #[test]
    fn buffer_revalidates_task_revision_and_pages_utf8_logs() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let buffer = AgentInspectionBuffer::new();
        buffer.activate_project(&project);
        let context = context();
        let result = process_result("äbcdef", true)?;
        let process_id = buffer.record_process_result(
            &project,
            context,
            ToolRunId::from_bytes([9; 32]),
            AgentProcessInspectionKind::Test,
            &result,
        )?;
        let overview = buffer
            .overview(&project, context.task_id())?
            .ok_or("missing overview")?;
        assert_eq!(overview.processes().len(), 1);
        assert_eq!(
            overview.processes()[0].kind(),
            AgentProcessInspectionKind::Test
        );
        assert!(overview.processes()[0].stdout().source_truncated());

        let first = buffer.load_process_log_page(
            &project,
            context.task_id(),
            overview.revision(),
            process_id,
            ProcessStream::Stdout,
            AgentLogPageOffset::START,
            AgentLogPageLimit::new(3)?,
        )?;
        assert_eq!(first.text(), "äb");
        assert!(first.page_truncated());
        assert!(first.source_truncated());
        let second = buffer.load_process_log_page(
            &project,
            context.task_id(),
            overview.revision(),
            process_id,
            ProcessStream::Stdout,
            first.next_offset().ok_or("missing next page")?,
            AgentLogPageLimit::new(16)?,
        )?;
        assert_eq!(second.text(), "cdef");
        assert!(!second.page_truncated());
        assert!(second.source_truncated());
        assert_eq!(
            buffer.load_process_log_page(
                &project,
                TaskId::from_bytes([99; 32]),
                overview.revision(),
                process_id,
                ProcessStream::Stdout,
                AgentLogPageOffset::START,
                AgentLogPageLimit::DEFAULT,
            ),
            Err(AgentInspectionQueryError::RecordUnavailable)
        );
        Ok(())
    }

    #[test]
    fn redacted_stream_never_returns_text_or_accepts_a_cursor() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let buffer = AgentInspectionBuffer::new();
        buffer.activate_project(&project);
        let context = context();
        let result = redacted_process_result()?;
        let process_id = buffer.record_process_result(
            &project,
            context,
            ToolRunId::from_bytes([10; 32]),
            AgentProcessInspectionKind::Diagnostic,
            &result,
        )?;
        let revision = buffer
            .overview(&project, context.task_id())?
            .ok_or("missing overview")?
            .revision();

        let page = buffer.load_process_log_page(
            &project,
            context.task_id(),
            revision,
            process_id,
            ProcessStream::Stdout,
            AgentLogPageOffset::START,
            AgentLogPageLimit::DEFAULT,
        )?;
        assert_eq!(page.text(), "");
        assert_eq!(
            page.redaction(),
            Some(ProcessOutputRedaction::SecretCandidate)
        );
        assert_eq!(
            buffer.load_process_log_page(
                &project,
                context.task_id(),
                revision,
                process_id,
                ProcessStream::Stdout,
                AgentLogPageOffset::new(1),
                AgentLogPageLimit::DEFAULT,
            ),
            Err(AgentInspectionQueryError::InvalidCursor)
        );
        Ok(())
    }

    #[test]
    fn new_run_and_project_deactivation_clear_volatile_content() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let buffer = AgentInspectionBuffer::new();
        buffer.activate_project(&project);
        let (preview, context) = update_preview("old\n", "new\n")?;
        buffer.record_patch_preview(&project, context, &preview)?;
        let first_revision = buffer
            .overview(&project, context.task_id())?
            .ok_or("missing preview")?
            .revision();
        let other_context = AgentInspectionContext::new(
            context.task_id(),
            AgentRunId::from_bytes([88; 32]),
            context.step_id(),
            context.verification_spec_id(),
            context.snapshot_id(),
        );
        buffer.record_process_result(
            &project,
            other_context,
            ToolRunId::from_bytes([11; 32]),
            AgentProcessInspectionKind::Build,
            &process_result("built\n", false)?,
        )?;
        let overview = buffer
            .overview(&project, context.task_id())?
            .ok_or("missing process")?;
        assert!(overview.patch().is_none());
        assert_ne!(overview.revision(), first_revision);

        buffer.deactivate_project();
        assert_eq!(
            buffer.overview(&project, context.task_id()),
            Err(AgentInspectionQueryError::Unavailable)
        );
        Ok(())
    }

    fn update_preview(
        before: &str,
        after: &str,
    ) -> Result<(PatchPreview, AgentInspectionContext), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let before_content = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
        let after_content = PatchFileContent::try_from_bytes(after.as_bytes().to_vec())?;
        let context = context();
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            context.run_id(),
            WorktreeId::from_bytes([6; 32]),
            context.snapshot_id(),
            context.step_id(),
            context.verification_spec_id(),
            PatchRationale::try_from_string("show exact patch".to_owned())?,
            vec![PatchOperation::Update(PatchUpdate::new(
                FileRevision::new(path.clone(), before_content.content_hash()),
                after_content.clone(),
            )?)],
        )?;
        let entry = PatchPreviewEntry::new(
            Some(path.clone()),
            Some(path),
            Some(PatchContentPreview::from_content(
                &before_content,
                16 * 1_024,
            )?),
            Some(PatchContentPreview::from_content(
                &after_content,
                16 * 1_024,
            )?),
        );
        Ok((PatchPreview::new(&action, vec![entry])?, context))
    }

    const fn context() -> AgentInspectionContext {
        AgentInspectionContext::new(
            TaskId::from_bytes([1; 32]),
            AgentRunId::from_bytes([2; 32]),
            TaskStepId::from_bytes([3; 32]),
            VerificationSpecId::from_bytes([4; 32]),
            SnapshotId::from_bytes([5; 32]),
        )
    }

    fn process_result(text: &str, truncated: bool) -> Result<ProcessRunResult, Box<dyn Error>> {
        let observed_bytes = u64::try_from(text.len())? + u64::from(truncated) * 10;
        let retained_limit = u32::try_from(text.len().max(1))?;
        Ok(ProcessRunResult::new(
            PolicyResourceId::from_bytes([20; 32]),
            PolicyDecisionId::from_bytes([21; 32]),
            ProcessTermination::Exited(ProcessExit::new(Some(0), true)?),
            ProcessDuration::from_millis(25),
            ProcessOutputCapture::new(
                ProcessStream::Stdout,
                ProcessOutputContent::text(text.to_owned())?,
                observed_bytes,
                retained_limit,
                truncated,
                ProcessOutputDigest::from_bytes([22; 32]),
            )?,
            ProcessOutputCapture::new(
                ProcessStream::Stderr,
                ProcessOutputContent::text(String::new())?,
                0,
                1,
                false,
                ProcessOutputDigest::from_bytes([23; 32]),
            )?,
        )?)
    }

    fn redacted_process_result() -> Result<ProcessRunResult, Box<dyn Error>> {
        Ok(ProcessRunResult::new(
            PolicyResourceId::from_bytes([30; 32]),
            PolicyDecisionId::from_bytes([31; 32]),
            ProcessTermination::Exited(ProcessExit::new(Some(1), false)?),
            ProcessDuration::from_millis(10),
            ProcessOutputCapture::new(
                ProcessStream::Stdout,
                ProcessOutputContent::redacted(ProcessOutputRedaction::SecretCandidate),
                32,
                64,
                false,
                ProcessOutputDigest::from_bytes([32; 32]),
            )?,
            ProcessOutputCapture::new(
                ProcessStream::Stderr,
                ProcessOutputContent::text(String::new())?,
                0,
                1,
                false,
                ProcessOutputDigest::from_bytes([33; 32]),
            )?,
        )?)
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([40; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([6; 32]),
                WorktreeAnchorId::from_bytes([41; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
