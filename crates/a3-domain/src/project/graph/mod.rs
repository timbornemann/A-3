mod id;
mod publication;
mod rank;
mod relation;
mod result;
mod symbol;

pub use id::SymbolId;
pub use publication::{IndexPublication, IndexPublicationError, PublishedIndex};
pub use rank::{
    Centrality, CentralityError, RankProjection, RankProjectionError, RankScore, RankScoreError,
    SymbolRank, SymbolRankSignals,
};
pub use relation::{
    EvidenceRef, GraphEdge, GraphEndpoint, LinkResolution, UnresolvedEdgeCandidate,
    UnresolvedGraphTarget, UnresolvedReason,
};
pub use result::{LinkedGraph, LinkedGraphError};
pub use symbol::GraphSymbol;

#[cfg(test)]
mod tests {
    use super::{
        EvidenceRef, GraphEdge, GraphEndpoint, GraphSymbol, LinkResolution, LinkedGraph,
        LinkedGraphError, RankProjection, RankProjectionError, RankScore, SymbolId, SymbolRank,
        SymbolRankSignals,
    };
    use crate::{
        Centrality, Confidence, ContentHash, FileRevision, LocalSymbolId, ParsedSymbol,
        RankingPolicyVersion, RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolKind,
        SymbolName, SyntaxProvider, SyntaxRelationKind,
    };

    #[test]
    fn graph_rejects_stale_or_cross_file_evidence() -> Result<(), Box<dyn std::error::Error>> {
        let first = revision("src/a.rs", 1)?;
        let second = revision("src/b.rs", 2)?;
        let first_symbol = symbol(SymbolId::from_bytes([3; 32]), first.clone(), "a")?;
        let second_symbol = symbol(SymbolId::from_bytes([4; 32]), second.clone(), "b")?;
        let snapshot = SnapshotId::from_bytes([5; 32]);
        let edge = GraphEdge::new(
            GraphEndpoint::Symbol(first_symbol.id()),
            GraphEndpoint::Symbol(second_symbol.id()),
            SyntaxRelationKind::Calls,
            SyntaxProvider::TreeSitter,
            Confidence::from_basis_points(7_500)?,
            LinkResolution::UniqueQualifiedName,
            snapshot,
            EvidenceRef::new(second.clone(), range()?),
        );
        assert_eq!(
            LinkedGraph::new(
                snapshot,
                vec![first.clone(), second.clone()],
                vec![first_symbol.clone(), second_symbol.clone()],
                vec![edge],
                Vec::new(),
            ),
            Err(LinkedGraphError::EvidenceSourceMismatch)
        );

        let stale_edge = GraphEdge::new(
            GraphEndpoint::Symbol(first_symbol.id()),
            GraphEndpoint::Symbol(second_symbol.id()),
            SyntaxRelationKind::Calls,
            SyntaxProvider::TreeSitter,
            Confidence::from_basis_points(7_500)?,
            LinkResolution::UniqueQualifiedName,
            snapshot,
            EvidenceRef::new(
                FileRevision::new(first.path().clone(), ContentHash::from_bytes([9; 32])),
                range()?,
            ),
        );
        assert_eq!(
            LinkedGraph::new(
                snapshot,
                vec![first, second],
                vec![first_symbol, second_symbol],
                vec![stale_edge],
                Vec::new(),
            ),
            Err(LinkedGraphError::EvidenceRevisionMissing)
        );
        Ok(())
    }

    #[test]
    fn rank_projection_is_stable_and_rejects_duplicate_symbols()
    -> Result<(), Box<dyn std::error::Error>> {
        let id = SymbolId::from_bytes([7; 32]);
        let signals = SymbolRankSignals {
            in_degree: 1,
            out_degree: 2,
            centrality: Centrality::from_basis_points(5_000)?,
            degree_contribution: 400,
            centrality_contribution: 1_500,
            entrypoint_contribution: 0,
            public_export_contribution: 2_000,
            manifest_contribution: 0,
            test_contribution: 0,
        };
        let row = SymbolRank::new(id, RankScore::try_from_sum(3_900)?, signals);
        assert_eq!(
            RankProjection::new(
                SnapshotId::from_bytes([8; 32]),
                RankingPolicyVersion::v1(),
                vec![row, row],
            ),
            Err(RankProjectionError::DuplicateSymbol)
        );
        assert_eq!(
            id.to_string(),
            "0707070707070707070707070707070707070707070707070707070707070707"
        );
        Ok(())
    }

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    fn symbol(
        id: SymbolId,
        revision: FileRevision,
        name: &str,
    ) -> Result<GraphSymbol, Box<dyn std::error::Error>> {
        let range = range()?;
        Ok(GraphSymbol::new(
            id,
            revision,
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string(name.to_owned())?,
                range,
                range,
            )?,
        ))
    }

    fn range() -> Result<SourceRange, Box<dyn std::error::Error>> {
        Ok(SourceRange::new(
            0,
            1,
            SourcePosition::new(0, 0),
            SourcePosition::new(0, 1),
        )?)
    }
}
