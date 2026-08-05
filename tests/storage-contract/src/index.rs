use crate::fixture::{ContractWorkspace, change, project, run, snapshot, unborn_head};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexFailure,
    KnowledgeIndexStore, KnowledgeStoreFailure,
};
use a3_domain::{
    Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
    GraphSymbol, IndexPublication, IndexRunId, IndexRunStatus, IndexRunTerminalOutcome,
    LinkResolution, LinkedGraph, LocalSymbolId, ParsedSymbol, RankProjection, RankScore,
    RankingPolicyVersion, RepositoryFileState, RepositoryId, RepositoryPath, SnapshotChangeKind,
    SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank,
    SymbolRankSignals, SymbolReference, SymbolRole, SymbolSignature, SymbolVisibility,
    SyntaxProvider, SyntaxRelationKind, UnresolvedEdgeCandidate, UnresolvedGraphTarget,
    UnresolvedReason, WorktreeId,
};

#[derive(Debug)]
struct ContractIndexControl;

impl IndexPersistenceControl for ContractIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let control = ContractIndexControl;
    let app_data = workspace.app_data_root("index");
    let common = workspace.create_directory("index-common")?;
    let primary_root = workspace.create_directory("index-primary")?;
    let linked_root = workspace.create_directory("index-linked")?;
    let repository_id = RepositoryId::from_bytes([4; 32]);
    let primary_id = WorktreeId::from_bytes([41; 32]);
    let linked_id = WorktreeId::from_bytes([42; 32]);
    let primary = project(
        repository_id,
        primary_id,
        &common,
        &primary_root,
        unborn_head()?,
    )?;
    let linked = project(
        repository_id,
        linked_id,
        &common,
        &linked_root,
        unborn_head()?,
    )?;
    let store = factory.open(&app_data).await?;

    assert_eq!(store.latest_snapshot(&primary).await?, None);
    assert_eq!(store.latest_snapshot(&linked).await?, None);
    assert_eq!(
        store.current_file_state(&primary).await?,
        RepositoryFileState::empty()
    );
    let first = snapshot(
        [51; 32],
        primary_id,
        None,
        1,
        vec![
            change(b"src/z.rs", [2; 32], SnapshotChangeKind::Delete)?,
            change(b"src/a.rs", [1; 32], SnapshotChangeKind::Upsert)?,
        ],
    )?;
    store.append_snapshot(&primary, &first).await?;
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));
    assert_eq!(
        store.current_file_state(&primary).await?,
        file_state(vec![(b"src/a.rs", [1; 32])])?
    );
    assert_eq!(store.latest_snapshot(&linked).await?, None);

    let foreign = snapshot(
        [52; 32],
        WorktreeId::from_bytes([99; 32]),
        None,
        1,
        Vec::new(),
    )?;
    assert_eq!(
        store.append_snapshot(&primary, &foreign).await,
        Err(KnowledgeIndexFailure::Storage(
            KnowledgeStoreFailure::IdentityConflict
        ))
    );
    let stale = snapshot(
        [53; 32],
        primary_id,
        Some(SnapshotId::from_bytes([100; 32])),
        2,
        Vec::new(),
    )?;
    assert_eq!(
        store.append_snapshot(&primary, &stale).await,
        Err(KnowledgeIndexFailure::SnapshotConflict)
    );
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));

    let linked_snapshot = snapshot([54; 32], linked_id, None, 1, Vec::new())?;
    store.append_snapshot(&linked, &linked_snapshot).await?;
    assert_eq!(
        store.latest_snapshot(&linked).await?,
        Some(linked_snapshot.clone())
    );
    assert_eq!(store.latest_snapshot(&primary).await?, Some(first.clone()));

    let modified = snapshot(
        [55; 32],
        primary_id,
        Some(first.id()),
        2,
        vec![
            change(b"src/a.rs", [8; 32], SnapshotChangeKind::Upsert)?,
            change(b"src/b.rs", [9; 32], SnapshotChangeKind::Upsert)?,
        ],
    )?;
    store.append_snapshot(&primary, &modified).await?;
    assert_eq!(
        store.current_file_state(&primary).await?,
        file_state(vec![(b"src/a.rs", [8; 32]), (b"src/b.rs", [9; 32])])?
    );
    let second = snapshot(
        [56; 32],
        primary_id,
        Some(modified.id()),
        3,
        vec![change(b"src/a.rs", [8; 32], SnapshotChangeKind::Delete)?],
    )?;
    store.append_snapshot(&primary, &second).await?;
    drop(store);

    let reopened = factory.open(&app_data).await?;
    assert_eq!(
        reopened.latest_snapshot(&primary).await?,
        Some(second.clone())
    );
    assert_eq!(
        reopened.current_file_state(&primary).await?,
        file_state(vec![(b"src/b.rs", [9; 32])])?
    );
    assert_eq!(
        reopened.latest_snapshot(&linked).await?,
        Some(linked_snapshot.clone())
    );
    assert_eq!(
        reopened
            .start_index_run(
                &primary,
                run([61; 32], SnapshotId::from_bytes([101; 32]), 1)?,
            )
            .await,
        Err(KnowledgeIndexFailure::SnapshotNotFound)
    );

    let first_run = reopened
        .start_index_run(&primary, run([62; 32], first.id(), 1)?)
        .await?;
    assert_eq!(first_run.sequence().get(), 1);
    assert_eq!(first_run.status(), IndexRunStatus::Building);
    assert_eq!(reopened.latest_index_run(&primary).await?, Some(first_run));
    assert_eq!(
        reopened
            .start_index_run(&primary, run([63; 32], second.id(), 1)?)
            .await,
        Err(KnowledgeIndexFailure::IndexRunAlreadyActive)
    );
    assert_eq!(
        reopened
            .finish_index_run(
                &primary,
                IndexRunId::from_bytes([110; 32]),
                IndexRunTerminalOutcome::Failed,
            )
            .await,
        Err(KnowledgeIndexFailure::IndexRunNotFound)
    );
    assert_eq!(reopened.latest_published_index_run(&primary).await?, None);

    let failed = reopened
        .finish_index_run(&primary, first_run.id(), IndexRunTerminalOutcome::Failed)
        .await?;
    assert_eq!(failed.status(), IndexRunStatus::Failed);
    assert_eq!(
        reopened
            .finish_index_run(&primary, first_run.id(), IndexRunTerminalOutcome::Cancelled,)
            .await,
        Err(KnowledgeIndexFailure::InvalidIndexRunTransition)
    );

    let second_run = reopened
        .start_index_run(&primary, run([63; 32], second.id(), 2)?)
        .await?;
    assert_eq!(second_run.sequence().get(), 2);
    let cancelled = reopened
        .finish_index_run(
            &primary,
            second_run.id(),
            IndexRunTerminalOutcome::Cancelled,
        )
        .await?;
    assert_eq!(cancelled.status(), IndexRunStatus::Cancelled);
    drop(reopened);

    let reopened_again = factory.open(&app_data).await?;
    assert_eq!(
        reopened_again.latest_index_run(&primary).await?,
        Some(cancelled)
    );
    assert_eq!(
        reopened_again.latest_published_index_run(&primary).await?,
        None
    );

    let first_publication = publication(second.id(), b"src/b.rs", [9; 32], 70)?;
    let first_published_run = reopened_again
        .start_index_run(&primary, run([64; 32], second.id(), 1)?)
        .await?;
    assert_eq!(first_published_run.sequence().get(), 3);
    let first_published_run = reopened_again
        .publish_index(
            &primary,
            first_published_run.id(),
            &first_publication,
            &control,
        )
        .await?;
    assert_eq!(first_published_run.status(), IndexRunStatus::Published);
    assert_eq!(
        reopened_again.latest_published_index_run(&primary).await?,
        Some(first_published_run)
    );
    let visible = reopened_again
        .latest_published_index(&primary, &control)
        .await?
        .ok_or("published index is missing")?;
    assert_eq!(visible.run(), first_published_run);
    assert_eq!(visible.publication(), &first_publication);
    drop(reopened_again);

    let reopened_again = factory.open(&app_data).await?;
    let reopened_visible = reopened_again
        .latest_published_index(&primary, &control)
        .await?
        .ok_or("published index did not survive reopen")?;
    assert_eq!(reopened_visible.run(), first_published_run);
    assert_eq!(reopened_visible.publication(), &first_publication);

    let duplicate_run = reopened_again
        .start_index_run(&primary, run([67; 32], second.id(), 1)?)
        .await?;
    assert_eq!(
        reopened_again
            .publish_index(&primary, duplicate_run.id(), &first_publication, &control,)
            .await,
        Err(KnowledgeIndexFailure::InvalidIndexRunTransition)
    );
    reopened_again
        .finish_index_run(
            &primary,
            duplicate_run.id(),
            IndexRunTerminalOutcome::Failed,
        )
        .await?;

    let mismatched_run = reopened_again
        .start_index_run(&primary, run([65; 32], second.id(), 2)?)
        .await?;
    assert_eq!(
        reopened_again
            .publish_index(&primary, mismatched_run.id(), &first_publication, &control,)
            .await,
        Err(KnowledgeIndexFailure::IndexPublicationMismatch)
    );
    assert_eq!(
        reopened_again.latest_published_index_run(&primary).await?,
        Some(first_published_run)
    );
    assert_eq!(
        reopened_again
            .rebuild_regenerable_index(&primary, &control)
            .await,
        Err(KnowledgeIndexFailure::IndexRunAlreadyActive)
    );
    reopened_again
        .finish_index_run(
            &primary,
            mismatched_run.id(),
            IndexRunTerminalOutcome::Failed,
        )
        .await?;

    let replacement_snapshot = snapshot(
        [57; 32],
        primary_id,
        Some(second.id()),
        4,
        vec![
            change(b"src/b.rs", [9; 32], SnapshotChangeKind::Delete)?,
            change(b"src/c.rs", [10; 32], SnapshotChangeKind::Upsert)?,
        ],
    )?;
    reopened_again
        .append_snapshot(&primary, &replacement_snapshot)
        .await?;
    let replacement_publication =
        publication(replacement_snapshot.id(), b"src/c.rs", [10; 32], 71)?;
    let replacement_run = reopened_again
        .start_index_run(&primary, run([66; 32], replacement_snapshot.id(), 1)?)
        .await?;
    let replacement_run = reopened_again
        .publish_index(
            &primary,
            replacement_run.id(),
            &replacement_publication,
            &control,
        )
        .await?;
    let replacement_visible = reopened_again
        .latest_published_index(&primary, &control)
        .await?
        .ok_or("replacement index is missing")?;
    assert_eq!(replacement_visible.run(), replacement_run);
    assert_eq!(replacement_visible.publication(), &replacement_publication);
    assert_ne!(replacement_visible.publication(), &first_publication);

    reopened_again
        .rebuild_regenerable_index(&primary, &control)
        .await?;
    assert_eq!(reopened_again.latest_index_run(&primary).await?, None);
    assert_eq!(
        reopened_again
            .latest_published_index(&primary, &control)
            .await?,
        None
    );
    assert_eq!(
        reopened_again.latest_snapshot(&primary).await?,
        Some(replacement_snapshot.clone())
    );
    assert_eq!(
        reopened_again.current_file_state(&primary).await?,
        file_state(vec![(b"src/c.rs", [10; 32])])?
    );
    assert_eq!(
        reopened_again.latest_snapshot(&linked).await?,
        Some(linked_snapshot.clone())
    );
    drop(reopened_again);

    let after_rebuild = factory.open(&app_data).await?;
    assert_eq!(
        after_rebuild
            .latest_published_index(&primary, &control)
            .await?,
        None
    );
    assert_eq!(
        after_rebuild.latest_snapshot(&primary).await?,
        Some(replacement_snapshot)
    );
    assert_eq!(
        after_rebuild.latest_snapshot(&linked).await?,
        Some(linked_snapshot)
    );
    Ok(())
}

