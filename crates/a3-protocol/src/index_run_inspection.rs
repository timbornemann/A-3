use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Strict pathless request for the retained Fast-Index run summaries.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryIndexRunInspectionRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryIndexRunInspectionRequestV1 {
    /// Creates a request for the current protocol version.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
        }
    }
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Strict request for one opaque-cursor file page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryIndexRunFilesRequestV1 {
    protocol_version: ProtocolVersion,
    run_ref: String,
    revision: String,
    search: Option<String>,
    filter: IndexRunFileFilterV1,
    cursor: Option<String>,
}

impl QueryIndexRunFilesRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque retained-run reference.
    #[must_use]
    pub fn run_ref(&self) -> &str {
        &self.run_ref
    }
    /// Returns the revision observed by the WebView.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }
    /// Returns the optional bounded path search.
    #[must_use]
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }
    /// Returns the closed result filter.
    #[must_use]
    pub const fn filter(&self) -> IndexRunFileFilterV1 {
        self.filter
    }
    /// Returns the opaque page cursor, if any.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

/// Strict revision-bound cancel or retry request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlIndexRunRequestV1 {
    protocol_version: ProtocolVersion,
    run_ref: String,
    revision: String,
    action: IndexRunControlActionV1,
}

impl ControlIndexRunRequestV1 {
    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    /// Returns the opaque retained-run reference.
    #[must_use]
    pub fn run_ref(&self) -> &str {
        &self.run_ref
    }
    /// Returns the revision observed by the WebView.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }
    /// Returns the requested control action.
    #[must_use]
    pub const fn action(&self) -> IndexRunControlActionV1 {
        self.action
    }
}

/// Closed revision-bound actions available to the inspector.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunControlActionV1 {
    /// Request cooperative cancellation of the active run.
    Cancel,
    /// Queue a complete rescan after a terminal run.
    Retry,
}

/// Server-side file-result filter vocabulary.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunFileFilterV1 {
    /// Every file row.
    #[default]
    All,
    /// Newly discovered rows.
    New,
    /// Changed rows.
    Changed,
    /// Unchanged rows.
    Unchanged,
    /// Deleted rows.
    Deleted,
    /// Freshly hashed rows.
    Hashed,
    /// Rows with reused hashes.
    HashReused,
    /// Structurally parsed rows.
    Structural,
    /// Rows with reused structural results.
    ParseReused,
    /// Generically indexed rows.
    Generic,
    /// Rows carrying a safe failure.
    Failed,
}

/// Retained summary response; detailed polling is isolated from the global status read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunInspectionResponseV1 {
    protocol_version: ProtocolVersion,
    result: IndexRunInspectionResultV1,
}

impl IndexRunInspectionResponseV1 {
    /// Creates the no-active-project response.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunInspectionResultV1::NoProject,
        }
    }
    /// Creates the response for a project without retained traces.
    #[must_use]
    pub const fn no_runs() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunInspectionResultV1::NoRuns,
        }
    }
    /// Creates an available response with the fixed 60-second stall threshold.
    #[must_use]
    pub fn available(
        current: IndexRunDetailV1,
        previous: Option<IndexRunDetailV1>,
        server_time_unix_millis: String,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunInspectionResultV1::Available {
                current: Box::new(current),
                previous: previous.map(Box::new),
                server_time_unix_millis,
                stall_threshold_seconds: 60,
            },
        }
    }
}

/// Tagged retained-summary result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum IndexRunInspectionResultV1 {
    /// No local project is active.
    NoProject,
    /// The active project has no retained trace.
    NoRuns,
    /// One current and optionally one previous trace are available.
    Available {
        /// Active trace or most recent terminal trace.
        current: Box<IndexRunDetailV1>,
        /// Immediately previous terminal trace while the current trace is active.
        previous: Option<Box<IndexRunDetailV1>>,
        /// Server wall clock used for stall calculation.
        server_time_unix_millis: String,
        /// Seconds without activity before the UI warns.
        stall_threshold_seconds: u32,
    },
}

