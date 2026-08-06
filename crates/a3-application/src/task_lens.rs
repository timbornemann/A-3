use crate::{
    JobContext, KnowledgeIndexFailure, KnowledgeSearchControl, KnowledgeSearchFailure,
    KnowledgeSearchStore, KnowledgeStoreFailure,
};
use a3_domain::{
    CandidateFreshness, CandidateTokenCost, ExactSearchPageSize, ExactSearchQuery,
    ExactSearchTarget, ExactSearchTerm, FusionError, FusionPolicy, FusionResultLimit,
    GraphEndpoint, IndexRunId, LexicalSearchPageSize, LexicalSearchQuery, LexicalSearchTerm,
    NormalizedRetrievalSignal, Progress, ProjectIdentity, PublishedIndex, RetrievalCandidate,
    RetrievalCandidateSet, RetrievalCandidateSetError, RetrievalCandidateSets,
    RetrievalCandidateSetsError, RetrievalCandidateSignals, RetrievalTargetId, SnapshotId,
    SourceChannel, TaskLens, TaskLensClaim, TaskLensCompileError, TaskLensPolicy, TaskLensSeed,
    TaskLensSeedSet, TaskLensTokenBudget, TraversalQuery, TraversalResultLimit,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TASK_LENS_PROGRESS_TOTAL: u64 = 7;
const MAX_TASK_LENS_EXACT_QUERIES: usize = 16;
const MAX_TASK_LENS_LEXICAL_TOKENS: usize = 32;
const MAX_TASK_LENS_GRAPH_SEEDS: usize = 4;
const MAX_TASK_LENS_CHANNEL_CANDIDATES: usize = 100;
const MAX_TASK_LENS_TIMEOUT_MILLIS: u64 = 120_000;
const MAX_TASK_LENS_CLAIMS: u16 = 128;
const MAX_TASK_LENS_SEMANTIC_HITS: u16 = 100;

/// Owned future returned by the object-safe Task Lens claim reader.
pub type TaskLensClaimStoreFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TaskLensClaimResult, TaskLensClaimStoreFailure>> + Send + 'a>,
>;

/// Owned future returning a shared immutable current index for Task Lens compilation.
pub type TaskLensIndexStoreFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<Arc<PublishedIndex>>, KnowledgeIndexFailure>> + Send + 'a,
    >,
>;

/// Owned future returned by the optional Task Lens semantic candidate provider.
pub type TaskLensSemanticSearchFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<TaskLensSemanticResult, TaskLensSemanticSearchFailure>>
            + Send
            + 'a,
    >,
>;

/// Cooperative cancellation and bounded phase progress for one complete Task Lens compilation.
pub trait TaskLensControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports one of the fixed monotone version-one phases.
    fn report_progress(&self, progress: Progress) -> Result<(), TaskLensControlError>;
}

impl TaskLensControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), TaskLensControlError> {
        JobContext::report_progress(self, progress).map_err(|_| TaskLensControlError::Unavailable)
    }
}

/// Task Lens progress delivery failed at the owning scheduler boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for TaskLensControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Task Lens progress is unavailable")
    }
}

impl Error for TaskLensControlError {}

/// Read-only boundary avoiding a deep clone of a complete immutable index per Lens.
pub trait TaskLensIndexStore: fmt::Debug + Send + Sync {
    /// Returns the latest atomically published index through a shared immutable capability.
    fn load_current_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a>;
}

/// Positive bounded deadline for exact through semantic Task Lens compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensTimeout(Duration);

impl TaskLensTimeout {
    /// Version-one local interactive deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(30));

