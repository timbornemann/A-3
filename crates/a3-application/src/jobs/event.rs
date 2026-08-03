use super::JobTimestamp;
use a3_domain::{JobId, JobOwner, JobStatus, Progress};
use crossbeam_channel::{Receiver, RecvTimeoutError, TryRecvError};
use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Per-job sequence number that makes event ordering explicit.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct JobEventSequence(u64);

impl JobEventSequence {
    pub(super) const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the primitive sequence value at serialization boundaries.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Typed lifecycle or progress change emitted by the scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobEventKind {
    /// The bounded queue accepted the job.
    Queued,
    /// A worker began executing the task.
    Started,
    /// The task accepted and reported a progress observation.
    Progressed(Progress),
    /// The owner requested cooperative cancellation.
    CancellationRequested,
    /// The task finished successfully.
    Succeeded,
    /// The task finished with a controlled failure or panic boundary.
    Failed,
    /// The task finished after cancellation.
    Cancelled,
}

impl JobEventKind {
    /// Returns the lifecycle status represented by this event, when applicable.
    #[must_use]
    pub const fn status(self) -> Option<JobStatus> {
        match self {
            Self::Queued => Some(JobStatus::Queued),
            Self::Started => Some(JobStatus::Running),
            Self::Progressed(_) => None,
            Self::CancellationRequested => Some(JobStatus::Cancelling),
            Self::Succeeded => Some(JobStatus::Succeeded),
            Self::Failed => Some(JobStatus::Failed),
            Self::Cancelled => Some(JobStatus::Cancelled),
        }
    }
}

/// Ordered event emitted for one scheduler-owned job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobEvent {
    job_id: JobId,
    owner: JobOwner,
    sequence: JobEventSequence,
    occurred_at: JobTimestamp,
    kind: JobEventKind,
}

impl JobEvent {
    pub(super) const fn new(
        job_id: JobId,
        owner: JobOwner,
        sequence: JobEventSequence,
        occurred_at: JobTimestamp,
        kind: JobEventKind,
    ) -> Self {
        Self {
            job_id,
            owner,
            sequence,
            occurred_at,
            kind,
        }
    }

    /// Returns the job that emitted this event.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the lifecycle owner of the job.
    #[must_use]
    pub const fn owner(&self) -> JobOwner {
        self.owner
    }

    /// Returns the per-job event sequence.
    #[must_use]
    pub const fn sequence(&self) -> JobEventSequence {
        self.sequence
    }

    /// Returns the injected monotone timestamp.
    #[must_use]
    pub const fn occurred_at(&self) -> JobTimestamp {
        self.occurred_at
    }

    /// Returns the typed event payload.
    #[must_use]
    pub const fn kind(&self) -> JobEventKind {
        self.kind
    }
}

/// Cloneable consumer for the scheduler's bounded event channel.
#[derive(Clone, Debug)]
pub struct JobEventStream {
    receiver: Receiver<JobEvent>,
}

impl JobEventStream {
    pub(super) const fn new(receiver: Receiver<JobEvent>) -> Self {
        Self { receiver }
    }

    /// Blocks until the next event is available or all publishers close.
    pub fn next(&self) -> Result<JobEvent, JobEventStreamClosed> {
        self.receiver.recv().map_err(|_| JobEventStreamClosed)
    }

    /// Waits for an event, returning `None` when the duration elapses.
    pub fn next_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<JobEvent>, JobEventStreamClosed> {
        match self.receiver.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(JobEventStreamClosed),
        }
    }

    /// Returns the next queued event without blocking.
    pub fn try_next(&self) -> Result<Option<JobEvent>, JobEventStreamClosed> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(JobEventStreamClosed),
        }
    }

    pub(super) fn drain(&self) -> Vec<JobEvent> {
        self.receiver.try_iter().collect()
    }
}

/// All scheduler event publishers have closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobEventStreamClosed;

impl fmt::Display for JobEventStreamClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("job event stream is closed")
    }
}

impl Error for JobEventStreamClosed {}
