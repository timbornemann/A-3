use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict request for one task-bound diff and verification overview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentInspectionRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl QueryAgentInspectionRequestV1 {
    /// Creates one untrusted task selection without path or evidence authority.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, task_id: String) -> Self {
        Self {
            protocol_version,
            task_id,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the opaque durable task identity.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

/// Safe retained process stream selected for explicit paging.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentInspectionStreamV1 {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// Strict detail request using only identifiers emitted by the current overview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentInspectionLogRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    inspection_revision: String,
    inspection_id: String,
    stream: AgentInspectionStreamV1,
    offset: u32,
    limit: u32,
}

impl QueryAgentInspectionLogRequestV1 {
    /// Creates an untrusted detail selection for boundary validation.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        protocol_version: ProtocolVersion,
        task_id: String,
        inspection_revision: String,
        inspection_id: String,
        stream: AgentInspectionStreamV1,
        offset: u32,
        limit: u32,
    ) -> Self {
        Self {
            protocol_version,
            task_id,
            inspection_revision,
            inspection_id,
            stream,
            offset,
            limit,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the task that emitted the selected record.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the decimal process-local revision emitted by the overview.
    #[must_use]
    pub fn inspection_revision(&self) -> &str {
        &self.inspection_revision
    }

    /// Returns the opaque record identity emitted by the overview.
    #[must_use]
    pub fn inspection_id(&self) -> &str {
        &self.inspection_id
    }

    /// Returns the selected stream.
    #[must_use]
    pub const fn stream(&self) -> AgentInspectionStreamV1 {
        self.stream
    }

    /// Returns the retained UTF-8 byte cursor.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// Returns the requested page size before Core validation.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }
}

/// Exact path bytes plus a separate control-safe display label.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentInspectionPathV1 {
    path_hex: String,
    display_path: String,
}

impl AgentInspectionPathV1 {
    /// Creates a lossless path projection from Core-validated repository bytes.
    #[must_use]
    pub const fn new(path_hex: String, display_path: String) -> Self {
        Self {
            path_hex,
            display_path,
        }
    }
}

/// Exact E3 file operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiffFileOperationV1 {
    /// Add a previously absent path.
    Add,
    /// Replace content at one existing path.
    Update,
    /// Move existing content to another path.
    Move,
    /// Remove existing content.
    Delete,
}

/// Actor provenance asserted only by a trusted Core observation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentChangeAttributionV1 {
    /// Exact action proposed by the Agent before approval.
    ProposedAgent,
    /// Actual E3 change set proves application by the Agent.
    AppliedAgent,
    /// Trusted observation explicitly proves a non-Agent change.
    External,
    /// No reliable actor evidence exists.
    Unattributed,
}

/// Exact line terminator retained separately from line text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiffLineEndingV1 {
    /// Line feed.
    Lf,
    /// Carriage return followed by line feed.
    Crlf,
    /// Bare carriage return.
    Cr,
    /// No terminator followed the retained line.
    None,
}

/// One exact retained line.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiffLineV1 {
    text: String,
    ending: AgentDiffLineEndingV1,
}

impl AgentDiffLineV1 {
    /// Creates a line from an already secret-checked E3 preview.
    #[must_use]
    pub const fn new(text: String, ending: AgentDiffLineEndingV1) -> Self {
        Self { text, ending }
    }
}

/// Shared row used by unified and side-by-side renderers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentDiffRowV1 {
    /// Unchanged context present on both sides.
    Context {
        /// One-based prior line.
        before_line: u32,
        /// One-based proposed line.
        after_line: u32,
        /// Exact retained line.
        line: AgentDiffLineV1,
    },
    /// Prior-side removal.
    Removed {
        /// One-based prior line.
        before_line: u32,
        /// Exact retained line.
        line: AgentDiffLineV1,
    },
    /// Proposed-side addition.
    Added {
        /// One-based proposed line.
        after_line: u32,
        /// Exact retained line.
        line: AgentDiffLineV1,
    },
}

/// One deterministic changed region with conventional coordinates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiffHunkV1 {
    before_start: u32,
    before_count: u32,
    after_start: u32,
    after_count: u32,
    rows: Vec<AgentDiffRowV1>,
}

