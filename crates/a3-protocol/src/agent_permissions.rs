use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// App-wide user selection; never accepted from a model action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentPermissionModeV1 {
    /// Changes and processes require exact approval.
    AskPermissions,
    /// Classified actions use the user's app-wide authority.
    FullMachine,
}

/// Exact visible revision required before a mode change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAgentPermissionsRequestV1 {
    /// Supported IPC version.
    pub protocol_version: ProtocolVersion,
    /// Exact positive visible permission revision.
    pub expected_revision: String,
    /// Explicitly selected mode.
    pub mode: AgentPermissionModeV1,
}

/// Content-free current app permission state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPermissionsResponseV1 {
    /// Supported IPC version.
    pub protocol_version: ProtocolVersion,
    /// Current positive permission revision.
    pub revision: String,
    /// Explicitly selected mode.
    pub mode: AgentPermissionModeV1,
}

impl AgentPermissionsResponseV1 {
    /// Constructs the versioned current settings projection.
    #[must_use]
    pub fn new(revision: String, mode: AgentPermissionModeV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            revision,
            mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_contract_is_closed_and_content_free() -> Result<(), Box<dyn std::error::Error>> {
        let response =
            AgentPermissionsResponseV1::new("2".to_owned(), AgentPermissionModeV1::FullMachine);
        let value = serde_json::to_value(&response)?;
        assert_eq!(
            value,
            json!({"protocolVersion":1,"revision":"2","mode":"fullMachine"})
        );
        assert_eq!(
            serde_json::from_value::<AgentPermissionsResponseV1>(value)?,
            response
        );
        for value in [
            json!({"protocolVersion":1,"expectedRevision":"1","mode":"fullMachine","shell":"anything"}),
            json!({"protocolVersion":1,"expectedRevision":"1","mode":"automatic"}),
            json!({"protocolVersion":1,"mode":"fullMachine"}),
            json!({"protocolVersion":1,"expectedRevision":1,"mode":"fullMachine"}),
        ] {
            assert!(serde_json::from_value::<UpdateAgentPermissionsRequestV1>(value).is_err());
        }
        Ok(())
    }
}
