use a3_domain::{
    AgentSession, AgentSessionEntry, AgentSessionId, AgentSessionRevision, AgentSessionSequence,
    AgentSessionTimestamp, AskResearchCompleteness, AskResearchPhase, AskResearchSelectionReason,
    AskResearchSourceId, AskResearchSourceKind, AskResearchState, FileRevision, IndexRunId,
    ProjectIdentity, SnapshotId, SourceRange,
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
        Self {
            session_id,
            user_sequence,
            index_run_id,
            snapshot_id,
            started_at,
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
}

/// One append-only, content-free live event in an Ask research turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResearchEvent {
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    sequence: u32,
    phase: AskResearchPhase,
    state: AskResearchState,
    action: String,
    query: Option<String>,
    completeness: AskResearchCompleteness,
    occurred_at: AgentSessionTimestamp,
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
        if sequence == 0 || !safe_text(&action, MAX_ACTION_BYTES) {
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
        })
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
    ) -> AskResearchStoreFuture<'a, ()>;
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
