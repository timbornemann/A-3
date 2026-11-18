//! Local-only libSQL storage adapters for A^3 catalog and project data.

mod agent_ask_research_repository;
mod agent_permission_repository;
mod agent_recovery_repository;
mod agent_session_repository;
mod catalog;
mod command_allowlist_repository;
mod deep_map_journal_repository;
mod deep_map_repository;
mod edit_no_change_repository;
mod edit_phase_reservation_repository;
mod exact_search_projection;
mod exact_search_repository;
mod function_flow_codec;
mod function_flow_repository;
mod goal_contract_repository;
mod graph_traversal_repository;
mod http_context_intent_repository;
mod http_correction_plan_repository;
mod index_codec;
mod index_invalidation_repository;
mod index_publication;
mod index_repository;
mod index_trace_repository;
mod knowledge;
mod layout;
mod lexical_search_projection;
mod lexical_search_repository;
mod local_store;
mod loopback_http_codec;
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
#[cfg(test)]
mod plan22_goal_contract_tests;
mod policy_repository;
mod project_catalog;
mod project_layout;
mod project_map_atlas_insight_repository;
mod project_map_scene_repository;
mod project_map_search_repository;
mod python_translation_codec;
mod repository_tree_repository;
mod run_journal_repository;
mod semantic_embedding_repository;
mod settings_repository;
mod source_read_repository;
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
    run_native_libsql_test_with_mode(future, false)
}

#[cfg(test)]
pub(crate) fn run_native_libsql_live_test<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    run_native_libsql_test_with_mode(future, true)
}

#[cfg(test)]
fn run_native_libsql_test_with_mode<F>(
    future: F,
    #[cfg_attr(not(windows), allow(unused_variables))] live: bool,
) -> Result<(), Box<dyn std::error::Error>>
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
        let max_attempts = if live { 1 } else { MAX_NATIVE_ATTEMPTS };
        for attempt in 1..=max_attempts {
            remove_native_libsql_success_marker(&marker)?;
            let mut child = std::process::Command::new(std::env::current_exe()?);
            if live {
                child.arg("--ignored");
            }
            let status = child
                .arg(test_name)
                .arg("--exact")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env("A3_NATIVE_LIBSQL_UNIT_TEST", test_name)
                .env("A3_NATIVE_LIBSQL_SUCCESS_MARKER", &marker)
                .status()?;
            let completed = marker.is_file();
            remove_native_libsql_success_marker(&marker)?;
            if completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < max_attempts {
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
    let identity = blake3::hash(test_name.as_bytes()).to_hex();
    std::env::temp_dir().join(format!(
        "a3-native-libsql-parent-{}-{identity}.complete",
        std::process::id()
    ))
}

#[cfg(all(test, windows))]
#[test]
fn native_marker_accepts_deep_harness_names_without_collisions()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix = "nested_contract_module::".repeat(32);
    let first = native_libsql_success_marker(&format!("{prefix}first"));
    let second = native_libsql_success_marker(&format!("{prefix}second"));
    assert_ne!(first, second);
    assert_eq!(first.parent(), Some(std::env::temp_dir().as_path()));
    for marker in [first, second] {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)?;
        drop(file);
        std::fs::remove_file(marker)?;
    }
    Ok(())
}

#[cfg(all(test, windows))]
fn remove_native_libsql_success_marker(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
