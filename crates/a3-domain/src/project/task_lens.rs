use super::{
    CandidateTokenCost, Confidence, EvidenceRef, ExactSearchTarget, FileRevision,
    FusedRetrievalResult, FusionPolicyVersion, GraphEndpoint, GraphSymbol, IndexRunId,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleClaimPolarity, ModuleClaimPredicate, ModuleId,
    ModuleKind, PublishedIndex, RepositoryCard, RepositoryModule, RepositoryPath,
    ResolvedModuleCardEvidence, ResultExplanation, RetrievalTargetId, SnapshotId, SourceChannel,
    SymbolId, VerifiedClaimKind, VerifiedClaimStatus,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_TASK_LENS_SEED_TEXT_BYTES: usize = 4 * 1_024;
const MAX_TASK_LENS_SUPPLEMENTAL_SEEDS: usize = 64;
const MAX_TASK_LENS_CLAIM_EVIDENCE: usize = 16;
const MAX_TASK_LENS_CLAIMS: usize = 128;
const MAX_TASK_LENS_ENTRIES: usize = 64;
const MAX_TASK_LENS_MODULES: usize = 8;
const MAX_TASK_LENS_TOKEN_BUDGET: u32 = 32_768;
const MIN_TASK_LENS_TOKEN_BUDGET: u32 = 256;
const TASK_LENS_DIGEST_DOMAIN: &[u8] = b"a3.task-lens.v1";

/// Bounded, normalized goal, step, diagnostic, or identifier text used only for retrieval.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLensSeedText(String);

impl TaskLensSeedText {
    /// Normalizes line endings and rejects empty, oversized, NUL, or unsupported control text.
    pub fn try_from_string(value: String) -> Result<Self, TaskLensSeedTextError> {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_TASK_LENS_SEED_TEXT_BYTES {
            return Err(TaskLensSeedTextError::InvalidLength(trimmed.len()));
        }
        if trimmed.chars().any(|character| {
            character == '\0' || (character.is_control() && character != '\n' && character != '\t')
        }) {
            return Err(TaskLensSeedTextError::InvalidCharacter);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns normalized retrieval text. Callers must not treat it as an instruction source.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TaskLensSeedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskLensSeedText")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Invalid retrieval seed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensSeedTextError {
    /// Text was empty or larger than four KiB after normalization.
    InvalidLength(usize),
    /// Text contained NUL or an unsupported control character.
    InvalidCharacter,
}

impl fmt::Display for TaskLensSeedTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => write!(
                formatter,
                "Task Lens seed text has {length} bytes; expected 1 through {MAX_TASK_LENS_SEED_TEXT_BYTES}"
            ),
            Self::InvalidCharacter => {
                formatter.write_str("Task Lens seed text contains an unsupported control character")
            }
        }
    }
}

impl Error for TaskLensSeedTextError {}

/// Stable origin of a compiler, test, or runtime diagnostic seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TaskLensDiagnosticKind {
    /// Compiler or type-checker diagnostic.
    Compiler,
    /// Failed or failing automated test.
    Test,
    /// Runtime failure or observed exception.
    Runtime,
}

/// Supplemental seed that can expand the immutable goal and current-step anchors.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TaskLensSeed {
    /// User-supplied canonical repository-relative path.
    ExplicitPath(RepositoryPath),
    /// User-supplied structural symbol identity.
    ExplicitSymbol(SymbolId),
    /// User-supplied identifier or signature fragment.
    ExplicitIdentifier(TaskLensSeedText),
    /// Bounded diagnostic text with an explicit origin.
    Diagnostic {
        /// Diagnostic class used by the deterministic query planner.
        kind: TaskLensDiagnosticKind,
        /// Normalized diagnostic text.
        text: TaskLensSeedText,
    },
    /// File confirmed as changed by the current snapshot delta.
    ChangedPath(RepositoryPath),
    /// Open, evidence-bound hypothesis that may need task-local revalidation.
    OpenHypothesis(ModuleCardClaimId),
    /// Bounded description of a failed verification.
    FailedVerification(TaskLensSeedText),
}

/// Canonical seed set retained with every Task Lens for reproducibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensSeedSet {
    goal: TaskLensSeedText,
    step: TaskLensSeedText,
    supplemental: Vec<TaskLensSeed>,
}

impl TaskLensSeedSet {
    /// Binds required goal and current-step anchors to a bounded canonical supplemental set.
    pub fn new(
        goal: TaskLensSeedText,
        step: TaskLensSeedText,
        mut supplemental: Vec<TaskLensSeed>,
    ) -> Result<Self, TaskLensSeedSetError> {
        if supplemental.len() > MAX_TASK_LENS_SUPPLEMENTAL_SEEDS {
            return Err(TaskLensSeedSetError::TooManySupplementalSeeds {
                count: supplemental.len(),
            });
        }
        supplemental.sort();
        if supplemental.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(TaskLensSeedSetError::DuplicateSupplementalSeed);
        }
        Ok(Self {
            goal,
            step,
            supplemental,
        })
    }

    /// Returns the durable-goal retrieval anchor.
    #[must_use]
    pub const fn goal(&self) -> &TaskLensSeedText {
        &self.goal
    }

    /// Returns the current-step retrieval anchor.
    #[must_use]
    pub const fn step(&self) -> &TaskLensSeedText {
        &self.step
    }

    /// Returns supplemental seeds in canonical order.
    #[must_use]
    pub fn supplemental(&self) -> &[TaskLensSeed] {
        &self.supplemental
    }
}

/// Invalid Task Lens seed collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensSeedSetError {
    /// More than 64 supplemental seeds were supplied.
    TooManySupplementalSeeds {
        /// Observed number of supplemental seeds.
        count: usize,
    },
    /// Canonicalization found the same supplemental seed twice.
    DuplicateSupplementalSeed,
}

impl fmt::Display for TaskLensSeedSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManySupplementalSeeds { count } => write!(
                formatter,
                "Task Lens has {count} supplemental seeds; maximum is {MAX_TASK_LENS_SUPPLEMENTAL_SEEDS}"
            ),
            Self::DuplicateSupplementalSeed => {
                formatter.write_str("Task Lens supplemental seed set contains a duplicate")
            }
        }
    }
}

impl Error for TaskLensSeedSetError {}

/// Positive bounded token allowance for the temporary retrieval lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLensTokenBudget(u32);

