use a3_domain::{
    AgentResearchDepth, AgentSession, AgentSessionEntry, AgentSessionId, AgentSessionMode,
    AgentSessionRevision, AgentSessionSequence, AgentSessionTimestamp, AskResearchCompleteness,
    AskResearchPhase, AskResearchSelectionReason, AskResearchSourceId, AskResearchSourceKind,
    AskResearchState, FileRevision, IndexRunId, ProjectIdentity, SnapshotId, SourceRange, TaskId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_ACTION_BYTES: usize = 512;
const MAX_QUERY_BYTES: usize = 4 * 1024;
const MAX_SYMBOL_BYTES: usize = 512;
const MAX_SOURCES_PER_TURN: usize = 200;

/// Validated literals for one bounded current-source search action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskSourceTextSearch {
    literals: Vec<String>,
}

impl AskSourceTextSearch {
    /// Accepts one to eight non-empty case-insensitive literal strings.
    pub fn new(literals: Vec<String>) -> Result<Self, AskResearchDataError> {
        if literals.is_empty()
            || literals.len() > 8
            || literals.iter().any(|literal| !safe_text(literal, 256))
        {
            return Err(AskResearchDataError::InvalidEvent);
        }
        Ok(Self { literals })
    }
    /// Returns literals in model-request order.
    #[must_use]
    pub fn literals(&self) -> &[String] {
        &self.literals
    }
}

/// One exact current-source literal match, without retained source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskSourceTextHit {
    revision: FileRevision,
    range: SourceRange,
    literal: String,
}

impl AskSourceTextHit {
    /// Creates one current source hit.
    #[must_use]
    pub fn new(revision: FileRevision, range: SourceRange, literal: String) -> Self {
        Self {
            revision,
            range,
            literal,
        }
    }
    /// Returns the hash-bound file revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }
    /// Returns the exact matched span.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }
    /// Returns the requested literal that matched.
    #[must_use]
    pub fn literal(&self) -> &str {
        &self.literal
    }
}

/// Result of a bounded current-source scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskSourceTextSearchResult {
    hits: Vec<AskSourceTextHit>,
    files_examined: u16,
    bytes_examined: u64,
    completeness: AskResearchCompleteness,
}

impl AskSourceTextSearchResult {
    /// Validates the fixed 100-hit, 2,000-file, and 32-MiB result boundaries.
    pub fn new(
        hits: Vec<AskSourceTextHit>,
        files_examined: u16,
        bytes_examined: u64,
        completeness: AskResearchCompleteness,
    ) -> Result<Self, AskResearchDataError> {
        if hits.len() > 100 || files_examined > 2_000 || bytes_examined > 32 * 1_024 * 1_024 {
            return Err(AskResearchDataError::InvalidDetail);
        }
        Ok(Self {
            hits,
            files_examined,
            bytes_examined,
            completeness,
        })
    }
    /// Returns hits in index-file, line, and literal order.
    #[must_use]
    pub fn hits(&self) -> &[AskSourceTextHit] {
        &self.hits
    }
    /// Returns safely readable current files actually examined.
    #[must_use]
    pub const fn files_examined(&self) -> u16 {
        self.files_examined
    }
    /// Returns current UTF-8 bytes actually examined.
    #[must_use]
    pub const fn bytes_examined(&self) -> u64 {
        self.bytes_examined
    }
    /// Returns whether all eligible index files were covered.
    #[must_use]
    pub const fn completeness(&self) -> AskResearchCompleteness {
        self.completeness
    }
}

/// Cooperative cancellation for a bounded repository-source search.
pub trait AskSourceSearchControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning Ask turn was cancelled.
    fn is_cancelled(&self) -> bool;
}

impl AskSourceSearchControl for crate::JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Future returned by a safe repository-source search adapter.
pub type AskSourceSearcherFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AskSourceTextSearchResult, AskSourceSearchFailure>> + Send + 'a>,
>;

