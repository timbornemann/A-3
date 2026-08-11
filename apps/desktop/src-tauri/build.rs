//! Generates Tauri context, command permissions, and platform resources.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_recent_projects",
            "cancel_deep_map",
            "open_project",
            "pause_deep_map",
            "query_deep_map",
            "query_index_activity",
            "query_index_overview",
            "query_project_status",
            "query_health",
            "rebuild_project_index",
            "resume_deep_map",
            "remove_project",
            "start_deep_map",
        ]),
    ))?;
    Ok(())
}
