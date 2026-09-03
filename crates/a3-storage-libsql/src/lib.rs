//! Local-only libSQL storage adapters for A^3 catalog and project data.

mod agent_ask_research_repository;
mod agent_recovery_repository;
mod agent_session_repository;
mod catalog;
mod command_allowlist_repository;
mod deep_map_journal_repository;
mod deep_map_repository;
mod exact_search_projection;
mod exact_search_repository;
mod goal_contract_repository;
mod graph_traversal_repository;
mod index_codec;
mod index_invalidation_repository;
mod index_publication;
mod index_repository;
mod knowledge;
mod layout;
mod lexical_search_projection;
mod lexical_search_repository;
mod local_store;
mod migration;
mod module_card_detail_repository;
mod module_card_evidence_repository;
mod module_card_freshness_repository;
mod module_card_repository;
mod module_dependency_graph_repository;
mod module_projection_codec;
mod module_remap_queue_repository;
mod module_runtime_repository;
mod module_tree_repository;
mod policy_repository;
mod project_catalog;
mod project_layout;
mod project_map_atlas_insight_repository;
mod project_map_scene_repository;
mod project_map_search_repository;
mod repository_tree_repository;
mod run_journal_repository;
mod semantic_embedding_repository;
mod settings_repository;
mod task_ledger_repository;
mod task_lens_claim_repository;
mod task_lens_workspace_repository;
mod ui_preferences_repository;
mod verification_evidence_repository;

pub use catalog::{CatalogDatabase, CatalogOpenError, CatalogVerification};
pub use knowledge::{KnowledgeDatabase, KnowledgeOpenError, KnowledgeVerification};
pub use layout::{StorageLayout, StorageLayoutError};
pub use local_store::LibsqlKnowledgeStore;
pub use migration::{CatalogSchemaVersion, KnowledgeSchemaVersion};
pub use project_layout::{ProjectStorageEntry, ProjectStorageLayout, ProjectStorageLayoutError};

#[cfg(test)]
static NATIVE_LIBSQL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn run_native_libsql_test<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    let _guard = NATIVE_LIBSQL_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error>::from(std::io::Error::other(
            "native libSQL unit-test lock was poisoned",
        ))
    })?;
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread
        .name()
        .ok_or_else(|| std::io::Error::other("native libSQL test has no harness thread name"))?;
    #[cfg(windows)]
    if std::env::var_os("A3_NATIVE_LIBSQL_UNIT_TEST").as_deref()
        != Some(std::ffi::OsStr::new(test_name))
    {
        let marker = native_libsql_success_marker(test_name);
        const MAX_NATIVE_ATTEMPTS: u8 = 3;
        const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;
        for attempt in 1..=MAX_NATIVE_ATTEMPTS {
            remove_native_libsql_success_marker(&marker)?;
            let status = std::process::Command::new(std::env::current_exe()?)
                .arg(test_name)
                .arg("--exact")
                .arg("--test-threads=1")
                .env("A3_NATIVE_LIBSQL_UNIT_TEST", test_name)
                .env("A3_NATIVE_LIBSQL_SUCCESS_MARKER", &marker)
                .status()?;
            let completed = marker.is_file();
            remove_native_libsql_success_marker(&marker)?;
            if completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < MAX_NATIVE_ATTEMPTS {
                continue;
            }
            return Err(std::io::Error::other(format!(
                "isolated native libSQL test {test_name} failed on attempt {attempt} with {status} before completion evidence"
            ))
            .into());
        }
        return Err(std::io::Error::other(format!(
            "isolated native libSQL test {test_name} exhausted its retry bound"
        ))
        .into());
    }
    let result = futures::executor::block_on(future);
    #[cfg(windows)]
    match result {
        Ok(()) => {
            let marker = std::env::var_os("A3_NATIVE_LIBSQL_SUCCESS_MARKER")
                .ok_or_else(|| std::io::Error::other("native test success marker is missing"))?;
            std::fs::write(marker, b"complete")?;
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("native libSQL unit test failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    result
}

#[cfg(all(test, windows))]
fn native_libsql_success_marker(test_name: &str) -> std::path::PathBuf {
    let safe_name = test_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    std::env::temp_dir().join(format!(
        "a3-native-libsql-parent-{}-{safe_name}.complete",
        std::process::id()
    ))
}

#[cfg(all(test, windows))]
fn remove_native_libsql_success_marker(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
