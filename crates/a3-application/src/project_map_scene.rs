use crate::{JobContext, KnowledgeStoreFailure, ModuleDependencyRelation};
use a3_domain::{
    IndexRunId, ModuleCardEvidenceId, ModuleCardId, ModuleId, ModuleKind, Progress,
    ProjectIdentity, SnapshotId,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Fixed limits for the first deterministic architecture-atlas policy.
pub const PROJECT_MAP_SCENE_OVERVIEW_MODULE_LIMIT: usize = 64;
/// Center plus at most thirty-one evidence-ranked direct neighbors.
pub const PROJECT_MAP_SCENE_FOCUS_MODULE_LIMIT: usize = 32;
/// Maximum visible relation groups in either scene mode.
pub const PROJECT_MAP_SCENE_RELATION_LIMIT: usize = 128;

/// Versioned deterministic selection and truncation policy for atlas scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenePolicyVersion {
    /// Manifest-first overview and evidence-ranked direct focus neighborhood.
    V1,
}

/// Request for either the bounded project overview or one focused neighborhood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapSceneQuery {
    focus_module_id: Option<ModuleId>,
}

impl ProjectMapSceneQuery {
    /// Creates a scene request without accepting paths, cursors, or caller-controlled limits.
    #[must_use]
    pub const fn new(focus_module_id: Option<ModuleId>) -> Self {
        Self { focus_module_id }
    }

    /// Returns the optional current primary module selected for focus mode.
    #[must_use]
    pub const fn focus_module_id(self) -> Option<ModuleId> {
        self.focus_module_id
    }
}

/// Current Module Card lifecycle summarized for a map region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapMappingStatus {
    /// The latest Card is current for the bound publication.
    Current,
    /// Direct evidence invalidated the latest Card.
    Stale,
    /// A direct dependency changed and the Card needs review.
    NeedsReview,
    /// No verified Card has been published for this current primary module.
    Unmapped,
}

/// Primary deterministic boundary kind admitted to an atlas region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSceneModuleKind {
    /// Package or workspace manifest boundary.
    ManifestBoundary,
    /// Deterministic structural path boundary.
    PathBoundary,
}

/// Optional Card anchors carried by a module region for progressive Inspector reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapCardBinding {
    card_id: ModuleCardId,
    source_index_run_id: IndexRunId,
    source_snapshot_id: SnapshotId,
}

impl ProjectMapCardBinding {
    /// Creates a binding already validated against the scene publication.
    #[must_use]
    pub const fn new(
        card_id: ModuleCardId,
        source_index_run_id: IndexRunId,
        source_snapshot_id: SnapshotId,
    ) -> Self {
        Self {
            card_id,
            source_index_run_id,
            source_snapshot_id,
        }
    }

    /// Returns the visible latest Card identity.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the publication run that produced the Card.
    #[must_use]
    pub const fn source_index_run_id(self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the immutable snapshot that produced the Card.
    #[must_use]
    pub const fn source_snapshot_id(self) -> SnapshotId {
        self.source_snapshot_id
    }
}

/// One deterministic rectangular region in the architecture atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapSceneModule {
    module_id: ModuleId,
    parent_module_id: Option<ModuleId>,
    kind: ProjectMapSceneModuleKind,
    display_name: String,
    rank: u16,
    manifest_count: u64,
    file_count: u64,
    symbol_count: u64,
    central_symbol_count: u64,
    entrypoint_count: u64,
    test_count: u64,
    mapping_status: ProjectMapMappingStatus,
    card_coverage_basis_points: Option<u16>,
    card_binding: Option<ProjectMapCardBinding>,
    representative_evidence_id: Option<ModuleCardEvidenceId>,
}

