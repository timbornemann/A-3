use crate::{
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
    EmbeddingRequestTimeout, JobCompletion, JobContext, JobTask, SemanticEmbeddingStore,
    SemanticEmbeddingStoreFailure,
};
use a3_domain::{
    EmbeddingCacheKey, EmbeddingModelProfile, EmbeddingTimestamp, EmbeddingVectorError, Progress,
    ProgressValueError, ProjectIdentity, SemanticCardBatch, SemanticEmbedding,
};
use futures::executor::block_on;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Whether the optional semantic pipeline may touch provider or cache capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingExecutionMode {
    /// Deterministic retrieval remains active while semantic work is a no-op.
    Disabled,
    /// Local provider and regenerable semantic cache may be used.
    Enabled,
}

/// Injected wall-clock boundary for persisted embedding creation metadata.
pub trait EmbeddingClock: fmt::Debug + Send + Sync {
    /// Returns the current non-secret creation timestamp.
    fn now(&self) -> Result<EmbeddingTimestamp, EmbeddingClockFailure>;
}

/// Wall-clock observation could not be represented safely for persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingClockFailure {
    /// System time or its portable conversion was unavailable.
    Unavailable,
}

impl fmt::Display for EmbeddingClockFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("embedding creation time is unavailable")
    }
}

impl Error for EmbeddingClockFailure {}

/// Cancellation plus bounded progress required by the local embedding batch job.
pub trait SemanticEmbeddingJobControl: EmbeddingOperationControl {
    /// Reports monotone card completion against the fixed batch size.
    fn report_progress(&self, progress: Progress) -> Result<(), EmbeddingProgressError>;
}

impl SemanticEmbeddingJobControl for JobContext {
    fn report_progress(&self, progress: Progress) -> Result<(), EmbeddingProgressError> {
        JobContext::report_progress(self, progress).map_err(|_| EmbeddingProgressError::Unavailable)
    }
}

/// Owning job can no longer accept progress updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProgressError {
    /// Scheduler event or lifecycle boundary is unavailable.
    Unavailable,
}

impl fmt::Display for EmbeddingProgressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("embedding job progress is unavailable")
    }
}

impl Error for EmbeddingProgressError {}

/// Observable result of an optional semantic-card batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateSemanticEmbeddingsOutcome {
    /// Optional capability was disabled and neither provider nor cache was touched.
    Disabled {
        /// Number of canonical cards intentionally skipped.
        card_count: usize,
    },
    /// Every card was found in or committed to the local semantic cache.
    Completed {
        /// Total canonical cards in the job.
        card_count: usize,
        /// Cards already present under the exact body/profile key.
        cache_hits: usize,
        /// Cards generated and persisted during this job.
        generated: usize,
    },
}

/// Provider-neutral orchestration of cache lookup, bounded requests, validation, and persistence.
#[derive(Clone)]
pub struct GenerateSemanticEmbeddings {
    runtime: EmbeddingRuntime,
}

#[derive(Clone)]
enum EmbeddingRuntime {
    Disabled,
    Enabled {
        provider: Arc<dyn EmbeddingProvider>,
        store: Arc<dyn SemanticEmbeddingStore>,
        clock: Arc<dyn EmbeddingClock>,
        request_timeout: EmbeddingRequestTimeout,
    },
}

