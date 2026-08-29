use crate::{JobContext, ModelProviderRequest, TaskLensControlError};
use a3_domain::{
    ContextBudgetError, ContextBudgetPlan, ContextBudgetUsage, ContextCompilerPolicyVersion,
    ContextDigest, GoalContract, GoalContractReference, IndexRunId, ModelProfile, Progress,
    ProjectIdentity, RunEventSequence, RunMemoryCheckpoint, RunMemoryDigest, SnapshotId,
    TaskLedger, TaskLedgerRevision, TaskLensDigest, TaskLensSeed, TaskStepId, ToolRunId,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_CONTEXT_SUPPLEMENTAL_SEEDS: usize = 64;
const MAX_CONTEXT_TOOL_RESULTS: usize = 64;
const MAX_CONTEXT_TOOL_PREVIEW_BYTES: usize = 16 * 1_024;

/// Future returned by the object-safe deterministic Context Compiler port.
pub type AgentContextCompilerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CompiledAgentContext, ContextCompileFailure>> + Send + 'a>>;

/// Cooperative cancellation and monotone phase progress for a complete context compile.
pub trait ContextCompileControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning controller has requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports a fixed H7 phase without exposing retrieved content.
    fn report_phase(&self, phase: ContextCompilePhase) -> Result<(), TaskLensControlError>;
}

/// Stable ordered phases of one Anchor through Validate compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContextCompilePhase {
    /// Goal, step, snapshot, profile, prompt, and budgets were validated.
    Anchor,
    /// Exact and lexical Task Lens retrieval is active.
    Retrieve,
    /// Graph, test, claim, optional semantic, and fusion ranking is active.
    Rank,
    /// Ranked zoom units and recent results are being packed.
    Pack,
    /// Final freshness, secret, budget, and digest checks are active.
    Validate,
    /// A provider-neutral request is complete.
    Complete,
}

impl ContextCompileControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_phase(&self, phase: ContextCompilePhase) -> Result<(), TaskLensControlError> {
        let completed = match phase {
            ContextCompilePhase::Anchor => 0,
            ContextCompilePhase::Retrieve => 1,
            ContextCompilePhase::Rank => 2,
            ContextCompilePhase::Pack => 3,
            ContextCompilePhase::Validate => 4,
            ContextCompilePhase::Complete => 5,
        };
        let progress =
            Progress::determinate(completed, 5).map_err(|_| TaskLensControlError::Unavailable)?;
        JobContext::report_progress(self, progress).map_err(|_| TaskLensControlError::Unavailable)
    }
}

/// Result class of one normalized read-only tool observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContextToolResultStatus {
    /// The read completed and produced the retained observation.
    Succeeded,
    /// The read failed with a normalized content-free classification.
    Failed,
    /// The read was cooperatively cancelled.
    Cancelled,
    /// Central policy denied the read before execution.
    Denied,
}

/// Digest of a complete bounded tool result whose preview may be truncated.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContextToolResultDigest([u8; 32]);

impl ContextToolResultDigest {
    /// Constructs a digest from the owning normalized tool boundary.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for ContextToolResultDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ContextToolResultDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContextToolResultDigest({self})")
    }
}

/// Bounded normalized preview retained for Context Pack selection.
#[derive(Clone, PartialEq, Eq)]
pub struct ContextToolResultPreview(String);

impl ContextToolResultPreview {
    /// Normalizes line endings and rejects empty, oversized, NUL, or unsafe control text.
    pub fn try_from_string(value: String) -> Result<Self, ContextToolResultPreviewError> {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_CONTEXT_TOOL_PREVIEW_BYTES {
            return Err(ContextToolResultPreviewError::InvalidLength(trimmed.len()));
        }
        if trimmed.chars().any(|character| {
            character == '\0' || (character.is_control() && character != '\n' && character != '\t')
        }) {
            return Err(ContextToolResultPreviewError::InvalidCharacter);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the normalized untrusted preview only to the Context Compiler.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ContextToolResultPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextToolResultPreview")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Invalid normalized tool preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextToolResultPreviewError {
    /// Preview was empty or exceeded 16 KiB after normalization.
    InvalidLength(usize),
    /// Preview contained NUL or an unsupported control character.
    InvalidCharacter,
}

impl fmt::Display for ContextToolResultPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(actual) => write!(
                formatter,
                "context tool preview has {actual} bytes; expected 1 through {MAX_CONTEXT_TOOL_PREVIEW_BYTES}"
            ),
            Self::InvalidCharacter => {
                formatter.write_str("context tool preview contains an unsupported character")
            }
        }
    }
}

