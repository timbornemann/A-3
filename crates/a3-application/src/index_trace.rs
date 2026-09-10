use a3_domain::{
    IndexTraceDiagnosticCode, IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId,
    IndexTraceParseOutcome, IndexTracePhase, IndexTracePhaseState, IndexTraceRevision,
    IndexTraceState, IndexTraceTimestamp, IndexTraceTrigger, ProjectIdentity, RepositoryPath,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Maximum number of file rows returned by one inspector request.
pub const INDEX_TRACE_FILE_PAGE_LIMIT: u16 = 100;
/// Maximum UTF-8 byte length of a server-side trace file search.
pub const INDEX_TRACE_SEARCH_BYTES: usize = 256;
/// Maximum number of recent events retained for one trace summary.
pub const INDEX_TRACE_EVENT_LIMIT: usize = 128;
/// Maximum safe parser diagnostics retained for one file.
pub const INDEX_TRACE_FILE_DIAGNOSTIC_LIMIT: usize = 8;

/// One fixed phase row in a Fast-Index trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTracePhaseProgress {
    phase: IndexTracePhase,
    state: IndexTracePhaseState,
    started_at: Option<IndexTraceTimestamp>,
    ended_at: Option<IndexTraceTimestamp>,
    completed: Option<u64>,
    total: Option<u64>,
}

impl IndexTracePhaseProgress {
    /// Creates a validated phase projection.
    pub fn new(
        phase: IndexTracePhase,
        state: IndexTracePhaseState,
        started_at: Option<IndexTraceTimestamp>,
        ended_at: Option<IndexTraceTimestamp>,
        completed: Option<u64>,
        total: Option<u64>,
    ) -> Result<Self, IndexTraceDataError> {
        if completed.is_some() != total.is_some()
            || completed
                .zip(total)
                .is_some_and(|(done, total)| done > total)
            || ended_at
                .zip(started_at)
                .is_some_and(|(end, start)| end < start)
        {
            return Err(IndexTraceDataError::InvalidProgress);
        }
        Ok(Self {
            phase,
            state,
            started_at,
            ended_at,
            completed,
            total,
        })
    }

    /// Creates the six pending rows for a new trace.
    #[must_use]
    pub fn pending_all() -> Vec<Self> {
        IndexTracePhase::ALL
            .into_iter()
            .map(|phase| Self {
                phase,
                state: IndexTracePhaseState::Pending,
                started_at: None,
                ended_at: None,
                completed: None,
                total: None,
            })
            .collect()
    }

    /// Returns the phase represented by this row.
    #[must_use]
    pub const fn phase(&self) -> IndexTracePhase {
        self.phase
    }

    /// Returns the phase state.
    #[must_use]
    pub const fn state(&self) -> IndexTracePhaseState {
        self.state
    }

    /// Returns when the phase started, if it has started.
    #[must_use]
    pub const fn started_at(&self) -> Option<IndexTraceTimestamp> {
        self.started_at
    }

    /// Returns when the phase ended, if it is terminal.
    #[must_use]
    pub const fn ended_at(&self) -> Option<IndexTraceTimestamp> {
        self.ended_at
    }

    /// Returns the completed sub-operation count when determinate.
    #[must_use]
    pub const fn completed(&self) -> Option<u64> {
        self.completed
    }

    /// Returns the total sub-operation count when determinate.
    #[must_use]
    pub const fn total(&self) -> Option<u64> {
        self.total
    }
}

/// Exact aggregate counters derived from the complete retained file set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexTraceCounts {
    /// Allowed files observed by discovery, excluding deleted rows.
    pub discovered: u64,
    /// Discovered files not yet classified against the previous snapshot.
    pub pending: u64,
    /// Files absent from the previous snapshot.
    pub new_files: u64,
    /// Files whose content changed.
    pub changed: u64,
    /// Files reused without a content change.
    pub unchanged: u64,
    /// Files removed since the previous snapshot.
    pub deleted: u64,
    /// Files whose content was freshly hashed.
    pub hashed: u64,
    /// Files whose previous hash was reused.
    pub hash_reused: u64,
    /// Files processed by a structural parser.
    pub structural: u64,
    /// Files whose previous structural result was reused.
    pub parse_reused: u64,
    /// Files handled by the generic text fallback.
    pub generic: u64,
    /// Files with at least one safe processing failure.
    pub failed: u64,
}

