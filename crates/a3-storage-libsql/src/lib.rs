//! Local-only libSQL storage adapters for A^3 catalog and project data.

mod catalog;
mod layout;
mod migration;

pub use catalog::{CatalogDatabase, CatalogOpenError, CatalogVerification};
pub use layout::{StorageLayout, StorageLayoutError};
pub use migration::CatalogSchemaVersion;
