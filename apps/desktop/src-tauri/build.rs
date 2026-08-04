//! Generates Tauri context, command permissions, and platform resources.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["open_project", "query_health"]),
    ))?;
    Ok(())
}