impl TaskLensTokenBudget {
    /// Version-one allowance for Project Map plus code and structured evidence.
    pub const DEFAULT: Self = Self(8_200);

    /// Creates a configurable allowance within the interactive product boundary.
    pub fn new(value: u32) -> Result<Self, TaskLensTokenBudgetError> {
        if !(MIN_TASK_LENS_TOKEN_BUDGET..=MAX_TASK_LENS_TOKEN_BUDGET).contains(&value) {
            return Err(TaskLensTokenBudgetError { value });
        }
        Ok(Self(value))
    }

    /// Returns the portable integer allowance.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Token allowance outside the fixed Task Lens boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensTokenBudgetError {
    value: u32,
}

impl fmt::Display for TaskLensTokenBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Task Lens token budget {} must be between {MIN_TASK_LENS_TOKEN_BUDGET} and {MAX_TASK_LENS_TOKEN_BUDGET}",
            self.value
        )
    }
}

impl Error for TaskLensTokenBudgetError {}

/// Version of Task Lens selection, costing, zoom, and digest semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLensPolicyVersion(u32);

impl TaskLensPolicyVersion {
    /// Initial deterministic Task Lens policy.
    pub const V1: Self = Self(1);

    /// Returns the stable positive version.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Coarse-to-concrete retrieval resolution retained per selected item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TaskLensZoomLevel {
    /// Repository Card overview.
    L0Repository,
    /// Relevant deterministic module boundary.
    L1Module,
    /// Relevant structural symbol.
    L2Symbol,
    /// Concrete current file or declaration span.
    L3SourceSpan,
}

/// One selected repository, module, symbol, or concrete source target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskLensTarget {
    /// Deterministic repository overview from the current publication.
    Repository(RepositoryCard),
    /// Relevant module boundary from the current publication.
    Module(RepositoryModule),
    /// Concrete current file selected for later bounded reading.
    File(FileRevision),
    /// Structural symbol overview without source duplication.
    Symbol(GraphSymbol),
    /// Exact declaration span selected for later bounded reading.
    SourceSpan {
        /// Owning structural symbol.
        symbol_id: SymbolId,
        /// Current revision and declaration range.
        evidence: EvidenceRef,
    },
}

impl TaskLensTarget {
    /// Returns the fixed zoom level represented by this target shape.
    #[must_use]
    pub const fn zoom_level(&self) -> TaskLensZoomLevel {
        match self {
            Self::Repository(_) => TaskLensZoomLevel::L0Repository,
            Self::Module(_) => TaskLensZoomLevel::L1Module,
            Self::Symbol(_) => TaskLensZoomLevel::L2Symbol,
            Self::File(_) | Self::SourceSpan { .. } => TaskLensZoomLevel::L3SourceSpan,
        }
    }
}

/// Auditable reason a target entered the Task Lens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskLensEntryReason {
    /// Unconditional L0 anchor.
    RepositoryAnchor,
    /// Ordered R4 fusion hit with its complete explanation.
    Retrieval {
        /// One-based rank in the fused result.
        rank: u16,
        /// Complete versioned fusion explanation.
        explanation: ResultExplanation,
    },
    /// Current verified claim expanded a selected module or target.
    Claim(ModuleCardClaimId),
}

/// One budgeted Task Lens selection with a conservative context-cost estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLensEntry {
    target: TaskLensTarget,
    estimated_tokens: CandidateTokenCost,
    reason: TaskLensEntryReason,
}

impl TaskLensEntry {
    /// Returns the selected zoom target.
    #[must_use]
    pub const fn target(&self) -> &TaskLensTarget {
        &self.target
    }

    /// Returns the zoom level encoded by the target.
    #[must_use]
    pub const fn zoom_level(&self) -> TaskLensZoomLevel {
        self.target.zoom_level()
    }

    /// Returns the conservative one-byte-per-token estimate plus structural overhead.
    #[must_use]
    pub const fn estimated_tokens(&self) -> CandidateTokenCost {
        self.estimated_tokens
    }

    /// Returns the auditable selection reason.
    #[must_use]
    pub const fn reason(&self) -> &TaskLensEntryReason {
        &self.reason
    }
}

/// Freshness-checkable projection of one persisted R9 claim for task-local expansion.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskLensClaim {
    source_index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    id: ModuleCardClaimId,
    module_id: ModuleId,
    polarity: ModuleClaimPolarity,
    predicate: ModuleClaimPredicate,
    kind: VerifiedClaimKind,
    status: VerifiedClaimStatus,
    confidence: Confidence,
    evidence: Vec<ResolvedModuleCardEvidence>,
}

impl TaskLensClaim {
    /// Reconstructs a bounded typed claim projection without trusting persistence rows.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        id: ModuleCardClaimId,
        module_id: ModuleId,
        polarity: ModuleClaimPolarity,
        predicate: ModuleClaimPredicate,
        kind: VerifiedClaimKind,
        status: VerifiedClaimStatus,
        confidence: Confidence,
        mut evidence: Vec<ResolvedModuleCardEvidence>,
    ) -> Result<Self, TaskLensClaimError> {
        if evidence.len() > MAX_TASK_LENS_CLAIM_EVIDENCE {
            return Err(TaskLensClaimError::TooMuchEvidence {
                count: evidence.len(),
            });
        }
        evidence.sort_by_key(ResolvedModuleCardEvidence::id);
        if evidence.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(TaskLensClaimError::DuplicateEvidence);
        }
        if evidence.is_empty() && !matches!(predicate, ModuleClaimPredicate::ArchitecturalIntent(_))
        {
            return Err(TaskLensClaimError::MissingEvidence);
        }
        let expected_kind = match &predicate {
            ModuleClaimPredicate::Observed(_) => VerifiedClaimKind::Observation,
            ModuleClaimPredicate::ArchitecturalIntent(_) => VerifiedClaimKind::Hypothesis,
            ModuleClaimPredicate::Path(_)
            | ModuleClaimPredicate::Symbol(_)
            | ModuleClaimPredicate::Relation { .. } => match polarity {
                ModuleClaimPolarity::Affirms => VerifiedClaimKind::Fact,
                ModuleClaimPolarity::Denies => VerifiedClaimKind::Hypothesis,
            },
        };
        if kind != expected_kind {
            return Err(TaskLensClaimError::ClassificationMismatch);
        }
        if kind == VerifiedClaimKind::Fact
            && !evidence
                .iter()
                .any(|item| evidence_supports_predicate(item, &predicate))
        {
            return Err(TaskLensClaimError::FactWithoutMatchingEvidence);
        }
        Ok(Self {
            source_index_run_id,
            snapshot_id,
            id,
            module_id,
            polarity,
            predicate,
            kind,
            status,
            confidence,
            evidence,
        })
    }

    /// Returns the exact index run that verified this claim.
    #[must_use]
    pub const fn source_index_run_id(&self) -> IndexRunId {
        self.source_index_run_id
    }

    /// Returns the immutable snapshot that verified this claim.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the stable claim identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardClaimId {
        self.id
    }

    /// Returns the owning module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns whether the claim affirms or denies its predicate.
    #[must_use]
    pub const fn polarity(&self) -> ModuleClaimPolarity {
        self.polarity
    }

    /// Returns the typed structural or prose predicate.
    #[must_use]
    pub const fn predicate(&self) -> &ModuleClaimPredicate {
        &self.predicate
    }

    /// Returns Fact, Observation, or Hypothesis independently from confidence.
    #[must_use]
    pub const fn kind(&self) -> VerifiedClaimKind {
        self.kind
    }

    /// Returns the independent claim lifecycle status.
    #[must_use]
    pub const fn status(&self) -> VerifiedClaimStatus {
        self.status
    }

    /// Returns confidence independently from kind and status.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Returns exact evidence resolved for the source run.
    #[must_use]
    pub fn evidence(&self) -> &[ResolvedModuleCardEvidence] {
        &self.evidence
    }
}