/// Materialized bounded summary for one retained trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceRunSummary {
    id: IndexTraceId,
    revision: IndexTraceRevision,
    state: IndexTraceState,
    trigger: IndexTraceTrigger,
    started_at: IndexTraceTimestamp,
    ended_at: Option<IndexTraceTimestamp>,
    last_activity_at: IndexTraceTimestamp,
    current_phase: Option<IndexTracePhase>,
    current_file: Option<RepositoryPath>,
    diagnostic: Option<IndexTraceDiagnosticCode>,
    details_incomplete: bool,
    previous_publication_available: bool,
    counts: IndexTraceCounts,
}

impl IndexTraceRunSummary {
    /// Creates the queued first revision before repository discovery begins.
    #[must_use]
    pub const fn queued(
        id: IndexTraceId,
        trigger: IndexTraceTrigger,
        at: IndexTraceTimestamp,
        previous_publication_available: bool,
    ) -> Self {
        Self {
            id,
            revision: IndexTraceRevision::FIRST,
            state: IndexTraceState::Queued,
            trigger,
            started_at: at,
            ended_at: None,
            last_activity_at: at,
            current_phase: Some(IndexTracePhase::Discover),
            current_file: None,
            diagnostic: None,
            details_incomplete: false,
            previous_publication_available,
            counts: IndexTraceCounts {
                discovered: 0,
                pending: 0,
                new_files: 0,
                changed: 0,
                unchanged: 0,
                deleted: 0,
                hashed: 0,
                hash_reused: 0,
                structural: 0,
                parse_reused: 0,
                generic: 0,
                failed: 0,
            },
        }
    }

    /// Reconstructs a validated durable summary.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        id: IndexTraceId,
        revision: IndexTraceRevision,
        state: IndexTraceState,
        trigger: IndexTraceTrigger,
        started_at: IndexTraceTimestamp,
        ended_at: Option<IndexTraceTimestamp>,
        last_activity_at: IndexTraceTimestamp,
        current_phase: Option<IndexTracePhase>,
        current_file: Option<RepositoryPath>,
        diagnostic: Option<IndexTraceDiagnosticCode>,
        details_incomplete: bool,
        previous_publication_available: bool,
        counts: IndexTraceCounts,
    ) -> Result<Self, IndexTraceDataError> {
        if last_activity_at < started_at
            || ended_at.is_some_and(|end| end < started_at || end < last_activity_at)
            || state.is_terminal() != ended_at.is_some()
            || !counts_are_valid(counts)
        {
            return Err(IndexTraceDataError::InvalidTiming);
        }
        Ok(Self {
            id,
            revision,
            state,
            trigger,
            started_at,
            ended_at,
            last_activity_at,
            current_phase,
            current_file,
            diagnostic,
            details_incomplete,
            previous_publication_available,
            counts,
        })
    }

    /// Returns the stable trace identifier.
    #[must_use]
    pub const fn id(&self) -> IndexTraceId {
        self.id
    }
    /// Returns the current monotone trace revision.
    #[must_use]
    pub const fn revision(&self) -> IndexTraceRevision {
        self.revision
    }
    /// Returns the lifecycle state.
    #[must_use]
    pub const fn state(&self) -> IndexTraceState {
        self.state
    }
    /// Returns why the trace was started.
    #[must_use]
    pub const fn trigger(&self) -> IndexTraceTrigger {
        self.trigger
    }
    /// Returns the timestamp captured before discovery.
    #[must_use]
    pub const fn started_at(&self) -> IndexTraceTimestamp {
        self.started_at
    }
    /// Returns the terminal timestamp, if any.
    #[must_use]
    pub const fn ended_at(&self) -> Option<IndexTraceTimestamp> {
        self.ended_at
    }
    /// Returns the timestamp of the latest observed progress.
    #[must_use]
    pub const fn last_activity_at(&self) -> IndexTraceTimestamp {
        self.last_activity_at
    }
    /// Returns the active or last phase.
    #[must_use]
    pub const fn current_phase(&self) -> Option<IndexTracePhase> {
        self.current_phase
    }
    /// Returns the currently processed repository-relative path.
    #[must_use]
    pub const fn current_file(&self) -> Option<&RepositoryPath> {
        self.current_file.as_ref()
    }
    /// Returns the safe terminal diagnostic, if one exists.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<IndexTraceDiagnosticCode> {
        self.diagnostic
    }
    /// Returns whether some journal detail could not be persisted.
    #[must_use]
    pub const fn details_incomplete(&self) -> bool {
        self.details_incomplete
    }
    /// Returns whether a previously published index remains usable.
    #[must_use]
    pub const fn previous_publication_available(&self) -> bool {
        self.previous_publication_available
    }
    /// Returns the complete derived counters.
    #[must_use]
    pub const fn counts(&self) -> IndexTraceCounts {
        self.counts
    }
}

