//! Cross-language graph-linking, ranking, freshness, and failure-path integration tests.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRole, DiscoveredFileRoles, FileRevision, GitHead, GitReferenceName,
    GraphEndpoint, IndexLanguage, IndexSchemaVersion, LanguageAdapterRevision,
    LanguageAdapterVersion, LanguageParseResult, Progress, RepositoryFileState, RepositoryPath,
    Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotId, SymbolId, SymbolRole,
    SyntaxRelationKind, UnresolvedGraphTarget, UnresolvedReason, WorktreeGeneration, WorktreeId,
};
use a3_repo_index::{
    DeterministicGraphLinker, DeterministicGraphRanker, GraphComputationControl,
    GraphComputationControlError, GraphLinkFailure, GraphLinkInput, GraphLinkPolicy,
    GraphRankFailure, ParserPoolSize, PythonLanguageAdapter, RankingPolicy, RustLanguageAdapter,
    TypeScriptJavaScriptLanguageAdapter,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const FIXTURE_FILES: &[FixtureFile] = &[
    FixtureFile::manifest(
        "Cargo.toml",
        include_bytes!("../../../fixtures/graph-linker/Cargo.toml"),
    ),
    FixtureFile::source(
        "src/lib.rs",
        include_bytes!("../../../fixtures/graph-linker/src/lib.rs"),
    ),
    FixtureFile::source(
        "src/main.rs",
        include_bytes!("../../../fixtures/graph-linker/src/main.rs"),
    ),
    FixtureFile::source(
        "src/service.rs",
        include_bytes!("../../../fixtures/graph-linker/src/service.rs"),
    ),
    FixtureFile::source(
        "web/helper.ts",
        include_bytes!("../../../fixtures/graph-linker/web/helper.ts"),
    ),
    FixtureFile::source(
        "web/main.ts",
        include_bytes!("../../../fixtures/graph-linker/web/main.ts"),
    ),
    FixtureFile::manifest(
        "python/pyproject.toml",
        include_bytes!("../../../fixtures/graph-linker/python/pyproject.toml"),
    ),
    FixtureFile::source(
        "python/sample/__init__.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/__init__.py"),
    ),
    FixtureFile::source(
        "python/sample/base.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/base.py"),
    ),
    FixtureFile::source(
        "python/sample/service.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/service.py"),
    ),
    FixtureFile::test(
        "python/tests/test_service.py",
        include_bytes!("../../../fixtures/graph-linker/python/tests/test_service.py"),
    ),
];

#[derive(Clone, Copy)]
struct FixtureFile {
    path: &'static str,
    source: &'static [u8],
    roles: DiscoveredFileRoles,
}

impl FixtureFile {
    const fn source(path: &'static str, source: &'static [u8]) -> Self {
        Self {
            path,
            source,
            roles: DiscoveredFileRoles::empty(),
        }
    }

    const fn manifest(path: &'static str, source: &'static [u8]) -> Self {
        Self {
            path,
            source,
            roles: DiscoveredFileRoles::empty().with(DiscoveredFileRole::Manifest),
        }
    }

    const fn test(path: &'static str, source: &'static [u8]) -> Self {
        Self {
            path,
            source,
            roles: DiscoveredFileRoles::empty().with(DiscoveredFileRole::Test),
        }
    }
}

#[derive(Debug, Default)]
struct SilentParseControl;

impl LanguageParseControl for SilentParseControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RecordingGraphControl {
    cancelled: AtomicBool,
    reject_progress: AtomicBool,
    progress: Mutex<Vec<Progress>>,
}

impl RecordingGraphControl {
    fn cancelled() -> Self {
        Self {
            cancelled: AtomicBool::new(true),
            ..Self::default()
        }
    }

    fn rejecting_progress() -> Self {
        Self {
            reject_progress: AtomicBool::new(true),
            ..Self::default()
        }
    }

    fn observations(&self) -> Result<Vec<Progress>, Box<dyn Error>> {
        self.progress
            .lock()
            .map(|values| values.clone())
            .map_err(|_| "graph progress mutex was poisoned".into())
    }
}