impl fmt::Debug for TaskLensClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskLensClaim")
            .field("source_index_run_id", &self.source_index_run_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("id", &self.id)
            .field("module_id", &self.module_id)
            .field("polarity", &self.polarity)
            .field("kind", &self.kind)
            .field("status", &self.status)
            .field("confidence", &self.confidence)
            .field("evidence_count", &self.evidence.len())
            .finish_non_exhaustive()
    }
}

/// Invalid persisted-claim projection supplied to the Task Lens boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensClaimError {
    /// Claim retained more than 16 evidence objects.
    TooMuchEvidence {
        /// Observed evidence count.
        count: usize,
    },
    /// Claim repeated an evidence identity.
    DuplicateEvidence,
    /// A non-intent R9 claim lost its required current evidence.
    MissingEvidence,
    /// Persisted classification did not match predicate and polarity.
    ClassificationMismatch,
    /// A deterministic Fact lacked exact predicate-matching evidence.
    FactWithoutMatchingEvidence,
}

impl fmt::Display for TaskLensClaimError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMuchEvidence { count } => write!(
                formatter,
                "Task Lens claim has {count} evidence objects; maximum is {MAX_TASK_LENS_CLAIM_EVIDENCE}"
            ),
            Self::DuplicateEvidence => {
                formatter.write_str("Task Lens claim repeats an evidence identity")
            }
            Self::MissingEvidence => {
                formatter.write_str("Task Lens claim requires current resolved evidence")
            }
            Self::ClassificationMismatch => formatter.write_str(
                "Task Lens claim classification does not match its predicate and polarity",
            ),
            Self::FactWithoutMatchingEvidence => {
                formatter.write_str("Task Lens Fact requires exact predicate-matching evidence")
            }
        }
    }
}

impl Error for TaskLensClaimError {}

/// Stable digest of policy, budget, seeds, publication, entries, and admitted claims.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLensDigest([u8; 32]);

impl TaskLensDigest {
    /// Returns the stable digest bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for TaskLensDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TaskLensDigest(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Deterministic, bounded task-local subgraph over one published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLens {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    policy_version: TaskLensPolicyVersion,
    fusion_policy_version: FusionPolicyVersion,
    token_budget: TaskLensTokenBudget,
    estimated_tokens: u32,
    seeds: TaskLensSeedSet,
    entries: Vec<TaskLensEntry>,
    claims: Vec<TaskLensClaim>,
    excluded_stale_claims: u16,
    truncated: bool,
    digest: TaskLensDigest,
}

impl TaskLens {
    /// Returns the exact published run used by all entries and facts.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the Task Lens policy version.
    #[must_use]
    pub const fn policy_version(&self) -> TaskLensPolicyVersion {
        self.policy_version
    }

    /// Returns the upstream R4 fusion policy version.
    #[must_use]
    pub const fn fusion_policy_version(&self) -> FusionPolicyVersion {
        self.fusion_policy_version
    }

    /// Returns the configured Task Lens allowance.
    #[must_use]
    pub const fn token_budget(&self) -> TaskLensTokenBudget {
        self.token_budget
    }

    /// Returns the conservative sum of all selected entry costs.
    #[must_use]
    pub const fn estimated_tokens(&self) -> u32 {
        self.estimated_tokens
    }

    /// Returns the canonical seed set retained for reproduction after refresh.
    #[must_use]
    pub const fn seeds(&self) -> &TaskLensSeedSet {
        &self.seeds
    }

    /// Returns entries in deterministic coarse-to-concrete selection order.
    #[must_use]
    pub fn entries(&self) -> &[TaskLensEntry] {
        &self.entries
    }

    /// Returns only current, evidence-resolved claims relevant to selected targets or modules.
    #[must_use]
    pub fn claims(&self) -> &[TaskLensClaim] {
        &self.claims
    }

    /// Returns how many stale or evidence-incompatible claims were excluded.
    #[must_use]
    pub const fn excluded_stale_claims(&self) -> u16 {
        self.excluded_stale_claims
    }

    /// Returns whether result, entry, module, claim, or token boundaries omitted material.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Returns the deterministic normalized lens digest.
    #[must_use]
    pub const fn digest(&self) -> TaskLensDigest {
        self.digest
    }

    /// Returns whether this lens can still be used for the supplied published index.
    #[must_use]
    pub fn is_current_for(&self, published: &PublishedIndex) -> bool {
        self.index_run_id == published.run().id()
            && self.snapshot_id == published.run().snapshot_id()
    }
}

/// Complete immutable version-one Task Lens compiler policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLensPolicy {
    version: TaskLensPolicyVersion,
}

