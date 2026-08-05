use super::{GraphComputationControl, GraphComputationControlError};
use a3_domain::{
    EvidenceRef, FileRevision, GraphEndpoint, IndexLanguage, LinkedGraph, ModuleId, ModuleKind,
    ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot,
    ModuleSymbolSet, Progress, RankProjection, RepositoryCard, RepositoryModule, RepositoryPath,
    SymbolId, SymbolRole, SyntaxRelationKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const PRIMARY_MODULE_ID_DOMAIN: &[u8] = b"a3.module.primary.v1\0";
const COMMUNITY_MODULE_ID_DOMAIN: &[u8] = b"a3.module.community.v1\0";
const WORK_POLL_INTERVAL: usize = 1_024;
const PROGRESS_TOTAL: u64 = 5;

/// Immutable inputs to deterministic module formation.
#[derive(Debug, Clone, Copy)]
pub struct ModuleFormationInput<'a> {
    graph: &'a LinkedGraph,
    ranking: &'a RankProjection,
    manifest_files: &'a [FileRevision],
    languages: &'a [IndexLanguage],
}

impl<'a> ModuleFormationInput<'a> {
    /// Binds the published graph candidates and their deterministic ranking metadata.
    #[must_use]
    pub const fn new(
        graph: &'a LinkedGraph,
        ranking: &'a RankProjection,
        manifest_files: &'a [FileRevision],
        languages: &'a [IndexLanguage],
    ) -> Self {
        Self {
            graph,
            ranking,
            manifest_files,
            languages,
        }
    }
}

/// Fixed resource and result bounds for module formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleFormationPolicy {
    version: ModulePolicyVersion,
    timeout: Duration,
    max_files: usize,
    max_symbols: usize,
    max_edges: usize,
    max_modules: usize,
    max_memberships: usize,
    central_symbol_limit: usize,
    role_symbol_limit: usize,
}

impl ModuleFormationPolicy {
    /// Returns deterministic V1 path, manifest, and SCC-community semantics.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: ModulePolicyVersion::v1(),
            timeout: Duration::from_secs(5),
            max_files: 250_000,
            max_symbols: 1_000_000,
            max_edges: 2_000_000,
            max_modules: 250_000,
            max_memberships: 2_000_000,
            central_symbol_limit: 16,
            role_symbol_limit: 256,
        }
    }

    /// Returns the durable module-formation revision.
    #[must_use]
    pub const fn version(self) -> ModulePolicyVersion {
        self.version
    }

    /// Returns the maximum accepted module count.
    #[must_use]
    pub const fn max_modules(self) -> usize {
        self.max_modules
    }

    /// Returns the maximum accepted primary plus supplementary membership count.
    #[must_use]
    pub const fn max_memberships(self) -> usize {
        self.max_memberships
    }
}

impl Default for ModuleFormationPolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// Stateless, LLM-free manifest/path module and graph-community projection.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicModuleFormer;

