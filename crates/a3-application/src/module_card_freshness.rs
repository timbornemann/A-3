use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{IndexRunId, InvalidationReason, Progress, ProjectIdentity, SnapshotId};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_REASON_COUNTS: usize = 5;

/// Invalid lifecycle states exposed by the bounded freshness projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleCardFreshnessStatus {
    /// The card's own evidence or tool compatibility changed.
    Stale,
    /// A directly depended-on module changed.
    NeedsReview,
}

/// One positive, auditable lifecycle-reason count in a freshness projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardFreshnessReasonCount {
    status: ModuleCardFreshnessStatus,
    reason: InvalidationReason,
    count: u64,
}

impl ModuleCardFreshnessReasonCount {
    /// Creates a legal stale or needs-review reason bucket.
    pub fn new(
        status: ModuleCardFreshnessStatus,
        reason: InvalidationReason,
        count: u64,
    ) -> Result<Self, ModuleCardFreshnessReasonCountError> {
        if count == 0 {
            return Err(ModuleCardFreshnessReasonCountError::ZeroCount);
        }
        let valid = match status {
            ModuleCardFreshnessStatus::Stale => matches!(
                reason,
                InvalidationReason::EvidenceChanged
                    | InvalidationReason::ModuleRemoved
                    | InvalidationReason::ParserVersionChanged
                    | InvalidationReason::MapperVersionChanged
            ),
            ModuleCardFreshnessStatus::NeedsReview => {
                reason == InvalidationReason::DirectDependencyChanged
            }
        };
        if !valid {
            return Err(ModuleCardFreshnessReasonCountError::InvalidStatusReason);
        }
        Ok(Self {
            status,
            reason,
            count,
        })
    }

    /// Returns whether this bucket belongs to stale or needs-review cards.
    #[must_use]
    pub const fn status(self) -> ModuleCardFreshnessStatus {
        self.status
    }

    /// Returns the durable invalidation reason.
    #[must_use]
    pub const fn reason(self) -> InvalidationReason {
        self.reason
    }

    /// Returns the exact positive count.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// A reason bucket was empty or paired an illegal lifecycle and reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardFreshnessReasonCountError {
    /// Zero-count buckets must be omitted from the bounded projection.
    ZeroCount,
    /// Only direct reasons may be stale and only dependency changes may need review.
    InvalidStatusReason,
}

impl fmt::Display for ModuleCardFreshnessReasonCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroCount => "module-card freshness reason count must be positive",
            Self::InvalidStatusReason => "module-card freshness status and reason are incompatible",
        })
    }
}

impl Error for ModuleCardFreshnessReasonCountError {}

/// Authoritative current lifecycle counts bound to one atomic index publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardFreshness {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    published_count: u64,
    stale_count: u64,
    needs_review_count: u64,
    total_count: u64,
    reason_counts: Vec<ModuleCardFreshnessReasonCount>,
}

impl ModuleCardFreshness {
    /// Validates exact aggregate counts and canonical reason ordering.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        published_count: u64,
        stale_count: u64,
        needs_review_count: u64,
        reason_counts: Vec<ModuleCardFreshnessReasonCount>,
    ) -> Result<Self, ModuleCardFreshnessError> {
        if reason_counts.len() > MAX_REASON_COUNTS {
            return Err(ModuleCardFreshnessError::TooManyReasonCounts);
        }
        if reason_counts.windows(2).any(|pair| {
            (pair[0].status(), pair[0].reason()) >= (pair[1].status(), pair[1].reason())
        }) {
            return Err(ModuleCardFreshnessError::UnstableReasonOrder);
        }
        let (counted_stale, counted_needs_review) = reason_counts
            .iter()
            .try_fold((0_u64, 0_u64), |(stale, needs_review), item| {
                match item.status() {
                    ModuleCardFreshnessStatus::Stale => item
                        .count()
                        .checked_add(stale)
                        .map(|count| (count, needs_review)),
                    ModuleCardFreshnessStatus::NeedsReview => item
                        .count()
                        .checked_add(needs_review)
                        .map(|count| (stale, count)),
                }
            })
            .ok_or(ModuleCardFreshnessError::CountOverflow)?;
        if counted_stale != stale_count || counted_needs_review != needs_review_count {
            return Err(ModuleCardFreshnessError::ContradictoryCounts);
        }
        let total_count = published_count
            .checked_add(stale_count)
            .and_then(|count| count.checked_add(needs_review_count))
            .ok_or(ModuleCardFreshnessError::CountOverflow)?;
        Ok(Self {
            index_run_id,
            snapshot_id,
            published_count,
            stale_count,
            needs_review_count,
            total_count,
            reason_counts,
        })
    }

    /// Returns the current publication that caused this freshness state.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot of the current publication.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns latest cards whose evidence is still current.
    #[must_use]
    pub const fn published_count(&self) -> u64 {
        self.published_count
    }

    /// Returns latest cards invalidated by their own evidence or tooling.
    #[must_use]
    pub const fn stale_count(&self) -> u64 {
        self.stale_count
    }

    /// Returns latest one-hop dependents requiring conservative review.
    #[must_use]
    pub const fn needs_review_count(&self) -> u64 {
        self.needs_review_count
    }

    /// Returns all latest cards exactly once.
    #[must_use]
    pub const fn total_count(&self) -> u64 {
        self.total_count
    }

    /// Returns at most five positive reason buckets in canonical order.
    #[must_use]
    pub fn reason_counts(&self) -> &[ModuleCardFreshnessReasonCount] {
        &self.reason_counts
    }
}

