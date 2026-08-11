use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{ContentHash, IndexRunId, Progress, ProjectIdentity, RepositoryPath, SnapshotId};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_PAGE_SIZE: u16 = 100;
const DEFAULT_PAGE_SIZE: u16 = 50;
const MAX_CHILD_NAME_BYTES: usize = 4_096;
const MAX_DISPLAY_CHARACTERS: usize = 256;

/// Positive bounded number of direct repository children returned by one read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryTreePageSize(u16);

impl RepositoryTreePageSize {
    /// Product default chosen for progressive desktop rendering.
    pub const DEFAULT: Self = Self(DEFAULT_PAGE_SIZE);

    /// Accepts one through one hundred direct children per page.
    pub fn new(value: u16) -> Result<Self, RepositoryTreePageSizeError> {
        if value == 0 || value > MAX_PAGE_SIZE {
            return Err(RepositoryTreePageSizeError);
        }
        Ok(Self(value))
    }

    /// Returns the validated SQL and IPC limit.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A repository-tree page size was zero or exceeded the fixed UI bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryTreePageSizeError;

impl fmt::Display for RepositoryTreePageSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository-tree page size must be between one and one hundred")
    }
}

impl Error for RepositoryTreePageSizeError {}

/// Lossless direct-child segment used only as a stable page cursor.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepositoryTreeChildName(Vec<u8>);

impl RepositoryTreeChildName {
    /// Validates one non-empty repository segment without path separators.
    pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, RepositoryTreeChildNameError> {
        if bytes.is_empty()
            || bytes.len() > MAX_CHILD_NAME_BYTES
            || bytes.iter().any(|byte| matches!(byte, 0 | b'/'))
        {
            return Err(RepositoryTreeChildNameError);
        }
        Ok(Self(bytes))
    }

    /// Returns the lossless segment bytes retained by persistence and cursors.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for RepositoryTreeChildName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepositoryTreeChildName")
            .field("byte_length", &self.0.len())
            .finish()
    }
}

/// A child cursor was empty, too long, or contained a path separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryTreeChildNameError;

impl fmt::Display for RepositoryTreeChildNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository-tree child name is invalid")
    }
}

impl Error for RepositoryTreeChildNameError {}

/// Bounded sanitized label that is never accepted back as an authoritative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTreeDisplayName {
    value: String,
    truncated: bool,
}

impl RepositoryTreeDisplayName {
    fn from_child_name(name: &RepositoryTreeChildName) -> Self {
        let source = String::from_utf8_lossy(name.as_bytes());
        let mut characters = source.chars();
        let value = characters
            .by_ref()
            .take(MAX_DISPLAY_CHARACTERS)
            .map(|character| {
                if character.is_control() {
                    '\u{fffd}'
                } else {
                    character
                }
            })
            .collect();
        Self {
            value,
            truncated: characters.next().is_some(),
        }
    }

    /// Returns safe UI text derived from, but not interchangeable with, the path segment.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns whether the display omitted characters beyond the fixed bound.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Structural kind of one direct child in the published repository tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTreeEntryKind {
    /// A derived directory prefix with one or more indexed descendants.
    Directory,
    /// One exact file revision from the current publication.
    File,
}

/// One direct child projected from exact file-revision paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTreeEntry {
    path: RepositoryPath,
    child_name: RepositoryTreeChildName,
    display_name: RepositoryTreeDisplayName,
    kind: RepositoryTreeEntryKind,
    descendant_file_count: u64,
    content_hash: Option<ContentHash>,
}

impl RepositoryTreeEntry {
    /// Constructs a directory or file entry while keeping file evidence exact.
    pub fn new(
        path: RepositoryPath,
        child_name: RepositoryTreeChildName,
        kind: RepositoryTreeEntryKind,
        descendant_file_count: u64,
        content_hash: Option<ContentHash>,
    ) -> Result<Self, RepositoryTreeEntryError> {
        let valid_shape = match kind {
            RepositoryTreeEntryKind::Directory => {
                descendant_file_count > 0 && content_hash.is_none()
            }
            RepositoryTreeEntryKind::File => descendant_file_count == 1 && content_hash.is_some(),
        };
        if !valid_shape {
            return Err(RepositoryTreeEntryError);
        }
        let display_name = RepositoryTreeDisplayName::from_child_name(&child_name);
        Ok(Self {
            path,
            child_name,
            display_name,
            kind,
            descendant_file_count,
            content_hash,
        })
    }

