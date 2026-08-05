use super::{EvidenceRef, FileRevision, IndexLanguage, SnapshotId, SymbolId};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// Stable deterministic identity of one path boundary or graph community.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId([u8; 32]);

impl ModuleId {
    /// Constructs an ID from the versioned module-former derivation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation used by persistence.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ModuleId({self})")
    }
}

/// Durable revision of deterministic module-formation semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModulePolicyVersion(u32);

impl ModulePolicyVersion {
    /// Returns the initial manifest, path, and graph-community policy.
    #[must_use]
    pub const fn v1() -> Self {
        Self(1)
    }

    /// Creates a non-zero policy revision reconstructed from persistence.
    pub fn new(value: u32) -> Result<Self, ModulePolicyVersionError> {
        if value == 0 {
            return Err(ModulePolicyVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the stable integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Module policy version zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModulePolicyVersionError;

impl fmt::Display for ModulePolicyVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module policy version must be positive")
    }
}

impl Error for ModulePolicyVersionError {}

/// Canonical repository-relative root of a primary module.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleRoot {
    /// The repository root, which has no non-empty `RepositoryPath` representation.
    Repository,
    /// A normalized repository-relative directory.
    Directory(super::RepositoryPath),
}

/// Deterministic signal that created one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleKind {
    /// One or more package manifests establish the primary boundary.
    ManifestBoundary,
    /// The nearest top-level path establishes the primary boundary.
    PathBoundary,
    /// Directed graph evidence establishes an additional community.
    GraphCommunity,
}

impl ModuleKind {
    /// Returns whether symbols use this module as a primary membership.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::ManifestBoundary | Self::PathBoundary)
    }
}

/// Ranked, bounded symbol list whose loss of tail entries remains explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSymbolSet {
    symbols: Vec<SymbolId>,
    truncated: bool,
}

impl ModuleSymbolSet {
    /// Retains caller-provided rank order while rejecting duplicate symbols.
    pub fn new(symbols: Vec<SymbolId>, truncated: bool) -> Result<Self, ModuleMapError> {
        let unique = symbols.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != symbols.len() {
            return Err(ModuleMapError::DuplicateFeaturedSymbol);
        }
        if truncated && symbols.is_empty() {
            return Err(ModuleMapError::InvalidTruncation);
        }
        Ok(Self { symbols, truncated })
    }

    /// Returns an empty, complete symbol list.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            symbols: Vec::new(),
            truncated: false,
        }
    }

    /// Returns symbols in deterministic descending relevance order.
    #[must_use]
    pub fn symbols(&self) -> &[SymbolId] {
        &self.symbols
    }

    /// Returns whether bounded projection omitted lower-ranked symbols.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// One primary path module or supplementary graph community.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryModule {
    id: ModuleId,
    kind: ModuleKind,
    root: Option<ModuleRoot>,
    manifests: Vec<FileRevision>,
    central_symbols: ModuleSymbolSet,
    entrypoints: ModuleSymbolSet,
    tests: ModuleSymbolSet,
}

impl RepositoryModule {
    /// Creates a module only when its kind, root, and manifest evidence agree.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ModuleId,
        kind: ModuleKind,
        root: Option<ModuleRoot>,
        mut manifests: Vec<FileRevision>,
        central_symbols: ModuleSymbolSet,
        entrypoints: ModuleSymbolSet,
        tests: ModuleSymbolSet,
    ) -> Result<Self, ModuleMapError> {
        manifests.sort_by(compare_revisions);
        if manifests
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(ModuleMapError::DuplicateManifest);
        }
        let shape_is_valid = match kind {
            ModuleKind::ManifestBoundary => root.is_some() && !manifests.is_empty(),
            ModuleKind::PathBoundary => root.is_some() && manifests.is_empty(),
            ModuleKind::GraphCommunity => root.is_none() && manifests.is_empty(),
        };
        if !shape_is_valid {
            return Err(ModuleMapError::InvalidModuleShape);
        }
        Ok(Self {
            id,
            kind,
            root,
            manifests,
            central_symbols,
            entrypoints,
            tests,
        })
    }

    /// Returns the deterministic module identity.
    #[must_use]
    pub const fn id(&self) -> ModuleId {
        self.id
    }

    /// Returns the signal class that formed the module.
    #[must_use]
    pub const fn kind(&self) -> ModuleKind {
        self.kind
    }

    /// Returns the primary path boundary, absent for graph communities.
    #[must_use]
    pub const fn root(&self) -> Option<&ModuleRoot> {
        self.root.as_ref()
    }

    /// Returns all canonical package-manifest revisions at this boundary.
    #[must_use]
    pub fn manifests(&self) -> &[FileRevision] {
        &self.manifests
    }

    /// Returns the highest-ranked members selected for deterministic exploration.
    #[must_use]
    pub const fn central_symbols(&self) -> &ModuleSymbolSet {
        &self.central_symbols
    }

    /// Returns adapter-proven entrypoint members.
    #[must_use]
    pub const fn entrypoints(&self) -> &ModuleSymbolSet {
        &self.entrypoints
    }

    /// Returns adapter-proven test members.
    #[must_use]
    pub const fn tests(&self) -> &ModuleSymbolSet {
        &self.tests
    }
}

