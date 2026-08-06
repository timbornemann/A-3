use crate::{
    GraphEndpoint, IndexRunId, MapperProfileVersion, ModuleCardId, ModuleCardStatus, ModuleId,
    PublishedIndex, RepositoryPath, SnapshotId, SymbolId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const MAX_INVALIDATION_CANDIDATES: usize = 250_000;

/// Stable reason why a published Module Card can no longer be delivered as current.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InvalidationReason {
    /// At least one direct EvidenceRef changed or disappeared.
    EvidenceChanged,
    /// The module no longer exists in the current deterministic projection.
    ModuleRemoved,
    /// A language adapter revision changed since the card was verified.
    ParserVersionChanged,
    /// The card was produced by an incompatible mapper profile.
    MapperVersionChanged,
    /// A directly depended-on module became stale.
    DirectDependencyChanged,
}

/// Queue priority derived from the bounded invalidation radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RemapPriority {
    /// The module's own card or evidence became stale.
    Direct,
    /// A direct dependency changed and requires conservative review.
    Dependent,
}

/// Freshness inputs reconstructed by a persistence adapter for one latest visible card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardInvalidationCandidate {
    source_index_run_id: IndexRunId,
    source_snapshot_id: SnapshotId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    mapper_profile_version: MapperProfileVersion,
    parser_versions_compatible: bool,
    evidence_is_current: bool,
}

impl ModuleCardInvalidationCandidate {
    /// Creates one adapter-decoded candidate without deciding its lifecycle state.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source_index_run_id: IndexRunId,
        source_snapshot_id: SnapshotId,
        card_id: ModuleCardId,
        module_id: ModuleId,
        mapper_profile_version: MapperProfileVersion,
        parser_versions_compatible: bool,
        evidence_is_current: bool,
    ) -> Self {
        Self {
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            mapper_profile_version,
            parser_versions_compatible,
            evidence_is_current,
        }
    }

    /// Returns the index run that originally verified the card.
    #[must_use]
    pub const fn source_index_run_id(self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the snapshot that originally verified the card.
    #[must_use]
    pub const fn source_snapshot_id(self) -> SnapshotId {
        self.source_snapshot_id
    }

    /// Returns the stable card identity.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the module described by the card.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }
}

/// One lifecycle transition to persist before the new index becomes visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardInvalidation {
    source_index_run_id: IndexRunId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    status: ModuleCardStatus,
    reason: InvalidationReason,
}

impl ModuleCardInvalidation {
    /// Returns the historical source run containing the card.
    #[must_use]
    pub const fn source_index_run_id(self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the affected card.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the affected module.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }

    /// Returns `Stale` for direct changes or `NeedsReview` for one-hop dependents.
    #[must_use]
    pub const fn status(self) -> ModuleCardStatus {
        self.status
    }

    /// Returns the auditable transition reason.
    #[must_use]
    pub const fn reason(self) -> InvalidationReason {
        self.reason
    }
}

/// One bounded, deterministically ordered remap request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemapRequest {
    source_index_run_id: IndexRunId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    target_index_run_id: IndexRunId,
    target_snapshot_id: SnapshotId,
    priority: RemapPriority,
    reason: InvalidationReason,
}

impl RemapRequest {
    /// Reconstructs one persisted pending request and rejects priority/reason mismatches.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persisted(
        source_index_run_id: IndexRunId,
        card_id: ModuleCardId,
        module_id: ModuleId,
        target_index_run_id: IndexRunId,
        target_snapshot_id: SnapshotId,
        priority: RemapPriority,
        reason: InvalidationReason,
    ) -> Result<Self, RemapRequestError> {
        let valid = match priority {
            RemapPriority::Direct => matches!(
                reason,
                InvalidationReason::EvidenceChanged
                    | InvalidationReason::ParserVersionChanged
                    | InvalidationReason::MapperVersionChanged
            ),
            RemapPriority::Dependent => reason == InvalidationReason::DirectDependencyChanged,
        };
        if !valid {
            return Err(RemapRequestError);
        }
        Ok(Self {
            source_index_run_id,
            card_id,
            module_id,
            target_index_run_id,
            target_snapshot_id,
            priority,
            reason,
        })
    }

    /// Returns the historical card run being replaced or reviewed.
    #[must_use]
    pub const fn source_index_run_id(self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the card that caused this request.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the module to remap.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }

    /// Returns the new deterministic index run the remap must use.
    #[must_use]
    pub const fn target_index_run_id(self) -> IndexRunId {
        self.target_index_run_id
    }

    /// Returns the new immutable snapshot the remap must use.
    #[must_use]
    pub const fn target_snapshot_id(self) -> SnapshotId {
        self.target_snapshot_id
    }

    /// Returns direct-before-dependent priority.
    #[must_use]
    pub const fn priority(self) -> RemapPriority {
        self.priority
    }

    /// Returns the auditable invalidation reason.
    #[must_use]
    pub const fn reason(self) -> InvalidationReason {
        self.reason
    }
}

