use crate::fixture::{ContractWorkspace, change, project, run, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory, fixture_modules};
use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore,
    KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore,
};
use a3_domain::{
    Centrality, Confidence, ContentHash, EvidenceRef, ExactSearchExplanation, ExactSearchPageSize,
    ExactSearchQuery, ExactSearchRole, ExactSearchTarget, ExactSearchTerm, FileRevision, GraphEdge,
    GraphEndpoint, GraphSymbol, IndexPublication, LexicalSearchExplanation, LexicalSearchPageSize,
    LexicalSearchQuery, LexicalSearchTarget, LexicalSearchTerm, LinkResolution, LinkedGraph,
    LocalSymbolId, ParsedSymbol, RankProjection, RankScore, RankingPolicyVersion, RepositoryId,
    RepositoryPath, SnapshotChangeKind, SnapshotId, SourceChannel, SourcePosition, SourceRange,
    SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SymbolRole, SymbolSignature,
    SyntaxProvider, SyntaxRelationKind, TraversalDepth, TraversalDirection, TraversalQuery,
    TraversalResultLimit, WorktreeId,
};

#[derive(Debug)]
struct SearchControl;

impl KnowledgeSearchControl for SearchControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct CancelledSearchControl;

impl KnowledgeSearchControl for CancelledSearchControl {
    fn is_cancelled(&self) -> bool {
        true
    }
}

