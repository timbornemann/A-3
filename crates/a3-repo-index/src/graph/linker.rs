use super::control::{GraphComputationControl, GraphComputationControlError};
use super::resolution::{ResolutionIndexes, ResolutionOutcome};
use a3_domain::{
    Confidence, EvidenceRef, GraphEdge, GraphEndpoint, GraphSymbol, IndexLanguage,
    LanguageParseResult, LinkResolution, LinkedGraph, LocalSymbolId, Progress, RepositoryFileState,
    RepositoryPath, Snapshot, SymbolId, SyntaxRelation, SyntaxSource, SyntaxTarget,
    UnresolvedEdgeCandidate, UnresolvedGraphTarget, UnresolvedReason,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const SYMBOL_ID_DOMAIN: &[u8] = b"a3.graph.symbol-id.v1\0";
const WORK_POLL_INTERVAL: usize = 1_024;

/// Immutable snapshot, effective files, and parse artifacts consumed by graph linking.
#[derive(Debug, Clone, Copy)]
pub struct GraphLinkInput<'a> {
    snapshot: &'a Snapshot,
    files: &'a RepositoryFileState,
    parses: &'a [LanguageParseResult],
}

impl<'a> GraphLinkInput<'a> {
    /// Creates a borrowed link request; the linker validates freshness and adapter compatibility.
    #[must_use]
    pub const fn new(
        snapshot: &'a Snapshot,
        files: &'a RepositoryFileState,
        parses: &'a [LanguageParseResult],
    ) -> Self {
        Self {
            snapshot,
            files,
            parses,
        }
    }

    /// Returns the immutable snapshot being linked.
    #[must_use]
    pub const fn snapshot(self) -> &'a Snapshot {
        self.snapshot
    }

    /// Returns the complete effective file state for the snapshot.
    #[must_use]
    pub const fn files(self) -> &'a RepositoryFileState {
        self.files
    }

    /// Returns all available structural parse outputs.
    #[must_use]
    pub const fn parses(self) -> &'a [LanguageParseResult] {
        self.parses
    }
}

/// Fixed V1 bounds for deterministic in-memory graph linking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphLinkPolicy {
    max_files: usize,
    max_parses: usize,
    max_symbols: usize,
    max_edges: usize,
    max_unresolved: usize,
    timeout: Duration,
    max_progress_events: usize,
}

impl GraphLinkPolicy {
    /// Returns the initial policy sized to the bounded discovery and parser contracts.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            max_files: 250_000,
            max_parses: 250_000,
            max_symbols: 1_000_000,
            max_edges: 2_000_000,
            max_unresolved: 2_000_000,
            timeout: Duration::from_secs(10),
            max_progress_events: 64,
        }
    }

    /// Returns the maximum effective files.
    #[must_use]
    pub const fn max_files(self) -> usize {
        self.max_files
    }

    /// Returns the maximum parsed files.
    #[must_use]
    pub const fn max_parses(self) -> usize {
        self.max_parses
    }

    /// Returns the maximum promoted symbols.
    #[must_use]
    pub const fn max_symbols(self) -> usize {
        self.max_symbols
    }

    /// Returns the maximum resolved edges.
    #[must_use]
    pub const fn max_edges(self) -> usize {
        self.max_edges
    }

    /// Returns the maximum unresolved candidates.
    #[must_use]
    pub const fn max_unresolved(self) -> usize {
        self.max_unresolved
    }

    /// Returns the wall-clock budget for one link pass.
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Returns the maximum progress notifications.
    #[must_use]
    pub const fn max_progress_events(self) -> usize {
        self.max_progress_events
    }
}

impl Default for GraphLinkPolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// Stateless deterministic graph linker.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicGraphLinker;

