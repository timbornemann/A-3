use crate::{PathPolicy, path_policy::open_regular_no_follow};
use a3_application::{
    AgentPermissionStore, AuthorizedMachineFileAction, MachineFileControl, MachineFileReceipt,
    MachineFileTool, MachineFileToolFailure as Failure, MachineFileToolFuture,
    PreparedMachineFileAction,
};
use a3_domain::{
    DiscoveryPolicy, MachineFileAction, MachineFileOperation, PatchFileContent, PolicyAction,
    PolicyDecisionReason, PolicyResourceId, ProjectIdentity, RepositoryPath,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const MAX_BYTES: usize = 64 * 1024;

/// Complete-file machine adapter owned by the composition root; never exposed to the WebView.
#[derive(Debug)]
pub struct WorkspaceMachineFileTool {
    permissions: Arc<dyn AgentPermissionStore>,
    serial: Mutex<()>,
}
impl WorkspaceMachineFileTool {
    /// Injects the sole durable source of app-wide machine authority.
    #[must_use]
    pub fn new(permissions: Arc<dyn AgentPermissionStore>) -> Self {
        Self {
            permissions,
            serial: Mutex::new(()),
        }
    }

    fn prepare_sync(
        &self,
        project: &ProjectIdentity,
        action: &MachineFileAction,
        control: &dyn MachineFileControl,
    ) -> Result<PreparedMachineFileAction, Failure> {
        cancelled(control)?;
        let path = Path::new(action.path().as_str());
        if !path.is_absolute() {
            return Err(Failure::Denied);
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Failure::Denied)?;
        let file = RepositoryPath::try_from_bytes(name.as_bytes().to_vec())
            .map_err(|_| Failure::Denied)?;
        let root = PathPolicy::from_selected_root(path.parent().ok_or(Failure::Denied)?)
            .map_err(|_| Failure::Denied)?;
        let target = root.root().as_path().join(name);
        // Reject special/secret/generated resources by their complete canonical path. Metadata
        // preparation deliberately does not open or hash a file before user authorization.
        let canonical_display = target.to_str().ok_or(Failure::Denied)?.replace('\\', "/");
        if DiscoveryPolicy::v1()
            .classify_built_in_path(canonical_display.as_bytes(), false)
            .is_some()
        {
            return Err(Failure::Denied);
        }
        validate_target(
            root.root().as_path(),
            &target,
            matches!(
                action.operation(),
                MachineFileOperation::Write { expected: None, .. }
            ) || (matches!(action.operation(), MachineFileOperation::Read(None))
                && matches!(fs::symlink_metadata(&target), Err(error) if error.kind() == std::io::ErrorKind::NotFound)),
        )?;
        if target.starts_with(project.worktree().root().as_path()) {
            return Err(Failure::Denied);
        }
        let mut hash = blake3::Hasher::new_derive_key("a3.machine-file-resource.v1");
        hash.update(target.as_os_str().as_encoded_bytes());
        PreparedMachineFileAction::new(
            project,
            action.clone(),
            root.root().clone(),
            file,
            PolicyResourceId::from_bytes(*hash.finalize().as_bytes()),
        )
    }

    fn execute_sync(
        &self,
        authorized: &AuthorizedMachineFileAction,
        control: &dyn MachineFileControl,
    ) -> Result<MachineFileReceipt, Failure> {
        let _serial = self.serial.try_lock().map_err(|_| Failure::Unavailable)?;
        cancelled(control)?;
        let prepared = authorized.prepared();
        let root = prepared.root().as_path();
        let relative =
            crate::platform_path::repository_path(prepared.file()).map_err(|_| Failure::Denied)?;
        let target = root.join(relative);
        let action = prepared.action();
        let absent = matches!(
            action.operation(),
            MachineFileOperation::Write { expected: None, .. }
        ) || (matches!(action.operation(), MachineFileOperation::Read(None))
            && matches!(fs::symlink_metadata(&target), Err(error) if error.kind() == std::io::ErrorKind::NotFound));
        validate_target(root, &target, absent)?;
        let resource = match prepared.policy_action() {
            PolicyAction::MachineFile { resource_id, .. } => *resource_id,
            _ => return Err(Failure::Denied),
        };
        let decision = authorized.decision().id();
        match action.operation() {
            MachineFileOperation::Read(expected) => {
                if expected.is_none()
                    && matches!(fs::symlink_metadata(&target), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                {
                    return Ok(MachineFileReceipt {
                        resource,
                        decision,
                        hash: None,
                        changed: false,
                        text: None,
                    });
                }
                let text = read_text(root, &target, control)?;
                if expected.is_some_and(|hash| hash != text.content_hash()) {
                    return Err(Failure::Stale);
                }
                Ok(MachineFileReceipt {
                    resource,
                    decision,
                    hash: Some(text.content_hash()),
                    changed: false,
                    text: Some(text),
                })
            }
            MachineFileOperation::Write { expected, content } => {
                verify_expected(root, &target, *expected, control)?;
                let permissions = if expected.is_some() {
                    Some(
                        fs::metadata(&target)
                            .map_err(|_| Failure::Unavailable)?
                            .permissions(),
                    )
                } else {
                    None
                };
                let temporary = root.join(format!(".a3-machine-{}.tmp", decision));
                let (staged, mut staging_file) = StagedFile::create(temporary, content.clone())?;
                for bytes in content.as_bytes().chunks(16 * 1024) {
                    cancelled(control)?;
                    staging_file
                        .write_all(bytes)
                        .map_err(|_| Failure::Unavailable)?;
                }
                if let Some(permissions) = permissions {
                    staging_file
                        .set_permissions(permissions)
                        .map_err(|_| Failure::Unavailable)?;
                }
                staging_file.sync_all().map_err(|_| Failure::Unavailable)?;
                drop(staging_file);
                cancelled(control)?;
                verify_expected(root, &target, *expected, control)?;
                if read_text(root, &staged.path, control)?.content_hash() != content.content_hash()
                {
                    return Err(Failure::Stale);
                }
                if expected.is_none() {
                    crate::workspace_patch::install_no_replace(&staged.path, &target).map_err(
                        |error| {
                            if error.kind() == std::io::ErrorKind::AlreadyExists {
                                Failure::Stale
                            } else {
                                Failure::ReconciliationRequired
                            }
                        },
                    )?;
                } else {
                    fs::rename(&staged.path, &target)
                        .map_err(|_| Failure::ReconciliationRequired)?;
                }
                // The action has started: cancellation cannot abandon post-effect verification.
                let text = read_text(root, &target, &FinishEffect)
                    .map_err(|_| Failure::ReconciliationRequired)?;
                if text.content_hash() != content.content_hash() {
                    return Err(Failure::ReconciliationRequired);
                }
                Ok(MachineFileReceipt {
                    resource,
                    decision,
                    hash: Some(text.content_hash()),
                    changed: expected != &Some(text.content_hash()),
                    text: None,
                })
            }
            MachineFileOperation::Delete(expected) => {
                verify_expected(root, &target, Some(*expected), control)?;
                cancelled(control)?;
                fs::remove_file(&target).map_err(|_| Failure::ReconciliationRequired)?;
                verify_absence(fs::symlink_metadata(&target))?;
                Ok(MachineFileReceipt {
                    resource,
                    decision,
                    hash: None,
                    changed: true,
                    text: None,
                })
            }
        }
    }
}

fn verify_absence(observation: std::io::Result<fs::Metadata>) -> Result<(), Failure> {
    match observation {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        // Access errors and inaccessible parents cannot prove absence after a deletion.
        _ => Err(Failure::ReconciliationRequired),
    }
}

#[cfg(test)]
#[test]
fn deletion_does_not_confuse_an_unobservable_target_with_proved_absence() {
    for kind in [
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::Interrupted,
        std::io::ErrorKind::Other,
    ] {
        assert_eq!(
            verify_absence(Err(std::io::Error::from(kind))),
            Err(Failure::ReconciliationRequired)
        );
    }
    assert_eq!(
        verify_absence(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
        Ok(())
    );
}
impl MachineFileTool for WorkspaceMachineFileTool {
    fn prepare<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        action: &'a MachineFileAction,
        control: &'a dyn MachineFileControl,
    ) -> MachineFileToolFuture<'a, PreparedMachineFileAction> {
        Box::pin(async move { self.prepare_sync(project, action, control) })
    }
    fn execute<'a>(
        &'a self,
        authorized: AuthorizedMachineFileAction,
        control: &'a dyn MachineFileControl,
    ) -> MachineFileToolFuture<'a, MachineFileReceipt> {
        Box::pin(async move {
            if authorized.decision().reason() == PolicyDecisionReason::SystemAutomatic {
                let current = self
                    .permissions
                    .load_agent_permissions()
                    .await
                    .map_err(|_| Failure::Denied)?;
                if authorized.decision().permission_settings() != Some(current) {
                    return Err(Failure::PermissionsChanged);
                }
            }
            self.execute_sync(&authorized, control)
        })
    }
}

