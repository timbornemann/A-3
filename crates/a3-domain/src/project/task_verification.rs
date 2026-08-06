use super::{AgentRunId, StepVerificationId, TaskEvidenceId, VerificationSpecId};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_EXPECTED_EVIDENCE_BYTES: usize = 4 * 1_024;
const MAX_VERIFICATION_REQUIREMENT_BYTES: usize = 8 * 1_024;
const MAX_VERIFICATION_FAILURE_BYTES: usize = 8 * 1_024;
const MAX_VERIFICATION_EVIDENCE: usize = 64;
const MAX_PERSISTED_TIMESTAMP_MILLIS: u64 = i64::MAX as u64;

macro_rules! verification_text_type {
    ($(#[$metadata:meta])* $name:ident, $field:literal, $maximum:expr) => {
        $(#[$metadata])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Normalizes and validates one bounded non-empty verification text value.
            pub fn try_from_string(value: String) -> Result<Self, TaskVerificationTextError> {
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

verification_text_type!(
    /// One evidence outcome expected from a task-step verification.
    ExpectedTaskEvidence,
    "expected evidence",
    MAX_EXPECTED_EVIDENCE_BYTES
);
verification_text_type!(
    /// Deterministic pass condition interpreted by the later verification engine.
    VerificationRequirement,
    "verification requirement",
    MAX_VERIFICATION_REQUIREMENT_BYTES
);
verification_text_type!(
    /// Safe bounded explanation retained for an unsuccessful verification attempt.
    VerificationFailureSummary,
    "verification failure summary",
    MAX_VERIFICATION_FAILURE_BYTES
);

fn normalize_text(
    value: String,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<String, TaskVerificationTextError> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.len() > maximum_bytes {
        return Err(TaskVerificationTextError {
            field,
            violation: TaskVerificationTextViolation::InvalidLength,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    if trimmed.chars().any(|character| {
        character == '\0' || (character.is_control() && character != '\n' && character != '\t')
    }) {
        return Err(TaskVerificationTextError {
            field,
            violation: TaskVerificationTextViolation::InvalidCharacter,
            actual_bytes: trimmed.len(),
            maximum_bytes,
        });
    }
    Ok(trimmed.to_owned())
}

/// Machine-readable failure class for one verification text value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskVerificationTextViolation {
    /// Normalized text was empty or exceeded its fixed UTF-8 byte limit.
    InvalidLength,
    /// Text contained NUL or an unsupported control character.
    InvalidCharacter,
}

/// Invalid bounded text in a task verification definition or result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskVerificationTextError {
    field: &'static str,
    violation: TaskVerificationTextViolation,
    actual_bytes: usize,
    maximum_bytes: usize,
}

impl TaskVerificationTextError {
    /// Returns the stable field name safe for diagnostics.
    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }

    /// Returns the rejected grammar class.
    #[must_use]
    pub const fn violation(self) -> TaskVerificationTextViolation {
        self.violation
    }

    /// Returns the observed normalized UTF-8 byte length.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }

    /// Returns the fixed field limit.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

impl fmt::Display for TaskVerificationTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.violation {
            TaskVerificationTextViolation::InvalidLength => write!(
                formatter,
                "{} has {} bytes; expected 1 through {}",
                self.field, self.actual_bytes, self.maximum_bytes
            ),
            TaskVerificationTextViolation::InvalidCharacter => {
                write!(
                    formatter,
                    "{} contains an unsupported character",
                    self.field
                )
            }
        }
    }
}

impl Error for TaskVerificationTextError {}

/// Non-executable verification category selected by one immutable step specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VerificationMethod {
    /// A bounded typed command result will be checked by the later execution boundary.
    Command,
    /// A test result with expected semantics will be checked.
    Test,
    /// The resulting diff must satisfy one deterministic invariant.
    DiffInvariant,
    /// A diagnostic source must contain or exclude the specified condition.
    Diagnostic,
    /// A scoped user decision is required as evidence.
    UserConfirm,
}

/// Immutable, non-executable pass condition attached to one task-step definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationSpec {
    id: VerificationSpecId,
    method: VerificationMethod,
    requirement: VerificationRequirement,
}

impl VerificationSpec {
    /// Binds a stable identity and method to a bounded deterministic requirement.
    #[must_use]
    pub const fn new(
        id: VerificationSpecId,
        method: VerificationMethod,
        requirement: VerificationRequirement,
    ) -> Self {
        Self {
            id,
            method,
            requirement,
        }
    }

    /// Returns the stable specification identity.
    #[must_use]
    pub const fn id(&self) -> VerificationSpecId {
        self.id
    }

    /// Returns the non-executable verification category.
    #[must_use]
    pub const fn method(&self) -> VerificationMethod {
        self.method
    }

    /// Returns the normalized pass condition.
    #[must_use]
    pub const fn requirement(&self) -> &VerificationRequirement {
        &self.requirement
    }
}

/// Persistable timestamp shared by Task Ledger transitions and verification records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskLedgerTimestamp(u64);

impl TaskLedgerTimestamp {
    /// Creates an exactly persistable Unix-millisecond timestamp.
    pub const fn from_unix_millis(value: u64) -> Result<Self, TaskLedgerTimestampError> {
        if value > MAX_PERSISTED_TIMESTAMP_MILLIS {
            return Err(TaskLedgerTimestampError);
        }
        Ok(Self(value))
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

/// Timestamp exceeded the exact signed range used by local persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskLedgerTimestampError;

impl fmt::Display for TaskLedgerTimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Task Ledger timestamp exceeds the persisted range")
    }
}

impl Error for TaskLedgerTimestampError {}

/// Deterministic outcome recorded by one verification execution.
#[derive(Clone, PartialEq, Eq)]
pub enum StepVerificationOutcome {
    /// The immutable specification was satisfied by the attached fresh evidence.
    Passed,
    /// The specification was not satisfied; the bounded failure is retained for replan.
    Failed {
        /// Safe summary of the observed mismatch.
        summary: VerificationFailureSummary,
    },
}

impl fmt::Debug for StepVerificationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => formatter.write_str("Passed"),
            Self::Failed { summary } => formatter
                .debug_struct("Failed")
                .field("summary_bytes", &summary.as_str().len())
                .finish_non_exhaustive(),
        }
    }
}