    /// Creates a deadline capped at two minutes.
    pub fn from_millis(value: u64) -> Result<Self, TaskLensTimeoutError> {
        if value == 0 || value > MAX_TASK_LENS_TIMEOUT_MILLIS {
            return Err(TaskLensTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Task Lens deadline was zero or exceeded two minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensTimeoutError {
    value: u64,
}

impl fmt::Display for TaskLensTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Task Lens timeout {} ms must be between 1 and {MAX_TASK_LENS_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for TaskLensTimeoutError {}

/// Maximum current verified claims reconstructed for one temporary lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensClaimLimit(u16);

impl TaskLensClaimLimit {
    /// Version-one claim read boundary.
    pub const DEFAULT: Self = Self(MAX_TASK_LENS_CLAIMS);

    /// Returns the bounded primitive.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Bounded current claim projection with explicit upstream truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensClaimResult {
    claims: Vec<TaskLensClaim>,
    truncated: bool,
}

impl TaskLensClaimResult {
    /// Rejects an adapter result that crossed the fixed R10 claim boundary.
    pub fn new(
        claims: Vec<TaskLensClaim>,
        truncated: bool,
    ) -> Result<Self, TaskLensClaimResultError> {
        if claims.len() > usize::from(MAX_TASK_LENS_CLAIMS) {
            return Err(TaskLensClaimResultError::TooManyClaims);
        }
        Ok(Self { claims, truncated })
    }

    /// Returns current claims in stable Claim ID order.
    #[must_use]
    pub fn claims(&self) -> &[TaskLensClaim] {
        &self.claims
    }

    /// Consumes the result into its bounded claims.
    #[must_use]
    pub fn into_claims(self) -> Vec<TaskLensClaim> {
        self.claims
    }

    /// Returns whether additional current claims were omitted at the boundary.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Invalid claim-store result cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensClaimResultError {
    /// More than 128 claims crossed the adapter boundary.
    TooManyClaims,
}

impl fmt::Display for TaskLensClaimResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Task Lens claim result exceeds 128 claims")
    }
}

impl Error for TaskLensClaimResultError {}

/// Read-only storage boundary for evidence-resolved R9 claims.
pub trait TaskLensClaimStore: fmt::Debug + Send + Sync {
    /// Reconstructs at most `limit` typed claims for the supplied published index capability.
    fn load_claims<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        limit: TaskLensClaimLimit,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimStoreFuture<'a>;
}

/// Stable claim-read failure without SQL, rows, or source content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensClaimStoreFailure {
    /// Shared local storage boundary failed.
    Storage(KnowledgeStoreFailure),
    /// Stored claim, classification, or evidence violated the typed contract.
    InvalidStoredProjection,
    /// Owning Task Lens operation was cancelled.
    Cancelled,
    /// Claim reconstruction exceeded its adapter deadline.
    TimedOut,
}

impl fmt::Display for TaskLensClaimStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(source) => write!(formatter, "Task Lens claim storage failed: {source}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored Task Lens claim projection is invalid")
            }
            Self::Cancelled => formatter.write_str("Task Lens claim read was cancelled"),
            Self::TimedOut => formatter.write_str("Task Lens claim read timed out"),
        }
    }
}

impl Error for TaskLensClaimStoreFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection | Self::Cancelled | Self::TimedOut => None,
        }
    }
}

/// One optional semantic target. Similarity remains explicitly non-evidentiary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensSemanticHit {
    target: ExactSearchTarget,
    similarity: NormalizedRetrievalSignal,
}

impl TaskLensSemanticHit {
    /// Binds one current target to a normalized similarity signal.
    #[must_use]
    pub const fn new(target: ExactSearchTarget, similarity: NormalizedRetrievalSignal) -> Self {
        Self { target, similarity }
    }

    /// Returns the current target projection.
    #[must_use]
    pub const fn target(&self) -> &ExactSearchTarget {
        &self.target
    }

    /// Returns similarity without any evidence or Fact capability.
    #[must_use]
    pub const fn similarity(&self) -> NormalizedRetrievalSignal {
        self.similarity
    }
}

/// Bounded semantic result tied to exactly one published run and snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensSemanticResult {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    hits: Vec<TaskLensSemanticHit>,
    truncated: bool,
}

impl TaskLensSemanticResult {
    /// Validates cardinality and stable target uniqueness without promoting similarity to fact.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        mut hits: Vec<TaskLensSemanticHit>,
        truncated: bool,
    ) -> Result<Self, TaskLensSemanticResultError> {
        if hits.len() > usize::from(MAX_TASK_LENS_SEMANTIC_HITS) {
            return Err(TaskLensSemanticResultError::TooManyHits);
        }
        let mut target_ids = BTreeSet::new();
        for hit in &hits {
            if !target_ids.insert(target_id(hit.target())) {
                return Err(TaskLensSemanticResultError::DuplicateTarget);
            }
        }
        hits.sort_by(|left, right| {
            right
                .similarity()
                .cmp(&left.similarity())
                .then_with(|| target_id(left.target()).cmp(&target_id(right.target())))
        });
        Ok(Self {
            index_run_id,
            snapshot_id,
            hits,
            truncated,
        })
    }

    /// Returns the exact source run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the exact source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns hits in descending similarity with stable-ID tie-breaking.
    #[must_use]
    pub fn hits(&self) -> &[TaskLensSemanticHit] {
        &self.hits
    }

    /// Returns whether the provider omitted lower-ranked candidates.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Invalid optional semantic result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensSemanticResultError {
    /// More than 100 candidates crossed the boundary.
    TooManyHits,
    /// The same stable target appeared twice.
    DuplicateTarget,
}

impl fmt::Display for TaskLensSemanticResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyHits => "Task Lens semantic result exceeds 100 hits",
            Self::DuplicateTarget => "Task Lens semantic result repeats a target",
        })
    }
}

impl Error for TaskLensSemanticResultError {}

/// Optional external boundary that generates semantic candidates after deterministic channels.
pub trait TaskLensSemanticSearch: fmt::Debug + Send + Sync {
    /// Searches with the canonical seed set while retaining publication and resource bounds.
    fn search<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        seeds: &'a TaskLensSeedSet,
        limit: TaskLensSemanticLimit,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensSemanticSearchFuture<'a>;
}

