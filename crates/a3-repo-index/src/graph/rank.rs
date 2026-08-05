use super::control::{GraphComputationControl, GraphComputationControlError};
use a3_domain::{
    Centrality, GraphEndpoint, LinkedGraph, Progress, RankProjection, RankScore,
    RankingPolicyVersion, RepositoryPath, SymbolId, SymbolRank, SymbolRankSignals, SymbolRole,
    SymbolVisibility, SyntaxProvider, SyntaxRelationKind,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const WORK_POLL_INTERVAL: usize = 1_024;

/// Versioned deterministic graph-ranking policy independent of parse artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankingPolicy {
    version: RankingPolicyVersion,
    entrypoint_weight: u32,
    public_export_weight: u32,
    manifest_weight: u32,
    test_weight: u32,
    incoming_degree_weight: u32,
    outgoing_degree_weight: u32,
    degree_contribution_limit: u32,
    centrality_weight_basis_points: u32,
    timeout: Duration,
    max_symbols: usize,
    max_edges: usize,
    max_progress_events: usize,
}

impl RankingPolicy {
    /// Returns ranking policy V1 with fixed integer weights and no floating-point drift.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: RankingPolicyVersion::v1(),
            entrypoint_weight: 5_000,
            public_export_weight: 2_000,
            manifest_weight: 1_500,
            test_weight: 1_500,
            incoming_degree_weight: 200,
            outgoing_degree_weight: 100,
            degree_contribution_limit: 4_000,
            centrality_weight_basis_points: 3_000,
            timeout: Duration::from_secs(5),
            max_symbols: 1_000_000,
            max_edges: 2_000_000,
            max_progress_events: 64,
        }
    }

    /// Returns the durable algorithm and weighting revision.
    #[must_use]
    pub const fn version(self) -> RankingPolicyVersion {
        self.version
    }

    /// Returns the wall-clock budget for one ranking pass.
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Returns the maximum graph symbols accepted by this policy.
    #[must_use]
    pub const fn max_symbols(self) -> usize {
        self.max_symbols
    }

    /// Returns the maximum resolved graph edges accepted by this policy.
    #[must_use]
    pub const fn max_edges(self) -> usize {
        self.max_edges
    }

    /// Returns the maximum progress notifications.
    #[must_use]
    pub const fn max_progress_events(self) -> usize {
        self.max_progress_events
    }
}

impl Default for RankingPolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// Stateless deterministic ranker operating only on a linked graph.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicGraphRanker;

