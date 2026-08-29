use crate::{
    DeepMapActivityObserver, DeepMapActivityUpdate, DeepMapExecutionFailure,
    DeepMapExecutionFuture, DeepMapExecutionOutcome, DeepMapExecutionRequest, DeepMapExecutor,
    DeepMapExplorerFailure, DeepMapExplorerStatus, DeepMapModelDescriptor, DeepMapPhase,
    DeepMapPublicationState, DeepMapPublicationStateFailure, DeepMapPublicationStateStore,
    DeepMapResumeState, DeepMapSafeAction, DeepMapTargetKind, ExploreDeepMap, ExplorerModelFailure,
    IgnoreDeepMapActivity, IndexPersistenceControl, IndexPersistenceControlError, JobContext,
    KnowledgeIndexFailure, KnowledgeIndexStore, ModelBackedExplorerProvider, ModelProvider,
    PlanDeepMap, ProposeModuleCardClaims, ProposeModuleCardClaimsFailure,
    PublishVerifiedModuleCards, PublishVerifiedModuleCardsFailure, PublishedIndexDeepMapReadTools,
    PublishedIndexEvidenceResolver, VerifiedModuleCardPublisher, VerifyModuleCards,
    VerifyModuleCardsFailure,
};
use a3_domain::{
    Confidence, ExplorerCheckpoint, MapperProfileVersion, ModelProfile, ModuleCardId,
    ModuleCardProposal, ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleId, Progress,
    ProjectIdentity, ProposedModuleCardField, PublishedIndex, SnapshotId,
};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

const MERGED_PROPOSAL_ENVELOPE_BYTES: usize = 512;
const MERGED_FIELD_OVERHEAD_BYTES: usize = 64;
const MERGED_EVIDENCE_ID_BYTES: usize = 67;
static IGNORE_ACTIVITY: IgnoreDeepMapActivity = IgnoreDeepMapActivity;

#[derive(Debug)]
struct DeepMapIndexReadControl<'a> {
    control: &'a dyn IndexPersistenceControl,
}

impl IndexPersistenceControl for DeepMapIndexReadControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        if self.control.is_cancelled() {
            Err(IndexPersistenceControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}

/// Complete production Deep-Map capability composed from verified model, index, reads and publish.
pub struct RunDeepMap {
    model: DeepMapModelDescriptor,
    profile: ModelProfile,
    provider: Arc<dyn ModelProvider>,
    index: Arc<dyn KnowledgeIndexStore>,
    publisher: Arc<dyn VerifiedModuleCardPublisher>,
    publication_state: Arc<dyn DeepMapPublicationStateStore>,
}

impl RunDeepMap {
    /// Creates an executor only for an exact provider/profile pair with live structured evidence.
    pub fn new(
        profile: ModelProfile,
        provider: Arc<dyn ModelProvider>,
        index: Arc<dyn KnowledgeIndexStore>,
        publisher: Arc<dyn VerifiedModuleCardPublisher>,
        publication_state: Arc<dyn DeepMapPublicationStateStore>,
    ) -> Result<Self, DeepMapExecutionFailure> {
        if provider.provider_id() != profile.provider_id() {
            return Err(DeepMapExecutionFailure::ModelRejected);
        }
        let model = DeepMapModelDescriptor::from_verified_profile(&profile)
            .map_err(|_| DeepMapExecutionFailure::ModelRejected)?;
        Ok(Self {
            model,
            profile,
            provider,
            index,
            publisher,
            publication_state,
        })
    }

    async fn load_published(
        &self,
        project: &ProjectIdentity,
        control: &JobContext,
    ) -> Result<PublishedIndex, DeepMapExecutionFailure> {
        let read_control = DeepMapIndexReadControl { control };
        self.index
            .latest_published_index(project, &read_control)
            .await
            .map_err(map_index_failure)?
            .ok_or(DeepMapExecutionFailure::NoPublishedIndex)
    }