    /// Returns the full lossless repository-relative path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns the raw direct-child key used for stable ordering and pagination.
    #[must_use]
    pub const fn child_name(&self) -> &RepositoryTreeChildName {
        &self.child_name
    }

    /// Returns bounded safe display text for the direct child.
    #[must_use]
    pub const fn display_name(&self) -> &RepositoryTreeDisplayName {
        &self.display_name
    }

    /// Returns whether this entry is a derived directory or exact file revision.
    #[must_use]
    pub const fn kind(&self) -> RepositoryTreeEntryKind {
        self.kind
    }

    /// Returns the exact number of indexed files below this direct child.
    #[must_use]
    pub const fn descendant_file_count(&self) -> u64 {
        self.descendant_file_count
    }

    /// Returns file evidence for files and no synthetic hash for directories.
    #[must_use]
    pub const fn content_hash(&self) -> Option<ContentHash> {
        self.content_hash
    }
}

/// A tree entry paired a kind with contradictory count or evidence data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryTreeEntryError;

impl fmt::Display for RepositoryTreeEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository-tree entry shape is invalid")
    }
}

impl Error for RepositoryTreeEntryError {}

/// Validated root or directory page request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTreeQuery {
    directory: Option<RepositoryPath>,
    after: Option<RepositoryTreeChildName>,
    page_size: RepositoryTreePageSize,
}

impl RepositoryTreeQuery {
    /// Creates a bounded query for the root or one indexed directory.
    #[must_use]
    pub const fn new(
        directory: Option<RepositoryPath>,
        after: Option<RepositoryTreeChildName>,
        page_size: RepositoryTreePageSize,
    ) -> Self {
        Self {
            directory,
            after,
            page_size,
        }
    }

    /// Returns the directory to enumerate, or None for the repository root.
    #[must_use]
    pub const fn directory(&self) -> Option<&RepositoryPath> {
        self.directory.as_ref()
    }

    /// Returns the exclusive raw child cursor.
    #[must_use]
    pub const fn after(&self) -> Option<&RepositoryTreeChildName> {
        self.after.as_ref()
    }

    /// Returns the fixed maximum number of entries delivered to the UI.
    #[must_use]
    pub const fn page_size(&self) -> RepositoryTreePageSize {
        self.page_size
    }
}

/// One canonical, bounded repository-tree page from an atomic publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTreePage {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    directory: Option<RepositoryPath>,
    entries: Vec<RepositoryTreeEntry>,
    next_cursor: Option<RepositoryTreeChildName>,
}

impl RepositoryTreePage {
    /// Validates direct-child paths, stable order, bounds, and next-cursor shape.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        directory: Option<RepositoryPath>,
        entries: Vec<RepositoryTreeEntry>,
        next_cursor: Option<RepositoryTreeChildName>,
    ) -> Result<Self, RepositoryTreePageError> {
        if entries.len() > usize::from(MAX_PAGE_SIZE) {
            return Err(RepositoryTreePageError::TooManyEntries);
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].child_name().as_bytes() >= pair[1].child_name().as_bytes())
        {
            return Err(RepositoryTreePageError::UnstableEntryOrder);
        }
        for entry in &entries {
            if !is_direct_child(directory.as_ref(), entry.path(), entry.child_name()) {
                return Err(RepositoryTreePageError::InvalidChildPath);
            }
        }
        match (entries.last(), next_cursor.as_ref()) {
            (Some(last), Some(cursor)) if last.child_name() == cursor => {}
            (_, None) => {}
            _ => return Err(RepositoryTreePageError::InvalidNextCursor),
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            directory,
            entries,
            next_cursor,
        })
    }

    /// Returns the exact published index run read by the adapter transaction.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the exact immutable snapshot read by the adapter transaction.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the enumerated directory, or None for repository root.
    #[must_use]
    pub const fn directory(&self) -> Option<&RepositoryPath> {
        self.directory.as_ref()
    }

    /// Returns at most one hundred direct children in lossless byte order.
    #[must_use]
    pub fn entries(&self) -> &[RepositoryTreeEntry] {
        &self.entries
    }

    /// Returns the last delivered child when another page exists.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&RepositoryTreeChildName> {
        self.next_cursor.as_ref()
    }
}

fn is_direct_child(
    directory: Option<&RepositoryPath>,
    path: &RepositoryPath,
    child_name: &RepositoryTreeChildName,
) -> bool {
    let bytes = path.as_bytes();
    match directory {
        None => bytes == child_name.as_bytes(),
        Some(directory) => {
            let parent = directory.as_bytes();
            bytes.len() == parent.len() + 1 + child_name.as_bytes().len()
                && bytes.starts_with(parent)
                && bytes.get(parent.len()) == Some(&b'/')
                && bytes.get(parent.len() + 1..) == Some(child_name.as_bytes())
        }
    }
}

