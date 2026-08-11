use crate::{JobContext, KnowledgeStoreFailure, ModuleTreeDisplayName, ModuleTreeEntryKind};
use a3_domain::{
    FileRevision, GraphEdge, IndexRunId, ModuleCardEvidenceId, ModuleId, ModuleKind, ModuleRoot,
    Progress, ProjectIdentity, SnapshotId, SyntaxRelationKind,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const DEFAULT_NODE_LIMIT: u16 = 50;
const MAX_NODE_LIMIT: u16 = 100;
const MAX_INSPECTED_EDGES: u64 = 4_096;
const MAX_VISIBLE_EDGE_GROUPS: usize = 256;

/// Positive maximum number of module nodes rendered by one project-map query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDependencyNodeLimit(u16);

impl ModuleDependencyNodeLimit {
    /// Product default chosen for a useful but still readable local graph.
    pub const DEFAULT: Self = Self(DEFAULT_NODE_LIMIT);

    /// Accepts one center node through one hundred total visible nodes.
    pub fn new(value: u16) -> Result<Self, ModuleDependencyNodeLimitError> {
        if value == 0 || value > MAX_NODE_LIMIT {
            return Err(ModuleDependencyNodeLimitError);
        }
        Ok(Self(value))
    }

    /// Returns the validated total node boundary.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A dependency-graph node limit was zero or exceeded the fixed UI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDependencyNodeLimitError;

impl fmt::Display for ModuleDependencyNodeLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module dependency node limit must be between one and one hundred")
    }
}

impl Error for ModuleDependencyNodeLimitError {}

/// Validated direct-neighborhood query around one deterministic primary module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyGraphQuery {
    center_module_id: ModuleId,
    node_limit: ModuleDependencyNodeLimit,
}

impl ModuleDependencyGraphQuery {
    /// Creates a bounded query whose limit includes the center module.
    #[must_use]
    pub const fn new(center_module_id: ModuleId, node_limit: ModuleDependencyNodeLimit) -> Self {
        Self {
            center_module_id,
            node_limit,
        }
    }

    /// Returns the selected current primary module.
    #[must_use]
    pub const fn center_module_id(&self) -> ModuleId {
        self.center_module_id
    }

    /// Returns the maximum total number of visible nodes.
    #[must_use]
    pub const fn node_limit(&self) -> ModuleDependencyNodeLimit {
        self.node_limit
    }
}

/// One deterministic primary module participating in the bounded graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyNode {
    module_id: ModuleId,
    kind: ModuleTreeEntryKind,
    root: ModuleRoot,
    display_name: ModuleTreeDisplayName,
    representative_revision: Option<FileRevision>,
}

impl ModuleDependencyNode {
    /// Constructs a navigable primary-module node with optional current boundary evidence.
    pub fn new(
        module_id: ModuleId,
        kind: ModuleKind,
        root: ModuleRoot,
        representative_revision: Option<FileRevision>,
    ) -> Result<Self, ModuleDependencyNodeError> {
        let kind = match kind {
            ModuleKind::ManifestBoundary => ModuleTreeEntryKind::ManifestBoundary,
            ModuleKind::PathBoundary => ModuleTreeEntryKind::PathBoundary,
            ModuleKind::GraphCommunity => return Err(ModuleDependencyNodeError),
        };
        let display_name = ModuleTreeDisplayName::from_root(&root);
        Ok(Self {
            module_id,
            kind,
            root,
            display_name,
            representative_revision,
        })
    }

    /// Returns the deterministic primary-module identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the manifest- or path-derived primary boundary kind.
    #[must_use]
    pub const fn kind(&self) -> ModuleTreeEntryKind {
        self.kind
    }

    /// Returns the canonical repository-relative module root.
    #[must_use]
    pub const fn root(&self) -> &ModuleRoot {
        &self.root
    }

    /// Returns bounded display-only text derived from the root.
    #[must_use]
    pub const fn display_name(&self) -> &ModuleTreeDisplayName {
        &self.display_name
    }

    /// Returns one current member revision when the module contains structural symbols.
    #[must_use]
    pub const fn representative_revision(&self) -> Option<&FileRevision> {
        self.representative_revision.as_ref()
    }

