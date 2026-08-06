use super::{AcceptanceCriterionId, TaskId};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_OBJECTIVE_BYTES: usize = 16 * 1_024;
const MAX_ITEM_BYTES: usize = 4 * 1_024;
const MAX_SUCCESS_VERIFICATION_BYTES: usize = 8 * 1_024;
const MAX_REVISION_REASON_BYTES: usize = 4 * 1_024;
const MAX_COLLECTION_ITEMS: usize = 64;
const MAX_PERSISTED_TIMESTAMP_MILLIS: u64 = i64::MAX as u64;

/// Goal Contract text field whose bounded grammar was validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractTextField {
    /// Required task outcome.
    Objective,
    /// One independently verifiable success condition.
    AcceptanceCriterion,
    /// One mandatory execution or product boundary.
    Constraint,
    /// One outcome explicitly excluded from the task.
    NonGoal,
    /// One explicit choice confirmed by the user.
    UserDecision,
    /// Overall verification required before the task may finish.
    SuccessVerification,
    /// Explanation for a material Goal Contract revision.
    RevisionReason,
}

impl fmt::Display for GoalContractTextField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Objective => "objective",
            Self::AcceptanceCriterion => "acceptance criterion",
            Self::Constraint => "constraint",
            Self::NonGoal => "non-goal",
            Self::UserDecision => "user decision",
            Self::SuccessVerification => "success verification",
            Self::RevisionReason => "revision reason",
        })
    }
}

/// Why bounded Goal Contract text was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractTextViolation {
    /// Normalized text was empty or exceeded its field-specific UTF-8 byte limit.
    InvalidLength,
    /// Text contained NUL or an unsupported control character.
    InvalidCharacter,
}

/// Invalid normalized Goal Contract text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalContractTextError {
    field: GoalContractTextField,
    violation: GoalContractTextViolation,
    actual_bytes: usize,
    maximum_bytes: usize,
}

impl GoalContractTextError {
    /// Returns the rejected field.
    #[must_use]
    pub const fn field(self) -> GoalContractTextField {
        self.field
    }

    /// Returns the machine-readable validation failure.
    #[must_use]
    pub const fn violation(self) -> GoalContractTextViolation {
        self.violation
    }

    /// Returns the normalized UTF-8 byte length observed at the boundary.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }

    /// Returns the fixed field-specific UTF-8 byte limit.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

impl fmt::Display for GoalContractTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.violation {
            GoalContractTextViolation::InvalidLength => write!(
                formatter,
                "Goal Contract {} has {} bytes; expected 1 through {}",
                self.field, self.actual_bytes, self.maximum_bytes
            ),
            GoalContractTextViolation::InvalidCharacter => write!(
                formatter,
                "Goal Contract {} contains an unsupported character",
                self.field
            ),
        }
    }
}

impl Error for GoalContractTextError {}

macro_rules! goal_text_type {
    ($(#[$metadata:meta])* $name:ident, $field:expr, $maximum:expr) => {
        $(#[$metadata])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Normalizes line endings and validates a bounded non-empty text value.
            pub fn try_from_string(value: String) -> Result<Self, GoalContractTextError> {
                normalize_text(value, $field, $maximum).map(Self)
            }

            /// Returns the normalized text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("bytes", &self.0.len())
                    .finish_non_exhaustive()
            }
        }
    };
}

goal_text_type!(
    /// Required task outcome retained in every model anchor.
    GoalObjective,
    GoalContractTextField::Objective,
    MAX_OBJECTIVE_BYTES
);
goal_text_type!(
    /// Independently verifiable statement carried by one acceptance criterion.
    AcceptanceCriterionStatement,
    GoalContractTextField::AcceptanceCriterion,
    MAX_ITEM_BYTES
);
goal_text_type!(
    /// Mandatory execution or product boundary for one task.
    GoalConstraint,
    GoalContractTextField::Constraint,
    MAX_ITEM_BYTES
);
goal_text_type!(
    /// Outcome explicitly excluded from one task revision.
    NonGoal,
    GoalContractTextField::NonGoal,
    MAX_ITEM_BYTES
);
goal_text_type!(
    /// Explicit task choice confirmed by the user.
    UserDecision,
    GoalContractTextField::UserDecision,
    MAX_ITEM_BYTES
);
goal_text_type!(
    /// Overall evidence-producing verification required before task completion.
    SuccessVerification,
    GoalContractTextField::SuccessVerification,
    MAX_SUCCESS_VERIFICATION_BYTES
);
goal_text_type!(
    /// Required explanation for a material revision after task creation.
    GoalRevisionReason,
    GoalContractTextField::RevisionReason,
    MAX_REVISION_REASON_BYTES
);