impl DeterministicModuleFormer {
    /// Forms primary path modules and supplementary strongly connected communities.
    pub fn form(
        &self,
        input: ModuleFormationInput<'_>,
        policy: ModuleFormationPolicy,
        control: &dyn GraphComputationControl,
    ) -> Result<ModuleProjection, ModuleFormationFailure> {
        let started = Instant::now();
        validate_input(input, policy)?;
        ensure_active(control, started, policy.timeout)?;
        report(control, 0)?;

        let manifest_boundaries = manifest_boundaries(input.manifest_files)?;
        let mut drafts = manifest_boundaries
            .iter()
            .map(|(root, manifests)| {
                let id = primary_module_id(root);
                (
                    id,
                    ModuleDraft::primary(
                        id,
                        ModuleKind::ManifestBoundary,
                        root.clone(),
                        manifests.clone(),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut primary_by_symbol = BTreeMap::<SymbolId, ModuleId>::new();
        let mut memberships = Vec::new();
        for (index, symbol) in input.graph.symbols().iter().enumerate() {
            poll(index, control, started, policy.timeout)?;
            let root = nearest_manifest_root(symbol.revision().path(), &manifest_boundaries)
                .unwrap_or_else(|| path_boundary(symbol.revision().path()));
            let module_id = primary_module_id(&root);
            let draft = drafts.entry(module_id).or_insert_with(|| {
                ModuleDraft::primary(
                    module_id,
                    ModuleKind::PathBoundary,
                    root.clone(),
                    Vec::new(),
                )
            });
            draft.members.insert(symbol.id());
            let evidence = match draft.manifests.first() {
                Some(manifest) => {
                    ModuleMembershipEvidence::manifest(symbol.revision().clone(), manifest.clone())
                }
                None => ModuleMembershipEvidence::path(symbol.revision().clone()),
            };
            memberships.push(ModuleMembership::new(module_id, symbol.id(), evidence));
            primary_by_symbol.insert(symbol.id(), module_id);
        }
        check_projection_bounds(&drafts, &memberships, policy)?;
        report(control, 1)?;

        let components = strongly_connected_components(input.graph, control, started, policy)?;
        let community_by_symbol = components
            .iter()
            .enumerate()
            .flat_map(|(index, component)| {
                component.iter().copied().map(move |symbol| (symbol, index))
            })
            .collect::<BTreeMap<_, _>>();
        let witnesses = community_witnesses(input.graph, &community_by_symbol);
        for component in components {
            let module_id = community_module_id(&component);
            let mut draft = ModuleDraft::community(module_id);
            for symbol_id in component {
                let graph_symbol = input
                    .graph
                    .symbols()
                    .binary_search_by_key(&symbol_id, |symbol| symbol.id())
                    .ok()
                    .and_then(|position| input.graph.symbols().get(position))
                    .ok_or(ModuleFormationFailure::InvalidInput)?;
                let relationship_evidence = witnesses
                    .get(&symbol_id)
                    .cloned()
                    .ok_or(ModuleFormationFailure::InvalidInput)?;
                let evidence = ModuleMembershipEvidence::graph(
                    graph_symbol.revision().clone(),
                    relationship_evidence,
                )
                .map_err(|_| ModuleFormationFailure::InvalidProjection)?;
                draft.members.insert(symbol_id);
                memberships.push(ModuleMembership::new(module_id, symbol_id, evidence));
            }
            drafts.insert(module_id, draft);
        }
        check_projection_bounds(&drafts, &memberships, policy)?;
        ensure_active(control, started, policy.timeout)?;
        report(control, 3)?;

        let symbols_by_id = input
            .graph
            .symbols()
            .iter()
            .map(|symbol| (symbol.id(), symbol))
            .collect::<BTreeMap<_, _>>();
        let mut modules = Vec::with_capacity(drafts.len());
        for draft in drafts.values() {
            let central_symbols = select_ranked(
                input.ranking,
                &draft.members,
                |_| true,
                policy.central_symbol_limit,
            )?;
            let entrypoints = select_ranked(
                input.ranking,
                &draft.members,
                |id| {
                    symbols_by_id.get(&id).is_some_and(|symbol| {
                        symbol.parsed().roles().contains(SymbolRole::Entrypoint)
                    })
                },
                policy.role_symbol_limit,
            )?;
            let tests = select_ranked(
                input.ranking,
                &draft.members,
                |id| {
                    symbols_by_id
                        .get(&id)
                        .is_some_and(|symbol| symbol.parsed().roles().contains(SymbolRole::Test))
                },
                policy.role_symbol_limit,
            )?;
            modules.push(
                RepositoryModule::new(
                    draft.id,
                    draft.kind,
                    draft.root.clone(),
                    draft.manifests.clone(),
                    central_symbols,
                    entrypoints,
                    tests,
                )
                .map_err(|_| ModuleFormationFailure::InvalidProjection)?,
            );
        }
        report(control, 4)?;

        let packages = modules
            .iter()
            .filter(|module| module.kind().is_primary())
            .map(RepositoryModule::id)
            .collect::<Vec<_>>();
        let primary_symbols = primary_by_symbol.keys().copied().collect::<BTreeSet<_>>();
        let repository_entrypoints = select_ranked(
            input.ranking,
            &primary_symbols,
            |id| {
                symbols_by_id
                    .get(&id)
                    .is_some_and(|symbol| symbol.parsed().roles().contains(SymbolRole::Entrypoint))
            },
            policy.role_symbol_limit,
        )?;
        let file_count = u32::try_from(input.graph.files().len())
            .map_err(|_| ModuleFormationFailure::ResourceLimitExceeded)?;
        let symbol_count = u32::try_from(input.graph.symbols().len())
            .map_err(|_| ModuleFormationFailure::ResourceLimitExceeded)?;
        let card = RepositoryCard::new(
            input.graph.snapshot_id(),
            policy.version(),
            packages,
            input.languages.to_vec(),
            repository_entrypoints,
            file_count,
            symbol_count,
        )
        .map_err(|_| ModuleFormationFailure::InvalidProjection)?;
        let projection = ModuleProjection::new(
            input.graph.snapshot_id(),
            policy.version(),
            modules,
            memberships,
            card,
        )
        .map_err(|_| ModuleFormationFailure::InvalidProjection)?;
        ensure_active(control, started, policy.timeout)?;
        report(control, PROGRESS_TOTAL)?;
        Ok(projection)
    }
}

fn validate_input(
    input: ModuleFormationInput<'_>,
    policy: ModuleFormationPolicy,
) -> Result<(), ModuleFormationFailure> {
    if input.graph.snapshot_id() != input.ranking.snapshot_id()
        || input.graph.files().len() > policy.max_files
        || input.graph.symbols().len() > policy.max_symbols
        || input.graph.edges().len() > policy.max_edges
    {
        return Err(ModuleFormationFailure::InvalidInput);
    }
    let files = input
        .graph
        .files()
        .iter()
        .map(|revision| (revision.path(), revision))
        .collect::<BTreeMap<_, _>>();
    let mut previous = None;
    for manifest in input.manifest_files {
        if previous.is_some_and(|path| path >= manifest.path())
            || files.get(manifest.path()).copied() != Some(manifest)
        {
            return Err(ModuleFormationFailure::InvalidInput);
        }
        previous = Some(manifest.path());
    }
    Ok(())
}

fn manifest_boundaries(
    manifests: &[FileRevision],
) -> Result<BTreeMap<ModuleRoot, Vec<FileRevision>>, ModuleFormationFailure> {
    let mut boundaries = BTreeMap::<ModuleRoot, Vec<FileRevision>>::new();
    for manifest in manifests {
        if is_package_manifest(manifest.path()) {
            boundaries
                .entry(parent_root(manifest.path())?)
                .or_default()
                .push(manifest.clone());
        }
    }
    Ok(boundaries)
}

fn is_package_manifest(path: &RepositoryPath) -> bool {
    let basename = path
        .as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or(path.as_bytes());
    [
        b"cargo.toml".as_slice(),
        b"package.json",
        b"deno.json",
        b"deno.jsonc",
        b"pyproject.toml",
        b"setup.py",
        b"setup.cfg",
        b"go.mod",
        b"pom.xml",
        b"build.gradle",
        b"build.gradle.kts",
        b"gemfile",
        b"composer.json",
    ]
    .iter()
    .any(|known| basename.eq_ignore_ascii_case(known))
}

fn parent_root(path: &RepositoryPath) -> Result<ModuleRoot, ModuleFormationFailure> {
    let Some(separator) = path.as_bytes().iter().rposition(|byte| *byte == b'/') else {
        return Ok(ModuleRoot::Repository);
    };
    RepositoryPath::try_from_bytes(path.as_bytes()[..separator].to_vec())
        .map(ModuleRoot::Directory)
        .map_err(|_| ModuleFormationFailure::InvalidInput)
}

fn nearest_manifest_root(
    path: &RepositoryPath,
    boundaries: &BTreeMap<ModuleRoot, Vec<FileRevision>>,
) -> Option<ModuleRoot> {
    boundaries
        .keys()
        .filter(|root| root_contains(root, path))
        .max_by_key(|root| root_depth(root))
        .cloned()
}

fn path_boundary(path: &RepositoryPath) -> ModuleRoot {
    match path.as_bytes().iter().position(|byte| *byte == b'/') {
        Some(separator) => RepositoryPath::try_from_bytes(path.as_bytes()[..separator].to_vec())
            .map(ModuleRoot::Directory)
            .unwrap_or(ModuleRoot::Repository),
        None => ModuleRoot::Repository,
    }
}

fn root_contains(root: &ModuleRoot, path: &RepositoryPath) -> bool {
    match root {
        ModuleRoot::Repository => true,
        ModuleRoot::Directory(directory) => {
            path.as_bytes() == directory.as_bytes()
                || path
                    .as_bytes()
                    .strip_prefix(directory.as_bytes())
                    .is_some_and(|suffix| suffix.starts_with(b"/"))
        }
    }
}

fn root_depth(root: &ModuleRoot) -> usize {
    match root {
        ModuleRoot::Repository => 0,
        ModuleRoot::Directory(path) => path.as_bytes().len().saturating_add(1),
    }
}

fn primary_module_id(root: &ModuleRoot) -> ModuleId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PRIMARY_MODULE_ID_DOMAIN);
    match root {
        ModuleRoot::Repository => hasher.update(&[0]),
        ModuleRoot::Directory(path) => {
            hasher.update(&[1]);
            hasher.update(path.as_bytes())
        }
    };
    ModuleId::from_bytes(*hasher.finalize().as_bytes())
}

fn community_module_id(symbols: &[SymbolId]) -> ModuleId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(COMMUNITY_MODULE_ID_DOMAIN);
    for symbol in symbols {
        hasher.update(symbol.as_bytes());
    }
    ModuleId::from_bytes(*hasher.finalize().as_bytes())
}

fn strongly_connected_components(
    graph: &LinkedGraph,
    control: &dyn GraphComputationControl,
    started: Instant,
    policy: ModuleFormationPolicy,
) -> Result<Vec<Vec<SymbolId>>, ModuleFormationFailure> {
    let mut forward = BTreeMap::<SymbolId, Vec<SymbolId>>::new();
    let mut reverse = BTreeMap::<SymbolId, Vec<SymbolId>>::new();
    for (index, edge) in graph.edges().iter().enumerate() {
        poll(index, control, started, policy.timeout)?;
        if matches!(
            edge.kind(),
            SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
        ) {
            continue;
        }
        let (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) =
            (edge.source(), edge.target())
        else {
            continue;
        };
        if source == target {
            continue;
        }
        forward.entry(*source).or_default().push(*target);
        reverse.entry(*target).or_default().push(*source);
        forward.entry(*target).or_default();
        reverse.entry(*source).or_default();
    }
    for neighbors in forward.values_mut().chain(reverse.values_mut()) {
        neighbors.sort();
        neighbors.dedup();
    }

    let mut visited = BTreeSet::new();
    let mut finish_order = Vec::with_capacity(forward.len());
    for root in forward.keys().copied() {
        if visited.contains(&root) {
            continue;
        }
        visited.insert(root);
        let mut stack = vec![(root, 0usize)];
        while let Some((node, next_neighbor)) = stack.last_mut() {
            poll(
                visited.len().saturating_add(finish_order.len()),
                control,
                started,
                policy.timeout,
            )?;
            let neighbors = forward
                .get(node)
                .ok_or(ModuleFormationFailure::InvalidInput)?;
            if let Some(neighbor) = neighbors.get(*next_neighbor).copied() {
                *next_neighbor = next_neighbor.saturating_add(1);
                if visited.insert(neighbor) {
                    stack.push((neighbor, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }
    report(control, 2)?;

    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for root in finish_order.into_iter().rev() {
        if !assigned.insert(root) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            component.push(node);
            let neighbors = reverse
                .get(&node)
                .ok_or(ModuleFormationFailure::InvalidInput)?;
            for neighbor in neighbors.iter().rev().copied() {
                if assigned.insert(neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        if component.len() > 1 {
            component.sort();
            components.push(component);
        }
    }
    components.sort();
    if components.len() > policy.max_modules {
        return Err(ModuleFormationFailure::ResourceLimitExceeded);
    }
    Ok(components)
}

fn community_witnesses(
    graph: &LinkedGraph,
    community_by_symbol: &BTreeMap<SymbolId, usize>,
) -> BTreeMap<SymbolId, Vec<EvidenceRef>> {
    let mut incoming = BTreeMap::<SymbolId, EvidenceRef>::new();
    let mut outgoing = BTreeMap::<SymbolId, EvidenceRef>::new();
    for edge in graph.edges() {
        if matches!(
            edge.kind(),
            SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
        ) {
            continue;
        }
        let (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) =
            (edge.source(), edge.target())
        else {
            continue;
        };
        if source == target
            || community_by_symbol.get(source).is_none()
            || community_by_symbol.get(source) != community_by_symbol.get(target)
        {
            continue;
        }
        outgoing
            .entry(*source)
            .or_insert_with(|| edge.evidence().clone());
        incoming
            .entry(*target)
            .or_insert_with(|| edge.evidence().clone());
    }
    community_by_symbol
        .keys()
        .copied()
        .map(|symbol| {
            let evidence = incoming
                .get(&symbol)
                .into_iter()
                .chain(outgoing.get(&symbol))
                .cloned()
                .collect::<Vec<_>>();
            (symbol, evidence)
        })
        .collect()
}

fn select_ranked(
    ranking: &RankProjection,
    members: &BTreeSet<SymbolId>,
    include: impl Fn(SymbolId) -> bool,
    limit: usize,
) -> Result<ModuleSymbolSet, ModuleFormationFailure> {
    let mut selected = Vec::new();
    let mut matched = 0usize;
    for rank in ranking.symbols() {
        let id = rank.symbol_id();
        if members.contains(&id) && include(id) {
            matched = matched
                .checked_add(1)
                .ok_or(ModuleFormationFailure::ResourceLimitExceeded)?;
            if selected.len() < limit {
                selected.push(id);
            }
        }
    }
    ModuleSymbolSet::new(selected, matched > limit)
        .map_err(|_| ModuleFormationFailure::InvalidProjection)
}

fn check_projection_bounds(
    drafts: &BTreeMap<ModuleId, ModuleDraft>,
    memberships: &[ModuleMembership],
    policy: ModuleFormationPolicy,
) -> Result<(), ModuleFormationFailure> {
    if drafts.len() > policy.max_modules || memberships.len() > policy.max_memberships {
        return Err(ModuleFormationFailure::ResourceLimitExceeded);
    }
    Ok(())
}

fn poll(
    completed: usize,
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), ModuleFormationFailure> {
    if completed.is_multiple_of(WORK_POLL_INTERVAL) {
        ensure_active(control, started, timeout)?;
    }
    Ok(())
}

fn ensure_active(
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), ModuleFormationFailure> {
    if control.is_cancelled() {
        return Err(ModuleFormationFailure::Cancelled);
    }
    if started.elapsed() > timeout {
        return Err(ModuleFormationFailure::TimedOut);
    }
    Ok(())
}

fn report(
    control: &dyn GraphComputationControl,
    completed: u64,
) -> Result<(), ModuleFormationFailure> {
    let progress = Progress::determinate(completed, PROGRESS_TOTAL)
        .map_err(|_| ModuleFormationFailure::InvalidProjection)?;
    control
        .report_progress(progress)
        .map_err(|GraphComputationControlError::Unavailable| {
            ModuleFormationFailure::ProgressUnavailable
        })
}

#[derive(Debug)]
struct ModuleDraft {
    id: ModuleId,
    kind: ModuleKind,
    root: Option<ModuleRoot>,
    manifests: Vec<FileRevision>,
    members: BTreeSet<SymbolId>,
}

impl ModuleDraft {
    fn primary(
        id: ModuleId,
        kind: ModuleKind,
        root: ModuleRoot,
        manifests: Vec<FileRevision>,
    ) -> Self {
        Self {
            id,
            kind,
            root: Some(root),
            manifests,
            members: BTreeSet::new(),
        }
    }

    fn community(id: ModuleId) -> Self {
        Self {
            id,
            kind: ModuleKind::GraphCommunity,
            root: None,
            manifests: Vec::new(),
            members: BTreeSet::new(),
        }
    }
}

/// Stable failure classification for deterministic module formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleFormationFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// Module formation exceeded its fixed wall-clock budget.
    TimedOut,
    /// Input or output exceeded a fixed resource limit.
    ResourceLimitExceeded,
    /// The owning scheduler rejected determinate progress.
    ProgressUnavailable,
    /// Graph, rank, manifest, or language input was inconsistent.
    InvalidInput,
    /// Constructed module or repository-card invariants were violated.
    InvalidProjection,
}

impl fmt::Display for ModuleFormationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "module formation was cancelled",
            Self::TimedOut => "module formation timed out",
            Self::ResourceLimitExceeded => "module formation resource limit was exceeded",
            Self::ProgressUnavailable => "module formation progress could not be reported",
            Self::InvalidInput => "module formation input is invalid",
            Self::InvalidProjection => "module projection is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for ModuleFormationFailure {}
