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
    original_page: Option<crate::AgentSourcePage>,
    original_evidence: Option<a3_domain::TaskEvidenceId>,
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
            original_page: None,
            original_evidence: None,
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

    /// Attaches the original page at the reader boundary, without persisting source bytes.
    pub fn with_original_page(
        mut self,
        page: crate::AgentSourcePage,
    ) -> Result<Self, AgentReadResultError> {
        if self.status != ContextToolResultStatus::Succeeded
            || !self.evidence.evidence().contains(&page.evidence())
        {
            return Err(AgentReadResultError::EvidenceSnapshotMismatch);
        }
        self.original_evidence =
            (!page.range().is_empty() && !page.text().is_empty()).then(|| page.evidence().id());
        self.original_page = Some(page);
        Ok(self)
    }

    /// Takes volatile original bytes for shared research admission; previews are not evidence.
    pub fn take_original_page(&mut self) -> Option<crate::AgentSourcePage> {
        self.original_page.take()
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
            replan: None,
            original_evidence: self.original_evidence,
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
    replan: Option<crate::ReplanResearchCheckpoint>,
    original_evidence: Option<a3_domain::TaskEvidenceId>,
}

impl RecordedAgentRead {
    /// Metadata marking an actual reader page, not a search/graph span preview.
    #[must_use]
    pub const fn original_evidence(&self) -> Option<a3_domain::TaskEvidenceId> {
        self.original_evidence
    }
    /// Includes the read's research receipt in the same transaction as its event.
    #[must_use]
    pub fn with_replan(mut self, checkpoint: crate::ReplanResearchCheckpoint) -> Self {
        self.replan = Some(checkpoint);
        self
    }

    /// Returns metadata only; original pages never reach persistence.
    #[must_use]
    pub const fn replan(&self) -> Option<&crate::ReplanResearchCheckpoint> {
        self.replan.as_ref()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        AgentFileStartLine, ContentHash, FileRevision, RepositoryPath, SourcePosition, SourceRange,
    };

    #[test]
    fn only_nonempty_successful_originals_keep_a_hydration_marker_after_take()
    -> Result<(), Box<dyn std::error::Error>> {
        for empty in [false, true] {
            let snapshot = SnapshotId::from_bytes([1; 32]);
            let page = crate::AgentSourcePage::new(
                FileRevision::new(
                    RepositoryPath::try_from_bytes(b"module.py".to_vec())?,
                    ContentHash::from_bytes([2; 32]),
                ),
                SourceRange::new(
                    0,
                    if empty { 0 } else { 4 },
                    SourcePosition::new(0, 0),
                    SourcePosition::new(0, if empty { 0 } else { 4 }),
                )?,
                AgentFileStartLine::new(1)?,
                if empty {
                    String::new()
                } else {
                    "pass".to_owned()
                },
                None,
                false,
            )?;
            let result = |status| {
                AgentReadResult::new(
                    ToolRunId::from_bytes([3; 32]),
                    status,
                    ContextToolResultPreview::try_from_string("bounded result".to_owned())?,
                    ContextToolResultDigest::from_bytes([4; 32]),
                    false,
                    snapshot,
                    AgentToolEvidenceSet::new(snapshot, vec![page.evidence()])?,
                    4,
                )
                .map_err(Box::<dyn std::error::Error>::from)
            };
            for status in [
                ContextToolResultStatus::Failed,
                ContextToolResultStatus::Denied,
                ContextToolResultStatus::Cancelled,
            ] {
                assert!(result(status)?.with_original_page(page.clone()).is_err());
            }
            let mut success =
                result(ContextToolResultStatus::Succeeded)?.with_original_page(page.clone())?;
            assert_eq!(success.take_original_page(), Some(page.clone()));
            assert_eq!(
                success.original_evidence,
                (!empty).then(|| page.evidence().id())
            );
        }
        Ok(())
    }
}