/// Immutable evidence-bound result of one verification execution.
#[derive(Clone, PartialEq, Eq)]
pub struct StepVerification {
    id: StepVerificationId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    outcome: StepVerificationOutcome,
    evidence_ids: Vec<TaskEvidenceId>,
    verified_at: TaskLedgerTimestamp,
}

impl StepVerification {
    /// Creates a bounded result only when it retains unique supporting evidence.
    pub fn new(
        id: StepVerificationId,
        spec_id: VerificationSpecId,
        run_id: AgentRunId,
        outcome: StepVerificationOutcome,
        evidence_ids: Vec<TaskEvidenceId>,
        verified_at: TaskLedgerTimestamp,
    ) -> Result<Self, StepVerificationError> {
        if evidence_ids.is_empty() || evidence_ids.len() > MAX_VERIFICATION_EVIDENCE {
            return Err(StepVerificationError::InvalidEvidenceCount(
                evidence_ids.len(),
            ));
        }
        let unique = evidence_ids.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != evidence_ids.len() {
            return Err(StepVerificationError::DuplicateEvidence);
        }
        Ok(Self {
            id,
            spec_id,
            run_id,
            outcome,
            evidence_ids,
            verified_at,
        })
    }

    /// Returns the verification execution identity.
    #[must_use]
    pub const fn id(&self) -> StepVerificationId {
        self.id
    }

    /// Returns the immutable specification that was evaluated.
    #[must_use]
    pub const fn spec_id(&self) -> VerificationSpecId {
        self.spec_id
    }

