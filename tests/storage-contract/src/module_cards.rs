use crate::fixture::{ContractWorkspace, change, project, run, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore,
    KnowledgeSearchControl, KnowledgeSearchStore, ModuleCardVerificationControl,
    ModuleCardVerificationControlError, PublishVerifiedModuleCards,
    PublishVerifiedModuleCardsFailure, TaskLensClaimLimit, TaskLensClaimStore,
    TaskLensClaimStoreFailure, TaskLensControl, TaskLensControlError, TaskLensIndexStore,
    VerifiedModuleCardPublisherFailure,
};
use a3_domain::{
    Confidence, LexicalSearchPageSize, LexicalSearchQuery, LexicalSearchTerm, MapperProfileVersion,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal,
    ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleCardVerificationCandidate,
    ModuleCardVerifier, ModuleClaimEnvelope, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleClaimProposal, Progress, ProposedModuleCardField, RepositoryId,
    ResolvedModuleCardEvidence, ResolvedModuleCardEvidenceSet, SnapshotChangeKind, SymbolId,
    VerifiedModuleCardBatch, WorktreeId,
};
use std::sync::Mutex;

#[derive(Debug)]
struct ContractIndexControl;

impl IndexPersistenceControl for ContractIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ContractCardControl {
    progress: Mutex<Vec<Progress>>,
}

impl ModuleCardVerificationControl for ContractCardControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        progress: Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        self.progress
            .lock()
            .map_err(|_| ModuleCardVerificationControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct CancelledCardControl;

impl ModuleCardVerificationControl for CancelledCardControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), ModuleCardVerificationControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct ContractSearchControl;

impl KnowledgeSearchControl for ContractSearchControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct ContractTaskLensControl;

impl TaskLensControl for ContractTaskLensControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct CancelledTaskLensControl;

