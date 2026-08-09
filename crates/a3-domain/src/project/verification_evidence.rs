use super::{
    AgentRunId, DiagnosticPolicy, DiffInvariantMode, DiscoveredCommandId, FileDelta, FileRevision,
    IndexRunId, PatchActionDigest, PatchChange, PatchChangeSet, PolicyDecisionId, PolicyResourceId,
    ProcessDuration, ProcessOutputCapture, ProcessOutputDigest, ProcessOutputRedaction,
    ProcessRunResult, ProcessTermination, PublishedIndex, RepositoryFileState, RepositoryPath,
    SnapshotDelta, SnapshotId, TaskEvidenceId, TaskLedgerTimestamp, TestCaseSelector, ToolRunId,
    VerificationMethod, VerificationRunId, VerificationSpec, VerificationSpecId,
    VerificationTarget,
};
use std::error::Error;
use std::fmt;

const MAX_VERIFICATION_DEPENDENCIES: usize = 512;
const MAX_TEST_CASE_NAME_BYTES: usize = 1_024;
const MAX_TEST_CASE_EVIDENCE: usize = 1_000_000;

/// Version of the durable, typed verification-evidence envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VerificationEvidenceSchemaVersion {
    /// Initial schema binding evidence to runs, specs, snapshots, and exact dependencies.
    V1,
}

/// One current path state on which a verification result depends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceDependency {
    /// The exact content-addressed path must remain present.
    Present(FileRevision),
    /// The path must remain absent.
    Absent(RepositoryPath),
}

impl EvidenceDependency {
    /// Returns the path whose state controls freshness.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        match self {
            Self::Present(revision) => revision.path(),
            Self::Absent(path) => path,
        }
    }
}

/// Canonical bounded path states required for evidence freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationDependencies(Vec<EvidenceDependency>);

impl VerificationDependencies {
    /// Canonicalizes dependencies and rejects duplicate path states.
    pub fn new(
        mut dependencies: Vec<EvidenceDependency>,
    ) -> Result<Self, VerificationDependenciesError> {
        if dependencies.len() > MAX_VERIFICATION_DEPENDENCIES {
            return Err(VerificationDependenciesError::TooMany {
                actual: dependencies.len(),
            });
        }
        dependencies.sort_by(|left, right| left.path().cmp(right.path()));
        if dependencies
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(VerificationDependenciesError::DuplicatePath);
        }
        Ok(Self(dependencies))
    }

    /// Returns canonical dependencies in repository-path order.
    #[must_use]
    pub fn as_slice(&self) -> &[EvidenceDependency] {
        &self.0
    }

    /// Returns whether freshness must conservatively require the exact snapshot.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A verification dependency set exceeded its fixed boundary or contradicted itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationDependenciesError {
    /// More than 512 path states were supplied.
    TooMany {
        /// Observed dependency count.
        actual: usize,
    },
    /// More than one state was supplied for the same repository path.
    DuplicatePath,
}

impl fmt::Display for VerificationDependenciesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany { actual } => write!(
                formatter,
                "verification has {actual} dependencies; maximum is {MAX_VERIFICATION_DEPENDENCIES}"
            ),
            Self::DuplicatePath => {
                formatter.write_str("verification dependencies repeat a repository path")
            }
        }
    }
}

impl Error for VerificationDependenciesError {}

/// Content-free, bounded summary of one completely drained process stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessStreamEvidence {
    digest: ProcessOutputDigest,
    observed_bytes: u64,
    retained_limit: u32,
    truncated: bool,
    redaction: Option<ProcessOutputRedaction>,
}

impl ProcessStreamEvidence {
    fn from_capture(capture: &ProcessOutputCapture) -> Self {
        Self {
            digest: capture.digest(),
            observed_bytes: capture.observed_bytes(),
            retained_limit: capture.retained_limit(),
            truncated: capture.truncated(),
            redaction: capture.content().redaction(),
        }
    }

    /// Reconstructs content-free persisted stream metadata after invariant validation.
    pub fn from_stored(
        digest: ProcessOutputDigest,
        observed_bytes: u64,
        retained_limit: u32,
        truncated: bool,
        redaction: Option<ProcessOutputRedaction>,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        if retained_limit == 0
            || (truncated && observed_bytes <= u64::from(retained_limit))
            || (!truncated && redaction.is_none() && observed_bytes > u64::from(retained_limit))
        {
            return Err(VerificationEvidenceBuildError::InvalidProcessStream);
        }
        Ok(Self {
            digest,
            observed_bytes,
            retained_limit,
            truncated,
            redaction,
        })
    }

    /// Returns the digest of every observed byte, including discarded overflow.
    #[must_use]
    pub const fn digest(self) -> ProcessOutputDigest {
        self.digest
    }

    /// Returns the number of completely drained stream bytes.
    #[must_use]
    pub const fn observed_bytes(self) -> u64 {
        self.observed_bytes
    }

    /// Returns the safe-text retention limit applied by the process boundary.
    #[must_use]
    pub const fn retained_limit(self) -> u32 {
        self.retained_limit
    }

    /// Returns whether overflow bytes were discarded after digesting.
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }

    /// Returns why retained content was withheld, if applicable.
    #[must_use]
    pub const fn redaction(self) -> Option<ProcessOutputRedaction> {
        self.redaction
    }
}

/// Immutable IDs binding a command process to one verification attempt and snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandEvidenceContext {
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    tool_run_id: ToolRunId,
    command_id: DiscoveredCommandId,
    snapshot_id: SnapshotId,
}

impl CommandEvidenceContext {
    /// Binds one tool run to the command and immutable verification context it serves.
    #[must_use]
    pub const fn new(
        verification_run_id: VerificationRunId,
        spec_id: VerificationSpecId,
        run_id: AgentRunId,
        tool_run_id: ToolRunId,
        command_id: DiscoveredCommandId,
        snapshot_id: SnapshotId,
    ) -> Self {
        Self {
            verification_run_id,
            spec_id,
            run_id,
            tool_run_id,
            command_id,
            snapshot_id,
        }
    }
}

/// Exact process-run evidence for one allowlisted discovered command.
#[derive(Clone, PartialEq, Eq)]
pub struct CommandEvidence {
    id: TaskEvidenceId,
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    tool_run_id: ToolRunId,
    command_id: DiscoveredCommandId,
    snapshot_id: SnapshotId,
    process_specification_id: PolicyResourceId,
    policy_decision_id: PolicyDecisionId,
    termination: ProcessTermination,
    duration: ProcessDuration,
    stdout: ProcessStreamEvidence,
    stderr: ProcessStreamEvidence,
    dependencies: VerificationDependencies,
}

impl CommandEvidence {
    /// Binds one final process result to its exact command, run, snapshot, and dependencies.
    #[must_use]
    pub fn new(
        context: CommandEvidenceContext,
        dependencies: VerificationDependencies,
        result: &ProcessRunResult,
    ) -> Self {
        let stdout = ProcessStreamEvidence::from_capture(result.stdout());
        let stderr = ProcessStreamEvidence::from_capture(result.stderr());
        let mut evidence = Self {
            id: TaskEvidenceId::from_bytes([0; 32]),
            verification_run_id: context.verification_run_id,
            spec_id: context.spec_id,
            run_id: context.run_id,
            tool_run_id: context.tool_run_id,
            command_id: context.command_id,
            snapshot_id: context.snapshot_id,
            process_specification_id: result.specification_id(),
            policy_decision_id: result.policy_decision_id(),
            termination: result.termination(),
            duration: result.duration(),
            stdout,
            stderr,
            dependencies,
        };
        evidence.id = derive_command_evidence_id(&evidence);
        evidence
    }

