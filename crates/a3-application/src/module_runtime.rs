use crate::{
    JobContext, KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore,
    KnowledgeStoreFailure,
};
use a3_domain::{
    GraphEndpoint, GraphSymbol, GraphTraversalResult, IndexRunId, ModuleCardEvidenceId, ModuleId,
    Progress, ProjectIdentity, SnapshotId, SymbolId, SymbolRole, SyntaxRelationKind,
    TraversalDepth, TraversalDirection, TraversalQuery, TraversalResultLimit,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_RUNTIME_ROOTS: u16 = 256;

/// Positive prefix boundary for adapter-proven entrypoint or test roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRuntimeRootLimit(u16);

impl ModuleRuntimeRootLimit {
    /// Product default keeps the first project-map expansion compact.
    pub const DEFAULT: Self = Self(20);

    /// Accepts one through the complete persisted V8 feature boundary.
    pub fn new(value: u16) -> Result<Self, ModuleRuntimeRootLimitError> {
        if value == 0 || value > MAX_RUNTIME_ROOTS {
            return Err(ModuleRuntimeRootLimitError);
        }
        Ok(Self(value))
    }

    /// Returns the validated visible-prefix boundary.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A runtime-root limit was zero or exceeded the persisted V8 bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRuntimeRootLimitError;

impl fmt::Display for ModuleRuntimeRootLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module runtime root limit must be between one and 256")
    }
}

impl Error for ModuleRuntimeRootLimitError {}

/// Bounded atomic request for both adapter-proven feature-root prefixes of one primary module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeMapQuery {
    module_id: ModuleId,
    entrypoint_limit: ModuleRuntimeRootLimit,
    test_limit: ModuleRuntimeRootLimit,
}

impl ModuleRuntimeMapQuery {
    /// Creates a query whose two limits can be expanded independently by the UI.
    #[must_use]
    pub const fn new(
        module_id: ModuleId,
        entrypoint_limit: ModuleRuntimeRootLimit,
        test_limit: ModuleRuntimeRootLimit,
    ) -> Self {
        Self {
            module_id,
            entrypoint_limit,
            test_limit,
        }
    }

    /// Returns the current primary module selected by stable identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the maximum visible entrypoint prefix.
    #[must_use]
    pub const fn entrypoint_limit(&self) -> ModuleRuntimeRootLimit {
        self.entrypoint_limit
    }

    /// Returns the maximum visible test prefix.
    #[must_use]
    pub const fn test_limit(&self) -> ModuleRuntimeRootLimit {
        self.test_limit
    }
}

/// Deterministic semantic role that makes a symbol a runtime-map root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleRuntimeRootKind {
    /// Program, library, or script entrypoint observed by a structural adapter.
    Entrypoint,
    /// Test definition observed by a structural adapter.
    Test,
}

impl ModuleRuntimeRootKind {
    const fn symbol_role(self) -> SymbolRole {
        match self {
            Self::Entrypoint => SymbolRole::Entrypoint,
            Self::Test => SymbolRole::Test,
        }
    }
}

/// One current rank-ordered structural symbol proving an entrypoint or test root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeRoot {
    kind: ModuleRuntimeRootKind,
    rank: u16,
    symbol: GraphSymbol,
}

impl ModuleRuntimeRoot {
    /// Binds one V8 feature row to exact current structural symbol evidence.
    pub fn new(
        kind: ModuleRuntimeRootKind,
        rank: u16,
        symbol: GraphSymbol,
    ) -> Result<Self, ModuleRuntimeRootError> {
        if rank == 0
            || rank > MAX_RUNTIME_ROOTS
            || !symbol.parsed().roles().contains(kind.symbol_role())
        {
            return Err(ModuleRuntimeRootError);
        }
        Ok(Self { kind, rank, symbol })
    }

    /// Returns the role proven by both feature projection and parsed symbol.
    #[must_use]
    pub const fn kind(&self) -> ModuleRuntimeRootKind {
        self.kind
    }

    /// Returns the one-based deterministic rank in the complete stored prefix.
    #[must_use]
    pub const fn rank(&self) -> u16 {
        self.rank
    }

    /// Returns the exact current structural symbol.
    #[must_use]
    pub const fn symbol(&self) -> &GraphSymbol {
        &self.symbol
    }

