use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict input payload for the V1 health query.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HealthRequestV1 {
    protocol_version: ProtocolVersion,
}

impl HealthRequestV1 {
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

/// Health states supported by the V1 IPC schema.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatusV1 {
    /// The application core is ready to accept queries.
    Ready,
}

/// Operating-system families exposed by the V1 health response.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformV1 {
    /// Microsoft Windows.
    Windows,
    /// Linux distributions supported by Tauri.
    Linux,
    /// Apple macOS.
    MacOs,
    /// A target outside the supported V1 matrix.
    Unsupported,
}

/// Versioned health response exposed at the untrusted UI boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HealthResponseV1 {
    protocol_version: ProtocolVersion,
    application_version: String,
    platform: PlatformV1,
    status: HealthStatusV1,
}

impl HealthResponseV1 {
    /// Creates a ready V1 health response from already validated core data.
    #[must_use]
    pub fn ready(application_version: String, platform: PlatformV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            application_version,
            platform,
            status: HealthStatusV1::Ready,
        }
    }

    /// Returns the protocol version carried by this message.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the application version as a protocol primitive.
    #[must_use]
    pub fn application_version(&self) -> &str {
        &self.application_version
    }

    /// Returns the operating-system family reported by the core.
    #[must_use]
    pub const fn platform(&self) -> PlatformV1 {
        self.platform
    }

    /// Returns the V1 health status.
    #[must_use]
    pub const fn status(&self) -> HealthStatusV1 {
        self.status
    }
}

/// Stable error codes exposed by V1 IPC commands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCodeV1 {
    /// The request used a protocol version this build does not support.
    UnsupportedProtocolVersion,
}

/// Safe, versioned error returned across the IPC boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommandErrorV1 {
    protocol_version: ProtocolVersion,
    code: ErrorCodeV1,
    message: String,
}

impl CommandErrorV1 {
    /// Creates the stable response for an unsupported request version.
    #[must_use]
    pub fn unsupported_protocol_version() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            code: ErrorCodeV1::UnsupportedProtocolVersion,
            message: "The requested protocol version is not supported.".to_owned(),
        }
    }

    /// Returns the protocol version used to encode this error.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the stable, localizable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCodeV1 {
        self.code
    }

    /// Returns the safe error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandErrorV1, ErrorCodeV1, HealthRequestV1, HealthResponseV1, HealthStatusV1, PlatformV1,
    };
    use crate::ProtocolVersion;
    use serde_json::json;

    #[test]
    fn ready_response_has_stable_json_shape() -> Result<(), serde_json::Error> {
        let response = HealthResponseV1::ready("0.1.0".to_owned(), PlatformV1::Windows);

        assert_eq!(
            serde_json::to_value(&response)?,
            json!({
                "applicationVersion": "0.1.0",
                "platform": "windows",
                "protocolVersion": 1,
                "status": "ready"
            })
        );
        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        assert_eq!(response.application_version(), "0.1.0");
        assert_eq!(response.platform(), PlatformV1::Windows);
        assert_eq!(response.status(), HealthStatusV1::Ready);
        Ok(())
    }

    #[test]
    fn health_request_rejects_unknown_fields() {
        let result = serde_json::from_value::<HealthRequestV1>(json!({
            "protocolVersion": 1,
            "unexpected": true
        }));

        assert!(result.is_err());
    }

    #[test]
    fn unsupported_version_error_has_stable_safe_shape() -> Result<(), serde_json::Error> {
        let error = CommandErrorV1::unsupported_protocol_version();

        assert_eq!(error.protocol_version(), ProtocolVersion::V1);
        assert_eq!(error.code(), ErrorCodeV1::UnsupportedProtocolVersion);
        assert_eq!(
            serde_json::to_value(&error)?,
            json!({
                "code": "unsupportedProtocolVersion",
                "message": "The requested protocol version is not supported.",
                "protocolVersion": 1
            })
        );
        Ok(())
    }
}
