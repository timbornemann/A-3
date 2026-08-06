use crate::platform_path;
use crate::secure_file::{SecureFileReadError, read_verified_text};
use crate::{PathEntryKind, PathPolicy};
use a3_application::{
    AuthorizedPatchAction, PatchApplyFailure, PatchApplyFuture, PatchPreviewFailure,
    PatchPreviewFuture, WorkspacePatchControl, WorkspacePatchTool,
};
use a3_domain::{
    FileRevision, PatchAction, PatchChange, PatchChangeSet, PatchContentPreview, PatchFileContent,
    PatchOperation, PatchPreview, PatchPreviewEntry, PolicyDecisionId, Progress, ProjectIdentity,
    PublishedIndex, RepositoryPath, WorktreeId,
};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_CONTENT_PREVIEW_BYTES: usize = 16 * 1_024;
const MAX_TOTAL_PREVIEW_BYTES: usize = 64 * 1_024;
const TEMPORARY_NAME_ATTEMPTS: u8 = 32;

/// Safe local full-file patch adapter with one in-process mutation lease per worktree.
#[derive(Debug, Default)]
pub struct WorkspacePatchAdapter {
    active_mutations: Mutex<BTreeSet<WorktreeId>>,
}

impl WorkspacePatchAdapter {
    /// Creates an adapter with no active worktree mutations.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active_mutations: Mutex::new(BTreeSet::new()),
        }
    }

    fn preview_sync(
        &self,
        project: &ProjectIdentity,
        published: &PublishedIndex,
        action: &PatchAction,
        control: &dyn WorkspacePatchControl,
    ) -> Result<PatchPreview, PatchPreviewFailure> {
        control
            .report_progress(Progress::Indeterminate)
            .map_err(|_| PatchPreviewFailure::ProgressUnavailable)?;
        let live = preflight(project, published, action, control).map_err(preview_failure)?;
        let entries = build_preview_entries(action, &live)?;
        let preview =
            PatchPreview::new(action, entries).map_err(|_| PatchPreviewFailure::InvalidResult)?;
        let total = u64::try_from(action.operations().len())
            .map_err(|_| PatchPreviewFailure::InvalidResult)?;
        control
            .report_progress(
                Progress::determinate(total, total)
                    .map_err(|_| PatchPreviewFailure::InvalidResult)?,
            )
            .map_err(|_| PatchPreviewFailure::ProgressUnavailable)?;
        Ok(preview)
    }

    fn apply_sync(
        &self,
        project: &ProjectIdentity,
        published: &PublishedIndex,
        authorized: AuthorizedPatchAction,
        control: &dyn WorkspacePatchControl,
    ) -> Result<PatchChangeSet, PatchApplyFailure> {
        let (action, policy_decision_id) = authorized.into_parts();
        let _lease = self.acquire_lease(action.worktree_id())?;
        let total = u64::try_from(action.operations().len())
            .map_err(|_| PatchApplyFailure::InvalidResult)?;
        control
            .report_progress(
                Progress::determinate(0, total).map_err(|_| PatchApplyFailure::InvalidResult)?,
            )
            .map_err(|_| PatchApplyFailure::ProgressUnavailable)?;
        let first_live = preflight(project, published, &action, control).map_err(apply_failure)?;
        let root = project.worktree().root().as_path();
        let mut staged = StagedFiles::create(&action, &first_live, control)?;

        // Staging can take time. Revalidate every expected hash and absent target immediately
        // before the first visible mutation instead of trusting the preview or first preflight.
        let live = preflight(project, published, &action, control).map_err(apply_failure)?;
        if control.is_cancelled() {
            return Err(PatchApplyFailure::Cancelled);
        }

        let mut changes = Vec::with_capacity(action.operations().len());
        for (index, (operation, live_operation)) in
            action.operations().iter().zip(&live).enumerate()
        {
            if control.is_cancelled() {
                return Err(changed_or(
                    &action,
                    policy_decision_id,
                    changes,
                    PatchApplyFailure::Cancelled,
                ));
            }
            let mutation = apply_operation(index, operation, live_operation, &mut staged);
            let change = match mutation {
                Ok(change) => change,
                Err(failure) => {
                    return Err(changed_or(&action, policy_decision_id, changes, failure));
                }
            };
            changes.push(change);

            // A successful filesystem primitive is the mutation evidence. This additional live
            // observation detects an immediate foreign edit and prevents a false success.
            if verify_change(root, changes.last(), control).is_err() {
                return Err(changed_or(
                    &action,
                    policy_decision_id,
                    changes,
                    PatchApplyFailure::Conflict,
                ));
            }
            let completed = u64::try_from(index.saturating_add(1))
                .map_err(|_| PatchApplyFailure::InvalidResult)?;
            let progress = Progress::determinate(completed, total)
                .map_err(|_| PatchApplyFailure::InvalidResult)?;
            if control.report_progress(progress).is_err() {
                return Err(changed_or(
                    &action,
                    policy_decision_id,
                    changes,
                    PatchApplyFailure::ProgressUnavailable,
                ));
            }
        }

        PatchChangeSet::new(&action, policy_decision_id, changes)
            .map_err(|_| PatchApplyFailure::InvalidResult)
    }

    fn acquire_lease(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<MutationLease<'_>, PatchApplyFailure> {
        let mut active = self
            .active_mutations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !active.insert(worktree_id) {
            return Err(PatchApplyFailure::Busy);
        }
        Ok(MutationLease {
            adapter: self,
            worktree_id,
        })
    }
}

