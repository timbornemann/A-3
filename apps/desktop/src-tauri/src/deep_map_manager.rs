use crate::job_ids::DesktopJobIds;
use a3_application::{
    DeepMapExecutionFailure, DeepMapExecutionOutcome, DeepMapExecutionRequest, DeepMapExecutor,
    DeepMapModelDescriptor, DeepMapResumeState, JobCancellationError, JobCompletion,
    JobEventStream, JobSchedulerSubmitError, JobSubmitter,
};
use a3_domain::{ExploreBudget, JobId, JobOwner, JobStatus, Progress, ProjectIdentity};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use futures::executor::block_on;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const COORDINATOR_TICK: Duration = Duration::from_millis(20);
const DEEP_MAP_JOB_OWNER: JobOwner = JobOwner::new(2);

/// Core-owned product lifecycle layered over terminal scheduler cancellation and R8 checkpoints.
pub(crate) struct DeepMapManager {
    commands: Sender<ManagerCommand>,
    activity: Arc<Mutex<DeepMapActivity>>,
    model: DeepMapModelDescriptor,
    worker: Option<JoinHandle<()>>,
}

impl DeepMapManager {
    pub(crate) fn start(
        submitter: JobSubmitter,
        events: JobEventStream,
        executor: Arc<dyn DeepMapExecutor>,
        job_ids: Arc<DesktopJobIds>,
    ) -> Result<Self, DeepMapManagerStartError> {
        let (commands, receiver) = bounded(8);
        let model = executor.model().clone();
        let activity = Arc::new(Mutex::new(DeepMapActivity::idle()));
        let worker_activity = Arc::clone(&activity);
        let worker = thread::Builder::new()
            .name("a3-deep-map-coordinator".to_owned())
            .spawn(move || {
                coordinator_loop(
                    submitter,
                    events,
                    executor,
                    job_ids,
                    receiver,
                    worker_activity,
                );
            })
            .map_err(DeepMapManagerStartError::WorkerSpawn)?;
        Ok(Self {
            commands,
            activity,
            model,
            worker: Some(worker),
        })
    }

    pub(crate) fn activate_project(
        &self,
        project: ProjectIdentity,
    ) -> Result<(), DeepMapManagerControlError> {
        self.request(|response| ManagerCommand::Activate(Box::new(project), response))
    }

    pub(crate) fn deactivate_project(&self) -> Result<(), DeepMapManagerControlError> {
        self.request(ManagerCommand::Deactivate)
    }

    pub(crate) fn start_mapping(
        &self,
        budget: ExploreBudget,
    ) -> Result<(), DeepMapManagerControlError> {
        self.request(|response| ManagerCommand::Start(budget, response))
    }

    pub(crate) fn pause(&self) -> Result<(), DeepMapManagerControlError> {
        self.request(ManagerCommand::Pause)
    }

    pub(crate) fn resume(&self) -> Result<(), DeepMapManagerControlError> {
        self.request(ManagerCommand::Resume)
    }

    pub(crate) fn cancel(&self) -> Result<(), DeepMapManagerControlError> {
        self.request(ManagerCommand::Cancel)
    }

    pub(crate) fn activity(&self) -> DeepMapActivity {
        *lock_recovering_poison(&self.activity)
    }

    pub(crate) const fn model(&self) -> &DeepMapModelDescriptor {
        &self.model
    }

    fn request(
        &self,
        command: impl FnOnce(Sender<Result<(), DeepMapManagerControlError>>) -> ManagerCommand,
    ) -> Result<(), DeepMapManagerControlError> {
        let (response, receiver) = bounded(1);
        match self.commands.try_send(command(response)) {
            Ok(()) => receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| DeepMapManagerControlError::CoordinatorStopped)?,
            Err(TrySendError::Full(_)) => Err(DeepMapManagerControlError::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(DeepMapManagerControlError::CoordinatorStopped)
            }
        }
    }

    fn stop_and_join(&mut self) -> Result<(), DeepMapManagerShutdownError> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.commands
            .send(ManagerCommand::Shutdown)
            .map_err(|_| DeepMapManagerShutdownError::CoordinatorStopped)?;
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| DeepMapManagerShutdownError::WorkerPanicked),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for DeepMapManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeepMapManager")
            .field("model", &self.model)
            .field("active", &self.worker.is_some())
            .finish()
    }
}

