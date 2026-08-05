use super::{
    EvidenceRef, ExactSearchExplanation, ExactSearchHit, ExactSearchPage, ExactSearchTarget,
    GraphEdge, GraphTraversalHit, GraphTraversalResult, IndexRunId, LexicalScore,
    LexicalSearchExplanation, LexicalSearchHit, LexicalSearchPage, RepositoryPath, SnapshotId,
    SourceChannel, SymbolId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const MAX_NORMALIZED_SIGNAL: u16 = 10_000;
const MAX_CANDIDATES_PER_SET: usize = 100;
const MAX_CANDIDATE_SETS: usize = 6;
const MAX_CANDIDATE_TOKEN_COST: u32 = 65_535;
const MAX_FUSION_RESULTS: u16 = 100;
const MAX_FUSION_SCORE: u32 = 100_000;
const MAX_MEMORY_EVIDENCE_REFS: usize = 16;

/// Normalized deterministic relevance or overlap strength in basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalizedRetrievalSignal(u16);

impl NormalizedRetrievalSignal {
    /// No contribution or overlap.
    pub const ZERO: Self = Self(0);
    /// Maximum normalized contribution or overlap.
    pub const FULL: Self = Self(MAX_NORMALIZED_SIGNAL);

    /// Creates a signal within the closed basis-point range.
    pub fn new(value: u16) -> Result<Self, NormalizedRetrievalSignalError> {
        if value > MAX_NORMALIZED_SIGNAL {
            return Err(NormalizedRetrievalSignalError { value });
        }
        Ok(Self(value))
    }

    /// Returns the stable basis-point representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Normalized retrieval signal outside zero through 10,000 basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedRetrievalSignalError {
    value: u16,
}

impl fmt::Display for NormalizedRetrievalSignalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "retrieval signal {} exceeds {MAX_NORMALIZED_SIGNAL} basis points",
            self.value
        )
    }
}

impl Error for NormalizedRetrievalSignalError {}

/// Freshness states admitted into fact-bearing retrieval fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CandidateFreshness {
    /// Candidate and evidence belong directly to the selected snapshot.
    Current,
    /// Candidate evidence is explicitly compatible with the selected snapshot.
    Compatible,
}

impl CandidateFreshness {
    const fn signal(self) -> NormalizedRetrievalSignal {
        match self {
            Self::Current => NormalizedRetrievalSignal::FULL,
            Self::Compatible => NormalizedRetrievalSignal(7_000),
        }
    }
}

/// Estimated tokens required to expose one candidate to a model later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CandidateTokenCost(u32);

impl CandidateTokenCost {
    /// Creates a positive, bounded token estimate.
    pub fn new(value: u32) -> Result<Self, CandidateTokenCostError> {
        if value == 0 || value > MAX_CANDIDATE_TOKEN_COST {
            return Err(CandidateTokenCostError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded token estimate.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Candidate token estimate outside the product boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateTokenCostError {
    value: u32,
}

impl fmt::Display for CandidateTokenCostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "candidate token cost {} must be between 1 and {MAX_CANDIDATE_TOKEN_COST}",
            self.value
        )
    }
}

impl Error for CandidateTokenCostError {}

/// Deterministic target-level signals supplied equally to every channel copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RetrievalCandidateSignals {
    goal_relevance: NormalizedRetrievalSignal,
    step_relevance: NormalizedRetrievalSignal,
    freshness: CandidateFreshness,
    token_cost: CandidateTokenCost,
    redundancy: NormalizedRetrievalSignal,
}

impl RetrievalCandidateSignals {
    /// Binds all version-one ranking inputs to one candidate target.
    #[must_use]
    pub const fn new(
        goal_relevance: NormalizedRetrievalSignal,
        step_relevance: NormalizedRetrievalSignal,
        freshness: CandidateFreshness,
        token_cost: CandidateTokenCost,
        redundancy: NormalizedRetrievalSignal,
    ) -> Self {
        Self {
            goal_relevance,
            step_relevance,
            freshness,
            token_cost,
            redundancy,
        }
    }

    /// Returns relevance to the durable goal contract.
    #[must_use]
    pub const fn goal_relevance(self) -> NormalizedRetrievalSignal {
        self.goal_relevance
    }

    /// Returns relevance to the current task step.
    #[must_use]
    pub const fn step_relevance(self) -> NormalizedRetrievalSignal {
        self.step_relevance
    }

    /// Returns evidence freshness admitted by the candidate producer.
    #[must_use]
    pub const fn freshness(self) -> CandidateFreshness {
        self.freshness
    }

    /// Returns the estimated later context cost.
    #[must_use]
    pub const fn token_cost(self) -> CandidateTokenCost {
        self.token_cost
    }

    /// Returns estimated overlap with already represented information.
    #[must_use]
    pub const fn redundancy(self) -> NormalizedRetrievalSignal {
        self.redundancy
    }
}

/// Stable identity used to merge the same current target across channels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RetrievalTargetId {
    /// Canonical repository-relative path, unique within one published run.
    File(RepositoryPath),
    /// Content- and adapter-derived symbol identity.
    Symbol(SymbolId),
}

impl RetrievalTargetId {
    /// Derives an identity without depending on a storage row or display string.
    #[must_use]
    pub fn from_target(target: &ExactSearchTarget) -> Self {
        match target {
            ExactSearchTarget::File(revision) => Self::File(revision.path().clone()),
            ExactSearchTarget::Symbol(symbol) => Self::Symbol(symbol.symbol().id()),
        }
    }
}

/// Validated shortest relationship path retained in a fusion explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipCandidateExplanation {
    path: Vec<GraphEdge>,
}