    async fn execute_owned(
        &self,
        project: &ProjectIdentity,
        request: DeepMapExecutionRequest,
        control: &JobContext,
        observer: &dyn DeepMapActivityObserver,
    ) -> Result<DeepMapExecutionOutcome, DeepMapExecutionFailure> {
        if matches!(&request, DeepMapExecutionRequest::Start { .. }) {
            match self
                .publication_state
                .load_deep_map_publication_state(project)
                .await
                .map_err(map_publication_state_failure)?
            {
                DeepMapPublicationState::Current { anchor, .. } => {
                    return Ok(DeepMapExecutionOutcome::AlreadyCurrent(anchor));
                }
                DeepMapPublicationState::NoPublishedIndex => {
                    return Err(DeepMapExecutionFailure::NoPublishedIndex);
                }
                DeepMapPublicationState::Ready(_) => {}
            }
        }
        if matches!(&request, DeepMapExecutionRequest::Start { .. }) {
            observer.observe(DeepMapActivityUpdate::new(
                DeepMapPhase::Planning,
                None,
                DeepMapTargetKind::Project,
                DeepMapSafeAction::BuildPlan,
                None,
                None,
                false,
            ));
        }
        let published = self.load_published(project, control).await?;
        let budget = request.budget();
        let (plan, checkpoint) = match request {
            DeepMapExecutionRequest::Start { .. } => {
                let coverage = a3_domain::ModuleCoverageSnapshot::empty(
                    published.run().snapshot_id(),
                    ModuleCardSchemaVersion::V1,
                );
                let plan = PlanDeepMap::version_one()
                    .execute(&published, &coverage, budget)
                    .map_err(|_| DeepMapExecutionFailure::Planning)?;
                observer.observe_plan(&plan);
                let checkpoint = ExplorerCheckpoint::new(&plan);
                (plan, checkpoint)
            }
            DeepMapExecutionRequest::Resume(state) => {
                if state.plan().index_run_id() != published.run().id()
                    || state.plan().snapshot_id() != published.run().snapshot_id()
                {
                    return Err(DeepMapExecutionFailure::StaleSnapshot);
                }
                (state.plan().clone(), state.checkpoint().clone())
            }
        };

        if plan.steps().is_empty() {
            return DeepMapExecutionOutcome::completed(DeepMapResumeState::new(
                plan, checkpoint, budget,
            )?);
        }

        let provider =
            ModelBackedExplorerProvider::new(self.provider.as_ref(), self.profile.clone())
                .map_err(map_explorer_model_failure)?;
        let tools = PublishedIndexDeepMapReadTools::new(Arc::clone(&self.index));
        let explored = ExploreDeepMap::version_one(&provider, &tools)
            .execute_observed(project, &plan, checkpoint, control, observer)
            .await
            .map_err(map_explorer_failure)?;
        let status = explored.status();
        let state = DeepMapResumeState::new(plan, explored.into_checkpoint(), budget)?;
        if control.cancellation_token().is_cancelled() || status == DeepMapExplorerStatus::Cancelled
        {
            return Ok(DeepMapExecutionOutcome::cancelled(state));
        }

        let claim_proposer =
            ProposeModuleCardClaims::new(self.provider.as_ref(), self.profile.clone())
                .map_err(map_claim_failure)?;
        let proposals = merge_module_proposals(state.checkpoint().confirmed_proposals())?;
        let mut candidates = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            observer.observe(DeepMapActivityUpdate::new(
                DeepMapPhase::Claiming,
                Some(proposal.module_id()),
                DeepMapTargetKind::Module,
                DeepMapSafeAction::GenerateClaims,
                None,
                None,
                false,
            ));
            match claim_proposer.execute(proposal, control).await {
                Ok(candidate) => candidates.push(candidate),
                Err(ProposeModuleCardClaimsFailure::Cancelled) => {
                    return Ok(DeepMapExecutionOutcome::cancelled(state));
                }
                Err(failure) => return Err(map_claim_failure(failure)),
            }
        }
        let resolver = PublishedIndexEvidenceResolver::new(self.index.as_ref());
        observer.observe(DeepMapActivityUpdate::new(
            DeepMapPhase::Verifying,
            None,
            DeepMapTargetKind::Project,
            DeepMapSafeAction::VerifyEvidence,
            None,
            None,
            false,
        ));
        let verified = match VerifyModuleCards::version_one(&resolver)
            .execute(project, &published, candidates, control)
            .await
        {
            Ok(verified) => verified,
            Err(VerifyModuleCardsFailure::Cancelled) => {
                return Ok(DeepMapExecutionOutcome::cancelled(state));
            }
            Err(failure) => return Err(map_verification_failure(failure)),
        };
        observer.observe(DeepMapActivityUpdate::new(
            DeepMapPhase::Publishing,
            None,
            DeepMapTargetKind::Project,
            DeepMapSafeAction::PublishCards,
            None,
            None,
            false,
        ));
        match PublishVerifiedModuleCards::new(self.publisher.as_ref())
            .execute(project, &verified, control)
            .await
        {
            Ok(_) => {}
            Err(PublishVerifiedModuleCardsFailure::Cancelled) => {
                return Ok(DeepMapExecutionOutcome::cancelled(state));
            }
            Err(PublishVerifiedModuleCardsFailure::Publisher(
                crate::VerifiedModuleCardPublisherFailure::AlreadyPublished,
            )) => {
                let expected = crate::DeepMapPublicationAnchor::new(
                    published.run().id(),
                    published.run().snapshot_id(),
                );
                let anchor =
                    resolve_publication_race(self.publication_state.as_ref(), project, expected)
                        .await?;
                observer.observe(DeepMapActivityUpdate::new(
                    DeepMapPhase::Publishing,
                    None,
                    DeepMapTargetKind::Project,
                    DeepMapSafeAction::PublishCards,
                    None,
                    None,
                    true,
                ));
                return Ok(DeepMapExecutionOutcome::AlreadyCurrent(anchor));
            }
            Err(failure) => return Err(map_publication_failure(failure)),
        }
        observer.observe(DeepMapActivityUpdate::new(
            DeepMapPhase::Publishing,
            None,
            DeepMapTargetKind::Project,
            DeepMapSafeAction::PublishCards,
            None,
            None,
            true,
        ));
        DeepMapExecutionOutcome::completed(state)
    }
}