/// Complete bounded summary projection for one retained trace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunDetailV1 {
    /// Opaque run reference valid only for the active project.
    pub run_ref: String,
    /// Canonical decimal monotone revision.
    pub revision: String,
    /// Current lifecycle state.
    pub state: IndexRunStateV1,
    /// Cause that started the run.
    pub trigger: IndexRunTriggerV1,
    /// Unix epoch milliseconds captured before discovery.
    pub started_at_unix_millis: String,
    /// Terminal Unix epoch milliseconds, if terminal.
    pub ended_at_unix_millis: Option<String>,
    /// Unix epoch milliseconds of the latest progress.
    pub last_activity_at_unix_millis: String,
    /// Duration through completion or the current server time.
    pub duration_millis: String,
    /// Active or last phase.
    pub current_phase: Option<IndexRunPhaseV1>,
    /// Safe bounded current file display.
    pub current_file: Option<IndexRunPathV1>,
    /// Whether some optional journal details could not be persisted.
    pub details_incomplete: bool,
    /// Whether the prior published index remains usable.
    pub previous_publication_available: bool,
    /// Exact counters derived from all retained file rows.
    pub counts: IndexRunCountsV1,
    /// Exactly six ordered phase rows.
    pub phases: Vec<IndexRunPhaseProgressV1>,
    /// Bounded most-recent safe events.
    pub events: Vec<IndexRunEventV1>,
    /// Safe terminal failure explanation and recovery.
    pub failure: Option<IndexRunFailureV1>,
}

/// Lifecycle states exposed to the WebView.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunStateV1 {
    /// Registered but not yet executing.
    Queued,
    /// Executing normally.
    Running,
    /// Cooperative cancellation has been requested.
    Cancelling,
    /// Published successfully.
    Succeeded,
    /// Terminated with a classified failure.
    Failed,
    /// Honored cooperative cancellation.
    Cancelled,
    /// Reconciled after a prior process ended unexpectedly.
    Interrupted,
}

/// Closed causes that may start an index run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunTriggerV1 {
    /// Initial project observation.
    InitialObservation,
    /// Accepted filesystem changes.
    FileChanges,
    /// Recovery after watcher degradation.
    RecoveryRescan,
    /// Explicit full retry from the inspector.
    ManualRetry,
}

/// Fixed six-phase Fast-Index pipeline.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunPhaseV1 {
    /// Discover allowed repository files.
    Discover,
    /// Hash current file bytes or reuse exact hashes.
    Hash,
    /// Parse or reuse file structure.
    Parse,
    /// Link structural relationships.
    Link,
    /// Rank deterministic signals.
    Rank,
    /// Atomically publish the new snapshot.
    Publish,
}

/// Lifecycle state of one fixed phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunPhaseStateV1 {
    /// Phase has not started.
    Pending,
    /// Phase is executing.
    Running,
    /// Phase completed successfully.
    Succeeded,
    /// Phase ended with a failure.
    Failed,
    /// Phase ended due to cancellation or interruption.
    Cancelled,
}

/// Timing and optional sub-progress for one fixed phase.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunPhaseProgressV1 {
    /// Fixed phase identifier.
    pub phase: IndexRunPhaseV1,
    /// Phase lifecycle state.
    pub state: IndexRunPhaseStateV1,
    /// Unix epoch milliseconds when the phase started.
    pub started_at_unix_millis: Option<String>,
    /// Unix epoch milliseconds when the phase ended.
    pub ended_at_unix_millis: Option<String>,
    /// Completed sub-operations when determinate.
    pub completed: Option<String>,
    /// Total sub-operations when determinate.
    pub total: Option<String>,
}

/// Exact decimal counters for the complete retained file set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunCountsV1 {
    /// Allowed files observed by discovery.
    pub discovered: String,
    /// Discovered files whose comparison is still pending.
    pub pending: String,
    /// Newly discovered files.
    pub new_files: String,
    /// Changed files.
    pub changed: String,
    /// Unchanged files.
    pub unchanged: String,
    /// Deleted files.
    pub deleted: String,
    /// Freshly hashed files.
    pub hashed: String,
    /// Files with a reused hash.
    pub hash_reused: String,
    /// Structurally parsed files.
    pub structural: String,
    /// Files with a reused parse result.
    pub parse_reused: String,
    /// Generically indexed files.
    pub generic: String,
    /// Files carrying at least one safe failure.
    pub failed: String,
}

