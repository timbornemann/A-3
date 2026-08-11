use crate::JobContext;
use crate::RepositoryChangeBatch;
use a3_domain::{
    DiscoveryPolicy, DiscoveryResult, IndexSchemaVersion, LanguageAdapterRevision, Progress,
    ProjectIdentity, RepositoryFileState, Snapshot, SnapshotDelta,
};
use std::error::Error;
use std::fmt;

/// Durable predecessor and reconstructed effective file state for one snapshot build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotBaseline {
    latest_snapshot: Option<Snapshot>,
    files: RepositoryFileState,
}

impl SnapshotBaseline {
    /// Creates a coherent baseline; file revisions cannot exist before the first snapshot.
    pub fn new(
        latest_snapshot: Option<Snapshot>,
        files: RepositoryFileState,
    ) -> Result<Self, SnapshotBaselineError> {
        if latest_snapshot.is_none() && !files.is_empty() {
            return Err(SnapshotBaselineError::FilesWithoutSnapshot);
        }
        Ok(Self {
            latest_snapshot,
            files,
        })
    }

    /// Returns the empty baseline for a worktree without a prior snapshot.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            latest_snapshot: None,
            files: RepositoryFileState::empty(),
        }
    }

    /// Returns the latest immutable predecessor, if present.
    #[must_use]
    pub const fn latest_snapshot(&self) -> Option<&Snapshot> {
        self.latest_snapshot.as_ref()
    }

    /// Returns the effective file state reconstructed from the durable chain.
    #[must_use]
    pub const fn files(&self) -> &RepositoryFileState {
        &self.files
    }
}

/// Invalid relationship between a snapshot predecessor and its effective state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotBaselineError {
    /// Effective revisions were supplied although no snapshot exists.
    FilesWithoutSnapshot,
}

impl fmt::Display for SnapshotBaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("snapshot baseline has files without a predecessor")
    }
}

impl Error for SnapshotBaselineError {}

/// Exact schema and language-adapter revisions captured by a new snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCompatibility {
    index_schema_version: IndexSchemaVersion,
    adapter_revisions: Vec<LanguageAdapterRevision>,
}

impl SnapshotCompatibility {
    /// Canonicalizes adapter order and rejects missing or duplicate families.
    pub fn new(
        index_schema_version: IndexSchemaVersion,
        mut adapter_revisions: Vec<LanguageAdapterRevision>,
    ) -> Result<Self, SnapshotCompatibilityError> {
        if adapter_revisions.is_empty() {
            return Err(SnapshotCompatibilityError::MissingAdapterRevisions);
        }
        adapter_revisions.sort();
        if adapter_revisions
            .windows(2)
            .any(|pair| pair[0].language() == pair[1].language())
        {
            return Err(SnapshotCompatibilityError::DuplicateAdapterRevision);
        }
        Ok(Self {
            index_schema_version,
            adapter_revisions,
        })
    }

    /// Returns the deterministic index schema revision.
    #[must_use]
    pub const fn index_schema_version(&self) -> IndexSchemaVersion {
        self.index_schema_version
    }

    /// Returns adapter revisions in canonical language order.
    #[must_use]
    pub fn adapter_revisions(&self) -> &[LanguageAdapterRevision] {
        &self.adapter_revisions
    }
}

/// Invalid snapshot compatibility input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCompatibilityError {
    /// At least the generic adapter revision is required.
    MissingAdapterRevisions,
    /// One language family was supplied more than once.
    DuplicateAdapterRevision,
}

impl fmt::Display for SnapshotCompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAdapterRevisions => {
                formatter.write_str("snapshot compatibility requires adapter revisions")
            }
            Self::DuplicateAdapterRevision => {
                formatter.write_str("snapshot compatibility contains duplicate adapters")
            }
        }
    }
}

impl Error for SnapshotCompatibilityError {}

/// Fixed V1 resource limits for discovery plus full-content hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositorySnapshotPolicy {
    discovery: DiscoveryPolicy,
    max_file_bytes: u64,
    max_total_hash_bytes: u64,
    read_buffer_bytes: usize,
}

