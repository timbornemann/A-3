//! Native-platform smoke tests for the owned polling watcher lifecycle.

mod support;

use a3_application::RepositoryRescanReason;
use a3_repo_index::{PollingRepositoryWatcher, RepositoryWatcherConfig};
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::time::{Duration, Instant};
use support::TempDirectory;

#[test]
fn watcher_debounces_same_size_burst_and_joins_promptly() -> Result<(), Box<dyn Error>> {
    let repository = TempDirectory::new()?;
    repository.git(["init", "--initial-branch=main"])?;
    repository.write("src/lib.rs", b"pub fn first() {}\n")?;
    repository.git(["add", "."])?;
    let project = RepositoryInspector::new().inspect(repository.path())?;
    let config = RepositoryWatcherConfig::new(
        Duration::from_millis(20),
        Duration::from_millis(60),
        Duration::from_millis(250),
        2,
    )?;
    let watcher = PollingRepositoryWatcher::start(project, config)?;
    let initial = watcher
        .next_batch(Duration::from_secs(3))?
        .ok_or("initial watcher observation timed out")?;
    assert_eq!(
        initial.full_rescan_reason(),
        Some(RepositoryRescanReason::InitialObservation)
    );

    repository.write("src/lib.rs", b"pub fn secon() {}\n")?;
    std::thread::sleep(Duration::from_millis(25));
    repository.write("src/lib.rs", b"pub fn third() {}\n")?;
    std::thread::sleep(Duration::from_millis(25));
    repository.write("src/lib.rs", b"pub fn final() {}\n")?;
    let batch = watcher
        .next_batch(Duration::from_secs(3))?
        .ok_or("debounced watcher batch timed out")?;
    assert_eq!(batch.paths().len(), 1);
    assert_eq!(batch.paths()[0].as_bytes(), b"src/lib.rs");
    assert!(!batch.requires_full_rescan());

    let shutdown_started = Instant::now();
    watcher.shutdown()?;
    assert!(shutdown_started.elapsed() <= Duration::from_millis(500));
    Ok(())
}