/// Persisted aggregate rows contradicted the lifecycle contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardFreshnessError {
    /// More reason rows crossed the fixed five-reason boundary.
    TooManyReasonCounts,
    /// Reason rows were duplicated or not in canonical order.
    UnstableReasonOrder,
    /// Aggregate lifecycle counts did not equal their reason buckets.
    ContradictoryCounts,
    /// Counts exceeded the lossless unsigned 64-bit boundary.
    CountOverflow,
}

impl fmt::Display for ModuleCardFreshnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyReasonCounts => "module-card freshness has more than five reason counts",
            Self::UnstableReasonOrder => "module-card freshness reason ordering is invalid",
            Self::ContradictoryCounts => "module-card freshness counts are contradictory",
            Self::CountOverflow => "module-card freshness count exceeds u64",
        })
    }
}

impl Error for ModuleCardFreshnessError {}

/// Cooperative cancellation and bounded progress for one freshness read.
pub trait ModuleCardFreshnessControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports the deterministic start and completion phases.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardFreshnessControlError>;
}

impl ModuleCardFreshnessControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardFreshnessControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ModuleCardFreshnessControlError::Unavailable)
    }
}

/// Freshness-read progress could not reach its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardFreshnessControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleCardFreshnessControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module-card freshness progress is unavailable")
    }
}

impl Error for ModuleCardFreshnessControlError {}

/// Owned future returned by the object-safe freshness port.
pub type ModuleCardFreshnessFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<ModuleCardFreshness>, ModuleCardFreshnessFailure>>
            + Send
            + 'a,
    >,
>;

/// Read-only lifecycle projection capability implemented by local persistence.
pub trait ModuleCardFreshnessStore: fmt::Debug + Send + Sync {
    /// Loads latest-card lifecycle counts against the current published index.
    fn load_module_card_freshness<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn ModuleCardFreshnessControl,
    ) -> ModuleCardFreshnessFuture<'a>;
}

/// Stable, content-free failure classification for freshness reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardFreshnessFailure {
    /// Shared local storage failed.
    Storage(KnowledgeStoreFailure),
    /// Persisted lifecycle rows contradicted the projection contract.
    InvalidStoredProjection,
    /// The owner cancelled before a result was delivered.
    Cancelled,
    /// The bounded adapter read exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleCardFreshnessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => {
                write!(formatter, "module-card freshness storage failed: {error}")
            }
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module-card freshness projection is invalid")
            }
            Self::Cancelled => formatter.write_str("module-card freshness read was cancelled"),
            Self::TimedOut => formatter.write_str("module-card freshness read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("module-card freshness progress is unavailable")
            }
        }
    }
}

impl Error for ModuleCardFreshnessFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

/// Small read use case retaining cancellation and progress outside persistence.
#[derive(Debug)]
pub struct GetModuleCardFreshness {
    store: Arc<dyn ModuleCardFreshnessStore>,
}

