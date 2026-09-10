use a3_application::{
    INDEX_TRACE_EVENT_LIMIT, INDEX_TRACE_FILE_PAGE_LIMIT, IndexTraceCheckpoint, IndexTraceCounts,
    IndexTraceEvent, IndexTraceEventKind, IndexTraceFileFilter, IndexTraceFilePage,
    IndexTraceFileQuery, IndexTraceFileRecord, IndexTracePhaseProgress, IndexTraceRunSummary,
    IndexTraceSnapshot, IndexTraceStoreFailure, RepositoryIndexObservation, RepositoryIndexPhase,
    RetainedIndexTraces,
};
use a3_domain::{
    IndexTraceDiagnosticCode, IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId,
    IndexTraceParseOutcome, IndexTracePhase, IndexTracePhaseState, IndexTraceRevision,
    IndexTraceState, IndexTraceTimestamp, IndexTraceTrigger, Progress, ProjectIdentity,
    RepositoryPath, WorktreeId,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
pub(crate) struct IndexTraceRuntime {
    worktree_id: Option<WorktreeId>,
    current: Option<TraceProjection>,
    previous: Option<IndexTraceSnapshot>,
}

impl IndexTraceRuntime {
    pub(crate) fn replace_loaded(
        &mut self,
        project: &ProjectIdentity,
        retained: RetainedIndexTraces,
    ) {
        self.worktree_id = Some(project.worktree().id());
        self.current = retained.current.and_then(TraceProjection::from_snapshot);
        self.previous = retained.previous;
    }

    pub(crate) fn begin(
        &mut self,
        project: &ProjectIdentity,
        job_id: a3_domain::JobId,
        trigger: IndexTraceTrigger,
        previous_publication_available: bool,
    ) -> Option<IndexTraceSnapshot> {
        let worktree_id = project.worktree().id();
        if self.worktree_id != Some(worktree_id) {
            self.current = None;
            self.previous = None;
            self.worktree_id = Some(worktree_id);
        }
        let at = now();
        let id = derive_trace_id(project, job_id, at);
        if let Some(current) = self.current.take()
            && let Ok(snapshot) = current.snapshot()
        {
            self.previous = current.summary.state.is_terminal().then_some(snapshot);
        }
        let projection = TraceProjection::queued(id, trigger, at, previous_publication_available);
        let snapshot = projection.snapshot().ok();
        self.current = Some(projection);
        snapshot
    }

    pub(crate) fn mark_created(&mut self) {
        if let Some(current) = self.current.as_mut() {
            current.persisted_revision = Some(IndexTraceRevision::FIRST);
            current.pending_events.clear();
        }
    }

    pub(crate) fn mark_details_incomplete(&mut self) {
        if let Some(current) = self.current.as_mut() {
            current.mark_details_incomplete(now());
        }
    }

    pub(crate) fn observe(&mut self, observation: RepositoryIndexObservation) {
        if let Some(current) = self.current.as_mut() {
            current.observe(observation, now());
        }
    }

    pub(crate) fn observe_for(
        &mut self,
        trace_id: IndexTraceId,
        observation: RepositoryIndexObservation,
    ) {
        if self
            .current
            .as_ref()
            .is_some_and(|trace| trace.summary.id == trace_id)
        {
            self.observe(observation);
        }
    }

    pub(crate) fn set_state(
        &mut self,
        state: IndexTraceState,
        diagnostic: Option<IndexTraceDiagnosticCode>,
    ) {
        if let Some(current) = self.current.as_mut() {
            current.set_state(state, diagnostic, now());
        }
    }

    pub(crate) fn set_state_for(
        &mut self,
        trace_id: IndexTraceId,
        state: IndexTraceState,
        diagnostic: Option<IndexTraceDiagnosticCode>,
    ) {
        if self
            .current
            .as_ref()
            .is_some_and(|trace| trace.summary.id == trace_id)
        {
            self.set_state(state, diagnostic);
        }
    }

    pub(crate) fn retained(&self) -> RetainedIndexTraces {
        let current = self
            .current
            .as_ref()
            .and_then(|trace| trace.snapshot().ok());
        let previous = current
            .as_ref()
            .is_some_and(|trace| !trace.summary().state().is_terminal())
            .then(|| self.previous.clone())
            .flatten();
        RetainedIndexTraces { current, previous }
    }

    pub(crate) fn retained_for(&self, worktree_id: WorktreeId) -> RetainedIndexTraces {
        if self.worktree_id == Some(worktree_id) {
            self.retained()
        } else {
            RetainedIndexTraces::default()
        }
    }

    pub(crate) fn checkpoint(&self) -> Option<IndexTraceCheckpoint> {
        self.current.as_ref().and_then(TraceProjection::checkpoint)
    }

    pub(crate) fn acknowledge(&mut self, checkpoint: &IndexTraceCheckpoint) {
        if let Some(current) = self.current.as_mut()
            && current.summary.id == checkpoint.snapshot.summary().id()
        {
            current.acknowledge(checkpoint);
        }
    }

    pub(crate) fn query_files(
        &self,
        worktree_id: WorktreeId,
        query: &IndexTraceFileQuery,
    ) -> Result<IndexTraceFilePage, IndexTraceStoreFailure> {
        if self.worktree_id != Some(worktree_id) {
            return Err(IndexTraceStoreFailure::NotFound);
        }
        let snapshot = self
            .retained()
            .current
            .filter(|trace| trace.summary().id() == query.trace_id)
            .or_else(|| {
                self.previous
                    .clone()
                    .filter(|trace| trace.summary().id() == query.trace_id)
            })
            .ok_or(IndexTraceStoreFailure::NotFound)?;
        if snapshot.summary().revision() != query.revision {
            return Err(IndexTraceStoreFailure::StaleRevision);
        }
        let search = query.search.as_ref().map(|value| value.to_lowercase());
        let matches = snapshot.files().iter().filter(|file| {
            search
                .as_ref()
                .is_none_or(|search| path_display(file.path()).to_lowercase().contains(search))
                && matches_filter(file, query.filter)
        });
        let total = u64::try_from(matches.clone().count())
            .map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?;
        let offset = match usize::try_from(query.offset) {
            Ok(offset) => offset,
            Err(_) => usize::MAX,
        };
        let files = matches
            .skip(offset)
            .take(usize::from(INDEX_TRACE_FILE_PAGE_LIMIT))
            .cloned()
            .collect::<Vec<_>>();
        let returned =
            u64::try_from(files.len()).map_err(|_| IndexTraceStoreFailure::InvalidStoredData)?;
        Ok(IndexTraceFilePage {
            files,
            total,
            offset: query.offset,
            has_more: query
                .offset
                .checked_add(returned)
                .is_some_and(|end| end < total),
        })
    }
}

#[derive(Debug)]
struct TraceProjection {
    summary: MutableSummary,
    phases: [MutablePhase; 6],
    events: VecDeque<IndexTraceEvent>,
    pending_events: VecDeque<IndexTraceEvent>,
    files: BTreeMap<RepositoryPath, IndexTraceFileRecord>,
    dirty_files: BTreeMap<RepositoryPath, IndexTraceRevision>,
    persisted_revision: Option<IndexTraceRevision>,
}

impl TraceProjection {
    fn queued(
        id: IndexTraceId,
        trigger: IndexTraceTrigger,
        at: IndexTraceTimestamp,
        previous_publication_available: bool,
    ) -> Self {
        let summary = MutableSummary {
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
            counts: IndexTraceCounts::default(),
        };
        let event = IndexTraceEvent::new(
            IndexTraceRevision::FIRST,
            at,
            IndexTraceEventKind::Queued,
            Some(IndexTracePhase::Discover),
            None,
            None,
        );
        Self {
            summary,
            phases: MutablePhase::pending_all(),
            events: VecDeque::from([event.clone()]),
            pending_events: VecDeque::from([event]),
            files: BTreeMap::new(),
            dirty_files: BTreeMap::new(),
            persisted_revision: None,
        }
    }

    fn from_snapshot(snapshot: IndexTraceSnapshot) -> Option<Self> {
        let summary = MutableSummary::from_summary(snapshot.summary());
        let phases = MutablePhase::from_rows(snapshot.phases())?;
        let files = snapshot
            .files()
            .iter()
            .cloned()
            .map(|file| (file.path().clone(), file))
            .collect();
        Some(Self {
            persisted_revision: Some(summary.revision),
            summary,
            phases,
            events: snapshot.events().iter().cloned().collect(),
            pending_events: VecDeque::new(),
            files,
            dirty_files: BTreeMap::new(),
        })
    }

    fn observe(&mut self, observation: RepositoryIndexObservation, at: IndexTraceTimestamp) {
        match observation {
            RepositoryIndexObservation::PhaseStarted(phase) => {
                self.enter_phase(map_phase(phase), at);
            }
            RepositoryIndexObservation::PhaseProgress { phase, progress } => {
                self.phase_progress(map_phase(phase), progress, at);
            }
            RepositoryIndexObservation::CurrentFile { phase, path } => {
                if self.summary.current_file.as_ref() != Some(&path)
                    || self.summary.current_phase != Some(map_phase(phase))
                {
                    self.summary.current_phase = Some(map_phase(phase));
                    self.summary.current_file = Some(path.clone());
                    self.push_event(
                        IndexTraceEventKind::FileObserved,
                        Some(map_phase(phase)),
                        Some(path),
                        None,
                        at,
                    );
                }
            }
            RepositoryIndexObservation::FileDiscovered(path) => {
                if !self.files.contains_key(&path)
                    && let Ok(file) = IndexTraceFileRecord::new(
                        path,
                        IndexTraceFileChange::Pending,
                        IndexTraceHashOutcome::Pending,
                        None,
                        Vec::new(),
                        false,
                    )
                {
                    let _inserted = self.upsert_file(file, at);
                }
            }
            RepositoryIndexObservation::FileResult(file) => {
                let path = file.path().clone();
                let diagnostics = file.diagnostics().to_vec();
                if self.upsert_file(file, at) {
                    for diagnostic in diagnostics {
                        self.push_event(
                            IndexTraceEventKind::Diagnostic,
                            Some(IndexTracePhase::Parse),
                            Some(path.clone()),
                            Some(diagnostic),
                            at,
                        );
                    }
                }
            }
            RepositoryIndexObservation::Diagnostic { phase, path, code } => {
                self.summary.diagnostic = Some(code);
                self.push_event(
                    IndexTraceEventKind::Diagnostic,
                    Some(map_phase(phase)),
                    path,
                    Some(code),
                    at,
                );
            }
        }
    }

    fn enter_phase(&mut self, phase: IndexTracePhase, at: IndexTraceTimestamp) {
        if let Some(current) = self.summary.current_phase
            && phase != current
            && phase.ordinal() != current.ordinal().saturating_add(1)
        {
            return;
        }
        for row in &mut self.phases {
            if row.phase.ordinal() < phase.ordinal() && row.state == IndexTracePhaseState::Running {
                row.state = IndexTracePhaseState::Succeeded;
                row.ended_at = Some(at);
            }
        }
        let row = &mut self.phases[usize::from(phase.ordinal())];
        if row.state == IndexTracePhaseState::Running {
            return;
        }
        if row.state == IndexTracePhaseState::Pending {
            row.state = IndexTracePhaseState::Running;
            row.started_at = Some(at);
        }
        self.summary.state = IndexTraceState::Running;
        self.summary.current_phase = Some(phase);
        self.summary.current_file = None;
        self.push_event(
            IndexTraceEventKind::PhaseStarted,
            Some(phase),
            None,
            None,
            at,
        );
    }

    fn phase_progress(
        &mut self,
        phase: IndexTracePhase,
        progress: Progress,
        at: IndexTraceTimestamp,
    ) {
        if self.summary.current_phase != Some(phase) {
            return;
        }
        let (completed, total) = match progress {
            Progress::Indeterminate => (None, None),
            Progress::Determinate { completed, total } => (Some(completed), Some(total.get())),
        };
        let row = &mut self.phases[usize::from(phase.ordinal())];
        if row.completed == completed && row.total == total {
            return;
        }
        row.completed = completed;
        row.total = total;
        self.push_event(IndexTraceEventKind::Progress, Some(phase), None, None, at);
    }

    fn upsert_file(&mut self, file: IndexTraceFileRecord, at: IndexTraceTimestamp) -> bool {
        let path = file.path().clone();
        if self.files.get(&path) == Some(&file) {
            return false;
        }
        if let Some(previous) = self.files.insert(path.clone(), file.clone()) {
            subtract_counts(&mut self.summary.counts, &previous);
        }
        add_counts(&mut self.summary.counts, &file);
        self.bump_revision(at);
        self.dirty_files.insert(path, self.summary.revision);
        true
    }

    fn set_state(
        &mut self,
        state: IndexTraceState,
        diagnostic: Option<IndexTraceDiagnosticCode>,
        at: IndexTraceTimestamp,
    ) {
        if !self.summary.state.can_transition_to(state) || self.summary.state == state {
            return;
        }
        self.summary.state = state;
        let event_path = diagnostic.and(self.summary.current_file.clone());
        self.summary.diagnostic = match self.summary.diagnostic {
            Some(existing @ (IndexTraceDiagnosticCode::Link | IndexTraceDiagnosticCode::Rank)) => {
                Some(existing)
            }
            _ => diagnostic.or(self.summary.diagnostic),
        };
        let event_diagnostic = diagnostic.and(self.summary.diagnostic);
        if state.is_terminal() {
            self.summary.ended_at = Some(at);
            self.summary.current_file = None;
            if let Some(phase) = self.summary.current_phase {
                let row = &mut self.phases[usize::from(phase.ordinal())];
                row.ended_at = Some(at);
                row.state = if state == IndexTraceState::Succeeded {
                    IndexTracePhaseState::Succeeded
                } else if state == IndexTraceState::Failed {
                    IndexTracePhaseState::Failed
                } else {
                    IndexTracePhaseState::Cancelled
                };
            }
            if state == IndexTraceState::Succeeded {
                for row in &mut self.phases {
                    if row.state == IndexTracePhaseState::Pending {
                        row.state = IndexTracePhaseState::Succeeded;
                        row.started_at = Some(at);
                        row.ended_at = Some(at);
                    }
                }
            }
        }
        let kind = match state {
            IndexTraceState::Running => IndexTraceEventKind::Started,
            IndexTraceState::Cancelling => IndexTraceEventKind::CancellationRequested,
            IndexTraceState::Succeeded => IndexTraceEventKind::Succeeded,
            IndexTraceState::Failed => IndexTraceEventKind::Failed,
            IndexTraceState::Cancelled => IndexTraceEventKind::Cancelled,
            IndexTraceState::Interrupted => IndexTraceEventKind::Interrupted,
            IndexTraceState::Queued => IndexTraceEventKind::Queued,
        };
        self.push_event(
            kind,
            self.summary.current_phase,
            event_path,
            event_diagnostic,
            at,
        );
    }

    fn mark_details_incomplete(&mut self, at: IndexTraceTimestamp) {
        if self.summary.details_incomplete {
            return;
        }
        self.summary.details_incomplete = true;
        self.summary.diagnostic = Some(IndexTraceDiagnosticCode::JournalIncomplete);
        self.push_event(
            IndexTraceEventKind::Diagnostic,
            self.summary.current_phase,
            None,
            Some(IndexTraceDiagnosticCode::JournalIncomplete),
            at,
        );
    }

    fn push_event(
        &mut self,
        kind: IndexTraceEventKind,
        phase: Option<IndexTracePhase>,
        path: Option<RepositoryPath>,
        diagnostic: Option<IndexTraceDiagnosticCode>,
        at: IndexTraceTimestamp,
    ) {
        self.bump_revision(at);
        let event = IndexTraceEvent::new(self.summary.revision, at, kind, phase, path, diagnostic);
        push_bounded(&mut self.events, event.clone());
        push_bounded(&mut self.pending_events, event);
    }

    fn bump_revision(&mut self, at: IndexTraceTimestamp) {
        if let Ok(next) = self.summary.revision.next() {
            self.summary.revision = next;
            self.summary.last_activity_at = at;
        } else {
            self.summary.details_incomplete = true;
        }
    }

    fn snapshot(&self) -> Result<IndexTraceSnapshot, a3_application::IndexTraceDataError> {
        IndexTraceSnapshot::new(
            self.summary.to_summary()?,
            self.phases
                .iter()
                .copied()
                .map(MutablePhase::to_progress)
                .collect::<Result<Vec<_>, _>>()?,
            self.events.iter().cloned().collect(),
            self.files.values().cloned().collect(),
        )
    }

    fn checkpoint(&self) -> Option<IndexTraceCheckpoint> {
        let expected_revision = self.persisted_revision?;
        if expected_revision == self.summary.revision
            && self.dirty_files.is_empty()
            && self.pending_events.is_empty()
        {
            return None;
        }
        let snapshot = self.snapshot().ok()?;
        let file_updates = self
            .dirty_files
            .keys()
            .take(256)
            .filter_map(|path| self.files.get(path).cloned())
            .collect();
        Some(IndexTraceCheckpoint {
            expected_revision,
            snapshot,
            file_updates,
            new_events: self.pending_events.iter().cloned().collect(),
        })
    }

    fn acknowledge(&mut self, checkpoint: &IndexTraceCheckpoint) {
        let acknowledged = checkpoint.snapshot.summary().revision();
        self.persisted_revision = Some(acknowledged);
        let paths = checkpoint
            .file_updates
            .iter()
            .map(|file| file.path().clone())
            .collect::<BTreeSet<_>>();
        self.dirty_files
            .retain(|path, revision| !paths.contains(path) || *revision > acknowledged);
        self.pending_events
            .retain(|event| event.revision() > acknowledged);
    }
}

#[derive(Debug)]
struct MutableSummary {
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

impl MutableSummary {
    fn from_summary(summary: &IndexTraceRunSummary) -> Self {
        Self {
            id: summary.id(),
            revision: summary.revision(),
            state: summary.state(),
            trigger: summary.trigger(),
            started_at: summary.started_at(),
            ended_at: summary.ended_at(),
            last_activity_at: summary.last_activity_at(),
            current_phase: summary.current_phase(),
            current_file: summary.current_file().cloned(),
            diagnostic: summary.diagnostic(),
            details_incomplete: summary.details_incomplete(),
            previous_publication_available: summary.previous_publication_available(),
            counts: summary.counts(),
        }
    }

    fn to_summary(&self) -> Result<IndexTraceRunSummary, a3_application::IndexTraceDataError> {
        IndexTraceRunSummary::restored(
            self.id,
            self.revision,
            self.state,
            self.trigger,
            self.started_at,
            self.ended_at,
            self.last_activity_at,
            self.current_phase,
            self.current_file.clone(),
            self.diagnostic,
            self.details_incomplete,
            self.previous_publication_available,
            self.counts,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct MutablePhase {
    phase: IndexTracePhase,
    state: IndexTracePhaseState,
    started_at: Option<IndexTraceTimestamp>,
    ended_at: Option<IndexTraceTimestamp>,
    completed: Option<u64>,
    total: Option<u64>,
}

impl MutablePhase {
    fn pending_all() -> [Self; 6] {
        IndexTracePhase::ALL.map(|phase| Self {
            phase,
            state: IndexTracePhaseState::Pending,
            started_at: None,
            ended_at: None,
            completed: None,
            total: None,
        })
    }

    fn from_rows(rows: &[IndexTracePhaseProgress]) -> Option<[Self; 6]> {
        if rows.len() != 6 {
            return None;
        }
        let values = rows
            .iter()
            .map(|row| Self {
                phase: row.phase(),
                state: row.state(),
                started_at: row.started_at(),
                ended_at: row.ended_at(),
                completed: row.completed(),
                total: row.total(),
            })
            .collect::<Vec<_>>();
        values.try_into().ok()
    }

    fn to_progress(self) -> Result<IndexTracePhaseProgress, a3_application::IndexTraceDataError> {
        IndexTracePhaseProgress::new(
            self.phase,
            self.state,
            self.started_at,
            self.ended_at,
            self.completed,
            self.total,
        )
    }
}

fn add_counts(counts: &mut IndexTraceCounts, file: &IndexTraceFileRecord) {
    if file.change() != IndexTraceFileChange::Deleted {
        counts.discovered = counts.discovered.saturating_add(1);
    }
    match file.change() {
        IndexTraceFileChange::Pending => counts.pending = counts.pending.saturating_add(1),
        IndexTraceFileChange::New => counts.new_files = counts.new_files.saturating_add(1),
        IndexTraceFileChange::Changed => counts.changed = counts.changed.saturating_add(1),
        IndexTraceFileChange::Unchanged => counts.unchanged = counts.unchanged.saturating_add(1),
        IndexTraceFileChange::Deleted => counts.deleted = counts.deleted.saturating_add(1),
    }
    match file.hash() {
        IndexTraceHashOutcome::Pending => {}
        IndexTraceHashOutcome::Hashed => counts.hashed = counts.hashed.saturating_add(1),
        IndexTraceHashOutcome::Reused => counts.hash_reused = counts.hash_reused.saturating_add(1),
        IndexTraceHashOutcome::NotApplicable => {}
    }
    match file.parse() {
        Some(IndexTraceParseOutcome::Structural) => {
            counts.structural = counts.structural.saturating_add(1)
        }
        Some(IndexTraceParseOutcome::Reused) => {
            counts.parse_reused = counts.parse_reused.saturating_add(1)
        }
        Some(IndexTraceParseOutcome::Generic) => counts.generic = counts.generic.saturating_add(1),
        Some(IndexTraceParseOutcome::Failed) => counts.failed = counts.failed.saturating_add(1),
        Some(IndexTraceParseOutcome::NotApplicable) | None => {}
    }
}

fn subtract_counts(counts: &mut IndexTraceCounts, file: &IndexTraceFileRecord) {
    if file.change() != IndexTraceFileChange::Deleted {
        counts.discovered = counts.discovered.saturating_sub(1);
    }
    match file.change() {
        IndexTraceFileChange::Pending => counts.pending = counts.pending.saturating_sub(1),
        IndexTraceFileChange::New => counts.new_files = counts.new_files.saturating_sub(1),
        IndexTraceFileChange::Changed => counts.changed = counts.changed.saturating_sub(1),
        IndexTraceFileChange::Unchanged => counts.unchanged = counts.unchanged.saturating_sub(1),
        IndexTraceFileChange::Deleted => counts.deleted = counts.deleted.saturating_sub(1),
    }
    match file.hash() {
        IndexTraceHashOutcome::Pending => {}
        IndexTraceHashOutcome::Hashed => counts.hashed = counts.hashed.saturating_sub(1),
        IndexTraceHashOutcome::Reused => counts.hash_reused = counts.hash_reused.saturating_sub(1),
        IndexTraceHashOutcome::NotApplicable => {}
    }
    match file.parse() {
        Some(IndexTraceParseOutcome::Structural) => {
            counts.structural = counts.structural.saturating_sub(1)
        }
        Some(IndexTraceParseOutcome::Reused) => {
            counts.parse_reused = counts.parse_reused.saturating_sub(1)
        }
        Some(IndexTraceParseOutcome::Generic) => counts.generic = counts.generic.saturating_sub(1),
        Some(IndexTraceParseOutcome::Failed) => counts.failed = counts.failed.saturating_sub(1),
        Some(IndexTraceParseOutcome::NotApplicable) | None => {}
    }
}

fn push_bounded(events: &mut VecDeque<IndexTraceEvent>, event: IndexTraceEvent) {
    if events.len() == INDEX_TRACE_EVENT_LIMIT {
        events.pop_front();
    }
    events.push_back(event);
}

fn matches_filter(file: &IndexTraceFileRecord, filter: IndexTraceFileFilter) -> bool {
    match filter {
        IndexTraceFileFilter::All => true,
        IndexTraceFileFilter::New => file.change() == IndexTraceFileChange::New,
        IndexTraceFileFilter::Changed => file.change() == IndexTraceFileChange::Changed,
        IndexTraceFileFilter::Unchanged => file.change() == IndexTraceFileChange::Unchanged,
        IndexTraceFileFilter::Deleted => file.change() == IndexTraceFileChange::Deleted,
        IndexTraceFileFilter::Hashed => file.hash() == IndexTraceHashOutcome::Hashed,
        IndexTraceFileFilter::HashReused => file.hash() == IndexTraceHashOutcome::Reused,
        IndexTraceFileFilter::Structural => {
            file.parse() == Some(IndexTraceParseOutcome::Structural)
        }
        IndexTraceFileFilter::ParseReused => file.parse() == Some(IndexTraceParseOutcome::Reused),
        IndexTraceFileFilter::Generic => file.parse() == Some(IndexTraceParseOutcome::Generic),
        IndexTraceFileFilter::Failed => file.parse() == Some(IndexTraceParseOutcome::Failed),
    }
}

pub(crate) fn path_display(path: &RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .take(512)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn map_phase(phase: RepositoryIndexPhase) -> IndexTracePhase {
    match phase {
        RepositoryIndexPhase::Discover => IndexTracePhase::Discover,
        RepositoryIndexPhase::Hash => IndexTracePhase::Hash,
        RepositoryIndexPhase::Parse => IndexTracePhase::Parse,
        RepositoryIndexPhase::Link => IndexTracePhase::Link,
        RepositoryIndexPhase::Rank => IndexTracePhase::Rank,
        RepositoryIndexPhase::Publish => IndexTracePhase::Publish,
    }
}

pub(crate) fn now() -> IndexTraceTimestamp {
    let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(i64::MAX as u128) as i64,
        Err(_) => 0,
    };
    match IndexTraceTimestamp::new(millis) {
        Ok(timestamp) => timestamp,
        Err(_) => IndexTraceTimestamp::UNIX_EPOCH,
    }
}

fn derive_trace_id(
    project: &ProjectIdentity,
    job_id: a3_domain::JobId,
    at: IndexTraceTimestamp,
) -> IndexTraceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3.index-trace.v1\0");
    hasher.update(project.worktree().id().as_bytes());
    hasher.update(&job_id.value().to_be_bytes());
    hasher.update(&at.unix_millis().to_be_bytes());
    IndexTraceId::from_bytes(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{IndexTraceRuntime, MutablePhase, TraceProjection};
    use a3_application::{IndexTraceEventKind, IndexTraceFileRecord, RepositoryIndexObservation};
    use a3_domain::{
        IndexTraceDiagnosticCode, IndexTraceFileChange, IndexTraceHashOutcome, IndexTraceId,
        IndexTraceParseOutcome, IndexTracePhase, IndexTracePhaseState, IndexTraceTimestamp,
        IndexTraceTrigger, RepositoryPath,
    };

    #[test]
    fn phases_begin_in_exact_pending_order() {
        let phases = MutablePhase::pending_all();
        assert_eq!(phases.map(|phase| phase.phase), IndexTracePhase::ALL);
        assert!(
            phases
                .iter()
                .all(|phase| phase.state == IndexTracePhaseState::Pending)
        );
        assert!(IndexTraceRuntime::default().retained().current.is_none());
    }

    #[test]
    fn phases_cannot_skip_or_move_backwards() {
        let at = IndexTraceTimestamp::UNIX_EPOCH;
        let mut trace = TraceProjection::queued(
            IndexTraceId::from_bytes([1; 32]),
            IndexTraceTrigger::InitialObservation,
            at,
            false,
        );
        trace.enter_phase(IndexTracePhase::Parse, at);
        assert_eq!(trace.summary.current_phase, Some(IndexTracePhase::Discover));
        trace.enter_phase(IndexTracePhase::Discover, at);
        trace.enter_phase(IndexTracePhase::Hash, at);
        assert_eq!(trace.summary.current_phase, Some(IndexTracePhase::Hash));
        assert_eq!(trace.phases[0].state, IndexTracePhaseState::Succeeded);
        trace.enter_phase(IndexTracePhase::Discover, at);
        assert_eq!(trace.summary.current_phase, Some(IndexTracePhase::Hash));
    }

    #[test]
    fn discovery_is_truthfully_pending_until_the_complete_file_result_arrives()
    -> Result<(), Box<dyn std::error::Error>> {
        let at = IndexTraceTimestamp::UNIX_EPOCH;
        let path = RepositoryPath::try_from_bytes(b"src/broken.rs".to_vec())?;
        let mut trace = TraceProjection::queued(
            IndexTraceId::from_bytes([1; 32]),
            IndexTraceTrigger::InitialObservation,
            at,
            false,
        );
        trace.observe(RepositoryIndexObservation::FileDiscovered(path.clone()), at);
        assert_eq!(trace.summary.counts.discovered, 1);
        assert_eq!(trace.summary.counts.pending, 1);
        let pending = trace
            .files
            .get(&path)
            .ok_or_else(|| std::io::Error::other("discovered path was not retained"))?;
        assert_eq!(pending.change(), IndexTraceFileChange::Pending);
        assert_eq!(pending.hash(), IndexTraceHashOutcome::Pending);

        trace.observe(
            RepositoryIndexObservation::FileResult(IndexTraceFileRecord::new(
                path.clone(),
                IndexTraceFileChange::New,
                IndexTraceHashOutcome::Hashed,
                Some(IndexTraceParseOutcome::Failed),
                vec![IndexTraceDiagnosticCode::Parse],
                false,
            )?),
            at,
        );
        assert_eq!(trace.summary.counts.pending, 0);
        assert_eq!(trace.summary.counts.new_files, 1);
        assert_eq!(trace.summary.counts.failed, 1);
        assert!(trace.events.iter().any(|event| {
            event.kind() == IndexTraceEventKind::Diagnostic
                && event.path() == Some(&path)
                && event.diagnostic() == Some(IndexTraceDiagnosticCode::Parse)
        }));
        Ok(())
    }

    #[test]
    fn journal_failure_is_visible_without_terminalizing_the_index_trace() {
        let at = IndexTraceTimestamp::UNIX_EPOCH;
        let mut trace = TraceProjection::queued(
            IndexTraceId::from_bytes([1; 32]),
            IndexTraceTrigger::InitialObservation,
            at,
            true,
        );

        trace.mark_details_incomplete(at);

        assert!(trace.summary.details_incomplete);
        assert_eq!(trace.summary.state, a3_domain::IndexTraceState::Queued);
        assert!(trace.events.iter().any(|event| {
            event.kind() == IndexTraceEventKind::Diagnostic
                && event.diagnostic() == Some(IndexTraceDiagnosticCode::JournalIncomplete)
        }));
    }

    #[test]
    fn terminal_failure_keeps_the_affected_file_and_specific_phase_code()
    -> Result<(), Box<dyn std::error::Error>> {
        let at = IndexTraceTimestamp::UNIX_EPOCH;
        let path = RepositoryPath::try_from_bytes(b"src/current.rs".to_vec())?;
        let mut trace = TraceProjection::queued(
            IndexTraceId::from_bytes([1; 32]),
            IndexTraceTrigger::InitialObservation,
            at,
            true,
        );
        trace.enter_phase(IndexTracePhase::Discover, at);
        trace.enter_phase(IndexTracePhase::Hash, at);
        trace.observe(
            RepositoryIndexObservation::CurrentFile {
                phase: a3_application::RepositoryIndexPhase::Hash,
                path: path.clone(),
            },
            at,
        );
        trace.set_state(
            a3_domain::IndexTraceState::Failed,
            Some(IndexTraceDiagnosticCode::SourceUnavailable),
            at,
        );
        assert!(trace.events.back().is_some_and(|event| {
            event.kind() == IndexTraceEventKind::Failed
                && event.path() == Some(&path)
                && event.diagnostic() == Some(IndexTraceDiagnosticCode::SourceUnavailable)
        }));

        let mut link_trace = TraceProjection::queued(
            IndexTraceId::from_bytes([2; 32]),
            IndexTraceTrigger::InitialObservation,
            at,
            true,
        );
        for phase in [
            IndexTracePhase::Discover,
            IndexTracePhase::Hash,
            IndexTracePhase::Parse,
            IndexTracePhase::Link,
        ] {
            link_trace.enter_phase(phase, at);
        }
        link_trace.observe(
            RepositoryIndexObservation::Diagnostic {
                phase: a3_application::RepositoryIndexPhase::Link,
                path: None,
                code: IndexTraceDiagnosticCode::Link,
            },
            at,
        );
        link_trace.set_state(
            a3_domain::IndexTraceState::Failed,
            Some(IndexTraceDiagnosticCode::Parse),
            at,
        );
        assert_eq!(
            link_trace.summary.diagnostic,
            Some(IndexTraceDiagnosticCode::Link)
        );
        Ok(())
    }
}
