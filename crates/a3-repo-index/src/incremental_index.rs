use crate::graph::{
    DeterministicGraphLinker, DeterministicGraphRanker, DeterministicModuleFormer,
    GraphComputationControl, GraphComputationControlError, GraphLinkFailure, GraphLinkInput,
    GraphLinkPolicy, GraphRankFailure, ModuleFormationFailure, ModuleFormationInput,
    ModuleFormationPolicy, RankingPolicy,
};
use crate::path::{RepositoryPathObservation, observe_repository_path, open_regular_no_follow};
use crate::{
    ParserPoolSize, PythonLanguageAdapter, PythonLanguageAdapterCreateError, RustLanguageAdapter,
    RustLanguageAdapterCreateError, TypeScriptJavaScriptLanguageAdapter,
    TypeScriptJavaScriptLanguageAdapterCreateError,
};
use a3_application::{
    IndexRunIdFactory, IndexRunIdFactoryFailure, LanguageAdapter, LanguageParseControl,
    LanguageParseControlError, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
    RepositoryIndexCompilation, RepositoryIndexCompiler, RepositoryIndexCompilerFailure,
    RepositoryIndexControl, RepositoryIndexMode, SnapshotCompatibility,
};
use a3_domain::{
    DiscoveredFileRole, DiscoveryResult, FileRevision, IndexLanguage, IndexPublication, IndexRunId,
    IndexSchemaVersion, LanguageParseResult, Progress, ProjectIdentity, RankingPolicyVersion,
    RepositoryFileState, RepositoryPath, Snapshot, SnapshotDelta,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::io::Read;
use std::time::{Duration, Instant};

const INDEX_TIMEOUT: Duration = Duration::from_secs(30);
const READ_BUFFER_BYTES: usize = 64 * 1024;
const RUN_ID_DOMAIN: &[u8] = b"a3.index-run.v1\0";

/// Stateful built-in Rust, TypeScript/JavaScript, and Python incremental compiler.
#[derive(Debug)]
pub struct BuiltinIncrementalIndexCompiler {
    rust: RustLanguageAdapter,
    typescript_javascript: TypeScriptJavaScriptLanguageAdapter,
    python: PythonLanguageAdapter,
    cached_snapshot: Option<a3_domain::SnapshotId>,
    parses: BTreeMap<RepositoryPath, LanguageParseResult>,
}

impl BuiltinIncrementalIndexCompiler {
    /// Creates all pinned parser pools with the same bounded per-grammar capacity.
    pub fn new(size: ParserPoolSize) -> Result<Self, BuiltinIncrementalIndexCompilerCreateError> {
        Ok(Self {
            rust: RustLanguageAdapter::new(size)
                .map_err(BuiltinIncrementalIndexCompilerCreateError::Rust)?,
            typescript_javascript: TypeScriptJavaScriptLanguageAdapter::new(size)
                .map_err(BuiltinIncrementalIndexCompilerCreateError::TypeScriptJavaScript)?,
            python: PythonLanguageAdapter::new(size)
                .map_err(BuiltinIncrementalIndexCompilerCreateError::Python)?,
            cached_snapshot: None,
            parses: BTreeMap::new(),
        })
    }

    fn adapters(&self) -> [&dyn LanguageAdapter; 3] {
        [&self.rust, &self.typescript_javascript, &self.python]
    }

    fn adapter_for(
        &self,
        path: &RepositoryPath,
    ) -> Result<Option<&dyn LanguageAdapter>, RepositoryIndexCompilerFailure> {
        let mut matching = self
            .adapters()
            .into_iter()
            .filter(|adapter| adapter.supports_path(path));
        let selected = matching.next();
        if matching.next().is_some() {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }
        Ok(selected)
    }

    fn cache_mode(
        &self,
        snapshot: &Snapshot,
        files: &BTreeMap<RepositoryPath, FileRevision>,
        supported: &BTreeSet<RepositoryPath>,
        changed: &BTreeSet<RepositoryPath>,
        delta: &SnapshotDelta,
    ) -> RepositoryIndexMode {
        let exact_snapshot = self.cached_snapshot == Some(snapshot.id()) && delta.is_empty();
        let exact_parent = self.cached_snapshot.is_some()
            && self.cached_snapshot == snapshot.parent_id()
            && snapshot.changes() == delta.snapshot_changes();
        if !exact_snapshot && !exact_parent {
            return RepositoryIndexMode::Full;
        }
        let reusable = supported.iter().all(|path| {
            changed.contains(path)
                || self.parses.get(path).is_some_and(|parse| {
                    files.get(path).is_some_and(|revision| {
                        parse.revision() == revision
                            && snapshot
                                .adapter_revisions()
                                .contains(parse.adapter_revision())
                    })
                })
        });
        if reusable {
            RepositoryIndexMode::Incremental
        } else {
            RepositoryIndexMode::Full
        }
    }
}

impl RepositoryIndexCompiler for BuiltinIncrementalIndexCompiler {
    fn compatibility(&self) -> Result<SnapshotCompatibility, RepositoryIndexCompilerFailure> {
        SnapshotCompatibility::new(
            IndexSchemaVersion::v4(),
            self.adapters()
                .into_iter()
                .map(|adapter| adapter.revision().clone())
                .collect(),
        )
        .map_err(|_| RepositoryIndexCompilerFailure::InvalidResult)
    }

    fn ranking_policy_version(&self) -> RankingPolicyVersion {
        RankingPolicy::v1().version()
    }

    fn compile(
        &mut self,
        project: &ProjectIdentity,
        snapshot: &Snapshot,
        files: &RepositoryFileState,
        discovery: &DiscoveryResult,
        delta: &SnapshotDelta,
        control: &dyn RepositoryIndexControl,
    ) -> Result<RepositoryIndexCompilation, RepositoryIndexCompilerFailure> {
        let started = Instant::now();
        ensure_active(control, started)?;
        report(control, 0)?;
        if snapshot.worktree_id() != project.worktree().id()
            || discovery.worktree_id() != project.worktree().id()
        {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }
        let compatibility = self.compatibility()?;
        if snapshot.index_schema_version() != compatibility.index_schema_version()
            || snapshot.adapter_revisions() != compatibility.adapter_revisions()
        {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }

        let files_by_path = files
            .revisions()
            .iter()
            .map(|revision| (revision.path().clone(), revision.clone()))
            .collect::<BTreeMap<_, _>>();
        let roles_by_path = discovery
            .files()
            .iter()
            .map(|file| (file.path().clone(), file.roles()))
            .collect::<BTreeMap<_, _>>();
        if files_by_path.len() != roles_by_path.len()
            || files_by_path.keys().ne(roles_by_path.keys())
        {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }

        let mut supported = BTreeSet::new();
        for path in files_by_path.keys() {
            if self.adapter_for(path)?.is_some() {
                supported.insert(path.clone());
            }
        }
        let changed = delta
            .files()
            .iter()
            .map(|change| change.path().clone())
            .collect::<BTreeSet<_>>();
        let mode = self.cache_mode(snapshot, &files_by_path, &supported, &changed, delta);
        let parse_paths = match mode {
            RepositoryIndexMode::Full => supported.iter().cloned().collect::<Vec<_>>(),
            RepositoryIndexMode::Incremental => supported
                .intersection(&changed)
                .cloned()
                .collect::<Vec<_>>(),
        };
        let mut next_parses = match mode {
            RepositoryIndexMode::Full => BTreeMap::new(),
            RepositoryIndexMode::Incremental => self.parses.clone(),
        };
        next_parses.retain(|path, _| supported.contains(path) && !changed.contains(path));

        let parse_control = CompilerParseControl { inner: control };
        for path in &parse_paths {
            ensure_active(control, started)?;
            let revision = files_by_path
                .get(path)
                .ok_or(RepositoryIndexCompilerFailure::InvalidResult)?;
            let roles = roles_by_path
                .get(path)
                .copied()
                .ok_or(RepositoryIndexCompilerFailure::InvalidResult)?;
            let source = read_source(project, revision, control, started)?;
            let adapter = self
                .adapter_for(path)?
                .ok_or(RepositoryIndexCompilerFailure::InvalidResult)?;
            let parse = adapter
                .parse(
                    LanguageParseInput::new(revision, &source, roles),
                    LanguageParsePolicy::v1(),
                    &parse_control,
                )
                .map_err(map_parse_failure)?;
            next_parses.insert(path.clone(), parse);
            validate_aggregate(&next_parses)?;
        }
        if next_parses.len() != supported.len() || next_parses.keys().ne(supported.iter()) {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }
        report(control, 2)?;

        let parses = next_parses.values().cloned().collect::<Vec<_>>();
        let graph_control = CompilerGraphControl { inner: control };
        let graph = DeterministicGraphLinker
            .link(
                GraphLinkInput::new(snapshot, files, &parses),
                GraphLinkPolicy::v1(),
                &graph_control,
            )
            .map_err(map_link_failure)?;
        ensure_active(control, started)?;
        report(control, 3)?;
        let ranking = DeterministicGraphRanker
            .rank(&graph, RankingPolicy::v1(), &graph_control)
            .map_err(map_rank_failure)?;
        ensure_active(control, started)?;
        let manifest_files = files_by_path
            .iter()
            .filter_map(|(path, revision)| {
                roles_by_path
                    .get(path)
                    .copied()
                    .filter(|roles| roles.contains(DiscoveredFileRole::Manifest))
                    .map(|_| revision.clone())
            })
            .collect::<Vec<_>>();
        let mut languages = next_parses
            .values()
            .map(LanguageParseResult::language)
            .collect::<BTreeSet<_>>();
        if supported.len() != files_by_path.len() {
            languages.insert(IndexLanguage::Generic);
        }
        let languages = languages.into_iter().collect::<Vec<_>>();
        let modules = DeterministicModuleFormer
            .form(
                ModuleFormationInput::new(&graph, &ranking, &manifest_files, &languages),
                ModuleFormationPolicy::v1(),
                &graph_control,
            )
            .map_err(map_module_failure)?;
        report(control, 4)?;
        let publication = IndexPublication::new(graph, ranking, manifest_files, modules)
            .map_err(|_| RepositoryIndexCompilerFailure::InvalidResult)?;
        report(control, 5)?;

        self.cached_snapshot = Some(snapshot.id());
        self.parses = next_parses;
        RepositoryIndexCompilation::new(publication, mode, parse_paths)
    }
}

fn validate_aggregate(
    parses: &BTreeMap<RepositoryPath, LanguageParseResult>,
) -> Result<(), RepositoryIndexCompilerFailure> {
    let policy = GraphLinkPolicy::v1();
    if parses.len() > policy.max_parses() {
        return Err(RepositoryIndexCompilerFailure::ResourceLimitExceeded);
    }
    let (symbols, relations) = parses
        .values()
        .try_fold((0usize, 0usize), |(symbols, relations), parse| {
            Some((
                symbols.checked_add(parse.symbols().len())?,
                relations.checked_add(parse.relations().len())?,
            ))
        })
        .ok_or(RepositoryIndexCompilerFailure::ResourceLimitExceeded)?;
    if symbols > policy.max_symbols()
        || relations > policy.max_edges().saturating_add(policy.max_unresolved())
    {
        return Err(RepositoryIndexCompilerFailure::ResourceLimitExceeded);
    }
    Ok(())
}

fn read_source(
    project: &ProjectIdentity,
    revision: &FileRevision,
    control: &dyn RepositoryIndexControl,
    started: Instant,
) -> Result<Vec<u8>, RepositoryIndexCompilerFailure> {
    let observation = observe_repository_path(project.worktree().root().as_path(), revision.path())
        .map_err(|_| RepositoryIndexCompilerFailure::Filesystem)?;
    let RepositoryPathObservation::Present { path, metadata } = observation else {
        return Err(RepositoryIndexCompilerFailure::RevisionMismatch);
    };
    let policy = LanguageParsePolicy::v1();
    let length = usize::try_from(metadata.len())
        .map_err(|_| RepositoryIndexCompilerFailure::ResourceLimitExceeded)?;
    if length > policy.max_source_bytes() {
        return Err(RepositoryIndexCompilerFailure::ResourceLimitExceeded);
    }
    let mut file =
        open_regular_no_follow(&path).map_err(|_| RepositoryIndexCompilerFailure::Filesystem)?;
    let mut source = Vec::with_capacity(length);
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    loop {
        ensure_active(control, started)?;
        let count = file
            .read(&mut buffer)
            .map_err(|_| RepositoryIndexCompilerFailure::Filesystem)?;
        if count == 0 {
            break;
        }
        if source.len().saturating_add(count) > policy.max_source_bytes() {
            return Err(RepositoryIndexCompilerFailure::ResourceLimitExceeded);
        }
        source.extend_from_slice(&buffer[..count]);
    }
    Ok(source)
}

fn ensure_active(
    control: &dyn RepositoryIndexControl,
    started: Instant,
) -> Result<(), RepositoryIndexCompilerFailure> {
    if control.is_cancelled() {
        return Err(RepositoryIndexCompilerFailure::Cancelled);
    }
    if started.elapsed() > INDEX_TIMEOUT {
        return Err(RepositoryIndexCompilerFailure::TimedOut);
    }
    Ok(())
}

fn report(
    control: &dyn RepositoryIndexControl,
    completed: u64,
) -> Result<(), RepositoryIndexCompilerFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 5)
                .map_err(|_| RepositoryIndexCompilerFailure::InvalidResult)?,
        )
        .map_err(|_| RepositoryIndexCompilerFailure::ProgressUnavailable)
}

