//! Runs one bounded original-intake job through the checked provider wire.

use crate::model_stream_collector::{ModelStreamCollectionFailure, collect_projected_model_events};
use crate::model_wire::{bounded_phase_timeout, phase_elapsed_timestamp};
use crate::{
    EditPhaseWireCost, GoalContractStore, GoalContractStoreFailure, GoalIntakeLoadResult,
    IntakePhaseAnchor, IntakePhaseAnchorFailure, IntakePhaseFailure, IntakePhaseFinish,
    IntakePhaseRejection, IntakePhaseReservationFailure, IntakePhaseReservationStore,
    MAX_INTAKE_PHASE_REPLY_BYTES, ModelFinishReason, ModelOperationControl, ModelProvider,
    ModelProviderFailure, ModelProviderUsage, ModelRequestTimeout, ModelWireProjection,
    PhaseWireBudget, PhaseWireBudgetError, PreparedIntakePhase, StoredPendingGoalIntake,
};
use a3_domain::{
    AgentRunBudget, AgentRunTimestamp, GoalIntakeReview, GoalIntakeReviewError, ProjectIdentity,
};
use std::{error::Error, fmt, time::Instant};

/// The existing provider and Goal boundary, with a durable Pending reservation before I/O.
pub struct ExecuteIntakePhase<'a> {
    provider: &'a dyn ModelProvider,
    goals: &'a dyn GoalContractStore,
    reservations: &'a dyn IntakePhaseReservationStore,
}

impl<'a> ExecuteIntakePhase<'a> {
    /// Wires the exact provider and the existing Pending/Review authority.
    #[must_use]
    pub const fn new(
        provider: &'a dyn ModelProvider,
        goals: &'a dyn GoalContractStore,
        reservations: &'a dyn IntakePhaseReservationStore,
    ) -> Self {
        Self {
            provider,
            goals,
            reservations,
        }
    }

