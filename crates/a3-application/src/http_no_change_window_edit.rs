//! Activate only the one accepted smaller-window correction through existing edges.
use crate::{
    AdvanceAgentController, AgentActionStore, AgentControllerControl, AgentControllerSignal,
    AgentSourcePage, AgentSourceReadControl, AgentSourceReader, AppendRunEvent, BoundEditSource,
    BoundEditSourceFailure, EditPhaseAnchor, HttpCorrectionPlanStore, HttpNoChangeWindowFailure,
    HttpNoChangeWindowMaterialPlan, HttpNoChangeWindowMaterialProof, PreparedEditPhase,
    RunJournalStore, StoredTaskLedger, TaskLedgerStoreVersion,
    derive_http_no_change_window_material_plan, replay_closed_no_change_edit_job,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, ContentHash, EvidenceRef,
    GoalContract, GoalContractReference, IndexRunId, ModelProfile, ProjectIdentity, PublishedIndex,
    SnapshotId, TaskLedger, TaskLedgerReplanCause, TaskLedgerRevision, TaskLedgerTimestamp,
    TaskLens, TaskStepAttemptNumber, TaskStepAttemptOutcome, TaskStepId, TaskStepStatus,
};
use std::{fmt, time::Instant};

/// Source/diagnosis descriptor returned only after the exact accepted B step reaches Execute.
/// It grants no source acquisition or delivery credit and does not imply an edit occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpNoChangeWindowEditDiagnostic {
    goal: GoalContractReference,
    run: AgentRunId,
    ledger: TaskLedgerRevision,
    snapshot: SnapshotId,
    index: IndexRunId,
    change: TaskStepId,
    attempt: TaskStepAttemptNumber,
    source: EvidenceRef,
    text_digest: ContentHash,
    http_summary: String,
}
impl HttpNoChangeWindowEditDiagnostic {
    /// Exact proper-subset full-line window selected by V6, not an enclosing definition.
    pub const fn required_source(&self) -> &EvidenceRef {
        &self.source
    }
    /// Original physical HTTP mismatch, separate from the byte-identical edit fact.
    pub fn http_summary(&self) -> &str {
        &self.http_summary
    }
    /// Bounded fact about the first edit, never success or verification evidence.
    pub const fn no_change_summary(&self) -> &str {
        "The previous correction changed zero source bytes and closed Failed; implementation and the complete HTTP proof remain open."
    }
    /// Reuses normal current Task Lens binding; caller must perform and journal a new real read.
    pub fn bind_source(
        &self,
        project: &ProjectIdentity,
        current: &PublishedIndex,
        lens: &TaskLens,
    ) -> Result<BoundEditSource, BoundEditSourceFailure> {
        if current.run().id() != self.index || current.run().snapshot_id() != self.snapshot {
            return Err(BoundEditSourceFailure::StalePublication);
        }
        BoundEditSource::for_file_span(project, current, lens, &self.source)
    }
    pub(crate) fn matches(&self, anchor: EditPhaseAnchor, phase: &PreparedEditPhase) -> bool {
        self.goal == anchor.goal()
            && self.run == anchor.run()
            && self.ledger == anchor.ledger()
            && self.snapshot == anchor.snapshot()
            && self.change == anchor.step()
            && self.attempt == anchor.attempt()
            && phase.targets_exact_span(&self.source)
            && phase
                .no_change_window()
                .is_some_and(|window| window.digest() == self.text_digest)
    }
}