/// Read-only current-source search boundary implemented by the workspace adapter.
pub trait AskSourceSearcher: fmt::Debug + Send + Sync {
    /// Scans only files from the supplied immutable publication.
    fn search<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a a3_domain::PublishedIndex,
        query: &'a AskSourceTextSearch,
        control: &'a dyn AskSourceSearchControl,
    ) -> AskSourceSearcherFuture<'a>;
}

/// Stable source-search failure without paths or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskSourceSearchFailure {
    /// The owning Ask turn was cancelled.
    Cancelled,
    /// The safe workspace reader was unavailable.
    Unavailable,
    /// The adapter returned data outside the fixed contract.
    InvalidResult,
}
impl fmt::Display for AskSourceSearchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ask source search failed")
    }
}
impl Error for AskSourceSearchFailure {}

/// Immutable header binding one Ask turn to exactly one published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchTurn {
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    started_at: AgentSessionTimestamp,
    mode: AgentSessionMode,
    depth: AgentResearchDepth,
    legacy: bool,
}

impl AskResearchTurn {
    /// Creates an index-bound research turn before any model request is made.
    #[must_use]
    pub const fn new(
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        started_at: AgentSessionTimestamp,
    ) -> Self {
        Self::new_for_mode(
            session_id,
            user_sequence,
            index_run_id,
            snapshot_id,
            started_at,
            AgentSessionMode::Ask,
            AgentResearchDepth::Standard,
        )
    }

    /// Creates one generic Ask, Plan, or Agent-preparation research section.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new_for_mode(
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        started_at: AgentSessionTimestamp,
        mode: AgentSessionMode,
        depth: AgentResearchDepth,
    ) -> Self {
        Self {
            session_id,
            user_sequence,
            index_run_id,
            snapshot_id,
            started_at,
            mode,
            depth,
            legacy: false,
        }
    }

    /// Returns the owning session.
    #[must_use]
    pub const fn session_id(&self) -> AgentSessionId {
        self.session_id
    }
    /// Returns the user-message sequence that started this research.
    #[must_use]
    pub const fn user_sequence(&self) -> AgentSessionSequence {
        self.user_sequence
    }
    /// Returns the pinned published index run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns the pinned snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the start time.
    #[must_use]
    pub const fn started_at(&self) -> AgentSessionTimestamp {
        self.started_at
    }
    /// Returns the conversation mode that owns this section.
    #[must_use]
    pub const fn mode(&self) -> AgentSessionMode {
        self.mode
    }
    /// Returns the fixed per-message depth.
    #[must_use]
    pub const fn depth(&self) -> AgentResearchDepth {
        self.depth
    }
    /// Marks a V30 Ask trace reconstructed without generic notes or depth metadata.
    #[must_use]
    pub const fn as_legacy(mut self) -> Self {
        self.legacy = true;
        self
    }
    /// Returns whether this was reconstructed from the V30 Ask-only schema.
    #[must_use]
    pub const fn legacy(&self) -> bool {
        self.legacy
    }
}

/// Persistable epistemic kind of a public work note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResearchPublicFindingKind {
    /// Direct current Evidence observation.
    Observation,
    /// Explicitly unproven search lead.
    Hypothesis,
    /// Conclusion supported by current Evidence.
    Conclusion,
}

/// Bounded public work note, structurally separate from every executable action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchPublicNote {
    goal: String,
    finding_kind: AskResearchPublicFindingKind,
    finding: String,
    source_ids: Vec<AskResearchSourceId>,
    gap: String,
    next_step: String,
}

