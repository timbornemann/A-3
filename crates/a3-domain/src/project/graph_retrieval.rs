use super::{
    ExactSearchTarget, GraphEdge, GraphEndpoint, IndexRunId, SnapshotId, SourceChannel, SymbolId,
    SyntaxRelationKind,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_TRAVERSAL_DEPTH: u8 = 2;
const MAX_TRAVERSAL_RESULTS: u16 = 100;

/// Direction in which persisted graph edges are followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TraversalDirection {
    /// Follow an edge from its declared source to its target.
    Outgoing,
    /// Follow an edge from its declared target back to its source.
    Incoming,
}

/// Positive graph distance capped at the interactive two-hop boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraversalDepth(u8);

impl TraversalDepth {
    /// Direct-neighbor traversal.
    pub const DIRECT: Self = Self(1);
    /// Maximum V1 interactive traversal depth.
    pub const INTERACTIVE_MAX: Self = Self(MAX_TRAVERSAL_DEPTH);

    /// Creates a positive depth within the fixed interactive boundary.
    pub fn new(value: u8) -> Result<Self, TraversalDepthError> {
        if value == 0 || value > MAX_TRAVERSAL_DEPTH {
            return Err(TraversalDepthError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded hop count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Graph traversal depth outside the V1 interactive boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalDepthError {
    value: u8,
}

impl fmt::Display for TraversalDepthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "graph traversal depth {} must be between 1 and {MAX_TRAVERSAL_DEPTH}",
            self.value
        )
    }
}

impl Error for TraversalDepthError {}

/// Positive number of graph targets returned by one bounded query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraversalResultLimit(u16);

impl TraversalResultLimit {
    /// Default result boundary for interactive graph expansion.
    pub const DEFAULT: Self = Self(20);

    /// Creates a positive limit capped at the product boundary.
    pub fn new(value: u16) -> Result<Self, TraversalResultLimitError> {
        if value == 0 || value > MAX_TRAVERSAL_RESULTS {
            return Err(TraversalResultLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Graph result limit outside the V1 product boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalResultLimitError {
    value: u16,
}

impl fmt::Display for TraversalResultLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "graph traversal result limit {} must be between 1 and {MAX_TRAVERSAL_RESULTS}",
            self.value
        )
    }
}

impl Error for TraversalResultLimitError {}

/// One typed and strictly bounded traversal over a published evidence graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraversalQuery {
    start: GraphEndpoint,
    direction: TraversalDirection,
    relation: SyntaxRelationKind,
    max_depth: TraversalDepth,
    result_limit: TraversalResultLimit,
}

impl TraversalQuery {
    /// Creates a generic relation-specific graph traversal.
    #[must_use]
    pub const fn new(
        start: GraphEndpoint,
        direction: TraversalDirection,
        relation: SyntaxRelationKind,
        max_depth: TraversalDepth,
        result_limit: TraversalResultLimit,
    ) -> Self {
        Self {
            start,
            direction,
            relation,
            max_depth,
            result_limit,
        }
    }

    /// Creates a direct callers query for one current symbol.
    #[must_use]
    pub const fn callers(symbol: SymbolId, result_limit: TraversalResultLimit) -> Self {
        Self::new(
            GraphEndpoint::Symbol(symbol),
            TraversalDirection::Incoming,
            SyntaxRelationKind::Calls,
            TraversalDepth::DIRECT,
            result_limit,
        )
    }

    /// Creates a direct callees query for one current symbol.
    #[must_use]
    pub const fn callees(symbol: SymbolId, result_limit: TraversalResultLimit) -> Self {
        Self::new(
            GraphEndpoint::Symbol(symbol),
            TraversalDirection::Outgoing,
            SyntaxRelationKind::Calls,
            TraversalDepth::DIRECT,
            result_limit,
        )
    }

    /// Creates a direct outgoing import query from a file or symbol.
    #[must_use]
    pub const fn imports(start: GraphEndpoint, result_limit: TraversalResultLimit) -> Self {
        Self::new(
            start,
            TraversalDirection::Outgoing,
            SyntaxRelationKind::Imports,
            TraversalDepth::DIRECT,
            result_limit,
        )
    }

