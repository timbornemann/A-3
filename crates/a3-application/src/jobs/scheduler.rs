use super::{
    CancellationToken, JobClock, JobEvent, JobEventKind, JobEventSequence, JobEventStream,
};
use a3_domain::{JobId, JobOwner, JobStatus, Progress, ProgressTransitionError};
use crossbeam_channel::{Sender, TrySendError, bounded};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

/// Validated resource limits for one scheduler instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSchedulerConfig {
    worker_count: NonZeroUsize,
    queue_capacity: NonZeroUsize,
    event_capacity: NonZeroUsize,
}

impl JobSchedulerConfig {
    /// Creates scheduler limits, rejecting every unbounded or zero-capacity dimension.
    pub fn new(
        worker_count: usize,
        queue_capacity: usize,
        event_capacity: usize,
    ) -> Result<Self, JobSchedulerConfigError> {
        Ok(Self {
            worker_count: NonZeroUsize::new(worker_count)
                .ok_or(JobSchedulerConfigError::ZeroWorkers)?,
            queue_capacity: NonZeroUsize::new(queue_capacity)
                .ok_or(JobSchedulerConfigError::ZeroQueueCapacity)?,
            event_capacity: NonZeroUsize::new(event_capacity)
                .ok_or(JobSchedulerConfigError::ZeroEventCapacity)?,
        })
    }

    /// Returns the maximum number of concurrently executing tasks.
    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count.get()
    }

    /// Returns the maximum number of queued tasks.
    #[must_use]
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity.get()
    }

    /// Returns the maximum number of buffered events.
    #[must_use]
    pub const fn event_capacity(self) -> usize {
        self.event_capacity.get()
    }
}

/// Invalid scheduler capacity configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobSchedulerConfigError {
    /// At least one owned worker is required.
    ZeroWorkers,
    /// The pending queue must be bounded above zero.
    ZeroQueueCapacity,
    /// The event channel must be bounded above zero.
    ZeroEventCapacity,
}

impl fmt::Display for JobSchedulerConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWorkers => formatter.write_str("job scheduler requires at least one worker"),
            Self::ZeroQueueCapacity => {
                formatter.write_str("job scheduler queue capacity must be non-zero")
            }
            Self::ZeroEventCapacity => {
                formatter.write_str("job scheduler event capacity must be non-zero")
            }
        }
    }
}

impl Error for JobSchedulerConfigError {}

/// Explicit result returned by a scheduler task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCompletion {
    /// The task completed its intended work.
    Succeeded,
    /// The task encountered a controlled failure.
    Failed,
    /// The task observed cancellation and stopped cooperatively.
    Cancelled,
}

/// Owned unit of work accepted by the scheduler.
pub trait JobTask: Send + 'static {
    /// Executes once with a cancellation and progress context.
    fn run(self: Box<Self>, context: JobContext) -> JobCompletion;
}

impl<F> JobTask for F
where
    F: FnOnce(JobContext) -> JobCompletion + Send + 'static,
{
    fn run(self: Box<Self>, context: JobContext) -> JobCompletion {
        self(context)
    }
}

/// Capabilities provided to a running task.
#[derive(Clone)]
pub struct JobContext {
    job_id: JobId,
    cancellation: CancellationToken,
    shared: Arc<SchedulerShared>,
}

impl JobContext {
    /// Returns the executing job identifier.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns a read-only cooperative cancellation token.
    #[must_use]
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Reports validated progress with bounded-channel backpressure.
    pub fn report_progress(&self, progress: Progress) -> Result<(), ProgressReportError> {
        self.shared.report_progress(self.job_id, progress)
    }
}

impl fmt::Debug for JobContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobContext")
            .field("job_id", &self.job_id)
            .field("cancellation", &self.cancellation)
            .finish_non_exhaustive()
    }
}

/// Immutable observation of scheduler-owned job state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSnapshot {
    job_id: JobId,
    owner: JobOwner,
    status: JobStatus,
    progress: Option<Progress>,
    undelivered_event_count: u64,
}