impl Error for ContextToolResultPreviewError {}

/// One journal-ordered bounded read-only tool result eligible for the next Context Pack.
#[derive(Clone, PartialEq, Eq)]
pub struct ContextToolResult {
    sequence: RunEventSequence,
    tool_run_id: ToolRunId,
    status: ContextToolResultStatus,
    preview: ContextToolResultPreview,
    digest: ContextToolResultDigest,
    truncated: bool,
    snapshot_before: SnapshotId,
    snapshot_after: SnapshotId,
}

impl ContextToolResult {
    /// Binds one safe preview to its full-result digest and exact snapshot transition.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        sequence: RunEventSequence,
        tool_run_id: ToolRunId,
        status: ContextToolResultStatus,
        preview: ContextToolResultPreview,
        digest: ContextToolResultDigest,
        truncated: bool,
        snapshot_before: SnapshotId,
        snapshot_after: SnapshotId,
    ) -> Self {
        Self {
            sequence,
            tool_run_id,
            status,
            preview,
            digest,
            truncated,
            snapshot_before,
            snapshot_after,
        }
    }

    /// Returns the journal order used for recency selection.
    #[must_use]
    pub const fn sequence(&self) -> RunEventSequence {
        self.sequence
    }

    /// Returns the stable owning tool-run identity.
    #[must_use]
    pub const fn tool_run_id(&self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the normalized result classification.
    #[must_use]
    pub const fn status(&self) -> ContextToolResultStatus {
        self.status
    }

    /// Returns the bounded untrusted preview.
    #[must_use]
    pub const fn preview(&self) -> &ContextToolResultPreview {
        &self.preview
    }

    /// Returns the complete-result digest.
    #[must_use]
    pub const fn digest(&self) -> ContextToolResultDigest {
        self.digest
    }

    /// Returns whether a tail was omitted from the preview.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Returns the snapshot observed before the read.
    #[must_use]
    pub const fn snapshot_before(&self) -> SnapshotId {
        self.snapshot_before
    }

    /// Returns the snapshot observed after the read.
    #[must_use]
    pub const fn snapshot_after(&self) -> SnapshotId {
        self.snapshot_after
    }
}

impl fmt::Debug for ContextToolResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextToolResult")
            .field("sequence", &self.sequence)
            .field("tool_run_id", &self.tool_run_id)
            .field("status", &self.status)
            .field("preview_bytes", &self.preview.as_str().len())
            .field("digest", &self.digest)
            .field("truncated", &self.truncated)
            .field("snapshot_before", &self.snapshot_before)
            .field("snapshot_after", &self.snapshot_after)
            .finish()
    }
}

/// Fully typed input for one newly compiled model turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextCompileInput {
    project: ProjectIdentity,
    goal_contract: GoalContract,
    task_ledger: TaskLedger,
    current_step_id: TaskStepId,
    model_profile: ModelProfile,
    run_memory: Option<RunMemoryCheckpoint>,
    supplemental_seeds: Vec<TaskLensSeed>,
    tool_results: Vec<ContextToolResult>,
}