    /// Returns the stable Evidence Inspector identity of the representative revision.
    #[must_use]
    pub fn representative_evidence_id(&self) -> Option<ModuleCardEvidenceId> {
        self.representative_revision
            .as_ref()
            .map(ModuleCardEvidenceId::for_file_revision_v1)
    }
}

/// A stored dependency node was not a deterministic primary module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDependencyNodeError;

impl fmt::Display for ModuleDependencyNodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module dependency node is not a valid primary module")
    }
}

impl Error for ModuleDependencyNodeError {}

/// A non-hierarchy syntax relation that can connect two primary modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleDependencyRelation {
    /// Import relationship.
    Imports,
    /// Export relationship.
    Exports,
    /// Syntactically visible call relationship.
    Calls,
    /// Trait or interface implementation.
    Implements,
    /// Type extension or inheritance.
    Extends,
    /// Read relationship.
    Reads,
    /// Write relationship.
    Writes,
    /// Configuration relationship.
    Configures,
    /// Test relationship.
    Tests,
    /// Build relationship.
    Builds,
    /// Documentation relationship.
    Documents,
}

impl ModuleDependencyRelation {
    fn from_graph(kind: SyntaxRelationKind) -> Option<Self> {
        match kind {
            SyntaxRelationKind::Contains | SyntaxRelationKind::Defines => None,
            SyntaxRelationKind::Imports => Some(Self::Imports),
            SyntaxRelationKind::Exports => Some(Self::Exports),
            SyntaxRelationKind::Calls => Some(Self::Calls),
            SyntaxRelationKind::Implements => Some(Self::Implements),
            SyntaxRelationKind::Extends => Some(Self::Extends),
            SyntaxRelationKind::Reads => Some(Self::Reads),
            SyntaxRelationKind::Writes => Some(Self::Writes),
            SyntaxRelationKind::Configures => Some(Self::Configures),
            SyntaxRelationKind::Tests => Some(Self::Tests),
            SyntaxRelationKind::Builds => Some(Self::Builds),
            SyntaxRelationKind::Documents => Some(Self::Documents),
        }
    }
}

/// One relation-specific cross-module dependency with exact representative evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyEdge {
    source_module_id: ModuleId,
    target_module_id: ModuleId,
    relation: ModuleDependencyRelation,
    observed_evidence_count: u64,
    representative_edge: GraphEdge,
}

impl ModuleDependencyEdge {
    /// Creates an observed dependency while rejecting hierarchy and self-module edges.
    pub fn new(
        source_module_id: ModuleId,
        target_module_id: ModuleId,
        relation: SyntaxRelationKind,
        observed_evidence_count: u64,
        representative_edge: GraphEdge,
    ) -> Result<Self, ModuleDependencyEdgeError> {
        let relation =
            ModuleDependencyRelation::from_graph(relation).ok_or(ModuleDependencyEdgeError)?;
        if source_module_id == target_module_id
            || observed_evidence_count == 0
            || ModuleDependencyRelation::from_graph(representative_edge.kind()) != Some(relation)
        {
            return Err(ModuleDependencyEdgeError);
        }
        Ok(Self {
            source_module_id,
            target_module_id,
            relation,
            observed_evidence_count,
            representative_edge,
        })
    }

    /// Returns the primary source module.
    #[must_use]
    pub const fn source_module_id(&self) -> ModuleId {
        self.source_module_id
    }

    /// Returns the primary target module.
    #[must_use]
    pub const fn target_module_id(&self) -> ModuleId {
        self.target_module_id
    }

    /// Returns the language-neutral graph relation.
    #[must_use]
    pub const fn relation(&self) -> ModuleDependencyRelation {
        self.relation
    }

    /// Returns the number of matching evidence edges in the inspected canonical prefix.
    #[must_use]
    pub const fn observed_evidence_count(&self) -> u64 {
        self.observed_evidence_count
    }

    /// Returns the first canonical exact edge supporting this observed group.
    #[must_use]
    pub const fn representative_edge(&self) -> &GraphEdge {
        &self.representative_edge
    }

