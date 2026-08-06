use super::{FileRevision, RepositoryPath, SnapshotId, WorktreeId};
use std::error::Error;
use std::fmt;

const MAX_DIRECTORY_PAGE_ENTRIES: u16 = 256;

/// Repository root or one normalized repository-relative directory.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum WorkspaceDirectory {
    /// The selected worktree root.
    Root,
    /// One directory below the selected worktree root.
    Subtree(RepositoryPath),
}

impl WorkspaceDirectory {
    /// Returns the repository-relative directory path, or `None` for the root.
    #[must_use]
    pub const fn path(&self) -> Option<&RepositoryPath> {
        match self {
            Self::Root => None,
            Self::Subtree(path) => Some(path),
        }
    }

    /// Returns whether a repository path is an immediate child of this directory.
    #[must_use]
    pub fn contains_direct_child(&self, candidate: &RepositoryPath) -> bool {
        direct_child_component(
            self.path().map(RepositoryPath::as_bytes),
            candidate.as_bytes(),
        )
        .is_some_and(|component| component.len() == candidate_suffix(self, candidate).len())
    }

    /// Returns the first child component for a descendant path.
    #[must_use]
    pub fn direct_child_component<'a>(&self, candidate: &'a RepositoryPath) -> Option<&'a [u8]> {
        direct_child_component(
            self.path().map(RepositoryPath::as_bytes),
            candidate.as_bytes(),
        )
    }
}

impl fmt::Debug for WorkspaceDirectory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Root => "WorkspaceDirectory::Root",
            Self::Subtree(_) => "WorkspaceDirectory::Subtree([REDACTED])",
        })
    }
}

fn candidate_suffix<'a>(directory: &WorkspaceDirectory, candidate: &'a RepositoryPath) -> &'a [u8] {
    match directory.path() {
        None => candidate.as_bytes(),
        Some(path) => candidate
            .as_bytes()
            .get(path.as_bytes().len().saturating_add(1)..)
            .unwrap_or_default(),
    }
}

fn direct_child_component<'a>(directory: Option<&[u8]>, candidate: &'a [u8]) -> Option<&'a [u8]> {
    let suffix = match directory {
        None => candidate,
        Some(directory) => {
            let remainder = candidate.strip_prefix(directory)?;
            remainder.strip_prefix(b"/")?
        }
    };
    if suffix.is_empty() {
        return None;
    }
    let end = suffix
        .iter()
        .position(|byte| *byte == b'/')
        .unwrap_or(suffix.len());
    suffix.get(..end)
}

/// Bounded count of entries returned by one directory-listing page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryPageSize(u16);

impl DirectoryPageSize {
    /// Creates a non-zero page size no larger than 256 entries.
    pub const fn new(value: u16) -> Result<Self, DirectoryPageSizeError> {
        if value == 0 || value > MAX_DIRECTORY_PAGE_ENTRIES {
            return Err(DirectoryPageSizeError { value });
        }
        Ok(Self(value))
    }

    /// Returns the stable primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A directory page size was zero or exceeded the fixed boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryPageSizeError {
    value: u16,
}

impl fmt::Display for DirectoryPageSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "directory page size {} is outside 1..={MAX_DIRECTORY_PAGE_ENTRIES}",
            self.value
        )
    }
}

impl Error for DirectoryPageSizeError {}

/// Snapshot-bound request for one forward-only directory page.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryListRequest {
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
    directory: WorkspaceDirectory,
    after: Option<RepositoryPath>,
    page_size: DirectoryPageSize,
}

impl WorkspaceDirectoryListRequest {
    /// Creates a request whose optional cursor names an immediate child of the directory.
    pub fn new(
        worktree_id: WorktreeId,
        snapshot_id: SnapshotId,
        directory: WorkspaceDirectory,
        after: Option<RepositoryPath>,
        page_size: DirectoryPageSize,
    ) -> Result<Self, WorkspaceDirectoryListRequestError> {
        if after
            .as_ref()
            .is_some_and(|cursor| !directory.contains_direct_child(cursor))
        {
            return Err(WorkspaceDirectoryListRequestError::CursorOutsideDirectory);
        }
        Ok(Self {
            worktree_id,
            snapshot_id,
            directory,
            after,
            page_size,
        })
    }

    /// Returns the exact selected worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the immutable published snapshot to list.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the requested root or subtree.
    #[must_use]
    pub const fn directory(&self) -> &WorkspaceDirectory {
        &self.directory
    }

