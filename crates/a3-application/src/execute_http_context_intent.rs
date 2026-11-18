//! One owned ContextIntent V1 dispatch after actual S1 acquisition; no plan application.
use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_projected_model_events};
use crate::model_wire::{bounded_phase_timeout, phase_elapsed_timestamp};
use crate::{
    AgentSourceReadControl, AgentSourceReader, EditPhaseCallKind, EditPhaseObservation,
    EditPhaseObservationOutcome, EditPhaseWireCost, HttpContextIntentReply,
    HttpCorrectionPlanFailure, HttpCorrectionPlanStore, ModelFinishReason, ModelOperationControl,
    ModelProvider, ModelProviderFailure, ModelProviderUsage, ModelRequestTimeout,
    ModelWireProjection, PhaseWireBudget, PreparedHttpContextIntent,
};
use a3_domain::{AgentRunTimestamp, ProjectIdentity};
use std::time::Instant;

/// Executes one current-run context-to-intention job; never materializes or activates a step.
pub struct ExecuteHttpContextIntent<'a> {
    provider: &'a dyn ModelProvider,
    source: &'a dyn AgentSourceReader,
    store: &'a dyn HttpCorrectionPlanStore,
}
impl<'a> ExecuteHttpContextIntent<'a> {
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
        phase: &PreparedHttpContextIntent,
        kind: EditPhaseCallKind,
        now: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &C,
    ) -> Result<HttpContextIntentExecutionReport, HttpContextIntentExecutionFailure>
    where
        C: ModelOperationControl + AgentSourceReadControl,
    {
        use HttpContextIntentExecutionFailure as E;
        let started = Instant::now();
        if phase.anchor().parent.worktree != project.worktree().id()
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
            .revalidate_sources(project, self.source, control)
            .await
            .map_err(E::Source)?;
        let at = || phase_elapsed_timestamp(started, now).ok_or(E::Binding);
        let reserved_at = at()?;
        let reservation = self
            .store
            .reserve_http_context_intent(project, phase, kind, wire, reserved_at)
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
                    match phase
                        .bind_reply(text.as_bytes())
                        .and_then(|bound| phase.decode_reply(bound))
                    {
                        Ok(value) => {
                            proposal = Some(value);
                            (EditPhaseObservationOutcome::AcceptedReply, usage, None)
                        }
                        Err(HttpCorrectionPlanFailure::Binding) => (
                            EditPhaseObservationOutcome::RejectedReply,
                            usage,
                            Some(E::Binding),
                        ),
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
            && let Err(error) = phase
                .revalidate_sources(project, self.source, control)
                .await
        {
            proposal = None;
            outcome = EditPhaseObservationOutcome::RejectedReply;
            failure = Some(E::Source(error));
        }
        if ModelOperationControl::is_cancelled(control) {
            proposal = None;
            outcome = EditPhaseObservationOutcome::Cancelled;
            failure = Some(E::Provider(ModelProviderFailure::Cancelled));
        }
        let observation = EditPhaseObservation::new(
            outcome,
            usage.prompt_tokens().and_then(|v| u32::try_from(v).ok()),
            usage.output_tokens().and_then(|v| u32::try_from(v).ok()),
            at()?,
        );
        let stored = self
            .store
            .finish_http_context_intent(project, phase, reservation, observation, proposal.clone())
            .await
            .map_err(E::Reservation)?;
        if ModelOperationControl::is_cancelled(control) {
            return Err(E::Provider(ModelProviderFailure::Cancelled));
        }
        if let Some(error) = failure {
            return Err(error);
        }
        if stored != proposal {
            return Err(E::Binding);
        }
        let proposal = stored.ok_or(E::Binding)?;
        Ok(HttpContextIntentExecutionReport {
            reply: proposal,
            projection,
            budget,
            usage,
            source_delivery: phase.source_delivery(projection.digest()),
            primary_delivery: phase.primary_delivery(projection.digest()),
        })
    }
}
/// Fully settled non-executable proposal and measured actual provider usage.
pub struct HttpContextIntentExecutionReport {
    /// Ready or null unverified result; neither grants materialization or execution.
    pub reply: HttpContextIntentReply,
    /// Complete actual wire identity.
    pub projection: ModelWireProjection,
    /// Complete conservative phase reservation.
    pub budget: PhaseWireBudget,
    /// Actual provider-reported counts, distinct from reservations.
    pub usage: ModelProviderUsage,
    /// Full actually delivered S1, the only range eligible for new source-byte credit.
    pub source_delivery: crate::SourceWireDelivery,
    /// Full repeated primary in this same wire, never counted again as new bytes.
    pub primary_delivery: crate::SourceWireDelivery,
}
/// Existing typed provider/debit/source failure categories, shared with the V4 phase.
pub type HttpContextIntentExecutionFailure = crate::HttpCorrectionPlanExecutionFailure;