impl JobSnapshot {
    /// Returns the observed job identifier.
    #[must_use]
    pub const fn job_id(self) -> JobId {
        self.job_id
    }

    /// Returns the lifecycle owner.
    #[must_use]
    pub const fn owner(self) -> JobOwner {
        self.owner
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn status(self) -> JobStatus {
        self.status
    }

    /// Returns the latest accepted progress observation.
    #[must_use]
    pub const fn progress(self) -> Option<Progress> {
        self.progress
    }

    /// Returns lifecycle events that could not enter the bounded event channel.
    #[must_use]
    pub const fn undelivered_event_count(self) -> u64 {
        self.undelivered_event_count
    }
}

/// Result of an idempotent cancellation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCancelResult {
    newly_requested: bool,
    event_delivered: bool,
}

impl JobCancelResult {
    /// Returns whether this call changed the cancellation state.
    #[must_use]
    pub const fn newly_requested(self) -> bool {
        self.newly_requested
    }

    /// Returns whether the cancellation event entered the bounded event channel.
    #[must_use]
    pub const fn event_delivered(self) -> bool {
        self.event_delivered
    }
}

/// Policy used when relinquishing scheduler ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownMode {
    /// Stop accepting work and finish every accepted task.
    Drain,
    /// Request cancellation for every active task and wait for all workers.
    CancelAndWait,
}

/// Evidence returned after every owned worker has terminated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    joined_workers: usize,
    jobs: Vec<JobSnapshot>,
    remaining_events: Vec<JobEvent>,
}

impl ShutdownReport {
    /// Returns the number of worker threads joined by shutdown.
    #[must_use]
    pub const fn joined_workers(&self) -> usize {
        self.joined_workers
    }

    /// Returns final job snapshots ordered by identifier.
    #[must_use]
    pub fn jobs(&self) -> &[JobSnapshot] {
        &self.jobs
    }

    /// Returns events that had not already been consumed when shutdown completed.
    #[must_use]
    pub fn remaining_events(&self) -> &[JobEvent] {
        &self.remaining_events
    }
}

/// Bounded owner of worker threads, queued tasks, cancellation, and event delivery.
pub struct JobScheduler {
    runtime: Option<SchedulerRuntime>,
}

impl JobScheduler {
    /// Starts the configured number of owned workers and returns its event stream.
    pub fn new(
        config: JobSchedulerConfig,
        clock: Arc<dyn JobClock>,
    ) -> Result<(Self, JobEventStream), JobSchedulerCreateError> {
        let (event_sender, event_receiver) = bounded(config.event_capacity());
        let event_stream = JobEventStream::new(event_receiver);
        let shared = Arc::new(SchedulerShared {
            config,
            clock,
            event_sender,
            state: Mutex::new(SchedulerState {
                accepting: true,
                queue: VecDeque::with_capacity(config.queue_capacity()),
                jobs: HashMap::new(),
            }),
            work_available: Condvar::new(),
        });
        let mut workers = Vec::with_capacity(config.worker_count());

        for index in 0..config.worker_count() {
            let worker_shared = Arc::clone(&shared);
            match thread::Builder::new()
                .name(format!("a3-job-{index}"))
                .spawn(move || worker_loop(worker_shared))
            {
                Ok(worker) => workers.push(worker),
                Err(source) => {
                    shared.stop_accepting();
                    for worker in workers {
                        let _joined = worker.join();
                    }
                    return Err(JobSchedulerCreateError::WorkerSpawn { source });
                }
            }
        }

        Ok((
            Self {
                runtime: Some(SchedulerRuntime {
                    shared,
                    workers,
                    internal_events: event_stream.clone(),
                }),
            },
            event_stream,
        ))
    }

    /// Accepts a task when both the task queue and first event have capacity.
    pub fn submit<T>(
        &self,
        job_id: JobId,
        owner: JobOwner,
        task: T,
    ) -> Result<(), JobSchedulerSubmitError>
    where
        T: JobTask,
    {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or(JobSchedulerSubmitError::ShuttingDown)?;
        runtime.shared.submit(job_id, owner, Box::new(task))
    }