    /// Returns the stable evidence-inspector identity for this exact symbol.
    #[must_use]
    pub fn evidence_id(&self) -> ModuleCardEvidenceId {
        ModuleCardEvidenceId::for_symbol_v1(&self.symbol)
    }
}

/// A runtime root contradicted its rank or adapter-proven role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRuntimeRootError;

impl fmt::Display for ModuleRuntimeRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module runtime root is inconsistent with its rank or symbol role")
    }
}

impl Error for ModuleRuntimeRootError {}

/// One role-specific visible prefix with separate storage- and formation-boundary signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeRootSet {
    kind: ModuleRuntimeRootKind,
    roots: Vec<ModuleRuntimeRoot>,
    stored_count: u16,
    projection_truncated: bool,
}

impl ModuleRuntimeRootSet {
    /// Validates contiguous rank order, identity uniqueness, and explicit truncation truth.
    pub fn new(
        kind: ModuleRuntimeRootKind,
        roots: Vec<ModuleRuntimeRoot>,
        stored_count: u16,
        projection_truncated: bool,
    ) -> Result<Self, ModuleRuntimeRootSetError> {
        let expected_ranks = 1_u16..;
        let symbols = roots
            .iter()
            .map(|root| root.symbol().id())
            .collect::<BTreeSet<_>>();
        if stored_count > MAX_RUNTIME_ROOTS
            || roots.len() > usize::from(stored_count)
            || roots.len() > usize::from(MAX_RUNTIME_ROOTS)
            || roots
                .iter()
                .zip(expected_ranks)
                .any(|(root, rank)| root.kind() != kind || root.rank() != rank)
            || symbols.len() != roots.len()
            || (projection_truncated && stored_count == 0)
        {
            return Err(ModuleRuntimeRootSetError);
        }
        Ok(Self {
            kind,
            roots,
            stored_count,
            projection_truncated,
        })
    }

    /// Returns the semantic class shared by every visible root.
    #[must_use]
    pub const fn kind(&self) -> ModuleRuntimeRootKind {
        self.kind
    }

    /// Returns the rank prefix selected by the current query.
    #[must_use]
    pub fn roots(&self) -> &[ModuleRuntimeRoot] {
        &self.roots
    }

    /// Returns the number of feature rows retained by deterministic module formation.
    #[must_use]
    pub const fn stored_count(&self) -> u16 {
        self.stored_count
    }

    /// Returns whether module formation omitted lower-ranked roots beyond storage.
    #[must_use]
    pub const fn projection_truncated(&self) -> bool {
        self.projection_truncated
    }

    /// Returns whether either the request or formation boundary hides additional roots.
    #[must_use]
    pub fn visible_truncated(&self) -> bool {
        self.roots.len() < usize::from(self.stored_count) || self.projection_truncated
    }
}

/// Stored runtime roots contradicted role, order, identity, count, or truncation invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRuntimeRootSetError;

impl fmt::Display for ModuleRuntimeRootSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module runtime root set is invalid")
    }
}

impl Error for ModuleRuntimeRootSetError {}

/// Atomic current module feature roots used to seed explicit runtime-flow reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeMap {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    module_id: ModuleId,
    entrypoints: ModuleRuntimeRootSet,
    tests: ModuleRuntimeRootSet,
}

impl ModuleRuntimeMap {
    /// Creates one run-bound map only from the two required feature classes.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        module_id: ModuleId,
        entrypoints: ModuleRuntimeRootSet,
        tests: ModuleRuntimeRootSet,
    ) -> Result<Self, ModuleRuntimeMapError> {
        if entrypoints.kind() != ModuleRuntimeRootKind::Entrypoint
            || tests.kind() != ModuleRuntimeRootKind::Test
        {
            return Err(ModuleRuntimeMapError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            module_id,
            entrypoints,
            tests,
        })
    }

    /// Returns the exact atomic publication behind roots and evidence.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the exact immutable snapshot behind roots and evidence.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the current primary module owning every root.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns adapter-proven program, library, or script roots.
    #[must_use]
    pub const fn entrypoints(&self) -> &ModuleRuntimeRootSet {
        &self.entrypoints
    }

    /// Returns adapter-proven test-definition roots.
    #[must_use]
    pub const fn tests(&self) -> &ModuleRuntimeRootSet {
        &self.tests
    }
}

/// A module runtime map swapped or mixed its required feature classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRuntimeMapError;