#[derive(Debug)]
struct CompilerParseControl<'a> {
    inner: &'a dyn RepositoryIndexControl,
}

impl LanguageParseControl for CompilerParseControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct CompilerGraphControl<'a> {
    inner: &'a dyn RepositoryIndexControl,
}

impl GraphComputationControl for CompilerGraphControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), GraphComputationControlError> {
        Ok(())
    }
}

fn map_parse_failure(failure: LanguageParseFailure) -> RepositoryIndexCompilerFailure {
    match failure {
        LanguageParseFailure::Cancelled => RepositoryIndexCompilerFailure::Cancelled,
        LanguageParseFailure::InputTooLarge | LanguageParseFailure::ResourceLimitExceeded => {
            RepositoryIndexCompilerFailure::ResourceLimitExceeded
        }
        LanguageParseFailure::RevisionMismatch => RepositoryIndexCompilerFailure::RevisionMismatch,
        LanguageParseFailure::ParserUnavailable | LanguageParseFailure::TimedOut => {
            RepositoryIndexCompilerFailure::TimedOut
        }
        LanguageParseFailure::ProgressUnavailable => {
            RepositoryIndexCompilerFailure::ProgressUnavailable
        }
        LanguageParseFailure::UnsupportedPath
        | LanguageParseFailure::ParseFailed
        | LanguageParseFailure::InvalidResult => RepositoryIndexCompilerFailure::InvalidResult,
    }
}

