//! One checked initial Plan call, with durable accounting before provider I/O.

use crate::model_stream_collector::{
    CollectedModelOutput, ModelStreamCollectionFailure, collect_projected_model_events,
};
use crate::model_wire::{bounded_phase_timeout, phase_elapsed_timestamp};
use crate::plan_phase::MAX_PLAN_PHASE_REPLY_BYTES;
use crate::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, CommandDiscoveryFailure,
    DiscoverProjectCommands, EditPhaseCallKind, EditPhaseReservationFailure, EditPhaseWireCost,
    GoalContractStore, GoalContractStoreFailure, InitialPlanFinish, InitialPlanRejection,
    InitialPlanReservationFailure, InitialPlanReservationStore, KnowledgeIndexFailure,
    ModelFinishReason, ModelMessage, ModelMessageError, ModelMessageRole, ModelOperationControl,
    ModelProvider, ModelProviderFailure, ModelProviderRequest, ModelProviderRequestError,
    ModelProviderUsage, ModelRequestTimeout, ModelWireProjection, PhaseWireBudget,
    PhaseWireBudgetError, PlanEvidenceKind, PlanPhaseFailure, PlanPhaseHalt, PlanPhaseOutcome,
    PlanPhaseState, PreparedPlanPhase, StoredTaskLedger, TaskLedgerStoreVersion, TaskLensControl,
    TaskLensIndexStore,
};
use a3_domain::{
    AgentRunTimestamp, GoalContract, ModelProfile, ProjectCommandCatalog, ProjectIdentity,
    PublishedIndex, TaskLedger, TaskStepId,
};
use std::{error::Error, fmt, sync::Arc, time::Instant};

const REPAIR_INSTRUCTION: &str = "\nFix JSON; name a change.";
const ADD_SELECTION_REPAIR_INSTRUCTION: &str = "\nFix JSON; choose decision.";

/// Executes one initial planning phase against the existing Goal and publication authority.
pub struct ExecuteInitialPlan<'a> {
    provider: &'a dyn ModelProvider,
    goals: &'a dyn GoalContractStore,
    index: &'a dyn TaskLensIndexStore,
    source: &'a dyn AgentSourceReader,
    reservations: &'a dyn InitialPlanReservationStore,
}

impl<'a> ExecuteInitialPlan<'a> {
    /// Uses the existing checked provider and the candidate's durable reservation boundary.
    #[must_use]
    pub const fn new(
        provider: &'a dyn ModelProvider,
        goals: &'a dyn GoalContractStore,
        index: &'a dyn TaskLensIndexStore,
        source: &'a dyn AgentSourceReader,
        reservations: &'a dyn InitialPlanReservationStore,
    ) -> Self {
        Self {
            provider,
            goals,
            index,
            source,
            reservations,
        }
    }