impl fmt::Display for ModuleRuntimeMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module runtime map has invalid feature classes")
    }
}

impl Error for ModuleRuntimeMapError {}

/// Result of reading the latest publication and optional V8 runtime-root projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleRuntimeMapLoadResult {
    /// No index has crossed the atomic publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic module projection.
    ProjectionUnavailable,
    /// The selected stable ID is absent or names a supplementary graph community.
    ModuleUnavailable,
    /// Current feature roots are available.
    Map(ModuleRuntimeMap),
}

/// Fixed evidence-graph preset permitted for one proven feature root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleRuntimeFlowKind {
    /// Follow syntactically observed calls from an entrypoint for at most two hops.
    EntrypointCalls,
    /// Follow direct test relationships from an adapter-proven test root.
    TestTargets,
}

impl ModuleRuntimeFlowKind {
    /// Returns the root role required before the graph boundary can be crossed.
    #[must_use]
    pub const fn root_kind(self) -> ModuleRuntimeRootKind {
        match self {
            Self::EntrypointCalls => ModuleRuntimeRootKind::Entrypoint,
            Self::TestTargets => ModuleRuntimeRootKind::Test,
        }
    }

    fn traversal(self, symbol_id: SymbolId, limit: TraversalResultLimit) -> TraversalQuery {
        match self {
            Self::EntrypointCalls => TraversalQuery::new(
                GraphEndpoint::Symbol(symbol_id),
                TraversalDirection::Outgoing,
                SyntaxRelationKind::Calls,
                TraversalDepth::INTERACTIVE_MAX,
                limit,
            ),
            Self::TestTargets => TraversalQuery::new(
                GraphEndpoint::Symbol(symbol_id),
                TraversalDirection::Outgoing,
                SyntaxRelationKind::Tests,
                TraversalDepth::DIRECT,
                limit,
            ),
        }
    }
}

/// Freshness- and role-bound request for one explicit runtime flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeFlowQuery {
    expected_index_run_id: IndexRunId,
    expected_snapshot_id: SnapshotId,
    module_id: ModuleId,
    root_symbol_id: SymbolId,
    kind: ModuleRuntimeFlowKind,
    result_limit: TraversalResultLimit,
}

impl ModuleRuntimeFlowQuery {
    /// Binds an allowed preset to the publication and root already visible to the user.
    #[must_use]
    pub const fn new(
        expected_index_run_id: IndexRunId,
        expected_snapshot_id: SnapshotId,
        module_id: ModuleId,
        root_symbol_id: SymbolId,
        kind: ModuleRuntimeFlowKind,
        result_limit: TraversalResultLimit,
    ) -> Self {
        Self {
            expected_index_run_id,
            expected_snapshot_id,
            module_id,
            root_symbol_id,
            kind,
            result_limit,
        }
    }

    /// Returns the published run expected by the visible root list.
    #[must_use]
    pub const fn expected_index_run_id(&self) -> IndexRunId {
        self.expected_index_run_id
    }

    /// Returns the immutable snapshot expected by the visible root list.
    #[must_use]
    pub const fn expected_snapshot_id(&self) -> SnapshotId {
        self.expected_snapshot_id
    }

    /// Returns the primary module that must own the root feature.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the content-bound structural root selected by the user.
    #[must_use]
    pub const fn root_symbol_id(&self) -> SymbolId {
        self.root_symbol_id
    }

    /// Returns the only fixed graph preset permitted for the root role.
    #[must_use]
    pub const fn kind(&self) -> ModuleRuntimeFlowKind {
        self.kind
    }

    /// Returns the requested bounded target count.
    #[must_use]
    pub const fn result_limit(&self) -> TraversalResultLimit {
        self.result_limit
    }

    fn traversal_query(&self) -> TraversalQuery {
        self.kind.traversal(self.root_symbol_id, self.result_limit)
    }
}

/// Result of validating a flow root against the latest atomic module projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRuntimeFlowRootValidation {
    /// No index has crossed the atomic publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic module projection.
    ProjectionUnavailable,
    /// Another publication replaced the root list visible to the caller.
    PublicationChanged,
    /// The selected primary module no longer exists.
    ModuleUnavailable,
    /// The symbol no longer has the role-specific feature membership in that module.
    RootUnavailable,
    /// The current run, snapshot, module, role, and symbol all match.
    Current,
}

