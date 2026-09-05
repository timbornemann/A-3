//! Bounded source-derived function artifacts; no runtime values or inferred execution traces.

use crate::{LocalSymbolId, SourceRange, SymbolName, SymbolReference};
use std::{collections::BTreeSet, error::Error, fmt};

/// Maximum retained analysis elements belonging to a single callable.
pub const MAX_FUNCTION_FLOW_ELEMENTS: usize = 4_096;
/// Maximum additional analysis elements in one Fast Index publication.
pub const MAX_INDEX_FLOW_ELEMENTS: usize = 2_000_000;

/// Syntactically explicit import binding used by the shared Fast Index resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticImportBinding {
    /// Local alias in its lexical scope.
    pub local: SymbolName,
    /// Literal module specifier or qualified Rust source path.
    pub module: SymbolReference,
    /// Named export; absence denotes a module/namespace import.
    pub export: Option<SymbolName>,
    /// Exact declaration evidence.
    pub range: SourceRange,
    /// Lexical availability, not a runtime initialization claim.
    pub scope: SourceRange,
}

macro_rules! local_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);
        impl $name {
            /// Validates a positive function-local identity.
            pub fn new(value: u32) -> Result<Self, FunctionFlowError> {
                if value == 0 || value as usize > MAX_FUNCTION_FLOW_ELEMENTS {
                    return Err(FunctionFlowError::InvalidIdentity);
                }
                Ok(Self(value))
            }
            /// Returns the bounded identity, meaningful only within its owner.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }
    };
}
local_id!(
    FlowStepId,
    "Identity of one occurrence, never the identity of its callee."
);
local_id!(
    FlowValueId,
    "Identity of a definition or value version inside one callable."
);

/// Source-observable operation. Branches are alternatives, not recorded executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowStepKind {
    /// Invoke a callable at this exact source location.
    Call,
    /// Statically described child-process or package-script transition.
    Process,
    /// Define or replace a binding.
    Assign,
    /// Evaluate a conditional expression.
    Condition,
    /// One alternative of a condition.
    Branch,
    /// Repeated body with no claimed iteration count.
    Loop,
    /// Normal return from this callable.
    Return,
    /// Explicit error or exception exit.
    Throw,
    /// Exit a loop.
    Break,
    /// Continue a loop.
    Continue,
    /// Await completion without claiming global execution order.
    Await,
    /// Handler or cleanup region.
    Handler,
    /// Execution depends on a callback, future, or generator consumer.
    Deferred,
    /// Unsupported operation whose effects are unknown.
    Unknown,
}

/// How a named value entered the local analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowValueKind {
    /// Formal parameter, in declaration order.
    Parameter,
    /// Local definition, including a distinct reassignment version.
    Local,
    /// Read of state whose external origin is not established.
    External,
    /// Result of one particular call occurrence.
    CallResult,
    /// Conservative join of alternative definitions.
    Merge,
    /// A statically numbered script argument, excluding executable and script name.
    ScriptArgument,
}

/// Whether a process call waits, only starts a child, or merely compiles code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowProcessMode {
    /// The invoking operation waits for child completion.
    Wait,
    /// No completion ordering is established.
    Spawn,
    /// Compilation does not execute the compiled application's entrypoint.
    CompileOnly,
}
/// Static target inside the already approved index; no filesystem access is authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowProcessTarget {
    /// Repository-relative script resolved from a statically established working directory.
    File(crate::RepositoryPath),
    /// Another named script in this exact package manifest.
    PackageScript(SymbolName),
}
/// Evidence-derived process intent, never an executable command payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowProcess {
    /// Completion relationship with its caller.
    pub mode: FlowProcessMode,
    /// Absence explicitly retains unknown working directories or dynamic targets.
    pub target: Option<FlowProcessTarget>,
    /// User-script arguments only; literal argument values are never retained.
    pub arguments: Vec<FlowArgument>,
}

/// Argument correspondence before a call target is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowArgument {
    /// Name for a Python keyword argument; absence denotes positional binding.
    pub keyword: Option<SymbolName>,
    /// Value definitions on which this argument depends.
    pub values: Vec<FlowValueId>,
    /// Exact argument source, retained without its literal value.
    pub range: SourceRange,
}

