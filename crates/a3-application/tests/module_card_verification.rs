//! R9 contracts for current evidence resolution and verified-only publication.

use a3_application::{
    IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeIndexFuture, KnowledgeIndexStore,
    ModuleCardEvidenceResolver, ModuleCardVerificationControl, PublishVerifiedModuleCards,
    PublishedIndexEvidenceResolver, VerifiedModuleCardPublisher,
    VerifiedModuleCardPublisherFailure, VerifiedModuleCardPublisherFuture, VerifyModuleCards,
};
use a3_domain::{
    CanonicalDirectory, Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GitHead,
    GitReferenceName, GraphEdge, GraphEndpoint, GraphSymbol, IndexLanguage, IndexPublication,
    IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStart, IndexRunStatus,
    IndexRunTerminalOutcome, LinkResolution, LinkedGraph, LocalSymbolId, MapperProfileVersion,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal,
    ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleCardVerificationCandidate,
    ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimProposal, ModuleId,
    ModuleKind, ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection,
    ModuleRoot, ModuleSymbolSet, ParsedSymbol, Progress, ProjectIdentity, ProposedModuleCardField,
    PublishedIndex, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
    RepositoryFileState, RepositoryId, RepositoryIdentity, RepositoryModule, RepositoryPath,
    Snapshot, SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName,
    SymbolRank, SymbolRankSignals, SyntaxProvider, SyntaxRelationKind, VerifiedModuleCardBatch,
    WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use futures::executor::block_on;
use std::collections::BTreeSet;
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug)]
struct FixedIndexStore {
    published: PublishedIndex,
    reads: AtomicUsize,
}

impl FixedIndexStore {
    fn new(published: PublishedIndex) -> Self {
        Self {
            published,
            reads: AtomicUsize::new(0),
        }
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::Acquire)
    }
}

impl KnowledgeIndexStore for FixedIndexStore {
    fn append_snapshot<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _snapshot: &'a Snapshot,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotConflict) })
    }

    fn latest_snapshot<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
    }

    fn current_file_state<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, RepositoryFileState> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
    }

    fn start_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _request: IndexRunStart,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunAlreadyActive) })
    }

    fn finish_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _run_id: IndexRunId,
        _outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn publish_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _run_id: IndexRunId,
        _publication: &'a IndexPublication,
        _control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::InvalidIndexRunTransition) })
    }

    fn latest_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn latest_published_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn latest_published_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>> {
        self.reads.fetch_add(1, Ordering::AcqRel);
        let published = self.published.clone();
        Box::pin(async move {
            if control.is_cancelled() {
                Err(KnowledgeIndexFailure::Cancelled)
            } else {
                Ok(Some(published))
            }
        })
    }

    fn rebuild_regenerable_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async { Err(KnowledgeIndexFailure::InvalidIndexRunTransition) })
    }
}

#[derive(Debug, Default)]
struct TestControl(AtomicBool);

impl TestControl {
    fn cancelled() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl ModuleCardVerificationControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), a3_application::ModuleCardVerificationControlError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RecordingPublisher(AtomicUsize);

impl RecordingPublisher {
    fn calls(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
struct BorrowedControl<'a>(&'a AtomicBool);

impl ModuleCardVerificationControl for BorrowedControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), a3_application::ModuleCardVerificationControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct CancelAfterCommitPublisher<'a>(&'a AtomicBool);

impl VerifiedModuleCardPublisher for CancelAfterCommitPublisher<'_> {
    fn publish<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _batch: &'a VerifiedModuleCardBatch,
        _timeout: a3_application::ModuleCardPublicationTimeout,
        _control: &'a dyn ModuleCardVerificationControl,
    ) -> VerifiedModuleCardPublisherFuture<'a> {
        Box::pin(async move {
            self.0.store(true, Ordering::Release);
            Ok(())
        })
    }
}

impl VerifiedModuleCardPublisher for RecordingPublisher {
    fn publish<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _batch: &'a VerifiedModuleCardBatch,
        _timeout: a3_application::ModuleCardPublicationTimeout,
        control: &'a dyn ModuleCardVerificationControl,
    ) -> VerifiedModuleCardPublisherFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(VerifiedModuleCardPublisherFailure::Cancelled);
            }
            self.0.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
    }
}

