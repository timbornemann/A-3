use crate::{
    JobContext, KnowledgeStoreFailure, ModuleCardClaimState, ModuleCardDetail, ModuleCardLifecycle,
    ProjectMapMappingStatus,
};
use a3_domain::{
    FileRevision, GraphEdge, GraphEndpoint, GraphSymbol, IndexRunId, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleId, ModuleKind, ModuleRoot,
    Progress, ProjectIdentity, PublishedIndex, RepositoryPath, SnapshotId, SymbolId, SymbolKind,
    SymbolRole, SymbolVisibility, SyntaxProvider, SyntaxRelationKind, UnresolvedGraphTarget,
    UnresolvedReason,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Maximum module regions in the project overview.
pub const PROJECT_MAP_ATLAS_MODULE_LIMIT: usize = 64;
/// Maximum file regions in one module scene.
pub const PROJECT_MAP_ATLAS_FILE_LIMIT: usize = 32;
/// Maximum structural symbols in one file scene.
pub const PROJECT_MAP_ATLAS_SYMBOL_LIMIT: usize = 48;
/// Center plus direct members or architectural neighbors in a symbol scene.
pub const PROJECT_MAP_ATLAS_SYMBOL_NEIGHBOR_LIMIT: usize = 32;
/// Maximum route groups transported by an atlas scene.
pub const PROJECT_MAP_ATLAS_RELATION_LIMIT: usize = 128;
/// Maximum unresolved or external boundary stubs for one explicit selection.
pub const PROJECT_MAP_ATLAS_BOUNDARY_LIMIT: usize = 16;
/// Fixed inventory page size owned by the Core policy.
pub const PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE: usize = 50;
/// Maximum non-root targets in a focused flow.
pub const PROJECT_MAP_ATLAS_FLOW_TARGET_LIMIT: usize = 31;
/// Maximum graph rows inspected for one entity or flow read.
pub const PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT: usize = 4_096;

const MAX_DISPLAY_BYTES: usize = 1_024;
const MAX_DETAIL_BYTES: usize = 4 * 1_024;
const MAX_PURPOSE_BYTES: usize = 160;
const MAX_CURSOR_BYTES: usize = 256;

/// Versioned deterministic ranking, truncation, and scene-construction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapAtlasPolicyVersion {
    /// Initial progressive Project → Module → File → Symbol policy.
    V1,
}

/// Positive one-based file position in the deterministic module inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectMapFileOrdinal(u32);

impl ProjectMapFileOrdinal {
    /// Creates a bounded Core-issued file position.
    pub fn new(value: u32) -> Result<Self, ProjectMapAtlasError> {
        if value == 0 || value > 250_000 {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self(value))
    }

    /// Returns the stable one-based value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Typed current selection emitted by an earlier Atlas or inventory response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectMapEntitySelection {
    /// One current primary deterministic module.
    Module {
        /// Stable primary module identity.
        module_id: ModuleId,
    },
    /// One current file identified by its deterministic position and exact revision evidence.
    File {
        /// Stable owning primary module identity.
        module_id: ModuleId,
        /// One-based position in the deterministic module file ranking.
        ordinal: ProjectMapFileOrdinal,
        /// Exact current file-revision evidence identity.
        evidence_id: ModuleCardEvidenceId,
    },
    /// One current content-bound structural symbol.
    Symbol {
        /// Stable owning primary module identity.
        module_id: ModuleId,
        /// Content-bound structural symbol identity.
        symbol_id: SymbolId,
        /// Exact current symbol evidence identity.
        evidence_id: ModuleCardEvidenceId,
    },
}

impl ProjectMapEntitySelection {
    /// Returns the primary module that owns the selection.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        match self {
            Self::Module { module_id }
            | Self::File { module_id, .. }
            | Self::Symbol { module_id, .. } => module_id,
        }
    }

    /// Returns the exact entity evidence for file and symbol selections.
    #[must_use]
    pub const fn evidence_id(self) -> Option<ModuleCardEvidenceId> {
        match self {
            Self::Module { .. } => None,
            Self::File { evidence_id, .. } | Self::Symbol { evidence_id, .. } => Some(evidence_id),
        }
    }
}

/// Exact current static-index evidence selection previously emitted by the Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapIndexEvidenceSelection {
    /// Current file revision selected from a module inventory or scene.
    File {
        /// Stable owning primary module identity.
        module_id: ModuleId,
        /// One-based position in the deterministic module file ranking.
        ordinal: ProjectMapFileOrdinal,
        /// Exact current file-revision evidence identity.
        evidence_id: ModuleCardEvidenceId,
    },
    /// Current structural symbol selected from a file or symbol scene.
    Symbol {
        /// Stable owning primary module identity.
        module_id: ModuleId,
        /// Content-bound structural symbol identity.
        symbol_id: SymbolId,
        /// Exact current symbol evidence identity.
        evidence_id: ModuleCardEvidenceId,
    },
    /// Current resolved relation selected from an Atlas or flow scene.
    Relation {
        /// Primary module containing the evidence source.
        module_id: ModuleId,
        /// One-based canonical resolved-edge position in the current graph.
        edge_sequence: u64,
        /// Exact current graph-edge evidence identity.
        evidence_id: ModuleCardEvidenceId,
    },
    /// Current unresolved relation candidate selected from a dashed boundary route.
    UnresolvedRelation {
        /// Primary module containing the evidence source.
        module_id: ModuleId,
        /// One-based canonical candidate position in the current graph.
        candidate_sequence: u64,
        /// Exact file-revision evidence selected by the Core.
        evidence_id: ModuleCardEvidenceId,
    },
}

/// Revalidated source target for one exact current static-index Evidence selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapIndexEvidenceTarget {
    revision: FileRevision,
    range: Option<a3_domain::SourceRange>,
}

impl ProjectMapIndexEvidenceTarget {
    /// Returns the exact current file revision that the secure source reader must revalidate.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the source span for a symbol or relation, or `None` for whole-file Evidence.
    #[must_use]
    pub const fn range(&self) -> Option<a3_domain::SourceRange> {
        self.range
    }
}

impl ProjectMapIndexEvidenceSelection {
    /// Returns the owning primary module used for membership revalidation.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        match self {
            Self::File { module_id, .. }
            | Self::Symbol { module_id, .. }
            | Self::Relation { module_id, .. }
            | Self::UnresolvedRelation { module_id, .. } => module_id,
        }
    }

    /// Returns the opaque exact Evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> ModuleCardEvidenceId {
        match self {
            Self::File { evidence_id, .. }
            | Self::Symbol { evidence_id, .. }
            | Self::Relation { evidence_id, .. }
            | Self::UnresolvedRelation { evidence_id, .. } => evidence_id,
        }
    }
}

/// Scene level selected by semantic zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapAtlasLevel {
    /// Bounded primary module overview.
    Project,
    /// Bounded file regions inside one module.
    Module,
    /// Bounded architecture symbols inside one file.
    File,
    /// One symbol with direct members and architectural neighbors.
    Symbol,
}

/// Strict Atlas-scene request without paths, publication anchors, or rendering limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapAtlasSceneQuery {
    selection: Option<ProjectMapEntitySelection>,
}

impl ProjectMapAtlasSceneQuery {
    /// Creates a project overview or a progressively focused scene.
    #[must_use]
    pub const fn new(selection: Option<ProjectMapEntitySelection>) -> Self {
        Self { selection }
    }

    /// Returns the optional previously emitted current selection.
    #[must_use]
    pub const fn selection(self) -> Option<ProjectMapEntitySelection> {
        self.selection
    }
}

/// Stable opaque identifier for a node inside one bounded response.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectMapAtlasNodeId([u8; 32]);

impl ProjectMapAtlasNodeId {
    /// Reconstructs an identifier created by the trusted Atlas adapter.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical bytes for IPC encoding.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ProjectMapAtlasNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectMapAtlasNodeId(redacted)")
    }
}

/// Visual and semantic category of one Atlas region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapAtlasNodeKind {
    /// Manifest-backed module boundary.
    ManifestModule,
    /// Structural path module boundary.
    PathModule,
    /// Current source file.
    File,
    /// Language module or namespace declaration.
    Namespace,
    /// Class, struct, interface, trait, enum, implementation, or alias.
    Type,
    /// Free function, method, or callable entry point.
    Callable,
    /// Field, constant, variable, variant, static, or parameter.
    Member,
    /// External or unresolved target deliberately kept outside the repository graph.
    Boundary,
}

/// One region or landmark in a bounded progressive scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapAtlasNode {
    id: ProjectMapAtlasNodeId,
    parent_id: Option<ProjectMapAtlasNodeId>,
    selection: Option<ProjectMapEntitySelection>,
    kind: ProjectMapAtlasNodeKind,
    display_name: String,
    detail: Option<String>,
    rank: u16,
    volume: u64,
    file_count: u64,
    symbol_count: u64,
    member_count: u64,
    mapping_status: Option<ProjectMapMappingStatus>,
    purpose: Option<String>,
    current_risk_count: u64,
    evidence_id: Option<ModuleCardEvidenceId>,
    claim_badge_count: u16,
    dimmed: bool,
}

impl ProjectMapAtlasNode {
    /// Creates one bounded node and rejects contradictory presentation metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ProjectMapAtlasNodeId,
        parent_id: Option<ProjectMapAtlasNodeId>,
        selection: Option<ProjectMapEntitySelection>,
        kind: ProjectMapAtlasNodeKind,
        display_name: String,
        detail: Option<String>,
        rank: u16,
        volume: u64,
        file_count: u64,
        symbol_count: u64,
        member_count: u64,
        mapping_status: Option<ProjectMapMappingStatus>,
        purpose: Option<String>,
        current_risk_count: u64,
        evidence_id: Option<ModuleCardEvidenceId>,
        claim_badge_count: u16,
        dimmed: bool,
    ) -> Result<Self, ProjectMapAtlasError> {
        validate_text(&display_name, MAX_DISPLAY_BYTES)?;
        if let Some(value) = &detail {
            validate_text(value, MAX_DETAIL_BYTES)?;
        }
        if let Some(value) = &purpose {
            validate_text(value, MAX_PURPOSE_BYTES)?;
        }
        if rank == 0
            || volume == 0
            || parent_id == Some(id)
            || matches!(kind, ProjectMapAtlasNodeKind::Boundary) != selection.is_none()
            || selection.and_then(ProjectMapEntitySelection::evidence_id) != evidence_id
                && !matches!(
                    kind,
                    ProjectMapAtlasNodeKind::ManifestModule
                        | ProjectMapAtlasNodeKind::PathModule
                        | ProjectMapAtlasNodeKind::Boundary
                )
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            id,
            parent_id,
            selection,
            kind,
            display_name,
            detail,
            rank,
            volume,
            file_count,
            symbol_count,
            member_count,
            mapping_status,
            purpose,
            current_risk_count,
            evidence_id,
            claim_badge_count,
            dimmed,
        })
    }

    /// Returns the scene-local stable node identity.
    #[must_use]
    pub const fn id(&self) -> ProjectMapAtlasNodeId {
        self.id
    }
    /// Returns the containing region when one is visible.
    #[must_use]
    pub const fn parent_id(&self) -> Option<ProjectMapAtlasNodeId> {
        self.parent_id
    }
    /// Returns the Core-issued entity selection, absent only for a boundary stub.
    #[must_use]
    pub const fn selection(&self) -> Option<ProjectMapEntitySelection> {
        self.selection
    }
    /// Returns the semantic node category.
    #[must_use]
    pub const fn kind(&self) -> ProjectMapAtlasNodeKind {
        self.kind
    }
    /// Returns bounded display-only text.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    /// Returns optional bounded secondary text.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    /// Returns the one-based deterministic rank.
    #[must_use]
    pub const fn rank(&self) -> u16 {
        self.rank
    }
    /// Returns the uncapped structural volume used by the deterministic layout.
    #[must_use]
    pub const fn volume(&self) -> u64 {
        self.volume
    }
    /// Returns exact current files represented by this node.
    #[must_use]
    pub const fn file_count(&self) -> u64 {
        self.file_count
    }
    /// Returns exact current structural symbols represented by this node.
    #[must_use]
    pub const fn symbol_count(&self) -> u64 {
        self.symbol_count
    }
    /// Returns exact directly contained members where available.
    #[must_use]
    pub const fn member_count(&self) -> u64 {
        self.member_count
    }
    /// Returns independent Deep-Map lifecycle for module regions.
    #[must_use]
    pub const fn mapping_status(&self) -> Option<ProjectMapMappingStatus> {
        self.mapping_status
    }
    /// Returns a current verified purpose of at most 160 bytes.
    #[must_use]
    pub fn purpose(&self) -> Option<&str> {
        self.purpose.as_deref()
    }
    /// Returns current verified risk statements for a module.
    #[must_use]
    pub const fn current_risk_count(&self) -> u64 {
        self.current_risk_count
    }
    /// Returns the exact entity evidence hook when this node has one.
    #[must_use]
    pub const fn evidence_id(&self) -> Option<ModuleCardEvidenceId> {
        self.evidence_id
    }
    /// Returns exact current claims whose evidence set contains this evidence ID.
    #[must_use]
    pub const fn claim_badge_count(&self) -> u16 {
        self.claim_badge_count
    }
    /// Returns the Task-Lens presentation hint computed from current evidence.
    #[must_use]
    pub const fn dimmed(&self) -> bool {
        self.dimmed
    }
}

/// Why a visible route target is not a resolved repository-local fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapAtlasUncertainty {
    /// Target belongs outside the repository boundary.
    External,
    /// Deterministic linker found no exact match.
    NoDeterministicMatch,
    /// More than one target matched the same deterministic key.
    AmbiguousMatch,
    /// Target requires runtime semantics.
    DynamicReference,
    /// Adapter-emitted file target is absent.
    MissingFile,
}

impl From<UnresolvedReason> for ProjectMapAtlasUncertainty {
    fn from(value: UnresolvedReason) -> Self {
        match value {
            UnresolvedReason::NoDeterministicMatch => Self::NoDeterministicMatch,
            UnresolvedReason::AmbiguousMatch => Self::AmbiguousMatch,
            UnresolvedReason::DynamicReference => Self::DynamicReference,
            UnresolvedReason::MissingFile => Self::MissingFile,
        }
    }
}

/// One grouped deterministic route rendered non-authoritatively by SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapAtlasRelation {
    source_node_id: ProjectMapAtlasNodeId,
    target_node_id: ProjectMapAtlasNodeId,
    relation: SyntaxRelationKind,
    evidence_count: u64,
    confidence_basis_points: u16,
    provider: SyntaxProvider,
    evidence: Option<ProjectMapIndexEvidenceSelection>,
    claim_badge_count: u16,
    uncertainty: Option<ProjectMapAtlasUncertainty>,
}

