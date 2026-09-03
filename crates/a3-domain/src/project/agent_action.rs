use super::{
    DiscoveredCommandId, GraphEndpoint, ModuleCardClaimId, PatchAction, RepositoryPath, SymbolId,
    SyntaxRelationKind, TaskReplanReason, TaskStepBlockingReason, TaskStepId,
    TaskStepResultSummary, TraversalDepth, TraversalDirection, TraversalQuery,
    TraversalResultLimit,
};
use std::error::Error;
use std::fmt;

const MAX_AGENT_SEARCH_QUERY_BYTES: usize = 4 * 1_024;
const MAX_AGENT_TEST_SELECTOR_BYTES: usize = 1_024;
const MAX_AGENT_FILE_LINES: u16 = 500;
const MAX_AGENT_SEARCH_RESULTS: u16 = 100;

/// Version of the general structured AgentAction union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentActionSchemaVersion(u16);

impl AgentActionSchemaVersion {
    /// First read-only-phase AgentAction schema.
    pub const V1: Self = Self(1);
    /// Editing-phase schema adding structured patch and discovered-command actions.
    pub const V2: Self = Self(2);
    /// Editing-phase schema adding a non-authoritative public work note beside the action.
    pub const V3: Self = Self(3);
    /// Schema emitted for newly compiled mutating-controller turns.
    pub const CURRENT: Self = Self::V3;

    /// Reconstructs a schema version understood by this build.
    pub const fn from_u16(value: u16) -> Result<Self, AgentActionSchemaVersionError> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            3 => Ok(Self::V3),
            _ => Err(AgentActionSchemaVersionError { value }),
        }
    }

    /// Returns the stable wire representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Unknown AgentAction schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentActionSchemaVersionError {
    value: u16,
}

impl fmt::Display for AgentActionSchemaVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AgentAction schema version {} is unsupported",
            self.value
        )
    }
}

impl Error for AgentActionSchemaVersionError {}

macro_rules! agent_text_type {
    ($(#[$metadata:meta])* $name:ident, $label:literal, $maximum:expr) => {
        $(#[$metadata])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Normalizes line endings and validates one bounded non-empty value.
            pub fn try_from_string(value: String) -> Result<Self, AgentActionTextError> {
                normalize_agent_text(value, $label, $maximum).map(Self)
            }

            /// Returns text only to the concrete read-tool adapter or prompt compiler.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("bytes", &self.0.len())
                    .finish()
            }
        }
    };
}

agent_text_type!(
    /// Query passed to the deterministic fused retrieval pipeline.
    AgentSearchQuery,
    "agent search query",
    MAX_AGENT_SEARCH_QUERY_BYTES
);
agent_text_type!(
    /// Bounded test name or selector resolved by a later read-only adapter.
    AgentTestSelector,
    "agent test selector",
    MAX_AGENT_TEST_SELECTOR_BYTES
);

