use crate::{
    AdvanceAgentController, AgentActionStore, AgentActionStoreFailure, AgentActivity,
    AgentActivityLoadResult, AgentControllerSignal, AgentInspectionContext,
    AgentProcessInspectionKind, GetAgentActivity, GetAgentActivityFailure, GrantPolicyApproval,
    GrantPolicyApprovalError, PolicyStore, PolicyStoreFailure, RevokePolicyApproval,
    RevokePolicyApprovalError, RunJournalStore, TaskLedgerStoreVersion, TaskLensWorkspaceControl,
    TaskLensWorkspaceStore,
};
use a3_domain::{
    ActionClass, AgentControllerState, AgentRun, AgentRunTimestamp, ApprovalGrant, ApprovalId,
    ApprovalRequest, ApprovalRequestId, ApprovalStatus, PatchAction, PatchOperation, PolicyAction,
    PolicyActionFingerprint, PolicyDecisionReason, PolicyResourceId, PolicyScopeDigest,
    ProcessExecutionMode, ProcessNetworkScope, ProcessPlanBinding, ProcessSpec, ProjectIdentity,
    RepositoryPath, RiskLevel, RunEventId, TaskId, TaskLedger, TaskLedgerTimestamp,
    TaskStepBlockingReason, TaskStepStatus, WorkspaceDirectory, WorktreeId,
};
use std::error::Error;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

const USER_DENIAL_REASON: &str = "user denied the exact mutation approval";

/// Positive process-local revision binding one displayed approval to a later user decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentApprovalRevision(u64);

impl AgentApprovalRevision {
    /// Reconstructs an untrusted positive revision before record-level revalidation.
    pub const fn new(value: u64) -> Result<Self, AgentApprovalRevisionError> {
        if value == 0 {
            return Err(AgentApprovalRevisionError);
        }
        Ok(Self(value))
    }

    /// Returns the positive process-local value used by strict IPC.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A WebView-supplied approval revision was not positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentApprovalRevisionError;

impl fmt::Display for AgentApprovalRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent approval revision must be positive")
    }
}

impl Error for AgentApprovalRevisionError {}

/// Closed operation label for one exact path in a pending E3 action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalFileOperation {
    /// Create one previously absent file.
    Add,
    /// Replace one exact current file revision.
    Update,
    /// Move one exact current revision to one absent target.
    Move,
    /// Delete one exact current file revision.
    Delete,
}

/// Exact source and target scope of one pending patch operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentApprovalFile {
    operation: AgentApprovalFileOperation,
    source_path: Option<RepositoryPath>,
    target_path: Option<RepositoryPath>,
}

impl AgentApprovalFile {
    fn from_operation(operation: &PatchOperation) -> Self {
        match operation {
            PatchOperation::Add(add) => Self {
                operation: AgentApprovalFileOperation::Add,
                source_path: None,
                target_path: Some(add.path().clone()),
            },
            PatchOperation::Update(update) => Self {
                operation: AgentApprovalFileOperation::Update,
                source_path: Some(update.expected().path().clone()),
                target_path: Some(update.expected().path().clone()),
            },
            PatchOperation::Move(movement) => Self {
                operation: AgentApprovalFileOperation::Move,
                source_path: Some(movement.expected().path().clone()),
                target_path: Some(movement.destination().clone()),
            },
            PatchOperation::Delete(expected) => Self {
                operation: AgentApprovalFileOperation::Delete,
                source_path: Some(expected.path().clone()),
                target_path: None,
            },
        }
    }

    /// Returns Add, Update, Move, or Delete.
    #[must_use]
    pub const fn operation(&self) -> AgentApprovalFileOperation {
        self.operation
    }

    /// Returns the exact current path for Update, Move, or Delete.
    #[must_use]
    pub const fn source_path(&self) -> Option<&RepositoryPath> {
        self.source_path.as_ref()
    }

    /// Returns the exact proposed path for Add, Update, or Move.
    #[must_use]
    pub const fn target_path(&self) -> Option<&RepositoryPath> {
        self.target_path.as_ref()
    }
}

/// Exact bounded patch scope displayed before a one-time approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentApprovalPatch {
    rationale: String,
    files: Vec<AgentApprovalFile>,
}

impl AgentApprovalPatch {
    fn from_action(action: &PatchAction) -> Self {
        Self {
            rationale: action.rationale().as_str().to_owned(),
            files: action
                .operations()
                .iter()
                .map(AgentApprovalFile::from_operation)
                .collect(),
        }
    }

    /// Returns the normalized, bounded, secret-checked E3 rationale.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    /// Returns every operation in canonical E3 order.
    #[must_use]
    pub fn files(&self) -> &[AgentApprovalFile] {
        &self.files
    }
}

/// Exact worktree-relative current working directory of one pending process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentApprovalWorkingDirectory {
    /// Selected worktree root.
    Root,
    /// Exact repository-relative subtree.
    Subtree(RepositoryPath),
}

impl AgentApprovalWorkingDirectory {
    fn from_directory(value: &WorkspaceDirectory) -> Self {
        match value {
            WorkspaceDirectory::Root => Self::Root,
            WorkspaceDirectory::Subtree(path) => Self::Subtree(path.clone()),
        }
    }

    /// Returns the exact relative subtree or `None` for the root.
    #[must_use]
    pub const fn path(&self) -> Option<&RepositoryPath> {
        match self {
            Self::Root => None,
            Self::Subtree(path) => Some(path),
        }
    }
}

/// Declarative network boundary shown without inventing OS sandboxing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalNetworkScope {
    /// The specification declares network access denied.
    Denied,
    /// The specification requests one exact content-free target.
    Requested(PolicyResourceId),
}

/// Full validated direct-argv process specification displayed before approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentApprovalProcess {
    kind: AgentProcessInspectionKind,
    executable: String,
    arguments: Vec<String>,
    working_directory: AgentApprovalWorkingDirectory,
    environment_allowlist: Vec<String>,
    timeout_millis: u64,
    stdout_limit: u32,
    stderr_limit: u32,
    execution_mode: ProcessExecutionMode,
    plan_binding: ProcessPlanBinding,
    network: AgentApprovalNetworkScope,
    specification_id: PolicyResourceId,
}