impl ProjectMapAtlasRelation {
    /// Creates one route while rejecting self edges and evidence mismatches.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_node_id: ProjectMapAtlasNodeId,
        target_node_id: ProjectMapAtlasNodeId,
        relation: SyntaxRelationKind,
        evidence_count: u64,
        confidence_basis_points: u16,
        provider: SyntaxProvider,
        evidence: Option<ProjectMapIndexEvidenceSelection>,
        claim_badge_count: u16,
        uncertainty: Option<ProjectMapAtlasUncertainty>,
    ) -> Result<Self, ProjectMapAtlasError> {
        if source_node_id == target_node_id
            || evidence_count == 0
            || confidence_basis_points > 10_000
            || evidence.is_none()
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            source_node_id,
            target_node_id,
            relation,
            evidence_count,
            confidence_basis_points,
            provider,
            evidence,
            claim_badge_count,
            uncertainty,
        })
    }

    /// Returns the visible source node.
    #[must_use]
    pub const fn source_node_id(&self) -> ProjectMapAtlasNodeId {
        self.source_node_id
    }
    /// Returns the visible target node.
    #[must_use]
    pub const fn target_node_id(&self) -> ProjectMapAtlasNodeId {
        self.target_node_id
    }
    /// Returns the language-neutral relationship.
    #[must_use]
    pub const fn relation(&self) -> SyntaxRelationKind {
        self.relation
    }
    /// Returns exact represented edge count.
    #[must_use]
    pub const fn evidence_count(&self) -> u64 {
        self.evidence_count
    }
    /// Returns deterministic confidence in basis points.
    #[must_use]
    pub const fn confidence_basis_points(&self) -> u16 {
        self.confidence_basis_points
    }
    /// Returns the deterministic observer.
    #[must_use]
    pub const fn provider(&self) -> SyntaxProvider {
        self.provider
    }
    /// Returns an exact current relation-evidence hook when available.
    #[must_use]
    pub const fn evidence(&self) -> Option<ProjectMapIndexEvidenceSelection> {
        self.evidence
    }
    /// Returns exact current claims whose evidence set contains this route Evidence ID.
    #[must_use]
    pub const fn claim_badge_count(&self) -> u16 {
        self.claim_badge_count
    }
    /// Returns why the target is a dashed boundary relation.
    #[must_use]
    pub const fn uncertainty(&self) -> Option<ProjectMapAtlasUncertainty> {
        self.uncertainty
    }
}

/// One breadcrumb step with a Core-issued selection for semantic zoom navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapAtlasBreadcrumb {
    label: String,
    selection: Option<ProjectMapEntitySelection>,
}

impl ProjectMapAtlasBreadcrumb {
    /// Creates one bounded breadcrumb; `None` represents the project root.
    pub fn new(
        label: String,
        selection: Option<ProjectMapEntitySelection>,
    ) -> Result<Self, ProjectMapAtlasError> {
        validate_text(&label, MAX_DISPLAY_BYTES)?;
        Ok(Self { label, selection })
    }

    /// Returns display-only breadcrumb text.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Returns the level selection or `None` for the project root.
    #[must_use]
    pub const fn selection(&self) -> Option<ProjectMapEntitySelection> {
        self.selection
    }
}

/// Complete bounded Atlas scene bound to one atomic index publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapAtlasScene {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    policy_version: ProjectMapAtlasPolicyVersion,
    level: ProjectMapAtlasLevel,
    selection: Option<ProjectMapEntitySelection>,
    breadcrumb: Vec<ProjectMapAtlasBreadcrumb>,
    nodes: Vec<ProjectMapAtlasNode>,
    node_count: u64,
    relations: Vec<ProjectMapAtlasRelation>,
    relation_count: u64,
    boundary_count: u64,
    unresolved_count: u64,
    inspected_edge_count: u64,
    nodes_truncated: bool,
    relations_truncated: bool,
    boundaries_truncated: bool,
    source_edges_truncated: bool,
}

impl ProjectMapAtlasScene {
    /// Creates a scene and validates level-specific bounds, ranks, endpoints, and counts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        level: ProjectMapAtlasLevel,
        selection: Option<ProjectMapEntitySelection>,
        breadcrumb: Vec<ProjectMapAtlasBreadcrumb>,
        nodes: Vec<ProjectMapAtlasNode>,
        node_count: u64,
        relations: Vec<ProjectMapAtlasRelation>,
        relation_count: u64,
        boundary_count: u64,
        unresolved_count: u64,
        inspected_edge_count: u64,
        nodes_truncated: bool,
        relations_truncated: bool,
        boundaries_truncated: bool,
        source_edges_truncated: bool,
    ) -> Result<Self, ProjectMapAtlasError> {
        let node_limit = match level {
            ProjectMapAtlasLevel::Project => PROJECT_MAP_ATLAS_MODULE_LIMIT,
            ProjectMapAtlasLevel::Module => PROJECT_MAP_ATLAS_FILE_LIMIT,
            ProjectMapAtlasLevel::File => PROJECT_MAP_ATLAS_SYMBOL_LIMIT,
            ProjectMapAtlasLevel::Symbol => PROJECT_MAP_ATLAS_SYMBOL_NEIGHBOR_LIMIT,
        } + PROJECT_MAP_ATLAS_BOUNDARY_LIMIT;
        let ids = nodes
            .iter()
            .map(ProjectMapAtlasNode::id)
            .collect::<BTreeSet<_>>();
        let ranks = nodes
            .iter()
            .map(ProjectMapAtlasNode::rank)
            .collect::<BTreeSet<_>>();
        let relation_keys = relations
            .iter()
            .map(|edge| {
                (
                    edge.source_node_id(),
                    edge.target_node_id(),
                    edge.relation(),
                )
            })
            .collect::<BTreeSet<_>>();
        if nodes.len() > node_limit
            || nodes.len() != ids.len()
            || nodes.len() != ranks.len()
            || nodes
                .iter()
                .enumerate()
                .any(|(index, node)| usize::from(node.rank()) != index + 1)
            || relation_count < relations.len() as u64
            || relations.len() > PROJECT_MAP_ATLAS_RELATION_LIMIT
            || relation_keys.len() != relations.len()
            || relations.iter().any(|edge| {
                !ids.contains(&edge.source_node_id()) || !ids.contains(&edge.target_node_id())
            })
            || boundary_count
                < nodes
                    .iter()
                    .filter(|node| node.kind() == ProjectMapAtlasNodeKind::Boundary)
                    .count() as u64
            || unresolved_count > boundary_count
            || inspected_edge_count > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT as u64
            || nodes_truncated
                != (node_count
                    > nodes
                        .iter()
                        .filter(|node| node.kind() != ProjectMapAtlasNodeKind::Boundary)
                        .count() as u64)
            || relations_truncated != (relation_count > relations.len() as u64)
            || boundaries_truncated
                != (boundary_count
                    > nodes
                        .iter()
                        .filter(|node| node.kind() == ProjectMapAtlasNodeKind::Boundary)
                        .count() as u64)
            || (level == ProjectMapAtlasLevel::Project) != selection.is_none()
            || breadcrumb.is_empty()
            || breadcrumb.len() > 4
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            policy_version: ProjectMapAtlasPolicyVersion::V1,
            level,
            selection,
            breadcrumb,
            nodes,
            node_count,
            relations,
            relation_count,
            boundary_count,
            unresolved_count,
            inspected_edge_count,
            nodes_truncated,
            relations_truncated,
            boundaries_truncated,
            source_edges_truncated,
        })
    }

    /// Returns the current publication run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns the immutable current snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the deterministic V1 policy.
    #[must_use]
    pub const fn policy_version(&self) -> ProjectMapAtlasPolicyVersion {
        self.policy_version
    }
    /// Returns the semantic zoom level.
    #[must_use]
    pub const fn level(&self) -> ProjectMapAtlasLevel {
        self.level
    }
    /// Returns the current level selection.
    #[must_use]
    pub const fn selection(&self) -> Option<ProjectMapEntitySelection> {
        self.selection
    }
    /// Returns project-to-current navigation steps.
    #[must_use]
    pub fn breadcrumb(&self) -> &[ProjectMapAtlasBreadcrumb] {
        &self.breadcrumb
    }
    /// Returns bounded scene nodes.
    #[must_use]
    pub fn nodes(&self) -> &[ProjectMapAtlasNode] {
        &self.nodes
    }
    /// Returns all eligible nodes before the fixed scene cap.
    #[must_use]
    pub const fn node_count(&self) -> u64 {
        self.node_count
    }
    /// Returns bounded visible relation groups.
    #[must_use]
    pub fn relations(&self) -> &[ProjectMapAtlasRelation] {
        &self.relations
    }
    /// Returns all eligible relation groups before the fixed cap.
    #[must_use]
    pub const fn relation_count(&self) -> u64 {
        self.relation_count
    }
    /// Returns all eligible external or unresolved targets.
    #[must_use]
    pub const fn boundary_count(&self) -> u64 {
        self.boundary_count
    }
    /// Returns unresolved targets among the boundary count.
    #[must_use]
    pub const fn unresolved_count(&self) -> u64 {
        self.unresolved_count
    }
    /// Returns graph rows inspected for completeness accounting.
    #[must_use]
    pub const fn inspected_edge_count(&self) -> u64 {
        self.inspected_edge_count
    }
    /// Returns whether lower-ranked nodes were omitted.
    #[must_use]
    pub const fn nodes_truncated(&self) -> bool {
        self.nodes_truncated
    }
    /// Returns whether lower-ranked relation groups were omitted.
    #[must_use]
    pub const fn relations_truncated(&self) -> bool {
        self.relations_truncated
    }
    /// Returns whether lower-ranked boundary targets were omitted.
    #[must_use]
    pub const fn boundaries_truncated(&self) -> bool {
        self.boundaries_truncated
    }
    /// Returns whether the fixed 4,096-row inspection cap was reached.
    #[must_use]
    pub const fn source_edges_truncated(&self) -> bool {
        self.source_edges_truncated
    }
}

/// Allowed fixed inventory projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapInventoryView {
    /// Current files uniquely owned by a module.
    Files,
    /// Current structural symbols in one file or module.
    Symbols,
    /// Direct members of one type or symbol.
    Members,
}

/// Opaque publication- and scope-bound inventory cursor.
#[derive(Clone, PartialEq, Eq)]
pub struct ProjectMapInventoryCursor(String);

impl ProjectMapInventoryCursor {
    /// Accepts a bounded opaque token emitted and authenticated by the adapter.
    pub fn try_from_string(value: String) -> Result<Self, ProjectMapAtlasError> {
        if value.is_empty()
            || value.len() > MAX_CURSOR_BYTES
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self(value))
    }

    /// Returns the opaque wire representation without interpreting it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProjectMapInventoryCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectMapInventoryCursor(redacted)")
    }
}

/// Request for exactly one fixed 50-entry inventory page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapInventoryPageQuery {
    selection: ProjectMapEntitySelection,
    view: ProjectMapInventoryView,
    cursor: Option<ProjectMapInventoryCursor>,
}

impl ProjectMapInventoryPageQuery {
    /// Creates a page request without a caller-controlled page size.
    #[must_use]
    pub const fn new(
        selection: ProjectMapEntitySelection,
        view: ProjectMapInventoryView,
        cursor: Option<ProjectMapInventoryCursor>,
    ) -> Self {
        Self {
            selection,
            view,
            cursor,
        }
    }
    /// Returns the current scope selection.
    #[must_use]
    pub const fn selection(&self) -> ProjectMapEntitySelection {
        self.selection
    }
    /// Returns the closed inventory kind.
    #[must_use]
    pub const fn view(&self) -> ProjectMapInventoryView {
        self.view
    }
    /// Returns the opaque cursor emitted by an earlier matching page.
    #[must_use]
    pub fn cursor(&self) -> Option<&ProjectMapInventoryCursor> {
        self.cursor.as_ref()
    }
}

/// One fixed inventory page; only this page needs to remain in frontend state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapInventoryPage {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    selection: ProjectMapEntitySelection,
    view: ProjectMapInventoryView,
    page_number: u32,
    total_count: u64,
    items: Vec<ProjectMapAtlasNode>,
    previous_cursor: Option<ProjectMapInventoryCursor>,
    next_cursor: Option<ProjectMapInventoryCursor>,
}

impl ProjectMapInventoryPage {
    /// Creates one validated fixed page.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        selection: ProjectMapEntitySelection,
        view: ProjectMapInventoryView,
        page_number: u32,
        total_count: u64,
        items: Vec<ProjectMapAtlasNode>,
        previous_cursor: Option<ProjectMapInventoryCursor>,
        next_cursor: Option<ProjectMapInventoryCursor>,
    ) -> Result<Self, ProjectMapAtlasError> {
        if page_number == 0
            || items.len() > PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE
            || total_count < items.len() as u64
            || items
                .iter()
                .enumerate()
                .any(|(index, item)| usize::from(item.rank()) != index + 1)
            || (page_number == 1) != previous_cursor.is_none()
            || next_cursor.is_some()
                != (u64::from(page_number) * (PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE as u64)
                    < total_count)
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            selection,
            view,
            page_number,
            total_count,
            items,
            previous_cursor,
            next_cursor,
        })
    }

    /// Returns the current publication run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns the current publication snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the scope selection.
    #[must_use]
    pub const fn selection(&self) -> ProjectMapEntitySelection {
        self.selection
    }
    /// Returns the closed inventory projection.
    #[must_use]
    pub const fn view(&self) -> ProjectMapInventoryView {
        self.view
    }
    /// Returns the one-based current page.
    #[must_use]
    pub const fn page_number(&self) -> u32 {
        self.page_number
    }
    /// Returns the full inventory count.
    #[must_use]
    pub const fn total_count(&self) -> u64 {
        self.total_count
    }
    /// Returns at most fifty current items.
    #[must_use]
    pub fn items(&self) -> &[ProjectMapAtlasNode] {
        &self.items
    }
    /// Returns a cursor for the previous page.
    #[must_use]
    pub fn previous_cursor(&self) -> Option<&ProjectMapInventoryCursor> {
        self.previous_cursor.as_ref()
    }
    /// Returns a cursor for the next page.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&ProjectMapInventoryCursor> {
        self.next_cursor.as_ref()
    }
}

/// Exact current claim reference attached only by evidence-ID equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapClaimReference {
    card_id: ModuleCardId,
    claim_id: ModuleCardClaimId,
    confidence_basis_points: u16,
}

/// Current verified Module Card information allowed to enrich one bounded Atlas read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapAtlasModuleInsight {
    module_id: ModuleId,
    mapping_status: ProjectMapMappingStatus,
    purpose: Option<String>,
    current_risk_count: u64,
    claims_by_evidence: BTreeMap<ModuleCardEvidenceId, Vec<ProjectMapClaimReference>>,
}

