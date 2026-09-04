use super::{
    AgentRunId, DiscoveredCommandId, PolicyResourceId, RepositoryPath, StepVerificationId,
    TaskEvidenceId, VerificationSpecId,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_EXPECTED_EVIDENCE_BYTES: usize = 4 * 1_024;
const MAX_VERIFICATION_REQUIREMENT_BYTES: usize = 8 * 1_024;
const MAX_VERIFICATION_FAILURE_BYTES: usize = 8 * 1_024;
const MAX_VERIFICATION_EVIDENCE: usize = 64;
const MAX_VERIFICATION_PATHS: usize = 64;
const MAX_TEST_CASE_NAME_BYTES: usize = 1_024;
const MAX_TEST_CASES: u32 = 1_000_000;
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
    /// Exact normalized test-case selector retained by a typed test specification.
    TestCaseSelectorName,
    "test-case selector",
    MAX_TEST_CASE_NAME_BYTES
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

/// Declared repository breadth used to order relevant process verifications narrowly first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VerificationScope {
    /// One selected test, file, target, or similarly precise unit.
    Targeted,
    /// One package, crate, module, or workspace member.
    Package,
    /// The complete repository workspace.
    Workspace,
}

/// One bounded selector for structured test evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestCaseSelector {
    /// Every case reported by the selected test command is relevant.
    All,
    /// Exactly one normalized case name must be present in the report.
    Exact(TestCaseSelectorName),
}

/// Positive minimum number of selected test cases required for semantic success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MinimumTestCaseCount(u32);

impl MinimumTestCaseCount {
    /// Creates a non-zero case boundary that remains cheap to validate and persist.
    pub const fn new(value: u32) -> Result<Self, MinimumTestCaseCountError> {
        if value == 0 || value > MAX_TEST_CASES {
            return Err(MinimumTestCaseCountError { value });
        }
        Ok(Self(value))
    }

    /// Returns the required selected-case count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Test semantic boundary was zero or exceeded the fixed limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimumTestCaseCountError {
    value: u32,
}

impl fmt::Display for MinimumTestCaseCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "minimum test-case count {} must be between 1 and {MAX_TEST_CASES}",
            self.value
        )
    }
}

impl Error for MinimumTestCaseCountError {}

/// Deterministic path-set relation checked against an actual patch change set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiffInvariantMode {
    /// The verification requires no changed paths.
    NoChanges,
    /// Every changed path must be a member of the declared set.
    OnlyPaths,
    /// The actual changed paths must equal the declared set exactly.
    ExactPaths,
}

/// Typed path invariant with a canonical bounded path set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffInvariantVerification {
    mode: DiffInvariantMode,
    paths: Vec<RepositoryPath>,
}

impl DiffInvariantVerification {
    /// Creates a path invariant; only `NoChanges` accepts an empty path set.
    pub fn new(
        mode: DiffInvariantMode,
        mut paths: Vec<RepositoryPath>,
    ) -> Result<Self, DiffInvariantVerificationError> {
        paths.sort();
        if paths.len() > MAX_VERIFICATION_PATHS {
            return Err(DiffInvariantVerificationError::TooManyPaths {
                actual: paths.len(),
            });
        }
        if paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(DiffInvariantVerificationError::DuplicatePath);
        }
        if (mode == DiffInvariantMode::NoChanges) != paths.is_empty() {
            return Err(DiffInvariantVerificationError::InvalidPathCount);
        }
        Ok(Self { mode, paths })
    }

    /// Returns the closed set relation.
    #[must_use]
    pub const fn mode(&self) -> DiffInvariantMode {
        self.mode
    }

    /// Returns canonical paths participating in the invariant.
    #[must_use]
    pub fn paths(&self) -> &[RepositoryPath] {
        &self.paths
    }
}

/// Invalid path set for a deterministic diff invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffInvariantVerificationError {
    /// More than 64 paths were supplied.
    TooManyPaths {
        /// Observed path count.
        actual: usize,
    },
    /// One path appeared more than once.
    DuplicatePath,
    /// Empty versus non-empty paths did not match the selected mode.
    InvalidPathCount,
}