/// Fresh evidence-grounded memory reason admitted into the evidence priority band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCandidateExplanation {
    relevance: NormalizedRetrievalSignal,
    evidence: Vec<EvidenceRef>,
}

impl MemoryCandidateExplanation {
    /// Creates a bounded memory reason that cannot exist without source evidence.
    pub fn new(
        relevance: NormalizedRetrievalSignal,
        mut evidence: Vec<EvidenceRef>,
    ) -> Result<Self, MemoryCandidateExplanationError> {
        if evidence.is_empty() || evidence.len() > MAX_MEMORY_EVIDENCE_REFS {
            return Err(MemoryCandidateExplanationError {
                evidence_count: evidence.len(),
            });
        }
        evidence.sort_by(|left, right| {
            left.revision()
                .path()
                .cmp(right.revision().path())
                .then_with(|| {
                    left.revision()
                        .content_hash()
                        .cmp(&right.revision().content_hash())
                })
                .then_with(|| left.range().cmp(&right.range()))
        });
        evidence.dedup();
        Ok(Self {
            relevance,
            evidence,
        })
    }

    /// Returns relevance assigned by the deterministic memory candidate producer.
    #[must_use]
    pub const fn relevance(&self) -> NormalizedRetrievalSignal {
        self.relevance
    }

    /// Returns the resolved fresh evidence retained for later validation and context.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceRef] {
        &self.evidence
    }
}

/// Memory candidate without a bounded non-empty evidence set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryCandidateExplanationError {
    evidence_count: usize,
}

impl fmt::Display for MemoryCandidateExplanationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "memory candidate has {} evidence references; expected 1 through {MAX_MEMORY_EVIDENCE_REFS}",
            self.evidence_count
        )
    }
}

impl Error for MemoryCandidateExplanationError {}

impl RelationshipCandidateExplanation {
    fn from_hit(hit: &GraphTraversalHit) -> Self {
        Self {
            path: hit.path().to_vec(),
        }
    }

    /// Returns the ordered graph evidence from seed to target.
    #[must_use]
    pub fn path(&self) -> &[GraphEdge] {
        &self.path
    }
}

/// Channel-specific reason and native score retained before normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalCandidateReason {
    /// Exact path, identifier, signature, prefix, or structural role.
    Exact(ExactSearchExplanation),
    /// Weighted lexical match with its strongest field and native score.
    Lexical {
        /// Strongest projected field.
        explanation: LexicalSearchExplanation,
        /// Deterministic native lexical score.
        score: LexicalScore,
    },
    /// Non-test evidence-graph relationship.
    Graph(RelationshipCandidateExplanation),
    /// Dedicated test relationship.
    Test(RelationshipCandidateExplanation),
    /// Fresh evidence-grounded memory relevance and source references.
    Memory(MemoryCandidateExplanation),
    /// Similarity candidate that is explicitly not factual evidence.
    Semantic(NormalizedRetrievalSignal),
}

impl RetrievalCandidateReason {
    /// Returns the independent candidate channel represented by this reason.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        match self {
            Self::Exact(_) => SourceChannel::Exact,
            Self::Lexical { .. } => SourceChannel::Lexical,
            Self::Graph(_) => SourceChannel::Graph,
            Self::Test(_) => SourceChannel::Test,
            Self::Memory(_) => SourceChannel::Memory,
            Self::Semantic(_) => SourceChannel::Semantic,
        }
    }

    fn normalized_score(&self) -> NormalizedRetrievalSignal {
        match self {
            Self::Exact(explanation) => exact_reason_score(*explanation),
            Self::Lexical { score, .. } => {
                let normalized = score.get().saturating_mul(10_000) / 100_000;
                NormalizedRetrievalSignal(
                    u16::try_from(normalized).unwrap_or(MAX_NORMALIZED_SIGNAL),
                )
            }
            Self::Graph(explanation) | Self::Test(explanation) => relationship_score(explanation),
            Self::Memory(explanation) => explanation.relevance(),
            Self::Semantic(relevance) => *relevance,
        }
    }
}

fn exact_reason_score(explanation: ExactSearchExplanation) -> NormalizedRetrievalSignal {
    let score = match explanation {
        ExactSearchExplanation::NormalizedPathExact
        | ExactSearchExplanation::QualifiedNameExact => 10_000,
        ExactSearchExplanation::SymbolNameExact => 9_500,
        ExactSearchExplanation::SignatureExact
        | ExactSearchExplanation::ManifestRole
        | ExactSearchExplanation::EntrypointRole
        | ExactSearchExplanation::TestRole => 9_000,
        ExactSearchExplanation::QualifiedNamePrefix => 8_000,
        ExactSearchExplanation::SymbolNamePrefix => 7_500,
        ExactSearchExplanation::SignaturePrefix => 7_000,
    };
    NormalizedRetrievalSignal(score)
}

fn relationship_score(explanation: &RelationshipCandidateExplanation) -> NormalizedRetrievalSignal {
    let confidence = explanation
        .path()
        .iter()
        .map(|edge| edge.confidence().basis_points())
        .min()
        .unwrap_or(0);
    let distance_factor = if explanation.path().len() == 1 {
        10_000_u32
    } else {
        7_000_u32
    };
    let normalized = u32::from(confidence).saturating_mul(distance_factor) / 10_000;
    NormalizedRetrievalSignal(u16::try_from(normalized).unwrap_or(MAX_NORMALIZED_SIGNAL))
}

/// One target candidate before cross-channel deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalCandidate {
    target: ExactSearchTarget,
    reason: RetrievalCandidateReason,
    signals: RetrievalCandidateSignals,
}

