//! Desktop composition root and explicit boundary mappings for A^3.

mod clock;
/// Narrow, typed commands exposed to the untrusted desktop WebView.
pub mod commands;
mod platform;
mod project_picker;

use a3_application::{
    GetHealth, HealthQuery, JobEventStream, JobScheduler, JobSchedulerConfig,
    JobSchedulerConfigError, JobSchedulerCreateError, OpenProject, OpenProjectError,
    OpenProjectOutcome, ProjectDirectoryPicker, ProjectInspectionFailure,
};
use a3_domain::{ApplicationVersion, ApplicationVersionError, GitHead, Health, Platform};
use a3_protocol::{
    CommandErrorV1, ErrorCodeV1, GitHeadV1, HealthResponseV1, OpenProjectResponseV1, PlatformV1,
    ProjectSummaryV1,
};
use a3_workspace::RepositoryInspector;
use clock::SystemJobClock;
use platform::SystemPlatform;
use project_picker::NativeProjectDirectoryPicker;
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;

const MAX_PROJECT_PATH_DISPLAY_CHARS: usize = 32_768;

/// Owns the concrete application use cases used by the desktop process.
#[derive(Debug)]
pub struct CompositionRoot {
    health_query: GetHealth,
    open_project: OpenProject,
    _job_scheduler: JobScheduler,
    _job_events: JobEventStream,
}

impl CompositionRoot {
    /// Wires process metadata, project selection, and a bounded owned background-job runtime.
    pub fn new(
        application_version: ApplicationVersion,
        platform: Platform,
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
    ) -> Result<Self, CompositionRootError> {
        CompositionBase::new(application_version, platform)
            .map(|base| base.finish(project_directory_picker))
    }

    /// Wires the desktop application using package, platform, and native-picker adapters.
    pub fn from_environment(
        project_directory_picker: Arc<dyn ProjectDirectoryPicker>,
    ) -> Result<Self, CompositionRootError> {
        CompositionBase::from_environment().map(|base| base.finish(project_directory_picker))
    }

    /// Executes the health use case and maps its domain result to IPC V1.
    #[must_use]
    pub fn query_health(&self) -> HealthResponseV1 {
        map_health_to_v1(self.health_query.execute())
    }

    /// Executes one user-controlled native project selection and maps it to IPC V1.
    pub fn open_project(&self) -> Result<OpenProjectResponseV1, CommandErrorV1> {
        self.open_project
            .execute()
            .map(map_open_project_to_v1)
            .map_err(map_open_project_error_to_v1)
    }
}

#[derive(Debug)]
struct CompositionBase {
    health_query: GetHealth,
    job_scheduler: JobScheduler,
    job_events: JobEventStream,
}

impl CompositionBase {
    fn new(
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
            job_scheduler,
            job_events,
        })
    }

    fn from_environment() -> Result<Self, CompositionRootError> {
        let version = ApplicationVersion::try_from(env!("CARGO_PKG_VERSION"))
            .map_err(CompositionRootError::InvalidVersion)?;
        Self::new(version, SystemPlatform::current())
    }

    fn finish(self, project_directory_picker: Arc<dyn ProjectDirectoryPicker>) -> CompositionRoot {
        CompositionRoot {
            health_query: self.health_query,
            open_project: OpenProject::new(
                project_directory_picker,
                Arc::new(RepositoryInspector::new()),
            ),
            _job_scheduler: self.job_scheduler,
            _job_events: self.job_events,
        }
    }
}

/// Starts the Tauri desktop process with its narrow command surface.
pub fn run() -> Result<(), DesktopRunError> {
    let base = CompositionBase::from_environment().map_err(DesktopRunError::Composition)?;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            app.manage(base.finish(Arc::new(NativeProjectDirectoryPicker::new(
                app.handle().clone(),
            ))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_project,
            commands::query_health
        ])
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

fn map_open_project_to_v1(outcome: OpenProjectOutcome) -> OpenProjectResponseV1 {
    match outcome {
        OpenProjectOutcome::Cancelled => OpenProjectResponseV1::cancelled(),
        OpenProjectOutcome::Opened(project) => {
            let head = match project.head() {
                GitHead::Born {
                    object_id,
                    reference,
                } => GitHeadV1::Born {
                    object_id: object_id.as_str().to_owned(),
                    reference: reference
                        .as_ref()
                        .map(|reference| reference.as_str().to_owned()),
                },
                GitHead::Unborn { reference } => GitHeadV1::Unborn {
                    reference: reference.as_str().to_owned(),
                },
            };
            OpenProjectResponseV1::opened(ProjectSummaryV1::new(
                project.repository().id().to_string(),
                project.worktree().id().to_string(),
                project_path_display(project.worktree().root().as_path()),
                head,
            ))
        }
    }
}

fn project_path_display(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .take(MAX_PROJECT_PATH_DISPLAY_CHARS)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn map_open_project_error_to_v1(error: OpenProjectError) -> CommandErrorV1 {
    let code = match error {
        OpenProjectError::DirectorySelection(_) => ErrorCodeV1::ProjectSelectionFailed,
        OpenProjectError::Inspection(ProjectInspectionFailure::SelectionUnavailable) => {
            ErrorCodeV1::ProjectSelectionUnavailable
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::NotRepository) => {
            ErrorCodeV1::NotGitRepository
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::NotWorktreeRoot) => {
            ErrorCodeV1::ProjectRootRequired
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::UnsupportedRepository) => {
            ErrorCodeV1::UnsupportedRepository
        }
        OpenProjectError::Inspection(ProjectInspectionFailure::InvalidRepositoryMetadata) => {
            ErrorCodeV1::InvalidRepositoryMetadata
        }
    };
    CommandErrorV1::project_open(code)
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

#[cfg(test)]
mod tests {
    use super::{MAX_PROJECT_PATH_DISPLAY_CHARS, project_path_display};
    use std::path::Path;

    #[test]
    fn project_path_display_is_bounded_and_contains_no_control_characters() {
        let path = format!("C:\\\n{}", "a".repeat(MAX_PROJECT_PATH_DISPLAY_CHARS + 8));

        let display = project_path_display(Path::new(&path));

        assert_eq!(display.chars().count(), MAX_PROJECT_PATH_DISPLAY_CHARS);
        assert!(!display.chars().any(char::is_control));
        assert!(display.contains('\u{fffd}'));
    }
}
