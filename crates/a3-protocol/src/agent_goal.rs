use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Whether one desktop-authored acceptance criterion gates task completion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentGoalCriterionRequirementV1 {
    /// Current successful evidence is mandatory before Done.
    Must,
    /// The outcome remains visible but does not independently block Done.
    Should,
}

/// Strict user-authored acceptance criterion used by create and revise commands.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalCriterionInputV1 {
    criterion_id: Option<String>,
    statement: String,
    requirement: AgentGoalCriterionRequirementV1,
}

impl fmt::Debug for AgentGoalCriterionInputV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentGoalCriterionInputV1")
            .field("has_criterion_id", &self.criterion_id.is_some())
            .field("statement_bytes", &self.statement.len())
            .field("requirement", &self.requirement)
            .finish_non_exhaustive()
    }
}

impl AgentGoalCriterionInputV1 {
    /// Returns the optional stable identity retained while revising an existing criterion.
    #[must_use]
    pub fn criterion_id(&self) -> Option<&str> {
        self.criterion_id.as_deref()
    }

    /// Returns the user-authored verification statement.
    #[must_use]
    pub fn statement(&self) -> &str {
        &self.statement
    }

    /// Returns whether the criterion is mandatory or advisory.
    #[must_use]
    pub const fn requirement(&self) -> AgentGoalCriterionRequirementV1 {
        self.requirement
    }
}

/// Strict user-authored content for one Goal Contract revision.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalDraftInputV1 {
    objective: String,
    acceptance_criteria: Vec<AgentGoalCriterionInputV1>,
    constraints: Vec<String>,
    non_goals: Vec<String>,
    user_decisions: Vec<String>,
    success_verification: String,
}

impl fmt::Debug for AgentGoalDraftInputV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentGoalDraftInputV1")
            .field("objective_bytes", &self.objective.len())
            .field("acceptance_criteria", &self.acceptance_criteria.len())
            .field("constraints", &self.constraints.len())
            .field("non_goals", &self.non_goals.len())
            .field("user_decisions", &self.user_decisions.len())
            .field(
                "success_verification_bytes",
                &self.success_verification.len(),
            )
            .finish_non_exhaustive()
    }
}

impl AgentGoalDraftInputV1 {
    /// Returns the required task outcome.
    #[must_use]
    pub fn objective(&self) -> &str {
        &self.objective
    }

    /// Returns the ordered criterion inputs.
    #[must_use]
    pub fn acceptance_criteria(&self) -> &[AgentGoalCriterionInputV1] {
        &self.acceptance_criteria
    }

    /// Returns the ordered task constraints.
    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    /// Returns the ordered explicitly excluded outcomes.
    #[must_use]
    pub fn non_goals(&self) -> &[String] {
        &self.non_goals
    }

    /// Returns the ordered choices confirmed by the user.
    #[must_use]
    pub fn user_decisions(&self) -> &[String] {
        &self.user_decisions
    }

    /// Returns the overall evidence-producing completion check.
    #[must_use]
    pub fn success_verification(&self) -> &str {
        &self.success_verification
    }
}

/// Requests one current Goal Contract from the active Core-owned worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentGoalRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl QueryAgentGoalRequestV1 {
    /// Returns the requested protocol version before any opaque value is parsed.
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

/// Requests atomic creation of a new task and its initial Goal Contract.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateAgentGoalRequestV1 {
    protocol_version: ProtocolVersion,
    draft: AgentGoalDraftInputV1,
}

impl fmt::Debug for CreateAgentGoalRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateAgentGoalRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("draft", &self.draft)
            .finish()
    }
}

impl CreateAgentGoalRequestV1 {
    /// Returns the requested protocol version before user content is interpreted.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the complete proposed initial contract content.
    #[must_use]
    pub const fn draft(&self) -> &AgentGoalDraftInputV1 {
        &self.draft
    }
}

/// Requests one immutable successor revision of an existing Goal Contract.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviseAgentGoalRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    expected_revision: u32,
    revision_reason: String,
    draft: AgentGoalDraftInputV1,
}

impl fmt::Debug for ReviseAgentGoalRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReviseAgentGoalRequestV1")
            .field("protocol_version", &self.protocol_version)
            .field("task_id", &self.task_id)
            .field("expected_revision", &self.expected_revision)
            .field("revision_reason_bytes", &self.revision_reason.len())
            .field("draft", &self.draft)
            .finish()
    }
}

