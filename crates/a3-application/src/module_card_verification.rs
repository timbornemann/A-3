use crate::{
    IndexPersistenceControl, IndexPersistenceControlError, JobContext, KnowledgeIndexFailure,
    KnowledgeIndexStore,
};
use a3_domain::{
    IndexRunId, ModuleCardEvidenceId, ModuleCardVerificationCandidate, ModuleCardVerificationError,
    ModuleCardVerifier, Progress, ProjectIdentity, ResolvedModuleCardEvidence,
    ResolvedModuleCardEvidenceSet, SnapshotId, VerifiedModuleCardBatch,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

const MAX_EVIDENCE_RESOLUTION_TIMEOUT_MILLIS: u64 = 30_000;

/// Owned future returned by the object-safe Module Card evidence resolver.
pub type ModuleCardEvidenceResolverFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<ResolvedModuleCardEvidenceSet, ModuleCardEvidenceResolverFailure>,
            > + Send
            + 'a,
    >,
>;

/// Owned future returned by the verified-only Module Card publisher.
pub type VerifiedModuleCardPublisherFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), VerifiedModuleCardPublisherFailure>> + Send + 'a>>;

/// Cooperative cancellation shared by resolver and publisher adapters.
pub trait ModuleCardVerificationControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl ModuleCardVerificationControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Positive bounded deadline for one local evidence-resolution request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardEvidenceResolutionTimeout(Duration);

impl ModuleCardEvidenceResolutionTimeout {
    /// Version-one local resolver deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(5));

    /// Creates a deadline capped at 30 seconds.
    pub fn from_millis(value: u64) -> Result<Self, ModuleCardEvidenceResolutionTimeoutError> {
        if value == 0 || value > MAX_EVIDENCE_RESOLUTION_TIMEOUT_MILLIS {
            return Err(ModuleCardEvidenceResolutionTimeoutError { value });
        }
        Ok(Self(Duration::from_millis(value)))
    }

    /// Returns the neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Resolver deadline was zero or exceeded 30 seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardEvidenceResolutionTimeoutError {
    value: u64,
}

impl fmt::Display for ModuleCardEvidenceResolutionTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Module Card evidence timeout {} ms must be between 1 and {MAX_EVIDENCE_RESOLUTION_TIMEOUT_MILLIS}",
            self.value
        )
    }
}

impl Error for ModuleCardEvidenceResolutionTimeoutError {}

/// Read-only adapter boundary resolving opaque Card Evidence IDs against one snapshot.
pub trait ModuleCardEvidenceResolver: fmt::Debug + Send + Sync {
    /// Resolves exactly the requested canonical Evidence IDs without persistence mutation.
    fn resolve<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        evidence_ids: &'a [ModuleCardEvidenceId],
        timeout: ModuleCardEvidenceResolutionTimeout,
        control: &'a dyn ModuleCardVerificationControl,
    ) -> ModuleCardEvidenceResolverFuture<'a>;
}

/// Read-only resolver over the latest atomically published Knowledge Index.
#[derive(Debug, Clone, Copy)]
pub struct PublishedIndexEvidenceResolver<'a> {
    store: &'a dyn KnowledgeIndexStore,
}

impl<'a> PublishedIndexEvidenceResolver<'a> {
    /// Narrows an existing Knowledge Index store to the R9 evidence capability.
    #[must_use]
    pub const fn new(store: &'a dyn KnowledgeIndexStore) -> Self {
        Self { store }
    }
}