impl RepositorySnapshotPolicy {
    /// Returns the immutable V1 discovery and hashing policy.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            discovery: DiscoveryPolicy::v1(),
            max_file_bytes: 4 * 1024 * 1024,
            max_total_hash_bytes: 8 * 1024 * 1024 * 1024,
            read_buffer_bytes: 64 * 1024,
        }
    }

    /// Returns the discovery policy that must produce the candidate set.
    #[must_use]
    pub const fn discovery(self) -> DiscoveryPolicy {
        self.discovery
    }

    /// Returns the defensive per-file hashing boundary.
    #[must_use]
    pub const fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }

    /// Returns the aggregate full-content read boundary for one run.
    #[must_use]
    pub const fn max_total_hash_bytes(self) -> u64 {
        self.max_total_hash_bytes
    }

    /// Returns the largest individual filesystem read request.
    #[must_use]
    pub const fn read_buffer_bytes(self) -> usize {
        self.read_buffer_bytes
    }
}

impl Default for RepositorySnapshotPolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// Successful result of one coherent discovery and hashing observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositorySnapshotBuild {
    /// Content, HEAD, schema, and adapter compatibility equal the durable baseline.
    Unchanged {
        /// Freshly validated relevant candidate set.
        discovery: DiscoveryResult,
        /// Fresh full-content state, equal to the baseline state.
        files: RepositoryFileState,
    },
    /// A new immutable generation is required.
    Created {
        /// Freshly validated relevant candidate set.
        discovery: DiscoveryResult,
        /// Fresh full-content state represented by the snapshot.
        files: RepositoryFileState,
        /// Rich transient delta, including conservative rename hints.
        delta: SnapshotDelta,
        /// Deterministically identified immutable snapshot ready for persistence.
        snapshot: Box<Snapshot>,
    },
}

/// Cooperative cancellation and monotone progress boundary for one snapshot build.
pub trait RepositorySnapshotControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports indeterminate discovery or determinate hashing progress.
    fn report_progress(&self, progress: Progress) -> Result<(), RepositorySnapshotControlError>;

    /// Reports whether snapshot construction is discovering candidates or hashing contents.
    fn report_phase(
        &self,
        _phase: RepositorySnapshotPhase,
    ) -> Result<(), RepositorySnapshotControlError> {
        Ok(())
    }
}

/// Coarse snapshot phase mapped into the end-to-end Fast-Index lifecycle by its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySnapshotPhase {
    /// Candidate discovery and ignore classification are running.
    Discover,
    /// Exact content hashing and coherent snapshot confirmation are running.
    Hash,
}

impl RepositorySnapshotControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositorySnapshotControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| RepositorySnapshotControlError::Unavailable)
    }
}

/// Stable progress-delivery failure at the application boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySnapshotControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for RepositorySnapshotControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository snapshot progress is unavailable")
    }
}

impl Error for RepositorySnapshotControlError {}

/// Outbound port for coherent bounded discovery, BLAKE3 hashing, and snapshot planning.
pub trait RepositorySnapshotBuilder: fmt::Debug + Send + Sync {
    /// Produces an unchanged observation or the exact next immutable snapshot.
    fn build_snapshot(
        &self,
        project: &ProjectIdentity,
        baseline: &SnapshotBaseline,
        compatibility: &SnapshotCompatibility,
        policy: RepositorySnapshotPolicy,
        control: &dyn RepositorySnapshotControl,
    ) -> Result<RepositorySnapshotBuild, RepositorySnapshotFailure>;
}

/// One incrementally confirmed observation and the exact files that were rehashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalRepositorySnapshotBuild {
    observation: RepositorySnapshotBuild,
    hashed_paths: Vec<a3_domain::RepositoryPath>,
}

impl IncrementalRepositorySnapshotBuild {
    /// Creates adapter evidence after canonical path hashing.
    pub fn new(
        observation: RepositorySnapshotBuild,
        mut hashed_paths: Vec<a3_domain::RepositoryPath>,
    ) -> Result<Self, RepositorySnapshotFailure> {
        hashed_paths.sort();
        if hashed_paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RepositorySnapshotFailure::InvalidSnapshot);
        }
        Ok(Self {
            observation,
            hashed_paths,
        })
    }

    /// Returns the confirmed immutable snapshot observation.
    #[must_use]
    pub const fn observation(&self) -> &RepositorySnapshotBuild {
        &self.observation
    }

    /// Consumes the wrapper and returns the snapshot observation.
    #[must_use]
    pub fn into_observation(self) -> RepositorySnapshotBuild {
        self.observation
    }

    /// Returns only paths whose complete bytes were read during confirmation.
    #[must_use]
    pub fn hashed_paths(&self) -> &[a3_domain::RepositoryPath] {
        &self.hashed_paths
    }
}