impl fmt::Display for DiffInvariantVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyPaths { .. } => "diff invariant exceeds 64 paths",
            Self::DuplicatePath => "diff invariant repeats a path",
            Self::InvalidPathCount => "diff invariant path count does not match its mode",
        })
    }
}

impl Error for DiffInvariantVerificationError {}

/// Maximum normalized diagnostic severity tolerated by one verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticPolicy {
    /// Error diagnostics fail; warnings and informational diagnostics are allowed.
    NoErrors,
    /// Errors and warnings both fail.
    NoWarnings,
}

/// Executable V1 target carried by a step verification specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationTarget {
    /// Historical H2 method-plus-text definition; retained for audit but not executable by E6.
    Legacy(VerificationMethod),
    /// One exact allowlisted discovered command must exit successfully.
    Command {
        /// Evidence-bound discovered command identity.
        command_id: DiscoveredCommandId,
        /// Declared breadth used only for deterministic scheduling.
        scope: VerificationScope,
    },
    /// One exact command must provide structured matching test-case semantics.
    Test {
        /// Evidence-bound discovered test command identity.
        command_id: DiscoveredCommandId,
        /// Relevant structured cases.
        selector: TestCaseSelector,
        /// Required number of matching cases.
        minimum_cases: MinimumTestCaseCount,
        /// Declared breadth used only for deterministic scheduling.
        scope: VerificationScope,
    },
    /// One deterministic relation must hold for the actual changed paths.
    DiffInvariant(DiffInvariantVerification),
    /// One exact diagnostic command must satisfy the closed severity policy.
    Diagnostic {
        /// Evidence-bound discovered lint or format command identity.
        command_id: DiscoveredCommandId,
        /// Maximum tolerated diagnostic severity.
        policy: DiagnosticPolicy,
        /// Declared breadth used only for deterministic scheduling.
        scope: VerificationScope,
    },
    /// One explicit user confirmation must match this content-free scope.
    UserConfirm {
        /// Stable scope digest displayed to and confirmed by the user.
        scope_id: PolicyResourceId,
    },
}

impl VerificationTarget {
    /// Returns the closed public verification category.
    #[must_use]
    pub const fn method(&self) -> VerificationMethod {
        match self {
            Self::Legacy(method) => *method,
            Self::Command { .. } => VerificationMethod::Command,
            Self::Test { .. } => VerificationMethod::Test,
            Self::DiffInvariant(_) => VerificationMethod::DiffInvariant,
            Self::Diagnostic { .. } => VerificationMethod::Diagnostic,
            Self::UserConfirm { .. } => VerificationMethod::UserConfirm,
        }
    }

    /// Returns whether E6 may execute this typed target.
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        !matches!(self, Self::Legacy(_))
    }
}

/// Immutable, non-executable pass condition attached to one task-step definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationSpec {
    id: VerificationSpecId,
    target: VerificationTarget,
    requirement: VerificationRequirement,
}

impl VerificationSpec {
    /// Reuses one immutable verification contract for a newly introduced replan step.
    ///
    /// The target and requirement remain unchanged; only the Core-owned identity is replaced so
    /// retained historical steps and their replacements cannot share a specification identity.
    #[must_use]
    pub fn reidentified(&self, id: VerificationSpecId) -> Self {
        Self {
            id,
            target: self.target.clone(),
            requirement: self.requirement.clone(),
        }
    }

