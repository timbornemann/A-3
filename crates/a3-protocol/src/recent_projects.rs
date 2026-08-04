use crate::{ProjectSummaryV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict input payload for the V1 recent-project query.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListRecentProjectsRequestV1 {
    protocol_version: ProtocolVersion,
}

impl ListRecentProjectsRequestV1 {
    /// Creates a request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a request for the protocol version emitted by this build.
    #[must_use]
    pub const fn current() -> Self {
        Self::new(ProtocolVersion::CURRENT)
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Versioned bounded response containing most-recent-first project summaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecentProjectsResponseV1 {
    protocol_version: ProtocolVersion,
    projects: Vec<RecentProjectSummaryV1>,
}

impl RecentProjectsResponseV1 {
    /// Creates a response from already bounded and validated projections.
    #[must_use]
    pub const fn new(projects: Vec<RecentProjectSummaryV1>) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            projects,
        }
    }

    /// Returns the response protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns recent projects in descending open order.
    #[must_use]
    pub fn projects(&self) -> &[RecentProjectSummaryV1] {
        &self.projects
    }
}

/// WebView-safe catalog identity and existing project summary projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecentProjectSummaryV1 {
    project_id: String,
    project: ProjectSummaryV1,
}

impl RecentProjectSummaryV1 {
    /// Creates a recent-project projection from boundary primitives.
    #[must_use]
    pub const fn new(project_id: String, project: ProjectSummaryV1) -> Self {
        Self {
            project_id,
            project,
        }
    }

    /// Returns the lowercase catalog project identity digest.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Returns the existing safe project summary.
    #[must_use]
    pub const fn project(&self) -> &ProjectSummaryV1 {
        &self.project
    }
}

#[cfg(test)]
mod tests {
    use super::{RecentProjectSummaryV1, RecentProjectsResponseV1};
    use crate::{GitHeadV1, ProjectSummaryV1, ProtocolVersion};
    use serde_json::json;

    #[test]
    fn recent_projects_response_has_stable_json_shape() -> Result<(), serde_json::Error> {
        let response = RecentProjectsResponseV1::new(vec![RecentProjectSummaryV1::new(
            "33".repeat(32),
            ProjectSummaryV1::new(
                "11".repeat(32),
                "22".repeat(32),
                "/worktree".to_owned(),
                GitHeadV1::Unborn {
                    reference: "refs/heads/main".to_owned(),
                },
            ),
        )]);

        assert_eq!(
            serde_json::to_value(&response)?,
            json!({
                "protocolVersion": 1,
                "projects": [{
                    "projectId": "33".repeat(32),
                    "project": {
                        "repositoryId": "11".repeat(32),
                        "worktreeId": "22".repeat(32),
                        "worktreeRootDisplay": "/worktree",
                        "head": {
                            "kind": "unborn",
                            "reference": "refs/heads/main"
                        }
                    }
                }]
            })
        );
        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        Ok(())
    }
}
