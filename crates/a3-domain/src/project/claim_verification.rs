use super::{
    FileRevision, GraphEdge, GraphEndpoint, GraphSymbol, IndexRunId, LinkResolution,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal, ModuleCardStatus,
    ModuleId, PublishedIndex, RepositoryPath, SnapshotId, SymbolId, SyntaxProvider,
    SyntaxRelationKind,
};
use crate::Confidence;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const MAX_CLAIM_STATEMENT_BYTES: usize = 2_048;
const MAX_EVIDENCE_PER_CLAIM: usize = 16;
const MAX_CLAIMS_PER_CARD: usize = 2_048;
const MAX_CARDS_PER_VERIFICATION: usize = 512;

impl ModuleCardEvidenceId {
    /// Derives version-one evidence identity for one exact current file revision.
    #[must_use]
    pub fn for_file_revision_v1(revision: &FileRevision) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"a3:module-card-evidence:file:v1\0");
        hash_revision(&mut hasher, revision);
        Self::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Derives version-one evidence identity for one content-bound structural symbol.
    #[must_use]
    pub fn for_symbol_v1(symbol: &GraphSymbol) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"a3:module-card-evidence:symbol:v1\0");
        hasher.update(symbol.id().as_bytes());
        Self::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Derives version-one evidence identity for one exact current graph edge.
    #[must_use]
    pub fn for_graph_edge_v1(edge: &GraphEdge) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"a3:module-card-evidence:graph-edge:v1\0");
        hasher.update(edge.snapshot_id().as_bytes());
        hash_endpoint(&mut hasher, edge.source());
        hash_endpoint(&mut hasher, edge.target());
        hasher.update(&[relation_kind_code(edge.kind())]);
        hasher.update(&[provider_code(edge.provider())]);
        hasher.update(&[resolution_code(edge.resolution())]);
        hasher.update(&edge.confidence().basis_points().to_le_bytes());
        hash_revision(&mut hasher, edge.evidence().revision());
        hasher.update(&edge.evidence().range().start_byte().to_le_bytes());
        hasher.update(&edge.evidence().range().end_byte().to_le_bytes());
        Self::from_bytes(*hasher.finalize().as_bytes())
    }
}

fn hash_revision(hasher: &mut blake3::Hasher, revision: &FileRevision) {
    hasher.update(&(revision.path().as_bytes().len() as u64).to_le_bytes());
    hasher.update(revision.path().as_bytes());
    hasher.update(revision.content_hash().as_bytes());
}

fn hash_endpoint(hasher: &mut blake3::Hasher, endpoint: &GraphEndpoint) {
    match endpoint {
        GraphEndpoint::File(path) => {
            hasher.update(&[0]);
            hasher.update(&(path.as_bytes().len() as u64).to_le_bytes());
            hasher.update(path.as_bytes());
        }
        GraphEndpoint::Symbol(symbol_id) => {
            hasher.update(&[1]);
            hasher.update(symbol_id.as_bytes());
        }
    }
}

const fn relation_kind_code(kind: SyntaxRelationKind) -> u8 {
    match kind {
        SyntaxRelationKind::Contains => 0,
        SyntaxRelationKind::Defines => 1,
        SyntaxRelationKind::Imports => 2,
        SyntaxRelationKind::Exports => 3,
        SyntaxRelationKind::Calls => 4,
        SyntaxRelationKind::Implements => 5,
        SyntaxRelationKind::Extends => 6,
        SyntaxRelationKind::Reads => 7,
        SyntaxRelationKind::Writes => 8,
        SyntaxRelationKind::Configures => 9,
        SyntaxRelationKind::Tests => 10,
        SyntaxRelationKind::Builds => 11,
        SyntaxRelationKind::Documents => 12,
    }
}

const fn provider_code(provider: SyntaxProvider) -> u8 {
    match provider {
        SyntaxProvider::TreeSitter => 0,
        SyntaxProvider::Manifest => 1,
        SyntaxProvider::LanguageHeuristic => 2,
    }
}

const fn resolution_code(resolution: LinkResolution) -> u8 {
    match resolution {
        LinkResolution::AdapterLocalSymbol => 0,
        LinkResolution::AdapterFile => 1,
        LinkResolution::ExactModuleReference => 2,
        LinkResolution::UniqueFileLocalName => 3,
        LinkResolution::UniqueQualifiedName => 4,
    }
}

/// Stable identity of one logical Module Card claim across verification runs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleCardClaimId([u8; 32]);

impl ModuleCardClaimId {
    /// Reconstructs an ID produced by a versioned claim proposer.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical persisted bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ModuleCardClaimId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModuleCardClaimId(redacted)")
    }
}

/// Bounded non-authoritative prose retained only for Observation or Hypothesis claims.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleClaimStatement(String);

impl ModuleClaimStatement {
    /// Accepts safe non-empty single-line claim text.
    pub fn try_from_string(value: String) -> Result<Self, ModuleClaimStatementError> {
        if value.trim().is_empty()
            || value.len() > MAX_CLAIM_STATEMENT_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ModuleClaimStatementError {
                actual: value.len(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the bounded statement for presentation, never for deterministic control flow.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ModuleClaimStatement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleClaimStatement")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Claim prose was empty, unsafe, or larger than 2 KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleClaimStatementError {
    actual: usize,
}

impl fmt::Display for ModuleClaimStatementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "module claim statement has {} bytes or invalid control characters",
            self.actual
        )
    }
}

impl Error for ModuleClaimStatementError {}

/// Whether a claim asserts or denies one typed predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleClaimPolarity {
    /// The predicate is asserted to hold.
    Affirms,
    /// The predicate is asserted not to hold.
    Denies,
}

/// Typed proposition that can be verified without parsing prose.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleClaimPredicate {
    /// A current file revision is part of the described module view.
    Path(RepositoryPath),
    /// A structural symbol exists in the current published graph.
    Symbol(SymbolId),
    /// A current published graph contains one exact supported relationship.
    Relation {
        /// Exact source endpoint.
        source: GraphEndpoint,
        /// Exact target endpoint.
        target: GraphEndpoint,
        /// Imports, Exports, Calls, or Tests.
        kind: SyntaxRelationKind,
    },
    /// Direct source/tool observation whose meaning is not a deterministic graph invariant.
    Observed(ModuleClaimStatement),
    /// Architecture intent or interpretation that deterministic evidence cannot prove.
    ArchitecturalIntent(ModuleClaimStatement),
}

