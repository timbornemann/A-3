//! Exact initial source selection for a project-bound Ask turn.

#[path = "project_ask_callers.rs"]
mod callers;
pub(crate) use callers::{
    BoundCallerPlan, render_bound_caller_routes, select_caller_neighbor_revisions,
};

use crate::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchStore};
use a3_domain::{
    EvidenceRef, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ExactSearchTarget,
    ExactSearchTerm, FileRevision, GraphEdge, GraphEndpoint, GraphSymbol, LinkedGraph,
    ProjectIdentity, PublishedIndex, RepositoryPath, SourceRange, SymbolId, SymbolKind, SymbolName,
    SyntaxRelationKind, UnresolvedGraphTarget, UnresolvedReason,
};
use std::{error::Error, fmt};

pub(crate) const MAX_ASK_SOURCES: usize = 4;
const MAX_SYMBOL_LOOKUPS: usize = 8;
const MAX_EDGES_PER_SOURCE: usize = 4_096;

#[derive(Clone)]
pub(crate) struct ProjectAskSourceSelection {
    pub(crate) revision: FileRevision,
    /// The original declaration range, when a symbol was selected.
    pub(crate) required: Option<SourceRange>,
    pub(crate) symbol: Option<SymbolId>,
    /// An explicit file selection remains in force when merged with a symbol.
    pub(crate) require_complete: bool,
}

pub(crate) struct ProjectAskSymbolSelections {
    pub(crate) matches: Vec<ProjectAskSourceSelection>,
    pub(crate) caller_seed: Option<SymbolId>,
}

/// Selects exact indexed path mentions in question order.
///
/// Bare path-shaped tokens and explicit single-token path mentions are supported.
/// Unresolved terms give no selection and carry no claim about the project.
pub(crate) fn select_project_ask_sources(
    index: &PublishedIndex,
    question: &str,
) -> Result<Vec<ProjectAskSourceSelection>, ProjectAskSelectionFailure> {
    select_paths(index.publication().graph().files(), question)
}

pub(crate) fn select_attached_ask_source(
    index: &PublishedIndex,
    path: &RepositoryPath,
) -> Result<ProjectAskSourceSelection, ProjectAskSelectionFailure> {
    let revision = indexed_file(index.publication().graph().files(), path)
        .ok_or(ProjectAskSelectionFailure::StalePublication)?;
    Ok(ProjectAskSourceSelection {
        revision: revision.clone(),
        required: None,
        symbol: None,
        require_complete: true,
    })
}

fn select_paths(
    files: &[FileRevision],
    question: &str,
) -> Result<Vec<ProjectAskSourceSelection>, ProjectAskSelectionFailure> {
    let mut selections: Vec<ProjectAskSourceSelection> = Vec::new();
    for path in question.split_whitespace().filter_map(mentioned_path) {
        let Some(revision) = indexed_file(files, &path) else {
            continue;
        };
        if selections.iter().any(|entry| entry.revision == *revision) {
            continue;
        }
        if selections.len() == MAX_ASK_SOURCES {
            return Err(ProjectAskSelectionFailure::Capacity);
        }
        selections.push(ProjectAskSourceSelection {
            revision: revision.clone(),
            required: None,
            symbol: None,
            require_complete: true,
        });
    }
    Ok(selections)
}