/// Positive maximum semantic candidates requested by the Task Lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensSemanticLimit(u16);

impl TaskLensSemanticLimit {
    /// Version-one optional semantic boundary.
    pub const DEFAULT: Self = Self(20);

    /// Returns the bounded primitive.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Stable optional semantic provider failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensSemanticSearchFailure {
    /// Provider or local semantic store failed.
    Unavailable,
    /// Provider returned an invalid or stale target projection.
    InvalidResult,
    /// Owning Task Lens operation was cancelled.
    Cancelled,
    /// Optional semantic search exceeded its deadline.
    TimedOut,
}

impl fmt::Display for TaskLensSemanticSearchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Task Lens semantic search is unavailable",
            Self::InvalidResult => "Task Lens semantic search returned an invalid result",
            Self::Cancelled => "Task Lens semantic search was cancelled",
            Self::TimedOut => "Task Lens semantic search timed out",
        })
    }
}

impl Error for TaskLensSemanticSearchFailure {}

/// Inbound use case compiling one deterministic Task Lens through ordered read-only channels.
#[derive(Debug, Clone, Copy)]
pub struct CompileTaskLens<'a> {
    index: &'a dyn TaskLensIndexStore,
    search: &'a dyn KnowledgeSearchStore,
    claims: &'a dyn TaskLensClaimStore,
    semantic: Option<&'a dyn TaskLensSemanticSearch>,
    timeout: TaskLensTimeout,
}

impl<'a> CompileTaskLens<'a> {
    /// Composes deterministic index/search and current verified-claim boundaries.
    #[must_use]
    pub const fn new(
        index: &'a dyn TaskLensIndexStore,
        search: &'a dyn KnowledgeSearchStore,
        claims: &'a dyn TaskLensClaimStore,
    ) -> Self {
        Self {
            index,
            search,
            claims,
            semantic: None,
            timeout: TaskLensTimeout::DEFAULT,
        }
    }

    /// Enables an optional semantic candidate source that always runs last.
    #[must_use]
    pub const fn with_semantic(mut self, semantic: &'a dyn TaskLensSemanticSearch) -> Self {
        self.semantic = Some(semantic);
        self
    }

    /// Overrides the bounded whole-operation deadline, primarily for owning compositions.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: TaskLensTimeout) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the exact Task Lens policy version used after channel fusion.
    #[must_use]
    pub const fn policy_version(self) -> a3_domain::TaskLensPolicyVersion {
        TaskLensPolicy::v1().version()
    }

    /// Executes exact → lexical → graph/test → claims → optional semantic → compile.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        seeds: TaskLensSeedSet,
        token_budget: TaskLensTokenBudget,
        control: &dyn TaskLensControl,
    ) -> Result<TaskLens, CompileTaskLensFailure> {
        let deadline = TaskLensDeadline::new(control, self.timeout);
        deadline.report(0)?;
        let published = self
            .index
            .load_current_index(project, &deadline)
            .await
            .map_err(|source| deadline.map_index_failure(source))?
            .ok_or(CompileTaskLensFailure::IndexUnavailable)?;
        deadline.report(1)?;

        let queries = TaskLensQueries::new(&seeds, &published)?;
        let exact = collect_exact(
            self.search,
            project,
            &published,
            &seeds,
            &queries,
            &deadline,
        )
        .await?;
        deadline.report(2)?;
        let lexical = collect_lexical(
            self.search,
            project,
            &published,
            &seeds,
            queries.lexical.as_ref(),
            &deadline,
        )
        .await?;
        deadline.report(3)?;
        let (graph, tests) = collect_relationships(
            self.search,
            project,
            &published,
            &seeds,
            &exact,
            lexical.as_ref(),
            &deadline,
        )
        .await?;
        deadline.report(4)?;

        let claim_result = self
            .claims
            .load_claims(project, &published, TaskLensClaimLimit::DEFAULT, &deadline)
            .await
            .map_err(|source| deadline.map_claim_failure(source))?;
        deadline.check()?;
        deadline.report(5)?;

        let semantic = match self.semantic {
            Some(provider) => {
                let result = provider
                    .search(
                        project,
                        &published,
                        &seeds,
                        TaskLensSemanticLimit::DEFAULT,
                        &deadline,
                    )
                    .await
                    .map_err(|source| deadline.map_semantic_failure(source))?;
                deadline.check()?;
                Some(semantic_candidate_set(&published, &seeds, result)?)
            }
            None => None,
        };
        deadline.report(6)?;

        let mut sets = vec![exact];
        if let Some(lexical) = lexical {
            sets.push(lexical);
        }
        if let Some(graph) = graph {
            sets.push(graph);
        }
        if let Some(tests) = tests {
            sets.push(tests);
        }
        if let Some(semantic) = semantic {
            sets.push(semantic);
        }
        let fused = FusionPolicy::v1().fuse(
            RetrievalCandidateSets::new(published.run().id(), published.run().snapshot_id(), sets)?,
            FusionResultLimit::new(32).map_err(|_| CompileTaskLensFailure::ResourceLimit)?,
        )?;
        let claims_truncated = claim_result.truncated();
        let lens = TaskLensPolicy::v1().compile(
            &published,
            seeds,
            &fused,
            claim_result.into_claims(),
            claims_truncated,
            token_budget,
        )?;
        deadline.report(TASK_LENS_PROGRESS_TOTAL)?;
        Ok(lens)
    }
}