    /// Reconstructs persisted content-free process evidence and verifies its derived identity.
    pub fn from_stored(
        expected_id: TaskEvidenceId,
        context: CommandEvidenceContext,
        process: StoredProcessEvidence,
        dependencies: VerificationDependencies,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        let mut evidence = Self {
            id: expected_id,
            verification_run_id: context.verification_run_id,
            spec_id: context.spec_id,
            run_id: context.run_id,
            tool_run_id: context.tool_run_id,
            command_id: context.command_id,
            snapshot_id: context.snapshot_id,
            process_specification_id: process.specification_id,
            policy_decision_id: process.policy_decision_id,
            termination: process.termination,
            duration: process.duration,
            stdout: process.stdout,
            stderr: process.stderr,
            dependencies,
        };
        let actual_id = derive_command_evidence_id(&evidence);
        if actual_id != expected_id {
            return Err(VerificationEvidenceBuildError::EvidenceIdentityMismatch);
        }
        evidence.id = actual_id;
        Ok(evidence)
    }

    /// Returns the durable evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the verification attempt shared by derived evidence.
    #[must_use]
    pub const fn verification_run_id(&self) -> VerificationRunId {
        self.verification_run_id
    }

    /// Returns the immutable verification specification.
    #[must_use]
    pub const fn spec_id(&self) -> VerificationSpecId {
        self.spec_id
    }

    /// Returns the controlled agent run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the bounded tool run that executed the command.
    #[must_use]
    pub const fn tool_run_id(&self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the exact discovered command identity.
    #[must_use]
    pub const fn command_id(&self) -> DiscoveredCommandId {
        self.command_id
    }

    /// Returns the worktree snapshot observed by the command.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact normalized process specification authorized by policy.
    #[must_use]
    pub const fn process_specification_id(&self) -> PolicyResourceId {
        self.process_specification_id
    }

    /// Returns the policy decision that opened the process boundary.
    #[must_use]
    pub const fn policy_decision_id(&self) -> PolicyDecisionId {
        self.policy_decision_id
    }

    /// Returns exit, timeout, or cancellation without interpreting command semantics.
    #[must_use]
    pub const fn termination(&self) -> ProcessTermination {
        self.termination
    }

    /// Returns the monotonic process duration.
    #[must_use]
    pub const fn duration(&self) -> ProcessDuration {
        self.duration
    }

    /// Returns the content-free stdout proof.
    #[must_use]
    pub const fn stdout(&self) -> ProcessStreamEvidence {
        self.stdout
    }

    /// Returns the content-free stderr proof.
    #[must_use]
    pub const fn stderr(&self) -> ProcessStreamEvidence {
        self.stderr
    }

    /// Returns exact path states controlling freshness.
    #[must_use]
    pub const fn dependencies(&self) -> &VerificationDependencies {
        &self.dependencies
    }

    /// Returns whether only the generic process-success condition is satisfied.
    #[must_use]
    pub const fn process_succeeded(&self) -> bool {
        matches!(self.termination, ProcessTermination::Exited(exit) if exit.success())
    }
}

impl fmt::Debug for CommandEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandEvidence")
            .field("id", &self.id)
            .field("verification_run_id", &self.verification_run_id)
            .field("spec_id", &self.spec_id)
            .field("run_id", &self.run_id)
            .field("tool_run_id", &self.tool_run_id)
            .field("command_id", &self.command_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("process_specification_id", &self.process_specification_id)
            .field("policy_decision_id", &self.policy_decision_id)
            .field("termination", &self.termination)
            .field("duration", &self.duration)
            .field("stdout", &self.stdout)
            .field("stderr", &self.stderr)
            .field("dependency_count", &self.dependencies.as_slice().len())
            .finish()
    }
}

/// Content-free persisted process fields used to reconstruct CommandEvidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredProcessEvidence {
    specification_id: PolicyResourceId,
    policy_decision_id: PolicyDecisionId,
    termination: ProcessTermination,
    duration: ProcessDuration,
    stdout: ProcessStreamEvidence,
    stderr: ProcessStreamEvidence,
}

impl StoredProcessEvidence {
    /// Groups exact process metadata without exposing retained command output.
    #[must_use]
    pub const fn new(
        specification_id: PolicyResourceId,
        policy_decision_id: PolicyDecisionId,
        termination: ProcessTermination,
        duration: ProcessDuration,
        stdout: ProcessStreamEvidence,
        stderr: ProcessStreamEvidence,
    ) -> Self {
        Self {
            specification_id,
            policy_decision_id,
            termination,
            duration,
            stdout,
            stderr,
        }
    }
}

/// Bounded structured test-case name emitted by a test adapter.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TestCaseName(String);

impl TestCaseName {
    /// Normalizes one non-empty safe test-case name.
    pub fn try_from_string(value: String) -> Result<Self, TestCaseNameError> {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_TEST_CASE_NAME_BYTES {
            return Err(TestCaseNameError::InvalidLength {
                actual: trimmed.len(),
            });
        }
        if trimmed.chars().any(|character| {
            character == '\0' || (character.is_control() && character != '\n' && character != '\t')
        }) {
            return Err(TestCaseNameError::InvalidCharacter);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the normalized selector-compatible name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TestCaseName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TestCaseName")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Structured test-case name crossed its fixed text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestCaseNameError {
    /// The normalized name was empty or exceeded 1,024 bytes.
    InvalidLength {
        /// Observed normalized byte length.
        actual: usize,
    },
    /// The name contained NUL or an unsupported control character.
    InvalidCharacter,
}

impl fmt::Display for TestCaseNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "test-case name length is invalid",
            Self::InvalidCharacter => "test-case name contains an unsupported character",
        })
    }
}

impl Error for TestCaseNameError {}

/// Closed semantic outcome reported for one structured test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TestCaseOutcome {
    /// The test executed and passed.
    Passed,
    /// The test executed and failed.
    Failed,
    /// The test was discovered but deliberately did not execute.
    Ignored,
}

/// One structured test-case result with no unbounded failure output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseEvidence {
    name: TestCaseName,
    outcome: TestCaseOutcome,
}

impl TestCaseEvidence {
    /// Creates one adapter-normalized case result.
    #[must_use]
    pub const fn new(name: TestCaseName, outcome: TestCaseOutcome) -> Self {
        Self { name, outcome }
    }

    /// Returns the exact normalized case name.
    #[must_use]
    pub const fn name(&self) -> &TestCaseName {
        &self.name
    }

    /// Returns passed, failed, or ignored.
    #[must_use]
    pub const fn outcome(&self) -> TestCaseOutcome {
        self.outcome
    }
}

/// Structured test semantics bound to an exact command result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEvidence {
    id: TaskEvidenceId,
    command: CommandEvidence,
    cases: Vec<TestCaseEvidence>,
}

impl TestEvidence {
    /// Canonicalizes a bounded report and rejects duplicate test-case names.
    pub fn new(
        command: CommandEvidence,
        mut cases: Vec<TestCaseEvidence>,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        if cases.len() > MAX_TEST_CASE_EVIDENCE {
            return Err(VerificationEvidenceBuildError::TooManyTestCases {
                actual: cases.len(),
            });
        }
        cases.sort_by(|left, right| left.name().cmp(right.name()));
        if cases
            .windows(2)
            .any(|pair| pair[0].name() == pair[1].name())
        {
            return Err(VerificationEvidenceBuildError::DuplicateTestCase);
        }
        let id = derive_test_evidence_id(command.id(), &cases);
        Ok(Self { id, command, cases })
    }

