use crate::job_ids::DesktopJobIds;
use a3_application::{
    AgentRunExecutionFailure, AgentRunExecutionOutcome, AgentRunExecutionRequest, AgentRunExecutor,
    JobCancellationError, JobCompletion, JobEventStream, JobSchedulerSubmitError, JobSubmitter,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, JobId, JobOwner, JobStatus, ProjectIdentity, TaskId, TaskLedgerRevision,
};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::async_runtime::block_on;

const COORDINATOR_TICK: Duration = Duration::from_millis(20);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(6);
const PROJECT_QUIESCE_TIMEOUT: Duration = Duration::from_secs(5);
const AGENT_JOB_OWNER: JobOwner = JobOwner::new(3);

/// Future returned by the Core recovery seam after scheduler-owned work has stopped.
pub(crate) type AgentRuntimeRecoveryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AgentRuntimeRecoveryFailure>> + Send + 'a>>;

/// Narrow post-worker recovery seam used to validate Pause and persist explicit Cancel.
pub(crate) trait AgentRuntimeRecovery: fmt::Debug + Send + Sync {
    /// Inspects the stopped nonterminal task and returns its current durable checkpoint anchors.
    fn validate_pause<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: AgentRunExecutionRequest,
    ) -> AgentRuntimeRecoveryFuture<'a, AgentPauseCheckpoint>;

    /// Persists the explicit user Cancel after no worker can still mutate the selected task.
    fn cancel<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: AgentRunExecutionRequest,
    ) -> AgentRuntimeRecoveryFuture<'a, ()>;
}

/// Durable, content-free checkpoint facts retained only after a safe cooperative pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentPauseCheckpoint {
    task_id: TaskId,
    ledger_revision: TaskLedgerRevision,
    ledger_store_version: TaskLedgerStoreVersion,
}

impl AgentPauseCheckpoint {
    pub(crate) fn new(
        task_id: TaskId,
        ledger_revision: TaskLedgerRevision,
        ledger_store_version: TaskLedgerStoreVersion,
        controller_state: AgentControllerState,
    ) -> Result<Self, AgentRuntimeRecoveryFailure> {
        if controller_state.is_terminal() {
            return Err(AgentRuntimeRecoveryFailure::InvalidCheckpoint);
        }
        Ok(Self {
            task_id,
            ledger_revision,
            ledger_store_version,
        })
    }

    pub(crate) const fn task_id(self) -> TaskId {
        self.task_id
    }

    fn permits_resume(self, request: AgentRunExecutionRequest) -> bool {
        request.task_id() == self.task_id
            && request.ledger_revision().get() >= self.ledger_revision.get()
            && request.ledger_store_version().get() > self.ledger_store_version.get()
    }

    const fn as_request(self) -> AgentRunExecutionRequest {
        AgentRunExecutionRequest::new(
            self.task_id,
            self.ledger_revision,
            self.ledger_store_version,
        )
    }
}

/// Core-owned Agent product lifecycle layered over terminal scheduler cancellation.
pub(crate) struct AgentRunManager {
    commands: Sender<ManagerCommand>,
    activity: Arc<Mutex<AgentRunActivity>>,
    worker: Option<JoinHandle<()>>,
}

impl AgentRunManager {
    pub(crate) fn start(
        submitter: JobSubmitter,
        events: JobEventStream,
        executor: Arc<dyn AgentRunExecutor>,
        recovery: Arc<dyn AgentRuntimeRecovery>,
        job_ids: Arc<DesktopJobIds>,
    ) -> Result<Self, AgentRunManagerStartError> {
        let (commands, receiver) = bounded(8);
        let activity = Arc::new(Mutex::new(AgentRunActivity::idle()));
        let worker_activity = Arc::clone(&activity);
        let worker = thread::Builder::new()
            .name("a3-agent-run-coordinator".to_owned())
            .spawn(move || {
                coordinator_loop(
                    submitter,
                    events,
                    executor,
                    recovery,
                    job_ids,
                    receiver,
                    worker_activity,
                );
            })
            .map_err(AgentRunManagerStartError::WorkerSpawn)?;
        Ok(Self {
            commands,
            activity,
            worker: Some(worker),
        })
    }

    pub(crate) fn activate_project(
        &self,
        project: ProjectIdentity,
    ) -> Result<(), AgentRunManagerControlError> {
        self.request(|response| ManagerCommand::Activate(Box::new(project), response))
    }