/// One explicit flow result or a precise reason why the visible seed cannot be traversed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleRuntimeFlowLoadResult {
    /// No index has crossed the atomic publication boundary.
    NoPublishedIndex,
    /// The latest historical publication lacks required deterministic projections.
    ProjectionUnavailable,
    /// A publish occurred after the root list became visible.
    PublicationChanged,
    /// The selected primary module no longer exists.
    ModuleUnavailable,
    /// The selected symbol is not a current role-specific root of that module.
    RootUnavailable,
    /// Current shortest evidence paths were found, possibly with explicit truncation.
    Flow(GraphTraversalResult),
}

/// Cooperative cancellation and deterministic progress for module runtime reads.
pub trait ModuleRuntimeControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports bounded start, validation, traversal, and completion phases.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleRuntimeControlError>;
}

impl ModuleRuntimeControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleRuntimeControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ModuleRuntimeControlError::Unavailable)
    }
}

/// Module-runtime progress could not reach its owning runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRuntimeControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleRuntimeControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module runtime progress is unavailable")
    }
}

impl Error for ModuleRuntimeControlError {}

/// Owned future returned by the object-safe module-runtime storage port.
pub type ModuleRuntimeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ModuleRuntimeFailure>> + Send + 'a>>;

/// Narrow read-only access to current module feature roots and flow-root validation.
pub trait ModuleRuntimeStore: fmt::Debug + Send + Sync {
    /// Loads both role-specific prefixes from one atomic publication view.
    fn load_module_runtime_map<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleRuntimeMapQuery,
        control: &'a dyn ModuleRuntimeControl,
    ) -> ModuleRuntimeFuture<'a, ModuleRuntimeMapLoadResult>;

    /// Validates one visible root and expected publication immediately before traversal.
    fn validate_module_runtime_flow_root<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleRuntimeFlowQuery,
        control: &'a dyn ModuleRuntimeControl,
    ) -> ModuleRuntimeFuture<'a, ModuleRuntimeFlowRootValidation>;
}

/// Stable content-free failures for module-runtime reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRuntimeFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows or adapter results contradicted the runtime-map contract.
    InvalidStoredProjection,
    /// The owner cancelled before a complete result was delivered.
    Cancelled,
    /// A bounded local read exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleRuntimeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "module runtime storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module runtime projection is invalid")
            }
            Self::Cancelled => formatter.write_str("module runtime read was cancelled"),
            Self::TimedOut => formatter.write_str("module runtime read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("module runtime progress is unavailable")
            }
        }
    }
}

impl Error for ModuleRuntimeFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

/// Application use case for one atomic pair of entrypoint and test prefixes.
#[derive(Debug)]
pub struct GetModuleRuntimeMap {
    store: Arc<dyn ModuleRuntimeStore>,
}

impl GetModuleRuntimeMap {
    /// Wires the narrow V8 module-feature capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleRuntimeStore>) -> Self {
        Self { store }
    }

    /// Loads current roots while enforcing the caller's two visible limits.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleRuntimeMapQuery,
        control: &dyn ModuleRuntimeControl,
    ) -> Result<ModuleRuntimeMapLoadResult, ModuleRuntimeFailure> {
        report(control, 0, 2)?;
        cancelled(control)?;
        let result = self
            .store
            .load_module_runtime_map(project, query, control)
            .await?;
        if let ModuleRuntimeMapLoadResult::Map(map) = &result
            && (map.module_id() != query.module_id()
                || map.entrypoints().roots().len() > usize::from(query.entrypoint_limit().get())
                || map.tests().roots().len() > usize::from(query.test_limit().get()))
        {
            return Err(ModuleRuntimeFailure::InvalidStoredProjection);
        }
        cancelled(control)?;
        report(control, 2, 2)?;
        Ok(result)
    }
}

/// Application use case that permits only role-proven, freshness-bound R3 graph presets.
#[derive(Debug)]
pub struct TraceModuleRuntimeFlow {
    runtime_store: Arc<dyn ModuleRuntimeStore>,
    search_store: Arc<dyn KnowledgeSearchStore>,
}

impl TraceModuleRuntimeFlow {
    /// Wires current root validation to the existing deterministic graph traversal port.
    #[must_use]
    pub fn new(
        runtime_store: Arc<dyn ModuleRuntimeStore>,
        search_store: Arc<dyn KnowledgeSearchStore>,
    ) -> Self {
        Self {
            runtime_store,
            search_store,
        }
    }

