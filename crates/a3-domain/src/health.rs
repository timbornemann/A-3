use crate::ApplicationVersion;

/// Immutable health observation for the running A^3 application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Health {
    application_version: ApplicationVersion,
}

impl Health {
    /// Creates a ready health observation for an application version.
    #[must_use]
    pub const fn ready(application_version: ApplicationVersion) -> Self {
        Self {
            application_version,
        }
    }

    /// Returns the application version that produced this observation.
    #[must_use]
    pub const fn application_version(&self) -> &ApplicationVersion {
        &self.application_version
    }
}

#[cfg(test)]
mod tests {
    use super::Health;
    use crate::{ApplicationVersion, ApplicationVersionError};

    #[test]
    fn health_retains_validated_application_version() -> Result<(), ApplicationVersionError> {
        let health = Health::ready(ApplicationVersion::try_from("0.1.0")?);

        assert_eq!(health.application_version().as_str(), "0.1.0");
        Ok(())
    }
}
