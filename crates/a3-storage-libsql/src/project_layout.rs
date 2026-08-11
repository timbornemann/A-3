use crate::StorageLayout;
use a3_application::{ProjectStorageControl, ProjectStorageFailure, ProjectStorageUsage};
use a3_domain::{WorktreeId, WorktreeIdentity};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PROJECTS_DIRECTORY_NAME: &str = "projects";
const KNOWLEDGE_FILE_NAME: &str = "knowledge.db";
const STORAGE_INSPECTION_ENTRY_LIMIT: u32 = 100_000;
const STORAGE_INSPECTION_PROGRESS_INTERVAL: u32 = 256;
const STORAGE_INSPECTION_TIMEOUT: Duration = Duration::from_secs(2);

/// Canonical application-owned storage paths for exactly one worktree identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStorageLayout {
    root: PathBuf,
    knowledge: PathBuf,
    worktree_id: WorktreeId,
}

impl StorageLayout {
    /// Creates the private storage directory derived only from a validated worktree identity.
    pub fn prepare_project(
        &self,
        worktree: &WorktreeIdentity,
    ) -> Result<ProjectStorageLayout, ProjectStorageLayoutError> {
        if self.root().starts_with(worktree.root().as_path()) {
            return Err(ProjectStorageLayoutError::StorageInsideWorktree {
                storage_root: self.root().to_path_buf(),
                worktree_root: worktree.root().as_path().to_path_buf(),
            });
        }

        let projects = ensure_directory(
            self.root(),
            &self.root().join(PROJECTS_DIRECTORY_NAME),
            ProjectStorageEntry::ProjectsDirectory,
        )?;
        let root = ensure_directory(
            &projects,
            &projects.join(worktree.id().to_string()),
            ProjectStorageEntry::WorktreeDirectory,
        )?;
        let layout = ProjectStorageLayout {
            knowledge: root.join(KNOWLEDGE_FILE_NAME),
            root,
            worktree_id: worktree.id(),
        };
        layout.validate_knowledge_target()?;
        Ok(layout)
    }

    pub(crate) fn existing_project(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<Option<ProjectStorageLayout>, ProjectStorageLayoutError> {
        let Some(projects) = existing_directory(
            self.root(),
            &self.root().join(PROJECTS_DIRECTORY_NAME),
            ProjectStorageEntry::ProjectsDirectory,
        )?
        else {
            return Ok(None);
        };
        let Some(root) = existing_directory(
            &projects,
            &projects.join(worktree_id.to_string()),
            ProjectStorageEntry::WorktreeDirectory,
        )?
        else {
            return Ok(None);
        };
        let layout = ProjectStorageLayout {
            knowledge: root.join(KNOWLEDGE_FILE_NAME),
            root,
            worktree_id,
        };
        layout.validate_knowledge_target()?;
        Ok(Some(layout))
    }

    pub(crate) fn relocate_project(
        &self,
        source_worktree_id: WorktreeId,
        target_worktree: &WorktreeIdentity,
    ) -> Result<ProjectStorageLayout, ProjectStorageLayoutError> {
        if source_worktree_id == target_worktree.id() {
            return Err(ProjectStorageLayoutError::ReconciliationIdentityUnchanged);
        }
        if self.root().starts_with(target_worktree.root().as_path()) {
            return Err(ProjectStorageLayoutError::StorageInsideWorktree {
                storage_root: self.root().to_path_buf(),
                worktree_root: target_worktree.root().as_path().to_path_buf(),
            });
        }

        let projects = ensure_directory(
            self.root(),
            &self.root().join(PROJECTS_DIRECTORY_NAME),
            ProjectStorageEntry::ProjectsDirectory,
        )?;
        let source = self.existing_project(source_worktree_id)?;
        let target = self.existing_project(target_worktree.id())?;
        match (source, target) {
            (Some(source), None) => {
                let requested_target = projects.join(target_worktree.id().to_string());
                rename_no_replace(source.root(), &requested_target).map_err(|source_error| {
                    ProjectStorageLayoutError::Move {
                        source: source.root().to_path_buf(),
                        target: requested_target.clone(),
                        source_error,
                    }
                })?;
                self.existing_project(target_worktree.id())?.ok_or(
                    ProjectStorageLayoutError::ReconciliationSourceMissing(source_worktree_id),
                )
            }
            (None, Some(target)) => Ok(target),
            (Some(_), Some(_)) => Err(ProjectStorageLayoutError::ReconciliationTargetExists(
                target_worktree.id(),
            )),
            (None, None) => Err(ProjectStorageLayoutError::ReconciliationSourceMissing(
                source_worktree_id,
            )),
        }
    }
}

#[cfg(unix)]
fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE).map_err(io::Error::from)
}