    pub(crate) fn deactivate_project(&self) -> Result<(), AgentRunManagerControlError> {
        self.request(ManagerCommand::Deactivate)
    }

    pub(crate) fn start_attempt(
        &self,
        request: AgentRunExecutionRequest,
    ) -> Result<(), AgentRunManagerControlError> {
        self.request(|response| ManagerCommand::Start(request, response))
    }

    pub(crate) fn pause(&self, task_id: TaskId) -> Result<(), AgentRunManagerControlError> {
        self.request(|response| ManagerCommand::Pause(task_id, response))
    }

    pub(crate) fn cancel_owned_worker(
        &self,
        request: AgentRunExecutionRequest,
    ) -> Result<(), AgentRunManagerControlError> {
        self.request(|response| ManagerCommand::CancelOwnedWorker(request, response))
    }

    pub(crate) fn complete_external_cancel(
        &self,
        task_id: TaskId,
    ) -> Result<(), AgentRunManagerControlError> {
        self.request(|response| ManagerCommand::CompleteExternalCancel(task_id, response))
    }

    pub(crate) fn activity(&self) -> AgentRunActivity {
        lock_recovering_poison(&self.activity).clone()
    }

    fn request(
        &self,
        command: impl FnOnce(Sender<Result<(), AgentRunManagerControlError>>) -> ManagerCommand,
    ) -> Result<(), AgentRunManagerControlError> {
        let (response, receiver) = bounded(1);
        match self.commands.try_send(command(response)) {
            Ok(()) => receiver
                .recv_timeout(CONTROL_TIMEOUT)
                .map_err(|_| AgentRunManagerControlError::CoordinatorStopped)?,
            Err(TrySendError::Full(_)) => Err(AgentRunManagerControlError::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(AgentRunManagerControlError::CoordinatorStopped)
            }
        }
    }

