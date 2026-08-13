//! Deterministic bounded repository discovery and indexing for A^3.

mod classification;
mod config;
mod discovery;
mod graph;
mod hashing;
mod incremental_index;
mod language_input;
mod parser_pool;
mod path;
mod python_adapter;
mod repository;
mod rust_adapter;
mod snapshot;
mod typescript_javascript_adapter;
mod watcher;

pub use config::RepositoryProjectIgnoreSettingsSource;
pub use discovery::GitRepositoryDiscoverer;
pub use graph::{
    DeterministicGraphLinker, DeterministicGraphRanker, DeterministicModuleFormer,
    GraphComputationControl, GraphComputationControlError, GraphLinkFailure, GraphLinkInput,
    GraphLinkPolicy, GraphRankFailure, ModuleFormationFailure, ModuleFormationInput,
    ModuleFormationPolicy, RankingPolicy,
};
pub use incremental_index::{
    Blake3IndexRunIdFactory, BuiltinIncrementalIndexCompiler,
    BuiltinIncrementalIndexCompilerCreateError,
};
pub use language_input::verify_language_parse_input;
pub use parser_pool::{
    ParserPoolCreateError, ParserPoolSize, ParserPoolSizeError, TreeSitterParse,
    TreeSitterParserPool, normalize_parse_diagnostics, source_range_for_node,
};
pub use python_adapter::{PythonLanguageAdapter, PythonLanguageAdapterCreateError};
pub use rust_adapter::{RustLanguageAdapter, RustLanguageAdapterCreateError};
pub use snapshot::Blake3RepositorySnapshotBuilder;
pub use typescript_javascript_adapter::{
    TypeScriptJavaScriptLanguageAdapter, TypeScriptJavaScriptLanguageAdapterCreateError,
};
pub use watcher::{
    PollingRepositoryWatcher, RepositoryWatcherConfig, RepositoryWatcherConfigError,
    RepositoryWatcherReceiveError, RepositoryWatcherShutdownError, RepositoryWatcherStartError,
};