    /// Returns the test evidence identity, distinct from its underlying command evidence.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the exact process evidence from which the report was normalized.
    #[must_use]
    pub const fn command(&self) -> &CommandEvidence {
        &self.command
    }

    /// Returns canonical structured cases.
    #[must_use]
    pub fn cases(&self) -> &[TestCaseEvidence] {
        &self.cases
    }
}

/// Bounded count of diagnostics at one severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagnosticCount(u32);

impl DiagnosticCount {
    /// Constructs an adapter-observed diagnostic count.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the primitive count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Structured diagnostic counts bound to an exact command result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvidence {
    id: TaskEvidenceId,
    command: CommandEvidence,
    errors: DiagnosticCount,
    warnings: DiagnosticCount,
}

impl DiagnosticEvidence {
    /// Binds adapter-normalized counts to the complete command proof.
    #[must_use]
    pub fn new(
        command: CommandEvidence,
        errors: DiagnosticCount,
        warnings: DiagnosticCount,
    ) -> Self {
        let mut hasher = blake3::Hasher::new_derive_key("a3.diagnostic-evidence.v1");
        hasher.update(command.id().as_bytes());
        hasher.update(&errors.get().to_le_bytes());
        hasher.update(&warnings.get().to_le_bytes());
        Self {
            id: TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes()),
            command,
            errors,
            warnings,
        }
    }

    /// Returns the durable diagnostic evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the exact process evidence from which diagnostics were normalized.
    #[must_use]
    pub const fn command(&self) -> &CommandEvidence {
        &self.command
    }

    /// Returns error diagnostics.
    #[must_use]
    pub const fn errors(&self) -> DiagnosticCount {
        self.errors
    }

    /// Returns warning diagnostics.
    #[must_use]
    pub const fn warnings(&self) -> DiagnosticCount {
        self.warnings
    }
}

/// Trusted source from which a complete changed-path set was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffEvidenceSource {
    /// The E3 patch boundary reported the exact transitions it applied.
    Patch {
        /// Digest of the immutable patch action.
        action_digest: PatchActionDigest,
        /// Central policy decision that authorized the mutation.
        policy_decision_id: PolicyDecisionId,
    },
    /// Two complete published indexes were compared deterministically.
    PublishedIndexes {
        /// Older published index used as the comparison base.
        base_index_run_id: IndexRunId,
        /// Newer published index whose file state was observed.
        current_index_run_id: IndexRunId,
    },
}

/// Actual changed-path evidence from a patch or two complete published indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEvidence {
    id: TaskEvidenceId,
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    source: DiffEvidenceSource,
    base_snapshot_id: SnapshotId,
    snapshot_id: SnapshotId,
    changed_paths: Vec<RepositoryPath>,
    complete: bool,
    dependencies: VerificationDependencies,
}

impl DiffEvidence {
    /// Binds the actual patch transitions to their post-patch snapshot and path states.
    pub fn from_change_set(
        verification_run_id: VerificationRunId,
        snapshot_id: SnapshotId,
        dependencies: VerificationDependencies,
        changes: &PatchChangeSet,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        if snapshot_id == changes.base_snapshot_id() {
            return Err(VerificationEvidenceBuildError::SnapshotNotAdvanced);
        }
        let required = changed_path_dependencies(changes.changes());
        if required.iter().any(|dependency| {
            !dependencies
                .as_slice()
                .iter()
                .any(|candidate| candidate == dependency)
        }) {
            return Err(VerificationEvidenceBuildError::MissingChangedPathDependency);
        }
        let changed_paths = changes.changed_paths();
        let mut evidence = Self {
            id: TaskEvidenceId::from_bytes([0; 32]),
            verification_run_id,
            spec_id: changes.verification_spec_id(),
            run_id: changes.run_id(),
            source: DiffEvidenceSource::Patch {
                action_digest: changes.action_digest(),
                policy_decision_id: changes.policy_decision_id(),
            },
            base_snapshot_id: changes.base_snapshot_id(),
            snapshot_id,
            changed_paths,
            complete: changes.complete(),
            dependencies,
        };
        evidence.id = derive_diff_evidence_id(&evidence);
        Ok(evidence)
    }

    /// Compares two complete published file states, including a valid empty `NoChanges` result.
    pub fn from_published_indexes(
        verification_run_id: VerificationRunId,
        spec_id: VerificationSpecId,
        run_id: AgentRunId,
        base: &PublishedIndex,
        current: &PublishedIndex,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        if current.run().sequence() <= base.run().sequence()
            || current.run().id() == base.run().id()
            || current.run().snapshot_id() == base.run().snapshot_id()
        {
            return Err(VerificationEvidenceBuildError::IndexObservationNotAdvanced);
        }
        let base_state = RepositoryFileState::new(base.publication().graph().files().to_vec())
            .map_err(|_| VerificationEvidenceBuildError::InvalidPublishedIndex)?;
        let current_state =
            RepositoryFileState::new(current.publication().graph().files().to_vec())
                .map_err(|_| VerificationEvidenceBuildError::InvalidPublishedIndex)?;
        let delta = SnapshotDelta::between(&base_state, &current_state);
        if delta.files().len() > 128 {
            return Err(VerificationEvidenceBuildError::InvalidChangedPathCount);
        }
        let changed_paths = delta
            .files()
            .iter()
            .map(|change| change.path().clone())
            .collect::<Vec<_>>();
        let dependencies = VerificationDependencies::new(
            delta
                .files()
                .iter()
                .map(|change| match change {
                    FileDelta::Added { current } => EvidenceDependency::Present(current.clone()),
                    FileDelta::Modified {
                        previous,
                        current_hash,
                    } => EvidenceDependency::Present(FileRevision::new(
                        previous.path().clone(),
                        *current_hash,
                    )),
                    FileDelta::Deleted { previous } => {
                        EvidenceDependency::Absent(previous.path().clone())
                    }
                })
                .collect(),
        )
        .map_err(|_| VerificationEvidenceBuildError::InvalidChangedPathCount)?;
        let mut evidence = Self {
            id: TaskEvidenceId::from_bytes([0; 32]),
            verification_run_id,
            spec_id,
            run_id,
            source: DiffEvidenceSource::PublishedIndexes {
                base_index_run_id: base.run().id(),
                current_index_run_id: current.run().id(),
            },
            base_snapshot_id: base.run().snapshot_id(),
            snapshot_id: current.run().snapshot_id(),
            changed_paths,
            complete: true,
            dependencies,
        };
        evidence.id = derive_diff_evidence_id(&evidence);
        Ok(evidence)
    }