async fn resolve_publication_race(
    publication_state: &dyn DeepMapPublicationStateStore,
    project: &ProjectIdentity,
    expected: crate::DeepMapPublicationAnchor,
) -> Result<crate::DeepMapPublicationAnchor, DeepMapExecutionFailure> {
    match publication_state
        .load_deep_map_publication_state(project)
        .await
        .map_err(map_publication_state_failure)?
    {
        DeepMapPublicationState::Current { anchor, .. } if anchor == expected => Ok(anchor),
        DeepMapPublicationState::Current { .. } => Err(DeepMapExecutionFailure::StaleSnapshot),
        DeepMapPublicationState::NoPublishedIndex | DeepMapPublicationState::Ready(_) => {
            Err(DeepMapExecutionFailure::PublicationRejected)
        }
    }
}

#[derive(Debug)]
struct ModuleProposalAccumulator {
    snapshot_id: SnapshotId,
    schema_version: ModuleCardSchemaVersion,
    mapper_profile_version: MapperProfileVersion,
    confidence: Confidence,
    fields: Vec<ProposedModuleCardField>,
}

fn merge_module_proposals(
    fragments: &[ModuleCardProposal],
) -> Result<Vec<ModuleCardProposal>, DeepMapExecutionFailure> {
    let mut modules = BTreeMap::<ModuleId, ModuleProposalAccumulator>::new();
    for fragment in fragments {
        let accumulator =
            modules
                .entry(fragment.module_id())
                .or_insert_with(|| ModuleProposalAccumulator {
                    snapshot_id: fragment.snapshot_id(),
                    schema_version: fragment.schema_version(),
                    mapper_profile_version: fragment.mapper_profile_version(),
                    confidence: fragment.confidence(),
                    fields: Vec::new(),
                });
        if accumulator.snapshot_id != fragment.snapshot_id()
            || accumulator.schema_version != fragment.schema_version()
            || accumulator.mapper_profile_version != fragment.mapper_profile_version()
        {
            return Err(DeepMapExecutionFailure::InvalidModelResponse);
        }
        accumulator.confidence = accumulator.confidence.min(fragment.confidence());
        accumulator.fields.extend(fragment.fields().iter().cloned());
    }

    modules
        .into_iter()
        .map(|(module_id, accumulator)| {
            let encoded_bytes = merged_proposal_size(&accumulator.fields)
                .ok_or(DeepMapExecutionFailure::InvalidModelResponse)?;
            ModuleCardProposal::new(
                ModuleCardProposalEnvelope::new(
                    ModuleCardId::for_module_v1(module_id),
                    module_id,
                    accumulator.snapshot_id,
                    accumulator.schema_version,
                    accumulator.mapper_profile_version,
                    accumulator.confidence,
                ),
                accumulator.fields,
                encoded_bytes,
            )
            .map_err(|_| DeepMapExecutionFailure::InvalidModelResponse)
        })
        .collect()
}

