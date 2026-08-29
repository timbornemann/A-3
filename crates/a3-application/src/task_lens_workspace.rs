use crate::{
    CompileTaskLens, CompileTaskLensFailure, JobContext, KnowledgeSearchStore, StoredTaskLedger,
    TaskLensClaimStore, TaskLensControl, TaskLensIndexStore,
};
use a3_domain::{
    GoalContract, GoalContractReference, ProjectIdentity, TaskId, TaskLedgerRevision, TaskLens,
    TaskLensSeedSet, TaskLensSeedText, TaskLensTokenBudget, TaskStepId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_TASK_LENS_WORKSPACE_TASKS: u16 = 100;
const MAX_TASK_LENS_ANCHOR_BYTES: usize = 4 * 1_024;

/// Owned future returned by the read-only durable Task Lens workspace boundary.
pub type TaskLensWorkspaceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, TaskLensWorkspaceFailure>> + Send + 'a>>;

/// Cooperative cancellation for bounded Goal Contract and Task Ledger reads.
pub trait TaskLensWorkspaceControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning interactive operation requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl TaskLensWorkspaceControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Maximum durable tasks returned before the UI must acknowledge truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensWorkspaceTaskLimit(u16);

impl TaskLensWorkspaceTaskLimit {
    /// Compact default suitable for one progressive desktop selector.
    pub const DEFAULT: Self = Self(20);

    /// Creates a positive list boundary capped at one hundred tasks.
    pub const fn new(value: u16) -> Result<Self, TaskLensWorkspaceTaskLimitError> {
        if value == 0 || value > MAX_TASK_LENS_WORKSPACE_TASKS {
            return Err(TaskLensWorkspaceTaskLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the portable primitive limit.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Task list boundary was zero or exceeded the fixed product cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensWorkspaceTaskLimitError {
    value: u16,
}

impl fmt::Display for TaskLensWorkspaceTaskLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Task Lens task limit {} must be between 1 and {MAX_TASK_LENS_WORKSPACE_TASKS}",
            self.value
        )
    }
}

impl Error for TaskLensWorkspaceTaskLimitError {}

/// Bounded stable-ID-ordered current Goal Contract page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensWorkspaceGoalPage {
    goals: Vec<GoalContract>,
    truncated: bool,
}

impl TaskLensWorkspaceGoalPage {
    /// Rejects unordered, duplicate, or over-limit adapter output.
    pub fn new(
        goals: Vec<GoalContract>,
        truncated: bool,
        limit: TaskLensWorkspaceTaskLimit,
    ) -> Result<Self, TaskLensWorkspaceGoalPageError> {
        if goals.len() > usize::from(limit.get()) {
            return Err(TaskLensWorkspaceGoalPageError::TooManyTasks);
        }
        if goals
            .windows(2)
            .any(|pair| pair[0].task_id() >= pair[1].task_id())
        {
            return Err(TaskLensWorkspaceGoalPageError::InvalidOrder);
        }
        Ok(Self { goals, truncated })
    }

    /// Returns current Goal Contracts in stable task-identity order.
    #[must_use]
    pub fn goals(&self) -> &[GoalContract] {
        &self.goals
    }

    /// Returns whether additional durable tasks were omitted.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Invalid durable task-page output from an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensWorkspaceGoalPageError {
    /// The adapter crossed the requested limit.
    TooManyTasks,
    /// Task identities were duplicated or not strictly increasing.
    InvalidOrder,
}

impl fmt::Display for TaskLensWorkspaceGoalPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyTasks => "Task Lens task page exceeded its requested limit",
            Self::InvalidOrder => "Task Lens task page was not in stable unique order",
        })
    }
}

impl Error for TaskLensWorkspaceGoalPageError {}

/// One atomically read current Goal Contract and optional materialized Task Ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensWorkspaceTask {
    goal_contract: GoalContract,
    task_ledger: Option<StoredTaskLedger>,
}

impl TaskLensWorkspaceTask {
    /// Binds the exact values observed inside one consistent storage snapshot.
    #[must_use]
    pub const fn new(goal_contract: GoalContract, task_ledger: Option<StoredTaskLedger>) -> Self {
        Self {
            goal_contract,
            task_ledger,
        }
    }