impl GetModuleCardFreshness {
    /// Wires the narrow lifecycle projection capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleCardFreshnessStore>) -> Self {
        Self { store }
    }

    /// Loads the current atomic freshness projection, if an index is published.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        control: &dyn ModuleCardFreshnessControl,
    ) -> Result<Option<ModuleCardFreshness>, ModuleCardFreshnessFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleCardFreshnessFailure::Cancelled);
        }
        let freshness = self
            .store
            .load_module_card_freshness(project, control)
            .await?;
        if control.is_cancelled() {
            return Err(ModuleCardFreshnessFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(freshness)
    }
}

fn report(
    control: &dyn ModuleCardFreshnessControl,
    completed: u64,
) -> Result<(), ModuleCardFreshnessFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleCardFreshnessFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleCardFreshnessFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        GetModuleCardFreshness, ModuleCardFreshness, ModuleCardFreshnessControl,
        ModuleCardFreshnessControlError, ModuleCardFreshnessFailure, ModuleCardFreshnessFuture,
        ModuleCardFreshnessReasonCount, ModuleCardFreshnessStatus, ModuleCardFreshnessStore,
    };
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, IndexRunId, InvalidationReason, Progress,
        ProjectIdentity, RepositoryId, RepositoryIdentity, SnapshotId, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct StubStore {
        value: Option<ModuleCardFreshness>,
    }

    impl ModuleCardFreshnessStore for StubStore {
        fn load_module_card_freshness<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn ModuleCardFreshnessControl,
        ) -> ModuleCardFreshnessFuture<'a> {
            let value = self.value.clone();
            Box::pin(async move { Ok(value) })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        progress: Mutex<Vec<Progress>>,
    }

    impl ModuleCardFreshnessControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(
            &self,
            progress: Progress,
        ) -> Result<(), ModuleCardFreshnessControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleCardFreshnessControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn summary_rejects_contradictory_or_illegal_reason_counts()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            ModuleCardFreshnessReasonCount::new(
                ModuleCardFreshnessStatus::Stale,
                InvalidationReason::DirectDependencyChanged,
                1,
            )
            .is_err()
        );
        let reason = ModuleCardFreshnessReasonCount::new(
            ModuleCardFreshnessStatus::Stale,
            InvalidationReason::EvidenceChanged,
            1,
        )?;
        assert!(
            ModuleCardFreshness::new(
                IndexRunId::from_bytes([1; 32]),
                SnapshotId::from_bytes([2; 32]),
                0,
                2,
                0,
                vec![reason],
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn query_preserves_exact_publication_and_reports_bounded_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let freshness = ModuleCardFreshness::new(
            IndexRunId::from_bytes([3; 32]),
            SnapshotId::from_bytes([4; 32]),
            7,
            1,
            1,
            vec![
                ModuleCardFreshnessReasonCount::new(
                    ModuleCardFreshnessStatus::Stale,
                    InvalidationReason::EvidenceChanged,
                    1,
                )?,
                ModuleCardFreshnessReasonCount::new(
                    ModuleCardFreshnessStatus::NeedsReview,
                    InvalidationReason::DirectDependencyChanged,
                    1,
                )?,
            ],
        )?;
        let query = GetModuleCardFreshness::new(Arc::new(StubStore {
            value: Some(freshness.clone()),
        }));
        let control = RecordingControl::default();

        assert_eq!(
            block_on(query.execute(&project()?, &control))?,
            Some(freshness)
        );
        let progress = control
            .progress
            .lock()
            .map_err(|_| "progress lock was poisoned")?;
        assert_eq!(progress.len(), 2);
        assert_eq!(
            progress.first().and_then(|value| value.completed()),
            Some(0)
        );
        assert_eq!(progress.last().map(|value| value.is_complete()), Some(true));
        Ok(())
    }

    #[derive(Debug)]
    struct CancelledControl;

    impl ModuleCardFreshnessControl for CancelledControl {
        fn is_cancelled(&self) -> bool {
            true
        }

        fn report_progress(
            &self,
            _progress: Progress,
        ) -> Result<(), ModuleCardFreshnessControlError> {
            Ok(())
        }
    }

    #[test]
    fn query_honours_cancellation_before_storage() -> Result<(), Box<dyn std::error::Error>> {
        let query = GetModuleCardFreshness::new(Arc::new(StubStore { value: None }));
        assert_eq!(
            block_on(query.execute(&project()?, &CancelledControl)),
            Err(ModuleCardFreshnessFailure::Cancelled)
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn std::error::Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([5; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([6; 32]),
            WorktreeAnchorId::from_bytes([7; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
