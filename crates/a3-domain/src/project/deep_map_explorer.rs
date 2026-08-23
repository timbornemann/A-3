use super::{
    Confidence, ExpectedInformationGain, ExplorePlan, ExplorePolicyVersion, IndexRunId,
    MapperProfileVersion, ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardSchema,
    ModuleCardSchemaVersion, ModuleCardStatus, ModuleId, SnapshotId, SymbolId,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_GAIN_RATIONALE_BYTES: usize = 512;
const MAX_SEARCH_TEXT_BYTES: usize = 4_096;

/// Version of the strict structured explorer-action contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExplorerActionSchemaVersion(u16);

impl ExplorerActionSchemaVersion {
    /// Initial read-only Inspect, Search, and Proposal action contract.
    pub const V1: Self = Self(1);

    /// Returns the stable positive wire version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Bounded reason why a model expects one read to advance the current step.
#[derive(Clone, PartialEq, Eq)]
pub struct InformationGainRationale(String);

impl InformationGainRationale {
    /// Accepts one non-empty bounded rationale without interpreting it as instructions.
    pub fn try_from_string(value: String) -> Result<Self, InformationGainRationaleError> {
        if value.trim().is_empty()
            || value.len() > MAX_GAIN_RATIONALE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(InformationGainRationaleError {
                actual: value.len(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the bounded rationale for deterministic context compilation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InformationGainRationale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InformationGainRationale")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Information-gain rationale was empty or exceeded the fixed context boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InformationGainRationaleError {
    actual: usize,
}

impl fmt::Display for InformationGainRationaleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "information-gain rationale has {} bytes or invalid control characters; expected safe text of 1 through {MAX_GAIN_RATIONALE_BYTES} bytes",
            self.actual
        )
    }
}

impl Error for InformationGainRationaleError {}

/// Request to inspect exactly the immutable target of the current planned step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerInspectAction {
    expected_information_gain: ExpectedInformationGain,
    rationale: InformationGainRationale,
}

impl ExplorerInspectAction {
    /// Creates a bounded request; the application supplies the plan-owned target.
    #[must_use]
    pub const fn new(
        expected_information_gain: ExpectedInformationGain,
        rationale: InformationGainRationale,
    ) -> Self {
        Self {
            expected_information_gain,
            rationale,
        }
    }

    /// Returns the model's bounded expected gain.
    #[must_use]
    pub const fn expected_information_gain(&self) -> ExpectedInformationGain {
        self.expected_information_gain
    }

    /// Returns the non-authoritative explanation for requesting the read.
    #[must_use]
    pub const fn rationale(&self) -> &InformationGainRationale {
        &self.rationale
    }
}

/// Version-one read-only deterministic retrieval operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExplorerSearchKind {
    /// Exact path, identifier, signature, or role lookup.
    Exact,
    /// Bounded lexical full-text lookup.
    Lexical,
    /// Incoming call relationships.
    Callers,
    /// Outgoing call relationships.
    Callees,
    /// Imported symbols or files.
    Imports,
    /// Exported symbols.
    Exports,
    /// Test relationships.
    Tests,
}

/// Type-safe query paired with its compatible retrieval operation.
#[derive(Clone, PartialEq, Eq)]
pub enum ExplorerSearchQuery {
    /// Bounded text interpreted only by an exact or lexical adapter.
    Text(String),
    /// Exact symbol identity used by a graph traversal preset.
    Symbol(SymbolId),
}

impl fmt::Debug for ExplorerSearchQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => formatter
                .debug_struct("Text")
                .field("bytes", &value.len())
                .finish(),
            Self::Symbol(symbol_id) => formatter.debug_tuple("Symbol").field(symbol_id).finish(),
        }
    }
}

/// Positive read result limit capped to the deterministic retrieval maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerSearchLimit(u16);

impl ExplorerSearchLimit {
    /// Validates the shared one-through-100 retrieval limit.
    pub fn new(value: u16) -> Result<Self, ExplorerSearchActionError> {
        if value == 0 || value > 100 {
            return Err(ExplorerSearchActionError::InvalidLimit(value));
        }
        Ok(Self(value))
    }

    /// Returns the validated retrieval limit.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// One bounded exact, lexical, or graph read request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerSearchAction {
    kind: ExplorerSearchKind,
    query: ExplorerSearchQuery,
    limit: ExplorerSearchLimit,
    expected_information_gain: ExpectedInformationGain,
    rationale: InformationGainRationale,
}