/// Resolves explicit qualified mentions against the exact projection of this publication.
///
/// The port supports full or prefix matching, so query the terminal identifier and
/// check the complete qualification locally. A cursor means uniqueness was not
/// established within this budget. That mention contributes no source.
pub(crate) async fn select_symbol_sources(
    search: &dyn KnowledgeSearchStore,
    project: &ProjectIdentity,
    index: &PublishedIndex,
    question: &str,
    control: &dyn KnowledgeSearchControl,
) -> Result<ProjectAskSymbolSelections, ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;

    if control.is_cancelled() {
        return Err(Failure::Search(KnowledgeSearchFailure::Cancelled));
    }
    let graph = index.publication().graph();
    let mut attempted: Vec<(String, Option<SymbolId>)> = Vec::new();
    let mut selections: Vec<ProjectAskSourceSelection> = Vec::new();
    for token in question.split_whitespace() {
        let Some(qualified) = mentioned_symbol(token) else {
            continue;
        };
        if attempted.iter().any(|(previous, _)| previous == &qualified)
            || mentioned_path(token)
                .is_some_and(|path| indexed_file(graph.files(), &path).is_some())
        {
            continue;
        }
        let leaf = qualified
            .rsplit_once("::")
            .map_or(qualified.as_str(), |(_, leaf)| leaf);
        let Ok(term) = ExactSearchTerm::try_from_string(leaf.to_owned()) else {
            continue;
        };
        if attempted.len() == MAX_SYMBOL_LOOKUPS {
            return Err(Failure::Capacity);
        }
        let page = search
            .search_exact(
                project,
                &ExactSearchQuery::Symbol(term),
                ExactSearchPageSize::DEFAULT,
                None,
                control,
            )
            .await
            .map_err(Failure::Search)?;
        if control.is_cancelled() {
            return Err(Failure::Search(KnowledgeSearchFailure::Cancelled));
        }
        if page.index_run_id() != index.run().id()
            || page.snapshot_id() != index.run().snapshot_id()
        {
            return Err(Failure::StalePublication);
        }
        let mut bound = None;
        if let Some(symbol) = unique_qualified_symbol(&page, &qualified) {
            let exact = indexed_symbol(graph, symbol.id());
            let range = symbol.parsed().declaration_range();
            if exact != Some(symbol) || range.is_empty() {
                return Err(Failure::InvalidSymbol);
            }
            bound = Some(symbol.id());
            if !selections
                .iter()
                .any(|selection| selection.symbol == Some(symbol.id()))
            {
                selections.push(ProjectAskSourceSelection {
                    revision: symbol.revision().clone(),
                    required: Some(range),
                    symbol: Some(symbol.id()),
                    require_complete: false,
                });
            }
        }
        attempted.push((qualified, bound));
    }
    Ok(ProjectAskSymbolSelections {
        matches: selections,
        caller_seed: caller_seed(&attempted),
    })
}

fn caller_seed(mentions: &[(String, Option<SymbolId>)]) -> Option<SymbolId> {
    let has_qualified = mentions.iter().any(|(name, _)| name.contains("::"));
    let mut candidates = mentions
        .iter()
        .filter(|(name, _)| !has_qualified || name.contains("::"))
        .map(|(_, symbol)| *symbol);
    candidates
        .next()
        .flatten()
        .filter(|_| candidates.next().is_none())
}

pub(crate) struct ProjectAskImportNeighbor {
    pub(crate) revision: FileRevision,
    pub(crate) declaration: Option<ProjectAskSourceSelection>,
}