/// One safe retained event; it never contains source text or raw errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceEvent {
    revision: IndexTraceRevision,
    occurred_at: IndexTraceTimestamp,
    kind: IndexTraceEventKind,
    phase: Option<IndexTracePhase>,
    path: Option<RepositoryPath>,
    diagnostic: Option<IndexTraceDiagnosticCode>,
}

impl IndexTraceEvent {
    /// Creates one event containing only closed codes and safe paths.
    #[must_use]
    pub const fn new(
        revision: IndexTraceRevision,
        occurred_at: IndexTraceTimestamp,
        kind: IndexTraceEventKind,
        phase: Option<IndexTracePhase>,
        path: Option<RepositoryPath>,
        diagnostic: Option<IndexTraceDiagnosticCode>,
    ) -> Self {
        Self {
            revision,
            occurred_at,
            kind,
            phase,
            path,
            diagnostic,
        }
    }

    /// Returns the revision that introduced this event.
    #[must_use]
    pub const fn revision(&self) -> IndexTraceRevision {
        self.revision
    }
    /// Returns when the event occurred.
    #[must_use]
    pub const fn occurred_at(&self) -> IndexTraceTimestamp {
        self.occurred_at
    }
    /// Returns the closed event kind.
    #[must_use]
    pub const fn kind(&self) -> IndexTraceEventKind {
        self.kind
    }
    /// Returns the associated phase, if any.
    #[must_use]
    pub const fn phase(&self) -> Option<IndexTracePhase> {
        self.phase
    }
    /// Returns the associated safe repository-relative path, if any.
    #[must_use]
    pub const fn path(&self) -> Option<&RepositoryPath> {
        self.path.as_ref()
    }
    /// Returns the associated stable diagnostic code, if any.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<IndexTraceDiagnosticCode> {
        self.diagnostic
    }
}

/// Closed event vocabulary displayed by the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTraceEventKind {
    /// The trace was registered before work started.
    Queued,
    /// The scheduler started executing the trace.
    Started,
    /// One fixed phase began.
    PhaseStarted,
    /// Determinate or indeterminate work advanced.
    Progress,
    /// One allowed file was observed.
    FileObserved,
    /// One safe diagnostic was recorded.
    Diagnostic,
    /// Cooperative cancellation was requested.
    CancellationRequested,
    /// The index was published successfully.
    Succeeded,
    /// The run ended with a classified failure.
    Failed,
    /// The run honored cooperative cancellation.
    Cancelled,
    /// A prior process ended before the run became terminal.
    Interrupted,
}

