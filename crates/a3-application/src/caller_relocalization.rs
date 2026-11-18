//! One durable retry of the direct Caller lookup for the actual active declaration.

use crate::{
    AgentControllerControl, AgentReadAction, AgentReadResult, AgentReadTimeout,
    AgentReadToolFailure, AgentReadTools, BoundEditSource, ContextToolResultStatus,
    EditPhaseAnchor, EditPhaseState, PreparedEditPhase, RecordedAgentRead, SourceReadAdmission,
};
use a3_domain::{
    AgentGraphInspection, AgentInspectAction, AgentInspectTarget, AgentRun, AgentRunError,
    AgentRunTimestamp, AgentToolAttemptStatus, ContentHash, ProjectIdentity, RunEventId, SymbolId,
    SyntaxRelationKind, TaskLens, TraversalDepth, TraversalDirection, TraversalResultLimit,
};

/// A current direct-caller query selected by the Core for the edited declaration.
#[derive(Debug)]
pub struct CallerRelocalization {
    anchor: EditPhaseAnchor,
    callee: SymbolId,
}

impl CallerRelocalization {
    /// Pins a selected callee to the exact existing edit target and current Task Lens.
    pub fn new(
        current: EditPhaseState<'_>,
        phase: &PreparedEditPhase,
        lens: &TaskLens,
        callee: SymbolId,
    ) -> Result<Self, AgentReadToolFailure> {
        let anchor = current.anchor();
        if anchor != phase.anchor() {
            return Err(AgentReadToolFailure::InvalidResult);
        }
        let selected =
            BoundEditSource::for_symbol(current.project(), current.published(), lens, callee)
                .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let declaration = selected.required();
        if !phase.targets_declaration(declaration.revision(), declaration.range()) {
            return Err(AgentReadToolFailure::InvalidResult);
        }
        Ok(Self { anchor, callee })
    }

    /// Returns the exact active journal and snapshot anchor for admission.
    #[must_use]
    pub const fn anchor(&self) -> EditPhaseAnchor {
        self.anchor
    }

    /// Returns the active declaration whose direct caller remains open.
    #[must_use]
    pub const fn callee(&self) -> SymbolId {
        self.callee
    }

    /// Runs the reserved direct incoming Calls inspect. Its result is only metadata.
    pub async fn observe(
        &self,
        project: &ProjectIdentity,
        admission: CallerRelocalizationAdmission,
        tools: &dyn AgentReadTools,
        timeout: AgentReadTimeout,
        control: &dyn AgentControllerControl,
    ) -> Result<ObservedCallerRelocalization, AgentReadToolFailure> {
        let attempt = admission.source.attempt();
        if admission.anchor != self.anchor
            || admission.callee != self.callee
            || project.worktree().id() != self.anchor.worktree()
            || attempt.run_id() != self.anchor.run()
            || attempt.snapshot_id() != self.anchor.snapshot()
            || attempt.status() != AgentToolAttemptStatus::InFlight
        {
            return Err(AgentReadToolFailure::InvalidResult);
        }
        let remaining = admission
            .deadline
            .unix_millis()
            .checked_sub(attempt.started_at().unix_millis())
            .ok_or(AgentReadToolFailure::TimedOut)?;
        let requested = u64::try_from(timeout.duration().as_millis())
            .map_err(|_| AgentReadToolFailure::InvalidResult)?;
        let timeout = AgentReadTimeout::from_millis(requested.min(remaining))
            .map_err(|_| AgentReadToolFailure::TimedOut)?;
        let query = AgentGraphInspection::new(
            self.callee,
            TraversalDirection::Incoming,
            SyntaxRelationKind::Calls,
            TraversalDepth::DIRECT,
            TraversalResultLimit::DEFAULT,
        );
        let action =
            AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::Graph(query)));
        let mut read = tools
            .execute(
                project,
                self.anchor.snapshot(),
                attempt.tool_run_id(),
                &action,
                timeout,
                control,
            )
            .await?;
        if read.tool_run_id() != attempt.tool_run_id()
            || read.snapshot_id() != self.anchor.snapshot()
            || read.status() != ContextToolResultStatus::Succeeded
            || read.take_original_page().is_some()
        {
            return Err(AgentReadToolFailure::InvalidResult);
        }
        let outcome = if read.truncated() {
            CallerRelocalizationOutcome::Incomplete
        } else if read.evidence().evidence().is_empty() {
            CallerRelocalizationOutcome::NoMatch
        } else {
            CallerRelocalizationOutcome::CandidateMetadata
        };
        Ok(ObservedCallerRelocalization {
            read,
            receipt: CallerRelocalizationReceipt::new(admission.source.key(), self.callee, outcome),
        })
    }
}

/// Single-use tool capability charged before entering the existing graph reader.
#[derive(Debug)]
pub struct CallerRelocalizationAdmission {
    source: SourceReadAdmission,
    anchor: EditPhaseAnchor,
    callee: SymbolId,
    deadline: AgentRunTimestamp,
}

impl CallerRelocalizationAdmission {
    /// Issues the capability from the existing committed tool attempt.
    #[must_use]
    pub const fn new(
        source: SourceReadAdmission,
        anchor: EditPhaseAnchor,
        callee: SymbolId,
        deadline: AgentRunTimestamp,
    ) -> Self {
        Self {
            source,
            anchor,
            callee,
            deadline,
        }
    }
}

/// Closed result of the one bounded direct-caller inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerRelocalizationOutcome {
    /// No current direct caller appeared within the bounded scope.
    NoMatch,
    /// Current candidate metadata requires a separate bound original read.
    CandidateMetadata,
    /// A bounded result was incomplete and cannot establish absence.
    Incomplete,
    /// Recovery interrupted the lookup before any recorded result.
    Interrupted,
    /// The existing tool attempt terminated without a successful observation.
    ToolFailed,
}

/// Content-free durable outcome for one charged direct-caller acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallerRelocalizationReceipt {
    key: ContentHash,
    callee: SymbolId,
    outcome: CallerRelocalizationOutcome,
}

impl CallerRelocalizationReceipt {
    /// Reconstructs the observation after validating the exact tool and query receipt.
    #[must_use]
    pub const fn new(
        key: ContentHash,
        callee: SymbolId,
        outcome: CallerRelocalizationOutcome,
    ) -> Self {
        Self {
            key,
            callee,
            outcome,
        }
    }

    /// Returns the exact charged source acquisition identity.
    #[must_use]
    pub const fn key(self) -> ContentHash {
        self.key
    }

    /// Returns the original scoped direct-caller target.
    #[must_use]
    pub const fn callee(self) -> SymbolId {
        self.callee
    }

    /// Returns the bounded result without giving delivery credit.
    #[must_use]
    pub const fn outcome(self) -> CallerRelocalizationOutcome {
        self.outcome
    }
}

/// A real graph-inspect result. The caller commits it through AppendAgentRead.
#[derive(Debug)]
pub struct ObservedCallerRelocalization {
    read: AgentReadResult,
    receipt: CallerRelocalizationReceipt,
}

impl ObservedCallerRelocalization {
    /// Reports what the bounded current graph search returned.
    #[must_use]
    pub const fn receipt(&self) -> CallerRelocalizationReceipt {
        self.receipt
    }

    /// Forms the exact existing journal event and its scoped observation marker.
    pub fn record(
        self,
        run: &mut AgentRun,
        event: RunEventId,
        observed_at: AgentRunTimestamp,
    ) -> Result<RecordedAgentRead, AgentRunError> {
        self.read
            .record(run, event, observed_at)
            .map(|read| read.with_caller_relocalization(self.receipt))
    }
}
