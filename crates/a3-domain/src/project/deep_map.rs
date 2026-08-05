use super::{
    ContentHash, FileRevision, IndexRunId, ModuleId, ModuleKind, ModuleProjection, PublishedIndex,
    RepositoryPath, SnapshotId, SymbolId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

const MAX_EXPLORE_TOKENS: u32 = 1_000_000;
const MAX_EXPLORE_MILLISECONDS: u64 = 86_400_000;
const MAX_EXPLORE_TOOL_CALLS: u16 = 4_096;
const MAX_RANKED_SEED_CANDIDATES: usize = 16_384;
const MAX_MODULE_CARD_BYTES: u32 = 65_536;
const MAX_MODULE_CARD_EVIDENCE_IDS: u16 = 512;

/// Durable version of the structured Module Card contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleCardSchemaVersion(u16);

impl ModuleCardSchemaVersion {
    /// Initial evidence-aware Module Card schema.
    pub const V1: Self = Self(1);

    /// Returns the persisted positive version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Durable version of deterministic Deep-Map planning semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExplorePolicyVersion(u16);

impl ExplorePolicyVersion {
    /// Initial coverage, seed, gain, budget, and stop policy.
    pub const V1: Self = Self(1);

    /// Returns the persisted positive version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Stable identity of one logical Module Card across verified body revisions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleCardId([u8; 32]);

impl ModuleCardId {
    /// Reconstructs an ID produced by the versioned mapper.
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

impl fmt::Debug for ModuleCardId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModuleCardId(redacted)")
    }
}

/// Opaque stable identifier resolved to exact source evidence by the verifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleCardEvidenceId([u8; 32]);

impl ModuleCardEvidenceId {
    /// Reconstructs an evidence identity produced by a trusted resolver.
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

impl fmt::Debug for ModuleCardEvidenceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModuleCardEvidenceId(redacted)")
    }
}

/// Version of prompt, exploration, and proposal-normalization semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapperProfileVersion(u16);

impl MapperProfileVersion {
    /// Initial mapper profile used by the version-one schema.
    pub const V1: Self = Self(1);

    /// Returns the persisted positive version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Explicit lifecycle state carried by every complete Module Card document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleCardStatus {
    /// Explorer output has passed structural validation only.
    Proposed,
    /// Evidence and deterministic claims have passed verification.
    Verified,
    /// Verified card is durably visible for retrieval.
    Published,
    /// Underlying evidence changed or disappeared.
    Stale,
    /// A dependent change requires conservative human or mapper review.
    NeedsReview,
}

/// Required non-content property in the versioned Module Card envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleCardMetadataField {
    /// Stable logical card identity.
    Id,
    /// Deterministic module identity described by the card.
    ModuleId,
    /// Immutable source snapshot.
    SnapshotId,
    /// Canonical union of field-level Evidence IDs.
    EvidenceIds,
    /// Confidence retained separately from epistemic status.
    Confidence,
    /// Mapper profile that generated the proposal.
    MapperProfileVersion,
    /// Explicit proposal, verification, publication, or freshness state.
    Status,
}

const MODULE_CARD_V1_METADATA: [ModuleCardMetadataField; 7] = [
    ModuleCardMetadataField::Id,
    ModuleCardMetadataField::ModuleId,
    ModuleCardMetadataField::SnapshotId,
    ModuleCardMetadataField::EvidenceIds,
    ModuleCardMetadataField::Confidence,
    ModuleCardMetadataField::MapperProfileVersion,
    ModuleCardMetadataField::Status,
];

/// One bounded field in the version-one Module Card schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleCardField {
    /// Human-readable module title.
    Title,
    /// Canonical repository paths owned by the module.
    Paths,
    /// Concise reason the module exists.
    Purpose,
    /// Behaviors owned by the module.
    Responsibilities,
    /// Publicly consumed symbols or interfaces.
    PublicSurface,
    /// Confirmed execution or package entrypoints.
    Entrypoints,
    /// Incoming and outgoing module dependencies.
    Dependencies,
    /// Important data movement through the module.
    DataFlows,
    /// Behavioral or structural rules that must hold.
    Invariants,
    /// Tests and test roots covering the module.
    Tests,
    /// Known failure modes or maintenance risks.
    Risks,
    /// Unresolved questions retained explicitly.
    OpenQuestions,
}

/// Whether exploration must cover a field before a module is sufficiently mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CoverageRequirement {
    /// Exploration must cover the field before completion.
    Must,
    /// Exploration should cover the field when budget remains.
    Should,
}

/// Static validation and coverage metadata for one Module Card field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardFieldSpec {
    field: ModuleCardField,
    requirement: CoverageRequirement,
    max_items: u16,
    max_item_bytes: u16,
    evidence_required_when_non_empty: bool,
}

impl ModuleCardFieldSpec {
    /// Returns the represented schema field.
    #[must_use]
    pub const fn field(self) -> ModuleCardField {
        self.field
    }

    /// Returns the field's completion requirement.
    #[must_use]
    pub const fn requirement(self) -> CoverageRequirement {
        self.requirement
    }

    /// Returns the maximum number of values accepted for the field.
    #[must_use]
    pub const fn max_items(self) -> u16 {
        self.max_items
    }

    /// Returns the maximum UTF-8 bytes accepted per value.
    #[must_use]
    pub const fn max_item_bytes(self) -> u16 {
        self.max_item_bytes
    }

    /// Returns whether a non-empty value must retain source evidence.
    #[must_use]
    pub const fn evidence_required_when_non_empty(self) -> bool {
        self.evidence_required_when_non_empty
    }
}