/// Outbound port for hint-driven hashing with an authoritative full-rescan fallback.
pub trait IncrementalRepositorySnapshotBuilder: RepositorySnapshotBuilder {
    /// Confirms a coalesced watcher batch through Git discovery and exact BLAKE3 hashes.
    fn build_incremental_snapshot(
        &self,
        project: &ProjectIdentity,
        baseline: &SnapshotBaseline,
        compatibility: &SnapshotCompatibility,
        changes: &RepositoryChangeBatch,
        policy: RepositorySnapshotPolicy,
        control: &dyn RepositorySnapshotControl,
    ) -> Result<IncrementalRepositorySnapshotBuild, RepositorySnapshotFailure>;
}

/// Stable application classification of repository snapshot failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySnapshotFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// Discovery failed before any snapshot could be assembled.
    Discovery,
    /// Project, discovery, or baseline worktree identities disagree.
    IdentityMismatch,
    /// Git metadata is missing, inconsistent, or unsupported.
    InvalidRepository,
    /// A fixed file or aggregate hashing limit was exceeded.
    ResourceLimitExceeded,
    /// A file, HEAD, or Git index changed during the observation.
    WorktreeChanged,
    /// A repository path or file could not be accessed safely.
    Filesystem,
    /// The owning scheduler rejected progress reporting.
    ProgressUnavailable,
    /// Generation, compatibility, delta, or snapshot identity was invalid.
    InvalidSnapshot,
}

impl fmt::Display for RepositorySnapshotFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("repository snapshot was cancelled"),
            Self::Discovery => formatter.write_str("repository discovery failed"),
            Self::IdentityMismatch => formatter.write_str("repository snapshot identity conflicts"),
            Self::InvalidRepository => formatter.write_str("repository metadata is invalid"),
            Self::ResourceLimitExceeded => {
                formatter.write_str("repository snapshot resource limit was exceeded")
            }
            Self::WorktreeChanged => {
                formatter.write_str("worktree changed during snapshot observation")
            }
            Self::Filesystem => formatter.write_str("repository snapshot filesystem access failed"),
            Self::ProgressUnavailable => {
                formatter.write_str("repository snapshot progress could not be reported")
            }
            Self::InvalidSnapshot => formatter.write_str("repository snapshot is invalid"),
        }
    }
}

impl Error for RepositorySnapshotFailure {}

#[cfg(test)]
mod tests {
    use super::{SnapshotBaseline, SnapshotBaselineError, SnapshotCompatibility};
    use a3_domain::{
        ContentHash, FileRevision, IndexLanguage, IndexSchemaVersion, LanguageAdapterRevision,
        LanguageAdapterVersion, RepositoryFileState, RepositoryPath,
    };

    #[test]
    fn baseline_rejects_effective_files_without_a_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        let files = RepositoryFileState::new(vec![FileRevision::new(
            RepositoryPath::try_from_bytes(b"file.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        )])?;
        assert_eq!(
            SnapshotBaseline::new(None, files),
            Err(SnapshotBaselineError::FilesWithoutSnapshot)
        );
        Ok(())
    }

    #[test]
    fn compatibility_rejects_duplicate_adapter_families() -> Result<(), Box<dyn std::error::Error>>
    {
        let revision = || {
            Ok::<_, Box<dyn std::error::Error>>(LanguageAdapterRevision::new(
                IndexLanguage::Generic,
                LanguageAdapterVersion::try_from_string("v1".to_owned())?,
            ))
        };
        assert!(
            SnapshotCompatibility::new(
                IndexSchemaVersion::new(1)?,
                vec![revision()?, revision()?],
            )
            .is_err()
        );
        Ok(())
    }
}