    /// Sends one bound initial or repair request and settles it before returning a result.
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        phase: &PreparedPlanPhase,
        kind: EditPhaseCallKind,
        observed_at: AgentRunTimestamp,
        timeout: ModelRequestTimeout,
        control: &C,
    ) -> Result<InitialPlanExecutionReport, InitialPlanExecutionFailure>
    where
        C: ModelOperationControl + TaskLensControl + AgentSourceReadControl,
    {
        let started = Instant::now();
        let anchor = phase.anchor();
        let initial = phase.request();
        if project.worktree().id() != anchor.worktree()
            || initial.profile().reference() != anchor.model_profile()
            || initial.profile().provider_id() != self.provider.provider_id()
        {
            return Err(InitialPlanExecutionFailure::Binding);
        }
        require_not_cancelled(control)?;
        let repair = match kind {
            EditPhaseCallKind::Initial => None,
            EditPhaseCallKind::StructuralRepair => {
                Some(repair_request(initial, phase.addition_selection())?)
            }
        };
        let request = repair.as_ref().unwrap_or(initial);
        let projection = self
            .provider
            .project_wire_request(request)
            .map_err(InitialPlanExecutionFailure::Provider)?;
        let budget = PhaseWireBudget::fit(projection, request.profile())
            .map_err(InitialPlanExecutionFailure::WireBudget)?;
        let wire = EditPhaseWireCost::new(
            projection.digest(),
            projection.request_bytes(),
            budget.prompt_tokens(),
            budget.output_tokens(),
        )
        .map_err(InitialPlanExecutionFailure::WireCost)?;

        self.load_current(project, phase, control).await?;
        phase
            .revalidate_source(project, self.source, control)
            .await
            .map_err(InitialPlanExecutionFailure::Source)?;
        require_not_cancelled(control)?;
        let reserved_at = elapsed_at(started, observed_at)?;
        let reservation = self
            .reservations
            .reserve_initial_plan(project, phase, kind, wire, reserved_at)
            .await
            .map_err(InitialPlanExecutionFailure::Reservation)?;
        if reservation.anchor() != anchor
            || reservation.job_digest() != phase.job_digest()
            || reservation.kind() != kind
            || reservation.wire() != wire
            || reservation.reserved_at() != reserved_at
        {
            return Err(InitialPlanExecutionFailure::Binding);
        }

        let received = match bounded_phase_timeout(
            timeout,
            reservation.deadline(),
            elapsed_at(started, observed_at)?,
        ) {
            Some(remaining) if !ModelOperationControl::is_cancelled(control) => {
                collect_projected_model_events(
                    self.provider,
                    request,
                    projection,
                    remaining,
                    control,
                    MAX_PLAN_PHASE_REPLY_BYTES,
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
        let mut observed = match received {
            Ok(response) => {
                self.observe_reply(project, phase, response, projection, budget, control)
                    .await
            }
            Err(error) => PlanObservation::from_collection_failure(error),
        };
        if ModelOperationControl::is_cancelled(control) && observed.outcome.is_ok() {
            observed.outcome = Err(InitialPlanExecutionFailure::Provider(
                ModelProviderFailure::Cancelled,
            ));
            observed.rejection = InitialPlanRejection::Cancelled;
        }
        let usage = observed.usage;
        let (settlement, disposition) = observed.into_settlement();
        let stored = self
            .reservations
            .finish_initial_plan(
                project,
                reservation,
                settlement,
                elapsed_at(started, observed_at)?,
            )
            .await
            .map_err(InitialPlanExecutionFailure::Reservation)?;
        require_not_cancelled(control)?;
        let outcome = match (disposition?, stored) {
            (PlanDisposition::Materialize(expected), Some(stored))
                if stored.ledger() == &expected
                    && stored.version() == TaskLedgerStoreVersion::INITIAL =>
            {
                InitialPlanExecutionOutcome::Materialized(stored)
            }
            (PlanDisposition::NeedEvidence { offer, kind }, None) => {
                InitialPlanExecutionOutcome::NeedEvidence { offer, kind }
            }
            (PlanDisposition::TooLarge, None) => InitialPlanExecutionOutcome::TooLarge,
            (PlanDisposition::Halted(reason), None) => InitialPlanExecutionOutcome::Halted(reason),
            _ => return Err(InitialPlanExecutionFailure::Binding),
        };
        Ok(InitialPlanExecutionReport {
            outcome,
            projection,
            budget,
            usage,
        })
    }

    async fn load_current(
        &self,
        project: &ProjectIdentity,
        phase: &PreparedPlanPhase,
        control: &dyn TaskLensControl,
    ) -> Result<CurrentPlanInputs, InitialPlanExecutionFailure> {
        let anchor = phase.anchor();
        let goal = self
            .goals
            .load_current_goal_contract(project, anchor.goal().task_id())
            .await
            .map_err(InitialPlanExecutionFailure::GoalStore)?
            .ok_or(InitialPlanExecutionFailure::Binding)?;
        let published = self
            .index
            .load_current_index(project, control)
            .await
            .map_err(InitialPlanExecutionFailure::Index)?
            .ok_or(InitialPlanExecutionFailure::Binding)?;
        let catalog = if anchor.command_catalog_id().is_some() {
            Some(
                DiscoverProjectCommands
                    .execute_plan22(project.worktree().id(), &published)
                    .map_err(InitialPlanExecutionFailure::Commands)?,
            )
        } else {
            None
        };
        let current = CurrentPlanInputs {
            goal,
            published,
            catalog,
        };
        if current.state(project, phase.request().profile())?.anchor() != anchor {
            return Err(InitialPlanExecutionFailure::Binding);
        }
        Ok(current)
    }

    async fn observe_reply<C>(
        &self,
        project: &ProjectIdentity,
        phase: &PreparedPlanPhase,
        response: CollectedModelOutput,
        projection: ModelWireProjection,
        budget: PhaseWireBudget,
        control: &C,
    ) -> PlanObservation
    where
        C: ModelOperationControl + TaskLensControl + AgentSourceReadControl,
    {
        let (text, completion) = response.into_parts();
        let usage = completion.usage();
        if !budget.accepts_usage(projection, usage) {
            return PlanObservation::failed(
                InitialPlanExecutionFailure::Usage,
                InitialPlanRejection::Unknown,
                usage,
            );
        }
        if completion.reason() != ModelFinishReason::Stop {
            let rejection = if completion.reason() == ModelFinishReason::OutputLimit {
                InitialPlanRejection::InvalidStructure
            } else {
                InitialPlanRejection::RejectedReply
            };
            return PlanObservation::failed(
                InitialPlanExecutionFailure::IncompleteReply,
                rejection,
                usage,
            );
        }
        if let Err(error) = require_not_cancelled(control) {
            return PlanObservation::failed(error, InitialPlanRejection::Cancelled, usage);
        }
        let current = match self.load_current(project, phase, control).await {
            Ok(current) => current,
            Err(error) => {
                return PlanObservation::failed(error, InitialPlanRejection::RejectedReply, usage);
            }
        };
        if let Err(failure) = phase.revalidate_source(project, self.source, control).await {
            let rejection = if failure == AgentSourceReadFailure::Cancelled {
                InitialPlanRejection::Cancelled
            } else {
                InitialPlanRejection::RejectedReply
            };
            return PlanObservation::failed(
                InitialPlanExecutionFailure::Source(failure),
                rejection,
                usage,
            );
        }
        let outcome = current
            .state(project, phase.request().profile())
            .and_then(|current| {
                phase
                    .complete(current, text.as_bytes())
                    .map_err(InitialPlanExecutionFailure::Reply)
            });
        let rejection = match &outcome {
            Err(InitialPlanExecutionFailure::Reply(
                PlanPhaseFailure::InvalidReply
                | PlanPhaseFailure::UnknownOffer
                | PlanPhaseFailure::DuplicateOffer
                | PlanPhaseFailure::InvalidDependency
                | PlanPhaseFailure::Capacity,
            )) => InitialPlanRejection::InvalidStructure,
            _ => InitialPlanRejection::RejectedReply,
        };
        PlanObservation {
            outcome,
            rejection,
            usage,
        }
    }
}

struct CurrentPlanInputs {
    goal: GoalContract,
    published: Arc<PublishedIndex>,
    catalog: Option<ProjectCommandCatalog>,
}

impl CurrentPlanInputs {
    fn state<'a>(
        &'a self,
        project: &ProjectIdentity,
        profile: &'a ModelProfile,
    ) -> Result<PlanPhaseState<'a>, InitialPlanExecutionFailure> {
        PlanPhaseState::new(
            project,
            &self.goal,
            &self.published,
            profile,
            self.catalog.as_ref(),
        )
        .map_err(InitialPlanExecutionFailure::Reply)
    }
}

