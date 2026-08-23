use crate::{
    DecodeExplorerAction, DeepMapReadControl, DeepMapReadFailure, DeepMapReadTimeout,
    DeepMapReadTools, ExplorerActionDecodeError, ExplorerModelControl, ExplorerModelFailure,
    ExplorerModelProvider, ExplorerModelRequest, ExplorerModelRequestPhase, ExplorerModelTimeout,
    ExplorerObservation, ExplorerRepairReason,
};
use a3_domain::{
    ExplorePlan, ExploreStep, ExplorerAction, ExplorerCheckpoint, ExplorerCheckpointError,
    ExplorerSearchAction, ModuleCardProposal, ProjectIdentity,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned future returned by the inbound R8 read-only explorer use case.
pub type DeepMapExplorerFuture<'a> = Pin<
    Box<dyn Future<Output = Result<DeepMapExplorerOutcome, DeepMapExplorerFailure>> + Send + 'a>,
>;

/// Terminal state of one bounded R8 invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapExplorerStatus {
    /// Every planner-produced step has a structurally valid evidence-bound proposal.
    Completed,
    /// Cooperative cancellation stopped before the next step was confirmed.
    Cancelled,
}

/// Checkpoint-bearing result returned on success and cooperative cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapExplorerOutcome {
    status: DeepMapExplorerStatus,
    checkpoint: ExplorerCheckpoint,
}

impl DeepMapExplorerOutcome {
    fn new(status: DeepMapExplorerStatus, checkpoint: ExplorerCheckpoint) -> Self {
        Self { status, checkpoint }
    }

    /// Returns whether the plan completed or was cooperatively cancelled.
    #[must_use]
    pub const fn status(&self) -> DeepMapExplorerStatus {
        self.status
    }

    /// Returns resumable state with only consecutively confirmed steps.
    #[must_use]
    pub const fn checkpoint(&self) -> &ExplorerCheckpoint {
        &self.checkpoint
    }

    /// Moves resumable state back to its caller-owned persistence boundary.
    #[must_use]
    pub fn into_checkpoint(self) -> ExplorerCheckpoint {
        self.checkpoint
    }
}

/// Read-only R8 orchestration over a deterministic plan and narrow capability ports.
#[derive(Debug, Clone, Copy)]
pub struct ExploreDeepMap<'a> {
    provider: &'a dyn ExplorerModelProvider,
    tools: &'a dyn DeepMapReadTools,
    model_timeout: ExplorerModelTimeout,
    read_timeout: DeepMapReadTimeout,
    decoder: DecodeExplorerAction,
}

impl<'a> ExploreDeepMap<'a> {
    /// Composes the version-one explorer with fixed local request deadlines.
    #[must_use]
    pub const fn version_one(
        provider: &'a dyn ExplorerModelProvider,
        tools: &'a dyn DeepMapReadTools,
    ) -> Self {
        Self {
            provider,
            tools,
            model_timeout: ExplorerModelTimeout::DEFAULT,
            read_timeout: DeepMapReadTimeout::DEFAULT,
            decoder: DecodeExplorerAction::version_one(),
        }
    }