impl DeterministicGraphRanker {
    /// Computes explainable entrypoint, relationship, test, manifest, and centrality signals.
    pub fn rank(
        &self,
        graph: &LinkedGraph,
        policy: RankingPolicy,
        control: &dyn GraphComputationControl,
    ) -> Result<RankProjection, GraphRankFailure> {
        if graph.symbols().len() > policy.max_symbols() || graph.edges().len() > policy.max_edges()
        {
            return Err(GraphRankFailure::ResourceLimitExceeded);
        }
        let started = Instant::now();
        ensure_active(control, started, policy.timeout())?;
        let total_work = graph
            .symbols()
            .len()
            .checked_mul(2)
            .and_then(|value| value.checked_add(graph.edges().len()))
            .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
        let mut progress = ProgressReporter::new(total_work, policy.max_progress_events());
        progress.report(control, 0)?;

        let mut state = BTreeMap::<SymbolId, RankState>::new();
        let mut symbol_files = BTreeMap::<SymbolId, RepositoryPath>::new();
        let mut completed = 0usize;
        for symbol in graph.symbols() {
            poll_work(completed, control, started, policy.timeout())?;
            state.insert(
                symbol.id(),
                RankState {
                    entrypoint: symbol.parsed().roles().contains(SymbolRole::Entrypoint),
                    public_export: symbol.parsed().visibility() == SymbolVisibility::Public,
                    test_related: symbol.parsed().roles().contains(SymbolRole::Test),
                    ..RankState::default()
                },
            );
            symbol_files.insert(symbol.id(), symbol.revision().path().clone());
            completed = completed
                .checked_add(1)
                .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
            progress.maybe_report(control, completed)?;
        }

        let mut file_roots = BTreeMap::<RepositoryPath, SymbolId>::new();
        for edge in graph.edges() {
            if edge.kind() != SyntaxRelationKind::Defines {
                continue;
            }
            if let (GraphEndpoint::File(path), GraphEndpoint::Symbol(id)) =
                (edge.source(), edge.target())
                && file_roots.insert(path.clone(), *id).is_some()
            {
                return Err(GraphRankFailure::InvalidGraph);
            }
        }
        let mut module_degree = file_roots
            .values()
            .map(|id| (*id, 0u32))
            .collect::<BTreeMap<_, _>>();

        for edge in graph.edges() {
            poll_work(completed, control, started, policy.timeout())?;
            if let GraphEndpoint::Symbol(id) = edge.source() {
                let source = state.get_mut(id).ok_or(GraphRankFailure::InvalidGraph)?;
                source.out_degree = source
                    .out_degree
                    .checked_add(1)
                    .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
                source.manifest_related |= edge.provider() == SyntaxProvider::Manifest
                    || matches!(
                        edge.kind(),
                        SyntaxRelationKind::Builds | SyntaxRelationKind::Configures
                    );
                source.test_related |= edge.kind() == SyntaxRelationKind::Tests;
            }
            if let GraphEndpoint::Symbol(id) = edge.target() {
                let target = state.get_mut(id).ok_or(GraphRankFailure::InvalidGraph)?;
                target.in_degree = target
                    .in_degree
                    .checked_add(1)
                    .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
                target.public_export |= edge.kind() == SyntaxRelationKind::Exports;
                target.manifest_related |= edge.provider() == SyntaxProvider::Manifest
                    || matches!(
                        edge.kind(),
                        SyntaxRelationKind::Builds | SyntaxRelationKind::Configures
                    );
                target.test_related |= edge.kind() == SyntaxRelationKind::Tests;
            }
            if !matches!(
                edge.kind(),
                SyntaxRelationKind::Contains | SyntaxRelationKind::Defines
            ) {
                let source_module = endpoint_module(edge.source(), &symbol_files, &file_roots);
                let target_module = endpoint_module(edge.target(), &symbol_files, &file_roots);
                if let (Some(source_module), Some(target_module)) = (source_module, target_module)
                    && source_module != target_module
                {
                    increment_module_degree(&mut module_degree, source_module)?;
                    increment_module_degree(&mut module_degree, target_module)?;
                }
            }
            completed = completed
                .checked_add(1)
                .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
            progress.maybe_report(control, completed)?;
        }

        let maximum_module_degree = module_degree.values().copied().max().unwrap_or(0);
        let mut ranks = Vec::with_capacity(graph.symbols().len());
        for symbol in graph.symbols() {
            poll_work(completed, control, started, policy.timeout())?;
            let item = state
                .get(&symbol.id())
                .copied()
                .ok_or(GraphRankFailure::InvalidGraph)?;
            let module = symbol_files
                .get(&symbol.id())
                .and_then(|path| file_roots.get(path))
                .copied();
            let degree = module
                .and_then(|id| module_degree.get(&id))
                .copied()
                .unwrap_or(0);
            let centrality_basis_points = if maximum_module_degree == 0 {
                0
            } else {
                u16::try_from((u64::from(degree) * 10_000) / u64::from(maximum_module_degree))
                    .map_err(|_| GraphRankFailure::InvalidProjection)?
            };
            let centrality = Centrality::from_basis_points(centrality_basis_points)
                .map_err(|_| GraphRankFailure::InvalidProjection)?;
            let degree_contribution = item
                .in_degree
                .checked_mul(policy.incoming_degree_weight)
                .and_then(|value| {
                    item.out_degree
                        .checked_mul(policy.outgoing_degree_weight)
                        .and_then(|outgoing| value.checked_add(outgoing))
                })
                .ok_or(GraphRankFailure::ResourceLimitExceeded)?
                .min(policy.degree_contribution_limit);
            let centrality_contribution = u32::from(centrality_basis_points)
                .checked_mul(policy.centrality_weight_basis_points)
                .ok_or(GraphRankFailure::ResourceLimitExceeded)?
                / 10_000;
            let signals = SymbolRankSignals {
                in_degree: item.in_degree,
                out_degree: item.out_degree,
                centrality,
                degree_contribution,
                centrality_contribution,
                entrypoint_contribution: if item.entrypoint {
                    policy.entrypoint_weight
                } else {
                    0
                },
                public_export_contribution: if item.public_export {
                    policy.public_export_weight
                } else {
                    0
                },
                manifest_contribution: if item.manifest_related {
                    policy.manifest_weight
                } else {
                    0
                },
                test_contribution: if item.test_related {
                    policy.test_weight
                } else {
                    0
                },
            };
            let score = RankScore::try_from_sum(
                u64::from(signals.degree_contribution)
                    + u64::from(signals.centrality_contribution)
                    + u64::from(signals.entrypoint_contribution)
                    + u64::from(signals.public_export_contribution)
                    + u64::from(signals.manifest_contribution)
                    + u64::from(signals.test_contribution),
            )
            .map_err(|_| GraphRankFailure::InvalidProjection)?;
            ranks.push(SymbolRank::new(symbol.id(), score, signals));
            completed = completed
                .checked_add(1)
                .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
            progress.maybe_report(control, completed)?;
        }
        progress.report(control, total_work)?;
        ensure_active(control, started, policy.timeout())?;
        RankProjection::new(graph.snapshot_id(), policy.version(), ranks)
            .map_err(|_| GraphRankFailure::InvalidProjection)
    }
}

fn endpoint_module(
    endpoint: &GraphEndpoint,
    symbol_files: &BTreeMap<SymbolId, RepositoryPath>,
    file_roots: &BTreeMap<RepositoryPath, SymbolId>,
) -> Option<SymbolId> {
    let path = match endpoint {
        GraphEndpoint::File(path) => Some(path),
        GraphEndpoint::Symbol(id) => symbol_files.get(id),
    }?;
    file_roots.get(path).copied()
}