    /// Reconstructs persisted diff evidence after canonical path and identity validation.
    pub fn from_stored(
        expected_id: TaskEvidenceId,
        context: StoredDiffEvidenceContext,
        mut changed_paths: Vec<RepositoryPath>,
        dependencies: VerificationDependencies,
    ) -> Result<Self, VerificationEvidenceBuildError> {
        if context.base_snapshot_id == context.snapshot_id {
            return Err(VerificationEvidenceBuildError::SnapshotNotAdvanced);
        }
        if let DiffEvidenceSource::PublishedIndexes {
            base_index_run_id,
            current_index_run_id,
        } = context.source
            && base_index_run_id == current_index_run_id
        {
            return Err(VerificationEvidenceBuildError::IndexObservationNotAdvanced);
        }
        if changed_paths.len() > 128
            || (changed_paths.is_empty()
                && !matches!(context.source, DiffEvidenceSource::PublishedIndexes { .. }))
            || (changed_paths.is_empty() && !dependencies.is_empty())
        {
            return Err(VerificationEvidenceBuildError::InvalidChangedPathCount);
        }
        changed_paths.sort();
        if changed_paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(VerificationEvidenceBuildError::DuplicateChangedPath);
        }
        if changed_paths.iter().any(|path| {
            !dependencies
                .as_slice()
                .iter()
                .any(|dependency| dependency.path() == path)
        }) {
            return Err(VerificationEvidenceBuildError::MissingChangedPathDependency);
        }
        let mut evidence = Self {
            id: expected_id,
            verification_run_id: context.verification_run_id,
            spec_id: context.spec_id,
            run_id: context.run_id,
            source: context.source,
            base_snapshot_id: context.base_snapshot_id,
            snapshot_id: context.snapshot_id,
            changed_paths,
            complete: context.complete,
            dependencies,
        };
        let actual_id = derive_diff_evidence_id(&evidence);
        if actual_id != expected_id {
            return Err(VerificationEvidenceBuildError::EvidenceIdentityMismatch);
        }
        evidence.id = actual_id;
        Ok(evidence)
    }

    /// Returns the durable diff evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the verification attempt.
    #[must_use]
    pub const fn verification_run_id(&self) -> VerificationRunId {
        self.verification_run_id
    }

    /// Returns the exact diff verification specification.
    #[must_use]
    pub const fn spec_id(&self) -> VerificationSpecId {
        self.spec_id
    }

    /// Returns the controlled run that mutated the worktree.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the trusted source of the changed-path set.
    #[must_use]
    pub const fn source(&self) -> DiffEvidenceSource {
        self.source
    }

    /// Returns the snapshot revalidated before mutation.
    #[must_use]
    pub const fn base_snapshot_id(&self) -> SnapshotId {
        self.base_snapshot_id
    }

    /// Returns the observed post-patch snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns canonical actual changed paths.
    #[must_use]
    pub fn changed_paths(&self) -> &[RepositoryPath] {
        &self.changed_paths
    }

    /// Returns whether every authorized operation completed.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Returns post-patch states controlling freshness.
    #[must_use]
    pub const fn dependencies(&self) -> &VerificationDependencies {
        &self.dependencies
    }
}

/// Exact persisted header used to reconstruct DiffEvidence without a live PatchChangeSet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredDiffEvidenceContext {
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    source: DiffEvidenceSource,
    base_snapshot_id: SnapshotId,
    snapshot_id: SnapshotId,
    complete: bool,
}

impl StoredDiffEvidenceContext {
    /// Groups immutable diff anchors loaded from strict persistence columns.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        verification_run_id: VerificationRunId,
        spec_id: VerificationSpecId,
        run_id: AgentRunId,
        source: DiffEvidenceSource,
        base_snapshot_id: SnapshotId,
        snapshot_id: SnapshotId,
        complete: bool,
    ) -> Self {
        Self {
            verification_run_id,
            spec_id,
            run_id,
            source,
            base_snapshot_id,
            snapshot_id,
            complete,
        }
    }
}

/// Explicit user confirmation bound to one exact content-free scope and snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserConfirmationEvidence {
    id: TaskEvidenceId,
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    scope_id: PolicyResourceId,
    confirmed_at: TaskLedgerTimestamp,
}

impl UserConfirmationEvidence {
    /// Records one explicit confirmation after the trusted UI has displayed the exact scope.
    #[must_use]
    pub fn new(
        verification_run_id: VerificationRunId,
        spec_id: VerificationSpecId,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        scope_id: PolicyResourceId,
        confirmed_at: TaskLedgerTimestamp,
    ) -> Self {
        let mut hasher = blake3::Hasher::new_derive_key("a3.user-confirmation-evidence.v1");
        hasher.update(verification_run_id.as_bytes());
        hasher.update(spec_id.as_bytes());
        hasher.update(run_id.as_bytes());
        hasher.update(snapshot_id.as_bytes());
        hasher.update(scope_id.as_bytes());
        hasher.update(&confirmed_at.unix_millis().to_le_bytes());
        Self {
            id: TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes()),
            verification_run_id,
            spec_id,
            run_id,
            snapshot_id,
            scope_id,
            confirmed_at,
        }
    }

    /// Returns the durable confirmation evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        self.id
    }

    /// Returns the verification attempt.
    #[must_use]
    pub const fn verification_run_id(&self) -> VerificationRunId {
        self.verification_run_id
    }

    /// Returns the exact confirmation specification.
    #[must_use]
    pub const fn spec_id(&self) -> VerificationSpecId {
        self.spec_id
    }

    /// Returns the controlled run awaiting confirmation.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact worktree snapshot shown to the user.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the content-free confirmed scope.
    #[must_use]
    pub const fn scope_id(&self) -> PolicyResourceId {
        self.scope_id
    }

    /// Returns when confirmation became durable.
    #[must_use]
    pub const fn confirmed_at(&self) -> TaskLedgerTimestamp {
        self.confirmed_at
    }
}

/// Closed set of operational E6 evidence artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationEvidence {
    /// Generic process success for one exact command.
    Command(CommandEvidence),
    /// Structured test semantics plus the exact underlying process.
    Test(TestEvidence),
    /// Actual patch transitions and their post-patch state.
    Diff(DiffEvidence),
    /// Structured diagnostic counts plus the exact underlying process.
    Diagnostic(DiagnosticEvidence),
    /// Explicit user confirmation of one exact scope.
    UserConfirmation(UserConfirmationEvidence),
}

impl VerificationEvidence {
    /// Returns the durable Task Ledger evidence identity.
    #[must_use]
    pub const fn id(&self) -> TaskEvidenceId {
        match self {
            Self::Command(evidence) => evidence.id(),
            Self::Test(evidence) => evidence.id(),
            Self::Diff(evidence) => evidence.id(),
            Self::Diagnostic(evidence) => evidence.id(),
            Self::UserConfirmation(evidence) => evidence.id(),
        }
    }

    /// Returns the verification specification this artifact can prove.
    #[must_use]
    pub const fn spec_id(&self) -> VerificationSpecId {
        match self {
            Self::Command(evidence) => evidence.spec_id(),
            Self::Test(evidence) => evidence.command().spec_id(),
            Self::Diff(evidence) => evidence.spec_id(),
            Self::Diagnostic(evidence) => evidence.command().spec_id(),
            Self::UserConfirmation(evidence) => evidence.spec_id(),
        }
    }