    fn stop_and_join(&mut self) -> Result<(), AgentRunManagerShutdownError> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.commands
            .send(ManagerCommand::Shutdown)
            .map_err(|_| AgentRunManagerShutdownError::CoordinatorStopped)?;
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| AgentRunManagerShutdownError::WorkerPanicked),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for AgentRunManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentRunManager")
            .field("active", &self.worker.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for AgentRunManager {
    fn drop(&mut self) {
        let _shutdown = self.stop_and_join();
    }
}

#[derive(Debug)]
enum ManagerCommand {
    Activate(
        Box<ProjectIdentity>,
        Sender<Result<(), AgentRunManagerControlError>>,
    ),
    Deactivate(Sender<Result<(), AgentRunManagerControlError>>),
    Start(
        AgentRunExecutionRequest,
        Sender<Result<(), AgentRunManagerControlError>>,
    ),
    Pause(TaskId, Sender<Result<(), AgentRunManagerControlError>>),
    CancelOwnedWorker(
        AgentRunExecutionRequest,
        Sender<Result<(), AgentRunManagerControlError>>,
    ),
    CompleteExternalCancel(TaskId, Sender<Result<(), AgentRunManagerControlError>>),
    Shutdown,
}

struct CoordinatorState {
    project: Option<ProjectIdentity>,
    active: Option<ManagedAttempt>,
    paused: Option<AgentPauseCheckpoint>,
}

struct ManagedAttempt {
    id: JobId,
    request: AgentRunExecutionRequest,
    intent: TerminationIntent,
    result: SharedAttemptResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminationIntent {
    None,
    Pause,
    Cancel(AgentRunExecutionRequest),
    Reset,
}

type SharedAttemptResult =
    Arc<Mutex<Option<Result<AgentRunExecutionOutcome, AgentRunExecutionFailure>>>>;

#[allow(clippy::too_many_arguments)]
fn coordinator_loop(
    submitter: JobSubmitter,
    events: JobEventStream,
    executor: Arc<dyn AgentRunExecutor>,
    recovery: Arc<dyn AgentRuntimeRecovery>,
    job_ids: Arc<DesktopJobIds>,
    commands: Receiver<ManagerCommand>,
    activity: Arc<Mutex<AgentRunActivity>>,
) {
    let mut state = CoordinatorState {
        project: None,
        active: None,
        paused: None,
    };

    loop {
        while events.try_next().ok().flatten().is_some() {}
        refresh_attempt(&submitter, recovery.as_ref(), &mut state, &activity);

        match commands.recv_timeout(COORDINATOR_TICK) {
            Ok(ManagerCommand::Shutdown) => {
                if let Some(active) = state.active.as_mut() {
                    active.intent = TerminationIntent::Reset;
                    let _cancel = submitter.cancel(active.id);
                }
                return;
            }
            Ok(command) => handle_command(
                command,
                &submitter,
                &executor,
                recovery.as_ref(),
                job_ids.as_ref(),
                &mut state,
                &activity,
            ),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_command(
    command: ManagerCommand,
    submitter: &JobSubmitter,
    executor: &Arc<dyn AgentRunExecutor>,
    recovery: &dyn AgentRuntimeRecovery,
    job_ids: &DesktopJobIds,
    state: &mut CoordinatorState,
    activity: &Mutex<AgentRunActivity>,
) {
    match command {
        ManagerCommand::Activate(project, response) => {
            let result = quiesce_for_project_change(submitter, recovery, state, activity);
            if result.is_ok() {
                state.project = Some(*project);
                state.paused = None;
                set_activity(activity, AgentRunActivity::idle());
            }
            let _sent = response.send(result);
        }
        ManagerCommand::Deactivate(response) => {
            let result = quiesce_for_project_change(submitter, recovery, state, activity);
            if result.is_ok() {
                state.project = None;
                state.paused = None;
                set_activity(activity, AgentRunActivity::idle());
            }
            let _sent = response.send(result);
        }
        ManagerCommand::Start(request, response) => {
            let result =
                submit_recovered_attempt(submitter, executor, job_ids, state, request, activity);
            let _sent = response.send(result);
        }
        ManagerCommand::Pause(task_id, response) => {
            let result = pause_attempt(submitter, state, task_id, activity);
            let _sent = response.send(result);
        }
        ManagerCommand::CancelOwnedWorker(request, response) => {
            let result = cancel_owned_worker(submitter, state, request, activity);
            let _sent = response.send(result);
        }
        ManagerCommand::CompleteExternalCancel(task_id, response) => {
            let result = complete_external_cancel(state, task_id, activity);
            let _sent = response.send(result);
        }
        ManagerCommand::Shutdown => {}
    }
}

fn submit_attempt(
    submitter: &JobSubmitter,
    executor: &Arc<dyn AgentRunExecutor>,
    job_ids: &DesktopJobIds,
    state: &mut CoordinatorState,
    request: AgentRunExecutionRequest,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    if state.active.is_some() || state.paused.is_some() {
        return Err(AgentRunManagerControlError::AlreadyPending);
    }
    let project = state
        .project
        .clone()
        .ok_or(AgentRunManagerControlError::NoActiveProject)?;
    let id = job_ids
        .allocate()
        .map_err(|_| AgentRunManagerControlError::JobIdsExhausted)?;
    let result: SharedAttemptResult = Arc::new(Mutex::new(None));
    let task_result = Arc::clone(&result);
    let executor = Arc::clone(executor);
    submitter
        .submit(id, AGENT_JOB_OWNER, move |context| {
            let outcome = block_on(executor.execute(&project, request, &context));
            let completion = match &outcome {
                Ok(AgentRunExecutionOutcome::Completed) => JobCompletion::Succeeded,
                Ok(AgentRunExecutionOutcome::Cancelled) => JobCompletion::Cancelled,
                Err(_) => JobCompletion::Failed,
            };
            *lock_recovering_poison(&task_result) = Some(outcome);
            completion
        })
        .map_err(map_submit_error)?;
    state.active = Some(ManagedAttempt {
        id,
        request,
        intent: TerminationIntent::None,
        result,
    });
    set_activity(activity, AgentRunActivity::queued(request));
    Ok(())
}

fn submit_recovered_attempt(
    submitter: &JobSubmitter,
    executor: &Arc<dyn AgentRunExecutor>,
    job_ids: &DesktopJobIds,
    state: &mut CoordinatorState,
    request: AgentRunExecutionRequest,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    if state.active.is_some() {
        return Err(AgentRunManagerControlError::AlreadyPending);
    }
    if let Some(checkpoint) = state.paused {
        if checkpoint.task_id() != request.task_id() {
            return Err(AgentRunManagerControlError::TaskMismatch);
        }
        if !checkpoint.permits_resume(request) {
            return Err(AgentRunManagerControlError::AnchorsChanged);
        }
    }
    state.paused = None;
    let result = submit_attempt(submitter, executor, job_ids, state, request, activity);
    if result.is_err() {
        set_activity(
            activity,
            AgentRunActivity::terminal(AgentRunActivityState::Failed, request),
        );
    }
    result
}

fn pause_attempt(
    submitter: &JobSubmitter,
    state: &mut CoordinatorState,
    task_id: TaskId,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    let Some(active) = state.active.as_ref() else {
        return Err(AgentRunManagerControlError::NotRunning);
    };
    if active.request.task_id() != task_id {
        return Err(AgentRunManagerControlError::TaskMismatch);
    }
    if active.intent != TerminationIntent::None
        || !submitter
            .snapshot(active.id)
            .is_some_and(|snapshot| snapshot.status() == JobStatus::Running)
    {
        return Err(AgentRunManagerControlError::AlreadyPending);
    }
    submitter
        .cancel(active.id)
        .map_err(map_cancellation_error)?;
    if let Some(active) = state.active.as_mut() {
        active.intent = TerminationIntent::Pause;
    }
    set_activity_state(activity, AgentRunActivityState::Pausing);
    Ok(())
}

fn cancel_owned_worker(
    submitter: &JobSubmitter,
    state: &mut CoordinatorState,
    request: AgentRunExecutionRequest,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    let Some(active) = state.active.as_ref() else {
        return Err(AgentRunManagerControlError::NotRunning);
    };
    if active.request.task_id() != request.task_id() {
        return Err(AgentRunManagerControlError::TaskMismatch);
    }
    submitter
        .cancel(active.id)
        .map_err(map_cancellation_error)?;
    if let Some(active) = state.active.as_mut() {
        active.intent = TerminationIntent::Cancel(request);
    }
    set_activity_state(activity, AgentRunActivityState::Cancelling);
    Ok(())
}

fn complete_external_cancel(
    state: &mut CoordinatorState,
    task_id: TaskId,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    if state.active.is_some() {
        return Err(AgentRunManagerControlError::AlreadyPending);
    }
    let checkpoint = state
        .paused
        .ok_or(AgentRunManagerControlError::NotRunning)?;
    if checkpoint.task_id() != task_id {
        return Err(AgentRunManagerControlError::TaskMismatch);
    }
    state.paused = None;
    set_activity(
        activity,
        AgentRunActivity::terminal(AgentRunActivityState::Cancelled, checkpoint.as_request()),
    );
    Ok(())
}

fn refresh_attempt(
    submitter: &JobSubmitter,
    recovery: &dyn AgentRuntimeRecovery,
    state: &mut CoordinatorState,
    activity: &Mutex<AgentRunActivity>,
) {
    let Some(active) = state.active.as_ref() else {
        return;
    };
    let Some(snapshot) = submitter.snapshot(active.id) else {
        return;
    };
    if !snapshot.status().is_terminal() {
        let next = match (snapshot.status(), active.intent) {
            (_, TerminationIntent::Pause) => AgentRunActivityState::Pausing,
            (_, TerminationIntent::Cancel(_) | TerminationIntent::Reset) => {
                AgentRunActivityState::Cancelling
            }
            (JobStatus::Queued, TerminationIntent::None) => AgentRunActivityState::Queued,
            (JobStatus::Running, TerminationIntent::None) => AgentRunActivityState::Running,
            (JobStatus::Cancelling, TerminationIntent::None) => AgentRunActivityState::Cancelling,
            (
                JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled,
                TerminationIntent::None,
            ) => AgentRunActivityState::Failed,
        };
        set_activity_state(activity, next);
        return;
    }

    let Some(active) = state.active.take() else {
        return;
    };
    let result = lock_recovering_poison(&active.result).take();
    if active.intent == TerminationIntent::Reset {
        state.paused = None;
        set_activity(activity, AgentRunActivity::idle());
        return;
    }
    let Some(project) = state.project.as_ref() else {
        state.paused = None;
        set_activity(
            activity,
            AgentRunActivity::terminal(AgentRunActivityState::Failed, active.request),
        );
        return;
    };
    match (snapshot.status(), active.intent, result) {
        (
            JobStatus::Succeeded,
            TerminationIntent::None,
            Some(Ok(AgentRunExecutionOutcome::Completed)),
        ) => {
            state.paused = None;
            set_activity(
                activity,
                AgentRunActivity::terminal(AgentRunActivityState::Succeeded, active.request),
            );
        }
        (
            JobStatus::Cancelled,
            TerminationIntent::Pause,
            Some(Ok(AgentRunExecutionOutcome::Cancelled)),
        ) => match block_on(recovery.validate_pause(project, active.request)) {
            Ok(checkpoint) if checkpoint.task_id() == active.request.task_id() => {
                state.paused = Some(checkpoint);
                set_activity(
                    activity,
                    AgentRunActivity::terminal(
                        AgentRunActivityState::Paused,
                        checkpoint.as_request(),
                    ),
                );
            }
            Ok(_) | Err(_) => {
                state.paused = None;
                set_activity(
                    activity,
                    AgentRunActivity::terminal(AgentRunActivityState::Failed, active.request),
                );
            }
        },
        (JobStatus::Cancelled, TerminationIntent::Cancel(request), _) => {
            let cancelled = block_on(recovery.cancel(project, request)).is_ok();
            state.paused = None;
            set_activity(
                activity,
                AgentRunActivity::terminal(
                    if cancelled {
                        AgentRunActivityState::Cancelled
                    } else {
                        AgentRunActivityState::Failed
                    },
                    active.request,
                ),
            );
        }
        _ => {
            state.paused = None;
            set_activity(
                activity,
                AgentRunActivity::terminal(AgentRunActivityState::Failed, active.request),
            );
        }
    }
}

fn quiesce_for_project_change(
    submitter: &JobSubmitter,
    recovery: &dyn AgentRuntimeRecovery,
    state: &mut CoordinatorState,
    activity: &Mutex<AgentRunActivity>,
) -> Result<(), AgentRunManagerControlError> {
    state.paused = None;
    refresh_attempt(submitter, recovery, state, activity);
    if let Some(active) = state.active.as_mut() {
        active.intent = TerminationIntent::Reset;
        let cancellation = submitter.cancel(active.id);
        if matches!(
            cancellation,
            Err(JobCancellationError::JobAlreadyFinished { .. })
        ) {
            refresh_attempt(submitter, recovery, state, activity);
        } else {
            cancellation.map_err(map_cancellation_error)?;
        }
        if state.active.is_some() {
            set_activity_state(activity, AgentRunActivityState::Cancelling);
        }
    }
    let deadline = Instant::now() + PROJECT_QUIESCE_TIMEOUT;
    while state.active.is_some() {
        refresh_attempt(submitter, recovery, state, activity);
        if state.active.is_none() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(AgentRunManagerControlError::StopTimedOut);
        }
        thread::sleep(COORDINATOR_TICK);
    }
    Ok(())
}

fn map_cancellation_error(error: JobCancellationError) -> AgentRunManagerControlError {
    match error {
        JobCancellationError::JobAlreadyFinished { .. } => AgentRunManagerControlError::NotRunning,
        JobCancellationError::ShuttingDown | JobCancellationError::UnknownJob { .. } => {
            AgentRunManagerControlError::CoordinatorStopped
        }
    }
}

fn map_submit_error(error: JobSchedulerSubmitError) -> AgentRunManagerControlError {
    match error {
        JobSchedulerSubmitError::QueueFull | JobSchedulerSubmitError::EventBackpressure => {
            AgentRunManagerControlError::QueueFull
        }
        JobSchedulerSubmitError::ShuttingDown
        | JobSchedulerSubmitError::DuplicateJobId { .. }
        | JobSchedulerSubmitError::EventStreamClosed => {
            AgentRunManagerControlError::CoordinatorStopped
        }
    }
}

fn set_activity_state(activity: &Mutex<AgentRunActivity>, state: AgentRunActivityState) {
    lock_recovering_poison(activity).state = state;
}

fn set_activity(activity: &Mutex<AgentRunActivity>, value: AgentRunActivity) {
    *lock_recovering_poison(activity) = value;
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRunActivityState {
    Idle,
    Queued,
    Running,
    Pausing,
    Paused,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl AgentRunActivityState {
    pub(crate) const fn owns_live_worker(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Running | Self::Pausing | Self::Cancelling
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRunActivity {
    state: AgentRunActivityState,
    task_id: Option<TaskId>,
}

impl AgentRunActivity {
    const fn idle() -> Self {
        Self {
            state: AgentRunActivityState::Idle,
            task_id: None,
        }
    }

    const fn queued(request: AgentRunExecutionRequest) -> Self {
        Self {
            state: AgentRunActivityState::Queued,
            task_id: Some(request.task_id()),
        }
    }

    const fn terminal(state: AgentRunActivityState, request: AgentRunExecutionRequest) -> Self {
        Self {
            state,
            task_id: Some(request.task_id()),
        }
    }

    pub(crate) const fn state(&self) -> AgentRunActivityState {
        self.state
    }

    pub(crate) const fn task_id(&self) -> Option<TaskId> {
        self.task_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRunManagerControlError {
    NoActiveProject,
    NotRunning,
    AlreadyPending,
    AnchorsChanged,
    TaskMismatch,
    QueueFull,
    JobIdsExhausted,
    StopTimedOut,
    CoordinatorStopped,
}

impl fmt::Display for AgentRunManagerControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoActiveProject => "Agent execution requires an active project",
            Self::NotRunning => "Agent execution is not running",
            Self::AlreadyPending => "Agent execution already has a pending action",
            Self::AnchorsChanged => "Agent execution anchors changed",
            Self::TaskMismatch => "Agent execution belongs to another task",
            Self::QueueFull => "Agent execution queue is full",
            Self::JobIdsExhausted => "Agent execution job identifiers are exhausted",
            Self::StopTimedOut => "Agent execution did not stop within the fixed deadline",
            Self::CoordinatorStopped => "Agent execution coordinator is unavailable",
        })
    }
}

impl Error for AgentRunManagerControlError {}

#[derive(Debug)]
pub(crate) enum AgentRunManagerStartError {
    WorkerSpawn(std::io::Error),
}

impl fmt::Display for AgentRunManagerStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent execution coordinator could not start")
    }
}

impl Error for AgentRunManagerStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn(source) => Some(source),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRunManagerShutdownError {
    CoordinatorStopped,
    WorkerPanicked,
}

impl fmt::Display for AgentRunManagerShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent execution coordinator did not shut down cleanly")
    }
}

impl Error for AgentRunManagerShutdownError {}

/// Recovery validation failed without exposing persistence or repository details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRuntimeRecoveryFailure {
    InvalidCheckpoint,
    Unavailable,
}

impl fmt::Display for AgentRuntimeRecoveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Agent runtime recovery failed")
    }
}