impl TaskLensControl for CancelledTaskLensControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data = workspace.app_data_root("module-card-publication");
    let common = workspace.create_directory("module-card-publication-common")?;
    let root = workspace.create_directory("module-card-publication-root")?;
    let repository_id = RepositoryId::from_bytes([131; 32]);
    let worktree_id = WorktreeId::from_bytes([132; 32]);
    let project = project(repository_id, worktree_id, &common, &root, unborn_head()?)?;
    let store = factory.open(&app_data).await?;
    let snapshot = snapshot(
        [133; 32],
        worktree_id,
        None,
        1,
        vec![change(
            b"src/lib.rs",
            [134; 32],
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    store.append_snapshot(&project, &snapshot).await?;
    let run = store
        .start_index_run(&project, run([135; 32], snapshot.id(), 1)?)
        .await?;
    let publication = crate::index::publication(snapshot.id(), b"src/lib.rs", [134; 32], 136)?;
    store
        .publish_index(&project, run.id(), &publication, &ContractIndexControl)
        .await?;
    let published = store
        .latest_published_index(&project, &ContractIndexControl)
        .await?
        .ok_or("published Module Card fixture is missing")?;
    let batch = verified_batch(&published)?;
    let publisher = PublishVerifiedModuleCards::new(&store);

    assert_eq!(
        publisher
            .execute(&project, &batch, &CancelledCardControl)
            .await,
        Err(PublishVerifiedModuleCardsFailure::Cancelled)
    );
    let control = ContractCardControl::default();
    let receipt = publisher.execute(&project, &batch, &control).await?;
    assert_eq!(receipt.snapshot_id(), snapshot.id());
    assert_eq!(receipt.card_count(), 1);
    {
        let progress = control.progress.lock().map_err(|_| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                "Module Card progress lock was poisoned",
            ))
        })?;
        assert!(progress.len() <= 64);
        assert_eq!(
            progress.first().and_then(|value| value.completed()),
            Some(0)
        );
        assert_eq!(progress.last().map(|value| value.is_complete()), Some(true));
    }
    assert_eq!(
        publisher
            .execute(&project, &batch, &ContractCardControl::default())
            .await,
        Err(PublishVerifiedModuleCardsFailure::Publisher(
            VerifiedModuleCardPublisherFailure::Rejected
        ))
    );

    assert_eq!(
        store
            .load_claims(
                &project,
                &published,
                TaskLensClaimLimit::DEFAULT,
                &CancelledTaskLensControl,
            )
            .await,
        Err(TaskLensClaimStoreFailure::Cancelled)
    );
    let claims = store
        .load_claims(
            &project,
            &published,
            TaskLensClaimLimit::DEFAULT,
            &ContractTaskLensControl,
        )
        .await?;
    assert!(!claims.truncated());
    assert_eq!(claims.claims().len(), 1);
    assert_eq!(
        claims.claims()[0].id(),
        ModuleCardClaimId::from_bytes([138; 32])
    );
    assert_eq!(claims.claims()[0].evidence().len(), 1);
    assert_eq!(
        claims.claims()[0].evidence()[0].id(),
        batch.evidence().evidence()[0].id()
    );
    assert_eq!(
        store
            .load_current_index(&project, &ContractTaskLensControl)
            .await?
            .ok_or("shared Task Lens index is missing")?
            .run(),
        published.run()
    );

    let query = LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("launch".to_owned())?);
    let page = store
        .search_lexical(
            &project,
            &query,
            LexicalSearchPageSize::DEFAULT,
            None,
            &ContractSearchControl,
        )
        .await?;
    assert!(!page.hits().is_empty());

    let replacement_snapshot = crate::fixture::snapshot(
        [139; 32],
        worktree_id,
        Some(snapshot.id()),
        2,
        vec![change(
            b"src/lib.rs",
            [140; 32],
            SnapshotChangeKind::Upsert,
        )?],
    )?;
    store
        .append_snapshot(&project, &replacement_snapshot)
        .await?;
    let replacement_run = store
        .start_index_run(
            &project,
            crate::fixture::run([141; 32], replacement_snapshot.id(), 1)?,
        )
        .await?;
    let replacement_publication =
        crate::index::publication(replacement_snapshot.id(), b"src/lib.rs", [140; 32], 142)?;
    store
        .publish_index(
            &project,
            replacement_run.id(),
            &replacement_publication,
            &ContractIndexControl,
        )
        .await?;
    assert_eq!(
        store
            .load_claims(
                &project,
                &published,
                TaskLensClaimLimit::DEFAULT,
                &ContractTaskLensControl,
            )
            .await,
        Err(TaskLensClaimStoreFailure::InvalidStoredProjection)
    );
    let replacement = store
        .latest_published_index(&project, &ContractIndexControl)
        .await?
        .ok_or("replacement publication is missing")?;
    assert_eq!(
        store
            .load_current_index(&project, &ContractTaskLensControl)
            .await?
            .ok_or("replacement Task Lens index is missing")?
            .run(),
        replacement.run()
    );
    assert!(
        store
            .load_claims(
                &project,
                &replacement,
                TaskLensClaimLimit::DEFAULT,
                &ContractTaskLensControl,
            )
            .await?
            .claims()
            .is_empty()
    );

    store
        .rebuild_regenerable_index(&project, &ContractIndexControl)
        .await?;
    assert_eq!(
        store
            .load_current_index(&project, &ContractTaskLensControl)
            .await?,
        None
    );
    assert_eq!(store.latest_index_run(&project).await?, None);
    crate::release_contract_store(store);
    let reopened = factory.open(&app_data).await?;
    assert_eq!(
        reopened.latest_snapshot(&project).await?,
        Some(replacement_snapshot)
    );
    assert_eq!(reopened.latest_index_run(&project).await?, None);
    crate::complete_contract_phase()
}

fn verified_batch(
    published: &a3_domain::PublishedIndex,
) -> ContractResult<VerifiedModuleCardBatch> {
    let module = published
        .publication()
        .modules()
        .modules()
        .first()
        .ok_or("published fixture module is missing")?;
    let symbol = published
        .publication()
        .graph()
        .symbols()
        .first()
        .ok_or("published fixture symbol is missing")?;
    let evidence_id = ModuleCardEvidenceId::for_symbol_v1(symbol);
    let card_id = ModuleCardId::from_bytes([137; 32]);
    let proposal = ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::from_basis_points(8_000)?,
        ),
        vec![ProposedModuleCardField::new(
            ModuleCardField::PublicSurface,
            vec!["launch".to_owned()],
            vec![evidence_id],
        )?],
        512,
    )?;
    let claim = ModuleClaimProposal::new(
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([138; 32]),
            card_id,
            module.id(),
            published.run().snapshot_id(),
            ModuleCardField::PublicSurface,
            0,
            Confidence::from_basis_points(7_000)?,
        ),
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Symbol(SymbolId::from_bytes(*symbol.id().as_bytes())),
        vec![evidence_id],
    )?;
    let candidate = ModuleCardVerificationCandidate::new(proposal, vec![claim])?;
    let evidence = ResolvedModuleCardEvidenceSet::new(
        published.run().snapshot_id(),
        vec![ResolvedModuleCardEvidence::Symbol {
            id: evidence_id,
            symbol: symbol.clone(),
        }],
    )?;
    Ok(ModuleCardVerifier::verify(
        published,
        vec![candidate],
        &evidence,
    )?)
}
