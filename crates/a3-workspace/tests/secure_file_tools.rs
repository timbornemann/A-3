//! Public secure-file-tool contract tests against controlled filesystem boundaries.

mod support;

use a3_application::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, WorkspaceDirectoryLister,
    WorkspaceDirectoryProgressError, WorkspaceDirectoryReadControl, WorkspaceDirectoryReadFailure,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, CanonicalDirectory, ContentHash,
    DirectoryPageSize, FileRevision, GitHead, GitReferenceName, IndexPublication, IndexRunId,
    IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkedGraph, ModulePolicyVersion,
    ModuleProjection, ModuleSymbolSet, Progress, ProjectIdentity, PublishedIndex, RankProjection,
    RankingPolicyVersion, RepositoryCard, RepositoryId, RepositoryIdentity, RepositoryPath,
    SnapshotId, WorkspaceDirectory, WorkspaceDirectoryEntryKind, WorkspaceDirectoryListRequest,
    WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_workspace::{IndexedWorkspaceDirectoryLister, WorkspaceAgentSourceReader};
use std::error::Error;
use std::fs;
use std::path::Path;
use support::TempDirectory;

#[derive(Debug)]
struct Active;

impl AgentSourceReadControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl WorkspaceDirectoryReadControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), WorkspaceDirectoryProgressError> {
        Ok(())
    }
}

#[test]
fn allowed_subtree_read_returns_bounded_text_and_exact_evidence() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir_all(root.join("src/nested"))?;
    let content = b"first\nsecond\nthird\n";
    let revision = write_revision(&root, b"src/nested/lib.rs", content)?;
    let project = project(&root)?;
    let request = AgentFileInspection::new(
        revision.path().clone(),
        AgentFileStartLine::new(2)?,
        AgentFileLineCount::new(1)?,
    );

    let page = futures::executor::block_on(
        WorkspaceAgentSourceReader.read_page(&project, &revision, &request, &Active),
    )?;

    assert_eq!(page.text(), "second\n");
    assert_eq!(page.revision(), &revision);
    let evidence = page.evidence();
    assert_eq!(evidence.location().revision(), &revision);
    assert_eq!(evidence.location().range(), Some(page.range()));
    assert!(page.truncated());
    assert_eq!(page.next_start_line(), Some(AgentFileStartLine::new(3)?));
    Ok(())
}

#[test]
fn directory_pages_use_only_safe_published_children_and_carry_evidence()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir_all(root.join("src/nested"))?;
    fs::write(root.join("ignored.txt"), "must stay absent")?;

    let readme = write_revision(&root, b"README.md", b"readme\n")?;
    let library = write_revision(&root, b"src/lib.rs", b"pub fn library() {}\n")?;
    let nested = write_revision(&root, b"src/nested/mod.rs", b"pub mod child;\n")?;
    let secret = write_revision(&root, b".env", b"password=fixture-secret-value\n")?;
    let generated = write_revision(&root, b"target/generated.rs", b"generated\n")?;
    let project = project(&root)?;
    let snapshot_id = SnapshotId::from_bytes([6; 32]);
    let index = published_index(
        snapshot_id,
        vec![readme, library.clone(), nested.clone(), secret, generated],
    )?;
    let first = WorkspaceDirectoryListRequest::new(
        project.worktree().id(),
        snapshot_id,
        WorkspaceDirectory::Root,
        None,
        DirectoryPageSize::new(1)?,
    )?;

    let first_page = futures::executor::block_on(
        IndexedWorkspaceDirectoryLister.list(&project, &index, &first, &Active),
    )?;
    assert_eq!(entry_paths(&first_page), vec![b"README.md".as_slice()]);
    assert!(first_page.truncated());
    let second = WorkspaceDirectoryListRequest::new(
        project.worktree().id(),
        snapshot_id,
        WorkspaceDirectory::Root,
        first_page.next_after().cloned(),
        DirectoryPageSize::new(2)?,
    )?;
    let second_page = futures::executor::block_on(
        IndexedWorkspaceDirectoryLister.list(&project, &index, &second, &Active),
    )?;
    assert_eq!(entry_paths(&second_page), vec![b"src".as_slice()]);
    assert_eq!(
        second_page.entries()[0].kind(),
        WorkspaceDirectoryEntryKind::Directory
    );
    assert_eq!(second_page.entries()[0].supporting_revision(), &library);
    assert!(!second_page.truncated());

    let subtree = WorkspaceDirectoryListRequest::new(
        project.worktree().id(),
        snapshot_id,
        WorkspaceDirectory::Subtree(RepositoryPath::try_from_bytes(b"src".to_vec())?),
        None,
        DirectoryPageSize::new(2)?,
    )?;
    let subtree_page = futures::executor::block_on(
        IndexedWorkspaceDirectoryLister.list(&project, &index, &subtree, &Active),
    )?;
    assert_eq!(
        entry_paths(&subtree_page),
        vec![b"src/lib.rs".as_slice(), b"src/nested".as_slice()]
    );
    assert_eq!(subtree_page.entries()[1].supporting_revision(), &nested);
    Ok(())
}

