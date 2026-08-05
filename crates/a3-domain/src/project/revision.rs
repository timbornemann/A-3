use super::{ContentHash, RepositoryPath, SnapshotChange, SnapshotChangeKind};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// Current verified content identity of one relevant repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRevision {
    path: RepositoryPath,
    content_hash: ContentHash,
}

impl FileRevision {
    /// Creates one revision from a normalized path and a full-content digest.
    #[must_use]
    pub const fn new(path: RepositoryPath, content_hash: ContentHash) -> Self {
        Self { path, content_hash }
    }

    /// Returns the repository-relative path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns the BLAKE3 content identity.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }
}

/// Complete effective set of relevant file revisions at one worktree observation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryFileState {
    revisions: Vec<FileRevision>,
}

impl RepositoryFileState {
    /// Canonicalizes bytewise path order and rejects duplicate paths.
    pub fn new(mut revisions: Vec<FileRevision>) -> Result<Self, RepositoryFileStateError> {
        revisions.sort_by(|left, right| left.path.cmp(&right.path));
        if revisions
            .windows(2)
            .any(|pair| pair[0].path == pair[1].path)
        {
            return Err(RepositoryFileStateError::DuplicatePath);
        }
        Ok(Self { revisions })
    }

    /// Returns an empty effective file set.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            revisions: Vec::new(),
        }
    }

    /// Returns revisions in canonical repository-path byte order.
    #[must_use]
    pub fn revisions(&self) -> &[FileRevision] {
        &self.revisions
    }

    /// Returns whether the effective file set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.revisions.is_empty()
    }
}

/// Invalid effective repository file state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryFileStateError {
    /// More than one revision targeted the same normalized path.
    DuplicatePath,
}

impl fmt::Display for RepositoryFileStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePath => {
                formatter.write_str("repository file state contains duplicate paths")
            }
        }
    }
}

impl Error for RepositoryFileStateError {}

/// Exact content transition for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDelta {
    /// A previously absent path now has a verified revision.
    Added {
        /// Current revision at the new path.
        current: FileRevision,
    },
    /// An existing path now has different verified content.
    Modified {
        /// Previous revision being invalidated.
        previous: FileRevision,
        /// New content hash at the same path.
        current_hash: ContentHash,
    },
    /// A previously present path is absent from the new relevant file set.
    Deleted {
        /// Previous revision being invalidated.
        previous: FileRevision,
    },
}

impl FileDelta {
    /// Returns the affected repository path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        match self {
            Self::Added { current } => current.path(),
            Self::Modified { previous, .. } | Self::Deleted { previous } => previous.path(),
        }
    }

    /// Returns the prior content hash when an existing revision is invalidated.
    #[must_use]
    pub const fn previous_hash(&self) -> Option<ContentHash> {
        match self {
            Self::Added { .. } => None,
            Self::Modified { previous, .. } | Self::Deleted { previous } => {
                Some(previous.content_hash())
            }
        }
    }

    /// Returns the current content hash when the path remains present.
    #[must_use]
    pub const fn current_hash(&self) -> Option<ContentHash> {
        match self {
            Self::Added { current } => Some(current.content_hash()),
            Self::Modified { current_hash, .. } => Some(*current_hash),
            Self::Deleted { .. } => None,
        }
    }

    fn as_snapshot_change(&self) -> SnapshotChange {
        match self {
            Self::Added { current } => SnapshotChange::new(
                current.path().clone(),
                current.content_hash(),
                SnapshotChangeKind::Upsert,
            ),
            Self::Modified {
                previous,
                current_hash,
            } => SnapshotChange::new(
                previous.path().clone(),
                *current_hash,
                SnapshotChangeKind::Upsert,
            ),
            Self::Deleted { previous } => SnapshotChange::new(
                previous.path().clone(),
                previous.content_hash(),
                SnapshotChangeKind::Delete,
            ),
        }
    }
}

/// Conservative content-equality hint that one deleted path may have moved to one added path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenameCandidate {
    from: RepositoryPath,
    to: RepositoryPath,
    content_hash: ContentHash,
}

impl RenameCandidate {
    fn new(from: RepositoryPath, to: RepositoryPath, content_hash: ContentHash) -> Self {
        Self {
            from,
            to,
            content_hash,
        }
    }

    /// Returns the deleted prior path.
    #[must_use]
    pub const fn from(&self) -> &RepositoryPath {
        &self.from
    }

    /// Returns the added current path.
    #[must_use]
    pub const fn to(&self) -> &RepositoryPath {
        &self.to
    }

    /// Returns the exact content digest shared by both paths.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }
}

/// Deterministic difference between two complete effective file states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDelta {
    files: Vec<FileDelta>,
    rename_candidates: Vec<RenameCandidate>,
}