impl TaskLensPolicy {
    /// Returns version one with fixed ordering, zoom, costing, and cardinality boundaries.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: TaskLensPolicyVersion::V1,
        }
    }

    /// Returns the policy version retained by every output.
    #[must_use]
    pub const fn version(self) -> TaskLensPolicyVersion {
        self.version
    }

    /// Compiles one task-local lens from an already fused current retrieval result.
    pub fn compile(
        self,
        published: &PublishedIndex,
        seeds: TaskLensSeedSet,
        fused: &FusedRetrievalResult,
        claims: Vec<TaskLensClaim>,
        claims_truncated: bool,
        token_budget: TaskLensTokenBudget,
    ) -> Result<TaskLens, TaskLensCompileError> {
        let run = published.run();
        if fused.index_run_id() != run.id() || fused.snapshot_id() != run.snapshot_id() {
            return Err(TaskLensCompileError::PublicationMismatch);
        }
        let publication = published.publication();
        let modules = publication.modules();
        let mut builder = TaskLensBuilder::new(token_budget);
        builder.add_required(TaskLensEntry {
            target: TaskLensTarget::Repository(modules.repository_card().clone()),
            estimated_tokens: estimate_repository_card(modules.repository_card())?,
            reason: TaskLensEntryReason::RepositoryAnchor,
        })?;

        let mut semantic_hits = Vec::new();
        for (index, hit) in fused.hits().iter().enumerate() {
            let rank = u16::try_from(index + 1).map_err(|_| TaskLensCompileError::ResourceLimit)?;
            if !retrieval_target_is_current(published, hit.target()) {
                return Err(TaskLensCompileError::StaleRetrievalTarget);
            }
            if hit.explanation().priority() == super::FusionPriority::Semantic {
                semantic_hits.push((rank, hit));
                continue;
            }
            let reason = TaskLensEntryReason::Retrieval {
                rank,
                explanation: hit.explanation().clone(),
            };
            if !builder.add_retrieval_target(published, hit.target(), reason)? {
                builder.truncated = true;
            }
        }

        let mut claims = claims;
        if claims.len() > MAX_TASK_LENS_CLAIMS {
            return Err(TaskLensCompileError::ResourceLimit);
        }
        claims.sort_by_key(TaskLensClaim::id);
        if claims.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(TaskLensCompileError::DuplicateClaim);
        }
        let mut selected_claims = Vec::new();
        let mut excluded_stale_claims = 0_u16;
        let explicitly_seeded_claims = seeds
            .supplemental()
            .iter()
            .filter_map(|seed| match seed {
                TaskLensSeed::OpenHypothesis(claim_id) => Some(*claim_id),
                TaskLensSeed::ExplicitPath(_)
                | TaskLensSeed::ExplicitSymbol(_)
                | TaskLensSeed::ExplicitIdentifier(_)
                | TaskLensSeed::Diagnostic { .. }
                | TaskLensSeed::ChangedPath(_)
                | TaskLensSeed::FailedVerification(_) => None,
            })
            .collect::<BTreeSet<_>>();
        for claim in claims {
            if !claim_is_current(published, &claim) {
                excluded_stale_claims = excluded_stale_claims.saturating_add(1);
                builder.truncated = true;
                continue;
            }
            let target_ids = claim_target_ids(claim.predicate());
            let relevant = builder.selected_modules.contains(&claim.module_id())
                || target_ids
                    .iter()
                    .any(|target| builder.selected_targets.contains(target))
                || explicitly_seeded_claims.contains(&claim.id());
            if !relevant {
                continue;
            }
            if !builder.add_claim_module(
                published,
                claim.module_id(),
                TaskLensEntryReason::Claim(claim.id()),
            )? {
                builder.truncated = true;
                continue;
            }
            if !builder.reserve_claim(&claim)? {
                builder.truncated = true;
                continue;
            }
            for target_id in target_ids {
                if !builder.add_claim_target(
                    published,
                    &target_id,
                    TaskLensEntryReason::Claim(claim.id()),
                )? {
                    builder.truncated = true;
                }
            }
            selected_claims.push(claim);
        }

        for (rank, hit) in semantic_hits {
            let reason = TaskLensEntryReason::Retrieval {
                rank,
                explanation: hit.explanation().clone(),
            };
            if !builder.add_retrieval_target(published, hit.target(), reason)? {
                builder.truncated = true;
            }
        }

        if builder.entries.len() > MAX_TASK_LENS_ENTRIES
            || builder.selected_modules.len() > MAX_TASK_LENS_MODULES
            || builder.estimated_tokens > token_budget.get()
        {
            return Err(TaskLensCompileError::ResourceLimit);
        }
        let truncated = builder.truncated || fused.truncated() || claims_truncated;
        let digest = task_lens_digest(
            self.version,
            fused.policy_version(),
            token_budget,
            run.id(),
            run.snapshot_id(),
            &seeds,
            &builder.entries,
            &selected_claims,
            excluded_stale_claims,
            truncated,
        );
        Ok(TaskLens {
            index_run_id: run.id(),
            snapshot_id: run.snapshot_id(),
            policy_version: self.version,
            fusion_policy_version: fused.policy_version(),
            token_budget,
            estimated_tokens: builder.estimated_tokens,
            seeds,
            entries: builder.entries,
            claims: selected_claims,
            excluded_stale_claims,
            truncated,
            digest,
        })
    }
}

#[derive(Clone)]
struct TaskLensBuilder {
    token_budget: TaskLensTokenBudget,
    estimated_tokens: u32,
    entries: Vec<TaskLensEntry>,
    keys: BTreeSet<TaskLensTargetKey>,
    selected_modules: BTreeSet<ModuleId>,
    selected_targets: BTreeSet<RetrievalTargetId>,
    truncated: bool,
}

impl TaskLensBuilder {
    fn new(token_budget: TaskLensTokenBudget) -> Self {
        Self {
            token_budget,
            estimated_tokens: 0,
            entries: Vec::new(),
            keys: BTreeSet::new(),
            selected_modules: BTreeSet::new(),
            selected_targets: BTreeSet::new(),
            truncated: false,
        }
    }

    fn add_required(&mut self, entry: TaskLensEntry) -> Result<(), TaskLensCompileError> {
        if !self.add(entry)? {
            return Err(TaskLensCompileError::InsufficientBudgetForRepositoryCard);
        }
        Ok(())
    }