    /// Reconstructs a historical H2 method-plus-text specification for audit and migration.
    ///
    /// Legacy specifications are deliberately not executable by the E6 verification engine.
    #[must_use]
    pub const fn new(
        id: VerificationSpecId,
        method: VerificationMethod,
        requirement: VerificationRequirement,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::Legacy(method),
            requirement,
        }
    }

    /// Creates an operational exact-command specification.
    #[must_use]
    pub const fn command(
        id: VerificationSpecId,
        requirement: VerificationRequirement,
        command_id: DiscoveredCommandId,
        scope: VerificationScope,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::Command { command_id, scope },
            requirement,
        }
    }

    /// Creates an operational structured-test specification.
    #[must_use]
    pub const fn test(
        id: VerificationSpecId,
        requirement: VerificationRequirement,
        command_id: DiscoveredCommandId,
        selector: TestCaseSelector,
        minimum_cases: MinimumTestCaseCount,
        scope: VerificationScope,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::Test {
                command_id,
                selector,
                minimum_cases,
                scope,
            },
            requirement,
        }
    }

    /// Creates an operational deterministic diff-invariant specification.
    #[must_use]
    pub const fn diff_invariant(
        id: VerificationSpecId,
        requirement: VerificationRequirement,
        invariant: DiffInvariantVerification,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::DiffInvariant(invariant),
            requirement,
        }
    }

    /// Creates an operational structured-diagnostic specification.
    #[must_use]
    pub const fn diagnostic(
        id: VerificationSpecId,
        requirement: VerificationRequirement,
        command_id: DiscoveredCommandId,
        policy: DiagnosticPolicy,
        scope: VerificationScope,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::Diagnostic {
                command_id,
                policy,
                scope,
            },
            requirement,
        }
    }

    /// Creates an operational exact-scope user-confirmation specification.
    #[must_use]
    pub const fn user_confirm(
        id: VerificationSpecId,
        requirement: VerificationRequirement,
        scope_id: PolicyResourceId,
    ) -> Self {
        Self {
            id,
            target: VerificationTarget::UserConfirm { scope_id },
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
        self.target.method()
    }

    /// Returns the typed operational target or explicit legacy classification.
    #[must_use]
    pub const fn target(&self) -> &VerificationTarget {
        &self.target
    }

    /// Returns whether the E6 engine may evaluate this specification.
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        self.target.is_operational()
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
        DiffInvariantMode, DiffInvariantVerification, ExpectedTaskEvidence, MinimumTestCaseCount,
        StepVerification, StepVerificationError, StepVerificationOutcome, TaskLedgerTimestamp,
        TaskVerificationTextViolation, TestCaseSelector, VerificationFailureSummary,
        VerificationMethod, VerificationRequirement, VerificationScope, VerificationSpec,
        VerificationTarget,
    };
    use crate::{
        AgentRunId, DiscoveredCommandId, RepositoryPath, StepVerificationId, TaskEvidenceId,
        VerificationSpecId,
    };
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

    #[test]
    fn operational_specs_are_typed_and_legacy_text_is_not_executable() -> Result<(), Box<dyn Error>>
    {
        let requirement =
            || VerificationRequirement::try_from_string("targeted tests pass".to_owned());
        let legacy = VerificationSpec::new(
            VerificationSpecId::from_bytes([1; 32]),
            VerificationMethod::Test,
            requirement()?,
        );
        assert!(!legacy.is_operational());

        let command_id = DiscoveredCommandId::from_bytes([2; 32]);
        let test = VerificationSpec::test(
            VerificationSpecId::from_bytes([3; 32]),
            requirement()?,
            command_id,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Targeted,
        );
        assert!(test.is_operational());
        assert!(matches!(
            test.target(),
            VerificationTarget::Test {
                command_id: actual,
                selector: TestCaseSelector::All,
                minimum_cases,
                ..
            } if *actual == command_id && minimum_cases.get() == 1
        ));
        Ok(())
    }

    #[test]
    fn diff_invariant_paths_are_canonical_bounded_and_mode_safe() -> Result<(), Box<dyn Error>> {
        let first = RepositoryPath::try_from_bytes(b"src/a.rs".to_vec())?;
        let second = RepositoryPath::try_from_bytes(b"src/b.rs".to_vec())?;
        let invariant = DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![second.clone(), first.clone()],
        )?;
        assert_eq!(invariant.paths(), &[first, second]);
        assert!(
            DiffInvariantVerification::new(
                DiffInvariantMode::NoChanges,
                vec![RepositoryPath::try_from_bytes(b"src/a.rs".to_vec())?,]
            )
            .is_err()
        );
        Ok(())
    }
}
