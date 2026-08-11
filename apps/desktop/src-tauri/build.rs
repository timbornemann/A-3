//! Generates Tauri context, command permissions, and platform resources.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_recent_projects",
            "open_project",
            "query_index_activity",
            "query_index_overview",
            "query_project_status",
            "query_health",
            "rebuild_project_index",
            "remove_project",
        ]),
    ))?;
    Ok(())
}