    /// Returns the exclusive forward cursor.
    #[must_use]
    pub const fn after(&self) -> Option<&RepositoryPath> {
        self.after.as_ref()
    }

    /// Returns the maximum entries in this page.
    #[must_use]
    pub const fn page_size(&self) -> DirectoryPageSize {
        self.page_size
    }
}

impl fmt::Debug for WorkspaceDirectoryListRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceDirectoryListRequest")
            .field("worktree_id", &self.worktree_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("directory", &self.directory)
            .field("has_cursor", &self.after.is_some())
            .field("page_size", &self.page_size)
            .finish()
    }
}

/// A directory cursor did not belong to the requested directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDirectoryListRequestError {
    /// The cursor was not an immediate child of the requested directory.
    CursorOutsideDirectory,
}

impl fmt::Display for WorkspaceDirectoryListRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("directory cursor is outside the requested directory")
    }
}

impl Error for WorkspaceDirectoryListRequestError {}

/// Stable kind of one published directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDirectoryEntryKind {
    /// A direct indexed file.
    File,
    /// A direct directory inferred from at least one indexed descendant.
    Directory,
}

/// One direct child backed by an exact current file revision.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryEntry {
    path: RepositoryPath,
    kind: WorkspaceDirectoryEntryKind,
    supporting_revision: FileRevision,
}

impl WorkspaceDirectoryEntry {
    /// Creates a direct file entry whose path is its evidence revision path.
    #[must_use]
    pub fn file(revision: FileRevision) -> Self {
        Self {
            path: revision.path().clone(),
            kind: WorkspaceDirectoryEntryKind::File,
            supporting_revision: revision,
        }
    }

    /// Creates an inferred directory backed by one exact indexed descendant.
    pub fn directory(
        path: RepositoryPath,
        supporting_revision: FileRevision,
    ) -> Result<Self, WorkspaceDirectoryEntryError> {
        let prefix_length = path.as_bytes().len();
        let supports_directory = supporting_revision
            .path()
            .as_bytes()
            .strip_prefix(path.as_bytes())
            .is_some_and(|suffix| suffix.starts_with(b"/") && suffix.len() > 1);
        if !supports_directory || prefix_length >= supporting_revision.path().as_bytes().len() {
            return Err(WorkspaceDirectoryEntryError::InvalidDirectoryEvidence);
        }
        Ok(Self {
            path,
            kind: WorkspaceDirectoryEntryKind::Directory,
            supporting_revision,
        })
    }

    /// Returns the direct child path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns whether this entry is a file or inferred directory.
    #[must_use]
    pub const fn kind(&self) -> WorkspaceDirectoryEntryKind {
        self.kind
    }

    /// Returns exact snapshot evidence supporting this entry.
    #[must_use]
    pub const fn supporting_revision(&self) -> &FileRevision {
        &self.supporting_revision
    }
}

impl fmt::Debug for WorkspaceDirectoryEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceDirectoryEntry")
            .field("kind", &self.kind)
            .field("path", &"[REDACTED]")
            .field("supporting_revision", &"[REDACTED]")
            .finish()
    }
}

/// An inferred directory did not have a strict descendant as evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDirectoryEntryError {
    /// The evidence revision was not below the directory path.
    InvalidDirectoryEvidence,
}

impl fmt::Display for WorkspaceDirectoryEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("directory entry lacks descendant file evidence")
    }
}

impl Error for WorkspaceDirectoryEntryError {}

/// One canonical, snapshot-bound and forward-only directory page.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryListing {
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
    directory: WorkspaceDirectory,
    entries: Vec<WorkspaceDirectoryEntry>,
    next_after: Option<RepositoryPath>,
    truncated: bool,
}

