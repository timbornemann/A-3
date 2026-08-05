//! Reusable, adapter-neutral contracts for A^3 storage ports.
//!
//! This crate is dev-only. It depends on application and domain contracts, never
//! on a storage engine, so every adapter can execute the exact same behavior.

mod catalog;
mod fixture;
mod index;
mod reconciliation;
mod search;
mod semantic;

use a3_application::{
    KnowledgeIndexStore, KnowledgeSearchStore, KnowledgeStore, SemanticEmbeddingStore,
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
    type Store: KnowledgeStore + KnowledgeIndexStore + KnowledgeSearchStore + SemanticEmbeddingStore;

    /// Opens the store at `app_data_root`, preserving data across repeated calls.
    fn open<'a>(&'a self, app_data_root: &'a Path) -> ContractFactoryFuture<'a, Self::Store>;
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
    index::verify(factory, &workspace).await?;
    search::verify(factory, &workspace).await?;
    semantic::verify(factory, &workspace).await?;
    reconciliation::verify(factory, &workspace).await
}