impl fmt::Debug for ModuleClaimPredicate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(_) => formatter.write_str("Path(redacted)"),
            Self::Symbol(_) => formatter.write_str("Symbol(redacted)"),
            Self::Relation { kind, .. } => formatter
                .debug_struct("Relation")
                .field("endpoints", &"redacted")
                .field("kind", kind)
                .finish(),
            Self::Observed(statement) => {
                formatter.debug_tuple("Observed").field(statement).finish()
            }
            Self::ArchitecturalIntent(statement) => formatter
                .debug_tuple("ArchitecturalIntent")
                .field(statement)
                .finish(),
        }
    }
}

/// Snapshot- and field-item-bound metadata of one proposed claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleClaimEnvelope {
    id: ModuleCardClaimId,
    card_id: ModuleCardId,
    module_id: ModuleId,
    snapshot_id: SnapshotId,
    field: ModuleCardField,
    value_index: u16,
    confidence: Confidence,
}

impl ModuleClaimEnvelope {
    /// Groups claim identity, ownership, exact field item, snapshot, and confidence.
    #[must_use]
    pub const fn new(
        id: ModuleCardClaimId,
        card_id: ModuleCardId,
        module_id: ModuleId,
        snapshot_id: SnapshotId,
        field: ModuleCardField,
        value_index: u16,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            card_id,
            module_id,
            snapshot_id,
            field,
            value_index,
            confidence,
        }
    }
}

/// Structured, non-authoritative claim paired with exact field-level evidence identities.
#[derive(Clone, PartialEq, Eq)]
pub struct ModuleClaimProposal {
    envelope: ModuleClaimEnvelope,
    polarity: ModuleClaimPolarity,
    predicate: ModuleClaimPredicate,
    evidence_ids: Vec<ModuleCardEvidenceId>,
}

impl fmt::Debug for ModuleClaimProposal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleClaimProposal")
            .field("envelope", &self.envelope)
            .field("polarity", &self.polarity)
            .field("predicate", &self.predicate)
            .field("evidence_count", &self.evidence_ids.len())
            .finish()
    }
}

impl ModuleClaimProposal {
    /// Validates predicate class and bounded canonical evidence.
    pub fn new(
        envelope: ModuleClaimEnvelope,
        polarity: ModuleClaimPolarity,
        predicate: ModuleClaimPredicate,
        mut evidence_ids: Vec<ModuleCardEvidenceId>,
    ) -> Result<Self, ModuleClaimProposalError> {
        if let ModuleClaimPredicate::Relation { kind, .. } = predicate
            && !matches!(
                kind,
                SyntaxRelationKind::Imports
                    | SyntaxRelationKind::Exports
                    | SyntaxRelationKind::Calls
                    | SyntaxRelationKind::Tests
            )
        {
            return Err(ModuleClaimProposalError::UnsupportedRelation);
        }
        let supplied_count = evidence_ids.len();
        evidence_ids.sort_unstable();
        evidence_ids.dedup();
        if evidence_ids.len() != supplied_count {
            return Err(ModuleClaimProposalError::DuplicateEvidence);
        }
        let evidence_required = !matches!(predicate, ModuleClaimPredicate::ArchitecturalIntent(_));
        if (evidence_required && evidence_ids.is_empty())
            || evidence_ids.len() > MAX_EVIDENCE_PER_CLAIM
        {
            return Err(ModuleClaimProposalError::InvalidEvidenceCount(
                evidence_ids.len(),
            ));
        }
        Ok(Self {
            envelope,
            polarity,
            predicate,
            evidence_ids,
        })
    }

    /// Returns the logical claim identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardClaimId {
        self.envelope.id
    }

    /// Returns the logical Module Card identity.
    #[must_use]
    pub const fn card_id(&self) -> ModuleCardId {
        self.envelope.card_id
    }

    /// Returns the described module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.envelope.module_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.envelope.snapshot_id
    }

    /// Returns the Module Card field containing the presentation value.
    #[must_use]
    pub const fn field(&self) -> ModuleCardField {
        self.envelope.field
    }

    /// Returns the zero-based item index within the field.
    #[must_use]
    pub const fn value_index(&self) -> u16 {
        self.envelope.value_index
    }

    /// Returns confidence separately from epistemic type and lifecycle status.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.envelope.confidence
    }

    /// Returns whether the predicate is affirmed or denied.
    #[must_use]
    pub const fn polarity(&self) -> ModuleClaimPolarity {
        self.polarity
    }

    /// Returns the typed proposition.
    #[must_use]
    pub const fn predicate(&self) -> &ModuleClaimPredicate {
        &self.predicate
    }

    /// Returns canonical field-level Evidence IDs.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }
}

/// Invalid typed claim proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleClaimProposalError {
    /// Relation kind is outside Imports, Exports, Calls, and Tests.
    UnsupportedRelation,
    /// Required evidence was absent or the per-claim maximum was exceeded.
    InvalidEvidenceCount(usize),
    /// The same evidence identity appeared more than once.
    DuplicateEvidence,
}

impl fmt::Display for ModuleClaimProposalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedRelation => {
                formatter.write_str("claim relation is not verifiable by R9")
            }
            Self::InvalidEvidenceCount(actual) => {
                write!(formatter, "claim has invalid evidence count {actual}")
            }
            Self::DuplicateEvidence => formatter.write_str("claim repeats an Evidence ID"),
        }
    }
}

impl Error for ModuleClaimProposalError {}

/// One trusted resolution of an opaque Module Card Evidence ID.
#[derive(Clone, PartialEq, Eq)]
pub enum ResolvedModuleCardEvidence {
    /// Exact current file revision.
    File {
        /// Opaque ID requested from the resolver.
        id: ModuleCardEvidenceId,
        /// Exact file revision.
        revision: FileRevision,
    },
    /// Exact current structural symbol.
    Symbol {
        /// Opaque ID requested from the resolver.
        id: ModuleCardEvidenceId,
        /// Complete current symbol projection.
        symbol: GraphSymbol,
    },
    /// Exact current deterministic graph edge.
    GraphEdge {
        /// Opaque ID requested from the resolver.
        id: ModuleCardEvidenceId,
        /// Complete current edge projection.
        edge: GraphEdge,
    },
}

