use crate::platform_path;
use crate::{PathEntryKind, PathPolicy, PathPolicyError};
use a3_application::{
    WorkspaceDirectoryLister, WorkspaceDirectoryListerFuture, WorkspaceDirectoryReadControl,
    WorkspaceDirectoryReadFailure,
};
use a3_domain::{
    DiscoveryPolicy, Progress, ProjectIdentity, PublishedIndex, RepositoryPath, WorkspaceDirectory,
    WorkspaceDirectoryEntry, WorkspaceDirectoryEntryKind, WorkspaceDirectoryListRequest,
    WorkspaceDirectoryListing,
};
use std::collections::BTreeMap;

/// Local adapter for snapshot-bound directory pages over the published ignore-filtered file set.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexedWorkspaceDirectoryLister;

impl WorkspaceDirectoryLister for IndexedWorkspaceDirectoryLister {
    fn list<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        request: &'a WorkspaceDirectoryListRequest,
        control: &'a dyn WorkspaceDirectoryReadControl,
    ) -> WorkspaceDirectoryListerFuture<'a> {
        Box::pin(async move { list_directory(project, published, request, control) })
    }
}

fn list_directory(
    project: &ProjectIdentity,
    published: &PublishedIndex,
    request: &WorkspaceDirectoryListRequest,
    control: &dyn WorkspaceDirectoryReadControl,
) -> Result<WorkspaceDirectoryListing, WorkspaceDirectoryReadFailure> {
    ensure_active(control)?;
    if request.worktree_id() != project.worktree().id()
        || request.snapshot_id() != published.run().snapshot_id()
    {
        return Err(WorkspaceDirectoryReadFailure::Denied);
    }
    validate_live_directory(project, request.directory())?;

    let files = published.publication().graph().files();
    report(control, Progress::Indeterminate)?;
    let total = u64::try_from(files.len().max(1))
        .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)?;
    let report_stride = files.len().div_ceil(64).max(1);
    let mut entries = BTreeMap::<RepositoryPath, WorkspaceDirectoryEntry>::new();
    for (index, revision) in files.iter().enumerate() {
        ensure_active(control)?;
        if DiscoveryPolicy::v1()
            .classify_built_in_path(revision.path().as_bytes(), false)
            .is_none()
            && let Some(component) = request.directory().direct_child_component(revision.path())
        {
            let entry = if request.directory().contains_direct_child(revision.path()) {
                WorkspaceDirectoryEntry::file(revision.clone())
            } else {
                let path = direct_child_path(request.directory(), component)?;
                WorkspaceDirectoryEntry::directory(path, revision.clone())
                    .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)?
            };
            match entries.get(entry.path()) {
                Some(existing)
                    if existing.kind() == WorkspaceDirectoryEntryKind::Directory
                        && entry.kind() == WorkspaceDirectoryEntryKind::Directory => {}
                Some(_) => return Err(WorkspaceDirectoryReadFailure::InvalidResult),
                None => {
                    entries.insert(entry.path().clone(), entry);
                }
            }
        }
        let completed = index.saturating_add(1);
        if completed == files.len() || completed % report_stride == 0 {
            report(
                control,
                Progress::determinate(
                    u64::try_from(completed)
                        .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)?,
                    total,
                )
                .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)?,
            )?;
        }
    }
    if files.is_empty() {
        report(
            control,
            Progress::determinate(1, 1)
                .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)?,
        )?;
    }

    let maximum = usize::from(request.page_size().get());
    let mut page = entries
        .into_values()
        .filter(|entry| request.after().is_none_or(|cursor| entry.path() > cursor))
        .take(maximum.saturating_add(1))
        .collect::<Vec<_>>();
    let truncated = page.len() > maximum;
    if truncated {
        page.truncate(maximum);
    }
    let next_after = truncated
        .then(|| page.last().map(|entry| entry.path().clone()))
        .flatten();
    WorkspaceDirectoryListing::new(request, page, next_after, truncated)
        .map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)
}

fn validate_live_directory(
    project: &ProjectIdentity,
    directory: &WorkspaceDirectory,
) -> Result<(), WorkspaceDirectoryReadFailure> {
    let policy = PathPolicy::from_selected_root(project.worktree().root().as_path())
        .map_err(|_| WorkspaceDirectoryReadFailure::Unavailable)?;
    let Some(path) = directory.path() else {
        return Ok(());
    };
    if DiscoveryPolicy::v1()
        .classify_built_in_path(path.as_bytes(), true)
        .is_some()
    {
        return Err(WorkspaceDirectoryReadFailure::Denied);
    }
    let relative =
        platform_path::repository_path(path).map_err(|_| WorkspaceDirectoryReadFailure::Denied)?;
    let resolved = policy.resolve_existing(relative).map_err(map_path_error)?;
    if resolved.kind() != PathEntryKind::Directory {
        return Err(WorkspaceDirectoryReadFailure::Unavailable);
    }
    Ok(())
}

fn direct_child_path(
    directory: &WorkspaceDirectory,
    component: &[u8],
) -> Result<RepositoryPath, WorkspaceDirectoryReadFailure> {
    let mut bytes = directory
        .path()
        .map_or_else(Vec::new, |path| path.as_bytes().to_vec());
    if !bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(component);
    RepositoryPath::try_from_bytes(bytes).map_err(|_| WorkspaceDirectoryReadFailure::InvalidResult)
}

fn ensure_active(
    control: &dyn WorkspaceDirectoryReadControl,
) -> Result<(), WorkspaceDirectoryReadFailure> {
    if control.is_cancelled() {
        return Err(WorkspaceDirectoryReadFailure::Cancelled);
    }
    Ok(())
}

fn report(
    control: &dyn WorkspaceDirectoryReadControl,
    progress: Progress,
) -> Result<(), WorkspaceDirectoryReadFailure> {
    control
        .report_progress(progress)
        .map_err(|_| WorkspaceDirectoryReadFailure::ProgressUnavailable)
}

fn map_path_error(error: PathPolicyError) -> WorkspaceDirectoryReadFailure {
    match error {
        PathPolicyError::OutsideRoot { .. } => WorkspaceDirectoryReadFailure::Denied,
        PathPolicyError::Canonicalize { .. }
        | PathPolicyError::Metadata { .. }
        | PathPolicyError::NotDirectory(_)
        | PathPolicyError::UnsupportedFileType(_)
        | PathPolicyError::InvalidCanonicalPath(_) => WorkspaceDirectoryReadFailure::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::direct_child_path;
    use a3_domain::{RepositoryPath, WorkspaceDirectory};

    #[test]
    fn direct_child_path_preserves_repository_separators() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = direct_child_path(&WorkspaceDirectory::Root, b"src")?;
        let subtree = direct_child_path(
            &WorkspaceDirectory::Subtree(RepositoryPath::try_from_bytes(b"src".to_vec())?),
            b"nested",
        )?;

        assert_eq!(root.as_bytes(), b"src");
        assert_eq!(subtree.as_bytes(), b"src/nested");
        Ok(())
    }
}