impl ExplorerSearchAction {
    /// Creates an exact or lexical text search.
    pub fn text(
        kind: ExplorerSearchKind,
        query: String,
        limit: ExplorerSearchLimit,
        expected_information_gain: ExpectedInformationGain,
        rationale: InformationGainRationale,
    ) -> Result<Self, ExplorerSearchActionError> {
        if !matches!(
            kind,
            ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical
        ) {
            return Err(ExplorerSearchActionError::QueryKindMismatch);
        }
        if query.trim().is_empty()
            || query.len() > MAX_SEARCH_TEXT_BYTES
            || query.chars().any(char::is_control)
        {
            return Err(ExplorerSearchActionError::InvalidTextLength(query.len()));
        }
        Ok(Self {
            kind,
            query: ExplorerSearchQuery::Text(query),
            limit,
            expected_information_gain,
            rationale,
        })
    }

    /// Creates a graph traversal over one exact current symbol.
    pub fn graph(
        kind: ExplorerSearchKind,
        symbol_id: SymbolId,
        limit: ExplorerSearchLimit,
        expected_information_gain: ExpectedInformationGain,
        rationale: InformationGainRationale,
    ) -> Result<Self, ExplorerSearchActionError> {
        if matches!(
            kind,
            ExplorerSearchKind::Exact | ExplorerSearchKind::Lexical
        ) {
            return Err(ExplorerSearchActionError::QueryKindMismatch);
        }
        Ok(Self {
            kind,
            query: ExplorerSearchQuery::Symbol(symbol_id),
            limit,
            expected_information_gain,
            rationale,
        })
    }

    /// Returns the selected read-only search operation.
    #[must_use]
    pub const fn kind(&self) -> ExplorerSearchKind {
        self.kind
    }

    /// Returns the compatible bounded query.
    #[must_use]
    pub const fn query(&self) -> &ExplorerSearchQuery {
        &self.query
    }

    /// Returns the maximum requested results.
    #[must_use]
    pub const fn limit(&self) -> ExplorerSearchLimit {
        self.limit
    }

    /// Returns the model's bounded expected gain.
    #[must_use]
    pub const fn expected_information_gain(&self) -> ExpectedInformationGain {
        self.expected_information_gain
    }

    /// Returns the non-authoritative explanation for requesting the read.
    #[must_use]
    pub const fn rationale(&self) -> &InformationGainRationale {
        &self.rationale
    }
}

/// Invalid pairing or bound in a structured Search action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerSearchActionError {
    /// Exact/lexical text and graph symbol query kinds were mixed.
    QueryKindMismatch,
    /// Text was empty or larger than 4 KiB.
    InvalidTextLength(usize),
    /// Result limit was outside one through 100.
    InvalidLimit(u16),
}

impl fmt::Display for ExplorerSearchActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueryKindMismatch => {
                formatter.write_str("search kind is incompatible with its query type")
            }
            Self::InvalidTextLength(actual) => write!(
                formatter,
                "search text has {actual} bytes; expected 1 through {MAX_SEARCH_TEXT_BYTES}"
            ),
            Self::InvalidLimit(actual) => {
                write!(
                    formatter,
                    "search result limit {actual} must be between 1 and 100"
                )
            }
        }
    }
}

impl Error for ExplorerSearchActionError {}

/// One non-empty Module Card field with evidence attached at field granularity.
#[derive(Clone, PartialEq, Eq)]
pub struct ProposedModuleCardField {
    field: ModuleCardField,
    values: Vec<String>,
    evidence_ids: Vec<ModuleCardEvidenceId>,
}

