//! Contract and regression tests for deterministic hashing and snapshot planning.

mod support;

use a3_application::{
    RepositorySnapshotBuild, RepositorySnapshotBuilder, RepositorySnapshotControl,
    RepositorySnapshotControlError, RepositorySnapshotFailure, RepositorySnapshotPolicy,
    SnapshotBaseline, SnapshotCompatibility,
};
use a3_domain::{
    FileDelta, GitHead, IndexLanguage, IndexSchemaVersion, LanguageAdapterRevision,
    LanguageAdapterVersion, Progress, SnapshotChangeKind,
};
use a3_repo_index::Blake3RepositorySnapshotBuilder;
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use support::TempDirectory;

#[derive(Debug, Default)]
struct TestControl {
    cancelled: AtomicBool,
    cancel_after_content_progress: bool,
    observations: Mutex<Vec<Progress>>,
}

impl TestControl {
    fn cancelling_during_hash() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            cancel_after_content_progress: true,
            observations: Mutex::new(Vec::new()),
        }
    }
}

impl RepositorySnapshotControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositorySnapshotControlError> {
        if self.cancel_after_content_progress && progress.completed().is_some_and(|value| value > 0)
        {
            self.cancelled.store(true, Ordering::Release);
        }
        self.observations
            .lock()
            .map_err(|_| RepositorySnapshotControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct IndexMutatingControl {
    root: PathBuf,
    mutated: AtomicBool,
}

impl RepositorySnapshotControl for IndexMutatingControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositorySnapshotControlError> {
        if progress.completed().is_some_and(|value| value > 0)
            && self
                .mutated
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            std::fs::write(self.root.join("late.bin"), [0_u8])
                .map_err(|_| RepositorySnapshotControlError::Unavailable)?;
            let status = Command::new("git")
                .args(["add", "late.bin"])
                .current_dir(&self.root)
                .status()
                .map_err(|_| RepositorySnapshotControlError::Unavailable)?;
            if !status.success() {
                return Err(RepositorySnapshotControlError::Unavailable);
            }
        }
        Ok(())
    }
}