impl RetrievalCandidate {
    /// Converts an exact-search hit without discarding its native explanation.
    #[must_use]
    pub fn from_exact(hit: &ExactSearchHit, signals: RetrievalCandidateSignals) -> Self {
        Self {
            target: hit.target().clone(),
            reason: RetrievalCandidateReason::Exact(hit.explanation()),
            signals,
        }
    }

    /// Converts a lexical hit without discarding field or native score.
    #[must_use]
    pub fn from_lexical(hit: &LexicalSearchHit, signals: RetrievalCandidateSignals) -> Self {
        Self {
            target: hit.target().clone(),
            reason: RetrievalCandidateReason::Lexical {
                explanation: hit.explanation(),
                score: hit.score(),
            },
            signals,
        }
    }

    /// Converts a validated graph or test hit with its complete evidence path.
    #[must_use]
    pub fn from_relationship(hit: &GraphTraversalHit, signals: RetrievalCandidateSignals) -> Self {
        let explanation = RelationshipCandidateExplanation::from_hit(hit);
        let reason = if hit.source_channel() == SourceChannel::Test {
            RetrievalCandidateReason::Test(explanation)
        } else {
            RetrievalCandidateReason::Graph(explanation)
        };
        Self {
            target: hit.target().clone(),
            reason,
            signals,
        }
    }

    /// Creates a fresh evidence-grounded memory candidate.
    #[must_use]
    pub const fn memory(
        target: ExactSearchTarget,
        explanation: MemoryCandidateExplanation,
        signals: RetrievalCandidateSignals,
    ) -> Self {
        Self {
            target,
            reason: RetrievalCandidateReason::Memory(explanation),
            signals,
        }
    }

    /// Creates a semantic candidate without promoting similarity to evidence.
    #[must_use]
    pub const fn semantic(
        target: ExactSearchTarget,
        similarity: NormalizedRetrievalSignal,
        signals: RetrievalCandidateSignals,
    ) -> Self {
        Self {
            target,
            reason: RetrievalCandidateReason::Semantic(similarity),
            signals,
        }
    }

    /// Returns the current evidence-bearing target.
    #[must_use]
    pub const fn target(&self) -> &ExactSearchTarget {
        &self.target
    }

    /// Returns the stable target identity used for deduplication.
    #[must_use]
    pub fn target_id(&self) -> RetrievalTargetId {
        RetrievalTargetId::from_target(&self.target)
    }

    /// Returns the channel-native reason.
    #[must_use]
    pub const fn reason(&self) -> &RetrievalCandidateReason {
        &self.reason
    }

    /// Returns all target-level ranking inputs.
    #[must_use]
    pub const fn signals(&self) -> RetrievalCandidateSignals {
        self.signals
    }
}

/// Whether a channel supplied every candidate or stopped at an upstream boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CandidateSetCompleteness {
    /// The candidate producer reported no omitted result.
    Complete,
    /// Pagination, a result cap, or another visible boundary omitted candidates.
    Truncated,
}

/// Independent candidates produced by exactly one channel and publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalCandidateSet {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    source_channel: SourceChannel,
    completeness: CandidateSetCompleteness,
    candidates: Vec<RetrievalCandidate>,
}

impl RetrievalCandidateSet {
    /// Creates a complete bounded homogeneous set.
    pub fn complete(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        source_channel: SourceChannel,
        candidates: Vec<RetrievalCandidate>,
    ) -> Result<Self, RetrievalCandidateSetError> {
        Self::new(
            index_run_id,
            snapshot_id,
            source_channel,
            CandidateSetCompleteness::Complete,
            candidates,
        )
    }

    /// Creates a visibly truncated bounded homogeneous set.
    pub fn truncated(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        source_channel: SourceChannel,
        candidates: Vec<RetrievalCandidate>,
    ) -> Result<Self, RetrievalCandidateSetError> {
        Self::new(
            index_run_id,
            snapshot_id,
            source_channel,
            CandidateSetCompleteness::Truncated,
            candidates,
        )
    }

    fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        source_channel: SourceChannel,
        completeness: CandidateSetCompleteness,
        candidates: Vec<RetrievalCandidate>,
    ) -> Result<Self, RetrievalCandidateSetError> {
        if candidates.len() > MAX_CANDIDATES_PER_SET {
            return Err(RetrievalCandidateSetError::TooManyCandidates);
        }
        let mut target_ids = BTreeSet::new();
        for candidate in &candidates {
            if candidate.reason().source_channel() != source_channel {
                return Err(RetrievalCandidateSetError::MixedSourceChannel);
            }
            if !target_ids.insert(candidate.target_id()) {
                return Err(RetrievalCandidateSetError::DuplicateTarget);
            }
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            source_channel,
            completeness,
            candidates,
        })
    }

    /// Converts one exact page after pairing every hit with target-level signals.
    pub fn from_exact_page(
        page: &ExactSearchPage,
        signals: &[RetrievalCandidateSignals],
    ) -> Result<Self, RetrievalCandidateSetError> {
        if page.hits().len() != signals.len() {
            return Err(RetrievalCandidateSetError::SignalCountMismatch);
        }
        let candidates = page
            .hits()
            .iter()
            .zip(signals)
            .map(|(hit, signals)| RetrievalCandidate::from_exact(hit, *signals))
            .collect();
        Self::new(
            page.index_run_id(),
            page.snapshot_id(),
            SourceChannel::Exact,
            if page.next_cursor().is_some() {
                CandidateSetCompleteness::Truncated
            } else {
                CandidateSetCompleteness::Complete
            },
            candidates,
        )
    }

    /// Converts one lexical page after pairing every hit with target-level signals.
    pub fn from_lexical_page(
        page: &LexicalSearchPage,
        signals: &[RetrievalCandidateSignals],
    ) -> Result<Self, RetrievalCandidateSetError> {
        if page.hits().len() != signals.len() {
            return Err(RetrievalCandidateSetError::SignalCountMismatch);
        }
        let candidates = page
            .hits()
            .iter()
            .zip(signals)
            .map(|(hit, signals)| RetrievalCandidate::from_lexical(hit, *signals))
            .collect();
        Self::new(
            page.index_run_id(),
            page.snapshot_id(),
            SourceChannel::Lexical,
            if page.next_cursor().is_some() {
                CandidateSetCompleteness::Truncated
            } else {
                CandidateSetCompleteness::Complete
            },
            candidates,
        )
    }

    /// Converts one graph/test result with its native publication and channel.
    pub fn from_graph_result(
        result: &GraphTraversalResult,
        signals: &[RetrievalCandidateSignals],
    ) -> Result<Self, RetrievalCandidateSetError> {
        if result.hits().len() != signals.len() {
            return Err(RetrievalCandidateSetError::SignalCountMismatch);
        }
        let candidates = result
            .hits()
            .iter()
            .zip(signals)
            .map(|(hit, signals)| RetrievalCandidate::from_relationship(hit, *signals))
            .collect();
        Self::new(
            result.index_run_id(),
            result.snapshot_id(),
            result.query().source_channel(),
            if result.truncated() {
                CandidateSetCompleteness::Truncated
            } else {
                CandidateSetCompleteness::Complete
            },
            candidates,
        )
    }

    /// Returns the source publication.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the sole channel represented by this set.
    #[must_use]
    pub const fn source_channel(&self) -> SourceChannel {
        self.source_channel
    }

    /// Returns whether the producer omitted candidates before fusion.
    #[must_use]
    pub const fn completeness(&self) -> CandidateSetCompleteness {
        self.completeness
    }

    /// Returns candidates in their channel-native deterministic order.
    #[must_use]
    pub fn candidates(&self) -> &[RetrievalCandidate] {
        &self.candidates
    }
}

/// Invalid channel-specific candidate set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalCandidateSetError {
    /// A channel returned more candidates than the fusion boundary accepts.
    TooManyCandidates,
    /// At least one candidate used another source channel.
    MixedSourceChannel,
    /// A channel returned the same stable target more than once.
    DuplicateTarget,
    /// Target-level signals did not cover exactly all converted hits.
    SignalCountMismatch,
}

impl fmt::Display for RetrievalCandidateSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooManyCandidates => "retrieval candidate set exceeds its fixed boundary",
            Self::MixedSourceChannel => "retrieval candidate set mixes source channels",
            Self::DuplicateTarget => "retrieval candidate set contains a duplicate target",
            Self::SignalCountMismatch => {
                "retrieval candidate signals do not cover exactly every hit"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for RetrievalCandidateSetError {}

/// All separate channel sets eligible for one deterministic fusion run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalCandidateSets {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    sets: Vec<RetrievalCandidateSet>,
}

impl RetrievalCandidateSets {
    /// Binds unique channel sets to one atomically published run and snapshot.
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        mut sets: Vec<RetrievalCandidateSet>,
    ) -> Result<Self, RetrievalCandidateSetsError> {
        if sets.len() > MAX_CANDIDATE_SETS {
            return Err(RetrievalCandidateSetsError::TooManySets);
        }
        let mut channels = BTreeSet::new();
        for set in &sets {
            if set.index_run_id() != index_run_id || set.snapshot_id() != snapshot_id {
                return Err(RetrievalCandidateSetsError::PublicationMismatch);
            }
            if !channels.insert(set.source_channel()) {
                return Err(RetrievalCandidateSetsError::DuplicateChannel);
            }
        }
        sets.sort_by_key(RetrievalCandidateSet::source_channel);
        Ok(Self {
            index_run_id,
            snapshot_id,
            sets,
        })
    }

    /// Returns the sole published run represented by every set.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the sole snapshot represented by every set.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns channel sets in stable source-channel order.
    #[must_use]
    pub fn sets(&self) -> &[RetrievalCandidateSet] {
        &self.sets
    }
}

/// Incompatible collection of independent candidate sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalCandidateSetsError {
    /// More than one set was supplied for one source channel.
    DuplicateChannel,
    /// A set came from another run or snapshot.
    PublicationMismatch,
    /// More channel sets were supplied than the source-channel model permits.
    TooManySets,
}

impl fmt::Display for RetrievalCandidateSetsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DuplicateChannel => "retrieval candidate sets contain a duplicate channel",
            Self::PublicationMismatch => {
                "retrieval candidate sets belong to different publications"
            }
            Self::TooManySets => "retrieval candidate set collection exceeds its channel boundary",
        };
        formatter.write_str(message)
    }
}

impl Error for RetrievalCandidateSetsError {}

/// Positive number of deduplicated hits retained after fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FusionResultLimit(u16);

impl FusionResultLimit {
    /// Default interactive fusion boundary.
    pub const DEFAULT: Self = Self(20);