impl WorkspacePatchTool for WorkspacePatchAdapter {
    fn preview<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        action: &'a PatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchPreviewFuture<'a> {
        Box::pin(async move { self.preview_sync(project, published, action, control) })
    }

    fn apply<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        authorized: AuthorizedPatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchApplyFuture<'a> {
        Box::pin(async move { self.apply_sync(project, published, authorized, control) })
    }
}

struct MutationLease<'a> {
    adapter: &'a WorkspacePatchAdapter,
    worktree_id: WorktreeId,
}

impl Drop for MutationLease<'_> {
    fn drop(&mut self) {
        let mut active = self
            .adapter
            .active_mutations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        active.remove(&self.worktree_id);
    }
}

#[derive(Debug)]
struct LiveOperation {
    source: Option<PathBuf>,
    target: Option<PathBuf>,
    before: Option<PatchFileContent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreflightFailure {
    Denied,
    StaleSnapshot,
    Conflict,
    Cancelled,
    Unavailable,
    InvalidResult,
}

fn preflight(
    project: &ProjectIdentity,
    published: &PublishedIndex,
    action: &PatchAction,
    control: &dyn WorkspacePatchControl,
) -> Result<Vec<LiveOperation>, PreflightFailure> {
    if project.worktree().id() != action.worktree_id() {
        return Err(PreflightFailure::Denied);
    }
    if published.run().snapshot_id() != action.snapshot_id() {
        return Err(PreflightFailure::StaleSnapshot);
    }
    let root = project.worktree().root().as_path();
    let policy = PathPolicy::from_selected_root(root).map_err(|_| PreflightFailure::Denied)?;
    let revisions = published.publication().graph().files();
    let mut live = Vec::with_capacity(action.operations().len());

    for operation in action.operations() {
        if control.is_cancelled() {
            return Err(PreflightFailure::Cancelled);
        }
        let prepared = match operation {
            PatchOperation::Add(add) => {
                require_published_absence(revisions, add.path())?;
                let target = resolve_absent_target(root, &policy, add.path())?;
                LiveOperation {
                    source: None,
                    target: Some(target),
                    before: None,
                }
            }
            PatchOperation::Update(update) => {
                require_published_revision(revisions, update.expected())?;
                let source = resolve_existing_file(root, &policy, update.expected().path())?;
                let before = read_before(root, update.expected(), control)?;
                LiveOperation {
                    source: Some(source.clone()),
                    target: Some(source),
                    before: Some(before),
                }
            }
            PatchOperation::Move(movement) => {
                require_published_revision(revisions, movement.expected())?;
                require_published_absence(revisions, movement.destination())?;
                let source = resolve_existing_file(root, &policy, movement.expected().path())?;
                let target = resolve_absent_target(root, &policy, movement.destination())?;
                let before = read_before(root, movement.expected(), control)?;
                LiveOperation {
                    source: Some(source),
                    target: Some(target),
                    before: Some(before),
                }
            }
            PatchOperation::Delete(expected) => {
                require_published_revision(revisions, expected)?;
                let source = resolve_existing_file(root, &policy, expected.path())?;
                let before = read_before(root, expected, control)?;
                LiveOperation {
                    source: Some(source),
                    target: None,
                    before: Some(before),
                }
            }
        };
        live.push(prepared);
    }
    Ok(live)
}

fn require_published_revision(
    revisions: &[FileRevision],
    expected: &FileRevision,
) -> Result<(), PreflightFailure> {
    let position = revisions
        .binary_search_by(|revision| revision.path().cmp(expected.path()))
        .map_err(|_| PreflightFailure::Conflict)?;
    if &revisions[position] != expected {
        return Err(PreflightFailure::Conflict);
    }
    Ok(())
}

fn require_published_absence(
    revisions: &[FileRevision],
    path: &RepositoryPath,
) -> Result<(), PreflightFailure> {
    if revisions
        .binary_search_by(|revision| revision.path().cmp(path))
        .is_ok()
    {
        return Err(PreflightFailure::Conflict);
    }
    Ok(())
}

fn read_before(
    root: &Path,
    expected: &FileRevision,
    control: &dyn WorkspacePatchControl,
) -> Result<PatchFileContent, PreflightFailure> {
    let bytes = read_verified_text(root, expected, || control.is_cancelled())
        .map_err(map_secure_read_failure)?;
    PatchFileContent::try_from_bytes(bytes).map_err(|_| PreflightFailure::InvalidResult)
}

fn resolve_existing_file(
    root: &Path,
    policy: &PathPolicy,
    path: &RepositoryPath,
) -> Result<PathBuf, PreflightFailure> {
    let relative = platform_path::repository_path(path).map_err(|_| PreflightFailure::Denied)?;
    ensure_no_link_components(root, &relative, false)?;
    let canonical = policy
        .resolve_existing(&relative)
        .map_err(|_| PreflightFailure::Denied)?;
    if canonical.kind() != PathEntryKind::File {
        return Err(PreflightFailure::Conflict);
    }
    Ok(canonical.as_path().to_path_buf())
}

fn resolve_absent_target(
    root: &Path,
    policy: &PathPolicy,
    path: &RepositoryPath,
) -> Result<PathBuf, PreflightFailure> {
    let relative = platform_path::repository_path(path).map_err(|_| PreflightFailure::Denied)?;
    ensure_no_link_components(root, &relative, true)?;
    let file_name = relative.file_name().ok_or(PreflightFailure::Denied)?;
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent = if parent_relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        let canonical = policy
            .resolve_existing(parent_relative)
            .map_err(|_| PreflightFailure::Denied)?;
        if canonical.kind() != PathEntryKind::Directory {
            return Err(PreflightFailure::Denied);
        }
        canonical.as_path().to_path_buf()
    };
    if !parent.starts_with(policy.root().as_path()) {
        return Err(PreflightFailure::Denied);
    }
    let target = parent.join(file_name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) => {
            if is_link_or_reparse(&metadata) {
                Err(PreflightFailure::Denied)
            } else {
                Err(PreflightFailure::Conflict)
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(target),
        Err(_) => Err(PreflightFailure::Unavailable),
    }
}

fn ensure_no_link_components(
    root: &Path,
    relative: &Path,
    allow_missing_final: bool,
) -> Result<(), PreflightFailure> {
    let count = relative.components().count();
    let mut current = root.to_path_buf();
    for (index, component) in relative.components().enumerate() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_reparse(&metadata) => {
                return Err(PreflightFailure::Denied);
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && allow_missing_final
                    && index.saturating_add(1) == count =>
            {
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PreflightFailure::Denied);
            }
            Err(_) => return Err(PreflightFailure::Unavailable),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(any(unix, windows)))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn build_preview_entries(
    action: &PatchAction,
    live: &[LiveOperation],
) -> Result<Vec<PatchPreviewEntry>, PatchPreviewFailure> {
    let mut remaining = MAX_TOTAL_PREVIEW_BYTES;
    action
        .operations()
        .iter()
        .zip(live)
        .map(|(operation, live_operation)| {
            let before = live_operation
                .before
                .as_ref()
                .map(|content| content_preview(content, &mut remaining))
                .transpose()?;
            let after_content = match operation {
                PatchOperation::Add(add) => Some(add.content()),
                PatchOperation::Update(update) => Some(update.content()),
                PatchOperation::Move(_) => live_operation.before.as_ref(),
                PatchOperation::Delete(_) => None,
            };
            let after = after_content
                .map(|content| content_preview(content, &mut remaining))
                .transpose()?;
            let (source_path, target_path) = match operation {
                PatchOperation::Add(add) => (None, Some(add.path().clone())),
                PatchOperation::Update(update) => (
                    Some(update.expected().path().clone()),
                    Some(update.expected().path().clone()),
                ),
                PatchOperation::Move(movement) => (
                    Some(movement.expected().path().clone()),
                    Some(movement.destination().clone()),
                ),
                PatchOperation::Delete(expected) => (Some(expected.path().clone()), None),
            };
            Ok(PatchPreviewEntry::new(
                source_path,
                target_path,
                before,
                after,
            ))
        })
        .collect()
}

fn content_preview(
    content: &PatchFileContent,
    remaining: &mut usize,
) -> Result<PatchContentPreview, PatchPreviewFailure> {
    let maximum = (*remaining).min(MAX_CONTENT_PREVIEW_BYTES);
    let preview = PatchContentPreview::from_content(content, maximum)
        .map_err(|_| PatchPreviewFailure::InvalidResult)?;
    *remaining = remaining.saturating_sub(preview.bytes().len());
    Ok(preview)
}

#[derive(Debug)]
struct StagedFiles {
    paths: Vec<Option<PathBuf>>,
}

impl StagedFiles {
    fn create(
        action: &PatchAction,
        live: &[LiveOperation],
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, PatchApplyFailure> {
        let mut staged = Self {
            paths: vec![None; action.operations().len()],
        };
        for (index, (operation, live_operation)) in action.operations().iter().zip(live).enumerate()
        {
            if control.is_cancelled() {
                return Err(PatchApplyFailure::Cancelled);
            }
            let Some(content) = operation.new_content() else {
                continue;
            };
            let target = live_operation
                .target
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            let permissions = match operation {
                PatchOperation::Update(_) => Some(
                    fs::metadata(
                        live_operation
                            .source
                            .as_deref()
                            .ok_or(PatchApplyFailure::InvalidResult)?,
                    )
                    .map_err(|_| PatchApplyFailure::Unavailable)?
                    .permissions(),
                ),
                PatchOperation::Add(_) => None,
                PatchOperation::Move(_) | PatchOperation::Delete(_) => {
                    return Err(PatchApplyFailure::InvalidResult);
                }
            };
            let path = stage_file(action, index, target, content, permissions)?;
            staged.paths[index] = Some(path);
        }
        Ok(staged)
    }

    fn path(&self, index: usize) -> Result<&Path, PatchApplyFailure> {
        self.paths
            .get(index)
            .and_then(Option::as_deref)
            .ok_or(PatchApplyFailure::InvalidResult)
    }

    fn consumed(&mut self, index: usize) {
        if let Some(path) = self.paths.get_mut(index) {
            *path = None;
        }
    }
}

impl Drop for StagedFiles {
    fn drop(&mut self) {
        for path in self.paths.iter().flatten() {
            let _ignored = fs::remove_file(path);
        }
    }
}

fn stage_file(
    action: &PatchAction,
    operation_index: usize,
    target: &Path,
    content: &PatchFileContent,
    permissions: Option<Permissions>,
) -> Result<PathBuf, PatchApplyFailure> {
    let parent = target.parent().ok_or(PatchApplyFailure::Denied)?;
    for attempt in 0..TEMPORARY_NAME_ATTEMPTS {
        let temporary = parent.join(temporary_name(action, operation_index, attempt));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(PatchApplyFailure::Unavailable),
        };
        let result = (|| {
            file.write_all(content.as_bytes())?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.sync_all()
        })();
        drop(file);
        if result.is_err() {
            let _ignored = fs::remove_file(&temporary);
            return Err(PatchApplyFailure::Unavailable);
        }
        return Ok(temporary);
    }
    Err(PatchApplyFailure::Unavailable)
}

fn temporary_name(action: &PatchAction, operation_index: usize, attempt: u8) -> String {
    let digest = action.digest().as_bytes();
    let mut prefix = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ignored = write!(prefix, "{byte:02x}");
    }
    format!(".a3-patch-{prefix}-{operation_index}-{attempt}.tmp")
}

