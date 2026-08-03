use a3_domain::{ApplicationVersion, Health, Platform};

/// Inbound application port for querying the health of A^3.
pub trait HealthQuery {
    /// Returns the current immutable health observation.
    fn execute(&self) -> Health;
}

/// Use case that reports process health from validated application state.
#[derive(Clone, Debug)]
pub struct GetHealth {
    application_version: ApplicationVersion,
    platform: Platform,
}

impl GetHealth {
    /// Creates the health-query use case.
    #[must_use]
    pub const fn new(application_version: ApplicationVersion, platform: Platform) -> Self {
        Self {
            application_version,
            platform,
        }
    }
}

impl HealthQuery for GetHealth {
    fn execute(&self) -> Health {
        Health::ready(self.application_version.clone(), self.platform)
    }
}

#[cfg(test)]
mod tests {
    use super::{GetHealth, HealthQuery};
    use a3_domain::{ApplicationVersion, ApplicationVersionError, Platform};

    #[test]
    fn health_use_case_is_reachable_through_inbound_port() -> Result<(), ApplicationVersionError> {
        let query = GetHealth::new(ApplicationVersion::try_from("0.1.0")?, Platform::Linux);

        let health = HealthQuery::execute(&query);

        assert_eq!(health.application_version().as_str(), "0.1.0");
        assert_eq!(health.platform(), Platform::Linux);
        Ok(())
    }
}
