//! Deterministic module-formation contract fixtures.

use a3_domain::{
    Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint, GraphSymbol,
    IndexLanguage, LinkResolution, LinkedGraph, LocalSymbolId, ModuleKind, ModuleRoot, Progress,
    RankProjection, RankScore, RankingPolicyVersion, RepositoryPath, SnapshotId, SourcePosition,
    SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SymbolRole,
    SyntaxProvider, SyntaxRelationKind,
};
use a3_repo_index::{
    DeterministicModuleFormer, GraphComputationControl, GraphComputationControlError,
    ModuleFormationFailure, ModuleFormationInput, ModuleFormationPolicy,
};
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct RecordingControl {
    cancelled: AtomicBool,
    reports: AtomicUsize,
}

#[derive(Debug)]
struct RejectingProgressControl;

impl GraphComputationControl for RejectingProgressControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), GraphComputationControlError> {
        Err(GraphComputationControlError::Unavailable)
    }
}

impl GraphComputationControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), GraphComputationControlError> {
        self.reports.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn monorepo_boundaries_communities_and_repository_card_are_deterministic()
-> Result<(), Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([1; 32]);
    let cargo = revision("Cargo.toml", 2)?;
    let package_a = revision("packages/a/package.json", 3)?;
    let package_b = revision("packages/b/package.json", 4)?;
    let a_main = revision("packages/a/src/main.ts", 5)?;
    let a_worker = revision("packages/a/src/worker.ts", 6)?;
    let b_test = revision("packages/b/src/worker.test.ts", 7)?;
    let root_tool = revision("tools/check.rs", 8)?;

    let a_main_symbol = symbol(10, a_main.clone(), "main", Some(SymbolRole::Entrypoint))?;
    let a_worker_symbol = symbol(11, a_worker.clone(), "worker", None)?;
    let b_test_symbol = symbol(12, b_test.clone(), "worker_test", Some(SymbolRole::Test))?;
    let root_tool_symbol = symbol(13, root_tool.clone(), "check", None)?;
    let symbols = vec![
        a_main_symbol.clone(),
        a_worker_symbol.clone(),
        b_test_symbol.clone(),
        root_tool_symbol.clone(),
    ];
    let edges = vec![
        call_edge(snapshot_id, &a_worker_symbol, &b_test_symbol)?,
        call_edge(snapshot_id, &b_test_symbol, &a_worker_symbol)?,
    ];
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![
            cargo.clone(),
            package_a.clone(),
            package_b.clone(),
            a_main,
            a_worker,
            b_test,
            root_tool,
        ],
        symbols.clone(),
        edges,
        Vec::new(),
    )?;
    let ranking = ranking(snapshot_id, &symbols)?;
    let manifests = vec![cargo, package_a, package_b];
    let control = RecordingControl::default();
    let input = ModuleFormationInput::new(
        &graph,
        &ranking,
        &manifests,
        &[IndexLanguage::Rust, IndexLanguage::TypeScriptJavaScript],
    );

    let first = DeterministicModuleFormer.form(input, ModuleFormationPolicy::v1(), &control)?;
    let second = DeterministicModuleFormer.form(input, ModuleFormationPolicy::v1(), &control)?;
    assert_eq!(first, second);
    assert_eq!(control.reports.load(Ordering::Relaxed), 12);

    let primary = first
        .modules()
        .iter()
        .filter(|module| module.kind().is_primary())
        .collect::<Vec<_>>();
    let package_a_root =
        ModuleRoot::Directory(RepositoryPath::try_from_bytes(b"packages/a".to_vec())?);
    let package_b_root =
        ModuleRoot::Directory(RepositoryPath::try_from_bytes(b"packages/b".to_vec())?);
    assert_eq!(primary.len(), 3);
    assert!(primary.iter().any(|module| {
        module.root() == Some(&ModuleRoot::Repository)
            && module
                .manifests()
                .iter()
                .any(|item| item.path().as_bytes() == b"Cargo.toml")
    }));
    assert!(
        primary
            .iter()
            .any(|module| module.root() == Some(&package_a_root))
    );
    assert!(
        primary
            .iter()
            .any(|module| module.root() == Some(&package_b_root))
    );

    let community = first
        .modules()
        .iter()
        .find(|module| module.kind() == ModuleKind::GraphCommunity)
        .ok_or("graph community missing")?;
    let community_members = first
        .memberships()
        .iter()
        .filter(|membership| membership.module_id() == community.id())
        .collect::<Vec<_>>();
    assert_eq!(community_members.len(), 2);
    assert!(community_members.iter().all(|membership| {
        !membership.evidence().relationships().is_empty()
            && !membership.evidence().kind().is_primary()
    }));

    for symbol in &symbols {
        let memberships = first
            .memberships()
            .iter()
            .filter(|membership| membership.symbol_id() == symbol.id())
            .collect::<Vec<_>>();
        assert_eq!(
            memberships
                .iter()
                .filter(|membership| membership.evidence().kind().is_primary())
                .count(),
            1
        );
    }
    assert_eq!(first.repository_card().packages().len(), 3);
    assert_eq!(
        first.repository_card().languages(),
        &[IndexLanguage::Rust, IndexLanguage::TypeScriptJavaScript]
    );
    assert_eq!(
        first.repository_card().entrypoints().symbols(),
        &[a_main_symbol.id()]
    );
    assert!(
        primary
            .iter()
            .any(|module| { module.entrypoints().symbols() == [a_main_symbol.id()] })
    );
    assert!(
        primary
            .iter()
            .any(|module| { module.tests().symbols() == [b_test_symbol.id()] })
    );
    Ok(())
}

