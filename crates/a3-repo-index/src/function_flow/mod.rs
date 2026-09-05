//! Additional Fast Index analysis over the already parsed tree.

mod identity;
mod imports;
mod package;
mod process;
mod syntax;
pub(crate) use package::package_script;

pub(crate) fn link<'a>(
    publication: &a3_domain::IndexPublication,
    parses: impl Iterator<Item = &'a a3_domain::LanguageParseResult>,
    control: &dyn a3_application::RepositoryIndexControl,
    started: Instant,
) -> Result<a3_domain::FunctionFlowBatch, a3_application::RepositoryIndexCompilerFailure> {
    use a3_application::RepositoryIndexCompilerFailure as Failure;
    use a3_domain::{
        FlowCallLink, FlowStepKind, GraphEndpoint, IndexedFunctionFlow, SyntaxRelationKind,
    };
    use std::collections::{BTreeMap, BTreeSet};
    let owners = publication
        .graph()
        .symbols()
        .iter()
        .map(|s| ((s.revision().path(), s.parsed().id()), s))
        .collect::<BTreeMap<_, _>>();
    let mut targets = BTreeMap::<_, BTreeSet<_>>::new();
    for edge in publication.graph().edges() {
        crate::incremental_index::ensure_active(control, started)?;
        if edge.kind() == SyntaxRelationKind::Calls
            && let (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) =
                (edge.source(), edge.target())
        {
            targets
                .entry((*source, edge.evidence().range()))
                .or_default()
                .insert(*target);
        }
    }
    let mut modules = BTreeMap::<_, Vec<_>>::new();
    let mut scripts = BTreeMap::<_, Vec<_>>::new();
    for owner in publication.graph().symbols() {
        if owner.parsed().kind() == SymbolKind::Module
            && owner.parsed().declaration_range().start_byte() == 0
        {
            modules
                .entry(owner.revision().path())
                .or_default()
                .push(owner.id());
        }
        if owner.parsed().kind() == SymbolKind::Function
            && let Some(name) = owner.parsed().name().as_str().strip_prefix("scripts:")
        {
            scripts
                .entry((owner.revision().path(), name))
                .or_default()
                .push(owner.id());
        }
    }
    let parses = parses.collect::<Vec<_>>();
    let mut remaining = parses
        .iter()
        .map(|p| p.function_flows().len())
        .sum::<usize>();
    let mut elements = 0usize;
    let mut result = Vec::new();
    for parse in parses {
        for flow in parse.function_flows() {
            crate::incremental_index::ensure_active(control, started)?;
            remaining = remaining.saturating_sub(1);
            let limited;
            let flow = if elements
                .saturating_add(flow.element_count())
                .saturating_add(remaining)
                > a3_domain::MAX_INDEX_FLOW_ELEMENTS
            {
                limited = FunctionFlow::new(
                    flow.owner(),
                    flow.range(),
                    Vec::new(),
                    Vec::new(),
                    vec![FlowGap {
                        kind: FlowGapKind::Limit,
                        range: flow.range(),
                    }],
                )
                .and_then(|f| f.with_lexical_scope(flow.lexical_scope()))
                .map_err(|_| Failure::InvalidResult)?;
                &limited
            } else {
                flow
            };
            elements += flow.element_count();
            let owner = owners
                .get(&(parse.revision().path(), flow.owner()))
                .ok_or(Failure::InvalidResult)?;
            let calls = flow
                .steps()
                .iter()
                .filter(|s| matches!(s.kind, FlowStepKind::Call | FlowStepKind::Process))
                .map(|s| {
                    let target = if let Some(process) = &s.process {
                        if process.mode == a3_domain::FlowProcessMode::CompileOnly {
                            None
                        } else {
                            let candidates = match &process.target {
                                Some(a3_domain::FlowProcessTarget::File(path)) => modules.get(path),
                                Some(a3_domain::FlowProcessTarget::PackageScript(name)) => {
                                    scripts.get(&(owner.revision().path(), name.as_str()))
                                }
                                None => None,
                            };
                            candidates
                                .filter(|v| v.len() == 1)
                                .and_then(|v| v.first())
                                .copied()
                        }
                    } else {
                        s.callee_range
                            .and_then(|r| targets.get(&(owner.id(), r)))
                            .filter(|set| set.len() == 1)
                            .and_then(|set| set.first())
                            .copied()
                    };
                    FlowCallLink { step: s.id, target }
                })
                .collect();
            result.push(
                IndexedFunctionFlow::new(owner, flow.clone(), calls)
                    .map_err(|_| Failure::InvalidResult)?,
            );
        }
    }
    crate::incremental_index::ensure_active(control, started)?;
    a3_domain::FunctionFlowBatch::new(publication, result).map_err(|_| Failure::InvalidResult)
}

use a3_application::{LanguageParseControl, LanguageParseFailure, LanguageParseInput};
use a3_domain::{FlowGap, FlowGapKind, FunctionFlow, LanguageParseResult, SymbolKind};
use std::time::{Duration, Instant};
use tree_sitter::Tree;

pub(crate) fn attach(
    tree: &Tree,
    input: LanguageParseInput<'_>,
    parsed: LanguageParseResult,
    control: &dyn LanguageParseControl,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut flows = Vec::new();
    let mut imports = imports::collect(tree, input.source(), control, deadline)?;
    let blocked = identity::blocked_roots(tree, input.source(), &imports, control, deadline)?;
    if blocked.contains("require") {
        // CommonJS can only claim the intrinsic require, never a repository replacement.
        imports.retain(|i| {
            !tree
                .root_node()
                .descendant_for_byte_range(
                    i.range.start_byte() as usize,
                    i.range.end_byte() as usize,
                )
                .is_some_and(|mut n| {
                    while let Some(p) = n.parent() {
                        if p.kind() == "variable_declarator" {
                            return true;
                        }
                        if p.kind() == "import_statement" {
                            break;
                        }
                        n = p;
                    }
                    false
                })
        });
    }
    let mut elements = 0usize;
    for symbol in parsed.symbols() {
        if !(matches!(symbol.kind(), SymbolKind::Function | SymbolKind::Method)
            || symbol.kind() == SymbolKind::Module && symbol.declaration_range().start_byte() == 0)
        {
            continue;
        }
        if control.is_cancelled() {
            return Err(LanguageParseFailure::Cancelled);
        }
        let range = symbol.declaration_range();
        let node = tree
            .root_node()
            .descendant_for_byte_range(range.start_byte() as usize, range.end_byte() as usize);
        let Some(node) = node else {
            return Err(LanguageParseFailure::InvalidResult);
        };
        let flow = syntax::analyze(
            node,
            symbol,
            input,
            parsed.language(),
            control,
            (deadline, &imports, &blocked),
        )?;
        let flow = process::annotate(
            tree,
            input.source(),
            parsed.revision().path(),
            &imports,
            &blocked,
            flow,
        )?;
        if elements.saturating_add(flow.element_count()) + parsed.symbols().len()
            > a3_domain::MAX_INDEX_FLOW_ELEMENTS
        {
            flows.push(
                FunctionFlow::new(
                    symbol.id(),
                    range,
                    Vec::new(),
                    Vec::new(),
                    vec![FlowGap {
                        kind: FlowGapKind::Limit,
                        range,
                    }],
                )
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            );
            elements += 1;
        } else {
            elements += flow.element_count();
            flows.push(flow);
        }
    }
    parsed
        .with_function_flows(flows)
        .and_then(|p| p.with_static_imports(imports))
        .map_err(|_| LanguageParseFailure::InvalidResult)
}
