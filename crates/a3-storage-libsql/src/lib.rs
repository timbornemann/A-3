//! Local-only libSQL storage adapters for A^3 catalog and project data.

mod catalog;
mod exact_search_projection;
mod exact_search_repository;
mod graph_traversal_repository;
mod index_codec;
mod index_publication;
mod index_repository;
mod knowledge;
mod layout;
mod lexical_search_projection;
mod lexical_search_repository;
mod local_store;
mod migration;
mod module_projection_codec;
mod project_catalog;
mod project_layout;
mod semantic_embedding_repository;

pub use catalog::{CatalogDatabase, CatalogOpenError, CatalogVerification};
pub use knowledge::{KnowledgeDatabase, KnowledgeOpenError, KnowledgeVerification};
pub use layout::{StorageLayout, StorageLayoutError};
pub use local_store::LibsqlKnowledgeStore;
pub use migration::{CatalogSchemaVersion, KnowledgeSchemaVersion};
pub use project_layout::{ProjectStorageEntry, ProjectStorageLayout, ProjectStorageLayoutError};