/// Persisted remap priority and reason did not describe a legal queue state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemapRequestError;

impl fmt::Display for RemapRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("remap request priority and reason are incompatible")
    }
}

impl Error for RemapRequestError {}

/// Complete direct-plus-one-hop invalidation decision for one new published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexInvalidationPlan {
    target_index_run_id: IndexRunId,
    target_snapshot_id: SnapshotId,
    invalidations: Vec<ModuleCardInvalidation>,
    remaps: Vec<RemapRequest>,
}

impl IndexInvalidationPlan {
    /// Computes the smallest documented invalidation radius from adapter-verified freshness flags.
    pub fn compile(
        published: &PublishedIndex,
        current_mapper_profile: MapperProfileVersion,
        mut candidates: Vec<ModuleCardInvalidationCandidate>,
    ) -> Result<Self, InvalidationPlanError> {
        if candidates.len() > MAX_INVALIDATION_CANDIDATES {
            return Err(InvalidationPlanError::TooManyCandidates {
                count: candidates.len(),
            });
        }
        candidates.sort_by_key(|candidate| (candidate.module_id, candidate.card_id));
        if candidates
            .windows(2)
            .any(|pair| pair[0].module_id == pair[1].module_id)
        {
            return Err(InvalidationPlanError::DuplicateModuleCandidate);
        }

        let current_modules = published
            .publication()
            .modules()
            .modules()
            .iter()
            .map(|module| module.id())
            .collect::<BTreeSet<_>>();
        let mut direct =
            BTreeMap::<ModuleId, (ModuleCardInvalidationCandidate, InvalidationReason)>::new();
        for candidate in &candidates {
            let reason = if !current_modules.contains(&candidate.module_id) {
                Some(InvalidationReason::ModuleRemoved)
            } else if candidate.mapper_profile_version != current_mapper_profile {
                Some(InvalidationReason::MapperVersionChanged)
            } else if !candidate.parser_versions_compatible {
                Some(InvalidationReason::ParserVersionChanged)
            } else if !candidate.evidence_is_current {
                Some(InvalidationReason::EvidenceChanged)
            } else {
                None
            };
            if let Some(reason) = reason {
                direct.insert(candidate.module_id, (*candidate, reason));
            }
        }

        let direct_modules = direct.keys().copied().collect::<BTreeSet<_>>();
        let dependent_modules = direct_dependents(published, &direct_modules);
        let candidates_by_module = candidates
            .iter()
            .map(|candidate| (candidate.module_id, *candidate))
            .collect::<BTreeMap<_, _>>();
        let mut invalidations = Vec::new();
        let mut remaps = Vec::new();
        for (module_id, (candidate, reason)) in direct {
            invalidations.push(ModuleCardInvalidation {
                source_index_run_id: candidate.source_index_run_id,
                card_id: candidate.card_id,
                module_id,
                status: ModuleCardStatus::Stale,
                reason,
            });
            if current_modules.contains(&module_id) {
                remaps.push(RemapRequest {
                    source_index_run_id: candidate.source_index_run_id,
                    card_id: candidate.card_id,
                    module_id,
                    target_index_run_id: published.run().id(),
                    target_snapshot_id: published.run().snapshot_id(),
                    priority: RemapPriority::Direct,
                    reason,
                });
            }
        }
        for module_id in dependent_modules.difference(&direct_modules) {
            let Some(candidate) = candidates_by_module.get(module_id).copied() else {
                continue;
            };
            invalidations.push(ModuleCardInvalidation {
                source_index_run_id: candidate.source_index_run_id,
                card_id: candidate.card_id,
                module_id: *module_id,
                status: ModuleCardStatus::NeedsReview,
                reason: InvalidationReason::DirectDependencyChanged,
            });
            remaps.push(RemapRequest {
                source_index_run_id: candidate.source_index_run_id,
                card_id: candidate.card_id,
                module_id: *module_id,
                target_index_run_id: published.run().id(),
                target_snapshot_id: published.run().snapshot_id(),
                priority: RemapPriority::Dependent,
                reason: InvalidationReason::DirectDependencyChanged,
            });
        }
        invalidations.sort_by_key(|item| (item.status, item.module_id, item.card_id));
        remaps.sort_by_key(|item| (item.priority, item.module_id, item.card_id));
        Ok(Self {
            target_index_run_id: published.run().id(),
            target_snapshot_id: published.run().snapshot_id(),
            invalidations,
            remaps,
        })
    }