impl AgentDiffHunkV1 {
    /// Creates a hunk already derived and bounded by the Application Core.
    #[must_use]
    pub const fn new(
        before_start: u32,
        before_count: u32,
        after_start: u32,
        after_count: u32,
        rows: Vec<AgentDiffRowV1>,
    ) -> Self {
        Self {
            before_start,
            before_count,
            after_start,
            after_count,
            rows,
        }
    }
}

/// Exact complete-content metadata for one retained E3 prefix.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiffContentV1 {
    retained_bytes: String,
    total_bytes: String,
    content_hash: String,
    encoding: AgentDiffEncodingV1,
    line_endings: AgentDiffContentLineEndingsV1,
    content_truncated: bool,
}

impl AgentDiffContentV1 {
    /// Creates content metadata without duplicating hunk text.
    #[must_use]
    pub const fn new(
        retained_bytes: String,
        total_bytes: String,
        content_hash: String,
        encoding: AgentDiffEncodingV1,
        line_endings: AgentDiffContentLineEndingsV1,
        content_truncated: bool,
    ) -> Self {
        Self {
            retained_bytes,
            total_bytes,
            content_hash,
            encoding,
            line_endings,
            content_truncated,
        }
    }
}

/// E3 text encoding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiffEncodingV1 {
    /// UTF-8 without a byte-order mark.
    Utf8,
    /// UTF-8 with a byte-order mark.
    Utf8Bom,
}

/// Complete file line-ending classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiffContentLineEndingsV1 {
    /// No line ending exists.
    None,
    /// Uniform LF.
    Lf,
    /// Uniform CRLF.
    Crlf,
    /// Uniform bare CR.
    Cr,
    /// Mixed representations.
    Mixed,
}

/// One exact file-level patch projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentDiffFileV1 {
    operation: AgentDiffFileOperationV1,
    source_path: Option<AgentInspectionPathV1>,
    target_path: Option<AgentInspectionPathV1>,
    before: Option<AgentDiffContentV1>,
    after: Option<AgentDiffContentV1>,
    hunks: Vec<AgentDiffHunkV1>,
    added_lines: u32,
    removed_lines: u32,
    attribution: AgentChangeAttributionV1,
    content_truncated: bool,
}

impl AgentDiffFileV1 {
    /// Creates a fully mapped Core-owned file projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        operation: AgentDiffFileOperationV1,
        source_path: Option<AgentInspectionPathV1>,
        target_path: Option<AgentInspectionPathV1>,
        before: Option<AgentDiffContentV1>,
        after: Option<AgentDiffContentV1>,
        hunks: Vec<AgentDiffHunkV1>,
        added_lines: u32,
        removed_lines: u32,
        attribution: AgentChangeAttributionV1,
        content_truncated: bool,
    ) -> Self {
        Self {
            operation,
            source_path,
            target_path,
            before,
            after,
            hunks,
            added_lines,
            removed_lines,
            attribution,
            content_truncated,
        }
    }
}

/// Current exact pre-approval patch retained by the privileged runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentPatchInspectionV1 {
    inspection_id: String,
    run_id: String,
    step_id: String,
    verification_spec_id: String,
    snapshot_id: String,
    retained_bytes: String,
    files: Vec<AgentDiffFileV1>,
}

impl AgentPatchInspectionV1 {
    /// Creates a task-bound exact patch view.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        inspection_id: String,
        run_id: String,
        step_id: String,
        verification_spec_id: String,
        snapshot_id: String,
        retained_bytes: String,
        files: Vec<AgentDiffFileV1>,
    ) -> Self {
        Self {
            inspection_id,
            run_id,
            step_id,
            verification_spec_id,
            snapshot_id,
            retained_bytes,
            files,
        }
    }
}

/// Process category without inferring verification success from exit alone.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentInspectionProcessKindV1 {
    /// Structured test run.
    Test,
    /// Build command.
    Build,
    /// Structured diagnostic run.
    Diagnostic,
    /// Lint command.
    Lint,
    /// Format command.
    Format,
    /// Generic operational command.
    Command,
}

