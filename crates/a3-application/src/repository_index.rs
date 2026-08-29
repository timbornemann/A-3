use crate::{
    IncrementalRepositorySnapshotBuilder, IndexPersistenceControl, IndexPersistenceControlError,
    JobContext, KnowledgeIndexFailure, KnowledgeIndexStore, RepositoryChangeBatch,
    RepositorySnapshotBuild, RepositorySnapshotControl, RepositorySnapshotControlError,
    RepositorySnapshotFailure, RepositorySnapshotPolicy, SnapshotBaseline, SnapshotCompatibility,
};
use a3_domain::{
    DiscoveryResult, IndexPublication, IndexRunId, IndexRunStart, IndexRunTerminalOutcome,
    Progress, ProjectIdentity, PublishedIndex, RankingPolicyVersion, RepositoryFileState,
    RepositoryPath, Snapshot, SnapshotDelta,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Cooperative cancellation and monotone end-to-end index progress boundary.
pub trait RepositoryIndexControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning job requested cancellation.
    fn is_cancelled(&self) -> bool;

    /// Reports one monotone phase boundary for the complete refresh.
    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryIndexControlError>;

    /// Reports the current deterministic Fast-Index phase and its fixed six-phase progress.
    fn report_phase(&self, phase: RepositoryIndexPhase) -> Result<(), RepositoryIndexControlError> {
        let progress = Progress::determinate(phase.completed_boundaries(), 6)
            .map_err(|_| RepositoryIndexControlError::Unavailable)?;
        self.report_progress(progress)
    }
}

/// User-visible phase of the deterministic Fast Index defined by ADR-0006.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryIndexPhase {
    /// Discover the bounded, ignore-filtered repository file set.
    Discover,
    /// Hash exact file contents into one immutable snapshot observation.
    Hash,
    /// Parse supported source and manifest files with pinned adapters.
    Parse,
    /// Resolve structural relationships into the snapshot graph.
    Link,
    /// Rank symbols and form deterministic module projections.
    Rank,
    /// Atomically replace the published read model.
    Publish,
}

impl RepositoryIndexPhase {
    /// Returns the number of phase boundaries completed when this phase begins.
    #[must_use]
    pub const fn completed_boundaries(self) -> u64 {
        match self {
            Self::Discover => 0,
            Self::Hash => 1,
            Self::Parse => 2,
            Self::Link => 3,
            Self::Rank => 4,
            Self::Publish => 5,
        }
    }
}

impl RepositoryIndexControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryIndexControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| RepositoryIndexControlError::Unavailable)
    }
}

/// Stable progress-delivery failure for a complete repository refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryIndexControlError {
    /// The owning scheduler no longer accepts progress.
    Unavailable,
}

impl fmt::Display for RepositoryIndexControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository index progress is unavailable")
    }
}

impl Error for RepositoryIndexControlError {}

/// Whether the compiler reused a complete parent-snapshot parse cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryIndexMode {
    /// Every supported file was parsed because no coherent parent cache existed.
    Full,
    /// Only changed supported files were parsed; unchanged artifacts were reused.
    Incremental,
}

/// Complete deterministic compiler result plus objective parse-scope evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryIndexCompilation {
    publication: IndexPublication,
    mode: RepositoryIndexMode,
    parsed_paths: Vec<RepositoryPath>,
}

impl RepositoryIndexCompilation {
    /// Creates a canonical compiler result.
    pub fn new(
        publication: IndexPublication,
        mode: RepositoryIndexMode,
        mut parsed_paths: Vec<RepositoryPath>,
    ) -> Result<Self, RepositoryIndexCompilerFailure> {
        parsed_paths.sort();
        if parsed_paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RepositoryIndexCompilerFailure::InvalidResult);
        }
        Ok(Self {
            publication,
            mode,
            parsed_paths,
        })
    }

    /// Returns the exact payload prepared for atomic publication.
    #[must_use]
    pub const fn publication(&self) -> &IndexPublication {
        &self.publication
    }

    /// Returns whether the parent parse cache was reused.
    #[must_use]
    pub const fn mode(&self) -> RepositoryIndexMode {
        self.mode
    }

    /// Returns exactly the source paths parsed by this refresh.
    #[must_use]
    pub fn parsed_paths(&self) -> &[RepositoryPath] {
        &self.parsed_paths
    }
}