const MODULE_CARD_V1_FIELDS: [ModuleCardFieldSpec; 12] = [
    field(ModuleCardField::Title, CoverageRequirement::Must, 1, 256),
    field(ModuleCardField::Paths, CoverageRequirement::Must, 32, 1_024),
    field(
        ModuleCardField::Purpose,
        CoverageRequirement::Must,
        8,
        2_048,
    ),
    field(
        ModuleCardField::Responsibilities,
        CoverageRequirement::Must,
        32,
        2_048,
    ),
    field(
        ModuleCardField::PublicSurface,
        CoverageRequirement::Must,
        64,
        2_048,
    ),
    field(
        ModuleCardField::Entrypoints,
        CoverageRequirement::Should,
        64,
        2_048,
    ),
    field(
        ModuleCardField::Dependencies,
        CoverageRequirement::Must,
        128,
        2_048,
    ),
    field(
        ModuleCardField::DataFlows,
        CoverageRequirement::Should,
        32,
        2_048,
    ),
    field(
        ModuleCardField::Invariants,
        CoverageRequirement::Must,
        32,
        2_048,
    ),
    field(
        ModuleCardField::Tests,
        CoverageRequirement::Must,
        128,
        2_048,
    ),
    field(
        ModuleCardField::Risks,
        CoverageRequirement::Should,
        32,
        2_048,
    ),
    field(
        ModuleCardField::OpenQuestions,
        CoverageRequirement::Should,
        32,
        2_048,
    ),
];

const fn field(
    field: ModuleCardField,
    requirement: CoverageRequirement,
    max_items: u16,
    max_item_bytes: u16,
) -> ModuleCardFieldSpec {
    ModuleCardFieldSpec {
        field,
        requirement,
        max_items,
        max_item_bytes,
        evidence_required_when_non_empty: true,
    }
}

/// Versioned schema descriptor shared by planner, explorer, and verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardSchema {
    version: ModuleCardSchemaVersion,
}

impl ModuleCardSchema {
    /// Returns the accepted version-one schema.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            version: ModuleCardSchemaVersion::V1,
        }
    }

    /// Returns the durable schema revision.
    #[must_use]
    pub const fn version(self) -> ModuleCardSchemaVersion {
        self.version
    }

    /// Returns every field specification in canonical order.
    #[must_use]
    pub const fn fields(self) -> &'static [ModuleCardFieldSpec] {
        &MODULE_CARD_V1_FIELDS
    }

    /// Returns mandatory envelope properties in canonical schema order.
    #[must_use]
    pub const fn metadata_fields(self) -> &'static [ModuleCardMetadataField] {
        &MODULE_CARD_V1_METADATA
    }

    /// Returns the mapper profile paired with this schema revision.
    #[must_use]
    pub const fn mapper_profile_version(self) -> MapperProfileVersion {
        MapperProfileVersion::V1
    }

    /// Returns the maximum encoded size accepted before structured validation.
    #[must_use]
    pub const fn max_document_bytes(self) -> u32 {
        MAX_MODULE_CARD_BYTES
    }

    /// Returns the maximum distinct Evidence IDs accepted across all fields.
    #[must_use]
    pub const fn max_evidence_ids(self) -> u16 {
        MAX_MODULE_CARD_EVIDENCE_IDS
    }

    /// Returns the coverage requirement for one field.
    #[must_use]
    pub fn requirement(self, field: ModuleCardField) -> CoverageRequirement {
        self.fields()
            .iter()
            .find(|spec| spec.field() == field)
            .map_or(CoverageRequirement::Should, |spec| spec.requirement())
    }

    /// Returns all mandatory fields in canonical order.
    #[must_use]
    pub fn must_fields(self) -> Vec<ModuleCardField> {
        self.fields()
            .iter()
            .filter(|spec| spec.requirement() == CoverageRequirement::Must)
            .map(|spec| spec.field())
            .collect()
    }
}

/// Verified field coverage already available for one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCoverage {
    module_id: ModuleId,
    covered_fields: Vec<ModuleCardField>,
}

impl ModuleCoverage {
    /// Canonicalizes fields already covered by verified evidence.
    pub fn new(module_id: ModuleId, mut covered_fields: Vec<ModuleCardField>) -> Self {
        covered_fields.sort();
        covered_fields.dedup();
        Self {
            module_id,
            covered_fields,
        }
    }

    /// Returns the covered module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns unique covered fields in canonical order.
    #[must_use]
    pub fn covered_fields(&self) -> &[ModuleCardField] {
        &self.covered_fields
    }
}

/// Snapshot- and schema-bound verified coverage used as planner input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCoverageSnapshot {
    snapshot_id: SnapshotId,
    schema_version: ModuleCardSchemaVersion,
    modules: Vec<ModuleCoverage>,
}

impl ModuleCoverageSnapshot {
    /// Creates canonical coverage for one snapshot and schema.
    pub fn new(
        snapshot_id: SnapshotId,
        schema_version: ModuleCardSchemaVersion,
        mut modules: Vec<ModuleCoverage>,
    ) -> Result<Self, DeepMapPlanError> {
        modules.sort_by_key(ModuleCoverage::module_id);
        if modules
            .windows(2)
            .any(|pair| pair[0].module_id() == pair[1].module_id())
        {
            return Err(DeepMapPlanError::DuplicateCoverageModule);
        }
        Ok(Self {
            snapshot_id,
            schema_version,
            modules,
        })
    }

    /// Creates coverage with no previously mapped modules.
    #[must_use]
    pub const fn empty(snapshot_id: SnapshotId, schema_version: ModuleCardSchemaVersion) -> Self {
        Self {
            snapshot_id,
            schema_version,
            modules: Vec::new(),
        }
    }

    /// Returns the covered immutable index snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the Module Card schema interpreted by this coverage.
    #[must_use]
    pub const fn schema_version(&self) -> ModuleCardSchemaVersion {
        self.schema_version
    }

    /// Returns module coverage in stable module-ID order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleCoverage] {
        &self.modules
    }
}

/// Hard upper bounds for one Deep-Map exploration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExploreBudget {
    tokens: u32,
    milliseconds: u64,
    tool_calls: u16,
}