fn apply_operation(
    index: usize,
    operation: &PatchOperation,
    live: &LiveOperation,
    staged: &mut StagedFiles,
) -> Result<PatchChange, PatchApplyFailure> {
    match operation {
        PatchOperation::Add(add) => {
            let target = live
                .target
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            if install_no_replace(staged.path(index)?, target).map_err(mutation_failure)? {
                staged.consumed(index);
            }
            Ok(PatchChange::Added(FileRevision::new(
                add.path().clone(),
                add.content().content_hash(),
            )))
        }
        PatchOperation::Update(update) => {
            let target = live
                .target
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            fs::rename(staged.path(index)?, target).map_err(mutation_failure)?;
            staged.consumed(index);
            Ok(PatchChange::Updated {
                previous: update.expected().clone(),
                current: FileRevision::new(
                    update.expected().path().clone(),
                    update.content().content_hash(),
                ),
            })
        }
        PatchOperation::Move(movement) => {
            let source = live
                .source
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            let target = live
                .target
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            move_no_replace(source, target).map_err(mutation_failure)?;
            Ok(PatchChange::Moved {
                previous: movement.expected().clone(),
                current: FileRevision::new(
                    movement.destination().clone(),
                    movement.expected().content_hash(),
                ),
            })
        }
        PatchOperation::Delete(expected) => {
            let source = live
                .source
                .as_deref()
                .ok_or(PatchApplyFailure::InvalidResult)?;
            fs::remove_file(source).map_err(mutation_failure)?;
            Ok(PatchChange::Deleted(expected.clone()))
        }
    }
}

