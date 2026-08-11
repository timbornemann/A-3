use a3_application::{
    JobCompletion, JobEventStream, JobSchedulerSubmitError, JobSubmitter, KnowledgeIndexStore,
    RefreshRepositoryIndex, RefreshRepositoryIndexError, RepositoryChangeBatch,
    RepositoryIndexCompilerFailure, RepositoryRescanReason,
};
use a3_domain::{JobId, JobOwner, JobStatus, ProjectIdentity};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    BuiltinIncrementalIndexCompilerCreateError, ParserPoolSize, ParserPoolSizeError,
    PollingRepositoryWatcher, RepositoryWatcherConfig, RepositoryWatcherStartError,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
use futures::executor::block_on;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const COORDINATOR_TICK: Duration = Duration::from_millis(20);
const INDEX_JOB_OWNER: JobOwner = JobOwner::new(1);

/// Owns active-project watching and translates bounded change batches into scheduler jobs.
pub(crate) struct RepositoryIndexManager {
    commands: Sender<ManagerCommand>,
    rebuild_state: Arc<Mutex<RepositoryIndexRebuildState>>,
    worker: Option<JoinHandle<()>>,
}

impl RepositoryIndexManager {
    pub(crate) fn start(
        submitter: JobSubmitter,
        events: JobEventStream,
        store: Arc<dyn KnowledgeIndexStore>,
    ) -> Result<Self, RepositoryIndexManagerStartError> {
        let (commands, receiver) = bounded(2);
        let rebuild_state = Arc::new(Mutex::new(RepositoryIndexRebuildState::Idle));
        let worker_rebuild_state = Arc::clone(&rebuild_state);
        let worker = thread::Builder::new()
            .name("a3-index-coordinator".to_owned())
            .spawn(move || {
                coordinator_loop(submitter, events, store, receiver, worker_rebuild_state);
            })
            .map_err(RepositoryIndexManagerStartError::WorkerSpawn)?;
        Ok(Self {
            commands,
            rebuild_state,
            worker: Some(worker),
        })
    }

