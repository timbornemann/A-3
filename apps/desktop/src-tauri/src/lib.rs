//! Desktop composition root and explicit boundary mappings for A^3.

use a3_application::{GetHealth, HealthQuery};
use a3_domain::{ApplicationVersion, ApplicationVersionError, Health};
use a3_protocol::HealthResponseV1;

/// Owns the concrete application use cases used by the desktop process.
#[derive(Clone, Debug)]
pub struct CompositionRoot {
    health_query: GetHealth,
}

impl CompositionRoot {
    /// Wires the desktop application around validated process metadata.
    #[must_use]
    pub const fn new(application_version: ApplicationVersion) -> Self {
        Self {
            health_query: GetHealth::new(application_version),
        }
    }

    /// Wires the desktop application using this package's build version.
    pub fn from_package_version() -> Result<Self, ApplicationVersionError> {
        ApplicationVersion::try_from(env!("CARGO_PKG_VERSION")).map(Self::new)
    }

    /// Executes the health use case and maps its domain result to IPC V1.
    #[must_use]
    pub fn query_health(&self) -> HealthResponseV1 {
        map_health_to_v1(self.health_query.execute())
    }
}

fn map_health_to_v1(health: Health) -> HealthResponseV1 {
    HealthResponseV1::ready(health.application_version().as_str().to_owned())
}