    /// Returns the stable Evidence Inspector identity of the representative edge.
    #[must_use]
    pub fn evidence_id(&self) -> ModuleCardEvidenceId {
        ModuleCardEvidenceId::for_graph_edge_v1(&self.representative_edge)
    }
}

/// A module dependency contradicted relation, count, or endpoint invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDependencyEdgeError;

impl fmt::Display for ModuleDependencyEdgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module dependency edge is invalid")
    }
}

impl Error for ModuleDependencyEdgeError {}

/// Evidence-bound direct module neighborhood from one atomic publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyGraph {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    center_module_id: ModuleId,
    nodes: Vec<ModuleDependencyNode>,
    observed_neighbor_count: u64,
    nodes_truncated: bool,
    edges: Vec<ModuleDependencyEdge>,
    observed_edge_group_count: u64,
    edges_truncated: bool,
    inspected_edge_count: u64,
    source_edges_truncated: bool,
    unmapped_edge_count: u64,
}

impl ModuleDependencyGraph {
    /// Validates bounds, canonical ordering, center incidence, and completeness signals.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        center_module_id: ModuleId,
        nodes: Vec<ModuleDependencyNode>,
        observed_neighbor_count: u64,
        nodes_truncated: bool,
        edges: Vec<ModuleDependencyEdge>,
        observed_edge_group_count: u64,
        edges_truncated: bool,
        inspected_edge_count: u64,
        source_edges_truncated: bool,
        unmapped_edge_count: u64,
    ) -> Result<Self, ModuleDependencyGraphError> {
        let visible_neighbor_count = nodes.len().saturating_sub(1);
        let visible_evidence_count = edges.iter().try_fold(0_u64, |count, edge| {
            count.checked_add(edge.observed_evidence_count())
        });
        if nodes.is_empty()
            || nodes.len() > usize::from(MAX_NODE_LIMIT)
            || nodes
                .windows(2)
                .any(|pair| pair[0].module_id() >= pair[1].module_id())
            || nodes
                .iter()
                .filter(|node| node.module_id() == center_module_id)
                .count()
                != 1
            || observed_neighbor_count
                < u64::try_from(visible_neighbor_count).map_err(|_| ModuleDependencyGraphError)?
            || nodes_truncated
                != (observed_neighbor_count
                    > u64::try_from(visible_neighbor_count)
                        .map_err(|_| ModuleDependencyGraphError)?)
            || edges.len() > MAX_VISIBLE_EDGE_GROUPS
            || observed_edge_group_count
                < u64::try_from(edges.len()).map_err(|_| ModuleDependencyGraphError)?
            || edges_truncated
                != (observed_edge_group_count
                    > u64::try_from(edges.len()).map_err(|_| ModuleDependencyGraphError)?)
            || inspected_edge_count > MAX_INSPECTED_EDGES
            || observed_neighbor_count > inspected_edge_count
            || observed_edge_group_count > inspected_edge_count
            || visible_evidence_count.is_none_or(|count| count > inspected_edge_count)
            || (source_edges_truncated && inspected_edge_count != MAX_INSPECTED_EDGES)
            || unmapped_edge_count > inspected_edge_count
        {
            return Err(ModuleDependencyGraphError);
        }

        let node_ids = nodes
            .iter()
            .map(ModuleDependencyNode::module_id)
            .collect::<BTreeSet<_>>();
        let mut previous_key = None;
        for edge in &edges {
            let key = (
                edge.source_module_id(),
                edge.target_module_id(),
                edge.relation(),
            );
            if !node_ids.contains(&edge.source_module_id())
                || !node_ids.contains(&edge.target_module_id())
                || (edge.source_module_id() != center_module_id
                    && edge.target_module_id() != center_module_id)
                || edge.representative_edge().snapshot_id() != snapshot_id
                || previous_key.is_some_and(|previous| previous >= key)
            {
                return Err(ModuleDependencyGraphError);
            }
            previous_key = Some(key);
        }

        Ok(Self {
            index_run_id,
            snapshot_id,
            center_module_id,
            nodes,
            observed_neighbor_count,
            nodes_truncated,
            edges,
            observed_edge_group_count,
            edges_truncated,
            inspected_edge_count,
            source_edges_truncated,
            unmapped_edge_count,
        })
    }

    /// Returns the exact publication run behind all nodes and edges.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot behind every representative edge.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the requested primary center module.
    #[must_use]
    pub const fn center_module_id(&self) -> ModuleId {
        self.center_module_id
    }

    /// Returns at most one hundred canonically ordered primary modules.
    #[must_use]
    pub fn nodes(&self) -> &[ModuleDependencyNode] {
        &self.nodes
    }

    /// Returns the distinct neighbors observed before the node limit was applied.
    #[must_use]
    pub const fn observed_neighbor_count(&self) -> u64 {
        self.observed_neighbor_count
    }

    /// Returns whether lower-ranked observed neighbors were omitted.
    #[must_use]
    pub const fn nodes_truncated(&self) -> bool {
        self.nodes_truncated
    }

    /// Returns at most 256 canonically ordered relation-specific dependency groups.
    #[must_use]
    pub fn edges(&self) -> &[ModuleDependencyEdge] {
        &self.edges
    }

    /// Returns the dependency groups observed between the selected nodes before edge limiting.
    #[must_use]
    pub const fn observed_edge_group_count(&self) -> u64 {
        self.observed_edge_group_count
    }

    /// Returns whether lower-ranked observed dependency groups were omitted.
    #[must_use]
    pub const fn edges_truncated(&self) -> bool {
        self.edges_truncated
    }

    /// Returns the number of current graph edges inspected for this neighborhood.
    #[must_use]
    pub const fn inspected_edge_count(&self) -> u64 {
        self.inspected_edge_count
    }

    /// Returns whether more incident graph edges existed beyond the canonical inspection bound.
    #[must_use]
    pub const fn source_edges_truncated(&self) -> bool {
        self.source_edges_truncated
    }

    /// Returns inspected edges whose endpoint could not map uniquely to a primary module.
    #[must_use]
    pub const fn unmapped_edge_count(&self) -> u64 {
        self.unmapped_edge_count
    }
}