/// Stateful deterministic compiler boundary used by the refresh use case.
pub trait RepositoryIndexCompiler: fmt::Debug + Send {
    /// Returns exact built-in adapter revisions for new snapshots.
    fn compatibility(&self) -> Result<SnapshotCompatibility, RepositoryIndexCompilerFailure>;

    /// Returns the deterministic ranking policy revision applied to publications.
    fn ranking_policy_version(&self) -> RankingPolicyVersion;

    /// Compiles one coherent snapshot, reusing only an exact parent cache.
    fn compile(
        &mut self,
        project: &ProjectIdentity,
        snapshot: &Snapshot,
        files: &RepositoryFileState,
        discovery: &DiscoveryResult,
        delta: &SnapshotDelta,
        control: &dyn RepositoryIndexControl,
    ) -> Result<RepositoryIndexCompilation, RepositoryIndexCompilerFailure>;
}

/// Stable compiler failures without parser, filesystem, or graph implementation details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryIndexCompilerFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// Exact source bytes could not be read safely.
    Filesystem,
    /// Source bytes no longer matched the immutable snapshot revision.
    RevisionMismatch,
    /// A parser lease, parse, link, rank, or complete refresh exceeded its deadline.
    TimedOut,
    /// A fixed file, parse-artifact, graph, or memory limit was exceeded.
    ResourceLimitExceeded,
    /// Progress could not be delivered to the owning job.
    ProgressUnavailable,
    /// Adapter compatibility or deterministic output violated a contract.
    InvalidResult,
}

impl fmt::Display for RepositoryIndexCompilerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "repository index compilation was cancelled",
            Self::Filesystem => "repository source is unavailable",
            Self::RevisionMismatch => "repository source revision changed during compilation",
            Self::TimedOut => "repository index compilation timed out",
            Self::ResourceLimitExceeded => "repository index compilation exceeded a resource limit",
            Self::ProgressUnavailable => "repository index progress is unavailable",
            Self::InvalidResult => "repository index compiler result is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for RepositoryIndexCompilerFailure {}

/// Deterministic worktree-local index-attempt ID boundary.
pub trait IndexRunIdFactory: fmt::Debug + Send + Sync {
    /// Derives a stable collision-resistant ID from the exact attempt coordinates.
    fn create(
        &self,
        project: &ProjectIdentity,
        snapshot: &Snapshot,
        ranking_policy_version: RankingPolicyVersion,
        attempt_ordinal: u64,
    ) -> Result<IndexRunId, IndexRunIdFactoryFailure>;
}

/// Invalid index-attempt coordinates at the adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRunIdFactoryFailure;

impl fmt::Display for IndexRunIdFactoryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("index run identity could not be derived")
    }
}

impl Error for IndexRunIdFactoryFailure {}

/// Observable outcome of one watcher-triggered refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryIndexRefresh {
    snapshot: Snapshot,
    hashed_paths: Vec<RepositoryPath>,
    compilation: RepositoryIndexCompilation,
    published_index: PublishedIndex,
    published: bool,
}

impl RepositoryIndexRefresh {
    /// Returns the exact immutable observation compiled by this refresh.
    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Returns exact complete-content hashing scope.
    #[must_use]
    pub fn hashed_paths(&self) -> &[RepositoryPath] {
        &self.hashed_paths
    }

    /// Returns parse scope and deterministic publication evidence.
    #[must_use]
    pub const fn compilation(&self) -> &RepositoryIndexCompilation {
        &self.compilation
    }

