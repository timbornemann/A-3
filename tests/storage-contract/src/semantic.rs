use crate::fixture::{ContractWorkspace, project, snapshot};
use crate::{ContractResult, KnowledgeStoreContractFactory};
use a3_application::{
    EmbeddingOperationControl, KnowledgeIndexStore, SemanticCacheRebuildControl,
    SemanticCacheRebuildProgressError, SemanticEmbeddingStore, SemanticEmbeddingStoreFailure,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingCacheKey, EmbeddingDimension, EmbeddingModelId,
    EmbeddingModelProfile, EmbeddingProviderId, EmbeddingTimestamp, NormalizedSemanticCard,
    Progress, RepositoryId, SemanticCardId, SemanticEmbedding, SnapshotId, VectorSearchLimit,
    WorktreeId,
};
use std::sync::Mutex;

pub(crate) async fn verify<F>(factory: &F, workspace: &ContractWorkspace) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let app_data_root = workspace.app_data_root("semantic-cache");
    let common = workspace.create_directory("semantic-common")?;
    let root = workspace.create_directory("semantic-root")?;
    let worktree_id = WorktreeId::from_bytes([171; 32]);
    let project = project(
        RepositoryId::from_bytes([170; 32]),
        worktree_id,
        &common,
        &root,
        crate::fixture::unborn_head()?,
    )?;
    let first_snapshot = snapshot([172; 32], worktree_id, None, 1, Vec::new())?;
    let first_snapshot_id = first_snapshot.id();
    let profile = build_profile("contract-model", 2)?;
    let other_profile = build_profile("contract-model-v2", 3)?;
    let cards = [
        card([173; 32], first_snapshot_id, "semantic alpha")?,
        card([174; 32], first_snapshot_id, "semantic beta")?,
        card([175; 32], first_snapshot_id, "semantic gamma")?,
    ];
    let embeddings = [
        embedding(cards[0].clone(), &profile, vec![1.0, 0.0], 10)?,
        embedding(cards[1].clone(), &profile, vec![0.0, 1.0], 11)?,
        embedding(cards[2].clone(), &profile, vec![-1.0, 0.0], 12)?,
    ];
    let keys = cards
        .iter()
        .map(|card| EmbeddingCacheKey::from_card(card, &profile))
        .collect::<Vec<_>>();
    let active = ContractEmbeddingControl::active();

    let store = factory.open(&app_data_root).await?;
    store.append_snapshot(&project, &first_snapshot).await?;
    assert!(
        store
            .find_cached(&project, &profile, &keys, &active)
            .await?
            .is_empty()
    );
    store
        .store_batch(&project, &profile, &embeddings, &active)
        .await?;
    assert_eq!(
        store
            .find_cached(&project, &profile, &keys, &active)
            .await?,
        keys
    );

    let capability = store
        .vector_search_capability(&project, &profile, &active)
        .await?;
    let query =
        a3_domain::EmbeddingVector::normalize_l2(vec![1.0, 0.0], EmbeddingDimension::new(2)?)?;
    let result = store
        .search_similar(
            &project,
            first_snapshot_id,
            &profile,
            &query,
            VectorSearchLimit::new(2)?,
            &active,
        )
        .await?;
    assert_eq!(result.capability(), capability);
    assert_eq!(result.hits().len(), 2);
    assert_eq!(result.hits()[0].card_id(), cards[0].id());
    assert!(result.truncated());

    let second_snapshot = snapshot(
        [176; 32],
        worktree_id,
        Some(first_snapshot_id),
        2,
        Vec::new(),
    )?;
    let revised_card = card([173; 32], second_snapshot.id(), "semantic alpha revised")?;
    let revised_embedding = embedding(revised_card.clone(), &profile, vec![0.0, 1.0], 13)?;
    let revised_key = EmbeddingCacheKey::from_card(&revised_card, &profile);
    store.append_snapshot(&project, &second_snapshot).await?;
    store
        .store_batch(&project, &profile, &[revised_embedding], &active)
        .await?;
    let mut revision_keys = vec![keys[0], revised_key];
    revision_keys.sort();
    assert_eq!(
        store
            .find_cached(&project, &profile, &[keys[0], revised_key], &active)
            .await?,
        revision_keys
    );
    let revised_result = store
        .search_similar(
            &project,
            second_snapshot.id(),
            &profile,
            &query,
            VectorSearchLimit::new(2)?,
            &active,
        )
        .await?;
    assert_eq!(revised_result.hits().len(), 1);
    assert_eq!(
        revised_result.hits()[0].body_hash(),
        revised_card.body_hash()
    );

    let other_key = EmbeddingCacheKey::new(cards[0].id(), other_profile.id(), cards[0].body_hash());
    assert!(
        store
            .find_cached(&project, &other_profile, &[other_key], &active)
            .await?
            .is_empty()
    );
    let cancelled = ContractEmbeddingControl::cancelled();
    assert_eq!(
        store
            .find_cached(&project, &profile, &keys, &cancelled)
            .await,
        Err(SemanticEmbeddingStoreFailure::Cancelled)
    );
    assert_eq!(
        store.rebuild_semantic_cache(&project, &cancelled).await,
        Err(SemanticEmbeddingStoreFailure::Cancelled)
    );
    let unavailable_progress = ContractEmbeddingControl::fail_progress_after(1);
    assert_eq!(
        store
            .rebuild_semantic_cache(&project, &unavailable_progress)
            .await,
        Err(SemanticEmbeddingStoreFailure::ProgressUnavailable)
    );
    assert!(
        store
            .find_cached(&project, &profile, &keys, &active)
            .await?
            .is_empty()
    );
    store
        .store_batch(&project, &profile, &embeddings, &active)
        .await?;
    assert_eq!(
        store
            .find_cached(&project, &profile, &keys, &active)
            .await?,
        keys
    );
    drop(store);

    let reopened = factory.open(&app_data_root).await?;
    assert_eq!(
        reopened
            .find_cached(&project, &profile, &keys, &active)
            .await?,
        keys
    );
    reopened.rebuild_semantic_cache(&project, &active).await?;
    assert!(
        active
            .progress
            .lock()
            .map_err(|_| std::io::Error::other("semantic progress lock was poisoned"))?
            .last()
            .is_some_and(|progress| progress.is_complete())
    );
    assert!(
        reopened
            .find_cached(&project, &profile, &keys, &active)
            .await?
            .is_empty()
    );
    assert_eq!(
        reopened
            .latest_snapshot(&project)
            .await?
            .map(|value| value.id()),
        Some(second_snapshot.id())
    );
    Ok(())
}

