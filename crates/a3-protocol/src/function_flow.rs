//! Versioned, bounded function-flow exploration IPC. Never accepts paths, SQL or executable commands.
use crate::{ProjectMapEntitySelectionV1, ProjectMapIndexEvidenceSelectionV1, ProtocolVersion};
use serde::{Deserialize, Serialize};
/// Content-bound root and occurrence path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowSelectionV1 {
    /// Current published Fast Index run.
    pub run_id: String,
    /// Content-bound root symbol.
    pub root: String,
    /// At most seven call occurrences from the root.
    pub call_path: Vec<u32>,
}
/// Strict read-only request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryFunctionFlowsRequestV1 {
    /// IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Closed bounded query.
    pub query: FunctionFlowQueryV1,
}
/// Versioned read result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowsResponseV1 {
    /// IPC protocol version.
    pub protocol_version: ProtocolVersion,
    /// Mutually exclusive availability or payload.
    pub result: FunctionFlowsResultV1,
}
/// Safe source metadata and existing preview/map selections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowSourceV1 {
    /// Repository-relative display path.
    pub path: String,
    /// One-based source line.
    pub line: u32,
    /// Core-issued existing source-preview selection.
    pub preview: Option<ProjectMapIndexEvidenceSelectionV1>,
    /// Core-issued selection for the existing code map.
    pub map_selection: Option<ProjectMapEntitySelectionV1>,
}
/// One function, test or script entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowEntryV1 {
    /// Exact callable selection.
    pub selection: FunctionFlowSelectionV1,
    /// Source-derived name.
    pub name: String,
    /// User-facing entry group.
    pub category: FunctionFlowCategoryV1,
    /// Current source evidence.
    pub source: FunctionFlowSourceV1,
}
/// Fixed-size callable inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowPageV1 {
    /// At most fifty entries.
    pub entries: Vec<FunctionFlowEntryV1>,
    /// A following page exists.
    pub has_more: bool,
}
/// One operation in evaluation order, never a runtime trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowStepV1 {
    /// Process completion relationship; absent for ordinary operations.
    pub process_mode: Option<FunctionFlowProcessModeV1>,
    /// Additional local input/output references were omitted by the response budget.
    pub values_truncated: bool,
    /// Occurrence identity inside its function.
    pub id: u32,
    /// Enclosing condition, alternative or repetition.
    pub parent: Option<u32>,
    /// Source-observable operation.
    pub kind: FunctionFlowStepKindV1,
    /// Identifier-only target or binding.
    pub name: Option<String>,
    /// One-based evidence line.
    pub line: u32,
    /// Expandable exact callee context.
    pub target: Option<FunctionFlowSelectionV1>,
    /// Local value versions consumed.
    pub inputs: Vec<u32>,
    /// Local value versions produced.
    pub outputs: Vec<u32>,
}
/// Statically known completion relationship; no process is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowProcessModeV1 {
    /// Invocation waits for child completion.
    Wait,
    /// Invocation only starts a child; ordering is not established.
    Spawn,
    /// Compilation is not application execution.
    CompileOnly,
}
/// One local definition, not a runtime variable dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowValueV1 {
    /// Local value-version identity.
    pub id: u32,
    /// Binding name or synthetic result label.
    pub name: String,
    /// Definition origin.
    pub kind: FunctionFlowValueKindV1,
    /// One-based definition line.
    pub line: u32,
}
/// Bounded detail for a selected function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowViewV1 {
    /// Current occurrence context.
    pub selection: FunctionFlowSelectionV1,
    /// Callable name.
    pub name: String,
    /// Current source evidence.
    pub source: FunctionFlowSourceV1,
    /// Root through selected function.
    pub breadcrumbs: Vec<FunctionFlowEntryV1>,
    /// At most fifty operations.
    pub steps: Vec<FunctionFlowStepV1>,
    /// At most fifty local definitions.
    pub values: Vec<FunctionFlowValueV1>,
    /// Total retained operations.
    pub step_total: u32,
    /// Total retained definitions.
    pub value_total: u32,
    /// Bounded explicit analysis gaps.
    pub gaps: Vec<FunctionFlowGapV1>,
    /// More gaps exist than displayed.
    pub gaps_truncated: bool,
}
/// An explicit analysis boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowGapV1 {
    /// Why the analysis is incomplete.
    pub kind: FunctionFlowGapKindV1,
    /// One-based source line.
    pub line: u32,
}
/// One value in one distinct call context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowTraceNodeV1 {
    /// Occurrence-specific selection.
    pub selection: FunctionFlowSelectionV1,
    /// Local definition.
    pub value: FunctionFlowValueV1,
    /// Source-derived callable name.
    pub function_name: String,
    /// Source-relative display path.
    pub path: String,
    /// External or partially modeled effects remain.
    pub unknown: bool,
}
/// Finite interprocedural value trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionFlowTraceV1 {
    /// Requested traversal direction.
    pub direction: FunctionFlowTraceDirectionV1,
    /// At most fifty distinct occurrences.
    pub nodes: Vec<FunctionFlowTraceNodeV1>,
    /// A fixed query budget stopped exploration.
    pub truncated: bool,
}
/// Entry group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowCategoryV1 {
    /// Function classification.
    Function,
    /// Test classification.
    Test,
    /// Entrypoint classification.
    Entrypoint,
    /// Script classification.
    Script,
}
/// Closed source-observable operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowStepKindV1 {
    /// Static process transition; never executed by this analysis.
    Process,
    /// Call classification.
    Call,
    /// Assign classification.
    Assign,
    /// Condition classification.
    Condition,
    /// Branch classification.
    Branch,
    /// Loop classification.
    Loop,
    /// Return classification.
    Return,
    /// Throw classification.
    Throw,
    /// Break classification.
    Break,
    /// Continue classification.
    Continue,
    /// Await classification.
    Await,
    /// Handler classification.
    Handler,
    /// Deferred classification.
    Deferred,
    /// Unknown classification.
    Unknown,
}
/// Closed definition kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowValueKindV1 {
    /// Statically numbered script argument.
    ScriptArgument,
    /// Parameter classification.
    Parameter,
    /// Local classification.
    Local,
    /// External classification.
    External,
    /// CallResult classification.
    CallResult,
    /// Merge classification.
    Merge,
}
/// Closed analysis gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowGapKindV1 {
    /// Unsupported classification.
    Unsupported,
    /// Dynamic classification.
    Dynamic,
    /// Limit classification.
    Limit,
    /// ParseError classification.
    ParseError,
}
/// Value traversal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFlowTraceDirectionV1 {
    /// Origins classification.
    Origins,
    /// Uses classification.
    Uses,
}
/// Closed input operations with fixed server-side bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum FunctionFlowQueryV1 {
    /// Preview a live-verified occurrence without accepting a path or source range.
    Source {
        /// Current run and exact call context.
        selection: FunctionFlowSelectionV1,
        /// Function-local step identity.
        step: u32,
    },
    /// Search a fixed fifty-entry page.
    Catalog {
        /// Bounded name/path text.
        term: String,
        /// Canonical page offset.
        offset: u32,
    },
    /// Inspect one exact call context.
    Inspect {
        /// Core-issued root and occurrence path.
        selection: FunctionFlowSelectionV1,
        /// First operation, multiple of fifty.
        step_offset: u32,
        /// First value, multiple of fifty.
        value_offset: u32,
    },
    /// Trace one local value in its call context.
    Trace {
        /// Core-issued root and occurrence path.
        selection: FunctionFlowSelectionV1,
        /// Positive bounded local value identity.
        value: u32,
        /// Origins or uses.
        direction: FunctionFlowTraceDirectionV1,
    },
}
/// Closed output alternatives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum FunctionFlowsResultV1 {
    /// Bounded plain-text source using the existing safe preview contract.
    Source {
        /// Live-hash-verified source page.
        preview: crate::ProjectMapSourcePreviewV1,
    },
    /// No active worktree.
    NoProject,
    /// No current flow-capable index.
    NoPublishedIndex,
    /// The source snapshot or selected occurrence changed.
    SelectionChanged,
    /// One callable inventory page.
    Catalog {
        /// Bounded page.
        page: FunctionFlowPageV1,
    },
    /// One selected callable.
    Flow {
        /// Bounded step and value pages.
        flow: Box<FunctionFlowViewV1>,
    },
    /// One finite value trace.
    Trace {
        /// Bounded context-sensitive trace.
        trace: FunctionFlowTraceV1,
    },
}