    /// Requests cooperative cancellation without detaching the owned task.
    pub fn cancel(&self, job_id: JobId) -> Result<JobCancelResult, JobCancellationError> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or(JobCancellationError::ShuttingDown)?;
        runtime.shared.cancel(job_id)
    }

    /// Returns the latest state for an accepted job.
    #[must_use]
    pub fn snapshot(&self, job_id: JobId) -> Option<JobSnapshot> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.shared.snapshot(job_id))
    }

    /// Relinquishes ownership only after every worker has been joined.
    pub fn shutdown(
        mut self,
        mode: ShutdownMode,
    ) -> Result<ShutdownReport, JobSchedulerShutdownError> {
        match self.runtime.take() {
            Some(runtime) => runtime.shutdown(mode),
            None => Ok(ShutdownReport {
                joined_workers: 0,
                jobs: Vec::new(),
                remaining_events: Vec::new(),
            }),
        }
    }
}

impl fmt::Debug for JobScheduler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobScheduler")
            .field("active", &self.runtime.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for JobScheduler {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let _shutdown_result = runtime.shutdown(ShutdownMode::CancelAndWait);
        }
    }
}

struct SchedulerRuntime {
    shared: Arc<SchedulerShared>,
    workers: Vec<JoinHandle<()>>,
    internal_events: JobEventStream,
}

impl SchedulerRuntime {
    fn shutdown(self, mode: ShutdownMode) -> Result<ShutdownReport, JobSchedulerShutdownError> {
        self.shared.stop_accepting();
        if mode == ShutdownMode::CancelAndWait {
            self.shared.cancel_all();
        }

        let mut panicked_workers = 0;
        let joined_workers = self.workers.len();
        for worker in self.workers {
            if worker.join().is_err() {
                panicked_workers += 1;
            }
        }

        let jobs = self.shared.snapshots();
        let remaining_events = self.internal_events.drain();
        if panicked_workers == 0 {
            Ok(ShutdownReport {
                joined_workers,
                jobs,
                remaining_events,
            })
        } else {
            Err(JobSchedulerShutdownError::WorkersPanicked {
                count: panicked_workers,
            })
        }
    }
}

struct SchedulerShared {
    config: JobSchedulerConfig,
    clock: Arc<dyn JobClock>,
    event_sender: Sender<JobEvent>,
    state: Mutex<SchedulerState>,
    work_available: Condvar,
}

impl SchedulerShared {
    fn submit(
        &self,
        job_id: JobId,
        owner: JobOwner,
        task: Box<dyn JobTask>,
    ) -> Result<(), JobSchedulerSubmitError> {
        let mut state = lock_recovering_poison(&self.state);
        if !state.accepting {
            return Err(JobSchedulerSubmitError::ShuttingDown);
        }
        if state.jobs.contains_key(&job_id) {
            return Err(JobSchedulerSubmitError::DuplicateJobId { job_id });
        }
        if state.queue.len() >= self.config.queue_capacity() {
            return Err(JobSchedulerSubmitError::QueueFull);
        }

        let event = JobEvent::new(
            job_id,
            owner,
            JobEventSequence::new(1),
            self.clock.now(),
            JobEventKind::Queued,
        );
        match self.event_sender.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(JobSchedulerSubmitError::EventBackpressure),
            Err(TrySendError::Disconnected(_)) => {
                return Err(JobSchedulerSubmitError::EventStreamClosed);
            }
        }

