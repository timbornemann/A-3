//! Reusable, adapter-neutral contracts for A^3 storage ports.
//!
//! This crate is dev-only. It depends on application and domain contracts, never
//! on a storage engine, so every adapter can execute the exact same behavior.

mod catalog;
mod fixture;
mod index;
mod reconciliation;
mod search;
mod semantic;

use a3_application::{
    KnowledgeIndexStore, KnowledgeSearchStore, KnowledgeStore, SemanticEmbeddingStore,
};
use std::error::Error;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// Boxed error used by adapter factories and shared contract scenarios.
pub type ContractError = Box<dyn Error + Send + Sync + 'static>;

/// Result returned by the shared contract harness.
pub type ContractResult<T> = Result<T, ContractError>;

/// Borrowing future returned by a storage-adapter factory.
pub type ContractFactoryFuture<'a, S> = Pin<Box<dyn Future<Output = ContractResult<S>> + 'a>>;

/// Creates a fresh or reopened adapter at a contract-owned app-data root.
///
/// Implementations may translate the generic path into their own validated
/// layout type, but must not change the contract scenarios.
pub trait KnowledgeStoreContractFactory {
    /// Concrete adapter that implements every current storage capability.
    type Store: KnowledgeStore + KnowledgeIndexStore + KnowledgeSearchStore + SemanticEmbeddingStore;

    /// Opens the store at `app_data_root`, preserving data across repeated calls.
    fn open<'a>(&'a self, app_data_root: &'a Path) -> ContractFactoryFuture<'a, Self::Store>;
}

/// Runs every shared catalog, snapshot, and index-run contract sequentially.
///
/// A fresh temporary workspace is owned for the entire run. Individual
/// scenarios use distinct app-data roots so their durable state cannot leak.
pub async fn verify_knowledge_store_contract<F>(factory: &F) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let workspace = fixture::ContractWorkspace::new()?;
    catalog::verify(factory, &workspace).await?;
    index::verify(factory, &workspace).await?;
    search::verify(factory, &workspace).await?;
    semantic::verify(factory, &workspace).await?;
    reconciliation::verify(factory, &workspace).await
}
