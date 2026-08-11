use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{
    Confidence, IndexRunId, InvalidationReason, MapperProfileVersion, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardSchemaVersion, ModuleId,
    Progress, ProjectIdentity, ProposedModuleCardField, SnapshotId, VerifiedClaimKind,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_CLAIM_EVIDENCE: usize = 16;

/// Stable-ID request for the latest durable card of one current primary module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardDetailQuery {
    module_id: ModuleId,
}

impl ModuleCardDetailQuery {
    /// Selects one module without granting the caller generic claim or SQL access.
    #[must_use]
    pub const fn new(module_id: ModuleId) -> Self {
        Self { module_id }
    }

    /// Returns the current primary module selected by stable identity.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }
}

/// Freshness of the selected card relative to the latest atomic publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardLifecycle {
    /// Card and claims remain valid for the latest publication.
    Current,
    /// Direct evidence or a compatibility boundary invalidated this card.
    Stale {
        /// Publication that recorded the transition.
        invalidated_by_index_run_id: IndexRunId,
        /// Auditable deterministic invalidation cause.
        reason: InvalidationReason,
    },
    /// A direct dependency changed and the card needs conservative review.
    NeedsReview {
        /// Publication that recorded the transition.
        invalidated_by_index_run_id: IndexRunId,
        /// Auditable deterministic invalidation cause.
        reason: InvalidationReason,
    },
}

/// Effective presentation state after card- and claim-level freshness are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardClaimState {
    /// The claim may be presented as current for the selected publication.
    Current,
    /// The claim must be presented as stale, never as a current fact.
    Stale,
    /// The claim must be presented as requiring review, never as a current fact.
    NeedsReview,
}

/// One verified claim classification attached to exactly one displayed field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardClaimPresentation {
    id: ModuleCardClaimId,
    kind: VerifiedClaimKind,
    confidence: Confidence,
    state: ModuleCardClaimState,
    evidence_ids: Vec<ModuleCardEvidenceId>,
}

impl ModuleCardClaimPresentation {
    /// Reconstructs a bounded claim projection without interpreting confidence as status.
    pub fn new(
        id: ModuleCardClaimId,
        kind: VerifiedClaimKind,
        confidence: Confidence,
        state: ModuleCardClaimState,
        evidence_ids: Vec<ModuleCardEvidenceId>,
    ) -> Result<Self, ModuleCardClaimPresentationError> {
        if evidence_ids.len() > MAX_CLAIM_EVIDENCE
            || evidence_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || (evidence_ids.is_empty() && kind != VerifiedClaimKind::Hypothesis)
        {
            return Err(ModuleCardClaimPresentationError);
        }
        Ok(Self {
            id,
            kind,
            confidence,
            state,
            evidence_ids,
        })
    }

    /// Returns the stable verified-claim identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardClaimId {
        self.id
    }

    /// Returns Fact, Observation, or Hypothesis independently from confidence.
    #[must_use]
    pub const fn kind(&self) -> VerifiedClaimKind {
        self.kind
    }

    /// Returns independently verified confidence in basis points.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns the effective freshness label required for safe presentation.
    #[must_use]
    pub const fn state(&self) -> ModuleCardClaimState {
        self.state
    }

    /// Returns exact evidence identities retained for the following inspector slice.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }
}

/// Persisted claim metadata exceeded its bound or contradicted its classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardClaimPresentationError;

impl fmt::Display for ModuleCardClaimPresentationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module card claim presentation is invalid")
    }
}

impl Error for ModuleCardClaimPresentationError {}

/// One bounded field value and the sole verified claim that classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardValuePresentation {
    value: String,
    claim: ModuleCardClaimPresentation,
}

impl ModuleCardValuePresentation {
    /// Binds display text to exactly one typed claim.
    #[must_use]
    pub const fn new(value: String, claim: ModuleCardClaimPresentation) -> Self {
        Self { value, claim }
    }

    /// Returns the validated Module Card value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the claim that determines classification and freshness.
    #[must_use]
    pub const fn claim(&self) -> &ModuleCardClaimPresentation {
        &self.claim
    }
}