/// Three ordinary controller edges followed by an exact current-window descriptor.
#[derive(Clone, Copy)]
pub struct ActivateHttpNoChangeWindowEdit<'a> {
    origins: &'a dyn HttpCorrectionPlanStore,
    journal: &'a dyn RunJournalStore,
    actions: &'a dyn AgentActionStore,
    reader: &'a dyn AgentSourceReader,
}
impl fmt::Debug for ActivateHttpNoChangeWindowEdit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActivateHttpNoChangeWindowEdit")
            .finish_non_exhaustive()
    }
}
impl<'a> ActivateHttpNoChangeWindowEdit<'a> {
    /// Reuses the existing finite controller, journal and current-index action ports.
    pub const fn new(
        origins: &'a dyn HttpCorrectionPlanStore,
        journal: &'a dyn RunJournalStore,
        actions: &'a dyn AgentActionStore,
        reader: &'a dyn AgentSourceReader,
    ) -> Self {
        Self {
            origins,
            journal,
            actions,
            reader,
        }
    }
    /// Resumes only N+1..N+5; does not dispatch a model, patch, or source acquisition.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: &mut TaskLedgerStoreVersion,
        current: &PublishedIndex,
        profile: &ModelProfile,
        base: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpNoChangeWindowEditDiagnostic, HttpNoChangeWindowFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpNoChangeWindowFailure as E;
        let started = Instant::now();
        for _ in 0..4 {
            let now = observed(started, base)?;
            crate::http_no_change_window_apply::checkpoint(run, now, control)?;
            let stored = StoredTaskLedger::new(ledger.clone(), *version);
            let origin = self
                .origins
                .load_applied_http_no_change_window_origin(
                    project, goal, &stored, run, current, profile, now,
                )
                .await
                .map_err(E::Origin)?;
            let material = origin.material();
            let closed = material.closed_origin();
            let owner = closed.fact().owner();
            let first = crate::http_no_change_window_material::original_material(closed, ledger)?;
            let plan = derive_http_no_change_window_material_plan(
                material,
                first.change(),
                first.proof(),
            )?;
            if origin.applied_event().id() != plan.event_id() || profile != material.profile() {
                return Err(E::Binding);
            }
            ensure_prefix(project, goal, run, &stored, current, material, &plan)?;
            let old = read_exact(
                self.reader,
                project,
                closed.fact().window().original(),
                control,
            )
            .await?;
            replay_closed_no_change_edit_job(closed.fact(), &old).map_err(|_| E::Binding)?;
            crate::http_no_change_replan::replay_http_no_change_replan_job(
                goal,
                ledger,
                profile,
                closed,
                &old,
                material.settlement(),
            )
            .map_err(|_| E::Binding)?;
            let selected =
                read_exact(self.reader, project, material.selected_window(), control).await?;
            if blake3::hash(selected.text().as_bytes()).as_bytes()
                != material.text_digest().as_bytes()
            {
                return Err(E::Binding);
            }
            crate::http_no_change_window_apply::checkpoint(run, observed(started, base)?, control)?;
            if run.state() == AgentControllerState::Execute {
                let change = ledger.step(plan.change().id()).ok_or(E::Binding)?;
                let [attempt] = change.attempts() else {
                    return Err(E::Binding);
                };
                let original = ledger
                    .step(closed.parent().anchor.step)
                    .ok_or(E::Binding)?
                    .definition();
                let summary = crate::http_correction_edit::mismatch_summary(
                    original.verification_spec().target(),
                    &closed.parent().diagnostic,
                )
                .map_err(|_| E::Binding)?;
                return Ok(HttpNoChangeWindowEditDiagnostic {
                    goal: goal.reference(),
                    run: run.id(),
                    ledger: ledger.revision(),
                    snapshot: owner.snapshot,
                    index: owner.index,
                    change: change.definition().id(),
                    attempt: attempt.number(),
                    source: material.selected_window().clone(),
                    text_digest: material.text_digest(),
                    http_summary: summary,
                });
            }
            let (signal, event, expected) = match run.state() {
                AgentControllerState::Replan => (
                    AgentControllerSignal::ReplanApplied,
                    plan.activation_events()[0],
                    AgentControllerState::Localize,
                ),
                AgentControllerState::Localize => (
                    AgentControllerSignal::LocalizationComplete,
                    plan.activation_events()[1],
                    AgentControllerState::Plan,
                ),
                AgentControllerState::Plan => (
                    AgentControllerSignal::PlanReady,
                    plan.activation_events()[2],
                    AgentControllerState::Execute,
                ),
                _ => return Err(E::Binding),
            };
            let now = observed(started, base)?;
            crate::http_no_change_window_apply::checkpoint(run, now, control)?;
            let mut next = run.clone();
            let mut next_ledger = ledger.clone();
            if signal == AgentControllerSignal::PlanReady {
                next_ledger
                    .start_step(
                        plan.change().id(),
                        run.id(),
                        TaskLedgerTimestamp::from_unix_millis(now.unix_millis())
                            .map_err(|_| E::Binding)?,
                    )
                    .map_err(|_| E::Binding)?;
            }
            let advance = AdvanceAgentController
                .execute(&mut next, signal, event, owner.snapshot, now, false)
                .map_err(E::Controller)?;
            if advance.state() != expected {
                return Err(E::BudgetExhausted);
            }
            crate::http_no_change_window_apply::checkpoint(run, observed(started, base)?, control)?;
            if expected == AgentControllerState::Execute {
                let next_version = self
                    .actions
                    .commit_current_index_ledger_action(
                        project,
                        *version,
                        run.last_event_sequence(),
                        owner.index,
                        &next_ledger,
                        &next,
                        advance.event(),
                    )
                    .await
                    .map_err(E::Commit)?;
                *ledger = next_ledger;
                *version = next_version;
            } else {
                AppendRunEvent::new(self.journal)
                    .execute(project, run.last_event_sequence(), &next, advance.event())
                    .await
                    .map_err(E::Journal)?;
            }
            *run = next;
            crate::http_no_change_window_apply::checkpoint(run, observed(started, base)?, control)?;
        }
        Err(E::Binding)
    }
}

