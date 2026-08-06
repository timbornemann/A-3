//! Public E3 PatchAction contract tests against controlled filesystem boundaries.

mod support;

use a3_application::{
    AuthorizedPatchAction, PatchApplyFailure, PatchPreviewFailure, WorkspacePatchControl,
    WorkspacePatchProgressError, WorkspacePatchTool,
};
use a3_domain::{
    AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest, ApprovalRequestId,
    CanonicalDirectory, ContentHash, FileRevision, GitHead, GitReferenceName, IndexPublication,
    IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkedGraph, ModulePolicyVersion,
    ModuleProjection, ModuleSymbolSet, PatchAction, PatchActionSchemaVersion, PatchAdd,
    PatchChange, PatchFileContent, PatchLineEndings, PatchMove, PatchOperation, PatchRationale,
    PatchTextEncoding, PatchUpdate, PolicyDecision, PolicyDecisionId, PolicyEvaluationTiming,
    Progress, ProjectIdentity, PublishedIndex, RankProjection, RankingPolicyVersion,
    RepositoryCard, RepositoryId, RepositoryIdentity, RepositoryPath, SnapshotId, TaskStepId,
    VerificationSpecId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_workspace::WorkspacePatchAdapter;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use support::TempDirectory;

#[derive(Debug, Default)]
struct Active {
    progress: Mutex<Vec<Progress>>,
}

impl WorkspacePatchControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        self.progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct ConflictAfterFirstChange {
    conflicting_path: PathBuf,
}

impl WorkspacePatchControl for ConflictAfterFirstChange {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        if progress.completed() == Some(1) && !self.conflicting_path.exists() {
            fs::write(&self.conflicting_path, b"user content\n")
                .map_err(|_| WorkspacePatchProgressError)?;
        }
        Ok(())
    }
}

#[test]
fn preview_and_apply_all_operations_preserve_exact_bytes_and_evidence() -> Result<(), Box<dyn Error>>
{
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir_all(root.join("src"))?;
    let update_before = b"\xef\xbb\xbfbefore\r\nzweite Zeile \xc3\xa4\r\n";
    let update_after = b"\xef\xbb\xbfafter\r\nzweite Zeile \xc3\xb6\r\n";
    let move_content = b"move me\r\nunchanged\r\n";
    let delete_content = b"delete me\n";
    let update = write_revision(&root, b"src/update.txt", update_before)?;
    let movement = write_revision(&root, b"src/move.txt", move_content)?;
    let deletion = write_revision(&root, b"src/delete.txt", delete_content)?;
    let snapshot_id = SnapshotId::from_bytes([6; 32]);
    let project = project(&root)?;
    let published = published_index(
        snapshot_id,
        vec![update.clone(), movement.clone(), deletion.clone()],
    )?;
    let action = patch_action(
        project.worktree().id(),
        snapshot_id,
        vec![
            PatchOperation::Update(PatchUpdate::new(
                update.clone(),
                PatchFileContent::try_from_bytes(update_after.to_vec())?,
            )?),
            PatchOperation::Delete(deletion.clone()),
            PatchOperation::Add(PatchAdd::new(
                path(b"src/add.txt")?,
                PatchFileContent::try_from_bytes(b"added\n".to_vec())?,
            )),
            PatchOperation::Move(PatchMove::new(movement.clone(), path(b"src/moved.txt")?)?),
        ],
    )?;
    let control = Active::default();
    let adapter = WorkspacePatchAdapter::new();

    let preview =
        futures::executor::block_on(adapter.preview(&project, &published, &action, &control))?;
    assert_eq!(preview.entries().len(), 4);
    assert_eq!(
        preview.entries()[0]
            .target_path()
            .map(RepositoryPath::as_bytes),
        Some(b"src/add.txt".as_slice())
    );
    let update_preview = preview
        .entries()
        .iter()
        .find(|entry| entry.source_path() == Some(update.path()))
        .ok_or("missing update preview")?;
    assert_eq!(
        update_preview.before().ok_or("missing before")?.bytes(),
        update_before
    );
    assert_eq!(
        update_preview.after().ok_or("missing after")?.bytes(),
        update_after
    );
    assert_eq!(
        update_preview.after().ok_or("missing after")?.encoding(),
        PatchTextEncoding::Utf8Bom
    );
    assert_eq!(
        update_preview
            .after()
            .ok_or("missing after")?
            .line_endings(),
        PatchLineEndings::Crlf
    );

    let authorization = authorize(action.clone())?;
    let changes =
        futures::executor::block_on(adapter.apply(&project, &published, authorization, &control))?;

    assert!(changes.complete());
    assert_eq!(changes.changes().len(), 4);
    assert_eq!(changes.changed_paths().len(), 5);
    assert_eq!(fs::read(root.join("src/add.txt"))?, b"added\n");
    assert_eq!(fs::read(root.join("src/update.txt"))?, update_after);
    assert_eq!(fs::read(root.join("src/moved.txt"))?, move_content);
    assert!(!root.join("src/move.txt").exists());
    assert!(!root.join("src/delete.txt").exists());
    let updated = changes
        .changes()
        .iter()
        .find_map(|change| match change {
            PatchChange::Updated { current, .. } => Some(current),
            _ => None,
        })
        .ok_or("missing update evidence")?;
    assert_eq!(updated.content_hash(), hash(update_after));
    assert_eq!(changes.action_digest(), action.digest());
    Ok(())
}

