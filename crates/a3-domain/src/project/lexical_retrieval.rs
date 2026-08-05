use super::{
    ExactSearchSymbol, ExactSearchTarget, FileRevision, IndexRunId, QualifiedSymbolName,
    SnapshotId, SourceChannel, SymbolId,
};
use std::error::Error;
use std::fmt;

const MAX_LEXICAL_QUERY_BYTES: usize = 4 * 1_024;
const MAX_LEXICAL_QUERY_TOKENS: usize = 32;
const MAX_LEXICAL_PAGE_SIZE: u16 = 100;
const MAX_LEXICAL_SCORE: u32 = 100_000;

/// Bounded text containing at least one searchable three-character token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LexicalSearchTerm(String);

impl LexicalSearchTerm {
    /// Validates a single-line query before it can reach FTS syntax generation.
    pub fn try_from_string(value: String) -> Result<Self, LexicalSearchTermError> {
        if value.is_empty() || value.len() > MAX_LEXICAL_QUERY_BYTES {
            return Err(LexicalSearchTermError::InvalidLength(value.len()));
        }
        if value.chars().any(char::is_control) {
            return Err(LexicalSearchTermError::InvalidCharacter);
        }
        let token_count = searchable_token_count(&value);
        if token_count == 0 {
            return Err(LexicalSearchTermError::MissingSearchableToken);
        }
        if token_count > MAX_LEXICAL_QUERY_TOKENS {
            return Err(LexicalSearchTermError::TooManySearchableTokens(token_count));
        }
        Ok(Self(value))
    }

    /// Returns the validated user text, never an FTS expression.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn searchable_token_count(value: &str) -> usize {
    let mut length = 0_u8;
    let mut count = 0_usize;
    for character in value.chars() {
        if character.is_alphanumeric() || character == '_' {
            length = length.saturating_add(1);
        } else {
            if length >= 3 {
                count = count.saturating_add(1);
            }
            length = 0;
        }
    }
    if length >= 3 {
        count = count.saturating_add(1);
    }
    count
}

/// Invalid lexical query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalSearchTermError {
    /// The query was empty or exceeded its fixed byte boundary.
    InvalidLength(usize),
    /// The query contained a control character.
    InvalidCharacter,
    /// No alphanumeric or underscore token contained at least three characters.
    MissingSearchableToken,
    /// The query contained too many independent searchable tokens.
    TooManySearchableTokens(usize),
}

impl fmt::Display for LexicalSearchTermError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "lexical-search query has invalid byte length {length}"
                )
            }
            Self::InvalidCharacter => {
                formatter.write_str("lexical-search query contains a control character")
            }
            Self::MissingSearchableToken => {
                formatter.write_str("lexical-search query has no searchable token")
            }
            Self::TooManySearchableTokens(count) => write!(
                formatter,
                "lexical-search query has {count} searchable tokens; at most {MAX_LEXICAL_QUERY_TOKENS} are allowed"
            ),
        }
    }
}

impl Error for LexicalSearchTermError {}

/// One typo-tolerant full-text retrieval request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LexicalSearchQuery {
    term: LexicalSearchTerm,
}

impl LexicalSearchQuery {
    /// Creates a query from text that has already crossed the domain boundary.
    #[must_use]
    pub const fn new(term: LexicalSearchTerm) -> Self {
        Self { term }
    }

    /// Returns the validated user text.
    #[must_use]
    pub const fn term(&self) -> &LexicalSearchTerm {
        &self.term
    }
}

/// Number of results returned by one lexical-search page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalSearchPageSize(u16);

impl LexicalSearchPageSize {
    /// Default page size for interactive lexical retrieval.
    pub const DEFAULT: Self = Self(20);

    /// Creates a positive page size capped at the product boundary.
    pub fn new(value: u16) -> Result<Self, LexicalSearchPageSizeError> {
        if value == 0 || value > MAX_LEXICAL_PAGE_SIZE {
            return Err(LexicalSearchPageSizeError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Lexical-search page size outside the supported product boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalSearchPageSizeError {
    value: u16,
}

impl fmt::Display for LexicalSearchPageSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "lexical-search page size {} must be between 1 and {MAX_LEXICAL_PAGE_SIZE}",
            self.value
        )
    }
}

impl Error for LexicalSearchPageSizeError {}

/// Deterministic weighted lexical relevance where a larger value ranks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LexicalScore(u32);

impl LexicalScore {
    /// Creates a positive score within the fixed version-one ranking range.
    pub fn new(value: u32) -> Result<Self, LexicalScoreError> {
        if value == 0 || value > MAX_LEXICAL_SCORE {
            return Err(LexicalScoreError { value });
        }
        Ok(Self(value))
    }

