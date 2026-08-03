//! Deterministic concurrency, progress, cancellation, and shutdown tests.

use a3_application::{
    JobClock, JobCompletion, JobContext, JobEvent, JobEventKind, JobEventStream, JobScheduler,
    JobSchedulerConfig, JobSchedulerSubmitError, JobTimestamp, ShutdownMode,
};
use a3_domain::{JobId, JobOwner, JobStatus, Progress};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::error::Error;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const CANCELLATION_BUDGET: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
struct FakeClock {
    milliseconds: AtomicU64,
}

impl FakeClock {
    fn advance(&self, milliseconds: u64) {
        self.milliseconds.fetch_add(milliseconds, Ordering::SeqCst);
    }
}

impl JobClock for FakeClock {
    fn now(&self) -> JobTimestamp {
        JobTimestamp::from_millis(self.milliseconds.load(Ordering::SeqCst))
    }
}

#[test]
fn job_reports_ordered_monotone_progress() -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(FakeClock::default());
    let scheduler_clock: Arc<dyn JobClock> = clock.clone();
    let config = JobSchedulerConfig::new(1, 2, 16)?;
    let (scheduler, events) = JobScheduler::new(config, scheduler_clock)?;
    let task_clock = clock.clone();

    scheduler.submit(
        JobId::new(1),
        JobOwner::new(7),
        move |context: JobContext| {
            for completed in [0, 1, 3] {
                task_clock.advance(10);
                let progress = match Progress::determinate(completed, 3) {
                    Ok(progress) => progress,
                    Err(_) => return JobCompletion::Failed,
                };
                if context.report_progress(progress).is_err() {
                    return JobCompletion::Failed;
                }
            }
            JobCompletion::Succeeded
        },
    )?;

    let observed = collect_until_terminal(&events, JobId::new(1), TEST_TIMEOUT)?;
    let kinds: Vec<JobEventKind> = observed.iter().map(JobEvent::kind).collect();
    assert_eq!(
        kinds,
        vec![
            JobEventKind::Queued,
            JobEventKind::Started,
            JobEventKind::Progressed(Progress::determinate(0, 3)?),
            JobEventKind::Progressed(Progress::determinate(1, 3)?),
            JobEventKind::Progressed(Progress::determinate(3, 3)?),
            JobEventKind::Succeeded,
        ]
    );
    assert_eq!(
        observed
            .iter()
            .map(|event| event.sequence().value())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert!(
        observed
            .windows(2)
            .all(|pair| pair[0].occurred_at() <= pair[1].occurred_at())
    );

    let report = scheduler.shutdown(ShutdownMode::Drain)?;
    assert_eq!(report.joined_workers(), 1);
    assert_eq!(report.jobs()[0].status(), JobStatus::Succeeded);
    assert_eq!(
        report.jobs()[0].progress(),
        Some(Progress::determinate(3, 3)?)
    );
    Ok(())
}