/// Selects a single dynamic bare-name call. The candidate supplies a source
/// location, never the identity or effects of the runtime callable.
pub(crate) fn select_unresolved_effect_call(
    index: &PublishedIndex,
    selection: &ProjectAskSourceSelection,
) -> Option<(EvidenceRef, SymbolName)> {
    let graph = index.publication().graph();
    let id = selection.symbol?;
    let declaration = selection.required?;
    let symbol = indexed_symbol(graph, id)?;
    if !matches!(
        symbol.parsed().kind(),
        SymbolKind::Function | SymbolKind::Method
    ) || symbol.revision() != &selection.revision
        || symbol.parsed().declaration_range() != declaration
        || indexed_file(graph.files(), selection.revision.path()) != Some(&selection.revision)
    {
        return None;
    }

    let source = GraphEndpoint::Symbol(id);
    if source_edges(graph, &source)
        .ok()?
        .iter()
        .any(|edge| edge.kind() == SyntaxRelationKind::Calls)
    {
        return None;
    }
    let candidates = graph.unresolved();
    let start = candidates.partition_point(|candidate| candidate.source() < &source);
    let end = candidates.partition_point(|candidate| candidate.source() <= &source);
    if end - start > MAX_EDGES_PER_SOURCE {
        return None;
    }
    let mut calls = candidates[start..end]
        .iter()
        .filter(|candidate| candidate.kind() == SyntaxRelationKind::Calls);
    let call = calls.next()?;
    let evidence = call.evidence();
    if calls.next().is_some()
        || call.reason() != UnresolvedReason::DynamicReference
        || call.snapshot_id() != index.run().snapshot_id()
        || evidence.revision() != &selection.revision
        || evidence.range().is_empty()
        || !declaration.contains(evidence.range())
    {
        return None;
    }
    let UnresolvedGraphTarget::Reference(reference) = call.target() else {
        return None;
    };
    let name = reference.as_str();
    if !is_symbol_component(name) {
        return None;
    }
    Some((
        evidence.clone(),
        SymbolName::try_from_string(name.to_owned()).ok()?,
    ))
}

/// Selects current first-hop import targets adjacent to an explicitly bound symbol.
///
/// Only direct imports from that symbol, its file, or the file's defining module
/// contribute candidates. These revisions are navigation hints, not resolved calls.
pub(crate) fn select_import_neighbors(
    index: &PublishedIndex,
    selected: &[ProjectAskSourceSelection],
) -> Result<Vec<ProjectAskImportNeighbor>, ProjectAskSelectionFailure> {
    import_neighbors(index.publication().graph(), selected)
}

fn import_neighbors(
    graph: &LinkedGraph,
    selected: &[ProjectAskSourceSelection],
) -> Result<Vec<ProjectAskImportNeighbor>, ProjectAskSelectionFailure> {
    use ProjectAskSelectionFailure as Failure;

    if selected.len() > MAX_ASK_SOURCES {
        return Err(Failure::Capacity);
    }
    let remaining = MAX_ASK_SOURCES - selected.len();
    let mut neighbors: Vec<(FileRevision, Option<GraphEndpoint>)> = Vec::new();
    let mut visited = Vec::new();
    for selection in selected {
        let revision = &selection.revision;
        if indexed_file(graph.files(), revision.path()) != Some(revision) {
            return Err(Failure::StalePublication);
        }
        let Some(id) = selection.symbol else {
            continue;
        };
        let symbol = indexed_symbol(graph, id).ok_or(Failure::InvalidSymbol)?;
        let declaration = symbol.parsed().declaration_range();
        if symbol.revision() != revision || selection.required != Some(declaration) {
            return Err(Failure::InvalidSymbol);
        }
        let file = GraphEndpoint::File(revision.path().clone());
        let module = defining_module(graph, &file, revision, declaration)?;
        for endpoint in [Some(file), module, Some(GraphEndpoint::Symbol(id))]
            .into_iter()
            .flatten()
        {
            if visited.contains(&endpoint) {
                continue;
            }
            for edge in source_edges(graph, &endpoint)? {
                if edge.kind() != SyntaxRelationKind::Imports
                    || edge.evidence().revision() != revision
                {
                    continue;
                }
                let target = match edge.target() {
                    GraphEndpoint::File(path) => indexed_file(graph.files(), path),
                    GraphEndpoint::Symbol(id) => {
                        indexed_symbol(graph, *id).map(GraphSymbol::revision)
                    }
                }
                .ok_or(Failure::StalePublication)?;
                if selected.iter().any(|source| source.revision == *target) {
                    continue;
                }
                if let Some((_, endpoint)) = neighbors
                    .iter_mut()
                    .find(|(revision, _)| revision == target)
                {
                    if endpoint.as_ref() != Some(edge.target()) {
                        *endpoint = None;
                    }
                    continue;
                }
                if neighbors.len() == remaining {
                    return Err(Failure::Capacity);
                }
                neighbors.push((target.clone(), Some(edge.target().clone())));
            }
            visited.push(endpoint);
        }
    }
    neighbors.sort_unstable_by(|(left, _), (right, _)| left.path().cmp(right.path()));
    Ok(neighbors
        .into_iter()
        .map(|(revision, target)| {
            let declaration =
                target.and_then(|endpoint| imported_declaration(graph, &revision, &endpoint));
            ProjectAskImportNeighbor {
                revision,
                declaration,
            }
        })
        .collect())
}