impl GenerateSemanticEmbeddings {
    /// Creates a no-provider, no-cache use case for the default optional-off state.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            runtime: EmbeddingRuntime::Disabled,
        }
    }

    /// Creates an enabled batch use case from all required narrow capabilities.
    #[must_use]
    pub fn enabled(
        provider: Arc<dyn EmbeddingProvider>,
        store: Arc<dyn SemanticEmbeddingStore>,
        clock: Arc<dyn EmbeddingClock>,
        request_timeout: EmbeddingRequestTimeout,
    ) -> Self {
        Self {
            runtime: EmbeddingRuntime::Enabled {
                provider,
                store,
                clock,
                request_timeout,
            },
        }
    }

    /// Returns whether the optional semantic capability is constructively available.
    #[must_use]
    pub const fn mode(&self) -> EmbeddingExecutionMode {
        match self.runtime {
            EmbeddingRuntime::Disabled => EmbeddingExecutionMode::Disabled,
            EmbeddingRuntime::Enabled { .. } => EmbeddingExecutionMode::Enabled,
        }
    }

    /// Generates only missing exact body/profile keys in bounded provider batches.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        profile: &EmbeddingModelProfile,
        cards: SemanticCardBatch,
        control: &dyn SemanticEmbeddingJobControl,
    ) -> Result<GenerateSemanticEmbeddingsOutcome, GenerateSemanticEmbeddingsError> {
        let EmbeddingRuntime::Enabled {
            provider,
            store,
            clock,
            request_timeout,
        } = &self.runtime
        else {
            return Ok(GenerateSemanticEmbeddingsOutcome::Disabled {
                card_count: cards.len(),
            });
        };
        if cards.is_empty() {
            return Ok(GenerateSemanticEmbeddingsOutcome::Completed {
                card_count: 0,
                cache_hits: 0,
                generated: 0,
            });
        }
        if control.is_cancelled() {
            return Err(GenerateSemanticEmbeddingsError::Cancelled);
        }

        let requested = cards
            .cards()
            .iter()
            .map(|card| (EmbeddingCacheKey::from_card(card, profile), card))
            .collect::<BTreeMap<_, _>>();
        let requested_keys = requested.keys().copied().collect::<Vec<_>>();
        let cached_projection = store
            .find_cached(project, profile, &requested_keys, control)
            .await
            .map_err(GenerateSemanticEmbeddingsError::Store)?;
        if control.is_cancelled() {
            return Err(GenerateSemanticEmbeddingsError::Cancelled);
        }

        let mut cached = BTreeSet::new();
        for key in cached_projection {
            if !requested.contains_key(&key) || !cached.insert(key) {
                return Err(GenerateSemanticEmbeddingsError::InvalidCacheProjection);
            }
        }
        report_progress(control, cached.len(), cards.len())?;

        let missing = requested
            .into_iter()
            .filter_map(|(key, card)| (!cached.contains(&key)).then_some(card.clone()))
            .collect::<Vec<_>>();
        let batch_size = usize::from(profile.max_batch_size().get());
        let mut generated = 0_usize;
        for batch in missing.chunks(batch_size) {
            if control.is_cancelled() {
                return Err(GenerateSemanticEmbeddingsError::Cancelled);
            }
            let raw = provider
                .embed(profile, batch, *request_timeout, control)
                .await
                .map_err(GenerateSemanticEmbeddingsError::Provider)?;
            if raw.len() != batch.len() {
                return Err(
                    GenerateSemanticEmbeddingsError::ProviderResultCountMismatch {
                        expected: batch.len(),
                        actual: raw.len(),
                    },
                );
            }
            if control.is_cancelled() {
                return Err(GenerateSemanticEmbeddingsError::Cancelled);
            }

            let mut embeddings = Vec::with_capacity(batch.len());
            for (index, (card, components)) in
                batch.iter().cloned().zip(raw.into_vectors()).enumerate()
            {
                let created_at = clock
                    .now()
                    .map_err(GenerateSemanticEmbeddingsError::Clock)?;
                let embedding =
                    SemanticEmbedding::from_provider_output(card, profile, components, created_at)
                        .map_err(|source| {
                            GenerateSemanticEmbeddingsError::InvalidProviderVector { index, source }
                        })?;
                embeddings.push(embedding);
            }
            store
                .store_batch(project, profile, &embeddings, control)
                .await
                .map_err(GenerateSemanticEmbeddingsError::Store)?;
            generated = generated.saturating_add(embeddings.len());
            report_progress(control, cached.len().saturating_add(generated), cards.len())?;
        }

        Ok(GenerateSemanticEmbeddingsOutcome::Completed {
            card_count: cards.len(),
            cache_hits: cached.len(),
            generated,
        })
    }
}

impl fmt::Debug for GenerateSemanticEmbeddings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenerateSemanticEmbeddings")
            .field("mode", &self.mode())
            .finish_non_exhaustive()
    }
}

/// Scheduler-owned low-priority local embedding task with an explicit shutdown path.
pub struct SemanticEmbeddingBatchJob {
    use_case: GenerateSemanticEmbeddings,
    project: ProjectIdentity,
    profile: EmbeddingModelProfile,
    cards: SemanticCardBatch,
}

impl SemanticEmbeddingBatchJob {
    /// Owns every input required for one scheduler execution.
    #[must_use]
    pub const fn new(
        use_case: GenerateSemanticEmbeddings,
        project: ProjectIdentity,
        profile: EmbeddingModelProfile,
        cards: SemanticCardBatch,
    ) -> Self {
        Self {
            use_case,
            project,
            profile,
            cards,
        }
    }
}

