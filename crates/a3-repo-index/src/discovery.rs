use crate::classification::{classify_path, classify_prefix, roles_for_path};
use crate::config::{ProjectConfigurationError, ProjectIgnore, load_project_ignore};
use crate::path::{RepositoryPathObservation, observe_repository_path, open_regular_no_follow};
use crate::repository::{RepositoryValidationError, open_validated};
use a3_application::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryFailure,
};
use a3_domain::{
    DiscoveredFile, DiscoveryExclusionCounts, DiscoveryExclusionReason, DiscoveryOrigin,
    DiscoveryPolicy, DiscoveryResult, Progress, ProjectIdentity, RepositoryPath,
};
use gix::bstr::BStr;
use gix::dir::entry::Status;
use gix::ignore::glob::pattern::Case;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Local Git-backed discovery adapter.
#[derive(Debug, Clone)]
pub struct GitRepositoryDiscoverer {
    prefix_reader: Arc<dyn PrefixReader>,
}

impl GitRepositoryDiscoverer {
    /// Creates a discoverer with no ambient configuration or network capability.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefix_reader: Arc::new(LocalPrefixReader),
        }
    }
}

impl Default for GitRepositoryDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryDiscoverer for GitRepositoryDiscoverer {
    fn discover(
        &self,
        project: &ProjectIdentity,
        policy: DiscoveryPolicy,
        control: &dyn RepositoryDiscoveryControl,
    ) -> Result<DiscoveryResult, RepositoryDiscoveryFailure> {
        ensure_active(control)?;
        report(control, Progress::Indeterminate)?;

        let root = project.worktree().root().as_path();
        let repository = open_validated(project).map_err(map_repository_error)?;
        let project_ignore = load_project_ignore(root, policy).map_err(classify_config_error)?;
        let filesystem = repository
            .filesystem_options()
            .map_err(|_| RepositoryDiscoveryFailure::InvalidRepository)?;
        let case = if filesystem.ignore_case {
            Case::Fold
        } else {
            Case::Sensitive
        };
        let index = repository
            .index_or_empty()
            .map_err(|_| RepositoryDiscoveryFailure::InvalidRepository)?;

        let mut candidates = CandidateSet::new(policy);
        for entry in index.entries() {
            ensure_active(control)?;
            candidates.insert(entry.path(&index).as_ref(), DiscoveryOrigin::Tracked)?;
        }

        let interrupted = AtomicBool::new(false);
        let mut collector = UntrackedCollector {
            candidates: &mut candidates,
            project_ignore: &project_ignore,
            case,
            control,
            interrupted: &interrupted,
            exclusions: DiscoveryExclusionCounts::default(),
            failure: None,
        };
        let options = repository
            .dirwalk_options()
            .map_err(|_| RepositoryDiscoveryFailure::InvalidRepository)?
            .emit_tracked(false)
            .emit_ignored(None)
            .emit_untracked(gix::dir::walk::EmissionMode::Matching)
            .recurse_repositories(false);
        let walk_result = repository.dirwalk(
            &index,
            std::iter::empty::<&BStr>(),
            &interrupted,
            options,
            &mut collector,
        );
        if let Some(failure) = collector.failure {
            return Err(failure);
        }
        walk_result.map_err(|_| {
            if control.is_cancelled() {
                RepositoryDiscoveryFailure::Cancelled
            } else {
                RepositoryDiscoveryFailure::Filesystem
            }
        })?;

        ensure_active(control)?;
        let mut exclusions = collector.exclusions;
        let total = candidates.len();
        let progress_total = u64::try_from(total.max(1))
            .map_err(|_| RepositoryDiscoveryFailure::ResourceLimitExceeded)?;
        report(
            control,
            Progress::determinate(0, progress_total)
                .map_err(|_| RepositoryDiscoveryFailure::InvalidResult)?,
        )?;
        let report_stride = total.div_ceil(32).max(1);
        let mut files = Vec::with_capacity(total);

        for (index, (path_bytes, origin)) in candidates.into_iter().enumerate() {
            ensure_active(control)?;
            classify_candidate(
                root,
                path_bytes,
                origin,
                policy,
                &project_ignore,
                case,
                self.prefix_reader.as_ref(),
                &mut exclusions,
                &mut files,
            )?;
            let completed = index.saturating_add(1);
            if completed == total || completed % report_stride == 0 {
                report(
                    control,
                    Progress::determinate(
                        u64::try_from(completed)
                            .map_err(|_| RepositoryDiscoveryFailure::ResourceLimitExceeded)?,
                        progress_total,
                    )
                    .map_err(|_| RepositoryDiscoveryFailure::InvalidResult)?,
                )?;
            }
        }
        if total == 0 {
            report(
                control,
                Progress::determinate(1, 1)
                    .map_err(|_| RepositoryDiscoveryFailure::InvalidResult)?,
            )?;
        }

        DiscoveryResult::new(project.worktree().id(), policy.version(), files, exclusions)
            .map_err(|_| RepositoryDiscoveryFailure::InvalidResult)
    }
}

