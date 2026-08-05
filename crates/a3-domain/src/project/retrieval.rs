use super::{
    FileRevision, GraphSymbol, IndexRunId, RepositoryPath, SnapshotId, SymbolId, SymbolRole,
};
use std::error::Error;
use std::fmt;

const MAX_SEARCH_TEXT_BYTES: usize = 16 * 1_024;
const MAX_EXACT_SEARCH_PAGE_SIZE: u16 = 100;

/// Bounded user-supplied identifier or signature text for exact retrieval.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExactSearchTerm(String);

impl ExactSearchTerm {
    /// Validates non-empty text while retaining signature layout whitespace.
    pub fn try_from_string(value: String) -> Result<Self, ExactSearchTextError> {
        validate_search_text(&value, true)?;
        Ok(Self(value))
    }

    /// Returns the exact text used by the deterministic search projection.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Bounded, adapter-derived qualified symbol name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QualifiedSymbolName(String);

impl QualifiedSymbolName {
    /// Validates a stored single-line qualified name.
    pub fn try_from_string(value: String) -> Result<Self, ExactSearchTextError> {
        validate_search_text(&value, false)?;
        Ok(Self(value))
    }

    /// Returns the deterministic qualified name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_search_text(value: &str, retain_layout: bool) -> Result<(), ExactSearchTextError> {
    if value.is_empty() || value.len() > MAX_SEARCH_TEXT_BYTES {
        return Err(ExactSearchTextError::InvalidLength(value.len()));
    }
    if value.chars().any(|character| {
        character == '\0'
            || (character.is_control()
                && !(retain_layout && matches!(character, '\n' | '\r' | '\t')))
    }) {
        return Err(ExactSearchTextError::InvalidCharacter);
    }
    Ok(())
}

/// Invalid bounded retrieval text supplied by a caller or reconstructed from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactSearchTextError {
    /// The value was empty or exceeded the fixed byte boundary.
    InvalidLength(usize),
    /// The value contained NUL or an unsupported control character.
    InvalidCharacter,
}

impl fmt::Display for ExactSearchTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "exact-search text has invalid byte length {length}"
                )
            }
            Self::InvalidCharacter => {
                formatter.write_str("exact-search text contains an invalid character")
            }
        }
    }
}

impl Error for ExactSearchTextError {}

/// Structural role selected without fuzzy or semantic inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExactSearchRole {
    /// Repository manifest or dependency declaration file.
    Manifest,
    /// Syntactically identified executable, library, or script entrypoint.
    Entrypoint,
    /// Syntactically identified test symbol.
    Test,
}

/// One bounded deterministic retrieval request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExactSearchQuery {
    /// Exact normalized, repository-relative path bytes.
    Path(RepositoryPath),
    /// Exact and prefix matching across qualified name, simple name, and signature.
    Symbol(ExactSearchTerm),
    /// Deterministic structural-role lookup.
    Role(ExactSearchRole),
}

/// Number of results returned by one exact-search page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactSearchPageSize(u16);

impl ExactSearchPageSize {
    /// Default page size for interactive retrieval.
    pub const DEFAULT: Self = Self(20);

    /// Creates a positive page size capped at the product boundary.
    pub fn new(value: u16) -> Result<Self, ExactSearchPageSizeError> {
        if value == 0 || value > MAX_EXACT_SEARCH_PAGE_SIZE {
            return Err(ExactSearchPageSizeError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Exact-search page size outside the supported product boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactSearchPageSizeError {
    value: u16,
}

impl fmt::Display for ExactSearchPageSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "exact-search page size {} must be between 1 and {MAX_EXACT_SEARCH_PAGE_SIZE}",
            self.value
        )
    }
}

impl Error for ExactSearchPageSizeError {}

/// Retrieval channel that produced a candidate before future fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceChannel {
    /// Normalized path, identifier, signature, or structural-role projection.
    Exact,
    /// Full-text lexical retrieval.
    Lexical,
    /// Evidence-graph relationship expansion.
    Graph,
    /// Test relationship expansion.
    Test,
    /// Fresh evidence-grounded task memory.
    Memory,
    /// Semantic similarity used only for candidate generation.
    Semantic,
}