/// Stored rows contradicted the bounded module dependency graph contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDependencyGraphError;

impl fmt::Display for ModuleDependencyGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module dependency graph shape is invalid")
    }
}

impl Error for ModuleDependencyGraphError {}

/// Result of reading the latest publication and its optional V8 module projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleDependencyGraphLoadResult {
    /// No index has crossed the atomic publish boundary.
    NoPublishedIndex,
    /// The current historical publication predates deterministic module projection.
    ProjectionUnavailable,
    /// The selected primary module is absent from the current projection.
    CenterUnavailable,
    /// One current bounded direct neighborhood is available.
    Graph(ModuleDependencyGraph),
}

/// Cooperative cancellation and deterministic progress for one dependency-graph read.
pub trait ModuleDependencyGraphControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports start and completion to the owning runtime.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleDependencyGraphControlError>;
}

impl ModuleDependencyGraphControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleDependencyGraphControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ModuleDependencyGraphControlError::Unavailable)
    }
}

/// Dependency-graph progress could not reach its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleDependencyGraphControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleDependencyGraphControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module dependency graph progress is unavailable")
    }
}

impl Error for ModuleDependencyGraphControlError {}

/// Owned future returned by the object-safe dependency-graph port.
pub type ModuleDependencyGraphFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ModuleDependencyGraphLoadResult, ModuleDependencyGraphFailure>>
            + Send
            + 'a,
    >,
>;

/// Narrow read-only capability for one current bounded module neighborhood.
pub trait ModuleDependencyGraphStore: fmt::Debug + Send + Sync {
    /// Loads only the selected direct neighborhood, never the complete published graph.
    fn load_module_dependency_graph<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleDependencyGraphQuery,
        control: &'a dyn ModuleDependencyGraphControl,
    ) -> ModuleDependencyGraphFuture<'a>;
}

