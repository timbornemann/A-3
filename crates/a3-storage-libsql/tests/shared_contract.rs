//! Executes the adapter-neutral storage contracts against local libSQL.

use a3_storage_contract_tests::{
    ContractError, ContractFactoryFuture, KnowledgeStoreContractFactory,
    KnowledgeStoreContractGroup, verify_knowledge_store_contract_group,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use futures::executor::block_on;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

// libSQL's Windows native test runtime is not safe when separate local database
// fixtures are opened and torn down concurrently inside one process.
static SHARED_CONTRACT_TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy)]
struct LibsqlContractFactory;

impl KnowledgeStoreContractFactory for LibsqlContractFactory {
    type Store = LibsqlKnowledgeStore;

    fn open<'a>(&'a self, app_data_root: &'a Path) -> ContractFactoryFuture<'a, Self::Store> {
        Box::pin(async move {
            let layout = StorageLayout::prepare(app_data_root)?;
            Ok(LibsqlKnowledgeStore::open(&layout).await?)
        })
    }
}

#[test]
fn libsql_satisfies_shared_catalog_recency_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::CatalogRecency)
}

#[test]
fn libsql_satisfies_shared_catalog_linked_worktree_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::CatalogLinkedWorktrees)
}

#[test]
fn libsql_satisfies_shared_index_snapshot_validation_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexSnapshotValidation)
}

#[test]
fn libsql_satisfies_shared_index_snapshot_reopen_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexSnapshotReopen)
}

#[test]
fn libsql_satisfies_shared_index_run_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexRuns)
}

#[test]
fn libsql_satisfies_shared_index_publication_visibility_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexPublicationVisibility)
}

#[test]
fn libsql_satisfies_shared_index_duplicate_publication_rejection() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexDuplicatePublicationRejection)
}

#[test]
fn libsql_satisfies_shared_index_mismatched_publication_rejection() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexMismatchedPublicationRejection)
}

#[test]
fn libsql_satisfies_shared_index_replacement_publication() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexReplacementPublication)
}

#[test]
fn libsql_satisfies_shared_index_rebuild_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::IndexRebuild)
}

#[test]
fn libsql_satisfies_shared_module_card_publication_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::ModuleCardPublication)
}

#[test]
fn libsql_satisfies_shared_goal_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::GoalContracts)
}

#[test]
fn libsql_satisfies_shared_task_ledger_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::TaskLedgers)
}

#[test]
fn libsql_satisfies_shared_run_journal_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::RunJournals)
}

#[test]
fn libsql_satisfies_shared_agent_recovery_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::AgentRecovery)
}

#[test]
fn libsql_satisfies_shared_search_availability_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchAvailability)
}

#[test]
fn libsql_satisfies_shared_search_cancellation_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchCancellation)
}

#[test]
fn libsql_satisfies_shared_exact_search_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchExact)
}

#[test]
fn libsql_satisfies_shared_lexical_search_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchLexical)
}

#[test]
fn libsql_satisfies_shared_graph_search_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchGraph)
}

#[test]
fn libsql_satisfies_shared_search_replacement_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::SearchReplacement)
}

#[test]
fn libsql_satisfies_shared_semantic_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::Semantic)
}

#[test]
fn libsql_satisfies_shared_worktree_move_reconciliation_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::ReconciliationWorktreeMove)
}

#[test]
fn libsql_satisfies_shared_repository_move_reconciliation_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::ReconciliationRepositoryMove)
}

#[test]
fn libsql_satisfies_shared_separate_open_reconciliation_contract() -> Result<(), ContractError> {
    run_shared_contract(KnowledgeStoreContractGroup::ReconciliationSeparateOpen)
}

fn run_shared_contract(group: KnowledgeStoreContractGroup) -> Result<(), ContractError> {
    let _test_lock = lock_shared_contract_test()?;
    #[cfg(windows)]
    let current_thread = std::thread::current();
    #[cfg(windows)]
    let test_name = current_thread.name().ok_or_else(|| {
        Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
            "shared libSQL contract test has no harness thread name",
        ))
    })?;
    #[cfg(windows)]
    if std::env::var_os("A3_LIBSQL_ISOLATED_TEST").as_deref()
        != Some(std::ffi::OsStr::new(test_name))
    {
        let success_marker = isolated_contract_success_marker(test_name);
        const MAX_NATIVE_ATTEMPTS: u8 = 3;
        const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;
        for attempt in 1..=MAX_NATIVE_ATTEMPTS {
            remove_success_marker(&success_marker)?;
            let mut child = std::process::Command::new(std::env::current_exe()?)
                .arg(test_name)
                .arg("--exact")
                .arg("--test-threads=1")
                .env("A3_LIBSQL_ISOLATED_TEST", test_name)
                .env("A3_STORAGE_CONTRACT_RETAIN_WORKSPACE", "1")
                .env("A3_STORAGE_CONTRACT_SUCCESS_MARKER", &success_marker)
                .spawn()?;
            let child_id = child.id();
            let status = child.wait()?;
            cleanup_isolated_contract_workspaces(child_id)?;
            let contract_completed = success_marker.is_file();
            remove_success_marker(&success_marker)?;
            if contract_completed {
                return Ok(());
            }
            if status.code() == Some(STATUS_ACCESS_VIOLATION) && attempt < MAX_NATIVE_ATTEMPTS {
                continue;
            }
            return Err(std::io::Error::other(format!(
                "isolated shared libSQL contract {test_name} failed on attempt {attempt} with {status} before completion evidence"
            ))
            .into());
        }
        return Err(std::io::Error::other(format!(
            "isolated shared libSQL contract {test_name} exhausted its native retry bound"
        ))
        .into());
    }
    let result = block_on(verify_knowledge_store_contract_group(
        &LibsqlContractFactory,
        group,
    ));
    #[cfg(windows)]
    match result {
        Ok(()) => {
            let marker = std::env::var_os("A3_STORAGE_CONTRACT_SUCCESS_MARKER")
                .ok_or_else(|| std::io::Error::other("success marker path is missing"))?;
            std::fs::write(marker, b"complete")?;
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("shared libSQL contract failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    result
}

#[cfg(windows)]
fn isolated_contract_success_marker(test_name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "a3-storage-contract-parent-{}-{test_name}.complete",
        std::process::id()
    ))
}

#[cfg(windows)]
fn remove_success_marker(path: &Path) -> Result<(), ContractError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
fn cleanup_isolated_contract_workspaces(child_id: u32) -> Result<(), ContractError> {
    let temporary_root = std::env::temp_dir();
    let expected_prefix = format!("a3-storage-contract-{child_id}-");
    for entry in std::fs::read_dir(&temporary_root)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(&expected_prefix)
        {
            continue;
        }
        let target = entry.path();
        if target.parent() != Some(temporary_root.as_path()) {
            return Err(std::io::Error::other(
                "isolated contract workspace escaped the temporary root",
            )
            .into());
        }
        std::fs::remove_dir_all(target)?;
    }
    Ok(())
}

fn lock_shared_contract_test() -> Result<MutexGuard<'static, ()>, ContractError> {
    SHARED_CONTRACT_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
            "shared storage contract test lock was poisoned",
        ))
    })
}