impl ProjectMapAtlasModuleInsight {
    /// Creates a summary-only insight for an overview row.
    pub fn summary(
        module_id: ModuleId,
        mapping_status: ProjectMapMappingStatus,
        purpose: Option<String>,
        current_risk_count: u64,
    ) -> Result<Self, ProjectMapAtlasError> {
        let purpose = purpose.and_then(|value| safe_summary_text(&value, MAX_PURPOSE_BYTES));
        if mapping_status != ProjectMapMappingStatus::Current && current_risk_count != 0 {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            module_id,
            mapping_status,
            purpose,
            current_risk_count,
            claims_by_evidence: BTreeMap::new(),
        })
    }

    /// Derives purpose, current risks, and exact claim-Evidence matches from one validated Card.
    pub fn from_detail(detail: &ModuleCardDetail) -> Result<Self, ProjectMapAtlasError> {
        let mapping_status = match detail.lifecycle() {
            ModuleCardLifecycle::Current => ProjectMapMappingStatus::Current,
            ModuleCardLifecycle::Stale { .. } => ProjectMapMappingStatus::Stale,
            ModuleCardLifecycle::NeedsReview { .. } => ProjectMapMappingStatus::NeedsReview,
        };
        let content_may_be_described = mapping_status != ProjectMapMappingStatus::Stale;
        let purpose = content_may_be_described
            .then(|| {
                detail
                    .fields()
                    .iter()
                    .find(|field| field.field() == ModuleCardField::Purpose)
                    .and_then(|field| field.values().first())
                    .and_then(|value| safe_summary_text(value.value(), MAX_PURPOSE_BYTES))
            })
            .flatten();
        let current_risk_count = if mapping_status == ProjectMapMappingStatus::Current {
            detail
                .fields()
                .iter()
                .find(|field| field.field() == ModuleCardField::Risks)
                .map_or(0, |field| field.values().len() as u64)
        } else {
            0
        };
        let mut claims_by_evidence =
            BTreeMap::<ModuleCardEvidenceId, Vec<ProjectMapClaimReference>>::new();
        if mapping_status == ProjectMapMappingStatus::Current {
            for value in detail.fields().iter().flat_map(|field| field.values()) {
                let claim = value.claim();
                if claim.state() != ModuleCardClaimState::Current {
                    return Err(ProjectMapAtlasError);
                }
                let reference = ProjectMapClaimReference::new(
                    detail.id(),
                    claim.id(),
                    claim.confidence().basis_points(),
                )?;
                for evidence_id in claim.evidence_ids() {
                    claims_by_evidence
                        .entry(*evidence_id)
                        .or_default()
                        .push(reference);
                }
            }
        }
        Ok(Self {
            module_id: detail.module_id(),
            mapping_status,
            purpose,
            current_risk_count,
            claims_by_evidence,
        })
    }

    /// Returns the enriched primary module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the current mapping lifecycle.
    #[must_use]
    pub const fn mapping_status(&self) -> ProjectMapMappingStatus {
        self.mapping_status
    }

    /// Returns a verified bounded purpose, never stale Card text.
    #[must_use]
    pub fn purpose(&self) -> Option<&str> {
        self.purpose.as_deref()
    }

    /// Returns current verified risk values.
    #[must_use]
    pub const fn current_risk_count(&self) -> u64 {
        self.current_risk_count
    }

    fn claims_for(&self, evidence_id: ModuleCardEvidenceId) -> &[ProjectMapClaimReference] {
        self.claims_by_evidence
            .get(&evidence_id)
            .map_or(&[], Vec::as_slice)
    }
}

impl ProjectMapClaimReference {
    /// Creates a current verified claim reference.
    pub fn new(
        card_id: ModuleCardId,
        claim_id: ModuleCardClaimId,
        confidence_basis_points: u16,
    ) -> Result<Self, ProjectMapAtlasError> {
        if confidence_basis_points > 10_000 {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            card_id,
            claim_id,
            confidence_basis_points,
        })
    }
    /// Returns the verified Card identity.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }
    /// Returns the exact current claim identity.
    #[must_use]
    pub const fn claim_id(self) -> ModuleCardClaimId {
        self.claim_id
    }
    /// Returns independent claim confidence.
    #[must_use]
    pub const fn confidence_basis_points(self) -> u16 {
        self.confidence_basis_points
    }
}

/// One relation count in an entity Inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapRelationCount {
    relation: SyntaxRelationKind,
    incoming: u64,
    outgoing: u64,
}

impl ProjectMapRelationCount {
    /// Creates one non-empty relation aggregate.
    pub fn new(
        relation: SyntaxRelationKind,
        incoming: u64,
        outgoing: u64,
    ) -> Result<Self, ProjectMapAtlasError> {
        if incoming == 0 && outgoing == 0 {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            relation,
            incoming,
            outgoing,
        })
    }
    /// Returns the relationship category.
    #[must_use]
    pub const fn relation(self) -> SyntaxRelationKind {
        self.relation
    }
    /// Returns incoming edges.
    #[must_use]
    pub const fn incoming(self) -> u64 {
        self.incoming
    }
    /// Returns outgoing edges.
    #[must_use]
    pub const fn outgoing(self) -> u64 {
        self.outgoing
    }
}

/// Progressive Inspector payload for one current entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapEntityContext {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    entity: ProjectMapAtlasNode,
    relation_counts: Vec<ProjectMapRelationCount>,
    related_nodes: Vec<ProjectMapAtlasNode>,
    architecture_relations: Vec<ProjectMapAtlasRelation>,
    architecture_relation_count: u64,
    boundary_nodes: Vec<ProjectMapAtlasNode>,
    boundary_relations: Vec<ProjectMapAtlasRelation>,
    boundary_count: u64,
    document_relation_count: u64,
    claims: Vec<ProjectMapClaimReference>,
    source_edges_truncated: bool,
}

impl ProjectMapEntityContext {
    /// Creates one bounded Inspector context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        entity: ProjectMapAtlasNode,
        relation_counts: Vec<ProjectMapRelationCount>,
        related_nodes: Vec<ProjectMapAtlasNode>,
        architecture_relations: Vec<ProjectMapAtlasRelation>,
        architecture_relation_count: u64,
        boundary_nodes: Vec<ProjectMapAtlasNode>,
        boundary_relations: Vec<ProjectMapAtlasRelation>,
        boundary_count: u64,
        document_relation_count: u64,
        claims: Vec<ProjectMapClaimReference>,
        source_edges_truncated: bool,
    ) -> Result<Self, ProjectMapAtlasError> {
        if related_nodes.len() > 32
            || architecture_relations.len() > 32
            || related_nodes.len() != architecture_relations.len()
            || architecture_relation_count < architecture_relations.len() as u64
            || boundary_nodes.len() > PROJECT_MAP_ATLAS_BOUNDARY_LIMIT
            || boundary_relations.len() != boundary_nodes.len()
            || boundary_count < boundary_nodes.len() as u64
            || claims.len() > 64
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            entity,
            relation_counts,
            related_nodes,
            architecture_relations,
            architecture_relation_count,
            boundary_nodes,
            boundary_relations,
            boundary_count,
            document_relation_count,
            claims,
            source_edges_truncated,
        })
    }

    /// Returns current publication run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns current publication snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns selected entity metadata.
    #[must_use]
    pub const fn entity(&self) -> &ProjectMapAtlasNode {
        &self.entity
    }
    /// Returns all non-zero relation aggregates.
    #[must_use]
    pub fn relation_counts(&self) -> &[ProjectMapRelationCount] {
        &self.relation_counts
    }
    /// Returns one counterpart node for every visible direct architecture route.
    #[must_use]
    pub fn related_nodes(&self) -> &[ProjectMapAtlasNode] {
        &self.related_nodes
    }
    /// Returns at most 32 direct architecture routes.
    #[must_use]
    pub fn architecture_relations(&self) -> &[ProjectMapAtlasRelation] {
        &self.architecture_relations
    }
    /// Returns all eligible direct architecture routes.
    #[must_use]
    pub const fn architecture_relation_count(&self) -> u64 {
        self.architecture_relation_count
    }
    /// Returns at most sixteen boundary stubs.
    #[must_use]
    pub fn boundary_nodes(&self) -> &[ProjectMapAtlasNode] {
        &self.boundary_nodes
    }
    /// Returns matching dashed boundary routes.
    #[must_use]
    pub fn boundary_relations(&self) -> &[ProjectMapAtlasRelation] {
        &self.boundary_relations
    }
    /// Returns all eligible boundary candidates.
    #[must_use]
    pub const fn boundary_count(&self) -> u64 {
        self.boundary_count
    }
    /// Returns direct `Documents` relations kept outside the map routes.
    #[must_use]
    pub const fn document_relation_count(&self) -> u64 {
        self.document_relation_count
    }
    /// Returns exact current claim references.
    #[must_use]
    pub fn claims(&self) -> &[ProjectMapClaimReference] {
        &self.claims
    }
    /// Returns whether the inspection cap omitted graph rows.
    #[must_use]
    pub const fn source_edges_truncated(&self) -> bool {
        self.source_edges_truncated
    }
}

/// Closed focused flow preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapFlowPreset {
    /// Incoming call graph, at most two hops.
    Callers,
    /// Outgoing call graph, at most two hops.
    Callees,
    /// Direct test-to-subject and subject-to-test relationships.
    Tests,
    /// Direct read and write relationships.
    DataAccess,
}

/// Flow-scene request without caller-controlled relation, direction, hop, or result limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapFlowSceneQuery {
    selection: ProjectMapEntitySelection,
    preset: ProjectMapFlowPreset,
}

impl ProjectMapFlowSceneQuery {
    /// Creates one fixed-preset flow request.
    #[must_use]
    pub const fn new(selection: ProjectMapEntitySelection, preset: ProjectMapFlowPreset) -> Self {
        Self { selection, preset }
    }
    /// Returns the root entity selection.
    #[must_use]
    pub const fn selection(self) -> ProjectMapEntitySelection {
        self.selection
    }
    /// Returns the fixed flow preset.
    #[must_use]
    pub const fn preset(self) -> ProjectMapFlowPreset {
        self.preset
    }
}

/// One edge on a complete shortest evidence path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapFlowStep {
    source_node_id: ProjectMapAtlasNodeId,
    target_node_id: ProjectMapAtlasNodeId,
    relation: SyntaxRelationKind,
    evidence: ProjectMapIndexEvidenceSelection,
}

impl ProjectMapFlowStep {
    /// Creates one non-self path step.
    pub fn new(
        source_node_id: ProjectMapAtlasNodeId,
        target_node_id: ProjectMapAtlasNodeId,
        relation: SyntaxRelationKind,
        evidence: ProjectMapIndexEvidenceSelection,
    ) -> Result<Self, ProjectMapAtlasError> {
        if source_node_id == target_node_id {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            source_node_id,
            target_node_id,
            relation,
            evidence,
        })
    }
    /// Returns source node identity.
    #[must_use]
    pub const fn source_node_id(&self) -> ProjectMapAtlasNodeId {
        self.source_node_id
    }
    /// Returns target node identity.
    #[must_use]
    pub const fn target_node_id(&self) -> ProjectMapAtlasNodeId {
        self.target_node_id
    }
    /// Returns the observed relationship.
    #[must_use]
    pub const fn relation(&self) -> SyntaxRelationKind {
        self.relation
    }
    /// Returns exact current relation evidence.
    #[must_use]
    pub const fn evidence(&self) -> ProjectMapIndexEvidenceSelection {
        self.evidence
    }
}

/// One target plus its full shortest current evidence path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapFlowTarget {
    node_id: ProjectMapAtlasNodeId,
    depth: u8,
    path: Vec<ProjectMapFlowStep>,
}

impl ProjectMapFlowTarget {
    /// Creates one bounded acyclic shortest path.
    pub fn new(
        node_id: ProjectMapAtlasNodeId,
        depth: u8,
        path: Vec<ProjectMapFlowStep>,
    ) -> Result<Self, ProjectMapAtlasError> {
        if depth == 0
            || depth > 2
            || path.len() != usize::from(depth)
            || path
                .last()
                .is_none_or(|step| step.target_node_id() != node_id)
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            node_id,
            depth,
            path,
        })
    }
    /// Returns target node identity.
    #[must_use]
    pub const fn node_id(&self) -> ProjectMapAtlasNodeId {
        self.node_id
    }
    /// Returns one- or two-hop depth.
    #[must_use]
    pub const fn depth(&self) -> u8 {
        self.depth
    }
    /// Returns every evidence step on the shortest path.
    #[must_use]
    pub fn path(&self) -> &[ProjectMapFlowStep] {
        &self.path
    }
}

/// Complete bounded focused flow scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapFlowScene {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    preset: ProjectMapFlowPreset,
    root: ProjectMapAtlasNode,
    nodes: Vec<ProjectMapAtlasNode>,
    targets: Vec<ProjectMapFlowTarget>,
    target_count: u64,
    inspected_edge_count: u64,
    targets_truncated: bool,
    source_edges_truncated: bool,
}

impl ProjectMapFlowScene {
    /// Creates a fixed-preset flow and validates all target/path bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        preset: ProjectMapFlowPreset,
        root: ProjectMapAtlasNode,
        nodes: Vec<ProjectMapAtlasNode>,
        targets: Vec<ProjectMapFlowTarget>,
        target_count: u64,
        inspected_edge_count: u64,
        targets_truncated: bool,
        source_edges_truncated: bool,
    ) -> Result<Self, ProjectMapAtlasError> {
        let ids = nodes
            .iter()
            .map(ProjectMapAtlasNode::id)
            .collect::<BTreeSet<_>>();
        if nodes.len() > PROJECT_MAP_ATLAS_FLOW_TARGET_LIMIT
            || nodes.len() != ids.len()
            || targets.len() != nodes.len()
            || targets.len() > PROJECT_MAP_ATLAS_FLOW_TARGET_LIMIT
            || target_count < targets.len() as u64
            || targets_truncated != (target_count > targets.len() as u64)
            || inspected_edge_count > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT as u64
            || targets
                .iter()
                .any(|target| !ids.contains(&target.node_id()))
        {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            preset,
            root,
            nodes,
            targets,
            target_count,
            inspected_edge_count,
            targets_truncated,
            source_edges_truncated,
        })
    }
    /// Returns current publication run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns current snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the closed flow preset.
    #[must_use]
    pub const fn preset(&self) -> ProjectMapFlowPreset {
        self.preset
    }
    /// Returns the selected root.
    #[must_use]
    pub const fn root(&self) -> &ProjectMapAtlasNode {
        &self.root
    }
    /// Returns at most 31 unique targets.
    #[must_use]
    pub fn nodes(&self) -> &[ProjectMapAtlasNode] {
        &self.nodes
    }
    /// Returns one complete shortest path per visible target.
    #[must_use]
    pub fn targets(&self) -> &[ProjectMapFlowTarget] {
        &self.targets
    }
    /// Returns all eligible targets before capping.
    #[must_use]
    pub const fn target_count(&self) -> u64 {
        self.target_count
    }
    /// Returns inspected graph rows.
    #[must_use]
    pub const fn inspected_edge_count(&self) -> u64 {
        self.inspected_edge_count
    }
    /// Returns whether targets were omitted.
    #[must_use]
    pub const fn targets_truncated(&self) -> bool {
        self.targets_truncated
    }
    /// Returns whether graph rows beyond 4,096 were omitted.
    #[must_use]
    pub const fn source_edges_truncated(&self) -> bool {
        self.source_edges_truncated
    }
}

/// Common availability result for current progressive Atlas reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectMapAtlasLoadResult<T> {
    /// No atomic index publication exists.
    NoPublishedIndex,
    /// The latest publication predates deterministic modules.
    ProjectionUnavailable,
    /// The Core-issued selection no longer belongs to the latest publication.
    SelectionChanged,
    /// One current bounded result is available.
    Available(T),
}

/// Cooperative cancellation and bounded progress for Atlas reads.
pub trait ProjectMapAtlasControl: fmt::Debug + Send + Sync {
    /// Returns whether the owner cancelled the read.
    fn is_cancelled(&self) -> bool;
    /// Reports only fixed start and terminal progress.
    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapAtlasControlError>;
}

