//! Owned bounded provider call; no decoded result escapes before durable settlement.
use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_projected_model_events};
use crate::model_wire::{bounded_phase_timeout, phase_elapsed_timestamp};
use crate::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, EditPhaseCallKind,
    EditPhaseObservation, EditPhaseObservationOutcome, EditPhaseReservationFailure,
    EditPhaseWireCost, HttpCorrectionPlanStore, HttpNoChangeReplanFailure,
    HttpNoChangeReplanSettlement, ModelFinishReason, ModelOperationControl, ModelProvider,
    ModelProviderFailure, ModelProviderUsage, ModelRequestTimeout, ModelWireProjection,
    PhaseWireBudget, PhaseWireBudgetError, PreparedHttpNoChangeReplanV6Job,
};
use a3_domain::{AgentRunTimestamp, ProjectIdentity};
use std::{fmt, time::Instant};

/// Executes one reserved V6 window-selection call; no automatic follow-up or mutation.
pub struct ExecuteHttpNoChangeReplan<'a> {
    provider: &'a dyn ModelProvider,
    source: &'a dyn AgentSourceReader,
    store: &'a dyn HttpCorrectionPlanStore,
}
impl<'a> ExecuteHttpNoChangeReplan<'a> {
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
        phase: &PreparedHttpNoChangeReplanV6Job,
        kind: EditPhaseCallKind,
        now: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &C,
    ) -> Result<HttpNoChangeReplanExecutionReport, HttpNoChangeReplanExecutionFailure>
    where
        C: ModelOperationControl + AgentSourceReadControl,
    {
        use HttpNoChangeReplanExecutionFailure as E;
        let started = Instant::now();
        if phase.anchor().closed_owner.worktree != project.worktree().id()
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
            .reserve_http_no_change_replan(project, phase, kind, wire, reserved_at)
            .await
            .map_err(E::Reservation)?;
        if reservation.job != phase.job_digest()
            || reservation.kind != kind
            || reservation.wire != wire
            || reservation.reserved_at != reserved_at
            || reservation.deadline != phase.closed_origin().fact().call().deadline()
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
                    512,
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
                } else {
                    match crate::http_no_change_replan::reply_contains_secret(text.as_bytes()) {
                        Ok(true) => (
                            EditPhaseObservationOutcome::RejectedReply,
                            usage,
                            Some(E::UnsafeReply),
                        ),
                        Err(_) => (
                            EditPhaseObservationOutcome::RejectedReply,
                            usage,
                            Some(E::UnsafeInspection),
                        ),
                        Ok(false) => {
                            if completion.reason() != ModelFinishReason::Stop {
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
                                    Err(HttpNoChangeReplanFailure::UnsafeContent) => (
                                        EditPhaseObservationOutcome::RejectedReply,
                                        usage,
                                        Some(E::UnsafeReply),
                                    ),
                                    Err(HttpNoChangeReplanFailure::UnsafeInspection) => (
                                        EditPhaseObservationOutcome::RejectedReply,
                                        usage,
                                        Some(E::UnsafeInspection),
                                    ),
                                    Err(_) => (
                                        EditPhaseObservationOutcome::InvalidStructure,
                                        usage,
                                        Some(E::InvalidReply),
                                    ),
                                }
                            }
                        }
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
            .finish_http_no_change_replan(
                project,
                phase,
                reservation,
                observation,
                proposal.clone(),
            )
            .await
            .map_err(E::Reservation)?;
        if let Some(error) = failure {
            return Err(error);
        }
        if stored.job_digest() != phase.job_digest()
            || stored.call_kind() != kind
            || stored.wire() != wire
            || stored.reserved_at() != reserved_at
            || stored.observation() != observation
            || stored.decision() != proposal.as_ref()
        {
            return Err(E::Binding);
        }
        if stored.decision().is_none() {
            return Err(E::Binding);
        }
        Ok(HttpNoChangeReplanExecutionReport {
            settlement: stored,
            projection,
            budget,
            usage,
        })
    }
}
/// Fully settled non-executable proposal and measured actual provider usage.
pub struct HttpNoChangeReplanExecutionReport {
    /// Retained proposal, not permission to execute anything.
    pub settlement: HttpNoChangeReplanSettlement,
    /// Complete actual wire identity.
    pub projection: ModelWireProjection,
    /// Complete conservative phase reservation.
    pub budget: PhaseWireBudget,
    /// Actual provider-reported counts, distinct from reservations.
    pub usage: ModelProviderUsage,
}
/// Closed current-run Plan failure; invalid output never becomes a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpNoChangeReplanExecutionFailure {
    /// Core or reservation binding mismatch.
    Binding,
    /// Invalid preparation or structure.
    Preparation(HttpNoChangeReplanFailure),
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
    /// Bounded inspection could not complete; no secret claim, retained text or repair.
    UnsafeInspection,
    /// Invalid phase structure or bounded collection; only stored InvalidStructure permits repair.
    InvalidReply,
}
impl fmt::Display for HttpNoChangeReplanExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP NoChange V6 Plan execution failed: {self:?}")
    }
}
impl std::error::Error for HttpNoChangeReplanExecutionFailure {}
