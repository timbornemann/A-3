use crate::{
    AgentControllerStateV1, AgentInspectionPathV1, AgentInspectionProcessKindV1, ProtocolVersion,
    TaskLensStepStatusV1,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Strict task-only selector for the current approval presentation.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentApprovalRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl QueryAgentApprovalRequestV1 {
    /// Returns the version checked before the task selector is interpreted.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the opaque durable task identity.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

impl fmt::Debug for QueryAgentApprovalRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryAgentApprovalRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("has_task_id", &!self.task_id.is_empty())
            .finish()
    }
}

/// Explicit closed decision; no request, grant, path, or process identity is accepted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalControlActionV1 {
    /// Store an exact one-time grant without starting execution.
    AllowOnce,
    /// Reject the request and block its owning step.
    Deny,
    /// Start a new owned attempt with the active grant.
    Continue,
    /// Withdraw an active unused grant.
    Revoke,
}

/// Optimistic control against exactly the presentation and Ledger visible to the user.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlAgentApprovalRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    expected_approval_revision: String,
    expected_ledger_revision: u32,
    expected_ledger_store_version: String,
    action: AgentApprovalControlActionV1,
}

impl ControlAgentApprovalRequestV1 {
    /// Returns the version checked before the remaining fields.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the selected opaque task identity.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the decimal volatile presentation revision.
    #[must_use]
    pub fn expected_approval_revision(&self) -> &str {
        &self.expected_approval_revision
    }

    /// Returns the visible durable Ledger revision.
    #[must_use]
    pub const fn expected_ledger_revision(&self) -> u32 {
        self.expected_ledger_revision
    }

    /// Returns the decimal optimistic Ledger store version.
    #[must_use]
    pub fn expected_ledger_store_version(&self) -> &str {
        &self.expected_ledger_store_version
    }

    /// Returns the explicit closed user decision.
    #[must_use]
    pub const fn action(&self) -> AgentApprovalControlActionV1 {
        self.action
    }
}

impl fmt::Debug for ControlAgentApprovalRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlAgentApprovalRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("has_task_id", &!self.task_id.is_empty())
            .field(
                "expected_approval_revision",
                &self.expected_approval_revision,
            )
            .field("expected_ledger_revision", &self.expected_ledger_revision)
            .field(
                "expected_ledger_store_version",
                &self.expected_ledger_store_version,
            )
            .field("action", &self.action)
            .finish()
    }
}

/// Core-derived policy action class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalActionClassV1 {
    /// Bounded observation.
    Read,
    /// Deterministic local derivation.
    Derive,
    /// Workspace mutation.
    Write,
    /// Known direct-argv execution.
    ExecuteSafe,
    /// Open direct-argv or shell execution.
    ExecuteOpen,
    /// External communication.
    Network,
    /// Irreversible local effect.
    Destructive,
    /// External publication.
    Publish,
    /// Access beyond the approved root.
    OutsideRoot,
}

/// Coarse Core-derived risk shown before a decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalRiskV1 {
    /// No privileged mutation.
    Low,
    /// Bounded local mutation or safe process.
    Moderate,
    /// Open execution or network.
    High,
    /// Destruction, publication, shell, or outside-root access.
    Critical,
}

/// Trusted source that required the approval.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalReasonV1 {
    /// Immutable system policy required approval.
    SystemPolicy,
    /// Trusted workspace policy tightened the baseline.
    WorkspacePolicy,
}

/// Effective lifecycle of one exact request and optional grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalStatusV1 {
    /// No decision has been stored.
    Pending,
    /// A live unused one-time grant exists.
    Active,
    /// The exact grant authorized one policy decision.
    Consumed,
    /// The live grant was withdrawn.
    Revoked,
    /// The exclusive expiry boundary passed.
    Expired,
    /// The request was explicitly rejected.
    Denied,
}

/// Exact E3 operation over one source/target path shape.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalFileOperationV1 {
    /// Create an absent target.
    Add,
    /// Replace one existing path.
    Update,
    /// Move one source to an absent target.
    Move,
    /// Remove one existing source.
    Delete,
}

/// Exact source/target shape for one E3 operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalFileV1 {
    operation: AgentApprovalFileOperationV1,
    source_path: Option<AgentInspectionPathV1>,
    target_path: Option<AgentInspectionPathV1>,
}

impl AgentApprovalFileV1 {
    /// Creates one Core-derived exact path operation.
    #[must_use]
    pub const fn new(
        operation: AgentApprovalFileOperationV1,
        source_path: Option<AgentInspectionPathV1>,
        target_path: Option<AgentInspectionPathV1>,
    ) -> Self {
        Self {
            operation,
            source_path,
            target_path,
        }
    }
}

