use crate::index_trace_runtime::{IndexTraceRuntime, now};
use crate::job_ids::DesktopJobIds;
use a3_application::{
    IndexTraceFilePage, IndexTraceFileQuery, IndexTraceStore, JobCompletion, JobContext,
    JobEventStream, JobSchedulerSubmitError, JobSubmitter, KnowledgeIndexStore,
    RefreshRepositoryIndex, RefreshRepositoryIndexError, RepositoryChangeBatch,
    RepositoryIndexCompilerFailure, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryIndexObservation, RepositoryIndexPhase, RepositoryRescanReason, RetainedIndexTraces,
};
use a3_domain::{
    IndexTraceDiagnosticCode, IndexTraceId, IndexTraceRevision, IndexTraceState, IndexTraceTrigger,
    JobId, JobOwner, JobStatus, Progress, ProjectIdentity,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    BuiltinIncrementalIndexCompilerCreateError, ParserPoolSize, ParserPoolSizeError,
    PollingRepositoryWatcher, RepositoryWatcherConfig, RepositoryWatcherStartError,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::async_runtime::block_on;

const COORDINATOR_TICK: Duration = Duration::from_millis(20);
const INDEX_JOB_OWNER: JobOwner = JobOwner::new(1);

/// Owns active-project watching and translates bounded change batches into scheduler jobs.
pub(crate) struct RepositoryIndexManager {
    commands: Sender<ManagerCommand>,
    activity: Arc<Mutex<RepositoryIndexActivity>>,
    rebuild_state: Arc<Mutex<RepositoryIndexRebuildState>>,
    trace_runtime: Arc<Mutex<IndexTraceRuntime>>,
    worker: Option<JoinHandle<()>>,
}

impl RepositoryIndexManager {
    pub(crate) fn start(
        submitter: JobSubmitter,
        events: JobEventStream,
        store: Arc<dyn KnowledgeIndexStore>,
        trace_store: Arc<dyn IndexTraceStore>,
        job_ids: Arc<DesktopJobIds>,
    ) -> Result<Self, RepositoryIndexManagerStartError> {
        let (commands, receiver) = bounded(2);
        let activity = Arc::new(Mutex::new(RepositoryIndexActivity::idle()));
        let rebuild_state = Arc::new(Mutex::new(RepositoryIndexRebuildState::Idle));
        let trace_runtime = Arc::new(Mutex::new(IndexTraceRuntime::default()));
        let worker_activity = Arc::clone(&activity);
        let worker_rebuild_state = Arc::clone(&rebuild_state);
        let worker_trace_runtime = Arc::clone(&trace_runtime);
        let views = RepositoryIndexCoordinatorViews {
            activity: worker_activity,
            rebuild_state: worker_rebuild_state,
            trace_runtime: worker_trace_runtime,
        };
        let worker = thread::Builder::new()
            .name("a3-index-coordinator".to_owned())
            .spawn(move || {
                coordinator_loop(
                    submitter,
                    events,
                    store,
                    trace_store,
                    job_ids,
                    receiver,
                    views,
                );
            })
            .map_err(RepositoryIndexManagerStartError::WorkerSpawn)?;
        Ok(Self {
            commands,
            activity,
            rebuild_state,
            trace_runtime,
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

    pub(crate) fn deactivate_project(&self) -> Result<(), RepositoryIndexDeactivationError> {
        let (response, receiver) = bounded(1);
        match self.commands.try_send(ManagerCommand::Deactivate(response)) {
            Ok(()) => receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| RepositoryIndexDeactivationError::CoordinatorStopped)?,
            Err(TrySendError::Full(_)) => Err(RepositoryIndexDeactivationError::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(RepositoryIndexDeactivationError::CoordinatorStopped)
            }
        }
    }

    pub(crate) fn rebuild_state(&self) -> RepositoryIndexRebuildState {
        *lock_recovering_poison(&self.rebuild_state)
    }

    pub(crate) fn activity(&self) -> RepositoryIndexActivity {
        *lock_recovering_poison(&self.activity)
    }

    pub(crate) fn traces(&self, worktree_id: a3_domain::WorktreeId) -> RetainedIndexTraces {
        lock_recovering_poison(&self.trace_runtime).retained_for(worktree_id)
    }

    pub(crate) fn query_trace_files(
        &self,
        worktree_id: a3_domain::WorktreeId,
        query: &IndexTraceFileQuery,
    ) -> Result<IndexTraceFilePage, a3_application::IndexTraceStoreFailure> {
        lock_recovering_poison(&self.trace_runtime).query_files(worktree_id, query)
    }

    pub(crate) fn cancel_trace(
        &self,
        trace_id: IndexTraceId,
        revision: IndexTraceRevision,
    ) -> Result<(), RepositoryIndexTraceControlError> {
        let (response, receiver) = bounded(1);
        self.commands
            .try_send(ManagerCommand::CancelTrace {
                trace_id,
                revision,
                response,
            })
            .map_err(map_trace_control_send)?;
        receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| RepositoryIndexTraceControlError::CoordinatorStopped)?
    }

    pub(crate) fn retry_trace(
        &self,
        trace_id: IndexTraceId,
        revision: IndexTraceRevision,
    ) -> Result<(), RepositoryIndexTraceControlError> {
        let (response, receiver) = bounded(1);
        self.commands
            .try_send(ManagerCommand::RetryTrace {
                trace_id,
                revision,
                response,
            })
            .map_err(map_trace_control_send)?;
        receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| RepositoryIndexTraceControlError::CoordinatorStopped)?
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
    Deactivate(Sender<Result<(), RepositoryIndexDeactivationError>>),
    Rebuild(Sender<Result<(), RepositoryIndexRebuildRequestError>>),
    CancelTrace {
        trace_id: IndexTraceId,
        revision: IndexTraceRevision,
        response: Sender<Result<(), RepositoryIndexTraceControlError>>,
    },
    RetryTrace {
        trace_id: IndexTraceId,
        revision: IndexTraceRevision,
        response: Sender<Result<(), RepositoryIndexTraceControlError>>,
    },
    Shutdown,
}

struct ProjectActivation {
    project: ProjectIdentity,
    watcher: PollingRepositoryWatcher,
    compiler: Box<BuiltinIncrementalIndexCompiler>,
}

struct ActiveProject {
    project: ProjectIdentity,
    watcher: Option<PollingRepositoryWatcher>,
    compiler: Arc<Mutex<Box<BuiltinIncrementalIndexCompiler>>>,
    pending: Option<RepositoryChangeBatch>,
    active_job: Option<ManagedJob>,
    pending_rebuild: bool,
    watcher_failed: bool,
    deactivated: bool,
    pending_trigger: Option<IndexTraceTrigger>,
}

#[derive(Clone, Copy)]
struct ManagedJob {
    id: JobId,
    kind: ManagedJobKind,
    trace_id: Option<IndexTraceId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManagedJobKind {
    Refresh,
    Rebuild,
}

struct RepositoryIndexCoordinatorViews {
    activity: Arc<Mutex<RepositoryIndexActivity>>,
    rebuild_state: Arc<Mutex<RepositoryIndexRebuildState>>,
    trace_runtime: Arc<Mutex<IndexTraceRuntime>>,
}

fn coordinator_loop(
    submitter: JobSubmitter,
    events: JobEventStream,
    store: Arc<dyn KnowledgeIndexStore>,
    trace_store: Arc<dyn IndexTraceStore>,
    job_ids: Arc<DesktopJobIds>,
    commands: Receiver<ManagerCommand>,
    views: RepositoryIndexCoordinatorViews,
) {
    let RepositoryIndexCoordinatorViews {
        activity,
        rebuild_state,
        trace_runtime,
    } = views;
    let refresh = Arc::new(RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        Arc::clone(&store),
        Arc::new(Blake3IndexRunIdFactory),
    ));
    let mut active: Option<ActiveProject> = None;
    loop {
        while events.try_next().ok().flatten().is_some() {}
        match commands.try_recv() {
            Ok(command) => {
                prepare_trace_for_command(&command, trace_store.as_ref(), &trace_runtime);
                if handle_manager_command(
                    command,
                    &submitter,
                    &mut active,
                    &activity,
                    &rebuild_state,
                    &trace_runtime,
                ) {
                    return;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }

        let Some(state) = active.as_mut() else {
            if let Ok(command) = commands.recv_timeout(COORDINATOR_TICK) {
                prepare_trace_for_command(&command, trace_store.as_ref(), &trace_runtime);
                if handle_manager_command(
                    command,
                    &submitter,
                    &mut active,
                    &activity,
                    &rebuild_state,
                    &trace_runtime,
                ) {
                    return;
                }
            }
            continue;
        };

        if let Some(job) = state.active_job
            && let Some(snapshot) = submitter.snapshot(job.id)
        {
            if job.kind == ManagedJobKind::Refresh {
                set_index_activity_from_job(&activity, snapshot.status(), snapshot.progress());
                let (state, diagnostic) = trace_outcome_from_job_status(snapshot.status());
                if let Some(trace_id) = job.trace_id {
                    lock_recovering_poison(&trace_runtime)
                        .set_state_for(trace_id, state, diagnostic);
                }
            }
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

        persist_trace_checkpoint(trace_store.as_ref(), &state.project, &trace_runtime);

        if state.deactivated {
            if state.active_job.is_none() {
                set_index_activity(&activity, RepositoryIndexActivity::idle());
                active = None;
            }
            thread::sleep(COORDINATOR_TICK);
            continue;
        }

        if state.active_job.is_none() && state.pending_rebuild {
            let Ok(job_id) = job_ids.allocate() else {
                state.pending_rebuild = false;
                set_rebuild_state(&rebuild_state, RepositoryIndexRebuildState::Failed);
                continue;
            };
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
                        trace_id: None,
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
            let Some(watcher) = state.watcher.as_mut() else {
                state.watcher_failed = true;
                continue;
            };
            match watcher.next_batch(COORDINATOR_TICK) {
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
            let Ok(job_id) = job_ids.allocate() else {
                set_index_activity_from_job(&activity, JobStatus::Failed, None);
                continue;
            };
            let task_project = state.project.clone();
            let task_compiler = Arc::clone(&state.compiler);
            let task_refresh = Arc::clone(&refresh);
            let task_trace_runtime = Arc::clone(&trace_runtime);
            let trigger = match state.pending_trigger.take() {
                Some(trigger) => trigger,
                None => trace_trigger(&batch),
            };
            let fallback_paths = batch.paths().to_vec();
            let previous_publication_available =
                block_on(store.latest_published_index_run(&state.project))
                    .ok()
                    .flatten()
                    .is_some();
            let initial_trace = lock_recovering_poison(&trace_runtime).begin(
                &state.project,
                job_id,
                trigger,
                previous_publication_available,
            );
            let trace_id = initial_trace.as_ref().map(|trace| trace.summary().id());
            if let Some(initial_trace) = initial_trace {
                if block_on(trace_store.create_trace(&state.project, &initial_trace)).is_ok() {
                    lock_recovering_poison(&trace_runtime).mark_created();
                } else {
                    lock_recovering_poison(&trace_runtime).mark_details_incomplete();
                }
            }
            match submitter.submit(job_id, INDEX_JOB_OWNER, move |context| {
                let mut compiler = lock_recovering_poison(&task_compiler);
                let trace_control = TraceControl {
                    context: &context,
                    runtime: &task_trace_runtime,
                    trace_id,
                };
                let result = block_on(task_refresh.execute(
                    &task_project,
                    &batch,
                    &mut **compiler,
                    &trace_control,
                ));
                let (completion, state, diagnostic) = completion_and_trace_outcome(&result);
                if let Some(trace_id) = trace_id {
                    lock_recovering_poison(&task_trace_runtime)
                        .set_state_for(trace_id, state, diagnostic);
                }
                completion
            }) {
                Ok(()) => {
                    set_index_activity(
                        &activity,
                        RepositoryIndexActivity::queued(RepositoryIndexPhase::Discover),
                    );
                    state.active_job = Some(ManagedJob {
                        id: job_id,
                        kind: ManagedJobKind::Refresh,
                        trace_id,
                    });
                }
                Err(error) => {
                    if let Some(trace_id) = trace_id {
                        lock_recovering_poison(&trace_runtime).set_state_for(
                            trace_id,
                            IndexTraceState::Failed,
                            Some(IndexTraceDiagnosticCode::WorkerUnavailable),
                        );
                        persist_trace_checkpoint(
                            trace_store.as_ref(),
                            &state.project,
                            &trace_runtime,
                        );
                    }
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
    activity: &Mutex<RepositoryIndexActivity>,
    rebuild_state: &Mutex<RepositoryIndexRebuildState>,
    trace_runtime: &Mutex<IndexTraceRuntime>,
) -> bool {
    match command {
        ManagerCommand::Activate(activation) => {
            let retiring_job = active.as_ref().and_then(|state| state.active_job);
            if let Some(job) = retiring_job {
                let _cancellation = submitter.cancel(job.id);
            }
            if let Some(mut previous) = active.take()
                && let Some(watcher) = previous.watcher.take()
            {
                let _shutdown = watcher.shutdown();
            }
            *active = Some(ActiveProject {
                project: activation.project,
                watcher: Some(activation.watcher),
                compiler: Arc::new(Mutex::new(activation.compiler)),
                pending: None,
                active_job: retiring_job.map(|job| ManagedJob {
                    id: job.id,
                    kind: ManagedJobKind::Refresh,
                    trace_id: job.trace_id,
                }),
                pending_rebuild: false,
                watcher_failed: false,
                deactivated: false,
                pending_trigger: None,
            });
            set_index_activity(activity, RepositoryIndexActivity::idle());
            set_rebuild_state(rebuild_state, RepositoryIndexRebuildState::Idle);
            false
        }
        ManagerCommand::Deactivate(response) => {
            let result = match active.as_mut() {
                None => Err(RepositoryIndexDeactivationError::NoActiveProject),
                Some(state) if state.deactivated => {
                    Err(RepositoryIndexDeactivationError::AlreadyPending)
                }
                Some(state) => {
                    if let Some(job) = state.active_job {
                        let _cancellation = submitter.cancel(job.id);
                    }
                    state.pending = None;
                    state.pending_rebuild = false;
                    state.watcher_failed = true;
                    state.deactivated = true;
                    if state.active_job.is_none() {
                        set_index_activity(activity, RepositoryIndexActivity::idle());
                    }
                    set_rebuild_state(rebuild_state, RepositoryIndexRebuildState::Idle);
                    match state.watcher.take() {
                        Some(watcher) => watcher
                            .shutdown()
                            .map_err(|_| RepositoryIndexDeactivationError::WatcherShutdown),
                        None => Ok(()),
                    }
                }
            };
            let _response = response.send(result);
            false
        }
        ManagerCommand::Rebuild(response) => {
            let result = match active.as_mut() {
                None => Err(RepositoryIndexRebuildRequestError::NoActiveProject),
                Some(state) if state.deactivated => {
                    Err(RepositoryIndexRebuildRequestError::NoActiveProject)
                }
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
        ManagerCommand::CancelTrace {
            trace_id,
            revision,
            response,
        } => {
            let result =
                validate_trace_control(active.as_ref(), trace_runtime, trace_id, revision, false)
                    .and_then(|job| job.ok_or(RepositoryIndexTraceControlError::InvalidState))
                    .and_then(|job| {
                        submitter
                            .cancel(job.id)
                            .map(|_| ())
                            .map_err(|_| RepositoryIndexTraceControlError::CoordinatorStopped)
                    });
            if result.is_ok() {
                lock_recovering_poison(trace_runtime).set_state(IndexTraceState::Cancelling, None);
            }
            let _response = response.send(result);
            false
        }
        ManagerCommand::RetryTrace {
            trace_id,
            revision,
            response,
        } => {
            let result =
                validate_trace_control(active.as_ref(), trace_runtime, trace_id, revision, true)
                    .and_then(|_| {
                        let state = active
                            .as_mut()
                            .ok_or(RepositoryIndexTraceControlError::NoActiveProject)?;
                        if state.active_job.is_some()
                            || state.pending.is_some()
                            || state.pending_rebuild
                        {
                            return Err(RepositoryIndexTraceControlError::Busy);
                        }
                        let retry = RepositoryChangeBatch::full_rescan(
                            Vec::new(),
                            RepositoryRescanReason::Explicit,
                        )
                        .map_err(|_| RepositoryIndexTraceControlError::CoordinatorStopped)?;
                        state.pending = Some(retry);
                        state.pending_trigger = Some(IndexTraceTrigger::ManualRetry);
                        Ok(())
                    });
            let _response = response.send(result);
            false
        }
        ManagerCommand::Shutdown => {
            if let Some(mut state) = active.take() {
                if let Some(job) = state.active_job {
                    let _cancellation = submitter.cancel(job.id);
                }
                if let Some(watcher) = state.watcher.take() {
                    let _shutdown = watcher.shutdown();
                }
            }
            set_index_activity(activity, RepositoryIndexActivity::idle());
            true
        }
    }
}

fn prepare_trace_for_command(
    command: &ManagerCommand,
    store: &dyn IndexTraceStore,
    runtime: &Mutex<IndexTraceRuntime>,
) {
    let ManagerCommand::Activate(activation) = command else {
        return;
    };
    let loaded = block_on(async {
        store
            .reconcile_interrupted(&activation.project, now())
            .await?;
        store.load_retained(&activation.project).await
    });
    match loaded {
        Ok(retained) => {
            lock_recovering_poison(runtime).replace_loaded(&activation.project, retained);
        }
        Err(_) => {
            lock_recovering_poison(runtime)
                .replace_loaded(&activation.project, RetainedIndexTraces::default());
        }
    }
}

fn persist_trace_checkpoint(
    store: &dyn IndexTraceStore,
    project: &ProjectIdentity,
    runtime: &Mutex<IndexTraceRuntime>,
) {
    let checkpoint = lock_recovering_poison(runtime).checkpoint();
    let Some(checkpoint) = checkpoint else {
        return;
    };
    if block_on(store.checkpoint_trace(project, &checkpoint)).is_ok() {
        lock_recovering_poison(runtime).acknowledge(&checkpoint);
    } else {
        lock_recovering_poison(runtime).mark_details_incomplete();
    }
}

fn trace_trigger(batch: &RepositoryChangeBatch) -> IndexTraceTrigger {
    match batch.full_rescan_reason() {
        Some(RepositoryRescanReason::InitialObservation) => IndexTraceTrigger::InitialObservation,
        Some(
            RepositoryRescanReason::EventLoss
            | RepositoryRescanReason::RepositoryMetadataChanged
            | RepositoryRescanReason::SourceUnavailable
            | RepositoryRescanReason::Explicit,
        ) => IndexTraceTrigger::RecoveryRescan,
        None => IndexTraceTrigger::FileChanges,
    }
}

fn validate_trace_control(
    active: Option<&ActiveProject>,
    runtime: &Mutex<IndexTraceRuntime>,
    trace_id: IndexTraceId,
    revision: IndexTraceRevision,
    retry: bool,
) -> Result<Option<ManagedJob>, RepositoryIndexTraceControlError> {
    let state = active.ok_or(RepositoryIndexTraceControlError::NoActiveProject)?;
    if state.deactivated {
        return Err(RepositoryIndexTraceControlError::NoActiveProject);
    }
    let retained = lock_recovering_poison(runtime).retained_for(state.project.worktree().id());
    let trace = retained
        .current
        .filter(|trace| trace.summary().id() == trace_id)
        .ok_or(RepositoryIndexTraceControlError::UnknownTrace)?;
    if trace.summary().revision() != revision {
        return Err(RepositoryIndexTraceControlError::StaleRevision);
    }
    if retry {
        if !trace.summary().state().is_terminal() {
            return Err(RepositoryIndexTraceControlError::InvalidState);
        }
        Ok(None)
    } else {
        let job = state
            .active_job
            .filter(|job| job.trace_id == Some(trace_id))
            .ok_or(RepositoryIndexTraceControlError::InvalidState)?;
        if trace.summary().state().is_terminal() {
            return Err(RepositoryIndexTraceControlError::InvalidState);
        }
        Ok(Some(job))
    }
}

fn map_trace_control_send(error: TrySendError<ManagerCommand>) -> RepositoryIndexTraceControlError {
    match error {
        TrySendError::Full(_) => RepositoryIndexTraceControlError::Busy,
        TrySendError::Disconnected(_) => RepositoryIndexTraceControlError::CoordinatorStopped,
    }
}

struct TraceControl<'a> {
    context: &'a JobContext,
    runtime: &'a Mutex<IndexTraceRuntime>,
    trace_id: Option<IndexTraceId>,
}

impl fmt::Debug for TraceControl<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TraceControl")
    }
}

impl RepositoryIndexControl for TraceControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryIndexControlError> {
        self.context
            .report_progress(progress)
            .map_err(|_| RepositoryIndexControlError::Unavailable)
    }

    fn observe(&self, observation: RepositoryIndexObservation) {
        if let Some(trace_id) = self.trace_id {
            lock_recovering_poison(self.runtime).observe_for(trace_id, observation);
        }
    }
}

fn rescan_after_submit_failure(
    paths: Vec<a3_domain::RepositoryPath>,
    _error: JobSchedulerSubmitError,
) -> Option<RepositoryChangeBatch> {
    RepositoryChangeBatch::full_rescan(paths, RepositoryRescanReason::EventLoss).ok()
}

fn completion_and_trace_outcome(
    result: &Result<a3_application::RepositoryIndexRefresh, RefreshRepositoryIndexError>,
) -> (
    JobCompletion,
    IndexTraceState,
    Option<IndexTraceDiagnosticCode>,
) {
    match result {
        Ok(_) => (JobCompletion::Succeeded, IndexTraceState::Succeeded, None),
        Err(RefreshRepositoryIndexError::Cancelled)
        | Err(RefreshRepositoryIndexError::Compiler(RepositoryIndexCompilerFailure::Cancelled))
        | Err(RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::Cancelled,
        ))
        | Err(RefreshRepositoryIndexError::Storage(
            a3_application::KnowledgeIndexFailure::Cancelled,
        )) => (JobCompletion::Cancelled, IndexTraceState::Cancelled, None),
        Err(error) => (
            JobCompletion::Failed,
            IndexTraceState::Failed,
            Some(trace_diagnostic(error)),
        ),
    }
}

fn trace_outcome_from_job_status(
    status: JobStatus,
) -> (IndexTraceState, Option<IndexTraceDiagnosticCode>) {
    match status {
        JobStatus::Queued => (IndexTraceState::Queued, None),
        JobStatus::Running => (IndexTraceState::Running, None),
        JobStatus::Cancelling => (IndexTraceState::Cancelling, None),
        JobStatus::Succeeded => (IndexTraceState::Succeeded, None),
        JobStatus::Failed => (
            IndexTraceState::Failed,
            Some(IndexTraceDiagnosticCode::WorkerUnavailable),
        ),
        JobStatus::Cancelled => (IndexTraceState::Cancelled, None),
    }
}

fn trace_diagnostic(error: &RefreshRepositoryIndexError) -> IndexTraceDiagnosticCode {
    match error {
        RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::Discovery,
        )
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::InvalidRepository,
        ) => IndexTraceDiagnosticCode::Discovery,
        RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::Filesystem,
        )
        | RefreshRepositoryIndexError::Compiler(RepositoryIndexCompilerFailure::Filesystem) => {
            IndexTraceDiagnosticCode::SourceUnavailable
        }
        RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::WorktreeChanged,
        )
        | RefreshRepositoryIndexError::Compiler(RepositoryIndexCompilerFailure::RevisionMismatch) => {
            IndexTraceDiagnosticCode::RevisionChanged
        }
        RefreshRepositoryIndexError::Compiler(RepositoryIndexCompilerFailure::TimedOut)
        | RefreshRepositoryIndexError::Storage(a3_application::KnowledgeIndexFailure::TimedOut) => {
            IndexTraceDiagnosticCode::Timeout
        }
        RefreshRepositoryIndexError::Compiler(
            RepositoryIndexCompilerFailure::ResourceLimitExceeded,
        )
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::ResourceLimitExceeded,
        ) => IndexTraceDiagnosticCode::ResourceLimit,
        RefreshRepositoryIndexError::ProgressUnavailable
        | RefreshRepositoryIndexError::Compiler(
            RepositoryIndexCompilerFailure::ProgressUnavailable,
        )
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::ProgressUnavailable,
        )
        | RefreshRepositoryIndexError::Storage(
            a3_application::KnowledgeIndexFailure::ProgressUnavailable,
        ) => IndexTraceDiagnosticCode::ProgressUnavailable,
        RefreshRepositoryIndexError::Storage(_) => IndexTraceDiagnosticCode::Publish,
        RefreshRepositoryIndexError::Compiler(_) => IndexTraceDiagnosticCode::Parse,
        RefreshRepositoryIndexError::InvalidBaseline
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::IdentityMismatch,
        )
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::InvalidSnapshot,
        )
        | RefreshRepositoryIndexError::RunIdentity(_)
        | RefreshRepositoryIndexError::AttemptExhausted => {
            IndexTraceDiagnosticCode::WorkerUnavailable
        }
        RefreshRepositoryIndexError::Cancelled
        | RefreshRepositoryIndexError::Snapshot(
            a3_application::RepositorySnapshotFailure::Cancelled,
        ) => IndexTraceDiagnosticCode::WorkerUnavailable,
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

fn set_index_activity(state: &Mutex<RepositoryIndexActivity>, value: RepositoryIndexActivity) {
    *lock_recovering_poison(state) = value;
}

fn set_index_activity_from_job(
    state: &Mutex<RepositoryIndexActivity>,
    status: JobStatus,
    progress: Option<Progress>,
) {
    let previous = *lock_recovering_poison(state);
    let (phase, completed) = match progress.and_then(index_phase_from_progress) {
        Some(current) => current,
        None => (previous.phase, previous.completed),
    };
    set_index_activity(
        state,
        RepositoryIndexActivity {
            state: match status {
                JobStatus::Queued => RepositoryIndexActivityState::Queued,
                JobStatus::Running => RepositoryIndexActivityState::Running,
                JobStatus::Cancelling => RepositoryIndexActivityState::Cancelling,
                JobStatus::Succeeded => RepositoryIndexActivityState::Succeeded,
                JobStatus::Failed => RepositoryIndexActivityState::Failed,
                JobStatus::Cancelled => RepositoryIndexActivityState::Cancelled,
            },
            phase,
            completed,
        },
    );
}

fn index_phase_from_progress(progress: Progress) -> Option<(Option<RepositoryIndexPhase>, u64)> {
    let completed = progress.completed()?;
    if progress.total() != Some(RepositoryIndexActivity::TOTAL_PHASES) {
        return None;
    }
    let phase = match completed {
        0 => RepositoryIndexPhase::Discover,
        1 => RepositoryIndexPhase::Hash,
        2 => RepositoryIndexPhase::Parse,
        3 => RepositoryIndexPhase::Link,
        4 => RepositoryIndexPhase::Rank,
        5 | 6 => RepositoryIndexPhase::Publish,
        _ => return None,
    };
    Some((Some(phase), completed))
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
pub(crate) struct RepositoryIndexActivity {
    state: RepositoryIndexActivityState,
    phase: Option<RepositoryIndexPhase>,
    completed: u64,
}

impl RepositoryIndexActivity {
    pub(crate) const TOTAL_PHASES: u64 = 6;

    pub(crate) const fn idle() -> Self {
        Self {
            state: RepositoryIndexActivityState::Idle,
            phase: None,
            completed: 0,
        }
    }

    const fn queued(phase: RepositoryIndexPhase) -> Self {
        Self {
            state: RepositoryIndexActivityState::Queued,
            phase: Some(phase),
            completed: 0,
        }
    }

    pub(crate) const fn state(self) -> RepositoryIndexActivityState {
        self.state
    }

    pub(crate) const fn phase(self) -> Option<RepositoryIndexPhase> {
        self.phase
    }

    pub(crate) const fn completed(self) -> u64 {
        self.completed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexActivityState {
    Idle,
    Queued,
    Running,
    Cancelling,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexTraceControlError {
    NoActiveProject,
    UnknownTrace,
    StaleRevision,
    InvalidState,
    Busy,
    CoordinatorStopped,
}

impl fmt::Display for RepositoryIndexTraceControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Fast-Index trace control could not be accepted")
    }
}

impl Error for RepositoryIndexTraceControlError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryIndexDeactivationError {
    NoActiveProject,
    AlreadyPending,
    WatcherShutdown,
    QueueFull,
    CoordinatorStopped,
}

impl fmt::Display for RepositoryIndexDeactivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository indexing could not be deactivated")
    }
}

impl Error for RepositoryIndexDeactivationError {}

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
        ManagedJob, ManagedJobKind, ManagerCommand, ProjectActivation, RepositoryIndexActivity,
        RepositoryIndexActivityState, RepositoryIndexDeactivationError,
        RepositoryIndexRebuildRequestError, RepositoryIndexRebuildState,
        RepositoryIndexTraceControlError, handle_manager_command, set_index_activity_from_job,
        trace_outcome_from_job_status, validate_trace_control,
    };
    use crate::clock::SystemJobClock;
    use crate::index_trace_runtime::IndexTraceRuntime;
    use a3_application::{
        JobScheduler, JobSchedulerConfig, ProjectInspector, RepositoryIndexPhase, ShutdownMode,
    };
    use a3_domain::{IndexTraceState, IndexTraceTrigger, JobId, JobStatus, Progress};
    use a3_repo_index::{
        BuiltinIncrementalIndexCompiler, ParserPoolSize, PollingRepositoryWatcher,
        RepositoryWatcherConfig,
    };
    use a3_workspace::RepositoryInspector;
    use crossbeam_channel::bounded;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn refresh_activity_maps_only_the_fixed_six_phase_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let activity = Mutex::new(RepositoryIndexActivity::idle());

        set_index_activity_from_job(
            &activity,
            JobStatus::Running,
            Some(Progress::determinate(3, 6)?),
        );
        assert_eq!(
            *activity
                .lock()
                .map_err(|_| std::io::Error::other("activity mutex was poisoned"))?,
            RepositoryIndexActivity {
                state: RepositoryIndexActivityState::Running,
                phase: Some(RepositoryIndexPhase::Link),
                completed: 3,
            }
        );

        set_index_activity_from_job(
            &activity,
            JobStatus::Succeeded,
            Some(Progress::determinate(6, 6)?),
        );
        assert_eq!(
            activity
                .lock()
                .map_err(|_| std::io::Error::other("activity mutex was poisoned"))?
                .state(),
            RepositoryIndexActivityState::Succeeded
        );
        Ok(())
    }

    #[test]
    fn rebuild_request_without_an_active_project_is_rejected_before_scheduling()
    -> Result<(), Box<dyn std::error::Error>> {
        let config = JobSchedulerConfig::new(1, 1, 8)?;
        let (scheduler, _events) = JobScheduler::new(config, Arc::new(SystemJobClock::new()))?;
        let submitter = scheduler.submitter()?;
        let (response, receiver) = bounded(1);
        let mut active = None;
        let activity = Mutex::new(RepositoryIndexActivity::idle());
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);
        let trace_runtime = Mutex::new(IndexTraceRuntime::default());

        assert!(!handle_manager_command(
            ManagerCommand::Rebuild(response),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
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
    fn deactivation_without_an_active_project_is_rejected() -> Result<(), Box<dyn std::error::Error>>
    {
        let config = JobSchedulerConfig::new(1, 1, 8)?;
        let (scheduler, _events) = JobScheduler::new(config, Arc::new(SystemJobClock::new()))?;
        let submitter = scheduler.submitter()?;
        let (response, receiver) = bounded(1);
        let mut active = None;
        let activity = Mutex::new(RepositoryIndexActivity::idle());
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);
        let trace_runtime = Mutex::new(IndexTraceRuntime::default());

        assert!(!handle_manager_command(
            ManagerCommand::Deactivate(response),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1))?,
            Err(RepositoryIndexDeactivationError::NoActiveProject)
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
        let activity = Mutex::new(RepositoryIndexActivity::idle());
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);
        let trace_runtime = Mutex::new(IndexTraceRuntime::default());

        assert!(!handle_manager_command(
            ManagerCommand::Activate(Box::new(activation)),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        let (response, receiver) = bounded(1);
        assert!(!handle_manager_command(
            ManagerCommand::Rebuild(response),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?, Ok(()));
        assert!(active.as_ref().is_some_and(|state| state.pending_rebuild));
        assert_eq!(
            *rebuild_state
                .lock()
                .map_err(|_| std::io::Error::other("rebuild state mutex was poisoned"))?,
            RepositoryIndexRebuildState::Queued
        );
        let (response, receiver) = bounded(1);
        assert!(!handle_manager_command(
            ManagerCommand::Deactivate(response),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?, Ok(()));
        assert!(active.as_ref().is_some_and(|state| {
            state.deactivated && state.watcher.is_none() && !state.pending_rebuild
        }));
        let (response, receiver) = bounded(1);
        assert!(!handle_manager_command(
            ManagerCommand::Rebuild(response),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1))?,
            Err(RepositoryIndexRebuildRequestError::NoActiveProject)
        );
        assert!(handle_manager_command(
            ManagerCommand::Shutdown,
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));

        scheduler.shutdown(ShutdownMode::CancelAndWait)?;
        Ok(())
    }

    #[test]
    fn trace_controls_reject_stale_revisions_and_retry_as_a_full_rescan()
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
            project: project.clone(),
            compiler: Box::new(BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(
                1,
            )?)?),
        };
        let mut active = None;
        let activity = Mutex::new(RepositoryIndexActivity::idle());
        let rebuild_state = Mutex::new(RepositoryIndexRebuildState::Idle);
        let trace_runtime = Mutex::new(IndexTraceRuntime::default());
        assert!(!handle_manager_command(
            ManagerCommand::Activate(Box::new(activation)),
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        let job_id = JobId::new(991);
        let queued = trace_runtime
            .lock()
            .map_err(|_| std::io::Error::other("trace runtime mutex was poisoned"))?
            .begin(
                &project,
                job_id,
                IndexTraceTrigger::InitialObservation,
                true,
            )
            .ok_or_else(|| std::io::Error::other("trace projection was not created"))?;
        let trace_id = queued.summary().id();
        trace_runtime
            .lock()
            .map_err(|_| std::io::Error::other("trace runtime mutex was poisoned"))?
            .set_state(IndexTraceState::Running, None);
        let running = trace_runtime
            .lock()
            .map_err(|_| std::io::Error::other("trace runtime mutex was poisoned"))?
            .retained()
            .current
            .ok_or_else(|| std::io::Error::other("running trace was not retained"))?;
        let running_revision = running.summary().revision();
        let state = active
            .as_mut()
            .ok_or_else(|| std::io::Error::other("project was not activated"))?;
        state.pending = None;
        state.active_job = Some(ManagedJob {
            id: job_id,
            kind: ManagedJobKind::Refresh,
            trace_id: Some(trace_id),
        });
        assert!(matches!(
            validate_trace_control(
                active.as_ref(),
                &trace_runtime,
                trace_id,
                running_revision.next()?,
                false,
            ),
            Err(RepositoryIndexTraceControlError::StaleRevision)
        ));

        trace_runtime
            .lock()
            .map_err(|_| std::io::Error::other("trace runtime mutex was poisoned"))?
            .set_state(IndexTraceState::Succeeded, None);
        active
            .as_mut()
            .ok_or_else(|| std::io::Error::other("project was not activated"))?
            .active_job = None;
        let terminal = trace_runtime
            .lock()
            .map_err(|_| std::io::Error::other("trace runtime mutex was poisoned"))?
            .retained()
            .current
            .ok_or_else(|| std::io::Error::other("terminal trace was not retained"))?;
        let (response, receiver) = bounded(1);
        assert!(!handle_manager_command(
            ManagerCommand::RetryTrace {
                trace_id,
                revision: terminal.summary().revision(),
                response,
            },
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?, Ok(()));
        assert!(active.as_ref().is_some_and(|state| {
            state.pending.is_some()
                && state.pending_trigger == Some(IndexTraceTrigger::ManualRetry)
                && !state.pending_rebuild
        }));

        assert!(handle_manager_command(
            ManagerCommand::Shutdown,
            &submitter,
            &mut active,
            &activity,
            &rebuild_state,
            &trace_runtime,
        ));
        scheduler.shutdown(ShutdownMode::CancelAndWait)?;
        Ok(())
    }

    #[test]
    fn panicking_index_task_maps_to_a_safe_worker_failure() -> Result<(), Box<dyn std::error::Error>>
    {
        let config = JobSchedulerConfig::new(1, 1, 2)?;
        let (scheduler, _events) = JobScheduler::new(config, Arc::new(SystemJobClock::new()))?;
        let submitter = scheduler.submitter()?;
        let job_id = JobId::new(992);
        submitter.submit(job_id, super::INDEX_JOB_OWNER, |_context| {
            std::panic::resume_unwind(Box::new("bounded test panic"))
        })?;
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let status = loop {
            let status = submitter
                .snapshot(job_id)
                .ok_or_else(|| std::io::Error::other("panic fixture job was not retained"))?
                .status();
            if status.is_terminal() {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::other("panic fixture did not finish").into());
            }
            std::thread::yield_now();
        };
        assert_eq!(status, JobStatus::Failed);
        assert_eq!(
            trace_outcome_from_job_status(status),
            (
                IndexTraceState::Failed,
                Some(a3_domain::IndexTraceDiagnosticCode::WorkerUnavailable)
            )
        );
        scheduler.shutdown(ShutdownMode::CancelAndWait)?;
        Ok(())
    }
}