fn cancelled(control: &dyn MachineFileControl) -> Result<(), Failure> {
    if control.is_cancelled() {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}
fn validate_target(root: &Path, target: &Path, absent: bool) -> Result<(), Failure> {
    let current = fs::canonicalize(root).map_err(|_| Failure::Denied)?;
    if current != root
        || crate::workspace_patch::is_link_or_reparse(
            &fs::symlink_metadata(root).map_err(|_| Failure::Denied)?,
        )
    {
        return Err(Failure::Stale);
    }
    match fs::symlink_metadata(target) {
        Ok(metadata)
            if !absent
                && metadata.is_file()
                && !crate::workspace_patch::is_link_or_reparse(&metadata)
                && fs::canonicalize(target).map_err(|_| Failure::Denied)? == target =>
        {
            Ok(())
        }
        Err(error) if absent && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(Failure::Stale),
    }
}
fn read_text(
    root: &Path,
    target: &Path,
    control: &dyn MachineFileControl,
) -> Result<PatchFileContent, Failure> {
    cancelled(control)?;
    validate_target(root, target, false)?;
    let mut file = open_regular_no_follow(target).map_err(|_| Failure::Denied)?;
    if file.metadata().map_err(|_| Failure::Unavailable)?.len() > MAX_BYTES as u64 {
        return Err(Failure::Denied);
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        cancelled(control)?;
        let count = file.read(&mut chunk).map_err(|_| Failure::Unavailable)?;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > MAX_BYTES {
            return Err(Failure::Denied);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    validate_target(root, target, false)?;
    let content = PatchFileContent::try_from_bytes(bytes).map_err(|_| Failure::Denied)?;
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Failure::Denied)?;
    let revision = a3_domain::FileRevision::new(
        RepositoryPath::try_from_bytes(name.as_bytes().to_vec()).map_err(|_| Failure::Denied)?,
        content.content_hash(),
    );
    crate::secure_file::read_verified_text(root, &revision, || control.is_cancelled()).map_err(
        |error| match error {
            crate::secure_file::SecureFileReadError::Cancelled => Failure::Cancelled,
            crate::secure_file::SecureFileReadError::Stale => Failure::Stale,
            _ => Failure::Denied,
        },
    )?;
    Ok(content)
}
fn verify_expected(
    root: &Path,
    target: &Path,
    expected: Option<a3_domain::ContentHash>,
    control: &dyn MachineFileControl,
) -> Result<(), Failure> {
    if let Some(hash) = expected {
        if read_text(root, target, control)?.content_hash() != hash {
            return Err(Failure::Stale);
        }
    } else {
        validate_target(root, target, true)?;
    }
    Ok(())
}
#[derive(Debug)]
struct FinishEffect;
impl MachineFileControl for FinishEffect {
    fn is_cancelled(&self) -> bool {
        false
    }
}
struct StagedFile {
    path: PathBuf,
    content: PatchFileContent,
}
impl StagedFile {
    fn create(path: PathBuf, content: PatchFileContent) -> Result<(Self, fs::File), Failure> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| Failure::Unavailable)?;
        Ok((Self { path, content }, file))
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        // Clean only the exclusively reserved, still canonical staging name with our bytes.
        // A foreign replacement or parent change must never be deleted as cleanup.
        let Some(root) = self.path.parent() else {
            return;
        };
        if validate_target(root, &self.path, false).is_err() {
            return;
        }
        let Ok(file) = open_regular_no_follow(&self.path) else {
            return;
        };
        let mut bytes = Vec::new();
        if file
            .take(self.content.as_bytes().len() as u64 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || !self.content.as_bytes().starts_with(&bytes)
        {
            return;
        }
        let _cleanup = fs::remove_file(&self.path);
    }
}