    /// Returns the latest Goal Contract observed for the task.
    #[must_use]
    pub const fn goal_contract(&self) -> &GoalContract {
        &self.goal_contract
    }

    /// Returns the optional latest materialized ledger from the same read transaction.
    #[must_use]
    pub const fn task_ledger(&self) -> Option<&StoredTaskLedger> {
        self.task_ledger.as_ref()
    }
}

/// Read-only storage capability for durable Task Lens anchors.
pub trait TaskLensWorkspaceStore: fmt::Debug + Send + Sync {
    /// Lists a bounded stable page of current Goal Contracts for one worktree.
    fn list_current_goal_contracts<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        limit: TaskLensWorkspaceTaskLimit,
        control: &'a dyn TaskLensWorkspaceControl,
    ) -> TaskLensWorkspaceFuture<'a, TaskLensWorkspaceGoalPage>;

    /// Atomically reads one current Goal Contract and its optional current ledger.
    fn load_current_task<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
        control: &'a dyn TaskLensWorkspaceControl,
    ) -> TaskLensWorkspaceFuture<'a, Option<TaskLensWorkspaceTask>>;
}

/// Stable failure classification for read-only durable Task Lens anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensWorkspaceFailure {
    /// Local worktree storage could not be reached.
    Unavailable,
    /// Local worktree storage failed integrity checks.
    Corrupt,
    /// The database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content or adapter ordering violated a contract.
    InvalidStoredData,
    /// The owning operation requested cooperative cancellation.
    Cancelled,
}

impl fmt::Display for TaskLensWorkspaceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Task Lens workspace storage is unavailable",
            Self::Corrupt => "Task Lens workspace storage is corrupt",
            Self::UnsupportedSchema => "Task Lens workspace storage uses an unsupported schema",
            Self::InvalidStoredData => "Task Lens workspace storage contains invalid data",
            Self::Cancelled => "Task Lens workspace read was cancelled",
        })
    }
}

impl Error for TaskLensWorkspaceFailure {}

/// Inbound use case listing current durable tasks without exposing persistence rows.
#[derive(Debug, Clone)]
pub struct ListTaskLensTasks {
    store: Arc<dyn TaskLensWorkspaceStore>,
}

impl ListTaskLensTasks {
    /// Creates the use case from its narrow read-only store.
    #[must_use]
    pub const fn new(store: Arc<dyn TaskLensWorkspaceStore>) -> Self {
        Self { store }
    }

    /// Lists at most the default twenty durable Goal Contracts.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<TaskLensWorkspaceGoalPage, TaskLensWorkspaceFailure> {
        self.store
            .list_current_goal_contracts(project, TaskLensWorkspaceTaskLimit::DEFAULT, control)
            .await
    }
}

/// Valid current Goal Contract and ledger pair used to select an active plan step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensTaskAnchor {
    goal_contract: GoalContract,
    task_ledger: StoredTaskLedger,
}

impl TaskLensTaskAnchor {
    fn new(goal_contract: GoalContract, task_ledger: StoredTaskLedger) -> Self {
        Self {
            goal_contract,
            task_ledger,
        }
    }

    /// Returns the exact current Goal Contract.
    #[must_use]
    pub const fn goal_contract(&self) -> &GoalContract {
        &self.goal_contract
    }

    /// Returns the exact current materialized Task Ledger and store version.
    #[must_use]
    pub const fn task_ledger(&self) -> &StoredTaskLedger {
        &self.task_ledger
    }
}

/// Expected availability states when selecting a durable task for a Lens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskLensTaskLoadResult {
    /// No task with this stable identity exists in the active worktree.
    NotFound,
    /// A Goal Contract exists but U5 has not created its Task Ledger yet.
    LedgerUnavailable {
        /// Current Goal Contract that proves the task exists.
        goal_contract: GoalContract,
    },
    /// The current ledger still serves an earlier Goal Contract revision.
    GoalRevisionMismatch {
        /// Current Goal Contract reference.
        current_goal: GoalContractReference,
        /// Goal Contract reference materialized by the ledger.
        ledger_goal: GoalContractReference,
    },
    /// Current revisions agree and active plan steps can be selected.
    Available(TaskLensTaskAnchor),
}