fn normalize_text(
    value: String,
    field: GoalContractTextField,
    maximum_bytes: usize,
) -> Result<String, GoalContractTextError> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.len() > maximum_bytes {
        return Err(GoalContractTextError {
            field,
            violation: GoalContractTextViolation::InvalidLength,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    if trimmed.chars().any(|character| {
        character == '\0' || (character.is_control() && character != '\n' && character != '\t')
    }) {
        return Err(GoalContractTextError {
            field,
            violation: GoalContractTextViolation::InvalidCharacter,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    Ok(trimmed.to_owned())
}

/// One stable mandatory success condition within a revisioned Goal Contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AcceptanceCriterion {
    id: AcceptanceCriterionId,
    statement: AcceptanceCriterionStatement,
}

impl AcceptanceCriterion {
    /// Binds a stable criterion identity to a bounded verification statement.
    #[must_use]
    pub const fn new(id: AcceptanceCriterionId, statement: AcceptanceCriterionStatement) -> Self {
        Self { id, statement }
    }

    /// Returns the stable criterion identity.
    #[must_use]
    pub const fn id(&self) -> AcceptanceCriterionId {
        self.id
    }

    /// Returns the normalized required outcome.
    #[must_use]
    pub const fn statement(&self) -> &AcceptanceCriterionStatement {
        &self.statement
    }
}

/// Goal Contract collection whose cardinality or uniqueness was invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractCollection {
    /// Mandatory acceptance criteria.
    AcceptanceCriteria,
    /// Mandatory task constraints.
    Constraints,
    /// Explicit non-goals.
    NonGoals,
    /// Confirmed user decisions.
    UserDecisions,
}

impl fmt::Display for GoalContractCollection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AcceptanceCriteria => "acceptance criteria",
            Self::Constraints => "constraints",
            Self::NonGoals => "non-goals",
            Self::UserDecisions => "user decisions",
        })
    }
}

/// Invalid content collection supplied for one Goal Contract revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractDraftError {
    /// Acceptance criteria were empty or a collection exceeded 64 items.
    InvalidCount {
        /// Rejected collection.
        collection: GoalContractCollection,
        /// Observed item count.
        count: usize,
    },
    /// A collection repeated an identity or normalized value.
    DuplicateItem(GoalContractCollection),
}

impl fmt::Display for GoalContractDraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCount { collection, count } => write!(
                formatter,
                "Goal Contract {collection} contains {count} items; acceptance criteria require 1 through {MAX_COLLECTION_ITEMS} and other collections allow 0 through {MAX_COLLECTION_ITEMS}"
            ),
            Self::DuplicateItem(collection) => {
                write!(formatter, "Goal Contract {collection} contains a duplicate")
            }
        }
    }
}

impl Error for GoalContractDraftError {}

/// Validated mutable input from which immutable Goal Contract revisions are created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalContractDraft {
    objective: GoalObjective,
    acceptance_criteria: Vec<AcceptanceCriterion>,
    constraints: Vec<GoalConstraint>,
    non_goals: Vec<NonGoal>,
    user_decisions: Vec<UserDecision>,
    success_verification: SuccessVerification,
}

impl GoalContractDraft {
    /// Validates all fixed cardinality and uniqueness rules for one revision.
    pub fn new(
        objective: GoalObjective,
        acceptance_criteria: Vec<AcceptanceCriterion>,
        constraints: Vec<GoalConstraint>,
        non_goals: Vec<NonGoal>,
        user_decisions: Vec<UserDecision>,
        success_verification: SuccessVerification,
    ) -> Result<Self, GoalContractDraftError> {
        validate_count(
            GoalContractCollection::AcceptanceCriteria,
            acceptance_criteria.len(),
            true,
        )?;
        validate_count(
            GoalContractCollection::Constraints,
            constraints.len(),
            false,
        )?;
        validate_count(GoalContractCollection::NonGoals, non_goals.len(), false)?;
        validate_count(
            GoalContractCollection::UserDecisions,
            user_decisions.len(),
            false,
        )?;
        let criterion_ids = acceptance_criteria
            .iter()
            .map(AcceptanceCriterion::id)
            .collect::<BTreeSet<_>>();
        let criterion_statements = acceptance_criteria
            .iter()
            .map(AcceptanceCriterion::statement)
            .collect::<BTreeSet<_>>();
        if criterion_ids.len() != acceptance_criteria.len()
            || criterion_statements.len() != acceptance_criteria.len()
        {
            return Err(GoalContractDraftError::DuplicateItem(
                GoalContractCollection::AcceptanceCriteria,
            ));
        }
        validate_unique(&constraints, GoalContractCollection::Constraints)?;
        validate_unique(&non_goals, GoalContractCollection::NonGoals)?;
        validate_unique(&user_decisions, GoalContractCollection::UserDecisions)?;
        Ok(Self {
            objective,
            acceptance_criteria,
            constraints,
            non_goals,
            user_decisions,
            success_verification,
        })
    }

