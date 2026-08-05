use super::{GraphEndpoint, LinkedGraph, RankProjection};
use crate::{
    ContentHash, FileRevision, IndexRunRecord, IndexRunStatus, ModuleKind, ModuleMembershipKind,
    ModuleProjection, RepositoryPath, SourceRange, SymbolId, SyntaxRelationKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// Complete deterministic index payload prepared for one atomic publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPublication {
    graph: LinkedGraph,
    ranking: RankProjection,
    manifest_files: Vec<FileRevision>,
    modules: ModuleProjection,
}

impl IndexPublication {
    /// Binds one complete graph to an exact ranking projection.
    pub fn new(
        graph: LinkedGraph,
        ranking: RankProjection,
        mut manifest_files: Vec<FileRevision>,
        modules: ModuleProjection,
    ) -> Result<Self, IndexPublicationError> {
        if graph.snapshot_id() != ranking.snapshot_id() {
            return Err(IndexPublicationError::SnapshotMismatch);
        }

        let graph_symbols = graph
            .symbols()
            .iter()
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();
        let mut ranked_symbols = ranking
            .symbols()
            .iter()
            .map(|rank| rank.symbol_id())
            .collect::<Vec<_>>();
        ranked_symbols.sort();
        if graph_symbols != ranked_symbols {
            return Err(IndexPublicationError::RankingCoverageMismatch);
        }

        manifest_files.sort_by(|left, right| left.path().cmp(right.path()));
        if manifest_files
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(IndexPublicationError::DuplicateManifestPath);
        }
        for manifest in &manifest_files {
            let position = graph
                .files()
                .binary_search_by(|revision| revision.path().cmp(manifest.path()))
                .map_err(|_| IndexPublicationError::ManifestRevisionMissing)?;
            if &graph.files()[position] != manifest {
                return Err(IndexPublicationError::ManifestRevisionMissing);
            }
        }
        validate_modules(&graph, &manifest_files, &modules)?;

        Ok(Self {
            graph,
            ranking,
            manifest_files,
            modules,
        })
    }

    /// Returns the complete snapshot-bound linked graph.
    #[must_use]
    pub const fn graph(&self) -> &LinkedGraph {
        &self.graph
    }

    /// Returns the complete deterministic ranking projection.
    #[must_use]
    pub const fn ranking(&self) -> &RankProjection {
        &self.ranking
    }

    /// Returns discovery-proven manifest files in canonical path order.
    #[must_use]
    pub fn manifest_files(&self) -> &[FileRevision] {
        &self.manifest_files
    }

    /// Returns the complete deterministic module and repository-card projection.
    #[must_use]
    pub const fn modules(&self) -> &ModuleProjection {
        &self.modules
    }
}