    /// Returns the exact complete index visible after this refresh.
    #[must_use]
    pub const fn published_index(&self) -> &PublishedIndex {
        &self.published_index
    }

    /// Returns whether this refresh made a new run atomically visible.
    #[must_use]
    pub const fn published(&self) -> bool {
        self.published
    }
}

/// Complete application use case from one coalesced watcher batch to atomic publication.
#[derive(Debug)]
pub struct RefreshRepositoryIndex {
    snapshots: Arc<dyn IncrementalRepositorySnapshotBuilder>,
    store: Arc<dyn KnowledgeIndexStore>,
    run_ids: Arc<dyn IndexRunIdFactory>,
}

impl RefreshRepositoryIndex {
    /// Wires narrow snapshot, compiler-ID, and persistence ports.
    #[must_use]
    pub fn new(
        snapshots: Arc<dyn IncrementalRepositorySnapshotBuilder>,
        store: Arc<dyn KnowledgeIndexStore>,
        run_ids: Arc<dyn IndexRunIdFactory>,
    ) -> Self {
        Self {
            snapshots,
            store,
            run_ids,
        }
    }

    /// Confirms, compiles, and if necessary atomically publishes one repository observation.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        changes: &RepositoryChangeBatch,
        compiler: &mut dyn RepositoryIndexCompiler,
        control: &dyn RepositoryIndexControl,
    ) -> Result<RepositoryIndexRefresh, RefreshRepositoryIndexError> {
        control
            .report_phase(RepositoryIndexPhase::Discover)
            .map_err(|_| RefreshRepositoryIndexError::ProgressUnavailable)?;
        ensure_active(control)?;
        let latest = self.store.latest_snapshot(project).await?;
        let files = self.store.current_file_state(project).await?;
        let baseline = SnapshotBaseline::new(latest.clone(), files)
            .map_err(|_| RefreshRepositoryIndexError::InvalidBaseline)?;
        let compatibility = compiler.compatibility()?;
        let muted = MutedControl { inner: control };
        let confirmed = self.snapshots.build_incremental_snapshot(
            project,
            &baseline,
            &compatibility,
            changes,
            RepositorySnapshotPolicy::v1(),
            &muted,
        )?;
        let hashed_paths = confirmed.hashed_paths().to_vec();
        let (snapshot, discovery, current_files, delta, created) =
            observation_parts(confirmed.into_observation(), latest)?;
        if created {
            self.store.append_snapshot(project, &snapshot).await?;
        }
        ensure_active(control)?;

        let ranking_policy_version = compiler.ranking_policy_version();
        let latest_published = self.store.latest_published_index_run(project).await?;
        let publication_required = latest_published.is_none_or(|run| {
            run.snapshot_id() != snapshot.id()
                || run.ranking_policy_version() != ranking_policy_version
        });

        let mut active_run = None;
        if publication_required {
            let next_sequence = match self.store.next_index_run_sequence(project).await {
                Ok(sequence) => sequence,
                Err(KnowledgeIndexFailure::IndexRunSequenceExhausted) => {
                    return Err(RefreshRepositoryIndexError::AttemptExhausted);
                }
                Err(error) => return Err(error.into()),
            };
            let run_id = self.run_ids.create(
                project,
                &snapshot,
                ranking_policy_version,
                next_sequence.get(),
            )?;
            let run = self
                .store
                .start_index_run(
                    project,
                    IndexRunStart::new(
                        run_id,
                        snapshot.id(),
                        ranking_policy_version,
                        next_sequence,
                    ),
                )
                .await?;
            active_run = Some(run.id());
        }

        let compilation = match compiler.compile(
            project,
            &snapshot,
            &current_files,
            &discovery,
            &delta,
            &muted,
        ) {
            Ok(compilation) => compilation,
            Err(error) => {
                if let Some(run_id) = active_run {
                    finish_unpublished_run(self.store.as_ref(), project, run_id, error, control)
                        .await?;
                }
                return Err(error.into());
            }
        };
        if control.report_phase(RepositoryIndexPhase::Publish).is_err() {
            finish_after_refresh_error(self.store.as_ref(), project, active_run, control).await?;
            return Err(RefreshRepositoryIndexError::ProgressUnavailable);
        }
        ensure_active_with_run(self.store.as_ref(), project, active_run, control).await?;

        if let Some(run_id) = active_run
            && let Err(error) = self
                .store
                .publish_index(project, run_id, compilation.publication(), &muted)
                .await
        {
            let outcome = if matches!(error, KnowledgeIndexFailure::Cancelled) {
                IndexRunTerminalOutcome::Cancelled
            } else {
                IndexRunTerminalOutcome::Failed
            };
            self.store
                .finish_index_run(project, run_id, outcome)
                .await?;
            return Err(error.into());
        }
        let published_index = self
            .store
            .latest_published_index(project, &muted)
            .await?
            .ok_or(RefreshRepositoryIndexError::InvalidBaseline)?;
        if published_index.run().snapshot_id() != snapshot.id() {
            return Err(RefreshRepositoryIndexError::InvalidBaseline);
        }
        control
            .report_progress(
                Progress::determinate(6, 6)
                    .map_err(|_| RefreshRepositoryIndexError::ProgressUnavailable)?,
            )
            .map_err(|_| RefreshRepositoryIndexError::ProgressUnavailable)?;

        Ok(RepositoryIndexRefresh {
            snapshot,
            hashed_paths,
            compilation,
            published_index,
            published: active_run.is_some(),
        })
    }
}

