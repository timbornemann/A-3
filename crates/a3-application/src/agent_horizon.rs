//! Core-bound automatic horizon handoff; this slice performs no provider I/O.
use crate::{
    AdmitAgentGoalExecution, AdmitSourceTaskStep, AdvanceAgentController, AgentActionStore,
    AgentActionStoreFailure, AgentControllerControl, AgentControllerSignal,
    ContinueVerifiedAgentPlan, ContinueVerifiedAgentPlanOutcome, RunEventPageLimit,
    RunJournalStore, RunJournalStoreFailure, TaskLedgerStoreVersion,
};
use a3_domain::{
    AcceptanceCriterionId, AcceptanceCriterionRequirement, AgentBudgetDimension,
    AgentControllerState, AgentRun, AgentRunTimestamp, GoalContract, PendingPlanning,
    PendingPlanningStage, ProjectIdentity, RunEvent, RunEventCode, RunEventId, RunEventKind,
    RunEventOutcome, RunEventPayload, RunEventSequence, SnapshotId, TaskLedger,
    TaskLedgerReplanCause, TaskLedgerTimestamp, TaskReplanReason, TaskStepDefinition, TaskStepId,
    TaskStepStatus, VerificationTarget,
};
use std::error::Error;
use std::fmt;

/// Integrity and dispatch disposition; stale/terminal audit markers are not executable jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPlanningPairDisposition {
    /// Exact live checkpoint under its original source snapshot.
    Live,
    /// Sources/evidence changed; only audit or explicit Cancel is allowed in this slice.
    StaleInert,
    /// Failed or cancelled owner retains history without permission to resume.
    TerminalInert,
}

/// Checks a stored pair and its exact original request, never its latest displayed page.
/// This is safe after both writes or for audit; readers inside Ledger-first TXs must
/// validate intrinsic Ledger shape only. It does not replace a current-index check.
pub fn validate_pending_planning_pair(
    ledger: &TaskLedger,
    run: &AgentRun,
    request: &RunEvent,
) -> Result<PendingPlanningPairDisposition, AgentHorizonFailure> {
    let p = ledger
        .pending_planning()
        .ok_or(AgentHorizonFailure::NoPending)?;
    p.validate_for_ledger(ledger)?;
    if p.run_id() != run.id()
        || p.goal_ref() != run.goal_contract()
        || run.task_ledger_revision() != ledger.revision()
        || request.run_id() != run.id()
        || request.id() != p.requested_event_id()
        || request.sequence() != p.requested_sequence()
        || request.snapshot_id() != p.snapshot_id()
        || request.sequence().get() > run.last_event_sequence().get()
        || request.payload().code() != RunEventCode::ControllerDecision
        || request.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || !matches!(
            request.kind(),
            RunEventKind::StateTransition {
                from: AgentControllerState::Verify,
                to: AgentControllerState::Plan
            }
        )
    {
        return Err(AgentHorizonFailure::InvalidPair);
    }
    if matches!(
        run.state(),
        AgentControllerState::Failed | AgentControllerState::Cancelled
    ) {
        return Ok(PendingPlanningPairDisposition::TerminalInert);
    }
    if run.state() != AgentControllerState::Plan {
        return Err(AgentHorizonFailure::InvalidPair);
    }
    if run.current_snapshot_id() != p.snapshot_id() || !live_step_shape(ledger, p) {
        return Ok(PendingPlanningPairDisposition::StaleInert);
    }
    Ok(PendingPlanningPairDisposition::Live)
}

/// Loads the owner directly from the Ledger and its exact journal position, not an
/// invented InProgress attempt or a bounded presentation-history tail.
pub async fn load_pending_planning_pair(
    project: &ProjectIdentity,
    ledger: &TaskLedger,
    journal: &dyn RunJournalStore,
) -> Result<(AgentRun, RunEvent, PendingPlanningPairDisposition), AgentHorizonFailure> {
    let pending = ledger
        .pending_planning()
        .ok_or(AgentHorizonFailure::NoPending)?;
    let run = journal
        .load_agent_run(project, pending.run_id())
        .await?
        .ok_or(AgentHorizonFailure::InvalidPair)?;
    let after = RunEventSequence::new(pending.requested_sequence().get() - 1)
        .map_err(|_| AgentHorizonFailure::InvalidPair)?;
    let page = journal
        .load_run_events(
            project,
            pending.run_id(),
            Some(after),
            RunEventPageLimit::new(1).map_err(|_| AgentHorizonFailure::InvalidPair)?,
        )
        .await?;
    let event = page
        .events()
        .first()
        .ok_or(AgentHorizonFailure::InvalidPair)?
        .clone();
    let disposition = validate_pending_planning_pair(ledger, &run, &event)?;
    Ok((run, event, disposition))
}