    /// Returns the stable integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Lexical score outside the versioned deterministic range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalScoreError {
    value: u32,
}

impl fmt::Display for LexicalScoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "lexical score {} must be between 1 and {MAX_LEXICAL_SCORE}",
            self.value
        )
    }
}

impl Error for LexicalScoreError {}

/// Highest-weight field explaining one lexical hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LexicalSearchExplanation {
    /// The normalized repository path supplied the strongest match.
    Path,
    /// The containment-derived qualified name supplied the strongest match.
    QualifiedName,
    /// The simple adapter-derived symbol name supplied the strongest match.
    SymbolName,
    /// The adapter-derived declaration signature supplied the strongest match.
    Signature,
}

/// Evidence target returned by lexical retrieval.
pub type LexicalSearchTarget = ExactSearchTarget;

/// Symbol projection returned by lexical retrieval.
pub type LexicalSearchSymbol = ExactSearchSymbol;

/// One deterministic lexical candidate with explicit provenance and score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSearchHit {
    target: LexicalSearchTarget,
    source_channel: SourceChannel,
    explanation: LexicalSearchExplanation,
    score: LexicalScore,
}

impl LexicalSearchHit {
    /// Creates a path-derived file hit.
    #[must_use]
    pub const fn file(revision: FileRevision, score: LexicalScore) -> Self {
        Self {
            target: LexicalSearchTarget::File(revision),
            source_channel: SourceChannel::Lexical,
            explanation: LexicalSearchExplanation::Path,
            score,
        }
    }

    /// Creates a symbol hit explained by one of its projected fields.
    #[must_use]
    pub const fn symbol(
        symbol: LexicalSearchSymbol,
        explanation: LexicalSearchExplanation,
        score: LexicalScore,
    ) -> Self {
        Self {
            target: LexicalSearchTarget::Symbol(symbol),
            source_channel: SourceChannel::Lexical,
            explanation,
            score,
        }
    }

    /// Returns the current evidence-bearing file or symbol target.
    #[must_use]
    pub const fn target(&self) -> &LexicalSearchTarget {
        &self.target
    }

    /// Returns the lexical source channel.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        self.source_channel
    }

    /// Returns the strongest weighted field explaining this hit.
    #[must_use]
    pub const fn explanation(&self) -> LexicalSearchExplanation {
        self.explanation
    }

    /// Returns the deterministic final relevance score.
    #[must_use]
    pub const fn score(&self) -> LexicalScore {
        self.score
    }
}

/// Final stable sort key used by lexical keyset pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexicalSearchPosition {
    /// Last file hit ordered by descending score and canonical path bytes.
    File {
        /// Final deterministic relevance.
        score: LexicalScore,
        /// Canonical repository-relative path.
        path: super::RepositoryPath,
    },
    /// Last symbol hit with all deterministic tie-breakers.
    Symbol {
        /// Final deterministic relevance.
        score: LexicalScore,
        /// Canonical repository-relative path.
        path: super::RepositoryPath,
        /// Containment-derived qualified name.
        qualified_name: QualifiedSymbolName,
        /// Content- and adapter-bound final tie-breaker.
        symbol_id: SymbolId,
    },
}

/// Snapshot-bound continuation for deterministic lexical candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSearchCursor {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: LexicalSearchQuery,
    position: LexicalSearchPosition,
}

impl LexicalSearchCursor {
    /// Creates a continuation from an adapter-validated final page hit.
    #[must_use]
    pub const fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        query: LexicalSearchQuery,
        position: LexicalSearchPosition,
    ) -> Self {
        Self {
            index_run_id,
            snapshot_id,
            query,
            position,
        }
    }

    /// Returns the published run searched by the preceding page.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot searched by the preceding page.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact lexical query bound to this continuation.
    #[must_use]
    pub const fn query(&self) -> &LexicalSearchQuery {
        &self.query
    }

    /// Returns the last full stable ordering key.
    #[must_use]
    pub const fn position(&self) -> &LexicalSearchPosition {
        &self.position
    }
}