/// Machine-readable reason why an exact-search hit matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExactSearchExplanation {
    /// The query equalled canonical repository-relative path bytes.
    NormalizedPathExact,
    /// The query equalled a deterministic qualified symbol name.
    QualifiedNameExact,
    /// The query equalled the adapter-derived simple symbol name.
    SymbolNameExact,
    /// The query equalled the adapter-derived declaration signature.
    SignatureExact,
    /// The deterministic qualified name starts with the query.
    QualifiedNamePrefix,
    /// The simple symbol name starts with the query.
    SymbolNamePrefix,
    /// The declaration signature starts with the query.
    SignaturePrefix,
    /// The file is classified as a repository manifest.
    ManifestRole,
    /// The symbol carries the syntactic entrypoint role.
    EntrypointRole,
    /// The symbol carries the syntactic test role.
    TestRole,
}

impl ExactSearchExplanation {
    /// Returns the stable ordering key used before path and identity tie-breakers.
    #[must_use]
    pub const fn sort_order(self) -> u8 {
        match self {
            Self::NormalizedPathExact | Self::QualifiedNameExact => 0,
            Self::SymbolNameExact => 1,
            Self::SignatureExact => 2,
            Self::QualifiedNamePrefix => 3,
            Self::SymbolNamePrefix => 4,
            Self::SignaturePrefix => 5,
            Self::ManifestRole | Self::EntrypointRole | Self::TestRole => 0,
        }
    }
}

/// Search projection of one symbol and the exact source revision that supplied it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSearchSymbol {
    symbol: GraphSymbol,
    qualified_name: QualifiedSymbolName,
}

impl ExactSearchSymbol {
    /// Binds a graph symbol to its deterministic containment-derived name.
    #[must_use]
    pub const fn new(symbol: GraphSymbol, qualified_name: QualifiedSymbolName) -> Self {
        Self {
            symbol,
            qualified_name,
        }
    }

    /// Returns the content- and adapter-bound symbol projection.
    #[must_use]
    pub const fn symbol(&self) -> &GraphSymbol {
        &self.symbol
    }

    /// Returns the deterministic qualified name used for matching and ordering.
    #[must_use]
    pub const fn qualified_name(&self) -> &QualifiedSymbolName {
        &self.qualified_name
    }
}

/// Exact, evidence-bearing target returned by deterministic retrieval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactSearchTarget {
    /// Current revision of one normalized repository path.
    File(FileRevision),
    /// Current revision and structural projection of one symbol.
    Symbol(ExactSearchSymbol),
}

impl ExactSearchTarget {
    /// Returns the exact current file revision supplying this target.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        match self {
            Self::File(revision) => revision,
            Self::Symbol(symbol) => symbol.symbol().revision(),
        }
    }
}

/// One deterministic retrieval hit with an explicit source channel and explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSearchHit {
    target: ExactSearchTarget,
    source_channel: SourceChannel,
    explanation: ExactSearchExplanation,
}

impl ExactSearchHit {
    /// Creates a path or manifest-file hit when the explanation matches the target class.
    pub fn file(
        revision: FileRevision,
        explanation: ExactSearchExplanation,
    ) -> Result<Self, ExactSearchHitError> {
        if !matches!(
            explanation,
            ExactSearchExplanation::NormalizedPathExact | ExactSearchExplanation::ManifestRole
        ) {
            return Err(ExactSearchHitError::TargetExplanationMismatch);
        }
        Ok(Self {
            target: ExactSearchTarget::File(revision),
            source_channel: SourceChannel::Exact,
            explanation,
        })
    }

    /// Creates a symbol hit after validating any role-specific explanation.
    pub fn symbol(
        symbol: ExactSearchSymbol,
        explanation: ExactSearchExplanation,
    ) -> Result<Self, ExactSearchHitError> {
        if matches!(
            explanation,
            ExactSearchExplanation::NormalizedPathExact | ExactSearchExplanation::ManifestRole
        ) {
            return Err(ExactSearchHitError::TargetExplanationMismatch);
        }
        let roles = symbol.symbol().parsed().roles();
        if (explanation == ExactSearchExplanation::EntrypointRole
            && !roles.contains(SymbolRole::Entrypoint))
            || (explanation == ExactSearchExplanation::TestRole
                && !roles.contains(SymbolRole::Test))
        {
            return Err(ExactSearchHitError::MissingSymbolRole);
        }
        Ok(Self {
            target: ExactSearchTarget::Symbol(symbol),
            source_channel: SourceChannel::Exact,
            explanation,
        })
    }

    /// Returns the exact evidence-bearing target.
    #[must_use]
    pub const fn target(&self) -> &ExactSearchTarget {
        &self.target
    }

    /// Returns the retrieval channel; R1 exact hits always use `Exact`.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        self.source_channel
    }

    /// Returns the deterministic match explanation.
    #[must_use]
    pub const fn explanation(&self) -> ExactSearchExplanation {
        self.explanation
    }
}

