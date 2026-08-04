//! Deterministic bounded repository discovery and indexing for A^3.

mod classification;
mod config;
mod discovery;
mod hashing;
mod path;
mod repository;
mod snapshot;

pub use discovery::GitRepositoryDiscoverer;
pub use snapshot::Blake3RepositorySnapshotBuilder;