impl fmt::Debug for ResolvedModuleCardEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::File { .. } => "File",
            Self::Symbol { .. } => "Symbol",
            Self::GraphEdge { .. } => "GraphEdge",
        };
        formatter
            .debug_struct("ResolvedModuleCardEvidence")
            .field("kind", &kind)
            .field("id", &self.id())
            .finish()
    }
}

impl ResolvedModuleCardEvidence {
    /// Returns the opaque proposal-facing Evidence ID.
    #[must_use]
    pub const fn id(&self) -> ModuleCardEvidenceId {
        match self {
            Self::File { id, .. } | Self::Symbol { id, .. } | Self::GraphEdge { id, .. } => *id,
        }
    }
}

/// Canonical resolver output for one immutable snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModuleCardEvidenceSet {
    snapshot_id: SnapshotId,
    evidence: Vec<ResolvedModuleCardEvidence>,
}

impl ResolvedModuleCardEvidenceSet {
    /// Rejects duplicate opaque identities before deterministic validation.
    pub fn new(
        snapshot_id: SnapshotId,
        mut evidence: Vec<ResolvedModuleCardEvidence>,
    ) -> Result<Self, ModuleCardVerificationError> {
        evidence.sort_by_key(ResolvedModuleCardEvidence::id);
        if evidence.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(ModuleCardVerificationError::DuplicateResolvedEvidence);
        }
        Ok(Self {
            snapshot_id,
            evidence,
        })
    }

    /// Returns the immutable evidence snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns evidence in stable ID order.
    #[must_use]
    pub fn evidence(&self) -> &[ResolvedModuleCardEvidence] {
        &self.evidence
    }
}

/// One complete Card proposal paired with exactly one typed claim per field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardVerificationCandidate {
    proposal: ModuleCardProposal,
    claims: Vec<ModuleClaimProposal>,
}

impl ModuleCardVerificationCandidate {
    /// Validates claim ownership, item coverage, and field-evidence membership.
    pub fn new(
        proposal: ModuleCardProposal,
        mut claims: Vec<ModuleClaimProposal>,
    ) -> Result<Self, ModuleCardVerificationError> {
        let value_count = proposal
            .fields()
            .iter()
            .map(|field| field.values().len())
            .sum::<usize>();
        if claims.len() != value_count || claims.len() > MAX_CLAIMS_PER_CARD {
            return Err(ModuleCardVerificationError::IncompleteClaimCoverage);
        }
        claims.sort_by_key(|claim| (claim.field(), claim.value_index(), claim.id()));
        if claims.windows(2).any(|pair| {
            pair[0].id() == pair[1].id()
                || (pair[0].field(), pair[0].value_index())
                    == (pair[1].field(), pair[1].value_index())
        }) {
            return Err(ModuleCardVerificationError::DuplicateClaim);
        }
        for claim in &claims {
            if claim.card_id() != proposal.id()
                || claim.module_id() != proposal.module_id()
                || claim.snapshot_id() != proposal.snapshot_id()
            {
                return Err(ModuleCardVerificationError::ClaimEnvelopeMismatch);
            }
            let field = proposal
                .fields()
                .iter()
                .find(|field| field.field() == claim.field())
                .ok_or(ModuleCardVerificationError::IncompleteClaimCoverage)?;
            let value = field
                .values()
                .get(usize::from(claim.value_index()))
                .ok_or(ModuleCardVerificationError::IncompleteClaimCoverage)?;
            if claim
                .evidence_ids()
                .iter()
                .any(|id| !field.evidence_ids().contains(id))
            {
                return Err(ModuleCardVerificationError::ClaimEvidenceOutsideField);
            }
            if let ModuleClaimPredicate::Observed(statement)
            | ModuleClaimPredicate::ArchitecturalIntent(statement) = claim.predicate()
                && statement.as_str() != value
            {
                return Err(ModuleCardVerificationError::StatementValueMismatch);
            }
        }
        Ok(Self { proposal, claims })
    }

    /// Returns the structurally valid Card proposal.
    #[must_use]
    pub const fn proposal(&self) -> &ModuleCardProposal {
        &self.proposal
    }

    /// Returns claims in stable field-item order.
    #[must_use]
    pub fn claims(&self) -> &[ModuleClaimProposal] {
        &self.claims
    }
}

/// Epistemic type assigned only by deterministic R9 verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedClaimKind {
    /// Typed path, symbol, or graph predicate matched the current published index.
    Fact,
    /// Evidence is fresh but does not prove a deterministic structural invariant.
    Observation,
    /// Interpretation or negative absence claim remains explicitly unproven.
    Hypothesis,
}

/// Freshness/lifecycle state of a claim produced by R9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedClaimStatus {
    /// Claim is current for the verified snapshot.
    Active,
}

/// Verified claim retaining confidence independently from epistemic kind and status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedModuleClaim {
    proposal: ModuleClaimProposal,
    kind: VerifiedClaimKind,
    status: VerifiedClaimStatus,
}

impl VerifiedModuleClaim {
    /// Returns the original typed claim.
    #[must_use]
    pub const fn proposal(&self) -> &ModuleClaimProposal {
        &self.proposal
    }

    /// Returns Fact, Observation, or Hypothesis classification.
    #[must_use]
    pub const fn kind(&self) -> VerifiedClaimKind {
        self.kind
    }

    /// Returns current lifecycle state.
    #[must_use]
    pub const fn status(&self) -> VerifiedClaimStatus {
        self.status
    }

    /// Returns proposal confidence without conflating it with kind or status.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.proposal.confidence()
    }
}

/// Structurally and evidentially verified Module Card, not yet published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedModuleCard {
    proposal: ModuleCardProposal,
    claims: Vec<VerifiedModuleClaim>,
}

impl VerifiedModuleCard {
    /// Returns the stable card identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardId {
        self.proposal.id()
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.proposal.snapshot_id()
    }

    /// Returns status that can only be constructed by the verifier.
    #[must_use]
    pub const fn status(&self) -> ModuleCardStatus {
        ModuleCardStatus::Verified
    }

    /// Returns verified and explicitly unverified claims in stable item order.
    #[must_use]
    pub fn claims(&self) -> &[VerifiedModuleClaim] {
        &self.claims
    }
}

/// Canonical publish-capable output from one contradiction-free verification batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedModuleCardBatch {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    cards: Vec<VerifiedModuleCard>,
}

impl VerifiedModuleCardBatch {
    /// Returns the exact published index used for verification.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns verified cards in stable logical-ID order.
    #[must_use]
    pub fn cards(&self) -> &[VerifiedModuleCard] {
        &self.cards
    }
}