    /// Returns the required task outcome.
    #[must_use]
    pub const fn objective(&self) -> &GoalObjective {
        &self.objective
    }

    /// Returns mandatory success conditions in user-defined order.
    #[must_use]
    pub fn acceptance_criteria(&self) -> &[AcceptanceCriterion] {
        &self.acceptance_criteria
    }

    /// Returns mandatory boundaries in user-defined order.
    #[must_use]
    pub fn constraints(&self) -> &[GoalConstraint] {
        &self.constraints
    }

    /// Returns explicit exclusions in user-defined order.
    #[must_use]
    pub fn non_goals(&self) -> &[NonGoal] {
        &self.non_goals
    }

    /// Returns confirmed decisions in user-defined order.
    #[must_use]
    pub fn user_decisions(&self) -> &[UserDecision] {
        &self.user_decisions
    }

    /// Returns the overall completion verification.
    #[must_use]
    pub const fn success_verification(&self) -> &SuccessVerification {
        &self.success_verification
    }
}

fn validate_count(
    collection: GoalContractCollection,
    count: usize,
    required: bool,
) -> Result<(), GoalContractDraftError> {
    if count > MAX_COLLECTION_ITEMS || (required && count == 0) {
        Err(GoalContractDraftError::InvalidCount { collection, count })
    } else {
        Ok(())
    }
}

fn validate_unique<T: Ord>(
    values: &[T],
    collection: GoalContractCollection,
) -> Result<(), GoalContractDraftError> {
    if values.iter().collect::<BTreeSet<_>>().len() == values.len() {
        Ok(())
    } else {
        Err(GoalContractDraftError::DuplicateItem(collection))
    }
}

/// Positive monotone revision number of one Goal Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalContractRevision(u32);

impl GoalContractRevision {
    /// Initial immutable Goal Contract revision.
    pub const INITIAL: Self = Self(1);

    /// Validates a persisted positive revision number.
    pub fn new(value: u32) -> Result<Self, GoalContractRevisionError> {
        if value == 0 {
            Err(GoalContractRevisionError::Zero)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the portable integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn next(self) -> Result<Self, GoalContractRevisionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(GoalContractRevisionError::Exhausted)
    }
}

/// Invalid Goal Contract revision number or exhausted revision space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractRevisionError {
    /// Persisted revision zero is invalid.
    Zero,
    /// The maximum portable revision cannot be incremented.
    Exhausted,
}

impl fmt::Display for GoalContractRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Zero => "Goal Contract revision must be positive",
            Self::Exhausted => "Goal Contract revision space is exhausted",
        })
    }
}

impl Error for GoalContractRevisionError {}

/// Portable non-negative creation time stored as Unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalContractTimestamp(u64);

impl GoalContractTimestamp {
    /// Validates a timestamp representable by signed SQLite integer storage.
    pub fn from_unix_millis(value: u64) -> Result<Self, GoalContractTimestampError> {
        if value > MAX_PERSISTED_TIMESTAMP_MILLIS {
            Err(GoalContractTimestampError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

/// Goal Contract timestamp outside the portable persistence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalContractTimestampError {
    value: u64,
}

impl fmt::Display for GoalContractTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Goal Contract timestamp {} exceeds the portable persistence range",
            self.value
        )
    }
}

impl Error for GoalContractTimestampError {}

/// Immutable validated revision of one durable task goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalContract {
    task_id: TaskId,
    revision: GoalContractRevision,
    previous_revision: Option<GoalContractRevision>,
    revision_reason: Option<GoalRevisionReason>,
    draft: GoalContractDraft,
    created_at: GoalContractTimestamp,
}

