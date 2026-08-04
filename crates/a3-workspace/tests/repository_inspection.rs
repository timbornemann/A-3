//! Repository-inspection contract tests using controlled local Git fixtures.

mod support;

use a3_domain::GitHead;
use a3_workspace::{RepositoryInspectionError, RepositoryInspector};
use std::error::Error;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use support::TempDirectory;

const MAX_COMMAND_DIAGNOSTIC_BYTES: usize = 4_096;

#[test]
fn unborn_repository_without_remote_has_stable_identity() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let repository_root = fixture.path().join("repository");
    fs::create_dir(&repository_root)?;
    initialize_repository(&repository_root)?;

    let first = RepositoryInspector::new().inspect(&repository_root)?;
    let second = RepositoryInspector::new().inspect(&repository_root)?;

    assert_eq!(first, second);
    assert_eq!(first.repository().main_remote(), None);
    assert!(matches!(
        first.head(),
        GitHead::Unborn { reference } if reference.as_str() == "refs/heads/main"
    ));
    Ok(())
}

#[test]
fn linked_worktrees_share_repository_identity_but_not_worktree_identity()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let repository_root = fixture.path().join("repository");
    let linked_root = fixture.path().join("linked");
    fs::create_dir(&repository_root)?;
    initialize_repository(&repository_root)?;
    commit_fixture(&repository_root)?;
    run_git(
        &repository_root,
        [
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("--detach"),
            linked_root.as_os_str().to_owned(),
            OsString::from("HEAD"),
        ],
    )?;

    let primary = RepositoryInspector::new().inspect(&repository_root)?;
    let linked = RepositoryInspector::new().inspect(&linked_root)?;

    assert_eq!(primary.repository().id(), linked.repository().id());
    assert_eq!(
        primary.repository().common_directory(),
        linked.repository().common_directory()
    );
    assert_ne!(primary.worktree().id(), linked.worktree().id());
    assert_ne!(primary.worktree().root(), linked.worktree().root());
    assert!(matches!(
        linked.head(),
        GitHead::Born {
            reference: None,
            ..
        }
    ));
    Ok(())
}

#[test]
fn selecting_a_subdirectory_never_expands_the_workspace_root() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let repository_root = fixture.path().join("repository");
    let nested = repository_root.join("nested");
    fs::create_dir_all(&nested)?;
    initialize_repository(&repository_root)?;

    let result = RepositoryInspector::new().inspect(&nested);

    assert!(matches!(
        result,
        Err(RepositoryInspectionError::GitRepositoryOpen)
            | Err(RepositoryInspectionError::SelectedPathIsNotWorktreeRoot { .. })
    ));
    Ok(())
}

fn initialize_repository(repository_root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    run_git(repository_root, ["init"])?;
    run_git(repository_root, ["symbolic-ref", "HEAD", "refs/heads/main"])?;
    Ok(())
}

fn commit_fixture(repository_root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    fs::write(repository_root.join("fixture.txt"), "fixture")?;
    run_git(repository_root, ["add", "fixture.txt"])?;
    run_git(
        repository_root,
        [
            "-c",
            "user.name=A3 Test",
            "-c",
            "user.email=a3-test@example.invalid",
            "commit",
            "--no-gpg-sign",
            "-m",
            "fixture",
        ],
    )?;
    Ok(())
}

fn run_git<I, S>(current_directory: &Path, arguments: I) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .current_dir(current_directory)
        .args(arguments)
        .output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = bounded_diagnostic(&output.stderr);
    let stdout = bounded_diagnostic(&output.stdout);
    Err(io::Error::other(format!(
        "Git fixture command failed with {}: stdout={stdout:?}, stderr={stderr:?}",
        output.status
    )))
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let end = bytes.len().min(MAX_COMMAND_DIAGNOSTIC_BYTES);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