    /// Creates a positive result limit capped at 100.
    pub fn new(value: u16) -> Result<Self, FusionResultLimitError> {
        if value == 0 || value > MAX_FUSION_RESULTS {
            return Err(FusionResultLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the bounded primitive representation.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Fusion result limit outside the product boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FusionResultLimitError {
    value: u16,
}

impl fmt::Display for FusionResultLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "fusion result limit {} must be between 1 and {MAX_FUSION_RESULTS}",
            self.value
        )
    }
}

impl Error for FusionResultLimitError {}

/// Version of the complete weighting, guard, and tie-break policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FusionPolicyVersion(u32);

impl FusionPolicyVersion {
    /// Returns the stable positive version number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Hard provenance band applied before any weighted score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FusionPriority {
    /// At least one exact deterministic channel matched the target.
    Exact,
    /// At least one non-semantic evidence-bearing channel matched the target.
    Evidence,
    /// The target is supported only by semantic similarity.
    Semantic,
}

/// Bounded deterministic weighted score within one hard priority band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FusionScore(u32);

impl FusionScore {
    /// Returns the stable integer score.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Non-negative weighted points retained for auditability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FusionContribution(u32);

impl FusionContribution {
    /// Returns the point contribution or penalty.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One normalized source reason retained after stable-ID deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSourceExplanation {
    reason: RetrievalCandidateReason,
    normalized_score: NormalizedRetrievalSignal,
}

impl ResultSourceExplanation {
    /// Returns the channel-native reason.
    #[must_use]
    pub const fn reason(&self) -> &RetrievalCandidateReason {
        &self.reason
    }

    /// Returns its normalized channel-native score.
    #[must_use]
    pub const fn normalized_score(&self) -> NormalizedRetrievalSignal {
        self.normalized_score
    }
}

/// A normalized signal and its version-one weighted points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FusionSignalExplanation {
    signal: NormalizedRetrievalSignal,
    contribution: FusionContribution,
}

impl FusionSignalExplanation {
    /// Returns the normalized input signal.
    #[must_use]
    pub const fn signal(self) -> NormalizedRetrievalSignal {
        self.signal
    }

    /// Returns the weighted point contribution.
    #[must_use]
    pub const fn contribution(self) -> FusionContribution {
        self.contribution
    }
}

/// Token-cost signal with its derived efficiency and weighted points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FusionTokenExplanation {
    token_cost: CandidateTokenCost,
    efficiency: NormalizedRetrievalSignal,
    contribution: FusionContribution,
}

impl FusionTokenExplanation {
    /// Returns the original bounded token estimate.
    #[must_use]
    pub const fn token_cost(self) -> CandidateTokenCost {
        self.token_cost
    }

    /// Returns the inversely normalized efficiency signal.
    #[must_use]
    pub const fn efficiency(self) -> NormalizedRetrievalSignal {
        self.efficiency
    }

    /// Returns the weighted token-efficiency points.
    #[must_use]
    pub const fn contribution(self) -> FusionContribution {
        self.contribution
    }
}

/// Complete machine-readable explanation of one fused result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultExplanation {
    priority: FusionPriority,
    sources: Vec<ResultSourceExplanation>,
    source_contribution: FusionContribution,
    goal: FusionSignalExplanation,
    step: FusionSignalExplanation,
    freshness: FusionSignalExplanation,
    token: FusionTokenExplanation,
    corroboration: FusionSignalExplanation,
    redundancy: FusionSignalExplanation,
    final_score: FusionScore,
}

impl ResultExplanation {
    /// Returns the non-negotiable provenance band.
    #[must_use]
    pub const fn priority(&self) -> FusionPriority {
        self.priority
    }

    /// Returns all deduplicated source reasons in channel order.
    #[must_use]
    pub fn sources(&self) -> &[ResultSourceExplanation] {
        &self.sources
    }

    /// Returns points from the strongest native source score.
    #[must_use]
    pub const fn source_contribution(&self) -> FusionContribution {
        self.source_contribution
    }

    /// Returns goal relevance and its weighted points.
    #[must_use]
    pub const fn goal(&self) -> FusionSignalExplanation {
        self.goal
    }

    /// Returns current-step relevance and its weighted points.
    #[must_use]
    pub const fn step(&self) -> FusionSignalExplanation {
        self.step
    }

    /// Returns evidence freshness and its weighted points.
    #[must_use]
    pub const fn freshness(&self) -> FusionSignalExplanation {
        self.freshness
    }

    /// Returns token cost, normalized efficiency, and weighted points.
    #[must_use]
    pub const fn token(&self) -> FusionTokenExplanation {
        self.token
    }

    /// Returns independent non-semantic corroboration and its points.
    #[must_use]
    pub const fn corroboration(&self) -> FusionSignalExplanation {
        self.corroboration
    }

    /// Returns redundancy overlap and the points subtracted from the score.
    #[must_use]
    pub const fn redundancy(&self) -> FusionSignalExplanation {
        self.redundancy
    }

    /// Returns the final score used within the priority band.
    #[must_use]
    pub const fn final_score(&self) -> FusionScore {
        self.final_score
    }
}

/// One cross-channel deduplicated and fully explained target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusedRetrievalHit {
    target_id: RetrievalTargetId,
    target: ExactSearchTarget,
    explanation: ResultExplanation,
}

impl FusedRetrievalHit {
    /// Returns the stable cross-channel identity.
    #[must_use]
    pub const fn target_id(&self) -> &RetrievalTargetId {
        &self.target_id
    }

    /// Returns the current target projection.
    #[must_use]
    pub const fn target(&self) -> &ExactSearchTarget {
        &self.target
    }

    /// Returns the complete policy explanation.
    #[must_use]
    pub const fn explanation(&self) -> &ResultExplanation {
        &self.explanation
    }
}

/// Deterministic fusion output bound to one publication and policy version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusedRetrievalResult {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    policy_version: FusionPolicyVersion,
    hits: Vec<FusedRetrievalHit>,
    truncated: bool,
}

impl FusedRetrievalResult {
    /// Returns the atomically published source run.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact fusion policy version used for every score and guard.
    #[must_use]
    pub const fn policy_version(&self) -> FusionPolicyVersion {
        self.policy_version
    }