impl AgentContextCompileInput {
    /// Validates durable goal/ledger ownership and canonical bounded optional inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project: ProjectIdentity,
        goal_contract: GoalContract,
        task_ledger: TaskLedger,
        current_step_id: TaskStepId,
        model_profile: ModelProfile,
        run_memory: Option<RunMemoryCheckpoint>,
        mut supplemental_seeds: Vec<TaskLensSeed>,
        mut tool_results: Vec<ContextToolResult>,
    ) -> Result<Self, AgentContextCompileInputError> {
        if task_ledger.goal_contract() != goal_contract.reference() {
            return Err(AgentContextCompileInputError::GoalLedgerMismatch);
        }
        let current_step = task_ledger
            .step(current_step_id)
            .ok_or(AgentContextCompileInputError::CurrentStepUnavailable)?;
        if !current_step.is_active_plan_step() {
            return Err(AgentContextCompileInputError::CurrentStepRetired);
        }
        if run_memory.as_ref().is_some_and(|memory| {
            memory.goal_contract() != goal_contract.reference()
                || memory.ledger_revision() != task_ledger.revision()
        }) {
            return Err(AgentContextCompileInputError::RunMemoryMismatch);
        }
        if supplemental_seeds.len() > MAX_CONTEXT_SUPPLEMENTAL_SEEDS {
            return Err(AgentContextCompileInputError::TooManySupplementalSeeds(
                supplemental_seeds.len(),
            ));
        }
        supplemental_seeds.sort();
        if supplemental_seeds.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AgentContextCompileInputError::DuplicateSupplementalSeed);
        }
        if tool_results.len() > MAX_CONTEXT_TOOL_RESULTS {
            return Err(AgentContextCompileInputError::TooManyToolResults(
                tool_results.len(),
            ));
        }
        tool_results.sort_by_key(ContextToolResult::sequence);
        if tool_results
            .windows(2)
            .any(|pair| pair[0].sequence() == pair[1].sequence())
        {
            return Err(AgentContextCompileInputError::DuplicateToolSequence);
        }
        let tool_ids = tool_results
            .iter()
            .map(ContextToolResult::tool_run_id)
            .collect::<BTreeSet<_>>();
        if tool_ids.len() != tool_results.len() {
            return Err(AgentContextCompileInputError::DuplicateToolRun);
        }
        Ok(Self {
            project,
            goal_contract,
            task_ledger,
            current_step_id,
            model_profile,
            run_memory,
            supplemental_seeds,
            tool_results,
        })
    }

    /// Returns the exact project whose current publication must be retrieved.
    #[must_use]
    pub const fn project(&self) -> &ProjectIdentity {
        &self.project
    }

    /// Returns the immutable Goal Contract revision carried in full.
    #[must_use]
    pub const fn goal_contract(&self) -> &GoalContract {
        &self.goal_contract
    }

    /// Returns the matching durable ledger materialization.
    #[must_use]
    pub const fn task_ledger(&self) -> &TaskLedger {
        &self.task_ledger
    }

    /// Returns the explicit current active-plan step.
    #[must_use]
    pub const fn current_step_id(&self) -> TaskStepId {
        self.current_step_id
    }

    /// Returns the exact run-shaping profile.
    #[must_use]
    pub const fn model_profile(&self) -> &ModelProfile {
        &self.model_profile
    }

    /// Returns optional H8 memory rebuilt from the same Goal and Ledger revision.
    #[must_use]
    pub const fn run_memory(&self) -> Option<&RunMemoryCheckpoint> {
        self.run_memory.as_ref()
    }

    /// Returns canonical supplemental Task Lens seeds.
    #[must_use]
    pub fn supplemental_seeds(&self) -> &[TaskLensSeed] {
        &self.supplemental_seeds
    }

    /// Returns journal-ordered recent normalized tool results.
    #[must_use]
    pub fn tool_results(&self) -> &[ContextToolResult] {
        &self.tool_results
    }
}

/// Invalid cross-aggregate or optional Context Compiler input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentContextCompileInputError {
    /// Ledger serves another Goal Contract revision.
    GoalLedgerMismatch,
    /// Requested current step is absent.
    CurrentStepUnavailable,
    /// Requested step belongs only to replan history.
    CurrentStepRetired,
    /// Optional run memory belongs to another Goal or Ledger revision.
    RunMemoryMismatch,
    /// More than 64 supplemental retrieval seeds were supplied.
    TooManySupplementalSeeds(usize),
    /// Supplemental seed set contained a duplicate.
    DuplicateSupplementalSeed,
    /// More than 64 recent normalized tool results were supplied.
    TooManyToolResults(usize),
    /// Two tool observations claimed the same journal position.
    DuplicateToolSequence,
    /// One ToolRunId appeared more than once.
    DuplicateToolRun,
}