// A declaration focuses a local observation. An import edge does not establish
// which implementation a dynamic receiver will call at runtime.
fn imported_declaration(
    graph: &LinkedGraph,
    revision: &FileRevision,
    endpoint: &GraphEndpoint,
) -> Option<ProjectAskSourceSelection> {
    let GraphEndpoint::Symbol(id) = endpoint else {
        return None;
    };
    let imported = indexed_symbol(graph, *id)?;
    if imported.revision() != revision {
        return None;
    }
    let callable = match imported.parsed().kind() {
        SymbolKind::Function | SymbolKind::Method => imported,
        SymbolKind::Class => {
            let mut direct: Option<&GraphSymbol> = None;
            for edge in source_edges(graph, endpoint).ok()? {
                if edge.kind() != SyntaxRelationKind::Contains {
                    continue;
                }
                let GraphEndpoint::Symbol(child) = edge.target() else {
                    return None;
                };
                let child = indexed_symbol(graph, *child)?;
                if child.revision() != revision
                    || edge.evidence().revision() != revision
                    || !imported
                        .parsed()
                        .declaration_range()
                        .contains(child.parsed().declaration_range())
                {
                    return None;
                }
                if matches!(
                    child.parsed().kind(),
                    SymbolKind::Function | SymbolKind::Method
                ) {
                    if direct.is_some_and(|previous| previous.id() != child.id()) {
                        return None;
                    }
                    direct = Some(child);
                }
            }
            direct?
        }
        _ => return None,
    };
    let declaration = callable.parsed().declaration_range();
    (!declaration.is_empty()).then(|| ProjectAskSourceSelection {
        revision: revision.clone(),
        required: Some(declaration),
        symbol: Some(callable.id()),
        require_complete: true,
    })
}

fn defining_module(
    graph: &LinkedGraph,
    file: &GraphEndpoint,
    revision: &FileRevision,
    declaration: SourceRange,
) -> Result<Option<GraphEndpoint>, ProjectAskSelectionFailure> {
    let mut modules = source_edges(graph, file)?.iter().filter_map(|edge| {
        let GraphEndpoint::Symbol(id) = edge.target() else {
            return None;
        };
        if edge.kind() != SyntaxRelationKind::Defines || edge.evidence().revision() != revision {
            return None;
        }
        let symbol = indexed_symbol(graph, *id)?;
        (symbol.revision() == revision
            && symbol.parsed().kind() == SymbolKind::Module
            && symbol.parsed().declaration_range().contains(declaration))
        .then_some(*id)
    });
    let first = modules.next();
    if modules.any(|candidate| Some(candidate) != first) {
        return Err(ProjectAskSelectionFailure::InvalidSymbol);
    }
    Ok(first.map(GraphEndpoint::Symbol))
}

fn source_edges<'a>(
    graph: &'a LinkedGraph,
    source: &GraphEndpoint,
) -> Result<&'a [GraphEdge], ProjectAskSelectionFailure> {
    let edges = graph.edges();
    let start = edges.partition_point(|edge| edge.source() < source);
    let end = edges.partition_point(|edge| edge.source() <= source);
    if end - start > MAX_EDGES_PER_SOURCE {
        return Err(ProjectAskSelectionFailure::Capacity);
    }
    Ok(&edges[start..end])
}