/// Inbound use case resolving one durable Task Lens anchor atomically.
#[derive(Debug, Clone)]
pub struct GetTaskLensTask {
    store: Arc<dyn TaskLensWorkspaceStore>,
}

impl GetTaskLensTask {
    /// Creates the use case from its narrow read-only store.
    #[must_use]
    pub const fn new(store: Arc<dyn TaskLensWorkspaceStore>) -> Self {
        Self { store }
    }

    /// Loads and classifies one current task without silently accepting stale plan anchors.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<TaskLensTaskLoadResult, TaskLensWorkspaceFailure> {
        let Some(task) = self
            .store
            .load_current_task(project, task_id, control)
            .await?
        else {
            return Ok(TaskLensTaskLoadResult::NotFound);
        };
        let (goal_contract, task_ledger) = (task.goal_contract, task.task_ledger);
        let Some(task_ledger) = task_ledger else {
            return Ok(TaskLensTaskLoadResult::LedgerUnavailable { goal_contract });
        };
        let current_goal = goal_contract.reference();
        let ledger_goal = task_ledger.ledger().goal_contract();
        if current_goal != ledger_goal {
            return Ok(TaskLensTaskLoadResult::GoalRevisionMismatch {
                current_goal,
                ledger_goal,
            });
        }
        Ok(TaskLensTaskLoadResult::Available(TaskLensTaskAnchor::new(
            goal_contract,
            task_ledger,
        )))
    }
}

/// One compiled Lens bound back to the exact durable task/plan/step anchors used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensCompilation {
    goal_contract: GoalContractReference,
    ledger_revision: TaskLedgerRevision,
    ledger_store_version: crate::TaskLedgerStoreVersion,
    step_id: TaskStepId,
    lens: TaskLens,
}

impl TaskLensCompilation {
    /// Returns the immutable Goal Contract revision used as the goal seed.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the plan revision used to select the current step.
    #[must_use]
    pub const fn ledger_revision(&self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the optimistic persistence version observed with the ledger.
    #[must_use]
    pub const fn ledger_store_version(&self) -> crate::TaskLedgerStoreVersion {
        self.ledger_store_version
    }

    /// Returns the exact active plan step used as the step seed.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step_id
    }

    /// Returns the deterministic current-index Task Lens.
    #[must_use]
    pub const fn lens(&self) -> &TaskLens {
        &self.lens
    }
}

/// Expected absence/staleness states or a usable deterministic Lens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileWorkspaceTaskLensResult {
    /// Durable task lookup returned no current Goal Contract.
    TaskNotFound,
    /// U5 has not created a durable Task Ledger for the task.
    LedgerUnavailable,
    /// Goal and plan revisions do not currently agree.
    GoalRevisionMismatch {
        /// Current Goal Contract reference.
        current_goal: GoalContractReference,
        /// Goal Contract reference materialized by the ledger.
        ledger_goal: GoalContractReference,
    },
    /// The requested stable step is absent or retired from the current plan.
    StepUnavailable,
    /// No atomic index publication exists yet.
    IndexUnavailable,
    /// A bounded deterministic Lens is available.
    Available(Box<TaskLensCompilation>),
}

/// Inbound use case recompiling a Lens only from reloaded current durable anchors.
#[derive(Debug, Clone)]
pub struct CompileWorkspaceTaskLens {
    task: GetTaskLensTask,
    index: Arc<dyn TaskLensIndexStore>,
    search: Arc<dyn KnowledgeSearchStore>,
    claims: Arc<dyn TaskLensClaimStore>,
}

impl CompileWorkspaceTaskLens {
    /// Composes the durable task workspace with the existing ordered R10 compiler.
    #[must_use]
    pub const fn new(
        workspace: Arc<dyn TaskLensWorkspaceStore>,
        index: Arc<dyn TaskLensIndexStore>,
        search: Arc<dyn KnowledgeSearchStore>,
        claims: Arc<dyn TaskLensClaimStore>,
    ) -> Self {
        Self {
            task: GetTaskLensTask::new(workspace),
            index,
            search,
            claims,
        }
    }