/// Complete status for one allowed repository path in a trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceFileRecord {
    path: RepositoryPath,
    change: IndexTraceFileChange,
    hash: IndexTraceHashOutcome,
    parse: Option<IndexTraceParseOutcome>,
    diagnostics: Vec<IndexTraceDiagnosticCode>,
    diagnostics_truncated: bool,
}

impl IndexTraceFileRecord {
    /// Creates and validates one complete file result.
    pub fn new(
        path: RepositoryPath,
        change: IndexTraceFileChange,
        hash: IndexTraceHashOutcome,
        parse: Option<IndexTraceParseOutcome>,
        mut diagnostics: Vec<IndexTraceDiagnosticCode>,
        diagnostics_truncated: bool,
    ) -> Result<Self, IndexTraceDataError> {
        if diagnostics.len() > INDEX_TRACE_FILE_DIAGNOSTIC_LIMIT {
            return Err(IndexTraceDataError::TooManyDiagnostics);
        }
        diagnostics.dedup();
        let valid_outcomes = match change {
            IndexTraceFileChange::Pending => {
                hash == IndexTraceHashOutcome::Pending && parse.is_none()
            }
            IndexTraceFileChange::Deleted => {
                hash == IndexTraceHashOutcome::NotApplicable
                    && parse == Some(IndexTraceParseOutcome::NotApplicable)
            }
            IndexTraceFileChange::New
            | IndexTraceFileChange::Changed
            | IndexTraceFileChange::Unchanged => {
                matches!(
                    hash,
                    IndexTraceHashOutcome::Hashed | IndexTraceHashOutcome::Reused
                ) && parse != Some(IndexTraceParseOutcome::NotApplicable)
            }
        };
        if !valid_outcomes {
            return Err(IndexTraceDataError::InvalidFileOutcome);
        }
        Ok(Self {
            path,
            change,
            hash,
            parse,
            diagnostics,
            diagnostics_truncated,
        })
    }

    /// Returns the repository-relative file path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }
    /// Returns how the file changed relative to the prior snapshot.
    #[must_use]
    pub const fn change(&self) -> IndexTraceFileChange {
        self.change
    }
    /// Returns whether hashing was performed or reused.
    #[must_use]
    pub const fn hash(&self) -> IndexTraceHashOutcome {
        self.hash
    }
    /// Returns the parse outcome once parsing was applicable.
    #[must_use]
    pub const fn parse(&self) -> Option<IndexTraceParseOutcome> {
        self.parse
    }
    /// Returns all retained safe diagnostic codes for this file.
    #[must_use]
    pub fn diagnostics(&self) -> &[IndexTraceDiagnosticCode] {
        &self.diagnostics
    }
    /// Returns whether additional diagnostics were discarded by the bound.
    #[must_use]
    pub const fn diagnostics_truncated(&self) -> bool {
        self.diagnostics_truncated
    }
}

/// Summary, six phases, bounded events, and optional complete files loaded from storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceSnapshot {
    summary: IndexTraceRunSummary,
    phases: Vec<IndexTracePhaseProgress>,
    events: Vec<IndexTraceEvent>,
    files: Vec<IndexTraceFileRecord>,
}