/// One canonical V1 Module Card field with field- and claim-level evidence hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardDetailField {
    field: ModuleCardField,
    values: Vec<ModuleCardValuePresentation>,
    evidence_ids: Vec<ModuleCardEvidenceId>,
}

impl ModuleCardDetailField {
    /// Applies the authoritative V1 item, byte, duplicate, and evidence constraints.
    pub fn new(
        field: ModuleCardField,
        values: Vec<ModuleCardValuePresentation>,
        evidence_ids: Vec<ModuleCardEvidenceId>,
    ) -> Result<Self, ModuleCardDetailFieldError> {
        if evidence_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ModuleCardDetailFieldError);
        }
        ProposedModuleCardField::new(
            field,
            values.iter().map(|item| item.value.clone()).collect(),
            evidence_ids.clone(),
        )
        .map_err(|_| ModuleCardDetailFieldError)?;
        let claim_ids = values
            .iter()
            .map(|value| value.claim.id())
            .collect::<BTreeSet<_>>();
        let field_evidence = evidence_ids.iter().copied().collect::<BTreeSet<_>>();
        if claim_ids.len() != values.len()
            || values.iter().any(|value| {
                value
                    .claim
                    .evidence_ids()
                    .iter()
                    .any(|id| !field_evidence.contains(id))
            })
        {
            return Err(ModuleCardDetailFieldError);
        }
        Ok(Self {
            field,
            values,
            evidence_ids,
        })
    }

    /// Returns the canonical schema field.
    #[must_use]
    pub const fn field(&self) -> ModuleCardField {
        self.field
    }

    /// Returns values in their durable value-index order.
    #[must_use]
    pub fn values(&self) -> &[ModuleCardValuePresentation] {
        &self.values
    }

    /// Returns canonical field-level evidence identities.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }
}

/// A field contradicted the accepted V1 Module Card or claim-evidence contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardDetailFieldError;

impl fmt::Display for ModuleCardDetailFieldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module card detail field is invalid")
    }
}

impl Error for ModuleCardDetailFieldError {}

/// Latest durable Module Card bound to one latest atomic index publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardDetail {
    current_index_run_id: IndexRunId,
    current_snapshot_id: SnapshotId,
    source_index_run_id: IndexRunId,
    source_snapshot_id: SnapshotId,
    id: ModuleCardId,
    module_id: ModuleId,
    schema_version: ModuleCardSchemaVersion,
    mapper_profile_version: MapperProfileVersion,
    confidence: Confidence,
    lifecycle: ModuleCardLifecycle,
    fields: Vec<ModuleCardDetailField>,
}