struct TaskLensQueries {
    exact: Vec<ExactSearchQuery>,
    lexical: Option<LexicalSearchQuery>,
    truncated: bool,
}

impl TaskLensQueries {
    fn new(
        seeds: &TaskLensSeedSet,
        published: &PublishedIndex,
    ) -> Result<Self, CompileTaskLensFailure> {
        let mut exact = Vec::new();
        let mut seen = BTreeSet::new();
        let mut truncated = false;
        for seed in seeds.supplemental() {
            let query = match seed {
                TaskLensSeed::ExplicitPath(path) | TaskLensSeed::ChangedPath(path) => {
                    Some(ExactSearchQuery::Path(path.clone()))
                }
                TaskLensSeed::ExplicitSymbol(symbol_id) => published
                    .publication()
                    .graph()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.id() == *symbol_id)
                    .map(|symbol| symbol.parsed().name().as_str().to_owned())
                    .map(exact_symbol_query)
                    .transpose()?,
                TaskLensSeed::ExplicitIdentifier(text) => {
                    Some(exact_symbol_query(text.as_str().to_owned())?)
                }
                TaskLensSeed::Diagnostic { .. }
                | TaskLensSeed::OpenHypothesis(_)
                | TaskLensSeed::FailedVerification(_) => None,
            };
            if let Some(query) = query {
                push_exact_query(&mut exact, &mut seen, query, &mut truncated);
            }
        }
        let tokens = seed_tokens(seeds, published);
        for token in &tokens {
            push_exact_query(
                &mut exact,
                &mut seen,
                exact_symbol_query(token.clone())?,
                &mut truncated,
            );
        }
        let lexical_text = tokens
            .iter()
            .filter(|token| token.len() >= 3)
            .take(MAX_TASK_LENS_LEXICAL_TOKENS)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        let lexical = if lexical_text.is_empty() {
            None
        } else {
            Some(LexicalSearchQuery::new(
                LexicalSearchTerm::try_from_string(lexical_text)
                    .map_err(|_| CompileTaskLensFailure::InvalidSeedQuery)?,
            ))
        };
        Ok(Self {
            exact,
            lexical,
            truncated,
        })
    }
}

fn push_exact_query(
    queries: &mut Vec<ExactSearchQuery>,
    seen: &mut BTreeSet<ExactSearchQuery>,
    query: ExactSearchQuery,
    truncated: &mut bool,
) {
    if !seen.insert(query.clone()) {
        return;
    }
    if queries.len() == MAX_TASK_LENS_EXACT_QUERIES {
        *truncated = true;
    } else {
        queries.push(query);
    }
}

fn exact_symbol_query(value: String) -> Result<ExactSearchQuery, CompileTaskLensFailure> {
    ExactSearchTerm::try_from_string(value)
        .map(ExactSearchQuery::Symbol)
        .map_err(|_| CompileTaskLensFailure::InvalidSeedQuery)
}

async fn collect_exact(
    store: &dyn KnowledgeSearchStore,
    project: &ProjectIdentity,
    published: &PublishedIndex,
    seeds: &TaskLensSeedSet,
    queries: &TaskLensQueries,
    deadline: &TaskLensDeadline<'_>,
) -> Result<RetrievalCandidateSet, CompileTaskLensFailure> {
    let mut candidates = BTreeMap::<RetrievalTargetId, (u8, RetrievalCandidate)>::new();
    let mut truncated = queries.truncated;
    for query in &queries.exact {
        deadline.check()?;
        let page = store
            .search_exact(project, query, ExactSearchPageSize::DEFAULT, None, deadline)
            .await
            .map_err(|source| deadline.map_search_failure(source))?;
        validate_publication(published, page.index_run_id(), page.snapshot_id())?;
        truncated |= page.next_cursor().is_some();
        for hit in page.hits() {
            let target_id = target_id(hit.target());
            let order = hit.explanation().sort_order();
            let candidate =
                RetrievalCandidate::from_exact(hit, target_signals(seeds, hit.target())?);
            match candidates.get(&target_id) {
                Some((existing_order, _)) if *existing_order <= order => {}
                Some(_) => {
                    candidates.insert(target_id, (order, candidate));
                }
                None if candidates.len() < MAX_TASK_LENS_CHANNEL_CANDIDATES => {
                    candidates.insert(target_id, (order, candidate));
                }
                None => truncated = true,
            }
        }
    }
    let candidates = candidates
        .into_values()
        .map(|(_, candidate)| candidate)
        .collect();
    candidate_set(published, SourceChannel::Exact, candidates, truncated)
}