    /// Returns the controlled agent run that owns the artifact.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        match self {
            Self::Command(evidence) => evidence.run_id(),
            Self::Test(evidence) => evidence.command().run_id(),
            Self::Diff(evidence) => evidence.run_id(),
            Self::Diagnostic(evidence) => evidence.command().run_id(),
            Self::UserConfirmation(evidence) => evidence.run_id(),
        }
    }

    /// Returns the shared verification-run identity.
    #[must_use]
    pub const fn verification_run_id(&self) -> VerificationRunId {
        match self {
            Self::Command(evidence) => evidence.verification_run_id(),
            Self::Test(evidence) => evidence.command().verification_run_id(),
            Self::Diff(evidence) => evidence.verification_run_id(),
            Self::Diagnostic(evidence) => evidence.command().verification_run_id(),
            Self::UserConfirmation(evidence) => evidence.verification_run_id(),
        }
    }

    /// Returns the snapshot against which the evidence was observed.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        match self {
            Self::Command(evidence) => evidence.snapshot_id(),
            Self::Test(evidence) => evidence.command().snapshot_id(),
            Self::Diff(evidence) => evidence.snapshot_id(),
            Self::Diagnostic(evidence) => evidence.command().snapshot_id(),
            Self::UserConfirmation(evidence) => evidence.snapshot_id(),
        }
    }

    /// Returns the exact verification category represented by this artifact.
    #[must_use]
    pub const fn method(&self) -> VerificationMethod {
        match self {
            Self::Command(_) => VerificationMethod::Command,
            Self::Test(_) => VerificationMethod::Test,
            Self::Diff(_) => VerificationMethod::DiffInvariant,
            Self::Diagnostic(_) => VerificationMethod::Diagnostic,
            Self::UserConfirmation(_) => VerificationMethod::UserConfirm,
        }
    }

    fn dependencies(&self) -> Option<&VerificationDependencies> {
        match self {
            Self::Command(evidence) => Some(evidence.dependencies()),
            Self::Test(evidence) => Some(evidence.command().dependencies()),
            Self::Diff(evidence)
                if matches!(
                    evidence.source(),
                    DiffEvidenceSource::PublishedIndexes { .. }
                ) =>
            {
                None
            }
            Self::Diff(evidence) => Some(evidence.dependencies()),
            Self::Diagnostic(evidence) => Some(evidence.command().dependencies()),
            Self::UserConfirmation(_) => None,
        }
    }
}

/// Deterministic freshness result against the latest published repository index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceFreshness {
    /// Every exact dependency still matches, or the snapshot remains unchanged.
    Fresh,
    /// The evidence can no longer support reasoning.
    Stale(EvidenceFreshnessFailure),
}

impl EvidenceFreshness {
    /// Evaluates dependency state without treating a newer unrelated snapshot as stale.
    #[must_use]
    pub fn evaluate(evidence: &VerificationEvidence, current: &PublishedIndex) -> Self {
        let current_graph = current.publication().graph();
        let Some(dependencies) = evidence.dependencies() else {
            return if evidence.snapshot_id() == current_graph.snapshot_id() {
                Self::Fresh
            } else {
                Self::Stale(EvidenceFreshnessFailure::SnapshotChanged)
            };
        };
        if dependencies.is_empty() {
            return if evidence.snapshot_id() == current_graph.snapshot_id() {
                Self::Fresh
            } else {
                Self::Stale(EvidenceFreshnessFailure::SnapshotChanged)
            };
        }
        for dependency in dependencies.as_slice() {
            let current_revision = current_graph
                .files()
                .iter()
                .find(|revision| revision.path() == dependency.path());
            let matches = match dependency {
                EvidenceDependency::Present(expected) => current_revision == Some(expected),
                EvidenceDependency::Absent(_) => current_revision.is_none(),
            };
            if !matches {
                return Self::Stale(EvidenceFreshnessFailure::DependencyChanged);
            }
        }
        Self::Fresh
    }
}

/// Content-free reason verification evidence became stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceFreshnessFailure {
    /// Evidence without granular dependencies belongs to another snapshot.
    SnapshotChanged,
    /// A required path revision or absence no longer holds.
    DependencyChanged,
}

/// Deterministic semantic result of evaluating one exact evidence artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationEvidenceEvaluation {
    /// The operational specification was satisfied.
    Passed,
    /// The artifact could not prove the specification.
    Failed(VerificationEvidenceFailure),
}

impl VerificationEvidenceEvaluation {
    /// Evaluates exact target semantics; freshness is deliberately checked separately.
    #[must_use]
    pub fn evaluate(spec: &VerificationSpec, evidence: &VerificationEvidence) -> Self {
        if !spec.is_operational() {
            return Self::Failed(VerificationEvidenceFailure::LegacySpecification);
        }
        if spec.id() != evidence.spec_id() {
            return Self::Failed(VerificationEvidenceFailure::SpecificationMismatch);
        }
        if spec.method() != evidence.method() {
            return Self::Failed(VerificationEvidenceFailure::EvidenceKindMismatch);
        }
        match (spec.target(), evidence) {
            (
                VerificationTarget::Command { command_id, .. },
                VerificationEvidence::Command(actual),
            ) => evaluate_command(*command_id, actual),
            (
                VerificationTarget::Test {
                    command_id,
                    selector,
                    minimum_cases,
                    ..
                },
                VerificationEvidence::Test(actual),
            ) => evaluate_test(*command_id, selector, minimum_cases.get(), actual),
            (VerificationTarget::DiffInvariant(invariant), VerificationEvidence::Diff(actual)) => {
                evaluate_diff(invariant.mode(), invariant.paths(), actual)
            }
            (
                VerificationTarget::Diagnostic {
                    command_id, policy, ..
                },
                VerificationEvidence::Diagnostic(actual),
            ) => evaluate_diagnostic(*command_id, *policy, actual),
            (
                VerificationTarget::UserConfirm { scope_id },
                VerificationEvidence::UserConfirmation(actual),
            ) if *scope_id == actual.scope_id() => Self::Passed,
            (VerificationTarget::UserConfirm { .. }, VerificationEvidence::UserConfirmation(_)) => {
                Self::Failed(VerificationEvidenceFailure::ConfirmationScopeMismatch)
            }
            _ => Self::Failed(VerificationEvidenceFailure::EvidenceKindMismatch),
        }
    }
}

/// Closed content-free reason an artifact did not prove its exact specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationEvidenceFailure {
    /// Historical H2 method-plus-text specs cannot execute.
    LegacySpecification,
    /// Evidence belongs to another immutable specification.
    SpecificationMismatch,
    /// Evidence category does not match the target method.
    EvidenceKindMismatch,
    /// Process evidence belongs to another discovered command.
    CommandMismatch,
    /// The process timed out, was cancelled, or exited unsuccessfully.
    ProcessUnsuccessful,
    /// No structured case matched the target; exit code alone is insufficient.
    MissingStructuredTestCases,
    /// Fewer executed passing cases than required were present.
    TooFewPassingTestCases,
    /// At least one selected structured test case failed.
    SelectedTestCaseFailed,
    /// A partial patch action cannot satisfy a final diff invariant.
    IncompleteChangeSet,
    /// Actual changed paths did not satisfy the declared relation.
    DiffInvariantMismatch,
    /// Structured error diagnostics violated the policy.
    ErrorDiagnosticsPresent,
    /// Structured warning diagnostics violated the policy.
    WarningDiagnosticsPresent,
    /// Confirmation was granted for another scope.
    ConfirmationScopeMismatch,
}

/// Invalid structured evidence supplied by an adapter or patch boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationEvidenceBuildError {
    /// A structured test report exceeded one million cases.
    TooManyTestCases {
        /// Observed case count.
        actual: usize,
    },
    /// One structured report repeated the same normalized test-case name.
    DuplicateTestCase,
    /// Persisted content-free process stream metadata violated runner invariants.
    InvalidProcessStream,
    /// A post-patch evidence snapshot equalled the pre-patch snapshot.
    SnapshotNotAdvanced,
    /// A published-index comparison did not advance to a distinct newer observation.
    IndexObservationNotAdvanced,
    /// A supposedly complete published index contained an invalid duplicate file state.
    InvalidPublishedIndex,
    /// A changed path lacked an exact post-patch present or absent dependency.
    MissingChangedPathDependency,
    /// Persisted changed-path evidence was empty or exceeded 128 path transitions.
    InvalidChangedPathCount,
    /// Persisted changed-path evidence repeated one path.
    DuplicateChangedPath,
    /// Persisted fields did not derive the claimed immutable evidence identity.
    EvidenceIdentityMismatch,
}