fn validate_modules(
    graph: &LinkedGraph,
    manifest_files: &[FileRevision],
    modules: &ModuleProjection,
) -> Result<(), IndexPublicationError> {
    if modules.snapshot_id() != graph.snapshot_id() {
        return Err(IndexPublicationError::ModuleSnapshotMismatch);
    }
    if modules.repository_card().file_count()
        != u32::try_from(graph.files().len())
            .map_err(|_| IndexPublicationError::ModuleCoverageMismatch)?
        || modules.repository_card().symbol_count()
            != u32::try_from(graph.symbols().len())
                .map_err(|_| IndexPublicationError::ModuleCoverageMismatch)?
    {
        return Err(IndexPublicationError::ModuleCoverageMismatch);
    }
    let files = graph
        .files()
        .iter()
        .map(|revision| (revision.path().clone(), revision.content_hash()))
        .collect::<BTreeMap<_, _>>();
    let manifests = manifest_files
        .iter()
        .map(|revision| (revision.path().clone(), revision.content_hash()))
        .collect::<BTreeMap<_, _>>();
    for module in modules.modules() {
        if module.kind() == ModuleKind::ManifestBoundary
            && module.manifests().iter().any(|manifest| {
                manifests.get(manifest.path()).copied() != Some(manifest.content_hash())
            })
        {
            return Err(IndexPublicationError::ModuleEvidenceMissing);
        }
    }
    let symbols = graph
        .symbols()
        .iter()
        .map(|symbol| (symbol.id(), symbol.revision()))
        .collect::<BTreeMap<_, _>>();
    let projected_symbols = modules
        .memberships()
        .iter()
        .map(|membership| membership.symbol_id())
        .collect::<BTreeSet<_>>();
    if projected_symbols != symbols.keys().copied().collect() {
        return Err(IndexPublicationError::ModuleCoverageMismatch);
    }
    let membership_pairs = modules
        .memberships()
        .iter()
        .map(|membership| (membership.module_id(), membership.symbol_id()))
        .collect::<BTreeSet<_>>();
    let mut graph_evidence =
        BTreeMap::<(RepositoryPath, ContentHash, SourceRange), Vec<(SymbolId, SymbolId)>>::new();
    for edge in graph.edges() {
        if matches!(
            edge.kind(),
            SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
        ) {
            continue;
        }
        if let (GraphEndpoint::Symbol(source), GraphEndpoint::Symbol(target)) =
            (edge.source(), edge.target())
        {
            let evidence = edge.evidence();
            graph_evidence
                .entry((
                    evidence.revision().path().clone(),
                    evidence.revision().content_hash(),
                    evidence.range(),
                ))
                .or_default()
                .push((*source, *target));
        }
    }
    for membership in modules.memberships() {
        let evidence = membership.evidence();
        if symbols.get(&membership.symbol_id()).copied() != Some(evidence.member_revision())
            || files.get(evidence.member_revision().path()).copied()
                != Some(evidence.member_revision().content_hash())
        {
            return Err(IndexPublicationError::ModuleEvidenceMissing);
        }
        if let Some(manifest) = evidence.manifest_revision()
            && manifests.get(manifest.path()).copied() != Some(manifest.content_hash())
        {
            return Err(IndexPublicationError::ModuleEvidenceMissing);
        }
        if evidence.kind() == ModuleMembershipKind::GraphCommunity {
            for relationship in evidence.relationships() {
                let key = (
                    relationship.revision().path().clone(),
                    relationship.revision().content_hash(),
                    relationship.range(),
                );
                let supports_membership = graph_evidence.get(&key).is_some_and(|edges| {
                    edges.iter().any(|(source, target)| {
                        (*source == membership.symbol_id()
                            && membership_pairs.contains(&(membership.module_id(), *target)))
                            || (*target == membership.symbol_id()
                                && membership_pairs.contains(&(membership.module_id(), *source)))
                    })
                });
                if !supports_membership {
                    return Err(IndexPublicationError::ModuleEvidenceMissing);
                }
            }
        }
    }
    Ok(())
}

/// One fully reconstructed and atomically visible deterministic index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedIndex {
    run: IndexRunRecord,
    publication: IndexPublication,
}

impl PublishedIndex {
    /// Binds a published run record to its exact graph and ranking payload.
    pub fn new(
        run: IndexRunRecord,
        publication: IndexPublication,
    ) -> Result<Self, IndexPublicationError> {
        if run.status() != IndexRunStatus::Published {
            return Err(IndexPublicationError::RunNotPublished);
        }
        if run.snapshot_id() != publication.graph().snapshot_id() {
            return Err(IndexPublicationError::RunSnapshotMismatch);
        }
        if run.ranking_policy_version() != publication.ranking().policy_version() {
            return Err(IndexPublicationError::RunRankingPolicyMismatch);
        }
        Ok(Self { run, publication })
    }

    /// Returns the visible run and its durable ordering information.
    #[must_use]
    pub const fn run(&self) -> IndexRunRecord {
        self.run
    }

    /// Returns the exact graph and ranking payload committed with the run.
    #[must_use]
    pub const fn publication(&self) -> &IndexPublication {
        &self.publication
    }
}

/// Invalid relationship between graph, ranking, and visible run metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPublicationError {
    /// Graph and ranking refer to different immutable snapshots.
    SnapshotMismatch,
    /// Ranking rows do not cover exactly the graph symbols.
    RankingCoverageMismatch,
    /// Two manifest entries targeted the same normalized path.
    DuplicateManifestPath,
    /// A manifest was not the exact current revision in the linked graph.
    ManifestRevisionMissing,
    /// Module formation used a different immutable graph snapshot.
    ModuleSnapshotMismatch,
    /// Module memberships or repository-card counts do not cover the graph exactly.
    ModuleCoverageMismatch,
    /// Module membership or boundary evidence is absent or stale.
    ModuleEvidenceMissing,
    /// A reconstructed visible index did not have published run state.
    RunNotPublished,
    /// The run and graph refer to different snapshots.
    RunSnapshotMismatch,
    /// The run and rank projection use different ranking policies.
    RunRankingPolicyMismatch,
}