async fn collect_lexical(
    store: &dyn KnowledgeSearchStore,
    project: &ProjectIdentity,
    published: &PublishedIndex,
    seeds: &TaskLensSeedSet,
    query: Option<&LexicalSearchQuery>,
    deadline: &TaskLensDeadline<'_>,
) -> Result<Option<RetrievalCandidateSet>, CompileTaskLensFailure> {
    let Some(query) = query else {
        return Ok(None);
    };
    deadline.check()?;
    let page = store
        .search_lexical(
            project,
            query,
            LexicalSearchPageSize::new(100).map_err(|_| CompileTaskLensFailure::ResourceLimit)?,
            None,
            deadline,
        )
        .await
        .map_err(|source| deadline.map_search_failure(source))?;
    validate_publication(published, page.index_run_id(), page.snapshot_id())?;
    let candidates = page
        .hits()
        .iter()
        .map(|hit| {
            Ok(RetrievalCandidate::from_lexical(
                hit,
                target_signals(seeds, hit.target())?,
            ))
        })
        .collect::<Result<Vec<_>, CompileTaskLensFailure>>()?;
    candidate_set(
        published,
        SourceChannel::Lexical,
        candidates,
        page.next_cursor().is_some(),
    )
    .map(Some)
}

async fn collect_relationships(
    store: &dyn KnowledgeSearchStore,
    project: &ProjectIdentity,
    published: &PublishedIndex,
    seeds: &TaskLensSeedSet,
    exact: &RetrievalCandidateSet,
    lexical: Option<&RetrievalCandidateSet>,
    deadline: &TaskLensDeadline<'_>,
) -> Result<(Option<RetrievalCandidateSet>, Option<RetrievalCandidateSet>), CompileTaskLensFailure>
{
    let mut graph_seeds = Vec::new();
    let mut seen_seeds = BTreeSet::new();
    for candidate in exact.candidates().iter().chain(
        lexical
            .into_iter()
            .flat_map(RetrievalCandidateSet::candidates),
    ) {
        if let ExactSearchTarget::Symbol(symbol) = candidate.target()
            && seen_seeds.insert(symbol.symbol().id())
        {
            graph_seeds.push(symbol.symbol().id());
            if graph_seeds.len() == MAX_TASK_LENS_GRAPH_SEEDS {
                break;
            }
        }
    }
    let limit = TraversalResultLimit::new(10).map_err(|_| CompileTaskLensFailure::ResourceLimit)?;
    let mut graph = BTreeMap::new();
    let mut tests = BTreeMap::new();
    let mut graph_truncated = false;
    let mut tests_truncated = false;
    for symbol_id in graph_seeds {
        let queries = [
            TraversalQuery::callers(symbol_id, limit),
            TraversalQuery::callees(symbol_id, limit),
            TraversalQuery::imports(GraphEndpoint::Symbol(symbol_id), limit),
            TraversalQuery::exports(GraphEndpoint::Symbol(symbol_id), limit),
            TraversalQuery::tests(GraphEndpoint::Symbol(symbol_id), limit),
        ];
        for query in queries {
            deadline.check()?;
            let result = store
                .traverse_graph(project, &query, deadline)
                .await
                .map_err(|source| deadline.map_search_failure(source))?;
            validate_publication(published, result.index_run_id(), result.snapshot_id())?;
            let is_test = result.query().source_channel() == SourceChannel::Test;
            if is_test {
                tests_truncated |= result.truncated();
            } else {
                graph_truncated |= result.truncated();
            }
            let target_map = if is_test { &mut tests } else { &mut graph };
            for hit in result.hits() {
                let target_id = target_id(hit.target());
                if target_map.contains_key(&target_id) {
                    continue;
                }
                if target_map.len() == MAX_TASK_LENS_CHANNEL_CANDIDATES {
                    if is_test {
                        tests_truncated = true;
                    } else {
                        graph_truncated = true;
                    }
                    continue;
                }
                target_map.insert(
                    target_id,
                    RetrievalCandidate::from_relationship(
                        hit,
                        target_signals(seeds, hit.target())?,
                    ),
                );
            }
        }
    }
    let graph = if graph.is_empty() {
        None
    } else {
        Some(candidate_set(
            published,
            SourceChannel::Graph,
            graph.into_values().collect(),
            graph_truncated,
        )?)
    };
    let tests = if tests.is_empty() {
        None
    } else {
        Some(candidate_set(
            published,
            SourceChannel::Test,
            tests.into_values().collect(),
            tests_truncated,
        )?)
    };
    Ok((graph, tests))
}