/// Stable content-free failure classes for module dependency reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleDependencyGraphFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted current module or evidence-graph invariants.
    InvalidStoredProjection,
    /// The owner cancelled before a result was delivered.
    Cancelled,
    /// The bounded adapter query exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleDependencyGraphFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "module dependency storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module dependency projection is invalid")
            }
            Self::Cancelled => formatter.write_str("module dependency read was cancelled"),
            Self::TimedOut => formatter.write_str("module dependency read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("module dependency progress is unavailable")
            }
        }
    }
}

impl Error for ModuleDependencyGraphFailure {
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

/// Application use case retaining cancellation and progress outside persistence.
#[derive(Debug)]
pub struct GetModuleDependencyGraph {
    store: Arc<dyn ModuleDependencyGraphStore>,
}

impl GetModuleDependencyGraph {
    /// Wires the narrow progressive dependency-graph capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleDependencyGraphStore>) -> Self {
        Self { store }
    }

    /// Reads one current neighborhood or an explicit availability state.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleDependencyGraphQuery,
        control: &dyn ModuleDependencyGraphControl,
    ) -> Result<ModuleDependencyGraphLoadResult, ModuleDependencyGraphFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleDependencyGraphFailure::Cancelled);
        }
        let result = self
            .store
            .load_module_dependency_graph(project, query, control)
            .await?;
        if let ModuleDependencyGraphLoadResult::Graph(graph) = &result
            && (graph.center_module_id() != query.center_module_id()
                || graph.nodes().len() > usize::from(query.node_limit().get()))
        {
            return Err(ModuleDependencyGraphFailure::InvalidStoredProjection);
        }
        if control.is_cancelled() {
            return Err(ModuleDependencyGraphFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

fn report(
    control: &dyn ModuleDependencyGraphControl,
    completed: u64,
) -> Result<(), ModuleDependencyGraphFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleDependencyGraphFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleDependencyGraphFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        GetModuleDependencyGraph, ModuleDependencyEdge, ModuleDependencyGraph,
        ModuleDependencyGraphControl, ModuleDependencyGraphControlError,
        ModuleDependencyGraphFailure, ModuleDependencyGraphFuture, ModuleDependencyGraphLoadResult,
        ModuleDependencyGraphQuery, ModuleDependencyGraphStore, ModuleDependencyNode,
        ModuleDependencyNodeLimit,
    };
    use a3_domain::{
        CanonicalDirectory, Confidence, ContentHash, EvidenceRef, FileRevision, GitHead,
        GitReferenceName, GraphEdge, GraphEndpoint, IndexRunId, LinkResolution, ModuleId,
        ModuleKind, ModuleRoot, Progress, ProjectIdentity, RepositoryId, RepositoryIdentity,
        RepositoryPath, SnapshotId, SourcePosition, SourceRange, SyntaxProvider,
        SyntaxRelationKind, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingStore;

    impl ModuleDependencyGraphStore for RecordingStore {
        fn load_module_dependency_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a ModuleDependencyGraphQuery,
            _control: &'a dyn ModuleDependencyGraphControl,
        ) -> ModuleDependencyGraphFuture<'a> {
            Box::pin(async move {
                graph(query.center_module_id())
                    .map(ModuleDependencyGraphLoadResult::Graph)
                    .map_err(|_| ModuleDependencyGraphFailure::InvalidStoredProjection)
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        progress: Mutex<Vec<Progress>>,
        cancelled: bool,
    }

    impl ModuleDependencyGraphControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(
            &self,
            progress: Progress,
        ) -> Result<(), ModuleDependencyGraphControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleDependencyGraphControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn use_case_reports_progress_and_returns_evidence_bound_graph() -> Result<(), Box<dyn Error>> {
        let control = RecordingControl::default();
        let query = ModuleDependencyGraphQuery::new(
            ModuleId::from_bytes([1; 32]),
            ModuleDependencyNodeLimit::DEFAULT,
        );
        let result = block_on(
            GetModuleDependencyGraph::new(Arc::new(RecordingStore)).execute(
                &project()?,
                &query,
                &control,
            ),
        )?;
        let ModuleDependencyGraphLoadResult::Graph(graph) = result else {
            return Err("expected graph".into());
        };
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].evidence_id().as_bytes().len(), 32);
        assert_eq!(control.progress.lock().map_err(|_| "poisoned")?.len(), 2);
        Ok(())
    }

    #[test]
    fn graph_rejects_missing_center_and_contradictory_evidence_bounds() -> Result<(), Box<dyn Error>>
    {
        let value = graph(ModuleId::from_bytes([1; 32]))?;
        assert!(
            ModuleDependencyGraph::new(
                value.index_run_id,
                value.snapshot_id,
                ModuleId::from_bytes([9; 32]),
                value.nodes.clone(),
                value.observed_neighbor_count,
                value.nodes_truncated,
                value.edges.clone(),
                value.observed_edge_group_count,
                value.edges_truncated,
                value.inspected_edge_count,
                value.source_edges_truncated,
                value.unmapped_edge_count,
            )
            .is_err()
        );
        assert!(
            ModuleDependencyGraph::new(
                value.index_run_id,
                value.snapshot_id,
                value.center_module_id,
                value.nodes,
                value.observed_neighbor_count,
                value.nodes_truncated,
                value.edges,
                value.observed_edge_group_count,
                value.edges_truncated,
                0,
                false,
                value.unmapped_edge_count,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn cancellation_stops_before_storage() -> Result<(), Box<dyn Error>> {
        let control = RecordingControl {
            progress: Mutex::new(Vec::new()),
            cancelled: true,
        };
        let result = block_on(
            GetModuleDependencyGraph::new(Arc::new(RecordingStore)).execute(
                &project()?,
                &ModuleDependencyGraphQuery::new(
                    ModuleId::from_bytes([1; 32]),
                    ModuleDependencyNodeLimit::DEFAULT,
                ),
                &control,
            ),
        );
        assert_eq!(result, Err(ModuleDependencyGraphFailure::Cancelled));
        Ok(())
    }

    #[test]
    fn use_case_rejects_a_store_result_above_the_requested_node_limit() -> Result<(), Box<dyn Error>>
    {
        let query = ModuleDependencyGraphQuery::new(
            ModuleId::from_bytes([1; 32]),
            ModuleDependencyNodeLimit::new(1)?,
        );
        let result = block_on(
            GetModuleDependencyGraph::new(Arc::new(RecordingStore)).execute(
                &project()?,
                &query,
                &RecordingControl::default(),
            ),
        );
        assert_eq!(
            result,
            Err(ModuleDependencyGraphFailure::InvalidStoredProjection)
        );
        Ok(())
    }

    fn graph(center: ModuleId) -> Result<ModuleDependencyGraph, Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([3; 32]);
        let source = revision("src/lib.rs", 4)?;
        let target = revision("tools/lib.rs", 5)?;
        let edge = GraphEdge::new(
            GraphEndpoint::File(source.path().clone()),
            GraphEndpoint::File(target.path().clone()),
            SyntaxRelationKind::Imports,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterFile,
            snapshot_id,
            EvidenceRef::new(source.clone(), range()?),
        );
        ModuleDependencyGraph::new(
            IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            center,
            vec![
                ModuleDependencyNode::new(
                    center,
                    ModuleKind::PathBoundary,
                    ModuleRoot::Directory(path("src")?),
                    Some(source),
                )?,
                ModuleDependencyNode::new(
                    ModuleId::from_bytes([2; 32]),
                    ModuleKind::PathBoundary,
                    ModuleRoot::Directory(path("tools")?),
                    Some(target),
                )?,
            ],
            1,
            false,
            vec![ModuleDependencyEdge::new(
                center,
                ModuleId::from_bytes([2; 32]),
                SyntaxRelationKind::Imports,
                1,
                edge,
            )?],
            1,
            false,
            1,
            false,
            0,
        )
        .map_err(Into::into)
    }

    fn revision(path_value: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            path(path_value)?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    fn path(value: &str) -> Result<RepositoryPath, Box<dyn Error>> {
        Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
    }

    fn range() -> Result<SourceRange, Box<dyn Error>> {
        Ok(SourceRange::new(
            0,
            1,
            SourcePosition::new(0, 0),
            SourcePosition::new(0, 1),
        )?)
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([6; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([7; 32]),
            WorktreeAnchorId::from_bytes([8; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