impl DeterministicGraphLinker {
    /// Promotes exact adapter relations and retains every other relation as an explicit candidate.
    pub fn link(
        &self,
        input: GraphLinkInput<'_>,
        policy: GraphLinkPolicy,
        control: &dyn GraphComputationControl,
    ) -> Result<LinkedGraph, GraphLinkFailure> {
        validate_bounds(input, policy)?;
        let started = Instant::now();
        ensure_active(control, started, policy.timeout())?;

        let mut parse_order = input.parses().iter().collect::<Vec<_>>();
        parse_order.sort_by(|left, right| left.revision().path().cmp(right.revision().path()));
        validate_input(input, &parse_order)?;

        let total_work = parse_order
            .iter()
            .try_fold(0usize, |total, result| {
                total
                    .checked_add(result.symbols().len())
                    .and_then(|value| value.checked_add(result.relations().len()))
            })
            .ok_or(GraphLinkFailure::ResourceLimitExceeded)?;
        let mut progress = ProgressReporter::new(total_work, policy.max_progress_events());
        progress.report(control, 0)?;

        let mut symbols = Vec::new();
        let mut local_ids = BTreeMap::new();
        let mut completed = 0usize;
        for result in &parse_order {
            for symbol in result.symbols() {
                poll_work(completed, control, started, policy.timeout())?;
                let id = derive_symbol_id(result, symbol.id());
                if local_ids
                    .insert((result.revision().path().clone(), symbol.id()), id)
                    .is_some()
                {
                    return Err(GraphLinkFailure::InvalidInput);
                }
                push_bounded(
                    &mut symbols,
                    GraphSymbol::new(id, result.revision().clone(), symbol.clone()),
                    policy.max_symbols(),
                )?;
                completed = completed
                    .checked_add(1)
                    .ok_or(GraphLinkFailure::ResourceLimitExceeded)?;
                progress.maybe_report(control, completed)?;
            }
        }

        let indexes = ResolutionIndexes::new(input.files(), &symbols, input.parses())?;
        let mut edges = Vec::new();
        let mut unresolved = Vec::new();
        for result in &parse_order {
            for relation in result.relations() {
                poll_work(completed, control, started, policy.timeout())?;
                link_relation(
                    input.snapshot().id(),
                    result,
                    relation,
                    &local_ids,
                    &indexes,
                    &mut edges,
                    &mut unresolved,
                    policy,
                )?;
                completed = completed
                    .checked_add(1)
                    .ok_or(GraphLinkFailure::ResourceLimitExceeded)?;
                progress.maybe_report(control, completed)?;
            }
        }
        progress.report(control, total_work)?;
        ensure_active(control, started, policy.timeout())?;

        LinkedGraph::new(
            input.snapshot().id(),
            input.files().revisions().to_vec(),
            symbols,
            edges,
            unresolved,
        )
        .map_err(|_| GraphLinkFailure::InvalidGraph)
    }
}

#[allow(clippy::too_many_arguments)]
fn link_relation(
    snapshot_id: a3_domain::SnapshotId,
    result: &LanguageParseResult,
    relation: &SyntaxRelation,
    local_ids: &BTreeMap<(RepositoryPath, LocalSymbolId), SymbolId>,
    indexes: &ResolutionIndexes,
    edges: &mut Vec<GraphEdge>,
    unresolved: &mut Vec<UnresolvedEdgeCandidate>,
    policy: GraphLinkPolicy,
) -> Result<(), GraphLinkFailure> {
    let path = result.revision().path();
    let source = resolve_source(path, relation.source(), local_ids)?;
    let evidence = EvidenceRef::new(result.revision().clone(), relation.evidence_range());
    let outcome = match relation.target() {
        SyntaxTarget::Symbol(local_id) => ResolutionOutcome::Resolved {
            endpoint: GraphEndpoint::Symbol(local_symbol_id(path, *local_id, local_ids)?),
            resolution: LinkResolution::AdapterLocalSymbol,
            confidence_cap: Confidence::certain(),
        },
        SyntaxTarget::File(target) => {
            if indexes.contains_file(target) {
                ResolutionOutcome::Resolved {
                    endpoint: GraphEndpoint::File(target.clone()),
                    resolution: LinkResolution::AdapterFile,
                    confidence_cap: Confidence::certain(),
                }
            } else {
                ResolutionOutcome::Unresolved {
                    target: UnresolvedGraphTarget::File(target.clone()),
                    reason: UnresolvedReason::MissingFile,
                }
            }
        }
        SyntaxTarget::Unresolved(reference) => {
            if relation.kind() == a3_domain::SyntaxRelationKind::Calls {
                indexes.resolve_call(result, relation, reference)?
            } else {
                indexes.resolve(result.language(), path, reference, relation.kind())?
            }
        }
    };

    match outcome {
        ResolutionOutcome::Resolved {
            endpoint,
            resolution,
            confidence_cap,
        } => push_bounded(
            edges,
            GraphEdge::new(
                source,
                endpoint,
                relation.kind(),
                relation.provider(),
                minimum_confidence(relation.confidence(), confidence_cap)?,
                resolution,
                snapshot_id,
                evidence,
            ),
            policy.max_edges(),
        ),
        ResolutionOutcome::Unresolved { target, reason } => push_bounded(
            unresolved,
            UnresolvedEdgeCandidate::new(
                source,
                target,
                relation.kind(),
                relation.provider(),
                relation.confidence(),
                reason,
                snapshot_id,
                evidence,
            ),
            policy.max_unresolved(),
        ),
    }
}

