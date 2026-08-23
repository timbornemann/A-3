use crate::{
    DeepMapExecutionFailure, DeepMapExecutionFuture, DeepMapExecutionOutcome,
    DeepMapExecutionRequest, DeepMapExecutor, DeepMapExplorerFailure, DeepMapExplorerStatus,
    DeepMapModelDescriptor, DeepMapResumeState, ExploreDeepMap, ExplorerModelFailure,
    IndexPersistenceControl, IndexPersistenceControlError, JobContext, KnowledgeIndexFailure,
    KnowledgeIndexStore, ModelBackedExplorerProvider, ModelProvider, PlanDeepMap,
    ProposeModuleCardClaims, ProposeModuleCardClaimsFailure, PublishVerifiedModuleCards,
    PublishVerifiedModuleCardsFailure, PublishedIndexDeepMapReadTools,
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
}

impl RunDeepMap {
    /// Creates an executor only for an exact provider/profile pair with live structured evidence.
    pub fn new(
        profile: ModelProfile,
        provider: Arc<dyn ModelProvider>,
        index: Arc<dyn KnowledgeIndexStore>,
        publisher: Arc<dyn VerifiedModuleCardPublisher>,
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
    ) -> Result<DeepMapExecutionOutcome, DeepMapExecutionFailure> {
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
            .execute(project, &plan, checkpoint, control)
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
            match claim_proposer.execute(proposal, control).await {
                Ok(candidate) => candidates.push(candidate),
                Err(ProposeModuleCardClaimsFailure::Cancelled) => {
                    return Ok(DeepMapExecutionOutcome::cancelled(state));
                }
                Err(failure) => return Err(map_claim_failure(failure)),
            }
        }
        let resolver = PublishedIndexEvidenceResolver::new(self.index.as_ref());
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
        match PublishVerifiedModuleCards::new(self.publisher.as_ref())
            .execute(project, &verified, control)
            .await
        {
            Ok(_) => {}
            Err(PublishVerifiedModuleCardsFailure::Cancelled) => {
                return Ok(DeepMapExecutionOutcome::cancelled(state));
            }
            Err(failure) => return Err(map_publication_failure(failure)),
        }
        DeepMapExecutionOutcome::completed(state)
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
        Box::pin(self.execute_owned(project, request, control))
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
        PublishVerifiedModuleCardsFailure::Publisher(_) => DeepMapExecutionFailure::Publication,
        PublishVerifiedModuleCardsFailure::Cancelled => DeepMapExecutionFailure::Publication,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{ModuleCardEvidenceId, ModuleCardField};
    use std::error::Error;

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
}