    /// Validates role and freshness, traverses one fixed preset, then rechecks the result anchor.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleRuntimeFlowQuery,
        control: &dyn ModuleRuntimeControl,
    ) -> Result<ModuleRuntimeFlowLoadResult, ModuleRuntimeFailure> {
        report(control, 0, 3)?;
        cancelled(control)?;
        let validation = self
            .runtime_store
            .validate_module_runtime_flow_root(project, query, control)
            .await?;
        let early = match validation {
            ModuleRuntimeFlowRootValidation::NoPublishedIndex => {
                Some(ModuleRuntimeFlowLoadResult::NoPublishedIndex)
            }
            ModuleRuntimeFlowRootValidation::ProjectionUnavailable => {
                Some(ModuleRuntimeFlowLoadResult::ProjectionUnavailable)
            }
            ModuleRuntimeFlowRootValidation::PublicationChanged => {
                Some(ModuleRuntimeFlowLoadResult::PublicationChanged)
            }
            ModuleRuntimeFlowRootValidation::ModuleUnavailable => {
                Some(ModuleRuntimeFlowLoadResult::ModuleUnavailable)
            }
            ModuleRuntimeFlowRootValidation::RootUnavailable => {
                Some(ModuleRuntimeFlowLoadResult::RootUnavailable)
            }
            ModuleRuntimeFlowRootValidation::Current => None,
        };
        if let Some(result) = early {
            report(control, 3, 3)?;
            return Ok(result);
        }
        cancelled(control)?;
        report(control, 1, 3)?;
        let traversal_query = query.traversal_query();
        let search_control = RuntimeSearchControl(control);
        let traversal = match self
            .search_store
            .traverse_graph(project, &traversal_query, &search_control)
            .await
        {
            Ok(result) => result,
            Err(
                KnowledgeSearchFailure::IndexUnavailable | KnowledgeSearchFailure::SeedUnavailable,
            ) => {
                report(control, 3, 3)?;
                return Ok(ModuleRuntimeFlowLoadResult::PublicationChanged);
            }
            Err(KnowledgeSearchFailure::ProjectionUnavailable(_)) => {
                report(control, 3, 3)?;
                return Ok(ModuleRuntimeFlowLoadResult::ProjectionUnavailable);
            }
            Err(error) => return Err(map_search_failure(error)),
        };
        cancelled(control)?;
        report(control, 2, 3)?;
        if traversal.index_run_id() != query.expected_index_run_id()
            || traversal.snapshot_id() != query.expected_snapshot_id()
            || traversal.query() != &traversal_query
        {
            report(control, 3, 3)?;
            return Ok(ModuleRuntimeFlowLoadResult::PublicationChanged);
        }
        report(control, 3, 3)?;
        Ok(ModuleRuntimeFlowLoadResult::Flow(traversal))
    }
}

#[derive(Debug)]
struct RuntimeSearchControl<'a>(&'a dyn ModuleRuntimeControl);

impl KnowledgeSearchControl for RuntimeSearchControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn map_search_failure(error: KnowledgeSearchFailure) -> ModuleRuntimeFailure {
    match error {
        KnowledgeSearchFailure::Storage(error) => ModuleRuntimeFailure::Storage(error),
        KnowledgeSearchFailure::Cancelled => ModuleRuntimeFailure::Cancelled,
        KnowledgeSearchFailure::TimedOut => ModuleRuntimeFailure::TimedOut,
        KnowledgeSearchFailure::IndexUnavailable | KnowledgeSearchFailure::SeedUnavailable => {
            ModuleRuntimeFailure::InvalidStoredProjection
        }
        KnowledgeSearchFailure::InvalidCursor | KnowledgeSearchFailure::InvalidStoredProjection => {
            ModuleRuntimeFailure::InvalidStoredProjection
        }
        KnowledgeSearchFailure::ProjectionUnavailable(_) => {
            ModuleRuntimeFailure::InvalidStoredProjection
        }
    }
}