async fn finish_after_refresh_error(
    store: &dyn KnowledgeIndexStore,
    project: &ProjectIdentity,
    run_id: Option<IndexRunId>,
    control: &dyn RepositoryIndexControl,
) -> Result<(), RefreshRepositoryIndexError> {
    if let Some(run_id) = run_id {
        let outcome = if control.is_cancelled() {
            IndexRunTerminalOutcome::Cancelled
        } else {
            IndexRunTerminalOutcome::Failed
        };
        store.finish_index_run(project, run_id, outcome).await?;
    }
    Ok(())
}

fn observation_parts(
    observation: RepositorySnapshotBuild,
    latest: Option<Snapshot>,
) -> Result<
    (
        Snapshot,
        DiscoveryResult,
        RepositoryFileState,
        SnapshotDelta,
        bool,
    ),
    RefreshRepositoryIndexError,
> {
    match observation {
        RepositorySnapshotBuild::Created {
            discovery,
            files,
            delta,
            snapshot,
        } => Ok((*snapshot, discovery, files, delta, true)),
        RepositorySnapshotBuild::Unchanged { discovery, files } => latest
            .map(|snapshot| (snapshot, discovery, files, SnapshotDelta::empty(), false))
            .ok_or(RefreshRepositoryIndexError::InvalidBaseline),
    }
}

async fn finish_unpublished_run(
    store: &dyn KnowledgeIndexStore,
    project: &ProjectIdentity,
    run_id: IndexRunId,
    failure: RepositoryIndexCompilerFailure,
    control: &dyn RepositoryIndexControl,
) -> Result<(), RefreshRepositoryIndexError> {
    let outcome = if failure == RepositoryIndexCompilerFailure::Cancelled || control.is_cancelled()
    {
        IndexRunTerminalOutcome::Cancelled
    } else {
        IndexRunTerminalOutcome::Failed
    };
    store.finish_index_run(project, run_id, outcome).await?;
    Ok(())
}

