use crate::{JobContext, KnowledgeStoreFailure, ModuleCardLifecycle};
use a3_domain::{
    FileRevision, GraphEdge, IndexRunId, ModuleCardEvidenceId, ModuleCardId, ModuleId, Progress,
    ProjectIdentity, SnapshotId, SymbolId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Capability-bound request for one Evidence ID exposed by a visible Module Card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardEvidenceQuery {
    current_index_run_id: IndexRunId,
    current_snapshot_id: SnapshotId,
    source_index_run_id: IndexRunId,
    source_snapshot_id: SnapshotId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    evidence_id: ModuleCardEvidenceId,
}

impl ModuleCardEvidenceQuery {
    /// Retains every Core-issued selection anchor so the store can reject publication races.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        current_index_run_id: IndexRunId,
        current_snapshot_id: SnapshotId,
        source_index_run_id: IndexRunId,
        source_snapshot_id: SnapshotId,
        card_id: ModuleCardId,
        module_id: ModuleId,
        evidence_id: ModuleCardEvidenceId,
    ) -> Self {
        Self {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            evidence_id,
        }
    }

    /// Returns the publication run visible when the user selected the Evidence hook.
    #[must_use]
    pub const fn current_index_run_id(self) -> IndexRunId {
        self.current_index_run_id
    }

    /// Returns the publication snapshot visible when the user selected the Evidence hook.
    #[must_use]
    pub const fn current_snapshot_id(self) -> SnapshotId {
        self.current_snapshot_id
    }

    /// Returns the historical run that verified the selected Card.
    #[must_use]
    pub const fn source_index_run_id(self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the immutable snapshot carrying the stored Evidence payload.
    #[must_use]
    pub const fn source_snapshot_id(self) -> SnapshotId {
        self.source_snapshot_id
    }

    /// Returns the exact visible Card identity.
    #[must_use]
    pub const fn card_id(self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the current primary module selected by the user.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }

    /// Returns the opaque Evidence hook selected from that Card.
    #[must_use]
    pub const fn evidence_id(self) -> ModuleCardEvidenceId {
        self.evidence_id
    }
}

/// Whether the immutable Evidence payload still resolves in the latest atomic publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardEvidenceFreshness {
    /// The exact content-bound payload is present in the latest published index.
    Current,
    /// The payload remains audit history but no longer resolves in the latest index.
    Stale,
}

/// Bounded typed Provenance retained by a verified Module Card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCardEvidencePayload {
    /// Exact content-addressed repository file revision.
    File {
        /// Historical revision retained by the Card publication.
        revision: FileRevision,
    },
    /// Exact structural symbol identity and its content-addressed file revision.
    Symbol {
        /// Content- and adapter-bound symbol identity.
        symbol_id: SymbolId,
        /// Historical revision that contained the symbol.
        revision: FileRevision,
    },
    /// Exact deterministic graph relation with its source range.
    GraphEdge {
        /// Full historical graph edge retained by the Card publication.
        edge: GraphEdge,
    },
}

impl ModuleCardEvidencePayload {
    fn canonical_id(&self) -> ModuleCardEvidenceId {
        match self {
            Self::File { revision } => ModuleCardEvidenceId::for_file_revision_v1(revision),
            Self::Symbol { symbol_id, .. } => ModuleCardEvidenceId::for_symbol_id_v1(*symbol_id),
            Self::GraphEdge { edge } => ModuleCardEvidenceId::for_graph_edge_v1(edge),
        }
    }

    /// Returns the immutable file revision anchoring every supported Evidence variant.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        match self {
            Self::File { revision } | Self::Symbol { revision, .. } => revision,
            Self::GraphEdge { edge } => edge.evidence().revision(),
        }
    }
}

/// One safely resolved Card Evidence item with independent Card and Evidence freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardEvidenceDetail {
    current_index_run_id: IndexRunId,
    current_snapshot_id: SnapshotId,
    source_index_run_id: IndexRunId,
    source_snapshot_id: SnapshotId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    evidence_id: ModuleCardEvidenceId,
    card_lifecycle: ModuleCardLifecycle,
    freshness: ModuleCardEvidenceFreshness,
    payload: ModuleCardEvidencePayload,
}

