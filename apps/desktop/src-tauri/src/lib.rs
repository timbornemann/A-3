//! Desktop composition root and explicit boundary mappings for A^3.

/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod platform;

use a3_application::{GetHealth, HealthQuery};
use a3_domain::{ApplicationVersion, ApplicationVersionError, Health, Platform};
use a3_protocol::{HealthResponseV1, PlatformV1};
use platform::SystemPlatform;
use std::error::Error;
use std::fmt;

/// Owns the concrete application use cases used by the desktop process.
#[derive(Clone, Debug)]
pub struct CompositionRoot {
    health_query: GetHealth,
}

impl CompositionRoot {
    /// Wires the desktop application around validated process metadata.
    #[must_use]
    pub const fn new(application_version: ApplicationVersion, platform: Platform) -> Self {
        Self {
            health_query: GetHealth::new(application_version, platform),
        }
    }

    /// Wires the desktop application using package and platform adapters.
    pub fn from_environment() -> Result<Self, ApplicationVersionError> {
        ApplicationVersion::try_from(env!("CARGO_PKG_VERSION"))
            .map(|version| Self::new(version, SystemPlatform::current()))
    }

    /// Executes the health use case and maps its domain result to IPC V1.
    #[must_use]
    pub fn query_health(&self) -> HealthResponseV1 {
        map_health_to_v1(self.health_query.execute())
    }
}

/// Starts the Tauri desktop process with its narrow command surface.
pub fn run() -> Result<(), DesktopRunError> {
    let root = CompositionRoot::from_environment().map_err(DesktopRunError::InvalidVersion)?;

    tauri::Builder::default()
        .manage(root)
        .invoke_handler(tauri::generate_handler![commands::query_health])
        .run(tauri::generate_context!())
        .map_err(DesktopRunError::Tauri)
}

fn map_health_to_v1(health: Health) -> HealthResponseV1 {
    HealthResponseV1::ready(
        health.application_version().as_str().to_owned(),
        map_platform_to_v1(health.platform()),
    )
}

const fn map_platform_to_v1(platform: Platform) -> PlatformV1 {
    match platform {
        Platform::Windows => PlatformV1::Windows,
        Platform::Linux => PlatformV1::Linux,
        Platform::MacOs => PlatformV1::MacOs,
        Platform::Unsupported => PlatformV1::Unsupported,
    }
}

/// Failure while constructing or running the desktop process.
#[derive(Debug)]
pub enum DesktopRunError {
    /// Build metadata contained an invalid application version.
    InvalidVersion(ApplicationVersionError),
    /// Tauri failed to construct or run the desktop application.
    Tauri(tauri::Error),
}

impl fmt::Display for DesktopRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersion(error) => {
                write!(formatter, "invalid application version: {error}")
            }
            Self::Tauri(error) => write!(formatter, "desktop runtime failed: {error}"),
        }
    }
}

impl Error for DesktopRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidVersion(error) => Some(error),
            Self::Tauri(error) => Some(error),
        }
    }
}
