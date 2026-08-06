//! Application contracts for bounded durable remap queue reads.

use a3_application::{
    LoadPendingModuleRemaps, ModuleRemapQueueFailure, ModuleRemapQueueFuture,
    ModuleRemapQueueStore, PendingRemapQueue, RemapQueueControl, RemapQueueControlError,
    RemapQueueLimit,
};
use a3_domain::{
    CanonicalDirectory, GitHead, GitReferenceName, IndexRunId, InvalidationReason, ModuleCardId,
    ModuleId, Progress, ProjectIdentity, RemapPriority, RemapRequest, RepositoryId,
    RepositoryIdentity, SnapshotId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use futures::executor::block_on;
use std::error::Error;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn use_case_preserves_direct_before_dependent_order_and_progress() -> Result<(), Box<dyn Error>> {
    let project = project()?;
    let queue = queue()?;
    let store = StubStore {
        queue: queue.clone(),
        calls: AtomicUsize::new(0),
    };
    let control = TestControl::default();

    let loaded = block_on(LoadPendingModuleRemaps::new(&store).execute(
        &project,
        RemapQueueLimit::DEFAULT,
        &control,
    ))?;

    assert_eq!(loaded, queue);
    assert_eq!(loaded.entries()[0].priority(), RemapPriority::Direct);
    assert_eq!(loaded.entries()[1].priority(), RemapPriority::Dependent);
    assert_eq!(store.calls.load(Ordering::Acquire), 1);
    let progress = control
        .progress
        .lock()
        .map_err(|_| "progress lock was poisoned")?;
    assert_eq!(progress.len(), 2);
    assert_eq!(progress[0].completed(), Some(0));
    assert!(progress[1].is_complete());
    Ok(())
}

#[test]
fn cancellation_prevents_the_store_read() -> Result<(), Box<dyn Error>> {
    let project = project()?;
    let store = StubStore {
        queue: queue()?,
        calls: AtomicUsize::new(0),
    };
    let control = CancelledControl;

    let result = block_on(LoadPendingModuleRemaps::new(&store).execute(
        &project,
        RemapQueueLimit::new(1)?,
        &control,
    ));

    assert_eq!(result, Err(ModuleRemapQueueFailure::Cancelled));
    assert_eq!(store.calls.load(Ordering::Acquire), 0);
    Ok(())
}

#[test]
fn queue_rejects_the_same_module_at_two_priorities() -> Result<(), Box<dyn Error>> {
    let target_run = IndexRunId::from_bytes([61; 32]);
    let target_snapshot = SnapshotId::from_bytes([62; 32]);
    let module_id = ModuleId::from_bytes([63; 32]);
    let entries = vec![
        RemapRequest::from_persisted(
            IndexRunId::from_bytes([64; 32]),
            ModuleCardId::from_bytes([65; 32]),
            module_id,
            target_run,
            target_snapshot,
            RemapPriority::Direct,
            InvalidationReason::EvidenceChanged,
        )?,
        RemapRequest::from_persisted(
            IndexRunId::from_bytes([66; 32]),
            ModuleCardId::from_bytes([67; 32]),
            module_id,
            target_run,
            target_snapshot,
            RemapPriority::Dependent,
            InvalidationReason::DirectDependencyChanged,
        )?,
    ];

    assert!(PendingRemapQueue::new(target_run, target_snapshot, entries, false).is_err());
    Ok(())
}

#[derive(Debug)]
struct StubStore {
    queue: PendingRemapQueue,
    calls: AtomicUsize,
}

impl ModuleRemapQueueStore for StubStore {
    fn load_pending<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _limit: RemapQueueLimit,
        _control: &'a dyn RemapQueueControl,
    ) -> ModuleRemapQueueFuture<'a> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move { Ok(self.queue.clone()) })
    }
}

#[derive(Debug, Default)]
struct TestControl {
    progress: Mutex<Vec<Progress>>,
}

impl RemapQueueControl for TestControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RemapQueueControlError> {
        self.progress
            .lock()
            .map_err(|_| RemapQueueControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct CancelledControl;

impl RemapQueueControl for CancelledControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RemapQueueControlError> {
        Ok(())
    }
}

fn queue() -> Result<PendingRemapQueue, Box<dyn Error>> {
    let target_run = IndexRunId::from_bytes([11; 32]);
    let target_snapshot = SnapshotId::from_bytes([12; 32]);
    PendingRemapQueue::new(
        target_run,
        target_snapshot,
        vec![
            RemapRequest::from_persisted(
                IndexRunId::from_bytes([21; 32]),
                ModuleCardId::from_bytes([31; 32]),
                ModuleId::from_bytes([41; 32]),
                target_run,
                target_snapshot,
                RemapPriority::Direct,
                InvalidationReason::EvidenceChanged,
            )?,
            RemapRequest::from_persisted(
                IndexRunId::from_bytes([22; 32]),
                ModuleCardId::from_bytes([32; 32]),
                ModuleId::from_bytes([42; 32]),
                target_run,
                target_snapshot,
                RemapPriority::Dependent,
                InvalidationReason::DirectDependencyChanged,
            )?,
        ],
        false,
    )
    .map_err(Into::into)
}

fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(std::env::current_dir()?)?;
    let repository_id = RepositoryId::from_bytes([51; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository_id, root.clone(), None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([52; 32]),
            WorktreeAnchorId::from_bytes([53; 32]),
            repository_id,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}