fn increment_module_degree(
    degrees: &mut BTreeMap<SymbolId, u32>,
    id: SymbolId,
) -> Result<(), GraphRankFailure> {
    let degree = degrees.get_mut(&id).ok_or(GraphRankFailure::InvalidGraph)?;
    *degree = degree
        .checked_add(1)
        .ok_or(GraphRankFailure::ResourceLimitExceeded)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct RankState {
    in_degree: u32,
    out_degree: u32,
    entrypoint: bool,
    public_export: bool,
    manifest_related: bool,
    test_related: bool,
}

fn poll_work(
    completed: usize,
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), GraphRankFailure> {
    if completed.is_multiple_of(WORK_POLL_INTERVAL) {
        ensure_active(control, started, timeout)?;
    }
    Ok(())
}

fn ensure_active(
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), GraphRankFailure> {
    if control.is_cancelled() {
        return Err(GraphRankFailure::Cancelled);
    }
    if started.elapsed() > timeout {
        return Err(GraphRankFailure::TimedOut);
    }
    Ok(())
}

struct ProgressReporter {
    total: usize,
    interval: usize,
    last: Option<usize>,
}

impl ProgressReporter {
    fn new(total: usize, maximum_events: usize) -> Self {
        let events = maximum_events.max(2).saturating_sub(1);
        let interval = total.saturating_add(events.saturating_sub(1)) / events;
        Self {
            total,
            interval: interval.max(1),
            last: None,
        }
    }

    fn maybe_report(
        &mut self,
        control: &dyn GraphComputationControl,
        completed: usize,
    ) -> Result<(), GraphRankFailure> {
        if completed == self.total || completed.is_multiple_of(self.interval) {
            self.report(control, completed)?;
        }
        Ok(())
    }

    fn report(
        &mut self,
        control: &dyn GraphComputationControl,
        completed: usize,
    ) -> Result<(), GraphRankFailure> {
        if self.last == Some(completed) {
            return Ok(());
        }
        let reported_total = self.total.max(1);
        let reported_completed = if self.total == 0 { 1 } else { completed };
        let progress = Progress::determinate(reported_completed as u64, reported_total as u64)
            .map_err(|_| GraphRankFailure::InvalidProjection)?;
        control.report_progress(progress).map_err(
            |GraphComputationControlError::Unavailable| GraphRankFailure::ProgressUnavailable,
        )?;
        self.last = Some(completed);
        Ok(())
    }
}

/// Stable failure classification for one parse-independent rank pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphRankFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// Ranking exceeded its fixed wall-clock budget.
    TimedOut,
    /// Graph input or intermediate counters exceeded fixed bounds.
    ResourceLimitExceeded,
    /// The owning scheduler rejected progress reporting.
    ProgressUnavailable,
    /// The linked graph referred to an absent ranked symbol.
    InvalidGraph,
    /// Score or projection invariants were violated.
    InvalidProjection,
}

impl fmt::Display for GraphRankFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "graph ranking was cancelled",
            Self::TimedOut => "graph ranking timed out",
            Self::ResourceLimitExceeded => "graph ranking resource limit was exceeded",
            Self::ProgressUnavailable => "graph ranking progress could not be reported",
            Self::InvalidGraph => "graph ranking input is invalid",
            Self::InvalidProjection => "graph rank projection is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphRankFailure {}

#[cfg(test)]
mod tests {
    use super::{DeterministicGraphRanker, GraphRankFailure, RankingPolicy};
    use crate::{GraphComputationControl, GraphComputationControlError};
    use a3_domain::{
        ContentHash, FileRevision, GraphSymbol, LinkedGraph, LocalSymbolId, ParsedSymbol, Progress,
        RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName,
    };

    #[derive(Debug)]
    struct SilentControl;

    impl GraphComputationControl for SilentControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), GraphComputationControlError> {
            Ok(())
        }
    }

    #[test]
    fn rank_policy_rejects_symbol_overflow() -> Result<(), Box<dyn std::error::Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let range = SourceRange::new(0, 0, SourcePosition::new(0, 0), SourcePosition::new(0, 0))?;
        let graph = LinkedGraph::new(
            SnapshotId::from_bytes([2; 32]),
            vec![revision.clone()],
            vec![GraphSymbol::new(
                SymbolId::from_bytes([3; 32]),
                revision,
                ParsedSymbol::new(
                    LocalSymbolId::new(1)?,
                    SymbolKind::Module,
                    SymbolName::try_from_string("lib".to_owned())?,
                    range,
                    range,
                )?,
            )],
            Vec::new(),
            Vec::new(),
        )?;
        let mut policy = RankingPolicy::v1();
        policy.max_symbols = 0;
        assert_eq!(
            DeterministicGraphRanker.rank(&graph, policy, &SilentControl),
            Err(GraphRankFailure::ResourceLimitExceeded)
        );
        Ok(())
    }
}
