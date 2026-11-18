//! Current original modules reached through bounded reverse call traversal.

use super::{
    MAX_ASK_SOURCES, ProjectAskSelectionFailure, ProjectAskSourceSelection, defining_module,
    indexed_file, indexed_symbol, source_edges,
};
use crate::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{
    EvidenceRef, ExactSearchTarget, FileRevision, GraphEdge, GraphEndpoint, GraphSymbol,
    GraphTraversalResult, LinkResolution, LinkedGraph, ProjectIdentity, PublishedIndex, SymbolId,
    SymbolName, SyntaxRelationKind, TraversalDepth, TraversalDirection, TraversalQuery,
    TraversalResultLimit,
};

const MAX_CALLER_ROUTE_TEXT_BYTES: usize = 900;
const MAX_IMPORT_QUOTE_BYTES: usize = 256;

/// Verified current static paths and the owner modules needed to show them.
pub(crate) struct BoundCallerPlan {
    revisions: Vec<FileRevision>,
    target: BoundCallerNode,
    routes: Vec<Vec<BoundCallerNode>>,
}

#[derive(PartialEq, Eq)]
struct BoundCallerNode {
    evidence: EvidenceRef,
    symbol: Option<(SymbolId, SymbolName)>,
    import: Option<EvidenceRef>,
}

impl BoundCallerPlan {
    /// Returns additional complete modules, including any intermediate caller.
    pub(crate) fn revisions(&self) -> &[FileRevision] {
        &self.revisions
    }
}

/// Selects complete owner-file revisions and verifies resolved static caller paths.
///
/// Results are navigation only. Missing edges do not prove absence of other callers.
pub(crate) async fn select_caller_neighbor_revisions(
    search: &dyn KnowledgeSearchStore,
    project: &ProjectIdentity,
    index: &PublishedIndex,
    seed: &ProjectAskSourceSelection,
    control: &dyn KnowledgeSearchControl,
) -> Result<BoundCallerPlan, ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;

    if control.is_cancelled() {
        return Err(Failure::Search(KnowledgeSearchFailure::Cancelled));
    }
    let graph = index.publication().graph();
    let id = seed.symbol.ok_or(Failure::InvalidSymbol)?;
    let symbol = indexed_symbol(graph, id).ok_or(Failure::InvalidSymbol)?;
    if symbol.revision() != &seed.revision
        || seed.required != Some(symbol.parsed().declaration_range())
    {
        return Err(Failure::InvalidSymbol);
    }
    let query = caller_query(id);
    let result = search
        .traverse_graph(project, &query, control)
        .await
        .map_err(Failure::Search)?;
    if control.is_cancelled() {
        return Err(Failure::Search(KnowledgeSearchFailure::Cancelled));
    }
    if result.index_run_id() != index.run().id()
        || result.snapshot_id() != index.run().snapshot_id()
    {
        return Err(Failure::StalePublication);
    }
    bind_caller_sources(graph, &query, symbol, &result)
}

fn caller_query(id: SymbolId) -> TraversalQuery {
    TraversalQuery::new(
        GraphEndpoint::Symbol(id),
        TraversalDirection::Incoming,
        SyntaxRelationKind::Calls,
        TraversalDepth::INTERACTIVE_MAX,
        TraversalResultLimit::DEFAULT,
    )
}