impl AgentApprovalProcess {
    fn from_spec(kind: AgentProcessInspectionKind, spec: &ProcessSpec) -> Self {
        Self {
            kind,
            executable: spec.executable().as_str().to_owned(),
            arguments: spec
                .arguments()
                .iter()
                .map(|argument| argument.as_str().to_owned())
                .collect(),
            working_directory: AgentApprovalWorkingDirectory::from_directory(
                spec.working_directory(),
            ),
            environment_allowlist: spec
                .environment_allowlist()
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
            timeout_millis: spec.timeout().as_millis(),
            stdout_limit: spec.stdout_limit().get(),
            stderr_limit: spec.stderr_limit().get(),
            execution_mode: spec.execution_mode(),
            plan_binding: spec.plan_binding(),
            network: match spec.network() {
                ProcessNetworkScope::Denied => AgentApprovalNetworkScope::Denied,
                ProcessNetworkScope::Requested(scope) => {
                    AgentApprovalNetworkScope::Requested(scope)
                }
            },
            specification_id: spec.specification_id(),
        }
    }

    /// Returns the product-facing Test, Build, Diagnostic, Lint, Format, or Command category.
    #[must_use]
    pub const fn kind(&self) -> AgentProcessInspectionKind {
        self.kind
    }

    /// Returns the exact validated executable token.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns the exact ordered argv tail without shell interpretation.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Returns the exact worktree-relative CWD selection.
    #[must_use]
    pub const fn working_directory(&self) -> &AgentApprovalWorkingDirectory {
        &self.working_directory
    }

    /// Returns admitted environment names; values never enter this projection.
    #[must_use]
    pub fn environment_allowlist(&self) -> &[String] {
        &self.environment_allowlist
    }

    /// Returns the positive process timeout in milliseconds.
    #[must_use]
    pub const fn timeout_millis(&self) -> u64 {
        self.timeout_millis
    }

    /// Returns the retained stdout byte cap.
    #[must_use]
    pub const fn stdout_limit(&self) -> u32 {
        self.stdout_limit
    }

    /// Returns the retained stderr byte cap.
    #[must_use]
    pub const fn stderr_limit(&self) -> u32 {
        self.stderr_limit
    }

    /// Returns KnownSafe or Open; shell mode cannot enter `ProcessSpec` V1.
    #[must_use]
    pub const fn execution_mode(&self) -> ProcessExecutionMode {
        self.execution_mode
    }

    /// Returns the exact plan binding checked by central policy.
    #[must_use]
    pub const fn plan_binding(&self) -> ProcessPlanBinding {
        self.plan_binding
    }

    /// Returns the declarative network boundary.
    #[must_use]
    pub const fn network(&self) -> AgentApprovalNetworkScope {
        self.network
    }

    /// Returns the exact content-free ProcessSpec identity.
    #[must_use]
    pub const fn specification_id(&self) -> PolicyResourceId {
        self.specification_id
    }
}

/// Closed exact action detail retained solely for informed approval presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentApprovalAction {
    /// One bounded E3 patch scope.
    Patch(AgentApprovalPatch),
    /// One complete E4 direct-process specification.
    Process(AgentApprovalProcess),
}

/// Volatile lifecycle marker; durable grant state remains authoritative in `PolicyStore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalPresentationState {
    /// No user decision has been persisted yet.
    Pending,
    /// An exact durable grant was created; its current status must be loaded again.
    Granted(ApprovalId),
    /// The user denied the exact request and the owning step was durably blocked.
    Denied,
}

/// One exact pending or recently resolved task-bound approval presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentApprovalPresentation {
    revision: AgentApprovalRevision,
    context: AgentInspectionContext,
    request_id: ApprovalRequestId,
    action_fingerprint: PolicyActionFingerprint,
    scope_digest: PolicyScopeDigest,
    action_class: ActionClass,
    risk_level: RiskLevel,
    reason: PolicyDecisionReason,
    requested_at: a3_domain::AgentRunTimestamp,
    expires_at: a3_domain::AgentRunTimestamp,
    action: AgentApprovalAction,
    state: AgentApprovalPresentationState,
}

impl AgentApprovalPresentation {
    /// Returns the revision that must accompany a later control request.
    #[must_use]
    pub const fn revision(&self) -> AgentApprovalRevision {
        self.revision
    }

    /// Returns the exact durable task/run/step/verification/snapshot anchors.
    #[must_use]
    pub const fn context(&self) -> AgentInspectionContext {
        self.context
    }

    /// Returns the Core-owned durable request identity; IPC control never accepts it.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }

    /// Returns the exact action fingerprint checked again before use.
    #[must_use]
    pub const fn action_fingerprint(&self) -> PolicyActionFingerprint {
        self.action_fingerprint
    }

    /// Returns the exact content-free scope digest.
    #[must_use]
    pub const fn scope_digest(&self) -> PolicyScopeDigest {
        self.scope_digest
    }

    /// Returns the Core-derived security class.
    #[must_use]
    pub const fn action_class(&self) -> ActionClass {
        self.action_class
    }

    /// Returns the Core-derived risk level.
    #[must_use]
    pub const fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    /// Returns why central policy required explicit approval.
    #[must_use]
    pub const fn reason(&self) -> PolicyDecisionReason {
        self.reason
    }

    /// Returns when the durable request was created.
    #[must_use]
    pub const fn requested_at(&self) -> a3_domain::AgentRunTimestamp {
        self.requested_at
    }

    /// Returns the exclusive request/grant expiration boundary.
    #[must_use]
    pub const fn expires_at(&self) -> a3_domain::AgentRunTimestamp {
        self.expires_at
    }

    /// Returns exact patch paths or exact process specification.
    #[must_use]
    pub const fn action(&self) -> &AgentApprovalAction {
        &self.action
    }

    /// Returns the local presentation lifecycle; grant status is loaded separately.
    #[must_use]
    pub const fn state(&self) -> AgentApprovalPresentationState {
        self.state
    }
}