fn indexed_symbol(graph: &LinkedGraph, id: SymbolId) -> Option<&GraphSymbol> {
    let symbols = graph.symbols();
    let position = symbols.binary_search_by_key(&id, GraphSymbol::id).ok()?;
    symbols.get(position)
}

fn unique_qualified_symbol<'a>(
    page: &'a ExactSearchPage,
    requested: &str,
) -> Option<&'a GraphSymbol> {
    if page.next_cursor().is_some() {
        return None;
    }
    let mut matches = page.hits().iter().filter_map(|hit| {
        let ExactSearchTarget::Symbol(symbol) = hit.target() else {
            return None;
        };
        let canonical = symbol.qualified_name().as_str();
        matches_qualification(canonical, requested).then(|| symbol.symbol())
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn matches_qualification(canonical: &str, requested: &str) -> bool {
    canonical == requested
        || canonical
            .strip_suffix(requested)
            .is_some_and(|prefix| prefix.ends_with("::"))
}

fn indexed_file<'a>(files: &'a [FileRevision], path: &RepositoryPath) -> Option<&'a FileRevision> {
    let position = files
        .binary_search_by(|revision| revision.path().cmp(path))
        .ok()?;
    files.get(position)
}

fn mentioned_symbol(token: &str) -> Option<String> {
    let (name, marked) = unwrapped_mention(token)?;
    if !name.contains('.') && !name.contains("::") {
        return ((marked || name.contains('_')) && is_symbol_component(name))
            .then(|| name.to_owned());
    }
    let canonical = name.replace('.', "::");
    canonical
        .split("::")
        .all(is_symbol_component)
        .then_some(canonical)
}

fn is_symbol_component(component: &str) -> bool {
    let mut chars = component.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

fn mentioned_path(token: &str) -> Option<RepositoryPath> {
    let (path, marked) = unwrapped_mention(token)?;
    if (!marked && !path.contains(['.', '/', '\\']))
        || path.contains(':')
        || path.chars().any(char::is_control)
    {
        return None;
    }
    let relative = path
        .strip_prefix("./")
        .or_else(|| path.strip_prefix(".\\"))
        .unwrap_or(path);
    RepositoryPath::try_from_bytes(relative.replace('\\', "/").into_bytes()).ok()
}

fn unwrapped_mention(token: &str) -> Option<(&str, bool)> {
    let token = token
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches(['.', ',', ';', '!', '?', ')', ']', '}']);
    let marked = token.starts_with(['@', '`', '"', '\'']);
    let token = token.strip_prefix('@').unwrap_or(token);
    let value = match token.chars().next()? {
        quote @ ('`' | '"' | '\'') => token.strip_prefix(quote)?.strip_suffix(quote)?,
        _ => token,
    };
    Some((value, marked))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectAskSelectionFailure {
    Search(KnowledgeSearchFailure),
    StalePublication,
    InvalidSymbol,
    Capacity,
}

impl fmt::Display for ProjectAskSelectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Search(_) => "project Ask exact search failed",
            Self::StalePublication => "project Ask exact search publication changed",
            Self::InvalidSymbol => "project Ask exact search supplied an invalid symbol",
            Self::Capacity => "project Ask source selection exceeds its bounded scope",
        })
    }
}

impl Error for ProjectAskSelectionFailure {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{Confidence, EvidenceRef, LinkResolution, SyntaxProvider};
    use a3_domain::{
        ContentHash, ExactSearchCursor, ExactSearchExplanation, ExactSearchHit,
        ExactSearchPosition, ExactSearchSymbol, IndexRunId, LocalSymbolId, ParsedSymbol,
        QualifiedSymbolName, SnapshotId, SourcePosition, SymbolKind, SymbolName,
    };
    use std::error::Error;