enum PlanDisposition {
    Halted(PlanPhaseHalt),
    Materialize(TaskLedger),
    NeedEvidence {
        offer: TaskStepId,
        kind: PlanEvidenceKind,
    },
    TooLarge,
}

struct PlanObservation {
    outcome: Result<PlanPhaseOutcome, InitialPlanExecutionFailure>,
    rejection: InitialPlanRejection,
    usage: ModelProviderUsage,
}

impl PlanObservation {
    fn failed(
        failure: InitialPlanExecutionFailure,
        rejection: InitialPlanRejection,
        usage: ModelProviderUsage,
    ) -> Self {
        Self {
            outcome: Err(failure),
            rejection,
            usage,
        }
    }

    fn from_collection_failure(error: ModelStreamCollectionFailure) -> Self {
        let failure = match error {
            ModelStreamCollectionFailure::Provider(error) => error,
            ModelStreamCollectionFailure::OutputTooLarge(_) => {
                ModelProviderFailure::InvalidResponse
            }
        };
        let rejection = if failure == ModelProviderFailure::Cancelled {
            InitialPlanRejection::Cancelled
        } else {
            InitialPlanRejection::ProviderFailure
        };
        Self::failed(
            InitialPlanExecutionFailure::Provider(failure),
            rejection,
            ModelProviderUsage::new(None, None),
        )
    }