impl fmt::Debug for SemanticEmbeddingBatchJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticEmbeddingBatchJob")
            .field("profile_id", &self.profile.id())
            .field("cards", &self.cards)
            .finish_non_exhaustive()
    }
}

impl JobTask for SemanticEmbeddingBatchJob {
    fn run(self: Box<Self>, context: JobContext) -> JobCompletion {
        match block_on(
            self.use_case
                .execute(&self.project, &self.profile, self.cards, &context),
        ) {
            Ok(_) => JobCompletion::Succeeded,
            Err(error) if error.is_cancellation() => JobCompletion::Cancelled,
            Err(_) => JobCompletion::Failed,
        }
    }
}

/// Failure of cache-aware embedding batch orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateSemanticEmbeddingsError {
    /// The owning job cancelled before a boundary completed.
    Cancelled,
    /// Local provider failed without exposing payload or endpoint details.
    Provider(EmbeddingProviderFailure),
    /// Semantic cache failed without exposing SQL or engine rows.
    Store(SemanticEmbeddingStoreFailure),
    /// Injected wall clock could not provide portable creation metadata.
    Clock(EmbeddingClockFailure),
    /// Cache returned an unrequested or duplicate key.
    InvalidCacheProjection,
    /// Provider did not retain one-vector-per-card ordering.
    ProviderResultCountMismatch {
        /// Requested card count.
        expected: usize,
        /// Returned vector count.
        actual: usize,
    },
    /// Provider vector failed dimension, finiteness, or norm validation.
    InvalidProviderVector {
        /// Zero-based index within the bounded provider batch.
        index: usize,
        /// Stable vector validation failure.
        source: EmbeddingVectorError,
    },
    /// Fixed job progress values could not be represented.
    InvalidProgress(ProgressValueError),
    /// Platform cardinality could not fit the portable progress representation.
    ProgressCountOverflow,
    /// Owning scheduler rejected progress delivery.
    ProgressUnavailable(EmbeddingProgressError),
}

impl GenerateSemanticEmbeddingsError {
    /// Returns whether the scheduler should end this job as cancelled rather than failed.
    #[must_use]
    pub const fn is_cancellation(&self) -> bool {
        matches!(
            self,
            Self::Cancelled
                | Self::Provider(EmbeddingProviderFailure::Cancelled)
                | Self::Store(SemanticEmbeddingStoreFailure::Cancelled)
        )
    }
}

impl fmt::Display for GenerateSemanticEmbeddingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("semantic embedding job was cancelled"),
            Self::Provider(source) => write!(formatter, "embedding provider failed: {source}"),
            Self::Store(source) => write!(formatter, "semantic cache failed: {source}"),
            Self::Clock(source) => source.fmt(formatter),
            Self::InvalidCacheProjection => {
                formatter.write_str("semantic cache returned an invalid key projection")
            }
            Self::ProviderResultCountMismatch { expected, actual } => write!(
                formatter,
                "embedding provider returned {actual} vectors for {expected} cards"
            ),
            Self::InvalidProviderVector { index, source } => {
                write!(
                    formatter,
                    "embedding provider vector {index} is invalid: {source}"
                )
            }
            Self::InvalidProgress(source) => {
                write!(formatter, "embedding job progress is invalid: {source}")
            }
            Self::ProgressCountOverflow => {
                formatter.write_str("embedding job progress count exceeds the portable boundary")
            }
            Self::ProgressUnavailable(source) => source.fmt(formatter),
        }
    }
}

impl Error for GenerateSemanticEmbeddingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::Clock(source) => Some(source),
            Self::InvalidProviderVector { source, .. } => Some(source),
            Self::InvalidProgress(source) => Some(source),
            Self::ProgressUnavailable(source) => Some(source),
            Self::Cancelled
            | Self::InvalidCacheProjection
            | Self::ProviderResultCountMismatch { .. }
            | Self::ProgressCountOverflow => None,
        }
    }
}