fn normalize_agent_text(
    value: String,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<String, AgentActionTextError> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.len() > maximum_bytes {
        return Err(AgentActionTextError {
            field,
            violation: AgentActionTextViolation::InvalidLength,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    if trimmed.chars().any(|character| {
        character == '\0' || (character.is_control() && character != '\n' && character != '\t')
    }) {
        return Err(AgentActionTextError {
            field,
            violation: AgentActionTextViolation::InvalidCharacter,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    Ok(trimmed.to_owned())
}

/// Machine-readable invalid action-text class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActionTextViolation {
    /// Text was empty or exceeded its UTF-8 byte boundary.
    InvalidLength,
    /// Text contained NUL or another unsupported control character.
    InvalidCharacter,
}

/// A search query or test selector violated its fixed text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentActionTextError {
    field: &'static str,
    violation: AgentActionTextViolation,
    actual_bytes: usize,
    maximum_bytes: usize,
}

impl AgentActionTextError {
    /// Returns the stable field label.
    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }

    /// Returns the rejected text class.
    #[must_use]
    pub const fn violation(self) -> AgentActionTextViolation {
        self.violation
    }

    /// Returns the observed UTF-8 byte count.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }

    /// Returns the field-specific maximum.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

impl fmt::Display for AgentActionTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.violation {
            AgentActionTextViolation::InvalidLength => write!(
                formatter,
                "{} has {} bytes; expected 1 through {}",
                self.field, self.actual_bytes, self.maximum_bytes
            ),
            AgentActionTextViolation::InvalidCharacter => {
                write!(
                    formatter,
                    "{} contains an unsupported character",
                    self.field
                )
            }
        }
    }
}

impl Error for AgentActionTextError {}

/// Positive result limit for one fused agent search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentSearchLimit(u16);

impl AgentSearchLimit {
    /// Creates a positive result limit capped at one hundred.
    pub const fn new(value: u16) -> Result<Self, AgentSearchLimitError> {
        if value == 0 || value > MAX_AGENT_SEARCH_RESULTS {
            Err(AgentSearchLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the requested maximum number of results.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Search result limit was zero or exceeded one hundred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSearchLimitError {
    value: u16,
}

impl fmt::Display for AgentSearchLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "agent search result limit {} must be between 1 and {MAX_AGENT_SEARCH_RESULTS}",
            self.value
        )
    }
}

impl Error for AgentSearchLimitError {}

/// Read-only search request over the deterministic retrieval pipeline.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentSearchAction {
    query: AgentSearchQuery,
    limit: AgentSearchLimit,
}

impl AgentSearchAction {
    /// Creates one bounded search without exposing channel selection to the model.
    #[must_use]
    pub const fn new(query: AgentSearchQuery, limit: AgentSearchLimit) -> Self {
        Self { query, limit }
    }

    /// Returns the bounded query.
    #[must_use]
    pub const fn query(&self) -> &AgentSearchQuery {
        &self.query
    }

    /// Returns the result limit.
    #[must_use]
    pub const fn limit(&self) -> AgentSearchLimit {
        self.limit
    }
}

impl fmt::Debug for AgentSearchAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSearchAction")
            .field("query_bytes", &self.query.as_str().len())
            .field("limit", &self.limit)
            .finish()
    }
}

/// One-based first line for a bounded file inspection page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentFileStartLine(u32);

impl AgentFileStartLine {
    /// Creates a one-based source line.
    pub const fn new(value: u32) -> Result<Self, AgentFileStartLineError> {
        if value == 0 {
            Err(AgentFileStartLineError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the one-based line.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// File inspection start line was zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentFileStartLineError;

impl fmt::Display for AgentFileStartLineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent file inspection start line must be non-zero")
    }
}

impl Error for AgentFileStartLineError {}

/// Number of source lines returned by one file inspection page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentFileLineCount(u16);

impl AgentFileLineCount {
    /// Creates a positive line count capped at five hundred.
    pub const fn new(value: u16) -> Result<Self, AgentFileLineCountError> {
        if value == 0 || value > MAX_AGENT_FILE_LINES {
            Err(AgentFileLineCountError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the maximum lines in the page.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// File inspection page length was zero or exceeded five hundred lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentFileLineCountError {
    value: u16,
}

impl fmt::Display for AgentFileLineCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "agent file inspection line count {} must be between 1 and {MAX_AGENT_FILE_LINES}",
            self.value
        )
    }
}

impl Error for AgentFileLineCountError {}

/// Bounded page within one workspace-relative repository file.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentFileInspection {
    path: RepositoryPath,
    start_line: AgentFileStartLine,
    line_count: AgentFileLineCount,
}

impl AgentFileInspection {
    /// Creates one paged file target whose canonicalization remains an adapter responsibility.
    #[must_use]
    pub const fn new(
        path: RepositoryPath,
        start_line: AgentFileStartLine,
        line_count: AgentFileLineCount,
    ) -> Self {
        Self {
            path,
            start_line,
            line_count,
        }
    }

    /// Returns the validated repository-relative path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns the first requested line.
    #[must_use]
    pub const fn start_line(&self) -> AgentFileStartLine {
        self.start_line
    }