/// Stable reason why retained process text was withheld.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentProcessRedactionV1 {
    /// Captured bytes were not valid UTF-8.
    InvalidUtf8,
    /// Text contained a possible credential.
    SecretCandidate,
    /// Text contained an unsafe control character.
    UnsafeControl,
}

/// Content-free retained stream metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentProcessStreamV1 {
    digest: String,
    observed_bytes: String,
    retained_bytes: String,
    retained_limit: u32,
    source_truncated: bool,
    redaction: Option<AgentProcessRedactionV1>,
}

impl AgentProcessStreamV1 {
    /// Creates one safe stream summary.
    #[must_use]
    pub const fn new(
        digest: String,
        observed_bytes: String,
        retained_bytes: String,
        retained_limit: u32,
        source_truncated: bool,
        redaction: Option<AgentProcessRedactionV1>,
    ) -> Self {
        Self {
            digest,
            observed_bytes,
            retained_bytes,
            retained_limit,
            source_truncated,
            redaction,
        }
    }
}

/// Portable process termination.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentProcessTerminationV1 {
    /// Direct child exited after its owned process group became empty.
    Exited {
        /// Portable exit code, absent for signal-like termination.
        code: Option<i32>,
        /// Platform success classification.
        success: bool,
    },
    /// The process deadline elapsed.
    TimedOut,
    /// Cooperative cancellation terminated the process group.
    Cancelled,
}

/// One completed volatile process summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentProcessInspectionV1 {
    inspection_id: String,
    run_id: String,
    step_id: String,
    verification_spec_id: String,
    snapshot_id: String,
    kind: AgentInspectionProcessKindV1,
    termination: AgentProcessTerminationV1,
    duration_millis: String,
    stdout: AgentProcessStreamV1,
    stderr: AgentProcessStreamV1,
}

impl AgentProcessInspectionV1 {
    /// Creates one task-bound process row.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        inspection_id: String,
        run_id: String,
        step_id: String,
        verification_spec_id: String,
        snapshot_id: String,
        kind: AgentInspectionProcessKindV1,
        termination: AgentProcessTerminationV1,
        duration_millis: String,
        stdout: AgentProcessStreamV1,
        stderr: AgentProcessStreamV1,
    ) -> Self {
        Self {
            inspection_id,
            run_id,
            step_id,
            verification_spec_id,
            snapshot_id,
            kind,
            termination,
            duration_millis,
            stdout,
            stderr,
        }
    }
}

/// Operational verification method.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentVerificationMethodV1 {
    /// Generic command.
    Command,
    /// Structured tests.
    Test,
    /// Exact diff invariant.
    DiffInvariant,
    /// Structured diagnostics.
    Diagnostic,
    /// Explicit user confirmation.
    UserConfirm,
}

/// Stable semantic failure reason derived by the Core.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentVerificationFailureV1 {
    /// Historical non-operational specification.
    LegacySpecification,
    /// Artifact belongs to another specification.
    SpecificationMismatch,
    /// Artifact category differs from the specification.
    EvidenceKindMismatch,
    /// Artifact belongs to another command.
    CommandMismatch,
    /// Process did not exit successfully.
    ProcessUnsuccessful,
    /// No selected structured tests were reported.
    MissingStructuredTestCases,
    /// Too few selected tests passed.
    TooFewPassingTestCases,
    /// A selected test failed.
    SelectedTestCaseFailed,
    /// Partial patch result cannot prove the invariant.
    IncompleteChangeSet,
    /// Actual changed paths violated the invariant.
    DiffInvariantMismatch,
    /// Structured error diagnostics were present.
    ErrorDiagnosticsPresent,
    /// Structured warning diagnostics were present.
    WarningDiagnosticsPresent,
    /// Confirmation referred to another scope.
    ConfirmationScopeMismatch,
}

/// Fresh semantic evaluation of one durable artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentVerificationEvaluationV1 {
    /// Artifact satisfies its exact specification.
    Passed,
    /// Artifact does not satisfy its exact specification.
    Failed {
        /// Typed content-free failure.
        reason: AgentVerificationFailureV1,
    },
}

