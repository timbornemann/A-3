use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict input payload for the lightweight V1 Fast-Index activity query.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryIndexActivityRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryIndexActivityRequestV1 {
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

/// Lightweight response that never reads repository files or reconstructs an index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexActivityResponseV1 {
    protocol_version: ProtocolVersion,
    result: IndexActivityResultV1,
}

impl IndexActivityResponseV1 {
    /// Creates the response used before any project has been selected.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexActivityResultV1::NoProject,
        }
    }

    /// Creates a response from the Core-owned activity snapshot.
    #[must_use]
    pub const fn active(activity: IndexActivityV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexActivityResultV1::Active { activity },
        }
    }

    /// Returns the mutually exclusive project/activity result.
    #[must_use]
    pub const fn result(&self) -> &IndexActivityResultV1 {
        &self.result
    }
}

/// Whether a Core-owned project exists for the activity query.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum IndexActivityResultV1 {
    /// No project is active in this desktop process.
    NoProject,
    /// The bounded scheduler projection for the active project.
    Active {
        /// Current Fast-Index lifecycle and phase.
        activity: IndexActivityV1,
    },
}

/// Bounded projection of one owned Fast-Index job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexActivityV1 {
    state: IndexActivityStateV1,
    phase: Option<IndexPhaseV1>,
    completed_phases: u64,
    total_phases: u64,
}

impl IndexActivityV1 {
    /// Creates a projection from manager-validated lifecycle values.
    #[must_use]
    pub const fn new(
        state: IndexActivityStateV1,
        phase: Option<IndexPhaseV1>,
        completed_phases: u64,
        total_phases: u64,
    ) -> Self {
        Self {
            state,
            phase,
            completed_phases,
            total_phases,
        }
    }

    /// Returns the scheduler-owned lifecycle state.
    #[must_use]
    pub const fn state(self) -> IndexActivityStateV1 {
        self.state
    }

    /// Returns the deterministic phase, if a run has started.
    #[must_use]
    pub const fn phase(self) -> Option<IndexPhaseV1> {
        self.phase
    }

    /// Returns completed phase boundaries.
    #[must_use]
    pub const fn completed_phases(self) -> u64 {
        self.completed_phases
    }

    /// Returns the fixed V1 phase count.
    #[must_use]
    pub const fn total_phases(self) -> u64 {
        self.total_phases
    }
}

/// Lifecycle of the current or most recently completed Fast-Index job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexActivityStateV1 {
    /// No Fast-Index job has been submitted for the active project.
    Idle,
    /// The bounded scheduler accepted the job.
    Queued,
    /// An owned worker is executing the job.
    Running,
    /// Cooperative cancellation was requested.
    Cancelling,
    /// The job completed and its publication is visible.
    Succeeded,
    /// The job failed without replacing the previous publication.
    Failed,
    /// The job stopped cooperatively without replacing the previous publication.
    Cancelled,
}

/// Fixed ADR-0006 Fast-Index phase names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexPhaseV1 {
    /// Discover repository candidates.
    Discover,
    /// Hash exact contents.
    Hash,
    /// Parse supported files.
    Parse,
    /// Link graph relationships.
    Link,
    /// Rank symbols and form modules.
    Rank,
    /// Atomically publish the complete index.
    Publish,
}

#[cfg(test)]
mod tests {
    use super::{IndexActivityResponseV1, IndexActivityStateV1, IndexActivityV1, IndexPhaseV1};
    use serde_json::json;

    #[test]
    fn active_activity_has_a_strict_bounded_shape() -> Result<(), serde_json::Error> {
        let response = IndexActivityResponseV1::active(IndexActivityV1::new(
            IndexActivityStateV1::Running,
            Some(IndexPhaseV1::Link),
            3,
            6,
        ));

        assert_eq!(
            serde_json::to_value(response)?,
            json!({
                "protocolVersion": 1,
                "result": {
                    "status": "active",
                    "activity": {
                        "state": "running",
                        "phase": "link",
                        "completedPhases": 3,
                        "totalPhases": 6
                    }
                }
            })
        );
        Ok(())
    }
}