fn live_step_shape(ledger: &TaskLedger, p: &PendingPlanning) -> bool {
    ledger
        .steps()
        .filter(|step| step.is_active_plan_step())
        .all(|step| match p.stage() {
            PendingPlanningStage::NeedsPlan => step.status() == TaskStepStatus::Completed,
            PendingPlanningStage::PlanAppended { added_step_ids, .. } => {
                if added_step_ids.contains(&step.definition().id()) {
                    matches!(
                        step.status(),
                        TaskStepStatus::Ready | TaskStepStatus::Pending
                    )
                } else {
                    step.status() == TaskStepStatus::Completed
                }
            }
        })
}

/// Guards the coupled projection edge. Actual request-event integrity is checked after
/// both writes with validate_pending_planning_pair; no provider reservations are claimed.
pub fn validate_pending_planning_transition(
    old_ledger: &TaskLedger,
    old_run: &AgentRun,
    ledger: &TaskLedger,
    run: &AgentRun,
    event: &RunEvent,
) -> Result<(), AgentHorizonFailure> {
    let (old, new) = (old_ledger.pending_planning(), ledger.pending_planning());
    if old.is_none() && new.is_none() {
        if ledger
            .replans()
            .iter()
            .skip(old_ledger.replans().len())
            .any(|r| r.cause() == TaskLedgerReplanCause::HorizonExtension)
        {
            return Err(AgentHorizonFailure::InvalidPair);
        }
        return Ok(());
    }
    if old_run.id() != run.id()
        || old_run.goal_contract() != run.goal_contract()
        || old_run.budget() != run.budget()
        || old_run.usage() != run.usage()
        || old_run.created_at() != run.created_at()
        || old_run.model_profile() != run.model_profile()
        || event.run_id() != run.id()
        || event.sequence() != run.last_event_sequence()
        || old_run.last_event_sequence().get().checked_add(1) != Some(event.sequence().get())
    {
        return Err(AgentHorizonFailure::InvalidPair);
    }
    match (old, new) {
        (None, Some(p)) => {
            if old_run.state() != AgentControllerState::Verify
                || !matches!(p.stage(), PendingPlanningStage::NeedsPlan)
                || p.run_id() != run.id()
                || p.goal_ref() != run.goal_contract()
                || p.base_revision() != old_ledger.revision()
                || ledger.revision() != old_ledger.revision()
                || p.snapshot_id() != old_run.current_snapshot_id()
                || p.requested_event_id() != event.id()
                || p.requested_sequence() != event.sequence()
                || !old_ledger
                    .steps()
                    .filter(|s| s.is_active_plan_step())
                    .all(|s| s.status() == TaskStepStatus::Completed)
            {
                return Err(AgentHorizonFailure::InvalidPair);
            }
            if validate_pending_planning_pair(ledger, run, event)?
                != PendingPlanningPairDisposition::Live
            {
                return Err(AgentHorizonFailure::InvalidPair);
            }
        }
        (Some(a), Some(b)) if a == b => {
            // Recovery diagnostics, stale marking, Failed and Cancelled keep the audit marker.
            // No ordinary PlanReady transition may execute while it remains present.
            if !matches!(
                run.state(),
                AgentControllerState::Plan
                    | AgentControllerState::Failed
                    | AgentControllerState::Cancelled
            ) || ledger.revision() != old_ledger.revision()
            {
                return Err(AgentHorizonFailure::InvalidPair);
            }
        }
        (Some(a), Some(b)) => {
            if !matches!(a.stage(), PendingPlanningStage::NeedsPlan)
                || !matches!(b.stage(), PendingPlanningStage::PlanAppended { .. })
                || a.run_id() != b.run_id()
                || a.goal_ref() != b.goal_ref()
                || a.base_revision() != b.base_revision()
                || a.snapshot_id() != b.snapshot_id()
                || a.requested_event_id() != b.requested_event_id()
                || a.requested_sequence() != b.requested_sequence()
                || old_run.state() != AgentControllerState::Plan
                || run.state() != AgentControllerState::Plan
                || old_run.current_snapshot_id() != a.snapshot_id()
                || run.current_snapshot_id() != a.snapshot_id()
                || !live_step_shape(old_ledger, a)
                || !matches!(event.kind(),RunEventKind::LedgerUpdated {from,to} if from==old_ledger.revision() && to==ledger.revision())
            {
                return Err(AgentHorizonFailure::InvalidPair);
            }
            b.validate_for_ledger(ledger)?;
            if !live_step_shape(ledger, b) {
                return Err(AgentHorizonFailure::InvalidPair);
            }
        }
        (Some(a), None) => {
            let PendingPlanningStage::PlanAppended { added_step_ids, .. } = a.stage() else {
                return Err(AgentHorizonFailure::InvalidPair);
            };
            let started = ledger
                .steps()
                .filter(|s| s.status() == TaskStepStatus::InProgress)
                .collect::<Vec<_>>();
            if old_run.state() != AgentControllerState::Plan
                || run.state() != AgentControllerState::Execute
                || old_run.current_snapshot_id() != a.snapshot_id()
                || run.current_snapshot_id() != a.snapshot_id()
                || !live_step_shape(old_ledger, a)
                || ledger.revision() != old_ledger.revision()
                || started.len() != 1
                || !added_step_ids.contains(&started[0].definition().id())
                || started[0]
                    .attempts()
                    .last()
                    .is_none_or(|attempt| attempt.run_id() != run.id())
                || !matches!(
                    event.kind(),
                    RunEventKind::StateTransition {
                        from: AgentControllerState::Plan,
                        to: AgentControllerState::Execute
                    }
                )
            {
                return Err(AgentHorizonFailure::InvalidPair);
            }
        }
        _ => return Err(AgentHorizonFailure::InvalidPair),
    }
    Ok(())
}

