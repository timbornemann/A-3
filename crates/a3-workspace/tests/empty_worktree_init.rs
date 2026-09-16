//! Empty-worktree initialization contract tests using isolated local directories.

mod support;

use a3_application::EmptyWorktreeInitializer;
use a3_domain::GitHead;
use a3_workspace::{RepositoryInspector, WorkspaceEmptyWorktreeInitializer};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use support::TempDirectory;

const MAX_COMMAND_DIAGNOSTIC_BYTES: usize = 4_096;

#[test]
fn empty_directory_becomes_unborn_main_without_a_remote() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project_root = fixture.path().join("empty-project");
    fs::create_dir(&project_root)?;

    WorkspaceEmptyWorktreeInitializer::new().initialize(&project_root)?;
    let identity = RepositoryInspector::new().inspect(&project_root)?;

    assert_eq!(identity.repository().main_remote(), None);
    assert!(matches!(
        identity.head(),
        GitHead::Unborn { reference } if reference.as_str() == "refs/heads/main"
    ));
    assert!(!project_root.join("README.md").exists());
    let worktree_entries = fs::read_dir(&project_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() != ".git")
        .count();
    assert_eq!(worktree_entries, 0);
    Ok(())
}

#[test]
fn non_empty_directory_is_refused() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project_root = fixture.path().join("with-file");
    fs::create_dir(&project_root)?;
    fs::write(project_root.join("notes.txt"), "idea")?;

    let result = WorkspaceEmptyWorktreeInitializer::new().initialize(&project_root);

    assert!(matches!(
        result,
        Err(a3_workspace::EmptyWorktreeInitError::DirectoryNotEmpty)
    ));
    assert!(!project_root.join(".git").exists());
    Ok(())
}

#[test]
fn existing_git_metadata_is_not_overwritten() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project_root = fixture.path().join("already-git");
    fs::create_dir(&project_root)?;
    initialize_repository(&project_root)?;

    let result = WorkspaceEmptyWorktreeInitializer::new().initialize(&project_root);

    assert!(matches!(
        result,
        Err(a3_workspace::EmptyWorktreeInitError::AlreadyRepository)
    ));
    Ok(())
}

#[test]
fn nested_directory_inside_another_worktree_is_refused() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let repository_root = fixture.path().join("repository");
    let nested = repository_root.join("nested");
    fs::create_dir_all(&nested)?;
    initialize_repository(&repository_root)?;

    let result = WorkspaceEmptyWorktreeInitializer::new().initialize(&nested);

    assert!(matches!(
        result,
        Err(a3_workspace::EmptyWorktreeInitError::NotWorktreeRoot)
    ));
    assert!(!nested.join(".git").exists());
    Ok(())
}

#[test]
fn isolated_init_writes_unborn_main_without_user_gitconfig() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project_root = fixture.path().join("isolated");
    fs::create_dir(&project_root)?;
    fs::write(
        fixture.path().join("gitconfig"),
        "[init]\n\tdefaultBranch = master\n",
    )?;

    WorkspaceEmptyWorktreeInitializer::new().initialize(&project_root)?;

    let identity = RepositoryInspector::new().inspect(&project_root)?;
    assert!(matches!(
        identity.head(),
        GitHead::Unborn { reference } if reference.as_str() == "refs/heads/main"
    ));
    let config = fs::read_to_string(project_root.join(".git").join("config"))?;
    assert!(
        !config.contains("defaultBranch"),
        "isolated init must not copy user init.defaultBranch into the new repository"
    );
    Ok(())
}

#[test]
fn application_port_maps_directory_not_empty() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project_root = fixture.path().join("with-file");
    fs::create_dir(&project_root)?;
    fs::write(project_root.join("notes.txt"), "idea")?;

    let result = EmptyWorktreeInitializer::initialize_empty_worktree(
        &WorkspaceEmptyWorktreeInitializer::new(),
        &project_root,
    );

    assert_eq!(
        result,
        Err(a3_application::EmptyWorktreeInitializationFailure::DirectoryNotEmpty)
    );
    Ok(())
}

fn initialize_repository(repository_root: &Path) -> Result<(), Box<dyn Error>> {
    run_git(repository_root, ["init"])?;
    run_git(repository_root, ["symbolic-ref", "HEAD", "refs/heads/main"])?;
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
