//! Generates Tauri context, command permissions, and platform resources.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["query_health"])),
    )?;
    Ok(())
}
