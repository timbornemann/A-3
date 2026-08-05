use a3_domain::RepositoryPath;
use std::error::Error;
use std::fmt;

const MAX_CHANGE_HINTS: usize = 250_000;

/// Why a watcher observation must be confirmed by a complete repository rescan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepositoryRescanReason {
    /// A newly owned watcher has no prior observation to compare.
    InitialObservation,
    /// A bounded watcher queue could not retain every observation.
    EventLoss,
    /// Git HEAD or index metadata changed independently of file-content hints.
    RepositoryMetadataChanged,
    /// A polling observation could not be completed coherently.
    SourceUnavailable,
    /// The caller explicitly requested authoritative reconstruction.
    Explicit,
}

/// Canonical coalesced path hints for one repository-index refresh.
///
/// Hints are never authoritative. The snapshot adapter must confirm them through
/// Git-backed discovery and exact content hashing before any index mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryChangeBatch {
    paths: Vec<RepositoryPath>,
    full_rescan: Option<RepositoryRescanReason>,
}

impl RepositoryChangeBatch {
    /// Canonicalizes a non-empty incremental hint set.
    pub fn incremental(paths: Vec<RepositoryPath>) -> Result<Self, RepositoryChangeBatchError> {
        Self::new(paths, None)
    }

    /// Creates a full-rescan request, optionally retaining coalesced diagnostic hints.
    pub fn full_rescan(
        paths: Vec<RepositoryPath>,
        reason: RepositoryRescanReason,
    ) -> Result<Self, RepositoryChangeBatchError> {
        Self::new(paths, Some(reason))
    }

    fn new(
        mut paths: Vec<RepositoryPath>,
        full_rescan: Option<RepositoryRescanReason>,
    ) -> Result<Self, RepositoryChangeBatchError> {
        paths.sort();
        paths.dedup();
        if paths.len() > MAX_CHANGE_HINTS {
            return Err(RepositoryChangeBatchError::TooManyPaths);
        }
        if paths.is_empty() && full_rescan.is_none() {
            return Err(RepositoryChangeBatchError::EmptyIncrementalBatch);
        }
        Ok(Self { paths, full_rescan })
    }

    /// Returns canonical repository-relative path hints.
    #[must_use]
    pub fn paths(&self) -> &[RepositoryPath] {
        &self.paths
    }

    /// Returns the reason a complete rescan is mandatory, if any.
    #[must_use]
    pub const fn full_rescan_reason(&self) -> Option<RepositoryRescanReason> {
        self.full_rescan
    }

    /// Returns whether every relevant file must be rehashed.
    #[must_use]
    pub const fn requires_full_rescan(&self) -> bool {
        self.full_rescan.is_some()
    }
}

/// Invalid or unbounded watcher batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryChangeBatchError {
    /// An incremental batch contained no path hints.
    EmptyIncrementalBatch,
    /// The coalesced batch exceeded the fixed repository-file bound.
    TooManyPaths,
}

impl fmt::Display for RepositoryChangeBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIncrementalBatch => {
                formatter.write_str("incremental repository change batch is empty")
            }
            Self::TooManyPaths => {
                formatter.write_str("repository change batch exceeds the path limit")
            }
        }
    }
}

impl Error for RepositoryChangeBatchError {}

#[cfg(test)]
mod tests {
    use super::{RepositoryChangeBatch, RepositoryChangeBatchError, RepositoryRescanReason};
    use a3_domain::RepositoryPath;

    #[test]
    fn batches_coalesce_paths_and_require_evidence_for_empty_work()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let batch = RepositoryChangeBatch::incremental(vec![path.clone(), path.clone()])?;
        assert_eq!(batch.paths(), &[path]);
        assert!(!batch.requires_full_rescan());
        assert_eq!(
            RepositoryChangeBatch::incremental(Vec::new()),
            Err(RepositoryChangeBatchError::EmptyIncrementalBatch)
        );
        assert!(
            RepositoryChangeBatch::full_rescan(
                Vec::new(),
                RepositoryRescanReason::InitialObservation
            )?
            .requires_full_rescan()
        );
        Ok(())
    }
}
