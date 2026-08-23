use crate::{
    DeepMapExecutionFailure, DeepMapExecutionFuture, DeepMapExecutionOutcome,
    DeepMapExecutionRequest, DeepMapExecutor, DeepMapExplorerFailure, DeepMapExplorerStatus,
    DeepMapModelDescriptor, DeepMapResumeState, ExploreDeepMap, ExplorerModelFailure, JobContext,
    KnowledgeIndexFailure, KnowledgeIndexStore, ModelBackedExplorerProvider, ModelProvider,
    PlanDeepMap, ProposeModuleCardClaims, ProposeModuleCardClaimsFailure,
    PublishVerifiedModuleCards, PublishVerifiedModuleCardsFailure, PublishedIndexDeepMapReadTools,
    PublishedIndexEvidenceResolver, VerifiedModuleCardPublisher, VerifyModuleCards,
    VerifyModuleCardsFailure,
};
use a3_domain::{
    ExplorerCheckpoint, ModelProfile, ModuleCardSchemaVersion, ProjectIdentity, PublishedIndex,
};
use std::fmt;
use std::sync::Arc;

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
        self.index
            .latest_published_index(project, control)
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
        let mut candidates = Vec::with_capacity(state.checkpoint().confirmed_proposals().len());
        for proposal in state.checkpoint().confirmed_proposals() {
            match claim_proposer.execute(proposal.clone(), control).await {
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