impl Drop for DeepMapManager {
    fn drop(&mut self) {
        let _shutdown = self.stop_and_join();
    }
}

#[derive(Debug)]
enum ManagerCommand {
    Activate(
        Box<ProjectIdentity>,
        Sender<Result<(), DeepMapManagerControlError>>,
    ),
    Deactivate(Sender<Result<(), DeepMapManagerControlError>>),
    Start(
        ExploreBudget,
        Sender<Result<(), DeepMapManagerControlError>>,
    ),
    Pause(Sender<Result<(), DeepMapManagerControlError>>),
    Resume(Sender<Result<(), DeepMapManagerControlError>>),
    Cancel(Sender<Result<(), DeepMapManagerControlError>>),
    Shutdown,
}

struct CoordinatorState {
    project: Option<ProjectIdentity>,
    active: Option<ManagedAttempt>,
    resume: Option<DeepMapResumeState>,
}

struct ManagedAttempt {
    id: JobId,
    intent: TerminationIntent,
    result: SharedAttemptResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminationIntent {
    None,
    Pause,
    Cancel,
    Reset,
}

type SharedAttemptResult =
    Arc<Mutex<Option<Result<DeepMapExecutionOutcome, DeepMapExecutionFailure>>>>;

fn coordinator_loop(
    submitter: JobSubmitter,
    events: JobEventStream,
    executor: Arc<dyn DeepMapExecutor>,
    job_ids: Arc<DesktopJobIds>,
    commands: Receiver<ManagerCommand>,
    activity: Arc<Mutex<DeepMapActivity>>,
) {
    let mut state = CoordinatorState {
        project: None,
        active: None,
        resume: None,
    };

    loop {
        while events.try_next().ok().flatten().is_some() {}
        refresh_attempt(&submitter, &mut state, &activity);

        match commands.recv_timeout(COORDINATOR_TICK) {
            Ok(ManagerCommand::Shutdown) => {
                if let Some(active) = state.active.as_mut() {
                    active.intent = TerminationIntent::Cancel;
                    let _cancel = submitter.cancel(active.id);
                }
                return;
            }
            Ok(command) => handle_command(
                command, &submitter, &executor, &job_ids, &mut state, &activity,
            ),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn handle_command(
    command: ManagerCommand,
    submitter: &JobSubmitter,
    executor: &Arc<dyn DeepMapExecutor>,
    job_ids: &DesktopJobIds,
    state: &mut CoordinatorState,
    activity: &Mutex<DeepMapActivity>,
) {
    match command {
        ManagerCommand::Activate(project, response) => {
            let resetting = cancel_active(submitter, state, TerminationIntent::Reset);
            state.project = Some(*project);
            state.resume = None;
            if resetting {
                set_activity_state(activity, DeepMapActivityState::Cancelling);
            } else {
                set_activity(activity, DeepMapActivity::idle());
            }
            let _sent = response.send(Ok(()));
        }
        ManagerCommand::Deactivate(response) => {
            let resetting = cancel_active(submitter, state, TerminationIntent::Reset);
            state.project = None;
            state.resume = None;
            if resetting {
                set_activity_state(activity, DeepMapActivityState::Cancelling);
            } else {
                set_activity(activity, DeepMapActivity::idle());
            }
            let _sent = response.send(Ok(()));
        }
        ManagerCommand::Start(budget, response) => {
            let result = submit_attempt(
                submitter,
                executor,
                job_ids,
                state,
                DeepMapExecutionRequest::Start { budget },
                activity,
            );
            let _sent = response.send(result);
        }
        ManagerCommand::Resume(response) => {
            let result = match state.resume.take() {
                Some(resume) => {
                    let fallback = resume.clone();
                    let result = submit_attempt(
                        submitter,
                        executor,
                        job_ids,
                        state,
                        DeepMapExecutionRequest::Resume(Box::new(resume)),
                        activity,
                    );
                    if result.is_err() {
                        state.resume = Some(fallback);
                    }
                    result
                }
                None => Err(DeepMapManagerControlError::NotPaused),
            };
            let _sent = response.send(result);
        }
        ManagerCommand::Pause(response) => {
            let result = match state.active.as_ref() {
                Some(active)
                    if active.intent == TerminationIntent::None
                        && submitter
                            .snapshot(active.id)
                            .is_some_and(|snapshot| snapshot.status() == JobStatus::Running) =>
                {
                    let id = active.id;
                    let cancellation = submitter.cancel(id).map_err(map_cancellation_error);
                    if cancellation.is_ok() {
                        if let Some(active) = state.active.as_mut() {
                            active.intent = TerminationIntent::Pause;
                        }
                        set_activity_state(activity, DeepMapActivityState::Pausing);
                    }
                    cancellation.map(|_| ())
                }
                Some(_) => Err(DeepMapManagerControlError::AlreadyPending),
                None => Err(DeepMapManagerControlError::NotRunning),
            };
            let _sent = response.send(result);
        }
        ManagerCommand::Cancel(response) => {
            let result = if let Some(active) = state.active.as_mut() {
                let id = active.id;
                let cancellation = submitter.cancel(id).map_err(map_cancellation_error);
                if cancellation.is_ok() {
                    if let Some(active) = state.active.as_mut() {
                        active.intent = TerminationIntent::Cancel;
                    }
                    set_activity_state(activity, DeepMapActivityState::Cancelling);
                }
                cancellation.map(|_| ())
            } else if state.resume.take().is_some() {
                set_activity_state(activity, DeepMapActivityState::Cancelled);
                Ok(())
            } else {
                Err(DeepMapManagerControlError::NotRunning)
            };
            let _sent = response.send(result);
        }
        ManagerCommand::Shutdown => {}
    }
}

fn submit_attempt(
    submitter: &JobSubmitter,
    executor: &Arc<dyn DeepMapExecutor>,
    job_ids: &DesktopJobIds,
    state: &mut CoordinatorState,
    request: DeepMapExecutionRequest,
    activity: &Mutex<DeepMapActivity>,
) -> Result<(), DeepMapManagerControlError> {
    if state.active.is_some() || state.resume.is_some() {
        return Err(DeepMapManagerControlError::AlreadyPending);
    }
    let project = state
        .project
        .clone()
        .ok_or(DeepMapManagerControlError::NoActiveProject)?;
    let budget = request.budget();
    let id = job_ids
        .allocate()
        .map_err(|_| DeepMapManagerControlError::JobIdsExhausted)?;
    let task_executor = Arc::clone(executor);
    let result: SharedAttemptResult = Arc::new(Mutex::new(None));
    let task_result = Arc::clone(&result);
    submitter
        .submit(id, DEEP_MAP_JOB_OWNER, move |context| {
            let outcome = block_on(task_executor.execute(&project, request, &context));
            let completion = match &outcome {
                Ok(DeepMapExecutionOutcome::Completed(_)) => JobCompletion::Succeeded,
                Ok(DeepMapExecutionOutcome::Cancelled(_)) => JobCompletion::Cancelled,
                Err(_) => JobCompletion::Failed,
            };
            *lock_recovering_poison(&task_result) = Some(outcome);
            completion
        })
        .map_err(map_submit_error)?;
    state.active = Some(ManagedAttempt {
        id,
        intent: TerminationIntent::None,
        result,
    });
    state.resume = None;
    set_activity(activity, DeepMapActivity::queued(budget));
    Ok(())
}

fn refresh_attempt(
    submitter: &JobSubmitter,
    state: &mut CoordinatorState,
    activity: &Mutex<DeepMapActivity>,
) {
    let Some(active) = state.active.as_ref() else {
        return;
    };
    let Some(snapshot) = submitter.snapshot(active.id) else {
        return;
    };
    if !snapshot.status().is_terminal() {
        let next = match (snapshot.status(), active.intent) {
            (_, TerminationIntent::Pause) => DeepMapActivityState::Pausing,
            (_, TerminationIntent::Cancel | TerminationIntent::Reset) => {
                DeepMapActivityState::Cancelling
            }
            (JobStatus::Queued, TerminationIntent::None) => DeepMapActivityState::Queued,
            (JobStatus::Running, TerminationIntent::None) => DeepMapActivityState::Running,
            (JobStatus::Cancelling, TerminationIntent::None) => DeepMapActivityState::Cancelling,
            (
                JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled,
                TerminationIntent::None,
            ) => DeepMapActivityState::Failed,
        };
        update_running_activity(activity, next, snapshot.progress());
        return;
    }

    let Some(active) = state.active.take() else {
        return;
    };
    let result = lock_recovering_poison(&active.result).take();
    let budget = lock_recovering_poison(activity).budget;
    if active.intent == TerminationIntent::Reset {
        state.resume = None;
        set_activity(activity, DeepMapActivity::idle());
        return;
    }
    match (snapshot.status(), active.intent, result) {
        (
            JobStatus::Succeeded,
            TerminationIntent::None,
            Some(Ok(DeepMapExecutionOutcome::Completed(completed))),
        ) if budget == Some(completed.budget()) => {
            let counts = step_counts(&completed);
            state.resume = None;
            set_activity(
                activity,
                DeepMapActivity::terminal(DeepMapActivityState::Succeeded, budget, counts),
            );
        }
        (
            JobStatus::Cancelled,
            TerminationIntent::Pause,
            Some(Ok(DeepMapExecutionOutcome::Cancelled(resume))),
        ) if budget == Some(resume.budget()) => {
            let counts = step_counts(&resume);
            state.resume = Some(resume);
            set_activity(
                activity,
                DeepMapActivity::terminal(DeepMapActivityState::Paused, budget, counts),
            );
        }
        (JobStatus::Cancelled, TerminationIntent::Cancel, _) => {
            state.resume = None;
            set_activity(
                activity,
                DeepMapActivity::terminal(
                    DeepMapActivityState::Cancelled,
                    budget,
                    StepCounts::default(),
                ),
            );
        }
        _ => {
            state.resume = None;
            set_activity(
                activity,
                DeepMapActivity::terminal(
                    DeepMapActivityState::Failed,
                    budget,
                    StepCounts::default(),
                ),
            );
        }
    }
}

fn cancel_active(
    submitter: &JobSubmitter,
    state: &mut CoordinatorState,
    intent: TerminationIntent,
) -> bool {
    if let Some(active) = state.active.as_mut() {
        active.intent = intent;
        let _cancel = submitter.cancel(active.id);
        true
    } else {
        false
    }
}

fn map_cancellation_error(error: JobCancellationError) -> DeepMapManagerControlError {
    match error {
        JobCancellationError::JobAlreadyFinished { .. } => DeepMapManagerControlError::NotRunning,
        JobCancellationError::ShuttingDown | JobCancellationError::UnknownJob { .. } => {
            DeepMapManagerControlError::CoordinatorStopped
        }
    }
}

fn map_submit_error(error: JobSchedulerSubmitError) -> DeepMapManagerControlError {
    match error {
        JobSchedulerSubmitError::QueueFull | JobSchedulerSubmitError::EventBackpressure => {
            DeepMapManagerControlError::QueueFull
        }
        JobSchedulerSubmitError::ShuttingDown
        | JobSchedulerSubmitError::DuplicateJobId { .. }
        | JobSchedulerSubmitError::EventStreamClosed => {
            DeepMapManagerControlError::CoordinatorStopped
        }
    }
}

fn update_running_activity(
    activity: &Mutex<DeepMapActivity>,
    state: DeepMapActivityState,
    progress: Option<Progress>,
) {
    let mut current = lock_recovering_poison(activity);
    current.state = state;
    current.progress = progress;
}

fn set_activity_state(activity: &Mutex<DeepMapActivity>, state: DeepMapActivityState) {
    lock_recovering_poison(activity).state = state;
}

fn set_activity(activity: &Mutex<DeepMapActivity>, value: DeepMapActivity) {
    *lock_recovering_poison(activity) = value;
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeepMapActivityState {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeepMapActivity {
    state: DeepMapActivityState,
    budget: Option<ExploreBudget>,
    progress: Option<Progress>,
    completed_steps: u64,
    total_steps: u64,
}

impl DeepMapActivity {
    const fn idle() -> Self {
        Self {
            state: DeepMapActivityState::Idle,
            budget: None,
            progress: None,
            completed_steps: 0,
            total_steps: 0,
        }
    }

    const fn queued(budget: ExploreBudget) -> Self {
        Self {
            state: DeepMapActivityState::Queued,
            budget: Some(budget),
            progress: None,
            completed_steps: 0,
            total_steps: 0,
        }
    }

    const fn terminal(
        state: DeepMapActivityState,
        budget: Option<ExploreBudget>,
        counts: StepCounts,
    ) -> Self {
        Self {
            state,
            budget,
            progress: None,
            completed_steps: counts.completed,
            total_steps: counts.total,
        }
    }

    pub(crate) const fn state(self) -> DeepMapActivityState {
        self.state
    }

    pub(crate) const fn budget(self) -> Option<ExploreBudget> {
        self.budget
    }

    pub(crate) const fn progress(self) -> Option<Progress> {
        self.progress
    }

    pub(crate) const fn completed_steps(self) -> u64 {
        self.completed_steps
    }

    pub(crate) const fn total_steps(self) -> u64 {
        self.total_steps
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct StepCounts {
    completed: u64,
    total: u64,
}

fn step_counts(state: &DeepMapResumeState) -> StepCounts {
    StepCounts {
        completed: u64::try_from(state.completed_steps()).map_or(u64::MAX, |value| value),
        total: u64::try_from(state.total_steps()).map_or(u64::MAX, |value| value),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeepMapManagerControlError {
    NoActiveProject,
    NotRunning,
    NotPaused,
    AlreadyPending,
    QueueFull,
    JobIdsExhausted,
    CoordinatorStopped,
}

impl fmt::Display for DeepMapManagerControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoActiveProject => "Deep Map requires an active project",
            Self::NotRunning => "Deep Map is not running",
            Self::NotPaused => "Deep Map has no paused checkpoint",
            Self::AlreadyPending => "Deep Map already has a pending action",
            Self::QueueFull => "Deep Map queue is full",
            Self::JobIdsExhausted => "Deep Map job identifiers are exhausted",
            Self::CoordinatorStopped => "Deep Map coordinator is unavailable",
        })
    }
}

impl Error for DeepMapManagerControlError {}

#[derive(Debug)]
pub(crate) enum DeepMapManagerStartError {
    WorkerSpawn(std::io::Error),
}

impl fmt::Display for DeepMapManagerStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Deep Map coordinator could not start")
    }
}

impl Error for DeepMapManagerStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn(source) => Some(source),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeepMapManagerShutdownError {
    CoordinatorStopped,
    WorkerPanicked,
}

impl fmt::Display for DeepMapManagerShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Deep Map coordinator did not shut down cleanly")
    }
}

impl Error for DeepMapManagerShutdownError {}

#[cfg(test)]
mod tests {
    use super::{DeepMapActivityState, DeepMapManager, DeepMapManagerControlError};
    use crate::job_ids::DesktopJobIds;
    use a3_application::{
        DeepMapExecutionFailure, DeepMapExecutionFuture, DeepMapExecutionOutcome,
        DeepMapExecutionRequest, DeepMapExecutor, DeepMapModelDescriptor, DeepMapResumeState,
        JobClock, JobCompletion, JobScheduler, JobSchedulerConfig, JobTimestamp,
    };
    use a3_domain::{
        CanonicalDirectory, Centrality, ContentHash, ExploreBudget, FileRevision, GitHead,
        GitReferenceName, GraphSymbol, IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord,
        IndexRunSequence, IndexRunStatus, JobId, JobOwner, LinkedGraph, LocalSymbolId,
        ModelCapabilities, ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit,
        ModelProfile, ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId,
        ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
        ModuleCoverageSnapshot, ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
        ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, ParsedSymbol,
        ProjectIdentity, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
        RepositoryId, RepositoryIdentity, RepositoryModule, RepositoryPath, SnapshotId,
        SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank,
        SymbolRankSignals, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
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
        model: DeepMapModelDescriptor,
        initial: DeepMapResumeState,
        attempts: AtomicUsize,
    }

    impl DeepMapExecutor for PausingExecutor {
        fn model(&self) -> &DeepMapModelDescriptor {
            &self.model
        }

        fn execute<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            request: DeepMapExecutionRequest,
            control: &'a a3_application::JobContext,
        ) -> DeepMapExecutionFuture<'a> {
            Box::pin(async move {
                self.attempts.fetch_add(1, Ordering::AcqRel);
                let state = match request {
                    DeepMapExecutionRequest::Start { .. } => self.initial.clone(),
                    DeepMapExecutionRequest::Resume(state) => *state,
                };
                while !control.cancellation_token().is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(DeepMapExecutionOutcome::cancelled(state))
            })
        }
    }