impl ModuleCardEvidenceDetail {
    /// Validates canonical identity, snapshot binding, and safe stale presentation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current_index_run_id: IndexRunId,
        current_snapshot_id: SnapshotId,
        source_index_run_id: IndexRunId,
        source_snapshot_id: SnapshotId,
        card_id: ModuleCardId,
        module_id: ModuleId,
        evidence_id: ModuleCardEvidenceId,
        card_lifecycle: ModuleCardLifecycle,
        freshness: ModuleCardEvidenceFreshness,
        payload: ModuleCardEvidencePayload,
    ) -> Result<Self, ModuleCardEvidenceDetailError> {
        let source_binding_is_valid = source_index_run_id != current_index_run_id
            || source_snapshot_id == current_snapshot_id;
        let payload_snapshot_is_valid = match &payload {
            ModuleCardEvidencePayload::GraphEdge { edge } => {
                edge.snapshot_id() == source_snapshot_id
            }
            ModuleCardEvidencePayload::File { .. } | ModuleCardEvidencePayload::Symbol { .. } => {
                true
            }
        };
        let stale_presentation_is_valid = freshness == ModuleCardEvidenceFreshness::Current
            || matches!(card_lifecycle, ModuleCardLifecycle::Stale { .. });
        if !source_binding_is_valid
            || !payload_snapshot_is_valid
            || !stale_presentation_is_valid
            || payload.canonical_id() != evidence_id
        {
            return Err(ModuleCardEvidenceDetailError);
        }
        Ok(Self {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            card_id,
            module_id,
            evidence_id,
            card_lifecycle,
            freshness,
            payload,
        })
    }

    /// Returns the latest publication run used for revalidation.
    #[must_use]
    pub const fn current_index_run_id(&self) -> IndexRunId {
        self.current_index_run_id
    }

    /// Returns the latest publication snapshot used for revalidation.
    #[must_use]
    pub const fn current_snapshot_id(&self) -> SnapshotId {
        self.current_snapshot_id
    }

    /// Returns the historical run that verified the Card.
    #[must_use]
    pub const fn source_index_run_id(&self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the immutable source snapshot carrying the payload.
    #[must_use]
    pub const fn source_snapshot_id(&self) -> SnapshotId {
        self.source_snapshot_id
    }

    /// Returns the exact visible Card identity.
    #[must_use]
    pub const fn card_id(&self) -> ModuleCardId {
        self.card_id
    }

    /// Returns the current primary module identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the canonical opaque Evidence identity.
    #[must_use]
    pub const fn evidence_id(&self) -> ModuleCardEvidenceId {
        self.evidence_id
    }

    /// Returns Card freshness independently from Evidence freshness.
    #[must_use]
    pub const fn card_lifecycle(&self) -> ModuleCardLifecycle {
        self.card_lifecycle
    }

    /// Returns whether this exact payload still resolves in the latest publication.
    #[must_use]
    pub const fn freshness(&self) -> ModuleCardEvidenceFreshness {
        self.freshness
    }

    /// Returns the bounded typed payload without source text or filesystem authority.
    #[must_use]
    pub const fn payload(&self) -> &ModuleCardEvidencePayload {
        &self.payload
    }
}

/// Stored or constructed Evidence contradicted its typed identity or freshness contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardEvidenceDetailError;

impl fmt::Display for ModuleCardEvidenceDetailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module card evidence detail is invalid")
    }
}

impl Error for ModuleCardEvidenceDetailError {}

/// Result of one atomic latest-publication, latest-Card, and Evidence read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCardEvidenceLoadResult {
    /// No index crossed the durable publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic modules.
    ProjectionUnavailable,
    /// The selected ID is absent or names a supplementary graph community.
    ModuleUnavailable,
    /// The current module has no durable verified Card yet.
    CardUnavailable,
    /// A publish or Card replacement made the echoed selection anchors obsolete.
    SelectionChanged,
    /// The opaque ID was not exposed by the selected latest Card.
    EvidenceUnavailable,
    /// One bounded, membership-checked Evidence payload is available.
    Detail(Box<ModuleCardEvidenceDetail>),
}