impl ExploreBudget {
    /// Creates positive bounded token, wall-time, and tool-call limits.
    pub fn new(
        tokens: u32,
        milliseconds: u64,
        tool_calls: u16,
    ) -> Result<Self, ExploreBudgetError> {
        if tokens == 0 || tokens > MAX_EXPLORE_TOKENS {
            return Err(ExploreBudgetError::Tokens);
        }
        if milliseconds == 0 || milliseconds > MAX_EXPLORE_MILLISECONDS {
            return Err(ExploreBudgetError::Time);
        }
        if tool_calls == 0 || tool_calls > MAX_EXPLORE_TOOL_CALLS {
            return Err(ExploreBudgetError::ToolCalls);
        }
        Ok(Self {
            tokens,
            milliseconds,
            tool_calls,
        })
    }

    /// Interactive version-one planning defaults.
    pub const DEFAULT: Self = Self {
        tokens: 32_000,
        milliseconds: 120_000,
        tool_calls: 64,
    };

    /// Returns the maximum reserved model tokens.
    #[must_use]
    pub const fn tokens(self) -> u32 {
        self.tokens
    }

    /// Returns the maximum reserved wall time in milliseconds.
    #[must_use]
    pub const fn milliseconds(self) -> u64 {
        self.milliseconds
    }

    /// Returns the maximum number of read-only tool calls.
    /// Returns the maximum number of read-only tool calls.
    #[must_use]
    pub const fn tool_calls(self) -> u16 {
        self.tool_calls
    }

    /// Returns whether a total reserved cost remains within every dimension.
    #[must_use]
    pub const fn contains(self, cost: ExploreCost) -> bool {
        cost.tokens <= self.tokens
            && cost.milliseconds <= self.milliseconds
            && cost.tool_calls <= self.tool_calls
    }

    const fn is_exhausted_by(self, cost: ExploreCost) -> bool {
        cost.tokens >= self.tokens
            || cost.milliseconds >= self.milliseconds
            || cost.tool_calls >= self.tool_calls
    }
}

/// Invalid zero or unbounded exploration budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExploreBudgetError {
    /// Token count was zero or unbounded.
    Tokens,
    /// Time allowance was zero or unbounded.
    Time,
    /// Tool-call count was zero or unbounded.
    ToolCalls,
}

impl fmt::Display for ExploreBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Tokens => "exploration token budget is zero or exceeds its bound",
            Self::Time => "exploration time budget is zero or exceeds its bound",
            Self::ToolCalls => "exploration tool budget is zero or exceeds its bound",
        })
    }
}

impl Error for ExploreBudgetError {}

/// Deterministic conservative cost reserved by one plan or step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExploreCost {
    tokens: u32,
    milliseconds: u64,
    tool_calls: u16,
}

impl ExploreCost {
    /// Creates a bounded actual or reserved exploration cost; zero is a valid no-op.
    pub fn new(tokens: u32, milliseconds: u64, tool_calls: u16) -> Result<Self, ExploreCostError> {
        if tokens > MAX_EXPLORE_TOKENS
            || milliseconds > MAX_EXPLORE_MILLISECONDS
            || tool_calls > MAX_EXPLORE_TOOL_CALLS
        {
            return Err(ExploreCostError::OutOfBounds);
        }
        Ok(Self::reserved(tokens, milliseconds, tool_calls))
    }

    const fn reserved(tokens: u32, milliseconds: u64, tool_calls: u16) -> Self {
        Self {
            tokens,
            milliseconds,
            tool_calls,
        }
    }

    /// Adds actual or reserved costs without wraparound or global-bound overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, ExploreCostError> {
        Self::new(
            self.tokens
                .checked_add(other.tokens)
                .ok_or(ExploreCostError::Overflow)?,
            self.milliseconds
                .checked_add(other.milliseconds)
                .ok_or(ExploreCostError::Overflow)?,
            self.tool_calls
                .checked_add(other.tool_calls)
                .ok_or(ExploreCostError::Overflow)?,
        )
    }

    /// Returns reserved model tokens.
    #[must_use]
    pub const fn tokens(self) -> u32 {
        self.tokens
    }

    /// Returns reserved wall time in milliseconds.
    #[must_use]
    pub const fn milliseconds(self) -> u64 {
        self.milliseconds
    }

    /// Returns reserved read-only tool calls.
    #[must_use]
    pub const fn tool_calls(self) -> u16 {
        self.tool_calls
    }
}

/// Invalid exploration-cost construction or accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExploreCostError {
    /// One dimension exceeded the global version-one representation bound.
    OutOfBounds,
    /// Arithmetic overflowed before validation.
    Overflow,
}

impl fmt::Display for ExploreCostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OutOfBounds => "exploration cost exceeds a global bound",
            Self::Overflow => "exploration cost arithmetic overflowed",
        })
    }
}

impl Error for ExploreCostError {}

/// Exact published-index target selected for read-only exploration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExploreTarget {
    /// Deterministic module boundary overview.
    Module(ModuleId),
    /// Exact current package manifest revision.
    Manifest {
        /// Lossless repository-relative manifest path.
        path: RepositoryPath,
        /// Exact manifest content revision.
        content_hash: ContentHash,
    },
    /// Exact current structural symbol.
    Symbol(SymbolId),
}

/// Deterministic reason that gave a seed its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExploreSeedReason {
    /// Package metadata is expected to reveal boundaries and dependencies.
    Manifest,
    /// Entrypoint is expected to reveal public behavior and flows.
    Entrypoint,
    /// Rank-selected central symbol is expected to reveal responsibilities.
    CentralSymbol,
    /// Test root is expected to reveal invariants and risks.
    TestRoot,
    /// Strong graph coupling created an additional module view.
    GraphCommunity,
    /// The module still lacks mandatory field coverage.
    UncoveredModule,
}

/// Evidence class an exploration step must return before it can advance coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExploreEvidenceRequirement {
    /// Evidence must resolve to the current module projection.
    CurrentModuleProjection,
    /// Evidence must resolve to the exact manifest path and content hash.
    CurrentManifestRevision,
    /// Evidence must resolve to the exact current symbol and containing revision.
    CurrentSymbolRevision,
}