/// Synchronous no-I/O presentation observer injected into the finite mutating controller.
pub trait AgentApprovalSink: fmt::Debug + Send + Sync {
    /// Retains one exact E3 action only after its durable request exists.
    fn record_patch_request(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        action: &PatchAction,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure>;

    /// Retains one exact E4 process specification only after its durable request exists.
    fn record_process_request(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        kind: AgentProcessInspectionKind,
        spec: &ProcessSpec,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure>;
}

/// Bounded in-memory approval owner instantiated and lifecycle-managed by the composition root.
pub struct AgentApprovalBuffer {
    state: Mutex<AgentApprovalBufferState>,
}

impl AgentApprovalBuffer {
    /// Creates an inactive empty buffer; project activation is explicit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(AgentApprovalBufferState::default()),
        }
    }

    /// Activates one worktree and clears any presentation from another project.
    pub fn activate_project(&self, project: &ProjectIdentity) {
        let mut state = lock_recovering_poison(&self.state);
        let worktree_id = project.worktree().id();
        if state.active_worktree_id != Some(worktree_id) {
            state.presentation = None;
            state.active_worktree_id = Some(worktree_id);
        }
    }

    /// Clears exact volatile action data during project switch, removal, or shutdown.
    pub fn deactivate_project(&self) {
        let mut state = lock_recovering_poison(&self.state);
        state.presentation = None;
        state.active_worktree_id = None;
    }

    /// Returns the one current task-bound presentation, when retained.
    pub fn presentation(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
    ) -> Result<Option<AgentApprovalPresentation>, AgentApprovalQueryError> {
        let state = lock_recovering_poison(&self.state);
        state.ensure_project(project)?;
        Ok(state
            .presentation
            .as_ref()
            .filter(|presentation| presentation.context().task_id() == task_id)
            .cloned())
    }

    /// Revalidates the visible revision and records the Core-generated durable grant identity.
    pub fn mark_granted(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_revision: AgentApprovalRevision,
        approval_id: ApprovalId,
    ) -> Result<AgentApprovalRevision, AgentApprovalQueryError> {
        self.update_state(
            project,
            task_id,
            expected_revision,
            AgentApprovalPresentationState::Granted(approval_id),
        )
    }

    /// Revalidates the visible revision and records a durable denial outcome.
    pub fn mark_denied(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_revision: AgentApprovalRevision,
    ) -> Result<AgentApprovalRevision, AgentApprovalQueryError> {
        self.update_state(
            project,
            task_id,
            expected_revision,
            AgentApprovalPresentationState::Denied,
        )
    }

    fn update_state(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_revision: AgentApprovalRevision,
        next_state: AgentApprovalPresentationState,
    ) -> Result<AgentApprovalRevision, AgentApprovalQueryError> {
        let mut state = lock_recovering_poison(&self.state);
        state.ensure_project(project)?;
        let next_revision = state.next_revision()?;
        let presentation = state
            .presentation
            .as_mut()
            .filter(|presentation| {
                presentation.context().task_id() == task_id
                    && presentation.revision() == expected_revision
            })
            .ok_or(AgentApprovalQueryError::RevisionChanged)?;
        presentation.revision = next_revision;
        presentation.state = next_state;
        Ok(next_revision)
    }

    fn record(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        policy_action: &PolicyAction,
        action: AgentApprovalAction,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
        validate_request(project, context, request, reason, policy_action)?;
        let mut state = lock_recovering_poison(&self.state);
        state.ensure_active_project(project)?;
        let revision = state
            .next_revision()
            .map_err(|_| AgentApprovalSinkFailure::RevisionExhausted)?;
        state.presentation = Some(AgentApprovalPresentation {
            revision,
            context,
            request_id: request.id(),
            action_fingerprint: request.action_fingerprint(),
            scope_digest: request.scope_digest(),
            action_class: request.action_class(),
            risk_level: request.risk_level(),
            reason,
            requested_at: request.requested_at(),
            expires_at: request.expires_at(),
            action,
            state: AgentApprovalPresentationState::Pending,
        });
        Ok(revision)
    }
}

impl Default for AgentApprovalBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AgentApprovalBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock_recovering_poison(&self.state);
        formatter
            .debug_struct("AgentApprovalBuffer")
            .field("active", &state.active_worktree_id.is_some())
            .field("has_presentation", &state.presentation.is_some())
            .finish()
    }
}

impl AgentApprovalSink for AgentApprovalBuffer {
    fn record_patch_request(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        action: &PatchAction,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
        if context.run_id() != action.run_id()
            || context.step_id() != action.task_step_id()
            || context.verification_spec_id() != action.verification_spec_id()
            || context.snapshot_id() != action.snapshot_id()
        {
            return Err(AgentApprovalSinkFailure::AnchorMismatch);
        }
        self.record(
            project,
            context,
            request,
            reason,
            &action.policy_action(),
            AgentApprovalAction::Patch(AgentApprovalPatch::from_action(action)),
        )
    }

    fn record_process_request(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        kind: AgentProcessInspectionKind,
        spec: &ProcessSpec,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
        if context.run_id() != spec.run_id()
            || spec.plan_binding() != ProcessPlanBinding::Validated(context.step_id())
        {
            return Err(AgentApprovalSinkFailure::AnchorMismatch);
        }
        self.record(
            project,
            context,
            request,
            reason,
            &spec.policy_action(),
            AgentApprovalAction::Process(AgentApprovalProcess::from_spec(kind, spec)),
        )
    }
}

#[derive(Default)]
struct AgentApprovalBufferState {
    active_worktree_id: Option<WorktreeId>,
    next_revision_value: u64,
    presentation: Option<AgentApprovalPresentation>,
}

impl AgentApprovalBufferState {
    fn ensure_active_project(
        &self,
        project: &ProjectIdentity,
    ) -> Result<(), AgentApprovalSinkFailure> {
        if self.active_worktree_id == Some(project.worktree().id()) {
            Ok(())
        } else {
            Err(AgentApprovalSinkFailure::InactiveProject)
        }
    }

    fn ensure_project(&self, project: &ProjectIdentity) -> Result<(), AgentApprovalQueryError> {
        if self.active_worktree_id == Some(project.worktree().id()) {
            Ok(())
        } else {
            Err(AgentApprovalQueryError::Unavailable)
        }
    }

    fn next_revision(&mut self) -> Result<AgentApprovalRevision, AgentApprovalQueryError> {
        let value = self
            .next_revision_value
            .checked_add(1)
            .ok_or(AgentApprovalQueryError::RevisionExhausted)?;
        self.next_revision_value = value;
        Ok(AgentApprovalRevision(value))
    }
}