/// Persisted rows contradicted the canonical page contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTreePageError {
    /// More than one hundred children crossed the application boundary.
    TooManyEntries,
    /// Child names were duplicated or not in strict byte order.
    UnstableEntryOrder,
    /// A projected full path was not a direct child of the requested directory.
    InvalidChildPath,
    /// A next cursor was present without matching the last delivered child.
    InvalidNextCursor,
}

impl fmt::Display for RepositoryTreePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyEntries => "repository-tree page exceeds its fixed entry bound",
            Self::UnstableEntryOrder => "repository-tree entries are not canonically ordered",
            Self::InvalidChildPath => "repository-tree entry is not a direct child",
            Self::InvalidNextCursor => "repository-tree next cursor is invalid",
        })
    }
}

impl Error for RepositoryTreePageError {}

/// Cooperative cancellation and deterministic progress for one tree read.
pub trait RepositoryTreeControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports the start and completion phases to the owning runtime.
    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryTreeControlError>;
}

impl RepositoryTreeControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryTreeControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| RepositoryTreeControlError::Unavailable)
    }
}

/// Repository-tree progress could not reach its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTreeControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for RepositoryTreeControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository-tree progress is unavailable")
    }
}

impl Error for RepositoryTreeControlError {}

/// Owned future returned by the object-safe repository-tree port.
pub type RepositoryTreeFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Option<RepositoryTreePage>, RepositoryTreeFailure>> + Send + 'a>,
>;

/// Narrow read-only capability for one published directory page.
pub trait RepositoryTreeStore: fmt::Debug + Send + Sync {
    /// Loads a bounded page without reading the worktree filesystem.
    fn load_repository_tree_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a RepositoryTreeQuery,
        control: &'a dyn RepositoryTreeControl,
    ) -> RepositoryTreeFuture<'a>;
}

/// Stable content-free failure classes for repository-tree reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTreeFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the page or published-index contract.
    InvalidStoredProjection,
    /// The requested directory is not present in the current publication.
    DirectoryUnavailable,
    /// The owner cancelled before a result was delivered.
    Cancelled,
    /// The bounded adapter query exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for RepositoryTreeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "repository-tree storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored repository-tree projection is invalid")
            }
            Self::DirectoryUnavailable => {
                formatter.write_str("repository-tree directory is unavailable")
            }
            Self::Cancelled => formatter.write_str("repository-tree read was cancelled"),
            Self::TimedOut => formatter.write_str("repository-tree read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("repository-tree progress is unavailable")
            }
        }
    }
}

impl Error for RepositoryTreeFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::DirectoryUnavailable
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

/// Application use case retaining cancellation and progress outside persistence.
#[derive(Debug)]
pub struct GetRepositoryTreePage {
    store: Arc<dyn RepositoryTreeStore>,
}

impl GetRepositoryTreePage {
    /// Wires the narrow progressive repository-tree capability.
    #[must_use]
    pub fn new(store: Arc<dyn RepositoryTreeStore>) -> Self {
        Self { store }
    }

    /// Reads one current page or None when no index has been published.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &RepositoryTreeQuery,
        control: &dyn RepositoryTreeControl,
    ) -> Result<Option<RepositoryTreePage>, RepositoryTreeFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(RepositoryTreeFailure::Cancelled);
        }
        let page = self
            .store
            .load_repository_tree_page(project, query, control)
            .await?;
        if control.is_cancelled() {
            return Err(RepositoryTreeFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(page)
    }
}