fn merged_proposal_size(fields: &[ProposedModuleCardField]) -> Option<usize> {
    let mut bytes = MERGED_PROPOSAL_ENVELOPE_BYTES;
    for field in fields {
        bytes = bytes.checked_add(MERGED_FIELD_OVERHEAD_BYTES)?;
        for value in field.values() {
            bytes = bytes.checked_add(value.len().checked_mul(2)?.checked_add(3)?)?;
        }
        bytes = bytes.checked_add(
            field
                .evidence_ids()
                .len()
                .checked_mul(MERGED_EVIDENCE_ID_BYTES)?,
        )?;
    }
    Some(bytes)
}

impl fmt::Debug for RunDeepMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunDeepMap")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl DeepMapExecutor for RunDeepMap {
    fn model(&self) -> &DeepMapModelDescriptor {
        &self.model
    }

    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: DeepMapExecutionRequest,
        control: &'a JobContext,
    ) -> DeepMapExecutionFuture<'a> {
        Box::pin(self.execute_owned(project, request, control, &IGNORE_ACTIVITY))
    }

    fn execute_observed<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: DeepMapExecutionRequest,
        control: &'a JobContext,
        observer: &'a dyn DeepMapActivityObserver,
    ) -> DeepMapExecutionFuture<'a> {
        Box::pin(self.execute_owned(project, request, control, observer))
    }
}

const fn map_index_failure(failure: KnowledgeIndexFailure) -> DeepMapExecutionFailure {
    match failure {
        KnowledgeIndexFailure::Cancelled => DeepMapExecutionFailure::Planning,
        KnowledgeIndexFailure::TimedOut => DeepMapExecutionFailure::Planning,
        KnowledgeIndexFailure::Storage(_)
        | KnowledgeIndexFailure::SnapshotConflict
        | KnowledgeIndexFailure::SnapshotNotFound
        | KnowledgeIndexFailure::IndexRunAlreadyActive
        | KnowledgeIndexFailure::IndexRunNotFound
        | KnowledgeIndexFailure::InvalidIndexRunTransition
        | KnowledgeIndexFailure::IndexPublicationMismatch
        | KnowledgeIndexFailure::IndexPublicationTooLarge
        | KnowledgeIndexFailure::ProgressUnavailable => DeepMapExecutionFailure::NoPublishedIndex,
    }
}

const fn map_explorer_failure(failure: DeepMapExplorerFailure) -> DeepMapExecutionFailure {
    match failure {
        DeepMapExplorerFailure::Checkpoint(_) => DeepMapExecutionFailure::InvalidCheckpoint,
        DeepMapExplorerFailure::Model(failure) => map_explorer_model_failure(failure),
        DeepMapExplorerFailure::InvalidModelOutput => DeepMapExecutionFailure::InvalidModelResponse,
        DeepMapExplorerFailure::Read(_) => DeepMapExecutionFailure::Read,
    }
}

const fn map_explorer_model_failure(failure: ExplorerModelFailure) -> DeepMapExecutionFailure {
    match failure {
        ExplorerModelFailure::Unavailable | ExplorerModelFailure::Cancelled => {
            DeepMapExecutionFailure::ModelUnavailable
        }
        ExplorerModelFailure::Rejected => DeepMapExecutionFailure::ModelRejected,
        ExplorerModelFailure::InvalidResponse => DeepMapExecutionFailure::InvalidModelResponse,
        ExplorerModelFailure::TimedOut => DeepMapExecutionFailure::ModelTimedOut,
    }
}

fn map_verification_failure(failure: VerifyModuleCardsFailure) -> DeepMapExecutionFailure {
    match failure {
        VerifyModuleCardsFailure::Resolver(_) | VerifyModuleCardsFailure::Verification(_) => {
            DeepMapExecutionFailure::Verification
        }
        VerifyModuleCardsFailure::Cancelled => DeepMapExecutionFailure::Verification,
    }
}

fn map_claim_failure(failure: ProposeModuleCardClaimsFailure) -> DeepMapExecutionFailure {
    match failure {
        ProposeModuleCardClaimsFailure::Model(failure) => map_model_provider_failure(failure),
        ProposeModuleCardClaimsFailure::Cancelled => DeepMapExecutionFailure::ModelUnavailable,
        ProposeModuleCardClaimsFailure::InvalidRequest
        | ProposeModuleCardClaimsFailure::InvalidOutput(_) => {
            DeepMapExecutionFailure::InvalidModelResponse
        }
    }
}