impl AskResearchPublicNote {
    /// Creates one safe note without prompts, provider output, or hidden reasoning.
    pub fn new(
        goal: String,
        finding_kind: AskResearchPublicFindingKind,
        finding: String,
        mut source_ids: Vec<AskResearchSourceId>,
        gap: String,
        next_step: String,
    ) -> Result<Self, AskResearchDataError> {
        if !safe_text(&goal, 1024)
            || !safe_text(&finding, 4096)
            || !safe_text(&gap, 1024)
            || !safe_text(&next_step, 1024)
            || source_ids.len() > 32
            || (finding_kind != AskResearchPublicFindingKind::Hypothesis && source_ids.is_empty())
        {
            return Err(AskResearchDataError::InvalidEvent);
        }
        // Revalidation can map several historical spans to one current, enclosing source.
        // Keep the original bound before canonicalization, and preserve citation order.
        let mut seen = Vec::with_capacity(source_ids.len());
        source_ids.retain(|id| {
            if seen.contains(id) {
                false
            } else {
                seen.push(*id);
                true
            }
        });
        Ok(Self {
            goal,
            finding_kind,
            finding,
            source_ids,
            gap,
            next_step,
        })
    }
    #[must_use]
    /// Returns the current bounded subgoal.
    pub fn goal(&self) -> &str {
        &self.goal
    }
    #[must_use]
    /// Returns the epistemic classification of the finding.
    pub const fn finding_kind(&self) -> AskResearchPublicFindingKind {
        self.finding_kind
    }
    #[must_use]
    /// Returns the public observation, hypothesis, or conclusion.
    pub fn finding(&self) -> &str {
        &self.finding
    }
    #[must_use]
    /// Returns the original source chain supporting this finding.
    pub fn source_ids(&self) -> &[AskResearchSourceId] {
        &self.source_ids
    }
    #[must_use]
    /// Returns the remaining Evidence gap.
    pub fn gap(&self) -> &str {
        &self.gap
    }
    #[must_use]
    /// Returns the purpose of the next action.
    pub fn next_step(&self) -> &str {
        &self.next_step
    }
}

/// One append-only, content-free live event in an Ask research turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchEvent {
    work_state: Option<a3_domain::ResearchWorkState>,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    sequence: u32,
    phase: AskResearchPhase,
    state: AskResearchState,
    action: String,
    query: Option<String>,
    completeness: AskResearchCompleteness,
    occurred_at: AgentSessionTimestamp,
    public_note: Option<AskResearchPublicNote>,
}

impl AskResearchEvent {
    /// Creates a bounded event without prompts, source text, model payloads, or internal reasoning.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        sequence: u32,
        phase: AskResearchPhase,
        state: AskResearchState,
        action: String,
        query: Option<String>,
        completeness: AskResearchCompleteness,
        occurred_at: AgentSessionTimestamp,
    ) -> Result<Self, AskResearchDataError> {
        if sequence == 0 || sequence > 1024 || !safe_text(&action, MAX_ACTION_BYTES) {
            return Err(AskResearchDataError::InvalidEvent);
        }
        if query
            .as_ref()
            .is_some_and(|value| !safe_text(value, MAX_QUERY_BYTES))
        {
            return Err(AskResearchDataError::InvalidEvent);
        }
        Ok(Self {
            session_id,
            user_sequence,
            sequence,
            phase,
            state,
            action,
            query,
            completeness,
            occurred_at,
            public_note: None,
            work_state: None,
        })
    }

    /// Attaches one already validated public note without changing action authority.
    #[must_use]
    pub fn with_public_note(mut self, note: AskResearchPublicNote) -> Self {
        self.public_note = Some(note);
        self
    }

    /// Commits an admitted research checkpoint atomically with this audit event.
    #[must_use]
    pub fn with_work_state(mut self, state: a3_domain::ResearchWorkState) -> Self {
        self.work_state = Some(state);
        self
    }

    /// Returns the optional Core-owned checkpoint, never a raw model proposal.
    #[must_use]
    pub const fn work_state(&self) -> Option<&a3_domain::ResearchWorkState> {
        self.work_state.as_ref()
    }

    /// Returns the owning session.
    #[must_use]
    pub const fn session_id(&self) -> AgentSessionId {
        self.session_id
    }
    /// Returns the owning user-message sequence.
    #[must_use]
    pub const fn user_sequence(&self) -> AgentSessionSequence {
        self.user_sequence
    }
    /// Returns the monotone event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }
    /// Returns the user-facing phase.
    #[must_use]
    pub const fn phase(&self) -> AskResearchPhase {
        self.phase
    }
    /// Returns the active or terminal state.
    #[must_use]
    pub const fn state(&self) -> AskResearchState {
        self.state
    }
    /// Returns the concrete safe action text.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
    /// Returns the bounded visible search expression, when applicable.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
    /// Returns whether the associated search completed its scope.
    #[must_use]
    pub const fn completeness(&self) -> AskResearchCompleteness {
        self.completeness
    }
    /// Returns when the event occurred.
    #[must_use]
    pub const fn occurred_at(&self) -> AgentSessionTimestamp {
        self.occurred_at
    }
    /// Returns the optional public, non-authoritative work note.
    #[must_use]
    pub const fn public_note(&self) -> Option<&AskResearchPublicNote> {
        self.public_note.as_ref()
    }
}

