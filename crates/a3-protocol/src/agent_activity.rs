use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Requests bounded current execution activity for one selected durable task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentActivityRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl QueryAgentActivityRequestV1 {
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

/// Finite materialized controller state shown without granting control capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentControllerStateV1 {
    /// Validate and anchor the requested task.
    Intake,
    /// Retrieve deterministic repository context.
    Localize,
    /// Construct or inspect the typed task plan.
    Plan,
    /// Execute one allowed action for the current step.
    Execute,
    /// Verify the produced outcome against explicit evidence.
    Verify,
    /// Replace future plan steps after a material finding.
    Replan,
    /// Wait for one scoped user approval.
    AwaitApproval,
    /// Terminal successful state after acceptance verification.
    Done,
    /// Terminal unsuccessful state.
    Failed,
    /// Terminal user- or policy-requested cancellation.
    Cancelled,
}

/// Coarse result of one content-free durable journal event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentActivityOutcomeV1 {
    /// The operation completed successfully.
    Succeeded,
    /// The operation failed.
    Failed,
    /// The operation was cancelled.
    Cancelled,
    /// Policy or the user denied the operation.
    Denied,
}

/// Stable content-free reason retained by the run journal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentActivityCodeV1 {
    /// No additional reason was retained.
    None,
    /// The user explicitly requested the action.
    UserRequest,
    /// The deterministic controller selected the transition.
    ControllerDecision,
    /// Central policy allowed or denied an action.
    PolicyDecision,
    /// A bounded operation reached its deadline.
    Timeout,
    /// Cooperative cancellation was observed.
    Cancellation,
    /// Structured model output was rejected.
    InvalidModelOutput,
    /// A bounded tool operation failed.
    ToolFailure,
    /// Deterministic verification did not pass.
    VerificationFailure,
    /// Durable state was recovered after restart.
    StateRecovered,
}

/// Coarse model-selected action class; selection alone is not execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSelectedActionV1 {
    /// Deterministic bounded retrieval.
    Search,
    /// Targeted bounded read-only inspection.
    Inspect,
    /// Safe non-verifying Task Ledger update.
    UpdateLedger,
    /// Request for deterministic acceptance verification.
    Finish,
    /// One complete structured full-file patch.
    ApplyPatch,
    /// One discovered and plan-bound direct process.
    Run,
}

/// Resource charge and optional action selection for one bounded model turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityTurnV1 {
    selected_action: Option<AgentSelectedActionV1>,
    prompt_tokens: u32,
    output_tokens: u32,
    repair_used: bool,
}

impl AgentActivityTurnV1 {
    /// Creates one turn charge from already validated materialized counters.
    #[must_use]
    pub const fn new(
        selected_action: Option<AgentSelectedActionV1>,
        prompt_tokens: u32,
        output_tokens: u32,
        repair_used: bool,
    ) -> Self {
        Self {
            selected_action,
            prompt_tokens,
            output_tokens,
            repair_used,
        }
    }
}

/// Event-specific fields preserving the distinction between model output and tool execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentActivityEventKindV1 {
    /// Mandatory first event for the selected run.
    RunStarted,
    /// Validated transition between finite controller states.
    StateTransition {
        /// Materialized state before the transition.
        from: AgentControllerStateV1,
        /// Materialized state after the transition.
        to: AgentControllerStateV1,
    },
    /// A deterministic context pack was compiled.
    ContextCompiled,
    /// A model response completed; this does not itself execute an action.
    ModelInteraction {
        /// Charge for a current run, or absent only for a legacy journal event.
        turn: Option<AgentActivityTurnV1>,
    },
    /// A real bounded tool action was requested or completed.
    ToolAction,
    /// The Task Ledger advanced through one validated replan.
    LedgerUpdated {
        /// Materialized revision before the replan.
        from_revision: u32,
        /// Immediate next materialized revision.
        to_revision: u32,
    },
    /// Deterministic verification was recorded.
    VerificationRecorded,
    /// A scoped approval was requested or resolved.
    ApprovalRecorded,
    /// A content-free diagnostic marker was recorded.
    Diagnostic,
}

/// One bounded WebView-safe item from the immutable run journal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityEventV1 {
    sequence: String,
    occurred_at_unix_millis: String,
    snapshot_id: String,
    event: AgentActivityEventKindV1,
    code: AgentActivityCodeV1,
    outcome: Option<AgentActivityOutcomeV1>,
}

impl AgentActivityEventV1 {
    /// Creates one event from already validated content-free primitives.
    #[must_use]
    pub fn new(
        sequence: String,
        occurred_at_unix_millis: String,
        snapshot_id: String,
        event: AgentActivityEventKindV1,
        code: AgentActivityCodeV1,
        outcome: Option<AgentActivityOutcomeV1>,
    ) -> Self {
        Self {
            sequence,
            occurred_at_unix_millis,
            snapshot_id,
            event,
            code,
            outcome,
        }
    }
}

/// Immutable hard ceilings selected when the displayed run started.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityBudgetV1 {
    turn_limit: u32,
    prompt_token_limit: String,
    output_token_limit: String,
    action_limit: u32,
    duration_limit_millis: String,
    repair_limit: u32,
}