impl ProjectMapAtlasControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapAtlasControlError> {
        JobContext::report_progress(self, progress).map_err(|_| ProjectMapAtlasControlError)
    }
}

/// Progress delivery failed because the owner disappeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapAtlasControlError;

impl fmt::Display for ProjectMapAtlasControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project map atlas progress is unavailable")
    }
}
impl Error for ProjectMapAtlasControlError {}

/// Stable content-free failure classes for every progressive Atlas read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapAtlasFailure {
    /// Local storage failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the strict read model.
    InvalidStoredProjection,
    /// The owning UI generation cancelled the read.
    Cancelled,
    /// The fixed two-second read deadline elapsed.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ProjectMapAtlasFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project map atlas storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored project map atlas projection is invalid")
            }
            Self::Cancelled => formatter.write_str("project map atlas read was cancelled"),
            Self::TimedOut => formatter.write_str("project map atlas read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("project map atlas progress is unavailable")
            }
        }
    }
}
impl Error for ProjectMapAtlasFailure {}

/// Owned future returned by one narrow Atlas store read.
pub type ProjectMapAtlasFuture<'a, T> = Pin<
    Box<
        dyn Future<Output = Result<ProjectMapAtlasLoadResult<T>, ProjectMapAtlasFailure>>
            + Send
            + 'a,
    >,
>;

/// Read-only port for all four policy-owned progressive Atlas projections.
pub trait ProjectMapAtlasStore: fmt::Debug + Send + Sync {
    /// Loads one semantic-zoom scene.
    fn load_atlas_scene<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ProjectMapAtlasSceneQuery,
        control: &'a dyn ProjectMapAtlasControl,
    ) -> ProjectMapAtlasFuture<'a, ProjectMapAtlasScene>;
    /// Loads progressive Inspector metadata and direct relationships.
    fn load_entity_context<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        selection: ProjectMapEntitySelection,
        control: &'a dyn ProjectMapAtlasControl,
    ) -> ProjectMapAtlasFuture<'a, ProjectMapEntityContext>;
    /// Loads one fixed fifty-entry inventory page.
    fn load_inventory_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ProjectMapInventoryPageQuery,
        control: &'a dyn ProjectMapAtlasControl,
    ) -> ProjectMapAtlasFuture<'a, ProjectMapInventoryPage>;
    /// Loads one fixed-preset focused flow.
    fn load_flow_scene<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ProjectMapFlowSceneQuery,
        control: &'a dyn ProjectMapAtlasControl,
    ) -> ProjectMapAtlasFuture<'a, ProjectMapFlowScene>;
    /// Revalidates one Core-issued current static-index Evidence selection for source preview.
    fn load_index_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        selection: ProjectMapIndexEvidenceSelection,
        control: &'a dyn ProjectMapAtlasControl,
    ) -> ProjectMapAtlasFuture<'a, ProjectMapIndexEvidenceTarget>;
}

/// Application use case retaining cancellation and fixed limits outside persistence.
#[derive(Debug)]
pub struct ExploreProjectMapAtlas {
    store: Arc<dyn ProjectMapAtlasStore>,
}

impl ExploreProjectMapAtlas {
    /// Wires the narrow progressive Atlas capability.
    #[must_use]
    pub fn new(store: Arc<dyn ProjectMapAtlasStore>) -> Self {
        Self { store }
    }

    /// Reads one current scene.
    pub async fn scene(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapAtlasSceneQuery,
        control: &dyn ProjectMapAtlasControl,
    ) -> Result<ProjectMapAtlasLoadResult<ProjectMapAtlasScene>, ProjectMapAtlasFailure> {
        self.execute(
            control,
            self.store.load_atlas_scene(project, query, control),
        )
        .await
    }
    /// Reads one current Inspector context.
    pub async fn context(
        &self,
        project: &ProjectIdentity,
        selection: ProjectMapEntitySelection,
        control: &dyn ProjectMapAtlasControl,
    ) -> Result<ProjectMapAtlasLoadResult<ProjectMapEntityContext>, ProjectMapAtlasFailure> {
        self.execute(
            control,
            self.store.load_entity_context(project, selection, control),
        )
        .await
    }
    /// Reads one current fixed inventory page.
    pub async fn inventory(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapInventoryPageQuery,
        control: &dyn ProjectMapAtlasControl,
    ) -> Result<ProjectMapAtlasLoadResult<ProjectMapInventoryPage>, ProjectMapAtlasFailure> {
        self.execute(
            control,
            self.store.load_inventory_page(project, query, control),
        )
        .await
    }
    /// Reads one current focused flow.
    pub async fn flow(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapFlowSceneQuery,
        control: &dyn ProjectMapAtlasControl,
    ) -> Result<ProjectMapAtlasLoadResult<ProjectMapFlowScene>, ProjectMapAtlasFailure> {
        self.execute(control, self.store.load_flow_scene(project, query, control))
            .await
    }

    async fn execute<T>(
        &self,
        control: &dyn ProjectMapAtlasControl,
        future: ProjectMapAtlasFuture<'_, T>,
    ) -> Result<ProjectMapAtlasLoadResult<T>, ProjectMapAtlasFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ProjectMapAtlasFailure::Cancelled);
        }
        let result = future.await?;
        if control.is_cancelled() {
            return Err(ProjectMapAtlasFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

/// One Atlas object violated a fixed size, identity, or consistency invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapAtlasError;

impl fmt::Display for ProjectMapAtlasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project map atlas value is invalid")
    }
}
impl Error for ProjectMapAtlasError {}

fn report(
    control: &dyn ProjectMapAtlasControl,
    completed: u64,
) -> Result<(), ProjectMapAtlasFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ProjectMapAtlasFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ProjectMapAtlasFailure::ProgressUnavailable)
}

fn validate_text(value: &str, maximum: usize) -> Result<(), ProjectMapAtlasError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ProjectMapAtlasError);
    }
    Ok(())
}

/// Builds one deterministic progressive scene from an already atomically reconstructed index.
///
/// `Ok(None)` means the Core-issued selection is no longer current or uniquely owned.
pub fn build_project_map_atlas_scene(
    published: &PublishedIndex,
    query: &ProjectMapAtlasSceneQuery,
) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
    build_project_map_atlas_scene_with_insights(published, query, &[])
}

/// Builds one deterministic scene and enriches only exact current verified Module Card matches.
pub fn build_project_map_atlas_scene_with_insights(
    published: &PublishedIndex,
    query: &ProjectMapAtlasSceneQuery,
    insights: &[ProjectMapAtlasModuleInsight],
) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
    AtlasIndex::new(published, insights)?.scene(query.selection())
}

/// Builds one deterministic progressive Inspector context from the current publication.
///
/// `Ok(None)` means the selection changed at the publication boundary.
pub fn build_project_map_entity_context(
    published: &PublishedIndex,
    selection: ProjectMapEntitySelection,
) -> Result<Option<ProjectMapEntityContext>, ProjectMapAtlasError> {
    build_project_map_entity_context_with_insights(published, selection, &[])
}

/// Builds one Inspector context with exact current claim-Evidence references.
pub fn build_project_map_entity_context_with_insights(
    published: &PublishedIndex,
    selection: ProjectMapEntitySelection,
    insights: &[ProjectMapAtlasModuleInsight],
) -> Result<Option<ProjectMapEntityContext>, ProjectMapAtlasError> {
    AtlasIndex::new(published, insights)?.context(selection)
}

/// Builds one fixed fifty-entry inventory page from the current publication.
///
/// `Ok(None)` means the selection or opaque cursor is stale or belongs to another scope.
pub fn build_project_map_inventory_page(
    published: &PublishedIndex,
    query: &ProjectMapInventoryPageQuery,
) -> Result<Option<ProjectMapInventoryPage>, ProjectMapAtlasError> {
    build_project_map_inventory_page_with_insights(published, query, &[])
}

/// Builds one inventory page with exact current entity claim badges.
pub fn build_project_map_inventory_page_with_insights(
    published: &PublishedIndex,
    query: &ProjectMapInventoryPageQuery,
    insights: &[ProjectMapAtlasModuleInsight],
) -> Result<Option<ProjectMapInventoryPage>, ProjectMapAtlasError> {
    AtlasIndex::new(published, insights)?.inventory(query)
}

/// Builds one fixed-preset focused flow from the current publication.
///
/// `Ok(None)` means the root selection is no longer current or cannot represent a flow endpoint.
pub fn build_project_map_flow_scene(
    published: &PublishedIndex,
    query: &ProjectMapFlowSceneQuery,
) -> Result<Option<ProjectMapFlowScene>, ProjectMapAtlasError> {
    build_project_map_flow_scene_with_insights(published, query, &[])
}

/// Builds one focused flow with exact current node and route claim badges.
pub fn build_project_map_flow_scene_with_insights(
    published: &PublishedIndex,
    query: &ProjectMapFlowSceneQuery,
    insights: &[ProjectMapAtlasModuleInsight],
) -> Result<Option<ProjectMapFlowScene>, ProjectMapAtlasError> {
    AtlasIndex::new(published, insights)?.flow(query)
}

/// Revalidates one current file, symbol, resolved edge, or unresolved candidate selection.
///
/// `Ok(None)` means that publication replacement or tampering invalidated the opaque selection.
pub fn resolve_project_map_index_evidence(
    published: &PublishedIndex,
    selection: ProjectMapIndexEvidenceSelection,
) -> Result<Option<ProjectMapIndexEvidenceTarget>, ProjectMapAtlasError> {
    AtlasIndex::new(published, &[])?.index_evidence(selection)
}

struct AtlasIndex<'a> {
    published: &'a PublishedIndex,
    symbols: BTreeMap<SymbolId, &'a GraphSymbol>,
    primary_modules: BTreeMap<SymbolId, ModuleId>,
    file_modules: BTreeMap<RepositoryPath, Option<ModuleId>>,
    ranks: BTreeMap<SymbolId, usize>,
    insights: BTreeMap<ModuleId, &'a ProjectMapAtlasModuleInsight>,
}

impl<'a> AtlasIndex<'a> {
    fn new(
        published: &'a PublishedIndex,
        insights: &'a [ProjectMapAtlasModuleInsight],
    ) -> Result<Self, ProjectMapAtlasError> {
        let publication = published.publication();
        let symbols = publication
            .graph()
            .symbols()
            .iter()
            .map(|symbol| (symbol.id(), symbol))
            .collect::<BTreeMap<_, _>>();
        let primary_modules = publication
            .modules()
            .memberships()
            .iter()
            .filter(|membership| membership.evidence().kind().is_primary())
            .map(|membership| (membership.symbol_id(), membership.module_id()))
            .collect::<BTreeMap<_, _>>();
        let mut file_modules = BTreeMap::<RepositoryPath, Option<ModuleId>>::new();
        for symbol in publication.graph().symbols() {
            let Some(module_id) = primary_modules.get(&symbol.id()).copied() else {
                continue;
            };
            file_modules
                .entry(symbol.revision().path().clone())
                .and_modify(|current| {
                    if *current != Some(module_id) {
                        *current = None;
                    }
                })
                .or_insert(Some(module_id));
        }
        let ranks = publication
            .ranking()
            .symbols()
            .iter()
            .enumerate()
            .map(|(index, rank)| (rank.symbol_id(), index))
            .collect::<BTreeMap<_, _>>();
        let insight_count = insights.len();
        let insights = insights
            .iter()
            .map(|insight| (insight.module_id(), insight))
            .collect::<BTreeMap<_, _>>();
        if insights.len() != insight_count {
            return Err(ProjectMapAtlasError);
        }
        Ok(Self {
            published,
            symbols,
            primary_modules,
            file_modules,
            ranks,
            insights,
        })
    }

    fn run_id(&self) -> IndexRunId {
        self.published.run().id()
    }

    fn snapshot_id(&self) -> SnapshotId {
        self.published.run().snapshot_id()
    }

