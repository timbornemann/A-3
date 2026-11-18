//! Owned bounded provider call; no decoded result escapes before durable settlement.
use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_projected_model_events};
use crate::model_wire::{bounded_phase_timeout, phase_elapsed_timestamp};
use crate::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, EditPhaseCallKind,
    EditPhaseObservation, EditPhaseObservationOutcome, EditPhaseReservationFailure,
    EditPhaseWireCost, HttpCorrectionPlanFailure, HttpCorrectionPlanProposal,
    HttpCorrectionPlanStore, ModelFinishReason, ModelOperationControl, ModelProvider,
    ModelProviderFailure, ModelProviderUsage, ModelRequestTimeout, ModelWireProjection,
    PhaseWireBudget, PhaseWireBudgetError, PreparedHttpCorrectionPlan,
};
use a3_domain::{AgentRunTimestamp, ProjectIdentity};
use std::{fmt, time::Instant};

/// Executes only the current-run HTTP source-selection job.
pub struct ExecuteHttpCorrectionPlan<'a> {
    provider: &'a dyn ModelProvider,
    source: &'a dyn AgentSourceReader,
    store: &'a dyn HttpCorrectionPlanStore,
}
impl<'a> ExecuteHttpCorrectionPlan<'a> {
    /// Uses the existing provider/Reader and concrete durable current-run reservation boundary.
    pub const fn new(
        provider: &'a dyn ModelProvider,
        source: &'a dyn AgentSourceReader,
        store: &'a dyn HttpCorrectionPlanStore,
    ) -> Self {
        Self {
            provider,
            source,
            store,
        }
    }
    /// Returns only a fully settled bounded proposal; never updates a Ledger or dispatches a tool.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        phase: &PreparedHttpCorrectionPlan,
        kind: EditPhaseCallKind,
        now: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &C,
    ) -> Result<HttpCorrectionPlanExecutionReport, HttpCorrectionPlanExecutionFailure>
    where
        C: ModelOperationControl + AgentSourceReadControl,
    {
        use HttpCorrectionPlanExecutionFailure as E;
        let started = Instant::now();
        if phase.anchor().worktree != project.worktree().id()
            || phase.request().profile().provider_id() != self.provider.provider_id()
        {
            return Err(E::Binding);
        }
        if ModelOperationControl::is_cancelled(control) {
            return Err(E::Provider(ModelProviderFailure::Cancelled));
        }
        let request = phase.request_for(kind).map_err(E::Preparation)?;
        let projection = self
            .provider
            .project_wire_request(&request)
            .map_err(E::Provider)?;
        let budget = PhaseWireBudget::fit(projection, request.profile()).map_err(E::Wire)?;
        let wire = EditPhaseWireCost::new(
            projection.digest(),
            projection.request_bytes(),
            budget.prompt_tokens(),
            budget.output_tokens(),
        )
        .map_err(E::Reservation)?;
        phase
            .revalidate_source(project, self.source, control)
            .await
            .map_err(E::Source)?;
        let at = || phase_elapsed_timestamp(started, now).ok_or(E::Binding);
        let reserved_at = at()?;
        let reservation = self
            .store
            .reserve_http_correction_plan(project, phase, kind, wire, reserved_at)
            .await
            .map_err(E::Reservation)?;
        if reservation.job != phase.job_digest()
            || reservation.kind != kind
            || reservation.wire != wire
            || reservation.reserved_at != reserved_at
        {
            return Err(E::Binding);
        }
        let received = match bounded_phase_timeout(timeout, reservation.deadline, at()?) {
            Some(remaining) if !ModelOperationControl::is_cancelled(control) => {
                collect_projected_model_events(
                    self.provider,
                    &request,
                    projection,
                    remaining,
                    control,
                    1024,
                )
                .await
            }
            Some(_) => Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::Cancelled,
            )),
            None => Err(ModelStreamCollectionFailure::Provider(
                ModelProviderFailure::TimedOut,
            )),
        };
        let mut proposal = None;
        let (mut outcome, usage, mut failure) = match received {
            Ok(response) => {
                let (text, completion) = response.into_parts();
                let usage = completion.usage();
                if !budget.accepts_usage(projection, usage) {
                    (EditPhaseObservationOutcome::Unknown, usage, Some(E::Usage))
                } else if completion.reason() != ModelFinishReason::Stop {
                    (
                        if completion.reason() == ModelFinishReason::OutputLimit {
                            EditPhaseObservationOutcome::InvalidStructure
                        } else {
                            EditPhaseObservationOutcome::RejectedReply
                        },
                        usage,
                        Some(E::InvalidReply),
                    )
                } else {
                    match phase.decode_proposal(text.as_bytes()) {
                        Ok(value) => {
                            proposal = Some(value);
                            (EditPhaseObservationOutcome::AcceptedReply, usage, None)
                        }
                        Err(HttpCorrectionPlanFailure::UnsafeContent) => (
                            EditPhaseObservationOutcome::RejectedReply,
                            usage,
                            Some(E::UnsafeReply),
                        ),
                        Err(_) => (
                            EditPhaseObservationOutcome::InvalidStructure,
                            usage,
                            Some(E::InvalidReply),
                        ),
                    }
                }
            }
            Err(error) => {
                let (outcome, failure) = match error {
                    ModelStreamCollectionFailure::Provider(ModelProviderFailure::Cancelled) => (
                        EditPhaseObservationOutcome::Cancelled,
                        E::Provider(ModelProviderFailure::Cancelled),
                    ),
                    ModelStreamCollectionFailure::Provider(value) => {
                        (EditPhaseObservationOutcome::Unknown, E::Provider(value))
                    }
                    ModelStreamCollectionFailure::OutputTooLarge(_) => {
                        (EditPhaseObservationOutcome::Unknown, E::InvalidReply)
                    }
                };
                (outcome, ModelProviderUsage::new(None, None), Some(failure))
            }
        };
        if ModelOperationControl::is_cancelled(control) {
            proposal = None;
            outcome = EditPhaseObservationOutcome::Cancelled;
            failure = Some(E::Provider(ModelProviderFailure::Cancelled));
        }
        if proposal.is_some()
            && let Err(error) = phase.revalidate_source(project, self.source, control).await
        {
            proposal = None;
            outcome = EditPhaseObservationOutcome::RejectedReply;
            failure = Some(E::Source(error));
        }
        let observation = EditPhaseObservation::new(
            outcome,
            usage.prompt_tokens().and_then(|v| u32::try_from(v).ok()),
            usage.output_tokens().and_then(|v| u32::try_from(v).ok()),
            at()?,
        );
        let stored = self
            .store
            .finish_http_correction_plan(project, phase, reservation, observation, proposal.clone())
            .await
            .map_err(E::Reservation)?;
        if let Some(error) = failure {
            return Err(error);
        }
        if stored != proposal {
            return Err(E::Binding);
        }
        let proposal = stored.ok_or(E::Binding)?;
        Ok(HttpCorrectionPlanExecutionReport {
            proposal,
            projection,
            budget,
            usage,
        })
    }
}
/// Fully settled non-executable proposal and measured actual provider usage.
pub struct HttpCorrectionPlanExecutionReport {
    /// Retained proposal, not permission to execute anything.
    pub proposal: HttpCorrectionPlanProposal,
    /// Complete actual wire identity.
    pub projection: ModelWireProjection,
    /// Complete conservative phase reservation.
    pub budget: PhaseWireBudget,
    /// Actual provider-reported counts, distinct from reservations.
    pub usage: ModelProviderUsage,
}
/// Closed current-run Plan failure; invalid output never becomes a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpCorrectionPlanExecutionFailure {
    /// Core or reservation binding mismatch.
    Binding,
    /// Invalid preparation or structure.
    Preparation(HttpCorrectionPlanFailure),
    /// Complete actual wire does not fit.
    Wire(PhaseWireBudgetError),
    /// Durable debit or settlement failed.
    Reservation(EditPhaseReservationFailure),
    /// Current source changed or is unavailable.
    Source(AgentSourceReadFailure),
    /// Provider did not complete the bounded job.
    Provider(ModelProviderFailure),
    /// Missing or excessive actual usage.
    Usage,
    /// Valid JSON contained a rejected secret candidate; no semantic repair or retained text.
    UnsafeReply,
    /// Invalid or incomplete output, eligible only for the stored single repair.
    InvalidReply,
}
impl fmt::Display for HttpCorrectionPlanExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP correction Plan execution failed: {self:?}")
    }
}
impl std::error::Error for HttpCorrectionPlanExecutionFailure {}