impl fmt::Display for VerificationEvidenceBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyTestCases { .. } => "structured test evidence exceeds its case limit",
            Self::DuplicateTestCase => "structured test evidence repeats a case name",
            Self::InvalidProcessStream => "persisted process stream metadata is invalid",
            Self::SnapshotNotAdvanced => "diff evidence does not use a post-patch snapshot",
            Self::IndexObservationNotAdvanced => {
                "diff evidence does not compare distinct ordered published indexes"
            }
            Self::InvalidPublishedIndex => "diff evidence received an invalid published index",
            Self::MissingChangedPathDependency => {
                "diff evidence does not cover every changed path state"
            }
            Self::InvalidChangedPathCount => "diff evidence changed-path count is invalid",
            Self::DuplicateChangedPath => "diff evidence repeats a changed path",
            Self::EvidenceIdentityMismatch => "verification evidence identity does not match",
        })
    }
}

impl Error for VerificationEvidenceBuildError {}

fn evaluate_command(
    command_id: DiscoveredCommandId,
    evidence: &CommandEvidence,
) -> VerificationEvidenceEvaluation {
    if evidence.command_id() != command_id {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::CommandMismatch,
        );
    }
    if !evidence.process_succeeded() {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::ProcessUnsuccessful,
        );
    }
    VerificationEvidenceEvaluation::Passed
}

fn evaluate_test(
    command_id: DiscoveredCommandId,
    selector: &TestCaseSelector,
    minimum_cases: u32,
    evidence: &TestEvidence,
) -> VerificationEvidenceEvaluation {
    if let VerificationEvidenceEvaluation::Failed(failure) =
        evaluate_command(command_id, evidence.command())
    {
        return VerificationEvidenceEvaluation::Failed(failure);
    }
    let selected = evidence
        .cases()
        .iter()
        .filter(|case| match selector {
            TestCaseSelector::All => true,
            TestCaseSelector::Exact(name) => case.name().as_str() == name.as_str(),
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::MissingStructuredTestCases,
        );
    }
    if selected
        .iter()
        .any(|case| case.outcome() == TestCaseOutcome::Failed)
    {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::SelectedTestCaseFailed,
        );
    }
    let passing = selected
        .iter()
        .filter(|case| case.outcome() == TestCaseOutcome::Passed)
        .count();
    if passing < minimum_cases as usize {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::TooFewPassingTestCases,
        );
    }
    VerificationEvidenceEvaluation::Passed
}

fn evaluate_diff(
    mode: DiffInvariantMode,
    expected: &[RepositoryPath],
    evidence: &DiffEvidence,
) -> VerificationEvidenceEvaluation {
    if !evidence.complete() {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::IncompleteChangeSet,
        );
    }
    let actual = evidence.changed_paths();
    let passed = match mode {
        DiffInvariantMode::NoChanges => actual.is_empty(),
        DiffInvariantMode::OnlyPaths => actual.iter().all(|path| expected.contains(path)),
        DiffInvariantMode::ExactPaths => actual == expected,
    };
    if passed {
        VerificationEvidenceEvaluation::Passed
    } else {
        VerificationEvidenceEvaluation::Failed(VerificationEvidenceFailure::DiffInvariantMismatch)
    }
}

fn evaluate_diagnostic(
    command_id: DiscoveredCommandId,
    policy: DiagnosticPolicy,
    evidence: &DiagnosticEvidence,
) -> VerificationEvidenceEvaluation {
    if let VerificationEvidenceEvaluation::Failed(failure) =
        evaluate_command(command_id, evidence.command())
    {
        return VerificationEvidenceEvaluation::Failed(failure);
    }
    if evidence.errors().get() != 0 {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::ErrorDiagnosticsPresent,
        );
    }
    if policy == DiagnosticPolicy::NoWarnings && evidence.warnings().get() != 0 {
        return VerificationEvidenceEvaluation::Failed(
            VerificationEvidenceFailure::WarningDiagnosticsPresent,
        );
    }
    VerificationEvidenceEvaluation::Passed
}

fn changed_path_dependencies(changes: &[PatchChange]) -> Vec<EvidenceDependency> {
    let mut dependencies = Vec::new();
    for change in changes {
        match change {
            PatchChange::Added(current) | PatchChange::Updated { current, .. } => {
                dependencies.push(EvidenceDependency::Present(current.clone()));
            }
            PatchChange::Moved { previous, current } => {
                dependencies.push(EvidenceDependency::Absent(previous.path().clone()));
                dependencies.push(EvidenceDependency::Present(current.clone()));
            }
            PatchChange::Deleted(previous) => {
                dependencies.push(EvidenceDependency::Absent(previous.path().clone()));
            }
        }
    }
    dependencies.sort_by(|left, right| left.path().cmp(right.path()));
    dependencies
}

fn derive_command_evidence_id(evidence: &CommandEvidence) -> TaskEvidenceId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.command-evidence.v1");
    hasher.update(evidence.verification_run_id.as_bytes());
    hasher.update(evidence.spec_id.as_bytes());
    hasher.update(evidence.run_id.as_bytes());
    hasher.update(evidence.tool_run_id.as_bytes());
    hasher.update(evidence.command_id.as_bytes());
    hasher.update(evidence.snapshot_id.as_bytes());
    hasher.update(evidence.process_specification_id.as_bytes());
    hasher.update(evidence.policy_decision_id.as_bytes());
    update_termination(&mut hasher, evidence.termination);
    hasher.update(&evidence.duration.as_millis().to_le_bytes());
    update_stream(&mut hasher, evidence.stdout);
    update_stream(&mut hasher, evidence.stderr);
    update_dependencies(&mut hasher, &evidence.dependencies);
    TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes())
}

fn derive_test_evidence_id(
    command_evidence_id: TaskEvidenceId,
    cases: &[TestCaseEvidence],
) -> TaskEvidenceId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.test-evidence.v1");
    hasher.update(command_evidence_id.as_bytes());
    hasher.update(&(cases.len() as u64).to_le_bytes());
    for case in cases {
        update_bytes(&mut hasher, case.name().as_str().as_bytes());
        hasher.update(&[match case.outcome() {
            TestCaseOutcome::Passed => 0,
            TestCaseOutcome::Failed => 1,
            TestCaseOutcome::Ignored => 2,
        }]);
    }
    TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes())
}

fn derive_diff_evidence_id(evidence: &DiffEvidence) -> TaskEvidenceId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.diff-evidence.v1");
    hasher.update(evidence.verification_run_id.as_bytes());
    hasher.update(evidence.spec_id.as_bytes());
    hasher.update(evidence.run_id.as_bytes());
    match evidence.source {
        DiffEvidenceSource::Patch {
            action_digest,
            policy_decision_id,
        } => {
            hasher.update(&[0]);
            hasher.update(&action_digest.as_bytes());
            hasher.update(policy_decision_id.as_bytes());
        }
        DiffEvidenceSource::PublishedIndexes {
            base_index_run_id,
            current_index_run_id,
        } => {
            hasher.update(&[1]);
            hasher.update(base_index_run_id.as_bytes());
            hasher.update(current_index_run_id.as_bytes());
        }
    }
    hasher.update(evidence.base_snapshot_id.as_bytes());
    hasher.update(evidence.snapshot_id.as_bytes());
    hasher.update(&[u8::from(evidence.complete)]);
    for path in &evidence.changed_paths {
        update_bytes(&mut hasher, path.as_bytes());
    }
    update_dependencies(&mut hasher, &evidence.dependencies);
    TaskEvidenceId::from_bytes(*hasher.finalize().as_bytes())
}

