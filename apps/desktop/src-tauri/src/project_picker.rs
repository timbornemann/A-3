use a3_application::{ProjectDirectoryPicker, ProjectDirectorySelectionError};
use std::fmt;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const OPEN_PROJECT_DIALOG_TITLE: &str = "A^3 project worktree";

/// Native single-directory picker owned by the privileged desktop adapter.
#[derive(Clone)]
pub(crate) struct NativeProjectDirectoryPicker {
    app: AppHandle,
    title: &'static str,
}

impl NativeProjectDirectoryPicker {
    /// Binds native dialog access to the running desktop application.
    pub(crate) const fn new(app: AppHandle) -> Self {
        Self::with_title(app, OPEN_PROJECT_DIALOG_TITLE)
    }

    /// Binds native dialog access with an explicit folder-dialog title.
    pub(crate) const fn with_title(app: AppHandle, title: &'static str) -> Self {
        Self { app, title }
    }
}

impl fmt::Debug for NativeProjectDirectoryPicker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProjectDirectoryPicker")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl ProjectDirectoryPicker for NativeProjectDirectoryPicker {
    fn pick_project_directory(&self) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
        self.app
            .dialog()
            .file()
            .set_title(self.title)
            .blocking_pick_folder()
            .map(tauri_plugin_dialog::FilePath::into_path)
            .transpose()
            .map_err(|_| ProjectDirectorySelectionError::InvalidNativeSelection)
    }
}
