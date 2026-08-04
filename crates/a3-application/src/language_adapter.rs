use crate::JobContext;
use a3_domain::{
    DiscoveredFileRoles, FileRevision, LanguageAdapterContractVersion, LanguageAdapterRevision,
    LanguageParseResult, Progress, RepositoryPath,
};
use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Exact bounded source bytes associated with a previously hashed file revision.
#[derive(Debug, Clone, Copy)]
pub struct LanguageParseInput<'a> {
    revision: &'a FileRevision,
    source: &'a [u8],
    discovery_roles: DiscoveredFileRoles,
}

impl<'a> LanguageParseInput<'a> {
    /// Creates a borrowed parse request; adapters must revalidate the content hash.
    #[must_use]
    pub const fn new(
        revision: &'a FileRevision,
        source: &'a [u8],
        discovery_roles: DiscoveredFileRoles,
    ) -> Self {
        Self {
            revision,
            source,
            discovery_roles,
        }
    }

    /// Returns the exact path and expected content hash.
    #[must_use]
    pub const fn revision(self) -> &'a FileRevision {
        self.revision
    }

    /// Returns source bytes bounded by the active parse policy.
    #[must_use]
    pub const fn source(self) -> &'a [u8] {
        self.source
    }

    /// Returns overlapping roles detected before parsing.
    #[must_use]
    pub const fn discovery_roles(self) -> DiscoveredFileRoles {
        self.discovery_roles
    }
}

/// Fixed V1 resource policy shared by every structural language adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageParsePolicy {
    contract_version: LanguageAdapterContractVersion,
    max_source_bytes: usize,
    parse_timeout: Duration,
    pool_wait_timeout: Duration,
    max_tree_nodes: usize,
    max_tree_depth: usize,
    max_symbols: usize,
    max_relations: usize,
    max_diagnostics: usize,
    max_progress_events: usize,
}

impl LanguageParsePolicy {
    /// Returns the immutable initial adapter contract and resource limits.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            contract_version: LanguageAdapterContractVersion::v1(),
            max_source_bytes: 4 * 1024 * 1024,
            parse_timeout: Duration::from_secs(2),
            pool_wait_timeout: Duration::from_millis(500),
            max_tree_nodes: 1_000_000,
            max_tree_depth: 4_096,
            max_symbols: 100_000,
            max_relations: 200_000,
            max_diagnostics: 4_096,
            max_progress_events: 32,
        }
    }

    /// Returns the versioned input/output contract.
    #[must_use]
    pub const fn contract_version(self) -> LanguageAdapterContractVersion {
        self.contract_version
    }

    /// Returns the largest source accepted by a structural adapter.
    #[must_use]
    pub const fn max_source_bytes(self) -> usize {
        self.max_source_bytes
    }

    /// Returns the per-file parser execution timeout.
    #[must_use]
    pub const fn parse_timeout(self) -> Duration {
        self.parse_timeout
    }

    /// Returns the maximum wait for one parser lease.
    #[must_use]
    pub const fn pool_wait_timeout(self) -> Duration {
        self.pool_wait_timeout
    }

    /// Returns the maximum traversed concrete-syntax nodes per file.
    #[must_use]
    pub const fn max_tree_nodes(self) -> usize {
        self.max_tree_nodes
    }

    /// Returns the maximum concrete-syntax nesting depth.
    #[must_use]
    pub const fn max_tree_depth(self) -> usize {
        self.max_tree_depth
    }

    /// Returns the maximum symbols emitted for one file.
    #[must_use]
    pub const fn max_symbols(self) -> usize {
        self.max_symbols
    }

    /// Returns the maximum syntactic relations emitted for one file.
    #[must_use]
    pub const fn max_relations(self) -> usize {
        self.max_relations
    }

    /// Returns the maximum diagnostics emitted for one file.
    #[must_use]
    pub const fn max_diagnostics(self) -> usize {
        self.max_diagnostics
    }

    /// Returns the maximum parser progress notifications for one file.
    #[must_use]
    pub const fn max_progress_events(self) -> usize {
        self.max_progress_events
    }
}