    /// Reloads current Goal/Ledger state, validates the active step, then compiles R10.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        step_id: TaskStepId,
        workspace_control: &dyn TaskLensWorkspaceControl,
        lens_control: &dyn TaskLensControl,
    ) -> Result<CompileWorkspaceTaskLensResult, CompileWorkspaceTaskLensFailure> {
        let anchor = match self
            .task
            .execute(project, task_id, workspace_control)
            .await
            .map_err(CompileWorkspaceTaskLensFailure::Workspace)?
        {
            TaskLensTaskLoadResult::NotFound => {
                return Ok(CompileWorkspaceTaskLensResult::TaskNotFound);
            }
            TaskLensTaskLoadResult::LedgerUnavailable { .. } => {
                return Ok(CompileWorkspaceTaskLensResult::LedgerUnavailable);
            }
            TaskLensTaskLoadResult::GoalRevisionMismatch {
                current_goal,
                ledger_goal,
            } => {
                return Ok(CompileWorkspaceTaskLensResult::GoalRevisionMismatch {
                    current_goal,
                    ledger_goal,
                });
            }
            TaskLensTaskLoadResult::Available(anchor) => anchor,
        };
        let ledger = anchor.task_ledger().ledger();
        let Some(step) = ledger
            .step(step_id)
            .filter(|candidate| candidate.is_active_plan_step())
        else {
            return Ok(CompileWorkspaceTaskLensResult::StepUnavailable);
        };
        let goal_seed = TaskLensSeedText::try_from_string(bounded_anchor(
            anchor.goal_contract().draft().objective().as_str(),
        ))
        .map_err(|_| CompileWorkspaceTaskLensFailure::InvalidDurableAnchor)?;
        let step_seed = TaskLensSeedText::try_from_string(bounded_anchor(
            step.definition().intended_outcome().as_str(),
        ))
        .map_err(|_| CompileWorkspaceTaskLensFailure::InvalidDurableAnchor)?;
        let seeds = TaskLensSeedSet::new(goal_seed, step_seed, Vec::new())
            .map_err(|_| CompileWorkspaceTaskLensFailure::InvalidDurableAnchor)?;
        let lens = match CompileTaskLens::new(
            self.index.as_ref(),
            self.search.as_ref(),
            self.claims.as_ref(),
        )
        .execute(project, seeds, TaskLensTokenBudget::DEFAULT, lens_control)
        .await
        {
            Ok(lens) => lens,
            Err(CompileTaskLensFailure::IndexUnavailable) => {
                return Ok(CompileWorkspaceTaskLensResult::IndexUnavailable);
            }
            Err(error) => return Err(CompileWorkspaceTaskLensFailure::Compile(error)),
        };
        Ok(CompileWorkspaceTaskLensResult::Available(Box::new(
            TaskLensCompilation {
                goal_contract: anchor.goal_contract().reference(),
                ledger_revision: ledger.revision(),
                ledger_store_version: anchor.task_ledger().version(),
                step_id,
                lens,
            },
        )))
    }
}

fn bounded_anchor(value: &str) -> String {
    if value.len() <= MAX_TASK_LENS_ANCHOR_BYTES {
        return value.to_owned();
    }
    let mut boundary = MAX_TASK_LENS_ANCHOR_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_owned()
}

/// Durable anchor loading or the existing ordered R10 compiler failed.
#[derive(Debug)]
pub enum CompileWorkspaceTaskLensFailure {
    /// Current Goal Contract/Task Ledger storage failed.
    Workspace(TaskLensWorkspaceFailure),
    /// Durable validated text could not form bounded retrieval seeds.
    InvalidDurableAnchor,
    /// Exact through claim/semantic retrieval or compilation failed.
    Compile(CompileTaskLensFailure),
}

