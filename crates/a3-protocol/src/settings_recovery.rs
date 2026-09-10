use crate::{ModelRoleV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Content-free diagnosis; the native boundary chooses the affected roles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRecoveryResponseV1 {
    protocol_version: ProtocolVersion,
    settings_revision: String,
    invalid_profiles: Vec<ModelRoleV1>,
}

impl SettingsRecoveryResponseV1 {
    /// Creates a bounded, canonical diagnosis or a completed recovery receipt.
    #[must_use]
    pub fn new(settings_revision: String, roles: &[ModelRoleV1]) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            settings_revision,
            invalid_profiles: [
                ModelRoleV1::Coding,
                ModelRoleV1::Mapping,
                ModelRoleV1::Embedding,
            ]
            .into_iter()
            .filter(|role| roles.contains(role))
            .collect(),
        }
    }
}

/// Explicit recovery bound to the revision shown by the preceding diagnosis.
/// Callers cannot select data to discard or supply replacement capabilities.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecoverModelProfilesRequestV1 {
    protocol_version: ProtocolVersion,
    expected_settings_revision: String,
}

impl RecoverModelProfilesRequestV1 {
    /// Returns the schema version checked before storage access.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the canonical CAS revision to validate at the native boundary.
    #[must_use]
    pub fn expected_settings_revision(&self) -> &str {
        &self.expected_settings_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_request_cannot_select_roles_paths_or_capabilities() {
        for field in ["roles", "path", "sql", "verified", "providerKind"] {
            let mut value =
                serde_json::json!({"protocolVersion": 1, "expectedSettingsRevision": "100"});
            value[field] = serde_json::json!("untrusted");
            assert!(serde_json::from_value::<RecoverModelProfilesRequestV1>(value).is_err());
        }
    }

    #[test]
    fn diagnosis_is_bounded_and_content_free() -> Result<(), serde_json::Error> {
        let response = SettingsRecoveryResponseV1::new(
            "100".into(),
            &[
                ModelRoleV1::Mapping,
                ModelRoleV1::Coding,
                ModelRoleV1::Coding,
            ],
        );
        assert_eq!(
            serde_json::to_value(response)?,
            serde_json::json!({
                "protocolVersion": 1, "settingsRevision": "100", "invalidProfiles": ["coding", "mapping"]
            })
        );
        Ok(())
    }
}
