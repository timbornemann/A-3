//! Deterministic bounded repository discovery and indexing for A^3.

mod classification;
mod config;
mod discovery;
mod path;

pub use discovery::GitRepositoryDiscoverer;