fn semantic_candidate_set(
    published: &PublishedIndex,
    seeds: &TaskLensSeedSet,
    result: TaskLensSemanticResult,
) -> Result<RetrievalCandidateSet, CompileTaskLensFailure> {
    validate_publication(published, result.index_run_id(), result.snapshot_id())?;
    let candidates = result
        .hits()
        .iter()
        .map(|hit| {
            Ok(RetrievalCandidate::semantic(
                hit.target().clone(),
                hit.similarity(),
                target_signals(seeds, hit.target())?,
            ))
        })
        .collect::<Result<Vec<_>, CompileTaskLensFailure>>()?;
    candidate_set(
        published,
        SourceChannel::Semantic,
        candidates,
        result.truncated(),
    )
}

fn candidate_set(
    published: &PublishedIndex,
    channel: SourceChannel,
    candidates: Vec<RetrievalCandidate>,
    truncated: bool,
) -> Result<RetrievalCandidateSet, CompileTaskLensFailure> {
    if truncated {
        RetrievalCandidateSet::truncated(
            published.run().id(),
            published.run().snapshot_id(),
            channel,
            candidates,
        )
    } else {
        RetrievalCandidateSet::complete(
            published.run().id(),
            published.run().snapshot_id(),
            channel,
            candidates,
        )
    }
    .map_err(Into::into)
}

fn validate_publication(
    published: &PublishedIndex,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
) -> Result<(), CompileTaskLensFailure> {
    if published.run().id() != index_run_id || published.run().snapshot_id() != snapshot_id {
        Err(CompileTaskLensFailure::InvalidChannelProjection)
    } else {
        Ok(())
    }
}

fn target_signals(
    seeds: &TaskLensSeedSet,
    target: &ExactSearchTarget,
) -> Result<RetrievalCandidateSignals, CompileTaskLensFailure> {
    let target_tokens = target_tokens(target);
    let goal_tokens = ascii_tokens(seeds.goal().as_str().as_bytes(), 2);
    let step_tokens = ascii_tokens(seeds.step().as_str().as_bytes(), 2);
    Ok(RetrievalCandidateSignals::new(
        overlap_signal(&goal_tokens, &target_tokens)?,
        overlap_signal(&step_tokens, &target_tokens)?,
        CandidateFreshness::Current,
        CandidateTokenCost::new(target_token_cost(target))
            .map_err(|_| CompileTaskLensFailure::ResourceLimit)?,
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

fn overlap_signal(
    seed_tokens: &BTreeSet<String>,
    target_tokens: &BTreeSet<String>,
) -> Result<NormalizedRetrievalSignal, CompileTaskLensFailure> {
    if seed_tokens.is_empty() || target_tokens.is_empty() {
        return Ok(NormalizedRetrievalSignal::ZERO);
    }
    let overlap = seed_tokens.intersection(target_tokens).count();
    let denominator = seed_tokens.len().min(target_tokens.len());
    let value = overlap.saturating_mul(10_000) / denominator;
    NormalizedRetrievalSignal::new(
        u16::try_from(value).map_err(|_| CompileTaskLensFailure::ResourceLimit)?,
    )
    .map_err(|_| CompileTaskLensFailure::ResourceLimit)
}

fn seed_tokens(seeds: &TaskLensSeedSet, published: &PublishedIndex) -> BTreeSet<String> {
    let mut tokens = ascii_tokens(seeds.goal().as_str().as_bytes(), 2);
    tokens.extend(ascii_tokens(seeds.step().as_str().as_bytes(), 2));
    for seed in seeds.supplemental() {
        match seed {
            TaskLensSeed::ExplicitPath(path) | TaskLensSeed::ChangedPath(path) => {
                tokens.extend(ascii_tokens(path.as_bytes(), 2));
            }
            TaskLensSeed::ExplicitSymbol(symbol_id) => {
                if let Some(symbol) = published
                    .publication()
                    .graph()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.id() == *symbol_id)
                {
                    tokens.extend(ascii_tokens(symbol.parsed().name().as_str().as_bytes(), 2));
                }
            }
            TaskLensSeed::ExplicitIdentifier(text) | TaskLensSeed::FailedVerification(text) => {
                tokens.extend(ascii_tokens(text.as_str().as_bytes(), 2));
            }
            TaskLensSeed::Diagnostic { text, .. } => {
                tokens.extend(ascii_tokens(text.as_str().as_bytes(), 2));
            }
            TaskLensSeed::OpenHypothesis(_) => {}
        }
    }
    tokens
}

fn target_tokens(target: &ExactSearchTarget) -> BTreeSet<String> {
    let mut tokens = ascii_tokens(target.revision().path().as_bytes(), 2);
    if let ExactSearchTarget::Symbol(symbol) = target {
        tokens.extend(ascii_tokens(
            symbol.symbol().parsed().name().as_str().as_bytes(),
            2,
        ));
        tokens.extend(ascii_tokens(symbol.qualified_name().as_str().as_bytes(), 2));
        if let Some(signature) = symbol.symbol().parsed().signature() {
            tokens.extend(ascii_tokens(signature.as_str().as_bytes(), 2));
        }
    }
    tokens
}

fn ascii_tokens(value: &[u8], minimum_length: usize) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = Vec::new();
    for byte in value {
        if byte.is_ascii_alphanumeric() || *byte == b'_' {
            current.push(byte.to_ascii_lowercase());
        } else {
            push_ascii_token(&mut tokens, &mut current, minimum_length);
        }
    }
    push_ascii_token(&mut tokens, &mut current, minimum_length);
    tokens
}

fn push_ascii_token(tokens: &mut BTreeSet<String>, current: &mut Vec<u8>, minimum_length: usize) {
    if current.len() >= minimum_length
        && let Ok(token) = String::from_utf8(std::mem::take(current))
    {
        tokens.insert(token);
    }
    current.clear();
}

fn target_id(target: &ExactSearchTarget) -> RetrievalTargetId {
    RetrievalTargetId::from_target(target)
}

#[derive(Debug)]
struct TaskLensDeadline<'a> {
    control: &'a dyn TaskLensControl,
    started: Instant,
    timeout: Duration,
}