impl ModuleCardEvidenceResolver for PublishedIndexEvidenceResolver<'_> {
    fn resolve<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        evidence_ids: &'a [ModuleCardEvidenceId],
        timeout: ModuleCardEvidenceResolutionTimeout,
        control: &'a dyn ModuleCardVerificationControl,
    ) -> ModuleCardEvidenceResolverFuture<'a> {
        Box::pin(async move {
            let read_control = ResolverIndexReadControl::new(control, timeout.duration());
            read_control.ensure_active()?;
            let published = self
                .store
                .latest_published_index(project, &read_control)
                .await
                .map_err(|failure| read_control.classify(failure))?
                .ok_or(ModuleCardEvidenceResolverFailure::SnapshotUnavailable)?;
            read_control.ensure_active()?;
            if published.run().id() != index_run_id || published.run().snapshot_id() != snapshot_id
            {
                return Err(ModuleCardEvidenceResolverFailure::SnapshotUnavailable);
            }

            let requested = evidence_ids.iter().copied().collect::<BTreeSet<_>>();
            if requested.len() != evidence_ids.len() {
                return Err(ModuleCardEvidenceResolverFailure::InvalidResponse);
            }
            let graph = published.publication().graph();
            let mut resolved = Vec::with_capacity(requested.len());
            for revision in graph.files() {
                read_control.ensure_active()?;
                let id = ModuleCardEvidenceId::for_file_revision_v1(revision);
                if requested.contains(&id) {
                    resolved.push(ResolvedModuleCardEvidence::File {
                        id,
                        revision: revision.clone(),
                    });
                }
            }
            for symbol in graph.symbols() {
                read_control.ensure_active()?;
                let id = ModuleCardEvidenceId::for_symbol_v1(symbol);
                if requested.contains(&id) {
                    resolved.push(ResolvedModuleCardEvidence::Symbol {
                        id,
                        symbol: symbol.clone(),
                    });
                }
            }
            for edge in graph.edges() {
                read_control.ensure_active()?;
                let id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
                if requested.contains(&id) {
                    resolved.push(ResolvedModuleCardEvidence::GraphEdge {
                        id,
                        edge: edge.clone(),
                    });
                }
            }
            if resolved.len() != requested.len() {
                return Err(ModuleCardEvidenceResolverFailure::EvidenceUnavailable);
            }
            ResolvedModuleCardEvidenceSet::new(snapshot_id, resolved)
                .map_err(|_| ModuleCardEvidenceResolverFailure::InvalidResponse)
        })
    }
}

#[derive(Debug)]
struct ResolverIndexReadControl<'a> {
    control: &'a dyn ModuleCardVerificationControl,
    started: Instant,
    timeout: Duration,
}

impl<'a> ResolverIndexReadControl<'a> {
    fn new(control: &'a dyn ModuleCardVerificationControl, timeout: Duration) -> Self {
        Self {
            control,
            started: Instant::now(),
            timeout,
        }
    }

    fn ensure_active(&self) -> Result<(), ModuleCardEvidenceResolverFailure> {
        if self.control.is_cancelled() {
            Err(ModuleCardEvidenceResolverFailure::Cancelled)
        } else if self.started.elapsed() >= self.timeout {
            Err(ModuleCardEvidenceResolverFailure::TimedOut)
        } else {
            Ok(())
        }
    }

    fn classify(&self, failure: KnowledgeIndexFailure) -> ModuleCardEvidenceResolverFailure {
        if self.control.is_cancelled() {
            ModuleCardEvidenceResolverFailure::Cancelled
        } else if self.started.elapsed() >= self.timeout
            || failure == KnowledgeIndexFailure::TimedOut
        {
            ModuleCardEvidenceResolverFailure::TimedOut
        } else {
            match failure {
                KnowledgeIndexFailure::Cancelled => ModuleCardEvidenceResolverFailure::Cancelled,
                KnowledgeIndexFailure::TimedOut => ModuleCardEvidenceResolverFailure::TimedOut,
                _ => ModuleCardEvidenceResolverFailure::Storage,
            }
        }
    }
}