const fn map_model_provider_failure(
    failure: crate::ModelProviderFailure,
) -> DeepMapExecutionFailure {
    match failure {
        crate::ModelProviderFailure::Unavailable
        | crate::ModelProviderFailure::EndpointDenied
        | crate::ModelProviderFailure::Cancelled => DeepMapExecutionFailure::ModelUnavailable,
        crate::ModelProviderFailure::Rejected => DeepMapExecutionFailure::ModelRejected,
        crate::ModelProviderFailure::InvalidResponse => {
            DeepMapExecutionFailure::InvalidModelResponse
        }
        crate::ModelProviderFailure::TimedOut => DeepMapExecutionFailure::ModelTimedOut,
    }
}

const fn map_publication_failure(
    failure: PublishVerifiedModuleCardsFailure,
) -> DeepMapExecutionFailure {
    match failure {
        PublishVerifiedModuleCardsFailure::Publisher(failure) => match failure {
            crate::VerifiedModuleCardPublisherFailure::AlreadyPublished
            | crate::VerifiedModuleCardPublisherFailure::Rejected => {
                DeepMapExecutionFailure::PublicationRejected
            }
            crate::VerifiedModuleCardPublisherFailure::Storage => {
                DeepMapExecutionFailure::PublicationStorage
            }
            crate::VerifiedModuleCardPublisherFailure::TimedOut => {
                DeepMapExecutionFailure::PublicationTimedOut
            }
            crate::VerifiedModuleCardPublisherFailure::ProgressUnavailable => {
                DeepMapExecutionFailure::PublicationProgressUnavailable
            }
            crate::VerifiedModuleCardPublisherFailure::Cancelled => {
                DeepMapExecutionFailure::PublicationRejected
            }
        },
        PublishVerifiedModuleCardsFailure::Cancelled => {
            DeepMapExecutionFailure::PublicationRejected
        }
    }
}