fn map_repository_error(error: RepositoryValidationError) -> RepositoryDiscoveryFailure {
    match error {
        RepositoryValidationError::RootUnavailable => RepositoryDiscoveryFailure::RootUnavailable,
        RepositoryValidationError::InvalidRepository => {
            RepositoryDiscoveryFailure::InvalidRepository
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn classify_candidate(
    root: &Path,
    path_bytes: Vec<u8>,
    origin: DiscoveryOrigin,
    policy: DiscoveryPolicy,
    project_ignore: &ProjectIgnore,
    case: Case,
    prefix_reader: &dyn PrefixReader,
    exclusions: &mut DiscoveryExclusionCounts,
    files: &mut Vec<DiscoveredFile>,
) -> Result<(), RepositoryDiscoveryFailure> {
    let roles = roles_for_path(&path_bytes);
    let repository_path = RepositoryPath::try_from_bytes(path_bytes)
        .map_err(|_| RepositoryDiscoveryFailure::InvalidPath)?;
    let observation = observe_repository_path(root, &repository_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidInput {
            RepositoryDiscoveryFailure::InvalidPath
        } else {
            RepositoryDiscoveryFailure::Filesystem
        }
    })?;
    let (path, metadata) = match observation {
        RepositoryPathObservation::Missing => return Ok(()),
        RepositoryPathObservation::SymbolicLink => {
            exclusions.record(DiscoveryExclusionReason::SymbolicLink);
            return Ok(());
        }
        RepositoryPathObservation::Present { path, metadata } => (path, metadata),
    };
    let is_dir = metadata.is_dir();
    if let Some(reason) = classify_path(repository_path.as_bytes(), is_dir, project_ignore, case) {
        exclusions.record(reason);
        return Ok(());
    }
    if !metadata.is_file() {
        exclusions.record(DiscoveryExclusionReason::SpecialFile);
        return Ok(());
    }
    if metadata.len() > policy.max_file_bytes() {
        exclusions.record(DiscoveryExclusionReason::TooLarge);
        return Ok(());
    }

    let prefix = prefix_reader
        .read_prefix(&path, policy.inspection_prefix_bytes(), metadata.len())
        .map_err(|_| RepositoryDiscoveryFailure::Filesystem)?;
    if let Some(reason) = classify_prefix(&prefix) {
        exclusions.record(reason);
        return Ok(());
    }

    files.push(DiscoveredFile::new(
        repository_path,
        origin,
        metadata.len(),
        roles,
    ));
    Ok(())
}

trait PrefixReader: fmt::Debug + Send + Sync {
    fn read_prefix(&self, path: &Path, limit: usize, observed_size: u64) -> io::Result<Vec<u8>>;
}

#[derive(Debug)]
struct LocalPrefixReader;

impl PrefixReader for LocalPrefixReader {
    fn read_prefix(&self, path: &Path, limit: usize, observed_size: u64) -> io::Result<Vec<u8>> {
        let file = open_regular_no_follow(path)?;
        let read_limit = u64::try_from(limit).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "prefix limit cannot fit u64")
        })?;
        let capacity = usize::try_from(observed_size)
            .unwrap_or(usize::MAX)
            .min(limit);
        let mut prefix = Vec::with_capacity(capacity);
        file.take(read_limit).read_to_end(&mut prefix)?;
        Ok(prefix)
    }
}

