use a3_application::{
    JobCompletion, JobEventStream, JobSchedulerSubmitError, JobSubmitter, KnowledgeIndexStore,
    RefreshRepositoryIndex, RefreshRepositoryIndexError, RepositoryChangeBatch,
    RepositoryIndexCompilerFailure, RepositoryRescanReason,
};
use a3_domain::{JobId, JobOwner, ProjectIdentity};
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
    worker: Option<JoinHandle<()>>,
}

impl RepositoryIndexManager {
    pub(crate) fn start(
        submitter: JobSubmitter,
        events: JobEventStream,
        store: Arc<dyn KnowledgeIndexStore>,
    ) -> Result<Self, RepositoryIndexManagerStartError> {
        let (commands, receiver) = bounded(2);
        let worker = thread::Builder::new()
            .name("a3-index-coordinator".to_owned())
            .spawn(move || coordinator_loop(submitter, events, store, receiver))
            .map_err(RepositoryIndexManagerStartError::WorkerSpawn)?;
        Ok(Self {
            commands,
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
    active_job: Option<JobId>,
    watcher_failed: bool,
}

fn coordinator_loop(
    submitter: JobSubmitter,
    events: JobEventStream,
    store: Arc<dyn KnowledgeIndexStore>,
    commands: Receiver<ManagerCommand>,
) {
    let refresh = Arc::new(RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store,
        Arc::new(Blake3IndexRunIdFactory),
    ));
    let mut active: Option<ActiveProject> = None;
    let mut next_job_id = 1u64;

    loop {
        while events.try_next().ok().flatten().is_some() {}
        match commands.try_recv() {
            Ok(ManagerCommand::Activate(activation)) => {
                let retiring_job = active.as_ref().and_then(|state| state.active_job);
                if let Some(job_id) = retiring_job {
                    let _cancellation = submitter.cancel(job_id);
                }
                if let Some(previous) = active.take() {
                    let _shutdown = previous.watcher.shutdown();
                }
                active = Some(ActiveProject {
                    project: activation.project,
                    watcher: activation.watcher,
                    compiler: Arc::new(Mutex::new(activation.compiler)),
                    pending: None,
                    active_job: retiring_job,
                    watcher_failed: false,
                });
            }
            Ok(ManagerCommand::Shutdown) => {
                if let Some(state) = active.take() {
                    if let Some(job_id) = state.active_job {
                        let _cancellation = submitter.cancel(job_id);
                    }
                    let _shutdown = state.watcher.shutdown();
                }
                return;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }

        let Some(state) = active.as_mut() else {
            if let Ok(command) = commands.recv_timeout(COORDINATOR_TICK) {
                if matches!(command, ManagerCommand::Shutdown) {
                    return;
                }
                handle_deferred_command(command, &mut active);
            }
            continue;
        };

        if let Some(job_id) = state.active_job
            && submitter
                .snapshot(job_id)
                .is_some_and(|snapshot| snapshot.status().is_terminal())
        {
            state.active_job = None;
        }

        if state.active_job.is_none() && state.pending.is_none() && !state.watcher_failed {
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
                Ok(()) => state.active_job = Some(job_id),
                Err(error) => {
                    state.pending = rescan_after_submit_failure(fallback_paths, error);
                    thread::sleep(COORDINATOR_TICK);
                }
            }
        }
    }
}

fn handle_deferred_command(command: ManagerCommand, active: &mut Option<ActiveProject>) {
    if let ManagerCommand::Activate(activation) = command {
        *active = Some(ActiveProject {
            project: activation.project,
            watcher: activation.watcher,
            compiler: Arc::new(Mutex::new(activation.compiler)),
            pending: None,
            active_job: None,
            watcher_failed: false,
        });
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