#[cfg(windows)]
fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    // Windows' standard rename refuses an existing destination. Unix needs
    // renameat2/renameatx_np above because its basic rename may replace it.
    fs::rename(source, target)
}

#[cfg(not(any(unix, windows)))]
fn rename_no_replace(_source: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace directory rename is unsupported on this platform",
    ))
}

impl ProjectStorageLayout {
    /// Returns the canonical private directory for this worktree.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the validated worktree identity from which this layout was derived.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the only path at which this worktree's knowledge database may be opened.
    #[must_use]
    pub fn knowledge_path(&self) -> &Path {
        &self.knowledge
    }

    pub(crate) fn measure_usage(
        &self,
        control: &dyn ProjectStorageControl,
    ) -> Result<ProjectStorageUsage, ProjectStorageFailure> {
        let deadline = Instant::now()
            .checked_add(STORAGE_INSPECTION_TIMEOUT)
            .ok_or(ProjectStorageFailure::TimedOut)?;
        let mut pending = vec![self.root.clone()];
        let mut entries = 0_u32;
        let mut bytes = 0_u64;

        while let Some(directory) = pending.pop() {
            let children =
                fs::read_dir(&directory).map_err(|_| ProjectStorageFailure::Unavailable)?;
            for child in children {
                if control.is_cancelled() {
                    return Err(ProjectStorageFailure::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Err(ProjectStorageFailure::TimedOut);
                }
                entries = entries
                    .checked_add(1)
                    .ok_or(ProjectStorageFailure::TooManyEntries)?;
                if entries > STORAGE_INSPECTION_ENTRY_LIMIT {
                    return Err(ProjectStorageFailure::TooManyEntries);
                }

                let path = child
                    .map_err(|_| ProjectStorageFailure::Unavailable)?
                    .path();
                let metadata =
                    fs::symlink_metadata(&path).map_err(|_| ProjectStorageFailure::Unavailable)?;
                if metadata.file_type().is_symlink() {
                    return Err(ProjectStorageFailure::InvalidLayout);
                }
                let canonical =
                    fs::canonicalize(&path).map_err(|_| ProjectStorageFailure::Unavailable)?;
                if !canonical.starts_with(&self.root) {
                    return Err(ProjectStorageFailure::InvalidLayout);
                }
                if metadata.is_dir() {
                    pending.push(canonical);
                } else if metadata.is_file() {
                    bytes = bytes
                        .checked_add(metadata.len())
                        .ok_or(ProjectStorageFailure::SizeOverflow)?;
                } else {
                    return Err(ProjectStorageFailure::InvalidLayout);
                }

                if entries.is_multiple_of(STORAGE_INSPECTION_PROGRESS_INTERVAL) {
                    control
                        .report_entries(entries)
                        .map_err(|_| ProjectStorageFailure::ProgressUnavailable)?;
                }
            }
        }
        control
            .report_entries(entries)
            .map_err(|_| ProjectStorageFailure::ProgressUnavailable)?;
        Ok(ProjectStorageUsage::from_bytes(bytes))
    }

    pub(crate) fn validate_knowledge_target(&self) -> Result<(), ProjectStorageLayoutError> {
        let metadata = match fs::symlink_metadata(&self.knowledge) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(ProjectStorageLayoutError::Inspect {
                    entry: ProjectStorageEntry::KnowledgeDatabase,
                    path: self.knowledge.clone(),
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(ProjectStorageLayoutError::SymbolicLink {
                entry: ProjectStorageEntry::KnowledgeDatabase,
                path: self.knowledge.clone(),
            });
        }
        if !metadata.is_file() {
            return Err(ProjectStorageLayoutError::NotRegularFile {
                entry: ProjectStorageEntry::KnowledgeDatabase,
                path: self.knowledge.clone(),
            });
        }
        let canonical = fs::canonicalize(&self.knowledge).map_err(|source| {
            ProjectStorageLayoutError::Canonicalize {
                entry: ProjectStorageEntry::KnowledgeDatabase,
                path: self.knowledge.clone(),
                source,
            }
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(ProjectStorageLayoutError::OutsideParent {
                entry: ProjectStorageEntry::KnowledgeDatabase,
                canonical,
                parent: self.root.clone(),
            });
        }
        Ok(())
    }
}

fn ensure_directory(
    parent: &Path,
    requested: &Path,
    entry: ProjectStorageEntry,
) -> Result<PathBuf, ProjectStorageLayoutError> {
    match fs::symlink_metadata(requested) {
        Ok(metadata) => validate_directory_metadata(requested, entry, &metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(requested).map_err(|source| ProjectStorageLayoutError::Create {
                entry,
                path: requested.to_path_buf(),
                source,
            })?;
        }
        Err(source) => {
            return Err(ProjectStorageLayoutError::Inspect {
                entry,
                path: requested.to_path_buf(),
                source,
            });
        }
    }

    let metadata =
        fs::symlink_metadata(requested).map_err(|source| ProjectStorageLayoutError::Inspect {
            entry,
            path: requested.to_path_buf(),
            source,
        })?;
    validate_directory_metadata(requested, entry, &metadata)?;
    let canonical =
        fs::canonicalize(requested).map_err(|source| ProjectStorageLayoutError::Canonicalize {
            entry,
            path: requested.to_path_buf(),
            source,
        })?;
    if !canonical.starts_with(parent) {
        return Err(ProjectStorageLayoutError::OutsideParent {
            entry,
            canonical,
            parent: parent.to_path_buf(),
        });
    }
    Ok(canonical)
}

fn existing_directory(
    parent: &Path,
    requested: &Path,
    entry: ProjectStorageEntry,
) -> Result<Option<PathBuf>, ProjectStorageLayoutError> {
    let metadata = match fs::symlink_metadata(requested) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ProjectStorageLayoutError::Inspect {
                entry,
                path: requested.to_path_buf(),
                source,
            });
        }
    };
    validate_directory_metadata(requested, entry, &metadata)?;
    let canonical =
        fs::canonicalize(requested).map_err(|source| ProjectStorageLayoutError::Canonicalize {
            entry,
            path: requested.to_path_buf(),
            source,
        })?;
    if !canonical.starts_with(parent) {
        return Err(ProjectStorageLayoutError::OutsideParent {
            entry,
            canonical,
            parent: parent.to_path_buf(),
        });
    }
    Ok(Some(canonical))
}