    /// Returns fused hits in final deterministic order.
    #[must_use]
    pub fn hits(&self) -> &[FusedRetrievalHit] {
        &self.hits
    }

    /// Returns whether an upstream boundary or the result limit omitted candidates.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Complete immutable weighting and priority policy for retrieval fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FusionPolicy {
    version: FusionPolicyVersion,
    source_weight: u32,
    goal_weight: u32,
    step_weight: u32,
    freshness_weight: u32,
    token_weight: u32,
    corroboration_weight: u32,
    redundancy_penalty_weight: u32,
}

impl FusionPolicy {
    /// Returns version one with a hard exact/evidence/semantic guard and integer weights.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: FusionPolicyVersion(1),
            source_weight: 30_000,
            goal_weight: 20_000,
            step_weight: 20_000,
            freshness_weight: 10_000,
            token_weight: 10_000,
            corroboration_weight: 10_000,
            redundancy_penalty_weight: 20_000,
        }
    }

    /// Returns the version governing guards, weights, normalization, and tie-breakers.
    #[must_use]
    pub const fn version(self) -> FusionPolicyVersion {
        self.version
    }

    /// Normalizes, deduplicates, scores, and stably orders one bounded candidate collection.
    pub fn fuse(
        self,
        input: RetrievalCandidateSets,
        result_limit: FusionResultLimit,
    ) -> Result<FusedRetrievalResult, FusionError> {
        let index_run_id = input.index_run_id;
        let snapshot_id = input.snapshot_id;
        let source_truncated = input
            .sets
            .iter()
            .any(|set| set.completeness == CandidateSetCompleteness::Truncated);
        let mut accumulators = BTreeMap::<RetrievalTargetId, CandidateAccumulator>::new();
        for set in input.sets {
            for candidate in set.candidates {
                let target_id = candidate.target_id();
                match accumulators.get_mut(&target_id) {
                    Some(existing) => existing.merge(candidate)?,
                    None => {
                        accumulators.insert(target_id, CandidateAccumulator::new(candidate));
                    }
                }
            }
        }

        let mut hits = accumulators
            .into_iter()
            .map(|(target_id, accumulator)| self.score(target_id, accumulator))
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            left.explanation
                .priority
                .cmp(&right.explanation.priority)
                .then_with(|| {
                    right
                        .explanation
                        .final_score
                        .cmp(&left.explanation.final_score)
                })
                .then_with(|| left.target_id.cmp(&right.target_id))
        });
        let limit = usize::from(result_limit.get());
        let truncated = source_truncated || hits.len() > limit;
        hits.truncate(limit);
        Ok(FusedRetrievalResult {
            index_run_id,
            snapshot_id,
            policy_version: self.version,
            hits,
            truncated,
        })
    }

    fn score(
        self,
        target_id: RetrievalTargetId,
        mut accumulator: CandidateAccumulator,
    ) -> FusedRetrievalHit {
        accumulator
            .sources
            .sort_by_key(RetrievalCandidateReason::source_channel);
        let priority = priority_for(&accumulator.sources);
        let source_signal = accumulator
            .sources
            .iter()
            .map(RetrievalCandidateReason::normalized_score)
            .max()
            .unwrap_or(NormalizedRetrievalSignal::ZERO);
        let non_semantic_sources = accumulator
            .sources
            .iter()
            .filter(|source| source.source_channel() != SourceChannel::Semantic)
            .count();
        let corroboration_value = non_semantic_sources
            .saturating_sub(1)
            .saturating_mul(2_500)
            .min(usize::from(MAX_NORMALIZED_SIGNAL));
        let corroboration_signal = NormalizedRetrievalSignal(
            u16::try_from(corroboration_value).unwrap_or(MAX_NORMALIZED_SIGNAL),
        );
        let token_efficiency = token_efficiency(accumulator.signals.token_cost());

        let source_contribution = weighted(source_signal, self.source_weight);
        let goal = signal_explanation(accumulator.signals.goal_relevance(), self.goal_weight);
        let step = signal_explanation(accumulator.signals.step_relevance(), self.step_weight);
        let freshness = signal_explanation(
            accumulator.signals.freshness().signal(),
            self.freshness_weight,
        );
        let token_contribution = weighted(token_efficiency, self.token_weight);
        let corroboration = signal_explanation(corroboration_signal, self.corroboration_weight);
        let redundancy = signal_explanation(
            accumulator.signals.redundancy(),
            self.redundancy_penalty_weight,
        );
        let positive = source_contribution
            .0
            .saturating_add(goal.contribution.0)
            .saturating_add(step.contribution.0)
            .saturating_add(freshness.contribution.0)
            .saturating_add(token_contribution.0)
            .saturating_add(corroboration.contribution.0);
        let final_score = FusionScore(
            positive
                .saturating_sub(redundancy.contribution.0)
                .min(MAX_FUSION_SCORE),
        );
        let sources = accumulator
            .sources
            .into_iter()
            .map(|reason| ResultSourceExplanation {
                normalized_score: reason.normalized_score(),
                reason,
            })
            .collect();
        FusedRetrievalHit {
            target_id,
            target: accumulator.target,
            explanation: ResultExplanation {
                priority,
                sources,
                source_contribution,
                goal,
                step,
                freshness,
                token: FusionTokenExplanation {
                    token_cost: accumulator.signals.token_cost(),
                    efficiency: token_efficiency,
                    contribution: token_contribution,
                },
                corroboration,
                redundancy,
                final_score,
            },
        }
    }
}