/// Deterministic verification assigned before any exploration occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExploreVerificationMethod {
    /// Validate field-level Evidence IDs against the published index snapshot.
    ResolveFieldEvidenceAgainstPublishedIndex,
}

/// Lifecycle state of one immutable planner-produced step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExploreStepStatus {
    /// The read-only explorer has not attempted the step.
    Planned,
}

impl ExploreSeedReason {
    const fn priority(self) -> u16 {
        match self {
            Self::Manifest => 600,
            Self::Entrypoint => 500,
            Self::CentralSymbol => 400,
            Self::TestRoot => 350,
            Self::GraphCommunity => 300,
            Self::UncoveredModule => 250,
        }
    }

    const fn cost(self) -> ExploreCost {
        match self {
            Self::Manifest => ExploreCost::reserved(384, 600, 1),
            Self::Entrypoint | Self::CentralSymbol | Self::TestRoot => {
                ExploreCost::reserved(512, 750, 1)
            }
            Self::GraphCommunity | Self::UncoveredModule => ExploreCost::reserved(256, 400, 1),
        }
    }
}

/// Bounded integer estimate used to rank the next expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExpectedInformationGain(u16);

impl ExpectedInformationGain {
    /// Creates a bounded score supplied by deterministic exploration measurement.
    pub fn new(basis_points: u16) -> Result<Self, ExpectedInformationGainError> {
        if basis_points > 10_000 {
            return Err(ExpectedInformationGainError);
        }
        Ok(Self(basis_points))
    }

    /// Returns the bounded ranking score in basis-point units.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

/// Expected information gain exceeded 100 percent of the ranking scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedInformationGainError;

impl fmt::Display for ExpectedInformationGainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected information gain exceeds 10,000 basis points")
    }
}

impl Error for ExpectedInformationGainError {}

/// One immutable deterministic read-only expansion reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExploreStep {
    sequence: u16,
    module_id: ModuleId,
    target: ExploreTarget,
    reason: ExploreSeedReason,
    coverage_fields: Vec<ModuleCardField>,
    expected_information_gain: ExpectedInformationGain,
    reserved_cost: ExploreCost,
    evidence_requirement: ExploreEvidenceRequirement,
    verification_method: ExploreVerificationMethod,
    status: ExploreStepStatus,
}

impl ExploreStep {
    /// Returns the one-based deterministic execution order.
    #[must_use]
    pub const fn sequence(&self) -> u16 {
        self.sequence
    }
    /// Returns the module whose coverage this step advances.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }
    /// Returns the exact published-index target.
    #[must_use]
    pub const fn target(&self) -> &ExploreTarget {
        &self.target
    }
    /// Returns why the target was selected.
    #[must_use]
    pub const fn reason(&self) -> ExploreSeedReason {
        self.reason
    }
    /// Returns fields the step is expected to cover.
    #[must_use]
    pub fn coverage_fields(&self) -> &[ModuleCardField] {
        &self.coverage_fields
    }
    /// Returns the deterministic expected-gain score.
    #[must_use]
    pub const fn expected_information_gain(&self) -> ExpectedInformationGain {
        self.expected_information_gain
    }
    /// Returns budget reserved before execution.
    #[must_use]
    pub const fn reserved_cost(&self) -> ExploreCost {
        self.reserved_cost
    }

    /// Returns evidence required before coverage may advance.
    #[must_use]
    pub const fn evidence_requirement(&self) -> ExploreEvidenceRequirement {
        self.evidence_requirement
    }

    /// Returns the fixed post-exploration verification method.
    #[must_use]
    pub const fn verification_method(&self) -> ExploreVerificationMethod {
        self.verification_method
    }

    /// Returns the current immutable planner state.
    #[must_use]
    pub const fn status(&self) -> ExploreStepStatus {
        self.status
    }
}

/// Why deterministic planning stopped adding read-only expansions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorePlanStopReason {
    /// Every mandatory field is already covered or reserved by a step.
    CoveragePlanned,
    /// No remaining candidate fits every budget dimension.
    BudgetExhausted,
    /// Remaining candidates are below the configured gain floor.
    BelowInformationGainThreshold,
    /// No evidence-backed seed can advance missing coverage.
    NoEligibleSeed,
}

/// Runtime stop conditions consumed later by the read-only explorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorationStopReason {
    /// The owning job was cancelled.
    Cancelled,
    /// A hard token, time, or tool dimension was exhausted.
    BudgetExhausted,
    /// Every mandatory field has verified coverage.
    CoverageSatisfied,
    /// Three consecutive expansions yielded insufficient new information.
    Stagnated,
    /// Every remaining expansion is below the relevance threshold.
    BelowInformationGainThreshold,
}

/// Version-one runtime stop policy; it performs no model or tool access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorationStopPolicy {
    minimum_gain_basis_points: u16,
    stagnant_expansion_limit: u8,
}

impl ExplorationStopPolicy {
    /// Returns the documented version-one gain and stagnation limits.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            minimum_gain_basis_points: 100,
            stagnant_expansion_limit: 3,
        }
    }

    /// Returns the minimum expected information gain.
    #[must_use]
    pub const fn minimum_gain_basis_points(self) -> u16 {
        self.minimum_gain_basis_points
    }

    /// Returns the consecutive low-gain expansion limit.
    #[must_use]
    pub const fn stagnant_expansion_limit(self) -> u8 {
        self.stagnant_expansion_limit
    }

    /// Evaluates stop conditions in safe deterministic priority order.
    #[must_use]
    pub fn evaluate(self, state: ExplorationStopState) -> Option<ExplorationStopReason> {
        if state.cancelled {
            Some(ExplorationStopReason::Cancelled)
        } else if state.budget.is_exhausted_by(state.consumed) {
            Some(ExplorationStopReason::BudgetExhausted)
        } else if state.coverage_satisfied {
            Some(ExplorationStopReason::CoverageSatisfied)
        } else if state.consecutive_low_gain >= self.stagnant_expansion_limit {
            Some(ExplorationStopReason::Stagnated)
        } else if state.remaining_gain.basis_points() < self.minimum_gain_basis_points {
            Some(ExplorationStopReason::BelowInformationGainThreshold)
        } else {
            None
        }
    }
}

