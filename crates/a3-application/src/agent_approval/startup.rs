//! Exact volatile approval projection for one ordered startup envelope.

use super::*;
use a3_domain::LOOPBACK_DEFAULT_PORT;

impl AgentApprovalBuffer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_startup_presentation(
        &self,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &ApprovalRequest,
        reason: PolicyDecisionReason,
        prepared: &PreparedLoopbackStartupCoverage,
        stage: usize,
        previous: Option<(AgentApprovalRevision, ApprovalId)>,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
        use AgentApprovalSinkFailure::AnchorMismatch;

        let start = prepared.start();
        if context.run_id() != start.run_id()
            || context.snapshot_id() != prepared.snapshot()
            || start.plan_binding() != ProcessPlanBinding::Validated(context.step_id())
        {
            return Err(AnchorMismatch);
        }
        let actions = prepared.policy_actions();
        let policy_action = actions.get(stage).ok_or(AnchorMismatch)?;
        let action = present_action(prepared, stage, policy_action)?;
        validate_request(project, context, request, reason, policy_action)?;
        let mut state = lock_recovering_poison(&self.state);
        state.ensure_active_project(project)?;
        match (stage, previous) {
            (0, None) => {
                if state.presentation.as_ref().is_some_and(|current| {
                    current.context() == context
                        && current.state() != AgentApprovalPresentationState::Denied
                }) {
                    return Err(AnchorMismatch);
                }
            }
            (1.., Some((revision, approval))) => {
                let predecessor_index = stage.checked_sub(1).ok_or(AnchorMismatch)?;
                let predecessor = actions.get(predecessor_index).ok_or(AnchorMismatch)?;
                let preceding_action = present_action(prepared, predecessor_index, predecessor)?;
                let current = state.presentation.as_ref().ok_or(AnchorMismatch)?;
                if current.context() != context
                    || current.revision() != revision
                    || current.state() != AgentApprovalPresentationState::Granted(approval)
                    || current.request_id() == request.id()
                    || current.action_fingerprint() != predecessor.fingerprint()
                    || current.scope_digest() != predecessor.scope_digest()
                    || current.action() != &preceding_action
                {
                    return Err(AnchorMismatch);
                }
            }
            _ => return Err(AnchorMismatch),
        }
        state.record_presentation(context, request, reason, action)
    }
}

fn present_action(
    prepared: &PreparedLoopbackStartupCoverage,
    stage: usize,
    action: &PolicyAction,
) -> Result<AgentApprovalAction, AgentApprovalSinkFailure> {
    use AgentApprovalSinkFailure::AnchorMismatch;

    match action {
        PolicyAction::LoopbackStartup { phase, .. } => {
            let process = prepared.process(*phase).map(|spec| {
                AgentApprovalProcess::from_spec(AgentProcessInspectionKind::Command, spec)
            });
            if process.is_none() != (*phase == LoopbackStartupPhase::ReserveEndpoint) {
                return Err(AnchorMismatch);
            }
            let port = if *phase == LoopbackStartupPhase::Default {
                NonZeroU16::new(LOOPBACK_DEFAULT_PORT).ok_or(AnchorMismatch)?
            } else {
                prepared.recipe().port()
            };
            Ok(AgentApprovalAction::LoopbackStartup {
                phase: *phase,
                process,
                port,
            })
        }
        PolicyAction::LoopbackHttpProbe { method, .. } => {
            let count = prepared.recipe().checks().len();
            let default_start = stage == count + 4;
            let index = if default_start {
                usize::from(
                    prepared
                        .recipe()
                        .default_start_check_index()
                        .ok_or(AnchorMismatch)?,
                )
            } else {
                stage
                    .checked_sub(3)
                    .filter(|index| *index < count)
                    .ok_or(AnchorMismatch)?
            };
            let check = prepared
                .recipe()
                .checks()
                .get(index)
                .ok_or(AnchorMismatch)?;
            let port = if default_start {
                NonZeroU16::new(LOOPBACK_DEFAULT_PORT).ok_or(AnchorMismatch)?
            } else {
                prepared.recipe().port()
            };
            Ok(AgentApprovalAction::LoopbackProbe {
                method: *method,
                url: format!("http://127.0.0.1:{port}{}", check.probe().path()),
            })
        }
        _ => Err(AnchorMismatch),
    }
}