/// Evidence class also determines whether a membership is primary or additional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleMembershipKind {
    /// Membership follows a package-manifest boundary.
    Manifest,
    /// Membership follows a deterministic path boundary.
    Path,
    /// Membership follows a directed graph community.
    GraphCommunity,
}

impl ModuleMembershipKind {
    /// Returns whether this evidence supplies the unique primary membership.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Manifest | Self::Path)
    }
}

/// Current file and relationship evidence supporting one symbol membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleMembershipEvidence {
    kind: ModuleMembershipKind,
    member_revision: FileRevision,
    manifest_revision: Option<FileRevision>,
    relationships: Vec<EvidenceRef>,
}

impl ModuleMembershipEvidence {
    /// Creates path-containment evidence for a primary membership.
    #[must_use]
    pub const fn path(member_revision: FileRevision) -> Self {
        Self {
            kind: ModuleMembershipKind::Path,
            member_revision,
            manifest_revision: None,
            relationships: Vec::new(),
        }
    }

    /// Creates package-manifest evidence for a primary membership.
    #[must_use]
    pub const fn manifest(member_revision: FileRevision, manifest_revision: FileRevision) -> Self {
        Self {
            kind: ModuleMembershipKind::Manifest,
            member_revision,
            manifest_revision: Some(manifest_revision),
            relationships: Vec::new(),
        }
    }

    /// Creates an additional membership from non-empty canonical graph evidence.
    pub fn graph(
        member_revision: FileRevision,
        mut relationships: Vec<EvidenceRef>,
    ) -> Result<Self, ModuleMapError> {
        relationships.sort_by(compare_evidence);
        relationships.dedup();
        if relationships.is_empty() {
            return Err(ModuleMapError::MissingMembershipEvidence);
        }
        Ok(Self {
            kind: ModuleMembershipKind::GraphCommunity,
            member_revision,
            manifest_revision: None,
            relationships,
        })
    }

    /// Returns the evidence and membership class.
    #[must_use]
    pub const fn kind(&self) -> ModuleMembershipKind {
        self.kind
    }

    /// Returns the exact current revision containing the member symbol.
    #[must_use]
    pub const fn member_revision(&self) -> &FileRevision {
        &self.member_revision
    }

    /// Returns the selected package-manifest revision for manifest membership.
    #[must_use]
    pub const fn manifest_revision(&self) -> Option<&FileRevision> {
        self.manifest_revision.as_ref()
    }

    /// Returns graph-edge evidence supporting additional membership.
    #[must_use]
    pub fn relationships(&self) -> &[EvidenceRef] {
        &self.relationships
    }
}

/// One symbol's evidence-grounded membership in one deterministic module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleMembership {
    module_id: ModuleId,
    symbol_id: SymbolId,
    evidence: ModuleMembershipEvidence,
}

impl ModuleMembership {
    /// Binds one symbol and module to current typed evidence.
    #[must_use]
    pub const fn new(
        module_id: ModuleId,
        symbol_id: SymbolId,
        evidence: ModuleMembershipEvidence,
    ) -> Self {
        Self {
            module_id,
            symbol_id,
            evidence,
        }
    }

    /// Returns the owning module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the member symbol.
    #[must_use]
    pub const fn symbol_id(&self) -> SymbolId {
        self.symbol_id
    }

    /// Returns the primary or supplementary membership evidence.
    #[must_use]
    pub const fn evidence(&self) -> &ModuleMembershipEvidence {
        &self.evidence
    }
}

/// Deterministic level-zero repository summary usable without an LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCard {
    snapshot_id: SnapshotId,
    policy_version: ModulePolicyVersion,
    packages: Vec<ModuleId>,
    languages: Vec<IndexLanguage>,
    entrypoints: ModuleSymbolSet,
    file_count: u32,
    symbol_count: u32,
}