/// One stable lexical page from exactly one published snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSearchPage {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    hits: Vec<LexicalSearchHit>,
    next_cursor: Option<LexicalSearchCursor>,
}

impl LexicalSearchPage {
    /// Creates a bounded page and verifies its continuation against the final hit.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        hits: Vec<LexicalSearchHit>,
        next_cursor: Option<LexicalSearchCursor>,
        page_size: LexicalSearchPageSize,
    ) -> Result<Self, LexicalSearchPageError> {
        if hits.len() > usize::from(page_size.get()) {
            return Err(LexicalSearchPageError::TooManyHits);
        }
        if next_cursor.as_ref().is_some_and(|cursor| {
            cursor.index_run_id() != index_run_id || cursor.snapshot_id() != snapshot_id
        }) {
            return Err(LexicalSearchPageError::CursorPublicationMismatch);
        }
        if next_cursor.as_ref().is_some_and(|cursor| {
            hits.last()
                .is_none_or(|hit| !hit_matches_position(hit, cursor.position()))
        }) {
            return Err(LexicalSearchPageError::CursorPositionMismatch);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            hits,
            next_cursor,
        })
    }

    /// Returns the atomically published run searched by this page.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot searched by this page.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns deterministically ordered lexical hits.
    #[must_use]
    pub fn hits(&self) -> &[LexicalSearchHit] {
        &self.hits
    }

    /// Returns a snapshot-bound continuation when another bounded candidate exists.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&LexicalSearchCursor> {
        self.next_cursor.as_ref()
    }
}

fn hit_matches_position(hit: &LexicalSearchHit, position: &LexicalSearchPosition) -> bool {
    match (hit.target(), position) {
        (LexicalSearchTarget::File(revision), LexicalSearchPosition::File { score, path }) => {
            hit.score() == *score && revision.path() == path
        }
        (
            LexicalSearchTarget::Symbol(symbol),
            LexicalSearchPosition::Symbol {
                score,
                path,
                qualified_name,
                symbol_id,
            },
        ) => {
            hit.score() == *score
                && symbol.symbol().revision().path() == path
                && symbol.qualified_name() == qualified_name
                && symbol.symbol().id() == *symbol_id
        }
        _ => false,
    }
}

/// Invalid adapter-produced lexical page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalSearchPageError {
    /// The adapter returned more hits than requested.
    TooManyHits,
    /// The continuation references another run or snapshot.
    CursorPublicationMismatch,
    /// The continuation does not describe the final hit in the page.
    CursorPositionMismatch,
}

impl fmt::Display for LexicalSearchPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyHits => {
                formatter.write_str("lexical-search page exceeds its requested size")
            }
            Self::CursorPublicationMismatch => {
                formatter.write_str("lexical-search cursor belongs to a different publication")
            }
            Self::CursorPositionMismatch => {
                formatter.write_str("lexical-search cursor does not describe the final page hit")
            }
        }
    }
}

impl Error for LexicalSearchPageError {}

#[cfg(test)]
mod tests {
    use super::{LexicalScore, LexicalSearchTerm, LexicalSearchTermError};

    #[test]
    fn lexical_query_requires_a_bounded_searchable_token() {
        assert_eq!(
            LexicalSearchTerm::try_from_string("ab --".to_owned()),
            Err(LexicalSearchTermError::MissingSearchableToken)
        );
        assert!(LexicalSearchTerm::try_from_string("launch' OR 1=1".to_owned()).is_ok());
        assert!(LexicalSearchTerm::try_from_string("line\nfeed".to_owned()).is_err());
        assert!(LexicalSearchTerm::try_from_string(vec!["token"; 33].join(" ")).is_err());
    }

    #[test]
    fn lexical_score_is_positive_and_bounded() {
        assert!(LexicalScore::new(1).is_ok());
        assert!(LexicalScore::new(0).is_err());
        assert!(LexicalScore::new(100_001).is_err());
    }
}
