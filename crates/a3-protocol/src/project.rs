use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict input payload for the V1 project-open command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpenProjectRequestV1 {
    protocol_version: ProtocolVersion,
}

impl OpenProjectRequestV1 {
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

/// Versioned response from one native project selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpenProjectResponseV1 {
    protocol_version: ProtocolVersion,
    result: OpenProjectResultV1,
}

impl OpenProjectResponseV1 {
    /// Creates a response for a picker dismissed by the user.
    #[must_use]
    pub const fn cancelled() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: OpenProjectResultV1::Cancelled,
        }
    }

    /// Creates a response for a safely inspected local worktree.
    #[must_use]
    pub const fn opened(project: ProjectSummaryV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: OpenProjectResultV1::Opened { project },
        }
    }

    /// Returns the protocol version carried by this response.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the native-selection result.
    #[must_use]
    pub const fn result(&self) -> &OpenProjectResultV1 {
        &self.result
    }
}

/// Mutually exclusive result of the project-open command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum OpenProjectResultV1 {
    /// The user dismissed the native directory picker.
    Cancelled,
    /// A selected Git worktree was safely identified.
    Opened {
        /// Bounded protocol projection of the project identity.
        project: ProjectSummaryV1,
    },
}

/// Safe project identity projected to the untrusted WebView.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectSummaryV1 {
    repository_id: String,
    worktree_id: String,
    worktree_root_display: String,
    head: GitHeadV1,
}

impl ProjectSummaryV1 {
    /// Creates a WebView-safe project summary from boundary primitives.
    #[must_use]
    pub fn new(
        repository_id: String,
        worktree_id: String,
        worktree_root_display: String,
        head: GitHeadV1,
    ) -> Self {
        Self {
            repository_id,
            worktree_id,
            worktree_root_display,
            head,
        }
    }

    /// Returns the lowercase repository identity digest.
    #[must_use]
    pub fn repository_id(&self) -> &str {
        &self.repository_id
    }

    /// Returns the lowercase worktree identity digest.
    #[must_use]
    pub fn worktree_id(&self) -> &str {
        &self.worktree_id
    }

    /// Returns the non-authoritative worktree path intended only for display.
    #[must_use]
    pub fn worktree_root_display(&self) -> &str {
        &self.worktree_root_display
    }

    /// Returns the observed Git HEAD projection.
    #[must_use]
    pub const fn head(&self) -> &GitHeadV1 {
        &self.head
    }
}

/// WebView-safe Git HEAD state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum GitHeadV1 {
    /// HEAD resolves to an object, optionally through a branch reference.
    Born {
        /// Lowercase SHA-1 or SHA-256 object identity.
        object_id: String,
        /// Full branch reference, absent for detached HEAD.
        reference: Option<String>,
    },
    /// HEAD points at a branch with no commit yet.
    Unborn {
        /// Full branch reference that will receive the first commit.
        reference: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{GitHeadV1, OpenProjectRequestV1, OpenProjectResponseV1, ProjectSummaryV1};
    use crate::ProtocolVersion;
    use serde_json::json;

    #[test]
    fn opened_project_response_has_stable_json_shape() -> Result<(), serde_json::Error> {
        let response = OpenProjectResponseV1::opened(ProjectSummaryV1::new(
            "11".repeat(32),
            "22".repeat(32),
            "C:\\worktree".to_owned(),
            GitHeadV1::Unborn {
                reference: "refs/heads/main".to_owned(),
            },
        ));

        assert_eq!(
            serde_json::to_value(&response)?,
            json!({
                "protocolVersion": 1,
                "result": {
                    "status": "opened",
                    "project": {
                        "repositoryId": "11".repeat(32),
                        "worktreeId": "22".repeat(32),
                        "worktreeRootDisplay": "C:\\worktree",
                        "head": {
                            "kind": "unborn",
                            "reference": "refs/heads/main"
                        }
                    }
                }
            })
        );
        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        Ok(())
    }

    #[test]
    fn project_request_rejects_unknown_fields() {
        let result = serde_json::from_value::<OpenProjectRequestV1>(json!({
            "protocolVersion": 1,
            "selectedPath": "untrusted"
        }));

        assert!(result.is_err());
    }
}