        let cancellation = CancellationToken::new();
        state.jobs.insert(
            job_id,
            JobRecord {
                owner,
                status: JobStatus::Queued,
                progress: None,
                cancellation: cancellation.clone(),
                next_sequence: 2,
                undelivered_event_count: 0,
                cancellation_event_delivered: None,
            },
        );
        state.queue.push_back(QueuedJob {
            job_id,
            cancellation,
            task,
        });
        self.work_available.notify_one();
        Ok(())
    }

    fn cancel(&self, job_id: JobId) -> Result<JobCancelResult, JobCancellationError> {
        let mut state = lock_recovering_poison(&self.state);
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobCancellationError::UnknownJob { job_id })?;
        if record.status.is_terminal() {
            return Err(JobCancellationError::JobAlreadyFinished { job_id });
        }
        if record.status == JobStatus::Cancelling {
            return Ok(JobCancelResult {
                newly_requested: false,
                event_delivered: record.cancellation_event_delivered == Some(true),
            });
        }

        record.status = JobStatus::Cancelling;
        let newly_requested = record.cancellation.request();
        let event = next_event(
            record,
            job_id,
            self.clock.as_ref(),
            JobEventKind::CancellationRequested,
        );
        let event_delivered = try_deliver_lifecycle_event(&self.event_sender, record, event);
        record.cancellation_event_delivered = Some(event_delivered);
        Ok(JobCancelResult {
            newly_requested,
            event_delivered,
        })
    }

    fn cancel_all(&self) {
        let job_ids: Vec<JobId> = {
            let state = lock_recovering_poison(&self.state);
            state
                .jobs
                .iter()
                .filter_map(|(job_id, record)| (!record.status.is_terminal()).then_some(*job_id))
                .collect()
        };
        for job_id in job_ids {
            let _cancel_result = self.cancel(job_id);
        }
    }

    fn stop_accepting(&self) {
        let mut state = lock_recovering_poison(&self.state);
        state.accepting = false;
        self.work_available.notify_all();
    }

    fn next_job(&self) -> Option<QueuedJob> {
        let mut state = lock_recovering_poison(&self.state);
        loop {
            if let Some(job) = state.queue.pop_front() {
                return Some(job);
            }
            if !state.accepting {
                return None;
            }
            state = match self.work_available.wait(state) {
                Ok(next_state) => next_state,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }

    fn start_job(&self, job_id: JobId) -> bool {
        let mut state = lock_recovering_poison(&self.state);
        let Some(record) = state.jobs.get_mut(&job_id) else {
            return false;
        };
        if record.status == JobStatus::Cancelling {
            record.status = JobStatus::Cancelled;
            let event = next_event(record, job_id, self.clock.as_ref(), JobEventKind::Cancelled);
            let _delivered = try_deliver_lifecycle_event(&self.event_sender, record, event);
            return false;
        }
        if !record.status.allows(JobStatus::Running) {
            return false;
        }

        record.status = JobStatus::Running;
        let event = next_event(record, job_id, self.clock.as_ref(), JobEventKind::Started);
        let _delivered = try_deliver_lifecycle_event(&self.event_sender, record, event);
        true
    }

    fn finish_job(&self, job_id: JobId, completion: JobCompletion, task_panicked: bool) {
        let mut state = lock_recovering_poison(&self.state);
        let Some(record) = state.jobs.get_mut(&job_id) else {
            return;
        };

        if record.cancellation.is_cancelled() || completion == JobCompletion::Cancelled {
            if record.status == JobStatus::Running {
                record.status = JobStatus::Cancelling;
                let requested = next_event(
                    record,
                    job_id,
                    self.clock.as_ref(),
                    JobEventKind::CancellationRequested,
                );
                let _delivered = try_deliver_lifecycle_event(&self.event_sender, record, requested);
            }
            if record.status == JobStatus::Cancelling {
                record.status = JobStatus::Cancelled;
                let event =
                    next_event(record, job_id, self.clock.as_ref(), JobEventKind::Cancelled);
                let _delivered = try_deliver_lifecycle_event(&self.event_sender, record, event);
            }
            return;
        }

        let (status, kind) = if task_panicked || completion == JobCompletion::Failed {
            (JobStatus::Failed, JobEventKind::Failed)
        } else {
            (JobStatus::Succeeded, JobEventKind::Succeeded)
        };
        if record.status.allows(status) {
            record.status = status;
            let event = next_event(record, job_id, self.clock.as_ref(), kind);
            let _delivered = try_deliver_lifecycle_event(&self.event_sender, record, event);
        }
    }

    fn report_progress(
        &self,
        job_id: JobId,
        progress: Progress,
    ) -> Result<(), ProgressReportError> {
        let mut state = lock_recovering_poison(&self.state);
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(ProgressReportError::UnknownJob { job_id })?;
        if record.status != JobStatus::Running {
            return Err(ProgressReportError::NotRunning {
                status: record.status,
            });
        }
        if let Some(previous) = record.progress {
            progress
                .validate_after(previous)
                .map_err(ProgressReportError::InvalidTransition)?;
        }

        let event = JobEvent::new(
            job_id,
            record.owner,
            JobEventSequence::new(record.next_sequence),
            self.clock.now(),
            JobEventKind::Progressed(progress),
        );
        match self.event_sender.try_send(event) {
            Ok(()) => {
                record.next_sequence = record.next_sequence.saturating_add(1);
                record.progress = Some(progress);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(ProgressReportError::EventBackpressure),
            Err(TrySendError::Disconnected(_)) => Err(ProgressReportError::EventStreamClosed),
        }
    }

    fn snapshot(&self, job_id: JobId) -> Option<JobSnapshot> {
        let state = lock_recovering_poison(&self.state);
        state
            .jobs
            .get(&job_id)
            .map(|record| record.snapshot(job_id))
    }

    fn snapshots(&self) -> Vec<JobSnapshot> {
        let state = lock_recovering_poison(&self.state);
        let mut snapshots: Vec<JobSnapshot> = state
            .jobs
            .iter()
            .map(|(job_id, record)| record.snapshot(*job_id))
            .collect();
        snapshots.sort_by_key(|snapshot| snapshot.job_id());
        snapshots
    }
}

struct SchedulerState {
    accepting: bool,
    queue: VecDeque<QueuedJob>,
    jobs: HashMap<JobId, JobRecord>,
}

struct QueuedJob {
    job_id: JobId,
    cancellation: CancellationToken,
    task: Box<dyn JobTask>,
}

struct JobRecord {
    owner: JobOwner,
    status: JobStatus,
    progress: Option<Progress>,
    cancellation: CancellationToken,
    next_sequence: u64,
    undelivered_event_count: u64,
    cancellation_event_delivered: Option<bool>,
}

impl JobRecord {
    const fn snapshot(&self, job_id: JobId) -> JobSnapshot {
        JobSnapshot {
            job_id,
            owner: self.owner,
            status: self.status,
            progress: self.progress,
            undelivered_event_count: self.undelivered_event_count,
        }
    }
}

fn worker_loop(shared: Arc<SchedulerShared>) {
    while let Some(job) = shared.next_job() {
        if !shared.start_job(job.job_id) {
            continue;
        }

        let context = JobContext {
            job_id: job.job_id,
            cancellation: job.cancellation,
            shared: Arc::clone(&shared),
        };
        let execution = catch_unwind(AssertUnwindSafe(|| job.task.run(context)));
        match execution {
            Ok(completion) => shared.finish_job(job.job_id, completion, false),
            Err(_) => shared.finish_job(job.job_id, JobCompletion::Failed, true),
        }
    }
}

fn next_event(
    record: &mut JobRecord,
    job_id: JobId,
    clock: &dyn JobClock,
    kind: JobEventKind,
) -> JobEvent {
    let event = JobEvent::new(
        job_id,
        record.owner,
        JobEventSequence::new(record.next_sequence),
        clock.now(),
        kind,
    );
    record.next_sequence = record.next_sequence.saturating_add(1);
    event
}

fn try_deliver_lifecycle_event(
    sender: &Sender<JobEvent>,
    record: &mut JobRecord,
    event: JobEvent,
) -> bool {
    if sender.try_send(event).is_ok() {
        true
    } else {
        record.undelivered_event_count = record.undelivered_event_count.saturating_add(1);
        false
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Failure while starting owned worker threads.
#[derive(Debug)]
pub enum JobSchedulerCreateError {
    /// The operating system rejected a worker thread.
    WorkerSpawn {
        /// Original standard-library spawn failure.
        source: std::io::Error,
    },
}

impl fmt::Display for JobSchedulerCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerSpawn { source } => {
                write!(formatter, "failed to spawn job worker: {source}")
            }
        }
    }
}

impl Error for JobSchedulerCreateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn { source } => Some(source),
        }
    }
}