    fn add(&mut self, entry: TaskLensEntry) -> Result<bool, TaskLensCompileError> {
        let key = TaskLensTargetKey::from_target(entry.target());
        if self.keys.contains(&key) {
            return Ok(true);
        }
        let next_tokens = self
            .estimated_tokens
            .checked_add(entry.estimated_tokens().get())
            .ok_or(TaskLensCompileError::ResourceLimit)?;
        if self.entries.len() == MAX_TASK_LENS_ENTRIES || next_tokens > self.token_budget.get() {
            return Ok(false);
        }
        if let TaskLensTarget::Module(module) = entry.target() {
            if !self.selected_modules.contains(&module.id())
                && self.selected_modules.len() == MAX_TASK_LENS_MODULES
            {
                return Ok(false);
            }
            self.selected_modules.insert(module.id());
        }
        match entry.target() {
            TaskLensTarget::File(revision) => {
                self.selected_targets
                    .insert(RetrievalTargetId::File(revision.path().clone()));
            }
            TaskLensTarget::Symbol(symbol) => {
                self.selected_targets
                    .insert(RetrievalTargetId::Symbol(symbol.id()));
            }
            TaskLensTarget::Repository(_)
            | TaskLensTarget::Module(_)
            | TaskLensTarget::SourceSpan { .. } => {}
        }
        self.keys.insert(key);
        self.estimated_tokens = next_tokens;
        self.entries.push(entry);
        Ok(true)
    }

    fn add_retrieval_target(
        &mut self,
        published: &PublishedIndex,
        target: &ExactSearchTarget,
        reason: TaskLensEntryReason,
    ) -> Result<bool, TaskLensCompileError> {
        let checkpoint = self.clone();
        let module_ids = module_ids_for_target(published, target);
        for module_id in &module_ids {
            if self.selected_modules.contains(module_id) {
                continue;
            }
            let module = published
                .publication()
                .modules()
                .modules()
                .iter()
                .find(|module| module.id() == *module_id)
                .ok_or(TaskLensCompileError::InvalidModuleProjection)?;
            if !self.add(TaskLensEntry {
                target: TaskLensTarget::Module(module.clone()),
                estimated_tokens: estimate_module(module)?,
                reason: reason.clone(),
            })? {
                *self = checkpoint;
                return Ok(false);
            }
        }
        if matches!(target, ExactSearchTarget::Symbol(_)) && module_ids.is_empty() {
            return Err(TaskLensCompileError::InvalidModuleProjection);
        }
        match target {
            ExactSearchTarget::File(revision) => {
                let added = self.add(TaskLensEntry {
                    target: TaskLensTarget::File(revision.clone()),
                    estimated_tokens: estimate_file(revision)?,
                    reason,
                })?;
                if !added {
                    *self = checkpoint;
                }
                Ok(added)
            }
            ExactSearchTarget::Symbol(symbol) => {
                if !self.add(TaskLensEntry {
                    target: TaskLensTarget::Symbol(symbol.symbol().clone()),
                    estimated_tokens: estimate_symbol(symbol.symbol())?,
                    reason: reason.clone(),
                })? {
                    *self = checkpoint;
                    return Ok(false);
                }
                let evidence = EvidenceRef::new(
                    symbol.symbol().revision().clone(),
                    symbol.symbol().parsed().declaration_range(),
                );
                self.add(TaskLensEntry {
                    target: TaskLensTarget::SourceSpan {
                        symbol_id: symbol.symbol().id(),
                        evidence,
                    },
                    estimated_tokens: estimate_source_span(symbol.symbol())?,
                    reason,
                })
            }
        }
    }

    fn add_claim_target(
        &mut self,
        published: &PublishedIndex,
        target_id: &RetrievalTargetId,
        reason: TaskLensEntryReason,
    ) -> Result<bool, TaskLensCompileError> {
        match resolve_claim_target(published, target_id) {
            Some(ResolvedClaimTarget::File(revision)) => {
                self.add_retrieval_target(published, &ExactSearchTarget::File(revision), reason)
            }
            Some(ResolvedClaimTarget::Symbol(symbol)) => {
                let checkpoint = self.clone();
                let module_ids = module_ids_for_symbol(published, symbol.id());
                if module_ids.is_empty() {
                    return Err(TaskLensCompileError::InvalidModuleProjection);
                }
                for module_id in module_ids {
                    if self.selected_modules.contains(&module_id) {
                        continue;
                    }
                    let module = published
                        .publication()
                        .modules()
                        .modules()
                        .iter()
                        .find(|module| module.id() == module_id)
                        .ok_or(TaskLensCompileError::InvalidModuleProjection)?;
                    if !self.add(TaskLensEntry {
                        target: TaskLensTarget::Module(module.clone()),
                        estimated_tokens: estimate_module(module)?,
                        reason: reason.clone(),
                    })? {
                        *self = checkpoint;
                        return Ok(false);
                    }
                }
                if !self.add(TaskLensEntry {
                    target: TaskLensTarget::Symbol(symbol.clone()),
                    estimated_tokens: estimate_symbol(&symbol)?,
                    reason: reason.clone(),
                })? {
                    *self = checkpoint;
                    return Ok(false);
                }
                let evidence = EvidenceRef::new(
                    symbol.revision().clone(),
                    symbol.parsed().declaration_range(),
                );
                self.add(TaskLensEntry {
                    target: TaskLensTarget::SourceSpan {
                        symbol_id: symbol.id(),
                        evidence,
                    },
                    estimated_tokens: estimate_source_span(&symbol)?,
                    reason,
                })
            }
            None => Ok(false),
        }
    }

    fn add_claim_module(
        &mut self,
        published: &PublishedIndex,
        module_id: ModuleId,
        reason: TaskLensEntryReason,
    ) -> Result<bool, TaskLensCompileError> {
        if self.selected_modules.contains(&module_id) {
            return Ok(true);
        }
        let module = published
            .publication()
            .modules()
            .modules()
            .iter()
            .find(|module| module.id() == module_id)
            .ok_or(TaskLensCompileError::InvalidModuleProjection)?;
        self.add(TaskLensEntry {
            target: TaskLensTarget::Module(module.clone()),
            estimated_tokens: estimate_module(module)?,
            reason,
        })
    }

