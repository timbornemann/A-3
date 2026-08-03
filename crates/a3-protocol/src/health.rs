use crate::ProtocolVersion;

/// Health states supported by the V1 IPC schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthStatusV1 {
    /// The application core is ready to accept queries.
    Ready,
}

/// Versioned health response exposed at the untrusted UI boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthResponseV1 {
    protocol_version: ProtocolVersion,
    application_version: String,
    status: HealthStatusV1,
}

impl HealthResponseV1 {
    /// Creates a ready V1 health response from already validated core data.
    #[must_use]
    pub fn ready(application_version: String) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            application_version,
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

    /// Returns the V1 health status.
    #[must_use]
    pub const fn status(&self) -> HealthStatusV1 {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthResponseV1, HealthStatusV1};
    use crate::ProtocolVersion;

    #[test]
    fn ready_response_is_explicitly_versioned() {
        let response = HealthResponseV1::ready("0.1.0".to_owned());

        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        assert_eq!(response.application_version(), "0.1.0");
        assert_eq!(response.status(), HealthStatusV1::Ready);
    }
}
