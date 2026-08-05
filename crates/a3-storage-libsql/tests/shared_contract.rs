//! Executes the adapter-neutral storage contracts against local libSQL.

use a3_storage_contract_tests::{
    ContractError, ContractFactoryFuture, KnowledgeStoreContractFactory,
    verify_knowledge_store_contract,
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
fn libsql_satisfies_the_shared_storage_contract() -> Result<(), ContractError> {
    let _test_lock = lock_shared_contract_test()?;
    let result = block_on(verify_knowledge_store_contract(&LibsqlContractFactory));
    // The Windows native backend finishes worker teardown just after the final store drops.
    // Give that owned teardown a bounded grace period before the test process exits.
    #[cfg(windows)]
    std::thread::sleep(std::time::Duration::from_millis(500));
    result
}

fn lock_shared_contract_test() -> Result<MutexGuard<'static, ()>, ContractError> {
    SHARED_CONTRACT_TEST_LOCK.lock().map_err(|_| {
        Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
            "shared storage contract test lock was poisoned",
        ))
    })
}
