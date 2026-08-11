use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict input payload for removing the Core-owned active worktree from the project list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RemoveProjectRequestV1 {
    protocol_version: ProtocolVersion,
}

impl RemoveProjectRequestV1 {
    /// Creates a request for a specific protocol version.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Creates a request for the current build's protocol version.
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

/// Successful bounded response for project-list removal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RemoveProjectResponseV1 {
    protocol_version: ProtocolVersion,
    result: RemoveProjectResultV1,
}

impl RemoveProjectResponseV1 {
    /// Creates the only successful result; private storage retention cannot be disabled.
    #[must_use]
    pub const fn removed() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: RemoveProjectResultV1::Removed {
                retained_private_storage: true,
            },
        }
    }

    /// Returns the response protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the fixed successful removal projection.
    #[must_use]
    pub const fn result(self) -> RemoveProjectResultV1 {
        self.result
    }
}

/// V1 result whose retention flag makes the non-destructive semantics explicit to the UI.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum RemoveProjectResultV1 {
    /// The recent-list entry was removed while private A^3 storage was retained.
    Removed {
        /// Always true for this versioned operation.
        #[serde(rename = "retainedPrivateStorage")]
        retained_private_storage: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::RemoveProjectResponseV1;
    use serde_json::json;

    #[test]
    fn removed_response_has_a_stable_non_destructive_shape() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(RemoveProjectResponseV1::removed())?,
            json!({
                "protocolVersion": 1,
                "result": {
                    "retainedPrivateStorage": true,
                    "status": "removed"
                }
            })
        );
        Ok(())
    }
}