fn map_link_failure(failure: GraphLinkFailure) -> RepositoryIndexCompilerFailure {
    match failure {
        GraphLinkFailure::Cancelled => RepositoryIndexCompilerFailure::Cancelled,
        GraphLinkFailure::TimedOut => RepositoryIndexCompilerFailure::TimedOut,
        GraphLinkFailure::ResourceLimitExceeded => {
            RepositoryIndexCompilerFailure::ResourceLimitExceeded
        }
        GraphLinkFailure::ProgressUnavailable => {
            RepositoryIndexCompilerFailure::ProgressUnavailable
        }
        GraphLinkFailure::InvalidInput | GraphLinkFailure::InvalidGraph => {
            RepositoryIndexCompilerFailure::InvalidResult
        }
    }
}

fn map_rank_failure(failure: GraphRankFailure) -> RepositoryIndexCompilerFailure {
    match failure {
        GraphRankFailure::Cancelled => RepositoryIndexCompilerFailure::Cancelled,
        GraphRankFailure::TimedOut => RepositoryIndexCompilerFailure::TimedOut,
        GraphRankFailure::ResourceLimitExceeded => {
            RepositoryIndexCompilerFailure::ResourceLimitExceeded
        }
        GraphRankFailure::ProgressUnavailable => {
            RepositoryIndexCompilerFailure::ProgressUnavailable
        }
        GraphRankFailure::InvalidGraph | GraphRankFailure::InvalidProjection => {
            RepositoryIndexCompilerFailure::InvalidResult
        }
    }
}