impl ProposedModuleCardField {
    /// Validates schema-specific item, byte, duplicate, and evidence bounds.
    pub fn new(
        field: ModuleCardField,
        values: Vec<String>,
        mut evidence_ids: Vec<ModuleCardEvidenceId>,
    ) -> Result<Self, ModuleCardProposalError> {
        let schema = ModuleCardSchema::v1();
        let spec = schema
            .fields()
            .iter()
            .find(|spec| spec.field() == field)
            .copied()
            .ok_or(ModuleCardProposalError::UnknownField)?;
        if values.is_empty() || values.len() > usize::from(spec.max_items()) {
            return Err(ModuleCardProposalError::InvalidItemCount(field));
        }
        if values.iter().any(|value| {
            value.trim().is_empty()
                || value.len() > usize::from(spec.max_item_bytes())
                || value.chars().any(char::is_control)
        }) {
            return Err(ModuleCardProposalError::InvalidItemBytes(field));
        }
        let distinct_values = values.iter().collect::<BTreeSet<_>>();
        if distinct_values.len() != values.len() {
            return Err(ModuleCardProposalError::DuplicateItem(field));
        }
        let evidence_count = evidence_ids.len();
        evidence_ids.sort_unstable();
        evidence_ids.dedup();
        if evidence_ids.len() != evidence_count {
            return Err(ModuleCardProposalError::DuplicateFieldEvidence(field));
        }
        if spec.evidence_required_when_non_empty() && evidence_ids.is_empty() {
            return Err(ModuleCardProposalError::MissingFieldEvidence(field));
        }
        Ok(Self {
            field,
            values,
            evidence_ids,
        })
    }

    /// Returns the schema field.
    #[must_use]
    pub const fn field(&self) -> ModuleCardField {
        self.field
    }

    /// Returns the proposed values without granting them epistemic status.
    #[must_use]
    pub fn values(&self) -> &[String] {
        &self.values
    }

    /// Returns canonical field-specific evidence identities.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }
}

impl fmt::Debug for ProposedModuleCardField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProposedModuleCardField")
            .field("field", &self.field)
            .field("value_count", &self.values.len())
            .field(
                "value_bytes",
                &self.values.iter().map(String::len).sum::<usize>(),
            )
            .field("evidence_count", &self.evidence_ids.len())
            .finish()
    }
}

/// Structurally valid, non-authoritative Module Card candidate awaiting R9 verification.
#[derive(Clone, PartialEq, Eq)]
pub struct ModuleCardProposal {
    envelope: ModuleCardProposalEnvelope,
    fields: Vec<ProposedModuleCardField>,
    evidence_ids: Vec<ModuleCardEvidenceId>,
}

/// Required typed envelope supplied with every non-authoritative card proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCardProposalEnvelope {
    id: ModuleCardId,
    module_id: ModuleId,
    snapshot_id: SnapshotId,
    schema_version: ModuleCardSchemaVersion,
    mapper_profile_version: MapperProfileVersion,
    confidence: Confidence,
}

impl ModuleCardProposalEnvelope {
    /// Groups proposal identity, provenance, version, and confidence metadata.
    #[must_use]
    pub const fn new(
        id: ModuleCardId,
        module_id: ModuleId,
        snapshot_id: SnapshotId,
        schema_version: ModuleCardSchemaVersion,
        mapper_profile_version: MapperProfileVersion,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            module_id,
            snapshot_id,
            schema_version,
            mapper_profile_version,
            confidence,
        }
    }
}

impl ModuleCardProposal {
    /// Creates a canonical proposal after the raw document size has been bounded.
    pub fn new(
        envelope: ModuleCardProposalEnvelope,
        mut fields: Vec<ProposedModuleCardField>,
        encoded_bytes: usize,
    ) -> Result<Self, ModuleCardProposalError> {
        let schema = ModuleCardSchema::v1();
        if envelope.schema_version != schema.version() {
            return Err(ModuleCardProposalError::UnsupportedSchemaVersion);
        }
        if envelope.mapper_profile_version != schema.mapper_profile_version() {
            return Err(ModuleCardProposalError::UnsupportedMapperProfile);
        }
        if encoded_bytes > schema.max_document_bytes() as usize {
            return Err(ModuleCardProposalError::DocumentTooLarge(encoded_bytes));
        }
        if fields.is_empty() {
            return Err(ModuleCardProposalError::EmptyProposal);
        }
        fields.sort_by_key(ProposedModuleCardField::field);
        if fields
            .windows(2)
            .any(|pair| pair[0].field() == pair[1].field())
        {
            return Err(ModuleCardProposalError::DuplicateField);
        }
        let evidence_ids = fields
            .iter()
            .flat_map(|field| field.evidence_ids().iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if evidence_ids.len() > usize::from(schema.max_evidence_ids()) {
            return Err(ModuleCardProposalError::TooManyEvidenceIds(
                evidence_ids.len(),
            ));
        }
        Ok(Self {
            envelope,
            fields,
            evidence_ids,
        })
    }

    /// Returns the proposed logical card identity.
    #[must_use]
    pub const fn id(&self) -> ModuleCardId {
        self.envelope.id
    }

    /// Returns the described deterministic module.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.envelope.module_id
    }