    /// Creates a direct outgoing export query from a file or symbol.
    #[must_use]
    pub const fn exports(start: GraphEndpoint, result_limit: TraversalResultLimit) -> Self {
        Self::new(
            start,
            TraversalDirection::Outgoing,
            SyntaxRelationKind::Exports,
            TraversalDepth::DIRECT,
            result_limit,
        )
    }

    /// Creates a direct incoming query for tests that point at one target.
    #[must_use]
    pub const fn tests(start: GraphEndpoint, result_limit: TraversalResultLimit) -> Self {
        Self::new(
            start,
            TraversalDirection::Incoming,
            SyntaxRelationKind::Tests,
            TraversalDepth::DIRECT,
            result_limit,
        )
    }

    /// Returns the seed that must exist in the selected published run.
    #[must_use]
    pub const fn start(&self) -> &GraphEndpoint {
        &self.start
    }

    /// Returns the direction in which edges are followed.
    #[must_use]
    pub const fn direction(&self) -> TraversalDirection {
        self.direction
    }

    /// Returns the only relation kind eligible for this traversal.
    #[must_use]
    pub const fn relation(&self) -> SyntaxRelationKind {
        self.relation
    }

    /// Returns the maximum number of edges in an explanation path.
    #[must_use]
    pub const fn max_depth(&self) -> TraversalDepth {
        self.max_depth
    }

    /// Returns the maximum number of targets in the result.
    #[must_use]
    pub const fn result_limit(&self) -> TraversalResultLimit {
        self.result_limit
    }

    /// Returns the retrieval channel used by this relation.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        if matches!(self.relation, SyntaxRelationKind::Tests) {
            SourceChannel::Test
        } else {
            SourceChannel::Graph
        }
    }
}

/// Current evidence-bearing file or symbol reached by graph traversal.
pub type GraphTraversalTarget = ExactSearchTarget;

/// One graph candidate explained by its complete shortest evidence path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphTraversalHit {
    target: GraphTraversalTarget,
    source_channel: SourceChannel,
    path: Vec<GraphEdge>,
}

impl GraphTraversalHit {
    /// Creates a hit only when every edge forms one simple, query-compatible path.
    pub fn new(
        target: GraphTraversalTarget,
        path: Vec<GraphEdge>,
        query: &TraversalQuery,
        snapshot_id: SnapshotId,
    ) -> Result<Self, GraphTraversalHitError> {
        if path.is_empty() || path.len() > usize::from(query.max_depth().get()) {
            return Err(GraphTraversalHitError::InvalidLength);
        }
        let mut current = query.start().clone();
        let mut visited = BTreeSet::from([current.clone()]);
        for edge in &path {
            if edge.snapshot_id() != snapshot_id || edge.kind() != query.relation() {
                return Err(GraphTraversalHitError::IncompatibleEdge);
            }
            let (from, to) = traversal_endpoints(edge, query.direction());
            if from != &current {
                return Err(GraphTraversalHitError::DisconnectedPath);
            }
            if !visited.insert(to.clone()) {
                return Err(GraphTraversalHitError::Cycle);
            }
            current = to.clone();
        }
        if current != target_endpoint(&target) {
            return Err(GraphTraversalHitError::TargetMismatch);
        }
        Ok(Self {
            target,
            source_channel: query.source_channel(),
            path,
        })
    }

    /// Returns the current file or symbol reached by traversal.
    #[must_use]
    pub const fn target(&self) -> &GraphTraversalTarget {
        &self.target
    }

    /// Returns `Graph` or the dedicated `Test` relationship channel.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        self.source_channel
    }

    /// Returns the ordered evidence edges from the seed to this target.
    #[must_use]
    pub fn path(&self) -> &[GraphEdge] {
        &self.path
    }
}

fn traversal_endpoints(
    edge: &GraphEdge,
    direction: TraversalDirection,
) -> (&GraphEndpoint, &GraphEndpoint) {
    match direction {
        TraversalDirection::Outgoing => (edge.source(), edge.target()),
        TraversalDirection::Incoming => (edge.target(), edge.source()),
    }
}