impl<'a> TaskLensDeadline<'a> {
    fn new(control: &'a dyn TaskLensControl, timeout: TaskLensTimeout) -> Self {
        Self {
            control,
            started: Instant::now(),
            timeout: timeout.duration(),
        }
    }

    fn check(&self) -> Result<(), CompileTaskLensFailure> {
        if self.control.is_cancelled() {
            Err(CompileTaskLensFailure::Cancelled)
        } else if self.started.elapsed() >= self.timeout {
            Err(CompileTaskLensFailure::TimedOut)
        } else {
            Ok(())
        }
    }

    fn report(&self, completed: u64) -> Result<(), CompileTaskLensFailure> {
        self.check()?;
        self.control
            .report_progress(
                Progress::determinate(completed, TASK_LENS_PROGRESS_TOTAL)
                    .map_err(|_| CompileTaskLensFailure::ResourceLimit)?,
            )
            .map_err(|_| CompileTaskLensFailure::ProgressUnavailable)
    }

    fn map_index_failure(&self, source: KnowledgeIndexFailure) -> CompileTaskLensFailure {
        if self.control.is_cancelled() {
            CompileTaskLensFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || source == KnowledgeIndexFailure::TimedOut
        {
            CompileTaskLensFailure::TimedOut
        } else if source == KnowledgeIndexFailure::Cancelled {
            CompileTaskLensFailure::Cancelled
        } else {
            CompileTaskLensFailure::Index(source)
        }
    }

    fn map_search_failure(&self, source: KnowledgeSearchFailure) -> CompileTaskLensFailure {
        if self.control.is_cancelled() {
            CompileTaskLensFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || source == KnowledgeSearchFailure::TimedOut
        {
            CompileTaskLensFailure::TimedOut
        } else if source == KnowledgeSearchFailure::Cancelled {
            CompileTaskLensFailure::Cancelled
        } else {
            CompileTaskLensFailure::Search(source)
        }
    }

    fn map_claim_failure(&self, source: TaskLensClaimStoreFailure) -> CompileTaskLensFailure {
        if self.control.is_cancelled() {
            CompileTaskLensFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || source == TaskLensClaimStoreFailure::TimedOut
        {
            CompileTaskLensFailure::TimedOut
        } else if source == TaskLensClaimStoreFailure::Cancelled {
            CompileTaskLensFailure::Cancelled
        } else {
            CompileTaskLensFailure::Claims(source)
        }
    }

    fn map_semantic_failure(
        &self,
        source: TaskLensSemanticSearchFailure,
    ) -> CompileTaskLensFailure {
        if self.control.is_cancelled() {
            CompileTaskLensFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || source == TaskLensSemanticSearchFailure::TimedOut
        {
            CompileTaskLensFailure::TimedOut
        } else if source == TaskLensSemanticSearchFailure::Cancelled {
            CompileTaskLensFailure::Cancelled
        } else {
            CompileTaskLensFailure::Semantic(source)
        }
    }
}

impl TaskLensControl for TaskLensDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        if self.control.is_cancelled() || self.started.elapsed() >= self.timeout {
            Err(TaskLensControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl KnowledgeSearchControl for TaskLensDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }
}

/// Stable failure of complete ordered Task Lens compilation.
#[derive(Debug)]
pub enum CompileTaskLensFailure {
    /// No atomically published index is available.
    IndexUnavailable,
    /// Deterministic index read failed.
    Index(KnowledgeIndexFailure),
    /// Exact, lexical, or graph read failed.
    Search(KnowledgeSearchFailure),
    /// Current verified-claim read failed.
    Claims(TaskLensClaimStoreFailure),
    /// Optional semantic candidate generation failed.
    Semantic(TaskLensSemanticSearchFailure),
    /// Seed text could not produce a valid bounded exact or lexical query.
    InvalidSeedQuery,
    /// A channel returned another run, snapshot, or malformed target projection.
    InvalidChannelProjection,
    /// One channel-specific candidate set violated R4 boundaries.
    CandidateSet(RetrievalCandidateSetError),
    /// Cross-channel collection violated R4 publication or uniqueness boundaries.
    CandidateSets(RetrievalCandidateSetsError),
    /// R4 normalization, deduplication, or ranking failed.
    Fusion(FusionError),
    /// R10 zoom, freshness, budget, or digest compilation failed.
    Compile(TaskLensCompileError),
    /// Owning operation cancelled before completion.
    Cancelled,
    /// Complete Task Lens compilation exceeded its deadline.
    TimedOut,
    /// Phase progress could not reach the owning job.
    ProgressUnavailable,
    /// Fixed cardinality or portable integer boundary was exceeded.
    ResourceLimit,
}

impl fmt::Display for CompileTaskLensFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexUnavailable => formatter.write_str("Task Lens requires a published index"),
            Self::Index(source) => write!(formatter, "Task Lens index read failed: {source}"),
            Self::Search(source) => write!(formatter, "Task Lens search failed: {source}"),
            Self::Claims(source) => write!(formatter, "Task Lens claim read failed: {source}"),
            Self::Semantic(source) => {
                write!(formatter, "Task Lens semantic search failed: {source}")
            }
            Self::InvalidSeedQuery => {
                formatter.write_str("Task Lens seeds cannot form a valid bounded query")
            }
            Self::InvalidChannelProjection => {
                formatter.write_str("Task Lens channel returned another or invalid publication")
            }
            Self::CandidateSet(source) => source.fmt(formatter),
            Self::CandidateSets(source) => source.fmt(formatter),
            Self::Fusion(source) => source.fmt(formatter),
            Self::Compile(source) => source.fmt(formatter),
            Self::Cancelled => formatter.write_str("Task Lens compilation was cancelled"),
            Self::TimedOut => formatter.write_str("Task Lens compilation timed out"),
            Self::ProgressUnavailable => formatter.write_str("Task Lens progress is unavailable"),
            Self::ResourceLimit => {
                formatter.write_str("Task Lens compilation exceeded a fixed resource boundary")
            }
        }
    }
}

impl Error for CompileTaskLensFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Index(source) => Some(source),
            Self::Search(source) => Some(source),
            Self::Claims(source) => Some(source),
            Self::Semantic(source) => Some(source),
            Self::CandidateSet(source) => Some(source),
            Self::CandidateSets(source) => Some(source),
            Self::Fusion(source) => Some(source),
            Self::Compile(source) => Some(source),
            Self::IndexUnavailable
            | Self::InvalidSeedQuery
            | Self::InvalidChannelProjection
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable
            | Self::ResourceLimit => None,
        }
    }
}

