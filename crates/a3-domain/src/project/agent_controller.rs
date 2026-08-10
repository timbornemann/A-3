use super::{
    AcceptanceCriterionId, AcceptanceCriterionRequirement, AgentAction, AgentRunId,
    AgentRunTimestamp, GoalContract, GoalContractReference, ModelTokenCount, SnapshotId,
    TaskEvidenceId, TaskLedgerRevision,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_RUN_TURNS: u32 = 10_000;
const MAX_RUN_TOKENS: u64 = i64::MAX as u64;
const MAX_RUN_ACTIONS: u32 = 100_000;
const MAX_RUN_DURATION_MILLIS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_RUN_REPAIRS: u32 = 10_000;
const MAX_ACCEPTANCE_EVIDENCE_PER_CRITERION: usize = 64;

/// Current evidence proving one mandatory acceptance criterion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriterionVerification {
    criterion_id: AcceptanceCriterionId,
    evidence_ids: Vec<TaskEvidenceId>,
}

impl AcceptanceCriterionVerification {
    /// Binds one criterion to a bounded unique non-empty current-evidence set.
    pub fn new(
        criterion_id: AcceptanceCriterionId,
        mut evidence_ids: Vec<TaskEvidenceId>,
    ) -> Result<Self, AcceptanceVerificationError> {
        if evidence_ids.is_empty() || evidence_ids.len() > MAX_ACCEPTANCE_EVIDENCE_PER_CRITERION {
            return Err(AcceptanceVerificationError::InvalidEvidenceCount(
                evidence_ids.len(),
            ));
        }
        evidence_ids.sort_unstable();
        if evidence_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AcceptanceVerificationError::DuplicateEvidence);
        }
        Ok(Self {
            criterion_id,
            evidence_ids,
        })
    }

    /// Returns the verified Goal Contract criterion.
    #[must_use]
    pub const fn criterion_id(&self) -> AcceptanceCriterionId {
        self.criterion_id
    }

    /// Returns current evidence retained by the verifier.
    #[must_use]
    pub fn evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.evidence_ids
    }
}

/// Complete acceptance-verifier receipt required before a run can enter Done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceVerificationReceipt {
    run_id: AgentRunId,
    goal_contract: GoalContractReference,
    ledger_revision: TaskLedgerRevision,
    snapshot_id: SnapshotId,
    criteria: Vec<AcceptanceCriterionVerification>,
}

impl AcceptanceVerificationReceipt {
    /// Validates exact coverage of every mandatory criterion in one immutable Goal revision.
    pub fn new(
        run_id: AgentRunId,
        goal_contract: &GoalContract,
        ledger_revision: TaskLedgerRevision,
        snapshot_id: SnapshotId,
        mut criteria: Vec<AcceptanceCriterionVerification>,
    ) -> Result<Self, AcceptanceVerificationError> {
        criteria.sort_by_key(AcceptanceCriterionVerification::criterion_id);
        if criteria
            .windows(2)
            .any(|pair| pair[0].criterion_id == pair[1].criterion_id)
        {
            return Err(AcceptanceVerificationError::DuplicateCriterion);
        }
        let expected = goal_contract
            .draft()
            .acceptance_criteria()
            .iter()
            .filter(|criterion| criterion.requirement() == AcceptanceCriterionRequirement::Must)
            .map(|criterion| criterion.id())
            .collect::<BTreeSet<_>>();
        let actual = criteria
            .iter()
            .map(AcceptanceCriterionVerification::criterion_id)
            .collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(AcceptanceVerificationError::CriterionCoverageMismatch);
        }
        Ok(Self {
            run_id,
            goal_contract: goal_contract.reference(),
            ledger_revision,
            snapshot_id,
            criteria,
        })
    }

    /// Returns the run whose acceptance was verified.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact verified Goal Contract revision.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the exact verified Task Ledger revision.
    #[must_use]
    pub const fn ledger_revision(&self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the immutable repository snapshot checked by the verifier.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns complete criterion coverage in stable identity order.
    #[must_use]
    pub fn criteria(&self) -> &[AcceptanceCriterionVerification] {
        &self.criteria
    }
}