    fn scene(
        &self,
        selection: Option<ProjectMapEntitySelection>,
    ) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
        match selection {
            None => self.project_scene().map(Some),
            Some(ProjectMapEntitySelection::Module { module_id }) => self.module_scene(module_id),
            Some(selection @ ProjectMapEntitySelection::File { .. }) => self.file_scene(selection),
            Some(selection @ ProjectMapEntitySelection::Symbol { .. }) => {
                self.symbol_scene(selection)
            }
        }
    }

    fn index_evidence(
        &self,
        selection: ProjectMapIndexEvidenceSelection,
    ) -> Result<Option<ProjectMapIndexEvidenceTarget>, ProjectMapAtlasError> {
        let target = match selection {
            ProjectMapIndexEvidenceSelection::File {
                module_id,
                ordinal,
                evidence_id,
            } => {
                let entity = ProjectMapEntitySelection::File {
                    module_id,
                    ordinal,
                    evidence_id,
                };
                let Some(file) = self.resolve_file(entity) else {
                    return Ok(None);
                };
                ProjectMapIndexEvidenceTarget {
                    revision: file.revision.clone(),
                    range: None,
                }
            }
            ProjectMapIndexEvidenceSelection::Symbol {
                module_id,
                symbol_id,
                evidence_id,
            } => {
                let entity = ProjectMapEntitySelection::Symbol {
                    module_id,
                    symbol_id,
                    evidence_id,
                };
                let Some(symbol) = self.resolve_symbol(entity) else {
                    return Ok(None);
                };
                ProjectMapIndexEvidenceTarget {
                    revision: symbol.revision().clone(),
                    range: Some(symbol.parsed().declaration_range()),
                }
            }
            ProjectMapIndexEvidenceSelection::Relation {
                module_id,
                edge_sequence,
                evidence_id,
            } => {
                let Some(index) = edge_sequence
                    .checked_sub(1)
                    .and_then(|value| usize::try_from(value).ok())
                else {
                    return Ok(None);
                };
                let Some(edge) = self.published.publication().graph().edges().get(index) else {
                    return Ok(None);
                };
                if self
                    .endpoint_module(edge.source())
                    .or_else(|| self.endpoint_module(edge.target()))
                    != Some(module_id)
                    || ModuleCardEvidenceId::for_graph_edge_v1(edge) != evidence_id
                {
                    return Ok(None);
                }
                ProjectMapIndexEvidenceTarget {
                    revision: edge.evidence().revision().clone(),
                    range: Some(edge.evidence().range()),
                }
            }
            ProjectMapIndexEvidenceSelection::UnresolvedRelation {
                module_id,
                candidate_sequence,
                evidence_id,
            } => {
                let Some(index) = candidate_sequence
                    .checked_sub(1)
                    .and_then(|value| usize::try_from(value).ok())
                else {
                    return Ok(None);
                };
                let Some(candidate) = self.published.publication().graph().unresolved().get(index)
                else {
                    return Ok(None);
                };
                if self.endpoint_module(candidate.source()) != Some(module_id)
                    || ModuleCardEvidenceId::for_file_revision_v1(candidate.evidence().revision())
                        != evidence_id
                {
                    return Ok(None);
                }
                ProjectMapIndexEvidenceTarget {
                    revision: candidate.evidence().revision().clone(),
                    range: Some(candidate.evidence().range()),
                }
            }
        };
        Ok(Some(target))
    }

    fn project_scene(&self) -> Result<ProjectMapAtlasScene, ProjectMapAtlasError> {
        let mut modules = self
            .published
            .publication()
            .modules()
            .modules()
            .iter()
            .filter(|module| module.kind().is_primary())
            .collect::<Vec<_>>();
        modules.sort_by(|left, right| {
            module_priority(left.kind())
                .cmp(&module_priority(right.kind()))
                .then_with(|| {
                    right
                        .entrypoints()
                        .symbols()
                        .is_empty()
                        .cmp(&left.entrypoints().symbols().is_empty())
                })
                .then_with(|| {
                    right
                        .tests()
                        .symbols()
                        .is_empty()
                        .cmp(&left.tests().symbols().is_empty())
                })
                .then_with(|| {
                    right
                        .central_symbols()
                        .symbols()
                        .is_empty()
                        .cmp(&left.central_symbols().symbols().is_empty())
                })
                .then_with(|| left.id().cmp(&right.id()))
        });
        let node_count = count(modules.len())?;
        modules.truncate(PROJECT_MAP_ATLAS_MODULE_LIMIT);
        let mut nodes = Vec::with_capacity(modules.len());
        for (index, module) in modules.iter().enumerate() {
            let module_id = module.id();
            let insight = self.insights.get(&module_id).copied();
            let file_count = count(self.ranked_files(module_id).len())?;
            let symbol_count = count(
                self.primary_modules
                    .values()
                    .filter(|candidate| **candidate == module_id)
                    .count(),
            )?;
            nodes.push(ProjectMapAtlasNode::new(
                module_node_id(module_id),
                None,
                Some(ProjectMapEntitySelection::Module { module_id }),
                match module.kind() {
                    ModuleKind::ManifestBoundary => ProjectMapAtlasNodeKind::ManifestModule,
                    ModuleKind::PathBoundary => ProjectMapAtlasNodeKind::PathModule,
                    ModuleKind::GraphCommunity => return Err(ProjectMapAtlasError),
                },
                module_display_name(module.root()),
                Some(format!("{} Dateien · {} Symbole", file_count, symbol_count)),
                rank(index)?,
                file_count.max(1),
                file_count,
                symbol_count,
                0,
                Some(insight.map_or(
                    ProjectMapMappingStatus::Unmapped,
                    ProjectMapAtlasModuleInsight::mapping_status,
                )),
                insight
                    .and_then(ProjectMapAtlasModuleInsight::purpose)
                    .map(str::to_owned),
                insight.map_or(0, ProjectMapAtlasModuleInsight::current_risk_count),
                None,
                0,
                false,
            )?);
        }
        let visible = nodes
            .iter()
            .filter_map(|node| match node.selection() {
                Some(ProjectMapEntitySelection::Module { module_id }) => {
                    Some((module_id, node.id()))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let grouped = self.group_module_relations(&visible);
        let relation_count = count(grouped.len())?;
        let relations = grouped
            .into_iter()
            .take(PROJECT_MAP_ATLAS_RELATION_LIMIT)
            .map(|group| self.relation_from_group(group))
            .collect::<Result<Vec<_>, _>>()?;
        ProjectMapAtlasScene::new(
            self.run_id(),
            self.snapshot_id(),
            ProjectMapAtlasLevel::Project,
            None,
            vec![ProjectMapAtlasBreadcrumb::new("Projekt".to_owned(), None)?],
            nodes,
            node_count,
            relations,
            relation_count,
            0,
            0,
            count(
                self.published
                    .publication()
                    .graph()
                    .edges()
                    .len()
                    .min(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT),
            )?,
            node_count > modules.len() as u64,
            relation_count > PROJECT_MAP_ATLAS_RELATION_LIMIT as u64,
            false,
            self.published.publication().graph().edges().len()
                > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
    }

    fn module_scene(
        &self,
        module_id: ModuleId,
    ) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
        let Some(module) = self.primary_module(module_id) else {
            return Ok(None);
        };
        let files = self.ranked_files(module_id);
        let node_count = count(files.len())?;
        let visible_files = files
            .iter()
            .take(PROJECT_MAP_ATLAS_FILE_LIMIT)
            .collect::<Vec<_>>();
        let mut nodes = visible_files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                self.file_node(
                    module_id,
                    file,
                    rank(index)?,
                    ProjectMapFileOrdinal::new(
                        u32::try_from(index + 1).map_err(|_| ProjectMapAtlasError)?,
                    )?,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let visible = nodes
            .iter()
            .filter_map(|node| match node.selection() {
                Some(ProjectMapEntitySelection::File { ordinal, .. }) => files
                    .get(usize::try_from(ordinal.get().saturating_sub(1)).ok()?)
                    .map(|file| (file.revision.path().clone(), node.id())),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let grouped = self.group_file_relations(&visible);
        let mut relations = grouped
            .iter()
            .take(PROJECT_MAP_ATLAS_RELATION_LIMIT)
            .cloned()
            .map(|group| self.relation_from_group(group))
            .collect::<Result<Vec<_>, _>>()?;
        let normal_relation_count = count(grouped.len())?;
        let boundary = self.boundaries_for_visible(&visible, &BTreeMap::new(), nodes.len())?;
        let boundary_count = boundary.total;
        nodes.extend(boundary.nodes);
        relations.extend(boundary.relations);
        let relation_count = normal_relation_count.saturating_add(boundary_count);
        relations.truncate(PROJECT_MAP_ATLAS_RELATION_LIMIT);
        rerank_nodes(&mut nodes)?;
        ProjectMapAtlasScene::new(
            self.run_id(),
            self.snapshot_id(),
            ProjectMapAtlasLevel::Module,
            Some(ProjectMapEntitySelection::Module { module_id }),
            vec![
                ProjectMapAtlasBreadcrumb::new("Projekt".to_owned(), None)?,
                ProjectMapAtlasBreadcrumb::new(
                    module_display_name(module.root()),
                    Some(ProjectMapEntitySelection::Module { module_id }),
                )?,
            ],
            nodes,
            node_count,
            relations,
            relation_count,
            boundary_count,
            boundary_count,
            count(
                self.published
                    .publication()
                    .graph()
                    .edges()
                    .len()
                    .min(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT),
            )?,
            node_count > PROJECT_MAP_ATLAS_FILE_LIMIT as u64,
            relation_count > PROJECT_MAP_ATLAS_RELATION_LIMIT as u64,
            boundary_count > PROJECT_MAP_ATLAS_BOUNDARY_LIMIT as u64,
            self.published.publication().graph().edges().len()
                > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
        .map(Some)
    }

    fn file_scene(
        &self,
        selection: ProjectMapEntitySelection,
    ) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
        let Some(file) = self.resolve_file(selection) else {
            return Ok(None);
        };
        let module_id = selection.module_id();
        let Some(module) = self.primary_module(module_id) else {
            return Ok(None);
        };
        let symbols = self.ranked_file_symbols(file.revision.path());
        let node_count = count(symbols.len())?;
        let selected_symbols = symbols
            .iter()
            .take(PROJECT_MAP_ATLAS_SYMBOL_LIMIT)
            .copied()
            .collect::<Vec<_>>();
        let selected_ids = selected_symbols
            .iter()
            .map(|symbol| symbol.id())
            .collect::<BTreeSet<_>>();
        let parents = self.symbol_parents(&selected_ids);
        let mut nodes = selected_symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| {
                self.symbol_node(symbol, rank(index)?, parents.get(&symbol.id()).copied())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let visible_symbols = nodes
            .iter()
            .filter_map(|node| match node.selection() {
                Some(ProjectMapEntitySelection::Symbol { symbol_id, .. }) => {
                    Some((symbol_id, node.id()))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let grouped = self.group_symbol_relations(&visible_symbols, true);
        let normal_relation_count = count(grouped.len())?;
        let mut relations = grouped
            .iter()
            .take(PROJECT_MAP_ATLAS_RELATION_LIMIT)
            .cloned()
            .map(|group| self.relation_from_group(group))
            .collect::<Result<Vec<_>, _>>()?;
        let boundary =
            self.boundaries_for_visible(&BTreeMap::new(), &visible_symbols, nodes.len())?;
        let boundary_count = boundary.total;
        nodes.extend(boundary.nodes);
        relations.extend(boundary.relations);
        relations.truncate(PROJECT_MAP_ATLAS_RELATION_LIMIT);
        rerank_nodes(&mut nodes)?;
        let relation_count = normal_relation_count.saturating_add(boundary_count);
        ProjectMapAtlasScene::new(
            self.run_id(),
            self.snapshot_id(),
            ProjectMapAtlasLevel::File,
            Some(selection),
            vec![
                ProjectMapAtlasBreadcrumb::new("Projekt".to_owned(), None)?,
                ProjectMapAtlasBreadcrumb::new(
                    module_display_name(module.root()),
                    Some(ProjectMapEntitySelection::Module { module_id }),
                )?,
                ProjectMapAtlasBreadcrumb::new(
                    path_display(file.revision.path()),
                    Some(selection),
                )?,
            ],
            nodes,
            node_count,
            relations,
            relation_count,
            boundary_count,
            boundary_count,
            count(
                self.published
                    .publication()
                    .graph()
                    .edges()
                    .len()
                    .min(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT),
            )?,
            node_count > PROJECT_MAP_ATLAS_SYMBOL_LIMIT as u64,
            relation_count > PROJECT_MAP_ATLAS_RELATION_LIMIT as u64,
            boundary_count > PROJECT_MAP_ATLAS_BOUNDARY_LIMIT as u64,
            self.published.publication().graph().edges().len()
                > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
        .map(Some)
    }

    fn symbol_scene(
        &self,
        selection: ProjectMapEntitySelection,
    ) -> Result<Option<ProjectMapAtlasScene>, ProjectMapAtlasError> {
        let Some(root) = self.resolve_symbol(selection) else {
            return Ok(None);
        };
        let module_id = selection.module_id();
        let Some(module) = self.primary_module(module_id) else {
            return Ok(None);
        };
        let mut candidates = BTreeMap::<SymbolId, NeighborFacts>::new();
        for (index, edge) in self
            .published
            .publication()
            .graph()
            .edges()
            .iter()
            .take(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT)
            .enumerate()
        {
            let neighbor = match (edge.source(), edge.target()) {
                (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target))
                    if *source == root.id() =>
                {
                    Some(*target)
                }
                (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target))
                    if *target == root.id() =>
                {
                    Some(*source)
                }
                _ => None,
            };
            let Some(neighbor) = neighbor else { continue };
            if !is_symbol_scene_relation(edge.kind()) || !self.symbols.contains_key(&neighbor) {
                continue;
            }
            let facts = candidates.entry(neighbor).or_insert(NeighborFacts {
                evidence_count: 0,
                confidence: 0,
                priority: relation_priority(edge.kind()),
                edge_index: index,
            });
            facts.evidence_count = facts.evidence_count.saturating_add(1);
            if edge.confidence().basis_points() > facts.confidence {
                facts.confidence = edge.confidence().basis_points();
                facts.edge_index = index;
            }
            facts.priority = facts.priority.min(relation_priority(edge.kind()));
        }
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        candidates.sort_by(|(left_id, left), (right_id, right)| {
            right
                .evidence_count
                .cmp(&left.evidence_count)
                .then_with(|| right.confidence.cmp(&left.confidence))
                .then_with(|| left.priority.cmp(&right.priority))
                .then_with(|| left_id.cmp(right_id))
        });
        let node_count = count(candidates.len().saturating_add(1))?;
        candidates.truncate(PROJECT_MAP_ATLAS_SYMBOL_NEIGHBOR_LIMIT.saturating_sub(1));
        let mut nodes = vec![self.symbol_node(root, 1, None)?];
        for (index, (symbol_id, _)) in candidates.iter().enumerate() {
            let symbol = self
                .symbols
                .get(symbol_id)
                .copied()
                .ok_or(ProjectMapAtlasError)?;
            nodes.push(self.symbol_node(symbol, rank(index + 1)?, None)?);
        }
        let visible = nodes
            .iter()
            .filter_map(|node| match node.selection() {
                Some(ProjectMapEntitySelection::Symbol { symbol_id, .. }) => {
                    Some((symbol_id, node.id()))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let grouped = self.group_symbol_relations(&visible, false);
        let normal_relation_count = count(grouped.len())?;
        let mut relations = grouped
            .iter()
            .take(PROJECT_MAP_ATLAS_RELATION_LIMIT)
            .cloned()
            .map(|group| self.relation_from_group(group))
            .collect::<Result<Vec<_>, _>>()?;
        let boundary = self.boundaries_for_visible(&BTreeMap::new(), &visible, nodes.len())?;
        let boundary_count = boundary.total;
        nodes.extend(boundary.nodes);
        relations.extend(boundary.relations);
        relations.truncate(PROJECT_MAP_ATLAS_RELATION_LIMIT);
        rerank_nodes(&mut nodes)?;
        let relation_count = normal_relation_count.saturating_add(boundary_count);
        let file_selection = self.file_selection_for_revision(module_id, root.revision())?;
        ProjectMapAtlasScene::new(
            self.run_id(),
            self.snapshot_id(),
            ProjectMapAtlasLevel::Symbol,
            Some(selection),
            vec![
                ProjectMapAtlasBreadcrumb::new("Projekt".to_owned(), None)?,
                ProjectMapAtlasBreadcrumb::new(
                    module_display_name(module.root()),
                    Some(ProjectMapEntitySelection::Module { module_id }),
                )?,
                ProjectMapAtlasBreadcrumb::new(
                    path_display(root.revision().path()),
                    Some(file_selection),
                )?,
                ProjectMapAtlasBreadcrumb::new(
                    safe_text(root.parsed().name().as_str(), MAX_DISPLAY_BYTES),
                    Some(selection),
                )?,
            ],
            nodes,
            node_count,
            relations,
            relation_count,
            boundary_count,
            boundary_count,
            count(
                self.published
                    .publication()
                    .graph()
                    .edges()
                    .len()
                    .min(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT),
            )?,
            node_count > PROJECT_MAP_ATLAS_SYMBOL_NEIGHBOR_LIMIT as u64,
            relation_count > PROJECT_MAP_ATLAS_RELATION_LIMIT as u64,
            boundary_count > PROJECT_MAP_ATLAS_BOUNDARY_LIMIT as u64,
            self.published.publication().graph().edges().len()
                > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
        .map(Some)
    }

    fn context(
        &self,
        selection: ProjectMapEntitySelection,
    ) -> Result<Option<ProjectMapEntityContext>, ProjectMapAtlasError> {
        let Some(entity) = self.entity_node(selection, 1)? else {
            return Ok(None);
        };
        let mut counts = BTreeMap::<SyntaxRelationKind, (u64, u64)>::new();
        let mut related = BTreeMap::<
            (ProjectMapAtlasNodeId, SyntaxRelationKind),
            (ProjectMapAtlasNode, EdgeGroup),
        >::new();
        let mut documents = 0_u64;
        let edges = self.published.publication().graph().edges();
        for (index, edge) in edges
            .iter()
            .take(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT)
            .enumerate()
        {
            let incoming = self.selection_matches_endpoint(selection, edge.target());
            let outgoing = self.selection_matches_endpoint(selection, edge.source());
            if !incoming && !outgoing {
                continue;
            }
            let entry = counts.entry(edge.kind()).or_default();
            if incoming {
                entry.0 = entry.0.saturating_add(1);
            }
            if outgoing {
                entry.1 = entry.1.saturating_add(1);
            }
            if edge.kind() == SyntaxRelationKind::Documents {
                documents = documents.saturating_add(1);
                continue;
            }
            if !is_architecture_relation(edge.kind()) {
                continue;
            }
            let neighbor_endpoint = if outgoing {
                edge.target()
            } else {
                edge.source()
            };
            let Some(mut neighbor) = self.node_for_endpoint(neighbor_endpoint, 1)? else {
                continue;
            };
            let key = (neighbor.id(), edge.kind());
            if let Some((_, group)) = related.get_mut(&key) {
                group.evidence_count = group.evidence_count.saturating_add(1);
                if edge.confidence().basis_points() > group.confidence {
                    group.confidence = edge.confidence().basis_points();
                    group.edge_index = index;
                }
            } else {
                let endpoints = if outgoing {
                    (entity.id(), neighbor.id())
                } else {
                    (neighbor.id(), entity.id())
                };
                neighbor.rank = 1;
                related.insert(
                    key,
                    (
                        neighbor,
                        EdgeGroup {
                            source: endpoints.0,
                            target: endpoints.1,
                            relation: edge.kind(),
                            evidence_count: 1,
                            confidence: edge.confidence().basis_points(),
                            edge_index: index,
                        },
                    ),
                );
            }
        }
        let relation_counts = counts
            .into_iter()
            .map(|(kind, (incoming, outgoing))| {
                ProjectMapRelationCount::new(kind, incoming, outgoing)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let architecture_relation_count = count(related.len())?;
        let mut related_nodes = Vec::new();
        let mut architecture_relations = Vec::new();
        for (index, (_, (mut node, group))) in related.into_iter().take(32).enumerate() {
            node.rank = rank(index)?;
            related_nodes.push(node);
            architecture_relations.push(self.relation_from_group(group)?);
        }
        let visible_files = match selection {
            ProjectMapEntitySelection::File { .. } => {
                let file = self.resolve_file(selection).ok_or(ProjectMapAtlasError)?;
                BTreeMap::from([(file.revision.path().clone(), entity.id())])
            }
            _ => BTreeMap::new(),
        };
        let visible_symbols = match selection {
            ProjectMapEntitySelection::Symbol { symbol_id, .. } => {
                BTreeMap::from([(symbol_id, entity.id())])
            }
            _ => BTreeMap::new(),
        };
        let boundary = self.boundaries_for_visible(&visible_files, &visible_symbols, 0)?;
        ProjectMapEntityContext::new(
            self.run_id(),
            self.snapshot_id(),
            entity,
            relation_counts,
            related_nodes,
            architecture_relations,
            architecture_relation_count,
            boundary.nodes,
            boundary.relations,
            boundary.total,
            documents,
            selection
                .evidence_id()
                .and_then(|evidence_id| {
                    self.insights.get(&selection.module_id()).map(|insight| {
                        insight
                            .claims_for(evidence_id)
                            .iter()
                            .copied()
                            .take(64)
                            .collect::<Vec<_>>()
                    })
                })
                .unwrap_or_default(),
            edges.len() > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
        .map(Some)
    }

    fn inventory(
        &self,
        query: &ProjectMapInventoryPageQuery,
    ) -> Result<Option<ProjectMapInventoryPage>, ProjectMapAtlasError> {
        if self.entity_node(query.selection(), 1)?.is_none() {
            return Ok(None);
        }
        let offset = match query.cursor() {
            None => 0_usize,
            Some(cursor) => {
                match decode_cursor(self.run_id(), query.selection(), query.view(), cursor) {
                    Some(value) => value,
                    None => return Ok(None),
                }
            }
        };
        let mut nodes = match (query.selection(), query.view()) {
            (ProjectMapEntitySelection::Module { module_id }, ProjectMapInventoryView::Files) => {
                let files = self.ranked_files(module_id);
                files
                    .iter()
                    .enumerate()
                    .map(|(index, file)| {
                        self.file_node(
                            module_id,
                            file,
                            1,
                            ProjectMapFileOrdinal::new(
                                u32::try_from(index + 1).map_err(|_| ProjectMapAtlasError)?,
                            )?,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            (ProjectMapEntitySelection::Module { module_id }, ProjectMapInventoryView::Symbols) => {
                self.ranked_module_symbols(module_id)
                    .into_iter()
                    .map(|symbol| self.symbol_node(symbol, 1, None))
                    .collect::<Result<Vec<_>, _>>()?
            }
            (
                selection @ ProjectMapEntitySelection::File { .. },
                ProjectMapInventoryView::Symbols,
            ) => {
                let Some(file) = self.resolve_file(selection) else {
                    return Ok(None);
                };
                self.ranked_all_file_symbols(file.revision.path())
                    .into_iter()
                    .map(|symbol| self.symbol_node(symbol, 1, None))
                    .collect::<Result<Vec<_>, _>>()?
            }
            (
                selection @ ProjectMapEntitySelection::Symbol { .. },
                ProjectMapInventoryView::Members,
            ) => {
                let Some(symbol) = self.resolve_symbol(selection) else {
                    return Ok(None);
                };
                self.direct_member_symbols(symbol.id())
                    .into_iter()
                    .map(|member| self.symbol_node(member, 1, Some(symbol_node_id(symbol.id()))))
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => return Ok(None),
        };
        let total_count = count(nodes.len())?;
        if offset > nodes.len() || offset % PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE != 0 {
            return Ok(None);
        }
        nodes = nodes
            .into_iter()
            .skip(offset)
            .take(PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE)
            .collect();
        rerank_nodes(&mut nodes)?;
        let page_number = u32::try_from(offset / PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE + 1)
            .map_err(|_| ProjectMapAtlasError)?;
        let previous_cursor = offset
            .checked_sub(PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE)
            .map(|previous| encode_cursor(self.run_id(), query.selection(), query.view(), previous))
            .transpose()?;
        let next_offset = offset.saturating_add(PROJECT_MAP_ATLAS_INVENTORY_PAGE_SIZE);
        let next_cursor = (next_offset
            < usize::try_from(total_count).map_err(|_| ProjectMapAtlasError)?)
        .then(|| encode_cursor(self.run_id(), query.selection(), query.view(), next_offset))
        .transpose()?;
        ProjectMapInventoryPage::new(
            self.run_id(),
            self.snapshot_id(),
            query.selection(),
            query.view(),
            page_number,
            total_count,
            nodes,
            previous_cursor,
            next_cursor,
        )
        .map(Some)
    }

    fn flow(
        &self,
        query: &ProjectMapFlowSceneQuery,
    ) -> Result<Option<ProjectMapFlowScene>, ProjectMapAtlasError> {
        let root_endpoint = match query.selection() {
            selection @ ProjectMapEntitySelection::File { .. } => {
                let Some(file) = self.resolve_file(selection) else {
                    return Ok(None);
                };
                GraphEndpoint::File(file.revision.path().clone())
            }
            selection @ ProjectMapEntitySelection::Symbol { .. } => {
                let Some(symbol) = self.resolve_symbol(selection) else {
                    return Ok(None);
                };
                GraphEndpoint::Symbol(symbol.id())
            }
            ProjectMapEntitySelection::Module { .. } => return Ok(None),
        };
        let Some(root) = self.entity_node(query.selection(), 1)? else {
            return Ok(None);
        };
        let inspected = self
            .published
            .publication()
            .graph()
            .edges()
            .iter()
            .take(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT)
            .collect::<Vec<_>>();
        let max_depth = match query.preset() {
            ProjectMapFlowPreset::Callers | ProjectMapFlowPreset::Callees => 2,
            ProjectMapFlowPreset::Tests | ProjectMapFlowPreset::DataAccess => 1,
        };
        let mut queue = VecDeque::from([(root_endpoint.clone(), Vec::<usize>::new())]);
        let mut visited = BTreeSet::from([root_endpoint.clone()]);
        let mut found = Vec::<(GraphEndpoint, Vec<usize>)>::new();
        while let Some((current, path)) = queue.pop_front() {
            if path.len() >= max_depth {
                continue;
            }
            for (index, edge) in inspected.iter().enumerate() {
                let next = flow_next(query.preset(), &current, edge);
                let Some(next) = next else { continue };
                if !visited.insert(next.clone()) {
                    continue;
                }
                let mut next_path = path.clone();
                next_path.push(index);
                if self.node_for_endpoint(&next, 1)?.is_some() {
                    found.push((next.clone(), next_path.clone()));
                }
                queue.push_back((next, next_path));
            }
        }
        let target_count = count(found.len())?;
        found.truncate(PROJECT_MAP_ATLAS_FLOW_TARGET_LIMIT);
        let mut nodes = Vec::new();
        let mut targets = Vec::new();
        for (index, (endpoint, path_indices)) in found.into_iter().enumerate() {
            let mut node = self
                .node_for_endpoint(&endpoint, rank(index)?)?
                .ok_or(ProjectMapAtlasError)?;
            node.rank = rank(index)?;
            let mut path = Vec::new();
            let mut previous = root_endpoint.clone();
            for edge_index in path_indices.iter().copied() {
                let edge = inspected
                    .get(edge_index)
                    .copied()
                    .ok_or(ProjectMapAtlasError)?;
                let next =
                    flow_next(query.preset(), &previous, edge).ok_or(ProjectMapAtlasError)?;
                let source_id = if previous == root_endpoint {
                    root.id()
                } else {
                    self.node_for_endpoint(&previous, 1)?
                        .ok_or(ProjectMapAtlasError)?
                        .id()
                };
                let target_id = self
                    .node_for_endpoint(&next, 1)?
                    .ok_or(ProjectMapAtlasError)?
                    .id();
                path.push(ProjectMapFlowStep::new(
                    source_id,
                    target_id,
                    edge.kind(),
                    self.edge_selection(edge_index, edge)?,
                )?);
                previous = next;
            }
            targets.push(ProjectMapFlowTarget::new(
                node.id(),
                u8::try_from(path.len()).map_err(|_| ProjectMapAtlasError)?,
                path,
            )?);
            nodes.push(node);
        }
        ProjectMapFlowScene::new(
            self.run_id(),
            self.snapshot_id(),
            query.preset(),
            root,
            nodes,
            targets,
            target_count,
            count(inspected.len())?,
            target_count > PROJECT_MAP_ATLAS_FLOW_TARGET_LIMIT as u64,
            self.published.publication().graph().edges().len()
                > PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT,
        )
        .map(Some)
    }

    fn primary_module(&self, module_id: ModuleId) -> Option<&a3_domain::RepositoryModule> {
        self.published
            .publication()
            .modules()
            .modules()
            .iter()
            .find(|module| module.id() == module_id && module.kind().is_primary())
    }

    fn ranked_files(&self, module_id: ModuleId) -> Vec<RankedFile<'a>> {
        let Some(module) = self.primary_module(module_id) else {
            return Vec::new();
        };
        let manifest_paths = module
            .manifests()
            .iter()
            .map(|revision| revision.path())
            .collect::<BTreeSet<_>>();
        let mut files = BTreeMap::<RepositoryPath, RankedFile<'a>>::new();
        for symbol in self.symbols.values() {
            if self.primary_modules.get(&symbol.id()).copied() != Some(module_id) {
                continue;
            }
            let parsed = symbol.parsed();
            let entry = files
                .entry(symbol.revision().path().clone())
                .or_insert(RankedFile {
                    revision: symbol.revision(),
                    manifest: manifest_paths.contains(symbol.revision().path()),
                    entrypoint: false,
                    public_symbol: false,
                    test: false,
                    best_rank: usize::MAX,
                    structural_symbols: 0,
                });
            entry.entrypoint |= parsed.roles().contains(SymbolRole::Entrypoint);
            entry.test |= parsed.roles().contains(SymbolRole::Test);
            entry.public_symbol |= parsed.visibility() == SymbolVisibility::Public;
            entry.best_rank = entry
                .best_rank
                .min(self.ranks.get(&symbol.id()).copied().unwrap_or(usize::MAX));
            if is_file_scene_symbol(parsed.kind()) {
                entry.structural_symbols = entry.structural_symbols.saturating_add(1);
            }
        }
        let mut files = files.into_values().collect::<Vec<_>>();
        files.sort_by(|left, right| {
            right
                .manifest
                .cmp(&left.manifest)
                .then_with(|| right.entrypoint.cmp(&left.entrypoint))
                .then_with(|| right.public_symbol.cmp(&left.public_symbol))
                .then_with(|| right.test.cmp(&left.test))
                .then_with(|| left.best_rank.cmp(&right.best_rank))
                .then_with(|| left.revision.path().cmp(right.revision.path()))
        });
        files
    }

    fn ranked_file_symbols(&self, path: &RepositoryPath) -> Vec<&'a GraphSymbol> {
        self.ranked_all_file_symbols(path)
            .into_iter()
            .filter(|symbol| is_file_scene_symbol(symbol.parsed().kind()))
            .collect()
    }

    fn ranked_all_file_symbols(&self, path: &RepositoryPath) -> Vec<&'a GraphSymbol> {
        let mut symbols = self
            .symbols
            .values()
            .copied()
            .filter(|symbol| symbol.revision().path() == path)
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| symbol_compare(left, right, &self.ranks));
        symbols
    }

    fn ranked_module_symbols(&self, module_id: ModuleId) -> Vec<&'a GraphSymbol> {
        let mut symbols = self
            .symbols
            .values()
            .copied()
            .filter(|symbol| self.primary_modules.get(&symbol.id()).copied() == Some(module_id))
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| symbol_compare(left, right, &self.ranks));
        symbols
    }

    fn direct_member_symbols(&self, symbol_id: SymbolId) -> Vec<&'a GraphSymbol> {
        let mut members = self
            .published
            .publication()
            .graph()
            .edges()
            .iter()
            .filter_map(|edge| match (edge.source(), edge.target(), edge.kind()) {
                (
                    GraphEndpoint::Symbol(source),
                    GraphEndpoint::Symbol(target),
                    SyntaxRelationKind::Contains | SyntaxRelationKind::Defines,
                ) if *source == symbol_id => self.symbols.get(target).copied(),
                _ => None,
            })
            .collect::<Vec<_>>();
        members.sort_by(|left, right| symbol_compare(left, right, &self.ranks));
        members.dedup_by_key(|symbol| symbol.id());
        members
    }

    fn resolve_file(&self, selection: ProjectMapEntitySelection) -> Option<RankedFile<'a>> {
        let ProjectMapEntitySelection::File {
            module_id,
            ordinal,
            evidence_id,
        } = selection
        else {
            return None;
        };
        let index = usize::try_from(ordinal.get().checked_sub(1)?).ok()?;
        let file = self.ranked_files(module_id).get(index).cloned()?;
        (ModuleCardEvidenceId::for_file_revision_v1(file.revision) == evidence_id).then_some(file)
    }

    fn resolve_symbol(&self, selection: ProjectMapEntitySelection) -> Option<&'a GraphSymbol> {
        let ProjectMapEntitySelection::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } = selection
        else {
            return None;
        };
        let symbol = self.symbols.get(&symbol_id).copied()?;
        (self.primary_modules.get(&symbol_id).copied() == Some(module_id)
            && ModuleCardEvidenceId::for_symbol_id_v1(symbol_id) == evidence_id)
            .then_some(symbol)
    }

    fn file_selection_for_revision(
        &self,
        module_id: ModuleId,
        revision: &FileRevision,
    ) -> Result<ProjectMapEntitySelection, ProjectMapAtlasError> {
        let position = self
            .ranked_files(module_id)
            .iter()
            .position(|file| file.revision == revision)
            .ok_or(ProjectMapAtlasError)?;
        Ok(ProjectMapEntitySelection::File {
            module_id,
            ordinal: ProjectMapFileOrdinal::new(
                u32::try_from(position + 1).map_err(|_| ProjectMapAtlasError)?,
            )?,
            evidence_id: ModuleCardEvidenceId::for_file_revision_v1(revision),
        })
    }

    fn file_node(
        &self,
        module_id: ModuleId,
        file: &RankedFile<'a>,
        display_rank: u16,
        ordinal: ProjectMapFileOrdinal,
    ) -> Result<ProjectMapAtlasNode, ProjectMapAtlasError> {
        let evidence_id = ModuleCardEvidenceId::for_file_revision_v1(file.revision);
        let claim_badge_count = self.claim_badge_count(module_id, evidence_id)?;
        let landmarks = self
            .ranked_file_symbols(file.revision.path())
            .into_iter()
            .take(3)
            .map(|symbol| symbol.parsed().name().as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        ProjectMapAtlasNode::new(
            file_node_id(evidence_id),
            Some(module_node_id(module_id)),
            Some(ProjectMapEntitySelection::File {
                module_id,
                ordinal,
                evidence_id,
            }),
            ProjectMapAtlasNodeKind::File,
            file_name_display(file.revision.path()),
            (!landmarks.is_empty()).then_some(safe_text(&landmarks, MAX_DETAIL_BYTES)),
            display_rank,
            file.structural_symbols.max(1),
            1,
            file.structural_symbols,
            0,
            None,
            None,
            0,
            Some(evidence_id),
            claim_badge_count,
            false,
        )
    }

    fn symbol_node(
        &self,
        symbol: &'a GraphSymbol,
        display_rank: u16,
        parent_id: Option<ProjectMapAtlasNodeId>,
    ) -> Result<ProjectMapAtlasNode, ProjectMapAtlasError> {
        let module_id = self
            .primary_modules
            .get(&symbol.id())
            .copied()
            .ok_or(ProjectMapAtlasError)?;
        let evidence_id = ModuleCardEvidenceId::for_symbol_id_v1(symbol.id());
        let claim_badge_count = self.claim_badge_count(module_id, evidence_id)?;
        let member_count = count(self.direct_member_symbols(symbol.id()).len())?;
        let detail = symbol
            .parsed()
            .signature()
            .map(|signature| safe_text(signature.as_str(), MAX_DETAIL_BYTES))
            .or_else(|| {
                Some(format!(
                    "{:?} · {:?}",
                    symbol.parsed().kind(),
                    symbol.parsed().visibility()
                ))
            });
        let volume = u64::from(
            symbol
                .parsed()
                .declaration_range()
                .end_byte()
                .saturating_sub(symbol.parsed().declaration_range().start_byte()),
        )
        .max(1);
        ProjectMapAtlasNode::new(
            symbol_node_id(symbol.id()),
            parent_id.or_else(|| {
                Some(file_node_id(ModuleCardEvidenceId::for_file_revision_v1(
                    symbol.revision(),
                )))
            }),
            Some(ProjectMapEntitySelection::Symbol {
                module_id,
                symbol_id: symbol.id(),
                evidence_id,
            }),
            project_map_node_kind_for_symbol(symbol.parsed().kind()),
            safe_text(symbol.parsed().name().as_str(), MAX_DISPLAY_BYTES),
            detail,
            display_rank,
            volume,
            0,
            1,
            member_count,
            None,
            None,
            0,
            Some(evidence_id),
            claim_badge_count,
            false,
        )
    }

    fn entity_node(
        &self,
        selection: ProjectMapEntitySelection,
        display_rank: u16,
    ) -> Result<Option<ProjectMapAtlasNode>, ProjectMapAtlasError> {
        match selection {
            ProjectMapEntitySelection::Module { module_id } => {
                let Some(module) = self.primary_module(module_id) else {
                    return Ok(None);
                };
                let files = count(self.ranked_files(module_id).len())?;
                let symbols = count(
                    self.primary_modules
                        .values()
                        .filter(|candidate| **candidate == module_id)
                        .count(),
                )?;
                let insight = self.insights.get(&module_id).copied();
                ProjectMapAtlasNode::new(
                    module_node_id(module_id),
                    None,
                    Some(selection),
                    match module.kind() {
                        ModuleKind::ManifestBoundary => ProjectMapAtlasNodeKind::ManifestModule,
                        ModuleKind::PathBoundary => ProjectMapAtlasNodeKind::PathModule,
                        ModuleKind::GraphCommunity => return Ok(None),
                    },
                    module_display_name(module.root()),
                    Some(format!("{} Dateien · {} Symbole", files, symbols)),
                    display_rank,
                    files.max(1),
                    files,
                    symbols,
                    0,
                    Some(insight.map_or(
                        ProjectMapMappingStatus::Unmapped,
                        ProjectMapAtlasModuleInsight::mapping_status,
                    )),
                    insight
                        .and_then(ProjectMapAtlasModuleInsight::purpose)
                        .map(str::to_owned),
                    insight.map_or(0, ProjectMapAtlasModuleInsight::current_risk_count),
                    None,
                    0,
                    false,
                )
                .map(Some)
            }
            selection @ ProjectMapEntitySelection::File { .. } => {
                let Some(file) = self.resolve_file(selection) else {
                    return Ok(None);
                };
                self.file_node(
                    selection.module_id(),
                    &file,
                    display_rank,
                    match selection {
                        ProjectMapEntitySelection::File { ordinal, .. } => ordinal,
                        _ => return Err(ProjectMapAtlasError),
                    },
                )
                .map(Some)
            }
            selection @ ProjectMapEntitySelection::Symbol { .. } => {
                let Some(symbol) = self.resolve_symbol(selection) else {
                    return Ok(None);
                };
                self.symbol_node(symbol, display_rank, None).map(Some)
            }
        }
    }

    fn node_for_endpoint(
        &self,
        endpoint: &GraphEndpoint,
        display_rank: u16,
    ) -> Result<Option<ProjectMapAtlasNode>, ProjectMapAtlasError> {
        match endpoint {
            GraphEndpoint::Symbol(symbol_id) => {
                let Some(module_id) = self.primary_modules.get(symbol_id).copied() else {
                    return Ok(None);
                };
                let selection = ProjectMapEntitySelection::Symbol {
                    module_id,
                    symbol_id: *symbol_id,
                    evidence_id: ModuleCardEvidenceId::for_symbol_id_v1(*symbol_id),
                };
                self.entity_node(selection, display_rank)
            }
            GraphEndpoint::File(path) => {
                let Some(Some(module_id)) = self.file_modules.get(path).copied() else {
                    return Ok(None);
                };
                let files = self.ranked_files(module_id);
                let Some(position) = files.iter().position(|file| file.revision.path() == path)
                else {
                    return Ok(None);
                };
                let file = &files[position];
                self.file_node(
                    module_id,
                    file,
                    display_rank,
                    ProjectMapFileOrdinal::new(
                        u32::try_from(position + 1).map_err(|_| ProjectMapAtlasError)?,
                    )?,
                )
                .map(Some)
            }
        }
    }

    fn symbol_parents(
        &self,
        visible: &BTreeSet<SymbolId>,
    ) -> BTreeMap<SymbolId, ProjectMapAtlasNodeId> {
        let mut parents = BTreeMap::new();
        for edge in self.published.publication().graph().edges() {
            if !matches!(
                edge.kind(),
                SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
            ) {
                continue;
            }
            if let (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) =
                (edge.source(), edge.target())
                && visible.contains(source)
                && visible.contains(target)
            {
                parents
                    .entry(*target)
                    .or_insert_with(|| symbol_node_id(*source));
            }
        }
        parents
    }

    fn group_module_relations(
        &self,
        visible: &BTreeMap<ModuleId, ProjectMapAtlasNodeId>,
    ) -> Vec<EdgeGroup> {
        self.group_relations(
            |endpoint| {
                self.endpoint_module(endpoint)
                    .and_then(|module| visible.get(&module).copied())
            },
            false,
        )
    }

    fn group_file_relations(
        &self,
        visible: &BTreeMap<RepositoryPath, ProjectMapAtlasNodeId>,
    ) -> Vec<EdgeGroup> {
        self.group_relations(
            |endpoint| {
                self.endpoint_path(endpoint)
                    .and_then(|path| visible.get(path).copied())
            },
            false,
        )
    }

    fn group_symbol_relations(
        &self,
        visible: &BTreeMap<SymbolId, ProjectMapAtlasNodeId>,
        include_containment: bool,
    ) -> Vec<EdgeGroup> {
        self.group_relations(
            |endpoint| match endpoint {
                GraphEndpoint::Symbol(symbol) => visible.get(symbol).copied(),
                GraphEndpoint::File(_) => None,
            },
            include_containment,
        )
    }

    fn group_relations(
        &self,
        mut resolve: impl FnMut(&GraphEndpoint) -> Option<ProjectMapAtlasNodeId>,
        include_containment: bool,
    ) -> Vec<EdgeGroup> {
        let mut groups = BTreeMap::<
            (
                ProjectMapAtlasNodeId,
                ProjectMapAtlasNodeId,
                SyntaxRelationKind,
            ),
            EdgeGroup,
        >::new();
        for (index, edge) in self
            .published
            .publication()
            .graph()
            .edges()
            .iter()
            .take(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT)
            .enumerate()
        {
            if !(is_architecture_relation(edge.kind())
                || include_containment
                    && matches!(
                        edge.kind(),
                        SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
                    ))
            {
                continue;
            }
            let (Some(source), Some(target)) = (resolve(edge.source()), resolve(edge.target()))
            else {
                continue;
            };
            if source == target {
                continue;
            }
            let key = (source, target, edge.kind());
            let group = groups.entry(key).or_insert(EdgeGroup {
                source,
                target,
                relation: edge.kind(),
                evidence_count: 0,
                confidence: 0,
                edge_index: index,
            });
            group.evidence_count = group.evidence_count.saturating_add(1);
            if edge.confidence().basis_points() > group.confidence {
                group.confidence = edge.confidence().basis_points();
                group.edge_index = index;
            }
        }
        let mut groups = groups.into_values().collect::<Vec<_>>();
        groups.sort_by(|left, right| {
            right
                .evidence_count
                .cmp(&left.evidence_count)
                .then_with(|| right.confidence.cmp(&left.confidence))
                .then_with(|| {
                    relation_priority(left.relation).cmp(&relation_priority(right.relation))
                })
                .then_with(|| {
                    (left.source, left.target, left.relation).cmp(&(
                        right.source,
                        right.target,
                        right.relation,
                    ))
                })
        });
        groups
    }

    fn relation_from_group(
        &self,
        group: EdgeGroup,
    ) -> Result<ProjectMapAtlasRelation, ProjectMapAtlasError> {
        let edge = self
            .published
            .publication()
            .graph()
            .edges()
            .get(group.edge_index)
            .ok_or(ProjectMapAtlasError)?;
        let evidence = self.edge_selection(group.edge_index, edge)?;
        ProjectMapAtlasRelation::new(
            group.source,
            group.target,
            group.relation,
            group.evidence_count,
            group.confidence,
            edge.provider(),
            Some(evidence),
            self.claim_badge_count(evidence.module_id(), evidence.evidence_id())?,
            None,
        )
    }

    fn edge_selection(
        &self,
        edge_index: usize,
        edge: &GraphEdge,
    ) -> Result<ProjectMapIndexEvidenceSelection, ProjectMapAtlasError> {
        let module_id = self
            .endpoint_module(edge.source())
            .or_else(|| self.endpoint_module(edge.target()))
            .ok_or(ProjectMapAtlasError)?;
        Ok(ProjectMapIndexEvidenceSelection::Relation {
            module_id,
            edge_sequence: count(edge_index.saturating_add(1))?,
            evidence_id: ModuleCardEvidenceId::for_graph_edge_v1(edge),
        })
    }

    fn boundaries_for_visible(
        &self,
        visible_files: &BTreeMap<RepositoryPath, ProjectMapAtlasNodeId>,
        visible_symbols: &BTreeMap<SymbolId, ProjectMapAtlasNodeId>,
        current_node_count: usize,
    ) -> Result<BoundaryProjection, ProjectMapAtlasError> {
        let mut nodes = Vec::new();
        let mut relations = Vec::new();
        let mut total = 0_u64;
        for (index, candidate) in self
            .published
            .publication()
            .graph()
            .unresolved()
            .iter()
            .take(PROJECT_MAP_ATLAS_INSPECTED_EDGE_LIMIT)
            .enumerate()
        {
            let source = match candidate.source() {
                GraphEndpoint::File(path) => visible_files.get(path).copied(),
                GraphEndpoint::Symbol(symbol_id) => visible_symbols.get(symbol_id).copied(),
            };
            let Some(source) = source else { continue };
            total = total.saturating_add(1);
            if nodes.len() == PROJECT_MAP_ATLAS_BOUNDARY_LIMIT {
                continue;
            }
            let display = match candidate.target() {
                UnresolvedGraphTarget::File(path) => path_display(path),
                UnresolvedGraphTarget::Reference(reference) => {
                    safe_text(reference.as_str(), MAX_DISPLAY_BYTES)
                }
            };
            let evidence_id =
                ModuleCardEvidenceId::for_file_revision_v1(candidate.evidence().revision());
            let target = boundary_node_id(evidence_id, &display, index);
            nodes.push(ProjectMapAtlasNode::new(
                target,
                None,
                None,
                ProjectMapAtlasNodeKind::Boundary,
                display,
                Some(format!(
                    "{:?} · {:?}",
                    candidate.reason(),
                    candidate.provider()
                )),
                rank(current_node_count + nodes.len())?,
                1,
                0,
                0,
                0,
                None,
                None,
                0,
                None,
                0,
                false,
            )?);
            let module_id = self
                .endpoint_module(candidate.source())
                .ok_or(ProjectMapAtlasError)?;
            relations.push(ProjectMapAtlasRelation::new(
                source,
                target,
                candidate.kind(),
                1,
                candidate.confidence().basis_points(),
                candidate.provider(),
                Some(ProjectMapIndexEvidenceSelection::UnresolvedRelation {
                    module_id,
                    candidate_sequence: count(index.saturating_add(1))?,
                    evidence_id,
                }),
                self.claim_badge_count(module_id, evidence_id)?,
                Some(candidate.reason().into()),
            )?);
        }
        Ok(BoundaryProjection {
            nodes,
            relations,
            total,
        })
    }

    fn endpoint_module(&self, endpoint: &GraphEndpoint) -> Option<ModuleId> {
        match endpoint {
            GraphEndpoint::Symbol(symbol) => self.primary_modules.get(symbol).copied(),
            GraphEndpoint::File(path) => self.file_modules.get(path).copied().flatten(),
        }
    }

    fn endpoint_path<'b>(&'b self, endpoint: &'b GraphEndpoint) -> Option<&'b RepositoryPath> {
        match endpoint {
            GraphEndpoint::File(path) => Some(path),
            GraphEndpoint::Symbol(symbol) => self
                .symbols
                .get(symbol)
                .map(|value| value.revision().path()),
        }
    }

    fn selection_matches_endpoint(
        &self,
        selection: ProjectMapEntitySelection,
        endpoint: &GraphEndpoint,
    ) -> bool {
        match selection {
            ProjectMapEntitySelection::Module { module_id } => {
                self.endpoint_module(endpoint) == Some(module_id)
            }
            selection @ ProjectMapEntitySelection::File { .. } => self
                .resolve_file(selection)
                .is_some_and(|file| self.endpoint_path(endpoint) == Some(file.revision.path())),
            ProjectMapEntitySelection::Symbol { symbol_id, .. } => {
                endpoint == &GraphEndpoint::Symbol(symbol_id)
            }
        }
    }

    fn claim_badge_count(
        &self,
        module_id: ModuleId,
        evidence_id: ModuleCardEvidenceId,
    ) -> Result<u16, ProjectMapAtlasError> {
        u16::try_from(
            self.insights
                .get(&module_id)
                .map_or(0, |insight| insight.claims_for(evidence_id).len()),
        )
        .map_err(|_| ProjectMapAtlasError)
    }
}

#[derive(Clone)]
struct RankedFile<'a> {
    revision: &'a FileRevision,
    manifest: bool,
    entrypoint: bool,
    public_symbol: bool,
    test: bool,
    best_rank: usize,
    structural_symbols: u64,
}

#[derive(Clone)]
struct EdgeGroup {
    source: ProjectMapAtlasNodeId,
    target: ProjectMapAtlasNodeId,
    relation: SyntaxRelationKind,
    evidence_count: u64,
    confidence: u16,
    edge_index: usize,
}

struct NeighborFacts {
    evidence_count: u64,
    confidence: u16,
    priority: u8,
    edge_index: usize,
}

struct BoundaryProjection {
    nodes: Vec<ProjectMapAtlasNode>,
    relations: Vec<ProjectMapAtlasRelation>,
    total: u64,
}

fn flow_next(
    preset: ProjectMapFlowPreset,
    current: &GraphEndpoint,
    edge: &GraphEdge,
) -> Option<GraphEndpoint> {
    match preset {
        ProjectMapFlowPreset::Callees
            if edge.kind() == SyntaxRelationKind::Calls && edge.source() == current =>
        {
            Some(edge.target().clone())
        }
        ProjectMapFlowPreset::Callers
            if edge.kind() == SyntaxRelationKind::Calls && edge.target() == current =>
        {
            Some(edge.source().clone())
        }
        ProjectMapFlowPreset::Tests
            if edge.kind() == SyntaxRelationKind::Tests && edge.source() == current =>
        {
            Some(edge.target().clone())
        }
        ProjectMapFlowPreset::Tests
            if edge.kind() == SyntaxRelationKind::Tests && edge.target() == current =>
        {
            Some(edge.source().clone())
        }
        ProjectMapFlowPreset::DataAccess
            if matches!(
                edge.kind(),
                SyntaxRelationKind::Reads | SyntaxRelationKind::Writes
            ) && edge.source() == current =>
        {
            Some(edge.target().clone())
        }
        ProjectMapFlowPreset::DataAccess
            if matches!(
                edge.kind(),
                SyntaxRelationKind::Reads | SyntaxRelationKind::Writes
            ) && edge.target() == current =>
        {
            Some(edge.source().clone())
        }
        _ => None,
    }
}

fn is_architecture_relation(kind: SyntaxRelationKind) -> bool {
    matches!(
        kind,
        SyntaxRelationKind::Imports
            | SyntaxRelationKind::Exports
            | SyntaxRelationKind::Implements
            | SyntaxRelationKind::Extends
            | SyntaxRelationKind::Builds
            | SyntaxRelationKind::Configures
    )
}

fn is_symbol_scene_relation(kind: SyntaxRelationKind) -> bool {
    is_architecture_relation(kind)
        || matches!(
            kind,
            SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
        )
}

fn is_file_scene_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module
            | SymbolKind::Namespace
            | SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Trait
            | SymbolKind::Interface
            | SymbolKind::Class
            | SymbolKind::Implementation
            | SymbolKind::TypeAlias
    )
}

fn symbol_compare(
    left: &GraphSymbol,
    right: &GraphSymbol,
    ranks: &BTreeMap<SymbolId, usize>,
) -> std::cmp::Ordering {
    project_map_symbol_kind_priority(left.parsed().kind())
        .cmp(&project_map_symbol_kind_priority(right.parsed().kind()))
        .then_with(|| {
            project_map_visibility_priority(left.parsed().visibility()).cmp(
                &project_map_visibility_priority(right.parsed().visibility()),
            )
        })
        .then_with(|| {
            right
                .parsed()
                .roles()
                .contains(SymbolRole::Entrypoint)
                .cmp(&left.parsed().roles().contains(SymbolRole::Entrypoint))
        })
        .then_with(|| {
            right
                .parsed()
                .roles()
                .contains(SymbolRole::Test)
                .cmp(&left.parsed().roles().contains(SymbolRole::Test))
        })
        .then_with(|| {
            ranks
                .get(&left.id())
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&ranks.get(&right.id()).copied().unwrap_or(usize::MAX))
        })
        .then_with(|| left.id().cmp(&right.id()))
}

const fn module_priority(kind: ModuleKind) -> u8 {
    match kind {
        ModuleKind::ManifestBoundary => 0,
        ModuleKind::PathBoundary => 1,
        ModuleKind::GraphCommunity => 2,
    }
}

const fn relation_priority(kind: SyntaxRelationKind) -> u8 {
    match kind {
        SyntaxRelationKind::Implements => 0,
        SyntaxRelationKind::Extends => 1,
        SyntaxRelationKind::Imports => 2,
        SyntaxRelationKind::Exports => 3,
        SyntaxRelationKind::Builds => 4,
        SyntaxRelationKind::Configures => 5,
        SyntaxRelationKind::Contains => 6,
        SyntaxRelationKind::Defines => 7,
        SyntaxRelationKind::Calls => 8,
        SyntaxRelationKind::Tests => 9,
        SyntaxRelationKind::Reads => 10,
        SyntaxRelationKind::Writes => 11,
        SyntaxRelationKind::Documents => 12,
    }
}

fn module_node_id(module_id: ModuleId) -> ProjectMapAtlasNodeId {
    hashed_node_id(b"module", module_id.as_bytes())
}
fn file_node_id(evidence_id: ModuleCardEvidenceId) -> ProjectMapAtlasNodeId {
    hashed_node_id(b"file", evidence_id.as_bytes())
}
fn symbol_node_id(symbol_id: SymbolId) -> ProjectMapAtlasNodeId {
    hashed_node_id(b"symbol", symbol_id.as_bytes())
}
fn boundary_node_id(
    evidence_id: ModuleCardEvidenceId,
    display: &str,
    index: usize,
) -> ProjectMapAtlasNodeId {
    let mut value = Vec::with_capacity(32 + display.len() + 8);
    value.extend_from_slice(evidence_id.as_bytes());
    value.extend_from_slice(display.as_bytes());
    value.extend_from_slice(&(index as u64).to_le_bytes());
    hashed_node_id(b"boundary", &value)
}
fn hashed_node_id(namespace: &[u8], value: &[u8]) -> ProjectMapAtlasNodeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3:project-map-atlas-node:v1\0");
    hasher.update(namespace);
    hasher.update(&[0]);
    hasher.update(value);
    ProjectMapAtlasNodeId::from_bytes(*hasher.finalize().as_bytes())
}

fn module_display_name(root: Option<&ModuleRoot>) -> String {
    match root {
        Some(ModuleRoot::Repository) | None => "Repository".to_owned(),
        Some(ModuleRoot::Directory(path)) => file_name_display(path),
    }
}

fn path_display(path: &RepositoryPath) -> String {
    safe_bytes(path.as_bytes(), MAX_DETAIL_BYTES)
}
fn file_name_display(path: &RepositoryPath) -> String {
    let bytes = path.as_bytes();
    let start = bytes
        .iter()
        .rposition(|byte| matches!(*byte, b'/' | b'\\'))
        .map_or(0, |index| index + 1);
    safe_bytes(&bytes[start..], MAX_DISPLAY_BYTES)
}

fn safe_bytes(bytes: &[u8], maximum: usize) -> String {
    let text = String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect::<String>();
    safe_text(&text, maximum)
}

fn safe_text(value: &str, maximum: usize) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
            continue;
        }
        if result.len().saturating_add(character.len_utf8()) > maximum {
            break;
        }
        result.push(character);
    }
    if result.is_empty() {
        "Unbenannt".to_owned()
    } else {
        result
    }
}

fn safe_summary_text(value: &str, maximum: usize) -> Option<String> {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_control() {
            continue;
        }
        if result.len().saturating_add(character.len_utf8()) > maximum {
            break;
        }
        result.push(character);
    }
    (!result.is_empty()).then_some(result)
}

fn rank(index: usize) -> Result<u16, ProjectMapAtlasError> {
    u16::try_from(index.saturating_add(1)).map_err(|_| ProjectMapAtlasError)
}
fn count(value: usize) -> Result<u64, ProjectMapAtlasError> {
    u64::try_from(value).map_err(|_| ProjectMapAtlasError)
}
fn rerank_nodes(nodes: &mut [ProjectMapAtlasNode]) -> Result<(), ProjectMapAtlasError> {
    for (index, node) in nodes.iter_mut().enumerate() {
        node.rank = rank(index)?;
    }
    Ok(())
}

fn encode_cursor(
    run_id: IndexRunId,
    selection: ProjectMapEntitySelection,
    view: ProjectMapInventoryView,
    offset: usize,
) -> Result<ProjectMapInventoryCursor, ProjectMapAtlasError> {
    let offset = u64::try_from(offset).map_err(|_| ProjectMapAtlasError)?;
    let digest = cursor_digest(run_id, selection, view, offset);
    let mut value = String::with_capacity(80);
    for byte in offset.to_le_bytes().into_iter().chain(digest) {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").map_err(|_| ProjectMapAtlasError)?;
    }
    ProjectMapInventoryCursor::try_from_string(value)
}

fn decode_cursor(
    run_id: IndexRunId,
    selection: ProjectMapEntitySelection,
    view: ProjectMapInventoryView,
    cursor: &ProjectMapInventoryCursor,
) -> Option<usize> {
    if cursor.as_str().len() != 80 {
        return None;
    }
    let bytes = (0..40)
        .map(|index| u8::from_str_radix(&cursor.as_str()[index * 2..index * 2 + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    let offset = u64::from_le_bytes(bytes[..8].try_into().ok()?);
    let digest = cursor_digest(run_id, selection, view, offset);
    (bytes[8..] == digest)
        .then(|| usize::try_from(offset).ok())
        .flatten()
}

fn cursor_digest(
    run_id: IndexRunId,
    selection: ProjectMapEntitySelection,
    view: ProjectMapInventoryView,
    offset: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3:project-map-inventory-cursor:v1\0");
    hasher.update(run_id.as_bytes());
    match selection {
        ProjectMapEntitySelection::Module { module_id } => {
            hasher.update(&[0]);
            hasher.update(module_id.as_bytes());
        }
        ProjectMapEntitySelection::File {
            module_id,
            ordinal,
            evidence_id,
        } => {
            hasher.update(&[1]);
            hasher.update(module_id.as_bytes());
            hasher.update(&ordinal.get().to_le_bytes());
            hasher.update(evidence_id.as_bytes());
        }
        ProjectMapEntitySelection::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } => {
            hasher.update(&[2]);
            hasher.update(module_id.as_bytes());
            hasher.update(symbol_id.as_bytes());
            hasher.update(evidence_id.as_bytes());
        }
    }
    hasher.update(&[match view {
        ProjectMapInventoryView::Files => 0,
        ProjectMapInventoryView::Symbols => 1,
        ProjectMapInventoryView::Members => 2,
    }]);
    hasher.update(&offset.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Maps a structural symbol into its stable Atlas visual category.
#[must_use]
pub const fn project_map_node_kind_for_symbol(kind: SymbolKind) -> ProjectMapAtlasNodeKind {
    match kind {
        SymbolKind::Module | SymbolKind::Namespace => ProjectMapAtlasNodeKind::Namespace,
        SymbolKind::Function | SymbolKind::Method => ProjectMapAtlasNodeKind::Callable,
        SymbolKind::Struct
        | SymbolKind::Enum
        | SymbolKind::Trait
        | SymbolKind::Interface
        | SymbolKind::Class
        | SymbolKind::Implementation
        | SymbolKind::TypeAlias => ProjectMapAtlasNodeKind::Type,
        SymbolKind::Constant
        | SymbolKind::Static
        | SymbolKind::Variable
        | SymbolKind::Field
        | SymbolKind::Variant
        | SymbolKind::Parameter => ProjectMapAtlasNodeKind::Member,
    }
}

/// Returns the deterministic architecture priority used before stored rank and stable ID.
#[must_use]
pub const fn project_map_symbol_kind_priority(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Interface
        | SymbolKind::Trait
        | SymbolKind::Enum => 0,
        SymbolKind::Implementation | SymbolKind::TypeAlias => 1,
        SymbolKind::Module | SymbolKind::Namespace => 2,
        SymbolKind::Function | SymbolKind::Method => 3,
        SymbolKind::Constant | SymbolKind::Static | SymbolKind::Field | SymbolKind::Variant => 4,
        SymbolKind::Variable | SymbolKind::Parameter => 5,
    }
}

/// Returns the deterministic visibility priority used by symbol ranking.
#[must_use]
pub const fn project_map_visibility_priority(visibility: SymbolVisibility) -> u8 {
    match visibility {
        SymbolVisibility::Public => 0,
        SymbolVisibility::Protected => 1,
        SymbolVisibility::Internal => 2,
        SymbolVisibility::Private => 3,
        SymbolVisibility::Unknown => 4,
        SymbolVisibility::Local => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_ordinals_and_inventory_cursors_are_bounded() {
        assert!(ProjectMapFileOrdinal::new(0).is_err());
        assert!(ProjectMapFileOrdinal::new(1).is_ok());
        assert!(ProjectMapInventoryCursor::try_from_string("00ff".to_owned()).is_ok());
        assert!(ProjectMapInventoryCursor::try_from_string("not-opaque".to_owned()).is_err());
    }

    #[test]
    fn node_kind_mapping_covers_all_structural_symbols() {
        assert_eq!(
            project_map_node_kind_for_symbol(SymbolKind::Class),
            ProjectMapAtlasNodeKind::Type
        );
        assert_eq!(
            project_map_node_kind_for_symbol(SymbolKind::Function),
            ProjectMapAtlasNodeKind::Callable
        );
        assert_eq!(
            project_map_node_kind_for_symbol(SymbolKind::Field),
            ProjectMapAtlasNodeKind::Member
        );
    }
}