fn validate_request(
    project: &ProjectIdentity,
    context: AgentInspectionContext,
    request: &ApprovalRequest,
    reason: PolicyDecisionReason,
    action: &PolicyAction,
) -> Result<(), AgentApprovalSinkFailure> {
    if action_worktree(action) != Some(project.worktree().id())
        || request.run_id() != context.run_id()
        || request.action_fingerprint() != action.fingerprint()
        || request.scope_digest() != action.scope_digest()
        || request.action_class() != action.class()
        || request.risk_level() != action.risk()
        || !matches!(
            reason,
            PolicyDecisionReason::SystemApprovalRequired
                | PolicyDecisionReason::WorkspaceApprovalRequired
        )
    {
        return Err(AgentApprovalSinkFailure::AnchorMismatch);
    }
    Ok(())
}

fn action_worktree(action: &PolicyAction) -> Option<WorktreeId> {
    match action {
        PolicyAction::Root { worktree_id, .. } | PolicyAction::Git { worktree_id, .. } => {
            Some(*worktree_id)
        }
        PolicyAction::Path { scope, .. } => match scope {
            a3_domain::PolicyPathScope::Worktree { worktree_id, .. } => Some(*worktree_id),
            a3_domain::PolicyPathScope::OutsideRoot { .. } => None,
        },
        PolicyAction::Patch(patch) => Some(patch.worktree_id()),
        PolicyAction::Process(process) => Some(process.worktree_id()),
        PolicyAction::Network { .. } => None,
    }
}

/// Effective approval lifecycle after combining volatile presentation and durable policy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalStatus {
    /// No user decision has been persisted and the request is still live.
    Pending,
    /// A matching grant is live and unused.
    Active,
    /// A matching grant was consumed exactly once.
    Consumed,
    /// A matching grant was explicitly withdrawn before use.
    Revoked,
    /// The request or otherwise active grant reached its exclusive expiry.
    Expired,
    /// The user denied the request and the owning step is durably blocked.
    Denied,
}

/// Current exact task-bound approval plus the Core-derived controls that remain safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentApprovalCenter {
    ledger_revision: u32,
    ledger_store_version: TaskLedgerStoreVersion,
    controller_state: AgentControllerState,
    step_status: TaskStepStatus,
    presentation: AgentApprovalPresentation,
    status: AgentApprovalStatus,
    approval: Option<ApprovalGrant>,
    run: AgentRun,
    ledger: TaskLedger,
}

impl AgentApprovalCenter {
    /// Returns the exact visible Task Ledger revision.
    #[must_use]
    pub const fn ledger_revision(&self) -> u32 {
        self.ledger_revision
    }

    /// Returns the optimistic Task Ledger store version required by controls.
    #[must_use]
    pub const fn ledger_store_version(&self) -> TaskLedgerStoreVersion {
        self.ledger_store_version
    }

    /// Returns the durable finite controller state.
    #[must_use]
    pub const fn controller_state(&self) -> AgentControllerState {
        self.controller_state
    }

    /// Returns the exact owning step state.
    #[must_use]
    pub const fn step_status(&self) -> TaskStepStatus {
        self.step_status
    }

    /// Returns the exact bounded action presentation and its request metadata.
    #[must_use]
    pub const fn presentation(&self) -> &AgentApprovalPresentation {
        &self.presentation
    }

    /// Returns the effective request/grant lifecycle.
    #[must_use]
    pub const fn status(&self) -> AgentApprovalStatus {
        self.status
    }

    /// Returns whether an exact one-time grant can be stored now.
    #[must_use]
    pub const fn can_allow_once(&self) -> bool {
        matches!(self.status, AgentApprovalStatus::Pending)
            && matches!(self.controller_state, AgentControllerState::AwaitApproval)
            && matches!(self.step_status, TaskStepStatus::AwaitingApproval)
    }

    /// Returns whether the still-pending request can be explicitly denied now.
    #[must_use]
    pub const fn can_deny(&self) -> bool {
        self.can_allow_once()
    }

    /// Returns whether a live unused grant can be supplied to a new owned Agent attempt.
    #[must_use]
    pub const fn can_continue(&self) -> bool {
        matches!(self.status, AgentApprovalStatus::Active)
            && matches!(self.controller_state, AgentControllerState::AwaitApproval)
            && matches!(self.step_status, TaskStepStatus::AwaitingApproval)
    }

    /// Returns whether a live unused grant can still be withdrawn.
    #[must_use]
    pub const fn can_revoke(&self) -> bool {
        self.can_continue()
    }

    fn approval(&self) -> Option<&ApprovalGrant> {
        self.approval.as_ref()
    }
}

/// Expected task-bound approval availability states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentApprovalLoadResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no Task Ledger yet.
    LedgerUnavailable,
    /// Current Goal and Ledger still refer to different immutable revisions.
    GoalRevisionMismatch {
        /// Current Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Ledger.
        ledger_revision: u32,
    },
    /// Durable or volatile anchors changed during the bounded read.
    ActivityChanged,
    /// No exact same-process action presentation is available.
    ApprovalUnavailable,
    /// The current exact presentation and durable lifecycle are consistent.
    Available(Box<AgentApprovalCenter>),
}

/// Loads one exact approval solely from the selected task and Core-owned current state.
#[derive(Debug, Clone)]
pub struct GetAgentApprovalCenter {
    activity: GetAgentActivity,
    policy: std::sync::Arc<dyn PolicyStore>,
    presentations: std::sync::Arc<AgentApprovalBuffer>,
}

impl GetAgentApprovalCenter {
    /// Composes existing durable activity/policy ports with the bounded volatile presentation.
    #[must_use]
    pub fn new(
        workspace: std::sync::Arc<dyn TaskLensWorkspaceStore>,
        journal: std::sync::Arc<dyn RunJournalStore>,
        policy: std::sync::Arc<dyn PolicyStore>,
        presentations: std::sync::Arc<AgentApprovalBuffer>,
    ) -> Self {
        Self {
            activity: GetAgentActivity::new(workspace, journal),
            policy,
            presentations,
        }
    }