fn build_profile(model: &str, dimensions: u16) -> ContractResult<EmbeddingModelProfile> {
    Ok(EmbeddingModelProfile::v1(
        EmbeddingProviderId::new("contract-local".to_owned())?,
        EmbeddingModelId::new(model.to_owned())?,
        EmbeddingDimension::new(dimensions)?,
        EmbeddingBatchSize::new(8)?,
    ))
}

fn card(
    id: [u8; 32],
    snapshot_id: SnapshotId,
    body: &str,
) -> ContractResult<NormalizedSemanticCard> {
    Ok(NormalizedSemanticCard::normalize_v1(
        SemanticCardId::from_bytes(id),
        snapshot_id,
        body,
    )?)
}

fn embedding(
    card: NormalizedSemanticCard,
    profile: &EmbeddingModelProfile,
    vector: Vec<f32>,
    created_at_ms: u64,
) -> ContractResult<SemanticEmbedding> {
    Ok(SemanticEmbedding::from_provider_output(
        card,
        profile,
        vector,
        EmbeddingTimestamp::from_unix_millis(created_at_ms)?,
    )?)
}

#[derive(Debug)]
struct ContractEmbeddingControl {
    cancelled: bool,
    progress: Mutex<Vec<Progress>>,
    maximum_progress_events: Option<usize>,
}

impl ContractEmbeddingControl {
    fn active() -> Self {
        Self {
            cancelled: false,
            progress: Mutex::new(Vec::new()),
            maximum_progress_events: None,
        }
    }

    fn cancelled() -> Self {
        Self {
            cancelled: true,
            progress: Mutex::new(Vec::new()),
            maximum_progress_events: None,
        }
    }

    fn fail_progress_after(maximum_progress_events: usize) -> Self {
        Self {
            cancelled: false,
            progress: Mutex::new(Vec::new()),
            maximum_progress_events: Some(maximum_progress_events),
        }
    }
}

impl EmbeddingOperationControl for ContractEmbeddingControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl SemanticCacheRebuildControl for ContractEmbeddingControl {
    fn report_progress(&self, progress: Progress) -> Result<(), SemanticCacheRebuildProgressError> {
        let mut events = self
            .progress
            .lock()
            .map_err(|_| SemanticCacheRebuildProgressError::Unavailable)?;
        if self
            .maximum_progress_events
            .is_some_and(|maximum| events.len() >= maximum)
        {
            return Err(SemanticCacheRebuildProgressError::Unavailable);
        }
        events.push(progress);
        Ok(())
    }
}
