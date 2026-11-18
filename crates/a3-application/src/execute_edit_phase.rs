//! One checked provider dispatch after the durable candidate Edit reservation.

use crate::edit_phase::MAX_EDIT_PHASE_REPLY_BYTES;
use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_projected_model_events};
use crate::model_wire::bounded_phase_timeout;
use crate::{
    CompiledEditPhaseRequest, EditPhaseFailure, EditPhaseHalt, EditPhaseObservation,
    EditPhaseObservationOutcome, EditPhaseOutcome, EditPhaseReservationFailure,
    EditPhaseReservationStore, EditPhaseState, EditPhaseWireCost, ModelFinishReason,
    ModelOperationControl, ModelProvider, ModelProviderFailure, ModelProviderUsage,
    ModelRequestTimeout, ModelWireProjection, PatchPreviewFailure, PhaseWireBudget,
    PhaseWireBudgetError, PreparedEditPhase, SourceWireDelivery, WorkspacePatchControl,
    WorkspacePatchTool,
};
use a3_domain::AgentRunTimestamp;
use std::{error::Error, fmt};

/// The existing local provider, durable accounting and safe read boundary for one edit.
pub struct ExecuteEditPhase<'a> {
    provider: &'a dyn ModelProvider,
    reservations: &'a dyn EditPhaseReservationStore,
    patch_tool: &'a dyn WorkspacePatchTool,
}

impl<'a> ExecuteEditPhase<'a> {
    /// Uses the candidate's checked provider and durable reservation adapter.
    #[must_use]
    pub const fn new(
        provider: &'a dyn ModelProvider,
        reservations: &'a dyn EditPhaseReservationStore,
        patch_tool: &'a dyn WorkspacePatchTool,
    ) -> Self {
        Self {
            provider,
            reservations,
            patch_tool,
        }
    }