impl GoalContract {
    /// Creates the first immutable revision of a durable task goal.
    #[must_use]
    pub const fn initial(
        task_id: TaskId,
        draft: GoalContractDraft,
        created_at: GoalContractTimestamp,
    ) -> Self {
        Self {
            task_id,
            revision: GoalContractRevision::INITIAL,
            previous_revision: None,
            revision_reason: None,
            draft,
            created_at,
        }
    }

    /// Reconstructs one locally valid revision before full history validation.
    pub fn reconstruct(
        task_id: TaskId,
        revision: GoalContractRevision,
        previous_revision: Option<GoalContractRevision>,
        revision_reason: Option<GoalRevisionReason>,
        draft: GoalContractDraft,
        created_at: GoalContractTimestamp,
    ) -> Result<Self, GoalContractRestoreError> {
        if revision == GoalContractRevision::INITIAL {
            if previous_revision.is_some() || revision_reason.is_some() {
                return Err(GoalContractRestoreError::InvalidInitialMetadata);
            }
        } else {
            let expected_previous = GoalContractRevision(revision.0 - 1);
            if previous_revision != Some(expected_previous) || revision_reason.is_none() {
                return Err(GoalContractRestoreError::InvalidRevisionMetadata);
            }
        }
        Ok(Self {
            task_id,
            revision,
            previous_revision,
            revision_reason,
            draft,
            created_at,
        })
    }

    /// Creates the next revision while leaving this revision unchanged and auditable.
    pub fn revise(
        &self,
        draft: GoalContractDraft,
        reason: GoalRevisionReason,
        created_at: GoalContractTimestamp,
    ) -> Result<Self, GoalContractRevisionFailure> {
        if draft == self.draft {
            return Err(GoalContractRevisionFailure::NoMaterialChange);
        }
        if created_at < self.created_at {
            return Err(GoalContractRevisionFailure::TimestampRegressed);
        }
        let revision = self
            .revision
            .next()
            .map_err(|_| GoalContractRevisionFailure::RevisionExhausted)?;
        Ok(Self {
            task_id: self.task_id,
            revision,
            previous_revision: Some(self.revision),
            revision_reason: Some(reason),
            draft,
            created_at,
        })
    }

    /// Returns the durable task identity.
    #[must_use]
    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }

    /// Returns this immutable revision number.
    #[must_use]
    pub const fn revision(&self) -> GoalContractRevision {
        self.revision
    }

    /// Returns the direct predecessor for revised contracts.
    #[must_use]
    pub const fn previous_revision(&self) -> Option<GoalContractRevision> {
        self.previous_revision
    }

    /// Returns why this revision changed, absent only for the initial revision.
    #[must_use]
    pub const fn revision_reason(&self) -> Option<&GoalRevisionReason> {
        self.revision_reason.as_ref()
    }

    /// Returns all validated content of this revision.
    #[must_use]
    pub const fn draft(&self) -> &GoalContractDraft {
        &self.draft
    }

    /// Returns this revision's durable creation time.
    #[must_use]
    pub const fn created_at(&self) -> GoalContractTimestamp {
        self.created_at
    }

    /// Returns the stable pair that future Agent Runs must retain.
    #[must_use]
    pub const fn reference(&self) -> GoalContractReference {
        GoalContractReference {
            task_id: self.task_id,
            revision: self.revision,
        }
    }
}

/// Persisted Goal Contract metadata was internally inconsistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractRestoreError {
    /// Revision one unexpectedly carried predecessor metadata.
    InvalidInitialMetadata,
    /// A later revision lacked its immediate predecessor or revision reason.
    InvalidRevisionMetadata,
}

impl fmt::Display for GoalContractRestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInitialMetadata => {
                "initial Goal Contract revision cannot carry predecessor metadata"
            }
            Self::InvalidRevisionMetadata => {
                "revised Goal Contract must cite its immediate predecessor and a reason"
            }
        })
    }
}

impl Error for GoalContractRestoreError {}

/// A requested Goal Contract revision did not represent a valid next revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractRevisionFailure {
    /// All durable goal content was identical to the current revision.
    NoMaterialChange,
    /// Revision numbering exhausted the portable integer range.
    RevisionExhausted,
    /// New revision creation time preceded the current revision.
    TimestampRegressed,
}