impl AgentActivityBudgetV1 {
    /// Creates the six-dimensional immutable run budget projection.
    #[must_use]
    pub fn new(
        turn_limit: u32,
        prompt_token_limit: String,
        output_token_limit: String,
        action_limit: u32,
        duration_limit_millis: String,
        repair_limit: u32,
    ) -> Self {
        Self {
            turn_limit,
            prompt_token_limit,
            output_token_limit,
            action_limit,
            duration_limit_millis,
            repair_limit,
        }
    }
}

/// Durable cumulative usage observed through the displayed run's latest event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityUsageV1 {
    turn_count: u32,
    prompt_tokens: String,
    output_tokens: String,
    action_count: u32,
    elapsed_at_last_event_millis: String,
    repair_count: u32,
}

impl AgentActivityUsageV1 {
    /// Creates the durable usage projection without evaluating it in the WebView.
    #[must_use]
    pub fn new(
        turn_count: u32,
        prompt_tokens: String,
        output_tokens: String,
        action_count: u32,
        elapsed_at_last_event_millis: String,
        repair_count: u32,
    ) -> Self {
        Self {
            turn_count,
            prompt_tokens,
            output_tokens,
            action_count,
            elapsed_at_last_event_millis,
            repair_count,
        }
    }
}

/// Ledger state that currently prevents a selected task step from continuing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentActivityBlockerStatusV1 {
    /// The attempt stopped on a retained execution blocker.
    Blocked,
    /// The active attempt requires scoped user approval.
    AwaitingApproval,
}

/// One bounded current blocker projected directly from an active-plan step.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityBlockerV1 {
    step_id: String,
    status: AgentActivityBlockerStatusV1,
    reason: String,
}

impl AgentActivityBlockerV1 {
    /// Creates one blocker from a validated ledger reason.
    #[must_use]
    pub fn new(step_id: String, status: AgentActivityBlockerStatusV1, reason: String) -> Self {
        Self {
            step_id,
            status,
            reason,
        }
    }
}

impl fmt::Debug for AgentActivityBlockerV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentActivityBlockerV1")
            .field("step_id", &self.step_id)
            .field("status", &self.status)
            .field("reason_bytes", &self.reason.len())
            .finish_non_exhaustive()
    }
}

/// Materialized selected run with bounded recent timeline and budget state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityRunV1 {
    run_id: String,
    step_id: String,
    attempt_number: u32,
    ledger_revision: u32,
    ledger_revision_matches_current: bool,
    state: AgentControllerStateV1,
    terminal: bool,
    current_snapshot_id: String,
    created_at_unix_millis: String,
    updated_at_unix_millis: String,
    budget: AgentActivityBudgetV1,
    usage: AgentActivityUsageV1,
    earlier_events_omitted: bool,
    timeline: Vec<AgentActivityEventV1>,
}

impl AgentActivityRunV1 {
    /// Creates a complete bounded run read model from validated primitives.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: String,
        step_id: String,
        attempt_number: u32,
        ledger_revision: u32,
        ledger_revision_matches_current: bool,
        state: AgentControllerStateV1,
        terminal: bool,
        current_snapshot_id: String,
        created_at_unix_millis: String,
        updated_at_unix_millis: String,
        budget: AgentActivityBudgetV1,
        usage: AgentActivityUsageV1,
        earlier_events_omitted: bool,
        timeline: Vec<AgentActivityEventV1>,
    ) -> Self {
        Self {
            run_id,
            step_id,
            attempt_number,
            ledger_revision,
            ledger_revision_matches_current,
            state,
            terminal,
            current_snapshot_id,
            created_at_unix_millis,
            updated_at_unix_millis,
            budget,
            usage,
            earlier_events_omitted,
            timeline,
        }
    }
}

/// Current task activity projected from one revalidated ledger anchor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityV1 {
    current_ledger_revision: u32,
    ledger_store_version: String,
    blockers: Vec<AgentActivityBlockerV1>,
    run: Option<Box<AgentActivityRunV1>>,
}

impl AgentActivityV1 {
    /// Creates one current activity projection with no mutation capability.
    #[must_use]
    pub fn new(
        current_ledger_revision: u32,
        ledger_store_version: String,
        blockers: Vec<AgentActivityBlockerV1>,
        run: Option<AgentActivityRunV1>,
    ) -> Self {
        Self {
            current_ledger_revision,
            ledger_store_version,
            blockers,
            run: run.map(Box::new),
        }
    }
}

/// Expected read states for the selected task's execution activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentActivityResultV1 {
    /// No active Core-owned project exists.
    NoProject,
    /// The selected task is absent from the active worktree.
    TaskNotFound,
    /// The selected task has no materialized Task Ledger yet.
    LedgerUnavailable,
    /// The current goal and materialized ledger refer to different revisions.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Task Ledger.
        ledger_revision: u32,
    },
    /// One of the durable anchors changed during the bounded read.
    ActivityChanged,
    /// A revalidated current activity projection is available.
    Available {
        /// Current ledger, blockers, budget, and bounded timeline.
        activity: Box<AgentActivityV1>,
    },
}