impl Default for LanguageParsePolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// Cooperative cancellation and bounded per-file progress boundary.
pub trait LanguageParseControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports monotone byte progress for the current file.
    fn report_progress(&self, progress: Progress) -> Result<(), LanguageParseControlError>;
}

impl LanguageParseControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), LanguageParseControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| LanguageParseControlError::Unavailable)
    }
}

/// Stable progress-delivery failure at the application boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageParseControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for LanguageParseControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("language parse progress is unavailable")
    }
}

impl Error for LanguageParseControlError {}

/// Outbound port implemented by each deterministic structural language adapter.
pub trait LanguageAdapter: fmt::Debug + Send + Sync {
    /// Returns the exact language, adapter, and grammar revision captured in snapshots.
    fn revision(&self) -> &LanguageAdapterRevision;

    /// Returns the shared input/output contract implemented by this adapter.
    fn contract_version(&self) -> LanguageAdapterContractVersion;

    /// Returns whether the adapter deterministically recognizes a repository path.
    fn supports_path(&self, path: &RepositoryPath) -> bool;

    /// Parses one verified source buffer without filesystem, persistence, or publication access.
    fn parse(
        &self,
        input: LanguageParseInput<'_>,
        policy: LanguageParsePolicy,
        control: &dyn LanguageParseControl,
    ) -> Result<LanguageParseResult, LanguageParseFailure>;
}

/// Stable application classification of one per-file language parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageParseFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// The adapter does not recognize the requested path.
    UnsupportedPath,
    /// Source exceeded the fixed per-file input bound.
    InputTooLarge,
    /// Source bytes did not match the expected file content hash.
    RevisionMismatch,
    /// No bounded parser lease became available before the wait budget expired.
    ParserUnavailable,
    /// Parsing exceeded its per-file execution timeout.
    TimedOut,
    /// The grammar did not produce a syntax tree.
    ParseFailed,
    /// Tree traversal or adapter artifacts exceeded a fixed bound.
    ResourceLimitExceeded,
    /// The owning scheduler rejected progress reporting.
    ProgressUnavailable,
    /// The adapter emitted output that violated a domain invariant.
    InvalidResult,
}

impl fmt::Display for LanguageParseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("language parse was cancelled"),
            Self::UnsupportedPath => formatter.write_str("language adapter does not support path"),
            Self::InputTooLarge => formatter.write_str("language parse input is too large"),
            Self::RevisionMismatch => formatter.write_str("language parse input hash conflicts"),
            Self::ParserUnavailable => formatter.write_str("language parser is unavailable"),
            Self::TimedOut => formatter.write_str("language parse timed out"),
            Self::ParseFailed => formatter.write_str("language parser produced no syntax tree"),
            Self::ResourceLimitExceeded => {
                formatter.write_str("language parse resource limit was exceeded")
            }
            Self::ProgressUnavailable => {
                formatter.write_str("language parse progress could not be reported")
            }
            Self::InvalidResult => formatter.write_str("language parse result is invalid"),
        }
    }
}

impl Error for LanguageParseFailure {}

#[cfg(test)]
mod tests {
    use super::{LanguageParseInput, LanguageParsePolicy};
    use a3_domain::{
        ContentHash, DiscoveredFileRole, DiscoveredFileRoles, FileRevision, RepositoryPath,
    };

    #[test]
    fn v1_policy_and_input_keep_all_parse_bounds_explicit() -> Result<(), Box<dyn std::error::Error>>
    {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let roles = DiscoveredFileRoles::empty().with(DiscoveredFileRole::Test);
        let input = LanguageParseInput::new(&revision, b"fn test() {}", roles);
        let policy = LanguageParsePolicy::v1();

        assert_eq!(input.revision(), &revision);
        assert!(input.discovery_roles().contains(DiscoveredFileRole::Test));
        assert_eq!(policy.max_source_bytes(), 4 * 1024 * 1024);
        assert!(policy.parse_timeout() > std::time::Duration::ZERO);
        assert!(policy.pool_wait_timeout() > std::time::Duration::ZERO);
        assert!(policy.max_tree_nodes() >= policy.max_symbols());
        Ok(())
    }
}