impl IndexPersistenceControl for ResolverIndexReadControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled() || self.started.elapsed() >= self.timeout
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        if self.is_cancelled() {
            Err(IndexPersistenceControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}

/// Publisher boundary whose input type is constructible only by successful R9 verification.
pub trait VerifiedModuleCardPublisher: fmt::Debug + Send + Sync {
    /// Atomically publishes one contradiction-free verified batch.
    fn publish<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        batch: &'a VerifiedModuleCardBatch,
        control: &'a dyn ModuleCardVerificationControl,
    ) -> VerifiedModuleCardPublisherFuture<'a>;
}

/// Inbound use case resolving all Evidence IDs and invoking the deterministic domain verifier.
#[derive(Debug, Clone, Copy)]
pub struct VerifyModuleCards<'a> {
    resolver: &'a dyn ModuleCardEvidenceResolver,
    timeout: ModuleCardEvidenceResolutionTimeout,
}

impl<'a> VerifyModuleCards<'a> {
    /// Creates the version-one verification use case.
    #[must_use]
    pub const fn version_one(resolver: &'a dyn ModuleCardEvidenceResolver) -> Self {
        Self {
            resolver,
            timeout: ModuleCardEvidenceResolutionTimeout::DEFAULT,
        }
    }

    /// Resolves a canonical exact evidence set before model-free verification.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        candidates: Vec<ModuleCardVerificationCandidate>,
        control: &dyn ModuleCardVerificationControl,
    ) -> Result<VerifiedModuleCardBatch, VerifyModuleCardsFailure> {
        if control.is_cancelled() {
            return Err(VerifyModuleCardsFailure::Cancelled);
        }
        let evidence_ids = candidates
            .iter()
            .flat_map(|candidate| candidate.proposal().evidence_ids().iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let evidence = self
            .resolver
            .resolve(
                project,
                published.run().id(),
                published.run().snapshot_id(),
                &evidence_ids,
                self.timeout,
                control,
            )
            .await?;
        if control.is_cancelled() {
            return Err(VerifyModuleCardsFailure::Cancelled);
        }
        ModuleCardVerifier::verify(published, candidates, &evidence).map_err(Into::into)
    }
}

/// Inbound publish gate accepting no Proposal or unverified Card type.
#[derive(Debug, Clone, Copy)]
pub struct PublishVerifiedModuleCards<'a> {
    publisher: &'a dyn VerifiedModuleCardPublisher,
}

impl<'a> PublishVerifiedModuleCards<'a> {
    /// Composes the verified-only publisher boundary.
    #[must_use]
    pub const fn new(publisher: &'a dyn VerifiedModuleCardPublisher) -> Self {
        Self { publisher }
    }

    /// Publishes an already verified batch and returns snapshot-bound safe metadata.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        batch: &VerifiedModuleCardBatch,
        control: &dyn ModuleCardVerificationControl,
    ) -> Result<PublishedModuleCardReceipt, PublishVerifiedModuleCardsFailure> {
        if control.is_cancelled() {
            return Err(PublishVerifiedModuleCardsFailure::Cancelled);
        }
        self.publisher.publish(project, batch, control).await?;
        if control.is_cancelled() {
            return Err(PublishVerifiedModuleCardsFailure::Cancelled);
        }
        Ok(PublishedModuleCardReceipt {
            snapshot_id: batch.snapshot_id(),
            card_count: batch.cards().len(),
        })
    }
}

/// Safe result metadata emitted only after a verified-only publisher succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedModuleCardReceipt {
    snapshot_id: SnapshotId,
    card_count: usize,
}

impl PublishedModuleCardReceipt {
    /// Returns the published immutable snapshot.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the number of atomically published Cards.
    #[must_use]
    pub const fn card_count(self) -> usize {
        self.card_count
    }
}

/// Stable resolver failure without database rows or source payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardEvidenceResolverFailure {
    /// Published snapshot is unavailable.
    SnapshotUnavailable,
    /// At least one opaque Evidence ID could not be resolved.
    EvidenceUnavailable,
    /// Resolver exceeded its fixed deadline.
    TimedOut,
    /// The local Knowledge Index could not be read.
    Storage,
    /// Owning operation cancelled resolution.
    Cancelled,
    /// Adapter output violated the domain resolver contract.
    InvalidResponse,
}