/// Draft operation validated by its enclosing FunctionFlow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowStep {
    /// Function-local occurrence identity.
    pub id: FlowStepId,
    /// Source-observable operation.
    pub kind: FlowStepKind,
    /// Enclosing conditional, loop, or handler; always an earlier step.
    pub parent: Option<FlowStepId>,
    /// Evidence supporting this operation.
    pub range: SourceRange,
    /// Identifier-only target or binding name, never an arbitrary expression body.
    pub name: Option<SymbolReference>,
    /// Exact callee evidence shared with the existing Calls graph.
    pub callee_range: Option<SourceRange>,
    /// Static process metadata only for Process operations.
    pub process: Option<FlowProcess>,
    /// Definitions read by this operation.
    pub inputs: Vec<FlowValueId>,
    /// Definitions produced by this operation.
    pub outputs: Vec<FlowValueId>,
    /// Ordered call arguments.
    pub arguments: Vec<FlowArgument>,
}

/// One local value version; definitions with identical names remain distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowValue {
    /// Function-local identity.
    pub id: FlowValueId,
    /// Source name or a fixed synthetic result label.
    pub name: SymbolName,
    /// Parameter, local definition, external state, call result, or merge.
    pub kind: FlowValueKind,
    /// Declaration or defining expression evidence.
    pub range: SourceRange,
    /// Source range of the lexical binding scope.
    pub scope: SourceRange,
    /// Earlier value versions used to construct this value.
    pub dependencies: Vec<FlowValueId>,
    /// Exact call or assignment producing the value, if known.
    pub producer: Option<FlowStepId>,
    /// Zero-based user argument slot, only for ScriptArgument definitions.
    pub script_argument: Option<u16>,
}

/// Why analysis cannot claim to cover a callable completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlowGapKind {
    /// Unsupported syntax, implicit effects, or language semantics.
    Unsupported,
    /// Ambiguous dynamic lookup or external/aliased state.
    Dynamic,
    /// Fixed structural analysis budget exhausted.
    Limit,
    /// A source parse diagnostic intersects this callable.
    ParseError,
}

/// Located, content-free explanation of incomplete static knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowGap {
    /// Closed reason suitable for safe user-facing localization.
    pub kind: FlowGapKind,
    /// Source evidence for the gap.
    pub range: SourceRange,
}

/// Validated per-callable artifact attached to the existing file parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlow {
    owner: LocalSymbolId,
    lexical_scope: SourceRange,
    range: SourceRange,
    steps: Vec<FlowStep>,
    values: Vec<FlowValue>,
    gaps: Vec<FlowGap>,
}