#[cfg(unix)]
fn install_no_replace(source: &Path, target: &Path) -> io::Result<bool> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE)?;
    Ok(true)
}

#[cfg(not(unix))]
fn install_no_replace(source: &Path, target: &Path) -> io::Result<bool> {
    // Safe std has no cross-platform rename-no-replace primitive. A hard link reserves an absent
    // destination atomically and therefore cannot overwrite a concurrent user-created file.
    fs::hard_link(source, target)?;
    Ok(fs::remove_file(source).is_ok())
}

#[cfg(unix)]
fn move_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    install_no_replace(source, target).map(|_| ())
}

#[cfg(not(unix))]
fn move_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    // Preserve the no-overwrite invariant even where std rename replaces an existing file.
    fs::hard_link(source, target)?;
    if let Err(error) = fs::remove_file(source) {
        let _ignored = fs::remove_file(target);
        return Err(error);
    }
    Ok(())
}

fn verify_change(
    root: &Path,
    change: Option<&PatchChange>,
    control: &dyn WorkspacePatchControl,
) -> Result<(), ()> {
    let change = change.ok_or(())?;
    match change {
        PatchChange::Added(current) | PatchChange::Updated { current, .. } => {
            read_verified_text(root, current, || control.is_cancelled())
                .map(|_| ())
                .map_err(|_| ())
        }
        PatchChange::Moved { previous, current } => {
            require_absent(root, previous.path())?;
            read_verified_text(root, current, || control.is_cancelled())
                .map(|_| ())
                .map_err(|_| ())
        }
        PatchChange::Deleted(previous) => require_absent(root, previous.path()),
    }
}