/// Source-only continuation outcome; ReadyForAcceptance still requires the sole verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePlanContinuationOutcome {
    /// An existing checked continuation or acceptance request, never a Done assertion.
    Continued(ContinueVerifiedAgentPlanOutcome),
    /// The old horizon ended and the original Core job is now durably checkpointed.
    PlanningRequested {
        /// Current Ledger CAS version; the plan revision has not changed yet.
        ledger_version: TaskLedgerStoreVersion,
        /// Exact Verify-to-Plan event, also available through its durable journal position.
        request_event: RunEvent,
    },
}

/// Candidate Goal-bound continuation through the existing controller and atomic store.
#[derive(Debug, Clone, Copy)]
pub struct ContinueSourceAgentPlan<'a> {
    store: &'a dyn AgentActionStore,
}
impl<'a> ContinueSourceAgentPlan<'a> {
    /// Uses the existing coupled Ledger/run transaction boundary.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }
    /// Automatically requests the next horizon only for structurally unplanned Must proofs.
    /// Completed Proves metadata never establishes fresh global evidence here.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected: TaskLedgerStoreVersion,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        current_snapshot: SnapshotId,
        event_id: RunEventId,
        at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<SourcePlanContinuationOutcome, AgentHorizonFailure> {
        source_preflight(goal, run, ledger, current_snapshot, at, control)?;
        if ledger.pending_planning().is_some() {
            return Err(AgentHorizonFailure::InvalidState);
        }
        let completed = ledger
            .steps()
            .filter(|step| step.is_active_plan_step())
            .all(|step| step.status() == TaskStepStatus::Completed);
        let proofs_unplanned = !missing_proves(goal, ledger).is_empty();
        // Pure acceptance consumes no further model/action/repair allowance.
        // Every new horizon or step still needs the unchanged hard work budget.
        if !completed || proofs_unplanned {
            require_remaining_work_budget(run, at)?;
        }
        if !completed || !proofs_unplanned {
            let outcome = ContinueVerifiedAgentPlan::new(self.store)
                .execute(project, expected, run, ledger, event_id, at, control)
                .await
                .map_err(|error| match error {
                    crate::ContinueVerifiedAgentPlanError::Storage(error) => {
                        AgentHorizonFailure::Storage(error)
                    }
                    crate::ContinueVerifiedAgentPlanError::Cancelled => {
                        AgentHorizonFailure::Cancelled
                    }
                    crate::ContinueVerifiedAgentPlanError::Ledger(error) => {
                        AgentHorizonFailure::Ledger(error)
                    }
                    _ => AgentHorizonFailure::InvalidState,
                })?;
            return Ok(SourcePlanContinuationOutcome::Continued(outcome));
        }
        if run.state() != AgentControllerState::Verify {
            return Err(AgentHorizonFailure::InvalidState);
        }
        let mut next_run = run.clone();
        let advance = AdvanceAgentController
            .execute(
                &mut next_run,
                AgentControllerSignal::VerificationNeedsPlanning,
                event_id,
                current_snapshot,
                at,
                false,
            )
            .map_err(|_| AgentHorizonFailure::InvalidState)?;
        if next_run.state() != AgentControllerState::Plan {
            return Err(AgentHorizonFailure::InvalidState);
        }
        let event = advance.event().clone();
        let pending = PendingPlanning::new(
            run.id(),
            goal.reference(),
            ledger.revision(),
            current_snapshot,
            event.id(),
            event.sequence(),
        )?;
        let mut next_ledger = ledger.clone();
        next_ledger.request_horizon_planning(pending, timestamp(at)?)?;
        validate_pending_planning_transition(ledger, run, &next_ledger, &next_run, &event)?;
        let version = self
            .store
            .commit_ledger_action(
                project,
                expected,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                &event,
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(SourcePlanContinuationOutcome::PlanningRequested {
            ledger_version: version,
            request_event: event,
        })
    }
}

/// Appends a deliberately supplied Core-validated plan; no planner/provider port is introduced.
#[derive(Debug, Clone, Copy)]
pub struct AppendAgentHorizonPlan<'a> {
    store: &'a dyn AgentActionStore,
}
impl<'a> AppendAgentHorizonPlan<'a> {
    /// Uses the same existing atomic transaction as checkpointing and activation.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }
    /// Installs one through three genuinely changed obligations and preserves all prior steps.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected: TaskLedgerStoreVersion,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        current_snapshot: SnapshotId,
        request: &RunEvent,
        additions: Vec<TaskStepDefinition>,
        event_id: RunEventId,
        at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<TaskLedgerStoreVersion, AgentHorizonFailure> {
        source_preflight(goal, run, ledger, current_snapshot, at, control)?;
        require_remaining_work_budget(run, at)?;
        require_live(ledger, run, request, current_snapshot)?;
        if !matches!(
            ledger
                .pending_planning()
                .ok_or(AgentHorizonFailure::NoPending)?
                .stage(),
            PendingPlanningStage::NeedsPlan
        ) {
            return Err(AgentHorizonFailure::InvalidState);
        }
        validate_progress(goal, ledger, &additions)?;
        let mut next_ledger = ledger.clone();
        let revision = next_ledger.append_horizon(
            additions,
            TaskReplanReason::try_from_string(
                "Core-bound continuation of structurally unplanned original proofs".to_owned(),
            )
            .map_err(|_| AgentHorizonFailure::InvalidState)?,
            timestamp(at)?,
        )?;
        let mut next_run = run.clone();
        let event = next_run
            .record_ledger_update(
                event_id,
                revision,
                RunEventPayload::new(
                    RunEventCode::ControllerDecision,
                    Some(RunEventOutcome::Succeeded),
                    None,
                ),
                current_snapshot,
                at,
            )
            .map_err(|_| AgentHorizonFailure::InvalidState)?;
        validate_pending_planning_transition(ledger, run, &next_ledger, &next_run, &event)?;
        let version = self
            .store
            .commit_ledger_action(
                project,
                expected,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                &event,
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(version)
    }
}

/// Activates an already appended actual step and consumes its marker in the same CAS.
#[derive(Debug, Clone, Copy)]
pub struct ActivateAgentHorizonPlan<'a> {
    store: &'a dyn AgentActionStore,
}
impl<'a> ActivateAgentHorizonPlan<'a> {
    /// Uses the existing atomic Ledger/run persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn AgentActionStore) -> Self {
        Self { store }
    }
    /// Restarts from persisted definitions without invoking or repeating a planning model.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        expected: TaskLedgerStoreVersion,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        current_snapshot: SnapshotId,
        request: &RunEvent,
        step_id: TaskStepId,
        event_id: RunEventId,
        at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<TaskLedgerStoreVersion, AgentHorizonFailure> {
        source_preflight(goal, run, ledger, current_snapshot, at, control)?;
        require_remaining_work_budget(run, at)?;
        require_live(ledger, run, request, current_snapshot)?;
        let mut next_ledger = ledger.clone();
        next_ledger.start_appended_horizon(step_id, run.id(), timestamp(at)?)?;
        let mut next_run = run.clone();
        let advance = AdvanceAgentController
            .execute(
                &mut next_run,
                AgentControllerSignal::PlanReady,
                event_id,
                current_snapshot,
                at,
                false,
            )
            .map_err(|_| AgentHorizonFailure::InvalidState)?;
        let event = advance.event().clone();
        validate_pending_planning_transition(ledger, run, &next_ledger, &next_run, &event)?;
        let version = self
            .store
            .commit_ledger_action(
                project,
                expected,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                &event,
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(version)
    }
}