    /// Revalidates task, run, request, optional grant, and volatile action before returning data.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        observed_at: AgentRunTimestamp,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<AgentApprovalLoadResult, GetAgentApprovalCenterFailure> {
        let initial = match self.load_activity(project, task_id, control).await? {
            ApprovalActivityLoad::Expected(result) => return Ok(result),
            ApprovalActivityLoad::Available(activity) => activity,
        };
        let presentation = match self.presentations.presentation(project, task_id) {
            Ok(Some(presentation)) => presentation,
            Ok(None) | Err(AgentApprovalQueryError::Unavailable) => {
                return Ok(AgentApprovalLoadResult::ApprovalUnavailable);
            }
            Err(error) => return Err(GetAgentApprovalCenterFailure::Presentation(error)),
        };
        let Some(center) = self
            .build_center(project, &initial, presentation, observed_at)
            .await?
        else {
            return Ok(AgentApprovalLoadResult::ActivityChanged);
        };
        let current = match self.load_activity(project, task_id, control).await? {
            ApprovalActivityLoad::Available(activity) if activity == initial => activity,
            ApprovalActivityLoad::Available(_) | ApprovalActivityLoad::Expected(_) => {
                return Ok(AgentApprovalLoadResult::ActivityChanged);
            }
        };
        if center.run
            != *current
                .run()
                .map(|selected| selected.run())
                .ok_or(GetAgentApprovalCenterFailure::InvalidAnchors)?
        {
            return Ok(AgentApprovalLoadResult::ActivityChanged);
        }
        Ok(AgentApprovalLoadResult::Available(Box::new(center)))
    }

    async fn load_activity(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<ApprovalActivityLoad, GetAgentApprovalCenterFailure> {
        self.activity
            .execute(project, task_id, control)
            .await
            .map_err(GetAgentApprovalCenterFailure::Activity)
            .map(|result| match result {
                AgentActivityLoadResult::TaskNotFound => {
                    ApprovalActivityLoad::Expected(AgentApprovalLoadResult::TaskNotFound)
                }
                AgentActivityLoadResult::LedgerUnavailable => {
                    ApprovalActivityLoad::Expected(AgentApprovalLoadResult::LedgerUnavailable)
                }
                AgentActivityLoadResult::GoalRevisionMismatch {
                    current_revision,
                    ledger_revision,
                } => {
                    ApprovalActivityLoad::Expected(AgentApprovalLoadResult::GoalRevisionMismatch {
                        current_revision,
                        ledger_revision,
                    })
                }
                AgentActivityLoadResult::ActivityChanged => {
                    ApprovalActivityLoad::Expected(AgentApprovalLoadResult::ActivityChanged)
                }
                AgentActivityLoadResult::Available(activity) => {
                    ApprovalActivityLoad::Available(activity)
                }
            })
    }

    async fn build_center(
        &self,
        project: &ProjectIdentity,
        activity: &AgentActivity,
        presentation: AgentApprovalPresentation,
        observed_at: AgentRunTimestamp,
    ) -> Result<Option<AgentApprovalCenter>, GetAgentApprovalCenterFailure> {
        let Some(selected) = activity.run() else {
            return Ok(None);
        };
        let context = presentation.context();
        let stored = activity.anchor().task_ledger();
        let ledger = stored.ledger();
        let Some(step) = ledger.step(context.step_id()) else {
            return Ok(None);
        };
        if context.task_id() != ledger.task_id()
            || context.run_id() != selected.run().id()
            || context.step_id() != selected.step_id()
            || context.snapshot_id() != selected.run().current_snapshot_id()
        {
            return Ok(None);
        }
        let Some(request) = self
            .policy
            .load_approval_request(project, presentation.request_id())
            .await
            .map_err(GetAgentApprovalCenterFailure::Policy)?
        else {
            return Ok(None);
        };
        if !request_matches_presentation(&request, &presentation) {
            return Ok(None);
        }
        let approval = self
            .policy
            .load_approval_for_request(project, request.id())
            .await
            .map_err(GetAgentApprovalCenterFailure::Policy)?;
        if approval
            .as_ref()
            .is_some_and(|grant| !approval_matches_request(grant, &request))
        {
            return Ok(None);
        }
        let status = match presentation.state() {
            AgentApprovalPresentationState::Denied if approval.is_none() => {
                AgentApprovalStatus::Denied
            }
            AgentApprovalPresentationState::Denied => return Ok(None),
            AgentApprovalPresentationState::Granted(expected_id) => match &approval {
                Some(grant) if grant.id() == expected_id => map_approval_status(grant, observed_at),
                _ => return Ok(None),
            },
            AgentApprovalPresentationState::Pending => match &approval {
                Some(grant) => map_approval_status(grant, observed_at),
                None if observed_at >= request.expires_at() => AgentApprovalStatus::Expired,
                None => AgentApprovalStatus::Pending,
            },
        };
        if !lifecycle_matches_activity(status, selected, step.status()) {
            return Ok(None);
        }
        Ok(Some(AgentApprovalCenter {
            ledger_revision: ledger.revision().get(),
            ledger_store_version: stored.version(),
            controller_state: selected.run().state(),
            step_status: step.status(),
            presentation,
            status,
            approval,
            run: selected.run().clone(),
            ledger: ledger.clone(),
        }))
    }
}

enum ApprovalActivityLoad {
    Expected(AgentApprovalLoadResult),
    Available(Box<AgentActivity>),
}

fn request_matches_presentation(
    request: &ApprovalRequest,
    presentation: &AgentApprovalPresentation,
) -> bool {
    request.id() == presentation.request_id()
        && request.run_id() == presentation.context().run_id()
        && request.action_fingerprint() == presentation.action_fingerprint()
        && request.scope_digest() == presentation.scope_digest()
        && request.action_class() == presentation.action_class()
        && request.risk_level() == presentation.risk_level()
        && request.requested_at() == presentation.requested_at()
        && request.expires_at() == presentation.expires_at()
}

fn approval_matches_request(approval: &ApprovalGrant, request: &ApprovalRequest) -> bool {
    approval.request_id() == request.id()
        && approval.run_id() == request.run_id()
        && approval.action_fingerprint() == request.action_fingerprint()
        && approval.scope_digest() == request.scope_digest()
        && approval.action_class() == request.action_class()
        && approval.risk_level() == request.risk_level()
        && approval.expires_at() == request.expires_at()
}