impl FunctionFlow {
    /// Validates containment, identity, causal references, and bounded storage.
    pub fn new(
        owner: LocalSymbolId,
        range: SourceRange,
        steps: Vec<FlowStep>,
        values: Vec<FlowValue>,
        gaps: Vec<FlowGap>,
    ) -> Result<Self, FunctionFlowError> {
        let count = steps.len()
            + values.len()
            + gaps.len()
            + steps
                .iter()
                .map(|s| {
                    s.inputs.len()
                        + s.outputs.len()
                        + s.process.as_ref().map_or(0, |p| {
                            1 + p
                                .arguments
                                .iter()
                                .map(|a| 1 + a.values.len())
                                .sum::<usize>()
                        })
                        + s.arguments
                            .iter()
                            .map(|a| 1 + a.values.len())
                            .sum::<usize>()
                })
                .sum::<usize>()
            + values.iter().map(|v| v.dependencies.len()).sum::<usize>();
        if count > MAX_FUNCTION_FLOW_ELEMENTS {
            return Err(FunctionFlowError::Limit);
        }
        let step_ids = steps.iter().map(|s| s.id).collect::<BTreeSet<_>>();
        let value_ids = values.iter().map(|v| v.id).collect::<BTreeSet<_>>();
        if step_ids.len() != steps.len()
            || value_ids.len() != values.len()
            || steps.windows(2).any(|p| p[0].id >= p[1].id)
            || values.windows(2).any(|p| p[0].id >= p[1].id)
        {
            return Err(FunctionFlowError::InvalidIdentity);
        }
        for step in &steps {
            if !range.contains(step.range) {
                return Err(FunctionFlowError::OutsideOwner);
            }
            if step.callee_range.is_some_and(|r| {
                !matches!(step.kind, FlowStepKind::Call | FlowStepKind::Process)
                    || !step.range.contains(r)
            }) {
                return Err(FunctionFlowError::InvalidReference);
            }
            if step.parent.is_some_and(|id| {
                id >= step.id
                    || !steps.iter().any(|parent| {
                        parent.id == id
                            && parent.range.contains(step.range)
                            && matches!(
                                parent.kind,
                                FlowStepKind::Condition
                                    | FlowStepKind::Branch
                                    | FlowStepKind::Loop
                                    | FlowStepKind::Handler
                                    | FlowStepKind::Deferred
                            )
                    })
            }) || step
                .inputs
                .iter()
                .chain(&step.outputs)
                .any(|id| !value_ids.contains(id))
            {
                return Err(FunctionFlowError::InvalidReference);
            }
            if (step.kind == FlowStepKind::Process) != step.process.is_some() {
                return Err(FunctionFlowError::InvalidReference);
            }
            for argument in step
                .arguments
                .iter()
                .chain(step.process.iter().flat_map(|p| &p.arguments))
            {
                if !step.range.contains(argument.range)
                    || argument.values.iter().any(|id| !value_ids.contains(id))
                {
                    return Err(FunctionFlowError::InvalidReference);
                }
            }
        }
        for value in &values {
            if (value.kind == FlowValueKind::ScriptArgument) != value.script_argument.is_some()
                || value.script_argument.is_some_and(|slot| slot >= 64)
            {
                return Err(FunctionFlowError::InvalidReference);
            }
            if !range.contains(value.range)
                || !range.contains(value.scope)
                || !value.scope.contains(value.range)
            {
                return Err(FunctionFlowError::OutsideOwner);
            }
            if value
                .dependencies
                .iter()
                .any(|id| *id >= value.id || !value_ids.contains(id))
                || value.producer.is_some_and(|id| !step_ids.contains(&id))
            {
                return Err(FunctionFlowError::InvalidReference);
            }
        }
        if gaps.iter().any(|gap| !range.contains(gap.range)) {
            return Err(FunctionFlowError::OutsideOwner);
        }
        Ok(Self {
            owner,
            lexical_scope: range,
            range,
            steps,
            values,
            gaps,
        })
    }
    /// Binds the callable declaration to its enclosing lexical scope.
    pub fn with_lexical_scope(mut self, scope: SourceRange) -> Result<Self, FunctionFlowError> {
        if !scope.contains(self.range) {
            return Err(FunctionFlowError::OutsideOwner);
        }
        self.lexical_scope = scope;
        Ok(self)
    }
    /// Returns the scope in which the callable name can be considered for resolution.
    #[must_use]
    pub const fn lexical_scope(&self) -> SourceRange {
        self.lexical_scope
    }
    /// Returns the original parsed symbol owning this artifact.
    #[must_use]
    pub const fn owner(&self) -> LocalSymbolId {
        self.owner
    }
    /// Returns the complete source extent of the callable.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }
    /// Returns operations in language evaluation order within their parent regions.
    #[must_use]
    pub fn steps(&self) -> &[FlowStep] {
        &self.steps
    }
    /// Returns local definitions, retaining reassignment and scope identity.
    #[must_use]
    pub fn values(&self) -> &[FlowValue] {
        &self.values
    }
    /// Returns known analysis gaps; an empty list is not a runtime proof.
    #[must_use]
    pub fn gaps(&self) -> &[FlowGap] {
        &self.gaps
    }
    /// Returns the count used by the aggregate publication budget.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.steps.len()
            + self.values.len()
            + self.gaps.len()
            + self
                .steps
                .iter()
                .map(|s| {
                    s.inputs.len()
                        + s.outputs.len()
                        + s.process.as_ref().map_or(0, |p| {
                            1 + p
                                .arguments
                                .iter()
                                .map(|a| 1 + a.values.len())
                                .sum::<usize>()
                        })
                        + s.arguments
                            .iter()
                            .map(|a| 1 + a.values.len())
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .values
                .iter()
                .map(|v| v.dependencies.len())
                .sum::<usize>()
    }
}

/// Invalid parser, persistence, or caller-supplied analysis data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionFlowError {
    /// Elements exceed the fixed function budget.
    Limit,
    /// Nonpositive, duplicate, unordered, or out-of-range identity.
    InvalidIdentity,
    /// An element refers to an absent or noncausal local identity.
    InvalidReference,
    /// Evidence lies outside the owning declaration.
    OutsideOwner,
}
impl fmt::Display for FunctionFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid function flow analysis")
    }
}
impl Error for FunctionFlowError {}