impl Error for AgentRuntimeRecoveryFailure {}

#[cfg(test)]
mod tests {
    use super::{
        AgentPauseCheckpoint, AgentRunActivityState, AgentRunManager, AgentRunManagerControlError,
        AgentRuntimeRecovery, AgentRuntimeRecoveryFailure, AgentRuntimeRecoveryFuture,
    };
    use crate::job_ids::DesktopJobIds;
    use a3_application::{
        AgentRunExecutionFuture, AgentRunExecutionOutcome, AgentRunExecutionRequest,
        AgentRunExecutor, JobClock, JobCompletion, JobScheduler, JobSchedulerConfig, JobTimestamp,
        TaskLedgerStoreVersion,
    };
    use a3_domain::{
        AgentControllerState, CanonicalDirectory, GitHead, GitReferenceName, JobId, JobOwner,
        ProjectIdentity, RepositoryId, RepositoryIdentity, TaskId, TaskLedgerRevision,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use std::error::Error;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    #[derive(Debug)]
    struct TestClock(AtomicU64);

    impl JobClock for TestClock {
        fn now(&self) -> JobTimestamp {
            JobTimestamp::from_millis(self.0.fetch_add(1, Ordering::AcqRel))
        }
    }

    #[derive(Debug)]
    struct PausingExecutor {
        attempts: AtomicUsize,
        started: Option<std::sync::mpsc::SyncSender<usize>>,
    }

    impl AgentRunExecutor for PausingExecutor {
        fn execute<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _request: AgentRunExecutionRequest,
            control: &'a a3_application::JobContext,
        ) -> AgentRunExecutionFuture<'a> {
            let attempt = self.attempts.fetch_add(1, Ordering::AcqRel) + 1;
            if let Some(started) = &self.started {
                assert!(started.try_send(attempt).is_ok());
            }
            Box::pin(async move {
                control.cancellation_token().cancelled().await;
                Ok(AgentRunExecutionOutcome::Cancelled)
            })
        }
    }

    #[derive(Debug)]
    struct Recovery {
        pauses: AtomicUsize,
        cancels: AtomicUsize,
        cancel_store_version: AtomicU64,
        invalid_pause: bool,
    }

    impl AgentRuntimeRecovery for Recovery {
        fn validate_pause<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            request: AgentRunExecutionRequest,
        ) -> AgentRuntimeRecoveryFuture<'a, AgentPauseCheckpoint> {
            self.pauses.fetch_add(1, Ordering::AcqRel);
            Box::pin(async move {
                let state = if self.invalid_pause {
                    AgentControllerState::Cancelled
                } else {
                    AgentControllerState::Execute
                };
                AgentPauseCheckpoint::new(
                    request.task_id(),
                    request.ledger_revision(),
                    request.ledger_store_version(),
                    state,
                )
            })
        }

        fn cancel<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            request: AgentRunExecutionRequest,
        ) -> AgentRuntimeRecoveryFuture<'a, ()> {
            self.cancels.fetch_add(1, Ordering::AcqRel);
            self.cancel_store_version
                .store(request.ledger_store_version().get(), Ordering::Release);
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn explicit_start_pause_and_cancel_require_owned_terminal_work() -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(2);
        let executor = Arc::new(PausingExecutor {
            attempts: AtomicUsize::new(0),
            started: Some(started_sender),
        });
        let recovery = Arc::new(Recovery {
            pauses: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            cancel_store_version: AtomicU64::new(0),
            invalid_pause: false,
        });
        let manager = AgentRunManager::start(
            scheduler.submitter()?,
            events,
            executor.clone(),
            recovery.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;

        assert_eq!(executor.attempts.load(Ordering::Acquire), 0);
        manager.activate_project(project_fixture()?)?;
        assert_eq!(manager.activity().state(), AgentRunActivityState::Idle);
        assert_eq!(executor.attempts.load(Ordering::Acquire), 0);

        let request = request()?;
        manager.start_attempt(request)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;
        // Scheduler Running precedes the executor callback. Observe its explicit start,
        // not an incidental scheduling delay, before checking callback side effects.
        assert_eq!(started_receiver.recv_timeout(Duration::from_secs(1))?, 1);
        assert_eq!(executor.attempts.load(Ordering::Acquire), 1);

        manager.pause(request.task_id())?;
        wait_for_state(&manager, AgentRunActivityState::Paused)?;
        assert_eq!(recovery.pauses.load(Ordering::Acquire), 1);
        assert_eq!(manager.activity().task_id(), Some(request.task_id()));
        assert_eq!(
            manager.start_attempt(request),
            Err(AgentRunManagerControlError::AnchorsChanged)
        );

        let resumed_request = AgentRunExecutionRequest::new(
            request.task_id(),
            request.ledger_revision(),
            TaskLedgerStoreVersion::new(8)?,
        );
        manager.start_attempt(resumed_request)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;
        assert_eq!(started_receiver.recv_timeout(Duration::from_secs(1))?, 2);
        assert_eq!(executor.attempts.load(Ordering::Acquire), 2);

        manager.cancel_owned_worker(resumed_request)?;
        wait_for_state(&manager, AgentRunActivityState::Cancelled)?;
        assert_eq!(recovery.cancels.load(Ordering::Acquire), 1);
        assert_eq!(recovery.cancel_store_version.load(Ordering::Acquire), 8);
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn queued_attempt_cannot_claim_a_pause_checkpoint() -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let submitter = scheduler.submitter()?;
        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(1);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        submitter.submit(JobId::new(100), JobOwner::new(100), move |_| {
            if started_sender.send(()).is_err() || release_receiver.recv().is_err() {
                return JobCompletion::Failed;
            }
            JobCompletion::Succeeded
        })?;
        started_receiver.recv_timeout(Duration::from_secs(1))?;

        let executor = Arc::new(PausingExecutor {
            attempts: AtomicUsize::new(0),
            started: None,
        });
        let recovery = Arc::new(Recovery {
            pauses: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            cancel_store_version: AtomicU64::new(0),
            invalid_pause: false,
        });
        let manager = AgentRunManager::start(
            submitter,
            events,
            executor.clone(),
            recovery.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        let request = request()?;
        manager.start_attempt(request)?;
        wait_for_state(&manager, AgentRunActivityState::Queued)?;
        assert_eq!(
            manager.pause(request.task_id()),
            Err(AgentRunManagerControlError::AlreadyPending)
        );
        manager.cancel_owned_worker(request)?;
        release_sender.send(())?;
        wait_for_state(&manager, AgentRunActivityState::Cancelled)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 0);
        assert_eq!(recovery.pauses.load(Ordering::Acquire), 0);
        assert_eq!(recovery.cancels.load(Ordering::Acquire), 1);
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn externally_committed_paused_cancel_clears_without_a_second_recovery_commit()
    -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let recovery = Arc::new(Recovery {
            pauses: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            cancel_store_version: AtomicU64::new(0),
            invalid_pause: false,
        });
        let manager = AgentRunManager::start(
            scheduler.submitter()?,
            events,
            Arc::new(PausingExecutor {
                attempts: AtomicUsize::new(0),
                started: None,
            }),
            recovery.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        let request = request()?;
        manager.start_attempt(request)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;
        manager.pause(request.task_id())?;
        wait_for_state(&manager, AgentRunActivityState::Paused)?;

        manager.complete_external_cancel(request.task_id())?;

        assert_eq!(manager.activity().state(), AgentRunActivityState::Cancelled);
        assert_eq!(recovery.cancels.load(Ordering::Acquire), 0);
        assert_eq!(
            manager.complete_external_cancel(request.task_id()),
            Err(AgentRunManagerControlError::NotRunning)
        );
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn shutdown_keeps_the_executor_owned_until_scheduler_join() -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let manager = AgentRunManager::start(
            scheduler.submitter()?,
            events,
            Arc::new(PausingExecutor {
                attempts: AtomicUsize::new(0),
                started: None,
            }),
            Arc::new(Recovery {
                pauses: AtomicUsize::new(0),
                cancels: AtomicUsize::new(0),
                cancel_store_version: AtomicU64::new(0),
                invalid_pause: false,
            }),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        manager.start_attempt(request()?)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;

        drop(manager);
        let report = scheduler.shutdown(a3_application::ShutdownMode::CancelAndWait)?;

        assert_eq!(report.joined_workers(), 1);
        assert!(
            report
                .jobs()
                .iter()
                .any(|job| job.status() == a3_domain::JobStatus::Cancelled)
        );
        Ok(())
    }

    #[test]
    fn project_activation_waits_until_old_owned_work_stops() -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let executor = Arc::new(PausingExecutor {
            attempts: AtomicUsize::new(0),
            started: None,
        });
        let recovery = Arc::new(Recovery {
            pauses: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            cancel_store_version: AtomicU64::new(0),
            invalid_pause: false,
        });
        let manager = AgentRunManager::start(
            scheduler.submitter()?,
            events,
            executor.clone(),
            recovery.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        manager.start_attempt(request()?)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;

        manager.activate_project(project_fixture()?)?;
        assert_eq!(manager.activity().state(), AgentRunActivityState::Idle);
        assert_eq!(executor.attempts.load(Ordering::Acquire), 1);
        assert_eq!(recovery.pauses.load(Ordering::Acquire), 0);
        assert_eq!(recovery.cancels.load(Ordering::Acquire), 0);
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn terminal_controller_state_cannot_become_paused() -> Result<(), Box<dyn Error>> {
        let (scheduler, events) = JobScheduler::new(
            JobSchedulerConfig::new(1, 4, 64)?,
            Arc::new(TestClock(AtomicU64::new(1))),
        )?;
        let manager = AgentRunManager::start(
            scheduler.submitter()?,
            events,
            Arc::new(PausingExecutor {
                attempts: AtomicUsize::new(0),
                started: None,
            }),
            Arc::new(Recovery {
                pauses: AtomicUsize::new(0),
                cancels: AtomicUsize::new(0),
                cancel_store_version: AtomicU64::new(0),
                invalid_pause: true,
            }),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        let request = request()?;
        manager.start_attempt(request)?;
        wait_for_state(&manager, AgentRunActivityState::Running)?;
        manager.pause(request.task_id())?;
        wait_for_state(&manager, AgentRunActivityState::Failed)?;
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    fn wait_for_state(
        manager: &AgentRunManager,
        expected: AgentRunActivityState,
    ) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if manager.activity().state() == expected {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(std::io::Error::other(format!(
            "Agent Run did not reach {expected:?}; current state is {:?}",
            manager.activity().state()
        ))
        .into())
    }

    fn request() -> Result<AgentRunExecutionRequest, Box<dyn Error>> {
        Ok(AgentRunExecutionRequest::new(
            TaskId::from_bytes([31; 32]),
            TaskLedgerRevision::new(3)?,
            TaskLedgerStoreVersion::new(7)?,
        ))
    }

    fn project_fixture() -> Result<ProjectIdentity, Box<dyn Error>> {
        let repository_id = RepositoryId::from_bytes([21; 32]);
        let path = std::env::current_dir()?;
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(path.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([22; 32]),
            WorktreeAnchorId::from_bytes([23; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(path)?,
        );
        let head = GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        };
        Ok(ProjectIdentity::new(repository, worktree, head)?)
    }

    #[test]
    fn recovery_failure_type_remains_content_free() {
        assert_eq!(
            AgentRuntimeRecoveryFailure::Unavailable.to_string(),
            "Agent runtime recovery failed"
        );
    }
}