impl ModuleCardDetail {
    /// Validates ordering, V1 compatibility, total bytes, evidence union, and state propagation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current_index_run_id: IndexRunId,
        current_snapshot_id: SnapshotId,
        source_index_run_id: IndexRunId,
        source_snapshot_id: SnapshotId,
        id: ModuleCardId,
        module_id: ModuleId,
        schema_version: ModuleCardSchemaVersion,
        mapper_profile_version: MapperProfileVersion,
        confidence: Confidence,
        lifecycle: ModuleCardLifecycle,
        fields: Vec<ModuleCardDetailField>,
    ) -> Result<Self, ModuleCardDetailFieldError> {
        let expected_state = match lifecycle {
            ModuleCardLifecycle::Current => ModuleCardClaimState::Current,
            ModuleCardLifecycle::Stale { .. } => ModuleCardClaimState::Stale,
            ModuleCardLifecycle::NeedsReview { .. } => ModuleCardClaimState::NeedsReview,
        };
        let lifecycle_is_valid = match lifecycle {
            ModuleCardLifecycle::Current => true,
            ModuleCardLifecycle::Stale { reason, .. } => matches!(
                reason,
                InvalidationReason::EvidenceChanged
                    | InvalidationReason::ModuleRemoved
                    | InvalidationReason::ParserVersionChanged
                    | InvalidationReason::MapperVersionChanged
            ),
            ModuleCardLifecycle::NeedsReview { reason, .. } => {
                reason == InvalidationReason::DirectDependencyChanged
            }
        };
        let total_value_bytes = fields
            .iter()
            .flat_map(ModuleCardDetailField::values)
            .map(|value| value.value().len())
            .try_fold(0_usize, usize::checked_add)
            .ok_or(ModuleCardDetailFieldError)?;
        let evidence = fields
            .iter()
            .flat_map(ModuleCardDetailField::evidence_ids)
            .copied()
            .collect::<BTreeSet<_>>();
        if !lifecycle_is_valid
            || schema_version != ModuleCardSchemaVersion::V1
            || mapper_profile_version != MapperProfileVersion::V1
            || fields.is_empty()
            || fields
                .windows(2)
                .any(|pair| pair[0].field() >= pair[1].field())
            || total_value_bytes > 65_536
            || evidence.len() > 512
            || fields
                .iter()
                .flat_map(ModuleCardDetailField::values)
                .any(|value| value.claim().state() != expected_state)
        {
            return Err(ModuleCardDetailFieldError);
        }
        Ok(Self {
            current_index_run_id,
            current_snapshot_id,
            source_index_run_id,
            source_snapshot_id,
            id,
            module_id,
            schema_version,
            mapper_profile_version,
            confidence,
            lifecycle,
            fields,
        })
    }

    /// Returns the latest published run against which this detail was selected.
    #[must_use]
    pub const fn current_index_run_id(&self) -> IndexRunId {
        self.current_index_run_id
    }

    /// Returns the latest immutable snapshot against which freshness was evaluated.
    #[must_use]
    pub const fn current_snapshot_id(&self) -> SnapshotId {
        self.current_snapshot_id
    }

    /// Returns the historical run that verified and published the card body.
    #[must_use]
    pub const fn source_index_run_id(&self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the immutable source snapshot carrying the referenced evidence.
    #[must_use]
    pub const fn source_snapshot_id(&self) -> SnapshotId {
        self.source_snapshot_id
    }

    /// Returns the stable logical card identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardId {
        self.id
    }

    /// Returns the selected current primary module identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the durable Module Card document schema interpreted by this read model.
    #[must_use]
    pub const fn schema_version(&self) -> ModuleCardSchemaVersion {
        self.schema_version
    }

    /// Returns the deterministic mapper profile that produced the Card.
    #[must_use]
    pub const fn mapper_profile_version(&self) -> MapperProfileVersion {
        self.mapper_profile_version
    }

    /// Returns card confidence separately from classification and freshness.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns current, stale, or needs-review state with invalidation provenance.
    #[must_use]
    pub const fn lifecycle(&self) -> ModuleCardLifecycle {
        self.lifecycle
    }

    /// Returns present V1 fields in canonical schema order.
    #[must_use]
    pub fn fields(&self) -> &[ModuleCardDetailField] {
        &self.fields
    }
}

/// Result of one atomic latest-publication and latest-card read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCardDetailLoadResult {
    /// No index crossed the durable publication boundary.
    NoPublishedIndex,
    /// The latest historical publication predates deterministic module projection.
    ProjectionUnavailable,
    /// The selected ID is absent or names a supplementary graph community.
    ModuleUnavailable,
    /// The current module has no durable verified Card yet.
    CardUnavailable,
    /// The latest deterministic Card and effective freshness labels are available.
    Detail(Box<ModuleCardDetail>),
}

/// Cooperative cancellation and deterministic progress for Module Card reads.
pub trait ModuleCardDetailControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports bounded start, read, and completion phases.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardDetailControlError>;
}

impl ModuleCardDetailControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardDetailControlError> {
        JobContext::report_progress(self, progress)
            .map_err(|_| ModuleCardDetailControlError::Unavailable)
    }
}

/// Module Card progress could not reach its owning runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardDetailControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleCardDetailControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module card detail progress is unavailable")
    }
}

impl Error for ModuleCardDetailControlError {}

/// Owned future returned by the object-safe Module Card detail port.
pub type ModuleCardDetailFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ModuleCardDetailLoadResult, ModuleCardDetailFailure>>
            + Send
            + 'a,
    >,
>;