fn map_module_failure(failure: ModuleFormationFailure) -> RepositoryIndexCompilerFailure {
    match failure {
        ModuleFormationFailure::Cancelled => RepositoryIndexCompilerFailure::Cancelled,
        ModuleFormationFailure::TimedOut => RepositoryIndexCompilerFailure::TimedOut,
        ModuleFormationFailure::ResourceLimitExceeded => {
            RepositoryIndexCompilerFailure::ResourceLimitExceeded
        }
        ModuleFormationFailure::ProgressUnavailable => {
            RepositoryIndexCompilerFailure::ProgressUnavailable
        }
        ModuleFormationFailure::InvalidInput | ModuleFormationFailure::InvalidProjection => {
            RepositoryIndexCompilerFailure::InvalidResult
        }
    }
}

/// Failure to construct one of the pinned built-in language adapters.
#[derive(Debug)]
pub enum BuiltinIncrementalIndexCompilerCreateError {
    /// Rust adapter initialization failed.
    Rust(RustLanguageAdapterCreateError),
    /// TypeScript/JavaScript adapter initialization failed.
    TypeScriptJavaScript(TypeScriptJavaScriptLanguageAdapterCreateError),
    /// Python adapter initialization failed.
    Python(PythonLanguageAdapterCreateError),
}

