use crate::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{
    CandidateFreshness, CandidateTokenCost, ExactSearchPageSize, ExactSearchQuery,
    ExactSearchTarget, ExactSearchTerm, ExactSearchTextError, FusedRetrievalResult, FusionError,
    FusionPolicy, FusionResultLimit, LexicalSearchPageSize, LexicalSearchQuery, LexicalSearchTerm,
    LexicalSearchTermError, ModuleId, NormalizedRetrievalSignal, ProjectIdentity,
    RetrievalCandidateSet, RetrievalCandidateSetError, RetrievalCandidateSets,
    RetrievalCandidateSetsError, RetrievalCandidateSignals,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const SEARCH_CHANNEL_LIMIT: u16 = 100;

/// One bounded user query executed across deterministic exact and lexical projections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapSearchQuery {
    exact: ExactSearchQuery,
    lexical: LexicalSearchQuery,
}

impl ProjectMapSearchQuery {
    /// Normalizes surrounding whitespace and rejects text outside both search contracts.
    pub fn try_from_string(value: String) -> Result<Self, ProjectMapSearchQueryError> {
        let normalized = value.trim().to_owned();
        let exact = ExactSearchTerm::try_from_string(normalized.clone())
            .map_err(ProjectMapSearchQueryError::Exact)?;
        let lexical = LexicalSearchTerm::try_from_string(normalized)
            .map_err(ProjectMapSearchQueryError::Lexical)?;
        Ok(Self {
            exact: ExactSearchQuery::Symbol(exact),
            lexical: LexicalSearchQuery::new(lexical),
        })
    }

    /// Returns the exact identifier/signature request used as the priority channel.
    #[must_use]
    pub const fn exact(&self) -> &ExactSearchQuery {
        &self.exact
    }

    /// Returns the typo-tolerant full-text request used as the evidence channel.
    #[must_use]
    pub const fn lexical(&self) -> &LexicalSearchQuery {
        &self.lexical
    }
}

/// Search text was not valid for the deterministic hybrid query boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSearchQueryError {
    /// Exact identifier/signature validation failed.
    Exact(ExactSearchTextError),
    /// Lexical validation failed or no searchable token remained.
    Lexical(LexicalSearchTermError),
}

impl fmt::Display for ProjectMapSearchQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(source) => write!(formatter, "invalid exact Project Map query: {source}"),
            Self::Lexical(source) => {
                write!(formatter, "invalid lexical Project Map query: {source}")
            }
        }
    }
}

impl Error for ProjectMapSearchQueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exact(source) => Some(source),
            Self::Lexical(source) => Some(source),
        }
    }
}

/// Inbound read-only use case for bounded, evidence-bearing Project Map search.
#[derive(Debug)]
pub struct SearchProjectMap {
    store: Arc<dyn KnowledgeSearchStore>,
}

/// One fused current search result plus optional evidence-proven primary-module bindings.
#[derive(Debug)]
pub struct ProjectMapSearchResult {
    retrieval: FusedRetrievalResult,
    module_bindings: Vec<Option<ModuleId>>,
}

impl ProjectMapSearchResult {
    /// Returns the fused result used for deterministic ranking and visible provenance.
    #[must_use]
    pub const fn retrieval(&self) -> &FusedRetrievalResult {
        &self.retrieval
    }

    /// Returns the optional unique primary-module binding for one zero-based ranked hit.
    #[must_use]
    pub fn module_binding(&self, index: usize) -> Option<ModuleId> {
        self.module_bindings.get(index).copied().flatten()
    }
}

impl std::ops::Deref for ProjectMapSearchResult {
    type Target = FusedRetrievalResult;

    fn deref(&self) -> &Self::Target {
        &self.retrieval
    }
}

impl SearchProjectMap {
    /// Wires the existing deterministic search port without adding file or database capabilities.
    #[must_use]
    pub fn new(store: Arc<dyn KnowledgeSearchStore>) -> Self {
        Self { store }
    }