#[test]
fn user_edit_after_preview_blocks_apply_and_is_never_overwritten() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    let expected = write_revision(&root, b"file.txt", b"indexed\n")?;
    let snapshot_id = SnapshotId::from_bytes([7; 32]);
    let project = project(&root)?;
    let published = published_index(snapshot_id, vec![expected.clone()])?;
    let action = patch_action(
        project.worktree().id(),
        snapshot_id,
        vec![PatchOperation::Update(PatchUpdate::new(
            expected,
            PatchFileContent::try_from_bytes(b"agent\n".to_vec())?,
        )?)],
    )?;
    let adapter = WorkspacePatchAdapter::new();
    let control = Active::default();
    futures::executor::block_on(adapter.preview(&project, &published, &action, &control))?;
    fs::write(root.join("file.txt"), b"user edit\n")?;

    assert_eq!(
        futures::executor::block_on(adapter.apply(
            &project,
            &published,
            authorize(action)?,
            &control,
        )),
        Err(PatchApplyFailure::Conflict)
    );
    assert_eq!(fs::read(root.join("file.txt"))?, b"user edit\n");
    Ok(())
}

#[test]
fn add_never_overwrites_a_live_path_absent_from_the_snapshot() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    fs::write(root.join("new.txt"), b"user content\n")?;
    let snapshot_id = SnapshotId::from_bytes([8; 32]);
    let project = project(&root)?;
    let published = published_index(snapshot_id, Vec::new())?;
    let action = patch_action(
        project.worktree().id(),
        snapshot_id,
        vec![PatchOperation::Add(PatchAdd::new(
            path(b"new.txt")?,
            PatchFileContent::try_from_bytes(b"agent content\n".to_vec())?,
        ))],
    )?;

    assert_eq!(
        futures::executor::block_on(WorkspacePatchAdapter::new().preview(
            &project,
            &published,
            &action,
            &Active::default(),
        )),
        Err(PatchPreviewFailure::Conflict)
    );
    assert_eq!(fs::read(root.join("new.txt"))?, b"user content\n");
    Ok(())
}

#[test]
fn symlink_or_reparse_destination_cannot_escape_the_selected_root() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    fs::create_dir(&root)?;
    fs::create_dir(&outside)?;
    create_directory_link(&outside, &root.join("escape"))?;
    let snapshot_id = SnapshotId::from_bytes([9; 32]);
    let project = project(&root)?;
    let published = published_index(snapshot_id, Vec::new())?;
    let action = patch_action(
        project.worktree().id(),
        snapshot_id,
        vec![PatchOperation::Add(PatchAdd::new(
            path(b"escape/outside.txt")?,
            PatchFileContent::try_from_bytes(b"must not escape\n".to_vec())?,
        ))],
    )?;

    assert_eq!(
        futures::executor::block_on(WorkspacePatchAdapter::new().preview(
            &project,
            &published,
            &action,
            &Active::default(),
        )),
        Err(PatchPreviewFailure::Denied)
    );
    assert!(!outside.join("outside.txt").exists());
    Ok(())
}

#[test]
fn a_late_conflict_returns_the_exact_partial_change_set() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    let snapshot_id = SnapshotId::from_bytes([10; 32]);
    let project = project(&root)?;
    let published = published_index(snapshot_id, Vec::new())?;
    let action = patch_action(
        project.worktree().id(),
        snapshot_id,
        vec![
            PatchOperation::Add(PatchAdd::new(
                path(b"a-first.txt")?,
                PatchFileContent::try_from_bytes(b"first\n".to_vec())?,
            )),
            PatchOperation::Add(PatchAdd::new(
                path(b"z-second.txt")?,
                PatchFileContent::try_from_bytes(b"second\n".to_vec())?,
            )),
        ],
    )?;
    let control = ConflictAfterFirstChange {
        conflicting_path: root.join("z-second.txt"),
    };
    let result = futures::executor::block_on(WorkspacePatchAdapter::new().apply(
        &project,
        &published,
        authorize(action.clone())?,
        &control,
    ));

    let Err(PatchApplyFailure::Changed(changes)) = result else {
        return Err("late conflict did not expose a partial change set".into());
    };
    assert!(!changes.complete());
    assert_eq!(changes.changes().len(), 1);
    assert_eq!(changes.action_digest(), action.digest());
    assert_eq!(fs::read(root.join("a-first.txt"))?, b"first\n");
    assert_eq!(fs::read(root.join("z-second.txt"))?, b"user content\n");
    Ok(())
}