impl fmt::Display for BuiltinIncrementalIndexCompilerCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rust(error) => write!(formatter, "Rust index adapter failed: {error}"),
            Self::TypeScriptJavaScript(error) => {
                write!(
                    formatter,
                    "TypeScript/JavaScript index adapter failed: {error}"
                )
            }
            Self::Python(error) => write!(formatter, "Python index adapter failed: {error}"),
        }
    }
}

impl Error for BuiltinIncrementalIndexCompilerCreateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Rust(source) => Some(source),
            Self::TypeScriptJavaScript(source) => Some(source),
            Self::Python(source) => Some(source),
        }
    }
}

/// BLAKE3 implementation of deterministic worktree-local index-attempt identities.
#[derive(Debug, Clone, Copy, Default)]
pub struct Blake3IndexRunIdFactory;

impl IndexRunIdFactory for Blake3IndexRunIdFactory {
    fn create(
        &self,
        project: &ProjectIdentity,
        snapshot: &Snapshot,
        ranking_policy_version: RankingPolicyVersion,
        attempt_ordinal: u64,
    ) -> Result<IndexRunId, IndexRunIdFactoryFailure> {
        if attempt_ordinal == 0 || snapshot.worktree_id() != project.worktree().id() {
            return Err(IndexRunIdFactoryFailure);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(RUN_ID_DOMAIN);
        hasher.update(project.repository().id().as_bytes());
        hasher.update(project.worktree().id().as_bytes());
        hasher.update(snapshot.id().as_bytes());
        hasher.update(&ranking_policy_version.get().to_be_bytes());
        hasher.update(&attempt_ordinal.to_be_bytes());
        Ok(IndexRunId::from_bytes(*hasher.finalize().as_bytes()))
    }
}