    /// Returns the immutable source snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.envelope.snapshot_id
    }

    /// Returns the interpreted Module Card schema.
    #[must_use]
    pub const fn schema_version(&self) -> ModuleCardSchemaVersion {
        self.envelope.schema_version
    }

    /// Returns the proposal-producing mapper profile.
    #[must_use]
    pub const fn mapper_profile_version(&self) -> MapperProfileVersion {
        self.envelope.mapper_profile_version
    }

    /// Returns confidence separately from status and evidence validity.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.envelope.confidence
    }

    /// Returns proposal-only status; this type cannot represent Verified or Published.
    #[must_use]
    pub const fn status(&self) -> ModuleCardStatus {
        ModuleCardStatus::Proposed
    }

    /// Returns fields in canonical schema order.
    #[must_use]
    pub fn fields(&self) -> &[ProposedModuleCardField] {
        &self.fields
    }

    /// Returns the canonical union of all field-level evidence.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }

    /// Returns whether this proposal includes one field.
    #[must_use]
    pub fn contains_field(&self, field: ModuleCardField) -> bool {
        self.fields
            .binary_search_by_key(&field, ProposedModuleCardField::field)
            .is_ok()
    }
}

impl fmt::Debug for ModuleCardProposal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleCardProposal")
            .field("envelope", &self.envelope)
            .field("field_count", &self.fields.len())
            .field("evidence_count", &self.evidence_ids.len())
            .finish()
    }
}

/// Structural Module Card proposal failure before deterministic claim verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCardProposalError {
    /// The field is absent from the accepted schema.
    UnknownField,
    /// A field was empty or exceeded its item limit.
    InvalidItemCount(ModuleCardField),
    /// A field contained an empty or oversized value.
    InvalidItemBytes(ModuleCardField),
    /// A field repeated a value.
    DuplicateItem(ModuleCardField),
    /// A non-empty field had no field-specific Evidence ID.
    MissingFieldEvidence(ModuleCardField),
    /// A field repeated an Evidence ID.
    DuplicateFieldEvidence(ModuleCardField),
    /// The same schema field occurred more than once.
    DuplicateField,
    /// No non-empty field was proposed.
    EmptyProposal,
    /// The outer structured document exceeded 64 KiB.
    DocumentTooLarge(usize),
    /// The union exceeded 512 Evidence IDs.
    TooManyEvidenceIds(usize),
    /// The proposal named another Module Card schema.
    UnsupportedSchemaVersion,
    /// The proposal named another mapper profile.
    UnsupportedMapperProfile,
}

impl fmt::Display for ModuleCardProposalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownField => formatter.write_str("proposal contains an unknown card field"),
            Self::InvalidItemCount(field) => {
                write!(
                    formatter,
                    "proposal field {field:?} has an invalid item count"
                )
            }
            Self::InvalidItemBytes(field) => {
                write!(
                    formatter,
                    "proposal field {field:?} has an invalid item size"
                )
            }
            Self::DuplicateItem(field) => {
                write!(formatter, "proposal field {field:?} repeats an item")
            }
            Self::MissingFieldEvidence(field) => {
                write!(formatter, "proposal field {field:?} has no evidence")
            }
            Self::DuplicateFieldEvidence(field) => {
                write!(formatter, "proposal field {field:?} repeats evidence")
            }
            Self::DuplicateField => formatter.write_str("proposal repeats a schema field"),
            Self::EmptyProposal => formatter.write_str("proposal has no non-empty fields"),
            Self::DocumentTooLarge(actual) => write!(
                formatter,
                "proposal document has {actual} bytes and exceeds the schema boundary"
            ),
            Self::TooManyEvidenceIds(actual) => write!(
                formatter,
                "proposal has {actual} distinct Evidence IDs and exceeds the schema boundary"
            ),
            Self::UnsupportedSchemaVersion => {
                formatter.write_str("proposal uses an unsupported Module Card schema")
            }
            Self::UnsupportedMapperProfile => {
                formatter.write_str("proposal uses an unsupported mapper profile")
            }
        }
    }
}

impl Error for ModuleCardProposalError {}

/// Strict union of all executable model outputs accepted during R8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerAction {
    /// Inspect the exact target selected by the current deterministic plan step.
    Inspect(ExplorerInspectAction),
    /// Run one typed read-only deterministic search.
    Search(ExplorerSearchAction),
    /// Submit a proposal for structural and evidence-membership checks.
    Propose(ModuleCardProposal),
}