/// Narrow read-only capability for one selected module's latest durable card.
pub trait ModuleCardDetailStore: fmt::Debug + Send + Sync {
    /// Loads the current publication, selected current module, and latest card atomically.
    fn load_module_card_detail<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleCardDetailQuery,
        control: &'a dyn ModuleCardDetailControl,
    ) -> ModuleCardDetailFuture<'a>;
}

/// Stable content-free failure classes for Module Card reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardDetailFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the Module Card detail contract.
    InvalidStoredProjection,
    /// The owner cancelled before a complete result was delivered.
    Cancelled,
    /// The bounded local read exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleCardDetailFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "module card detail storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module card detail projection is invalid")
            }
            Self::Cancelled => formatter.write_str("module card detail read was cancelled"),
            Self::TimedOut => formatter.write_str("module card detail read timed out"),
            Self::ProgressUnavailable => {
                formatter.write_str("module card detail progress is unavailable")
            }
        }
    }
}

impl Error for ModuleCardDetailFailure {
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

/// Application use case retaining cancellation and progress outside persistence.
#[derive(Debug)]
pub struct GetModuleCardDetail {
    store: Arc<dyn ModuleCardDetailStore>,
}

impl GetModuleCardDetail {
    /// Wires the narrow explicit Module Card detail capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleCardDetailStore>) -> Self {
        Self { store }
    }