/// Exact signals used to evaluate runtime stop conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorationStopState {
    budget: ExploreBudget,
    consumed: ExploreCost,
    coverage_satisfied: bool,
    consecutive_low_gain: u8,
    remaining_gain: ExpectedInformationGain,
    cancelled: bool,
}

impl ExplorationStopState {
    #[allow(clippy::too_many_arguments)]
    /// Captures current runtime signals without executing tools or a model.
    #[must_use]
    pub const fn new(
        budget: ExploreBudget,
        consumed: ExploreCost,
        coverage_satisfied: bool,
        consecutive_low_gain: u8,
        remaining_gain: ExpectedInformationGain,
        cancelled: bool,
    ) -> Self {
        Self {
            budget,
            consumed,
            coverage_satisfied,
            consecutive_low_gain,
            remaining_gain,
            cancelled,
        }
    }
}

/// Complete deterministic Deep-Map plan for one published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorePlan {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    schema_version: ModuleCardSchemaVersion,
    policy_version: ExplorePolicyVersion,
    budget: ExploreBudget,
    reserved_cost: ExploreCost,
    steps: Vec<ExploreStep>,
    stop_reason: ExplorePlanStopReason,
}

impl ExplorePlan {
    /// Returns the published index run used for planning.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }
    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the Module Card schema revision.
    #[must_use]
    pub const fn schema_version(&self) -> ModuleCardSchemaVersion {
        self.schema_version
    }
    /// Returns deterministic planner semantics.
    #[must_use]
    pub const fn policy_version(&self) -> ExplorePolicyVersion {
        self.policy_version
    }
    /// Returns hard plan limits.
    #[must_use]
    pub const fn budget(&self) -> ExploreBudget {
        self.budget
    }
    /// Returns the sum reserved by all plan steps.
    #[must_use]
    pub const fn reserved_cost(&self) -> ExploreCost {
        self.reserved_cost
    }
    /// Returns steps in one-based deterministic order.
    #[must_use]
    pub fn steps(&self) -> &[ExploreStep] {
        &self.steps
    }
    /// Returns why no further step was added.
    #[must_use]
    pub const fn stop_reason(&self) -> ExplorePlanStopReason {
        self.stop_reason
    }
}

#[derive(Debug, Clone)]
struct SeedCandidate {
    module_id: ModuleId,
    target: ExploreTarget,
    reason: ExploreSeedReason,
    fields: BTreeSet<ModuleCardField>,
}

/// Pure deterministic planner operating only on one published index projection.
#[derive(Debug, Clone, Copy)]
pub struct DeepMapPlanner {
    schema: ModuleCardSchema,
    policy_version: ExplorePolicyVersion,
    stop_policy: ExplorationStopPolicy,
}