impl fmt::Display for GoalContractRevisionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoMaterialChange => "Goal Contract revision has no material change",
            Self::RevisionExhausted => "Goal Contract revision space is exhausted",
            Self::TimestampRegressed => "Goal Contract revision timestamp regressed",
        })
    }
}

impl Error for GoalContractRevisionFailure {}

/// Stable task-and-revision anchor required by future Agent Run construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalContractReference {
    task_id: TaskId,
    revision: GoalContractRevision,
}

impl GoalContractReference {
    /// Returns the durable task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    /// Returns the exact immutable Goal Contract revision.
    #[must_use]
    pub const fn revision(self) -> GoalContractRevision {
        self.revision
    }
}

/// Complete chronological Goal Contract history retained for audit and resume.
#[derive(Clone, PartialEq, Eq)]
pub struct GoalContractHistory {
    revisions: Vec<GoalContract>,
}

impl GoalContractHistory {
    /// Validates a non-empty, contiguous, single-task chronological revision chain.
    pub fn new(revisions: Vec<GoalContract>) -> Result<Self, GoalContractHistoryError> {
        let Some(initial) = revisions.first() else {
            return Err(GoalContractHistoryError::Empty);
        };
        if initial.revision != GoalContractRevision::INITIAL
            || initial.previous_revision.is_some()
            || initial.revision_reason.is_some()
        {
            return Err(GoalContractHistoryError::MissingInitialRevision);
        }
        for pair in revisions.windows(2) {
            let previous = &pair[0];
            let current = &pair[1];
            if current.task_id != initial.task_id {
                return Err(GoalContractHistoryError::MixedTask);
            }
            let expected_revision = previous
                .revision
                .next()
                .map_err(|_| GoalContractHistoryError::NonContiguous)?;
            if current.revision != expected_revision
                || current.previous_revision != Some(previous.revision)
            {
                return Err(GoalContractHistoryError::NonContiguous);
            }
            if current.created_at < previous.created_at {
                return Err(GoalContractHistoryError::TimestampRegressed);
            }
            if current.draft == previous.draft {
                return Err(GoalContractHistoryError::NoMaterialChange);
            }
        }
        Ok(Self { revisions })
    }

    /// Appends an already validated immediate revision while preserving all prior entries.
    pub fn append(&mut self, revision: GoalContract) -> Result<(), GoalContractHistoryError> {
        let current = self.current();
        if revision.task_id != current.task_id {
            return Err(GoalContractHistoryError::MixedTask);
        }
        let expected = current
            .revision
            .next()
            .map_err(|_| GoalContractHistoryError::NonContiguous)?;
        if revision.revision != expected
            || revision.previous_revision != Some(current.revision)
            || revision.revision_reason.is_none()
        {
            return Err(GoalContractHistoryError::NonContiguous);
        }
        if revision.created_at < current.created_at {
            return Err(GoalContractHistoryError::TimestampRegressed);
        }
        if revision.draft == current.draft {
            return Err(GoalContractHistoryError::NoMaterialChange);
        }
        self.revisions.push(revision);
        Ok(())
    }

    /// Returns the durable task identity shared by every revision.
    #[must_use]
    pub fn task_id(&self) -> TaskId {
        self.revisions[0].task_id
    }

    /// Returns every immutable revision in chronological order.
    #[must_use]
    pub fn revisions(&self) -> &[GoalContract] {
        &self.revisions
    }

    /// Returns the latest immutable revision.
    #[must_use]
    pub fn current(&self) -> &GoalContract {
        &self.revisions[self.revisions.len() - 1]
    }
}

impl fmt::Debug for GoalContractHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoalContractHistory")
            .field("task_id", &self.task_id())
            .field("revision_count", &self.revisions.len())
            .field("current_revision", &self.current().revision)
            .finish()
    }
}

/// Invalid chronological Goal Contract history reconstructed from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalContractHistoryError {
    /// A task cannot exist without a Goal Contract revision.
    Empty,
    /// The chain did not start at revision one.
    MissingInitialRevision,
    /// At least one revision belonged to another task.
    MixedTask,
    /// Revision numbers or direct predecessor links had a gap.
    NonContiguous,
    /// A later revision's creation time preceded its predecessor.
    TimestampRegressed,
    /// A later revision repeated all durable goal content.
    NoMaterialChange,
}