fn update_termination(hasher: &mut blake3::Hasher, termination: ProcessTermination) {
    match termination {
        ProcessTermination::Exited(exit) => {
            hasher.update(&[0]);
            hasher.update(&exit.code().unwrap_or(i32::MIN).to_le_bytes());
        }
        ProcessTermination::TimedOut => {
            hasher.update(&[1]);
        }
        ProcessTermination::Cancelled => {
            hasher.update(&[2]);
        }
    }
}

fn update_stream(hasher: &mut blake3::Hasher, stream: ProcessStreamEvidence) {
    hasher.update(&stream.digest().as_bytes());
    hasher.update(&stream.observed_bytes().to_le_bytes());
    hasher.update(&stream.retained_limit().to_le_bytes());
    hasher.update(&[u8::from(stream.truncated())]);
    hasher.update(&[match stream.redaction() {
        None => 0,
        Some(ProcessOutputRedaction::InvalidUtf8) => 1,
        Some(ProcessOutputRedaction::SecretCandidate) => 2,
        Some(ProcessOutputRedaction::UnsafeControl) => 3,
    }]);
}

fn update_dependencies(hasher: &mut blake3::Hasher, dependencies: &VerificationDependencies) {
    hasher.update(&(dependencies.as_slice().len() as u64).to_le_bytes());
    for dependency in dependencies.as_slice() {
        match dependency {
            EvidenceDependency::Present(revision) => {
                hasher.update(&[0]);
                update_bytes(hasher, revision.path().as_bytes());
                hasher.update(revision.content_hash().as_bytes());
            }
            EvidenceDependency::Absent(path) => {
                hasher.update(&[1]);
                update_bytes(hasher, path.as_bytes());
            }
        }
    }
}

