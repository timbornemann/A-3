//! Reproducible manual performance baseline for the S3 discovery phase.

mod support;

use a3_application::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
};
use a3_domain::{DiscoveryPolicy, Progress};
use a3_repo_index::GitRepositoryDiscoverer;
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

#[test]
#[ignore = "manual 100,000-LOC discovery baseline"]
fn discover_mixed_100k_loc_fixture() -> Result<(), Box<dyn Error>> {
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
