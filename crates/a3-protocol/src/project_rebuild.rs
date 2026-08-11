use crate::{ProtocolVersion, RebuildStateV1};
use serde::{Deserialize, Serialize};

/// Strict input payload for rebuilding the Core-owned active project index.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RebuildProjectIndexRequestV1 {
    protocol_version: ProtocolVersion,
}

impl RebuildProjectIndexRequestV1 {
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

/// Acknowledgement that the owned index coordinator accepted a rebuild request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RebuildProjectIndexResponseV1 {
    protocol_version: ProtocolVersion,
    state: RebuildStateV1,
}

impl RebuildProjectIndexResponseV1 {
    /// Creates the only successful immediate response: a queued owned job.
    #[must_use]
    pub const fn queued() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            state: RebuildStateV1::Queued,
        }
    }

    /// Returns the response protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the accepted lifecycle state.
    #[must_use]
    pub const fn state(self) -> RebuildStateV1 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::RebuildProjectIndexResponseV1;
    use serde_json::json;

    #[test]
    fn queued_rebuild_response_has_a_stable_shape() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(RebuildProjectIndexResponseV1::queued())?,
            json!({ "protocolVersion": 1, "state": "queued" })
        );
        Ok(())
    }
}