    /// Returns the maximum lines in the page.
    #[must_use]
    pub const fn line_count(&self) -> AgentFileLineCount {
        self.line_count
    }
}

impl fmt::Debug for AgentFileInspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentFileInspection")
            .field("path_bytes", &self.path.as_bytes().len())
            .field("start_line", &self.start_line)
            .field("line_count", &self.line_count)
            .finish()
    }
}

/// One bounded evidence-graph expansion centered on a current symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGraphInspection(TraversalQuery);

impl AgentGraphInspection {
    /// Creates a typed graph query using the shared interactive traversal boundaries.
    #[must_use]
    pub const fn new(
        symbol_id: SymbolId,
        direction: TraversalDirection,
        relation: SyntaxRelationKind,
        depth: TraversalDepth,
        limit: TraversalResultLimit,
    ) -> Self {
        Self(TraversalQuery::new(
            GraphEndpoint::Symbol(symbol_id),
            direction,
            relation,
            depth,
            limit,
        ))
    }

    /// Returns the complete bounded graph traversal query.
    #[must_use]
    pub const fn query(&self) -> &TraversalQuery {
        &self.0
    }
}

/// Target of one general read-only inspect action.
#[derive(Clone, PartialEq, Eq)]
pub enum AgentInspectTarget {
    /// Read a bounded page of one repository file.
    File(AgentFileInspection),
    /// Resolve and inspect one current indexed symbol.
    Symbol(SymbolId),
    /// Expand one typed evidence-graph relation.
    Graph(AgentGraphInspection),
    /// Resolve one evidence-bound Module Card claim.
    Claim(ModuleCardClaimId),
    /// Resolve one bounded test selector without running it.
    Test(AgentTestSelector),
}

impl fmt::Debug for AgentInspectTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(target) => target.fmt(formatter),
            Self::Symbol(id) => formatter.debug_tuple("Symbol").field(id).finish(),
            Self::Graph(target) => formatter.debug_tuple("Graph").field(target).finish(),
            Self::Claim(id) => formatter.debug_tuple("Claim").field(id).finish(),
            Self::Test(selector) => formatter
                .debug_struct("Test")
                .field("selector_bytes", &selector.as_str().len())
                .finish(),
        }
    }
}

/// One targeted read-only inspection request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInspectAction {
    target: AgentInspectTarget,
}

impl AgentInspectAction {
    /// Creates one inspect action over a typed target.
    #[must_use]
    pub const fn new(target: AgentInspectTarget) -> Self {
        Self { target }
    }

    /// Returns the exact target.
    #[must_use]
    pub const fn target(&self) -> &AgentInspectTarget {
        &self.target
    }
}

/// Safe Task Ledger update intent; no variant can mark a step verified or completed.
#[derive(Clone, PartialEq, Eq)]
pub enum AgentLedgerUpdate {
    /// Retain a bounded non-authoritative result summary for later verification.
    RecordResult(TaskStepResultSummary),
    /// Report that current execution cannot continue.
    ReportBlocked(TaskStepBlockingReason),
    /// Ask the controller to enter its validated replan path.
    RequestReplan(TaskReplanReason),
}

impl fmt::Debug for AgentLedgerUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordResult(summary) => formatter
                .debug_struct("RecordResult")
                .field("summary_bytes", &summary.as_str().len())
                .finish(),
            Self::ReportBlocked(reason) => formatter
                .debug_struct("ReportBlocked")
                .field("reason_bytes", &reason.as_str().len())
                .finish(),
            Self::RequestReplan(reason) => formatter
                .debug_struct("RequestReplan")
                .field("reason_bytes", &reason.as_str().len())
                .finish(),
        }
    }
}

/// Plan-bound request to update only safe non-verification Ledger state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentUpdateLedgerAction {
    step_id: TaskStepId,
    update: AgentLedgerUpdate,
}

impl AgentUpdateLedgerAction {
    /// Binds one safe update intent to the exact current step identity.
    #[must_use]
    pub const fn new(step_id: TaskStepId, update: AgentLedgerUpdate) -> Self {
        Self { step_id, update }
    }

