use super::{LinkedGraph, RankProjection};
use crate::{IndexRunRecord, IndexRunStatus};
use std::error::Error;
use std::fmt;

/// Complete deterministic index payload prepared for one atomic publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPublication {
    graph: LinkedGraph,
    ranking: RankProjection,
}

impl IndexPublication {
    /// Binds one complete graph to an exact ranking projection.
    pub fn new(graph: LinkedGraph, ranking: RankProjection) -> Result<Self, IndexPublicationError> {
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

        Ok(Self { graph, ranking })
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
        Centrality, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkedGraph,
        RankProjection, RankScore, RankingPolicyVersion, SnapshotId, SymbolId, SymbolRank,
        SymbolRankSignals,
    };

    #[test]
    fn empty_publication_requires_matching_snapshot_and_published_run()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let graph = LinkedGraph::new(snapshot_id, Vec::new(), Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let publication = IndexPublication::new(graph, ranking)?;
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
            IndexPublication::new(graph, ranking),
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
            IndexPublication::new(graph, ranking),
            Err(IndexPublicationError::RankingCoverageMismatch)
        );
        Ok(())
    }
}