impl RepositoryCard {
    /// Creates a canonical package/language/entrypoint overview.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        snapshot_id: SnapshotId,
        policy_version: ModulePolicyVersion,
        mut packages: Vec<ModuleId>,
        mut languages: Vec<IndexLanguage>,
        entrypoints: ModuleSymbolSet,
        file_count: u32,
        symbol_count: u32,
    ) -> Result<Self, ModuleMapError> {
        packages.sort();
        if packages.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ModuleMapError::DuplicateCardPackage);
        }
        languages.sort();
        languages.dedup();
        Ok(Self {
            snapshot_id,
            policy_version,
            packages,
            languages,
            entrypoints,
            file_count,
            symbol_count,
        })
    }

    /// Returns the immutable repository snapshot summarized by this card.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the module-formation semantics used by the card.
    #[must_use]
    pub const fn policy_version(&self) -> ModulePolicyVersion {
        self.policy_version
    }

    /// Returns primary module identities representing repository packages/path areas.
    #[must_use]
    pub fn packages(&self) -> &[ModuleId] {
        &self.packages
    }

    /// Returns deterministically observed language adapter families.
    #[must_use]
    pub fn languages(&self) -> &[IndexLanguage] {
        &self.languages
    }

    /// Returns repository entrypoints in deterministic rank order.
    #[must_use]
    pub const fn entrypoints(&self) -> &ModuleSymbolSet {
        &self.entrypoints
    }

    /// Returns the number of current repository files in the source graph.
    #[must_use]
    pub const fn file_count(&self) -> u32 {
        self.file_count
    }

    /// Returns the number of current structural symbols in the source graph.
    #[must_use]
    pub const fn symbol_count(&self) -> u32 {
        self.symbol_count
    }
}

/// Complete canonical module projection for one published graph snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleProjection {
    snapshot_id: SnapshotId,
    policy_version: ModulePolicyVersion,
    modules: Vec<RepositoryModule>,
    memberships: Vec<ModuleMembership>,
    repository_card: RepositoryCard,
}

impl ModuleProjection {
    /// Canonicalizes modules and memberships and enforces projection-local invariants.
    pub fn new(
        snapshot_id: SnapshotId,
        policy_version: ModulePolicyVersion,
        mut modules: Vec<RepositoryModule>,
        mut memberships: Vec<ModuleMembership>,
        repository_card: RepositoryCard,
    ) -> Result<Self, ModuleMapError> {
        if repository_card.snapshot_id() != snapshot_id
            || repository_card.policy_version() != policy_version
        {
            return Err(ModuleMapError::CardMismatch);
        }
        modules.sort_by_key(RepositoryModule::id);
        if modules.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(ModuleMapError::DuplicateModule);
        }
        let modules_by_id = modules
            .iter()
            .map(|module| (module.id(), module))
            .collect::<BTreeMap<_, _>>();
        let packages = modules
            .iter()
            .filter(|module| module.kind().is_primary())
            .map(RepositoryModule::id)
            .collect::<Vec<_>>();
        if packages != repository_card.packages() {
            return Err(ModuleMapError::CardMismatch);
        }

        memberships.sort_by(|left, right| {
            left.symbol_id()
                .cmp(&right.symbol_id())
                .then_with(|| left.module_id().cmp(&right.module_id()))
        });
        if memberships.windows(2).any(|pair| {
            pair[0].symbol_id() == pair[1].symbol_id() && pair[0].module_id() == pair[1].module_id()
        }) {
            return Err(ModuleMapError::DuplicateMembership);
        }
        let mut primary_counts = BTreeMap::<SymbolId, usize>::new();
        let membership_pairs = memberships
            .iter()
            .map(|membership| (membership.module_id(), membership.symbol_id()))
            .collect::<BTreeSet<_>>();
        for membership in &memberships {
            let module = modules_by_id
                .get(&membership.module_id())
                .ok_or(ModuleMapError::UnknownModule)?;
            validate_membership_shape(module, membership)?;
            if membership.evidence().kind().is_primary() {
                let count = primary_counts.entry(membership.symbol_id()).or_default();
                *count = count.saturating_add(1);
            }
        }
        if primary_counts.values().any(|count| *count != 1)
            || memberships
                .iter()
                .any(|membership| !primary_counts.contains_key(&membership.symbol_id()))
        {
            return Err(ModuleMapError::PrimaryMembershipCount);
        }
        for module in &modules {
            for symbol in module
                .central_symbols()
                .symbols()
                .iter()
                .chain(module.entrypoints().symbols())
                .chain(module.tests().symbols())
            {
                if !membership_pairs.contains(&(module.id(), *symbol)) {
                    return Err(ModuleMapError::FeaturedSymbolNotMember);
                }
            }
        }
        let card_entrypoints = repository_card
            .entrypoints()
            .symbols()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let primary_entrypoints = modules
            .iter()
            .filter(|module| module.kind().is_primary())
            .flat_map(|module| module.entrypoints().symbols().iter().copied())
            .collect::<BTreeSet<_>>();
        let card_entrypoints_valid = if repository_card.entrypoints().is_truncated() {
            card_entrypoints.len() < primary_entrypoints.len()
                && card_entrypoints.is_subset(&primary_entrypoints)
        } else {
            card_entrypoints == primary_entrypoints
        };
        if !card_entrypoints_valid {
            return Err(ModuleMapError::CardMismatch);
        }
        Ok(Self {
            snapshot_id,
            policy_version,
            modules,
            memberships,
            repository_card,
        })
    }

    /// Returns the immutable graph snapshot projected into modules.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the deterministic module policy revision.
    #[must_use]
    pub const fn policy_version(&self) -> ModulePolicyVersion {
        self.policy_version
    }

    /// Returns modules in stable identity order.
    #[must_use]
    pub fn modules(&self) -> &[RepositoryModule] {
        &self.modules
    }

    /// Returns memberships in symbol-then-module identity order.
    #[must_use]
    pub fn memberships(&self) -> &[ModuleMembership] {
        &self.memberships
    }

    /// Returns the deterministic level-zero repository summary.
    #[must_use]
    pub const fn repository_card(&self) -> &RepositoryCard {
        &self.repository_card
    }
}