/// Visible pair of opposing claims; no merge or majority decision is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardContradiction {
    first_card_id: ModuleCardId,
    second_card_id: ModuleCardId,
    predicate: ModuleClaimPredicate,
}

impl ModuleCardContradiction {
    /// Returns the first conflicting Card identity.
    #[must_use]
    pub const fn first_card_id(&self) -> ModuleCardId {
        self.first_card_id
    }

    /// Returns the second conflicting Card identity.
    #[must_use]
    pub const fn second_card_id(&self) -> ModuleCardId {
        self.second_card_id
    }

    /// Returns the exact typed predicate with opposing polarities.
    #[must_use]
    pub const fn predicate(&self) -> &ModuleClaimPredicate {
        &self.predicate
    }
}

/// Deterministic verifier over one atomically published graph and resolved evidence set.
#[derive(Debug, Clone, Copy)]
pub struct ModuleCardVerifier;

impl ModuleCardVerifier {
    /// Verifies a bounded batch without model calls, persistence, or network access.
    pub fn verify(
        published: &PublishedIndex,
        mut candidates: Vec<ModuleCardVerificationCandidate>,
        evidence: &ResolvedModuleCardEvidenceSet,
    ) -> Result<VerifiedModuleCardBatch, ModuleCardVerificationError> {
        if candidates.is_empty() || candidates.len() > MAX_CARDS_PER_VERIFICATION {
            return Err(ModuleCardVerificationError::InvalidCardCount(
                candidates.len(),
            ));
        }
        let snapshot_id = published.run().snapshot_id();
        if evidence.snapshot_id() != snapshot_id {
            return Err(ModuleCardVerificationError::SnapshotMismatch);
        }
        candidates.sort_by_key(|candidate| candidate.proposal().id());
        if candidates
            .windows(2)
            .any(|pair| pair[0].proposal().id() == pair[1].proposal().id())
        {
            return Err(ModuleCardVerificationError::DuplicateCard);
        }
        validate_candidates(published, &candidates)?;
        let resolved = validate_resolved_evidence(published, evidence)?;
        validate_exact_evidence_set(&candidates, &resolved)?;
        let contradictions = find_contradictions(&candidates);
        if !contradictions.is_empty() {
            return Err(ModuleCardVerificationError::Contradictions(
                ModuleCardContradictionReport { contradictions },
            ));
        }

        let mut cards = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let mut claims = Vec::with_capacity(candidate.claims().len());
            for claim in candidate.claims() {
                claims.push(verify_claim(published, claim, &resolved)?);
            }
            cards.push(VerifiedModuleCard {
                proposal: candidate.proposal,
                claims,
            });
        }
        Ok(VerifiedModuleCardBatch {
            index_run_id: published.run().id(),
            snapshot_id,
            cards,
        })
    }
}

fn validate_candidates(
    published: &PublishedIndex,
    candidates: &[ModuleCardVerificationCandidate],
) -> Result<(), ModuleCardVerificationError> {
    for candidate in candidates {
        let proposal = candidate.proposal();
        if proposal.snapshot_id() != published.run().snapshot_id() {
            return Err(ModuleCardVerificationError::SnapshotMismatch);
        }
        if !published
            .publication()
            .modules()
            .modules()
            .iter()
            .any(|module| module.id() == proposal.module_id())
        {
            return Err(ModuleCardVerificationError::UnknownModule);
        }
    }
    Ok(())
}

fn validate_resolved_evidence<'a>(
    published: &PublishedIndex,
    evidence: &'a ResolvedModuleCardEvidenceSet,
) -> Result<
    BTreeMap<ModuleCardEvidenceId, &'a ResolvedModuleCardEvidence>,
    ModuleCardVerificationError,
> {
    let graph = published.publication().graph();
    let mut resolved = BTreeMap::new();
    for item in evidence.evidence() {
        let (current, expected_id) = match item {
            ResolvedModuleCardEvidence::File { revision, .. } => (
                graph.files().iter().any(|current| current == revision),
                ModuleCardEvidenceId::for_file_revision_v1(revision),
            ),
            ResolvedModuleCardEvidence::Symbol { symbol, .. } => (
                graph.symbols().iter().any(|current| current == symbol),
                ModuleCardEvidenceId::for_symbol_v1(symbol),
            ),
            ResolvedModuleCardEvidence::GraphEdge { edge, .. } => (
                graph.edges().iter().any(|current| current == edge),
                ModuleCardEvidenceId::for_graph_edge_v1(edge),
            ),
        };
        if !current || item.id() != expected_id {
            return Err(ModuleCardVerificationError::StaleOrFabricatedEvidence);
        }
        resolved.insert(item.id(), item);
    }
    Ok(resolved)
}

fn validate_exact_evidence_set(
    candidates: &[ModuleCardVerificationCandidate],
    resolved: &BTreeMap<ModuleCardEvidenceId, &ResolvedModuleCardEvidence>,
) -> Result<(), ModuleCardVerificationError> {
    let expected = candidates
        .iter()
        .flat_map(|candidate| candidate.proposal().evidence_ids().iter().copied())
        .collect::<BTreeSet<_>>();
    let actual = resolved.keys().copied().collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(ModuleCardVerificationError::UnresolvedOrUnexpectedEvidence);
    }
    Ok(())
}

fn find_contradictions(
    candidates: &[ModuleCardVerificationCandidate],
) -> Vec<ModuleCardContradiction> {
    let mut seen = BTreeMap::<ModuleClaimPredicate, (ModuleCardId, ModuleClaimPolarity)>::new();
    let mut contradictions = Vec::new();
    for candidate in candidates {
        for claim in candidate.claims() {
            if matches!(
                claim.predicate(),
                ModuleClaimPredicate::ArchitecturalIntent(_) | ModuleClaimPredicate::Observed(_)
            ) {
                continue;
            }
            match seen.get(claim.predicate()) {
                Some((card_id, polarity)) if *polarity != claim.polarity() => {
                    contradictions.push(ModuleCardContradiction {
                        first_card_id: *card_id,
                        second_card_id: claim.card_id(),
                        predicate: claim.predicate().clone(),
                    });
                }
                None => {
                    seen.insert(
                        claim.predicate().clone(),
                        (claim.card_id(), claim.polarity()),
                    );
                }
                _ => {}
            }
        }
    }
    contradictions
}