/// Bounded patch rationale and every exact affected path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalPatchV1 {
    rationale: String,
    files: Vec<AgentApprovalFileV1>,
}

impl AgentApprovalPatchV1 {
    /// Creates an already validated exact patch projection.
    #[must_use]
    pub const fn new(rationale: String, files: Vec<AgentApprovalFileV1>) -> Self {
        Self { rationale, files }
    }
}

/// Worktree-relative process working directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentApprovalWorkingDirectoryV1 {
    /// Worktree root.
    Root,
    /// Exact repository-relative subtree.
    Subtree {
        /// Lossless path and safe display label.
        path: AgentInspectionPathV1,
    },
}

/// Process policy mode from the validated specification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalExecutionModeV1 {
    /// Current-plan discovered command.
    KnownSafe,
    /// Arbitrary direct-argv executable.
    Open,
    /// Explicit shell interpretation.
    Shell,
}

/// Exact current-plan binding of the process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentApprovalPlanBindingV1 {
    /// No validated step binding.
    Unbound,
    /// One exact current step authorized the command.
    Validated {
        /// Opaque durable step identity.
        step_id: String,
    },
}

/// Declarative network boundary of the process specification.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentApprovalNetworkV1 {
    /// Network use is not requested.
    Denied,
    /// One content-free target identity is requested.
    Requested {
        /// Stable target identity without credentials or address text.
        scope_digest: String,
    },
}

/// Full direct-argv ProcessSpec with environment names but never values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalProcessV1 {
    process_kind: AgentInspectionProcessKindV1,
    executable: String,
    arguments: Vec<String>,
    working_directory: AgentApprovalWorkingDirectoryV1,
    environment_allowlist: Vec<String>,
    timeout_millis: String,
    stdout_limit: u32,
    stderr_limit: u32,
    execution_mode: AgentApprovalExecutionModeV1,
    plan_binding: AgentApprovalPlanBindingV1,
    network: AgentApprovalNetworkV1,
    specification_id: String,
}

impl AgentApprovalProcessV1 {
    /// Creates a complete already validated direct-process presentation.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        process_kind: AgentInspectionProcessKindV1,
        executable: String,
        arguments: Vec<String>,
        working_directory: AgentApprovalWorkingDirectoryV1,
        environment_allowlist: Vec<String>,
        timeout_millis: String,
        stdout_limit: u32,
        stderr_limit: u32,
        execution_mode: AgentApprovalExecutionModeV1,
        plan_binding: AgentApprovalPlanBindingV1,
        network: AgentApprovalNetworkV1,
        specification_id: String,
    ) -> Self {
        Self {
            process_kind,
            executable,
            arguments,
            working_directory,
            environment_allowlist,
            timeout_millis,
            stdout_limit,
            stderr_limit,
            execution_mode,
            plan_binding,
            network,
            specification_id,
        }
    }
}

/// Closed exact mutation detail displayed before approval.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentApprovalActionV1 {
    /// Bounded E3 patch.
    Patch {
        /// Exact rationale and source/target paths.
        patch: AgentApprovalPatchV1,
    },
    /// Complete E4 process specification.
    Process {
        /// Direct argv, CWD, limits, environment names, plan, and network scope.
        process: AgentApprovalProcessV1,
    },
}

/// Current fully informed, exact, task-bound decision surface.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalV1 {
    approval_revision: String,
    ledger_revision: u32,
    ledger_store_version: String,
    controller_state: AgentControllerStateV1,
    step_status: TaskLensStepStatusV1,
    step_id: String,
    snapshot_id: String,
    scope_digest: String,
    action_class: AgentApprovalActionClassV1,
    risk: AgentApprovalRiskV1,
    reason: AgentApprovalReasonV1,
    requested_at_unix_millis: String,
    expires_at_unix_millis: String,
    status: AgentApprovalStatusV1,
    action: AgentApprovalActionV1,
    can_allow_once: bool,
    can_deny: bool,
    can_continue: bool,
    can_revoke: bool,
}