    fn reserve_claim(&mut self, claim: &TaskLensClaim) -> Result<bool, TaskLensCompileError> {
        let cost = estimate_claim(claim)?;
        let next_tokens = self
            .estimated_tokens
            .checked_add(cost.get())
            .ok_or(TaskLensCompileError::ResourceLimit)?;
        if next_tokens > self.token_budget.get() {
            return Ok(false);
        }
        self.estimated_tokens = next_tokens;
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TaskLensTargetKey {
    Repository,
    Module(ModuleId),
    File(RepositoryPath),
    Symbol(SymbolId),
    SourceSpan(SymbolId, u32, u32),
}

impl TaskLensTargetKey {
    fn from_target(target: &TaskLensTarget) -> Self {
        match target {
            TaskLensTarget::Repository(_) => Self::Repository,
            TaskLensTarget::Module(module) => Self::Module(module.id()),
            TaskLensTarget::File(revision) => Self::File(revision.path().clone()),
            TaskLensTarget::Symbol(symbol) => Self::Symbol(symbol.id()),
            TaskLensTarget::SourceSpan {
                symbol_id,
                evidence,
            } => Self::SourceSpan(
                *symbol_id,
                evidence.range().start_byte(),
                evidence.range().end_byte(),
            ),
        }
    }
}

fn module_ids_for_target(published: &PublishedIndex, target: &ExactSearchTarget) -> Vec<ModuleId> {
    let modules = published.publication().modules();
    let mut primary = BTreeSet::new();
    let mut communities = BTreeSet::new();
    for membership in modules.memberships() {
        let matches = match target {
            ExactSearchTarget::File(revision) => {
                membership.evidence().member_revision().path() == revision.path()
            }
            ExactSearchTarget::Symbol(symbol) => membership.symbol_id() == symbol.symbol().id(),
        };
        if !matches {
            continue;
        }
        if membership.evidence().kind().is_primary() {
            primary.insert(membership.module_id());
        } else {
            communities.insert(membership.module_id());
        }
    }
    primary.into_iter().chain(communities).collect()
}

fn module_ids_for_symbol(published: &PublishedIndex, symbol_id: SymbolId) -> Vec<ModuleId> {
    let mut primary = BTreeSet::new();
    let mut communities = BTreeSet::new();
    for membership in published.publication().modules().memberships() {
        if membership.symbol_id() != symbol_id {
            continue;
        }
        if membership.evidence().kind().is_primary() {
            primary.insert(membership.module_id());
        } else {
            communities.insert(membership.module_id());
        }
    }
    primary.into_iter().chain(communities).collect()
}

fn claim_target_ids(predicate: &ModuleClaimPredicate) -> Vec<RetrievalTargetId> {
    let mut targets = BTreeSet::new();
    match predicate {
        ModuleClaimPredicate::Path(path) => {
            targets.insert(RetrievalTargetId::File(path.clone()));
        }
        ModuleClaimPredicate::Symbol(symbol_id) => {
            targets.insert(RetrievalTargetId::Symbol(*symbol_id));
        }
        ModuleClaimPredicate::Relation { source, target, .. } => {
            targets.insert(retrieval_id_from_endpoint(source));
            targets.insert(retrieval_id_from_endpoint(target));
        }
        ModuleClaimPredicate::Observed(_) | ModuleClaimPredicate::ArchitecturalIntent(_) => {}
    }
    targets.into_iter().collect()
}

fn retrieval_id_from_endpoint(endpoint: &GraphEndpoint) -> RetrievalTargetId {
    match endpoint {
        GraphEndpoint::File(path) => RetrievalTargetId::File(path.clone()),
        GraphEndpoint::Symbol(symbol_id) => RetrievalTargetId::Symbol(*symbol_id),
    }
}

enum ResolvedClaimTarget {
    File(FileRevision),
    Symbol(GraphSymbol),
}

fn resolve_claim_target(
    published: &PublishedIndex,
    target_id: &RetrievalTargetId,
) -> Option<ResolvedClaimTarget> {
    match target_id {
        RetrievalTargetId::File(path) => published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| revision.path() == path)
            .cloned()
            .map(ResolvedClaimTarget::File),
        RetrievalTargetId::Symbol(symbol_id) => published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == *symbol_id)
            .cloned()
            .map(ResolvedClaimTarget::Symbol),
    }
}

fn claim_is_current(published: &PublishedIndex, claim: &TaskLensClaim) -> bool {
    let run = published.run();
    if claim.status() != VerifiedClaimStatus::Active
        || !published
            .publication()
            .modules()
            .modules()
            .iter()
            .any(|module| module.id() == claim.module_id())
    {
        return false;
    }
    claim.evidence().iter().all(|evidence| match evidence {
        ResolvedModuleCardEvidence::File { id, revision } => {
            *id == ModuleCardEvidenceId::for_file_revision_v1(revision)
                && published.publication().graph().files().contains(revision)
        }
        ResolvedModuleCardEvidence::Symbol { id, symbol } => {
            *id == ModuleCardEvidenceId::for_symbol_v1(symbol)
                && published.publication().graph().symbols().contains(symbol)
        }
        ResolvedModuleCardEvidence::GraphEdge { id, edge } => {
            *id == ModuleCardEvidenceId::for_graph_edge_v1(edge)
                && edge.snapshot_id() == run.snapshot_id()
                && published.publication().graph().edges().contains(edge)
        }
    })
}

fn retrieval_target_is_current(published: &PublishedIndex, target: &ExactSearchTarget) -> bool {
    match target {
        ExactSearchTarget::File(revision) => {
            published.publication().graph().files().contains(revision)
        }
        ExactSearchTarget::Symbol(symbol) => published
            .publication()
            .graph()
            .symbols()
            .contains(symbol.symbol()),
    }
}

fn evidence_supports_predicate(
    evidence: &ResolvedModuleCardEvidence,
    predicate: &ModuleClaimPredicate,
) -> bool {
    match (evidence, predicate) {
        (ResolvedModuleCardEvidence::File { revision, .. }, ModuleClaimPredicate::Path(path)) => {
            revision.path() == path
        }
        (
            ResolvedModuleCardEvidence::Symbol { symbol, .. },
            ModuleClaimPredicate::Symbol(symbol_id),
        ) => symbol.id() == *symbol_id,
        (
            ResolvedModuleCardEvidence::GraphEdge { edge, .. },
            ModuleClaimPredicate::Relation {
                source,
                target,
                kind,
            },
        ) => edge.source() == source && edge.target() == target && edge.kind() == *kind,
        (
            ResolvedModuleCardEvidence::File { .. }
            | ResolvedModuleCardEvidence::Symbol { .. }
            | ResolvedModuleCardEvidence::GraphEdge { .. },
            ModuleClaimPredicate::Observed(_)
            | ModuleClaimPredicate::ArchitecturalIntent(_)
            | ModuleClaimPredicate::Path(_)
            | ModuleClaimPredicate::Symbol(_)
            | ModuleClaimPredicate::Relation { .. },
        ) => false,
    }
}