const fn map_publication_state_failure(
    failure: DeepMapPublicationStateFailure,
) -> DeepMapExecutionFailure {
    match failure {
        DeepMapPublicationStateFailure::Storage
        | DeepMapPublicationStateFailure::InvalidStoredData => {
            DeepMapExecutionFailure::PublicationStorage
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        JobClock, JobCompletion, JobScheduler, JobSchedulerConfig, JobTimestamp,
        KnowledgeIndexFuture, ModelOperationControl, ModelProviderFuture, ModelProviderRequest,
        ModelRequestTimeout, ModuleCardPublicationTimeout, ModuleCardVerificationControl,
        ShutdownMode, VerifiedModuleCardPublisherFuture,
    };
    use a3_domain::{
        GitHead, GitReferenceName, IndexPublication, IndexRunId, IndexRunRecord, IndexRunStart,
        IndexRunTerminalOutcome, JobId, JobOwner, ModelCapabilities, ModelContextLimit, ModelId,
        ModelOutputLimit, ModelParallelismLimit, ModelProfileSettings, ModelPromptSchemaGrounding,
        ModelProviderId, ModelSamplingProfile, ModelStopSequences, ModelStructuredOutputCapability,
        ModelTemperature, ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP,
        ModuleCardEvidenceId, ModuleCardField, RepositoryFileState, RepositoryId,
        RepositoryIdentity, Snapshot, VerifiedModuleCardBatch, WorktreeAnchorId, WorktreeId,
        WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    #[derive(Debug, Default)]
    struct FixedClock;

    impl JobClock for FixedClock {
        fn now(&self) -> JobTimestamp {
            JobTimestamp::from_millis(1)
        }
    }

    #[derive(Debug)]
    struct CountingPublicationState {
        state: DeepMapPublicationState,
        calls: AtomicUsize,
    }

    impl CountingPublicationState {
        const fn new(state: DeepMapPublicationState) -> Self {
            Self {
                state,
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl DeepMapPublicationStateStore for CountingPublicationState {
        fn load_deep_map_publication_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> crate::DeepMapPublicationStateFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(self.state) })
        }
    }

    #[derive(Debug)]
    struct RejectingProvider {
        provider_id: ModelProviderId,
        calls: AtomicUsize,
    }

    impl RejectingProvider {
        fn new(provider_id: ModelProviderId) -> Self {
            Self {
                provider_id,
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl ModelProvider for RejectingProvider {
        fn provider_id(&self) -> &ModelProviderId {
            &self.provider_id
        }

        fn stream<'a>(
            &'a self,
            _request: &'a ModelProviderRequest,
            _timeout: ModelRequestTimeout,
            _control: &'a dyn ModelOperationControl,
        ) -> ModelProviderFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(crate::ModelProviderFailure::Rejected) })
        }
    }

    #[derive(Debug, Default)]
    struct RejectingIndex {
        calls: AtomicUsize,
    }

    impl RejectingIndex {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn unexpected<'a, T>(&'a self) -> KnowledgeIndexFuture<'a, T> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
        }
    }

    impl KnowledgeIndexStore for RejectingIndex {
        fn append_snapshot<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _snapshot: &'a Snapshot,
        ) -> KnowledgeIndexFuture<'a, ()> {
            self.unexpected()
        }

        fn latest_snapshot<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
            self.unexpected()
        }

        fn current_file_state<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, RepositoryFileState> {
            self.unexpected()
        }

        fn start_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _request: IndexRunStart,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            self.unexpected()
        }

        fn finish_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: IndexRunId,
            _outcome: IndexRunTerminalOutcome,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            self.unexpected()
        }

        fn publish_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: IndexRunId,
            _publication: &'a IndexPublication,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
            self.unexpected()
        }

        fn latest_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
            self.unexpected()
        }

        fn latest_published_index_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
        ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
            self.unexpected()
        }

        fn latest_published_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>> {
            self.unexpected()
        }

        fn rebuild_regenerable_index<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _control: &'a dyn IndexPersistenceControl,
        ) -> KnowledgeIndexFuture<'a, ()> {
            self.unexpected()
        }
    }

    #[derive(Debug, Default)]
    struct RejectingPublisher {
        calls: AtomicUsize,
    }

    impl RejectingPublisher {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl VerifiedModuleCardPublisher for RejectingPublisher {
        fn publish<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _batch: &'a VerifiedModuleCardBatch,
            _timeout: ModuleCardPublicationTimeout,
            _control: &'a dyn ModuleCardVerificationControl,
        ) -> VerifiedModuleCardPublisherFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(crate::VerifiedModuleCardPublisherFailure::Rejected) })
        }
    }

    #[derive(Debug)]
    struct RejectingIndexProgressControl;

    impl IndexPersistenceControl for RejectingIndexProgressControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
            Err(IndexPersistenceControlError::Unavailable)
        }
    }

    #[test]
    fn initial_index_read_cannot_reset_the_owning_deep_map_job_progress()
    -> Result<(), Box<dyn Error>> {
        let control = DeepMapIndexReadControl {
            control: &RejectingIndexProgressControl,
        };
        control.report_progress(Progress::determinate(1, 1)?)?;
        Ok(())
    }

    #[test]
    fn current_index_preflight_uses_no_index_model_or_publisher_work() -> Result<(), Box<dyn Error>>
    {
        let anchor = crate::DeepMapPublicationAnchor::new(
            IndexRunId::from_bytes([41; 32]),
            SnapshotId::from_bytes([42; 32]),
        );
        let state = Arc::new(CountingPublicationState::new(
            DeepMapPublicationState::Current {
                anchor,
                card_count: 2,
            },
        ));
        let provider_id = ModelProviderId::try_from_string("test-provider".to_owned())?;
        let provider = Arc::new(RejectingProvider::new(provider_id.clone()));
        let index = Arc::new(RejectingIndex::default());
        let publisher = Arc::new(RejectingPublisher::default());
        let executor = Arc::new(RunDeepMap::new(
            verified_profile(provider_id)?,
            provider.clone(),
            index.clone(),
            publisher.clone(),
            state.clone(),
        )?);
        let project = project()?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let (scheduler, _events) =
            JobScheduler::new(JobSchedulerConfig::new(1, 1, 8)?, Arc::new(FixedClock))?;
        scheduler.submit(
            JobId::new(1),
            JobOwner::new(1),
            move |context: JobContext| {
                let result = block_on(executor.execute(
                    &project,
                    DeepMapExecutionRequest::Start {
                        budget: a3_domain::ExploreBudget::DEFAULT,
                    },
                    &context,
                ));
                let completion = if result.is_ok() {
                    JobCompletion::Succeeded
                } else {
                    JobCompletion::Failed
                };
                let _sent = result_sender.send(result);
                completion
            },
        )?;
        let outcome = result_receiver.recv_timeout(Duration::from_secs(2))??;
        let _report = scheduler.shutdown(ShutdownMode::Drain)?;

        assert_eq!(outcome, DeepMapExecutionOutcome::AlreadyCurrent(anchor));
        assert_eq!(state.calls(), 1);
        assert_eq!(index.calls(), 0);
        assert_eq!(provider.calls(), 0);
        assert_eq!(publisher.calls(), 0);
        Ok(())
    }

    #[test]
    fn publication_race_is_success_only_for_the_same_index_anchor() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let expected = crate::DeepMapPublicationAnchor::new(
            IndexRunId::from_bytes([51; 32]),
            SnapshotId::from_bytes([52; 32]),
        );
        let current = CountingPublicationState::new(DeepMapPublicationState::Current {
            anchor: expected,
            card_count: 1,
        });
        assert_eq!(
            block_on(resolve_publication_race(&current, &project, expected))?,
            expected
        );

        let stale = CountingPublicationState::new(DeepMapPublicationState::Current {
            anchor: crate::DeepMapPublicationAnchor::new(
                IndexRunId::from_bytes([53; 32]),
                SnapshotId::from_bytes([54; 32]),
            ),
            card_count: 1,
        });
        assert_eq!(
            block_on(resolve_publication_race(&stale, &project, expected)),
            Err(DeepMapExecutionFailure::StaleSnapshot)
        );
        Ok(())
    }

    #[test]
    fn exploration_fragments_are_merged_into_one_core_owned_card_per_module()
    -> Result<(), Box<dyn Error>> {
        let module = ModuleId::from_bytes([1; 32]);
        let other_module = ModuleId::from_bytes([2; 32]);
        let snapshot = SnapshotId::from_bytes([3; 32]);
        let title = fragment(
            module,
            snapshot,
            ModuleCardField::Title,
            ModuleCardEvidenceId::from_bytes([4; 32]),
            9_000,
        )?;
        let purpose = fragment(
            module,
            snapshot,
            ModuleCardField::Purpose,
            ModuleCardEvidenceId::from_bytes([5; 32]),
            7_000,
        )?;
        let other = fragment(
            other_module,
            snapshot,
            ModuleCardField::Title,
            ModuleCardEvidenceId::from_bytes([6; 32]),
            8_000,
        )?;

        let merged = merge_module_proposals(&[title, purpose, other])?;
        assert_eq!(merged.len(), 2);
        let card = merged
            .iter()
            .find(|proposal| proposal.module_id() == module)
            .ok_or("merged module card is missing")?;
        assert_eq!(card.id(), ModuleCardId::for_module_v1(module));
        assert_eq!(card.confidence().basis_points(), 7_000);
        assert_eq!(
            card.fields()
                .iter()
                .map(ProposedModuleCardField::field)
                .collect::<Vec<_>>(),
            vec![ModuleCardField::Title, ModuleCardField::Purpose]
        );
        Ok(())
    }

    fn fragment(
        module_id: ModuleId,
        snapshot_id: SnapshotId,
        field: ModuleCardField,
        evidence_id: ModuleCardEvidenceId,
        confidence: u16,
    ) -> Result<ModuleCardProposal, Box<dyn Error>> {
        Ok(ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                ModuleCardId::for_module_fields_v1(module_id, &[field]),
                module_id,
                snapshot_id,
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::from_basis_points(confidence)?,
            ),
            vec![ProposedModuleCardField::new(
                field,
                vec![format!("{field:?} value")],
                vec![evidence_id],
            )?],
            512,
        )?)
    }

    fn verified_profile(provider_id: ModelProviderId) -> Result<ModelProfile, Box<dyn Error>> {
        Ok(ModelProfile::from_probe(
            provider_id,
            ModelId::try_from_string("test-model".to_owned())?,
            ModelProfileSettings::new(
                ModelContextLimit::new(16_384)?,
                ModelOutputLimit::new(4_096)?,
                ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                ModelParallelismLimit::new(1)?,
                ModelSamplingProfile::new(
                    ModelTemperature::from_milli(0)?,
                    ModelTopP::from_milli(1_000)?,
                ),
                ModelStopSequences::empty(),
                ModelPromptSchemaGrounding::FormatFieldOnly,
            )?,
            ModelCapabilities::new(
                ModelStructuredOutputCapability::Verified,
                ModelToolCallMode::Disabled,
            ),
        ))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([61; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([62; 32]),
                WorktreeAnchorId::from_bytes([63; 32]),
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