/// Snapshot- and plan-bound state retained by the owner for safe resume.
#[derive(Clone, PartialEq, Eq)]
pub struct ExplorerCheckpoint {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    schema_version: ModuleCardSchemaVersion,
    policy_version: ExplorePolicyVersion,
    confirmed_proposals: Vec<ModuleCardProposal>,
}

impl ExplorerCheckpoint {
    /// Starts an empty checkpoint bound to one immutable deterministic plan.
    #[must_use]
    pub fn new(plan: &ExplorePlan) -> Self {
        Self {
            index_run_id: plan.index_run_id(),
            snapshot_id: plan.snapshot_id(),
            schema_version: plan.schema_version(),
            policy_version: plan.policy_version(),
            confirmed_proposals: Vec::new(),
        }
    }

    /// Checks that this state can safely resume the supplied exact plan.
    pub fn validate_for(&self, plan: &ExplorePlan) -> Result<(), ExplorerCheckpointError> {
        if self.index_run_id != plan.index_run_id()
            || self.snapshot_id != plan.snapshot_id()
            || self.schema_version != plan.schema_version()
            || self.policy_version != plan.policy_version()
        {
            return Err(ExplorerCheckpointError::PlanMismatch);
        }
        if self.confirmed_proposals.len() > plan.steps().len() {
            return Err(ExplorerCheckpointError::TooManyConfirmedSteps);
        }
        for (proposal, step) in self.confirmed_proposals.iter().zip(plan.steps().iter()) {
            validate_proposal_for_step(plan, step.module_id(), step.coverage_fields(), proposal)?;
        }
        Ok(())
    }

    /// Confirms only the next unconfirmed step, preventing gaps and replay.
    pub fn confirm_next(
        &mut self,
        plan: &ExplorePlan,
        proposal: ModuleCardProposal,
    ) -> Result<(), ExplorerCheckpointError> {
        self.validate_for(plan)?;
        let step = plan
            .steps()
            .get(self.confirmed_proposals.len())
            .ok_or(ExplorerCheckpointError::NoRemainingStep)?;
        validate_proposal_for_step(plan, step.module_id(), step.coverage_fields(), &proposal)?;
        self.confirmed_proposals.push(proposal);
        Ok(())
    }

    /// Returns the number of consecutive confirmed plan steps.
    #[must_use]
    pub fn confirmed_step_count(&self) -> usize {
        self.confirmed_proposals.len()
    }

    /// Returns structurally valid proposals in plan order.
    #[must_use]
    pub fn confirmed_proposals(&self) -> &[ModuleCardProposal] {
        &self.confirmed_proposals
    }

    /// Returns whether every planner-produced step is confirmed.
    #[must_use]
    pub fn is_complete_for(&self, plan: &ExplorePlan) -> bool {
        self.validate_for(plan).is_ok() && self.confirmed_proposals.len() == plan.steps().len()
    }
}

impl fmt::Debug for ExplorerCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExplorerCheckpoint")
            .field("index_run_id", &self.index_run_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("schema_version", &self.schema_version)
            .field("policy_version", &self.policy_version)
            .field("confirmed_step_count", &self.confirmed_proposals.len())
            .finish()
    }
}

fn validate_proposal_for_step(
    plan: &ExplorePlan,
    module_id: ModuleId,
    coverage_fields: &[ModuleCardField],
    proposal: &ModuleCardProposal,
) -> Result<(), ExplorerCheckpointError> {
    if proposal.module_id() != module_id
        || proposal.id() != ModuleCardId::for_module_fields_v1(module_id, coverage_fields)
    {
        return Err(ExplorerCheckpointError::ModuleMismatch);
    }
    if proposal.snapshot_id() != plan.snapshot_id() {
        return Err(ExplorerCheckpointError::SnapshotMismatch);
    }
    if proposal.schema_version() != plan.schema_version() {
        return Err(ExplorerCheckpointError::SchemaMismatch);
    }
    if coverage_fields
        .iter()
        .any(|field| !proposal.contains_field(*field))
    {
        return Err(ExplorerCheckpointError::MissingPlannedField);
    }
    Ok(())
}