/// Invalid relationship between an exact-search target and its explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactSearchHitError {
    /// A file-only explanation was paired with a symbol or vice versa.
    TargetExplanationMismatch,
    /// A role explanation was paired with a symbol that does not carry that role.
    MissingSymbolRole,
}

impl fmt::Display for ExactSearchHitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetExplanationMismatch => {
                formatter.write_str("exact-search target and explanation do not match")
            }
            Self::MissingSymbolRole => {
                formatter.write_str("exact-search symbol does not carry the explained role")
            }
        }
    }
}

impl Error for ExactSearchHitError {}

/// Last stable sort key used by keyset pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactSearchPosition {
    /// Last canonical file path returned by a file query.
    File(RepositoryPath),
    /// Last complete symbol ordering key returned by a symbol query.
    Symbol {
        /// Match class ordered before all lexical tie-breakers.
        explanation: ExactSearchExplanation,
        /// Canonical source path bytes.
        path: RepositoryPath,
        /// Deterministic containment-derived qualified name.
        qualified_name: QualifiedSymbolName,
        /// Content- and adapter-bound final tie-breaker.
        symbol_id: SymbolId,
    },
}

/// Snapshot-bound continuation cursor for deterministic keyset pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSearchCursor {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: ExactSearchQuery,
    position: ExactSearchPosition,
}

impl ExactSearchCursor {
    /// Creates a cursor only when its position belongs to the query result class.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        query: ExactSearchQuery,
        position: ExactSearchPosition,
    ) -> Result<Self, ExactSearchCursorError> {
        let valid = matches!(
            (&query, &position),
            (
                ExactSearchQuery::Role(ExactSearchRole::Manifest),
                ExactSearchPosition::File(_)
            ) | (
                ExactSearchQuery::Symbol(_),
                ExactSearchPosition::Symbol { .. }
            ) | (
                ExactSearchQuery::Role(ExactSearchRole::Entrypoint | ExactSearchRole::Test),
                ExactSearchPosition::Symbol { .. }
            )
        );
        if !valid {
            return Err(ExactSearchCursorError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            query,
            position,
        })
    }

    /// Returns the exact published index run that produced the preceding page.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot searched by the preceding page.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the request to which this continuation is bound.
    #[must_use]
    pub const fn query(&self) -> &ExactSearchQuery {
        &self.query
    }

    /// Returns the last stable sort key from the preceding page.
    #[must_use]
    pub const fn position(&self) -> &ExactSearchPosition {
        &self.position
    }
}

/// A continuation position did not belong to its exact-search query class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactSearchCursorError;

impl fmt::Display for ExactSearchCursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("exact-search cursor position does not match its query")
    }
}

impl Error for ExactSearchCursorError {}

/// One stable page from exactly one atomically published index snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSearchPage {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    hits: Vec<ExactSearchHit>,
    next_cursor: Option<ExactSearchCursor>,
}

impl ExactSearchPage {
    /// Creates a bounded page after an adapter has read one coherent publication.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        hits: Vec<ExactSearchHit>,
        next_cursor: Option<ExactSearchCursor>,
        page_size: ExactSearchPageSize,
    ) -> Result<Self, ExactSearchPageError> {
        if hits.len() > usize::from(page_size.get()) {
            return Err(ExactSearchPageError::TooManyHits);
        }
        if next_cursor.as_ref().is_some_and(|cursor| {
            cursor.index_run_id() != index_run_id || cursor.snapshot_id() != snapshot_id
        }) {
            return Err(ExactSearchPageError::CursorPublicationMismatch);
        }
        if next_cursor.is_some() && hits.is_empty() {
            return Err(ExactSearchPageError::CursorWithoutHits);
        }
        if next_cursor.as_ref().is_some_and(|cursor| {
            hits.last()
                .is_none_or(|hit| !hit_matches_position(hit, cursor.position()))
        }) {
            return Err(ExactSearchPageError::CursorPositionMismatch);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            hits,
            next_cursor,
        })
    }

    /// Returns the atomically published index run searched by this page.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot searched by this page.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns deterministically ordered exact hits.
    #[must_use]
    pub fn hits(&self) -> &[ExactSearchHit] {
        &self.hits
    }

    /// Returns a snapshot-bound continuation when another result exists.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&ExactSearchCursor> {
        self.next_cursor.as_ref()
    }
}