impl ReviseAgentGoalRequestV1 {
    /// Returns the requested protocol version before opaque values or content are parsed.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the opaque task identity selected by the user.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the exact revision the editor was based on.
    #[must_use]
    pub const fn expected_revision(&self) -> u32 {
        self.expected_revision
    }

    /// Returns the required explanation for the material revision.
    #[must_use]
    pub fn revision_reason(&self) -> &str {
        &self.revision_reason
    }

    /// Returns the complete proposed successor content.
    #[must_use]
    pub const fn draft(&self) -> &AgentGoalDraftInputV1 {
        &self.draft
    }
}

/// WebView-safe criterion projected from one validated durable revision.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalCriterionV1 {
    criterion_id: String,
    statement: String,
    requirement: AgentGoalCriterionRequirementV1,
}

impl fmt::Debug for AgentGoalCriterionV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentGoalCriterionV1")
            .field("criterion_id", &self.criterion_id)
            .field("statement_bytes", &self.statement.len())
            .field("requirement", &self.requirement)
            .finish_non_exhaustive()
    }
}

impl AgentGoalCriterionV1 {
    /// Creates a criterion from already validated boundary primitives.
    #[must_use]
    pub fn new(
        criterion_id: String,
        statement: String,
        requirement: AgentGoalCriterionRequirementV1,
    ) -> Self {
        Self {
            criterion_id,
            statement,
            requirement,
        }
    }
}

/// Complete immutable Goal Contract revision shown in the Agent workspace.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalContractV1 {
    task_id: String,
    revision: u32,
    previous_revision: Option<u32>,
    revision_reason: Option<String>,
    objective: String,
    acceptance_criteria: Vec<AgentGoalCriterionV1>,
    constraints: Vec<String>,
    non_goals: Vec<String>,
    user_decisions: Vec<String>,
    success_verification: String,
    created_at_unix_millis: String,
}

impl fmt::Debug for AgentGoalContractV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentGoalContractV1")
            .field("task_id", &self.task_id)
            .field("revision", &self.revision)
            .field("previous_revision", &self.previous_revision)
            .field("has_revision_reason", &self.revision_reason.is_some())
            .field("objective_bytes", &self.objective.len())
            .field("acceptance_criteria", &self.acceptance_criteria.len())
            .field("constraints", &self.constraints.len())
            .field("non_goals", &self.non_goals.len())
            .field("user_decisions", &self.user_decisions.len())
            .field(
                "success_verification_bytes",
                &self.success_verification.len(),
            )
            .field("created_at_unix_millis", &self.created_at_unix_millis)
            .finish_non_exhaustive()
    }
}

impl AgentGoalContractV1 {
    /// Creates a projection from one fully validated domain revision.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_id: String,
        revision: u32,
        previous_revision: Option<u32>,
        revision_reason: Option<String>,
        objective: String,
        acceptance_criteria: Vec<AgentGoalCriterionV1>,
        constraints: Vec<String>,
        non_goals: Vec<String>,
        user_decisions: Vec<String>,
        success_verification: String,
        created_at_unix_millis: String,
    ) -> Self {
        Self {
            task_id,
            revision,
            previous_revision,
            revision_reason,
            objective,
            acceptance_criteria,
            constraints,
            non_goals,
            user_decisions,
            success_verification,
            created_at_unix_millis,
        }
    }
}

/// Expected read states for one selected task in the Agent workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentGoalResultV1 {
    /// No active Core-owned project exists.
    NoProject,
    /// The selected task is absent from the active worktree.
    TaskNotFound,
    /// The current immutable Goal Contract is available.
    Available {
        /// Current immutable revision loaded from the active worktree store.
        goal: Box<AgentGoalContractV1>,
    },
}

/// Versioned response for the current Goal Contract query.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentGoalResultV1,
}

impl AgentGoalResponseV1 {
    /// Creates the explicit no-project state.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(AgentGoalResultV1::NoProject)
    }

    /// Creates the explicit missing-task state.
    #[must_use]
    pub const fn task_not_found() -> Self {
        Self::with_result(AgentGoalResultV1::TaskNotFound)
    }

    /// Creates the available current Goal Contract state.
    #[must_use]
    pub fn available(goal: AgentGoalContractV1) -> Self {
        Self::with_result(AgentGoalResultV1::Available {
            goal: Box::new(goal),
        })
    }

    const fn with_result(result: AgentGoalResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the expected current-goal read state.
    #[must_use]
    pub const fn result(&self) -> &AgentGoalResultV1 {
        &self.result
    }
}