/// Resume or confirmation state did not match the immutable ExplorePlan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerCheckpointError {
    /// Run, snapshot, schema, or planner policy differs.
    PlanMismatch,
    /// Checkpoint contains more confirmations than the plan has steps.
    TooManyConfirmedSteps,
    /// All steps were already confirmed.
    NoRemainingStep,
    /// A proposal describes another module.
    ModuleMismatch,
    /// A proposal belongs to another immutable snapshot.
    SnapshotMismatch,
    /// A proposal uses another Module Card schema.
    SchemaMismatch,
    /// A proposal omitted an outcome required by its plan step.
    MissingPlannedField,
}

impl fmt::Display for ExplorerCheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PlanMismatch => "explorer checkpoint belongs to another plan",
            Self::TooManyConfirmedSteps => "explorer checkpoint exceeds the plan length",
            Self::NoRemainingStep => "explorer plan has no unconfirmed step",
            Self::ModuleMismatch => "proposal belongs to another module",
            Self::SnapshotMismatch => "proposal belongs to another snapshot",
            Self::SchemaMismatch => "proposal uses another Module Card schema",
            Self::MissingPlannedField => "proposal omits a field required by the current step",
        })
    }
}

impl Error for ExplorerCheckpointError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_rejects_evidence_free_and_duplicate_fields() {
        assert_eq!(
            ProposedModuleCardField::new(ModuleCardField::Title, vec!["Core".to_owned()], vec![]),
            Err(ModuleCardProposalError::MissingFieldEvidence(
                ModuleCardField::Title
            ))
        );
        let repeated = ModuleCardEvidenceId::from_bytes([1; 32]);
        assert_eq!(
            ProposedModuleCardField::new(
                ModuleCardField::Title,
                vec!["Core".to_owned()],
                vec![repeated, repeated],
            ),
            Err(ModuleCardProposalError::DuplicateFieldEvidence(
                ModuleCardField::Title
            ))
        );

        let field = ProposedModuleCardField::new(
            ModuleCardField::Title,
            vec!["Core".to_owned()],
            vec![ModuleCardEvidenceId::from_bytes([1; 32])],
        );
        assert!(field.is_ok());
        if let Ok(field) = field {
            assert_eq!(
                ModuleCardProposal::new(
                    ModuleCardProposalEnvelope::new(
                        ModuleCardId::from_bytes([2; 32]),
                        ModuleId::from_bytes([3; 32]),
                        SnapshotId::from_bytes([4; 32]),
                        ModuleCardSchemaVersion::V1,
                        MapperProfileVersion::V1,
                        Confidence::certain(),
                    ),
                    vec![field.clone(), field],
                    512,
                ),
                Err(ModuleCardProposalError::DuplicateField)
            );
        }
    }

    #[test]
    fn search_queries_keep_text_and_graph_operations_separate()
    -> Result<(), Box<dyn std::error::Error>> {
        let gain = ExpectedInformationGain::new(500)?;
        let rationale = InformationGainRationale::try_from_string("find callers".to_owned())?;
        let limit = ExplorerSearchLimit::new(20)?;
        assert_eq!(
            ExplorerSearchAction::text(
                ExplorerSearchKind::Callers,
                "symbol".to_owned(),
                limit,
                gain,
                rationale.clone(),
            ),
            Err(ExplorerSearchActionError::QueryKindMismatch)
        );
        assert!(
            ExplorerSearchAction::graph(
                ExplorerSearchKind::Callers,
                SymbolId::from_bytes([8; 32]),
                limit,
                gain,
                rationale,
            )
            .is_ok()
        );
        Ok(())
    }

    #[test]
    fn proposal_debug_output_never_contains_field_values() -> Result<(), Box<dyn std::error::Error>>
    {
        let proposal = ModuleCardProposal::new(
            ModuleCardProposalEnvelope::new(
                ModuleCardId::from_bytes([2; 32]),
                ModuleId::from_bytes([3; 32]),
                SnapshotId::from_bytes([4; 32]),
                ModuleCardSchemaVersion::V1,
                MapperProfileVersion::V1,
                Confidence::certain(),
            ),
            vec![ProposedModuleCardField::new(
                ModuleCardField::Title,
                vec!["sensitive module title".to_owned()],
                vec![ModuleCardEvidenceId::from_bytes([1; 32])],
            )?],
            512,
        )?;
        assert!(!format!("{proposal:?}").contains("sensitive module title"));
        assert_eq!(proposal.status(), ModuleCardStatus::Proposed);
        Ok(())
    }
}