impl fmt::Display for CompileWorkspaceTaskLensFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(error) => write!(formatter, "Task Lens workspace failed: {error}"),
            Self::InvalidDurableAnchor => {
                formatter.write_str("Task Lens durable anchor could not form a valid seed")
            }
            Self::Compile(error) => write!(formatter, "Task Lens compilation failed: {error}"),
        }
    }
}

impl Error for CompileWorkspaceTaskLensFailure {}

#[cfg(test)]
mod tests {
    use super::{
        CompileWorkspaceTaskLens, CompileWorkspaceTaskLensResult, GetTaskLensTask,
        TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
        TaskLensWorkspaceFuture, TaskLensWorkspaceGoalPage, TaskLensWorkspaceStore,
        TaskLensWorkspaceTask, TaskLensWorkspaceTaskLimit,
    };
    use crate::{
        KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture,
        KnowledgeSearchStore, StoredTaskLedger, TaskLedgerStoreVersion, TaskLensClaimLimit,
        TaskLensClaimReadFuture, TaskLensClaimResult, TaskLensClaimStore,
        TaskLensClaimStoreFailure, TaskLensClaimStoreFuture, TaskLensControl, TaskLensControlError,
        TaskLensIndexStore, TaskLensIndexStoreFuture,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
        CanonicalDirectory, ExpectedTaskEvidence, GitHead, GitReferenceName, GoalContract,
        GoalContractDraft, GoalContractTimestamp, GoalObjective, ModuleCardClaimId, Progress,
        ProjectIdentity, PublishedIndex, RepositoryId, RepositoryIdentity, SuccessVerification,
        TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId, TaskStepOutcome,
        TaskStepRationale, VerificationMethod, VerificationRequirement, VerificationSpec,
        VerificationSpecId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use std::error::Error;
    use std::sync::Arc;

    #[derive(Debug)]
    struct Control;

    impl TaskLensWorkspaceControl for Control {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    impl TaskLensControl for Control {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct Store {
        task: Option<TaskLensWorkspaceTask>,
    }

    impl TaskLensWorkspaceStore for Store {
        fn list_current_goal_contracts<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            limit: TaskLensWorkspaceTaskLimit,
            _control: &'a dyn TaskLensWorkspaceControl,
        ) -> TaskLensWorkspaceFuture<'a, TaskLensWorkspaceGoalPage> {
            Box::pin(async move {
                let goals = self
                    .task
                    .as_ref()
                    .map(|task| vec![task.goal_contract().clone()])
                    .unwrap_or_default();
                TaskLensWorkspaceGoalPage::new(goals, false, limit)
                    .map_err(|_| TaskLensWorkspaceFailure::InvalidStoredData)
            })
        }

        fn load_current_task<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task_id: TaskId,
            _control: &'a dyn TaskLensWorkspaceControl,
        ) -> TaskLensWorkspaceFuture<'a, Option<TaskLensWorkspaceTask>> {
            Box::pin(async move { Ok(self.task.clone()) })
        }
    }

    #[derive(Debug)]
    struct NoIndex;

    impl TaskLensIndexStore for NoIndex {
        fn load_current_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensIndexStoreFuture<'a> {
            Box::pin(async { Ok(None) })
        }
    }

    #[derive(Debug)]
    struct UnusedSearch;

    impl KnowledgeSearchStore for UnusedSearch {
        fn search_exact<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a a3_domain::ExactSearchQuery,
            _page_size: a3_domain::ExactSearchPageSize,
            _cursor: Option<&'a a3_domain::ExactSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, a3_domain::ExactSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::IndexUnavailable) })
        }

        fn search_lexical<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a a3_domain::LexicalSearchQuery,
            _page_size: a3_domain::LexicalSearchPageSize,
            _cursor: Option<&'a a3_domain::LexicalSearchCursor>,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, a3_domain::LexicalSearchPage> {
            Box::pin(async { Err(KnowledgeSearchFailure::IndexUnavailable) })
        }

        fn traverse_graph<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a a3_domain::TraversalQuery,
            _control: &'a dyn KnowledgeSearchControl,
        ) -> KnowledgeSearchFuture<'a, a3_domain::GraphTraversalResult> {
            Box::pin(async { Err(KnowledgeSearchFailure::IndexUnavailable) })
        }
    }

    #[derive(Debug)]
    struct EmptyClaims;

    impl TaskLensClaimStore for EmptyClaims {
        fn load_claims<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _limit: TaskLensClaimLimit,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimStoreFuture<'a> {
            Box::pin(async {
                TaskLensClaimResult::new(Vec::new(), false)
                    .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)
            })
        }

        fn load_claim<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _published: &'a PublishedIndex,
            _claim_id: ModuleCardClaimId,
            _control: &'a dyn TaskLensControl,
        ) -> TaskLensClaimReadFuture<'a> {
            Box::pin(async { Ok(None) })
        }
    }

    #[test]
    fn current_task_requires_matching_goal_and_ledger_revision() -> Result<(), Box<dyn Error>> {
        let (goal, ledger, _) = anchors()?;
        let store = Arc::new(Store {
            task: Some(TaskLensWorkspaceTask::new(
                goal.clone(),
                Some(StoredTaskLedger::new(
                    ledger,
                    TaskLedgerStoreVersion::INITIAL,
                )),
            )),
        });
        let result = futures::executor::block_on(GetTaskLensTask::new(store).execute(
            &project()?,
            goal.task_id(),
            &Control,
        ))?;
        assert!(matches!(result, TaskLensTaskLoadResult::Available(_)));
        Ok(())
    }

    #[test]
    fn compile_reloads_durable_anchors_and_reports_missing_index() -> Result<(), Box<dyn Error>> {
        let (goal, ledger, step_id) = anchors()?;
        let workspace = Arc::new(Store {
            task: Some(TaskLensWorkspaceTask::new(
                goal.clone(),
                Some(StoredTaskLedger::new(
                    ledger,
                    TaskLedgerStoreVersion::INITIAL,
                )),
            )),
        });
        let compiler = CompileWorkspaceTaskLens::new(
            workspace,
            Arc::new(NoIndex),
            Arc::new(UnusedSearch),
            Arc::new(EmptyClaims),
        );
        let result = futures::executor::block_on(compiler.execute(
            &project()?,
            goal.task_id(),
            step_id,
            &Control,
            &Control,
        ))?;
        assert_eq!(result, CompileWorkspaceTaskLensResult::IndexUnavailable);
        Ok(())
    }

    #[test]
    fn compile_rejects_a_nonexistent_step_before_retrieval() -> Result<(), Box<dyn Error>> {
        let (goal, ledger, _) = anchors()?;
        let workspace = Arc::new(Store {
            task: Some(TaskLensWorkspaceTask::new(
                goal.clone(),
                Some(StoredTaskLedger::new(
                    ledger,
                    TaskLedgerStoreVersion::INITIAL,
                )),
            )),
        });
        let compiler = CompileWorkspaceTaskLens::new(
            workspace,
            Arc::new(NoIndex),
            Arc::new(UnusedSearch),
            Arc::new(EmptyClaims),
        );
        let result = futures::executor::block_on(compiler.execute(
            &project()?,
            goal.task_id(),
            TaskStepId::from_bytes([44; 32]),
            &Control,
            &Control,
        ))?;
        assert_eq!(result, CompileWorkspaceTaskLensResult::StepUnavailable);
        Ok(())
    }

    fn anchors() -> Result<(GoalContract, TaskLedger, TaskStepId), Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes([90; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("implement durable task lens".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([91; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "lens is evidence grounded".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run task lens tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let step_id = TaskStepId::from_bytes([92; 32]);
        let step = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("show a deterministic task lens".to_owned())?,
            TaskStepRationale::try_from_string("the user selected this focus".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "current evidence metadata".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([93; 32]),
                VerificationMethod::Test,
                VerificationRequirement::try_from_string("run the contract test".to_owned())?,
            ),
        )?;
        let ledger = TaskLedger::new(
            goal.reference(),
            vec![step],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        Ok((goal, ledger, step_id))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([2; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([3; 32]),
                WorktreeAnchorId::from_bytes([4; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
            },
        )?)
    }
}