fn resolve_source(
    path: &RepositoryPath,
    source: SyntaxSource,
    local_ids: &BTreeMap<(RepositoryPath, LocalSymbolId), SymbolId>,
) -> Result<GraphEndpoint, GraphLinkFailure> {
    match source {
        SyntaxSource::File => Ok(GraphEndpoint::File(path.clone())),
        SyntaxSource::Symbol(local_id) => Ok(GraphEndpoint::Symbol(local_symbol_id(
            path, local_id, local_ids,
        )?)),
    }
}

fn local_symbol_id(
    path: &RepositoryPath,
    local_id: LocalSymbolId,
    local_ids: &BTreeMap<(RepositoryPath, LocalSymbolId), SymbolId>,
) -> Result<SymbolId, GraphLinkFailure> {
    local_ids
        .get(&(path.clone(), local_id))
        .copied()
        .ok_or(GraphLinkFailure::InvalidInput)
}

fn derive_symbol_id(result: &LanguageParseResult, local_id: LocalSymbolId) -> SymbolId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SYMBOL_ID_DOMAIN);
    update_bytes(&mut hasher, result.revision().path().as_bytes());
    hasher.update(result.revision().content_hash().as_bytes());
    update_bytes(&mut hasher, result.language().as_str().as_bytes());
    update_bytes(
        &mut hasher,
        result.adapter_revision().version().as_str().as_bytes(),
    );
    hasher.update(&result.contract_version().get().to_le_bytes());
    hasher.update(&local_id.get().to_le_bytes());
    SymbolId::from_bytes(*hasher.finalize().as_bytes())
}

fn update_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn minimum_confidence(
    observed: Confidence,
    resolution_cap: Confidence,
) -> Result<Confidence, GraphLinkFailure> {
    Confidence::from_basis_points(observed.basis_points().min(resolution_cap.basis_points()))
        .map_err(|_| GraphLinkFailure::InvalidGraph)
}

fn validate_bounds(
    input: GraphLinkInput<'_>,
    policy: GraphLinkPolicy,
) -> Result<(), GraphLinkFailure> {
    if input.files().revisions().len() > policy.max_files()
        || input.parses().len() > policy.max_parses()
    {
        return Err(GraphLinkFailure::ResourceLimitExceeded);
    }
    let symbol_count = input
        .parses()
        .iter()
        .try_fold(0usize, |count, parse| {
            count.checked_add(parse.symbols().len())
        })
        .ok_or(GraphLinkFailure::ResourceLimitExceeded)?;
    let relation_count = input
        .parses()
        .iter()
        .try_fold(0usize, |count, parse| {
            count.checked_add(parse.relations().len())
        })
        .ok_or(GraphLinkFailure::ResourceLimitExceeded)?;
    if symbol_count > policy.max_symbols()
        || relation_count
            > policy
                .max_edges()
                .checked_add(policy.max_unresolved())
                .ok_or(GraphLinkFailure::ResourceLimitExceeded)?
    {
        return Err(GraphLinkFailure::ResourceLimitExceeded);
    }
    Ok(())
}

fn validate_input(
    input: GraphLinkInput<'_>,
    parse_order: &[&LanguageParseResult],
) -> Result<(), GraphLinkFailure> {
    let files = input
        .files()
        .revisions()
        .iter()
        .map(|revision| (revision.path(), revision.content_hash()))
        .collect::<BTreeMap<_, _>>();
    let revisions = input.snapshot().adapter_revisions();
    let mut paths = BTreeSet::new();
    for result in parse_order {
        if !paths.insert(result.revision().path()) {
            return Err(GraphLinkFailure::InvalidInput);
        }
        if files.get(result.revision().path()) != Some(&result.revision().content_hash()) {
            return Err(GraphLinkFailure::InvalidInput);
        }
        if !revisions.contains(&result.adapter_revision().clone()) {
            return Err(GraphLinkFailure::InvalidInput);
        }
        if result.language() == IndexLanguage::Generic && !result.symbols().is_empty() {
            return Err(GraphLinkFailure::InvalidInput);
        }
    }
    Ok(())
}

fn push_bounded<T>(values: &mut Vec<T>, value: T, maximum: usize) -> Result<(), GraphLinkFailure> {
    if values.len() >= maximum {
        return Err(GraphLinkFailure::ResourceLimitExceeded);
    }
    values.push(value);
    Ok(())
}

fn poll_work(
    completed: usize,
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), GraphLinkFailure> {
    if completed.is_multiple_of(WORK_POLL_INTERVAL) {
        ensure_active(control, started, timeout)?;
    }
    Ok(())
}