/// Cooperative cancellation and deterministic progress for Evidence Inspector reads.
pub trait ModuleCardEvidenceControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports bounded start and completion phases.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardEvidenceControlError>;
}

impl ModuleCardEvidenceControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardEvidenceControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ModuleCardEvidenceControlError::Unavailable)
    }
}

/// Evidence Inspector progress could not reach its owning runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardEvidenceControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleCardEvidenceControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module card evidence progress is unavailable")
    }
}

impl Error for ModuleCardEvidenceControlError {}

/// Owned future returned by the object-safe Evidence Inspector port.
pub type ModuleCardEvidenceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ModuleCardEvidenceLoadResult, ModuleCardEvidenceFailure>>
            + Send
            + 'a,
    >,
>;

/// Narrow read-only capability for one Evidence hook of one selected latest Card.
pub trait ModuleCardEvidenceStore: fmt::Debug + Send + Sync {
    /// Revalidates selection and Evidence membership within one atomic read.
    fn load_module_card_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleCardEvidenceQuery,
        control: &'a dyn ModuleCardEvidenceControl,
    ) -> ModuleCardEvidenceFuture<'a>;
}

/// Stable content-free failure classes for Evidence Inspector reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardEvidenceFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the Evidence Inspector contract.
    InvalidStoredProjection,
    /// The owner cancelled before a complete result was delivered.
    Cancelled,
    /// The bounded local read exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleCardEvidenceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => {
                write!(formatter, "module card evidence storage failed: {error}")
            }
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module card evidence projection is invalid")
            }
            Self::Cancelled => formatter.write_str("module card evidence read was cancelled"),
            Self::TimedOut => formatter.write_str("module card evidence read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("module card evidence progress is unavailable")
            }
        }
    }
}

impl Error for ModuleCardEvidenceFailure {
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

/// Application use case retaining cancellation and anchor verification outside persistence.
#[derive(Debug)]
pub struct GetModuleCardEvidence {
    store: Arc<dyn ModuleCardEvidenceStore>,
}

impl GetModuleCardEvidence {
    /// Wires the narrow explicit Evidence Inspector capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleCardEvidenceStore>) -> Self {
        Self { store }
    }