fn ensure_active(
    control: &dyn RepositoryDiscoveryControl,
) -> Result<(), RepositoryDiscoveryFailure> {
    if control.is_cancelled() {
        return Err(RepositoryDiscoveryFailure::Cancelled);
    }
    Ok(())
}

fn report(
    control: &dyn RepositoryDiscoveryControl,
    progress: Progress,
) -> Result<(), RepositoryDiscoveryFailure> {
    control
        .report_progress(progress)
        .map_err(|_| RepositoryDiscoveryFailure::ProgressUnavailable)
}

fn classify_config_error(error: ProjectConfigurationError) -> RepositoryDiscoveryFailure {
    match error {
        ProjectConfigurationError::Invalid => RepositoryDiscoveryFailure::InvalidConfiguration,
        ProjectConfigurationError::Io => RepositoryDiscoveryFailure::Filesystem,
    }
}

#[derive(Debug)]
struct CandidateSet {
    entries: BTreeMap<Vec<u8>, DiscoveryOrigin>,
    total_path_bytes: usize,
    policy: DiscoveryPolicy,
}

impl CandidateSet {
    fn new(policy: DiscoveryPolicy) -> Self {
        Self {
            entries: BTreeMap::new(),
            total_path_bytes: 0,
            policy,
        }
    }

    fn insert(
        &mut self,
        path: &[u8],
        origin: DiscoveryOrigin,
    ) -> Result<(), RepositoryDiscoveryFailure> {
        if path.is_empty() {
            return Err(RepositoryDiscoveryFailure::InvalidPath);
        }
        if let Some(existing) = self.entries.get_mut(path) {
            if origin == DiscoveryOrigin::Tracked {
                *existing = origin;
            }
            return Ok(());
        }
        if self.entries.len() >= self.policy.max_candidates() {
            return Err(RepositoryDiscoveryFailure::ResourceLimitExceeded);
        }
        let next_total = self
            .total_path_bytes
            .checked_add(path.len())
            .ok_or(RepositoryDiscoveryFailure::ResourceLimitExceeded)?;
        if next_total > self.policy.max_total_path_bytes() {
            return Err(RepositoryDiscoveryFailure::ResourceLimitExceeded);
        }
        RepositoryPath::try_from_bytes(path.to_vec())
            .map_err(|_| RepositoryDiscoveryFailure::InvalidPath)?;
        self.entries.insert(path.to_vec(), origin);
        self.total_path_bytes = next_total;
        Ok(())
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn into_iter(self) -> impl Iterator<Item = (Vec<u8>, DiscoveryOrigin)> {
        self.entries.into_iter()
    }
}

struct UntrackedCollector<'a> {
    candidates: &'a mut CandidateSet,
    project_ignore: &'a ProjectIgnore,
    case: Case,
    control: &'a dyn RepositoryDiscoveryControl,
    interrupted: &'a AtomicBool,
    exclusions: DiscoveryExclusionCounts,
    failure: Option<RepositoryDiscoveryFailure>,
}