impl ProjectMapSceneModule {
    /// Builds one bounded safe module region and rejects contradictory Card state.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        module_id: ModuleId,
        parent_module_id: Option<ModuleId>,
        kind: ModuleKind,
        display_name: String,
        rank: u16,
        manifest_count: u64,
        file_count: u64,
        symbol_count: u64,
        central_symbol_count: u64,
        entrypoint_count: u64,
        test_count: u64,
        mapping_status: ProjectMapMappingStatus,
        card_coverage_basis_points: Option<u16>,
        card_binding: Option<ProjectMapCardBinding>,
        representative_evidence_id: Option<ModuleCardEvidenceId>,
    ) -> Result<Self, ProjectMapSceneError> {
        let kind = match kind {
            ModuleKind::ManifestBoundary => ProjectMapSceneModuleKind::ManifestBoundary,
            ModuleKind::PathBoundary => ProjectMapSceneModuleKind::PathBoundary,
            ModuleKind::GraphCommunity => return Err(ProjectMapSceneError),
        };
        if display_name.is_empty()
            || display_name.chars().count() > 256
            || display_name.chars().any(char::is_control)
            || parent_module_id == Some(module_id)
            || file_count > symbol_count
            || central_symbol_count > symbol_count
            || entrypoint_count > symbol_count
            || test_count > symbol_count
            || card_coverage_basis_points.is_some_and(|value| value > 10_000)
            || (mapping_status == ProjectMapMappingStatus::Unmapped) != card_binding.is_none()
            || card_coverage_basis_points.is_some() != card_binding.is_some()
        {
            return Err(ProjectMapSceneError);
        }
        Ok(Self {
            module_id,
            parent_module_id,
            kind,
            display_name,
            rank,
            manifest_count,
            file_count,
            symbol_count,
            central_symbol_count,
            entrypoint_count,
            test_count,
            mapping_status,
            card_coverage_basis_points,
            card_binding,
            representative_evidence_id,
        })
    }

    /// Returns the stable primary module identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the nearest primary ancestor when it is visible in the project hierarchy.
    #[must_use]
    pub const fn parent_module_id(&self) -> Option<ModuleId> {
        self.parent_module_id
    }

    /// Returns the deterministic boundary kind.
    #[must_use]
    pub const fn kind(&self) -> ProjectMapSceneModuleKind {
        self.kind
    }

    /// Returns bounded display-only text.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the one-based deterministic scene rank.
    #[must_use]
    pub const fn rank(&self) -> u16 {
        self.rank
    }

    /// Returns exact manifest count at the boundary.
    #[must_use]
    pub const fn manifest_count(&self) -> u64 {
        self.manifest_count
    }

    /// Returns exact current member-file count.
    #[must_use]
    pub const fn file_count(&self) -> u64 {
        self.file_count
    }

    /// Returns exact current primary-symbol count.
    #[must_use]
    pub const fn symbol_count(&self) -> u64 {
        self.symbol_count
    }

    /// Returns the bounded current central-symbol count.
    #[must_use]
    pub const fn central_symbol_count(&self) -> u64 {
        self.central_symbol_count
    }

    /// Returns the bounded current entry-point count.
    #[must_use]
    pub const fn entrypoint_count(&self) -> u64 {
        self.entrypoint_count
    }

    /// Returns the bounded current test count.
    #[must_use]
    pub const fn test_count(&self) -> u64 {
        self.test_count
    }

    /// Returns the independently encoded mapping lifecycle.
    #[must_use]
    pub const fn mapping_status(&self) -> ProjectMapMappingStatus {
        self.mapping_status
    }

    /// Returns verified Card field coverage in basis points.
    #[must_use]
    pub const fn card_coverage_basis_points(&self) -> Option<u16> {
        self.card_coverage_basis_points
    }

    /// Returns current Card anchors for progressive Inspector loading.
    #[must_use]
    pub const fn card_binding(&self) -> Option<ProjectMapCardBinding> {
        self.card_binding
    }

    /// Returns a safe current file Evidence hook when the module has structural symbols.
    #[must_use]
    pub const fn representative_evidence_id(&self) -> Option<ModuleCardEvidenceId> {
        self.representative_evidence_id
    }
}

/// One non-authoritative route between two visible regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapSceneRelation {
    source_module_id: ModuleId,
    target_module_id: ModuleId,
    relation: ModuleDependencyRelation,
    observed_evidence_count: u64,
    evidence_id: Option<ModuleCardEvidenceId>,
}

impl ProjectMapSceneRelation {
    /// Creates one grouped route with an optional exact representative Evidence hook.
    pub fn new(
        source_module_id: ModuleId,
        target_module_id: ModuleId,
        relation: ModuleDependencyRelation,
        observed_evidence_count: u64,
        evidence_id: Option<ModuleCardEvidenceId>,
    ) -> Result<Self, ProjectMapSceneError> {
        if source_module_id == target_module_id || observed_evidence_count == 0 {
            return Err(ProjectMapSceneError);
        }
        Ok(Self {
            source_module_id,
            target_module_id,
            relation,
            observed_evidence_count,
            evidence_id,
        })
    }

    /// Returns the source region.
    #[must_use]
    pub const fn source_module_id(&self) -> ModuleId {
        self.source_module_id
    }

    /// Returns the target region.
    #[must_use]
    pub const fn target_module_id(&self) -> ModuleId {
        self.target_module_id
    }

    /// Returns the grouped language-neutral relation.
    #[must_use]
    pub const fn relation(&self) -> ModuleDependencyRelation {
        self.relation
    }

    /// Returns the number of exact source edges represented by this route.
    #[must_use]
    pub const fn observed_evidence_count(&self) -> u64 {
        self.observed_evidence_count
    }