fn weighted(signal: NormalizedRetrievalSignal, weight: u32) -> FusionContribution {
    FusionContribution(u32::from(signal.get()).saturating_mul(weight) / 10_000)
}

fn signal_explanation(signal: NormalizedRetrievalSignal, weight: u32) -> FusionSignalExplanation {
    FusionSignalExplanation {
        signal,
        contribution: weighted(signal, weight),
    }
}

fn token_efficiency(token_cost: CandidateTokenCost) -> NormalizedRetrievalSignal {
    let used = token_cost.get().saturating_sub(1);
    let range = MAX_CANDIDATE_TOKEN_COST.saturating_sub(1);
    let penalty = used.saturating_mul(10_000) / range;
    NormalizedRetrievalSignal(u16::try_from(10_000_u32.saturating_sub(penalty)).unwrap_or_default())
}

fn priority_for(sources: &[RetrievalCandidateReason]) -> FusionPriority {
    if sources
        .iter()
        .any(|source| source.source_channel() == SourceChannel::Exact)
    {
        FusionPriority::Exact
    } else if sources
        .iter()
        .any(|source| source.source_channel() != SourceChannel::Semantic)
    {
        FusionPriority::Evidence
    } else {
        FusionPriority::Semantic
    }
}

struct CandidateAccumulator {
    target: ExactSearchTarget,
    signals: RetrievalCandidateSignals,
    sources: Vec<RetrievalCandidateReason>,
}

impl CandidateAccumulator {
    fn new(candidate: RetrievalCandidate) -> Self {
        Self {
            target: candidate.target,
            signals: candidate.signals,
            sources: vec![candidate.reason],
        }
    }

    fn merge(&mut self, candidate: RetrievalCandidate) -> Result<(), FusionError> {
        if self.target != candidate.target {
            return Err(FusionError::ConflictingTargetProjection);
        }
        if self.signals != candidate.signals {
            return Err(FusionError::ConflictingTargetSignals);
        }
        if self
            .sources
            .iter()
            .any(|source| source.source_channel() == candidate.reason.source_channel())
        {
            return Err(FusionError::DuplicateChannelContribution);
        }
        self.sources.push(candidate.reason);
        Ok(())
    }
}

/// Invalid cross-channel state encountered during deterministic fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionError {
    /// The same stable ID carried different current target data.
    ConflictingTargetProjection,
    /// Channel copies of one stable ID carried different target-level signals.
    ConflictingTargetSignals,
    /// The same channel contributed the same stable target more than once.
    DuplicateChannelContribution,
}

