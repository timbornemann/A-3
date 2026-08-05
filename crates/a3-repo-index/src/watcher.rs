use crate::GitRepositoryDiscoverer;
use crate::path::{RepositoryPathObservation, observe_repository_path};
use crate::repository::{inspect_head, inspect_index_checksum, open_validated};
use a3_application::{
    RepositoryChangeBatch, RepositoryDiscoverer, RepositoryDiscoveryControl,
    RepositoryDiscoveryControlError, RepositoryRescanReason,
};
use a3_domain::{DiscoveryPolicy, GitHead, Progress, ProjectIdentity, RepositoryPath};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::Metadata;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

const MIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MAX_DEBOUNCE: Duration = Duration::from_secs(5);
const MAX_QUEUE_CAPACITY: usize = 64;

/// Fixed polling, debounce, latency, and backpressure limits for one owned watcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryWatcherConfig {
    poll_interval: Duration,
    debounce: Duration,
    max_batch_latency: Duration,
    queue_capacity: NonZeroUsize,
}

impl RepositoryWatcherConfig {
    /// Validates positive bounded timing and queue dimensions.
    pub fn new(
        poll_interval: Duration,
        debounce: Duration,
        max_batch_latency: Duration,
        queue_capacity: usize,
    ) -> Result<Self, RepositoryWatcherConfigError> {
        if !(MIN_POLL_INTERVAL..=MAX_POLL_INTERVAL).contains(&poll_interval) {
            return Err(RepositoryWatcherConfigError::PollInterval);
        }
        if debounce < poll_interval || debounce > MAX_DEBOUNCE {
            return Err(RepositoryWatcherConfigError::Debounce);
        }
        if max_batch_latency < debounce || max_batch_latency > MAX_DEBOUNCE {
            return Err(RepositoryWatcherConfigError::BatchLatency);
        }
        let queue_capacity = NonZeroUsize::new(queue_capacity)
            .filter(|capacity| capacity.get() <= MAX_QUEUE_CAPACITY)
            .ok_or(RepositoryWatcherConfigError::QueueCapacity)?;
        Ok(Self {
            poll_interval,
            debounce,
            max_batch_latency,
            queue_capacity,
        })
    }

    /// Returns the production V1 timing and capacity policy.
    #[must_use]
    pub fn v1() -> Self {
        Self {
            poll_interval: Duration::from_millis(100),
            debounce: Duration::from_millis(200),
            max_batch_latency: Duration::from_millis(750),
            queue_capacity: NonZeroUsize::MIN,
        }
    }

    /// Returns the interval between Git-backed filesystem observations.
    #[must_use]
    pub const fn poll_interval(self) -> Duration {
        self.poll_interval
    }

    /// Returns required quiet time before a burst is emitted.
    #[must_use]
    pub const fn debounce(self) -> Duration {
        self.debounce
    }

    /// Returns the maximum time a continuously changing burst may remain pending.
    #[must_use]
    pub const fn max_batch_latency(self) -> Duration {
        self.max_batch_latency
    }

    /// Returns the bounded number of batches waiting for indexing.
    #[must_use]
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity.get()
    }
}

impl Default for RepositoryWatcherConfig {
    fn default() -> Self {
        Self::v1()
    }
}

/// Invalid watcher resource policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryWatcherConfigError {
    /// Polling would be too frequent, too slow, or disabled.
    PollInterval,
    /// Debounce is shorter than one poll or exceeds the fixed limit.
    Debounce,
    /// Maximum batch latency is shorter than debounce or exceeds the fixed limit.
    BatchLatency,
    /// The output queue is empty or exceeds the fixed capacity.
    QueueCapacity,
}

impl fmt::Display for RepositoryWatcherConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::PollInterval => "repository watcher poll interval is invalid",
            Self::Debounce => "repository watcher debounce is invalid",
            Self::BatchLatency => "repository watcher batch latency is invalid",
            Self::QueueCapacity => "repository watcher queue capacity is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for RepositoryWatcherConfigError {}