/// Failure to accept a scheduler job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobSchedulerSubmitError {
    /// Scheduler shutdown has begun.
    ShuttingDown,
    /// The bounded pending queue has no capacity.
    QueueFull,
    /// Another accepted job already owns this identifier.
    DuplicateJobId {
        /// Rejected duplicate identifier.
        job_id: JobId,
    },
    /// The bounded event channel cannot accept the required queued event.
    EventBackpressure,
    /// No event consumer remains connected.
    EventStreamClosed,
}

impl fmt::Display for JobSchedulerSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShuttingDown => formatter.write_str("job scheduler is shutting down"),
            Self::QueueFull => formatter.write_str("job scheduler queue is full"),
            Self::DuplicateJobId { job_id } => {
                write!(formatter, "job {} is already registered", job_id.value())
            }
            Self::EventBackpressure => formatter.write_str("job event channel is full"),
            Self::EventStreamClosed => formatter.write_str("job event stream is closed"),
        }
    }
}

impl Error for JobSchedulerSubmitError {}

/// Failure to request cancellation for a scheduler job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCancellationError {
    /// Scheduler shutdown has already consumed the runtime.
    ShuttingDown,
    /// The supplied identifier was never accepted.
    UnknownJob {
        /// Unknown identifier.
        job_id: JobId,
    },
    /// The job already reached a terminal state.
    JobAlreadyFinished {
        /// Terminal job identifier.
        job_id: JobId,
    },
}