impl fmt::Display for GoalContractHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Goal Contract history is empty",
            Self::MissingInitialRevision => "Goal Contract history does not start at revision one",
            Self::MixedTask => "Goal Contract history mixes task identities",
            Self::NonContiguous => "Goal Contract history is not contiguous",
            Self::TimestampRegressed => "Goal Contract history timestamp regressed",
            Self::NoMaterialChange => "Goal Contract history contains a content-identical revision",
        })
    }
}

impl Error for GoalContractHistoryError {}

#[cfg(test)]
mod tests {
    use super::{
        AcceptanceCriterion, AcceptanceCriterionStatement, GoalConstraint, GoalContract,
        GoalContractCollection, GoalContractDraft, GoalContractDraftError, GoalContractHistory,
        GoalContractHistoryError, GoalContractRevision, GoalContractRevisionFailure,
        GoalContractTextViolation, GoalContractTimestamp, GoalObjective, GoalRevisionReason,
        MAX_OBJECTIVE_BYTES, NonGoal, SuccessVerification, UserDecision,
    };
    use crate::{AcceptanceCriterionId, TaskId};
    use std::error::Error;

    #[test]
    fn initial_contract_requires_bounded_normalized_content() -> Result<(), Box<dyn Error>> {
        let contract = GoalContract::initial(
            TaskId::from_bytes([1; 32]),
            draft("ship feature\r\nwith tests")?,
            GoalContractTimestamp::from_unix_millis(10)?,
        );

        assert_eq!(contract.revision(), GoalContractRevision::INITIAL);
        assert_eq!(contract.previous_revision(), None);
        assert_eq!(contract.revision_reason(), None);
        assert_eq!(
            contract.draft().objective().as_str(),
            "ship feature\nwith tests"
        );
        assert_eq!(contract.draft().acceptance_criteria().len(), 1);
        assert_eq!(contract.reference().task_id(), contract.task_id());
        assert_eq!(contract.reference().revision(), contract.revision());

        let control = GoalObjective::try_from_string("unsafe\u{0007}text".to_owned());
        assert!(matches!(
            control,
            Err(error) if error.violation() == GoalContractTextViolation::InvalidCharacter
        ));
        assert!(GoalObjective::try_from_string(" ".to_owned()).is_err());
        assert!(GoalObjective::try_from_string("x".repeat(MAX_OBJECTIVE_BYTES + 1)).is_err());
        Ok(())
    }

    #[test]
    fn draft_rejects_missing_and_duplicate_contract_items() -> Result<(), Box<dyn Error>> {
        let empty = GoalContractDraft::new(
            GoalObjective::try_from_string("goal".to_owned())?,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("verify".to_owned())?,
        );
        assert_eq!(
            empty,
            Err(GoalContractDraftError::InvalidCount {
                collection: GoalContractCollection::AcceptanceCriteria,
                count: 0,
            })
        );

        let repeated = GoalConstraint::try_from_string("offline".to_owned())?;
        let duplicate = GoalContractDraft::new(
            GoalObjective::try_from_string("goal".to_owned())?,
            vec![criterion(1, "works")?],
            vec![repeated.clone(), repeated],
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("verify".to_owned())?,
        );
        assert_eq!(
            duplicate,
            Err(GoalContractDraftError::DuplicateItem(
                GoalContractCollection::Constraints
            ))
        );

        let duplicate_criterion_id = GoalContractDraft::new(
            GoalObjective::try_from_string("goal".to_owned())?,
            vec![criterion(2, "first")?, criterion(2, "second")?],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("verify".to_owned())?,
        );
        assert!(matches!(
            duplicate_criterion_id,
            Err(GoalContractDraftError::DuplicateItem(
                GoalContractCollection::AcceptanceCriteria
            ))
        ));
        Ok(())
    }

    #[test]
    fn revision_preserves_old_content_and_requires_a_material_change() -> Result<(), Box<dyn Error>>
    {
        let initial = GoalContract::initial(
            TaskId::from_bytes([3; 32]),
            draft("initial goal")?,
            GoalContractTimestamp::from_unix_millis(20)?,
        );
        assert_eq!(
            initial.revise(
                initial.draft().clone(),
                GoalRevisionReason::try_from_string("clarify".to_owned())?,
                GoalContractTimestamp::from_unix_millis(21)?,
            ),
            Err(GoalContractRevisionFailure::NoMaterialChange)
        );

        let revised = initial.revise(
            draft("revised goal")?,
            GoalRevisionReason::try_from_string("user changed the outcome".to_owned())?,
            GoalContractTimestamp::from_unix_millis(21)?,
        )?;
        assert_eq!(initial.draft().objective().as_str(), "initial goal");
        assert_eq!(revised.draft().objective().as_str(), "revised goal");
        assert_eq!(revised.revision().get(), 2);
        assert_eq!(revised.previous_revision(), Some(initial.revision()));
        assert_eq!(revised.task_id(), initial.task_id());
        assert_eq!(
            initial.revise(
                draft("another goal")?,
                GoalRevisionReason::try_from_string("regressed time".to_owned())?,
                GoalContractTimestamp::from_unix_millis(19)?,
            ),
            Err(GoalContractRevisionFailure::TimestampRegressed)
        );
        Ok(())
    }