    /// Returns an exact representative graph Evidence hook when available.
    #[must_use]
    pub const fn evidence_id(&self) -> Option<ModuleCardEvidenceId> {
        self.evidence_id
    }
}

/// Complete bounded scene bound to one atomic publication and policy version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMapScene {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    policy_version: ScenePolicyVersion,
    focus_module_id: Option<ModuleId>,
    primary_module_count: u64,
    modules: Vec<ProjectMapSceneModule>,
    observed_relation_group_count: u64,
    relations: Vec<ProjectMapSceneRelation>,
    inspected_edge_count: u64,
    unmapped_edge_count: u64,
    source_edges_truncated: bool,
}

impl ProjectMapScene {
    /// Validates all fixed bounds, counts, identities, and visible relation endpoints.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        policy_version: ScenePolicyVersion,
        focus_module_id: Option<ModuleId>,
        primary_module_count: u64,
        modules: Vec<ProjectMapSceneModule>,
        observed_relation_group_count: u64,
        relations: Vec<ProjectMapSceneRelation>,
        inspected_edge_count: u64,
        unmapped_edge_count: u64,
        source_edges_truncated: bool,
    ) -> Result<Self, ProjectMapSceneError> {
        let module_limit = if focus_module_id.is_some() {
            PROJECT_MAP_SCENE_FOCUS_MODULE_LIMIT
        } else {
            PROJECT_MAP_SCENE_OVERVIEW_MODULE_LIMIT
        };
        let module_ids = modules
            .iter()
            .map(ProjectMapSceneModule::module_id)
            .collect::<BTreeSet<_>>();
        let ranks = modules
            .iter()
            .map(ProjectMapSceneModule::rank)
            .collect::<BTreeSet<_>>();
        let relation_keys = relations
            .iter()
            .map(|route| {
                (
                    route.source_module_id(),
                    route.target_module_id(),
                    route.relation(),
                )
            })
            .collect::<BTreeSet<_>>();
        if modules.len() > module_limit
            || primary_module_count < modules.len() as u64
            || module_ids.len() != modules.len()
            || ranks.len() != modules.len()
            || modules
                .iter()
                .enumerate()
                .any(|(index, module)| usize::from(module.rank()) != index + 1)
            || focus_module_id.is_some_and(|focus| !module_ids.contains(&focus))
            || relations.len() > PROJECT_MAP_SCENE_RELATION_LIMIT
            || observed_relation_group_count < relations.len() as u64
            || relation_keys.len() != relations.len()
            || relations.iter().any(|route| {
                !module_ids.contains(&route.source_module_id())
                    || !module_ids.contains(&route.target_module_id())
            })
            || unmapped_edge_count > inspected_edge_count
        {
            return Err(ProjectMapSceneError);
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            policy_version,
            focus_module_id,
            primary_module_count,
            modules,
            observed_relation_group_count,
            relations,
            inspected_edge_count,
            unmapped_edge_count,
            source_edges_truncated,
        })
    }

    /// Returns the current publication run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable publication snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the deterministic scene-policy version.
    #[must_use]
    pub const fn policy_version(&self) -> ScenePolicyVersion {
        self.policy_version
    }

    /// Returns the focused center, or None for the project overview.
    #[must_use]
    pub const fn focus_module_id(&self) -> Option<ModuleId> {
        self.focus_module_id
    }

    /// Returns all current primary modules before scene selection.
    #[must_use]
    pub const fn primary_module_count(&self) -> u64 {
        self.primary_module_count
    }

    /// Returns the deterministic visible module regions.
    #[must_use]
    pub fn modules(&self) -> &[ProjectMapSceneModule] {
        &self.modules
    }

    /// Returns relation groups observed before the fixed route cap.
    #[must_use]
    pub const fn observed_relation_group_count(&self) -> u64 {
        self.observed_relation_group_count
    }

    /// Returns at most 128 routes between visible modules.
    #[must_use]
    pub fn relations(&self) -> &[ProjectMapSceneRelation] {
        &self.relations
    }

    /// Returns exact graph edges inspected for completeness accounting.
    #[must_use]
    pub const fn inspected_edge_count(&self) -> u64 {
        self.inspected_edge_count
    }

    /// Returns inspected edges that could not map uniquely to two primary modules.
    #[must_use]
    pub const fn unmapped_edge_count(&self) -> u64 {
        self.unmapped_edge_count
    }

    /// Returns whether the fixed source-edge scan omitted additional edges.
    #[must_use]
    pub const fn source_edges_truncated(&self) -> bool {
        self.source_edges_truncated
    }

    /// Returns whether lower-ranked primary modules were omitted.
    #[must_use]
    pub fn modules_truncated(&self) -> bool {
        self.primary_module_count > self.modules.len() as u64
    }

    /// Returns whether lower-ranked relation groups were omitted.
    #[must_use]
    pub fn relations_truncated(&self) -> bool {
        self.observed_relation_group_count > self.relations.len() as u64
    }
}

