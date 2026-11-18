//! One accepted strict-window material revision in the same original Run.
use crate::{
    AgentActionStore, AgentActionStoreFuture, AgentControllerControl, AgentSourceReadControl,
    AgentSourceReader, ApplyAgentPlanRevision, HttpCorrectionPlanApplied, HttpCorrectionPlanStore,
    HttpNoChangeReplanAnchor, HttpNoChangeReplanResumeState, HttpNoChangeWindowFailure,
    HttpNoChangeWindowMaterialProof, PreparedHttpNoChangeReplanV6Job, StoredTaskLedger,
    TaskLedgerStoreVersion, derive_http_no_change_window_material_plan,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunTimestamp, GoalContract, GoalOriginalCoverage,
    ProjectIdentity, PublishedIndex, RunEvent, RunEventSequence, TaskLedger,
    TaskStepAttemptOutcome, TaskStepStatus,
};
use std::{fmt, time::Instant};

/// Applies only an authenticated settled V6 proper subset; never activates or dispatches tools.
#[derive(Clone, Copy)]
pub struct ApplyHttpNoChangeWindowPlan<'a> {
    plans: &'a dyn HttpCorrectionPlanStore,
    actions: &'a dyn AgentActionStore,
    reader: &'a dyn AgentSourceReader,
}
impl fmt::Debug for ApplyHttpNoChangeWindowPlan<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApplyHttpNoChangeWindowPlan")
            .finish_non_exhaustive()
    }
}
impl<'a> ApplyHttpNoChangeWindowPlan<'a> {
    /// Reuses the existing plan store, normal safe reader and atomic action boundary.
    pub const fn new(
        plans: &'a dyn HttpCorrectionPlanStore,
        actions: &'a dyn AgentActionStore,
        reader: &'a dyn AgentSourceReader,
    ) -> Self {
        Self {
            plans,
            actions,
            reader,
        }
    }

    /// Retires exactly the failed first correction and its unstarted HTTP proof.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        current: &PublishedIndex,
        phase: &PreparedHttpNoChangeReplanV6Job,
        now: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpCorrectionPlanApplied, HttpNoChangeWindowFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpNoChangeWindowFailure as E;
        let started = Instant::now();
        checkpoint(run, now, control)?;
        let stored = StoredTaskLedger::new(ledger.clone(), version);
        let resume = self
            .plans
            .load_http_no_change_replan(project, goal, &stored, run, current, phase.profile(), now)
            .await
            .map_err(E::Origin)?
            .ok_or(E::Binding)?;
        let anchor =
            HttpNoChangeReplanAnchor::from_closed(resume.closed()).map_err(|_| E::Binding)?;
        if &anchor != phase.anchor() {
            return Err(E::Binding);
        }
        let HttpNoChangeReplanResumeState::Observed(settlement) = resume.state() else {
            return Err(E::NoWindow);
        };
        if settlement.job_digest() != phase.job_digest() {
            return Err(E::Binding);
        }
        let decision = settlement.decision().ok_or(E::NoWindow)?;
        phase.encode_proposal(decision).map_err(|_| E::Binding)?;
        let material = HttpNoChangeWindowMaterialProof::from_recorded(
            resume.closed().clone(),
            settlement.as_ref().clone(),
            phase.profile().clone(),
        )?;
        ensure_current(project, goal, run, ledger, version, current, &material)?;
        let first = crate::http_no_change_window_material::original_material(
            material.closed_origin(),
            ledger,
        )?;
        ensure_graph(ledger, &material, first.change().id(), first.proof().id())?;
        let plan =
            derive_http_no_change_window_material_plan(&material, first.change(), first.proof())?;
        crate::AdmitSourceTaskStep
            .execute(goal, plan.change())
            .map_err(|_| E::Binding)?;
        crate::AdmitSourceTaskStep
            .execute(goal, plan.proof())
            .map_err(|_| E::Binding)?;
        phase
            .revalidate_source(project, self.reader, control)
            .await
            .map_err(E::Source)?;
        let at = crate::model_wire::phase_elapsed_timestamp(started, now).ok_or(E::Binding)?;
        checkpoint(run, at, control)?;
        let result = HttpCorrectionPlanApplied {
            correction: plan.change().id(),
            proof: plan.proof().id(),
            version,
        };
        let reason = plan.reason().clone();
        let (retire, additions, event) = plan.into_parts();
        let actions = MaterialActions {
            inner: self.actions,
            proof: &material,
        };
        let next_version = ApplyAgentPlanRevision::new(&actions)
            .execute(
                project,
                version,
                run,
                ledger,
                retire.to_vec(),
                additions,
                reason,
                event,
                at,
                control,
            )
            .await
            .map_err(E::Replan)?;
        Ok(HttpCorrectionPlanApplied {
            version: next_version,
            ..result
        })
    }
}