    /// Returns the target run that becomes visible with this plan.
    #[must_use]
    pub const fn target_index_run_id(&self) -> IndexRunId {
        self.target_index_run_id
    }

    /// Returns the target snapshot that becomes visible with this plan.
    #[must_use]
    pub const fn target_snapshot_id(&self) -> SnapshotId {
        self.target_snapshot_id
    }

    /// Returns stable direct and dependent lifecycle transitions.
    #[must_use]
    pub fn invalidations(&self) -> &[ModuleCardInvalidation] {
        &self.invalidations
    }

    /// Returns stable direct-before-dependent remap requests.
    #[must_use]
    pub fn remaps(&self) -> &[RemapRequest] {
        &self.remaps
    }
}

fn direct_dependents(
    published: &PublishedIndex,
    invalidated: &BTreeSet<ModuleId>,
) -> BTreeSet<ModuleId> {
    if invalidated.is_empty() {
        return BTreeSet::new();
    }
    let modules = published.publication().modules();
    let primary_by_symbol = modules
        .memberships()
        .iter()
        .filter(|membership| membership.evidence().kind().is_primary())
        .map(|membership| (membership.symbol_id(), membership.module_id()))
        .collect::<BTreeMap<_, _>>();
    let mut modules_by_path = BTreeMap::<RepositoryPath, BTreeSet<ModuleId>>::new();
    for symbol in published.publication().graph().symbols() {
        if let Some(module_id) = primary_by_symbol.get(&symbol.id()).copied() {
            modules_by_path
                .entry(symbol.revision().path().clone())
                .or_default()
                .insert(module_id);
        }
    }
    let mut dependents = BTreeSet::new();
    for edge in published.publication().graph().edges() {
        let source_modules = endpoint_modules(edge.source(), &primary_by_symbol, &modules_by_path);
        let target_modules = endpoint_modules(edge.target(), &primary_by_symbol, &modules_by_path);
        if target_modules
            .iter()
            .any(|module| invalidated.contains(module))
        {
            dependents.extend(
                source_modules
                    .into_iter()
                    .filter(|module| !invalidated.contains(module)),
            );
        }
    }
    dependents
}

fn endpoint_modules(
    endpoint: &GraphEndpoint,
    primary_by_symbol: &BTreeMap<SymbolId, ModuleId>,
    modules_by_path: &BTreeMap<RepositoryPath, BTreeSet<ModuleId>>,
) -> BTreeSet<ModuleId> {
    match endpoint {
        GraphEndpoint::File(path) => modules_by_path.get(path).cloned().unwrap_or_default(),
        GraphEndpoint::Symbol(symbol_id) => primary_by_symbol
            .get(symbol_id)
            .copied()
            .into_iter()
            .collect(),
    }
}

/// Invalid persistence projection or an exceeded deterministic planning bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidationPlanError {
    /// More candidates were supplied than one index can contain modules.
    TooManyCandidates {
        /// Supplied candidate count.
        count: usize,
    },
    /// Persistence supplied more than one latest card for a module.
    DuplicateModuleCandidate,
}

impl fmt::Display for InvalidationPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyCandidates { count } => {
                write!(
                    formatter,
                    "invalidation candidate count {count} exceeds the fixed limit"
                )
            }
            Self::DuplicateModuleCandidate => {
                formatter.write_str("invalidation candidates contain a duplicate module")
            }
        }
    }
}

impl Error for InvalidationPlanError {}