fn bind_caller_sources(
    graph: &LinkedGraph,
    query: &TraversalQuery,
    primary: &GraphSymbol,
    result: &GraphTraversalResult,
) -> Result<BoundCallerPlan, ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;
    let invalid = Failure::Search(KnowledgeSearchFailure::InvalidStoredProjection);

    if result.query() != query {
        return Err(invalid);
    }
    if result.truncated() {
        return Err(Failure::Capacity);
    }
    let mut neighbors = Vec::new();
    let mut routes = Vec::with_capacity(result.hits().len());
    for hit in result.hits() {
        let mut route = Vec::with_capacity(hit.path().len());
        let mut callee = query.start();
        for edge in hit.path() {
            if edge.kind() != SyntaxRelationKind::Calls
                || edge.target() != callee
                || edge.snapshot_id() != graph.snapshot_id()
                || edge.evidence().range().is_empty()
                || !source_edges(graph, edge.source())?.contains(edge)
            {
                return Err(invalid);
            }
            let revision = edge.evidence().revision();
            if revision != primary.revision() && !neighbors.contains(revision) {
                if neighbors.len() == MAX_ASK_SOURCES - 1 {
                    return Err(Failure::Capacity);
                }
                neighbors.push(revision.clone());
            }
            let symbol = match edge.source() {
                GraphEndpoint::File(_) => None,
                GraphEndpoint::Symbol(id) => Some((
                    *id,
                    indexed_symbol(graph, *id)
                        .ok_or(invalid)?
                        .parsed()
                        .name()
                        .clone(),
                )),
            };
            route.push(BoundCallerNode {
                evidence: edge.evidence().clone(),
                symbol,
                import: unique_resolved_import(graph, edge),
            });
            callee = edge.source();
        }
        let target_matches = match (callee, hit.target()) {
            (GraphEndpoint::File(path), ExactSearchTarget::File(revision)) => {
                indexed_file(graph.files(), path) == Some(revision)
            }
            (GraphEndpoint::Symbol(id), ExactSearchTarget::Symbol(symbol)) => {
                indexed_symbol(graph, *id) == Some(symbol.symbol())
            }
            _ => false,
        };
        if !target_matches {
            return Err(invalid);
        }
        // Traversal walks from the seed toward its callers.
        route.reverse();
        routes.push(route);
    }
    Ok(BoundCallerPlan {
        revisions: neighbors,
        target: BoundCallerNode {
            evidence: EvidenceRef::new(
                primary.revision().clone(),
                primary.parsed().declaration_range(),
            ),
            symbol: Some((primary.id(), primary.parsed().name().clone())),
            import: None,
        },
        routes,
    })
}

// An optional quote needs an exact resolved import to this symbol from the
// same file and the caller's own lexical owner or defining module.
fn unique_resolved_import(graph: &LinkedGraph, call: &GraphEdge) -> Option<EvidenceRef> {
    if call.resolution() != LinkResolution::ExactModuleReference
        || !matches!(call.target(), GraphEndpoint::Symbol(_))
    {
        return None;
    }
    let revision = call.evidence().revision();
    let file = GraphEndpoint::File(revision.path().clone());
    let module = defining_module(graph, &file, revision, call.evidence().range()).ok()?;
    let mut found = None;
    for owner in [Some(call.source().clone()), Some(file), module]
        .into_iter()
        .flatten()
    {
        for edge in source_edges(graph, &owner).ok()? {
            if edge.kind() != SyntaxRelationKind::Imports
                || edge.target() != call.target()
                || edge.evidence().revision() != revision
                || edge.snapshot_id() != call.snapshot_id()
                || edge.evidence().range().is_empty()
            {
                continue;
            }
            if found.as_ref().is_some_and(|prior| prior != edge.evidence()) {
                return None;
            }
            found = Some(edge.evidence().clone());
        }
    }
    found
}