    #[test]
    fn caller_seed_requires_the_unambiguous_requested_qualification() {
        let target = SymbolId::from_bytes([1; 32]);
        let report = SymbolId::from_bytes([2; 32]);
        let primary = "domain::create_task".to_owned();
        let similar = "create_task_report".to_owned();
        assert_eq!(
            caller_seed(&[
                (primary.clone(), Some(target)),
                (similar.clone(), Some(report))
            ]),
            Some(target)
        );
        assert_eq!(
            caller_seed(&[(primary, None), (similar, Some(report))]),
            None
        );
        assert_eq!(
            caller_seed(&[
                ("domain::create_task".to_owned(), Some(target)),
                ("reporting::create_task_report".to_owned(), Some(report))
            ]),
            None
        );
    }

    #[test]
    fn exact_current_paths_are_unique_and_keep_question_order() -> Result<(), Box<dyn Error>> {
        let files = indexed(&["Config.py", "README", "config.py", "src/config.py"])?;
        let sources = select_paths(
            &files,
            r#"Welcher Wert gilt in (`config.py`)? Prüfe @src\config.py, ./config.py und "README"."#,
        )?;
        assert_eq!(
            sources
                .iter()
                .map(|source| &source.revision)
                .collect::<Vec<_>>(),
            [&files[2], &files[3], &files[1]],
        );
        assert!(
            sources
                .iter()
                .all(|source| source.required.is_none() && source.symbol.is_none())
        );
        Ok(())
    }

    #[test]
    fn ambiguous_basenames_foreign_paths_and_unresolved_terms_give_no_source()
    -> Result<(), Box<dyn Error>> {
        let files = indexed(&["a/config.py", "b/config.py", "config.py.bak", "writer.py"])?;
        let sources = select_paths(
            &files,
            r#"config.py ../writer.py /writer.py C:writer.py C:\writer.py \\host\writer.py https://host/writer.py dispatcher.dispatch"#,
        )?;
        assert!(sources.is_empty());
        Ok(())
    }

    #[test]
    fn extra_explicit_sources_require_a_narrower_question() -> Result<(), Box<dyn Error>> {
        let files = indexed(&["a.py", "b.py", "c.py", "d.py", "e.py"])?;
        assert!(matches!(
            select_paths(&files, "e.py e.py c.py d.py b.py a.py"),
            Err(ProjectAskSelectionFailure::Capacity)
        ));
        let sources = select_paths(&files, "e.py e.py c.py d.py b.py")?;
        assert_eq!(
            sources
                .iter()
                .map(|source| &source.revision)
                .collect::<Vec<_>>(),
            [&files[4], &files[2], &files[3], &files[1]],
        );
        Ok(())
    }

    #[test]
    fn explicit_qualified_mentions_are_normalized_without_guessing_names() {
        assert_eq!(
            mentioned_symbol("(`Storage.save_tasks`)?").as_deref(),
            Some("Storage::save_tasks")
        );
        assert_eq!(
            mentioned_symbol("@dispatcher::dispatch").as_deref(),
            Some("dispatcher::dispatch")
        );
        assert_eq!(
            mentioned_symbol("create_task_report?").as_deref(),
            Some("create_task_report")
        );
        assert_eq!(
            mentioned_symbol("Öffentlich.eintrag").as_deref(),
            Some("Öffentlich::eintrag")
        );
        for token in [
            "storage",
            "../storage.save_tasks",
            "C:Storage.save_tasks",
            "https://Storage.save_tasks",
            "Storage...save_tasks",
            "::save_tasks",
            "Storage.42",
            "Storage.save_tasks(arg)",
        ] {
            assert!(mentioned_symbol(token).is_none(), "accepted {token}");
        }
        assert!(matches_qualification(
            "storage::Storage::save_tasks",
            "Storage::save_tasks"
        ));
        assert!(matches_qualification(
            "Storage::save_tasks",
            "Storage::save_tasks"
        ));
        assert!(!matches_qualification(
            "storage::OtherStorage::save_tasks",
            "Storage::save_tasks"
        ));
        assert!(!matches_qualification(
            "storage::Storage::save_tasks_report",
            "Storage::save_tasks"
        ));
        assert!(!matches_qualification(
            "storage::storage::save_tasks",
            "Storage::save_tasks"
        ));
    }

