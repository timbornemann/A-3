//! Local-only libSQL storage adapters for A^3 catalog and project data.

mod catalog;
mod knowledge;
mod layout;
mod local_store;
mod migration;
mod project_catalog;
mod project_layout;

pub use catalog::{CatalogDatabase, CatalogOpenError, CatalogVerification};
pub use knowledge::{KnowledgeDatabase, KnowledgeOpenError, KnowledgeVerification};
pub use layout::{StorageLayout, StorageLayoutError};
pub use local_store::LibsqlKnowledgeStore;
pub use migration::{CatalogSchemaVersion, KnowledgeSchemaVersion};
pub use project_layout::{ProjectStorageEntry, ProjectStorageLayout, ProjectStorageLayoutError};
