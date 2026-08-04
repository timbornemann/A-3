use crate::path::{RepositoryPathObservation, observe_repository_path, open_regular_no_follow};
use a3_application::{
    RepositorySnapshotControl, RepositorySnapshotFailure, RepositorySnapshotPolicy,
};
use a3_domain::{ContentHash, DiscoveryResult, FileRevision, Progress, RepositoryFileState};
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::Path;

pub(crate) fn hash_discovery(
    root: &Path,
    discovery: &DiscoveryResult,
    policy: RepositorySnapshotPolicy,
    control: &dyn RepositorySnapshotControl,
) -> Result<RepositoryFileState, RepositorySnapshotFailure> {
    let total_bytes = discovery.files().iter().try_fold(0u64, |total, file| {
        if file.size_bytes() > policy.max_file_bytes() {
            return Err(RepositorySnapshotFailure::ResourceLimitExceeded);
        }
        total
            .checked_add(file.size_bytes())
            .filter(|next| *next <= policy.max_total_hash_bytes())
            .ok_or(RepositorySnapshotFailure::ResourceLimitExceeded)
    })?;
    let mut progress = HashProgress::new(total_bytes, control)?;
    let mut revisions = Vec::with_capacity(discovery.files().len());
    let mut buffer = vec![0u8; policy.read_buffer_bytes()];

    for discovered in discovery.files() {
        ensure_active(control)?;
        let observation = observe_repository_path(root, discovered.path())
            .map_err(|_| RepositorySnapshotFailure::Filesystem)?;
        let (path, observed_metadata) = match observation {
            RepositoryPathObservation::Present { path, metadata } => (path, metadata),
            RepositoryPathObservation::Missing | RepositoryPathObservation::SymbolicLink => {
                return Err(RepositorySnapshotFailure::WorktreeChanged);
            }
        };
        if !observed_metadata.is_file() || observed_metadata.len() != discovered.size_bytes() {
            return Err(RepositorySnapshotFailure::WorktreeChanged);
        }
        let content_hash = hash_file(
            &path,
            discovered.size_bytes(),
            policy,
            control,
            &mut progress,
            &mut buffer,
        )?;
        revisions.push(FileRevision::new(discovered.path().clone(), content_hash));
    }
    progress.finish()?;
    RepositoryFileState::new(revisions).map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)
}

fn hash_file(
    path: &Path,
    expected_size: u64,
    policy: RepositorySnapshotPolicy,
    control: &dyn RepositorySnapshotControl,
    progress: &mut HashProgress<'_>,
    buffer: &mut [u8],
) -> Result<ContentHash, RepositorySnapshotFailure> {
    let mut file = open_regular_no_follow(path).map_err(classify_open_error)?;
    let before = FileStability::from_file(&file)?;
    if before.len() != expected_size || before.len() > policy.max_file_bytes() {
        return Err(RepositorySnapshotFailure::WorktreeChanged);
    }

    let mut hasher = blake3::Hasher::new();
    let mut bytes_read = 0u64;
    loop {
        ensure_active(control)?;
        let count = file
            .read(buffer)
            .map_err(|_| RepositorySnapshotFailure::Filesystem)?;
        if count == 0 {
            break;
        }
        let count_u64 =
            u64::try_from(count).map_err(|_| RepositorySnapshotFailure::ResourceLimitExceeded)?;
        bytes_read = bytes_read
            .checked_add(count_u64)
            .filter(|total| *total <= policy.max_file_bytes())
            .ok_or(RepositorySnapshotFailure::ResourceLimitExceeded)?;
        hasher.update(&buffer[..count]);
        progress.advance(count_u64)?;
    }
    let after = FileStability::from_file(&file)?;
    if bytes_read != expected_size || before != after {
        return Err(RepositorySnapshotFailure::WorktreeChanged);
    }
    Ok(ContentHash::from_bytes(*hasher.finalize().as_bytes()))
}

fn classify_open_error(error: io::Error) -> RepositorySnapshotFailure {
    if matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::InvalidData
    ) {
        RepositorySnapshotFailure::WorktreeChanged
    } else {
        RepositorySnapshotFailure::Filesystem
    }
}

fn ensure_active(control: &dyn RepositorySnapshotControl) -> Result<(), RepositorySnapshotFailure> {
    if control.is_cancelled() {
        return Err(RepositorySnapshotFailure::Cancelled);
    }
    Ok(())
}

struct HashProgress<'a> {
    control: &'a dyn RepositorySnapshotControl,
    completed: u64,
    total: u64,
    stride: u64,
    next_report: u64,
}

impl<'a> HashProgress<'a> {
    fn new(
        content_bytes: u64,
        control: &'a dyn RepositorySnapshotControl,
    ) -> Result<Self, RepositorySnapshotFailure> {
        let total = content_bytes.max(1);
        report(
            control,
            Progress::determinate(0, total)
                .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?,
        )?;
        let stride = total.div_ceil(32).max(1);
        Ok(Self {
            control,
            completed: 0,
            total,
            stride,
            next_report: stride,
        })
    }

    fn advance(&mut self, bytes: u64) -> Result<(), RepositorySnapshotFailure> {
        self.completed = self
            .completed
            .checked_add(bytes)
            .filter(|completed| *completed <= self.total)
            .ok_or(RepositorySnapshotFailure::ResourceLimitExceeded)?;
        if self.completed >= self.next_report {
            report(
                self.control,
                Progress::determinate(self.completed, self.total)
                    .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?,
            )?;
            self.next_report = self
                .completed
                .checked_div(self.stride)
                .and_then(|quotient| quotient.checked_add(1))
                .and_then(|quotient| quotient.checked_mul(self.stride))
                .unwrap_or(self.total);
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RepositorySnapshotFailure> {
        if self.completed < self.total {
            self.completed = self.total;
            report(
                self.control,
                Progress::determinate(self.completed, self.total)
                    .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?,
            )?;
        }
        Ok(())
    }
}

fn report(
    control: &dyn RepositorySnapshotControl,
    progress: Progress,
) -> Result<(), RepositorySnapshotFailure> {
    control
        .report_progress(progress)
        .map_err(|_| RepositorySnapshotFailure::ProgressUnavailable)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStability {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    modified_seconds: i64,
    #[cfg(unix)]
    modified_nanoseconds: i64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    creation_time: u64,
    #[cfg(windows)]
    last_write_time: u64,
}

impl FileStability {
    fn from_file(file: &File) -> Result<Self, RepositorySnapshotFailure> {
        let metadata = file
            .metadata()
            .map_err(|_| RepositorySnapshotFailure::Filesystem)?;
        if !metadata.is_file() {
            return Err(RepositorySnapshotFailure::WorktreeChanged);
        }
        Ok(Self::from_metadata(&metadata))
    }

    fn len(&self) -> u64 {
        self.len
    }

    #[cfg(unix)]
    fn from_metadata(metadata: &Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }

    #[cfg(windows)]
    fn from_metadata(metadata: &Metadata) -> Self {
        use std::os::windows::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            attributes: metadata.file_attributes(),
            creation_time: metadata.creation_time(),
            last_write_time: metadata.last_write_time(),
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}