impl GraphComputationControl for RecordingGraphControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn report_progress(&self, progress: Progress) -> Result<(), GraphComputationControlError> {
        if self.reject_progress.load(Ordering::Acquire) {
            return Err(GraphComputationControlError::Unavailable);
        }
        self.progress
            .lock()
            .map_err(|_| GraphComputationControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

struct ParsedFixture {
    snapshot: Snapshot,
    files: RepositoryFileState,
    parses: Vec<LanguageParseResult>,
}

#[test]
fn mixed_fixture_links_deterministically_and_matches_the_v1_golden() -> Result<(), Box<dyn Error>> {
    let fixture = parsed_fixture()?;
    let link_control = RecordingGraphControl::default();
    let linker = DeterministicGraphLinker;
    let graph = linker.link(
        GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
        GraphLinkPolicy::v1(),
        &link_control,
    )?;
    let rank_control = RecordingGraphControl::default();
    let rank = DeterministicGraphRanker.rank(&graph, RankingPolicy::v1(), &rank_control)?;

    let mut reversed = fixture.parses.clone();
    reversed.reverse();
    let repeated = linker.link(
        GraphLinkInput::new(&fixture.snapshot, &fixture.files, &reversed),
        GraphLinkPolicy::v1(),
        &RecordingGraphControl::default(),
    )?;
    assert_eq!(graph, repeated);
    assert_eq!(
        rank,
        DeterministicGraphRanker.rank(
            &repeated,
            RankingPolicy::v1(),
            &RecordingGraphControl::default(),
        )?
    );

    for observations in [link_control.observations()?, rank_control.observations()?] {
        assert!(observations.len() <= 64);
        assert!(
            observations
                .last()
                .is_some_and(|progress| progress.is_complete())
        );
        for pair in observations.windows(2) {
            pair[1].validate_after(pair[0])?;
        }
    }

    let golden = normalized_graph(&graph, &rank)?;
    assert_eq!(
        golden,
        include_str!("../../../fixtures/graph-linker/graph-v1.golden")
    );
    Ok(())
}

#[test]
fn linker_resolves_only_evidence_supported_targets_and_keeps_dynamic_calls_unresolved()
-> Result<(), Box<dyn Error>> {
    let fixture = parsed_fixture()?;
    let graph = DeterministicGraphLinker.link(
        GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
        GraphLinkPolicy::v1(),
        &RecordingGraphControl::default(),
    )?;

    assert!(has_file_edge(
        &graph,
        "src/lib.rs",
        "src/service.rs",
        SyntaxRelationKind::Imports,
    ));
    assert!(has_file_edge(
        &graph,
        "web/main.ts",
        "web/helper.ts",
        SyntaxRelationKind::Imports,
    ));
    assert!(has_symbol_edge(
        &graph,
        "python/sample/service.py",
        "BaseService",
        SyntaxRelationKind::Imports,
    ));
    assert!(has_symbol_edge(
        &graph,
        "python/pyproject.toml",
        "main",
        SyntaxRelationKind::Configures,
    ));
    assert!(graph.unresolved().iter().any(|candidate| {
        candidate.kind() == SyntaxRelationKind::Calls
            && candidate.reason() == UnresolvedReason::DynamicReference
    }));
    assert!(graph.unresolved().iter().any(|candidate| {
        candidate.kind() == SyntaxRelationKind::Imports
            && matches!(
                candidate.target(),
                UnresolvedGraphTarget::Reference(reference)
                    if reference.as_str() == "external-package"
            )
    }));
    for edge in graph.edges() {
        assert_eq!(edge.snapshot_id(), fixture.snapshot.id());
        let source_length = FIXTURE_FILES
            .iter()
            .find(|file| file.path == path_text(edge.evidence().revision().path()))
            .map(|file| file.source.len())
            .ok_or("edge evidence path is absent from fixture")?;
        assert!(fixture.files.revisions().iter().any(|revision| {
            revision == edge.evidence().revision()
                && usize::try_from(edge.evidence().range().end_byte())
                    .is_ok_and(|end| end <= source_length)
        }));
    }
    Ok(())
}

#[test]
fn rank_is_parse_independent_versioned_and_explainable() -> Result<(), Box<dyn Error>> {
    let fixture = parsed_fixture()?;
    let graph = DeterministicGraphLinker.link(
        GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
        GraphLinkPolicy::v1(),
        &RecordingGraphControl::default(),
    )?;
    drop(fixture.parses);

    let projection = DeterministicGraphRanker.rank(
        &graph,
        RankingPolicy::v1(),
        &RecordingGraphControl::default(),
    )?;
    assert_eq!(projection.policy_version(), RankingPolicy::v1().version());
    let main = graph
        .symbols()
        .iter()
        .find(|symbol| {
            path_text(symbol.revision().path()) == "src/main.rs"
                && symbol.parsed().roles().contains(SymbolRole::Entrypoint)
        })
        .ok_or("Rust entrypoint symbol missing")?;
    let main_rank = projection
        .symbols()
        .iter()
        .find(|rank| rank.symbol_id() == main.id())
        .ok_or("Rust entrypoint rank missing")?;
    assert!(main_rank.signals().entrypoint_contribution > 0);
    assert!(projection.symbols().windows(2).all(|pair| {
        pair[0].score() > pair[1].score()
            || (pair[0].score() == pair[1].score() && pair[0].symbol_id() < pair[1].symbol_id())
    }));
    Ok(())
}

#[test]
fn stale_input_cancellation_and_progress_failures_are_visible() -> Result<(), Box<dyn Error>> {
    let fixture = parsed_fixture()?;
    let linker = DeterministicGraphLinker;
    assert_eq!(
        linker.link(
            GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
            GraphLinkPolicy::v1(),
            &RecordingGraphControl::cancelled(),
        ),
        Err(GraphLinkFailure::Cancelled)
    );
    assert_eq!(
        linker.link(
            GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
            GraphLinkPolicy::v1(),
            &RecordingGraphControl::rejecting_progress(),
        ),
        Err(GraphLinkFailure::ProgressUnavailable)
    );

    let stale_files = RepositoryFileState::new(
        fixture
            .files
            .revisions()
            .iter()
            .map(|revision| {
                if path_text(revision.path()) == "src/lib.rs" {
                    FileRevision::new(revision.path().clone(), ContentHash::from_bytes([9; 32]))
                } else {
                    revision.clone()
                }
            })
            .collect(),
    )?;
    assert_eq!(
        linker.link(
            GraphLinkInput::new(&fixture.snapshot, &stale_files, &fixture.parses),
            GraphLinkPolicy::v1(),
            &RecordingGraphControl::default(),
        ),
        Err(GraphLinkFailure::InvalidInput)
    );

    let mut incompatible_revisions = fixture.snapshot.adapter_revisions().to_vec();
    for revision in &mut incompatible_revisions {
        if revision.language() == IndexLanguage::Python {
            *revision = LanguageAdapterRevision::new(
                IndexLanguage::Python,
                LanguageAdapterVersion::try_from_string("incompatible-python-v2".to_owned())?,
            );
        }
    }
    let incompatible_snapshot = Snapshot::new(
        fixture.snapshot.id(),
        fixture.snapshot.worktree_id(),
        fixture.snapshot.parent_id(),
        fixture.snapshot.generation(),
        fixture.snapshot.head().clone(),
        fixture.snapshot.index_schema_version(),
        incompatible_revisions,
        fixture.snapshot.changes().to_vec(),
    )?;
    assert_eq!(
        linker.link(
            GraphLinkInput::new(&incompatible_snapshot, &fixture.files, &fixture.parses),
            GraphLinkPolicy::v1(),
            &RecordingGraphControl::default(),
        ),
        Err(GraphLinkFailure::InvalidInput)
    );

    let graph = linker.link(
        GraphLinkInput::new(&fixture.snapshot, &fixture.files, &fixture.parses),
        GraphLinkPolicy::v1(),
        &RecordingGraphControl::default(),
    )?;
    assert_eq!(
        DeterministicGraphRanker.rank(
            &graph,
            RankingPolicy::v1(),
            &RecordingGraphControl::cancelled(),
        ),
        Err(GraphRankFailure::Cancelled)
    );
    assert_eq!(
        DeterministicGraphRanker.rank(
            &graph,
            RankingPolicy::v1(),
            &RecordingGraphControl::rejecting_progress(),
        ),
        Err(GraphRankFailure::ProgressUnavailable)
    );
    Ok(())
}

fn parsed_fixture() -> Result<ParsedFixture, Box<dyn Error>> {
    let rust = RustLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let typescript = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let python = PythonLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let adapters: [&dyn LanguageAdapter; 3] = [&rust, &typescript, &python];
    let mut revisions = Vec::new();
    let mut parses = Vec::new();
    for file in FIXTURE_FILES {
        let path = RepositoryPath::try_from_bytes(file.path.as_bytes().to_vec())?;
        let revision = FileRevision::new(
            path.clone(),
            ContentHash::from_bytes(*blake3::hash(file.source).as_bytes()),
        );
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.supports_path(&path))
            .ok_or("fixture path has no structural adapter")?;
        parses.push(adapter.parse(
            LanguageParseInput::new(&revision, file.source, file.roles),
            LanguageParsePolicy::v1(),
            &SilentParseControl,
        )?);
        revisions.push(revision);
    }
    let files = RepositoryFileState::new(revisions.clone())?;
    let changes = revisions
        .iter()
        .map(|revision| {
            SnapshotChange::new(
                revision.path().clone(),
                revision.content_hash(),
                SnapshotChangeKind::Upsert,
            )
        })
        .collect();
    let snapshot = Snapshot::new(
        SnapshotId::from_bytes([0x42; 32]),
        WorktreeId::from_bytes([0x24; 32]),
        None,
        WorktreeGeneration::new(1)?,
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
        IndexSchemaVersion::new(1)?,
        adapters
            .iter()
            .map(|adapter| adapter.revision().clone())
            .collect(),
        changes,
    )?;
    Ok(ParsedFixture {
        snapshot,
        files,
        parses,
    })
}

fn normalized_graph(
    graph: &a3_domain::LinkedGraph,
    rank: &a3_domain::RankProjection,
) -> Result<String, std::fmt::Error> {
    let labels = graph
        .symbols()
        .iter()
        .map(|symbol| {
            (
                symbol.id(),
                format!(
                    "{}::{}#{}",
                    path_text(symbol.revision().path()),
                    symbol.parsed().name().as_str(),
                    symbol.parsed().id().get()
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut symbols = String::new();
    for symbol in graph.symbols() {
        writeln!(
            symbols,
            "symbol id={} label={} kind={:?} visibility={:?} test={} entry={}",
            symbol.id(),
            labels.get(&symbol.id()).map_or("?", String::as_str),
            symbol.parsed().kind(),
            symbol.parsed().visibility(),
            symbol.parsed().roles().contains(SymbolRole::Test),
            symbol.parsed().roles().contains(SymbolRole::Entrypoint),
        )?;
    }
    let mut edges = String::new();
    let mut edge_kinds = BTreeMap::new();
    let mut resolutions = BTreeMap::new();
    for edge in graph.edges() {
        *edge_kinds.entry(edge.kind()).or_insert(0usize) += 1;
        *resolutions.entry(edge.resolution()).or_insert(0usize) += 1;
        writeln!(
            edges,
            "edge kind={:?} source={} target={} provider={:?} confidence={} resolution={:?} evidence={}:{}..{}",
            edge.kind(),
            endpoint_label(edge.source(), &labels),
            endpoint_label(edge.target(), &labels),
            edge.provider(),
            edge.confidence().basis_points(),
            edge.resolution(),
            path_text(edge.evidence().revision().path()),
            edge.evidence().range().start_byte(),
            edge.evidence().range().end_byte(),
        )?;
    }
    let mut candidates = String::new();
    let mut candidate_kinds = BTreeMap::new();
    let mut reasons = BTreeMap::new();
    for candidate in graph.unresolved() {
        *candidate_kinds.entry(candidate.kind()).or_insert(0usize) += 1;
        *reasons.entry(candidate.reason()).or_insert(0usize) += 1;
        writeln!(
            candidates,
            "candidate kind={:?} source={} target={} provider={:?} confidence={} reason={:?} evidence={}:{}..{}",
            candidate.kind(),
            endpoint_label(candidate.source(), &labels),
            unresolved_label(candidate.target()),
            candidate.provider(),
            candidate.confidence().basis_points(),
            candidate.reason(),
            path_text(candidate.evidence().revision().path()),
            candidate.evidence().range().start_byte(),
            candidate.evidence().range().end_byte(),
        )?;
    }
    let mut ranks = String::new();
    for item in rank.symbols() {
        let signals = item.signals();
        writeln!(
            ranks,
            "rank label={} score={} in={} out={} centrality={} entry={} export={} manifest={} test={}",
            labels.get(&item.symbol_id()).map_or("?", String::as_str),
            item.score().get(),
            signals.in_degree,
            signals.out_degree,
            signals.centrality.basis_points(),
            signals.entrypoint_contribution,
            signals.public_export_contribution,
            signals.manifest_contribution,
            signals.test_contribution,
        )?;
    }

    let mut output = String::new();
    writeln!(
        output,
        "snapshot={} files={} symbols={} edges={} unresolved={} ranking={}",
        graph.snapshot_id(),
        graph.files().len(),
        graph.symbols().len(),
        graph.edges().len(),
        graph.unresolved().len(),
        rank.policy_version().get(),
    )?;
    writeln!(output, "symbol_digest={}", blake3::hash(symbols.as_bytes()))?;
    writeln!(output, "edge_digest={}", blake3::hash(edges.as_bytes()))?;
    writeln!(
        output,
        "candidate_digest={}",
        blake3::hash(candidates.as_bytes())
    )?;
    writeln!(output, "rank_digest={}", blake3::hash(ranks.as_bytes()))?;
    for (kind, count) in edge_kinds {
        writeln!(output, "edge_kind {kind:?}={count}")?;
    }
    for (resolution, count) in resolutions {
        writeln!(output, "resolution {resolution:?}={count}")?;
    }
    for (kind, count) in candidate_kinds {
        writeln!(output, "candidate_kind {kind:?}={count}")?;
    }
    for (reason, count) in reasons {
        writeln!(output, "candidate_reason {reason:?}={count}")?;
    }
    for item in rank.symbols().iter().take(10) {
        let signals = item.signals();
        writeln!(
            output,
            "top label={} score={} centrality={} entry={} export={} manifest={} test={}",
            labels.get(&item.symbol_id()).map_or("?", String::as_str),
            item.score().get(),
            signals.centrality.basis_points(),
            signals.entrypoint_contribution,
            signals.public_export_contribution,
            signals.manifest_contribution,
            signals.test_contribution,
        )?;
    }
    Ok(output)
}

fn endpoint_label(endpoint: &GraphEndpoint, labels: &BTreeMap<SymbolId, String>) -> String {
    match endpoint {
        GraphEndpoint::File(path) => format!("file:{}", path_text(path)),
        GraphEndpoint::Symbol(id) => labels
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("missing:{id}")),
    }
}

fn unresolved_label(target: &UnresolvedGraphTarget) -> String {
    match target {
        UnresolvedGraphTarget::File(path) => format!("file:{}", path_text(path)),
        UnresolvedGraphTarget::Reference(reference) => {
            format!("reference:{}", reference.as_str())
        }
    }
}

fn has_file_edge(
    graph: &a3_domain::LinkedGraph,
    source_path: &str,
    target_path: &str,
    kind: SyntaxRelationKind,
) -> bool {
    graph.edges().iter().any(|edge| {
        edge.kind() == kind
            && endpoint_path(graph, edge.source()) == Some(source_path)
            && matches!(
                edge.target(),
                GraphEndpoint::File(path) if path_text(path) == target_path
            )
    })
}

fn has_symbol_edge(
    graph: &a3_domain::LinkedGraph,
    source_path: &str,
    target_name: &str,
    kind: SyntaxRelationKind,
) -> bool {
    graph.edges().iter().any(|edge| {
        edge.kind() == kind
            && endpoint_path(graph, edge.source()) == Some(source_path)
            && matches!(
                edge.target(),
                GraphEndpoint::Symbol(id)
                    if graph.symbols().iter().any(|symbol| {
                        symbol.id() == *id && symbol.parsed().name().as_str() == target_name
                    })
            )
    })
}

fn endpoint_path<'a>(
    graph: &'a a3_domain::LinkedGraph,
    endpoint: &'a GraphEndpoint,
) -> Option<&'a str> {
    match endpoint {
        GraphEndpoint::File(path) => std::str::from_utf8(path.as_bytes()).ok(),
        GraphEndpoint::Symbol(id) => graph
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == *id)
            .and_then(|symbol| std::str::from_utf8(symbol.revision().path().as_bytes()).ok()),
    }
}

fn path_text(path: &RepositoryPath) -> &str {
    std::str::from_utf8(path.as_bytes()).unwrap_or("<non-utf8>")
}