impl DeepMapPlanner {
    /// Returns the accepted version-one planner.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            schema: ModuleCardSchema::v1(),
            policy_version: ExplorePolicyVersion::V1,
            stop_policy: ExplorationStopPolicy::v1(),
        }
    }

    /// Creates a complete bounded plan from a published index and verified coverage.
    pub fn plan(
        self,
        published: &PublishedIndex,
        coverage: &ModuleCoverageSnapshot,
        budget: ExploreBudget,
    ) -> Result<ExplorePlan, DeepMapPlanError> {
        let projection = published.publication().modules();
        if coverage.snapshot_id() != published.run().snapshot_id() {
            return Err(DeepMapPlanError::CoverageSnapshotMismatch);
        }
        if coverage.schema_version() != self.schema.version() {
            return Err(DeepMapPlanError::CoverageSchemaMismatch);
        }
        validate_coverage_modules(projection, coverage)?;

        let mut planned = coverage
            .modules()
            .iter()
            .map(|module| {
                (
                    module.module_id(),
                    module
                        .covered_fields()
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for module in projection.modules() {
            planned.entry(module.id()).or_default();
        }
        let must = self
            .schema
            .must_fields()
            .into_iter()
            .collect::<BTreeSet<_>>();
        if planned.values().all(|fields| must.is_subset(fields)) {
            return Ok(self.finish(
                published,
                budget,
                ExploreCost::default(),
                Vec::new(),
                ExplorePlanStopReason::CoveragePlanned,
            ));
        }

        let incomplete_modules = planned
            .iter()
            .filter_map(|(module_id, fields)| (!must.is_subset(fields)).then_some(*module_id))
            .collect::<BTreeSet<_>>();
        let mut candidates = collect_candidates(projection, self.schema, &incomplete_modules);
        candidates.sort_by(|left, right| compare_candidates(self.schema, left, right));
        let mut steps = Vec::new();
        let mut reserved = ExploreCost::default();
        let mut budget_blocked = false;
        let mut below_threshold = false;

        for candidate in candidates {
            if planned.values().all(|fields| must.is_subset(fields)) {
                break;
            }
            let fields = candidate
                .fields
                .difference(
                    planned
                        .get(&candidate.module_id)
                        .ok_or(DeepMapPlanError::UnknownCoverageModule)?,
                )
                .copied()
                .collect::<Vec<_>>();
            if fields.is_empty() {
                continue;
            }
            let gain = information_gain(self.schema, candidate.reason, &fields);
            if gain.basis_points() < self.stop_policy.minimum_gain_basis_points() {
                below_threshold = true;
                continue;
            }
            let next = reserved
                .checked_add(candidate.reason.cost())
                .map_err(|_| DeepMapPlanError::CostOverflow)?;
            if !budget.contains(next) {
                budget_blocked = true;
                continue;
            }
            let sequence =
                u16::try_from(steps.len() + 1).map_err(|_| DeepMapPlanError::CostOverflow)?;
            planned
                .get_mut(&candidate.module_id)
                .ok_or(DeepMapPlanError::UnknownCoverageModule)?
                .extend(fields.iter().copied());
            let evidence_requirement = evidence_requirement(&candidate.target);
            steps.push(ExploreStep {
                sequence,
                module_id: candidate.module_id,
                target: candidate.target,
                reason: candidate.reason,
                coverage_fields: fields,
                expected_information_gain: gain,
                reserved_cost: candidate.reason.cost(),
                evidence_requirement,
                verification_method:
                    ExploreVerificationMethod::ResolveFieldEvidenceAgainstPublishedIndex,
                status: ExploreStepStatus::Planned,
            });
            reserved = next;
        }

        let stop_reason = if planned.values().all(|fields| must.is_subset(fields)) {
            ExplorePlanStopReason::CoveragePlanned
        } else if budget_blocked {
            ExplorePlanStopReason::BudgetExhausted
        } else if below_threshold {
            ExplorePlanStopReason::BelowInformationGainThreshold
        } else {
            ExplorePlanStopReason::NoEligibleSeed
        };
        Ok(self.finish(published, budget, reserved, steps, stop_reason))
    }

    fn finish(
        self,
        published: &PublishedIndex,
        budget: ExploreBudget,
        reserved_cost: ExploreCost,
        steps: Vec<ExploreStep>,
        stop_reason: ExplorePlanStopReason,
    ) -> ExplorePlan {
        ExplorePlan {
            index_run_id: published.run().id(),
            snapshot_id: published.run().snapshot_id(),
            schema_version: self.schema.version(),
            policy_version: self.policy_version,
            budget,
            reserved_cost,
            steps,
            stop_reason,
        }
    }
}

const fn evidence_requirement(target: &ExploreTarget) -> ExploreEvidenceRequirement {
    match target {
        ExploreTarget::Module(_) => ExploreEvidenceRequirement::CurrentModuleProjection,
        ExploreTarget::Manifest { .. } => ExploreEvidenceRequirement::CurrentManifestRevision,
        ExploreTarget::Symbol(_) => ExploreEvidenceRequirement::CurrentSymbolRevision,
    }
}

fn validate_coverage_modules(
    projection: &ModuleProjection,
    coverage: &ModuleCoverageSnapshot,
) -> Result<(), DeepMapPlanError> {
    let modules = projection
        .modules()
        .iter()
        .map(|module| module.id())
        .collect::<BTreeSet<_>>();
    if coverage
        .modules()
        .iter()
        .any(|module| !modules.contains(&module.module_id()))
    {
        return Err(DeepMapPlanError::UnknownCoverageModule);
    }
    Ok(())
}

fn collect_candidates(
    projection: &ModuleProjection,
    schema: ModuleCardSchema,
    incomplete_modules: &BTreeSet<ModuleId>,
) -> Vec<SeedCandidate> {
    let mut ranked = Vec::new();
    for module in projection.modules() {
        if !incomplete_modules.contains(&module.id()) {
            continue;
        }
        let mut candidates = BTreeMap::<(ModuleId, ExploreTarget), SeedCandidate>::new();
        let boundary_reason = if module.kind() == ModuleKind::GraphCommunity {
            ExploreSeedReason::GraphCommunity
        } else {
            ExploreSeedReason::UncoveredModule
        };
        add_candidate(
            &mut candidates,
            module.id(),
            ExploreTarget::Module(module.id()),
            boundary_reason,
            &[
                ModuleCardField::Title,
                ModuleCardField::Paths,
                ModuleCardField::Purpose,
                ModuleCardField::Responsibilities,
                ModuleCardField::PublicSurface,
                ModuleCardField::Entrypoints,
                ModuleCardField::Dependencies,
                ModuleCardField::DataFlows,
                ModuleCardField::Invariants,
                ModuleCardField::Tests,
                ModuleCardField::Risks,
                ModuleCardField::OpenQuestions,
            ],
        );
        for manifest in module.manifests() {
            add_manifest_candidate(&mut candidates, module.id(), manifest);
        }
        for symbol in module.entrypoints().symbols() {
            add_candidate(
                &mut candidates,
                module.id(),
                ExploreTarget::Symbol(*symbol),
                ExploreSeedReason::Entrypoint,
                &[
                    ModuleCardField::Entrypoints,
                    ModuleCardField::PublicSurface,
                    ModuleCardField::DataFlows,
                ],
            );
        }
        for symbol in module.central_symbols().symbols() {
            add_candidate(
                &mut candidates,
                module.id(),
                ExploreTarget::Symbol(*symbol),
                ExploreSeedReason::CentralSymbol,
                &[
                    ModuleCardField::Responsibilities,
                    ModuleCardField::PublicSurface,
                    ModuleCardField::Dependencies,
                    ModuleCardField::DataFlows,
                    ModuleCardField::Invariants,
                ],
            );
        }
        for symbol in module.tests().symbols() {
            add_candidate(
                &mut candidates,
                module.id(),
                ExploreTarget::Symbol(*symbol),
                ExploreSeedReason::TestRoot,
                &[
                    ModuleCardField::Tests,
                    ModuleCardField::Invariants,
                    ModuleCardField::Risks,
                ],
            );
        }
        ranked.extend(candidates.into_values());
        if ranked.len() > MAX_RANKED_SEED_CANDIDATES + 512 {
            ranked.sort_by(|left, right| compare_candidates(schema, left, right));
            ranked.truncate(MAX_RANKED_SEED_CANDIDATES);
        }
    }
    ranked.sort_by(|left, right| compare_candidates(schema, left, right));
    ranked.truncate(MAX_RANKED_SEED_CANDIDATES);
    ranked
}

fn add_manifest_candidate(
    candidates: &mut BTreeMap<(ModuleId, ExploreTarget), SeedCandidate>,
    module_id: ModuleId,
    manifest: &FileRevision,
) {
    add_candidate(
        candidates,
        module_id,
        ExploreTarget::Manifest {
            path: manifest.path().clone(),
            content_hash: manifest.content_hash(),
        },
        ExploreSeedReason::Manifest,
        &[
            ModuleCardField::Paths,
            ModuleCardField::PublicSurface,
            ModuleCardField::Dependencies,
            ModuleCardField::Invariants,
        ],
    );
}

fn add_candidate(
    candidates: &mut BTreeMap<(ModuleId, ExploreTarget), SeedCandidate>,
    module_id: ModuleId,
    target: ExploreTarget,
    reason: ExploreSeedReason,
    fields: &[ModuleCardField],
) {
    let candidate = candidates
        .entry((module_id, target.clone()))
        .or_insert_with(|| SeedCandidate {
            module_id,
            target,
            reason,
            fields: BTreeSet::new(),
        });
    if reason.priority() > candidate.reason.priority() {
        candidate.reason = reason;
    }
    candidate.fields.extend(fields.iter().copied());
}

fn compare_candidates(
    schema: ModuleCardSchema,
    left: &SeedCandidate,
    right: &SeedCandidate,
) -> std::cmp::Ordering {
    let left_fields = left.fields.iter().copied().collect::<Vec<_>>();
    let right_fields = right.fields.iter().copied().collect::<Vec<_>>();
    information_gain(schema, right.reason, &right_fields)
        .cmp(&information_gain(schema, left.reason, &left_fields))
        .then_with(|| right.reason.priority().cmp(&left.reason.priority()))
        .then_with(|| left.module_id.cmp(&right.module_id))
        .then_with(|| left.target.cmp(&right.target))
}

fn information_gain(
    schema: ModuleCardSchema,
    reason: ExploreSeedReason,
    fields: &[ModuleCardField],
) -> ExpectedInformationGain {
    let requirement_score = if fields
        .iter()
        .any(|field| schema.requirement(*field) == CoverageRequirement::Must)
    {
        500
    } else {
        200
    };
    let breadth_score = u16::try_from(fields.len())
        .map_or(u16::MAX, |value| value)
        .saturating_mul(5);
    ExpectedInformationGain(
        reason
            .priority()
            .saturating_add(requirement_score)
            .saturating_add(breadth_score)
            .min(10_000),
    )
}

/// Invalid coverage or arithmetic supplied to deterministic planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapPlanError {
    /// Coverage belongs to another immutable snapshot.
    CoverageSnapshotMismatch,
    /// Coverage was produced for another Module Card schema.
    CoverageSchemaMismatch,
    /// Coverage repeated one module identity.
    DuplicateCoverageModule,
    /// Coverage refers to a module absent from the publication.
    UnknownCoverageModule,
    /// Reserved-cost arithmetic exceeded its bounded representation.
    CostOverflow,
}

impl fmt::Display for DeepMapPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CoverageSnapshotMismatch => "coverage belongs to another index snapshot",
            Self::CoverageSchemaMismatch => "coverage uses another Module Card schema",
            Self::DuplicateCoverageModule => "coverage repeats one module",
            Self::UnknownCoverageModule => "coverage refers to an absent module",
            Self::CostOverflow => "exploration cost arithmetic overflowed",
        })
    }
}