impl fmt::Display for IndexPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::SnapshotMismatch => "graph and ranking snapshots differ",
            Self::RankingCoverageMismatch => "ranking does not cover exactly the graph symbols",
            Self::DuplicateManifestPath => "manifest projection contains a duplicate path",
            Self::ManifestRevisionMissing => {
                "manifest projection does not reference a current graph file revision"
            }
            Self::ModuleSnapshotMismatch => "module projection belongs to another graph snapshot",
            Self::ModuleCoverageMismatch => "module projection does not cover the graph exactly",
            Self::ModuleEvidenceMissing => "module projection evidence is stale or absent",
            Self::RunNotPublished => "visible index run is not published",
            Self::RunSnapshotMismatch => "published run and graph snapshots differ",
            Self::RunRankingPolicyMismatch => "published run and ranking policies differ",
        };
        formatter.write_str(message)
    }
}

impl Error for IndexPublicationError {}

#[cfg(test)]
mod tests {
    use super::{IndexPublication, IndexPublicationError, PublishedIndex};
    use crate::{
        Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
        GraphSymbol, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkResolution,
        LinkedGraph, LocalSymbolId, ModuleId, ModuleKind, ModuleMembership,
        ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot,
        ModuleSymbolSet, ParsedSymbol, RankProjection, RankScore, RankingPolicyVersion,
        RepositoryCard, RepositoryModule, RepositoryPath, SnapshotId, SourcePosition, SourceRange,
        SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SyntaxProvider,
        SyntaxRelationKind,
    };

    #[test]
    fn empty_publication_requires_matching_snapshot_and_published_run()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let publication =
            IndexPublication::new(graph, ranking, Vec::new(), empty_modules(snapshot_id, 0)?)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([2; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );

        assert_eq!(PublishedIndex::new(run, publication)?.run(), run);
        Ok(())
    }

    #[test]
    fn mismatched_snapshot_is_rejected_before_storage() -> Result<(), Box<dyn std::error::Error>> {
        let graph = LinkedGraph::new(
            SnapshotId::from_bytes([1; 32]),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = RankProjection::new(
            SnapshotId::from_bytes([2; 32]),
            RankingPolicyVersion::v1(),
            Vec::new(),
        )?;

        assert_eq!(
            IndexPublication::new(
                graph,
                ranking,
                Vec::new(),
                empty_modules(SnapshotId::from_bytes([1; 32]), 0)?,
            ),
            Err(IndexPublicationError::SnapshotMismatch)
        );
        Ok(())
    }

    #[test]
    fn ranking_must_cover_exactly_the_graph_symbols() -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([3; 32]);
        let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(
            snapshot_id,
            RankingPolicyVersion::v1(),
            vec![SymbolRank::new(
                SymbolId::from_bytes([4; 32]),
                RankScore::try_from_sum(0)?,
                SymbolRankSignals {
                    in_degree: 0,
                    out_degree: 0,
                    centrality: Centrality::from_basis_points(0)?,
                    degree_contribution: 0,
                    centrality_contribution: 0,
                    entrypoint_contribution: 0,
                    public_export_contribution: 0,
                    manifest_contribution: 0,
                    test_contribution: 0,
                },
            )],
        )?;

        assert_eq!(
            IndexPublication::new(graph, ranking, Vec::new(), empty_modules(snapshot_id, 0)?,),
            Err(IndexPublicationError::RankingCoverageMismatch)
        );
        Ok(())
    }