/// One current Evidence reference disclosed by the research UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchSource {
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    id: AskResearchSourceId,
    ordinal: u32,
    revision: FileRevision,
    range: Option<SourceRange>,
    symbol: Option<String>,
    kind: AskResearchSourceKind,
    reason: AskResearchSelectionReason,
}

impl AskResearchSource {
    /// Creates a source reference; source text deliberately is not accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        id: AskResearchSourceId,
        ordinal: u32,
        revision: FileRevision,
        range: Option<SourceRange>,
        symbol: Option<String>,
        kind: AskResearchSourceKind,
        reason: AskResearchSelectionReason,
    ) -> Result<Self, AskResearchDataError> {
        if ordinal == 0
            || symbol
                .as_ref()
                .is_some_and(|value| !safe_text(value, MAX_SYMBOL_BYTES))
        {
            return Err(AskResearchDataError::InvalidSource);
        }
        Ok(Self {
            session_id,
            user_sequence,
            id,
            ordinal,
            revision,
            range,
            symbol,
            kind,
            reason,
        })
    }

    /// Returns the owning session.
    #[must_use]
    pub const fn session_id(&self) -> AgentSessionId {
        self.session_id
    }
    /// Returns the owning user-message sequence.
    #[must_use]
    pub const fn user_sequence(&self) -> AgentSessionSequence {
        self.user_sequence
    }
    /// Returns the opaque presentation identity.
    #[must_use]
    pub const fn id(&self) -> AskResearchSourceId {
        self.id
    }
    /// Returns the stable per-turn ordering position.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// Returns the current source revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }
    /// Returns the exact optional source span.
    #[must_use]
    pub const fn range(&self) -> Option<SourceRange> {
        self.range
    }
    /// Returns an optional display symbol.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }
    /// Returns the closed source kind.
    #[must_use]
    pub const fn kind(&self) -> AskResearchSourceKind {
        self.kind
    }
    /// Returns the closed selection reason.
    #[must_use]
    pub const fn reason(&self) -> AskResearchSelectionReason {
        self.reason
    }
}

/// One reconstructed turn with its complete bounded event trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchDetail {
    work_state: Option<a3_domain::ResearchWorkState>,
    turn: AskResearchTurn,
    events: Vec<AskResearchEvent>,
    cited_sources: Vec<AskResearchSourceId>,
}

impl AskResearchDetail {
    /// Validates one bounded ordered detail projection.
    pub fn new(
        turn: AskResearchTurn,
        events: Vec<AskResearchEvent>,
        cited_sources: Vec<AskResearchSourceId>,
    ) -> Result<Self, AskResearchDataError> {
        if events.is_empty()
            || events.len() > 64
            || events.iter().any(|event| {
                event.session_id() != turn.session_id()
                    || event.user_sequence() != turn.user_sequence()
            })
            || events
                .windows(2)
                .any(|pair| pair[0].sequence() >= pair[1].sequence())
            || cited_sources.len() > MAX_SOURCES_PER_TURN
        {
            return Err(AskResearchDataError::InvalidDetail);
        }
        Ok(Self {
            turn,
            events,
            cited_sources,
            work_state: None,
        })
    }

    /// Attaches the latest checkpoint from the same persisted turn/read snapshot.
    #[must_use]
    pub fn with_work_state(mut self, state: a3_domain::ResearchWorkState) -> Self {
        self.work_state = Some(state);
        self
    }

