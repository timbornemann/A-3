//! Path-policy integration tests against real filesystem canonicalization.

mod support;

use a3_workspace::{PathEntryKind, PathPolicy, PathPolicyError};
use std::error::Error;
use std::fs;
use std::path::Path;
use support::TempDirectory;

#[test]
fn resolves_only_existing_entries_inside_the_selected_root() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    let outside = fixture.path().join("outside.txt");
    fs::create_dir(&root)?;
    fs::write(root.join("inside.txt"), "inside")?;
    fs::write(&outside, "outside")?;

    let policy = PathPolicy::from_selected_root(&root)?;
    let inside = policy.resolve_existing("inside.txt")?;

    assert_eq!(inside.kind(), PathEntryKind::File);
    assert!(inside.as_path().starts_with(policy.root().as_path()));
    assert!(matches!(
        policy.resolve_existing(&outside),
        Err(PathPolicyError::OutsideRoot { .. })
    ));
    Ok(())
}

#[test]
fn rejects_a_symlink_that_resolves_outside_the_selected_root() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    fs::create_dir(&root)?;
    fs::create_dir(&outside)?;
    create_directory_symlink(&outside, &root.join("escape"))?;

    let policy = PathPolicy::from_selected_root(&root)?;

    assert!(matches!(
        policy.resolve_existing("escape"),
        Err(PathPolicyError::OutsideRoot { .. })
    ));
    Ok(())
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
