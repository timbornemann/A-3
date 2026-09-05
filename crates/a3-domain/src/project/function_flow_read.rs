use crate::{FlowStepId, FlowValueId, FunctionFlowError, SymbolId};

/// Fixed-page, read-only view into an indexed function context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FunctionFlowReadView {
    /// Fifty operations, starting at the validated offset.
    Steps(u16),
    /// Fifty local value versions, starting at the validated offset.
    Values(u16),
    /// Bounded context-sensitive origins of one local value.
    Origins(FlowValueId),
    /// Bounded context-sensitive uses of one local value.
    Uses(FlowValueId),
}
/// Validated query; its caller must pin it to the current published run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionFlowReadRequest {
    root: SymbolId,
    call_path: Vec<FlowStepId>,
    view: FunctionFlowReadView,
}
impl FunctionFlowReadRequest {
    /// Enforces the same context and page boundaries as the interactive explorer.
    pub fn new(
        root: SymbolId,
        call_path: Vec<FlowStepId>,
        view: FunctionFlowReadView,
    ) -> Result<Self, FunctionFlowError> {
        if call_path.len() >= 8
            || matches!(view,FunctionFlowReadView::Steps(n)|FunctionFlowReadView::Values(n) if n>4050 || n%50!=0)
        {
            return Err(FunctionFlowError::InvalidReference);
        }
        Ok(Self {
            root,
            call_path,
            view,
        })
    }
    /// Content-bound root symbol.
    #[must_use]
    pub const fn root(&self) -> SymbolId {
        self.root
    }
    /// Exact call occurrences, not callee identities.
    #[must_use]
    pub fn call_path(&self) -> &[FlowStepId] {
        &self.call_path
    }
    /// Selected bounded view.
    #[must_use]
    pub const fn view(&self) -> FunctionFlowReadView {
        self.view
    }
}