impl WorkspaceDirectoryListing {
    /// Validates entry ownership, ordering, bounds, and cursor/truncation consistency.
    pub fn new(
        request: &WorkspaceDirectoryListRequest,
        entries: Vec<WorkspaceDirectoryEntry>,
        next_after: Option<RepositoryPath>,
        truncated: bool,
    ) -> Result<Self, WorkspaceDirectoryListingError> {
        if entries.len() > usize::from(request.page_size().get()) {
            return Err(WorkspaceDirectoryListingError::TooManyEntries {
                actual: entries.len(),
            });
        }
        if entries
            .iter()
            .any(|entry| !request.directory().contains_direct_child(entry.path()))
        {
            return Err(WorkspaceDirectoryListingError::EntryOutsideDirectory);
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].path() >= pair[1].path())
        {
            return Err(WorkspaceDirectoryListingError::EntriesNotCanonical);
        }
        if entries
            .iter()
            .any(|entry| request.after().is_some_and(|cursor| entry.path() <= cursor))
        {
            return Err(WorkspaceDirectoryListingError::EntryBeforeCursor);
        }
        let expected_cursor = entries.last().map(|entry| entry.path().clone());
        if truncated {
            if next_after != expected_cursor {
                return Err(WorkspaceDirectoryListingError::InvalidNextCursor);
            }
        } else if next_after.is_some() {
            return Err(WorkspaceDirectoryListingError::InvalidNextCursor);
        }
        Ok(Self {
            worktree_id: request.worktree_id(),
            snapshot_id: request.snapshot_id(),
            directory: request.directory().clone(),
            entries,
            next_after,
            truncated,
        })
    }

    /// Returns the exact selected worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the immutable published snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the listed directory.
    #[must_use]
    pub const fn directory(&self) -> &WorkspaceDirectory {
        &self.directory
    }

    /// Returns direct children in canonical repository-path order.
    #[must_use]
    pub fn entries(&self) -> &[WorkspaceDirectoryEntry] {
        &self.entries
    }

    /// Returns the exclusive cursor for the next page.
    #[must_use]
    pub const fn next_after(&self) -> Option<&RepositoryPath> {
        self.next_after.as_ref()
    }

    /// Returns whether more direct children remain.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for WorkspaceDirectoryListing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceDirectoryListing")
            .field("worktree_id", &self.worktree_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("directory", &self.directory)
            .field("entry_count", &self.entries.len())
            .field("has_next_after", &self.next_after.is_some())
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// A directory adapter assembled an invalid page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDirectoryListingError {
    /// More entries were returned than requested.
    TooManyEntries {
        /// Observed entry count.
        actual: usize,
    },
    /// An entry was not an immediate child of the requested directory.
    EntryOutsideDirectory,
    /// Entries were duplicated or not strictly ordered.
    EntriesNotCanonical,
    /// An entry did not follow the exclusive cursor.
    EntryBeforeCursor,
    /// Truncation and the next-page cursor disagreed.
    InvalidNextCursor,
}

impl fmt::Display for WorkspaceDirectoryListingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEntries { actual } => write!(
                formatter,
                "directory listing has {actual} entries; maximum is {MAX_DIRECTORY_PAGE_ENTRIES}"
            ),
            Self::EntryOutsideDirectory => {
                formatter.write_str("directory listing contains a non-child entry")
            }
            Self::EntriesNotCanonical => formatter.write_str("directory listing is not canonical"),
            Self::EntryBeforeCursor => {
                formatter.write_str("directory listing did not advance beyond its cursor")
            }
            Self::InvalidNextCursor => {
                formatter.write_str("directory listing cursor and truncation disagree")
            }
        }
    }
}

impl Error for WorkspaceDirectoryListingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContentHash;

    fn revision(path: &[u8], hash: u8) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    #[test]
    fn request_and_listing_are_direct_child_bound_and_forward_only() -> Result<(), Box<dyn Error>> {
        let directory =
            WorkspaceDirectory::Subtree(RepositoryPath::try_from_bytes(b"src".to_vec())?);
        let request = WorkspaceDirectoryListRequest::new(
            WorktreeId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            directory.clone(),
            None,
            DirectoryPageSize::new(2)?,
        )?;
        let child_directory = WorkspaceDirectoryEntry::directory(
            RepositoryPath::try_from_bytes(b"src/nested".to_vec())?,
            revision(b"src/nested/mod.rs", 3)?,
        )?;
        let file = WorkspaceDirectoryEntry::file(revision(b"src/lib.rs", 4)?);
        let listing =
            WorkspaceDirectoryListing::new(&request, vec![file, child_directory], None, false)?;

        assert_eq!(listing.entries().len(), 2);
        assert!(directory.contains_direct_child(listing.entries()[0].path()));
        assert_eq!(
            listing.entries()[1].kind(),
            WorkspaceDirectoryEntryKind::Directory
        );
        assert_eq!(
            DirectoryPageSize::new(0),
            Err(DirectoryPageSizeError { value: 0 })
        );
        assert!(
            WorkspaceDirectoryListRequest::new(
                request.worktree_id(),
                request.snapshot_id(),
                directory,
                Some(RepositoryPath::try_from_bytes(b"other/file.rs".to_vec())?),
                DirectoryPageSize::new(1)?,
            )
            .is_err()
        );
        Ok(())
    }
}
