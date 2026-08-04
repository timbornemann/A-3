use a3_application::{ProjectDirectoryPicker, ProjectDirectorySelectionError};
use std::fmt;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// Native single-directory picker owned by the privileged desktop adapter.
#[derive(Clone)]
pub(crate) struct NativeProjectDirectoryPicker {
    app: AppHandle,
}

impl NativeProjectDirectoryPicker {
    /// Binds native dialog access to the running desktop application.
    pub(crate) const fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl fmt::Debug for NativeProjectDirectoryPicker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProjectDirectoryPicker")
            .finish_non_exhaustive()
    }
}

impl ProjectDirectoryPicker for NativeProjectDirectoryPicker {
    fn pick_project_directory(&self) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
        self.app
            .dialog()
            .file()
            .set_title("A^3 project worktree")
            .blocking_pick_folder()
            .map(tauri_plugin_dialog::FilePath::into_path)
            .transpose()
            .map_err(|_| ProjectDirectorySelectionError::InvalidNativeSelection)
    }
}