#[test]
fn cancellation_prevents_module_formation() -> Result<(), Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([20; 32]);
    let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let control = RecordingControl::default();
    control.cancelled.store(true, Ordering::Relaxed);
    assert_eq!(
        DeterministicModuleFormer.form(
            ModuleFormationInput::new(&graph, &ranking, &[], &[]),
            ModuleFormationPolicy::v1(),
            &control,
        ),
        Err(ModuleFormationFailure::Cancelled)
    );
    Ok(())
}

#[test]
fn repositories_without_manifests_use_distinct_top_level_path_boundaries()
-> Result<(), Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([30; 32]);
    let app = symbol(31, revision("apps/api.py", 32)?, "api", None)?;
    let library = symbol(33, revision("libs/core.py", 34)?, "core", None)?;
    let symbols = vec![app, library];
    let files = symbols
        .iter()
        .map(|symbol| symbol.revision().clone())
        .collect();
    let graph = LinkedGraph::new(snapshot_id, files, symbols.clone(), Vec::new(), Vec::new())?;
    let ranking = ranking(snapshot_id, &symbols)?;
    let projection = DeterministicModuleFormer.form(
        ModuleFormationInput::new(&graph, &ranking, &[], &[IndexLanguage::Python]),
        ModuleFormationPolicy::v1(),
        &RecordingControl::default(),
    )?;

    let roots = projection
        .modules()
        .iter()
        .map(|module| module.root().cloned())
        .collect::<Vec<_>>();
    assert!(roots.contains(&Some(ModuleRoot::Directory(
        RepositoryPath::try_from_bytes(b"apps".to_vec())?
    ))));
    assert!(roots.contains(&Some(ModuleRoot::Directory(
        RepositoryPath::try_from_bytes(b"libs".to_vec())?
    ))));
    assert_eq!(projection.memberships().len(), 2);
    assert!(
        projection
            .memberships()
            .iter()
            .all(|membership| membership.evidence().kind().is_primary())
    );
    Ok(())
}

#[test]
fn progress_failure_is_typed_before_any_projection_is_returned() -> Result<(), Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([40; 32]);
    let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    assert_eq!(
        DeterministicModuleFormer.form(
            ModuleFormationInput::new(&graph, &ranking, &[], &[]),
            ModuleFormationPolicy::v1(),
            &RejectingProgressControl,
        ),
        Err(ModuleFormationFailure::ProgressUnavailable)
    );
    Ok(())
}

fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}

fn symbol(
    id: u8,
    revision: FileRevision,
    name: &str,
    role: Option<SymbolRole>,
) -> Result<GraphSymbol, Box<dyn Error>> {
    let range = range()?;
    let mut parsed = a3_domain::ParsedSymbol::new(
        LocalSymbolId::new(u32::from(id))?,
        SymbolKind::Function,
        SymbolName::try_from_string(name.to_owned())?,
        range,
        range,
    )?;
    if let Some(role) = role {
        parsed = parsed.with_role(role);
    }
    Ok(GraphSymbol::new(
        SymbolId::from_bytes([id; 32]),
        revision,
        parsed,
    ))
}

fn call_edge(
    snapshot_id: SnapshotId,
    source: &GraphSymbol,
    target: &GraphSymbol,
) -> Result<GraphEdge, Box<dyn Error>> {
    Ok(GraphEdge::new(
        GraphEndpoint::Symbol(source.id()),
        GraphEndpoint::Symbol(target.id()),
        SyntaxRelationKind::Calls,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        EvidenceRef::new(source.revision().clone(), range()?),
    ))
}

fn ranking(
    snapshot_id: SnapshotId,
    symbols: &[GraphSymbol],
) -> Result<RankProjection, Box<dyn Error>> {
    let rows = symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            let score = u64::try_from(symbols.len().saturating_sub(index))?;
            Ok(SymbolRank::new(
                symbol.id(),
                RankScore::try_from_sum(score)?,
                SymbolRankSignals {
                    in_degree: 0,
                    out_degree: 0,
                    centrality: a3_domain::Centrality::from_basis_points(0)?,
                    degree_contribution: 0,
                    centrality_contribution: 0,
                    entrypoint_contribution: 0,
                    public_export_contribution: 0,
                    manifest_contribution: 0,
                    test_contribution: 0,
                },
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        rows,
    )?)
}

fn range() -> Result<SourceRange, Box<dyn Error>> {
    Ok(SourceRange::new(
        0,
        1,
        SourcePosition::new(0, 0),
        SourcePosition::new(0, 1),
    )?)
}
