use crate::{AgentControllerStateV1, ProtocolVersion};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Requests current H11/E8 recovery controls for one selected durable task.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentTaskRecoveryRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl fmt::Debug for QueryAgentTaskRecoveryRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryAgentTaskRecoveryRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("has_task_id", &!self.task_id.is_empty())
            .finish()
    }
}

impl QueryAgentTaskRecoveryRequestV1 {
    /// Returns the protocol version before the opaque task identity is parsed.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the opaque task identity selected by the user.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

/// Closed explicit H11 recovery decision exposed to the untrusted WebView.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentTaskControlActionV1 {
    /// Continue only after Core verifies current snapshots and evidence.
    Resume,
    /// Reopen stale work and require a new plan before further model work.
    Replan,
    /// Atomically terminate the selected current run.
    Cancel,
}

/// Applies one recovery choice against the exact Ledger anchors shown to the user.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlAgentTaskRunRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    expected_ledger_revision: u32,
    expected_ledger_store_version: String,
    action: AgentTaskControlActionV1,
}

impl fmt::Debug for ControlAgentTaskRunRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlAgentTaskRunRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("has_task_id", &!self.task_id.is_empty())
            .field("expected_ledger_revision", &self.expected_ledger_revision)
            .field(
                "expected_ledger_store_version",
                &self.expected_ledger_store_version,
            )
            .field("action", &self.action)
            .finish()
    }
}

impl ControlAgentTaskRunRequestV1 {
    /// Returns the protocol version before any other request field is interpreted.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the opaque task identity selected by the user.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the exact visible Task Ledger revision used as an optimistic anchor.
    #[must_use]
    pub const fn expected_ledger_revision(&self) -> u32 {
        self.expected_ledger_revision
    }

    /// Returns the decimal optimistic Task Ledger store version.
    #[must_use]
    pub fn expected_ledger_store_version(&self) -> &str {
        &self.expected_ledger_store_version
    }

    /// Returns the explicit closed recovery decision.
    #[must_use]
    pub const fn action(&self) -> AgentTaskControlActionV1 {
        self.action
    }
}

/// Content-free current recovery state and exact optimistic control anchors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentTaskRecoveryV1 {
    ledger_revision: u32,
    ledger_store_version: String,
    state: AgentControllerStateV1,
    run_snapshot_id: String,
    published_snapshot_id: String,
    snapshot_changed: bool,
    interrupted_tool_attempts: u32,
    stale_evidence_count: u32,
    mutation_reconciliation_required: bool,
    mutation_replan_required: bool,
    can_resume: bool,
}

impl AgentTaskRecoveryV1 {
    /// Creates one projection from already validated Core-owned recovery facts.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        ledger_revision: u32,
        ledger_store_version: String,
        state: AgentControllerStateV1,
        run_snapshot_id: String,
        published_snapshot_id: String,
        snapshot_changed: bool,
        interrupted_tool_attempts: u32,
        stale_evidence_count: u32,
        mutation_reconciliation_required: bool,
        mutation_replan_required: bool,
        can_resume: bool,
    ) -> Self {
        Self {
            ledger_revision,
            ledger_store_version,
            state,
            run_snapshot_id,
            published_snapshot_id,
            snapshot_changed,
            interrupted_tool_attempts,
            stale_evidence_count,
            mutation_reconciliation_required,
            mutation_replan_required,
            can_resume,
        }
    }
}

/// Expected bounded result of recovery inspection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentTaskRecoveryResultV1 {
    /// No project is currently active in the Core.
    NoProject,
    /// The selected task no longer exists.
    TaskNotFound,
    /// The task exists but has no Task Ledger.
    LedgerUnavailable,
    /// Goal and Ledger revisions do not agree.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Ledger.
        ledger_revision: u32,
    },
    /// Durable state changed during the bounded read.
    ActivityChanged,
    /// No retained attempt has created a run.
    RunUnavailable,
    /// The latest run is terminal or no longer belongs to an active attempt.
    RunNotControllable {
        /// Last materialized finite controller state.
        state: AgentControllerStateV1,
    },
    /// Recovery controls are available against the returned exact anchors.
    Available {
        /// Current recovery projection.
        recovery: AgentTaskRecoveryV1,
    },
}

/// Strict envelope for current Agent task recovery controls.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentTaskRecoveryResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentTaskRecoveryResultV1,
}

impl AgentTaskRecoveryResponseV1 {
    /// Creates a response from an expected bounded Core result.
    #[must_use]
    pub const fn new(result: AgentTaskRecoveryResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the explicit current recovery inspection state.
    #[must_use]
    pub const fn result(&self) -> &AgentTaskRecoveryResultV1 {
        &self.result
    }
}

/// Stable effect of one atomically committed recovery decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentTaskControlOutcomeV1 {
    /// Recovery anchors were adopted and future continuation is allowed.
    Resumed,
    /// Invalid work was reopened and the normal replan path is now required.
    ReplanRequired,
    /// The run reached terminal Cancelled.
    Cancelled,
}

/// Expected bounded result of one explicit Agent task control command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentTaskControlResultV1 {
    /// No project is currently active in the Core.
    NoProject,
    /// The selected task no longer exists.
    TaskNotFound,
    /// The task exists but has no Task Ledger.
    LedgerUnavailable,
    /// Goal and Ledger revisions do not agree.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Ledger.
        ledger_revision: u32,
    },
    /// Optimistic anchors changed before the command could commit.
    ActivityChanged,
    /// No retained attempt has created a run.
    RunUnavailable,
    /// The latest run is terminal or no longer belongs to an active attempt.
    RunNotControllable {
        /// Last materialized finite controller state.
        state: AgentControllerStateV1,
    },
    /// An Unknown mutation must first receive a full authoritative reconciliation.
    MutationReconciliationRequired,
    /// Resume is unsafe and requires the explicit Replan or Cancel choice.
    ResumeRequiresReplan,
    /// The requested choice was atomically committed.
    Applied {
        /// Stable effect of the committed choice.
        outcome: AgentTaskControlOutcomeV1,
        /// New optimistic Ledger store version.
        ledger_store_version: String,
        /// Resulting finite controller state.
        state: AgentControllerStateV1,
        /// Number of stale completed steps reopened.
        reopened_step_count: u32,
        /// Number of abandoned tool attempts marked interrupted.
        interrupted_tool_attempts: u32,
    },
}

/// Strict envelope for one explicit Agent task control command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentTaskControlResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentTaskControlResultV1,
}

impl AgentTaskControlResponseV1 {
    /// Creates a response from an expected bounded Core result.
    #[must_use]
    pub const fn new(result: AgentTaskControlResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the explicit result of the attempted recovery decision.
    #[must_use]
    pub const fn result(&self) -> &AgentTaskControlResultV1 {
        &self.result
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentTaskControlActionV1, ControlAgentTaskRunRequestV1};
    use std::error::Error;

    #[test]
    fn control_request_is_closed_and_carries_no_run_or_snapshot_identity()
    -> Result<(), Box<dyn Error>> {
        let request: ControlAgentTaskRunRequestV1 = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedLedgerRevision": 2,
            "expectedLedgerStoreVersion": "7",
            "action": "cancel"
        }))?;
        assert_eq!(request.action(), AgentTaskControlActionV1::Cancel);
        let leaked = serde_json::from_value::<ControlAgentTaskRunRequestV1>(serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedLedgerRevision": 2,
            "expectedLedgerStoreVersion": "7",
            "action": "resume",
            "runId": "22".repeat(32)
        }));
        assert!(leaked.is_err());
        Ok(())
    }
}