    #[test]
    fn history_rejects_gaps_mixed_tasks_and_content_identical_rows() -> Result<(), Box<dyn Error>> {
        let initial = GoalContract::initial(
            TaskId::from_bytes([4; 32]),
            draft("initial")?,
            GoalContractTimestamp::from_unix_millis(30)?,
        );
        let revised = initial.revise(
            draft("revised")?,
            GoalRevisionReason::try_from_string("changed".to_owned())?,
            GoalContractTimestamp::from_unix_millis(31)?,
        )?;
        let history = GoalContractHistory::new(vec![initial.clone(), revised.clone()])?;
        assert_eq!(history.revisions(), &[initial.clone(), revised.clone()]);
        assert_eq!(history.current(), &revised);

        let mixed = GoalContract::reconstruct(
            TaskId::from_bytes([5; 32]),
            GoalContractRevision::new(2)?,
            Some(GoalContractRevision::INITIAL),
            Some(GoalRevisionReason::try_from_string("changed".to_owned())?),
            draft("other task")?,
            GoalContractTimestamp::from_unix_millis(31)?,
        )?;
        assert_eq!(
            GoalContractHistory::new(vec![initial.clone(), mixed]),
            Err(GoalContractHistoryError::MixedTask)
        );

        let gap = GoalContract::reconstruct(
            initial.task_id(),
            GoalContractRevision::new(3)?,
            Some(GoalContractRevision::new(2)?),
            Some(GoalRevisionReason::try_from_string("jumped".to_owned())?),
            draft("third")?,
            GoalContractTimestamp::from_unix_millis(32)?,
        )?;
        assert_eq!(
            GoalContractHistory::new(vec![initial.clone(), gap]),
            Err(GoalContractHistoryError::NonContiguous)
        );

        let identical = GoalContract::reconstruct(
            initial.task_id(),
            GoalContractRevision::new(2)?,
            Some(GoalContractRevision::INITIAL),
            Some(GoalRevisionReason::try_from_string(
                "claimed change".to_owned(),
            )?),
            initial.draft().clone(),
            GoalContractTimestamp::from_unix_millis(31)?,
        )?;
        assert_eq!(
            GoalContractHistory::new(vec![initial, identical]),
            Err(GoalContractHistoryError::NoMaterialChange)
        );
        Ok(())
    }

    #[test]
    fn debug_output_redacts_user_supplied_goal_text() -> Result<(), Box<dyn Error>> {
        let contract = GoalContract::initial(
            TaskId::from_bytes([6; 32]),
            draft("never-print-this-objective")?,
            GoalContractTimestamp::from_unix_millis(40)?,
        );
        let debug = format!("{contract:?}");
        assert!(!debug.contains("never-print-this-objective"));
        assert!(debug.contains("bytes"));
        Ok(())
    }

    fn criterion(id: u8, statement: &str) -> Result<AcceptanceCriterion, Box<dyn Error>> {
        Ok(AcceptanceCriterion::new(
            AcceptanceCriterionId::from_bytes([id; 32]),
            AcceptanceCriterionStatement::try_from_string(statement.to_owned())?,
        ))
    }

    fn draft(objective: &str) -> Result<GoalContractDraft, Box<dyn Error>> {
        Ok(GoalContractDraft::new(
            GoalObjective::try_from_string(objective.to_owned())?,
            vec![criterion(9, "all checks pass")?],
            vec![GoalConstraint::try_from_string("stay offline".to_owned())?],
            vec![NonGoal::try_from_string("no release".to_owned())?],
            vec![UserDecision::try_from_string(
                "use the documented architecture".to_owned(),
            )?],
            SuccessVerification::try_from_string("run the required quality gate".to_owned())?,
        )?)
    }
}