impl fmt::Display for FusionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ConflictingTargetProjection => {
                "fusion candidates disagree about one stable target projection"
            }
            Self::ConflictingTargetSignals => {
                "fusion candidates disagree about one target's ranking signals"
            }
            Self::DuplicateChannelContribution => {
                "fusion candidates contain a duplicate channel contribution"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for FusionError {}

#[cfg(test)]
mod tests {
    use super::{
        CandidateFreshness, CandidateTokenCost, FusionError, FusionPolicy, FusionPriority,
        FusionResultLimit, MemoryCandidateExplanation, NormalizedRetrievalSignal,
        RetrievalCandidate, RetrievalCandidateSet, RetrievalCandidateSets,
        RetrievalCandidateSetsError, RetrievalCandidateSignals,
    };
    use crate::{
        ContentHash, EvidenceRef, ExactSearchExplanation, ExactSearchHit, ExactSearchTarget,
        FileRevision, IndexRunId, RepositoryPath, SnapshotId, SourceChannel, SourcePosition,
        SourceRange,
    };

    #[test]
    fn all_numeric_boundaries_are_explicit() {
        assert!(NormalizedRetrievalSignal::new(10_000).is_ok());
        assert!(NormalizedRetrievalSignal::new(10_001).is_err());
        assert!(CandidateTokenCost::new(1).is_ok());
        assert!(CandidateTokenCost::new(0).is_err());
        assert!(FusionResultLimit::new(100).is_ok());
        assert!(FusionResultLimit::new(101).is_err());
        assert!(
            MemoryCandidateExplanation::new(NormalizedRetrievalSignal::FULL, Vec::new()).is_err()
        );
    }

    #[test]
    fn memory_evidence_is_a_canonical_deduplicated_set() -> Result<(), Box<dyn std::error::Error>> {
        let first = evidence_ref(b"src/a.rs", [1; 32])?;
        let second = evidence_ref(b"src/z.rs", [2; 32])?;

        let explanation = MemoryCandidateExplanation::new(
            NormalizedRetrievalSignal::FULL,
            vec![second.clone(), first.clone(), second],
        )?;

        assert_eq!(
            explanation.evidence(),
            &[first, evidence_ref(b"src/z.rs", [2; 32])?]
        );
        Ok(())
    }

    #[test]
    fn exact_priority_cannot_be_displaced_by_semantic_similarity()
    -> Result<(), Box<dyn std::error::Error>> {
        let run_id = IndexRunId::from_bytes([1; 32]);
        let snapshot_id = SnapshotId::from_bytes([2; 32]);
        let exact_target = file_target(b"src/exact.rs", [3; 32])?;
        let semantic_target = file_target(b"src/popular.rs", [4; 32])?;
        let weak = signals(0, 0, 8_000, 8_000)?;
        let strong = signals(10_000, 10_000, 1, 0)?;
        let ExactSearchTarget::File(exact_revision) = exact_target.clone() else {
            return Err("expected file target".into());
        };
        let exact_hit =
            ExactSearchHit::file(exact_revision, ExactSearchExplanation::NormalizedPathExact)?;
        let exact = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Exact,
            vec![RetrievalCandidate::from_exact(&exact_hit, weak)],
        )?;
        let semantic = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Semantic,
            vec![RetrievalCandidate::semantic(
                semantic_target,
                NormalizedRetrievalSignal::FULL,
                strong,
            )],
        )?;
        let input = RetrievalCandidateSets::new(run_id, snapshot_id, vec![semantic, exact])?;

        let result = FusionPolicy::v1().fuse(input, FusionResultLimit::DEFAULT)?;

        assert_eq!(result.hits().len(), 2);
        assert_eq!(
            result.hits()[0].explanation().priority(),
            FusionPriority::Exact
        );
        assert_eq!(result.policy_version().get(), 1);
        Ok(())
    }

    #[test]
    fn conflicting_current_file_revisions_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let run_id = IndexRunId::from_bytes([5; 32]);
        let snapshot_id = SnapshotId::from_bytes([6; 32]);
        let first_target = file_target(b"src/lib.rs", [7; 32])?;
        let conflicting_target = file_target(b"src/lib.rs", [8; 32])?;
        let signals = signals(5_000, 5_000, 100, 0)?;
        let ExactSearchTarget::File(first_revision) = first_target else {
            return Err("expected file target".into());
        };
        let exact_hit =
            ExactSearchHit::file(first_revision, ExactSearchExplanation::NormalizedPathExact)?;
        let exact = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Exact,
            vec![RetrievalCandidate::from_exact(&exact_hit, signals)],
        )?;
        let semantic = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Semantic,
            vec![RetrievalCandidate::semantic(
                conflicting_target,
                NormalizedRetrievalSignal::FULL,
                signals,
            )],
        )?;
        let input = RetrievalCandidateSets::new(run_id, snapshot_id, vec![exact, semantic])?;

        assert_eq!(
            FusionPolicy::v1().fuse(input, FusionResultLimit::DEFAULT),
            Err(FusionError::ConflictingTargetProjection)
        );
        Ok(())
    }

    #[test]
    fn channel_sets_reject_duplicate_channels_and_signal_disagreement()
    -> Result<(), Box<dyn std::error::Error>> {
        let run_id = IndexRunId::from_bytes([9; 32]);
        let snapshot_id = SnapshotId::from_bytes([10; 32]);
        let target = file_target(b"src/lib.rs", [11; 32])?;
        let first_signals = signals(5_000, 5_000, 100, 0)?;
        let second_signals = signals(6_000, 5_000, 100, 0)?;
        let ExactSearchTarget::File(revision) = target.clone() else {
            return Err("expected file target".into());
        };
        let exact_hit =
            ExactSearchHit::file(revision, ExactSearchExplanation::NormalizedPathExact)?;
        let exact = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Exact,
            vec![RetrievalCandidate::from_exact(&exact_hit, first_signals)],
        )?;
        assert_eq!(
            RetrievalCandidateSets::new(run_id, snapshot_id, vec![exact.clone(), exact.clone()],),
            Err(RetrievalCandidateSetsError::DuplicateChannel)
        );

        let semantic = RetrievalCandidateSet::complete(
            run_id,
            snapshot_id,
            SourceChannel::Semantic,
            vec![RetrievalCandidate::semantic(
                target,
                NormalizedRetrievalSignal::FULL,
                second_signals,
            )],
        )?;
        let input = RetrievalCandidateSets::new(run_id, snapshot_id, vec![exact, semantic])?;
        assert_eq!(
            FusionPolicy::v1().fuse(input, FusionResultLimit::DEFAULT),
            Err(FusionError::ConflictingTargetSignals)
        );
        Ok(())
    }

    #[test]
    fn upstream_truncation_is_never_hidden() -> Result<(), Box<dyn std::error::Error>> {
        let run_id = IndexRunId::from_bytes([12; 32]);
        let snapshot_id = SnapshotId::from_bytes([13; 32]);
        let semantic = RetrievalCandidateSet::truncated(
            run_id,
            snapshot_id,
            SourceChannel::Semantic,
            vec![RetrievalCandidate::semantic(
                file_target(b"src/lib.rs", [14; 32])?,
                NormalizedRetrievalSignal::FULL,
                signals(5_000, 5_000, 100, 0)?,
            )],
        )?;
        let input = RetrievalCandidateSets::new(run_id, snapshot_id, vec![semantic])?;

        let result = FusionPolicy::v1().fuse(input, FusionResultLimit::DEFAULT)?;

        assert!(result.truncated());
        Ok(())
    }

    fn signals(
        goal: u16,
        step: u16,
        tokens: u32,
        redundancy: u16,
    ) -> Result<RetrievalCandidateSignals, Box<dyn std::error::Error>> {
        Ok(RetrievalCandidateSignals::new(
            NormalizedRetrievalSignal::new(goal)?,
            NormalizedRetrievalSignal::new(step)?,
            CandidateFreshness::Current,
            CandidateTokenCost::new(tokens)?,
            NormalizedRetrievalSignal::new(redundancy)?,
        ))
    }

    fn file_target(
        path: &[u8],
        hash: [u8; 32],
    ) -> Result<ExactSearchTarget, Box<dyn std::error::Error>> {
        Ok(ExactSearchTarget::File(FileRevision::new(
            RepositoryPath::try_from_bytes(path.to_vec())?,
            ContentHash::from_bytes(hash),
        )))
    }

    fn evidence_ref(
        path: &[u8],
        hash: [u8; 32],
    ) -> Result<EvidenceRef, Box<dyn std::error::Error>> {
        Ok(EvidenceRef::new(
            FileRevision::new(
                RepositoryPath::try_from_bytes(path.to_vec())?,
                ContentHash::from_bytes(hash),
            ),
            SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?,
        ))
    }
}