fn map_approval_status(
    approval: &ApprovalGrant,
    observed_at: AgentRunTimestamp,
) -> AgentApprovalStatus {
    match approval.status_at(observed_at) {
        ApprovalStatus::Active => AgentApprovalStatus::Active,
        ApprovalStatus::Consumed => AgentApprovalStatus::Consumed,
        ApprovalStatus::Revoked => AgentApprovalStatus::Revoked,
        ApprovalStatus::Expired => AgentApprovalStatus::Expired,
    }
}

fn lifecycle_matches_activity(
    status: AgentApprovalStatus,
    selected: &crate::AgentActivityRun,
    step_status: TaskStepStatus,
) -> bool {
    match status {
        AgentApprovalStatus::Pending
        | AgentApprovalStatus::Active
        | AgentApprovalStatus::Revoked
        | AgentApprovalStatus::Expired => {
            selected.is_active_attempt()
                && selected.run().state() == AgentControllerState::AwaitApproval
                && step_status == TaskStepStatus::AwaitingApproval
        }
        AgentApprovalStatus::Denied => {
            !selected.is_active_attempt()
                && selected.run().state() == AgentControllerState::Failed
                && step_status == TaskStepStatus::Blocked
        }
        AgentApprovalStatus::Consumed => true,
    }
}

/// Task-bound approval read failed at a privileged boundary.
#[derive(Debug)]
pub enum GetAgentApprovalCenterFailure {
    /// Durable task/run activity could not be read.
    Activity(GetAgentActivityFailure),
    /// Durable request or grant state could not be read.
    Policy(PolicyStoreFailure),
    /// The volatile presentation owner failed.
    Presentation(AgentApprovalQueryError),
    /// A supposedly available activity omitted its selected run.
    InvalidAnchors,
}

impl fmt::Display for GetAgentApprovalCenterFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Activity(_) => "Agent approval activity is unavailable",
            Self::Policy(_) => "Agent approval policy state is unavailable",
            Self::Presentation(_) => "Agent approval presentation is unavailable",
            Self::InvalidAnchors => "Agent approval activity anchors are invalid",
        })
    }
}

impl Error for GetAgentApprovalCenterFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Activity(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Presentation(error) => Some(error),
            Self::InvalidAnchors => None,
        }
    }
}

/// Closed decision accepted by the task-bound Approval Center mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalControlAction {
    /// Persist one exact action- and scope-bound one-time grant.
    AllowOnce,
    /// Block the exact waiting step and fail the current run without a tool effect.
    Deny,
    /// Request a new owned attempt using the active internal grant identity.
    Continue,
    /// Withdraw a still-active exact grant before use.
    Revoke,
}

/// Core-generated identities and time for one explicit Approval Center operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentApprovalControlMetadata {
    approval_id: ApprovalId,
    event_id: RunEventId,
    observed_at: AgentRunTimestamp,
}

impl AgentApprovalControlMetadata {
    /// Groups identifiers and time that never originate in the WebView.
    #[must_use]
    pub const fn new(
        approval_id: ApprovalId,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
    ) -> Self {
        Self {
            approval_id,
            event_id,
            observed_at,
        }
    }
}

/// Applied effect of one exact Approval Center decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalControlOutcome {
    /// The exact grant is durable but no Agent attempt has been started.
    GrantStored {
        /// New volatile revision after binding the Core-generated grant identity.
        approval_revision: AgentApprovalRevision,
    },
    /// The waiting step is durably Blocked and the run is Failed.
    Denied {
        /// New optimistic Ledger store version after the atomic block transition.
        ledger_store_version: TaskLedgerStoreVersion,
        /// New volatile revision of the resolved presentation.
        approval_revision: AgentApprovalRevision,
    },
    /// The exact grant is durably revoked.
    Revoked {
        /// New volatile revision after observing the lifecycle mutation.
        approval_revision: AgentApprovalRevision,
    },
    /// The Core may submit one new scheduler attempt with this internal grant.
    ContinueReady {
        /// Internal identity that must never be mapped to WebView output.
        approval_id: ApprovalId,
    },
}

/// Expected result of one task-bound Approval Center control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentApprovalControlResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no Task Ledger.
    LedgerUnavailable,
    /// Current Goal and Ledger revisions differ.
    GoalRevisionMismatch,
    /// Durable or volatile anchors changed before the decision.
    ActivityChanged,
    /// Exact action presentation is not available in this process.
    ApprovalUnavailable,
    /// The requested lifecycle action is no longer allowed.
    ActionUnavailable,
    /// The action was applied or is ready for the composition-root scheduler.
    Applied(AgentApprovalControlOutcome),
}

/// Revalidates and applies AllowOnce, Deny, Revoke, or Continue without WebView policy IDs.
#[derive(Debug, Clone)]
pub struct ControlAgentApproval {
    query: GetAgentApprovalCenter,
    policy: std::sync::Arc<dyn PolicyStore>,
    actions: std::sync::Arc<dyn AgentActionStore>,
    presentations: std::sync::Arc<AgentApprovalBuffer>,
}

impl ControlAgentApproval {
    /// Composes existing E1/E7 ports and the same task-bound read used for presentation.
    #[must_use]
    pub fn new(
        workspace: std::sync::Arc<dyn TaskLensWorkspaceStore>,
        journal: std::sync::Arc<dyn RunJournalStore>,
        policy: std::sync::Arc<dyn PolicyStore>,
        actions: std::sync::Arc<dyn AgentActionStore>,
        presentations: std::sync::Arc<AgentApprovalBuffer>,
    ) -> Self {
        Self {
            query: GetAgentApprovalCenter::new(
                workspace,
                journal,
                std::sync::Arc::clone(&policy),
                std::sync::Arc::clone(&presentations),
            ),
            policy,
            actions,
            presentations,
        }
    }