/// Successful result of one atomic Goal Contract creation or revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentGoalMutationResponseV1 {
    protocol_version: ProtocolVersion,
    goal: AgentGoalContractV1,
}

impl AgentGoalMutationResponseV1 {
    /// Creates a successful mutation response from the newly durable revision.
    #[must_use]
    pub const fn new(goal: AgentGoalContractV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            goal,
        }
    }

    /// Returns the newly persisted immutable revision.
    #[must_use]
    pub const fn goal(&self) -> &AgentGoalContractV1 {
        &self.goal
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentGoalContractV1, AgentGoalCriterionRequirementV1, AgentGoalCriterionV1,
        AgentGoalMutationResponseV1, ReviseAgentGoalRequestV1,
    };
    use serde_json::{json, to_value};

    #[test]
    fn agent_goal_projection_carries_must_and_should_without_protocol_ambiguity()
    -> Result<(), serde_json::Error> {
        let response = AgentGoalMutationResponseV1::new(AgentGoalContractV1::new(
            "11".repeat(32),
            2,
            Some(1),
            Some("scope clarified".to_owned()),
            "build the Agent workspace".to_owned(),
            vec![
                AgentGoalCriterionV1::new(
                    "22".repeat(32),
                    "goal remains visible".to_owned(),
                    AgentGoalCriterionRequirementV1::Must,
                ),
                AgentGoalCriterionV1::new(
                    "33".repeat(32),
                    "animation is polished".to_owned(),
                    AgentGoalCriterionRequirementV1::Should,
                ),
            ],
            vec!["remain local-only".to_owned()],
            vec!["do not start a run".to_owned()],
            vec!["show both requirements".to_owned()],
            "reopen the stored revision".to_owned(),
            "1786000000000".to_owned(),
        ));

        assert_eq!(
            to_value(response)?,
            json!({
                "protocolVersion": 1,
                "goal": {
                    "taskId": "11".repeat(32),
                    "revision": 2,
                    "previousRevision": 1,
                    "revisionReason": "scope clarified",
                    "objective": "build the Agent workspace",
                    "acceptanceCriteria": [
                        {"criterionId": "22".repeat(32), "statement": "goal remains visible", "requirement": "must"},
                        {"criterionId": "33".repeat(32), "statement": "animation is polished", "requirement": "should"}
                    ],
                    "constraints": ["remain local-only"],
                    "nonGoals": ["do not start a run"],
                    "userDecisions": ["show both requirements"],
                    "successVerification": "reopen the stored revision",
                    "createdAtUnixMillis": "1786000000000"
                }
            })
        );
        Ok(())
    }

    #[test]
    fn revise_request_rejects_unknown_nested_fields() {
        let request = json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedRevision": 1,
            "revisionReason": "scope clarified",
            "draft": {
                "objective": "goal",
                "acceptanceCriteria": [{
                    "criterionId": "22".repeat(32),
                    "statement": "verified",
                    "requirement": "must",
                    "executable": true
                }],
                "constraints": [],
                "nonGoals": [],
                "userDecisions": [],
                "successVerification": "run tests"
            }
        });

        assert!(serde_json::from_value::<ReviseAgentGoalRequestV1>(request).is_err());
    }

    #[test]
    fn debug_output_redacts_all_user_authored_goal_text() -> Result<(), serde_json::Error> {
        let secret = "PRIVATE-GOAL-TEXT";
        let request = json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "expectedRevision": 1,
            "revisionReason": secret,
            "draft": {
                "objective": secret,
                "acceptanceCriteria": [{
                    "criterionId": "22".repeat(32),
                    "statement": secret,
                    "requirement": "must"
                }],
                "constraints": [secret],
                "nonGoals": [secret],
                "userDecisions": [secret],
                "successVerification": secret
            }
        });
        let request = serde_json::from_value::<ReviseAgentGoalRequestV1>(request)?;

        let debug = format!("{request:?}");
        assert!(!debug.contains(secret));
        assert!(debug.contains("revision_reason_bytes"));
        assert!(debug.contains("objective_bytes"));
        Ok(())
    }
}