async fn ensure_active_with_run(
    store: &dyn KnowledgeIndexStore,
    project: &ProjectIdentity,
    run_id: Option<IndexRunId>,
    control: &dyn RepositoryIndexControl,
) -> Result<(), RefreshRepositoryIndexError> {
    if control.is_cancelled() {
        if let Some(run_id) = run_id {
            store
                .finish_index_run(project, run_id, IndexRunTerminalOutcome::Cancelled)
                .await?;
        }
        return Err(RefreshRepositoryIndexError::Cancelled);
    }
    Ok(())
}

fn ensure_active(control: &dyn RepositoryIndexControl) -> Result<(), RefreshRepositoryIndexError> {
    if control.is_cancelled() {
        return Err(RefreshRepositoryIndexError::Cancelled);
    }
    Ok(())
}

#[derive(Debug)]
struct MutedControl<'a> {
    inner: &'a dyn RepositoryIndexControl,
}

impl RepositorySnapshotControl for MutedControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositorySnapshotControlError> {
        Ok(())
    }

    fn report_phase(
        &self,
        phase: crate::RepositorySnapshotPhase,
    ) -> Result<(), RepositorySnapshotControlError> {
        let phase = match phase {
            crate::RepositorySnapshotPhase::Discover => return Ok(()),
            crate::RepositorySnapshotPhase::Hash => RepositoryIndexPhase::Hash,
        };
        self.inner
            .report_phase(phase)
            .map_err(|_| RepositorySnapshotControlError::Unavailable)
    }
}

impl RepositoryIndexControl for MutedControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }

    fn report_phase(&self, phase: RepositoryIndexPhase) -> Result<(), RepositoryIndexControlError> {
        self.inner.report_phase(phase)
    }
}

impl IndexPersistenceControl for MutedControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

/// Stable end-to-end refresh failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRepositoryIndexError {
    /// The durable baseline contradicted its snapshot chain.
    InvalidBaseline,
    /// Snapshot confirmation failed.
    Snapshot(RepositorySnapshotFailure),
    /// Deterministic compilation failed.
    Compiler(RepositoryIndexCompilerFailure),
    /// Durable snapshot, run, or publication mutation failed.
    Storage(KnowledgeIndexFailure),
    /// Attempt identity derivation failed.
    RunIdentity(IndexRunIdFactoryFailure),
    /// No later worktree-local attempt coordinate can be represented.
    AttemptExhausted,
    /// The owning job requested cancellation.
    Cancelled,
    /// End-to-end progress could not be delivered.
    ProgressUnavailable,
}

impl fmt::Display for RefreshRepositoryIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseline => formatter.write_str("repository index baseline is invalid"),
            Self::Snapshot(error) => write!(formatter, "snapshot confirmation failed: {error}"),
            Self::Compiler(error) => write!(formatter, "index compilation failed: {error}"),
            Self::Storage(error) => write!(formatter, "index persistence failed: {error}"),
            Self::RunIdentity(error) => write!(formatter, "index run identity failed: {error}"),
            Self::AttemptExhausted => formatter.write_str("index attempt sequence is exhausted"),
            Self::Cancelled => formatter.write_str("repository index refresh was cancelled"),
            Self::ProgressUnavailable => {
                formatter.write_str("repository index refresh progress is unavailable")
            }
        }
    }
}

impl Error for RefreshRepositoryIndexError {}

impl From<RepositorySnapshotFailure> for RefreshRepositoryIndexError {
    fn from(value: RepositorySnapshotFailure) -> Self {
        Self::Snapshot(value)
    }
}

impl From<RepositoryIndexCompilerFailure> for RefreshRepositoryIndexError {
    fn from(value: RepositoryIndexCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}

impl From<KnowledgeIndexFailure> for RefreshRepositoryIndexError {
    fn from(value: KnowledgeIndexFailure) -> Self {
        Self::Storage(value)
    }
}

impl From<IndexRunIdFactoryFailure> for RefreshRepositoryIndexError {
    fn from(value: IndexRunIdFactoryFailure) -> Self {
        Self::RunIdentity(value)
    }
}