/// Renders static routes and bindings from exact call-site text in the delivered windows.
///
/// The resolver slices the supplied current original at the requested byte range.
/// A different token spelling establishes a static binding. An optional import
/// quote is presented as exact source, without inferring an import grammar.
pub(crate) fn render_bound_caller_routes<'source>(
    plan: &BoundCallerPlan,
    delivered: &[EvidenceRef],
    german: bool,
    resolve_call_site: impl Fn(&EvidenceRef) -> Option<&'source str>,
    resolve_import_line: impl Fn(&EvidenceRef, &str) -> Option<&'source str>,
) -> Result<Option<(String, Vec<EvidenceRef>)>, ProjectAskSelectionFailure> {
    if plan.routes.is_empty() {
        return Ok(None);
    }
    let mut text = if german {
        "Statisch sichtbare Aufrufpfade."
    } else {
        "Static call paths visible in the current sources."
    }
    .to_owned();
    let mut cited = Vec::new();
    let mut bindings = Vec::new();
    for route in &plan.routes {
        text.push_str(match (german, route.len()) {
            (true, 1) => " Direkt ",
            (true, _) => " Indirekt ",
            (false, 1) => " Direct ",
            (false, _) => " Indirect ",
        });
        let targets = route.iter().skip(1).chain(std::iter::once(&plan.target));
        for (caller, callee) in route.iter().zip(targets) {
            cite_window(&caller.evidence, delivered, &mut cited)?;
            push_node_name(&mut text, caller)?;
            text.push_str(" → ");
            if let Some((_, name)) = &callee.symbol {
                let token = resolve_call_site(&caller.evidence)
                    .filter(|token| {
                        usize::try_from(caller.evidence.range().len()).ok() == Some(token.len())
                    })
                    .filter(|token| !token.trim().is_empty())
                    .ok_or(ProjectAskSelectionFailure::InvalidSymbol)?;
                let binding = (caller, callee, token);
                if token != name.as_str() && !bindings.contains(&binding) {
                    bindings.push(binding);
                }
            }
        }
        cite_window(&plan.target.evidence, delivered, &mut cited)?;
        push_node_name(&mut text, &plan.target)?;
        text.push('.');
    }
    let mut import_quotes = Vec::new();
    for (caller, callee, token) in bindings {
        text.push_str(if german {
            " Statische Bindung von "
        } else {
            " Static binding of "
        });
        push_code_label(&mut text, token)?;
        text.push_str(" in ");
        push_node_name(&mut text, caller)?;
        text.push_str(if german { " an " } else { " to " });
        push_node_name(&mut text, callee)?;
        text.push('.');
        if let Some(import) = &caller.import
            && let Some(line) = resolve_import_line(import, token)
            && line.len() <= MAX_IMPORT_QUOTE_BYTES
            && !line.chars().any(|ch| ch.is_control() || ch == '`')
            && !import_quotes.contains(&(import, line))
        {
            import_quotes.push((import, line));
        }
    }
    let import_label = if german {
        " Importquelle "
    } else {
        " Import source "
    };
    for (import, line) in import_quotes {
        if text.len() + import_label.len() + line.len() + 3 <= MAX_CALLER_ROUTE_TEXT_BYTES
            && cite_window(import, delivered, &mut cited).is_ok()
        {
            text.push_str(import_label);
            push_code_label(&mut text, line)?;
            text.push('.');
        }
    }
    if text.len() > MAX_CALLER_ROUTE_TEXT_BYTES {
        return Err(ProjectAskSelectionFailure::Capacity);
    }
    Ok(Some((text, cited)))
}

fn cite_window(
    required: &EvidenceRef,
    delivered: &[EvidenceRef],
    cited: &mut Vec<EvidenceRef>,
) -> Result<(), ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;
    let Some(window) = delivered.iter().find(|window| {
        window.revision() == required.revision() && window.range().contains(required.range())
    }) else {
        return Err(
            if delivered.iter().any(|window| {
                window.revision().path() == required.revision().path()
                    && window.revision() != required.revision()
            }) {
                Failure::StalePublication
            } else {
                Failure::InvalidSymbol
            },
        );
    };
    if !cited.contains(window) {
        if cited.len() == MAX_ASK_SOURCES {
            return Err(Failure::Capacity);
        }
        cited.push(window.clone());
    }
    Ok(())
}

fn push_node_name(
    text: &mut String,
    node: &BoundCallerNode,
) -> Result<(), ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;
    let path = std::str::from_utf8(node.evidence.revision().path().as_bytes())
        .map_err(|_| Failure::InvalidSymbol)?;
    let name = node.symbol.as_ref().map(|(_, name)| name.as_str());
    if text.len() + path.len() + name.map_or(0, |name| name.len() + 2) + 2
        > MAX_CALLER_ROUTE_TEXT_BYTES
    {
        return Err(Failure::Capacity);
    }
    if path
        .chars()
        .chain(name.into_iter().flat_map(str::chars))
        .any(|ch| ch.is_control() || ch == '`')
    {
        return Err(Failure::InvalidSymbol);
    }
    text.push('`');
    text.push_str(path);
    if let Some(name) = name {
        text.push_str("::");
        text.push_str(name);
    }
    text.push('`');
    Ok(())
}