/// Owned platform-neutral watcher using bounded Git discovery and metadata polling.
pub struct PollingRepositoryWatcher {
    batches: Receiver<RepositoryChangeBatch>,
    shutdown: Sender<()>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PollingRepositoryWatcher {
    /// Starts one named owned worker; the initial successful observation requests a full scan.
    pub fn start(
        project: ProjectIdentity,
        config: RepositoryWatcherConfig,
    ) -> Result<Self, RepositoryWatcherStartError> {
        let (batch_sender, batches) = bounded(config.queue_capacity());
        let (shutdown, shutdown_receiver) = bounded(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("a3-repository-watcher".to_owned())
            .spawn(move || {
                watcher_loop(
                    project,
                    config,
                    batch_sender,
                    shutdown_receiver,
                    worker_cancelled,
                );
            })
            .map_err(RepositoryWatcherStartError::WorkerSpawn)?;
        Ok(Self {
            batches,
            shutdown,
            cancelled,
            worker: Some(worker),
        })
    }

    /// Waits at most the caller-provided bound for one coalesced refresh request.
    pub fn next_batch(
        &self,
        timeout: Duration,
    ) -> Result<Option<RepositoryChangeBatch>, RepositoryWatcherReceiveError> {
        match self.batches.recv_timeout(timeout) {
            Ok(batch) => Ok(Some(batch)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                Err(RepositoryWatcherReceiveError::WorkerStopped)
            }
        }
    }

    /// Requests shutdown and relinquishes ownership only after the worker joins.
    pub fn shutdown(mut self) -> Result<(), RepositoryWatcherShutdownError> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<(), RepositoryWatcherShutdownError> {
        self.cancelled.store(true, Ordering::Release);
        let _signal = self.shutdown.try_send(());
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| RepositoryWatcherShutdownError::WorkerPanicked),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for PollingRepositoryWatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PollingRepositoryWatcher")
            .field("queued_batches", &self.batches.len())
            .field("active", &self.worker.is_some())
            .finish()
    }
}

impl Drop for PollingRepositoryWatcher {
    fn drop(&mut self) {
        let _shutdown = self.stop_and_join();
    }
}

/// The operating system could not start the owned watcher worker.
#[derive(Debug)]
pub enum RepositoryWatcherStartError {
    /// Thread creation failed.
    WorkerSpawn(std::io::Error),
}

impl fmt::Display for RepositoryWatcherStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository watcher worker could not be started")
    }
}

impl Error for RepositoryWatcherStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn(source) => Some(source),
        }
    }
}

/// The watcher worker stopped before delivering another observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryWatcherReceiveError {
    /// The owned worker terminated.
    WorkerStopped,
}

impl fmt::Display for RepositoryWatcherReceiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository watcher worker stopped")
    }
}

impl Error for RepositoryWatcherReceiveError {}

/// Watcher shutdown observed an internal worker panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryWatcherShutdownError {
    /// The owned worker panicked before it could be joined normally.
    WorkerPanicked,
}

impl fmt::Display for RepositoryWatcherShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository watcher worker panicked")
    }
}

impl Error for RepositoryWatcherShutdownError {}

