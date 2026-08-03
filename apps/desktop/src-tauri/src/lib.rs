//! Desktop composition root and explicit boundary mappings for A^3.

mod clock;
/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod platform;

use a3_application::{
    GetHealth, HealthQuery, JobEventStream, JobScheduler, JobSchedulerConfig,
    JobSchedulerConfigError, JobSchedulerCreateError,
};
use a3_domain::{ApplicationVersion, ApplicationVersionError, Health, Platform};
use a3_protocol::{HealthResponseV1, PlatformV1};
use clock::SystemJobClock;
use platform::SystemPlatform;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Owns the concrete application use cases used by the desktop process.
#[derive(Debug)]
pub struct CompositionRoot {
    health_query: GetHealth,
    _job_scheduler: JobScheduler,
    _job_events: JobEventStream,
}

impl CompositionRoot {
    /// Wires process metadata and a bounded, owned background-job runtime.
    pub fn new(
        application_version: ApplicationVersion,
        platform: Platform,
    ) -> Result<Self, CompositionRootError> {
        let config = JobSchedulerConfig::new(2, 32, 256)
            .map_err(CompositionRootError::InvalidJobSchedulerConfig)?;
        let (job_scheduler, job_events) =
            JobScheduler::new(config, Arc::new(SystemJobClock::new()))
                .map_err(CompositionRootError::JobScheduler)?;

        Ok(Self {
            health_query: GetHealth::new(application_version, platform),
            _job_scheduler: job_scheduler,
            _job_events: job_events,
        })
    }

    /// Wires the desktop application using package and platform adapters.
    pub fn from_environment() -> Result<Self, CompositionRootError> {
        let version = ApplicationVersion::try_from(env!("CARGO_PKG_VERSION"))
            .map_err(CompositionRootError::InvalidVersion)?;
        Self::new(version, SystemPlatform::current())
    }

    /// Executes the health use case and maps its domain result to IPC V1.
    #[must_use]
    pub fn query_health(&self) -> HealthResponseV1 {
        map_health_to_v1(self.health_query.execute())
    }
}

/// Starts the Tauri desktop process with its narrow command surface.
pub fn run() -> Result<(), DesktopRunError> {
    let root = CompositionRoot::from_environment().map_err(DesktopRunError::Composition)?;

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

/// Failure while constructing the desktop composition root.
#[derive(Debug)]
pub enum CompositionRootError {
    /// Build metadata contained an invalid application version.
    InvalidVersion(ApplicationVersionError),
    /// The compile-time desktop scheduler limits were invalid.
    InvalidJobSchedulerConfig(JobSchedulerConfigError),
    /// The operating system rejected an owned scheduler worker.
    JobScheduler(JobSchedulerCreateError),
}

impl fmt::Display for CompositionRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersion(error) => {
                write!(formatter, "invalid application version: {error}")
            }
            Self::InvalidJobSchedulerConfig(error) => {
                write!(formatter, "invalid job scheduler configuration: {error}")
            }
            Self::JobScheduler(error) => write!(formatter, "job scheduler failed: {error}"),
        }
    }
}

impl Error for CompositionRootError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidVersion(error) => Some(error),
            Self::InvalidJobSchedulerConfig(error) => Some(error),
            Self::JobScheduler(error) => Some(error),
        }
    }
}

/// Failure while constructing or running the desktop process.
#[derive(Debug)]
pub enum DesktopRunError {
    /// The process composition root could not be constructed.
    Composition(CompositionRootError),
    /// Tauri failed to construct or run the desktop application.
    Tauri(tauri::Error),
}

impl fmt::Display for DesktopRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(error) => write!(formatter, "composition failed: {error}"),
            Self::Tauri(error) => write!(formatter, "desktop runtime failed: {error}"),
        }
    }
}

impl Error for DesktopRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Composition(error) => Some(error),
            Self::Tauri(error) => Some(error),
        }
    }
}