impl IndexTraceSnapshot {
    /// Creates a validated snapshot with exactly six ordered phases.
    pub fn new(
        summary: IndexTraceRunSummary,
        phases: Vec<IndexTracePhaseProgress>,
        events: Vec<IndexTraceEvent>,
        mut files: Vec<IndexTraceFileRecord>,
    ) -> Result<Self, IndexTraceDataError> {
        if phases.len() != IndexTracePhase::ALL.len()
            || phases
                .iter()
                .zip(IndexTracePhase::ALL)
                .any(|(row, phase)| row.phase() != phase)
            || events.len() > INDEX_TRACE_EVENT_LIMIT
        {
            return Err(IndexTraceDataError::InvalidProjection);
        }
        files.sort_by(|left, right| left.path().cmp(right.path()));
        if files
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(IndexTraceDataError::InvalidProjection);
        }
        Ok(Self {
            summary,
            phases,
            events,
            files,
        })
    }

    /// Returns the run summary.
    #[must_use]
    pub const fn summary(&self) -> &IndexTraceRunSummary {
        &self.summary
    }
    /// Returns the six ordered phase rows.
    #[must_use]
    pub fn phases(&self) -> &[IndexTracePhaseProgress] {
        &self.phases
    }
    /// Returns the bounded recent events.
    #[must_use]
    pub fn events(&self) -> &[IndexTraceEvent] {
        &self.events
    }
    /// Returns all known allowed and deleted file rows.
    #[must_use]
    pub fn files(&self) -> &[IndexTraceFileRecord] {
        &self.files
    }
}

/// The only two trace snapshots retained for an active project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetainedIndexTraces {
    /// Active trace or most recently terminal trace.
    pub current: Option<IndexTraceSnapshot>,
    /// Immediately previous terminal trace while the current trace is active.
    pub previous: Option<IndexTraceSnapshot>,
}

/// Validated server-side file query without any filesystem capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceFileQuery {
    /// Retained trace to query.
    pub trace_id: IndexTraceId,
    /// Exact trace revision that the caller observed.
    pub revision: IndexTraceRevision,
    /// Optional bounded case-insensitive path fragment.
    pub search: Option<String>,
    /// Exact result category filter.
    pub filter: IndexTraceFileFilter,
    /// Server-validated row offset decoded from an opaque cursor.
    pub offset: u64,
}

impl IndexTraceFileQuery {
    /// Creates a bounded query and rejects control characters or oversized search text.
    pub fn new(
        trace_id: IndexTraceId,
        revision: IndexTraceRevision,
        search: Option<String>,
        filter: IndexTraceFileFilter,
        offset: u64,
    ) -> Result<Self, IndexTraceDataError> {
        if search.as_ref().is_some_and(|value| {
            value.len() > INDEX_TRACE_SEARCH_BYTES || value.chars().any(char::is_control)
        }) {
            return Err(IndexTraceDataError::InvalidSearch);
        }
        Ok(Self {
            trace_id,
            revision,
            search,
            filter,
            offset,
        })
    }
}

/// Closed file-result filter supported by the inspector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IndexTraceFileFilter {
    /// All files.
    #[default]
    All,
    /// Newly discovered files.
    New,
    /// Changed files.
    Changed,
    /// Unchanged files.
    Unchanged,
    /// Deleted files.
    Deleted,
    /// Freshly hashed files.
    Hashed,
    /// Files with reused hashes.
    HashReused,
    /// Structurally parsed files.
    Structural,
    /// Files with reused parse output.
    ParseReused,
    /// Generically indexed files.
    Generic,
    /// Files carrying a safe failure code.
    Failed,
}

/// One fixed-size page of file results plus navigation metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceFilePage {
    /// At most [`INDEX_TRACE_FILE_PAGE_LIMIT`] ordered file rows.
    pub files: Vec<IndexTraceFileRecord>,
    /// Exact number of rows matching the query.
    pub total: u64,
    /// Offset of the first returned row.
    pub offset: u64,
    /// Whether a later page exists.
    pub has_more: bool,
}

/// One bounded atomic journal update. File rows are upserted, not rewritten wholesale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTraceCheckpoint {
    /// Last revision known to durable storage.
    pub expected_revision: IndexTraceRevision,
    /// Complete current summary projection.
    pub snapshot: IndexTraceSnapshot,
    /// Bounded file rows changed since the prior checkpoint.
    pub file_updates: Vec<IndexTraceFileRecord>,
    /// Bounded events appended since the prior checkpoint.
    pub new_events: Vec<IndexTraceEvent>,
}