/// Acceptance evidence did not prove every mandatory criterion exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceVerificationError {
    /// One criterion had no evidence or exceeded the fixed evidence boundary.
    InvalidEvidenceCount(usize),
    /// One evidence identity was repeated for a criterion.
    DuplicateEvidence,
    /// One criterion appeared more than once.
    DuplicateCriterion,
    /// Receipt criteria differed from the immutable Goal Contract criteria.
    CriterionCoverageMismatch,
}

impl fmt::Display for AcceptanceVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEvidenceCount(_) => "acceptance criterion evidence count is invalid",
            Self::DuplicateEvidence => "acceptance criterion evidence contains a duplicate",
            Self::DuplicateCriterion => "acceptance verification repeats a criterion",
            Self::CriterionCoverageMismatch => {
                "acceptance verification does not cover the exact Goal Contract criteria"
            }
        })
    }
}

impl Error for AcceptanceVerificationError {}

macro_rules! positive_u32_limit {
    ($(#[$metadata:meta])* $name:ident, $error:ident, $maximum:expr, $label:literal) => {
        $(#[$metadata])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            /// Creates a positive controller limit within its fixed allocation boundary.
            pub const fn new(value: u32) -> Result<Self, $error> {
                if value == 0 || value > $maximum {
                    Err($error { value })
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the persisted integer representation.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        /// Controller limit was zero or exceeded its fixed boundary.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $error {
            value: u32,
        }

        impl fmt::Display for $error {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "{} {} must be between 1 and {}",
                    $label,
                    self.value,
                    $maximum
                )
            }
        }

        impl Error for $error {}
    };
}

positive_u32_limit!(
    /// Maximum number of bounded model turns in one run.
    AgentTurnLimit,
    AgentTurnLimitError,
    MAX_RUN_TURNS,
    "agent turn limit"
);
positive_u32_limit!(
    /// Maximum number of model-selected actions in one run.
    AgentActionLimit,
    AgentActionLimitError,
    MAX_RUN_ACTIONS,
    "agent action limit"
);
positive_u32_limit!(
    /// Maximum number of structured-output repair attempts in one run.
    AgentRepairLimit,
    AgentRepairLimitError,
    MAX_RUN_REPAIRS,
    "agent repair limit"
);

/// Maximum cumulative prompt or output tokens in one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentTokenLimit(u64);

impl AgentTokenLimit {
    /// Creates a positive token limit exactly representable by local persistence.
    pub const fn new(value: u64) -> Result<Self, AgentTokenLimitError> {
        if value == 0 || value > MAX_RUN_TOKENS {
            Err(AgentTokenLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Token limit was zero or exceeded signed persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTokenLimitError {
    value: u64,
}

impl fmt::Display for AgentTokenLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "agent token limit {} must be between 1 and {MAX_RUN_TOKENS}",
            self.value
        )
    }
}

impl Error for AgentTokenLimitError {}

/// Maximum wall-clock duration of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentRunDurationLimit(u64);

impl AgentRunDurationLimit {
    /// Creates a positive duration capped at seven days.
    pub const fn from_millis(value: u64) -> Result<Self, AgentRunDurationLimitError> {
        if value == 0 || value > MAX_RUN_DURATION_MILLIS {
            Err(AgentRunDurationLimitError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the persisted millisecond representation.
    #[must_use]
    pub const fn millis(self) -> u64 {
        self.0
    }
}

/// Run-duration limit was zero or exceeded seven days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunDurationLimitError {
    value: u64,
}

impl fmt::Display for AgentRunDurationLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "agent run duration {} ms must be between 1 and {MAX_RUN_DURATION_MILLIS}",
            self.value
        )
    }
}

impl Error for AgentRunDurationLimitError {}

/// Immutable hard ceilings selected when one agent run starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunBudget {
    turn_limit: AgentTurnLimit,
    prompt_token_limit: AgentTokenLimit,
    output_token_limit: AgentTokenLimit,
    action_limit: AgentActionLimit,
    duration_limit: AgentRunDurationLimit,
    repair_limit: AgentRepairLimit,
}

impl AgentRunBudget {
    /// Conservative V1 defaults for one bounded local read-only run.
    pub const DEFAULT: Self = Self {
        turn_limit: AgentTurnLimit(128),
        prompt_token_limit: AgentTokenLimit(2_097_152),
        output_token_limit: AgentTokenLimit(524_288),
        action_limit: AgentActionLimit(128),
        duration_limit: AgentRunDurationLimit(7_200_000),
        repair_limit: AgentRepairLimit(32),
    };