fn timestamp(at: AgentRunTimestamp) -> Result<TaskLedgerTimestamp, AgentHorizonFailure> {
    TaskLedgerTimestamp::from_unix_millis(at.unix_millis())
        .map_err(|_| AgentHorizonFailure::InvalidState)
}
fn source_preflight(
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &TaskLedger,
    current: SnapshotId,
    at: AgentRunTimestamp,
    control: &dyn AgentControllerControl,
) -> Result<(), AgentHorizonFailure> {
    if control.is_cancelled() {
        return Err(AgentHorizonFailure::Cancelled);
    }
    if goal.original().is_none() {
        return Err(AgentHorizonFailure::MissingOriginal);
    }
    if run.state().is_terminal()
        || goal.reference() != run.goal_contract()
        || ledger.goal_contract() != run.goal_contract()
        || ledger.revision() != run.task_ledger_revision()
    {
        return Err(AgentHorizonFailure::InvalidState);
    }
    if current != run.current_snapshot_id() {
        return Err(AgentHorizonFailure::SnapshotChanged);
    }
    if run
        .budget_exhaustion(at)
        .map_err(|_| AgentHorizonFailure::InvalidState)?
        .is_some_and(|exhaustion| exhaustion.dimension() == AgentBudgetDimension::Time)
    {
        return Err(AgentHorizonFailure::BudgetExhausted);
    }
    // Equality can be the last admissible charge; an actual overrun is never
    // excused by deterministic completion. Check all dimensions: an earlier
    // exact ceiling must not hide a later overrun in exhaustion priority order.
    let budget = run.budget();
    let usage = run.usage();
    if u64::from(usage.turn_count()) > u64::from(budget.turn_limit().get())
        || usage.prompt_tokens() > budget.prompt_token_limit().get()
        || usage.output_tokens() > budget.output_token_limit().get()
        || u64::from(usage.action_count()) > u64::from(budget.action_limit().get())
        || u64::from(usage.repair_count()) > u64::from(budget.repair_limit().get())
    {
        return Err(AgentHorizonFailure::BudgetExhausted);
    }
    AdmitAgentGoalExecution
        .execute_task(goal, ledger)
        .map_err(|_| AgentHorizonFailure::InvalidState)?;
    Ok(())
}
/// A completed proof horizon may request deterministic acceptance at its exact
/// model ceilings. Any further planning/execution still obeys every hard limit.
fn require_remaining_work_budget(
    run: &AgentRun,
    at: AgentRunTimestamp,
) -> Result<(), AgentHorizonFailure> {
    if run
        .budget_exhaustion(at)
        .map_err(|_| AgentHorizonFailure::InvalidState)?
        .is_some()
    {
        return Err(AgentHorizonFailure::BudgetExhausted);
    }
    Ok(())
}