/// Versioned response containing only bounded current execution activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentActivityResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentActivityResultV1,
}

impl AgentActivityResponseV1 {
    /// Creates the explicit no-project state.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(AgentActivityResultV1::NoProject)
    }

    /// Creates the explicit missing-task state.
    #[must_use]
    pub const fn task_not_found() -> Self {
        Self::with_result(AgentActivityResultV1::TaskNotFound)
    }

    /// Creates the explicit no-ledger state.
    #[must_use]
    pub const fn ledger_unavailable() -> Self {
        Self::with_result(AgentActivityResultV1::LedgerUnavailable)
    }

    /// Creates the explicit immutable-goal mismatch state.
    #[must_use]
    pub const fn goal_revision_mismatch(current_revision: u32, ledger_revision: u32) -> Self {
        Self::with_result(AgentActivityResultV1::GoalRevisionMismatch {
            current_revision,
            ledger_revision,
        })
    }

    /// Creates the explicit concurrent-anchor-change state.
    #[must_use]
    pub const fn activity_changed() -> Self {
        Self::with_result(AgentActivityResultV1::ActivityChanged)
    }

    /// Creates the available bounded current activity state.
    #[must_use]
    pub fn available(activity: AgentActivityV1) -> Self {
        Self::with_result(AgentActivityResultV1::Available {
            activity: Box::new(activity),
        })
    }

    const fn with_result(result: AgentActivityResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the explicit current activity read state.
    #[must_use]
    pub const fn result(&self) -> &AgentActivityResultV1 {
        &self.result
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentActivityBlockerStatusV1, AgentActivityBlockerV1, AgentActivityBudgetV1,
        AgentActivityCodeV1, AgentActivityEventKindV1, AgentActivityEventV1,
        AgentActivityOutcomeV1, AgentActivityResponseV1, AgentActivityRunV1, AgentActivityTurnV1,
        AgentActivityUsageV1, AgentActivityV1, AgentControllerStateV1, AgentSelectedActionV1,
        QueryAgentActivityRequestV1,
    };
    use serde_json::json;

    #[test]
    fn activity_request_rejects_unknown_fields() {
        let result = serde_json::from_str::<QueryAgentActivityRequestV1>(
            r#"{"protocolVersion":"v1","taskId":"00","runId":"untrusted"}"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn activity_projection_keeps_action_selection_distinct_from_tool_execution()
    -> Result<(), serde_json::Error> {
        let response = AgentActivityResponseV1::available(AgentActivityV1::new(
            2,
            "7".to_owned(),
            vec![AgentActivityBlockerV1::new(
                "33".repeat(32),
                AgentActivityBlockerStatusV1::AwaitingApproval,
                "approval required".to_owned(),
            )],
            Some(AgentActivityRunV1::new(
                "22".repeat(32),
                "33".repeat(32),
                1,
                2,
                true,
                AgentControllerStateV1::AwaitApproval,
                false,
                "44".repeat(32),
                "100".to_owned(),
                "120".to_owned(),
                AgentActivityBudgetV1::new(
                    8,
                    "8000".to_owned(),
                    "2000".to_owned(),
                    8,
                    "60000".to_owned(),
                    2,
                ),
                AgentActivityUsageV1::new(
                    1,
                    "120".to_owned(),
                    "40".to_owned(),
                    1,
                    "20".to_owned(),
                    0,
                ),
                false,
                vec![
                    AgentActivityEventV1::new(
                        "1".to_owned(),
                        "100".to_owned(),
                        "44".repeat(32),
                        AgentActivityEventKindV1::RunStarted,
                        AgentActivityCodeV1::None,
                        None,
                    ),
                    AgentActivityEventV1::new(
                        "2".to_owned(),
                        "110".to_owned(),
                        "44".repeat(32),
                        AgentActivityEventKindV1::ModelInteraction {
                            turn: Some(AgentActivityTurnV1::new(
                                Some(AgentSelectedActionV1::Run),
                                120,
                                40,
                                false,
                            )),
                        },
                        AgentActivityCodeV1::None,
                        Some(AgentActivityOutcomeV1::Succeeded),
                    ),
                    AgentActivityEventV1::new(
                        "3".to_owned(),
                        "120".to_owned(),
                        "44".repeat(32),
                        AgentActivityEventKindV1::ToolAction,
                        AgentActivityCodeV1::PolicyDecision,
                        Some(AgentActivityOutcomeV1::Denied),
                    ),
                ],
            )),
        ));
        let value = serde_json::to_value(response)?;
        assert_eq!(
            value["result"]["activity"]["run"]["timeline"][1]["event"],
            json!({
                "kind": "modelInteraction",
                "turn": {
                    "selectedAction": "run",
                    "promptTokens": 120,
                    "outputTokens": 40,
                    "repairUsed": false
                }
            })
        );
        assert_eq!(
            value["result"]["activity"]["run"]["timeline"][2]["event"],
            json!({ "kind": "toolAction" })
        );
        Ok(())
    }
}