fn target_endpoint(target: &GraphTraversalTarget) -> GraphEndpoint {
    match target {
        GraphTraversalTarget::File(revision) => GraphEndpoint::File(revision.path().clone()),
        GraphTraversalTarget::Symbol(symbol) => GraphEndpoint::Symbol(symbol.symbol().id()),
    }
}

/// Invalid relationship between a graph query, explanation path, and target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphTraversalHitError {
    /// The path was empty or exceeded the query depth.
    InvalidLength,
    /// An edge used another relation or snapshot.
    IncompatibleEdge,
    /// Consecutive edges did not connect in traversal direction.
    DisconnectedPath,
    /// The path revisited an endpoint.
    Cycle,
    /// The final edge did not reach the declared target.
    TargetMismatch,
}

impl fmt::Display for GraphTraversalHitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidLength => "graph traversal path has an invalid length",
            Self::IncompatibleEdge => "graph traversal path contains an incompatible edge",
            Self::DisconnectedPath => "graph traversal path is disconnected",
            Self::Cycle => "graph traversal path contains a cycle",
            Self::TargetMismatch => "graph traversal target does not match the final edge",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphTraversalHitError {}

/// One bounded deterministic traversal result from exactly one publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphTraversalResult {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    query: TraversalQuery,
    hits: Vec<GraphTraversalHit>,
    truncated: bool,
}

impl GraphTraversalResult {
    /// Creates a result after validating result count, channels, and unique targets.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        query: TraversalQuery,
        hits: Vec<GraphTraversalHit>,
        truncated: bool,
    ) -> Result<Self, GraphTraversalResultError> {
        if hits.len() > usize::from(query.result_limit().get()) {
            return Err(GraphTraversalResultError::TooManyHits);
        }
        let mut targets = BTreeSet::new();
        for hit in &hits {
            if hit.source_channel() != query.source_channel()
                || hit.path().is_empty()
                || hit.path().len() > usize::from(query.max_depth().get())
            {
                return Err(GraphTraversalResultError::InvalidHit);
            }
            if !targets.insert(target_endpoint(hit.target())) {
                return Err(GraphTraversalResultError::DuplicateTarget);
            }
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            query,
            hits,
            truncated,
        })
    }

    /// Returns the atomically published run traversed by this result.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot traversed by this result.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact query that produced this result.
    #[must_use]
    pub const fn query(&self) -> &TraversalQuery {
        &self.query
    }

    /// Returns shortest-path-first candidates in deterministic edge order.
    #[must_use]
    pub fn hits(&self) -> &[GraphTraversalHit] {
        &self.hits
    }

    /// Returns whether a result or edge-inspection boundary omitted more candidates.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Invalid adapter-produced graph traversal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphTraversalResultError {
    /// More targets were returned than requested.
    TooManyHits,
    /// A hit did not match the query channel or depth.
    InvalidHit,
    /// More than one explanation was returned for the same target.
    DuplicateTarget,
}

impl fmt::Display for GraphTraversalResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooManyHits => "graph traversal result exceeds its requested limit",
            Self::InvalidHit => "graph traversal result contains an incompatible hit",
            Self::DuplicateTarget => "graph traversal result contains a duplicate target",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphTraversalResultError {}

#[cfg(test)]
mod tests {
    use super::{TraversalDepth, TraversalQuery, TraversalResultLimit};
    use crate::SymbolId;

    #[test]
    fn traversal_bounds_are_positive_and_interactive() {
        assert!(TraversalDepth::new(1).is_ok());
        assert!(TraversalDepth::new(0).is_err());
        assert!(TraversalDepth::new(3).is_err());
        assert!(TraversalResultLimit::new(1).is_ok());
        assert!(TraversalResultLimit::new(101).is_err());
    }

    #[test]
    fn caller_and_callee_presets_have_opposite_directions() {
        let symbol = SymbolId::from_bytes([1; 32]);
        let callers = TraversalQuery::callers(symbol, TraversalResultLimit::DEFAULT);
        let callees = TraversalQuery::callees(symbol, TraversalResultLimit::DEFAULT);
        assert_ne!(callers.direction(), callees.direction());
        assert_eq!(callers.relation(), callees.relation());
    }
}
