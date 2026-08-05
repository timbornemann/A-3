use super::SymbolId;
use crate::{RankingPolicyVersion, SnapshotId};
use std::error::Error;
use std::fmt;

/// Deterministic fixed-point module centrality in basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Centrality(u16);

impl Centrality {
    /// Creates centrality between zero and 10,000 basis points.
    pub fn from_basis_points(value: u16) -> Result<Self, CentralityError> {
        if value > 10_000 {
            return Err(CentralityError(value));
        }
        Ok(Self(value))
    }

    /// Returns the stable fixed-point representation.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

/// Centrality exceeded 100 percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CentralityError(u16);

impl fmt::Display for CentralityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "centrality {} exceeds 10,000 basis points",
            self.0
        )
    }
}

impl Error for CentralityError {}

/// Bounded deterministic ranking score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RankScore(u32);

impl RankScore {
    /// Converts an exact weighted sum without silent truncation.
    pub fn try_from_sum(value: u64) -> Result<Self, RankScoreError> {
        let value = u32::try_from(value).map_err(|_| RankScoreError(value))?;
        Ok(Self(value))
    }

    /// Returns the stable integer score.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A weighted rank cannot be represented by the durable score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankScoreError(u64);

impl fmt::Display for RankScoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "rank score {} is too large", self.0)
    }
}

impl Error for RankScoreError {}

/// Explainable components of one symbol's deterministic rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRankSignals {
    /// Number of resolved incoming graph edges.
    pub in_degree: u32,
    /// Number of resolved outgoing graph edges.
    pub out_degree: u32,
    /// Degree-derived fixed-point module centrality.
    pub centrality: Centrality,
    /// Score contributed by exact incoming and outgoing graph degree.
    pub degree_contribution: u32,
    /// Score contributed by normalized module centrality.
    pub centrality_contribution: u32,
    /// Score contributed by an adapter-observed entrypoint role.
    pub entrypoint_contribution: u32,
    /// Score contributed by public visibility or export evidence.
    pub public_export_contribution: u32,
    /// Score contributed by manifest/config/build proximity.
    pub manifest_contribution: u32,
    /// Score contributed by test roles or test relations.
    pub test_contribution: u32,
}

/// One explainable rank row for a stable graph symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRank {
    symbol_id: SymbolId,
    score: RankScore,
    signals: SymbolRankSignals,
}

impl SymbolRank {
    /// Creates one rank row from an exact weighted score and its inputs.
    #[must_use]
    pub const fn new(symbol_id: SymbolId, score: RankScore, signals: SymbolRankSignals) -> Self {
        Self {
            symbol_id,
            score,
            signals,
        }
    }

    /// Returns the ranked symbol.
    #[must_use]
    pub const fn symbol_id(self) -> SymbolId {
        self.symbol_id
    }

    /// Returns the deterministic weighted score.
    #[must_use]
    pub const fn score(self) -> RankScore {
        self.score
    }

    /// Returns the explainable raw signals and contributions.
    #[must_use]
    pub const fn signals(self) -> SymbolRankSignals {
        self.signals
    }
}

/// Ranking-only projection over an already linked graph snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankProjection {
    snapshot_id: SnapshotId,
    policy_version: RankingPolicyVersion,
    symbols: Vec<SymbolRank>,
}

impl RankProjection {
    /// Canonicalizes descending score order and rejects duplicate symbol rows.
    pub fn new(
        snapshot_id: SnapshotId,
        policy_version: RankingPolicyVersion,
        mut symbols: Vec<SymbolRank>,
    ) -> Result<Self, RankProjectionError> {
        symbols.sort_by(|left, right| {
            right
                .score()
                .cmp(&left.score())
                .then_with(|| left.symbol_id().cmp(&right.symbol_id()))
        });
        let mut identities = symbols
            .iter()
            .map(|rank| rank.symbol_id())
            .collect::<Vec<_>>();
        identities.sort();
        if identities.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RankProjectionError::DuplicateSymbol);
        }
        Ok(Self {
            snapshot_id,
            policy_version,
            symbols,
        })
    }

    /// Returns the immutable graph snapshot being ranked.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the algorithm and weighting revision.
    #[must_use]
    pub const fn policy_version(&self) -> RankingPolicyVersion {
        self.policy_version
    }

    /// Returns descending score order with stable ID tie-breaking.
    #[must_use]
    pub fn symbols(&self) -> &[SymbolRank] {
        &self.symbols
    }
}

/// Invalid ranking projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankProjectionError {
    /// More than one row ranked the same symbol.
    DuplicateSymbol,
}

impl fmt::Display for RankProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("rank projection contains a duplicate symbol")
    }
}

impl Error for RankProjectionError {}