/// Freshness failure derived against the latest publication.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentEvidenceStaleReasonV1 {
    /// A snapshot-bound artifact belongs to an older snapshot.
    SnapshotChanged,
    /// A granular present or absent dependency changed.
    DependencyChanged,
}

/// Current freshness of one durable artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentEvidenceFreshnessV1 {
    /// Every relevant dependency still matches.
    Fresh,
    /// The artifact can no longer prove current work.
    Stale {
        /// Typed content-free cause.
        reason: AgentEvidenceStaleReasonV1,
    },
}

/// Durable process stream metadata; text remains volatile-only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationProcessStreamV1 {
    digest: String,
    observed_bytes: String,
    retained_limit: u32,
    source_truncated: bool,
    redaction: Option<AgentProcessRedactionV1>,
}

impl AgentVerificationProcessStreamV1 {
    /// Creates content-free durable stream metadata.
    #[must_use]
    pub const fn new(
        digest: String,
        observed_bytes: String,
        retained_limit: u32,
        source_truncated: bool,
        redaction: Option<AgentProcessRedactionV1>,
    ) -> Self {
        Self {
            digest,
            observed_bytes,
            retained_limit,
            source_truncated,
            redaction,
        }
    }
}

/// Durable command metadata shared by command-like evidence kinds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationCommandV1 {
    command_id: String,
    termination: AgentProcessTerminationV1,
    duration_millis: String,
    stdout: AgentVerificationProcessStreamV1,
    stderr: AgentVerificationProcessStreamV1,
}

impl AgentVerificationCommandV1 {
    /// Creates one content-free durable command projection.
    #[must_use]
    pub const fn new(
        command_id: String,
        termination: AgentProcessTerminationV1,
        duration_millis: String,
        stdout: AgentVerificationProcessStreamV1,
        stderr: AgentVerificationProcessStreamV1,
    ) -> Self {
        Self {
            command_id,
            termination,
            duration_millis,
            stdout,
            stderr,
        }
    }
}

/// Structured test-case outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentTestCaseOutcomeV1 {
    /// Executed and passed.
    Passed,
    /// Executed and failed.
    Failed,
    /// Discovered but deliberately not executed.
    Ignored,
}

/// One bounded structured test case.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentTestCaseV1 {
    name: String,
    outcome: AgentTestCaseOutcomeV1,
}

impl AgentTestCaseV1 {
    /// Creates an adapter-normalized test case.
    #[must_use]
    pub const fn new(name: String, outcome: AgentTestCaseOutcomeV1) -> Self {
        Self { name, outcome }
    }
}

/// Trusted source of actual changed-path evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentDiffEvidenceSourceV1 {
    /// Exact E3 patch change set.
    PatchChangeSet,
    /// Difference between ordered complete published indexes.
    PublishedIndexes,
}

/// Method-specific durable artifact semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentVerificationEvidenceDetailV1 {
    /// Generic command evidence.
    Command {
        /// Content-free command result.
        command: AgentVerificationCommandV1,
    },
    /// Structured test evidence.
    Test {
        /// Content-free command result.
        command: AgentVerificationCommandV1,
        /// Total passed cases.
        passed: String,
        /// Total failed cases.
        failed: String,
        /// Total ignored cases.
        ignored: String,
        /// At most the first one hundred canonical cases.
        cases: Vec<AgentTestCaseV1>,
        /// Additional structured cases were omitted.
        cases_truncated: bool,
    },
    /// Exact actual changed-path evidence.
    Diff {
        /// Trusted evidence source.
        source: AgentDiffEvidenceSourceV1,
        /// Snapshot before the observed change.
        base_snapshot_id: String,
        /// Snapshot after the observed change.
        snapshot_id: String,
        /// Canonical actual changed paths.
        changed_paths: Vec<AgentInspectionPathV1>,
        /// Every authorized operation completed.
        complete: bool,
    },
    /// Structured diagnostic evidence.
    Diagnostic {
        /// Content-free command result.
        command: AgentVerificationCommandV1,
        /// Structured error count.
        errors: u32,
        /// Structured warning count.
        warnings: u32,
    },
    /// Exact user-confirmed policy scope.
    UserConfirmation {
        /// Content-free scope identity.
        scope_id: String,
        /// Durable confirmation time.
        confirmed_at_unix_millis: String,
    },
}