    #[test]
    fn unique_exact_qualification_requires_a_complete_page() -> Result<(), Box<dyn Error>> {
        let run = IndexRunId::from_bytes([1; 32]);
        let snapshot = SnapshotId::from_bytes([2; 32]);
        let limit = ExactSearchPageSize::DEFAULT;
        let exact = hit(3, "storage::Storage::save_tasks", "save_tasks")?;
        let different_owner = hit(4, "reporting::OtherStorage::save_tasks", "save_tasks")?;
        let prefix = hit(
            5,
            "storage::Storage::save_tasks_report",
            "save_tasks_report",
        )?;
        let page = ExactSearchPage::new(
            run,
            snapshot,
            vec![exact.clone(), different_owner, prefix],
            None,
            limit,
        )?;
        assert_eq!(
            unique_qualified_symbol(&page, "Storage::save_tasks").map(GraphSymbol::id),
            Some(SymbolId::from_bytes([3; 32])),
        );
        assert!(unique_qualified_symbol(&page, "Unknown::save_tasks").is_none());

        let ambiguous = ExactSearchPage::new(
            run,
            snapshot,
            vec![
                exact.clone(),
                hit(6, "other::Storage::save_tasks", "save_tasks")?,
            ],
            None,
            limit,
        )?;
        assert!(unique_qualified_symbol(&ambiguous, "Storage::save_tasks").is_none());

        let ExactSearchTarget::Symbol(last) = exact.target() else {
            return Err("expected exact symbol test hit".into());
        };
        let cursor = ExactSearchCursor::new(
            run,
            snapshot,
            ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("save_tasks".to_owned())?),
            ExactSearchPosition::Symbol {
                explanation: exact.explanation(),
                path: last.symbol().revision().path().clone(),
                qualified_name: last.qualified_name().clone(),
                symbol_id: last.symbol().id(),
            },
        )?;
        let truncated = ExactSearchPage::new(run, snapshot, vec![exact], Some(cursor), limit)?;
        assert!(unique_qualified_symbol(&truncated, "Storage::save_tasks").is_none());
        Ok(())
    }

    fn hit(id: u8, qualified: &str, name: &str) -> Result<ExactSearchHit, Box<dyn Error>> {
        let range = SourceRange::new(0, 10, SourcePosition::new(0, 0), SourcePosition::new(0, 10))?;
        let parsed = ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string(name.to_owned())?,
            range,
            range,
        )?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(format!("file_{id}.py").into_bytes())?,
            ContentHash::from_bytes([id; 32]),
        );
        Ok(ExactSearchHit::symbol(
            ExactSearchSymbol::new(
                GraphSymbol::new(SymbolId::from_bytes([id; 32]), revision, parsed),
                QualifiedSymbolName::try_from_string(qualified.to_owned())?,
            ),
            if name == "save_tasks" {
                ExactSearchExplanation::SymbolNameExact
            } else {
                ExactSearchExplanation::SymbolNamePrefix
            },
        )?)
    }

    #[test]
    fn imports_follow_file_and_module_once_without_following_calls_or_second_hops()
    -> Result<(), Box<dyn Error>> {
        let files = indexed(&["a.py", "b.py", "c.py", "d.py", "e.py"])?;
        let snapshot = SnapshotId::from_bytes([20; 32]);
        let range = SourceRange::new(0, 10, SourcePosition::new(0, 0), SourcePosition::new(0, 10))?;
        let symbol = |id,
                      kind,
                      revision: &FileRevision,
                      name: &str|
         -> Result<GraphSymbol, Box<dyn Error>> {
            Ok(GraphSymbol::new(
                SymbolId::from_bytes([id; 32]),
                revision.clone(),
                ParsedSymbol::new(
                    LocalSymbolId::new(u32::from(id))?,
                    kind,
                    SymbolName::try_from_string(name.to_owned())?,
                    range,
                    range,
                )?,
            ))
        };
        let root = symbol(21, SymbolKind::Module, &files[0], "a")?;
        let method = symbol(22, SymbolKind::Function, &files[0], "start")?;
        let imported = symbol(23, SymbolKind::Function, &files[2], "target")?;
        let another_import = symbol(24, SymbolKind::Function, &files[1], "also_target")?;
        let file = |index: usize| GraphEndpoint::File(files[index].path().clone());
        let relation = |from, to, kind, revision: &FileRevision| {
            GraphEdge::new(
                from,
                to,
                kind,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                LinkResolution::ExactModuleReference,
                snapshot,
                EvidenceRef::new(revision.clone(), range),
            )
        };
        let graph = LinkedGraph::new(
            snapshot,
            files.clone(),
            vec![
                root.clone(),
                method.clone(),
                imported.clone(),
                another_import.clone(),
            ],
            vec![
                relation(
                    file(0),
                    GraphEndpoint::Symbol(root.id()),
                    SyntaxRelationKind::Defines,
                    &files[0],
                ),
                relation(
                    GraphEndpoint::Symbol(root.id()),
                    GraphEndpoint::Symbol(imported.id()),
                    SyntaxRelationKind::Imports,
                    &files[0],
                ),
                relation(file(0), file(1), SyntaxRelationKind::Imports, &files[0]),
                relation(
                    GraphEndpoint::Symbol(root.id()),
                    file(1),
                    SyntaxRelationKind::Imports,
                    &files[0],
                ),
                relation(
                    GraphEndpoint::Symbol(root.id()),
                    GraphEndpoint::Symbol(another_import.id()),
                    SyntaxRelationKind::Imports,
                    &files[0],
                ),
                relation(
                    GraphEndpoint::Symbol(method.id()),
                    file(3),
                    SyntaxRelationKind::Calls,
                    &files[0],
                ),
                relation(file(1), file(4), SyntaxRelationKind::Imports, &files[1]),
            ],
            Vec::new(),
        )?;
        let mut selected = vec![ProjectAskSourceSelection {
            revision: files[0].clone(),
            required: Some(range),
            symbol: Some(method.id()),
            require_complete: false,
        }];
        let imports = import_neighbors(&graph, &selected)?;
        assert!(
            imports
                .iter()
                .map(|neighbor| &neighbor.revision)
                .eq(&files[1..3])
        );
        assert!(imports[0].declaration.is_none());
        let declaration = imports[1]
            .declaration
            .as_ref()
            .ok_or("exact imported callable missing")?;
        assert_eq!(declaration.symbol, Some(imported.id()));
        assert_eq!(declaration.required, Some(range));
        for index in [3, 4] {
            selected.push(ProjectAskSourceSelection {
                revision: files[index].clone(),
                required: None,
                symbol: None,
                require_complete: true,
            });
        }
        assert!(matches!(
            import_neighbors(&graph, &selected),
            Err(ProjectAskSelectionFailure::Capacity)
        ));
        selected[0].revision =
            FileRevision::new(files[0].path().clone(), ContentHash::from_bytes([99; 32]));
        assert!(matches!(
            import_neighbors(&graph, &selected),
            Err(ProjectAskSelectionFailure::StalePublication)
        ));
        Ok(())
    }

    fn indexed(paths: &[&str]) -> Result<Vec<FileRevision>, Box<dyn Error>> {
        paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                Ok(FileRevision::new(
                    RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
                    ContentHash::from_bytes([u8::try_from(index)?; 32]),
                ))
            })
            .collect()
    }
}