fn watcher_loop(
    project: ProjectIdentity,
    config: RepositoryWatcherConfig,
    batches: Sender<RepositoryChangeBatch>,
    shutdown: Receiver<()>,
    cancelled: Arc<AtomicBool>,
) {
    let discoverer = GitRepositoryDiscoverer::new();
    let control = WatchDiscoveryControl { cancelled };
    let mut previous: Option<WatchObservation> = None;
    let mut pending = PendingChanges::default();
    let mut source_unavailable = false;

    loop {
        if control.is_cancelled() {
            return;
        }
        let observed_at = Instant::now();
        match WatchObservation::capture(&project, &discoverer, &control) {
            Ok(current) => {
                source_unavailable = false;
                match previous.as_ref() {
                    None => pending
                        .require_rescan(RepositoryRescanReason::InitialObservation, observed_at),
                    Some(previous) => {
                        let comparison = previous.compare(&current);
                        if !comparison.paths.is_empty() {
                            pending.record(comparison.paths, observed_at);
                        }
                        if comparison.metadata_changed {
                            pending.require_rescan(
                                RepositoryRescanReason::RepositoryMetadataChanged,
                                observed_at,
                            );
                        }
                    }
                }
                previous = Some(current);
            }
            Err(()) if !source_unavailable => {
                source_unavailable = true;
                pending.require_rescan(RepositoryRescanReason::SourceUnavailable, observed_at)
            }
            Err(()) => {}
        }
        pending.try_deliver(&batches, config, Instant::now());

        match shutdown.recv_timeout(config.poll_interval()) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[derive(Debug)]
struct WatchDiscoveryControl {
    cancelled: Arc<AtomicBool>,
}

impl RepositoryDiscoveryControl for WatchDiscoveryControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryDiscoveryControlError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchObservation {
    files: BTreeMap<RepositoryPath, WatchStamp>,
    head: GitHead,
    index_checksum: Option<String>,
}

impl WatchObservation {
    fn capture(
        project: &ProjectIdentity,
        discoverer: &GitRepositoryDiscoverer,
        control: &WatchDiscoveryControl,
    ) -> Result<Self, ()> {
        let repository = open_validated(project).map_err(|_| ())?;
        let head = inspect_head(&repository).map_err(|_| ())?;
        let index_checksum = inspect_index_checksum(&repository).map_err(|_| ())?;
        let discovery = discoverer
            .discover(project, DiscoveryPolicy::v1(), control)
            .map_err(|_| ())?;
        let mut files = BTreeMap::new();
        for file in discovery.files() {
            if control.is_cancelled() {
                return Err(());
            }
            let observation =
                observe_repository_path(project.worktree().root().as_path(), file.path())
                    .map_err(|_| ())?;
            let RepositoryPathObservation::Present { metadata, .. } = observation else {
                return Err(());
            };
            files.insert(file.path().clone(), WatchStamp::from_metadata(&metadata));
        }
        Ok(Self {
            files,
            head,
            index_checksum,
        })
    }

    fn compare(&self, current: &Self) -> ObservationComparison {
        let paths = self
            .files
            .keys()
            .chain(current.files.keys())
            .filter(|path| self.files.get(*path) != current.files.get(*path))
            .cloned()
            .collect();
        ObservationComparison {
            paths,
            metadata_changed: self.head != current.head
                || self.index_checksum != current.index_checksum,
        }
    }
}

#[derive(Debug)]
struct ObservationComparison {
    paths: BTreeSet<RepositoryPath>,
    metadata_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchStamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    creation_time: u64,
    #[cfg(windows)]
    last_write_time: u64,
}

impl WatchStamp {
    #[cfg(unix)]
    fn from_metadata(metadata: &Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }

    #[cfg(windows)]
    fn from_metadata(metadata: &Metadata) -> Self {
        use std::os::windows::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            attributes: metadata.file_attributes(),
            creation_time: metadata.creation_time(),
            last_write_time: metadata.last_write_time(),
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

#[derive(Debug, Default)]
struct PendingChanges {
    paths: BTreeSet<RepositoryPath>,
    full_rescan: Option<RepositoryRescanReason>,
    first_observed: Option<Instant>,
    last_observed: Option<Instant>,
}

impl PendingChanges {
    fn record(&mut self, paths: BTreeSet<RepositoryPath>, observed_at: Instant) {
        self.paths.extend(paths);
        self.first_observed.get_or_insert(observed_at);
        self.last_observed = Some(observed_at);
    }

    fn require_rescan(&mut self, reason: RepositoryRescanReason, observed_at: Instant) {
        self.full_rescan = Some(
            self.full_rescan
                .map_or(reason, |current| current.max(reason)),
        );
        self.first_observed.get_or_insert(observed_at);
        self.last_observed = Some(observed_at);
    }

    fn try_deliver(
        &mut self,
        sender: &Sender<RepositoryChangeBatch>,
        config: RepositoryWatcherConfig,
        now: Instant,
    ) {
        let (Some(first), Some(last)) = (self.first_observed, self.last_observed) else {
            return;
        };
        if now.saturating_duration_since(last) < config.debounce()
            && now.saturating_duration_since(first) < config.max_batch_latency()
        {
            return;
        }
        let paths = self.paths.iter().cloned().collect::<Vec<_>>();
        let batch = match self.full_rescan {
            Some(reason) => RepositoryChangeBatch::full_rescan(paths, reason),
            None => RepositoryChangeBatch::incremental(paths),
        };
        let Ok(batch) = batch else {
            self.require_rescan(RepositoryRescanReason::EventLoss, now);
            return;
        };
        match sender.try_send(batch) {
            Ok(()) => *self = Self::default(),
            Err(TrySendError::Full(_)) => {
                self.require_rescan(RepositoryRescanReason::EventLoss, now);
            }
            Err(TrySendError::Disconnected(_)) => *self = Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingChanges, RepositoryWatcherConfig};
    use a3_application::RepositoryRescanReason;
    use a3_domain::RepositoryPath;
    use crossbeam_channel::bounded;
    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};

    #[test]
    fn coalescer_marks_queue_overflow_as_event_loss() -> Result<(), Box<dyn std::error::Error>> {
        let config = RepositoryWatcherConfig::new(
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(20),
            1,
        )?;
        let (sender, receiver) = bounded(1);
        let now = Instant::now();
        let first = RepositoryPath::try_from_bytes(b"first.rs".to_vec())?;
        let second = RepositoryPath::try_from_bytes(b"second.rs".to_vec())?;
        let mut pending = PendingChanges::default();
        pending.record(BTreeSet::from([first]), now);
        pending.try_deliver(&sender, config, now + Duration::from_millis(20));
        pending.record(BTreeSet::from([second]), now + Duration::from_millis(30));
        pending.try_deliver(&sender, config, now + Duration::from_millis(50));
        assert_eq!(pending.full_rescan, Some(RepositoryRescanReason::EventLoss));
        let _first_batch = receiver.recv()?;
        pending.try_deliver(&sender, config, now + Duration::from_millis(70));
        let recovery = receiver.recv()?;
        assert_eq!(
            recovery.full_rescan_reason(),
            Some(RepositoryRescanReason::EventLoss)
        );
        Ok(())
    }
}