fn update_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContentHash, DiffInvariantVerification, IndexPublication, IndexRunId, IndexRunRecord,
        IndexRunSequence, IndexRunStatus, LinkedGraph, MinimumTestCaseCount, ModulePolicyVersion,
        ModuleProjection, ModuleSymbolSet, PatchAction, PatchActionSchemaVersion, PatchAdd,
        PatchFileContent, PatchOperation, PatchRationale, ProcessExit, ProcessOutputContent,
        ProcessStream, RankProjection, RankingPolicyVersion, RepositoryCard, TaskStepId,
        TestCaseSelectorName, VerificationRequirement, WorktreeId,
    };

    #[test]
    fn exit_zero_without_structured_cases_cannot_pass_a_test_specification()
    -> Result<(), Box<dyn Error>> {
        let command_id = DiscoveredCommandId::from_bytes([8; 32]);
        let spec = VerificationSpec::test(
            VerificationSpecId::from_bytes([2; 32]),
            requirement()?,
            command_id,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            crate::VerificationScope::Targeted,
        );
        let command = successful_command(spec.id(), command_id, SnapshotId::from_bytes([3; 32]))?;
        let empty = VerificationEvidence::Test(TestEvidence::new(command.clone(), Vec::new())?);
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &empty),
            VerificationEvidenceEvaluation::Failed(
                VerificationEvidenceFailure::MissingStructuredTestCases
            )
        );

        let passed_case = TestCaseEvidence::new(
            TestCaseName::try_from_string("domain::case".to_owned())?,
            TestCaseOutcome::Passed,
        );
        let passed = VerificationEvidence::Test(TestEvidence::new(command, vec![passed_case])?);
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &passed),
            VerificationEvidenceEvaluation::Passed
        );
        Ok(())
    }

    #[test]
    fn exact_test_selection_rejects_ignored_failed_and_missing_cases() -> Result<(), Box<dyn Error>>
    {
        let command_id = DiscoveredCommandId::from_bytes([8; 32]);
        let spec = VerificationSpec::test(
            VerificationSpecId::from_bytes([2; 32]),
            requirement()?,
            command_id,
            TestCaseSelector::Exact(TestCaseSelectorName::try_from_string(
                "selected".to_owned(),
            )?),
            MinimumTestCaseCount::new(1)?,
            crate::VerificationScope::Targeted,
        );
        for (outcome, expected) in [
            (
                TestCaseOutcome::Ignored,
                VerificationEvidenceFailure::TooFewPassingTestCases,
            ),
            (
                TestCaseOutcome::Failed,
                VerificationEvidenceFailure::SelectedTestCaseFailed,
            ),
        ] {
            let command =
                successful_command(spec.id(), command_id, SnapshotId::from_bytes([3; 32]))?;
            let report = VerificationEvidence::Test(TestEvidence::new(
                command,
                vec![TestCaseEvidence::new(
                    TestCaseName::try_from_string("selected".to_owned())?,
                    outcome,
                )],
            )?);
            assert_eq!(
                VerificationEvidenceEvaluation::evaluate(&spec, &report),
                VerificationEvidenceEvaluation::Failed(expected)
            );
        }
        Ok(())
    }

    #[test]
    fn diagnostics_require_both_process_success_and_structured_policy_semantics()
    -> Result<(), Box<dyn Error>> {
        let command_id = DiscoveredCommandId::from_bytes([8; 32]);
        let spec = VerificationSpec::diagnostic(
            VerificationSpecId::from_bytes([2; 32]),
            requirement()?,
            command_id,
            DiagnosticPolicy::NoWarnings,
            crate::VerificationScope::Package,
        );
        let command = successful_command(spec.id(), command_id, SnapshotId::from_bytes([3; 32]))?;
        let warnings = VerificationEvidence::Diagnostic(DiagnosticEvidence::new(
            command,
            DiagnosticCount::new(0),
            DiagnosticCount::new(1),
        ));
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &warnings),
            VerificationEvidenceEvaluation::Failed(
                VerificationEvidenceFailure::WarningDiagnosticsPresent
            )
        );
        Ok(())
    }

    #[test]
    fn diff_evidence_requires_post_patch_state_and_checks_actual_paths()
    -> Result<(), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/new.rs".to_vec())?;
        let content = PatchFileContent::try_from_bytes(b"pub fn new() {}\n".to_vec())?;
        let spec_id = VerificationSpecId::from_bytes([2; 32]);
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([4; 32]),
            SnapshotId::from_bytes([3; 32]),
            TaskStepId::from_bytes([5; 32]),
            spec_id,
            PatchRationale::try_from_string("add exact fixture".to_owned())?,
            vec![PatchOperation::Add(PatchAdd::new(
                path.clone(),
                content.clone(),
            ))],
        )?;
        let current = FileRevision::new(path.clone(), content.content_hash());
        let changes = PatchChangeSet::new(
            &action,
            PolicyDecisionId::from_bytes([6; 32]),
            vec![PatchChange::Added(current.clone())],
        )?;
        let dependencies =
            VerificationDependencies::new(vec![EvidenceDependency::Present(current)])?;
        let evidence = VerificationEvidence::Diff(DiffEvidence::from_change_set(
            VerificationRunId::from_bytes([7; 32]),
            SnapshotId::from_bytes([9; 32]),
            dependencies,
            &changes,
        )?);
        let spec = VerificationSpec::diff_invariant(
            spec_id,
            requirement()?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path])?,
        );
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &evidence),
            VerificationEvidenceEvaluation::Passed
        );
        Ok(())
    }

    #[test]
    fn published_index_diff_can_prove_no_changes_and_requires_exact_current_snapshot()
    -> Result<(), Box<dyn Error>> {
        let base = published_index_run(SnapshotId::from_bytes([31; 32]), Vec::new(), 31, 1)?;
        let current = published_index_run(SnapshotId::from_bytes([32; 32]), Vec::new(), 32, 2)?;
        let spec_id = VerificationSpecId::from_bytes([33; 32]);
        let evidence = VerificationEvidence::Diff(DiffEvidence::from_published_indexes(
            VerificationRunId::from_bytes([34; 32]),
            spec_id,
            AgentRunId::from_bytes([35; 32]),
            &base,
            &current,
        )?);
        let spec = VerificationSpec::diff_invariant(
            spec_id,
            requirement()?,
            DiffInvariantVerification::new(DiffInvariantMode::NoChanges, Vec::new())?,
        );

        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &evidence),
            VerificationEvidenceEvaluation::Passed
        );
        assert_eq!(
            EvidenceFreshness::evaluate(&evidence, &current),
            EvidenceFreshness::Fresh
        );
        let later = published_index_run(SnapshotId::from_bytes([36; 32]), Vec::new(), 36, 3)?;
        assert_eq!(
            EvidenceFreshness::evaluate(&evidence, &later),
            EvidenceFreshness::Stale(EvidenceFreshnessFailure::SnapshotChanged)
        );
        assert_eq!(
            DiffEvidence::from_published_indexes(
                VerificationRunId::from_bytes([34; 32]),
                spec_id,
                AgentRunId::from_bytes([35; 32]),
                &base,
                &base,
            ),
            Err(VerificationEvidenceBuildError::IndexObservationNotAdvanced)
        );
        Ok(())
    }

    #[test]
    fn stored_process_stream_metadata_revalidates_runner_invariants() {
        let digest = ProcessOutputDigest::from_bytes([37; 32]);
        assert_eq!(
            ProcessStreamEvidence::from_stored(digest, 0, 0, false, None),
            Err(VerificationEvidenceBuildError::InvalidProcessStream)
        );
        assert_eq!(
            ProcessStreamEvidence::from_stored(digest, 2_048, 1_024, false, None),
            Err(VerificationEvidenceBuildError::InvalidProcessStream)
        );
        assert!(ProcessStreamEvidence::from_stored(digest, 2_048, 1_024, true, None).is_ok());
    }

    #[test]
    fn freshness_tracks_exact_dependencies_and_conservatively_handles_empty_sets()
    -> Result<(), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?;
        let revision = FileRevision::new(path.clone(), ContentHash::from_bytes([1; 32]));
        let snapshot = SnapshotId::from_bytes([3; 32]);
        let current_snapshot = SnapshotId::from_bytes([4; 32]);
        let command_id = DiscoveredCommandId::from_bytes([8; 32]);
        let spec_id = VerificationSpecId::from_bytes([2; 32]);
        let result = process_result(true)?;
        let command = CommandEvidence::new(
            command_context(spec_id, command_id, snapshot),
            VerificationDependencies::new(vec![EvidenceDependency::Present(revision.clone())])?,
            &result,
        );
        assert_eq!(
            EvidenceFreshness::evaluate(
                &VerificationEvidence::Command(command),
                &published_index(current_snapshot, vec![revision])?
            ),
            EvidenceFreshness::Fresh
        );

        let conservative = successful_command(spec_id, command_id, snapshot)?;
        assert_eq!(
            EvidenceFreshness::evaluate(
                &VerificationEvidence::Command(conservative),
                &published_index(current_snapshot, Vec::new())?
            ),
            EvidenceFreshness::Stale(EvidenceFreshnessFailure::SnapshotChanged)
        );
        Ok(())
    }

    #[test]
    fn user_confirmation_is_exact_scope_and_exact_snapshot_evidence() -> Result<(), Box<dyn Error>>
    {
        let spec_id = VerificationSpecId::from_bytes([2; 32]);
        let expected_scope = PolicyResourceId::from_bytes([3; 32]);
        let spec = VerificationSpec::user_confirm(spec_id, requirement()?, expected_scope);
        let evidence = VerificationEvidence::UserConfirmation(UserConfirmationEvidence::new(
            VerificationRunId::from_bytes([7; 32]),
            spec_id,
            AgentRunId::from_bytes([1; 32]),
            SnapshotId::from_bytes([4; 32]),
            PolicyResourceId::from_bytes([5; 32]),
            TaskLedgerTimestamp::from_unix_millis(10)?,
        ));
        assert_eq!(
            VerificationEvidenceEvaluation::evaluate(&spec, &evidence),
            VerificationEvidenceEvaluation::Failed(
                VerificationEvidenceFailure::ConfirmationScopeMismatch
            )
        );
        Ok(())
    }

    fn requirement() -> Result<VerificationRequirement, Box<dyn Error>> {
        Ok(VerificationRequirement::try_from_string(
            "typed evidence satisfies the target".to_owned(),
        )?)
    }

    fn successful_command(
        spec_id: VerificationSpecId,
        command_id: DiscoveredCommandId,
        snapshot_id: SnapshotId,
    ) -> Result<CommandEvidence, Box<dyn Error>> {
        Ok(CommandEvidence::new(
            command_context(spec_id, command_id, snapshot_id),
            VerificationDependencies::new(Vec::new())?,
            &process_result(true)?,
        ))
    }

    fn command_context(
        spec_id: VerificationSpecId,
        command_id: DiscoveredCommandId,
        snapshot_id: SnapshotId,
    ) -> CommandEvidenceContext {
        CommandEvidenceContext::new(
            VerificationRunId::from_bytes([7; 32]),
            spec_id,
            AgentRunId::from_bytes([1; 32]),
            ToolRunId::from_bytes([6; 32]),
            command_id,
            snapshot_id,
        )
    }

    fn process_result(success: bool) -> Result<ProcessRunResult, Box<dyn Error>> {
        let empty = ProcessOutputContent::text(String::new())?;
        let capture = |stream| {
            ProcessOutputCapture::new(
                stream,
                empty.clone(),
                0,
                1_024,
                false,
                ProcessOutputDigest::from_bytes(*blake3::hash(&[]).as_bytes()),
            )
        };
        Ok(ProcessRunResult::new(
            PolicyResourceId::from_bytes([10; 32]),
            PolicyDecisionId::from_bytes([11; 32]),
            ProcessTermination::Exited(ProcessExit::new(
                Some(if success { 0 } else { 1 }),
                success,
            )?),
            ProcessDuration::from_millis(25),
            capture(ProcessStream::Stdout)?,
            capture(ProcessStream::Stderr)?,
        )?)
    }

    fn published_index(
        snapshot_id: SnapshotId,
        files: Vec<FileRevision>,
    ) -> Result<PublishedIndex, Box<dyn Error>> {
        published_index_run(snapshot_id, files, 12, 1)
    }

    fn published_index_run(
        snapshot_id: SnapshotId,
        files: Vec<FileRevision>,
        index_run_byte: u8,
        sequence: u64,
    ) -> Result<PublishedIndex, Box<dyn Error>> {
        let file_count = u32::try_from(files.len())?;
        let graph = LinkedGraph::new(snapshot_id, files, Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), Vec::new())?;
        let module_policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            snapshot_id,
            module_policy,
            Vec::new(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            file_count,
            0,
        )?;
        let modules =
            ModuleProjection::new(snapshot_id, module_policy, Vec::new(), Vec::new(), card)?;
        let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([index_run_byte; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(sequence)?,
            IndexRunStatus::Published,
        );
        Ok(PublishedIndex::new(run, publication)?)
    }
}