impl fmt::Display for AgentContextCompileInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::GoalLedgerMismatch => "context goal and Task Ledger revision do not match",
            Self::CurrentStepUnavailable => "context current step is absent from the Task Ledger",
            Self::CurrentStepRetired => "context current step was retired by a replan",
            Self::RunMemoryMismatch => "context run memory does not match Goal and Task Ledger",
            Self::TooManySupplementalSeeds(_) => "context has too many supplemental seeds",
            Self::DuplicateSupplementalSeed => "context has a duplicate supplemental seed",
            Self::TooManyToolResults(_) => "context has too many recent tool results",
            Self::DuplicateToolSequence => "context repeats a tool-result journal sequence",
            Self::DuplicateToolRun => "context repeats a ToolRunId",
        })
    }
}

impl Error for AgentContextCompileInputError {}

/// Complete validated Context Pack and provider-neutral request for exactly one turn.
pub struct CompiledAgentContext {
    request: ModelProviderRequest,
    policy_version: ContextCompilerPolicyVersion,
    digest: ContextDigest,
    goal_contract: GoalContractReference,
    ledger_revision: TaskLedgerRevision,
    current_step_id: TaskStepId,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    task_lens_digest: TaskLensDigest,
    run_memory_digest: Option<RunMemoryDigest>,
    budget_plan: ContextBudgetPlan,
    budget_usage: ContextBudgetUsage,
    excluded_stale_claims: u16,
    truncated: bool,
}