fn require_live(
    ledger: &TaskLedger,
    run: &AgentRun,
    request: &RunEvent,
    current: SnapshotId,
) -> Result<(), AgentHorizonFailure> {
    if validate_pending_planning_pair(ledger, run, request)? != PendingPlanningPairDisposition::Live
        || ledger
            .pending_planning()
            .is_none_or(|p| p.snapshot_id() != current)
    {
        return Err(AgentHorizonFailure::SnapshotChanged);
    }
    Ok(())
}
fn missing_proves(goal: &GoalContract, ledger: &TaskLedger) -> Vec<AcceptanceCriterionId> {
    goal.draft()
        .acceptance_criteria()
        .iter()
        .filter(|c| c.requirement() == AcceptanceCriterionRequirement::Must)
        .filter(|c| {
            !ledger
                .steps()
                .filter(|s| s.is_active_plan_step())
                .any(|s| s.definition().acceptance_criteria().contains(&c.id()))
        })
        .map(|c| c.id())
        .collect()
}
fn material_eq(a: &TaskStepDefinition, b: &TaskStepDefinition) -> bool {
    a.addressed_criteria() == b.addressed_criteria()
        && a.acceptance_criteria() == b.acceptance_criteria()
        && a.verification_spec().target() == b.verification_spec().target()
        && a.verification_spec().python_integer_translation()
            == b.verification_spec().python_integer_translation()
}
fn concrete_target(step: &TaskStepDefinition) -> bool {
    match step.verification_spec().target() {
        VerificationTarget::DiffInvariant(diff) => !diff.paths().is_empty(),
        VerificationTarget::Test { .. } | VerificationTarget::LoopbackHttp(_) => true,
        _ => false,
    }
}
fn validate_progress(
    goal: &GoalContract,
    ledger: &TaskLedger,
    additions: &[TaskStepDefinition],
) -> Result<(), AgentHorizonFailure> {
    if additions.is_empty() || additions.len() > 3 {
        return Err(AgentHorizonFailure::NoMaterialProgress);
    }
    let open = missing_proves(goal, ledger);
    if open.is_empty() {
        return Err(AgentHorizonFailure::NoUnplannedProof);
    }
    for (index, step) in additions.iter().enumerate() {
        AdmitSourceTaskStep
            .execute(goal, step)
            .map_err(|_| AgentHorizonFailure::InvalidState)?;
        if step
            .addressed_criteria()
            .iter()
            .chain(step.acceptance_criteria())
            .any(|id| !open.contains(id))
            || (step.acceptance_criteria().is_empty() && !concrete_target(step))
            || ledger
                .steps()
                .any(|old| material_eq(old.definition(), step))
            || additions[..index].iter().any(|old| material_eq(old, step))
        {
            return Err(AgentHorizonFailure::NoMaterialProgress);
        }
    }
    Ok(())
}

