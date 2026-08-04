//! Reproducible manual performance baseline for the S3 discovery phase.

mod support;

use a3_application::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositorySnapshotBuild, RepositorySnapshotBuilder, RepositorySnapshotControl,
    RepositorySnapshotControlError, RepositorySnapshotPolicy, SnapshotBaseline,
    SnapshotCompatibility,
};
use a3_domain::{
    DiscoveryPolicy, IndexLanguage, IndexSchemaVersion, LanguageAdapterRevision,
    LanguageAdapterVersion, Progress, ProjectIdentity,
};
use a3_repo_index::{Blake3RepositorySnapshotBuilder, GitRepositoryDiscoverer};
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::time::Instant;
use support::TempDirectory;

const FILE_COUNT: usize = 200;
const LINES_PER_FILE: usize = 500;

#[derive(Debug)]
struct SilentControl;

impl RepositoryDiscoveryControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryDiscoveryControlError> {
        Ok(())
    }
}

impl RepositorySnapshotControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositorySnapshotControlError> {
        Ok(())
    }
}

#[test]
#[ignore = "manual 100,000-LOC discovery baseline"]
fn discover_mixed_100k_loc_fixture() -> Result<(), Box<dyn Error>> {
    let (_fixture, project) = mixed_100k_loc_fixture()?;

    let started = Instant::now();
    let result =
        GitRepositoryDiscoverer::new().discover(&project, DiscoveryPolicy::v1(), &SilentControl)?;
    let elapsed = started.elapsed();

    assert_eq!(result.files().len(), FILE_COUNT);
    println!(
        "A^3 S3 discovery baseline: {} files, {} LOC, {elapsed:?}",
        FILE_COUNT,
        FILE_COUNT.saturating_mul(LINES_PER_FILE)
    );
    Ok(())
}

#[test]
#[ignore = "manual 100,000-LOC discovery and hashing baseline"]
fn snapshot_mixed_100k_loc_fixture() -> Result<(), Box<dyn Error>> {
    let (_fixture, project) = mixed_100k_loc_fixture()?;
    let compatibility = SnapshotCompatibility::new(
        IndexSchemaVersion::new(1)?,
        vec![LanguageAdapterRevision::new(
            IndexLanguage::Generic,
            LanguageAdapterVersion::try_from_string("path-only-v1".to_owned())?,
        )],
    )?;

    let started = Instant::now();
    let result = Blake3RepositorySnapshotBuilder::new().build_snapshot(
        &project,
        &SnapshotBaseline::empty(),
        &compatibility,
        RepositorySnapshotPolicy::v1(),
        &SilentControl,
    )?;
    let elapsed = started.elapsed();

    assert!(matches!(result, RepositorySnapshotBuild::Created { .. }));
    println!(
        "A^3 S4 snapshot baseline: {} files, {} LOC, {elapsed:?}",
        FILE_COUNT,
        FILE_COUNT.saturating_mul(LINES_PER_FILE)
    );
    Ok(())
}

fn mixed_100k_loc_fixture() -> Result<(TempDirectory, ProjectIdentity), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    let source = (0..LINES_PER_FILE)
        .map(|line| format!("pub fn line_{line}() {{}}\n"))
        .collect::<String>();
    let mut git_add = vec!["add".to_owned()];
    for file_index in 0..FILE_COUNT {
        let path = format!("src/file_{file_index:03}.rs");
        fixture.write(&path, source.as_bytes())?;
        if file_index % 2 == 0 {
            git_add.push(path);
        }
    }
    fixture.git(git_add.iter().map(String::as_str))?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;
    Ok((fixture, project))
}