fn report_progress(
    control: &dyn SemanticEmbeddingJobControl,
    completed: usize,
    total: usize,
) -> Result<(), GenerateSemanticEmbeddingsError> {
    let completed = u64::try_from(completed)
        .map_err(|_| GenerateSemanticEmbeddingsError::ProgressCountOverflow)?;
    let total =
        u64::try_from(total).map_err(|_| GenerateSemanticEmbeddingsError::ProgressCountOverflow)?;
    let progress = Progress::determinate(completed, total)
        .map_err(GenerateSemanticEmbeddingsError::InvalidProgress)?;
    control
        .report_progress(progress)
        .map_err(GenerateSemanticEmbeddingsError::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        EmbeddingClock, EmbeddingClockFailure, EmbeddingProgressError, GenerateSemanticEmbeddings,
        GenerateSemanticEmbeddingsError, GenerateSemanticEmbeddingsOutcome,
        SemanticEmbeddingBatchJob, SemanticEmbeddingJobControl,
    };
    use crate::{
        EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
        EmbeddingProviderFuture, EmbeddingRequestTimeout, JobClock, JobEventKind, JobScheduler,
        JobSchedulerConfig, JobTimestamp, RawEmbeddingBatch, SemanticEmbeddingStore,
        SemanticEmbeddingStoreFailure, SemanticEmbeddingStoreFuture, ShutdownMode,
    };
    use a3_domain::{
        CanonicalDirectory, EmbeddingBatchSize, EmbeddingCacheKey, EmbeddingDimension,
        EmbeddingModelId, EmbeddingModelProfile, EmbeddingProviderId, EmbeddingTimestamp,
        EmbeddingVector, GitHead, GitReferenceName, JobId, JobOwner, NormalizedSemanticCard,
        Progress, ProjectIdentity, RepositoryId, RepositoryIdentity, SemanticCardBatch,
        SemanticCardId, SemanticEmbedding, SnapshotId, VectorSearchCapability, VectorSearchLimit,
        VectorSearchResult, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::collections::BTreeSet;
    use std::io;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::Duration;

    #[derive(Debug)]
    struct StubProvider {
        calls: AtomicUsize,
        returned_dimension: usize,
    }

    impl EmbeddingProvider for StubProvider {
        fn embed<'a>(
            &'a self,
            _profile: &'a EmbeddingModelProfile,
            cards: &'a [NormalizedSemanticCard],
            _timeout: EmbeddingRequestTimeout,
            control: &'a dyn EmbeddingOperationControl,
        ) -> EmbeddingProviderFuture<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let dimension = self.returned_dimension;
            Box::pin(async move {
                if control.is_cancelled() {
                    return Err(EmbeddingProviderFailure::Cancelled);
                }
                let vectors = cards
                    .iter()
                    .map(|_| {
                        let mut values = vec![0.0; dimension];
                        if let Some(first) = values.first_mut() {
                            *first = 1.0;
                        }
                        values
                    })
                    .collect();
                RawEmbeddingBatch::new(vectors)
                    .map_err(|_| EmbeddingProviderFailure::InvalidResponse)
            })
        }
    }

    #[derive(Debug, Default)]
    struct StubStore {
        cached: Mutex<BTreeSet<EmbeddingCacheKey>>,
        lookups: AtomicUsize,
        writes: AtomicUsize,
    }

    impl SemanticEmbeddingStore for StubStore {
        fn find_cached<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _profile: &'a EmbeddingModelProfile,
            keys: &'a [EmbeddingCacheKey],
            control: &'a dyn EmbeddingOperationControl,
        ) -> SemanticEmbeddingStoreFuture<'a, Vec<EmbeddingCacheKey>> {
            self.lookups.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                if control.is_cancelled() {
                    return Err(SemanticEmbeddingStoreFailure::Cancelled);
                }
                let cached = lock_recovering_poison(&self.cached);
                Ok(keys
                    .iter()
                    .copied()
                    .filter(|key| cached.contains(key))
                    .collect())
            })
        }

        fn store_batch<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _profile: &'a EmbeddingModelProfile,
            embeddings: &'a [SemanticEmbedding],
            control: &'a dyn EmbeddingOperationControl,
        ) -> SemanticEmbeddingStoreFuture<'a, ()> {
            self.writes.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                if control.is_cancelled() {
                    return Err(SemanticEmbeddingStoreFailure::Cancelled);
                }
                let mut cached = lock_recovering_poison(&self.cached);
                cached.extend(embeddings.iter().map(SemanticEmbedding::cache_key));
                Ok(())
            })
        }

        fn vector_search_capability<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _profile: &'a EmbeddingModelProfile,
            _control: &'a dyn EmbeddingOperationControl,
        ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchCapability> {
            Box::pin(async { Ok(VectorSearchCapability::LinearFallback) })
        }

        fn search_similar<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            snapshot_id: SnapshotId,
            profile: &'a EmbeddingModelProfile,
            _query: &'a EmbeddingVector,
            limit: VectorSearchLimit,
            _control: &'a dyn EmbeddingOperationControl,
        ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchResult> {
            Box::pin(async move {
                VectorSearchResult::new(
                    snapshot_id,
                    profile.id(),
                    VectorSearchCapability::LinearFallback,
                    limit,
                    Vec::new(),
                    false,
                )
                .map_err(|_| SemanticEmbeddingStoreFailure::InvalidStoredData)
            })
        }

        fn rebuild_semantic_cache<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn crate::SemanticCacheRebuildControl,
        ) -> SemanticEmbeddingStoreFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Debug)]
    struct FakeClock(AtomicU64);

    impl EmbeddingClock for FakeClock {
        fn now(&self) -> Result<EmbeddingTimestamp, EmbeddingClockFailure> {
            EmbeddingTimestamp::from_unix_millis(self.0.fetch_add(1, Ordering::Relaxed))
                .map_err(|_| EmbeddingClockFailure::Unavailable)
        }
    }

    #[derive(Debug, Default)]
    struct SchedulerClock(AtomicU64);

    impl JobClock for SchedulerClock {
        fn now(&self) -> JobTimestamp {
            JobTimestamp::from_millis(self.0.fetch_add(1, Ordering::Relaxed))
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        cancelled: AtomicBool,
        progress: Mutex<Vec<Progress>>,
    }

    impl EmbeddingOperationControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Acquire)
        }
    }

    impl SemanticEmbeddingJobControl for RecordingControl {
        fn report_progress(&self, progress: Progress) -> Result<(), EmbeddingProgressError> {
            lock_recovering_poison(&self.progress).push(progress);
            Ok(())
        }
    }

    #[test]
    fn disabled_mode_touches_neither_provider_nor_store() -> Result<(), Box<dyn std::error::Error>>
    {
        let provider = Arc::new(StubProvider {
            calls: AtomicUsize::new(0),
            returned_dimension: 2,
        });
        let store = Arc::new(StubStore::default());
        let use_case = GenerateSemanticEmbeddings::disabled();
        let batch = cards(1)?;

        let outcome = block_on(use_case.execute(
            &project()?,
            &profile(2, 2)?,
            batch,
            &RecordingControl::default(),
        ))?;

        assert_eq!(
            outcome,
            GenerateSemanticEmbeddingsOutcome::Disabled { card_count: 1 }
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        assert_eq!(store.lookups.load(Ordering::Relaxed), 0);
        assert_eq!(store.writes.load(Ordering::Relaxed), 0);
        Ok(())
    }

    #[test]
    fn enabled_mode_uses_exact_cache_keys_batches_and_monotone_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(StubProvider {
            calls: AtomicUsize::new(0),
            returned_dimension: 2,
        });
        let store = Arc::new(StubStore::default());
        let profile = profile(2, 2)?;
        let batch = cards(3)?;
        let cached_key = EmbeddingCacheKey::from_card(&batch.cards()[0], &profile);
        lock_recovering_poison(&store.cached).insert(cached_key);
        let use_case = GenerateSemanticEmbeddings::enabled(
            provider.clone(),
            store.clone(),
            Arc::new(FakeClock(AtomicU64::new(10))),
            EmbeddingRequestTimeout::DEFAULT,
        );
        let control = RecordingControl::default();

        let outcome = block_on(use_case.execute(&project()?, &profile, batch, &control))?;

        assert_eq!(
            outcome,
            GenerateSemanticEmbeddingsOutcome::Completed {
                card_count: 3,
                cache_hits: 1,
                generated: 2,
            }
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
        assert_eq!(store.writes.load(Ordering::Relaxed), 1);
        let progress = lock_recovering_poison(&control.progress);
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].completed(), Some(1));
        assert_eq!(progress[1].completed(), Some(3));
        assert!(progress[1].is_complete());
        Ok(())
    }

    #[test]
    fn invalid_provider_dimension_is_rejected_before_storage()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(StubProvider {
            calls: AtomicUsize::new(0),
            returned_dimension: 1,
        });
        let store = Arc::new(StubStore::default());
        let use_case = GenerateSemanticEmbeddings::enabled(
            provider,
            store.clone(),
            Arc::new(FakeClock(AtomicU64::new(1))),
            EmbeddingRequestTimeout::DEFAULT,
        );

        let result = block_on(use_case.execute(
            &project()?,
            &profile(2, 2)?,
            cards(1)?,
            &RecordingControl::default(),
        ));

        assert!(matches!(
            result,
            Err(GenerateSemanticEmbeddingsError::InvalidProviderVector { .. })
        ));
        assert_eq!(store.writes.load(Ordering::Relaxed), 0);
        Ok(())
    }

    #[test]
    fn cancellation_stops_before_cache_or_provider_access() -> Result<(), Box<dyn std::error::Error>>
    {
        let provider = Arc::new(StubProvider {
            calls: AtomicUsize::new(0),
            returned_dimension: 2,
        });
        let store = Arc::new(StubStore::default());
        let use_case = GenerateSemanticEmbeddings::enabled(
            provider.clone(),
            store.clone(),
            Arc::new(FakeClock(AtomicU64::new(1))),
            EmbeddingRequestTimeout::DEFAULT,
        );
        let control = RecordingControl::default();
        control.cancelled.store(true, Ordering::Release);

        assert_eq!(
            block_on(use_case.execute(&project()?, &profile(2, 2)?, cards(1)?, &control)),
            Err(GenerateSemanticEmbeddingsError::Cancelled)
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        assert_eq!(store.lookups.load(Ordering::Relaxed), 0);
        Ok(())
    }

    #[test]
    fn scheduler_owns_batch_job_progress_and_completion() -> Result<(), Box<dyn std::error::Error>>
    {
        let provider = Arc::new(StubProvider {
            calls: AtomicUsize::new(0),
            returned_dimension: 2,
        });
        let store = Arc::new(StubStore::default());
        let use_case = GenerateSemanticEmbeddings::enabled(
            provider,
            store,
            Arc::new(FakeClock(AtomicU64::new(1))),
            EmbeddingRequestTimeout::DEFAULT,
        );
        let job = SemanticEmbeddingBatchJob::new(use_case, project()?, profile(2, 2)?, cards(2)?);
        let scheduler_clock: Arc<dyn JobClock> = Arc::new(SchedulerClock::default());
        let (scheduler, events) =
            JobScheduler::new(JobSchedulerConfig::new(1, 1, 16)?, scheduler_clock)?;
        scheduler.submit(JobId::new(41), JobOwner::new(7), job)?;

        let mut saw_complete_progress = false;
        let mut succeeded = false;
        while !succeeded {
            let event = events
                .next_timeout(Duration::from_secs(2))?
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "embedding job timed out")
                })?;
            match event.kind() {
                JobEventKind::Progressed(progress) => {
                    saw_complete_progress |= progress.is_complete();
                }
                JobEventKind::Succeeded => succeeded = true,
                JobEventKind::Failed | JobEventKind::Cancelled => {
                    return Err(io::Error::other("embedding job did not succeed").into());
                }
                JobEventKind::Queued
                | JobEventKind::Started
                | JobEventKind::CancellationRequested => {}
            }
        }

        assert!(saw_complete_progress);
        let shutdown = scheduler.shutdown(ShutdownMode::Drain)?;
        assert_eq!(shutdown.joined_workers(), 1);
        Ok(())
    }

    fn profile(
        dimension: u16,
        batch_size: u16,
    ) -> Result<EmbeddingModelProfile, Box<dyn std::error::Error>> {
        Ok(EmbeddingModelProfile::v1(
            EmbeddingProviderId::new("local".to_owned())?,
            EmbeddingModelId::new("embed-v1".to_owned())?,
            EmbeddingDimension::new(dimension)?,
            EmbeddingBatchSize::new(batch_size)?,
        ))
    }

    fn cards(count: u8) -> Result<SemanticCardBatch, Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([9; 32]);
        let cards = (0..count)
            .map(|index| {
                NormalizedSemanticCard::normalize_v1(
                    SemanticCardId::from_bytes([index; 32]),
                    snapshot_id,
                    &format!("semantic card {index}"),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemanticCardBatch::new(snapshot_id, cards)?)
    }

    fn project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )
        .map_err(Into::into)
    }

    fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}