fn ensure_active(
    control: &dyn GraphComputationControl,
    started: Instant,
    timeout: Duration,
) -> Result<(), GraphLinkFailure> {
    if control.is_cancelled() {
        return Err(GraphLinkFailure::Cancelled);
    }
    if started.elapsed() > timeout {
        return Err(GraphLinkFailure::TimedOut);
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
    ) -> Result<(), GraphLinkFailure> {
        if completed == self.total || completed.is_multiple_of(self.interval) {
            self.report(control, completed)?;
        }
        Ok(())
    }

    fn report(
        &mut self,
        control: &dyn GraphComputationControl,
        completed: usize,
    ) -> Result<(), GraphLinkFailure> {
        if self.last == Some(completed) {
            return Ok(());
        }
        let reported_total = self.total.max(1);
        let reported_completed = if self.total == 0 { 1 } else { completed };
        let progress = Progress::determinate(reported_completed as u64, reported_total as u64)
            .map_err(|_| GraphLinkFailure::InvalidGraph)?;
        control.report_progress(progress).map_err(
            |GraphComputationControlError::Unavailable| GraphLinkFailure::ProgressUnavailable,
        )?;
        self.last = Some(completed);
        Ok(())
    }
}

/// Stable failure classification for one graph-link pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphLinkFailure {
    /// The owning job requested cooperative cancellation.
    Cancelled,
    /// Linking exceeded its fixed wall-clock budget.
    TimedOut,
    /// Files, artifacts, or produced graph data exceeded fixed bounds.
    ResourceLimitExceeded,
    /// The owning scheduler rejected progress reporting.
    ProgressUnavailable,
    /// Parse artifacts were stale, duplicated, or snapshot-incompatible.
    InvalidInput,
    /// The produced graph violated a domain invariant.
    InvalidGraph,
}

impl fmt::Display for GraphLinkFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "graph linking was cancelled",
            Self::TimedOut => "graph linking timed out",
            Self::ResourceLimitExceeded => "graph linking resource limit was exceeded",
            Self::ProgressUnavailable => "graph linking progress could not be reported",
            Self::InvalidInput => "graph linking input is stale or invalid",
            Self::InvalidGraph => "linked graph is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphLinkFailure {}

#[cfg(test)]
mod tests {
    use super::{DeterministicGraphLinker, GraphLinkFailure, GraphLinkInput, GraphLinkPolicy};
    use crate::{GraphComputationControl, GraphComputationControlError};
    use a3_domain::{
        ContentHash, FileRevision, GitHead, GitReferenceName, IndexLanguage, IndexSchemaVersion,
        LanguageAdapterRevision, LanguageAdapterVersion, Progress, RepositoryFileState,
        RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotId,
        WorktreeGeneration, WorktreeId,
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
    fn link_policy_rejects_file_overflow_and_accepts_an_empty_graph()
    -> Result<(), Box<dyn std::error::Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"README.md".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let files = RepositoryFileState::new(vec![revision.clone()])?;
        let populated_snapshot = snapshot(vec![SnapshotChange::new(
            revision.path().clone(),
            revision.content_hash(),
            SnapshotChangeKind::Upsert,
        )])?;
        let mut policy = GraphLinkPolicy::v1();
        policy.max_files = 0;
        assert_eq!(
            DeterministicGraphLinker.link(
                GraphLinkInput::new(&populated_snapshot, &files, &[]),
                policy,
                &SilentControl,
            ),
            Err(GraphLinkFailure::ResourceLimitExceeded)
        );

        let empty_snapshot = snapshot(Vec::new())?;
        let empty_files = RepositoryFileState::empty();
        let graph = DeterministicGraphLinker.link(
            GraphLinkInput::new(&empty_snapshot, &empty_files, &[]),
            GraphLinkPolicy::v1(),
            &SilentControl,
        )?;
        assert!(graph.files().is_empty());
        assert!(graph.symbols().is_empty());
        Ok(())
    }

    fn snapshot(changes: Vec<SnapshotChange>) -> Result<Snapshot, Box<dyn std::error::Error>> {
        Ok(Snapshot::new(
            SnapshotId::from_bytes([2; 32]),
            WorktreeId::from_bytes([3; 32]),
            None,
            WorktreeGeneration::new(1)?,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
            IndexSchemaVersion::new(1)?,
            vec![LanguageAdapterRevision::new(
                IndexLanguage::Generic,
                LanguageAdapterVersion::try_from_string("generic-v1".to_owned())?,
            )],
            changes,
        )?)
    }
}