    /// Explores only unconfirmed plan steps and performs at most one repair request in total.
    pub fn execute<'b, C>(
        &'b self,
        project: &'b ProjectIdentity,
        plan: &'b ExplorePlan,
        mut checkpoint: ExplorerCheckpoint,
        control: &'b C,
    ) -> DeepMapExplorerFuture<'b>
    where
        C: DeepMapReadControl + ExplorerModelControl,
        'a: 'b,
    {
        Box::pin(async move {
            checkpoint.validate_for(plan)?;
            let mut repair_used = false;

            while checkpoint.confirmed_step_count() < plan.steps().len() {
                if is_cancelled(control) {
                    return Ok(DeepMapExplorerOutcome::new(
                        DeepMapExplorerStatus::Cancelled,
                        checkpoint,
                    ));
                }
                let step_index = checkpoint.confirmed_step_count();
                let step = &plan.steps()[step_index];
                let mut observation = None;
                let mut tool_used = false;

                loop {
                    let action = match self
                        .request_authorized_action(
                            plan,
                            step,
                            observation.clone(),
                            tool_used,
                            &mut repair_used,
                            control,
                        )
                        .await
                    {
                        Ok(action) => action,
                        Err(DeepMapExplorerFailure::Model(ExplorerModelFailure::Cancelled)) => {
                            return Ok(DeepMapExplorerOutcome::new(
                                DeepMapExplorerStatus::Cancelled,
                                checkpoint,
                            ));
                        }
                        Err(error) => return Err(error),
                    };
                    if is_cancelled(control) {
                        return Ok(DeepMapExplorerOutcome::new(
                            DeepMapExplorerStatus::Cancelled,
                            checkpoint,
                        ));
                    }

                    match action {
                        ExplorerAction::Inspect(_) => {
                            let result = self
                                .tools
                                .inspect(
                                    project,
                                    plan.snapshot_id(),
                                    step.target(),
                                    self.read_timeout,
                                    control,
                                )
                                .await;
                            observation = Some(match result {
                                Ok(result) => result,
                                Err(DeepMapReadFailure::Cancelled) => {
                                    return Ok(DeepMapExplorerOutcome::new(
                                        DeepMapExplorerStatus::Cancelled,
                                        checkpoint,
                                    ));
                                }
                                Err(error) => return Err(error.into()),
                            });
                            tool_used = true;
                        }
                        ExplorerAction::Search(action) => {
                            let result = self
                                .tools
                                .search(
                                    project,
                                    plan.snapshot_id(),
                                    &action,
                                    self.read_timeout,
                                    control,
                                )
                                .await;
                            observation = Some(match result {
                                Ok(result) => result,
                                Err(DeepMapReadFailure::Cancelled) => {
                                    return Ok(DeepMapExplorerOutcome::new(
                                        DeepMapExplorerStatus::Cancelled,
                                        checkpoint,
                                    ));
                                }
                                Err(error) => return Err(error.into()),
                            });
                            tool_used = true;
                        }
                        ExplorerAction::Propose(proposal) => {
                            checkpoint.confirm_next(plan, proposal)?;
                            break;
                        }
                    }

                    if is_cancelled(control) {
                        return Ok(DeepMapExplorerOutcome::new(
                            DeepMapExplorerStatus::Cancelled,
                            checkpoint,
                        ));
                    }
                }
            }

            Ok(DeepMapExplorerOutcome::new(
                DeepMapExplorerStatus::Completed,
                checkpoint,
            ))
        })
    }

    async fn request_authorized_action<C>(
        &self,
        plan: &ExplorePlan,
        step: &ExploreStep,
        observation: Option<ExplorerObservation>,
        tool_used: bool,
        repair_used: &mut bool,
        control: &C,
    ) -> Result<ExplorerAction, DeepMapExplorerFailure>
    where
        C: DeepMapReadControl + ExplorerModelControl,
    {
        let primary = ExplorerModelRequest::for_step(
            plan,
            step,
            observation.clone(),
            ExplorerModelRequestPhase::Primary,
        );
        let output = self
            .provider
            .complete(&primary, self.model_timeout, control)
            .await?;
        if ExplorerModelControl::is_cancelled(control) {
            return Err(ExplorerModelFailure::Cancelled.into());
        }
        match self
            .decoder
            .decode(output.as_str())
            .map_err(ActionRejection::Structured)
            .and_then(|action| authorize(action, plan, step, observation.as_ref(), tool_used))
        {
            Ok(action) => Ok(action),
            Err(rejection) if !*repair_used => {
                *repair_used = true;
                let repair = ExplorerModelRequest::for_step(
                    plan,
                    step,
                    observation.clone(),
                    ExplorerModelRequestPhase::Repair(rejection.repair_reason()),
                );
                let repaired = self
                    .provider
                    .complete(&repair, self.model_timeout, control)
                    .await?;
                if ExplorerModelControl::is_cancelled(control) {
                    return Err(ExplorerModelFailure::Cancelled.into());
                }
                self.decoder
                    .decode(repaired.as_str())
                    .map_err(ActionRejection::Structured)
                    .and_then(|action| {
                        authorize(action, plan, step, observation.as_ref(), tool_used)
                    })
                    .map_err(|_| DeepMapExplorerFailure::InvalidModelOutput)
            }
            Err(_) => Err(DeepMapExplorerFailure::InvalidModelOutput),
        }
    }
}

fn authorize(
    action: ExplorerAction,
    plan: &ExplorePlan,
    step: &ExploreStep,
    observation: Option<&ExplorerObservation>,
    tool_used: bool,
) -> Result<ExplorerAction, ActionRejection> {
    match &action {
        ExplorerAction::Inspect(inspect) => {
            authorize_read_gain(
                inspect.expected_information_gain().basis_points(),
                step,
                tool_used,
            )?;
        }
        ExplorerAction::Search(search) => {
            authorize_search(search, step, tool_used)?;
        }
        ExplorerAction::Propose(proposal) => {
            authorize_proposal(proposal, plan, step, observation)?;
        }
    }
    Ok(action)
}