impl CompiledAgentContext {
    /// Assembles already validated feature output at the Application boundary.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        request: ModelProviderRequest,
        policy_version: ContextCompilerPolicyVersion,
        digest: ContextDigest,
        goal_contract: GoalContractReference,
        ledger_revision: TaskLedgerRevision,
        current_step_id: TaskStepId,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        task_lens_digest: TaskLensDigest,
        run_memory_digest: Option<RunMemoryDigest>,
        budget_plan: ContextBudgetPlan,
        budget_usage: ContextBudgetUsage,
        excluded_stale_claims: u16,
        truncated: bool,
    ) -> Self {
        Self {
            request,
            policy_version,
            digest,
            goal_contract,
            ledger_revision,
            current_step_id,
            index_run_id,
            snapshot_id,
            task_lens_digest,
            run_memory_digest,
            budget_plan,
            budget_usage,
            excluded_stale_claims,
            truncated,
        }
    }

    /// Returns the request ready for the neutral ModelProvider port.
    #[must_use]
    pub const fn request(&self) -> &ModelProviderRequest {
        &self.request
    }

    /// Moves the provider request into the owning controller turn.
    #[must_use]
    pub fn into_request(self) -> ModelProviderRequest {
        self.request
    }

    /// Returns versioned packing and digest semantics.
    #[must_use]
    pub const fn policy_version(&self) -> ContextCompilerPolicyVersion {
        self.policy_version
    }

    /// Returns the digest of normalized prompt content and governing state.
    #[must_use]
    pub const fn digest(&self) -> ContextDigest {
        self.digest
    }

    /// Returns the exact Goal Contract revision present in the anchor.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the exact Task Ledger revision present in the anchor.
    #[must_use]
    pub const fn ledger_revision(&self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the current step present in the anchor.
    #[must_use]
    pub const fn current_step_id(&self) -> TaskStepId {
        self.current_step_id
    }

    /// Returns the exact current published index run selected by retrieval.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the exact immutable snapshot present in the anchor.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the upstream Task Lens identity.
    #[must_use]
    pub const fn task_lens_digest(&self) -> TaskLensDigest {
        self.task_lens_digest
    }

    /// Returns the exact H8 checkpoint included in the request, when one was supplied.
    #[must_use]
    pub const fn run_memory_digest(&self) -> Option<RunMemoryDigest> {
        self.run_memory_digest
    }

    /// Returns scaled hard area ceilings and reserves.
    #[must_use]
    pub const fn budget_plan(&self) -> ContextBudgetPlan {
        self.budget_plan
    }

    /// Returns actual deterministic area costs.
    #[must_use]
    pub const fn budget_usage(&self) -> ContextBudgetUsage {
        self.budget_usage
    }

    /// Returns stale/incompatible claims excluded by the trusted Task Lens.
    #[must_use]
    pub const fn excluded_stale_claims(&self) -> u16 {
        self.excluded_stale_claims
    }

    /// Returns whether any ranked or tool-result tail was omitted under a hard boundary.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for CompiledAgentContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompiledAgentContext")
            .field("request", &self.request)
            .field("policy_version", &self.policy_version)
            .field("digest", &self.digest)
            .field("goal_contract", &self.goal_contract)
            .field("ledger_revision", &self.ledger_revision)
            .field("current_step_id", &self.current_step_id)
            .field("index_run_id", &self.index_run_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("task_lens_digest", &self.task_lens_digest)
            .field("run_memory_digest", &self.run_memory_digest)
            .field("budget_plan", &self.budget_plan)
            .field("budget_usage", &self.budget_usage)
            .field("excluded_stale_claims", &self.excluded_stale_claims)
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// Inbound port implemented by the deterministic `a3-context` feature.
pub trait AgentContextCompiler: fmt::Debug + Send + Sync {
    /// Compiles a fresh context and provider request without invoking a model or tool.
    fn compile<'a>(
        &'a self,
        input: &'a AgentContextCompileInput,
        control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a>;
}

/// Stable content-free failure classes at the Context Compiler boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCompileFailure {
    /// Prompt capability or static contract preparation failed.
    PromptUnavailable,
    /// Scaled area or output-reserve planning failed.
    Budget(ContextBudgetError),
    /// No current atomically published index exists.
    IndexUnavailable,
    /// Deterministic retrieval, fusion, or Task Lens compilation failed.
    RetrievalFailed,
    /// Goal, step, lens, profile, or snapshot bindings did not agree.
    StaleOrMismatchedInput,
    /// Unabridged mandatory anchor exceeded its hard area allowance.
    AnchorTooLarge,
    /// A likely secret marker was found before provider request construction.
    SecretCandidate,
    /// Normalized context or provider request violated a fixed boundary.
    InvalidPack,
    /// The owning operation requested cancellation.
    Cancelled,
    /// Ordered retrieval exceeded its whole-operation deadline.
    TimedOut,
    /// Monotone progress could not reach the owning runtime.
    ProgressUnavailable,
}

impl fmt::Display for ContextCompileFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PromptUnavailable => formatter.write_str("agent prompt is unavailable"),
            Self::Budget(error) => write!(formatter, "Context Pack budget is invalid: {error}"),
            Self::IndexUnavailable => formatter.write_str("Context Pack requires a current index"),
            Self::RetrievalFailed => formatter.write_str("Context Pack retrieval failed"),
            Self::StaleOrMismatchedInput => {
                formatter.write_str("Context Pack input is stale or mismatched")
            }
            Self::AnchorTooLarge => {
                formatter.write_str("Context Pack mandatory anchor exceeds its budget")
            }
            Self::SecretCandidate => formatter.write_str("Context Pack contains a possible secret"),
            Self::InvalidPack => formatter.write_str("Context Pack violates its schema"),
            Self::Cancelled => formatter.write_str("Context Pack compilation was cancelled"),
            Self::TimedOut => formatter.write_str("Context Pack compilation timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("Context Pack progress is unavailable")
            }
        }
    }
}

impl Error for ContextCompileFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Budget(error) => Some(error),
            Self::PromptUnavailable
            | Self::IndexUnavailable
            | Self::RetrievalFailed
            | Self::StaleOrMismatchedInput
            | Self::AnchorTooLarge
            | Self::SecretCandidate
            | Self::InvalidPack
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextToolResultPreview, ContextToolResultPreviewError};
    use std::error::Error;

    #[test]
    fn tool_previews_are_normalized_bounded_and_redacted() -> Result<(), Box<dyn Error>> {
        let preview = ContextToolResultPreview::try_from_string("  private\r\nvalue  ".to_owned())?;
        assert_eq!(preview.as_str(), "private\nvalue");
        assert!(!format!("{preview:?}").contains("private"));
        assert_eq!(
            ContextToolResultPreview::try_from_string("\0".to_owned()),
            Err(ContextToolResultPreviewError::InvalidCharacter)
        );
        Ok(())
    }
}