#[test]
fn worker_and_queue_limits_are_enforced_under_contention() -> Result<(), Box<dyn Error>> {
    let clock: Arc<dyn JobClock> = Arc::new(FakeClock::default());
    let config = JobSchedulerConfig::new(2, 1, 64)?;
    let (scheduler, events) = JobScheduler::new(config, clock)?;
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let (started_sender, started_receiver) = bounded(3);
    let (release_sender, release_receiver) = bounded(3);

    scheduler.submit(
        JobId::new(10),
        JobOwner::new(1),
        blocking_task(
            Arc::clone(&active),
            Arc::clone(&maximum),
            started_sender.clone(),
            release_receiver.clone(),
        ),
    )?;
    started_receiver.recv_timeout(TEST_TIMEOUT)?;

    scheduler.submit(
        JobId::new(11),
        JobOwner::new(1),
        blocking_task(
            Arc::clone(&active),
            Arc::clone(&maximum),
            started_sender.clone(),
            release_receiver.clone(),
        ),
    )?;
    started_receiver.recv_timeout(TEST_TIMEOUT)?;

    scheduler.submit(
        JobId::new(12),
        JobOwner::new(1),
        blocking_task(
            Arc::clone(&active),
            Arc::clone(&maximum),
            started_sender,
            release_receiver,
        ),
    )?;
    assert_eq!(
        scheduler.submit(JobId::new(13), JobOwner::new(1), |_| {
            JobCompletion::Succeeded
        },),
        Err(JobSchedulerSubmitError::QueueFull)
    );

    for _ in 0..3 {
        release_sender.send(())?;
    }
    collect_terminal_jobs(&events, 3, TEST_TIMEOUT)?;

    assert_eq!(maximum.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    let report = scheduler.shutdown(ShutdownMode::Drain)?;
    assert_eq!(report.joined_workers(), 2);
    assert!(
        report
            .jobs()
            .iter()
            .all(|job| job.status() == JobStatus::Succeeded)
    );
    Ok(())
}

#[test]
fn cancellation_wakes_a_running_job_within_budget() -> Result<(), Box<dyn Error>> {
    let clock: Arc<dyn JobClock> = Arc::new(FakeClock::default());
    let config = JobSchedulerConfig::new(1, 1, 16)?;
    let (scheduler, events) = JobScheduler::new(config, clock)?;
    let (started_sender, started_receiver) = mpsc::sync_channel(0);
    let (stopped_sender, stopped_receiver) = mpsc::sync_channel(0);

    scheduler.submit(
        JobId::new(20),
        JobOwner::new(2),
        move |context: JobContext| {
            if started_sender.send(()).is_err() {
                return JobCompletion::Failed;
            }
            context.cancellation_token().wait_cancelled();
            if stopped_sender.send(()).is_err() {
                return JobCompletion::Failed;
            }
            JobCompletion::Cancelled
        },
    )?;

    started_receiver.recv_timeout(TEST_TIMEOUT)?;
    let cancellation_started = Instant::now();
    let cancellation = scheduler.cancel(JobId::new(20))?;
    assert!(cancellation.newly_requested());
    assert!(cancellation.event_delivered());
    stopped_receiver.recv_timeout(CANCELLATION_BUDGET)?;
    let observed = collect_until_terminal(&events, JobId::new(20), CANCELLATION_BUDGET)?;
    assert!(cancellation_started.elapsed() < CANCELLATION_BUDGET);
    assert!(
        observed
            .iter()
            .any(|event| { event.kind() == JobEventKind::CancellationRequested })
    );
    assert_eq!(
        observed.last().map(JobEvent::kind),
        Some(JobEventKind::Cancelled)
    );

    let report = scheduler.shutdown(ShutdownMode::Drain)?;
    assert_eq!(report.jobs()[0].status(), JobStatus::Cancelled);
    Ok(())
}

#[test]
fn cancel_and_wait_joins_workers_and_skips_queued_tasks() -> Result<(), Box<dyn Error>> {
    let clock: Arc<dyn JobClock> = Arc::new(FakeClock::default());
    let config = JobSchedulerConfig::new(1, 2, 32)?;
    let (scheduler, _events) = JobScheduler::new(config, clock)?;
    let queued_executions = Arc::new(AtomicUsize::new(0));
    let (started_sender, started_receiver) = mpsc::sync_channel(0);

    scheduler.submit(
        JobId::new(30),
        JobOwner::new(3),
        move |context: JobContext| {
            if started_sender.send(()).is_err() {
                return JobCompletion::Failed;
            }
            context.cancellation_token().wait_cancelled();
            JobCompletion::Cancelled
        },
    )?;
    started_receiver.recv_timeout(TEST_TIMEOUT)?;

    let queued_counter = Arc::clone(&queued_executions);
    scheduler.submit(JobId::new(31), JobOwner::new(3), move |_| {
        queued_counter.fetch_add(1, Ordering::SeqCst);
        JobCompletion::Succeeded
    })?;

    let report = scheduler.shutdown(ShutdownMode::CancelAndWait)?;
    assert_eq!(report.joined_workers(), 1);
    assert_eq!(queued_executions.load(Ordering::SeqCst), 0);
    assert_eq!(report.jobs().len(), 2);
    assert!(
        report
            .jobs()
            .iter()
            .all(|job| job.status() == JobStatus::Cancelled)
    );
    Ok(())
}

#[test]
fn lifecycle_event_overflow_is_visible_in_the_final_snapshot() -> Result<(), Box<dyn Error>> {
    let clock: Arc<dyn JobClock> = Arc::new(FakeClock::default());
    let config = JobSchedulerConfig::new(1, 1, 1)?;
    let (scheduler, _events) = JobScheduler::new(config, clock)?;

    scheduler.submit(JobId::new(40), JobOwner::new(4), |context: JobContext| {
        let progress = match Progress::determinate(1, 1) {
            Ok(progress) => progress,
            Err(_) => return JobCompletion::Failed,
        };
        assert!(context.report_progress(progress).is_err());
        JobCompletion::Succeeded
    })?;

    let report = scheduler.shutdown(ShutdownMode::Drain)?;
    let snapshot = report.jobs()[0];
    assert_eq!(snapshot.status(), JobStatus::Succeeded);
    assert_eq!(snapshot.progress(), None);
    assert!(snapshot.undelivered_event_count() >= 2);
    Ok(())
}

fn blocking_task(
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
    started: Sender<()>,
    release: Receiver<()>,
) -> impl FnOnce(JobContext) -> JobCompletion {
    move |_| {
        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
        maximum.fetch_max(current, Ordering::SeqCst);
        if started.send(()).is_err() || release.recv().is_err() {
            active.fetch_sub(1, Ordering::SeqCst);
            return JobCompletion::Failed;
        }
        active.fetch_sub(1, Ordering::SeqCst);
        JobCompletion::Succeeded
    }
}

fn collect_until_terminal(
    events: &JobEventStream,
    job_id: JobId,
    timeout: Duration,
) -> Result<Vec<JobEvent>, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    let mut observed = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = events
            .next_timeout(remaining)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "job event timed out"))?;
        if event.job_id() == job_id {
            let terminal = event.kind().status().is_some_and(JobStatus::is_terminal);
            observed.push(event);
            if terminal {
                return Ok(observed);
            }
        }
    }
}

fn collect_terminal_jobs(
    events: &JobEventStream,
    expected: usize,
    timeout: Duration,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    let mut terminal_count = 0;
    while terminal_count < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = events
            .next_timeout(remaining)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "job event timed out"))?;
        if event.kind().status().is_some_and(JobStatus::is_terminal) {
            terminal_count += 1;
        }
    }
    Ok(())
}