/// One immutable durable artifact with freshly derived semantics and freshness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationEvidenceV1 {
    evidence_id: String,
    run_id: String,
    snapshot_id: String,
    method: AgentVerificationMethodV1,
    evaluation: AgentVerificationEvaluationV1,
    freshness: AgentEvidenceFreshnessV1,
    detail: AgentVerificationEvidenceDetailV1,
}

impl AgentVerificationEvidenceV1 {
    /// Creates one Core-derived durable artifact projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        evidence_id: String,
        run_id: String,
        snapshot_id: String,
        method: AgentVerificationMethodV1,
        evaluation: AgentVerificationEvaluationV1,
        freshness: AgentEvidenceFreshnessV1,
        detail: AgentVerificationEvidenceDetailV1,
    ) -> Self {
        Self {
            evidence_id,
            run_id,
            snapshot_id,
            method,
            evaluation,
            freshness,
            detail,
        }
    }
}

/// Durable verification outcome for one attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentStepVerificationOutcomeV1 {
    /// Ledger recorded a passed verification.
    Passed,
    /// Ledger recorded a safe bounded failure summary.
    Failed {
        /// User-readable safe failure summary.
        summary: String,
    },
}

/// One retained step attempt that reached verification.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationAttemptV1 {
    number: u32,
    outcome: AgentStepVerificationOutcomeV1,
    evidence: Vec<AgentVerificationEvidenceV1>,
}

impl AgentVerificationAttemptV1 {
    /// Creates one chronological attempt projection.
    #[must_use]
    pub const fn new(
        number: u32,
        outcome: AgentStepVerificationOutcomeV1,
        evidence: Vec<AgentVerificationEvidenceV1>,
    ) -> Self {
        Self {
            number,
            outcome,
            evidence,
        }
    }
}

/// Current materialized step status.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentVerificationStepStatusV1 {
    /// Not yet eligible or started.
    Pending,
    /// All prerequisites are complete and execution may start.
    Ready,
    /// Currently owned by a run.
    InProgress,
    /// Waiting for scoped user approval.
    AwaitingApproval,
    /// The active attempt is being checked against its specification.
    Verifying,
    /// Successfully verified.
    Completed,
    /// Failed and requires follow-up.
    Failed,
    /// Explicitly cancelled.
    Cancelled,
    /// Blocked by a typed reason.
    Blocked,
    /// Previously complete evidence is no longer current.
    Stale,
}

/// Why a previously completed step became stale.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum AgentStepStaleCauseV1 {
    /// One or more exact artifacts were invalidated.
    VerificationEvidence {
        /// Invalidated artifact identities.
        evidence_ids: Vec<String>,
    },
    /// A prerequisite step became stale first.
    Dependency {
        /// Stale prerequisite identity.
        step_id: String,
    },
}

/// One active plan step and every retained typed verification attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationStepV1 {
    step_id: String,
    intended_outcome: String,
    status: AgentVerificationStepStatusV1,
    stale_cause: Option<AgentStepStaleCauseV1>,
    verification_spec_id: String,
    method: AgentVerificationMethodV1,
    attempts: Vec<AgentVerificationAttemptV1>,
}

impl AgentVerificationStepV1 {
    /// Creates one active step projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        step_id: String,
        intended_outcome: String,
        status: AgentVerificationStepStatusV1,
        stale_cause: Option<AgentStepStaleCauseV1>,
        verification_spec_id: String,
        method: AgentVerificationMethodV1,
        attempts: Vec<AgentVerificationAttemptV1>,
    ) -> Self {
        Self {
            step_id,
            intended_outcome,
            status,
            stale_cause,
            verification_spec_id,
            method,
            attempts,
        }
    }
}

/// Must/Should classification retained separately from current proof state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentCriterionRequirementV1 {
    /// Gates Done.
    Must,
    /// Desired but non-blocking.
    Should,
}

/// Core-derived current proof state for one criterion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentCriterionProofStateV1 {
    /// Every mapped active step has fresh passing evidence.
    Proven,
    /// Work has not reached successful verification.
    Pending,
    /// A mapped verification failed semantically.
    Failed,
    /// A mapped step or artifact is stale.
    Stale,
    /// No active step maps to the criterion.
    Missing,
}