    fn into_settlement(
        self,
    ) -> (
        InitialPlanFinish,
        Result<PlanDisposition, InitialPlanExecutionFailure>,
    ) {
        let prompt = self
            .usage
            .prompt_tokens()
            .and_then(|tokens| u32::try_from(tokens).ok());
        let output = self
            .usage
            .output_tokens()
            .and_then(|tokens| u32::try_from(tokens).ok());
        let rejected = |kind| InitialPlanFinish::ObservedFailure {
            kind,
            reported_prompt_tokens: prompt,
            reported_output_tokens: output,
        };
        match (self.outcome, prompt, output) {
            (Ok(PlanPhaseOutcome::Halted(reason)), Some(prompt_tokens), Some(output_tokens))
                if prompt_tokens > 0 && output_tokens > 0 =>
            {
                (
                    InitialPlanFinish::Halted {
                        reason,
                        prompt_tokens,
                        output_tokens,
                    },
                    Ok(PlanDisposition::Halted(reason)),
                )
            }
            (Ok(PlanPhaseOutcome::Steps(plan)), Some(prompt_tokens), Some(output_tokens)) => {
                let expected = plan.ledger().clone();
                (
                    InitialPlanFinish::Accepted {
                        plan,
                        prompt_tokens,
                        output_tokens,
                    },
                    Ok(PlanDisposition::Materialize(expected)),
                )
            }
            (
                Ok(PlanPhaseOutcome::NeedEvidence { offer, kind }),
                Some(prompt_tokens),
                Some(output_tokens),
            ) => (
                InitialPlanFinish::NeedEvidence {
                    prompt_tokens,
                    output_tokens,
                },
                Ok(PlanDisposition::NeedEvidence { offer, kind }),
            ),
            (Ok(PlanPhaseOutcome::TooLarge), Some(prompt_tokens), Some(output_tokens)) => (
                InitialPlanFinish::TooLarge {
                    prompt_tokens,
                    output_tokens,
                },
                Ok(PlanDisposition::TooLarge),
            ),
            (Err(error), _, _) => (rejected(self.rejection), Err(error)),
            _ => (
                rejected(InitialPlanRejection::Unknown),
                Err(InitialPlanExecutionFailure::Usage),
            ),
        }
    }
}

fn require_not_cancelled(
    control: &dyn ModelOperationControl,
) -> Result<(), InitialPlanExecutionFailure> {
    if control.is_cancelled() {
        return Err(InitialPlanExecutionFailure::Provider(
            ModelProviderFailure::Cancelled,
        ));
    }
    Ok(())
}