    #[test]
    fn manifest_projection_requires_a_current_graph_revision()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([5; 32]);
        let manifest = crate::FileRevision::new(
            crate::RepositoryPath::try_from_bytes(b"Cargo.toml".to_vec())?,
            crate::ContentHash::from_bytes([6; 32]),
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![manifest.clone()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let publication = IndexPublication::new(
            graph,
            ranking,
            vec![manifest.clone()],
            empty_modules(snapshot_id, 1)?,
        )?;
        assert_eq!(publication.manifest_files(), &[manifest]);
        Ok(())
    }

    #[test]
    fn structural_edges_cannot_prove_graph_community_membership()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([7; 32]);
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([8; 32]),
        );
        let first_id = SymbolId::from_bytes([9; 32]);
        let second_id = SymbolId::from_bytes([10; 32]);
        let first = graph_symbol(first_id, revision.clone(), 1, "first")?;
        let second = graph_symbol(second_id, revision.clone(), 2, "second")?;
        let evidence = EvidenceRef::new(revision.clone(), source_range()?);
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![revision.clone()],
            vec![first, second],
            vec![GraphEdge::new(
                GraphEndpoint::Symbol(first_id),
                GraphEndpoint::Symbol(second_id),
                SyntaxRelationKind::Contains,
                SyntaxProvider::TreeSitter,
                Confidence::from_basis_points(10_000)?,
                LinkResolution::AdapterLocalSymbol,
                snapshot_id,
                evidence.clone(),
            )],
            Vec::new(),
        )?;
        let ranking = RankProjection::new(
            snapshot_id,
            RankingPolicyVersion::v1(),
            [first_id, second_id]
                .into_iter()
                .map(|symbol_id| {
                    Ok(SymbolRank::new(
                        symbol_id,
                        RankScore::try_from_sum(0)?,
                        SymbolRankSignals {
                            in_degree: 0,
                            out_degree: 0,
                            centrality: Centrality::from_basis_points(0)?,
                            degree_contribution: 0,
                            centrality_contribution: 0,
                            entrypoint_contribution: 0,
                            public_export_contribution: 0,
                            manifest_contribution: 0,
                            test_contribution: 0,
                        },
                    ))
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
        )?;
        let primary_id = ModuleId::from_bytes([11; 32]);
        let community_id = ModuleId::from_bytes([12; 32]);
        let modules = vec![
            RepositoryModule::new(
                primary_id,
                ModuleKind::PathBoundary,
                Some(ModuleRoot::Repository),
                Vec::new(),
                ModuleSymbolSet::empty(),
                ModuleSymbolSet::empty(),
                ModuleSymbolSet::empty(),
            )?,
            RepositoryModule::new(
                community_id,
                ModuleKind::GraphCommunity,
                None,
                Vec::new(),
                ModuleSymbolSet::empty(),
                ModuleSymbolSet::empty(),
                ModuleSymbolSet::empty(),
            )?,
        ];
        let mut memberships = Vec::new();
        for symbol_id in [first_id, second_id] {
            memberships.push(ModuleMembership::new(
                primary_id,
                symbol_id,
                ModuleMembershipEvidence::path(revision.clone()),
            ));
            memberships.push(ModuleMembership::new(
                community_id,
                symbol_id,
                ModuleMembershipEvidence::graph(revision.clone(), vec![evidence.clone()])?,
            ));
        }
        let policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            snapshot_id,
            policy,
            vec![primary_id],
            Vec::new(),
            ModuleSymbolSet::empty(),
            1,
            2,
        )?;
        let projection = ModuleProjection::new(snapshot_id, policy, modules, memberships, card)?;

        assert_eq!(
            IndexPublication::new(graph, ranking, Vec::new(), projection),
            Err(IndexPublicationError::ModuleEvidenceMissing)
        );
        Ok(())
    }

    fn empty_modules(
        snapshot_id: SnapshotId,
        file_count: u32,
    ) -> Result<ModuleProjection, Box<dyn std::error::Error>> {
        let policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            snapshot_id,
            policy,
            Vec::new(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            file_count,
            0,
        )?;
        Ok(ModuleProjection::new(
            snapshot_id,
            policy,
            Vec::new(),
            Vec::new(),
            card,
        )?)
    }

    fn graph_symbol(
        id: SymbolId,
        revision: FileRevision,
        local_id: u32,
        name: &str,
    ) -> Result<GraphSymbol, Box<dyn std::error::Error>> {
        let range = source_range()?;
        Ok(GraphSymbol::new(
            id,
            revision,
            ParsedSymbol::new(
                LocalSymbolId::new(local_id)?,
                SymbolKind::Function,
                SymbolName::try_from_string(name.to_owned())?,
                range,
                range,
            )?,
        ))
    }

    fn source_range() -> Result<SourceRange, Box<dyn std::error::Error>> {
        Ok(SourceRange::new(
            0,
            1,
            SourcePosition::new(0, 0),
            SourcePosition::new(0, 1),
        )?)
    }
}