/// Exact artifacts proving one mapped step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentCriterionProofV1 {
    step_id: String,
    evidence_ids: Vec<String>,
}

impl AgentCriterionProofV1 {
    /// Creates one fresh successful proof mapping.
    #[must_use]
    pub const fn new(step_id: String, evidence_ids: Vec<String>) -> Self {
        Self {
            step_id,
            evidence_ids,
        }
    }
}

/// One Goal Contract criterion with its current proof state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentCriterionInspectionV1 {
    criterion_id: String,
    statement: String,
    requirement: AgentCriterionRequirementV1,
    proof_state: AgentCriterionProofStateV1,
    proofs: Vec<AgentCriterionProofV1>,
}

impl AgentCriterionInspectionV1 {
    /// Creates one Core-derived criterion row.
    #[must_use]
    pub const fn new(
        criterion_id: String,
        statement: String,
        requirement: AgentCriterionRequirementV1,
        proof_state: AgentCriterionProofStateV1,
        proofs: Vec<AgentCriterionProofV1>,
    ) -> Self {
        Self {
            criterion_id,
            statement,
            requirement,
            proof_state,
            proofs,
        }
    }
}

/// Durable verification state re-evaluated against the latest publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentVerificationInspectionV1 {
    goal_revision: u32,
    ledger_revision: u32,
    ledger_store_version: String,
    published_snapshot_id: String,
    criteria: Vec<AgentCriterionInspectionV1>,
    steps: Vec<AgentVerificationStepV1>,
}

impl AgentVerificationInspectionV1 {
    /// Creates one consistent current durable projection.
    #[must_use]
    pub const fn new(
        goal_revision: u32,
        ledger_revision: u32,
        ledger_store_version: String,
        published_snapshot_id: String,
        criteria: Vec<AgentCriterionInspectionV1>,
        steps: Vec<AgentVerificationStepV1>,
    ) -> Self {
        Self {
            goal_revision,
            ledger_revision,
            ledger_store_version,
            published_snapshot_id,
            criteria,
            steps,
        }
    }
}

/// Complete U6 view composed from volatile exact content and durable truth.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentInspectionV1 {
    inspection_revision: Option<String>,
    patch: Option<AgentPatchInspectionV1>,
    processes: Vec<AgentProcessInspectionV1>,
    verification: AgentVerificationInspectionV1,
}

impl AgentInspectionV1 {
    /// Creates one task-bound read model. Revision is absent when no volatile records remain.
    #[must_use]
    pub const fn new(
        inspection_revision: Option<String>,
        patch: Option<AgentPatchInspectionV1>,
        processes: Vec<AgentProcessInspectionV1>,
        verification: AgentVerificationInspectionV1,
    ) -> Self {
        Self {
            inspection_revision,
            patch,
            processes,
            verification,
        }
    }
}

/// U6 overview response envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentInspectionResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentInspectionResultV1,
}

impl AgentInspectionResponseV1 {
    /// Creates a response when no Core-owned project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(AgentInspectionResultV1::NoProject)
    }

    /// Creates a response when the selected task no longer exists.
    #[must_use]
    pub const fn task_not_found() -> Self {
        Self::with_result(AgentInspectionResultV1::TaskNotFound)
    }

    /// Creates a response when the Goal exists but no Ledger does.
    #[must_use]
    pub const fn ledger_unavailable() -> Self {
        Self::with_result(AgentInspectionResultV1::LedgerUnavailable)
    }

    /// Creates a response when Goal and Ledger revisions disagree.
    #[must_use]
    pub const fn goal_revision_mismatch() -> Self {
        Self::with_result(AgentInspectionResultV1::GoalRevisionMismatch)
    }

    /// Creates a response requiring an explicit overview refresh.
    #[must_use]
    pub const fn inspection_changed() -> Self {
        Self::with_result(AgentInspectionResultV1::InspectionChanged)
    }

    /// Creates a response containing a current task-bound projection.
    #[must_use]
    pub fn available(inspection: AgentInspectionV1) -> Self {
        Self::with_result(AgentInspectionResultV1::Available {
            inspection: Box::new(inspection),
        })
    }

    const fn with_result(result: AgentInspectionResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the closed overview availability state.
    #[must_use]
    pub const fn result(&self) -> &AgentInspectionResultV1 {
        &self.result
    }
}