fn publication(
    snapshot_id: SnapshotId,
    path: &[u8],
    hash: [u8; 32],
    symbol_byte: u8,
) -> ContractResult<IndexPublication> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
    );
    let declaration =
        SourceRange::new(0, 12, SourcePosition::new(0, 0), SourcePosition::new(0, 12))?;
    let selection = SourceRange::new(3, 9, SourcePosition::new(0, 3), SourcePosition::new(0, 9))?;
    let evidence_range =
        SourceRange::new(0, 9, SourcePosition::new(0, 0), SourcePosition::new(0, 9))?;
    let parsed = ParsedSymbol::new(
        LocalSymbolId::new(1)?,
        SymbolKind::Function,
        SymbolName::try_from_string("launch".to_owned())?,
        declaration,
        selection,
    )?
    .with_signature(SymbolSignature::try_from_string("fn launch()".to_owned())?)
    .with_visibility(SymbolVisibility::Public)
    .with_role(SymbolRole::Entrypoint)
    .with_documentation_range(evidence_range);
    let symbol_id = SymbolId::from_bytes([symbol_byte; 32]);
    let symbol = GraphSymbol::new(symbol_id, revision.clone(), parsed);
    let evidence = EvidenceRef::new(revision.clone(), evidence_range);
    let edge = GraphEdge::new(
        GraphEndpoint::File(revision.path().clone()),
        GraphEndpoint::Symbol(symbol_id),
        SyntaxRelationKind::Defines,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterLocalSymbol,
        snapshot_id,
        evidence.clone(),
    );
    let unresolved = UnresolvedEdgeCandidate::new(
        GraphEndpoint::Symbol(symbol_id),
        UnresolvedGraphTarget::Reference(SymbolReference::try_from_string(
            "runtime_target".to_owned(),
        )?),
        SyntaxRelationKind::Calls,
        SyntaxProvider::LanguageHeuristic,
        Confidence::from_basis_points(4_000)?,
        UnresolvedReason::DynamicReference,
        snapshot_id,
        evidence,
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![revision],
        vec![symbol],
        vec![edge],
        vec![unresolved],
    )?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(10_200)?,
            SymbolRankSignals {
                in_degree: 1,
                out_degree: 0,
                centrality: Centrality::from_basis_points(10_000)?,
                degree_contribution: 200,
                centrality_contribution: 3_000,
                entrypoint_contribution: 5_000,
                public_export_contribution: 2_000,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        )],
    )?;
    Ok(IndexPublication::new(graph, ranking)?)
}

fn file_state(entries: Vec<(&[u8], [u8; 32])>) -> ContractResult<RepositoryFileState> {
    let revisions = entries
        .into_iter()
        .map(|(path, hash)| {
            Ok(FileRevision::new(
                RepositoryPath::try_from_bytes(path.to_vec())?,
                ContentHash::from_bytes(hash),
            ))
        })
        .collect::<Result<Vec<_>, a3_domain::RepositoryPathError>>()?;
    Ok(RepositoryFileState::new(revisions)?)
}