    /// Returns the stable checklist independently of the bounded event page.
    #[must_use]
    pub const fn work_state(&self) -> Option<&a3_domain::ResearchWorkState> {
        self.work_state.as_ref()
    }
    /// Returns the immutable index binding.
    #[must_use]
    pub const fn turn(&self) -> &AskResearchTurn {
        &self.turn
    }
    /// Returns events in ascending sequence order.
    #[must_use]
    pub fn events(&self) -> &[AskResearchEvent] {
        &self.events
    }
    /// Returns source identities explicitly cited by the answer.
    #[must_use]
    pub fn cited_sources(&self) -> &[AskResearchSourceId] {
        &self.cited_sources
    }
}

/// Bounded Ask turn list, newest user message first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchTurnPage(Vec<AskResearchDetail>);

impl AskResearchTurnPage {
    /// Creates a page of at most 32 turns.
    pub fn new(turns: Vec<AskResearchDetail>) -> Result<Self, AskResearchDataError> {
        if turns.len() > 32 {
            return Err(AskResearchDataError::InvalidDetail);
        }
        Ok(Self(turns))
    }
    /// Returns newest-first turn details.
    #[must_use]
    pub fn turns(&self) -> &[AskResearchDetail] {
        &self.0
    }
}

/// One bounded source page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchSourcePage {
    sources: Vec<AskResearchSource>,
    has_more: bool,
}

/// One coherent presentation read containing the exact trace revision and its first source page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchProjection {
    detail: AskResearchDetail,
    sources: AskResearchSourcePage,
    source_count: u16,
}

impl AskResearchProjection {
    /// Creates a projection only when every source belongs to the exact requested turn.
    pub fn new(
        detail: AskResearchDetail,
        sources: AskResearchSourcePage,
        source_count: u16,
    ) -> Result<Self, AskResearchDataError> {
        if sources.sources().iter().any(|source| {
            source.session_id() != detail.turn().session_id()
                || source.user_sequence() != detail.turn().user_sequence()
        }) || usize::from(source_count) < sources.sources().len()
            || sources.has_more() != (usize::from(source_count) > sources.sources().len())
        {
            return Err(AskResearchDataError::InvalidDetail);
        }
        Ok(Self {
            detail,
            sources,
            source_count,
        })
    }
    /// Returns the exact bounded event and citation projection.
    #[must_use]
    pub const fn detail(&self) -> &AskResearchDetail {
        &self.detail
    }
    /// Returns the first bounded source page from the same read snapshot.
    #[must_use]
    pub const fn sources(&self) -> &AskResearchSourcePage {
        &self.sources
    }
    /// Returns the total source count observed in the same read snapshot.
    #[must_use]
    pub const fn source_count(&self) -> u16 {
        self.source_count
    }
}