    /// Reads one current selection or an explicit availability state.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleCardDetailQuery,
        control: &dyn ModuleCardDetailControl,
    ) -> Result<ModuleCardDetailLoadResult, ModuleCardDetailFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleCardDetailFailure::Cancelled);
        }
        let result = self
            .store
            .load_module_card_detail(project, query, control)
            .await?;
        if let ModuleCardDetailLoadResult::Detail(detail) = &result
            && detail.module_id() != query.module_id()
        {
            return Err(ModuleCardDetailFailure::InvalidStoredProjection);
        }
        if control.is_cancelled() {
            return Err(ModuleCardDetailFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

fn report(
    control: &dyn ModuleCardDetailControl,
    completed: u64,
) -> Result<(), ModuleCardDetailFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleCardDetailFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        CanonicalDirectory, GitHead, GitReferenceName, RepositoryId, RepositoryIdentity,
        WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct RecordingStore;

    impl ModuleCardDetailStore for RecordingStore {
        fn load_module_card_detail<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a ModuleCardDetailQuery,
            _control: &'a dyn ModuleCardDetailControl,
        ) -> ModuleCardDetailFuture<'a> {
            Box::pin(async move {
                Ok(ModuleCardDetailLoadResult::Detail(Box::new(detail(
                    query.module_id(),
                )?)))
            })
        }
    }

    #[derive(Debug)]
    struct MismatchedStore;

    impl ModuleCardDetailStore for MismatchedStore {
        fn load_module_card_detail<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a ModuleCardDetailQuery,
            _control: &'a dyn ModuleCardDetailControl,
        ) -> ModuleCardDetailFuture<'a> {
            Box::pin(async move {
                Ok(ModuleCardDetailLoadResult::Detail(Box::new(detail(
                    ModuleId::from_bytes([9; 32]),
                )?)))
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        progress: Mutex<Vec<Progress>>,
        cancelled: bool,
    }

    impl ModuleCardDetailControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(&self, progress: Progress) -> Result<(), ModuleCardDetailControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleCardDetailControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn field_rejects_unclassified_values_and_evidence_outside_field() -> Result<(), Box<dyn Error>>
    {
        let evidence = ModuleCardEvidenceId::from_bytes([1; 32]);
        let other = ModuleCardEvidenceId::from_bytes([2; 32]);
        let claim = ModuleCardClaimPresentation::new(
            ModuleCardClaimId::from_bytes([3; 32]),
            VerifiedClaimKind::Fact,
            Confidence::from_basis_points(8_000)?,
            ModuleCardClaimState::Current,
            vec![other],
        )?;
        assert!(
            ModuleCardDetailField::new(
                ModuleCardField::Title,
                vec![ModuleCardValuePresentation::new("A".to_owned(), claim)],
                vec![evidence],
            )
            .is_err()
        );
        assert!(
            ModuleCardDetailField::new(ModuleCardField::Title, Vec::new(), vec![other, evidence],)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn stale_card_rejects_claim_that_could_look_current() -> Result<(), Box<dyn Error>> {
        let mut current_detail = detail(ModuleId::from_bytes([4; 32]))?;
        current_detail.lifecycle = ModuleCardLifecycle::Stale {
            invalidated_by_index_run_id: IndexRunId::from_bytes([7; 32]),
            reason: InvalidationReason::EvidenceChanged,
        };
        let field = current_detail.fields.remove(0);
        assert!(
            ModuleCardDetail::new(
                current_detail.current_index_run_id,
                current_detail.current_snapshot_id,
                current_detail.source_index_run_id,
                current_detail.source_snapshot_id,
                current_detail.id,
                current_detail.module_id,
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                current_detail.confidence,
                current_detail.lifecycle,
                vec![field],
            )
            .is_err()
        );

        let invalid_lifecycle = ModuleCardLifecycle::NeedsReview {
            invalidated_by_index_run_id: IndexRunId::from_bytes([7; 32]),
            reason: InvalidationReason::EvidenceChanged,
        };
        let mut invalid_detail = detail(ModuleId::from_bytes([4; 32]))?;
        invalid_detail.fields[0].values[0].claim.state = ModuleCardClaimState::NeedsReview;
        assert!(
            ModuleCardDetail::new(
                invalid_detail.current_index_run_id,
                invalid_detail.current_snapshot_id,
                invalid_detail.source_index_run_id,
                invalid_detail.source_snapshot_id,
                invalid_detail.id,
                invalid_detail.module_id,
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                invalid_detail.confidence,
                invalid_lifecycle,
                invalid_detail.fields.drain(..).collect(),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn use_case_reports_bounded_progress_and_honors_cancellation() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let query = ModuleCardDetailQuery::new(ModuleId::from_bytes([4; 32]));
        let control = RecordingControl::default();
        assert!(matches!(
            block_on(
                GetModuleCardDetail::new(Arc::new(RecordingStore))
                    .execute(&project, &query, &control)
            )?,
            ModuleCardDetailLoadResult::Detail(_)
        ));
        let progress = control
            .progress
            .lock()
            .map_err(|_| std::io::Error::other("progress lock poisoned"))?;
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].completed(), Some(0));
        assert_eq!(progress[1].completed(), Some(2));
        drop(progress);

        let cancelled = RecordingControl {
            cancelled: true,
            ..RecordingControl::default()
        };
        assert_eq!(
            block_on(
                GetModuleCardDetail::new(Arc::new(RecordingStore))
                    .execute(&project, &query, &cancelled,)
            ),
            Err(ModuleCardDetailFailure::Cancelled)
        );
        assert_eq!(
            block_on(GetModuleCardDetail::new(Arc::new(MismatchedStore)).execute(
                &project,
                &query,
                &RecordingControl::default(),
            )),
            Err(ModuleCardDetailFailure::InvalidStoredProjection)
        );
        Ok(())
    }

    fn detail(module_id: ModuleId) -> Result<ModuleCardDetail, ModuleCardDetailFailure> {
        let evidence = ModuleCardEvidenceId::from_bytes([1; 32]);
        let claim = ModuleCardClaimPresentation::new(
            ModuleCardClaimId::from_bytes([2; 32]),
            VerifiedClaimKind::Fact,
            Confidence::from_basis_points(8_000)
                .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)?,
            ModuleCardClaimState::Current,
            vec![evidence],
        )
        .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)?;
        let field = ModuleCardDetailField::new(
            ModuleCardField::Title,
            vec![ModuleCardValuePresentation::new("Module".to_owned(), claim)],
            vec![evidence],
        )
        .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)?;
        ModuleCardDetail::new(
            IndexRunId::from_bytes([3; 32]),
            SnapshotId::from_bytes([4; 32]),
            IndexRunId::from_bytes([3; 32]),
            SnapshotId::from_bytes([4; 32]),
            ModuleCardId::from_bytes([5; 32]),
            module_id,
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::from_basis_points(8_000)
                .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)?,
            ModuleCardLifecycle::Current,
            vec![field],
        )
        .map_err(|_| ModuleCardDetailFailure::InvalidStoredProjection)
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