#[test]
fn traversal_and_symlink_escape_do_not_cross_the_selected_root() -> Result<(), Box<dyn Error>> {
    assert!(RepositoryPath::try_from_bytes(b"../outside.txt".to_vec()).is_err());

    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    fs::create_dir(&root)?;
    fs::create_dir(&outside)?;
    let outside_content = b"outside\n";
    fs::write(outside.join("secret.rs"), outside_content)?;
    create_directory_symlink(&outside, &root.join("escape"))?;
    let project = project(&root)?;
    let path = RepositoryPath::try_from_bytes(b"escape/secret.rs".to_vec())?;
    let revision = FileRevision::new(path.clone(), hash(outside_content));
    let request = inspection(path)?;

    assert_eq!(
        futures::executor::block_on(
            WorkspaceAgentSourceReader.read_page(&project, &revision, &request, &Active,)
        ),
        Err(AgentSourceReadFailure::Denied)
    );

    let snapshot_id = SnapshotId::from_bytes([7; 32]);
    let index = published_index(snapshot_id, vec![revision])?;
    let listing = WorkspaceDirectoryListRequest::new(
        project.worktree().id(),
        snapshot_id,
        WorkspaceDirectory::Subtree(RepositoryPath::try_from_bytes(b"escape".to_vec())?),
        None,
        DirectoryPageSize::new(10)?,
    )?;
    assert_eq!(
        futures::executor::block_on(
            IndexedWorkspaceDirectoryLister.list(&project, &index, &listing, &Active,)
        ),
        Err(WorkspaceDirectoryReadFailure::Denied)
    );
    Ok(())
}

#[test]
fn secret_binary_and_oversized_files_are_stopped_before_context_output()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    let project = project(&root)?;

    let secret = b"password=fixture-secret-value\n";
    let secret_revision = write_revision(&root, b"ordinary.txt", secret)?;
    assert_read_failure(
        &project,
        &secret_revision,
        AgentSourceReadFailure::SecretCandidate,
    )?;

    let binary = b"ordinary\0binary";
    let binary_revision = write_revision(&root, b"binary.txt", binary)?;
    assert_read_failure(
        &project,
        &binary_revision,
        AgentSourceReadFailure::BinaryContent,
    )?;

    let large_path = root.join("large.txt");
    let large = fs::File::create(&large_path)?;
    large.set_len(4 * 1_024 * 1_024 + 1)?;
    let large_revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"large.txt".to_vec())?,
        ContentHash::from_bytes([9; 32]),
    );
    assert_read_failure(
        &project,
        &large_revision,
        AgentSourceReadFailure::FileTooLarge,
    )?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn special_files_are_rejected_without_opening_them() -> Result<(), Box<dyn Error>> {
    use std::os::unix::net::UnixListener;

    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    fs::create_dir(&root)?;
    let _socket = UnixListener::bind(root.join("agent.sock"))?;
    let project = project(&root)?;
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"agent.sock".to_vec())?,
        ContentHash::from_bytes([10; 32]),
    );

    assert_read_failure(&project, &revision, AgentSourceReadFailure::Denied)?;
    Ok(())
}

fn assert_read_failure(
    project: &ProjectIdentity,
    revision: &FileRevision,
    expected: AgentSourceReadFailure,
) -> Result<(), Box<dyn Error>> {
    let request = inspection(revision.path().clone())?;
    assert_eq!(
        futures::executor::block_on(
            WorkspaceAgentSourceReader.read_page(project, revision, &request, &Active,)
        ),
        Err(expected)
    );
    Ok(())
}

fn inspection(path: RepositoryPath) -> Result<AgentFileInspection, Box<dyn Error>> {
    Ok(AgentFileInspection::new(
        path,
        AgentFileStartLine::new(1)?,
        AgentFileLineCount::new(20)?,
    ))
}

fn write_revision(
    root: &Path,
    repository_path: &[u8],
    content: &[u8],
) -> Result<FileRevision, Box<dyn Error>> {
    let path = RepositoryPath::try_from_bytes(repository_path.to_vec())?;
    let os_path = root.join(std::str::from_utf8(repository_path)?);
    if let Some(parent) = os_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(os_path, content)?;
    Ok(FileRevision::new(path, hash(content)))
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

fn entry_paths(listing: &a3_domain::WorkspaceDirectoryListing) -> Vec<&[u8]> {
    listing
        .entries()
        .iter()
        .map(|entry| entry.path().as_bytes())
        .collect()
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
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
