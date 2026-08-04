//! Deterministic bounded repository discovery and indexing for A^3.

mod classification;
mod config;
mod discovery;
mod hashing;
mod language_input;
mod parser_pool;
mod path;
mod repository;
mod snapshot;

pub use discovery::GitRepositoryDiscoverer;
pub use language_input::verify_language_parse_input;
pub use parser_pool::{
    ParserPoolCreateError, ParserPoolSize, ParserPoolSizeError, TreeSitterParse,
    TreeSitterParserPool, source_range_for_node,
};
pub use snapshot::Blake3RepositorySnapshotBuilder;