/// A constructed or stored scene contradicted its fixed policy bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectMapSceneError;

impl fmt::Display for ProjectMapSceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project map scene is invalid")
    }
}

impl Error for ProjectMapSceneError {}

/// Availability state of the latest bounded map scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectMapSceneLoadResult {
    /// No index crossed the atomic publication boundary.
    NoPublishedIndex,
    /// The publication predates deterministic modules.
    ProjectionUnavailable,
    /// The requested focus module is absent or supplementary.
    FocusUnavailable,
    /// One current bounded scene is available.
    Scene(ProjectMapScene),
}

/// Cooperative cancellation and deterministic progress for scene reads.
pub trait ProjectMapSceneControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning UI generation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports only bounded start and terminal progress.
    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapSceneControlError>;
}

impl ProjectMapSceneControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ProjectMapSceneControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ProjectMapSceneControlError::Unavailable)
    }
}

/// Scene progress could not reach its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSceneControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ProjectMapSceneControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project map scene progress is unavailable")
    }
}

impl Error for ProjectMapSceneControlError {}

/// Owned future returned by the narrow scene store.
pub type ProjectMapSceneFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ProjectMapSceneLoadResult, ProjectMapSceneFailure>> + Send + 'a>,
>;

/// Read-only port for a policy-owned project-map scene.
pub trait ProjectMapSceneStore: fmt::Debug + Send + Sync {
    /// Loads the latest scene without caller-controlled traversal or rendering limits.
    fn load_project_map_scene<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ProjectMapSceneQuery,
        control: &'a dyn ProjectMapSceneControl,
    ) -> ProjectMapSceneFuture<'a>;
}

/// Stable content-free failure classes for scene reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMapSceneFailure {
    /// Local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the scene contract.
    InvalidStoredProjection,
    /// The owning generation cancelled the read.
    Cancelled,
    /// The bounded adapter query exceeded its deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ProjectMapSceneFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project map scene storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored project map scene is invalid")
            }
            Self::Cancelled => formatter.write_str("project map scene read was cancelled"),
            Self::TimedOut => formatter.write_str("project map scene read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("project map scene progress is unavailable")
            }
        }
    }
}

impl Error for ProjectMapSceneFailure {
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

/// Application use case retaining cancellation and scene bounds outside persistence.
#[derive(Debug)]
pub struct GetProjectMapScene {
    store: Arc<dyn ProjectMapSceneStore>,
}

impl GetProjectMapScene {
    /// Wires the narrow deterministic scene capability.
    #[must_use]
    pub fn new(store: Arc<dyn ProjectMapSceneStore>) -> Self {
        Self { store }
    }

    /// Reads one current overview or focused scene.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ProjectMapSceneQuery,
        control: &dyn ProjectMapSceneControl,
    ) -> Result<ProjectMapSceneLoadResult, ProjectMapSceneFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ProjectMapSceneFailure::Cancelled);
        }
        let result = self
            .store
            .load_project_map_scene(project, query, control)
            .await?;
        if let ProjectMapSceneLoadResult::Scene(scene) = &result
            && scene.focus_module_id() != query.focus_module_id()
        {
            return Err(ProjectMapSceneFailure::InvalidStoredProjection);
        }
        if control.is_cancelled() {
            return Err(ProjectMapSceneFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

fn report(
    control: &dyn ProjectMapSceneControl,
    completed: u64,
) -> Result<(), ProjectMapSceneFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ProjectMapSceneFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ProjectMapSceneFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_rejects_routes_outside_visible_modules_and_duplicate_ranks()
    -> Result<(), Box<dyn Error>> {
        let module = ProjectMapSceneModule::new(
            ModuleId::from_bytes([1; 32]),
            None,
            ModuleKind::PathBoundary,
            "src".to_owned(),
            1,
            0,
            1,
            1,
            1,
            0,
            0,
            ProjectMapMappingStatus::Unmapped,
            None,
            None,
            None,
        )?;
        let route = ProjectMapSceneRelation::new(
            module.module_id(),
            ModuleId::from_bytes([2; 32]),
            ModuleDependencyRelation::Imports,
            1,
            None,
        )?;
        assert!(
            ProjectMapScene::new(
                IndexRunId::from_bytes([3; 32]),
                SnapshotId::from_bytes([4; 32]),
                ScenePolicyVersion::V1,
                None,
                1,
                vec![module],
                1,
                vec![route],
                1,
                0,
                false,
            )
            .is_err()
        );
        Ok(())
    }
}