fn report(
    control: &dyn RepositoryTreeControl,
    completed: u64,
) -> Result<(), RepositoryTreeFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| RepositoryTreeFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| RepositoryTreeFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        GetRepositoryTreePage, RepositoryTreeChildName, RepositoryTreeControl,
        RepositoryTreeControlError, RepositoryTreeEntry, RepositoryTreeEntryKind,
        RepositoryTreeFailure, RepositoryTreeFuture, RepositoryTreePage, RepositoryTreePageSize,
        RepositoryTreeQuery, RepositoryTreeStore,
    };
    use a3_domain::{
        CanonicalDirectory, ContentHash, GitHead, GitReferenceName, IndexRunId, Progress,
        ProjectIdentity, RepositoryId, RepositoryIdentity, RepositoryPath, SnapshotId,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingStore;

    impl RepositoryTreeStore for RecordingStore {
        fn load_repository_tree_page<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a RepositoryTreeQuery,
            _control: &'a dyn RepositoryTreeControl,
        ) -> RepositoryTreeFuture<'a> {
            Box::pin(async move {
                let name = RepositoryTreeChildName::try_from_bytes(b"src".to_vec())
                    .map_err(|_| RepositoryTreeFailure::InvalidStoredProjection)?;
                let entry = RepositoryTreeEntry::new(
                    RepositoryPath::try_from_bytes(b"src".to_vec())
                        .map_err(|_| RepositoryTreeFailure::InvalidStoredProjection)?,
                    name.clone(),
                    RepositoryTreeEntryKind::Directory,
                    2,
                    None,
                )
                .map_err(|_| RepositoryTreeFailure::InvalidStoredProjection)?;
                RepositoryTreePage::new(
                    IndexRunId::from_bytes([7; 32]),
                    SnapshotId::from_bytes([8; 32]),
                    query.directory().cloned(),
                    vec![entry],
                    Some(name),
                )
                .map(Some)
                .map_err(|_| RepositoryTreeFailure::InvalidStoredProjection)
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        progress: Mutex<Vec<Progress>>,
        cancelled: bool,
        unavailable: bool,
    }

    impl RepositoryTreeControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(&self, progress: Progress) -> Result<(), RepositoryTreeControlError> {
            if self.unavailable {
                return Err(RepositoryTreeControlError::Unavailable);
            }
            self.progress
                .lock()
                .map_err(|_| RepositoryTreeControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn page_rejects_non_direct_children_and_contradictory_file_evidence()
    -> Result<(), Box<dyn Error>> {
        let name = RepositoryTreeChildName::try_from_bytes(b"nested".to_vec())?;
        let invalid_file = RepositoryTreeEntry::new(
            RepositoryPath::try_from_bytes(b"nested".to_vec())?,
            name.clone(),
            RepositoryTreeEntryKind::File,
            1,
            None,
        );
        assert!(invalid_file.is_err());

        let entry = RepositoryTreeEntry::new(
            RepositoryPath::try_from_bytes(b"parent/other".to_vec())?,
            name,
            RepositoryTreeEntryKind::Directory,
            1,
            None,
        )?;
        let page = RepositoryTreePage::new(
            IndexRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            Some(RepositoryPath::try_from_bytes(b"parent".to_vec())?),
            vec![entry],
            None,
        );
        assert!(page.is_err());
        Ok(())
    }

    #[test]
    fn display_is_bounded_and_lossless_cursor_remains_separate() -> Result<(), Box<dyn Error>> {
        let mut bytes = vec![b'a'; 300];
        bytes[2] = b'\n';
        bytes[3] = 0xff;
        let name = RepositoryTreeChildName::try_from_bytes(bytes.clone())?;
        let entry = RepositoryTreeEntry::new(
            RepositoryPath::try_from_bytes(bytes.clone())?,
            name,
            RepositoryTreeEntryKind::File,
            1,
            Some(ContentHash::from_bytes([3; 32])),
        )?;

        assert_eq!(entry.child_name().as_bytes(), bytes);
        assert_eq!(entry.display_name().as_str().chars().count(), 256);
        assert!(entry.display_name().is_truncated());
        assert!(!entry.display_name().as_str().chars().any(char::is_control));
        Ok(())
    }

    #[test]
    fn use_case_reports_bounded_progress_and_honors_cancellation() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let query = RepositoryTreeQuery::new(None, None, RepositoryTreePageSize::DEFAULT);
        let control = RecordingControl::default();
        let page = block_on(
            GetRepositoryTreePage::new(Arc::new(RecordingStore))
                .execute(&project, &query, &control),
        )?
        .ok_or_else(|| std::io::Error::other("expected repository-tree page"))?;
        assert_eq!(page.entries().len(), 1);
        let progress = control
            .progress
            .lock()
            .map_err(|_| std::io::Error::other("progress lock poisoned"))?;
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].completed(), Some(0));
        assert_eq!(progress[1].completed(), Some(2));
        drop(progress);

        let cancelled = RecordingControl {
            cancelled: true,
            ..RecordingControl::default()
        };
        assert_eq!(
            block_on(
                GetRepositoryTreePage::new(Arc::new(RecordingStore))
                    .execute(&project, &query, &cancelled)
            ),
            Err(RepositoryTreeFailure::Cancelled)
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([3; 32]),
            WorktreeAnchorId::from_bytes([4; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