fn require_absent(root: &Path, path: &RepositoryPath) -> Result<(), ()> {
    let relative = platform_path::repository_path(path).map_err(|_| ())?;
    match fs::symlink_metadata(root.join(relative)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) | Err(_) => Err(()),
    }
}

fn changed_or(
    action: &PatchAction,
    policy_decision_id: PolicyDecisionId,
    changes: Vec<PatchChange>,
    unchanged_failure: PatchApplyFailure,
) -> PatchApplyFailure {
    if changes.is_empty() {
        return unchanged_failure;
    }
    let result = if changes.len() == action.operations().len() {
        PatchChangeSet::new(action, policy_decision_id, changes)
    } else {
        PatchChangeSet::partial(action, policy_decision_id, changes)
    };
    result.map_or(PatchApplyFailure::InvalidResult, |change_set| {
        PatchApplyFailure::Changed(Box::new(change_set))
    })
}

fn mutation_failure(error: io::Error) -> PatchApplyFailure {
    match error.kind() {
        io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound => PatchApplyFailure::Conflict,
        io::ErrorKind::PermissionDenied => PatchApplyFailure::Denied,
        _ => PatchApplyFailure::Unavailable,
    }
}

fn map_secure_read_failure(failure: SecureFileReadError) -> PreflightFailure {
    match failure {
        SecureFileReadError::Denied
        | SecureFileReadError::InvalidEncoding
        | SecureFileReadError::Binary
        | SecureFileReadError::SecretCandidate => PreflightFailure::Denied,
        SecureFileReadError::Stale => PreflightFailure::Conflict,
        SecureFileReadError::Cancelled => PreflightFailure::Cancelled,
        SecureFileReadError::Unavailable | SecureFileReadError::TooLarge => {
            PreflightFailure::Unavailable
        }
    }
}

fn preview_failure(failure: PreflightFailure) -> PatchPreviewFailure {
    match failure {
        PreflightFailure::Denied => PatchPreviewFailure::Denied,
        PreflightFailure::StaleSnapshot => PatchPreviewFailure::StaleSnapshot,
        PreflightFailure::Conflict => PatchPreviewFailure::Conflict,
        PreflightFailure::Cancelled => PatchPreviewFailure::Cancelled,
        PreflightFailure::Unavailable => PatchPreviewFailure::Unavailable,
        PreflightFailure::InvalidResult => PatchPreviewFailure::InvalidResult,
    }
}

fn apply_failure(failure: PreflightFailure) -> PatchApplyFailure {
    match failure {
        PreflightFailure::Denied => PatchApplyFailure::Denied,
        PreflightFailure::StaleSnapshot => PatchApplyFailure::StaleSnapshot,
        PreflightFailure::Conflict => PatchApplyFailure::Conflict,
        PreflightFailure::Cancelled => PatchApplyFailure::Cancelled,
        PreflightFailure::Unavailable => PatchApplyFailure::Unavailable,
        PreflightFailure::InvalidResult => PatchApplyFailure::InvalidResult,
    }
}