/// Invalid adapter-produced exact-search page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactSearchPageError {
    /// The adapter returned more hits than the requested product boundary.
    TooManyHits,
    /// The continuation referenced a different run or snapshot.
    CursorPublicationMismatch,
    /// A continuation was returned without a preceding result key.
    CursorWithoutHits,
    /// The continuation key was not the final hit in this page.
    CursorPositionMismatch,
}

impl fmt::Display for ExactSearchPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyHits => {
                formatter.write_str("exact-search page exceeds its requested size")
            }
            Self::CursorPublicationMismatch => {
                formatter.write_str("exact-search cursor belongs to a different publication")
            }
            Self::CursorWithoutHits => {
                formatter.write_str("exact-search page has a cursor but no preceding hit")
            }
            Self::CursorPositionMismatch => {
                formatter.write_str("exact-search cursor does not describe the final page hit")
            }
        }
    }
}

impl Error for ExactSearchPageError {}

fn hit_matches_position(hit: &ExactSearchHit, position: &ExactSearchPosition) -> bool {
    match (hit.target(), position) {
        (ExactSearchTarget::File(revision), ExactSearchPosition::File(path)) => {
            revision.path() == path
        }
        (
            ExactSearchTarget::Symbol(symbol),
            ExactSearchPosition::Symbol {
                explanation,
                path,
                qualified_name,
                symbol_id,
            },
        ) => {
            hit.explanation() == *explanation
                && symbol.symbol().revision().path() == path
                && symbol.qualified_name() == qualified_name
                && symbol.symbol().id() == *symbol_id
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExactSearchCursor, ExactSearchCursorError, ExactSearchExplanation, ExactSearchHit,
        ExactSearchHitError, ExactSearchPosition, ExactSearchQuery, ExactSearchRole,
        ExactSearchTerm, QualifiedSymbolName, SourceChannel,
    };
    use crate::{
        ContentHash, FileRevision, IndexRunId, LocalSymbolId, ParsedSymbol, RepositoryPath,
        SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRole,
    };

    #[test]
    fn exact_hit_requires_the_explained_role() -> Result<(), Box<dyn std::error::Error>> {
        let symbol = search_symbol(false)?;
        assert_eq!(
            ExactSearchHit::symbol(symbol, ExactSearchExplanation::TestRole),
            Err(ExactSearchHitError::MissingSymbolRole)
        );
        let hit = ExactSearchHit::symbol(search_symbol(true)?, ExactSearchExplanation::TestRole)?;
        assert_eq!(hit.source_channel(), SourceChannel::Exact);
        assert_eq!(hit.explanation(), ExactSearchExplanation::TestRole);
        Ok(())
    }

    #[test]
    fn cursor_is_bound_to_the_query_result_class() -> Result<(), Box<dyn std::error::Error>> {
        let query = ExactSearchQuery::Path(path()?);
        let result = ExactSearchCursor::new(
            IndexRunId::from_bytes([4; 32]),
            SnapshotId::from_bytes([5; 32]),
            query,
            ExactSearchPosition::File(path()?),
        );
        assert_eq!(result, Err(ExactSearchCursorError));

        let query = ExactSearchQuery::Role(ExactSearchRole::Manifest);
        let cursor = ExactSearchCursor::new(
            IndexRunId::from_bytes([4; 32]),
            SnapshotId::from_bytes([5; 32]),
            query.clone(),
            ExactSearchPosition::File(path()?),
        )?;
        assert_eq!(cursor.query(), &query);
        Ok(())
    }

    #[test]
    fn search_text_is_bounded_and_rejects_controls() {
        assert!(ExactSearchTerm::try_from_string(String::new()).is_err());
        assert!(ExactSearchTerm::try_from_string("name\u{7}".to_owned()).is_err());
        assert!(ExactSearchTerm::try_from_string("fn name(\n)".to_owned()).is_ok());
        assert!(QualifiedSymbolName::try_from_string("module\nname".to_owned()).is_err());
    }

    fn search_symbol(
        is_test: bool,
    ) -> Result<super::ExactSearchSymbol, Box<dyn std::error::Error>> {
        let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
        let mut parsed = ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("test".to_owned())?,
            range,
            range,
        )?;
        if is_test {
            parsed = parsed.with_role(SymbolRole::Test);
        }
        Ok(super::ExactSearchSymbol::new(
            crate::GraphSymbol::new(
                SymbolId::from_bytes([3; 32]),
                FileRevision::new(path()?, ContentHash::from_bytes([2; 32])),
                parsed,
            ),
            QualifiedSymbolName::try_from_string("module::test".to_owned())?,
        ))
    }

    fn path() -> Result<RepositoryPath, Box<dyn std::error::Error>> {
        Ok(RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?)
    }
}