    #[test]
    fn explicit_start_pause_resume_and_cancel_never_run_before_start() -> Result<(), Box<dyn Error>>
    {
        let config = JobSchedulerConfig::new(1, 4, 64)?;
        let (scheduler, events) =
            JobScheduler::new(config, Arc::new(TestClock(AtomicU64::new(1))))?;
        let executor = Arc::new(PausingExecutor {
            model: DeepMapModelDescriptor::from_verified_profile(&model_profile()?)?,
            initial: resume_fixture()?,
            attempts: AtomicUsize::new(0),
        });
        let manager = DeepMapManager::start(
            scheduler.submitter()?,
            events,
            executor.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 0);
        assert_eq!(manager.activity().state(), DeepMapActivityState::Idle);

        manager.start_mapping(ExploreBudget::DEFAULT)?;
        wait_for_state(&manager, DeepMapActivityState::Running)?;
        wait_for_attempts(&executor, 1)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 1);

        manager.pause()?;
        wait_for_state(&manager, DeepMapActivityState::Paused)?;
        assert!(manager.activity().total_steps() > 0);
        assert_eq!(
            manager.start_mapping(ExploreBudget::DEFAULT),
            Err(DeepMapManagerControlError::AlreadyPending)
        );

        manager.resume()?;
        wait_for_state(&manager, DeepMapActivityState::Running)?;
        wait_for_attempts(&executor, 2)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 2);

        manager.cancel()?;
        wait_for_state(&manager, DeepMapActivityState::Cancelled)?;
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn queued_attempt_cannot_claim_a_checkpoint_safe_pause() -> Result<(), Box<dyn Error>> {
        let config = JobSchedulerConfig::new(1, 4, 64)?;
        let (scheduler, events) =
            JobScheduler::new(config, Arc::new(TestClock(AtomicU64::new(1))))?;
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
            model: DeepMapModelDescriptor::from_verified_profile(&model_profile()?)?,
            initial: resume_fixture()?,
            attempts: AtomicUsize::new(0),
        });
        let manager = DeepMapManager::start(
            submitter,
            events,
            executor.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        manager.start_mapping(ExploreBudget::DEFAULT)?;
        wait_for_state(&manager, DeepMapActivityState::Queued)?;
        assert_eq!(
            manager.pause(),
            Err(DeepMapManagerControlError::AlreadyPending)
        );
        manager.cancel()?;
        release_sender.send(())?;
        wait_for_state(&manager, DeepMapActivityState::Cancelled)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 0);
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn project_activation_cancels_old_work_before_returning_to_idle() -> Result<(), Box<dyn Error>>
    {
        let config = JobSchedulerConfig::new(1, 4, 64)?;
        let (scheduler, events) =
            JobScheduler::new(config, Arc::new(TestClock(AtomicU64::new(1))))?;
        let executor = Arc::new(PausingExecutor {
            model: DeepMapModelDescriptor::from_verified_profile(&model_profile()?)?,
            initial: resume_fixture()?,
            attempts: AtomicUsize::new(0),
        });
        let manager = DeepMapManager::start(
            scheduler.submitter()?,
            events,
            executor.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        manager.start_mapping(ExploreBudget::DEFAULT)?;
        wait_for_state(&manager, DeepMapActivityState::Running)?;
        wait_for_attempts(&executor, 1)?;

        manager.activate_project(project_fixture()?)?;
        wait_for_state(&manager, DeepMapActivityState::Idle)?;
        assert_eq!(executor.attempts.load(Ordering::Acquire), 1);
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    #[test]
    fn pause_rejects_an_executor_checkpoint_with_a_changed_budget() -> Result<(), Box<dyn Error>> {
        let config = JobSchedulerConfig::new(1, 4, 64)?;
        let (scheduler, events) =
            JobScheduler::new(config, Arc::new(TestClock(AtomicU64::new(1))))?;
        let executor = Arc::new(PausingExecutor {
            model: DeepMapModelDescriptor::from_verified_profile(&model_profile()?)?,
            initial: resume_fixture()?,
            attempts: AtomicUsize::new(0),
        });
        let manager = DeepMapManager::start(
            scheduler.submitter()?,
            events,
            executor.clone(),
            Arc::new(DesktopJobIds::new()),
        )?;
        manager.activate_project(project_fixture()?)?;
        manager.start_mapping(ExploreBudget::MINIMUM)?;
        wait_for_state(&manager, DeepMapActivityState::Running)?;
        wait_for_attempts(&executor, 1)?;
        manager.pause()?;
        wait_for_state(&manager, DeepMapActivityState::Failed)?;
        drop(manager);
        drop(scheduler);
        Ok(())
    }

    fn wait_for_state(
        manager: &DeepMapManager,
        expected: DeepMapActivityState,
    ) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if manager.activity().state() == expected {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(std::io::Error::other(format!(
            "Deep Map did not reach {expected:?}; current state is {:?}",
            manager.activity().state()
        ))
        .into())
    }

    fn wait_for_attempts(
        executor: &PausingExecutor,
        expected: usize,
    ) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if executor.attempts.load(Ordering::Acquire) == expected {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(std::io::Error::other(format!("Deep Map did not start {expected} attempt(s)")).into())
    }

    fn model_profile() -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            ModelProviderId::try_from_string("local".to_owned())?,
            ModelId::try_from_string("mapper".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(2_048)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::RepeatSchemaInPrompt,
            )?,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
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

    fn resume_fixture() -> Result<DeepMapResumeState, Box<dyn Error>> {
        let published = published_fixture()?;
        let coverage = ModuleCoverageSnapshot::empty(
            published.run().snapshot_id(),
            a3_domain::ModuleCardSchemaVersion::V1,
        );
        let plan =
            a3_domain::DeepMapPlanner::v1().plan(&published, &coverage, ExploreBudget::DEFAULT)?;
        if plan.steps().is_empty() {
            return Err(std::io::Error::other("fixture did not produce a Deep-Map step").into());
        }
        let checkpoint = a3_domain::ExplorerCheckpoint::new(&plan);
        Ok(DeepMapResumeState::new(
            plan,
            checkpoint,
            ExploreBudget::DEFAULT,
        )?)
    }

    fn published_fixture() -> Result<a3_domain::PublishedIndex, Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let manifest = revision("Cargo.toml", 2)?;
        let source = revision("src/lib.rs", 3)?;
        let symbol_id = SymbolId::from_bytes([4; 32]);
        let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
        let symbol = GraphSymbol::new(
            symbol_id,
            source.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string("main".to_owned())?,
                range,
                range,
            )?,
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![manifest.clone(), source.clone()],
            vec![symbol],
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = RankProjection::new(
            snapshot_id,
            RankingPolicyVersion::v1(),
            vec![SymbolRank::new(
                symbol_id,
                RankScore::try_from_sum(1_000)?,
                SymbolRankSignals {
                    in_degree: 0,
                    out_degree: 0,
                    centrality: Centrality::from_basis_points(1_000)?,
                    degree_contribution: 0,
                    centrality_contribution: 1_000,
                    entrypoint_contribution: 0,
                    public_export_contribution: 0,
                    manifest_contribution: 0,
                    test_contribution: 0,
                },
            )],
        )?;
        let module_id = ModuleId::from_bytes([5; 32]);
        let featured = ModuleSymbolSet::new(vec![symbol_id], false)?;
        let module = RepositoryModule::new(
            module_id,
            ModuleKind::ManifestBoundary,
            Some(ModuleRoot::Repository),
            vec![manifest.clone()],
            featured.clone(),
            featured.clone(),
            ModuleSymbolSet::empty(),
        )?;
        let membership = ModuleMembership::new(
            module_id,
            symbol_id,
            ModuleMembershipEvidence::manifest(source, manifest.clone()),
        );
        let card = RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module_id],
            vec![IndexLanguage::Rust],
            featured,
            2,
            1,
        )?;
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module],
            vec![membership],
            card,
        )?;
        let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([6; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(a3_domain::PublishedIndex::new(run, publication)?)
    }

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    #[test]
    fn executor_failure_type_remains_content_free() {
        assert_eq!(
            DeepMapExecutionFailure::Publication.to_string(),
            "Deep Map publication failed"
        );
    }
}