    pub(crate) fn activate_project(
        &self,
        project: ProjectIdentity,
    ) -> Result<(), RepositoryIndexActivationError> {
        let watcher =
            PollingRepositoryWatcher::start(project.clone(), RepositoryWatcherConfig::v1())
                .map_err(RepositoryIndexActivationError::Watcher)?;
        let pool_size =
            ParserPoolSize::new(1).map_err(RepositoryIndexActivationError::ParserPoolSize)?;
        let compiler = BuiltinIncrementalIndexCompiler::new(pool_size)
            .map_err(RepositoryIndexActivationError::Compiler)?;
        let command = ManagerCommand::Activate(Box::new(ProjectActivation {
            project,
            watcher,
            compiler: Box::new(compiler),
        }));
        match self.commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(RepositoryIndexActivationError::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(RepositoryIndexActivationError::CoordinatorStopped)
            }
        }
    }

    pub(crate) fn request_rebuild(&self) -> Result<(), RepositoryIndexRebuildRequestError> {
        let (response, receiver) = bounded(1);
        match self.commands.try_send(ManagerCommand::Rebuild(response)) {
            Ok(()) => receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| RepositoryIndexRebuildRequestError::CoordinatorStopped)?,
            Err(TrySendError::Full(_)) => Err(RepositoryIndexRebuildRequestError::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(RepositoryIndexRebuildRequestError::CoordinatorStopped)
            }
        }
    }

    pub(crate) fn rebuild_state(&self) -> RepositoryIndexRebuildState {
        *lock_recovering_poison(&self.rebuild_state)
    }

    fn stop_and_join(&mut self) -> Result<(), RepositoryIndexManagerShutdownError> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.commands
            .send(ManagerCommand::Shutdown)
            .map_err(|_| RepositoryIndexManagerShutdownError::CoordinatorStopped)?;
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| RepositoryIndexManagerShutdownError::WorkerPanicked),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for RepositoryIndexManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepositoryIndexManager")
            .field("active", &self.worker.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for RepositoryIndexManager {
    fn drop(&mut self) {
        let _shutdown = self.stop_and_join();
    }
}

enum ManagerCommand {
    Activate(Box<ProjectActivation>),
    Rebuild(Sender<Result<(), RepositoryIndexRebuildRequestError>>),
    Shutdown,
}

struct ProjectActivation {
    project: ProjectIdentity,
    watcher: PollingRepositoryWatcher,
    compiler: Box<BuiltinIncrementalIndexCompiler>,
}

struct ActiveProject {
    project: ProjectIdentity,
    watcher: PollingRepositoryWatcher,
    compiler: Arc<Mutex<Box<BuiltinIncrementalIndexCompiler>>>,
    pending: Option<RepositoryChangeBatch>,
    active_job: Option<ManagedJob>,
    pending_rebuild: bool,
    watcher_failed: bool,
}

#[derive(Clone, Copy)]
struct ManagedJob {
    id: JobId,
    kind: ManagedJobKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManagedJobKind {
    Refresh,
    Rebuild,
}

fn coordinator_loop(
    submitter: JobSubmitter,
    events: JobEventStream,
    store: Arc<dyn KnowledgeIndexStore>,
    commands: Receiver<ManagerCommand>,
    rebuild_state: Arc<Mutex<RepositoryIndexRebuildState>>,
) {
    let refresh = Arc::new(RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        Arc::clone(&store),
        Arc::new(Blake3IndexRunIdFactory),
    ));
    let mut active: Option<ActiveProject> = None;
    let mut next_job_id = 1u64;

    loop {
        while events.try_next().ok().flatten().is_some() {}
        match commands.try_recv() {
            Ok(command) => {
                if handle_manager_command(command, &submitter, &mut active, &rebuild_state) {
                    return;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }

        let Some(state) = active.as_mut() else {
            if let Ok(command) = commands.recv_timeout(COORDINATOR_TICK)
                && handle_manager_command(command, &submitter, &mut active, &rebuild_state)
            {
                return;
            }
            continue;
        };

        if let Some(job) = state.active_job
            && let Some(snapshot) = submitter.snapshot(job.id)
        {
            if job.kind == ManagedJobKind::Rebuild && snapshot.status() == JobStatus::Running {
                set_rebuild_state(&rebuild_state, RepositoryIndexRebuildState::Running);
            }
            if snapshot.status().is_terminal() {
                if job.kind == ManagedJobKind::Rebuild {
                    let terminal = match snapshot.status() {
                        JobStatus::Succeeded => RepositoryIndexRebuildState::Succeeded,
                        JobStatus::Cancelled => RepositoryIndexRebuildState::Cancelled,
                        JobStatus::Failed => RepositoryIndexRebuildState::Failed,
                        JobStatus::Queued | JobStatus::Running | JobStatus::Cancelling => {
                            RepositoryIndexRebuildState::Failed
                        }
                    };
                    set_rebuild_state(&rebuild_state, terminal);
                    if terminal == RepositoryIndexRebuildState::Succeeded {
                        state.pending = RepositoryChangeBatch::full_rescan(
                            Vec::new(),
                            RepositoryRescanReason::Explicit,
                        )
                        .ok();
                    }
                }
                state.active_job = None;
            }
        }

        if state.active_job.is_none() && state.pending_rebuild {
            let job_id = JobId::new(next_job_id);
            next_job_id = next_job_id.saturating_add(1);
            let task_project = state.project.clone();
            let task_store = Arc::clone(&store);
            match submitter.submit(job_id, INDEX_JOB_OWNER, move |context| {
                completion_for_rebuild(block_on(
                    task_store.rebuild_regenerable_index(&task_project, &context),
                ))
            }) {
                Ok(()) => {
                    state.pending_rebuild = false;
                    state.active_job = Some(ManagedJob {
                        id: job_id,
                        kind: ManagedJobKind::Rebuild,
                    });
                }
                Err(_) => thread::sleep(COORDINATOR_TICK),
            }
        }

        if state.active_job.is_none()
            && !state.pending_rebuild
            && state.pending.is_none()
            && !state.watcher_failed
        {
            match state.watcher.next_batch(COORDINATOR_TICK) {
                Ok(batch) => state.pending = batch,
                Err(_) => {
                    state.watcher_failed = true;
                    state.pending = RepositoryChangeBatch::full_rescan(
                        Vec::new(),
                        RepositoryRescanReason::SourceUnavailable,
                    )
                    .ok();
                }
            }
        }

        if state.active_job.is_none()
            && let Some(batch) = state.pending.take()
        {
            let job_id = JobId::new(next_job_id);
            next_job_id = next_job_id.saturating_add(1);
            let task_project = state.project.clone();
            let task_compiler = Arc::clone(&state.compiler);
            let task_refresh = Arc::clone(&refresh);
            let fallback_paths = batch.paths().to_vec();
            match submitter.submit(job_id, INDEX_JOB_OWNER, move |context| {
                let mut compiler = lock_recovering_poison(&task_compiler);
                let result = block_on(task_refresh.execute(
                    &task_project,
                    &batch,
                    &mut **compiler,
                    &context,
                ));
                completion_for(result)
            }) {
                Ok(()) => {
                    state.active_job = Some(ManagedJob {
                        id: job_id,
                        kind: ManagedJobKind::Refresh,
                    });
                }
                Err(error) => {
                    state.pending = rescan_after_submit_failure(fallback_paths, error);
                    thread::sleep(COORDINATOR_TICK);
                }
            }
        }
    }
}

fn handle_manager_command(
    command: ManagerCommand,
    submitter: &JobSubmitter,
    active: &mut Option<ActiveProject>,
    rebuild_state: &Mutex<RepositoryIndexRebuildState>,
) -> bool {
    match command {
        ManagerCommand::Activate(activation) => {
            let retiring_job = active.as_ref().and_then(|state| state.active_job);
            if let Some(job) = retiring_job {
                let _cancellation = submitter.cancel(job.id);
            }
            if let Some(previous) = active.take() {
                let _shutdown = previous.watcher.shutdown();
            }
            *active = Some(ActiveProject {
                project: activation.project,
                watcher: activation.watcher,
                compiler: Arc::new(Mutex::new(activation.compiler)),
                pending: None,
                active_job: retiring_job.map(|job| ManagedJob {
                    id: job.id,
                    kind: ManagedJobKind::Refresh,
                }),
                pending_rebuild: false,
                watcher_failed: false,
            });
            set_rebuild_state(rebuild_state, RepositoryIndexRebuildState::Idle);
            false
        }
        ManagerCommand::Rebuild(response) => {
            let result = match active.as_mut() {
                None => Err(RepositoryIndexRebuildRequestError::NoActiveProject),
                Some(state)
                    if state.pending_rebuild
                        || state
                            .active_job
                            .is_some_and(|job| job.kind == ManagedJobKind::Rebuild) =>
                {
                    Err(RepositoryIndexRebuildRequestError::AlreadyPending)
                }
                Some(state) => {
                    if let Some(job) = state.active_job {
                        let _cancellation = submitter.cancel(job.id);
                    }
                    state.pending = None;
                    state.pending_rebuild = true;
                    set_rebuild_state(rebuild_state, RepositoryIndexRebuildState::Queued);
                    Ok(())
                }
            };
            let _response = response.send(result);
            false
        }
        ManagerCommand::Shutdown => {
            if let Some(state) = active.take() {
                if let Some(job) = state.active_job {
                    let _cancellation = submitter.cancel(job.id);
                }
                let _shutdown = state.watcher.shutdown();
            }
            true
        }
    }
}

fn rescan_after_submit_failure(
    paths: Vec<a3_domain::RepositoryPath>,
    _error: JobSchedulerSubmitError,
) -> Option<RepositoryChangeBatch> {
    RepositoryChangeBatch::full_rescan(paths, RepositoryRescanReason::EventLoss).ok()
}

fn completion_for(
    result: Result<a3_application::RepositoryIndexRefresh, RefreshRepositoryIndexError>,
) -> JobCompletion {
    match result {
        Ok(_) => JobCompletion::Succeeded,
        Err(RefreshRepositoryIndexError::Cancelled)
        | Err(RefreshRepositoryIndexError::Compiler(RepositoryIndexCompilerFailure::Cancelled))
        | Err(RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::Cancelled,
        ))
        | Err(RefreshRepositoryIndexError::Storage(
            a3_application::KnowledgeIndexFailure::Cancelled,
        )) => JobCompletion::Cancelled,
        Err(_) => JobCompletion::Failed,
    }
}

fn completion_for_rebuild(
    result: Result<(), a3_application::KnowledgeIndexFailure>,
) -> JobCompletion {
    match result {
        Ok(()) => JobCompletion::Succeeded,
        Err(a3_application::KnowledgeIndexFailure::Cancelled) => JobCompletion::Cancelled,
        Err(_) => JobCompletion::Failed,
    }
}

fn set_rebuild_state(
    state: &Mutex<RepositoryIndexRebuildState>,
    value: RepositoryIndexRebuildState,
) {
    *lock_recovering_poison(state) = value;
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Debug)]
pub(crate) enum RepositoryIndexManagerStartError {
    WorkerSpawn(std::io::Error),
}

impl fmt::Display for RepositoryIndexManagerStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository index coordinator could not be started")
    }
}