    /// Combines independently validated hard limits.
    #[must_use]
    pub const fn new(
        turn_limit: AgentTurnLimit,
        prompt_token_limit: AgentTokenLimit,
        output_token_limit: AgentTokenLimit,
        action_limit: AgentActionLimit,
        duration_limit: AgentRunDurationLimit,
        repair_limit: AgentRepairLimit,
    ) -> Self {
        Self {
            turn_limit,
            prompt_token_limit,
            output_token_limit,
            action_limit,
            duration_limit,
            repair_limit,
        }
    }

    /// Returns the maximum model turns.
    #[must_use]
    pub const fn turn_limit(self) -> AgentTurnLimit {
        self.turn_limit
    }

    /// Returns the maximum cumulative prompt tokens.
    #[must_use]
    pub const fn prompt_token_limit(self) -> AgentTokenLimit {
        self.prompt_token_limit
    }

    /// Returns the maximum cumulative output tokens.
    #[must_use]
    pub const fn output_token_limit(self) -> AgentTokenLimit {
        self.output_token_limit
    }

    /// Returns the maximum model-selected actions.
    #[must_use]
    pub const fn action_limit(self) -> AgentActionLimit {
        self.action_limit
    }

    /// Returns the wall-clock run deadline relative to creation.
    #[must_use]
    pub const fn duration_limit(self) -> AgentRunDurationLimit {
        self.duration_limit
    }

    /// Returns the maximum structured-output repair attempts.
    #[must_use]
    pub const fn repair_limit(self) -> AgentRepairLimit {
        self.repair_limit
    }

    /// Returns the first deterministically ordered exhausted dimension, if any.
    pub fn exhaustion(
        self,
        usage: AgentRunUsage,
        created_at: AgentRunTimestamp,
        observed_at: AgentRunTimestamp,
    ) -> Result<Option<AgentBudgetExhaustion>, AgentBudgetEvaluationError> {
        let elapsed = observed_at
            .unix_millis()
            .checked_sub(created_at.unix_millis())
            .ok_or(AgentBudgetEvaluationError::TimestampRegressed)?;
        let candidates = [
            (
                AgentBudgetDimension::Time,
                self.duration_limit.millis(),
                elapsed,
            ),
            (
                AgentBudgetDimension::Turns,
                u64::from(self.turn_limit.get()),
                u64::from(usage.turn_count),
            ),
            (
                AgentBudgetDimension::PromptTokens,
                self.prompt_token_limit.get(),
                usage.prompt_tokens,
            ),
            (
                AgentBudgetDimension::OutputTokens,
                self.output_token_limit.get(),
                usage.output_tokens,
            ),
            (
                AgentBudgetDimension::Actions,
                u64::from(self.action_limit.get()),
                u64::from(usage.action_count),
            ),
            (
                AgentBudgetDimension::Repairs,
                u64::from(self.repair_limit.get()),
                u64::from(usage.repair_count),
            ),
        ];
        Ok(candidates
            .into_iter()
            .find(|(_, limit, observed)| observed >= limit)
            .map(|(dimension, limit, observed)| AgentBudgetExhaustion {
                dimension,
                limit,
                observed,
            }))
    }
}

/// Coarse class of the sole optional model-selected action in one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentTurnActionClass {
    /// Deterministic bounded retrieval.
    Search,
    /// Targeted bounded read-only inspection.
    Inspect,
    /// Safe non-verifying Task Ledger update.
    UpdateLedger,
    /// Request for deterministic acceptance verification.
    Finish,
    /// One complete structured full-file patch.
    ApplyPatch,
    /// One discovered and plan-bound direct process.
    Run,
}

impl AgentTurnActionClass {
    /// Classifies one already validated V1 action without retaining untrusted content.
    #[must_use]
    pub const fn from_action(action: &AgentAction) -> Self {
        match action {
            AgentAction::Search(_) => Self::Search,
            AgentAction::Inspect(_) => Self::Inspect,
            AgentAction::UpdateLedger(_) => Self::UpdateLedger,
            AgentAction::Finish(_) => Self::Finish,
            AgentAction::ApplyPatch(_) => Self::ApplyPatch,
            AgentAction::Run(_) => Self::Run,
        }
    }
}