impl From<RetrievalCandidateSetError> for CompileTaskLensFailure {
    fn from(value: RetrievalCandidateSetError) -> Self {
        Self::CandidateSet(value)
    }
}

impl From<RetrievalCandidateSetsError> for CompileTaskLensFailure {
    fn from(value: RetrievalCandidateSetsError) -> Self {
        Self::CandidateSets(value)
    }
}

impl From<FusionError> for CompileTaskLensFailure {
    fn from(value: FusionError) -> Self {
        Self::Fusion(value)
    }
}

impl From<TaskLensCompileError> for CompileTaskLensFailure {
    fn from(value: TaskLensCompileError) -> Self {
        Self::Compile(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{TaskLensSemanticHit, TaskLensSemanticResult, TaskLensTimeout};
    use a3_domain::{
        ContentHash, FileRevision, IndexRunId, NormalizedRetrievalSignal, RepositoryPath,
        SnapshotId,
    };

    #[test]
    fn timeout_and_semantic_result_boundaries_are_explicit()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(TaskLensTimeout::from_millis(0).is_err());
        assert!(TaskLensTimeout::from_millis(120_001).is_err());
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([3; 32]),
        );
        let hit = TaskLensSemanticHit::new(
            a3_domain::ExactSearchTarget::File(revision),
            NormalizedRetrievalSignal::FULL,
        );
        let intervening = TaskLensSemanticHit::new(
            a3_domain::ExactSearchTarget::File(FileRevision::new(
                RepositoryPath::try_from_bytes(b"src/other.rs".to_vec())?,
                ContentHash::from_bytes([4; 32]),
            )),
            NormalizedRetrievalSignal::new(9_000)?,
        );
        let duplicate_with_different_score =
            TaskLensSemanticHit::new(hit.target().clone(), NormalizedRetrievalSignal::new(8_000)?);
        assert!(
            TaskLensSemanticResult::new(
                IndexRunId::from_bytes([1; 32]),
                SnapshotId::from_bytes([2; 32]),
                vec![hit, intervening, duplicate_with_different_score],
                false,
            )
            .is_err()
        );
        Ok(())
    }
}