impl Error for DeepMapPlanError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Centrality, GraphSymbol, IndexLanguage, IndexPublication, IndexRunRecord, IndexRunSequence,
        IndexRunStatus, LinkedGraph, LocalSymbolId, ModuleMembership, ModuleMembershipEvidence,
        ModulePolicyVersion, ModuleRoot, ModuleSymbolSet, ParsedSymbol, RankProjection, RankScore,
        RankingPolicyVersion, RepositoryCard, RepositoryModule, SourcePosition, SourceRange,
        SymbolKind, SymbolName, SymbolRank, SymbolRankSignals,
    };

    #[test]
    fn planner_is_deterministic_complete_and_budget_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        let published = published_fixture()?;
        let coverage = ModuleCoverageSnapshot::empty(
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
        );
        let planner = DeepMapPlanner::v1();

        let first = planner.plan(&published, &coverage, ExploreBudget::DEFAULT)?;
        let second = planner.plan(&published, &coverage, ExploreBudget::DEFAULT)?;

        assert_eq!(first, second);
        assert_eq!(first.index_run_id(), published.run().id());
        assert_eq!(first.stop_reason(), ExplorePlanStopReason::CoveragePlanned);
        assert!(!first.steps().is_empty());
        assert_eq!(first.steps()[0].reason(), ExploreSeedReason::Manifest);
        assert_eq!(
            first
                .steps()
                .iter()
                .map(ExploreStep::reason)
                .collect::<Vec<_>>(),
            vec![
                ExploreSeedReason::Manifest,
                ExploreSeedReason::Entrypoint,
                ExploreSeedReason::UncoveredModule,
            ]
        );
        assert_eq!(
            first.steps()[0].evidence_requirement(),
            ExploreEvidenceRequirement::CurrentManifestRevision
        );
        assert_eq!(first.steps()[0].status(), ExploreStepStatus::Planned);
        assert_eq!(
            first.steps()[0].verification_method(),
            ExploreVerificationMethod::ResolveFieldEvidenceAgainstPublishedIndex
        );
        assert!(first.budget().contains(first.reserved_cost()));
        assert_eq!(first.reserved_cost(), ExploreCost::new(1_152, 1_750, 3)?);
        assert!(
            first
                .steps()
                .iter()
                .enumerate()
                .all(|(index, step)| usize::from(step.sequence()) == index + 1)
        );
        Ok(())
    }

    #[test]
    fn covered_modules_are_skipped_and_tiny_budget_cannot_overflow()
    -> Result<(), Box<dyn std::error::Error>> {
        let published = published_fixture()?;
        let module_id = published.publication().modules().modules()[0].id();
        let schema = ModuleCardSchema::v1();
        let covered = ModuleCoverageSnapshot::new(
            published.run().snapshot_id(),
            schema.version(),
            vec![ModuleCoverage::new(module_id, schema.must_fields())],
        )?;
        let complete = DeepMapPlanner::v1().plan(&published, &covered, ExploreBudget::DEFAULT)?;
        assert!(complete.steps().is_empty());
        assert_eq!(
            complete.stop_reason(),
            ExplorePlanStopReason::CoveragePlanned
        );

        let empty = ModuleCoverageSnapshot::empty(
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
        );
        for tiny in [
            ExploreBudget::new(255, 10_000, 10)?,
            ExploreBudget::new(10_000, 399, 10)?,
            ExploreBudget::new(10_000, 10_000, 1)?,
        ] {
            let limited = DeepMapPlanner::v1().plan(&published, &empty, tiny)?;
            assert_eq!(
                limited.stop_reason(),
                ExplorePlanStopReason::BudgetExhausted
            );
            assert!(tiny.contains(limited.reserved_cost()));
        }

        let no_incomplete_modules = BTreeSet::new();
        assert!(
            collect_candidates(
                published.publication().modules(),
                schema,
                &no_incomplete_modules,
            )
            .is_empty()
        );
        Ok(())
    }

    #[test]
    fn coverage_and_stop_inputs_remain_snapshot_schema_and_policy_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        let published = published_fixture()?;
        let stale = ModuleCoverageSnapshot::empty(
            SnapshotId::from_bytes([90; 32]),
            ModuleCardSchemaVersion::V1,
        );
        assert_eq!(
            DeepMapPlanner::v1().plan(&published, &stale, ExploreBudget::DEFAULT),
            Err(DeepMapPlanError::CoverageSnapshotMismatch)
        );
        let unknown = ModuleCoverageSnapshot::new(
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
            vec![ModuleCoverage::new(
                ModuleId::from_bytes([91; 32]),
                Vec::new(),
            )],
        )?;
        assert_eq!(
            DeepMapPlanner::v1().plan(&published, &unknown, ExploreBudget::DEFAULT),
            Err(DeepMapPlanError::UnknownCoverageModule)
        );

        let policy = ExplorationStopPolicy::v1();
        let budget = ExploreBudget::new(1_000, 1_000, 2)?;
        assert_eq!(
            policy.evaluate(ExplorationStopState::new(
                budget,
                ExploreCost::default(),
                false,
                0,
                ExpectedInformationGain::new(500)?,
                true,
            )),
            Some(ExplorationStopReason::Cancelled)
        );
        assert_eq!(
            policy.evaluate(ExplorationStopState::new(
                budget,
                ExploreCost::new(1_000, 100, 1)?,
                false,
                0,
                ExpectedInformationGain::new(500)?,
                false,
            )),
            Some(ExplorationStopReason::BudgetExhausted)
        );
        assert_eq!(
            policy.evaluate(ExplorationStopState::new(
                budget,
                ExploreCost::default(),
                false,
                3,
                ExpectedInformationGain::new(500)?,
                false,
            )),
            Some(ExplorationStopReason::Stagnated)
        );
        assert_eq!(
            policy.evaluate(ExplorationStopState::new(
                budget,
                ExploreCost::default(),
                false,
                0,
                ExpectedInformationGain::new(99)?,
                false,
            )),
            Some(ExplorationStopReason::BelowInformationGainThreshold)
        );
        Ok(())
    }

    #[test]
    fn module_card_schema_is_bounded_and_evidence_aware() {
        let schema = ModuleCardSchema::v1();
        assert_eq!(schema.fields().len(), 12);
        assert_eq!(schema.metadata_fields().len(), 7);
        assert_eq!(schema.mapper_profile_version(), MapperProfileVersion::V1);
        assert_eq!(schema.max_document_bytes(), 65_536);
        assert_eq!(schema.max_evidence_ids(), 512);
        assert!(schema.fields().iter().all(|spec| {
            spec.max_items() > 0
                && spec.max_item_bytes() > 0
                && spec.evidence_required_when_non_empty()
        }));
        assert!(schema.must_fields().contains(&ModuleCardField::Purpose));
        assert_eq!(
            schema.requirement(ModuleCardField::OpenQuestions),
            CoverageRequirement::Should
        );
        assert_eq!(
            format!("{:?}", ModuleCardId::from_bytes([7; 32])),
            "ModuleCardId(redacted)"
        );
        assert_eq!(
            format!("{:?}", ModuleCardEvidenceId::from_bytes([8; 32])),
            "ModuleCardEvidenceId(redacted)"
        );
    }

    fn published_fixture() -> Result<PublishedIndex, Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let manifest = revision("Cargo.toml", 2)?;
        let source = revision("src/lib.rs", 3)?;
        let symbol_id = SymbolId::from_bytes([4; 32]);
        let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
        let symbol = GraphSymbol::new(
            symbol_id,
            source.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(1)?,
                SymbolKind::Function,
                SymbolName::try_from_string("main".to_owned())?,
                range,
                range,
            )?,
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![manifest.clone(), source.clone()],
            vec![symbol],
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = RankProjection::new(
            snapshot_id,
            RankingPolicyVersion::v1(),
            vec![SymbolRank::new(
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
            )],
        )?;
        let module_id = ModuleId::from_bytes([5; 32]);
        let featured = ModuleSymbolSet::new(vec![symbol_id], false)?;
        let module = RepositoryModule::new(
            module_id,
            ModuleKind::ManifestBoundary,
            Some(ModuleRoot::Repository),
            vec![manifest.clone()],
            featured.clone(),
            featured.clone(),
            ModuleSymbolSet::empty(),
        )?;
        let membership = ModuleMembership::new(
            module_id,
            symbol_id,
            ModuleMembershipEvidence::manifest(source, manifest.clone()),
        );
        let card = RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module_id],
            vec![IndexLanguage::Rust],
            featured,
            2,
            1,
        )?;
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module],
            vec![membership],
            card,
        )?;
        let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([6; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(PublishedIndex::new(run, publication)?)
    }

    fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([hash; 32]),
        ))
    }
}