    /// Returns the step that a later controller must validate as current.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step_id
    }

    /// Returns the non-verifying update intent.
    #[must_use]
    pub const fn update(&self) -> &AgentLedgerUpdate {
        &self.update
    }
}

/// Unit action requesting deterministic acceptance verification before successful termination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentFinishAction;

/// Plan-bound request to execute one current, discovered, explicitly confirmed command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentRunAction {
    step_id: TaskStepId,
    command_id: DiscoveredCommandId,
}

impl AgentRunAction {
    /// Binds a command identity to the exact current Task Ledger step.
    #[must_use]
    pub const fn new(step_id: TaskStepId, command_id: DiscoveredCommandId) -> Self {
        Self {
            step_id,
            command_id,
        }
    }

    /// Returns the step that must own the active attempt.
    #[must_use]
    pub const fn step_id(self) -> TaskStepId {
        self.step_id
    }

    /// Returns the current discovered-command identity to resolve through the E5 catalog.
    #[must_use]
    pub const fn command_id(self) -> DiscoveredCommandId {
        self.command_id
    }
}

/// Closed versioned union of model-selected controller actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAction {
    /// Query deterministic retrieval without selecting a trust channel.
    Search(AgentSearchAction),
    /// Inspect one typed repository, graph, claim, or test target.
    Inspect(AgentInspectAction),
    /// Update safe Task Ledger progress without setting verification status.
    UpdateLedger(AgentUpdateLedgerAction),
    /// Request final acceptance verification; the model cannot declare success itself.
    Finish(AgentFinishAction),
    /// Apply one complete E3 patch after central policy and exact approval.
    ApplyPatch(Box<PatchAction>),
    /// Run one E5-discovered command; raw argv and shell text are not representable here.
    Run(AgentRunAction),
}

impl AgentAction {
    /// Conservatively classifies both patching and process execution as worktree mutations.
    #[must_use]
    pub const fn mutates_workspace(&self) -> bool {
        matches!(self, Self::ApplyPatch(_) | Self::Run(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_action_types_are_bounded_and_redact_untrusted_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let search = AgentAction::Search(AgentSearchAction::new(
            AgentSearchQuery::try_from_string("secret architecture query".to_owned())?,
            AgentSearchLimit::new(20)?,
        ));
        let file = AgentInspectTarget::File(AgentFileInspection::new(
            RepositoryPath::try_from_bytes(b"secret/path.rs".to_vec())?,
            AgentFileStartLine::new(1)?,
            AgentFileLineCount::new(200)?,
        ));

        assert!(!search.mutates_workspace());
        assert!(!format!("{search:?}").contains("secret architecture query"));
        assert!(!format!("{file:?}").contains("secret/path.rs"));
        assert!(AgentSearchQuery::try_from_string("x".repeat(4_097)).is_err());
        assert!(AgentTestSelector::try_from_string("\0".to_owned()).is_err());
        assert!(AgentFileStartLine::new(0).is_err());
        assert!(AgentFileLineCount::new(501).is_err());
        assert!(AgentSearchLimit::new(101).is_err());
        Ok(())
    }

    #[test]
    fn ledger_action_cannot_represent_verified_or_completed_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let action = AgentAction::UpdateLedger(AgentUpdateLedgerAction::new(
            TaskStepId::from_bytes([1; 32]),
            AgentLedgerUpdate::RecordResult(TaskStepResultSummary::try_from_string(
                "unverified model summary".to_owned(),
            )?),
        ));

        assert!(!action.mutates_workspace());
        assert!(!format!("{action:?}").contains("unverified model summary"));
        Ok(())
    }

    #[test]
    fn mutating_actions_are_plan_bound_without_raw_command_text() {
        let action = AgentAction::Run(AgentRunAction::new(
            TaskStepId::from_bytes([4; 32]),
            DiscoveredCommandId::from_bytes([5; 32]),
        ));

        assert!(action.mutates_workspace());
        assert!(matches!(action, AgentAction::Run(_)));
    }
}
