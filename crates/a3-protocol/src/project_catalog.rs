use crate::{ProjectSummaryV1, ProtocolVersion, RecentProjectSummaryV1};
use serde::{Deserialize, Serialize};

/// Cursor movement requested for a fixed-size catalog page.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectCatalogDirectionV1 {
    /// Reads the first page and forbids a cursor.
    Initial,
    /// Reads entries activated before the cursor.
    Next,
    /// Reads entries activated after the cursor.
    Previous,
}

/// Strict, pathless V1 request for one project-catalog page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectCatalogRequestV1 {
    protocol_version: ProtocolVersion,
    search: Option<String>,
    cursor: Option<String>,
    direction: ProjectCatalogDirectionV1,
}

impl QueryProjectCatalogRequestV1 {
    /// Creates a catalog request from boundary primitives.
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        search: Option<String>,
        cursor: Option<String>,
        direction: ProjectCatalogDirectionV1,
    ) -> Self {
        Self {
            protocol_version,
            search,
            cursor,
            direction,
        }
    }

    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    #[must_use]
    /// Returns the optional bounded search text.
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    #[must_use]
    /// Returns the optional opaque navigation cursor.
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[must_use]
    /// Returns the requested cursor direction.
    pub const fn direction(&self) -> ProjectCatalogDirectionV1 {
        self.direction
    }
}

/// Fixed-size safe project catalog page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectCatalogResponseV1 {
    protocol_version: ProtocolVersion,
    projects: Vec<RecentProjectSummaryV1>,
    previous_cursor: Option<String>,
    next_cursor: Option<String>,
}

impl ProjectCatalogResponseV1 {
    /// Creates a response from a Core-bounded page.
    #[must_use]
    pub const fn new(
        projects: Vec<RecentProjectSummaryV1>,
        previous_cursor: Option<String>,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            projects,
            previous_cursor,
            next_cursor,
        }
    }
}

/// Strict request for activating one ID obtained from the catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivateCatalogProjectRequestV1 {
    protocol_version: ProtocolVersion,
    worktree_id: String,
}

impl ActivateCatalogProjectRequestV1 {
    #[must_use]
    /// Creates a pathless activation request for one listed worktree ID.
    pub const fn new(protocol_version: ProtocolVersion, worktree_id: String) -> Self {
        Self {
            protocol_version,
            worktree_id,
        }
    }

    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    #[must_use]
    /// Returns the listed worktree ID to activate.
    pub fn worktree_id(&self) -> &str {
        &self.worktree_id
    }
}

/// Strict pathless startup-restoration request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestoreLastProjectRequestV1 {
    protocol_version: ProtocolVersion,
}

impl RestoreLastProjectRequestV1 {
    #[must_use]
    /// Creates a pathless restoration request.
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    #[must_use]
    /// Creates a restoration request for the current protocol.
    pub const fn current() -> Self {
        Self::new(ProtocolVersion::CURRENT)
    }

    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Result of activating a catalog entry or restoring the latest one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectActivationResultV1 {
    /// No catalog entry exists; no fallback was attempted.
    NoSavedProject,
    /// The exact stored worktree was revalidated and activated.
    Activated {
        /// Stable catalog project identity.
        #[serde(rename = "projectId")]
        project_id: String,
        /// Safe active-project projection.
        project: ProjectSummaryV1,
    },
}

/// Versioned activation/restoration response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectActivationResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectActivationResultV1,
}

impl ProjectActivationResponseV1 {
    #[must_use]
    /// Reports that no stored project exists and no fallback was attempted.
    pub const fn no_saved_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectActivationResultV1::NoSavedProject,
        }
    }

    #[must_use]
    /// Reports the exact project successfully activated by the Core.
    pub const fn activated(project_id: String, project: ProjectSummaryV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectActivationResultV1::Activated {
                project_id,
                project,
            },
        }
    }
}

/// Strict request for non-destructively removing one catalog entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RemoveCatalogProjectRequestV1 {
    protocol_version: ProtocolVersion,
    worktree_id: String,
}

impl RemoveCatalogProjectRequestV1 {
    #[must_use]
    /// Creates a pathless removal request for one listed worktree ID.
    pub const fn new(protocol_version: ProtocolVersion, worktree_id: String) -> Self {
        Self {
            protocol_version,
            worktree_id,
        }
    }

    #[must_use]
    /// Returns the requested protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    #[must_use]
    /// Returns the listed worktree ID to remove.
    pub fn worktree_id(&self) -> &str {
        &self.worktree_id
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivateCatalogProjectRequestV1, ProjectCatalogDirectionV1, QueryProjectCatalogRequestV1,
        RemoveCatalogProjectRequestV1, RestoreLastProjectRequestV1,
    };
    use crate::ProtocolVersion;
    use serde_json::json;

    #[test]
    fn catalog_request_has_no_path_field() -> Result<(), serde_json::Error> {
        let request = QueryProjectCatalogRequestV1::new(
            ProtocolVersion::CURRENT,
            Some("client".to_owned()),
            None,
            ProjectCatalogDirectionV1::Initial,
        );
        assert_eq!(
            serde_json::to_value(request)?,
            json!({
                "protocolVersion": 1,
                "search": "client",
                "cursor": null,
                "direction": "initial"
            })
        );
        Ok(())
    }

    #[test]
    fn catalog_commands_reject_path_authority_and_unknown_fields() {
        for value in [
            json!({
                "protocolVersion": 1,
                "worktreeId": "22".repeat(32),
                "path": "C:\\secret"
            }),
            json!({
                "protocolVersion": 1,
                "worktreeId": "22".repeat(32),
                "worktreeRoot": "C:\\secret"
            }),
        ] {
            assert!(
                serde_json::from_value::<ActivateCatalogProjectRequestV1>(value.clone()).is_err()
            );
            assert!(serde_json::from_value::<RemoveCatalogProjectRequestV1>(value).is_err());
        }
        assert!(
            serde_json::from_value::<RestoreLastProjectRequestV1>(json!({
                "protocolVersion": 1,
                "worktreeId": "22".repeat(32)
            }))
            .is_err()
        );
    }
}