impl AskResearchSourcePage {
    /// Creates a page of at most 50 sources.
    pub fn new(
        sources: Vec<AskResearchSource>,
        has_more: bool,
    ) -> Result<Self, AskResearchDataError> {
        if sources.len() > 50 {
            return Err(AskResearchDataError::InvalidSource);
        }
        Ok(Self { sources, has_more })
    }
    /// Returns source metadata in ordinal order.
    #[must_use]
    pub fn sources(&self) -> &[AskResearchSource] {
        &self.sources
    }
    /// Returns whether another page exists.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// Object-safe future returned by Ask research persistence.
pub type AskResearchStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AskResearchStoreFailure>> + Send + 'a>>;

/// Project-local append-only persistence and atomic Ask completion boundary.
pub trait AskResearchStore: fmt::Debug + Send + Sync {
    /// Begins one unique index-bound Ask turn.
    fn begin_turn<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        turn: &'a AskResearchTurn,
        first_event: &'a AskResearchEvent,
    ) -> AskResearchStoreFuture<'a, ()>;
    /// Appends one immediate research event.
    fn append_event<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        event: &'a AskResearchEvent,
    ) -> AskResearchStoreFuture<'a, ()>;
    /// Appends newly discovered metadata-only source references.
    fn append_sources<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        sources: &'a [AskResearchSource],
    ) -> AskResearchStoreFuture<'a, ()>;
    /// Commits the answer, citations, and terminal research event in one transaction.
    #[allow(clippy::too_many_arguments)]
    fn complete_turn<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_session_revision: AgentSessionRevision,
        session: &'a AgentSession,
        answer: &'a AgentSessionEntry,
        event: &'a AskResearchEvent,
        cited_sources: &'a [AskResearchSourceId],
        diagrams: &'a [crate::EvidenceDiagramArtifact],
    ) -> AskResearchStoreFuture<'a, ()>;
    /// Links a previously completed Plan research section to the internal task created after the
    /// user explicitly accepts that plan. This is an internal persistence operation; identifiers
    /// are never accepted from or returned to the WebView.
    fn link_task_to_turn<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        task_id: TaskId,
    ) -> AskResearchStoreFuture<'a, ()>;
    /// Loads the internal task already linked to one research turn.
    ///
    /// This read supports crash-safe adoption when task materialization committed before the
    /// conversation session could record its presentation work item. The task identity remains
    /// behind the trusted application boundary and is never accepted from the WebView.
    fn load_linked_task<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> AskResearchStoreFuture<'a, Option<TaskId>>;
    /// Lists at most 32 recorded turns for a session.
    fn list_turns<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        limit: u16,
    ) -> AskResearchStoreFuture<'a, AskResearchTurnPage>;
    /// Loads one exact recorded turn.
    fn load_detail<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> AskResearchStoreFuture<'a, Option<AskResearchDetail>>;
    /// Loads detail, citations, and the first source page from one consistent storage snapshot.
    fn load_projection<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        source_limit: u16,
    ) -> AskResearchStoreFuture<'a, Option<AskResearchProjection>> {
        Box::pin(async move {
            let Some(detail) = self.load_detail(project, session_id, user_sequence).await? else {
                return Ok(None);
            };
            let sources = self
                .list_sources(project, session_id, user_sequence, None, source_limit)
                .await?;
            let mut source_count = sources.sources().len();
            let mut after = sources.sources().last().map(AskResearchSource::ordinal);
            let mut has_more = sources.has_more();
            while has_more {
                let page = self
                    .list_sources(project, session_id, user_sequence, after, 50)
                    .await?;
                source_count = source_count.saturating_add(page.sources().len());
                after = page.sources().last().map(AskResearchSource::ordinal);
                has_more = page.has_more();
                if after.is_none() || source_count > 200 {
                    return Err(AskResearchStoreFailure::InvalidStoredData);
                }
            }
            let source_count = u16::try_from(source_count)
                .map_err(|_| AskResearchStoreFailure::InvalidStoredData)?;
            AskResearchProjection::new(detail, sources, source_count)
                .map(Some)
                .map_err(|_| AskResearchStoreFailure::InvalidStoredData)
        })
    }
    /// Loads one bounded ordinal source page.
    fn list_sources<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        after_ordinal: Option<u32>,
        limit: u16,
    ) -> AskResearchStoreFuture<'a, AskResearchSourcePage>;
    /// Resolves one opaque source only within its owning project.
    fn load_source<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        source_id: AskResearchSourceId,
    ) -> AskResearchStoreFuture<'a, Option<AskResearchSource>>;
    /// Lists at most three diagrams atomically completed with one research turn.
    fn list_diagrams<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> AskResearchStoreFuture<'a, Vec<crate::EvidenceDiagramArtifact>>;
    /// Lists bounded diagram artifacts for the visible tail of one session.
    fn list_session_diagrams<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        user_turn_limit: u16,
    ) -> AskResearchStoreFuture<'a, Vec<SessionEvidenceDiagramArtifact>>;
    /// Loads one artifact only when it belongs to the supplied project and session.
    fn load_diagram<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        artifact_id: a3_domain::AgentDiagramArtifactId,
    ) -> AskResearchStoreFuture<'a, Option<(AgentSessionSequence, crate::EvidenceDiagramArtifact)>>;
    /// Reconstructs the current, typed research handoff linked to one internal Agent task.
    ///
    /// The task identity never crosses the WebView boundary. Missing or legacy trace data yields
    /// `None`; callers must still revalidate every returned anchor against the current index.
    fn load_handoff_for_task<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
    ) -> AskResearchStoreFuture<'a, Option<crate::ResearchHandoff>>;
}