fn push_code_label(text: &mut String, token: &str) -> Result<(), ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;
    if text.len() + token.len() + 2 > MAX_CALLER_ROUTE_TEXT_BYTES {
        return Err(Failure::Capacity);
    }
    if token.chars().any(|ch| ch.is_control() || ch == '`') {
        return Err(Failure::InvalidSymbol);
    }
    text.push('`');
    text.push_str(token);
    text.push('`');
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        Confidence, ContentHash, ExactSearchSymbol, GraphEdge, GraphTraversalHit, IndexRunId,
        LinkResolution, LocalSymbolId, ParsedSymbol, QualifiedSymbolName, RepositoryPath,
        SnapshotId, SourcePosition, SourceRange, SymbolKind, SyntaxProvider,
    };
    use std::error::Error;

    #[test]
    fn reverse_call_paths_bind_intermediate_modules_and_reject_unpublished_edges()
    -> Result<(), Box<dyn Error>> {
        let run = IndexRunId::from_bytes([1; 32]);
        let snapshot = SnapshotId::from_bytes([2; 32]);
        let range = SourceRange::new(0, 20, SourcePosition::new(0, 0), SourcePosition::new(0, 20))?;
        let names = [
            ("domain", "create_task"),
            ("service", "schedule"),
            ("api", "post"),
        ];
        let symbols = names
            .into_iter()
            .enumerate()
            .map(
                |(index, (file, name))| -> Result<GraphSymbol, Box<dyn Error>> {
                    let id = u8::try_from(index + 1)?;
                    Ok(GraphSymbol::new(
                        SymbolId::from_bytes([id; 32]),
                        FileRevision::new(
                            RepositoryPath::try_from_bytes(format!("{file}.py").into_bytes())?,
                            ContentHash::from_bytes([id; 32]),
                        ),
                        ParsedSymbol::new(
                            LocalSymbolId::new(1)?,
                            SymbolKind::Function,
                            SymbolName::try_from_string(name.to_owned())?,
                            range,
                            range,
                        )?,
                    ))
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let call =
            |source: usize, target: usize, confidence| -> Result<GraphEdge, Box<dyn Error>> {
                let token = if source == 1 {
                    "persist_task"
                } else {
                    "schedule"
                };
                let call_range = SourceRange::new(
                    0,
                    token.len(),
                    SourcePosition::new(0, 0),
                    SourcePosition::new(0, u32::try_from(token.len())?),
                )?;
                Ok(GraphEdge::new(
                    GraphEndpoint::Symbol(symbols[source].id()),
                    GraphEndpoint::Symbol(symbols[target].id()),
                    SyntaxRelationKind::Calls,
                    SyntaxProvider::TreeSitter,
                    confidence,
                    LinkResolution::ExactModuleReference,
                    snapshot,
                    EvidenceRef::new(symbols[source].revision().clone(), call_range),
                ))
            };
        let service = call(1, 0, Confidence::certain())?;
        let api = call(2, 1, Confidence::certain())?;
        let graph = LinkedGraph::new(
            snapshot,
            symbols
                .iter()
                .map(|symbol| symbol.revision().clone())
                .collect(),
            symbols.clone(),
            vec![service.clone(), api.clone()],
            Vec::new(),
        )?;
        let query = caller_query(symbols[0].id());
        let result = |path, truncated| -> Result<GraphTraversalResult, Box<dyn Error>> {
            let hit = GraphTraversalHit::new(
                ExactSearchTarget::Symbol(ExactSearchSymbol::new(
                    symbols[2].clone(),
                    QualifiedSymbolName::try_from_string("api::post".to_owned())?,
                )),
                path,
                &query,
                snapshot,
            )?;
            Ok(GraphTraversalResult::new(
                run,
                snapshot,
                query.clone(),
                vec![hit],
                truncated,
            )?)
        };
        let primary = &symbols[0];
        let path = vec![service.clone(), api];
        let mut plan = bind_caller_sources(&graph, &query, primary, &result(path.clone(), false)?)?;
        assert_eq!(
            plan.revisions(),
            [symbols[1].revision().clone(), symbols[2].revision().clone()],
        );
        let delivered = symbols
            .iter()
            .map(|symbol| EvidenceRef::new(symbol.revision().clone(), range))
            .collect::<Vec<_>>();
        let resolve = |evidence: &EvidenceRef| match evidence.revision() {
            revision if revision == symbols[1].revision() => Some("persist_task"),
            revision if revision == symbols[2].revision() => Some("schedule"),
            _ => None,
        };
        let (german, sources) =
            render_bound_caller_routes(&plan, &delivered, true, resolve, |_, _| None)?
                .ok_or("expected bound route")?;
        assert!(german.contains(
            "Indirekt `api.py::post` → `service.py::schedule` → `domain.py::create_task`"
        ));
        assert_eq!(
            sources,
            [
                delivered[2].clone(),
                delivered[1].clone(),
                delivered[0].clone()
            ]
        );
        let (english, _) =
            render_bound_caller_routes(&plan, &delivered, false, resolve, |_, _| None)?
                .ok_or("expected bound route")?;
        assert!(english.contains("Indirect"));
        assert!(german.contains("Statische Bindung von `persist_task` in `service.py::schedule` an `domain.py::create_task`"));
        assert_eq!(german.matches("Statische Bindung").count(), 1);
        assert!(matches!(
            render_bound_caller_routes(&plan, &delivered, true, |_| None, |_, _| None),
            Err(ProjectAskSelectionFailure::InvalidSymbol)
        ));
        assert!(matches!(
            render_bound_caller_routes(&plan, &delivered, true, |_| Some("x"), |_, _| None),
            Err(ProjectAskSelectionFailure::InvalidSymbol)
        ));
        let direct = GraphTraversalHit::new(
            ExactSearchTarget::Symbol(ExactSearchSymbol::new(
                symbols[1].clone(),
                QualifiedSymbolName::try_from_string("service::schedule".to_owned())?,
            )),
            vec![service.clone()],
            &query,
            snapshot,
        )?;
        let direct = GraphTraversalResult::new(run, snapshot, query.clone(), vec![direct], false)?;
        let direct = bind_caller_sources(&graph, &query, primary, &direct)?;
        let (direct, _) =
            render_bound_caller_routes(&direct, &delivered, true, resolve, |_, _| None)?
                .ok_or("expected direct route")?;
        assert!(direct.contains("Direkt"));
        let empty = GraphTraversalResult::new(run, snapshot, query.clone(), Vec::new(), false)?;
        let empty = bind_caller_sources(&graph, &query, primary, &empty)?;
        assert!(render_bound_caller_routes(&empty, &[], true, resolve, |_, _| None)?.is_none());
        assert!(matches!(
            render_bound_caller_routes(&plan, &delivered[1..], true, resolve, |_, _| None),
            Err(ProjectAskSelectionFailure::InvalidSymbol)
        ));
        let mut partial = delivered.clone();
        partial[1] = EvidenceRef::new(
            partial[1].revision().clone(),
            SourceRange::new(0, 9, SourcePosition::new(0, 0), SourcePosition::new(0, 9))?,
        );
        assert!(matches!(
            render_bound_caller_routes(&plan, &partial, true, resolve, |_, _| None),
            Err(ProjectAskSelectionFailure::InvalidSymbol)
        ));
        let mut stale = delivered.clone();
        stale[1] = EvidenceRef::new(
            FileRevision::new(
                stale[1].revision().path().clone(),
                ContentHash::from_bytes([99; 32]),
            ),
            range,
        );
        assert!(matches!(
            render_bound_caller_routes(&plan, &stale, true, resolve, |_, _| None),
            Err(ProjectAskSelectionFailure::StalePublication)
        ));
        plan.target.symbol = Some((
            primary.id(),
            SymbolName::try_from_string("x".repeat(MAX_CALLER_ROUTE_TEXT_BYTES))?,
        ));
        assert!(matches!(
            render_bound_caller_routes(&plan, &delivered, true, resolve, |_, _| None),
            Err(ProjectAskSelectionFailure::Capacity)
        ));
        assert!(matches!(
            bind_caller_sources(&graph, &query, primary, &result(path, true)?),
            Err(ProjectAskSelectionFailure::Capacity)
        ));
        let unbound = call(2, 1, Confidence::from_basis_points(9_000)?)?;
        assert!(matches!(
            bind_caller_sources(
                &graph,
                &query,
                primary,
                &result(vec![service, unbound], false)?
            ),
            Err(ProjectAskSelectionFailure::Search(
                KnowledgeSearchFailure::InvalidStoredProjection
            ))
        ));
        Ok(())
    }
}