impl fmt::Display for ModuleCardEvidenceResolverFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SnapshotUnavailable => "Module Card evidence snapshot is unavailable",
            Self::EvidenceUnavailable => "Module Card evidence is unavailable",
            Self::TimedOut => "Module Card evidence resolution timed out",
            Self::Storage => "Module Card evidence storage read failed",
            Self::Cancelled => "Module Card evidence resolution was cancelled",
            Self::InvalidResponse => "Module Card evidence resolver returned an invalid response",
        })
    }
}

impl Error for ModuleCardEvidenceResolverFailure {}

/// Stable verified-only publisher failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedModuleCardPublisherFailure {
    /// Storage boundary rejected the verified batch.
    Rejected,
    /// Atomic publication failed.
    Storage,
    /// Owning operation cancelled publication before commit.
    Cancelled,
}

impl fmt::Display for VerifiedModuleCardPublisherFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Rejected => "verified Module Card batch was rejected",
            Self::Storage => "verified Module Card publication failed",
            Self::Cancelled => "verified Module Card publication was cancelled",
        })
    }
}

impl Error for VerifiedModuleCardPublisherFailure {}

/// Evidence resolution or deterministic verification failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyModuleCardsFailure {
    /// Evidence resolver boundary failed.
    Resolver(ModuleCardEvidenceResolverFailure),
    /// Deterministic domain verification failed.
    Verification(ModuleCardVerificationError),
    /// Owning operation cancelled before or after resolution.
    Cancelled,
}

impl fmt::Display for VerifyModuleCardsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Resolver(_) => "Module Card evidence resolution failed",
            Self::Verification(_) => "Module Card deterministic verification failed",
            Self::Cancelled => "Module Card verification was cancelled",
        })
    }
}

impl Error for VerifyModuleCardsFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolver(source) => Some(source),
            Self::Verification(source) => Some(source),
            Self::Cancelled => None,
        }
    }
}

impl From<ModuleCardEvidenceResolverFailure> for VerifyModuleCardsFailure {
    fn from(value: ModuleCardEvidenceResolverFailure) -> Self {
        if value == ModuleCardEvidenceResolverFailure::Cancelled {
            Self::Cancelled
        } else {
            Self::Resolver(value)
        }
    }
}

impl From<ModuleCardVerificationError> for VerifyModuleCardsFailure {
    fn from(value: ModuleCardVerificationError) -> Self {
        Self::Verification(value)
    }
}

/// Verified-only publication failed or was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishVerifiedModuleCardsFailure {
    /// Publisher boundary failed.
    Publisher(VerifiedModuleCardPublisherFailure),
    /// Owning operation cancelled before or after publication.
    Cancelled,
}

impl fmt::Display for PublishVerifiedModuleCardsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Publisher(_) => "verified Module Card publisher failed",
            Self::Cancelled => "verified Module Card publication was cancelled",
        })
    }
}

impl Error for PublishVerifiedModuleCardsFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Publisher(source) => Some(source),
            Self::Cancelled => None,
        }
    }
}

impl From<VerifiedModuleCardPublisherFailure> for PublishVerifiedModuleCardsFailure {
    fn from(value: VerifiedModuleCardPublisherFailure) -> Self {
        if value == VerifiedModuleCardPublisherFailure::Cancelled {
            Self::Cancelled
        } else {
            Self::Publisher(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModuleCardEvidenceResolutionTimeout;

    #[test]
    fn evidence_resolution_timeout_is_positive_and_bounded() {
        assert!(ModuleCardEvidenceResolutionTimeout::from_millis(0).is_err());
        assert!(ModuleCardEvidenceResolutionTimeout::from_millis(30_001).is_err());
        assert!(ModuleCardEvidenceResolutionTimeout::from_millis(500).is_ok());
    }
}
