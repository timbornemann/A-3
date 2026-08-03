use crate::{ApplicationVersion, Platform};

/// Immutable health observation for the running A^3 application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Health {
    application_version: ApplicationVersion,
    platform: Platform,
}

impl Health {
    /// Creates a ready health observation for an application version.
    #[must_use]
    pub const fn ready(application_version: ApplicationVersion, platform: Platform) -> Self {
        Self {
            application_version,
            platform,
        }
    }

    /// Returns the application version that produced this observation.
    #[must_use]
    pub const fn application_version(&self) -> &ApplicationVersion {
        &self.application_version
    }

    /// Returns the operating-system family observed by the desktop adapter.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }
}

#[cfg(test)]
mod tests {
    use super::Health;
    use crate::{ApplicationVersion, ApplicationVersionError, Platform};

    #[test]
    fn health_retains_validated_application_version() -> Result<(), ApplicationVersionError> {
        let health = Health::ready(ApplicationVersion::try_from("0.1.0")?, Platform::Windows);

        assert_eq!(health.application_version().as_str(), "0.1.0");
        assert_eq!(health.platform(), Platform::Windows);
        Ok(())
    }
}