    /// Interprets one delivered original range and atomically appends its observed Review.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        phase: &PreparedIntakePhase,
        task_budget: AgentRunBudget,
        observed_at: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &dyn ModelOperationControl,
    ) -> Result<IntakePhaseExecutionReport, IntakePhaseExecutionFailure> {
        let started = Instant::now();
        let anchor = phase.anchor();
        let request = phase.request();
        if project.worktree().id() != anchor.worktree()
            || request.profile().reference() != anchor.model_profile()
            || request.profile().provider_id() != self.provider.provider_id()
        {
            return Err(IntakePhaseExecutionFailure::Binding);
        }
        if control.is_cancelled() {
            return Err(IntakePhaseExecutionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
        }
        let projection = self
            .provider
            .project_wire_request(request)
            .map_err(IntakePhaseExecutionFailure::Provider)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())
            .map_err(IntakePhaseExecutionFailure::WireBudget)?;
        let wire = EditPhaseWireCost::new(
            projection.digest(),
            projection.request_bytes(),
            budget.prompt_tokens(),
            budget.output_tokens(),
        )
        .map_err(IntakePhaseExecutionFailure::WireCost)?;
        self.load_current(project, anchor).await?;
        let reserved_at = elapsed_at(started, observed_at)?;
        let reservation = self
            .reservations
            .reserve_intake_phase(
                project,
                anchor,
                phase.kind(),
                task_budget,
                wire,
                reserved_at,
            )
            .await
            .map_err(IntakePhaseExecutionFailure::Reservation)?;
        if reservation.anchor() != anchor
            || reservation.kind() != phase.kind()
            || reservation.wire() != wire
            || reservation.reserved_at() != reserved_at
        {
            return Err(IntakePhaseExecutionFailure::Binding);
        }
        let received = match bounded_phase_timeout(
            timeout,
            reservation.deadline(),
            elapsed_at(started, observed_at)?,
        ) {
            Some(remaining) if !control.is_cancelled() => {
                collect_projected_model_events(
                    self.provider,
                    request,
                    projection,
                    remaining,
                    control,
                    MAX_INTAKE_PHASE_REPLY_BYTES,
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
        let (review, rejection, usage) = match received {
            Ok(response) => {
                let (text, completion) = response.into_parts();
                let usage = completion.usage();
                if !budget.accepts_usage(projection, usage) {
                    (
                        Err(IntakePhaseExecutionFailure::Usage),
                        IntakePhaseRejection::Unknown,
                        usage,
                    )
                } else if completion.reason() != ModelFinishReason::Stop {
                    let rejection = if completion.reason() == ModelFinishReason::OutputLimit {
                        IntakePhaseRejection::InvalidStructure
                    } else {
                        IntakePhaseRejection::RejectedReply
                    };
                    (
                        Err(IntakePhaseExecutionFailure::IncompleteReply),
                        rejection,
                        usage,
                    )
                } else if control.is_cancelled() {
                    (
                        Err(IntakePhaseExecutionFailure::Provider(
                            ModelProviderFailure::Cancelled,
                        )),
                        IntakePhaseRejection::Cancelled,
                        usage,
                    )
                } else {
                    let result = self.build_successor(project, phase, text.as_bytes()).await;
                    let rejection = match &result {
                        Err(IntakePhaseExecutionFailure::Reply(
                            IntakePhaseFailure::InvalidReply
                            | IntakePhaseFailure::WrongWindow
                            | IntakePhaseFailure::ReplyTooLarge,
                        )) => IntakePhaseRejection::InvalidStructure,
                        Err(IntakePhaseExecutionFailure::GoalStore(_)) => {
                            IntakePhaseRejection::Unknown
                        }
                        _ => IntakePhaseRejection::RejectedReply,
                    };
                    (result, rejection, usage)
                }
            }
            Err(failure) => {
                let failure = match failure {
                    ModelStreamCollectionFailure::Provider(error) => error,
                    ModelStreamCollectionFailure::OutputTooLarge(_) => {
                        ModelProviderFailure::InvalidResponse
                    }
                };
                let rejection = if failure == ModelProviderFailure::Cancelled {
                    IntakePhaseRejection::Cancelled
                } else {
                    IntakePhaseRejection::ProviderFailure
                };
                (
                    Err(IntakePhaseExecutionFailure::Provider(failure)),
                    rejection,
                    ModelProviderUsage::new(None, None),
                )
            }
        };
        let prompt_tokens = usage
            .prompt_tokens()
            .and_then(|tokens| u32::try_from(tokens).ok());
        let output_tokens = usage
            .output_tokens()
            .and_then(|tokens| u32::try_from(tokens).ok());
        let settlement = match (&review, prompt_tokens, output_tokens) {
            (Ok(review), Some(prompt_tokens), Some(output_tokens)) => IntakePhaseFinish::Review {
                review,
                prompt_tokens,
                output_tokens,
            },
            _ => IntakePhaseFinish::ObservedFailure {
                kind: rejection,
                reported_prompt_tokens: prompt_tokens,
                reported_output_tokens: output_tokens,
            },
        };
        self.reservations
            .finish_intake_phase(
                project,
                reservation,
                settlement,
                elapsed_at(started, observed_at)?,
            )
            .await
            .map_err(IntakePhaseExecutionFailure::Reservation)?;
        if control.is_cancelled() {
            return Err(IntakePhaseExecutionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
        }
        Ok(IntakePhaseExecutionReport {
            review: review?,
            projection,
            budget,
            usage,
        })
    }

    async fn load_current(
        &self,
        project: &ProjectIdentity,
        anchor: IntakePhaseAnchor,
    ) -> Result<(StoredPendingGoalIntake, Option<GoalIntakeReview>), IntakePhaseExecutionFailure>
    {
        let task = anchor.goal().task_id();
        let Some(GoalIntakeLoadResult::Pending(stored)) = self
            .goals
            .load_goal_intake(project, task)
            .await
            .map_err(IntakePhaseExecutionFailure::GoalStore)?
        else {
            return Err(IntakePhaseExecutionFailure::Binding);
        };
        let review = self
            .goals
            .load_goal_intake_review(project, task)
            .await
            .map_err(IntakePhaseExecutionFailure::GoalStore)?;
        let current = IntakePhaseAnchor::new(
            project,
            &stored,
            review.as_ref().map(GoalIntakeReview::revision),
            anchor.window(),
            anchor.model_profile(),
        )
        .map_err(IntakePhaseExecutionFailure::Anchor)?;
        if current != anchor {
            return Err(IntakePhaseExecutionFailure::Binding);
        }
        Ok((stored, review))
    }

    async fn build_successor(
        &self,
        project: &ProjectIdentity,
        phase: &PreparedIntakePhase,
        reply: &[u8],
    ) -> Result<GoalIntakeReview, IntakePhaseExecutionFailure> {
        let anchor = phase.anchor();
        let (stored, previous) = self.load_current(project, anchor).await?;
        let proposal = phase
            .complete(reply, anchor, &stored, previous.as_ref())
            .map_err(IntakePhaseExecutionFailure::Reply)?;
        match previous {
            None => GoalIntakeReview::begin(stored.pending(), proposal)
                .map_err(IntakePhaseExecutionFailure::Review),
            Some(mut review) => {
                review
                    .append(stored.pending(), proposal)
                    .map_err(IntakePhaseExecutionFailure::Review)?;
                Ok(review)
            }
        }
    }
}

fn elapsed_at(
    started: Instant,
    observed_at: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, IntakePhaseExecutionFailure> {
    phase_elapsed_timestamp(started, observed_at).ok_or(IntakePhaseExecutionFailure::Clock)
}

/// Committed typed interpretation and distinct estimated and observed token accounting.
#[derive(Debug)]
pub struct IntakePhaseExecutionReport {
    /// Durable Review after the atomic reservation settlement.
    pub review: GoalIntakeReview,
    /// Exact request body identity and options.
    pub projection: ModelWireProjection,
    /// Conservative charged wire estimate.
    pub budget: PhaseWireBudget,
    /// Actual complete terminal provider counters.
    pub usage: ModelProviderUsage,
}

/// Content-free failure before a candidate interpretation is visible.
#[derive(Debug)]
pub enum IntakePhaseExecutionFailure {
    /// Worktree, original, Review predecessor or provider identity changed.
    Binding,
    /// The original range no longer resolves to the pending request.
    Anchor(IntakePhaseAnchorFailure),
    /// Actual wire options or sizes cannot fit.
    WireBudget(PhaseWireBudgetError),
    /// The positive reservation costs could not be represented.
    WireCost(crate::EditPhaseReservationFailure),
    /// The durable pre-I/O reservation or atomic Review settlement failed.
    Reservation(IntakePhaseReservationFailure),
    /// Pending capture or Review could not be loaded.
    GoalStore(GoalContractStoreFailure),
    /// The checked provider transport or stream failed.
    Provider(ModelProviderFailure),
    /// Reported usage was missing or outside the admitted budget.
    Usage,
    /// The provider stopped before the complete structured response.
    IncompleteReply,
    /// Exact quote classification crossed a structured phase boundary.
    Reply(IntakePhaseFailure),
    /// The typed findings could not extend the existing Review.
    Review(GoalIntakeReviewError),
    /// The owner timestamp and elapsed duration could not be represented.
    Clock,
}

impl fmt::Display for IntakePhaseExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Binding => "intake phase no longer matches the pending request",
            Self::Anchor(_) => "intake phase source anchor changed",
            Self::WireBudget(_) => "intake phase wire capacity was rejected",
            Self::WireCost(_) => "intake phase wire cost cannot be represented",
            Self::Reservation(_) => "intake reservation or settlement failed",
            Self::GoalStore(_) => "intake phase cannot load its pending goal",
            Self::Provider(_) => "intake phase transport failed",
            Self::Usage => "intake provider usage was missing or out of bounds",
            Self::IncompleteReply => "intake provider reply did not finish normally",
            Self::Reply(_) => "intake provider reply did not match its source contract",
            Self::Review(_) => "intake findings cannot extend the current review",
            Self::Clock => "intake phase timing cannot be represented",
        })
    }
}

impl Error for IntakePhaseExecutionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Anchor(error) => Some(error),
            Self::WireBudget(error) => Some(error),
            Self::WireCost(error) => Some(error),
            Self::Reservation(error) => Some(error),
            Self::GoalStore(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::Reply(error) => Some(error),
            Self::Review(error) => Some(error),
            Self::Binding | Self::Usage | Self::IncompleteReply | Self::Clock => None,
        }
    }
}
