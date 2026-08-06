//! Reusable, adapter-neutral contracts for A^3 storage ports.
//!
//! This crate is dev-only. It depends on application and domain contracts, never
//! on a storage engine, so every adapter can execute the exact same behavior.

mod catalog;
mod fixture;
mod goal_contract;
mod index;
mod module_cards;
mod reconciliation;
mod run_journal;
mod search;
mod semantic;
mod task_ledger;

use a3_application::{
    AgentActionStore, GoalContractStore, KnowledgeIndexStore, KnowledgeSearchStore, KnowledgeStore,
    ModuleRemapQueueStore, RunJournalStore, SemanticEmbeddingStore, TaskLedgerStore,
    TaskLensClaimStore, TaskLensIndexStore, VerifiedModuleCardPublisher,
};
use a3_domain::{
    FileRevision, GraphEndpoint, IndexLanguage, LinkedGraph, ModuleId, ModuleKind,
    ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot,
    ModuleSymbolSet, RankProjection, RepositoryCard, RepositoryModule, SymbolRole,
    SyntaxRelationKind,
};
use std::error::Error;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// Boxed error used by adapter factories and shared contract scenarios.
pub type ContractError = Box<dyn Error + Send + Sync + 'static>;

/// Result returned by the shared contract harness.
pub type ContractResult<T> = Result<T, ContractError>;

pub(crate) fn complete_contract_phase() -> ContractResult<()> {
    #[cfg(windows)]
    if let Some(marker) = std::env::var_os("A3_STORAGE_CONTRACT_SUCCESS_MARKER") {
        std::fs::write(marker, b"complete")?;
    }
    Ok(())
}

pub(crate) fn release_contract_store<S>(store: S) {
    #[cfg(windows)]
    if std::env::var_os("A3_STORAGE_CONTRACT_RETAIN_WORKSPACE").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::mem::forget(store);
        return;
    }
    drop(store);
}

/// Borrowing future returned by a storage-adapter factory.
pub type ContractFactoryFuture<'a, S> = Pin<Box<dyn Future<Output = ContractResult<S>> + 'a>>;

