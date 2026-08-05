use super::{GraphEdge, GraphEndpoint, GraphSymbol, SymbolId, UnresolvedEdgeCandidate};
use crate::{ContentHash, FileRevision, RepositoryPath, SnapshotId};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// Canonical, snapshot-bound output of deterministic graph linking before persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedGraph {
    snapshot_id: SnapshotId,
    files: Vec<FileRevision>,
    symbols: Vec<GraphSymbol>,
    edges: Vec<GraphEdge>,
    unresolved: Vec<UnresolvedEdgeCandidate>,
}

impl LinkedGraph {
    /// Validates freshness and endpoint ownership, then canonicalizes every graph set.
    pub fn new(
        snapshot_id: SnapshotId,
        mut files: Vec<FileRevision>,
        mut symbols: Vec<GraphSymbol>,
        mut edges: Vec<GraphEdge>,
        mut unresolved: Vec<UnresolvedEdgeCandidate>,
    ) -> Result<Self, LinkedGraphError> {
        files.sort_by(|left, right| left.path().cmp(right.path()));
        if files
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(LinkedGraphError::DuplicateFile);
        }
        let file_revisions = files
            .iter()
            .map(|revision| (revision.path().clone(), revision.content_hash()))
            .collect::<BTreeMap<_, _>>();

        symbols.sort_by_key(GraphSymbol::id);
        if symbols.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(LinkedGraphError::DuplicateSymbol);
        }
        let mut local_symbols = BTreeSet::new();
        for symbol in &symbols {
            validate_revision(symbol.revision(), &file_revisions)?;
            if !local_symbols.insert((symbol.revision().path().clone(), symbol.parsed().id())) {
                return Err(LinkedGraphError::DuplicateLocalSymbol);
            }
        }
        let symbol_files = symbols
            .iter()
            .map(|symbol| (symbol.id(), symbol.revision().path().clone()))
            .collect::<BTreeMap<_, _>>();

        edges.sort_by(compare_edges);
        for edge in &edges {
            validate_snapshot(edge.snapshot_id(), snapshot_id)?;
            validate_revision(edge.evidence().revision(), &file_revisions)?;
            validate_endpoint(edge.source(), &file_revisions, &symbol_files)?;
            validate_endpoint(edge.target(), &file_revisions, &symbol_files)?;
            validate_evidence_source(
                edge.source(),
                edge.evidence().revision().path(),
                &symbol_files,
            )?;
        }
        if edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(LinkedGraphError::DuplicateEdge);
        }

        unresolved.sort_by(compare_candidates);
        for candidate in &unresolved {
            validate_snapshot(candidate.snapshot_id(), snapshot_id)?;
            validate_revision(candidate.evidence().revision(), &file_revisions)?;
            validate_endpoint(candidate.source(), &file_revisions, &symbol_files)?;
            validate_evidence_source(
                candidate.source(),
                candidate.evidence().revision().path(),
                &symbol_files,
            )?;
        }
        if unresolved.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(LinkedGraphError::DuplicateCandidate);
        }

        Ok(Self {
            snapshot_id,
            files,
            symbols,
            edges,
            unresolved,
        })
    }

    /// Returns the immutable snapshot containing the graph.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the complete effective file state in canonical path order.
    #[must_use]
    pub fn files(&self) -> &[FileRevision] {
        &self.files
    }

    /// Returns symbols in stable global-ID order.
    #[must_use]
    pub fn symbols(&self) -> &[GraphSymbol] {
        &self.symbols
    }

    /// Returns resolved edges in canonical endpoint and evidence order.
    #[must_use]
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// Returns candidates that were deliberately not promoted to resolved edges.
    #[must_use]
    pub fn unresolved(&self) -> &[UnresolvedEdgeCandidate] {
        &self.unresolved
    }
}

fn validate_revision(
    revision: &FileRevision,
    files: &BTreeMap<RepositoryPath, ContentHash>,
) -> Result<(), LinkedGraphError> {
    match files.get(revision.path()) {
        Some(hash) if *hash == revision.content_hash() => Ok(()),
        _ => Err(LinkedGraphError::EvidenceRevisionMissing),
    }
}

fn validate_snapshot(actual: SnapshotId, expected: SnapshotId) -> Result<(), LinkedGraphError> {
    if actual != expected {
        return Err(LinkedGraphError::SnapshotMismatch);
    }
    Ok(())
}