impl fmt::Display for JobCancellationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShuttingDown => formatter.write_str("job scheduler is shutting down"),
            Self::UnknownJob { job_id } => {
                write!(formatter, "job {} is not registered", job_id.value())
            }
            Self::JobAlreadyFinished { job_id } => {
                write!(formatter, "job {} already finished", job_id.value())
            }
        }
    }
}

impl Error for JobCancellationError {}

/// Failure to accept a task's progress report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressReportError {
    /// The context references an unknown job.
    UnknownJob {
        /// Unknown identifier.
        job_id: JobId,
    },
    /// Progress is only accepted while the task is running.
    NotRunning {
        /// Current non-running state.
        status: JobStatus,
    },
    /// The observation violates monotone progress invariants.
    InvalidTransition(ProgressTransitionError),
    /// The bounded event channel must be drained before retrying.
    EventBackpressure,
    /// No event consumer remains connected.
    EventStreamClosed,
}

impl fmt::Display for ProgressReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownJob { job_id } => {
                write!(formatter, "job {} is not registered", job_id.value())
            }
            Self::NotRunning { status } => write!(formatter, "job is not running: {status:?}"),
            Self::InvalidTransition(error) => write!(formatter, "invalid job progress: {error}"),
            Self::EventBackpressure => formatter.write_str("job event channel is full"),
            Self::EventStreamClosed => formatter.write_str("job event stream is closed"),
        }
    }
}

impl Error for ProgressReportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidTransition(error) => Some(error),
            _ => None,
        }
    }
}

/// Failure observed while joining scheduler-owned workers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobSchedulerShutdownError {
    /// An internal worker panicked outside the task panic boundary.
    WorkersPanicked {
        /// Number of workers that could not be joined normally.
        count: usize,
    },
}

impl fmt::Display for JobSchedulerShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkersPanicked { count } => write!(formatter, "{count} job workers panicked"),
        }
    }
}

impl Error for JobSchedulerShutdownError {}

#[cfg(test)]
mod tests {
    use super::{JobSchedulerConfig, JobSchedulerConfigError};

    #[test]
    fn scheduler_rejects_zero_capacity_dimensions() {
        assert_eq!(
            JobSchedulerConfig::new(0, 1, 1),
            Err(JobSchedulerConfigError::ZeroWorkers)
        );
        assert_eq!(
            JobSchedulerConfig::new(1, 0, 1),
            Err(JobSchedulerConfigError::ZeroQueueCapacity)
        );
        assert_eq!(
            JobSchedulerConfig::new(1, 1, 0),
            Err(JobSchedulerConfigError::ZeroEventCapacity)
        );
    }
}