/// One bounded safe event projected to the WebView.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunEventV1 {
    /// Revision that introduced the event.
    pub revision: String,
    /// Unix epoch milliseconds when the event occurred.
    pub occurred_at_unix_millis: String,
    /// Closed event kind.
    pub kind: IndexRunEventKindV1,
    /// Associated phase, if any.
    pub phase: Option<IndexRunPhaseV1>,
    /// Safe bounded repository-relative display, if any.
    pub file: Option<IndexRunPathV1>,
    /// Safe localized diagnostic, if any.
    pub failure: Option<IndexRunFailureV1>,
}

/// Closed event vocabulary for retained trace activity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunEventKindV1 {
    /// Trace registered.
    Queued,
    /// Scheduler began execution.
    Started,
    /// Phase entered.
    PhaseStarted,
    /// Work advanced.
    Progress,
    /// File processed.
    FileObserved,
    /// Safe diagnostic recorded.
    Diagnostic,
    /// Cancellation requested.
    CancellationRequested,
    /// Run succeeded.
    Succeeded,
    /// Run failed.
    Failed,
    /// Run cancelled.
    Cancelled,
    /// Run reconciled as interrupted.
    Interrupted,
}

/// Stable failure codes that never contain adapter error text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunFailureCodeV1 {
    /// Repository discovery failed.
    Discovery,
    /// An allowed source file could not be read.
    SourceUnavailable,
    /// Repository contents changed during the snapshot attempt.
    RevisionChanged,
    /// Structural parsing failed.
    Parse,
    /// Relationship linking failed.
    Link,
    /// Ranking failed.
    Rank,
    /// Atomic publication failed.
    Publish,
    /// A configured resource bound was reached.
    ResourceLimit,
    /// A bounded operation timed out.
    Timeout,
    /// Scheduler progress reporting became unavailable.
    ProgressUnavailable,
    /// Optional journal persistence lost some detail.
    JournalIncomplete,
    /// The owned worker could not execute or report completion.
    WorkerUnavailable,
    /// The prior process ended before a terminal result was recorded.
    Interrupted,
}

/// Localized safe explanation and concrete recovery guidance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunFailureV1 {
    /// Stable machine-readable failure code.
    pub code: IndexRunFailureCodeV1,
    /// Bounded explanation without raw adapter text.
    pub explanation: String,
    /// Bounded recommended recovery action.
    pub recovery: String,
}

/// Bounded, control-character-free repository-relative display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunPathV1 {
    /// Safe repository-relative display value.
    pub display: String,
    /// Whether the original display exceeded the bound.
    pub truncated: bool,
}

/// Response for one server-side searched and cursor-paginated file page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunFilesResponseV1 {
    protocol_version: ProtocolVersion,
    /// Tagged page result.
    pub result: IndexRunFilesResultV1,
}

impl IndexRunFilesResponseV1 {
    /// Creates the no-active-project result.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunFilesResultV1::NoProject,
        }
    }
    /// Creates the result for a trace that is no longer retained.
    #[must_use]
    pub const fn not_found() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunFilesResultV1::NotFound,
        }
    }
    /// Creates the result for an outdated trace revision.
    #[must_use]
    pub const fn stale() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunFilesResultV1::StaleRevision,
        }
    }
    /// Creates one validated page response projection.
    #[must_use]
    pub fn page(
        run_ref: String,
        revision: String,
        files: Vec<IndexRunFileV1>,
        total: String,
        previous_cursor: Option<String>,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: IndexRunFilesResultV1::Page {
                run_ref,
                revision,
                files,
                total,
                previous_cursor,
                next_cursor,
            },
        }
    }
}