#[test]
fn resolver_returns_only_exact_current_evidence_and_honors_cancellation()
-> Result<(), Box<dyn Error>> {
    let published = published_fixture()?;
    let project = project_fixture()?;
    let store = FixedIndexStore::new(published.clone());
    let resolver = PublishedIndexEvidenceResolver::new(&store);
    let graph = published.publication().graph();
    let revision = graph
        .files()
        .iter()
        .find(|revision| revision.path().as_bytes() == b"src/lib.rs")
        .ok_or("source revision missing")?;
    let symbol = graph.symbols().first().ok_or("symbol missing")?;
    let edge = graph.edges().first().ok_or("edge missing")?;
    let expected = [
        ModuleCardEvidenceId::for_file_revision_v1(revision),
        ModuleCardEvidenceId::for_symbol_v1(symbol),
        ModuleCardEvidenceId::for_graph_edge_v1(edge),
    ];

    let resolved = block_on(resolver.resolve(
        &project,
        published.run().id(),
        published.run().snapshot_id(),
        &expected,
        a3_application::ModuleCardEvidenceResolutionTimeout::DEFAULT,
        &TestControl::default(),
    ))?;
    assert_eq!(
        resolved
            .evidence()
            .iter()
            .map(a3_domain::ResolvedModuleCardEvidence::id)
            .collect::<BTreeSet<_>>(),
        expected.into_iter().collect()
    );
    assert_eq!(store.reads(), 1);

    let cancelled_store = FixedIndexStore::new(published.clone());
    let cancelled_resolver = PublishedIndexEvidenceResolver::new(&cancelled_store);
    let cancelled = block_on(cancelled_resolver.resolve(
        &project,
        published.run().id(),
        published.run().snapshot_id(),
        &expected,
        a3_application::ModuleCardEvidenceResolutionTimeout::DEFAULT,
        &TestControl::cancelled(),
    ));
    assert_eq!(
        cancelled,
        Err(a3_application::ModuleCardEvidenceResolverFailure::Cancelled)
    );
    assert_eq!(cancelled_store.reads(), 0);
    Ok(())
}

#[test]
fn resolver_rejects_stale_run_and_fabricated_evidence_id() -> Result<(), Box<dyn Error>> {
    let published = published_fixture()?;
    let project = project_fixture()?;
    let store = FixedIndexStore::new(published.clone());
    let resolver = PublishedIndexEvidenceResolver::new(&store);
    let fabricated = [ModuleCardEvidenceId::from_bytes([99; 32])];

    let stale = block_on(resolver.resolve(
        &project,
        IndexRunId::from_bytes([98; 32]),
        published.run().snapshot_id(),
        &fabricated,
        a3_application::ModuleCardEvidenceResolutionTimeout::DEFAULT,
        &TestControl::default(),
    ));
    assert_eq!(
        stale,
        Err(a3_application::ModuleCardEvidenceResolverFailure::SnapshotUnavailable)
    );
    let missing = block_on(resolver.resolve(
        &project,
        published.run().id(),
        published.run().snapshot_id(),
        &fabricated,
        a3_application::ModuleCardEvidenceResolutionTimeout::DEFAULT,
        &TestControl::default(),
    ));
    assert_eq!(
        missing,
        Err(a3_application::ModuleCardEvidenceResolverFailure::EvidenceUnavailable)
    );
    Ok(())
}

#[test]
fn only_a_verified_batch_crosses_the_publish_boundary() -> Result<(), Box<dyn Error>> {
    let published = published_fixture()?;
    let project = project_fixture()?;
    let candidate = verification_candidate(&published)?;
    let store = FixedIndexStore::new(published.clone());
    let resolver = PublishedIndexEvidenceResolver::new(&store);
    let control = TestControl::default();
    let batch = block_on(VerifyModuleCards::version_one(&resolver).execute(
        &project,
        &published,
        vec![candidate],
        &control,
    ))?;
    assert_eq!(batch.cards().len(), 1);

    let publisher = RecordingPublisher::default();
    let receipt =
        block_on(PublishVerifiedModuleCards::new(&publisher).execute(&project, &batch, &control))?;
    assert_eq!(receipt.snapshot_id(), published.run().snapshot_id());
    assert_eq!(receipt.card_count(), 1);
    assert_eq!(publisher.calls(), 1);

    let cancellation = AtomicBool::new(false);
    let late_cancel_publisher = CancelAfterCommitPublisher(&cancellation);
    let late_cancel_control = BorrowedControl(&cancellation);
    let committed = block_on(
        PublishVerifiedModuleCards::new(&late_cancel_publisher).execute(
            &project,
            &batch,
            &late_cancel_control,
        ),
    )?;
    assert_eq!(committed.card_count(), 1);
    assert!(cancellation.load(Ordering::Acquire));
    Ok(())
}

