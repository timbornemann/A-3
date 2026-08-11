use crate::{ProjectSummaryV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict input payload for the V1 active-project status query.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectStatusRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryProjectStatusRequestV1 {
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

/// Versioned response describing the Core-owned active project, if present.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectStatusResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectStatusResultV1,
}

impl ProjectStatusResponseV1 {
    /// Creates the response used before any project has been selected.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectStatusResultV1::NoProject,
        }
    }

    /// Creates a bounded status response for the validated Core-owned project.
    #[must_use]
    pub fn active(
        project_id: String,
        project: ProjectSummaryV1,
        index: ProjectIndexStatusV1,
        storage_bytes: Option<String>,
        rebuild_state: RebuildStateV1,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectStatusResultV1::Active {
                project_id,
                project: Box::new(project),
                index: Box::new(index),
                storage_bytes,
                rebuild_state,
            },
        }
    }

    /// Returns the response protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the mutually exclusive active-project result.
    #[must_use]
    pub const fn result(&self) -> &ProjectStatusResultV1 {
        &self.result
    }
}

/// Active-project result selected entirely by privileged Core state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectStatusResultV1 {
    /// No project has been opened in this desktop process.
    NoProject,
    /// A validated project is active and its bounded index metadata was loaded.
    Active {
        /// Stable catalog identity assigned by local storage.
        #[serde(rename = "projectId")]
        project_id: String,
        /// Existing WebView-safe repository and worktree projection.
        project: Box<ProjectSummaryV1>,
        /// Durable snapshot and index-run projection.
        index: Box<ProjectIndexStatusV1>,
        /// Exact private A^3 storage usage as lossless decimal text.
        #[serde(rename = "storageBytes")]
        storage_bytes: Option<String>,
        /// Lifecycle of the latest user-requested regenerable-index rebuild.
        #[serde(rename = "rebuildState")]
        rebuild_state: RebuildStateV1,
    },
}

/// WebView-safe durable index status for one worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectIndexStatusV1 {
    state: IndexStateV1,
    latest_snapshot: Option<ProjectSnapshotV1>,
    latest_attempt_snapshot_id: Option<String>,
    published_snapshot_id: Option<String>,
}

impl ProjectIndexStatusV1 {
    /// Creates a status from already validated application projections.
    #[must_use]
    pub const fn new(
        state: IndexStateV1,
        latest_snapshot: Option<ProjectSnapshotV1>,
        latest_attempt_snapshot_id: Option<String>,
        published_snapshot_id: Option<String>,
    ) -> Self {
        Self {
            state,
            latest_snapshot,
            latest_attempt_snapshot_id,
            published_snapshot_id,
        }
    }

    /// Returns the latest durable attempt state or `notStarted`.
    #[must_use]
    pub const fn state(&self) -> IndexStateV1 {
        self.state
    }

    /// Returns the latest observed immutable snapshot.
    #[must_use]
    pub const fn latest_snapshot(&self) -> Option<&ProjectSnapshotV1> {
        self.latest_snapshot.as_ref()
    }

    /// Returns the input snapshot of the latest index attempt.
    #[must_use]
    pub fn latest_attempt_snapshot_id(&self) -> Option<&str> {
        self.latest_attempt_snapshot_id.as_deref()
    }

    /// Returns the snapshot still visible through the publish boundary.
    #[must_use]
    pub fn published_snapshot_id(&self) -> Option<&str> {
        self.published_snapshot_id.as_deref()
    }
}

/// Durable lifecycle projection of the latest index attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexStateV1 {
    /// No index attempt has been recorded.
    NotStarted,
    /// The latest attempt currently owns the index mutation slot.
    Building,
    /// The latest attempt was atomically published.
    Published,
    /// The latest attempt failed without replacing a previous publication.
    Failed,
    /// The latest attempt was cancelled without replacing a previous publication.
    Cancelled,
}

/// Core-owned lifecycle of a user-requested regenerable-index rebuild.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RebuildStateV1 {
    /// No rebuild has been requested for the active project.
    Idle,
    /// The coordinator accepted the request and is quiescing prior index work.
    Queued,
    /// The bounded rebuild job is deleting only regenerable projections.
    Running,
    /// Regenerable projections were removed and an authoritative refresh was requested.
    Succeeded,
    /// The rebuild failed without deleting authoritative task or snapshot history.
    Failed,
    /// The owning scheduler cancelled the rebuild before commit.
    Cancelled,
}

/// Bounded identity and generation of the latest durable snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectSnapshotV1 {
    snapshot_id: String,
    generation: String,
}

impl ProjectSnapshotV1 {
    /// Creates a snapshot projection using a decimal string for lossless JavaScript transport.
    #[must_use]
    pub const fn new(snapshot_id: String, generation: String) -> Self {
        Self {
            snapshot_id,
            generation,
        }
    }

    /// Returns the lowercase snapshot digest.
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Returns the positive worktree generation as canonical decimal text.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IndexStateV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1,
        RebuildStateV1,
    };
    use crate::{GitHeadV1, ProjectSummaryV1};
    use serde_json::json;

    #[test]
    fn active_project_status_has_a_stable_bounded_shape() -> Result<(), serde_json::Error> {
        let response = ProjectStatusResponseV1::active(
            "33".repeat(32),
            ProjectSummaryV1::new(
                "11".repeat(32),
                "22".repeat(32),
                "/worktree".to_owned(),
                GitHeadV1::Unborn {
                    reference: "refs/heads/main".to_owned(),
                },
            ),
            ProjectIndexStatusV1::new(
                IndexStateV1::Published,
                Some(ProjectSnapshotV1::new("44".repeat(32), "7".to_owned())),
                Some("44".repeat(32)),
                Some("44".repeat(32)),
            ),
            Some("4096".to_owned()),
            RebuildStateV1::Idle,
        );

        assert_eq!(
            serde_json::to_value(response)?,
            json!({
                "protocolVersion": 1,
                "result": {
                    "status": "active",
                    "projectId": "33".repeat(32),
                    "project": {
                        "repositoryId": "11".repeat(32),
                        "worktreeId": "22".repeat(32),
                        "worktreeRootDisplay": "/worktree",
                        "head": { "kind": "unborn", "reference": "refs/heads/main" }
                    },
                    "index": {
                        "state": "published",
                        "latestSnapshot": {
                            "snapshotId": "44".repeat(32),
                            "generation": "7"
                        },
                        "latestAttemptSnapshotId": "44".repeat(32),
                        "publishedSnapshotId": "44".repeat(32)
                    },
                    "storageBytes": "4096",
                    "rebuildState": "idle"
                }
            })
        );
        Ok(())
    }
}