fn authorize_search(
    action: &ExplorerSearchAction,
    step: &ExploreStep,
    tool_used: bool,
) -> Result<(), ActionRejection> {
    authorize_read_gain(
        action.expected_information_gain().basis_points(),
        step,
        tool_used,
    )
}

fn authorize_read_gain(
    requested_basis_points: u16,
    step: &ExploreStep,
    tool_used: bool,
) -> Result<(), ActionRejection> {
    if tool_used || step.reserved_cost().tool_calls() == 0 {
        return Err(ActionRejection::UnauthorizedRead);
    }
    if requested_basis_points < 100
        || requested_basis_points > step.expected_information_gain().basis_points()
    {
        return Err(ActionRejection::UnauthorizedRead);
    }
    Ok(())
}

fn authorize_proposal(
    proposal: &ModuleCardProposal,
    plan: &ExplorePlan,
    step: &ExploreStep,
    observation: Option<&ExplorerObservation>,
) -> Result<(), ActionRejection> {
    if proposal.module_id() != step.module_id()
        || proposal.id()
            != a3_domain::ModuleCardId::for_module_fields_v1(
                step.module_id(),
                step.coverage_fields(),
            )
        || proposal.snapshot_id() != plan.snapshot_id()
        || proposal.schema_version() != plan.schema_version()
        || step
            .coverage_fields()
            .iter()
            .any(|field| !proposal.contains_field(*field))
    {
        return Err(ActionRejection::InvalidProposal);
    }
    let observed = observation.ok_or(ActionRejection::InvalidProposal)?;
    let observed_ids = observed
        .evidence_ids()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if proposal
        .evidence_ids()
        .iter()
        .any(|evidence_id| !observed_ids.contains(evidence_id))
    {
        return Err(ActionRejection::InvalidProposal);
    }
    Ok(())
}

fn is_cancelled<C>(control: &C) -> bool
where
    C: DeepMapReadControl + ExplorerModelControl,
{
    DeepMapReadControl::is_cancelled(control) || ExplorerModelControl::is_cancelled(control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionRejection {
    Structured(ExplorerActionDecodeError),
    UnauthorizedRead,
    InvalidProposal,
}

impl ActionRejection {
    const fn repair_reason(self) -> ExplorerRepairReason {
        match self {
            Self::Structured(_) => ExplorerRepairReason::InvalidStructuredOutput,
            Self::UnauthorizedRead => ExplorerRepairReason::UnauthorizedRead,
            Self::InvalidProposal => ExplorerRepairReason::InvalidProposal,
        }
    }
}

/// Stable R8 failure classification retaining typed boundary sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapExplorerFailure {
    /// Resume state did not match the immutable plan or confirmation failed.
    Checkpoint(ExplorerCheckpointError),
    /// Local structured model provider failed.
    Model(ExplorerModelFailure),
    /// Read-only inspection or search adapter failed.
    Read(DeepMapReadFailure),
    /// Invalid output remained after the sole repair or no repair remained.
    InvalidModelOutput,
}

impl fmt::Display for DeepMapExplorerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Checkpoint(_) => "explorer checkpoint validation failed",
            Self::Model(_) => "explorer model boundary failed",
            Self::Read(_) => "explorer read boundary failed",
            Self::InvalidModelOutput => {
                "explorer model output remained invalid after the bounded repair policy"
            }
        })
    }
}

impl Error for DeepMapExplorerFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Checkpoint(source) => Some(source),
            Self::Model(source) => Some(source),
            Self::Read(source) => Some(source),
            Self::InvalidModelOutput => None,
        }
    }
}

impl From<ExplorerCheckpointError> for DeepMapExplorerFailure {
    fn from(value: ExplorerCheckpointError) -> Self {
        Self::Checkpoint(value)
    }
}

impl From<ExplorerModelFailure> for DeepMapExplorerFailure {
    fn from(value: ExplorerModelFailure) -> Self {
        Self::Model(value)
    }
}

impl From<DeepMapReadFailure> for DeepMapExplorerFailure {
    fn from(value: DeepMapReadFailure) -> Self {
        Self::Read(value)
    }
}