fn ensure_prefix(
    project: &ProjectIdentity,
    goal: &GoalContract,
    run: &AgentRun,
    stored: &StoredTaskLedger,
    current: &PublishedIndex,
    material: &HttpNoChangeWindowMaterialProof,
    plan: &HttpNoChangeWindowMaterialPlan,
) -> Result<(), HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let ledger = stored.ledger();
    let version = stored.version();
    let closed = material.closed_origin();
    let owner = closed.fact().owner();
    let delta = run
        .last_event_sequence()
        .get()
        .checked_sub(closed.closure().sequence().get())
        .ok_or(E::Binding)?;
    let execute = run.state() == AgentControllerState::Execute;
    let expected_version = closed
        .closed_version()
        .get()
        .checked_add(if execute { 2 } else { 1 })
        .ok_or(E::Binding)?;
    let revision = owner.ledger.get().checked_add(1).ok_or(E::Binding)?;
    if !matches!(
        (run.state(), delta),
        (AgentControllerState::Replan, 1)
            | (AgentControllerState::Localize, 2)
            | (AgentControllerState::Plan, 3)
            | (AgentControllerState::Execute, 4 | 5)
    ) || version.get() != expected_version
        || ledger.revision().get() != revision
        || run.task_ledger_revision() != ledger.revision()
        || run.id() != owner.run
        || project.worktree().id() != owner.worktree
        || goal.reference() != owner.goal
        || goal.original().map(|v| v.digest()) != Some(owner.original_goal)
        || ledger.goal_contract() != owner.goal
        || run.goal_contract() != owner.goal
        || run.current_snapshot_id() != owner.snapshot
        || current.run().snapshot_id() != owner.snapshot
        || current.run().id() != owner.index
        || run.model_profile() != Some(owner.profile)
        || !material.profile().executable_actions_enabled()
        || ledger.pending_planning().is_some()
        || ledger.step(plan.change().id()).is_none_or(|step| {
            !step.is_active_plan_step()
                || step.definition() != plan.change()
                || (if execute {
                    step.status() != TaskStepStatus::InProgress
                        || step.attempts().len() != 1
                        || step.attempts().last().is_none_or(|attempt| {
                            attempt.run_id() != owner.run
                                || attempt.number() != TaskStepAttemptNumber::FIRST
                                || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
                        })
                } else {
                    step.status() != TaskStepStatus::Ready || !step.attempts().is_empty()
                })
        })
        || ledger.step(plan.proof().id()).is_none_or(|step| {
            !step.is_active_plan_step()
                || step.definition() != plan.proof()
                || step.status() != TaskStepStatus::Pending
                || !step.attempts().is_empty()
        })
        || plan.retired_steps().iter().any(|id| {
            ledger
                .step(*id)
                .is_none_or(|step| step.is_active_plan_step())
        })
    {
        return Err(E::Binding);
    }
    let replan = ledger.replans().last().ok_or(E::Binding)?;
    let mut retired = plan.retired_steps().to_vec();
    retired.sort_unstable();
    let mut added = vec![plan.change().id(), plan.proof().id()];
    added.sort_unstable();
    if replan.cause() != TaskLedgerReplanCause::ErrorCorrection
        || replan.previous_revision() != owner.ledger
        || replan.revision() != ledger.revision()
        || replan.retired_step_ids() != retired
        || replan.added_step_ids() != added
    {
        return Err(E::Binding);
    }
    Ok(())
}

async fn read_exact(
    reader: &dyn AgentSourceReader,
    project: &ProjectIdentity,
    required: &EvidenceRef,
    control: &dyn AgentSourceReadControl,
) -> Result<AgentSourcePage, HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let inspection =
        crate::bound_edit_source::inspection_for_span(required).map_err(|_| E::Binding)?;
    let page = reader
        .read_page(project, required.revision(), &inspection, control)
        .await
        .map_err(E::Source)?;
    crate::bound_edit_source::validate_source_page(&page, &inspection, Some(required.range()))
        .map_err(|_| E::Binding)?;
    if &EvidenceRef::new(page.revision().clone(), page.range()) != required {
        return Err(E::Binding);
    }
    Ok(page)
}
fn observed(
    started: Instant,
    base: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, HttpNoChangeWindowFailure> {
    crate::model_wire::phase_elapsed_timestamp(started, base)
        .ok_or(HttpNoChangeWindowFailure::Binding)
}