fn verification_candidate(
    published: &PublishedIndex,
) -> Result<ModuleCardVerificationCandidate, Box<dyn Error>> {
    let module = published
        .publication()
        .modules()
        .modules()
        .first()
        .ok_or("module missing")?;
    let edge = published
        .publication()
        .graph()
        .edges()
        .first()
        .ok_or("edge missing")?;
    let evidence_id = ModuleCardEvidenceId::for_graph_edge_v1(edge);
    let card_id = ModuleCardId::from_bytes([50; 32]);
    let proposal = ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::certain(),
        ),
        vec![ProposedModuleCardField::new(
            ModuleCardField::PublicSurface,
            vec!["exports main".to_owned()],
            vec![evidence_id],
        )?],
        512,
    )?;
    let claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([51; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::PublicSurface,
            0,
            Confidence::from_basis_points(8_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Relation {
            source: edge.source().clone(),
            target: edge.target().clone(),
            kind: edge.kind(),
        },
        vec![evidence_id],
    )?;
    Ok(ModuleCardVerificationCandidate::new(proposal, vec![claim])?)
}

fn project_fixture() -> Result<ProjectIdentity, Box<dyn Error>> {
    let repository_id = RepositoryId::from_bytes([21; 32]);
    let path = std::fs::canonicalize(std::env::current_dir()?)?;
    let repository = RepositoryIdentity::new(
        repository_id,
        CanonicalDirectory::from_canonicalized(path.clone())?,
        None,
    );
    let worktree = WorktreeIdentity::new(
        WorktreeId::from_bytes([22; 32]),
        WorktreeAnchorId::from_bytes([23; 32]),
        repository_id,
        CanonicalDirectory::from_canonicalized(path)?,
    );
    let head = GitHead::Unborn {
        reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
    };
    Ok(ProjectIdentity::new(repository, worktree, head)?)
}

fn published_fixture() -> Result<PublishedIndex, Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([1; 32]);
    let manifest = revision("Cargo.toml", 2)?;
    let source = revision("src/lib.rs", 3)?;
    let symbol_id = SymbolId::from_bytes([4; 32]);
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let symbol = GraphSymbol::new(
        symbol_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("main".to_owned())?,
            range,
            range,
        )?,
    );
    let edge = GraphEdge::new(
        GraphEndpoint::File(source.path().clone()),
        GraphEndpoint::Symbol(symbol_id),
        SyntaxRelationKind::Exports,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        EvidenceRef::new(source.clone(), range),
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![manifest.clone(), source.clone()],
        vec![symbol],
        vec![edge],
        Vec::new(),
    )?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(1_000)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(1_000)?,
                degree_contribution: 0,
                centrality_contribution: 1_000,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        )],
    )?;
    let module_id = ModuleId::from_bytes([5; 32]);
    let featured = ModuleSymbolSet::new(vec![symbol_id], false)?;
    let module = RepositoryModule::new(
        module_id,
        ModuleKind::ManifestBoundary,
        Some(ModuleRoot::Repository),
        vec![manifest.clone()],
        featured.clone(),
        featured.clone(),
        ModuleSymbolSet::empty(),
    )?;
    let membership = ModuleMembership::new(
        module_id,
        symbol_id,
        ModuleMembershipEvidence::manifest(source, manifest.clone()),
    );
    let repository_card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module_id],
        vec![IndexLanguage::Rust],
        featured,
        2,
        1,
    )?;
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module],
        vec![membership],
        repository_card,
    )?;
    let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([6; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(PublishedIndex::new(run, publication)?)
}

fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}