fn ensure_current(
    project: &ProjectIdentity,
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &TaskLedger,
    version: TaskLedgerStoreVersion,
    current: &PublishedIndex,
    material: &HttpNoChangeWindowMaterialProof,
) -> Result<(), HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let closed = material.closed_origin();
    let a = closed.fact().owner();
    if run.state() != AgentControllerState::Replan
        || run.id() != a.run
        || project.worktree().id() != a.worktree
        || goal.reference() != a.goal
        || goal.original().map(|original| original.digest()) != Some(a.original_goal)
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::Binding)?
            .is_complete()
        || ledger.goal_contract() != a.goal
        || run.goal_contract() != a.goal
        || ledger.revision() != a.ledger
        || run.task_ledger_revision() != a.ledger
        || version != closed.closed_version()
        || run.last_event_sequence() != closed.closure().sequence()
        || run.current_snapshot_id() != a.snapshot
        || current.run().snapshot_id() != a.snapshot
        || current.run().id() != a.index
        || run.model_profile() != Some(a.profile)
        || ledger.pending_planning().is_some()
        || !current
            .publication()
            .graph()
            .files()
            .contains(material.selected_window().revision())
    {
        return Err(E::Binding);
    }
    Ok(())
}

/// Rejects any active external dependency or parent of the exact old two-step graph.
pub(crate) fn ensure_graph(
    ledger: &TaskLedger,
    material: &HttpNoChangeWindowMaterialProof,
    correction: a3_domain::TaskStepId,
    proof: a3_domain::TaskStepId,
) -> Result<(), HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let owner = material.closed_origin().fact().owner();
    let failed = ledger.step(correction).ok_or(E::Binding)?;
    let check = ledger.step(proof).ok_or(E::Binding)?;
    if !failed.is_active_plan_step()
        || failed.status() != TaskStepStatus::Failed
        || failed.attempts().last().is_none_or(|attempt| {
            attempt.run_id() != owner.run
                || attempt.number() != owner.attempt
                || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Failed { .. })
        })
        || !check.is_active_plan_step()
        || check.status() != TaskStepStatus::Pending
        || !check.attempts().is_empty()
        || ledger.steps().any(|step| {
            step.is_active_plan_step()
                && step.definition().id() != correction
                && step.definition().id() != proof
                && (step.status() != TaskStepStatus::Completed
                    || step
                        .definition()
                        .parent_step_id()
                        .is_some_and(|parent| parent == correction || parent == proof)
                    || step.definition().dependencies().iter().any(|dependency| {
                        dependency.prerequisite() == correction
                            || dependency.prerequisite() == proof
                    }))
        })
    {
        return Err(E::UnsupportedGraph);
    }
    Ok(())
}

/// Preserves cancellation, monotone time and the original Run hard budget at each boundary.
pub(crate) fn checkpoint(
    run: &AgentRun,
    now: AgentRunTimestamp,
    control: &dyn AgentControllerControl,
) -> Result<(), HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    if control.is_cancelled() {
        return Err(E::Cancelled);
    }
    if now < run.updated_at() {
        return Err(E::Binding);
    }
    if run
        .budget_exhaustion(now)
        .map_err(|_| E::Binding)?
        .is_some()
    {
        return Err(E::BudgetExhausted);
    }
    Ok(())
}

#[derive(Debug)]
struct MaterialActions<'a> {
    inner: &'a dyn AgentActionStore,
    proof: &'a HttpNoChangeWindowMaterialProof,
}
impl AgentActionStore for MaterialActions<'_> {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        self.inner.commit_http_no_change_window_plan_action(
            project,
            version,
            sequence,
            self.proof.closed_origin().fact().owner().index,
            ledger,
            run,
            event,
            self.proof,
        )
    }
}