fn validate_endpoint(
    endpoint: &GraphEndpoint,
    files: &BTreeMap<RepositoryPath, ContentHash>,
    symbols: &BTreeMap<SymbolId, RepositoryPath>,
) -> Result<(), LinkedGraphError> {
    let exists = match endpoint {
        GraphEndpoint::File(path) => files.contains_key(path),
        GraphEndpoint::Symbol(id) => symbols.contains_key(id),
    };
    if !exists {
        return Err(LinkedGraphError::UnknownEndpoint);
    }
    Ok(())
}

fn validate_evidence_source(
    endpoint: &GraphEndpoint,
    evidence_path: &RepositoryPath,
    symbols: &BTreeMap<SymbolId, RepositoryPath>,
) -> Result<(), LinkedGraphError> {
    let source_path = match endpoint {
        GraphEndpoint::File(path) => Some(path),
        GraphEndpoint::Symbol(id) => symbols.get(id),
    };
    if source_path != Some(evidence_path) {
        return Err(LinkedGraphError::EvidenceSourceMismatch);
    }
    Ok(())
}

fn compare_edges(left: &GraphEdge, right: &GraphEdge) -> Ordering {
    left.source()
        .cmp(right.source())
        .then_with(|| left.target().cmp(right.target()))
        .then_with(|| left.kind().cmp(&right.kind()))
        .then_with(|| left.provider().cmp(&right.provider()))
        .then_with(|| left.confidence().cmp(&right.confidence()))
        .then_with(|| left.resolution().cmp(&right.resolution()))
        .then_with(|| compare_evidence(left.evidence(), right.evidence()))
}

fn compare_candidates(left: &UnresolvedEdgeCandidate, right: &UnresolvedEdgeCandidate) -> Ordering {
    left.source()
        .cmp(right.source())
        .then_with(|| left.target().cmp(right.target()))
        .then_with(|| left.kind().cmp(&right.kind()))
        .then_with(|| left.provider().cmp(&right.provider()))
        .then_with(|| left.confidence().cmp(&right.confidence()))
        .then_with(|| left.reason().cmp(&right.reason()))
        .then_with(|| compare_evidence(left.evidence(), right.evidence()))
}

fn compare_evidence(left: &super::EvidenceRef, right: &super::EvidenceRef) -> Ordering {
    left.revision()
        .path()
        .cmp(right.revision().path())
        .then_with(|| {
            left.revision()
                .content_hash()
                .as_bytes()
                .cmp(right.revision().content_hash().as_bytes())
        })
        .then_with(|| left.range().cmp(&right.range()))
}

/// Invalid or stale linked-graph aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedGraphError {
    /// More than one effective revision used the same path.
    DuplicateFile,
    /// More than one graph symbol used the same stable ID.
    DuplicateSymbol,
    /// More than one graph symbol represented the same file-local adapter symbol.
    DuplicateLocalSymbol,
    /// An edge or candidate targeted a different snapshot.
    SnapshotMismatch,
    /// A symbol or evidence reference was absent or stale in the effective file state.
    EvidenceRevisionMissing,
    /// A relation's evidence came from a different file than its source endpoint.
    EvidenceSourceMismatch,
    /// A resolved endpoint was absent from the graph.
    UnknownEndpoint,
    /// Two identical resolved edges were supplied.
    DuplicateEdge,
    /// Two identical unresolved candidates were supplied.
    DuplicateCandidate,
}

impl fmt::Display for LinkedGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DuplicateFile => "linked graph contains a duplicate file",
            Self::DuplicateSymbol => "linked graph contains a duplicate symbol ID",
            Self::DuplicateLocalSymbol => "linked graph contains a duplicate local symbol",
            Self::SnapshotMismatch => "linked graph relation belongs to another snapshot",
            Self::EvidenceRevisionMissing => "linked graph evidence revision is stale or absent",
            Self::EvidenceSourceMismatch => "linked graph evidence does not belong to its source",
            Self::UnknownEndpoint => "linked graph relation refers to an unknown endpoint",
            Self::DuplicateEdge => "linked graph contains a duplicate edge",
            Self::DuplicateCandidate => "linked graph contains a duplicate unresolved candidate",
        };
        formatter.write_str(message)
    }
}

impl Error for LinkedGraphError {}