fn verify_claim(
    published: &PublishedIndex,
    claim: &ModuleClaimProposal,
    resolved: &BTreeMap<ModuleCardEvidenceId, &ResolvedModuleCardEvidence>,
) -> Result<VerifiedModuleClaim, ModuleCardVerificationError> {
    let graph = published.publication().graph();
    let kind = match claim.predicate() {
        ModuleClaimPredicate::ArchitecturalIntent(_) => VerifiedClaimKind::Hypothesis,
        ModuleClaimPredicate::Observed(_) => {
            ensure_claim_evidence_resolved(claim, resolved)?;
            VerifiedClaimKind::Observation
        }
        predicate if claim.polarity() == ModuleClaimPolarity::Denies => {
            ensure_endpoints_exist(graph, predicate)?;
            VerifiedClaimKind::Hypothesis
        }
        ModuleClaimPredicate::Path(path) => {
            if !graph.files().iter().any(|revision| revision.path() == path)
                || !claim.evidence_ids().iter().any(|id| {
                    matches!(resolved.get(id), Some(ResolvedModuleCardEvidence::File { revision, .. }) if revision.path() == path)
                })
            {
                return Err(ModuleCardVerificationError::UnsupportedDeterministicClaim);
            }
            VerifiedClaimKind::Fact
        }
        ModuleClaimPredicate::Symbol(symbol_id) => {
            if !graph
                .symbols()
                .iter()
                .any(|symbol| symbol.id() == *symbol_id)
            {
                return Err(ModuleCardVerificationError::FabricatedSymbolId);
            }
            if !claim.evidence_ids().iter().any(|id| {
                matches!(resolved.get(id), Some(ResolvedModuleCardEvidence::Symbol { symbol, .. }) if symbol.id() == *symbol_id)
            }) {
                return Err(ModuleCardVerificationError::UnsupportedDeterministicClaim);
            }
            VerifiedClaimKind::Fact
        }
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => {
            ensure_graph_endpoint_exists(graph, source)?;
            ensure_graph_endpoint_exists(graph, target)?;
            if !claim.evidence_ids().iter().any(|id| {
                matches!(resolved.get(id), Some(ResolvedModuleCardEvidence::GraphEdge { edge, .. })
                    if edge.source() == source && edge.target() == target && edge.kind() == *kind)
            }) {
                return Err(ModuleCardVerificationError::UnsupportedDeterministicClaim);
            }
            VerifiedClaimKind::Fact
        }
    };
    Ok(VerifiedModuleClaim {
        proposal: claim.clone(),
        kind,
        status: VerifiedClaimStatus::Active,
    })
}

fn ensure_claim_evidence_resolved(
    claim: &ModuleClaimProposal,
    resolved: &BTreeMap<ModuleCardEvidenceId, &ResolvedModuleCardEvidence>,
) -> Result<(), ModuleCardVerificationError> {
    if claim
        .evidence_ids()
        .iter()
        .all(|id| resolved.contains_key(id))
    {
        Ok(())
    } else {
        Err(ModuleCardVerificationError::UnresolvedOrUnexpectedEvidence)
    }
}

fn ensure_endpoints_exist(
    graph: &super::LinkedGraph,
    predicate: &ModuleClaimPredicate,
) -> Result<(), ModuleCardVerificationError> {
    match predicate {
        ModuleClaimPredicate::Path(path) => {
            if graph.files().iter().any(|revision| revision.path() == path) {
                Ok(())
            } else {
                Err(ModuleCardVerificationError::UnknownPath)
            }
        }
        ModuleClaimPredicate::Symbol(symbol_id) => {
            if graph
                .symbols()
                .iter()
                .any(|symbol| symbol.id() == *symbol_id)
            {
                Ok(())
            } else {
                Err(ModuleCardVerificationError::FabricatedSymbolId)
            }
        }
        ModuleClaimPredicate::Relation { source, target, .. } => {
            ensure_graph_endpoint_exists(graph, source)?;
            ensure_graph_endpoint_exists(graph, target)
        }
        ModuleClaimPredicate::Observed(_) | ModuleClaimPredicate::ArchitecturalIntent(_) => Ok(()),
    }
}

fn ensure_graph_endpoint_exists(
    graph: &super::LinkedGraph,
    endpoint: &GraphEndpoint,
) -> Result<(), ModuleCardVerificationError> {
    let exists = match endpoint {
        GraphEndpoint::File(path) => graph.files().iter().any(|revision| revision.path() == path),
        GraphEndpoint::Symbol(symbol_id) => graph
            .symbols()
            .iter()
            .any(|symbol| symbol.id() == *symbol_id),
    };
    if exists {
        Ok(())
    } else {
        match endpoint {
            GraphEndpoint::File(_) => Err(ModuleCardVerificationError::UnknownPath),
            GraphEndpoint::Symbol(_) => Err(ModuleCardVerificationError::FabricatedSymbolId),
        }
    }
}

/// Bounded contradiction details returned instead of merging opposing Cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCardContradictionReport {
    contradictions: Vec<ModuleCardContradiction>,
}

impl ModuleCardContradictionReport {
    /// Returns every detected opposing typed predicate.
    #[must_use]
    pub fn contradictions(&self) -> &[ModuleCardContradiction] {
        &self.contradictions
    }
}

/// Invalid, stale, fabricated, incomplete, or contradictory verification input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCardVerificationError {
    /// Batch was empty or exceeded 512 cards.
    InvalidCardCount(usize),
    /// Card or evidence belonged to another snapshot.
    SnapshotMismatch,
    /// Proposal referenced an absent module.
    UnknownModule,
    /// More than one candidate used the same logical Card identity.
    DuplicateCard,
    /// Claim count did not exactly cover every field value.
    IncompleteClaimCoverage,
    /// Claim identity or field-item location was repeated.
    DuplicateClaim,
    /// Claim card, module, or snapshot did not match its proposal.
    ClaimEnvelopeMismatch,
    /// Claim used evidence not attached to its field.
    ClaimEvidenceOutsideField,
    /// Observation or intent statement differed from its field value.
    StatementValueMismatch,
    /// Resolver repeated an opaque evidence identity.
    DuplicateResolvedEvidence,
    /// Resolver returned evidence absent or stale in the published index.
    StaleOrFabricatedEvidence,
    /// Required Evidence IDs were absent or unexpected IDs were returned.
    UnresolvedOrUnexpectedEvidence,
    /// A structured claim used a symbol absent from the current graph.
    FabricatedSymbolId,
    /// A structured claim used a path absent from the current graph.
    UnknownPath,
    /// Evidence did not prove the exact typed deterministic proposition.
    UnsupportedDeterministicClaim,
    /// Opposing typed claims were detected and left visible.
    Contradictions(ModuleCardContradictionReport),
}