/// Owned future returned by the trace persistence port.
pub type IndexTraceStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, IndexTraceStoreFailure>> + Send + 'a>>;

/// Dedicated durable trace boundary, separate from atomic index publication.
pub trait IndexTraceStore: fmt::Debug + Send + Sync {
    /// Marks abandoned active traces and building index runs as interrupted or failed.
    fn reconcile_interrupted<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        at: IndexTraceTimestamp,
    ) -> IndexTraceStoreFuture<'a, ()>;

    /// Persists a trace before repository discovery starts.
    fn create_trace<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot: &'a IndexTraceSnapshot,
    ) -> IndexTraceStoreFuture<'a, ()>;

    /// Applies one optimistic bounded journal checkpoint.
    fn checkpoint_trace<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        checkpoint: &'a IndexTraceCheckpoint,
    ) -> IndexTraceStoreFuture<'a, ()>;

    /// Loads the current and optional immediately previous retained trace.
    fn load_retained<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> IndexTraceStoreFuture<'a, RetainedIndexTraces>;

    /// Searches and pages durable file rows without exposing storage capability.
    fn query_files<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a IndexTraceFileQuery,
    ) -> IndexTraceStoreFuture<'a, IndexTraceFilePage>;
}

/// Stable failure categories returned by the trace storage boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTraceStoreFailure {
    /// Storage could not complete the operation.
    Unavailable,
    /// A retained-trace uniqueness invariant was violated.
    Conflict,
    /// The requested trace is no longer retained.
    NotFound,
    /// The optimistic revision no longer matches.
    StaleRevision,
    /// Durable values violated the domain projection contract.
    InvalidStoredData,
}

impl fmt::Display for IndexTraceStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Fast-Index trace storage is unavailable")
    }
}

impl Error for IndexTraceStoreFailure {}

/// Validation failures for application trace projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTraceDataError {
    /// Phase progress or phase timing is inconsistent.
    InvalidProgress,
    /// Run lifecycle timestamps are inconsistent.
    InvalidTiming,
    /// File change, hash, and parse outcomes conflict.
    InvalidFileOutcome,
    /// A file carries more safe diagnostics than allowed.
    TooManyDiagnostics,
    /// A complete snapshot violates ordering or cardinality rules.
    InvalidProjection,
    /// A search term exceeds its bound or contains controls.
    InvalidSearch,
}

impl fmt::Display for IndexTraceDataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Fast-Index trace data is invalid")
    }
}

impl Error for IndexTraceDataError {}

fn counts_are_valid(counts: IndexTraceCounts) -> bool {
    counts
        .pending
        .checked_add(counts.new_files)
        .and_then(|value| value.checked_add(counts.changed))
        .and_then(|value| value.checked_add(counts.unchanged))
        == Some(counts.discovered)
        && counts
            .pending
            .checked_add(counts.hashed)
            .and_then(|value| value.checked_add(counts.hash_reused))
            == Some(counts.discovered)
        && counts
            .structural
            .checked_add(counts.parse_reused)
            .and_then(|value| value.checked_add(counts.generic))
            .and_then(|value| value.checked_add(counts.failed))
            .is_some_and(|value| value <= counts.discovered)
}

#[cfg(test)]
mod tests {
    use super::{IndexTraceFileFilter, IndexTraceFileQuery, IndexTracePhaseProgress};
    use a3_domain::{IndexTraceId, IndexTracePhase, IndexTraceRevision};

    #[test]
    fn phases_and_search_are_strictly_bounded() {
        assert_eq!(IndexTracePhaseProgress::pending_all().len(), 6);
        assert!(
            IndexTraceFileQuery::new(
                IndexTraceId::from_bytes([1; 32]),
                IndexTraceRevision::FIRST,
                Some("x".repeat(257)),
                IndexTraceFileFilter::All,
                0,
            )
            .is_err()
        );
        assert_eq!(IndexTracePhase::ALL.len(), 6);
    }
}