/// Closed overview availability state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentInspectionResultV1 {
    /// No project is active.
    NoProject,
    /// Selected task no longer exists.
    TaskNotFound,
    /// Selected Goal has no Ledger yet.
    LedgerUnavailable,
    /// Goal and Ledger revisions disagree.
    GoalRevisionMismatch,
    /// An anchor changed during the bounded read.
    InspectionChanged,
    /// Current task-bound projection.
    Available {
        /// Exact volatile and durable inspection data.
        inspection: Box<AgentInspectionV1>,
    },
}

/// One explicit safe retained log page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentInspectionLogPageV1 {
    text: String,
    offset: u32,
    next_offset: Option<u32>,
    page_truncated: bool,
    source_truncated: bool,
    redaction: Option<AgentProcessRedactionV1>,
}

impl AgentInspectionLogPageV1 {
    /// Creates one page from an already secret-checked retained stream.
    #[must_use]
    pub const fn new(
        text: String,
        offset: u32,
        next_offset: Option<u32>,
        page_truncated: bool,
        source_truncated: bool,
        redaction: Option<AgentProcessRedactionV1>,
    ) -> Self {
        Self {
            text,
            offset,
            next_offset,
            page_truncated,
            source_truncated,
            redaction,
        }
    }
}

/// Explicit log-page response envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentInspectionLogResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentInspectionLogResultV1,
}

impl AgentInspectionLogResponseV1 {
    /// Creates a response when no Core-owned project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(AgentInspectionLogResultV1::NoProject)
    }

    /// Creates a response when the selected record is no longer available.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self::with_result(AgentInspectionLogResultV1::Unavailable)
    }

    /// Creates a response when a newer inspection revision superseded the selection.
    #[must_use]
    pub const fn inspection_changed() -> Self {
        Self::with_result(AgentInspectionLogResultV1::InspectionChanged)
    }

    /// Creates a response containing one retained page.
    #[must_use]
    pub const fn available(page: AgentInspectionLogPageV1) -> Self {
        Self::with_result(AgentInspectionLogResultV1::Available { page })
    }

    const fn with_result(result: AgentInspectionLogResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the closed log-page availability state.
    #[must_use]
    pub const fn result(&self) -> &AgentInspectionLogResultV1 {
        &self.result
    }
}

/// Closed log-page availability state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum AgentInspectionLogResultV1 {
    /// No project is active.
    NoProject,
    /// Record is absent, foreign to the task, or has an invalid cursor.
    Unavailable,
    /// Overview revision changed and must be refreshed.
    InspectionChanged,
    /// One safe retained page is available.
    Available {
        /// Safe text and separate truncation signals.
        page: AgentInspectionLogPageV1,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        AgentInspectionStreamV1, QueryAgentInspectionLogRequestV1, QueryAgentInspectionRequestV1,
    };
    use crate::ProtocolVersion;

    #[test]
    fn overview_request_rejects_authority_bearing_fields() {
        let value = serde_json::json!({
            "protocolVersion": 1,
            "taskId": "11".repeat(32),
            "pathHex": "7372632f6c69622e7273"
        });

        assert!(serde_json::from_value::<QueryAgentInspectionRequestV1>(value).is_err());
    }

    #[test]
    fn log_request_has_only_task_bound_core_emitted_selection() -> Result<(), serde_json::Error> {
        let request = QueryAgentInspectionLogRequestV1::new(
            ProtocolVersion::CURRENT,
            "11".repeat(32),
            "7".to_owned(),
            "22".repeat(32),
            AgentInspectionStreamV1::Stdout,
            0,
            8_192,
        );
        let value = serde_json::to_value(request)?;

        assert_eq!(
            value.as_object().map(serde_json::Map::len),
            Some(7),
            "no path, run, step, snapshot, command, process, policy, or evidence selector is present"
        );
        Ok(())
    }
}