/// Tagged result of a file-page request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum IndexRunFilesResultV1 {
    /// No local project is active.
    NoProject,
    /// The referenced run is not retained for the active project.
    NotFound,
    /// The supplied revision is no longer current.
    StaleRevision,
    /// One file page and navigation cursors.
    Page {
        /// Opaque retained-run reference echoed from the request.
        run_ref: String,
        /// Exact revision represented by the page.
        revision: String,
        /// At most 100 safe file rows.
        files: Vec<IndexRunFileV1>,
        /// Exact decimal count of all matching rows.
        total: String,
        /// Opaque previous-page cursor.
        previous_cursor: Option<String>,
        /// Opaque next-page cursor.
        next_cursor: Option<String>,
    },
}

/// Complete safe result for one considered repository path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunFileV1 {
    /// Safe repository-relative path display.
    pub path: IndexRunPathV1,
    /// Change relative to the prior snapshot.
    pub change: IndexRunFileChangeV1,
    /// Hash computation result.
    pub hash: IndexRunHashOutcomeV1,
    /// Parse result once applicable.
    pub parse: Option<IndexRunParseOutcomeV1>,
    /// Bounded safe diagnostics for the file.
    pub failures: Vec<IndexRunFailureV1>,
    /// Whether additional file diagnostics were discarded.
    pub failures_truncated: bool,
}

/// File change classification exposed by V1.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunFileChangeV1 {
    /// Discovery accepted the file but classification is pending.
    Pending,
    /// File is new.
    New,
    /// File content changed.
    Changed,
    /// File content is unchanged.
    Unchanged,
    /// File was removed.
    Deleted,
}
/// Hash work outcome exposed by V1.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunHashOutcomeV1 {
    /// Hashing has not completed yet.
    Pending,
    /// Bytes were freshly hashed.
    Hashed,
    /// An exact prior hash was reused.
    Reused,
    /// Hashing does not apply to a deleted row.
    NotApplicable,
}
/// Parser work outcome exposed by V1.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexRunParseOutcomeV1 {
    /// A language-aware structural parser ran.
    Structural,
    /// A prior structural result was reused.
    Reused,
    /// The generic text indexer handled the file.
    Generic,
    /// Parsing failed safely.
    Failed,
    /// Parsing does not apply to a deleted row.
    NotApplicable,
}

/// Response to a revision-bound cancel or retry request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IndexRunControlResponseV1 {
    protocol_version: ProtocolVersion,
    /// Tagged control result.
    pub result: IndexRunControlResultV1,
}

impl IndexRunControlResponseV1 {
    /// Wraps a control result in the current protocol version.
    #[must_use]
    pub const fn new(result: IndexRunControlResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }
}

/// Tagged result of a revision-bound control request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum IndexRunControlResultV1 {
    /// The action was accepted.
    Accepted,
    /// No local project is active.
    NoProject,
    /// The run is no longer retained for the active project.
    NotFound,
    /// The supplied revision is outdated.
    StaleRevision,
    /// The action is not legal for the run state.
    InvalidState,
    /// Another index action is already queued or active.
    Busy,
    /// The scheduler could not accept the action.
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::{
        IndexRunFilesResponseV1, QueryIndexRunFilesRequestV1, QueryIndexRunInspectionRequestV1,
    };

    #[test]
    fn strict_requests_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<QueryIndexRunInspectionRequestV1>(
                serde_json::json!({"protocolVersion":1,"extra":true})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<QueryIndexRunFilesRequestV1>(serde_json::json!({"protocolVersion":1,"runRef":"00","revision":"1","filter":"all","cursor":null,"search":null,"extra":true})).is_err());
    }

    #[test]
    fn file_page_struct_variant_uses_the_exact_camel_case_wire_fields()
    -> Result<(), serde_json::Error> {
        let response = IndexRunFilesResponseV1::page(
            "ab".repeat(32),
            "7".to_owned(),
            Vec::new(),
            "0".to_owned(),
            Some("previous".to_owned()),
            Some("next".to_owned()),
        );

        assert_eq!(
            serde_json::to_value(response)?,
            serde_json::json!({
                "protocolVersion": 1,
                "result": {
                    "status": "page",
                    "runRef": "ab".repeat(32),
                    "revision": "7",
                    "files": [],
                    "total": "0",
                    "previousCursor": "previous",
                    "nextCursor": "next"
                }
            })
        );
        Ok(())
    }
}