    /// Applies one explicit choice after matching the exact previously visible revisions.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        expected_approval_revision: AgentApprovalRevision,
        expected_ledger_revision: u32,
        expected_ledger_store_version: TaskLedgerStoreVersion,
        action: AgentApprovalControlAction,
        metadata: AgentApprovalControlMetadata,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<AgentApprovalControlResult, ControlAgentApprovalFailure> {
        let center = match self
            .query
            .execute(project, task_id, metadata.observed_at, control)
            .await?
        {
            AgentApprovalLoadResult::TaskNotFound => {
                return Ok(AgentApprovalControlResult::TaskNotFound);
            }
            AgentApprovalLoadResult::LedgerUnavailable => {
                return Ok(AgentApprovalControlResult::LedgerUnavailable);
            }
            AgentApprovalLoadResult::GoalRevisionMismatch { .. } => {
                return Ok(AgentApprovalControlResult::GoalRevisionMismatch);
            }
            AgentApprovalLoadResult::ActivityChanged => {
                return Ok(AgentApprovalControlResult::ActivityChanged);
            }
            AgentApprovalLoadResult::ApprovalUnavailable => {
                return Ok(AgentApprovalControlResult::ApprovalUnavailable);
            }
            AgentApprovalLoadResult::Available(center) => center,
        };
        if center.presentation().revision() != expected_approval_revision
            || center.ledger_revision() != expected_ledger_revision
            || center.ledger_store_version() != expected_ledger_store_version
        {
            return Ok(AgentApprovalControlResult::ActivityChanged);
        }
        match action {
            AgentApprovalControlAction::AllowOnce => {
                self.allow_once(project, task_id, center, metadata).await
            }
            AgentApprovalControlAction::Deny => self.deny(project, task_id, center, metadata).await,
            AgentApprovalControlAction::Continue => Self::continue_with(center),
            AgentApprovalControlAction::Revoke => {
                self.revoke(project, task_id, center, metadata).await
            }
        }
    }

    async fn allow_once(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        center: Box<AgentApprovalCenter>,
        metadata: AgentApprovalControlMetadata,
    ) -> Result<AgentApprovalControlResult, ControlAgentApprovalFailure> {
        if !center.can_allow_once() {
            return Ok(AgentApprovalControlResult::ActionUnavailable);
        }
        let mut run = center.run.clone();
        let snapshot_id = run.current_snapshot_id();
        let approval = GrantPolicyApproval::new(self.policy.as_ref())
            .execute(
                project,
                &mut run,
                center.presentation().request_id(),
                metadata.approval_id,
                metadata.event_id,
                snapshot_id,
                metadata.observed_at,
            )
            .await;
        let approval = match approval {
            Ok(approval) => approval,
            Err(error) if grant_conflicted(&error) => {
                return Ok(AgentApprovalControlResult::ActivityChanged);
            }
            Err(error) => return Err(ControlAgentApprovalFailure::Grant(error)),
        };
        let revision = self
            .presentations
            .mark_granted(
                project,
                task_id,
                center.presentation().revision(),
                approval.id(),
            )
            .map_err(ControlAgentApprovalFailure::Presentation)?;
        Ok(AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::GrantStored {
                approval_revision: revision,
            },
        ))
    }

    async fn deny(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        center: Box<AgentApprovalCenter>,
        metadata: AgentApprovalControlMetadata,
    ) -> Result<AgentApprovalControlResult, ControlAgentApprovalFailure> {
        if !center.can_deny() {
            return Ok(AgentApprovalControlResult::ActionUnavailable);
        }
        let mut run = center.run.clone();
        let mut ledger = center.ledger.clone();
        let expected_sequence = run.last_event_sequence();
        let snapshot_id = run.current_snapshot_id();
        ledger.block_step(
            center.presentation().context().step_id(),
            run.id(),
            TaskStepBlockingReason::try_from_string(USER_DENIAL_REASON.to_owned())
                .map_err(ControlAgentApprovalFailure::BlockingReason)?,
            TaskLedgerTimestamp::from_unix_millis(metadata.observed_at.unix_millis())
                .map_err(ControlAgentApprovalFailure::LedgerTimestamp)?,
        )?;
        let advance = AdvanceAgentController.execute(
            &mut run,
            AgentControllerSignal::ApprovalDenied,
            metadata.event_id,
            snapshot_id,
            metadata.observed_at,
            false,
        )?;
        let next_version = self
            .actions
            .commit_ledger_action(
                project,
                center.ledger_store_version(),
                expected_sequence,
                &ledger,
                &run,
                advance.event(),
            )
            .await;
        let next_version = match next_version {
            Ok(version) => version,
            Err(error) if action_store_conflicted(error) => {
                return Ok(AgentApprovalControlResult::ActivityChanged);
            }
            Err(error) => return Err(ControlAgentApprovalFailure::ActionStore(error)),
        };
        let revision = self
            .presentations
            .mark_denied(project, task_id, center.presentation().revision())
            .map_err(ControlAgentApprovalFailure::Presentation)?;
        Ok(AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::Denied {
                ledger_store_version: next_version,
                approval_revision: revision,
            },
        ))
    }

    fn continue_with(
        center: Box<AgentApprovalCenter>,
    ) -> Result<AgentApprovalControlResult, ControlAgentApprovalFailure> {
        if !center.can_continue() {
            return Ok(AgentApprovalControlResult::ActionUnavailable);
        }
        let approval_id = center
            .approval()
            .map(ApprovalGrant::id)
            .ok_or(ControlAgentApprovalFailure::InvalidLifecycle)?;
        Ok(AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::ContinueReady { approval_id },
        ))
    }

    async fn revoke(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        center: Box<AgentApprovalCenter>,
        metadata: AgentApprovalControlMetadata,
    ) -> Result<AgentApprovalControlResult, ControlAgentApprovalFailure> {
        if !center.can_revoke() {
            return Ok(AgentApprovalControlResult::ActionUnavailable);
        }
        let mut run = center.run.clone();
        let approval = center
            .approval()
            .ok_or(ControlAgentApprovalFailure::InvalidLifecycle)?;
        let snapshot_id = run.current_snapshot_id();
        let revoked = RevokePolicyApproval::new(self.policy.as_ref())
            .execute(
                project,
                &mut run,
                approval.id(),
                metadata.event_id,
                snapshot_id,
                metadata.observed_at,
            )
            .await;
        match revoked {
            Ok(_) => {}
            Err(error) if revoke_conflicted(&error) => {
                return Ok(AgentApprovalControlResult::ActivityChanged);
            }
            Err(error) => return Err(ControlAgentApprovalFailure::Revoke(error)),
        }
        let revision = self
            .presentations
            .mark_granted(
                project,
                task_id,
                center.presentation().revision(),
                approval.id(),
            )
            .map_err(ControlAgentApprovalFailure::Presentation)?;
        Ok(AgentApprovalControlResult::Applied(
            AgentApprovalControlOutcome::Revoked {
                approval_revision: revision,
            },
        ))
    }
}