impl AgentApprovalV1 {
    /// Creates the complete Core-revalidated decision surface and available controls.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        approval_revision: String,
        ledger_revision: u32,
        ledger_store_version: String,
        controller_state: AgentControllerStateV1,
        step_status: TaskLensStepStatusV1,
        step_id: String,
        snapshot_id: String,
        scope_digest: String,
        action_class: AgentApprovalActionClassV1,
        risk: AgentApprovalRiskV1,
        reason: AgentApprovalReasonV1,
        requested_at_unix_millis: String,
        expires_at_unix_millis: String,
        status: AgentApprovalStatusV1,
        action: AgentApprovalActionV1,
        can_allow_once: bool,
        can_deny: bool,
        can_continue: bool,
        can_revoke: bool,
    ) -> Self {
        Self {
            approval_revision,
            ledger_revision,
            ledger_store_version,
            controller_state,
            step_status,
            step_id,
            snapshot_id,
            scope_digest,
            action_class,
            risk,
            reason,
            requested_at_unix_millis,
            expires_at_unix_millis,
            status,
            action,
            can_allow_once,
            can_deny,
            can_continue,
            can_revoke,
        }
    }
}

/// Expected bounded result of current approval inspection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum AgentApprovalResultV1 {
    /// No project is active.
    NoProject,
    /// The selected task no longer exists.
    TaskNotFound,
    /// The task has no durable Ledger yet.
    LedgerUnavailable,
    /// Goal and Ledger revisions differ.
    GoalRevisionMismatch {
        /// Current immutable Goal revision.
        current_revision: u32,
        /// Goal revision materialized by the Ledger.
        ledger_revision: u32,
    },
    /// Anchors changed during the bounded read.
    ActivityChanged,
    /// No same-process exact action presentation exists.
    Unavailable,
    /// A fully revalidated exact presentation is available.
    Available {
        /// Current approval details and controls.
        approval: Box<AgentApprovalV1>,
    },
}

/// Strict envelope for one approval inspection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentApprovalResultV1,
}

impl AgentApprovalResponseV1 {
    /// Creates a current-protocol response from a bounded Core result.
    #[must_use]
    pub const fn new(result: AgentApprovalResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the current explicit inspection result.
    #[must_use]
    pub const fn result(&self) -> &AgentApprovalResultV1 {
        &self.result
    }
}

/// Applied durable or scheduler-facing approval effect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalControlOutcomeV1 {
    /// Exact one-time grant is durable; execution has not started.
    GrantStored,
    /// Step was blocked and run failed without a tool effect.
    Denied,
    /// Active unused grant was withdrawn.
    Revoked,
    /// A new owned attempt was requested with the internal grant.
    ContinueRequested,
}

/// Immediate bounded scheduler result for Continue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentApprovalRuntimeStartV1 {
    /// No complete Agent executor is configured.
    Unavailable,
    /// Scheduler accepted the new attempt.
    Queued,
    /// Scheduler could not accept the attempt.
    Failed,
}

/// Expected bounded result of one explicit approval control.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum AgentApprovalControlResultV1 {
    /// No project is active.
    NoProject,
    /// The selected task no longer exists.
    TaskNotFound,
    /// The task has no durable Ledger yet.
    LedgerUnavailable,
    /// Goal and Ledger revisions differ.
    GoalRevisionMismatch,
    /// Visible optimistic anchors changed.
    ActivityChanged,
    /// Exact presentation or requested lifecycle action is unavailable.
    Unavailable,
    /// Decision was applied or Continue was submitted to the scheduler boundary.
    Applied {
        /// Stable applied effect.
        outcome: AgentApprovalControlOutcomeV1,
        /// Current process-local presentation revision.
        approval_revision: String,
        /// Current optimistic Ledger store version.
        ledger_store_version: String,
        /// Scheduler result only for Continue.
        runtime_start: Option<AgentApprovalRuntimeStartV1>,
    },
}

/// Strict envelope for an explicit approval control result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentApprovalControlResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentApprovalControlResultV1,
}

impl AgentApprovalControlResponseV1 {
    /// Creates a current-protocol response from a bounded Core result.
    #[must_use]
    pub const fn new(result: AgentApprovalControlResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the current explicit control result.
    #[must_use]
    pub const fn result(&self) -> &AgentApprovalControlResultV1 {
        &self.result
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentApprovalControlActionV1, ControlAgentApprovalRequestV1};
    use std::error::Error;

    #[test]
    fn control_accepts_only_closed_action_and_visible_anchors() -> Result<(), Box<dyn Error>> {
        let value = serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedApprovalRevision": "4",
            "expectedLedgerRevision": 3,
            "expectedLedgerStoreVersion": "8",
            "action": "allowOnce"
        });
        let request: ControlAgentApprovalRequestV1 = serde_json::from_value(value.clone())?;
        assert_eq!(request.action(), AgentApprovalControlActionV1::AllowOnce);

        let mut leaked = value;
        leaked["approvalId"] = serde_json::json!("22".repeat(32));
        assert!(serde_json::from_value::<ControlAgentApprovalRequestV1>(leaked).is_err());
        Ok(())
    }
}