/// Per-turn structured-output repair consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentTurnRepairUsage {
    /// Primary output decoded successfully.
    None,
    /// The single allowed repair path was consumed.
    One,
}

impl AgentTurnRepairUsage {
    const fn count(self) -> u32 {
        match self {
            Self::None => 0,
            Self::One => 1,
        }
    }
}

/// Actual bounded resource charge of one model turn and at most one selected action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTurnCharge {
    prompt_tokens: ModelTokenCount,
    output_tokens: ModelTokenCount,
    action: Option<AgentTurnActionClass>,
    repair: AgentTurnRepairUsage,
}

impl AgentTurnCharge {
    /// Records token counts and zero or one selected action for a single turn.
    #[must_use]
    pub const fn new(
        prompt_tokens: ModelTokenCount,
        output_tokens: ModelTokenCount,
        action: Option<AgentTurnActionClass>,
        repair: AgentTurnRepairUsage,
    ) -> Self {
        Self {
            prompt_tokens,
            output_tokens,
            action,
            repair,
        }
    }

    /// Returns charged prompt tokens.
    #[must_use]
    pub const fn prompt_tokens(self) -> ModelTokenCount {
        self.prompt_tokens
    }

    /// Returns charged output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> ModelTokenCount {
        self.output_tokens
    }

    /// Returns the sole selected action class, if decoding produced one.
    #[must_use]
    pub const fn action(self) -> Option<AgentTurnActionClass> {
        self.action
    }

    /// Returns whether the one repair path was consumed.
    #[must_use]
    pub const fn repair(self) -> AgentTurnRepairUsage {
        self.repair
    }
}

/// Durable cumulative usage materialized with one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentRunUsage {
    turn_count: u32,
    prompt_tokens: u64,
    output_tokens: u64,
    action_count: u32,
    repair_count: u32,
}

impl AgentRunUsage {
    /// Zero usage for a newly started run.
    pub const ZERO: Self = Self {
        turn_count: 0,
        prompt_tokens: 0,
        output_tokens: 0,
        action_count: 0,
        repair_count: 0,
    };

    /// Reconstructs persisted counters and rejects impossible per-turn cardinalities.
    pub const fn reconstruct(
        turn_count: u32,
        prompt_tokens: u64,
        output_tokens: u64,
        action_count: u32,
        repair_count: u32,
    ) -> Result<Self, AgentRunUsageError> {
        if action_count > turn_count || repair_count > turn_count {
            return Err(AgentRunUsageError::InvalidCardinality);
        }
        Ok(Self {
            turn_count,
            prompt_tokens,
            output_tokens,
            action_count,
            repair_count,
        })
    }

    /// Applies exactly one bounded turn charge using checked arithmetic.
    pub fn record_turn(self, charge: AgentTurnCharge) -> Result<Self, AgentRunUsageError> {
        let Some(turn_count) = self.turn_count.checked_add(1) else {
            return Err(AgentRunUsageError::Overflow);
        };
        let Some(prompt_tokens) = self
            .prompt_tokens
            .checked_add(u64::from(charge.prompt_tokens.get()))
        else {
            return Err(AgentRunUsageError::Overflow);
        };
        let Some(output_tokens) = self
            .output_tokens
            .checked_add(u64::from(charge.output_tokens.get()))
        else {
            return Err(AgentRunUsageError::Overflow);
        };
        let Some(action_count) =
            self.action_count
                .checked_add(if charge.action.is_some() { 1 } else { 0 })
        else {
            return Err(AgentRunUsageError::Overflow);
        };
        let Some(repair_count) = self.repair_count.checked_add(charge.repair.count()) else {
            return Err(AgentRunUsageError::Overflow);
        };
        Ok(Self {
            turn_count,
            prompt_tokens,
            output_tokens,
            action_count,
            repair_count,
        })
    }

    /// Returns completed model turns.
    #[must_use]
    pub const fn turn_count(self) -> u32 {
        self.turn_count
    }

    /// Returns cumulative prompt tokens.
    #[must_use]
    pub const fn prompt_tokens(self) -> u64 {
        self.prompt_tokens
    }

    /// Returns cumulative output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }

    /// Returns model-selected actions, necessarily no greater than turns.
    #[must_use]
    pub const fn action_count(self) -> u32 {
        self.action_count
    }

    /// Returns consumed one-shot repair attempts.
    #[must_use]
    pub const fn repair_count(self) -> u32 {
        self.repair_count
    }
}

/// Persisted usage was impossible or checked accumulation overflowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunUsageError {
    /// More actions or repairs existed than turns.
    InvalidCardinality,
    /// A cumulative counter exceeded its integer representation.
    Overflow,
}

impl fmt::Display for AgentRunUsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCardinality => "agent run usage has impossible turn cardinality",
            Self::Overflow => "agent run usage counter overflowed",
        })
    }
}

impl Error for AgentRunUsageError {}

/// Hard budget dimension selected by deterministic exhaustion priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentBudgetDimension {
    /// Wall-clock deadline.
    Time,
    /// Model turns.
    Turns,
    /// Prompt tokens.
    PromptTokens,
    /// Generated tokens.
    OutputTokens,
    /// Model-selected actions.
    Actions,
    /// Structured-output repair attempts.
    Repairs,
}

/// Stable content-free explanation for one exhausted hard limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentBudgetExhaustion {
    dimension: AgentBudgetDimension,
    limit: u64,
    observed: u64,
}

impl AgentBudgetExhaustion {
    /// Returns the exhausted resource.
    #[must_use]
    pub const fn dimension(self) -> AgentBudgetDimension {
        self.dimension
    }

    /// Returns the configured hard ceiling.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }

    /// Returns the observed cumulative amount.
    #[must_use]
    pub const fn observed(self) -> u64 {
        self.observed
    }
}

/// Budget evaluation received a timestamp before run creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBudgetEvaluationError {
    /// Observed wall-clock time preceded the immutable run start.
    TimestampRegressed,
}

impl fmt::Display for AgentBudgetEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent budget observation timestamp regressed")
    }
}

impl Error for AgentBudgetEvaluationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_turn_can_represent_zero_or_one_action_but_never_more() -> Result<(), Box<dyn Error>> {
        let empty = AgentTurnCharge::new(
            ModelTokenCount::new(10),
            ModelTokenCount::new(2),
            None,
            AgentTurnRepairUsage::One,
        );
        let action = AgentTurnCharge::new(
            ModelTokenCount::new(10),
            ModelTokenCount::new(2),
            Some(AgentTurnActionClass::Inspect),
            AgentTurnRepairUsage::None,
        );
        let usage = AgentRunUsage::default()
            .record_turn(empty)?
            .record_turn(action)?;

        assert_eq!(usage.turn_count(), 2);
        assert_eq!(usage.action_count(), 1);
        assert_eq!(usage.repair_count(), 1);
        assert!(AgentRunUsage::reconstruct(1, 0, 0, 2, 0).is_err());
        Ok(())
    }

    #[test]
    fn budget_exhaustion_is_deterministic_and_sticky() -> Result<(), Box<dyn Error>> {
        let budget = AgentRunBudget::new(
            AgentTurnLimit::new(2)?,
            AgentTokenLimit::new(100)?,
            AgentTokenLimit::new(50)?,
            AgentActionLimit::new(2)?,
            AgentRunDurationLimit::from_millis(1_000)?,
            AgentRepairLimit::new(1)?,
        );
        let created = AgentRunTimestamp::from_unix_millis(10)?;
        let charge = AgentTurnCharge::new(
            ModelTokenCount::new(50),
            ModelTokenCount::new(10),
            Some(AgentTurnActionClass::Search),
            AgentTurnRepairUsage::None,
        );
        let first = AgentRunUsage::default().record_turn(charge)?;
        assert_eq!(budget.exhaustion(first, created, created)?, None);
        let second = first.record_turn(charge)?;
        assert_eq!(
            budget
                .exhaustion(second, created, created)?
                .map(AgentBudgetExhaustion::dimension),
            Some(AgentBudgetDimension::Turns)
        );
        let third = second.record_turn(charge)?;
        assert_eq!(
            budget
                .exhaustion(third, created, created)?
                .map(AgentBudgetExhaustion::dimension),
            Some(AgentBudgetDimension::Turns)
        );
        Ok(())
    }
}