impl SnapshotDelta {
    /// Returns a content-empty transition, used for HEAD-only observations and cache warmup.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            files: Vec::new(),
            rename_candidates: Vec::new(),
        }
    }

    /// Computes added, modified, deleted, and unambiguous content-equal rename candidates.
    #[must_use]
    pub fn between(previous: &RepositoryFileState, current: &RepositoryFileState) -> Self {
        let mut files = Vec::new();
        let mut previous_index = 0usize;
        let mut current_index = 0usize;

        while previous_index < previous.revisions.len() || current_index < current.revisions.len() {
            match (
                previous.revisions.get(previous_index),
                current.revisions.get(current_index),
            ) {
                (Some(previous_revision), Some(current_revision)) => {
                    match previous_revision.path.cmp(&current_revision.path) {
                        std::cmp::Ordering::Less => {
                            files.push(FileDelta::Deleted {
                                previous: previous_revision.clone(),
                            });
                            previous_index = previous_index.saturating_add(1);
                        }
                        std::cmp::Ordering::Greater => {
                            files.push(FileDelta::Added {
                                current: current_revision.clone(),
                            });
                            current_index = current_index.saturating_add(1);
                        }
                        std::cmp::Ordering::Equal => {
                            if previous_revision.content_hash != current_revision.content_hash {
                                files.push(FileDelta::Modified {
                                    previous: previous_revision.clone(),
                                    current_hash: current_revision.content_hash,
                                });
                            }
                            previous_index = previous_index.saturating_add(1);
                            current_index = current_index.saturating_add(1);
                        }
                    }
                }
                (Some(previous_revision), None) => {
                    files.push(FileDelta::Deleted {
                        previous: previous_revision.clone(),
                    });
                    previous_index = previous_index.saturating_add(1);
                }
                (None, Some(current_revision)) => {
                    files.push(FileDelta::Added {
                        current: current_revision.clone(),
                    });
                    current_index = current_index.saturating_add(1);
                }
                (None, None) => break,
            }
        }

        let rename_candidates = rename_candidates(&files);
        Self {
            files,
            rename_candidates,
        }
    }

    /// Returns path deltas in canonical byte order.
    #[must_use]
    pub fn files(&self) -> &[FileDelta] {
        &self.files
    }

    /// Returns only unique one-deleted-to-one-added content matches.
    #[must_use]
    pub fn rename_candidates(&self) -> &[RenameCandidate] {
        &self.rename_candidates
    }

    /// Returns whether content state is unchanged.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Converts the richer transient delta to the durable snapshot change contract.
    #[must_use]
    pub fn snapshot_changes(&self) -> Vec<SnapshotChange> {
        self.files
            .iter()
            .map(FileDelta::as_snapshot_change)
            .collect()
    }
}

#[derive(Default)]
struct RenameSides {
    deleted: Vec<RepositoryPath>,
    added: Vec<RepositoryPath>,
}

fn rename_candidates(files: &[FileDelta]) -> Vec<RenameCandidate> {
    let mut by_hash = BTreeMap::<ContentHash, RenameSides>::new();
    for delta in files {
        match delta {
            FileDelta::Added { current } => by_hash
                .entry(current.content_hash())
                .or_default()
                .added
                .push(current.path().clone()),
            FileDelta::Deleted { previous } => by_hash
                .entry(previous.content_hash())
                .or_default()
                .deleted
                .push(previous.path().clone()),
            FileDelta::Modified { .. } => {}
        }
    }
    let mut candidates = by_hash
        .into_iter()
        .filter_map(|(content_hash, sides)| {
            match (sides.deleted.as_slice(), sides.added.as_slice()) {
                ([from], [to]) => {
                    Some(RenameCandidate::new(from.clone(), to.clone(), content_hash))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

#[cfg(test)]
mod tests {
    use super::{FileDelta, FileRevision, RepositoryFileState, SnapshotDelta};
    use crate::{ContentHash, RepositoryPath, SnapshotChangeKind};

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    #[test]
    fn delta_skips_unchanged_and_distinguishes_add_modify_delete()
    -> Result<(), Box<dyn std::error::Error>> {
        let previous = RepositoryFileState::new(vec![
            revision("deleted.rs", 1)?,
            revision("modified.rs", 2)?,
            revision("same.rs", 3)?,
        ])?;
        let current = RepositoryFileState::new(vec![
            revision("added.rs", 4)?,
            revision("modified.rs", 5)?,
            revision("same.rs", 3)?,
        ])?;

        let delta = SnapshotDelta::between(&previous, &current);
        assert_eq!(delta.files().len(), 3);
        assert!(matches!(delta.files()[0], FileDelta::Added { .. }));
        assert!(matches!(delta.files()[1], FileDelta::Deleted { .. }));
        assert!(matches!(delta.files()[2], FileDelta::Modified { .. }));
        assert_eq!(
            delta
                .snapshot_changes()
                .iter()
                .map(|change| change.kind())
                .collect::<Vec<_>>(),
            vec![
                SnapshotChangeKind::Upsert,
                SnapshotChangeKind::Delete,
                SnapshotChangeKind::Upsert,
            ]
        );
        Ok(())
    }

    #[test]
    fn rename_hint_requires_one_unique_add_and_delete_for_a_hash()
    -> Result<(), Box<dyn std::error::Error>> {
        let previous = RepositoryFileState::new(vec![revision("old.rs", 7)?])?;
        let current = RepositoryFileState::new(vec![revision("new.rs", 7)?])?;
        let delta = SnapshotDelta::between(&previous, &current);
        assert_eq!(delta.rename_candidates().len(), 1);
        assert_eq!(delta.rename_candidates()[0].from().as_bytes(), b"old.rs");
        assert_eq!(delta.rename_candidates()[0].to().as_bytes(), b"new.rs");

        let ambiguous =
            RepositoryFileState::new(vec![revision("copy-a.rs", 7)?, revision("copy-b.rs", 7)?])?;
        assert!(
            SnapshotDelta::between(&previous, &ambiguous)
                .rename_candidates()
                .is_empty()
        );
        Ok(())
    }
}