/// One persisted diagram with the immutable index anchor of its owning research turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEvidenceDiagramArtifact {
    user_sequence: AgentSessionSequence,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    artifact: crate::EvidenceDiagramArtifact,
}

impl SessionEvidenceDiagramArtifact {
    /// Reconstructs a store-validated session diagram projection.
    #[must_use]
    pub const fn new(
        user_sequence: AgentSessionSequence,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        artifact: crate::EvidenceDiagramArtifact,
    ) -> Self {
        Self {
            user_sequence,
            index_run_id,
            snapshot_id,
            artifact,
        }
    }

    /// Returns the owning user-message sequence.
    #[must_use]
    pub const fn user_sequence(&self) -> AgentSessionSequence {
        self.user_sequence
    }

    /// Returns the immutable published index run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable repository snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the validated artifact.
    #[must_use]
    pub const fn artifact(&self) -> &crate::EvidenceDiagramArtifact {
        &self.artifact
    }
}

/// Stable Ask research persistence failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResearchStoreFailure {
    /// Input crossed a fixed application boundary.
    InvalidInput,
    /// Durable rows violate the closed projection.
    InvalidStoredData,
    /// Another writer advanced the append-only turn first.
    Conflict,
    /// Local storage is temporarily unavailable.
    Unavailable,
}

impl fmt::Display for AskResearchStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "Ask research request is invalid",
            Self::InvalidStoredData => "Ask research storage contains invalid data",
            Self::Conflict => "Ask research changed concurrently",
            Self::Unavailable => "Ask research storage is unavailable",
        })
    }
}
impl Error for AskResearchStoreFailure {}

/// Invalid bounded in-memory Ask research data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResearchDataError {
    /// Event order or bounded display text is invalid.
    InvalidEvent,
    /// Source metadata or ordering is invalid.
    InvalidSource,
    /// The reconstructed turn detail is inconsistent.
    InvalidDetail,
}
impl fmt::Display for AskResearchDataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ask research data is invalid")
    }
}
impl Error for AskResearchDataError {}

fn safe_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{ContentHash, RepositoryPath, SourcePosition};

    #[test]
    fn revalidated_note_sources_are_unique_without_weakening_bounds() -> Result<(), Box<dyn Error>>
    {
        let first = AskResearchSourceId::from_bytes([1; 32]);
        let second = AskResearchSourceId::from_bytes([2; 32]);
        let note = |sources| {
            AskResearchPublicNote::new(
                "Explain storage selection".to_owned(),
                AskResearchPublicFindingKind::Observation,
                "The configuration selects a storage implementation".to_owned(),
                sources,
                "Check the default".to_owned(),
                "Read the factory".to_owned(),
            )
        };
        assert_eq!(
            note(vec![second, first, second, first])?.source_ids(),
            &[second, first]
        );
        assert!(note(vec![first; 33]).is_err());
        assert!(note(Vec::new()).is_err());
        Ok(())
    }

    #[test]
    fn source_search_contract_enforces_literal_and_resource_bounds() -> Result<(), Box<dyn Error>> {
        assert!(AskSourceTextSearch::new(Vec::new()).is_err());
        assert!(AskSourceTextSearch::new(vec!["TODO".to_owned(); 9]).is_err());
        assert!(AskSourceTextSearch::new(vec!["TODO".to_owned()]).is_ok());

        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
        let hit = AskSourceTextHit::new(revision, range, "TODO".to_owned());

        assert!(
            AskSourceTextSearchResult::new(vec![hit; 101], 1, 1, AskResearchCompleteness::Limited,)
                .is_err()
        );
        assert!(
            AskSourceTextSearchResult::new(Vec::new(), 2_001, 1, AskResearchCompleteness::Limited,)
                .is_err()
        );
        assert!(
            AskSourceTextSearchResult::new(
                Vec::new(),
                1,
                32 * 1_024 * 1_024 + 1,
                AskResearchCompleteness::Limited,
            )
            .is_err()
        );
        Ok(())
    }
}