impl Error for RepositoryIndexManagerStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn(source) => Some(source),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexRebuildState {
    Idle,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexRebuildRequestError {
    NoActiveProject,
    AlreadyPending,
    QueueFull,
    CoordinatorStopped,
}

impl fmt::Display for RepositoryIndexRebuildRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository index rebuild request could not be accepted")
    }
}

impl Error for RepositoryIndexRebuildRequestError {}

#[derive(Debug)]
pub(crate) enum RepositoryIndexActivationError {
    Watcher(RepositoryWatcherStartError),
    ParserPoolSize(ParserPoolSizeError),
    Compiler(BuiltinIncrementalIndexCompilerCreateError),
    QueueFull,
    CoordinatorStopped,
}

impl fmt::Display for RepositoryIndexActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository indexing could not be activated")
    }
}

impl Error for RepositoryIndexActivationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Watcher(source) => Some(source),
            Self::ParserPoolSize(source) => Some(source),
            Self::Compiler(source) => Some(source),
            Self::QueueFull | Self::CoordinatorStopped => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexManagerShutdownError {
    CoordinatorStopped,
    WorkerPanicked,
}

impl fmt::Display for RepositoryIndexManagerShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository index coordinator shutdown failed")
    }
}

impl Error for RepositoryIndexManagerShutdownError {}