fn grant_conflicted(error: &GrantPolicyApprovalError) -> bool {
    matches!(
        error,
        GrantPolicyApprovalError::RequestNotFound
            | GrantPolicyApprovalError::RequestRunMismatch
            | GrantPolicyApprovalError::SnapshotMismatch
            | GrantPolicyApprovalError::Grant(_)
            | GrantPolicyApprovalError::Store(
                PolicyStoreFailure::NotFound
                    | PolicyStoreFailure::AlreadyExists
                    | PolicyStoreFailure::RunSequenceConflict
                    | PolicyStoreFailure::ApprovalConflict
            )
    )
}

fn revoke_conflicted(error: &RevokePolicyApprovalError) -> bool {
    matches!(
        error,
        RevokePolicyApprovalError::ApprovalNotFound
            | RevokePolicyApprovalError::ApprovalRunMismatch
            | RevokePolicyApprovalError::SnapshotMismatch
            | RevokePolicyApprovalError::Revoke(_)
            | RevokePolicyApprovalError::Store(
                PolicyStoreFailure::NotFound
                    | PolicyStoreFailure::RunSequenceConflict
                    | PolicyStoreFailure::ApprovalConflict
            )
    )
}

const fn action_store_conflicted(error: AgentActionStoreFailure) -> bool {
    matches!(
        error,
        AgentActionStoreFailure::TaskNotFound
            | AgentActionStoreFailure::RunNotFound
            | AgentActionStoreFailure::LedgerVersionConflict
            | AgentActionStoreFailure::RunSequenceConflict
    )
}

/// Approval control failed without authorizing or exposing another action.
#[derive(Debug)]
pub enum ControlAgentApprovalFailure {
    /// The shared task-bound read failed.
    Query(GetAgentApprovalCenterFailure),
    /// Durable exact grant creation failed.
    Grant(GrantPolicyApprovalError),
    /// Durable exact grant revocation failed.
    Revoke(RevokePolicyApprovalError),
    /// Atomic denial Ledger/run commit failed.
    ActionStore(AgentActionStoreFailure),
    /// The local presentation changed unexpectedly after a durable write.
    Presentation(AgentApprovalQueryError),
    /// Fixed content-free blocking text violated its domain bound.
    BlockingReason(a3_domain::TaskStepTextError),
    /// Core time could not be represented as a Ledger timestamp.
    LedgerTimestamp(a3_domain::TaskLedgerTimestampError),
    /// Ledger denial transition violated a durable invariant.
    Ledger(a3_domain::TaskLedgerError),
    /// Finite controller rejected ApprovalDenied.
    Controller(crate::AgentControllerError),
    /// A supposedly active lifecycle omitted its durable grant.
    InvalidLifecycle,
}

impl fmt::Display for ControlAgentApprovalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Query(_) => "Agent approval state could not be revalidated",
            Self::Grant(_) => "Agent approval grant could not be stored",
            Self::Revoke(_) => "Agent approval grant could not be revoked",
            Self::ActionStore(_) => "Agent approval denial could not be committed",
            Self::Presentation(_) => "Agent approval presentation changed",
            Self::BlockingReason(_) => "Agent approval blocking reason is invalid",
            Self::LedgerTimestamp(_) => "Agent approval time is invalid",
            Self::Ledger(_) => "Agent approval Ledger transition is invalid",
            Self::Controller(_) => "Agent approval controller transition is invalid",
            Self::InvalidLifecycle => "Agent approval lifecycle is invalid",
        })
    }
}

impl Error for ControlAgentApprovalFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Query(error) => Some(error),
            Self::Grant(error) => Some(error),
            Self::Revoke(error) => Some(error),
            Self::ActionStore(error) => Some(error),
            Self::Presentation(error) => Some(error),
            Self::BlockingReason(error) => Some(error),
            Self::LedgerTimestamp(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::Controller(error) => Some(error),
            Self::InvalidLifecycle => None,
        }
    }
}

impl From<GetAgentApprovalCenterFailure> for ControlAgentApprovalFailure {
    fn from(error: GetAgentApprovalCenterFailure) -> Self {
        Self::Query(error)
    }
}

impl From<a3_domain::TaskLedgerError> for ControlAgentApprovalFailure {
    fn from(error: a3_domain::TaskLedgerError) -> Self {
        Self::Ledger(error)
    }
}

impl From<crate::AgentControllerError> for ControlAgentApprovalFailure {
    fn from(error: crate::AgentControllerError) -> Self {
        Self::Controller(error)
    }
}

/// Volatile presentation sink rejected an unsafe or stale record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalSinkFailure {
    /// The composition root has not activated the action's worktree.
    InactiveProject,
    /// Request, action, project, or task anchors did not match exactly.
    AnchorMismatch,
    /// The process-local revision counter exhausted u64.
    RevisionExhausted,
}

impl fmt::Display for AgentApprovalSinkFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InactiveProject => "Agent approval project is inactive",
            Self::AnchorMismatch => "Agent approval anchors do not match",
            Self::RevisionExhausted => "Agent approval revision is exhausted",
        })
    }
}

impl Error for AgentApprovalSinkFailure {}

/// Task-bound volatile approval lookup failed without exposing its retained action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentApprovalQueryError {
    /// The project or requested record is not retained.
    Unavailable,
    /// The presentation changed after the WebView displayed it.
    RevisionChanged,
    /// The process-local revision counter exhausted u64.
    RevisionExhausted,
}

impl fmt::Display for AgentApprovalQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Agent approval presentation is unavailable",
            Self::RevisionChanged => "Agent approval presentation changed",
            Self::RevisionExhausted => "Agent approval revision is exhausted",
        })
    }
}

impl Error for AgentApprovalQueryError {}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