fn validate_directory_metadata(
    path: &Path,
    entry: ProjectStorageEntry,
    metadata: &fs::Metadata,
) -> Result<(), ProjectStorageLayoutError> {
    if metadata.file_type().is_symlink() {
        return Err(ProjectStorageLayoutError::SymbolicLink {
            entry,
            path: path.to_path_buf(),
        });
    }
    if !metadata.is_dir() {
        return Err(ProjectStorageLayoutError::NotDirectory {
            entry,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Kind of application-owned project storage entry being validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStorageEntry {
    /// Shared directory containing one child per worktree identity.
    ProjectsDirectory,
    /// Private directory for one worktree identity.
    WorktreeDirectory,
    /// Per-worktree libSQL database file.
    KnowledgeDatabase,
}

impl fmt::Display for ProjectStorageEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectsDirectory => formatter.write_str("projects directory"),
            Self::WorktreeDirectory => formatter.write_str("worktree storage directory"),
            Self::KnowledgeDatabase => formatter.write_str("knowledge database"),
        }
    }
}

/// Failure while establishing the private per-worktree storage boundary.
#[derive(Debug)]
pub enum ProjectStorageLayoutError {
    /// The supplied app-data root would place runtime data inside the selected worktree.
    StorageInsideWorktree {
        /// Canonical application storage root.
        storage_root: PathBuf,
        /// Canonical selected worktree root.
        worktree_root: PathBuf,
    },
    /// A required private directory could not be created.
    Create {
        /// Entry being created.
        entry: ProjectStorageEntry,
        /// Requested path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// Metadata for a storage entry could not be inspected.
    Inspect {
        /// Entry being inspected.
        entry: ProjectStorageEntry,
        /// Requested path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// A storage entry was a symbolic link.
    SymbolicLink {
        /// Entry being validated.
        entry: ProjectStorageEntry,
        /// Rejected path.
        path: PathBuf,
    },
    /// A required directory path resolved to another entry type.
    NotDirectory {
        /// Entry being validated.
        entry: ProjectStorageEntry,
        /// Rejected path.
        path: PathBuf,
    },
    /// The knowledge database path resolved to a non-file entry.
    NotRegularFile {
        /// Entry being validated.
        entry: ProjectStorageEntry,
        /// Rejected path.
        path: PathBuf,
    },
    /// A storage entry could not be canonicalized.
    Canonicalize {
        /// Entry being canonicalized.
        entry: ProjectStorageEntry,
        /// Requested path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// A storage entry resolved outside its validated parent directory.
    OutsideParent {
        /// Entry being validated.
        entry: ProjectStorageEntry,
        /// Canonical path outside the expected parent.
        canonical: PathBuf,
        /// Canonical expected parent.
        parent: PathBuf,
    },
    /// Source and target IDs were identical, so no move could be proven.
    ReconciliationIdentityUnchanged,
    /// The confirmed source directory no longer existed in private app storage.
    ReconciliationSourceMissing(WorktreeId),
    /// The target identity already owned a directory and must never be overwritten.
    ReconciliationTargetExists(WorktreeId),
    /// The operating system could not atomically rename the private project directory.
    Move {
        /// Exact validated source directory.
        source: PathBuf,
        /// Exact target child under the same private parent.
        target: PathBuf,
        /// Operating-system rename failure.
        source_error: io::Error,
    },
}

impl fmt::Display for ProjectStorageLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageInsideWorktree { .. } => {
                formatter.write_str("application runtime storage must not be inside the worktree")
            }
            Self::Create { entry, .. } => write!(formatter, "could not create {entry}"),
            Self::Inspect { entry, .. } => write!(formatter, "could not inspect {entry}"),
            Self::SymbolicLink { entry, .. } => {
                write!(formatter, "{entry} must not be a symbolic link")
            }
            Self::NotDirectory { entry, .. } => write!(formatter, "{entry} is not a directory"),
            Self::NotRegularFile { entry, .. } => {
                write!(formatter, "{entry} is not a regular file")
            }
            Self::Canonicalize { entry, .. } => {
                write!(formatter, "could not canonicalize {entry}")
            }
            Self::OutsideParent { entry, .. } => {
                write!(formatter, "{entry} resolved outside its private parent")
            }
            Self::ReconciliationIdentityUnchanged => {
                formatter.write_str("reconciliation source and target identities are equal")
            }
            Self::ReconciliationSourceMissing(_) => {
                formatter.write_str("reconciliation source storage is missing")
            }
            Self::ReconciliationTargetExists(_) => {
                formatter.write_str("reconciliation target storage already exists")
            }
            Self::Move { .. } => formatter.write_str("could not move private worktree storage"),
        }
    }
}

impl Error for ProjectStorageLayoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Create { source, .. }
            | Self::Inspect { source, .. }
            | Self::Canonicalize { source, .. } => Some(source),
            Self::Move { source_error, .. } => Some(source_error),
            Self::StorageInsideWorktree { .. }
            | Self::SymbolicLink { .. }
            | Self::NotDirectory { .. }
            | Self::NotRegularFile { .. }
            | Self::OutsideParent { .. }
            | Self::ReconciliationIdentityUnchanged
            | Self::ReconciliationSourceMissing(_)
            | Self::ReconciliationTargetExists(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectStorageLayout;
    use a3_application::{
        ProjectStorageControl, ProjectStorageControlError, ProjectStorageFailure,
    };
    use a3_domain::WorktreeId;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug)]
    struct RecordingControl {
        cancelled: AtomicBool,
        reports: Mutex<Vec<u32>>,
    }

    impl RecordingControl {
        fn active() -> Self {
            Self {
                cancelled: AtomicBool::new(false),
                reports: Mutex::new(Vec::new()),
            }
        }

        fn cancelled() -> Self {
            Self {
                cancelled: AtomicBool::new(true),
                reports: Mutex::new(Vec::new()),
            }
        }

        fn reports(&self) -> MutexGuard<'_, Vec<u32>> {
            match self.reports.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            }
        }
    }

    impl ProjectStorageControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Acquire)
        }

        fn report_entries(&self, entries: u32) -> Result<(), ProjectStorageControlError> {
            self.reports().push(entries);
            Ok(())
        }
    }

    #[test]
    fn storage_usage_counts_only_validated_private_regular_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let artifacts = directory.path().join("artifacts");
        fs::create_dir(&artifacts)?;
        fs::write(directory.path().join("knowledge.db"), b"1234")?;
        fs::write(artifacts.join("card.bin"), b"567")?;
        let layout = layout(&directory);
        let control = RecordingControl::active();

        let usage = layout.measure_usage(&control)?;

        assert_eq!(usage.bytes(), 7);
        assert_eq!(control.reports().last(), Some(&3));
        Ok(())
    }

    #[test]
    fn storage_usage_honors_cancellation_before_reading_an_entry()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        fs::write(directory.path().join("knowledge.db"), b"content")?;

        assert_eq!(
            layout(&directory).measure_usage(&RecordingControl::cancelled()),
            Err(ProjectStorageFailure::Cancelled)
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn storage_usage_rejects_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new()?;
        let outside = directory
            .path()
            .parent()
            .ok_or_else(|| std::io::Error::other("test directory unexpectedly has no parent"))?;
        symlink(outside, directory.path().join("escape"))?;

        assert_eq!(
            layout(&directory).measure_usage(&RecordingControl::active()),
            Err(ProjectStorageFailure::InvalidLayout)
        );
        Ok(())
    }

    fn layout(directory: &TestDirectory) -> ProjectStorageLayout {
        ProjectStorageLayout {
            root: directory.path().to_path_buf(),
            knowledge: directory.path().join("knowledge.db"),
            worktree_id: WorktreeId::from_bytes([7; 32]),
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Result<Self, std::io::Error> {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "a3-project-storage-usage-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path.canonicalize()?))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _cleanup = fs::remove_dir_all(&self.0);
        }
    }
}
