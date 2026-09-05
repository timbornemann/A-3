use crate::{
    FileRevision, FlowStepId, FlowStepKind, FunctionFlow, FunctionFlowError, GraphEndpoint,
    GraphSymbol, IndexPublication, MAX_INDEX_FLOW_ELEMENTS, SymbolId, SyntaxRelationKind,
};
use std::collections::{BTreeMap, BTreeSet};

/// One occurrence's target as resolved by the shared Fast Index linker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCallLink {
    /// Exact source occurrence.
    pub step: FlowStepId,
    /// Local target; absence denotes unresolved static knowledge.
    pub target: Option<SymbolId>,
}
/// Local analysis bound to a content-addressed graph symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFunctionFlow {
    symbol: SymbolId,
    revision: FileRevision,
    analysis: FunctionFlow,
    calls: Vec<FlowCallLink>,
}
impl IndexedFunctionFlow {
    /// Validates owner and exactly one target record per call occurrence.
    pub fn new(
        owner: &GraphSymbol,
        analysis: FunctionFlow,
        mut calls: Vec<FlowCallLink>,
    ) -> Result<Self, FunctionFlowError> {
        if analysis.owner() != owner.parsed().id()
            || !owner
                .parsed()
                .declaration_range()
                .contains(analysis.range())
        {
            return Err(FunctionFlowError::InvalidReference);
        }
        calls.sort_by_key(|c| c.step);
        if calls.iter().map(|c| c.step).collect::<Vec<_>>()
            != analysis
                .steps()
                .iter()
                .filter(|s| matches!(s.kind, FlowStepKind::Call | FlowStepKind::Process))
                .map(|s| s.id)
                .collect::<Vec<_>>()
        {
            return Err(FunctionFlowError::InvalidReference);
        }
        Ok(Self {
            symbol: owner.id(),
            revision: owner.revision().clone(),
            analysis,
            calls,
        })
    }
    /// Returns the content-bound callable identity.
    #[must_use]
    pub const fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// Returns its exact source revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }
    /// Returns the immutable local artifact.
    #[must_use]
    pub const fn analysis(&self) -> &FunctionFlow {
        &self.analysis
    }
    /// Returns target links including explicit unresolved occurrences.
    #[must_use]
    pub fn calls(&self) -> &[FlowCallLink] {
        &self.calls
    }
}
/// Extra payload in the existing Fast Index transaction, with no separate lifecycle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionFlowBatch(Vec<IndexedFunctionFlow>);
impl FunctionFlowBatch {
    /// Validates all references against the exact existing graph.
    pub fn new(
        publication: &IndexPublication,
        mut flows: Vec<IndexedFunctionFlow>,
    ) -> Result<Self, FunctionFlowError> {
        flows.sort_by_key(IndexedFunctionFlow::symbol);
        if flows.windows(2).any(|p| p[0].symbol == p[1].symbol) {
            return Err(FunctionFlowError::InvalidIdentity);
        }
        let graph = publication.graph();
        let symbols = graph
            .symbols()
            .iter()
            .map(|s| (s.id(), s))
            .collect::<BTreeMap<_, _>>();
        let edges = graph
            .edges()
            .iter()
            .filter_map(|edge| {
                if edge.kind() != SyntaxRelationKind::Calls {
                    return None;
                }
                match (edge.source(), edge.target()) {
                    (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) => {
                        Some((*source, edge.evidence().range(), *target))
                    }
                    _ => None,
                }
            })
            .collect::<BTreeSet<_>>();
        let mut elements = 0usize;
        for flow in &flows {
            elements = elements.saturating_add(flow.analysis.element_count());
            if elements > MAX_INDEX_FLOW_ELEMENTS {
                return Err(FunctionFlowError::Limit);
            }
            let owner = symbols
                .get(&flow.symbol)
                .ok_or(FunctionFlowError::InvalidReference)?;
            if owner.revision() != &flow.revision || owner.parsed().id() != flow.analysis.owner() {
                return Err(FunctionFlowError::InvalidReference);
            }
            for link in &flow.calls {
                if let Some(target) = link.target {
                    let step = flow
                        .analysis
                        .steps()
                        .iter()
                        .find(|s| s.id == link.step)
                        .ok_or(FunctionFlowError::InvalidReference)?;
                    let valid = if let Some(process) = &step.process {
                        process.mode != crate::FlowProcessMode::CompileOnly
                            && symbols
                                .get(&target)
                                .is_some_and(|symbol| match &process.target {
                                    Some(crate::FlowProcessTarget::File(path)) => {
                                        symbol.parsed().kind() == crate::SymbolKind::Module
                                            && symbol.revision().path() == path
                                            && symbol.parsed().declaration_range().start_byte() == 0
                                    }
                                    Some(crate::FlowProcessTarget::PackageScript(name)) => {
                                        symbol.revision().path() == owner.revision().path()
                                            && symbol.parsed().kind() == crate::SymbolKind::Function
                                            && symbol.parsed().name().as_str()
                                                == format!("scripts:{}", name.as_str())
                                    }
                                    None => false,
                                })
                    } else {
                        step.callee_range
                            .is_some_and(|range| edges.contains(&(flow.symbol, range, target)))
                    };
                    if !valid {
                        return Err(FunctionFlowError::InvalidReference);
                    }
                }
            }
        }
        Ok(Self(flows))
    }
    /// Returns canonical function artifacts for the atomic publisher.
    #[must_use]
    pub fn functions(&self) -> &[IndexedFunctionFlow] {
        &self.0
    }
}