fn elapsed_at(
    started: Instant,
    at: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, InitialPlanExecutionFailure> {
    phase_elapsed_timestamp(started, at).ok_or(InitialPlanExecutionFailure::Clock)
}

fn repair_request(
    request: &ModelProviderRequest,
    addition_selection: bool,
) -> Result<ModelProviderRequest, InitialPlanExecutionFailure> {
    let mut messages = request.messages().to_vec();
    let first = messages
        .first_mut()
        .ok_or(InitialPlanExecutionFailure::Binding)?;
    if first.role() != ModelMessageRole::System {
        return Err(InitialPlanExecutionFailure::Binding);
    }
    let repair = if addition_selection {
        ADD_SELECTION_REPAIR_INSTRUCTION
    } else {
        REPAIR_INSTRUCTION
    };
    let instruction = format!("{}{repair}", first.content());
    *first = ModelMessage::try_from_string(ModelMessageRole::System, instruction)
        .map_err(InitialPlanExecutionFailure::RepairMessage)?;
    ModelProviderRequest::new(
        request.profile().clone(),
        messages,
        request.structured_output().cloned(),
    )
    .map_err(InitialPlanExecutionFailure::RepairRequest)
}

/// A settled initial Plan and its actual request and usage observations.
#[derive(Debug)]
pub struct InitialPlanExecutionReport {
    /// Only the durable Ledger or a regular nonmutating disposition is exposed.
    pub outcome: InitialPlanExecutionOutcome,
    /// Exact encoded body digest and provider options.
    pub projection: ModelWireProjection,
    /// Conservative admission cost charged before provider I/O.
    pub budget: PhaseWireBudget,
    /// Actual complete terminal provider counters.
    pub usage: ModelProviderUsage,
}

/// Result after durable settlement, without any executable Run authority.
#[derive(Debug)]
pub enum InitialPlanExecutionOutcome {
    /// The model explicitly declined the offered Add or requested clarification.
    Halted(PlanPhaseHalt),
    /// A Ready initial Ledger was committed with this exact Plan observation.
    Materialized(StoredTaskLedger),
    /// One Core-owned offer needs more current evidence.
    NeedEvidence {
        /// Exact offered target, with no persisted step implied.
        offer: TaskStepId,
        /// Closed missing source or verification category.
        kind: PlanEvidenceKind,
    },
    /// The model reported that the selected scope is too large.
    TooLarge,
}

/// Content-free failure to execute and settle one initial Plan.
#[derive(Debug)]
pub enum InitialPlanExecutionFailure {
    /// The prepared, current, reserved or committed scope differs.
    Binding,
    /// The current Goal could not be loaded.
    GoalStore(GoalContractStoreFailure),
    /// The current published index could not be loaded.
    Index(KnowledgeIndexFailure),
    /// The exact supplied original could not be revalidated against the live file.
    Source(AgentSourceReadFailure),
    /// The pinned deterministic command catalog could not be reconstructed.
    Commands(CommandDiscoveryFailure),
    /// The fixed repair instruction could not form a bounded system message.
    RepairMessage(ModelMessageError),
    /// The repaired request could not be constructed with the unchanged profile and schema.
    RepairRequest(ModelProviderRequestError),
    /// The checked provider transport or stream failed.
    Provider(ModelProviderFailure),
    /// The actual wire options or sizes cannot fit.
    WireBudget(PhaseWireBudgetError),
    /// The wire costs could not be represented for durable accounting.
    WireCost(EditPhaseReservationFailure),
    /// Durable reservation or settlement failed.
    Reservation(InitialPlanReservationFailure),
    /// The actual usage was absent or incompatible with the admitted wire.
    Usage,
    /// The provider stopped before a complete reply.
    IncompleteReply,
    /// The bounded Plan proposal did not pass its exact decoder and ledger admission.
    Reply(PlanPhaseFailure),
    /// The owner timestamp and elapsed duration could not be represented.
    Clock,
}

impl fmt::Display for InitialPlanExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Binding => "initial plan binding changed",
            Self::GoalStore(_) => "initial plan could not load its current goal",
            Self::Index(_) => "initial plan could not load its current publication",
            Self::Source(_) => "initial plan could not revalidate its original source",
            Self::Commands(_) => "initial plan could not reconstruct its command catalog",
            Self::RepairMessage(_) | Self::RepairRequest(_) => {
                "initial plan repair request is invalid"
            }
            Self::Provider(_) => "initial plan transport failed",
            Self::WireBudget(_) => "initial plan wire capacity was rejected",
            Self::WireCost(_) => "initial plan wire cost cannot be represented",
            Self::Reservation(_) => "initial plan reservation or settlement failed",
            Self::Usage => "initial plan provider usage was missing or out of bounds",
            Self::IncompleteReply => "initial plan reply did not finish normally",
            Self::Reply(_) => "initial plan proposal was rejected",
            Self::Clock => "initial plan timing cannot be represented",
        })
    }
}

impl Error for InitialPlanExecutionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::GoalStore(error) => Some(error),
            Self::Index(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Commands(error) => Some(error),
            Self::RepairMessage(error) => Some(error),
            Self::RepairRequest(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::WireBudget(error) => Some(error),
            Self::WireCost(error) => Some(error),
            Self::Reservation(error) => Some(error),
            Self::Reply(error) => Some(error),
            Self::Binding | Self::Usage | Self::IncompleteReply | Self::Clock => None,
        }
    }
}

#[cfg(test)]
mod tests;