    /// Returns the controlled run that produced this result.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the deterministic verification outcome.
    #[must_use]
    pub const fn outcome(&self) -> &StepVerificationOutcome {
        &self.outcome
    }

    /// Returns whether the specification was satisfied.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.outcome, StepVerificationOutcome::Passed)
    }

    /// Returns the unique evidence required for completion and freshness checks.
    #[must_use]
    pub fn evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.evidence_ids
    }

    /// Returns the persisted verification time.
    #[must_use]
    pub const fn verified_at(&self) -> TaskLedgerTimestamp {
        self.verified_at
    }
}

impl fmt::Debug for StepVerification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StepVerification")
            .field("id", &self.id)
            .field("spec_id", &self.spec_id)
            .field("run_id", &self.run_id)
            .field("outcome", &self.outcome)
            .field("evidence_count", &self.evidence_ids.len())
            .field("verified_at", &self.verified_at)
            .finish_non_exhaustive()
    }
}

/// Invalid evidence collection for one verification execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepVerificationError {
    /// A verification requires between one and 64 evidence identities.
    InvalidEvidenceCount(usize),
    /// The same evidence identity appeared more than once.
    DuplicateEvidence,
}

impl fmt::Display for StepVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEvidenceCount(count) => write!(
                formatter,
                "verification has {count} evidence items; expected 1 through {MAX_VERIFICATION_EVIDENCE}"
            ),
            Self::DuplicateEvidence => {
                formatter.write_str("verification contains duplicate evidence")
            }
        }
    }
}

impl Error for StepVerificationError {}

#[cfg(test)]
mod tests {
    use super::{
        ExpectedTaskEvidence, StepVerification, StepVerificationError, StepVerificationOutcome,
        TaskLedgerTimestamp, TaskVerificationTextViolation, VerificationFailureSummary,
        VerificationMethod, VerificationRequirement, VerificationSpec,
    };
    use crate::{AgentRunId, StepVerificationId, TaskEvidenceId, VerificationSpecId};
    use std::error::Error;

    #[test]
    fn verification_text_is_normalized_bounded_and_redacted() -> Result<(), Box<dyn Error>> {
        let expected = ExpectedTaskEvidence::try_from_string("  test output\r\n  ".to_owned())?;
        assert_eq!(expected.as_str(), "test output");
        assert!(!format!("{expected:?}").contains("test output"));

        let Err(error) = VerificationRequirement::try_from_string("\0".to_owned()) else {
            return Err("NUL verification text was accepted".into());
        };
        assert_eq!(
            error.violation(),
            TaskVerificationTextViolation::InvalidCharacter
        );
        Ok(())
    }

    #[test]
    fn verification_result_requires_unique_evidence_and_preserves_failure()
    -> Result<(), Box<dyn Error>> {
        let spec = VerificationSpec::new(
            VerificationSpecId::from_bytes([1; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string("all targeted tests pass".to_owned())?,
        );
        let evidence = TaskEvidenceId::from_bytes([2; 32]);
        assert_eq!(
            StepVerification::new(
                StepVerificationId::from_bytes([3; 32]),
                spec.id(),
                AgentRunId::from_bytes([4; 32]),
                StepVerificationOutcome::Passed,
                vec![evidence, evidence],
                TaskLedgerTimestamp::from_unix_millis(1)?,
            ),
            Err(StepVerificationError::DuplicateEvidence)
        );

        let failure = StepVerification::new(
            StepVerificationId::from_bytes([5; 32]),
            spec.id(),
            AgentRunId::from_bytes([4; 32]),
            StepVerificationOutcome::Failed {
                summary: VerificationFailureSummary::try_from_string(
                    "one assertion failed".to_owned(),
                )?,
            },
            vec![evidence],
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        assert!(!failure.passed());
        assert_eq!(failure.evidence_ids(), &[evidence]);
        assert!(!format!("{failure:?}").contains("one assertion failed"));
        Ok(())
    }
}