fn estimate_repository_card(
    card: &RepositoryCard,
) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 96_u32;
    add_count_cost(&mut cost, card.packages().len(), 16)?;
    add_count_cost(&mut cost, card.languages().len(), 8)?;
    add_count_cost(&mut cost, card.entrypoints().symbols().len(), 16)?;
    candidate_cost(cost)
}

fn estimate_module(module: &RepositoryModule) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 96_u32;
    for manifest in module.manifests() {
        add_bytes(&mut cost, manifest.path().as_bytes().len())?;
    }
    add_count_cost(&mut cost, module.central_symbols().symbols().len(), 16)?;
    add_count_cost(&mut cost, module.entrypoints().symbols().len(), 16)?;
    add_count_cost(&mut cost, module.tests().symbols().len(), 16)?;
    candidate_cost(cost)
}

fn estimate_file(revision: &FileRevision) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 64_u32;
    add_bytes(&mut cost, revision.path().as_bytes().len())?;
    candidate_cost(cost)
}

fn estimate_symbol(symbol: &GraphSymbol) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 96_u32;
    add_bytes(&mut cost, symbol.revision().path().as_bytes().len())?;
    add_bytes(&mut cost, symbol.parsed().name().as_str().len())?;
    if let Some(signature) = symbol.parsed().signature() {
        add_bytes(&mut cost, signature.as_str().len())?;
    }
    candidate_cost(cost)
}

fn estimate_source_span(symbol: &GraphSymbol) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 48_u32;
    let span_bytes = symbol
        .parsed()
        .declaration_range()
        .end_byte()
        .saturating_sub(symbol.parsed().declaration_range().start_byte());
    cost = cost
        .checked_add(span_bytes)
        .ok_or(TaskLensCompileError::ResourceLimit)?;
    candidate_cost(cost)
}

fn estimate_claim(claim: &TaskLensClaim) -> Result<CandidateTokenCost, TaskLensCompileError> {
    let mut cost = 64_u32;
    match claim.predicate() {
        ModuleClaimPredicate::Path(path) => add_bytes(&mut cost, path.as_bytes().len())?,
        ModuleClaimPredicate::Symbol(_) => {
            cost = cost
                .checked_add(16)
                .ok_or(TaskLensCompileError::ResourceLimit)?;
        }
        ModuleClaimPredicate::Relation { source, target, .. } => {
            add_endpoint_cost(&mut cost, source)?;
            add_endpoint_cost(&mut cost, target)?;
        }
        ModuleClaimPredicate::Observed(statement)
        | ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            add_bytes(&mut cost, statement.as_str().len())?;
        }
    }
    add_count_cost(&mut cost, claim.evidence().len(), 16)?;
    candidate_cost(cost)
}

fn add_endpoint_cost(
    total: &mut u32,
    endpoint: &GraphEndpoint,
) -> Result<(), TaskLensCompileError> {
    match endpoint {
        GraphEndpoint::File(path) => add_bytes(total, path.as_bytes().len()),
        GraphEndpoint::Symbol(_) => {
            *total = total
                .checked_add(16)
                .ok_or(TaskLensCompileError::ResourceLimit)?;
            Ok(())
        }
    }
}

fn add_bytes(total: &mut u32, bytes: usize) -> Result<(), TaskLensCompileError> {
    *total = total
        .checked_add(u32::try_from(bytes).map_err(|_| TaskLensCompileError::ResourceLimit)?)
        .ok_or(TaskLensCompileError::ResourceLimit)?;
    Ok(())
}

fn add_count_cost(
    total: &mut u32,
    count: usize,
    per_item: u32,
) -> Result<(), TaskLensCompileError> {
    let count = u32::try_from(count).map_err(|_| TaskLensCompileError::ResourceLimit)?;
    *total = total
        .checked_add(
            count
                .checked_mul(per_item)
                .ok_or(TaskLensCompileError::ResourceLimit)?,
        )
        .ok_or(TaskLensCompileError::ResourceLimit)?;
    Ok(())
}

fn candidate_cost(value: u32) -> Result<CandidateTokenCost, TaskLensCompileError> {
    CandidateTokenCost::new(value.clamp(1, 65_535)).map_err(|_| TaskLensCompileError::ResourceLimit)
}

#[allow(clippy::too_many_arguments)]
fn task_lens_digest(
    policy_version: TaskLensPolicyVersion,
    fusion_policy_version: FusionPolicyVersion,
    token_budget: TaskLensTokenBudget,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    seeds: &TaskLensSeedSet,
    entries: &[TaskLensEntry],
    claims: &[TaskLensClaim],
    excluded_stale_claims: u16,
    truncated: bool,
) -> TaskLensDigest {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TASK_LENS_DIGEST_DOMAIN);
    hasher.update(&policy_version.get().to_le_bytes());
    hasher.update(&fusion_policy_version.get().to_le_bytes());
    hasher.update(&token_budget.get().to_le_bytes());
    hasher.update(index_run_id.as_bytes());
    hasher.update(snapshot_id.as_bytes());
    update_bytes(&mut hasher, seeds.goal().as_str().as_bytes());
    update_bytes(&mut hasher, seeds.step().as_str().as_bytes());
    hasher.update(&(seeds.supplemental().len() as u64).to_le_bytes());
    for seed in seeds.supplemental() {
        hash_seed(&mut hasher, seed);
    }
    hasher.update(&(entries.len() as u64).to_le_bytes());
    for entry in entries {
        hash_entry(&mut hasher, entry);
    }
    hasher.update(&(claims.len() as u64).to_le_bytes());
    for claim in claims {
        hasher.update(claim.id().as_bytes());
        hasher.update(&[claim_kind_tag(claim.kind())]);
        hasher.update(&claim.confidence().basis_points().to_le_bytes());
        for evidence in claim.evidence() {
            hasher.update(evidence.id().as_bytes());
        }
    }
    hasher.update(&excluded_stale_claims.to_le_bytes());
    hasher.update(&[u8::from(truncated)]);
    TaskLensDigest(*hasher.finalize().as_bytes())
}