impl IndexPersistenceControl for SearchControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let control = SearchControl;
    let app_data = workspace.app_data_root("search");
    let common = workspace.create_directory("search-common")?;
    let root = workspace.create_directory("search-primary")?;
    let worktree_id = WorktreeId::from_bytes([81; 32]);
    let project = project(
        RepositoryId::from_bytes([8; 32]),
        worktree_id,
        &common,
        &root,
        unborn_head()?,
    )?;
    let store = factory.open(&app_data).await?;
    let launch_query =
        ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("launch".to_owned())?);
    let typo_query =
        LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("launcj".to_owned())?);
    let first_root_id = SymbolId::from_bytes([10; 32]);
    let call_graph_query = TraversalQuery::new(
        GraphEndpoint::Symbol(first_root_id),
        TraversalDirection::Outgoing,
        SyntaxRelationKind::Calls,
        TraversalDepth::INTERACTIVE_MAX,
        TraversalResultLimit::DEFAULT,
    );
    assert_eq!(
        store
            .search_exact(
                &project,
                &launch_query,
                ExactSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::IndexUnavailable)
    );
    assert_eq!(
        store
            .search_lexical(
                &project,
                &typo_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::IndexUnavailable)
    );
    assert_eq!(
        store
            .traverse_graph(&project, &call_graph_query, &control)
            .await,
        Err(KnowledgeSearchFailure::IndexUnavailable)
    );

    let first_snapshot = snapshot(
        [82; 32],
        worktree_id,
        None,
        1,
        vec![
            change(b"Cargo.toml", [1; 32], SnapshotChangeKind::Upsert)?,
            change(b"src/lib.rs", [2; 32], SnapshotChangeKind::Upsert)?,
            change(b"obsolete.rs", [4; 32], SnapshotChangeKind::Upsert)?,
        ],
    )?;
    store.append_snapshot(&project, &first_snapshot).await?;
    let first_publication = publication(first_snapshot.id(), [2; 32], 10, true)?;
    let first_run = store
        .start_index_run(&project, run([83; 32], first_snapshot.id(), 1)?)
        .await?;
    let first_run = store
        .publish_index(&project, first_run.id(), &first_publication, &control)
        .await?;
    assert_eq!(
        store
            .search_exact(
                &project,
                &launch_query,
                ExactSearchPageSize::DEFAULT,
                None,
                &CancelledSearchControl,
            )
            .await,
        Err(KnowledgeSearchFailure::Cancelled)
    );
    assert_eq!(
        store
            .search_lexical(
                &project,
                &typo_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &CancelledSearchControl,
            )
            .await,
        Err(KnowledgeSearchFailure::Cancelled)
    );
    assert_eq!(
        store
            .traverse_graph(&project, &call_graph_query, &CancelledSearchControl)
            .await,
        Err(KnowledgeSearchFailure::Cancelled)
    );

    let one = ExactSearchPageSize::new(1)?;
    let first_page = store
        .search_exact(&project, &launch_query, one, None, &control)
        .await?;
    assert_eq!(first_page.index_run_id(), first_run.id());
    assert_eq!(first_page.snapshot_id(), first_snapshot.id());
    assert_eq!(first_page.hits().len(), 1);
    assert_eq!(
        first_page.hits()[0].explanation(),
        ExactSearchExplanation::QualifiedNameExact
    );
    assert_symbol_hit(&first_page.hits()[0], "launch", [2; 32])?;
    let first_cursor = first_page
        .next_cursor()
        .cloned()
        .ok_or("first exact-search page has no continuation")?;

    let repeated = store
        .search_exact(&project, &launch_query, one, None, &control)
        .await?;
    assert_eq!(repeated, first_page);
    let second_page = store
        .search_exact(&project, &launch_query, one, Some(&first_cursor), &control)
        .await?;
    assert_eq!(second_page.hits().len(), 1);
    assert_eq!(
        second_page.hits()[0].explanation(),
        ExactSearchExplanation::SymbolNameExact
    );
    assert_symbol_hit(&second_page.hits()[0], "module::launch", [2; 32])?;
    assert!(second_page.next_cursor().is_none());

    assert_single_symbol(
        &store,
        &project,
        ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(
            "module::launch".to_owned(),
        )?),
        "module::launch",
        ExactSearchExplanation::QualifiedNameExact,
        &control,
    )
    .await?;
    assert_single_symbol(
        &store,
        &project,
        ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(
            "fn nested_launch()".to_owned(),
        )?),
        "module::launch",
        ExactSearchExplanation::SignatureExact,
        &control,
    )
    .await?;

    let path_query =
        ExactSearchQuery::Path(RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?);
    let path_page = store
        .search_exact(
            &project,
            &path_query,
            ExactSearchPageSize::DEFAULT,
            None,
            &control,
        )
        .await?;
    assert_file_hit(&path_page, b"src/lib.rs", [2; 32])?;
    let manifest_page = store
        .search_exact(
            &project,
            &ExactSearchQuery::Role(ExactSearchRole::Manifest),
            ExactSearchPageSize::DEFAULT,
            None,
            &control,
        )
        .await?;
    assert_file_hit(&manifest_page, b"Cargo.toml", [1; 32])?;
    assert_single_role_symbol(
        &store,
        &project,
        ExactSearchRole::Entrypoint,
        "launch",
        &control,
    )
    .await?;
    assert_single_role_symbol(
        &store,
        &project,
        ExactSearchRole::Test,
        "module::launch",
        &control,
    )
    .await?;
    let injection = ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(
        "launch' OR 1=1 --".to_owned(),
    )?);
    assert!(
        store
            .search_exact(
                &project,
                &injection,
                ExactSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await?
            .hits()
            .is_empty()
    );

    let lexical_one = LexicalSearchPageSize::new(1)?;
    let lexical_first = store
        .search_lexical(&project, &typo_query, lexical_one, None, &control)
        .await?;
    assert_eq!(lexical_first.index_run_id(), first_run.id());
    assert_eq!(lexical_first.snapshot_id(), first_snapshot.id());
    assert_eq!(lexical_first.hits().len(), 1);
    assert_eq!(
        lexical_first.hits()[0].source_channel(),
        SourceChannel::Lexical
    );
    assert_eq!(
        lexical_first.hits()[0].explanation(),
        LexicalSearchExplanation::SymbolName
    );
    assert_eq!(lexical_first.hits()[0].score().get(), 75_000);
    let lexical_cursor = lexical_first
        .next_cursor()
        .cloned()
        .ok_or("first lexical-search page has no continuation")?;
    assert_eq!(
        store
            .search_lexical(&project, &typo_query, lexical_one, None, &control)
            .await?,
        lexical_first
    );
    let lexical_second = store
        .search_lexical(
            &project,
            &typo_query,
            lexical_one,
            Some(&lexical_cursor),
            &control,
        )
        .await?;
    assert_eq!(lexical_second.hits().len(), 1);
    assert!(lexical_second.next_cursor().is_none());
    let signature_page = store
        .search_lexical(
            &project,
            &LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("nested".to_owned())?),
            LexicalSearchPageSize::DEFAULT,
            None,
            &control,
        )
        .await?;
    assert_eq!(signature_page.hits().len(), 1);
    assert_eq!(
        signature_page.hits()[0].explanation(),
        LexicalSearchExplanation::Signature
    );
    let injection = LexicalSearchQuery::new(LexicalSearchTerm::try_from_string(
        "zzzinjection' OR 1=1 --".to_owned(),
    )?);
    assert!(
        store
            .search_lexical(
                &project,
                &injection,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await?
            .hits()
            .is_empty()
    );
    let obsolete_query =
        LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("obsolete".to_owned())?);
    let obsolete = store
        .search_lexical(
            &project,
            &obsolete_query,
            LexicalSearchPageSize::DEFAULT,
            None,
            &control,
        )
        .await?;
    assert!(obsolete.hits().iter().any(|hit| matches!(
        hit.target(),
        LexicalSearchTarget::File(revision)
            if revision.path().as_bytes() == b"obsolete.rs"
    )));
    let removed_symbol_query = LexicalSearchQuery::new(LexicalSearchTerm::try_from_string(
        "removed_symbol".to_owned(),
    )?);
    assert!(
        store
            .search_lexical(
                &project,
                &removed_symbol_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await?
            .hits()
            .iter()
            .any(
                |hit| matches!(hit.target(), LexicalSearchTarget::Symbol(symbol)
            if symbol.symbol().parsed().name().as_str() == "removed_symbol")
            )
    );

    let graph_result = store
        .traverse_graph(&project, &call_graph_query, &control)
        .await?;
    assert_eq!(graph_result.index_run_id(), first_run.id());
    assert_eq!(graph_result.snapshot_id(), first_snapshot.id());
    assert_eq!(graph_result.hits().len(), 2);
    assert!(!graph_result.truncated());
    assert_eq!(
        store
            .traverse_graph(&project, &call_graph_query, &control)
            .await?,
        graph_result
    );
    let module_id = SymbolId::from_bytes([11; 32]);
    let nested_id = SymbolId::from_bytes([12; 32]);
    let mut graph_targets = graph_result
        .hits()
        .iter()
        .map(|hit| (target_endpoint(hit.target()), hit.path().len()))
        .collect::<Vec<_>>();
    graph_targets.sort();
    assert_eq!(
        graph_targets,
        vec![
            (GraphEndpoint::Symbol(module_id), 1),
            (GraphEndpoint::Symbol(nested_id), 1),
        ]
    );
    for hit in graph_result.hits() {
        assert_eq!(hit.source_channel(), SourceChannel::Graph);
        assert!(hit.path().iter().all(|edge| {
            edge.kind() == SyntaxRelationKind::Calls
                && edge.snapshot_id() == first_snapshot.id()
                && edge.evidence().revision().content_hash() == ContentHash::from_bytes([2; 32])
        }));
    }

    assert_single_graph_target(
        &store
            .traverse_graph(
                &project,
                &TraversalQuery::callers(first_root_id, TraversalResultLimit::DEFAULT),
                &control,
            )
            .await?,
        GraphEndpoint::Symbol(nested_id),
        SourceChannel::Graph,
    )?;
    assert_single_graph_target(
        &store
            .traverse_graph(
                &project,
                &TraversalQuery::imports(
                    GraphEndpoint::File(RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?),
                    TraversalResultLimit::DEFAULT,
                ),
                &control,
            )
            .await?,
        GraphEndpoint::File(RepositoryPath::try_from_bytes(b"Cargo.toml".to_vec())?),
        SourceChannel::Graph,
    )?;
    assert_single_graph_target(
        &store
            .traverse_graph(
                &project,
                &TraversalQuery::exports(
                    GraphEndpoint::File(RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?),
                    TraversalResultLimit::DEFAULT,
                ),
                &control,
            )
            .await?,
        GraphEndpoint::Symbol(first_root_id),
        SourceChannel::Graph,
    )?;
    assert_single_graph_target(
        &store
            .traverse_graph(
                &project,
                &TraversalQuery::tests(
                    GraphEndpoint::Symbol(first_root_id),
                    TraversalResultLimit::DEFAULT,
                ),
                &control,
            )
            .await?,
        GraphEndpoint::Symbol(nested_id),
        SourceChannel::Test,
    )?;
    let limited_query = TraversalQuery::callees(first_root_id, TraversalResultLimit::new(1)?);
    let limited = store
        .traverse_graph(&project, &limited_query, &control)
        .await?;
    assert_eq!(limited.hits().len(), 1);
    assert!(limited.truncated());
    assert_eq!(
        store
            .traverse_graph(
                &project,
                &TraversalQuery::callees(
                    SymbolId::from_bytes([99; 32]),
                    TraversalResultLimit::DEFAULT,
                ),
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::SeedUnavailable)
    );

    let replacement_snapshot = snapshot(
        [84; 32],
        worktree_id,
        Some(first_snapshot.id()),
        2,
        vec![
            change(b"src/lib.rs", [3; 32], SnapshotChangeKind::Upsert)?,
            change(b"obsolete.rs", [4; 32], SnapshotChangeKind::Delete)?,
        ],
    )?;
    store
        .append_snapshot(&project, &replacement_snapshot)
        .await?;
    let replacement_publication = publication(replacement_snapshot.id(), [3; 32], 20, false)?;
    let replacement_run = store
        .start_index_run(&project, run([85; 32], replacement_snapshot.id(), 1)?)
        .await?;
    store
        .publish_index(
            &project,
            replacement_run.id(),
            &replacement_publication,
            &control,
        )
        .await?;
    assert_eq!(
        store
            .search_exact(&project, &launch_query, one, Some(&first_cursor), &control,)
            .await,
        Err(KnowledgeSearchFailure::InvalidCursor)
    );
    let current_path = store
        .search_exact(
            &project,
            &path_query,
            ExactSearchPageSize::DEFAULT,
            None,
            &control,
        )
        .await?;
    assert_file_hit(&current_path, b"src/lib.rs", [3; 32])?;
    assert_eq!(
        store
            .search_lexical(
                &project,
                &typo_query,
                lexical_one,
                Some(&lexical_cursor),
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::InvalidCursor)
    );
    assert!(
        store
            .search_lexical(
                &project,
                &obsolete_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await?
            .hits()
            .is_empty()
    );
    assert!(
        store
            .search_lexical(
                &project,
                &removed_symbol_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await?
            .hits()
            .is_empty()
    );
    assert_eq!(
        store
            .traverse_graph(&project, &call_graph_query, &control)
            .await,
        Err(KnowledgeSearchFailure::SeedUnavailable)
    );
    let replacement_root_id = SymbolId::from_bytes([20; 32]);
    let replacement_graph = store
        .traverse_graph(
            &project,
            &TraversalQuery::callees(replacement_root_id, TraversalResultLimit::DEFAULT),
            &control,
        )
        .await?;
    assert_eq!(replacement_graph.snapshot_id(), replacement_snapshot.id());
    assert_eq!(replacement_graph.hits().len(), 2);
    store.rebuild_regenerable_index(&project, &control).await?;
    assert_eq!(
        store
            .search_lexical(
                &project,
                &typo_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::IndexUnavailable)
    );
    assert_eq!(
        store
            .traverse_graph(
                &project,
                &TraversalQuery::callees(replacement_root_id, TraversalResultLimit::DEFAULT,),
                &control,
            )
            .await,
        Err(KnowledgeSearchFailure::IndexUnavailable)
    );
    Ok(())
}

fn assert_single_graph_target(
    result: &a3_domain::GraphTraversalResult,
    expected: GraphEndpoint,
    channel: SourceChannel,
) -> ContractResult<()> {
    assert_eq!(result.hits().len(), 1);
    let hit = &result.hits()[0];
    assert_eq!(target_endpoint(hit.target()), expected);
    assert_eq!(hit.source_channel(), channel);
    assert_eq!(hit.path().len(), 1);
    assert_eq!(hit.path()[0].kind(), result.query().relation());
    assert_eq!(hit.path()[0].snapshot_id(), result.snapshot_id());
    Ok(())
}

fn target_endpoint(target: &ExactSearchTarget) -> GraphEndpoint {
    match target {
        ExactSearchTarget::File(revision) => GraphEndpoint::File(revision.path().clone()),
        ExactSearchTarget::Symbol(symbol) => GraphEndpoint::Symbol(symbol.symbol().id()),
    }
}

async fn assert_single_symbol<S: KnowledgeSearchStore>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    query: ExactSearchQuery,
    qualified_name: &str,
    explanation: ExactSearchExplanation,
    control: &SearchControl,
) -> ContractResult<()> {
    let page = store
        .search_exact(project, &query, ExactSearchPageSize::DEFAULT, None, control)
        .await?;
    assert_eq!(page.hits().len(), 1);
    assert_eq!(page.hits()[0].explanation(), explanation);
    assert_symbol_hit(&page.hits()[0], qualified_name, [2; 32])
}

async fn assert_single_role_symbol<S: KnowledgeSearchStore>(
    store: &S,
    project: &a3_domain::ProjectIdentity,
    role: ExactSearchRole,
    qualified_name: &str,
    control: &SearchControl,
) -> ContractResult<()> {
    let page = store
        .search_exact(
            project,
            &ExactSearchQuery::Role(role),
            ExactSearchPageSize::DEFAULT,
            None,
            control,
        )
        .await?;
    assert_eq!(page.hits().len(), 1);
    assert_symbol_hit(&page.hits()[0], qualified_name, [2; 32])
}

fn assert_symbol_hit(
    hit: &a3_domain::ExactSearchHit,
    qualified_name: &str,
    expected_hash: [u8; 32],
) -> ContractResult<()> {
    let ExactSearchTarget::Symbol(symbol) = hit.target() else {
        return Err("expected a symbol search hit".into());
    };
    assert_eq!(symbol.qualified_name().as_str(), qualified_name);
    assert_eq!(
        symbol.symbol().revision().content_hash(),
        ContentHash::from_bytes(expected_hash)
    );
    Ok(())
}

fn assert_file_hit(
    page: &a3_domain::ExactSearchPage,
    path: &[u8],
    expected_hash: [u8; 32],
) -> ContractResult<()> {
    assert_eq!(page.hits().len(), 1);
    let ExactSearchTarget::File(revision) = page.hits()[0].target() else {
        return Err("expected a file search hit".into());
    };
    assert_eq!(revision.path().as_bytes(), path);
    assert_eq!(
        revision.content_hash(),
        ContentHash::from_bytes(expected_hash)
    );
    Ok(())
}

fn publication(
    snapshot_id: SnapshotId,
    source_hash: [u8; 32],
    id_base: u8,
    include_obsolete: bool,
) -> ContractResult<IndexPublication> {
    let manifest = FileRevision::new(
        RepositoryPath::try_from_bytes(b"Cargo.toml".to_vec())?,
        ContentHash::from_bytes([1; 32]),
    );
    let source = FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
        ContentHash::from_bytes(source_hash),
    );
    let range = SourceRange::new(0, 20, SourcePosition::new(0, 0), SourcePosition::new(0, 20))?;
    let root_id = SymbolId::from_bytes([id_base; 32]);
    let module_id = SymbolId::from_bytes([id_base + 1; 32]);
    let nested_id = SymbolId::from_bytes([id_base + 2; 32]);
    let obsolete_id = SymbolId::from_bytes([id_base + 3; 32]);
    let root = GraphSymbol::new(
        root_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("launch".to_owned())?,
            range,
            range,
        )?
        .with_signature(SymbolSignature::try_from_string("fn launch()".to_owned())?)
        .with_role(SymbolRole::Entrypoint),
    );
    let module = GraphSymbol::new(
        module_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(2)?,
            SymbolKind::Module,
            SymbolName::try_from_string("module".to_owned())?,
            range,
            range,
        )?,
    );
    let nested = GraphSymbol::new(
        nested_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(3)?,
            SymbolKind::Function,
            SymbolName::try_from_string("launch".to_owned())?,
            range,
            range,
        )?
        .with_signature(SymbolSignature::try_from_string(
            "fn nested_launch()".to_owned(),
        )?)
        .with_role(SymbolRole::Test),
    );
    let evidence = EvidenceRef::new(source.clone(), range);
    let edges = vec![
        edge(
            GraphEndpoint::File(source.path().clone()),
            GraphEndpoint::Symbol(root_id),
            SyntaxRelationKind::Defines,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::File(source.path().clone()),
            GraphEndpoint::Symbol(module_id),
            SyntaxRelationKind::Defines,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(module_id),
            GraphEndpoint::Symbol(nested_id),
            SyntaxRelationKind::Contains,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(root_id),
            GraphEndpoint::Symbol(module_id),
            SyntaxRelationKind::Calls,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(root_id),
            GraphEndpoint::Symbol(nested_id),
            SyntaxRelationKind::Calls,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(module_id),
            GraphEndpoint::Symbol(nested_id),
            SyntaxRelationKind::Calls,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(nested_id),
            GraphEndpoint::Symbol(root_id),
            SyntaxRelationKind::Calls,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::File(source.path().clone()),
            GraphEndpoint::File(manifest.path().clone()),
            SyntaxRelationKind::Imports,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::File(source.path().clone()),
            GraphEndpoint::Symbol(root_id),
            SyntaxRelationKind::Exports,
            snapshot_id,
            &evidence,
        ),
        edge(
            GraphEndpoint::Symbol(nested_id),
            GraphEndpoint::Symbol(root_id),
            SyntaxRelationKind::Tests,
            snapshot_id,
            &evidence,
        ),
    ];
    let obsolete = FileRevision::new(
        RepositoryPath::try_from_bytes(b"obsolete.rs".to_vec())?,
        ContentHash::from_bytes([4; 32]),
    );
    let mut files = vec![manifest.clone(), source];
    let mut symbols = vec![root, module, nested];
    let mut rank_ids = vec![root_id, module_id, nested_id];
    if include_obsolete {
        files.push(obsolete.clone());
        symbols.push(GraphSymbol::new(
            obsolete_id,
            obsolete,
            ParsedSymbol::new(
                LocalSymbolId::new(4)?,
                SymbolKind::Function,
                SymbolName::try_from_string("removed_symbol".to_owned())?,
                range,
                range,
            )?,
        ));
        rank_ids.push(obsolete_id);
    }
    let graph = LinkedGraph::new(snapshot_id, files, symbols, edges, Vec::new())?;
    let ranks = rank_ids
        .into_iter()
        .map(|symbol_id| {
            Ok(SymbolRank::new(
                symbol_id,
                RankScore::try_from_sum(0)?,
                SymbolRankSignals {
                    in_degree: 0,
                    out_degree: 0,
                    centrality: Centrality::from_basis_points(0)?,
                    degree_contribution: 0,
                    centrality_contribution: 0,
                    entrypoint_contribution: 0,
                    public_export_contribution: 0,
                    manifest_contribution: 0,
                    test_contribution: 0,
                },
            ))
        })
        .collect::<ContractResult<Vec<_>>>()?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
    let manifests = vec![manifest];
    let modules = fixture_modules(&graph, &ranking, &manifests)?;
    Ok(IndexPublication::new(graph, ranking, manifests, modules)?)
}

fn edge(
    source: GraphEndpoint,
    target: GraphEndpoint,
    kind: SyntaxRelationKind,
    snapshot_id: SnapshotId,
    evidence: &EvidenceRef,
) -> GraphEdge {
    GraphEdge::new(
        source,
        target,
        kind,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        evidence.clone(),
    )
}