    /// Reserves one actual request, consumes its stream once and settles before exposing a result.
    pub async fn execute<C>(
        self,
        phase: PreparedEditPhase,
        current: EditPhaseState<'_>,
        compiled: &CompiledEditPhaseRequest,
        observed_at: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &C,
    ) -> Result<EditPhaseExecutionReport, EditPhaseExecutionFailure>
    where
        C: ModelOperationControl + WorkspacePatchControl,
    {
        let clock = phase
            .owner_clock(observed_at, current)
            .map_err(EditPhaseExecutionFailure::Reply)?;
        clock
            .check(control)
            .map_err(EditPhaseExecutionFailure::Reply)?;
        phase
            .require_scope_binding(compiled)
            .map_err(EditPhaseExecutionFailure::Reply)?;
        phase
            .require_current_scope(current)
            .map_err(EditPhaseExecutionFailure::Reply)?;
        let request = compiled.request();
        // Only complete Existing/Replace lowering can establish the narrow
        // unchanged-current-bytes fact with its own actual acquired target Original.
        // Empty Add/Append and legacy untracked replacements remain generic rejection.
        let no_change_window = phase
            .no_change_window()
            .filter(|_| compiled.target_source().is_some());
        if phase.anchor() != current.anchor()
            || compiled.anchor() != phase.anchor()
            || compiled.job_digest() != phase.job_digest()
            || request.profile().reference() != phase.anchor().model_profile()
            || request.profile().provider_id() != self.provider.provider_id()
        {
            return Err(EditPhaseExecutionFailure::Binding);
        }
        if ModelOperationControl::is_cancelled(control) {
            return Err(EditPhaseExecutionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
        }
        let projection = self
            .provider
            .project_wire_request(request)
            .map_err(EditPhaseExecutionFailure::Provider)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())
            .map_err(EditPhaseExecutionFailure::WireBudget)?;
        let wire = EditPhaseWireCost::new(
            projection.digest(),
            projection.request_bytes(),
            budget.prompt_tokens(),
            budget.output_tokens(),
        )
        .map_err(EditPhaseExecutionFailure::Reservation)?;
        phase
            .revalidate(current, self.patch_tool, control)
            .await
            .map_err(EditPhaseExecutionFailure::Reply)?;
        compiled
            .revalidate_read_only(current, self.patch_tool, control)
            .await
            .map_err(EditPhaseExecutionFailure::Reply)?;
        let project = current.project();
        let reserved_at = clock.now().map_err(EditPhaseExecutionFailure::Reply)?;
        let kind = compiled.kind();
        let acquired = compiled.acquired_source();
        let reservation = self
            .reservations
            .reserve_edit_job(project, &phase, compiled, wire, reserved_at)
            .await
            .map_err(EditPhaseExecutionFailure::Reservation)?;
        if reservation.anchor() != phase.anchor()
            || reservation.job_digest() != phase.job_digest()
            || reservation.kind() != kind
            || reservation.wire() != wire
            || reservation.reserved_at() != reserved_at
            || reservation.request_digest() != Some(compiled.request_digest())
            || reservation.source_read_key() != acquired.map(crate::AcquiredSourceRead::key)
        {
            return Err(EditPhaseExecutionFailure::Binding);
        }

        let elapsed_at = clock.now().map_err(EditPhaseExecutionFailure::Reply)?;
        let remaining = bounded_phase_timeout(timeout, reservation.deadline(), elapsed_at);
        let dispatch_attempted =
            remaining.is_some() && !ModelOperationControl::is_cancelled(control);
        let received = match remaining {
            Some(remaining) if dispatch_attempted => {
                collect_projected_model_events(
                    self.provider,
                    request,
                    projection,
                    remaining,
                    control,
                    MAX_EDIT_PHASE_REPLY_BYTES,
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
        let (outcome, status, usage) = match received {
            Ok(response) => {
                let (text, completion) = response.into_parts();
                // Route the raw reply using its actual dispatch owner before any further await.
                // Model JSON supplies only the semantic payload, never this job binding.
                let bound_reply = phase.bind_request_reply(compiled, text.as_bytes());
                let usage = completion.usage();
                let (outcome, status) = if !budget.accepts_usage(projection, usage) {
                    (
                        Err(EditPhaseExecutionFailure::Usage),
                        EditPhaseObservationOutcome::Unknown,
                    )
                } else if completion.reason() == ModelFinishReason::OutputLimit {
                    (
                        Err(EditPhaseExecutionFailure::OutputLimit),
                        EditPhaseObservationOutcome::InvalidStructure,
                    )
                } else if completion.reason() != ModelFinishReason::Stop {
                    (
                        Err(EditPhaseExecutionFailure::IncompleteReply),
                        EditPhaseObservationOutcome::RejectedReply,
                    )
                } else {
                    let revalidated = async {
                        if compiled.target_source().is_some() {
                            // A legal NeedEvidence/TooLarge must not settle a stale target read.
                            phase.revalidate(current, self.patch_tool, control).await?;
                        }
                        compiled
                            .revalidate_read_only(current, self.patch_tool, control)
                            .await
                    }
                    .await;
                    let outcome = match revalidated {
                        Ok(()) => match bound_reply {
                            Ok(reply) => {
                                phase
                                    .complete_request(
                                        reply,
                                        current,
                                        compiled,
                                        self.patch_tool,
                                        control,
                                    )
                                    .await
                            }
                            Err(failure) => Err(failure),
                        },
                        Err(failure) => Err(failure),
                    };
                    let status = match &outcome {
                        Ok(_) => EditPhaseObservationOutcome::AcceptedReply,
                        Err(EditPhaseFailure::NoChange) if no_change_window.is_some() => {
                            EditPhaseObservationOutcome::NoChange
                        }
                        Err(
                            EditPhaseFailure::Cancelled
                            | EditPhaseFailure::ReadBase(PatchPreviewFailure::Cancelled),
                        ) => EditPhaseObservationOutcome::Cancelled,
                        Err(EditPhaseFailure::DeadlineReached) => {
                            EditPhaseObservationOutcome::DeadlineReached
                        }
                        Err(
                            EditPhaseFailure::InvalidReply
                            | EditPhaseFailure::InvalidReplacement
                            | EditPhaseFailure::TooLarge,
                        ) => EditPhaseObservationOutcome::InvalidStructure,
                        Err(_) => EditPhaseObservationOutcome::RejectedReply,
                    };
                    (outcome.map_err(EditPhaseExecutionFailure::Reply), status)
                };
                (outcome, status, usage)
            }
            Err(failure) => {
                let failure = match failure {
                    ModelStreamCollectionFailure::Provider(error) => error,
                    ModelStreamCollectionFailure::OutputTooLarge(_) => {
                        ModelProviderFailure::InvalidResponse
                    }
                };
                let status = if acquired.is_some() && dispatch_attempted {
                    EditPhaseObservationOutcome::Unknown
                } else if failure == ModelProviderFailure::Cancelled {
                    EditPhaseObservationOutcome::Cancelled
                } else {
                    EditPhaseObservationOutcome::ProviderFailure
                };
                (
                    Err(EditPhaseExecutionFailure::Provider(failure)),
                    status,
                    ModelProviderUsage::new(None, None),
                )
            }
        };
        let status = if acquired.is_some()
            && status == EditPhaseObservationOutcome::AcceptedReply
            && ModelOperationControl::is_cancelled(control)
        {
            EditPhaseObservationOutcome::Cancelled
        } else {
            status
        };
        // Deadline has its own durable stale settlement, never a rejection/cancellation alias.
        let known_reply = matches!(
            status,
            EditPhaseObservationOutcome::AcceptedReply
                | EditPhaseObservationOutcome::RejectedReply
                | EditPhaseObservationOutcome::InvalidStructure
                | EditPhaseObservationOutcome::NoChange
                | EditPhaseObservationOutcome::DeadlineReached
        );
        let status = if known_reply
            && clock.now().map_err(EditPhaseExecutionFailure::Reply)? >= clock.deadline()
        {
            EditPhaseObservationOutcome::DeadlineReached
        } else {
            status
        };
        let observation = EditPhaseObservation::new(
            status,
            usage
                .prompt_tokens()
                .and_then(|tokens| u32::try_from(tokens).ok()),
            usage
                .output_tokens()
                .and_then(|tokens| u32::try_from(tokens).ok()),
            clock.now().map_err(EditPhaseExecutionFailure::Reply)?,
        );
        if status == EditPhaseObservationOutcome::NoChange {
            let window = no_change_window
                .as_ref()
                .ok_or(EditPhaseExecutionFailure::Binding)?;
            self.reservations
                .finish_edit_no_change(
                    project,
                    reservation,
                    observation,
                    window,
                    current.published(),
                    request.profile(),
                )
                .await
                .map_err(EditPhaseExecutionFailure::Reservation)?;
        } else {
            self.reservations
                .finish_edit_job(
                    project,
                    reservation,
                    compiled,
                    observation,
                    if status == EditPhaseObservationOutcome::AcceptedReply {
                        match &outcome {
                            Ok(EditPhaseOutcome::NeedEvidence(need)) => {
                                Some(EditPhaseHalt::NeedEvidence(*need))
                            }
                            Ok(EditPhaseOutcome::TooLarge) => Some(EditPhaseHalt::TooLarge),
                            _ => None,
                        }
                    } else {
                        None
                    },
                )
                .await
                .map_err(EditPhaseExecutionFailure::Reservation)?;
        }
        if ModelOperationControl::is_cancelled(control) {
            return Err(EditPhaseExecutionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
        }
        clock
            .check(control)
            .map_err(EditPhaseExecutionFailure::Reply)?;
        let outcome = outcome?;
        let is_proposal = matches!(&outcome, EditPhaseOutcome::Proposal(_));
        Ok(EditPhaseExecutionReport {
            outcome,
            projection,
            budget,
            usage,
            source_delivery: compiled
                .source_original()
                .cloned()
                .filter(|_| is_proposal)
                .map(|source| {
                    SourceWireDelivery::from_observed_dispatch(source, projection.digest())
                }),
        })
    }
}

/// Settled edit result and distinct estimated and reported model consumption.
#[derive(Debug)]
pub struct EditPhaseExecutionReport {
    /// The only allowed result of this phase. The existing controller owns any subsequent patch.
    pub outcome: EditPhaseOutcome,
    /// Exact body fingerprint and encoded provider options.
    pub projection: ModelWireProjection,
    /// Conservative reservation made before I/O.
    pub budget: PhaseWireBudget,
    /// Actual complete terminal provider counters.
    pub usage: ModelProviderUsage,
    /// The sole acquired current original in the observed wire: replace target or supplemental caller.
    pub source_delivery: Option<SourceWireDelivery>,
}

/// Content-free failure before a candidate edit result can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseExecutionFailure {
    /// The compiled request, current execution state and reserved owner differed.
    Binding,
    /// Actual wire options or sizes cannot fit the pinned phase profile.
    WireBudget(PhaseWireBudgetError),
    /// Durable accounting or its exact current-owner check failed.
    Reservation(EditPhaseReservationFailure),
    /// One bounded transport or stream failed.
    Provider(ModelProviderFailure),
    /// Actual reported token usage was missing or outside its wire bounds.
    Usage,
    /// The provider reached its output limit with verified terminal usage.
    OutputLimit,
    /// Provider stopped before delivering a complete reply for another reason.
    IncompleteReply,
    /// A reply failed strict structure or current-source lowering.
    Reply(EditPhaseFailure),
    /// The owner timestamp could not represent elapsed time.
    Clock,
}

impl fmt::Display for EditPhaseExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Binding => "edit phase request does not match its current owner",
            Self::WireBudget(_) => "edit phase wire budget was rejected",
            Self::Reservation(_) => "edit phase reservation or settlement was rejected",
            Self::Provider(_) => "edit phase transport failed",
            Self::Usage => "edit phase provider usage was missing or out of bounds",
            Self::OutputLimit => "edit phase provider reached its output limit",
            Self::IncompleteReply => "edit phase provider reply did not finish normally",
            Self::Reply(_) => "edit phase reply cannot produce a current result",
            Self::Clock => "edit phase timing is not representable",
        })
    }
}

impl Error for EditPhaseExecutionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WireBudget(error) => Some(error),
            Self::Reservation(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::Reply(error) => Some(error),
            Self::Binding
            | Self::Usage
            | Self::OutputLimit
            | Self::IncompleteReply
            | Self::Clock => None,
        }
    }
}