    /// Runs exact then lexical retrieval and fuses at most twenty current targets.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapSearchQuery,
        control: &dyn KnowledgeSearchControl,
    ) -> Result<ProjectMapSearchResult, SearchProjectMapFailure> {
        if control.is_cancelled() {
            return Err(SearchProjectMapFailure::Cancelled);
        }
        let exact = self
            .store
            .search_exact(
                project,
                query.exact(),
                ExactSearchPageSize::new(SEARCH_CHANNEL_LIMIT)
                    .map_err(|_| SearchProjectMapFailure::ResourceLimit)?,
                None,
                control,
            )
            .await
            .map_err(SearchProjectMapFailure::Search)?;
        if control.is_cancelled() {
            return Err(SearchProjectMapFailure::Cancelled);
        }
        let lexical = self
            .store
            .search_lexical(
                project,
                query.lexical(),
                LexicalSearchPageSize::new(SEARCH_CHANNEL_LIMIT)
                    .map_err(|_| SearchProjectMapFailure::ResourceLimit)?,
                None,
                control,
            )
            .await
            .map_err(SearchProjectMapFailure::Search)?;
        if control.is_cancelled() {
            return Err(SearchProjectMapFailure::Cancelled);
        }

        let exact_signals = exact
            .hits()
            .iter()
            .map(|hit| search_signals(hit.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let lexical_signals = lexical
            .hits()
            .iter()
            .map(|hit| search_signals(hit.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let exact = RetrievalCandidateSet::from_exact_page(&exact, &exact_signals)?;
        let lexical = RetrievalCandidateSet::from_lexical_page(&lexical, &lexical_signals)?;
        let publication = RetrievalCandidateSets::new(
            exact.index_run_id(),
            exact.snapshot_id(),
            vec![exact, lexical],
        )?;
        let retrieval = FusionPolicy::v1()
            .fuse(publication, FusionResultLimit::DEFAULT)
            .map_err(SearchProjectMapFailure::Fusion)?;
        checkpoint(control)?;
        let targets = retrieval
            .hits()
            .iter()
            .map(|hit| hit.target().clone())
            .collect::<Vec<_>>();
        let module_bindings = self
            .store
            .bind_modules(project, retrieval.index_run_id(), &targets, control)
            .await
            .map_err(SearchProjectMapFailure::Search)?;
        checkpoint(control)?;
        if module_bindings.len() != targets.len() {
            return Err(SearchProjectMapFailure::InvalidModuleBindings);
        }
        Ok(ProjectMapSearchResult {
            retrieval,
            module_bindings,
        })
    }
}

fn checkpoint(control: &dyn KnowledgeSearchControl) -> Result<(), SearchProjectMapFailure> {
    if control.is_cancelled() {
        Err(SearchProjectMapFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn search_signals(
    target: &ExactSearchTarget,
) -> Result<RetrievalCandidateSignals, SearchProjectMapFailure> {
    Ok(RetrievalCandidateSignals::new(
        NormalizedRetrievalSignal::ZERO,
        NormalizedRetrievalSignal::ZERO,
        CandidateFreshness::Current,
        CandidateTokenCost::new(target_token_cost(target))
            .map_err(|_| SearchProjectMapFailure::ResourceLimit)?,
        NormalizedRetrievalSignal::ZERO,
    ))
}

fn target_token_cost(target: &ExactSearchTarget) -> u32 {
    let bytes = match target {
        ExactSearchTarget::File(revision) => revision.path().as_bytes().len(),
        ExactSearchTarget::Symbol(symbol) => {
            let parsed = symbol.symbol().parsed();
            let span_bytes = match usize::try_from(
                parsed
                    .declaration_range()
                    .end_byte()
                    .saturating_sub(parsed.declaration_range().start_byte()),
            ) {
                Ok(value) => value,
                Err(_) => usize::MAX,
            };
            symbol
                .symbol()
                .revision()
                .path()
                .as_bytes()
                .len()
                .saturating_add(parsed.name().as_str().len())
                .saturating_add(symbol.qualified_name().as_str().len())
                .saturating_add(parsed.signature().map_or(0, |value| value.as_str().len()))
                .saturating_add(span_bytes)
        }
    };
    match u32::try_from(bytes.saturating_add(96)) {
        Ok(value) => value.clamp(1, 65_535),
        Err(_) => 65_535,
    }
}

/// Stable failure classification for complete Project Map search orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProjectMapFailure {
    /// One deterministic storage-backed search channel failed.
    Search(KnowledgeSearchFailure),
    /// The caller cancelled before or between bounded channel reads.
    Cancelled,
    /// A channel returned invalid cardinality or mixed provenance.
    InvalidCandidateSet(RetrievalCandidateSetError),
    /// Exact and lexical results did not belong to one atomically published view.
    InvalidPublication(RetrievalCandidateSetsError),
    /// Versioned fusion rejected invalid inputs.
    Fusion(FusionError),
    /// A fixed size or token-cost bound could not be represented.
    ResourceLimit,
    /// The adapter returned a binding vector that did not match the ranked targets.
    InvalidModuleBindings,
}

impl fmt::Display for SearchProjectMapFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Search(source) => write!(formatter, "Project Map search failed: {source}"),
            Self::Cancelled => formatter.write_str("Project Map search was cancelled"),
            Self::InvalidCandidateSet(source) => {
                write!(formatter, "Project Map candidate set is invalid: {source}")
            }
            Self::InvalidPublication(source) => {
                write!(
                    formatter,
                    "Project Map publication is inconsistent: {source}"
                )
            }
            Self::Fusion(source) => write!(formatter, "Project Map fusion failed: {source}"),
            Self::ResourceLimit => formatter.write_str("Project Map search exceeded a fixed bound"),
            Self::InvalidModuleBindings => {
                formatter.write_str("Project Map module bindings are inconsistent")
            }
        }
    }
}

impl Error for SearchProjectMapFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Search(source) => Some(source),
            Self::InvalidCandidateSet(source) => Some(source),
            Self::InvalidPublication(source) => Some(source),
            Self::Fusion(source) => Some(source),
            Self::Cancelled | Self::ResourceLimit | Self::InvalidModuleBindings => None,
        }
    }
}