fn hash_seed(hasher: &mut blake3::Hasher, seed: &TaskLensSeed) {
    match seed {
        TaskLensSeed::ExplicitPath(path) => {
            hasher.update(&[0]);
            update_bytes(hasher, path.as_bytes());
        }
        TaskLensSeed::ExplicitSymbol(symbol_id) => {
            hasher.update(&[1]);
            hasher.update(symbol_id.as_bytes());
        }
        TaskLensSeed::ExplicitIdentifier(text) => {
            hasher.update(&[2]);
            update_bytes(hasher, text.as_str().as_bytes());
        }
        TaskLensSeed::Diagnostic { kind, text } => {
            hasher.update(&[3, diagnostic_kind_tag(*kind)]);
            update_bytes(hasher, text.as_str().as_bytes());
        }
        TaskLensSeed::ChangedPath(path) => {
            hasher.update(&[4]);
            update_bytes(hasher, path.as_bytes());
        }
        TaskLensSeed::OpenHypothesis(claim_id) => {
            hasher.update(&[5]);
            hasher.update(claim_id.as_bytes());
        }
        TaskLensSeed::FailedVerification(text) => {
            hasher.update(&[6]);
            update_bytes(hasher, text.as_str().as_bytes());
        }
    }
}

fn hash_entry(hasher: &mut blake3::Hasher, entry: &TaskLensEntry) {
    hasher.update(&entry.estimated_tokens().get().to_le_bytes());
    match entry.target() {
        TaskLensTarget::Repository(card) => {
            hasher.update(&[0]);
            hasher.update(card.snapshot_id().as_bytes());
        }
        TaskLensTarget::Module(module) => {
            hasher.update(&[1]);
            hasher.update(module.id().as_bytes());
            hasher.update(&[module_kind_tag(module.kind())]);
        }
        TaskLensTarget::File(revision) => {
            hasher.update(&[2]);
            hash_revision(hasher, revision);
        }
        TaskLensTarget::Symbol(symbol) => {
            hasher.update(&[3]);
            hasher.update(symbol.id().as_bytes());
            hash_revision(hasher, symbol.revision());
        }
        TaskLensTarget::SourceSpan {
            symbol_id,
            evidence,
        } => {
            hasher.update(&[4]);
            hasher.update(symbol_id.as_bytes());
            hash_revision(hasher, evidence.revision());
            hasher.update(&evidence.range().start_byte().to_le_bytes());
            hasher.update(&evidence.range().end_byte().to_le_bytes());
        }
    }
    match entry.reason() {
        TaskLensEntryReason::RepositoryAnchor => {
            hasher.update(&[0]);
        }
        TaskLensEntryReason::Retrieval { rank, explanation } => {
            hasher.update(&[1]);
            hasher.update(&rank.to_le_bytes());
            hasher.update(&[fusion_priority_tag(explanation.priority())]);
            hasher.update(&explanation.final_score().get().to_le_bytes());
            hasher.update(&(explanation.sources().len() as u64).to_le_bytes());
            for source in explanation.sources() {
                hasher.update(&[source_channel_tag(source.reason().source_channel())]);
            }
        }
        TaskLensEntryReason::Claim(claim_id) => {
            hasher.update(&[2]);
            hasher.update(claim_id.as_bytes());
        }
    }
}

fn hash_revision(hasher: &mut blake3::Hasher, revision: &FileRevision) {
    update_bytes(hasher, revision.path().as_bytes());
    hasher.update(revision.content_hash().as_bytes());
}

fn update_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn diagnostic_kind_tag(kind: TaskLensDiagnosticKind) -> u8 {
    match kind {
        TaskLensDiagnosticKind::Compiler => 0,
        TaskLensDiagnosticKind::Test => 1,
        TaskLensDiagnosticKind::Runtime => 2,
    }
}

fn module_kind_tag(kind: ModuleKind) -> u8 {
    match kind {
        ModuleKind::ManifestBoundary => 0,
        ModuleKind::PathBoundary => 1,
        ModuleKind::GraphCommunity => 2,
    }
}

fn claim_kind_tag(kind: VerifiedClaimKind) -> u8 {
    match kind {
        VerifiedClaimKind::Fact => 0,
        VerifiedClaimKind::Observation => 1,
        VerifiedClaimKind::Hypothesis => 2,
    }
}

fn fusion_priority_tag(priority: super::FusionPriority) -> u8 {
    match priority {
        super::FusionPriority::Exact => 0,
        super::FusionPriority::Evidence => 1,
        super::FusionPriority::Semantic => 2,
    }
}

fn source_channel_tag(channel: SourceChannel) -> u8 {
    match channel {
        SourceChannel::Exact => 0,
        SourceChannel::Lexical => 1,
        SourceChannel::Graph => 2,
        SourceChannel::Test => 3,
        SourceChannel::Memory => 4,
        SourceChannel::Semantic => 5,
    }
}

/// Task Lens input violated publication, projection, or resource invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLensCompileError {
    /// Fusion output and published index refer to different runs or snapshots.
    PublicationMismatch,
    /// A fused candidate did not match the exact current graph projection.
    StaleRetrievalTarget,
    /// Current module membership could not resolve to its published module.
    InvalidModuleProjection,
    /// The same claim identity was supplied twice.
    DuplicateClaim,
    /// Even the mandatory L0 repository card did not fit the configured allowance.
    InsufficientBudgetForRepositoryCard,
    /// Portable integer or fixed cardinality boundary was exceeded.
    ResourceLimit,
}

impl fmt::Display for TaskLensCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PublicationMismatch => {
                "Task Lens fusion result does not match the published index"
            }
            Self::StaleRetrievalTarget => {
                "Task Lens retrieval candidate is stale for the published index"
            }
            Self::InvalidModuleProjection => {
                "Task Lens target does not resolve through the published module projection"
            }
            Self::DuplicateClaim => "Task Lens input repeats a claim identity",
            Self::InsufficientBudgetForRepositoryCard => {
                "Task Lens token allowance cannot retain the mandatory Repository Card"
            }
            Self::ResourceLimit => "Task Lens exceeded a fixed resource boundary",
        })
    }
}

impl Error for TaskLensCompileError {}
