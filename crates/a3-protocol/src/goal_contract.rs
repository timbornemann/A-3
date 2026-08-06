use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};
use std::fmt;

/// WebView-safe projection of one stable Goal Contract acceptance criterion.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AcceptanceCriterionV1 {
    criterion_id: String,
    statement: String,
}

impl fmt::Debug for AcceptanceCriterionV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptanceCriterionV1")
            .field("criterion_id", &self.criterion_id)
            .field("statement_bytes", &self.statement.len())
            .finish_non_exhaustive()
    }
}

impl AcceptanceCriterionV1 {
    /// Creates a criterion projection from already validated boundary primitives.
    #[must_use]
    pub fn new(criterion_id: String, statement: String) -> Self {
        Self {
            criterion_id,
            statement,
        }
    }

    /// Returns the lowercase criterion identity digest.
    #[must_use]
    pub fn criterion_id(&self) -> &str {
        &self.criterion_id
    }

    /// Returns the normalized verification statement.
    #[must_use]
    pub fn statement(&self) -> &str {
        &self.statement
    }
}

/// Bounded content carried by one immutable Goal Contract revision.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GoalContractDraftV1 {
    objective: String,
    acceptance_criteria: Vec<AcceptanceCriterionV1>,
    constraints: Vec<String>,
    non_goals: Vec<String>,
    user_decisions: Vec<String>,
    success_verification: String,
}

impl fmt::Debug for GoalContractDraftV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoalContractDraftV1")
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

impl GoalContractDraftV1 {
    /// Creates a UI projection from a domain-validated Goal Contract draft.
    #[must_use]
    pub fn new(
        objective: String,
        acceptance_criteria: Vec<AcceptanceCriterionV1>,
        constraints: Vec<String>,
        non_goals: Vec<String>,
        user_decisions: Vec<String>,
        success_verification: String,
    ) -> Self {
        Self {
            objective,
            acceptance_criteria,
            constraints,
            non_goals,
            user_decisions,
            success_verification,
        }
    }

    /// Returns the required task outcome.
    #[must_use]
    pub fn objective(&self) -> &str {
        &self.objective
    }

    /// Returns the ordered independently verifiable success conditions.
    #[must_use]
    pub fn acceptance_criteria(&self) -> &[AcceptanceCriterionV1] {
        &self.acceptance_criteria
    }

    /// Returns the ordered mandatory task boundaries.
    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    /// Returns the ordered outcomes excluded from the task.
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

/// Versioned, WebView-safe projection of one immutable Goal Contract revision.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GoalContractV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    revision: u32,
    previous_revision: Option<u32>,
    revision_reason: Option<String>,
    objective: String,
    acceptance_criteria: Vec<AcceptanceCriterionV1>,
    constraints: Vec<String>,
    non_goals: Vec<String>,
    user_decisions: Vec<String>,
    success_verification: String,
    created_at_unix_millis: String,
}

impl fmt::Debug for GoalContractV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoalContractV1")
            .field("protocol_version", &self.protocol_version)
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

impl GoalContractV1 {
    /// Creates a V1 projection from one domain-validated immutable revision.
    #[must_use]
    pub fn new(
        task_id: String,
        revision: u32,
        previous_revision: Option<u32>,
        revision_reason: Option<String>,
        draft: GoalContractDraftV1,
        created_at_unix_millis: String,
    ) -> Self {
        let GoalContractDraftV1 {
            objective,
            acceptance_criteria,
            constraints,
            non_goals,
            user_decisions,
            success_verification,
        } = draft;
        Self {
            protocol_version: ProtocolVersion::CURRENT,
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

    /// Returns the protocol version carried by this projection.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the lowercase task identity digest.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the non-zero immutable revision number.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    /// Returns the immediate predecessor, absent only for revision one.
    #[must_use]
    pub const fn previous_revision(&self) -> Option<u32> {
        self.previous_revision
    }

    /// Returns the required material-change reason, absent only for revision one.
    #[must_use]
    pub fn revision_reason(&self) -> Option<&str> {
        self.revision_reason.as_deref()
    }

    /// Returns the required task outcome.
    #[must_use]
    pub fn objective(&self) -> &str {
        &self.objective
    }

    /// Returns the ordered independently verifiable success conditions.
    #[must_use]
    pub fn acceptance_criteria(&self) -> &[AcceptanceCriterionV1] {
        &self.acceptance_criteria
    }

    /// Returns the ordered mandatory task boundaries.
    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    /// Returns the ordered outcomes excluded from the task.
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

    /// Returns exact Unix milliseconds as a decimal string safe across the JS boundary.
    #[must_use]
    pub fn created_at_unix_millis(&self) -> &str {
        &self.created_at_unix_millis
    }
}

#[cfg(test)]
mod tests {
    use super::{AcceptanceCriterionV1, GoalContractDraftV1, GoalContractV1};
    use crate::ProtocolVersion;
    use serde_json::json;

    #[test]
    fn goal_contract_revision_has_stable_json_shape() -> Result<(), serde_json::Error> {
        let contract = GoalContractV1::new(
            "11".repeat(32),
            2,
            Some(1),
            Some("the user clarified the outcome".to_owned()),
            GoalContractDraftV1::new(
                "implement the durable goal".to_owned(),
                vec![AcceptanceCriterionV1::new(
                    "22".repeat(32),
                    "the goal survives restart".to_owned(),
                )],
                vec!["remain local-only".to_owned()],
                vec!["do not start the controller".to_owned()],
                vec!["retain old revisions".to_owned()],
                "reopen and compare both revisions".to_owned(),
            ),
            "1786000000000".to_owned(),
        );

        assert_eq!(
            serde_json::to_value(&contract)?,
            json!({
                "protocolVersion": 1,
                "taskId": "11".repeat(32),
                "revision": 2,
                "previousRevision": 1,
                "revisionReason": "the user clarified the outcome",
                "objective": "implement the durable goal",
                "acceptanceCriteria": [{
                    "criterionId": "22".repeat(32),
                    "statement": "the goal survives restart"
                }],
                "constraints": ["remain local-only"],
                "nonGoals": ["do not start the controller"],
                "userDecisions": ["retain old revisions"],
                "successVerification": "reopen and compare both revisions",
                "createdAtUnixMillis": "1786000000000"
            })
        );
        assert_eq!(contract.protocol_version(), ProtocolVersion::V1);
        assert_eq!(contract.revision(), 2);
        assert_eq!(contract.previous_revision(), Some(1));
        Ok(())
    }

    #[test]
    fn goal_contract_projection_rejects_unknown_nested_fields() {
        let result = serde_json::from_value::<GoalContractV1>(json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "revision": 1,
            "previousRevision": null,
            "revisionReason": null,
            "objective": "goal",
            "acceptanceCriteria": [{
                "criterionId": "22".repeat(32),
                "statement": "verified",
                "executable": true
            }],
            "constraints": [],
            "nonGoals": [],
            "userDecisions": [],
            "successVerification": "run the verifier",
            "createdAtUnixMillis": "1"
        }));

        assert!(result.is_err());
    }
}