impl gix::dir::walk::Delegate for UntrackedCollector<'_> {
    fn emit(
        &mut self,
        entry: gix::dir::EntryRef<'_>,
        _collapsed_directory_status: Option<Status>,
    ) -> gix::dir::walk::Action {
        if self.control.is_cancelled() {
            self.interrupted.store(true, Ordering::Release);
            self.failure = Some(RepositoryDiscoveryFailure::Cancelled);
            return ControlFlow::Break(());
        }
        if entry.status != Status::Untracked {
            return ControlFlow::Continue(());
        }
        let path: &[u8] = entry.rela_path.as_ref().as_ref();
        let is_dir = entry.disk_kind.is_some_and(|kind| kind.is_dir());
        if let Some(reason) = classify_path(path, is_dir, self.project_ignore, self.case) {
            self.exclusions.record(reason);
            return ControlFlow::Continue(());
        }
        if is_dir {
            self.exclusions
                .record(DiscoveryExclusionReason::SpecialFile);
            return ControlFlow::Continue(());
        }
        if let Err(failure) = self.candidates.insert(path, DiscoveryOrigin::Untracked) {
            self.interrupted.store(true, Ordering::Release);
            self.failure = Some(failure);
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    fn can_recurse(
        &mut self,
        entry: gix::dir::EntryRef<'_>,
        for_deletion: Option<gix::dir::walk::ForDeletionMode>,
        worktree_root_is_repository: bool,
    ) -> bool {
        if self.control.is_cancelled() {
            self.interrupted.store(true, Ordering::Release);
            self.failure = Some(RepositoryDiscoveryFailure::Cancelled);
            return false;
        }
        let path: &[u8] = entry.rela_path.as_ref().as_ref();
        if classify_path(path, true, self.project_ignore, self.case).is_some() {
            return false;
        }
        entry.status.can_recurse(
            entry.disk_kind,
            entry.pathspec_match,
            for_deletion,
            worktree_root_is_repository,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{PrefixReader, classify_candidate};
    use crate::config::ProjectIgnore;
    use a3_domain::{
        DiscoveredFile, DiscoveryExclusionCounts, DiscoveryExclusionReason, DiscoveryOrigin,
        DiscoveryPolicy,
    };
    use gix::ignore::glob::pattern::Case;
    use std::error::Error;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug, Default)]
    struct RecordingReader {
        calls: AtomicUsize,
        maximum_requested: AtomicUsize,
    }

    impl PrefixReader for RecordingReader {
        fn read_prefix(
            &self,
            _path: &Path,
            limit: usize,
            _observed_size: u64,
        ) -> io::Result<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.maximum_requested.fetch_max(limit, Ordering::Relaxed);
            Ok(b"bounded text".to_vec())
        }
    }

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "a3-discovery-reader-{}-{}",
                std::process::id(),
                NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            fs::create_dir(&path)?;
            Ok(Self(fs::canonicalize(path)?))
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ignored = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn oversized_files_are_not_opened_and_prefix_requests_are_bounded() -> Result<(), Box<dyn Error>>
    {
        let root = TempDirectory::new()?;
        let policy = DiscoveryPolicy::v1();
        fs::File::create(root.0.join("large.txt"))?
            .set_len(policy.max_file_bytes().saturating_add(1))?;
        fs::write(root.0.join("normal.txt"), b"normal")?;
        let reader = RecordingReader::default();
        let mut exclusions = DiscoveryExclusionCounts::default();
        let mut files = Vec::<DiscoveredFile>::new();

        classify_candidate(
            &root.0,
            b"large.txt".to_vec(),
            DiscoveryOrigin::Untracked,
            policy,
            &ProjectIgnore::default(),
            Case::Sensitive,
            &reader,
            &mut exclusions,
            &mut files,
        )?;
        assert_eq!(reader.calls.load(Ordering::Relaxed), 0);
        assert_eq!(exclusions.get(DiscoveryExclusionReason::TooLarge), 1);

        classify_candidate(
            &root.0,
            b"normal.txt".to_vec(),
            DiscoveryOrigin::Untracked,
            policy,
            &ProjectIgnore::default(),
            Case::Sensitive,
            &reader,
            &mut exclusions,
            &mut files,
        )?;
        assert_eq!(reader.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            reader.maximum_requested.load(Ordering::Relaxed),
            policy.inspection_prefix_bytes()
        );
        assert_eq!(files.len(), 1);
        Ok(())
    }
}