impl fmt::Display for ModuleCardVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCardCount(_) => "verification batch has an invalid card count",
            Self::SnapshotMismatch => "verification input belongs to another snapshot",
            Self::UnknownModule => "verification proposal references an unknown module",
            Self::DuplicateCard => "verification batch repeats a Module Card identity",
            Self::IncompleteClaimCoverage => "claims do not cover every proposed field value",
            Self::DuplicateClaim => "verification candidate repeats a claim or field item",
            Self::ClaimEnvelopeMismatch => "claim envelope does not match its Module Card",
            Self::ClaimEvidenceOutsideField => "claim evidence is not attached to its field",
            Self::StatementValueMismatch => "claim statement differs from its field value",
            Self::DuplicateResolvedEvidence => "resolver repeated an Evidence ID",
            Self::StaleOrFabricatedEvidence => "resolved evidence is stale or fabricated",
            Self::UnresolvedOrUnexpectedEvidence => "resolved evidence set is not exact",
            Self::FabricatedSymbolId => "claim references a fabricated or stale Symbol ID",
            Self::UnknownPath => "claim references a path absent from the published graph",
            Self::UnsupportedDeterministicClaim => "evidence does not prove the typed claim",
            Self::Contradictions(_) => "opposing Module Card claims require review",
        })
    }
}