pub(crate) fn fixture_modules(
    graph: &LinkedGraph,
    ranking: &RankProjection,
    manifests: &[FileRevision],
) -> ContractResult<ModuleProjection> {
    let policy = ModulePolicyVersion::v1();
    let module_id = ModuleId::from_bytes([201; 32]);
    let kind = if manifests.is_empty() {
        ModuleKind::PathBoundary
    } else {
        ModuleKind::ManifestBoundary
    };
    let mut memberships = graph
        .symbols()
        .iter()
        .map(|symbol| {
            let evidence = manifests.first().map_or_else(
                || ModuleMembershipEvidence::path(symbol.revision().clone()),
                |manifest| {
                    ModuleMembershipEvidence::manifest(symbol.revision().clone(), manifest.clone())
                },
            );
            ModuleMembership::new(module_id, symbol.id(), evidence)
        })
        .collect::<Vec<_>>();
    let ranked = ranking
        .symbols()
        .iter()
        .map(|rank| rank.symbol_id())
        .collect::<Vec<_>>();
    let entrypoints = ranking
        .symbols()
        .iter()
        .filter_map(|rank| {
            graph
                .symbols()
                .binary_search_by_key(&rank.symbol_id(), |symbol| symbol.id())
                .ok()
                .and_then(|position| graph.symbols().get(position))
                .filter(|symbol| symbol.parsed().roles().contains(SymbolRole::Entrypoint))
                .map(|symbol| symbol.id())
        })
        .collect::<Vec<_>>();
    let tests = ranking
        .symbols()
        .iter()
        .filter_map(|rank| {
            graph
                .symbols()
                .binary_search_by_key(&rank.symbol_id(), |symbol| symbol.id())
                .ok()
                .and_then(|position| graph.symbols().get(position))
                .filter(|symbol| symbol.parsed().roles().contains(SymbolRole::Test))
                .map(|symbol| symbol.id())
        })
        .collect::<Vec<_>>();
    let module = RepositoryModule::new(
        module_id,
        kind,
        Some(ModuleRoot::Repository),
        manifests.to_vec(),
        ModuleSymbolSet::new(ranked, false)?,
        ModuleSymbolSet::new(entrypoints.clone(), false)?,
        ModuleSymbolSet::new(tests.clone(), false)?,
    )?;
    let mut modules = vec![module];
    if graph.symbols().len() > 1 {
        let community_id = ModuleId::from_bytes([203; 32]);
        let community_memberships = graph
            .symbols()
            .iter()
            .map(|symbol| {
                let relationship = graph.edges().iter().find(|edge| {
                    !matches!(
                        edge.kind(),
                        SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
                    ) && matches!(
                        (edge.source(), edge.target()),
                        (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target))
                            if source == &symbol.id() || target == &symbol.id()
                    )
                });
                relationship.map(|edge| {
                    ModuleMembershipEvidence::graph(
                        symbol.revision().clone(),
                        vec![edge.evidence().clone()],
                    )
                    .map(|evidence| ModuleMembership::new(community_id, symbol.id(), evidence))
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(|items| items.into_iter().collect::<Result<Vec<_>, _>>())
            .transpose()?;
        if let Some(community_memberships) = community_memberships {
            modules.push(RepositoryModule::new(
                community_id,
                ModuleKind::GraphCommunity,
                None,
                Vec::new(),
                ModuleSymbolSet::new(
                    ranking
                        .symbols()
                        .iter()
                        .map(|rank| rank.symbol_id())
                        .collect(),
                    false,
                )?,
                ModuleSymbolSet::new(entrypoints.clone(), false)?,
                ModuleSymbolSet::new(tests.clone(), false)?,
            )?);
            memberships.extend(community_memberships);
        }
    }
    let card = RepositoryCard::new(
        graph.snapshot_id(),
        policy,
        vec![module_id],
        vec![IndexLanguage::Generic],
        ModuleSymbolSet::new(entrypoints, false)?,
        u32::try_from(graph.files().len())?,
        u32::try_from(graph.symbols().len())?,
    )?;
    Ok(ModuleProjection::new(
        graph.snapshot_id(),
        policy,
        modules,
        memberships,
        card,
    )?)
}

/// Creates a fresh or reopened adapter at a contract-owned app-data root.
///
/// Implementations may translate the generic path into their own validated
/// layout type, but must not change the contract scenarios.
pub trait KnowledgeStoreContractFactory {
    /// Concrete adapter that implements every current storage capability.
    type Store: KnowledgeStore
        + KnowledgeIndexStore
        + GoalContractStore
        + TaskLedgerStore
        + AgentActionStore
        + RunJournalStore
        + KnowledgeSearchStore
        + SemanticEmbeddingStore
        + ModuleRemapQueueStore
        + TaskLensClaimStore
        + TaskLensIndexStore
        + VerifiedModuleCardPublisher;

    /// Opens the store at `app_data_root`, preserving data across repeated calls.
    fn open<'a>(&'a self, app_data_root: &'a Path) -> ContractFactoryFuture<'a, Self::Store>;
}

/// Independently executable adapter-neutral contract group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeStoreContractGroup {
    /// Catalog recency and reopen behavior.
    CatalogRecency,
    /// Catalog identity behavior for linked worktrees.
    CatalogLinkedWorktrees,
    /// Snapshot ordering, identity validation, and worktree isolation.
    IndexSnapshotValidation,
    /// Snapshot state projection and reopen behavior.
    IndexSnapshotReopen,
    /// Index-run sequencing, transitions, and reopen behavior.
    IndexRuns,
    /// Atomic index publication and preservation across reopen.
    IndexPublicationVisibility,
    /// Rejection of a duplicate publication.
    IndexDuplicatePublicationRejection,
    /// Rejection of a mismatched publication and a concurrent rebuild.
    IndexMismatchedPublicationRejection,
    /// Replacement of the visible index by a newer snapshot publication.
    IndexReplacementPublication,
    /// Regenerable-index rebuild and authoritative snapshot retention.
    IndexRebuild,
    /// Verified-only Module Card publication, cancellation, duplicate rejection, and rebuild.
    ModuleCardPublication,
    /// Append-only Goal Contract creation, revision, audit, reopen, and worktree isolation.
    GoalContracts,
    /// Versioned Task Ledger state, attempt history, replans, invalidation, and reopen behavior.
    TaskLedgers,
    /// Atomic append-only run journal, safe export, reopen, and worktree isolation.
    RunJournals,
    /// Retrieval behavior before an index is published.
    SearchAvailability,
    /// Cancellation behavior across all retrieval channels.
    SearchCancellation,
    /// Exact retrieval, pagination, role, path, and injection behavior.
    SearchExact,
    /// Lexical retrieval, pagination, scoring, and injection behavior.
    SearchLexical,
    /// Graph traversal presets, bounds, provenance, and missing seeds.
    SearchGraph,
    /// Cursor invalidation and visibility after replacement and rebuild.
    SearchReplacement,
    /// Semantic-card, profile, vector, and cache behavior.
    Semantic,
    /// Confirmed worktree movement within the same repository.
    ReconciliationWorktreeMove,
    /// Confirmed repository movement using durable evidence.
    ReconciliationRepositoryMove,
    /// Explicitly separate opening of a remote-matched repository.
    ReconciliationSeparateOpen,
}

/// Runs one shared group in a fresh contract-owned workspace.
pub async fn verify_knowledge_store_contract_group<F>(
    factory: &F,
    group: KnowledgeStoreContractGroup,
) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let workspace = fixture::ContractWorkspace::new()?;
    let result = match group {
        KnowledgeStoreContractGroup::CatalogRecency => {
            catalog::verify_recency_and_reopen(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::CatalogLinkedWorktrees => {
            catalog::verify_linked_worktrees(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexSnapshotValidation => {
            index::verify_snapshot_validation(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexSnapshotReopen => {
            index::verify_snapshot_reopen(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexRuns => index::verify_runs(factory, &workspace).await,
        KnowledgeStoreContractGroup::IndexPublicationVisibility => {
            index::verify_publication_visibility(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexDuplicatePublicationRejection => {
            index::verify_duplicate_publication_rejection(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexMismatchedPublicationRejection => {
            index::verify_mismatched_publication_rejection(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexReplacementPublication => {
            index::verify_replacement_publication(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::IndexRebuild => {
            index::verify_rebuild(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::ModuleCardPublication => {
            module_cards::verify(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::GoalContracts => {
            goal_contract::verify(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::TaskLedgers => task_ledger::verify(factory, &workspace).await,
        KnowledgeStoreContractGroup::RunJournals => run_journal::verify(factory, &workspace).await,
        KnowledgeStoreContractGroup::SearchAvailability => {
            search::verify_phase(
                factory,
                &workspace,
                search::SearchContractPhase::Availability,
            )
            .await
        }
        KnowledgeStoreContractGroup::SearchCancellation => {
            search::verify_phase(
                factory,
                &workspace,
                search::SearchContractPhase::Cancellation,
            )
            .await
        }
        KnowledgeStoreContractGroup::SearchExact => {
            search::verify_phase(factory, &workspace, search::SearchContractPhase::Exact).await
        }
        KnowledgeStoreContractGroup::SearchLexical => {
            search::verify_phase(factory, &workspace, search::SearchContractPhase::Lexical).await
        }
        KnowledgeStoreContractGroup::SearchGraph => {
            search::verify_phase(factory, &workspace, search::SearchContractPhase::Graph).await
        }
        KnowledgeStoreContractGroup::SearchReplacement => {
            search::verify_phase(
                factory,
                &workspace,
                search::SearchContractPhase::Replacement,
            )
            .await
        }
        KnowledgeStoreContractGroup::Semantic => semantic::verify(factory, &workspace).await,
        KnowledgeStoreContractGroup::ReconciliationWorktreeMove => {
            reconciliation::verify_confirmed_same_repository_move(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::ReconciliationRepositoryMove => {
            reconciliation::verify_confirmed_repository_move(factory, &workspace).await
        }
        KnowledgeStoreContractGroup::ReconciliationSeparateOpen => {
            reconciliation::verify_remote_match_can_be_opened_separately(factory, &workspace).await
        }
    };
    retain_workspace_during_native_teardown();
    result
}

fn retain_workspace_during_native_teardown() {
    // libSQL's Windows worker teardown can finish just after the adapter drops.
    // Keep its database files alive until that owned teardown has completed.
    #[cfg(windows)]
    if std::env::var_os("A3_STORAGE_CONTRACT_RETAIN_WORKSPACE").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Runs every shared catalog, snapshot, and index-run contract sequentially.
///
/// A fresh temporary workspace is owned for the entire run. Individual
/// scenarios use distinct app-data roots so their durable state cannot leak.
pub async fn verify_knowledge_store_contract<F>(factory: &F) -> ContractResult<()>
where
    F: KnowledgeStoreContractFactory,
{
    let workspace = fixture::ContractWorkspace::new()?;
    catalog::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    index::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    module_cards::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    goal_contract::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    search::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    semantic::verify(factory, &workspace).await?;
    retain_workspace_during_native_teardown();
    let result = reconciliation::verify(factory, &workspace).await;
    retain_workspace_during_native_teardown();
    result
}