#[cfg(test)]
mod tests {
    use super::{
        ManagerCommand, ProjectActivation, RepositoryIndexRebuildRequestError,
        RepositoryIndexRebuildState, handle_manager_command,
    };
    use crate::clock::SystemJobClock;
    use a3_application::{JobScheduler, JobSchedulerConfig, ProjectInspector, ShutdownMode};
    use a3_repo_index::{
        BuiltinIncrementalIndexCompiler, ParserPoolSize, PollingRepositoryWatcher,
        RepositoryWatcherConfig,
    };
    use a3_workspace::RepositoryInspector;
    use crossbeam_channel::bounded;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn rebuild_request_without_an_active_project_is_rejected_before_scheduling()
    -> Result<(), Box<dyn std::error::Error>> {
        let config = JobSchedulerConfig::new(1, 1, 8)?;
        let (scheduler, _events) = JobScheduler::new(config, Arc::new(SystemJobClock::new()))?;
        let submitter = scheduler.submitter()?;
        let (response, receiver) = bounded(1);
        let mut active = None;
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);

        assert!(!handle_manager_command(
            ManagerCommand::Rebuild(response),
            &submitter,
            &mut active,
            &rebuild_state,
        ));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1))?,
            Err(RepositoryIndexRebuildRequestError::NoActiveProject)
        );
        assert_eq!(
            *rebuild_state
                .lock()
                .map_err(|_| std::io::Error::other("rebuild state mutex was poisoned"))?,
            RepositoryIndexRebuildState::Idle
        );

        scheduler.shutdown(ShutdownMode::CancelAndWait)?;
        Ok(())
    }

    #[test]
    fn rebuild_request_quiesces_refresh_and_enters_the_owned_queue()
    -> Result<(), Box<dyn std::error::Error>> {
        let config = JobSchedulerConfig::new(1, 1, 8)?;
        let (scheduler, _events) = JobScheduler::new(config, Arc::new(SystemJobClock::new()))?;
        let submitter = scheduler.submitter()?;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()?;
        let project = RepositoryInspector::new().inspect_project(&root)?;
        let activation = ProjectActivation {
            watcher: PollingRepositoryWatcher::start(
                project.clone(),
                RepositoryWatcherConfig::v1(),
            )?,
            project,
            compiler: Box::new(BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(
                1,
            )?)?),
        };
        let mut active = None;
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);

        assert!(!handle_manager_command(
            ManagerCommand::Activate(Box::new(activation)),
            &submitter,
            &mut active,
            &rebuild_state,
        ));
        let (response, receiver) = bounded(1);
        assert!(!handle_manager_command(
            ManagerCommand::Rebuild(response),
            &submitter,
            &mut active,
            &rebuild_state,
        ));

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?, Ok(()));
        assert!(active.as_ref().is_some_and(|state| state.pending_rebuild));
        assert_eq!(
            *rebuild_state
                .lock()
                .map_err(|_| std::io::Error::other("rebuild state mutex was poisoned"))?,
            RepositoryIndexRebuildState::Queued
        );
        assert!(handle_manager_command(
            ManagerCommand::Shutdown,
            &submitter,
            &mut active,
            &rebuild_state,
        ));

        scheduler.shutdown(ShutdownMode::CancelAndWait)?;
        Ok(())
    }
}