impl Error for ModuleCardVerificationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Centrality, Confidence, ContentHash, EvidenceRef, GraphEdge, IndexLanguage,
        IndexPublication, IndexRunRecord, IndexRunSequence, IndexRunStatus, LinkResolution,
        LinkedGraph, LocalSymbolId, MapperProfileVersion, ModuleCardProposalEnvelope,
        ModuleCardSchemaVersion, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
        ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, ParsedSymbol,
        ProposedModuleCardField, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
        RepositoryModule, SourcePosition, SourceRange, SymbolKind, SymbolName, SymbolRank,
        SymbolRankSignals, SymbolRole, SymbolVisibility, SyntaxProvider,
    };

    #[test]
    fn verifier_assigns_fact_observation_and_hypothesis_without_conflating_confidence()
    -> Result<(), Box<dyn Error>> {
        let fixture = verification_fixture()?;
        let batch = ModuleCardVerifier::verify(
            &fixture.published,
            vec![fixture.candidate],
            &fixture.evidence,
        )?;

        assert_eq!(batch.cards().len(), 1);
        assert_eq!(batch.cards()[0].status(), ModuleCardStatus::Verified);
        let kinds = batch.cards()[0]
            .claims()
            .iter()
            .map(VerifiedModuleClaim::kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                VerifiedClaimKind::Observation,
                VerifiedClaimKind::Hypothesis,
                VerifiedClaimKind::Fact,
                VerifiedClaimKind::Fact,
                VerifiedClaimKind::Fact,
                VerifiedClaimKind::Fact,
            ]
        );
        assert!(
            batch.cards()[0]
                .claims()
                .iter()
                .all(|claim| claim.confidence().basis_points() == 7_000)
        );
        Ok(())
    }

    #[test]
    fn fabricated_symbol_id_is_rejected_even_with_current_evidence() -> Result<(), Box<dyn Error>> {
        let fixture = verification_fixture()?;
        let proposal = one_field_proposal(
            71,
            fixture.module_id,
            fixture.published.run().snapshot_id(),
            fixture.symbol_evidence,
        )?;
        let claim = ModuleClaimProposal::new(
            ModuleClaimEnvelope::new(
                ModuleCardClaimId::from_bytes([72; 32]),
                proposal.id(),
                fixture.module_id,
                proposal.snapshot_id(),
                ModuleCardField::PublicSurface,
                0,
                Confidence::certain(),
            ),
            ModuleClaimPolarity::Affirms,
            ModuleClaimPredicate::Symbol(SymbolId::from_bytes([250; 32])),
            vec![fixture.symbol_evidence],
        )?;
        let candidate = ModuleCardVerificationCandidate::new(proposal, vec![claim])?;
        let evidence = ResolvedModuleCardEvidenceSet::new(
            fixture.published.run().snapshot_id(),
            vec![ResolvedModuleCardEvidence::Symbol {
                id: fixture.symbol_evidence,
                symbol: fixture.published.publication().graph().symbols()[0].clone(),
            }],
        )?;
        assert_eq!(
            ModuleCardVerifier::verify(&fixture.published, vec![candidate], &evidence),
            Err(ModuleCardVerificationError::FabricatedSymbolId)
        );
        Ok(())
    }

    #[test]
    fn opposing_cards_are_reported_and_never_merged() -> Result<(), Box<dyn Error>> {
        let fixture = verification_fixture()?;
        let symbol = fixture.published.publication().graph().symbols()[0].clone();
        let first = single_symbol_candidate(
            81,
            fixture.module_id,
            fixture.published.run().snapshot_id(),
            fixture.symbol_evidence,
            symbol.id(),
            ModuleClaimPolarity::Affirms,
        )?;
        let second = single_symbol_candidate(
            82,
            fixture.module_id,
            fixture.published.run().snapshot_id(),
            fixture.symbol_evidence,
            symbol.id(),
            ModuleClaimPolarity::Denies,
        )?;
        let evidence = ResolvedModuleCardEvidenceSet::new(
            fixture.published.run().snapshot_id(),
            vec![ResolvedModuleCardEvidence::Symbol {
                id: fixture.symbol_evidence,
                symbol,
            }],
        )?;

        let result = ModuleCardVerifier::verify(&fixture.published, vec![first, second], &evidence);
        let Err(ModuleCardVerificationError::Contradictions(report)) = result else {
            return Err("expected visible contradiction report".into());
        };
        assert_eq!(report.contradictions().len(), 1);
        assert_ne!(
            report.contradictions()[0].first_card_id(),
            report.contradictions()[0].second_card_id()
        );
        Ok(())
    }

    #[test]
    fn verification_debug_output_redacts_paths_symbols_and_claim_text() -> Result<(), Box<dyn Error>>
    {
        let fixture = verification_fixture()?;
        let candidate_debug = format!("{:?}", fixture.candidate);
        let evidence_debug = format!("{:?}", fixture.evidence);

        assert!(!candidate_debug.contains("designed to isolate unsafe work"));
        assert!(!candidate_debug.contains("current source observation"));
        assert!(!candidate_debug.contains("src/lib.rs"));
        assert!(!evidence_debug.contains("src/lib.rs"));
        assert!(!evidence_debug.contains("run_test"));
        Ok(())
    }

    fn single_symbol_candidate(
        card_byte: u8,
        module_id: ModuleId,
        snapshot_id: SnapshotId,
        evidence_id: ModuleCardEvidenceId,
        symbol_id: SymbolId,
        polarity: ModuleClaimPolarity,
    ) -> Result<ModuleCardVerificationCandidate, Box<dyn Error>> {
        let proposal = one_field_proposal(card_byte, module_id, snapshot_id, evidence_id)?;
        let claim = ModuleClaimProposal::new(
            ModuleClaimEnvelope::new(
                ModuleCardClaimId::from_bytes([card_byte.wrapping_add(100); 32]),
                proposal.id(),
                module_id,
                snapshot_id,
                ModuleCardField::PublicSurface,
                0,
                Confidence::certain(),
            ),
            polarity,
            ModuleClaimPredicate::Symbol(symbol_id),
            vec![evidence_id],
        )?;
        Ok(ModuleCardVerificationCandidate::new(proposal, vec![claim])?)
    }

    fn one_field_proposal(
        card_byte: u8,
        module_id: ModuleId,
        snapshot_id: SnapshotId,
        evidence_id: ModuleCardEvidenceId,
    ) -> Result<ModuleCardProposal, Box<dyn Error>> {
        Ok(ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                ModuleCardId::from_bytes([card_byte; 32]),
                module_id,
                snapshot_id,
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::certain(),
            ),
            vec![ProposedModuleCardField::new(
                ModuleCardField::PublicSurface,
                vec!["public symbol".to_owned()],
                vec![evidence_id],
            )?],
            512,
        )?)
    }

    struct VerificationFixture {
        published: PublishedIndex,
        candidate: ModuleCardVerificationCandidate,
        evidence: ResolvedModuleCardEvidenceSet,
        module_id: ModuleId,
        symbol_evidence: ModuleCardEvidenceId,
    }

    fn verification_fixture() -> Result<VerificationFixture, Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let manifest = revision("Cargo.toml", 2)?;
        let source = revision("src/lib.rs", 3)?;
        let source_symbol =
            graph_symbol(10, 1, source.clone(), "run", Some(SymbolRole::Entrypoint))?;
        let dependency_symbol = graph_symbol(11, 2, source.clone(), "dependency", None)?;
        let test_symbol = graph_symbol(12, 3, source.clone(), "run_test", Some(SymbolRole::Test))?;
        let source_endpoint = GraphEndpoint::Symbol(source_symbol.id());
        let dependency_endpoint = GraphEndpoint::Symbol(dependency_symbol.id());
        let test_endpoint = GraphEndpoint::Symbol(test_symbol.id());
        let source_file = GraphEndpoint::File(source.path().clone());
        let edges = vec![
            edge(
                snapshot_id,
                source_endpoint.clone(),
                dependency_endpoint.clone(),
                SyntaxRelationKind::Imports,
                &source,
            )?,
            edge(
                snapshot_id,
                source_file,
                source_endpoint.clone(),
                SyntaxRelationKind::Exports,
                &source,
            )?,
            edge(
                snapshot_id,
                source_endpoint.clone(),
                dependency_endpoint,
                SyntaxRelationKind::Calls,
                &source,
            )?,
            edge(
                snapshot_id,
                test_endpoint,
                source_endpoint,
                SyntaxRelationKind::Tests,
                &source,
            )?,
        ];
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![manifest.clone(), source.clone()],
            vec![
                source_symbol.clone(),
                dependency_symbol.clone(),
                test_symbol.clone(),
            ],
            edges.clone(),
            Vec::new(),
        )?;
        let ranks = [source_symbol.id(), dependency_symbol.id(), test_symbol.id()]
            .into_iter()
            .map(|symbol_id| -> Result<SymbolRank, Box<dyn Error>> {
                Ok(SymbolRank::new(
                    symbol_id,
                    RankScore::try_from_sum(1_000)?,
                    SymbolRankSignals {
                        in_degree: 0,
                        out_degree: 0,
                        centrality: Centrality::from_basis_points(1_000)?,
                        degree_contribution: 0,
                        centrality_contribution: 1_000,
                        entrypoint_contribution: 0,
                        public_export_contribution: 0,
                        manifest_contribution: 0,
                        test_contribution: 0,
                    },
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
        let module_id = ModuleId::from_bytes([20; 32]);
        let all_symbols = ModuleSymbolSet::new(
            vec![source_symbol.id(), dependency_symbol.id(), test_symbol.id()],
            false,
        )?;
        let module = RepositoryModule::new(
            module_id,
            ModuleKind::ManifestBoundary,
            Some(ModuleRoot::Repository),
            vec![manifest.clone()],
            all_symbols.clone(),
            ModuleSymbolSet::new(vec![source_symbol.id()], false)?,
            ModuleSymbolSet::new(vec![test_symbol.id()], false)?,
        )?;
        let memberships = [source_symbol.id(), dependency_symbol.id(), test_symbol.id()]
            .into_iter()
            .map(|symbol_id| {
                ModuleMembership::new(
                    module_id,
                    symbol_id,
                    ModuleMembershipEvidence::manifest(source.clone(), manifest.clone()),
                )
            })
            .collect::<Vec<_>>();
        let repository_card = RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module_id],
            vec![IndexLanguage::Rust],
            ModuleSymbolSet::new(vec![source_symbol.id()], false)?,
            2,
            3,
        )?;
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module],
            memberships,
            repository_card,
        )?;
        let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([30; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        let published = PublishedIndex::new(run, publication)?;

        let evidence_ids = [
            ModuleCardEvidenceId::for_graph_edge_v1(&edges[0]),
            ModuleCardEvidenceId::for_graph_edge_v1(&edges[1]),
            ModuleCardEvidenceId::for_graph_edge_v1(&edges[2]),
            ModuleCardEvidenceId::for_graph_edge_v1(&edges[3]),
        ];
        let symbol_evidence = ModuleCardEvidenceId::for_symbol_v1(&source_symbol);
        let card_id = ModuleCardId::from_bytes([50; 32]);
        let fields = vec![
            field(
                ModuleCardField::Title,
                "current source observation",
                symbol_evidence,
            )?,
            field(
                ModuleCardField::Purpose,
                "designed to isolate unsafe work",
                symbol_evidence,
            )?,
            field(
                ModuleCardField::PublicSurface,
                "exports run",
                evidence_ids[1],
            )?,
            field(
                ModuleCardField::Dependencies,
                "imports dependency",
                evidence_ids[0],
            )?,
            field(
                ModuleCardField::DataFlows,
                "calls dependency",
                evidence_ids[2],
            )?,
            field(
                ModuleCardField::Tests,
                "run_test covers run",
                evidence_ids[3],
            )?,
        ];
        let proposal = ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                card_id,
                module_id,
                snapshot_id,
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::from_basis_points(8_000)?,
            ),
            fields,
            2_048,
        )?;
        let confidence = Confidence::from_basis_points(7_000)?;
        let claims = vec![
            prose_claim(
                51,
                &proposal,
                ModuleCardField::Title,
                ModuleClaimPredicate::Observed(ModuleClaimStatement::try_from_string(
                    "current source observation".to_owned(),
                )?),
                vec![symbol_evidence],
                confidence,
            )?,
            prose_claim(
                52,
                &proposal,
                ModuleCardField::Purpose,
                ModuleClaimPredicate::ArchitecturalIntent(ModuleClaimStatement::try_from_string(
                    "designed to isolate unsafe work".to_owned(),
                )?),
                Vec::new(),
                confidence,
            )?,
            relation_claim(
                53,
                &proposal,
                ModuleCardField::PublicSurface,
                &edges[1],
                evidence_ids[1],
                confidence,
            )?,
            relation_claim(
                54,
                &proposal,
                ModuleCardField::Dependencies,
                &edges[0],
                evidence_ids[0],
                confidence,
            )?,
            relation_claim(
                55,
                &proposal,
                ModuleCardField::DataFlows,
                &edges[2],
                evidence_ids[2],
                confidence,
            )?,
            relation_claim(
                56,
                &proposal,
                ModuleCardField::Tests,
                &edges[3],
                evidence_ids[3],
                confidence,
            )?,
        ];
        let candidate = ModuleCardVerificationCandidate::new(proposal, claims)?;
        let mut resolved = edges
            .into_iter()
            .enumerate()
            .map(|(index, edge)| ResolvedModuleCardEvidence::GraphEdge {
                id: evidence_ids[index],
                edge,
            })
            .collect::<Vec<_>>();
        resolved.push(ResolvedModuleCardEvidence::Symbol {
            id: symbol_evidence,
            symbol: source_symbol,
        });
        let evidence = ResolvedModuleCardEvidenceSet::new(snapshot_id, resolved)?;
        Ok(VerificationFixture {
            published,
            candidate,
            evidence,
            module_id,
            symbol_evidence,
        })
    }

    fn prose_claim(
        id: u8,
        proposal: &ModuleCardProposal,
        field: ModuleCardField,
        predicate: ModuleClaimPredicate,
        evidence: Vec<ModuleCardEvidenceId>,
        confidence: Confidence,
    ) -> Result<ModuleClaimProposal, Box<dyn Error>> {
        Ok(ModuleClaimProposal::new(
            claim_envelope(id, proposal, field, confidence),
            ModuleClaimPolarity::Affirms,
            predicate,
            evidence,
        )?)
    }

    fn relation_claim(
        id: u8,
        proposal: &ModuleCardProposal,
        field: ModuleCardField,
        edge: &GraphEdge,
        evidence: ModuleCardEvidenceId,
        confidence: Confidence,
    ) -> Result<ModuleClaimProposal, Box<dyn Error>> {
        Ok(ModuleClaimProposal::new(
            claim_envelope(id, proposal, field, confidence),
            ModuleClaimPolarity::Affirms,
            ModuleClaimPredicate::Relation {
                source: edge.source().clone(),
                target: edge.target().clone(),
                kind: edge.kind(),
            },
            vec![evidence],
        )?)
    }

    fn claim_envelope(
        id: u8,
        proposal: &ModuleCardProposal,
        field: ModuleCardField,
        confidence: Confidence,
    ) -> ModuleClaimEnvelope {
        ModuleClaimEnvelope::new(
            ModuleCardClaimId::from_bytes([id; 32]),
            proposal.id(),
            proposal.module_id(),
            proposal.snapshot_id(),
            field,
            0,
            confidence,
        )
    }

    fn field(
        field: ModuleCardField,
        value: &str,
        evidence: ModuleCardEvidenceId,
    ) -> Result<ProposedModuleCardField, Box<dyn Error>> {
        Ok(ProposedModuleCardField::new(
            field,
            vec![value.to_owned()],
            vec![evidence],
        )?)
    }

    fn graph_symbol(
        id: u8,
        local_id: u32,
        revision: FileRevision,
        name: &str,
        role: Option<SymbolRole>,
    ) -> Result<GraphSymbol, Box<dyn Error>> {
        let range = source_range()?;
        let mut parsed = ParsedSymbol::new(
            LocalSymbolId::new(local_id)?,
            SymbolKind::Function,
            SymbolName::try_from_string(name.to_owned())?,
            range,
            range,
        )?
        .with_visibility(SymbolVisibility::Public);
        if let Some(role) = role {
            parsed = parsed.with_role(role);
        }
        Ok(GraphSymbol::new(
            SymbolId::from_bytes([id; 32]),
            revision,
            parsed,
        ))
    }

    fn edge(
        snapshot_id: SnapshotId,
        source: GraphEndpoint,
        target: GraphEndpoint,
        kind: SyntaxRelationKind,
        revision: &FileRevision,
    ) -> Result<GraphEdge, Box<dyn Error>> {
        Ok(GraphEdge::new(
            source,
            target,
            kind,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterLocalSymbol,
            snapshot_id,
            EvidenceRef::new(revision.clone(), source_range()?),
        ))
    }

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }

    fn source_range() -> Result<SourceRange, Box<dyn Error>> {
        Ok(SourceRange::new(
            0,
            1,
            SourcePosition::new(0, 0),
            SourcePosition::new(0, 1),
        )?)
    }
}
