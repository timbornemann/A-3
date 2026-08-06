use crate::{
    ContextToolResult, ContextToolResultDigest, ContextToolResultPreview, ContextToolResultStatus,
};
use a3_domain::{
    AgentRun, AgentRunError, AgentRunTimestamp, AgentToolEvidenceSet, RunEvent, RunEventCode,
    RunEventId, RunEventKind, RunEventOutcome, RunEventPayload, RunEventRedaction,
    RunEventRedactionSource, RunEventSubject, SnapshotId, ToolRunId,
};
use std::error::Error;
use std::fmt;

/// Complete bounded read result before its definitive journal sequence exists.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentReadResult {
    tool_run_id: ToolRunId,
    status: ContextToolResultStatus,
    preview: ContextToolResultPreview,
    digest: ContextToolResultDigest,
    truncated: bool,
    snapshot_id: SnapshotId,
    evidence: AgentToolEvidenceSet,
    observed_output_bytes: u64,
}

impl AgentReadResult {
    /// Binds one normalized result and its evidence to exactly one immutable read snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tool_run_id: ToolRunId,
        status: ContextToolResultStatus,
        preview: ContextToolResultPreview,
        digest: ContextToolResultDigest,
        truncated: bool,
        snapshot_id: SnapshotId,
        evidence: AgentToolEvidenceSet,
        observed_output_bytes: u64,
    ) -> Result<Self, AgentReadResultError> {
        if evidence.snapshot_id() != snapshot_id {
            return Err(AgentReadResultError::EvidenceSnapshotMismatch);
        }
        Ok(Self {
            tool_run_id,
            status,
            preview,
            digest,
            truncated,
            snapshot_id,
            evidence,
            observed_output_bytes,
        })
    }

    /// Returns the stable owning tool-run identity.
    #[must_use]
    pub const fn tool_run_id(&self) -> ToolRunId {
        self.tool_run_id
    }

    /// Returns the immutable snapshot observed before and after the read.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the normalized result classification.
    #[must_use]
    pub const fn status(&self) -> ContextToolResultStatus {
        self.status
    }

    /// Returns the bounded untrusted preview eligible for the next Context Pack.
    #[must_use]
    pub const fn preview(&self) -> &ContextToolResultPreview {
        &self.preview
    }

    /// Returns the digest of the complete normalized result before preview truncation.
    #[must_use]
    pub const fn digest(&self) -> ContextToolResultDigest {
        self.digest
    }

    /// Returns whether result, evidence, or preview boundaries omitted material.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Returns how many normalized result bytes were observed before journal redaction.
    #[must_use]
    pub const fn observed_output_bytes(&self) -> u64 {
        self.observed_output_bytes
    }

    /// Returns canonical controller-owned source evidence.
    #[must_use]
    pub const fn evidence(&self) -> &AgentToolEvidenceSet {
        &self.evidence
    }

    /// Appends the tool event after its model event and assigns that exact sequence to context.
    pub fn record(
        self,
        run: &mut AgentRun,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
    ) -> Result<RecordedAgentRead, AgentRunError> {
        let (code, outcome) = match self.status {
            ContextToolResultStatus::Succeeded => (RunEventCode::None, RunEventOutcome::Succeeded),
            ContextToolResultStatus::Failed => (RunEventCode::ToolFailure, RunEventOutcome::Failed),
            ContextToolResultStatus::Cancelled => {
                (RunEventCode::Cancellation, RunEventOutcome::Cancelled)
            }
            ContextToolResultStatus::Denied => {
                (RunEventCode::PolicyDecision, RunEventOutcome::Denied)
            }
        };
        let event = run.record(
            event_id,
            RunEventKind::ToolAction,
            RunEventPayload::new(
                code,
                Some(outcome),
                Some(RunEventRedaction::new(
                    RunEventRedactionSource::ToolOutput,
                    self.observed_output_bytes,
                    self.truncated,
                )),
            ),
            self.snapshot_id,
            Some(RunEventSubject::Tool(self.tool_run_id)),
            observed_at,
        )?;
        let context_result = ContextToolResult::new(
            event.sequence(),
            self.tool_run_id,
            self.status,
            self.preview,
            self.digest,
            self.truncated,
            self.snapshot_id,
            self.snapshot_id,
        );
        Ok(RecordedAgentRead {
            event,
            context_result,
            evidence: self.evidence,
        })
    }
}

impl fmt::Debug for AgentReadResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentReadResult")
            .field("tool_run_id", &self.tool_run_id)
            .field("status", &self.status)
            .field("preview_bytes", &self.preview.as_str().len())
            .field("digest", &self.digest)
            .field("truncated", &self.truncated)
            .field("snapshot_id", &self.snapshot_id)
            .field("evidence_count", &self.evidence.evidence().len())
            .field("observed_output_bytes", &self.observed_output_bytes)
            .finish()
    }
}

/// A normalized read result mixed evidence from another snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentReadResultError {
    /// Evidence was not proven current in the result snapshot.
    EvidenceSnapshotMismatch,
}

impl fmt::Display for AgentReadResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agent read evidence belongs to another snapshot")
    }
}

impl Error for AgentReadResultError {}

/// Journaled tool observation plus the two projections needed by context and the Task Ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAgentRead {
    event: RunEvent,
    context_result: ContextToolResult,
    evidence: AgentToolEvidenceSet,
}

impl RecordedAgentRead {
    /// Returns the append-only tool event.
    #[must_use]
    pub const fn event(&self) -> &RunEvent {
        &self.event
    }

    /// Returns the bounded result eligible for the next fresh Context Pack.
    #[must_use]
    pub const fn context_result(&self) -> &ContextToolResult {
        &self.context_result
    }

    /// Returns controller-owned evidence eligible for a later Ledger update.
    #[must_use]
    pub const fn evidence(&self) -> &AgentToolEvidenceSet {
        &self.evidence
    }

    /// Consumes the recording into its durable event, context projection, and evidence.
    #[must_use]
    pub fn into_parts(self) -> (RunEvent, ContextToolResult, AgentToolEvidenceSet) {
        (self.event, self.context_result, self.evidence)
    }
}