impl From<RetrievalCandidateSetError> for SearchProjectMapFailure {
    fn from(value: RetrievalCandidateSetError) -> Self {
        Self::InvalidCandidateSet(value)
    }
}

impl From<RetrievalCandidateSetsError> for SearchProjectMapFailure {
    fn from(value: RetrievalCandidateSetsError) -> Self {
        Self::InvalidPublication(value)
    }
}

impl From<FusionError> for SearchProjectMapFailure {
    fn from(value: FusionError) -> Self {
        Self::Fusion(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectMapSearchQuery, SearchProjectMap, SearchProjectMapFailure};
    use crate::{
        KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    };
    use a3_domain::{
        CanonicalDirectory, ExactSearchCursor, ExactSearchPage, ExactSearchPageSize,
        ExactSearchQuery, GitHead, GitReferenceName, GraphTraversalResult, IndexRunId,
        LexicalSearchCursor, LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery,
        ProjectIdentity, RepositoryId, RepositoryIdentity, SnapshotId, TraversalQuery,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[test]
    fn empty_channels_from_one_publication_produce_a_complete_empty_result()
    -> Result<(), Box<dyn Error>> {
        let store = Arc::new(RecordingStore::new(
            IndexRunId::from_bytes([7; 32]),
            [8; 32],
        ));
        let result = block_on(SearchProjectMap::new(store.clone()).execute(
            &project()?,
            &ProjectMapSearchQuery::try_from_string("launch parser".to_owned())?,
            &Control(false),
        ))?;

        assert!(result.hits().is_empty());
        assert!(!result.truncated());
        assert_eq!(result.index_run_id(), IndexRunId::from_bytes([7; 32]));
        assert_eq!(
            store.calls.lock().map_err(|_| "poisoned")?.as_slice(),
            ["exact", "lexical"]
        );
        Ok(())
    }

    #[test]
    fn cancellation_prevents_any_storage_read() -> Result<(), Box<dyn Error>> {
        let store = Arc::new(RecordingStore::new(
            IndexRunId::from_bytes([7; 32]),
            [8; 32],
        ));
        let result = block_on(SearchProjectMap::new(store.clone()).execute(
            &project()?,
            &ProjectMapSearchQuery::try_from_string("launch parser".to_owned())?,
            &Control(true),
        ));

        assert!(matches!(result, Err(SearchProjectMapFailure::Cancelled)));
        assert!(store.calls.lock().map_err(|_| "poisoned")?.is_empty());
        Ok(())
    }

    #[test]
    fn different_channel_publications_are_rejected() -> Result<(), Box<dyn Error>> {
        let store = Arc::new(RecordingStore::with_lexical_run(
            IndexRunId::from_bytes([7; 32]),
            IndexRunId::from_bytes([9; 32]),
            [8; 32],
        ));
        let result = block_on(SearchProjectMap::new(store).execute(
            &project()?,
            &ProjectMapSearchQuery::try_from_string("launch parser".to_owned())?,
            &Control(false),
        ));

        assert!(matches!(
            result,
            Err(SearchProjectMapFailure::InvalidPublication(_))
        ));
        Ok(())
    }

    #[test]
    fn query_requires_a_searchable_lexical_token() {
        assert!(ProjectMapSearchQuery::try_from_string("ab".to_owned()).is_err());
        assert!(ProjectMapSearchQuery::try_from_string("  parser  ".to_owned()).is_ok());
    }

    #[derive(Debug)]
    struct RecordingStore {
        exact_run: IndexRunId,
        lexical_run: IndexRunId,
        snapshot: SnapshotId,
        calls: Mutex<Vec<&'static str>>,
    }

    impl RecordingStore {
        fn new(run: IndexRunId, snapshot: [u8; 32]) -> Self {
            Self::with_lexical_run(run, run, snapshot)
        }

        fn with_lexical_run(
            exact_run: IndexRunId,
            lexical_run: IndexRunId,
            snapshot: [u8; 32],
        ) -> Self {
            Self {
                exact_run,
                lexical_run,
                snapshot: SnapshotId::from_bytes(snapshot),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl KnowledgeSearchStore for RecordingStore {
        fn search_exact<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ExactSearchQuery,
            page_size: ExactSearchPageSize,
            _cursor: Option<&'a ExactSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?
                    .push("exact");
                ExactSearchPage::new(self.exact_run, self.snapshot, Vec::new(), None, page_size)
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
            })
        }

        fn search_lexical<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a LexicalSearchQuery,
            page_size: LexicalSearchPageSize,
            _cursor: Option<&'a LexicalSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?
                    .push("lexical");
                LexicalSearchPage::new(self.lexical_run, self.snapshot, Vec::new(), None, page_size)
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
            })
        }

        fn traverse_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a TraversalQuery,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
            Box::pin(async { Err(KnowledgeSearchFailure::SeedUnavailable) })
        }
    }

    #[derive(Debug)]
    struct Control(bool);

    impl KnowledgeSearchControl for Control {
        fn is_cancelled(&self) -> bool {
            self.0
        }
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }
}