/// Closed Core/atomic handoff failure; no request text or provider payload crosses this boundary.
#[derive(Debug)]
pub enum AgentHorizonFailure {
    /// Existing source/run/step state cannot perform this operation.
    InvalidState,
    /// Run, Goal, revision, stage or actual request event does not match.
    InvalidPair,
    /// This operation requires the original-backed candidate path.
    MissingOriginal,
    /// No durable planning marker exists.
    NoPending,
    /// No structurally unplanned Must proof remains; this is not a freshness claim.
    NoUnplannedProof,
    /// IDs/text changes alone, repeated obligations or unrelated criteria are not progress.
    NoMaterialProgress,
    /// The bound source snapshot or evidence readiness changed.
    SnapshotChanged,
    /// An existing run ceiling was reached, never reset by horizon extension.
    BudgetExhausted,
    /// Cancellation won before the atomic write.
    Cancelled,
    /// Domain graph/history/checkpoint validation rejected the candidate.
    Ledger(a3_domain::TaskLedgerError),
    /// The existing dual CAS rejected the write.
    Storage(AgentActionStoreFailure),
    /// Exact owner/request journal lookup failed.
    Journal(RunJournalStoreFailure),
}
impl fmt::Display for AgentHorizonFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidState => "agent horizon state is invalid",
            Self::InvalidPair => "agent horizon pair is inconsistent",
            Self::MissingOriginal => "agent horizon requires an original",
            Self::NoPending => "agent horizon planning is not pending",
            Self::NoUnplannedProof => "agent horizon has no unplanned mandatory proof",
            Self::NoMaterialProgress => "agent horizon has no new material obligation",
            Self::SnapshotChanged => "agent horizon source readiness changed",
            Self::BudgetExhausted => "agent horizon run budget is exhausted",
            Self::Journal(_) => "agent horizon journal read failed",
            Self::Cancelled => "agent horizon was cancelled",
            Self::Ledger(_) => "agent horizon violates the Ledger",
            Self::Storage(_) => "agent horizon atomic write failed",
        })
    }
}
impl Error for AgentHorizonFailure {}
impl From<a3_domain::TaskLedgerError> for AgentHorizonFailure {
    fn from(e: a3_domain::TaskLedgerError) -> Self {
        Self::Ledger(e)
    }
}
impl From<AgentActionStoreFailure> for AgentHorizonFailure {
    fn from(e: AgentActionStoreFailure) -> Self {
        Self::Storage(e)
    }
}

impl From<RunJournalStoreFailure> for AgentHorizonFailure {
    fn from(e: RunJournalStoreFailure) -> Self {
        Self::Journal(e)
    }
}
