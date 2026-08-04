//! Contract tests for the local Git repository discovery adapter.

mod support;

use a3_application::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositoryDiscoveryFailure,
};
use a3_domain::{
    DiscoveredFileRole, DiscoveryExclusionReason, DiscoveryOrigin, DiscoveryPolicy, Progress,
};
use a3_repo_index::GitRepositoryDiscoverer;
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use support::TempDirectory;

#[derive(Debug, Default)]
struct TestControl {
    cancelled: AtomicBool,
    progress: Mutex<Vec<Progress>>,
}

impl TestControl {
    fn cancelled() -> Self {
        Self {
            cancelled: AtomicBool::new(true),
            progress: Mutex::new(Vec::new()),
        }
    }
}

impl RepositoryDiscoveryControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryDiscoveryControlError> {
        self.progress
            .lock()
            .map_err(|_| RepositoryDiscoveryControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[test]
fn discovery_contract_filters_and_sorts_tracked_and_untracked_files() -> Result<(), Box<dyn Error>>
{
    let fixture = repository_fixture()?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;
    let control = TestControl::default();
    let discoverer = GitRepositoryDiscoverer::new();

    let first = discoverer.discover(&project, DiscoveryPolicy::v1(), &control)?;
    let second = discoverer.discover(&project, DiscoveryPolicy::v1(), &control)?;

    assert_eq!(first, second);
    let paths = first
        .files()
        .iter()
        .map(|file| String::from_utf8_lossy(file.path().as_bytes()).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            ".a3/project.toml",
            ".github/workflows/ci.yml",
            ".gitignore",
            "Cargo.toml",
            "src/lib.rs",
            "src/new.rs",
            "tests/sample.rs",
            "tracked-ignored.txt",
        ]
    );
    assert_eq!(
        first
            .files()
            .iter()
            .find(|file| file.path().as_bytes() == b"src/new.rs")
            .map(|file| file.origin()),
        Some(DiscoveryOrigin::Untracked)
    );
    assert!(
        first
            .files()
            .iter()
            .find(|file| file.path().as_bytes() == b"Cargo.toml")
            .is_some_and(|file| file.roles().contains(DiscoveredFileRole::Manifest))
    );
    assert!(
        first
            .files()
            .iter()
            .find(|file| file.path().as_bytes() == b"tests/sample.rs")
            .is_some_and(|file| file.roles().contains(DiscoveredFileRole::Test))
    );
    assert!(
        first
            .files()
            .iter()
            .find(|file| file.path().as_bytes() == b".github/workflows/ci.yml")
            .is_some_and(|file| {
                file.roles()
                    .contains(DiscoveredFileRole::ContinuousIntegration)
            })
    );
    for reason in [
        DiscoveryExclusionReason::ProjectIgnore,
        DiscoveryExclusionReason::Vendor,
        DiscoveryExclusionReason::Generated,
        DiscoveryExclusionReason::Secret,
        DiscoveryExclusionReason::TooLarge,
        DiscoveryExclusionReason::Binary,
    ] {
        assert!(first.exclusions().get(reason) > 0, "missing {reason:?}");
    }
    assert!(
        control
            .progress
            .lock()
            .map_err(|_| "progress mutex poisoned")?
            .iter()
            .any(|progress| progress.is_complete())
    );
    Ok(())
}

#[test]
fn invalid_or_negating_project_ignore_fails_closed() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    fixture.write(
        ".a3/project.toml",
        b"[discovery]\nignore = [\"!secrets/**\"]\n",
    )?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;

    assert_eq!(
        GitRepositoryDiscoverer::new().discover(
            &project,
            DiscoveryPolicy::v1(),
            &TestControl::default(),
        ),
        Err(RepositoryDiscoveryFailure::InvalidConfiguration)
    );
    Ok(())
}

#[test]
fn cancellation_stops_before_repository_or_file_access() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;

    assert_eq!(
        GitRepositoryDiscoverer::new().discover(
            &project,
            DiscoveryPolicy::v1(),
            &TestControl::cancelled(),
        ),
        Err(RepositoryDiscoveryFailure::Cancelled)
    );
    Ok(())
}

#[test]
fn discovery_never_follows_a_directory_link_outside_the_worktree() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let outside = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    outside.write("secret.rs", b"must not be observed\n")?;
    fixture.link_directory("linked", outside.path())?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;

    let result = GitRepositoryDiscoverer::new().discover(
        &project,
        DiscoveryPolicy::v1(),
        &TestControl::default(),
    )?;

    assert!(result.files().is_empty());
    assert_eq!(
        result
            .exclusions()
            .get(DiscoveryExclusionReason::SymbolicLink),
        1
    );
    Ok(())
}

fn repository_fixture() -> Result<TempDirectory, Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    fixture.write("src/lib.rs", b"pub fn tracked() {}\n")?;
    fixture.write("Cargo.toml", b"[package]\nname = \"fixture\"\n")?;
    fixture.write("tests/sample.rs", b"#[test]\nfn works() {}\n")?;
    fixture.write(
        ".github/workflows/ci.yml",
        b"name: CI\non: [push]\njobs: {}\n",
    )?;
    fixture.write("tracked-ignored.txt", b"tracked\n")?;
    fixture.git([
        "add",
        "src/lib.rs",
        "Cargo.toml",
        "tests/sample.rs",
        ".github/workflows/ci.yml",
        "tracked-ignored.txt",
    ])?;

    fixture.write(".gitignore", b"ignored/**\n*.log\ntracked-ignored.txt\n")?;
    fixture.git(["add", ".gitignore"])?;
    fixture.write(
        ".a3/project.toml",
        b"[discovery]\nignore = [\"private/**\"]\n",
    )?;
    fixture.write("src/new.rs", b"pub fn untracked() {}\n")?;
    fixture.write("ignored/no.rs", b"ignored\n")?;
    fixture.write("debug.log", b"ignored\n")?;
    fixture.write(".git/info/exclude", b"info-ignored.txt\n")?;
    fixture.write("info-ignored.txt", b"ignored by git info exclude\n")?;
    fixture.write("private/hidden.rs", b"private\n")?;
    fixture.write("node_modules/pkg/index.js", b"vendor\n")?;
    fixture.write("target/debug/generated.rs", b"generated\n")?;
    fixture.write(".env", b"TOKEN=not-indexed\n")?;
    fixture.write(
        "notes/key-material.txt",
        b"-----BEGIN OPENSSH PRIVATE KEY-----\nnot-indexed\n",
    )?;
    fixture.write("assets/data.custom", b"text\0binary")?;
    fixture.create_sparse_file(
        "large.txt",
        DiscoveryPolicy::v1().max_file_bytes().saturating_add(1),
    )?;
    Ok(fixture)
}