fn validate_membership_shape(
    module: &RepositoryModule,
    membership: &ModuleMembership,
) -> Result<(), ModuleMapError> {
    let kind_matches = matches!(
        (module.kind(), membership.evidence().kind()),
        (ModuleKind::ManifestBoundary, ModuleMembershipKind::Manifest)
            | (ModuleKind::PathBoundary, ModuleMembershipKind::Path)
            | (
                ModuleKind::GraphCommunity,
                ModuleMembershipKind::GraphCommunity
            )
    );
    if !kind_matches {
        return Err(ModuleMapError::MembershipKindMismatch);
    }
    if let Some(manifest) = membership.evidence().manifest_revision()
        && !module.manifests().contains(manifest)
    {
        return Err(ModuleMapError::MembershipManifestMismatch);
    }
    Ok(())
}

fn compare_revisions(left: &FileRevision, right: &FileRevision) -> std::cmp::Ordering {
    left.path().cmp(right.path()).then_with(|| {
        left.content_hash()
            .as_bytes()
            .cmp(right.content_hash().as_bytes())
    })
}

fn compare_evidence(left: &EvidenceRef, right: &EvidenceRef) -> std::cmp::Ordering {
    compare_revisions(left.revision(), right.revision())
        .then_with(|| left.range().cmp(&right.range()))
}

/// Invalid deterministic module projection or evidence relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleMapError {
    /// A ranked module symbol list repeated one symbol.
    DuplicateFeaturedSymbol,
    /// Truncation was claimed without retaining any prefix.
    InvalidTruncation,
    /// A module repeated a manifest path.
    DuplicateManifest,
    /// Kind, path root, and manifest set do not form a legal module.
    InvalidModuleShape,
    /// Additional membership had no graph evidence.
    MissingMembershipEvidence,
    /// A repository card repeated a package ID.
    DuplicateCardPackage,
    /// Card snapshot, policy, packages, or entrypoints disagree with the projection.
    CardMismatch,
    /// Two modules used the same stable identity.
    DuplicateModule,
    /// Two membership rows targeted the same symbol and module.
    DuplicateMembership,
    /// A membership referred to an absent module.
    UnknownModule,
    /// Module kind and membership evidence kind disagree.
    MembershipKindMismatch,
    /// Manifest membership selected evidence outside the owning module.
    MembershipManifestMismatch,
    /// A symbol lacked exactly one primary membership.
    PrimaryMembershipCount,
    /// A central, entrypoint, or test symbol was not a module member.
    FeaturedSymbolNotMember,
}

impl fmt::Display for ModuleMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DuplicateFeaturedSymbol => "module symbol projection contains a duplicate",
            Self::InvalidTruncation => "empty module symbol projection cannot be truncated",
            Self::DuplicateManifest => "module contains a duplicate manifest path",
            Self::InvalidModuleShape => "module kind, root, and manifest evidence disagree",
            Self::MissingMembershipEvidence => "graph membership has no relationship evidence",
            Self::DuplicateCardPackage => "repository card contains a duplicate package",
            Self::CardMismatch => "repository card does not match the module projection",
            Self::DuplicateModule => "module projection contains a duplicate module ID",
            Self::DuplicateMembership => "module projection contains a duplicate membership",
            Self::UnknownModule => "module membership refers to an unknown module",
            Self::MembershipKindMismatch => "module and membership evidence kinds disagree",
            Self::MembershipManifestMismatch => "membership manifest is not a module manifest",
            Self::PrimaryMembershipCount => {
                "each projected symbol needs exactly one primary module"
            }
            Self::FeaturedSymbolNotMember => "featured module symbol is not a module member",
        };
        formatter.write_str(message)
    }
}

impl Error for ModuleMapError {}