    /// Reads one exact visible Evidence hook or an explicit availability state.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleCardEvidenceQuery,
        control: &dyn ModuleCardEvidenceControl,
    ) -> Result<ModuleCardEvidenceLoadResult, ModuleCardEvidenceFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleCardEvidenceFailure::Cancelled);
        }
        let result = self
            .store
            .load_module_card_evidence(project, query, control)
            .await?;
        if let ModuleCardEvidenceLoadResult::Detail(detail) = &result
            && (detail.current_index_run_id() != query.current_index_run_id()
                || detail.current_snapshot_id() != query.current_snapshot_id()
                || detail.source_index_run_id() != query.source_index_run_id()
                || detail.source_snapshot_id() != query.source_snapshot_id()
                || detail.card_id() != query.card_id()
                || detail.module_id() != query.module_id()
                || detail.evidence_id() != query.evidence_id())
        {
            return Err(ModuleCardEvidenceFailure::InvalidStoredProjection);
        }
        if control.is_cancelled() {
            return Err(ModuleCardEvidenceFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

fn report(
    control: &dyn ModuleCardEvidenceControl,
    completed: u64,
) -> Result<(), ModuleCardEvidenceFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleCardEvidenceFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleCardEvidenceFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        CanonicalDirectory, ContentHash, GitHead, GitReferenceName, RepositoryId,
        RepositoryIdentity, RepositoryPath, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct RecordingStore {
        mismatch: bool,
    }

    impl ModuleCardEvidenceStore for RecordingStore {
        fn load_module_card_evidence<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a ModuleCardEvidenceQuery,
            _control: &'a dyn ModuleCardEvidenceControl,
        ) -> ModuleCardEvidenceFuture<'a> {
            Box::pin(async move {
                let evidence_id = if self.mismatch {
                    ModuleCardEvidenceId::from_bytes([99; 32])
                } else {
                    query.evidence_id()
                };
                let revision = FileRevision::new(
                    RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())
                        .map_err(|_| ModuleCardEvidenceFailure::InvalidStoredProjection)?,
                    ContentHash::from_bytes([8; 32]),
                );
                let canonical = ModuleCardEvidenceId::for_file_revision_v1(&revision);
                let detail = ModuleCardEvidenceDetail::new(
                    query.current_index_run_id(),
                    query.current_snapshot_id(),
                    query.source_index_run_id(),
                    query.source_snapshot_id(),
                    query.card_id(),
                    query.module_id(),
                    if self.mismatch {
                        evidence_id
                    } else {
                        canonical
                    },
                    ModuleCardLifecycle::Current,
                    ModuleCardEvidenceFreshness::Current,
                    ModuleCardEvidencePayload::File { revision },
                )
                .map_err(|_| ModuleCardEvidenceFailure::InvalidStoredProjection)?;
                Ok(ModuleCardEvidenceLoadResult::Detail(Box::new(detail)))
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        cancelled: bool,
        progress: Mutex<Vec<Progress>>,
    }

    impl ModuleCardEvidenceControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(
            &self,
            progress: Progress,
        ) -> Result<(), ModuleCardEvidenceControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleCardEvidenceControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn stale_payload_requires_stale_card_and_canonical_identity() -> Result<(), Box<dyn Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([8; 32]),
        );
        let evidence_id = ModuleCardEvidenceId::for_file_revision_v1(&revision);
        let base = (
            IndexRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            IndexRunId::from_bytes([3; 32]),
            SnapshotId::from_bytes([4; 32]),
            ModuleCardId::from_bytes([5; 32]),
            ModuleId::from_bytes([6; 32]),
        );
        assert!(
            ModuleCardEvidenceDetail::new(
                base.0,
                base.1,
                base.2,
                base.3,
                base.4,
                base.5,
                evidence_id,
                ModuleCardLifecycle::Current,
                ModuleCardEvidenceFreshness::Stale,
                ModuleCardEvidencePayload::File {
                    revision: revision.clone(),
                },
            )
            .is_err()
        );
        assert!(
            ModuleCardEvidenceDetail::new(
                base.0,
                base.1,
                base.2,
                base.3,
                base.4,
                base.5,
                ModuleCardEvidenceId::from_bytes([9; 32]),
                ModuleCardLifecycle::Stale {
                    invalidated_by_index_run_id: base.0,
                    reason: a3_domain::InvalidationReason::EvidenceChanged,
                },
                ModuleCardEvidenceFreshness::Stale,
                ModuleCardEvidencePayload::File { revision },
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn use_case_honors_cancellation_and_rejects_mismatched_result() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([8; 32]),
        );
        let query = ModuleCardEvidenceQuery::new(
            IndexRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            IndexRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            ModuleCardId::from_bytes([5; 32]),
            ModuleId::from_bytes([6; 32]),
            ModuleCardEvidenceId::for_file_revision_v1(&revision),
        );
        let control = RecordingControl::default();
        assert!(matches!(
            block_on(
                GetModuleCardEvidence::new(Arc::new(RecordingStore { mismatch: false }))
                    .execute(&project, &query, &control)
            )?,
            ModuleCardEvidenceLoadResult::Detail(_)
        ));
        assert_eq!(
            control
                .progress
                .lock()
                .map_err(|_| std::io::Error::other("progress lock poisoned"))?
                .len(),
            2
        );
        let cancelled = RecordingControl {
            cancelled: true,
            ..RecordingControl::default()
        };
        assert_eq!(
            block_on(
                GetModuleCardEvidence::new(Arc::new(RecordingStore { mismatch: false }))
                    .execute(&project, &query, &cancelled)
            ),
            Err(ModuleCardEvidenceFailure::Cancelled)
        );
        assert_eq!(
            block_on(
                GetModuleCardEvidence::new(Arc::new(RecordingStore { mismatch: true })).execute(
                    &project,
                    &query,
                    &RecordingControl::default()
                )
            ),
            Err(ModuleCardEvidenceFailure::InvalidStoredProjection)
        );
        Ok(())
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([3; 32]),
            WorktreeAnchorId::from_bytes([4; 32]),
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