#[test]
fn preview_is_deterministically_bounded_across_many_files() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    let snapshot_id = SnapshotId::from_bytes([17; 32]);
    let project = project(&root)?;
    let published = published_index(snapshot_id, Vec::new())?;
    let content = vec![b'x'; 20 * 1_024];
    let operations = (0..5)
        .map(|index| {
            Ok(PatchOperation::Add(PatchAdd::new(
                path(format!("file-{index}.txt").as_bytes())?,
                PatchFileContent::try_from_bytes(content.clone())?,
            )))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let action = patch_action(project.worktree().id(), snapshot_id, operations)?;

    let preview = futures::executor::block_on(WorkspacePatchAdapter::new().preview(
        &project,
        &published,
        &action,
        &Active::default(),
    ))?;

    assert_eq!(preview.retained_bytes(), 64 * 1_024);
    assert!(preview.entries().iter().all(|entry| {
        entry
            .after()
            .is_some_and(|after| after.bytes().len() <= 16 * 1_024)
    }));
    assert_eq!(
        preview.entries()[4]
            .after()
            .ok_or("missing final preview")?
            .bytes(),
        b""
    );
    Ok(())
}

fn patch_action(
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
    operations: Vec<PatchOperation>,
) -> Result<PatchAction, Box<dyn Error>> {
    Ok(PatchAction::new(
        PatchActionSchemaVersion::V1,
        AgentRunId::from_bytes([11; 32]),
        worktree_id,
        snapshot_id,
        TaskStepId::from_bytes([12; 32]),
        VerificationSpecId::from_bytes([13; 32]),
        PatchRationale::try_from_string("apply the verified E3 fixture".to_owned())?,
        operations,
    )?)
}

fn authorize(action: PatchAction) -> Result<AuthorizedPatchAction, Box<dyn Error>> {
    let policy_action = action.policy_action();
    let requested_at = AgentRunTimestamp::from_unix_millis(20)?;
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([14; 32]),
        action.run_id(),
        &policy_action,
        requested_at,
        AgentRunTimestamp::from_unix_millis(100)?,
    )?;
    let mut grant = ApprovalGrant::grant(
        ApprovalId::from_bytes([15; 32]),
        &request,
        AgentRunTimestamp::from_unix_millis(21)?,
    )?;
    let decision_id = PolicyDecisionId::from_bytes([16; 32]);
    let decided_at = AgentRunTimestamp::from_unix_millis(22)?;
    grant.consume(decision_id, action.run_id(), &policy_action, decided_at)?;
    let decision = PolicyDecision::approved(
        decision_id,
        action.run_id(),
        &policy_action,
        &grant,
        PolicyEvaluationTiming::new(decided_at, decided_at)?,
    )?;
    Ok(AuthorizedPatchAction::new(action, &decision)?)
}

fn write_revision(
    root: &Path,
    repository_path: &[u8],
    content: &[u8],
) -> Result<FileRevision, Box<dyn Error>> {
    let path = path(repository_path)?;
    let os_path = root.join(std::str::from_utf8(repository_path)?);
    if let Some(parent) = os_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(os_path, content)?;
    Ok(FileRevision::new(path, hash(content)))
}

fn path(bytes: &[u8]) -> Result<RepositoryPath, Box<dyn Error>> {
    Ok(RepositoryPath::try_from_bytes(bytes.to_vec())?)
}

fn hash(content: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(content).as_bytes())
}

fn project(root: &Path) -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(fs::canonicalize(root)?)?;
    let repository_id = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository_id, root.clone(), None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository_id,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn published_index(
    snapshot_id: SnapshotId,
    files: Vec<FileRevision>,
) -> Result<PublishedIndex, Box<dyn Error>> {
    let file_count = u32::try_from(files.len())?;
    let graph = LinkedGraph::new(snapshot_id, files, Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
    let policy = ModulePolicyVersion::v1();
    let card = RepositoryCard::new(
        snapshot_id,
        policy,
        Vec::new(),
        Vec::new(),
        ModuleSymbolSet::empty(),
        file_count,
        0,
    )?;
    let modules = ModuleProjection::new(snapshot_id, policy, Vec::new(), Vec::new(), card)?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([4; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(PublishedIndex::new(run, publication)?)
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(std::io::Error::other(format!(
        "could not create Windows junction: {}",
        String::from_utf8_lossy(&output.stderr)
    )))
}