#[test]
fn snapshot_contract_skips_mtime_and_tracks_head_content_rename_and_delete()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    fixture.git(["config", "user.name", "A3 Test"])?;
    fixture.git(["config", "user.email", "a3@example.invalid"])?;
    let first_content = b"pub fn a() {}\n";
    let second_content = b"pub fn b() {}\n";
    assert_eq!(first_content.len(), second_content.len());
    fixture.write("src/lib.rs", first_content)?;
    fixture.git(["add", "src/lib.rs"])?;
    fixture.git(["commit", "-m", "initial"])?;

    let project = RepositoryInspector::new().inspect(fixture.path())?;
    let builder = Blake3RepositorySnapshotBuilder::new();
    let compatibility = compatibility()?;
    let policy = RepositorySnapshotPolicy::v1();
    let first_build = builder.build_snapshot(
        &project,
        &SnapshotBaseline::empty(),
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    let (first_snapshot, first_files) = match first_build {
        RepositorySnapshotBuild::Created {
            snapshot,
            files,
            delta,
            ..
        } => {
            assert_eq!(snapshot.generation().get(), 1);
            assert_eq!(snapshot.head(), project.head());
            assert_eq!(delta.files().len(), 1);
            assert!(matches!(delta.files()[0], FileDelta::Added { .. }));
            (*snapshot, files)
        }
        RepositorySnapshotBuild::Unchanged { .. } => {
            return Err("first observation must create a snapshot".into());
        }
    };

    let repeated = builder.build_snapshot(
        &project,
        &SnapshotBaseline::empty(),
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    assert!(matches!(
        repeated,
        RepositorySnapshotBuild::Created { ref snapshot, .. } if snapshot.id() == first_snapshot.id()
    ));

    std::thread::sleep(Duration::from_millis(20));
    fixture.write("src/lib.rs", first_content)?;
    let first_baseline = SnapshotBaseline::new(Some(first_snapshot), first_files)?;
    assert!(matches!(
        builder.build_snapshot(
            &project,
            &first_baseline,
            &compatibility,
            policy,
            &TestControl::default(),
        )?,
        RepositorySnapshotBuild::Unchanged { .. }
    ));

    fixture.git(["commit", "--allow-empty", "-m", "head only"])?;
    let head_build = builder.build_snapshot(
        &project,
        &first_baseline,
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    let (head_snapshot, head_files) = created(head_build)?;
    assert_eq!(head_snapshot.generation().get(), 2);
    assert!(head_snapshot.changes().is_empty());
    assert_ne!(head_snapshot.head(), project.head());
    assert!(matches!(head_snapshot.head(), GitHead::Born { .. }));

    fixture.write("src/lib.rs", second_content)?;
    let modified_build = builder.build_snapshot(
        &project,
        &SnapshotBaseline::new(Some(head_snapshot), head_files)?,
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    let (modified_snapshot, modified_files, modified_delta) = created_with_delta(modified_build)?;
    assert_eq!(modified_snapshot.generation().get(), 3);
    assert!(matches!(
        modified_delta.files(),
        [FileDelta::Modified { .. }]
    ));
    assert_ne!(
        modified_delta.files()[0].previous_hash(),
        modified_delta.files()[0].current_hash()
    );

    std::fs::rename(
        fixture.path().join("src/lib.rs"),
        fixture.path().join("src/moved.rs"),
    )?;
    let rename_build = builder.build_snapshot(
        &project,
        &SnapshotBaseline::new(Some(modified_snapshot), modified_files)?,
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    let (rename_snapshot, rename_files, rename_delta) = created_with_delta(rename_build)?;
    assert_eq!(rename_snapshot.generation().get(), 4);
    assert_eq!(rename_delta.rename_candidates().len(), 1);
    assert_eq!(
        rename_delta.rename_candidates()[0].from().as_bytes(),
        b"src/lib.rs"
    );
    assert_eq!(
        rename_delta.rename_candidates()[0].to().as_bytes(),
        b"src/moved.rs"
    );
    assert_eq!(
        rename_snapshot
            .changes()
            .iter()
            .map(|change| change.kind())
            .collect::<Vec<_>>(),
        vec![SnapshotChangeKind::Delete, SnapshotChangeKind::Upsert]
    );

    std::fs::remove_file(fixture.path().join("src/moved.rs"))?;
    let deleted_build = builder.build_snapshot(
        &project,
        &SnapshotBaseline::new(Some(rename_snapshot), rename_files)?,
        &compatibility,
        policy,
        &TestControl::default(),
    )?;
    let (deleted_snapshot, deleted_files, deleted_delta) = created_with_delta(deleted_build)?;
    assert_eq!(deleted_snapshot.generation().get(), 5);
    assert!(deleted_files.is_empty());
    assert!(matches!(deleted_delta.files(), [FileDelta::Deleted { .. }]));
    assert!(deleted_delta.rename_candidates().is_empty());
    Ok(())
}

#[test]
fn hashing_observes_cancellation_between_bounded_reads() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    fixture.write("large.txt", vec![b'x'; 1024 * 1024])?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;
    let control = TestControl::cancelling_during_hash();

    assert_eq!(
        Blake3RepositorySnapshotBuilder::new().build_snapshot(
            &project,
            &SnapshotBaseline::empty(),
            &compatibility()?,
            RepositorySnapshotPolicy::v1(),
            &control,
        ),
        Err(RepositorySnapshotFailure::Cancelled)
    );
    let observations = control
        .observations
        .lock()
        .map_err(|_| "progress mutex poisoned")?;
    assert!(observations.iter().any(|progress| {
        progress
            .completed()
            .is_some_and(|completed| completed > 0 && !progress.is_complete())
    }));
    Ok(())
}

#[test]
fn index_change_during_hashing_discards_the_observation() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    fixture.git(["init", "--initial-branch=main"])?;
    fixture.write("source.txt", vec![b'x'; 1024 * 1024])?;
    let project = RepositoryInspector::new().inspect(fixture.path())?;
    let control = IndexMutatingControl {
        root: fixture.path().to_path_buf(),
        mutated: AtomicBool::new(false),
    };

    assert_eq!(
        Blake3RepositorySnapshotBuilder::new().build_snapshot(
            &project,
            &SnapshotBaseline::empty(),
            &compatibility()?,
            RepositorySnapshotPolicy::v1(),
            &control,
        ),
        Err(RepositorySnapshotFailure::WorktreeChanged)
    );
    assert!(control.mutated.load(Ordering::Acquire));
    Ok(())
}

fn compatibility() -> Result<SnapshotCompatibility, Box<dyn Error>> {
    Ok(SnapshotCompatibility::new(
        IndexSchemaVersion::new(1)?,
        vec![LanguageAdapterRevision::new(
            IndexLanguage::Generic,
            LanguageAdapterVersion::try_from_string("generic-v1".to_owned())?,
        )],
    )?)
}

fn created(
    build: RepositorySnapshotBuild,
) -> Result<(a3_domain::Snapshot, a3_domain::RepositoryFileState), Box<dyn Error>> {
    match build {
        RepositorySnapshotBuild::Created {
            snapshot, files, ..
        } => Ok((*snapshot, files)),
        RepositorySnapshotBuild::Unchanged { .. } => Err("expected a new snapshot".into()),
    }
}

fn created_with_delta(
    build: RepositorySnapshotBuild,
) -> Result<
    (
        a3_domain::Snapshot,
        a3_domain::RepositoryFileState,
        a3_domain::SnapshotDelta,
    ),
    Box<dyn Error>,
> {
    match build {
        RepositorySnapshotBuild::Created {
            snapshot,
            files,
            delta,
            ..
        } => Ok((*snapshot, files, delta)),
        RepositorySnapshotBuild::Unchanged { .. } => Err("expected a new snapshot".into()),
    }
}