fn cancelled(control: &dyn ModuleRuntimeControl) -> Result<(), ModuleRuntimeFailure> {
    if control.is_cancelled() {
        Err(ModuleRuntimeFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn report(
    control: &dyn ModuleRuntimeControl,
    completed: u64,
    total: u64,
) -> Result<(), ModuleRuntimeFailure> {
    control
        .report_progress(
            Progress::determinate(completed, total)
                .map_err(|_| ModuleRuntimeFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleRuntimeFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        GetModuleRuntimeMap, ModuleRuntimeControl, ModuleRuntimeControlError, ModuleRuntimeFailure,
        ModuleRuntimeFlowKind, ModuleRuntimeFlowLoadResult, ModuleRuntimeFlowQuery,
        ModuleRuntimeFlowRootValidation, ModuleRuntimeFuture, ModuleRuntimeMap,
        ModuleRuntimeMapLoadResult, ModuleRuntimeMapQuery, ModuleRuntimeRoot,
        ModuleRuntimeRootKind, ModuleRuntimeRootLimit, ModuleRuntimeRootSet, ModuleRuntimeStore,
        TraceModuleRuntimeFlow,
    };
    use crate::{
        KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    };
    use a3_domain::{
        CanonicalDirectory, Confidence, ContentHash, EvidenceRef, ExactSearchCursor,
        ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ExactSearchSymbol,
        ExactSearchTarget, FileRevision, GitHead, GitReferenceName, GraphEdge, GraphEndpoint,
        GraphSymbol, GraphTraversalHit, GraphTraversalResult, IndexRunId, LexicalSearchCursor,
        LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery, LinkResolution,
        LocalSymbolId, ModuleId, ParsedSymbol, Progress, ProjectIdentity, QualifiedSymbolName,
        RepositoryId, RepositoryIdentity, RepositoryPath, SnapshotId, SourceChannel,
        SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRole, SyntaxProvider,
        TraversalQuery, TraversalResultLimit, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RuntimeStore {
        map: ModuleRuntimeMap,
        validation: ModuleRuntimeFlowRootValidation,
    }

    impl ModuleRuntimeStore for RuntimeStore {
        fn load_module_runtime_map<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ModuleRuntimeMapQuery,
            _control: &'a dyn ModuleRuntimeControl,
        ) -> ModuleRuntimeFuture<'a, ModuleRuntimeMapLoadResult> {
            Box::pin(async { Ok(ModuleRuntimeMapLoadResult::Map(self.map.clone())) })
        }

        fn validate_module_runtime_flow_root<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ModuleRuntimeFlowQuery,
            _control: &'a dyn ModuleRuntimeControl,
        ) -> ModuleRuntimeFuture<'a, ModuleRuntimeFlowRootValidation> {
            Box::pin(async { Ok(self.validation) })
        }
    }

    #[derive(Debug)]
    struct SearchStore {
        run_id: IndexRunId,
        snapshot_id: SnapshotId,
        failure: Option<KnowledgeSearchFailure>,
    }

    impl KnowledgeSearchStore for SearchStore {
        fn search_exact<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ExactSearchQuery,
            _page_size: ExactSearchPageSize,
            _cursor: Option<&'a ExactSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }

        fn search_lexical<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a LexicalSearchQuery,
            _page_size: LexicalSearchPageSize,
            _cursor: Option<&'a LexicalSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::InvalidStoredProjection) })
        }

        fn traverse_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a TraversalQuery,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
            Box::pin(async move {
                if let Some(error) = self.failure {
                    return Err(error);
                }
                traversal(self.run_id, self.snapshot_id, query)
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
            })
        }
    }

    #[derive(Debug, Default)]
    struct Control {
        progress: Mutex<Vec<Progress>>,
        cancelled: bool,
    }

    impl ModuleRuntimeControl for Control {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(&self, progress: Progress) -> Result<(), ModuleRuntimeControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleRuntimeControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn roots_require_contiguous_rank_role_and_unique_identity() -> Result<(), Box<dyn Error>> {
        let first = symbol(
            [1; 32],
            [11; 32],
            b"src/main.rs",
            "main",
            SymbolRole::Entrypoint,
        )?;
        let second = symbol(
            [2; 32],
            [12; 32],
            b"src/lib.rs",
            "run",
            SymbolRole::Entrypoint,
        )?;
        assert!(ModuleRuntimeRoot::new(ModuleRuntimeRootKind::Test, 1, first.clone()).is_err());
        assert!(
            ModuleRuntimeRootSet::new(
                ModuleRuntimeRootKind::Entrypoint,
                vec![
                    ModuleRuntimeRoot::new(ModuleRuntimeRootKind::Entrypoint, 1, first.clone(),)?,
                    ModuleRuntimeRoot::new(ModuleRuntimeRootKind::Entrypoint, 3, second)?,
                ],
                2,
                false,
            )
            .is_err()
        );
        assert_eq!(
            ModuleRuntimeRoot::new(ModuleRuntimeRootKind::Entrypoint, 1, first)?
                .evidence_id()
                .as_bytes()
                .len(),
            32
        );
        Ok(())
    }

    #[test]
    fn map_use_case_rejects_a_store_prefix_above_the_requested_limit() -> Result<(), Box<dyn Error>>
    {
        let map = map()?;
        let module_id = map.module_id();
        let result = block_on(
            GetModuleRuntimeMap::new(Arc::new(RuntimeStore {
                map,
                validation: ModuleRuntimeFlowRootValidation::Current,
            }))
            .execute(
                &project()?,
                &ModuleRuntimeMapQuery::new(
                    module_id,
                    ModuleRuntimeRootLimit::new(1)?,
                    ModuleRuntimeRootLimit::DEFAULT,
                ),
                &Control::default(),
            ),
        );
        assert_eq!(result, Err(ModuleRuntimeFailure::InvalidStoredProjection));
        Ok(())
    }

    #[test]
    fn flow_uses_only_the_fixed_preset_and_matching_publication() -> Result<(), Box<dyn Error>> {
        let map = map()?;
        let run_id = map.index_run_id();
        let snapshot_id = map.snapshot_id();
        let module_id = map.module_id();
        let root_id = map.entrypoints().roots()[0].symbol().id();
        let control = Control::default();
        let result = block_on(
            TraceModuleRuntimeFlow::new(
                Arc::new(RuntimeStore {
                    map,
                    validation: ModuleRuntimeFlowRootValidation::Current,
                }),
                Arc::new(SearchStore {
                    run_id,
                    snapshot_id,
                    failure: None,
                }),
            )
            .execute(
                &project()?,
                &ModuleRuntimeFlowQuery::new(
                    run_id,
                    snapshot_id,
                    module_id,
                    root_id,
                    ModuleRuntimeFlowKind::EntrypointCalls,
                    TraversalResultLimit::DEFAULT,
                ),
                &control,
            ),
        )?;
        let ModuleRuntimeFlowLoadResult::Flow(flow) = result else {
            return Err("expected current flow".into());
        };
        assert_eq!(flow.hits().len(), 1);
        assert_eq!(flow.hits()[0].path().len(), 1);
        assert_eq!(control.progress.lock().map_err(|_| "poisoned")?.len(), 4);
        Ok(())
    }

    #[test]
    fn flow_reports_publication_change_instead_of_mixing_snapshots() -> Result<(), Box<dyn Error>> {
        let map = map()?;
        let query = ModuleRuntimeFlowQuery::new(
            map.index_run_id(),
            map.snapshot_id(),
            map.module_id(),
            map.entrypoints().roots()[0].symbol().id(),
            ModuleRuntimeFlowKind::EntrypointCalls,
            TraversalResultLimit::DEFAULT,
        );
        let result = block_on(
            TraceModuleRuntimeFlow::new(
                Arc::new(RuntimeStore {
                    map,
                    validation: ModuleRuntimeFlowRootValidation::Current,
                }),
                Arc::new(SearchStore {
                    run_id: IndexRunId::from_bytes([99; 32]),
                    snapshot_id: SnapshotId::from_bytes([98; 32]),
                    failure: None,
                }),
            )
            .execute(&project()?, &query, &Control::default()),
        )?;
        assert_eq!(result, ModuleRuntimeFlowLoadResult::PublicationChanged);
        Ok(())
    }

    #[test]
    fn flow_reports_a_missing_graph_projection_without_inventing_evidence()
    -> Result<(), Box<dyn Error>> {
        let map = map()?;
        let query = ModuleRuntimeFlowQuery::new(
            map.index_run_id(),
            map.snapshot_id(),
            map.module_id(),
            map.entrypoints().roots()[0].symbol().id(),
            ModuleRuntimeFlowKind::EntrypointCalls,
            TraversalResultLimit::DEFAULT,
        );
        let result = block_on(
            TraceModuleRuntimeFlow::new(
                Arc::new(RuntimeStore {
                    map,
                    validation: ModuleRuntimeFlowRootValidation::Current,
                }),
                Arc::new(SearchStore {
                    run_id: query.expected_index_run_id(),
                    snapshot_id: query.expected_snapshot_id(),
                    failure: Some(KnowledgeSearchFailure::ProjectionUnavailable(
                        SourceChannel::Graph,
                    )),
                }),
            )
            .execute(&project()?, &query, &Control::default()),
        )?;
        assert_eq!(result, ModuleRuntimeFlowLoadResult::ProjectionUnavailable);
        Ok(())
    }

    fn map() -> Result<ModuleRuntimeMap, Box<dyn Error>> {
        let entrypoints = ModuleRuntimeRootSet::new(
            ModuleRuntimeRootKind::Entrypoint,
            vec![
                ModuleRuntimeRoot::new(
                    ModuleRuntimeRootKind::Entrypoint,
                    1,
                    symbol(
                        [1; 32],
                        [11; 32],
                        b"src/main.rs",
                        "main",
                        SymbolRole::Entrypoint,
                    )?,
                )?,
                ModuleRuntimeRoot::new(
                    ModuleRuntimeRootKind::Entrypoint,
                    2,
                    symbol(
                        [2; 32],
                        [12; 32],
                        b"src/bin.rs",
                        "serve",
                        SymbolRole::Entrypoint,
                    )?,
                )?,
            ],
            2,
            false,
        )?;
        let tests = ModuleRuntimeRootSet::new(
            ModuleRuntimeRootKind::Test,
            vec![ModuleRuntimeRoot::new(
                ModuleRuntimeRootKind::Test,
                1,
                symbol(
                    [3; 32],
                    [13; 32],
                    b"tests/runtime.rs",
                    "runtime_works",
                    SymbolRole::Test,
                )?,
            )?],
            1,
            false,
        )?;
        Ok(ModuleRuntimeMap::new(
            IndexRunId::from_bytes([20; 32]),
            SnapshotId::from_bytes([21; 32]),
            ModuleId::from_bytes([22; 32]),
            entrypoints,
            tests,
        )?)
    }

    fn traversal(
        run_id: IndexRunId,
        snapshot_id: SnapshotId,
        query: &TraversalQuery,
    ) -> Result<GraphTraversalResult, Box<dyn Error>> {
        let GraphEndpoint::Symbol(source_id) = query.start() else {
            return Err("expected symbol flow seed".into());
        };
        let source = symbol(
            source_id.as_bytes().to_owned(),
            [31; 32],
            b"src/main.rs",
            "main",
            SymbolRole::Entrypoint,
        )?;
        let target = symbol(
            [32; 32],
            [33; 32],
            b"src/runtime.rs",
            "run",
            SymbolRole::Entrypoint,
        )?;
        let range = source.parsed().selection_range();
        let edge = GraphEdge::new(
            GraphEndpoint::Symbol(source.id()),
            GraphEndpoint::Symbol(target.id()),
            query.relation(),
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterLocalSymbol,
            snapshot_id,
            EvidenceRef::new(source.revision().clone(), range),
        );
        let target = ExactSearchTarget::Symbol(ExactSearchSymbol::new(
            target,
            QualifiedSymbolName::try_from_string("src::runtime::run".to_owned())?,
        ));
        let hit = GraphTraversalHit::new(target, vec![edge], query, snapshot_id)?;
        Ok(GraphTraversalResult::new(
            run_id,
            snapshot_id,
            query.clone(),
            vec![hit],
            false,
        )?)
    }

    fn symbol(
        symbol_id: impl Into<[u8; 32]>,
        hash: [u8; 32],
        path: &[u8],
        name: &str,
        role: SymbolRole,
    ) -> Result<GraphSymbol, Box<dyn Error>> {
        let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
        Ok(GraphSymbol::new(
            SymbolId::from_bytes(symbol_id.into()),
            FileRevision::new(
                RepositoryPath::try_from_bytes(path.to_vec())?,
                ContentHash::from_bytes(hash),
            ),
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string(name.to_owned())?,
                range,
                range,
            )?
            .with_role(role),
        ))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let root = CanonicalDirectory::from_canonicalized(root)?;
        let repository_id = RepositoryId::from_bytes([40; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([41; 32]),
                WorktreeAnchorId::from_bytes([42; 32]),
                repository_id,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
